# Phase 05 — HTTP/2 cleartext (H2C prior-knowledge): fixture-hardening preamble + downstream H2C + upstream H2C origination

- **Phase id:** `05`
- **Slug:** `05-http2`
- **Title:** HTTP/2 cleartext (H2C prior-knowledge) data plane: a fixture-hardening preamble that closes the cross-phase Docker-gated `host.docker.internal`/`STATIC` regression on fixtures 0003–0008 by introducing `ClusterType::StrictDns`, plus downstream H2C HCM dispatch via the `h2` codec, plus upstream H2C origination through the router proxy arm, plus first-time attachment of the `h2spec` conformance suite at a ≥95% pass gate
- **Depends on:** `04` (HTTP/1.1 data plane — codec + HCM + route matchers + router upstream). Phase 04 ROADMAP row is `done` as of commit `e626862` (the parent-04 state-6 close-out, which also flipped sub-phase row `04.3` from `in-progress` to `done`). Phase 05 enters `in-progress` at this state-1 close-out commit.
- **Seeded by:** `BOOTSTRAP_PROMPT.md` §8 row 05 — *"HTTP/2 downstream + upstream (low-level framer, own conn mgr)"* with the differential surface gate *"HTTP/2 fixture green; `h2spec` above threshold"*. The `h2` crate is on D-3.2's permitted-foundations list explicitly as *"HTTP/2 codec (from the hyper project), used as a low-level codec only. Never as a server runtime. Direct analogue of Go's `golang.org/x/net/http2`"*. `hyper` itself remains forbidden as a direct dependency.
- **Differential surface when done:**
  - **Fixtures restored to Docker-gated green by 05.1's C-1 fix:** `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, `tests/fixtures/0008-http1-router-upstream/`. These were latently red against upstream `envoyproxy/envoy:v1.33.0` for ≥5 phases (since phase-02.2's ADR-0015 landing) due to the `host.docker.internal`/`STATIC` parse-rejection regression — see the C-1 trace in §1 below.
  - **New fixtures green:** `tests/fixtures/0009-http2-direct-response/` (H2C downstream → `direct_response` route action returning a static 200 body via H2 framing) and `tests/fixtures/0010-http2-router-upstream/` (H2C downstream → router → H2C upstream cluster reaching the new in-tree `http2-echo-server` helper).
  - **First conformance suite attaches:** `tests/conformance/h2spec/` runs at the **≥95% pass** gate with failing tests catalogued in `tests/conformance/h2spec/known-failures.txt` and cross-referenced in 05.2's REVIEW §4.
  - **Fixtures unchanged but verified green:** `0001-tcp-echo`, `0002-static-admin-ready`, `0007-http1-direct-response` (no `host.docker.internal` substitution; not affected by C-1).
- **Sub-phases:** **`05.1`, `05.2`, `05.3`** projected (codified at parent-05 state-2 via **ADR-0022** — see §7).

This SPEC is the design contract for the parent phase 05. It projects the split into three sub-phases by surface boundary (fixture-hardening preamble → downstream H2C codec/HCM/h2spec → upstream H2C client + router H2-arm). The 3-way split is deliberate (mirrors phase 04's three-way precedent under ADR-0020) and was selected over a two-way split because the C-1 fixture-hardening preamble is a coherent, externally-caused dependency that the H2 work materially benefits from completing first; bundling it with downstream H2 (the closest two-way alternative) would push 05.1 to ~1700 LoC, leaving little headroom against the §6.1 split-gate (~1500 LoC) and arguably re-creating the gate-pressure that motivated phase-04's three-way decision.

This SPEC is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed phase-04 surface (via `git log` and the in-tree `envoy-http1` / `envoy-config` / `envoy-cluster` / `envoy-tls` / `envoy-tcp` / `envoy-bin` / `tests/differential` / `tests/helpers/{tcp,tls,http1}-echo-server` shape at HEAD `e626862`) must be able to operate as the parent-05 state-2 session — landing **ADR-0022** (split decision), the three sub-phase SPECs, and the ROADMAP rows for `05.1`, `05.2`, `05.3`. Each sub-phase then enters its own state 3 with its own SPEC + PLAN cadence.

---

## 1. Goal and acceptance signal

**Goal.** Land HTTP/2 cleartext on the data plane in three coordinated layers. Across all three layers, the architectural rule is **`envoy-http2` is the SOLE workspace dep on `h2`** — mirrors how `envoy-http1` is the sole dep on `httparse` (parent-04 SPEC §3 cross-sub-phase rule 1; established in 04.1) and `envoy-tls` is the sole dep on `rustls` (phase-03 precedent).

1. **Fixture-hardening preamble + `ClusterType::StrictDns`** (sub-phase **05.1**). Closes the cross-phase Docker-gated regression that has been latent across phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3 (see C-1 trace below). Adds a second variant to the `ClusterType` enum at `crates/envoy-config/src/bootstrap.rs` (currently single-variant `Static` at the `04163c5`-era line range; the `04.3` close at `e626862` does not extend it), implements the `STRICT_DNS` validator accept path, extends `crates/envoy-cluster/src/cluster.rs`'s `Cluster` construction to perform name-resolution at cluster-build time for `STRICT_DNS` clusters (`host.docker.internal` resolves locally via the Docker-managed `host-gateway` per ADR-0015; for `STATIC` clusters the existing literal-IP construction stays unchanged), and coordinates a 5-fixture edit (`tests/fixtures/{0003,0004,0005,0006,0008}/{envoy.yaml,envoy-rust.yaml}`) flipping `type: STATIC` → `type: STRICT_DNS` where `host.docker.internal` is the backend host literal. Also closes the dormant phase-02.1 REVIEW I3 (a positive `ClusterType::Static` variant-name regression guard) since adding the second variant gives the regression-guard test a way to discriminate against `Static`. NO H2 work in 05.1 — the sub-phase is purely a fixture-hardening preamble that restores green Docker-gated CI on the 5 affected fixtures before any new H2 layers are added on top.

2. **Downstream H2C HCM + `h2spec`** (sub-phase **05.2**). New workspace member `crates/envoy-http2/` (sole-dep-owner of `h2 = "0.4"`; pulls `bytes` for buffer mgmt, `tokio` for runtime, `thiserror` for typed errors, `tracing` for logging — all D-3.2 permitted foundations). Public surface: `Http2Codec` thin adapter over `h2::server::Builder`/`h2::server::Connection`, an `HCM` `ConnectionHandler` impl that drives an H2C connection (per-stream via `tokio::spawn`; one stream per logical request; reuses 04.x's `HCMConfig` end-to-end so the route-walk + router invocation site introduced by 04.1 + extended in 04.3 stays unchanged — only the codec layer at the connection edge changes from H1 to H2), a per-stream request-builder that translates `h2`'s `http::Request<h2::RecvStream>` into the existing `envoy_http1::codec::Request` value type (with `:authority` mapped to `Host:` for the route-walk) and a per-stream response-emitter that translates the existing `envoy_http1::codec::Response` value type into an `h2`-shaped `http::Response<()>` + a body stream. Schema additions in `envoy-config`: flip `CodecType::HTTP2` from reject to accept; introduce listener-side `Http2ProtocolOptions` (a subset of Envoy's full schema — `max_concurrent_streams`, `initial_stream_window_size`, `initial_connection_window_size`, `max_frame_size` — all optional with defaults from `h2`). Fixture `0009-http2-direct-response` proves an H2C round-trip byte-equivalent (modulo the existing 04.x header allow-list, modulo HTTP/2 framing's structural-equivalence rule per BEHAVIOR_CONTRACT.md row 4 — *"structurally equivalent (same frame types/order on equivalent events); not byte-equal"*). Harness gains `Driver::Http2` + `drive_http2` (drives via `h2::client`). First conformance suite attaches: `tests/conformance/h2spec/` runner crate that drives the upstream `h2spec` binary against an envoy-rust H2 listener, parses h2spec's output, asserts ≥95% pass overall, and matches any failures against `known-failures.txt` (any test failing that isn't on the known-failures list breaks the gate).

3. **Upstream H2C origination + router H2-arm + parent-05 close** (sub-phase **05.3**). New `envoy-http2::Client` (per-connection plaintext H2 client; one TCP connection per upstream call; no pooling — pooling is upstream-robustness-family territory and is materially more interesting under H2 because of stream multiplexing, so deferring it intentionally avoids prematurely committing to a pool design). Schema additions: cluster-side `Http2ProtocolOptions` via Envoy's `typed_extension_protocol_options` mechanism (`envoy.extensions.upstreams.http.v3.HttpProtocolOptions` typed_config; subset of fields). Router H2-arm: the existing `RouteAction::Route` arm landed in 04.3 at `crates/envoy-http1/src/hcm.rs:189-288` (`Proxy` `BuildOutcome` dispatching through `cluster_mgr.get → pick_endpoint → envoy_http1::Client::connect → send_request → router::write_proxied_response`) is extended to dispatch into either H1 or H2 based on the cluster's protocol options (decided at cluster-build time and stored as a typed `Cluster.upstream_protocol: UpstreamProtocol { Http1, Http2 }` field, default `Http1` for backwards-compat with all phase-04 clusters); on H2-cluster path, calls `envoy_http2::Client::connect → send_request → write_proxied_response` (the response-builder reuses 04.3's `router::write_proxied_response` shape since the response wire-format is HCM-on-downstream's concern, not protocol-of-the-upstream's). New helper crate `tests/helpers/http2-echo-server/` (sibling of `tcp-echo-server` / `tls-echo-server` / `http1-echo-server`; `h2`-based; deterministic alphabetically-sorted-headers echo body — load-bearing for differential equivalence). Fixture `0010-http2-router-upstream` proves an H2C round-trip end-to-end through the router. Parent ROADMAP row `05` flips `done` at 05.3's state-6 phase-done commit per the `e626862`-shape close-out (the last sub-phase commit also closes the parent in the same commit).

**C-1 trace for self-containment** (per D-3.4, since 05.1's preamble is the C-1 fix). Upstream Envoy v1.33.0 rejects the rendered `address: host.docker.internal` under `type: STATIC` with this critical-log line:

```
[critical][main] [source/server/server.cc:416] error initializing config '/etc/envoy/envoy.yaml':
malformed IP address: host.docker.internal. Consider setting resolver_name or setting cluster type
to 'STRICT_DNS' or 'LOGICAL_DNS'
```

The regression originates at phase-02.2's ADR-0015 landing (`host.docker.internal` introduced as the `BACKEND_HOST` substitution for cross-container reachability via `host-gateway`; commit `435c6fa`). Subsequent phases 02.2, 03.1, 03.2, 04.1, 04.2, 04.3 did not push to CI between the phase-02.1 close (run `24913934580`) and the phase-04.3 task 14 differential-test push (run `25106213773`), so the regression has been latent across **five phases**. Envoy v1.33's tightened `socket_address.address` parse semantics expect either a literal IP (under `STATIC`) or DNS resolution opt-in (under `STRICT_DNS`/`LOGICAL_DNS`). The 04.3 REVIEW (committed at `eb030d1`) flagged this as Important cross-phase carryforward C-1 with the recommended forward work being a coordinated edit across the 5 affected fixtures plus the schema growth. The phase-04.3 STATE.md handoff (committed at `e626862`) recorded three options for the phase-05 brainstorm — (a) fold into 05 as a Task-1 preamble, (b) split into a dedicated fixture-hardening sub-phase, (c) ratify the deferral. **The phase-05 brainstorm session selected option (b) implemented as sub-phase 05.1 inside parent 05** — see §5 below.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to phase 05's full feature surface across all three sub-phases:

- **(a)** the new differential fixtures `tests/fixtures/0009-http2-direct-response/` and `tests/fixtures/0010-http2-router-upstream/` are green at the Docker-gated CI level;
- **(b)** the pre-existing differential fixtures `0001-tcp-echo`, `0002-static-admin-ready`, `0003-tcp-proxy`, `0004-tls-downstream`, `0005-tls-upstream`, `0006-tls-sni`, `0007-http1-direct-response`, `0008-http1-router-upstream` are all green at the Docker-gated CI level (`0003`/`0004`/`0005`/`0006`/`0008` are restored to green by 05.1's C-1 fix; `0001`/`0002`/`0007` are unaffected by C-1 and continue green);
- **(c)** the conformance suite `tests/conformance/h2spec/` runs at **≥95% pass** with any failing tests explicitly catalogued in `tests/conformance/h2spec/known-failures.txt` and cross-referenced in 05.2's REVIEW §4 (each known-failure entry carries a one-line doctrine reason — e.g., *"deferred to access-log family"*, *"h2 crate doesn't expose hook"*, *"intentional Envoy-divergence per ADR-NNNN"*); the gate fails if any non-listed test regresses, OR if the overall pass rate drops below 95%, OR if a previously-listed-as-failing test starts passing without `known-failures.txt` being trimmed in lockstep;
- **(d)** the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 05.1 (≥1 new `STRICT_DNS` cluster seed) + 05.2 (≥1 new HCM `codec_type: HTTP2` + `Http2ProtocolOptions` listener-side seed) + 05.3 (≥1 new cluster-side `Http2ProtocolOptions` typed_extension_protocol_options seed). No new fuzz target ships in phase 05;
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- **(f)** all three sub-phase `REVIEW.md` verdicts are approved.

The parent-phase-done commit lands at the **last sub-phase's state-6 commit** (i.e., 05.3's phase-done commit also flips parent row `05` from `in-progress` to `done` — mirrors phase 04's `e626862` close-out, which mirrored phase 03's `ca81226` close-out).

---

## 2. Behavior-contract scope for phase 05

Phase 05 is the first phase to introduce HTTP/2 framing on the data plane. The phase makes **no edits** to `BEHAVIOR_CONTRACT.md`'s equivalence matrix or to its existing populated subsections (the `Header allow-list` table populated in phase 04 with `server`, `date`, `x-envoy-upstream-service-time`). Two engagement points:

1. **Equivalence-matrix row 4 (HTTP/2 & HTTP/3 framing)** is exercised for the first time. The contract row already reads *"Structurally equivalent (same frame types/order on equivalent events); not byte-equal"*. Phase 05's harness `drive_http2` helper drives via `h2::client` and asserts on the parsed response surface (`http::Response<h2::RecvStream>`) rather than on raw wire bytes. The harness compares the response status, the response-header set (modulo allow-list, same as 04.x), and the response-body bytes (byte-exact for fixture 0009's `direct_response` static body and for fixture 0010's `http2-echo-server` deterministic echo body — both are byte-exact across both proxies because the body is content, not framing). Frame-level equivalence is implicit (both proxies emit valid H2 framing or `h2`-the-codec rejects the connection); no fixture asserts on raw frame bytes.

2. **`Header allow-list` — no new rows.** The 3 phase-04 rows (`server`, `date`, `x-envoy-upstream-service-time`) cover H2 as well — they're emission-semantics rules, not framing-bound. Specifically:
   - `:status` is **not** a response header in the H1 sense — it is the response status code under H2 framing (transmitted in the HEADERS frame's pseudo-header block; serialized by `h2` transparently). It is asserted via `equivalence.response_status: exact` (matrix row 1), not via the header allow-list.
   - HTTP/1.1 hop-by-hop headers (`Connection`, `Transfer-Encoding`, `Upgrade`, `Keep-Alive`, `Proxy-Connection`) are **forbidden in H2 messages** per RFC 7540 §8.1.2.2. Their absence is enforced at the codec layer (`h2` rejects them; envoy-rust's H2 router-arm response-builder strips them defensively before handing the response off to `h2`), so they're never on the wire in H2 fixtures and don't need allow-list entries.
   - `:method`, `:path`, `:authority`, `:scheme` are request-side pseudo-headers under H2; they are not response surface and don't engage the response-header allow-list.

   If 05.2 or 05.3 surface a header that envoy-rust emits but Envoy doesn't (or vice versa), the BEHAVIOR_CONTRACT.md table grows in lockstep with the in-code `HEADER_ALLOW_LIST` constant (`tests/differential/src/lib.rs:189-193` per 04.3's posture — Task 10 commit `cdd0218`). No such surface is anticipated — the response wire-format is shaped by `Http1Response`'s writer logic (reused from 04.x; only the codec layer at the connection edge changes between H1 and H2) and the response headers come from the upstream (for fixture 0010) or the static `direct_response` config (for fixture 0009).

3. **HTTP/2 trailers — out of scope (deferred non-goal).** Fixture 0010's `http2-echo-server` does not emit response trailers; the router H2-arm does not forward trailers; envoy-rust's H2 codec wrapper does not parse trailers from upstream responses. Trailers (`HEADERS` frame after `END_STREAM` on a DATA frame's stream) are an H2 first-class feature but engaging them requires non-trivial harness work (asserting on trailer set-equality) and a doctrine call on whether trailers fall under the existing header allow-list or get their own. Deferred to a follow-on phase or to whichever phase first emits trailer-bearing responses.

No `Stat-name`, `Access log field`, `xDS wire`, or `Timing tolerances` subsections are touched in phase 05.

---

## 3. Deliverables (organized by sub-phase)

This section enumerates the ~13 deliverables across the three sub-phases. Each sub-phase's own SPEC (written at parent-05 state-2 via the split commit) will expand its own deliverables into the per-task PLAN cadence the project follows.

### Phase 05.1 — fixture-hardening preamble + `ClusterType::StrictDns` + phase-02.1 I3 close

**D1.1 — `ClusterType::StrictDns` schema variant.** `crates/envoy-config/src/bootstrap.rs::ClusterType` enum (currently single-variant `Static` per the 04.3-era HEAD — the 04.3 REVIEW §4 C-1 entry confirms this exactly) gains a `StrictDns` variant. Serde tag matches Envoy's `STRICT_DNS` literal. Validator path accepts `STRICT_DNS` and treats the cluster's `endpoints[*].address` field as a DNS name to resolve at cluster-build time; on resolution failure, returns a typed error (new `ConfigError::ClusterDnsResolutionFailed { cluster: String, address: String, source: std::io::Error }`). The `LOGICAL_DNS` variant is **not** added in 05.1 — it differs from `STRICT_DNS` only in whether DNS results are re-resolved per-request vs. cached; the simpler `STRICT_DNS` shape suffices for the C-1 fix and `LOGICAL_DNS` defers to a later phase per parent SPEC §4 below. ~80 LoC schema + ~50 LoC validator + ~6 unit tests covering the parse path, the validator-resolve path against a known-resolvable name (`localhost` is the safest test target — universally resolvable; `host.docker.internal` is environment-dependent and is exercised at fixture level not unit level), and a mutually-exclusive parse test (a cluster that declares both `type: STRICT_DNS` and a literal IP address — verifies the validator path is consistent).

**D2.1 — `Cluster::new` extension for `STRICT_DNS` resolution.** `crates/envoy-cluster/src/cluster.rs::Cluster::new` (or whichever constructor lives there at 05.1 entry — the `04.3` close at `e626862` shape per the 04.3 REVIEW §1 lists `Cluster::name()` at lines 24-26, suggesting a small struct; the planner reads the live shape at task-1 time) gains a `STRICT_DNS` resolution branch. For `Static` clusters the existing literal-IP construction stays unchanged (regression-guarded by the new I3-closing test). For `STRICT_DNS` clusters, the constructor calls `tokio::net::lookup_host(format!("{}:{}", address, port)).await?` and stores the resolved `SocketAddr` in the cluster's endpoint list. The lookup is performed once at cluster-build time (matches Envoy's `STRICT_DNS` semantics under v1.33's defaults — periodic re-resolution is a `dns_refresh_rate` knob deferred to a later phase). On `lookup_host` returning zero results, returns `ConfigError::ClusterDnsResolutionFailed`. ~50 LoC + ~3 tests including the I3 close-out (positive `Static` regression guard: a `Static` cluster with a literal IP passes through unchanged, with the constructor's `match cluster_type` arm exercised structurally rather than just configurationally — this is the test phase-02.1 REVIEW M1 originally projected before being deferred through 02.2/03.1/03.2/04.x as I3).

**D3.1 — Coordinated 5-fixture edit.** `tests/fixtures/{0003,0004,0005,0006,0008}/{envoy.yaml,envoy-rust.yaml}` flip `type: STATIC` → `type: STRICT_DNS` where the cluster's `endpoints[*].address` is `host.docker.internal` (the `BACKEND_HOST` substitution per ADR-0015). For `Static` clusters where the address is a literal IP (e.g., `127.0.0.1` for `0007`), no change. The 10 YAMLs are edited in lockstep (one commit per fixture pair, mirroring 04.3's per-fixture commit cadence; or one bundled commit per the planner's call). Fixtures 0001/0002/0007 are untouched (they don't use the `host.docker.internal` substitution at any cluster — fixture 0001 has no upstream cluster; fixture 0002 only exercises the admin endpoint; fixture 0007 is `direct_response`-only with no upstream). After this edit, all 5 affected Docker-gated tests pass against upstream Envoy v1.33.0 again. ~40 LoC of YAML diff total + the locally-verified Docker run.

**D4.1 (verification deliverable, no code).** Re-push to CI to confirm green Docker-gated runs across `0003`, `0004`, `0005`, `0006`, `0008`. PROGRESS.md quotes the CI run URL + the 5 test results inline per the standard verification cadence. If any fixture remains red after the schema + fixture edits, the planner re-enters state 3 (REVIEW.md re-loop per `BOOTSTRAP_PROMPT.md` §5.2) — but no further coding is anticipated; the schema growth + fixture edits are mechanically sufficient.

### Phase 05.2 — `envoy-http2` foundation + downstream H2C HCM + fixture 0009 + `h2spec` ≥95% gate

**D5.2 — New library crate `crates/envoy-http2/`.** Added to root `Cargo.toml` `[workspace] members`. Sole-dep-owner of `h2 = "0.4"` (the latest stable line at SPEC writeup). Cargo deps: `h2 = "0.4"`, `bytes = "1"`, `tokio = { version = "1", features = ["net", "io-util", "macros", "sync", "time"] }`, `thiserror = "2"`, `tracing = "0.1"`, `envoy-config = { path = "../envoy-config" }`, `envoy-listener = { path = "../envoy-listener" }`, `envoy-http1 = { path = "../envoy-http1" }` (for `Request`/`Response` value types, `headers::*` constants, `HCMConfig`, the route-walk and router-arm dispatch — the H2 wrapper is a codec-edge translator atop the existing 04.x HCM). Crate root `lib.rs` carries `#![forbid(unsafe_code)]` per D-3.8.

  **Module decomposition** (final shape decided at 05.2 SPEC writeup time; this is the projection):
  ```
  crates/envoy-http2/src/
    lib.rs        // crate root: #![forbid(unsafe_code)]; public re-exports
    codec.rs      // Http2Codec adapter over h2::server / h2::client
    hcm.rs        // HCM ConnectionHandler impl driving an H2 connection
    request.rs    // h2::RecvStream → envoy_http1::Request value-type translator
    response.rs   // envoy_http1::Response value-type → h2::SendStream emitter
    error.rs      // Http2Error typed-error enum
    client.rs     // (added in 05.3 — listed here for projection)
  ```

  Public surface re-exported at `lib.rs`:
  ```rust
  pub mod codec;
  pub mod hcm;
  pub mod request;
  pub mod response;
  pub use error::Http2Error;
  pub use hcm::{HCM, HCMConfig};      // HCM is the H2 connection handler;
                                       // HCMConfig type-aliases envoy_http1::HCMConfig
                                       // (the configuration is identical; only the
                                       // runtime dispatch differs by codec).
  // 05.3-projected:
  // pub use client::{Client, ClientStream};
  ```

  ~250 LoC impl + ~250 LoC unit tests in 05.2 (excluding `client.rs` which lands in 05.3).

**D6.2 — `envoy-config` schema additions for downstream H2.** Two edits in `crates/envoy-config/src/bootstrap.rs`:
  1. **`CodecType::HTTP2` accept path.** The existing `CodecType` enum (landed in 04.1; per 04.1 SPEC §3 D2.1 it accepts `AUTO` / `HTTP1` and rejects `HTTP2` / `HTTP3` with `ConfigError::UnsupportedCodecType`) flips to accept `HTTP2`. `HTTP3` continues to reject (deferred to QUIC family). `AUTO` continues to behave as `HTTP1`-only — byte-sniffing for the H2C `PRI` preamble is an explicit non-goal in 05 (see §4 below). Validator paths: `HTTP2` requires the listener to NOT have a TLS transport_socket (since 05's posture is plaintext H2C only — TLS+ALPN+H2 is deferred per §4 below); a TLS-bearing listener with `codec_type: HTTP2` rejects with `ConfigError::Http2OverTlsNotSupported`.
  2. **`Http2ProtocolOptions` schema (listener-side).** New struct in `bootstrap.rs`. Optional field on `HttpConnectionManagerConfig`. Subset of Envoy's `envoy.config.core.v3.Http2ProtocolOptions` proto:
     ```rust
     pub struct Http2ProtocolOptions {
         pub max_concurrent_streams: Option<u32>,            // h2 default: 100
         pub initial_stream_window_size: Option<u32>,        // h2 default: 65535
         pub initial_connection_window_size: Option<u32>,    // h2 default: 65535
         pub max_frame_size: Option<u32>,                    // h2 default: 16384
     }
     ```
     All four fields optional with defaults sourced from the `h2` crate. Validator rejects out-of-range values per RFC 7540 (e.g., `max_frame_size` must be in `[16384, 16777215]`); ~40 LoC + ~6 unit tests.

  Total ~150 LoC schema + ~80 LoC validator + ~12 unit tests + ≥1 fuzz-corpus seed. Validator rejection variants: `ConfigError::Http2OverTlsNotSupported`, `ConfigError::Http2ProtocolOptionsOutOfRange { field: &'static str, value: u32, range: (u32, u32) }`.

**D7.2 — HCM-on-H2 dispatch (`crates/envoy-http2/src/hcm.rs`).** Implements `envoy_listener::ConnectionHandler` (sibling of `envoy_http1::HCM` from 04.1). Per-connection state machine: hands the raw TCP stream to `h2::server::Builder::handshake(stream)` (which expects the H2C `PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n` preamble); for each accepted stream, spawns a `tokio::task` that:
  1. Reads the request headers from `h2::server::Connection::accept` (returns an `http::Request<h2::RecvStream>`).
  2. Translates `http::Request` headers + `:path` + `:method` + `:authority` into the existing `envoy_http1::codec::Request` value type via `request::http_to_envoy_request` (adapter in 05.2's `request.rs` — handles the `:authority` → `Host:` mapping for the route-walk, lowercases header names, drains the `h2::RecvStream` body bytes into `bytes::Bytes`).
  3. Hands the translated request to the existing 04.x route-walk + router invocation site (`envoy_http1::hcm::build_response(config, &request, close)` returns a `BuildOutcome` per 04.3's design).
  4. Translates the resulting `envoy_http1::codec::Response` back into an `http::Response<()>` + body via `response::envoy_response_to_http2(response, send_stream)` (adapter in 05.2's `response.rs` — handles the `:status` pseudo-header, strips H2-forbidden hop-by-hop headers `connection`/`transfer-encoding`/`upgrade`/`keep-alive`/`proxy-connection` defensively, writes body bytes to the `h2::SendStream`).
  5. Closes the stream (via `END_STREAM` on the body).

  In 05.2 the HCM only handles the `BuildOutcome::Synth` path (direct_response); the `BuildOutcome::Proxy` path is exercised structurally but its end-to-end test path is deferred to 05.3 (where the upstream H2 client lands). The 05.2 fixture 0009 is `direct_response`-only so this is not a regression.

  ~250 LoC impl + ~150 LoC unit tests (8-10 tests covering: H2 handshake completes; stream-1 GET request resolves to direct_response; stream-2 reuses the same connection; H2-forbidden header `Connection: close` in the response is stripped before emission; `:authority` correctly maps to `Host:` for the route-walk; missing `:authority` rejects with H2 stream-error; etc.).

**D8.2 — `envoy-bin` HCM-on-H2 wiring.** New `TypedConfig` dispatch arm — actually, the existing `HttpConnectionManager` arm in `crates/envoy-bin/src/main.rs` (sibling of `TcpProxy` arm landed in 02.2; the HCM arm landed in 04.1) gains a second branch that selects between `envoy_http1::HCM` and `envoy_http2::HCM` based on `HCMConfig.codec_type`. The `HCMConfig` (which lives in `envoy_http1` per 04.1 D3.1) gains a `codec_type: CodecType` field if it doesn't already (per 04.1 SPEC §3 D2.1 the `HttpConnectionManagerConfig` already carries `codec_type` — confirmed against the 04.3 close at HEAD `e626862`). The dispatch is a simple `match` at the `from_config` time. ~40 LoC + the in-process integration test `crates/envoy-bin/tests/http2_direct_response.rs` (sibling of `http1_direct_response.rs`; spawns envoy-bin subprocess via `CARGO_BIN_EXE_envoy-bin`; drives a single H2C `GET /` request via `h2::client`; reads response; asserts status + body + headers). ~120 LoC.

**D9.2 — Differential harness extensions for HTTP/2 + fixture 0009.**
  - `Driver::Http2 { method, path, host, expected_status, expected_body, expected_headers }` — new variant on the existing `Driver` enum in `tests/differential/src/lib.rs` (sibling of 04.1's `Driver::Http1`). The driver reuses the existing `BodyRule` and `HeaderRule` types.
  - `drive_http2` async helper — sibling of `drive_http1`. Opens a TCP connection to the listener; runs `h2::client::handshake(tcp)` to negotiate H2C; sends the constructed request via `h2::client::SendRequest::send_request`; reads the response (status + headers + body bytes) via the returned `h2::client::ResponseFuture`; returns `(http::StatusCode, Vec<(String, String)>, Vec<u8>)` matching `drive_http1`'s shape so `assert_equivalence`'s `diff_headers` works without modification.
  - Fixture `tests/fixtures/0009-http2-direct-response/` — 5 files (`envoy.yaml` with admin block + plaintext listener bind + HCM filter chain `codec_type: HTTP2` single-VH single-route `prefix: "/"` direct_response 200 `"ok\n"`; `envoy-rust.yaml` per-side divergences — no admin, `127.0.0.1` bind; `inputs/payload.bin` empty for the GET; `expectations.yaml` driver kind `http2` with `method: GET`, `path: "/"`, `host: "envoy-rust.test"`, `expected_status: 200`, `expected_body: { byte_exact: "ok\n" }`, `expected_headers: { rule: set_equal_modulo_allow_list }`; `README.md`).
  - Docker-gated `tests/differential/tests/http2_direct_response.rs` (sibling of `http1_direct_response.rs`).

  ~300 LoC harness + 5 fixture files + the Docker-gated test.

**D10.2 — `tests/conformance/h2spec/` runner crate + ≥95% pass gate.** New workspace member at `tests/conformance/h2spec/` (per `BOOTSTRAP_PROMPT.md` §7.3 directory). Cargo manifest declares an `[[test]]` entry that:
  1. Locates the upstream `h2spec` binary (Docker-gated CI; if `which h2spec` fails, the test is `eprintln!`-skipped per the established Docker-binary-locator pattern from 02.2's `TcpProxyBackend` and 03.2's `tls-echo-server`/04.3's `http1-echo-server`).
  2. Spawns envoy-bin as a subprocess against an h2spec-targeted YAML config (HCM with `codec_type: HTTP2` + a single VH with a single route returning `direct_response 200 "h2spec"`).
  3. Runs `h2spec -p <envoy-bin-port> --strict` (or whichever flags express "fail on any non-passing test"; the planner reads h2spec's CLI at task-1 time).
  4. Parses h2spec's output (h2spec emits a JSON-like or grep-friendly summary; the planner picks the form that's mechanically diffable).
  5. Asserts overall pass rate ≥ 95% AND any failing tests are listed in `tests/conformance/h2spec/known-failures.txt`. The known-failures file is maintained by-hand; failures land with a one-line doctrine reason. The gate fails if (a) overall pass rate drops below 95%, (b) a non-listed test fails, or (c) a listed-as-failing test starts passing without the file being trimmed.
  6. PROGRESS.md / REVIEW.md quote the h2spec output in full.

  ~250 LoC runner + the `known-failures.txt` (initially populated by the planner at task time; size unknown until envoy-rust's H2 dispatch is exercised end-to-end).

  **Sub-deliverable D10.2-deps:** the `h2spec` binary itself is not in-tree; CI provisions it (e.g., `apt-get install h2spec` or a `curl | tar` step in the GitHub Actions workflow). Local development runs the test against an installed h2spec; absent h2spec, the test is `eprintln!`-skipped.

### Phase 05.3 — `envoy-http2::Client` + router H2-arm + `http2-echo-server` helper + fixture 0010 + parent-05 close

**D11.3 — `envoy-http2::Client` (per-connection HTTP/2 client).** New module `crates/envoy-http2/src/client.rs`. Public surface (mirrors `envoy_http1::Client` from 04.3 D8.3):
  ```rust
  pub struct Client;

  impl Client {
      pub async fn connect(addr: std::net::SocketAddr, host: &str)
          -> Result<ClientStream, Http2Error>;
  }

  pub struct ClientStream {
      send_request: h2::client::SendRequest<bytes::Bytes>,
      host: String,
      // h2::client::Connection is driven on a background tokio::spawn for the
      // duration of the ClientStream's lifetime; dropped when the ClientStream
      // is dropped (one TCP connection per upstream call; no pooling).
  }

  impl ClientStream {
      pub async fn send_request(&mut self, request: envoy_http1::codec::Request)
          -> Result<envoy_http1::codec::Response, Http2Error>;
  }
  ```

  - `Client::connect` opens a plaintext TCP connection, calls `h2::client::handshake(tcp)`, drives the `h2::client::Connection` on a background `tokio::spawn`, and returns the `SendRequest` handle wrapped in `ClientStream`.
  - `ClientStream::send_request` translates the envoy `Request` value type into `http::Request<()>` (with `:authority` populated from the captured `host` if not already in the request's `Host:` header — symmetric with 04.3's `envoy_http1::Client` behavior; defensive against missing-Host), strips H2-forbidden hop-by-hop headers, sends the request via `h2::client::SendRequest::send_request`, writes the request body to the `SendStream` (CL-only — chunked/streaming request bodies deferred per §4 below), reads the response via `h2::client::ResponseFuture`, drains the response body from `h2::RecvStream` into `bytes::Bytes`, and translates the `http::Response<()>` + body back into an envoy `Response` value type.

  No connection pooling. Each `ClientStream` owns one TCP connection and is consumed by a single `send_request` call; subsequent calls require a new `Client::connect`. Pooling is upstream-robustness-family territory and is materially more interesting under H2 (one pooled connection serves many streams), so deferring it intentionally avoids prematurely committing to a pool design.

  `Http2Error` enum gains 4 variants: `UpstreamConnect { addr, source }`, `H2Handshake { source: h2::Error }`, `H2SendRequest { source: h2::Error }`, `H2RecvBody { source: h2::Error }`. (The 05.2 `error.rs` lands the codec-side variants like `MalformedH2HeaderBlock`; these client-side variants are additive.)

  ~250 LoC impl + ~250 LoC unit tests (8 tests covering connect-success / connect-refused / send-request-bytes-on-wire / explicit-Host-wins / response-body-read / multi-frame body / handshake-error-mapping / send-error-mapping).

**D12.3 — `envoy-config` schema additions for upstream H2.** `Http2ProtocolOptions` cluster-level via Envoy's `typed_extension_protocol_options` mechanism. The Envoy schema for this is `envoy.extensions.upstreams.http.v3.HttpProtocolOptions` (a wrapper that carries protocol-specific options for an upstream cluster's HTTP traffic). Subset shipped in 05.3:
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

  In code: a new `HttpProtocolOptions` typed_config variant in the `TypedConfig` enum (sibling of `TcpProxy` and `HttpConnectionManager`); a new `ExplicitHttpConfig` enum carrying either `Http1ProtocolOptions` (empty in 05; future fields like `chunk_encoding` defer) or `Http2ProtocolOptions` (the same struct that landed in 05.2 D6.2 listener-side, reused). Validator extension: rejects mixed `http_protocol_options` and `http2_protocol_options` on the same cluster (Envoy's `explicit_http_config` is mutually exclusive). New `Cluster.upstream_protocol: UpstreamProtocol { Http1, Http2 }` field stored on the parsed cluster, defaulted to `Http1` for clusters without `typed_extension_protocol_options` (backwards-compat with all phase-04 clusters).

  ~150 LoC schema + ~50 LoC validator + ~8 unit tests + ≥1 fuzz seed.

**D13.3 — Router H2-arm.** The existing `RouteAction::Route` arm (landed in 04.3 at `crates/envoy-http1/src/hcm.rs:189-288`) extends to dispatch into either H1 or H2 based on `Cluster.upstream_protocol`. The dispatch lives in the router-arm `serve_connection` site for the H1 HCM; it also lives at the H2 HCM's analogous `BuildOutcome::Proxy` dispatch point (the 05.2 D7.2 wires Synth-only; 05.3 wires Proxy too). The router-arm dispatch is a simple `match cluster.upstream_protocol` — `Http1` calls `envoy_http1::Client::connect/send_request` (unchanged from 04.3); `Http2` calls `envoy_http2::Client::connect/send_request`.

  Response-write path reuses 04.3's `envoy_http1::router::write_proxied_response` (since the response wire-format on the downstream is HCM-on-downstream's concern, not the upstream-protocol's; whether the upstream spoke H1 or H2 is invisible to the downstream once the response has been translated back into the envoy `Response` value type). The `x-envoy-upstream-service-time` measurement window stays the same (`Instant::now()` at connect; `start.elapsed()` after `send_request` returns).

  ~100 LoC dispatch site edits + ~50 LoC unit tests across both HCM crates.

**D14.3 — `tests/helpers/http2-echo-server/` (new workspace member).** Sibling of `tests/helpers/{tcp,tls,http1}-echo-server/`. `h2`-based; plaintext only (no TLS). Hand-parsed argv (`--port <u16>` + `--help` + `--version` per 04.3 task-11 review-fix shape). Minimal H2C echo: any request method + path produces `200` with `content-type: text/plain` + a body containing the deterministic echo (method + path + alphabetically-sorted-by-lowercased-name headers + body) — the alphabetic header sort is **load-bearing** for differential equivalence per 04.3 D3's posture (both proxies forward the SAME logical request to the SAME helper; the helper's sorted-header response is the byte-exact baseline). ~250 LoC + ~5 tests (4 argv + 1 round-trip).

  Cargo deps: `h2 = "0.4"` (the helper is permitted to depend on `h2` directly because it's a test-helper, not a workspace runtime crate; however, for tidiness it consumes `envoy_http2` instead of `h2` directly so the architectural rule "only `envoy-http2` depends on `h2`" stays enforced even for test helpers — same posture 04.3's `http1-echo-server` took with `envoy_http1` over direct `httparse`). `bytes`, `tokio`, `anyhow`, `thiserror`, `tracing`, `tracing-subscriber` — all pre-existing permitted foundations.

**D15.3 — Differential harness `Http2EchoBackend` + fixture 0010.** `Http2EchoBackend` mirrors `TcpProxyBackend` / `TlsEchoBackend` / `Http1EchoBackend` shape: `spawn() -> Result<Self>` (locates `http2-echo-server` binary at workspace `target/<profile>/http2-echo-server`; reserves port; spawns subprocess; waits for accept-readiness); `port() -> u16`; `container_host() -> &'static str` (`"host.docker.internal"` per ADR-0015 — but with `STRICT_DNS` cluster type per 05.1's preamble, so the DNS-rejection regression is no longer in play); SIGKILL-on-Drop posture. Locator helper `locate_http2_echo_server()`. `Driver::Http2` dispatch in `run_fixture`: a new `{{HTTP2_BACKEND_PORT}}` template marker substitutes the helper's port into the fixture YAMLs at render time.

  Fixture `tests/fixtures/0010-http2-router-upstream/` — 5 files (envoy.yaml with HCM `codec_type: HTTP2` + single-VH single-route `prefix: "/"` `route: { cluster: backend }` + cluster `backend` with `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options` + endpoint `{{BACKEND_HOST}}:{{HTTP2_BACKEND_PORT}}` under `type: STRICT_DNS`; envoy-rust.yaml per-side divergences mirroring 04.3 D4's posture; inputs/payload.bin empty for GET; expectations.yaml driver `http2` with proxy-shape assertions; README.md). Docker-gated `tests/differential/tests/http2_router_upstream.rs`. In-process integration backstop `crates/envoy-bin/tests/http2_router_upstream.rs`. ~250 LoC + 5 fixture files + 2 test files.

### Cross-sub-phase architectural rules (baked into the parent SPEC)

These rules are non-negotiable across the three sub-phases; sub-phase SPECs inherit them verbatim:

1. **`envoy-http2` is the SOLE workspace dep on `h2`.** Mirrors how `envoy-http1` is the sole dep on `httparse` (parent-04 SPEC §3 cross-sub-phase rule 1) and `envoy-tls` is the sole dep on `rustls`. No other crate calls `h2::*` directly. Test helpers consume `envoy-http2` (e.g., `http2-echo-server` consumes `envoy_http2::Codec` / `envoy_http2::Client` / similar surfaces) rather than `h2`. The differential harness's `drive_http2` helper is the one acceptable carve-out — it consumes `h2 = "0.4"` directly for low-level round-tripping, mirroring how 04.x's `drive_http1` consumes `httparse` directly. (This carve-out lands as documented carryforward analogous to phase 04.1 REVIEW M-architectural-claim.)

2. **HCM-on-H2 reuses 04.x's `HCMConfig` and route-walk wholesale.** Only the codec layer at the connection edge changes. The route-walk in `envoy_http1::hcm::build_response` is configuration-driven (it operates on the `Request` value type, not on raw H1 bytes), and accepts H2-translated `Request` values transparently. The router invocation site landed in 04.3 (the `BuildOutcome::Proxy` dispatch through `cluster_mgr.get → pick_endpoint → Client::connect → send_request → write_proxied_response`) is reused with `Client::connect` polymorphic over H1 and H2 via the new `Cluster.upstream_protocol` field.

3. **`:authority` → `Host:` mapping at the H2-to-envoy-Request translation boundary.** The H2 protocol carries the host name in the `:authority` pseudo-header; the route-walk in 04.x is `Host:`-driven (parent-04 SPEC §3 D3.1's "first-match-wins on `VirtualHost.domains` against request `Host:` header"). The translation adapter in 05.2's `request.rs` populates the envoy `Request.headers` with a synthesized `Host: <authority>` row at the bottom of the headers list (or wherever the lowercase-canonical position lives at 05.2 SPEC writeup time). Symmetric for the H2 client at 05.3: the captured `host` is sent as `:authority`.

4. **H2-forbidden hop-by-hop headers are stripped at the codec edges, not at the HCM core.** Per RFC 7540 §8.1.2.2, `Connection`, `Transfer-Encoding`, `Upgrade`, `Keep-Alive`, `Proxy-Connection` are forbidden in H2 messages. The translation adapters in 05.2's `request.rs` and `response.rs` (and 05.3's `client.rs` for outgoing requests) strip these defensively. The HCM core (in `envoy_http1`) does not need to know whether it's running under H1 or H2 dispatch.

5. **No H2-specific edits to `envoy-config`'s `RouteConfiguration` or `HeaderMatcher` schemas.** The route-walk + matcher fan-out from 04.1 + 04.2 is protocol-agnostic. Phase 05 extends only `CodecType` (accept `HTTP2`), adds `Http2ProtocolOptions` (listener and cluster forms), and grows `ClusterType` (the 05.1 preamble).

6. **`codec_type: AUTO` continues to behave as `HTTP1`-only.** Byte-sniffing for the H2C `PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n` preamble is an explicit non-goal in 05 (see §4 below). Fixtures 0009 and 0010 use explicit `codec_type: HTTP2`. AUTO byte-sniffing defers to whichever phase first surfaces a fixture or production-realism need that requires it.

7. **`http` crate (the `http::Request` / `http::Response` / `http::HeaderMap` types) is permitted as a transitive surface only.** The `h2` crate's API exposes `http::*` types directly (it's how `h2` represents headers and request/response metadata). Whether `http` ends up as a direct dependency in `crates/envoy-http2/Cargo.toml` (vs. transitive only via `h2`) is a small ADR — see §7 below for ADR-0024's possible landing. Either way, no other workspace crate imports `http::*` directly; the `envoy-http2` translation adapters absorb the `http`↔`envoy_http1::Request`/`Response` conversion at the codec edge.

---

## 4. Non-goals (deferred to later phases)

Out of phase 05 entirely:

- **HTTP/2 over TLS (ALPN-negotiated H2).** Listener-side ALPN config (`common_tls_context.alpn_protocols: ["h2", "http/1.1"]`), upstream-side ALPN (`UpstreamTlsContext.alpn_protocols`), and the codec-dispatch-by-ALPN mechanic. Carries the **M7 carryforward** (`TlsAcceptingHandler.inner: Arc<TcpProxy>` concrete-typed; HCM-in-TLS doesn't typecheck — phase-04.1 REVIEW M7) forward to the phase that ships ALPN-driven dispatch. That phase will likely either generalize `TlsAcceptingHandler` over `Arc<dyn ConnectionHandler>` or land a parallel `TlsAcceptingHcmHandler`.
- **`codec_type: AUTO` byte-sniffing for H2C.** AUTO continues to behave as `HTTP1`-only in 05. Fixtures use explicit `codec_type: HTTP2`. Defers to whichever phase first needs single-port H1/H2C multiplexing.
- **HTTP/2 over HTTP/1.1 Upgrade (`Upgrade: h2c`).** Envoy v1.33 does not support this mode on the server side per its docs (the `Upgrade: h2c` flow was deprecated and removed from the H2 spec post-RFC 7540). Out of scope indefinitely.
- **HTTP/3 / QUIC.** Separate family per `BOOTSTRAP_PROMPT.md` §9.
- **Cross-protocol H2↔H1 translation.** Specifically: a downstream H2 listener proxying to an upstream H1 cluster (or vice versa). Phase 05's fixtures are protocol-symmetric (0009 is H2-direct-response with no upstream; 0010 is H2 downstream + H2 upstream). The translation layer is non-trivial — pseudo-header conversion in both directions, framing-translation, request/response body re-framing — and benefits from its own focused phase. Defers to a follow-on phase.
- **Connection pooling on upstream H2.** H2's stream multiplexing means one pooled connection serves many streams; the pool design is materially richer than H1's (LRU/round-robin connection selection is insufficient — the pool must also track stream-count vs. `MAX_CONCURRENT_STREAMS`, handle `GOAWAY` frames mid-pool, etc.). Upstream-robustness family.
- **HTTP/2 trailers** in router proxy arm. The helper does not emit trailers; the router does not forward them; envoy-rust's H2 codec wrapper does not parse or write trailers. Defers to a follow-on phase or to whichever phase first emits trailer-bearing responses (gRPC fixtures will likely force this — gRPC family).
- **HTTP/2 server push (`PUSH_PROMISE` frames).** Removed from H3, rarely used in practice, and disabled by default in modern browsers. Deferred indefinitely.
- **HCM `server_name` field** (controls the `Server:` response header literally). Re-deferred from phase 04 — the parent-04 SPEC §4 named phase 05 as the natural landing point but the brainstorm chose to keep phase 05's scope focused on H2 codec/conn-mgr work. The `server` allow-list row continues to accommodate the divergence (`name-required, value-may-differ`); whichever phase first wants per-listener `Server:` value control lands the field.
- **Per-route `Http2ProtocolOptions` overrides.** Cluster-level only in 05.
- **`LOGICAL_DNS` cluster type.** 05.1 ships only `STRICT_DNS`. The two differ in whether DNS results are re-resolved per-request vs. cached at cluster-build time; `STRICT_DNS` matches the C-1 fixture-fix need. `LOGICAL_DNS` defers to whichever phase first needs per-request DNS re-resolution.
- **`dns_refresh_rate` / periodic DNS re-resolution for `STRICT_DNS` clusters.** The 05.1 implementation resolves once at cluster-build time. Periodic re-resolution is an Envoy knob (`Cluster.dns_refresh_rate`) that defers to a later phase.
- **HTTP/2 stream-level flow control tuning.** The default `h2`-crate window-size posture is used in 05; per-stream flow-control overrides (beyond the four `Http2ProtocolOptions` fields landed in 05.2 D6.2) defer.
- **HTTP/2 connection draining / `GOAWAY` handling on graceful shutdown.** Phase-08 (graceful drain) territory. Phase 05 ships H2 connections that close abruptly on listener shutdown; graceful drain semantics are out of scope here.
- **Upstream H2 cluster-side TLS (`transport_socket: tls` on a cluster with `http2_protocol_options`).** Combinatorial extension of M7. Defers to whichever phase first ships TLS+H2.
- **Per-listener / per-cluster compression-style options** (`compressor` filter, `gzip` extension). HTTP-filters family.
- **Trailers on direct_response.** Direct_response in 04.1 supported `inline_string` body only with no trailer support; 05 does not extend this for H2.
- **Server-Sent Events / chunked streaming responses in H2.** The H2 codec wrapper writes responses as a single body chunk via `h2::SendStream::send_data(.., end_of_stream=true)`; streaming bodies (chunked-equivalent via multiple DATA frames) defer to whichever phase first emits long-lived streaming responses.
- **Multiple HCM listeners.** Phase 02.1's `TooManyListeners` cap is unchanged in phase 05 (single listener per envoy-rust process). Future phases may relax this.

The sub-phase SPECs may surface small additional non-goals at SPEC writeup time; they will be enumerated in each sub-phase SPEC's own §4.

---

## 5. Splitting guidance for the planner

**Decision: split into 3 sub-phases.** The parent-05 split decision is codified in **ADR-0022** (lands at parent-05 state-2 alongside the sub-phase SPECs; mirrors phase-04's `1d9740d`-shape state-2 commit landing ADR-0020 + sub-phase SPECs).

**Three-way split rationale:**

The brainstorm considered three split shapes (recorded in ADR-0022's options list at landing time):

| Shape | Distribution | Estimate |
|---|---|---|
| **Single phase (no split)** | Fixture-hardening + downstream H2 + upstream H2 + h2spec all-in-one | ~3000 LoC, ~33 tasks → far over §6.1 gates |
| **Two-way split** | 05.1 = preamble + downstream H2 + h2spec; 05.2 = upstream H2 + parent close | 05.1 ≈ ~1700 LoC at the upper edge of the gate; 05.2 ≈ ~1300 LoC |
| **Three-way split** (chosen) | 05.1 = preamble; 05.2 = downstream + h2spec; 05.3 = upstream + parent close | each ≤ ~1300 LoC, ≤ ~14 tasks |

The three-way split was chosen over the two-way for two reasons:

1. **Headroom under the §6.1 split-gate.** The two-way's 05.1 at ~1700 LoC sits at the upper edge of the gate; the planner would have ~10% headroom before the gate fires. Phase-04's experience showed that brainstorm-time LoC estimates can drift by ~20% during execution (the actual phase-04.3 was estimated at ~1490 LoC and landed at ~1900 LoC of net Rust delta per the 04.3 REVIEW §1). The three-way's 05.2 at ~1300 LoC has ~13% headroom, with similar headroom on 05.3.
2. **Failure attribution.** A standalone fixture-hardening sub-phase 05.1 produces a clean REVIEW.md as an audit artifact for the cross-phase C-1 fix — useful for whichever future phase audits the C-1 close-out (or for an external reader trying to understand why fixtures 0003-0008 went red and then green again over the project's history). Bundling C-1 with downstream H2 (the two-way 05.1) would muddle the audit trail; a green REVIEW for "preamble + H2 downstream + h2spec" is harder to read forensically than two separate REVIEWs.

The brainstorm rejected single-phase and accepted the three-way as the natural shape. Each sub-phase fits comfortably under the §6.1 gates (~25 tasks / ~1500 LoC). **Do not nest-split any sub-phase.** If a sub-phase's actual PLAN.md crosses either gate at write-time, invoke `superpowers:systematic-debugging` first per `BOOTSTRAP_PROMPT.md` §6.1; nested splits of a sub-phase that was itself produced by a split deserve a fresh root-cause analysis (scope creep vs. planner overdecomposition).

**Sub-phase ordering and dependency:**

```
parent 05 (this SPEC)
    │
    ├─→ 05.1 (fixture-hardening preamble; ClusterType::StrictDns + 5 fixture edits + I3 close)
    │        │
    │        └─→ 05.2 (downstream H2C codec + HCM-on-H2 + fixture 0009 + h2spec ≥95% gate)
    │                │
    │                └─→ 05.3 (upstream H2C client + router H2-arm + fixture 0010 + parent 05 close)
```

Each sub-phase's `depends-on` ROADMAP column reflects this. The sub-phases ship strictly in order (05.1 → 05.2 → 05.3) — they cannot be parallelized because:

- 05.2's fixture 0009 dispatch through the differential harness depends on 05.1's restored Docker-gated baseline (fixture 0009 itself doesn't use `host.docker.internal`, but the harness's `cluster_mgr` build path does, and 05.2's `Driver::Http2` dispatch reuses the same `run_fixture` machinery that 05.1's fix unblocks).
- 05.3 extends both the schema (`Http2ProtocolOptions` cluster-side, `Cluster.upstream_protocol` field) and the runtime (router H2-arm in HCM dispatch) introduced in 05.2.

**Parent ROADMAP row 05 flips `done` at 05.3's state-6 phase-done commit** (mirrors phase 04's `e626862`-shape close-out: the last sub-phase commit also closes the parent).

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the sub-phase planners resolve them in-plan rather than mid-execution. Each sub-phase SPEC will inherit + extend its relevant signposts; this section lists the parent-level ones.

1. **`h2 = "0.4"` is the latest stable line as of phase-05 brainstorm.** The planner cross-checks at 05.2 Task 1 time. The `h2` crate is a permitted foundation per D-3.2 (no new ADR needed for the dependency itself); the choice of major version (0.4 at writeup) does not require an ADR.

2. **`Http2ProtocolOptions` schema subset.** Phase 05 ships only 4 fields (`max_concurrent_streams`, `initial_stream_window_size`, `initial_connection_window_size`, `max_frame_size`). Envoy's full proto has many more (e.g., `allow_connect`, `allow_metadata`, `hpack_table_size`, `override_stream_error_on_invalid_http_message`, `connection_keepalive`); they all default to RFC-conformant values and are not exercised by fixtures 0009/0010. The planner adds them only if a fixture or h2spec test forces it.

3. **`h2spec` binary management.** The planner picks the provisioning approach at 05.2 Task 1: (a) Docker image (e.g., `summerwind/h2spec`) wrapped in a `Command::new("docker")` invocation, (b) installed via system package or `curl | tar` in the CI workflow file, or (c) a Cargo-built `[[bin]]` from a vendored h2spec source (likely too heavy). Recommendation: (b) for CI, with a fallback `eprintln!`-skip pattern locally per the established Docker-binary-locator posture.

4. **`known-failures.txt` format.** The planner picks a format at 05.2 Task 1: (a) one test ID per line with a `# reason` comment, or (b) a structured TOML/YAML file with reason fields. Recommendation: (a) for diff-friendliness; (b) only if the reason structure benefits from typed fields (e.g., `[[failure]] id = "..." reason = "..." ticket = "..."`).

5. **`Cluster.upstream_protocol` field placement.** The planner decides whether to add this as a typed field on `envoy_cluster::Cluster` (the runtime struct) directly, or as a derived helper that consults the cluster's `typed_extension_protocol_options` lazily. Recommendation: the typed field, set at cluster-build time from the parsed config, defaulted to `Http1`. Avoids re-parsing config at each upstream call.

6. **Background `h2::client::Connection` driving.** Per `h2`'s API, a `h2::client::Connection` must be polled to drive the stream multiplexing; the typical pattern is `tokio::spawn(connection)` for the lifetime of the `SendRequest` handle. The planner decides at 05.3 Task 1 whether to use `tokio::spawn` directly (simplest; the connection task is dropped when the parent task drops the `SendRequest`), or wrap it in a `JoinHandle` stored on the `ClientStream` for explicit shutdown. Recommendation: `tokio::spawn` direct, matching `h2`'s docs.

7. **Test-helper architectural posture.** `http2-echo-server` consumes `envoy_http2` (not `h2` directly), mirroring 04.3's `http1-echo-server` consuming `envoy_http1` (not `httparse` directly). This keeps the architectural rule "only `envoy-http2` depends on `h2`" enforced at all dependency levels including test helpers.

8. **`drive_http2` carve-out.** The differential harness's `drive_http2` helper consumes `h2 = "0.4"` directly for low-level round-tripping (mirroring 04.x's `drive_http1` consuming `httparse` directly). This is a documented carve-out from the architectural rule, parallel to the phase 04.1 REVIEW M-architectural-claim posture for `httparse` in the differential harness.

9. **Body-bytes drain budget.** `h2::RecvStream` exposes body data via `data().await` returning `Option<Result<bytes::Bytes, h2::Error>>`. The planner decides at 05.2 Task 1 / 05.3 Task 1 whether to enforce a body-size cap (per 04.1's `BodyTooLarge` posture; the cap stays unbounded in 04.x for direct_response and bounded only by upstream behavior). For 05.2 fixture 0009 (direct_response GET with no body) and 05.3 fixture 0010 (deterministic-echo GET with deterministic body), no cap is needed. The planner adds a cap if a future fixture needs it.

10. **`x-envoy-upstream-service-time` header on H2 router responses.** The 04.3-landed allow-list row covers H2 too. The router H2-arm's measurement window is the same: `Instant::now()` immediately before `Client::connect`; `start.elapsed()` immediately after `send_request` returns the parsed response. The header is appended to the response by `write_proxied_response` (reused from 04.3 unchanged).

11. **Header name lowercasing.** H2 mandates lowercase header names on the wire (RFC 7540 §8.1.2). The `h2` crate enforces this — uppercase names cause a connection error. Envoy-rust's translation adapter at the H2 codec edge lowercases names defensively before handing off to `h2`. Symmetric on the inbound side: `h2::RecvStream` headers arrive lowercase already. This matches the 04.x posture (envoy-rust emits lowercase header names per parent-04 SPEC §3 architectural rule 2 / 04.1 SPEC's `headers.rs` constants).

12. **`:method`/`:path`/`:authority`/`:scheme` translation.** The H2-to-envoy-Request adapter in 05.2's `request.rs` reads these from the `http::Request<h2::RecvStream>` (where they're already separated into typed fields by `h2`); writes `Request.method` from `:method`, `Request.path` from `:path`, synthesizes a `Host: <authority>` row, and ignores `:scheme` (envoy-rust's HCM doesn't currently dispatch on scheme — that's a TLS-vs-plaintext concern, deferred). The H2 client adapter in 05.3's `client.rs` does the inverse: takes an envoy `Request` and synthesizes the pseudo-headers from `Request.method`, `Request.path`, the captured `host` (or the request's `Host:` header if explicit), and `:scheme: http` (since 05's posture is plaintext).

13. **`PRI` preamble handling.** `h2::server::handshake` expects the H2C `PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n` preamble at the start of the connection. Clients sending an HTTP/1.1 request to a `codec_type: HTTP2` listener get a connection-level error from `h2` (the handshake fails to detect the preamble). Envoy-rust does not byte-sniff to discriminate; it trusts the listener's configured `codec_type` and lets `h2` reject malformed connections at the codec layer. (AUTO byte-sniffing is a deferred non-goal per §4.)

14. **Cargo.lock sync cadence.** Per the established phase-precedent (phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85685a3`, phase-04.1 inline, phase-04.2 inline, phase-04.3 inline), the Cargo.lock sync lands inline with the dep-introducing task (05.1 introduces no new deps; 05.2's Task 1 introduces `h2` + transitive surface in `crates/envoy-http2/Cargo.toml`; 05.3's Task 1 introduces no new top-level deps). Phase-04.1 REVIEW M5 continues to carry forward to whichever phase ratifies a single cadence; 05 continues the inline-at-scaffold posture without new policy.

15. **`deny.toml` license allow-list.** The `h2` crate is dual-licensed MIT/Apache-2.0 (already on the allow-list). The `http` crate is dual-licensed MIT/Apache-2.0 (already on the allow-list). No `deny.toml` changes anticipated for 05.2 Task 1; the planner cross-checks `cargo deny check` output and lands an inline addition only if the transitive surface includes a new license.

16. **PLAN.md cadence.** Per the 04.3 close-out (M10 closed via the standalone pre-Task-1 PLAN.md commit `c02eea7` per parent-05 STATE.md handoff), each sub-phase's planner commits PLAN.md cleanly at state-2 close-out, before any Task 1 commit. The 04.1/04.2 inline-PLAN deviation is no longer the precedent.

17. **Fixture 0009 and 0010 use `STRICT_DNS` for the upstream cluster.** Even though 05.1's preamble lands `STRICT_DNS`, fixtures 0009 (which has no upstream) and 0010 (which has an H2C upstream at `host.docker.internal`) participate in the C-1 fix posture. Fixture 0010 declares `type: STRICT_DNS` at writeup time.

18. **In-process integration backstops.** Phase 05's two new fixtures gain in-process backstops at `crates/envoy-bin/tests/{http2_direct_response,http2_router_upstream}.rs` per the 04.3 D14 / 04.1 D4 posture. Each backstop spawns envoy-bin as a subprocess via `CARGO_BIN_EXE_envoy-bin`, drives the request via `h2::client`, asserts on the parsed response. The Docker-gated tests at `tests/differential/tests/{http2_direct_response,http2_router_upstream}.rs` are CI-only.

19. **`anyhow` boundary** at envoy-bin's integration tests and the differential harness. Tests in `crates/envoy-bin/tests/*` are in the binary crate's package and may use `anyhow` per D-3.2. The `tests/differential/` crate continues `anyhow::Result<()>` returns on `drive_http2` for consistency with 04.x's `drive_http1` posture.

20. **Phase-04 fixture YAMLs precedent for HCM filter naming.** 04.1+04.3 fixtures use `static_resources.listeners[0].filter_chains[0].filters[0]` of name `envoy.filters.network.http_connection_manager` with the HCM's `typed_config` carrying the route_config inline. Phase-05 fixtures 0009/0010 inherit this exactly, only changing `codec_type` from `HTTP1` to `HTTP2` and (for 0010) adding `typed_extension_protocol_options` to the cluster.

21. **Phase-05 ADR ledger projection.** Per §7 below, phase-05 lands ADR-0022 (split decision) at parent-05 state-2; ADR-0023 (`STRICT_DNS` cluster type) at 05.1 Task 1. ADR-0024 (`http` crate typed-surface scoping) and ADR-0025 (`h2spec` integration posture) are conditional and may not land. The DECISIONS.md ledger head is currently **ADR-0021** (last landed in 04.2 Task 1 commit `984aedd`); phase-05's projected ADRs land at the next-sequential numbers.

22. **HCMConfig polymorphism over codec.** The existing `envoy_http1::HCMConfig` is the per-listener immutable config. Phase 05's `envoy_http2::HCM` uses the same config struct (re-exported as `envoy_http2::HCMConfig` for ergonomic naming, or imported directly — the planner picks at 05.2 Task 1). The dispatch-by-codec lives at the listener-walk site in `envoy-bin/src/main.rs`, not at the HCMConfig level.

23. **`http1-echo-server` and `http2-echo-server` interop.** Both helpers exist in-tree at 05 close. Phase 05 fixtures only use `http2-echo-server` (0010); 04.3 fixtures use `http1-echo-server` (0008). Whichever phase first ships a cross-protocol fixture (e.g., H2 downstream → H1 upstream cluster) would mix the two. Out of scope for 05.

---

## 7. ADRs expected from this phase

**ADR-0022 — Split phase 05 into 05.1 + 05.2 + 05.3.** Lands at parent-05 state-2 (mirrors phase-04's `1d9740d`-shape state-2 commit landing ADR-0020 + sub-phase SPECs; mirrors phase-03's `f256d2c`-shape state-2 commit landing ADR-0017 + sub-phase SPECs). Provenance footer cites the brainstorm scope decisions: (i) C-1 fixture-hardening preamble in-scope as 05.1; (ii) plaintext H2C prior-knowledge for both downstream and upstream (TLS+ALPN deferred per §4); (iii) two H2 fixtures (0009 + 0010); (iv) `h2spec` ≥95% pass with catalogued failures targeting 100%; (v) `server_name` HCM field re-deferred. Options considered: (a) single phase (rejected, ~3000 LoC over §6.1 gates by ~100%); (b) two-way split bundling preamble with downstream (rejected, 05.1 at ~1700 LoC at the upper edge of the gate, insufficient headroom for execution-time drift per phase-04's experience); (c) three-way flat split (chosen). Decision: split into 05.1 (`fixture-hardening`), 05.2 (`http2-downstream`), 05.3 (`http2-upstream`). Rationale: each sub-phase fits comfortably under §6.1 gates with ~13% headroom; failure attribution localized; the standalone fixture-hardening sub-phase produces a clean REVIEW.md as an audit artifact for the cross-phase C-1 fix; mirrors the phase-04 three-way precedent under ADR-0020.

**ADR-0023 — `ClusterType::StrictDns` accepted; `LOGICAL_DNS` deferred.** Lands at 05.1 Task 1 (mirrors the phase 04.2 ADR-0021 inline-landing pattern; mirrors the phase 03.1 Task 1 ADR-0018+0019 inline-landing pattern). Provenance footer cites the C-1 cross-phase regression trace and the 04.3 REVIEW §3/§4 carryforward. Scope: extends the `ClusterType` enum from single-variant `Static` to `Static | StrictDns`; the validator accepts `STRICT_DNS` and resolves DNS names at cluster-build time via `tokio::net::lookup_host`; the implementation does NOT cover `LOGICAL_DNS` (which differs from `STRICT_DNS` only in re-resolution semantics — `STRICT_DNS` caches results at build time; `LOGICAL_DNS` re-resolves per-request). Rationale: `STRICT_DNS` is the simpler, more common case and is sufficient for the C-1 fix (`host.docker.internal` resolves locally via Docker's `host-gateway` and doesn't need per-request re-resolution). Future phases that need `LOGICAL_DNS` add it then. Consequences: `crates/envoy-config/src/bootstrap.rs::ClusterType` gains `StrictDns` variant; `crates/envoy-cluster/src/cluster.rs::Cluster::new` gains a `STRICT_DNS` resolution branch; `crates/envoy-config/src/lib.rs` gains `ConfigError::ClusterDnsResolutionFailed` variant; 5 fixture YAMLs flip cluster type. Closes phase-02.1 REVIEW I3 (positive `Static` regression guard). Phase-04.3 REVIEW §3 / §4 C-1 closes at this ADR's landing commit. Phase-04.1 REVIEW M-claim (drive_http1 per-function unit test masked by Docker-gated regression) is unblocked by this ADR's fixture-hardening though M-claim itself stays deferred per the 04.3 disposition.

**ADR-0024 (CONDITIONAL) — `http` crate (`http::Request`/`http::Response`/`http::HeaderMap`) typed-surface scoping.** Lands at 05.2 Task 1 IF the planner determines that `http` belongs as a direct dep on `crates/envoy-http2/Cargo.toml` (vs. transitive only via `h2`'s public API surface). The `h2` crate exposes `http::*` types as part of its API — there's no way to use `h2` without touching `http` types at the codec edge. The narrow scope question is whether to record this as a permitted-foundation grant for `http` directly (parallel to ADR-0021's narrow scoping for `regex`), or to treat it as transitive-only with no ADR. Recommendation: lands as a brief ADR acknowledging the transitive surface. Decision deferred to 05.2 Task 1.

**ADR-0025 (CONDITIONAL) — `h2spec` integration posture.** Lands at 05.2 Task 1 IF the `h2spec` runner integration surfaces a non-trivial doctrine choice (e.g., binary provisioning: Docker-image vs. apt-package vs. curl-tar; known-failures format; gate-mechanics for partial-pass). The brainstorm anticipates this is mostly mechanical and may not warrant an ADR; if the planner finds the gate-mechanics warrant policy-grade documentation, ADR-0025 records the call.

**Possible additional ADRs** (not anticipated but listed for projection completeness):

- **ADR-0026 (or later) — H2-specific allow-list rows for BEHAVIOR_CONTRACT.md** if 05.2 or 05.3 surface response-header divergences not covered by the existing 3 phase-04 rows. Likely unnecessary — the analysis in §2 above suggests the existing rows cover the H2 surface uneventfully.

- **ADR-0026 (or later) — H2 trailers handling posture** if a planner-time decision is forced by an unexpected interaction between the `h2` codec and the helper or fixture. Likely unnecessary — trailers are an explicit non-goal per §4.

If any of these fire, they take the next-sequential available ADR number at the time they land. Sub-phase planners may also find the need for sub-phase-local ADRs; those land at the relevant sub-phase Task-N commit per D-3.5.

---

## 8. Artifacts this phase produces

Created during execution (relative to repo root), spanning all three sub-phases:

- `docs/envoy-rust/phases/05-http2/SPEC.md` (this document; lands at parent-05 state-1 — this commit).
- `docs/envoy-rust/phases/05.1-fixture-hardening/SPEC.md` (sub-phase SPEC; lands at parent-05 state-2 alongside ADR-0022).
- `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md` (sub-phase SPEC; lands at parent-05 state-2).
- `docs/envoy-rust/phases/05.3-http2-upstream/SPEC.md` (sub-phase SPEC; lands at parent-05 state-2).
- Each sub-phase additionally produces its own `PLAN.md`, `PROGRESS.md`, `REVIEW.md`.
- `crates/envoy-http2/Cargo.toml` (05.2)
- `crates/envoy-http2/src/lib.rs` (with `#![forbid(unsafe_code)]`) (05.2)
- `crates/envoy-http2/src/{codec,hcm,request,response,error}.rs` (05.2; module decomposition decided at 05.2 SPEC writeup)
- `crates/envoy-http2/src/client.rs` (05.3)
- `tests/conformance/h2spec/Cargo.toml` (05.2)
- `tests/conformance/h2spec/src/lib.rs` or `tests/conformance/h2spec/tests/h2spec_runner.rs` (05.2; runner shape at 05.2 SPEC writeup)
- `tests/conformance/h2spec/known-failures.txt` (05.2; populated at 05.2 task time)
- `tests/helpers/http2-echo-server/Cargo.toml` (05.3)
- `tests/helpers/http2-echo-server/src/main.rs` (05.3; with `#![forbid(unsafe_code)]`)
- `crates/envoy-bin/tests/http2_direct_response.rs` (05.2; in-process integration backstop)
- `crates/envoy-bin/tests/http2_router_upstream.rs` (05.3; in-process integration backstop)
- `tests/differential/tests/http2_direct_response.rs` (05.2; Docker-gated)
- `tests/differential/tests/http2_router_upstream.rs` (05.3; Docker-gated)
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml` (05.1)
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml` (05.2)
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml` (05.3)
- `tests/fixtures/0009-http2-direct-response/{envoy.yaml,envoy-rust.yaml,inputs/payload.bin,expectations.yaml,README.md}` (05.2)
- `tests/fixtures/0010-http2-router-upstream/{envoy.yaml,envoy-rust.yaml,inputs/payload.bin,expectations.yaml,README.md}` (05.3)

Amended during execution:

- Root `Cargo.toml` — `[workspace] members` gains `crates/envoy-http2` (05.2), `tests/conformance/h2spec` (05.2), `tests/helpers/http2-echo-server` (05.3).
- `crates/envoy-config/src/bootstrap.rs` — substantial schema additions across all three sub-phases (`ClusterType::StrictDns` in 05.1; `CodecType::HTTP2` accept + listener-side `Http2ProtocolOptions` in 05.2; cluster-side `Http2ProtocolOptions` via `typed_extension_protocol_options` in 05.3; `Cluster.upstream_protocol` field in 05.3).
- `crates/envoy-config/src/lib.rs` — re-exports + new `ConfigError` variants across all three sub-phases (`ClusterDnsResolutionFailed` in 05.1; `Http2OverTlsNotSupported` + `Http2ProtocolOptionsOutOfRange` in 05.2; `MutuallyExclusiveExplicitHttpConfig` in 05.3).
- `crates/envoy-cluster/src/cluster.rs` — `Cluster::new` extension for `STRICT_DNS` resolution (05.1); `Cluster.upstream_protocol` field (05.3).
- `crates/envoy-bin/src/main.rs` — HCM-on-H2 dispatch arm (05.2); router H2-arm dispatch (05.3).
- `tests/differential/src/lib.rs` — `Driver::Http2` variant + `drive_http2` helper + `{{HTTP2_BACKEND_PORT}}` template marker (05.2/05.3).
- `tests/differential/src/backend.rs` — `Http2EchoBackend` + `locate_http2_echo_server` (05.3).
- `tests/fixtures/0003-tcp-proxy/{envoy.yaml,envoy-rust.yaml}` (05.1; STRICT_DNS flip)
- `tests/fixtures/0004-tls-downstream/{envoy.yaml,envoy-rust.yaml}` (05.1; STRICT_DNS flip)
- `tests/fixtures/0005-tls-upstream/{envoy.yaml,envoy-rust.yaml}` (05.1; STRICT_DNS flip)
- `tests/fixtures/0006-tls-sni/{envoy.yaml,envoy-rust.yaml}` (05.1; STRICT_DNS flip)
- `tests/fixtures/0008-http1-router-upstream/{envoy.yaml,envoy-rust.yaml}` (05.1; STRICT_DNS flip)
- `docs/envoy-rust/DECISIONS.md` — ADR-0022 (parent-05 state-2) + ADR-0023 (05.1 Task 1) + possibly ADR-0024 (05.2 Task 1) + possibly ADR-0025 (05.2 Task 1).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — no edits anticipated (per §2 above; the existing 3 phase-04 allow-list rows cover H2; row 4 `HTTP/2 framing` is engaged structurally without contract changes). If 05.2 or 05.3 surface unexpected response-header divergences, the contract grows in lockstep with the in-code allow-list constant.
- `docs/envoy-rust/ROADMAP.md`:
  - **At parent-05 state-1 (this commit):** row `05` `status` `planned` → `in-progress`. Add `sub-phases: 05.1, 05.2, 05.3` column.
  - **At parent-05 state-2:** add ROADMAP rows `05.1`, `05.2`, `05.3` (each `status: planned` initially; the ROADMAP-schema invariant 3 flips them to `in-progress` as STATE.md points at each).
  - **At each sub-phase state-6 phase-done commit:** that sub-phase's row flips `in-progress` → `done`; the **last** sub-phase commit (05.3's) ALSO flips parent row `05` `in-progress` → `done` per the ROADMAP-schema invariant ("parent flips to `done` only after all sub-phases are `done`").
- `docs/envoy-rust/STATE.md`:
  - **At parent-05 state-1 (this commit):** advance from `phase 05 lifecycle state 1` to `phase 05 lifecycle state 2 (parent SPEC.md exists; sub-phase SPECs do not)`. Next-skill: `superpowers:writing-plans` for the split-output (parent state-2 lands ADR-0022 + sub-phase SPECs; not a single PLAN.md but the equivalent in split-shape).
  - At each sub-phase transition, STATE.md advances per the standard lifecycle.
- `Cargo.lock` — synced inline with each dep-introducing task per the established phase-precedent (M5/M9 carryforward continues unchanged; phase 05 continues the inline-at-scaffold cadence).
- `deny.toml` — likely no-op at 05.2 Task 1 (`h2`, `http`, and their transitive deps are already on the allow-list per project-wide cargo-deny posture). The planner cross-checks `cargo deny check` output and lands an inline addition only if the transitive surface includes a new license.

Not touched in phase 05 (belong to earlier phases or are frozen):

- `crates/envoy-tls/`, `crates/envoy-tcp/`, `crates/envoy-http1/`, `tests/helpers/{tcp,tls,http1}-echo-server/` — finalized in earlier phases; phase 05 consumes via existing public APIs. (The 05.2 HCM-on-H2 imports `envoy_http1::HCMConfig` + the route-walk + the router invocation site; this is consumption, not modification.)
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0007-http1-direct-response/` — unedited; their fixtures must remain green at each sub-phase state-4 gate.
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.

---

## 9. Final commit message format (for parent-05 state-6 commit, landed at sub-phase 05.3's phase-done commit)

The parent-05 phase-done commit lands at 05.3's state-6 commit (mirrors phase 04's `e626862`-shape close-out where the 04.3 commit also closed parent 04). Format:

```
phase 05.3: HTTP/2 upstream origination + router H2-arm + fixture 0010 [parent 05 done]

(05.3-specific summary covering envoy-http2::Client, the router's H2-arm
dispatch for upstream-protocol-aware proxy, http2-echo-server helper, fixture
0010, harness Http2EchoBackend, parent-05 close.)

Closes parent phase 05 (HTTP/2 cleartext data plane). Sub-phases:
- 05.1 (commit <SHA>): ClusterType::StrictDns + 5-fixture coordinated edit + I3 close [ADR-0023].
- 05.2 (commit <SHA>): envoy-http2 codec + HCM-on-H2 + fixture 0009 + h2spec ≥95% gate.
- 05.3 (this commit): upstream H2C origination + router H2-arm + fixture 0010 + http2-echo-server.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (restored by 05.1 STRICT_DNS fix);
  tests/fixtures/0004-tls-downstream green (restored by 05.1);
  tests/fixtures/0005-tls-upstream green (restored by 05.1);
  tests/fixtures/0006-tls-sni green (restored by 05.1);
  tests/fixtures/0007-http1-direct-response green (unchanged);
  tests/fixtures/0008-http1-router-upstream green (restored by 05.1);
  tests/fixtures/0009-http2-direct-response green (NEW; HTTP/2 listener;
    direct_response action under H2 framing);
  tests/fixtures/0010-http2-router-upstream green (NEW; HTTP/2 downstream
    proxied through to http2-echo-server via H2 upstream cluster).
Conformance: tests/conformance/h2spec at ≥95% pass; failing tests catalogued
  in tests/conformance/h2spec/known-failures.txt with one-line doctrine
  reasons; cross-referenced in 05.2 REVIEW §4.
```

The parent-05 state-6 commit also flips ROADMAP rows `05` and `05.3` to `done` (rows `05.1` and `05.2` flipped at their own state-6 commits earlier in the phase). STATE.md advances to phase `06` lifecycle state 1; next-skill `superpowers:brainstorming` scoped to phase 06 ("Access log (file sink, Envoy default format) + stats + Prometheus admin endpoint" per `BOOTSTRAP_PROMPT.md` §8 row 06). Phase-05's projected ADR ledger (ADR-0022 + ADR-0023, possibly ADR-0024/0025) is closed; phase-06's projected ADRs land at the next-sequential numbers.
