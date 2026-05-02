# Phase 05.3 — upstream HTTP/2 cleartext (H2C prior-knowledge): `envoy-http2::Client` + router H2-arm + `http2-echo-server` helper + fixture 0010 + parent-05 close

- **Phase id:** `05.3`
- **Parent phase:** `05-http2` (split per **ADR-0022**; parent SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md`, committed at parent-05 state-1 SHA `cd1a70e`).
- **Slug:** `05.3-http2-upstream`
- **Title:** Land upstream HTTP/2 cleartext (H2C prior-knowledge) on the data plane and close parent phase 05: a new `envoy-http2::Client` (per-connection plaintext H2 client; one TCP connection per upstream call; no pooling — pooling defers to the upstream-robustness family) + cluster-side `Http2ProtocolOptions` via Envoy's `typed_extension_protocol_options.HttpProtocolOptions` mechanism + a new `Cluster.upstream_protocol: UpstreamProtocol { Http1, Http2 }` field on `crates/envoy-cluster/src/cluster.rs` (defaulted to `Http1` for backwards-compat with all phase-04 clusters) + router H2-arm extending the 04.3-landed `RouteAction::Route` arm at `crates/envoy-http1/src/hcm.rs:189-288` to dispatch H1-or-H2 by `cluster.upstream_protocol` (reusing `crate::router::write_proxied_response` unchanged since the response wire-format on the downstream is HCM-on-downstream's concern, not the upstream-protocol's) + new helper crate `tests/helpers/http2-echo-server/` (sibling of `tests/helpers/{tcp,tls,http1}-echo-server/`) + fixture `0010-http2-router-upstream` + harness `Http2EchoBackend` + parent-phase-05 close-out.
- **Depends on:** `05.2` (sub-phase ROADMAP row `done` after 05.2's state-6 phase-done commit; the `envoy-http2` foundation crate landed in 05.2 D1 ships the `codec` / `hcm` / `request` / `response` / `error` modules, and 05.3 extends that crate with `client.rs`; the `CodecType::HTTP2` accept-flip and listener-side `Http2ProtocolOptions` schema landed in 05.2 D2 are also load-bearing — fixture 0010's downstream is an H2C listener and inherits the listener-side surface unchanged). Transitively depends on `05.1` (the `STRICT_DNS` cluster-type from 05.1 D1 is reused unchanged on fixture 0010's `backend` cluster which references `host.docker.internal`). Strictly the **closing sub-phase** of parent phase 05 — its state-6 phase-done commit ALSO flips parent ROADMAP row `05` `in-progress` → `done` per the ROADMAP-schema invariant ("the parent flips to `done` only after all sub-phases are `done`"; mirrors phase-04's `e626862`-shape close-out where the 04.3 commit closed parent 04, and phase-03's `ca81226`-shape close-out where the 03.2 commit closed parent 03).
- **Differential surface when done:**
  - **Fixtures unchanged in 05.3 (must remain green at 05.3 state-4):** `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, `tests/fixtures/0007-http1-direct-response/`, `tests/fixtures/0008-http1-router-upstream/`, `tests/fixtures/0009-http2-direct-response/` — all 9 must remain green at the Docker-gated CI level. The 5 fixtures restored by 05.1 (0003/0004/0005/0006/0008) and the H2 listener fixture landed by 05.2 (0009) inherit their green baselines unchanged.
  - **New fixture green:** `tests/fixtures/0010-http2-router-upstream/` — H2C downstream listener with HCM `codec_type: HTTP2`, single VH `domains: ["*"]`, single route `prefix: "/"`, `route: { cluster: backend }`; cluster `backend` carries `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options { ... }` selecting H2 upstream, declares `type: STRICT_DNS` (per 05.1's preamble), and resolves `{{BACKEND_HOST}}` (`host.docker.internal` for the upstream-Envoy container per ADR-0015; `127.0.0.1` for envoy-rust) at port `{{HTTP2_BACKEND_PORT}}` to the new in-tree `http2-echo-server` helper.
  - **Conformance suite unchanged in 05.3:** `tests/conformance/h2spec/` continues at the **≥95% pass** gate landed in 05.2 D7. 05.3 does not edit the runner or the gate semantics; the upstream-direction work is exercised through fixture 0010, not h2spec. If 05.3's router H2-arm exposes any new code paths that h2spec could probe (e.g., the codec wrapper used for the upstream H2 client is the same `h2 = "0.4"` codec the listener-side already exercises, so no new conformance surface), the planner re-runs h2spec at state-4 to confirm the gate still passes; no `known-failures.txt` edits are anticipated.
- **Seeded by:** parent-05 SPEC §1 layer 3 (the goal-paragraph for sub-phase 05.3), §3 D11.3–D15.3 (the five 05.3 deliverables), §4 (non-goals — the 05.3-binding subset, especially the connection-pooling deferral, the H2-trailers deferral, the cross-protocol H2↔H1 translation deferral, the cluster-side TLS+H2 deferral, the per-route `Http2ProtocolOptions` overrides deferral), §5 (3-way split decision context — the rationale for placing the upstream H2 work in its own sub-phase after the downstream H2 codec/HCM is in-tree), §6 signposts 5 (`Cluster.upstream_protocol` field placement — typed field set at cluster-build time, defaulted to `Http1`), 6 (background `h2::client::Connection` driving via `tokio::spawn` direct), 7 (test-helper architectural posture — `http2-echo-server` consumes `envoy_http2`, not `h2` directly), 10 (`x-envoy-upstream-service-time` header on H2 router responses; same `Instant::now()` measurement window as 04.3), 12 (`:method`/`:path`/`:authority`/`:scheme` translation — 05.3 does the inverse direction at `client.rs`), 14 (Cargo.lock cadence — inline-at-scaffold; M5/M9 carryforward continues unchanged), 16 (PLAN.md cadence — pre-Task-1 standalone commit per `c02eea7` precedent), 17 (fixture 0010 declares `STRICT_DNS` per 05.1's schema growth), 18 (in-process integration backstops — `crates/envoy-bin/tests/http2_router_upstream.rs`), 19 (`anyhow` boundary), 20 (HCM filter naming), 21 (ADR ledger projection: NO new ADRs projected at 05.3 state-2; ADR-0023 landed at 05.1 Task 1, conditional ADR-0024/0025 at 05.2 Task 1), 23 (`http1-echo-server` and `http2-echo-server` interop), §7 (no ADR projections specific to 05.3), §8 (parent-05 artifact list, scoped to 05.3's slice + the parent ROADMAP-row flip), §9 (parent-05 final commit message format — the `[parent 05 done]` tag attaches to the 05.3 state-6 commit).

This SPEC is the design contract for sub-phase 05.3. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed phase-04 + sub-phase-05.1 + sub-phase-05.2 surface (via `git log` and the in-tree `envoy-config` / `envoy-cluster` / `envoy-http1` / `envoy-http2` / `envoy-tls` / `envoy-tcp` / `envoy-listener` / `envoy-bin` / `tests/differential` / `tests/helpers/{tcp,tls,http1}-echo-server` / `tests/conformance/h2spec` / `tests/fixtures/{0001..0009}` shape at sub-phase-05.2 close) must be able to execute it without consulting the parent `05-http2/SPEC.md`. The C-1 regression trace and 05.1 fixture-hardening posture are reproduced inline below (§1) for that reason.

---

## 1. Goal and acceptance signal

**Goal.** Land upstream HTTP/2 cleartext (H2C prior-knowledge) on the data plane and close parent phase 05 in five coordinated layers that all ship in this single sub-phase:

1. **`envoy-http2::Client` (per-connection plaintext H2 client).** New module `crates/envoy-http2/src/client.rs` (sibling of 05.2 D3's listener-side `hcm.rs`/`request.rs`/`response.rs` modules; the parent-05 SPEC §3 D5.2 module decomposition projection lists `client.rs` as 05.3-scoped explicitly). Public surface mirrors `envoy_http1::Client` (landed in 04.3 D1 at `crates/envoy-http1/src/client.rs`; verifiable at 05.3 Task 1 by `grep -n 'pub struct Client\|pub struct ClientStream\|impl Client\|impl ClientStream' crates/envoy-http1/src/client.rs`): `Client::connect(addr: SocketAddr, host: &str) -> Result<ClientStream, Http2Error>` opens a plaintext TCP connection, runs `h2::client::handshake(tcp).await` (which sends the H2C `PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n` preamble + the initial SETTINGS frame), drives the resulting `h2::client::Connection` on a background `tokio::spawn` for the lifetime of the `ClientStream` (per parent §6 signpost 6's recommendation: `tokio::spawn` direct, matching `h2`'s docs), and returns the captured `h2::client::SendRequest` handle wrapped in `ClientStream`; `ClientStream::send_request(request: envoy_http1::codec::Request) -> Result<envoy_http1::codec::Response, Http2Error>` translates the envoy `Request` value type into `http::Request<()>` (synthesizing `:method`, `:path`, `:authority` from `Request.method`, `Request.path`, the captured `host` — or the request's `Host:` header if explicitly set, mirroring 04.3's `envoy_http1::Client` behavior; `:scheme: http` since 05's posture is plaintext H2C only per parent §4), strips H2-forbidden hop-by-hop headers (`connection`, `transfer-encoding`, `upgrade`, `keep-alive`, `proxy-connection`) defensively per RFC 7540 §8.1.2.2 and parent §3 cross-sub-phase architectural rule 4, sends the request via `h2::client::SendRequest::send_request`, writes the request body to the returned `h2::SendStream` (CL-only — chunked/streaming request bodies deferred per §4 below), reads the response via the returned `h2::client::ResponseFuture`, drains the response body bytes from the `h2::RecvStream` into `bytes::Bytes` (the same drain pattern 05.2 D3 uses for the listener-side body intake; per parent §6 signpost 9 the body-bytes drain budget is unbounded in 05.3 — fixture 0010's deterministic-echo body is small and well-framed), and translates the `http::Response<()>` + body back into the envoy `Response` value type. **No connection pooling.** Each `ClientStream` owns one TCP connection and is consumed by a single `send_request` call; subsequent calls require a new `Client::connect`. Pooling is upstream-robustness-family territory and is materially more interesting under H2 (one pooled connection serves many streams; the pool must also track stream-count vs. `MAX_CONCURRENT_STREAMS`, handle `GOAWAY` frames mid-pool, etc.), so deferring it intentionally avoids prematurely committing to a pool design — see parent SPEC §4. The 05.2-landed `Http2Error` enum at `crates/envoy-http2/src/error.rs` gains 4 new client-side variants (`UpstreamConnect { addr, source }`, `H2ClientHandshake { source: h2::Error }`, `H2SendRequest { source: h2::Error }`, `H2RecvBody { source: h2::Error }`); the 05.2 codec-side variants (`H2Handshake`, `H2StreamAccept`, `H2BodyRead`, `MissingAuthority`, `MalformedH2HeaderBlock`, `BadStatusCode`) stay unchanged.

2. **Cluster-side `Http2ProtocolOptions` schema in `envoy-config` via `typed_extension_protocol_options`.** In `crates/envoy-config/src/bootstrap.rs`: extend the existing `Cluster` struct (at lines 46–56 of HEAD `e626862` per 05.1 SPEC §1 verifiable command — the planner re-checks at 05.3 Task 1 against the post-05.1+05.2 HEAD shape) with an optional `typed_extension_protocol_options` field that carries Envoy's `envoy.extensions.upstreams.http.v3.HttpProtocolOptions` typed_config. Subset shipped: a new `HttpProtocolOptions` variant on the existing `TypedConfig` enum (sibling of `TcpProxy` and `HttpConnectionManager` typed_configs introduced in earlier phases per ADR-0014); inside, an `ExplicitHttpConfig` enum carrying either `Http1ProtocolOptions` (empty in 05; future fields like `chunk_encoding` defer per §4 below) or `Http2ProtocolOptions` (the same struct landed at 05.2 D2.b listener-side, reused unchanged here so the validator's range checks fire identically on cluster-side and listener-side). YAML shape:
   ```yaml
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
   Validator rejects mixed `http_protocol_options` and `http2_protocol_options` on the same cluster (Envoy's `explicit_http_config` is mutually exclusive — at most one of the two oneof arms may be set per cluster). New `ConfigError::MutuallyExclusiveExplicitHttpConfig { cluster: String }` variant for this rejection. Re-uses the existing `ConfigError::Http2ProtocolOptionsOutOfRange { field, value, range }` variant landed at 05.2 D2.b for out-of-range field values (the range checks are codec-foundation-bound, not direction-bound, so the same variant is correct).

3. **`Cluster.upstream_protocol: UpstreamProtocol { Http1, Http2 }` field on `crates/envoy-cluster/src/cluster.rs`.** New typed enum `UpstreamProtocol { Http1, Http2 }` in `envoy-cluster`'s public surface (defaulted to `Http1` for backwards-compat with all phase-04 clusters and with the 8 fixtures unchanged in 05.3 — none of fixtures 0001–0009 declare `typed_extension_protocol_options`, so they all default to `Http1` and exercise the unchanged 04.3 H1 router-arm code path). Set at cluster-build time in `from_bootstrap` (the cluster-manager constructor at `crates/envoy-cluster/src/cluster.rs::from_bootstrap` at line 112 — verifiable at task-1 time by `grep -n 'pub fn from_bootstrap' crates/envoy-cluster/src/cluster.rs`) by inspecting the parsed cluster's `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config` and projecting `Http2(_) → UpstreamProtocol::Http2`, `Http1(_) → UpstreamProtocol::Http1`, absent → `UpstreamProtocol::Http1`. Stored as a public field on `Cluster` (`pub(crate) upstream_protocol: UpstreamProtocol`) and exposed via `pub fn Cluster::upstream_protocol(&self) -> UpstreamProtocol` accessor (mirrors the `Cluster::name()` accessor landed in 04.3 at `crates/envoy-cluster/src/cluster.rs:24-26`); also exposed via `ClusterHandle::upstream_protocol` delegate accessor (mirrors `ClusterHandle::name()` at `crates/envoy-cluster/src/cluster.rs:60-62`). Per parent §6 signpost 5 the recommendation is the typed-field-set-at-cluster-build-time posture, not a derived-from-config-at-each-call lazy lookup — avoids re-parsing config at every upstream call.

4. **Router H2-arm extending the 04.3 `RouteAction::Route` dispatch.** The existing `RouteAction::Route` arm at `crates/envoy-http1/src/hcm.rs:189-288` (the `BuildOutcome::Proxy { cluster: cluster_name }` arm of `serve_connection`'s outcome match — verifiable inline at HEAD by reading `crates/envoy-http1/src/hcm.rs` lines 189–288 against the e626862 baseline) extends to dispatch into either H1 or H2 based on the picked cluster's `upstream_protocol` field. Pseudocode (the planner cross-checks the live shape at 05.3 Task 1):
   ```rust
   // At crates/envoy-http1/src/hcm.rs (existing serve_connection's
   // BuildOutcome::Proxy arm), the Client::connect call site changes from:
   let mut client_stream = match Client::connect(endpoint, &host_header).await {
       Ok(s) => s,
       Err(source) => { /* 502 fallback unchanged */ }
   };
   let upstream_response = match client_stream.send_request(out_req).await {
       Ok(r) => r,
       Err(source) => { /* 502 fallback unchanged */ }
   };

   // ...to:
   let upstream_response = match cluster.upstream_protocol() {
       UpstreamProtocol::Http1 => {
           // Existing 04.3 path, unchanged.
           let mut client_stream = envoy_http1::Client::connect(endpoint, &host_header)
               .await
               .map_err(|s| /* 502 fallback */)?;
           client_stream.send_request(out_req).await
               .map_err(|s| /* 502 fallback */)?
       }
       UpstreamProtocol::Http2 => {
           // 05.3 NEW.
           let mut client_stream = envoy_http2::Client::connect(endpoint, &host_header)
               .await
               .map_err(|s| /* 502 fallback */)?;
           client_stream.send_request(out_req).await
               .map_err(|s| /* 502 fallback */)?
       }
   };
   ```
   The response wire-format on the **downstream** is HCM-on-downstream's concern, not the upstream-protocol's; whether the upstream spoke H1 or H2 is invisible to the downstream once the response has been translated back into the envoy `Response` value type by `envoy_http2::Client::send_request`. Therefore `crate::router::write_proxied_response(&mut downstream, upstream_response, elapsed_ms, close).await?` (the response-write helper landed in 04.3 at `crates/envoy-http1/src/router.rs`; the `elapsed_ms` capture and `x-envoy-upstream-service-time` injection per parent §6 signpost 10 / 04.3 BEHAVIOR_CONTRACT §3 stay symmetric across H1 and H2 upstreams) is **reused unchanged** — 05.3 does NOT edit `router.rs`; it only edits the `Client::connect` / `send_request` call site at `hcm.rs:189-288`. The `Instant::now()` measurement window for `x-envoy-upstream-service-time` stays the same: `Instant::now()` immediately before `Client::connect`; `start.elapsed()` immediately after `send_request` returns. The router H2-arm dispatch ALSO lands at the symmetric site inside `crates/envoy-http2/src/hcm.rs` (the listener-side H2 HCM that 05.2 D3 stubbed at the `BuildOutcome::Proxy` path with a 502 — see 05.2 SPEC §3 D3 test 6 `h2_proxy_outcome_returns_502_in_05_2`); 05.3 replaces that 502 stub with the same H1-or-H2 dispatch keyed on `cluster.upstream_protocol`, and renames the test to `h2_proxy_outcome_dispatches_to_upstream` per 05.2 SPEC §3 D3's projection.

5. **`tests/helpers/http2-echo-server/` workspace member + harness `Http2EchoBackend` + fixture 0010 + Docker-gated test + in-process backstop.** New helper crate at `tests/helpers/http2-echo-server/` (sibling of 04.3's `tests/helpers/http1-echo-server/` per parent §6 signpost 7's posture: the helper consumes `envoy_http2` instead of `h2` directly so the cross-sub-phase architectural rule "only `envoy-http2` depends on `h2`" stays enforced even for test helpers; mirrors how 04.3's `http1-echo-server` consumed `envoy_http1` over direct `httparse`). Hand-parsed argv (`--port <u16>` + `--help` + `--version` per the 04.3 task-11 review-fix shape that 04.3's `http1-echo-server` ships at `tests/helpers/http1-echo-server/src/main.rs:42-65`). Minimal H2C echo: any request method + path produces `200` with `content-type: text/plain` + a body containing the deterministic echo (method + path + alphabetically-sorted-by-lowercased-name headers + body) — the alphabetic header sort is **load-bearing** for differential equivalence per parent SPEC §3 D14.3 / 04.3 D3's posture (both proxies forward the SAME logical request to the SAME helper; the helper's sorted-header response is the byte-exact baseline). The differential-harness `Http2EchoBackend` (`tests/differential/src/backend.rs`; sibling of 04.3-landed `Http1EchoBackend` and 03.2-landed `TlsEchoBackend` and 02.2-landed `TcpProxyBackend`) carries `spawn() -> Result<Self>` (locates `http2-echo-server` binary at workspace `target/<profile>/http2-echo-server`, reserves an ephemeral port, spawns the subprocess, polls for accept-readiness with the 5s budget that the 04.x backends use), `port() -> u16`, `container_host() -> &'static str` (returns `"host.docker.internal"` per ADR-0015 — but with `STRICT_DNS` cluster type from 05.1's preamble, so the DNS-rejection regression is no longer in play), SIGKILL-on-Drop posture (mirrors 02.2 REVIEW M1's `*EchoBackend::Drop` polling loop posture, including the awareness-only `std::thread::sleep` carryforward — 05.3 does NOT close M1; it inherits and continues per 05.2's posture). New locator helper `locate_http2_echo_server()`. The `run_fixture` cascade in `tests/differential/src/lib.rs` grows a `{{HTTP2_BACKEND_PORT}}` template marker substitution for fixture 0010 (mirrors 04.3's `{{HTTP1_BACKEND_PORT}}` per 04.3 SPEC §8). Fixture `tests/fixtures/0010-http2-router-upstream/` ships 5 files: `envoy.yaml` (admin block + plaintext listener bind + HCM filter chain `codec_type: HTTP2` + single-VH single-route `prefix: "/"` `route: { cluster: backend }` + cluster `backend` with `type: STRICT_DNS` (per 05.1's preamble) + `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options` selecting H2 upstream + endpoint `{{BACKEND_HOST}}:{{HTTP2_BACKEND_PORT}}`); `envoy-rust.yaml` (per-side divergences mirroring 04.3 fixture 0008's posture — `127.0.0.1` bind, no admin, `request_headers_to_remove` omitted on the envoy-rust side per the same field-set-divergence rationale documented inline in `tests/fixtures/0008-http1-router-upstream/envoy.yaml:14-29`); `inputs/payload.bin` (empty for the GET); `expectations.yaml` (driver kind `http2` with `method: GET`, `path: "/"`, `host: "envoy-rust.test"`, `expected_status: 200`, `expected_body: { byte_exact: <deterministic-echo-body> }`, `expected_headers: { rule: set_equal_modulo_allow_list }`; the byte-exact body shape is determined by the helper's deterministic echo per parent SPEC §3 D14.3 — same general format as fixture 0008's `expectations.yaml` body shape with the request lines and the alphabetically-sorted headers); `README.md` (~30 lines describing the round-trip surface). Docker-gated `tests/differential/tests/http2_router_upstream.rs` is a 7-line wrapper calling `differential::run_fixture("0010-http2-router-upstream")`. In-process integration backstop at `crates/envoy-bin/tests/http2_router_upstream.rs` (sibling of 04.3's `crates/envoy-bin/tests/http1_router_upstream.rs` and 05.2's `crates/envoy-bin/tests/http2_direct_response.rs`) per parent §6 signpost 18: spawns envoy-bin via `CARGO_BIN_EXE_envoy-bin`, drives a single H2C `GET /` request via `h2::client` against an envoy-bin instance whose cluster `backend` points at an in-test-spawned `http2-echo-server`, asserts the parsed response's status + body + headers.

**Cross-phase items closed at 05.3.** None directly inside the 05.3 surface itself. The cross-phase items closed across parent phase 05 (phase-04.3 REVIEW C-1 + phase-02.1 REVIEW I3) closed at 05.1's state-4/state-6 commits respectively per 05.1 SPEC §1 / §3 D5 / §7 ADR-0023's Consequences; 05.3 inherits the closed state without re-engaging. Phase-04.1 REVIEW M-claim (`drive_http1` per-function unit test) was unblocked by 05.1's fixture-mask removal but stays deferred per the 04.3 disposition; 05.3 introduces no new H1 surfaces and does not extend the harness in a way that adds a third `Driver::Http1` consumer, so M-claim continues unchanged through 05.3 close. Phase-02.2 REVIEW M1 (`*EchoBackend::Drop` polling loop blocks on `std::thread::sleep` from a tokio-runtime thread) is inherited verbatim by the new `Http2EchoBackend`; M1 continues to track forward to whichever phase first parallelizes `run_fixture` per the established carryforward chain (02.2 → 03.2 → 04.3 → 05.3 → ...). Phase-04.1 REVIEW M1 (`diff_headers` value-comparison silently ignores duplicate-header value mismatches) and M2 (body-drain idle timeout returns `Ok(())` silently on read timeout) and M4 (`strip_port` uses `rfind(':')`; incorrect for bare-IPv6 Host) — all three may surface latently under H2's HPACK-derived header semantics or the `:authority` pseudo-header carrying IPv6, but 05.3's fixture 0010 does not exercise duplicate response headers, does not stall on body drain (`http2-echo-server`'s response body is small and well-framed), and uses a DNS-name `Host:` value (`envoy-rust.test` per the fixture's expectations.yaml `host:` field — same shape as 04.3 fixture 0008) so does not exercise the IPv6-Host code path. M1/M2/M4 continue tracking forward unchanged.

**Cross-phase items unblocked but not closed at 05.3.** None.

**Parent-phase-05 close-out at 05.3 state-6.** The 05.3 state-6 phase-done commit ALSO flips parent ROADMAP row `05` `in-progress` → `done` per the ROADMAP-schema invariant ("the parent flips to `done` only after all sub-phases are `done`"; ROADMAP.md schema bullet 18: *"When a phase is split, its own `status` becomes `in-progress` while its sub-phases land. The parent flips to `done` only after all sub-phases are `done`"*). Mirrors phase-04's `e626862`-shape close-out (the 04.3 state-6 commit also flipped parent-04 `in-progress` → `done` because 04.1/04.2 were already done and 04.3 was the last-standing sub-phase) and phase-03's `ca81226`-shape close-out (the 03.2 state-6 commit closed parent 03 the same way). Parent SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md` is **not edited** at this commit — it remains the historical artifact committed at parent-05 state-1 SHA `cd1a70e` per D-3.4 / D-3.5 (parent SPECs are preserved unedited as design-projection artifacts even after their sub-phases close; the 04-http1, 03-tls-tcp, and 02-tcp-proxy parent SPECs all follow this posture). The 05.3 state-6 commit's title carries the `[parent 05 done]` tag, mirroring 04.3's `[parent 04 done]` tag and 03.2's `[parent 03 done]` tag.

**Scope-shape inheritance from the parent-05 brainstorm.** The brainstorm explicitly bounded 05.3 to: codec extension (the `client.rs` module of the existing `envoy-http2` crate landed in 05.2 — NOT a new crate; NOT any extension of the 05.2 listener-side modules `codec.rs`/`hcm.rs`/`request.rs`/`response.rs`/`error.rs` other than additive variants on the typed-error enum); schema growth (cluster-side `Http2ProtocolOptions` via `typed_extension_protocol_options` only — NOT any extension of the listener-side `Http2ProtocolOptions` landed in 05.2 D2.b, NOT any extension of the `CodecType` enum, NOT any extension of `RouteConfiguration` or `HeaderMatcher` per parent §3 cross-sub-phase architectural rule 5); runtime growth (`Cluster.upstream_protocol` field + the H1-or-H2 dispatch at the `BuildOutcome::Proxy` site + the symmetric dispatch at the H2 listener-side HCM's Proxy stub site — NOT any new HCM, NOT any new ConnectionHandler trait); helper crate (`http2-echo-server` only — NOT any extension of `tcp-echo-server`/`tls-echo-server`/`http1-echo-server`); fixture (0010 only — NOT any new fixture beyond 0010, NOT any edit to fixtures 0001–0009); harness extensions (`Http2EchoBackend` + `{{HTTP2_BACKEND_PORT}}` template marker only — `Driver::Http2` and `drive_http2` were landed in 05.2 D5 and reused unchanged by fixture 0010); parent close-out (the ROADMAP row `05` flip + the STATE.md advance to phase 06 lifecycle state 1). This bounding is reproduced verbatim in §4 below as 05.3's non-goals.

**C-1 regression trace, reproduced inline for self-containment per D-3.4 (so that 05.3 can be executed without consulting parent SPEC).** Upstream Envoy v1.33.0 rejects the rendered `address: host.docker.internal` under `type: STATIC` with this critical-log line:

```
[critical][main] [source/server/server.cc:416] error initializing config '/etc/envoy/envoy.yaml':
malformed IP address: host.docker.internal. Consider setting resolver_name or setting cluster type
to 'STRICT_DNS' or 'LOGICAL_DNS'
```

The regression originated at phase-02.2's ADR-0015 landing (`host.docker.internal` introduced as the `BACKEND_HOST` substitution for cross-container reachability via Docker's `host-gateway`; commit `435c6fa`); was latent across phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3 (no CI push between phase-02.1 close and phase-04.3 task 14); was discovered at phase-04.3 task 14 (commit `eb6f972`); was dispositioned at the 04.3 STATE.md handoff (commit `e626862`); and was substantively closed at sub-phase 05.1's state-4 phase-done verification commit when the 5 affected Docker-gated fixtures (0003/0004/0005/0006/0008) re-greened simultaneously. Sub-phase 05.3 inherits the restored baseline; 05.3's fixture 0010 declares `type: STRICT_DNS` per parent §6 signpost 17 and 05.1's schema growth, so the C-1 regression is structurally precluded for the new fixture. (See parent-05 SPEC §1 / 05.1 SPEC §1 / 05.2 SPEC §1 for the full disposition history.)

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 05.3's feature surface AND the parent-phase-05 acceptance surface (since 05.3 is the closing sub-phase, its state-4 gate is also the parent-05 gate):

- (a) the new differential fixture `tests/fixtures/0010-http2-router-upstream/` is green at the Docker-gated CI level, with the CI run URL + the test result quoted inline in `PROGRESS.md`;
- (b) the 9 pre-existing differential fixtures `tests/fixtures/{0001-tcp-echo,0002-static-admin-ready,0003-tcp-proxy,0004-tls-downstream,0005-tls-upstream,0006-tls-sni,0007-http1-direct-response,0008-http1-router-upstream,0009-http2-direct-response}/` remain green at the Docker-gated CI level (they are not edited in 05.3; their fixtures were green at sub-phase-05.2 close and continue green);
- (c) the conformance suite `tests/conformance/h2spec/` continues at **≥95% pass** (landed at 05.2 D7; 05.3 does not edit the runner or the gate); the 05.3 state-4 verification re-runs h2spec to confirm no regression from the upstream-direction work;
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 05.3 with **one new seed** (`crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml`; a full bootstrap with one HCM listener + one cluster of `type: STRICT_DNS` whose `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options` carries the four-field subset); no new fuzz target ships in 05.3;
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job. `cargo deny check` is a no-op (no new top-level Cargo deps in 05.3 — the new `http2-echo-server` workspace member consumes existing `envoy-http2` + `tokio` + `anyhow` + `bytes` + `thiserror` + `tracing` + `tracing-subscriber` foundations);
- (f) `REVIEW.md` for this sub-phase is approved.

The 05.3 phase-done commit flips ROADMAP row `05.3` from `in-progress` to `done`. **At the same commit:** parent ROADMAP row `05` (`HTTP/2 downstream + upstream …`) flips from `in-progress` to `done` per the ROADMAP-schema invariant — since 05.1 and 05.2 are `done` at 05.3 start (per the strict 05.1 → 05.2 → 05.3 ordering), landing 05.3 `done` completes the parent. STATE.md advances from `05.3-http2-upstream` to `06-<slug>` (per `BOOTSTRAP_PROMPT.md` §8 row 06: *"Access log (file sink, Envoy default format) + stats + Prometheus admin endpoint"*; expected slug `06-access-log-stats` or similar — the planner uses whatever slug phase-06 brainstorm chooses), lifecycle state phase 06 state 1 (phase-06 directory does not exist yet), next-skill `superpowers:brainstorming` scoped to phase 06.

---

## 2. Behavior-contract scope for sub-phase 05.3

**No `BEHAVIOR_CONTRACT.md` edits in 05.3.** The upstream H2 client + router H2-arm produce no new response shapes that the existing 3 phase-04 `Header allow-list` rows (`server`, `date`, `x-envoy-upstream-service-time` from 04.3 commit `cdd0218`) don't already cover. The `x-envoy-upstream-service-time` row is engaged for the first time on H2 router responses (its 04.3-landed disposition reads "Only present on responses that proxied through to an upstream cluster (NOT on `direct_response` paths)"; fixture 0010 is router-proxy, so the row engages); the existing rule "name-required, value-may-differ" applies symmetrically across H1 and H2 upstream paths because the measurement window is `Instant::now()` based and identical on both sides per parent §6 signpost 10. The other two rows (`server`, `date`) cover the H2 emission path symmetrically since they are emission-semantics rules, not framing-bound — fixture 0010's response carries `server: <implementation-name>` and `date: <imf-fixdate>` whose values diverge per the same disposition that covers fixtures 0007/0008/0009.

Equivalence-matrix engagement (per `BEHAVIOR_CONTRACT.md` §7.2):

- **Row 1 (Response status)** — fixture 0010 exercises this via the H2 `:status` pseudo-header (asserted byte-exact `200` from `http2-echo-server`'s deterministic 200 response).
- **Row 2 (Response body)** — fixture 0010 byte-exact body equivalence on the helper's deterministic-echo body (method + path + alphabetically-sorted headers + body — same shape as fixture 0008's expectations.yaml body, modulo the request-header set delivered by the H2 codec edge which lowercases names per parent §6 signpost 11 and inserts `:authority`-derived `Host:` per parent §3 cross-sub-phase architectural rule 3).
- **Row 3 (Response headers)** — fixture 0010's response carries the existing 04.x `HEADER_ALLOW_LIST` from `tests/differential/src/lib.rs` (3 rows: `server`, `date`, `x-envoy-upstream-service-time`; all three engaged on the router-proxy path).
- **Row 4 (HTTP/2 & HTTP/3 framing)** — engaged transitively by fixture 0010's H2C downstream (the harness's `drive_http2` from 05.2 D5.b drives the request via `h2::client` and asserts on the parsed response surface). Frame-level equivalence is implicit (both proxies emit valid H2 framing or `h2`-the-codec rejects the connection); no fixture asserts on raw frame bytes. The upstream-direction H2 framing (envoy-rust's `envoy_http2::Client` writing H2 frames to `http2-echo-server`) is exercised structurally — both proxies' upstream H2 requests must be acceptable to `http2-echo-server`'s `h2::server` for the helper to produce a 200; framing-level divergence would surface as a connection-level `h2::Error` and the fixture would fail.
- **Row 5 (TLS handshake), Row 6 (TLS cert validation), Row 8 (TCP-stream byte equivalence)** — N/A in 05.3 (fixture 0010 is plaintext H2C end-to-end; no TLS).

**HTTP/1.1 hop-by-hop headers** (`Connection`, `Transfer-Encoding`, `Upgrade`, `Keep-Alive`, `Proxy-Connection`) are forbidden in H2 messages per RFC 7540 §8.1.2.2. Their absence is enforced at the codec layer: the `h2` crate rejects them at the codec layer if envoy-rust attempts to emit them, and 05.3's `Client::send_request` strips them defensively before handing the request off to `h2::SendStream` (mirroring 05.2 D3's defensive strip in `response.rs` for the listener-side response direction). Their absence is therefore not asserted by the fixture (they simply never appear on the wire in H2); no allow-list change required.

**HTTP/2 pseudo-headers on the upstream-bound request** (`:method`, `:path`, `:authority`, `:scheme`) are synthesized by `envoy_http2::Client::send_request` at the codec edge per parent §6 signpost 12 and parent §3 cross-sub-phase architectural rule 3 — the inverse of 05.2 D3's listener-side translation. `:method` from `Request.method`, `:path` from `Request.path`, `:authority` from the captured `host` (or the request's `Host:` header if explicit, mirroring 04.3's `envoy_http1::Client` posture per `crates/envoy-http1/src/client.rs`'s `Host:`-resolution behavior), `:scheme: http` (since 05's posture is plaintext H2C only — TLS+ALPN+H2 deferred per parent §4). These are not response surface and don't engage the response-header allow-list.

**HTTP/2 trailers — out of scope for 05.3 (deferred non-goal).** `http2-echo-server` does not emit response trailers; the router H2-arm does not forward trailers from upstream to downstream; envoy-rust's H2 codec wrapper (the listener-side response emission landed in 05.2 D3 and the client-side response intake landed in 05.3 D1) does not parse or write trailers. Trailers (HEADERS frame after END_STREAM on a DATA frame) defer to whichever phase first emits trailer-bearing responses (gRPC family will likely force this). See parent-05 SPEC §4.

The `HEADER_ALLOW_LIST` constant in `tests/differential/src/lib.rs` (the 04.3-landed shape with 3 rows) is unedited in 05.3. The allow-list applies to row-3 response headers; H2 pseudo-headers and forbidden hop-by-hop headers fall outside its scope.

No new `Stat-name`, `Access log field`, `xDS wire`, or `Timing tolerances` subsections are touched.

---

## 3. Deliverables

### D1 — `envoy-http2::Client` (per-connection H2 client) at `crates/envoy-http2/src/client.rs`

The core 05.3 runtime deliverable. New module under the existing `envoy-http2` crate (which 05.2 D1 introduced with the listener-side modules `lib`, `codec`, `hcm`, `request`, `response`, `error`); the parent-05 SPEC §3 D5.2 module decomposition projection lists `client.rs` as 05.3-scoped explicitly. Sole user (with the listener-side codec) of `h2 = "0.4"` in the workspace per the cross-sub-phase architectural rule (§5 below); mirrors `envoy_http1::Client` from 04.3 D1 at `crates/envoy-http1/src/client.rs`.

**Public surface re-exported at `crates/envoy-http2/src/lib.rs`:**

```rust
// 05.3 NEW — appended to the 05.2-landed lib.rs:
pub mod client;
pub use client::{Client, ClientStream};
```

**`client.rs` shape** (planner skeleton; exact shape lands at PLAN.md writeup):

```rust
pub struct Client;

impl Client {
    pub async fn connect(addr: SocketAddr, host: &str) -> Result<ClientStream, Http2Error>;
}

pub struct ClientStream {
    send_request: h2::client::SendRequest<bytes::Bytes>,
    host: String,
    // h2::client::Connection is driven on a background tokio::spawn for the
    // lifetime of the ClientStream; per parent §6 signpost 6's recommendation
    // (tokio::spawn direct, matching h2's docs). Fire-and-forget; the task
    // terminates when SendRequest drops + connection gracefully closes.
}

impl ClientStream {
    pub async fn send_request(&mut self, request: envoy_http1::codec::Request)
        -> Result<envoy_http1::codec::Response, Http2Error>;
}
```

`Client::connect` opens a plaintext TCP, calls `h2::client::handshake(tcp).await` (mapping `h2::Error` to `Http2Error::H2ClientHandshake`), drives the returned `h2::client::Connection` on `tokio::spawn` (errors logged via `tracing::warn!`; not propagated — the spawned task is fire-and-forget per parent §6 signpost 6), and returns `ClientStream` wrapping the `SendRequest` handle + the captured `host`.

`ClientStream::send_request` translation steps (the design contract; exact code at PLAN.md writeup):

1. **`:authority` resolution.** If `request.headers` carries a case-insensitive `Host:` header (per the existing `find_header` helper at `crates/envoy-http1/src/headers.rs`), use its value as `:authority`; otherwise use the captured `self.host`. Mirrors 04.3 `envoy_http1::Client::send_request`'s host-resolution per `crates/envoy-http1/src/client.rs` ("the `Host:` header is sourced from the `host` captured at connect time unless `request` already carries one (case-insensitive match), in which case `request`'s value wins").
2. **Build `http::Request<()>`** via `http::Request::builder().method(request.method.as_str()).uri(format!("http://{authority}{}", request.path)).version(http::Version::HTTP_2)`.
3. **Apply request headers, stripping H2-forbidden hop-by-hop names defensively.** Reuse a shared `H2_FORBIDDEN_HOP_BY_HOP` constant (the planner picks at Task 1 between (a) re-using 05.2's `response.rs` constant directly, (b) creating a `headers.rs` module in `envoy-http2`; recommendation is (b) since the symmetric strip happens at both codec edges). Lowercase header names defensively (the `h2` crate enforces lowercase per parent §6 signpost 11). Skip `Host:` (it became `:authority` above).
4. **Send the request** via `self.send_request.send_request(http_request, end_of_stream)` where `end_of_stream` is `true` iff the body is empty. If non-empty, write it via `send_stream.send_data(body, end_of_stream: true)`. CL-only — chunked/streaming request bodies deferred per §4.
5. **Read the response** via `response_future.await` returning `http::Response<h2::RecvStream>`; map `h2::Error` to `Http2Error::H2SendRequest`.
6. **Drain the response body** by looping `recv_stream.data().await` (returns `Option<Result<Bytes, h2::Error>>`) concatenating into a single `Bytes`; map errors to `Http2Error::H2RecvBody`. Mirrors 05.2 D3's listener-side body intake pattern.
7. **Translate back into `envoy_http1::codec::Response`** with the status from `response.status()`, headers from `response.headers()` (lowercased name strings; `to_str` skipping malformed value bytes), and the drained body bytes. The `Response` value type is reused unchanged from `envoy_http1::codec::Response` per parent §3 cross-sub-phase architectural rule 2 (the H1-vs-H2 distinction lives at the codec layer at the connection edge, not in the value types).

**`Http2Error` extension** in `crates/envoy-http2/src/error.rs` (the 05.2-landed enum gains 4 new variants, additive — the existing 6 variants from 05.2 D3 stay unchanged):

```rust
// 05.2-landed variants (unchanged):
//   H2Handshake { source: h2::Error }       — listener handshake
//   H2StreamAccept { source: h2::Error }    — listener stream accept
//   H2BodyRead { source: h2::Error }        — listener-side body read
//   MissingAuthority                         — listener-side
//   MalformedH2HeaderBlock                   — both sides
//   BadStatusCode { status: u16 }            — both sides

// 05.3 NEW — additive:
#[error("upstream H2 connect to {addr} failed: {source}")]
UpstreamConnect {
    addr: std::net::SocketAddr,
    #[source]
    source: std::io::Error,
},

#[error("client-side H2 handshake failed: {source}")]
H2ClientHandshake {
    #[source]
    source: h2::Error,
},

#[error("client-side H2 send_request failed: {source}")]
H2SendRequest {
    #[source]
    source: h2::Error,
},

#[error("client-side H2 response body read failed: {source}")]
H2RecvBody {
    #[source]
    source: h2::Error,
},
```

**Tests in `crates/envoy-http2/src/client.rs::tests`** (~8 tests projected):

1. `connect_succeeds_against_in_process_h2_listener` — spawns an in-process `h2::server` on a TcpListener; calls `Client::connect(addr, "test.example")`; asserts `Ok(ClientStream { .. })`. Verifies the H2C handshake completes end-to-end against a known-good H2 server.
2. `connect_returns_upstream_connect_on_refused` — calls `Client::connect(<unbound-port>, "test.example")`; asserts `Err(Http2Error::UpstreamConnect { addr, source })` where `source.kind() == ConnectionRefused`.
3. `send_request_writes_get_with_synthesized_pseudoheaders` — spawns an in-process h2 server that captures the received request; calls `Client::connect(addr, "test.example")` then `send_request(GET / with no Host:)`; asserts the captured request carries `:method = "GET"`, `:path = "/"`, `:authority = "test.example"`, `:scheme = "http"`.
4. `send_request_explicit_host_header_wins_over_captured_host` — same as test 3 but the request carries `Host: real.example` explicitly; asserts `:authority = "real.example"` (NOT `test.example`); mirrors 04.3 `envoy_http1::Client`'s host-resolution behavior per 04.3 SPEC §3 D1.
5. `send_request_reads_response_status_headers_body` — in-process h2 server returns `200 OK` with header `content-type: text/plain` and body `b"hello\n"`; asserts the returned `Response.status == 200`, headers contain `content-type: text/plain`, body bytes are `b"hello\n"`.
6. `send_request_drains_multi_frame_response_body` — in-process h2 server emits the response body across 3 DATA frames (e.g., chunks of 4 bytes each across a 12-byte body); asserts the drained body matches the concatenated bytes.
7. `send_request_strips_h2_forbidden_hop_by_hop_headers` — input request carries `connection: close`, `transfer-encoding: chunked`, `keep-alive: timeout=5`; asserts the in-process h2 server's captured request carries NONE of these names.
8. `send_request_maps_h2_handshake_failure_to_typed_error` — opens a TCP listener that responds to the handshake with garbage bytes (e.g., `b"GARBAGE"` instead of the expected H2 SETTINGS frame); calls `Client::connect`; asserts `Err(Http2Error::H2ClientHandshake { source })`.

**LoC estimate D1:** ~250 LoC `client.rs` impl (Client::connect + ClientStream + send_request + the request/response translation + the hop-by-hop strip) + ~250 LoC unit tests (8 tests × ~30 LoC each including the in-process h2-server scaffolding) + ~30 LoC `error.rs` extension (4 new variants) + ~5 LoC `lib.rs` re-export. Total D1: **~535 LoC**, ~40% of the 05.3 LoC budget.

### D2 — `envoy-config` schema additions for cluster-side H2 protocol options

Two coordinated edits in `crates/envoy-config/src/bootstrap.rs`:

**D2.a — `typed_extension_protocol_options` on `Cluster`.** The existing `Cluster` struct (at lines 46–56 of HEAD `e626862` per 05.1 SPEC §1; the 05.1 schema growth added `StrictDns` to `ClusterType` but did NOT touch `Cluster`'s field set; `transport_socket: Option<TransportSocket>` is the existing 03.2 field) gains a new optional `typed_extension_protocol_options: Option<TypedExtensionProtocolOptions>` field. The supporting type hierarchy:

```rust
// 05.3 NEW additions in crates/envoy-config/src/bootstrap.rs:

pub struct TypedExtensionProtocolOptions {
    #[serde(rename = "envoy.extensions.upstreams.http.v3.HttpProtocolOptions")]
    pub http_protocol_options: HttpProtocolOptions,
}

pub struct HttpProtocolOptions {
    #[serde(rename = "@type")]
    pub type_url: String,                          // validator: "type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions"
    pub explicit_http_config: ExplicitHttpConfig,
}

pub struct ExplicitHttpConfig {
    #[serde(default)]
    pub http_protocol_options: Option<Http1ProtocolOptions>,   // empty struct in 05.3 (oneof H1 arm)
    #[serde(default)]
    pub http2_protocol_options: Option<Http2ProtocolOptions>,  // reuse 05.2 D2.b struct (oneof H2 arm)
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Http1ProtocolOptions { /* empty in 05; chunk_encoding etc. defer per §4 */ }
```

(All structs `#[derive(Debug, Deserialize, PartialEq)]` with `#[serde(deny_unknown_fields)]` per the established envoy-config posture — boilerplate elided for brevity.)

The `Http2ProtocolOptions` struct from 05.2 D2.b (`crates/envoy-config/src/bootstrap.rs::Http2ProtocolOptions`) is reused unchanged — same 4 optional `u32` fields (`max_concurrent_streams`, `initial_stream_window_size`, `initial_connection_window_size`, `max_frame_size`); same RFC 7540 range checks (`max_frame_size` in `[16384, 16777215]`; window sizes in `[0, 2^31 - 1]`); same `ConfigError::Http2ProtocolOptionsOutOfRange { field, value, range }` rejection variant. The validator path that runs the range checks is invoked on both the listener-side use (at `HttpConnectionManagerConfig.http2_protocol_options`) and the cluster-side use (at `Cluster.typed_extension_protocol_options.http_protocol_options.explicit_http_config.http2_protocol_options`); the rejection variant carries no use-site discriminator (the `field` member on `Http2ProtocolOptionsOutOfRange` is a `&'static str` naming the field, not the use site).

Validator extensions:

1. **Mutual exclusion of `http_protocol_options` / `http2_protocol_options` inside `explicit_http_config`.** Envoy's `ExplicitHttpConfig` is a oneof; at most one of the two arms may be set. If both `Some`, reject with `ConfigError::MutuallyExclusiveExplicitHttpConfig { cluster: String }`. New `ConfigError` variant.
2. **`@type` URL well-formedness.** The `type_url` field on `HttpProtocolOptions` must equal `"type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions"` exactly. Mismatch rejects with a typed error (`ConfigError::UnsupportedTypedConfigUrl { got: String, expected: &'static str }` — verifiable at task-1 time whether this variant already exists from earlier phases' `typed_config` validation; if it doesn't exist for this exact use, the planner adds it; recommendation: re-use whatever the existing `TypedConfig` arm validation uses for the listener-side HCM `@type`, since the @type-validation pattern is identical).
3. **`Http2ProtocolOptions` range checks.** Apply the same range checks as 05.2 D2.b — `max_frame_size`, window sizes per RFC 7540. Re-uses `ConfigError::Http2ProtocolOptionsOutOfRange`.

**D2.b — `ConfigError` extension in `crates/envoy-config/src/lib.rs`.** Add 1 new variant:

```rust
#[error("cluster '{cluster}': explicit_http_config has both http_protocol_options and http2_protocol_options set; at most one is permitted")]
MutuallyExclusiveExplicitHttpConfig {
    cluster: String,
},
```

If `ConfigError::UnsupportedTypedConfigUrl` doesn't already exist (the planner verifies at task-1 time by `grep -n 'UnsupportedTypedConfigUrl\|@type' crates/envoy-config/src/lib.rs`), add it too:

```rust
#[error("typed config @type {got:?} not supported; expected {expected:?}")]
UnsupportedTypedConfigUrl {
    got: String,
    expected: &'static str,
},
```

(If the existing typed-config validator pattern uses a different rejection variant, the planner mirrors that pattern instead — see signpost 6 below.)

Re-exports in `crates/envoy-config/src/lib.rs`'s `pub use bootstrap::{...}` block extend with `TypedExtensionProtocolOptions`, `HttpProtocolOptions`, `ExplicitHttpConfig`, `Http1ProtocolOptions`. (The `Http2ProtocolOptions` struct is already re-exported per 05.2 D2.b.)

**Validator unit tests appended** to `crates/envoy-config/src/bootstrap.rs::tests` (~8 tests projected):

1. `parses_cluster_with_typed_extension_protocol_options_http2` — full bootstrap with one cluster carrying the full `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options { ... 4 fields ... }` block; validator accepts; struct round-trips.
2. `parses_cluster_with_typed_extension_protocol_options_http1` — full bootstrap with one cluster carrying `explicit_http_config.http_protocol_options: {}` (empty Http1 options); validator accepts.
3. `rejects_cluster_with_both_http1_and_http2_in_explicit_http_config` — full bootstrap with one cluster carrying BOTH `http_protocol_options: {}` AND `http2_protocol_options: { ... }`; validator returns `MutuallyExclusiveExplicitHttpConfig { cluster }`.
4. `rejects_cluster_with_wrong_typed_config_url` — full bootstrap with one cluster carrying `@type: type.googleapis.com/envoy.config.core.v3.Http2ProtocolOptions` (the listener-side type URL, not the cluster-side one); validator returns the URL-mismatch error.
5. `rejects_cluster_http2_protocol_options_max_frame_size_too_small` — `max_frame_size: 1024` (below 16384) on the cluster side; validator returns `Http2ProtocolOptionsOutOfRange { field: "max_frame_size", value: 1024, range: (16384, 16777215) }` (re-uses the 05.2-landed variant — confirms the cluster-side range check fires identically to the listener-side).
6. `parses_cluster_without_typed_extension_protocol_options_defaults_to_http1` — full bootstrap with one cluster declaring only `name` / `type` / `lb_policy` / `load_assignment` (no `typed_extension_protocol_options`); validator accepts; the struct's `typed_extension_protocol_options: None`. (The `Cluster.upstream_protocol` field projection from D3 below defaults to `Http1` for this case; the test's job is to verify the parse-side accepts the absent field.)
7. `rejects_cluster_with_unknown_typed_extension_key` — a cluster carrying `typed_extension_protocol_options: { "envoy.extensions.upstreams.http.v3.UnknownExtension": { ... } }` (a key other than `HttpProtocolOptions`); serde `deny_unknown_fields` rejects with the standard "unknown field" error since `TypedExtensionProtocolOptions` only declares the `HttpProtocolOptions` field.
8. `parses_cluster_with_strict_dns_and_http2_protocol_options_combined` — a cluster carrying BOTH `type: STRICT_DNS` (per 05.1) AND `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options`; validator accepts; round-trips. **Load-bearing for fixture 0010** which combines exactly these two surfaces.

Plus 1 corpus-walk acceptance test mirroring 04.2/05.2's pattern: `fuzz_corpus_cluster_http2_protocol_options_seed_parses` reads the new `cluster_http2_protocol_options.yaml` seed via `include_str!` and confirms it parses cleanly.

**Fuzz corpus extension.** `crates/envoy-config/fuzz/corpus/parse_bootstrap/` gains 1 new seed:

- `cluster_http2_protocol_options.yaml` — full bootstrap with one HCM listener (`codec_type: HTTP2` + listener-side `http2_protocol_options` minimal — re-uses the 05.2 listener-side surface) + one cluster of `type: STRICT_DNS` + `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options` carrying the four-field subset + endpoint at `localhost:7000`. Mirrors the existing 04.x + 05.1 + 05.2 seed shape. The seed exercises the validator's accept-path on the cluster-side `Http2ProtocolOptions`; the fuzzer never runs the H2 codec or the runtime cluster construction (`parse_bootstrap` only exercises serde + the validator).

Allow-list entry `!corpus/parse_bootstrap/cluster_http2_protocol_options.yaml` added to `crates/envoy-config/fuzz/.gitignore`.

**LoC estimate D2:** ~120 LoC schema delta (the 4 new structs/enums + the field on `Cluster`) + ~60 LoC validator path (mutual-exclusion + URL-check + range-check delegations) + ~120 LoC unit tests (8 new + 1 corpus-walk × ~13 LoC each) + ~25 LoC fuzz seed YAML + ~10 LoC `ConfigError` extension. Total D2: **~335 LoC**.

### D3 — `Cluster.upstream_protocol` field on `crates/envoy-cluster/src/cluster.rs`

`crates/envoy-cluster/src/cluster.rs` (HEAD shape: `Cluster { name, endpoints, cursor }` at lines 11–16; `Cluster::name()` accessor at lines 18–26 from 04.3 D5; `from_bootstrap` at line 112 — the live shape is verified at task-1 time and may also have the `STRICT_DNS` resolution branch landed by 05.1 D2; the planner cross-checks against the post-05.1 HEAD shape) gains the `upstream_protocol` field + the new `UpstreamProtocol` enum.

**New enum `UpstreamProtocol` in `crates/envoy-cluster/src/cluster.rs`:**

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpstreamProtocol {
    #[default]
    Http1,
    Http2,
}
```

`Default` derives `Http1` for backwards-compat with all phase-04 clusters per parent §3 D12.3 / parent §6 signpost 5; `Clone, Copy, Debug, PartialEq, Eq` derives mirror the existing `LbPolicy` posture.

**`Cluster` struct extension** adds a `pub(crate) upstream_protocol: UpstreamProtocol` field alongside the existing `name` / `endpoints` / `cursor` fields at `crates/envoy-cluster/src/cluster.rs:11-16`. Two new accessor pairs land mirroring the `Cluster::name()` / `ClusterHandle::name()` pair from 04.3 D5 at `crates/envoy-cluster/src/cluster.rs:24-26` and `:60-62`:

```rust
impl Cluster {
    pub fn upstream_protocol(&self) -> UpstreamProtocol { self.upstream_protocol }
}
impl ClusterHandle {
    pub fn upstream_protocol(&self) -> UpstreamProtocol { self.inner.upstream_protocol() }
}
```

**`from_bootstrap` extension** projects `upstream_protocol` from the parsed cluster's `typed_extension_protocol_options` per a sync `match`:

- `None` → `UpstreamProtocol::Http1` (default; backwards-compat with all phase-04 clusters).
- `Some(teo)` with `explicit_http_config.http2_protocol_options: Some(_)` → `UpstreamProtocol::Http2`.
- `Some(teo)` with `explicit_http_config.http_protocol_options: Some(_)` (the empty H1 oneof arm) → `UpstreamProtocol::Http1`.
- The "both Some" case is unreachable at runtime (D2.a's `MutuallyExclusiveExplicitHttpConfig` validator rejects it at parse time); the projection covers it with `UpstreamProtocol::Http1` defense-in-depth.

The 05.1 `STRICT_DNS` resolution branch in `from_bootstrap` is unchanged; the `upstream_protocol` projection runs alongside it (the two are orthogonal — `cluster_type` controls endpoint resolution shape, `upstream_protocol` controls upstream protocol dispatch).

**Tests in `crates/envoy-cluster/src/cluster.rs::tests`** (~3 new tests):

1. `cluster_upstream_protocol_defaults_to_http1` — `from_bootstrap` on a YAML where the cluster declares no `typed_extension_protocol_options`; assert `mgr.get("backend").unwrap().upstream_protocol() == UpstreamProtocol::Http1`.
2. `cluster_upstream_protocol_http2_set_from_typed_extension_protocol_options` — `from_bootstrap` on a YAML where the cluster declares the full `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options` block; assert `upstream_protocol() == UpstreamProtocol::Http2`.
3. `cluster_upstream_protocol_http1_set_from_explicit_http1_options` — `from_bootstrap` on a YAML where the cluster declares `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http_protocol_options: {}` (the empty Http1 variant); assert `upstream_protocol() == UpstreamProtocol::Http1`.

**LoC estimate D3:** ~30 LoC enum + accessor pair + struct field + `from_bootstrap` projection + ~50 LoC tests (3 × ~17 LoC) + the new YAML constants for the test inputs (~30 LoC). Total D3: **~110 LoC**.

### D4 — Router H2-arm at `crates/envoy-http1/src/hcm.rs` + symmetric H2-side dispatch

Extends the 04.3-landed `BuildOutcome::Proxy` arm at `crates/envoy-http1/src/hcm.rs:189-288` to dispatch H1-or-H2 based on `cluster.upstream_protocol()`. The exact line range is verified at 05.3 Task 1 against post-05.2 HEAD (05.2 D4 may have made minor edits to envoy-bin's HCM dispatch arm — but 05.2 SPEC §3 D4's design explicitly leaves `envoy_http1::HCM`'s internals alone, so the 04.3-era line range should still be authoritative).

**The dispatch site.** The 04.3 shape at `crates/envoy-http1/src/hcm.rs:243-277` (verbatim from the live file at HEAD `e626862`) reads:

```rust
let start = std::time::Instant::now();
let mut client_stream = match Client::connect(endpoint, &host_header).await {
    Ok(s) => s,
    Err(source) => { /* tracing::warn! + synth_status(502) + write + continue */ }
};
let upstream_response = match client_stream.send_request(out_req).await {
    Ok(r) => r,
    Err(source) => { /* same 502 fallback */ }
};
let elapsed_ms = start.elapsed().as_millis();

crate::router::write_proxied_response(&mut downstream, upstream_response, elapsed_ms, close).await?;
```

The 05.3 dispatch wraps the `Client::connect` + `send_request` pair in a `match cluster.upstream_protocol()`:

```rust
let start = std::time::Instant::now();
let upstream_response = match cluster.upstream_protocol() {
    envoy_cluster::UpstreamProtocol::Http1 => {
        // EXISTING 04.3 path — unchanged: Client::connect + send_request +
        // 502 fallback on either Err.
        let mut client_stream = envoy_http1::Client::connect(endpoint, &host_header).await
            .map_err(/* tracing::warn! + 502 fallback + continue */)?;
        client_stream.send_request(out_req).await.map_err(/* 502 fallback */)?
    }
    envoy_cluster::UpstreamProtocol::Http2 => {
        // 05.3 NEW. Same 502 fallback shape; envoy_http2::Client surface
        // mirrors envoy_http1::Client per D1, so the call site is symmetric.
        let mut client_stream = envoy_http2::Client::connect(endpoint, &host_header).await
            .map_err(/* tracing::warn! + 502 fallback + continue */)?;
        client_stream.send_request(out_req).await.map_err(/* 502 fallback */)?
    }
};
let elapsed_ms = start.elapsed().as_millis();
crate::router::write_proxied_response(&mut downstream, upstream_response, elapsed_ms, close).await?;
```

`envoy_http2::Client::send_request` accepts `envoy_http1::codec::Request` and returns `envoy_http1::codec::Response` per D1's contract, so the value types match and no wrapper conversion is needed. The 502 fallback shape (`tracing::warn!(cluster, addr, error)` + `synth_status(502, close)` + `Http1Response::write_to(&mut downstream)` + `if close { return Ok(()); } else { continue; }`) is duplicated structurally across both arms; the planner extracts a small `synth_502_and_continue` helper if the duplication exceeds ~30 LoC, or keeps the inline match if not (recommendation: inline, matching 04.3's posture).

**Symmetric dispatch at `crates/envoy-http2/src/hcm.rs`'s `BuildOutcome::Proxy` site.** Per 05.2 SPEC §3 D3 test 6 `h2_proxy_outcome_returns_502_in_05_2`, 05.2 stubbed the H2 listener-side HCM's `BuildOutcome::Proxy` arm with a 502 Bad Gateway response. 05.3 replaces the stub with the symmetric H1-or-H2 dispatch (when an H2 listener proxies to a cluster, the upstream may be H1 or H2 depending on the cluster's `upstream_protocol` field — same as the H1 listener). 05.2 SPEC §3 D3 test 6's projection reads: *"Will be replaced in 05.3 D13.3 with the actual upstream H2 dispatch — at 05.3 task time, this test is renamed to `h2_proxy_outcome_dispatches_to_upstream` and the assertion flips to a 200 from the upstream."* 05.3 does this rename + assertion flip, and the 05.2 stub helper (the 502 with the "upstream H2 not yet wired (sub-phase 05.3)" body per 05.2 §6 signpost 21) is removed.

**Tests in `crates/envoy-http1/src/hcm.rs::tests`** (~3 new tests):

1. `proxy_arm_dispatches_h1_for_http1_cluster` — HCM with a cluster of `upstream_protocol: Http1`; mocked H1 upstream returns 200; assert the response status is 200, the upstream-side bytes match an H1 request shape (verified by capturing the upstream-bound bytes via a TcpListener instead of an actual `envoy_http1::Client` mock — same test scaffolding pattern 04.3 uses).
2. `proxy_arm_dispatches_h2_for_http2_cluster` — HCM with a cluster of `upstream_protocol: Http2`; in-process h2 server returns 200 with body `"h2"`; assert the downstream-bound HTTP/1.1 response carries body `"h2"` (the upstream-direction H2 framing is invisible downstream because `write_proxied_response` reads the envoy `Response` value type which is protocol-agnostic).
3. `proxy_arm_returns_502_on_h2_upstream_connect_refused` — HCM with a cluster of `upstream_protocol: Http2` whose endpoint is an unbound port; assert the downstream-bound response is 502 (mirrors 04.3's H1 502 fallback test); the `tracing::warn!` line at the H2 arm fires.

**Tests in `crates/envoy-http2/src/hcm.rs::tests`** (~2 new tests; rename 05.2's test 6):

4. `h2_proxy_outcome_dispatches_to_upstream` (renamed from 05.2's `h2_proxy_outcome_returns_502_in_05_2`) — H2 listener-side HCM with a cluster of `upstream_protocol: Http2`; mocked H2 upstream returns 200; assert the downstream H2 response is 200.
5. `h2_proxy_outcome_dispatches_to_h1_upstream_when_cluster_is_http1` — H2 listener-side HCM with a cluster of `upstream_protocol: Http1`; mocked H1 upstream returns 200; assert the downstream H2 response is 200 (verifies that the H2 listener can proxy to an H1 cluster — though parent §4 lists "cross-protocol H2↔H1 translation" as a non-goal in the **dedicated cross-protocol-translation-layer sense** (request/response framing translation across the proxy edge with full feature support such as trailers/streaming bodies/etc.); the simple H2-listener-to-H1-cluster path through the existing route-walk + `Client::connect` polymorphism is naturally supported because the HCM core operates on the protocol-agnostic `Request`/`Response` value types).

**LoC estimate D4:** ~70 LoC dispatch wrap at `envoy-http1/src/hcm.rs:189-288` (the H1 arm of the new match is ~unchanged from 04.3; the H2 arm is the new ~40 LoC of dispatch + 502 fallback) + ~30 LoC symmetric dispatch at `envoy-http2/src/hcm.rs` (replacing 05.2's 502 stub) + ~80 LoC unit tests (5 tests × ~16 LoC). Total D4: **~180 LoC**.

### D5 — `tests/helpers/http2-echo-server/` workspace member

New helper crate at `tests/helpers/http2-echo-server/` (sibling of 04.3's `tests/helpers/http1-echo-server/` per parent §3 D14.3 / parent §6 signpost 7). Plaintext H2C only (no TLS). Hand-parsed argv (`--port <u16>` + `--help` + `--version` per the 04.3 task-11 review-fix shape that ships at `tests/helpers/http1-echo-server/src/main.rs:42-65`).

**`Cargo.toml`:**

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
thiserror = "2"
tokio = { version = "1", features = ["rt-multi-thread", "net", "io-util", "macros", "signal", "time", "sync"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
```

Mirrors the 04.3-landed `tests/helpers/http1-echo-server/Cargo.toml` shape — same dependency set modulo the foundation-crate substitution (`envoy-http2` over `envoy-http1`); per parent §6 signpost 7, the helper consumes `envoy-http2` (NOT `h2` directly) so the cross-sub-phase architectural rule "only `envoy-http2` depends on `h2`" stays enforced even for test helpers.

**Module structure:** `tests/helpers/http2-echo-server/{Cargo.toml, src/main.rs}` with `#![forbid(unsafe_code)]` per D-3.8. The argv parser (`--port <u16>` + `--help` + `--version`) mirrors `tests/helpers/http1-echo-server/src/main.rs:42-65` verbatim. The connection-accept loop spawns a `tokio::task` per accepted TCP connection; each task runs `h2::server::Builder::new().handshake(tcp).await` (via the new `envoy_http2::codec::server_handshake` thin wrapper — see below) then loops over `Connection::accept` to receive per-stream `(http::Request<h2::RecvStream>, h2::server::SendResponse<bytes::Bytes>)` pairs; for each stream, builds the deterministic echo body (method + path + alphabetically-sorted headers + body, matching `http1-echo-server`'s body shape exactly so cross-protocol fixtures remain comparable) and writes it via `SendResponse::send_response` + `SendStream::send_data(.., end_of_stream: true)`.

**Codec-edge thin wrapper to satisfy parent §6 signpost 7.** Per signpost 7, `http2-echo-server` consumes `envoy_http2`, NOT `h2` directly — but the listener-side surface 05.2 D1 ships (`hcm.rs`) is HCM-on-H2 with route-walk + filter-chain, which is overkill for an echo helper that produces method/path/headers/body echo regardless of routing. The planner extends `crates/envoy-http2/src/codec.rs` (the existing thin `h2::server::Builder` adapter from 05.2 D1) with a small `pub fn server_handshake(tcp: TcpStream) -> Result<h2::server::Connection<...>, Http2Error>` (or similar — exact signature at PLAN.md writeup) that re-exports `h2::server::Builder::handshake` adequately for the helper's needs without forcing the HCM machinery. ~30 LoC addition at `codec.rs`. Alternative: a new `crates/envoy-http2/src/server.rs` module — planner picks at Task 1; recommendation is the `codec.rs` extension since `codec.rs` is already the thin Builder adapter.

**Tests** (~5 tests in `tests/helpers/http2-echo-server/src/main.rs::tests`):

1. `parse_argv_accepts_port` — argv `["--port", "7000"]`; returns `Ok(Args { port: 7000 })`.
2. `parse_argv_rejects_missing_port` — argv `[]`; returns `Err(MissingFlag("--port"))`.
3. `parse_argv_help_returns_help_requested` — argv `["--help"]`; returns `Err(HelpRequested)`.
4. `parse_argv_version_returns_version_requested` — argv `["--version"]`; returns `Err(VersionRequested)`.
5. `echo_round_trip_against_in_test_h2_client` — spawns the echo server on a reserved ephemeral port; opens an h2 client connection; sends a `GET /test HTTP/2.0` with `Host: testharness` header; reads the response; asserts status 200 + body matches the deterministic-echo format with `method: GET\npath: /test\nheaders:\n  ...\nbody:\n` shape (the exact byte-string is the byte-exact test assertion).

**LoC estimate D5:** ~250 LoC `main.rs` impl (argv parse + connection-accept loop + per-stream task + deterministic-body construction + the SendResponse + SendStream::send_data pattern) + ~60 LoC unit tests (5 × ~12 LoC) + ~20 LoC `Cargo.toml`. Total D5: **~330 LoC**.

### D6 — Differential harness `Http2EchoBackend` + `{{HTTP2_BACKEND_PORT}}` template marker

Three coordinated edits to `tests/differential/`:

**D6.a — `Http2EchoBackend`** at `tests/differential/src/backend.rs` (sibling of 04.3-landed `Http1EchoBackend`, 03.2-landed `TlsEchoBackend`, 02.2-landed `TcpProxyBackend`). Public surface mirrors `Http1EchoBackend`'s exactly: `spawn() -> anyhow::Result<Self>` (locates binary via `locate_http2_echo_server`; reserves an ephemeral port; spawns the subprocess via `tokio::process::Command::new(bin).arg("--port").arg(port.to_string()).spawn()`; polls accept-readiness with a 5s budget), `port() -> u16`, `container_host() -> &'static str` (returns `"host.docker.internal"` per ADR-0015 — with `STRICT_DNS` from 05.1's preamble the resolution-rejection regression is no longer in play), `impl Drop` issuing `child.start_kill()` (SIGKILL-on-Drop; mirrors 02.2 REVIEW M1's polling-loop shape; M1 carryforward awareness-only, continues unchanged through 05.3 — the `std::thread::sleep` from a tokio-runtime thread issue is inherited verbatim).

The accept-readiness poll opens a TCP connection to the port and runs `h2::client::handshake` via `tokio::time::timeout` — success means the helper has completed its codec setup, not just that it's accepting TCP. (Alternative: simple TCP-connect-success polling like 04.3's H1 helper uses; the planner picks at Task 1 — recommendation: H2 handshake polling because the codec setup is what makes the helper actually ready to serve.) `locate_http2_echo_server()` is a sibling of 04.3's `locate_http1_echo_server` in the same `backend.rs` module; same lookup pattern (`target/<profile>/http2-echo-server`).

**D6.b — `run_fixture` template-marker substitution** in `tests/differential/src/lib.rs`. The existing `run_fixture` cascade (per 04.3 D14 / 05.2 D5.c shape — `port_key` match per fixture) gains a new arm dispatching fixture `0010-http2-router-upstream` to spawn `Http2EchoBackend` and substitute `{{HTTP2_BACKEND_PORT}}` in both `envoy.yaml` and `envoy-rust.yaml` at render time. Mirrors the 04.3-landed `{{HTTP1_BACKEND_PORT}}` substitution exactly.

**D6.c — `Driver::Http2` reuse, no new variant.** The `Driver::Http2` variant + the `drive_http2` async helper landed in 05.2 D5.a / D5.b are reused unchanged for fixture 0010. The shape mirrors 04.3 fixture 0008's reuse of `Driver::Http1` from 04.1.

**Tests appended** to `tests/differential/src/backend.rs::tests`:

1. `http2_echo_backend_spawns_and_echoes` — spawns `Http2EchoBackend` on a reserved port; opens an H2 client to the backend; sends `GET /test`; asserts a deterministic-shaped echo response.
2. `http2_echo_backend_drop_terminates_child` — spawns, captures the child PID, drops the backend; asserts the child process is terminated within ~1s.
3. `locate_http2_echo_server_returns_existing_path` — asserts the locator returns a path that exists at `target/<profile>/http2-echo-server`.

Plus 1 new test in `tests/differential/src/lib.rs::tests`:

4. `run_fixture_dispatches_http2_backend_on_template_marker` — runs `run_fixture` against an in-tree synthetic fixture YAML carrying `{{HTTP2_BACKEND_PORT}}`; asserts the backend was spawned and the substitution occurred.

**LoC estimate D6:** ~120 LoC `Http2EchoBackend` + locator + `wait_for_h2_accept_ready` + ~20 LoC `run_fixture` cascade extension + ~80 LoC unit tests (4 × ~20 LoC). Total D6: **~220 LoC**.

### D7 — Fixture `0010-http2-router-upstream/`

5 files in `tests/fixtures/0010-http2-router-upstream/`, mirroring 04.3's fixture-0008 shape and 05.2's fixture-0009 shape (with the cluster-side `typed_extension_protocol_options` block landed in D2.a).

**`envoy.yaml`** (template; harness substitutes `{{PORT}}`, `{{BACKEND_HOST}}`, `{{HTTP2_BACKEND_PORT}}` at render time):

```yaml
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
      type: STRICT_DNS                # per 05.1's schema growth
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

The `http2_protocol_options: {}` empty block is intentional — it selects H2 upstream while leaving all four `Http2ProtocolOptions` fields at their `h2`-crate defaults. Fixture 0010 doesn't tune H2 settings explicitly; if a future fixture needs to test specific `max_concurrent_streams` / window-size behavior, that fixture lands then.

**`envoy-rust.yaml`** (per-side divergences from `envoy.yaml`):
- bind `127.0.0.1` instead of `0.0.0.0`.
- no `admin` block (envoy-rust runs without admin in this fixture).
- `request_headers_to_remove` is omitted (envoy-rust does not inject these per parent SPEC §4 carryforward — field-set divergence is intentional, mirrors fixture 0008's `tests/fixtures/0008-http1-router-upstream/envoy-rust.yaml`'s posture).
- `generate_request_id: false` is omitted (envoy-rust does not inject `x-request-id` by default).

**`inputs/payload.bin`:** empty (0 bytes) — the GET has no request body. The `Driver::Http2` constructs the request from the driver fields per 05.2 D5.a; the file is present for harness-shape consistency with other fixtures but unread.

**`expectations.yaml`:**

```yaml
driver:
  kind: http2
  method: GET
  path: "/"
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
  expected_headers:
    rule: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body: byte_exact
```

The exact byte-exact body shape depends on the request-header set the harness emits — both proxies forward to `http2-echo-server` whose deterministic-echo body lists every received request header alphabetically by lowercase name. The H2 request carries `:authority` / `:method` / `:path` / `:scheme` pseudo-headers per RFC 7540 §8.1.2.3 + parent §6 signpost 12; if `http2-echo-server` lists them in the response body alongside regular headers, the body asserts on those + any regular headers the harness emits. The exact shape is determined at task-time by running the harness end-to-end against a known-good envoy-rust + Envoy pair and capturing the byte-exact response. The above is a planner-time projection; the planner refines at PLAN.md writeup or at fixture-write task time.

**`README.md`** (~30 lines describing the fixture surface, the H2C handshake, the H2-on-H2 round-trip, the cluster-side `Http2ProtocolOptions` configuration, and the cross-references to phase 05.3 SPEC §3 D7 + parent SPEC §3 D15.3 + 05.1 SPEC §3 D3 for the `STRICT_DNS` cluster type).

**Docker-gated test:** `tests/differential/tests/http2_router_upstream.rs` — 7-line wrapper:

```rust
#[tokio::test]
async fn http2_router_upstream() {
    differential::run_fixture("0010-http2-router-upstream").await.expect("fixture green");
}
```

**In-process integration backstop:** `crates/envoy-bin/tests/http2_router_upstream.rs` (sibling of 04.3's `crates/envoy-bin/tests/http1_router_upstream.rs` and 05.2's `crates/envoy-bin/tests/http2_direct_response.rs`). Spawns envoy-bin via `CARGO_BIN_EXE_envoy-bin` against an HCM-`HTTP2`-listener config that points its `backend` cluster at an in-test-spawned `http2-echo-server`; drives a single H2C `GET /` via `h2::client`; asserts the parsed response. ~150 LoC + spawned-process boilerplate.

**LoC estimate D7:** ~80 LoC fixture YAMLs (envoy.yaml + envoy-rust.yaml) + ~30 LoC README + ~25 LoC expectations.yaml + 7 LoC Docker-gated test wrapper + ~150 LoC in-process backstop. Total D7: **~292 LoC**.

### D8 — Parent-phase-05 close-out artifact wiring

**No new code in D8.** The state-6 phase-done commit for 05.3 is also the parent-phase-05 close-out commit; this deliverable enumerates the close-out wiring per parent SPEC §8 (artifacts amended at sub-phase state-6 commits) and §9 (parent close-out commit format).

The 05.3 state-6 commit:

1. **Flips ROADMAP row `05.3` `status` `in-progress` → `done`.**
2. **Flips parent ROADMAP row `05` `status` `in-progress` → `done`** per the ROADMAP-schema invariant (the parent flips when all sub-phases are done; 05.1 and 05.2 already done).
3. **Advances STATE.md** from `05.3-http2-upstream` lifecycle state 6 to phase `06-<slug>` lifecycle state 1 (phase-06 directory does not exist; next-skill `superpowers:brainstorming` scoped to phase 06 — *"Access log (file sink, Envoy default format) + stats + Prometheus admin endpoint"* per `BOOTSTRAP_PROMPT.md` §8 row 06). The slug is whatever the phase-06 brainstorm picks; expected `06-access-log-stats` or similar but the planner does not pre-decide.
4. **Adds Phase-05.3 rollovers Notes subsection** to STATE.md per the established phase-04.3 / phase-04.2 / phase-04.1 / phase-03.2 / phase-03.1 / phase-02.2 / phase-02.1 / phase-01 rollovers cadence — enumerates the 05.3 REVIEW.md verdict (anticipated: Approved with M-track follow-ups at most), in-phase closures (none), and the awareness-only items + any cross-phase carryforwards (M1 `*EchoBackend::Drop` polling continues; M-claim continues; M1/M2/M4 from 04.1 unchanged unless 05.3 surfaced something concrete).
5. **No DECISIONS.md edits anticipated** (per §7 below, no new ADRs are projected for 05.3 at state-2; if an unforeseen ambiguity surfaces during execution per D-3.5, the planner appends the next-sequential ADR — likely ADR-0024 or ADR-0026 depending on whether ADR-0024/0025 landed at 05.2 — at the time it lands).
6. **Parent SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md` is NOT edited** — remains the historical artifact committed at parent-05 state-1 SHA `cd1a70e` per D-3.4 / D-3.5. Same posture as parent-04 SPEC at `docs/envoy-rust/phases/04-http1/SPEC.md` after the 04.3 close-out.

LoC estimate D8: 0 LoC code, ~30 lines of ROADMAP / STATE.md / PROGRESS / REVIEW prose at the close-out commit.

---

## 4. Non-goals (subset of parent SPEC §4 that bind on 05.3)

The following are out of scope for 05.3 and defer to later phases. The list is a subset of parent-05 SPEC §4, scoped to items that are predictably tempting to fold into 05.3 by a planner reading only this SPEC.

**Deferred to later phases (per parent-05 SPEC §4 — items relevant to the upstream H2 / cluster-side surface):**

- **Connection pooling on upstream H2.** H2's stream multiplexing means one pooled connection serves many streams; the pool design is materially richer than H1's (LRU/round-robin connection selection is insufficient — the pool must also track stream-count vs. `MAX_CONCURRENT_STREAMS`, handle `GOAWAY` frames mid-pool, etc.). 05.3's `Client` implementation is explicitly per-connection — one TCP connection per upstream call. **Upstream-robustness family** territory. Future phases that ship pooling will likely refactor `Client` into a pool-aware shape (per-connection `ClientStream` becomes an entry in a pool keyed by `(addr, host)`); 05.3's narrow scope avoids prematurely committing to a pool design.
- **HTTP/2 over TLS (ALPN-negotiated H2).** Listener-side ALPN config (`common_tls_context.alpn_protocols: ["h2", "http/1.1"]`), upstream-side ALPN (`UpstreamTlsContext.alpn_protocols`), and codec-dispatch-by-ALPN. The validator continues to reject TLS+`codec_type: HTTP2` combos via 05.2's `ConfigError::Http2OverTlsNotSupported`. Carries the **M7 carryforward** (`TlsAcceptingHandler.inner: Arc<TcpProxy>` concrete-typed; HCM-in-TLS doesn't typecheck — phase-04.1 REVIEW M7) forward to whichever phase ships ALPN-driven dispatch. Cluster-side TLS+H2 is a combinatorial extension of M7; defers to whichever phase first ships TLS+H2.
- **Cross-protocol H2↔H1 translation in the framing-translation sense.** Specifically: the comprehensive cross-protocol translation layer that handles trailers, streaming bodies, full feature parity, etc. across the proxy edge with high fidelity. Defers to a follow-on phase. Note: the simple H2-listener-to-H1-cluster (and H1-listener-to-H2-cluster) **dispatch** path through the existing `Client::connect` polymorphism on `cluster.upstream_protocol` IS supported in 05.3 — this is structurally automatic because the HCM core (in `envoy-http1`) operates on the protocol-agnostic `Request`/`Response` value types per parent §3 cross-sub-phase architectural rule 2, so once the H1 listener-side HCM lands its H2-cluster dispatch arm and the H2 listener-side HCM lands its H1-cluster dispatch arm, the four combinations (H1-listener × {H1, H2}-cluster; H2-listener × {H1, H2}-cluster) are all exercised. What's deferred is the **richer translation layer** that handles edge cases like streaming request bodies, trailer forwarding, and HPACK-vs-text-headers fidelity — none of those are exercised by fixture 0010.
- **HTTP/2 trailers** (HEADERS frame after END_STREAM on a DATA frame). The helper does not emit trailers; the router H2-arm does not forward trailers; the H2 client wrapper does not parse or write trailers. Trailers (`HEADERS` frame after `END_STREAM` on a DATA frame's stream) are an H2 first-class feature but engaging them requires non-trivial harness work and a doctrine call on whether trailers fall under the existing header allow-list. Defers to a follow-on phase or to whichever phase first emits trailer-bearing responses (gRPC family will likely force this).
- **HTTP/2 server push (`PUSH_PROMISE` frames).** Removed from H3, rarely used in practice, and disabled by default in modern browsers. Deferred indefinitely.
- **HTTP/2 over HTTP/1.1 Upgrade (`Upgrade: h2c`).** Envoy v1.33 does not support this mode on the server side; out of scope indefinitely.
- **HTTP/3 / QUIC.** Separate family per `BOOTSTRAP_PROMPT.md` §9.
- **Per-route `Http2ProtocolOptions` overrides.** Cluster-level only in 05.3 (and listener-level in 05.2). A future phase that needs per-route overrides extends then.
- **`http_protocol_options` (the Http1 arm of `ExplicitHttpConfig`) field set.** 05.3 ships an empty `Http1ProtocolOptions` struct as a placeholder for the oneof's H1 arm. Real fields like `chunk_encoding`, `allow_chunked_length`, `enable_trailers` (cluster-side H1), `accept_http_10`, `default_host_for_http_10`, `header_key_format`, etc. defer to whichever phase first needs cluster-side H1 protocol-tuning.
- **Cross-cluster `http_protocol_options` (top-level on `Cluster` not under `typed_extension_protocol_options`).** Envoy supports an older `Cluster.http_protocol_options` and `Cluster.http2_protocol_options` form (deprecated in favor of the `typed_extension_protocol_options` mechanism 05.3 ships). 05.3 ships only the modern form; the deprecated form rejects at parse time via `serde::deny_unknown_fields` on the `Cluster` struct.
- **HTTP/2 stream-level flow control tuning.** The default `h2`-crate window-size posture is used in 05.3 for the upstream client; per-stream flow-control overrides (beyond the four `Http2ProtocolOptions` fields landed in 05.2 D2.b and reused cluster-side in 05.3 D2.a) defer.
- **HTTP/2 connection draining / `GOAWAY` handling on graceful shutdown.** Phase-08 (graceful drain) territory. Connections close abruptly on listener shutdown / process exit in 05.3.
- **Server-Sent Events / chunked streaming responses on the upstream-bound request side.** 05.3's `Client::send_request` writes the request body as a single body chunk via `h2::SendStream::send_data(.., end_of_stream=true)`; streaming bodies (multiple DATA frames) on the upstream-bound request defer to whichever phase first emits long-lived streaming requests upstream.
- **HCM `server_name` field.** Re-deferred from phase 04 + phase 05 per parent §4. The `server` allow-list row continues to accommodate the divergence.
- **`codec_type: AUTO` byte-sniffing for H2C.** AUTO continues to behave as `HTTP1`-only. Fixture 0010 uses explicit `codec_type: HTTP2` on the listener.
- **`LOGICAL_DNS` cluster type / `dns_refresh_rate` / `dns_lookup_family` / `respect_dns_ttl` / `dns_resolvers`.** All deferred per 05.1 ADR-0023's narrow scoping; 05.3 inherits the deferrals unchanged. Fixture 0010 declares `type: STRICT_DNS` per 05.1's schema (signpost 17 confirms this) and resolves `host.docker.internal` → host-gateway IP at startup via `tokio::net::lookup_host` (envoy-rust side) or Envoy's STRICT_DNS resolver (Envoy side).

**Not deferred — confirmed in scope for 05.3** (for clarity, since these have predictable confusion points):

- `crates/envoy-http2/src/client.rs` IS created in 05.3 (per parent SPEC §3 D5.2's projection that `client.rs` is 05.3-scoped explicitly, and per 05.2 SPEC §3 D1's explicit "the `client.rs` module is **not created in 05.2**").
- The H2 listener-side HCM's `BuildOutcome::Proxy` 502 stub from 05.2 D3 IS replaced with the actual upstream H2 dispatch in 05.3 D4 (per 05.2 SPEC §3 D3 test 6's projection: *"At 05.3 task time, this test is renamed to `h2_proxy_outcome_dispatches_to_upstream` and the assertion flips to a 200 from the upstream"*).
- The `Cluster.upstream_protocol: UpstreamProtocol { Http1, Http2 }` field IS added in 05.3 D3 (defaulted to `Http1` for backwards-compat; 8 fixtures unchanged in 05.3 use the default).
- `BEHAVIOR_CONTRACT.md` is NOT edited in 05.3 (per §2 above; `x-envoy-upstream-service-time` row from 04.3 covers the new H2 router-proxy responses without modification).
- `tests/differential/Cargo.toml` is NOT edited (the `h2 = "0.4"` direct dep was added in 05.2 D5.b for `drive_http2`; 05.3 reuses the 05.2-landed direct dep unchanged).
- The parent SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md` is NOT edited at 05.3's close-out commit (preserved unedited per D-3.4 / D-3.5).
- The 05.1 SPEC at `docs/envoy-rust/phases/05.1-fixture-hardening/SPEC.md` and 05.2 SPEC at `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md` are NOT edited (closed at their own state-6 commits).

---

## 5. Cross-sub-phase architectural rules inherited from parent SPEC §3

These rules are non-negotiable across the three sub-phases of parent phase 05; sub-phase 05.3 inherits them verbatim per parent-05 SPEC §3 cross-sub-phase architectural rules section. Reproduced here in brief paraphrase with parent-SPEC pointers; **most are load-bearing in 05.3 since 05.3 introduces the upstream H2 codec edge.**

1. **`envoy-http2` is the SOLE workspace dep on `h2`.** No other crate calls `h2::*` directly. (Parent-05 SPEC §3 architectural rule 1.) **Bearing on 05.3:** load-bearing. 05.3's `client.rs` lands inside `crates/envoy-http2/`; it directly imports `h2::client::*`. The new `tests/helpers/http2-echo-server/` consumes `envoy_http2` (NOT `h2` directly) per parent §6 signpost 7 — same posture 04.3's `http1-echo-server` took with `envoy_http1` over direct `httparse`. The differential harness's `drive_http2` helper consumes `h2 = "0.4"` directly per parent §6 signpost 8 — this is the **documented carve-out** landed at 05.2 D5.b, parallel to phase 04.1 REVIEW M-architectural-claim's `httparse` posture; 05.3 does NOT extend the carve-out (no other workspace crate gains a direct `h2` dep in 05.3).

2. **HCM-on-H2 reuses 04.x's `HCMConfig` and route-walk wholesale; only the codec layer at the connection edge changes.** (Parent-05 SPEC §3 architectural rule 2.) **Bearing on 05.3:** load-bearing on the **upstream-direction** codec edge. 05.3's `envoy_http2::Client` is the symmetric counterpart to 05.2's `envoy_http2::HCM` — both are codec-edge translators between `envoy_http1::codec::Request`/`Response` value types and the `h2`-shaped surface. The route-walk + `BuildOutcome` dispatch in `envoy_http1::hcm::build_response` is invoked unchanged from both 05.2's H2 listener-side dispatch path AND 05.3's H1+H2 router-arm dispatch path. The router invocation site landed in 04.3 (the `BuildOutcome::Proxy` arm dispatching through cluster_mgr/Client/write_proxied_response) is **finally fully exercised end-to-end on H2** in 05.3 (05.2 stubbed the Proxy path; 05.3 wires it).

3. **`:authority` ↔ `Host:` mapping at the H2-to-envoy-Request translation boundary.** (Parent-05 SPEC §3 architectural rule 3.) **Bearing on 05.3:** load-bearing on the **outbound** direction (the inverse of 05.2's listener-side inbound direction). 05.3's `client.rs::send_request` synthesizes `:authority` from the captured `host` (or the request's `Host:` if explicit, mirroring 04.3 H1 `Client` posture); the route-walk ran upstream-of-the-Client at the HCM's `build_response` site, so the translation here only concerns the upstream-bound request surface. Tests 3 and 4 in D1 (`send_request_writes_get_with_synthesized_pseudoheaders`, `send_request_explicit_host_header_wins_over_captured_host`) explicitly exercise this.

4. **H2-forbidden hop-by-hop headers stripped at the codec edges, not at the HCM core.** (Parent-05 SPEC §3 architectural rule 4.) **Bearing on 05.3:** load-bearing. 05.3's `client.rs::send_request` strips `connection`, `transfer-encoding`, `upgrade`, `keep-alive`, `proxy-connection` defensively before handing off to `h2::SendStream` (the request-side strip; symmetric to 05.2's response-side strip in `response.rs`). The HCM core (in `envoy_http1`) does not need to know whether it's running under H1 or H2 dispatch on the upstream side.

5. **No H2-specific edits to `envoy-config`'s `RouteConfiguration` or `HeaderMatcher` schemas.** (Parent-05 SPEC §3 architectural rule 5.) **Bearing on 05.3:** trivially satisfied. 05.3's `envoy-config` edits are confined to (a) the new `typed_extension_protocol_options` field on `Cluster` and (b) the supporting type hierarchy (`TypedExtensionProtocolOptions`, `HttpProtocolOptions`, `ExplicitHttpConfig`, `Http1ProtocolOptions`). Neither touches `RouteConfiguration` or `HeaderMatcher` (those continue to operate on the `Request` value type's normalized headers, which is protocol-agnostic).

6. **`codec_type: AUTO` continues to behave as `HTTP1`-only.** (Parent-05 SPEC §3 architectural rule 6.) **Bearing on 05.3:** trivially satisfied — 05.3 makes no `CodecType` changes; AUTO remains `HTTP1`-only per parent §4. Fixture 0010's listener uses explicit `codec_type: HTTP2` (inherited from 05.2's accept-flip).

7. **`http` crate is permitted as a transitive surface only — UNLESS ADR-0024 lands at 05.2.** (Parent-05 SPEC §3 architectural rule 7.) **Bearing on 05.3:** decisional inheritance. If ADR-0024 landed at 05.2 Task 1, `http = "1"` is already a direct dep on `crates/envoy-http2/Cargo.toml` and 05.3's `client.rs` consumes it directly. If ADR-0024 did NOT land at 05.2, `client.rs` reaches `http::*` types via `h2`'s public API only. The decision is inherited from 05.2; 05.3 does not re-litigate. **Recommendation per parent §6 signpost 21:** ADR-0024 likely landed at 05.2 Task 1 as a brief direct-dep grant.

The rules are listed for completeness; rules 1–4 and 7 are load-bearing in 05.3 (the upstream-direction codec edge attaches); rules 5–6 are trivially satisfied. The `http` crate decision (rule 7) was inherited from 05.2's Task-1 disposition.

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the 05.3 planner resolves them in-plan rather than mid-execution. Inherits parent-05 SPEC §6 signposts where they bind on 05.3, plus 05.3-local signposts.

**Inherited signposts from parent-05 SPEC §6:**

1. **Signpost 5 (`Cluster.upstream_protocol` field placement) — load-bearing at 05.3 D3.** Per parent §6 signpost 5, the field is set as a typed value at cluster-build time (in `from_bootstrap`) from the parsed config's `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config`, defaulted to `UpstreamProtocol::Http1`. **Recommendation:** the typed field, NOT a derived helper that consults the cluster's typed_extension_protocol_options lazily — avoids re-parsing config at each upstream call. D3's `from_bootstrap` projection follows this recommendation.

2. **Signpost 6 (Background `h2::client::Connection` driving) — load-bearing at 05.3 D1.** Per `h2`'s API, a `h2::client::Connection` must be polled to drive the stream multiplexing; the typical pattern is `tokio::spawn(connection)` for the lifetime of the `SendRequest` handle. **Recommendation:** `tokio::spawn` direct, matching `h2`'s docs. Wraps the connection in a fire-and-forget task that terminates when the SendRequest drops + the connection gracefully closes. Not wrapping it in a `JoinHandle` stored on the `ClientStream` for explicit shutdown — the per-ClientStream lifecycle is already short (connect → send_request → response read → drop), so explicit shutdown is unnecessary; the connection task drops cleanly with the SendRequest.

3. **Signpost 7 (Test-helper architectural posture) — load-bearing at 05.3 D5.** `http2-echo-server` consumes `envoy_http2` (NOT `h2` directly), mirroring 04.3's `http1-echo-server` consuming `envoy_http1` (NOT `httparse` directly). This keeps the architectural rule "only `envoy-http2` depends on `h2`" enforced at all dependency levels including test helpers. The helper's H2 server-side surface is reached through a thin `crates/envoy-http2/src/codec.rs` (or new `server.rs`) extension that re-exports `h2::server::Builder::handshake` adequately for the helper's needs (per D5's option (b) chosen).

4. **Signpost 10 (`x-envoy-upstream-service-time` on H2 router responses) — load-bearing at 05.3 D4.** The 04.3-landed allow-list row covers H2 too. The router H2-arm's measurement window is the same: `Instant::now()` immediately before `Client::connect`; `start.elapsed()` immediately after `send_request` returns the parsed response. The header is appended to the response by `write_proxied_response` (reused from 04.3 unchanged in 05.3 D4).

5. **Signpost 12 (`:method`/`:path`/`:authority`/`:scheme` translation) — load-bearing at 05.3 D1 in the inverse direction.** The H2 client adapter in 05.3's `client.rs` does the inverse of 05.2's `request.rs`: takes an envoy `Request` and synthesizes the pseudo-headers from `Request.method`, `Request.path`, the captured `host` (or the request's `Host:` header if explicit), and `:scheme: http` (since 05's posture is plaintext H2C only). D1's translation logic explicitly covers this.

6. **Signpost 14 (Cargo.lock sync cadence) — inline-at-scaffold per phase precedent.** Per parent §6 signpost 14, the Cargo.lock sync cadence follows the established phase-precedent. 05.3 introduces no new top-level Cargo deps (`http2-echo-server` consumes existing `envoy-http2` + `tokio` + `anyhow` + `bytes` + `thiserror` + `tracing` + `tracing-subscriber` — all already in workspace; `crates/envoy-http2/src/client.rs` adds a new internal module to an existing crate, no new deps; D2's `envoy-config` schema additions are pure type additions, no new deps). Cargo.lock sync at scaffold time (Task 1) is expected to be a no-op or near-no-op (just feature-resolution differences if `http2-echo-server`'s feature set differs from existing helpers; the planner cross-checks at state-4). The state-4 phase-done verification commit may include a single-line Cargo.lock diff or none at all.

7. **Signpost 16 (PLAN.md cadence) — standalone pre-Task-1 commit.** Per parent §6 signpost 16, each sub-phase's planner commits PLAN.md cleanly at state-2 close-out, before any Task 1 commit. Precedent: phase-04.3's `c02eea7`. The 05.1 PLAN.md and 05.2 PLAN.md follow this same shape; 05.3's PLAN.md does too. **The 05.3 PLAN.md is committed standalone, not folded into the Task 1 commit.**

8. **Signpost 17 (Fixture 0010 declares `STRICT_DNS`) — load-bearing at 05.3 D7.** Per parent §6 signpost 17, fixture 0010's cluster declares `type: STRICT_DNS` per 05.1's schema growth. D7's `envoy.yaml` projection follows this signpost. Without `STRICT_DNS`, fixture 0010 would re-introduce the C-1 regression (Envoy v1.33 rejects `host.docker.internal` under `type: STATIC`).

9. **Signpost 18 (In-process integration backstops) — load-bearing at 05.3 D7.** 05.3's fixture 0010 gains an in-process backstop at `crates/envoy-bin/tests/http2_router_upstream.rs` per the 04.3 D14 / 04.1 D4 / 05.2 D4 posture. The backstop spawns envoy-bin via `CARGO_BIN_EXE_envoy-bin`, drives the request via `h2::client`, asserts on the parsed response. The Docker-gated test at `tests/differential/tests/http2_router_upstream.rs` is CI-only.

10. **Signpost 19 (`anyhow` boundary) — load-bearing at 05.3 D5 + D7.** Tests in `crates/envoy-bin/tests/*` are in the binary crate's package and may use `anyhow` per D-3.2. The `tests/differential/` crate continues `anyhow::Result<()>` returns. The `tests/helpers/http2-echo-server/` package is a test helper (binary crate) and uses `anyhow` per the precedent set by 02.1's `tcp-echo-server`, 03.2's `tls-echo-server`, and 04.3's `http1-echo-server`.

11. **Signpost 20 (Phase-04 fixture YAMLs precedent) — load-bearing at 05.3 D7.** 04.x + 05.2 fixtures use `static_resources.listeners[0].filter_chains[0].filters[0]` of name `envoy.filters.network.http_connection_manager` with the HCM's `typed_config` carrying the route_config inline. 05.3 fixture 0010 inherits this exactly, only adding `typed_extension_protocol_options` to the cluster.

12. **Signpost 21 (ADR ledger projection) — NO new ADRs projected at 05.3 state-2.** Per parent §6 signpost 21 + §7, ADR-0022 landed at parent-05 state-2; ADR-0023 lands at 05.1 Task 1; conditional ADR-0024 (`http` crate scoping) and ADR-0025 (`h2spec` integration posture) at 05.2 Task 1. **05.3 lands NO ADRs at state-2 unless an unforeseen design ambiguity surfaces during execution per D-3.5.** The cluster-side `Http2ProtocolOptions` schema (D2.a), the `Cluster.upstream_protocol` field (D3), the router H2-arm dispatch (D4), the helper crate (D5), and fixture 0010 (D7) are all mechanically scoped per the parent-05 brainstorm; no Y/N decision points are projected at execution time. If a Y/N decision surfaces during execution that isn't covered by ADR-0022/0023/0024/0025, the planner appends the next-sequential ADR (ADR-0026 if 0024+0025 both landed at 05.2; ADR-0024/0025 if either of those numbers stays available) at the time it lands.

13. **Signpost 23 (`http1-echo-server` and `http2-echo-server` interop) — informational for 05.3.** Per parent §6 signpost 23, both helpers exist in-tree at 05 close. Phase 05 fixtures only use `http2-echo-server` (0010); 04.3 fixtures use `http1-echo-server` (0008). Whichever phase first ships a cross-protocol fixture (e.g., H2 downstream → H1 upstream cluster as a dedicated test) would mix the two. Out of scope for 05.3 — fixture 0010 is H2-on-H2 only.

**05.3-local signposts:**

14. **The 04.3 `Client::connect` / `send_request` envoy-rust posture is mirrored verbatim.** 05.3 D1's `envoy_http2::Client` public surface — `Client::connect(addr, host) -> Result<ClientStream, Http2Error>` and `ClientStream::send_request(Request) -> Result<Response, Http2Error>` — mirrors 04.3's `envoy_http1::Client` exactly (verifiable at task-1 time by `grep -nE 'pub (async )?fn (connect|send_request)' crates/envoy-http1/src/client.rs`). The mirroring is intentional — the router H2-arm at D4 dispatches polymorphically over the two surfaces, so name + signature parity reduces dispatch-site cognitive load. If 04.3's actual signatures differ from this projection, the planner mirrors what 04.3 actually has, not this projection.

15. **Defense-in-depth on the `from_bootstrap` extension.** The `match cluster_def.typed_extension_protocol_options { ... }` projection in D3 has 4 logical cases: (a) None (default Http1), (b) Some with explicit_http2_protocol_options Some (Http2), (c) Some with explicit_http_protocol_options Some (Http1), (d) Some with both Some (rejected at parse time per D2.a). Case (d) should be unreachable at runtime; the projection's `_ => UpstreamProtocol::Http1` defense-in-depth handles it gracefully if the validator misses (it shouldn't). Tests cover cases (a), (b), (c) explicitly; case (d) is unit-tested at D2.a's validator level.

16. **`envoy-cluster` adds a public dep on `envoy-config`'s new types.** Specifically, `envoy-cluster::from_bootstrap` consumes the `Cluster.typed_extension_protocol_options` field which carries `envoy_config::TypedExtensionProtocolOptions` + nested types. `envoy-cluster`'s `Cargo.toml` already path-deps `envoy-config` (per the 02.1-era introduction); no new dep entry needed; just consumption of new public types. Verify at 05.3 Task 1.

17. **Existing `from_bootstrap` 05.1 `STRICT_DNS` resolution branch is unaffected.** 05.1 D2 added a `match cluster_type { Static => ..., StrictDns => lookup_host(...) }` branch in `from_bootstrap`; 05.3 D3 adds an `upstream_protocol = match typed_extension_protocol_options { ... }` projection that runs **alongside** the cluster_type match (the two are orthogonal — `cluster_type` controls endpoint resolution shape, `upstream_protocol` controls upstream protocol dispatch). The two branches don't interact; both are computed per-cluster in `from_bootstrap`'s loop body and stored on the resulting `Cluster` struct.

18. **Helper-binary discovery posture mirrors 04.3.** `Http2EchoBackend::spawn` looks for the `http2-echo-server` binary at the workspace's `target/<profile>/http2-echo-server` path (the `<profile>` is `debug` or `release` based on the test runner's build mode); falls back to `eprintln!`-skip if not found, mirroring 04.3's `Http1EchoBackend::spawn` posture. Per 02.2 REVIEW M1's awareness-only carryforward, the SIGKILL-on-Drop polling loop blocks on `std::thread::sleep` from a tokio-runtime thread; 05.3 inherits this posture verbatim and continues the M1 carryforward (no closure attempted).

19. **Defense on the H2 client's connection-task termination.** The `tokio::spawn(connection)` in `Client::connect` returns immediately; the spawned task drives the h2 connection until its peer closes or an error occurs. On `ClientStream` drop, the `SendRequest` handle is dropped, which signals the connection task to drain in-flight streams and close gracefully (per `h2`'s docs). The connection task's error case (e.g., the peer sent malformed bytes after the response was read) is logged via `tracing::warn!` per the `Client::connect` skeleton in D1; the error does NOT propagate to the `ClientStream`'s caller (the `send_request` already returned successfully by the time the connection task encounters a post-response error).

20. **`#![forbid(unsafe_code)]` is unchanged on `crates/envoy-http2/src/lib.rs` (added in 05.2 D1) and added to `tests/helpers/http2-echo-server/src/main.rs` per D-3.8.** No `unsafe` in 05.3.

21. **No `BEHAVIOR_CONTRACT.md` edits.** Confirmed in §2 above. The 04.1+04.3 `HEADER_ALLOW_LIST` constant in `tests/differential/src/lib.rs` is unedited in 05.3.

22. **Fuzz seed file path consistency.** New seed lands at `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml`. Mirrors the existing 04.x + 05.1 + 05.2 seed shape. Allow-list entry `!corpus/parse_bootstrap/cluster_http2_protocol_options.yaml` added to `crates/envoy-config/fuzz/.gitignore`.

23. **STATE.md "Carryforwards" / "Notes" section bookkeeping.** At 05.3 state-6 phase-done commit, STATE.md's Notes section gains a "Phase-05.3 rollovers" subsection per the established cadence — enumerates 05.3 REVIEW.md verdict, in-phase closures (none anticipated), awareness-only items (M1 *EchoBackend::Drop continues; M-claim continues; 04.1 M1/M2/M4 continue tracked-forward), and the parent-05 close-out wiring. Also lands a "Phase-05 ADR ledger" final entry — confirms the actual landed ADRs at parent-05 close (ADR-0022, ADR-0023, and possibly ADR-0024/0025 from 05.2 + any unforeseen ADRs from 05.3 if any landed during execution). The 05.3-rollover section is the LAST per-phase rollover section through parent-05 close; phase-06 rollover sections begin at phase-06 close.

24. **No edits to parent-05 SPEC at the close-out commit.** Per D-3.4 / D-3.5, the parent-05 SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md` (committed at parent-05 state-1 SHA `cd1a70e`) is preserved unedited at 05.3's close-out commit. Same posture as parent-04 SPEC at `docs/envoy-rust/phases/04-http1/SPEC.md` after the 04.3 close-out (committed at SHA `805433e`, unedited through 04.3 close).

25. **Carryforwards from 05.1 + 05.2 — none active at 05.3 entry.** 05.1's REVIEW.md and 05.2's REVIEW.md verdicts (per their own SPEC §1 acceptance signal (f)) are anticipated to be Approved with M-track follow-ups at most; awareness-only items don't bind on 05.3. The cross-phase C-1 carryforward closed at 05.1's state-4. Phase-02.1 REVIEW I3 closed at 05.1's runtime test landing. Phase-04.1 REVIEW M-claim is unblocked but stays deferred; 05.3 does not extend the harness in a way that adds a third `Driver::Http1` consumer, so M-claim continues unchanged. M5/M9 (Cargo.lock cadence ratification ADR) continues to carry forward (05.3 introduces no new top-level deps so does not force the ratification call); the next phase that adds a workspace member with a new top-level dep (likely phase 06+) is the natural close site.

26. **LoC-budget reality check at PLAN-write time.** The parent-05 SPEC §3 / ADR-0022 brainstorm projected 05.3 at "~1300 LoC, ~14 tasks." This SPEC's §3 D1–D8 deliverable estimates total approximately **~2002 LoC** (~535 D1 + ~335 D2 + ~110 D3 + ~180 D4 + ~330 D5 + ~220 D6 + ~292 D7 + ~0 D8) — comparable in shape to 05.2's drift profile (~58% over the parent's projection). The drift is concentrated in D1 (the H2 client core: a thorough test surface for the codec edge inverse direction, mirroring 05.2 D3's listener-side test density) and D7 (the in-process backstop test). The PLAN-writer at 05.3 state-2 has three options: (a) accept the SPEC-write-time estimate and proceed under the §6.1 split-gate's "~1500 LoC" guardrail by leaning on the parent-05 SPEC §5 rule "do not nest-split a sub-phase that was itself produced by a split" (recommended; invoke `superpowers:systematic-debugging` first per `BOOTSTRAP_PROMPT.md` §6.1 to confirm the scope is genuine, not creep); (b) trim the test surface to come under 1500 LoC (NOT recommended — the test count was tuned to cover the H2 codec edge thoroughly; trimming risks under-coverage of the upstream-direction H2 surface); (c) systematic-debug and flag a SPEC-level scope deviation if the actual PLAN-time refinement crosses 25 tasks (the other §6.1 gate). The recommended posture is (a) — accept the estimate and PLAN against it. The PLAN-write planner records the chosen posture in PROGRESS Task 1.

27. **The H2 listener-side `Proxy` stub at `envoy-http2/src/hcm.rs` MUST be replaced.** Per 05.2 SPEC §3 D3 test 6 and §6 signpost 21, the 502 stub at the H2 listener-side HCM's `BuildOutcome::Proxy` arm MUST be replaced in 05.3 D4 with the actual upstream H1-or-H2 dispatch keyed on `cluster.upstream_protocol`. The planner verifies this at task time by reading 05.2's landed `crates/envoy-http2/src/hcm.rs` and the `h2_proxy_outcome_returns_502_in_05_2` test; the rename to `h2_proxy_outcome_dispatches_to_upstream` and the assertion-flip from 502 to 200 is mechanical.

---

## 7. ADRs expected from this sub-phase

**No ADRs are projected for 05.3 state-2.** Per parent SPEC §7, ADR-0022 lands at parent-05 state-2 (already landed alongside this SPEC); ADR-0023 lands at 05.1 Task 1; conditional ADR-0024 (`http` crate scoping) and ADR-0025 (`h2spec` integration posture) at 05.2 Task 1. **05.3's projected ADR landings are zero.**

The cluster-side `Http2ProtocolOptions` schema (D2.a) reuses the listener-side `Http2ProtocolOptions` struct and its range-check rules from 05.2 D2.b — no new doctrine call. The `Cluster.upstream_protocol` field (D3) is a typed enum following the established `LbPolicy` shape — no doctrine call. The router H2-arm (D4) dispatches polymorphically over the existing `Client::connect` / `send_request` shapes from 04.3 (H1) and 05.3 D1 (H2) — no doctrine call. The `http2-echo-server` helper (D5) follows the established `tcp-echo-server` / `tls-echo-server` / `http1-echo-server` posture verbatim — no doctrine call. Fixture 0010 (D7) follows the established 04.x + 05.x fixture shape — no doctrine call.

If an unforeseen design ambiguity surfaces during 05.3 execution per D-3.5 (decisions are written, not remembered), the planner appends the next-sequential available ADR at the time it lands. The DECISIONS.md ledger head before 05.3 Task 1 is one of:

- **ADR-0023** — if neither ADR-0024 nor ADR-0025 landed at 05.2 Task 1.
- **ADR-0024** — if only ADR-0024 landed at 05.2 Task 1.
- **ADR-0025** — if both ADR-0024 and ADR-0025 landed at 05.2 Task 1.

The 05.3 planner cross-checks the actual ledger head at Task 1 by reading the latest ADR in `docs/envoy-rust/DECISIONS.md` and adopts whatever the next-sequential available number is. Per parent §7's projection, recommendation is that ADR-0024 landed (as a brief direct-dep grant for `http`) and ADR-0025 likely did not; so the most likely ledger head at 05.3 entry is ADR-0024.

**Possible additional ADRs** (not anticipated; listed for projection completeness):

- **ADR-NEXT — H2-specific allow-list rows for BEHAVIOR_CONTRACT.md** if 05.3 surfaces response-header divergences not covered by the existing 3 phase-04 rows. **Not anticipated** — the analysis in §2 above suggests the existing rows cover the upstream H2 router-proxy responses uneventfully.
- **ADR-NEXT — `from_bootstrap` async-promotion ratification** if D3's `from_bootstrap` extension forces an additional promotion beyond what 05.1 already did (05.1 D2 already promoted `from_bootstrap` to async for `lookup_host`; 05.3 D3's `upstream_protocol` projection is a sync match, no further async needed). **Not anticipated** — the projection is sync.
- **ADR-NEXT — H2-trailers handling posture** if a planner-time decision is forced by an unexpected interaction between the `h2` codec and the helper or fixture. **Not anticipated** — trailers are an explicit non-goal per §4 and `http2-echo-server` does not emit them.

If any of these fire, they take the next-sequential available ADR number at the time they land. Sub-phase planners may also find the need for sub-phase-local ADRs; those land at the relevant sub-phase Task-N commit per D-3.5.

---

## 8. Artifacts this sub-phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/05.3-http2-upstream/PLAN.md` (lands at standalone pre-Task-1 commit per §6 signpost 7)
- `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md` (per-task progress notes)
- `docs/envoy-rust/phases/05.3-http2-upstream/REVIEW.md` (state-5 review)
- `crates/envoy-http2/src/client.rs` (D1; new module)
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml` (D2 fuzz seed)
- `tests/helpers/http2-echo-server/Cargo.toml` (D5)
- `tests/helpers/http2-echo-server/src/main.rs` (D5; with `#![forbid(unsafe_code)]`)
- `crates/envoy-bin/tests/http2_router_upstream.rs` (D7 in-process integration test)
- `tests/fixtures/0010-http2-router-upstream/envoy.yaml` (D7)
- `tests/fixtures/0010-http2-router-upstream/envoy-rust.yaml` (D7)
- `tests/fixtures/0010-http2-router-upstream/inputs/payload.bin` (D7; empty)
- `tests/fixtures/0010-http2-router-upstream/expectations.yaml` (D7)
- `tests/fixtures/0010-http2-router-upstream/README.md` (D7)
- `tests/differential/tests/http2_router_upstream.rs` (D7 Docker-gated test wrapper)

Amended during execution:

- `Cargo.toml` (root) — `[workspace] members` gains `tests/helpers/http2-echo-server`. (D5)
- `crates/envoy-http2/src/lib.rs` — append `pub mod client;` and `pub use client::{Client, ClientStream};`. (D1)
- `crates/envoy-http2/src/error.rs` — append 4 new `Http2Error` variants (`UpstreamConnect`, `H2ClientHandshake`, `H2SendRequest`, `H2RecvBody`). (D1)
- `crates/envoy-http2/src/codec.rs` (or new `server.rs` module — planner picks at Task 1) — minor `pub fn server_handshake` thin-wrapper extension to enable `http2-echo-server` to consume `envoy_http2` instead of `h2` directly per architectural rule 1. (D5)
- `crates/envoy-config/src/bootstrap.rs` — extend `Cluster` struct with `typed_extension_protocol_options: Option<TypedExtensionProtocolOptions>` field; add `TypedExtensionProtocolOptions` / `HttpProtocolOptions` / `ExplicitHttpConfig` / `Http1ProtocolOptions` types; extend validator with mutual-exclusion check + URL-mismatch check + range-check delegation; add ~8 new validator unit tests + 1 corpus-walk acceptance test. (D2)
- `crates/envoy-config/src/lib.rs` — append `ConfigError::MutuallyExclusiveExplicitHttpConfig { cluster }` variant; possibly append `ConfigError::UnsupportedTypedConfigUrl { got, expected }` if not already present from earlier phases; extend `pub use bootstrap::{...}` re-export with the new types. (D2)
- `crates/envoy-config/fuzz/.gitignore` — append `!corpus/parse_bootstrap/cluster_http2_protocol_options.yaml`. (D2)
- `crates/envoy-cluster/src/cluster.rs` — add `UpstreamProtocol { Http1, Http2 }` enum; add `Cluster.upstream_protocol: UpstreamProtocol` field; add `Cluster::upstream_protocol()` accessor + `ClusterHandle::upstream_protocol()` delegate accessor; extend `from_bootstrap` to project `upstream_protocol` from the parsed cluster's `typed_extension_protocol_options`; add ~3 new unit tests. (D3)
- `crates/envoy-http1/src/hcm.rs` — extend the `BuildOutcome::Proxy` arm at lines ~189–288 with the H1-or-H2 dispatch on `cluster.upstream_protocol()`; add ~3 new unit tests covering the dispatch arms. **Crucially:** `crate::router::write_proxied_response` is reused unchanged — 05.3 does NOT edit `router.rs`; the response wire-format on the downstream is HCM-on-downstream's concern. (D4)
- `crates/envoy-http2/src/hcm.rs` — replace 05.2's 502 stub at the `BuildOutcome::Proxy` arm with the symmetric H1-or-H2 dispatch on `cluster.upstream_protocol()`; rename 05.2's `h2_proxy_outcome_returns_502_in_05_2` test to `h2_proxy_outcome_dispatches_to_upstream` and flip the assertion from 502 to 200; add ~1 additional unit test for the H1-cluster-from-H2-listener case. (D4)
- `crates/envoy-bin/src/main.rs` — **no anticipated changes.** The H1-vs-H2 dispatch arm at the HCM construction site landed in 05.2 D4 is unchanged; the router-arm dispatch lives inside `envoy_http1::HCM` and `envoy_http2::HCM`'s connection handlers, not at envoy-bin's wiring level. The `cluster_mgr` already constructed at startup (landed in 02.1) and threaded into the HCM via the existing wiring (landed in 04.1, extended in 04.3) is consumed by D4's dispatch arms unchanged. (D4)
- `tests/differential/src/backend.rs` — add `Http2EchoBackend` struct + `spawn` + `port` + `container_host` + `Drop` impl; add `locate_http2_echo_server` helper; add ~3 harness unit tests. (D6)
- `tests/differential/src/lib.rs` — extend `run_fixture` cascade with the `{{HTTP2_BACKEND_PORT}}` template-marker substitution dispatching to spawn `Http2EchoBackend`; add ~1 harness unit test. The `Driver::Http2` variant + `drive_http2` helper from 05.2 D5 are reused unchanged; the `HEADER_ALLOW_LIST` constant is unedited. (D6)
- `docs/envoy-rust/DECISIONS.md` — **no anticipated edits.** No new ADRs projected for 05.3 state-2 per §7. If an unforeseen ADR fires during execution per D-3.5, the planner appends the next-sequential ADR at the time it lands.
- `docs/envoy-rust/ROADMAP.md`:
  - **At the state-6 phase-done commit:** flip row `05.3` `status: in-progress` → `status: done`. **AT THE SAME COMMIT:** flip parent row `05` `status: in-progress` → `status: done` per the ROADMAP-schema invariant ("the parent flips to `done` only after all sub-phases are `done`"; rows `05.1` and `05.2` are already `done` from their own state-6 commits earlier in the phase).
- `docs/envoy-rust/STATE.md`:
  - At PLAN.md commit (state-2 close-out): active phase id stays `05.3`; lifecycle state advances 2 → 3 (PLAN.md exists, implementation incomplete); next-skill advances to `superpowers:subagent-driven-development` per the user's standing preference (per auto-memory `feedback_execution_style`; matches 05.1/05.2's posture).
  - At state-6 phase-done commit: active phase id advances `05.3` → `06-<slug>` (slug consistent with `BOOTSTRAP_PROMPT.md` §8 row 06 — "Access log (file sink, Envoy default format) + stats + Prometheus admin endpoint"; expected slug `06-access-log-stats` or similar — the planner uses whatever slug phase-06 brainstorm chooses); slug advances accordingly; lifecycle state advances to phase 06 lifecycle state 1 (phase-06 directory does not exist; SPEC.md does not exist). Next-skill: `superpowers:brainstorming` scoped to phase 06.
  - Notes section gains the Phase-05.3 rollovers subsection per §6 signpost 23 (Phase-05.3 REVIEW.md verdict; in-phase closures (none anticipated); awareness-only items + cross-phase carryforwards continued: M1 *EchoBackend::Drop, M-claim, 04.1 M1/M2/M4, 04.2 M5/M8/M9/M11). Adds the parent-05 close-out summary under "Phase-05 ADR ledger (final)" — confirms ADR-0022 landed at state-2, ADR-0023 at 05.1 Task 1, conditional ADR-0024/0025 at 05.2 Task 1 (per their actual disposition), and any unforeseen ADRs landed during 05.3 execution.
- `Cargo.lock` — synced inline with the dep-introducing tasks per the established phase-precedent. 05.3 introduces no new top-level Cargo deps — Cargo.lock sync at scaffold time (Task 1 landing the new `http2-echo-server` workspace member) is expected to be a no-op or near-no-op (just feature-resolution differences if `http2-echo-server`'s feature set differs from existing helpers).
- `deny.toml` — likely no-op (no new top-level deps; no new transitive licenses). Cross-checked at state-4.

Not touched in 05.3 (belong to other phases or are frozen):

- `docs/envoy-rust/phases/05-http2/SPEC.md` (parent) — unedited; remains the design artifact committed at parent-05 state-1 SHA `cd1a70e`. **Per D-3.4 / D-3.5, parent SPECs are preserved unedited even at the close-out commit;** mirrors parent-04 SPEC's posture at the 04.3 close-out.
- `docs/envoy-rust/phases/05.1-fixture-hardening/SPEC.md` and `05.1-fixture-hardening/{PLAN,PROGRESS,REVIEW}.md` — closed at 05.1's state-6 phase-done commit; unedited in 05.3.
- `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md` and `05.2-http2-downstream/{PLAN,PROGRESS,REVIEW}.md` — closed at 05.2's state-6 phase-done commit; unedited in 05.3.
- `docs/envoy-rust/phases/{00,01,02,02.1,02.2,03,03.1,03.2,04,04.1,04.2,04.3}/*` — closed in earlier phases.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — no edits in 05.3 (per §2 above).
- `crates/envoy-tls/`, `crates/envoy-tcp/`, `crates/envoy-listener/`, `tests/helpers/{tcp,tls,http1}-echo-server/` — finalized in earlier phases; phase 05.3 consumes via existing public APIs without amendment.
- `crates/envoy-http1/src/router.rs` — unchanged in 05.3 (the `write_proxied_response` helper is reused unchanged; the H2 upstream's response is translated back into the protocol-agnostic `envoy_http1::codec::Response` value type by `envoy_http2::Client::send_request` per D1, so `write_proxied_response` doesn't need to know about H2 at all).
- `tests/differential/src/lib.rs::Driver::Http2` variant + `drive_http2` helper — unchanged in 05.3 (landed at 05.2 D5; reused unchanged for fixture 0010).
- `tests/differential/Cargo.toml` — unchanged in 05.3 (`h2 = "0.4"` direct dep was added at 05.2 D5.b for `drive_http2`; reused unchanged).
- `tests/conformance/h2spec/` — unchanged in 05.3 (the runner + the ≥95% gate + `known-failures.txt` landed at 05.2 D7; 05.3 does not edit them; the state-4 verification re-runs h2spec to confirm no regression).
- `tests/fixtures/{0001-tcp-echo,0002-static-admin-ready,0003-tcp-proxy,0004-tls-downstream,0005-tls-upstream,0006-tls-sni,0007-http1-direct-response,0008-http1-router-upstream,0009-http2-direct-response}/` — unedited; their fixtures must remain green at the 05.3 state-4 phase-done gate.
- Root `Cargo.toml`'s `[workspace] exclude` — unchanged (`crates/envoy-config/fuzz` continues to be excluded from the workspace per ADR-0009).
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.
- `crates/envoy-config/fuzz/Cargo.toml`, `crates/envoy-config/fuzz/fuzz_targets/` — unchanged. Only the corpus directory grows (1 new seed file).
- `.github/workflows/ci.yml` — unchanged in 05.3 (the h2spec binary provisioning step landed at 05.2 D7 if ADR-0025 landed; otherwise the planner picked the apt-or-curl-tar at task time — both choices are unchanged in 05.3).

---

## 9. Final commit message format (for state 6 of the 05.3 lifecycle, parent row 05 `done` commit)

The 05.3 phase-done commit ALSO closes parent phase 05 in a single commit (mirrors phase 04's `e626862`-shape close-out where the 04.3 commit closed parent 04, and phase 03's `ca81226`-shape close-out where the 03.2 commit closed parent 03). Format includes the `[parent 05 done]` tag in the title.

```
phase 05.3: HTTP/2 upstream origination + router H2-arm + fixture 0010 [parent 05 done]

envoy-http2 grows a per-connection HTTP/2 cleartext (H2C) Client (connect,
send_request; no pooling) at crates/envoy-http2/src/client.rs, sibling of
envoy_http1::Client from 04.3 D1. The new module owns the workspace's only
direct use of h2::client::*; the codec-edge translation between
envoy_http1::codec::{Request,Response} and the h2-shaped surface inverts
05.2's listener-side translation in request.rs/response.rs. Http2Error
gains 4 new client-side variants (UpstreamConnect, H2ClientHandshake,
H2SendRequest, H2RecvBody); the 05.2 codec-side variants stay unchanged.

envoy-config schema additions: cluster-side typed_extension_protocol_options
on the Cluster struct, carrying the
"envoy.extensions.upstreams.http.v3.HttpProtocolOptions" type URL and an
ExplicitHttpConfig oneof of Http1ProtocolOptions (empty in 05) or
Http2ProtocolOptions (the listener-side struct from 05.2 D2.b reused with
the same RFC 7540 range checks). Validator enforces mutual exclusion of
the two ExplicitHttpConfig arms (new ConfigError::MutuallyExclusiveExplicit
HttpConfig variant). ~8 new validator unit tests + 1 fuzz corpus seed
(cluster_http2_protocol_options.yaml).

envoy-cluster gains a typed UpstreamProtocol { Http1, Http2 } enum
(defaulted to Http1 for backwards-compat with all phase-04 clusters);
Cluster.upstream_protocol field set at cluster-build time in
from_bootstrap from the parsed typed_extension_protocol_options;
Cluster::upstream_protocol() + ClusterHandle::upstream_protocol() accessor
pair mirrors the Cluster::name() / ClusterHandle::name() pair from
04.3 D5. ~3 new envoy-cluster unit tests.

Router H2-arm extends the 04.3-landed BuildOutcome::Proxy dispatch at
crates/envoy-http1/src/hcm.rs:189-288 to dispatch H1-or-H2 by
cluster.upstream_protocol(). Reuses crate::router::write_proxied_response
unchanged (the response wire-format on the downstream is
HCM-on-downstream's concern, not the upstream-protocol's); the
x-envoy-upstream-service-time measurement window stays symmetric across
H1 and H2 upstreams (Instant::now() at connect; start.elapsed() after
send_request returns). Symmetric dispatch lands at
crates/envoy-http2/src/hcm.rs's BuildOutcome::Proxy arm, replacing 05.2's
502 stub; the 05.2 test h2_proxy_outcome_returns_502_in_05_2 is renamed
to h2_proxy_outcome_dispatches_to_upstream and the assertion flipped from
502 to 200 per 05.2 SPEC §3 D3 test 6's projection.

New helper crate tests/helpers/http2-echo-server (sibling of
tcp-echo-server / tls-echo-server / http1-echo-server) ships a
deterministic HTTP/2 cleartext echo server with alphabetically-sorted-
header response body; the determinism is load-bearing for the byte-exact
differential body equivalence. Helper consumes envoy_http2 (NOT h2
directly) per parent SPEC §6 signpost 7, mirroring 04.3's http1-echo-
server consuming envoy_http1 over direct httparse. New differential
harness Http2EchoBackend (with locate_http2_echo_server helper, SIGKILL-
on-Drop posture mirroring TcpProxyBackend / TlsEchoBackend /
Http1EchoBackend, including the awareness-only 02.2 REVIEW M1
std::thread::sleep carryforward continued unchanged) plus a run_fixture
dispatch cascade extension on the {{HTTP2_BACKEND_PORT}} template marker.

Fixture 0010-http2-router-upstream (5 files): HCM codec_type: HTTP2 +
single-VH single-route prefix: "/" route: { cluster: backend } + cluster
backend with type: STRICT_DNS (per 05.1's preamble) +
typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.
http2_protocol_options selecting H2 upstream + endpoint
{{BACKEND_HOST}}:{{HTTP2_BACKEND_PORT}}. Docker-gated test wrapper +
in-process integration backstop at crates/envoy-bin/tests/
http2_router_upstream.rs.

NO new ADRs land in 05.3 (per parent SPEC §7 / 05.3 SPEC §7). The
DECISIONS.md ledger head before 05.3 Task 1 is ADR-0024 (per the
recommended posture; 05.2 likely landed the http-direct-dep ADR and may
not have landed the conditional h2spec ADR-0025 — the actual head is
verified at 05.3 Task 1). No unforeseen ADRs surfaced during 05.3
execution.

Closes parent phase 05 (HTTP/2 cleartext data plane). Sub-phases:
- 05.1 (commit <SHA>): ClusterType::StrictDns + 5-fixture coordinated
  edit + I3 close + C-1 close [ADR-0023].
- 05.2 (commit <SHA>): envoy-http2 codec + HCM-on-H2 + fixture 0009 +
  h2spec ≥95% gate [ADR-0024 if landed, ADR-0025 if landed].
- 05.3 (this commit): envoy-http2::Client + router H2-arm + fixture 0010
  + http2-echo-server helper.

Phase-04.1 REVIEW M-claim (drive_http1 per-function unit test) stays
deferred per the 04.3 disposition; 05.3 introduces no new H1 surfaces.
Phase-02.2 REVIEW M1 (*EchoBackend::Drop polling loop blocks on
std::thread::sleep) inherited verbatim by Http2EchoBackend; M1 continues
to track forward to whichever phase first parallelizes run_fixture.
Phase-04.1 REVIEW M1/M2/M4 (diff_headers duplicate-header value
comparison; body-drain idle timeout silent Ok; strip_port IPv6-Host
incorrect rfind) continue to track forward — 05.3 fixture 0010 does not
exercise duplicate response headers, does not stall on body drain, and
uses a DNS-name Host: value so does not exercise the IPv6-Host code
path. M5/M9 (Cargo.lock cadence ratification ADR) continues to track
forward — 05.3 introduces no new top-level deps so does not force the
ratification call.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (restored by 05.1, unchanged);
  tests/fixtures/0004-tls-downstream green (restored by 05.1, unchanged);
  tests/fixtures/0005-tls-upstream green (restored by 05.1, unchanged);
  tests/fixtures/0006-tls-sni green (restored by 05.1, unchanged);
  tests/fixtures/0007-http1-direct-response green (unchanged);
  tests/fixtures/0008-http1-router-upstream green (restored by 05.1,
    unchanged in 05.3);
  tests/fixtures/0009-http2-direct-response green (HTTP/2 listener +
    direct_response action; unchanged in 05.3);
  tests/fixtures/0010-http2-router-upstream green (NEW; HTTP/2
    downstream proxied through to http2-echo-server via H2 upstream
    cluster; cluster.upstream_protocol = Http2 selected by
    typed_extension_protocol_options).
Conformance: tests/conformance/h2spec at ≥95% pass (gate landed at 05.2
  D7; unchanged in 05.3; state-4 re-run confirms no regression from the
  upstream-direction work).
```

Parent ROADMAP rows `05` and `05.3` flip to `done` in this commit (rows `05.1` and `05.2` flipped at their own state-6 commits earlier in the phase). STATE.md advances to phase `06` lifecycle state 1; next-skill `superpowers:brainstorming` scoped to phase 06 ("Access log (file sink, Envoy default format) + stats + Prometheus admin endpoint" per `BOOTSTRAP_PROMPT.md` §8 row 06). Phase 05's projected ADR ledger (ADR-0022 + ADR-0023 + conditional ADR-0024/0025 + any unforeseen ADRs from 05.3 if any landed) is closed; phase 06's projected ADRs land at the next-sequential numbers.
