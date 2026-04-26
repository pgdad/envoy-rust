# Phase 04 — HTTP connection manager (HTTP/1.1) + route match + router filter + direct_response

- **Phase id:** `04`
- **Slug:** `04-http1`
- **Title:** HTTP/1.1 data plane: codec library + HCM network filter + RouteConfiguration schema + router HTTP filter (hardcoded) + `direct_response` action + upstream HTTP/1.1 origination
- **Depends on:** `03` (TLS — both downstream termination and upstream origination + multi-cert SNI). Phase 03 ROADMAP row is `done` as of commit `ca81226`; phase 04 enters `in-progress` at this state-1 close-out commit.
- **Seeded by:** `BOOTSTRAP_PROMPT.md` §8 row 04 — "HTTP connection manager (HTTP/1.1) + route match + router filter + direct_response".
- **Differential surface when done:** two new fixtures green against upstream `envoyproxy/envoy:v1.33.0`:
  - `tests/fixtures/0007-http1-direct-response/` — HTTP/1.1 listener; `direct_response` route action returning a static 200 body with `Content-Length` framing. No upstream cluster touched.
  - `tests/fixtures/0008-http1-router-upstream/` — HTTP/1.1 listener; router filter proxies `GET /` through to a new in-tree `http1-echo-server` helper; response byte-exact (modulo header allow-list).
  Pre-existing fixtures `0001-tcp-echo`, `0002-static-admin-ready`, `0003-tcp-proxy`, `0004-tls-downstream`, `0005-tls-upstream`, `0006-tls-sni` remain green.
- **Sub-phases:** **`04.1`, `04.2`, `04.3`** (per the split decision projected in §5; codified at parent-04 state-2 via **ADR-0020**).

This SPEC is the design contract for the parent phase 04. It projects the split into three sub-phases by surface boundary (codec/HCM/direct_response → matcher fan-out → upstream proxying) — chosen over the alternative two-way split-by-traffic-direction shape because the matcher fan-out alone (all 7 `HeaderMatcher` modes + `StringMatcher` + `invert_match` + the new `regex` foundation) was sized at ~1300 LoC, which would have pushed a two-way 04.1 over the §6.1 split-gate. The 3-way split is unusual and was actively considered against the alternative of nested-splitting 04.1 → 04.1.1 / 04.1.2 (per `BOOTSTRAP_PROMPT.md` §6.1 the nested-split warrants `superpowers:systematic-debugging` first); the 3-way flat split avoids the nesting anti-pattern at the cost of one extra sub-phase row in the ROADMAP.

This SPEC is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed phase-03 surface (via `git log` and the in-tree `envoy-tls` / `envoy-config` / `envoy-tcp` / `envoy-bin` shape at HEAD `ca81226`) must be able to operate as the parent-04 state-2 session — landing ADR-0020, the renumbering note, and the three sub-phase SPECs. Each sub-phase then enters its own state 3 with its own SPEC + PLAN cadence.

---

## 1. Goal and acceptance signal

**Goal.** Land HTTP/1.1 on the data plane in three coordinated layers:

1. **Codec library + HCM scaffold + minimal routing** (sub-phase 04.1). New workspace member `envoy-http1` (mirrors `envoy-tls`'s sole-dep-owner shape — `httparse` is the only crate-direct dep beyond the std-lib + tokio + bytes + thiserror). HCM is a new network filter (sibling of `tcp_proxy`); per-connection it parses HTTP/1.1 requests, walks `route_config` (multi-VirtualHost; `domains: ["*"]` or exact match; first-match-wins), dispatches the matched route's `direct_response` action through a hardcoded router-filter call site (no chain framework — that's phase 07). Fixture `0007-http1-direct-response` proves the round-trip byte-exact (modulo header allow-list) against upstream Envoy v1.33.0. Plaintext only (TLS + HCM combinations work via phase 03's `TlsAcceptingHandler` adapter but are not exercised in 04.x fixtures).

2. **Header matcher fan-out** (sub-phase 04.2). Adds all 7 of Envoy's `HeaderMatcher` modes (`exact_match`, `prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`, `present_match`, `string_match` via the modern generic tagged-union) plus `invert_match: bool`. `safe_regex_match` and `string_match.safe_regex` require a new permitted foundation: `regex = "1"` lands under **ADR-0021** at 04.2 Task 1, scoped narrowly to header / route matching only. NO new fixture (matchers are config-side; the differential property is exercised by 04.1's fixture 0007's existing route-match path plus ~25 unit tests + a `parse_bootstrap` fuzz seed extension). 04.2 also adds a header-matcher-bearing route to fixture 0007 (envoy-side YAML edit — same on both sides — to prove a non-trivial matcher actually selects the route in production).

3. **Upstream HTTP/1.1 origination + router proxy arm + fixture 0008** (sub-phase 04.3). New `envoy-http1::Client` (per-connection HTTP/1.1 client; no pooling — pooling is upstream-robustness-family territory). Router filter's `Route(RouteAction_Route)` arm wires through to dial the cluster's picked endpoint, forward the request, and write the upstream's response back to the downstream. New helper crate `tests/helpers/http1-echo-server` (sibling of `tcp-echo-server` / `tls-echo-server`). Fixture `0008-http1-router-upstream` proves `GET /` proxied through to the helper round-trips byte-exact. The phase-03.2 M1 carryforward (`Cluster::name()` accessor) is evaluated opportunistically here — if the per-cluster proxy attribution wants a typed accessor, it closes; otherwise it forwards unchanged to phase 06.

Across all three sub-phases, the architectural rule is **`envoy-http1` is the SOLE workspace dep on `httparse`** — no other crate calls `httparse::Request::parse` directly. This mirrors how `envoy-tls` is the sole dep on `rustls`.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to phase 04's full feature surface:

- (a) the new differential fixtures `tests/fixtures/0007-http1-direct-response/` and `tests/fixtures/0008-http1-router-upstream/` are green;
- (b) the pre-existing differential fixtures `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/` remain green;
- (c) no conformance suites run this phase (first one — `h2spec` — attaches in phase 05);
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 04.1 + 04.2 + 04.3 (≥ 3 new HCM/route_config/direct_response/HeaderMatcher seeds);
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- (f) all three sub-phase `REVIEW.md` verdicts are approved.

The parent-phase-done commit lands at the **last sub-phase's state-6 commit** (i.e., 04.3's phase-done commit also flips parent row `04` from `in-progress` to `done` — mirrors phase 03's `ca81226`-shape close-out where the 03.2 commit also closed parent 03).

---

## 2. Behavior-contract scope for phase 04

Phase 04 is the first phase to introduce a new HTTP response header surface — every prior phase exercised either no headers (TCP-only fixtures 0001 / 0003 / 0004 / 0005 / 0006) or a single trivial admin response (fixture 0002 carried `Driver::HttpGet` against `/ready` and tolerated header divergence under ADR-0011). ADR-0011 explicitly deferred response-header equivalence to "phase 04 (the first phase that lays out a real HCM)"; that deferral expires here.

The currently-empty `BEHAVIOR_CONTRACT.md` `Header allow-list` section gets populated in 04.1 with two entries (`server`, `date`); 04.2 adds none; 04.3 adds one (`x-envoy-upstream-service-time`). The full table at parent-04 done:

| Header | Equivalence | Rationale |
|---|---|---|
| `server` | name-required, value-may-differ | Implementation-identifying. Both proxies emit `server: <name>`; envoy-rust default is `server: envoy-rust`, Envoy default is `server: envoy`. When HCM `server_name` config field is set (deferred to phase 05+), value tightens to exact-match on both sides. |
| `date` | name-required, value-may-differ | Wall-clock non-determinism (RFC 7231 §7.1.1.2 IMF-fixdate). |
| `x-envoy-upstream-service-time` | name-required, value-may-differ | Per-request upstream-side latency (ms). Only present on responses that proxied through to an upstream cluster (NOT `direct_response` paths). Both proxies emit; values diverge by measurement. Lands in 04.3. |

Headers NOT on the allow-list (= must be value-exact when present):
- `content-length` — deterministic for static-body responses (04.1) and for echoed responses where the upstream backend emits a deterministic body (04.3's `http1-echo-server` returns a deterministic shape per request).
- `content-type` — controlled by `direct_response` config (default `text/plain` per Envoy v1.33.0); for proxied responses, controlled by the upstream backend.
- `connection` — value-exact (`keep-alive` or `close`; driven by request's `Connection:` header per HTTP/1.1 §6.1; the HCM honors the request's connection posture).
- All other response headers Envoy emits (none currently anticipated for the four fixtures touched in phase 04; future fixtures may extend the allow-list).

**Equivalence-matrix dimensions touched** (no contract changes — just first-time usage of pre-existing rows):

- Row 1 (Response status): exercised for the first time on a non-`/ready`-admin path. Fixtures 0007 + 0008 both opt in via `equivalence.response_status` in their `expectations.yaml`.
- Row 3 (Response headers): exercised for the first time at all. Set-equal modulo allow-list (above). Both fixtures opt in.
- Row 2 (Response body): byte-exact for `direct_response` (static `inline_string` body); byte-exact for 0008 (echoed by `http1-echo-server` — the echo response is deterministic in its construction).

The `assert_equivalence` helper in `tests/differential/src/lib.rs` grows a header-set diff helper in 04.1 (`fn diff_headers(envoy: &[(String, String)], envoy_rust: &[(String, String)], allow_list: &HeaderAllowList) -> Result<()>`) that compares names-must-match-set-equal and values-must-match-when-not-allow-listed. The allow-list is a static `&[(&str, AllowMode)]` constant populated from the BEHAVIOR_CONTRACT.md table above; updates to the contract update the constant in lockstep.

---

## 3. Deliverables (organized by sub-phase)

This section enumerates the ~12 deliverables across the three sub-phases. Each sub-phase's own SPEC (written at parent-04 state-2 via the split commit) will expand its own deliverables into the per-task PLAN cadence the project follows.

### Phase 04.1 — codec + HCM + minimal routing + direct_response + fixture 0007

**D1.1 — `crates/envoy-http1/` (new workspace member).** Sole-dep-owner crate for HTTP/1.1 codec primitives. Public surface: `Http1Codec` (request parser via `httparse`), `Http1Response` (response writer; `Content-Length`-only body framing in 04.1), `Request` and `Response` value types (owned `String` headers preserving emission order; case-insensitive header lookup helper), `Http1Error` typed-error enum (~6 variants: `MalformedRequestLine`, `MalformedHeader`, `HeadersTooLarge`, `BodyTooLarge`, `UnexpectedEof`, `Io`). Cargo deps: `httparse = "1"` (already in workspace via envoy-bin's admin endpoint; this is the first time it's a runtime dep on a non-bin crate); `bytes = "1"` (per D-3.2 permitted; for zero-copy buffer mgmt); `tokio` (rt + io-util + macros); `thiserror = "2"`; `tracing = "0.1"`. Crate root `lib.rs:1` carries `#![forbid(unsafe_code)]`. ~200 LoC impl + ~150 LoC unit tests covering request-parse happy paths, malformed inputs, header-cap enforcement, and the response writer's wire format.

**D2.1 — `envoy-config` schema additions for HCM + route_config.** `TypedConfig` enum gains a new variant `HttpConnectionManager(HttpConnectionManagerConfig)` keyed on `@type: type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager` (sibling of phase 02.1's `TcpProxy`). New types in `crates/envoy-config/src/bootstrap.rs`:

- `HttpConnectionManagerConfig` — 4 required fields: `stat_prefix: String` (carried for forward-compat with phase 06 stats; ignored at runtime in 04.x), `codec_type: CodecType`, `route_config: RouteConfiguration`, `http_filters: Vec<HttpFilter>`. `#[serde(deny_unknown_fields)]`.
- `CodecType` enum: `AUTO`, `HTTP1`. `HTTP2` and `HTTP3` reject with `ConfigError::UnsupportedCodecType`.
- `HttpFilter` — `name: String` + `typed_config: HttpFilterTypedConfig`.
- `HttpFilterTypedConfig::Router(RouterConfig)` — only variant in 04.1; tagged on `@type: type.googleapis.com/envoy.extensions.filters.http.router.v3.Router`. `RouterConfig` is an empty struct (Envoy's Router has many fields, all deferred). Validator rejects any other `@type` with `ConfigError::UnsupportedHttpFilter`.
- `RouteConfiguration` — `name: String` + `virtual_hosts: Vec<VirtualHost>` (cardinality ≥ 1; validator rejects empty).
- `VirtualHost` — `name: String` + `domains: Vec<String>` (cardinality ≥ 1; each domain is either `"*"` or a literal hostname; case-insensitive match against request `Host:` header) + `routes: Vec<Route>` (cardinality ≥ 1).
- `Route` — `match: RouteMatch` + `direct_response: DirectResponse` (in 04.1; 04.3 adds `route: RouteAction_Route` variant).
- `RouteMatch` — oneof `prefix: String` or `path: String` (in 04.1; 04.2 adds `headers: Vec<HeaderMatcher>`).
- `DirectResponse` — `status: u16` (1xx-5xx; validator rejects out-of-range with `ConfigError::InvalidStatusCode`) + `body: DataSource` (only `inline_string` form accepted in 04.1; `filename` / `inline_bytes` / `environment_variable` forms reject with `ConfigError::UnsupportedDataSource` — the existing phase-03 `DataSource` struct is reused; the `inline_string` field is a 04.1 addition).

Validator extensions: `UnsupportedCodecType`, `UnsupportedHttpFilter`, `UnsupportedRouteMatcher`, `UnsupportedDomainMatcher`, `EmptyVirtualHosts`, `EmptyRoutes`, `InvalidStatusCode`, `UnsupportedDataSource`, plus per-field `deny_unknown_fields` regression guards. ~250 LoC schema + ~120 LoC validator + ~25 unit tests + 2 fuzz-corpus seeds.

**D3.1 — HCM as a network filter.** Implements `envoy_listener::ConnectionHandler` (sibling of `envoy_tcp::TcpProxy`). Per-connection state machine: parse request via `Http1Codec`; if request body is `Content-Length`-framed, IGNORE the body (request-body drain logic is deferred to 04.3 — fixture 0007 is `GET /healthz` with no body, so 04.1 doesn't exercise drain; the drain becomes load-bearing only when 04.3 starts forwarding bodies upstream); resolve route via `route_config` walk (first-match-wins on `VirtualHost.domains` against request `Host:` header — per HTTP/1.1 §5.4 the `Host:` header is mandatory; absent or malformed `Host:` produces `400 Bad Request`; then first-match-wins on `Route.match` within VH); dispatch action via hardcoded router invocation (no `Vec<Box<dyn HttpFilter>>` chain — that's phase 07); for `direct_response`, write Status + headers (`server: envoy-rust`, `date: <IMF-fixdate>`, `content-length: <body.len()>`, `content-type: text/plain`, `connection: keep-alive` or `close` per request's `Connection:`) + body via `Http1Response` writer. Connection lifecycle: HTTP/1.1 keep-alive default; if request carries `Connection: close`, response also carries `Connection: close` and the socket closes after the response is fully written; idle-connection 5s timeout reading next request line; HCM `idle_timeout` config knob deferred (HCM only accepts the 4 fields enumerated in D2.1). HCM crate placement TBD at sub-phase 04.1 SPEC writeup; current lean is `crates/envoy-http1/` (codec + per-connection state machine in one crate; envoy-bin just wires it; mirrors how `envoy-tcp::TcpProxy` lives in envoy-tcp not envoy-bin). ~250 LoC + ~10 unit tests covering route resolution, header generation, lifecycle.

**D4.1 — `envoy-bin` wiring.** New `TypedConfig` dispatch arm for `HttpConnectionManager` (sibling of `TcpProxy` arm). Per-listener filter-chain pre-pass: when first chain's first filter is HCM, build the per-listener HCM handler, optionally wrap in `TlsAcceptingHandler` per phase 03 if the chain has `transport_socket`, hand to `envoy_listener::Listener::bind`. NO upstream cluster reference at HCM-time — clusters are still managed by `cluster_mgr` from phase 02.1 but only consulted in 04.3 by the router's proxy arm. ~80 LoC. New in-process integration test `crates/envoy-bin/tests/http1_direct_response.rs` (Docker-free; spawns envoy-bin subprocess via `CARGO_BIN_EXE_envoy-bin`; drives a single `GET /healthz` HTTP/1.1 request via `tokio::net::TcpStream` + manual request bytes; reads response via `httparse`; asserts status + body + headers). ~120 LoC.

**D5.1 — Differential harness extensions for HTTP/1.1 + fixture 0007.**

- `Driver::Http1 { method: HttpMethod, path: String, host: String, expected_status: Option<u16>, expected_body: Option<BodyRule>, expected_headers: Option<HeaderRule> }` — new variant on the existing `Driver` enum in `tests/differential/src/lib.rs`. (Phase 01's `HttpGet { path, host }` variant is admin-specific — keeps targeting the admin port and validating against admin's response shape; the new `Http1` variant is HCM-aware.)
- `drive_http1` async helper — sibling of `drive_tcp` / `drive_tls`. Opens a TCP connection to the listener; writes a serialized HTTP/1.1 request constructed from the driver's `method` + `path` + `host` (and any future fields); reads the response by parsing the status line + headers via `httparse`, then reading exactly `Content-Length` body bytes (no chunked support in 04.1 — fixtures don't exercise it; 04.3 adds chunked-response reader if upstream emits it). Returns `(StatusCode, Vec<(String, String)>, Vec<u8>)`.
- `assert_equivalence` helper grows a `diff_headers` extension that compares names-set-equal modulo allow-list, values-exact-when-not-allow-listed. The allow-list is a static `HEADER_ALLOW_LIST: &[(&str, AllowMode)]` constant derived from the BEHAVIOR_CONTRACT.md table.
- Fixture `tests/fixtures/0007-http1-direct-response/` — 5 files (`envoy.yaml` with admin block + `0.0.0.0` listener bind + HCM filter chain with single-VH single-route `prefix: "/"` direct_response 200 `"ok\n"`; `envoy-rust.yaml` per-side divergences — no admin, `127.0.0.1` bind; `inputs/payload.bin` empty for GET; `expectations.yaml` driver kind `http1` with `method: GET`, `path: "/healthz"`, `host: "envoy-rust.test"`, `expected_status: 200`, `expected_body: { byte_exact: "ok\n" }`, `expected_headers: { rule: set_equal_modulo_allow_list }`; `README.md`).
- Docker-gated `tests/differential/tests/http1_direct_response.rs` (sibling of `tls_downstream.rs` / `tls_upstream.rs` / `tls_sni.rs`).

~250 LoC harness + ~10 tests + 5 fixture files.

### Phase 04.2 — header matcher fan-out + ADR-0021 (`regex` foundation)

**D6.2 — `HeaderMatcher` schema additions in `envoy-config`.** Adds 7 matcher modes + `invert_match: bool` + `StringMatcher` tagged union. New types in `bootstrap.rs`:

- `HeaderMatcher` — `name: String` (header name; matched case-insensitively per HTTP/1.1) + `mode: HeaderMatcherMode` + `invert_match: bool` (default `false`).
- `HeaderMatcherMode` enum (Envoy's oneof shape):
  - `ExactMatch(String)` — value equals literal (case-sensitive).
  - `PrefixMatch(String)` — value starts with literal.
  - `SuffixMatch(String)` — value ends with literal.
  - `SafeRegexMatch(SafeRegex)` — value matches regex; `SafeRegex` carries `regex: String` (compiled at config-load time into `Arc<regex::Regex>`).
  - `RangeMatch(Int64Range)` — value parses as i64 and falls in `[start, end)`; `Int64Range { start: i64, end: i64 }`.
  - `PresentMatch(bool)` — header presence (`true`) or absence (`false`); the `false` case maps to `invert_match: true` semantics in some Envoy configs but is parsed verbatim here.
  - `StringMatchVariant(StringMatcher)` — Envoy's modern generic matcher (the `string_match` field).
- `StringMatcher` enum: `Exact(String)`, `Prefix(String)`, `Suffix(String)`, `SafeRegex(SafeRegex)`, `Contains(String)`. `ignore_case: bool` field (default `false`) on each variant per Envoy's StringMatcher schema (or as an outer field — to be decided at 04.2 SPEC writeup; Envoy's actual proto has `ignore_case` on the StringMatcher level).

`Route.match.headers: Vec<HeaderMatcher>` — added to `RouteMatch`. Matcher semantics: ALL header matchers must match for the route to match (AND semantics; Envoy default).

Validator extensions: `ConfigError::InvalidRegex { source: regex::Error }`, `InvalidInt64Range { start: i64, end: i64 }` (rejects `start >= end`). ~150 LoC matcher + ~50 LoC schema + ~25 unit tests across all modes + edge cases. 1 fuzz-corpus seed extension.

Fixture 0007 (landed in 04.1) is amended in 04.2 to add a second route with a `headers:` matcher (e.g., `[{ name: "x-test", exact_match: "foo" }]`) demonstrating production matcher use; the fixture remains green on both sides because the matcher selects the same route on both proxies.

**D7.2 — ADR-0021 (`regex` permitted as a foundation for header / route matching).** Lands at 04.2 Task 1 (mirrors phase 03.1 Task 1's ADR-0018+0019 inline-landing pattern). Provenance footer cites the parent-04 brainstorm decision to land all 7 `HeaderMatcher` modes (one of which — `safe_regex_match` — requires regex compilation), plus the 3-way split decision (codified in ADR-0020) that placed the regex-bearing matcher fan-out in 04.2. Scope: narrowly permits `regex = "1"` for `HeaderMatcher::SafeRegex` + `StringMatcher::SafeRegex` compilation at config-load time. NOT permitted for general-purpose use elsewhere; future filter-framework regex needs (e.g., URL path templates) require an explicit scope-extension ADR. Cargo dep added to `crates/envoy-config/Cargo.toml`'s runtime `[dependencies]` section.

### Phase 04.3 — upstream HTTP/1.1 dial + router proxy arm + fixture 0008

**D8.3 — `envoy-http1::Client` (per-connection HTTP/1.1 client).** Public surface: `Client::connect(addr: SocketAddr, host: &str) -> Result<ClientStream, Http1Error>` (TCP-connect + sends nothing); `ClientStream::send_request(Request) -> Result<Response, Http1Error>` (writes serialized request including `Host:` header; reads response via `httparse::Response::parse`; handles `Content-Length` and `Transfer-Encoding: chunked` response framings — chunked reader is new in 04.3). NO connection pooling (deferred to upstream-robustness family). Request body forwarding: drain `Content-Length` bytes from downstream, write to upstream as `Content-Length`-framed (no `Transfer-Encoding: chunked` request bodies in 04.3 — Envoy supports chunked requests but envoy-rust's first cut handles the simpler CL case; chunked-request forwarding deferred). Response body forwarding: read CL or chunked from upstream, write CL or chunked to downstream (preserve framing). Trailers: not forwarded in 04.3 (deferred). New `Http1Error` variants: `UpstreamConnect`, `UpstreamHandshake` (placeholder; HTTP/1.1 has no handshake but the variant accommodates future TLS-on-upstream-HCM combos), `MalformedResponseLine`, `MalformedChunkedFraming`. ~250 LoC + ~8 tests.

**D9.3 — Router filter "proxy to cluster" arm.** `RouteAction` enum gains `Route(RouteAction_Route)` variant (`cluster: String`; future timeout / retry / weighted-clusters knobs deferred to upstream-robustness family). `Route` is the `route` field on Envoy's RouteAction oneof (next to `direct_response`, `redirect`, etc.). HCM's router invocation in 04.1 was hardcoded `direct_response`-only; 04.3 extends to a match on the action: `DirectResponse` arm unchanged (writes static body); `Route(action_route)` arm calls `cluster_mgr.get(&action_route.cluster).expect("validator ensures cluster present")`, picks endpoint via existing round-robin LB, calls `Client::connect(endpoint, original_host_header)`, forwards the request body (drained from downstream), reads response, writes response back to downstream with the header allow-list applied (envoy-rust adds `x-envoy-upstream-service-time: <ms>` per Envoy's wire-shape — both sides emit; values diverge per the allow-list). Validator extension: per-route `cluster` reference must point at a known cluster (`ConfigError::UnknownCluster` already exists from phase 02.1; reused here). ~150 LoC + ~6 tests.

**D10.3 — `tests/helpers/http1-echo-server/` (new workspace member).** Sibling of `tests/helpers/tcp-echo-server/` (phase 02.1) and `tests/helpers/tls-echo-server/` (phase 03.2). Hand-parsed argv (`--port <u16>`); no TLS (plaintext only). Minimal HTTP/1.1 echo: any request method + path produces `200 OK` with `Content-Type: text/plain` + a body containing the echoed request method + path + headers + body (similar to httpbin.org's `/anything` shape). Both proxies must produce byte-exact upstream output; the helper is a single shared binary so there's no per-side divergence in the upstream response. ~150 LoC + 5 tests (4 argv + 1 round-trip).

**D11.3 — Differential harness `Http1EchoBackend` + fixture 0008.** `Http1EchoBackend` mirrors `TcpProxyBackend` / `TlsEchoBackend` shape: `spawn() -> Result<Self>` (locates `http1-echo-server` binary at workspace `target/<profile>/http1-echo-server`; reserves port; spawns subprocess; waits for accept-readiness); `port() -> u16`; `container_host() -> &'static str` (`"host.docker.internal"` per ADR-0015); SIGKILL-on-Drop posture. Locator helper `locate_http1_echo_server()` mirrors `locate_tls_echo_server()`. `Driver::Http1` extension to handle proxied responses: when expectations include `expected_body: { byte_exact_with_request_echo }`, the harness compares the response body against the expected echo shape (constructed deterministically from the request). Fixture `tests/fixtures/0008-http1-router-upstream/` (5 files; envoy.yaml with HCM + single-VH single-route `prefix: "/"` `route: { cluster: backend }` + cluster `backend` with single endpoint `{{BACKEND_HOST}}:{{BACKEND_PORT}}`; envoy-rust.yaml per-side divergences; inputs/payload.bin = serialized HTTP request bytes; expectations.yaml driver `http1` with proxy-shape assertions; README.md). Docker-gated `tests/differential/tests/http1_router_upstream.rs`. ~200 LoC + 4 tests.

**D12.3 — `Cluster::name()` opportunistic close-out (the multi-phase carryforward).** Per the M1 chain (phase-02.1 REVIEW M1 → phase-02.2 §4 rec 1 → phase-03.1 §4 rec 2 → phase-03.2 Task 5 deferred to phase 06): 04.3 evaluates whether the per-route proxy attribution (e.g., `tracing::warn!(cluster = ..., addr = ..., ...)` log lines on per-cluster proxy errors, or a future `RouterError::UpstreamConnect { cluster: String, source }` variant) benefits enough to close the carryforward in 04.3. **Recommended decision (default): close M1 in 04.3.** The router filter's per-cluster proxy attribution is the natural use site (a future `RouterError::UpstreamConnect { cluster, source }` would be informative; per-cluster log attribution makes operational debugging materially easier). If closed, lands `pub(crate) fn Cluster::name(&self) -> &str` on `envoy_cluster::Cluster` + removes the field-level `#[allow(dead_code)]`. Decision recorded in 04.3 PROGRESS / REVIEW; documented in 04.3 SPEC §3 D12 at SPEC writeup time.

### Cross-sub-phase architectural rules (baked into the parent SPEC)

These rules are non-negotiable across the three sub-phases; sub-phase SPECs inherit them verbatim:

1. **`envoy-http1` is the SOLE workspace dep on `httparse`.** Mirrors how `envoy-tls` is the sole dep on `rustls`. No other crate calls `httparse::Request::parse` or `httparse::Response::parse` directly. envoy-bin and envoy-config consume `envoy-http1`'s public types instead.
2. **Route-walking algorithm lives in HCM, not in envoy-config.** envoy-config owns the `RouteConfiguration` schema + validation; the per-request route-resolution algorithm (VH `domains` match → first-match-wins; route `match` → first-match-wins) lives in `envoy-http1`'s HCM module. Mirrors how `envoy-cluster::ClusterManager` owns cluster resolution while envoy-config owns cluster schema.
3. **Router filter is hardcoded in HCM in 04.1; phase 07 generalizes.** No `Vec<Box<dyn HttpFilter>>` chain abstraction in 04.x. envoy-config still parses `http_filters: [{ name: "envoy.filters.http.router", ... }]` (Envoy fixtures require it as YAML input); the validator just rejects any other filter name with `ConfigError::UnsupportedHttpFilter`. When phase 07 lands the chain abstraction + extension registry, the hardcoded call site refactors into a chain-iteration call site.
4. **04.2's matcher additions extend `HeaderMatcher` purely additively.** No 04.1-landed schema field is renamed or restructured.
5. **04.3's upstream-HTTP/1.1 work uses the existing `envoy-cluster::ClusterHandle` API for endpoint resolution.** No new public surface on envoy-cluster (modulo D12.3's optional `name()` accessor).
6. **HCM-with-TLS works automatically via `TlsAcceptingHandler`.** Phase 03.1's adapter wraps any `Arc<dyn ConnectionHandler>` including the new HCM. NOT exercised in 04.x fixtures (all 4 fixtures touched in phase 04 are plaintext); a future fixture combining HTTP/1.1 + TLS termination is a small extension and lands when needed.

---

## 4. Non-goals (deferred to later phases)

Out of phase 04 entirely:

- **HTTP/2 and HTTP/3.** `codec_type: HTTP2` and `codec_type: HTTP3` reject with `ConfigError::UnsupportedCodecType`. Phase 05 (HTTP/2 with `h2`-codec usage per D-3.2) and the QUIC family.
- **HTTP filter chain framework** (per-route config; iteration protocol with `Continue` / `StopIteration` / `StopAllIterationAndBuffer` / etc. states; extension registry). Phase 07.
- **Connection pooling** on the upstream side. Upstream-robustness family.
- **Retries, hedging, request timeouts, idle timeouts** on the router action. Upstream-robustness family.
- **Request / response header manipulation on routes** (`request_headers_to_add`, `response_headers_to_remove`, `most_specific_header_mutations_wins`, etc.). 04.3 may pull a minimal subset if needed for the proxied flow (specifically: forwarding `Host:` to upstream is essential and is in-scope); broader header-manipulation is HTTP-filters family or a follow-on phase.
- **Access logs** (the `access_log` field on HCM). Phase 06.
- **Tracing** (the `tracing` field on HCM). Observability family.
- **xDS-driven RDS** (RouteConfiguration delivered via xDS). xDS family.
- **Wildcard `domains: ["*.example.com"]` matching** on virtual hosts. Phase 04 supports `["*"]` (catch-all) or exact-string matching only. Wildcard prefixes deferred to a follow-on or to whichever phase first needs them.
- **WebSocket upgrades** (`Upgrade: websocket` request header handling). Out of phase 04 entirely.
- **HTTP CONNECT method** (for proxying TLS through HTTP). Out of phase 04 entirely.
- **`100-Continue`** request expectations. Out of phase 04 entirely.
- **Pipelining** (per HTTP/1.1 §6.3.2 — multiple requests sent before responses). Not supported; envoy-rust serializes requests on a connection (one-at-a-time per spec), matching Envoy's posture.
- **Per-virtual-host `typed_per_filter_config` / per-route `typed_per_filter_config`.** Phase 07 (filter chain framework).
- **`per_request_buffer_limit_bytes`** and other request/response buffering knobs. Out of phase 04 entirely.
- **`server_name` HCM field** (controls the `Server:` response header literally). Deferred per the parent-SPEC's "minimal HCM scope" decision (see §3 D2.1: HCM accepts exactly 4 fields — `stat_prefix`, `codec_type`, `route_config`, `http_filters`). Phase 05 (where HTTP/2 may also want to emit `:status` plus `Server:` headers) is the natural landing point.
- **Multiple HTTP filters in `http_filters`.** 04.x's HCM accepts exactly one filter (the router); the chain framework landing in phase 07 lifts this restriction.
- **Multiple HCM listeners.** Phase 02.1's `TooManyListeners` cap is unchanged in phase 04 (single listener per envoy-rust process). Future phases may relax this.

The sub-phase SPECs may surface small additional non-goals at SPEC writeup time; they will be enumerated in each sub-phase SPEC's own §4.

---

## 5. Splitting guidance for the planner

**Decision: split into 3 sub-phases.** The parent-04 split decision is codified in **ADR-0020** (lands at parent-04 state-2 alongside the sub-phase SPECs; mirrors phase 03's `f256d2c`-shape state-2 commit landing ADR-0017 + sub-phase SPECs).

**Three-way split rationale:**

The natural two-way split would have been by traffic direction (mirroring phase 03's 03.1 = downstream / 03.2 = upstream cadence): 04.α = codec + HCM scaffold + minimal routing + direct_response + ALL header matchers + fixture 0007; 04.β = upstream HTTP/1.1 + router proxy arm + fixture 0008. The brainstorm rejected this shape because the matcher fan-out alone (all 7 `HeaderMatcher` modes + `StringMatcher` + `invert_match` + the new `regex` foundation under ADR-0021) was sized at ~1300 LoC, which would have pushed 04.α to ~2300+ LoC — over the §6.1 split-gate (~1500 LoC). The alternative was nested-splitting 04.α → 04.α.1 / 04.α.2, which `BOOTSTRAP_PROMPT.md` §6.1 flags as an anti-pattern requiring `superpowers:systematic-debugging` first.

The 3-way flat split avoids the nesting anti-pattern at the cost of one extra sub-phase row in the ROADMAP. The split boundary is by **surface boundary** (codec/HCM → matcher fan-out → upstream proxying), not strictly by traffic direction:

| Sub-phase | Surface | LoC est. | Tasks est. | Notes |
|---|---|---|---|---|
| **04.1** | `envoy-http1` codec + HCM + minimal routing (prefix + path) + direct_response + fixture 0007 + harness extensions | ~1500 | ~17 | Both gates hold comfortably |
| **04.2** | All 7 HeaderMatcher modes + StringMatcher + invert_match + ADR-0021 (`regex`) + fixture 0007 amendment to exercise a non-trivial matcher | ~1300 | ~14 | Both gates hold comfortably; no new fixture |
| **04.3** | Upstream HTTP/1.1 (`envoy-http1::Client`) + router proxy arm + http1-echo-server helper + fixture 0008 + Cluster::name() opportunistic close | ~1500 | ~17 | Both gates hold comfortably |
| **Total** | | **~4300** | **~48** | Way over single-phase gates; split is mandatory |

Each sub-phase fits comfortably under the §6.1 gates (~25 tasks / ~1500 LoC). **Do not nest-split any sub-phase.** If a sub-phase's actual PLAN.md crosses either gate at write-time, invoke `superpowers:systematic-debugging` first per BOOTSTRAP_PROMPT.md §6.1; nested-splits of a sub-phase that was itself produced by a split deserve a fresh root-cause analysis (scope creep vs. planner overdecomposition).

**Sub-phase ordering and dependency:**

```
parent 04 (this SPEC)
    │
    ├─→ 04.1 (codec + HCM + routing + direct_response + fixture 0007)
    │        │
    │        └─→ 04.2 (header matchers; depends on 04.1's RouteMatch schema)
    │                │
    │                └─→ 04.3 (upstream proxying; depends on 04.1's HCM + 04.2's matchers)
```

Each sub-phase's `depends-on` ROADMAP column reflects this. The sub-phases ship strictly in order (04.1 → 04.2 → 04.3) — they cannot be parallelized because 04.2 amends 04.1's fixture and 04.3 extends both.

**Parent ROADMAP row 04 flips `done` at 04.3's state-6 phase-done commit** (mirrors phase 03's `ca81226`-shape close-out: the last sub-phase commit also closes the parent).

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the sub-phase planners resolve them in-plan rather than mid-execution. Each sub-phase SPEC will inherit + extend its relevant signposts; this section lists the parent-level ones.

1. **Codec is request-only-parsing in 04.1.** No `httparse::Response` for parsing upstream responses (that's 04.3 D8.3). 04.1's `Http1Codec` exposes only `parse_request(buf: &mut Buf) -> Result<Option<Request>, Http1Error>`.

2. **Header model: case-insensitive lookup, case-preserving storage.** `Vec<(String, String)>` ordered by emission order (load-bearing for response wire-format byte-exactness — Envoy's HCM emits headers in a specific order that envoy-rust must match). Helper `headers.find(name: &str) -> Option<&str>` does case-insensitive name match per HTTP/1.1 §3.2. Common header names (`content-length`, `host`, `connection`, `server`, `date`, `content-type`) lifted into `crates/envoy-http1/src/headers.rs` constants.

3. **`server` header default is `envoy-rust`.** Allow-listed per the BEHAVIOR_CONTRACT.md edits in §2 above. HCM emits this on every response unless HCM `server_name` config field is set (deferred to phase 05+; see §4 non-goals). Lands in 04.1 D3.1.

4. **`date` header is generated via a hand-rolled IMF-fixdate writer.** RFC 7231 §7.1.1.1 IMF-fixdate format: `Sun, 06 Nov 1994 08:49:37 GMT`. Hand-rolled is ~30 LoC and avoids pulling `httpdate` (not on D-3.2's permitted list). Placement: `crates/envoy-http1/src/date.rs`. Test: pin a `SystemTime` value and assert the formatted string. Cross-check at 04.1 SPEC writeup whether to add `httpdate` under a tiny ADR or stick with the hand-rolled approach.

5. **Route walking is single-pass first-match-wins.** O(VHs × routes) per request; acceptable for 04.x (no fixture has > 4 routes). Phase 07 may introduce indexed/trie-based matchers when the matcher framework warrants it.

6. **`route_config` is parsed eagerly at startup.** No RDS (xDS family). The `RouteConfiguration` struct is held in the per-listener HCM config (Arc-shared across connections). Hot-reload is out of scope.

7. **`drive_http1` returns a `(Status, Headers, Body)` triple.** The harness's `assert_equivalence` extends to header set-equality + value-equality-modulo-allow-list. The allow-list is parsed from a static `HEADER_ALLOW_LIST: &[(&str, AllowMode)]` constant in `tests/differential/src/lib.rs`, populated per BEHAVIOR_CONTRACT.md edits and updated in lockstep.

8. **Phase 04.2's matcher impl** uses `enum HeaderMatcherMode { ExactMatch(String), PrefixMatch(String), SuffixMatch(String), SafeRegexMatch(SafeRegex), RangeMatch(Int64Range), PresentMatch(bool), StringMatchVariant(StringMatcher) }` + `invert_match: bool` field on the outer `HeaderMatcher` struct. `regex::Regex` is compiled at config-load time and held in `Arc<...>` for cheap clone. Validator rejects unparseable regex strings with `ConfigError::InvalidRegex { source: regex::Error }`.

9. **`http1-echo-server`'s response shape** is a deterministic echo of the request's method + path + headers + body, similar to httpbin.org's `/anything`. Format is structured (e.g., a JSON-like text block); both proxies must produce byte-exact output; the helper is a single shared binary so there's no per-side divergence in the upstream response. Specifically: `Content-Type: text/plain` + body of form `"method: GET\npath: /\nheaders:\n  host: <h>\n  ...\nbody: <b>\n"` (exact format TBD at 04.3 SPEC writeup).

10. **Fixture 0008's `payload.bin` is a serialized HTTP/1.1 request line + headers** (e.g., `"GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nContent-Length: 0\r\n\r\n"` as raw bytes); `drive_http1` reads it from disk and writes it directly onto the socket. This sidesteps having `drive_http1` know how to construct an HTTP/1.1 request from structured fields — keeps the harness simple and the wire-format under fixture-author control.

11. **`Host:` header is mandatory per HTTP/1.1 §5.4.** envoy-rust's HCM rejects requests without a `Host:` header with `400 Bad Request`. Both fixtures (0007 + 0008) include `Host:` in their request bytes.

12. **Connection lifecycle = HTTP/1.1 keep-alive default.** envoy-rust serves keep-alive unless request carries `Connection: close`. Idle-connection 5s timeout reading next request line. HCM `idle_timeout` config knob deferred (HCM accepts only the 4 fields enumerated in D2.1).

13. **No request-body drain in 04.1.** Fixture 0007 is `GET /healthz` with `Content-Length: 0` (no body). 04.3's upstream proxying introduces drain logic (downstream → upstream forwarding) and the response chunked-encoding reader.

14. **`x-envoy-upstream-service-time` header lands in 04.3 only.** 04.1's `direct_response` doesn't proxy upstream so the header is never emitted. 04.3 emits it on every router-proxy response (both proxies); allow-listed.

15. **HCM's per-listener config is held in an `Arc<HCMConfig>` shared across connection handlers.** Configuration is immutable post-startup; per-connection state (current request being parsed, response being built) lives on the per-connection task's stack/heap.

16. **Body limits.** envoy-http1's `BodyTooLarge` and `HeadersTooLarge` errors enforce reasonable defaults: headers ≤ 8 KiB (matches phase 02.2's admin tightening per phase-01 REVIEW I4), request body unlimited in 04.1 (since direct_response ignores body), upstream response body unlimited in 04.3. Knobs to make these configurable defer to upstream-robustness or HCM-modest-fields phase.

17. **HCM placement decision.** D3.1 leaves the HCM crate placement TBD ("`crates/envoy-http1/` is the lean, but envoy-bin or a new `envoy-http` orchestration crate are also fits"). Recommendation at parent-04 state-2: place HCM in `envoy-http1` so the codec + per-connection state machine + per-listener route-walker live together, mirroring how `envoy-tcp::TcpProxy` lives in envoy-tcp not envoy-bin. envoy-bin just wires HCM into the listener via a new `TypedConfig` dispatch arm.

18. **`anyhow` boundary** at envoy-bin's integration tests. `crates/envoy-bin/tests/http1_direct_response.rs` and `crates/envoy-bin/tests/http1_router_upstream.rs` are in the binary crate's package and may use `anyhow` (D-3.2 permits `anyhow` only in `envoy-bin`). The `tests/differential/` crate may use `anyhow::Result<()>` returns on `drive_http1` for consistency with `drive_tls` / `drive_tls_probes`'s phase-00-established harness-wide `anyhow` posture.

19. **`Cluster::name()` accessor evaluation.** Per D12.3, evaluate at 04.3 execution time. The recommended default close-in-04.3 (since the router's per-cluster proxy attribution is a strong use case) is documented in 04.3 PROGRESS / REVIEW. If closed, lands `pub(crate) fn Cluster::name(&self) -> &str` + removes the field-level `#[allow(dead_code)]` on `Cluster.name`.

20. **Phase-04 fixture YAMLs use `static_resources.listeners[0].filter_chains[0].filters[0]` of name `envoy.filters.network.http_connection_manager`** (sibling of `envoy.filters.network.tcp_proxy` in fixtures 0003-0006). The HCM's `typed_config` carries the route_config inline (not RDS).

---

## 7. ADRs expected from this phase

**ADR-0020 — Split phase 04 into 04.1 + 04.2 + 04.3.** Lands at parent-04 state-2 (mirrors phase 03's `f256d2c`-shape state-2 commit landing ADR-0017 + sub-phase SPECs). Provenance footer notes the 3-way split is unusual but explicitly motivated by the matcher fan-out scope: the parent-04 brainstorm landed two cascading scope decisions — (a) all 7 `HeaderMatcher` modes are in-scope for phase 04 (`exact_match`, `prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`, `present_match`, `string_match`) and (b) `prefix` + `path` + `headers` are all supported on `RouteMatch` — together sized at ~2300+ LoC, which would have pushed a two-way 04.α (downstream-everything) over the §6.1 split-gate (~1500 LoC). The 3-way flat split (codec/HCM → matchers → upstream) avoids the alternative of nested-splitting 04.α, which BOOTSTRAP_PROMPT.md §6.1 flags as an anti-pattern requiring `superpowers:systematic-debugging` first. The deferral alternative (move some matcher modes to 04.2 within a two-way split) was rejected in favor of landing all 7 modes coherently in a single sub-phase. Retains the parent-SPEC §3 D3.1 / signpost 17 architectural decision that the router HTTP filter is hardcoded into HCM in 04.x (no filter chain framework — that's phase 07).

**ADR-0021 — `regex` permitted as a foundation for header / route matching.** Lands at 04.2 Task 1 (mirrors phase 03.1 Task 1's ADR-0018+0019 inline-landing pattern). Provenance footer cites the parent-04 brainstorm decision to land all 7 `HeaderMatcher` modes (one of which — `safe_regex_match` — requires regex compilation). Scope: narrowly permits `regex = "1"` as a runtime dep on `envoy-config` for `HeaderMatcher::SafeRegex` + `StringMatcher::SafeRegex` compilation at config-load time. NOT permitted for general-purpose use elsewhere; future filter-framework regex needs (e.g., URL path templates) require an explicit scope-extension ADR. Consequences section names the cargo-deny implications (regex's MIT/Apache-2.0 dual license is already on the allow-list; no `[advisories]` entries needed).

**Possible additional ADRs** (only if execution surfaces a need; not anticipated):

- **ADR-0022 (or later) — `httpdate` permitted foundation** if the hand-rolled IMF-fixdate writer in `envoy-http1` proves error-prone or insufficient. Likely not — the format is simple and ~30 LoC suffices.
- **ADR-0022 (or later) — `Cluster::name()` accessor close-out** (per D12.3) — typically lands as a doc cross-reference + a public `name()` method, not a fresh ADR. ADR only if a posture decision (e.g., field-naming convention for cluster-attributed errors) is worth recording.
- **ADR-0022 (or later) — Header allow-list extensions** if 04.3 surfaces additional headers Envoy emits on proxied responses that envoy-rust can't readily match (e.g., `x-envoy-original-path`, `x-forwarded-for`). Likely a BEHAVIOR_CONTRACT.md edit + PROGRESS note, not an ADR, unless the policy affects multiple later phases.

If any of these fire, they take the next-sequential available ADR number at the time they land. Sub-phase planners may also find the need for sub-phase-local ADRs (e.g., a per-route timeout posture decision in 04.3); those land at the relevant sub-phase SPEC writeup time per D-3.5.

---

## 8. Artifacts this phase produces

Created during execution (relative to repo root), spanning all three sub-phases:

- `docs/envoy-rust/phases/04-http1/SPEC.md` (this document; lands at parent-04 state-1 — this commit).
- `docs/envoy-rust/phases/04.1-<slug>/SPEC.md` (sub-phase SPEC; lands at parent-04 state-2 alongside ADR-0020).
- `docs/envoy-rust/phases/04.2-<slug>/SPEC.md` (sub-phase SPEC; lands at parent-04 state-2).
- `docs/envoy-rust/phases/04.3-<slug>/SPEC.md` (sub-phase SPEC; lands at parent-04 state-2).
- Each sub-phase additionally produces its own `PLAN.md`, `PROGRESS.md`, `REVIEW.md`.
- `crates/envoy-http1/Cargo.toml`
- `crates/envoy-http1/src/lib.rs` (with `#![forbid(unsafe_code)]`)
- `crates/envoy-http1/src/codec.rs`, `crates/envoy-http1/src/headers.rs`, `crates/envoy-http1/src/date.rs`, `crates/envoy-http1/src/client.rs` (04.3) — exact module decomposition decided at sub-phase SPEC writeup.
- `crates/envoy-bin/tests/http1_direct_response.rs` (04.1)
- `crates/envoy-bin/tests/http1_router_upstream.rs` (04.3)
- `tests/differential/tests/http1_direct_response.rs` (04.1; Docker-gated)
- `tests/differential/tests/http1_router_upstream.rs` (04.3; Docker-gated)
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/` — 3+ new HCM/route_config/HeaderMatcher seeds across the 3 sub-phases.
- `tests/fixtures/0007-http1-direct-response/{envoy.yaml,envoy-rust.yaml,inputs/payload.bin,expectations.yaml,README.md}` (04.1)
- `tests/fixtures/0008-http1-router-upstream/{envoy.yaml,envoy-rust.yaml,inputs/payload.bin,expectations.yaml,README.md}` (04.3)
- `tests/helpers/http1-echo-server/{Cargo.toml,src/main.rs}` (04.3)

Amended during execution:

- Root `Cargo.toml` — `[workspace] members` gains `crates/envoy-http1` (04.1), `tests/helpers/http1-echo-server` (04.3).
- `crates/envoy-config/src/bootstrap.rs` — substantial schema additions across all three sub-phases (HCM types in 04.1; HeaderMatcher in 04.2; RouteAction_Route in 04.3).
- `crates/envoy-config/src/lib.rs` — re-exports + new `ConfigError` variants across all three sub-phases.
- `crates/envoy-config/Cargo.toml` — `regex = "1"` runtime dep added in 04.2 under ADR-0021.
- `crates/envoy-bin/src/main.rs` — new `TypedConfig` dispatch arm for HCM (04.1); per-listener wiring (04.1).
- `crates/envoy-cluster/src/cluster.rs` — `pub(crate) fn name(&self) -> &str` opportunistically added in 04.3 if D12.3 closes the M1 carryforward; field-level `#[allow(dead_code)]` removed.
- `tests/differential/src/lib.rs` — `Driver::Http1` variant + `drive_http1` helper + header allow-list constant + `diff_headers` helper (04.1); fixture 0008 dispatch (04.3).
- `tests/differential/src/backend.rs` — `Http1EchoBackend` + `locate_http1_echo_server` (04.3).
- `tests/differential/src/upstream.rs` — extends container-mount logic if 0008's envoy.yaml needs additional file mounts (likely no — 0008 is plaintext, no PEMs to mount).
- `tests/differential/Cargo.toml` — likely no changes (existing rustls/rcgen/etc. deps suffice; HTTP/1.1 doesn't need new deps).
- `docs/envoy-rust/DECISIONS.md` — ADR-0020 (parent-04 state-2) + ADR-0021 (04.2 Task 1).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — `Header allow-list` section populated in 04.1 (`server`, `date`); extended in 04.3 (`x-envoy-upstream-service-time`).
- `docs/envoy-rust/ROADMAP.md`:
  - **At parent-04 state-1 (this commit):** row `04` `status` `planned` → `in-progress`. Add `sub-phases: 04.1, 04.2, 04.3` column.
  - **At parent-04 state-2:** add ROADMAP rows `04.1`, `04.2`, `04.3` (each `status: planned` initially; the ROADMAP-schema invariant 3 flips them to `in-progress` as STATE.md points at each).
  - **At each sub-phase state-6 phase-done commit:** that sub-phase's row flips `in-progress` → `done`; the **last** sub-phase commit (04.3's) ALSO flips parent row `04` `in-progress` → `done` per the ROADMAP-schema invariant ("parent flips to `done` only after all sub-phases are `done`").
- `docs/envoy-rust/STATE.md`:
  - **At parent-04 state-1 (this commit):** advance from `phase 04 lifecycle state 1` to `phase 04 lifecycle state 2 (parent SPEC.md exists; sub-phase SPECs do not)`. Next-skill: `superpowers:writing-plans` for the split-output (parent state-2 lands ADR-0020 + sub-phase SPECs; not a single PLAN.md but the equivalent in split-shape).
  - At each sub-phase transition, STATE.md advances per the standard lifecycle.
- `Cargo.lock` — synced as a dedicated commit at each sub-phase's state-4 phase-done gate per the established phase-precedent (mirrors phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85685a3`). New transitive surface in 04.1 (httparse + bytes promotion to runtime; envoy-http1 package stanza), 04.2 (regex + regex-syntax + memchr et al. transitive surface), 04.3 (http1-echo-server package stanza).
- `deny.toml` — likely no-op at 04.1 (httparse and bytes are already in the workspace's transitive surface). 04.2 may need a `[licenses]` allow-list extension if `regex` or its transitives bring an unfamiliar license (most likely no — regex is MIT/Apache-2.0); cross-check at 04.2 Task 1.

Not touched in phase 04 (belong to earlier phases or are frozen):

- `crates/envoy-tls/`, `crates/envoy-tcp/`, `tests/helpers/{tcp,tls}-echo-server/` — finalized in earlier phases; phase 04 consumes via existing public APIs.
- `tests/fixtures/0001-tcp-echo/` through `tests/fixtures/0006-tls-sni/` — unedited; their fixtures must remain green at each sub-phase state-4 gate.
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.

---

## 9. Final commit message format (for parent-04 state-6 commit, landed at sub-phase 04.3's phase-done commit)

The parent-04 phase-done commit lands at 04.3's state-6 commit (mirrors phase 03's `ca81226`-shape close-out where the 03.2 commit also closed parent 03). Format:

```
phase 04.3: HTTP/1.1 upstream proxying + fixture 0008 [parent 04 done]

(04.3-specific summary covering the upstream HTTP/1.1 client, router proxy
arm, http1-echo-server helper, fixture 0008, harness Http1EchoBackend, the
M1 carryforward decision per D12.3.)

Closes parent phase 04 (HTTP/1.1 data plane). Sub-phases:
- 04.1 (commit <SHA>): envoy-http1 codec + HCM + minimal routing + direct_response + fixture 0007.
- 04.2 (commit <SHA>): all 7 HeaderMatcher modes + StringMatcher + invert_match + ADR-0021.
- 04.3 (this commit): upstream HTTP/1.1 origination + router proxy arm + fixture 0008.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (unchanged);
  tests/fixtures/0004-tls-downstream green (unchanged);
  tests/fixtures/0005-tls-upstream green (unchanged);
  tests/fixtures/0006-tls-sni green (unchanged);
  tests/fixtures/0007-http1-direct-response green (HTTP/1.1 listener; direct_response
  route action; matcher fan-out exercised via amended route in 04.2);
  tests/fixtures/0008-http1-router-upstream green (HTTP/1.1 proxy through to
  http1-echo-server; per-cluster routing).
Conformance: none (h2spec attaches in phase 05).
```

The parent-04 state-6 commit also flips ROADMAP rows `04` and `04.3` to `done` (rows `04.1` and `04.2` flipped at their own state-6 commits earlier in the phase). STATE.md advances to phase `05` lifecycle state 1; next-skill `superpowers:brainstorming` scoped to phase 05 ("HTTP/2 downstream + upstream (low-level framer, own conn mgr)" per `BOOTSTRAP_PROMPT.md` §8 row 05). Phase 04's projected ADR ledger (ADR-0020 + ADR-0021) is closed; phase 05's projected ADRs land at the next-sequential numbers (ADR-0022+).
