# Phase 05.2 — downstream HTTP/2 cleartext (H2C prior-knowledge): `envoy-http2` foundation + HCM-on-H2 dispatch + fixture 0009 + `h2spec` ≥95% gate

- **Phase id:** `05.2`
- **Parent phase:** `05-http2` (split per **ADR-0022**; parent SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md`, committed at parent-05 state-1 SHA `cd1a70e`).
- **Slug:** `05.2-http2-downstream`
- **Title:** Land downstream HTTP/2 cleartext (H2C prior-knowledge) on the data plane: a new workspace member `crates/envoy-http2/` (sole workspace dep on `h2 = "0.4"`; mirrors `envoy-http1`'s sole-owner-of-`httparse` posture established in 04.1) + `CodecType::HTTP2` accept-flip in `envoy-config` + listener-side `Http2ProtocolOptions` schema + HCM-on-H2 dispatch (reuses 04.x's `HCMConfig` end-to-end; only the codec layer at the connection edge changes) + fixture `0009-http2-direct-response` + first conformance suite `tests/conformance/h2spec/` at the **≥95% pass** gate with catalogued failures in `known-failures.txt` + harness `Driver::Http2` + `drive_http2`.
- **Depends on:** `05.1` (sub-phase ROADMAP row `done` after 05.1's state-6 phase-done commit; the C-1 fixture-hardening preamble must complete first because 05.2's fixture 0009 dispatches through the same `run_fixture` machinery whose `cluster_mgr` build path was unblocked by 05.1's `STRICT_DNS` schema growth — see 05.1 SPEC §1). Strictly precedes `05.3` (upstream H2C client + router H2-arm + parent-05 close).
- **Differential surface when done:**
  - **Fixture restored to Docker-gated green by 05.1's C-1 fix (unchanged in 05.2):** `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, `tests/fixtures/0007-http1-direct-response/`, `tests/fixtures/0008-http1-router-upstream/`. All 8 must remain green at 05.2's state-4 phase-done gate.
  - **New fixture green:** `tests/fixtures/0009-http2-direct-response/` — H2C downstream listener with HCM `codec_type: HTTP2`, single VH `domains: ["*"]`, single route `prefix: "/"`, `direct_response { status: 200, body: { inline_string: "ok\n" } }`. No upstream cluster (so the `STRICT_DNS` posture from 05.1 is N/A for 0009; fixture 0009 carries `clusters: []`).
  - **First conformance suite attaches:** `tests/conformance/h2spec/` runs `h2spec` against an envoy-rust H2 listener, parses the test summary, asserts overall pass rate ≥95% AND every failing test is enumerated in `tests/conformance/h2spec/known-failures.txt` with a one-line doctrine reason.
- **Seeded by:** parent-05 SPEC §1 layer 2, §3 D5.2–D10.2, §4 (non-goals — the 05.2-binding subset, especially the `H2-over-TLS`/`AUTO byte-sniffing`/`HTTP/2 trailers`/`HCM server_name` deferrals), §5 (3-way split decision context — the rationale for placing the downstream H2C codec/HCM/h2spec work in its own sub-phase between the C-1 preamble and the upstream H2 client work), §6 signposts 1 (`h2 = "0.4"` line), 2 (`Http2ProtocolOptions` 4-field subset), 3 (h2spec binary management), 4 (`known-failures.txt` format), 6 (`tokio::spawn` background driving — applies marginally to the listener-side HCM in 05.2 and load-bearingly to the client in 05.3), 8 (`drive_http2` carve-out), 11 (header name lowercasing), 12 (`:method`/`:path`/`:authority`/`:scheme` translation), 13 (`PRI` preamble handling), 14 (Cargo.lock cadence — inline-at-scaffold per the established phase-precedent; M5/M9 carryforward continues unchanged), 15 (deny.toml posture), 16 (PLAN.md cadence — pre-Task-1 standalone commit per `c02eea7` precedent), 17 (fixture 0010 STRICT_DNS projection — informational; lands in 05.3 not 05.2), 18 (in-process integration backstops), 19 (`anyhow` boundary), 20 (HCM filter naming), 21 (ADR ledger projection: ADR-0024 / ADR-0025 conditional at 05.2 Task 1), 22 (HCMConfig polymorphism over codec), §7 (ADR-0024 / ADR-0025 projections), §8 (parent-05 artifact list, scoped to 05.2's slice).

This SPEC is the design contract for sub-phase 05.2. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed phase-04 + sub-phase-05.1 surface (via `git log` and the in-tree `envoy-config` / `envoy-cluster` / `envoy-http1` / `envoy-tls` / `envoy-tcp` / `envoy-listener` / `envoy-bin` / `tests/differential` / `tests/helpers/{tcp,tls,http1}-echo-server` shape at sub-phase-05.1 close) must be able to execute it without consulting the parent `05-http2/SPEC.md`. The C-1 regression trace and 05.1 fixture-hardening posture are reproduced inline below (§1) for that reason.

---

## 1. Goal and acceptance signal

**Goal.** Land downstream HTTP/2 cleartext (H2C prior-knowledge) on the data plane in five coordinated layers that all ship in this single sub-phase:

1. **New workspace member `crates/envoy-http2/`.** Sole-dep-owner of `h2 = "0.4"` (the latest stable line per parent SPEC §6 signpost 1; the `h2` crate is on D-3.2's permitted-foundations list explicitly as *"HTTP/2 codec (from the hyper project), used as a low-level codec only. Never as a server runtime. Direct analogue of Go's `golang.org/x/net/http2`"*). Cargo deps: `h2 = "0.4"`, `bytes = "1"`, `tokio = { version = "1", features = ["net", "io-util", "macros", "sync", "time"] }`, `thiserror = "2"`, `tracing = "0.1"`, `envoy-config = { path = "../envoy-config" }`, `envoy-listener = { path = "../envoy-listener" }`, `envoy-http1 = { path = "../envoy-http1" }` (consumed for `Request`/`Response` value types, the `headers::*` constants, `HCMConfig`, the route-walk and the router invocation site — the H2 wrapper is a codec-edge translator atop the existing 04.x HCM). Crate root `lib.rs` carries `#![forbid(unsafe_code)]` per D-3.8. Module decomposition (final shape per D5.2 below): `lib.rs`, `codec.rs`, `hcm.rs`, `request.rs`, `response.rs`, `error.rs`. The `client.rs` module that 05.3 lands is NOT created in 05.2; the workspace member `crates/envoy-http2/` ships in 05.2 with the listener-side surfaces only.

2. **`envoy-config` schema additions for downstream H2.** In `crates/envoy-config/src/bootstrap.rs`: (a) flip `CodecType::HTTP2` from reject to accept — at 04.1 the validator rejects `HTTP2`/`HTTP3` with `ConfigError::UnsupportedCodecType { got: CodecType }` (verifiable at 05.2 Task 1 by `grep -nE "UnsupportedCodecType|HTTP2|HTTP3" crates/envoy-config/src/bootstrap.rs`); the validator now accepts `HTTP2` and continues to reject `HTTP3` (deferred to QUIC family). `AUTO` continues to behave as `HTTP1`-only — byte-sniffing for the H2C preamble is an explicit non-goal (see §4 below). (b) Introduce the listener-side `Http2ProtocolOptions` struct as an optional field on `HttpConnectionManagerConfig`. Subset of Envoy's `envoy.config.core.v3.Http2ProtocolOptions` proto; 4 optional `u32` fields (`max_concurrent_streams`, `initial_stream_window_size`, `initial_connection_window_size`, `max_frame_size`); all default to `h2`-crate defaults if absent. Validator rejects out-of-range values per RFC 7540 (e.g., `max_frame_size` must be in `[16384, 16777215]`). Two new `ConfigError` variants: `Http2OverTlsNotSupported` (TLS+H2 with codec_type:HTTP2 explicitly rejects since 05's posture is plaintext H2C only — TLS+ALPN+H2 deferred per parent §4) + `Http2ProtocolOptionsOutOfRange { field: &'static str, value: u32, range: (u32, u32) }`.

3. **HCM-on-H2 dispatch in `crates/envoy-http2/src/hcm.rs`.** Implements `envoy_listener::ConnectionHandler` (sibling of `envoy_http1::HCM` from 04.1). Per-connection state machine: hands the raw TCP stream to `h2::server::Builder::handshake(stream)` (which expects the H2C `PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n` preamble); for each accepted stream, spawns a `tokio::task` that (a) reads the request headers from `h2::server::Connection::accept` (returns an `http::Request<h2::RecvStream>`), (b) translates the `http::Request` headers + `:path` + `:method` + `:authority` into the existing `envoy_http1::codec::Request` value type via `request::http_to_envoy_request` (the `:authority` → `Host:` mapping is mandatory for the route-walk), (c) hands the translated request to the existing 04.x route-walk + router invocation site (`envoy_http1::hcm::build_response(config, &request, close)` returns a `BuildOutcome`), (d) translates the resulting `envoy_http1::codec::Response` back into an `http::Response<()>` + body via `response::envoy_response_to_http2(response, send_stream)`, (e) closes the stream (via `END_STREAM` on the body). In 05.2 the HCM only handles the `BuildOutcome::Synth` path (direct_response); the `BuildOutcome::Proxy` path is exercised structurally (the dispatch arm must compile) but its end-to-end test path is deferred to 05.3 (where the upstream H2 client lands). The 05.2 fixture 0009 is `direct_response`-only so this is not a regression on the differential surface.

4. **`envoy-bin` HCM-on-H2 wiring.** The existing `HttpConnectionManager` arm at `crates/envoy-bin/src/main.rs` (which landed in 04.1 and was extended in 04.3 with the cluster_mgr threading) gains a second branch that selects between `envoy_http1::HCM` and `envoy_http2::HCM` based on `HCMConfig.codec_type`. The `HCMConfig` already carries `codec_type: CodecType` per 04.1's landing (verifiable at task-1 time by `grep -n 'codec_type' crates/envoy-http1/src/hcm.rs`); the dispatch is a simple `match` at the `from_config` time. The TLS-detect-and-bail logic at the existing 04.3-era line range stays unchanged (HCM-with-TLS combos remain a phase-05+ deferral per parent §4 H2-over-TLS non-goal — for plaintext-H2 listeners, the existing TLS-detect simply returns false and the codepath proceeds; for TLS-bearing listeners with `codec_type: HTTP2`, the validator rejected the config at parse time via the new `ConfigError::Http2OverTlsNotSupported`, so the runtime never sees this combination). New in-process integration test `crates/envoy-bin/tests/http2_direct_response.rs` (sibling of 04.1's `http1_direct_response.rs` and 04.3's `http1_router_upstream.rs`) spawns envoy-bin via `CARGO_BIN_EXE_envoy-bin`, drives a single H2C `GET /` via `h2::client`, asserts the parsed response.

5. **Differential harness extensions for HTTP/2 + fixture 0009 + Docker-gated test.** New `Driver::Http2 { method, path, host, expected_status, expected_body, expected_headers }` variant on the existing `Driver` enum at `tests/differential/src/lib.rs` (sibling of 04.1's `Driver::Http1` / 04.2's `Driver::Http1ProbeList`). New `drive_http2` async helper that opens a TCP connection, runs `h2::client::handshake(tcp)`, sends the constructed request, reads the response, and returns `(http::StatusCode, Vec<(String, String)>, Vec<u8>)` matching `drive_http1`'s shape so `assert_equivalence`'s `diff_headers` works without modification. Fixture `tests/fixtures/0009-http2-direct-response/` ships 5 files: `envoy.yaml` (admin block + plaintext listener bind + HCM filter chain `codec_type: HTTP2` + single-VH single-route `prefix: "/"` `direct_response 200 "ok\n"`); `envoy-rust.yaml` (per-side divergences — no admin, `127.0.0.1` bind); `inputs/payload.bin` (empty for the GET); `expectations.yaml` (driver kind `http2` with `method: GET`, `path: "/"`, `host: "envoy-rust.test"`, `expected_status: 200`, `expected_body: { byte_exact: "ok\n" }`, `expected_headers: { rule: set_equal_modulo_allow_list }`); `README.md`. Docker-gated `tests/differential/tests/http2_direct_response.rs` is a 7-line wrapper calling `differential::run_fixture("0009-http2-direct-response")`.

6. **`tests/conformance/h2spec/` runner crate at the ≥95% pass gate.** New workspace member at `tests/conformance/h2spec/` (per `BOOTSTRAP_PROMPT.md` §7.3 directory). `[[test]]` entry that (a) locates the upstream `h2spec` binary (Docker-gated CI; if `which h2spec` fails locally, the test is `eprintln!`-skipped per the established Docker-binary-locator pattern from 02.2's `TcpProxyBackend`, 03.2's `tls-echo-server`, and 04.3's `http1-echo-server`), (b) spawns envoy-bin as a subprocess against an h2spec-targeted YAML config (HCM with `codec_type: HTTP2` + a single VH with a single route returning `direct_response 200 "h2spec"`), (c) runs `h2spec -p <envoy-bin-port> --strict` (or whichever flags express "fail on any non-passing test"; the planner reads h2spec's CLI at task-1 time), (d) parses h2spec's output (h2spec emits a JSON-like or grep-friendly summary; the planner picks the form that's mechanically diffable), (e) asserts overall pass rate ≥95% AND any failing tests are listed in `tests/conformance/h2spec/known-failures.txt`. The known-failures file is maintained by-hand; failures land with a one-line doctrine reason (e.g., *"deferred to access-log family"*, *"h2 crate doesn't expose hook"*, *"intentional Envoy-divergence per ADR-NNNN"*). The gate fails if (i) overall pass rate drops below 95%, (ii) a non-listed test fails, or (iii) a listed-as-failing test starts passing without the file being trimmed in lockstep.

**Cross-phase items closed at 05.2.** None directly. 05.1's C-1 close-out is the substantive close that lands at 05.1's state-4 phase-done verification commit; 05.2's only relationship to C-1 is that 05.2's fixture 0009 inherits 05.1's restored Docker-gated baseline through the shared `run_fixture` machinery in `tests/differential/src/lib.rs`. Phase-04.1 REVIEW M-claim (drive_http1 per-function unit test) continues unchanged in 05.2 (the Docker-gated regression mask was removed at 05.1's state-4; 05.2 introduces no new H1 surfaces and does not extend the harness in a way that adds a third `Driver::Http1` consumer).

**Cross-phase items unblocked but not closed at 05.2.** None.

**Scope-shape inheritance from the parent-05 brainstorm.** The brainstorm explicitly bounded 05.2 to: codec scaffold (the new `envoy-http2` crate's listener-side surfaces only — `Client` defers to 05.3); schema growth (`CodecType::HTTP2` accept + listener-side `Http2ProtocolOptions` only — NOT cluster-side `Http2ProtocolOptions` via `typed_extension_protocol_options` which lives in 05.3, NOT the `Cluster.upstream_protocol` field which lives in 05.3); HCM dispatch (HCM-on-H2 listener-side dispatch only — NOT the router H2-arm dispatch which lives in 05.3 and dispatches into `envoy_http2::Client`); fixture 0009 (one fixture only — NOT fixture 0010 which lands in 05.3 and exercises the upstream H2C path); h2spec attach (first conformance suite — the upstream-H2 surface in 05.3 may extend the runner with additional tests but does not replace it); harness extensions (`Driver::Http2` + `drive_http2` — these primitives are reused by 05.3's fixture 0010 unchanged). This bounding is reproduced verbatim in §4 below as 05.2's non-goals.

**C-1 regression trace, reproduced inline for self-containment per D-3.4 (so that 05.2 can be executed without consulting parent SPEC).** Upstream Envoy v1.33.0 rejects the rendered `address: host.docker.internal` under `type: STATIC` with this critical-log line:

```
[critical][main] [source/server/server.cc:416] error initializing config '/etc/envoy/envoy.yaml':
malformed IP address: host.docker.internal. Consider setting resolver_name or setting cluster type
to 'STRICT_DNS' or 'LOGICAL_DNS'
```

The regression originated at phase-02.2's ADR-0015 landing (`host.docker.internal` introduced as the `BACKEND_HOST` substitution for cross-container reachability via Docker's `host-gateway`; commit `435c6fa`); was latent across phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3 (no CI push between phase-02.1 close and phase-04.3 task 14); was discovered at phase-04.3 task 14 (commit `eb6f972`); was dispositioned at the 04.3 STATE.md handoff (commit `e626862`); and was substantively closed at sub-phase 05.1's state-4 phase-done verification commit when the 5 affected Docker-gated fixtures (0003/0004/0005/0006/0008) re-greened simultaneously. Sub-phase 05.2 inherits the restored baseline; 05.2's fixture 0009 has no upstream cluster (`clusters: []`) and is unaffected by C-1 directly, but the harness's `cluster_mgr` build path that 05.2's `Driver::Http2` exercises through `run_fixture` was only unblocked by 05.1's `STRICT_DNS` schema growth. (See parent-05 SPEC §1 / 05.1 SPEC §1 for the full disposition history.)

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 05.2's feature surface:

- (a) the new differential fixture `tests/fixtures/0009-http2-direct-response/` is green at the Docker-gated CI level, with the CI run URL + the test result quoted inline in `PROGRESS.md`;
- (b) the 8 pre-existing differential fixtures `tests/fixtures/{0001-tcp-echo,0002-static-admin-ready,0003-tcp-proxy,0004-tls-downstream,0005-tls-upstream,0006-tls-sni,0007-http1-direct-response,0008-http1-router-upstream}/` remain green at the Docker-gated CI level (they are not edited in 05.2; their fixtures were green at sub-phase-05.1 close and continue green);
- (c) the conformance suite `tests/conformance/h2spec/` runs at **≥95% pass** with any failing tests explicitly catalogued in `tests/conformance/h2spec/known-failures.txt` and cross-referenced in 05.2's REVIEW §4 (each known-failure entry carries a one-line doctrine reason); the gate fails if any non-listed test regresses, OR if the overall pass rate drops below 95%, OR if a previously-listed-as-failing test starts passing without `known-failures.txt` being trimmed in lockstep;
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 05.2 with **one new seed** (`crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml`; a full bootstrap with one HCM listener of `codec_type: HTTP2` + a listener-side `Http2ProtocolOptions` block); no new fuzz target ships in 05.2;
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job. The `cargo deny check` clearance covers `h2`, `http`, and their transitive surfaces (all dual-licensed MIT/Apache-2.0 already on the allow-list per project-wide cargo-deny posture);
- (f) `REVIEW.md` for this sub-phase is approved.

The 05.2 phase-done commit flips ROADMAP row `05.2` from `in-progress` to `done`. Parent row `05` stays `in-progress` until 05.3's phase-done commit (per the ROADMAP-schema invariant "parent flips to `done` only after all sub-phases are `done`"). STATE.md advances to phase `05.3` lifecycle state 2 (05.3's SPEC was already landed at parent-05 state-2 alongside 05.1's and 05.2's SPECs in the same commit; the next session runs `superpowers:writing-plans` scoped to sub-phase 05.3).

---

## 2. Behavior-contract scope for sub-phase 05.2

**No `BEHAVIOR_CONTRACT.md` edits in 05.2.** The HCM-on-H2 surface produces no new response shapes that the existing 3 phase-04 `Header allow-list` rows (`server`, `date`, `x-envoy-upstream-service-time` from 04.3) don't already cover. The `x-envoy-upstream-service-time` row is irrelevant for fixture 0009 (no proxy path → no upstream service time emitted; the row's "Only present on responses that proxied through to an upstream cluster (NOT on `direct_response` paths)" qualifier from 04.3's allow-list landing at commit `cdd0218` excludes 0009). The other two rows (`server`, `date`) cover both H1 and H2 emission paths since they are emission-semantics rules, not framing-bound.

Equivalence-matrix engagement (per `BEHAVIOR_CONTRACT.md` §7.2):

- **Row 1 (Response status)** — fixture 0009 exercises this via the H2 `:status` pseudo-header (asserted byte-exact `200`).
- **Row 2 (Response body)** — fixture 0009 byte-exact body equivalence on `"ok\n"`.
- **Row 3 (Response headers)** — fixture 0009's response carries the existing 04.x `HEADER_ALLOW_LIST` from `tests/differential/src/lib.rs` (3 rows: `server`, `date`, `x-envoy-upstream-service-time`; the third is N/A on direct_response per the allow-list disposition).
- **Row 4 (HTTP/2 & HTTP/3 framing)** — **engaged for the first time in the project's history.** The contract row reads *"Structurally equivalent (same frame types/order on equivalent events); not byte-equal"*. 05.2's harness `drive_http2` helper drives via `h2::client` and asserts on the parsed response surface (`http::Response<h2::RecvStream>`) rather than on raw wire bytes. Frame-level equivalence is implicit (both proxies emit valid H2 framing or `h2`-the-codec rejects the connection at handshake or stream level); no fixture asserts on raw frame bytes.
- **Row 5 (TLS handshake), Row 6 (TLS cert validation), Row 8 (TCP-stream byte equivalence)** — N/A in 05.2 (fixture 0009 is plaintext H2C; no TLS).

**HTTP/1.1 hop-by-hop headers** (`Connection`, `Transfer-Encoding`, `Upgrade`, `Keep-Alive`, `Proxy-Connection`) are forbidden in H2 messages per RFC 7540 §8.1.2.2. Their absence is enforced at the codec layer: the `h2` crate rejects them at the codec layer if envoy-rust attempts to emit them, and envoy-rust's H2 response-builder strips them defensively before handing the response off to `h2::SendStream`. Their absence is therefore not asserted by the fixture (they simply never appear on the wire in H2); no allow-list change required.

**HTTP/2 pseudo-headers** (`:method`, `:path`, `:authority`, `:scheme` request-side; `:status` response-side) are not response surface in the same sense as regular headers. The H2 codec wrapper carries them in the HEADERS frame's pseudo-header block; `h2` serializes them transparently. They are asserted via the existing matrix dimensions (request-side pseudo-headers via the request shape; response-side `:status` via Row 1's status assertion).

**HTTP/2 trailers — out of scope for 05.2 (deferred non-goal).** Fixture 0009 is direct_response — direct_response responses don't carry trailers in 04.1's posture (`DirectResponse { status, body }` has no trailer field), and 05.2 does not extend this. envoy-rust's H2 codec wrapper does not parse or write trailers; the H2 response-builder writes the response body as a single body chunk via `h2::SendStream::send_data(.., end_of_stream=true)`. Trailers (HEADERS frame after END_STREAM on a DATA frame) defer to whichever phase first emits trailer-bearing responses (gRPC family will likely force this). See parent-05 SPEC §4 for the deferral.

The `HEADER_ALLOW_LIST` constant in `tests/differential/src/lib.rs` (the 04.3-landed shape with 3 rows) is unedited in 05.2. The allow-list applies to row-3 response headers; H2 pseudo-headers and forbidden hop-by-hop headers fall outside its scope.

No new `Stat-name`, `Access log field`, `xDS wire`, or `Timing tolerances` subsections are touched.

---

## 3. Deliverables

### D1 — New workspace member `crates/envoy-http2/`

New library crate at `crates/envoy-http2/`; appended to root `Cargo.toml` `[workspace] members` alongside the existing `envoy-bin`, `envoy-cluster`, `envoy-config`, `envoy-http1`, `envoy-listener`, `envoy-tcp`, `envoy-tls`, `tests/differential`, `tests/helpers/{tcp,tls,http1}-echo-server` entries. Sole-dep-owner of `h2 = "0.4"` per the cross-sub-phase architectural rule (§5 below), mirroring `envoy-http1`'s sole-owner-of-`httparse` posture from 04.1 and `envoy-tls`'s sole-owner-of-`rustls` posture from 03.1.

**`Cargo.toml`:**

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
http = "1"                                    # only if ADR-0024 lands as direct dep; otherwise transitive via h2
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

The `http = "1"` direct-dep entry is **conditional** on ADR-0024's landing (see §7 below). If the planner determines that `http` belongs as a direct dep on `envoy-http2/Cargo.toml` (because the codec-edge translation modules import `http::Request`/`http::Response`/`http::HeaderMap` symbols visibly), it lands as a direct dep with ADR-0024 documenting the narrow scoping. If the planner determines that the symbols can be reached transitively via `h2`'s public re-exports (e.g., `h2::server::Builder::handshake` returns a future that produces `http::Request<h2::RecvStream>` — but this can be type-aliased), it stays transitive-only and ADR-0024 does not land. **Recommended posture per parent §6 signpost 7 + signpost 21:** `http` lands as a brief direct-dep grant with ADR-0024 acknowledging the narrow scope (codec edge only; no other workspace crate imports `http::*`).

**Module decomposition** (final shape per parent §3 D5.2):

```
crates/envoy-http2/src/
  lib.rs        // crate root: #![forbid(unsafe_code)]; public re-exports
  codec.rs      // Http2Codec adapter over h2::server (h2::client lives in client.rs which lands in 05.3)
  hcm.rs        // HCM ConnectionHandler impl driving an H2 connection
  request.rs    // h2 RecvStream → envoy_http1::codec::Request value-type translator
  response.rs   // envoy_http1::codec::Response value-type → h2 SendStream emitter
  error.rs      // Http2Error typed-error enum
```

The `client.rs` module is **not created in 05.2** (it lands in 05.3 and ships the `Client` + `ClientStream` types for upstream H2 origination). The `hcm.rs` module's implementation in 05.2 covers the listener-side dispatch only; the router H2-arm dispatch (which would invoke `envoy_http2::Client` for upstream H2 calls) is also a 05.3 surface.

**Public surface re-exported at `lib.rs`:**

```rust
#![forbid(unsafe_code)]

//! envoy-http2 — HTTP/2 cleartext (H2C prior-knowledge) codec wrapper.
//!
//! Owns the workspace's only direct dependency on the `h2` crate. All other
//! workspace crates import `envoy_http2::*` types instead of `h2::*` types.
//! See parent-phase-05 SPEC §3 architectural rule 1 + ADR-0022.

pub mod codec;
pub mod hcm;
pub mod request;
pub mod response;
mod error;

pub use error::Http2Error;
pub use hcm::{HCM, HCMConfig};

// 05.3-projected (NOT in 05.2):
// pub mod client;
// pub use client::{Client, ClientStream};
```

`HCMConfig` in `envoy_http2::hcm` is a **type alias** for `envoy_http1::HCMConfig` (the configuration is identical across H1 and H2 dispatch — only the runtime dispatch differs by codec). The dispatch-by-codec lives at the listener-walk site in `envoy-bin/src/main.rs` per parent §6 signpost 22, not at the HCMConfig level.

**LoC estimate D1:** ~50 LoC `Cargo.toml` + workspace member registration + ~30 LoC `lib.rs` + the per-module breakdown that follows in D2/D3. Module-internal LoC totals listed under D3 (`hcm.rs`) and the small adapter modules.

### D2 — `envoy-config` schema additions for downstream H2

Two coordinated edits in `crates/envoy-config/src/bootstrap.rs`:

**D2.a — `CodecType::HTTP2` accept-flip.** At sub-phase-05.1 close the `CodecType` enum in `crates/envoy-config/src/bootstrap.rs` carries variants `Auto | Http1 | Http2 | Http3` (verifiable at task-1 time by `grep -nE "enum CodecType|^    Http" crates/envoy-config/src/bootstrap.rs`); the validator at the existing `validate_hcm` call site rejects `Http2` and `Http3` via `ConfigError::UnsupportedCodecType { got: CodecType }`. The 05.2 edit narrows the rejection to `Http3` only — `Http2` is now accepted. Validator extension on the `Http2` arm: if the listener has a TLS transport_socket attached (i.e., the listener's `filter_chains[*].transport_socket.name == "envoy.transport_sockets.tls"`), reject with `ConfigError::Http2OverTlsNotSupported` (since 05's posture is plaintext H2C only — TLS+ALPN+H2 deferred per §4 below). No new `Cluster`-side schema in 05.2 (cluster-side `Http2ProtocolOptions` lands in 05.3 D12.3).

**D2.b — `Http2ProtocolOptions` listener-side struct.** New struct in `bootstrap.rs`. Optional field on `HttpConnectionManagerConfig`. Subset of Envoy's `envoy.config.core.v3.Http2ProtocolOptions` proto:

```rust
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Http2ProtocolOptions {
    pub max_concurrent_streams: Option<u32>,            // h2 default: 100
    pub initial_stream_window_size: Option<u32>,        // h2 default: 65535
    pub initial_connection_window_size: Option<u32>,    // h2 default: 65535
    pub max_frame_size: Option<u32>,                    // h2 default: 16384
}
```

All four fields optional with defaults sourced from the `h2` crate (consumed at HCM construction time, not at parse time). Validator rejects out-of-range values per RFC 7540: `max_frame_size` must be in `[16384, 16777215]` (RFC 7540 §6.5.2 SETTINGS_MAX_FRAME_SIZE); `initial_stream_window_size` and `initial_connection_window_size` must be in `[0, 2^31 - 1]` (RFC 7540 §6.9.1 / §6.9.2 — the wire format encodes a 31-bit unsigned integer); `max_concurrent_streams` has no upper bound per RFC (zero is valid, indicating no concurrent streams allowed). Validator emits `ConfigError::Http2ProtocolOptionsOutOfRange { field: &'static str, value: u32, range: (u32, u32) }` on out-of-range.

`HttpConnectionManagerConfig` gains:

```rust
#[serde(default)]
pub http2_protocol_options: Option<Http2ProtocolOptions>,
```

(Optional; validator runs the range check only if `Some`. Absent field is normal — most fixtures don't tune H2 settings.)

**Validator unit tests appended** to `crates/envoy-config/src/bootstrap.rs::tests` (~10 tests, projected breakdown):

1. `parses_hcm_with_codec_type_http2` — full bootstrap with HCM `codec_type: HTTP2` + no TLS listener; validator accepts.
2. `rejects_hcm_with_codec_type_http2_on_tls_listener` — HCM `codec_type: HTTP2` on a listener with a TLS transport_socket; validator returns `Http2OverTlsNotSupported`.
3. `still_rejects_hcm_with_codec_type_http3` — HTTP3 continues to reject with `UnsupportedCodecType`.
4. `parses_hcm_http2_protocol_options_default` — HCM with `codec_type: HTTP2` and no `http2_protocol_options` block; validator accepts; the parsed struct's `http2_protocol_options: None`.
5. `parses_hcm_http2_protocol_options_all_fields` — HCM with `codec_type: HTTP2` and `http2_protocol_options: { max_concurrent_streams: 50, initial_stream_window_size: 131072, initial_connection_window_size: 262144, max_frame_size: 32768 }`; validator accepts; struct round-trips.
6. `rejects_http2_protocol_options_max_frame_size_too_small` — `max_frame_size: 1024` (below 16384); validator returns `Http2ProtocolOptionsOutOfRange { field: "max_frame_size", value: 1024, range: (16384, 16777215) }`.
7. `rejects_http2_protocol_options_max_frame_size_too_large` — `max_frame_size: 17000000` (above 16777215); validator returns `Http2ProtocolOptionsOutOfRange { field: "max_frame_size", value: 17000000, range: (16384, 16777215) }`.
8. `rejects_http2_protocol_options_initial_stream_window_size_too_large` — `initial_stream_window_size: 0x80000000` (above 2^31 - 1); validator returns `Http2ProtocolOptionsOutOfRange`.
9. `rejects_http2_protocol_options_initial_connection_window_size_too_large` — `initial_connection_window_size: 0x80000000`; validator returns `Http2ProtocolOptionsOutOfRange`.
10. `rejects_http2_protocol_options_unknown_field` — `http2_protocol_options: { hpack_table_size: 4096 }` (a real Envoy field 05.2 doesn't ship); serde `deny_unknown_fields` rejects with the standard "unknown field" error.

Plus 1 corpus-walk acceptance test mirroring 04.2's pattern: `fuzz_corpus_hcm_codec_http2_seed_parses` reads the new `hcm_codec_http2.yaml` seed via `include_str!` and confirms it parses cleanly.

**ConfigError extension in `crates/envoy-config/src/lib.rs`:** add two new variants:

```rust
#[error("HTTP/2 over TLS is not supported in phase 05; the listener must be plaintext or use codec_type: HTTP1/AUTO")]
Http2OverTlsNotSupported,

#[error("Http2ProtocolOptions field {field} value {value} out of range; must be in [{}, {}]", .range.0, .range.1)]
Http2ProtocolOptionsOutOfRange {
    field: &'static str,
    value: u32,
    range: (u32, u32),
},
```

Re-exports in `crates/envoy-config/src/lib.rs`'s `pub use bootstrap::{...}` block extend with `Http2ProtocolOptions`.

**Fuzz corpus extension.** `crates/envoy-config/fuzz/corpus/parse_bootstrap/` gains 1 new seed:

- `hcm_codec_http2.yaml` — full bootstrap with one HCM listener of `codec_type: HTTP2` + listener-side `http2_protocol_options { max_concurrent_streams: 100, initial_stream_window_size: 65535, initial_connection_window_size: 65535, max_frame_size: 16384 }` + single VH `domains: ["*"]` + single route `prefix: "/"` `direct_response { status: 200, body: { inline_string: "fuzz\n" } }` + `clusters: []`. Mirrors the existing 04.x seed shape. The seed exercises the validator's accept-path on `CodecType::HTTP2` and the `Http2ProtocolOptions` struct; the fuzzer never runs the H2 codec (`parse_bootstrap` only exercises serde + the validator, not the runtime).

**LoC estimate D2:** ~150 LoC schema delta (`Http2ProtocolOptions` struct + the `HttpConnectionManagerConfig` field + the validator extensions + the 2 new ConfigError variants) + ~80 LoC validator path + ~130 LoC unit tests (10 new + 1 corpus-walk × ~12 LoC each) + ~20 LoC fuzz seed YAML. Total D2: **~380 LoC**.

### D3 — HCM-on-H2 dispatch in `crates/envoy-http2/src/hcm.rs`

The core 05.2 runtime deliverable. Implements `envoy_listener::ConnectionHandler` (sibling of `envoy_http1::HCM` from 04.1) for an H2C listener. Per-connection state machine:

1. Accepts a `tokio::net::TcpStream` from the listener's accept loop.
2. Calls `h2::server::Builder::new()` with the configured `Http2ProtocolOptions` (if any) — `max_concurrent_streams` → `Builder::max_concurrent_streams`, `initial_stream_window_size` → `Builder::initial_window_size`, `initial_connection_window_size` → `Builder::initial_connection_window_size`, `max_frame_size` → `Builder::max_frame_size`.
3. Calls `Builder::handshake(tcp_stream).await`, which expects the H2C `PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n` preamble. On handshake failure (no preamble; bad SETTINGS frame; etc.), returns a typed `Http2Error::H2Handshake { source: h2::Error }` and the connection closes (envoy-rust does NOT byte-sniff to discriminate; it trusts the listener's configured `codec_type` — see parent §6 signpost 13).
4. Loops over `h2::server::Connection::accept().await`, which returns `Option<Result<(http::Request<h2::RecvStream>, h2::server::SendResponse<bytes::Bytes>), h2::Error>>` per stream; `None` indicates the connection is gracefully closed by peer.
5. For each accepted stream, spawns a `tokio::task` (per parent §6 signpost 6 the planner's recommended posture is `tokio::spawn` direct, matching `h2`'s docs) that:
   - Reads the request body bytes from the `h2::RecvStream` into `bytes::Bytes` via the `h2::RecvStream::data()` async iteration pattern (drains until END_STREAM; per §6 signpost 9 the body-bytes drain budget is unbounded in 05.2 — fixture 0009 has no request body so this is a no-op for the only fixture exercising this path; future fixtures with non-trivial bodies may want a cap, deferred per §4).
   - Translates the `http::Request<h2::RecvStream>` headers + `:path` + `:method` + `:authority` into the existing `envoy_http1::codec::Request` value type via `request::http_to_envoy_request` (the adapter in `request.rs`):
     - `:method` → `Request.method` (parsed via `envoy_http1::codec::Method::parse_token`).
     - `:path` → `Request.path` (raw string).
     - `:authority` → synthesized as a `Host: <authority>` header row at the bottom of the `Request.headers` Vec (per parent §6 signpost 12 / cross-sub-phase architectural rule 3).
     - regular headers → lowercased (the `h2` crate already delivers them lowercase per parent §6 signpost 11) + appended to `Request.headers`.
     - body bytes → `Request.body: Bytes`.
   - Hands the translated request to the existing 04.x route-walk + router invocation site: `envoy_http1::hcm::build_response(config, &request, close: false)` returns a `BuildOutcome` per 04.3's design (variants: `Synth(Response)`, `Proxy { ... }`, `Reject(Response)`).
   - For `BuildOutcome::Synth(response)`: translates the resulting `envoy_http1::codec::Response` back into an `http::Response<()>` + body via `response::envoy_response_to_http2(response, send_response)` (the adapter in `response.rs`):
     - `Response.status` → `http::Response::builder().status(...)`.
     - `Response.headers` → response header map; H2-forbidden hop-by-hop names (`connection`, `transfer-encoding`, `upgrade`, `keep-alive`, `proxy-connection`) stripped defensively per parent §3 cross-sub-phase architectural rule 4.
     - `Response.body` → `h2::SendStream::send_data(body, end_of_stream: true)`.
   - For `BuildOutcome::Proxy { ... }`: in 05.2 this path is structurally exercised (it must compile) but unreachable from fixture 0009 (which is direct_response only). The actual dispatch into `envoy_http2::Client` lands in 05.3 D13.3 (router H2-arm). In 05.2 the planner adds a `tracing::warn!` log line + responds with a 502 Bad Gateway on this path (defense-in-depth; no fixture exercises it but the codepath must compile and be deterministic).
   - For `BuildOutcome::Reject(response)`: same translation as Synth.
   - Closes the stream by `send_data(.., end_of_stream=true)`'s flag (or by dropping the SendResponse handle).
6. Returns from the connection-driver task when `accept()` returns `None` or `Some(Err(_))`.

**`HCM` struct + `ConnectionHandler` impl** (sibling of `envoy_http1::HCM`):

```rust
// crates/envoy-http2/src/hcm.rs

pub use envoy_http1::HCMConfig;  // re-exported for ergonomic naming

pub struct HCM {
    config: std::sync::Arc<HCMConfig>,
}

impl HCM {
    pub fn new(config: std::sync::Arc<HCMConfig>) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl envoy_listener::ConnectionHandler for HCM {
    async fn handle(&self, stream: tokio::net::TcpStream) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // ... handshake + stream-loop body per the state machine above ...
    }
}
```

If `envoy_listener::ConnectionHandler` is not currently `async_trait::async_trait`-shaped at sub-phase-05.1 close (verifiable at task-1 time — phase-04.1's HCM impl shape may differ; if the trait uses a different async pattern such as `Future`-returning fns, the planner mirrors that pattern), the planner mirrors the established trait shape rather than introducing `async_trait` ad-hoc. **Recommendation:** the trait is whatever 04.1 lands; the planner mirrors verbatim.

**Defensive hop-by-hop header strip** in `response::envoy_response_to_http2`. Hop-by-hop headers per RFC 7540 §8.1.2.2:

```rust
const H2_FORBIDDEN_HOP_BY_HOP: &[&str] = &[
    "connection",
    "transfer-encoding",
    "upgrade",
    "keep-alive",
    "proxy-connection",
];

// in envoy_response_to_http2:
for (name, value) in response.headers.iter() {
    if H2_FORBIDDEN_HOP_BY_HOP.contains(&name.as_str()) {
        continue;
    }
    builder = builder.header(name, value);
}
```

The strip is defensive — `direct_response` responses don't typically carry these headers (envoy-rust's `DirectResponse` action emits `server`, `date`, `content-length`, `content-type` per 04.1's shape), but the strip ensures correctness if a future code path emits them.

**Tests in `crates/envoy-http2/src/hcm.rs::tests` + `request.rs::tests` + `response.rs::tests`** (~12 tests projected):

In `hcm.rs::tests`:
1. `h2_handshake_completes_against_in_process_listener` — spawns an envoy-rust H2 HCM via a shared TCP listener; opens a TCP connection from `h2::client::handshake`; asserts the handshake succeeds (no `H2Handshake` error returned).
2. `h2_get_resolves_to_direct_response_synth` — handshake + send a `GET /` H2 request; receive the response; assert status 200 + body `"ok\n"` + `server` header present.
3. `h2_authority_header_synthesizes_host_for_route_walk` — handshake + send `GET / :authority=test.example`; assert the route-walk used `Host: test.example` (verified by configuring an HCM with two virtual hosts, one matching `test.example` and one catch-all, asserting the matching VH was selected).
4. `h2_two_requests_share_one_tcp_connection` — handshake + send two `GET /` requests on the same connection (different stream IDs); assert both succeed; assert the connection wasn't closed between requests.
5. `h2_response_strips_hop_by_hop_headers_defensively` — an HCM whose response carries `connection: close` and `keep-alive: timeout=5`; assert the wire response (from h2 client side) does NOT carry these headers.
6. `h2_proxy_outcome_returns_502_in_05_2` — an HCM config with a route action of `route: { cluster: backend }` (`BuildOutcome::Proxy` path); assert the H2 response is 502 Bad Gateway with a non-empty body (the dispatch to envoy_http2::Client doesn't exist in 05.2; the response signals "not yet implemented" deterministically). Will be replaced in 05.3 D13.3 with the actual upstream H2 dispatch — at 05.3 task time, this test is renamed to `h2_proxy_outcome_dispatches_to_upstream` and the assertion flips to a 200 from the upstream.
7. `h2_handshake_fails_on_garbage_preamble` — opens a TCP connection, sends `b"GET / HTTP/1.1\r\n\r\n"` (an HTTP/1.1 request to a HTTP/2 listener); assert the connection is closed with a typed `H2Handshake` error from the HCM driver.
8. `h2_protocol_options_max_concurrent_streams_applied` — HCM with `Http2ProtocolOptions { max_concurrent_streams: Some(1), .. }`; from h2 client open 2 streams concurrently; assert the second stream is refused at the SETTINGS frame level.

In `request.rs::tests`:
9. `http_to_envoy_request_lowercases_headers` — input `http::Request` with `User-Agent: testharness`; output `envoy_http1::codec::Request` with `user-agent: testharness`.
10. `http_to_envoy_request_synthesizes_host_from_authority` — input `http::Request` with `:authority` set but no `Host` header; output `envoy_http1::codec::Request` with a synthesized `Host: <authority>` header at the bottom.

In `response.rs::tests`:
11. `envoy_response_to_http2_strips_h2_forbidden_headers` — input `envoy_http1::codec::Response` with `connection`, `transfer-encoding`, `upgrade`, `keep-alive`, `proxy-connection` headers; output `http::Response<()>` carries none of them.
12. `envoy_response_to_http2_preserves_status_and_body` — input status 418 + body `"teapot"`; output H2 response with `:status: 418` + body bytes `b"teapot"`.

**LoC estimate D3:** ~250 LoC `hcm.rs` impl (handshake + stream loop + per-stream task + dispatch on `BuildOutcome` + the 502 stub for Proxy) + ~80 LoC `request.rs` (the translator + edge cases for missing `:authority`) + ~80 LoC `response.rs` (the translator + the hop-by-hop strip) + ~50 LoC `error.rs` (the `Http2Error` enum with variants `H2Handshake { source: h2::Error }`, `H2StreamAccept { source: h2::Error }`, `H2BodyRead { source: h2::Error }`, `MissingAuthority`, `MalformedH2HeaderBlock`, `BadStatusCode { status: u16 }`) + ~250 LoC unit tests + ~80 LoC `codec.rs` (a thin adapter exposing the `h2::server::Builder` configuration; mostly delegates). Total D3: **~790 LoC**, ~60% of the 05.2 LoC budget.

### D4 — `envoy-bin` HCM-on-H2 wiring + in-process integration test

The existing `HttpConnectionManager` typed-config arm at `crates/envoy-bin/src/main.rs` (sibling of `TcpProxy` arm landed in 02.2; the HCM arm landed in 04.1; the cluster_mgr threading extended in 04.3) gains a second branch that selects between `envoy_http1::HCM` and `envoy_http2::HCM` based on `HCMConfig.codec_type`:

```rust
// 05.2 NEW — pseudocode for the planner; exact dispatch site lands at PLAN.md writeup:
match hcm_cfg.codec_type {
    CodecType::Auto | CodecType::Http1 => {
        // EXISTING 04.1 path — unchanged.
        let handler = envoy_http1::HCM::from_config(hcm_cfg, cluster_mgr.clone())?;
        // ... bind into the listener handler chain ...
    }
    CodecType::Http2 => {
        // 05.2 NEW. Same HCMConfig; different runtime dispatch.
        let handler = envoy_http2::HCM::new(std::sync::Arc::new(hcm_cfg.clone()));
        // ... bind into the listener handler chain ...
    }
    CodecType::Http3 => {
        // Validator already rejected this at parse time (UnsupportedCodecType).
        unreachable!("CodecType::Http3 rejected by validator");
    }
}
```

The exact `from_config`-vs-`new` shape of the H2 HCM constructor depends on whether the H2 HCM needs the cluster_mgr (it doesn't in 05.2 — Proxy path is stubbed; the cluster_mgr is unused). The planner cross-checks at task time. The TLS-detect-and-bail logic at the existing 04.3-era line range stays unchanged on the H1 path; for H2 listeners with TLS the validator already rejected at parse time per D2.a's `Http2OverTlsNotSupported`.

**New in-process integration test** at `crates/envoy-bin/tests/http2_direct_response.rs` (sibling of 04.1's `crates/envoy-bin/tests/http1_direct_response.rs`). Spawns envoy-bin via `CARGO_BIN_EXE_envoy-bin`, drives a single H2C `GET /` request via `h2::client`, asserts the parsed response. ~120 LoC + the spawned-process boilerplate per 04.x precedent.

**LoC estimate D4:** ~40 LoC dispatch wiring + ~120 LoC integration test. Total D4: **~160 LoC**.

### D5 — Differential harness extensions for HTTP/2

Three coordinated edits to `tests/differential/`:

**D5.a — `Driver::Http2` variant.** New variant on the existing `Driver` enum at `tests/differential/src/lib.rs` (sibling of 04.1's `Driver::Http1` and 04.2's `Driver::Http1ProbeList`). Shape mirrors `Http1`:

```rust
// tests/differential/src/lib.rs Driver enum extension:
Http2 {
    method: String,
    path: String,
    host: String,
    expected_status: u16,
    expected_body: BodyRule,
    expected_headers: HeaderRule,
}
```

The driver reuses the existing `BodyRule` and `HeaderRule` types (no new equivalence-rule shapes in 05.2 per §2 above).

**D5.b — `drive_http2` async helper.** Sibling of `drive_http1` from 04.1. Opens a TCP connection to the listener; runs `h2::client::handshake(tcp)` to negotiate H2C; sends the constructed request via `h2::client::SendRequest::send_request`; reads the response (status + headers + body bytes) via the returned `h2::client::ResponseFuture`; returns `(http::StatusCode, Vec<(String, String)>, Vec<u8>)` matching `drive_http1`'s shape so `assert_equivalence`'s `diff_headers` works without modification.

`drive_http2` consumes `h2 = "0.4"` directly per parent §6 signpost 8 (the carve-out documented as the differential harness's analogue of phase-04.1 REVIEW M-architectural-claim's `httparse` posture). `tests/differential/Cargo.toml` gains a direct `h2 = "0.4"` dep entry.

**D5.c — `run_fixture` dispatch arm on `Driver::Http2`.** The existing `run_fixture` cascade in `tests/differential/src/lib.rs` (per 04.3's shape at the post-Task-13 line range — `port_key` match + per-fixture template-marker substitution) grows a new arm dispatching `Driver::Http2` to `drive_http2`. No new template marker is introduced for fixture 0009 (which has no upstream cluster — its YAML carries only the listener bind port via the existing `{{PORT}}` substitution). Fixture 0010 (lands in 05.3) introduces the `{{HTTP2_BACKEND_PORT}}` template marker that the 05.3 harness extension wires.

**Tests appended** to `tests/differential/src/lib.rs::tests`:

1. `drive_http2_round_trip_against_in_process_listener` — spawns an envoy-bin subprocess with an HCM `codec_type: HTTP2` direct_response config; calls `drive_http2(addr, "GET", "/", "test.example", ...)`; asserts the returned tuple matches expectations.

**LoC estimate D5:** ~120 LoC harness extensions (`Driver::Http2` variant + `drive_http2` + dispatch arm + the `Cargo.toml` `h2` dep) + ~50 LoC unit test. Total D5: **~170 LoC**.

### D6 — Fixture `0009-http2-direct-response/`

5 files in `tests/fixtures/0009-http2-direct-response/`, mirroring 04.1's fixture-0007 shape:

**`envoy.yaml`:**

```yaml
node: { id: envoy-rust-phase-05.2-fixture-0009, cluster: envoy-rust-phase-05.2 }
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

**`envoy-rust.yaml`:** identical to `envoy.yaml` modulo per-side divergences:
- bind `127.0.0.1` instead of `0.0.0.0`.
- no `admin` block (envoy-rust runs without admin in this fixture).
- `generate_request_id: false` is omitted (envoy-rust does not inject `x-request-id` per 04.3 SPEC §4 non-goal — field-set divergence is intentional, mirrors 04.3 fixture 0008's pattern).

**`inputs/payload.bin`:** empty (0 bytes) — the GET has no request body. The `Driver::Http2` constructs the request from the driver fields per D5.a; the file is present for harness-shape consistency with other fixtures but unread.

**`expectations.yaml`:**

```yaml
driver:
  kind: http2
  method: GET
  path: "/"
  host: envoy-rust.test
  expected_status: 200
  expected_body:
    byte_exact: "ok\n"
  expected_headers:
    rule: set_equal_modulo_allow_list
```

**`README.md`:** ~30 lines describing the fixture surface, the H2C handshake the harness performs, the `direct_response` action, and the cross-reference to phase 05.2 SPEC §3 D6.

**Docker-gated test:** `tests/differential/tests/http2_direct_response.rs` — 7-line wrapper:

```rust
#[tokio::test]
async fn http2_direct_response() {
    differential::run_fixture("0009-http2-direct-response").await.expect("fixture green");
}
```

**LoC estimate D6:** ~80 LoC fixture YAMLs (envoy.yaml + envoy-rust.yaml) + ~30 LoC README + ~10 LoC expectations.yaml + 7 LoC Docker-gated test wrapper. Total D6: **~130 LoC**.

### D7 — `tests/conformance/h2spec/` runner crate at the ≥95% pass gate

New workspace member at `tests/conformance/h2spec/` (per `BOOTSTRAP_PROMPT.md` §7.3 directory). First conformance suite in the project's history.

**`Cargo.toml`:**

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

(No runtime deps; the runner is test-only. The `lib.rs` is empty or carries shared helpers if needed.)

**`tests/h2spec_runner.rs`** — the runner test:

1. **Locate `h2spec` binary.** Try `which h2spec` first (system PATH); if not found, try a project-internal path like `tools/h2spec` (provisioned by CI's GitHub Actions workflow via `apt-get install h2spec` or `curl | tar`); if neither found, `eprintln!`-skip the test per the established Docker-binary-locator pattern from 02.2's `TcpProxyBackend` and 03.2's `tls-echo-server` and 04.3's `http1-echo-server`. The skip is non-failing — local development without h2spec runs all other tests cleanly.
2. **Locate envoy-bin binary.** Use `CARGO_BIN_EXE_envoy-bin` per the 04.x integration-test posture.
3. **Locate the h2spec config YAML** at `tests/conformance/h2spec/h2spec.yaml`. The YAML is a minimal HCM `codec_type: HTTP2` config with one route returning `direct_response 200 "h2spec"` — h2spec primarily exercises codec-level behavior, so the response payload doesn't matter much for test results.
4. **Spawn envoy-bin** as a subprocess with the h2spec config; wait for accept-readiness on the configured listener port (poll with a 5s timeout per the 04.x backend-spawn pattern).
5. **Run `h2spec`** as `Command::new("h2spec").args(&["-p", port_string, "--strict"]).output().await` (or whichever flags h2spec exposes for "fail on any non-passing test"; the planner reads h2spec's CLI at task-1 time).
6. **Parse h2spec's output.** h2spec emits human-readable test reports plus a summary line like `Total: NNN, Passed: MM, Skipped: KK, Failed: LL`. The planner picks a parser shape at task-1 time (regex-based or a more structured form if h2spec ships JSON output); recommendation per parent §6 signpost 4 is the line-by-line passes-and-fails capture matching h2spec's terminal format.
7. **Read `tests/conformance/h2spec/known-failures.txt`.** One test ID per line; lines starting with `#` are doctrine-reason comments next to the test ID OR free-floating comments. Parse into a `BTreeSet<String>` of test IDs.
8. **Assert the gate:**
   - **Overall pass rate ≥ 95%** computed as `passed_tests / (passed_tests + failed_tests)` (skipped tests don't count toward either). Fail with a descriptive message naming the actual rate.
   - **Every failing test is in `known-failures.txt`.** Fail with a descriptive message naming each failing test that isn't on the list (these are regressions).
   - **Every test in `known-failures.txt` actually fails.** Fail with a descriptive message naming each previously-listed test that now passes (these indicate `known-failures.txt` should be trimmed in lockstep — the gate rejects stale known-failures entries to avoid silent rot).
9. **Quote the full h2spec output** into the test's stdout via `eprintln!` so PROGRESS.md / REVIEW.md can capture it inline at state-4 and state-5.

**`known-failures.txt` format** (chosen per parent §6 signpost 4 + the planner's recommendation):

```
# h2spec known-failures for envoy-rust at phase 05.2.
#
# Each line is a single h2spec test ID followed by a doctrine reason
# explaining why the failure is intentional (or which phase/family is
# expected to close it). Failures not listed here regress the gate.
#
# Format: <h2spec-test-id>  # <one-line reason>

# Example entries (final list populated at 05.2 task time after running
# h2spec against envoy-rust's H2 dispatch end-to-end):
# 5.1.1/2                   # GOAWAY handling deferred to phase-08 graceful drain
# 6.5.3/1                   # SETTINGS_INITIAL_WINDOW_SIZE override deferred per parent SPEC §4
```

The actual failing tests are **not known at SPEC writeup time** — they will be discovered when h2spec is first run end-to-end in 05.2 task time. The planner populates the list at task time with a doctrine reason per line. Failures that are **not** explainable as deferral-to-future-phase or codec-foundation-limitation are blockers and force a re-loop into REVIEW.md state 5 if discovered.

**Sub-deliverable D7-deps: h2spec binary provisioning.** CI provisions h2spec via `apt-get install h2spec` (Debian/Ubuntu) OR `curl -L https://github.com/summerwind/h2spec/releases/download/<version>/h2spec_linux_amd64.tar.gz | tar xz -C tools/` (cross-distro fallback). The planner picks at 05.2 task time per parent §6 signpost 3; recommendation is the curl-tar fallback for portability across CI host distros. Workflow file edit at `.github/workflows/ci.yml` adds the provisioning step before the `cargo test --workspace` step.

If the runner shape demands additional discoveries (e.g., h2spec's actual CLI flags differ from `-p <port> --strict`, or its output format isn't easily parseable), the planner adjusts at task time and may land **ADR-0025** to record the integration posture (per parent §7's conditional projection). If the integration is mechanical, ADR-0025 does not land and the CONDITIONAL ADR-0025 number stays available for phase-06+ ADRs.

**LoC estimate D7:** ~30 LoC `Cargo.toml` + workspace member registration + ~250 LoC `tests/h2spec_runner.rs` (binary location + envoy-bin spawn + h2spec invocation + output parse + gate assertion + known-failures parse) + ~60 LoC `tests/conformance/h2spec/h2spec.yaml` + ~30 LoC `known-failures.txt` (initial size unknown; estimate based on a typical h2spec failure surface for a from-scratch H2 implementation) + ~5 LoC CI workflow edit. Total D7: **~375 LoC**.

---

## 4. Non-goals (subset of parent SPEC §4 that bind on 05.2)

The following are out of scope for 05.2 and defer to other sub-phases or later phases. The list is a subset of parent-05 SPEC §4, scoped to items that are predictably tempting to fold into 05.2 by a planner reading only this SPEC.

**Deferred to sub-phase 05.3:**

- **Upstream H2C origination** (`envoy-http2::Client`, `crates/envoy-http2/src/client.rs`, `tests/helpers/http2-echo-server/` helper, fixture `0010-http2-router-upstream`). Parent-05 SPEC §3 D11.3–D15.3.
- **Router H2-arm dispatch.** The 05.2 HCM stubs the `BuildOutcome::Proxy` path with a 502 Bad Gateway response (D3 hcm.rs test 6); 05.3 wires it to dispatch into `envoy_http2::Client` based on the cluster's protocol options. Parent-05 SPEC §3 D13.3.
- **`Http2ProtocolOptions` cluster-side schema** (via Envoy's `typed_extension_protocol_options.HttpProtocolOptions`). 05.2 lands the listener-side `Http2ProtocolOptions` only. Parent-05 SPEC §3 D12.3.
- **`Cluster.upstream_protocol: UpstreamProtocol { Http1, Http2 }` field.** Parent-05 SPEC §3 D12.3 + signpost 5.
- **Parent ROADMAP row `05` flip to `done`.** Happens at sub-phase 05.3's state-6 phase-done commit, not 05.2's.
- **`tests/helpers/http2-echo-server/` workspace member.** Lands in 05.3 alongside fixture 0010. 05.2 has no upstream H2 surface to test against, so no echo-server is needed.

**Deferred to later phases (per parent-05 SPEC §4 — items relevant to the H2 codec / HCM surface):**

- **HTTP/2 over TLS (ALPN-negotiated H2).** Listener-side ALPN config (`common_tls_context.alpn_protocols: ["h2", "http/1.1"]`), upstream-side ALPN, and codec-dispatch-by-ALPN. The `ConfigError::Http2OverTlsNotSupported` validator variant (D2.a) is the explicit gate for this deferral — TLS+`codec_type: HTTP2` rejects at parse time. Carries the **M7 carryforward** (`TlsAcceptingHandler.inner: Arc<TcpProxy>` concrete-typed; HCM-in-TLS doesn't typecheck — phase-04.1 REVIEW M7) forward to whichever phase ships ALPN-driven dispatch.
- **`codec_type: AUTO` byte-sniffing for H2C.** AUTO continues to behave as `HTTP1`-only in 05.2. Fixture 0009 uses explicit `codec_type: HTTP2`. Defers to whichever phase first needs single-port H1/H2C multiplexing.
- **HTTP/2 over HTTP/1.1 Upgrade (`Upgrade: h2c`).** Envoy v1.33 does not support this mode on the server side; out of scope indefinitely.
- **HTTP/3 / QUIC.** Separate family per `BOOTSTRAP_PROMPT.md` §9. The validator continues to reject `CodecType::HTTP3` with `UnsupportedCodecType` per D2.a.
- **Cross-protocol H2↔H1 translation.** A downstream H2 listener proxying to an upstream H1 cluster (or vice versa). Phase 05's fixtures are protocol-symmetric; cross-protocol translation defers.
- **Connection pooling on upstream H2.** Upstream-robustness-family territory. (Irrelevant to 05.2 since the upstream H2 client itself defers to 05.3.)
- **HTTP/2 trailers** (HEADERS frame after END_STREAM on a DATA frame). The harness does not assert on trailers; the codec wrapper does not parse or write trailers; direct_response responses never carry trailers. Defers.
- **HTTP/2 server push (`PUSH_PROMISE` frames).** Removed from H3, rarely used in practice. Deferred indefinitely.
- **HCM `server_name` field.** Re-deferred from phase 04 per parent §4. The `server` allow-list row continues to accommodate the divergence.
- **Per-route `Http2ProtocolOptions` overrides.** Not in 05.2 (or any sub-phase of 05).
- **HTTP/2 stream-level flow control tuning.** The default `h2`-crate window-size posture is used in 05.2; per-stream flow-control overrides (beyond the four `Http2ProtocolOptions` fields landed in 05.2 D2.b) defer.
- **HTTP/2 connection draining / `GOAWAY` handling on graceful shutdown.** Phase-08 (graceful drain) territory. Connections close abruptly on listener shutdown in 05.2.
- **Server-Sent Events / chunked streaming responses in H2.** The H2 codec wrapper writes responses as a single body chunk via `h2::SendStream::send_data(.., end_of_stream=true)`; streaming bodies (multiple DATA frames) defer.
- **Multiple HCM listeners.** Phase 02.1's `TooManyListeners` cap unchanged in 05.2 (single listener per envoy-rust process).
- **`LOGICAL_DNS` cluster type / `dns_refresh_rate` / `dns_lookup_family`.** All deferred per 05.1 ADR-0023's narrow scoping; 05.2 inherits the deferrals unchanged (05.2 has no cluster-side schema work).

**Not deferred — confirmed in scope for 05.2** (for clarity, since these have predictable confusion points):

- `crates/envoy-http2/src/client.rs` is NOT created in 05.2. The crate ships in 05.2 with the 5 listener-side modules (`lib`, `codec`, `hcm`, `request`, `response`, `error`) only.
- `tests/fixtures/0010-http2-router-upstream/` is NOT created in 05.2 (lands in 05.3).
- `BEHAVIOR_CONTRACT.md` is NOT edited in 05.2 (per §2 above).
- The `BuildOutcome::Proxy` codepath in `envoy-http2::HCM` is structurally exercised (must compile) but stubbed with a 502 response. The full dispatch-to-upstream lands in 05.3 D13.3.
- `tests/differential/Cargo.toml` gains a direct `h2 = "0.4"` dep entry (the carve-out per §5 architectural rule 1 / parent §6 signpost 8).

---

## 5. Cross-sub-phase architectural rules inherited from parent SPEC §3

These rules are non-negotiable across the three sub-phases of parent phase 05; sub-phase 05.2 inherits them verbatim per parent-05 SPEC §3 cross-sub-phase architectural rules section. Reproduced here in brief paraphrase with parent-SPEC pointers; **most are load-bearing in 05.2 since 05.2 is the sub-phase that introduces the H2 codec.**

1. **`envoy-http2` is the SOLE workspace dep on `h2`.** No other crate calls `h2::*` directly. (Parent-05 SPEC §3 architectural rule 1.) **Bearing on 05.2:** load-bearing. 05.2 introduces `crates/envoy-http2/Cargo.toml` with `h2 = "0.4"` as a direct dep. The differential harness's `drive_http2` helper consumes `h2` directly per parent §6 signpost 8 — this is the **documented carve-out**, parallel to phase 04.1 REVIEW M-architectural-claim's `httparse` posture. No other workspace crate (envoy-config, envoy-cluster, envoy-http1, envoy-tls, envoy-tcp, envoy-listener, envoy-bin, the test helpers) imports `h2::*` directly. 05.3's `tests/helpers/http2-echo-server/` will consume `envoy_http2` (not `h2` directly) per parent §6 signpost 7.

2. **HCM-on-H2 reuses 04.x's `HCMConfig` and route-walk wholesale; only the codec layer at the connection edge changes.** (Parent-05 SPEC §3 architectural rule 2.) **Bearing on 05.2:** load-bearing. 05.2's `envoy_http2::HCM` consumes `envoy_http1::HCMConfig` directly (re-exported as `envoy_http2::HCMConfig` for ergonomic naming per D1). The route-walk + `BuildOutcome` dispatch lives in `envoy_http1::hcm::build_response` and is invoked unchanged from 05.2's H2 dispatch path. The router invocation site landed in 04.3 (the `BuildOutcome::Proxy` arm dispatching through cluster_mgr/Client/write_proxied_response) is **not exercised end-to-end in 05.2** — fixture 0009 is direct_response only, so only the `BuildOutcome::Synth` arm runs.

3. **`:authority` → `Host:` mapping at the H2-to-envoy-Request translation boundary.** (Parent-05 SPEC §3 architectural rule 3.) **Bearing on 05.2:** load-bearing. The translation adapter in 05.2's `request.rs` (`http_to_envoy_request`) populates the envoy `Request.headers` with a synthesized `Host: <authority>` row. The route-walk in `envoy_http1::hcm::build_response` is `Host:`-driven (parent-04 SPEC §3 D3.1's "first-match-wins on `VirtualHost.domains` against request `Host:` header"); without the `:authority` synthesis, route-walk would mis-dispatch. Test 3 in D3 (`h2_authority_header_synthesizes_host_for_route_walk`) explicitly exercises this.

4. **H2-forbidden hop-by-hop headers stripped at the codec edges, not at the HCM core.** (Parent-05 SPEC §3 architectural rule 4.) **Bearing on 05.2:** load-bearing. 05.2's `response.rs` strips `connection`, `transfer-encoding`, `upgrade`, `keep-alive`, `proxy-connection` defensively before handing off to `h2::SendStream`. The HCM core (in `envoy_http1`) does not need to know whether it's running under H1 or H2 dispatch. Test 5 in D3 (`h2_response_strips_hop_by_hop_headers_defensively`) and test 11 (`envoy_response_to_http2_strips_h2_forbidden_headers`) both exercise this.

5. **No H2-specific edits to `envoy-config`'s `RouteConfiguration` or `HeaderMatcher` schemas.** (Parent-05 SPEC §3 architectural rule 5.) **Bearing on 05.2:** trivially satisfied. 05.2's `envoy-config` edits are confined to (a) `CodecType::HTTP2` accept-flip and (b) the new `Http2ProtocolOptions` struct on `HttpConnectionManagerConfig`. Neither touches `RouteConfiguration` or `HeaderMatcher` (those continue to operate on the `Request` value type's normalized headers, which is protocol-agnostic).

6. **`codec_type: AUTO` continues to behave as `HTTP1`-only.** (Parent-05 SPEC §3 architectural rule 6.) **Bearing on 05.2:** load-bearing on the negative side — 05.2 must NOT extend AUTO to byte-sniff for the H2C `PRI` preamble. Fixture 0009 uses explicit `codec_type: HTTP2`. AUTO byte-sniffing defers per §4.

7. **`http` crate is permitted as a transitive surface only — UNLESS ADR-0024 lands.** (Parent-05 SPEC §3 architectural rule 7.) **Bearing on 05.2:** decisional. The `h2` crate's API exposes `http::*` types directly (`h2::server::Connection::accept` returns `(http::Request<h2::RecvStream>, h2::server::SendResponse<bytes::Bytes>)`); the question is whether `crates/envoy-http2/Cargo.toml` lists `http = "1"` as a direct dep (with ADR-0024 landing the narrow grant) or relies on `h2`'s public re-exports / type aliases. Recommended posture per parent §6 signpost 21: ADR-0024 lands as a brief direct-dep grant.

The rules are listed for completeness; rules 1–4 and 7 are load-bearing in 05.2; rules 5–6 are trivially satisfied. They become load-bearing again in 05.3 when the upstream H2 client + cluster-side schema attach.

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the 05.2 planner resolves them in-plan rather than mid-execution. Inherits parent-05 SPEC §6 signposts where they bind on 05.2, plus 05.2-local signposts.

**Inherited signposts from parent-05 SPEC §6:**

1. **Signpost 1 (`h2 = "0.4"` line) — load-bearing at 05.2 Task 1.** Per parent §6 signpost 1, `h2 = "0.4"` is the latest stable line as of phase-05 brainstorm. The planner cross-checks at 05.2 Task 1 by `cargo search h2 | head -1` — if the stable line has shifted (e.g., `h2 = "0.5"` is published), the planner records the version choice in PROGRESS Task 1 with the cross-check output. The `h2` crate is on D-3.2's permitted-foundations list (no ADR needed for the dep); the major-version choice does not require an ADR.

2. **Signpost 2 (`Http2ProtocolOptions` schema subset) — load-bearing at 05.2 D2.** Per parent §6 signpost 2, 05.2 ships only 4 fields (`max_concurrent_streams`, `initial_stream_window_size`, `initial_connection_window_size`, `max_frame_size`). Envoy's full proto has many more (`allow_connect`, `allow_metadata`, `hpack_table_size`, `override_stream_error_on_invalid_http_message`, `connection_keepalive`, etc.); they all default to RFC-conformant values and are not exercised by fixture 0009. The planner adds them only if a fixture or h2spec test forces it (none anticipated).

3. **Signpost 3 (h2spec binary management) — load-bearing at 05.2 D7.** Three options: (a) Docker image (e.g., `summerwind/h2spec`) wrapped in `Command::new("docker")`; (b) installed via system package (`apt-get install h2spec`) or `curl | tar` in the CI workflow file; (c) Cargo-built `[[bin]]` from a vendored h2spec source (likely too heavy). **Recommendation: (b) for CI** with a local `eprintln!`-skip fallback if `which h2spec` fails. The planner picks the exact provisioning at 05.2 task time based on which install path is most reliable across the GitHub Actions CI runner image.

4. **Signpost 4 (`known-failures.txt` format) — load-bearing at 05.2 D7.** Two options: (a) one test ID per line with `# reason` comment; (b) structured TOML/YAML. **Recommendation: (a)** for diff-friendliness. (a) was confirmed at SPEC writeup time. The planner can swap to (b) at Task 1 if a test ID has structural fields (e.g., a ticket reference) that warrant typed storage; recommendation is to NOT swap unless a concrete need surfaces.

5. **Signpost 6 (Background `h2::client::Connection` driving) — minimally load-bearing at 05.2 (relevant for `drive_http2`).** Parent §6 signpost 6 applies to the client-side connection-driving pattern. In 05.2, `drive_http2` (D5.b) runs in the harness — it's ephemeral, single-stream, and short-lived; the `tokio::spawn` direct posture is sufficient (no `JoinHandle` capture needed). The pattern becomes more load-bearing in 05.3's `Client::connect` where the connection drives multiple stream lifetimes; 05.3 picks at its own Task 1.

6. **Signpost 8 (`drive_http2` carve-out) — load-bearing at 05.2 D5.b.** The differential harness's `drive_http2` helper consumes `h2 = "0.4"` directly. This is a documented carve-out from cross-sub-phase architectural rule 1 (only `envoy-http2` depends on `h2`), parallel to the phase 04.1 REVIEW M-architectural-claim posture for `httparse` in the differential harness. PROGRESS.md records the carve-out at Task 5; REVIEW.md §4 carries the note as awareness-only.

7. **Signpost 11 (Header name lowercasing) — load-bearing at 05.2 D3.** H2 mandates lowercase header names on the wire (RFC 7540 §8.1.2). The `h2` crate enforces this — uppercase names cause a connection error. 05.2's `request.rs` translation adapter receives lowercase names from `h2::RecvStream` already; 05.2's `response.rs` translation adapter lowercases names defensively before handing off to `h2::SendStream`. This matches the 04.x posture (envoy-rust emits lowercase header names per parent-04 SPEC).

8. **Signpost 12 (`:method`/`:path`/`:authority`/`:scheme` translation) — load-bearing at 05.2 D3.** The H2-to-envoy-Request adapter in 05.2's `request.rs` reads pseudo-headers from the `http::Request<h2::RecvStream>` (where they're separated into typed fields by `h2`); writes `Request.method` from `:method`, `Request.path` from `:path`, synthesizes a `Host: <authority>` row, and ignores `:scheme` (envoy-rust's HCM doesn't currently dispatch on scheme — that's a TLS-vs-plaintext concern, deferred).

9. **Signpost 13 (`PRI` preamble handling) — load-bearing at 05.2 D3.** `h2::server::handshake` expects the H2C `PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n` preamble at the start of the connection. Clients sending an HTTP/1.1 request to a `codec_type: HTTP2` listener get a connection-level error from `h2`. Envoy-rust does not byte-sniff to discriminate; it trusts the listener's configured `codec_type` and lets `h2` reject malformed connections at the codec layer (test 7 in D3 — `h2_handshake_fails_on_garbage_preamble` — exercises this).

10. **Signpost 14 (Cargo.lock sync cadence) — inline-at-scaffold per phase precedent.** Per parent §6 signpost 14, the Cargo.lock sync cadence follows the established phase-precedent (phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85633e7`, phase-04.x inline). 05.2 introduces `h2 = "0.4"` + `bytes = "1"` (already in workspace) + the `http = "1"` direct dep (if ADR-0024 lands) on `crates/envoy-http2/Cargo.toml`. Cargo.lock sync at scaffold time (Task 1) is expected to land the `h2` + `http` + `slab`/`fnv`/`tokio-util` transitive surface as a non-trivial diff. The state-4 phase-done verification commit cross-checks the diff.

11. **Signpost 15 (deny.toml license allow-list) — likely no-op at 05.2.** The `h2` crate is dual-licensed MIT/Apache-2.0 (already on the allow-list). The `http` crate is dual-licensed MIT/Apache-2.0 (already on the allow-list). The transitive surface (`slab`, `fnv`, `tokio-util`) is also already covered. No `deny.toml` changes anticipated; the planner cross-checks `cargo deny check` output and lands an inline addition only if a transitive crate brings a new license.

12. **Signpost 16 (PLAN.md cadence) — standalone pre-Task-1 commit.** Per parent §6 signpost 16, each sub-phase's planner commits PLAN.md cleanly at state-2 close-out, before any Task 1 commit. Precedent: phase-04.3's `c02eea7`. The 05.1 PLAN.md follows this same shape (per 05.1 SPEC §6 signpost 2). **The 05.2 PLAN.md is committed standalone, not folded into the Task 1 commit.**

13. **Signpost 17 (Fixture 0010 STRICT_DNS projection) — informational for 05.2.** Parent §6 signpost 17 anticipates fixture 0010's cluster type as `STRICT_DNS` per 05.1's schema growth. **Bearing on 05.2:** trivially satisfied — 05.2 doesn't create fixture 0010. Fixture 0009 (which 05.2 does create) has `clusters: []` (no upstream cluster) so the STRICT_DNS posture is N/A for 0009.

14. **Signpost 18 (In-process integration backstops) — load-bearing at 05.2 D4.** 05.2's fixture 0009 gains an in-process backstop at `crates/envoy-bin/tests/http2_direct_response.rs` per the 04.3 D14 / 04.1 D4 posture. The backstop spawns envoy-bin via `CARGO_BIN_EXE_envoy-bin`, drives the request via `h2::client`, asserts on the parsed response. The Docker-gated test at `tests/differential/tests/http2_direct_response.rs` is CI-only.

15. **Signpost 19 (`anyhow` boundary) — load-bearing at 05.2 D4 + D5 + D7.** Tests in `crates/envoy-bin/tests/*` are in the binary crate's package and may use `anyhow` per D-3.2. The `tests/differential/` crate continues `anyhow::Result<()>` returns on `drive_http2` for consistency with 04.x's `drive_http1` posture. The `tests/conformance/h2spec/` runner crate is dev-deps-only and uses `anyhow` directly.

16. **Signpost 20 (Phase-04 fixture YAMLs precedent) — load-bearing at 05.2 D6.** 04.x fixtures use `static_resources.listeners[0].filter_chains[0].filters[0]` of name `envoy.filters.network.http_connection_manager` with the HCM's `typed_config` carrying the route_config inline. 05.2 fixture 0009 inherits this exactly, only changing `codec_type` from `HTTP1` to `HTTP2`.

17. **Signpost 21 (ADR ledger projection) — ADR-0024 / ADR-0025 conditional at 05.2 Task 1.** Per parent §6 signpost 21 + §7 ADR-0024 / ADR-0025 projections, **ADR-0024** (`http` crate typed-surface scoping) and **ADR-0025** (`h2spec` integration posture) are CONDITIONAL — the planner decides at 05.2 Task 1 whether either warrants landing. Recommendation per parent §7: ADR-0024 lands as a brief direct-dep grant; ADR-0025 likely does NOT land (the `h2spec` integration is mostly mechanical). The DECISIONS.md ledger head before 05.2 Task 1 is **ADR-0023** (landed at 05.1 Task 1 inline); ADR-0024 lands at the next-sequential number with no renumbering needed.

18. **Signpost 22 (HCMConfig polymorphism over codec) — load-bearing at 05.2 D4.** The existing `envoy_http1::HCMConfig` is the per-listener immutable config. 05.2's `envoy_http2::HCM` uses the same config struct (re-exported as `envoy_http2::HCMConfig` per D1's `pub use hcm::{HCM, HCMConfig};`). The dispatch-by-codec lives at the listener-walk site in `envoy-bin/src/main.rs` (D4), not at the HCMConfig level. The `HCMConfig.codec_type` field already exists from 04.1 (verifiable at task-1 time).

**05.2-local signposts:**

19. **`async_trait` posture for `envoy_listener::ConnectionHandler`.** The trait was established in 04.1 with whichever async pattern that phase chose (likely `async_trait::async_trait` macro per Rust ecosystem convention). The planner verifies at 05.2 Task 1 by reading `crates/envoy-listener/src/lib.rs` and mirrors the trait shape verbatim in `envoy_http2::HCM`'s impl. If the trait is not `async_trait`-shaped (e.g., it returns `impl Future` directly), the planner mirrors that posture instead. **Do not introduce `async_trait` ad-hoc** — match what's already in-tree.

20. **Per-stream `tokio::spawn` lifecycle.** 05.2's HCM spawns a `tokio::task` per accepted stream. The spawned task carries the cloned `Arc<HCMConfig>` and the per-stream `(http::Request, SendResponse)` pair. On task completion (response written + stream closed), the task drops cleanly; the spawned task is fire-and-forget with no `JoinHandle` retention. Errors in the per-stream task are logged via `tracing::error!` and do NOT propagate to the connection driver (per-stream errors are independent in H2; one stream failing should not tear down sibling streams). The connection-driver task stays alive until `h2::server::Connection::accept()` returns `None` or a fatal error.

21. **Defense-in-depth on the `BuildOutcome::Proxy` 05.2 stub.** The 502 response stub at the H2 HCM's Proxy arm includes a non-empty body (e.g., `b"upstream H2 not yet wired (sub-phase 05.3)"` or similar) so that diagnostic information is preserved if the test machinery accidentally exercises this path. The body should NOT include real cluster names or endpoint addresses (information leakage); a generic doctrine-line is sufficient. At 05.3 task time, the stub is replaced with the real upstream H2 dispatch.

22. **`http2_direct_response.rs` integration test cleanup.** The 04.x `crates/envoy-bin/tests/http1_direct_response.rs` integration test has a known SIGKILL-on-Drop posture for the spawned envoy-bin subprocess (per phase-02.2 REVIEW M1 carryforward). 05.2's `http2_direct_response.rs` mirrors this posture verbatim — drop the `Child` handle to terminate envoy-bin at test exit. The Drop's polling loop blocks on `std::thread::sleep` from a tokio-runtime thread (M1 awareness-only; M1 carries forward to whichever phase first parallelizes `run_fixture` per phase-02.2 REVIEW). 05.2 does not parallelize; M1 continues unchanged.

23. **`#![forbid(unsafe_code)]`** is added to `crates/envoy-http2/src/lib.rs` per D-3.8. No `unsafe` in 05.2.

24. **No `BEHAVIOR_CONTRACT.md` edits.** Confirmed in §2 above. The 04.1+04.3 `HEADER_ALLOW_LIST` constant in `tests/differential/src/lib.rs` is unedited in 05.2.

25. **Fuzz seed file path consistency.** New seed lands at `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml`. Mirrors the existing 04.x + 05.1 seed shape. Allow-list entry `!corpus/parse_bootstrap/hcm_codec_http2.yaml` added to `crates/envoy-config/fuzz/.gitignore`.

26. **05.3 SPEC is landed alongside this SPEC.** Per parent-05 state-2 lifecycle (mirrors phase-04 state-2 commit `1d9740d`), the parent-05 state-2 commit lands ADR-0022 + all three sub-phase SPECs (`05.1-fixture-hardening/SPEC.md`, this `05.2-http2-downstream/SPEC.md`, `05.3-http2-upstream/SPEC.md`). 05.2 execution starts after 05.1 closes (state-2 of the 05.1 lifecycle ran, state-3 of the 05.1 lifecycle ran, etc., culminating in 05.1's state-6 phase-done commit that flips ROADMAP row `05.1` to `done` and advances STATE.md to phase 05.2 lifecycle state 2). The 05.3 SPEC stays unedited during 05.2 execution; 05.3's PLAN.md / PROGRESS.md / REVIEW.md land in its own sub-phase execution window.

27. **Carryforwards from 05.1 — none active at 05.2 entry.** 05.1's REVIEW.md verdict (per the SPEC §1 acceptance signal (f)) is anticipated to be Approved with M-track follow-ups at most; awareness-only items don't bind on 05.2. The cross-phase C-1 carryforward closes at 05.1's state-4. Phase-02.1 REVIEW I3 closes at 05.1's runtime test landing. Phase-04.1 REVIEW M-claim is unblocked (the Docker-gated regression mask is removed) but stays deferred per the 04.3 disposition; 05.2 does not extend the harness in a way that adds a third `Driver::Http1` consumer, so M-claim continues unchanged.

28. **LoC-budget reality check at PLAN-write time.** The parent-05 SPEC §3 / ADR-0022 brainstorm projected 05.2 at "~1300 LoC, ~14 tasks." This SPEC's §3 D1–D7 deliverable estimates total **~2055 LoC** (~50 D1 + ~380 D2 + ~790 D3 + ~160 D4 + ~170 D5 + ~130 D6 + ~375 D7) — a 58% drift from the parent's projection, larger than phase-04.3's ~27% drift (1490 estimated → 1900 actual per 04.3 REVIEW §1). The drift is concentrated in D3 (the H2 HCM core: a more thorough test surface and the multi-module decomposition increases LoC vs. parent's whole-crate estimate). The PLAN-writer at 05.2 state-2 has three options: (a) accept the SPEC-write-time estimate and proceed under the §6.1 split-gate's "~1500 LoC" guardrail by leaning on the parent-05 SPEC §5 rule "do not nest-split a sub-phase that was itself produced by a split" (recommended; invoke `superpowers:systematic-debugging` first per `BOOTSTRAP_PROMPT.md` §6.1 to confirm the scope is genuine, not creep); (b) trim the test surface to come under 1500 LoC (NOT recommended — the test count was tuned to cover the H2 codec edge thoroughly; trimming risks under-coverage of the first H2 surface in the project); (c) systematic-debug and flag a SPEC-level scope deviation if the actual PLAN-time refinement crosses 25 tasks (the other §6.1 gate). The recommended posture is (a) — accept the estimate and PLAN against it. The PLAN-write planner records the chosen posture in PROGRESS Task 1.

---

## 7. ADRs expected from this sub-phase

**Up to two ADRs may land during 05.2 execution**, both at Task 1 alongside the schema variant addition (D2) + the codec scaffold (D1) + the HCM dispatch (D3). Mirrors phase 04.2 Task 1's ADR-0021 inline-landing pattern and phase 03.1 Task 1's ADR-0018 + ADR-0019 inline-landing pattern.

### ADR-0024 (CONDITIONAL) — `http` crate (`http::Request` / `http::Response` / `http::HeaderMap`) typed-surface scoping

- **Date:** 2026-MM-DD (the date 05.2 Task 1 lands; backdated to ADR landing day per the ADR-0021 / ADR-0018 / ADR-0014 precedent).
- **Status:** accepted IF landed; not landed if the planner determines the symbols are reachable transitively through `h2`'s public re-exports without forcing a direct `http` import.
- **Context:** Phase 05.2 introduces `crates/envoy-http2/`, the workspace's sole-dep-owner of `h2 = "0.4"`. The `h2` crate's API exposes `http::*` types directly: `h2::server::Connection::accept` returns `(http::Request<h2::RecvStream>, h2::server::SendResponse<bytes::Bytes>)`; `h2::client::SendRequest::send_request` accepts `http::Request<()>`; etc. The codec-edge translation modules in `envoy-http2` (`request.rs`, `response.rs`) import these symbols by name. The narrow scope question is whether `http` belongs as a direct dep on `crates/envoy-http2/Cargo.toml` (with this ADR documenting the narrow scoping, parallel to ADR-0021's narrow scoping for `regex`), or stays transitive-only through `h2`'s public API.
- **Options considered:**
  - **(i) Add `http = "1"` as a direct dep on `crates/envoy-http2/Cargo.toml`.** Direct imports `use http::{Request, Response, HeaderMap};` work cleanly; static-analysis tools see the dep explicitly. **Chosen IF the scoping warrants ADR-grade documentation.**
  - **(ii) Use `h2::http` re-exports (if available) to avoid a direct dep.** The `h2` crate may re-export the `http` types; the planner verifies at task time. If yes, then `use h2::http::{Request, Response, HeaderMap};` works without a `Cargo.toml` entry. Rejected if the re-export surface is incomplete (e.g., `http::HeaderName` not re-exported); accepted if complete.
  - **(iii) Use `http::*` transitively via `h2`'s public API only.** Treat `http::*` types as opaque types touched only at function boundaries. Rejected: the codec-edge translation requires reading individual fields (`request.method()`, `request.uri().path()`, `request.headers()`); opaque-only access blocks the implementation.
- **Decision (if ADR-0024 lands):** Add `http = "1"` as a direct dep on `crates/envoy-http2/Cargo.toml`. Narrowly scoped: only `crates/envoy-http2/` imports `http::*` symbols; no other workspace crate imports `http::*` directly (verifiable by `grep -rn 'use http::' crates/`). This is parallel to ADR-0021's narrow scoping for `regex` (where `regex` is permitted only on `crates/envoy-config/` for header / route matching at config-load time).
- **Rationale (if ADR-0024 lands):** Direct deps are easier to reason about at static-analysis time (cargo-deny, `cargo audit`, transitive-version drift detection). The `http` crate is dual-licensed MIT/Apache-2.0 (already covered by `deny.toml`) and is the de-facto Rust HTTP types crate (used by `hyper`, `reqwest`, `axum`, etc.; first-party `rust-lang` org). Treating its first use as a foundation grant is the cheapest, most honest formalization.
- **Consequences (if ADR-0024 lands):** `crates/envoy-http2/Cargo.toml`'s `[dependencies]` section gains `http = "1"`. `Cargo.lock` gains `http` as a direct surface (it's already present transitively from `h2`'s deps, so the lock-file diff is structural-only — `http` moves from a transitive dep to a direct dep). Future scope-extension ADRs (HCM internal use of `http::*` types, filter-framework `http::*` types) name this ADR explicitly.
- **Provenance:** projected as conditional in parent-05 SPEC §7. Lands at 05.2 Task 1 IF the planner determines the dep direction warrants policy-grade documentation. The DECISIONS.md ledger head before 05.2 Task 1 is ADR-0023 (landed at 05.1 Task 1 inline).

### ADR-0025 (CONDITIONAL) — `h2spec` integration posture

- **Date:** 2026-MM-DD (the date 05.2 Task 1 lands).
- **Status:** accepted IF landed; not landed if the integration is mechanical and warrants no policy-grade documentation.
- **Context:** Phase 05.2 attaches the project's first conformance suite at `tests/conformance/h2spec/`. The runner (D7) drives the upstream `h2spec` binary against an envoy-rust H2 listener, parses the test output, and asserts a ≥95% pass gate with catalogued failures in `known-failures.txt`. The `h2spec` binary is provisioned by CI (per parent §6 signpost 3); the runner's gate-mechanics involve doctrine choices (binary provisioning shape, output-parse format, known-failures format, regression-detection on previously-passing tests).
- **Options considered:**
  - **(i) Land ADR-0025 with full Options/Decision/Rationale/Consequences scaffolding.** Records the binary provisioning + output parsing + known-failures format choices for forward auditability. **Chosen IF** the gate-mechanics surface non-trivial doctrine choices (e.g., a non-obvious tradeoff between Docker-image vs. apt-package provisioning, or a non-obvious output parsing strategy).
  - **(ii) Skip ADR-0025; document the choices inline in PROGRESS Task N.** Rejected IF the choices are mechanically deterministic (e.g., h2spec's CLI is well-documented and the planner's choice is forced by the CLI).
- **Decision (if ADR-0025 lands):** documented per the planner's task-time choices. Likely covers: (a) binary provisioning (apt-get vs. curl-tar — recommendation curl-tar for cross-distro CI portability); (b) output parsing (regex over h2spec's stdout vs. a structured form); (c) known-failures format (one-line-per-test-id with `# reason` per parent §6 signpost 4); (d) regression-detection on previously-passing tests (the gate fails if a known-failures entry passes without `known-failures.txt` being trimmed in lockstep).
- **Rationale (if ADR-0025 lands):** First conformance suite in the project's history; the gate-mechanics doctrine is foundational for future conformance attaches (`h3spec` in QUIC family, gRPC interop in gRPC family, etc.). Recording the doctrine choices upfront avoids re-litigation in those future phases.
- **Consequences (if ADR-0025 lands):** `tests/conformance/h2spec/` ships per the documented posture. Future conformance suites (`h3spec`, `interop-tests`) reference ADR-0025 for the gate-mechanics shape and adapt only the binary-specific provisioning details.
- **Provenance:** projected as conditional in parent-05 SPEC §7. Lands at 05.2 Task 1 IF the gate-mechanics surface non-trivial doctrine choices. The DECISIONS.md ledger head before 05.2 Task 1 is ADR-0023 (or ADR-0024 if ADR-0024 lands first); ADR-0025 lands at the next-sequential available number.

**No additional ADRs anticipated for 05.2.** The HCM-on-H2 dispatch and the h2spec attach are mechanically scoped per the parent-05 brainstorm; no additional Y/N decision points are projected at execution time.

If a Y/N decision surfaces during execution that isn't covered by ADR-0024 / ADR-0025 (e.g., a `BEHAVIOR_CONTRACT.md` allow-list extension forced by an unexpected H2-specific response header surface, or a `MAX_FRAME_SIZE` posture beyond the 4-field subset in D2.b), the planner appends the next-sequential ADR (ADR-0026) at the time it lands.

---

## 8. Artifacts this sub-phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/05.2-http2-downstream/PLAN.md` (lands at standalone pre-Task-1 commit per §6 signpost 12)
- `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` (per-task progress notes)
- `docs/envoy-rust/phases/05.2-http2-downstream/REVIEW.md` (state-5 review)
- `crates/envoy-http2/Cargo.toml` (D1)
- `crates/envoy-http2/src/lib.rs` (with `#![forbid(unsafe_code)]`) (D1)
- `crates/envoy-http2/src/codec.rs` (D1)
- `crates/envoy-http2/src/hcm.rs` (D3)
- `crates/envoy-http2/src/request.rs` (D3)
- `crates/envoy-http2/src/response.rs` (D3)
- `crates/envoy-http2/src/error.rs` (D3)
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml` (D2 fuzz seed)
- `crates/envoy-bin/tests/http2_direct_response.rs` (D4 in-process integration test)
- `tests/conformance/h2spec/Cargo.toml` (D7)
- `tests/conformance/h2spec/src/lib.rs` (D7; empty or shared helpers)
- `tests/conformance/h2spec/tests/h2spec_runner.rs` (D7)
- `tests/conformance/h2spec/h2spec.yaml` (D7 envoy-rust HCM config for the h2spec target)
- `tests/conformance/h2spec/known-failures.txt` (D7; populated at task time)
- `tests/fixtures/0009-http2-direct-response/envoy.yaml` (D6)
- `tests/fixtures/0009-http2-direct-response/envoy-rust.yaml` (D6)
- `tests/fixtures/0009-http2-direct-response/inputs/payload.bin` (D6; empty)
- `tests/fixtures/0009-http2-direct-response/expectations.yaml` (D6)
- `tests/fixtures/0009-http2-direct-response/README.md` (D6)
- `tests/differential/tests/http2_direct_response.rs` (D6 Docker-gated test wrapper)

Amended during execution:

- `Cargo.toml` (root) — `[workspace] members` gains `crates/envoy-http2` and `tests/conformance/h2spec`. (D1 + D7)
- `crates/envoy-config/src/bootstrap.rs` — `CodecType::HTTP2` accept-flip + `Http2ProtocolOptions` struct + `HttpConnectionManagerConfig.http2_protocol_options` field + ~10 new validator unit tests + 1 corpus-walk acceptance test. (D2)
- `crates/envoy-config/src/lib.rs` — append `ConfigError::Http2OverTlsNotSupported` and `ConfigError::Http2ProtocolOptionsOutOfRange` variants; extend the `pub use bootstrap::{...}` re-export with `Http2ProtocolOptions`. (D2)
- `crates/envoy-config/fuzz/.gitignore` — append `!corpus/parse_bootstrap/hcm_codec_http2.yaml`. (D2)
- `crates/envoy-bin/src/main.rs` — extend the `HttpConnectionManager` typed-config arm with the H1-vs-H2 dispatch on `HCMConfig.codec_type`. (D4)
- `tests/differential/Cargo.toml` — add direct `h2 = "0.4"` dep entry (the carve-out per §5 architectural rule 1). (D5.b)
- `tests/differential/src/lib.rs` — add `Driver::Http2` variant + `drive_http2` helper + `run_fixture` dispatch arm for `Driver::Http2` + 1 new harness unit test. (D5)
- `docs/envoy-rust/DECISIONS.md` — append ADR-0024 and/or ADR-0025 at Task 1 IF they land (per §7 above). The DECISIONS.md ledger head before 05.2 Task 1 is ADR-0023.
- `docs/envoy-rust/ROADMAP.md` — at the state-6 phase-done commit, flip row `05.2` `status: planned` → `status: done`. Parent row `05` stays `in-progress` (flips at 05.3's state-6 commit per the ROADMAP-schema invariant). Row `05.3` stays `planned`.
- `docs/envoy-rust/STATE.md`:
  - At PLAN.md commit (state-2 close-out): active phase id stays `05.2`; lifecycle state advances 2 → 3 (PLAN.md exists, implementation incomplete); next-skill advances to `superpowers:subagent-driven-development` per the user's standing preference (per auto-memory `feedback_execution_style`; matches 05.1's posture).
  - At state-6 phase-done commit: active phase id advances `05.2` → `05.3`; slug advances `05.2-http2-downstream` → `05.3-http2-upstream`; lifecycle state advances to phase 05.3 lifecycle state 2 (05.3's SPEC was landed at parent-05 state-2 alongside this SPEC; PLAN.md does not exist for 05.3). Next-skill: `superpowers:writing-plans` scoped to sub-phase 05.3.
  - Notes section gains the 05.2 close-out bookkeeping (the carve-out for `drive_http2` consuming `h2` directly per §6 signpost 6 and the `Cargo.lock` sync diff for the `h2`+`http` direct-dep landing).
- `Cargo.lock` — synced inline with the dep-introducing tasks per the established phase-precedent (Task 1 lands `h2` + `http` direct deps; the lock-file diff lands at Task 1 commit). Expected diff: non-trivial (`h2` + `http` + `slab`/`fnv`/`tokio-util` transitive surface formalize as direct surfaces in the workspace's resolved graph).
- `deny.toml` — likely no-op (`h2`, `http`, and their transitive deps' licenses are already on the allow-list). Cross-checked at state-4.
- `.github/workflows/ci.yml` — extend with `h2spec` binary provisioning step before the `cargo test --workspace` step. Choice between `apt-get install h2spec` (Debian/Ubuntu runner) and `curl -L .../h2spec_linux_amd64.tar.gz | tar xz` (cross-distro fallback) lands at Task 7 per §6 signpost 3. (D7)

Not touched in 05.2 (belong to other phases or are frozen):

- `docs/envoy-rust/phases/05-http2/SPEC.md` (parent) — unedited; remains the design artifact committed at parent-05 state-1 SHA `cd1a70e`.
- `docs/envoy-rust/phases/05.1-fixture-hardening/SPEC.md` — closed at 05.1's state-6 phase-done commit; unedited in 05.2.
- `docs/envoy-rust/phases/05.3-http2-upstream/SPEC.md` — landed at parent-05 state-2 alongside this SPEC; unedited in 05.2 (its PLAN/PROGRESS/REVIEW land in its own sub-phase execution window).
- `docs/envoy-rust/phases/{00,01,02,02.1,02.2,03,03.1,03.2,04,04.1,04.2,04.3}/*` — closed in earlier phases.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — no edits in 05.2 (per §2 above).
- `crates/envoy-tls/`, `crates/envoy-tcp/`, `crates/envoy-http1/`, `crates/envoy-listener/`, `crates/envoy-cluster/`, `tests/helpers/{tcp,tls,http1}-echo-server/` — finalized in earlier phases; phase 05.2 consumes via existing public APIs without amendment. (Notably: `envoy-http1::HCMConfig` + `envoy-http1::hcm::build_response` + the route-walk + the `BuildOutcome` enum are all consumed unchanged from `envoy-http2`'s HCM-on-H2 dispatch.)
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, `tests/fixtures/0007-http1-direct-response/`, `tests/fixtures/0008-http1-router-upstream/` — unedited; their fixtures must remain green at the 05.2 state-4 phase-done gate.
- `tests/fixtures/0010-http2-router-upstream/` — does not exist at 05.2 close (lands in 05.3).
- Root `Cargo.toml`'s `[workspace] exclude` — unchanged (`crates/envoy-config/fuzz` continues to be excluded from the workspace per ADR-0009).
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.
- `crates/envoy-cluster/src/cluster.rs` — unchanged in 05.2 (the `Cluster.upstream_protocol` field that 05.3 introduces does not exist at 05.2 close).
- `crates/envoy-config/fuzz/Cargo.toml`, `crates/envoy-config/fuzz/fuzz_targets/` — unchanged. Only the corpus directory grows (1 new seed file).

---

## 9. Final commit message format (for state 6 of the 05.2 lifecycle)

The 05.2 phase-done commit flips ROADMAP row `05.2` `in-progress` → `done`; parent row `05` stays `in-progress` (flips at 05.3's phase-done commit). Format models the 04.x sub-phase shape (e.g., 04.1's `phase 04.1: HTTP/1.1 codec + HCM scaffold + direct_response + fixture 0007 [ADR-0020]`):

```
phase 05.2: envoy-http2 + HCM-on-H2 + fixture 0009 + h2spec ≥95% [<ADR-0024,ADR-0025>]

New workspace member crates/envoy-http2/ ships as the workspace's
sole-dep-owner of h2 = "0.4" per the cross-sub-phase architectural rule
established by parent-05 ADR-0022 (mirrors envoy-http1's sole-owner-of-
httparse posture from 04.1 + envoy-tls's sole-owner-of-rustls from 03.1).
Module decomposition: codec + hcm + request + response + error (the
client.rs module that lands in 05.3 is NOT created in 05.2).

envoy-config schema additions: CodecType::HTTP2 flips from reject to
accept; new listener-side Http2ProtocolOptions struct (4 optional u32
fields: max_concurrent_streams, initial_stream_window_size,
initial_connection_window_size, max_frame_size); 2 new ConfigError
variants (Http2OverTlsNotSupported, Http2ProtocolOptionsOutOfRange).
TLS+HTTP2 listener combos reject at parse time (TLS+ALPN+H2 deferred
per parent-05 SPEC §4). HTTP3 continues to reject. ~10 new validator
unit tests + 1 fuzz corpus seed (hcm_codec_http2.yaml).

HCM-on-H2 dispatch in crates/envoy-http2/src/hcm.rs implements
envoy_listener::ConnectionHandler. Per-stream tokio::spawn pattern.
Reuses envoy_http1::HCMConfig + envoy_http1::hcm::build_response + the
route-walk + BuildOutcome enum end-to-end; only the codec layer at the
connection edge changes. The :authority -> Host: synthesis in
request.rs makes the route-walk H2-transparent. H2-forbidden hop-by-hop
headers (connection/transfer-encoding/upgrade/keep-alive/proxy-connection)
stripped defensively in response.rs per RFC 7540 §8.1.2.2. The
BuildOutcome::Proxy path is structurally exercised but stubbed with a
502 Bad Gateway response (the upstream H2 dispatch lands in 05.3 D13.3).

envoy-bin's HCM dispatch arm gains H1-vs-H2 selection on HCMConfig.
codec_type. New in-process integration test
crates/envoy-bin/tests/http2_direct_response.rs.

Differential harness gains Driver::Http2 + drive_http2 (carve-out
consuming h2 directly per parent-05 SPEC §6 signpost 8 + cross-sub-phase
architectural rule 1's documented carve-out, parallel to phase-04.1
REVIEW M-architectural-claim's httparse posture).

Fixture 0009-http2-direct-response (5 files): HCM codec_type: HTTP2 +
direct_response 200 "ok\n". No upstream cluster (clusters: []).

First conformance suite tests/conformance/h2spec/ ships at the ≥95%
pass gate. Runner spawns envoy-bin against an HCM HTTP2 config, runs
h2spec via subprocess, parses output, asserts overall pass rate ≥95%
AND every failing test enumerated in known-failures.txt with one-line
doctrine reasons. CI workflow extends with h2spec binary provisioning
before cargo test --workspace.

ADR-0024 (CONDITIONAL) lands IF the planner determines `http` direct-dep
on crates/envoy-http2/Cargo.toml warrants narrow-scoped permitted-
foundation grant documentation. ADR-0025 (CONDITIONAL) lands IF the
h2spec integration's gate-mechanics surface non-trivial doctrine
choices. Both default to landing per the planner-recommended posture;
non-landing leaves the ADR numbers available for phase-06+.

Phase-04.1 REVIEW M-claim (drive_http1 per-function unit test) stays
deferred per the 04.3 disposition; 05.2 introduces no new H1 surfaces.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (restored by 05.1, unchanged);
  tests/fixtures/0004-tls-downstream green (restored by 05.1);
  tests/fixtures/0005-tls-upstream green (restored by 05.1);
  tests/fixtures/0006-tls-sni green (restored by 05.1);
  tests/fixtures/0007-http1-direct-response green (unchanged);
  tests/fixtures/0008-http1-router-upstream green (restored by 05.1);
  tests/fixtures/0009-http2-direct-response green (NEW; HTTP/2 listener
    + direct_response action under H2 framing).
Conformance: tests/conformance/h2spec at ≥95% pass; failing tests
  catalogued in tests/conformance/h2spec/known-failures.txt with one-
  line doctrine reasons.
```

ROADMAP row `05.2` flips `in-progress` → `done` at this commit. Parent row `05` stays `in-progress` (flips at 05.3's state-6 phase-done commit per the ROADMAP-schema invariant "parent flips to `done` only after all sub-phases are `done`"). STATE.md advances to phase `05.3` lifecycle state 2 (05.3's SPEC was landed at parent-05 state-2 alongside this one); next-skill `superpowers:writing-plans` scoped to sub-phase 05.3 (upstream H2C origination + router H2-arm + http2-echo-server helper + fixture 0010 + parent-05 close per parent-05 SPEC §3 D11.3–D15.3). Phase-05's projected ADR ledger after this commit: ADR-0022 (parent-05 split decision), ADR-0023 (StrictDns; landed at 05.1 Task 1), and ADR-0024 / ADR-0025 (conditional — landed at 05.2 Task 1 if applicable). Future ADRs from 05.3 land at the next-sequential numbers (ADR-0024+ if neither 05.2-conditional ADR landed; ADR-0025+ if only one landed; ADR-0026+ if both landed).
