# Phase 05.2 REVIEW — downstream HTTP/2 cleartext: `envoy-http2` + HCM-on-H2 + fixture 0009 + h2spec ≥95% gate

- **Phase id:** `05.2`
- **Slug:** `05.2-http2-downstream`
- **Reviewed range:** `b843168..0b88d91` (37 files, 7545 insertions / 61 deletions across 26 commits — PLAN.md + 14 substantive task commits + 6 review-fixup / progress-note commits + 1 post-CI parser-rewrite fixup + 1 state-4 close-out narration commit + 3 ancillary task-commits not separately enumerated above).
- **CI evidence cited:** run `25294149612` HEAD `dac3f8b` (state-4 phase-done gate verification, 2026-05-03).
- **Reviewed:** 2026-05-03.
- **Verdict:** **Approved with M-track follow-ups** — state 5 complete. Zero Critical findings. Three Important findings (I1 CI tarball checksum / I2 `Http2Error` variant misnomer at write paths / I3 `MalformedH2HeaderBlock` overload) — all are scoped, mechanical, hardening-or-ergonomic in character (none are correctness bugs); the reviewer recommends carrying forward to 05.3 rather than re-entering state 3, parallel to phase-04.3's C-1 cross-phase Important carryforward. Twelve awareness-only Minor findings (M1 through M12). Phase-04.3 REVIEW C-1 carryforward chain remains closed (substantively closed at 05.4 Task 7); 05.2 inherits the GREEN baseline cleanly with all 9 Docker-gated fixtures (0001-0008 inherited + new 0009) green simultaneously per CI run `25294149612`.

---

## §1 Summary

Phase 05.2 lands the project's first HTTP/2 surface end-to-end:

- New workspace member `crates/envoy-http2/` (~1100 LoC across 5 modules — error / request / response / codec / hcm), sole-dep-owner of `h2 = "0.4"` and (per ADR-0027) `http = "1"` — cleanly mirrors `envoy-http1`'s sole-owner-of-`httparse` posture established in 04.1 and `envoy-tls`'s sole-owner-of-`rustls` from 03.1.
- `envoy-config` schema growth: `CodecType::HTTP2` accept-flip (was reject-only), explicit `Http2OverTlsNotSupported` parse-time gate, and a 4-field `Http2ProtocolOptions` struct on `HttpConnectionManagerConfig` with RFC 7540 range validation (~150 LoC schema + ~130 LoC unit tests + corpus-walk acceptance via the new `hcm_codec_http2.yaml` fuzz corpus seed).
- `envoy-http1::HCMConfig` extended with an optional `http2_protocol_options` field (a sensible cross-cutting design carrying the H2 tuning across the codec boundary; documented as inert on the H1 path at `crates/envoy-http1/src/hcm.rs:43-46`).
- `envoy-bin` HCM-on-H2 dispatch wiring with H1/H2 branching on `HCMConfig.codec_type` + a defensive symmetric H2+TLS bail (post-review fixup I2 on Task 10 at `crates/envoy-bin/src/main.rs:269-277`).
- `tests/differential` extensions: `Driver::Http2` variant, `drive_http2` async helper (carve-out per parent-05 SPEC §6 signpost 8 parallel to phase-04.1's `httparse` posture), full per-axis equivalence cascade in the run-fixture dispatch arm.
- Fixture 0009 (5 files at `tests/fixtures/0009-http2-direct-response/`, minimal H2C direct-response, `clusters: []`).
- First conformance suite: `tests/conformance/h2spec/` workspace member with a runner enforcing three gates (≥95% pass / no surprise regressions / no stale known-failures entries). h2spec 2.6.0 pinned in CI via the new `install h2spec` step at `.github/workflows/ci.yml:43-49`.
- ADR-0027 landed at Task 1 (`http` graduated from transitive-only to a workspace foundation); ADR-0028 explicitly NOT landed (rationale recorded inline in PROGRESS Task 13 — gate-mechanics mechanically deterministic, no decision worth ADR-shaped permanent record).

State-4 phase-done gate verified GREEN on CI run `25294149612`:

- All 9 Docker-gated fixtures (0001-0008 inherited GREEN baseline + new 0009) pass simultaneously.
- h2spec: 144 passed / 1 failed / 1 skipped of 146 = **99.31%** (well above the 95% gate).
- Single failure `3.5/2` (invalid PRI preface — h2 sends RST instead of GOAWAY) catalogued in `tests/conformance/h2spec/known-failures.txt` as a foundation limitation per parent-05 SPEC §6 signpost 13 ("trust h2 codec to reject malformed handshakes").
- 5 stable-toolchain commands clean (build, clippy `-D warnings`, fmt, test, deny); fuzz `parse_bootstrap` clean for 30s short-budget CI run.

The implementation closely matches the SPEC, with documented deviations resolved at task time. PROGRESS.md is exemplary: every task carries Files-Modified, Verification, Verified-shapes-from-greps, Deviations-from-PLAN, and Carryforward sections; post-review fixups land in named follow-up commits with explicit close-out attribution; the parser-rewrite at `dac3f8b` (post first end-to-end CI run) is a model of how to handle "the foundation behaves slightly differently than the SPEC anticipated" without scope creep.

---

## §2 Strengths

1. **Architectural rule 1 ("envoy-http2 is the sole workspace dep on `h2`") is preserved at runtime.** No production crate other than `envoy-http2` imports `h2::*`. The two carve-outs are scoped and signposted: (a) `tests/differential/Cargo.toml` lists `h2 = "0.4"` as a `[dependencies]` entry for the `drive_http2` helper, matching the documented SPEC §6 signpost 8 carve-out parallel to phase-04.1's `httparse` posture; (b) `crates/envoy-bin/Cargo.toml` lists `h2 = "0.4"` in `[dev-dependencies]` only — the in-process integration backstop at `crates/envoy-bin/tests/http2_direct_response.rs` uses `h2::client` per SPEC §6 signpost 18 for the non-Docker backstop. Production envoy-bin code does not link `h2`.

2. **`HCMConfig` polymorphism over codec is clean.** `envoy_http2::hcm::HCMConfig` is a type alias to `envoy_http1::HCMConfig` (`crates/envoy-http2/src/hcm.rs:26`); the dispatch-by-codec lives at the listener-walk site in `crates/envoy-bin/src/main.rs:226-240`. The cross-cutting placement of `http2_protocol_options` on `envoy_http1::HCMConfig` (with a clear "inert on H1 path" doc-comment at `crates/envoy-http1/src/hcm.rs:43-46`) is pragmatic and avoids needless trait-level abstraction.

3. **Pseudo-header → envoy `Request` translation is correct and defensive.** `crates/envoy-http2/src/request.rs:24-84` (a) reads `:method` from `parts.method`, (b) reads `:path` from `parts.uri.path_and_query()` with a `/` fallback for path-less URIs, (c) prefers `parts.uri.authority()` for `:authority` then falls back to the `Host:` header (handles both h2-version delivery shapes), (d) raises `MissingAuthority` when neither exists, (e) skips an existing `Host:` header to avoid duplicate rows after the synthesis. Tests cover the four primary paths (lowercase preservation, authority synthesis, missing-authority error, non-UTF-8 value error).

4. **H2-forbidden hop-by-hop strip lives at the codec edge.** `crates/envoy-http2/src/response.rs:26-33` consts the 5 RFC 7540 §8.1.2.2 forbidden header names; `build_http_response` strips them defensively before calling `h2::SendStream` (which would also reject). The HCM core in `envoy-http1` is unchanged. Test 5 (`h2_response_strips_hop_by_hop_headers_defensively`) drives the full HCM-on-H2 round-trip and asserts none of the forbidden names appear in the client's parsed response — strong end-to-end coverage.

5. **PRI-preamble handling is correctly entrusted to `h2`.** `crates/envoy-http2/src/hcm.rs:55-64` calls `Builder::handshake(downstream)` and surfaces handshake failure as `Http2Error::H2Handshake { source }`. No byte-sniffing — matches parent §6 signpost 13. The garbage-preamble test (`h2_handshake_fails_on_garbage_preamble`) sends an HTTP/1.1 request and asserts the connection closes within 1 second with no HTTP/1.1 response body emitted.

6. **Per-stream `tokio::spawn` lifecycle matches SPEC §6 local signpost 20.** `serve_h2_connection` spawns one task per accepted stream (`crates/envoy-http2/src/hcm.rs:78-82`); per-stream errors are logged via `tracing::error!` and do NOT propagate to the connection-driver task. Sibling streams remain alive when one fails — correct H2 semantics.

7. **`Http2ProtocolOptions` validator ranges are RFC-precise.** `crates/envoy-config/src/bootstrap.rs:1180-1215` enforces `max_frame_size ∈ [16384, 16777215]` (RFC 7540 §6.5.2), window sizes ≤ `2^31 - 1` (§6.9.1/§6.9.2), and correctly leaves `max_concurrent_streams` unbounded (zero is valid). The `&'static str` field name carried on `ConfigError::Http2ProtocolOptionsOutOfRange` localizes diagnostics cleanly.

8. **Fixture 0009 is minimally scoped.** `tests/fixtures/0009-http2-direct-response/` carries `clusters: []` per SPEC, exercising the `BuildOutcome::Synth` arm only. envoy.yaml carries the `generate_request_id: false` upstream-only knob (avoids x-request-id divergence under the existing allow-list); envoy-rust.yaml omits it (no admin block). Per-side divergences are minimal and inherited from the 04.x precedent.

9. **h2spec parser is thoughtfully revised post-first-CI-run.** Initial scaffold scraped per-line `× <id>` / `✓ <id>` markers and a `Passed: N` / `Failed: N` summary that h2spec does not actually emit. The post-CI fixup at commit `dac3f8b` (a) tracks section-heading context (`3.5. HTTP/2 Connection Preface` ⇒ `current_section = "3.5"`), (b) derives full IDs as `<section>/<num>`, (c) reads counts from the canonical `<N> tests, <M> passed, <K> skipped, <L> failed` summary line, and (d) ships two unit tests (`parse_summary_line_extracts_pass_fail_counts`, `parse_h2spec_output_extracts_section_failure_ids`) that lock in the parser shape against future regressions without requiring h2spec installed. The summary-line approach is robust against per-test ornamentation.

10. **The h2spec gate has three-way maintenance discipline.** `tests/conformance/h2spec/tests/h2spec_runner.rs:118-142` enforces (a) ≥95% pass rate, (b) every failing test enumerated in `known-failures.txt`, (c) every entry in `known-failures.txt` actually fails (forces lockstep trim when the foundation gains capability). Gate (c) is unusually principled — most conformance suites skip the stale-entry check and let the file rot.

11. **Defensive runtime symmetric guard for H2+TLS.** Post-review fixup I2 on Task 10 (`crates/envoy-bin/src/main.rs:269-277`) adds a runtime bail mirroring the existing H1 bail. The validator already rejects this combination, so the guard is unreachable from any well-formed config — but a future validator regression would now surface as a clean config-load error rather than silently binding a non-functional plaintext H2 listener on a port the operator expected to be TLS-protected.

12. **PROGRESS.md is exemplary self-narrating.** Every task carries Files-Modified, Verification, Verified-shapes-from-greps-run-at-task-time, Deviations-from-PLAN, and Carryforward sections. Post-review fixups land in named follow-up commits with explicit close-out of which review items were addressed and which were deferred. Task 13 narrative records the explicit decision NOT to land ADR-0028 with the rationale (gate-mechanics mechanically deterministic).

13. **ADR-0027 text matches actual posture.** `docs/envoy-rust/DECISIONS.md:497-510` cleanly articulates the three options (direct dep / `h2::http` re-exports / opaque-only access), justifies the chosen direct dep narrowly (parallel to ADR-0021's `regex` scoping), and records the renumbering provenance (ADR-0024 projected → ADR-0027 landed because 05.4 landed 0024/0025/0026 in between). Ledger discipline is intact.

14. **Behavior-contract engagement is correctly minimal.** `BEHAVIOR_CONTRACT.md` is unedited per SPEC §2; equivalence-matrix Row 4 (HTTP/2 framing) is engaged structurally without any byte-level wire assertion (the harness drives via parsed `h2::client`, not raw wire bytes).

---

## §3 Issues

### Critical

None.

### Important

**I1. CI h2spec tarball lacks integrity verification.** `.github/workflows/ci.yml:43-49` provisions the h2spec binary via `curl -fsSL ... | sudo tar xz -C /usr/local/bin` with no SHA-256 verification, no GPG signature, and no tag pinning beyond the version string. A compromised release artifact would silently land in the CI image and execute as root via `sudo tar`. The blast radius is contained (CI runner is ephemeral; envoy-rust source is checked out before the install step), but a release-pipeline takeover or DNS hijack could inject arbitrary code into the test execution environment. **Fix sketch:** capture the published SHA-256 from the GitHub release page, hardcode it, and verify after download:

```yaml
H2SPEC_VERSION="2.6.0"
H2SPEC_SHA256="<paste from release notes>"
curl -fsSL -o /tmp/h2spec.tgz "https://.../h2spec_linux_amd64.tar.gz"
echo "${H2SPEC_SHA256}  /tmp/h2spec.tgz" | sha256sum -c -
sudo tar xz -C /usr/local/bin -f /tmp/h2spec.tgz
h2spec --version
```

Tracks SPEC §6 signpost 3 (which recommended option (b) but did not specify integrity verification). This is a process-hardening gap rather than a correctness gap; flag as Important because the blast surface is the differential-test toolchain.

**Disposition:** carry forward to 05.3 (or as a standalone 05.x security-pass).

**I2. `Http2Error` variant overload at the response-emission edge.** `crates/envoy-http2/src/response.rs:60-69` openly admits two misnomers in the doc-comment block: `send_response()` failures are mapped to `H2StreamAccept` (line 77) and `send_data()` failures are mapped to `H2BodyRead` (line 81). The variant names imply inbound/accept paths, but they're being used for outbound/send paths. Code reads as if the variants are repurposed defensively; the source `h2::Error` retains diagnostic fidelity, but the typed wrapper is misleading on stack traces and log lines. **Fix sketch:** add `H2ResponseSend { source: h2::Error }` and `H2BodyWrite { source: h2::Error }` (or rename `H2StreamAccept` → `H2StreamIo` if a single duplex variant is preferred). Touch only `error.rs` + the two call sites in `response.rs`. The `Http2Error` enum is non-`#[non_exhaustive]` (and is `pub`) so this is a public API change — but this crate has no external consumers yet, so additive variants are cheap.

**Disposition:** carry forward to 05.3, before the upstream H2 client introduces additional H2 error sites that would amplify the misnaming.

**I3. `MalformedH2HeaderBlock` is a coarse catch-all that hides three distinct failure modes.** `crates/envoy-http2/src/error.rs:40-50` documents the variant as covering structural pseudo-header issues, non-token byte names, and non-UTF-8 byte values; in practice it is also raised for `HeaderName::from_bytes` failure on the response side, `HeaderValue::from_str` failure, and `HttpResponse::builder().body(())` failure (`crates/envoy-http2/src/response.rs:49-57`). When a request-side translation failure hits this variant, an operator looking at logs cannot tell whether it was a non-UTF-8 header-value byte (request-side, defensible 400), an unrepresentable response status, or a stale state-machine bug elsewhere. **Fix sketch:** split into `InvalidHeaderName { name: String }`, `InvalidHeaderValue { name: String }`, `InvalidResponseHead`. The fix keeps the public-API impact bounded since callers currently only `match` exhaustively in tests.

**Disposition:** carry forward to 05.3.

### Minor

**M1. `error::Http2Error::BadStatusCode` is essentially unreachable in production.** `crates/envoy-http2/src/response.rs:38-40` calls `StatusCode::from_u16(resp.status)` to map a `Response { status: u16, .. }` whose `status` is already validated to `100..=599` by the route-walk and the validator (`ConfigError::InvalidStatusCode` at parse time, `synth_status` at synth time). `from_u16` only fails for values < 100 or ≥ 1000 or = 0. The variant exists as defense-in-depth and is documented as such (`error.rs:52-57`); leave it — defense-in-depth is the appropriate posture here. Awareness only.

**M2. Per-stream `tokio::spawn` task carries no per-test or per-fixture timeout.** `crates/envoy-http2/src/hcm.rs:78-82` spawns each H2 stream task without a wall-clock budget. If `handle_one_stream` or its callees block (e.g., the future `BuildOutcome::Proxy` H2 dispatch landing in 05.3 hangs), the parent connection-driver loop continues but the stream task leaks. Phase 05.2's only `BuildOutcome::Proxy` arm is the 502-stub which returns synchronously — so this is latent. SPEC §4 defers `HTTP/2 connection draining / GOAWAY handling on graceful shutdown` to phase-08, which is the natural landing site. **Fix sketch:** add a per-stream `tokio::time::timeout` budget when 05.3 wires the upstream H2 dispatch (the budget fits naturally there since the upstream call is the long-tail).

**Disposition:** carry forward to 05.3 or 08.

**M3. `h2_protocol_options_max_concurrent_streams_applied` test is `#[ignore]`-d.** `crates/envoy-http2/src/hcm.rs:515-527` ships the test as ignored with a thoughtful `#[ignore = "..."]` reason explaining that h2's public API at 0.4 doesn't deterministically surface peer SETTINGS observability without racing the response loop. This is the SPEC §3 D3 test 8 — partial coverage shifts to `codec.rs::tests::build_h2_server_applies_protocol_options` which only verifies that the setter is callable. The codec-edge configuration smoke is fine; the wire-effect test is genuinely blocked by upstream library shape. **Fix sketch:** when the upstream `h2` crate exposes a stable observability hook (or when the project lands its own h2-server harness with internal observability), replace `#[ignore]` with the actual driver. Track as a 05.3+ awareness item.

**Disposition:** awareness-only carryforward.

**M4. `expectations.yaml` syntax convention is undocumented.** SPEC §3 D6 documents `expected_headers: { rule: set_equal_modulo_allow_list }` (struct shape), but the fixture ships `expected_headers: set_equal_modulo_allow_list` (string-shaped — matches the unit-variant `Http1HeaderRule::SetEqualModuloAllowList` deserializer). The behavior is correct; the SPEC text is stale. **Fix sketch:** at next SPEC editing pass, normalize the SPEC's expectations-YAML examples to match the actual `Http1HeaderRule` deserializer shape. (Likely shared with phase 05.3's SPEC if it cross-references.)

**M5. `h2spec` runner relies on `target/<profile>/envoy-bin` convention with a fragile `.parent().parent().parent()` walk.** `tests/conformance/h2spec/tests/h2spec_runner.rs:177-204` mirrors the `tests/differential/src/subject.rs::locate_envoy_bin` pattern and walks three parents up from `CARGO_MANIFEST_DIR`. If the conformance crate is ever moved (e.g., from `tests/conformance/h2spec/` to `tests/h2spec/`), the parent count silently breaks. The pattern is cross-crate-stable but path-position-fragile. **Fix sketch:** factor `locate_envoy_bin` into a small `envoy-test-support` workspace member (or expose `subject::locate_envoy_bin` as `pub`); both the differential harness and the conformance runner consume it. Defer until the third consumer appears.

**Disposition:** awareness-only carryforward; revisit at N=3 consumers.

**M6. h2spec parser does not surface the count of `skipped` tests in the gate diagnostic.** `tests/conformance/h2spec/tests/h2spec_runner.rs:282-303` parses `passed` and `failed` from the summary line but discards `skipped`. The pass-rate gate is `passed / (passed + failed)`, which is the right fraction (skipped is correctly excluded), but if the skipped count drifts (say, jumps from 1 to 50 because a future h2spec release marks half its corpus as skipped), the operator would not see this in the gate output. Currently `eprintln!`-ed with the full stdout, but not surfaced as a structured field. **Fix sketch:** extract `skipped` and emit `eprintln!("h2spec: passed=… failed=… skipped=… total=… pass_rate=…")` at the gate site. Trivial.

**M7. The `envoy-http2` crate's `[dev-dependencies]` lists `envoy-cluster` for `ClusterManager::empty()` in HCM tests.** `crates/envoy-http2/Cargo.toml:25` reads `envoy-cluster = { path = "../envoy-cluster" }` solely for `envoy_cluster::ClusterManager::empty()` (used by `crates/envoy-http2/src/hcm.rs:198`). The `empty()` constructor is `#[doc(hidden)] pub` per `crates/envoy-cluster/src/cluster.rs:81-99`; the doc-comment explicitly addresses cross-crate test consumption. This is fine, but cross-crate use of a `#[doc(hidden)]` constructor is a soft architectural smell. **Fix sketch:** consider replacing with a small `Arc<ClusterManager>` injection from the test fixture (the route-walk only consults `cluster_mgr` on `BuildOutcome::Proxy` paths and the route in test config never hits Proxy). Defer; current shape is not actively harmful.

**M8. The `502 Bad Gateway` stub body literal mentions phase 05.3.** `crates/envoy-http2/src/hcm.rs:132` emits body `b"upstream H2 not yet wired (sub-phase 05.3)\n"`. SPEC §6 local signpost 21 explicitly says "no real cluster names or endpoint addresses" but accepts "doctrine-line is sufficient," so this is in-SPEC. When 05.3 replaces the stub, the literal should disappear. **Fix sketch:** none in 05.2; flag as a trivial 05.3 entry-point check.

**M9. h2spec config carries no `node` field at the runner level but uses `node:` in the YAML.** Cross-check: `tests/conformance/h2spec/h2spec.yaml` includes `node: { id: h2spec-target, cluster: envoy-rust-conformance }`. envoy-rust treats `node` as optional + open-shaped (per the `bootstrap.rs:25-29` comment), so this works. But the SPEC §3 D7 example for the runner config did not include a `node` block. **Fix sketch:** SPEC text could be tightened to either include or explicitly elide `node:` — non-blocking.

**M10. The `differential::Driver::Http2` variant is shape-symmetric with `Http1` but does not yet support `extra_headers`.** `tests/differential/src/lib.rs:86-96` carries `method/path/host/expected_status/expected_body/expected_headers` but no `extra_headers: Vec<(String, String)>` field (compare `Http1Probe` at `tests/differential/src/lib.rs:175-191`, which carries `extra_headers`). 05.2's only fixture (0009) does not need it; 05.3 may. The `drive_http2` helper already accepts an `extra_headers` parameter (`tests/differential/src/lib.rs:806`); only the enum variant lacks the field. **Fix sketch:** at 05.3 when fixture 0010 lands, add the field with `#[serde(default)]` and thread it through the dispatch arm.

**Disposition:** carry forward to 05.3.

**M11. The `Http2Error::MissingAuthority` recovery path is RFC-soft.** `crates/envoy-http2/src/hcm.rs:78-82` (the spawn site) logs the per-stream error via `tracing::error!` and lets the task end without sending any response. RFC 9113 §8.3.1 requires a request without `:authority` (and without a `Host:` header) to either be rejected by the codec layer or generate a 400 from the server. envoy-rust currently produces no response; the client observes a stream-level RST or no headers. h2 0.4 may or may not pre-reject this on the codec side. **Fix sketch:** raise a 400 Bad Request from the stream task on `MissingAuthority` (and other request-translation failures) instead of dropping silently. Maps to envoy-rust's H1 `synth_400` posture.

**Disposition:** carry forward to 05.3 or 06+.

**M12. The `h2_handshake_fails_on_garbage_preamble` test is permissive on the close shape.** `crates/envoy-http2/src/hcm.rs:498-512` accepts `Ok(0)` (clean FIN), `Ok(non-zero)` provided the bytes are not an HTTP/1.1 status line, OR `Err(_)` (RST/ECONNRESET). This catches anything-but-success but loses signal — a regression that, say, lets the connection complete an empty handshake and then hangs would be misclassified as an `Err(_)` and pass. **Fix sketch:** tighten to a positive assertion: deadline of 1 second AND received bytes do not include `:status: 200` (or any known-good H2 frame). Trivial; the existing assertion is already 80% there.

---

## §4 Recommendations

1. **Hardening (Important; carry to 05.3 / 05.x):** Land a SHA-256 verification step on the h2spec tarball (I1). The fix is mechanical and reduces a non-trivial supply-chain surface.

2. **Typed-error cleanup (Important; carry to 05.3):** Split `MalformedH2HeaderBlock` (I3) and the two-misnomer `H2StreamAccept` / `H2BodyRead`-as-write-paths (I2) before the upstream H2 client lands in 05.3 — both edges add new error sites that would amplify the misnaming. The `Http2Error` enum is `pub` but has no external consumers; additive variants are cheap now.

3. **Per-stream timeout budget (Minor; defer to 05.3 or 08):** When 05.3 wires `BuildOutcome::Proxy` to dispatch into `envoy_http2::Client`, layer in a `tokio::time::timeout` per-stream budget at the spawn site (M2). Phase 08 (graceful drain) is the natural site for connection-level GOAWAY handling.

4. **Locator helper extraction (Minor; carry forward indefinitely):** Factor `locate_envoy_bin()` into a workspace test-support crate when the third consumer appears (M5). Currently two consumers (`tests/differential/src/subject.rs` and `tests/conformance/h2spec/tests/h2spec_runner.rs`); the duplication is OK at N=2.

5. **SPEC text tightening (Minor; next SPEC writing-pass):** Update SPEC §3 D6's `expectations.yaml` example to match the actual `Http1HeaderRule` deserializer shape (M4). Defers until phase 06+ writes a fresh SPEC that cross-references this one.

6. **Documentation sync of post-CI fixups (Awareness):** PROGRESS Task 13's narration of the parser rewrite (commit `dac3f8b`) is excellent; consider lifting the parser-shape lessons (`✔` U+2714 vs `✓` U+2713; section-context tracking; canonical summary line) into a brief note in SPEC §6 signpost 4 so the next conformance suite (`h3spec`, gRPC interop) starts from documented prior art rather than re-discovering.

7. **Refactor `MissingAuthority` to emit a 400 (Minor; M11):** The H1 sibling synthesizes a 400 on missing/empty `Host:` (`crates/envoy-http1/src/hcm.rs:335-345`); the H2 path silently drops the stream task. Aligning the two is RFC-correct and ergonomic for clients.

---

## §5 Carryforward verdict

Phase 05.2 SPEC §4 explicitly defers the following surfaces to later sub-phases / phases. Each is correctly NOT touched in 05.2; this list is the carryforward inventory for the next reviewer:

**Deferred to 05.3 (next sub-phase):**

- Upstream H2C origination (`envoy-http2::Client`).
- Router H2-arm dispatch (`BuildOutcome::Proxy` → `envoy_http2::Client`, replacing the 502 stub at `crates/envoy-http2/src/hcm.rs:117-134`).
- Cluster-side `Http2ProtocolOptions` via `typed_extension_protocol_options.HttpProtocolOptions`.
- `Cluster.upstream_protocol: UpstreamProtocol { Http1, Http2 }` field.
- Fixture 0010 (`http2-router-upstream`).
- `tests/helpers/http2-echo-server/` workspace member.

**Carried forward by this REVIEW:**

- **I1** (CI tarball checksum) — hardening; close in 05.3 or as a standalone 05.x security-pass.
- **I2** (`Http2Error` variant misnaming at write paths) — close before 05.3 introduces additional H2 error sites.
- **I3** (`MalformedH2HeaderBlock` overload) — close in 05.3.
- **M2** (per-stream timeout) — fold into 05.3's upstream-H2 spawn site.
- **M3** (`h2_protocol_options_max_concurrent_streams_applied` `#[ignore]`-d) — track until upstream `h2` crate exposes deterministic SETTINGS observability.
- **M4** (SPEC §3 D6 expectations-YAML example shape drift) — close at next SPEC editing pass.
- **M5** (locator helper extraction) — defer until N=3 consumers.
- **M6** (h2spec gate diagnostic should surface skipped count) — trivial; close opportunistically in 05.3.
- **M8** (502 stub body literal mentions 05.3) — close at 05.3 entry; trivial.
- **M9** (SPEC §3 D7 h2spec config example missing/extra `node:` block) — close at next SPEC editing pass alongside M4.
- **M10** (`Driver::Http2` lacks `extra_headers` field) — close at 05.3 fixture 0010 wire-up.
- **M11** (RFC-soft `MissingAuthority` recovery) — fold into 05.3 or 06+.
- **M12** (garbage-preamble test permissive close-shape assertion) — trivial; close opportunistically.

**Closed in 05.2:**

- **Conditional ADR-0024** (lands as ADR-0027 with appropriate provenance per the established phase-03 / phase-05.4 renumbering precedent).
- **Conditional ADR-0028** (explicitly NOT landed; rationale recorded in PROGRESS Task 13 — gate-mechanics mechanically deterministic, no decision worth ADR-shaped permanent record).
- **Cross-phase C-1 carryforward** (substantively closed at 05.4 Task 7 verification commit `a8c2364`; 05.2 inherits the GREEN baseline cleanly via CI run `25294149612` — all 9 Docker-gated fixtures green simultaneously).
- **The `BuildOutcome::Proxy` codepath compiling-but-stubbed posture** (per SPEC §4 "not deferred — confirmed in scope" item 4): the 502-stub at `crates/envoy-http2/src/hcm.rs:117-134` correctly typechecks against the route-walk's `BuildOutcome::Proxy` arm and the stream task drops cleanly.

**Standing inventory (no change in 05.2):**

- **Phase-04.1 REVIEW M-architectural-claim** (`drive_http1` per-function unit test): 05.2 does not extend the harness in a way that would add a third `Driver::Http1` consumer (the new `Driver::Http2` is parallel, not nested), so M-claim continues unchanged.
- **Phase-04.1 REVIEW M5/M9** (Cargo.lock cadence): phase 05.2 continues the inline-at-scaffold cadence per parent-05 SPEC §6 signpost 14. No policy change; M5/M9 carry forward to whichever phase ratifies a single cadence.
- **Phase-02.2 REVIEW M1** (SIGKILL-on-Drop posture for envoy-bin subprocess): `crates/envoy-bin/tests/http2_direct_response.rs` mirrors the existing posture (`kill_on_drop(true)` + bare `drop(child)`); 05.2 does not parallelize `run_fixture` so M1 continues unchanged per SPEC §6 local signpost 22.
- **Phase-04.1 REVIEW M7** (`TlsAcceptingHandler.inner` concrete-typed): re-deferred per parent-05 SPEC §4 to whichever phase ships ALPN-driven dispatch. 05.2 does not generalize; M7 continues unchanged. The H2+TLS rejection (`Http2OverTlsNotSupported` + the runtime symmetric bail at `crates/envoy-bin/src/main.rs:269-277`) is the explicit gate.
- **Phase-04.1 REVIEW M1/M2/M4** (header-diff value-comparison; body-drain idle; `strip_port` IPv6 handling): 05.2 H2 path uses parsed `h2::client` rather than wire-byte assertion, so these H1-flavored carryforwards are tangentially relevant only via fixture 0009's HTTP/1-shaped expectations (no duplicate headers, small body, DNS Host); none materially exercised by 05.2.
- **Phase-04.2 REVIEW M8/M9/M11** (config-diff opacity; ADR-0021 supersession; duplicate-header semantics): 05.2 does not exercise. M9 paired with M5 — see above.

---

## §6 Verification gate observation

The state-4 phase-done gate evidence per PROGRESS Task 14 / CI run `25294149612` HEAD `dac3f8b` is comprehensive:

- **Acceptance signal (a)** GREEN — fixture 0009 Docker-gated test passes. `test http2_direct_response_fixture ... ok` (0.85s wall).
- **Acceptance signal (b)** GREEN — all 8 pre-existing fixtures pass simultaneously, no regression on 0001-0008. PROGRESS Task 14 quotes the per-fixture matrix: `echo_fixture` 1.05s / `admin_ready_fixture` 7.06s / `tcp_proxy_fixture` 2.65s / `tls_downstream_fixture` 2.77s / `tls_sni_fixture` 3.04s / `tls_upstream_fixture` 2.68s / `http1_direct_response_fixture` 0.85s / `http1_router_upstream_fixture` 2.47s / `http2_direct_response_fixture` 0.85s — all `test result: ok`.
- **Acceptance signal (c)** GREEN — h2spec 144/146 pass = 99.31% (well above 95% gate); single failure `3.5/2` classified as foundation limitation per parent-05 SPEC §6 signpost 13. Gate (a) ≥95%, Gate (b) no surprise regressions, Gate (c) no stale entries — all pass by construction.
- **Acceptance signal (d)** GREEN — fuzz `parse_bootstrap` clean for 30s with the new `hcm_codec_http2.yaml` corpus seed exercising the validator's HTTP2 + Http2ProtocolOptions accept-paths. `✓ fuzz (parse_bootstrap, 30s) in 1m5s (ID 74150226090)`.
- **Acceptance signal (e)** GREEN — `cargo build`, `cargo clippy -D warnings`, `cargo fmt --check`, `cargo test --workspace`, `cargo deny check` all clean. The single ignored test (`h2_protocol_options_max_concurrent_streams_applied`) is documented (M3) and not blocking. `cargo deny check` final line: `advisories ok, bans ok, licenses ok, sources ok` (5 pre-existing benign `license-not-encountered` advisory-only warnings on 0BSD / BSD-2-Clause / MPL-2.0 / Unicode-DFS-2016 / Zlib unmatched allowances — unchanged from the 05.4 baseline; do not represent new licenses brought in by 05.2).

The two iterative state-3 fixup commits (`f6ee022` task 10 review fixups, `3f117a3` task 11 review fixups, `f6a0ad4` task 13 review fixups, plus the post-CI-run parser fixup `dac3f8b`) are well-scoped and individually justified. PROGRESS records each fixup with explicit references to which review item is closed, which is deferred, and why.

The ADR-0027 landing matches the projected ADR-0024 conditional, with renumbering provenance correctly recorded. The single new ADR is the right shape (parallel to ADR-0021's `regex` scoping).

**No state-4 gate evidence is missing or weakly substantiated.**

---

## §7 Final verdict

**Approved with M-track follow-ups.**

Phase 05.2 cleanly engages Row 4 (HTTP/2 framing) of `BEHAVIOR_CONTRACT.md` for the first time in the project's history without requiring contract edits, lands the workspace's first conformance suite at a non-trivial 99.31% pass rate, preserves the cross-sub-phase architectural rule that `envoy-http2` is the sole `h2`-depending production crate, and inherits 05.4's restored Docker-gated baseline cleanly with all 9 fixtures green simultaneously. The PROGRESS narration is exemplary and the post-CI parser rewrite is a model of how to handle "the foundation behaves slightly differently than the SPEC anticipated" without scope creep.

Three Important findings (I1 CI tarball checksum, I2 error variant misnomer at write paths, I3 `MalformedH2HeaderBlock` overload) carry forward to 05.3 / 05.x hardening — all are scoped, mechanical, hardening-or-ergonomic in character, none are correctness bugs, and the reviewer recommends the cross-phase carryforward path (parallel to phase-04.3's C-1 disposition) rather than re-entering state 3. Twelve Minor findings cluster around test-shape ergonomics, locator-helper duplication, and SPEC-text drift; none gate phase-done.

**Phase 05.2 is approved for state-6 close-out.** 05.3 carries I1+I2+I3+M2+M4+M5+M6+M8+M9+M10+M11+M12 forward; the standing inventory carryforwards (M1 phase-02.2, M5/M9 phase-04.1 Cargo.lock cadence, M7 phase-04.1 TLS+H2 generalization, M-claim phase-04.1 `drive_http1` per-function) are unchanged.
