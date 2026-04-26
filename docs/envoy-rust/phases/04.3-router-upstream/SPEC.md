# Phase 04.3 — Upstream HTTP/1.1 origination + router proxy arm + fixture 0008 + http1-echo-server helper

- **Phase id:** `04.3`
- **Parent phase:** `04-http1` (split per ADR-0020)
- **Title:** Upstream HTTP/1.1 origination + router filter "proxy to cluster" arm + `http1-echo-server` helper crate + fixture `0008-http1-router-upstream` + opportunistic close-out of the multi-phase `Cluster::name()` carryforward
- **Depends on:**
  - `04.1` — codec + HCM scaffold + minimal routing + `direct_response` action + fixture 0007. 04.3 reuses `envoy-http1`'s `Http1Codec`, `Request`, `Response`, `Http1Response` writer, `Http1Error`, the per-listener `HCMConfig`, the route-walk algorithm, and the hardcoded router invocation site that 04.1 introduced as `direct_response`-only.
  - `04.2` — `HeaderMatcher` fan-out (all 7 modes + `StringMatcher` + `invert_match`) plus ADR-0021's `regex` foundation. 04.3 does not amend the matcher implementation but production code paths walk it whenever fixture 0007's matcher-bearing route (added in 04.2) selects.
  Sibling sub-phases 04.1 and 04.2 MUST both be `done` (their ROADMAP rows flipped to `done` in their own state-6 commits) before 04.3 enters `in-progress`. The strict ordering 04.1 → 04.2 → 04.3 is enforced because (a) 04.2 amends the fixture 04.1 introduced, and (b) 04.3's router-proxy arm extends the `RouteAction` enum 04.1 introduced and reuses the matcher walk 04.2 implemented.
- **Differential surface when done:** one new fixture green against upstream `envoyproxy/envoy:v1.33.0`:
  - `tests/fixtures/0008-http1-router-upstream/` — HTTP/1.1 listener; router filter forwards `GET /` (with `Host: envoy-rust.test`) to a single-endpoint cluster `backend` whose endpoint is the new in-tree `http1-echo-server` helper. Response body is the helper's deterministic echo (method + path + sorted headers + body) and is byte-exact across both proxies; response headers are set-equal modulo the BEHAVIOR_CONTRACT.md allow-list (extended in 04.3 with `x-envoy-upstream-service-time`).
  Pre-existing fixtures `0001-tcp-echo`, `0002-static-admin-ready`, `0003-tcp-proxy`, `0004-tls-downstream`, `0005-tls-upstream`, `0006-tls-sni`, and `0007-http1-direct-response` (with the matcher-bearing route added in 04.2) remain green.
- **Seeded by:** `docs/envoy-rust/phases/04-http1/SPEC.md` (parent, committed at SHA `805433e`) §3 D8.3 (envoy-http1::Client), D9.3 (router proxy arm), D10.3 (http1-echo-server helper), D11.3 (Http1EchoBackend + fixture 0008), D12.3 (`Cluster::name()` opportunistic close); §2 (BEHAVIOR_CONTRACT.md row 3 / `x-envoy-upstream-service-time`); §4 (non-goals 04.3 inherits); §6 signposts 9, 10, 11, 14, 19, 20; §9 final commit message format including the `[parent 04 done]` tag.

This SPEC is the design contract for sub-phase 04.3 (the **closing sub-phase** of parent phase 04). The next session — after 04.2 has landed its final commit and STATE.md has advanced to `04.3-router-upstream` — converts this into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed 04.1 + 04.2 surface (via `git log` and the in-tree `envoy-http1` / `envoy-config` / `envoy-cluster` / `envoy-bin` / `tests/differential` shape at the 04.2 phase-done commit) must be able to execute it without consulting the parent `04-http1/SPEC.md`.

---

## 1. Goal and acceptance signal

**Goal.** Close the parent phase 04 (HTTP/1.1 data plane) by adding upstream origination and the router filter's proxy-to-cluster arm. Three coordinated layers:

1. **Per-connection HTTP/1.1 client (`envoy-http1::Client`).** A new module under the existing `envoy-http1` crate (the SOLE workspace dep on `httparse` per parent SPEC §3 architectural rule 1, established in 04.1). Public surface: `Client::connect(addr: SocketAddr, host: &str) -> Result<ClientStream, Http1Error>` performs a plaintext TCP-connect to `addr` and stores `host` for the request's `Host:` header; `ClientStream::send_request(Request) -> Result<Response, Http1Error>` writes a serialized HTTP/1.1 request (request-line + headers + optional `Content-Length`-framed body) and reads back the response (status line + headers + `Content-Length`-framed OR `Transfer-Encoding: chunked` body — the chunked-encoding READER is new in 04.3; chunked WRITER on the request side is deferred). NO connection pooling — pooling is upstream-robustness-family territory (parent SPEC §4) and out of scope here.

2. **Router filter "proxy to cluster" arm.** The `RouteAction` enum (introduced in 04.1 as `DirectResponse`-only) gains a `Route(RouteAction_Route)` variant carrying a single field — `cluster: String` (a reference to a cluster declared under `static_resources.clusters`). The HCM's hardcoded router invocation site (introduced in 04.1 as `match action { DirectResponse(d) => write_static_response(d) }`) extends to a two-arm match: `DirectResponse` arm unchanged; `Route(action_route)` arm calls `cluster_mgr.get(&action_route.cluster).expect(...)`, picks the endpoint via the existing round-robin LB (landed in 02.1), calls `Client::connect(endpoint, original_host_header)`, forwards the request body downstream-to-upstream (CL only in 04.3; chunked-request-body forwarding deferred), reads the upstream response, writes the response back to the downstream with the header allow-list applied and the new `x-envoy-upstream-service-time: <ms>` header injected. Validator extension: per-route `cluster` reference must point at a known cluster (reuses `ConfigError::UnknownCluster` from phase 02.1 — that variant was introduced for `TcpProxyConfig.cluster` validation and is reused here for `RouteAction_Route.cluster`).

3. **`tests/helpers/http1-echo-server/` + fixture 0008.** New workspace member sibling of `tcp-echo-server` (phase 02.1) and `tls-echo-server` (phase 03.2). Plaintext only (no TLS). Hand-parsed argv (`--port <u16>`). Minimal HTTP/1.1 echo server with a deterministic response body — see §3 D3 for the byte-exact format (alphabetically-sorted headers are load-bearing for differential equivalence; both proxies forward to the SAME helper so the helper's response is the byte-exact baseline). Fixture `tests/fixtures/0008-http1-router-upstream/` proves the round-trip end-to-end: `GET / HTTP/1.1` from the harness through envoy-bin (or upstream Envoy) through the router-proxy arm to `http1-echo-server`, with byte-exact response body and set-equal-modulo-allow-list response headers.

Across all three layers, the architectural rule from parent SPEC §3 architectural rule 1 holds: **`envoy-http1` is the SOLE workspace dep on `httparse`** — the new `Client` module also calls `httparse::Response::parse` (new use site in 04.3 — 04.1 only used `httparse::Request::parse`); no other crate calls `httparse::*` directly.

**Opportunistic close-out.** Per parent SPEC §3 D12.3 + §6 signpost 19, sub-phase 04.3 evaluates the multi-phase `Cluster::name()` carryforward (originating in phase-02.1 REVIEW M1; re-deferred at phase-02.2 §4 rec 1, phase-03.1 §4 rec 2, phase-03.2 Task 5). The default decision recorded in this SPEC is **close in 04.3** — see §3 D5. The router filter's per-cluster proxy attribution (in error variants and in `tracing` log lines) is the natural use site.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 04.3's feature surface (= the full parent-phase-04 acceptance surface, minus the 04.1+04.2-already-done subset):

- (a) the new differential fixture `tests/fixtures/0008-http1-router-upstream/` is green;
- (b) the pre-existing differential fixtures `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, and `tests/fixtures/0007-http1-direct-response/` (with the matcher-bearing route added in 04.2) remain green;
- (c) no conformance suites run this sub-phase (first one — `h2spec` — attaches in phase 05);
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 04.1 (HCM + route_config + direct_response seeds) and 04.2 (`HeaderMatcher` seeds) plus 1 new 04.3 seed (`hcm_route_to_cluster.yaml` exercising the `RouteAction_Route` variant + `ConfigError::UnknownCluster` reject path);
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- (f) `REVIEW.md` for this sub-phase is approved.

**Parent ROADMAP row 04 flips to `done` in 04.3's state-6 phase-done commit** — per the ROADMAP-schema invariant ("the parent flips to `done` only after all sub-phases are `done`"); since 04.1 and 04.2 are `done` at 04.3 start (per the strict 04.1 → 04.2 → 04.3 ordering above), landing 04.3 `done` completes the parent. Mirrors phase 03's `ca81226`-shape close-out where the 03.2 commit also closed parent 03 in a single commit.

---

## 2. Behavior-contract scope for sub-phase 04.3

Sub-phase 04.3 makes **one** edit to `BEHAVIOR_CONTRACT.md`: the `Header allow-list` section gains one row.

| Header | Equivalence | Rationale |
|---|---|---|
| `x-envoy-upstream-service-time` | name-required, value-may-differ | Per-request upstream-side latency in milliseconds. envoy-rust measures from `Client::connect` start to last-response-byte-read end (computed in the router proxy arm before the response is written downstream). Envoy emits the same header (its semantics are documented at `https://www.envoyproxy.io/docs/envoy/v1.33.0/configuration/http/http_filters/router_filter#x-envoy-upstream-service-time`). Only present on responses that proxied through to an upstream cluster (NOT on `direct_response` paths — that's 04.1's surface where this header is never emitted). Both proxies emit on every router-proxy response; values diverge by measurement. |

After the 04.3 edit, the full allow-list (populated across phase 04 — `server` and `date` were added in 04.1; 04.2 added none; 04.3 adds `x-envoy-upstream-service-time`) reads:

| Header | Equivalence | Rationale |
|---|---|---|
| `server` | name-required, value-may-differ | Implementation-identifying. Both proxies emit `server: <name>`; envoy-rust default is `server: envoy-rust`, Envoy default is `server: envoy`. Lands in 04.1. |
| `date` | name-required, value-may-differ | Wall-clock non-determinism (RFC 7231 §7.1.1.2 IMF-fixdate). Lands in 04.1. |
| `x-envoy-upstream-service-time` | name-required, value-may-differ | (see above; lands in 04.3) |

**Headers NOT on the allow-list** (must be value-exact when present):

- `content-length` — for fixture 0008, the upstream response is from `http1-echo-server` whose body shape is deterministic (§3 D3 below), so `content-length` is value-exact.
- `content-type` — `http1-echo-server` emits `text/plain`; envoy-rust forwards verbatim; Envoy forwards verbatim; value-exact.
- `connection` — value-exact (`keep-alive` or `close`; driven by the request's `Connection:` header per HTTP/1.1 §6.1 — fixture 0008's request omits `Connection:`, so the HCM response carries `connection: keep-alive` on both sides).

**Equivalence-matrix dimensions touched** (no `BEHAVIOR_CONTRACT.md` matrix edits — just first-time exercise of the `x-envoy-upstream-service-time` allow-list row):

- Row 1 (Response status): exercised; fixture 0008 asserts `200` via `equivalence.response_status` in `expectations.yaml`.
- Row 2 (Response body): byte-exact; the helper's echo body is deterministic given the request bytes.
- Row 3 (Response headers): set-equal modulo allow-list; the allow-list constant `HEADER_ALLOW_LIST` in `tests/differential/src/lib.rs` (introduced in 04.1) gains the `x-envoy-upstream-service-time` row in lockstep with the `BEHAVIOR_CONTRACT.md` edit.

The other `BEHAVIOR_CONTRACT.md` subsections (`Stat-name mapping`, `Access log field mapping`, `xDS wire state machine`, `Timing tolerances`) remain unedited in 04.3.

---

## 3. Deliverables

### D1 — `envoy-http1::Client` (per-connection HTTP/1.1 client)

`crates/envoy-http1/src/client.rs` (new module; re-exported from `crates/envoy-http1/src/lib.rs`). Sole user of `httparse::Response::parse` in the workspace.

```rust
/// Per-connection plaintext HTTP/1.1 client. No pooling; one TCP connection
/// per upstream request (pooling is upstream-robustness-family territory,
/// out of phase 04 per parent SPEC §4).
pub struct Client;

impl Client {
    /// TCP-connect to `addr`. The `host` value is captured for the eventual
    /// `Host:` header on send_request. No bytes are sent during connect.
    pub async fn connect(addr: std::net::SocketAddr, host: &str)
        -> Result<ClientStream, Http1Error>;
}

pub struct ClientStream {
    stream: tokio::net::TcpStream,
    host: String,
    buf: bytes::BytesMut,
}

impl ClientStream {
    /// Serialize and write `request` (request-line + headers + optional
    /// CL-framed body), then read the response (status line + headers +
    /// CL-framed OR chunked body). The `Host:` header is sourced from the
    /// `host` captured at connect time unless `request` already carries one
    /// (case-insensitive match), in which case `request`'s value wins.
    pub async fn send_request(&mut self, request: crate::Request)
        -> Result<crate::Response, Http1Error>;
}
```

**Cargo deps** — no new direct deps. `httparse`, `bytes`, `tokio`, `thiserror`, `tracing` are already in `crates/envoy-http1/Cargo.toml` from 04.1.

**Body-forwarding logic.**

- *Request body (downstream → upstream): Content-Length only in 04.3.* `Request` carries an owned `body: bytes::Bytes` (the body was drained from the downstream connection by the router proxy arm before the upstream `send_request` call). The `Client` writes `Content-Length: <body.len()>` and the body bytes directly. Chunked-request-body forwarding from downstream to upstream is **explicitly deferred**: Envoy supports chunked requests but envoy-rust's first cut handles the simpler CL case. Fixture 0008's request is `Content-Length: 0` (a `GET /` with no body), which exercises the trivial-body path; non-zero CL bodies work via the same code path but are not covered by a fixture in 04.3.
- *Response body (upstream → downstream): Content-Length OR chunked, preserve framing.* The chunked READER is new in 04.3 and lives in `client.rs`: it reads chunk-size lines (hex digits + `\r\n`), reads exactly `<size>` body bytes + `\r\n` per chunk, terminates on a zero-size chunk, and does NOT read trailers (trailer forwarding is deferred — see §4 non-goals). The router proxy arm decides whether to write the response body downstream as CL or chunked based on what the upstream emitted; in 04.3 `http1-echo-server` always emits CL, so the chunked-WRITE-downstream path is exercised only by unit tests, not by fixture 0008.

**`Http1Error` variants — new in 04.3** (added to the `Http1Error` enum landed in 04.1):

```rust
#[derive(Debug, thiserror::Error)]
pub enum Http1Error {
    // ... 04.1 variants (MalformedRequestLine, MalformedHeader, HeadersTooLarge,
    //                   BodyTooLarge, UnexpectedEof, Io) unchanged ...

    /// 04.3 NEW — TCP-connect to upstream failed.
    #[error("connecting to upstream {addr}: {source}")]
    UpstreamConnect {
        addr: std::net::SocketAddr,
        #[source] source: std::io::Error,
    },

    /// 04.3 NEW — upstream's HTTP/1.1 response status line was malformed.
    #[error("malformed upstream response line")]
    MalformedResponseLine,

    /// 04.3 NEW — upstream's chunked-encoding framing violated RFC 7230 §4.1
    /// (e.g., non-hex chunk size, missing CRLF after chunk data, unexpected
    /// EOF mid-chunk).
    #[error("malformed chunked-encoding framing in upstream response")]
    MalformedChunkedFraming,
}
```

The `UpstreamHandshake` placeholder variant from parent SPEC §3 D8.3 is **not** added in 04.3 because plaintext HTTP/1.1 has no handshake; the parent SPEC tagged it as a placeholder for future TLS-on-upstream-HCM combos. If 04.3 surfaces such a need (unlikely — see §7), the variant lands then; otherwise it forwards to whichever phase first combines HTTP/1.1 with upstream TLS termination.

**Unit tests** in `crates/envoy-http1/src/client.rs::tests` (8 tests):

- `connect_succeeds_against_in_process_acceptor` — start a `tokio::net::TcpListener` on an ephemeral port; call `Client::connect(addr, "envoy-rust.test")`; assert no error.
- `connect_returns_upstream_connect_on_refused_port` — call `Client::connect("127.0.0.1:1", ...)` (kernel-refused port); assert `Http1Error::UpstreamConnect`.
- `send_request_writes_serialized_request_bytes` — start an in-process acceptor that records received bytes; build a `Request { method: "GET", path: "/", headers: [("user-agent", "test")], body: Bytes::new() }`; call `send_request`; assert the recorded bytes start with `"GET / HTTP/1.1\r\n"` and contain `"host: envoy-rust.test\r\n"` (host injected from the captured `host`) and `"user-agent: test\r\n"` and `"content-length: 0\r\n\r\n"`.
- `send_request_uses_request_host_when_provided` — same setup but `Request.headers` already includes `("Host", "explicit.example")`; assert the recorded bytes contain `"host: explicit.example\r\n"` (case-insensitive de-dup; explicit value wins; the captured `host` is dropped).
- `send_request_reads_cl_response_body` — acceptor responds with `"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello"`; assert returned `Response.body == "hello"`.
- `send_request_reads_chunked_response_body` — acceptor responds with `"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"`; assert returned `Response.body == "hello world"`.
- `send_request_returns_malformed_response_line_on_garbage` — acceptor responds with `"NOT AN HTTP RESPONSE"`; assert `Http1Error::MalformedResponseLine`.
- `send_request_returns_malformed_chunked_on_bad_size_line` — acceptor responds with `"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nXYZ\r\nhello\r\n"`; assert `Http1Error::MalformedChunkedFraming`.

**LoC budget.** ~250 LoC impl + ~250 LoC unit tests.

### D2 — Router filter "proxy to cluster" arm

Two changes coordinate to land this deliverable: a `envoy-config` schema extension (the `RouteAction::Route` variant) and the HCM router invocation site in `envoy-http1` extending from one match arm to two.

**Schema extension in `crates/envoy-config/src/bootstrap.rs`:**

```rust
/// 04.1 introduced this enum as DirectResponse-only.
/// 04.3 adds the Route variant.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum RouteAction {
    DirectResponse(DirectResponse),    // 04.1
    Route(RouteAction_Route),          // 04.3 NEW
}

/// 04.3 NEW. Names the cluster to forward the matched request to. Future
/// route-action knobs — timeout, retries, hedging, weighted clusters,
/// host-rewrite, request/response header manipulations — are all deferred
/// (§4 non-goals).
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteAction_Route {
    pub cluster: String,
}
```

The `Route` enum-variant casing matches Envoy's RouteAction oneof field name `route` after serde's typical kebab/snake normalization; the YAML fixtures express the choice via the route's `route: { cluster: backend }` shape (a peer of `direct_response: { ... }`).

**Validator extension** in `envoy_config::bootstrap::validate`:

- For each `Route.action == RouteAction::Route(action_route)`, look up `action_route.cluster` in the set of cluster names (`bootstrap.static_resources.clusters[*].name`); if not present, return `ConfigError::UnknownCluster { route: <vh.name>/<route.match.prefix-or-path>, cluster: action_route.cluster.clone() }`. The `UnknownCluster` variant **is reused from phase 02.1** (originally introduced for `TcpProxyConfig.cluster` validation; the variant carries `{ filter, cluster }` fields — at execution time the plan-writer decides whether to add a third `route` field, rename `filter` to `referrer`, or simply pass a synthesized referrer string like `"hcm.route(<vh>/<match>)"` — any of the three preserves backward compatibility for existing `TcpProxyConfig`-validation call sites).

**Validator unit tests** in `crates/envoy-config/src/bootstrap.rs::tests` (3 new tests):

- `parses_route_with_cluster_action` — happy-path bootstrap with HCM route action `route: { cluster: backend }` referencing a declared cluster; parse + validate succeed.
- `rejects_route_with_unknown_cluster` — same shape but `route: { cluster: nonexistent }`; expect `Err(ConfigError::UnknownCluster { ..., cluster: "nonexistent" })`.
- `rejects_route_action_with_both_direct_response_and_route` — fixture YAML with both `direct_response` and `route` keys on the same `Route` — expect `Err(serde error)` because `RouteAction` is a tagged-union enum and only one variant may be selected.

**Fuzz corpus extension.** `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_route_to_cluster.yaml` — happy-path bootstrap with HCM + `route: { cluster: backend }` + a `STATIC` cluster. (One seed; the unknown-cluster reject path is exercised by the validator's fuzz path which runs both parse and validate.)

**HCM router invocation site extension** in `crates/envoy-http1/src/hcm.rs` (or wherever 04.1 placed the hardcoded router invocation; the parent SPEC §6 signpost 17 lean is `crates/envoy-http1/src/hcm.rs`):

```rust
match route.action {
    RouteAction::DirectResponse(ref dr) => {
        // 04.1 path — unchanged. Write static body via Http1Response writer.
        write_direct_response(downstream_writer, dr, &request).await?;
    }
    RouteAction::Route(ref action_route) => {
        // 04.3 NEW.
        let cluster = cluster_mgr
            .get(&action_route.cluster)
            .expect("validator ensures cluster present");
        let endpoint = cluster
            .pick_endpoint()
            .ok_or_else(|| RouterError::NoHealthyEndpoint {
                cluster: action_route.cluster.clone(),
            })?;
        let host = request.headers.find("host").unwrap_or("").to_owned();
        let start = std::time::Instant::now();
        let mut client_stream = Client::connect(endpoint, &host).await
            .map_err(|source| RouterError::UpstreamConnect {
                cluster: action_route.cluster.clone(),
                source,
            })?;
        // body was already drained from downstream into request.body
        let upstream_response = client_stream.send_request(request).await
            .map_err(|source| RouterError::UpstreamRequestFailed {
                cluster: action_route.cluster.clone(),
                source,
            })?;
        let elapsed_ms = start.elapsed().as_millis();
        write_proxied_response(
            downstream_writer,
            upstream_response,
            elapsed_ms,
            connection_posture,
        ).await?;
    }
}
```

`write_proxied_response` applies the **header allow-list policy** in reverse: headers from upstream that envoy-rust would normally emit on its own (e.g., `server`, `date`) are *replaced* with envoy-rust's values (this matches Envoy's behavior — the upstream's `server` header is overwritten with envoy-rust's `server: envoy-rust`); other headers pass through verbatim. The new `x-envoy-upstream-service-time: <elapsed_ms>` header is appended just before the body. `Connection:` is set per the downstream request's connection posture (keep-alive vs close, captured before drain).

**`RouterError` enum** — new in 04.3 (introduced under `crates/envoy-http1/src/hcm.rs` or a new `crates/envoy-http1/src/router.rs` module; the plan-writer picks):

```rust
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("no healthy endpoint available for cluster '{cluster}'")]
    NoHealthyEndpoint { cluster: String },

    #[error("upstream connect failed for cluster '{cluster}': {source}")]
    UpstreamConnect { cluster: String, #[source] source: Http1Error },

    #[error("upstream request failed for cluster '{cluster}': {source}")]
    UpstreamRequestFailed { cluster: String, #[source] source: Http1Error },
}
```

The `cluster` field on each variant is what makes the D5 close-out (per-cluster log attribution) load-bearing — see §3 D5.

**Unit tests** in `crates/envoy-http1/src/{hcm,router}.rs::tests` (6 tests):

- `route_walk_dispatches_direct_response_unchanged` — regression test for the 04.1 path; ensure adding the `Route` arm doesn't break `DirectResponse` dispatch.
- `route_walk_dispatches_route_action_to_client_connect` — set up an in-process upstream acceptor; build a fake `cluster_mgr` returning a single endpoint pointing at the acceptor; build a `RouteConfiguration` with one VH + one route `route: { cluster: backend }`; drive a request through the HCM; assert the acceptor received the request and the downstream got the upstream's response back.
- `route_walk_returns_no_healthy_endpoint_when_cluster_empty` — cluster declared but no endpoints; assert `RouterError::NoHealthyEndpoint`.
- `route_walk_returns_upstream_connect_on_refused_port` — cluster's single endpoint points at `127.0.0.1:1`; assert `RouterError::UpstreamConnect`.
- `proxied_response_appends_x_envoy_upstream_service_time` — wire the path; assert the downstream-written response's headers contain `x-envoy-upstream-service-time` with a numeric value (don't pin the value; just assert presence and parseability).
- `proxied_response_overwrites_server_and_date_headers` — upstream returns `server: upstream-software/1.0` and `date: <some past time>`; assert the downstream-written response's `server` is `envoy-rust` and `date` is fresh (within ε of `Instant::now()`-formatted IMF-fixdate).

**LoC budget.** ~150 LoC impl (HCM/router extension + schema additions + validator) + ~120 LoC unit tests.

### D3 — `tests/helpers/http1-echo-server/` (new workspace member)

Sibling of `tests/helpers/tcp-echo-server/` (landed in phase 02.1) and `tests/helpers/tls-echo-server/` (landed in phase 03.2). Plaintext only (no TLS — fixture 0008's data plane is `plaintext downstream → plaintext upstream`). The cadence and skeleton match `tcp-echo-server`'s.

- `tests/helpers/http1-echo-server/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Deps: `envoy-http1 = { path = "../../../crates/envoy-http1" }` (consumes the codec + `Request`/`Response` types so the helper doesn't re-implement HTTP/1.1 parsing — and so `httparse` stays sole-deps under envoy-http1 per architectural rule 1), `anyhow`, `thiserror`, `tokio` (features `rt-multi-thread`, `net`, `io-util`, `macros`, `signal`), `tracing`, `tracing-subscriber`. Dev-deps: `tokio` adds nothing beyond runtime. (No `rcgen` or `tempfile` needed — no TLS.)

- `tests/helpers/http1-echo-server/src/main.rs` starts with `#![forbid(unsafe_code)]`. Contract:
  - **Hand-parsed argv mirroring `tcp-echo-server`'s shape from phase 02.1:** `--port <u16>` (required), `--help`, `--version`. `ArgvError` typed via `thiserror` (variants: `MissingFlag(&'static str)`, `MissingValue`, `InvalidPort`, `Trailing`, `HelpRequested`, `VersionRequested`).
  - **Runtime:** `tokio::net::TcpListener::bind(("127.0.0.1", port))`; accept loop with `tokio::select!` between `accept()` and `tokio::signal::ctrl_c()`; for each accepted stream, spawn onto a `tokio::task::JoinSet`: parse one HTTP/1.1 request via `envoy_http1::Http1Codec::parse_request`; build the deterministic echo response (see below); write the response via `envoy_http1::Http1Response`. After writing the response, close the connection (no keep-alive support — keeps the helper minimal; both proxies issue a single request per connection to fixture 0008's backend).
  - **On shutdown:** stop accepting, drain with `DRAIN_BUDGET = Duration::from_secs(5)`, abort stragglers, return 0.
  - **Logs on `stderr`** via `tracing_subscriber::fmt`.
  - **Exit codes:** `0` clean, `1` runtime error, `2` argv error. Mirrors `envoy-bin`'s, `tcp-echo-server`'s, and `tls-echo-server`'s convention.

**Response body format (LOAD-BEARING for differential equivalence).**

The helper produces a `200 OK` response with `Content-Type: text/plain` and a body of the following exact form:

```
method: <METHOD>
path: <PATH>
headers:
  <name1>: <value1>
  <name2>: <value2>
  ...
body: <BODY>
```

Where:

- `<METHOD>` is the request method as parsed (uppercase per HTTP/1.1 §3.1.1).
- `<PATH>` is the request-target as parsed (no normalization — `httparse`'s output verbatim).
- The headers list contains every request header, **sorted alphabetically by name (case-insensitive lowercase)**, one per line, with two-space indent + `<name>: <value>\n`. Sorting is critical for differential equivalence: both proxies forward the request to the SAME helper, but the order in which they emit headers on the upstream wire may differ (Envoy's HTTP/1.1 codec may re-order headers for canonicalization or filter-injection); sorting the helper's echo by header name eliminates this source of divergence so the byte-exact body equality holds. Lowercase normalization handles the case where one proxy emits `Host` and the other emits `host` — both render as `host:`.
- `<BODY>` is the request body bytes, decoded as UTF-8 if possible (else replaced byte-by-byte with `?`); for `Content-Length: 0` requests the body line reads `body: \n` (the trailing newline ensures a stable terminating byte).

**Worked example.** A request of bytes:

```
GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nContent-Length: 0\r\n\r\n
```

Produces a response with body bytes (newlines literal `\n`):

```
method: GET
path: /
headers:
  content-length: 0
  host: envoy-rust.test
body: 
```

(Note the trailing `body: ` line ends with a single space + `\n` even when the body is empty — the format is uniform.) This is the byte-exact body that fixture 0008's `expectations.yaml` asserts via `byte_exact` equivalence. Both Envoy and envoy-rust forward the same request to the same helper, get the same body back, and proxy it back to the harness verbatim — so the harness sees the same body bytes from both proxies.

**Unit tests** in `tests/helpers/http1-echo-server/src/main.rs::tests` (5 tests):

- `argv_parses_full_invocation` — `--port 10042` → `Ok(Args { port: 10042 })`.
- `argv_rejects_missing_port` — `--help` aside, no `--port` argv → `Err(ArgvError::MissingFlag("--port"))`.
- `argv_rejects_invalid_port` — `--port not-a-number` → `Err(ArgvError::InvalidPort)`.
- `argv_shows_help` — `--help` → `Err(ArgvError::HelpRequested)` (exit 0 path via main's translation).
- `accepts_and_echoes_request` — `#[tokio::test(flavor="multi_thread")]`: spawn the server in a task on a reserved port; open a `TcpStream`, write `b"GET / HTTP/1.1\r\nHost: x.test\r\nContent-Length: 0\r\n\r\n"`; read until EOF; parse the response; assert status `200`, `content-type: text/plain`, body byte-exact `"method: GET\npath: /\nheaders:\n  content-length: 0\n  host: x.test\nbody: \n"`.

**LoC budget.** ~150 LoC impl + ~150 LoC unit tests.

### D4 — Differential harness `Http1EchoBackend` + fixture 0008

**`Http1EchoBackend`** in `tests/differential/src/backend.rs` (sibling of `TcpProxyBackend` from phase 02.2 and `TlsEchoBackend` from phase 03.2):

```rust
pub struct Http1EchoBackend {
    port: u16,
    child: tokio::process::Child,
}

impl Http1EchoBackend {
    /// Locate the http1-echo-server binary at workspace target/<profile>/
    /// http1-echo-server (via locate_http1_echo_server() helper); reserve a
    /// port; spawn the binary as a subprocess; wait for accept-readiness via
    /// wait_accept_ready (existing helper from phase 02.2) plus a probe
    /// HTTP/1.1 request (write a `GET /healthz HTTP/1.1\r\nHost: probe\r\n
    /// Content-Length: 0\r\n\r\n`, expect a `200 OK` back) — the probe is
    /// what distinguishes Http1EchoBackend's readiness from TcpProxyBackend's
    /// (which only checks TCP-accept).
    pub async fn spawn() -> anyhow::Result<Self>;

    pub fn port(&self) -> u16;

    /// Mirrors TcpProxyBackend / TlsEchoBackend: reachable from the upstream
    /// Envoy container at "host.docker.internal" per ADR-0015, and from the
    /// envoy-rust host subprocess at "127.0.0.1".
    pub fn container_host(&self) -> &'static str;
}

impl Drop for Http1EchoBackend {
    /// Same SIGKILL-on-Drop posture as TcpProxyBackend / TlsEchoBackend
    /// (per phase-02.2 + 03.2 backends — the M1 carryforward concern about
    /// std::thread::sleep from a tokio thread inherits unchanged; tracked
    /// forward to whichever phase first parallelizes run_fixture).
    fn drop(&mut self) { /* ... */ }
}

/// Locator helper mirroring locate_tls_echo_server() from phase 03.2 and
/// locate_tcp_echo_server() from phase 02.2.
pub(crate) fn locate_http1_echo_server() -> anyhow::Result<std::path::PathBuf>;
```

**`Driver::Http1` extension.** The `Driver::Http1` variant landed in 04.1 already accommodates the fixture 0008 use case — its `expected_status`, `expected_body`, `expected_headers` fields are sufficient. The harness compares the response body byte-exact (no need for a `byte_exact_with_request_echo` body-rule because the helper's echo is fully deterministic given the request bytes; comparing the captured upstream response body byte-equal across both proxies is the same as comparing each side's body to the deterministic-echo expectation). The existing `assert_equivalence` + `diff_headers` allow-list-aware comparison from 04.1 governs the headers; the allow-list constant `HEADER_ALLOW_LIST` gains the `x-envoy-upstream-service-time` row in lockstep with the BEHAVIOR_CONTRACT.md edit (§2 above).

**`run_fixture` dispatch.** Detection cascade extended:

1. If either rendered template references `{{CA_PATH}}` / `{{LEAF_*_PATH}}` / `{{SERVER_*_PATH}}`, build `TlsTestPki::generate()?` (03.1/03.2 path; existing).
2. If either rendered template references `{{BACKEND_PORT}}`, spawn `TcpProxyBackend` (02.2 path; existing).
3. If either rendered template references `{{TLS_BACKEND_PORT}}`, spawn `TlsEchoBackend` (03.2 path; existing).
4. **(04.3 NEW)** If either rendered template references `{{HTTP1_BACKEND_PORT}}`, spawn `Http1EchoBackend::spawn()`; substitute `{{HTTP1_BACKEND_PORT}}` → assigned port and `{{BACKEND_HOST}}` → `host.docker.internal` for envoy-side / `127.0.0.1` for envoy-rust-side. (The plan-writer may opt to reuse `{{BACKEND_PORT}}` instead of introducing `{{HTTP1_BACKEND_PORT}}`, dispatching by an `expectations.yaml` flag like `backend_kind: http1_echo` instead — either shape is acceptable; the new `{{HTTP1_BACKEND_PORT}}` key is the recommended default since it mirrors phase 03.2's `{{TLS_BACKEND_PORT}}` precedent and keeps the dispatch mechanical.)

**Fixture `tests/fixtures/0008-http1-router-upstream/`** — 5 files:

- `envoy.yaml`:

    ```yaml
    node: { id: envoy-rust-phase-04.3-fixture-0008, cluster: envoy-rust-phase-04.3 }
    admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
    static_resources:
      listeners:
        - name: http1_listener
          address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
          filter_chains:
            - filters:
                - name: envoy.filters.network.http_connection_manager
                  typed_config:
                    "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                    stat_prefix: ingress_http1
                    codec_type: HTTP1
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
          type: STATIC
          lb_policy: ROUND_ROBIN
          load_assignment:
            cluster_name: backend
            endpoints:
              - lb_endpoints:
                  - endpoint:
                      address: { socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP1_BACKEND_PORT}} } }
    ```

- `envoy-rust.yaml` — same shape with the per-side divergences (no admin block; bind `127.0.0.1`; backend host `127.0.0.1`).

- `inputs/payload.bin` — raw bytes:

    ```
    GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nContent-Length: 0\r\n\r\n
    ```

  (Same shape as parent SPEC §6 signpost 10's worked example; `drive_http1` reads it from disk and writes it directly onto the socket. The fixture-author retains control of the wire-format request bytes — keeps the harness simple.)

- `expectations.yaml`:

    ```yaml
    driver:
      kind: http1
      method: GET
      path: "/"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body:
        byte_exact: "method: GET\npath: /\nheaders:\n  content-length: 0\n  host: envoy-rust.test\nbody: \n"
      expected_headers:
        rule: set_equal_modulo_allow_list
    equivalence:
      response_status: exact
      response_body: byte_exact
      response_headers: set_equal_modulo_allow_list
    ```

  Note: the `expected_body` byte-exact value is the deterministic helper echo (per §3 D3 above). The fixture author writes this expectation directly; the harness asserts both proxies' bodies match this expectation (and, transitively, each other).

- `README.md` — names the property (HTTP/1.1 upstream proxying via the router filter; per-cluster routing; deterministic helper echo; `x-envoy-upstream-service-time` allow-list row exercised), the `http1-echo-server` helper's role, and the ADR cross-references (ADR-0015 cross-container-host reachability, ADR-0020 split decision; no ADR-0021 dependency since fixture 0008's route uses `prefix: "/"` only — the matcher fan-out is exercised by 04.2's amendment to fixture 0007).

**Docker-gated integration test** `tests/differential/tests/http1_router_upstream.rs` — sibling of `tls_upstream.rs` / `tls_sni.rs`; same `#[ignore]`-unless-`DOCKER=1` gating pattern; calls `run_fixture("0008-http1-router-upstream")`.

**Harness unit tests** in `tests/differential/src/{backend,lib}.rs::tests` (4 new tests):

- `http1_echo_backend_spawns_and_echoes` — spawn `Http1EchoBackend`, open a TCP connection, write a full HTTP/1.1 request, read response, assert status `200` + body byte-exact deterministic-echo. Mirrors phase-02.2's `tcp_proxy_backend_spawns_and_echoes` and phase-03.2's `tls_echo_backend_spawns_and_echoes`.
- `http1_echo_backend_drop_terminates_child` — spawn, drop, assert child process exited.
- `locate_http1_echo_server_returns_existing_path` — assert the locator helper finds the binary in `target/debug/` (or `target/release/`).
- `run_fixture_dispatches_http1_backend_on_template_marker` — feed a synthetic template with `{{HTTP1_BACKEND_PORT}}` and assert the dispatch cascade selects `Http1EchoBackend::spawn`.

**`crates/envoy-bin/tests/http1_router_upstream.rs`** — Docker-free in-process integration test (sibling of 04.1's `http1_direct_response.rs` and phase 03.2's `tls_upstream.rs`). Spawn an in-process `tokio` HTTP/1.1 echo server on an ephemeral port (or use the locator to spawn the real `http1-echo-server` binary as a subprocess); spawn `envoy-bin` as a subprocess via `CARGO_BIN_EXE_envoy-bin` with a config that points at the in-process upstream and includes the HCM `route: { cluster: backend }` route action; open a plaintext TCP connection to envoy-bin's listener; write the request bytes; read the response; assert status + body. ~120 LoC + 1 test (`proxies_get_through_router_to_http1_echo_backend`).

**LoC budget.** ~200 LoC harness (Http1EchoBackend + locator + run_fixture dispatch + envoy-bin test) + ~80 LoC tests (4 harness tests + 1 envoy-bin integration test) + 5 fixture files.

### D5 — `Cluster::name()` opportunistic close-out (the multi-phase carryforward)

**Background.** The `Cluster::name()` accessor has been a multi-phase carryforward since phase 02.1's REVIEW.md identified M1 (add `pub(crate) fn Cluster::name(&self) -> &str` and remove the field-level `#[allow(dead_code)]` from `crates/envoy-cluster/src/cluster.rs`). Subsequent re-deferrals (each documented in `docs/envoy-rust/STATE.md` Notes section "Phase-02.1 / 02.2 / 03.1 / 03.2 rollovers"):

- **Phase 02.1 REVIEW M1** — original deferral; "tracked forward to phase 03.2 (opportunistic) or phase 06 (default)".
- **Phase 02.2 §4 recommendation 1** — "add `Cluster::name()` accessor when phase 03.2's TLS work or phase 06's stats first need it".
- **Phase 03.1 §4 recommendation 2** — re-deferred unchanged; "phase 03.2 D4 is the next opportunistic close site".
- **Phase 03.2 Task 5** — explicitly evaluated and re-deferred per phase 03.2 SPEC §3 D4. Phase 03.2's `TcpProxy::with_upstream_tls` chose to wrap upstream-TLS handshake errors in `TcpProxyError::UpstreamTlsHandshake { source }` *without* a `cluster: String` field; the per-cluster attribution was deemed not load-bearing for the phase-03.2 surface.
- **Phase 04 (parent SPEC §3 D12.3)** — designates 04.3 as "the next opportunistic close site" and recommends the **close** decision in 04.3 because "the router filter's per-cluster proxy attribution is the natural use site".

**Decision recorded in this SPEC: close M1 in 04.3.** Rationale:

1. **Per-cluster log attribution materially helps debugging.** The router proxy arm's `tracing::warn!(cluster = ..., addr = ..., source = ?, "upstream proxy error")` log lines on per-cluster proxy errors make operational debugging materially easier than the pre-04.3 alternative of reverse-engineering the cluster from the endpoint address.
2. **The new `RouterError` enum (§3 D2) ALREADY carries `cluster: String` on three variants** (`NoHealthyEndpoint`, `UpstreamConnect`, `UpstreamRequestFailed`). Per-route the cluster string is known from the `RouteAction_Route.cluster` field and is plumbed into the variants directly, but the symmetric posture (a `Cluster::name()` accessor on the resolved `ClusterHandle` so the caller doesn't have to re-thread the string) is the natural shape — and the natural shape removes the field-level `#[allow(dead_code)]` that has been outstanding since phase 02.1.
3. **The change is small** — ~10 LoC of `envoy-cluster` delta plus ~30 LoC of consumer wiring (the router filter's per-cluster log attribution lines, plus optional adoption in existing `TcpProxyError::*` variants if the plan-writer chooses to backfill them; the parent SPEC §3 D12.3 marks the backfill as not-required-but-permitted).

**The change.**

```rust
// crates/envoy-cluster/src/cluster.rs

pub struct Cluster {
    name: String,    // was annotated #[allow(dead_code)] since phase 02.1
    // ... endpoints, lb state, etc. unchanged ...
}

impl Cluster {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}
```

Plus removal of the field-level `#[allow(dead_code)]` annotation. The `pub(crate)` visibility matches the existing `pub(crate)` accessors on `Cluster`; if the consumers (`envoy-tcp` for TcpProxyError, `envoy-http1` for the new RouterError) live in different crates, the visibility lifts to `pub` and the accessor exposes through the `ClusterHandle` re-export from `envoy-cluster`'s public API. The plan-writer picks the visibility based on consumer placement.

**Decision recording.** The 04.3 PROGRESS.md and REVIEW.md document the close-in-04.3 decision with cross-references to phase-02.1 REVIEW M1, phase-02.2 §4 rec 1, phase-03.1 §4 rec 2, and phase-03.2 Task 5 (deferred). The carryforward chain in `docs/envoy-rust/STATE.md` Notes section gets a final entry: "Phase-04.3 rollovers — M1 closed (close-out commit `<SHA>`); carryforward chain ends here." If, during execution, a use case falls out (e.g., the `RouterError` variants do not in fact need `Cluster::name()` because the `cluster` string is already in scope at every error-construction site, with no symmetric wins from the accessor), the plan-writer may re-defer to phase 06 — but the SPEC's recommended default is **close**.

**LoC budget.** ~10 LoC `envoy-cluster` delta + ~30 LoC consumer wiring. ~3 unit tests appended to `crates/envoy-cluster/src/cluster.rs::tests` if the visibility lifts to `pub`:

- `cluster_name_returns_configured_name` — build a `Cluster` with name `"backend"`, assert `.name()` returns `"backend"`.
- `cluster_handle_exposes_name` — round-trip through `ClusterHandle`'s public surface.
- `cluster_name_outlives_borrow_correctly` — borrow-check regression guard.

---

**Total 04.3 budget: ~17 tasks, ~1500 LoC** (parent SPEC §5 projection):

| Surface | Net LoC (impl + tests) |
|---|---|
| envoy-http1::Client (D1: connect/send_request + chunked-response reader + 5 new Http1Error variants − UpstreamHandshake + 8 unit tests) | ~250 + ~250 |
| envoy-config schema + validator (D2 schema portion: RouteAction_Route variant + UnknownCluster reuse + 3 validator tests + 1 fuzz seed) | ~50 + ~30 |
| HCM router invocation extension + RouterError enum (D2 HCM portion: two-arm match + write_proxied_response + 6 unit tests) | ~100 + ~90 |
| http1-echo-server helper (D3: argv parser + accept loop + deterministic echo body + 5 unit tests) | ~150 + ~150 |
| Differential harness Http1EchoBackend + locate_http1_echo_server + run_fixture dispatch + 4 harness unit tests + envoy-bin in-process test (D4 harness portion) | ~150 + ~80 + ~120 |
| Fixture 0008 (5 files; envoy.yaml, envoy-rust.yaml, payload.bin, expectations.yaml, README.md) | ~80 |
| Docker-gated integration test `tests/differential/tests/http1_router_upstream.rs` | ~30 |
| Cluster::name() close-out (D5: ~10 LoC envoy-cluster + ~30 LoC consumer wiring + 3 optional tests) | ~40 + ~30 |
| BEHAVIOR_CONTRACT.md edit + HEADER_ALLOW_LIST constant update (D2 contract portion) | ~10 |
| **Total** | **~1490 LoC; ~17 tasks** |

Both `BOOTSTRAP_PROMPT.md` §6.1 gates (> ~25 tasks OR > ~1500 LoC) hold comfortably at ~17 tasks / ~1490 LoC.

---

## 4. Non-goals (deferred to later phases)

Out of phase 04 entirely, inherited from parent SPEC §4 (the subset that concerns 04.3's surface):

- **Connection pooling** on the upstream side (per-cluster `ConnectionPool` with idle-conn reuse, per-host limits, per-connection-max-requests). The `Client` in 04.3 is per-connection one-shot. **Upstream-robustness family.**
- **Retries** on the router action (`route.retry_policy`). Upstream-robustness family.
- **Hedging** (`hedge_on_per_try_timeout`). Upstream-robustness family.
- **Request timeouts** (`route.timeout`, `route.idle_timeout`). Upstream-robustness family.
- **Per-try timeouts** (`route.retry_policy.per_try_timeout`). Upstream-robustness family.
- **Weighted clusters** (`route.weighted_clusters` instead of `route.cluster`). Out of phase 04; future routing/LB family.
- **Request / response header manipulations on the route** (`request_headers_to_add`, `request_headers_to_remove`, `response_headers_to_add`, `response_headers_to_remove`, `most_specific_header_mutations_wins`). HTTP-filters family or a follow-on phase. *Note:* 04.3's `Host:` header forwarding is in-scope (essential for upstream HTTP/1.1) but is implicit in the request-passthrough — not a header-manipulation knob.
- **Chunked-request-body forwarding from downstream to upstream.** 04.3's `Client` only writes `Content-Length`-framed request bodies. Reading chunked bodies from downstream and re-framing them as chunked to upstream is deferred (Envoy supports this; envoy-rust's first cut handles the simpler CL case). Fixture 0008 exercises `Content-Length: 0` only.
- **Trailer forwarding** (request trailers downstream-to-upstream; response trailers upstream-to-downstream). Per HTTP/1.1 §4.1.2, trailers are emitted after the final chunk in chunked-encoding bodies. 04.3's chunked-response reader **does not** forward trailers (it stops after the zero-size chunk and discards any trailer bytes). Trailer support is deferred to whichever phase first surfaces a fixture-driven need (HTTP/2, where trailers are mandatory; or an HTTP/1.1 fixture targeting a backend that emits gRPC-Web trailers).
- **WebSocket upgrades** (`Upgrade: websocket` request header handling, 101-Switching-Protocols response). Out of phase 04.
- **HTTP CONNECT method** (for proxying TLS through HTTP). Out of phase 04.
- **`100-Continue`** request expectations (the `Expect: 100-continue` request header + the interim 100 response). Out of phase 04.
- **Pipelining** (per HTTP/1.1 §6.3.2 — multiple requests sent before responses on a single connection). Both proxies serialize requests on a connection (one-at-a-time per spec) — no fixture exercises pipelining and the `Client` does not support it.
- **HTTP filter chain framework** (`Vec<Box<dyn HttpFilter>>` per-listener; iteration protocol with `Continue` / `StopIteration` / `StopAllIterationAndBuffer` states; extension registry; per-route `typed_per_filter_config`; per-virtual-host `typed_per_filter_config`). Phase 07. envoy-config still parses `http_filters: [{ name: "envoy.filters.http.router", ... }]` (Envoy fixtures require it as YAML input); the validator just rejects any other filter name with `ConfigError::UnsupportedHttpFilter` (landed in 04.1). When phase 07 lands the chain abstraction, the hardcoded HCM call site refactors into a chain-iteration call site.
- **Multiple HTTP filters in `http_filters`.** 04.x's HCM accepts exactly one filter (the router); the chain framework landing in phase 07 lifts this restriction.
- **Access logs** (the `access_log` field on HCM). Phase 06.
- **Tracing** (the `tracing` field on HCM, distributed-tracing spans). Observability family.
- **xDS-driven RDS** (RouteConfiguration delivered via xDS). xDS family.
- **Wildcard `domains: ["*.example.com"]` matching** on virtual hosts. 04.x supports `["*"]` (catch-all) or exact-string matching only.
- **HTTP/2 and HTTP/3.** `codec_type: HTTP2` and `codec_type: HTTP3` reject with `ConfigError::UnsupportedCodecType` (landed in 04.1). Phase 05 (HTTP/2) and the QUIC family.
- **Multiple HCM listeners.** Phase 02.1's `TooManyListeners` cap is unchanged in phase 04.
- **TLS on upstream HTTP/1.1** (the combination of HCM + upstream TLS termination). The `TlsAcceptingHandler` adapter from phase 03.1 already handles HCM on the downstream side; the upstream-TLS plumbing from phase 03.2 already wraps `TcpStream` with `tokio_rustls::client::TlsStream`. Combining the two — i.e., the router's `Client::connect` returning a TLS-wrapped stream when the cluster has `transport_socket: UpstreamTlsContext` — is a small extension (likely ADR-0022; see §7) but is **not** exercised in 04.3's fixture surface (fixture 0008 is plaintext upstream).
- **HCM `server_name` config field** (controls the `Server:` response header literally). Deferred per parent SPEC §3 D2.1 / §4 to phase 05+.
- **`x-envoy-original-path` / `x-envoy-original-host` / `x-forwarded-for` / `x-forwarded-proto` / `x-request-id`** request-header injection. Envoy emits some of these by default; envoy-rust's HCM does not in 04.x. If fixture 0008 surfaces a divergence on these headers (Envoy adds them; envoy-rust does not), the resolution is one of: (a) extend envoy-rust to emit them; (b) extend the BEHAVIOR_CONTRACT.md allow-list. The default plan is (b) for any header that surfaces during fixture 0008 development; (a) lands as a follow-on if production-realism demands it.

The 04.3 plan-writer may surface small additional non-goals at execution time; they will be enumerated in 04.3 PROGRESS.md as deferrals.

---

## 5. Splitting guidance for the planner

Estimated scope (per §3 totals table above): **~17 tasks, ~1490 LoC.** Both `BOOTSTRAP_PROMPT.md` §6.1 gates (> ~25 tasks OR > ~1500 LoC) hold comfortably.

**Do not split 04.3 further.** Per parent SPEC §5, sub-phases produced by an already-split parent should not nest-split — nested splits of an already-split sub-phase were not anticipated at the parent-phase brainstorm and deserve a fresh root-cause analysis (scope creep vs. planner overdecomposition). If the plan as actually written crosses either §6.1 gate mid-write, invoke `superpowers:systematic-debugging` first per `BOOTSTRAP_PROMPT.md` §6.1; the parent SPEC §5 explicitly avoids the nesting anti-pattern by choosing the 3-way flat split (codec/HCM → matchers → upstream) over the alternative 04.α nested-split shape.

The 04.3 planner inherits the parent SPEC §5 ordering posture: `04.1 → 04.2 → 04.3` strict; 04.3 is the closing sub-phase; 04.3's state-6 phase-done commit also flips parent ROADMAP row `04` to `done`.

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the planner resolves them in-plan rather than mid-execution.

1. **Task ordering for 04.3.** envoy-config schema additions (D2 schema portion: `RouteAction_Route` + 3 validator tests + 1 fuzz seed) → envoy-http1::Client implementation (D1: client.rs + 8 unit tests) → HCM router invocation extension + RouterError enum (D2 HCM portion: two-arm match + write_proxied_response + 6 unit tests) → BEHAVIOR_CONTRACT.md edit + HEADER_ALLOW_LIST constant update → http1-echo-server helper crate (D3: argv parser + accept loop + deterministic echo + 5 unit tests) → differential harness Http1EchoBackend + locator + run_fixture dispatch + 4 harness tests (D4 harness portion) → envoy-bin in-process integration test `http1_router_upstream.rs` → fixture 0008 + Docker-gated integration test (D4 fixture portion) → `Cluster::name()` close-out folded into the HCM router invocation task block per phase-02.2 task-11 precedent (D5) → state-4 phase-done gate (Cargo.lock sync per phase-precedent) → state-5 REVIEW → state-6 phase-done commit (parent ROADMAP row `04` flips to `done` in the same commit per parent SPEC §1).

2. **`envoy-http1` is the SOLE workspace dep on `httparse`** — established in 04.1 as architectural rule 1. The new `Client::send_request` calls `httparse::Response::parse` (first use site of the response parser; 04.1 only used the request parser). No other crate calls `httparse::*` directly. envoy-bin and envoy-config consume `envoy-http1`'s public types; `http1-echo-server` consumes via `envoy-http1` dep. Reviewer should flag any new `httparse` import in any other crate.

3. **`#![forbid(unsafe_code)]` is mandatory** at every new crate's `lib.rs` / `main.rs`: `tests/helpers/http1-echo-server/src/main.rs`. Same as 04.1's discipline for `crates/envoy-http1/src/lib.rs` and 03.1's discipline for `crates/envoy-tls/src/lib.rs`.

4. **Workspace membership.** Root `Cargo.toml` `[workspace] members` grows by `tests/helpers/http1-echo-server` (04.3). `crates/envoy-http1` was added in 04.1.

5. **`Host:` header forwarding posture.** The router proxy arm captures the downstream request's `Host:` header value before the request is consumed by `Client::send_request`. The captured value is passed as the `host` argument to `Client::connect`; `send_request` emits it as the `Host:` header on the upstream wire UNLESS the downstream request explicitly carries a `Host:` header that survives normalization (which it always does — `Host:` is mandatory per HTTP/1.1 §5.4 and 04.1's HCM rejects requests without it with `400 Bad Request`). The captured-at-connect `host` is therefore primarily used as a fallback for synthetic test cases (e.g., D1's `connect_succeeds_against_in_process_acceptor`); in production, the request's `Host:` always wins. Plan-writer should NOT add a `request_headers_to_add: { host: <override> }` knob — that's host-rewrite (see §4 non-goals).

6. **`x-envoy-upstream-service-time` measurement window.** envoy-rust measures from `Client::connect` start (= the moment `tokio::net::TcpStream::connect(addr).await` is invoked) to the moment `Client::send_request` returns the parsed `Response` (= last-response-byte-read end). Wall-clock `std::time::Instant::now()` deltas, formatted as integer milliseconds via `elapsed.as_millis()`. Envoy's measurement may differ slightly (Envoy may exclude or include connect time differently); the allow-list rule (`name-required, value-may-differ`) accommodates this divergence. Both proxies emit on every router-proxy response.

7. **Header re-emit policy in `write_proxied_response`.** The router proxy arm builds the downstream response by:
   1. Starting with the upstream's response status line (forward verbatim).
   2. For each upstream header, if the header name is in the **HCM-emitted set** (`server`, `date`), replace with envoy-rust's value; otherwise pass verbatim.
   3. Append `x-envoy-upstream-service-time: <ms>`.
   4. Set `Connection:` per the downstream request's connection posture (captured before the request body was drained).
   5. Forward the body, preserving the upstream's framing (CL or chunked).

   This matches Envoy's behavior of overwriting `server` and `date` on proxied responses. The plan-writer codifies the HCM-emitted set as a `const HCM_EMITTED_HEADERS: &[&str] = &["server", "date"]` in `envoy-http1::hcm` (or `router`).

8. **`http1-echo-server` response shape — alphabetical header sort is LOAD-BEARING.** Per §3 D3 the helper's response body sorts request headers alphabetically by case-insensitive lowercase name. Without this sort, the byte-exact body assertion would be sensitive to the order in which Envoy's HTTP/1.1 codec emits headers on the upstream wire vs. envoy-rust's codec. Both proxies forward the same logical request to the same helper, but Envoy may reorder headers (canonicalization, filter-injection) — sorting in the helper neutralizes this. Reviewer should flag any "optimization" that removes the sort.

9. **`http1-echo-server` is single-purpose plaintext echo, no keep-alive.** Both Envoy and envoy-rust (for fixture 0008) issue one HTTP/1.1 request per upstream connection (because 04.3's `Client` doesn't pool — see §4 non-goals). The helper closes the connection after sending the response. This matches `tcp-echo-server`'s posture of close-on-shutdown (the difference being the helper actively closes after one request, vs. `tcp-echo-server` echoing until the client closes). The plan-writer should NOT add keep-alive support to the helper — keeping it minimal aligns with `tcp-echo-server`'s phase-02.1 minimalism.

10. **Fixture 0008's `payload.bin` is a serialized HTTP/1.1 request line + headers** (parent SPEC §6 signpost 10). `drive_http1` (landed in 04.1) reads it from disk and writes it directly onto the socket. This sidesteps having `drive_http1` know how to construct an HTTP/1.1 request from structured fields — keeps the harness simple and the wire-format under fixture-author control.

11. **Differential body comparison shape.** The harness compares the response body byte-exact. There is no need for a `byte_exact_with_request_echo` body-rule (which the parent SPEC §3 D11.3 mentioned as one option) because the helper's echo is fully deterministic given the fixture's `payload.bin` request bytes. The fixture-author writes the expected echo body directly into `expectations.yaml`'s `expected_body.byte_exact` field; if the helper's response shape changes, the expectation gets regenerated — a deliberate trade-off favoring fixture-stability over harness-cleverness.

12. **`Driver::Http1`'s `expected_body` and `expected_headers` are per-side (the harness drives both proxies and asserts each side's response matches the expectation; cross-side equivalence falls out).** This is the same shape as `Driver::TcpEcho` (where each side's echo is asserted byte-equal to the input payload, and cross-side equivalence falls out). The 04.3 fixture relies on this discipline; the plan-writer should NOT introduce a "diff envoy vs. envoy-rust" comparison mode for fixture 0008.

13. **Connection lifecycle on the downstream side (HCM-ward) is HTTP/1.1 keep-alive default** — established in 04.1. envoy-rust serves keep-alive unless the request carries `Connection: close`. Idle-connection 5s timeout reading next request line. Fixture 0008's request omits `Connection:`, so the response carries `connection: keep-alive` and the connection stays open after the response — the harness's `drive_http1` reads exactly the response (status line + headers + CL-framed body) and closes the socket from its side; envoy-bin's idle-timeout reaps the connection after.

14. **Connection lifecycle on the upstream side (Client-ward) is one-shot: connect, send, receive, drop.** Per §4 non-goals, no pooling. The `Client::connect` returns a `ClientStream`; calling `send_request` once consumes the stream's request semantics; the stream is dropped after the response is fully read (the underlying `TcpStream` closes). If 04.3's `Client` ever wants to reuse a stream for a second request (it does not in 04.3's fixture; this is a design hint for future pooling work), the API would lift to `&mut self` on `send_request` and add a `close()` method — but that's deferred entirely.

15. **Body limits on the upstream-response side.** envoy-http1's existing `BodyTooLarge` and `HeadersTooLarge` errors (introduced in 04.1) enforce defaults: headers ≤ 8 KiB, body unlimited. The chunked-encoding reader in `Client` enforces the same defaults: `HeadersTooLarge` if the response headers exceed 8 KiB; body is unlimited (the chunked reader streams until the zero-size chunk; for `http1-echo-server`'s deterministic-echo body, this is bounded by request size, which is bounded by `HeadersTooLarge` — so the unlimited-body posture is safe). Knobs to make these configurable (`per_request_buffer_limit_bytes`) defer to upstream-robustness or HCM-modest-fields phase.

16. **`Cluster::name()` accessor close-out** (per §3 D5): close in 04.3 is the recommended default. The change is small (~10 LoC in envoy-cluster + ~30 LoC of consumer wiring). The carryforward chain in `STATE.md` Notes section gets a final entry: "Phase-04.3 rollovers — M1 closed (close-out commit `<SHA>`); carryforward chain ends here." If, mid-execution, the plan-writer decides the accessor isn't load-bearing (e.g., the `RouterError` variants already carry `cluster: String` from the source — which they do per §3 D2's enum definition — so the symmetric `Cluster::name()` accessor is purely cosmetic), re-deferral to phase 06 is permitted. Document the decision either way.

17. **`anyhow` boundary** at envoy-bin's integration tests. `crates/envoy-bin/tests/http1_router_upstream.rs` is in the binary crate's package and may use `anyhow` (D-3.2 permits `anyhow` only in `envoy-bin`). The `tests/differential/` crate continues phase-00's harness-wide `anyhow` usage — `Http1EchoBackend::spawn` returns `anyhow::Result<Self>` consistent with `TcpProxyBackend` / `TlsEchoBackend`'s phase-02.2 / 03.2 posture.

18. **Cargo.lock sync at state-4** per the established phase-precedent (mirrors phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85633e6`/`85faf6` — actually `85633e6`/`85faf6` placeholders; the real precedent is `85633e6` from phase 03.2 if not the explicit `85faf6` — confirm at execution time by `git log --oneline -- Cargo.lock`). New transitive surface in 04.3 from the `http1-echo-server` package stanza (which adds an `envoy-http1` dep that's already in the workspace, plus tracing/anyhow/thiserror that are already in scope; expect a minimal diff).

19. **`x-envoy-upstream-service-time` allow-list addition is in lockstep** with the BEHAVIOR_CONTRACT.md edit (§2 above) and the `HEADER_ALLOW_LIST` constant in `tests/differential/src/lib.rs` (introduced in 04.1). Both edits land in the same commit (or in adjacent commits within the same task block) so the harness asserts the contract that's documented. Reviewer should diff the BEHAVIOR_CONTRACT.md table against the constant for parity.

20. **Phase-04 fixture YAMLs use `static_resources.listeners[0].filter_chains[0].filters[0]` of name `envoy.filters.network.http_connection_manager`** (sibling of `envoy.filters.network.tcp_proxy` in fixtures 0003-0006) — established in 04.1. The HCM's `typed_config` carries the route_config inline (not RDS). Fixture 0008's HCM carries one route with `route: { cluster: backend }`; fixture 0007's (post-04.2) HCM carries two routes — the original `direct_response` route from 04.1 plus the matcher-bearing route from 04.2.

21. **No new envoy-tls / envoy-tcp deps surface in 04.3.** 04.3's upstream is plaintext; the existing `envoy-tcp::TcpProxy::with_upstream_tls` (landed in 03.2) is irrelevant to the HTTP/1.1 router. envoy-tls is consumed transitively (envoy-bin still uses it for fixtures 0004/0005/0006/0007's TLS surface) but no new code in 04.3 calls into envoy-tls. If a future fixture combines HCM + upstream TLS (the small extension noted in §4 non-goals; likely ADR-0022 — see §7), it would land then.

22. **`http1-echo-server` binary location at runtime.** `Http1EchoBackend::spawn` looks for the binary at `target/<profile>/http1-echo-server` (workspace root). In CI, the binary is built as part of `cargo test --workspace` (the helper crate's `cargo build` is implicit because it's a workspace member). The locator helper `locate_http1_echo_server()` mirrors `locate_tls_echo_server()`'s phase-03.2 implementation: walk up from `CARGO_MANIFEST_DIR` to find the workspace root, then check `target/debug/` and `target/release/` for the binary. Returns `anyhow::Error` if not found; the test fails with a clear message instead of timing out.

---

## 7. ADRs expected from this sub-phase

**No anticipated ADRs in 04.3.** ADR-0020 (split decision) landed at parent-04 state-2 alongside the sub-phase SPECs; ADR-0021 (`regex` foundation) landed at 04.2 Task 1; both are landed before 04.3 starts.

Possible additional ADRs land only if execution proves they're needed (per D-3.5 ambiguity-resolution discipline). Likely candidates if any:

- **ADR-0022 (or later) — TLS on upstream HCM.** If 04.3 surfaces a fixture-driven need to combine HTTP/1.1 + upstream TLS termination (sibling of phase 03.2's upstream TLS work — the natural hook is `cluster.transport_socket: Upstream(UpstreamTlsContext)` on a cluster referenced by an HCM `route: { cluster: ... }`), the integration is a small extension: `Client::connect_tls(addr, host, upstream_tls: Arc<UpstreamTls>)` mirroring `TcpProxy::with_upstream_tls` from phase 03.2. **Not anticipated** — fixture 0008 is plaintext upstream and the parent SPEC §3 architectural rule 6 explicitly defers HCM-with-TLS combinations from 04.x fixtures. But not foreclosed: if a need surfaces, the ADR likely lands as ADR-0022.
- **ADR-0022 (or later) — Header allow-list extensions** if fixture 0008 surfaces additional headers Envoy emits on proxied responses that envoy-rust can't readily match. Most likely a `BEHAVIOR_CONTRACT.md` edit + PROGRESS note (no ADR), unless the policy affects multiple later phases.
- **ADR-0022 (or later) — `Cluster::name()` accessor close-out** (per §3 D5) — typically lands as a doc cross-reference + a `pub(crate) fn name()` method, not a fresh ADR. ADR only if a posture decision (e.g., field-naming convention for cluster-attributed errors, `pub` vs. `pub(crate)` visibility) is worth recording.
- **ADR-0022 (or later) — Chunked-request-body forwarding posture** if execution surfaces a need to support chunked-request bodies in 04.3 (it does not — fixture 0008 is `Content-Length: 0`; chunked-request forwarding is a §4 non-goal). The ADR would land if a future phase wants to lift the deferral.

If any of these fire, they take the next-sequential available ADR number at the time they land (likely ADR-0022).

If `cargo deny check` flips red on any new transitive license (most likely a no-op since `http1-echo-server`'s deps — envoy-http1 + tokio + anyhow + thiserror + tracing + tracing-subscriber — are all already in scope from earlier phases), land the exemption under a new ADR at the time it trips.

---

## 8. Artifacts this sub-phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/04.3-router-upstream/PLAN.md`
- `docs/envoy-rust/phases/04.3-router-upstream/PROGRESS.md`
- `docs/envoy-rust/phases/04.3-router-upstream/REVIEW.md`
- `tests/helpers/http1-echo-server/Cargo.toml`
- `tests/helpers/http1-echo-server/src/main.rs`
- `crates/envoy-bin/tests/http1_router_upstream.rs`
- `tests/differential/tests/http1_router_upstream.rs`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_route_to_cluster.yaml`
- `tests/fixtures/0008-http1-router-upstream/{envoy.yaml,envoy-rust.yaml,inputs/payload.bin,expectations.yaml,README.md}`
- `crates/envoy-http1/src/client.rs` (new module; re-exported from `lib.rs`)
- `crates/envoy-http1/src/router.rs` (new module if the plan-writer factors `RouterError` + `write_proxied_response` out of `hcm.rs`; optional — both shapes acceptable)

Amended during execution:

- Root `Cargo.toml` — add `tests/helpers/http1-echo-server` to `[workspace] members`. (`crates/envoy-http1` is already there from 04.1.)
- `crates/envoy-http1/src/lib.rs` — add `pub mod client;` (and optionally `pub mod router;`); re-export `Client`, `ClientStream`, the new `Http1Error` variants (`UpstreamConnect`, `MalformedResponseLine`, `MalformedChunkedFraming`); re-export `RouterError` if factored to `router.rs`.
- `crates/envoy-http1/src/hcm.rs` (or wherever 04.1 placed the hardcoded router invocation site) — extend the `match action` from one arm (`DirectResponse`) to two (`DirectResponse` unchanged + new `Route`); add `write_proxied_response` helper; add 6 new unit tests (per §3 D2). If `RouterError` is factored to `router.rs`, the match arms call into `router.rs`'s helpers; otherwise inline.
- `crates/envoy-config/src/bootstrap.rs` — add `RouteAction::Route(RouteAction_Route)` enum variant; add `RouteAction_Route { cluster: String }` struct (with `#[serde(deny_unknown_fields)]`); extend `validate` to enforce `RouteAction_Route.cluster` references a known cluster (reuse `ConfigError::UnknownCluster` from phase 02.1); add 3 new validator unit tests (`parses_route_with_cluster_action`, `rejects_route_with_unknown_cluster`, `rejects_route_action_with_both_direct_response_and_route`).
- `crates/envoy-config/src/lib.rs` — re-export `RouteAction_Route`; `ConfigError::UnknownCluster` is unchanged.
- `crates/envoy-cluster/src/cluster.rs` — add `pub(crate) fn name(&self) -> &str` (or `pub fn` if consumer placement requires it; see §3 D5); remove field-level `#[allow(dead_code)]` on `Cluster.name`. ~3 optional unit tests appended.
- `crates/envoy-bin/src/main.rs` — **no changes anticipated** to the HCM dispatch arm itself; the `Route(RouteAction_Route)` action lives entirely inside `envoy-http1`'s HCM module per parent SPEC §6 signpost 17 lean ("place HCM in `envoy-http1` so the codec + per-connection state machine + per-listener route-walker live together"). The router proxy arm calls `cluster_mgr` which envoy-bin already constructs at startup (landed in 02.1) and passes into the HCM via the existing wiring (landed in 04.1). If the plan-writer finds a needed wiring change (e.g., the `cluster_mgr` reference was held in a way that's not accessible to the HCM's hardcoded router site, requiring a thread-through), document it at PLAN time.
- `tests/differential/src/lib.rs` — extend `HEADER_ALLOW_LIST` constant with `("x-envoy-upstream-service-time", AllowMode::ValueMayDiffer)`; extend `run_fixture` dispatch cascade to spawn `Http1EchoBackend` on the `{{HTTP1_BACKEND_PORT}}` template marker; add 1 harness unit test (`run_fixture_dispatches_http1_backend_on_template_marker`).
- `tests/differential/src/backend.rs` — add `Http1EchoBackend` struct + `spawn` + `port` + `container_host` + `Drop` impl; add `locate_http1_echo_server` helper; add 3 harness unit tests (`http1_echo_backend_spawns_and_echoes`, `http1_echo_backend_drop_terminates_child`, `locate_http1_echo_server_returns_existing_path`).
- `tests/differential/src/upstream.rs` — **no changes anticipated** — fixture 0008 has no PEM mounts (plaintext throughout). If the upstream-Envoy container needs additional file mounts for fixture 0008 (it should not), document at PLAN time.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — Header allow-list section gains the `x-envoy-upstream-service-time` row per §2 above.
- `docs/envoy-rust/DECISIONS.md` — no anticipated ADRs (see §7). If one of §7's possibles fires, append the next-sequential ADR (likely ADR-0022).
- `docs/envoy-rust/ROADMAP.md`:
  - Row `04.3` `status` → `done` in the final commit.
  - *At the same commit:* row `04` (parent) `status` → `done` per the ROADMAP-schema invariant ("the parent flips to `done` only after all sub-phases are `done`") — since 04.1 and 04.2 are `done` at 04.3 start, landing 04.3 `done` completes the parent.
- `docs/envoy-rust/STATE.md`:
  - Active phase advances from `04.3-router-upstream` to `05-<slug>` (slug consistent with `BOOTSTRAP_PROMPT.md` §8 row 05 — "HTTP/2 downstream + upstream"; expected slug `05-http2`).
  - Lifecycle state advances to phase 05 state 0/1; next-skill → `superpowers:brainstorming` scoped to phase 05.
  - State-detection note: phase 05 directory does not exist yet at the close of 04.3.
  - Notes section gets a "Phase-04.3 rollovers" subsection: M1 closed (close-out commit `<SHA>`); carryforward chain ends here. Other inherited carryforwards (the `std::thread::sleep` from a tokio-runtime thread issue in `*EchoBackend::Drop` impls — phase-02.1 REVIEW M1 sibling, tracked separately) forward unchanged.
- `Cargo.lock` — synced as a dedicated commit at state-4 phase-done gate per the established phase-precedent (mirrors phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85433e6`). New transitive surface from the `http1-echo-server` package stanza (minimal — all its deps are already in scope).
- `deny.toml` — only if `cargo deny check` flags new licenses or transitive surfaces. Most likely a no-op.

Not touched in 04.3 (belong to earlier phases or are frozen):

- `docs/envoy-rust/phases/04-http1/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `805433e`.
- `docs/envoy-rust/phases/04.1-hcm-direct-response/`, `phases/04.2-route-matchers/` — landed and finalized before 04.3 begins.
- `docs/envoy-rust/phases/03-tls-tcp/SPEC.md`, `03.1-tls-foundation-downstream/`, `03.2-tls-upstream-sni/` — closed in phase 03.
- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md`, `02.1-config-cluster/`, `02.2-listener-tcp-proxy/` — closed in phase 02.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, `tests/fixtures/0007-http1-direct-response/` — unedited; their fixtures must remain green at the 04.3 state-4 phase-done gate. (Fixture 0007 was amended in 04.2 to add a matcher-bearing route; that amendment lands in the 04.2 phase-done commit and is unchanged in 04.3.)
- `crates/envoy-tls/`, `crates/envoy-tcp/`, `crates/envoy-listener/`, `tests/helpers/{tcp,tls}-echo-server/` — finalized in earlier phases; phase 04.3 consumes via existing public APIs without amendment.
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.

---

## 9. Final commit message format (for state 6 of the 04.3 lifecycle, parent row 04 `done` commit)

The 04.3 phase-done commit also closes parent phase 04 in a single commit (mirrors phase 03's `ca81226`-shape close-out where the 03.2 commit closed parent 03). Format includes the `[parent 04 done]` tag in the title:

```
phase 04.3: HTTP/1.1 upstream proxying + fixture 0008 [parent 04 done]

envoy-http1 grows a per-connection HTTP/1.1 Client (connect, send_request,
chunked-encoding response reader; no pooling) under the existing httparse
sole-deps architectural rule. The HCM router invocation extends from one
match arm to two: DirectResponse unchanged; the new Route arm dispatches
into cluster_mgr.get(cluster).pick_endpoint() → Client::connect → forward
the request → write the upstream response back to downstream with the
header allow-list applied (envoy-rust adds x-envoy-upstream-service-time
per Envoy's wire-shape; both sides emit; values diverge by measurement,
covered by the BEHAVIOR_CONTRACT.md allow-list extension landed in 04.3).

New helper crate tests/helpers/http1-echo-server (sibling of tcp-echo-server
and tls-echo-server) ships a deterministic HTTP/1.1 echo server with
alphabetically-sorted-header response body; the determinism is load-bearing
for the byte-exact differential body equivalence. New differential harness
Http1EchoBackend (with locate_http1_echo_server helper, SIGKILL-on-Drop
posture mirroring TcpProxyBackend / TlsEchoBackend) plus a run_fixture
dispatch cascade extension on the {{HTTP1_BACKEND_PORT}} template marker.

The multi-phase Cluster::name() carryforward (originating in phase-02.1
REVIEW M1; re-deferred at phase-02.2 §4 rec 1, phase-03.1 §4 rec 2, and
phase-03.2 Task 5) closes opportunistically in 04.3: the router filter's
per-cluster proxy attribution (RouterError variants carrying cluster:
String + tracing log lines on per-cluster proxy errors) is the natural
use site. envoy-cluster's Cluster::name() accessor lands; the field-level
#[allow(dead_code)] is removed.

Closes parent phase 04 (HTTP/1.1 data plane). Sub-phases:
- 04.1 (commit <SHA>): envoy-http1 codec + HCM + minimal routing +
  direct_response + fixture 0007.
- 04.2 (commit <SHA>): all 7 HeaderMatcher modes + StringMatcher +
  invert_match + ADR-0021 (regex foundation).
- 04.3 (this commit): upstream HTTP/1.1 origination + router proxy arm
  + http1-echo-server helper + fixture 0008 + Cluster::name() close-out.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (unchanged);
  tests/fixtures/0004-tls-downstream green (unchanged);
  tests/fixtures/0005-tls-upstream green (unchanged);
  tests/fixtures/0006-tls-sni green (unchanged);
  tests/fixtures/0007-http1-direct-response green (HTTP/1.1 listener;
  direct_response route action; matcher-bearing route exercised since
  04.2's amendment);
  tests/fixtures/0008-http1-router-upstream green (HTTP/1.1 proxy
  through to http1-echo-server; per-cluster routing;
  x-envoy-upstream-service-time allow-list row exercised).
Conformance: none (h2spec attaches in phase 05).
```

Parent ROADMAP rows `04` and `04.3` flip to `done` in this commit (rows `04.1` and `04.2` flipped at their own state-6 commits earlier in the phase). STATE.md advances to phase `05` lifecycle state 1; next-skill `superpowers:brainstorming` scoped to phase 05 ("HTTP/2 downstream + upstream — low-level framer, own conn mgr" per `BOOTSTRAP_PROMPT.md` §8 row 05). Phase 04's projected ADR ledger (ADR-0020 + ADR-0021) is closed; phase 05's projected ADRs land at the next-sequential numbers (ADR-0022+).
