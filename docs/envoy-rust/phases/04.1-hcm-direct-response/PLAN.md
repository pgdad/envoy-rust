# Phase 04.1 — HTTP/1.1 codec + HCM scaffold + minimal routing + direct_response + fixture 0007 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task (fresh subagent per task + two-stage review). Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **For every code-writing task:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — failing test first, verify fails, minimal implementation, verify passes, commit. No exceptions (doctrine D-3.1).
>
> **Source of truth:** `docs/envoy-rust/phases/04.1-hcm-direct-response/SPEC.md`. This plan operationalizes SPEC §§D1–D9. Where the plan and the SPEC disagree, the SPEC wins — flag the drift, land an ADR per D-3.5, and continue. The parent phase-04 SPEC at `docs/envoy-rust/phases/04-http1/SPEC.md` (committed at SHA `805433e`) is preserved unedited as a historical artifact; for execution it is superseded by sub-phase SPECs (this one for 04.1; `04.2-route-matchers/SPEC.md` for 04.2; `04.3-router-upstream/SPEC.md` for 04.3).

**Goal:** Land the new `envoy-http1` library crate (HTTP/1.1 request codec + per-listener HCM as a `ConnectionHandler` impl + per-connection state machine + route walker + hardcoded router-filter call site + `Http1Response` writer + `Http1Error`); extend `envoy-config` with the `HttpConnectionManager` `TypedConfig` variant + `RouteConfiguration` schema (multi-VH, `domains: ["*"]` or exact-string match, multi-route with `prefix:` / `path:` matchers, `direct_response` action with `inline_string` body) + 10 new `ConfigError` variants + 8 validator tests + 2 fuzz-corpus seeds; wire `envoy-bin` to dispatch HCM via a new `TypedConfig::HttpConnectionManager` arm in the listener-walk; extend the differential harness with `Driver::Http1` + `drive_http1` + `HEADER_ALLOW_LIST` + `diff_headers`; populate BEHAVIOR_CONTRACT.md's `Header allow-list` table with its first two entries (`server`, `date`); ship fixture `0007-http1-direct-response` byte-exact green against upstream Envoy `v1.33.0`.

**Architecture:** `crates/envoy-http1/` is the workspace's sole runtime owner of the `httparse` dependency (per parent-SPEC §3 cross-sub-phase rule 1; envoy-bin's admin endpoint is the pre-existing `httparse` consumer and is not refactored in 04.1 — the rule is a posture statement that takes effect when admin is next touched). Public surface: `Http1Codec::parse_request(buf)` (stateless wrapper over `httparse::Request::parse` returning `Result<Option<Request>, Http1Error>`); `Request` / `Response` value types with case-preserving `Vec<(String, String)>` headers; `find_header` case-insensitive lookup; `format_imf_fixdate(SystemTime) -> String` (hand-rolled ~30 LoC; no `httpdate` dep); `Http1Response::write_to::<W: AsyncWrite>` writer (CL-framed body in 04.1; chunked deferred to 04.3); `HCM` struct + `HCMConfig` Arc-shareable per-listener config + `HCM: ConnectionHandler` impl that runs a per-connection state machine reading `bytes::BytesMut`, parsing requests, walking `route_config.virtual_hosts` (first-match-wins on `Host:` value with port stripped; `domains: ["*"]` catch-all or exact-string match) and `vh.routes` (first-match-wins on `path` against `prefix:` / `path:` matchers), and dispatching the matched route's `direct_response` through a hardcoded router-filter call site (`match action { DirectResponse(dr) => synth_direct_response(req, dr) }` — exhaustive in 04.1 because the schema only parses `direct_response`; 04.3 adds a second arm). Five hardcoded response headers per response: `server: envoy-rust`, `date: <IMF-fixdate>`, `content-length: <body.len()>`, `content-type: text/plain`, `connection: <keep-alive|close>`. HTTP/1.1 keep-alive default; idle 5s read timeout; per-connection request body drain on `Content-Length: N`; `Transfer-Encoding: chunked` requests reject with 501. envoy-config grows the HCM `TypedConfig` arm + `RouteConfiguration` / `VirtualHost` / `Route` / `RouteMatch` / `DirectResponse` types + extends `DataSource` with `inline_string: Option<String>` (turning the existing `filename: String` into `Option<String>` and enforcing "exactly one of {filename, inline_string} is Some" plus per-callsite restrictions); 8 new validator unit tests; 2 new HCM-shaped fuzz-corpus seeds. envoy-bin walks the listener's first filter chain's first filter and dispatches on `TypedConfig::HttpConnectionManager(hcm_cfg)` by constructing `Arc<HCMConfig>` once via `HCMConfig::from_config(&hcm_cfg)?`, wrapping in `Arc::new(HCM { config }) as Arc<dyn ConnectionHandler>`, threading optionally through the existing `TlsAcceptingHandler` if `transport_socket: Some(_)` (unreachable in 04.x fixtures but wired for forward-compat), and handing to `Listener::bind`. Differential harness gains `Driver::Http1` (HCM-aware; targets the listener, not admin) + `HttpMethod` enum + `BodyRule` enum + `HeaderRule` enum + `AllowMode` enum + `HEADER_ALLOW_LIST: &[(&str, AllowMode)]` constant sourced from BEHAVIOR_CONTRACT.md (`server`, `date` in 04.1) + `drive_http1` async helper (open `TcpStream`; write request; read until `httparse::Response::parse` completes + Content-Length body fully consumed; return `(status, headers, body)` triple) + `diff_headers` helper (case-insensitive name set-equality + value-exact match for non-allow-listed names) + 3 new harness unit tests + `run_fixture` dispatch on `Driver::Http1`. BEHAVIOR_CONTRACT.md's `Header allow-list` table is populated for the first time. Fixture `0007-http1-direct-response` exercises a `GET /healthz` request against an HCM listener with single-VH single-route catch-all `direct_response 200 inline_string "ok\n"`; both proxies emit identical 5-header response sets modulo the allow-listed `server` + `date` value-may-differ rule.

**Tech stack:** Rust edition 2024 on pinned stable `1.95.0` (D-3.9). New runtime deps in `envoy-http1` only: `httparse = "1"` (already a permitted foundation per envoy-bin's admin parser; no new ADR), `bytes = "1"` (already permitted per D-3.2), `tokio = { version = "1", features = ["net", "io-util", "macros", "sync", "time"] }` (the `time` feature is needed for the idle 5s read timeout per SPEC §6 signpost 12), `thiserror = "2"` (matches the version pinned by envoy-config / envoy-tls per the established library-crate posture), `tracing = "0.1"`. Path-deps on `envoy-config`, `envoy-cluster`, `envoy-listener`. `envoy-cluster` is forward-looking (04.3 wires it through the router proxy arm); 04.1 adds the dep at scaffold time so 04.3 doesn't re-touch the manifest. Dev-deps: `tokio` adds `rt-multi-thread` for tests. No new direct deps on the D-3.2 forbidden list. `tests/differential/` adds `httparse = "1"` as a dev-dep (for the response parser in `drive_http1`); no new ADR — `httparse` is already a permitted foundation. `envoy-bin/Cargo.toml` adds `envoy-http1 = { path = "../envoy-http1" }` runtime dep; no new dev-deps.

---

## File structure (created / modified)

**Created:**

- `crates/envoy-http1/Cargo.toml`
- `crates/envoy-http1/src/lib.rs` (with `#![forbid(unsafe_code)]`)
- `crates/envoy-http1/src/codec.rs`
- `crates/envoy-http1/src/headers.rs`
- `crates/envoy-http1/src/date.rs`
- `crates/envoy-http1/src/response.rs`
- `crates/envoy-http1/src/hcm.rs`
- `crates/envoy-http1/src/error.rs`
- `crates/envoy-bin/tests/http1_direct_response.rs` (Rust-native integration test — backstop, no Docker)
- `tests/differential/tests/http1_direct_response.rs` (Docker-gated acceptance test)
- `tests/fixtures/0007-http1-direct-response/envoy.yaml`
- `tests/fixtures/0007-http1-direct-response/envoy-rust.yaml`
- `tests/fixtures/0007-http1-direct-response/inputs/payload.bin` (empty file; placeholder for forward-compat with 04.3's body-bearing requests)
- `tests/fixtures/0007-http1-direct-response/expectations.yaml`
- `tests/fixtures/0007-http1-direct-response/README.md`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_direct_response_happy.yaml`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_invalid_codec_type.yaml`
- `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md` (appended once per task during execution)

**Modified:**

- Root `Cargo.toml` — add `crates/envoy-http1` to `[workspace] members`. (`tests/helpers/http1-echo-server` lands in 04.3.)
- `crates/envoy-config/src/bootstrap.rs` — add `HttpConnectionManagerConfig`, `CodecType`, `HttpFilter`, `HttpFilterTypedConfig`, `RouterConfig`, `RouteConfiguration`, `VirtualHost`, `Route`, `RouteMatch`, `DirectResponse`; add the `HttpConnectionManager` variant on `TypedConfig`; extend `DataSource` with `inline_string: Option<String>` and convert `filename` from `String` to `Option<String>`; extend `validate` with `UnsupportedCodecType`, `UnsupportedHttpFilter`, `UnsupportedRouteMatcher`, `UnsupportedDomainMatcher`, `EmptyVirtualHosts`, `EmptyRoutes`, `EmptyDomains`, `InvalidStatusCode`, `UnsupportedDataSource`, `MultipleHttpFilters` arms; append 8 new validator unit tests + 5 parse-shape tests under Task 1.
- `crates/envoy-config/src/lib.rs` — re-export the new public types (`HttpConnectionManagerConfig`, `CodecType`, `HttpFilter`, `HttpFilterTypedConfig`, `RouterConfig`, `RouteConfiguration`, `VirtualHost`, `Route`, `RouteMatch`, `DirectResponse`); extend `ConfigError` enum re-exports with the new variants.
- `crates/envoy-bin/Cargo.toml` — add `envoy-http1 = { path = "../envoy-http1" }` runtime dep.
- `crates/envoy-bin/src/main.rs` — in the per-listener filter-chain pre-pass, add a new `TypedConfig::HttpConnectionManager(hcm_cfg)` arm sibling of the existing `TypedConfig::TcpProxy(_)` arm; build `Arc<HCMConfig>` once via `HCMConfig::from_config(&hcm_cfg)?`; build `Arc::new(HCM { config }) as Arc<dyn ConnectionHandler>`; if filter chain has `transport_socket: Some(_)`, wrap in `TlsAcceptingHandler` per phase 03.1's existing wiring (unreachable in 04.x fixtures but wired for forward-compat); hand to `Listener::bind`.
- `tests/differential/Cargo.toml` — add `httparse = "1"` dev-dep.
- `tests/differential/src/lib.rs` — add `Driver::Http1` variant + `HttpMethod` enum + `BodyRule` enum + `HeaderRule` enum + `AllowMode` enum + `HEADER_ALLOW_LIST` constant + `drive_http1` async helper + `diff_headers` helper + `DriveHttp1Result` struct + `Driver::Http1` dispatch in `run_fixture`; 3 new harness unit tests.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — `Header allow-list` section: replace `_(empty; populated starting phase 04)_` with the 2-row table from SPEC §2 (`server`, `date`).
- `docs/envoy-rust/ROADMAP.md` — at state 6 only, flip row `04.1` `status` → `done`. (Row `04` parent stays `in-progress`; flips at 04.3's final commit per the schema.)
- `docs/envoy-rust/STATE.md` — at state 6 only, advance active phase id `04.2`, slug `04.2-route-matchers`, lifecycle state 2 (SPEC.md exists from the parent-04 state-2 split commit `1d9740d`, PLAN.md does not), next-skill `superpowers:writing-plans`.
- `Cargo.lock` — sync as a dedicated commit at the state-4 phase-done gate per the established phase-precedent (mirrors phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85685a3`). Expect entries for the new `envoy-http1 v0.0.0` crate plus `httparse` promoted from envoy-bin's transitive surface to a direct workspace runtime dep via envoy-http1's manifest; `bytes` already in tokio's transitive surface.
- `deny.toml` — only if `cargo deny check` flips on a new transitive surface from the bytes / httparse chain. Most likely a no-op.

**Not touched in 04.1** (belong to 04.2 / 04.3 / earlier phases or are frozen):

- `docs/envoy-rust/phases/04-http1/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `805433e`.
- `docs/envoy-rust/phases/04.2-route-matchers/SPEC.md`, `phases/04.3-router-upstream/SPEC.md` — landed alongside this SPEC at parent-04 state-2 commit `1d9740d`; their PLAN/PROGRESS/REVIEW lifecycles begin after 04.1 closes.
- `docs/envoy-rust/phases/03-tls-tcp/SPEC.md`, `phases/03.1-tls-foundation-downstream/`, `phases/03.2-tls-upstream-sni/` — closed in phase 03.
- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md`, `phases/02.1-config-cluster/`, `phases/02.2-listener-tcp-proxy/` — closed in phase 02.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/` — unedited; their fixtures must remain green at 04.1 state-4 gate.
- `crates/envoy-tls/`, `crates/envoy-tcp/`, `crates/envoy-listener/`, `crates/envoy-cluster/` — finalized in earlier phases; 04.1 consumes via existing public APIs (only `envoy-listener::ConnectionHandler` is touched, and only as a consumer — the trait's shape is unchanged).
- `tests/helpers/tcp-echo-server/`, `tests/helpers/tls-echo-server/` — finalized in phases 02.1 / 03.2; 04.1 fixture 0007 has no upstream backend.
- `tests/helpers/http1-echo-server/` — does not exist yet; lands in 04.3.
- `docs/envoy-rust/DECISIONS.md` — no edits in 04.1 (ADR-0020 landed at parent-04 state-2 = sibling commit `1d9740d`; ADR-0021 lands at 04.2 Task 1).
- `crates/envoy-bin/src/admin.rs` — admin endpoint's pre-existing `httparse` import is NOT refactored in 04.1; the per parent-SPEC §3 cross-sub-phase rule 1 posture takes effect when admin is next touched, not now.
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.
- `.github/workflows/ci.yml` — untouched (SPEC §D8: no CI workflow changes in 04.1).

---

## Task index

Each task ends with a commit. `PROGRESS.md` gets a new section per task in the phase-03.1 / phase-03.2 style (task id, commit SHA, change summary, verification tail, any deviation). Use either the `sed`-then-amend idiom or the follow-up `phase 04.1: progress note (task N)` commit convention — whichever is picked for Task 1 stays consistent through Task 17.

Ordering rationale (SPEC §6 signpost 1): `envoy-config` schema additions ship before `envoy-http1` because the HCM consumes the new `HttpConnectionManagerConfig` and `RouteConfiguration` types at construction time; the codec / headers / date / response modules ship before `hcm.rs` because `serve_connection` consumes all four; `envoy-bin` wiring ships before the harness extensions because the in-process integration test is a Rust-native backstop that does not require harness changes; the BEHAVIOR_CONTRACT.md edit ships *after* the harness `HEADER_ALLOW_LIST` constant lands so the constant and the contract land in lockstep (the constant is sourced from the contract — but the harness code change to introduce the constant predates the contract edit by one commit, mirroring the established discipline that code refers to the contract); fixture 0007 + Docker-gated test ship last because they exercise the full stack.

1. **`envoy-config` — HCM `TypedConfig` variant + `RouteConfiguration` schema + `DirectResponse` + `DataSource` extension + 5 parse-shape tests**
2. **`envoy-config` — validator extensions + 10 new `ConfigError` variants + 8 validator tests**
3. **`envoy-config` — 2 fuzz corpus seeds (HCM-shaped YAML)**
4. **Scaffold `crates/envoy-http1/` skeleton + workspace member**
5. **`envoy-http1::error` — `Http1Error` enum**
6. **`envoy-http1::headers` — `find_header` + canonical-name constants + 2 tests**
7. **`envoy-http1::date` — `format_imf_fixdate` + 2 tests**
8. **`envoy-http1::codec` — `Http1Codec`, `Request`, `HttpVersion` + 5 tests**
9. **`envoy-http1::response` — `Http1Response` writer + 2 tests**
10. **`envoy-http1::hcm` — `HCM`, `HCMConfig`, `serve_connection`, `build_response`, `synth_*` + 6 tests**
11. **`envoy-bin` — `envoy-http1` dep + `TypedConfig::HttpConnectionManager` dispatch arm in `main.rs`**
12. **`envoy-bin` integration test `tests/http1_direct_response.rs` — in-process subprocess + TCP client**
13. **Differential harness — `Driver::Http1` grammar + `HttpMethod` + `BodyRule` + `HeaderRule` + `AllowMode` + `HEADER_ALLOW_LIST` + `diff_headers` + 3 unit tests**
14. **Differential harness — `drive_http1` + `DriveHttp1Result` + `run_fixture` `Driver::Http1` dispatch**
15. **`docs/envoy-rust/BEHAVIOR_CONTRACT.md` — populate `Header allow-list` table (`server`, `date`)**
16. **Fixture `0007-http1-direct-response` (5 files) + Docker-gated `tests/differential/tests/http1_direct_response.rs`**
17. **State 4 phase-done gate — run all 5 stable commands + CI fuzz job; quote outputs into PROGRESS.md**

Estimated total: 17 tasks, ~1500 LoC. Both `BOOTSTRAP_PROMPT.md` §6.1 gates (~25 tasks / ~1500 LoC) hold at the upper edge. **Do not split 04.1 further.** Per parent-SPEC §5: nested splits of an already-split sub-phase are an anti-pattern. If the plan as actually written crosses either gate mid-execution, invoke `superpowers:systematic-debugging` first per `BOOTSTRAP_PROMPT.md` §6.1 + the parent-04 state-1 brainstorm's express avoidance of nested splits — root-cause whether the gate-crossing is scope creep (un-deferred work that should move to 04.2 or 04.3) or planner overdecomposition (each task too granular) before attempting any nested split.

---

### Task 1: `envoy-config` — HCM `TypedConfig` variant + `RouteConfiguration` schema + `DirectResponse` + `DataSource` extension + 5 parse-shape tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (add types + parse-shape tests)
- Modify: `crates/envoy-config/src/lib.rs` (re-export new public types)
- Create (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why first:** every subsequent task that touches HCM construction (Tasks 10, 11), the integration test (Task 12), the harness driver (Tasks 13–14), and the fixture (Task 16) consumes one or more of the types added here. `envoy-config` is also the closest existing crate to the changes, so this task scopes a single-crate edit with no cross-crate ripple. ADRs land separately — none in 04.1 (per SPEC §7); ADR-0020 already landed at parent-04 state-2 commit `1d9740d`.

**Scope.** Five new public structs + four new public enums on `bootstrap.rs` + one new `TypedConfig` variant + a `DataSource` field-shape change. Parse-shape testing only in this task (validator-level tests land in Task 2). No validator wiring yet — the new types parse via serde, but `validate` does not yet check them. Task 2 wires the validator.

**Pre-flight check.** Verify the current ADR head and confirm no new ADRs need to be cited in this task's commit message:

- [ ] **Step 1: Verify the ADR ledger head.**

```bash
grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md
grep -n '^## ADR-00' docs/envoy-rust/DECISIONS.md | tail -3
```

Expected: count `20`; last three are `ADR-0018`, `ADR-0019`, `ADR-0020`. ADR-0020 is the parent-04 split decision and is referenced in this task's commit message. If any unexpected `ADR-00NN` appears, debug per `superpowers:systematic-debugging` before continuing — phase 04.1 anticipates no inter-state ADR landings between commits `1d9740d` and Task 17.

- [ ] **Step 2: Read the current `bootstrap.rs` shape so the additions slot in cleanly.**

```bash
grep -n '^pub enum TypedConfig\|^pub struct DataSource\|^pub struct FilterChain\|^pub enum CodecType\|^pub struct RouteConfiguration\|^pub struct HttpConnectionManagerConfig' crates/envoy-config/src/bootstrap.rs
```

Expected (pre-Task-1): `TypedConfig` and `DataSource` exist (introduced phase 02.1 and phase 03.1 respectively); none of the other names exist. The new types append after the existing `TypedConfig::TcpProxy` variant and the existing `DataSource` struct.

- [ ] **Step 3: Write a failing parse-shape test for the HCM happy path.**

Append to the existing `#[cfg(test)] mod tests { ... }` block in `crates/envoy-config/src/bootstrap.rs`:

```rust
#[test]
fn parses_listener_with_hcm_direct_response() {
    let yaml = r#"
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 0.0.0.0, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
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
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
"#;
    let bs: Bootstrap = serde_yaml::from_str(yaml).expect("parses");
    let listener = &bs.static_resources.listeners[0];
    let filter = &listener.filter_chains[0].filters[0];
    let TypedConfig::HttpConnectionManager(hcm) = filter.typed_config.as_ref().unwrap() else {
        panic!("expected HCM variant");
    };
    assert_eq!(hcm.stat_prefix, "ingress_http");
    assert!(matches!(hcm.codec_type, CodecType::HTTP1));
    assert_eq!(hcm.route_config.virtual_hosts.len(), 1);
    let vh = &hcm.route_config.virtual_hosts[0];
    assert_eq!(vh.domains, vec!["*".to_string()]);
    let route = &vh.routes[0];
    assert_eq!(route.r#match.prefix.as_deref(), Some("/"));
    assert_eq!(route.direct_response.status, 200);
    assert_eq!(route.direct_response.body.inline_string.as_deref(), Some("ok\n"));
    assert_eq!(hcm.http_filters.len(), 1);
    assert_eq!(hcm.http_filters[0].name, "envoy.filters.http.router");
}
```

- [ ] **Step 4: Run the test to verify it fails.**

```bash
cargo test -p envoy-config parses_listener_with_hcm_direct_response
```

Expected: FAIL with a serde-deserialization error referencing the unknown `@type` URL or unknown variant on `TypedConfig`. The compile may also fail if `CodecType` and `TypedConfig::HttpConnectionManager` don't exist yet — that's expected.

- [ ] **Step 5: Add the new types and the `TypedConfig` variant.**

Append to `crates/envoy-config/src/bootstrap.rs` after the existing `TypedConfig` enum + the existing `DataSource` struct. Place the new types in the order: `HttpConnectionManagerConfig`, `CodecType`, `HttpFilter`, `HttpFilterTypedConfig`, `RouterConfig`, `RouteConfiguration`, `VirtualHost`, `Route`, `RouteMatch`, `DirectResponse`. The `TypedConfig::HttpConnectionManager` variant is added on the existing enum (sibling of `TcpProxy`).

Also extend the existing `DataSource` struct: change `filename: String` to `filename: Option<String>` (with `#[serde(default)]`); add `inline_string: Option<String>` (with `#[serde(default)]`). The `deny_unknown_fields` attribute stays.

```rust
// (Append AFTER the existing `pub enum TypedConfig { ... }` declaration, by
//  ADDING a new variant inside the existing enum block. The existing variant
//  is `TcpProxy(TcpProxyConfig)`; insert this AFTER it.)
//
//   #[serde(rename = "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager")]
//   HttpConnectionManager(HttpConnectionManagerConfig),

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectionManagerConfig {
    pub stat_prefix: String,
    pub codec_type: CodecType,
    pub route_config: RouteConfiguration,
    pub http_filters: Vec<HttpFilter>,
}

#[derive(Debug, Deserialize, PartialEq, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum CodecType {
    AUTO,
    HTTP1,
    HTTP2,
    HTTP3,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpFilter {
    pub name: String,
    pub typed_config: HttpFilterTypedConfig,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum HttpFilterTypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router")]
    Router(RouterConfig),
}

#[derive(Debug, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct RouterConfig {
    // Empty in 04.1; Envoy's Router has many fields (suppress_envoy_headers,
    // dynamic_stats, start_child_span, ...); all deferred per SPEC §4.
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteConfiguration {
    pub name: String,
    pub virtual_hosts: Vec<VirtualHost>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VirtualHost {
    pub name: String,
    pub domains: Vec<String>,
    pub routes: Vec<Route>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Route {
    #[serde(rename = "match")]
    pub r#match: RouteMatch,
    pub direct_response: DirectResponse,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteMatch {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DirectResponse {
    pub status: u16,
    pub body: DataSource,
}
```

For the `DataSource` extension — locate the existing `pub struct DataSource { ... }` (introduced in phase 03.1) and rewrite it as:

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DataSource {
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub inline_string: Option<String>,
}
```

This converts `filename` from a required `String` to an `Option<String>` with `#[serde(default)]`. Existing 03.1 callers (TLS cert/key/CA paths) currently access `ds.filename` directly as a `String`; those callers must update to `ds.filename.as_deref().expect("validator ensured present")` in the same edit. Search for all usages:

```bash
grep -rn '\.filename' crates/envoy-config/src/ crates/envoy-tls/src/
```

Expected callsites: 3–4 (TLS cert cert chain, TLS cert key, validation context CA path, possibly admin or a test). Update each to `.filename.as_deref().expect("...")` form. The validator-level "exactly one of {filename, inline_string} is Some" + per-callsite restriction is enforced in Task 2.

- [ ] **Step 6: Re-export the new public types from `crates/envoy-config/src/lib.rs`.**

Find the existing `pub use bootstrap::{...};` block (introduced phase 02.1, extended in phase 03.1) and append:

```rust
pub use bootstrap::{
    // ... existing re-exports ...
    HttpConnectionManagerConfig, CodecType, HttpFilter, HttpFilterTypedConfig, RouterConfig,
    RouteConfiguration, VirtualHost, Route, RouteMatch, DirectResponse,
};
```

Keep alphabetic / logical-cluster ordering consistent with the existing pattern.

- [ ] **Step 7: Run the happy-path test to verify it passes.**

```bash
cargo test -p envoy-config parses_listener_with_hcm_direct_response
```

Expected: PASS. If the test fails on a `DataSource.filename` borrow form, the existing 03.1 callers haven't been updated to `as_deref()` form yet; fix per Step 5's note.

- [ ] **Step 8: Add 4 more parse-shape tests (5 total in this task).**

Append to the same `#[cfg(test)] mod tests` block:

```rust
#[test]
fn parses_route_with_path_matcher() {
    let yaml = r#"
prefix: ~
path: "/exact"
"#;
    let m: RouteMatch = serde_yaml::from_str(yaml).expect("parses");
    assert!(m.prefix.is_none());
    assert_eq!(m.path.as_deref(), Some("/exact"));
}

#[test]
fn parses_data_source_with_inline_string() {
    let yaml = r#"
inline_string: "hello"
"#;
    let ds: DataSource = serde_yaml::from_str(yaml).expect("parses");
    assert!(ds.filename.is_none());
    assert_eq!(ds.inline_string.as_deref(), Some("hello"));
}

#[test]
fn parses_data_source_with_filename() {
    let yaml = r#"
filename: "/tmp/cert.pem"
"#;
    let ds: DataSource = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(ds.filename.as_deref(), Some("/tmp/cert.pem"));
    assert!(ds.inline_string.is_none());
}

#[test]
fn rejects_unknown_field_in_hcm_config() {
    let yaml = r#"
stat_prefix: ingress_http
codec_type: HTTP1
access_log: []
route_config:
  name: r
  virtual_hosts: []
http_filters: []
"#;
    let res: Result<HttpConnectionManagerConfig, _> = serde_yaml::from_str(yaml);
    assert!(res.is_err(), "deny_unknown_fields should reject access_log");
    let err = res.err().unwrap().to_string();
    assert!(err.contains("access_log") || err.contains("unknown field"),
            "error mentions unknown field: {}", err);
}

#[test]
fn rejects_unknown_field_in_route_match() {
    let yaml = r#"
prefix: "/"
case_sensitive: true
"#;
    let res: Result<RouteMatch, _> = serde_yaml::from_str(yaml);
    assert!(res.is_err(), "deny_unknown_fields should reject case_sensitive");
}
```

- [ ] **Step 9: Run all 5 parse-shape tests.**

```bash
cargo test -p envoy-config parses_listener_with_hcm_direct_response \
                          parses_route_with_path_matcher \
                          parses_data_source_with_inline_string \
                          parses_data_source_with_filename \
                          rejects_unknown_field_in_hcm_config \
                          rejects_unknown_field_in_route_match
```

Expected: 5 + 1 = 6 passes (the `parses_listener_with_hcm_direct_response` from Step 3 plus the 5 added in Step 8 — note: Step 8 adds 5, total of 6 if Step-3's test counts; the SPEC's "5 parse-shape tests" tally treats Step-3's test plus 4 of Step-8's as the count. The exact tally is: 1 happy-path + 4 narrow-shape = 5; the `rejects_unknown_field_in_route_match` is the +5 to the count. The plan-writer aimed at 5; if execution finds 6 reads cleaner — keep all 6 and adjust the tally in PROGRESS.md). The `rejects_unknown_field_in_hcm_config` is Task 1's representative regression-guard; Task 2 adds the validator-driven counterpart.

- [ ] **Step 10: Run the full crate test to verify no regression.**

```bash
cargo test -p envoy-config
```

Expected: previous count (e.g., ~50 from phase 03.1 / phase 03.2 work) plus the 5–6 new tests, all passing. Note any test count drift in PROGRESS.md.

- [ ] **Step 11: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all three exit 0. The `DataSource.filename` field-type change may have surfaced clippy warnings in `envoy-tls` (because the callsites changed from direct field access to `.as_deref().expect()`); fix in lockstep before committing.

- [ ] **Step 12: Create `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md` with a Task 1 section.**

```markdown
# Phase 04.1 Progress

## Task 1 — envoy-config schema additions (2026-04-27)

- Commit: <SHA>
- Change: added HttpConnectionManagerConfig + CodecType + HttpFilter + HttpFilterTypedConfig + RouterConfig + RouteConfiguration + VirtualHost + Route + RouteMatch + DirectResponse types and the TypedConfig::HttpConnectionManager variant; extended DataSource (filename → Option, inline_string field new); 5 parse-shape tests.
- Verification: `cargo test -p envoy-config` → all green; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.
- Tests added: parses_listener_with_hcm_direct_response, parses_route_with_path_matcher, parses_data_source_with_inline_string, parses_data_source_with_filename, rejects_unknown_field_in_hcm_config, rejects_unknown_field_in_route_match.
- Deviations: <none anticipated; document any if found>
```

Replace `<SHA>` with the commit hash from Step 13.

- [ ] **Step 13: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs crates/envoy-tls/src/
git status   # confirm only the intended files are staged
git commit -m "phase 04.1: envoy-config — HCM TypedConfig variant + RouteConfiguration schema + DirectResponse + DataSource extension (task 1)"
```

If the PROGRESS.md edit happens in the same commit, add it; otherwise follow the established phase-02.1+ cadence (separate `phase 04.1: progress note (task 1)` commit). Either pattern is acceptable; keep the choice consistent through Task 17.

---

### Task 2: `envoy-config` — validator extensions + 10 new `ConfigError` variants + 8 validator tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend `validate` + add 8 validator tests)
- Modify: `crates/envoy-config/src/lib.rs` (add 10 `ConfigError` variants)
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Task 1 added the parse-shape types but `validate` does not yet check them. Without validation, envoy-bin would consume malformed-but-deserializable HCM configs (HTTP2 codec_type, multi-prefix-and-path matchers, empty virtual_hosts, etc.). Task 2 closes that gap so Tasks 10–11 can rely on validator-already-rejected guarantees.

**Scope.** Extend `validate` to walk every new type from Task 1 and reject malformed shapes. 10 new `ConfigError` variants per SPEC §3 D2; 8 new unit tests in `bootstrap.rs::tests`. This task does **not** add fuzz seeds (Task 3) or wire envoy-bin (Task 11).

- [ ] **Step 1: Extend `ConfigError` in `crates/envoy-config/src/lib.rs`.**

Find the existing `pub enum ConfigError { ... }` block. Append 10 new variants after the existing variants:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    // ... existing variants from phases 01–03 ...

    #[error("unsupported codec_type: {got:?}; only AUTO and HTTP1 are supported in phase 04")]
    UnsupportedCodecType { got: bootstrap::CodecType },

    #[error("unsupported HTTP filter: {name}; only envoy.filters.http.router is supported in phase 04.x")]
    UnsupportedHttpFilter { name: String },

    #[error("unsupported route matcher: {matcher}; exactly one of `prefix` or `path` must be set")]
    UnsupportedRouteMatcher { matcher: &'static str },

    #[error("unsupported virtual_host domain: {domain}; only \"*\" or syntactically-valid DNS names are supported in phase 04")]
    UnsupportedDomainMatcher { domain: String },

    #[error("RouteConfiguration `{route_config}` has no virtual_hosts")]
    EmptyVirtualHosts { route_config: String },

    #[error("VirtualHost `{virtual_host}` has no routes")]
    EmptyRoutes { virtual_host: String },

    #[error("VirtualHost `{virtual_host}` has no domains")]
    EmptyDomains { virtual_host: String },

    #[error("invalid status code: {status}; must be in 100..=599")]
    InvalidStatusCode { status: u16 },

    #[error("unsupported DataSource at field `{field}`: requires `{requires}`")]
    UnsupportedDataSource { field: &'static str, requires: &'static str },

    #[error("unsupported HTTP filter count: {count}; phase 04.x's HCM accepts exactly one filter (the router)")]
    MultipleHttpFilters { count: usize },
}
```

Note: `bootstrap::CodecType` must derive `Debug` (already done in Task 1). If thiserror complains about the `{got:?}` interpolation on a non-`Display` type, change the format to `{got:?}` (already shown above) — `Debug` is enough.

- [ ] **Step 2: Write 8 failing validator tests.**

Append to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
fn parse_then_validate(yaml: &str) -> Result<Bootstrap, ConfigError> {
    let bs: Bootstrap = serde_yaml::from_str(yaml)
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;
    validate(&bs)?;
    Ok(bs)
}
// (note: if `parse_then_validate` already exists in tests, reuse it — phase
//  02.1 + phase 03.1 have similar helpers; do not duplicate.)

fn make_hcm_listener_yaml(hcm_block: &str) -> String {
    format!(r#"
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: {{ address: 0.0.0.0, port_value: 8080 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
{}
  clusters: []
admin:
  address:
    socket_address: {{ address: 0.0.0.0, port_value: 0 }}
"#, hcm_block)
}

const VALID_ROUTER_FILTER: &str = r#"
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#;

#[test]
fn rejects_codec_type_http2() {
    let hcm = format!(r#"
                stat_prefix: x
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok" }}
{}"#, VALID_ROUTER_FILTER);
    let yaml = make_hcm_listener_yaml(&hcm);
    let err = parse_then_validate(&yaml).expect_err("should reject HTTP2");
    assert!(matches!(err, ConfigError::UnsupportedCodecType { got: CodecType::HTTP2 }),
            "got: {:?}", err);
}

#[test]
fn rejects_codec_type_http3() {
    let hcm = format!(r#"
                stat_prefix: x
                codec_type: HTTP3
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok" }}
{}"#, VALID_ROUTER_FILTER);
    let yaml = make_hcm_listener_yaml(&hcm);
    let err = parse_then_validate(&yaml).expect_err("should reject HTTP3");
    assert!(matches!(err, ConfigError::UnsupportedCodecType { got: CodecType::HTTP3 }),
            "got: {:?}", err);
}

#[test]
fn rejects_unsupported_http_filter() {
    let hcm = r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.lua
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#;
    // The @type IS the router (the only schema arm), but `name` is "lua" — validator rejects.
    let yaml = make_hcm_listener_yaml(hcm);
    let err = parse_then_validate(&yaml).expect_err("should reject non-router name");
    assert!(matches!(err, ConfigError::UnsupportedHttpFilter { .. }),
            "got: {:?}", err);
}

#[test]
fn rejects_route_match_with_both_prefix_and_path() {
    let hcm = format!(r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/x", path: "/y" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok" }}
{}"#, VALID_ROUTER_FILTER);
    let yaml = make_hcm_listener_yaml(&hcm);
    let err = parse_then_validate(&yaml).expect_err("should reject both prefix and path");
    assert!(matches!(err, ConfigError::UnsupportedRouteMatcher { .. }),
            "got: {:?}", err);
}

#[test]
fn rejects_route_match_with_neither_prefix_nor_path() {
    let hcm = format!(r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{}}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok" }}
{}"#, VALID_ROUTER_FILTER);
    let yaml = make_hcm_listener_yaml(&hcm);
    let err = parse_then_validate(&yaml).expect_err("should reject empty match");
    assert!(matches!(err, ConfigError::UnsupportedRouteMatcher { .. }),
            "got: {:?}", err);
}

#[test]
fn rejects_direct_response_with_filename_body() {
    let hcm = format!(r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ filename: "/tmp/x" }}
{}"#, VALID_ROUTER_FILTER);
    let yaml = make_hcm_listener_yaml(&hcm);
    let err = parse_then_validate(&yaml).expect_err("should reject filename in direct_response");
    assert!(matches!(err, ConfigError::UnsupportedDataSource { field: "direct_response.body", requires: "inline_string" }),
            "got: {:?}", err);
}

#[test]
fn rejects_direct_response_with_invalid_status() {
    let hcm = format!(r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 99
                            body: {{ inline_string: "ok" }}
{}"#, VALID_ROUTER_FILTER);
    let yaml = make_hcm_listener_yaml(&hcm);
    let err = parse_then_validate(&yaml).expect_err("should reject status < 100");
    assert!(matches!(err, ConfigError::InvalidStatusCode { status: 99 }),
            "got: {:?}", err);
}

#[test]
fn rejects_empty_virtual_hosts() {
    let hcm = format!(r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts: []
{}"#, VALID_ROUTER_FILTER);
    let yaml = make_hcm_listener_yaml(&hcm);
    let err = parse_then_validate(&yaml).expect_err("should reject empty virtual_hosts");
    assert!(matches!(err, ConfigError::EmptyVirtualHosts { .. }),
            "got: {:?}", err);
}
```

- [ ] **Step 3: Run all 8 tests to verify they fail.**

```bash
cargo test -p envoy-config rejects_codec_type_http2 rejects_codec_type_http3 \
                          rejects_unsupported_http_filter \
                          rejects_route_match_with_both_prefix_and_path \
                          rejects_route_match_with_neither_prefix_nor_path \
                          rejects_direct_response_with_filename_body \
                          rejects_direct_response_with_invalid_status \
                          rejects_empty_virtual_hosts
```

Expected: all 8 FAIL — either the test panics on `expect_err` (because `validate` returns `Ok` for the malformed-but-parseable inputs) or the test panics on `assert!(matches!(...))` (because `validate` returns the wrong error variant).

- [ ] **Step 4: Extend `validate` in `bootstrap.rs` to reject the 10 new error cases.**

Find the existing `pub fn validate(bs: &Bootstrap) -> Result<(), ConfigError>` (introduced phase 02.1; extended phase 03.1). Add a per-listener / per-filter walk that handles the new HCM `TypedConfig` variant. The validator's structure mirrors the existing `TypedConfig::TcpProxy` handling — extract the HCM block and walk its sub-shape.

Pseudocode (the actual implementation slots into the existing per-listener loop):

```rust
pub fn validate(bs: &Bootstrap) -> Result<(), ConfigError> {
    // ... existing checks (listeners cap, admin presence, clusters, etc.) ...

    for listener in &bs.static_resources.listeners {
        for chain in &listener.filter_chains {
            for filter in &chain.filters {
                match filter.typed_config.as_ref() {
                    Some(TypedConfig::TcpProxy(tp)) => {
                        // ... existing TcpProxy validation from phase 02.1 ...
                    }
                    Some(TypedConfig::HttpConnectionManager(hcm)) => {
                        validate_hcm(hcm)?;
                    }
                    None => { /* existing behavior */ }
                }
            }
        }
    }

    // ... existing post-listener checks ...

    Ok(())
}

fn validate_hcm(hcm: &HttpConnectionManagerConfig) -> Result<(), ConfigError> {
    // codec_type
    match hcm.codec_type {
        CodecType::AUTO | CodecType::HTTP1 => {}
        CodecType::HTTP2 | CodecType::HTTP3 => {
            return Err(ConfigError::UnsupportedCodecType { got: hcm.codec_type });
        }
    }

    // http_filters cardinality + name
    match hcm.http_filters.len() {
        1 => {
            let f = &hcm.http_filters[0];
            if f.name != "envoy.filters.http.router" {
                return Err(ConfigError::UnsupportedHttpFilter {
                    name: f.name.clone(),
                });
            }
            // typed_config @type already constrained to Router by the schema.
        }
        n => return Err(ConfigError::MultipleHttpFilters { count: n }),
    }

    // route_config
    if hcm.route_config.virtual_hosts.is_empty() {
        return Err(ConfigError::EmptyVirtualHosts {
            route_config: hcm.route_config.name.clone(),
        });
    }
    for vh in &hcm.route_config.virtual_hosts {
        if vh.domains.is_empty() {
            return Err(ConfigError::EmptyDomains {
                virtual_host: vh.name.clone(),
            });
        }
        for d in &vh.domains {
            if d != "*" && !is_valid_dns_name(d) {
                return Err(ConfigError::UnsupportedDomainMatcher {
                    domain: d.clone(),
                });
            }
        }
        if vh.routes.is_empty() {
            return Err(ConfigError::EmptyRoutes {
                virtual_host: vh.name.clone(),
            });
        }
        for r in &vh.routes {
            // RouteMatch: exactly one of prefix or path is Some.
            match (&r.r#match.prefix, &r.r#match.path) {
                (Some(_), None) | (None, Some(_)) => {}
                (Some(_), Some(_)) => {
                    return Err(ConfigError::UnsupportedRouteMatcher {
                        matcher: "both prefix and path are set",
                    });
                }
                (None, None) => {
                    return Err(ConfigError::UnsupportedRouteMatcher {
                        matcher: "neither prefix nor path is set",
                    });
                }
            }
            // direct_response.status range.
            if !(100..=599).contains(&r.direct_response.status) {
                return Err(ConfigError::InvalidStatusCode {
                    status: r.direct_response.status,
                });
            }
            // direct_response.body must be inline_string.
            validate_data_source(
                &r.direct_response.body,
                "direct_response.body",
                "inline_string",
            )?;
        }
    }
    Ok(())
}

fn validate_data_source(
    ds: &DataSource,
    field: &'static str,
    requires: &'static str,
) -> Result<(), ConfigError> {
    // Cardinality: exactly one of {filename, inline_string} is Some.
    let has_file = ds.filename.is_some();
    let has_inline = ds.inline_string.is_some();
    if has_file == has_inline {
        // both Some or both None
        return Err(ConfigError::UnsupportedDataSource { field, requires });
    }
    // Per-callsite restriction.
    match requires {
        "inline_string" => {
            if !has_inline {
                return Err(ConfigError::UnsupportedDataSource { field, requires });
            }
        }
        "filename" => {
            if !has_file {
                return Err(ConfigError::UnsupportedDataSource { field, requires });
            }
        }
        _ => unreachable!("unknown requires marker: {}", requires),
    }
    Ok(())
}

/// Returns true if `name` is a syntactically valid DNS name per RFC 1123 LDH
/// rule. Wildcard prefixes (`*.example.com`) return false in 04.1.
fn is_valid_dns_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 253 { return false; }
    if name.starts_with('*') { return false; } // wildcard prefix deferred
    name.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}
```

Also extend the existing 03.1 callsites that consume `DataSource.filename` to call `validate_data_source(ds, "...", "filename")` as part of their own validators. Search:

```bash
grep -n 'fn validate_tls_certificate\|fn validate_validation_context\|certificate_chain.*filename\|trusted_ca.*filename' crates/envoy-config/src/bootstrap.rs
```

Update each TLS validator path to call `validate_data_source(...)` rather than ad-hoc presence checks. The "exactly one of {filename, inline_string} is Some" cardinality and per-callsite restriction live behind one helper.

- [ ] **Step 5: Run all 8 new tests to verify they pass.**

```bash
cargo test -p envoy-config rejects_codec_type_http2 rejects_codec_type_http3 \
                          rejects_unsupported_http_filter \
                          rejects_route_match_with_both_prefix_and_path \
                          rejects_route_match_with_neither_prefix_nor_path \
                          rejects_direct_response_with_filename_body \
                          rejects_direct_response_with_invalid_status \
                          rejects_empty_virtual_hosts
```

Expected: all 8 PASS.

- [ ] **Step 6: Run the full crate test to verify no regression.**

```bash
cargo test -p envoy-config
```

Expected: all green. Existing TLS validator tests should still pass — if any TLS-side test fails on the `validate_data_source` refactor, fix in lockstep. The phase-03 tests assume `DataSource.filename` is required; the refactor makes it `Option`. Tests that hand-construct a `DataSource` value need updating (likely 0–4 tests).

- [ ] **Step 7: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all three exit 0.

- [ ] **Step 8: Append a Task 2 section to PROGRESS.md.**

```markdown
## Task 2 — envoy-config validator extensions (2026-04-27)

- Commit: <SHA>
- Change: extended `validate` with `validate_hcm` + `validate_data_source` + `is_valid_dns_name`; added 10 ConfigError variants; refactored phase-03 TLS validator paths to consume `validate_data_source`; 8 new validator tests.
- Verification: `cargo test -p envoy-config` → all green; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.
- Tests added: rejects_codec_type_http2, rejects_codec_type_http3, rejects_unsupported_http_filter, rejects_route_match_with_both_prefix_and_path, rejects_route_match_with_neither_prefix_nor_path, rejects_direct_response_with_filename_body, rejects_direct_response_with_invalid_status, rejects_empty_virtual_hosts.
- Deviations: <document any if found — e.g., test count drift if a TLS-side test had to be updated>.
```

- [ ] **Step 9: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 04.1: envoy-config — HCM validator extensions + 10 ConfigError variants + 8 validator tests (task 2)"
```

---

### Task 3: `envoy-config` — 2 fuzz corpus seeds (HCM-shaped YAML)

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_direct_response_happy.yaml`
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_invalid_codec_type.yaml`
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Tasks 1–2 established the HCM schema + validator. The fuzz corpus extension is independent of any code change and ships before the envoy-http1 work to keep the envoy-config phase-of-work coherent. The corpus seeds the existing `parse_bootstrap` fuzz target — no new target ships.

**Scope.** Two new YAML files under the existing fuzz corpus. The harness's `parse_bootstrap` target picks them up automatically. The CI fuzz job's `-max_total_time=30` budget per ADR-0010 is unchanged.

- [ ] **Step 1: Write `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_direct_response_happy.yaml`.**

```yaml
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 0.0.0.0, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
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
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
```

- [ ] **Step 2: Write `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_invalid_codec_type.yaml`.**

Same shape but with `codec_type: HTTP2`:

```yaml
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 0.0.0.0, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP2
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
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
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
```

- [ ] **Step 3: Verify both files parse cleanly through `parse_bootstrap` (one accepts, one rejects with `UnsupportedCodecType`).**

The fuzz target's contract is "must not panic" on any input. Both seeds satisfy this — the happy seed is parse+validate green; the invalid-codec seed is parse green / validate red, which `parse_bootstrap` treats as a clean rejection (not a panic).

To smoke-test outside of the fuzz harness:

```bash
cargo run -p envoy-config --example check_bootstrap -- crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_direct_response_happy.yaml 2>&1 | head -5
```

If a `check_bootstrap` example doesn't exist (it doesn't pre-Task-3 — phase-01 didn't add one), skip this step; the corpus files are static YAML, validated only by the existing fuzz target's roundtrip in CI. Alternative smoke-test:

```bash
# Quick parse+validate check via a one-off Rust scratch test:
cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly 2>&1 | tail -20
```

If a `fuzz_corpus_seeds_parse_or_reject_cleanly` test exists from phase 02.1+, it walks the corpus dir and asserts each file either parses+validates Ok or fails with a clean `ConfigError`. Verify both new seeds match the existing test's expectations. If the test's expectations enumerate seeds explicitly (e.g., a hand-listed allow-list), extend it to include the two new seeds.

- [ ] **Step 4: Smoke-test the fuzz target on the extended corpus (optional, locally).**

If the local box has the nightly toolchain + cargo-fuzz installed:

```bash
cd crates/envoy-config/fuzz
cargo +nightly fuzz run parse_bootstrap -- -max_total_time=10 -runs=100
```

Expected: 0 crashes, 0 leaks. The 10s budget is ample for the small corpus. CI's 30s budget runs the same target.

If the local box lacks the nightly toolchain, skip — CI will run the extended corpus on the next push.

- [ ] **Step 5: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo test -p envoy-config
```

Expected: green. The new seeds don't change any code; tests pass.

- [ ] **Step 6: Append a Task 3 section to PROGRESS.md.**

```markdown
## Task 3 — fuzz corpus extension (2026-04-27)

- Commit: <SHA>
- Change: added 2 HCM-shaped seeds to `crates/envoy-config/fuzz/corpus/parse_bootstrap/`: `hcm_direct_response_happy.yaml` (parse+validate Ok), `hcm_invalid_codec_type.yaml` (parse Ok / validate UnsupportedCodecType). The existing `parse_bootstrap` target picks them up automatically; `-max_total_time=30` budget unchanged per ADR-0010.
- Verification: corpus walk test (if present) green; local fuzz smoke-run (if applicable) clean.
- Deviations: <document any>.
```

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_direct_response_happy.yaml \
        crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_invalid_codec_type.yaml
git commit -m "phase 04.1: envoy-config — fuzz corpus extension (task 3)"
```

---

### Task 4: Scaffold `crates/envoy-http1/` skeleton + workspace member

**Files:**
- Create: `crates/envoy-http1/Cargo.toml`
- Create: `crates/envoy-http1/src/lib.rs`
- Modify: root `Cargo.toml` (add `crates/envoy-http1` to `[workspace] members`)
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Tasks 5–10 fill in the crate's modules. The crate must exist + compile (empty) + be in the workspace before any module can land. Mirrors phase-03.1 Task 5's "Scaffold envoy-tls skeleton" cadence.

**Scope.** Empty crate + module-stub declarations + workspace membership. No public surface yet (each module ships in its own subsequent task). Cargo.toml dep set is final at this point — Task 5 onward consumes it without re-touching.

- [ ] **Step 1: Create `crates/envoy-http1/Cargo.toml`.**

```toml
[package]
name = "envoy-http1"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[dependencies]
envoy-config = { path = "../envoy-config" }
envoy-cluster = { path = "../envoy-cluster" }
envoy-listener = { path = "../envoy-listener" }
httparse = "1"
bytes = "1"
tokio = { version = "1", features = ["net", "io-util", "macros", "sync", "time"] }
thiserror = "2"
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "test-util"] }
```

The `envoy-cluster` runtime dep is forward-looking for 04.3 (router proxy arm). 04.1 does not call into envoy-cluster at runtime; the dep is added at scaffold time so 04.3 doesn't need to re-touch the manifest. (SPEC §3 D1 Note: this can be deferred to 04.3 if a clean-scaffold posture is preferred — but adding it now is the SPEC's first-listed posture, so we do that.)

The `tracing` dep is forward-looking too — 04.1's HCM uses `tracing::warn!` only on the 400/404/501 error paths; if execution finds no `tracing` callsite, the dep stays present anyway since downstream crates (04.2, 04.3) will need it.

- [ ] **Step 2: Create `crates/envoy-http1/src/lib.rs`.**

```rust
#![forbid(unsafe_code)]
//! HTTP/1.1 codec + connection manager (HCM) for envoy-rust.
//!
//! This crate is the workspace's sole runtime owner of the `httparse`
//! dependency (per phase-04 parent SPEC §3 cross-sub-phase rule 1). All
//! HTTP/1.1 request parsing in runtime code goes through `Http1Codec`;
//! response wire-format generation goes through `Http1Response`.
//!
//! envoy-bin's admin endpoint historically imported `httparse` directly
//! (introduced in phase 01). The architectural posture from 04.1 onwards
//! is that admin code routes through this crate's public types when admin
//! is next touched; 04.1 does not perform an in-flight refactor of admin.

pub mod codec;
pub mod headers;
pub mod date;
pub mod response;
pub mod hcm;
mod error;

pub use error::Http1Error;
```

The `pub use codec::{...}`, `pub use response::{...}`, `pub use hcm::{...}` re-exports land in their respective tasks (Tasks 8–10) when those modules' types exist. Each module file is created empty (or with stub types) in Task 4 so the crate compiles after this task — see Step 3.

- [ ] **Step 3: Create empty module files so the crate compiles.**

Each file contains only a top-of-file doc comment so `mod foo;` declarations resolve. The actual content lands in Tasks 5–10.

```rust
// crates/envoy-http1/src/codec.rs
//! HTTP/1.1 request codec (a thin wrapper over `httparse::Request::parse`).
//! Populated in Task 8.
```

```rust
// crates/envoy-http1/src/headers.rs
//! Case-insensitive header name lookup + canonical-form name constants.
//! Populated in Task 6.
```

```rust
// crates/envoy-http1/src/date.rs
//! Hand-rolled IMF-fixdate writer (RFC 7231 §7.1.1.1).
//! Populated in Task 7.
```

```rust
// crates/envoy-http1/src/response.rs
//! Wire-format HTTP/1.1 response writer.
//! Populated in Task 9.
```

```rust
// crates/envoy-http1/src/hcm.rs
//! HTTP connection manager: per-listener config, per-connection state machine,
//! route walker, hardcoded router-filter call site.
//! Populated in Task 10.
```

```rust
// crates/envoy-http1/src/error.rs
//! Error type for envoy-http1.
//! Populated in Task 5.
```

- [ ] **Step 4: Add `crates/envoy-http1` to root `Cargo.toml` `[workspace] members`.**

Find the existing `members = [...]` array. Insert `"crates/envoy-http1",` in alphabetical position after `"crates/envoy-config"` and before `"crates/envoy-listener"`.

- [ ] **Step 5: Run `cargo build` to verify the empty crate compiles.**

```bash
cargo build -p envoy-http1
```

Expected: clean build with `Compiling envoy-http1 v0.0.0`. Warnings are acceptable (the empty modules trigger unused-import or dead-code warnings depending on the empty-module shape; the next tasks resolve them).

- [ ] **Step 6: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: build clean. Clippy may flag the empty modules' lack of contents; if it does, add `#[allow(dead_code)]` at the module level until the module is populated, OR add a single dummy re-export from `lib.rs` that wires through the empty module (e.g., `pub use codec::Placeholder;` if a `pub struct Placeholder;` is added). The cleanest workaround is the SPEC §3 D1 module decomposition: `mod error;` is private to the crate, and the rest re-export only after their content lands. So Tasks 5–10 can each ship `pub use {...}` from the lib.rs re-export block as their sub-task — the `lib.rs` file authored in Step 2 already lists all `pub mod foo;` declarations (which compile fine for empty modules), and the `pub use` lines are added per-task as their types land.

- [ ] **Step 7: Append a Task 4 section to PROGRESS.md.**

```markdown
## Task 4 — envoy-http1 scaffold (2026-04-27)

- Commit: <SHA>
- Change: scaffolded `crates/envoy-http1/` (Cargo.toml + lib.rs with `#![forbid(unsafe_code)]` + 6 empty module files); added to workspace members.
- Verification: `cargo build -p envoy-http1` → clean; `cargo build --workspace --all-targets` → clean.
- Deviations: <document any — most likely: tracing dep dropped if no callsite, OR envoy-cluster dep deferred to 04.3 per SPEC §3 D1's plan-writer-discretion note>.
```

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-http1/ Cargo.toml
git commit -m "phase 04.1: scaffold envoy-http1 crate (task 4)"
```

---

### Task 5: `envoy-http1::error` — `Http1Error` enum

**Files:**
- Modify: `crates/envoy-http1/src/error.rs` (replace stub with `Http1Error` enum)
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Tasks 6–10 all return `Result<_, Http1Error>`. Landing the error type first means subsequent tasks can write the failing test + impl in one shot rather than landing a placeholder error type and re-touching it. Mirrors phase-03.1's discipline of landing `TlsError` early.

**Scope.** Six error variants per SPEC §3 D1 "Error shape." 04.3 will extend additively with `UpstreamConnect`, `UpstreamHandshake`, `MalformedResponseLine`, `MalformedChunkedFraming`. No tests needed in this task — the variants are exercised by Tasks 6–10's tests.

- [ ] **Step 1: Replace `crates/envoy-http1/src/error.rs` with the `Http1Error` enum.**

```rust
//! Error type for envoy-http1.

#[derive(Debug, thiserror::Error)]
pub enum Http1Error {
    #[error("malformed request line")]
    MalformedRequestLine,

    #[error("malformed header (bad token, missing colon, etc.)")]
    MalformedHeader,

    #[error("request headers exceed cap of {cap} bytes")]
    HeadersTooLarge { cap: usize },

    #[error("request body exceeds cap of {cap} bytes")]
    BodyTooLarge { cap: usize },

    #[error("unexpected EOF mid-message")]
    UnexpectedEof,

    #[error("io: {source}")]
    Io {
        #[source]
        source: std::io::Error,
    },
}

impl From<std::io::Error> for Http1Error {
    fn from(source: std::io::Error) -> Self {
        Self::Io { source }
    }
}
```

The `From<std::io::Error>` conversion is conservative — keeps `?` ergonomic in tasks 8–10 without forcing every callsite to wrap manually. If clippy flags it as redundant (because thiserror's `#[from]` would do the same), use `#[from]` on the `source` field instead and drop the manual impl.

- [ ] **Step 2: Run `cargo build -p envoy-http1`.**

```bash
cargo build -p envoy-http1
```

Expected: clean build. The dead-code warning on the unused enum variants is acceptable until Task 6+ consume them; suppress with `#[allow(dead_code)]` on the enum if necessary, or accept the warning until Task 10's HCM uses every variant transitively.

- [ ] **Step 3: Run clippy.**

```bash
cargo clippy -p envoy-http1 --all-targets -- -D warnings
```

Expected: clean. If `unused_imports` or `dead_code` fires from the empty modules, that's the Task-4 carryforward (resolved as those modules land in Tasks 6–10).

- [ ] **Step 4: Append a Task 5 section to PROGRESS.md.**

```markdown
## Task 5 — envoy-http1::error (2026-04-27)

- Commit: <SHA>
- Change: replaced error.rs stub with Http1Error enum (6 variants: MalformedRequestLine, MalformedHeader, HeadersTooLarge, BodyTooLarge, UnexpectedEof, Io); added From<io::Error> conversion.
- Verification: `cargo build -p envoy-http1` → clean.
- Deviations: <document any — e.g., variant set adjusted per execution-time discovery>.
```

- [ ] **Step 5: Commit.**

```bash
git add crates/envoy-http1/src/error.rs
git commit -m "phase 04.1: envoy-http1 — Http1Error enum (task 5)"
```

---

### Task 6: `envoy-http1::headers` — `find_header` + canonical-name constants + 2 tests

**Files:**
- Modify: `crates/envoy-http1/src/headers.rs` (replace stub with constants + helper + 2 tests)
- Modify: `crates/envoy-http1/src/lib.rs` (no public re-export of `headers` — keep module-public access only since downstream code uses `envoy_http1::headers::find_header` form)
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Tasks 8 (codec), 9 (response), 10 (hcm), 12 (envoy-bin integration test), and 14 (drive_http1) all do case-insensitive header lookup. Land the helper + canonical-name constants once and reuse.

**Scope.** Six string constants (canonical lowercase form) + one helper function + 2 tests. Per SPEC §3 D1 "Headers — case-insensitive lookup, case-preserving storage."

- [ ] **Step 1: Write 2 failing tests.**

Create `crates/envoy-http1/src/headers.rs` with:

```rust
//! Case-insensitive header name lookup + canonical-form name constants.

pub const HOST: &str = "host";
pub const CONTENT_LENGTH: &str = "content-length";
pub const CONNECTION: &str = "connection";
pub const SERVER: &str = "server";
pub const DATE: &str = "date";
pub const CONTENT_TYPE: &str = "content-type";

/// Find a header by name using case-insensitive comparison per HTTP/1.1 §3.2.
/// Returns the value of the first matching header, or `None`.
pub fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_header_is_case_insensitive() {
        let headers = vec![("Host".to_string(), "x".to_string())];
        assert_eq!(find_header(&headers, "host"), Some("x"));
        assert_eq!(find_header(&headers, "HOST"), Some("x"));
        assert_eq!(find_header(&headers, "HoSt"), Some("x"));
    }

    #[test]
    fn find_header_returns_none_on_missing() {
        let headers: Vec<(String, String)> = vec![];
        assert_eq!(find_header(&headers, "host"), None);

        let headers = vec![("X-Foo".to_string(), "1".to_string())];
        assert_eq!(find_header(&headers, "host"), None);
    }
}
```

Note: this is a write-then-test (the test + impl land together). For TDD discipline per D-3.1, run the impl-less version first (commit just the tests with stub `pub fn find_header(...) -> Option<&str> { unimplemented!() }`), watch them fail, then add the impl. Either flow is acceptable — the implementer chooses.

- [ ] **Step 2: Run the 2 tests to verify they pass after the impl.**

```bash
cargo test -p envoy-http1 find_header_is_case_insensitive find_header_returns_none_on_missing
```

Expected: 2 PASS.

- [ ] **Step 3: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean. The `dead_code` warnings on `HOST`, `CONTENT_LENGTH`, etc. are acceptable until Task 10 consumes them; if clippy fires, suppress at the module level with `#[allow(dead_code)]` until Task 10. (The constants are deliberately published as `pub const` so external crates can reference them — they're not actually `dead_code`, but clippy sometimes flags unused workspace-internal `pub const`s. If so, leave the warning for Task 10 to resolve.)

- [ ] **Step 4: Append a Task 6 section to PROGRESS.md.**

```markdown
## Task 6 — envoy-http1::headers (2026-04-27)

- Commit: <SHA>
- Change: populated headers.rs with 6 canonical-name constants (HOST, CONTENT_LENGTH, CONNECTION, SERVER, DATE, CONTENT_TYPE) + find_header case-insensitive lookup helper + 2 unit tests.
- Verification: `cargo test -p envoy-http1` → +2 tests; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.
- Deviations: <document any>.
```

- [ ] **Step 5: Commit.**

```bash
git add crates/envoy-http1/src/headers.rs
git commit -m "phase 04.1: envoy-http1 — headers module (find_header + canonical names) (task 6)"
```

---

### Task 7: `envoy-http1::date` — `format_imf_fixdate` + 2 tests

**Files:**
- Modify: `crates/envoy-http1/src/date.rs` (replace stub with implementation + 2 tests)
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Task 10's HCM emits `date: <fmt>` on every response. Land the formatter before HCM consumes it.

**Scope.** Hand-rolled ~30 LoC IMF-fixdate writer per SPEC §3 D1 "date — hand-rolled IMF-fixdate writer." NO new ADR — the SPEC explicitly locks the hand-rolled approach (declines to pre-emptively land an `httpdate` ADR; if execution surfaces a concrete blocker, the implementer escalates per D-3.5).

The IMF-fixdate format (RFC 7231 §7.1.1.1): `Sun, 06 Nov 1994 08:49:37 GMT`. Format breakdown:

```
<day-name>, <day> <month> <year> <hour>:<minute>:<second> GMT
```

Day name: `Sun`/`Mon`/`Tue`/`Wed`/`Thu`/`Fri`/`Sat` (3-letter abbreviation; weekday from civil date via Howard Hinnant's algorithm).

Month: `Jan`/`Feb`/.../`Dec` (3-letter abbreviation).

Year: 4 digits.

Hour/minute/second: 2 digits each, zero-padded.

- [ ] **Step 1: Write 2 failing tests.**

Replace `crates/envoy-http1/src/date.rs` with:

```rust
//! Hand-rolled IMF-fixdate writer (RFC 7231 §7.1.1.1).
//!
//! No external crate dep — `httpdate` would be the obvious off-the-shelf
//! choice but the parent-04 SPEC §3 D1 locks the hand-rolled approach so
//! 04.1 doesn't pre-emptively land an `httpdate` ADR. ~30 LoC.

use std::time::{SystemTime, UNIX_EPOCH};

/// Format a `SystemTime` as an IMF-fixdate string per RFC 7231 §7.1.1.1.
///
/// Returns the canonical form: `Sun, 06 Nov 1994 08:49:37 GMT`.
///
/// Times before the Unix epoch return the epoch itself (defensive — clock
/// skew at boot can produce pre-epoch times briefly; the HCM never emits
/// such a header in practice).
pub fn format_imf_fixdate(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Time of day.
    let sec = (secs % 60) as u32;
    let min = ((secs / 60) % 60) as u32;
    let hour = ((secs / 3600) % 24) as u32;

    // Days since 1970-01-01 (= Thursday, day 4 of week if Sun=0).
    let days = (secs / 86_400) as i64;
    let weekday_idx = ((days + 4).rem_euclid(7)) as usize; // 0=Sun..6=Sat
    let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

    // Civil date — Howard Hinnant's algorithm (`days_from_civil` inverse).
    // Source: http://howardhinnant.github.io/date_algorithms.html#civil_from_days
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };

    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        day_names[weekday_idx],
        d,
        month_names[(m - 1) as usize],
        year,
        hour,
        min,
        sec,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn formats_canonical_imf_fixdate() {
        // 784111777 seconds after the epoch = Sun, 06 Nov 1994 08:49:37 GMT.
        let t = UNIX_EPOCH + Duration::from_secs(784_111_777);
        assert_eq!(format_imf_fixdate(t), "Sun, 06 Nov 1994 08:49:37 GMT");
    }

    #[test]
    fn formats_unix_epoch() {
        assert_eq!(format_imf_fixdate(UNIX_EPOCH), "Thu, 01 Jan 1970 00:00:00 GMT");
    }
}
```

- [ ] **Step 2: Run the 2 tests.**

```bash
cargo test -p envoy-http1 formats_canonical_imf_fixdate formats_unix_epoch
```

Expected: 2 PASS. If either fails, the most likely culprit is the Hinnant inverse — verify against a third reference (e.g., Python: `datetime.datetime.utcfromtimestamp(784111777).strftime("%a, %d %b %Y %H:%M:%S GMT")` should also produce `Sun, 06 Nov 1994 08:49:37 GMT`).

- [ ] **Step 3: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean. If clippy flags the i64/u64 cast chain, accept the lint suppression at the function level (`#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]`) — the algorithm's casts are deliberate and documented by Hinnant.

- [ ] **Step 4: Append a Task 7 section to PROGRESS.md.**

```markdown
## Task 7 — envoy-http1::date (2026-04-27)

- Commit: <SHA>
- Change: implemented format_imf_fixdate via Howard Hinnant's days-from-civil algorithm; ~30 LoC; 2 unit tests pinning the canonical 1994-11-06 example + the Unix epoch.
- Verification: `cargo test -p envoy-http1` → +2 tests; cross-check via `python3 -c 'import datetime; print(datetime.datetime.utcfromtimestamp(784111777).strftime("%a, %d %b %Y %H:%M:%S GMT"))'` → matches.
- Deviations: <document any — e.g., if the algorithm produced a different output, debugging path>.
```

- [ ] **Step 5: Commit.**

```bash
git add crates/envoy-http1/src/date.rs
git commit -m "phase 04.1: envoy-http1 — date module (IMF-fixdate writer) (task 7)"
```

---

### Task 8: `envoy-http1::codec` — `Http1Codec`, `Request`, `HttpVersion` + 5 tests

**Files:**
- Modify: `crates/envoy-http1/src/codec.rs` (replace stub with codec + value types + 5 tests)
- Modify: `crates/envoy-http1/src/lib.rs` (add `pub use codec::{Http1Codec, Request, HttpVersion};`)
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Task 10's HCM consumes `Http1Codec::parse_request` + `Request` directly. Task 9 (response writer) is independent and parallel — but lands first by alphabetical / dependency-graph convention here. Either order is acceptable; the chosen order is codec → response → hcm to match SPEC §3 D1's module enumeration.

**Scope.** Stateless codec (one-shot `parse_request`); `Request` value type with `bytes_consumed`; `HttpVersion` enum. 5 unit tests per SPEC §3 D1.

The codec is a thin wrapper over `httparse::Request::parse`. Per SPEC §3 D1: stateless because the per-connection state machine in `hcm.rs` already owns the buffer (`bytes::BytesMut`-backed). The codec is a pure parser.

The 8 KiB headers cap (per SPEC §6 signpost 14 + parent-SPEC §3 cross-sub-phase rule "headers cap matches phase-02.2 admin tightening per phase-01 REVIEW I4") is enforced at the codec level — `httparse` will return `Status::Partial` on small buffers, which is fine; the cap fires on the codec's accumulated-buffer-length check, not on httparse's internal limits. Concretely: if the buffer grows past 8192 bytes without httparse returning `Status::Complete`, the codec returns `HeadersTooLarge { cap: 8192 }`.

- [ ] **Step 1: Write 5 failing tests.**

Create `crates/envoy-http1/src/codec.rs`:

```rust
//! HTTP/1.1 request codec — a thin wrapper over `httparse::Request::parse`.

use crate::error::Http1Error;

/// Maximum size of the request headers section, in bytes.
/// Matches phase-02.2's admin tightening (per phase-01 REVIEW I4).
const HEADERS_CAP: usize = 8192;

/// Maximum number of header rows in a single request. Matches httparse's
/// default; sized so an attacker cannot flood the headers vec.
const MAX_HEADERS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    Http10,
    Http11,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    /// Method as parsed (case preserved). HTTP/1.1 §3.1.1: methods are
    /// case-sensitive — but the HCM in 04.x does not branch on method,
    /// so case-preservation here is just for forward-compat / logging.
    pub method: String,

    /// Request-target as raw bytes. The HCM matches `prefix:` / `path:`
    /// against this byte-for-byte (no normalization).
    pub path: String,

    pub version: HttpVersion,

    /// Header rows in emission order. Names are case-preserved as written
    /// (per the case-preserving storage discipline); use `find_header`
    /// for case-insensitive lookup.
    pub headers: Vec<(String, String)>,

    /// Number of bytes consumed from the input buffer to produce this
    /// request (= the offset to the start of the body, if any).
    pub bytes_consumed: usize,
}

pub struct Http1Codec;

impl Http1Codec {
    /// Attempt to parse a single HTTP/1.1 request from `buf`. Returns:
    /// - `Ok(Some(req))` on a fully-parsed request;
    /// - `Ok(None)` if `buf` does not yet contain a complete request
    ///   (caller reads more bytes and retries);
    /// - `Err(Http1Error::HeadersTooLarge)` if the buffer is past the
    ///   headers cap without httparse signaling `Complete`;
    /// - `Err(Http1Error::MalformedRequestLine)` / `MalformedHeader` on
    ///   malformed input.
    pub fn parse_request(buf: &[u8]) -> Result<Option<Request>, Http1Error> {
        if buf.len() > HEADERS_CAP {
            // Headers cap fires only when the buffer itself exceeds the cap
            // before httparse signaled Complete; once a request is fully
            // parsed within HEADERS_CAP, subsequent body bytes can grow the
            // buffer past the cap without tripping this check (the caller's
            // state machine has already advanced past the headers).
            // To distinguish: try parsing first; only flag HeadersTooLarge
            // if parsing returns Partial.
        }

        let mut headers_storage = [httparse::EMPTY_HEADER; MAX_HEADERS];
        let mut parsed = httparse::Request::new(&mut headers_storage);

        let bytes_consumed = match parsed.parse(buf) {
            Ok(httparse::Status::Complete(n)) => n,
            Ok(httparse::Status::Partial) => {
                if buf.len() > HEADERS_CAP {
                    return Err(Http1Error::HeadersTooLarge { cap: HEADERS_CAP });
                }
                return Ok(None);
            }
            Err(httparse::Error::TooManyHeaders) => {
                return Err(Http1Error::HeadersTooLarge { cap: HEADERS_CAP });
            }
            Err(httparse::Error::HeaderName)
            | Err(httparse::Error::HeaderValue)
            | Err(httparse::Error::NewLine)
            | Err(httparse::Error::Status) => {
                return Err(Http1Error::MalformedHeader);
            }
            Err(httparse::Error::Token)
            | Err(httparse::Error::Version) => {
                return Err(Http1Error::MalformedRequestLine);
            }
        };

        // httparse guarantees method/path/version are Some on Complete.
        let method = parsed.method.unwrap_or("").to_string();
        let path = parsed.path.unwrap_or("").to_string();
        let version = match parsed.version {
            Some(0) => HttpVersion::Http10,
            Some(1) => HttpVersion::Http11,
            _ => return Err(Http1Error::MalformedRequestLine),
        };

        // Convert borrowed httparse headers into owned String pairs.
        let headers: Vec<(String, String)> = parsed
            .headers
            .iter()
            .filter(|h| !h.name.is_empty())
            .map(|h| {
                let name = h.name.to_string();
                let value = std::str::from_utf8(h.value)
                    .map(str::to_string)
                    .unwrap_or_default();
                (name, value)
            })
            .collect();

        Ok(Some(Request {
            method,
            path,
            version,
            headers,
            bytes_consumed,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_get_root_with_host() {
        let buf = b"GET /healthz HTTP/1.1\r\nHost: x\r\n\r\n";
        let req = Http1Codec::parse_request(buf)
            .expect("ok")
            .expect("complete");
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/healthz");
        assert_eq!(req.version, HttpVersion::Http11);
        assert_eq!(req.bytes_consumed, buf.len());
        assert_eq!(req.headers.len(), 1);
        assert_eq!(req.headers[0].0, "Host");
        assert_eq!(req.headers[0].1, "x");
    }

    #[test]
    fn returns_none_on_partial_request_line() {
        let buf = b"GET /healthz HTTP/";
        assert_eq!(Http1Codec::parse_request(buf).expect("no err"), None);
    }

    #[test]
    fn returns_err_on_malformed_request_line() {
        // Missing path/version after method — httparse returns Err::Token or Err::NewLine.
        let buf = b"GET\r\n\r\n";
        let err = Http1Codec::parse_request(buf).expect_err("malformed");
        // Either MalformedRequestLine (Token/Version) or MalformedHeader (NewLine)
        // is acceptable here — the failure-mode taxonomy isn't load-bearing for
        // the test, only that we reject.
        assert!(matches!(
            err,
            Http1Error::MalformedRequestLine | Http1Error::MalformedHeader
        ), "got: {:?}", err);
    }

    #[test]
    fn enforces_headers_cap() {
        // 9 KiB of headers ensures httparse returns Partial on a buffer past
        // the 8 KiB cap; the codec then returns HeadersTooLarge.
        let mut buf = b"GET / HTTP/1.1\r\nHost: x\r\n".to_vec();
        for i in 0..200 {
            buf.extend_from_slice(format!("X-Pad-{i}: {}\r\n", "a".repeat(40)).as_bytes());
        }
        // No trailing CRLF, so httparse keeps returning Partial.
        let err = Http1Codec::parse_request(&buf).expect_err("too large");
        assert!(matches!(err, Http1Error::HeadersTooLarge { cap: 8192 }),
                "got: {:?}", err);
    }

    #[test]
    fn preserves_header_emission_order_and_case() {
        let buf = b"GET / HTTP/1.1\r\nHost: x\r\nX-Foo: 1\r\nX-Bar: 2\r\nX-Foo: 3\r\n\r\n";
        let req = Http1Codec::parse_request(buf)
            .expect("ok")
            .expect("complete");
        let names: Vec<&str> = req.headers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["Host", "X-Foo", "X-Bar", "X-Foo"]);
        let values: Vec<&str> = req.headers.iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(values, vec!["x", "1", "2", "3"]);
    }
}
```

- [ ] **Step 2: Add the `pub use` re-export to `crates/envoy-http1/src/lib.rs`.**

Insert after the existing `pub use error::Http1Error;` line:

```rust
pub use codec::{Http1Codec, Request, HttpVersion};
```

- [ ] **Step 3: Run the 5 tests.**

```bash
cargo test -p envoy-http1 \
  parses_get_root_with_host \
  returns_none_on_partial_request_line \
  returns_err_on_malformed_request_line \
  enforces_headers_cap \
  preserves_header_emission_order_and_case
```

Expected: 5 PASS. The most likely failure mode is `enforces_headers_cap`: if httparse 1.x returns `TooManyHeaders` when MAX_HEADERS is exceeded (it does — the codec maps that to `HeadersTooLarge`), the test passes; if instead the buffer-len check fires first, also passes. Either path is acceptable.

- [ ] **Step 4: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean. The `unused_unit` or similar lints around the `headers_storage` array initialization may need a small adjustment; if so, accept the simplest fix.

- [ ] **Step 5: Append a Task 8 section to PROGRESS.md.**

```markdown
## Task 8 — envoy-http1::codec (2026-04-27)

- Commit: <SHA>
- Change: replaced codec.rs stub with Http1Codec + Request + HttpVersion + 5 unit tests; re-exported from lib.rs.
- Verification: `cargo test -p envoy-http1` → +5 tests; `cargo clippy --workspace ... -- -D warnings` → clean.
- Deviations: <document any — e.g., if httparse error mapping needed adjustment>.
```

- [ ] **Step 6: Commit.**

```bash
git add crates/envoy-http1/src/codec.rs crates/envoy-http1/src/lib.rs
git commit -m "phase 04.1: envoy-http1 — codec module (Http1Codec + Request + 5 tests) (task 8)"
```

---

### Task 9: `envoy-http1::response` — `Http1Response` writer + 2 tests

**Files:**
- Modify: `crates/envoy-http1/src/response.rs` (replace stub with writer + 2 tests)
- Modify: `crates/envoy-http1/src/lib.rs` (add `pub use response::{Http1Response, Response};`)
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Task 10's HCM `synth_*` helpers each return a `Http1Response` value type; the HCM state machine then calls `Http1Response::write_to(&mut downstream)` to serialize. Land the writer first.

**Scope.** Two value types (`Response` was projected by SPEC §3 D1 but the codec already exposed `Request`; `Response` is the structural sibling) and one writer impl. 2 unit tests per SPEC §3 D1.

Per SPEC §3 D1: caller is responsible for setting `Content-Length: <body.len()>` in `headers` — `Http1Response` does not auto-compute it (HCM does). The writer is dumb: status line + headers in emission order + CRLF + body.

- [ ] **Step 1: Write 2 failing tests + impl.**

Replace `crates/envoy-http1/src/response.rs`:

```rust
//! Wire-format HTTP/1.1 response writer.

use crate::error::Http1Error;
use bytes::Bytes;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;

/// A logical HTTP response. Caller fills in `headers` with all required
/// fields (the writer does NOT compute Content-Length); HCM's `synth_*`
/// helpers in 04.1 always populate `server`, `date`, `content-length`,
/// `content-type`, `connection` in this exact order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,                       // 100..=599
    pub reason: Option<&'static str>,      // canonical reason per RFC 7231 §6.1;
                                           //   None falls back to a built-in table.
    pub headers: Vec<(String, String)>,    // emission-order preserving.
    pub body: Bytes,                       // CL-framed in 04.1; chunked deferred.
}

pub struct Http1Response;

impl Http1Response {
    /// Serializes `resp` onto `w` as a wire-format HTTP/1.1 response:
    /// status line + headers (in emission order) + blank line + body.
    pub async fn write_to<W>(resp: &Response, w: &mut W) -> Result<(), Http1Error>
    where
        W: AsyncWrite + Unpin,
    {
        let reason = resp.reason.unwrap_or_else(|| canonical_reason(resp.status));
        let mut buf: Vec<u8> = Vec::with_capacity(
            64 + resp.headers.iter().map(|(n, v)| n.len() + v.len() + 4).sum::<usize>()
              + resp.body.len(),
        );
        // Status line.
        buf.extend_from_slice(b"HTTP/1.1 ");
        buf.extend_from_slice(resp.status.to_string().as_bytes());
        buf.push(b' ');
        buf.extend_from_slice(reason.as_bytes());
        buf.extend_from_slice(b"\r\n");
        // Headers.
        for (name, value) in &resp.headers {
            buf.extend_from_slice(name.as_bytes());
            buf.extend_from_slice(b": ");
            buf.extend_from_slice(value.as_bytes());
            buf.extend_from_slice(b"\r\n");
        }
        // Blank line + body.
        buf.extend_from_slice(b"\r\n");
        buf.extend_from_slice(&resp.body);

        w.write_all(&buf).await?;
        w.flush().await?;
        Ok(())
    }
}

/// Canonical reason phrase for a status code per RFC 7231 §6.1.
/// Returns `"OK"` for unknown codes (matches Envoy's posture; cross-check
/// at execution time — if Envoy emits a different reason for an unknown
/// code, this falls back is harmless because the value-exact diff for the
/// status line is not part of the equivalence matrix).
fn canonical_reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Payload Too Large",
        414 => "URI Too Long",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        505 => "HTTP Version Not Supported",
        _ => "OK",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Run an async write into an in-memory Cursor and return the bytes.
    async fn write_to_vec(resp: &Response) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        Http1Response::write_to(resp, &mut buf).await.expect("write");
        buf
    }

    #[tokio::test]
    async fn writes_status_line_headers_body() {
        let resp = Response {
            status: 200,
            reason: None,
            headers: vec![
                ("server".to_string(), "envoy-rust".to_string()),
                ("date".to_string(), "Sun, 06 Nov 1994 08:49:37 GMT".to_string()),
                ("content-length".to_string(), "3".to_string()),
                ("content-type".to_string(), "text/plain".to_string()),
                ("connection".to_string(), "keep-alive".to_string()),
            ],
            body: Bytes::from_static(b"ok\n"),
        };
        let buf = write_to_vec(&resp).await;
        let expected = b"HTTP/1.1 200 OK\r\n\
                         server: envoy-rust\r\n\
                         date: Sun, 06 Nov 1994 08:49:37 GMT\r\n\
                         content-length: 3\r\n\
                         content-type: text/plain\r\n\
                         connection: keep-alive\r\n\
                         \r\n\
                         ok\n";
        assert_eq!(buf, expected.as_ref(), "wire bytes match");
        // Cursor used to satisfy unused-import lint suppression — drop after impl.
        let _ = Cursor::new(b"");
    }

    #[tokio::test]
    async fn writes_204_with_no_body() {
        let resp = Response {
            status: 204,
            reason: None,
            headers: vec![
                ("server".to_string(), "envoy-rust".to_string()),
                ("connection".to_string(), "keep-alive".to_string()),
            ],
            body: Bytes::new(),
        };
        let buf = write_to_vec(&resp).await;
        let expected = b"HTTP/1.1 204 No Content\r\n\
                         server: envoy-rust\r\n\
                         connection: keep-alive\r\n\
                         \r\n";
        assert_eq!(buf, expected.as_ref());
    }
}
```

The `Cursor` import + `let _ = Cursor::new(...)` line is a convenience — if it's not needed (e.g., test compiles cleanly without it), drop both.

- [ ] **Step 2: Add the `pub use` re-export to `crates/envoy-http1/src/lib.rs`.**

```rust
pub use response::{Http1Response, Response};
```

- [ ] **Step 3: Run the 2 tests.**

```bash
cargo test -p envoy-http1 writes_status_line_headers_body writes_204_with_no_body
```

Expected: 2 PASS.

- [ ] **Step 4: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 5: Append a Task 9 section to PROGRESS.md.**

```markdown
## Task 9 — envoy-http1::response (2026-04-27)

- Commit: <SHA>
- Change: replaced response.rs stub with Response value type + Http1Response writer + canonical_reason helper + 2 unit tests; re-exported from lib.rs.
- Verification: `cargo test -p envoy-http1` → +2 tests; `cargo clippy --workspace ... -- -D warnings` → clean.
- Deviations: <document any>.
```

- [ ] **Step 6: Commit.**

```bash
git add crates/envoy-http1/src/response.rs crates/envoy-http1/src/lib.rs
git commit -m "phase 04.1: envoy-http1 — response module (Http1Response writer + 2 tests) (task 9)"
```

---

### Task 10: `envoy-http1::hcm` — `HCM`, `HCMConfig`, `serve_connection`, `build_response`, `synth_*` + 6 tests

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (replace stub with HCM + state machine + 6 tests)
- Modify: `crates/envoy-http1/src/lib.rs` (add `pub use hcm::{HCM, HCMConfig};`)
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Task 11's envoy-bin dispatch arm consumes `HCM` + `HCMConfig::from_config`. Task 12's integration test exercises the full state machine. This is the largest single task in the phase (~280 LoC + 6 tests = ~380 LoC) but cannot be split without crossing a module boundary mid-impl.

**Scope.** Per SPEC §3 D1 + D3:

- `HCMConfig::from_config(&envoy_config::HttpConnectionManagerConfig) -> Result<Self, Http1Error>` — builds the per-listener immutable config from validated envoy-config types.
- `HCM { config: Arc<HCMConfig> }` + `impl ConnectionHandler for HCM` (delegates to `serve_connection`).
- `serve_connection(config: Arc<HCMConfig>, downstream: TcpStream)` — the per-connection state machine: read into `BytesMut`, parse, validate Host, walk routes, dispatch via `build_response`, write response, lifecycle.
- `build_response(config: &HCMConfig, req: &Request) -> Result<Response, Http1Error>` — VH walk + route walk + `RouteAction` match. The hardcoded router-filter call site lives here as the inner `match` over `direct_response` (exhaustive in 04.1 because the schema only parses `direct_response`; 04.3 extends the match).
- `synth_400`, `synth_404`, `synth_501`, `synth_direct_response` — pure functions building `Response` value types.
- 6 unit tests per SPEC §3 D1.

The state machine handles HTTP/1.1 keep-alive, idle 5s read timeout, request body drain, `Connection: close`, `Transfer-Encoding: chunked` request rejection (501).

- [ ] **Step 1: Write the 6 failing tests at the head of the file.**

Each test drives an in-process `tokio::net::TcpStream` pair: client side writes a request, server side runs `serve_connection`, client side reads the response and asserts.

```rust
//! HTTP connection manager: per-listener config, per-connection state machine,
//! route walker, hardcoded router-filter call site.

use crate::codec::{Http1Codec, Request, HttpVersion};
use crate::date::format_imf_fixdate;
use crate::error::Http1Error;
use crate::headers::{self, find_header};
use crate::response::{Http1Response, Response};

use bytes::{Bytes, BytesMut, Buf};
use envoy_config::{
    HttpConnectionManagerConfig, RouteConfiguration, VirtualHost, Route, RouteMatch,
    DirectResponse, DataSource,
};
use envoy_listener::{BoxFuture, ConnectionHandler};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::net::TcpStream;

const DEFAULT_SERVER_NAME: &str = "envoy-rust";
const DEFAULT_CONTENT_TYPE: &str = "text/plain";
const IDLE_READ_TIMEOUT: Duration = Duration::from_secs(5);
const READ_BUFFER_INITIAL_CAPACITY: usize = 8192;

#[derive(Debug)]
pub struct HCMConfig {
    pub stat_prefix: String,
    pub route_config: Arc<RouteConfiguration>,
    // 04.3: pub cluster_mgr: Arc<envoy_cluster::ClusterManager>,
}

impl HCMConfig {
    pub fn from_config(cfg: &HttpConnectionManagerConfig) -> Result<Self, Http1Error> {
        // The validator (envoy-config Task 2) has already enforced shape.
        // This constructor is `Result<>` for forward-compat with 04.3's
        // cluster lookup; in 04.1 it never returns Err.
        Ok(Self {
            stat_prefix: cfg.stat_prefix.clone(),
            route_config: Arc::new(clone_route_config(&cfg.route_config)),
        })
    }
}

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
                        },
                        direct_response: DirectResponse {
                            status: r.direct_response.status,
                            body: DataSource {
                                filename: r.direct_response.body.filename.clone(),
                                inline_string: r.direct_response.body.inline_string.clone(),
                            },
                        },
                    })
                    .collect(),
            })
            .collect(),
    }
}
// (If envoy-config can derive Clone on these types in Task 1's edit, drop
//  clone_route_config and use `rc.clone()`.)

pub struct HCM {
    pub config: Arc<HCMConfig>,
}

impl ConnectionHandler for HCM {
    fn handle(
        &self,
        downstream: TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let config = self.config.clone();
        Box::pin(async move {
            serve_connection(config, downstream)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

async fn serve_connection(
    config: Arc<HCMConfig>,
    mut downstream: TcpStream,
) -> Result<(), Http1Error> {
    let mut buf = BytesMut::with_capacity(READ_BUFFER_INITIAL_CAPACITY);
    loop {
        // 1. Try parsing what's already in the buffer (for keep-alive
        //    second-and-later requests where bytes from the previous read
        //    may already contain the next request's headers).
        let req = match Http1Codec::parse_request(&buf)? {
            Some(req) => req,
            None => {
                // 2. Need more bytes. Read with idle timeout.
                let read_n = match tokio::time::timeout(
                    IDLE_READ_TIMEOUT,
                    downstream.read_buf(&mut buf),
                ).await {
                    Ok(Ok(0)) => {
                        // peer closed; clean exit if the buffer is empty.
                        if buf.is_empty() { return Ok(()); }
                        return Err(Http1Error::UnexpectedEof);
                    }
                    Ok(Ok(_)) => continue,                       // re-parse
                    Ok(Err(source)) => return Err(Http1Error::Io { source }),
                    Err(_elapsed) => return Ok(()),              // idle timeout → clean close
                };
                let _ = read_n;
                continue;
            }
        };

        // 3. Determine close/keep-alive decision before any move.
        let close = req.headers.iter().any(|(n, v)| {
            n.eq_ignore_ascii_case(headers::CONNECTION) && v.eq_ignore_ascii_case("close")
        }) || req.version == HttpVersion::Http10;

        // 4. Compute body length (for drain) before consuming.
        let body_len = parse_content_length(&req.headers)?;
        let chunked = req.headers.iter().any(|(n, v)| {
            n.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked")
        });

        // 5. Build response (handles 400 / 404 / 501 / 200 internally).
        let mut resp = if chunked {
            synth_501(close)
        } else {
            build_response(&config, &req, close)
        };

        // 6. Stamp Date header now (as close to write time as possible for
        //    Envoy-equivalence; allow-listed anyway).
        // (`build_response` and `synth_*` already set Date — but using
        // SystemTime::now() each time means subsequent loop iterations
        // get fresh stamps. The `synth_*` helpers each call format_imf_fixdate
        // internally — no double-stamp.)
        let _ = &mut resp;

        // 7. Advance the buffer past the consumed request + body.
        let consumed = req.bytes_consumed;
        buf.advance(consumed);
        // 8. Drain body bytes (read_exact-style; up to body_len).
        let drained_so_far = buf.len().min(body_len);
        buf.advance(drained_so_far);
        let mut remaining = body_len - drained_so_far;
        while remaining > 0 {
            let mut throwaway = [0u8; 4096];
            let to_read = throwaway.len().min(remaining);
            let n = match tokio::time::timeout(
                IDLE_READ_TIMEOUT,
                downstream.read(&mut throwaway[..to_read]),
            ).await {
                Ok(Ok(0)) => return Err(Http1Error::UnexpectedEof),
                Ok(Ok(n)) => n,
                Ok(Err(source)) => return Err(Http1Error::Io { source }),
                Err(_elapsed) => return Ok(()),
            };
            remaining -= n;
        }

        // 9. Write response.
        Http1Response::write_to(&resp, &mut downstream).await?;

        // 10. Connection lifecycle.
        if close {
            return Ok(());
        }
        // Loop back; the buffer may contain pipelined bytes already, or
        // may need another read.
    }
}

fn parse_content_length(headers: &[(String, String)]) -> Result<usize, Http1Error> {
    match find_header(headers, headers::CONTENT_LENGTH) {
        Some(v) => v
            .parse::<usize>()
            .map_err(|_| Http1Error::MalformedHeader),
        None => Ok(0),
    }
}

fn build_response(config: &HCMConfig, req: &Request, close: bool) -> Response {
    // Validate Host header presence (HTTP/1.1 §5.4 — mandatory).
    let host_raw = match find_header(&req.headers, headers::HOST) {
        Some(h) => h,
        None => return synth_400(close),
    };
    let host = strip_port(host_raw);

    // Walk virtual_hosts first-match-wins on Host.
    let vh = match config
        .route_config
        .virtual_hosts
        .iter()
        .find(|vh| vh_matches(vh, host))
    {
        Some(vh) => vh,
        None => return synth_404(close),
    };

    // Walk routes first-match-wins on path.
    let route = match vh.routes.iter().find(|r| route_matches(r, &req.path)) {
        Some(r) => r,
        None => return synth_404(close),
    };

    // Hardcoded router-filter call site:
    //   match action { DirectResponse(dr) => synth_direct_response(req, dr) }
    // 04.3 will extend this match with a Route(_) arm.
    synth_direct_response(&route.direct_response, close)
}

fn strip_port(host: &str) -> &str {
    match host.rfind(':') {
        Some(i) => &host[..i],
        None => host,
    }
}

fn vh_matches(vh: &VirtualHost, host: &str) -> bool {
    vh.domains.iter().any(|d| {
        if d == "*" {
            true
        } else {
            d.eq_ignore_ascii_case(host)
        }
    })
}

fn route_matches(r: &Route, path: &str) -> bool {
    match (&r.r#match.prefix, &r.r#match.path) {
        (Some(p), None) => path.starts_with(p),
        (None, Some(p)) => path == p,
        _ => false, // validator rejects (Some, Some) and (None, None).
    }
}

fn now_imf_fixdate() -> String {
    format_imf_fixdate(SystemTime::now())
}

fn connection_value(close: bool) -> &'static str {
    if close { "close" } else { "keep-alive" }
}

fn synth_direct_response(dr: &DirectResponse, close: bool) -> Response {
    let body_str = dr.body.inline_string.as_deref().unwrap_or("");
    let body = Bytes::copy_from_slice(body_str.as_bytes());
    Response {
        status: dr.status,
        reason: None,
        headers: vec![
            (headers::SERVER.to_string(), DEFAULT_SERVER_NAME.to_string()),
            (headers::DATE.to_string(), now_imf_fixdate()),
            (headers::CONTENT_LENGTH.to_string(), body.len().to_string()),
            (headers::CONTENT_TYPE.to_string(), DEFAULT_CONTENT_TYPE.to_string()),
            (headers::CONNECTION.to_string(), connection_value(close).to_string()),
        ],
        body,
    }
}

fn synth_status(status: u16, close: bool) -> Response {
    let body = Bytes::new();
    Response {
        status,
        reason: None,
        headers: vec![
            (headers::SERVER.to_string(), DEFAULT_SERVER_NAME.to_string()),
            (headers::DATE.to_string(), now_imf_fixdate()),
            (headers::CONTENT_LENGTH.to_string(), "0".to_string()),
            (headers::CONTENT_TYPE.to_string(), DEFAULT_CONTENT_TYPE.to_string()),
            (headers::CONNECTION.to_string(), connection_value(close).to_string()),
        ],
        body,
    }
}

fn synth_400(close: bool) -> Response { synth_status(400, close) }
fn synth_404(close: bool) -> Response { synth_status(404, close) }
fn synth_501(close: bool) -> Response { synth_status(501, close) }

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{
        HttpConnectionManagerConfig, RouteConfiguration, VirtualHost, Route, RouteMatch,
        DirectResponse, DataSource, CodecType,
    };
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Build a minimal HCMConfig with a single VH `domains: ["*"]`,
    /// configurable routes.
    fn hcm_config_single_route(prefix: &str, status: u16, body: &str) -> Arc<HCMConfig> {
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            route_config: Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some(prefix.to_string()),
                            path: None,
                        },
                        direct_response: DirectResponse {
                            status,
                            body: DataSource {
                                filename: None,
                                inline_string: Some(body.to_string()),
                            },
                        },
                    }],
                }],
            }),
        })
    }

    /// Drive a single request through serve_connection over an in-process pair.
    /// Returns the response bytes.
    async fn drive(config: Arc<HCMConfig>, req_bytes: &[u8]) -> Vec<u8> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let _ = serve_connection(config, sock).await;
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(req_bytes).await.unwrap();
        let mut buf = Vec::new();
        client.read_to_end(&mut buf).await.unwrap();
        // Drop client to ensure server's loop exits.
        drop(client);
        let _ = server.await;
        buf
    }

    #[tokio::test]
    async fn direct_response_returns_status_and_body() {
        let config = hcm_config_single_route("/", 200, "ok\n");
        let req = b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(resp_str.starts_with("HTTP/1.1 200 OK\r\n"), "status: {resp_str}");
        assert!(resp_str.contains("server: envoy-rust\r\n"), "server: {resp_str}");
        assert!(resp_str.contains("date: "), "date: {resp_str}");
        assert!(resp_str.contains("content-length: 3\r\n"), "cl: {resp_str}");
        assert!(resp_str.contains("content-type: text/plain\r\n"), "ct: {resp_str}");
        assert!(resp_str.contains("connection: close\r\n"), "conn: {resp_str}");
        assert!(resp_str.ends_with("\r\nok\n"), "body: {resp_str}");
    }

    #[tokio::test]
    async fn host_match_strips_port() {
        let config = Arc::new(HCMConfig {
            stat_prefix: "x".to_string(),
            route_config: Arc::new(RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "specific".to_string(),
                    domains: vec!["foo.example.com".to_string()],
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                        },
                        direct_response: DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("hit\n".to_string()),
                            },
                        },
                    }],
                }],
            }),
        });
        let req = b"GET / HTTP/1.1\r\nHost: foo.example.com:8080\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(resp_str.starts_with("HTTP/1.1 200 OK\r\n"), "expected 200, got: {resp_str}");
        assert!(resp_str.ends_with("\r\nhit\n"));
    }

    #[tokio::test]
    async fn first_match_wins_on_routes() {
        let config = Arc::new(HCMConfig {
            stat_prefix: "x".to_string(),
            route_config: Arc::new(RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    routes: vec![
                        Route {
                            r#match: RouteMatch {
                                prefix: Some("/healthz".to_string()),
                                path: None,
                            },
                            direct_response: DirectResponse {
                                status: 200,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("first\n".to_string()),
                                },
                            },
                        },
                        Route {
                            r#match: RouteMatch {
                                prefix: Some("/".to_string()),
                                path: None,
                            },
                            direct_response: DirectResponse {
                                status: 500,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("never\n".to_string()),
                                },
                            },
                        },
                    ],
                }],
            }),
        });
        let req = b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(resp_str.starts_with("HTTP/1.1 200 OK\r\n"), "first match must win: {resp_str}");
        assert!(resp_str.ends_with("\r\nfirst\n"));
    }

    #[tokio::test]
    async fn missing_host_returns_400() {
        let config = hcm_config_single_route("/", 200, "ok\n");
        let req = b"GET / HTTP/1.1\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(resp_str.starts_with("HTTP/1.1 400 Bad Request\r\n"), "got: {resp_str}");
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let config = Arc::new(HCMConfig {
            stat_prefix: "x".to_string(),
            route_config: Arc::new(RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "specific".to_string(),
                    domains: vec!["only.example.com".to_string()],
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                        },
                        direct_response: DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        },
                    }],
                }],
            }),
        });
        // Host doesn't match any VH → 404.
        let req = b"GET / HTTP/1.1\r\nHost: other.example.com\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(resp_str.starts_with("HTTP/1.1 404 Not Found\r\n"), "got: {resp_str}");
    }

    #[tokio::test]
    async fn connection_close_closes_socket() {
        let config = hcm_config_single_route("/", 200, "ok\n");
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(resp_str.contains("connection: close\r\n"), "got: {resp_str}");
        // drive() called read_to_end which returns 0 once server closes — no
        // additional check needed beyond that drive returned at all.
    }
}
```

- [ ] **Step 2: Add the `pub use` re-export to `crates/envoy-http1/src/lib.rs`.**

```rust
pub use hcm::{HCM, HCMConfig};
```

- [ ] **Step 3: Run the 6 tests.**

```bash
cargo test -p envoy-http1 \
  direct_response_returns_status_and_body \
  host_match_strips_port \
  first_match_wins_on_routes \
  missing_host_returns_400 \
  unknown_route_returns_404 \
  connection_close_closes_socket
```

Expected: 6 PASS. Common execution-time issues:

- **`envoy-config` types may be `Clone`-derivable — check Task 1's edit.** If the types derive `Clone`, drop the `clone_route_config` helper and use `cfg.route_config.clone()`.
- **`envoy-listener::ConnectionHandler` shape mismatch.** Verify the trait signature: `fn handle(&self, downstream: TcpStream) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>`. If the project's `BoxFuture` re-export differs, adjust.
- **`tokio::io::AsyncReadExt` import path.** Should resolve cleanly with the `io-util` feature.

- [ ] **Step 4: Run the full envoy-http1 test suite.**

```bash
cargo test -p envoy-http1
```

Expected: full suite green. Total tests: Task 6 (2) + Task 7 (2) + Task 8 (5) + Task 9 (2) + Task 10 (6) = 17 unit tests.

- [ ] **Step 5: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 6: Append a Task 10 section to PROGRESS.md.**

```markdown
## Task 10 — envoy-http1::hcm (2026-04-27)

- Commit: <SHA>
- Change: implemented HCMConfig + HCM + serve_connection (per-connection state machine: read/parse/route/dispatch/write/lifecycle/idle-5s-timeout/body-drain/chunked-501-reject) + build_response (VH walk + route walk + hardcoded router call site) + synth_400/synth_404/synth_501/synth_direct_response helpers; 6 unit tests; re-exported HCM + HCMConfig.
- Verification: `cargo test -p envoy-http1` → 17 tests total (Task 6/7/8/9/10 = 2/2/5/2/6); `cargo clippy --workspace ... -- -D warnings` → clean.
- Deviations: <document any — e.g., clone_route_config helper retired if envoy-config types derive Clone, ConnectionHandler signature adjusted if differs from plan-time expectation>.
```

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-http1/src/hcm.rs crates/envoy-http1/src/lib.rs
git commit -m "phase 04.1: envoy-http1 — HCM module (state machine + route walker + 6 tests) (task 10)"
```

---

### Task 11: `envoy-bin` — `envoy-http1` dep + `TypedConfig::HttpConnectionManager` dispatch arm

**Files:**
- Modify: `crates/envoy-bin/Cargo.toml` (add `envoy-http1 = { path = "../envoy-http1" }`)
- Modify: `crates/envoy-bin/src/main.rs` (new dispatch arm in the listener-walk)
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Task 10 finished the HCM library. envoy-bin now wires it into the listener filter-chain dispatch sibling of the existing `TcpProxy` arm. After this task, fixture 0007 is reachable end-to-end via the binary; Task 12 backstops it with a Rust-native integration test.

**Scope.** One new path-dep + one new dispatch arm in the existing `match typed_config { ... }` block in `main.rs`. No new module file. Per SPEC §3 D4.

The HCM dispatch arm constructs `Arc<HCMConfig>` once via `HCMConfig::from_config(&hcm_cfg)?`, builds `Arc::new(HCM { config }) as Arc<dyn ConnectionHandler>`, and (if the filter chain has `transport_socket: Some(_)`) wraps in `TlsAcceptingHandler` per phase 03.1's existing wiring. The TLS-wrap branch is unreachable in 04.x fixtures (no fixture combines HTTP/1.1 + TLS) but the wiring is one line and avoids an `unreachable!()` ahead of phase 05's first such fixture.

- [ ] **Step 1: Read the current shape of `main.rs`'s listener-walk.**

```bash
grep -n 'TypedConfig::TcpProxy\|fn run\|TlsAcceptingHandler\|cluster_mgr.get' crates/envoy-bin/src/main.rs
```

Expected: a `match typed_config { ... TypedConfig::TcpProxy(_) => ..., }` block exists (introduced phase 02.2) plus a `TlsAcceptingHandler` adapter wiring (phase 03.1). Note line numbers for the dispatch arm.

- [ ] **Step 2: Add `envoy-http1` path-dep to `crates/envoy-bin/Cargo.toml`.**

In the `[dependencies]` section, alphabetically ordered:

```toml
envoy-http1 = { path = "../envoy-http1" }
```

- [ ] **Step 3: Add the HCM dispatch arm in `main.rs`.**

The change is a new arm in the existing `match` block that previously had a single `TcpProxy` arm. Insert as a sibling of the `TcpProxy` arm:

```rust
// (illustrative; locations and names match the actual phase-02.2/03.1 shape)
use envoy_http1::{HCM, HCMConfig};

// Inside the per-listener loop, in the match on typed_config:
match filter.typed_config.as_ref() {
    Some(TypedConfig::TcpProxy(tp_cfg)) => {
        // ... existing phase-02.2 / phase-03.1 path ...
    }
    Some(TypedConfig::HttpConnectionManager(hcm_cfg)) => {
        let hcm_config = Arc::new(HCMConfig::from_config(hcm_cfg)?);
        let hcm = Arc::new(HCM { config: hcm_config });

        let handler: Arc<dyn ConnectionHandler> = match chain.transport_socket.as_ref() {
            Some(ts) => {
                // Reuse the phase-03.1 TlsAcceptingHandler wiring. Unreachable
                // in 04.x fixtures but wired for forward-compat with phase 05+.
                let downstream_tls = build_downstream_tls_from_transport_socket(ts)?;
                Arc::new(TlsAcceptingHandler {
                    tls: Arc::new(downstream_tls),
                    inner: hcm,
                })
            }
            None => hcm,
        };

        listener.bind(handler).await?;
    }
    None => {
        return Err(anyhow!("filter has no typed_config"));
    }
}
```

The exact symbol names — `build_downstream_tls_from_transport_socket`, `TlsAcceptingHandler`, `chain.transport_socket` — match the phase-03.1 shape. If those names differ in the actual codebase, adapt at execution time and document deviation.

The `?` propagation on `HCMConfig::from_config(hcm_cfg)?` requires that the call return a type compatible with envoy-bin's error wrapping. envoy-bin uses `anyhow::Result<()>` per D-3.2; the `Http1Error` returned by `from_config` flows through `From<Http1Error> for anyhow::Error` automatically (anyhow's blanket conversion).

- [ ] **Step 4: Run `cargo build` to verify the binary compiles.**

```bash
cargo build -p envoy-bin
```

Expected: clean. If the `TypedConfig::HttpConnectionManager` arm pattern doesn't match — e.g., if the existing `match` was on something other than `&filter.typed_config` — adjust to the actual shape.

- [ ] **Step 5: Run all existing envoy-bin tests to verify no regression.**

```bash
cargo test -p envoy-bin
```

Expected: all green. The TcpProxy + TLS paths (phase-02.2 / 03.1 / 03.2 fixtures) are unchanged.

- [ ] **Step 6: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 7: Append a Task 11 section to PROGRESS.md.**

```markdown
## Task 11 — envoy-bin HCM dispatch (2026-04-27)

- Commit: <SHA>
- Change: added envoy-http1 path-dep; added TypedConfig::HttpConnectionManager match arm in main.rs's listener-walk; arm constructs Arc<HCMConfig> + wraps in HCM + optionally wraps in TlsAcceptingHandler when transport_socket is present (TLS-wrap path unreachable in 04.x fixtures but wired for forward-compat with phase 05+).
- Verification: `cargo build -p envoy-bin` → clean; `cargo test -p envoy-bin` → all existing tests green; workspace gate clean.
- Deviations: <document any — e.g., if symbol names differ from plan-time expectation>.
```

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-bin/Cargo.toml crates/envoy-bin/src/main.rs
git commit -m "phase 04.1: envoy-bin — HCM dispatch arm + envoy-http1 dep (task 11)"
```

---

### Task 12: `envoy-bin` integration test `tests/http1_direct_response.rs`

**Files:**
- Create: `crates/envoy-bin/tests/http1_direct_response.rs`
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Task 11 wired HCM but didn't test the end-to-end binary path. This task adds a Rust-native, no-Docker, in-process integration test that spawns `envoy-bin` as a subprocess (per phase-02.2's `tcp_proxy.rs` precedent) and drives a single HTTP/1.1 request through it. The Docker-gated differential test in Task 16 exercises both proxies; this task is the envoy-rust-only backstop so a regression in HCM wiring shows up locally without Docker.

**Scope.** One new integration test file (~120 LoC per SPEC §3 D4). Reuses phase-02.2's pattern of locating the binary via `env!("CARGO_BIN_EXE_envoy-bin")`. The test uses `httparse::Response::parse` to parse the response.

If `httparse` is not already a dev-dep of `envoy-bin` (it should be — phase 01 admin uses it as a runtime dep), add it to `[dev-dependencies]`. Verify:

```bash
grep -A 2 'httparse' crates/envoy-bin/Cargo.toml
```

If `httparse` is in `[dependencies]` (runtime), the `tests/` integration test reaches it directly. If not, add to dev-deps.

- [ ] **Step 1: Inspect `crates/envoy-bin/tests/tcp_proxy.rs` (phase 02.2 precedent) for the binary-locate + retry-loop shape.**

```bash
grep -n 'CARGO_BIN_EXE\|TempDir\|reserve_port\|tokio::time::timeout' crates/envoy-bin/tests/tcp_proxy.rs
```

Expected: helpers for binary-spawn + connect-retry-with-timeout. Mirror them.

- [ ] **Step 2: Write the integration test.**

```rust
//! Phase 04.1 envoy-bin integration test: spawn envoy-bin against a minimal
//! HCM-direct_response config, send a GET request, assert response shape.
//! No Docker. The Docker-gated differential test in tests/differential/tests/
//! http1_direct_response.rs is the full equivalence gate.

use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};

const ENVOY_RUST_BIN: &str = env!("CARGO_BIN_EXE_envoy-rust");
// (per phase-02.2's actual env var name; verify at execution time — could
// be CARGO_BIN_EXE_envoy-bin or similar depending on the binary name in
// envoy-bin/Cargo.toml's [[bin]] stanza.)

fn reserve_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn write_config(dir: &std::path::Path, port: u16) -> std::path::PathBuf {
    let cfg = format!(r#"
node:
  id: x
  cluster: y
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
                stat_prefix: ingress_http
                codec_type: HTTP1
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
"#);
    let path = dir.join("envoy-rust.yaml");
    std::fs::write(&path, cfg).unwrap();
    path
}

async fn wait_for_accept(addr: &str) -> anyhow::Result<()> {
    for _ in 0..50 {
        if TcpStream::connect(addr).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(anyhow::anyhow!("envoy-rust did not accept on {addr} within 5s"))
}

async fn spawn_envoy_rust(config: &std::path::Path) -> anyhow::Result<Child> {
    let child = Command::new(ENVOY_RUST_BIN)
        .arg("-c").arg(config)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    Ok(child)
}

#[tokio::test]
async fn http1_direct_response_round_trip() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let port = reserve_port();
    let cfg_path = write_config(dir.path(), port);
    let mut child = spawn_envoy_rust(&cfg_path).await?;
    let addr = format!("127.0.0.1:{port}");

    // Wait for accept.
    if let Err(e) = wait_for_accept(&addr).await {
        let _ = child.kill().await;
        return Err(e);
    }

    // Drive a single GET.
    let result = async {
        let mut stream = TcpStream::connect(&addr).await?;
        stream.write_all(b"GET /healthz HTTP/1.1\r\nHost: envoy-rust.test\r\n\r\n").await?;
        let mut buf = vec![0u8; 4096];
        let mut total = 0;
        // Read until headers + body are consumed.
        loop {
            let n = tokio::time::timeout(
                Duration::from_secs(5),
                stream.read(&mut buf[total..]),
            ).await??;
            if n == 0 { break; }
            total += n;
            // Try to parse.
            let mut headers = [httparse::EMPTY_HEADER; 32];
            let mut resp = httparse::Response::new(&mut headers);
            match resp.parse(&buf[..total])? {
                httparse::Status::Complete(headers_end) => {
                    // Find Content-Length.
                    let cl = resp.headers.iter()
                        .find(|h| h.name.eq_ignore_ascii_case("content-length"))
                        .and_then(|h| std::str::from_utf8(h.value).ok())
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    if total >= headers_end + cl {
                        // Assertions.
                        assert_eq!(resp.code, Some(200), "status");
                        let names_lc: Vec<String> = resp.headers.iter()
                            .map(|h| h.name.to_ascii_lowercase()).collect();
                        for required in ["server", "date", "content-length", "content-type", "connection"] {
                            assert!(names_lc.iter().any(|n| n == required),
                                    "missing header: {required}; got: {names_lc:?}");
                        }
                        assert_eq!(cl, 3, "content-length=3 for ok\\n body");
                        let body = &buf[headers_end..headers_end + cl];
                        assert_eq!(body, b"ok\n");
                        break;
                    }
                }
                httparse::Status::Partial => continue,
            }
            if total >= buf.len() { break; }
        }
        Ok::<_, anyhow::Error>(())
    }.await;

    let _ = child.kill().await;
    result
}
```

Two execution-time fixups likely needed:
- **`ENVOY_RUST_BIN` const name.** Phase-02.2's `tcp_proxy.rs` likely uses `env!("CARGO_BIN_EXE_<name>")` where `<name>` matches the package's `[[bin]]` name. Verify and adjust.
- **Config-file path argument.** `-c <path>` is Envoy's convention; verify envoy-rust's CLI accepts it (phase 01 introduced this). If the CLI uses a different flag, adapt.

- [ ] **Step 3: Run the integration test.**

```bash
cargo test -p envoy-bin --test http1_direct_response
```

Expected: PASS. Common failure modes:
- Envoy-rust subprocess fails to start because validator rejects the config: cross-check the YAML against Task 1 + Task 2 expectations.
- `wait_for_accept` times out: subprocess crashed before binding; check stderr.
- Header set assertion fails: the HCM emits a header in non-canonical case form (e.g., `Server` vs. `server`); `synth_*` helpers in Task 10 emit lowercase per `headers::*` constants — verify.

- [ ] **Step 4: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test -p envoy-bin
```

Expected: all green.

- [ ] **Step 5: Append a Task 12 section to PROGRESS.md.**

```markdown
## Task 12 — envoy-bin integration test (2026-04-27)

- Commit: <SHA>
- Change: added crates/envoy-bin/tests/http1_direct_response.rs — Rust-native integration test that spawns envoy-bin against an inline HCM-direct_response config and asserts the GET /healthz response shape (status 200, 5 expected headers, body "ok\n"). No Docker.
- Verification: `cargo test -p envoy-bin --test http1_direct_response` → PASS; full envoy-bin test suite still green.
- Deviations: <document any — e.g., binary env var name, CLI flag>.
```

- [ ] **Step 6: Commit.**

```bash
git add crates/envoy-bin/tests/http1_direct_response.rs
git commit -m "phase 04.1: envoy-bin integration test for HCM direct_response (task 12)"
```

---

### Task 13: Differential harness — `Driver::Http1` grammar + `HttpMethod` + `BodyRule` + `HeaderRule` + `AllowMode` + `HEADER_ALLOW_LIST` + `diff_headers` + 3 unit tests

**Files:**
- Modify: `tests/differential/Cargo.toml` (add `httparse = "1"` dev-dep)
- Modify: `tests/differential/src/lib.rs` (add types + constant + helper + 3 unit tests)
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Tasks 14 + 16 consume `Driver::Http1` + `HEADER_ALLOW_LIST` + `diff_headers`. Land the grammar + the constant + the diff helper + 3 unit tests in one task; Task 14 then adds the `drive_http1` async helper + `run_fixture` dispatch.

**Scope.** No async I/O in this task — pure data types + a synchronous diff helper. Per SPEC §3 D5.

- [ ] **Step 1: Add `httparse = "1"` to `tests/differential/Cargo.toml` `[dev-dependencies]`.**

(If already a dev-dep from a prior phase, skip.) Verify:

```bash
grep -A 1 '\[dev-dependencies\]' tests/differential/Cargo.toml | head -10
```

- [ ] **Step 2: Write 3 failing tests at the bottom of `tests/differential/src/lib.rs::tests`.**

Append:

```rust
#[test]
fn diff_headers_passes_set_equal_modulo_allow_list() {
    let envoy = vec![
        ("server".to_string(), "envoy".to_string()),
        ("date".to_string(), "Sun, 06 Nov 1994 08:49:37 GMT".to_string()),
        ("content-length".to_string(), "3".to_string()),
        ("content-type".to_string(), "text/plain".to_string()),
        ("connection".to_string(), "keep-alive".to_string()),
    ];
    let envoy_rust = vec![
        ("server".to_string(), "envoy-rust".to_string()),
        ("date".to_string(), "Mon, 07 Nov 1994 12:00:00 GMT".to_string()),
        ("content-length".to_string(), "3".to_string()),
        ("content-type".to_string(), "text/plain".to_string()),
        ("connection".to_string(), "keep-alive".to_string()),
    ];
    diff_headers(&envoy, &envoy_rust, HEADER_ALLOW_LIST).expect("server+date allow-listed");
}

#[test]
fn diff_headers_fails_on_value_diff_outside_allow_list() {
    let envoy = vec![
        ("content-length".to_string(), "3".to_string()),
    ];
    let envoy_rust = vec![
        ("content-length".to_string(), "4".to_string()),
    ];
    let err = diff_headers(&envoy, &envoy_rust, HEADER_ALLOW_LIST)
        .expect_err("content-length value mismatch");
    assert!(err.to_string().contains("content-length"), "msg: {err}");
}

#[test]
fn diff_headers_fails_on_name_set_diff() {
    let envoy = vec![
        ("x-foo".to_string(), "1".to_string()),
        ("date".to_string(), "...".to_string()),
    ];
    let envoy_rust = vec![
        ("date".to_string(), "...".to_string()),
    ];
    let err = diff_headers(&envoy, &envoy_rust, HEADER_ALLOW_LIST)
        .expect_err("envoy emits x-foo, envoy-rust does not");
    assert!(err.to_string().contains("x-foo"), "msg: {err}");
}
```

- [ ] **Step 3: Run the 3 tests to verify they fail.**

```bash
cargo test -p differential diff_headers_passes_set_equal_modulo_allow_list \
                          diff_headers_fails_on_value_diff_outside_allow_list \
                          diff_headers_fails_on_name_set_diff
```

Expected: 3 FAIL — `Driver::Http1`, `HEADER_ALLOW_LIST`, `diff_headers` don't exist.

- [ ] **Step 4: Add types + constant + diff helper to `tests/differential/src/lib.rs`.**

Insert near the existing `Driver` enum (introduced phase 02.2 / extended phase 03.1 / 03.2 with TLS variants):

```rust
// Existing Driver enum gets a new variant:
//   Http1 {
//       method: HttpMethod,
//       path: String,
//       host: String,
//       expected_status: Option<u16>,
//       expected_body: Option<BodyRule>,
//       expected_headers: Option<HeaderRule>,
//   }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    // 04.3 may add Post if the upstream-proxy fixture needs request-body
    // forwarding; otherwise 04.x is GET-only.
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyRule {
    ByteExact(Vec<u8>),
    // 04.3 adds: ByteExactWithRequestEcho — for the http1-echo-server's
    //   deterministic echo response shape.
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderRule {
    SetEqualModuloAllowList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowMode {
    NameRequired,
    // future: NameOptional, ValueRegex, ValueOneOf, ...
}

/// Header allow-list per BEHAVIOR_CONTRACT.md `Header allow-list` table.
/// Sourced from the contract; updates to the contract update this constant
/// in lockstep. 04.1 adds `server` and `date`; 04.3 adds
/// `x-envoy-upstream-service-time`.
pub const HEADER_ALLOW_LIST: &[(&str, AllowMode)] = &[
    ("server", AllowMode::NameRequired),
    ("date", AllowMode::NameRequired),
];

/// Set-equal modulo allow-list: case-insensitive name set equality, plus
/// value-exact match for any name not on the allow-list.
pub fn diff_headers(
    envoy: &[(String, String)],
    envoy_rust: &[(String, String)],
    allow_list: &[(&str, AllowMode)],
) -> anyhow::Result<()> {
    use std::collections::BTreeSet;

    fn names_lc(headers: &[(String, String)]) -> BTreeSet<String> {
        headers.iter().map(|(n, _)| n.to_ascii_lowercase()).collect()
    }

    let envoy_names = names_lc(envoy);
    let envoy_rust_names = names_lc(envoy_rust);

    if envoy_names != envoy_rust_names {
        let only_envoy: Vec<_> = envoy_names.difference(&envoy_rust_names).collect();
        let only_rust: Vec<_> = envoy_rust_names.difference(&envoy_names).collect();
        anyhow::bail!(
            "header name sets differ: only-in-envoy={only_envoy:?}, only-in-envoy-rust={only_rust:?}"
        );
    }

    for name in envoy_names.iter() {
        let allow_entry = allow_list.iter().find(|(n, _)| n.eq_ignore_ascii_case(name));
        let envoy_value = envoy
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        let rust_value = envoy_rust
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .unwrap_or("");

        match allow_entry {
            Some((_, AllowMode::NameRequired)) => {
                // Skip value comparison.
            }
            None => {
                if envoy_value != rust_value {
                    anyhow::bail!(
                        "header `{name}`: envoy=`{envoy_value}` envoy-rust=`{rust_value}`"
                    );
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveHttp1Result {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
```

For the `Driver::Http1` variant — find the existing `pub enum Driver { ... }` and add the variant per the SPEC §3 D5 grammar. The `expectations.yaml` parsing helpers (in `lib.rs`) need the new variant in the deserialization path. Phase-02.2 / phase-03.1 introduced custom serde `Deserialize` for `Driver` keyed on the `kind:` field; extend that deserializer with a new `"http1"` arm. Concretely:

```rust
// (assuming the existing Driver derives or hand-rolls Deserialize)
// Add the variant and deserialize support; the kind discriminator is "http1"
// per the fixture 0007 expectations.yaml in Task 16.
```

If the existing deserializer is `#[derive(Deserialize)]` with `#[serde(tag = "kind", rename_all = "snake_case")]` or similar, adding the variant + the inner-struct fields is sufficient. If hand-rolled, add a parallel arm.

- [ ] **Step 5: Run the 3 tests to verify they pass.**

```bash
cargo test -p differential diff_headers_passes_set_equal_modulo_allow_list \
                          diff_headers_fails_on_value_diff_outside_allow_list \
                          diff_headers_fails_on_name_set_diff
```

Expected: 3 PASS.

- [ ] **Step 6: Run the full differential test suite to verify no regression.**

```bash
cargo test -p differential
```

Expected: all green. Existing tests continue to pass — the `Driver::Http1` variant is additive.

- [ ] **Step 7: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 8: Append a Task 13 section to PROGRESS.md.**

```markdown
## Task 13 — differential harness Http1 grammar (2026-04-27)

- Commit: <SHA>
- Change: added Driver::Http1 variant + HttpMethod + BodyRule + HeaderRule + AllowMode + HEADER_ALLOW_LIST const + diff_headers helper + DriveHttp1Result struct; 3 unit tests on diff_headers; httparse=1 dev-dep added.
- Verification: `cargo test -p differential` → +3 tests; workspace gate clean.
- Deviations: <document any — e.g., Driver::Http1 deserialize wiring, expectations.yaml parsing>.
```

- [ ] **Step 9: Commit.**

```bash
git add tests/differential/src/lib.rs tests/differential/Cargo.toml
git commit -m "phase 04.1: differential harness — Driver::Http1 grammar + diff_headers + 3 tests (task 13)"
```

---

### Task 14: Differential harness — `drive_http1` + `run_fixture` `Driver::Http1` dispatch

**Files:**
- Modify: `tests/differential/src/lib.rs` (add `drive_http1` async helper + `run_fixture` dispatch)
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Task 13 landed the data types. Task 14 adds the async I/O — `drive_http1` (write a request, parse the response) — and wires `run_fixture` to dispatch on `Driver::Http1`. Tested end-to-end via fixture 0007 in Task 16.

**Scope.** One async function (~80 LoC) + the `run_fixture` cascade extension. No new unit tests in this task — Task 13's 3 tests cover the diff path; the drive function is exercised by Task 16's Docker-gated test.

Per SPEC §3 D5 + §6 signpost 8.

- [ ] **Step 1: Add `drive_http1` to `tests/differential/src/lib.rs`.**

```rust
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub async fn drive_http1(
    addr: std::net::SocketAddr,
    method: &HttpMethod,
    path: &str,
    host: &str,
) -> anyhow::Result<DriveHttp1Result> {
    let mut stream = TcpStream::connect(addr).await?;
    let req_line = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        method.as_str(),
        path,
        host,
    );
    stream.write_all(req_line.as_bytes()).await?;

    let mut buf = Vec::with_capacity(4096);
    let read_timeout = Duration::from_secs(5);

    // Read headers until httparse signals Complete; then read Content-Length
    // body bytes.
    let (status, headers, headers_end, content_length) = loop {
        let n = tokio::time::timeout(read_timeout, async {
            let mut chunk = [0u8; 4096];
            let n = stream.read(&mut chunk).await?;
            buf.extend_from_slice(&chunk[..n]);
            Ok::<_, std::io::Error>(n)
        }).await??;
        if n == 0 {
            anyhow::bail!("unexpected EOF before headers complete");
        }
        let mut hp_headers = [httparse::EMPTY_HEADER; 64];
        let mut resp = httparse::Response::new(&mut hp_headers);
        match resp.parse(&buf)? {
            httparse::Status::Complete(headers_end) => {
                let status = resp.code.ok_or_else(|| anyhow::anyhow!("no status code"))?;
                let mut headers: Vec<(String, String)> = Vec::with_capacity(resp.headers.len());
                for h in resp.headers.iter() {
                    if h.name.is_empty() { continue; }
                    let value = std::str::from_utf8(h.value)
                        .map_err(|e| anyhow::anyhow!("invalid utf8 header value: {e}"))?
                        .to_string();
                    headers.push((h.name.to_string(), value));
                }
                let content_length = headers.iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, v)| v.parse::<usize>().ok())
                    .unwrap_or(0);
                break (status, headers, headers_end, content_length);
            }
            httparse::Status::Partial => continue,
        }
    };

    // Read remaining body bytes.
    while buf.len() < headers_end + content_length {
        let mut chunk = [0u8; 4096];
        let n = tokio::time::timeout(read_timeout, stream.read(&mut chunk)).await??;
        if n == 0 {
            if buf.len() < headers_end + content_length {
                anyhow::bail!(
                    "unexpected EOF before body complete: have {}, expected {}",
                    buf.len() - headers_end,
                    content_length,
                );
            }
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }

    let body = buf[headers_end..headers_end + content_length].to_vec();

    Ok(DriveHttp1Result { status, headers, body })
}
```

- [ ] **Step 2: Wire `Driver::Http1` into `run_fixture`.**

Find the existing `pub async fn run_fixture(...)` (introduced phase 02.2; extended phase 03.1 / 03.2). The dispatch cascade per SPEC §3 D5:

```rust
pub async fn run_fixture(fixture_dir: &std::path::Path) -> anyhow::Result<()> {
    // ... existing read of expectations.yaml + per-fixture setup ...

    match driver {
        // ... existing Driver::TcpEcho / Driver::HttpGet / Driver::TlsTcp / Driver::TlsTcpProbeList arms ...

        Driver::Http1 { method, path, host, expected_status, expected_body, expected_headers } => {
            // (a) Drive against upstream Envoy in container.
            let envoy_result = drive_http1(envoy_addr, &method, &path, &host).await?;
            // (b) Drive against envoy-rust subject subprocess.
            let rust_result = drive_http1(subject_addr, &method, &path, &host).await?;

            // (c) Apply equivalence rules per the expectations.
            // Status (Row 1).
            if matches!(equivalence.response_status, EquivalenceMode::Exact) {
                if envoy_result.status != rust_result.status {
                    anyhow::bail!(
                        "status: envoy={} envoy-rust={}",
                        envoy_result.status, rust_result.status
                    );
                }
            }
            if let Some(es) = expected_status {
                anyhow::ensure!(
                    envoy_result.status == es && rust_result.status == es,
                    "expected_status={es}, envoy={}, envoy-rust={}",
                    envoy_result.status, rust_result.status,
                );
            }

            // Body (Row 2).
            if matches!(equivalence.response_body, BodyEquivalence::ByteExact) {
                if envoy_result.body != rust_result.body {
                    anyhow::bail!("body differs (byte_exact)");
                }
            }
            if let Some(BodyRule::ByteExact(expected)) = &expected_body {
                anyhow::ensure!(envoy_result.body == *expected, "envoy body != expected");
                anyhow::ensure!(rust_result.body == *expected, "envoy-rust body != expected");
            }

            // Headers (Row 3).
            if let Some(HeaderRule::SetEqualModuloAllowList) = expected_headers {
                diff_headers(&envoy_result.headers, &rust_result.headers, HEADER_ALLOW_LIST)?;
            } else if matches!(
                equivalence.response_headers.as_ref().map(|r| r.rule.as_str()),
                Some("set_equal_modulo_allow_list")
            ) {
                diff_headers(&envoy_result.headers, &rust_result.headers, HEADER_ALLOW_LIST)?;
            }

            Ok(())
        }
    }
}
```

The exact field names (`equivalence.response_status`, `equivalence.response_body`, etc.) match phase-02.2 / phase-03.1 shape; verify and adapt at execution time. The `expectations.yaml` schema for `equivalence.response_headers: { rule: set_equal_modulo_allow_list }` may need a small struct addition in the existing `Equivalence` shape — Task 13 already declared `HeaderRule` as a top-level type; the `Equivalence` shape grows a `response_headers: Option<HeaderRule>` (or similar) field if it doesn't already.

For fixtures 0007's `payload.bin` — `Driver::Http1` does not consume `payload.bin` in 04.1 (the request bytes are constructed from `method` / `path` / `host`). The empty `payload.bin` is a placeholder for forward-compat with 04.3's body-bearing requests. `run_fixture` ignores `payload.bin` for `Driver::Http1` in 04.1.

For fixture 0007's lack of `{{BACKEND_PORT}}` / `{{TLS_BACKEND_PORT}}` substitution — the existing `run_fixture` cascade detects template tokens to decide whether to spawn a backend. The `Driver::Http1` arm does not spawn a backend in 04.1; the existing detection cascade naturally handles this (no `{{BACKEND_PORT}}` in fixture 0007's templates → no backend spawn). Verify at execution time.

- [ ] **Step 3: Run the existing differential test suite to verify no regression.**

```bash
cargo test -p differential --lib
```

Expected: all green. The new code paths are unreached without a `Driver::Http1` fixture (which lands in Task 16).

- [ ] **Step 4: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 5: Append a Task 14 section to PROGRESS.md.**

```markdown
## Task 14 — differential harness drive_http1 + run_fixture dispatch (2026-04-27)

- Commit: <SHA>
- Change: added drive_http1 async helper (open TcpStream, write request, read until httparse complete + Content-Length consumed, return DriveHttp1Result); extended run_fixture with Driver::Http1 dispatch arm applying equivalence rules (status exact, body byte_exact, headers set_equal_modulo_allow_list).
- Verification: `cargo test -p differential` → all green; workspace gate clean.
- Deviations: <document any — e.g., Equivalence struct shape adjustments>.
```

- [ ] **Step 6: Commit.**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 04.1: differential harness — drive_http1 + run_fixture Http1 dispatch (task 14)"
```

---

### Task 15: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — populate `Header allow-list` table

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (replace the empty placeholder with the 2-row table from SPEC §2)
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Task 13 introduced the `HEADER_ALLOW_LIST` constant in the harness, sourced from BEHAVIOR_CONTRACT.md. ADR-0011 explicitly deferred response-header equivalence to "phase 04 (the first phase that lays out a real HCM)" — that deferral expires here. Task 15 closes ADR-0011's open loop by landing the table the constant claims to source.

**Scope.** Replace one line in BEHAVIOR_CONTRACT.md with a 2-row table per SPEC §2. No code changes.

- [ ] **Step 1: Verify the current state of `BEHAVIOR_CONTRACT.md`.**

```bash
grep -n -A 5 'Header allow-list' docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

Expected: at line ~27 the section header `## Header allow-list`, followed by a multi-line block-quoted preamble, followed by `_(empty; populated starting phase 04)_` at line 41.

- [ ] **Step 2: Replace `_(empty; populated starting phase 04)_` with the 2-row table.**

The exact replacement (preserving leading whitespace conventions):

```markdown
| Header | Equivalence | Rationale |
|---|---|---|
| `server` | name-required, value-may-differ | Implementation-identifying. Both proxies emit `server: <name>`; envoy-rust's HCM default is `server: envoy-rust`, Envoy's default is `server: envoy`. When HCM `server_name` config field is set (deferred to phase 05+ per parent SPEC §4), value tightens to exact-match on both sides. |
| `date` | name-required, value-may-differ | Wall-clock non-determinism (RFC 7231 §7.1.1.2 IMF-fixdate format). Both proxies stamp the response with the wall-clock at response-write time; values diverge because the two proxies write at slightly different instants. |
```

Use the Edit tool with `old_string = "_(empty; populated starting phase 04)_"` and the new table as `new_string`. The `_(empty; populated starting phase 04)_` placeholder is unique in the file — Edit succeeds without `replace_all`.

- [ ] **Step 3: Verify the replacement is well-formed.**

```bash
grep -B 2 -A 4 '^| Header' docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

Expected: the table renders as a 2-row markdown table immediately after the block-quoted preamble.

- [ ] **Step 4: Verify the harness `HEADER_ALLOW_LIST` constant matches the table.**

```bash
grep -A 4 'pub const HEADER_ALLOW_LIST' tests/differential/src/lib.rs
```

Expected: 2 entries (`server`, `date`); ordering matches the table.

- [ ] **Step 5: Append a Task 15 section to PROGRESS.md.**

```markdown
## Task 15 — BEHAVIOR_CONTRACT.md Header allow-list (2026-04-27)

- Commit: <SHA>
- Change: replaced `_(empty; populated starting phase 04)_` with the 2-row Header allow-list table (`server`, `date`); ADR-0011's deferral closes here. The harness `HEADER_ALLOW_LIST` constant (Task 13) is the in-code mirror of this table.
- Verification: harness constant + contract table are byte-identical in entry set + ordering.
- Deviations: <document any>.
```

- [ ] **Step 6: Commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 04.1: BEHAVIOR_CONTRACT.md — populate Header allow-list (server, date) (task 15)"
```

---

### Task 16: Fixture `0007-http1-direct-response` (5 files) + Docker-gated `tests/differential/tests/http1_direct_response.rs`

**Files:**
- Create: `tests/fixtures/0007-http1-direct-response/envoy.yaml`
- Create: `tests/fixtures/0007-http1-direct-response/envoy-rust.yaml`
- Create: `tests/fixtures/0007-http1-direct-response/inputs/payload.bin` (empty file)
- Create: `tests/fixtures/0007-http1-direct-response/expectations.yaml`
- Create: `tests/fixtures/0007-http1-direct-response/README.md`
- Create: `tests/differential/tests/http1_direct_response.rs`
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Why now:** Tasks 1–15 land everything needed to run a fixture end-to-end against both proxies. Task 16 is the differential gate.

**Scope.** Single fixture, single VH (`domains: ["*"]`), single route (`prefix: "/"`), `direct_response 200 inline_string "ok\n"`. Per SPEC §3 D6.

- [ ] **Step 1: Create `tests/fixtures/0007-http1-direct-response/envoy.yaml`.**

```yaml
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 0.0.0.0, port_value: {{PORT}} }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
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
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
```

- [ ] **Step 2: Create `tests/fixtures/0007-http1-direct-response/envoy-rust.yaml`.**

Per fixture-0003 precedent: bind `127.0.0.1`, no admin block.

```yaml
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 127.0.0.1, port_value: {{PORT}} }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
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

- [ ] **Step 3: Create `tests/fixtures/0007-http1-direct-response/inputs/payload.bin` as an empty file.**

```bash
mkdir -p tests/fixtures/0007-http1-direct-response/inputs
: > tests/fixtures/0007-http1-direct-response/inputs/payload.bin
```

- [ ] **Step 4: Create `tests/fixtures/0007-http1-direct-response/expectations.yaml`.**

```yaml
driver:
  kind: http1
  method: GET
  path: "/healthz"
  host: "envoy-rust.test"
  expected_status: 200
  expected_body:
    byte_exact: "ok\n"
  expected_headers:
    rule: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body: byte_exact
  response_headers:
    rule: set_equal_modulo_allow_list
```

- [ ] **Step 5: Create `tests/fixtures/0007-http1-direct-response/README.md`.**

```markdown
# Fixture 0007-http1-direct-response

This fixture drives a `GET /healthz` request through an HTTP/1.1 listener
configured with `envoy.filters.network.http_connection_manager`. The HCM
walks an inline `route_config` (single virtual_host with `domains: ["*"]`,
single route with `match: { prefix: "/" }`), and dispatches the matched
route's `direct_response` action: status `200`, body `inline_string: "ok\n"`.
No upstream cluster is touched.

The harness's `Driver::Http1 { method: GET, path: "/healthz", host:
"envoy-rust.test" }` opens a plaintext TCP connection to each proxy, writes
the request, reads until both the headers' CRLF terminator is seen AND
`Content-Length: 3` body bytes are consumed, asserts:

- Response status = 200 (Row 1, exact).
- Response body = `"ok\n"` (Row 2, byte_exact).
- Response header set is equal modulo the BEHAVIOR_CONTRACT.md allow-list
  (Row 3, set_equal_modulo_allow_list — `server` + `date` allowed to differ
  in value; all other headers — `content-length`, `content-type`, `connection`
  — value-exact).

Both proxies emit 5 response headers (`server`, `date`, `content-length`,
`content-type`, `connection`) per their respective HCM defaults. envoy-rust
emits `server: envoy-rust`; Envoy emits `server: envoy`. Both stamp `date:`
with the wall-clock at response-write time; the IMF-fixdate strings differ
slightly. `content-length` matches deterministically (`3`); `content-type`
matches (`text/plain`); `connection` matches (`keep-alive` — request did
not opt into `Connection: close`).

What is *out* of this fixture (each pinned to a later sub-phase or phase):

- HTTP route header matchers — sub-phase 04.2 (will amend this fixture's
  envoy.yaml + envoy-rust.yaml to add a second route with a `headers:`
  matcher demonstrating production matcher use).
- Upstream HTTP/1.1 origination — sub-phase 04.3 (fixture 0008 is the
  first fixture to proxy through to an upstream `http1-echo-server`).
- HTTP/2 / HTTP/3 — phases 05 and the QUIC family.
- HTTP filter chain (`Vec<Box<dyn HttpFilter>>` iteration protocol) —
  phase 07.
- Access logs, stats, Prometheus admin endpoint — phase 06.
- Multi-VH SNI matching with TLS — phase 05+ (HCM-with-TLS not exercised
  in 04.x; the listener filter chain has no `transport_socket`).
- HCM `server_name` config field (overrides the `server:` response header
  literal) — phase 05+; until then the BEHAVIOR_CONTRACT.md allow-list
  permits `server` to differ.

ADR references: ADR-0011 (response-header equivalence deferral closes here
via the BEHAVIOR_CONTRACT.md `Header allow-list` table populated at this
phase), ADR-0014 (`typed_config` deserialization), ADR-0020 (split phase 04
into 04.1 + 04.2 + 04.3).
```

- [ ] **Step 6: Create `tests/differential/tests/http1_direct_response.rs`.**

```rust
//! Phase 04.1 differential acceptance test: drive a GET /healthz through an
//! HCM-direct_response listener. Should produce identical (status, body,
//! header-set-modulo-allow-list) between upstream Envoy v1.33.0 and
//! envoy-rust. Docker-gated; in CI this runs on `ubuntu-latest` alongside
//! the phase-00 echo, phase-01 admin_ready, phase-02.2 tcp_proxy, and
//! phase-03 tls_* fixtures.

use std::path::PathBuf;

#[tokio::test]
async fn http1_direct_response_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0007-http1-direct-response");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
```

Per phase-03.1 / phase-03.2 precedent, this test is NOT marked `#[ignore]` — it runs unconditionally and fails fast if Docker is unavailable. CI provides Docker; local dev without Docker sees the same failure mode as the other acceptance tests.

- [ ] **Step 7: Run the Docker-gated test (locally if Docker is available).**

```bash
cargo test -p differential --test http1_direct_response
```

If Docker is available: expected to PASS — full end-to-end byte-exact differential through both proxies. If Docker is not available: expected to fail at upstream container start; same failure mode as the other acceptance tests.

If the test fails for a reason OTHER than "Docker not available," debug per `superpowers:systematic-debugging`. Common failure modes:

- **Envoy v1.33.0 rejects the inline `direct_response` shape.** The schema is well-established in v1.33.0 — this should not fire. If it does, cross-check the `@type` URL casing.
- **Envoy v1.33.0 rejects `admin.port_value: 0`.** Land an ADR introducing `{{ENVOY_ADMIN_PORT}}` per the established phase-02.2 / phase-03.1 contingency. Mirrors phase-03.1 contingency #2.
- **Header set differs by an unexpected name (e.g., Envoy emits `x-envoy-upstream-service-time` even on a direct_response path).** Cross-check at execution time. If true, add `x-envoy-upstream-service-time` to the BEHAVIOR_CONTRACT.md allow-list and to the harness `HEADER_ALLOW_LIST` constant — this is the natural early landing of the 04.3-projected entry. Document in PROGRESS.md.
- **`connection:` header value differs between proxies.** Envoy may emit `Keep-Alive: timeout=...` or similar — cross-check. If a value-difference is unavoidable, evaluate whether `connection` joins the allow-list (with the ADR landing here). Most likely both emit the same value for fixture 0007's request shape.
- **Header name case differs between proxies (e.g., Envoy emits `Server` mixed-case vs envoy-rust `server` lowercase).** The `diff_headers` helper does case-insensitive name matching (Task 13); this should not fire. If it does, the `synth_*` helpers in Task 10 may need to use canonical-case names matching Envoy's output.

- [ ] **Step 8: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

Expected: all four exit 0. The Docker-gated `http1_direct_response_fixture` is excluded by `--lib --bins` and runs only via `cargo test --workspace` (CI).

- [ ] **Step 9: Append a Task 16 section to PROGRESS.md.**

```markdown
## Task 16 — fixture 0007-http1-direct-response (2026-04-27)

- Commit: <SHA>
- Change: created tests/fixtures/0007-http1-direct-response/ (envoy.yaml + envoy-rust.yaml + inputs/payload.bin (empty) + expectations.yaml + README.md); created tests/differential/tests/http1_direct_response.rs Docker-gated acceptance test.
- Verification: `cargo test -p differential --test http1_direct_response` → PASS (Docker required); workspace gate clean.
- Deviations: <document any — e.g., admin.port_value adjustment if Envoy rejects 0; allow-list addition if Envoy emits x-envoy-upstream-service-time>.
```

- [ ] **Step 10: Commit.**

```bash
git add tests/fixtures/0007-http1-direct-response tests/differential/tests/http1_direct_response.rs
git commit -m "phase 04.1: fixture 0007-http1-direct-response + Docker-gated test (task 16)"
```

---

### Task 17: State 4 phase-done gate

**Files:**
- Modify (append): `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`

**Per `docs/envoy-rust/SKILL_ROUTING.md` state 4.** Run the full local stable-toolchain gate, observe both CI jobs (build+test+lint, fuzz), quote outputs into PROGRESS.md. The plan does not advance ROADMAP.md or STATE.md here — those flip in state 6 (the phase-done commit), not now (BOOTSTRAP_PROMPT.md §5.1: one state per session).

If the gate exposes `Cargo.lock` drift (typical with the new `envoy-http1` workspace member + `httparse` / `bytes` graph touches), land a dedicated `phase 04.1: sync Cargo.lock with phase 04.1 dep graph` commit immediately following Task 17's progress note. Phase-01 precedent: `4955252`. Phase-02.1 precedent: `dea4d16`. Phase-02.2 precedent: `2146014`. Phase-03.1 precedent: `eb039e6`. Phase-03.2 precedent: `85685a3`.

- [ ] **Step 1: Run the local stable-toolchain gate, capturing each command's output.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
cargo deny check
```

Expected: all five exit 0. Quote tails into PROGRESS.md.

The `cargo test --workspace --lib --bins` count expands from phase 03.2's tally:
- `envoy-config`: previous tally + Task 1 (5–6 tests) + Task 2 (8 tests) = previous + 13–14.
- `envoy-cluster`: unchanged.
- `envoy-listener`: unchanged.
- `envoy-tcp`: unchanged.
- `envoy-tls`: unchanged.
- `envoy-bin`: previous tally + Task 12 (1 integration test) = previous + 1 (integration test count, not lib-test count).
- `envoy-http1` (NEW): Task 6 (2) + Task 7 (2) + Task 8 (5) + Task 9 (2) + Task 10 (6) = 17 tests.
- `tcp-echo-server`: unchanged.
- `tls-echo-server`: unchanged.
- `differential` lib: previous tally + Task 13 (3 tests) = previous + 3. Docker-gated integration tests now total 7 (`echo`, `admin_ready`, `tcp_proxy`, `tls_downstream`, `tls_upstream`, `tls_sni`, `http1_direct_response`).

- [ ] **Step 2: Trigger CI and observe both jobs.**

After committing all task commits, push the branch and observe the CI run:

```bash
git push origin <branch>
gh run list --workflow=ci.yml -L 1
gh run watch <run-id>
```

Expected: both `build + test + lint` (now also runs `http1_direct_response_fixture`) and `fuzz (parse_bootstrap, 30s)` jobs succeed. The fuzz job exercises the extended `parse_bootstrap` corpus (2 new HCM seeds) automatically.

- [ ] **Step 3: If `Cargo.lock` is dirty, land a dedicated sync commit.**

```bash
git status
git diff Cargo.lock | head -50
git add Cargo.lock
git commit -m "phase 04.1: sync Cargo.lock with phase 04.1 dep graph"
```

The diff should add a `[[package]]` stanza for `envoy-http1 v0.0.0` plus update transitive entries for `httparse` (already in tree via envoy-bin's admin) and `bytes` (already in tree via tokio's transitives). Verify by `git diff` review before staging that no version regressed on existing direct deps and no surprising new transitive landed.

- [ ] **Step 4: Append the State-4 section to PROGRESS.md.**

Use the phase-03.2 PROGRESS State-4 section as the precedent shape. Quote the local-gate command outputs (per-crate test tails are the most informative), the CI run number + URL, and document any fix-during-gate commits (the goal is zero — phase 03.2 cleared on first attempt).

```markdown
## Task 17 / State 4 — phase-done gate verification (2026-04-27)

Per `docs/envoy-rust/SKILL_ROUTING.md` state 4: the local stable-toolchain gate ran clean on first attempt. ROADMAP.md and STATE.md are NOT advanced here per `BOOTSTRAP_PROMPT.md` §5.1 (one state per session); those flip in state 6 (the phase-done commit) after state 5's `REVIEW.md` is approved.

### Local stable-toolchain gate

`cargo build --workspace --all-targets`:
```
<tail>
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
<tail>
```

`cargo fmt --all -- --check`:
```
(no output — clean)
```

`cargo test --workspace --lib --bins`:
```
<per-crate tails>
```

Total: <N> tests, 0 failed, <ignored>.

`cargo deny check`:
```
advisories ok, bans ok, licenses ok, sources ok
```

### Cargo.lock sync

<note: dirty/clean; if dirty, the SHA of the dedicated sync commit>

### Outstanding for state 5/6

State 5 (`superpowers:requesting-code-review`) writes `REVIEW.md` for this phase. State 6 (the phase-done commit) flips ROADMAP row `04.1` `status` → `done` (parent row `04` stays `in-progress` until 04.3 lands per the schema) and advances STATE.md to phase `04.2-route-matchers` (lifecycle state 2; SPEC.md exists from the parent-04 state-2 split commit `1d9740d`, PLAN.md does not; next-skill `superpowers:writing-plans`).
```

- [ ] **Step 5: Commit the PROGRESS update.**

```bash
git add docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md
git commit -m "phase 04.1: state-4 phase-done gate verification (task 17)"
```

State 4 verification complete. Next session enters state 5 via `superpowers:requesting-code-review` (writing `REVIEW.md`); state 6 then ships the phase-done commit per `BOOTSTRAP_PROMPT.md` §5.3, flipping ROADMAP row `04.1` to `done` and advancing STATE.md to phase `04.2-route-matchers` at lifecycle state 2 with next-skill `superpowers:writing-plans`.

---

## Out-of-plan execution contingencies

These are NOT plan steps; they are decision rules for situations the SPEC and plan jointly anticipate but cannot pin at planning time. Per D-3.5, execution lands an ADR and proceeds when any trigger fires.

1. **Envoy v1.33.0 emits `x-envoy-upstream-service-time` even on direct_response paths.** SPEC §2 + parent-SPEC §2 anticipate this header lands at 04.3 (when proxied responses ship). If Envoy emits it on direct_response too — cross-check at Task 16 execution — add the row to BEHAVIOR_CONTRACT.md and the harness `HEADER_ALLOW_LIST` early; document in PROGRESS.md. No new ADR — the contract update is the artifact.

2. **Envoy v1.33.0 rejects `admin.port_value: 0` in fixture 0007.** Land an ADR introducing `{{ENVOY_ADMIN_PORT}}` per the established phase-02.2 / phase-03.1 contingency. Mirrors phase-02.2 fixture 0003 + phase-03.1 fixture 0004 fallback.

3. **Hand-rolled IMF-fixdate writer in Task 7 produces output that differs from Envoy's `date:` value format.** Both should target RFC 7231 §7.1.1.1; the value is allow-listed anyway, so a one-character format drift would not fail the differential (Row 3 set_equal_modulo_allow_list). The per-character match still matters for envoy-rust's own integration test in Task 12 — if a divergence is found at Task 7's tests, debug; if found at Task 12, the integration test's regex / contains-check is permissive enough that a small format drift likely doesn't matter. SPEC §6 signpost 5 anticipates this.

4. **`cargo deny check` flips red on a new transitive surface.** Most likely a no-op since `httparse` is already in tree via envoy-bin's admin and `bytes` is already in tree via tokio's transitives. If a non-trivial extension surfaces, update `deny.toml` per ADR-0005's discipline.

5. **`envoy-config`'s `RouteConfiguration` / `VirtualHost` / `Route` / etc. types are not `Clone`-derivable cleanly.** Task 10's `clone_route_config` helper is the workaround. If the types CAN derive `Clone` (no internal `Arc<dyn Trait>` or similar), prefer that and drop the helper.

6. **`envoy-listener::ConnectionHandler` trait shape differs from Task 10's plan-time expectation.** The trait was introduced phase 02.2 / generalized phase 03.1. Verify the actual shape and adapt at execution time; no ADR.

7. **Pipelined requests on a kept-alive connection break Task 10's per-iteration buffer slicing.** The `serve_connection` loop's `buf.advance(consumed)` + body drain is the critical path. If a fast client pipelines two requests in one TCP segment, the codec will return Some on the first parse, the body drains, and the loop re-parses — the second request's bytes are already in `buf`. Verify under unit test if concerns surface; Task 10's `first_match_wins_on_routes` is single-request and doesn't exercise pipelining.

8. **Envoy emits headers in mixed case (e.g., `Server` capitalized, `Content-Type` capitalized).** envoy-rust emits in canonical lowercase form per `headers::*` constants. The `diff_headers` helper does case-insensitive name comparison, so this is not a fixture-failure mode — but the integration test in Task 12 may use case-sensitive `contains("server: envoy-rust\r\n")` checks; cross-check and adjust to case-insensitive contains-or-regex.

9. **A task's scope balloons past ~10 sub-steps.** Invoke `superpowers:systematic-debugging` before splitting. Phase 04.1 has already been split (it's a sub-phase of 04); a nested split is not anticipated and deserves root-cause analysis (scope creep vs. planner overdecomposition), per SPEC §5 closing paragraph.

10. **ADR numbering shifts.** The plan does not anticipate any new ADRs in 04.1 (per SPEC §7). If any new ADR lands during execution before Task 1 (very unlikely — STATE.md confirms HEAD is `1d9740d`), renumber any references in this PLAN at the relevant task.

11. **The `parse_then_validate` helper or `make_hcm_listener_yaml` helper already exists in `bootstrap.rs::tests`.** Reuse rather than duplicate. The phase-02.1 and phase-03.1 plans referenced their own helpers; if a similar helper exists, the test bodies in Task 2 should call the existing one rather than declare a duplicate.

12. **Task 1's `DataSource.filename: String → Option<String>` change ripples into `envoy-tls` callsites.** Phase 03.1 introduced TLS callsites that consume `ds.filename`; Task 1 Step 5 anticipates an `as_deref().expect()` form, but the actual ripple may be a small refactor in `envoy-tls` to take `&str` parameters instead of `&DataSource`. Whichever shape is cleanest at execution time is acceptable.

13. **Task 13's `Driver::Http1` deserialization fails on `expectations.yaml` because the existing serde shape is hand-rolled and the new `kind: http1` arm needs explicit handling.** Mirror the `kind: tls_tcp` / `kind: tcp_echo` arms; same dispatch shape.

14. **`tests/differential/tests/http1_direct_response.rs` needs the `differential::*` re-exports.** If `Driver::Http1`, `BodyRule`, `HeaderRule`, etc. are not re-exported from the crate root, add `pub use` statements to `lib.rs` so the integration test can construct expectations.yaml-equivalents inline if needed. The test as written in Task 16 just calls `run_fixture` and doesn't construct types directly, so this is unlikely to fire.

15. **Task 10's `serve_connection` accumulates the buffer past 8 KiB during a long pipelined session.** The `Http1Codec::parse_request` cap fires at 8 KiB on the headers section of a single request; once a request is parsed, `buf.advance(consumed)` shrinks the buffer so the cap-check applies fresh on the next request. Verify the loop's `buf.advance` ordering; the SPEC §3 D3 pseudocode has been adapted for in-buffer pipelining.

---

## Final commit message format (state 6 — NOT this state)

The state-6 phase-done commit shape, per SPEC §9. Do NOT land this commit during plan execution; it lands at state 6 (after REVIEW.md is approved at state 5):

```
phase 04.1: HTTP/1.1 codec + HCM scaffold + direct_response + fixture 0007 [ADR-0020]

New library crate envoy-http1 owns the workspace's runtime dependency on
httparse (per ADR-0020's parent-04 split): Http1Codec parses HTTP/1.1
requests; Http1Response writes Content-Length-framed responses; HCM is a
ConnectionHandler impl that walks an inline RouteConfiguration
(multi-VirtualHost; domains: ["*"] or exact-string match against Host:;
multi-route per VH with prefix:/path: matchers; first-match-wins) and
dispatches the matched route's direct_response action through a hardcoded
router-filter call site that emits server/date/content-length/content-type/
connection headers. envoy-config grows the HttpConnectionManager TypedConfig
variant + RouteConfiguration schema + DirectResponse + DataSource.inline_string
extension with 8 new validator tests and 2 fuzz-corpus seeds. envoy-bin's
listener-walk gains an HCM dispatch arm. New differential harness
Driver::Http1 + drive_http1 + HEADER_ALLOW_LIST + diff_headers; fixture
0007-http1-direct-response lands green end-to-end. BEHAVIOR_CONTRACT.md's
Header allow-list table receives its first two entries (server, date) per
parent SPEC §2.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (unchanged);
  tests/fixtures/0004-tls-downstream green (unchanged);
  tests/fixtures/0005-tls-upstream green (unchanged);
  tests/fixtures/0006-tls-sni green (unchanged);
  tests/fixtures/0007-http1-direct-response green (HTTP/1.1 listener;
  direct_response 200 inline_string body; single-VH single-route prefix-match;
  set-equal-modulo-allow-list response header diff with server + date allowed
  to differ).
Conformance: none.
```

The state-6 commit also flips:
- `docs/envoy-rust/ROADMAP.md` row `04.1` `status` → `done`. (Row `04` parent stays `in-progress`; flips at 04.3's final commit per the schema invariant.)
- `docs/envoy-rust/STATE.md` → active id `04.2`, slug `04.2-route-matchers`, lifecycle state 2 (SPEC.md exists from parent-04 state-2 commit `1d9740d`, PLAN.md does not), next-skill `superpowers:writing-plans`.
- Appends a final State-6 section to PROGRESS.md.
