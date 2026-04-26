# Phase 04.1 — HTTP/1.1 codec + HCM scaffold + minimal routing + direct_response + fixture 0007

- **Phase id:** `04.1`
- **Parent phase:** `04-http1` (split per ADR-0020)
- **Title:** `envoy-http1` codec foundation + HCM as a network filter + minimal `RouteConfiguration` (prefix + path matchers; first-match-wins) + `direct_response` route action + fixture 0007
- **Depends on:** `03` (TLS — both downstream termination and upstream origination + multi-cert SNI). Phase 03 ROADMAP row is `done` as of commit `ca81226`. Parent phase `04` is `in-progress` as of the parent-04 state-1 close-out commit `805433e`.
- **Differential surface when done:** one new fixture green against upstream `envoyproxy/envoy:v1.33.0` — `tests/fixtures/0007-http1-direct-response/` (HTTP/1.1 listener; `direct_response` route action returning a static 200 body with `Content-Length` framing; no upstream cluster touched). Pre-existing fixtures `0001-tcp-echo`, `0002-static-admin-ready`, `0003-tcp-proxy`, `0004-tls-downstream`, `0005-tls-upstream`, `0006-tls-sni` remain green.
- **Seeded by:** `docs/envoy-rust/phases/04-http1/SPEC.md` (parent, committed at SHA `805433e`) §§1 (04.1 portion of the parent acceptance signal), 2 (Header allow-list edit landing `server` and `date`), 3 D1.1–D5.1 (deliverables expanded in §3 below), 4 (parent non-goals subset 04.1 inherits), 5 (split decision context — codified at parent-04 state-2 via ADR-0020), 6 (signposts inherited and extended).

This SPEC is the design contract for sub-phase 04.1. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the in-tree surface at parent-04 state-2 (envoy-tls / envoy-tcp / envoy-listener / envoy-cluster / envoy-config / envoy-bin shape inherited from phase 03.2) must be able to execute it without consulting the parent `04-http1/SPEC.md`.

---

## 1. Goal and acceptance signal

**Goal.** Land the first HTTP/1.1-aware data-plane path in the project: a listener whose filter chain declares `envoy.filters.network.http_connection_manager` parses HTTP/1.1 requests via `httparse`, walks an inline `route_config` (multi-VirtualHost with `domains: ["*"]` or exact-string match against the request `Host:` header; first-match-wins; multi-route per VH with `prefix:` or `path:` `RouteMatch` modes; first-match-wins), and dispatches the matched route's `direct_response` action through a hardcoded router-filter call site that writes a status + headers (`server`, `date`, `content-length`, `content-type`, `connection`) + a `Content-Length`-framed inline body back onto the connection.

The new `crates/envoy-http1/` library crate owns the workspace's sole runtime dependency on `httparse` (`envoy-bin`'s admin endpoint already pulls `httparse` for its in-binary `/ready` parser; the parent-SPEC architectural rule per §3 cross-sub-phase rule 1 narrows that dep to envoy-http1 from 04.1 onwards by routing envoy-bin's admin parser through envoy-http1's public types when the admin code is touched again in a later phase — admin code is not edited in 04.1, so this is a posture decision recorded here, not an in-flight refactor). The crate exposes `Http1Codec` (request parser), `Http1Response` (response writer; CL-only body framing in 04.1), `Request` and `Response` value types (owned `String` headers preserving emission order; case-insensitive name lookup helper), `Http1Error` typed-error enum, an `HCM` `ConnectionHandler` impl that wires through to a hardcoded router-filter call site, and an `HCMConfig` Arc-shareable per-listener configuration object.

The HCM crate-placement decision settled at this SPEC writeup time (parent-SPEC §6 signpost 17 left this TBD — the lean was `envoy-http1`; the alternatives were envoy-bin or a new `envoy-http` orchestration crate): **`envoy-http1` is the chosen home** for the codec + per-connection state machine + per-listener route-walker + the hardcoded router-filter call site. envoy-bin just wires the HCM into the listener via a new `TypedConfig` dispatch arm. This mirrors how `envoy-tcp::TcpProxy` lives in envoy-tcp (the per-connection state machine + the upstream dial) rather than in envoy-bin (which only does the wiring); is load-bearing for D3.1 (the HCM `ConnectionHandler` impl + route-walking algorithm + response builder live in `crates/envoy-http1/src/hcm.rs`); and keeps envoy-bin lean for orchestration code.

`envoy-config` grows the `HttpConnectionManager` `TypedConfig` variant + the `RouteConfiguration` schema (multi-VH, `domains: ["*"]` or exact-string match, multi-route with `prefix:` / `path:` matchers, `direct_response` route action with `inline_string` body) + 8 new `ConfigError` variants + 8 unit tests + 2 fuzz-corpus seeds. `envoy-bin` grows a new `TypedConfig` dispatch arm for HCM (sibling of the `TcpProxy` arm), an in-process integration test, and a one-line wiring change in the listener-walk to construct the per-listener `Arc<HCMConfig>` before handing the wrapped `Arc<HCM>` to `envoy-listener::Listener::bind`.

The harness gains a `Driver::Http1` variant (admin's `Driver::HttpGet` is admin-specific and stays as-is — it targets the admin port and validates against admin's response shape; the new `Driver::Http1` variant is HCM-aware and targets the HCM listener); a `drive_http1` async helper; a `diff_headers` helper that compares response-header sets modulo a static `HEADER_ALLOW_LIST` constant populated from the BEHAVIOR_CONTRACT.md table edits in §2; and a `HeaderRule` enum on the `Equivalence` shape (1 variant in 04.1; the shape leaves room for future `ExactSequence`-style additions).

Sub-phase 04.1 does **not** ship the `HeaderMatcher` fan-out (sub-phase 04.2 — see §4), the upstream HTTP/1.1 client (`envoy-http1::Client`) or the router's proxy arm (sub-phase 04.3), the `http1-echo-server` helper or fixture 0008 (sub-phase 04.3), or the `x-envoy-upstream-service-time` allow-list entry (sub-phase 04.3 — only `direct_response` is exercised here so the header is never emitted).

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 04.1's feature surface:

- (a) the new differential fixture `tests/fixtures/0007-http1-direct-response/` is green;
- (b) the pre-existing differential fixtures `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/` remain green;
- (c) no conformance suites run this sub-phase (first one — `h2spec` — attaches in phase 05);
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against an extended corpus that now includes 2 new HCM/route_config/direct_response seeds; no new fuzz target ships this sub-phase;
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- (f) `REVIEW.md` for this sub-phase is approved.

**Scope shape (inherited from parent-phase brainstorm).** Of the cascading scope decisions resolved during the parent-phase-04 state-1 brainstorm and codified in the parent SPEC (commit `805433e`), the four that bind on 04.1 are:

1. **Crate layout — new `envoy-http1` library crate.** All HTTP/1.1-specific code (codec + HCM state machine + route walker + hardcoded router call site + `Http1Response` writer + `Http1Error`) lives in `crates/envoy-http1/`. envoy-bin just wires it. Mirrors phase 03.1's "one crate per primitive" pattern (envoy-tls owns rustls; envoy-http1 owns httparse). Architectural rule from parent SPEC §3 cross-sub-phase rule 1: `envoy-http1` is the SOLE workspace dep on `httparse`. No other crate calls `httparse::Request::parse` directly; future parsing of upstream responses (parent-SPEC §3 D8.3 in 04.3) goes through envoy-http1's public surface.
2. **Router filter is hardcoded in HCM.** Per parent SPEC §3 cross-sub-phase rule 3, no `Vec<Box<dyn HttpFilter>>` chain abstraction in 04.x — phase 07 generalizes. envoy-config still parses `http_filters: [{ name: "envoy.filters.http.router", ... }]` as a YAML input (Envoy fixtures require the router filter to be present); the validator just rejects any other filter name with `ConfigError::UnsupportedHttpFilter`. The HCM in 04.1 invokes the router filter inline as a hardcoded function call after route resolution; the function's body is a `match action` over the `RouteAction` enum (`DirectResponse` arm in 04.1; `Route(_)` arm added in 04.3).
3. **Header allow-list — `server` + `date` populate the table at 04.1.** Per parent SPEC §2, the previously-empty `BEHAVIOR_CONTRACT.md` `Header allow-list` section gets its first two entries in 04.1. ADR-0011 (which deferred response-header equivalence to "phase 04 (the first phase that lays out a real HCM)") expires here. 04.2 adds nothing; 04.3 adds `x-envoy-upstream-service-time`.
4. **Driver — new `Driver::Http1` variant, NOT extension of `HttpGet`.** Per parent SPEC §3 D5.1, `Driver::HttpGet { path, host }` (phase-01) is admin-specific and continues targeting the admin port + tolerating header divergence under ADR-0011's pre-04 carve-out. The new `Driver::Http1` variant is HCM-aware: it targets the listener (not admin), enforces the new header allow-list, and grows future fields as 04.2 and 04.3 land their additions.

The remaining brainstorm forks (matcher fan-out scope, upstream HTTP/1.1 client shape, http1-echo-server response format) bind on 04.2 and 04.3 only. Forward references in this SPEC are explicit per D-3.4.

---

## 2. Behavior-contract scope for sub-phase 04.1

Sub-phase 04.1 is the first phase to introduce a non-trivial HTTP response header surface — every prior phase exercised either no headers (TCP-only fixtures 0001 / 0003 / 0004 / 0005 / 0006) or a single trivial admin response (fixture 0002 carries `Driver::HttpGet` against `/ready` and tolerated header divergence under ADR-0011). Per parent SPEC §2, ADR-0011's deferral expires in phase 04, and 04.1 is the sub-phase that lands the first two `BEHAVIOR_CONTRACT.md` `Header allow-list` rows.

**`BEHAVIOR_CONTRACT.md` edits in 04.1** — the previously-empty `Header allow-list` section (which today reads `_(empty; populated starting phase 04)_`) is replaced with a 2-row table:

| Header | Equivalence | Rationale |
|---|---|---|
| `server` | name-required, value-may-differ | Implementation-identifying. Both proxies emit `server: <name>`; envoy-rust's HCM default is `server: envoy-rust`, Envoy's default is `server: envoy`. When HCM `server_name` config field is set (deferred to phase 05+ per parent SPEC §4), value tightens to exact-match on both sides. |
| `date` | name-required, value-may-differ | Wall-clock non-determinism (RFC 7231 §7.1.1.2 IMF-fixdate format). Both proxies stamp the response with the wall-clock at response-write time; values diverge because the two proxies write at slightly different instants. |

04.2 adds no rows; 04.3 adds one (`x-envoy-upstream-service-time` per parent SPEC §2; lands at 04.3 because that's where the first proxied response — fixture 0008 — emits it).

Headers NOT on the allow-list as of 04.1 (= must be value-exact when present on a 04.1-touched response):

- `content-length` — deterministic for static-body responses; fixture 0007's body is `"ok\n"` (3 bytes) → both proxies emit `content-length: 3`.
- `content-type` — controlled by `direct_response` config; Envoy v1.33.0's default for an `inline_string` `direct_response` body is `text/plain`; envoy-rust matches. Both proxies emit `content-type: text/plain`.
- `connection` — value-exact (`keep-alive` if request did not request close; `close` if request carried `Connection: close`). Fixture 0007's request uses HTTP/1.1's keep-alive default (no `Connection:` header) → both proxies emit `connection: keep-alive`.

**Equivalence-matrix dimensions exercised** in 04.1 (no contract-rules changes — first-time usage of pre-existing equivalence-matrix rows):

- Row 1 (Response status): exercised for the first time on a non-`/ready`-admin path. Fixture 0007 opts in via `equivalence.response_status: exact` in its `expectations.yaml`.
- Row 3 (Response headers): exercised for the first time at all. Set-equal modulo allow-list (above). Fixture 0007 opts in via `equivalence.response_headers: { rule: set_equal_modulo_allow_list }`.
- Row 2 (Response body): byte-exact for the static `"ok\n"` body. Fixture 0007 opts in via `equivalence.response_body: byte_exact`.

The `assert_equivalence` helper in `tests/differential/src/lib.rs` grows a header-set diff helper in 04.1 (`fn diff_headers(envoy: &[(String, String)], envoy_rust: &[(String, String)], allow_list: &HeaderAllowList) -> anyhow::Result<()>`) that compares names-must-match-set-equal (case-insensitive) and values-must-match-when-not-allow-listed. The allow-list is a static `HEADER_ALLOW_LIST: &[(&str, AllowMode)]` constant populated from the table above; updates to the contract update the constant in lockstep. `AllowMode` is a 2-variant enum: `NameRequired` (name must appear; value not compared) — the only variant in 04.1 — leaves room for `NameOptional` or `ValueRegex` shapes if future phases need them.

No other dimension is engaged in 04.1. No access logs (phase 06). No stats (phase 06). No xDS (§9 family). No trailers (HTTP/1.1 chunked-trailer-bearing responses are not emitted by `direct_response`).

---

## 3. Deliverables

### D1 — New library crate `crates/envoy-http1/`

Added to the root `Cargo.toml` `[workspace] members`. Owns all HTTP/1.1-specific code; the only crate in the workspace that depends on `httparse`. (envoy-bin's admin endpoint pulls `httparse` already; per parent-SPEC §3 cross-sub-phase rule 1 the architectural posture from 04.1 onwards is that the admin code routes through envoy-http1's public types when admin is next touched. Admin code is not edited in 04.1, so this is recorded as a posture decision rather than executed as an in-flight refactor.)

- `crates/envoy-http1/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Dependencies from D-3.2 only (no new ADR — `httparse` is already a permitted foundation per phase-01 admin parser usage; `bytes` is permitted per D-3.2):
  - `envoy-config = { path = "../envoy-config" }`
  - `envoy-cluster = { path = "../envoy-cluster" }` (forward-looking — 04.3 wires `envoy-cluster::ClusterHandle` into the router proxy arm; 04.1 does not call into envoy-cluster but the dep is added at scaffold time so 04.3 doesn't need to re-touch the manifest. The plan-writer may defer the dep-add to 04.3 if a clean-scaffold posture is preferred; either choice is acceptable.)
  - `envoy-listener = { path = "../envoy-listener" }` (for the `ConnectionHandler` trait + `BoxFuture` re-export)
  - `httparse = "1"`
  - `bytes = "1"`
  - `tokio = { version = "1", features = ["net", "io-util", "macros", "sync", "time"] }` (the `time` feature is needed for the idle-connection 5s read timeout per §6 signpost 12)
  - `thiserror = "2"`
  - `tracing = "0.1"`

  Dev-deps: `tokio` adds `rt-multi-thread` for tests; `tempfile = "3"` if any unit test wants a tmpdir (covered by ADR-0018 from phase 03.1, dev-test-harness-only); `envoy-config = { path = "../envoy-config" }` is already a runtime dep so unit tests reach it directly.

- `crates/envoy-http1/src/lib.rs` starts with `#![forbid(unsafe_code)]` per D-3.8. Module decomposition (final shape decided here; the parent SPEC §8 left module decomposition open):

    ```text
    crates/envoy-http1/src/
      lib.rs        // crate root: #![forbid(unsafe_code)]; public re-exports;
                    //   doc comment naming the crate's role as the SOLE httparse owner.
      codec.rs      // Http1Codec (request parser via httparse), Request, Response value
                    //   types, helper `parse_request_line_and_headers` that adapts
                    //   httparse's borrowed slices into owned-String headers.
      headers.rs    // case-insensitive name lookup helper; common-header-name string
                    //   constants (CONTENT_LENGTH, HOST, CONNECTION, SERVER, DATE,
                    //   CONTENT_TYPE).
      date.rs       // hand-rolled IMF-fixdate writer (RFC 7231 §7.1.1.1) +
                    //   format_imf_fixdate(SystemTime) -> String. ~30 LoC.
      response.rs   // Http1Response writer; serializes a Response value type onto
                    //   any AsyncWrite stream as the wire-format response (status
                    //   line + headers in emission order + CRLF + body).
      hcm.rs        // HCM ConnectionHandler impl; HCMConfig per-listener Arc-shared
                    //   config; per-connection state machine; route walking algorithm
                    //   (VH domains match → first-match-wins, then route match →
                    //   first-match-wins); hardcoded router-filter call site that
                    //   matches on RouteAction (DirectResponse arm in 04.1).
      error.rs      // Http1Error enum (~6 variants).
    ```

  04.3 adds `client.rs` (`Http1Client` + `ClientStream`) plus 4 new `Http1Error` variants — additive only.

- Public surface (re-exported at `lib.rs`):

    ```rust
    pub mod codec;
    pub mod headers;
    pub mod date;
    pub mod response;
    pub mod hcm;
    mod error;

    pub use error::Http1Error;
    pub use codec::{Http1Codec, Request, Response};
    pub use response::Http1Response;
    pub use hcm::{HCM, HCMConfig};
    ```

- **Codec — request parsing.** `Http1Codec` is a thin adapter over `httparse::Request::parse`. The plan-writer may keep it stateless (one-shot `parse_request(buf: &[u8]) -> Result<Option<Request>, Http1Error>` returning `Ok(None)` on partial input) or wrap a small read-buffer state — the SPEC's preference is **stateless** because the per-connection state machine in `hcm.rs` already owns the buffer (`bytes::BytesMut`-backed); the codec is a pure parser:

    ```rust
    pub struct Http1Codec;

    impl Http1Codec {
        /// Attempt to parse a single HTTP/1.1 request from `buf`. Returns
        /// Ok(Some(req)) on a fully-parsed request; Ok(None) if `buf` does not
        /// yet contain a complete request (caller reads more bytes and retries);
        /// Err(_) on malformed input or limit violations.
        ///
        /// On Ok(Some(req)), the returned `Request` carries `req.bytes_consumed:
        /// usize` so the caller can advance the buffer past the parsed bytes.
        pub fn parse_request(buf: &[u8]) -> Result<Option<Request>, Http1Error>;
    }

    pub struct Request {
        pub method: String,                // owned for case-insensitive convenience
        pub path: String,                  // request-target as raw bytes; the HCM
                                           //   matches `prefix:` / `path:` against
                                           //   this byte-for-byte (no normalization).
        pub version: HttpVersion,          // HTTP/1.0 or HTTP/1.1 (HTTP/1.0 accepted
                                           //   for parsing; HCM rejects the request
                                           //   with 505 if version != 1.1 — Envoy
                                           //   posture; cross-check at execution).
        pub headers: Vec<(String, String)>,// emission-order preserving; case-preserving.
        pub bytes_consumed: usize,
    }

    pub enum HttpVersion { Http10, Http11 }
    ```

  Note: 04.1's HCM treats HTTP/1.0 requests per parent-SPEC §3 D3.1's "fixture 0007 is `GET /healthz`"-shape — the fixture sends HTTP/1.1, so the HTTP/1.0 path is unit-tested in the codec but not exercised end-to-end. The HCM's HTTP/1.0 posture (response carries `Connection: close` always, per HTTP/1.0 default) is captured in unit tests but is not load-bearing for fixture 0007.

- **Headers — case-insensitive lookup, case-preserving storage.** The `(String, String)` ordered-list shape is load-bearing: HCM emits response headers in a specific order that envoy-rust must match Envoy's order on for the value-exact diff to pass on non-allow-listed names (per §2 above). Helper:

    ```rust
    pub fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str>;
    ```

  Compares `name` against each header's name via `eq_ignore_ascii_case` per HTTP/1.1 §3.2 (header field names are case-insensitive). Common-header-name constants live in `headers.rs`:

    ```rust
    pub const HOST: &str = "host";
    pub const CONTENT_LENGTH: &str = "content-length";
    pub const CONNECTION: &str = "connection";
    pub const SERVER: &str = "server";
    pub const DATE: &str = "date";
    pub const CONTENT_TYPE: &str = "content-type";
    ```

  These are lowercase canonical-form names; envoy-rust emits header names in lowercase form on the wire (matches Envoy's HCM default; cross-check at execution time — if Envoy emits any header in mixed-case form, the constants are updated and the response writer's header-emission loop is updated in lockstep).

- **`date` — hand-rolled IMF-fixdate writer.** RFC 7231 §7.1.1.1 IMF-fixdate format: `Sun, 06 Nov 1994 08:49:37 GMT`. Hand-rolled is ~30 LoC and avoids pulling `httpdate` (not on D-3.2's permitted list). Placement: `crates/envoy-http1/src/date.rs`. Public surface:

    ```rust
    pub fn format_imf_fixdate(t: std::time::SystemTime) -> String;
    ```

  Implementation walks `t.duration_since(UNIX_EPOCH)?.as_secs()` and synthesizes the calendar date via integer arithmetic (Howard Hinnant's date algorithm or equivalent — ~30 LoC fitting in the file). Test: pin a `SystemTime` value (e.g., `UNIX_EPOCH + Duration::from_secs(784111777)`) and assert the output matches the canonical `"Sun, 06 Nov 1994 08:49:37 GMT"` string. **Decision (locked at this SPEC writeup): no new ADR for `httpdate`.** Per parent-SPEC §6 signpost 4 the call was left TBD; the hand-rolled approach is selected here. Future phases that find the hand-rolled writer insufficient (e.g., locale handling, additional formats) land an ADR introducing `httpdate` as a permitted foundation; 04.1 does not pre-emptively land that ADR.

- **Response writer.** `Http1Response` builds a wire-format response onto any `AsyncWrite` stream:

    ```rust
    pub struct Http1Response {
        pub status: u16,                    // 100..=599
        pub reason: Option<&'static str>,   // canonical reason per RFC 7231 §6.1;
                                            //   `None` falls back to a built-in table.
        pub headers: Vec<(String, String)>, // emission-order preserving.
        pub body: bytes::Bytes,             // CL-framed in 04.1; chunked deferred.
    }

    impl Http1Response {
        /// Writes status line + headers (in emission order) + CRLF + body.
        /// Caller is responsible for setting Content-Length: <body.len()>
        /// in `headers` — Http1Response does not auto-compute it (HCM does).
        pub async fn write_to<W>(&self, w: &mut W) -> Result<(), Http1Error>
        where
            W: tokio::io::AsyncWrite + Unpin;
    }
    ```

- **Error shape.** `Http1Error` enum (~6 variants in 04.1; 04.3 extends additively with `UpstreamConnect`, `UpstreamHandshake`, `MalformedResponseLine`, `MalformedChunkedFraming`):

    ```rust
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
        Io { #[source] source: std::io::Error },
    }
    ```

- **HCM — `ConnectionHandler` impl + per-listener `HCMConfig`.** Lives in `crates/envoy-http1/src/hcm.rs`. `HCMConfig` is the Arc-shareable per-listener immutable configuration object built once at startup from `envoy_config::HttpConnectionManagerConfig`:

    ```rust
    pub struct HCMConfig {
        pub stat_prefix: String,             // forward-compat with phase 06 stats;
                                             //   ignored at runtime in 04.x.
        pub route_config: Arc<RouteConfiguration>,  // pre-validated at config load.
        // 04.3: pub cluster_mgr: Arc<envoy_cluster::ClusterManager>,
    }

    impl HCMConfig {
        pub fn from_config(
            cfg: &envoy_config::HttpConnectionManagerConfig,
            // 04.3: cluster_mgr: Arc<envoy_cluster::ClusterManager>,
        ) -> Result<Self, Http1Error>;
    }

    pub struct HCM {
        pub config: Arc<HCMConfig>,
    }

    impl envoy_listener::ConnectionHandler for HCM {
        fn handle(&self, downstream: tokio::net::TcpStream)
            -> envoy_listener::BoxFuture<'static,
                 Result<(), Box<dyn std::error::Error + Send + Sync>>>
        {
            let config = self.config.clone();
            Box::pin(async move {
                hcm::serve_connection(config, downstream).await
                    .map_err(|e| Box::new(e) as Box<_>)
            })
        }
    }
    ```

  `serve_connection` is the per-connection state machine: read into a `bytes::BytesMut`; call `Http1Codec::parse_request`; if `Ok(None)`, read more (subject to the headers cap from §6 signpost 14 and the 5s idle read timeout); if `Ok(Some(req))`:
  1. Validate `Host:` is present (per HTTP/1.1 §5.4 — mandatory). Absent or malformed → write `400 Bad Request` and close.
  2. Validate HTTP version is 1.1 (Envoy posture; 1.0 returns 505 per cross-check at execution time).
  3. Walk `route_config.virtual_hosts` first-match-wins on the request's `Host:` value:
      - `domains: ["*"]` — catch-all.
      - exact-string match (case-insensitive per HTTP/1.1) against the `Host:` value (with port stripped — `Host: foo.example.com:8080` matches `domains: ["foo.example.com"]`).
      - Wildcard prefixes (`*.example.com`) are NOT supported in 04.x per parent-SPEC §4.
  4. Within the matched VH, walk `routes` first-match-wins on the request's `path` value:
      - `RouteMatch::Prefix(p)` matches if `req.path` starts with `p`.
      - `RouteMatch::Path(p)` matches if `req.path == p`.
      - 04.2 adds `RouteMatch::Headers(_)` (additive — same VH walk; same first-match-wins; matcher just gets richer).
  5. If no VH matches → `404 Not Found`. If VH matches but no route → `404 Not Found` (Envoy posture; cross-check at execution time).
  6. Dispatch on the matched route's `RouteAction`:
      - `DirectResponse(dr)`: build response = status `dr.status` + body `dr.body.inline_string` + headers `[server, date, content-length, content-type, connection]`. Write via `Http1Response::write_to`.
      - `Route(_)`: 04.3 adds the proxy arm. 04.1 does not parse this variant from YAML (the schema only accepts `direct_response` per D2.1 below); the `match` is therefore exhaustive in 04.1 with just `DirectResponse`.
  7. Connection lifecycle: HTTP/1.1 keep-alive default per §6.1. If request `Connection: close`, write response with `Connection: close` and close the socket after the response is fully written. Otherwise, loop back to step (1) to read the next request from the same connection. Idle-connection 5s read timeout per §6 signpost 12 — on timeout, drop the connection cleanly.
  8. Body handling in 04.1: requests with `Content-Length: N` (`N > 0`) — drain `N` bytes from the socket and discard them. This is technically not load-bearing for fixture 0007 (which is `GET /healthz` with no body), but the drain is cheap to implement and avoids leaving bytes on the wire that confuse the next-request parser. Requests with `Transfer-Encoding: chunked` — reject with `501 Not Implemented` in 04.1 (Envoy parses chunked requests; envoy-rust defers to 04.3 when the upstream-proxy path needs to read chunked request bodies). The 501 posture is documented in PROGRESS.md as a known-pre-04.3-divergence; fixture 0007 doesn't trip it.

- **Unit tests in `crates/envoy-http1/src/{codec,headers,date,response,hcm}.rs::tests`** (17 tests total, distributed across modules):

  `codec.rs::tests` (5 tests):
  - `parses_get_root_with_host` — happy path: `b"GET /healthz HTTP/1.1\r\nHost: x\r\n\r\n"` → `Ok(Some(Request { method: "GET", path: "/healthz", ... }))`; `bytes_consumed` matches buffer length.
  - `returns_none_on_partial_request_line` — `b"GET /healthz HTTP/"` → `Ok(None)` (codec waits for more bytes).
  - `returns_err_on_malformed_request_line` — `b"GET\r\n\r\n"` (missing path/version) → `Err(MalformedRequestLine)`.
  - `enforces_headers_cap` — request with header section > 8 KiB → `Err(HeadersTooLarge { cap: 8192 })`.
  - `preserves_header_emission_order_and_case` — request with `X-Foo: 1\r\nX-Bar: 2\r\nX-Foo: 3` → headers vec contains all three in order, names preserved as written (per case-insensitive lookup contract; storage case-preserving).

  `headers.rs::tests` (2 tests):
  - `find_header_is_case_insensitive` — `find_header(&[("Host".into(), "x".into())], "host")` returns `Some("x")`.
  - `find_header_returns_none_on_missing` — `find_header(&[], "host")` returns `None`.

  `date.rs::tests` (2 tests):
  - `formats_canonical_imf_fixdate` — `format_imf_fixdate(UNIX_EPOCH + Duration::from_secs(784111777))` returns `"Sun, 06 Nov 1994 08:49:37 GMT"`.
  - `formats_unix_epoch` — `format_imf_fixdate(UNIX_EPOCH)` returns `"Thu, 01 Jan 1970 00:00:00 GMT"`.

  `response.rs::tests` (2 tests):
  - `writes_status_line_headers_body` — `Http1Response { status: 200, headers: vec![...], body: Bytes::from("ok\n") }` → expected wire bytes match exactly (including CRLFs and the blank-line terminator).
  - `writes_204_with_no_body` — `Http1Response { status: 204, body: Bytes::new() }` → no body bytes after the headers' CRLF terminator (Envoy posture; cross-check at execution).

  `hcm.rs::tests` (6 tests):
  - `direct_response_returns_status_and_body` — minimal `HCMConfig` with one VH (`domains: ["*"]`) one route (`prefix: "/"`) `direct_response { status: 200, body: "ok\n" }`. Drive an in-process `tokio::net::TcpStream` pair: write `b"GET /healthz HTTP/1.1\r\nHost: x\r\n\r\n"`; read response; assert status 200, headers contain `server`/`date`/`content-length: 3`/`content-type: text/plain`/`connection: keep-alive`, body `"ok\n"`.
  - `host_match_strips_port` — VH `domains: ["foo.example.com"]`; request `Host: foo.example.com:8080` matches.
  - `first_match_wins_on_routes` — two routes: `prefix: "/healthz"` → 200; `prefix: "/"` → 500. Request `/healthz` returns 200 (first match wins; second route never reached).
  - `missing_host_returns_400` — request without `Host:` header → 400.
  - `unknown_route_returns_404` — VH matches but no route matches → 404.
  - `connection_close_closes_socket` — request with `Connection: close` → response carries `Connection: close`; subsequent read on the socket returns 0 bytes (clean close).

### D2 — `envoy-config` schema extensions for HCM + route_config

`crates/envoy-config/src/bootstrap.rs` gains the `HttpConnectionManager` `TypedConfig` variant + the `RouteConfiguration` schema + the `direct_response` route action + the `Router` HTTP filter typed-config (the only filter accepted in 04.x). The `Node` open-schema asymmetry from phase 01 is **not** widened.

The `HeaderMatcher` schema additions land in 04.2; the `RouteAction_Route` proxy variant lands in 04.3.

```rust
// On the existing TypedConfig enum (phase 02.1 introduced TcpProxy variant):
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum TypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy")]
    TcpProxy(TcpProxyConfig),
    // 04.1 NEW:
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager")]
    HttpConnectionManager(HttpConnectionManagerConfig),
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectionManagerConfig {
    pub stat_prefix: String,             // forward-compat; ignored at runtime in 04.x.
    pub codec_type: CodecType,
    pub route_config: RouteConfiguration,
    pub http_filters: Vec<HttpFilter>,   // exactly one (the router) accepted in 04.x.
}

#[derive(Debug, Deserialize, PartialEq)]
pub enum CodecType {
    AUTO,                                // accepted; envoy-rust treats as HTTP1 in 04.x
                                         //   since 04.x does not negotiate via ALPN
                                         //   (no TLS+HCM fixture in 04.x).
    HTTP1,
    // HTTP2 / HTTP3 are NOT serde variants — they are rejected by the validator
    //   with ConfigError::UnsupportedCodecType (see below). The plan-writer chooses
    //   between (a) a serde-rejecting `#[serde(other)]` catch-all + post-parse
    //   validator check, or (b) accepting all 4 variants via serde and rejecting
    //   HTTP2/HTTP3 in `validate`. Option (b) gives a better error message
    //   ("HTTP2 is not supported in phase 04") and is preferred.
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
    // Envoy's Router has many fields (suppress_envoy_headers, dynamic_stats,
    //   start_child_span, ...); all deferred. 04.1 accepts an empty struct.
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteConfiguration {
    pub name: String,
    pub virtual_hosts: Vec<VirtualHost>,  // cardinality ≥ 1; validator rejects empty.
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VirtualHost {
    pub name: String,
    pub domains: Vec<String>,             // cardinality ≥ 1; each is "*" or exact name.
    pub routes: Vec<Route>,               // cardinality ≥ 1.
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Route {
    #[serde(rename = "match")]            // `match` is a Rust keyword; rename via serde.
    pub r#match: RouteMatch,
    pub direct_response: DirectResponse,  // 04.3 generalizes this to a RouteAction
                                          //   tagged-union (direct_response | route).
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteMatch {
    // Exactly one of `prefix` or `path` must be Some (validator-enforced).
    // 04.2 adds `headers: Vec<HeaderMatcher>` — additive only.
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DirectResponse {
    pub status: u16,                      // 100..=599; validator rejects out-of-range.
    pub body: DataSource,                 // 04.1: only `inline_string` accepted.
}

// On the existing DataSource struct (phase 03.1 introduced filename-only):
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DataSource {
    #[serde(default)]
    pub filename: Option<String>,         // phase-03.1 form (TLS PEM paths)
    #[serde(default)]
    pub inline_string: Option<String>,    // 04.1 NEW: direct_response body
    // inline_bytes / environment_variable deferred per parent-SPEC §4.
}
```

The `DataSource` struct gains `inline_string: Option<String>` in 04.1 (was `filename: String` exact in 03.1; the field becomes `Option<String>` and the validator enforces "exactly one of {filename, inline_string} is Some"). Per-callsite enforcement: `direct_response.body` must be `inline_string` (validator rejects `filename` with `UnsupportedDataSource`); `tls_certificates[].certificate_chain` and `.private_key` and `validation_context.trusted_ca` must be `filename` (validator rejects `inline_string` with `UnsupportedDataSource`). The "exactly one of" cardinality check is the schema-level invariant; the per-callsite restriction is the validator-level invariant.

**Validator extensions** in `envoy-config::bootstrap::validate` — new `ConfigError` variants (04.1 portion):

- `UnsupportedCodecType { got: CodecType }` — `HTTP2` or `HTTP3` reject with this. `AUTO` and `HTTP1` accept.
- `UnsupportedHttpFilter { name: String }` — only `name == "envoy.filters.http.router"` paired with `HttpFilterTypedConfig::Router(_)` accepts. Mismatched `name` and `typed_config.@type` also rejects with this (envoy-config carries the precedent of `ConfigError::TypedConfigMismatch` from phase 02.1 — the plan-writer may reuse it instead of adding a new variant; either choice is acceptable).
- `UnsupportedRouteMatcher { matcher: &'static str }` — `RouteMatch` with both `prefix` and `path` Some (or both None) rejects. 04.2's `headers:` matcher additions extend the validator additively.
- `UnsupportedDomainMatcher { domain: String }` — VH `domains[i]` that is neither `"*"` nor a syntactically-valid DNS name (per RFC 1123 LDH rule) rejects. Wildcard prefixes (`*.example.com`) reject with this in 04.1.
- `EmptyVirtualHosts { route_config: String }` — `RouteConfiguration.virtual_hosts.is_empty()` rejects.
- `EmptyRoutes { virtual_host: String }` — `VirtualHost.routes.is_empty()` rejects.
- `EmptyDomains { virtual_host: String }` — `VirtualHost.domains.is_empty()` rejects.
- `InvalidStatusCode { status: u16 }` — `DirectResponse.status` outside `100..=599` rejects.
- `UnsupportedDataSource { field: &'static str, requires: &'static str }` — `direct_response.body` carrying `filename` rejects (`requires: "inline_string"`); `tls_certificates[].certificate_chain` carrying `inline_string` rejects (`requires: "filename"`); etc.
- `MultipleHttpFilters { count: usize }` — 04.x's HCM accepts exactly one filter (the router); the chain framework lifts this in phase 07. `count != 1` rejects.

Plus per-field `deny_unknown_fields` regression-guard tests (existing pattern from phase 03.1 D2's `rejects_unknown_field_in_downstream_tls_context`).

**Validator unit tests appended to `crates/envoy-config/src/bootstrap.rs::tests` (8 new tests):**

- `parses_listener_with_hcm_direct_response` — full happy-path fixture (listener with one filter chain carrying `HttpConnectionManager` typed_config + single VH `domains: ["*"]` + single route `prefix: "/"` direct_response 200 inline_string `"ok\n"` + http_filters: [{ name: "envoy.filters.http.router", typed_config: Router{} }]).
- `rejects_codec_type_http2` — `codec_type: HTTP2` → `ConfigError::UnsupportedCodecType { got: HTTP2 }`.
- `rejects_codec_type_http3` — `codec_type: HTTP3` → `ConfigError::UnsupportedCodecType`.
- `rejects_unsupported_http_filter` — `http_filters[0].name: "envoy.filters.http.lua"` (or any non-router) → `ConfigError::UnsupportedHttpFilter`.
- `rejects_route_match_with_both_prefix_and_path` — `match: { prefix: "/x", path: "/y" }` → `ConfigError::UnsupportedRouteMatcher`.
- `rejects_route_match_with_neither_prefix_nor_path` — `match: {}` → `ConfigError::UnsupportedRouteMatcher`.
- `rejects_direct_response_with_filename_body` — `direct_response.body.filename: "/tmp/x"` → `ConfigError::UnsupportedDataSource { field: "direct_response.body", requires: "inline_string" }`.
- `rejects_unknown_field_in_hcm_config` — `deny_unknown_fields` regression on `HttpConnectionManagerConfig` (e.g., `access_log: []` is rejected — that field is phase-06-shaped and out of phase 04 per parent-SPEC §4).

**Fuzz corpus extension.** `crates/envoy-config/fuzz/corpus/parse_bootstrap/` gains 2 new HCM-shaped seeds (04.1):

- `hcm_direct_response_happy.yaml` — full bootstrap with listener → filter chain with HCM typed_config + single-VH single-route direct_response 200 inline_string body + router filter. No upstream cluster.
- `hcm_invalid_codec_type.yaml` — same shape but `codec_type: HTTP2`. Exercises the validator's `UnsupportedCodecType` rejection path through serde.

The existing `parse_bootstrap` target picks them up automatically; no new fuzz target ships. The fuzz job's `-max_total_time=30` budget (per ADR-0010) is unchanged.

### D3 — HCM as a network filter (in `envoy-http1`)

Per the §1 crate-placement decision and §3 D1 module decomposition above, `crates/envoy-http1/src/hcm.rs` carries the HCM `ConnectionHandler` impl + the per-listener `HCMConfig` + the per-connection state machine + the route-walking algorithm + the hardcoded router-filter call site.

The HCM does **not** depend on `envoy-cluster` in 04.1 (no upstream proxying — that's 04.3). The optional `envoy-cluster` runtime dep added in D1 is forward-looking for 04.3; if the plan-writer prefers a clean scaffold, the dep is deferred to 04.3.

`envoy-listener::ConnectionHandler` stays unchanged (concrete on `tokio::net::TcpStream` per parent-phase-03 signpost 3 option α; HCM does not need the generic `S: AsyncRead + AsyncWrite + Unpin + Send + 'static` lift that envoy-tcp got in phase 03.1, because HCM's HCM-with-TLS combination works automatically via the existing `TlsAcceptingHandler` adapter — see parent-SPEC §3 cross-sub-phase rule 6 + §6 signpost 2 below). 04.x fixtures don't exercise HCM-with-TLS, so this is a posture statement, not an in-flight test.

**Per-connection state machine** (the `serve_connection` async fn called by the `ConnectionHandler::handle` impl):

```rust
async fn serve_connection(
    config: Arc<HCMConfig>,
    mut downstream: tokio::net::TcpStream,
) -> Result<(), Http1Error> {
    use bytes::BytesMut;
    let mut buf = BytesMut::with_capacity(8192);
    loop {
        // 1. Read with idle 5s timeout; on timeout, close cleanly.
        let read_n = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            downstream.read_buf(&mut buf),
        ).await {
            Ok(Ok(0)) => return Ok(()),                 // peer closed
            Ok(Ok(n)) => n,
            Ok(Err(source)) => return Err(Http1Error::Io { source }),
            Err(_elapsed) => return Ok(()),             // idle timeout → clean close
        };
        let _ = read_n;

        // 2. Try to parse a request from the accumulated buffer.
        let req = match Http1Codec::parse_request(&buf)? {
            Some(req) => req,
            None => continue,                           // need more bytes
        };
        let consumed = req.bytes_consumed;
        // 3. Drain Content-Length body bytes (discarded in 04.1 — no upstream).
        let body_len = parse_content_length(&req.headers)?.unwrap_or(0);
        // (the buffer may already contain some body bytes after the headers;
        //  read_exact the remainder if needed)
        // 4. Route + dispatch.
        let resp = build_response(&config, &req)?;
        // 5. Write response.
        resp.write_to(&mut downstream).await?;
        // 6. Connection lifecycle.
        let close = req.headers.iter().any(|(n, v)|
            n.eq_ignore_ascii_case("connection") &&
            v.eq_ignore_ascii_case("close"));
        if close { return Ok(()); }
        // 7. Advance the buffer past the consumed request (+ body) and loop.
        buf.advance(consumed + body_len);
    }
}
```

The `build_response` helper does the route walk + RouteAction match:

```rust
fn build_response(config: &HCMConfig, req: &Request)
    -> Result<Http1Response, Http1Error>
{
    let host = match find_header(&req.headers, "host") {
        Some(h) => strip_port(h),
        None => return Ok(synth_400(req)),
    };
    let vh = config.route_config.virtual_hosts.iter()
        .find(|vh| vh_matches(vh, host))
        .ok_or(())
        .or_else(|()| return Ok(synth_404(req)))?;
    let route = vh.routes.iter()
        .find(|r| route_matches(r, &req.path))
        .ok_or(())
        .or_else(|()| return Ok(synth_404(req)))?;
    // Hardcoded router-filter call site:
    match &route.direct_response {
        dr => Ok(synth_direct_response(req, dr)),
    }
    // 04.3 generalizes this match to:
    //   match &route.action {
    //     RouteAction::DirectResponse(dr) => synth_direct_response(req, dr),
    //     RouteAction::Route(r) => proxy_to_cluster(req, r, &config.cluster_mgr).await,
    //   }
}
```

The synth helpers (`synth_400`, `synth_404`, `synth_direct_response`) are pure functions that build a `Http1Response` value type with the appropriate status, headers (`server: envoy-rust`, `date: <fmt>`, `content-length: <body.len()>`, `content-type: text/plain`, `connection: <keep-alive|close>`), and body. They live alongside `build_response` in `hcm.rs`.

**Connection lifecycle.** HTTP/1.1 keep-alive default per RFC 7230 §6.1. envoy-rust serves keep-alive unless request carries `Connection: close`. Idle-connection 5s read timeout (per `tokio::time::timeout` above). HCM `idle_timeout` config knob deferred (HCM accepts only the 4 fields enumerated in D2 above).

**Body cap.** `BodyTooLarge` is enforceable but NOT enforced in 04.1 (the drain logic discards bytes; no DoS surface beyond what TCP-level backpressure handles). The error variant ships in `Http1Error` for forward-compat; 04.3 wires it.

**Tests.** Already enumerated under D1 (`hcm.rs::tests` — 6 tests). The plan-writer may move some to a separate `crates/envoy-http1/tests/hcm_integration.rs` file if the in-process pair is awkward as a unit test; either layout is acceptable.

### D4 — `envoy-bin` wiring (04.1 portion)

`crates/envoy-bin/src/main.rs::run` gains an HCM dispatch arm (sibling of the phase-02.2 `TcpProxy` dispatch arm and the phase-03.1 `TlsAcceptingHandler` dispatch arm).

1. **Per-listener filter-chain pre-pass.** For the listener's first (and only — per phase-02.1's `listeners.len() ∈ {0, 1}` cap) filter chain's first filter (per parent-SPEC §3 cross-sub-phase rule "the chain has exactly one filter in 04.x; phase 07 generalizes"; the validator enforces this via `MultipleHttpFilters`-equivalent checks at the network-filter level — the plan-writer may reuse phase-02.1's `ChainHasNoFilters` / `ChainHasMultipleFilters` if those exist or add them at this point):

   - If filter is `TypedConfig::TcpProxy(_)` → existing path. Unchanged from phase 02.2 / 03.1.
   - If filter is `TypedConfig::HttpConnectionManager(hcm_cfg)` → new path. Build `Arc<HCMConfig>` once via `HCMConfig::from_config(&hcm_cfg)?`; build `Arc::new(HCM { config: hcm_config }) as Arc<dyn ConnectionHandler>`; if filter chain has `transport_socket: Some(_)`, wrap in `TlsAcceptingHandler` per phase 03.1's existing wiring (this branch is unreachable in 04.x fixtures since no 04.x fixture combines HTTP/1.1 + TLS — but the wiring is one line of dispatch and avoids an unreachable!() ahead of phase 05's first such fixture). Hand to `envoy_listener::Listener::bind`.

2. **No new module file.** The HCM dispatch arm is a single new arm in the existing `match typed_config { ... }` block in `main.rs`. The HCM itself lives in `envoy-http1::hcm` (per §3 D1 + D3); envoy-bin just constructs it.

3. **`crates/envoy-bin/Cargo.toml`** adds: `envoy-http1 = { path = "../envoy-http1" }`. No new dev-deps in 04.1 — the in-process integration test (D5 step below) reuses the existing `tokio` + `httparse` already available in envoy-bin.

4. **Validator-already-rejects guarantees consumed.** envoy-bin assumes — and matches — the schema validator's rejections from D2: `UnsupportedCodecType`, `UnsupportedHttpFilter`, `UnsupportedRouteMatcher`, `EmptyVirtualHosts`, `EmptyRoutes`, `InvalidStatusCode`, `UnsupportedDataSource`, `MultipleHttpFilters`. The dispatch arm's `let TypedConfig::HttpConnectionManager(hcm_cfg) = filter.typed_config else { ... };` shape is acceptable (mirrors phase-02.2's `cluster_mgr.get(&tcp_proxy_cfg.cluster).expect("validator ensured present")` precedent).

5. **Integration test** `crates/envoy-bin/tests/http1_direct_response.rs` (backstop to fixture 0007, in-process, no Docker): writes a minimal config to a `tempfile::TempDir`-located YAML file; spawns `envoy-bin` as a subprocess (per phase-02.2's `tcp_proxy.rs` precedent: locate the binary via `env!("CARGO_BIN_EXE_envoy-bin")`); waits for accept-readiness via a connect-retry loop; opens a `tokio::net::TcpStream`; writes `b"GET /healthz HTTP/1.1\r\nHost: envoy-rust.test\r\n\r\n"`; reads response via a manual loop into a `Vec<u8>` until both the headers' CRLF terminator is seen AND `Content-Length: 3` body bytes are consumed; parses status + headers via `httparse::Response::parse` (plan-writer cross-checks at execution time — if envoy-bin's admin-side `httparse` import already supports response parsing, reuse the import; otherwise add `httparse` to dev-deps); asserts status 200, headers contain expected names (`server`/`date`/`content-length`/`content-type`/`connection`), body `"ok\n"`. Uses `anyhow::Result<()>` per D-3.2's permission for envoy-bin tests. ~120 LoC.

### D5 — Differential harness extensions for HTTP/1.1 + fixture 0007

`tests/differential/Cargo.toml` adds dev-deps: `httparse = "1"` (for the response parser in `drive_http1`). No new ADR — `httparse` is already a permitted foundation.

- **Driver grammar.** New tagged variant on `Driver` (in `tests/differential/src/lib.rs`):

    ```rust
    pub enum Driver {
        TcpEcho,                                                      // unchanged
        HttpGet { path: String },                                     // unchanged
        TlsTcp { sni: String, expected_cn: Option<String> },          // 03.1
        TlsTcpProbeList { probes: Vec<TlsTcpProbe> },                 // 03.2
        Http1 {                                                       // 04.1 NEW
            method: HttpMethod,
            path: String,
            host: String,
            expected_status: Option<u16>,
            expected_body: Option<BodyRule>,
            expected_headers: Option<HeaderRule>,
        },
    }

    pub enum HttpMethod {
        Get,
        // 04.3 may add Post if the upstream-proxy fixture needs request-body
        // forwarding; otherwise 04.x is GET-only.
    }

    pub enum BodyRule {
        ByteExact(Vec<u8>),
        // 04.3 adds: ByteExactWithRequestEcho — for the http1-echo-server's
        //   deterministic echo response shape.
    }

    pub enum HeaderRule {
        SetEqualModuloAllowList,
        // Future phases may add ExactSequence (preserve order strictly) or
        //   similar shapes; 04.1 lands the 1-variant enum so the field's
        //   serde shape is forward-compatible.
    }
    ```

- **`drive_http1(addr, payload, method, path, host, expected_status, expected_body, expected_headers, allow_list) -> anyhow::Result<DriveHttp1Result>`** — sibling of `drive_tcp` / `drive_tls` in shape but a new helper function:

  1. Open a `tokio::net::TcpStream::connect(addr).await?`.
  2. Construct the request bytes: `format!("{} {} HTTP/1.1\r\nHost: {}\r\n\r\n", method, path, host)`. Future-extensible to include a request body when a fixture needs one (04.3); 04.1 is GET-only so no body bytes appended.
  3. Write the request bytes; flush.
  4. Read the response into a `Vec<u8>` buffer. Loop: read via `read_buf`; after each read, attempt `httparse::Response::parse(&buf)`; on `Status::Complete(headers_end)`, find `Content-Length` in the parsed headers; continue reading until `buf.len() >= headers_end + content_length`; on `Status::Partial`, keep reading. (Chunked-encoding response readers are deferred to 04.3 per the body-framing scope in §4.)
  5. Parse status, headers, body bytes from the buffer.
  6. Run the assertions per the Equivalence rule — see `assert_equivalence` extensions below.
  7. Close the socket cleanly (drop).
  8. Return the parsed `(status, headers, body)` triple wrapped in `DriveHttp1Result` so the caller's `assert_equivalence` can compare envoy-side vs. envoy-rust-side.

    ```rust
    pub struct DriveHttp1Result {
        pub status: u16,
        pub headers: Vec<(String, String)>,
        pub body: Vec<u8>,
    }
    ```

- **`assert_equivalence` extensions** in `tests/differential/src/lib.rs`:

  - When `Driver::Http1` is in play, the harness drives both proxies via `drive_http1`, gets two `DriveHttp1Result`s, and compares them per the `Equivalence` rules:
    - `equivalence.response_status: exact` — assert `envoy.status == envoy_rust.status`.
    - `equivalence.response_body: byte_exact` — assert `envoy.body == envoy_rust.body`.
    - `equivalence.response_headers: { rule: set_equal_modulo_allow_list }` — call `diff_headers(&envoy.headers, &envoy_rust.headers, &HEADER_ALLOW_LIST)?`.
  - The new `diff_headers` helper:

    ```rust
    fn diff_headers(
        envoy: &[(String, String)],
        envoy_rust: &[(String, String)],
        allow_list: &[(&str, AllowMode)],
    ) -> anyhow::Result<()> {
        // 1. Build case-insensitive name sets; assert set-equality.
        // 2. For each common name, look up the allow-list entry:
        //    - If present and AllowMode::NameRequired: skip value comparison.
        //    - If absent: assert value-exact match.
        // 3. On any mismatch, return Err(anyhow!("..." with diff)).
    }

    pub enum AllowMode {
        NameRequired,
        // future: NameOptional, ValueRegex, ValueOneOf, ...
    }

    pub const HEADER_ALLOW_LIST: &[(&str, AllowMode)] = &[
        ("server", AllowMode::NameRequired),
        ("date", AllowMode::NameRequired),
        // 04.3 adds: ("x-envoy-upstream-service-time", AllowMode::NameRequired),
    ];
    ```

  The constant lives in `tests/differential/src/lib.rs` and is sourced from BEHAVIOR_CONTRACT.md; updates to the contract update the constant in lockstep.

- **`render_yaml` per-driver substitution.** Fixture 0007 reuses `{{PORT}}` from phase-02.2; no new substitution keys are needed (no PEM paths, no upstream backend mounts). `{{BACKEND_PORT}}` is NOT used (no upstream cluster).

- **Upstream container mount.** `tests/differential/src/upstream.rs::start` is unchanged in 04.1 — fixture 0007 has no PEM mounts and no upstream backend.

- **`run_fixture` dispatch.** Detection cascade extended:
  1. Existing `{{CA_PATH}}` / `{{LEAF_*_PATH}}` gating (phase 03.1) → builds `TlsTestPki`. Not fired by fixture 0007.
  2. Existing `{{BACKEND_PORT}}` gating (phase 02.2) → spawns `TcpProxyBackend`. Not fired by fixture 0007.
  3. Existing `{{TLS_BACKEND_PORT}}` gating (phase 03.2) → spawns `TlsEchoBackend`. Not fired by fixture 0007.
  4. New: `Driver::Http1` dispatch → calls `drive_http1` per the expectations.yaml's parsed driver fields.

  04.3 adds: detect `Driver::Http1` paired with an upstream-proxying expectation → spawn `Http1EchoBackend`.

- **Harness unit tests** in `tests/differential/src/lib.rs::tests` (3 new tests, 04.1):
  - `diff_headers_passes_set_equal_modulo_allow_list` — `envoy = [("server", "envoy"), ("date", "Sun, ...")]`, `envoy_rust = [("server", "envoy-rust"), ("date", "Mon, ...")]` → `Ok(())`.
  - `diff_headers_fails_on_value_diff_outside_allow_list` — `envoy = [("content-length", "3")]`, `envoy_rust = [("content-length", "4")]` → `Err(_)`.
  - `diff_headers_fails_on_name_set_diff` — `envoy = [("x-foo", "1"), ("date", "...")]`, `envoy_rust = [("date", "...")]` → `Err(_)` (envoy emits `x-foo`, envoy-rust does not).

- **Integration test** `tests/differential/tests/http1_direct_response.rs` — Docker-gated, same `#[ignore]`-unless-`DOCKER=1` pattern as `admin_ready.rs` (phase 01) and `tcp_proxy.rs` (phase 02.2) and `tls_*.rs` (phase 03). Calls `run_fixture("0007-http1-direct-response")`.

### D6 — Fixture `tests/fixtures/0007-http1-direct-response/`

**Property.** HTTP/1.1 listener; `direct_response` route action returning a static `200 OK` body (`"ok\n"`); no upstream cluster touched. Single-VH, single-route, prefix-based match (route's `match: { prefix: "/" }` matches any request path including `/healthz`).

Files:

- `envoy.yaml` — listener bound on `0.0.0.0:{{PORT}}` with one filter chain carrying a single network filter `envoy.filters.network.http_connection_manager`:

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

  Admin block follows fixture 0003's pattern (port 0 → ephemeral; if v1.33.0 rejects 0, fall back to a templated `{{ENVOY_ADMIN_PORT}}` reserved by the harness — same possible workaround phase-02.2 SPEC §D5 anticipated; not anticipated to trip).

  Note: per parent-SPEC §3 D5.1 "the request is `GET /healthz` — actually scratch the 404, just one route with prefix `/`": fixture 0007 has a single route with `prefix: "/"` (catch-all) so any incoming path matches. The fixture's request is `GET /healthz` and matches the single route.

- `envoy-rust.yaml` — same HCM shape with the per-side divergences from fixture 0003 (bind `127.0.0.1:{{PORT}}`, no admin block — envoy-rust does not bring up an admin endpoint in 04.1; admin is phase 01's surface and is loaded only for fixture 0002):

    ```yaml
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

- `inputs/payload.bin` — empty file (`GET` request has no body). The harness's `drive_http1` constructs the request line + headers from the `Driver::Http1` fields (`method`, `path`, `host`); `payload.bin` is a placeholder for forward-compat with 04.3's body-bearing requests where it carries the request body bytes.

- `expectations.yaml`:

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

- `README.md` — one paragraph naming the property; the HCM filter shape (single VH catch-all `domains: ["*"]`, single route prefix-`/` match, `direct_response` 200 inline_string `"ok\n"`); the absence of upstream proxying, header matchers, TLS-on-HCM, multiple filter chains as out-of-fixture (each tied to a later sub-phase or phase); ADR references (ADR-0015 cross-container-host reachability, ADR-0020 split decision); BEHAVIOR_CONTRACT.md cross-reference (`server` and `date` allow-list entries land at this fixture).

  **Forward reference to 04.2:** the fixture's `envoy.yaml` and `envoy-rust.yaml` are amended in 04.2 to add a second route with a `headers:` matcher (e.g., `[{ name: "x-test", exact_match: "foo" }]`) demonstrating production matcher use. The fixture remains green in 04.2 because the matcher selects the same route on both proxies. This SPEC's 04.1 fixture shape is the lower bound — 04.2's amendment is additive and does not touch 04.1's request/response/expectations contract.

### D7 — Phase-03 REVIEW carryforwards (status check; no action in 04.1)

Per parent-SPEC §1's baked-in defaults and §3's carryforward enumeration:

- **M1 (`Cluster::name()` accessor opportunistic close)** — parent-SPEC §3 D12.3 evaluates this at 04.3 execution time (the router's per-cluster proxy attribution is the natural use site). 04.1 does not touch envoy-cluster's accessor surface. No action.
- **Phase-03.2 REVIEW M2 (testcontainers `with_copy_to_container` macOS quirk)** — awareness-only; fixture 0007 has no PEM mounts, so this is a non-issue here. No action.
- **Phase-03.1 REVIEW M3 (rcgen 0.13 API drift)** — N/A in 04.1; fixture 0007 has no rcgen-built PKI.
- **Phase-02.2 REVIEW M4 (`Listener::serve` JoinSet type alias)** — phase 04.1 does not introduce a richer filter trait (HCM is a fresh `ConnectionHandler` impl on `Arc<HCM>`; the trait shape is unchanged from phase 02.2). No action.

### D8 — CI workflow

`.github/workflows/ci.yml` changes: **none** in 04.1. The existing `build` job runs `cargo test --workspace`, which picks up the new `envoy-http1` crate automatically. The existing `fuzz` job exercises the extended `parse_bootstrap` corpus via the same `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` invocation (now covering 2 new HCM-shaped seeds).

The Docker-gated integration test `tests/differential/tests/http1_direct_response.rs` runs under the same `#[ignore]`-unless-`DOCKER=1` gating pattern as the existing differential tests.

### D9 — ADRs to land during execution

**None anticipated in 04.1.** Per parent-SPEC §7:

- **ADR-0020 (split phase 04 into 04.1 + 04.2 + 04.3)** lands at parent-04 state-2 (= the commit landing this SPEC alongside ADR-0020 and the 04.2 + 04.3 sub-phase SPECs and the parent ROADMAP/STATE edits). 04.1 does not author ADR-0020 — it is a sibling of this SPEC at the same commit.
- **ADR-0021 (`regex` permitted as a foundation for header / route matching)** lands at 04.2 Task 1 (mirrors phase 03.1 Task 1's ADR-0018+0019 inline-landing pattern). 04.1 does not need `regex` — the matcher fan-out lives in 04.2.

Per the parent-SPEC's projection that 04.1 has no new ADRs, the 04.1 plan-writer may proceed directly to D2 (envoy-config schema) at task 1 without a leading ADR-landing task.

Possible additional ADRs land only if execution proves they're needed (per D-3.5 ambiguity-resolution discipline). Likely candidates if any:

- **`httpdate` permitted foundation** if the hand-rolled IMF-fixdate writer in `crates/envoy-http1/src/date.rs` proves error-prone or insufficient (e.g., subtle leap-year handling, locale concerns). Locked at this SPEC writeup time per §3 D1: hand-rolled is the chosen approach. If a future phase finds it insufficient, that phase lands an ADR; 04.1 explicitly declines to pre-emptively land it.
- **`cargo deny` exemption** for any new transitive license surface from the bytes / httparse chain — most likely a no-op since both are already in the workspace's transitive surface.
- **HTTP/1.0 posture** (response shape on `HTTP/1.0` requests) if Envoy v1.33.0's behavior diverges materially from envoy-rust's "always close" posture. Cross-check at execution time. Not anticipated to need an ADR (the 1.0 path is unit-test-only in 04.1; fixture 0007 sends 1.1).

If any of these fire, they take the next-sequential available ADR number at the time they land (ADR-0022+, since ADR-0020 and ADR-0021 are already projected for 04 parent state-2 and 04.2 Task 1 respectively).

---

## 4. Non-goals (deferred to 04.2, 04.3, or later phases)

The 04.1 reviewer needs to know which deferred items belong to which sub-phase so the deferral target is clear. The following enumeration pulls from parent SPEC §4 and explicitly tags the deferral target.

**Deferred to sub-phase 04.2** (matcher fan-out scope):

- **`HeaderMatcher` schema additions** — all 7 modes (`exact_match`, `prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`, `present_match`, `string_match`) + `invert_match: bool` + `StringMatcher` tagged union. 04.1 ships only `prefix:` and `path:` `RouteMatch` modes; no `headers:` field on `RouteMatch` until 04.2.
- **`regex = "1"` runtime dep on `envoy-config`** under ADR-0021. 04.1's matcher impl is exact-string only (case-sensitive for path; case-insensitive for VH `domains:` per HTTP/1.1 §5.4); no regex compilation.
- **Fixture 0007 `headers:` matcher amendment** — 04.2 amends fixture 0007's envoy.yaml + envoy-rust.yaml to add a second route with a `headers:` matcher (e.g., `[{ name: "x-test", exact_match: "foo" }]`) demonstrating production matcher use. 04.1 ships fixture 0007 with the single-route shape per D6 above.

**Deferred to sub-phase 04.3** (upstream HTTP/1.1 + router proxy arm):

- **`envoy-http1::Client` (per-connection HTTP/1.1 client)** — `Client::connect`, `ClientStream::send_request`, `Transfer-Encoding: chunked` response framing reader, `Http1Error::UpstreamConnect` / `UpstreamHandshake` / `MalformedResponseLine` / `MalformedChunkedFraming` variants.
- **Router filter "proxy to cluster" arm** — `RouteAction::Route(RouteAction_Route)` variant with `cluster: String` field; HCM's hardcoded router-filter call site grows from a 1-arm match to a 2-arm match.
- **`tests/helpers/http1-echo-server/`** — new workspace member. NOT 04.1's concern; deliberately omitted from D1's module-decomposition. The helper's response body shape is decided at 04.3 SPEC writeup time.
- **Fixture `0008-http1-router-upstream`** — HTTP/1.1 listener; router proxies `GET /` through to `http1-echo-server`. NOT in 04.1's differential surface.
- **`x-envoy-upstream-service-time` header allow-list entry** — only present on responses that proxied through to an upstream cluster (NOT `direct_response` paths). 04.1's `direct_response` never emits this header; the allow-list constant in `tests/differential/src/lib.rs` reflects only `server` + `date` until 04.3.
- **`Driver::Http1` extensions** — `BodyRule::ByteExactWithRequestEcho` for the upstream-echo response shape; `Http1EchoBackend` in the harness; `locate_http1_echo_server()` helper.
- **Request-body forwarding** (drain CL bytes from downstream → write to upstream) — 04.1's HCM drains and discards request bodies (no upstream).
- **`Cluster::name()` accessor opportunistic close-out (M1 carryforward)** — parent-SPEC §3 D12.3.

**Deferred to later phases** (unchanged from parent-SPEC §4):

- **HTTP/2 and HTTP/3** — `codec_type: HTTP2` and `codec_type: HTTP3` reject with `ConfigError::UnsupportedCodecType`. Phase 05 (HTTP/2 with `h2`-codec usage per D-3.2) and the QUIC family.
- **HTTP filter chain framework** (per-route config; iteration protocol with `Continue` / `StopIteration` / `StopAllIterationAndBuffer` / etc. states; extension registry). Phase 07.
- **Connection pooling** on the upstream side. Upstream-robustness family.
- **Retries, hedging, request timeouts, idle timeouts** on the router action. Upstream-robustness family.
- **Request / response header manipulation on routes** (`request_headers_to_add`, `response_headers_to_remove`, `most_specific_header_mutations_wins`, etc.). HTTP-filters family or a follow-on phase.
- **Access logs** (the `access_log` field on HCM). Phase 06.
- **Tracing** (the `tracing` field on HCM). Observability family.
- **xDS-driven RDS** (RouteConfiguration delivered via xDS). xDS family.
- **Wildcard `domains: ["*.example.com"]` matching** on virtual hosts. 04.1 supports `["*"]` (catch-all) or exact-string matching only. Wildcard prefixes deferred.
- **WebSocket upgrades** (`Upgrade: websocket` request header handling). Out of phase 04 entirely.
- **HTTP CONNECT method** (for proxying TLS through HTTP). Out of phase 04 entirely.
- **`100-Continue`** request expectations. Out of phase 04 entirely.
- **Pipelining** (per HTTP/1.1 §6.3.2 — multiple requests sent before responses). Not supported; envoy-rust serializes requests on a connection (one-at-a-time per spec), matching Envoy's posture.
- **Per-virtual-host `typed_per_filter_config` / per-route `typed_per_filter_config`.** Phase 07 (filter chain framework).
- **`per_request_buffer_limit_bytes`** and other request/response buffering knobs. Out of phase 04 entirely.
- **`server_name` HCM field** (controls the `Server:` response header literally). Deferred per parent-SPEC §4. Phase 05 is the natural landing point.
- **Multiple HTTP filters in `http_filters`.** 04.x's HCM accepts exactly one filter (the router); the chain framework landing in phase 07 lifts this restriction.
- **Multiple HCM listeners.** Phase 02.1's `TooManyListeners` cap is unchanged in 04.1 (single listener per envoy-rust process).
- **HCM-with-TLS fixtures.** Phase 03.1's `TlsAcceptingHandler` adapter wraps any `Arc<dyn ConnectionHandler>` including the new HCM (per parent-SPEC §3 cross-sub-phase rule 6); not exercised in 04.x fixtures (all 04.x fixtures touched in phase 04 are plaintext); a future fixture combining HTTP/1.1 + TLS termination is a small extension and lands when needed.
- **Stats subsystem, access logs, Prometheus** — phase 06.
- **Admin endpoints beyond phase 01's `/ready`** — phase 08.
- **`type: LOGICAL_DNS`, `type: STRICT_DNS`, `type: EDS`** — phase 04 still accepts only `STATIC` per phase-02.1's validator.
- **`lb_policy` variants beyond `ROUND_ROBIN`** — §9 load-balancing family.

---

## 5. Splitting guidance for the planner

Estimated scope:

| Surface | Net LoC (impl + tests) |
|---|---|
| envoy-http1 codec + headers + date + response writer + Http1Error (D1: ~200 + ~100) | ~200 + ~100 |
| envoy-http1 HCM + per-listener HCMConfig + per-connection state machine + route-walking + hardcoded router call site + 6 hcm.rs unit tests (D3: ~280 + ~100) | ~280 + ~100 |
| envoy-config schema (HttpConnectionManager + RouteConfiguration + VirtualHost + Route + RouteMatch + DirectResponse + DataSource extension + 8 validator tests + 2 fuzz seeds) | ~250 + ~120 |
| envoy-bin wiring (HCM dispatch arm + Cargo.toml dep add) + integration test `http1_direct_response.rs` | ~80 + ~120 |
| Harness `Driver::Http1` + `drive_http1` + `HEADER_ALLOW_LIST` + `AllowMode` + `diff_headers` + `HeaderRule` + `BodyRule::ByteExact` + `render_yaml` no-op + `run_fixture` dispatch + 3 unit tests + Docker-gated integration test | ~180 + ~80 |
| Fixture 0007 (5 files) | ~80 |
| BEHAVIOR_CONTRACT.md edit (Header allow-list table populated with `server` + `date` rows) | ~10 |
| ADRs 0020, 0021 — landed at parent-04 state-2 (sibling commit) and 04.2 Task 1 respectively, NOT in 04.1 | 0 |
| **Total** | **~1500 LoC; ~17 tasks** |

Both `BOOTSTRAP_PROMPT.md` §6.1 gates (> ~25 tasks OR > ~1500 LoC) hold comfortably at ~17 tasks / ~1500 LoC. The LoC estimate is at the upper edge of the gate; the plan-writer is asked to keep the per-deliverable LoC under control during PLAN.md authorship and to cross-check at the state-3 PLAN.md REVIEW.md gate that the per-task LoC stays balanced.

**Do not split 04.1 further.** Per parent-SPEC §5: nested splits of an already-split sub-phase are an anti-pattern. If the plan as actually written crosses either gate mid-write, invoke `superpowers:systematic-debugging` first per `BOOTSTRAP_PROMPT.md` §6.1 + the parent-04 state-1 brainstorm's express avoidance of nested splits — root-cause-analyze whether the gate-crossing is scope creep (un-deferred work that should move to 04.2 or 04.3) or planner overdecomposition (each task too granular) before attempting any nested split. Parent-SPEC §5's identical guidance applies here verbatim.

Sub-phase ordering and dependency (per parent-SPEC §5):

```
parent 04 (committed at SHA 805433e)
    │
    ├─→ 04.1 (this SPEC; codec + HCM + routing + direct_response + fixture 0007)
    │        │
    │        └─→ 04.2 (header matchers; depends on 04.1's RouteMatch schema)
    │                │
    │                └─→ 04.3 (upstream proxying; depends on 04.1's HCM + 04.2's matchers)
```

04.1 must ship complete before 04.2 begins. The parent ROADMAP row 04 stays `in-progress` until 04.3's state-6 commit (per parent-SPEC §5 + ROADMAP-schema invariant 3); 04.1's state-6 commit only flips row 04.1 to `done`.

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the planner resolves them in-plan rather than mid-execution. Inherits the parent-SPEC §6 signposts that apply to 04.1 (parent signposts 1–7, 10–13, 15–18, 20 — i.e., the codec / header / date / route-walk / lifecycle / driver / harness signposts; parent signposts 8 (matcher modes) and 14 (`x-envoy-upstream-service-time`) are 04.2 / 04.3 territory and not inherited; parent signpost 9 (http1-echo-server) is 04.3 territory and not inherited; parent signpost 19 (Cluster::name accessor) is 04.3 territory and not inherited; parent signpost 17 (HCM placement decision) is settled in §1 above so does not appear here as a TBD).

1. **Task ordering for 04.1.** envoy-http1 crate scaffold + `#![forbid(unsafe_code)]` + workspace-membership add → envoy-config schema additions (D2: HCM types + RouteConfiguration + DirectResponse + DataSource extension + 8 validator tests + 2 fuzz seeds) → envoy-http1 codec + headers + date + response-writer + 9 unit tests (codec/headers/date/response modules) → envoy-http1 hcm + HCMConfig + per-connection state machine + route walker + hardcoded router call site + 6 hcm.rs unit tests → envoy-bin HCM dispatch arm + integration test `crates/envoy-bin/tests/http1_direct_response.rs` → harness `Driver::Http1` + `drive_http1` + `HEADER_ALLOW_LIST` + `diff_headers` + 3 unit tests → BEHAVIOR_CONTRACT.md `Header allow-list` table populated → fixture 0007 (5 files) + Docker-gated integration test `tests/differential/tests/http1_direct_response.rs` → state-4 phase-done gate.

2. **Codec is request-only-parsing in 04.1.** No `httparse::Response` parsing for upstream responses (that's 04.3 D8.3). 04.1's `Http1Codec` exposes only `parse_request(buf: &[u8]) -> Result<Option<Request>, Http1Error>`. The differential harness's `drive_http1` is the one place 04.1 calls `httparse::Response::parse` — this is intentional because the harness lives outside the workspace's enforcement boundary (the architectural rule "envoy-http1 is the SOLE workspace runtime dep on httparse" applies to runtime crates, not to the test harness; the harness is permitted to call httparse directly per phase-00's harness posture). If 04.3's planning surfaces a desire to consolidate response-parsing into envoy-http1's public surface, that's a 04.3 architectural choice — 04.1 does not pre-emptively wire it.

3. **Header model: case-insensitive lookup, case-preserving storage.** `Vec<(String, String)>` ordered by emission order (load-bearing for response wire-format byte-exactness — Envoy's HCM emits headers in a specific order that envoy-rust must match for the `diff_headers` set-equal check to behave as expected — see §2). Helper `find_header(headers, name)` does case-insensitive name match per HTTP/1.1 §3.2. Common header names (`content-length`, `host`, `connection`, `server`, `date`, `content-type`) lifted into `crates/envoy-http1/src/headers.rs` constants in canonical lowercase form.

4. **`server` header default is `envoy-rust`.** Allow-listed per the BEHAVIOR_CONTRACT.md edits in §2 above. HCM emits this on every response unless HCM `server_name` config field is set (deferred to phase 05+ per §4 non-goals). Lands in 04.1 D3's `synth_*` helpers. The constant lives in `crates/envoy-http1/src/hcm.rs` as `const DEFAULT_SERVER_NAME: &str = "envoy-rust";` (NOT in `headers.rs` — that file holds header *names*, not values).

5. **`date` header is generated via a hand-rolled IMF-fixdate writer.** Per §3 D1's locked decision: hand-rolled ~30 LoC in `crates/envoy-http1/src/date.rs`. NO new ADR for `httpdate` in 04.1. Test pins `SystemTime::UNIX_EPOCH + Duration::from_secs(784111777)` and asserts the canonical `"Sun, 06 Nov 1994 08:49:37 GMT"` output. Cross-check at execution time whether Envoy's `date` header format matches the hand-rolled output character-for-character (it should — both implementations target RFC 7231 §7.1.1.1 IMF-fixdate); the `date` header value is allow-listed anyway, so a one-character format drift would not fail the differential. The per-character match still matters for envoy-rust's own integration test in `crates/envoy-bin/tests/http1_direct_response.rs`.

6. **Route walking is single-pass first-match-wins.** O(VHs × routes) per request; acceptable for 04.x (no fixture has > 4 routes). Phase 07 may introduce indexed/trie-based matchers when the matcher framework warrants it. The walk lives in `crates/envoy-http1/src/hcm.rs::build_response`.

7. **`route_config` is parsed eagerly at startup.** No RDS (xDS family). The `RouteConfiguration` struct is held in `Arc<HCMConfig>` (per §3 D3); Arc-shared across per-connection tasks. Hot-reload is out of scope per parent-SPEC §4.

8. **`drive_http1` returns a `(Status, Headers, Body)` triple.** The harness's `assert_equivalence` extends to header set-equality + value-equality-modulo-allow-list. The allow-list is a static `HEADER_ALLOW_LIST: &[(&str, AllowMode)]` constant in `tests/differential/src/lib.rs`, populated per BEHAVIOR_CONTRACT.md edits and updated in lockstep.

9. **Fixture 0007's `payload.bin` is empty.** `GET /healthz` has no body. The harness's `drive_http1` constructs the request line + headers from the `Driver::Http1` fields (`method`, `path`, `host`); `payload.bin` is a placeholder for forward-compat with 04.3's body-bearing requests where it carries the request body bytes. (Phase 02's TCP fixtures use `payload.bin` as the bytes-on-wire payload; phase 03's TLS fixtures inherit that semantics; 04.1 keeps the file present for fixture-shape uniformity but its content is empty.)

10. **`Host:` header is mandatory per HTTP/1.1 §5.4.** envoy-rust's HCM rejects requests without a `Host:` header with `400 Bad Request`. Fixture 0007's request bytes include `Host: envoy-rust.test`. The `drive_http1` helper always emits a `Host:` line (no opt-out); the `Driver::Http1.host` field is required (not `Option<String>`).

11. **Connection lifecycle = HTTP/1.1 keep-alive default.** envoy-rust serves keep-alive unless request carries `Connection: close`. Fixture 0007's request doesn't include `Connection:`; both proxies emit `connection: keep-alive`. The harness's `drive_http1` reads exactly `Content-Length` body bytes after the headers' CRLF terminator and then drops the socket — the keep-alive vs. close behavior is asserted via the response header value, not via socket-state observation.

12. **Idle-connection 5s read timeout.** HCM enforces a 5s `tokio::time::timeout` on each `read_buf` call between requests on a kept-alive connection. On timeout, the connection closes cleanly. HCM `idle_timeout` config knob deferred per §4 non-goals (HCM accepts only the 4 fields enumerated in D2). The 5s default is hardcoded; if a future phase needs configurability, that phase adds the field + the validator extension + the constructor argument in lockstep.

13. **No request-body drain in 04.1 (semantically).** Fixture 0007 is `GET /healthz` with no `Content-Length` (or `Content-Length: 0`). 04.3's upstream proxying introduces drain logic (downstream → upstream forwarding) and the response chunked-encoding reader. 04.1's `serve_connection` *does* read `Content-Length` bytes from the socket and discard them (per §3 D3 step 3) so subsequent requests on a kept-alive connection are not corrupted by stray body bytes — but the discarded bytes are not echoed anywhere. Requests with `Transfer-Encoding: chunked` reject with `501 Not Implemented` in 04.1 (Envoy parses chunked requests; envoy-rust defers chunked-request body handling to 04.3).

14. **Body limits.** envoy-http1's `BodyTooLarge` and `HeadersTooLarge` errors enforce reasonable defaults: headers ≤ 8 KiB (matches phase 02.2's admin tightening per phase-01 REVIEW I4), request body unlimited in 04.1 (since `direct_response` ignores body), upstream response body unlimited in 04.3. Knobs to make these configurable defer to upstream-robustness or HCM-modest-fields phase.

15. **HCM's per-listener config is held in `Arc<HCMConfig>`** shared across per-connection tasks (per §3 D3). Configuration is immutable post-startup; per-connection state (current request being parsed, response being built) lives on the per-connection task's stack/heap (no per-connection allocation beyond the read buffer + parsed `Request` value).

16. **`anyhow` boundary** at envoy-bin's integration tests. `crates/envoy-bin/tests/http1_direct_response.rs` is in the binary crate's package and may use `anyhow` (D-3.2 permits `anyhow` only in `envoy-bin`). The `tests/differential/` crate uses `anyhow::Result<()>` returns on `drive_http1` for consistency with `drive_tls` / `drive_tls_probes`'s phase-00-established harness-wide `anyhow` posture. envoy-http1 itself does NOT use `anyhow` — it returns `Result<_, Http1Error>` per D-3.2's library-crate posture.

17. **Phase-04.1 fixture YAMLs use `static_resources.listeners[0].filter_chains[0].filters[0]` of name `envoy.filters.network.http_connection_manager`** (sibling of `envoy.filters.network.tcp_proxy` in fixtures 0003-0006). The HCM's `typed_config` carries the `route_config` inline (not RDS).

18. **Per parent-SPEC §3 cross-sub-phase rule 1, `envoy-http1` is the SOLE workspace runtime dep on `httparse`.** No other crate calls `httparse::Request::parse` or `httparse::Response::parse` directly. envoy-bin's admin endpoint already imports `httparse` (phase 01 — predates the architectural rule); the rule's posture from 04.1 onwards is that the admin code routes through envoy-http1's public types when admin is next touched. Admin code is NOT edited in 04.1 — the posture is recorded here for the 04.1 reviewer (and for whichever future phase first edits admin) but not executed as an in-flight refactor.

19. **`#![forbid(unsafe_code)]` is mandatory** at every new crate's `lib.rs`: `crates/envoy-http1/src/lib.rs`. httparse + bytes + tokio + thiserror all carry their own internal unsafe behind their crates' allowlists; no envoy-rust-owned code carries unsafe.

20. **Workspace membership.** Root `Cargo.toml` `[workspace] members` grows by `crates/envoy-http1` (04.1). The `tests/helpers/http1-echo-server` add lands in 04.3.

---

## 7. ADRs expected from this sub-phase

**None anticipated in 04.1.** Per parent-SPEC §7 + §3 D9 above:

- **ADR-0020 (Split phase 04 into 04.1 + 04.2 + 04.3)** lands at parent-04 state-2 (= the commit landing this SPEC alongside ADR-0020 and the 04.2 + 04.3 sub-phase SPECs and the parent ROADMAP/STATE edits). 04.1 does not author ADR-0020 — it lands as a sibling artifact at the same commit. The 04.1 plan-writer therefore proceeds directly to D2 (envoy-config schema) at task 1.
- **ADR-0021 (`regex` permitted as a foundation for header / route matching)** lands at 04.2 Task 1 (mirrors phase 03.1 Task 1's ADR-0018+0019 inline-landing pattern), NOT 04.1.

Additional ADRs may be required during 04.1 execution per D-3.5 if:

- **`httpdate` permitted foundation** — the hand-rolled IMF-fixdate writer in `crates/envoy-http1/src/date.rs` proves error-prone or insufficient. Locked at this SPEC writeup time per §3 D1: hand-rolled is the chosen approach. The plan-writer is asked to favor the hand-rolled approach unless execution surfaces a concrete blocker.
- **`cargo deny check` flips red** on any new transitive license from the bytes / httparse chain. Both crates are already in the workspace's transitive surface (via envoy-bin's admin parser and via tokio's `bytes`-using internals); no new license surface anticipated. If a non-trivial extension surfaces, it lands its own ADR (likely ADR-0022) at landing time.
- **HTTP/1.0 response posture** — if Envoy v1.33.0's behavior on HTTP/1.0 requests diverges materially from envoy-rust's "always close" posture. Cross-check at execution time. Not anticipated to need an ADR (the 1.0 path is unit-test-only in 04.1; fixture 0007 sends 1.1).

If any of these fire, they take the next-sequential available ADR number at the time they land (ADR-0022+).

---

## 8. Artifacts this sub-phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/04.1-hcm-direct-response/PLAN.md`
- `docs/envoy-rust/phases/04.1-hcm-direct-response/PROGRESS.md`
- `docs/envoy-rust/phases/04.1-hcm-direct-response/REVIEW.md`
- `crates/envoy-http1/Cargo.toml`
- `crates/envoy-http1/src/lib.rs` (with `#![forbid(unsafe_code)]`)
- `crates/envoy-http1/src/codec.rs`
- `crates/envoy-http1/src/headers.rs`
- `crates/envoy-http1/src/date.rs`
- `crates/envoy-http1/src/response.rs`
- `crates/envoy-http1/src/hcm.rs`
- `crates/envoy-http1/src/error.rs`
- `crates/envoy-bin/tests/http1_direct_response.rs`
- `tests/differential/tests/http1_direct_response.rs`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_direct_response_happy.yaml`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_invalid_codec_type.yaml`
- `tests/fixtures/0007-http1-direct-response/envoy.yaml`
- `tests/fixtures/0007-http1-direct-response/envoy-rust.yaml`
- `tests/fixtures/0007-http1-direct-response/inputs/payload.bin`
- `tests/fixtures/0007-http1-direct-response/expectations.yaml`
- `tests/fixtures/0007-http1-direct-response/README.md`

Amended during execution:

- Root `Cargo.toml` — add `crates/envoy-http1` to `[workspace] members`. (`tests/helpers/http1-echo-server` lands in 04.3.)
- `crates/envoy-bin/Cargo.toml` — add `envoy-http1` path-dep.
- `crates/envoy-bin/src/main.rs` — new `TypedConfig::HttpConnectionManager` dispatch arm in the per-listener filter-chain pre-pass; per-listener `Arc<HCMConfig>` construction; integration with the existing `TlsAcceptingHandler` wiring (HCM-with-TLS path is wired but unreachable in 04.x fixtures — see §1 above).
- `crates/envoy-config/src/bootstrap.rs` — add `HttpConnectionManagerConfig`, `CodecType`, `HttpFilter`, `HttpFilterTypedConfig`, `RouterConfig`, `RouteConfiguration`, `VirtualHost`, `Route`, `RouteMatch`, `DirectResponse`; extend `DataSource` with `inline_string: Option<String>` and convert `filename` to `Option<String>`; extend `validate` with `UnsupportedCodecType`, `UnsupportedHttpFilter`, `UnsupportedRouteMatcher`, `UnsupportedDomainMatcher`, `EmptyVirtualHosts`, `EmptyRoutes`, `EmptyDomains`, `InvalidStatusCode`, `UnsupportedDataSource`, `MultipleHttpFilters` `ConfigError` variants; 8 new validator unit tests.
- `crates/envoy-config/src/lib.rs` — re-export new public types (`HttpConnectionManagerConfig`, `CodecType`, `HttpFilter`, `HttpFilterTypedConfig`, `RouterConfig`, `RouteConfiguration`, `VirtualHost`, `Route`, `RouteMatch`, `DirectResponse`); extend `ConfigError` enum re-exports.
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/` — 2 new HCM-shaped seeds (listed under "Created" above).
- `tests/differential/src/lib.rs` — add `Driver::Http1` variant + `HttpMethod` enum + `BodyRule` enum + `HeaderRule` enum + `AllowMode` enum + `HEADER_ALLOW_LIST` constant + `drive_http1` helper + `diff_headers` helper + `DriveHttp1Result` struct + `Driver::Http1` dispatch in `run_fixture`; 3 new unit tests.
- `tests/differential/Cargo.toml` — add `httparse = "1"` dev-dep.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — `Header allow-list` section: replace `_(empty; populated starting phase 04)_` with the 2-row table from §2 above (`server`, `date`).
- `docs/envoy-rust/ROADMAP.md` — row `04.1` `status` `planned` → `in-progress` (at state-3 commit; per ROADMAP-schema invariant 3, the row flips when STATE.md points at this sub-phase) → `done` (at state-6 commit). Row `04` (parent) stays `in-progress` (parent flips `done` only when ALL sub-phases are `done` per ROADMAP-schema; 04.2 and 04.3 are still `planned`).
- `docs/envoy-rust/STATE.md` — at state-6 commit: advance to `phase 04.2 lifecycle state 3` (sub-phase 04.2's SPEC was already landed at parent-04 state-2 alongside this SPEC, so 04.2 enters state 3 directly per the lifecycle's "if SPEC.md exists, skip state 1+2" rule); next-skill: `superpowers:writing-plans` for 04.2's PLAN.md.
- `Cargo.lock` — synced as a dedicated commit at the state-4 phase-done gate per the established phase-precedent (mirrors phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85433f0`/`85px83b` — the SHAs are illustrative; the actual SHA materializes at the 04.1 state-4 phase-done gate). New transitive surface: `envoy-http1` package stanza; `httparse` promoted from envoy-bin's transitive surface to a direct workspace runtime dep via envoy-http1's manifest.
- `deny.toml` — only if `cargo deny check` flips red on any new transitive license surface from the bytes / httparse chain. Most likely a no-op.

Not touched in 04.1 (belong to 04.2 / 04.3 / earlier phases or are frozen):

- `docs/envoy-rust/phases/04-http1/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `805433e`.
- `docs/envoy-rust/phases/04.2-<slug>/SPEC.md`, `phases/04.3-<slug>/SPEC.md` — landed alongside this SPEC at parent-04 state-2 (sibling artifacts); their PLAN/PROGRESS/REVIEW lifecycles begin after 04.1 closes.
- `docs/envoy-rust/phases/03-tls-tcp/SPEC.md`, `phases/03.1-tls-foundation-downstream/`, `phases/03.2-tls-upstream-sni/` — closed in phase 03.
- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md`, `phases/02.1-config-cluster/`, `phases/02.2-listener-tcp-proxy/` — closed in phase 02.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/` — unedited; their fixtures must remain green at 04.1 state-4 gate.
- `crates/envoy-tls/`, `crates/envoy-tcp/`, `crates/envoy-listener/`, `crates/envoy-cluster/` — finalized in earlier phases; 04.1 consumes via existing public APIs (only `envoy-listener::ConnectionHandler` is touched, and only as a consumer — the trait's shape is unchanged).
- `tests/helpers/tcp-echo-server/`, `tests/helpers/tls-echo-server/` — finalized in phases 02.1 / 03.2; 04.1 fixture 0007 has no upstream backend.
- `tests/helpers/http1-echo-server/` — does not exist yet; lands in 04.3.
- `docs/envoy-rust/DECISIONS.md` — no edits in 04.1 (ADR-0020 lands at parent-04 state-2 = sibling commit; ADR-0021 lands at 04.2 Task 1).
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.

---

## 9. Final commit message format (for state 6 of the 04.1 lifecycle)

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
