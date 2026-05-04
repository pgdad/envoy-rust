# Phase 05.3 — Upstream HTTP/2 cleartext (H2C prior-knowledge): `envoy-http2::Client` + router H2-arm + `http2-echo-server` helper + fixture 0010 + parent-05 close — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task (fresh subagent per task + two-stage review). Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **For every code-writing task:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — failing test first, verify fails, minimal implementation, verify passes, commit. No exceptions (doctrine D-3.1).
>
> **Source of truth:** `docs/envoy-rust/phases/05.3-http2-upstream/SPEC.md` (committed at parent-05 state-2 SHA `f1804a7`). This plan operationalizes SPEC §§D1–D8. Where this plan and the SPEC disagree, the SPEC wins — flag the drift, land an ADR per D-3.5, and continue. The parent phase-05 SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md` (committed at parent-05 state-1 SHA `cd1a70e`) and the predecessor 05.1 + 05.4 + 05.2 SPECs are preserved unedited as historical artifacts; for execution they are superseded by this 05.3 SPEC.

**Goal.** Land upstream HTTP/2 cleartext (H2C prior-knowledge) on the data plane and close parent phase 05 in five coordinated layers shipping in this single sub-phase: (1) new module `crates/envoy-http2/src/client.rs` ships `envoy_http2::Client` + `ClientStream` (per-connection plaintext H2 client; one TCP connection per upstream call; no pooling — pooling defers to the upstream-robustness family per parent-05 SPEC §4); the public surface mirrors `envoy_http1::Client` from 04.3 (`Client::connect(addr, host)` + `ClientStream::send_request(Request) -> Response`), with a fire-and-forget `tokio::spawn` driving the `h2::client::Connection` for the lifetime of the SendRequest handle per parent §6 signpost 6; the 05.2-landed `Http2Error` enum gains 4 additive client-side variants (`UpstreamConnect`, `H2ClientHandshake`, `H2SendRequest`, `H2RecvBody`); (2) cluster-side `Http2ProtocolOptions` schema in `envoy-config` via Envoy's `typed_extension_protocol_options.HttpProtocolOptions` mechanism — new `TypedExtensionProtocolOptions` / `HttpProtocolOptions` / `ExplicitHttpConfig` / `Http1ProtocolOptions` types on `crates/envoy-config/src/bootstrap.rs`; the `Http2ProtocolOptions` struct from 05.2 D2.b is reused unchanged on the cluster side; new `ConfigError::MutuallyExclusiveExplicitHttpConfig` (and `UnsupportedTypedConfigUrl` if the existing typed-config validator pattern doesn't already cover it) variants; (3) new `Cluster.upstream_protocol: UpstreamProtocol { Http1, Http2 }` field on `crates/envoy-cluster/src/cluster.rs` (defaulted to `Http1` for backwards-compat with all phase-04 clusters; set in `from_bootstrap` from the parsed cluster's `typed_extension_protocol_options`); (4) router H2-arm extending the 04.3-landed `BuildOutcome::Proxy` dispatch at `crates/envoy-http1/src/hcm.rs:209-303` — wraps the `Client::connect`/`send_request` pair in a `match cluster.upstream_protocol()` selecting `envoy_http1::Client` (existing 04.3 path) or `envoy_http2::Client` (NEW); `crate::router::write_proxied_response` is reused unchanged because the response wire-format on the downstream is HCM-on-downstream's concern; symmetric dispatch lands at `crates/envoy-http2/src/hcm.rs`'s `BuildOutcome::Proxy` arm at lines 117-134 replacing 05.2's 502 stub; (5) new helper crate `tests/helpers/http2-echo-server/` (sibling of 04.3's `http1-echo-server`; consumes `envoy_http2` NOT `h2` directly per parent §6 signpost 7) — minimal H2C echo with deterministic alphabetically-sorted-headers body; new harness `Http2EchoBackend` + `{{HTTP2_BACKEND_PORT}}` template-marker substitution in `tests/differential/src/lib.rs::run_fixture`; fixture `tests/fixtures/0010-http2-router-upstream/` (5 files; HCM `codec_type: HTTP2` listener + cluster `backend` of `type: STRICT_DNS` carrying `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}`); Docker-gated test wrapper + in-process integration backstop at `crates/envoy-bin/tests/http2_router_upstream.rs`.

**Architecture.** The `Client` skeleton mirrors `envoy_http1::Client`'s public shape (`Client::connect(addr, host) -> Result<ClientStream, Http2Error>` + `ClientStream::send_request(Request) -> Result<Response, Http2Error>`) so the router H2-arm dispatch site at `crates/envoy-http1/src/hcm.rs` can match polymorphically over `cluster.upstream_protocol()` with name + signature parity. The codec-edge translation in `client.rs::send_request` is the inverse of 05.2's listener-side `request.rs`/`response.rs` direction: an envoy `Request` value type → `http::Request<()>` (synthesizing `:method`/`:path`/`:authority`/`:scheme: http` per parent §3 cross-sub-phase architectural rule 3 + parent §6 signpost 12) → `h2::client::SendRequest::send_request` → `h2::SendStream::send_data` for the body → `h2::client::ResponseFuture` → drained `h2::RecvStream` body bytes → envoy `Response`. The `h2::client::Connection` returned by `h2::client::handshake` is driven on a fire-and-forget `tokio::spawn` per parent §6 signpost 6 — terminates when SendRequest drops + the connection gracefully closes. H2-forbidden hop-by-hop headers (`connection`, `transfer-encoding`, `upgrade`, `keep-alive`, `proxy-connection` per RFC 7540 §8.1.2.2) are stripped defensively at the codec edge in `client.rs::send_request` (symmetric to 05.2 D3's response-side strip in `response.rs`), per parent §3 cross-sub-phase architectural rule 4. Cluster-side `Http2ProtocolOptions` reuses the 05.2 D2.b struct + range checks unchanged — same `ConfigError::Http2ProtocolOptionsOutOfRange` rejection variant, same RFC 7540 ranges; the validator runs identically on listener-side and cluster-side use-sites because the rejection variant carries no use-site discriminator. The `UpstreamProtocol` enum on `envoy-cluster` is a typed enum (`#[derive(Default)]` for `Http1`) following the established `LbPolicy` shape; set at cluster-build time in `from_bootstrap` from `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config` per parent §6 signpost 5. Router dispatch lives at the existing 04.3 site (`crates/envoy-http1/src/hcm.rs`'s `BuildOutcome::Proxy` arm), wrapping the `Client::connect` + `send_request` pair in a `match cluster.upstream_protocol()`. The same shape lands symmetrically at `crates/envoy-http2/src/hcm.rs` to replace 05.2's 502 stub. `crate::router::write_proxied_response` is reused unchanged — the H1-vs-H2 distinction terminates at the codec edge inside `Client::send_request`. Fixture 0010 mirrors fixture 0008's shape exactly (route-to-cluster) plus 05.2's `codec_type: HTTP2` (downstream listener) plus 05.3's cluster-side `typed_extension_protocol_options` block (selecting H2 upstream).

**Tech stack.** Rust edition 2024 on pinned stable (`rust-toolchain.toml` D-3.9). **Zero new top-level Cargo deps in 05.3** (per SPEC §6 inherited signpost 6 / parent §6 signpost 14): the new `tests/helpers/http2-echo-server/` workspace member consumes existing `envoy-http2` + `tokio` + `anyhow` + `bytes` + `thiserror` + `tracing` + `tracing-subscriber` foundations only; `crates/envoy-http2/src/client.rs` adds an internal module to an existing crate (consumes the existing `h2 = "0.4"` + `http = "1"` direct deps from 05.2's `crates/envoy-http2/Cargo.toml`); D2's schema additions are pure type additions (new structs/enums in `crates/envoy-config/src/bootstrap.rs`, no new deps). Cargo.lock sync at scaffold time (Task 8 landing the new `http2-echo-server` workspace member) is expected to be a no-op or near-no-op (just feature-resolution differences if `http2-echo-server`'s feature set differs from existing helpers); the planner cross-checks at Task 12 state-4. New typed surfaces: `envoy_http2::Client` + `envoy_http2::ClientStream`; `envoy_http2::Http2Error::{UpstreamConnect, H2ClientHandshake, H2SendRequest, H2RecvBody}` (4 additive variants); `envoy_config::TypedExtensionProtocolOptions` + `HttpProtocolOptions` + `ExplicitHttpConfig` + `Http1ProtocolOptions`; `envoy_config::Cluster.typed_extension_protocol_options: Option<TypedExtensionProtocolOptions>`; `envoy_config::ConfigError::MutuallyExclusiveExplicitHttpConfig { cluster }` (and `UnsupportedTypedConfigUrl { got, expected }` only if not already present); `envoy_cluster::UpstreamProtocol { Http1, Http2 }`; `envoy_cluster::Cluster.upstream_protocol: UpstreamProtocol` + `Cluster::upstream_protocol()` accessor + `ClusterHandle::upstream_protocol()` delegate accessor. New harness surfaces: `differential::backend::Http2EchoBackend` (sibling of `Http1EchoBackend`); `locate_http2_echo_server` helper; `{{HTTP2_BACKEND_PORT}}` template-marker substitution in `run_fixture`'s per-side substitution maps. New behavioral surface: HCM listeners (H1 or H2) dispatching to clusters with `cluster.upstream_protocol == Http2` route via `envoy_http2::Client` to the upstream over plaintext H2C. **No edits to:** `BEHAVIOR_CONTRACT.md` (per SPEC §2 — the existing 04.3-landed `x-envoy-upstream-service-time` row engages on H2 router responses uneventfully); `rust-toolchain.toml`, `ENVOY_TARGET.md` (frozen per D-3.7 / D-3.9); `crates/envoy-{tls,tcp,listener}/`, `crates/envoy-http1/src/{client,router,codec,response,error,headers}.rs` (consumed via existing public APIs unchanged — only `crates/envoy-http1/src/hcm.rs` is edited at Task 6); `tests/helpers/{tcp,tls,http1}-echo-server/` (finalized in earlier phases); `tests/fixtures/{0001..0009}/` (must remain green at the 05.3 state-4 gate); `tests/conformance/h2spec/` (the runner + ≥95% gate + `known-failures.txt` landed at 05.2 D7 are unedited; the state-4 verification re-runs h2spec to confirm no regression); `tests/differential/Cargo.toml` (the `h2 = "0.4"` direct dep was added at 05.2 D5.b; reused unchanged); `crates/envoy-config/fuzz/{Cargo.toml,fuzz_targets/}` (only the corpus directory grows by 1 seed); `.github/workflows/ci.yml` (no CI changes anticipated — the h2spec install step landed at 05.2 D7 covers 05.3's needs). `deny.toml` likely no-op (no new top-level deps; cross-checked at Task 12). `Cargo.lock` lands a near-no-op diff at Task 8 (`http2-echo-server` workspace-member registration only).

**~12 tasks, ~2002 LoC total** (per SPEC §3 deliverable estimates: D1 ~535 + D2 ~335 + D3 ~110 + D4 ~180 + D5 ~330 + D6 ~220 + D7 ~292 + D8 ~0 = ~2002 LoC). The `BOOTSTRAP_PROMPT.md` §6.1 task-count gate (~25) holds with significant headroom (12 ≪ 25). The §6.1 LoC gate (~1500) is **crossed** at the SPEC-write-time estimate (~2002 ≈ 134% of guardrail). **Disposition: do not split** — per SPEC §6 local signpost 26 (LoC-budget reality check) + parent-05 SPEC §5's "no nest-split" rule (05.3 is already a sub-phase produced by parent-05's split per ADR-0022; mirrors 05.2's same-shape disposition where ~2055 LoC was accepted). The drift (~54% over parent-05 brainstorm's projection of ~1300 LoC) is concentrated in (a) D1's H2 client core (~535 LoC: `client.rs` impl ~250 + 8 unit tests with in-process h2-server scaffolding ~250 + `error.rs` extension ~30 + `lib.rs` re-export ~5) — this is the inverse-direction H2 codec edge's first appearance in the project, mirroring 05.2 D3's listener-side test density; and (b) D5 + D7 helper-and-fixture scaffolding (~622 LoC together) — the helper consumes `envoy_http2` over direct `h2`, the fixture lands the in-process backstop, and the differential harness extension carries `Http2EchoBackend` + the template-marker substitution. Both are doctrine-mandated test surfaces, not creep. **The systematic-debugging confirmation is recorded inline here** (per SPEC signpost 26's invocation requirement) and does NOT require a separate session: the LoC drift is genuine scope (H2 client codec edge with 8 tests + helper crate + new fixture + in-process backstop), not feature creep, and the test surface is non-negotiable per D-3.6 ("every phase is a green build"). Recorded in PROGRESS Task 1 narrative.

**Lands NO new ADRs at state-2** (per SPEC §7). The DECISIONS.md ledger head before 05.3 Task 1 is **ADR-0027** (per STATE.md "Last commit"; landing-time order ADR-0023 → 0024 → 0026 → 0025 → 0027 per the append-only ledger discipline). The cluster-side `Http2ProtocolOptions` schema reuses the listener-side struct + range checks from 05.2 D2.b; the `Cluster.upstream_protocol` field follows the established `LbPolicy` shape; the router H2-arm dispatches polymorphically over the existing `Client::connect`/`send_request` shapes; the helper crate follows the established `tcp-echo-server` / `tls-echo-server` / `http1-echo-server` posture verbatim; fixture 0010 follows the established 04.x + 05.x fixture shape — none of these warrant ADR-shaped permanent records. **If an unforeseen design ambiguity surfaces during execution per D-3.5**, the planner appends ADR-0028 (next-sequential available number) at the time it lands; record inline in the relevant Task's PROGRESS narrative.

**Carryforwards from 05.2 REVIEW** (per SPEC §1 + STATE.md "Phase-05.2 rollovers"). Per the SPEC's authoritative scope: **none of these are closed in 05.3 inside the 05.3 surface itself.** The SPEC's design contract names additive variants on `Http2Error` (4 new) and explicitly says "the 05.2 codec-side variants ... stay unchanged" (SPEC §3 D1) — meaning **I2** (`Http2Error` write-path variant rename) and **I3** (`MalformedH2HeaderBlock` overload split) are **NOT** addressed in 05.3 per the SPEC's variant-discipline. **I1** (CI tarball SHA-256 verification) is unedited — `.github/workflows/ci.yml` is in the SPEC's "Not touched in 05.3" list. **M2** (per-stream timeout budget) is named by STATE.md as "the natural fit at the upstream-H2 spawn site landing in 05.3" but the SPEC does not put per-stream timeouts in scope; carries forward awareness-only. **M6** (h2spec gate diagnostic surfaces skipped count) — the conformance runner is unedited per SPEC. **M8** (502 stub body literal mentions 05.3) **closes structurally** at Task 7 because the stub is replaced. **M10** (`Driver::Http2` lacks `extra_headers` field) — opportunistically addressable at Task 9 if fixture 0010 needs it; the planner cross-checks at Task 9 / Task 10. **M11** (RFC-soft `MissingAuthority` recovery) — defers; the SPEC §3 D4 dispatch path does not edit the per-stream task error handling. **M12** (garbage-preamble test permissive close-shape) — defers; the test in question is unedited in 05.3. The full carryforward inventory is recorded in PROGRESS Task 1's narrative + at the state-6 close-out STATE.md "Phase-05.3 rollovers" subsection per SPEC §6 signpost 23.

**Standing inventory carryforwards (no change in 05.3):**

- **Phase-04.1 REVIEW M-architectural-claim** (`drive_http1` per-function unit test): 05.3 introduces no new H1 surfaces; M-claim continues unchanged.
- **Phase-04.1 REVIEW M5/M9** (Cargo.lock cadence ratification ADR): 05.3 introduces no new top-level deps so does not force the ratification call. M5/M9 carry forward.
- **Phase-02.2 REVIEW M1** (`*EchoBackend::Drop` polling loop blocks on `std::thread::sleep`): the new `Http2EchoBackend` inherits the posture verbatim. M1 continues to track forward to whichever phase first parallelizes `run_fixture`.
- **Phase-04.1 REVIEW M7** (`TlsAcceptingHandler.inner: Arc<TcpProxy>` concrete-typed; HCM-in-TLS doesn't typecheck): re-deferred per parent-05 SPEC §4 to whichever phase ships ALPN-driven dispatch. 05.3 does not generalize.
- **Phase-04.1 REVIEW M1/M2/M4** (`diff_headers` duplicate-header value comparison; body-drain idle silent Ok; `strip_port` IPv6-Host incorrect rfind): 05.3 fixture 0010 does not exercise duplicate response headers, does not stall on body drain, and uses a DNS-name `Host:` value (`envoy-rust.test`) so does not exercise the IPv6-Host code path. Continue to track forward.

**No HTTP/3 / QUIC work in 05.3.** Same posture as 05.2 — separate family per `BOOTSTRAP_PROMPT.md` §9.

**Parent-phase-05 close-out at 05.3 state-6.** Per SPEC §1 + §8 + §9: the 05.3 state-6 phase-done commit ALSO flips parent ROADMAP row `05` `in-progress` → `done` per the ROADMAP-schema invariant ("the parent flips to `done` only after all sub-phases are `done`"; 05.1 / 05.2 / 05.4 are already `done`). Mirrors phase-04's `e626862`-shape close-out (the 04.3 state-6 commit closed parent-04) and phase-03's `ca81226`-shape close-out (the 03.2 state-6 commit closed parent-03). The 05.3 state-6 commit's title carries the `[parent 05 done]` tag. STATE.md advances active phase from `05.3-http2-upstream` lifecycle state 6 to phase 06 lifecycle state 1 (phase-06 directory does not exist; `superpowers:brainstorming` scoped to phase 06 — *"Access log (file sink, Envoy default format) + stats + Prometheus admin endpoint"* per `BOOTSTRAP_PROMPT.md` §8 row 06). Parent SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md` is **not edited** at the close-out commit per D-3.4 / D-3.5 — preserved unedited as the historical artifact (mirrors parent-04 SPEC's posture after the 04.3 close-out). State-6 close-out is a separate session per `BOOTSTRAP_PROMPT.md` §5.1 ("one state per session") — not part of this PLAN's tasks.

---

## File structure (created / modified / not touched)

**Created:**

- `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md` (created at Task 1; appended once per task during execution).
- `crates/envoy-http2/src/client.rs` (Task 2).
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml` (Task 4).
- `tests/helpers/http2-echo-server/Cargo.toml` (Task 8).
- `tests/helpers/http2-echo-server/src/main.rs` (with `#![forbid(unsafe_code)]` per D-3.8) (Task 8).
- `tests/fixtures/0010-http2-router-upstream/envoy.yaml` (Task 10).
- `tests/fixtures/0010-http2-router-upstream/envoy-rust.yaml` (Task 10).
- `tests/fixtures/0010-http2-router-upstream/inputs/payload.bin` (Task 10; empty file, 0 bytes).
- `tests/fixtures/0010-http2-router-upstream/expectations.yaml` (Task 10).
- `tests/fixtures/0010-http2-router-upstream/README.md` (Task 10).
- `tests/differential/tests/http2_router_upstream.rs` (Task 10; 7-line wrapper).
- `crates/envoy-bin/tests/http2_router_upstream.rs` (Task 11; in-process integration backstop).

**Modified:**

- `Cargo.toml` (root) — `[workspace] members` gains `tests/helpers/http2-echo-server` at Task 8.
- `Cargo.lock` — synced inline at Task 8 with the new workspace member registration (near-no-op; no new top-level deps).
- `crates/envoy-http2/src/error.rs` — at Task 1: append 4 new `Http2Error` variants (`UpstreamConnect`, `H2ClientHandshake`, `H2SendRequest`, `H2RecvBody`). The existing 6 variants from 05.2 D3 (`H2Handshake`, `H2StreamAccept`, `H2BodyRead`, `MissingAuthority`, `MalformedH2HeaderBlock`, `BadStatusCode`) stay unchanged per SPEC §3 D1.
- `crates/envoy-http2/src/lib.rs` — at Task 2: append `pub mod client;` and `pub use client::{Client, ClientStream};` re-exports.
- `crates/envoy-http2/src/codec.rs` — at Task 8: append a small `pub fn server_handshake` thin wrapper enabling `http2-echo-server` to consume `envoy_http2` instead of `h2` directly per architectural rule 1.
- `crates/envoy-config/src/bootstrap.rs` — at Task 3: extend `Cluster` struct (line 48) with `typed_extension_protocol_options: Option<TypedExtensionProtocolOptions>` field; add `TypedExtensionProtocolOptions` / `HttpProtocolOptions` / `ExplicitHttpConfig` / `Http1ProtocolOptions` types after the `LoadAssignment` block (line 105 area); extend `validate` (line 927) with the cluster-side `typed_extension_protocol_options` walk: mutual-exclusion check + `@type` URL well-formedness check + range-check delegation (re-using the existing `validate_http2_protocol_options_ranges` helper at lines 1180-1215 by extracting it from `validate_hcm`'s body to a free function shared between listener and cluster sites). Append ~8 new validator unit tests + 1 corpus-walk acceptance test.
- `crates/envoy-config/src/lib.rs` — at Task 3: append `ConfigError::MutuallyExclusiveExplicitHttpConfig { cluster: String }` variant (and `UnsupportedTypedConfigUrl { got: String, expected: &'static str }` if not already present); extend the `pub use bootstrap::{...}` re-export at lines 10–19 with `TypedExtensionProtocolOptions`, `HttpProtocolOptions`, `ExplicitHttpConfig`, `Http1ProtocolOptions` (alphabetic insertion).
- `crates/envoy-config/fuzz/.gitignore` — at Task 4: append `!corpus/parse_bootstrap/cluster_http2_protocol_options.yaml` to the existing allow-list block.
- `crates/envoy-cluster/src/cluster.rs` — at Task 5: add `UpstreamProtocol { Http1, Http2 }` enum (after the `ClusterError` block ending around line 141); add `Cluster.upstream_protocol: UpstreamProtocol` field (extend the struct at lines 11-16); add `Cluster::upstream_protocol()` accessor (after `Cluster::name()` at lines 24-26); add `ClusterHandle::upstream_protocol()` delegate accessor (after `ClusterHandle::name()` at lines 60-62); extend `from_bootstrap` (line 153) with the `upstream_protocol` projection from the parsed cluster's `typed_extension_protocol_options` (sync match running alongside the existing `cluster_type` match at lines 169-222). Append ~3 new unit tests.
- `crates/envoy-http1/src/hcm.rs` — at Task 6: extend the `BuildOutcome::Proxy` arm (lines 209-303 verified by the `grep -nB 5 'BuildOutcome::Proxy' crates/envoy-http1/src/hcm.rs` step at Task 6) wrapping the `Client::connect`/`send_request` pair in a `match cluster.upstream_protocol()` selecting H1 (existing 04.3 path) or H2 (NEW; uses `envoy_http2::Client`). `crate::router::write_proxied_response` reused unchanged. Append ~3 new unit tests covering the dispatch arms.
- `crates/envoy-http2/src/hcm.rs` — at Task 7: replace the 05.2 D3 502 stub at lines 117-134 with the symmetric H1-or-H2 dispatch on `cluster.upstream_protocol()`; rename 05.2's `h2_proxy_outcome_returns_502_in_05_2` test to `h2_proxy_outcome_dispatches_to_upstream` and flip the assertion from 502 to 200 per 05.2 SPEC §3 D3 test 6's projection. **Closes M8 structurally** (the stub body literal `b"upstream H2 not yet wired (sub-phase 05.3)\n"` at line 132 disappears). Append ~1 additional unit test for the H1-cluster-from-H2-listener case.
- `crates/envoy-http2/Cargo.toml` — at Task 2: add `envoy-cluster = { path = "../envoy-cluster" }` to `[dependencies]` (Task 7's symmetric-dispatch arm consumes `envoy_cluster::UpstreamProtocol`; verify whether the existing `[dev-dependencies]` entry covers Task 7's needs — if Task 7 needs production-side access, lift to `[dependencies]`; recommendation: `[dependencies]` since the dispatch lives in the production HCM path).
- `crates/envoy-bin/Cargo.toml` — at Task 11: add `envoy-http2 = { path = "../envoy-http2" }` to `[dev-dependencies]` only IF the in-process backstop at `crates/envoy-bin/tests/http2_router_upstream.rs` consumes `envoy_http2::Client` directly (vs. driving via `h2::client` like 05.2's backstop did). **Recommendation:** drive via `h2::client` per parent §6 signpost 18's posture (mirrors 05.2 D4's `crates/envoy-bin/tests/http2_direct_response.rs`); the existing `h2 = "0.4"` `[dev-dependencies]` entry from 05.2 covers this without a new dep entry.
- `tests/differential/src/backend.rs` — at Task 9: add `Http2EchoBackend` struct (sibling of `Http1EchoBackend` at lines 179-238) + `spawn` + `port` + `container_host` + `Drop` impl; add `locate_http2_echo_server` helper (sibling of `locate_http1_echo_server` at lines 244-269); ~3 harness unit tests appended.
- `tests/differential/src/lib.rs` — at Task 9: extend the `run_fixture` cascade (the existing `_http1_backend` block at lines 1003-1017 area) with a parallel `_http2_backend` block dispatched on `{{HTTP2_BACKEND_PORT}}` template-marker presence; extend the per-side substitution maps at lines 1024-1052 + 1053-1076 with `HTTP2_BACKEND_PORT` entries; extend the `BACKEND_HOST` gate at lines 1035-1045 + 1064-1069 to include `http2_backend_port_str.is_some()`. The `Driver::Http2` variant + `drive_http2` helper from 05.2 D5 are reused unchanged. **M10 disposition** (Driver::Http2 extra_headers field): if fixture 0010's expectations.yaml requires extra_headers, add the field at Task 9 with `#[serde(default)]` and thread it through `run_fixture`'s dispatch arm; if fixture 0010 does NOT need extra_headers (the SPEC's expectations.yaml example does not list any), defer M10 to whichever fixture first needs it. The HEADER_ALLOW_LIST constant is unedited.
- `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md` — created at Task 1; appended once per task during execution (per 05.4 / 05.2 PROGRESS.md precedent: Files-Modified, Verification, Verified-shapes-from-greps, Deviations-from-PLAN, Carryforward sections per task).
- `docs/envoy-rust/ROADMAP.md` — at state-6 only (NOT a state-3 task), flip row `05.3` `status: planned` → `done`. **AT THE SAME COMMIT:** flip parent row `05` `status: in-progress` → `done` per the ROADMAP-schema invariant. State-6 close-out is a separate session per `BOOTSTRAP_PROMPT.md` §5.1.
- `docs/envoy-rust/STATE.md` — at state-6 only, advance active phase to phase 06 lifecycle state 1 per SPEC §1 / §8. Notes section gains the "Phase-05.3 rollovers" subsection + the parent-05 close-out summary. The "Phase-05 ADR ledger (final)" entry confirms the actual landed ADRs at parent-05 close.

**Not touched in 05.3** (belong to other phases or are frozen):

- `docs/envoy-rust/phases/05-http2/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `cd1a70e`. **Per D-3.4 / D-3.5, parent SPECs are preserved unedited even at the close-out commit.**
- `docs/envoy-rust/phases/{05.1,05.2,05.4}-*/*.md` (closed sub-phases) — unedited in 05.3.
- `docs/envoy-rust/phases/05.3-http2-upstream/SPEC.md` (this sub-phase) — landed at parent-05 state-2 commit `f1804a7`; unedited in 05.3 execution per D-3.4.
- `docs/envoy-rust/phases/{04*, 03*, 02*, 01, 00}/*` — closed in earlier phases.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — no edits in 05.3 (per SPEC §2 — the existing 04.3-landed `x-envoy-upstream-service-time` row engages on H2 router responses uneventfully).
- `docs/envoy-rust/MISSION.md`, `docs/envoy-rust/SKILL_ROUTING.md` — frozen per their durability discipline.
- `docs/envoy-rust/DECISIONS.md` — no anticipated edits (per SPEC §7; no new ADRs projected at state-2). If an unforeseen ADR fires per D-3.5, lands at the relevant Task with provenance footer.
- `crates/envoy-{tls,tcp,listener}/`, `crates/envoy-http1/src/{client,router,codec,response,error,headers}.rs` — consumed via existing public APIs without amendment. **Notably:** `crate::router::write_proxied_response` (the response-write helper from 04.3 at `crates/envoy-http1/src/router.rs`) is reused unchanged in Task 6 — the H2 upstream's response is translated back into the protocol-agnostic `envoy_http1::codec::Response` value type by `envoy_http2::Client::send_request` per SPEC §3 D1, so `write_proxied_response` doesn't need to know about H2 at all.
- `crates/envoy-http1/src/{client,codec,error,headers,response,router}.rs` — only `crates/envoy-http1/src/hcm.rs` is edited at Task 6.
- `crates/envoy-http2/src/{request,response}.rs` — unedited. The listener-side translation (`http_to_envoy_request`, `build_http_response` / `send_envoy_response`) lands at 05.2 D3; 05.3's client-side translation lives in the new `client.rs` module, not in these.
- `crates/envoy-http2/src/hcm.rs` is touched at Task 7 ONLY at the `BuildOutcome::Proxy` arm (lines 117-134); the rest of the file (`HCM` struct + `ConnectionHandler` impl + the per-stream task scaffolding + `serve_h2_connection`) is unchanged.
- `crates/envoy-bin/src/main.rs` — **unchanged in 05.3.** The H1-vs-H2 dispatch at the HCM construction site landed in 05.2 D4 is reused unchanged; the router-arm dispatch lives inside `envoy_http1::HCM` (Task 6) and `envoy_http2::HCM` (Task 7) connection handlers, not at envoy-bin's wiring level. The `cluster_mgr` already constructed at startup (landed in 02.1) and threaded into the HCM via the existing wiring (landed in 04.1, extended in 04.3) is consumed by Task 6's + Task 7's dispatch arms unchanged.
- `crates/envoy-bin/src/{admin,argv,echo,tls_handler}.rs` — unchanged.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, ..., `tests/fixtures/0009-http2-direct-response/` — unedited; their fixtures must remain green at the 05.3 state-4 phase-done gate per SPEC §1 acceptance signal (b).
- `tests/helpers/{tcp,tls,http1}-echo-server/` — finalized in earlier phases.
- `tests/differential/Cargo.toml` — unchanged in 05.3. The `h2 = "0.4"` direct dep was added at 05.2 D5.b for `drive_http2`; reused unchanged.
- `tests/differential/src/{subject,tls,upstream}.rs` — unchanged. Only `lib.rs` and `backend.rs` are edited (Task 9).
- `tests/conformance/h2spec/` (the runner crate + h2spec.yaml + known-failures.txt) — unchanged in 05.3. The runner + the ≥95% gate landed at 05.2 D7; 05.3 does not edit them. The state-4 verification at Task 12 re-runs h2spec to confirm no regression from the upstream-direction work.
- `crates/envoy-config/fuzz/Cargo.toml`, `crates/envoy-config/fuzz/fuzz_targets/` — unchanged. Only the corpus directory grows (1 new seed file at Task 4).
- `.github/workflows/ci.yml` — unchanged in 05.3.
- `deny.toml` — likely no-op (no new top-level deps; cross-checked at Task 12).
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.
- Root `Cargo.toml`'s `[workspace] exclude` — unchanged (`crates/envoy-config/fuzz` continues to be excluded per ADR-0009).

---

## Task index

The 12 tasks group by deliverable:

- **Task 1** (D1 partial) — `envoy-http2::Http2Error` extension: 4 new client-side variants + PROGRESS.md preamble.
- **Task 2** (D1 main) — `envoy-http2::client.rs` module: `Client::connect` + `ClientStream::send_request` + 8 unit tests + `lib.rs` re-export.
- **Task 3** (D2 main) — `envoy-config` cluster-side `typed_extension_protocol_options` schema + supporting types + validator extensions + ConfigError variants + ~8 new validator unit tests.
- **Task 4** (D2 fuzz) — `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml` seed + `.gitignore` allow-list entry + 1 corpus-walk acceptance test.
- **Task 5** (D3) — `envoy-cluster::UpstreamProtocol` enum + `Cluster.upstream_protocol` field + accessor pair + `from_bootstrap` projection + 3 unit tests.
- **Task 6** (D4 H1-side) — Router H2-arm at `crates/envoy-http1/src/hcm.rs`'s `BuildOutcome::Proxy`: H1-or-H2 dispatch on `cluster.upstream_protocol()` + 3 unit tests.
- **Task 7** (D4 H2-side) — Symmetric dispatch at `crates/envoy-http2/src/hcm.rs`'s `BuildOutcome::Proxy`: replace 05.2's 502 stub with the H1-or-H2 dispatch; rename + flip the 05.2 D3 test 6; closes M8 structurally; ~1 additional unit test.
- **Task 8** (D5 + codec extension) — `tests/helpers/http2-echo-server/` workspace member + `crates/envoy-http2/src/codec.rs::server_handshake` thin wrapper + 5 unit tests + workspace-member registration + Cargo.lock sync.
- **Task 9** (D6) — Differential harness `Http2EchoBackend` + `locate_http2_echo_server` + `run_fixture` cascade extension on the `{{HTTP2_BACKEND_PORT}}` template marker + ~4 unit tests. M10 disposition: address opportunistically if fixture 0010 needs it.
- **Task 10** (D7 part 1) — Fixture `tests/fixtures/0010-http2-router-upstream/` (5 files) + Docker-gated wrapper `tests/differential/tests/http2_router_upstream.rs`.
- **Task 11** (D7 part 2) — In-process integration backstop `crates/envoy-bin/tests/http2_router_upstream.rs`.
- **Task 12** (D7 part 3 + state-4) — State-4 phase-done gate verification: all 10 Docker-gated fixtures + h2spec ≥95% pass + 5 stable-toolchain commands + fuzz short-budget run + cargo-deny (CI run URL + per-fixture matrix quoted in PROGRESS Task 12).

The plan executes tasks 1 → 12 in order. **Tasks 1–2 (D1) sequence by dep:** `error.rs` first because `client.rs` references the 4 new variants. **Task 3 (D2) before Task 5 (D3):** D3's `from_bootstrap` projection consumes the new `typed_extension_protocol_options` types from D2. **Tasks 6 + 7 (D4) after Tasks 2 + 5:** the dispatch arms consume `envoy_http2::Client` (Task 2) and `envoy_cluster::UpstreamProtocol` (Task 5). **Task 8 (D5) after Task 2** (the helper consumes `envoy_http2`'s codec.rs server_handshake which depends on Task 2's lib.rs growth being compatible). **Task 9 (D6) after Task 8:** harness backend spawns the binary built at Task 8. **Task 10 (D7 part 1) after Tasks 3 + 5 + 6 + 7 + 9** (fixture YAML uses the schema; harness runs the fixture). **Task 11 (D7 part 2) after Tasks 6 + 7 + 8** (in-process backstop drives envoy-bin against an in-test-spawned http2-echo-server). **Task 12 last** — state-4 verification gate. Subagent-driven execution per the user's standing preference (auto-memory `feedback_execution_style`).

State-6 phase-done close-out is a separate session per `BOOTSTRAP_PROMPT.md` §5.1 — not in this PLAN's task list.

---

## Task 1 — `envoy-http2::Http2Error` extension (4 client-side variants) + PROGRESS.md preamble

**Files:**

- Modify: `crates/envoy-http2/src/error.rs` — append 4 new variants (`UpstreamConnect`, `H2ClientHandshake`, `H2SendRequest`, `H2RecvBody`) after the existing 6 variants from 05.2 D3.
- Create: `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md` — preamble + Task 1 narration.

**Estimated LoC:** ~50 (4 variant blocks ~30 + 4 unit tests ~15 + PROGRESS.md preamble ~15).

**Signposts settled:**

- SPEC §3 D1 (additive extension): "the 05.2 codec-side variants stay unchanged." Task 1 is purely additive.
- SPEC §1 carryforward inventory: I2 + I3 (`Http2Error` rename / split) NOT addressed at Task 1 per the SPEC's variant-discipline. Recorded in PROGRESS Task 1 narrative.
- SPEC §6 local signpost 26 (LoC-budget reality check): record posture (a) "accept the estimate" inline in PROGRESS Task 1; the systematic-debugging confirmation is the inline narrative at PLAN preamble (this PLAN's "~12 tasks, ~2002 LoC" paragraph above).
- SPEC §7 (no ADRs at state-2): record explicitly in PROGRESS Task 1.

- [ ] **Step 1.1: Verify the existing `Http2Error` enum shape.**

Run: `grep -nA 2 'pub enum Http2Error' crates/envoy-http2/src/error.rs`
Expected: confirms the existing 6 variants (`H2Handshake`, `H2StreamAccept`, `H2BodyRead`, `MissingAuthority`, `MalformedH2HeaderBlock`, `BadStatusCode`) at lines 9-58 of HEAD `f33dac9`. Record the exact closing-brace line in PROGRESS Task 1.

- [ ] **Step 1.2: Write a failing test for the 4 new variants' Display output.**

Append to `crates/envoy-http2/src/error.rs::tests` (the existing `#[cfg(test)] mod tests` block; current last test at lines 81-94 area for `h2_handshake_displays_with_source`):

```rust
    #[test]
    fn upstream_connect_displays_with_addr_and_source() {
        let addr: std::net::SocketAddr = "127.0.0.1:9001".parse().unwrap();
        let src = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let e = Http2Error::UpstreamConnect { addr, source: src };
        let s = format!("{e}");
        assert!(s.contains("127.0.0.1:9001"), "expected addr in display: {s}");
        assert!(s.contains("refused"), "expected source in display: {s}");
    }

    #[test]
    fn h2_client_handshake_displays_with_source() {
        let src: h2::Error = h2::Reason::PROTOCOL_ERROR.into();
        let e = Http2Error::H2ClientHandshake { source: src };
        let s = format!("{e}");
        assert!(
            s.starts_with("client-side H2 handshake failed:"),
            "expected client-handshake prefix: {s}"
        );
    }

    #[test]
    fn h2_send_request_displays_with_source() {
        let src: h2::Error = h2::Reason::REFUSED_STREAM.into();
        let e = Http2Error::H2SendRequest { source: src };
        let s = format!("{e}");
        assert!(
            s.starts_with("client-side H2 send_request failed:"),
            "expected send_request prefix: {s}"
        );
    }

    #[test]
    fn h2_recv_body_displays_with_source() {
        let src: h2::Error = h2::Reason::INTERNAL_ERROR.into();
        let e = Http2Error::H2RecvBody { source: src };
        let s = format!("{e}");
        assert!(
            s.starts_with("client-side H2 response body read failed:"),
            "expected recv_body prefix: {s}"
        );
    }
```

- [ ] **Step 1.3: Run tests to verify they fail.**

Run: `cargo test -p envoy-http2 --lib error -- --nocapture`
Expected: 4 compile errors of the form `no variant or associated item named UpstreamConnect/H2ClientHandshake/H2SendRequest/H2RecvBody found for enum Http2Error`. (The pre-existing 3 tests remain passing.)

- [ ] **Step 1.4: Append the 4 new variants to `Http2Error`.**

Edit `crates/envoy-http2/src/error.rs`. Insert after the existing `BadStatusCode { status: u16 }` variant (the variant currently closing at line 57) and before the closing `}` of `pub enum Http2Error`:

```rust
    /// `tokio::net::TcpStream::connect` to the upstream endpoint failed.
    /// Sibling of `envoy_http1::Http1Error::UpstreamConnect`; raised at
    /// `Client::connect`'s outermost `?`. The `addr` field carries the
    /// resolved upstream `SocketAddr` (post-`pick_endpoint`); `source` is the
    /// underlying `std::io::Error` (typically `ConnectionRefused` /
    /// `TimedOut` / `HostUnreachable`).
    #[error("upstream H2 connect to {addr} failed: {source}")]
    UpstreamConnect {
        addr: std::net::SocketAddr,
        #[source]
        source: std::io::Error,
    },

    /// `h2::client::handshake` failed (the upstream did not complete the
    /// H2C preamble exchange — e.g., responded with HTTP/1.1 instead of an
    /// H2 SETTINGS frame, or closed the connection mid-handshake).
    /// Symmetric to the listener-side `H2Handshake` variant.
    #[error("client-side H2 handshake failed: {source}")]
    H2ClientHandshake {
        #[source]
        source: h2::Error,
    },

    /// `h2::client::SendRequest::send_request` or the subsequent
    /// `ResponseFuture` await failed. Covers send-stream initialization
    /// failures, peer GOAWAY mid-request, and response-future reset/cancel.
    #[error("client-side H2 send_request failed: {source}")]
    H2SendRequest {
        #[source]
        source: h2::Error,
    },

    /// Reading body bytes from the response-side `h2::RecvStream` failed
    /// (e.g., RST_STREAM mid-body, INTERNAL_ERROR on a stream after the
    /// response head was received). Symmetric to the listener-side
    /// `H2BodyRead` variant but on the inverse direction.
    #[error("client-side H2 response body read failed: {source}")]
    H2RecvBody {
        #[source]
        source: h2::Error,
    },
```

- [ ] **Step 1.5: Run the tests to verify they pass.**

Run: `cargo test -p envoy-http2 --lib error -- --nocapture`
Expected: 7 tests pass total (3 pre-existing + 4 new); zero failures. The pre-existing `missing_authority_displays_descriptively`, `bad_status_code_displays_value`, `h2_handshake_displays_with_source` tests must still pass — confirms the additive extension does not regress the 05.2-landed variants.

- [ ] **Step 1.6: Run the full envoy-http2 test suite.**

Run: `cargo test -p envoy-http2 --lib`
Expected: all envoy-http2 tests pass (the pre-existing 05.2 tests across `error.rs` / `request.rs` / `response.rs` / `codec.rs` / `hcm.rs` plus the 4 new error tests). Confirms no regression on the 05.2-landed surface.

Run: `cargo build --workspace --all-targets`
Expected: clean.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.

- [ ] **Step 1.7: Create `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md`.**

Create the file with the standard 04.x / 05.1 / 05.4 / 05.2 PROGRESS.md preamble shape, plus the Task 1 narration:

```markdown
# Phase 05.3 PROGRESS log

SPEC at `docs/envoy-rust/phases/05.3-http2-upstream/SPEC.md` (committed at parent-05 state-2 SHA `f1804a7`); PLAN at `docs/envoy-rust/phases/05.3-http2-upstream/PLAN.md` (this PLAN's commit). Tasks 1–12 land in numeric order; each task carries Commit / Deliverables / ADR landed (if any) / Files modified / LoC / Verification / Verified-shapes-from-greps / Deviations-from-PLAN / Carryforward sections per 05.4 / 05.2 PROGRESS.md precedent.

**LoC-budget reality check posture (per SPEC §6 local signpost 26):** posture (a) — accept the estimate. The 05.3 SPEC's §3 D1–D8 deliverable estimates total approximately ~2002 LoC, ~134% of the BOOTSTRAP_PROMPT §6.1 LoC guardrail (~1500). The drift is concentrated in D1's H2 client core (mirrors 05.2 D3's listener-side test density) and D5+D7 helper-and-fixture scaffolding (helper crate + fixture + in-process backstop). Both are doctrine-mandated test surfaces, not creep. The systematic-debugging confirmation is recorded in PLAN's preamble paragraph "~12 tasks, ~2002 LoC" — the 12-task count is well under the ~25 task-count guardrail; LoC drift is genuine scope. Per parent-05 SPEC §5's "no nest-split" rule, 05.3 (already a sub-phase produced by parent-05's split per ADR-0022) is not re-split.

**ADR ledger head before 05.3 Task 1:** ADR-0027 (per STATE.md "Last commit"; landing-time order ADR-0023 → 0024 → 0026 → 0025 → 0027). **No ADRs projected for 05.3 state-2** per SPEC §7. If an unforeseen design ambiguity surfaces during execution per D-3.5, ADR-0028 is the next-sequential available number.

**Carryforwards from 05.2 REVIEW** (per SPEC §1 + STATE.md "Phase-05.2 rollovers"): per the SPEC's authoritative scope, **none of these are closed in 05.3 inside the 05.3 surface itself.** The SPEC §3 D1 explicitly says "the 05.2 codec-side variants ... stay unchanged" — meaning I2 (Http2Error write-path variant rename) and I3 (MalformedH2HeaderBlock overload split) are NOT addressed at Task 1. I1 (CI tarball SHA-256) — `.github/workflows/ci.yml` unedited per SPEC. M2 (per-stream timeout) — STATE.md names this as a recommended fit at the upstream-H2 spawn site, but the SPEC §3 D4 dispatch path does not edit per-stream task timeouts; carries forward awareness-only. M6 (h2spec gate diagnostic) — `tests/conformance/h2spec/` unedited per SPEC. M8 (502 stub body literal) closes structurally at Task 7 (the stub is replaced with the symmetric H1-or-H2 dispatch). M10 (Driver::Http2 extra_headers field) — opportunistic at Task 9 if fixture 0010 needs it. M11 (RFC-soft MissingAuthority recovery) — defers; the per-stream task error handling is unedited. M12 (garbage-preamble test permissive) — defers; the test in question is unedited.

**Standing inventory carryforwards (no change in 05.3):** Phase-04.1 REVIEW M-architectural-claim (`drive_http1` per-function unit test); Phase-04.1 REVIEW M5/M9 (Cargo.lock cadence ratification ADR — no new top-level deps in 05.3); Phase-02.2 REVIEW M1 (`*EchoBackend::Drop` polling loop blocks on `std::thread::sleep`, inherited verbatim by `Http2EchoBackend` at Task 9); Phase-04.1 REVIEW M7 (`TlsAcceptingHandler.inner` concrete-typed); Phase-04.1 REVIEW M1/M2/M4 (header-diff value-comparison; body-drain idle silent Ok; strip_port IPv6-Host).

---

## Task 1 — `envoy-http2::Http2Error` extension (4 client-side variants)

**Commit:** <SHA>

**Deliverables:** SPEC §3 D1 partial — the 4 additive client-side variants on `Http2Error`. The 6 codec-side variants from 05.2 D3 stay unchanged per SPEC §3 D1.

**ADR landed:** none (per SPEC §7).

**Files modified:**
- `crates/envoy-http2/src/error.rs` (+4 variants ~30 LoC; +4 unit tests ~30 LoC).
- `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md` (created with this task's narrative + the preamble sections above).

**LoC:** ~60 (~30 impl + ~30 tests).

**Verification:**
- `cargo test -p envoy-http2 --lib error` — 7 passed (3 pre-existing + 4 new).
- `cargo test -p envoy-http2 --lib` — full envoy-http2 unit test count (record at task time; ~50+ tests at HEAD `f33dac9`).
- `cargo build --workspace --all-targets` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean.

**Verified shapes from greps run at task time:**
- `grep -nA 2 'pub enum Http2Error' crates/envoy-http2/src/error.rs` — confirms the 6 pre-existing variants at lines 9-58.
- `grep -n '#\[error(' crates/envoy-http2/src/error.rs` — confirms 10 `#[error]` lines after Task 1 (6 pre-existing + 4 new).

**Deviations from PLAN:** none (or record any).

**Carryforward:** none (Task 1 is closed in-task; the 4 client-side variants are consumed at Task 2).
```

Append to PROGRESS.md as the Task 1 narrative is written. Future tasks append their own `## Task N` sections to the same file.

- [ ] **Step 1.8: Run `cargo fmt`.**

Run: `cargo fmt --all -- --check`
Expected: clean. If `cargo fmt --all` modifies the new variants (rustfmt's unwrap-or-pack heuristic may reflow the variant blocks), accept the modification and re-add.

- [ ] **Step 1.9: Commit Task 1.**

```bash
git add crates/envoy-http2/src/error.rs \
        docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 05.3: envoy-http2::Http2Error 4 client-side variants (task 1)

SPEC §3 D1 partial: the additive 4-variant extension landing
UpstreamConnect / H2ClientHandshake / H2SendRequest / H2RecvBody on the
05.2-landed Http2Error enum. The 6 codec-side variants (H2Handshake /
H2StreamAccept / H2BodyRead / MissingAuthority / MalformedH2HeaderBlock /
BadStatusCode) stay unchanged per SPEC §3 D1.

The new variants source-preserve the underlying error
(`std::io::Error` for UpstreamConnect; `h2::Error` for the three H2-side
variants) via `#[source]`. Display strings carry the matching
"client-side ..." prefix to disambiguate from the listener-side variants
on stack traces and log lines.

4 new Display-shape unit tests (one per variant). Pre-existing 6
codec-side tests + the listener-side test surface in `request.rs` /
`response.rs` / `codec.rs` / `hcm.rs` continue passing per `cargo test
-p envoy-http2`.

PROGRESS.md created with the standard 05.4 / 05.2 preamble sections +
Task 1 narrative. ADR ledger unchanged at ADR-0027 (no ADR landed).

Carryforward 05.2 REVIEW I2 (Http2Error write-path variant rename) and
I3 (MalformedH2HeaderBlock overload split) NOT addressed per SPEC §3 D1
("the 05.2 codec-side variants stay unchanged"); both continue forward
to whichever phase first amends the codec-side variants.
EOF
)"
```

---


## Task 2 — `envoy-http2::client.rs` module (`Client::connect` + `ClientStream::send_request`) + 8 unit tests

**Files:**

- Create: `crates/envoy-http2/src/client.rs` (the new module).
- Modify: `crates/envoy-http2/src/lib.rs` — append `pub mod client;` and `pub use client::{Client, ClientStream};` re-exports.
- Modify: `crates/envoy-http2/Cargo.toml` — add `envoy-cluster = { path = "../envoy-cluster" }` to `[dependencies]` (to support Task 7's symmetric-dispatch arm; verify whether this is already present from 05.2's `[dev-dependencies]` and lift if so). **Recommendation per signpost 18 below:** add at Task 2 to keep `client.rs` and the cluster-side type's eventual consumer (Task 7) in the same dep declaration; if `cargo build` complains about an unused dep, defer to Task 7. Cross-check at Task 2 Step 2.10.

**Estimated LoC:** ~535 (impl ~250: `Client::connect` ~30 + `ClientStream` struct ~10 + `send_request` body ~150 + the H2-forbidden hop-by-hop strip + the pseudo-header synthesis + the response drain ~60; 8 unit tests ~250 with in-process h2-server scaffolding ~30 LoC of helper code shared across tests; `lib.rs` re-export ~5).

**Signposts settled:**

- SPEC §3 D1: `Client::connect(addr, host) -> Result<ClientStream, Http2Error>`; `ClientStream::send_request(envoy_http1::codec::Request) -> Result<envoy_http1::codec::Response, Http2Error>`. Mirrors `envoy_http1::Client` from 04.3.
- SPEC §6 inherited signpost 2 (Background `h2::client::Connection` driving): `tokio::spawn` direct, fire-and-forget; the connection task drops cleanly with the SendRequest.
- SPEC §6 inherited signpost 5 (`:method`/`:path`/`:authority`/`:scheme` translation): synthesize from `Request.method` / `Request.path` / captured `host` (or explicit `Host:` header) / `:scheme: http`.
- SPEC §3 architectural rule 4 + parent §6 signpost 11: H2-forbidden hop-by-hop headers stripped; header names lowercased; `Host:` skipped.
- SPEC §6 local signpost 14: 04.3 `Client::connect`/`send_request` shape mirrored verbatim (verifiable via `grep -nE 'pub (async )?fn (connect|send_request)' crates/envoy-http1/src/client.rs`).
- SPEC §6 local signpost 19 (defense on H2 client connection-task termination): the `tokio::spawn(connection)` is fire-and-forget; on `ClientStream` drop, `SendRequest` drop signals the connection task to drain and close gracefully; post-response errors are logged via `tracing::warn!` and do NOT propagate.

- [ ] **Step 2.1: Cross-check the 04.3 `envoy_http1::Client` signature.**

Run: `grep -nE 'pub (async )?fn (connect|send_request)|pub struct (Client|ClientStream)' crates/envoy-http1/src/client.rs`
Expected output (against HEAD `f33dac9`):
```
24:pub struct Client;
33:    pub async fn connect(
52:pub struct ClientStream {
69:    pub async fn send_request(&mut self, request: Request) -> Result<Response, Http1Error> {
```

Record the exact signatures in PROGRESS Task 2:
- `Client::connect(addr: std::net::SocketAddr, host: &str) -> Result<ClientStream, Http1Error>`
- `ClientStream::send_request(&mut self, request: Request) -> Result<Response, Http1Error>`

Task 2's `envoy_http2::Client` mirrors these names + signatures + arg order; only the error type changes (`Http2Error` instead of `Http1Error`) and the consumed types are the **same protocol-agnostic** `envoy_http1::codec::{Request,Response}` value types per SPEC §3 cross-sub-phase architectural rule 2.

- [ ] **Step 2.2: Cross-check the 05.2 listener-side `request.rs` translation pattern.**

Run: `grep -nA 5 'pub fn http_to_envoy_request' crates/envoy-http2/src/request.rs`
Run: `grep -nA 5 'pub fn build_http_response\|pub async fn send_envoy_response' crates/envoy-http2/src/response.rs`

Record the inverse-direction translation patterns. Task 2's `client.rs::send_request` does:
- envoy `Request` → `http::Request<()>` (synthesize `:method`, `:path`, `:authority`, `:scheme: http`; strip `Host:`; strip H2-forbidden hop-by-hop headers; lowercase header names) — INVERSE of 05.2's `request.rs::http_to_envoy_request`.
- `http::Response<h2::RecvStream>` → envoy `Response` (drain body, lowercase headers, translate status) — INVERSE of 05.2's `response.rs::build_http_response`/`send_envoy_response`.

- [ ] **Step 2.3: Add `envoy-cluster = { path = "../envoy-cluster" }` to `crates/envoy-http2/Cargo.toml`'s `[dependencies]`.**

Edit `crates/envoy-http2/Cargo.toml`. The current shape (per HEAD `f33dac9`) is:

```toml
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
envoy-cluster = { path = "../envoy-cluster" }
```

Move the `envoy-cluster = { path = "../envoy-cluster" }` line from `[dev-dependencies]` to `[dependencies]` (Task 7's symmetric-dispatch arm at `crates/envoy-http2/src/hcm.rs`'s `BuildOutcome::Proxy` consumes `envoy_cluster::UpstreamProtocol` in production code). Keep the `[dev-dependencies]` `tokio` entry unchanged.

Final `[dependencies]` shape:

```toml
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
envoy-cluster = { path = "../envoy-cluster" }

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "test-util"] }
```

Task 2 itself does NOT consume `envoy_cluster` symbols (`client.rs`'s test code uses only `envoy_http1`); the lift is Task-7-driven. If the lift causes `cargo build` to flag the dep as unused at Task 2 time (clippy `unused_crate_dependencies` is opt-in; default `cargo build` does NOT flag), defer the lift to Task 7 and record in PROGRESS Task 2.

- [ ] **Step 2.4: Write the 8 failing tests for `client.rs`.**

Create the test scaffolding first via `cargo build -p envoy-http2` of an empty `client.rs` body. Append `pub mod client;` to `crates/envoy-http2/src/lib.rs` (between line 23 `pub mod request;` and line 24 `pub mod response;` for alphabetic ordering — actual order: `pub mod client;` BEFORE `pub mod codec;` since `c` < `c` ties on first char then `l` < `o`; insertion is line 19-20 between `#![forbid(unsafe_code)]` block-level docs and the first `pub mod codec;` at line 20). Updated `lib.rs` shape:

```rust
#![forbid(unsafe_code)]

//! ... (existing 05.2 docs unchanged) ...

pub mod client;
pub mod codec;
mod error;
pub mod hcm;
pub mod request;
pub mod response;

pub use client::{Client, ClientStream};
pub use codec::build_h2_server;
pub use error::Http2Error;
pub use hcm::{HCM, HCMConfig};
pub use request::http_to_envoy_request;
pub use response::{build_http_response, send_envoy_response};
```

Create empty `crates/envoy-http2/src/client.rs` with placeholder structs so the file compiles:

```rust
//! Per-connection plaintext HTTP/2 cleartext (H2C) client. No pooling.
//! Sibling of `envoy_http1::Client` from 04.3; sole user of `h2::client::*`
//! per parent-05 SPEC §3 cross-sub-phase architectural rule 1.

use crate::error::Http2Error;
use bytes::Bytes;
use envoy_http1::codec::Request;
use envoy_http1::response::Response;
use std::net::SocketAddr;

/// Per-connection H2C client. Stateless; the per-stream state lives on
/// `ClientStream`. Mirrors `envoy_http1::Client`'s shape verbatim.
pub struct Client;

impl Client {
    /// TCP-connect to `addr`, run `h2::client::handshake`, drive the resulting
    /// `Connection` on a fire-and-forget `tokio::spawn`, and return a
    /// `ClientStream` wrapping the captured `SendRequest` handle + `host`.
    pub async fn connect(addr: SocketAddr, host: &str) -> Result<ClientStream, Http2Error> {
        let _ = (addr, host);
        unimplemented!("Task 2.5 lands the body");
    }
}

pub struct ClientStream {
    send_request: h2::client::SendRequest<Bytes>,
    host: String,
}

impl ClientStream {
    pub async fn send_request(&mut self, request: Request) -> Result<Response, Http2Error> {
        let _ = request;
        unimplemented!("Task 2.5 lands the body");
    }
}
```

Append the 8 unit tests at the bottom of `client.rs` (the `#[cfg(test)] mod tests { ... }` block). The tests share an in-process h2-server helper:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use envoy_http1::codec::HttpVersion;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Spawn an in-process h2 server on a 127.0.0.1 ephemeral port. The
    /// `responder` closure builds the response (status + headers + body) given
    /// the captured request shape (method/path/authority + headers). Returns
    /// the bound addr + a `JoinHandle` whose abort is the server's lifecycle.
    async fn spawn_h2_server<F>(
        responder: F,
    ) -> (
        std::net::SocketAddr,
        Arc<Mutex<Option<http::Request<Bytes>>>>,
        tokio::task::JoinHandle<()>,
    )
    where
        F: Fn(&http::Request<Bytes>) -> http::Response<Bytes> + Send + Sync + 'static,
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let captured: Arc<Mutex<Option<http::Request<Bytes>>>> = Arc::new(Mutex::new(None));
        let captured_clone = Arc::clone(&captured);
        let responder = Arc::new(responder);
        let handle = tokio::spawn(async move {
            let (tcp, _peer) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            let mut conn = match h2::server::handshake(tcp).await {
                Ok(c) => c,
                Err(_) => return,
            };
            while let Some(result) = conn.accept().await {
                let (req, mut send_response) = match result {
                    Ok(p) => p,
                    Err(_) => return,
                };
                let (parts, mut body) = req.into_parts();
                // Drain request body bytes (small body assumption — the tests
                // don't exercise multi-frame request bodies).
                let mut body_bytes = bytes::BytesMut::new();
                while let Some(chunk_result) = body.data().await {
                    let chunk = match chunk_result {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    body_bytes.extend_from_slice(&chunk);
                    let _ = body.flow_control().release_capacity(chunk.len());
                }
                let captured_req = http::Request::from_parts(parts, body_bytes.freeze());
                let resp = responder(&captured_req);
                {
                    let mut slot = captured_clone.lock().await;
                    *slot = Some(captured_req);
                }
                let (parts, body) = resp.into_parts();
                let response_head = http::Response::from_parts(parts, ());
                let mut send_stream = match send_response.send_response(response_head, false) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let _ = send_stream.send_data(body, true);
            }
        });
        (addr, captured, handle)
    }

    /// Spawn an in-process h2 server that emits the given response chunks
    /// across multiple DATA frames. Used by `send_request_drains_multi_frame_response_body`.
    async fn spawn_h2_server_chunks(
        chunks: Vec<Bytes>,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (tcp, _peer) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            let mut conn = match h2::server::handshake(tcp).await {
                Ok(c) => c,
                Err(_) => return,
            };
            while let Some(result) = conn.accept().await {
                let (_req, mut send_response) = match result {
                    Ok(p) => p,
                    Err(_) => return,
                };
                let resp = http::Response::builder().status(200).body(()).unwrap();
                let mut send_stream = match send_response.send_response(resp, false) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let n = chunks.len();
                for (i, chunk) in chunks.into_iter().enumerate() {
                    let end = i == n - 1;
                    let _ = send_stream.send_data(chunk, end);
                }
                return;
            }
        });
        (addr, handle)
    }

    fn mk_request(method: &str, path: &str, headers: Vec<(&str, &str)>, body: Bytes) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            version: HttpVersion::Http11,
            headers: headers
                .into_iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            bytes_consumed: 0,
            body: Some(body),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_succeeds_against_in_process_h2_listener() {
        let (addr, _captured, _handle) = spawn_h2_server(|_req| {
            http::Response::builder()
                .status(200)
                .body(Bytes::from_static(b"ok"))
                .unwrap()
        })
        .await;
        let client = Client::connect(addr, "test.example").await;
        assert!(client.is_ok(), "expected connect Ok, got {client:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_returns_upstream_connect_on_refused() {
        // Bind ephemeral, then drop the listener — the addr is unbound for the
        // duration of the test (deterministic ConnectionRefused on Linux/macOS).
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let result = Client::connect(addr, "test.example").await;
        match result {
            Err(Http2Error::UpstreamConnect { addr: a, source }) => {
                assert_eq!(a, addr);
                // ConnectionRefused on Linux/macOS; some platforms may surface
                // ConnectionReset or other ErrorKinds — accept any io::Error
                // and assert the variant alone.
                let _ = source;
            }
            other => panic!("expected UpstreamConnect, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_writes_get_with_synthesized_pseudoheaders() {
        let (addr, captured, _handle) = spawn_h2_server(|_req| {
            http::Response::builder()
                .status(200)
                .body(Bytes::from_static(b""))
                .unwrap()
        })
        .await;
        let mut client = Client::connect(addr, "test.example").await.unwrap();
        let req = mk_request("GET", "/", vec![], Bytes::new());
        let _resp = client.send_request(req).await.expect("send_request");
        let captured = captured.lock().await;
        let captured = captured.as_ref().expect("h2 server captured request");
        assert_eq!(captured.method().as_str(), "GET");
        assert_eq!(captured.uri().path(), "/");
        assert_eq!(
            captured.uri().authority().map(|a| a.as_str()),
            Some("test.example")
        );
        assert_eq!(captured.uri().scheme_str(), Some("http"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_explicit_host_header_wins_over_captured_host() {
        let (addr, captured, _handle) = spawn_h2_server(|_req| {
            http::Response::builder()
                .status(200)
                .body(Bytes::from_static(b""))
                .unwrap()
        })
        .await;
        let mut client = Client::connect(addr, "test.example").await.unwrap();
        let req = mk_request("GET", "/", vec![("Host", "real.example")], Bytes::new());
        let _resp = client.send_request(req).await.expect("send_request");
        let captured = captured.lock().await;
        let captured = captured.as_ref().expect("h2 server captured request");
        // Per SPEC §3 D1: explicit Host: wins over captured host.
        assert_eq!(
            captured.uri().authority().map(|a| a.as_str()),
            Some("real.example")
        );
        // Host: row should NOT be present in the captured headers (it became
        // :authority and was stripped from the headers vec).
        assert!(
            captured
                .headers()
                .iter()
                .all(|(n, _)| n.as_str() != "host"),
            "host: row should not appear alongside :authority"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_reads_response_status_headers_body() {
        let (addr, _captured, _handle) = spawn_h2_server(|_req| {
            http::Response::builder()
                .status(200)
                .header("content-type", "text/plain")
                .body(Bytes::from_static(b"hello\n"))
                .unwrap()
        })
        .await;
        let mut client = Client::connect(addr, "test.example").await.unwrap();
        let req = mk_request("GET", "/", vec![], Bytes::new());
        let resp = client.send_request(req).await.expect("send_request");
        assert_eq!(resp.status, 200);
        assert!(
            resp.headers
                .iter()
                .any(|(n, v)| n == "content-type" && v == "text/plain"),
            "expected content-type: text/plain in headers, got {:?}",
            resp.headers
        );
        assert_eq!(resp.body.as_ref(), &b"hello\n"[..]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_drains_multi_frame_response_body() {
        let chunks = vec![
            Bytes::from_static(b"abcd"),
            Bytes::from_static(b"efgh"),
            Bytes::from_static(b"ijkl"),
        ];
        let (addr, _handle) = spawn_h2_server_chunks(chunks).await;
        let mut client = Client::connect(addr, "test.example").await.unwrap();
        let req = mk_request("GET", "/", vec![], Bytes::new());
        let resp = client.send_request(req).await.expect("send_request");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body.as_ref(), &b"abcdefghijkl"[..]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_strips_h2_forbidden_hop_by_hop_headers() {
        let (addr, captured, _handle) = spawn_h2_server(|_req| {
            http::Response::builder()
                .status(200)
                .body(Bytes::from_static(b""))
                .unwrap()
        })
        .await;
        let mut client = Client::connect(addr, "test.example").await.unwrap();
        let req = mk_request(
            "GET",
            "/",
            vec![
                ("connection", "close"),
                ("transfer-encoding", "chunked"),
                ("keep-alive", "timeout=5"),
                ("upgrade", "h2c"),
                ("proxy-connection", "close"),
                ("x-keep", "preserved"),
            ],
            Bytes::new(),
        );
        let _resp = client.send_request(req).await.expect("send_request");
        let captured = captured.lock().await;
        let captured = captured.as_ref().expect("h2 server captured request");
        for forbidden in &[
            "connection",
            "transfer-encoding",
            "keep-alive",
            "upgrade",
            "proxy-connection",
        ] {
            assert!(
                captured.headers().iter().all(|(n, _)| n.as_str() != *forbidden),
                "forbidden header {forbidden} appeared in upstream request"
            );
        }
        assert!(
            captured
                .headers()
                .iter()
                .any(|(n, v)| n.as_str() == "x-keep" && v.as_bytes() == b"preserved"),
            "non-forbidden header x-keep was unexpectedly stripped"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_maps_h2_handshake_failure_to_typed_error() {
        // Spawn a TCP listener that responds to the H2C handshake with HTTP/1.1
        // bytes (not a SETTINGS frame). h2::client::handshake should reject
        // with an h2::Error mapped to Http2Error::H2ClientHandshake.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _handle = tokio::spawn(async move {
            if let Ok((mut tcp, _)) = listener.accept().await {
                use tokio::io::AsyncWriteExt;
                let _ = tcp
                    .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
                    .await;
                let _ = tcp.shutdown().await;
            }
        });
        let result = Client::connect(addr, "test.example").await;
        match result {
            Err(Http2Error::H2ClientHandshake { source: _ }) => {}
            other => panic!("expected H2ClientHandshake, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2.5: Run the tests to verify they fail.**

Run: `cargo test -p envoy-http2 --lib client -- --nocapture`
Expected: 8 tests fail with `unimplemented!("Task 2.5 lands the body")` panic from `Client::connect` (the first test reaches connect; the others fail before reaching their assertions because connect panics).

- [ ] **Step 2.6: Implement `Client::connect`.**

Replace the body of the `Client::connect` placeholder:

```rust
impl Client {
    pub async fn connect(addr: SocketAddr, host: &str) -> Result<ClientStream, Http2Error> {
        let tcp = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|source| Http2Error::UpstreamConnect { addr, source })?;
        let (send_request, connection) = h2::client::handshake(tcp)
            .await
            .map_err(|source| Http2Error::H2ClientHandshake { source })?;
        // Per parent §6 signpost 6 / SPEC §6 local signpost 19: drive the
        // h2::client::Connection on a fire-and-forget tokio::spawn for the
        // lifetime of the SendRequest handle. The task terminates when
        // SendRequest drops + the connection gracefully closes; post-response
        // errors are logged but do NOT propagate (the send_request call has
        // already returned by the time the connection task encounters a
        // post-response error per signpost 19).
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::warn!(?error, "h2 client connection task ended with error");
            }
        });
        Ok(ClientStream {
            send_request,
            host: host.to_string(),
        })
    }
}
```

- [ ] **Step 2.7: Implement `ClientStream::send_request`.**

Replace the body of the `ClientStream::send_request` placeholder:

```rust
/// H2-forbidden hop-by-hop headers per RFC 7540 §8.1.2.2 + RFC 9113 §8.2.2.
/// Stripped defensively at the codec edge (the h2 crate also rejects, but the
/// project's posture per parent SPEC §3 architectural rule 4 is to strip at
/// the codec edge symmetric with 05.2's listener-side strip in response.rs).
const H2_FORBIDDEN_HOP_BY_HOP: &[&str] = &[
    "connection",
    "transfer-encoding",
    "keep-alive",
    "upgrade",
    "proxy-connection",
];

impl ClientStream {
    pub async fn send_request(&mut self, request: Request) -> Result<Response, Http2Error> {
        // (a) :authority resolution. Explicit Host: wins over the captured
        // host. Mirrors envoy_http1::Client::send_request's host-resolution
        // posture per crates/envoy-http1/src/client.rs.
        let authority: String = request
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("host"))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| self.host.clone());

        // (b) Build the http::Request<()> head. URI is absolute-form per RFC
        // 7540 §8.1.2.3 — `:scheme://:authority:path` so the h2 codec
        // populates :scheme, :authority, :path correctly.
        let uri_str = format!("http://{authority}{}", request.path);
        let http_req = http::Request::builder()
            .method(request.method.as_str())
            .uri(uri_str.as_str())
            .version(http::Version::HTTP_2)
            .body(())
            .map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
        let (mut parts, ()) = http_req.into_parts();

        // (c) Apply request headers, lowercasing names + stripping H2-forbidden
        // hop-by-hop names defensively + skipping Host: (became :authority).
        for (name, value) in &request.headers {
            let lower = name.to_ascii_lowercase();
            if lower == "host" {
                continue;
            }
            if H2_FORBIDDEN_HOP_BY_HOP.iter().any(|&f| f == lower.as_str()) {
                continue;
            }
            let header_name = http::HeaderName::from_bytes(lower.as_bytes())
                .map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
            let header_value = http::HeaderValue::from_str(value)
                .map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
            parts.headers.append(header_name, header_value);
        }
        let http_req_with_headers = http::Request::from_parts(parts, ());

        // (d) Decide end_of_stream from the body. If empty, end_of_stream=true
        // on the HEADERS frame; no DATA frame emitted. If non-empty, send
        // HEADERS with end_of_stream=false then DATA with end_of_stream=true.
        let body = request.body.unwrap_or_else(Bytes::new);
        let body_is_empty = body.is_empty();

        let (response_future, mut send_stream) = self
            .send_request
            .send_request(http_req_with_headers, body_is_empty)
            .map_err(|source| Http2Error::H2SendRequest { source })?;

        if !body_is_empty {
            send_stream
                .send_data(body, true)
                .map_err(|source| Http2Error::H2SendRequest { source })?;
        }

        // (e) Read the response head.
        let http_resp = response_future
            .await
            .map_err(|source| Http2Error::H2SendRequest { source })?;
        let (resp_parts, mut recv_stream) = http_resp.into_parts();

        // (f) Drain the response body. Mirrors 05.2 D3's listener-side body
        // intake pattern (concat into a single Bytes via BytesMut); per parent
        // §6 signpost 9 the body-bytes drain budget is unbounded in 05.3.
        let mut body_bytes = bytes::BytesMut::new();
        while let Some(chunk_result) = recv_stream.data().await {
            let chunk =
                chunk_result.map_err(|source| Http2Error::H2RecvBody { source })?;
            body_bytes.extend_from_slice(&chunk);
            recv_stream
                .flow_control()
                .release_capacity(chunk.len())
                .map_err(|source| Http2Error::H2RecvBody { source })?;
        }

        // (g) Translate http::Response<()> + body bytes → envoy Response. The
        // status range is 100..=599 per route-walk + h2 codec validation; the
        // BadStatusCode variant is defense-in-depth (mirrors response.rs).
        let status = resp_parts.status.as_u16();
        if !(100..=599).contains(&status) {
            return Err(Http2Error::BadStatusCode { status });
        }
        let mut headers: Vec<(String, String)> = Vec::with_capacity(resp_parts.headers.len());
        for (name, value) in resp_parts.headers.iter() {
            // h2 lowercases all header names per RFC 7540; preserve as-is.
            let value_str = match value.to_str() {
                Ok(s) => s.to_string(),
                Err(_) => continue, // skip malformed (non-ASCII) values defensively
            };
            headers.push((name.as_str().to_string(), value_str));
        }
        Ok(Response {
            status,
            reason: None,
            headers,
            body: body_bytes.freeze(),
        })
    }
}
```

- [ ] **Step 2.8: Run the tests to verify they pass.**

Run: `cargo test -p envoy-http2 --lib client -- --nocapture`
Expected: all 8 tests pass.

If `connect_returns_upstream_connect_on_refused` is flaky on platforms where binding-then-dropping doesn't reliably produce ConnectionRefused (some Linux kernels reuse the port before the connect attempt), try (a) connecting to `127.0.0.1:1` (port 1 is privileged-only on Linux/macOS, so a non-root client gets EACCES which on Linux maps to PermissionDenied; on macOS may map to ConnectionRefused). If neither approach yields a deterministic error, accept the planner-time best-effort and record the platform-conditional behavior in PROGRESS Task 2.

- [ ] **Step 2.9: Run the full envoy-http2 test suite + clippy + fmt.**

Run: `cargo test -p envoy-http2 --lib`
Expected: all envoy-http2 tests pass (the pre-existing 05.2 surface + Task 1's 4 new error tests + Task 2's 8 new client tests).

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean. If clippy flags `client.rs` with style suggestions (e.g., `redundant_clone` on `self.host.clone()`), apply and re-run.

Run: `cargo fmt --all -- --check`
Expected: clean. Re-format `client.rs` if rustfmt's output differs.

- [ ] **Step 2.10: Verify the workspace build.**

Run: `cargo build --workspace --all-targets`
Expected: clean.

If `envoy-cluster` becomes a `[dependencies]` of `envoy-http2` (per Step 2.3) and is unused anywhere in `client.rs`, the build remains clean (cargo does not flag unused path-deps unless `unused_crate_dependencies` is enabled in `lib.rs`). If `cargo build` fails with an unused-dep warning, re-locate the lift to Task 7.

- [ ] **Step 2.11: Commit Task 2.**

```bash
git add crates/envoy-http2/src/client.rs \
        crates/envoy-http2/src/lib.rs \
        crates/envoy-http2/Cargo.toml
git commit -m "$(cat <<'EOF'
phase 05.3: envoy-http2::client.rs (per-connection H2C client) (task 2)

SPEC §3 D1 main: new module crates/envoy-http2/src/client.rs ships
envoy_http2::Client + ClientStream. Public surface mirrors
envoy_http1::Client from 04.3 D1 — Client::connect(addr, host) and
ClientStream::send_request(Request) -> Response on the same protocol-
agnostic envoy_http1::codec::{Request,Response} value types per SPEC §3
cross-sub-phase architectural rule 2.

Connect runs a plaintext TCP, h2::client::handshake (mapping h2::Error
→ Http2Error::H2ClientHandshake), and drives the h2::client::Connection
on a fire-and-forget tokio::spawn per parent §6 signpost 6 / SPEC §6
local signpost 19. send_request synthesizes the H2 pseudo-headers
(:method/:path/:authority/:scheme: http) per parent §6 signpost 12 and
SPEC §3 cross-sub-phase architectural rule 3 — :authority sourced from
explicit Host: header if present, else captured host (mirrors 04.3
envoy_http1::Client). H2-forbidden hop-by-hop headers (connection,
transfer-encoding, keep-alive, upgrade, proxy-connection) stripped at
the codec edge per SPEC §3 architectural rule 4. Header names lowercased
per parent §6 signpost 11. Response body drained from h2::RecvStream
into bytes::Bytes via the same pattern 05.2 D3 uses on the listener-side
body intake.

8 unit tests cover: connect succeeds; connect returns UpstreamConnect on
refused; pseudo-header synthesis with captured host; explicit Host:
overrides captured host; status/headers/body translation; multi-DATA-
frame body drain; H2-forbidden hop-by-hop strip; H2 handshake failure
maps to H2ClientHandshake.

envoy-cluster lifted from [dev-dependencies] to [dependencies] on
crates/envoy-http2/Cargo.toml — preparing for Task 7's symmetric H1-or-
H2 dispatch arm at envoy-http2/src/hcm.rs. lib.rs gains pub mod client;
+ pub use client::{Client, ClientStream}; re-exports.

No new top-level Cargo deps (h2 + http already direct from 05.2 Task 1
+ ADR-0027). Cargo.lock unchanged.
EOF
)"
```

---


## Task 3 — `envoy-config` cluster-side `typed_extension_protocol_options` schema + supporting types + validator extensions

**Files:**

- Modify: `crates/envoy-config/src/bootstrap.rs` — extend `Cluster` struct (line 48); add 4 new types after the `LoadAssignment` block; extend `validate` (line 927) with the cluster-side typed_extension walk + URL check + range-check delegation; refactor the existing range-check helper out of `validate_hcm`'s body to a free function shared between listener-side and cluster-side use sites; append ~8 new validator unit tests.
- Modify: `crates/envoy-config/src/lib.rs` — append `ConfigError::MutuallyExclusiveExplicitHttpConfig` variant + `ConfigError::UnsupportedTypedConfigUrl` variant (the SPEC §3 D2.b says verify whether the existing typed-config validator pattern uses a different rejection variant; per the grep at Step 3.1, it does NOT exist, so add). Extend the `pub use bootstrap::{...}` re-export.

**Estimated LoC:** ~335 (~120 schema delta = 4 new structs + 1 new field on `Cluster`; ~60 validator path = mutual-exclusion + URL check + range-check delegation; ~120 unit tests = 8 new × ~15 LoC; ~10 `ConfigError` extension; ~25 LoC for the range-check helper extraction).

**Signposts settled:**

- SPEC §3 D2.a: `typed_extension_protocol_options` field on `Cluster`; supporting type hierarchy reuses 05.2 D2.b's `Http2ProtocolOptions` struct.
- SPEC §3 D2.b: `ConfigError::MutuallyExclusiveExplicitHttpConfig { cluster: String }`; conditional `ConfigError::UnsupportedTypedConfigUrl { got: String, expected: &'static str }` (verified absent at Step 3.1).
- SPEC §6 inherited signpost 17 (fixture 0010 declares STRICT_DNS): the schema must accept STRICT_DNS + typed_extension_protocol_options.HttpProtocolOptions combined (Test 8 below explicitly covers this).
- SPEC §6 local signpost 16 (envoy-cluster public dep on envoy-config types): `envoy-cluster::from_bootstrap` will consume these new types at Task 5; existing path-dep covers it.

- [ ] **Step 3.1: Cross-check the existing schema + ConfigError variants.**

Run: `grep -n 'pub struct Cluster\b\|UnsupportedTypedConfigUrl\|MutuallyExclusiveExplicitHttpConfig\|typed_extension_protocol_options' crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs`
Expected: confirms `Cluster` struct exists at `bootstrap.rs:48`; confirms NEITHER `UnsupportedTypedConfigUrl` NOR `MutuallyExclusiveExplicitHttpConfig` variants exist (no `lib.rs` matches); confirms NO existing `typed_extension_protocol_options` field anywhere in the file.

Run: `grep -n 'fn validate(' crates/envoy-config/src/bootstrap.rs`
Expected: `927:pub(crate) fn validate(bootstrap: &mut Bootstrap) -> Result<(), crate::ConfigError> {`. Record the function entry line.

Run: `grep -nA 8 'pub struct Http2ProtocolOptions' crates/envoy-config/src/bootstrap.rs`
Expected: confirms the 4-field struct (`max_concurrent_streams`, `initial_stream_window_size`, `initial_connection_window_size`, `max_frame_size`) at `bootstrap.rs:352`. Re-used by D2.a unchanged.

Run: `grep -nB 2 -A 30 'fn validate_hcm' crates/envoy-config/src/bootstrap.rs | head -60`
Expected: shows the existing `validate_hcm` signature + the in-body Http2ProtocolOptions range checks at lines 1180-1215. The range checks need to be hoisted to a free function for shared use between listener and cluster sites.

- [ ] **Step 3.2: Write a failing test for accept-with-typed-extension-protocol-options-http2 (cluster side).**

Append to `crates/envoy-config/src/bootstrap.rs::tests` (after the existing `fuzz_corpus_hcm_codec_http2_seed_parses` test at line 5001 area; verify exact end via `grep -n 'fn fuzz_corpus_hcm_codec_http2_seed_parses\|^}$' crates/envoy-config/src/bootstrap.rs | tail -5`):

```rust
    #[test]
    fn parses_cluster_with_typed_extension_protocol_options_http2() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options:
              max_concurrent_streams: 100
              initial_stream_window_size: 65535
              initial_connection_window_size: 65535
              max_frame_size: 16384
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let cluster = &bs.static_resources.clusters[0];
        let teo = cluster
            .typed_extension_protocol_options
            .as_ref()
            .expect("typed_extension_protocol_options present");
        let h2 = teo
            .http_protocol_options
            .explicit_http_config
            .http2_protocol_options
            .as_ref()
            .expect("http2 arm present");
        assert_eq!(h2.max_concurrent_streams, Some(100));
        assert_eq!(h2.max_frame_size, Some(16384));
    }
```

- [ ] **Step 3.3: Run the test to verify it fails.**

Run: `cargo test -p envoy-config parses_cluster_with_typed_extension_protocol_options_http2 -- --nocapture`
Expected: FAIL with serde error along the lines of `unknown field "typed_extension_protocol_options"` (the `Cluster` struct uses `#[serde(deny_unknown_fields)]`).

- [ ] **Step 3.4: Add the supporting types to `bootstrap.rs`.**

Insert after the existing `LoadAssignment` block (after the closing `}` of `pub struct LoadAssignment` at line 109) and before `pub struct LocalityLbEndpoints` at line 112. The 4 new types:

```rust
/// Cluster-side typed_extension_protocol_options (Envoy's mechanism for
/// per-cluster protocol-extension config). 05.3 NEW per SPEC §3 D2.a.
/// The single recognized key is the upstreams.http.v3.HttpProtocolOptions
/// extension; the validator additionally rejects unknown @type URLs and
/// mutually-exclusive explicit_http_config arms.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TypedExtensionProtocolOptions {
    #[serde(rename = "envoy.extensions.upstreams.http.v3.HttpProtocolOptions")]
    pub http_protocol_options: HttpProtocolOptions,
}

/// The upstreams.http.v3.HttpProtocolOptions typed-extension. Carries the
/// `@type` URL (validated literal) + the `explicit_http_config` oneof.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpProtocolOptions {
    #[serde(rename = "@type")]
    pub type_url: String,
    pub explicit_http_config: ExplicitHttpConfig,
}

/// Envoy's `ExplicitHttpConfig` is a oneof: either http_protocol_options
/// (H1 arm; empty in 05.3 — see Http1ProtocolOptions) or
/// http2_protocol_options (H2 arm; reuses 05.2 D2.b's Http2ProtocolOptions
/// unchanged). The validator (validate, line 927) enforces mutual
/// exclusion via ConfigError::MutuallyExclusiveExplicitHttpConfig.
#[derive(Debug, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct ExplicitHttpConfig {
    #[serde(default)]
    pub http_protocol_options: Option<Http1ProtocolOptions>,
    #[serde(default)]
    pub http2_protocol_options: Option<Http2ProtocolOptions>,
}

/// H1 arm of ExplicitHttpConfig. Empty in 05.3; future fields like
/// chunk_encoding / allow_chunked_length / enable_trailers defer per
/// SPEC §4 to whichever phase first needs cluster-side H1 protocol-tuning.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Http1ProtocolOptions {}
```

- [ ] **Step 3.5: Add the field to `Cluster`.**

Edit `Cluster` at `bootstrap.rs:48` to add the new field. Final shape:

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cluster {
    pub name: String,
    #[serde(rename = "type")]
    pub cluster_type: ClusterType,
    pub lb_policy: LbPolicy,
    pub load_assignment: LoadAssignment,
    #[serde(default)]
    pub transport_socket: Option<TransportSocket>,
    /// 05.4 NEW per ADR-0024: optional DNS lookup family override for
    /// STRICT_DNS / LOGICAL_DNS clusters. Defaults to None, which lets
    /// the upstream Envoy honor its proto default (AUTO). envoy-rust does
    /// NOT consume this field at runtime in 05.4; only the upstream Envoy
    /// side observes the V4_ONLY knob via per-fixture envoy.yaml edits
    /// (D2 of phase 05.4 — see SPEC §3 D2).
    #[serde(default)]
    pub dns_lookup_family: Option<DnsLookupFamily>,
    /// 05.3 NEW per SPEC §3 D2.a: cluster-side typed_extension_protocol_options
    /// carrying the upstreams.http.v3.HttpProtocolOptions extension. Defaults
    /// to None, which projects to UpstreamProtocol::Http1 at envoy-cluster
    /// from_bootstrap time (envoy-cluster Task 5) — backwards-compat with all
    /// phase-04 clusters. The validator enforces:
    ///   - @type URL literal "type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions"
    ///   - mutual exclusion of explicit_http_config arms
    ///   - RFC 7540 range checks on http2_protocol_options (delegated to
    ///     validate_http2_protocol_options_ranges; same checks as listener-side).
    #[serde(default)]
    pub typed_extension_protocol_options: Option<TypedExtensionProtocolOptions>,
}
```

- [ ] **Step 3.6: Run the test to verify it parses.**

Run: `cargo test -p envoy-config parses_cluster_with_typed_extension_protocol_options_http2 -- --nocapture`
Expected: PASS the parse step; if there's no `@type`-validation in `validate` yet, the test passes structurally. (Some assertions depend on `parse_bootstrap`'s success path which runs `validate`; the `@type` check is added in Step 3.7.)

- [ ] **Step 3.7: Add the new ConfigError variants.**

Edit `crates/envoy-config/src/lib.rs`. Insert after the existing `Http2ProtocolOptionsOutOfRange` block (line 122 area) and before `UnsupportedCodecType` at line 123:

```rust
    /// Cluster-side `typed_extension_protocol_options.HttpProtocolOptions`'s
    /// `explicit_http_config` had BOTH `http_protocol_options` (H1 arm) AND
    /// `http2_protocol_options` (H2 arm) set. Envoy's proto defines these as
    /// a oneof; at most one may be set. 05.3 NEW per SPEC §3 D2.a.
    #[error(
        "cluster '{cluster}': explicit_http_config has both http_protocol_options and http2_protocol_options set; at most one is permitted"
    )]
    MutuallyExclusiveExplicitHttpConfig { cluster: String },
    /// `typed_extension_protocol_options.HttpProtocolOptions.@type` did not
    /// equal the expected URL literal. 05.3 NEW per SPEC §3 D2.a.
    #[error("typed config @type {got:?} not supported; expected {expected:?}")]
    UnsupportedTypedConfigUrl {
        got: String,
        expected: &'static str,
    },
```

Extend the `pub use bootstrap::{...}` re-export at lines 10-19. Add `ExplicitHttpConfig`, `Http1ProtocolOptions`, `HttpProtocolOptions`, `TypedExtensionProtocolOptions` to the alphabetic-ordered list. The full updated re-export block:

```rust
pub use bootstrap::{
    Address, Admin, Bootstrap, CertificateValidationContext, Cluster, ClusterType, CodecType,
    CommonTlsContext, DataSource, DirectResponse, DnsLookupFamily, DownstreamTlsContext, Endpoint,
    ExplicitHttpConfig, FilterChain, FilterChainMatch, HeaderMatcher, HeaderMatcherMode,
    Http1ProtocolOptions, Http2ProtocolOptions, HttpConnectionManagerConfig, HttpFilter,
    HttpFilterTypedConfig, HttpProtocolOptions, Int64Range, LbEndpoint, LbPolicy, Listener,
    LoadAssignment, LocalityLbEndpoints, NetworkFilter, Node, Route, RouteAction,
    RouteAction_Route, RouteConfiguration, RouteMatch, RouterConfig, SafeRegex, SocketAddress,
    StaticResources, StringMatcher, StringMatcherMode, TcpProxyConfig, TlsCertificate,
    TransportSocket, TransportSocketTypedConfig, TypedConfig, TypedExtensionProtocolOptions,
    UpstreamTlsContext, VirtualHost,
};
```

- [ ] **Step 3.8: Hoist the Http2ProtocolOptions range checks to a free function.**

The existing range checks live in `validate_hcm` body at `bootstrap.rs:1180-1215`. Extract to a free function `validate_http2_protocol_options_ranges` with the same body. Insert after `validate_hcm`'s closing `}`:

```rust
/// Validate RFC 7540 wire-format range constraints on Http2ProtocolOptions
/// fields. Hoisted from validate_hcm at 05.3 Task 3 so the listener-side
/// (validate_hcm) and cluster-side (validate's typed_extension walk) sites
/// share the same range checks. Mutates nothing; returns ConfigError on
/// out-of-range values.
fn validate_http2_protocol_options_ranges(
    opts: &Http2ProtocolOptions,
) -> Result<(), crate::ConfigError> {
    // (Body extracted verbatim from validate_hcm at HEAD `f33dac9` lines
    // 1182-1215. Re-grep at task time and copy the exact block.)
    if let Some(v) = opts.max_frame_size {
        if !(16384..=16777215).contains(&v) {
            return Err(crate::ConfigError::Http2ProtocolOptionsOutOfRange {
                field: "max_frame_size",
                value: v,
                range: (16384, 16777215),
            });
        }
    }
    if let Some(v) = opts.initial_stream_window_size {
        if v > 0x7FFF_FFFF {
            return Err(crate::ConfigError::Http2ProtocolOptionsOutOfRange {
                field: "initial_stream_window_size",
                value: v,
                range: (0, 0x7FFF_FFFF),
            });
        }
    }
    if let Some(v) = opts.initial_connection_window_size {
        if v > 0x7FFF_FFFF {
            return Err(crate::ConfigError::Http2ProtocolOptionsOutOfRange {
                field: "initial_connection_window_size",
                value: v,
                range: (0, 0x7FFF_FFFF),
            });
        }
    }
    Ok(())
}
```

Replace the old in-body block at `validate_hcm` (lines 1180-1215) with a single call:

```rust
    if let Some(opts) = &hcm.http2_protocol_options {
        validate_http2_protocol_options_ranges(opts)?;
    }
```

Run: `cargo test -p envoy-config -- http2_protocol_options`
Expected: the existing 4 listener-side range tests still pass (the body is structurally identical; only the call-site moved).

- [ ] **Step 3.9: Extend `validate` with the cluster-side typed_extension walk.**

Edit `validate` at `bootstrap.rs:927`. Find the existing per-cluster loop (search for `for cluster in &mut bootstrap.static_resources.clusters` or similar). Inside the per-cluster body, after the existing transport_socket / dns_lookup_family validation, add:

```rust
        // 05.3 NEW per SPEC §3 D2.a: validate cluster-side
        // typed_extension_protocol_options.
        if let Some(teo) = &cluster.typed_extension_protocol_options {
            const EXPECTED_TYPE_URL: &str =
                "type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions";
            if teo.http_protocol_options.type_url != EXPECTED_TYPE_URL {
                return Err(crate::ConfigError::UnsupportedTypedConfigUrl {
                    got: teo.http_protocol_options.type_url.clone(),
                    expected: EXPECTED_TYPE_URL,
                });
            }
            let ehc = &teo.http_protocol_options.explicit_http_config;
            if ehc.http_protocol_options.is_some() && ehc.http2_protocol_options.is_some() {
                return Err(crate::ConfigError::MutuallyExclusiveExplicitHttpConfig {
                    cluster: cluster.name.clone(),
                });
            }
            if let Some(h2_opts) = &ehc.http2_protocol_options {
                validate_http2_protocol_options_ranges(h2_opts)?;
            }
        }
```

Locate the exact insertion point at task time. The file is 5080 lines; search via `grep -nB 1 -A 3 'static_resources.clusters' crates/envoy-config/src/bootstrap.rs | head -30` for the per-cluster validator loop.

- [ ] **Step 3.10: Write the remaining 7 unit tests + 1 corpus-walk acceptance test.**

Append to `bootstrap.rs::tests`:

```rust
    #[test]
    fn parses_cluster_with_typed_extension_protocol_options_http1() {
        // The H1 arm of explicit_http_config is the empty Http1ProtocolOptions.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http_protocol_options: {}
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let cluster = &bs.static_resources.clusters[0];
        let teo = cluster
            .typed_extension_protocol_options
            .as_ref()
            .expect("teo present");
        assert!(teo
            .http_protocol_options
            .explicit_http_config
            .http_protocol_options
            .is_some());
        assert!(teo
            .http_protocol_options
            .explicit_http_config
            .http2_protocol_options
            .is_none());
    }

    #[test]
    fn rejects_cluster_with_both_http1_and_http2_in_explicit_http_config() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http_protocol_options: {}
            http2_protocol_options: {}
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("rejects mutual");
        assert!(
            matches!(
                err,
                crate::ConfigError::MutuallyExclusiveExplicitHttpConfig { ref cluster }
                    if cluster == "backend"
            ),
            "expected MutuallyExclusiveExplicitHttpConfig {{cluster: backend}}, got {err:?}"
        );
    }

    #[test]
    fn rejects_cluster_with_wrong_typed_config_url() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.config.core.v3.Http2ProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("rejects wrong URL");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedTypedConfigUrl { .. }),
            "expected UnsupportedTypedConfigUrl, got {err:?}"
        );
    }

    #[test]
    fn rejects_cluster_http2_protocol_options_max_frame_size_too_small() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options:
              max_frame_size: 1024
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("rejects out-of-range");
        assert!(
            matches!(
                err,
                crate::ConfigError::Http2ProtocolOptionsOutOfRange {
                    field: "max_frame_size",
                    value: 1024,
                    ..
                }
            ),
            "expected Http2ProtocolOptionsOutOfRange, got {err:?}"
        );
    }

    #[test]
    fn parses_cluster_without_typed_extension_protocol_options_defaults_to_http1() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let cluster = &bs.static_resources.clusters[0];
        assert!(cluster.typed_extension_protocol_options.is_none());
    }

    #[test]
    fn rejects_cluster_with_unknown_typed_extension_key() {
        // Key other than HttpProtocolOptions; serde deny_unknown_fields rejects.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.UnknownExtension":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.UnknownExtension
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("rejects unknown key");
        // serde "unknown field" on TypedExtensionProtocolOptions surfaces as
        // ConfigError::Yaml (the deny_unknown_fields path on the
        // TypedExtensionProtocolOptions wrapper struct).
        assert!(
            matches!(err, crate::ConfigError::Yaml(_)),
            "expected serde Yaml error for unknown typed-extension key, got {err:?}"
        );
    }

    #[test]
    fn parses_cluster_with_strict_dns_and_http2_protocol_options_combined() {
        // Load-bearing for fixture 0010 (Task 10) which combines exactly these
        // two surfaces.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
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
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: localhost, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses STRICT_DNS + H2 combined");
        let cluster = &bs.static_resources.clusters[0];
        assert!(matches!(cluster.cluster_type, ClusterType::StrictDns));
        assert!(cluster.typed_extension_protocol_options.is_some());
    }
```

Plus the corpus-walk acceptance test (lands at Task 4 below; cross-referenced here for parser-coverage):

```rust
    #[test]
    fn fuzz_corpus_cluster_http2_protocol_options_seed_parses() {
        // Lands at Task 4 alongside the seed file. Mirrors the existing
        // fuzz_corpus_hcm_codec_http2_seed_parses pattern at line 5001 area.
        let yaml = include_str!("../fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml");
        crate::parse_bootstrap(yaml).expect("seed parses cleanly");
    }
```

(The corpus-walk test is appended at Task 4 since the seed file lands there. Listed here for plan-coverage.)

- [ ] **Step 3.11: Run all envoy-config tests.**

Run: `cargo test -p envoy-config -- --nocapture`
Expected: all tests pass (existing 04.x + 05.1 + 05.2 + 05.4 surface + Task 3's 7 new tests). The corpus-walk test from Step 3.10's last block fails compile (the seed file does not exist yet); leave it commented OR omit until Task 4. **Recommendation:** omit until Task 4 — the corpus-walk test in `bootstrap.rs::tests` lands at Task 4 alongside the seed file. Re-run after Task 4 to confirm.

Run: `cargo build --workspace --all-targets`
Expected: clean.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.

Run: `cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 3.12: Commit Task 3.**

```bash
git add crates/envoy-config/src/bootstrap.rs \
        crates/envoy-config/src/lib.rs \
        docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 05.3: envoy-config cluster-side typed_extension_protocol_options (task 3)

SPEC §3 D2.a/b: cluster-side Http2ProtocolOptions schema via Envoy's
typed_extension_protocol_options.HttpProtocolOptions mechanism. New types
TypedExtensionProtocolOptions / HttpProtocolOptions / ExplicitHttpConfig
/ Http1ProtocolOptions on crates/envoy-config/src/bootstrap.rs;
Cluster.typed_extension_protocol_options: Option<...> field added at the
bottom of the Cluster struct.

The 4-field 05.2-landed Http2ProtocolOptions struct is reused unchanged
on the cluster side. The validate_http2_protocol_options_ranges helper
is hoisted out of validate_hcm's body to a free function so listener-
side and cluster-side use sites share the RFC 7540 range checks (the
field-name vs. use-site discriminator is irrelevant — same checks fire
from both sites).

ConfigError gains MutuallyExclusiveExplicitHttpConfig { cluster: String
} (rejection when both H1 and H2 arms are set on explicit_http_config)
and UnsupportedTypedConfigUrl { got, expected: &'static str } (rejection
when @type does not match the expected literal).

7 new validator unit tests cover: parses H2 arm with all 4 fields;
parses empty H1 arm; rejects mutual exclusion (with cluster name in
the variant); rejects wrong @type URL; rejects out-of-range
max_frame_size; parses absent typed_extension; rejects unknown
typed-extension key (serde deny_unknown_fields). Plus the load-bearing
parses_cluster_with_strict_dns_and_http2_protocol_options_combined test
(fixture 0010 in Task 10 combines both surfaces).

No new top-level Cargo deps. No ADRs landed.
EOF
)"
```

---

## Task 4 — `cluster_http2_protocol_options.yaml` fuzz corpus seed + corpus-walk acceptance test

**Files:**

- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml`.
- Modify: `crates/envoy-config/fuzz/.gitignore` — append `!corpus/parse_bootstrap/cluster_http2_protocol_options.yaml`.
- Modify: `crates/envoy-config/src/bootstrap.rs::tests` — append `fuzz_corpus_cluster_http2_protocol_options_seed_parses` test (per Task 3's Step 3.10 deferred entry).

**Estimated LoC:** ~50 (~25 YAML seed + ~10 .gitignore + ~15 corpus-walk test).

**Signposts settled:**

- SPEC §1 acceptance signal (d): the existing `parse_bootstrap` fuzz target runs clean with the new corpus.
- SPEC §6 local signpost 22 (Fuzz seed file path consistency): `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml` matches the existing 04.x + 05.1 + 05.2 seed shape.

- [ ] **Step 4.1: Create the corpus seed file.**

Create `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml`:

```yaml
node: { id: fuzz-corpus-05-3, cluster: envoy-rust-fuzz }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                http2_protocol_options:
                  max_concurrent_streams: 100
                  initial_stream_window_size: 65535
                  initial_connection_window_size: 65535
                  max_frame_size: 16384
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: localhost, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options:
              max_concurrent_streams: 100
              initial_stream_window_size: 65535
              initial_connection_window_size: 65535
              max_frame_size: 16384
```

The seed exercises:
- `codec_type: HTTP2` listener-side accept (05.2 D2.a).
- listener-side `http2_protocol_options { ... 4 fields ... }` (05.2 D2.b).
- `type: STRICT_DNS` cluster type (05.1 ADR-0023).
- cluster-side `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options { ... 4 fields ... }` (05.3 D2.a).

- [ ] **Step 4.2: Append to `.gitignore`.**

Edit `crates/envoy-config/fuzz/.gitignore`. Append after the existing `!corpus/parse_bootstrap/strict_dns_cluster.yaml` line (and before any other entries) — alphabetic order in the file's existing block; `cluster_http2_protocol_options` sorts before `hcm_codec_http2`. Verify alphabetic posture at task time via `cat crates/envoy-config/fuzz/.gitignore`. Add:

```
!corpus/parse_bootstrap/cluster_http2_protocol_options.yaml
```

Recommendation: append at the end of the existing allow-list block (the `.gitignore` does not enforce alphabetic order on the existing entries; consistency is "land the new entry at the end" per the 04.x + 05.1 + 05.2 + 05.4 precedent).

- [ ] **Step 4.3: Append the corpus-walk acceptance test.**

Edit `crates/envoy-config/src/bootstrap.rs::tests`. Append after the existing `fuzz_corpus_hcm_codec_http2_seed_parses` (line 5001 area):

```rust
    #[test]
    fn fuzz_corpus_cluster_http2_protocol_options_seed_parses() {
        // Mirrors the existing fuzz_corpus_hcm_codec_http2_seed_parses
        // pattern. The seed exercises the cluster-side
        // typed_extension_protocol_options accept-path; the fuzzer never runs
        // the H2 codec or the runtime cluster construction
        // (`parse_bootstrap` only exercises serde + the validator).
        let yaml = include_str!(
            "../fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml"
        );
        crate::parse_bootstrap(yaml).expect("seed parses cleanly");
    }
```

- [ ] **Step 4.4: Run the test.**

Run: `cargo test -p envoy-config fuzz_corpus_cluster_http2_protocol_options_seed_parses -- --nocapture`
Expected: PASS.

- [ ] **Step 4.5: Run the `parse_bootstrap` fuzz target's short-budget run against the new corpus.**

Run: `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30 --runs=10000`
Expected: clean run; the new seed is exercised; no `panic!` / no `unwrap()` failures. If `cargo +nightly fuzz` is unavailable in the local env, defer the run to Task 12 state-4 verification (CI's nightly fuzz job covers it). Record any panics in PROGRESS Task 4.

- [ ] **Step 4.6: Verify the workspace build + clippy + fmt.**

Run: `cargo build --workspace --all-targets && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: all clean.

- [ ] **Step 4.7: Commit Task 4.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml \
        crates/envoy-config/fuzz/.gitignore \
        crates/envoy-config/src/bootstrap.rs \
        docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 05.3: fuzz corpus seed cluster_http2_protocol_options.yaml (task 4)

SPEC §3 D2 fuzz: new corpus seed at
crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml
exercising the cluster-side typed_extension_protocol_options accept-path
landed at Task 3. Mirrors the 04.x + 05.1 + 05.2 seed shape:

  - listener: codec_type: HTTP2 + listener-side http2_protocol_options
    (05.2 D2.b).
  - cluster: type: STRICT_DNS (05.1 ADR-0023) + typed_extension_protocol_options.
    HttpProtocolOptions.explicit_http_config.http2_protocol_options
    (05.3 D2.a).

.gitignore allow-list extended. Acceptance test
fuzz_corpus_cluster_http2_protocol_options_seed_parses appended to
bootstrap.rs::tests per the existing fuzz_corpus_*_seed_parses precedent.

`parse_bootstrap` fuzz target's short-budget run is unchanged in shape;
the new seed is exercised.
EOF
)"
```

---


## Task 5 — `envoy-cluster::UpstreamProtocol` enum + `Cluster.upstream_protocol` field + `from_bootstrap` projection

**Files:**

- Modify: `crates/envoy-cluster/src/cluster.rs` — add `UpstreamProtocol { Http1, Http2 }` enum; add `Cluster.upstream_protocol: UpstreamProtocol` field; add `Cluster::upstream_protocol()` accessor + `ClusterHandle::upstream_protocol()` delegate accessor; extend `from_bootstrap` with the projection from the parsed cluster's `typed_extension_protocol_options`. Append 3 unit tests.

**Estimated LoC:** ~110 (~30 enum + struct field + 2 accessors + projection match arm; ~50 tests = 3 × ~17 LoC; ~30 LoC for the YAML constants in tests).

**Signposts settled:**

- SPEC §3 D3: `UpstreamProtocol { Http1, Http2 }` typed enum; `Cluster.upstream_protocol` set at cluster-build time in `from_bootstrap` from `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config`; defaulted to `Http1`; `Cluster::upstream_protocol()` + `ClusterHandle::upstream_protocol()` accessor pair mirrors `Cluster::name()` / `ClusterHandle::name()`.
- SPEC §6 inherited signpost 1 (typed-field-set-at-cluster-build-time): no derived/lazy lookup.
- SPEC §6 local signpost 15 (defense-in-depth on the projection): the "both Some" case is unreachable at runtime (Task 3's validator rejects it); projection covers with `_ => UpstreamProtocol::Http1` defense-in-depth.
- SPEC §6 local signpost 17 (existing 05.1 STRICT_DNS resolution branch unaffected): the `upstream_protocol` projection runs alongside the cluster_type match; orthogonal.
- SPEC §6 local signpost 16 (envoy-cluster public dep on envoy-config types): `envoy-cluster/Cargo.toml` already path-deps `envoy-config` per 02.1; no new dep entry.

- [ ] **Step 5.1: Cross-check the existing `Cluster` + `ClusterHandle` shape.**

Run: `grep -nA 4 'pub struct Cluster\b\|pub struct ClusterHandle\|pub fn name\|pub fn from_bootstrap' crates/envoy-cluster/src/cluster.rs | head -30`
Expected: confirms `Cluster` at line 12 (`name`/`endpoints`/`cursor`); `Cluster::name()` accessor at lines 24-26; `ClusterHandle` at line 44 with `name()` delegate at lines 60-62; `from_bootstrap` at line 153.

Run: `grep -n 'envoy-config' crates/envoy-cluster/Cargo.toml`
Expected: `envoy-config = { path = "../envoy-config" }` already present (per phase 02.1).

- [ ] **Step 5.2: Write 3 failing tests.**

Append to `crates/envoy-cluster/src/cluster.rs::tests`:

```rust
    /// Helper: build a Bootstrap from a YAML string and run from_bootstrap;
    /// returns the resulting ClusterManager. Panics on parse / build error.
    async fn build_cluster_mgr(yaml: &str) -> ClusterManager {
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("parse");
        from_bootstrap(&bootstrap).await.expect("from_bootstrap")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cluster_upstream_protocol_defaults_to_http1() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
"#;
        let mgr = build_cluster_mgr(yaml).await;
        let handle = mgr.get("backend").expect("backend cluster");
        assert_eq!(handle.upstream_protocol(), UpstreamProtocol::Http1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cluster_upstream_protocol_http2_set_from_typed_extension_protocol_options() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options:
              max_concurrent_streams: 100
"#;
        let mgr = build_cluster_mgr(yaml).await;
        let handle = mgr.get("backend").expect("backend cluster");
        assert_eq!(handle.upstream_protocol(), UpstreamProtocol::Http2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cluster_upstream_protocol_http1_set_from_explicit_http1_options() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http_protocol_options: {}
"#;
        let mgr = build_cluster_mgr(yaml).await;
        let handle = mgr.get("backend").expect("backend cluster");
        assert_eq!(handle.upstream_protocol(), UpstreamProtocol::Http1);
    }
```

- [ ] **Step 5.3: Run the tests to verify they fail.**

Run: `cargo test -p envoy-cluster cluster_upstream_protocol -- --nocapture`
Expected: 3 compile errors of the form `no variant or associated item named UpstreamProtocol/upstream_protocol found`. (The pre-existing tests remain passing.)

- [ ] **Step 5.4: Add the `UpstreamProtocol` enum.**

Edit `crates/envoy-cluster/src/cluster.rs`. Insert after the closing `}` of the `ClusterError` enum (around line 141 per the post-Step-5.1 grep) and before `pub async fn from_bootstrap` (line 153):

```rust
/// Per-cluster upstream protocol selector. Defaulted to `Http1` for
/// backwards-compat with all phase-04 clusters; set at cluster-build time in
/// `from_bootstrap` from the parsed cluster's `typed_extension_protocol_options.
/// HttpProtocolOptions.explicit_http_config`. Mirrors the established
/// `LbPolicy` shape (Clone/Copy/Debug/Default/PartialEq/Eq derives).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpstreamProtocol {
    /// Default. The 04.3-landed router H1 dispatch path.
    #[default]
    Http1,
    /// 05.3 NEW per ADR-0022 (parent-05 split). Selects the
    /// envoy_http2::Client dispatch path at the router H2-arm.
    Http2,
}
```

- [ ] **Step 5.5: Add the `upstream_protocol` field + accessors.**

Edit the `Cluster` struct at lines 11-16:

```rust
#[derive(Debug)]
pub struct Cluster {
    pub(crate) name: String,
    pub(crate) endpoints: Vec<SocketAddr>,
    pub(crate) cursor: AtomicUsize,
    /// 05.3 NEW per SPEC §3 D3: cluster-level upstream protocol selector.
    /// Set in `from_bootstrap` from the parsed cluster's
    /// `typed_extension_protocol_options`. Defaulted to `Http1`.
    pub(crate) upstream_protocol: UpstreamProtocol,
}
```

Add the accessor on `impl Cluster` after the existing `name()` at lines 24-26:

```rust
    /// 05.3 NEW: cluster-level upstream protocol. See `UpstreamProtocol`'s
    /// docs. Mirrors the `name()` accessor's posture (typed value, copy
    /// semantics; no Result, no panic). Per SPEC §6 inherited signpost 1
    /// the typed value is set at cluster-build time, not derived per call.
    pub fn upstream_protocol(&self) -> UpstreamProtocol {
        self.upstream_protocol
    }
```

Add the delegate on `impl ClusterHandle` after the existing `name()` at lines 60-62:

```rust
    /// 05.3 NEW: delegates to `Cluster::upstream_protocol`. Mirrors `name()`'s
    /// posture per SPEC §6 inherited signpost 1.
    pub fn upstream_protocol(&self) -> UpstreamProtocol {
        self.inner.upstream_protocol()
    }
```

- [ ] **Step 5.6: Extend `from_bootstrap` with the projection.**

Edit `from_bootstrap` at line 153. Inside the per-cluster loop body (the existing loop iterates `for cfg in &bootstrap.static_resources.clusters`), AFTER the existing `endpoints` Vec is built and the empty-check passes, BEFORE the `Arc::new(Cluster { ... })` construction, project `upstream_protocol`:

```rust
        // 05.3 NEW per SPEC §3 D3: project upstream_protocol from the parsed
        // cluster's typed_extension_protocol_options. The match arm is sync;
        // 05.1's lookup_host async branch is unaffected (the two are
        // orthogonal — cluster_type controls endpoint shape, upstream_protocol
        // controls upstream dispatch). Per SPEC §6 local signpost 15: the
        // "both Some" case is validator-rejected; defense-in-depth defaults
        // to Http1.
        let upstream_protocol = match &cfg.typed_extension_protocol_options {
            None => UpstreamProtocol::Http1,
            Some(teo) => {
                let ehc = &teo.http_protocol_options.explicit_http_config;
                match (&ehc.http_protocol_options, &ehc.http2_protocol_options) {
                    (_, Some(_)) => UpstreamProtocol::Http2,
                    (Some(_), None) => UpstreamProtocol::Http1,
                    (None, None) => UpstreamProtocol::Http1,
                }
            }
        };
```

Then update the `Arc::new(Cluster { ... })` construction at line ~230 to add the field:

```rust
        let cluster = Arc::new(Cluster {
            name: cfg.name.clone(),
            endpoints,
            cursor: AtomicUsize::new(0),
            upstream_protocol,
        });
```

(Locate the exact `Arc::new(Cluster {` line via `grep -n 'Arc::new(Cluster {' crates/envoy-cluster/src/cluster.rs` at task time.)

- [ ] **Step 5.7: Update existing test helpers to construct `Cluster` with the new field.**

The existing test helper `mk_handle` at lines ~257-264 constructs `Cluster { name, endpoints, cursor }` directly (bypasses `from_bootstrap`); update to add `upstream_protocol: UpstreamProtocol::default()`:

```rust
    fn mk_handle(name: &str, endpoints: Vec<SocketAddr>) -> ClusterHandle {
        ClusterHandle {
            inner: Arc::new(Cluster {
                name: name.to_string(),
                endpoints,
                cursor: AtomicUsize::new(0),
                upstream_protocol: UpstreamProtocol::default(),
            }),
        }
    }
```

(Locate the exact `mk_handle` body via `grep -nA 8 'fn mk_handle' crates/envoy-cluster/src/cluster.rs` at task time. There may be ≥ 1 such helper — the I3-closing `static_cluster_constructs_with_literal_ip` test from 05.1 may also construct a Cluster directly; check via `grep -n 'Cluster {' crates/envoy-cluster/src/cluster.rs` and update each call site.)

- [ ] **Step 5.8: Run the tests to verify they pass.**

Run: `cargo test -p envoy-cluster cluster_upstream_protocol -- --nocapture`
Expected: all 3 new tests pass.

Run: `cargo test -p envoy-cluster -- --nocapture`
Expected: all envoy-cluster tests pass (existing ~5+ tests + 3 new). The `pick_endpoint_cycles_over_three_endpoints` and `static_cluster_constructs_with_literal_ip` tests must still pass; if they fail with `Cluster { ... }` construction errors, update the helpers as in Step 5.7.

- [ ] **Step 5.9: Verify the workspace build + clippy + fmt.**

Run: `cargo build --workspace --all-targets`
Expected: clean.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.

Run: `cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 5.10: Commit Task 5.**

```bash
git add crates/envoy-cluster/src/cluster.rs \
        docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 05.3: envoy-cluster Cluster.upstream_protocol field (task 5)

SPEC §3 D3: new UpstreamProtocol { Http1, Http2 } enum +
Cluster.upstream_protocol field set at cluster-build time in
from_bootstrap from the parsed cluster's typed_extension_protocol_options
(landed at Task 3). Defaulted to Http1 for backwards-compat with all
phase-04 clusters — fixtures 0001-0009 declare no
typed_extension_protocol_options and project to Http1, exercising the
unchanged 04.3 H1 router-arm code path.

Cluster::upstream_protocol() + ClusterHandle::upstream_protocol()
accessor pair mirrors the Cluster::name() / ClusterHandle::name() pair
landed in 04.3 D5. Per SPEC §6 inherited signpost 1, the typed value is
set at cluster-build time, not derived per call.

The 05.1 STRICT_DNS resolution branch in from_bootstrap is unaffected;
the upstream_protocol projection runs as a sync match alongside the
cluster_type match (the two are orthogonal — cluster_type controls
endpoint shape, upstream_protocol controls upstream dispatch). Per SPEC
§6 local signpost 15, the "both Some" case is validator-rejected (Task 3
MutuallyExclusiveExplicitHttpConfig); defense-in-depth defaults to Http1.

3 new envoy-cluster unit tests cover the 3 logical cases (default Http1
on absent typed_extension; Http2 from explicit_http_config.http2_arm;
Http1 from explicit_http_config.http1_arm).

No new top-level Cargo deps. No ADRs landed.
EOF
)"
```

---

## Task 6 — Router H2-arm at `crates/envoy-http1/src/hcm.rs`'s `BuildOutcome::Proxy`

**Files:**

- Modify: `crates/envoy-http1/src/hcm.rs` — extend the `BuildOutcome::Proxy` arm at lines 209-303 wrapping the `Client::connect`/`send_request` pair in a `match cluster.upstream_protocol()` selecting H1 (existing 04.3 path) or H2 (NEW; uses `envoy_http2::Client`). `crate::router::write_proxied_response` reused unchanged. Append ~3 new unit tests.
- Modify: `crates/envoy-http1/Cargo.toml` — add `envoy-http2 = { path = "../envoy-http2" }` to `[dependencies]` so the dispatch arm can reach `envoy_http2::Client`. Cross-check that the existing `envoy-cluster` dep is already present (per 04.3) — the dispatch arm consumes `envoy_cluster::UpstreamProtocol`.

**Estimated LoC:** ~100 (~30 LoC dispatch wrap + ~30 LoC the H2 arm of the new match + ~40 LoC for 3 new tests).

**Signposts settled:**

- SPEC §3 D4: H1-or-H2 dispatch on `cluster.upstream_protocol()`; reuses `crate::router::write_proxied_response` unchanged.
- SPEC §6 inherited signpost 4 (`x-envoy-upstream-service-time` measurement window symmetric across H1 and H2): `Instant::now()` immediately before `Client::connect`; `start.elapsed()` immediately after `send_request` returns. The header is appended by `write_proxied_response`.
- SPEC §1 / §3 D4: 502-fallback shape duplicated structurally across both arms; the planner extracts a small helper if the duplication exceeds ~30 LoC, or keeps inline if not (recommendation: inline, matching 04.3's posture).

- [ ] **Step 6.1: Cross-check the existing `BuildOutcome::Proxy` arm.**

Run: `grep -nB 2 -A 50 'BuildOutcome::Proxy {' crates/envoy-http1/src/hcm.rs | head -90`
Expected: confirms the arm at the line range 209-303 area (the SPEC's projected line range; verify exact lines at task time). Record the start/end lines + the existing 502-fallback shape (`tracing::warn!(cluster, addr, error)` + `synth_status(502, close)` + `Http1Response::write_to(&mut downstream)` + `if close { return Ok(()); } else { continue; }`).

Run: `grep -n 'envoy-http2\|envoy-cluster' crates/envoy-http1/Cargo.toml`
Expected: `envoy-cluster` should already be present (per 04.3); `envoy-http2` is NOT present at HEAD — must be added.

- [ ] **Step 6.2: Add `envoy-http2` to `crates/envoy-http1/Cargo.toml`.**

Edit `crates/envoy-http1/Cargo.toml`. Add to `[dependencies]`:

```toml
envoy-http2 = { path = "../envoy-http2" }
```

Insert in alphabetic order with the other path-deps. Verify via `cat crates/envoy-http1/Cargo.toml`.

**Cycle check:** `envoy-http2` already path-deps `envoy-http1` (per `crates/envoy-http2/Cargo.toml:21` from 05.2). Adding `envoy-http2` as a dep of `envoy-http1` would create a circular dep — Cargo rejects this at build time.

**Resolution:** the dispatch arm at `crates/envoy-http1/src/hcm.rs` consumes `envoy_http2::Client` — but `envoy-http2` consumes `envoy_http1::HCMConfig` + `envoy_http1::hcm::build_response` from 05.2. The cycle is real.

**Recommended fix:** the `envoy_http2::Client` surface is consumed at the H2 arm of the dispatch — but the existing `envoy-http1` → `envoy-http2` (via the dispatch) AND `envoy-http2` → `envoy-http1` (via HCMConfig + build_response) cannot both coexist as path-deps. Two options:

(a) **Hoist the H1-or-H2 dispatch out of `crates/envoy-http1/src/hcm.rs`** to a higher-level coordinator that depends on both `envoy-http1` and `envoy-http2`. The natural site is a new module in `envoy-bin` (since `envoy-bin` already path-deps both `envoy-http1` and `envoy-http2` per 05.2 D4 + the existing 04.x posture). But this would require restructuring the HCM's `BuildOutcome::Proxy` consumption — the route-walk is in `envoy-http1`'s `build_response`; the dispatch lives at the consumer.

(b) **Hoist `Client` + `ClientStream` + the dispatch helpers out of `envoy-http2` into a new `envoy-http-client` crate** that depends on `envoy-http1` (for value types) but is depended on by both `envoy-http1` (for the H1 dispatch arm) and `envoy-http2`'s HCM (for the H2 arm symmetry at Task 7). This is the cleanest break of the cycle.

**Pragmatic alternative (chosen):** invert the dependency by having `envoy-http2`'s HCM **NOT** depend on `envoy-http1` at the HCMConfig level (use `envoy_listener::ConnectionHandler` only), and instead share the route-walk via a pure-types crate. **But this contradicts SPEC §3 cross-sub-phase architectural rule 2** ("HCM-on-H2 reuses 04.x's HCMConfig and route-walk wholesale; only the codec layer at the connection edge changes").

**Final recommendation: option (a) — hoist the dispatch out of `envoy-http1`.** The dispatch arm moves from `crates/envoy-http1/src/hcm.rs` to a new function in **`envoy-bin`** (or to a new sibling crate `envoy-router-dispatch` / similar). The route-walk in `envoy-http1::hcm::build_response` runs unchanged; its `BuildOutcome::Proxy` result is the dispatch's input.

**However:** moving the dispatch site is a significant restructure. **Pragmatic alternative B (preferred):** keep the dispatch in `crates/envoy-http1/src/hcm.rs` AND add `envoy-http2` as a dep of `envoy-http1`, AND **break the cycle by removing `envoy-http1` from `envoy-http2`'s `[dependencies]`**. Per 05.2's `crates/envoy-http2/Cargo.toml:21`, `envoy-http1` is a `[dependencies]` entry. Re-checking what `envoy-http2` actually consumes from `envoy-http1`: per `crates/envoy-http2/src/hcm.rs`, it imports `envoy_http1::HCMConfig as Http1HCMConfig` + `envoy_http1::hcm::build_response` + the `BuildOutcome` enum.

These are core symbols — the cycle break of removing `envoy-http1` from `envoy-http2` is non-trivial and would require duplicating those types. **This is genuinely a SPEC-vs-implementation tension** — SPEC §3 cross-sub-phase architectural rule 2 says "reuse wholesale" but the cycle precludes it.

**RESOLUTION at Task 6 task time:** the planner runs `cargo build` against the proposed dep change to confirm the cycle. If confirmed, the planner stops and **lands ADR-0028** documenting the cycle + the dispatch-hoist choice. The hoist target is **`envoy-bin`** — the dispatch arm becomes a small `envoy_bin::router_dispatch::dispatch_to_upstream` function that (a) takes the `cluster: &ClusterHandle` + `out_req: Request` + `endpoint: SocketAddr` + `host: &str`, (b) matches on `cluster.upstream_protocol()`, (c) calls the appropriate Client. The H1 HCM at `envoy-http1::hcm::serve_connection` calls this dispatch via a function-pointer / closure parameter passed at `HCMConfig` build time. Same shape as Task 7's symmetric dispatch at `envoy-http2::hcm`.

**For this PLAN's scope:** Step 6.2 lands the `envoy-http2` dep in `envoy-http1/Cargo.toml` ONLY IF the cycle does NOT exist (verify via `cargo build -p envoy-http1`). If the cycle exists, Task 6 escalates to: (a) write ADR-0028 documenting the cycle + chosen dispatch-hoist target; (b) restructure per the chosen target (recommendation: hoist to `envoy-bin`); (c) proceed with the dispatch lambdas threaded through `HCMConfig`. This restructure may grow Task 6 to ~200 LoC; record as deviation in PROGRESS Task 6.

**Cycle-existence check at Step 6.2:**

Run: `grep -n 'envoy-http1' crates/envoy-http2/Cargo.toml`
Expected: line 21: `envoy-http1 = { path = "../envoy-http1" }`. **The cycle exists at HEAD.**

**Decision:** Task 6 proceeds with the dispatch-hoist to `envoy-bin`. The `envoy-http1` HCM gains a function-pointer field on `HCMConfig` (`upstream_dispatch: Arc<dyn UpstreamDispatch + Send + Sync>` — a trait object); `envoy-bin`'s startup wires the dispatch object that knows how to call both `envoy_http1::Client` and `envoy_http2::Client`; the `BuildOutcome::Proxy` arm at `crates/envoy-http1/src/hcm.rs` calls `config.upstream_dispatch.dispatch(...)`. This restructure is RECORDED IN ADR-0028.

(Step 6.2's exact restructuring details are deferred to task-time per D-3.5; the planner appends ADR-0028 inline at Task 6 commit. The PLAN's remaining steps proceed assuming the dispatch-hoist; the H2-side mirror at Task 7 follows the same trait-object pattern.)

**Trait-object skeleton** (lands at Task 6; consumed by both Tasks 6 and 7):

```rust
// New module: crates/envoy-http1/src/upstream_dispatch.rs
//
// Or: a sibling crate envoy-router-core that defines the trait + the
// envoy_http1::codec::{Request,Response} value types. The planner picks at
// task time; recommendation is the sibling crate (cleanest break of the
// cycle; envoy-http1 and envoy-http2 both path-dep envoy-router-core).

use crate::codec::Request;
use crate::response::Response;

#[async_trait::async_trait]
pub trait UpstreamDispatch: Send + Sync {
    async fn dispatch(
        &self,
        cluster_name: &str,
        endpoint: std::net::SocketAddr,
        host: &str,
        request: Request,
    ) -> Result<Response, std::io::Error>; // generic error wrapper
}
```

If the planner recommends NOT introducing `async-trait` per phase 02.2's no-async-trait posture, an alternative is `BoxFuture` + a fn-style trait — same shape as `envoy_listener::ConnectionHandler`. The planner picks at task time; recommendation: the BoxFuture posture for consistency with `ConnectionHandler`.

**Implementation in `envoy-bin` (NEW module):**

```rust
// crates/envoy-bin/src/router_dispatch.rs

use envoy_cluster::UpstreamProtocol;
use envoy_http1::codec::Request;
use envoy_http1::response::Response;
use envoy_http1::upstream_dispatch::UpstreamDispatch;

pub struct ProtocolAwareDispatch {
    pub cluster_mgr: std::sync::Arc<envoy_cluster::ClusterManager>,
}

impl UpstreamDispatch for ProtocolAwareDispatch {
    fn dispatch<'a>(
        &'a self,
        cluster_name: &'a str,
        endpoint: std::net::SocketAddr,
        host: &'a str,
        request: Request,
    ) -> futures::future::BoxFuture<'a, Result<Response, std::io::Error>> {
        Box::pin(async move {
            let cluster = self.cluster_mgr.get(cluster_name).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("cluster {cluster_name} not found"),
                )
            })?;
            match cluster.upstream_protocol() {
                UpstreamProtocol::Http1 => {
                    let mut s = envoy_http1::Client::connect(endpoint, host)
                        .await
                        .map_err(|e| std::io::Error::other(format!("{e}")))?;
                    s.send_request(request)
                        .await
                        .map_err(|e| std::io::Error::other(format!("{e}")))
                }
                UpstreamProtocol::Http2 => {
                    let mut s = envoy_http2::Client::connect(endpoint, host)
                        .await
                        .map_err(|e| std::io::Error::other(format!("{e}")))?;
                    s.send_request(request)
                        .await
                        .map_err(|e| std::io::Error::other(format!("{e}")))
                }
            }
        })
    }
}
```

**Wire-up at `envoy-bin/src/main.rs`:** when the HCM is constructed, wrap the cluster_mgr in `ProtocolAwareDispatch` and pass to `HCMConfig::with_upstream_dispatch(...)` (or via a constructor parameter). The `Arc<dyn UpstreamDispatch>` is threaded through to the HCM's connection handler and consumed at `BuildOutcome::Proxy`.

This restructure is GENUINELY non-trivial. The PLAN's LoC estimate for Task 6 grows from ~100 to ~250 LoC. Task 6's commit includes ADR-0028.

**Alternative simpler resolution (CONSIDER):** can the existing tight `envoy-http1` ↔ `envoy-http2` coupling stay AS-IS and only the **dispatch consumer** (the H2 side, not the H1 side) be the one that depends on the other? 

Specifically: at HEAD, `envoy-http2` consumes `envoy-http1` (for HCMConfig + build_response). The H2-side dispatch at Task 7 (replacing the 502 stub at `crates/envoy-http2/src/hcm.rs:117-134`) can directly use `envoy_http1::Client` AND `envoy_http2::Client` — `envoy-http2` already has `envoy-http1` in its deps; `envoy-http2` IS itself, so calling `crate::Client` for the H2 case is fine. **The H1-side dispatch at Task 6** is the problem: `envoy-http1` cannot consume `envoy-http2` without breaking the cycle.

**Smart resolution:** **invert the SPEC's H1-side dispatch site choice**. Instead of putting the H1-or-H2 dispatch inside `envoy-http1::hcm`, put it in `envoy-bin` AT the listener-walk, OR in `envoy-http2::hcm` AS the only HCM that does the dispatch — and have the H1 HCM at `envoy-http1::hcm` continue with the existing 04.3 H1-only dispatch. Then for H1-listener-with-H2-cluster combinations, the **H1 listener's HCM can ONLY proxy to H1 clusters**.

But that contradicts SPEC §3 D4 + §6 local signpost 15 + the fixture 0010 design (which is H2 listener + H2 cluster).

**Wait — fixture 0010 is H2 LISTENER + H2 cluster.** It does NOT exercise H1-listener-with-H2-cluster. Per the SPEC §3 D4: "Pseudocode (the planner cross-checks the live shape at 05.3 Task 1): At crates/envoy-http1/src/hcm.rs (existing serve_connection's BuildOutcome::Proxy arm)..." but per parent §4 "Cross-protocol H2↔H1 translation in the framing-translation sense" is deferred.

**HOWEVER:** the SPEC §3 D4 also says: "The router H2-arm dispatch ALSO lands at the symmetric site inside `crates/envoy-http2/src/hcm.rs` ... 05.3 replaces that 502 stub with the same H1-or-H2 dispatch keyed on `cluster.upstream_protocol`". So the SPEC genuinely wants symmetric dispatch on BOTH sides.

**Final final decision** at Task 6: use option (a) the dispatch-hoist via a trait + `envoy-bin` implementing it. Land ADR-0028 inline. Recommendation: **do this restructure carefully at task time**; if the restructure scope grows beyond 300 LoC, **re-evaluate** whether the SPEC §3 D4 H1-listener-side dispatch can defer to a later phase (with only the H2-listener-side dispatch in 05.3, since fixture 0010 only exercises H2-listener-side).

**For this PLAN:** the steps below proceed assuming the trait-object hoist. If the restructure proves too invasive at task time, the planner records a SPEC-deviation in PROGRESS Task 6 and pares scope to **H2-listener-side dispatch only** (fixture 0010 still passes). Either way, ADR-0028 lands documenting the cycle resolution.

- [ ] **Step 6.3: Verify the cycle and decide the resolution path.**

Run: `cargo tree -p envoy-http2 | grep envoy-http1`
Expected: confirms `envoy-http2` depends on `envoy-http1`.

Run: `grep -n 'envoy-http2\|envoy-http1' crates/envoy-http1/Cargo.toml crates/envoy-http2/Cargo.toml`
Expected: `crates/envoy-http2/Cargo.toml:21:envoy-http1 = { path = "../envoy-http1" }`; **no** `envoy-http2` in `crates/envoy-http1/Cargo.toml`.

Decision recorded in PROGRESS Task 6: `envoy-http1` ↔ `envoy-http2` cycle prevents the SPEC's projected `crates/envoy-http1/src/hcm.rs` direct-use of `envoy_http2::Client`. Resolution: either (A) trait-object hoist via `envoy-bin` (+ ADR-0028), OR (B) defer H1-listener-side dispatch and ship only H2-listener-side dispatch in 05.3 (+ ADR-0028 documenting the deferral). Pick at task time — recommendation (A) if the restructure fits ≤200 LoC; otherwise (B).

- [ ] **Step 6.4: Implement the chosen resolution.**

Per Step 6.3's decision, follow option (A) or (B). The remaining Step 6.5-6.10 details below assume option (A); if option (B) is chosen, Task 6 ships ONLY the trait + an unused-at-H1 stub, and the H2-side replacement of the 502 stub at Task 7 continues to do the H1-or-H2 dispatch internally (no inter-crate cycle since `envoy-http2` already consumes `envoy-http1`).

For this PLAN's projected steps: **option (A) implementation** lands the trait + the `envoy-bin`-side wiring at Task 6, AND a small dispatch call at the H1 HCM's `BuildOutcome::Proxy` arm. The `BuildOutcome::Proxy` arm (lines 209-303) is restructured to call `config.upstream_dispatch.dispatch(...)` instead of the inline `Client::connect + send_request` pair.

The detailed code edits + tests for option (A) are deferred to task time — too dependent on the actual restructure shape — but the SPEC §3 D4 acceptance criteria (fixture 0010 passes through the dispatched H2 path) is the binding outcome regardless of (A) vs (B).

- [ ] **Step 6.5: Write 3 unit tests for the dispatch arms.**

Append to `crates/envoy-http1/src/hcm.rs::tests` (or to the new `crates/envoy-bin/tests/router_dispatch.rs` integration test if option (A) is chosen and the dispatch lives in `envoy-bin`):

```rust
    // Conditional: include only if option (A) chosen and the dispatch is
    // testable from envoy-http1's test surface via a mock UpstreamDispatch.

    #[tokio::test(flavor = "multi_thread")]
    async fn proxy_arm_dispatches_h1_for_http1_cluster() {
        // Cluster with upstream_protocol: Http1 (default). Mock dispatch
        // returns a 200 from a captured-call. Assert downstream sees 200.
        // Implementation: per the chosen UpstreamDispatch trait shape;
        // exact code at task time.
        unimplemented!("Step 6.5 test — exact body at task time per chosen trait shape");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxy_arm_dispatches_h2_for_http2_cluster() {
        // Cluster with upstream_protocol: Http2. Mock UpstreamDispatch routes
        // to an in-process h2 server returning 200 with body "h2". Assert
        // the downstream HTTP/1.1 response carries body "h2" — the upstream-
        // direction H2 framing is invisible downstream because
        // write_proxied_response reads the protocol-agnostic envoy Response
        // value type.
        unimplemented!("Step 6.5 test — exact body at task time per chosen trait shape");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxy_arm_returns_502_on_h2_upstream_connect_refused() {
        // Cluster with upstream_protocol: Http2 whose endpoint is an unbound
        // port. Mock UpstreamDispatch returns a synthesized error. Assert the
        // downstream-bound response is 502 (mirrors 04.3's H1 502 fallback).
        unimplemented!("Step 6.5 test — exact body at task time per chosen trait shape");
    }
```

**Note:** the bodies are deferred to task time because they depend on the chosen `UpstreamDispatch` trait shape. The tests' high-level assertions are stable; the wire-up to the trait is task-time.

- [ ] **Step 6.6: Run the tests + clippy + fmt + build.**

Run: `cargo test -p envoy-http1 -- --nocapture`
Run: `cargo test -p envoy-bin -- --nocapture` (if option (A) lands a `crates/envoy-bin/tests/router_dispatch.rs`)
Run: `cargo build --workspace --all-targets`
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Run: `cargo fmt --all -- --check`
Expected: all clean; new tests pass.

- [ ] **Step 6.7: Append ADR-0028 to `docs/envoy-rust/DECISIONS.md`.**

Append after the existing ADR-0027 block. The ADR-0028 block carries `Date / Status / Context / Options-considered / Decision / Rationale / Consequences / Provenance` per the established 27-block precedent. Provenance footer notes the cycle was unanticipated at SPEC writeup (parent-05 state-2 commit `f1804a7`); chose (A) the trait-object hoist via envoy-bin or (B) the H1-listener-side deferral, per task-time judgment.

```markdown

---

## ADR-0028: Resolution of the `envoy-http1` ↔ `envoy-http2` cycle introduced by SPEC §3 D4 router dispatch

- **Date:** 2026-MM-DD (the date 05.3 Task 6 lands).
- **Status:** accepted.
- **Context:** Phase 05.3 SPEC §3 D4 projects the router H2-arm dispatch at `crates/envoy-http1/src/hcm.rs`'s `BuildOutcome::Proxy` arm — the dispatch site needs to call both `envoy_http1::Client` (for H1 clusters) and `envoy_http2::Client` (for H2 clusters). At HEAD `f33dac9`, `envoy-http2` already path-deps `envoy-http1` (for `HCMConfig` + `build_response` per SPEC §3 cross-sub-phase architectural rule 2). Adding `envoy-http2` as a path-dep of `envoy-http1` would create a circular dep that Cargo rejects.
- **Options considered:**
  - (A) **Trait-object hoist via `envoy-bin`.** Define a trait `UpstreamDispatch` in `crates/envoy-http1/src/upstream_dispatch.rs` (or a new `envoy-router-core` crate); have `envoy-bin` wire a `ProtocolAwareDispatch` impl that knows how to call both Clients; thread `Arc<dyn UpstreamDispatch>` through `HCMConfig` to the `BuildOutcome::Proxy` arm.
  - (B) **Defer the H1-listener-side dispatch.** Ship only the H2-listener-side dispatch at Task 7 (which lives inside `envoy-http2` and can call both `envoy_http1::Client` and `envoy_http2::Client` without cycle). Fixture 0010 (H2 listener + H2 cluster) still passes. The H1-listener-with-H2-cluster path defers to a later phase that lands the dispatch hoist or restructures the crate graph.
  - (C) **Hoist `Client` + `ClientStream` out of `envoy-http2` into a new `envoy-http-client` crate** depended on by both `envoy-http1` and `envoy-http2`. Cleanest break but largest restructure (~400 LoC moved across 3 crates).
- **Decision:** [Pick at task time. Recommendation: (A) if restructure fits ≤200 LoC. (B) otherwise — defer H1-listener-side dispatch to a later phase. (C) only if the cycle proves intractable in (A) and (B) is unacceptable.]
- **Rationale:** [Fill at task time per chosen option.]
- **Consequences:** [Fill at task time. (A): `HCMConfig` gains an `upstream_dispatch: Arc<dyn UpstreamDispatch + Send + Sync>` field; existing constructors require a builder change. (B): SPEC §3 D4 partial; H1-listener-side dispatch deferred. (C): `envoy-http-client` crate scaffold lands.]
- **Provenance:** unanticipated at parent-05 SPEC + 05.3 SPEC writeup (commit `f1804a7`). Cycle surfaced at 05.3 Task 6 task time when adding `envoy-http2` to `crates/envoy-http1/Cargo.toml`. Per D-3.5 (decisions are written, not remembered), the chosen resolution lands as ADR-0028 alongside Task 6's commit.
```

- [ ] **Step 6.8: Commit Task 6.**

```bash
git add crates/envoy-http1/Cargo.toml \
        crates/envoy-http1/src/hcm.rs \
        crates/envoy-http1/src/upstream_dispatch.rs   # if option (A) lands the new module
        crates/envoy-bin/src/main.rs \
        crates/envoy-bin/src/router_dispatch.rs       # if option (A)
        crates/envoy-bin/Cargo.toml \
        docs/envoy-rust/DECISIONS.md \
        docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 05.3: router H1-side dispatch + ADR-0028 (task 6)

SPEC §3 D4 H1-side: extend crates/envoy-http1/src/hcm.rs's
BuildOutcome::Proxy arm to dispatch H1-or-H2 by cluster.upstream_protocol().

The SPEC's projected direct-use of envoy_http2::Client at
crates/envoy-http1/src/hcm.rs is precluded by the existing
envoy-http2 → envoy-http1 path-dep (per crates/envoy-http2/Cargo.toml:21
from 05.2 Task 1). ADR-0028 documents the cycle + the chosen resolution.

Resolution: [(A) trait-object hoist via envoy-bin / (B) H1-listener-side
deferral / (C) envoy-http-client crate hoist — pick at task time].

[Bodies of the 3 dispatch tests, ADR-0028 prose, and the actual
restructure code at task time per the chosen option.]

3 new unit tests cover: H1-cluster dispatches via H1 path; H2-cluster
dispatches via H2 path; 502 fallback on H2 upstream connect refused.

Cargo dep graph adjusted per chosen option. No new top-level Cargo deps
on the third-party side.
EOF
)"
```

---

## Task 7 — Symmetric H1-or-H2 dispatch at `crates/envoy-http2/src/hcm.rs` (replace 05.2's 502 stub) — closes M8 structurally

**Files:**

- Modify: `crates/envoy-http2/src/hcm.rs` — replace the 05.2-landed 502 stub at lines 117-134 with the symmetric H1-or-H2 dispatch on `cluster.upstream_protocol()`. Use the trait-object dispatch from Task 6 (option A) OR call `envoy_http1::Client` and `crate::Client` directly (option B — `envoy-http2` already consumes both). Rename the 05.2 D3 test 6 `h2_proxy_outcome_returns_502_in_05_2` → `h2_proxy_outcome_dispatches_to_upstream` and flip the assertion from 502 to 200. Append 1 additional unit test for the H1-cluster-from-H2-listener case. Closes 05.2 REVIEW M8 structurally (the stub body literal `b"upstream H2 not yet wired (sub-phase 05.3)\n"` disappears).

**Estimated LoC:** ~60 (~30 LoC dispatch wrap + ~30 LoC for renamed/flipped + 1 new test).

**Signposts settled:**

- SPEC §3 D4 (symmetric dispatch): replaces the 05.2 502 stub with the H1-or-H2 dispatch keyed on `cluster.upstream_protocol`. The 05.2 test 6 rename + assertion-flip per 05.2 SPEC §3 D3 test 6's projection.
- SPEC §6 local signpost 27: "The H2 listener-side `Proxy` stub at `envoy-http2/src/hcm.rs` MUST be replaced."
- SPEC §1 paragraph 8 (M8 closes structurally): the stub body literal disappears.

- [ ] **Step 7.1: Cross-check the existing 502 stub.**

Run: `grep -nB 2 -A 20 'BuildOutcome::Proxy { \.\. }' crates/envoy-http2/src/hcm.rs`
Expected: confirms the stub at lines 117-134:
```rust
        BuildOutcome::Proxy { .. } => {
            // 05.2 STUB: ...
            tracing::warn!(...);
            Response { status: 502, ..., body: Bytes::from_static(b"upstream H2 not yet wired (sub-phase 05.3)\n") }
        }
```

Run: `grep -nB 2 -A 30 'h2_proxy_outcome_returns_502_in_05_2' crates/envoy-http2/src/hcm.rs`
Expected: confirms the test at the line range under `mod tests`.

- [ ] **Step 7.2: Write the dispatch replacement code (depends on Task 6's chosen option).**

If Task 6 chose (A) trait-object hoist: the H2-side HCM accepts the same `Arc<dyn UpstreamDispatch>` (passed through HCMConfig); the `BuildOutcome::Proxy` arm calls `config.upstream_dispatch.dispatch(cluster_name, endpoint, host, request)`. Code:

```rust
        BuildOutcome::Proxy { cluster: cluster_name } => {
            // Validator ensures every cluster name referenced from a
            // RouteAction::Route exists in the bootstrap; the .expect() is
            // defense-in-depth (mirrors envoy-http1/src/hcm.rs:215-218).
            let cluster = cluster_mgr
                .get(&cluster_name)
                .expect("validator ensures cluster present");
            let endpoint = match cluster.pick_endpoint() {
                Some(e) => e,
                None => {
                    tracing::warn!(cluster = %cluster.name(), "no healthy endpoint — emitting 502");
                    return synth_h2_502();
                }
            };
            let start = std::time::Instant::now();
            let upstream_resp = match config.upstream_dispatch.dispatch(
                &cluster_name,
                endpoint,
                &host_header,
                out_req,
            ).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(?e, "H2 listener: upstream dispatch failed; emitting 502");
                    return synth_h2_502();
                }
            };
            let elapsed_ms = start.elapsed().as_millis();
            // Append x-envoy-upstream-service-time per parent §6 signpost 10.
            let mut headers = upstream_resp.headers;
            headers.push(("x-envoy-upstream-service-time".to_string(), elapsed_ms.to_string()));
            Response {
                status: upstream_resp.status,
                reason: upstream_resp.reason,
                headers,
                body: upstream_resp.body,
            }
        }
```

If Task 6 chose (B) defer: the H2-side dispatch can call both `envoy_http1::Client` AND `crate::Client` directly (no cycle since `envoy-http2` already consumes `envoy-http1`):

```rust
        BuildOutcome::Proxy { cluster: cluster_name } => {
            let cluster = cluster_mgr.get(&cluster_name).expect("validator");
            let endpoint = match cluster.pick_endpoint() {
                Some(e) => e,
                None => return synth_h2_502(),
            };
            let start = std::time::Instant::now();
            let upstream_resp_result = match cluster.upstream_protocol() {
                envoy_cluster::UpstreamProtocol::Http1 => {
                    match envoy_http1::Client::connect(endpoint, &host_header).await {
                        Ok(mut s) => s.send_request(out_req).await.map_err(|e| format!("{e}")),
                        Err(e) => Err(format!("{e}")),
                    }
                }
                envoy_cluster::UpstreamProtocol::Http2 => {
                    match crate::Client::connect(endpoint, &host_header).await {
                        Ok(mut s) => s.send_request(out_req).await.map_err(|e| format!("{e}")),
                        Err(e) => Err(format!("{e}")),
                    }
                }
            };
            let upstream_resp = match upstream_resp_result {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(error = %e, "H2 listener: upstream dispatch failed");
                    return synth_h2_502();
                }
            };
            let elapsed_ms = start.elapsed().as_millis();
            let mut headers = upstream_resp.headers;
            headers.push(("x-envoy-upstream-service-time".to_string(), elapsed_ms.to_string()));
            Response { status: upstream_resp.status, reason: upstream_resp.reason, headers, body: upstream_resp.body }
        }
```

The exact arm shape depends on the surrounding `serve_one_stream` body — locate via `grep -nB 5 -A 50 'BuildOutcome::Proxy { \.\. }' crates/envoy-http2/src/hcm.rs` at task time.

The `synth_h2_502` helper produces a 502 Response without the 05.2-stub's body literal — replace with empty body or a generic `b"Bad Gateway"`. Mirrors envoy-http1's `synth_status(502, close)` shape:

```rust
fn synth_h2_502() -> Response {
    Response {
        status: 502,
        reason: None,
        headers: vec![
            ("server".to_string(), "envoy-rust".to_string()),
            ("content-type".to_string(), "text/plain".to_string()),
        ],
        body: Bytes::from_static(b""),
    }
}
```

- [ ] **Step 7.3: Rename + flip the 05.2 D3 test 6.**

Locate the existing test:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_proxy_outcome_returns_502_in_05_2() {
        // Pre-Task-7: asserts a 502 Bad Gateway response from the H2 listener
        // when the route resolves to BuildOutcome::Proxy.
    }
```

Rename to `h2_proxy_outcome_dispatches_to_upstream` and flip the assertion. The test now spawns an in-process h2 server upstream that returns 200 with body "h2-upstream-ok"; spawns the envoy-http2 HCM with a single-cluster ClusterManager whose cluster has `upstream_protocol == Http2` and `endpoints == [in-process h2 server addr]`; drives an h2 client request through the HCM; asserts the response is 200 with body "h2-upstream-ok".

Implementation skeleton (exact at task time):

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_proxy_outcome_dispatches_to_upstream() {
        // Spawn upstream h2 server (helper from Task 2 client.rs::tests, OR
        // re-extract for envoy-http2's hcm.rs tests via a shared test_helpers module).
        let (upstream_addr, _captured, _upstream_handle) = /* spawn_h2_server returning 200 "h2-upstream-ok" */;
        // Build single-cluster ClusterManager with upstream_protocol: Http2.
        let cluster_mgr = build_h2_cluster_mgr_with_upstream(upstream_addr);
        // Spawn envoy-http2 HCM listener; route every request to the cluster.
        let (listener_addr, _hcm_server) = spawn_h2_hcm(synth_h2_hcm_config_proxy(cluster_mgr)).await;
        // Drive a downstream h2 client request.
        let tcp = tokio::net::TcpStream::connect(listener_addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move { let _ = conn.await; });
        let req = http::Request::builder().method("GET").uri("http://test.example/").body(()).unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        // Drain body and assert.
        let (_parts, mut body) = resp.into_parts();
        let mut body_bytes = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            body_bytes.extend_from_slice(&chunk);
            let _ = body.flow_control().release_capacity(chunk.len());
        }
        assert_eq!(body_bytes.as_ref(), &b"h2-upstream-ok"[..]);
    }
```

The supporting helpers `build_h2_cluster_mgr_with_upstream` + `synth_h2_hcm_config_proxy` extend the existing 05.2 `synth_h2_hcm_config` helper at line 167 area to (a) include a non-empty cluster_mgr and (b) include a route resolving to `RouteAction::Route` instead of `DirectResponse`. Patterns adapted from `tests/differential` + 05.2 D3's hcm.rs test surface; exact code at task time.

- [ ] **Step 7.4: Append the additional H1-cluster-from-H2-listener test.**

Append to `crates/envoy-http2/src/hcm.rs::tests`:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_proxy_outcome_dispatches_to_h1_upstream_when_cluster_is_http1() {
        // Per SPEC §3 D4 test 5: H2 listener-side HCM with a cluster of
        // upstream_protocol: Http1 dispatches via envoy_http1::Client.
        // The HCM core operates on the protocol-agnostic Request/Response
        // value types, so this is structurally automatic.
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        // Spawn a minimal H1 server that returns 200 "h1-from-h2-listener".
        let _upstream_handle = tokio::spawn(async move {
            if let Ok((mut tcp, _)) = upstream_listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 4096];
                let _ = tcp.read(&mut buf).await;
                let _ = tcp
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 21\r\n\r\nh1-from-h2-listener\r\n")
                    .await;
                let _ = tcp.shutdown().await;
            }
        });
        let cluster_mgr = build_cluster_mgr_with_upstream(upstream_addr, envoy_cluster::UpstreamProtocol::Http1);
        let (listener_addr, _hcm) = spawn_h2_hcm(synth_h2_hcm_config_proxy(cluster_mgr)).await;
        let tcp = tokio::net::TcpStream::connect(listener_addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move { let _ = conn.await; });
        let req = http::Request::builder().method("GET").uri("http://test.example/").body(()).unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        // Body equality is best-effort here (h1-echo-server isn't used; the
        // ad-hoc H1 response above frames "h1-from-h2-listener" + CRLF).
    }
```

- [ ] **Step 7.5: Run the tests + workspace sanity checks.**

Run: `cargo test -p envoy-http2 -- --nocapture`
Expected: all tests pass. The renamed `h2_proxy_outcome_dispatches_to_upstream` PASSES (returns 200); the 05.2 `h2_proxy_outcome_returns_502_in_05_2` no longer exists.

Run: `cargo build --workspace --all-targets && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: all clean.

- [ ] **Step 7.6: Commit Task 7.**

```bash
git add crates/envoy-http2/src/hcm.rs \
        docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 05.3: H2 listener-side router dispatch — replace 05.2 502 stub (task 7)

SPEC §3 D4 H2-side: replaces the 05.2-landed 502 stub at
crates/envoy-http2/src/hcm.rs:117-134 with the symmetric H1-or-H2
dispatch keyed on cluster.upstream_protocol(). The dispatch shape
[mirrors Task 6's chosen option (A) trait-object call OR (B) direct
calls to envoy_http1::Client + crate::Client — envoy-http2 already
consumes envoy-http1 so direct calls are cycle-free here].

The 05.2 SPEC §3 D3 test 6 h2_proxy_outcome_returns_502_in_05_2 is
renamed to h2_proxy_outcome_dispatches_to_upstream and the assertion
flipped from 502 to 200, per 05.2 SPEC §3 D3 test 6's projection. One
additional test exercises the H1-cluster-from-H2-listener case (per
SPEC §3 D4 test 5).

Closes 05.2 REVIEW M8 structurally — the 502 stub body literal
b"upstream H2 not yet wired (sub-phase 05.3)\n" disappears.

x-envoy-upstream-service-time injection lands inline at the H2-side
dispatch with the same Instant::now()-at-connect / start.elapsed()-
after-send_request measurement window as the H1-side per parent §6
signpost 10. Mirrors the H1 dispatch behavior at task 6.

No new top-level Cargo deps. No ADRs landed at this task.
EOF
)"
```

---


## Task 8 — `tests/helpers/http2-echo-server/` workspace member + `crates/envoy-http2/src/codec.rs::server_handshake` thin wrapper

**Files:**

- Create: `tests/helpers/http2-echo-server/Cargo.toml`.
- Create: `tests/helpers/http2-echo-server/src/main.rs` (with `#![forbid(unsafe_code)]`).
- Modify: `crates/envoy-http2/src/codec.rs` — append `pub fn server_handshake` thin wrapper enabling the helper to consume `envoy_http2` instead of `h2` directly per parent §6 signpost 7.
- Modify: `Cargo.toml` (root) — `[workspace] members` gains `tests/helpers/http2-echo-server`.
- Modify: `Cargo.lock` — synced inline (`cargo build -p http2-echo-server`).

**Estimated LoC:** ~330 (impl ~250: argv parser ~30 + connection-accept loop + per-stream task + deterministic-body construction ~220; 5 unit tests ~60; Cargo.toml ~20).

**Signposts settled:**

- SPEC §3 D5: helper consumes `envoy_http2` (NOT `h2` directly) per parent §6 signpost 7.
- SPEC §3 D5 codec-edge thin wrapper: `pub fn server_handshake` on `envoy_http2::codec` re-exports `h2::server::Builder::handshake` adequately for the helper.
- SPEC §6 inherited signpost 6 (Cargo.lock cadence — inline-at-scaffold per phase precedent): the new workspace member registers; near-no-op diff anticipated.
- SPEC §6 inherited signpost 9 (`anyhow` boundary): helper uses `anyhow` per the precedent set by 02.1's `tcp-echo-server`, 03.2's `tls-echo-server`, and 04.3's `http1-echo-server`.
- SPEC §6 inherited signpost 11 (existing 04.x fixture YAMLs precedent — N/A here, helper-only).

- [ ] **Step 8.1: Cross-check the existing `http1-echo-server` shape.**

Run: `wc -l tests/helpers/http1-echo-server/src/main.rs tests/helpers/http1-echo-server/Cargo.toml`
Expected: 385 + 19 lines (per HEAD `f33dac9`).

Run: `grep -nE 'fn parse_argv|fn print_help|fn print_version|fn run|async fn handle_connection|fn make_response|sort_by' tests/helpers/http1-echo-server/src/main.rs`
Expected: confirms the layout (argv parser at lines ~42-90; print_help around 88; run() around 97; handle_connection further down). Mirror this shape for `http2-echo-server`.

- [ ] **Step 8.2: Add `server_handshake` to `crates/envoy-http2/src/codec.rs`.**

Edit `crates/envoy-http2/src/codec.rs`. Append after the existing `build_h2_server` function and before the `#[cfg(test)] mod tests {` block:

```rust
/// Thin wrapper around `h2::server::handshake`. Used by external test
/// helpers (`tests/helpers/http2-echo-server/`) so they can consume
/// `envoy_http2` instead of `h2` directly per parent-05 SPEC §6 signpost 7
/// (mirrors 04.3's `http1-echo-server` consuming `envoy_http1` over direct
/// `httparse`). Production code uses `build_h2_server` + `Builder::handshake`
/// directly; this re-export only exists to satisfy the architectural rule
/// that only `envoy-http2` depends on `h2` workspace-wide.
pub async fn server_handshake(
    tcp: tokio::net::TcpStream,
) -> Result<h2::server::Connection<tokio::net::TcpStream, bytes::Bytes>, crate::Http2Error> {
    h2::server::handshake(tcp)
        .await
        .map_err(|source| crate::Http2Error::H2Handshake { source })
}
```

The return type re-exposes `h2::server::Connection` directly per the SPEC's note ("re-export `h2::server::Builder::handshake` adequately for the helper's needs without forcing the HCM machinery"). The helper at `http2-echo-server/src/main.rs` consumes the returned `Connection` to call `.accept()` per-stream. **This is a deliberate thin re-export, NOT a full type-encapsulation:** the helper's per-stream loop reads `(http::Request<h2::RecvStream>, h2::server::SendResponse<bytes::Bytes>)` pairs from `Connection::accept`. The h2 types leak via the return value because the helper's loop is mechanical and doesn't warrant a full custom-typed wrapper. Architectural rule 1 is satisfied at the `Cargo.toml` level: `tests/helpers/http2-echo-server/Cargo.toml` declares `envoy-http2` as the only HTTP-related dep; `h2` types reach the helper only through `envoy_http2`'s public API.

- [ ] **Step 8.3: Verify the new function compiles.**

Run: `cargo build -p envoy-http2`
Expected: clean.

Append a unit test to `crates/envoy-http2/src/codec.rs::tests`:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn server_handshake_accepts_h2_connection() {
        // End-to-end smoke: spawn a 127.0.0.1 listener, do a parallel
        // h2::client::handshake from a separate task, assert the server-side
        // server_handshake returns Ok.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            server_handshake(tcp).await
        });
        let client_task = tokio::spawn(async move {
            let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
            h2::client::handshake(tcp).await
        });
        let (server_result, client_result) = tokio::join!(server_task, client_task);
        assert!(server_result.unwrap().is_ok());
        assert!(client_result.unwrap().is_ok());
    }
```

Run: `cargo test -p envoy-http2 --lib codec -- --nocapture`
Expected: PASS.

- [ ] **Step 8.4: Create `tests/helpers/http2-echo-server/Cargo.toml`.**

```toml
[package]
name = "http2-echo-server"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[[bin]]
name = "http2-echo-server"
path = "src/main.rs"

[dependencies]
envoy-http2 = { path = "../../../crates/envoy-http2" }
anyhow = "1"
bytes = "1"
http = "1"
thiserror = "2"
tokio = { version = "1", features = ["rt-multi-thread", "net", "io-util", "macros", "signal", "time", "sync"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }

[dependencies.h2]
# Re-exported by envoy_http2::codec::server_handshake; the helper's accept
# loop consumes h2::server::Connection / h2::server::SendResponse /
# h2::SendStream / h2::RecvStream via the returned Connection. This is the
# documented helper-side h2-direct surface per parent §6 signpost 7's
# "h2 types leak via the return value" caveat — same shape as
# tests/differential's drive_http2 carve-out.
version = "0.4"
```

**Note on the `h2` direct dep:** the helper consumes `h2::server::Connection`'s methods (`.accept()`) and the per-stream `(http::Request<RecvStream>, SendResponse<Bytes>)` pair. Even with `envoy_http2::codec::server_handshake` returning the Connection, calling `.accept()` on it requires `h2` to be in scope at the helper. **This is the documented carve-out** parallel to `tests/differential/Cargo.toml`'s `h2 = "0.4"` for `drive_http2` (per 05.2 D5.b + parent §6 signpost 8). Per parent §6 signpost 7, the architectural rule is preserved AT THE PRODUCTION-CRATE level — test helpers can carve out per the established precedent.

If the planner prefers to fully eliminate the helper-side `h2` dep, the alternative is to wrap the entire accept-and-respond loop inside `envoy_http2::codec` (or a new `envoy_http2::server` module) — adding ~80 LoC of typed-wrapper code. **Recommendation per task time:** start with the documented carve-out (helper has direct `h2` dep) and only fully encapsulate if the second helper consumer (some future H3 helper) emerges. Mirror the 04.3 / 05.2 precedent where the helper-side carve-out is an accepted pattern.

- [ ] **Step 8.5: Add `tests/helpers/http2-echo-server` to root `Cargo.toml` `[workspace] members`.**

Edit `Cargo.toml` at the workspace root. Add `"tests/helpers/http2-echo-server",` in alphabetic order with the other `tests/helpers/*` entries (between `"tests/helpers/http1-echo-server",` and `"tests/helpers/tcp-echo-server",`).

- [ ] **Step 8.6: Sync Cargo.lock.**

Run: `cargo build -p http2-echo-server`
Expected: clean. Cargo.lock gains a `http2-echo-server` entry. No new top-level deps register (all already in workspace).

- [ ] **Step 8.7: Create `tests/helpers/http2-echo-server/src/main.rs`.**

```rust
#![forbid(unsafe_code)]

//! `http2-echo-server` — minimal HTTP/2 cleartext (H2C) echo server for the
//! envoy-rust differential harness. Sibling of `tcp-echo-server` (phase 02.1),
//! `tls-echo-server` (phase 03.2), and `http1-echo-server` (phase 04.3).
//! Plaintext H2C only — no TLS.
//!
//! Per parent-05 SPEC §6 signpost 7: the helper consumes `envoy_http2` (NOT
//! `h2` directly for the handshake). The accept loop's per-stream surface
//! still reaches `h2::server::Connection` types (via the wrapper's return
//! value); this carve-out is the documented helper-side direct-surface
//! parallel to `tests/differential`'s `drive_http2` consumption.
//!
//! The deterministic-echo response body shape is LOAD-BEARING for differential
//! equivalence (per SPEC §3 D5): the helper produces a `200 OK` response with
//! `content-type: text/plain` and a body of:
//!
//! ```text
//! method: <METHOD>
//! path: <PATH>
//! headers:
//!   <name1>: <value1>     (alphabetically sorted by lowercase name)
//!   <name2>: <value2>
//!   ...
//! body: <BODY>
//! ```
//!
//! Both proxies forward the same logical request; the alphabetic header sort
//! eliminates ordering divergences from differential body comparison. Mirrors
//! `http1-echo-server`'s body shape exactly so cross-protocol fixtures (if a
//! later phase ships them) remain comparable.

use std::process::ExitCode;

use anyhow::Result;
use bytes::Bytes;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::task::JoinSet;

/// Parsed argv surface. Mirrors `http1-echo-server`'s `Args` shape verbatim.
#[derive(Debug, PartialEq)]
struct Args {
    port: u16,
}

#[derive(Debug, Error, PartialEq)]
enum ArgvError {
    #[error("required flag {0} missing")]
    MissingFlag(&'static str),
    #[error("flag expects a value")]
    MissingValue,
    #[error("port value must be a u16")]
    InvalidPort,
    #[error("trailing arguments after --port <u16>")]
    Trailing,
    #[error("--help")]
    HelpRequested,
    #[error("--version")]
    VersionRequested,
}

/// Argv parser. Identical shape to `http1-echo-server::parse_argv` per parent
/// §6 signpost 7's "mirror the established helper posture verbatim".
fn parse_argv(args: &[String]) -> Result<Args, ArgvError> {
    let mut i = 0;
    let mut port: Option<u16> = None;
    while i < args.len() {
        match args[i].as_str() {
            "--help" => return Err(ArgvError::HelpRequested),
            "--version" => return Err(ArgvError::VersionRequested),
            "--port" => {
                let v = args.get(i + 1).ok_or(ArgvError::MissingValue)?;
                port = Some(v.parse().map_err(|_| ArgvError::InvalidPort)?);
                i += 2;
            }
            _ => return Err(ArgvError::Trailing),
        }
    }
    Ok(Args {
        port: port.ok_or(ArgvError::MissingFlag("--port"))?,
    })
}

fn print_help() {
    println!(
        "http2-echo-server: HTTP/2 cleartext echo server helper for the envoy-rust differential harness.\n\
         \n\
         Usage:\n  http2-echo-server --port <u16>\n  \
         http2-echo-server --help\n  http2-echo-server --version"
    );
}

fn print_version() {
    println!("http2-echo-server {}", env!("CARGO_PKG_VERSION"));
}

async fn run(args: Args) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", args.port)).await?;
    tracing::info!("http2-echo-server listening on 0.0.0.0:{}", args.port);

    let mut join_set: JoinSet<()> = JoinSet::new();
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("shutdown signal received");
                break;
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _)) => {
                        join_set.spawn(handle_connection(stream));
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "accept failed; continuing");
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_connection(tcp: tokio::net::TcpStream) {
    let mut conn = match envoy_http2::codec::server_handshake(tcp).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "h2 handshake failed");
            return;
        }
    };
    while let Some(stream_result) = conn.accept().await {
        let (req, mut send_response) = match stream_result {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "h2 stream accept failed");
                return;
            }
        };
        tokio::spawn(async move {
            // Drain the request body bytes (small body assumption).
            let (parts, mut body) = req.into_parts();
            let mut body_bytes = bytes::BytesMut::new();
            while let Some(chunk_result) = body.data().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(error = %e, "h2 body read failed");
                        return;
                    }
                };
                body_bytes.extend_from_slice(&chunk);
                let _ = body.flow_control().release_capacity(chunk.len());
            }
            let response_body = make_response_body(&parts, &body_bytes);
            let response = http::Response::builder()
                .status(200)
                .header("content-type", "text/plain")
                .body(())
                .unwrap();
            let mut send_stream = match send_response.send_response(response, false) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "send_response failed");
                    return;
                }
            };
            if let Err(e) = send_stream.send_data(Bytes::from(response_body), true) {
                tracing::warn!(error = %e, "send_data failed");
            }
        });
    }
}

/// Build the deterministic-echo body. The body shape MUST match
/// `http1-echo-server::make_response`'s body shape exactly so cross-protocol
/// fixtures (if any) remain comparable. The alphabetic header sort is
/// LOAD-BEARING for differential equivalence (both proxies forward the same
/// logical request; the helper's sorted-header response is the byte-exact
/// baseline).
fn make_response_body(parts: &http::request::Parts, body_bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(256 + body_bytes.len());
    out.extend_from_slice(b"method: ");
    out.extend_from_slice(parts.method.as_str().as_bytes());
    out.push(b'\n');
    out.extend_from_slice(b"path: ");
    out.extend_from_slice(parts.uri.path().as_bytes());
    out.push(b'\n');
    out.extend_from_slice(b"headers:\n");
    let mut sorted_headers: Vec<(String, Vec<u8>)> = parts
        .headers
        .iter()
        .map(|(n, v)| (n.as_str().to_lowercase(), v.as_bytes().to_vec()))
        .collect();
    // Add the H2 pseudo-headers explicitly so the body shape includes them
    // (h2 codec strips them from the user-facing HeaderMap).
    sorted_headers.push((":authority".to_string(), parts.uri.authority().map(|a| a.as_str().as_bytes().to_vec()).unwrap_or_default()));
    sorted_headers.push((":method".to_string(), parts.method.as_str().as_bytes().to_vec()));
    sorted_headers.push((":path".to_string(), parts.uri.path().as_bytes().to_vec()));
    sorted_headers.push((":scheme".to_string(), parts.uri.scheme_str().unwrap_or("http").as_bytes().to_vec()));
    sorted_headers.sort_by(|a, b| a.0.cmp(&b.0));
    for (n, v) in &sorted_headers {
        out.extend_from_slice(b"  ");
        out.extend_from_slice(n.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(v);
        out.push(b'\n');
    }
    out.extend_from_slice(b"body: ");
    out.extend_from_slice(body_bytes);
    out
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_argv(&argv) {
        Ok(a) => a,
        Err(ArgvError::HelpRequested) => {
            print_help();
            return ExitCode::SUCCESS;
        }
        Err(ArgvError::VersionRequested) => {
            print_version();
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}");
            print_help();
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to build tokio runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_argv_accepts_port() {
        let args = parse_argv(&["--port".to_string(), "7000".to_string()]).unwrap();
        assert_eq!(args, Args { port: 7000 });
    }

    #[test]
    fn parse_argv_rejects_missing_port() {
        let err = parse_argv(&[]).unwrap_err();
        assert_eq!(err, ArgvError::MissingFlag("--port"));
    }

    #[test]
    fn parse_argv_help_returns_help_requested() {
        let err = parse_argv(&["--help".to_string()]).unwrap_err();
        assert_eq!(err, ArgvError::HelpRequested);
    }

    #[test]
    fn parse_argv_version_returns_version_requested() {
        let err = parse_argv(&["--version".to_string()]).unwrap_err();
        assert_eq!(err, ArgvError::VersionRequested);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn echo_round_trip_against_in_test_h2_client() {
        // Spawn the helper on an ephemeral 127.0.0.1 port; open an h2 client
        // connection; send GET /test with Host: testharness; assert the
        // response body shape.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _server_task = tokio::spawn(async move {
            if let Ok((tcp, _)) = listener.accept().await {
                handle_connection(tcp).await;
            }
        });
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://testharness/test")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        let (_parts, mut body) = resp.into_parts();
        let mut body_bytes = bytes::BytesMut::new();
        while let Some(chunk_result) = body.data().await {
            let chunk = chunk_result.unwrap();
            body_bytes.extend_from_slice(&chunk);
            let _ = body.flow_control().release_capacity(chunk.len());
        }
        let s = std::str::from_utf8(&body_bytes).unwrap();
        assert!(s.starts_with("method: GET\n"), "body shape: {s}");
        assert!(s.contains("path: /test\n"), "body shape: {s}");
        assert!(s.contains(":authority: testharness\n"), "body shape: {s}");
        assert!(s.contains(":scheme: http\n"), "body shape: {s}");
    }
}
```

- [ ] **Step 8.8: Run the helper's tests.**

Run: `cargo test -p http2-echo-server`
Expected: 5 tests pass.

Run: `cargo build -p http2-echo-server --release`
Expected: clean release build (used by harness via `target/release/http2-echo-server`).

- [ ] **Step 8.9: Verify the workspace build + clippy + fmt.**

Run: `cargo build --workspace --all-targets`
Expected: clean.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.

Run: `cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 8.10: Commit Task 8.**

```bash
git add tests/helpers/http2-echo-server/Cargo.toml \
        tests/helpers/http2-echo-server/src/main.rs \
        crates/envoy-http2/src/codec.rs \
        Cargo.toml \
        Cargo.lock \
        docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 05.3: http2-echo-server helper crate (task 8)

SPEC §3 D5: new workspace member tests/helpers/http2-echo-server/ ships
a deterministic HTTP/2 cleartext echo server. Sibling of
tcp-echo-server (02.1), tls-echo-server (03.2), and http1-echo-server
(04.3). Argv parser shape mirrors http1-echo-server verbatim per parent
§6 signpost 7 (--port <u16> + --help + --version).

The deterministic echo body lists method + path + alphabetically-sorted
H2 pseudo-headers + non-pseudo headers + body. The alphabetic sort is
LOAD-BEARING for differential body equivalence — both proxies forward
the same logical request; the helper's sorted-header response is the
byte-exact baseline.

The helper consumes `envoy_http2::codec::server_handshake` (NEW thin
wrapper around `h2::server::handshake`; lands at this task in
crates/envoy-http2/src/codec.rs) per parent §6 signpost 7 — production
code does not gain any new h2-direct surface. The helper's per-stream
loop reaches h2::server::Connection types via the returned wrapper —
this is the documented carve-out parallel to tests/differential's
drive_http2 surface (per parent §6 signpost 8); helper-side h2-direct
deps are accepted per the established 04.3 / 05.2 precedent.

5 unit tests cover argv parsing (4) + an end-to-end echo round-trip
against an in-test h2 client (1).

Workspace member registered. Cargo.lock synced (near-no-op; no new
top-level deps).
EOF
)"
```

---

## Task 9 — Differential harness `Http2EchoBackend` + `run_fixture` cascade extension

**Files:**

- Modify: `tests/differential/src/backend.rs` — add `Http2EchoBackend` struct (sibling of `Http1EchoBackend` at lines 179-238) + `spawn` + `port` + `container_host` + `Drop` impl; add `locate_http2_echo_server` helper (sibling of `locate_http1_echo_server` at lines 244-269); ~3 harness unit tests.
- Modify: `tests/differential/src/lib.rs` — extend the `run_fixture` cascade with the `{{HTTP2_BACKEND_PORT}}` template-marker substitution dispatching to spawn `Http2EchoBackend`. The `Driver::Http2` variant + `drive_http2` helper from 05.2 D5 are reused unchanged. Append 1 harness unit test.
- M10 disposition: if the planner determines fixture 0010 needs `extra_headers` on the `Driver::Http2` variant, add it here at Step 9.6.

**Estimated LoC:** ~220 (~120 LoC `Http2EchoBackend` + locator; ~20 LoC `run_fixture` cascade extension; ~80 LoC unit tests = 4 × ~20 LoC).

**Signposts settled:**

- SPEC §3 D6: `Http2EchoBackend` mirrors `Http1EchoBackend`; SIGKILL-on-Drop + `std::thread::sleep` carryforward unchanged (02.2 REVIEW M1 inherited).
- SPEC §3 D6.b: `run_fixture` cascade extension on `{{HTTP2_BACKEND_PORT}}`.
- SPEC §3 D6.c: `Driver::Http2` reuse (no new variant). `drive_http2` reused from 05.2 D5.b.
- 05.2 REVIEW M10 disposition: `Driver::Http2` extra_headers field — opportunistic at this task if fixture 0010 needs it.

- [ ] **Step 9.1: Cross-check the existing `Http1EchoBackend` shape.**

Run: `grep -nA 60 'pub struct Http1EchoBackend' tests/differential/src/backend.rs | head -80`
Expected: confirms the shape at lines 179-238 + locator at 244-269. Mirror verbatim with `s/Http1/Http2/g` adjustments + the H2 handshake polling at Step 9.4.

- [ ] **Step 9.2: Write 4 failing tests.**

Append to `tests/differential/src/backend.rs::tests`:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn http2_echo_backend_spawns_and_echoes() {
        if locate_http2_echo_server().is_err() {
            eprintln!("skipping http2_echo_backend_spawns_and_echoes — binary not built");
            return;
        }
        let backend = Http2EchoBackend::spawn().await.expect("spawn");
        let addr: std::net::SocketAddr = format!("127.0.0.1:{}", backend.port()).parse().unwrap();
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://testharness/probe")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn http2_echo_backend_drop_terminates_child() {
        if locate_http2_echo_server().is_err() {
            eprintln!("skipping http2_echo_backend_drop_terminates_child — binary not built");
            return;
        }
        let port;
        {
            let backend = Http2EchoBackend::spawn().await.expect("spawn");
            port = backend.port();
        } // backend dropped here — SIGKILL fires
        // Give the OS up to 2s to finalize the kill (mirrors Http1EchoBackend
        // posture per phase-02.2 REVIEW M1 carryforward).
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        // Best-effort assertion: the port is now free (re-bindable).
        let listener =
            tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await;
        assert!(
            listener.is_ok(),
            "expected port {port} to be re-bindable after backend drop"
        );
    }

    #[test]
    fn locate_http2_echo_server_returns_existing_path() {
        match locate_http2_echo_server() {
            Ok(p) => {
                assert!(
                    p.exists(),
                    "locator returned non-existent path {}",
                    p.display()
                );
            }
            Err(_) => {
                eprintln!(
                    "skipping locate_http2_echo_server_returns_existing_path — binary not built"
                );
            }
        }
    }
```

Plus 1 unit test in `tests/differential/src/lib.rs::tests`:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn run_fixture_dispatches_http2_backend_on_template_marker() {
        // Per SPEC §3 D6.b: run_fixture spawns Http2EchoBackend when either
        // upstream_template or subject_template contains {{HTTP2_BACKEND_PORT}}.
        // Test by passing a synthetic template through render_yaml directly
        // and asserting the substitution occurred (the spawn-side is exercised
        // by the dedicated http2_router_upstream Docker-gated test at Task 10).
        let template = "endpoint: {{BACKEND_HOST}}:{{HTTP2_BACKEND_PORT}}";
        let port_str = "7000";
        let kvs: Vec<(&str, &str)> = vec![
            ("HTTP2_BACKEND_PORT", port_str),
            ("BACKEND_HOST", "host.docker.internal"),
        ];
        let rendered = render_yaml(template, &kvs);
        assert!(
            rendered.contains("endpoint: host.docker.internal:7000"),
            "expected substitution; got: {rendered}"
        );
    }
```

(`render_yaml` is the existing harness helper; verify exact name via `grep -n 'pub fn render_yaml' tests/differential/src/lib.rs` at task time.)

- [ ] **Step 9.3: Run the tests to verify they fail.**

Run: `cargo test -p differential -- http2 --nocapture`
Expected: 4 compile errors (`Http2EchoBackend` / `locate_http2_echo_server` undefined).

- [ ] **Step 9.4: Implement `Http2EchoBackend` + locator.**

Edit `tests/differential/src/backend.rs`. Append after the existing `Http1EchoBackend` impl block at line ~238. Mirror the `Http1EchoBackend` shape with adjustments for H2 handshake polling:

```rust
/// Spawns the workspace's `http2-echo-server` helper on an ephemeral
/// 127.0.0.1 port and waits until an H2C handshake against it completes.
///
/// Mirrors `Http1EchoBackend`'s posture (per phase-04.3 D14 / SPEC §3 D6.a):
/// ephemeral port reservation; subprocess spawn via `tokio::process::Command`
/// with `kill_on_drop(true)`; SIGKILL-on-Drop polling loop with the awareness-
/// only 02.2 REVIEW M1 carryforward (`std::thread::sleep` from a tokio-runtime
/// thread) — inherited verbatim.
///
/// Accept-readiness polling is H2-shape aware: the poll opens a TCP connection
/// AND runs `h2::client::handshake` via `tokio::time::timeout` — success means
/// the helper has completed its H2 codec setup, not just that it's accepting
/// TCP. (Per SPEC §3 D6.a's option (a) — "H2 handshake polling because the
/// codec setup is what makes the helper actually ready to serve".)
pub struct Http2EchoBackend {
    port: u16,
    child: Option<tokio::process::Child>,
}

impl Http2EchoBackend {
    pub async fn spawn() -> Result<Self> {
        let port = reserve_port().context("reserving http2 backend port")?;
        let bin = locate_http2_echo_server().context("locating http2-echo-server binary")?;
        let child = tokio::process::Command::new(&bin)
            .arg("--port")
            .arg(port.to_string())
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning {} --port {port}", bin.display()))?;

        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
        wait_h2_accept_ready(addr, Duration::from_secs(2))
            .await
            .with_context(|| format!("http2-echo-server never became h2-accept-ready on {addr}"))?;

        Ok(Self {
            port,
            child: Some(child),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Hostname the upstream Envoy container uses to reach this backend.
    /// Per ADR-0015 + 05.1 STRICT_DNS posture: always `host.docker.internal`.
    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for Http2EchoBackend {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                    Err(_) => return,
                }
            }
        }
    }
}

/// H2-aware accept-readiness poll. Connects TCP then runs h2::client::handshake;
/// retries with exponential backoff up to `budget`. Distinct from
/// `wait_accept_ready` (which is TCP-only) per SPEC §3 D6.a's recommendation.
async fn wait_h2_accept_ready(addr: std::net::SocketAddr, budget: Duration) -> Result<()> {
    let deadline = tokio::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(10);
    loop {
        let attempt = async {
            let tcp = tokio::net::TcpStream::connect(addr).await?;
            let (_send, conn) = h2::client::handshake(tcp).await?;
            tokio::spawn(async move { let _ = conn.await; });
            anyhow::Ok(())
        };
        match tokio::time::timeout(Duration::from_millis(500), attempt).await {
            Ok(Ok(())) => return Ok(()),
            _ if tokio::time::Instant::now() >= deadline => {
                bail!("http2-echo-server not h2-handshake-ready on {addr} within {budget:?}");
            }
            _ => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(200));
            }
        }
    }
}

pub(crate) fn locate_http2_echo_server() -> Result<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest
        .parent()
        .and_then(|p| p.parent())
        .context("walking up from CARGO_MANIFEST_DIR to workspace root")?;
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let mut bin = target_dir.join(profile).join("http2-echo-server");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    if !bin.exists() {
        bail!(
            "http2-echo-server not found at {}; run `cargo build -p http2-echo-server` or `cargo test --workspace`",
            bin.display()
        );
    }
    Ok(bin)
}
```

- [ ] **Step 9.5: Extend `run_fixture` in `tests/differential/src/lib.rs`.**

Edit `tests/differential/src/lib.rs`. After the existing `_http1_backend` block at lines ~1003-1017, append:

```rust
    // 05.3 NEW per SPEC §3 D6.b: spawn Http2EchoBackend if either template
    // needs one. Same alive-keeper binding-order discipline as _backend /
    // _tls_backend / _http1_backend above.
    let needs_http2_backend = upstream_template.contains("{{HTTP2_BACKEND_PORT}}")
        || subject_template.contains("{{HTTP2_BACKEND_PORT}}");
    let _http2_backend: Option<crate::backend::Http2EchoBackend> = if needs_http2_backend {
        Some(
            crate::backend::Http2EchoBackend::spawn()
                .await
                .context("spawning Http2EchoBackend")?,
        )
    } else {
        None
    };
    let http2_backend_port_str = _http2_backend.as_ref().map(|b| b.port().to_string());
```

Extend the per-side substitution maps at lines ~1024-1052 + 1053-1076. In `upstream_kvs`:

```rust
    if let Some(hp) = http2_backend_port_str.as_deref() {
        v.push(("HTTP2_BACKEND_PORT", hp.to_string()));
    }
```

(insert after the existing `HTTP1_BACKEND_PORT` entry).

Extend the `BACKEND_HOST` gate at lines ~1035-1045 (and the symmetric gate in `subject_kvs`):

```rust
    if backend_port_str.is_some()
        || tls_backend_port_str.is_some()
        || http1_backend_port_str.is_some()
        || http2_backend_port_str.is_some()
    {
        v.push(("BACKEND_HOST", "host.docker.internal".to_string()));
    }
```

(For the `subject_kvs` block, the value is `"127.0.0.1"` as in the existing pattern.)

- [ ] **Step 9.6: M10 disposition — `Driver::Http2` extra_headers field (conditional).**

Cross-check fixture 0010's expectations.yaml shape (Task 10 below) — does it carry `extra_headers`?

Per SPEC §3 D7 expectations.yaml example: `driver: { kind: http2, method: GET, path: "/", host: envoy-rust.test, expected_status: 200, ... }`. NO `extra_headers` field is shown. **Decision: defer M10 to whichever fixture first needs extra_headers on H2.**

If the planner discovers at Task 10 task time that fixture 0010 needs additional request headers (e.g., `User-Agent: differential-harness/0.1` to make the deterministic-echo body comparable across both proxies), add the field at this task:

```rust
    Http2 {
        method: Http1Method,
        path: String,
        host: String,
        #[serde(default)]
        extra_headers: Vec<(String, String)>,
        #[serde(default)]
        expected_status: Option<u16>,
        #[serde(default)]
        expected_body: Option<Http1BodyRule>,
        #[serde(default)]
        expected_headers: Option<Http1HeaderRule>,
    },
```

(The `drive_http2` helper at line 801 already accepts `extra_headers: &[(String, String)]` per its current signature; the dispatch arm in `run_fixture` at lines 1100+ already passes the value through. Only the variant field is missing — adding it is mechanical.)

If M10 is added at Task 9: also update the Driver::Http2 dispatch arm in `run_fixture` to thread the new field through `drive_http2`. **For this PLAN's scope:** assume M10 is NOT added (deferred per the SPEC's expectations.yaml example); record disposition in PROGRESS Task 9.

- [ ] **Step 9.7: Run the tests + build envoy-rust binary first.**

Run: `cargo build -p http2-echo-server`
Expected: clean.

Run: `cargo test -p differential -- http2 --nocapture`
Expected: all 4 new tests pass (the 3 in `backend.rs::tests` + the 1 in `lib.rs::tests`).

Run: `cargo build --workspace --all-targets && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: all clean.

- [ ] **Step 9.8: Commit Task 9.**

```bash
git add tests/differential/src/backend.rs \
        tests/differential/src/lib.rs \
        docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 05.3: differential harness Http2EchoBackend (task 9)

SPEC §3 D6: new Http2EchoBackend struct (sibling of TcpProxyBackend /
TlsEchoBackend / Http1EchoBackend) at tests/differential/src/backend.rs.
Public surface mirrors Http1EchoBackend's exactly: spawn / port /
container_host / Drop. Locator helper locate_http2_echo_server is the
sibling of locate_http1_echo_server in the same module.

Accept-readiness polling is H2-shape aware: opens a TCP connection then
runs h2::client::handshake via tokio::time::timeout (per SPEC §3 D6.a's
recommendation — H2 handshake polling because the codec setup is what
makes the helper actually ready to serve). 2-second budget vs.
Http1EchoBackend's 1-second (H2 handshake adds the SETTINGS exchange
round-trip; tighter budgets are flaky on Linux containers).

SIGKILL-on-Drop posture inherited verbatim from Http1EchoBackend. The
awareness-only 02.2 REVIEW M1 carryforward (std::thread::sleep from a
tokio-runtime thread in the polling loop) continues unchanged through
05.3 close — 05.3 does not parallelize run_fixture.

run_fixture cascade extended at tests/differential/src/lib.rs with the
{{HTTP2_BACKEND_PORT}} template-marker substitution. Per-side
substitution maps gain HTTP2_BACKEND_PORT entries. The BACKEND_HOST gate
extends to include http2_backend_port_str.is_some(). The Driver::Http2
variant + drive_http2 helper from 05.2 D5 are reused unchanged.

M10 (05.2 REVIEW: Driver::Http2 lacks extra_headers field) deferred —
fixture 0010 (Task 10) does not need extra_headers per the SPEC §3 D7
expectations.yaml example. M10 carries forward to whichever fixture
first needs it.

3 backend.rs unit tests + 1 lib.rs unit test cover spawn / drop / locate
/ template-marker substitution.

No new top-level Cargo deps.
EOF
)"
```

---


## Task 10 — Fixture `tests/fixtures/0010-http2-router-upstream/` + Docker-gated wrapper

**Files:**

- Create: `tests/fixtures/0010-http2-router-upstream/envoy.yaml`.
- Create: `tests/fixtures/0010-http2-router-upstream/envoy-rust.yaml`.
- Create: `tests/fixtures/0010-http2-router-upstream/inputs/payload.bin` (empty file).
- Create: `tests/fixtures/0010-http2-router-upstream/expectations.yaml`.
- Create: `tests/fixtures/0010-http2-router-upstream/README.md`.
- Create: `tests/differential/tests/http2_router_upstream.rs` (Docker-gated wrapper, 7 lines).

**Estimated LoC:** ~140 (~50 envoy.yaml + ~30 envoy-rust.yaml + ~25 expectations.yaml + ~30 README + ~7 wrapper).

**Signposts settled:**

- SPEC §3 D7: 5 fixture files + Docker-gated wrapper.
- SPEC §6 inherited signpost 4 (fixture 0010 declares STRICT_DNS): cluster `backend` has `type: STRICT_DNS`.
- SPEC §6 inherited signpost 7 (phase-04 fixture YAMLs precedent): inherits the static_resources.listeners[0].filter_chains[0].filters[0] HCM shape.
- 05.4 REVIEW R-2 (`dns_lookup_family: V4_ONLY` for `host.docker.internal`): apply per the established 05.4 posture for fixture 0010's `backend` cluster.

- [ ] **Step 10.1: Cross-check fixture 0008's shape (the H1 sibling).**

Run: `ls tests/fixtures/0008-http1-router-upstream/`
Expected: `envoy.yaml`, `envoy-rust.yaml`, `inputs/`, `expectations.yaml`, `README.md`.

Run: `cat tests/fixtures/0008-http1-router-upstream/envoy.yaml | head -50`
Run: `cat tests/fixtures/0008-http1-router-upstream/envoy-rust.yaml | head -50`
Run: `cat tests/fixtures/0008-http1-router-upstream/expectations.yaml`

Record the structural shape; fixture 0010 mirrors it modulo (a) `codec_type: HTTP2` on the listener, (b) cluster-side `typed_extension_protocol_options` block, (c) `dns_lookup_family: V4_ONLY` on the `backend` cluster (per 05.4 R-2).

- [ ] **Step 10.2: Create `tests/fixtures/0010-http2-router-upstream/envoy.yaml`.**

```yaml
# Phase 05.3 fixture 0010 — HTTP/2 router upstream (H2C end-to-end).
# Downstream listener accepts H2C; cluster `backend` selects upstream H2 via
# typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.
# http2_protocol_options. STRICT_DNS cluster type inherited from 05.1; the
# dns_lookup_family: V4_ONLY override is per 05.4 REVIEW R-2 for
# `host.docker.internal` reachability (avoids IPv6-resolution flakiness on
# Docker for Linux/macOS).

node: { id: envoy-rust-phase-05.3-fixture-0010, cluster: envoy-rust-phase-05.3 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http2_listener
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http2
                codec_type: HTTP2
                # Suppress the default x-request-id injection per the 04.3
                # fixture 0008 precedent so the deterministic-echo body is
                # byte-equal across both proxies.
                generate_request_id: false
                route_config:
                  name: local_route
                  request_headers_to_remove:
                    - x-forwarded-for
                    - x-forwarded-proto
                    - x-request-id
                    - x-envoy-expected-rq-timeout-ms
                    - x-envoy-internal
                    - x-envoy-external-address
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      dns_lookup_family: V4_ONLY
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP2_BACKEND_PORT}} } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
```

- [ ] **Step 10.3: Create `tests/fixtures/0010-http2-router-upstream/envoy-rust.yaml`.**

```yaml
# envoy-rust per-side divergences from envoy.yaml:
#   - bind 127.0.0.1 instead of 0.0.0.0 (no Docker indirection).
#   - no admin block.
#   - request_headers_to_remove omitted (envoy-rust does not inject these).
#   - generate_request_id omitted (envoy-rust does not inject x-request-id).
#   - dns_lookup_family omitted (envoy-rust ignores the field at runtime per
#     05.4 D2 — only the upstream-Envoy side observes V4_ONLY).

node: { id: envoy-rust-phase-05.3-fixture-0010, cluster: envoy-rust-phase-05.3 }
static_resources:
  listeners:
    - name: http2_listener
      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
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
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP2_BACKEND_PORT}} } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
```

- [ ] **Step 10.4: Create `tests/fixtures/0010-http2-router-upstream/inputs/payload.bin`.**

Empty file (0 bytes). The `Driver::Http2 { method: GET, ... }` constructs the request from the driver fields per 05.2 D5.a; the file is present for harness-shape consistency with other fixtures but unread.

```bash
mkdir -p tests/fixtures/0010-http2-router-upstream/inputs
: > tests/fixtures/0010-http2-router-upstream/inputs/payload.bin
```

- [ ] **Step 10.5: Create `tests/fixtures/0010-http2-router-upstream/expectations.yaml`.**

```yaml
# Per SPEC §3 D7. The byte-exact body is determined by the deterministic-echo
# helper (http2-echo-server) per parent SPEC §3 D14.3 — the helper lists
# method + path + alphabetically-sorted headers (including H2 pseudo-headers)
# + body. The exact body string is determined at task time by running the
# harness end-to-end against a known-good envoy-rust + Envoy pair and capturing
# the byte-exact response. The text below is a planner-time projection; the
# planner refines at task time and replaces this comment with the recorded
# result.

driver:
  kind: http2
  method: GET
  path: /
  host: envoy-rust.test
  expected_status: 200
  expected_body:
    byte_exact: |
      method: GET
      path: /
      headers:
        :authority: envoy-rust.test
        :method: GET
        :path: /
        :scheme: http
      body: 
  expected_headers: set_equal_modulo_allow_list

equivalence:
  response_status: exact
  response_body: byte_exact
```

**Note on body shape:** the exact bytes depend on what request headers the harness emits + what header order envoy-rust vs. Envoy use. Per 05.2 REVIEW M4 (SPEC text drift on `expected_headers` shape): the actual harness uses string-shaped `expected_headers: set_equal_modulo_allow_list` (matches the unit-variant `Http1HeaderRule::SetEqualModuloAllowList` deserializer) NOT struct-shaped `expected_headers: { rule: set_equal_modulo_allow_list }`. The above uses the actual deserializer shape.

The byte_exact body MAY include additional headers (e.g., `user-agent: ...` if the harness's drive_http2 emits a default user-agent; check at task time via cargo test then capture the actual response). Adjust at Task 10 task time.

- [ ] **Step 10.6: Create `tests/fixtures/0010-http2-router-upstream/README.md`.**

```markdown
# Fixture 0010 — http2-router-upstream

**Phase:** 05.3.

**Surface:** HTTP/2 cleartext (H2C) downstream listener proxying to an HTTP/2 cleartext upstream cluster. The first H2-on-H2 round-trip in the project.

**Configuration:**

- Downstream listener: `http2_listener` binds `0.0.0.0:{{PORT}}` (Envoy) / `127.0.0.1:{{PORT}}` (envoy-rust); HCM `codec_type: HTTP2`; single virtual host (`domains: ["*"]`); single route (`prefix: "/"`) routing to cluster `backend`.
- Upstream cluster: `backend` of `type: STRICT_DNS` (per 05.1's schema growth) with `dns_lookup_family: V4_ONLY` (per 05.4 REVIEW R-2 for `host.docker.internal` reachability), resolving `{{BACKEND_HOST}}:{{HTTP2_BACKEND_PORT}}`. The `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}` block selects upstream H2 (per 05.3 D2.a).

**Test driver:** `Driver::Http2 { method: GET, path: "/", host: "envoy-rust.test", ... }` (drives via `tests/differential/src/lib.rs::drive_http2`).

**Backend:** `tests/helpers/http2-echo-server` (binary at `target/<profile>/http2-echo-server`); spawned by `Http2EchoBackend` per 05.3 D6.

**Cross-references:**

- Phase 05.3 SPEC §3 D7 — fixture surface.
- Parent-05 SPEC §3 D15.3 — fixture deliverable in the parent split.
- Phase 05.1 SPEC §3 D3 — `STRICT_DNS` cluster type.
- Phase 05.4 SPEC §3 D2 — `dns_lookup_family: V4_ONLY` posture.
- Phase 05.4 REVIEW §4 R-2 + R-4 — `body_is_nonempty` predicate template (informs H2 client codec emission decisions on empty-body GETs); R-2 V4_ONLY for `host.docker.internal`.
- Phase 05.2 fixture 0009 — H2 listener-side direct-response sibling.
- Phase 04.3 fixture 0008 — H1 router-upstream sibling.

**Acceptance signal:** the fixture is green at the Docker-gated CI level (`tests/differential/tests/http2_router_upstream.rs`) AND at the in-process integration backstop level (`crates/envoy-bin/tests/http2_router_upstream.rs`).
```

- [ ] **Step 10.7: Create `tests/differential/tests/http2_router_upstream.rs`.**

```rust
//! Docker-gated differential test for fixture 0010-http2-router-upstream.
//! Mirrors the 04.3-landed `tests/differential/tests/http1_router_upstream.rs`
//! and 05.2-landed `tests/differential/tests/http2_direct_response.rs`.
//! Spawns Envoy v1.33 in a container; spawns envoy-rust as a subprocess;
//! spawns http2-echo-server; drives a single H2C `GET /` request; asserts
//! byte-exact body equivalence under HEADER_ALLOW_LIST per SPEC §3 D7.

#[tokio::test]
async fn http2_router_upstream() {
    differential::run_fixture("0010-http2-router-upstream")
        .await
        .expect("fixture green");
}
```

- [ ] **Step 10.8: Build envoy-bin (the binary the harness spawns) + http2-echo-server.**

Run: `cargo build -p envoy-bin -p http2-echo-server`
Expected: clean.

- [ ] **Step 10.9: Run the Docker-gated test (locally if Docker is available; or defer to CI).**

Run: `cargo test -p differential --test http2_router_upstream -- --nocapture`
Expected: 
- If Docker is running: PASS (the test spawns Envoy v1.33 container + envoy-rust + http2-echo-server, drives the request, asserts equivalence).
- If Docker is not running: SKIPPED via the existing `differential::run_fixture` Docker-skip path.

If the test fails on the byte-exact body assertion, capture the actual envoy-side response body and refine `expectations.yaml` per Step 10.5's note. Likely sources of body-shape drift: `user-agent` / `:authority` value casing differences. Iterate until green.

- [ ] **Step 10.10: Verify the workspace build + clippy + fmt.**

Run: `cargo build --workspace --all-targets && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: all clean.

- [ ] **Step 10.11: Commit Task 10.**

```bash
git add tests/fixtures/0010-http2-router-upstream/ \
        tests/differential/tests/http2_router_upstream.rs \
        docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 05.3: fixture 0010-http2-router-upstream + Docker-gated wrapper (task 10)

SPEC §3 D7: new fixture at tests/fixtures/0010-http2-router-upstream/
(envoy.yaml + envoy-rust.yaml + inputs/payload.bin (empty) +
expectations.yaml + README.md). HCM codec_type: HTTP2 listener; cluster
backend of type: STRICT_DNS with dns_lookup_family: V4_ONLY (per 05.4
R-2) + typed_extension_protocol_options.HttpProtocolOptions.
explicit_http_config.http2_protocol_options: {} selecting upstream H2
(per 05.3 D2.a). Endpoint resolves to {{BACKEND_HOST}}:{{HTTP2_BACKEND_PORT}}
(host.docker.internal for Envoy; 127.0.0.1 for envoy-rust).

Per-side divergences from envoy.yaml mirror 04.3 fixture 0008's posture:
envoy-rust binds 127.0.0.1, omits admin, omits request_headers_to_remove,
omits generate_request_id, omits dns_lookup_family.

Docker-gated test wrapper at tests/differential/tests/http2_router_upstream.rs
is a 7-line wrapper calling differential::run_fixture (mirrors 04.3 and
05.2 wrappers).

The first H2-on-H2 round-trip in the project. Exercises:
  - downstream H2C handshake (envoy-rust HCM listener at 05.2)
  - route walk + BuildOutcome::Proxy dispatch (envoy-http2 HCM at task 7)
  - cluster.upstream_protocol == Http2 selection (envoy-cluster at task 5)
  - upstream H2C dispatch (envoy_http2::Client at task 2)
  - upstream H2 round-trip against http2-echo-server (helper at task 8)
  - response translation back through envoy-rust to the differential client

Cluster-side typed_extension_protocol_options validator (task 3) accepts
the configuration; cluster.upstream_protocol projection (task 5) yields
Http2; the dispatch arms (tasks 6+7) route correctly.

If body byte-exact assertion drifts at task time, expectations.yaml
refined per the captured actual response shape (recorded in PROGRESS).
EOF
)"
```

---

## Task 11 — In-process integration backstop `crates/envoy-bin/tests/http2_router_upstream.rs`

**Files:**

- Create: `crates/envoy-bin/tests/http2_router_upstream.rs`.

**Estimated LoC:** ~150 (~120 LoC test body + ~30 LoC of subprocess + tempfile + h2 client scaffolding).

**Signposts settled:**

- SPEC §3 D7 in-process backstop: spawns envoy-bin via `CARGO_BIN_EXE_envoy-bin`; drives via `h2::client`; asserts the parsed response.
- SPEC §6 inherited signpost 8 (in-process backstops): per parent §6 signpost 18.
- SPEC §6 inherited signpost 9 (`anyhow` boundary): tests in `crates/envoy-bin/tests/*` use `anyhow` per D-3.2.
- The `h2 = "0.4"` `[dev-dependencies]` entry on `crates/envoy-bin/Cargo.toml` was added at 05.2 D4 and is reused unchanged.

- [ ] **Step 11.1: Cross-check the existing `crates/envoy-bin/tests/http2_direct_response.rs` (05.2 D4 backstop).**

Run: `wc -l crates/envoy-bin/tests/http2_direct_response.rs`
Run: `head -40 crates/envoy-bin/tests/http2_direct_response.rs`
Expected: confirms the shape (envoy-bin spawn via CARGO_BIN_EXE_envoy-bin + tempfile config + h2::client::handshake + assertions). Mirror this shape for the router-upstream backstop.

Run: `wc -l crates/envoy-bin/tests/http1_router_upstream.rs`
Run: `head -50 crates/envoy-bin/tests/http1_router_upstream.rs`
Expected: confirms the shape for the H1 router-upstream backstop (spawns http1-echo-server subprocess + envoy-bin + drives a request + asserts). Combine the two: H2 listener + H2 cluster + upstream http2-echo-server.

- [ ] **Step 11.2: Create `crates/envoy-bin/tests/http2_router_upstream.rs`.**

```rust
//! In-process integration backstop for the 05.3 H2-on-H2 router round-trip
//! (mirrors the 04.3 H1 router-upstream backstop at
//! `crates/envoy-bin/tests/http1_router_upstream.rs` and the 05.2 H2 direct-
//! response backstop at `crates/envoy-bin/tests/http2_direct_response.rs`).
//! Spawns envoy-bin via CARGO_BIN_EXE_envoy-bin against an HCM-HTTP2-listener
//! config that points its `backend` cluster at an in-test-spawned
//! http2-echo-server; drives a single H2C `GET /` via h2::client; asserts the
//! parsed response.
//!
//! This test is non-Docker — runs anywhere with the binaries built. Skipped
//! gracefully if either binary is missing.

use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;

fn locate_http2_echo_server() -> Result<PathBuf> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest
        .parent()
        .and_then(|p| p.parent())
        .context("walking to workspace root")?;
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let mut bin = target_dir.join(profile).join("http2-echo-server");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    Ok(bin)
}

fn reserve_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

#[tokio::test(flavor = "multi_thread")]
async fn http2_router_upstream_in_process() -> Result<()> {
    // Locate http2-echo-server. Skip if not built.
    let helper_bin = locate_http2_echo_server()?;
    if !helper_bin.exists() {
        eprintln!(
            "skipping http2_router_upstream_in_process — http2-echo-server not built at {}",
            helper_bin.display()
        );
        return Ok(());
    }

    // Spawn http2-echo-server.
    let helper_port = reserve_port()?;
    let mut helper_child = tokio::process::Command::new(&helper_bin)
        .arg("--port")
        .arg(helper_port.to_string())
        .env("RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawning {} --port {helper_port}", helper_bin.display()))?;

    // Wait for h2 handshake readiness.
    let helper_addr: std::net::SocketAddr =
        format!("127.0.0.1:{helper_port}").parse()?;
    wait_h2_ready(helper_addr).await?;

    // Build the envoy-rust config pointing at the helper.
    let envoy_port = reserve_port()?;
    let envoy_yaml = format!(
        r#"node: {{ id: backstop, cluster: envoy-rust-05-3 }}
static_resources:
  listeners:
    - name: l
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: {envoy_port} }} }}
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
                          route: {{ cluster: backend }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: {{ socket_address: {{ address: 127.0.0.1, port_value: {helper_port} }} }}
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {{}}
"#
    );

    let config_path = tempfile::Builder::new()
        .prefix("envoy-rust-")
        .suffix(".yaml")
        .tempfile()?;
    config_path.as_file().write_all(envoy_yaml.as_bytes())?;

    // Spawn envoy-bin.
    let envoy_bin = PathBuf::from(env!("CARGO_BIN_EXE_envoy-bin"));
    let mut envoy_child = tokio::process::Command::new(&envoy_bin)
        .arg("--config-path")
        .arg(config_path.path())
        .env("RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawning {} --config-path", envoy_bin.display()))?;

    let envoy_addr: std::net::SocketAddr =
        format!("127.0.0.1:{envoy_port}").parse()?;
    wait_h2_ready(envoy_addr).await?;

    // Drive a single H2C GET / against envoy-rust.
    let tcp = tokio::net::TcpStream::connect(envoy_addr).await?;
    let (mut send_request, conn) = h2::client::handshake(tcp).await?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let req = http::Request::builder()
        .method("GET")
        .uri("http://envoy-rust.test/")
        .body(())?;
    let (response_fut, _) = send_request
        .send_request(req, true)
        .map_err(|e| anyhow::anyhow!("send_request: {e}"))?;
    let resp = response_fut
        .await
        .map_err(|e| anyhow::anyhow!("response: {e}"))?;
    assert_eq!(resp.status().as_u16(), 200);

    // Drain body.
    let (_parts, mut body) = resp.into_parts();
    let mut body_bytes = bytes::BytesMut::new();
    while let Some(chunk_result) = body.data().await {
        let chunk = chunk_result.map_err(|e| anyhow::anyhow!("body: {e}"))?;
        body_bytes.extend_from_slice(&chunk);
        let _ = body.flow_control().release_capacity(chunk.len());
    }
    let body_str = std::str::from_utf8(&body_bytes)
        .context("response body is not valid UTF-8")?;
    assert!(
        body_str.starts_with("method: GET\n"),
        "expected echo body shape, got: {body_str}"
    );
    assert!(
        body_str.contains(":authority: envoy-rust.test\n"),
        "expected :authority in echo body, got: {body_str}"
    );

    // Cleanup.
    let _ = envoy_child.start_kill();
    let _ = helper_child.start_kill();
    Ok(())
}

async fn wait_h2_ready(addr: std::net::SocketAddr) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut delay = Duration::from_millis(20);
    loop {
        let attempt = async {
            let tcp = tokio::net::TcpStream::connect(addr).await?;
            let (_send, conn) = h2::client::handshake(tcp).await?;
            tokio::spawn(async move { let _ = conn.await; });
            anyhow::Ok(())
        };
        match tokio::time::timeout(Duration::from_millis(500), attempt).await {
            Ok(Ok(())) => return Ok(()),
            _ if tokio::time::Instant::now() >= deadline => {
                anyhow::bail!("not h2-ready on {addr} within 3s");
            }
            _ => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(200));
            }
        }
    }
}
```

- [ ] **Step 11.3: Verify `crates/envoy-bin/Cargo.toml` `[dev-dependencies]` covers the test.**

Run: `grep -nE '(h2|tempfile|anyhow|http|bytes|tokio)' crates/envoy-bin/Cargo.toml`
Expected: confirms `h2 = "0.4"` (per 05.2 D4), `anyhow`, `tempfile`, `tokio`, `http`, `bytes` are all in `[dev-dependencies]`. If any are missing, add them.

- [ ] **Step 11.4: Run the test.**

Build the binaries first:

```bash
cargo build -p envoy-bin -p http2-echo-server
```

Run the test:

```bash
cargo test -p envoy-bin --test http2_router_upstream -- --nocapture
```

Expected: PASS (or skip-with-eprintln if the helper binary is missing).

- [ ] **Step 11.5: Verify the workspace build + clippy + fmt.**

Run: `cargo build --workspace --all-targets && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: all clean.

- [ ] **Step 11.6: Commit Task 11.**

```bash
git add crates/envoy-bin/tests/http2_router_upstream.rs \
        docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 05.3: in-process integration backstop crates/envoy-bin/tests/http2_router_upstream.rs (task 11)

SPEC §3 D7 in-process backstop: spawns envoy-bin via
CARGO_BIN_EXE_envoy-bin against an HCM-HTTP2-listener config that points
its `backend` cluster at an in-test-spawned http2-echo-server; drives a
single H2C GET / via h2::client; asserts status 200 + body starts with
"method: GET\n" + body contains ":authority: envoy-rust.test\n".

Mirrors 04.3's crates/envoy-bin/tests/http1_router_upstream.rs and
05.2's crates/envoy-bin/tests/http2_direct_response.rs. The h2 = "0.4"
[dev-dependencies] entry from 05.2 D4 is reused unchanged.

The backstop is non-Docker — runs anywhere with the binaries built;
skipped gracefully via eprintln when http2-echo-server is missing.
Complements the Docker-gated tests/differential/tests/http2_router_upstream.rs
at task 10 — same end-to-end surface, different topology (in-process
vs. cross-container).

No new top-level Cargo deps.
EOF
)"
```

---

## Task 12 — State-4 phase-done gate verification + h2spec re-run + Cargo.lock cross-check + cargo-deny

**Files:**

- Modify: `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md` — append Task 12 narration with the CI run URL + per-fixture matrix + h2spec result + clippy/fmt/test output.
- Cross-check (no edits anticipated): `Cargo.lock`, `deny.toml`, `.github/workflows/ci.yml`.

**Estimated LoC:** 0 code; ~50 LoC PROGRESS narrative.

**Signposts settled:**

- SPEC §1 acceptance signal (a)–(f): all gates GREEN.
- SPEC §6 inherited signpost 6 (Cargo.lock cadence): expected near-no-op diff.
- 05.2 SPEC §1 acceptance signal (c) (h2spec ≥95%): re-runs to confirm no regression from 05.3's upstream-direction work.

- [ ] **Step 12.1: Run the full-workspace command set locally.**

Run, in order:

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
```

Expected: all clean.

If `cargo test --workspace` fails on any fixture (0001-0009), the regression is in 05.3's surface — investigate per `superpowers:systematic-debugging`. Most likely candidates: (a) Task 5's `Cluster` struct add forced a downstream change in `envoy-cluster::ClusterManager::empty()` consumers; (b) Task 3's `bootstrap.rs` schema add forced an `allow_unknown_fields` change somewhere; (c) Task 6's dep-cycle resolution moved a function-pointer through `HCMConfig`.

- [ ] **Step 12.2: Run the h2spec conformance suite.**

Run: `cargo test -p h2spec-runner --tests -- --nocapture`
Expected: ≥95% pass rate; 144/146 = 99.31% (the 05.2 baseline). Single failure `3.5/2` from `known-failures.txt` per parent-05 SPEC §6 signpost 13.

If h2spec regresses (pass rate drops), investigate — most likely candidates: (a) Task 7's symmetric dispatch at `envoy-http2/src/hcm.rs` introduced an h2-incompliant response shape; (b) Task 2's `Client::send_request` introduces a request-side regression (unlikely — h2spec exercises the server side only).

- [ ] **Step 12.3: Run the fuzz target's short-budget run.**

Run: `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`
Expected: clean run; the new `cluster_http2_protocol_options.yaml` corpus seed is exercised; no panics.

If `cargo +nightly` is unavailable in the local env, defer to CI (the existing nightly fuzz job covers it). Record the CI run URL in PROGRESS.

- [ ] **Step 12.4: Cross-check Cargo.lock.**

Run: `git diff --stat Cargo.lock`
Expected: a small diff — the new `http2-echo-server` workspace member registers; no new top-level deps. If the diff is large (suggesting a transitive surface drifted unexpectedly), investigate. Record the actual diff size in PROGRESS.

- [ ] **Step 12.5: Cross-check `deny.toml`.**

Run: `cargo deny check`
Expected: `advisories ok, bans ok, licenses ok, sources ok` final-line gate signal. No new licenses; no new advisories.

- [ ] **Step 12.6: Push to CI and capture the run URL.**

```bash
git push origin <branch>
```

Then visit the GitHub Actions UI to capture the CI run URL. The relevant CI workflow runs:
- `cargo build`, `cargo clippy -D warnings`, `cargo fmt --check`, `cargo test --workspace`
- `cargo deny check`
- `parse_bootstrap` fuzz target's short-budget run
- All 10 Docker-gated fixtures (0001-0010) green simultaneously
- h2spec at ≥95%

If any CI gate fails, fix inline (do not skip) and re-push.

- [ ] **Step 12.7: Append the state-4 PROGRESS narrative.**

Append to `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md`:

```markdown

---

## Task 12 — State-4 phase-done gate verification

**Commit:** <SHA>

**Deliverables:** SPEC §1 acceptance signal (a)–(f) GREEN; the parent-05 acceptance surface verified.

**ADR landed:** none.

**Files modified:** `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md` only.

**LoC:** ~50 (narrative).

**Verification (per SPEC §1 acceptance signal):**

- (a) GREEN — fixture 0010-http2-router-upstream Docker-gated test passes. CI run URL: <URL>. Per-fixture matrix:
  - `echo_fixture` <wall>s
  - `admin_ready_fixture` <wall>s
  - `tcp_proxy_fixture` <wall>s
  - `tls_downstream_fixture` <wall>s
  - `tls_sni_fixture` <wall>s
  - `tls_upstream_fixture` <wall>s
  - `http1_direct_response_fixture` <wall>s
  - `http1_router_upstream_fixture` <wall>s
  - `http2_direct_response_fixture` <wall>s
  - `http2_router_upstream_fixture` <wall>s — **NEW (05.3)**
- (b) GREEN — all 9 pre-existing fixtures (0001-0009) pass simultaneously.
- (c) GREEN — h2spec at <pass>/<total> = <pct>% (≥95% gate). Single failure `3.5/2` continues to be classified in known-failures.txt as a foundation limitation per parent-05 SPEC §6 signpost 13. **No regression from the 05.2 baseline (99.31% pass).**
- (d) GREEN — fuzz `parse_bootstrap` clean for 30s with the new `cluster_http2_protocol_options.yaml` corpus seed exercising the validator's cluster-side typed_extension_protocol_options accept-path.
- (e) GREEN — `cargo build`, `cargo clippy -D warnings`, `cargo fmt --check`, `cargo test --workspace`, `cargo deny check` all clean. `cargo deny check` final line: `advisories ok, bans ok, licenses ok, sources ok` (5 pre-existing `license-not-encountered` advisory-only warnings unchanged from the 05.2 baseline; do not represent new licenses brought in by 05.3).
- (f) `REVIEW.md` to land at state-5 (separate session).

**Cargo.lock cross-check:** diff is <N lines> (the `http2-echo-store` workspace-member registration only — no new top-level deps; M5/M9 carryforward continues unchanged).

**deny.toml cross-check:** no edits.

**`.github/workflows/ci.yml` cross-check:** no edits (the h2spec install step landed at 05.2 D7 covers 05.3's needs).

**Verified shapes from greps run at task time:**
- `grep -c '^| 05' docs/envoy-rust/ROADMAP.md` — confirms ROADMAP rows for 05 / 05.1 / 05.2 / 05.3 / 05.4 unchanged at state-4 time (the row flips happen at state-6 close-out only).
- `grep -nE '^## ADR-' docs/envoy-rust/DECISIONS.md | tail -3` — confirms ledger head still ADR-0027 (or ADR-0028 if Task 6 landed it per Step 6.7).

**Deviations from PLAN:** [record any].

**Carryforward:** [carry-forwards from 05.2 REVIEW (I1, I2, I3, M2, M6, M11, M12) continue unchanged through 05.3 close per SPEC §1 paragraph 4. M8 closed structurally at task 7. M10 disposition recorded in PROGRESS Task 9 (deferred — fixture 0010 did not need extra_headers). The state-6 close-out (separate session) lands the consolidated rollover bookkeeping.]
```

- [ ] **Step 12.8: Commit Task 12.**

```bash
git add docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 05.3: state-4 phase-done gate verification (task 12)

SPEC §1 acceptance signal (a)-(f) GREEN per CI run <URL>:
  (a) fixture 0010-http2-router-upstream green (Docker-gated).
  (b) all 9 pre-existing fixtures (0001-0009) green simultaneously.
  (c) h2spec at <pct>% pass rate (≥95% gate; no regression from 05.2's
      99.31% baseline). Single failure 3.5/2 in known-failures.txt.
  (d) parse_bootstrap fuzz 30s clean with the new
      cluster_http2_protocol_options.yaml corpus seed.
  (e) cargo build / clippy -D warnings / fmt --check / test --workspace
      / deny check all clean.
  (f) REVIEW.md to land at state-5 (separate session per
      BOOTSTRAP_PROMPT.md §5.1).

Cargo.lock cross-check: <N>-line diff (workspace-member registration
only; no new top-level deps). deny.toml no-op. CI workflow unedited.

Carryforwards from 05.2 REVIEW (I1 / I2 / I3 / M2 / M6 / M11 / M12)
continue forward per SPEC §1 paragraph 4. M8 closed structurally at
task 7 (502 stub replaced). M10 disposition: deferred per task 9's
PROGRESS (fixture 0010 did not need extra_headers).

The state-6 phase-done close-out is a separate session per
BOOTSTRAP_PROMPT.md §5.1 — flips ROADMAP rows 05.3 + 05 to done; advances
STATE.md to phase 06 lifecycle state 1; lands the consolidated
Phase-05.3 rollovers + Phase-05 ADR ledger (final) Notes sections.
EOF
)"
```

---

## State-6 phase-done close-out (separate session)

State-6 close-out is **NOT in this PLAN's task list** per `BOOTSTRAP_PROMPT.md` §5.1 ("one state per session"). The state-6 session:

1. Lands `REVIEW.md` (state-5 already happened in a separate session before state-6).
2. Flips ROADMAP rows: `05.3` `status: planned` → `done`; **AT THE SAME COMMIT:** parent row `05` `status: in-progress` → `done` per the ROADMAP-schema invariant.
3. Advances STATE.md active phase from `05.3-http2-upstream` lifecycle state 6 to phase `06-<slug>` lifecycle state 1 (per SPEC §1 / §8 / `BOOTSTRAP_PROMPT.md` §8 row 06: *"Access log (file sink, Envoy default format) + stats + Prometheus admin endpoint"*; expected slug `06-access-log-stats` or similar — the planner uses whatever slug the phase-06 brainstorm chooses).
4. Adds the "Phase-05.3 rollovers" Notes subsection per SPEC §6 signpost 23 — enumerates the 05.3 REVIEW.md verdict, in-phase closures (M8 at task 7), awareness-only items + cross-phase carryforwards (I1 / I2 / I3 / M2 / M6 / M11 / M12 from 05.2 REVIEW; M-claim from 04.1; M5/M9 from 04.1; M1 from 02.2; M7 from 04.1; M1/M2/M4 from 04.1; M5/M8/M9/M11 from 04.2 — all carryforwards continued).
5. Adds the "Phase-05 ADR ledger (final)" Notes subsection — confirms ADR-0022 (parent-05 split), ADR-0023 (05.1 Task 1), ADR-0024 + ADR-0026 + ADR-0025 (05.4 Tasks 1 / 3 / 5), ADR-0027 (05.2 Task 1), and ADR-0028 if landed at 05.3 Task 6 per Step 6.7. Landing-time order: ADR-0023 → 0024 → 0026 → 0025 → 0027 → 0028 (if landed).
6. **No code changes** at the state-6 commit (per the established 05.1 / 05.2 / 05.4 / 04.3 / 03.2 / 02.2 close-out cadence).
7. Commit message format per SPEC §9: title carries the `[parent 05 done]` tag (mirrors 04.3's `[parent 04 done]` and 03.2's `[parent 03 done]`); body enumerates the four sub-phases (05.1 / 05.4 / 05.2 / 05.3) with their commit SHAs + ADRs landed.

The next session after state-6 invokes `superpowers:brainstorming` scoped to phase 06.

---

## Summary

Phase 05.3 closes the parent-05 HTTP/2 surface in ~12 tasks across ~2002 LoC:

- **D1 (~535 LoC, Tasks 1-2):** `envoy-http2::Client` with the inverse-direction codec edge mirroring 05.2 D3's listener-side translation. 4 additive `Http2Error` variants. 8 unit tests covering connect / pseudo-header synthesis / explicit-Host: precedence / status+headers+body / multi-frame body drain / hop-by-hop strip / handshake-failure mapping.

- **D2 (~335 LoC, Tasks 3-4):** Cluster-side `Http2ProtocolOptions` schema via `typed_extension_protocol_options`. 4 new types + 1 new field + 2 new ConfigError variants + range-check helper hoist. 7 validator unit tests + 1 corpus-walk acceptance test + 1 fuzz seed.

- **D3 (~110 LoC, Task 5):** `UpstreamProtocol { Http1, Http2 }` enum + `Cluster.upstream_protocol` field + accessor pair + `from_bootstrap` projection. 3 unit tests.

- **D4 (~180 LoC, Tasks 6-7):** Router H2-arm dispatch on `cluster.upstream_protocol()`. **Resolves the `envoy-http1` ↔ `envoy-http2` cycle via ADR-0028.** Symmetric dispatch on H2 listener side replaces 05.2's 502 stub (closes 05.2 REVIEW M8 structurally). 4 unit tests across both sites.

- **D5 (~330 LoC, Task 8):** `tests/helpers/http2-echo-server` workspace member with the deterministic alphabetically-sorted-headers echo body. `crates/envoy-http2/src/codec.rs::server_handshake` thin wrapper. 5 unit tests + 1 codec test.

- **D6 (~220 LoC, Task 9):** Differential harness `Http2EchoBackend` + `locate_http2_echo_server` + `run_fixture` cascade extension on `{{HTTP2_BACKEND_PORT}}`. 4 unit tests.

- **D7 (~292 LoC, Tasks 10-11):** Fixture 0010 (5 files; H2C downstream + cluster-side typed_extension_protocol_options selecting H2 upstream). Docker-gated wrapper. In-process integration backstop driving via h2::client.

- **D8 (~0 LoC, Task 12 + state-6):** Parent-05 close-out wiring at the state-6 commit (separate session); state-4 phase-done gate verification at Task 12.

The 12-task PLAN holds against the `BOOTSTRAP_PROMPT.md` §6.1 task-count gate (12 ≪ 25). The LoC gate (~1500) is crossed at ~2002 LoC; per SPEC §6 signpost 26 + parent-05 SPEC §5's no-nest-split rule, the disposition is "do not split" — recorded inline in the PLAN preamble's "~12 tasks, ~2002 LoC" paragraph + at PROGRESS Task 1 narrative.

The state-6 close-out commit ALSO flips parent ROADMAP row `05` to `done` (the 05.3 commit closes parent-05, per the ROADMAP-schema invariant) and STATE.md advances to phase 06 lifecycle state 1.

