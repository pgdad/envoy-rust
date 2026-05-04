# Phase 05.3 REVIEW — upstream HTTP/2 cleartext: `envoy-http2::Client` + cluster-side `Http2ProtocolOptions` + `Cluster.upstream_protocol` + router H2-arm + fixture 0010 + parent-05 close-out

- **Phase id:** `05.3`
- **Slug:** `05.3-http2-upstream`
- **Reviewed range:** `f33dac9..53ac466` (30 files, ~8018 insertions / ~109 deletions across 20 commits — pre-Task-1 PLAN.md commit `4b92e05` + 12 substantive task commits + 4 progress-note / review-fixup / state-4 / fixup commits).
- **CI evidence cited:** local state-4 phase-done verification at HEAD `83e4da7` (per PROGRESS.md Task 12). The `CI run URL` field in PROGRESS.md Task 12 (a) is the literal placeholder string `<CI run URL TBD by controller after push>` — see §6 below.
- **Reviewed:** 2026-05-04.
- **Verdict:** **Approved with M-track follow-ups** — state 5 complete. Zero Critical findings. Three Important findings (I1 H1-listener-with-H2-cluster path is structurally unreachable per ADR-0028 and not flagged on a runtime path; I2 upstream dispatch errors are stringified at the dispatch site, which silently dissolves the typed error chain; I3 Task 12 PROGRESS gate evidence is a local run with the CI run URL still a placeholder). Eight Minor findings (M1–M8). The verdict matches phase 05.4, 05.2, 05.1, 04.3 / 04.2 / 04.1 — all of parent-05's siblings landed at "Approved with M-track follow-ups." 05.3's deliverables are functionally complete and the architectural invariants hold; the Important findings are scoped (I1 doctrinal/cycle-resolution; I2 ergonomic; I3 evidence-discipline) and don't gate state-6 close-out, but each warrants 06+ disposition.

---

## §1 Summary

Phase 05.3 lands the project's first end-to-end HTTP/2-on-HTTP/2 round trip and closes parent phase 05:

- New `envoy-http2::client` module (`crates/envoy-http2/src/client.rs`, 548 LoC: ~180 impl + ~340 tests + helpers) shipping a per-connection plaintext H2C `Client` + `ClientStream`. `Client::connect` runs a TCP handshake → `h2::client::handshake` → biased `tokio::select!` against a 10ms-window-then-spawn pattern that detects an immediate `h2::client::Connection` failure (e.g., HTTP/1.1-responding bad server) before the connection task is fire-and-forget-spawned (`crates/envoy-http2/src/client.rs:42-61`); `ClientStream::send_request` performs a 7-step translate-strip-send-drain pipeline against `envoy_http1::codec::Request`/`Response` value types per cross-sub-phase architectural rule 2.
- `Http2Error` extension at `crates/envoy-http2/src/error.rs:59-99` adds 4 client-side variants (`UpstreamConnect`, `H2ClientHandshake`, `H2SendRequest`, `H2RecvBody`) with per-variant Display tests; the 6 codec-side variants from 05.2 D3 stay byte-identical per SPEC §3 D1.
- Cluster-side `typed_extension_protocol_options` schema (`crates/envoy-config/src/bootstrap.rs:121-162` + `:1043-1063`) — 4 new types (`TypedExtensionProtocolOptions`, `HttpProtocolOptions`, `ExplicitHttpConfig`, `Http1ProtocolOptions`); 2 new `ConfigError` variants (`MutuallyExclusiveExplicitHttpConfig`, `UnsupportedTypedConfigUrl`); shared `validate_http2_protocol_options_ranges` free function hoisted from `validate_hcm` so listener-side and cluster-side ranges are guaranteed to drift in lockstep.
- Fuzz corpus seed `cluster_http2_protocol_options.yaml` (49 LoC) exercises the new accept-path; corpus-walk acceptance test appended.
- `envoy-cluster::UpstreamProtocol { Http1 (default), Http2 }` enum + `Cluster.upstream_protocol` field set at `from_bootstrap` time (`crates/envoy-cluster/src/cluster.rs:166-174` + `:270-280`), with 3 dedicated unit tests (`cluster_upstream_protocol_defaults_to_http1`, `cluster_upstream_protocol_http2_set_from_typed_extension_protocol_options`, `cluster_upstream_protocol_http1_set_from_explicit_http1_options`).
- ADR-0028 (`docs/envoy-rust/DECISIONS.md:513-530`) — the only ADR landed in 05.3, documenting the unanticipated `envoy-http1` ↔ `envoy-http2` Cargo cycle that surfaces if `envoy-http2` is added as a path-dep of `envoy-http1` (which would be required for the symmetric H1-listener-side dispatch SPEC §3 D4 originally projected). Decision: option (B) — defer the H1-listener-side dispatch; ship only the H2-listener-side dispatch at Task 7. **The 05.3 SPEC §7 explicitly projected zero ADRs; ADR-0028 is a documented in-execution deviation per D-3.5.**
- Symmetric H1-or-H2 dispatch on the H2-listener-side at `crates/envoy-http2/src/hcm.rs:118-230` — replaces 05.2's 502 stub keyed on `cluster.upstream_protocol()`. Closes 05.2 REVIEW M8 structurally (the literal stub body `b"upstream H2 not yet wired (sub-phase 05.3)\n"` is gone).
- New workspace member `tests/helpers/http2-echo-server/` (340 LoC) — the H2C sibling of `tcp-echo-server` / `tls-echo-server` / `http1-echo-server`. Argv shape mirrors `http1-echo-server` verbatim; consumes `envoy_http2` for the handshake (via the new thin `crates/envoy-http2/src/codec.rs::server_handshake` wrapper) so the architectural invariant "only `envoy-http2` deps on `h2`" stays enforced at all dependency levels including test helpers.
- Differential harness `Http2EchoBackend` (`tests/differential/src/backend.rs:255-310`) + `wait_h2_accept_ready` 2s-budget poll (vs Http1EchoBackend's 1s; H2 handshake adds the SETTINGS exchange round-trip) + `locate_http2_echo_server`. `run_fixture` cascade extended with `{{HTTP2_BACKEND_PORT}}` template marker (`tests/differential/src/lib.rs:1019-1101`).
- Fixture 0010 (`tests/fixtures/0010-http2-router-upstream/`) — the project's first H2C-on-H2C end-to-end fixture: `codec_type: HTTP2` listener proxying via `STRICT_DNS` cluster with `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}`. 5-file fixture (envoy.yaml + envoy-rust.yaml + inputs/payload.bin (0 bytes) + expectations.yaml + README.md) + Docker-gated wrapper.
- In-process integration backstop at `crates/envoy-bin/tests/http2_router_upstream.rs` (207 LoC) — non-Docker, parallel to 05.2's `http2_direct_response.rs` and 04.3's `http1_router_upstream.rs`. Uses `h2 = "0.4"` as `[dev-dependencies]` only.

State-4 phase-done verification per PROGRESS Task 12 reports all signals (a)–(e) GREEN locally; signal (c) (h2spec) deferred to CI per the absence of a local h2spec install. Cargo.lock diff is 15 lines (the new `http2-echo-server` workspace-member registration only — no new top-level deps; M5/M9 carryforward continues unchanged). `cargo deny check` is clean with the same 5 pre-existing license-not-encountered advisory-only warnings carried from the 05.2 baseline.

The architectural rule "`envoy-http2` is the sole workspace dep on `h2`" is preserved at runtime: empirical verification by `grep -rn 'use h2\|h2::' crates/` returns no `envoy-http*` matches outside `crates/envoy-http2/` itself; the documented carve-outs are (a) `tests/differential/Cargo.toml:14` `[dependencies]` for the `drive_http2` helper (per parent-05 SPEC §6 signpost 8), (b) `tests/helpers/http2-echo-server/Cargo.toml:22-29` `[dependencies]` for the helper's accept-loop (per parent §6 signpost 7's documented "h2 types leak via the return value" caveat), (c) `crates/envoy-bin/Cargo.toml:29` `[dev-dependencies]` only for the in-process backstop. All three carve-outs have inline doc-comment justifications.

---

## §2 Strengths

1. **Architectural rule 1 ("`envoy-http2` is the sole workspace dep on `h2`") is preserved at runtime.** Empirical grep verification: no production crate other than `envoy-http2` imports `h2::*`. The three carve-outs (`tests/differential` `[dependencies]`, `tests/helpers/http2-echo-server` `[dependencies]`, `crates/envoy-bin` `[dev-dependencies]`) are scoped, signposted, and inline-justified. The `crates/envoy-bin/Cargo.toml:29` `h2 = "0.4"` entry is dev-only — production envoy-bin code does not link `h2`. This is a non-trivial discipline outcome at this phase boundary because the H2 client code lands in 05.3.

2. **The `envoy-http1` ↔ `envoy-http2` Cargo cycle is correctly identified, documented, and resolved (without scope creep).** ADR-0028 (`docs/envoy-rust/DECISIONS.md:513-530`) walks through three options — (A) trait-object hoist (~250 LoC, restructures 4 crates), (B) defer H1-listener H2-arm (~50 LoC, doc-only), (C) hoist `Client` into `envoy-http-client` (~400 LoC, restructures 3 crates) — and picks (B) with explicit rationale: 05.3's only new fixture is H2-listener × H2-cluster, which option (B) covers; the H1-listener × H2-cluster combination has zero 05.3 fixture demand. The ADR's "Consequences" block names the concrete combinatorial gap (H1-listener can proxy only to H1 clusters; H2-listener can proxy to either H1 or H2 clusters) and identifies the carryforward landing site (a later phase that lands either (A) or (C)). This is exemplary ADR discipline.

3. **The H2-listener-side symmetric dispatch is structurally correct.** `crates/envoy-http2/src/hcm.rs:118-230` keys on `cluster.upstream_protocol()` and dispatches into `envoy_http1::Client::connect` for `Http1` clusters and `crate::Client::connect` for `Http2` clusters. The cycle-free dispatch path works because `envoy-http2` already path-deps `envoy-http1` per 05.2 Task 1 (the inverse direction is the cycle). Two unit tests (`h2_proxy_outcome_dispatches_to_upstream`, `h2_proxy_outcome_dispatches_to_h1_upstream_when_cluster_is_http1`) exercise both arms with in-process upstream servers.

4. **The H2 client's handshake-failure detection is genuinely defensible** despite the 10ms latency cost on bad upstreams. `Client::connect` at `crates/envoy-http2/src/client.rs:42-61` uses `Box::pin(connection)` + `biased` `tokio::select!` against a 10ms `tokio::time::sleep` to detect immediate H2 connection failure (e.g., upstream responding with HTTP/1.1) before fire-and-forget-spawning the connection task. The PROGRESS Task 2 deviation note (lines 75–81) explicitly acknowledges this is a deliberate deviation from PLAN's "fire-and-forget directly" projection and explains the reasoning (h2's `client::handshake` returns before the server's SETTINGS frame arrives, so a bad-server scenario only manifests when the connection future is driven). The 8th unit test (`send_request_maps_h2_handshake_failure_to_typed_error` at `client.rs:526-547`) drives an HTTP/1.1-responding listener and asserts the typed `H2ClientHandshake` outcome — closes a real correctness gap.

5. **`Http2Error` 4 new client-side variants are RFC-symmetric with the listener-side.** `crates/envoy-http2/src/error.rs:59-99` — `H2ClientHandshake` is the dual of `H2Handshake`; `H2RecvBody` is the dual of `H2BodyRead` (one is response-side, the other request-side); `H2SendRequest` covers the new send-stream / response-future failure surface; `UpstreamConnect` is the sibling of `envoy_http1::Http1Error::UpstreamConnect`. The 4 new variants each carry a Display-with-source unit test (`error.rs:138-181`).

6. **The 7-step `ClientStream::send_request` pipeline is correct, defensive, and well-documented.** `crates/envoy-http2/src/client.rs:119-219` carries a 6-paragraph doc-comment naming each step (a)–(g) and the failure-site → variant mapping at the end. The implementation matches the doc: (a) explicit `Host:` wins over captured host (test 4 verifies); (b) absolute-form URI synthesizes `:scheme: http`; (c) lowercases all names + skips `Host:` + strips H2-forbidden hop-by-hop names defensively; (d) `end_of_stream=true` on HEADERS for empty bodies, otherwise HEADERS+DATA(end=true); (e)/(f)/(g) drain + translate to `Response`. Test 7 `send_request_strips_h2_forbidden_hop_by_hop_headers` exercises 5 forbidden names + 1 preserved name end-to-end.

7. **Cluster-side typed_extension_protocol_options validation is complete.** `crates/envoy-config/src/bootstrap.rs:1043-1063` rejects unknown `@type` URLs (`UnsupportedTypedConfigUrl`), rejects mutually-exclusive `explicit_http_config` (`MutuallyExclusiveExplicitHttpConfig`), and delegates range checks to the hoisted `validate_http2_protocol_options_ranges` free function (`bootstrap.rs:1347-1383`). The hoist is load-bearing: it guarantees listener-side and cluster-side range checks stay drift-free as a single source of truth. 7 dedicated unit tests cover the accept-path (HTTP1 default, HTTP2 explicit, HTTP1 explicit), the validator-reject paths (mutually-exclusive, unsupported `@type` URL), and the corpus-walk acceptance.

8. **The `Cluster.upstream_protocol` field is set at construction time, not derived per-call.** `crates/envoy-cluster/src/cluster.rs:270-280` projects the field from `cfg.typed_extension_protocol_options` at `from_bootstrap` time using a deterministic match arm. The "both Some" case (mutually-exclusive H1+H2) is validator-rejected upstream of `from_bootstrap`; the cluster code defaults defensively to `Http1` if both are unexpectedly present. Per parent §6 signpost 5's recommendation. The 3 unit tests (`cluster_upstream_protocol_defaults_to_http1`, `..._http2_set_from_typed_extension_protocol_options`, `..._http1_set_from_explicit_http1_options`) cover all three logical projection cases.

9. **The `validate_http2_protocol_options_ranges` hoist is genuinely shared, not duplicated.** PROGRESS Task 3 deviation note 1 calls out the planner's discipline: instead of copying the let-chain block from PLAN's pseudocode, the actual implementation was extracted verbatim from the live `validate_hcm` body (preserving the let-chain idiom and local consts). Re-grep at task time over reading the PLAN.

10. **The `http2-echo-server` helper consumes `envoy_http2` over direct `h2`.** `tests/helpers/http2-echo-server/src/main.rs:128` calls `envoy_http2::codec::server_handshake(tcp).await` instead of `h2::server::handshake` directly. The new wrapper `crates/envoy-http2/src/codec.rs:42-48` is the documented thin re-export that satisfies the architectural invariant. The helper's `[dependencies.h2]` block (`tests/helpers/http2-echo-server/Cargo.toml:22-29`) carries an inline 5-line justification for the unavoidable carve-out (the wrapper's return type is `h2::server::Connection<TcpStream, Bytes>`, so the helper's accept loop reaches `h2::server::*` types via the wrapper's return value — same shape as `tests/differential`'s `drive_http2`).

11. **The `Http2EchoBackend` accept-readiness probe is H2-shape-aware.** `tests/differential/src/backend.rs:312-338` runs both TCP-connect and `h2::client::handshake` inside the polling loop, with a 2-second budget (vs Http1EchoBackend's 1s). This means "ready" really means "the helper has completed its H2 codec setup" — not "the kernel accepted a TCP SYN." Distinct from `wait_accept_ready` (TCP-only). The 4 unit tests (`http2_echo_backend_spawns_and_echoes`, `http2_echo_backend_drop_terminates_child`, `locate_http2_echo_server_returns_existing_path`, `run_fixture_dispatches_http2_backend_on_template_marker`) cover the spawn round-trip, drop posture, locator path-construction, and template-marker dispatch.

12. **05.2 REVIEW M8 closure is structural, not nominal.** The 05.2 stub literal `b"upstream H2 not yet wired (sub-phase 05.3)\n"` is gone — verifiable by `grep -n 'upstream H2 not yet wired' crates/envoy-http2/src/hcm.rs` returning zero results (PROGRESS Task 7 documents this). The replacement is a real H1-or-H2 dispatch keyed on `cluster.upstream_protocol()`. The renamed test `h2_proxy_outcome_dispatches_to_upstream` (formerly `h2_proxy_outcome_returns_502_in_05_2`) flips the assertion from 502 to 200 with a real upstream H2 server.

13. **The `H2_FORBIDDEN_HOP_BY_HOP` constant is consolidated.** Per Task 2 review I2 (the 05.2 review I2 differs — this is the in-phase Task-2 review I2, separate scope). PROGRESS Task 7 records the consolidation: per-module duplicates in `client.rs` and `response.rs` were removed; both modules now reference a single canonical `pub(crate) const` at `crates/envoy-http2/src/lib.rs:34-40` with a 5-line doc-comment naming the RFC sources (RFC 7540 §8.1.2.2 + RFC 9113 §8.2.2).

14. **Fixture 0010's expected_body matches the H2 echo helper's deterministic output exactly.** The expected body string at `tests/fixtures/0010-http2-router-upstream/expectations.yaml:9` is `"method: GET\npath: /\nheaders:\n  :authority: envoy-rust.test\n  :method: GET\n  :path: /\n  :scheme: http\nbody: "` — the alphabetic-sort of pseudo-headers matches `tests/helpers/http2-echo-server/src/main.rs:184-228`'s `make_response_body`'s implementation, where lowercased pseudo-headers (`:authority`, `:method`, `:path`, `:scheme`) sort before any non-pseudo headers (none in this fixture). Cross-verified at `tests/helpers/http2-echo-server/src/main.rs:217` (`sorted_headers.sort_by(|a, b| a.0.cmp(&b.0))`).

15. **PROGRESS.md is exemplary self-narrating.** Every task carries Files-Modified, LoC, Verification, Verified-shapes-from-greps-run-at-task-time, Deviations-from-PLAN, and Carryforward sections. Task 2's "deviations" section is particularly substantive (4 numbered deviations with full reasoning). Task 7's "deviations" enumerates the YAML-vs-direct-construction decision for the cluster-mgr test helper. The fixup commit `53ac466` exists specifically to fill in commit SHA `83e4da7` in the Task 12 `Commit:` field — a self-correction cadence that's worth noting (see §6 below).

16. **The `[parent 05 done]` close-out artifact wiring is well-prepared.** STATE.md `Notes` infrastructure is in place (per §8 of SPEC); the ROADMAP rows 05 / 05.1 / 05.2 / 05.3 / 05.4 are at expected pre-close-out states (`grep -c '^| 05' docs/envoy-rust/ROADMAP.md → 5`); ADR-0028 is the ledger head; the SPEC §9 commit message format is well-specified. The state-6 close-out commit can land mechanically.

---

## §3 Issues

### Critical

None.

### Important

**I1. ADR-0028 documents a partial-D4 deferral but the H1-listener-with-H2-cluster combination is not flagged as user-visible behavior.** ADR-0028 (`docs/envoy-rust/DECISIONS.md:513-530`) cleanly walks options and chooses option (B). However: at runtime, an operator who configures an H1 listener pointed at a cluster carrying `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options` will silently get an H1-only dispatch at `crates/envoy-http1/src/hcm.rs:260-293` — the existing 04.3-landed `Client::connect` call site is the unedited H1-only path. The cluster's `upstream_protocol` field is set to `UpstreamProtocol::Http2` per `from_bootstrap`, but `crates/envoy-http1/src/hcm.rs` does not read it. This is a silent protocol-misnegotiation: Envoy's behavior would be H2-on-the-wire to the upstream; envoy-rust's would be H1-on-the-wire. Validation does NOT reject this combination — the `envoy-config` validator at `bootstrap.rs:927+` does not gate `codec_type: HTTP1` listener × `typed_extension_protocol_options.http2_protocol_options` cluster. **The validator should reject this combination at parse time** (mirroring the existing `Http2OverTlsNotSupported` gate landed at 05.2 D2), with the diagnostic naming the cycle resolution / ADR-0028 deferral. **Fix sketch:** add a `ConfigError::Http2ClusterFromHttp1Listener { listener: String, cluster: String }` variant; in the validator's per-listener loop, if the listener's HCM `codec_type == HTTP1` (or `AUTO`) and any reachable cluster's `upstream_protocol == Http2`, raise the error. The reachability check can be conservative: scan all clusters referenced by the HCM's route_config virtual_hosts and reject if any are H2.

**Disposition:** carry forward to a 06+ phase (alongside the option (A) or (C) restructure that closes ADR-0028). The user-visible silent misnegotiation is the bigger gap; the validator gate is the cheap defense-in-depth.

**I2. The H2-listener dispatch site stringifies all upstream errors via `format!("{e}")`, dissolving the typed error chain.** `crates/envoy-http2/src/hcm.rs:167-180` reads:

```rust
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
```

The `format!("{e}")` calls drop the `#[source]` chain on both `Http1Error` (the H1 arm) and `Http2Error` (the H2 arm). Downstream the error is logged at line 185 via `tracing::warn!(error = %e, ...)` — which only displays the top-level error message; the underlying `h2::Error`'s reason code is gone, the `std::io::Error::kind()` is gone, and the diagnostic chain is collapsed to a string. This makes operational debugging strictly worse than the H1 path at `crates/envoy-http1/src/hcm.rs:262-268` (which logs `error = ?source` with the `Debug` chain intact). **Fix sketch:** define a small private sum type `enum DispatchError { Http1(envoy_http1::Http1Error), Http2(crate::Http2Error) }` and have both arms map to that; log it via `?` instead of `%e` at line 185. The two arms are structurally distinct error types so a typed sum is the natural shape; the H2 listener-side cluster_mgr dispatch is the only consumer.

**Disposition:** carry forward to 06+. Mechanical, scoped to one site, no public-API impact.

**I3. The state-4 phase-done gate evidence carries a placeholder CI run URL.** PROGRESS.md Task 12 line 448 reads literally `CI run URL: <CI run URL TBD by controller after push>`. The state-4 gate verification at `83e4da7` was run **locally only** — `cargo test --workspace` per-fixture timings and a 31-second local fuzz run. Acceptance signal (c) (h2spec ≥95%) is **deferred** (line 460: "h2spec binary not installed locally; ... CI run will exercise the full 144/146 = 99.31% baseline"). The fixup commit `53ac466` filled in the commit SHA `83e4da7` but did not fill in the CI run URL or the actual h2spec percentage from CI; the placeholder is preserved verbatim. By comparison, 05.2 PROGRESS Task 14 cites a real CI run URL `25294149612` HEAD `dac3f8b` with the actual h2spec count `144 passed / 1 failed / 1 skipped of 146 = 99.31%`. **The 05.3 gate evidence is local-only and incomplete.** This is a discipline regression vs 05.2's posture. The state-6 close-out commit should land with a real CI run URL filled into PROGRESS Task 12 (a) and (c). **Fix sketch:** before the state-6 close-out commit, push the branch to trigger CI, capture the run URL + h2spec line, and replace the placeholder in PROGRESS Task 12 (a) and (c) (or land the fix as a state-5b commit alongside REVIEW.md).

**Disposition:** must close before the state-6 phase-done close-out. Otherwise the [parent 05 done] commit lands with sub-phase evidence that says "TBD by controller after push" — a permanent ledger gap.

### Minor

**M1. The H2 listener dispatch path leaks `:scheme: http` in the H2 outbound's URI but the H1 dispatch arm receives `version: HttpVersion::Http11`.** `crates/envoy-http2/src/hcm.rs:157-164` constructs the outbound `Request` with `version: HttpVersion::Http11`. When this is dispatched via `crate::Client::connect` (the H2 arm), the `version` field is ignored (per `client.rs:137` which always sets `http::Version::HTTP_2`). When dispatched via `envoy_http1::Client::connect` (the H1 arm), the version is honored and a real HTTP/1.1 request goes on the wire. Behaviour is correct but the `version` field's value at the dispatch site is misleading-by-default. The relevant tests pass because the value of `version` is never observed by the H2 arm. **Fix sketch:** comment at line 160 calling out the asymmetry (the H1 arm honors `HttpVersion::Http11`, the H2 arm rebuilds via `http::Request::builder().version(http::Version::HTTP_2)`). No code change.

**M2. The H1-cluster-from-H2-listener test (`h2_proxy_outcome_dispatches_to_h1_upstream_when_cluster_is_http1`) uses an ad-hoc raw-TCP H1 server that hand-writes `HTTP/1.1 200 OK\r\nContent-Length: 19\r\n\r\nh1-from-h2-listener` instead of using the real `tests/helpers/http1-echo-server`.** `crates/envoy-http2/src/hcm.rs:759-773` justifies the choice in a comment ("the tests/helpers/http1-echo-server isn't usable from a unit test"). This is correct in context — locating + spawning a workspace binary from within a unit test is awkward and slow — but it means the H1-from-H2 dispatch path is exercised against a hand-rolled-in-test server that may diverge from `http1-echo-server`'s wire shape over time. The fixture-level Docker-gated test (0010) doesn't cover H1-from-H2 either (it's H2-cluster). **Fix sketch:** consider lifting `http1-echo-server`'s deterministic-echo body shape into a test-support helper that can be invoked in-process; both the unit test and any future Docker-gated H1-from-H2 fixture would consume it. Defer until N=2 consumers.

**M3. The validator gates the "both H1 and H2 set" mutual-exclusion (`MutuallyExclusiveExplicitHttpConfig`) but the cluster-construction code at `crates/envoy-cluster/src/cluster.rs:274-279` defensively defaults to `Http1` if both are set.** The validator runs before cluster construction, so the "both set" case is structurally unreachable at runtime. The defense-in-depth posture is fine. However: the match arm `(_, Some(_)) => UpstreamProtocol::Http2` makes the `(Some(_), Some(_))` case yield `Http2` (because it matches the `(_, Some(_))` arm first) — which is a **different** defense-in-depth choice than what the doc-comment at line 268-269 says ("defense-in-depth defaults to Http1"). The comment claims `Http1`; the code yields `Http2`. **Fix sketch:** flip the arm order to `(Some(_), Some(_)) => UpstreamProtocol::Http1, (_, Some(_)) => Http2, (Some(_), None) => Http1, (None, None) => Http1` — then the comment matches the code. Or update the comment to say "defaults to Http2 if both set." Mechanical, one-line.

**M4. The 05.3 `expectations.yaml` for fixture 0010 carries `kind: byte_exact` body shape but the body string has trailing space after `body: ` with no terminating newline.** `tests/fixtures/0010-http2-router-upstream/expectations.yaml:9` body string ends with `body: ` (literal trailing space, no newline). This is by-design per `make_response_body` (`tests/helpers/http2-echo-server/src/main.rs:225-227` writes `b"body: "` then appends body bytes; for empty bodies, no trailing newline). Per fixture 0008's `expectations.yaml` precedent the shape is consistent (PROGRESS Task 10 Body-shape-captured-at-task-time block notes the trailing space). However: the YAML quoting hides this — a reader of the .yaml file may not see the trailing space. **Fix sketch:** add a single comment line to expectations.yaml noting the trailing-space-by-design (mirrors the inline rationale 0008 ships).

**M5. The in-process backstop at `crates/envoy-bin/tests/http2_router_upstream.rs:41-46` reserves an ephemeral port via `bind 127.0.0.1:0` then drops the listener** — same TOCTOU race as the 04.3 / 05.2 backstops, inheriting M5 from the existing precedent. After `drop(listener)` the kernel may release the port back to the pool; another concurrent test in the same `cargo test --workspace` run may bind the same port before envoy-bin re-binds it. With `cargo test --test http2_router_upstream` the test runs in isolation; only `cargo test --workspace` exposes the race. **Fix sketch:** either (a) hold the listener and pass its file descriptor into envoy-bin via an `LISTEN_FD` env var (the systemd-style `sd_listen_fds` pattern; ~50 LoC across envoy-bin + the harness), or (b) add a retry loop wrapping the spawn-and-handshake. Defer; same disposition as 04.3 + 05.2 carryforwards.

**M6. The `Http2EchoBackend::Drop` polling loop at `tests/differential/src/backend.rs:296-309` blocks on `std::thread::sleep(Duration::from_millis(50))` from a tokio-runtime thread.** Phase-02.2 REVIEW M1 inherited verbatim per PROGRESS preamble. Continues the standing carryforward chain (02.2 → 03.2 → 04.3 → 05.2 → 05.3). No new findings; tracked forward to whichever phase first parallelizes `run_fixture`.

**M7. The 05.3 SPEC §6 inherited signpost 7 (test-helper architectural posture) is satisfied through the `server_handshake` thin wrapper at `crates/envoy-http2/src/codec.rs:42-48`** — but the wrapper's signature returns `h2::server::Connection<tokio::net::TcpStream, bytes::Bytes>`, which means `h2::server::*` types still leak into the helper's call sites. The helper's `Cargo.toml:22-29` block carries the carve-out justification inline. This is consistent with the parent §6 signpost 7 caveat ("h2 types leak via the return value"); no fix is recommended. Awareness only.

**M8. PROGRESS Task 12's per-fixture timing list at lines 448-458 omits the `tls_downstream_fixture` entry — Task 12 reports 9 fixtures but the project has 10 fixtures (0001-0010).** Reading carefully: line 449 lists `echo_fixture` (0001), line 450 `admin_ready_fixture` (0002), line 451 `tcp_proxy_fixture` (0003), line 452 `tls_downstream_fixture` (0004), line 453 `tls_sni_fixture` (0006), line 454 `tls_upstream_fixture` (0005), line 455 `http1_direct_response_fixture` (0007), line 456 `http1_router_upstream_fixture` (0008), line 457 `http2_direct_response_fixture` (0009), line 458 `http2_router_upstream` (0010 — labeled `NEW (05.3)`). Actually all 10 are present (4=tls_downstream, 5=tls_upstream, 6=tls_sni). My initial scan miscounted; this is **not a real issue** but the list is hard to read because the per-fixture name doesn't carry its fixture ID. **Fix sketch:** consider annotating the per-fixture timings in PROGRESS with their fixture IDs `(0001) echo_fixture 1.10s` for future reviewers. Trivial; awareness only.

---

## §4 Recommendations

1. **Validator gate against H1-listener-with-H2-cluster (Important, I1):** before the [parent 05 done] state-6 close-out — or land in an early 06.x task — add `ConfigError::Http2ClusterFromHttp1Listener { listener, cluster }` and the per-listener reachability scan in `validate`. This prevents the silent protocol-misnegotiation that ADR-0028 option (B) leaves on the runtime path.

2. **Land a real CI run URL + h2spec percentage in PROGRESS Task 12 (Important, I3):** push the branch to trigger CI before the state-6 close-out, capture the run URL + actual h2spec line, replace the `<TBD>` placeholder. Mechanical; the close-out commit's evidence chain depends on this.

3. **Typed-error preservation at the H2 dispatch site (Important, I2):** add a private `DispatchError` sum in `crates/envoy-http2/src/hcm.rs` to preserve the typed source chain into `tracing::warn!`. Mechanical; one-site fix.

4. **Cycle-resolution restructure (carryforward from ADR-0028):** when the H1-listener × H2-cluster combination acquires a fixture (likely in 06+ when the access-log fixture surface adds shapes), pick option (A) (trait-object hoist) or (C) (`envoy-http-client` extraction) per the ADR's outline. Until then, the validator gate from R-1 above contains the gap.

5. **Match-arm comment alignment in `Cluster::from_bootstrap` (Minor, M3):** flip the match arm order so the doc-comment at line 268-269 matches the code's defense-in-depth behavior, OR update the comment. One-line touch.

6. **Documentation of the load-bearing trailing space in fixture 0010's `expectations.yaml` (Minor, M4):** add a brief comment header. Trivial.

7. **TOCTOU-free port reservation for in-process integration backstops (Minor, M5; standing carryforward from 04.3 + 05.2):** when N=4 backstops have accreted, lift to a workspace-shared listener-fd-passing helper.

---

## §5 Carryforward verdict

Phase 05.3 SPEC §4 explicitly defers the following surfaces to later sub-phases / phases. Each is correctly NOT touched in 05.3.

**Deferred to phase 06+ (per SPEC §4):**

- Connection pooling on upstream H2 (one-connection-per-call posture preserved).
- HTTP/2 over TLS (ALPN-negotiated H2) on listener and cluster sides.
- Cross-protocol H2↔H1 framing-translation layer (trailers, streaming bodies, full feature parity).
- HTTP/2 trailers (HEADERS frame after END_STREAM).
- HTTP/2 server push / `PUSH_PROMISE`.
- HTTP/2 over HTTP/1.1 Upgrade (`Upgrade: h2c`).
- HTTP/3 / QUIC.
- Per-route `Http2ProtocolOptions` overrides.
- `Http1ProtocolOptions` field set (cluster-side H1 protocol-tuning).
- Cross-cluster top-level `http_protocol_options` / `http2_protocol_options` (the deprecated form; only `typed_extension_protocol_options` is shipped).
- HTTP/2 stream-level flow-control tuning beyond the four `Http2ProtocolOptions` fields.
- HTTP/2 connection draining / `GOAWAY` on graceful shutdown (phase-08 territory).
- Streaming request bodies on the upstream-bound side.
- HCM `server_name` field.
- `codec_type: AUTO` byte-sniffing for H2C.
- `LOGICAL_DNS` cluster type / `dns_refresh_rate` / `respect_dns_ttl` / `dns_resolvers`.

**Carried forward by this REVIEW:**

- **I1** (H1-listener-with-H2-cluster silent misnegotiation; validator gate not yet added) — close in early 06.x as a defensive validator gate; full fix when option (A) or (C) restructure lands.
- **I2** (H2 dispatch site stringifies typed errors) — close in 06+. Mechanical, ~10 LoC.
- **I3** (Task 12 PROGRESS gate evidence is local-only with placeholder CI URL) — must close before the state-6 close-out commit lands.
- **M1** (`HttpVersion::Http11` field on H2 dispatch outbound is misleading-by-default for the H2 arm) — close opportunistically.
- **M2** (H1-from-H2 dispatch test uses ad-hoc raw-TCP H1 server) — close opportunistically when a second consumer appears.
- **M3** (`Cluster::from_bootstrap` "both Some" defense-in-depth match arm yields `Http2`, not the `Http1` the doc-comment claims) — close opportunistically; one-line.
- **M4** (fixture 0010 trailing-space comment) — close opportunistically; trivial.
- **M5** (in-process backstop port-reservation TOCTOU; standing carryforward from 04.3 + 05.2) — defer until N=4 backstops.
- **M6** (`Http2EchoBackend::Drop` polling loop blocks on `std::thread::sleep`) — standing carryforward chain inherited verbatim from phase-02.2 REVIEW M1.
- **M7** (the H2-helper `Cargo.toml` carve-out is signposted) — awareness only.
- **M8** (PROGRESS per-fixture timings would be more readable with fixture IDs) — awareness only; trivial future-reviewer ergonomics.

**Closed in 05.3:**

- **05.2 REVIEW M8** (502-stub body literal) — closed structurally at Task 7 (`crates/envoy-http2/src/hcm.rs`'s `BuildOutcome::Proxy` arm now does the symmetric H1-or-H2 dispatch; the literal stub body is gone).
- **05.2 Task 2 review I2** (per-module `H2_FORBIDDEN_HOP_BY_HOP` consolidation — distinct from the 05.2 REVIEW.md I2) — closed at Task 7 alongside the dispatch wiring.

**Carried forward unchanged from earlier phases (not engaged in 05.3):**

- **05.2 REVIEW I1** (CI tarball SHA-256 verification on the h2spec install step in `.github/workflows/ci.yml`) — `.github/workflows/ci.yml` is unedited per SPEC §8; carries forward to a 05.x security-pass or whichever phase first edits CI.
- **05.2 REVIEW I2** (`Http2Error` write-path variant misnomer at `H2StreamAccept` / `H2BodyRead` for response-side send-paths) — the 05.2 codec-side variants stayed unchanged in 05.3 per SPEC §3 D1's explicit "the 05.2 codec-side variants ... stay unchanged"; carries forward.
- **05.2 REVIEW I3** (`MalformedH2HeaderBlock` overload split into 3 fine-grained variants) — same reasoning as I2; carries forward.
- **05.2 REVIEW M2** (per-stream timeout budget) — STATE.md "Phase-05.2 rollovers" called out the upstream-H2 spawn site as the natural fit; the actual landing site at `crates/envoy-http2/src/hcm.rs:79-83` does not add a per-stream `tokio::time::timeout`. Carries forward to phase-08 (graceful drain) or 06+.
- **05.2 REVIEW M3** (`h2_protocol_options_max_concurrent_streams_applied` `#[ignore]`-d) — preserved at `crates/envoy-http2/src/hcm.rs:797-809`; carries forward.
- **05.2 REVIEW M4 / M9** (SPEC §3 D6 `expectations.yaml` example shape drift; SPEC §3 D7 h2spec config example missing/extra `node:`) — close at next SPEC editing pass.
- **05.2 REVIEW M5** (locator helper `locate_envoy_bin` extraction) — defer until N=3 consumers.
- **05.2 REVIEW M6** (h2spec gate diagnostic should surface skipped count) — `tests/conformance/h2spec/` unedited per SPEC; carries forward.
- **05.2 REVIEW M10** (`Driver::Http2` lacks `extra_headers` field) — DEFERRED. PROGRESS Task 9 disposition records: fixture 0010 does not need `extra_headers` per SPEC §3 D7. Carries forward to whichever fixture first needs it.
- **05.2 REVIEW M11** (RFC-soft `MissingAuthority` recovery; H2 stream task drops silently instead of synthesizing a 400) — `crates/envoy-http2/src/hcm.rs:79-83` still drops silently. Carries forward to 06+.
- **05.2 REVIEW M12** (garbage-preamble test permissive close-shape assertion) — preserved at `crates/envoy-http2/src/hcm.rs:715-751`; carries forward.

**Standing inventory carryforwards (no change in 05.3):**

- **Phase-04.1 REVIEW M-architectural-claim** (`drive_http1` per-function unit test): 05.3 introduces no new H1 surfaces and does not extend the harness in a way that adds a third `Driver::Http1` consumer; M-claim continues unchanged.
- **Phase-04.1 REVIEW M5/M9** (Cargo.lock cadence ratification ADR): 05.3 introduces no new top-level Cargo deps; M5/M9 carries forward unchanged.
- **Phase-04.1 REVIEW M7** (`TlsAcceptingHandler.inner: Arc<TcpProxy>` concrete-typed; HCM-in-TLS doesn't typecheck): re-deferred per SPEC §4 to whichever phase ships ALPN-driven dispatch.
- **Phase-04.1 REVIEW M1/M2/M4** (header-diff value-comparison; body-drain idle silent Ok; `strip_port` IPv6-Host): 05.3 does not exercise; carries forward.
- **Phase-02.2 REVIEW M1** (`*EchoBackend::Drop` polling loop blocks on `std::thread::sleep`): inherited verbatim by `Http2EchoBackend`; M1 continues to track.

---

## §6 Verification gate observation

The state-4 phase-done gate evidence per PROGRESS Task 12 is **incomplete vs the 05.2 baseline** in two specific respects, but functionally green for what was run:

- **Acceptance signal (a)** GREEN locally — fixture 0010's in-process backstop passes (`http2_router_upstream` 2.54s, plus `http2_router_upstream_in_process` via the envoy-bin integration test). **The CI run URL is a placeholder** (`<CI run URL TBD by controller after push>`) — see I3 above. By comparison, 05.2 PROGRESS Task 14 cited a real CI run ID `25294149612`.
- **Acceptance signal (b)** GREEN locally — all 9 pre-existing fixtures (0001-0009) pass simultaneously per `cargo test --workspace`. The per-fixture timings list at PROGRESS Task 12 lines 448-458 is complete (10 fixtures including 0010).
- **Acceptance signal (c)** **NOT VERIFIED locally** — h2spec binary not installed; the conformance crate gracefully reports "h2spec not found — skipping locally" and passes its 3 runner-unit tests. The actual h2spec percentage from CI is not captured in PROGRESS. By comparison, 05.2 PROGRESS Task 14 captured the actual `144 passed / 1 failed / 1 skipped of 146 = 99.31%` line.
- **Acceptance signal (d)** GREEN — fuzz `parse_bootstrap` clean for 31s locally with the new `cluster_http2_protocol_options.yaml` corpus seed exercising the validator's typed_extension_protocol_options accept-paths. `Done 486555 runs in 31 second(s)` with no panics, crashes, or OOM.
- **Acceptance signal (e)** GREEN — `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean. The single ignored test (`h2_protocol_options_max_concurrent_streams_applied`) is documented (05.2 M3 carryforward) and not blocking. `cargo deny check` final line: `advisories ok, bans ok, licenses ok, sources ok` (5 pre-existing benign `license-not-encountered` advisory-only warnings unchanged from the 05.2 baseline; no new licenses brought in by 05.3).

The Cargo.lock diff is 15 lines (the new `http2-echo-server` workspace-member registration only — no new top-level deps).

The ADR ledger landed correctly: ADR-0028 at `docs/envoy-rust/DECISIONS.md:513-530` (verified by `grep -nE '^## ADR-' docs/envoy-rust/DECISIONS.md | tail -3` per PROGRESS Task 12). The ADR documents the deliberate D-3.5 in-execution deviation from SPEC §7's "no new ADRs projected" projection; the rationale is well-reasoned (cycle-resolution forced at task time when the controller attempted to add `envoy-http2` to `crates/envoy-http1/Cargo.toml`).

The fixup commit `53ac466` exists specifically because the original Task 12 PROGRESS commit `83e4da7` was missing the verifying SHA in its `Commit:` field. The self-correction is good cadence; however, the same fixup commit did NOT also fill in the CI run URL or the h2spec percentage — see I3.

**The gate evidence is sufficient to declare state-4 GREEN at the local level, but the [parent 05 done] state-6 close-out commit will land with sub-phase evidence that says "TBD by controller after push" if I3 is not closed beforehand.** The recommendation is to push the branch, capture the CI run URL + h2spec percentage, and amend PROGRESS Task 12 (a) and (c) before the state-6 close-out commit fires.

---

## §7 Final verdict

**Approved with M-track follow-ups.**

Phase 05.3 lands the project's first end-to-end H2C-on-H2C round-trip cleanly, ships 12 substantive tasks across ~8000 LoC with disciplined PROGRESS narration, preserves the cross-sub-phase architectural rule that `envoy-http2` is the sole `h2`-depending production crate (verified empirically), and resolves an unforeseen Cargo cycle via ADR-0028 with explicit rationale and named consequences. The new `envoy-http2::Client` is well-tested (8 unit tests covering connect-success, refused-connect, pseudo-header synthesis, explicit-Host wins over captured-host, response status/header/body drain, multi-frame body drain, hop-by-hop strip, and handshake-failure mapping) and the H2-listener-side dispatch closes 05.2 REVIEW M8 structurally rather than nominally. Three Important findings (I1 silent H1-listener × H2-cluster misnegotiation per ADR-0028 deferral; I2 typed-error chain dissolved at the dispatch site; I3 state-4 gate evidence is local-only with placeholder CI URL) carry forward — I3 is the only one that should close before the state-6 phase-done close-out commit lands; I1 and I2 are scoped, mechanical 06+ follow-ups parallel to the 05.2 → 05.3 carryforward path. Eight Minor findings cluster around comment-vs-code drift, test-shape ergonomics, and standing carryforwards inherited from 02.2 / 04.1 / 04.3 / 05.2; none gate phase-done.

**Phase 05.3 is approved for state-6 close-out, conditional on closing I3 (filling the CI run URL + h2spec percentage in PROGRESS Task 12) before the [parent 05 done] commit.** I1 and I2 carry forward to 06+ alongside the standing inventory carryforwards (05.2 I1+I2+I3+M2+M3+M4+M5+M6+M9+M10+M11+M12; 04.1 M-claim, M1/M2/M4, M5/M9 Cargo.lock, M7 TLS+H2; 02.2 M1).
