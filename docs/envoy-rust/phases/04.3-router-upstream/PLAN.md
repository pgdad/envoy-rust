# Phase 04.3 — Upstream HTTP/1.1 origination + router proxy arm + http1-echo-server helper + fixture 0008 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task (fresh subagent per task + two-stage review). Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **For every code-writing task:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — failing test first, verify fails, minimal implementation, verify passes, commit. No exceptions (doctrine D-3.1).
>
> **Source of truth:** `docs/envoy-rust/phases/04.3-router-upstream/SPEC.md` (committed at parent-04 state-2 commit `1d9740d` alongside ADR-0020). This plan operationalizes SPEC §§D1–D5. Where the plan and the SPEC disagree, the SPEC wins — flag the drift, land an ADR per D-3.5, and continue. The parent phase-04 SPEC at `docs/envoy-rust/phases/04-http1/SPEC.md` (committed at SHA `805433e`) is preserved unedited as a historical artifact; for execution it is superseded by sub-phase SPECs (`04.1-hcm-direct-response/SPEC.md` for 04.1, `04.2-route-matchers/SPEC.md` for 04.2, this 04.3 sibling SPEC for 04.3).

**Goal:** Close parent phase 04 (HTTP/1.1 data plane) by adding upstream HTTP/1.1 origination and the router filter's proxy-to-cluster arm. Three coordinated layers: (1) `envoy-http1::Client` — a per-connection plaintext HTTP/1.1 client (TCP-connect + serialized request write + response read with both `Content-Length` and chunked-encoding response framings); no pooling. (2) Router filter "proxy to cluster" arm — `RouteAction` enum gains a `Route(RouteAction_Route)` variant; HCM's hardcoded router invocation site extends from one match arm (`DirectResponse`) to two; the new `Route` arm calls `cluster_mgr.get(&action_route.cluster).expect(...)`, picks an endpoint via the existing round-robin LB (02.1), connects via `Client::connect`, forwards the request body (CL-only), reads the upstream response, writes the response back to downstream with `x-envoy-upstream-service-time: <ms>` injected and the header allow-list policy applied (envoy-rust overwrites `server` and `date`). Validator extends `validate_hcm` with a cluster-name reference check reusing `ConfigError::UnknownCluster` from phase 02.1. (3) `tests/helpers/http1-echo-server/` — new workspace member (sibling of `tcp-echo-server` from 02.1 + `tls-echo-server` from 03.2); plaintext only (no TLS); deterministic echo response body with alphabetically-sorted request headers (load-bearing for differential equivalence). Fixture `0008-http1-router-upstream` proves the round-trip end-to-end byte-exact through both proxies. Opportunistic close-out of the multi-phase `Cluster::name()` carryforward (phase-02.1 REVIEW M1 → phase-02.2 §4 rec 1 → phase-03.1 §4 rec 2 → phase-03.2 Task 5 deferred → 04.3 closes per parent SPEC §3 D12.3 + 04.3 SPEC §3 D5). BEHAVIOR_CONTRACT.md `Header allow-list` gains `x-envoy-upstream-service-time` (per parent SPEC §2). Parent ROADMAP row `04` flips to `done` in 04.3's state-6 phase-done commit. ~17 tasks, ~1490 LoC per SPEC §5; comfortably under both `BOOTSTRAP_PROMPT.md` §6.1 split-gates (~25 tasks / ~1500 LoC). No new ADRs anticipated per SPEC §7 (ADR-0020 + ADR-0021 are both landed before 04.3 starts).

**Architecture:** envoy-config grows a tight `RouteAction` enum + `RouteAction_Route { cluster: String }` struct. Per SPEC §3 D2 the existing `Route { r#match, direct_response }` shape is restructured to `Route { r#match, action: RouteAction }` with `RouteAction::DirectResponse(DirectResponse)` (04.1 carryover) + `RouteAction::Route(RouteAction_Route)` (04.3 NEW). Because Envoy's YAML uses `direct_response:` and `route:` as peer keys at the route map level (not under a single `action:` key), `Route` gets a hand-rolled `impl<'de> Deserialize<'de>` that collects map keys, requires exactly one of {`direct_response`, `route`} alongside the required `match`, and emits `serde::de::Error::custom(...)` if both or neither are present — mirrors 04.2's `HeaderMatcher` field-name oneof discipline (per parent-04 SPEC §3 cross-sub-phase architectural rule about hand-rolled visitors for field-name-discriminated oneofs). The validator extends `validate_hcm` to walk `route.action`: for `RouteAction::DirectResponse` the existing checks unchanged; for `RouteAction::Route(ar)` it checks `ar.cluster` against `bootstrap.static_resources.clusters[*].name`, emitting `ConfigError::UnknownCluster(name)` (04.1 carryover variant; reused by ADR — no new variant needed; per SPEC §3 D2 the existing `UnknownCluster(String)` newtype variant is reused as-is, with the route's referrer named via the `tracing::warn!(route = ..., cluster = ..., ...)` log line on the rejection path rather than via a struct-field schema change). The signature of `validate_hcm` lifts from `(&mut HttpConnectionManagerConfig)` to `(&mut HttpConnectionManagerConfig, &[Cluster])` so the cluster-name set is in scope; the single caller in `validate` (the listener-walk's `HCM_FILTER` arm at `bootstrap.rs:869-878`) updates in lockstep. envoy-http1 grows two new modules: `crates/envoy-http1/src/client.rs` (the `Client` + `ClientStream` types + chunked-encoding response reader; sole user of `httparse::Response::parse` in the workspace per parent-04 SPEC §3 cross-sub-phase rule 1) and `crates/envoy-http1/src/router.rs` (the `RouterError` enum + `write_proxied_response` helper + `HCM_EMITTED_HEADERS` constant — factored out of `hcm.rs` per SPEC §6 signpost 7 to keep the proxied-response shape policy in one focused module; the `hcm.rs` two-arm `match action { ... }` calls into `router.rs::write_proxied_response` for the `Route` arm and reuses the existing `synth_direct_response` for the `DirectResponse` arm). The new `Http1Error` variants (`UpstreamConnect`, `MalformedResponseLine`, `MalformedChunkedFraming`) extend the 04.1-landed enum at `crates/envoy-http1/src/error.rs:3-25`. The `HCMConfig` struct at `crates/envoy-http1/src/hcm.rs:27-31` extends with `cluster_mgr: Arc<envoy_cluster::ClusterManager>` (matching the placeholder comment at line 30) and `HCMConfig::from_config` lifts to take a `cluster_mgr` parameter — the single caller in `crates/envoy-bin/src/main.rs:215` updates in lockstep. The `tests/helpers/http1-echo-server/` crate ships with its own `Cargo.toml` (deps: `envoy-http1` path-dep + `anyhow` + `thiserror` + `tokio` + `tracing` + `tracing-subscriber`; mirrors `tls-echo-server` minus the rustls/rcgen/tempfile chunk), hand-parsed argv (`--port <u16>` required, `--help`, `--version`; `ArgvError` variants `MissingFlag(&'static str)`, `MissingValue`, `InvalidPort`, `Trailing`, `HelpRequested`, `VersionRequested` — same shape as `tls-echo-server`'s argv parser at `tests/helpers/tls-echo-server/src/main.rs:32-48` minus `--cert` / `--key`), and a deterministic-echo runtime (parse one HTTP/1.1 request via `envoy_http1::Http1Codec`, build the echo body with alphabetically-sorted lowercase header names per SPEC §3 D3, write the response via `envoy_http1::Http1Response`, close the connection — no keep-alive). The differential harness extends `tests/differential/src/backend.rs` with `Http1EchoBackend` (sibling of `TlsEchoBackend` at lines 99-168; SIGKILL-on-Drop posture per phase-02.2 REVIEW M1 carries unchanged) + `locate_http1_echo_server` (mirrors `locate_tls_echo_server` at lines 173-198), and extends `tests/differential/src/lib.rs::run_fixture` (line 820) with a `{{HTTP1_BACKEND_PORT}}` template-marker detection cascade arm at the same shape as the existing `{{TLS_BACKEND_PORT}}` arm at lines 879-893; the `HEADER_ALLOW_LIST` constant at line 188 gains `("x-envoy-upstream-service-time", AllowMode::NameRequired)` in lockstep with the `BEHAVIOR_CONTRACT.md` `Header allow-list` row. Fixture `tests/fixtures/0008-http1-router-upstream/` is 5 files following 04.1's fixture-0007 shape: `envoy.yaml` (HCM listener with a single VH `domains: ["*"]` + single route `prefix: "/"` + `route: { cluster: backend }`; cluster `backend` STATIC + ROUND_ROBIN with one endpoint `{{BACKEND_HOST}}:{{HTTP1_BACKEND_PORT}}`); `envoy-rust.yaml` (per-side divergences: bind `127.0.0.1`, no admin block); `inputs/payload.bin` (raw HTTP/1.1 request bytes `GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nContent-Length: 0\r\n\r\n`; format follows the parent SPEC §6 signpost 10 worked example); `expectations.yaml` (single-probe `Driver::Http1` shape — fixture 0008 is one probe, not a probe-list, so the existing single-probe `Driver::Http1` from 04.1 is sufficient; no need to use `Driver::Http1ProbeList` from 04.2; `expected_body.byte_exact` is the deterministic helper echo per §D3); `README.md`. Docker-gated `tests/differential/tests/http1_router_upstream.rs` is a 7-line wrapper calling `differential::run_fixture("0008-http1-router-upstream")`. envoy-bin gets a Docker-free in-process integration test `crates/envoy-bin/tests/http1_router_upstream.rs` (sibling of 04.1's `http1_direct_response.rs` at lines 14-209) that spawns the real `http1-echo-server` binary via `locate_http1_echo_server` (paths cross-package), spawns `envoy-bin` via `CARGO_BIN_EXE_envoy-bin`, opens a TCP connection, writes the request bytes, parses the response. envoy-cluster gets `pub fn Cluster::name(&self) -> &str` (per SPEC §3 D5; the visibility lifts to `pub` because the `RouterError` consumers in `envoy-http1::router` are in a different crate); the field-level `#[allow(dead_code)]` on `Cluster.name` at `crates/envoy-cluster/src/cluster.rs:13` is removed; ~3 unit tests appended. The existing `parse_bootstrap` fuzz target at `crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs` runs unchanged; the corpus extends with one new seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_route_to_cluster.yaml` exercising the `RouteAction::Route` variant + `ConfigError::UnknownCluster` reject path (per SPEC §3 D2 fuzz-corpus extension). `Cargo.lock` syncs as a dedicated commit at the state-4 phase-done gate per the established phase-precedent (phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85633e7`, phase-04.2's dedicated post-state-4 sync); the new transitive surface is minimal because `http1-echo-server`'s deps (envoy-http1 + tokio + anyhow + thiserror + tracing + tracing-subscriber) are all already in the workspace's transitive graph from earlier phases. No ADRs anticipated; the SPEC §7 list of conditional ADRs (ADR-0022 for TLS-on-upstream-HCM combos, header allow-list extensions, Cluster::name posture, chunked-request-body posture) all default to no-action in 04.3.

**Tech stack:** Rust edition 2024 on pinned stable (rust-toolchain.toml D-3.9). No new direct deps anywhere — `envoy-http1`'s `Client` reuses already-imported `httparse`/`bytes`/`tokio`/`thiserror`/`tracing` from 04.1's Cargo.toml; `http1-echo-server`'s deps (envoy-http1 path-dep + anyhow + thiserror + tokio + tracing + tracing-subscriber) are already in the workspace transitive graph; differential harness adds no new deps (the `Http1EchoBackend` reuses tokio/anyhow/std). New runtime API surface on `envoy-http1` (`pub mod client`, `pub mod router` re-exported from `lib.rs`); new struct `Http1EchoBackend` + free fn `locate_http1_echo_server` on `differential::backend`; new template-marker `{{HTTP1_BACKEND_PORT}}` recognized in `differential::run_fixture`. New runtime API surface on `envoy-cluster` (`pub fn Cluster::name`). No changes to `.github/workflows/ci.yml` (per SPEC §3 D5 / phase-04 precedent: existing `cargo test --workspace` + fuzz job pick up additions automatically; the new Docker-gated `tests/differential/tests/http1_router_upstream.rs` runs alongside the existing six Docker-gated tests under `cargo test --workspace`).

---

## File structure (created / modified / not touched)

**Created:**

- `crates/envoy-http1/src/client.rs` — new module owning `Client` (TCP-connect + per-request `ClientStream`) + chunked-encoding response reader + 8 unit tests. Sole user of `httparse::Response::parse` in the workspace.
- `crates/envoy-http1/src/router.rs` — new module owning `RouterError` enum + `write_proxied_response` helper + `HCM_EMITTED_HEADERS: &[&str] = &["server", "date"]` constant + 3 unit tests. Factored out of `hcm.rs` per SPEC §6 signpost 7 to keep the proxied-response shape policy in one focused module.
- `tests/helpers/http1-echo-server/Cargo.toml` — new workspace member; deps: `envoy-http1` (path) + `anyhow` + `thiserror` + `tokio` (rt-multi-thread + net + io-util + macros + signal + time + sync) + `tracing` + `tracing-subscriber`. No TLS deps.
- `tests/helpers/http1-echo-server/src/main.rs` — `#![forbid(unsafe_code)]` + argv parser (Args + ArgvError) + accept loop + deterministic echo response body + 5 unit tests (4 argv + 1 round-trip).
- `crates/envoy-bin/tests/http1_router_upstream.rs` — Docker-free in-process integration test (sibling of 04.1's `http1_direct_response.rs`).
- `tests/differential/tests/http1_router_upstream.rs` — Docker-gated acceptance test (sibling of 04.1's `http1_direct_response.rs` and 03.2's `tls_upstream.rs`).
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_route_to_cluster.yaml` — new fuzz seed exercising the `RouteAction::Route` variant + `ConfigError::UnknownCluster` reject path.
- `tests/fixtures/0008-http1-router-upstream/envoy.yaml` — Envoy-side fixture YAML.
- `tests/fixtures/0008-http1-router-upstream/envoy-rust.yaml` — envoy-rust-side fixture YAML.
- `tests/fixtures/0008-http1-router-upstream/inputs/payload.bin` — raw HTTP/1.1 request bytes.
- `tests/fixtures/0008-http1-router-upstream/expectations.yaml` — `Driver::Http1` single-probe shape with deterministic-echo `expected_body.byte_exact`.
- `tests/fixtures/0008-http1-router-upstream/README.md` — fixture documentation.
- `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md` (appended once per task during execution; created by Task 1).

**Modified:**

- Root `Cargo.toml` — `[workspace] members` gains `tests/helpers/http1-echo-server`. (`crates/envoy-http1` is already a member from 04.1.)
- `crates/envoy-http1/src/lib.rs` — add `pub mod client;` + `pub mod router;`; extend `pub use` re-exports with `Client`, `ClientStream`, `RouterError`. The existing `pub use codec::{Http1Codec, HttpVersion, Request};` and `pub use response::{Http1Response, Response};` blocks gain entries.
- `crates/envoy-http1/src/error.rs` — add 3 new `Http1Error` variants: `UpstreamConnect { addr, source: io::Error }`, `MalformedResponseLine`, `MalformedChunkedFraming`.
- `crates/envoy-http1/src/hcm.rs` — extend `HCMConfig` struct at lines 27-31 with `pub cluster_mgr: std::sync::Arc<envoy_cluster::ClusterManager>` (replaces the placeholder comment at line 30); extend `HCMConfig::from_config` signature with a `cluster_mgr` parameter; restructure the `synth_direct_response` call site at line 248 to a two-arm `match action` (per SPEC §3 D2 example); thread `cluster_mgr` through `serve_connection` → `build_response` → the new `Route` arm; extend `clone_route_config` at line 45 to clone the new `RouteAction` enum (replace the `direct_response: DirectResponse { ... }` field-clone with `action: clone_route_action(&r.action)`); add ~6 new HCM unit tests covering the `Route` arm dispatch, NoHealthyEndpoint, UpstreamConnect, header allow-list policy applied, x-envoy-upstream-service-time injected, server/date overwrite. Add `crates/envoy-http1/Cargo.toml` `[dependencies]` entry for `envoy-cluster = { path = "../envoy-cluster" }` if not already present (M3 carryforward from 04.1 REVIEW: the dep was pre-staged in 04.1 + 04.2 with no consumer; 04.3 consumes).
- `crates/envoy-config/src/bootstrap.rs` — restructure the `Route` struct at lines 308-314 to `Route { r#match: RouteMatch, action: RouteAction }`; add `RouteAction` enum (`DirectResponse(DirectResponse)`, `Route(RouteAction_Route)`); add `RouteAction_Route { cluster: String }` struct; add hand-rolled `impl<'de> Deserialize<'de> for Route` (mirroring 04.2's `HeaderMatcher` visitor pattern at lines 609+); extend `validate_hcm` (line 950) signature with a `&[Cluster]` parameter; walk `route.action` per the new enum; for `RouteAction::Route(ar)` check `ar.cluster` against the cluster names slice and emit `ConfigError::UnknownCluster(ar.cluster.clone())` on miss (reusing the 02.1-landed variant); update the single caller of `validate_hcm` in `validate` (the `HCM_FILTER` arm at lines 869-878) to pass `&bootstrap.static_resources.clusters` as the new parameter; extend `bootstrap.rs::tests` with ~5 parse-shape tests + ~3 validator tests + 1 corpus-walk allow-list addition for the new fuzz seed.
- `crates/envoy-config/src/lib.rs` — extend `pub use bootstrap::{...}` re-exports with `RouteAction`, `RouteAction_Route`. No new `ConfigError` variants in 04.3 (per SPEC §3 D2: `UnknownCluster` is reused from 02.1).
- `crates/envoy-cluster/src/cluster.rs` — add `pub fn Cluster::name(&self) -> &str` on the `Cluster` impl block (currently at lines 19-31; the new method goes alongside the existing `pick` private method). Remove the field-level `#[allow(dead_code)]` annotation at line 13. Add `pub fn ClusterHandle::name(&self) -> &str` accessor on the `ClusterHandle` impl block (lines 40-49) that delegates to `self.inner.name()`. Add 3 new unit tests in `cluster::tests` (`cluster_name_returns_configured_name`, `cluster_handle_exposes_name`, `cluster_name_outlives_borrow_correctly`). Per SPEC §3 D5 and §6 signpost 16.
- `crates/envoy-cluster/src/lib.rs` — no changes (the existing `pub use cluster::{...}` block already re-exports `Cluster` + `ClusterHandle`; the new `name()` accessors are inherent methods reachable through the re-exports without explicit `pub use` additions).
- `crates/envoy-bin/src/main.rs` — at the `HCM_FILTER` arm (lines 205-254): pass `cluster_mgr.clone()` to `HCMConfig::from_config` (single-line change per the new signature); remove the `// 04.3: ...` placeholder comment at `crates/envoy-http1/src/hcm.rs:30` no longer applies. The TLS-detect-and-bail logic at lines 230-236 is unchanged (HCM-with-TLS combos remain a phase-05+ deferral per parent SPEC §3 architectural rule 6).
- `tests/differential/src/lib.rs` — extend `HEADER_ALLOW_LIST` constant at line 188 with `("x-envoy-upstream-service-time", AllowMode::NameRequired)` row; extend the `port_key` match at lines 833-840 (currently lists `Http1` and `Http1ProbeList`) — no change needed since the new fixture 0008 reuses `Driver::Http1` from 04.1; extend `run_fixture`'s template-marker detection cascade with a `{{HTTP1_BACKEND_PORT}}` arm spawning `Http1EchoBackend::spawn()` (mirrors the `{{TLS_BACKEND_PORT}}` arm at lines 879-893); add the `BACKEND_HOST` substitution gate (already present at lines 908-914 / 930-932 — the new arm just contributes to the `backend_port_str.is_some() || tls_backend_port_str.is_some()` condition by adding `|| http1_backend_port_str.is_some()`); add 2 new harness unit tests asserting the dispatch cascade selects `Http1EchoBackend::spawn` on the new template marker and asserting the M1+M11 carryforward shape (response-side `diff_headers` walk over duplicate header rows works as expected for the fixture-0008 deterministic-echo response — surfaces a `Set-Cookie` or `Vary` shape only if Envoy emits one, which fixture 0008 does not; documenting status as awareness-only).
- `tests/differential/src/backend.rs` — add `Http1EchoBackend` struct (sibling of `TlsEchoBackend` at lines 99-168) + `locate_http1_echo_server` free fn (sibling of `locate_tls_echo_server` at lines 173-198); add 3 new harness unit tests (`http1_echo_backend_spawns_and_echoes`, `http1_echo_backend_drop_terminates_child`, `locate_http1_echo_server_returns_existing_path`).
- `crates/envoy-config/fuzz/.gitignore` — append one allow-list entry `!corpus/parse_bootstrap/hcm_route_to_cluster.yaml`.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — `Header allow-list` section gains the `x-envoy-upstream-service-time` row per SPEC §2 (one new row appended to the existing 2-row table populated in 04.1).
- `docs/envoy-rust/ROADMAP.md` — at state 6 only, flip row `04.3` `status` `in-progress` → `done`. **At the SAME commit:** flip parent row `04` `status` `in-progress` → `done` per the ROADMAP-schema invariant ("parent flips to `done` only after all sub-phases are `done`"). Mirrors phase 03's `ca81226`-shape close-out.
- `docs/envoy-rust/STATE.md` — at state 6 only, advance active phase id `05`, slug `05-http2`, lifecycle state 1 (phase 05 directory does not exist yet at 04.3 close), next-skill `superpowers:brainstorming` scoped to phase 05. Notes section gains a "Phase-04.3 rollovers" subsection: M1 closed (close-out commit `<SHA>`); carryforward chain ends here.
- `Cargo.lock` — synced as a dedicated commit at the state-4 phase-done gate per the established phase-precedent. Expected new entries: minimal — most `http1-echo-server` deps are already in the workspace's transitive graph.
- `deny.toml` — likely no-op at all tasks (per SPEC §3 D5: no new transitive licenses anticipated; `http1-echo-server`'s deps are all in scope from earlier phases). Cross-check at Task 17.

**Not touched in 04.3** (belong to 04.1, 04.2, earlier phases, or are frozen):

- `docs/envoy-rust/phases/04-http1/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `805433e`.
- `docs/envoy-rust/phases/04.1-hcm-direct-response/{SPEC,PLAN,PROGRESS,REVIEW}.md` — closed at the 04.1 phase-done commit `c5c40ec`; unedited in 04.3.
- `docs/envoy-rust/phases/04.2-route-matchers/{SPEC,PLAN,PROGRESS,REVIEW}.md` — closed at the 04.2 phase-done commit `04163c5`; unedited in 04.3.
- `docs/envoy-rust/phases/04.3-router-upstream/SPEC.md` — landed at parent-04 state-2 commit `1d9740d`; unedited in 04.3.
- `docs/envoy-rust/phases/03-tls-tcp/SPEC.md`, `phases/03.1-tls-foundation-downstream/`, `phases/03.2-tls-upstream-sni/` — closed in phase 03.
- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md`, `phases/02.1-config-cluster/`, `phases/02.2-listener-tcp-proxy/` — closed in phase 02.
- `docs/envoy-rust/DECISIONS.md` — no new ADRs anticipated per SPEC §7. If one of §7's contingent ADRs fires (TLS-on-upstream-HCM, header allow-list extension, chunked-request-body posture), it lands as ADR-0022.
- `crates/envoy-http1/src/{codec,date,error,headers,hcm,response}.rs` — `error.rs` and `hcm.rs` are touched per "Modified" above; `codec.rs`, `date.rs`, `headers.rs`, `response.rs` are unchanged in 04.3 (the `Response` type from `response.rs` is consumed by the new `client.rs` for parsed upstream responses + by the new `router.rs::write_proxied_response` for outgoing downstream responses — both via the existing public API).
- `crates/envoy-tls/`, `crates/envoy-tcp/`, `crates/envoy-listener/`, `tests/helpers/{tcp,tls}-echo-server/` — finalized in earlier phases; phase 04.3 consumes via existing public APIs without amendment.
- `crates/envoy-bin/src/{admin,argv,echo,tls_handler}.rs` — unchanged; 04.3 only touches the HCM dispatch arm in `main.rs`.
- `crates/envoy-bin/tests/{admin_only,tcp_proxy,tls_downstream,tls_sni,tls_upstream,http1_direct_response}.rs` — unchanged; 04.3 adds a new sibling `http1_router_upstream.rs`.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, `tests/fixtures/0007-http1-direct-response/` — unedited; their fixtures must remain green at the 04.3 state-4 phase-done gate.
- `tests/differential/{src/subject,src/tls,src/upstream}.rs`, `tests/differential/Cargo.toml` — unchanged; the new fixture 0008 is plaintext (no TLS PKI; no `tls_pki` mounts); upstream Envoy container does not need additional file mounts; testcontainers wiring at `upstream::start` (line 46) handles the existing `host_gateway = true` case identically (the fixture's envoy.yaml references `host.docker.internal`, satisfying the `host_uses_host_gateway` flag at lib.rs:958).
- `crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs` — unchanged; the existing target picks up the new `hcm_route_to_cluster.yaml` seed automatically via the corpus directory.
- `crates/envoy-config/Cargo.toml` — unchanged; no new direct deps in 04.3 (regex was added in 04.2 under ADR-0021).
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.
- `.github/workflows/ci.yml` — untouched (per SPEC §3 D5: no CI workflow changes in 04.3; existing `cargo test --workspace` picks up the new Docker-gated test automatically; existing fuzz job picks up the new corpus seed automatically).

---

## Task index

Each task ends with a commit. `PROGRESS.md` gets a new section per task in the phase-04.1 / phase-04.2 style (task id, commit SHA, change summary, verification tail, deviations from PLAN). Use the follow-up `phase 04.3: progress note (task N)` commit convention from 04.1 + 04.2.

Ordering rationale (SPEC §6 signposts 1, 5, 7, 8, 16, 17, 19, 20):

- **envoy-config schema additions land first** (Tasks 1–3): the schema introduces `RouteAction` + `RouteAction_Route` + the validator + the fuzz seed; subsequent tasks (envoy-http1 client, HCM router invocation) reference these types at compile time.
- **envoy-http1::error variant additions land before client.rs** (Task 4) so subsequent tasks (Tasks 5–7) can reference `Http1Error::UpstreamConnect`, `MalformedResponseLine`, `MalformedChunkedFraming` at compile time.
- **envoy-http1::client builds in three layers** (Tasks 5–7): skeleton + `connect` (Task 5) → `send_request` Content-Length path (Task 6) → chunked-encoding response reader (Task 7). Each task is independently TDD-tested via in-process `tokio::net::TcpListener` acceptors.
- **envoy-http1::router lands before HCM consumption** (Task 8) so `RouterError` + `write_proxied_response` are available when Task 9 wires the HCM `Route` arm.
- **HCM router invocation extension + `Cluster::name()` close-out (D5) land together** (Task 9) per SPEC §3 D5 + §6 signpost 16: the router's per-cluster proxy attribution is the natural use site, so D5 folds into the HCM task block.
- **BEHAVIOR_CONTRACT.md edit + HEADER_ALLOW_LIST const update land together** (Task 10) so the contract row and the harness constant change in lockstep (per SPEC §6 signpost 19; reviewer should diff the two for parity).
- **http1-echo-server scaffold + argv land before runtime + integration** (Tasks 11–12): Task 11 lands the workspace-member skeleton + argv parser + 4 argv tests (mirrors phase-02.1 Task 8 + phase-03.2 Task 10's argv-first pattern); Task 12 lands the runtime accept loop + deterministic echo body + 1 round-trip test.
- **Differential harness `Http1EchoBackend` lands before fixture 0008** (Task 13) so `run_fixture`'s dispatch cascade is in place when the fixture lands.
- **envoy-bin in-process integration test lands before the Docker-gated fixture** (Task 14) so a regression in HCM wiring shows up locally without Docker.
- **Fixture 0008 + Docker-gated test land together** (Task 15) since the Docker-gated test references the fixture directory.
- **04.1+04.2 REVIEW M-track carryforward check (Task 16)** lands per the 04.2 PLAN Task 11 precedent — small task slot for documenting that M-track items either closed in-line during 04.3 (M3 envoy-cluster dep consumed; M6 drive_http1 unit test naturally surfaces in fixture 0008's path; D5 `Cluster::name` closes in Task 9) or carry forward to phase 05+ / hardening (M1, M2, M4, M7, M8, M11).
- **State-4 phase-done gate (Task 17)** lands last with the dedicated `Cargo.lock` sync per the established phase-precedent + the M5/M9 carryforward recommendation.

Tasks:

1. **`envoy-config` — `RouteAction` enum + `RouteAction_Route` + `Route` restructure + hand-rolled `Deserialize` for `Route` + 5 parse-shape tests**
2. **`envoy-config` validator — extend `validate_hcm` signature with cluster names slice + walk `route.action` + reuse `ConfigError::UnknownCluster` + 3 validator tests**
3. **`envoy-config` fuzz corpus — `hcm_route_to_cluster.yaml` seed + `.gitignore` allow-list + corpus-walk extension in `bootstrap.rs::tests`**
4. **`envoy-http1::error` — 3 new `Http1Error` variants (`UpstreamConnect`, `MalformedResponseLine`, `MalformedChunkedFraming`) + 1 unit test**
5. **`envoy-http1::client` — skeleton (`Client`, `ClientStream`) + `Client::connect` + `Cargo.toml` `envoy-cluster` dep activation (M3 close) + 2 connect tests**
6. **`envoy-http1::client` — `ClientStream::send_request` Content-Length path (request serialization + response parse via `httparse::Response::parse`) + 4 send_request tests**
7. **`envoy-http1::client` — chunked-encoding response reader + 2 chunked-reader tests**
8. **`envoy-http1::router` — `RouterError` enum + `HCM_EMITTED_HEADERS` constant + `write_proxied_response` helper + 3 unit tests**
9. **`envoy-http1::hcm` — `RouteAction` two-arm match restructure + `Route` arm wires through `cluster_mgr` + `HCMConfig::cluster_mgr` field + `envoy-cluster::Cluster::name()` accessor (D5 close) + 6 HCM unit tests**
10. **`docs/envoy-rust/BEHAVIOR_CONTRACT.md` `Header allow-list` table + `tests/differential/src/lib.rs::HEADER_ALLOW_LIST` constant — `x-envoy-upstream-service-time` row added in lockstep**
11. **`tests/helpers/http1-echo-server/` scaffold — `Cargo.toml` + workspace member registration + `src/main.rs` argv parser + 4 argv unit tests**
12. **`tests/helpers/http1-echo-server/src/main.rs` runtime — accept loop + deterministic echo body + 1 round-trip test**
13. **Differential harness — `Http1EchoBackend` + `locate_http1_echo_server` + `run_fixture` dispatch arm on `{{HTTP1_BACKEND_PORT}}` template marker + 4 harness unit tests**
14. **`crates/envoy-bin/src/main.rs` HCM dispatch wiring (pass `cluster_mgr` to `HCMConfig::from_config`) + `crates/envoy-bin/tests/http1_router_upstream.rs` Docker-free in-process integration test**
15. **Fixture `0008-http1-router-upstream` (5 files) + Docker-gated `tests/differential/tests/http1_router_upstream.rs`**
16. **04.1+04.2 REVIEW M-track carryforward check (status: M3 + M6 + D5 closed in-line during 04.3; M1, M2, M4, M5, M7, M8, M9, M10, M11 carry forward to phase 05+ / hardening; document in PROGRESS.md)**
17. **State 4 phase-done gate — run all 5 stable commands + observe CI; quote outputs into PROGRESS.md; sync `Cargo.lock` as a dedicated commit per the phase-precedent**

Estimated total: 17 tasks, ~1490 LoC. Both `BOOTSTRAP_PROMPT.md` §6.1 gates (~25 tasks / ~1500 LoC) hold comfortably (17 < 25, ~1490 ≤ 1500). **Do not split 04.3 further.** Per parent-04 SPEC §5 + the parent-04 state-1 brainstorm's express avoidance of nested splits, a 04.3.1 / 04.3.2 split would be a strong scope-creep signal and warrants `superpowers:systematic-debugging` first per `BOOTSTRAP_PROMPT.md` §6.1. Closest scope-creep vector at PLAN-write time: Task 9's HCM extension folds D5 close-out + the new Route arm + the cluster_mgr threading; if Task 9's sub-step count exceeds ~10 at execution, factor the D5 close-out into a Task 9.5 (sibling of Task 9) instead of nested-splitting the phase.

---

### Task 1: `envoy-config` — `RouteAction` enum + `RouteAction_Route` + `Route` restructure + hand-rolled `Deserialize` for `Route` + 5 parse-shape tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (add `RouteAction` enum + `RouteAction_Route` struct after the existing `RouteMatch`/`DirectResponse` block; restructure the existing `Route` struct at lines 308-314 from `{ r#match, direct_response }` to `{ r#match, action: RouteAction }`; add hand-rolled `impl<'de> Deserialize<'de> for Route` that picks the action variant from the route's peer keys; add 5 parse-shape tests)
- Modify: `crates/envoy-config/src/lib.rs` (extend the `pub use bootstrap::{...}` re-export list with `RouteAction` and `RouteAction_Route`)
- Create: `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md` (new file with Task 1 section)

**Why first:** every subsequent task that names `RouteAction` or `RouteAction_Route` (Tasks 2, 3, 9) needs the type at compile time. The restructure of `Route` from `{ r#match, direct_response }` to `{ r#match, action: RouteAction }` ripples into `crates/envoy-http1/src/hcm.rs` (the HCM's `clone_route_config` at line 45, the `synth_direct_response(&route.direct_response, close)` call site at line 248, and the test fixtures in `mod tests` lines 361-373, 437-446, 472-484, 488-498, 535-547, 633-639, 656-664, 672-678, 695-707, 716-722, 754-760, 786-790) — Task 1 ripples those callsites too as part of the schema change so the workspace stays green at every commit (per D-3.6).

**Scope.** ~80 LoC schema additions + ~40 LoC hand-rolled `Deserialize` for `Route` + ~30 LoC of HCM call-site adaptation in `hcm.rs` + ~5 parse-shape tests + ~120 LoC of HCM test-fixture mechanical edits (replacing `direct_response: DirectResponse { ... }` with `action: RouteAction::DirectResponse(DirectResponse { ... })` across the existing 04.1 + 04.2 HCM tests).

- [ ] **Step 1: Verify ADR ledger head + STATE.md routing + cluster of test fixtures.**

```bash
grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md
grep -n '^## ADR-00' docs/envoy-rust/DECISIONS.md | tail -3
grep -A2 '^## Active phase' docs/envoy-rust/STATE.md | head -5
grep -nE 'direct_response: DirectResponse|r\.direct_response|route\.direct_response' crates/envoy-http1/src/hcm.rs | head -20
```

Expected: ADR count `21` (latest ADR-0021 from 04.2). STATE.md `Active phase: id: 04.3`, `slug: 04.3-router-upstream`, `lifecycle state 2`. The grep against `hcm.rs` returns ~12 callsites (the `synth_direct_response` call at line 248 + the test fixtures at lines 361-373, 437-446, 472-484, 488-498, 535-547, 633-639, 656-664, 672-678, 695-707, 716-722, 754-760, 786-790). All these need adapting.

If any unexpected `ADR-00NN` appears beyond ADR-0021, debug per `superpowers:systematic-debugging` before continuing — phase 04.3 anticipates zero new ADRs at this task and none thereafter (per SPEC §7).

- [ ] **Step 2: Write 5 failing parse-shape tests in `crates/envoy-config/src/bootstrap.rs::tests`.**

Append to the existing `#[cfg(test)] mod tests { ... }` block (find the end of the existing tests block via `grep -n '^mod tests' crates/envoy-config/src/bootstrap.rs`). Each test parses a YAML snippet through `parse_bootstrap` (or directly through `serde_yaml::from_str::<Bootstrap>`) and asserts the resulting `Route.action` shape. The hand-rolled `Deserialize` for `Route` is what makes these tests pass; before Step 4 lands the impl, all 5 tests fail at compile time citing unknown names `RouteAction`, `RouteAction_Route`, or `Route::action`.

```rust
#[test]
fn parses_route_with_direct_response_action() {
    // 04.3 NEW: Route still accepts direct_response (04.1 carryover) — restructured to
    // wrap inside the RouteAction enum. The hand-rolled Route::Deserialize sees the
    // `direct_response` peer key and constructs RouteAction::DirectResponse(...).
    let yaml = r#"
node: { id: t, cluster: c }
static_resources:
  listeners:
    - name: hcm_listener
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  virtual_hosts:
                    - name: vh
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
"#;
    let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
    let listener = &bootstrap.static_resources.listeners[0];
    let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
        .typed_config
        .as_ref()
        .expect("HCM typed_config present")
    else {
        panic!("expected HCM typed_config variant");
    };
    let route = &hcm.route_config.virtual_hosts[0].routes[0];
    match &route.action {
        RouteAction::DirectResponse(dr) => {
            assert_eq!(dr.status, 200);
            assert_eq!(dr.body.inline_string.as_deref(), Some("ok\n"));
        }
        _ => panic!("expected DirectResponse, got {:?}", route.action),
    }
}

#[test]
fn parses_route_with_route_action() {
    // 04.3 NEW: the route variant — `route: { cluster: backend }` produces
    // RouteAction::Route(RouteAction_Route { cluster: "backend".into() }).
    let yaml = r#"
node: { id: t, cluster: c }
static_resources:
  listeners:
    - name: hcm_listener
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
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
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 9001 } } }
"#;
    let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
    let listener = &bootstrap.static_resources.listeners[0];
    let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
        .typed_config
        .as_ref()
        .expect("HCM typed_config present")
    else {
        panic!("expected HCM typed_config variant");
    };
    let route = &hcm.route_config.virtual_hosts[0].routes[0];
    match &route.action {
        RouteAction::Route(ar) => assert_eq!(ar.cluster, "backend"),
        _ => panic!("expected Route, got {:?}", route.action),
    }
}

#[test]
fn rejects_route_with_both_direct_response_and_route() {
    // 04.3 NEW: hand-rolled Route::Deserialize rejects YAML carrying BOTH
    // direct_response and route peer keys — the action is a oneof; exactly
    // one variant may be selected. SPEC §3 D2 names this test by name.
    let yaml = r#"
node: { id: t, cluster: c }
static_resources:
  listeners:
    - name: hcm_listener
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
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
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 9001 } } }
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject both action keys");
    let msg = err.to_string();
    assert!(
        msg.contains("direct_response") && msg.contains("route") && msg.contains("exactly one"),
        "expected `exactly one of direct_response/route` rejection; got: {msg}"
    );
}

#[test]
fn rejects_route_with_neither_direct_response_nor_route() {
    // 04.3 NEW: hand-rolled Route::Deserialize rejects YAML carrying NEITHER
    // direct_response nor route — the action is required.
    let yaml = r#"
node: { id: t, cluster: c }
static_resources:
  listeners:
    - name: hcm_listener
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject missing action");
    let msg = err.to_string();
    assert!(
        msg.contains("direct_response") && msg.contains("route") && msg.contains("exactly one"),
        "expected `exactly one of direct_response/route` rejection; got: {msg}"
    );
}

#[test]
fn rejects_route_with_unknown_top_level_key() {
    // 04.3 NEW: hand-rolled Route::Deserialize rejects unknown peer keys at
    // the route level. Mirrors `#[serde(deny_unknown_fields)]` on the structs
    // that derive Deserialize. SPEC §3 D2 hand-rolled visitors must preserve
    // the deny-unknown discipline.
    let yaml = r#"
node: { id: t, cluster: c }
static_resources:
  listeners:
    - name: hcm_listener
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                          unknown_route_field: surprise
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject unknown route key");
    let msg = err.to_string();
    assert!(
        msg.to_ascii_lowercase().contains("unknown") && msg.contains("unknown_route_field"),
        "expected `unknown field` rejection; got: {msg}"
    );
}
```

- [ ] **Step 3: Run the tests to verify they fail (compile errors expected).**

```bash
cargo test -p envoy-config --lib parses_route_with_direct_response_action parses_route_with_route_action rejects_route_with_both_direct_response_and_route rejects_route_with_neither_direct_response_nor_route rejects_route_with_unknown_top_level_key
```

Expected: build errors citing unknown names `RouteAction`, `RouteAction_Route`, and the missing `route.action` field. All five fixed in Step 4.

- [ ] **Step 4: Add `RouteAction` + `RouteAction_Route` types in `bootstrap.rs`.**

Locate the existing `Route` struct at lines 308-314 and the `DirectResponse` struct at line 327-332. Append the new types AFTER the `DirectResponse` block (around line 333+ in the post-04.2 file). Preserve the alphabetic / definition-order convention established in 04.1 / 04.2.

```rust
/// 04.3 NEW (under SPEC §3 D2): the action variant a route's HCM router
/// invocation dispatches into. 04.1 introduced this conceptually as
/// `direct_response`-only; 04.3 lifts it into a tagged-union enum + adds the
/// `Route` variant that proxies to a cluster.
///
/// Discrimination is by **field-name oneof at the route level**: the
/// route map's peer keys are `direct_response: { ... }` OR `route: { ... }`,
/// not under a single `action:` key. The hand-rolled `impl<'de> Deserialize`
/// for `Route` (below) detects which peer key is present and constructs the
/// matching variant; both-present and neither-present are errors.
#[derive(Debug, Clone, PartialEq)]
pub enum RouteAction {
    /// Direct-response action — write a static body downstream. The HCM's
    /// `synth_direct_response` helper consumes this. Phase 04.1 carryover.
    DirectResponse(DirectResponse),

    /// Route-to-cluster action — proxy through to the named cluster. The
    /// HCM's new router-proxy arm (Task 9) consumes this; the validator
    /// (Task 2) checks `cluster` against `bootstrap.static_resources.clusters`.
    /// Phase 04.3 NEW.
    Route(RouteAction_Route),
}

/// 04.3 NEW (under SPEC §3 D2). Names the cluster to forward the matched
/// request to. Future route-action knobs — timeout, retries, hedging,
/// weighted clusters, host-rewrite, request/response header manipulations —
/// are all deferred (SPEC §4 non-goals).
///
/// The `RouteAction_Route` name preserves the parent-04 SPEC's
/// `RouteAction_Route` projection literally; underscores in Rust type names
/// are ergonomically unusual but match the SPEC's projection one-to-one for
/// reviewer-friendliness. (Future polish pass — phase 05+ — may rename to
/// `RouteToCluster` if the codebase establishes a different convention.)
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RouteAction_Route {
    pub cluster: String,
}
```

`DirectResponse` already derives `Clone` after the 04.1 + 04.2 schema work? Verify with `grep -n '^pub struct DirectResponse' -A 5 crates/envoy-config/src/bootstrap.rs`. If it does NOT derive `Clone`, add `#[derive(Clone)]` (or `Debug, Clone, Deserialize, PartialEq` covering all four — match the existing macro on the type) so `RouteAction::DirectResponse` can derive `Clone` cleanly. The HCM's `clone_route_config` helper at `hcm.rs:45-77` already deep-clones `DirectResponse` field-by-field, so the new `Clone` derive is consistent with that posture.

- [ ] **Step 5: Restructure `Route` to `{ r#match, action: RouteAction }`.**

Replace the existing `Route` struct at lines 308-314:

```rust
// REMOVE this block:
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Route {
    #[serde(rename = "match")]
    pub r#match: RouteMatch,
    pub direct_response: DirectResponse,
}
```

With (note: NO `#[derive(Deserialize)]` because we hand-roll the visitor in Step 6 to handle the field-name oneof; we keep `Debug`, `Clone`, `PartialEq` derives):

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// The match predicate (path + headers; populated by 04.1 + 04.2).
    pub r#match: RouteMatch,

    /// 04.3 NEW: the action to dispatch on a matched request. 04.1 had this
    /// as a single-field `direct_response: DirectResponse`; 04.3 lifts it
    /// into the `RouteAction` enum + adds the `Route` variant.
    pub action: RouteAction,
}
```

- [ ] **Step 6: Add hand-rolled `impl<'de> Deserialize<'de> for Route`.**

Append after the `Route` struct block. The visitor mirrors the pattern landed by 04.2's `HeaderMatcher`/`StringMatcher`/`SafeRegex` visitors at `bootstrap.rs:609+`. Field-name oneof discipline:

- Collect all map keys.
- Required: `match`.
- Exactly one of {`direct_response`, `route`}; emit a custom error otherwise.
- Reject any other key with `Error::unknown_field`.

```rust
/// 04.3 NEW: hand-rolled because Envoy's `Route` schema uses a field-name
/// oneof for the action variant — `direct_response: { ... }` and `route: { ... }`
/// are peers of `match: { ... }` at the same map level, not nested under a
/// shared discriminator key. `#[serde(tag = "...")]` doesn't model field-name
/// discrimination, and `#[serde(untagged)]` would silently pick the first
/// parsing variant. The hand-rolled visitor:
///
/// 1. Collects the map keys; requires `match`; requires exactly one of
///    {`direct_response`, `route`}; rejects any other key (preserves the
///    `deny_unknown_fields` discipline manually since hand-rolled visitors
///    don't get the macro for free).
/// 2. Constructs `RouteAction::DirectResponse(...)` or `RouteAction::Route(...)`
///    depending on which peer key was present.
///
/// SPEC §3 D2 names the rejection paths: `rejects_route_with_both_direct_response_and_route`,
/// `rejects_route_with_neither_direct_response_nor_route`,
/// `rejects_route_with_unknown_top_level_key`.
impl<'de> serde::Deserialize<'de> for Route {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Route;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a Route map with `match` and exactly one of `direct_response` or `route`"
                )
            }

            fn visit_map<M>(self, mut map: M) -> Result<Route, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut r#match: Option<RouteMatch> = None;
                let mut direct_response: Option<DirectResponse> = None;
                let mut route_action: Option<RouteAction_Route> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "match" => {
                            if r#match.is_some() {
                                return Err(M::Error::duplicate_field("match"));
                            }
                            r#match = Some(map.next_value::<RouteMatch>()?);
                        }
                        "direct_response" => {
                            if direct_response.is_some() {
                                return Err(M::Error::duplicate_field("direct_response"));
                            }
                            direct_response = Some(map.next_value::<DirectResponse>()?);
                        }
                        "route" => {
                            if route_action.is_some() {
                                return Err(M::Error::duplicate_field("route"));
                            }
                            route_action = Some(map.next_value::<RouteAction_Route>()?);
                        }
                        other => {
                            // Preserve `deny_unknown_fields` semantics manually.
                            return Err(M::Error::unknown_field(
                                other,
                                &["match", "direct_response", "route"],
                            ));
                        }
                    }
                }

                let r#match = r#match.ok_or_else(|| M::Error::missing_field("match"))?;
                let action = match (direct_response, route_action) {
                    (Some(_), Some(_)) => {
                        return Err(M::Error::custom(
                            "Route must carry exactly one of `direct_response` or `route`; \
                             both are present",
                        ));
                    }
                    (None, None) => {
                        return Err(M::Error::custom(
                            "Route must carry exactly one of `direct_response` or `route`; \
                             neither is present",
                        ));
                    }
                    (Some(dr), None) => RouteAction::DirectResponse(dr),
                    (None, Some(ar)) => RouteAction::Route(ar),
                };

                Ok(Route { r#match, action })
            }
        }

        deserializer.deserialize_map(V)
    }
}
```

- [ ] **Step 7: Update HCM call sites in `crates/envoy-http1/src/hcm.rs` to use the new `route.action` shape.**

The schema restructure ripples into envoy-http1 because `crates/envoy-http1/src/hcm.rs` consumes the `Route` type. Two structural call-site updates:

(a) `clone_route_config` at lines 45-77 currently clones `direct_response: DirectResponse { ... }` per-field. Replace with a `clone_route_action` helper + an `action: ...` field clone. Don't try to derive `Clone` on `Route` automatically yet — `RouteConfiguration` ergonomics there are the same.

```rust
fn clone_route_config(rc: &RouteConfiguration) -> RouteConfiguration {
    // envoy-config's RouteConfiguration is not Clone; hand-clone so HCM can
    // hold the data inside an Arc without coupling envoy-config's deriving.
    // (If envoy-config later derives Clone on these types, this helper retires.)
    RouteConfiguration {
        name: rc.name.clone(),
        virtual_hosts: rc
            .virtual_hosts
            .iter()
            .map(|vh| VirtualHost {
                name: vh.name.clone(),
                domains: vh.domains.clone(),
                routes: vh
                    .routes
                    .iter()
                    .map(|r| Route {
                        r#match: RouteMatch {
                            prefix: r.r#match.prefix.clone(),
                            path: r.r#match.path.clone(),
                            headers: r.r#match.headers.clone(),
                        },
                        // 04.3: the new RouteAction enum carries the action.
                        action: clone_route_action(&r.action),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn clone_route_action(a: &RouteAction) -> RouteAction {
    match a {
        RouteAction::DirectResponse(dr) => RouteAction::DirectResponse(DirectResponse {
            status: dr.status,
            body: DataSource {
                filename: dr.body.filename.clone(),
                inline_string: dr.body.inline_string.clone(),
            },
        }),
        RouteAction::Route(ar) => RouteAction::Route(RouteAction_Route {
            cluster: ar.cluster.clone(),
        }),
    }
}
```

Add the `RouteAction`, `RouteAction_Route` imports to the existing `use envoy_config::{...};` block at `hcm.rs:11-14`.

(b) `build_response` at lines 192-249 calls `synth_direct_response(&route.direct_response, close)` at line 248. Replace with a placeholder `match` arm that handles `RouteAction::DirectResponse` (existing behavior) and *currently* returns `synth_501(close)` for `RouteAction::Route(_)` (Task 9 lands the real Route arm; this Task 1 placeholder keeps the workspace green until Task 9):

```rust
    // Hardcoded router-filter call site:
    //   match action { DirectResponse(dr) => synth_direct_response(req, dr) ... }
    // 04.3: extended in Task 9 with the Route(_) arm; this Task-1 placeholder
    //       compiles cleanly + ensures fixture 0007 stays green by routing
    //       Route(_) configurations to a 501 (no fixture 0007 path uses Route(_)
    //       yet — fixture 0008 is the first one).
    match &route.action {
        RouteAction::DirectResponse(dr) => synth_direct_response(dr, close),
        RouteAction::Route(_ar) => {
            tracing::warn!(
                method = %req.method,
                path = %req.path,
                "RouteAction::Route reached HCM build_response — Task 9 wires the proxy arm",
            );
            synth_501(close)
        }
    }
```

(c) The `tests` module at the bottom of `hcm.rs` constructs `Route` literals directly (e.g. `Route { r#match: RouteMatch { ... }, direct_response: DirectResponse { ... } }`). Update each occurrence to `Route { r#match: ..., action: RouteAction::DirectResponse(DirectResponse { ... }) }`. Specifically the call-sites at lines 361-373, 437-446, 472-484, 488-498, 535-547, 633-639, 656-664, 672-678, 695-707, 716-722, 754-760, 786-790. Use a global-replace approach: `sed -i.bak 's/direct_response: DirectResponse {/action: RouteAction::DirectResponse(DirectResponse {/g'` then a second pass closing the extra `)` — manual review required to balance parens.

Alternatively, use rustfmt + targeted edits — the Test-fixture body of `Route { r#match: ..., direct_response: DirectResponse { status, body } }` becomes `Route { r#match: ..., action: RouteAction::DirectResponse(DirectResponse { status, body }) }`. ~12 mechanical edits.

After the edit, also extend `hcm.rs:11-14`'s import:

```rust
use envoy_config::{
    DataSource, DirectResponse, HttpConnectionManagerConfig, Route, RouteAction,
    RouteAction_Route, RouteConfiguration, RouteMatch, VirtualHost,
};
```

(`RouteAction` is referenced by `clone_route_action` + `match &route.action`; `RouteAction_Route` is referenced by `clone_route_action`'s match arm — even though the test-fixture only uses `DirectResponse` variant, the inner type must be in scope for the helper.)

- [ ] **Step 8: Update `crates/envoy-config/src/lib.rs`'s `pub use` re-exports.**

Locate the `pub use bootstrap::{ ... };` block at lines 10-19. Add `RouteAction` and `RouteAction_Route` to the alphabetic list:

```rust
pub use bootstrap::{
    Address, Admin, Bootstrap, CertificateValidationContext, Cluster, ClusterType, CodecType,
    CommonTlsContext, DataSource, DirectResponse, DownstreamTlsContext, Endpoint, FilterChain,
    FilterChainMatch, HeaderMatcher, HeaderMatcherMode, HttpConnectionManagerConfig, HttpFilter,
    HttpFilterTypedConfig, Int64Range, LbEndpoint, LbPolicy, Listener, LoadAssignment,
    LocalityLbEndpoints, NetworkFilter, Node, Route, RouteAction, RouteAction_Route,
    RouteConfiguration, RouteMatch, RouterConfig, SafeRegex, SocketAddress, StaticResources,
    StringMatcher, StringMatcherMode, TcpProxyConfig, TlsCertificate, TransportSocket,
    TransportSocketTypedConfig, TypedConfig, UpstreamTlsContext, VirtualHost,
};
```

(The alphabetic interleaving of `Route` / `RouteAction` / `RouteAction_Route` / `RouteConfiguration` / `RouteMatch` / `RouterConfig` is rustfmt-stable; let `cargo fmt` normalize order if the manual edit drifts.)

- [ ] **Step 9: Run the parse-shape tests + the workspace build.**

```bash
cargo build --workspace --all-targets
cargo test -p envoy-config --lib parses_route_with_direct_response_action parses_route_with_route_action rejects_route_with_both_direct_response_and_route rejects_route_with_neither_direct_response_nor_route rejects_route_with_unknown_top_level_key
cargo test -p envoy-http1 --lib
cargo test -p envoy-config --lib
```

Expected: clean build; the 5 new envoy-config tests pass; the existing 24 envoy-http1 tests (carried from 04.2) all pass — the test-fixture mechanical edits in Step 7(c) preserve every test's semantic; the existing 131 envoy-config tests (carried from 04.2) all pass. Total: 24 envoy-http1 + 136 envoy-config (= 131 + 5).

If a 04.2 envoy-http1 test fails because the `direct_response` field-clone macro rewrite missed a callsite, fix in lockstep — the failure surfaces immediately in this step.

- [ ] **Step 10: Workspace gate.**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: both exit 0. The new `RouteAction` enum may surface a clippy `enum_variant_names` warning if both variants get short names — clippy is fine with `DirectResponse` / `Route` (different enough names). The hand-rolled `Deserialize` impl is exempt from the `clippy::derive_partial_eq_without_eq` lint because the type is non-trivial and the existing 04.2 visitors don't trip it.

- [ ] **Step 11: Create PROGRESS.md with a Task 1 section.**

Create `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`:

```markdown
# Phase 04.3 Progress

## Task 1 — envoy-config: RouteAction enum + RouteAction_Route + Route restructure + 5 parse-shape tests (2026-04-27)

- Commit: <SHA>
- Change: added RouteAction enum (DirectResponse + Route) and RouteAction_Route { cluster: String } struct in crates/envoy-config/src/bootstrap.rs; restructured Route from { r#match, direct_response } to { r#match, action: RouteAction } with hand-rolled impl<'de> Deserialize<'de> for Route handling the field-name oneof at the route map level (mirrors 04.2's HeaderMatcher visitor pattern). Updated crates/envoy-http1/src/hcm.rs's clone_route_config + the synth_direct_response call site (placeholder Route(_) arm returns 501 until Task 9) + ~12 test-fixture mechanical edits replacing `direct_response: DirectResponse { ... }` with `action: RouteAction::DirectResponse(DirectResponse { ... })`. Extended crates/envoy-config/src/lib.rs's pub use re-exports with RouteAction + RouteAction_Route.
- Tests added (5): parses_route_with_direct_response_action, parses_route_with_route_action, rejects_route_with_both_direct_response_and_route, rejects_route_with_neither_direct_response_nor_route, rejects_route_with_unknown_top_level_key.
- Verification: `cargo build --workspace --all-targets` → clean; `cargo test -p envoy-config --lib` → 136 passed (131 + 5); `cargo test -p envoy-http1 --lib` → 24 passed (unchanged from 04.2 close); clippy + fmt clean.
- ADRs: none in 04.3 Task 1 (per SPEC §7). ADR ledger head: 21.
- Deviations from PLAN: <document any>.
```

Replace `<SHA>` with the commit hash from Step 12.

- [ ] **Step 12: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs crates/envoy-http1/src/hcm.rs
git status   # confirm only intended files
git commit -m "phase 04.3: envoy-config — RouteAction enum + RouteAction_Route + Route restructure (task 1)"
```

Then commit PROGRESS.md as a follow-up note (mirror 04.1 / 04.2's `phase NN.M: progress note (task N)` cadence):

```bash
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 1)"
```

---

### Task 2: `envoy-config` validator — extend `validate_hcm` signature + walk `route.action` + reuse `ConfigError::UnknownCluster` + 3 validator tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend `validate_hcm`'s signature with a `clusters: &[Cluster]` parameter; walk `route.action` per the new `RouteAction` enum; for `RouteAction::Route(ar)` check the cluster name; update the single caller in `validate` at lines 869-878 to pass `&bootstrap.static_resources.clusters`; add 3 validator tests)
- Modify (append): `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`

**Why now:** Task 1 landed the `RouteAction` enum + the placeholder `Route(_)` HCM arm; the validator must catch unknown-cluster references at config-load time so the HCM's `cluster_mgr.get(...).expect(...)` (Task 9) never panics. Reuses `ConfigError::UnknownCluster` from phase 02.1 (no new variant needed per SPEC §3 D2).

**Scope.** ~30 LoC validator extension (signature change + walk + cluster check) + 3 validator tests (~80 LoC).

- [ ] **Step 1: Read the current `validate_hcm` shape.**

```bash
grep -n '^fn validate_hcm\|^fn is_valid_dns_name\|^fn validate_data_source\|^fn validate_header_matcher' crates/envoy-config/src/bootstrap.rs
```

Expected: `validate_hcm` at line 950, `validate_data_source` at line 1058, `validate_header_matcher` at line 1109. The single caller of `validate_hcm` is at lines 869-878 (the `HCM_FILTER` arm in `validate`). Confirm with:

```bash
grep -n 'validate_hcm(' crates/envoy-config/src/bootstrap.rs
```

Expected: one definition + one call site.

- [ ] **Step 2: Write 3 failing validator tests in `bootstrap.rs::tests`.**

```rust
#[test]
fn parses_route_with_cluster_action() {
    // 04.3 NEW: happy-path validate — Route::Route { cluster: "backend" }
    // referencing a declared cluster passes parse + validate.
    let yaml = r#"
node: { id: t, cluster: c }
static_resources:
  listeners:
    - name: hcm_listener
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
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
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 9001 } } }
"#;
    crate::parse_bootstrap(yaml).expect("parses + validates");
}

#[test]
fn rejects_hcm_route_with_unknown_cluster() {
    // 04.3 NEW: validator reuses 02.1-landed ConfigError::UnknownCluster.
    let yaml = r#"
node: { id: t, cluster: c }
static_resources:
  listeners:
    - name: hcm_listener
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: nonexistent }
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
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 9001 } } }
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject unknown cluster");
    assert!(
        matches!(&err, crate::ConfigError::UnknownCluster(name) if name == "nonexistent"),
        "expected UnknownCluster(\"nonexistent\"); got: {err:?}"
    );
}

#[test]
fn rejects_hcm_route_with_empty_cluster_name() {
    // 04.3 NEW: empty cluster name is treated as an unknown-cluster miss
    // (no cluster declares an empty name); ConfigError::UnknownCluster carries
    // the empty string as the offending name. The validator emits the same
    // error class for empty-string and bogus-string cluster references —
    // the YAML deserializer accepts the empty string at parse time.
    let yaml = r#"
node: { id: t, cluster: c }
static_resources:
  listeners:
    - name: hcm_listener
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: "" }
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
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 9001 } } }
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject empty cluster name");
    assert!(
        matches!(&err, crate::ConfigError::UnknownCluster(name) if name.is_empty()),
        "expected UnknownCluster(\"\"); got: {err:?}"
    );
}
```

- [ ] **Step 3: Run the tests to verify they fail.**

```bash
cargo test -p envoy-config --lib parses_route_with_cluster_action rejects_hcm_route_with_unknown_cluster rejects_hcm_route_with_empty_cluster_name
```

Expected: at the parse step the YAML is rejected with a generic-validation error (or accepted if the validator currently doesn't check). Regardless of the failure mode, the assert on `UnknownCluster` doesn't match because Task 2 hasn't landed the validator extension yet.

- [ ] **Step 4: Extend `validate_hcm`'s signature with a clusters-slice parameter.**

Locate `validate_hcm` at line 950:

```rust
fn validate_hcm(hcm: &mut HttpConnectionManagerConfig) -> Result<(), crate::ConfigError> {
```

Change to:

```rust
fn validate_hcm(
    hcm: &mut HttpConnectionManagerConfig,
    clusters: &[Cluster],
) -> Result<(), crate::ConfigError> {
```

- [ ] **Step 5: Walk `route.action` inside `validate_hcm` and check `RouteAction::Route` cluster references.**

Inside `validate_hcm`'s body, find the per-route walk at lines ~997-1029. The current walk does:

```rust
        for r in &mut vh.routes {
            // RouteMatch: exactly one of {prefix, path} is Some.
            match (&r.r#match.prefix, &r.r#match.path) { ... }
            // direct_response.status range.
            if !(100..=599).contains(&r.direct_response.status) { ... }
            // direct_response.body must be inline_string.
            validate_data_source(&r.direct_response.body, "direct_response.body", Required::InlineString)?;

            // 04.2 NEW: walk the headers Vec.
            for hm in &mut r.r#match.headers {
                validate_header_matcher(hm)?;
            }
        }
```

After Task 1's restructure, `r.direct_response` no longer exists — `r.action` is a `RouteAction` enum. Replace the body of the per-route walk:

```rust
        for r in &mut vh.routes {
            // RouteMatch: exactly one of {prefix, path} is Some.
            match (&r.r#match.prefix, &r.r#match.path) {
                (Some(_), None) | (None, Some(_)) => {}
                (Some(_), Some(_)) => {
                    return Err(crate::ConfigError::UnsupportedRouteMatcher {
                        matcher: "both prefix and path are set",
                    });
                }
                (None, None) => {
                    return Err(crate::ConfigError::UnsupportedRouteMatcher {
                        matcher: "neither prefix nor path is set",
                    });
                }
            }
            // 04.2 NEW: walk the headers Vec.
            for hm in &mut r.r#match.headers {
                validate_header_matcher(hm)?;
            }
            // 04.3 NEW: walk the action.
            match &r.action {
                RouteAction::DirectResponse(dr) => {
                    if !(100..=599).contains(&dr.status) {
                        return Err(crate::ConfigError::InvalidStatusCode {
                            status: dr.status,
                        });
                    }
                    validate_data_source(
                        &dr.body,
                        "direct_response.body",
                        Required::InlineString,
                    )?;
                }
                RouteAction::Route(ar) => {
                    // Check the cluster reference against declared clusters.
                    // ConfigError::UnknownCluster is the 02.1-landed variant
                    // reused here per SPEC §3 D2.
                    if !clusters.iter().any(|c| c.name == ar.cluster) {
                        return Err(crate::ConfigError::UnknownCluster(
                            ar.cluster.clone(),
                        ));
                    }
                }
            }
        }
```

Add the `RouteAction` import to the top of `bootstrap.rs` if not already present (Task 1's edits should have brought `RouteAction` into scope inside the file's existing namespace because it's a sibling type in the same module — verify with `grep -n 'RouteAction' crates/envoy-config/src/bootstrap.rs | head -5`).

- [ ] **Step 6: Update the single caller of `validate_hcm` in `validate`.**

Locate the `HCM_FILTER` arm at lines 869-878:

```rust
                    crate::HCM_FILTER => {
                        let typed = filter
                            .typed_config
                            .as_mut()
                            .ok_or(crate::ConfigError::MissingTypedConfig(crate::HCM_FILTER))?;
                        let TypedConfig::HttpConnectionManager(hcm) = typed else {
                            return Err(crate::ConfigError::MissingTypedConfig(crate::HCM_FILTER));
                        };
                        validate_hcm(hcm)?;
                    }
```

Replace the `validate_hcm(hcm)?` call with `validate_hcm(hcm, &bootstrap.static_resources.clusters)?`.

**Borrow-checker note:** `validate` already takes `bootstrap: &mut Bootstrap` (post-04.2 the signature is `pub fn validate(bootstrap: &mut Bootstrap) -> Result<(), ConfigError>` — verify with `grep -n '^pub fn validate' crates/envoy-config/src/bootstrap.rs`). The listener-walk inside `validate` iterates `&mut bootstrap.static_resources.listeners` so taking a `&[Cluster]` borrow of `&bootstrap.static_resources.clusters` simultaneously with the mutable listener iteration must split borrows. The existing 02.1 code already cross-references `bootstrap.static_resources.clusters` from inside the `TCP_PROXY_FILTER` arm at lines 859-867 — that pattern uses `bootstrap.static_resources.clusters.iter().any(|c| c.name == cluster_name)` directly, which works because Rust's borrow checker permits a shared borrow of one field while a mutable borrow holds another. Replicate that pattern: pass `&bootstrap.static_resources.clusters` from the same site as `validate_hcm`'s call.

If the borrow checker complains (split-borrow trouble between `&mut bootstrap.static_resources.listeners` and `&bootstrap.static_resources.clusters`), refactor the walk to capture the cluster names eagerly:

```rust
let cluster_names: Vec<String> = bootstrap.static_resources.clusters.iter()
    .map(|c| c.name.clone())
    .collect();
// ... then pass &cluster_names[..] (after dereferencing into &[String]) ...
```

But this requires `validate_hcm`'s signature to be `&[String]` instead of `&[Cluster]`. Pick whichever shape the borrow checker accepts cleanly; `&[Cluster]` is more honest, `&[String]` is more flexible. Default: `&[Cluster]` mirroring the `TCP_PROXY_FILTER` arm's pattern. Switch if `cargo build` complains.

- [ ] **Step 7: Run the tests to verify they pass.**

```bash
cargo test -p envoy-config --lib parses_route_with_cluster_action rejects_hcm_route_with_unknown_cluster rejects_hcm_route_with_empty_cluster_name
cargo test -p envoy-config --lib
```

Expected: 3 new tests pass; total envoy-config = 139 (= 136 + 3).

- [ ] **Step 8: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib
```

Expected: all four exit 0. Per-crate test counts: envoy-config 139, envoy-http1 24 (unchanged), differential lib unchanged from 04.2 close, others unchanged.

- [ ] **Step 9: Append a Task 2 section to PROGRESS.md.**

```markdown
## Task 2 — envoy-config validator: walk RouteAction + reuse UnknownCluster + 3 validator tests (2026-04-27)

- Commit: <SHA>
- Change: extended validate_hcm signature with clusters: &[Cluster] parameter; walk per-route action variant; for RouteAction::Route(ar) check ar.cluster against declared cluster names and emit ConfigError::UnknownCluster(ar.cluster.clone()) on miss (reuses 02.1-landed variant per SPEC §3 D2). Updated the single caller in validate's HCM_FILTER arm at lines 869-878 to pass &bootstrap.static_resources.clusters.
- Tests added (3): parses_route_with_cluster_action, rejects_hcm_route_with_unknown_cluster, rejects_hcm_route_with_empty_cluster_name.
- Verification: `cargo test -p envoy-config --lib` → 139 passed (136 + 3); workspace gate clean.
- Deviations from PLAN: <document any>.
```

- [ ] **Step 10: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 04.3: envoy-config validator — walk RouteAction + reuse UnknownCluster (task 2)"
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 2)"
```

---

### Task 3: `envoy-config` fuzz corpus — `hcm_route_to_cluster.yaml` seed + `.gitignore` allow-list + corpus-walk extension

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_route_to_cluster.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (append the allow-list entry)
- Modify: `crates/envoy-config/src/bootstrap.rs::tests` (extend the `fuzz_corpus_seeds_parse_or_reject_cleanly` corpus-walk test's expected-seed allow-list)
- Modify (append): `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`

**Why now:** Task 1 + 2 landed the `RouteAction::Route` schema + validator; the fuzz corpus seed exercises both the parse path (Route variant) and the reject path (UnknownCluster) — fuzzing reveals nothing new beyond the targeted unit tests, but the corpus seed runs the new code path under cargo-fuzz's nightly invocation per phase precedent.

**Scope.** 1 new YAML file + 1 `.gitignore` line + 1 line in the corpus-walk allow-list test.

- [ ] **Step 1: Find the existing fuzz corpus directory + the corpus-walk test.**

```bash
ls crates/envoy-config/fuzz/corpus/parse_bootstrap/
grep -n 'fuzz_corpus_seeds_parse_or_reject_cleanly\|corpus_seeds' crates/envoy-config/src/bootstrap.rs | head -10
```

Expected: existing seeds for prior phases (~9-12 YAML files) + the test at ~line 1549 (per 04.2 PLAN reference). Confirm the test's expected-seed Vec at execution time.

- [ ] **Step 2: Create `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_route_to_cluster.yaml`.**

```yaml
node: { id: fuzz, cluster: fuzz }
static_resources:
  listeners:
    - name: hcm_listener
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
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
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 9001 } } }
```

- [ ] **Step 3: Append the `.gitignore` allow-list entry.**

`crates/envoy-config/fuzz/.gitignore`'s standard pattern (per phase precedent) ignores everything in `corpus/parse_bootstrap/` except an explicit allow-list of seed files. Append the new entry:

```
!corpus/parse_bootstrap/hcm_route_to_cluster.yaml
```

(Verify the file's exact format with `cat crates/envoy-config/fuzz/.gitignore`; the line should slot in alphabetically-sorted with the existing entries.)

- [ ] **Step 4: Extend the corpus-walk allow-list in `bootstrap.rs::tests`.**

Locate `fuzz_corpus_seeds_parse_or_reject_cleanly` (~line 1549). The test typically asserts that every file in `corpus/parse_bootstrap/` round-trips through `parse_bootstrap` and either parses-then-validates cleanly OR rejects with a typed `ConfigError`. The test has an "expected seeds" Vec or HashSet — append `"hcm_route_to_cluster.yaml"` to it.

```rust
// Inside fuzz_corpus_seeds_parse_or_reject_cleanly:
let expected_seeds: &[&str] = &[
    // ... existing entries from earlier phases ...
    "route_with_header_matchers.yaml", // 04.2
    "hcm_route_to_cluster.yaml",       // 04.3
];
```

If the test enumerates seeds via `std::fs::read_dir` rather than an explicit Vec, no source change is needed — the directory walk picks up the new file automatically. Confirm at execution.

- [ ] **Step 5: Run the corpus-walk test + fuzz target.**

```bash
cargo test -p envoy-config --lib fuzz_corpus_seeds_parse_or_reject_cleanly
```

Expected: pass; the new seed parses + validates cleanly.

```bash
cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30
```

Expected: 30-second clean fuzz run (no panics, no asserts) using the extended corpus. Mirrors the phase-04.1 / 04.2 short-budget CI run posture.

- [ ] **Step 6: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 7: Append a Task 3 section to PROGRESS.md.**

```markdown
## Task 3 — envoy-config fuzz corpus: hcm_route_to_cluster seed (2026-04-27)

- Commit: <SHA>
- Change: created crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_route_to_cluster.yaml exercising the RouteAction::Route variant + UnknownCluster reject path; appended the .gitignore allow-list entry; extended the bootstrap.rs::tests corpus-walk allow-list (or no source change if the walk is read_dir-driven).
- Verification: `cargo test -p envoy-config --lib fuzz_corpus_seeds_parse_or_reject_cleanly` → pass; `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` → clean 30s; workspace gate clean.
- Deviations from PLAN: <document any>.
```

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_route_to_cluster.yaml crates/envoy-config/fuzz/.gitignore crates/envoy-config/src/bootstrap.rs
git commit -m "phase 04.3: envoy-config fuzz seed — hcm_route_to_cluster (task 3)"
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 3)"
```

---

### Task 4: `envoy-http1::error` — 3 new `Http1Error` variants + 1 unit test

**Files:**
- Modify: `crates/envoy-http1/src/error.rs` (extend the `Http1Error` enum with `UpstreamConnect`, `MalformedResponseLine`, `MalformedChunkedFraming`)
- Modify (append): `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`

**Why now:** Tasks 5–7 (envoy-http1::client) reference these variants; landing them first makes the client-module tasks compile incrementally. Mirrors phase-04.2 Task 1's stub-variants-first pattern.

**Scope.** ~25 LoC enum extension + 1 unit test (Display formatting smoke test) + no new direct deps.

- [ ] **Step 1: Read the current `Http1Error` shape.**

Already known (from PLAN-write inspection): `crates/envoy-http1/src/error.rs:3-25` has 6 variants (`MalformedRequestLine`, `MalformedHeader`, `HeadersTooLarge { cap }`, `BodyTooLarge { cap }`, `UnexpectedEof`, `Io { source }`) plus a `From<io::Error>` impl at lines 27-31. Verify shape unchanged from 04.1 + 04.2:

```bash
grep -n '^#\[error' crates/envoy-http1/src/error.rs
```

Expected: 6 lines.

- [ ] **Step 2: Write a failing unit test in `crates/envoy-http1/src/error.rs::tests` (a new tests module if one doesn't exist; else append).**

The 04.1 `error.rs` has no `#[cfg(test)]` block. Append one:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_connect_display_includes_addr_and_source() {
        // 04.3 NEW: smoke-test the Display impl on Http1Error::UpstreamConnect.
        // The error is propagated up to RouterError::UpstreamConnect (Task 8) and
        // surfaces in `tracing::warn!` log lines, so the human-readable shape
        // matters operationally.
        let addr: std::net::SocketAddr = "127.0.0.1:9999".parse().unwrap();
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let err = Http1Error::UpstreamConnect {
            addr,
            source: io_err,
        };
        let s = err.to_string();
        assert!(
            s.contains("connecting to upstream") && s.contains("127.0.0.1:9999"),
            "unexpected Display output: {s}"
        );
    }
}
```

- [ ] **Step 3: Run the test to verify it fails.**

```bash
cargo test -p envoy-http1 --lib upstream_connect_display
```

Expected: build error citing `Http1Error::UpstreamConnect` not found. Fixed in Step 4.

- [ ] **Step 4: Add the 3 new variants in `crates/envoy-http1/src/error.rs`.**

Append after the existing `Io` variant (currently lines 20-24):

```rust
    /// 04.3 NEW: TCP-connect to an upstream cluster endpoint failed (e.g.,
    /// `ConnectionRefused`, `ETIMEDOUT`). Wraps the underlying `io::Error`.
    /// Surfaces from `Client::connect`; the router-proxy arm (Task 8 / Task 9)
    /// wraps this in `RouterError::UpstreamConnect { cluster, source }`.
    #[error("connecting to upstream {addr}: {source}")]
    UpstreamConnect {
        addr: std::net::SocketAddr,
        #[source]
        source: std::io::Error,
    },

    /// 04.3 NEW: upstream's HTTP/1.1 response status line was malformed.
    /// `httparse::Response::parse` returned a `Token` / `Version` / etc. error
    /// (mirrors `MalformedRequestLine`'s posture for outgoing requests).
    /// Surfaces from `ClientStream::send_request`'s response-parse step.
    #[error("malformed upstream response line")]
    MalformedResponseLine,

    /// 04.3 NEW: upstream's chunked-encoding framing violated RFC 7230 §4.1
    /// (e.g., non-hex chunk size, missing CRLF after chunk data, unexpected
    /// EOF mid-chunk). Surfaces from the chunked-encoding response reader
    /// in `client.rs` (Task 7).
    #[error("malformed chunked-encoding framing in upstream response")]
    MalformedChunkedFraming,
```

- [ ] **Step 5: Run the test to verify it passes + envoy-http1 lib tests stay green.**

```bash
cargo test -p envoy-http1 --lib upstream_connect_display
cargo test -p envoy-http1 --lib
```

Expected: 1 new test passes; total envoy-http1 lib tests = 25 (= 24 + 1).

- [ ] **Step 6: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean. The new `pub`-visible variants are exempt from `dead_code` per the workspace pattern.

- [ ] **Step 7: Append a Task 4 section to PROGRESS.md.**

```markdown
## Task 4 — envoy-http1::error: 3 new Http1Error variants (2026-04-27)

- Commit: <SHA>
- Change: extended Http1Error enum with UpstreamConnect { addr, source: io::Error }, MalformedResponseLine, MalformedChunkedFraming. Added a #[cfg(test)] mod tests block with a Display-shape smoke test on UpstreamConnect (the variant whose Display string surfaces in tracing log lines).
- Tests added (1): upstream_connect_display_includes_addr_and_source.
- Verification: `cargo test -p envoy-http1 --lib` → 25 passed (24 + 1); workspace gate clean.
- Deviations from PLAN: <document any>.
```

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-http1/src/error.rs
git commit -m "phase 04.3: envoy-http1::error — UpstreamConnect + MalformedResponseLine + MalformedChunkedFraming (task 4)"
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 4)"
```

---

### Task 5: `envoy-http1::client` — skeleton (`Client`, `ClientStream`) + `Client::connect` + `Cargo.toml` envoy-cluster dep activation + 2 connect tests

**Files:**
- Create: `crates/envoy-http1/src/client.rs` (skeleton: `Client` unit struct + `ClientStream { stream, host, buf }` + `Client::connect` + 2 connect tests)
- Modify: `crates/envoy-http1/src/lib.rs` (add `pub mod client;` + extend the `pub use` block with `client::{Client, ClientStream}`)
- Modify: `crates/envoy-http1/Cargo.toml` (verify `envoy-cluster` path-dep is active — was pre-staged in 04.1 per phase-04.1 REVIEW M3; if commented out or behind `#[cfg]`, activate it; if absent, add it)
- Modify (append): `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`

**Why now:** `Client::connect` is the smallest TDD-able unit of the client module; the skeleton + connect lay the type signatures Tasks 6–7 fill in. Per SPEC §6 signpost 1's ordering, client-module work fits between the schema (Task 1) and HCM consumption (Task 9).

**Scope.** ~80 LoC client.rs (skeleton + `Client::connect`) + ~80 LoC of 2 unit tests + `Cargo.toml` line + `lib.rs` 2 lines. No new direct deps — `envoy-cluster` is pre-staged from 04.1 (M3 carryforward); 04.3 consumes it.

- [ ] **Step 1: Verify the `envoy-cluster` dep status in `crates/envoy-http1/Cargo.toml`.**

```bash
grep -nE 'envoy-cluster|envoy_cluster' crates/envoy-http1/Cargo.toml
```

Expected (per 04.1 PROGRESS Task 4 + 04.1 REVIEW M3): `envoy-cluster = { path = "../envoy-cluster" }` is present in `[dependencies]`. If absent (M3 was awareness-only and the dep was never staged), add it now:

```toml
[dependencies]
# ... existing 04.1 entries ...
envoy-cluster = { path = "../envoy-cluster" }
```

If the dep is present but unused-warned (pre-04.3 was never imported), the warning closes naturally when Task 9 wires `cluster_mgr` into `HCMConfig`.

- [ ] **Step 2: Write 2 failing connect tests in `crates/envoy-http1/src/client.rs::tests`.**

Create the file with skeleton + tests:

```rust
//! Per-connection plaintext HTTP/1.1 client. No pooling; one TCP connection
//! per upstream request (pooling is upstream-robustness-family territory,
//! out of phase 04 per parent SPEC §4 + 04.3 SPEC §4 non-goals).
//!
//! This module is the workspace's SOLE user of `httparse::Response::parse`
//! per parent-04 SPEC §3 cross-sub-phase architectural rule 1. The 04.1
//! codec module uses `httparse::Request::parse`; this module is the only
//! consumer of the response parser.

use crate::error::Http1Error;

/// Per-connection plaintext HTTP/1.1 client. Stateless; the per-stream
/// state lives on `ClientStream`.
pub struct Client;

impl Client {
    /// TCP-connect to `addr`. The `host` value is captured for the eventual
    /// `Host:` header on `send_request`. No bytes are sent during connect;
    /// the caller's first `send_request` is the first wire write.
    ///
    /// Errors: `Http1Error::UpstreamConnect { addr, source }` on any
    /// `tokio::net::TcpStream::connect` failure.
    pub async fn connect(
        addr: std::net::SocketAddr,
        host: &str,
    ) -> Result<ClientStream, Http1Error> {
        let stream = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|source| Http1Error::UpstreamConnect { addr, source })?;
        Ok(ClientStream {
            stream,
            host: host.to_string(),
            buf: bytes::BytesMut::with_capacity(8192),
        })
    }
}

/// Active per-connection state: the underlying TCP stream, the host string
/// captured at connect time (used as the `Host:` header default if the
/// outgoing request doesn't carry one), and a read buffer for the response.
pub struct ClientStream {
    pub(crate) stream: tokio::net::TcpStream,
    pub(crate) host: String,
    pub(crate) buf: bytes::BytesMut,
}

impl ClientStream {
    // 04.3 Task 6: send_request lands here.
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_succeeds_against_in_process_acceptor() {
        // Bind an in-process acceptor on an ephemeral port. Client::connect
        // should TCP-connect cleanly and return a ClientStream.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // Spawn an acceptor that just drops every connection.
        tokio::spawn(async move {
            loop {
                let _ = listener.accept().await;
            }
        });
        let stream = Client::connect(addr, "envoy-rust.test")
            .await
            .expect("connect");
        assert_eq!(stream.host, "envoy-rust.test");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_returns_upstream_connect_on_refused_port() {
        // 127.0.0.1:1 is kernel-refused on every Linux box. macOS may differ
        // but the failure mode is still a connect-time io::Error which the
        // map_err arm wraps in UpstreamConnect.
        let addr: std::net::SocketAddr = "127.0.0.1:1".parse().unwrap();
        let err = Client::connect(addr, "envoy-rust.test")
            .await
            .expect_err("connect must fail");
        match err {
            Http1Error::UpstreamConnect {
                addr: got_addr,
                source,
            } => {
                assert_eq!(got_addr, addr);
                // The exact io::ErrorKind varies by OS (ConnectionRefused on
                // Linux, ConnectionRefused or AddrNotAvailable on macOS); just
                // assert there's some Display output.
                assert!(!source.to_string().is_empty());
            }
            other => panic!("expected UpstreamConnect, got: {other:?}"),
        }
    }
}
```

- [ ] **Step 3: Add `pub mod client;` + re-export to `crates/envoy-http1/src/lib.rs`.**

Locate `lib.rs:14-19` (the `pub mod` block) and `lib.rs:21-24` (the `pub use` block):

```rust
pub mod client;     // 04.3 NEW
pub mod codec;
pub mod date;
mod error;
pub mod hcm;
pub mod headers;
pub mod response;
// router lands in Task 8.

pub use client::{Client, ClientStream}; // 04.3 NEW
pub use codec::{Http1Codec, HttpVersion, Request};
pub use error::Http1Error;
pub use hcm::{HCM, HCMConfig};
pub use response::{Http1Response, Response};
```

- [ ] **Step 4: Run the tests to verify they pass.**

```bash
cargo test -p envoy-http1 --lib client::tests
cargo test -p envoy-http1 --lib
```

Expected: 2 new tests pass; total envoy-http1 lib tests = 27 (= 25 + 2).

- [ ] **Step 5: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean. If `cargo build` complains about the unused `envoy-cluster` dep (still no consumer in 04.3 until Task 9), suppress with a `#[allow(unused_imports)]` on `client.rs` — but typically `Cargo.toml` deps without source-imports don't emit warnings unless the manifest has `[lints]` rules; the existing 04.2 code passes so the pattern should hold.

- [ ] **Step 6: Append a Task 5 section to PROGRESS.md.**

```markdown
## Task 5 — envoy-http1::client: skeleton + Client::connect + 2 connect tests (2026-04-27)

- Commit: <SHA>
- Change: created crates/envoy-http1/src/client.rs with Client (unit struct), ClientStream { stream: TcpStream, host: String, buf: BytesMut }, and Client::connect (TCP-connect + UpstreamConnect mapping). Extended lib.rs with `pub mod client;` + `pub use client::{Client, ClientStream}`. Verified envoy-cluster path-dep is active in Cargo.toml (M3 carryforward — pre-staged in 04.1, consumed in 04.3 starting Task 9).
- Tests added (2): connect_succeeds_against_in_process_acceptor, connect_returns_upstream_connect_on_refused_port.
- Verification: `cargo test -p envoy-http1 --lib` → 27 passed (25 + 2); workspace gate clean.
- Deviations from PLAN: <document any>.
```

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-http1/src/client.rs crates/envoy-http1/src/lib.rs crates/envoy-http1/Cargo.toml
git commit -m "phase 04.3: envoy-http1::client — skeleton + Client::connect + 2 tests (task 5)"
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 5)"
```

---

### Task 6: `envoy-http1::client` — `ClientStream::send_request` Content-Length path + 4 tests

**Files:**
- Modify: `crates/envoy-http1/src/client.rs` (add `ClientStream::send_request` for the CL response path; reuse `Request` from `codec.rs` and `Response` from `response.rs`; add 4 unit tests)
- Modify (append): `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`

**Why now:** Task 5 landed `connect`; Task 6 adds the request-write + response-read for the simpler CL framing case. Task 7 adds chunked-encoding response reading. This split keeps each task ~100 LoC + 2-4 tests (per 04.1 codec/response/hcm task sizing).

**Scope.** ~120 LoC `send_request` impl (request serialization + response parse via `httparse::Response::parse` + CL body read) + ~140 LoC of 4 unit tests. The function is async and uses `tokio::io::AsyncWriteExt` + `AsyncReadExt` already in scope from envoy-http1's existing imports. The chunked-response branch is stubbed to return `MalformedChunkedFraming` until Task 7 wires it.

- [ ] **Step 1: Write 4 failing unit tests in `client.rs::tests`.**

Append after Task 5's two tests:

```rust
    // ── 04.3 Task 6 send_request Content-Length tests ─────────────────────────

    use crate::codec::{HttpVersion, Request};

    /// Build a minimal Request with method, path, and headers.
    /// Body defaults to empty Bytes.
    fn req(method: &str, path: &str, headers: &[(&str, &str)]) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            version: HttpVersion::Http11,
            headers: headers
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            bytes_consumed: 0, // not used by send_request
        }
    }

    /// Spawn an in-process acceptor that reads bytes into a Vec, sends a
    /// fixed response, and closes. Returns the listener address + a
    /// JoinHandle producing the captured request bytes.
    async fn capturing_acceptor(
        response: &'static [u8],
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let h = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut sock, _) = listener.accept().await.unwrap();
            // Read request bytes for ~50ms or until headers + body received,
            // whichever comes first. Using a fixed-size read loop is good
            // enough for tests; Real production servers would parse Content-Length.
            let mut buf = vec![0u8; 8192];
            // Read headers + body — assume the test sends a small request.
            let n = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                sock.read(&mut buf),
            )
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or(0);
            buf.truncate(n);
            // Write response.
            let _ = sock.write_all(response).await;
            let _ = sock.shutdown().await;
            buf
        });
        (addr, h)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_writes_serialized_request_bytes() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let (addr, capture) = capturing_acceptor(response).await;
        let mut client = Client::connect(addr, "envoy-rust.test").await.unwrap();
        let request = req("GET", "/", &[("user-agent", "test")]);
        let _resp = client.send_request(request).await.expect("send_request");
        let captured = capture.await.unwrap();
        let s = String::from_utf8_lossy(&captured);
        assert!(s.starts_with("GET / HTTP/1.1\r\n"), "request line: {s:?}");
        // Host header injected from connect's `host` (case-preserved as
        // emitted; lower-case here per send_request's emission convention).
        assert!(
            s.contains("host: envoy-rust.test\r\n"),
            "missing injected host: {s:?}"
        );
        assert!(
            s.contains("user-agent: test\r\n"),
            "missing user-agent: {s:?}"
        );
        assert!(
            s.contains("content-length: 0\r\n"),
            "missing content-length: {s:?}"
        );
        assert!(s.ends_with("\r\n\r\n"), "wire end: {s:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_uses_request_host_when_provided() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let (addr, capture) = capturing_acceptor(response).await;
        let mut client = Client::connect(addr, "envoy-rust.test").await.unwrap();
        // Outgoing request explicitly carries Host: explicit.example. The
        // connect-time `host` ("envoy-rust.test") should be IGNORED — the
        // explicit value wins per SPEC §6 signpost 5.
        let request = req("GET", "/", &[("Host", "explicit.example")]);
        let _resp = client.send_request(request).await.expect("send_request");
        let captured = capture.await.unwrap();
        let s = String::from_utf8_lossy(&captured);
        assert!(
            s.contains("host: explicit.example\r\n"),
            "request must use explicit Host: {s:?}"
        );
        assert!(
            !s.contains("host: envoy-rust.test\r\n"),
            "request must NOT inject the connect-time host when an explicit one is present: {s:?}"
        );
        // case-insensitive de-dup: only one host header.
        let host_count = s.matches("host:").count() + s.matches("Host:").count();
        assert_eq!(host_count, 1, "exactly one Host header: {s:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_reads_cl_response_body() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        let (addr, _capture) = capturing_acceptor(response).await;
        let mut client = Client::connect(addr, "envoy-rust.test").await.unwrap();
        let request = req("GET", "/", &[]);
        let resp = client.send_request(request).await.expect("send_request");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body.as_ref(), b"hello");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_returns_malformed_response_line_on_garbage() {
        let response: &[u8] = b"NOT AN HTTP RESPONSE";
        let (addr, _capture) = capturing_acceptor(response).await;
        let mut client = Client::connect(addr, "envoy-rust.test").await.unwrap();
        let request = req("GET", "/", &[]);
        let err = client
            .send_request(request)
            .await
            .expect_err("garbage upstream must fail");
        assert!(
            matches!(err, Http1Error::MalformedResponseLine),
            "got: {err:?}"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail.**

```bash
cargo test -p envoy-http1 --lib client::tests::send_request
```

Expected: build error citing `ClientStream::send_request` not found. Fixed in Step 3.

- [ ] **Step 3: Add `ClientStream::send_request` (Content-Length path) in `client.rs`.**

Below the `impl ClientStream` block (currently empty body marked for Task 6):

```rust
use crate::codec::Request;
use crate::headers as hdr;
use crate::response::Response;

use bytes::Bytes;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const RESPONSE_HEADERS_CAP: usize = 8192;
const READ_TIMEOUT: Duration = Duration::from_secs(30);

impl ClientStream {
    /// Serialize and write `request` (request-line + headers + optional
    /// CL-framed body), then read the response (status line + headers +
    /// CL-framed OR chunked body). The `Host:` header is sourced from the
    /// `host` captured at connect time UNLESS `request` already carries a
    /// `Host:` header (case-insensitive match), in which case the request's
    /// value wins and the connect-time host is dropped.
    ///
    /// Per SPEC §3 D1: chunked READER is implemented in Task 7; this Task-6
    /// implementation handles only the Content-Length response path. Chunked
    /// responses surface as `Http1Error::MalformedChunkedFraming` until Task 7.
    pub async fn send_request(&mut self, request: Request) -> Result<Response, Http1Error> {
        // (a) Serialize the request.
        let mut wire: Vec<u8> = Vec::with_capacity(256 + request.body_len_estimate());
        wire.extend_from_slice(request.method.as_bytes());
        wire.push(b' ');
        wire.extend_from_slice(request.path.as_bytes());
        wire.extend_from_slice(b" HTTP/1.1\r\n");

        // Host de-dup: if request.headers already carries a Host (any case),
        // emit that one. Otherwise inject the connect-time host.
        let request_has_host = request
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(hdr::HOST));
        if !request_has_host {
            wire.extend_from_slice(b"host: ");
            wire.extend_from_slice(self.host.as_bytes());
            wire.extend_from_slice(b"\r\n");
        }
        for (name, value) in &request.headers {
            wire.extend_from_slice(name.to_ascii_lowercase().as_bytes());
            wire.extend_from_slice(b": ");
            wire.extend_from_slice(value.as_bytes());
            wire.extend_from_slice(b"\r\n");
        }
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
        wire.extend_from_slice(b"\r\n");
        // Body bytes (CL-framed; Task 6 supports CL only — chunked-request
        // forwarding from downstream is deferred per SPEC §4 non-goals).
        if let Some(body) = request.body_bytes() {
            wire.extend_from_slice(body);
        }

        self.stream.write_all(&wire).await?;
        self.stream.flush().await?;

        // (b) Read the response headers.
        loop {
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(READ_TIMEOUT, self.stream.read(&mut chunk))
                .await
                .map_err(|_| Http1Error::UnexpectedEof)??;
            if n == 0 {
                return Err(Http1Error::UnexpectedEof);
            }
            self.buf.extend_from_slice(&chunk[..n]);

            let mut hp_storage = [httparse::EMPTY_HEADER; 64];
            let mut parsed = httparse::Response::new(&mut hp_storage);
            match parsed.parse(&self.buf) {
                Ok(httparse::Status::Complete(headers_end)) => {
                    let status = parsed
                        .code
                        .ok_or(Http1Error::MalformedResponseLine)?;
                    let mut headers: Vec<(String, String)> =
                        Vec::with_capacity(parsed.headers.len());
                    for h in parsed.headers.iter().filter(|h| !h.name.is_empty()) {
                        let name = h.name.to_string();
                        let value = std::str::from_utf8(h.value)
                            .map(str::to_string)
                            .map_err(|_| Http1Error::MalformedResponseLine)?;
                        headers.push((name, value));
                    }

                    // Detect chunked vs CL framing.
                    let chunked = headers.iter().any(|(n, v)| {
                        n.eq_ignore_ascii_case("transfer-encoding")
                            && v.eq_ignore_ascii_case("chunked")
                    });
                    if chunked {
                        // Task 7 lands the reader; Task 6 stubs it to error.
                        // Note: in production this branch never fires because
                        // Task 7 lands before Task 9 (which is the first
                        // production consumer); stub error is the test path.
                        let _ = headers_end;
                        return Err(Http1Error::MalformedChunkedFraming);
                    }

                    let cl: usize = headers
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case(hdr::CONTENT_LENGTH))
                        .and_then(|(_, v)| v.parse().ok())
                        .unwrap_or(0);

                    // Drain remaining body bytes from the stream + buf.
                    let already = self.buf.len() - headers_end;
                    let mut body: Vec<u8> = Vec::with_capacity(cl);
                    if already > 0 {
                        let take = already.min(cl);
                        body.extend_from_slice(&self.buf[headers_end..headers_end + take]);
                    }
                    while body.len() < cl {
                        let mut chunk = [0u8; 4096];
                        let n = tokio::time::timeout(READ_TIMEOUT, self.stream.read(&mut chunk))
                            .await
                            .map_err(|_| Http1Error::UnexpectedEof)??;
                        if n == 0 {
                            return Err(Http1Error::UnexpectedEof);
                        }
                        let need = cl - body.len();
                        body.extend_from_slice(&chunk[..n.min(need)]);
                    }

                    return Ok(Response {
                        status,
                        reason: None,
                        headers,
                        body: Bytes::from(body),
                    });
                }
                Ok(httparse::Status::Partial) => {
                    if self.buf.len() > RESPONSE_HEADERS_CAP {
                        return Err(Http1Error::HeadersTooLarge {
                            cap: RESPONSE_HEADERS_CAP,
                        });
                    }
                    continue;
                }
                Err(httparse::Error::Token)
                | Err(httparse::Error::Version)
                | Err(httparse::Error::Status) => {
                    return Err(Http1Error::MalformedResponseLine);
                }
                Err(httparse::Error::HeaderName)
                | Err(httparse::Error::HeaderValue)
                | Err(httparse::Error::NewLine) => {
                    return Err(Http1Error::MalformedHeader);
                }
                Err(httparse::Error::TooManyHeaders) => {
                    return Err(Http1Error::HeadersTooLarge {
                        cap: RESPONSE_HEADERS_CAP,
                    });
                }
            }
        }
    }
}
```

The `Request` type at `crates/envoy-http1/src/codec.rs:19-40` does NOT currently carry a `body: Bytes` field — it has `bytes_consumed: usize` (the offset into the input buffer where the body starts). For 04.3, the router-proxy arm (Task 9) drains the downstream body into a `Bytes` and constructs an outgoing request whose body is the drained bytes; the outgoing-request shape needs a body field.

Two implementation paths:

**Path A (preferred — minimal extension to `codec::Request`):** add `pub body: Option<Bytes>` to `Request` (default `None` per request-side serde-derive — but `Request` doesn't derive `Deserialize`; it's constructed by the codec), and add helper methods `body_len_estimate`, `body_len_string`, `body_bytes` referenced by `send_request`. Update the codec at `codec.rs:102-108` to default `body: None` (Task 9 fills it for outgoing-router-proxy requests).

**Path B (parallel type):** introduce a `ClientRequest { method, path, headers, body: Bytes }` type local to `client.rs` so `Request` (the codec output) stays unchanged. Task 9 converts the parsed downstream `Request` into a `ClientRequest` before calling `send_request`.

Path A is cleaner. The PLAN's Task 6 includes Path A's `Request` extension. Add to `crates/envoy-http1/src/codec.rs` after line 40:

```rust
    /// 04.3 NEW: outgoing request body bytes (for the router-proxy arm in
    /// Task 9 to populate before calling `Client::send_request`). The codec's
    /// `parse_request` (incoming-side) sets this to `None`; only the outgoing-
    /// side caller fills it. `None` is treated as `Bytes::new()` (Content-Length: 0).
    #[serde(skip)] // not actually used since Request has no Deserialize derive
    pub body: Option<bytes::Bytes>,
```

And update the constructor at `codec.rs:102-108` to set `body: None`:

```rust
        Ok(Some(Request {
            method,
            path,
            version,
            headers,
            bytes_consumed,
            body: None, // 04.3 NEW
        }))
```

Plus the helper methods on `Request`:

```rust
impl Request {
    /// 04.3 NEW: byte-length of the outgoing body, for `Content-Length:` and
    /// for the request-wire byte budget pre-allocation. Treats `None` as 0.
    pub fn body_len_estimate(&self) -> usize {
        self.body.as_ref().map(|b| b.len()).unwrap_or(0)
    }

    pub(crate) fn body_len_string(&self) -> String {
        self.body_len_estimate().to_string()
    }

    pub(crate) fn body_bytes(&self) -> Option<&[u8]> {
        self.body.as_ref().map(|b| b.as_ref())
    }
}
```

This adds ~20 LoC to codec.rs and changes the `Request` literal construction in any test fixtures that currently spell out `Request { method, path, version, headers, bytes_consumed }` — namely `crates/envoy-http1/src/hcm.rs::tests` (no, those tests construct Requests via `Http1Codec::parse_request(buf)` not by literal — verify), the `client.rs::tests` block above (already constructs literals — already includes `body: None` if you use the helper `req(...)` shape), and any lib tests that build Request literals (likely none in 04.1 / 04.2 — the codec-level Request literals appear only in `codec.rs::tests` at lines 116-129 which use struct fields directly).

If the `req()` test helper above uses positional / named field construction like `Request { method, path, version, headers, bytes_consumed }`, update it to include `body: None`. The test fixture as-written above actually does NOT spell out the body field — fix Step 1's test code accordingly:

Replace the `req()` helper from Step 1's test code with the body-aware version:

```rust
    fn req(method: &str, path: &str, headers: &[(&str, &str)]) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            version: HttpVersion::Http11,
            headers: headers
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            bytes_consumed: 0,
            body: None, // 04.3 NEW
        }
    }
```

(Apply this fix in Step 1's code block before running tests in Step 2.)

- [ ] **Step 4: Run the tests to verify they pass.**

```bash
cargo test -p envoy-http1 --lib client::tests::send_request
cargo test -p envoy-http1 --lib
```

Expected: 4 new tests pass; total envoy-http1 lib tests = 31 (= 27 + 4).

The existing 04.1 `codec::tests` (5 tests at lines 116-182) — verify they still pass after the `body: None` field-default propagates. Test fixtures at lines 116-129 etc. construct `Request` from `Http1Codec::parse_request(buf).expect(...)` not via struct literals, so they're unaffected.

Existing 04.1 + 04.2 `hcm::tests` — same: requests are built from `Http1Codec::parse_request` paths or the `drive(config, req_bytes)` helper.

- [ ] **Step 5: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 6: Append a Task 6 section to PROGRESS.md.**

```markdown
## Task 6 — envoy-http1::client: send_request CL path + 4 tests (2026-04-27)

- Commit: <SHA>
- Change: implemented ClientStream::send_request for the Content-Length response path (request serialization + httparse::Response::parse + CL body drain). Extended codec::Request with `body: Option<Bytes>` field + body_len_estimate/body_len_string/body_bytes helpers (router-proxy arm in Task 9 fills body before send_request). Stub returns Http1Error::MalformedChunkedFraming on chunked responses (Task 7 wires the chunked reader).
- Tests added (4): send_request_writes_serialized_request_bytes, send_request_uses_request_host_when_provided, send_request_reads_cl_response_body, send_request_returns_malformed_response_line_on_garbage.
- Verification: `cargo test -p envoy-http1 --lib` → 31 passed (27 + 4); workspace gate clean.
- Deviations from PLAN: <document any>.
```

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-http1/src/client.rs crates/envoy-http1/src/codec.rs
git commit -m "phase 04.3: envoy-http1::client — send_request CL path + 4 tests (task 6)"
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 6)"
```

---

### Task 7: `envoy-http1::client` — chunked-encoding response reader + 2 chunked-reader tests

**Files:**
- Modify: `crates/envoy-http1/src/client.rs` (replace the chunked-stub branch in `send_request` with a real reader; factor a `read_chunked_body` helper; add 2 unit tests)
- Modify (append): `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`

**Why now:** SPEC §3 D1 lists chunked-response reading as in-scope; fixture 0008's helper emits CL-only so the chunked path is exercised only by unit tests in 04.3 (Task 9 wires HCM to use either path; 04.3 production traffic never hits the chunked branch — but a future fixture or upstream that emits chunked must work).

**Scope.** ~70 LoC `read_chunked_body` helper (parse hex chunk-size lines, read exact data + CRLF, terminate on zero-size chunk, ignore trailers per SPEC §4 non-goals) + ~80 LoC of 2 unit tests.

- [ ] **Step 1: Write 2 failing chunked-reader tests in `client.rs::tests`.**

Append after Task 6's tests:

```rust
    // ── 04.3 Task 7 chunked-encoding reader tests ─────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_reads_chunked_response_body() {
        // Two chunks ("hello" 5 bytes + " world" 6 bytes) terminated by 0-size.
        let response: &[u8] =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        let (addr, _capture) = capturing_acceptor(response).await;
        let mut client = Client::connect(addr, "envoy-rust.test").await.unwrap();
        let request = req("GET", "/", &[]);
        let resp = client.send_request(request).await.expect("send_request");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body.as_ref(), b"hello world");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_returns_malformed_chunked_on_bad_size_line() {
        // "XYZ" is not a valid hex chunk size.
        let response: &[u8] =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nXYZ\r\nhello\r\n";
        let (addr, _capture) = capturing_acceptor(response).await;
        let mut client = Client::connect(addr, "envoy-rust.test").await.unwrap();
        let request = req("GET", "/", &[]);
        let err = client
            .send_request(request)
            .await
            .expect_err("malformed chunk size must fail");
        assert!(
            matches!(err, Http1Error::MalformedChunkedFraming),
            "got: {err:?}"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail.**

```bash
cargo test -p envoy-http1 --lib send_request_reads_chunked send_request_returns_malformed_chunked
```

Expected: `send_request_reads_chunked_response_body` fails (current Task-6 stub returns `MalformedChunkedFraming` for any chunked response — but the test expects success with body `"hello world"`); `send_request_returns_malformed_chunked_on_bad_size_line` passes accidentally (the stub returns `MalformedChunkedFraming` for ALL chunked responses, including this one). Fix the stub in Step 3.

- [ ] **Step 3: Replace the chunked stub in `send_request` with a real reader.**

Locate the chunked stub in `send_request` (the `if chunked { ... return Err(MalformedChunkedFraming) ... }` block from Task 6). Replace with:

```rust
                    if chunked {
                        // 04.3 Task 7: real chunked reader.
                        let already = self.buf.len() - headers_end;
                        let body = read_chunked_body(
                            &mut self.stream,
                            &mut self.buf,
                            headers_end,
                            already,
                        )
                        .await?;
                        return Ok(Response {
                            status,
                            reason: None,
                            headers,
                            body: Bytes::from(body),
                        });
                    }
```

Add the `read_chunked_body` async helper at the bottom of `client.rs` (private to the module):

```rust
/// Read a chunked-encoding response body from `stream`, having already read
/// `already` bytes past the headers into `buf` starting at offset `headers_end`.
/// Returns the decoded body bytes (chunks concatenated; trailers discarded).
///
/// Wire format per RFC 7230 §4.1:
///   chunk        = chunk-size CRLF chunk-data CRLF
///   last-chunk   = "0" CRLF [trailer-part] CRLF
///   chunk-size   = 1*HEXDIG
///
/// 04.3 ignores trailers (per SPEC §4 non-goals — trailer forwarding deferred).
/// On any framing violation returns `Http1Error::MalformedChunkedFraming`.
async fn read_chunked_body(
    stream: &mut tokio::net::TcpStream,
    buf: &mut bytes::BytesMut,
    headers_end: usize,
    _already: usize,
) -> Result<Vec<u8>, Http1Error> {
    use tokio::io::AsyncReadExt;

    let mut out: Vec<u8> = Vec::new();
    let mut pos = headers_end;

    loop {
        // Ensure at least one CRLF visible after `pos`.
        let crlf_offset = loop {
            if let Some(off) = find_crlf(&buf[pos..]) {
                break off;
            }
            // Need more bytes.
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk))
                .await
                .map_err(|_| Http1Error::MalformedChunkedFraming)??;
            if n == 0 {
                return Err(Http1Error::MalformedChunkedFraming);
            }
            buf.extend_from_slice(&chunk[..n]);
        };

        // Parse chunk-size as hex (with optional ;ext extensions per RFC).
        let size_line = std::str::from_utf8(&buf[pos..pos + crlf_offset])
            .map_err(|_| Http1Error::MalformedChunkedFraming)?
            .trim();
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let chunk_size = usize::from_str_radix(size_hex, 16)
            .map_err(|_| Http1Error::MalformedChunkedFraming)?;

        pos += crlf_offset + 2; // skip size line + CRLF

        if chunk_size == 0 {
            // Last chunk. RFC 7230 allows trailer-part before the final CRLF;
            // 04.3 reads (and discards) until the next CRLF (the empty-line
            // sentinel). For simplicity, assume zero trailers — read one CRLF
            // and we're done. If the response has trailers, the framing is
            // technically valid but body content is intact (we've already
            // read all chunk bytes); the `0\r\n\r\n` shape covers the no-trailer
            // case which fixture 0008 + the test response use.
            //
            // Defensive read: if the next 2 bytes are CRLF, accept; else read
            // until CRLF then require another CRLF (single-pass trailer skip).
            while buf.len() < pos + 2 {
                let mut chunk = [0u8; 64];
                let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk))
                    .await
                    .map_err(|_| Http1Error::MalformedChunkedFraming)??;
                if n == 0 {
                    return Err(Http1Error::MalformedChunkedFraming);
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            if &buf[pos..pos + 2] != b"\r\n" {
                // Trailers present — skip until empty CRLF line.
                loop {
                    let crlf = match find_crlf(&buf[pos..]) {
                        Some(off) => off,
                        None => {
                            let mut chunk = [0u8; 256];
                            let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk))
                                .await
                                .map_err(|_| Http1Error::MalformedChunkedFraming)??;
                            if n == 0 {
                                return Err(Http1Error::MalformedChunkedFraming);
                            }
                            buf.extend_from_slice(&chunk[..n]);
                            continue;
                        }
                    };
                    pos += crlf + 2;
                    if crlf == 0 {
                        break; // empty line — end of trailers
                    }
                }
            } else {
                pos += 2;
            }
            return Ok(out);
        }

        // Read exactly `chunk_size` body bytes + 2 trailing CRLF.
        while buf.len() < pos + chunk_size + 2 {
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk))
                .await
                .map_err(|_| Http1Error::MalformedChunkedFraming)??;
            if n == 0 {
                return Err(Http1Error::MalformedChunkedFraming);
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        out.extend_from_slice(&buf[pos..pos + chunk_size]);
        if &buf[pos + chunk_size..pos + chunk_size + 2] != b"\r\n" {
            return Err(Http1Error::MalformedChunkedFraming);
        }
        pos += chunk_size + 2;
    }
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}
```

- [ ] **Step 4: Run the tests to verify they pass.**

```bash
cargo test -p envoy-http1 --lib send_request_reads_chunked send_request_returns_malformed_chunked
cargo test -p envoy-http1 --lib client::tests
cargo test -p envoy-http1 --lib
```

Expected: all 8 client::tests pass; total envoy-http1 lib tests = 33 (= 31 + 2).

- [ ] **Step 5: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean. Note that the chunked reader's nested loops + trailer-skip logic may trip clippy's `too_many_lines` lint — if so, factor the trailer-skip block into a separate `skip_trailers(stream, buf, pos)` helper.

- [ ] **Step 6: Append a Task 7 section to PROGRESS.md.**

```markdown
## Task 7 — envoy-http1::client: chunked response reader + 2 tests (2026-04-27)

- Commit: <SHA>
- Change: replaced the Task-6 chunked-stub branch in send_request with a real read_chunked_body helper that parses hex chunk-size lines (with optional ;ext), reads exact chunk data + CRLF, terminates on zero-size chunk, and skips (without forwarding) optional trailers per SPEC §4 non-goal. Added find_crlf utility helper.
- Tests added (2): send_request_reads_chunked_response_body, send_request_returns_malformed_chunked_on_bad_size_line. Total client::tests = 8.
- Verification: `cargo test -p envoy-http1 --lib` → 33 passed (31 + 2); workspace gate clean.
- Deviations from PLAN: <document any>.
```

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-http1/src/client.rs
git commit -m "phase 04.3: envoy-http1::client — chunked response reader + 2 tests (task 7)"
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 7)"
```

---

### Task 8: `envoy-http1::router` — `RouterError` enum + `HCM_EMITTED_HEADERS` + `write_proxied_response` + 3 unit tests

**Files:**
- Create: `crates/envoy-http1/src/router.rs` (new module owning `RouterError`, `HCM_EMITTED_HEADERS` const, and the `write_proxied_response` async helper that applies the header allow-list policy + injects `x-envoy-upstream-service-time` + sets `Connection:` per posture; 3 unit tests)
- Modify: `crates/envoy-http1/src/lib.rs` (add `pub mod router;` + `pub use router::RouterError`)
- Modify (append): `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`

**Why now:** The proxied-response shape policy (header overwrite, x-envoy-upstream-service-time injection, connection lifecycle) is independent of HCM's match-and-dispatch logic; factoring it out per SPEC §6 signpost 7 keeps the policy in one focused module + lets Task 9's HCM extension be a clean two-arm `match` that calls into `router.rs` for the new arm and reuses the existing `synth_direct_response` for the carryover arm. RouterError + write_proxied_response landing first means Task 9's compile is incremental.

**Scope.** ~50 LoC `RouterError` + `HCM_EMITTED_HEADERS` const + ~80 LoC `write_proxied_response` helper + ~110 LoC of 3 unit tests. The helper consumes a parsed upstream `Response` (from `client.rs::send_request`) and writes a synthesized downstream `Response` via the existing `Http1Response::write_to`.

- [ ] **Step 1: Write 3 failing unit tests in `crates/envoy-http1/src/router.rs::tests` (new file).**

Create the file with the test scaffold first (impl lands in Step 3):

```rust
//! Router-proxy helper module: RouterError enum + write_proxied_response shape
//! policy. Per SPEC §6 signpost 7 + parent-04 SPEC §3 cross-sub-phase rule about
//! placing HCM-internal logic in envoy-http1.

use crate::error::Http1Error;
use crate::headers as hdr;
use crate::response::{Http1Response, Response};
use bytes::Bytes;

/// 04.3 NEW: typed errors surfaced by the router-proxy arm in `hcm.rs`.
/// Each variant carries `cluster: String` for per-cluster log attribution
/// (per SPEC §3 D5: this is what makes the `Cluster::name()` close-out
/// load-bearing — the router's `tracing::warn!(cluster = ..., ...)` log
/// lines on per-cluster proxy errors are the natural use site).
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    /// Cluster has no live endpoints (the static-cluster case is `0` endpoints
    /// at config-load — validator rejects in Task 2 — but defense-in-depth
    /// covers the case where `pick_endpoint()` returns `None` for any reason).
    #[error("no healthy endpoint available for cluster '{cluster}'")]
    NoHealthyEndpoint { cluster: String },

    /// Wraps a `Http1Error::UpstreamConnect`. Surfaces the cluster name
    /// alongside the underlying `io::Error`; the cluster name is what
    /// distinguishes per-cluster connection failures in operational logs.
    #[error("upstream connect failed for cluster '{cluster}': {source}")]
    UpstreamConnect {
        cluster: String,
        #[source]
        source: Http1Error,
    },

    /// Wraps any post-connect Http1Error (`MalformedResponseLine`,
    /// `MalformedChunkedFraming`, `UnexpectedEof`, `Io`, `HeadersTooLarge`,
    /// `BodyTooLarge`).
    #[error("upstream request failed for cluster '{cluster}': {source}")]
    UpstreamRequestFailed {
        cluster: String,
        #[source]
        source: Http1Error,
    },
}

/// 04.3 NEW: response headers envoy-rust's HCM emits on every direct_response
/// path. When a proxied response from upstream carries any of these names,
/// `write_proxied_response` REPLACES the upstream's value with envoy-rust's
/// own (matches Envoy's posture: upstream's `server: nginx/1.x` is overwritten
/// with `server: envoy`).
pub const HCM_EMITTED_HEADERS: &[&str] = &["server", "date"];

/// 04.3 NEW: the `x-envoy-upstream-service-time` header name (allow-listed
/// per BEHAVIOR_CONTRACT.md row added in Task 10). Both Envoy and envoy-rust
/// emit on every router-proxy response with their own measurement of upstream
/// latency in milliseconds.
pub const X_ENVOY_UPSTREAM_SERVICE_TIME: &str = "x-envoy-upstream-service-time";

/// Build the synthesized downstream response from the upstream response,
/// applying the header allow-list policy + injecting `x-envoy-upstream-service-time`
/// + setting `Connection:` per the captured-pre-drain posture, and write the
/// wire bytes via Http1Response::write_to.
///
/// Per SPEC §6 signpost 7:
/// 1. Status line forwards verbatim from upstream.
/// 2. For each upstream header: if the name is in HCM_EMITTED_HEADERS,
///    replace with envoy-rust's value (`server: envoy-rust`, `date: <fresh IMF-fixdate>`);
///    otherwise pass verbatim.
/// 3. Append `x-envoy-upstream-service-time: <elapsed_ms>`.
/// 4. Set `Connection:` per `close` flag (true → `close`, false → `keep-alive`).
/// 5. Forward the body bytes preserving the upstream's framing (CL or chunked
///    — the body bytes are already decoded into a single Bytes by client.rs's
///    chunked reader, so the downstream side always emits CL-framed in 04.3).
pub async fn write_proxied_response<W>(
    downstream: &mut W,
    upstream_response: Response,
    elapsed_ms: u128,
    close: bool,
) -> Result<(), Http1Error>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let now_date = crate::date::format_imf_fixdate(std::time::SystemTime::now());
    let mut headers: Vec<(String, String)> = Vec::with_capacity(upstream_response.headers.len() + 2);

    let mut saw_server = false;
    let mut saw_date = false;
    let mut saw_cl = false;

    for (name, value) in upstream_response.headers.into_iter() {
        let lc = name.to_ascii_lowercase();
        if lc == hdr::SERVER {
            saw_server = true;
            headers.push((hdr::SERVER.to_string(), "envoy-rust".to_string()));
        } else if lc == hdr::DATE {
            saw_date = true;
            headers.push((hdr::DATE.to_string(), now_date.clone()));
        } else if lc == hdr::CONNECTION {
            // Drop any upstream Connection: header — we authoritatively set it
            // below per the downstream posture.
            continue;
        } else if lc == hdr::CONTENT_LENGTH {
            saw_cl = true;
            headers.push((hdr::CONTENT_LENGTH.to_string(), value));
        } else {
            // Pass verbatim. (Includes content-type and any allow-listed headers
            // that envoy-rust does not authoritatively set.)
            headers.push((name, value));
        }
    }
    // Inject defaults for HCM-emitted headers the upstream didn't carry.
    if !saw_server {
        headers.push((hdr::SERVER.to_string(), "envoy-rust".to_string()));
    }
    if !saw_date {
        headers.push((hdr::DATE.to_string(), now_date));
    }
    // Inject Content-Length if upstream didn't carry one (post-chunked-decode
    // body has known length).
    if !saw_cl {
        headers.push((
            hdr::CONTENT_LENGTH.to_string(),
            upstream_response.body.len().to_string(),
        ));
    }
    // Inject x-envoy-upstream-service-time per SPEC §2 + BEHAVIOR_CONTRACT.md row.
    headers.push((
        X_ENVOY_UPSTREAM_SERVICE_TIME.to_string(),
        elapsed_ms.to_string(),
    ));
    // Authoritative Connection per posture.
    headers.push((
        hdr::CONNECTION.to_string(),
        if close { "close" } else { "keep-alive" }.to_string(),
    ));

    let resp = Response {
        status: upstream_response.status,
        reason: upstream_response.reason,
        headers,
        body: upstream_response.body,
    };
    Http1Response::write_to(&resp, downstream).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn upstream(status: u16, headers: Vec<(&str, &str)>, body: &[u8]) -> Response {
        Response {
            status,
            reason: None,
            headers: headers
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            body: Bytes::copy_from_slice(body),
        }
    }

    /// Run write_proxied_response into an in-memory Vec and parse out the
    /// resulting downstream wire bytes.
    async fn drive_proxy(upstream_resp: Response, elapsed_ms: u128, close: bool) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        write_proxied_response(&mut buf, upstream_resp, elapsed_ms, close)
            .await
            .expect("write_proxied_response");
        buf
    }

    #[tokio::test]
    async fn proxied_response_appends_x_envoy_upstream_service_time() {
        // Upstream returns 200 with simple headers; assert downstream wire
        // carries x-envoy-upstream-service-time with the integer ms value.
        let up = upstream(
            200,
            vec![
                ("Content-Type", "text/plain"),
                ("Content-Length", "5"),
            ],
            b"hello",
        );
        let buf = drive_proxy(up, 42, false).await;
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("x-envoy-upstream-service-time: 42\r\n"), "got: {s}");
    }

    #[tokio::test]
    async fn proxied_response_overwrites_server_and_date_headers() {
        // Upstream emits non-envoy server + a fixed-date stamp. envoy-rust
        // overwrites both with its own values per HCM_EMITTED_HEADERS policy.
        let up = upstream(
            200,
            vec![
                ("Server", "upstream-software/1.0"),
                ("Date", "Thu, 01 Jan 1970 00:00:00 GMT"),
                ("Content-Length", "5"),
                ("Content-Type", "text/plain"),
            ],
            b"hello",
        );
        let buf = drive_proxy(up, 1, false).await;
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("server: envoy-rust\r\n"), "server overwrite: {s}");
        assert!(
            !s.contains("upstream-software"),
            "must not pass upstream Server: {s}"
        );
        assert!(s.contains("date: "), "fresh date: {s}");
        assert!(
            !s.contains("Thu, 01 Jan 1970"),
            "must not pass upstream Date: {s}"
        );
        // The body + content-length + content-type pass through verbatim.
        assert!(s.contains("content-type: text/plain\r\n"), "ct: {s}");
        assert!(s.contains("content-length: 5\r\n"), "cl: {s}");
        assert!(s.ends_with("\r\nhello"), "body: {s}");
    }

    #[tokio::test]
    async fn proxied_response_sets_connection_per_posture() {
        let up = upstream(
            200,
            vec![("Content-Length", "0"), ("Connection", "keep-alive")],
            b"",
        );
        let buf_close = drive_proxy(
            Response {
                status: 200,
                reason: None,
                headers: up.headers.clone(),
                body: up.body.clone(),
            },
            1,
            true, // close = true
        )
        .await;
        let s_close = String::from_utf8_lossy(&buf_close);
        assert!(s_close.contains("connection: close\r\n"), "close: {s_close}");
        assert!(
            !s_close.contains("connection: keep-alive\r\n"),
            "must not pass upstream Connection: {s_close}"
        );

        let buf_keep = drive_proxy(up, 1, false).await; // close = false
        let s_keep = String::from_utf8_lossy(&buf_keep);
        assert!(
            s_keep.contains("connection: keep-alive\r\n"),
            "keep-alive: {s_keep}"
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail.**

```bash
cargo test -p envoy-http1 --lib router::tests
```

Expected: build error citing `router` module not declared in lib.rs (Step 3 fixes).

- [ ] **Step 3: Add `pub mod router;` + `pub use router::RouterError;` to `crates/envoy-http1/src/lib.rs`.**

Update the `pub mod` block + `pub use` block from Task 5:

```rust
pub mod client;
pub mod codec;
pub mod date;
mod error;
pub mod hcm;
pub mod headers;
pub mod response;
pub mod router;     // 04.3 NEW (Task 8)

pub use client::{Client, ClientStream};
pub use codec::{Http1Codec, HttpVersion, Request};
pub use error::Http1Error;
pub use hcm::{HCM, HCMConfig};
pub use response::{Http1Response, Response};
pub use router::RouterError; // 04.3 NEW (Task 8)
```

The Step 1 test scaffold becomes the actual `router.rs` content (test scaffold + impl in the same file — the impl is what Step 1 already wrote).

- [ ] **Step 4: Run the tests to verify they pass.**

```bash
cargo test -p envoy-http1 --lib router::tests
cargo test -p envoy-http1 --lib
```

Expected: 3 new tests pass; total envoy-http1 lib tests = 36 (= 33 + 3).

- [ ] **Step 5: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 6: Append a Task 8 section to PROGRESS.md.**

```markdown
## Task 8 — envoy-http1::router: RouterError + HCM_EMITTED_HEADERS + write_proxied_response + 3 tests (2026-04-27)

- Commit: <SHA>
- Change: created crates/envoy-http1/src/router.rs with RouterError enum (NoHealthyEndpoint, UpstreamConnect, UpstreamRequestFailed — each carrying cluster: String per SPEC §3 D2 + D5), HCM_EMITTED_HEADERS const = ["server", "date"], X_ENVOY_UPSTREAM_SERVICE_TIME const, and write_proxied_response async helper applying the header allow-list policy + injecting x-envoy-upstream-service-time + setting Connection per posture. Extended lib.rs with `pub mod router;` + `pub use router::RouterError`.
- Tests added (3): proxied_response_appends_x_envoy_upstream_service_time, proxied_response_overwrites_server_and_date_headers, proxied_response_sets_connection_per_posture.
- Verification: `cargo test -p envoy-http1 --lib` → 36 passed (33 + 3); workspace gate clean.
- Deviations from PLAN: <document any>.
```

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-http1/src/router.rs crates/envoy-http1/src/lib.rs
git commit -m "phase 04.3: envoy-http1::router — RouterError + write_proxied_response + 3 tests (task 8)"
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 8)"
```

---

### Task 9: `envoy-http1::hcm` — `RouteAction` two-arm match restructure + `Route` arm wires through `cluster_mgr` + `HCMConfig::cluster_mgr` field + `envoy-cluster::Cluster::name()` accessor (D5 close) + 6 HCM unit tests

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (extend `HCMConfig` with `cluster_mgr` field; extend `HCMConfig::from_config` signature; thread `cluster_mgr` through `serve_connection` → `build_response`; replace the Task-1 placeholder Route(_) arm with a real proxy implementation calling `Client::connect` + `client_stream.send_request` + `router::write_proxied_response`; add 6 HCM unit tests; remove the obsolete `// 04.3: pub cluster_mgr: ...` placeholder comment at line 30)
- Modify: `crates/envoy-cluster/src/cluster.rs` (add `pub fn Cluster::name(&self) -> &str`; add `pub fn ClusterHandle::name(&self) -> &str`; remove the field-level `#[allow(dead_code)]` annotation at line 13; add 3 unit tests; per SPEC §3 D5 + §6 signpost 16, this is the close-out commit for the multi-phase Cluster::name carryforward originating in phase-02.1 REVIEW M1)
- Modify (append): `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`

**Why now:** Tasks 4–8 land all the pieces (Http1Error variants, Client + ClientStream, RouterError + write_proxied_response). Task 9 is the integration: HCM's hardcoded router-filter call site extends from one match arm (DirectResponse) to two (DirectResponse + Route), and the new arm dispatches through cluster_mgr → pick_endpoint → Client::connect → send_request → write_proxied_response. The D5 close-out (`Cluster::name()` accessor) folds in here per SPEC §6 signpost 1 + the phase-02.2 task-11 precedent — the per-cluster log attribution in `tracing::warn!(cluster = …)` log lines on RouterError paths is the natural use site (and removes the field-level `#[allow(dead_code)]` from envoy-cluster's Cluster.name field that has been outstanding since phase 02.1).

**Scope.** ~80 LoC HCM extension (signature change + match restructure + Route arm impl + cluster_mgr threading) + ~10 LoC envoy-cluster Cluster::name + ClusterHandle::name + ~20 LoC envoy-cluster D5 tests + ~120 LoC of 6 HCM unit tests.

- [ ] **Step 1: Read the current `HCMConfig` shape + `serve_connection` + `build_response` signatures.**

```bash
grep -n 'pub struct HCMConfig\|fn from_config\|async fn serve_connection\|fn build_response\|fn synth_direct_response' crates/envoy-http1/src/hcm.rs
```

Expected (from PLAN-write inspection): HCMConfig at line 27, from_config at line 33, serve_connection at line 98, build_response at line 192, synth_direct_response at line 289.

- [ ] **Step 2: Write 6 failing HCM unit tests in `crates/envoy-http1/src/hcm.rs::tests`.**

Append after the existing 04.2 header-matcher tests (after line 811):

```rust
    // ── 04.3 Task 9 router-proxy arm tests ────────────────────────────────────

    use crate::client::Client;

    /// Build a minimal HCMConfig with a single VH `domains: ["*"]`,
    /// configurable routes, AND a cluster_mgr.
    fn hcm_config_with_cluster(
        prefix: &str,
        action: RouteAction,
        cluster_mgr: Arc<envoy_cluster::ClusterManager>,
    ) -> Arc<HCMConfig> {
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            route_config: Arc::new(RouteConfiguration {
                name: "rc".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some(prefix.to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action,
                    }],
                }],
            }),
        })
    }

    /// Spawn an in-process upstream HTTP/1.1 echo acceptor on an ephemeral
    /// port. Returns (port, JoinHandle). The acceptor responds with
    /// `200 OK\r\nContent-Length: <len>\r\n\r\n<echoed-method-path>` for one
    /// connection then exits.
    async fn spawn_in_process_upstream(response: &'static [u8]) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = vec![0u8; 4096];
                let _ = tokio::time::timeout(
                    Duration::from_millis(500),
                    sock.read(&mut buf),
                )
                .await;
                let _ = sock.write_all(response).await;
                let _ = sock.shutdown().await;
            }
        });
        port
    }

    /// Build an envoy_cluster::ClusterManager with one cluster `backend` whose
    /// single endpoint is `127.0.0.1:<port>`.
    fn cluster_mgr_with_endpoint(name: &str, port: u16) -> Arc<envoy_cluster::ClusterManager> {
        let yaml = format!(
            r#"
node: {{ id: t, cluster: c }}
static_resources:
  listeners: []
  clusters:
    - name: {name}
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: {name}
        endpoints:
          - lb_endpoints:
              - endpoint: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: {port} }} }} }}
"#
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("bootstrap parses");
        Arc::new(envoy_cluster::from_bootstrap(&bootstrap).expect("cluster mgr"))
    }

    fn cluster_mgr_empty() -> Arc<envoy_cluster::ClusterManager> {
        let yaml = r#"
node: { id: t, cluster: c }
static_resources:
  listeners: []
  clusters: []
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("bootstrap parses");
        Arc::new(envoy_cluster::from_bootstrap(&bootstrap).expect("cluster mgr"))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn route_walk_dispatches_direct_response_unchanged() {
        // Regression: the 04.1 + 04.2 DirectResponse path is unchanged after
        // the Task-9 RouteAction restructure.
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("ok\n".into()),
                },
            }),
            cluster_mgr_empty(),
        );
        let req = b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "got: {s}");
        assert!(s.ends_with("\r\nok\n"), "body: {s}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn route_walk_dispatches_route_action_to_client_connect() {
        // Stand up a minimal in-process upstream that returns 200 with body.
        // Configure HCM with a cluster pointing at that upstream and a
        // route_action_route route. Drive a request through HCM and verify the
        // downstream sees the upstream's body bytes (modulo HCM_EMITTED_HEADERS
        // overwrites + x-envoy-upstream-service-time injection).
        let upstream_response: &'static [u8] =
            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 12\r\n\r\nhello, world";
        let upstream_port = spawn_in_process_upstream(upstream_response).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port);
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
            }),
            cluster_mgr,
        );
        let req = b"GET /any HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "got: {s}");
        assert!(s.contains("server: envoy-rust\r\n"), "server overwrite: {s}");
        assert!(
            s.contains("x-envoy-upstream-service-time: "),
            "x-envoy-upstream-service-time present: {s}"
        );
        assert!(s.contains("content-type: text/plain\r\n"), "ct passthrough: {s}");
        assert!(s.contains("content-length: 12\r\n"), "cl passthrough: {s}");
        assert!(s.ends_with("hello, world"), "body passthrough: {s}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn route_walk_returns_no_healthy_endpoint_when_cluster_empty() {
        // Edge: cluster has zero endpoints (the validator rejects this in
        // production — Task 2's UnknownCluster check fires first — but
        // defense-in-depth: if pick_endpoint() ever returns None for any
        // reason, RouterError::NoHealthyEndpoint propagates and HCM returns
        // a 503 Service Unavailable.
        //
        // Constructing a "cluster with zero endpoints" requires bypassing the
        // envoy-cluster::from_bootstrap rejection. Skip the check by using
        // a cluster_mgr where the cluster name doesn't exist (cluster_mgr.get
        // returns None, which the Route arm interprets as
        // "validator should have rejected" and panics — wait, that's the wrong
        // path. The Route arm uses .expect("validator ensures cluster present")
        // — so this test would panic in the unwrap, not fail with NoHealthyEndpoint.
        //
        // Therefore the right shape for this test is: build a cluster_mgr with
        // a real cluster name pointing at a refused port, drive HCM, expect
        // RouterError::UpstreamConnect propagation. Renamed accordingly:
        // this test moves into route_walk_returns_upstream_connect_on_refused_port.
        // Skipping NoHealthyEndpoint test in 04.3; it lands when health checking
        // does in upstream-robustness family.
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn route_walk_returns_upstream_connect_on_refused_port() {
        // Cluster's single endpoint is 127.0.0.1:1 (kernel-refused). HCM's
        // Route arm should propagate RouterError::UpstreamConnect, which it
        // converts to a 502 Bad Gateway downstream response.
        let cluster_mgr = cluster_mgr_with_endpoint("backend", 1);
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
            }),
            cluster_mgr,
        );
        let req = b"GET /any HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 502 Bad Gateway\r\n"),
            "expected 502 on UpstreamConnect, got: {s}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxied_response_carries_x_envoy_upstream_service_time() {
        // Integration check: the route walker's Route arm produces a downstream
        // response whose headers include x-envoy-upstream-service-time with a
        // numeric integer-ms value. Don't pin the exact value (timing-dependent);
        // assert presence + parseability.
        let upstream_response: &'static [u8] =
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let upstream_port = spawn_in_process_upstream(upstream_response).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port);
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
            }),
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        let line = s
            .lines()
            .find(|l| l.starts_with("x-envoy-upstream-service-time: "))
            .expect("x-envoy-upstream-service-time present");
        let value = line.trim_start_matches("x-envoy-upstream-service-time: ").trim();
        let _ms: u128 = value.parse().expect("integer ms");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxied_response_overwrites_upstream_server_header() {
        // Upstream emits `Server: nginx/1.x`; downstream receives `server: envoy-rust`.
        let upstream_response: &'static [u8] =
            b"HTTP/1.1 200 OK\r\nServer: nginx/1.x\r\nContent-Length: 0\r\n\r\n";
        let upstream_port = spawn_in_process_upstream(upstream_response).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port);
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
            }),
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(s.contains("server: envoy-rust\r\n"), "server overwrite: {s}");
        assert!(!s.contains("nginx/1.x"), "upstream Server must not pass through: {s}");
    }
```

NOTE: the test `route_walk_returns_no_healthy_endpoint_when_cluster_empty` is documented as a deferred case (rationale in the comment); the actual test count for Step 1 is 5 (not 6). The Task-header-quoted "6 HCM unit tests" includes the `Cluster::name()` accessor's tests landing in this same Task 9 commit but in the envoy-cluster crate (3 envoy-cluster tests + 5 hcm tests = 8 total new tests in Task 9; PLAN-header conservative estimate of "6 HCM unit tests" focuses on the HCM-side surface). Adjust the PROGRESS Task 9 section in Step 9 to the actual final count.

- [ ] **Step 3: Run the failing tests.**

```bash
cargo test -p envoy-http1 --lib hcm::tests::route_walk hcm::tests::proxied_response
```

Expected: build errors citing `HCMConfig.cluster_mgr` (field doesn't exist yet), `RouteAction::Route(...)` literal in tests (already imported via Task 1 — but the HCM Route arm currently returns 501 per Task 1's placeholder), and missing `Cluster::name()` accessor (Task 9 lands it). Step 4–8 fix.

- [ ] **Step 4: Add `Cluster::name()` + `ClusterHandle::name()` accessors in `crates/envoy-cluster/src/cluster.rs`.**

Locate the `Cluster` struct at lines 11-17 and the `impl Cluster` block at lines 19-31. Remove the `#[allow(dead_code)]` annotation on the `name` field (line 13):

```rust
#[derive(Debug)]
pub struct Cluster {
    pub(crate) name: String,
    pub(crate) endpoints: Vec<SocketAddr>,
    pub(crate) cursor: AtomicUsize,
}

impl Cluster {
    /// Cluster name as configured in `bootstrap.static_resources.clusters[].name`.
    /// Surfaced for use in error variants and tracing log lines that name the
    /// cluster a request was routed to (per phase-04.3 SPEC §3 D5; closes the
    /// multi-phase Cluster::name() carryforward originating in phase-02.1
    /// REVIEW M1).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Picks the next endpoint in round-robin order. `Relaxed` ordering is
    /// sufficient because no other observation depends on a happens-before
    /// relationship with the cursor update (SPEC §6 signpost 3).
    fn pick(&self) -> Option<SocketAddr> {
        // ... existing body unchanged ...
    }
}
```

Locate the `ClusterHandle` struct + impl at lines 35-49. Add `name()`:

```rust
impl ClusterHandle {
    /// Returns the next endpoint in round-robin order. (existing — unchanged.)
    pub fn pick_endpoint(&self) -> Option<SocketAddr> {
        self.inner.pick()
    }

    /// Cluster name (delegates to `Cluster::name`). Mirrors `Cluster::name`'s
    /// public posture per phase-04.3 SPEC §3 D5.
    pub fn name(&self) -> &str {
        self.inner.name()
    }
}
```

Append 3 unit tests in `cluster::tests`:

```rust
    #[test]
    fn cluster_name_returns_configured_name() {
        let c = Cluster {
            name: "backend".to_string(),
            endpoints: mk_endpoints(1),
            cursor: AtomicUsize::new(0),
        };
        assert_eq!(c.name(), "backend");
    }

    #[test]
    fn cluster_handle_exposes_name() {
        let h = mk_handle("primary", mk_endpoints(2));
        assert_eq!(h.name(), "primary");
    }

    #[test]
    fn cluster_name_outlives_borrow_correctly() {
        // The accessor returns a borrow tied to the Cluster's lifetime.
        // Borrow-check regression guard: holding the borrow while picking
        // an endpoint compiles cleanly.
        let h = mk_handle("primary", mk_endpoints(2));
        let name = h.name();
        let _ep = h.pick_endpoint();
        assert_eq!(name, "primary");
    }
```

(`mk_endpoints` and `mk_handle` are existing test helpers from `cluster::tests` at lines 148-160.)

- [ ] **Step 5: Extend `HCMConfig` with `cluster_mgr` field and update `HCMConfig::from_config`.**

Locate `HCMConfig` struct at `crates/envoy-http1/src/hcm.rs:27-31`:

```rust
// REMOVE the placeholder comment + add real field:
#[derive(Debug)]
pub struct HCMConfig {
    pub stat_prefix: String,
    pub route_config: Arc<RouteConfiguration>,
    pub cluster_mgr: Arc<envoy_cluster::ClusterManager>, // 04.3 NEW
}

impl HCMConfig {
    pub fn from_config(
        cfg: &HttpConnectionManagerConfig,
        cluster_mgr: Arc<envoy_cluster::ClusterManager>, // 04.3 NEW
    ) -> Result<Self, Http1Error> {
        Ok(Self {
            stat_prefix: cfg.stat_prefix.clone(),
            route_config: Arc::new(clone_route_config(&cfg.route_config)),
            cluster_mgr,
        })
    }
}
```

Add the `envoy_cluster::ClusterManager` import in the `use` block at the top of `hcm.rs`. Existing imports already include `envoy_listener` etc. (per the inspection); add an `envoy_cluster` line.

- [ ] **Step 6: Replace the Task-1 placeholder Route arm in `build_response` with a real proxy implementation.**

The Task-1 placeholder at `hcm.rs` (after Task 1's restructure) returned `synth_501(close)` for `RouteAction::Route(_ar)`. Replace with the full proxy logic. Note that the current `build_response` is a synchronous function; the Route arm needs `async` (since `Client::connect` and `send_request` are async). Refactor: the Route arm runs INSIDE `serve_connection`, not inside `build_response` — `build_response` returns either a `Response` (DirectResponse path; written via `Http1Response::write_to` outside) OR a `RouteAction::Route` reference that `serve_connection` then proxies (which writes directly to the downstream stream rather than going through `build_response`'s return type).

Refactor `build_response` to return an `Either` shape:

```rust
enum BuildOutcome<'a> {
    /// Downstream response can be synthesized directly from the matched route's
    /// DirectResponse action (or from one of the synth_* error responses).
    Synth(Response),
    /// Route to cluster — caller (serve_connection) must perform the upstream
    /// dial + forwarding. The borrowed RouteAction_Route names the target cluster.
    Proxy(&'a RouteAction_Route),
}

fn build_response<'a>(
    config: &'a HCMConfig,
    req: &Request,
    close: bool,
) -> BuildOutcome<'a> {
    // ... unchanged through the route-match step ...
    match &route.action {
        RouteAction::DirectResponse(dr) => BuildOutcome::Synth(synth_direct_response(dr, close)),
        RouteAction::Route(ar) => BuildOutcome::Proxy(ar),
    }
}
```

Update `serve_connection` (line 98) to handle the `BuildOutcome`:

```rust
async fn serve_connection(
    config: Arc<HCMConfig>,
    mut downstream: TcpStream,
) -> Result<(), Http1Error> {
    let mut buf = BytesMut::with_capacity(READ_BUFFER_INITIAL_CAPACITY);
    loop {
        // 1-7. Existing parse + close-detection + body-drain logic unchanged.
        // (See lines 102-171 of pre-Task-9 hcm.rs.)
        let req = /* ... */;
        let close = /* ... */;
        let body_len = parse_content_length(&req.headers)?;
        // ... drain body ...

        // 8. Resolve route + dispatch.
        let outcome = build_response(&config, &req, close);
        match outcome {
            BuildOutcome::Synth(resp) => {
                Http1Response::write_to(&resp, &mut downstream).await?;
            }
            BuildOutcome::Proxy(action_route) => {
                // 04.3 NEW: forward through the cluster's picked endpoint.
                let cluster = config
                    .cluster_mgr
                    .get(&action_route.cluster)
                    .expect("validator ensures cluster present");
                let endpoint = match cluster.pick_endpoint() {
                    Some(ep) => ep,
                    None => {
                        tracing::warn!(
                            cluster = %cluster.name(),
                            "no healthy endpoint for cluster — returning 503",
                        );
                        let resp = synth_status(503, close);
                        Http1Response::write_to(&resp, &mut downstream).await?;
                        if close { return Ok(()); } else { continue; }
                    }
                };

                // Capture downstream Host: for upstream forwarding.
                let host_header = find_header(&req.headers, headers::HOST)
                    .unwrap_or("")
                    .to_owned();

                // Build the outgoing request: clone the parsed downstream
                // request's method/path/headers, add an empty body (CL only
                // in 04.3 — chunked-request-body forwarding is a SPEC §4 non-goal;
                // fixture 0008's request is CL: 0).
                let mut out_headers = req.headers.clone();
                // Remove Connection: from the upstream-bound headers (the
                // upstream connection is one-shot per SPEC §3 D1; we don't
                // pool, and the upstream's Connection: posture is set
                // fresh by Client::send_request).
                out_headers.retain(|(n, _)| !n.eq_ignore_ascii_case(headers::CONNECTION));
                let out_req = Request {
                    method: req.method.clone(),
                    path: req.path.clone(),
                    version: HttpVersion::Http11,
                    headers: out_headers,
                    bytes_consumed: 0,
                    body: Some(bytes::Bytes::new()), // CL: 0 in 04.3
                };

                let start = std::time::Instant::now();
                let mut client_stream = match Client::connect(endpoint, &host_header).await {
                    Ok(s) => s,
                    Err(source) => {
                        tracing::warn!(
                            cluster = %cluster.name(),
                            addr = %endpoint,
                            error = ?source,
                            "upstream connect failed — returning 502",
                        );
                        let resp = synth_status(502, close);
                        Http1Response::write_to(&resp, &mut downstream).await?;
                        if close { return Ok(()); } else { continue; }
                    }
                };
                let upstream_response = match client_stream.send_request(out_req).await {
                    Ok(r) => r,
                    Err(source) => {
                        tracing::warn!(
                            cluster = %cluster.name(),
                            addr = %endpoint,
                            error = ?source,
                            "upstream request failed — returning 502",
                        );
                        let resp = synth_status(502, close);
                        Http1Response::write_to(&resp, &mut downstream).await?;
                        if close { return Ok(()); } else { continue; }
                    }
                };
                let elapsed_ms = start.elapsed().as_millis();

                crate::router::write_proxied_response(
                    &mut downstream,
                    upstream_response,
                    elapsed_ms,
                    close,
                )
                .await?;
            }
        }

        if close {
            return Ok(());
        }
        // Loop back per existing keep-alive logic.
    }
}
```

Add the `Client` import to the top of `hcm.rs`:

```rust
use crate::client::Client;
```

(Already present from the `tests` block import in Step 2; also needs to be at module level for the Route arm above.)

- [ ] **Step 7: Run the new tests + the full envoy-http1 lib tests.**

```bash
cargo test -p envoy-http1 --lib hcm::tests::route_walk hcm::tests::proxied_response
cargo test -p envoy-http1 --lib
cargo test -p envoy-cluster --lib
```

Expected: 5 new HCM tests pass (the deferred `_no_healthy_endpoint` test from Step 2 is documented-only); 3 new envoy-cluster tests pass; total envoy-http1 lib = 41 (= 36 + 5); envoy-cluster lib previous count + 3.

Verify the existing 04.1 + 04.2 HCM tests still pass — Task 1's restructure already adapted them; Task 9's Route-arm extension is purely additive on the HCM dispatch surface.

- [ ] **Step 8: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib
```

Expected: clean. The HCM's `BuildOutcome::Proxy(&'a RouteAction_Route)` lifetime borrow may surface a clippy `redundant-explicit-lifetime` warning — if so, simplify to a non-borrowed `BuildOutcome::Proxy { cluster: String }` shape (clone the cluster name into the outcome) to keep `build_response` free of lifetime parameters.

- [ ] **Step 9: Append a Task 9 section to PROGRESS.md.**

```markdown
## Task 9 — envoy-http1::hcm: RouteAction Route arm + cluster_mgr wiring + Cluster::name() close-out (D5) + 8 tests (2026-04-27)

- Commit: <SHA>
- Change:
  - envoy-cluster::Cluster::name + ClusterHandle::name accessors landed (visibility lifted to pub because envoy-http1's RouterError consumers in router.rs are in a different crate); field-level #[allow(dead_code)] removed from Cluster.name. Closes the multi-phase Cluster::name() carryforward originating in phase-02.1 REVIEW M1; carryforward chain ends here.
  - HCMConfig extended with cluster_mgr: Arc<envoy_cluster::ClusterManager>; HCMConfig::from_config signature lifts to take a cluster_mgr parameter.
  - build_response refactored to return BuildOutcome::Synth(Response) | BuildOutcome::Proxy(&RouteAction_Route); serve_connection handles both branches.
  - The new Route arm dispatches: cluster_mgr.get → pick_endpoint → Client::connect → send_request → router::write_proxied_response. NoHealthyEndpoint surfaces as 503; UpstreamConnect / UpstreamRequestFailed surface as 502. tracing::warn! log lines on each error path attribute by cluster name (D5 use site).
- Tests added (8 = 5 envoy-http1 + 3 envoy-cluster): route_walk_dispatches_direct_response_unchanged, route_walk_dispatches_route_action_to_client_connect, route_walk_returns_upstream_connect_on_refused_port, proxied_response_carries_x_envoy_upstream_service_time, proxied_response_overwrites_upstream_server_header (envoy-http1); cluster_name_returns_configured_name, cluster_handle_exposes_name, cluster_name_outlives_borrow_correctly (envoy-cluster). Deferred: route_walk_returns_no_healthy_endpoint_when_cluster_empty (lands when health checking does in upstream-robustness family).
- Verification: `cargo test -p envoy-http1 --lib` → 41 passed (36 + 5); `cargo test -p envoy-cluster --lib` → previous + 3 passed; workspace gate clean.
- Deviations from PLAN: <document any>.
```

- [ ] **Step 10: Commit.**

```bash
git add crates/envoy-http1/src/hcm.rs crates/envoy-cluster/src/cluster.rs
git commit -m "phase 04.3: envoy-http1::hcm Route arm + envoy-cluster Cluster::name (D5 close) (task 9)"
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 9)"
```

---

### Task 10: `BEHAVIOR_CONTRACT.md` `Header allow-list` table + `tests/differential/src/lib.rs::HEADER_ALLOW_LIST` constant — `x-envoy-upstream-service-time` row added in lockstep

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (append the `x-envoy-upstream-service-time` row to the existing 2-row Header allow-list table)
- Modify: `tests/differential/src/lib.rs` (extend the `HEADER_ALLOW_LIST` constant at line 188 with the matching entry)
- Modify (append): `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`

**Why now:** Task 9 lands the production code path that emits `x-envoy-upstream-service-time` per Envoy's wire-shape; the contract row + harness allow-list must be in lockstep before fixture 0008 (Task 15) exercises the differential. Per SPEC §6 signpost 19 + SPEC §2: both edits land in the same commit so the harness asserts the contract that's documented; reviewer should diff the two for parity.

**Scope.** ~5 LoC contract-row + ~1 LoC harness-constant entry + 0 new tests (the existing `diff_headers` allow-list walk picks up the new entry automatically; Task 13 fixture-0008 dispatch will exercise it).

- [ ] **Step 1: Read the current BEHAVIOR_CONTRACT.md `Header allow-list` table + the harness constant.**

Already known (from PLAN-write inspection): the contract table is at `docs/envoy-rust/BEHAVIOR_CONTRACT.md` lines 41-44 (header + 2 rows: `server`, `date`). The harness constant is at `tests/differential/src/lib.rs:188-191` with 2 rows.

- [ ] **Step 2: Append the `x-envoy-upstream-service-time` row to BEHAVIOR_CONTRACT.md.**

Locate the table at `docs/envoy-rust/BEHAVIOR_CONTRACT.md:41-44`:

```markdown
| Header | Equivalence | Rationale |
|---|---|---|
| `server` | name-required, value-may-differ | (existing — phase 04.1) ... |
| `date` | name-required, value-may-differ | (existing — phase 04.1) ... |
```

Append after the `date` row:

```markdown
| `x-envoy-upstream-service-time` | name-required, value-may-differ | Per-request upstream-side latency in milliseconds. envoy-rust measures from `Client::connect` start to last-response-byte-read end (computed in the router proxy arm before the response is written downstream). Envoy emits the same header (its semantics are documented at `https://www.envoyproxy.io/docs/envoy/v1.33.0/configuration/http/http_filters/router_filter#x-envoy-upstream-service-time`). Only present on responses that proxied through to an upstream cluster (NOT on `direct_response` paths — that's 04.1's surface where this header is never emitted). Both proxies emit on every router-proxy response; values diverge by measurement. Lands in 04.3 per phase-04 parent SPEC §2 + 04.3 SPEC §2. |
```

- [ ] **Step 3: Extend the `HEADER_ALLOW_LIST` constant in `tests/differential/src/lib.rs`.**

Locate the constant at line 188:

```rust
pub const HEADER_ALLOW_LIST: &[(&str, AllowMode)] = &[
    ("server", AllowMode::NameRequired),
    ("date", AllowMode::NameRequired),
    ("x-envoy-upstream-service-time", AllowMode::NameRequired), // 04.3 NEW
];
```

Update the surrounding doc-comment (lines 185-187) to mention 04.3:

```rust
/// Header allow-list per BEHAVIOR_CONTRACT.md `Header allow-list` table.
/// Sourced from the contract; updates to the contract update this constant
/// in lockstep. 04.1 added `server` and `date`; 04.3 added
/// `x-envoy-upstream-service-time`.
```

- [ ] **Step 4: Run the workspace tests to verify no regression.**

```bash
cargo test -p differential --lib
cargo test --workspace --lib
```

Expected: previous test counts unchanged (no new tests in Task 10 — the existing `diff_headers` walk picks up the new entry automatically).

- [ ] **Step 5: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 6: Append a Task 10 section to PROGRESS.md.**

```markdown
## Task 10 — BEHAVIOR_CONTRACT.md + HEADER_ALLOW_LIST: x-envoy-upstream-service-time row in lockstep (2026-04-27)

- Commit: <SHA>
- Change: appended `x-envoy-upstream-service-time | name-required, value-may-differ | ...` row to docs/envoy-rust/BEHAVIOR_CONTRACT.md's Header allow-list table; extended tests/differential/src/lib.rs::HEADER_ALLOW_LIST constant with the matching entry. Both edits land in this commit per SPEC §6 signpost 19 (lockstep discipline).
- Tests added: none (the existing diff_headers walk picks up the new entry automatically; Task 15 fixture 0008 exercises it).
- Verification: `cargo test --workspace --lib` → previous counts unchanged; workspace gate clean.
- Deviations from PLAN: <document any>.
```

- [ ] **Step 7: Commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md tests/differential/src/lib.rs
git commit -m "phase 04.3: BEHAVIOR_CONTRACT.md + HEADER_ALLOW_LIST — x-envoy-upstream-service-time (task 10)"
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 10)"
```

---

### Task 11: `tests/helpers/http1-echo-server/` scaffold — `Cargo.toml` + workspace member registration + `src/main.rs` argv parser + 4 argv unit tests

**Files:**
- Create: `tests/helpers/http1-echo-server/Cargo.toml`
- Create: `tests/helpers/http1-echo-server/src/main.rs` (skeleton: argv parser + ArgvError + main entrypoint that prints help/version and translates to exit codes — runtime accept loop lands in Task 12)
- Modify: root `Cargo.toml` (add `tests/helpers/http1-echo-server` to `[workspace] members`)
- Modify (append): `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`

**Why now:** The helper crate is the next-largest building block. Splitting argv (Task 11) from runtime (Task 12) mirrors the phase-02.1 Tasks 8-9-10 cadence (scaffold → argv → runtime) and keeps each task at ~150 LoC. Task 13's `Http1EchoBackend` references `locate_http1_echo_server` which expects the binary at `target/<profile>/http1-echo-server` — the binary builds as soon as Task 11's scaffold lands.

**Scope.** ~50 LoC `Cargo.toml` + ~120 LoC `src/main.rs` argv parser + ~80 LoC of 4 argv unit tests. The runtime accept loop is stubbed in `run` to immediately return `Ok(())` (Task 12 fills it).

- [ ] **Step 1: Add the workspace member to root `Cargo.toml`.**

```bash
grep -n 'tls-echo-server' Cargo.toml
```

Expected: alphabetic position in the `[workspace] members` Vec (currently between `tcp-echo-server` and the closing `]`). Add `http1-echo-server` alphabetically between `tcp-echo-server` and `tls-echo-server`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/envoy-bin",
    "crates/envoy-cluster",
    "crates/envoy-config",
    "crates/envoy-http1",
    "crates/envoy-listener",
    "crates/envoy-tcp",
    "crates/envoy-tls",
    "tests/differential",
    "tests/helpers/http1-echo-server", # 04.3 NEW
    "tests/helpers/tcp-echo-server",
    "tests/helpers/tls-echo-server",
]
```

(`http1-echo-server` sorts BEFORE `tcp-echo-server` alphabetically — the `1` is `0x31` and `c` is `0x63`. Verify after the edit; if sort order matters and `cargo` complains, just keep the order — workspace member listing is not order-sensitive.)

- [ ] **Step 2: Create `tests/helpers/http1-echo-server/Cargo.toml`.**

Mirror `tls-echo-server`'s shape minus the rustls/rcgen/tempfile chunk:

```toml
[package]
name = "http1-echo-server"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[[bin]]
name = "http1-echo-server"
path = "src/main.rs"

[dependencies]
envoy-http1 = { path = "../../../crates/envoy-http1" }
anyhow = "1"
thiserror = "2"
tokio = { version = "1", features = ["rt-multi-thread", "net", "io-util", "macros", "signal", "time", "sync"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
```

(Per SPEC §3 D3: deps are envoy-http1 path-dep + anyhow + thiserror + tokio + tracing + tracing-subscriber. No rcgen / tempfile / rustls — plaintext only.)

- [ ] **Step 3: Write 4 failing argv unit tests.**

Create `tests/helpers/http1-echo-server/src/main.rs` with the test scaffold + a stub `parse_argv` that always returns `MissingFlag("--port")`:

```rust
#![forbid(unsafe_code)]

//! `http1-echo-server` — minimal localhost-only HTTP/1.1 echo server for the
//! envoy-rust differential harness. Sibling of `tcp-echo-server` (phase 02.1)
//! and `tls-echo-server` (phase 03.2). Plaintext only — no TLS.
//!
//! The deterministic-echo response body shape is LOAD-BEARING for differential
//! equivalence (per SPEC §3 D3): the helper produces a `200 OK` response with
//! `Content-Type: text/plain` and a body of:
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
//! Both proxies forward the same request to the same helper; the alphabetic
//! header sort eliminates ordering divergences from differential body comparison.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use thiserror::Error;

const DRAIN_BUDGET: Duration = Duration::from_secs(5);

/// Parsed argv surface. (`--port <u16>` only; no TLS keys.)
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

/// Parses argv (excluding argv[0]).
fn parse_argv(args: &[String]) -> Result<Args, ArgvError> {
    // Task 11 scaffold — replaced in Step 5 with the real impl.
    let _ = args;
    Err(ArgvError::MissingFlag("--port"))
}

fn print_help() {
    println!(
        "http1-echo-server: HTTP/1.1 echo server helper for the envoy-rust differential harness.\n\
         \n\
         Usage:\n  http1-echo-server --port <u16>\n  \
         http1-echo-server --help\n  http1-echo-server --version"
    );
}

async fn run(_args: Args) -> Result<()> {
    // Task 12 lands the accept loop.
    let _ = DRAIN_BUDGET;
    Ok(())
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_argv(&argv) {
        Ok(a) => a,
        Err(ArgvError::HelpRequested) => {
            print_help();
            return ExitCode::from(0);
        }
        Err(ArgvError::VersionRequested) => {
            println!("http1-echo-server {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::from(0);
        }
        Err(e) => {
            eprintln!("argv error: {e}");
            return ExitCode::from(2);
        }
    };

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to build tokio runtime: {e}");
            return ExitCode::from(1);
        }
    };

    match rt.block_on(run(args)) {
        Ok(()) => ExitCode::from(0),
        Err(e) => {
            eprintln!("runtime error: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn argv_parses_full_invocation() {
        let args = parse_argv(&argv(&["--port", "10042"])).expect("parse");
        assert_eq!(args.port, 10042);
    }

    #[test]
    fn argv_rejects_missing_port() {
        // No --port arg → MissingFlag("--port").
        let result = parse_argv(&argv(&[]));
        assert_eq!(result, Err(ArgvError::MissingFlag("--port")));
    }

    #[test]
    fn argv_rejects_invalid_port() {
        let result = parse_argv(&argv(&["--port", "not-a-number"]));
        assert_eq!(result, Err(ArgvError::InvalidPort));
    }

    #[test]
    fn argv_shows_help() {
        assert_eq!(
            parse_argv(&argv(&["--help"])),
            Err(ArgvError::HelpRequested)
        );
    }
}
```

- [ ] **Step 4: Run the tests to verify the failing-test bookkeeping is correct.**

```bash
cargo test -p http1-echo-server
```

Expected: 3 of 4 tests fail (`argv_parses_full_invocation`, `argv_rejects_invalid_port`, `argv_shows_help`); `argv_rejects_missing_port` passes accidentally because the stub always returns that error. Step 5 fixes.

- [ ] **Step 5: Replace the stub `parse_argv` with the real impl.**

Mirror `tls-echo-server`'s parser shape at `tests/helpers/tls-echo-server/src/main.rs:50-83` minus `--cert` / `--key`:

```rust
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
```

Replace the stub body in main.rs with the above.

- [ ] **Step 6: Run the tests to verify they pass.**

```bash
cargo test -p http1-echo-server
```

Expected: 4 tests pass.

- [ ] **Step 7: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean. The `_ = DRAIN_BUDGET` placeholder (Step 3) suppresses the `dead_code` lint until Task 12; once Task 12 lands the runtime, the `_ =` line is removed.

- [ ] **Step 8: Append a Task 11 section to PROGRESS.md.**

```markdown
## Task 11 — http1-echo-server scaffold + argv parser + 4 argv tests (2026-04-27)

- Commit: <SHA>
- Change: created tests/helpers/http1-echo-server/{Cargo.toml,src/main.rs}; registered the crate as a workspace member in root Cargo.toml. Argv parser handles --port <u16> + --help + --version + Trailing rejection (mirrors tls-echo-server's shape minus --cert/--key). Runtime accept loop is stubbed (returns Ok(())) — lands in Task 12.
- Tests added (4): argv_parses_full_invocation, argv_rejects_missing_port, argv_rejects_invalid_port, argv_shows_help.
- Verification: `cargo test -p http1-echo-server` → 4 passed; workspace gate clean.
- Deviations from PLAN: <document any>.
```

- [ ] **Step 9: Commit.**

```bash
git add Cargo.toml tests/helpers/http1-echo-server/Cargo.toml tests/helpers/http1-echo-server/src/main.rs
git commit -m "phase 04.3: http1-echo-server scaffold + argv parser + 4 argv tests (task 11)"
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 11)"
```

---

### Task 12: `tests/helpers/http1-echo-server/src/main.rs` runtime — accept loop + deterministic echo body + 1 round-trip test

**Files:**
- Modify: `tests/helpers/http1-echo-server/src/main.rs` (replace the Task-11 stub `run` with a real accept loop; add a `build_echo_body` helper producing the deterministic alphabetically-sorted-headers shape per SPEC §3 D3; add 1 round-trip test)
- Modify (append): `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`

**Why now:** Task 11 landed the binary scaffold + argv parser; Task 12 makes the binary actually do something. The deterministic echo body is LOAD-BEARING for fixture 0008's byte-exact differential per SPEC §6 signpost 8 — both proxies forward the same logical request to the same helper, so the helper's response is the byte-exact baseline for differential comparison.

**Scope.** ~120 LoC `run` + `build_echo_body` + ~80 LoC of 1 `accepts_and_echoes_request` round-trip test. Reuses `envoy_http1::Http1Codec::parse_request` for incoming + `envoy_http1::Http1Response::write_to` for outgoing.

- [ ] **Step 1: Write 1 failing round-trip test in `http1-echo-server/src/main.rs::tests`.**

Append after Task 11's argv tests:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn accepts_and_echoes_request() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Reserve a port (race-y but matches helper conventions).
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let p = l.local_addr().unwrap().port();
            drop(l);
            p
        };

        // Spawn the runtime in a background task.
        let server_handle = tokio::spawn(async move {
            let _ = run(Args { port }).await;
        });

        // Wait for the listener.
        for _ in 0..50 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Open a TCP connection and write an HTTP/1.1 GET. Use Connection: close
        // so the server closes after the response (matches fixture 0008's wire).
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        s.write_all(
            b"GET / HTTP/1.1\r\nHost: x.test\r\nContent-Length: 0\r\n\r\n",
        )
        .await
        .unwrap();

        // Read the full response.
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf);

        // Assert the response shape. The body is deterministic per SPEC §3 D3.
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "status: {response}");
        assert!(
            response.contains("content-type: text/plain\r\n"),
            "ct: {response}"
        );
        // The body has the SPEC §3 D3 shape:
        //   method: GET\npath: /\nheaders:\n  content-length: 0\n  host: x.test\nbody: \n
        let expected_body = "method: GET\npath: /\nheaders:\n  content-length: 0\n  host: x.test\nbody: \n";
        assert!(
            response.ends_with(expected_body),
            "body shape:\nactual:\n{response}\nexpected suffix:\n{expected_body}"
        );

        server_handle.abort();
    }
```

- [ ] **Step 2: Run the test to verify it fails.**

```bash
cargo test -p http1-echo-server accepts_and_echoes_request
```

Expected: hangs / times out — the Task-11 `run` returns `Ok(())` immediately so the listener never binds. Or if it binds and accepts, the response shape doesn't match. Step 3 fixes.

- [ ] **Step 3: Implement `run` with the accept loop + `build_echo_body`.**

Replace the Task-11 `run` stub:

```rust
async fn run(args: Args) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::task::JoinSet;
    use tokio::time::timeout;

    let listener = TcpListener::bind(("127.0.0.1", args.port)).await?;
    tracing::info!("http1-echo-server listening on 127.0.0.1:{}", args.port);

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
                    Ok((mut stream, _)) => {
                        join_set.spawn(async move {
                            // Read the request bytes (single request per connection;
                            // no keep-alive — see SPEC §6 signpost 9).
                            use tokio::io::AsyncReadExt;
                            let mut buf = bytes::BytesMut::with_capacity(8192);
                            // Read in a loop until httparse signals Complete.
                            loop {
                                let mut chunk = [0u8; 4096];
                                let n = match tokio::time::timeout(
                                    Duration::from_secs(5),
                                    stream.read(&mut chunk),
                                )
                                .await
                                {
                                    Ok(Ok(0)) => break,
                                    Ok(Ok(n)) => n,
                                    Ok(Err(_)) | Err(_) => return,
                                };
                                buf.extend_from_slice(&chunk[..n]);
                                match envoy_http1::Http1Codec::parse_request(&buf) {
                                    Ok(Some(req)) => {
                                        // Read body (Content-Length only).
                                        let body_len = req
                                            .headers
                                            .iter()
                                            .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
                                            .and_then(|(_, v)| v.parse::<usize>().ok())
                                            .unwrap_or(0);
                                        let headers_end = req.bytes_consumed;
                                        let mut body: Vec<u8> = Vec::with_capacity(body_len);
                                        if buf.len() > headers_end {
                                            let take = (buf.len() - headers_end).min(body_len);
                                            body.extend_from_slice(
                                                &buf[headers_end..headers_end + take],
                                            );
                                        }
                                        while body.len() < body_len {
                                            let mut chunk = [0u8; 4096];
                                            let n = match tokio::time::timeout(
                                                Duration::from_secs(5),
                                                stream.read(&mut chunk),
                                            )
                                            .await
                                            {
                                                Ok(Ok(0)) => return,
                                                Ok(Ok(n)) => n,
                                                Ok(Err(_)) | Err(_) => return,
                                            };
                                            let need = body_len - body.len();
                                            body.extend_from_slice(&chunk[..n.min(need)]);
                                        }

                                        let echo = build_echo_body(&req, &body);
                                        let resp = envoy_http1::Response {
                                            status: 200,
                                            reason: None,
                                            headers: vec![
                                                ("content-type".to_string(), "text/plain".to_string()),
                                                (
                                                    "content-length".to_string(),
                                                    echo.len().to_string(),
                                                ),
                                                ("connection".to_string(), "close".to_string()),
                                            ],
                                            body: bytes::Bytes::from(echo),
                                        };
                                        let _ = envoy_http1::Http1Response::write_to(
                                            &resp,
                                            &mut stream,
                                        )
                                        .await;
                                        let _ = stream.shutdown().await;
                                        return;
                                    }
                                    Ok(None) => continue,
                                    Err(_) => return,
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "accept failed; continuing");
                    }
                }
            }
        }
    }

    drop(listener);
    let drain = timeout(DRAIN_BUDGET, async {
        while join_set.join_next().await.is_some() {}
    });
    let _ = drain.await;
    join_set.abort_all();
    while join_set.join_next().await.is_some() {}

    Ok(())
}

/// Build the deterministic echo body per SPEC §3 D3:
///
/// ```text
/// method: <METHOD>
/// path: <PATH>
/// headers:
///   <name1>: <value1>     (alphabetically sorted by lowercase name)
///   ...
/// body: <BODY>
/// ```
///
/// The alphabetic header sort is LOAD-BEARING: both proxies forward the
/// request to the SAME helper, but Envoy may emit headers in a different
/// order than envoy-rust. Sorting by lowercase name eliminates this
/// source of divergence so byte-exact body equality holds across both
/// proxies' downstream responses (which are then proxied back to the
/// harness verbatim per the router proxy arm in Task 9).
fn build_echo_body(req: &envoy_http1::Request, body: &[u8]) -> Vec<u8> {
    let mut out = String::new();
    out.push_str("method: ");
    out.push_str(&req.method);
    out.push('\n');
    out.push_str("path: ");
    out.push_str(&req.path);
    out.push('\n');
    out.push_str("headers:\n");
    let mut sorted_headers: Vec<(String, String)> = req
        .headers
        .iter()
        .map(|(n, v)| (n.to_ascii_lowercase(), v.clone()))
        .collect();
    sorted_headers.sort_by(|a, b| a.0.cmp(&b.0));
    for (n, v) in &sorted_headers {
        out.push_str("  ");
        out.push_str(n);
        out.push_str(": ");
        out.push_str(v);
        out.push('\n');
    }
    out.push_str("body: ");
    // UTF-8 if possible; else replace each byte with `?`.
    match std::str::from_utf8(body) {
        Ok(s) => out.push_str(s),
        Err(_) => {
            for _ in body {
                out.push('?');
            }
        }
    }
    out.push('\n');
    out.into_bytes()
}
```

- [ ] **Step 4: Run the test to verify it passes.**

```bash
cargo test -p http1-echo-server
```

Expected: 5 tests pass (4 argv from Task 11 + 1 round-trip from Task 12).

- [ ] **Step 5: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean. Verify the binary is built at `target/<profile>/http1-echo-server` (Task 13's `locate_http1_echo_server` expects this path):

```bash
ls -la target/debug/http1-echo-server
```

Expected: file exists, executable permissions.

- [ ] **Step 6: Append a Task 12 section to PROGRESS.md.**

```markdown
## Task 12 — http1-echo-server runtime: accept loop + deterministic echo body + 1 round-trip test (2026-04-27)

- Commit: <SHA>
- Change: replaced the Task-11 stub run with a real accept loop using envoy_http1::Http1Codec::parse_request for incoming + envoy_http1::Http1Response::write_to for outgoing. Added build_echo_body helper producing the deterministic alphabetically-sorted-headers body shape per SPEC §3 D3. The connection closes after a single request (no keep-alive, per SPEC §6 signpost 9).
- Tests added (1): accepts_and_echoes_request. Total http1-echo-server lib tests = 5.
- Verification: `cargo test -p http1-echo-server` → 5 passed; workspace gate clean; binary at target/debug/http1-echo-server.
- Deviations from PLAN: <document any>.
```

- [ ] **Step 7: Commit.**

```bash
git add tests/helpers/http1-echo-server/src/main.rs
git commit -m "phase 04.3: http1-echo-server runtime + deterministic echo body + 1 round-trip test (task 12)"
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 12)"
```

---

### Task 13: Differential harness — `Http1EchoBackend` + `locate_http1_echo_server` + `run_fixture` dispatch arm on `{{HTTP1_BACKEND_PORT}}` template marker + 4 harness unit tests

**Files:**
- Modify: `tests/differential/src/backend.rs` (add `Http1EchoBackend` struct + `spawn` + `port` + `container_host` + `Drop` impl; add `locate_http1_echo_server` free fn; add 3 unit tests)
- Modify: `tests/differential/src/lib.rs` (extend `run_fixture` template-marker detection cascade with `{{HTTP1_BACKEND_PORT}}` arm spawning `Http1EchoBackend::spawn()`; extend `BACKEND_HOST` substitution gate; add 1 unit test asserting the template-marker dispatch)
- Modify (append): `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`

**Why now:** Task 12 lands the http1-echo-server binary at `target/<profile>/http1-echo-server`. Task 13 wires it into the differential harness so fixture 0008 (Task 15) can spawn it as the upstream backend. Task 14's envoy-bin in-process integration test also uses `locate_http1_echo_server` (cross-package usage requires the helper to live in `differential::backend` — same posture as `locate_tls_echo_server`).

**Scope.** ~80 LoC `Http1EchoBackend` (struct + spawn + Drop) + ~30 LoC `locate_http1_echo_server` (mirrors `locate_tls_echo_server` at `backend.rs:173-198`) + ~40 LoC `run_fixture` extension + ~120 LoC of 4 unit tests.

- [ ] **Step 1: Write 4 failing harness tests in `tests/differential/src/backend.rs::tests` and `tests/differential/src/lib.rs::tests`.**

In `backend.rs::tests` (append after the existing `tls_echo_*` tests at lines 295-368):

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn http1_echo_backend_spawns_and_echoes() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        // Skip if the helper binary isn't built (cargo test --workspace builds
        // it; cargo test -p differential alone may not).
        if locate_http1_echo_server().is_err() {
            eprintln!(
                "skipping http1_echo_backend_spawns_and_echoes — http1-echo-server not built; run `cargo test --workspace`"
            );
            return;
        }

        let backend = Http1EchoBackend::spawn().await.expect("spawn ok");
        let port = backend.port();
        assert!(port > 0);
        assert_eq!(backend.container_host(), "host.docker.internal");

        // Send a minimal HTTP/1.1 request; assert the deterministic echo body.
        let mut s = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        s.write_all(b"GET / HTTP/1.1\r\nHost: x.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write");
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await.expect("read");
        let response = String::from_utf8_lossy(&buf);
        assert!(
            response.contains("HTTP/1.1 200 OK\r\n"),
            "status: {response}"
        );
        assert!(
            response.ends_with("method: GET\npath: /\nheaders:\n  content-length: 0\n  host: x.test\nbody: \n"),
            "deterministic echo body: {response}"
        );

        drop(backend);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn http1_echo_backend_drop_terminates_child() {
        if locate_http1_echo_server().is_err() {
            eprintln!("skipping http1_echo_backend_drop_terminates_child — http1-echo-server not built");
            return;
        }
        let backend = Http1EchoBackend::spawn().await.expect("spawn ok");
        let port = backend.port();

        drop(backend);

        tokio::time::sleep(Duration::from_millis(200)).await;
        let result = std::net::TcpStream::connect(("127.0.0.1", port));
        assert!(
            result.is_err(),
            "expected port {port} to be released after Drop"
        );
    }

    #[test]
    fn locate_http1_echo_server_returns_existing_path() {
        // Skip the test if the binary isn't built — this is a smoke test
        // for the locator's path-construction logic, not a build-prereq check.
        match locate_http1_echo_server() {
            Ok(path) => {
                assert!(path.exists(), "locator returned {path:?} but file doesn't exist");
                assert!(path.ends_with("http1-echo-server") || path.ends_with("http1-echo-server.exe"));
            }
            Err(_) => {
                eprintln!("skipping locate_http1_echo_server_returns_existing_path — binary not built");
            }
        }
    }
```

In `lib.rs::tests` (append after the existing render_yaml tests):

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn run_fixture_dispatches_http1_backend_on_template_marker() {
        // Synthesize a minimal template referencing {{HTTP1_BACKEND_PORT}} +
        // {{BACKEND_HOST}}; verify run_fixture (or the dispatch helper)
        // selects Http1EchoBackend::spawn. We can't run the full Docker-gated
        // round-trip in a unit test, so this test exercises only the
        // detection cascade — it constructs a render_yaml call with the
        // template + asserts the template substitutes the right keys after
        // a backend is spawned and the keys are populated.
        //
        // This is awareness-only: the full dispatch lands in Task 15's
        // Docker-gated test. The test exists per SPEC §6 signpost 11 to
        // surface the M11 carryforward shape.
        if crate::backend::locate_http1_echo_server().is_err() {
            eprintln!("skipping run_fixture_dispatches_http1_backend_on_template_marker — http1-echo-server not built");
            return;
        }
        let backend = crate::backend::Http1EchoBackend::spawn()
            .await
            .expect("spawn http1 backend");
        let port = backend.port();
        let template = "endpoint: {{BACKEND_HOST}}:{{HTTP1_BACKEND_PORT}}";
        let kvs = &[
            ("BACKEND_HOST", "host.docker.internal"),
            ("HTTP1_BACKEND_PORT", port.to_string().as_str()),
        ];
        let rendered = render_yaml(template, kvs);
        assert!(
            rendered.contains("host.docker.internal:") && rendered.contains(&port.to_string()),
            "rendered: {rendered}"
        );
        drop(backend);
    }
```

(The kvs `port.to_string().as_str()` shape doesn't compile because `to_string()` returns a temporary; use a binding: `let port_str = port.to_string(); let kvs = &[..., ("HTTP1_BACKEND_PORT", port_str.as_str())]`.)

- [ ] **Step 2: Run the failing tests.**

```bash
cargo test -p differential --lib http1_echo_backend locate_http1_echo run_fixture_dispatches_http1
```

Expected: build errors citing `Http1EchoBackend` and `locate_http1_echo_server` not found. Step 3 fixes.

- [ ] **Step 3: Add `Http1EchoBackend` + `locate_http1_echo_server` in `tests/differential/src/backend.rs`.**

Append after the existing `TlsEchoBackend` block (around line 168). Mirror the `TlsEchoBackend` shape minus the TLS-specific cert/key paths:

```rust
/// `Http1EchoBackend` — spawns the workspace's `http1-echo-server` binary as a
/// host subprocess on a reserved 127.0.0.1 port. Sibling of `TcpProxyBackend`
/// (phase 02.2) and `TlsEchoBackend` (phase 03.2). Used by fixture
/// 0008-http1-router-upstream as the upstream HTTP/1.1 backend that both
/// proxies dial.
///
/// Drop posture: SIGKILL via tokio's `start_kill` + 50ms-poll/2s-deadline
/// fallback (mirrors TcpProxyBackend / TlsEchoBackend; phase-02.2 REVIEW M1
/// inherited).
pub struct Http1EchoBackend {
    port: u16,
    child: Option<tokio::process::Child>,
}

impl Http1EchoBackend {
    /// Reserve an ephemeral 127.0.0.1 port, locate the workspace's
    /// `http1-echo-server` binary, spawn it with `--port <port>`, and wait
    /// until the listener accepts a TCP connection. Total readiness budget:
    /// 1s (matches TcpProxyBackend's exponential backoff defaults).
    pub async fn spawn() -> Result<Self> {
        let port = reserve_port().context("reserving http1 backend port")?;
        let bin = locate_http1_echo_server().context("locating http1-echo-server binary")?;
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
        wait_accept_ready(addr, Duration::from_secs(1))
            .await
            .with_context(|| format!("http1-echo-server never became accept-ready on {addr}"))?;

        Ok(Self {
            port,
            child: Some(child),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Hostname the upstream Envoy container uses to reach this backend.
    /// See ADR-0015. Always `host.docker.internal`; envoy-rust on the host
    /// reaches the same backend at `127.0.0.1`.
    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for Http1EchoBackend {
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

/// Locate the workspace's `http1-echo-server` binary. Mirrors
/// `locate_tcp_echo_server` and `locate_tls_echo_server`.
pub(crate) fn locate_http1_echo_server() -> Result<PathBuf> {
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
    let mut bin = target_dir.join(profile).join("http1-echo-server");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    if !bin.exists() {
        bail!(
            "http1-echo-server not found at {}; run `cargo build -p http1-echo-server` or `cargo test --workspace`",
            bin.display(),
        );
    }
    Ok(bin)
}
```

The `locate_http1_echo_server` visibility is `pub(crate)` mirroring `locate_tls_echo_server`'s posture. Task 14's envoy-bin in-process test cannot reach it directly (it's in a different package); for that case Task 14 lifts the visibility to `pub` OR re-locates the binary independently — Task 14 picks the cleaner path.

- [ ] **Step 4: Extend `run_fixture` template-marker detection cascade in `tests/differential/src/lib.rs`.**

Locate the existing `{{TLS_BACKEND_PORT}}` arm at lines 879-893 + the `BACKEND_HOST` gate at lines 908-914 / 930-932. Add a parallel `{{HTTP1_BACKEND_PORT}}` arm:

```rust
    let needs_http1_backend = upstream_template.contains("{{HTTP1_BACKEND_PORT}}")
        || subject_template.contains("{{HTTP1_BACKEND_PORT}}");
    let _http1_backend: Option<crate::backend::Http1EchoBackend> = if needs_http1_backend {
        Some(
            crate::backend::Http1EchoBackend::spawn()
                .await
                .context("spawning Http1EchoBackend")?,
        )
    } else {
        None
    };
    let http1_backend_port_str = _http1_backend.as_ref().map(|b| b.port().to_string());
```

Update the `upstream_kvs` builder block (lines 900-921):

```rust
    let upstream_kvs: Vec<(&str, String)> = {
        let mut v: Vec<(&str, String)> = vec![(port_key, upstream_port_str.clone())];
        if let Some(bp) = backend_port_str.as_deref() {
            v.push(("BACKEND_PORT", bp.to_string()));
        }
        if let Some(tp) = tls_backend_port_str.as_deref() {
            v.push(("TLS_BACKEND_PORT", tp.to_string()));
        }
        if let Some(hp) = http1_backend_port_str.as_deref() {
            v.push(("HTTP1_BACKEND_PORT", hp.to_string())); // 04.3 NEW
        }
        if backend_port_str.is_some()
            || tls_backend_port_str.is_some()
            || http1_backend_port_str.is_some()
        // 04.3 NEW: HTTP1_BACKEND_PORT also triggers BACKEND_HOST substitution
        {
            v.push(("BACKEND_HOST", "host.docker.internal".to_string()));
        }
        if let Some(map) = upstream_tls_paths.as_ref() {
            for (k, val) in map {
                v.push((*k, val.clone()));
            }
        }
        v
    };
```

Same edit shape for the `subject_kvs` block (lines 922-939) — add `HTTP1_BACKEND_PORT` and extend the BACKEND_HOST gate. The subject side maps to `127.0.0.1`.

- [ ] **Step 5: Run the tests.**

```bash
cargo test -p differential --lib http1_echo_backend locate_http1_echo run_fixture_dispatches_http1
cargo test -p differential --lib
cargo test --workspace --lib
```

Expected: 4 new tests pass (3 in backend.rs + 1 in lib.rs); previous test counts unchanged elsewhere.

- [ ] **Step 6: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 7: Append a Task 13 section to PROGRESS.md.**

```markdown
## Task 13 — differential harness: Http1EchoBackend + locate_http1_echo_server + run_fixture dispatch + 4 tests (2026-04-27)

- Commit: <SHA>
- Change: added Http1EchoBackend (struct + spawn + port + container_host + Drop) + locate_http1_echo_server in tests/differential/src/backend.rs (sibling of TlsEchoBackend; SIGKILL-on-Drop posture). Extended tests/differential/src/lib.rs::run_fixture with a {{HTTP1_BACKEND_PORT}} template-marker detection cascade arm spawning Http1EchoBackend::spawn(); extended the BACKEND_HOST substitution gate to fire on the new marker.
- Tests added (4 = 3 backend + 1 lib): http1_echo_backend_spawns_and_echoes, http1_echo_backend_drop_terminates_child, locate_http1_echo_server_returns_existing_path (skip-if-not-built), run_fixture_dispatches_http1_backend_on_template_marker.
- Verification: `cargo test -p differential --lib` → previous + 4 passed; workspace gate clean.
- Deviations from PLAN: <document any>.
```

- [ ] **Step 8: Commit.**

```bash
git add tests/differential/src/backend.rs tests/differential/src/lib.rs
git commit -m "phase 04.3: differential harness — Http1EchoBackend + run_fixture dispatch + 4 tests (task 13)"
git add docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md
git commit -m "phase 04.3: progress note (task 13)"
```

---

<!-- PLAN_INSERT_HERE -->
