# Phase 06.2 — `envoy-accesslog` foundation + Envoy default format + HCM access-log wiring + fixture 0012 + BEHAVIOR_CONTRACT.md `Access log field mapping` first-time population

- **Phase id:** `06.2`
- **Parent phase:** `06-observability` (split per **ADR-0029**; parent SPEC at `docs/envoy-rust/phases/06-observability/SPEC.md`, committed at parent-06 state-1 close-out commit).
- **Slug:** `06.2-access-log`
- **Title:** Land the access-log subsystem foundation: a new workspace member `crates/envoy-accesslog/` (sole-dep-owner of any access-log surface; mirrors `envoy-http1`'s sole-owner-of-`httparse` posture from 04.1, `envoy-tls`'s sole-owner-of-`rustls` from 03.1, `envoy-http2`'s sole-owner-of-`h2` from 05.2, and `envoy-stats` / `envoy-admin`'s sole-ownership posture from 06.1) shipping `AccessLogRecord` (15-field struct), a concrete `FileSink` (no `Sink` trait yet — option (c) per parent SPEC §3 D8.2), `default_format::format` emitter, and a hand-rolled ISO-8601 timestamp emitter from `std::time::SystemTime` with a Gregorian calendar arithmetic helper + golden tests + `envoy-config` schema growth (`HttpConnectionManagerConfig.access_log: Vec<AccessLogConfig>`; the validator gates on `name = "envoy.access_loggers.file"` + `@type = type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog` and rejects others with `ConfigError::UnsupportedAccessLogType { actual: String }`) + HCM access-log dispatch wiring (HCMConfig grows an `access_log: Vec<FileSink>` field; HCM at on-response-complete time builds an `AccessLogRecord` and dispatches fire-and-forget to each sink; emission errors logged via `tracing::warn!`; H2 inherits via the `HCMConfig` type-alias landed in 05.2 D1) + fixture `0012-access-log-file-sink` (HCM emits one access-log line per request to a file sink; both proxies' files are diffed semantically per the access-log field-mapping rule populated in BEHAVIOR_CONTRACT.md at this sub-phase's first-fixture commit) + harness `Driver::Http1WithAccessLog` + per-token `AccessLogLineRule` + Docker-gated `tests/differential/tests/access_log_file_sink.rs` + the BEHAVIOR_CONTRACT.md `Access log field mapping` section first-time population (one row per default-format token; the section's standing comment *"populated in phase 06 when access logs first ship"* is fulfilled at this sub-phase's first-task or first-fixture commit per parent SPEC §2.2).
- **Depends on:** `06.1` (sub-phase ROADMAP row `done` after 06.1's state-6 phase-done commit; the `envoy-stats` registry that 06.3's `http.<prefix>.access_logs_total` counter will plug into is registered in 06.1, but 06.2 does NOT touch the stats registry — the `access_logs_total` counter lands in 06.3 D15.3 per parent SPEC §3, and 06.2's HCM access-log wiring intentionally avoids any stats coupling so the access-log subsystem can be reasoned about in isolation; the dependency on 06.1 is therefore strictly ordering-on-the-execution-chain, not a load-bearing API consumption). Strictly precedes `06.3` (comprehensive stats wiring + 05.3 I1 closure + parent-06 close).
- **Seeded by:** parent-06 SPEC §1 layer 2 (the goal-paragraph for sub-phase 06.2 — *"Access-log foundation + HCM wiring + file sink"*), §2.2 (the `Access log field mapping` projection — the 14-token table this SPEC's §2 expands into draft form), §3 D8.2–D13.2 (the six 06.2 deliverables this SPEC's §3 refines into per-deliverable detail), §4 (non-goals — the 06.2-binding subset, especially the format-string-customization deferral, the sinks-beyond-FileSink deferral, the JSON-format deferral, the `%FILTER_STATE%`/`%DYNAMIC_METADATA%` deferral, the access-log filtering deferral, the admin-side access logs deferral), §6 cross-sub-phase architectural rules 1 (sole-dep-owner — `envoy-accesslog`) and 4 (fire-and-forget HCM emission), §7 ADR projection (ADR-0029 already landed at parent-06 state-2; conditional ADR-0030 — foundations grant for `async_trait = "0.1"` or `time = "0.3"` — explicitly NOT recommended; this SPEC commits to option (c) per D8.2 + the hand-rolled ISO-8601 emitter), §8 state-machine signposts.
- **Differential surface when done:**
  - **Pre-existing fixtures unchanged in 06.2 (must remain green at 06.2 state-4):** `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, `tests/fixtures/0007-http1-direct-response/`, `tests/fixtures/0008-http1-router-upstream/`, `tests/fixtures/0009-http2-direct-response/`, `tests/fixtures/0010-http2-router-upstream/`, `tests/fixtures/0011-admin-stats-prometheus/` — all 11 must remain green at the Docker-gated CI level. Fixture `0011-admin-stats-prometheus` (landed in 06.1) inherits its representative-stats-subset assertion shape unchanged in 06.2 (the comprehensive-stat-set extension lands in 06.3 D17.3, not 06.2).
  - **New fixture green:** `tests/fixtures/0012-access-log-file-sink/` — H1 downstream listener with HCM `codec_type: HTTP1` + an `access_log:` block selecting the file access logger + a single VH `domains: ["*"]` + a single route `prefix: "/"` `direct_response { status: 200, body: { inline_string: "ok\n" } }`. After the request completes, the harness reads the configured access-log file path from each proxy's config and diffs the contents per the per-token equivalence rules populated in BEHAVIOR_CONTRACT.md `Access log field mapping`.
  - **Conformance suite unchanged in 06.2:** `tests/conformance/h2spec/` continues at the **≥95% pass** gate landed in 05.2 D7. 06.2 does not engage H2-framing surfaces; the access-log subsystem is codec-agnostic (the HCM emits records on the H1 path; the H2 path inherits via the `HCMConfig` type-alias from 05.2 D1 but fixture 0012 exercises the H1 path only — the H2-side access-log integration is structurally exercised but not asserted by a fixture in 06.2). The 06.2 state-4 verification re-runs h2spec to confirm no regression.

This SPEC is the design contract for sub-phase 06.2. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed phase-00 through phase-06.1 surface (via `git log` and the in-tree `crates/envoy-{accesslog (NEW in 06.2), admin (06.1), bin, cluster, config, http1, http2, listener, stats (06.1), tcp, tls}` shape at sub-phase-06.1 close + the 11 in-tree fixtures + the `tests/conformance/h2spec/` runner crate) must be able to execute it without consulting the parent `06-observability/SPEC.md`. The parent SPEC's projection rules for the `Access log field mapping` table (parent §2.2) are reproduced verbatim in §2 below for that reason.

---

## 1. Goal and acceptance signal

**Goal.** Land the access-log subsystem foundation in five coordinated layers that all ship in this single sub-phase:

1. **New workspace member `crates/envoy-accesslog/`.** Sole-dep-owner of any access-log surface per cross-sub-phase architectural rule 1 from parent-06 SPEC §6 (no other workspace crate calls out to access-log primitives directly; HCM consumers import `envoy_accesslog::*` types). Cargo deps: `tokio = { version = "1", features = ["fs", "io-util", "sync"] }` (for the `FileSink`'s `tokio::fs::OpenOptions::append(true)` + `AsyncWriteExt::write_all` posture + the `tokio::sync::Mutex<File>` serialization), `bytes = "1"` (for `Bytes`-shape buffer interactions if needed; likely no-op for the default-format emitter which emits `String`), `tracing = "0.1"`, `thiserror = "2"`, `envoy-http1 = { path = "../envoy-http1" }` (consumed for the `Request` / `Response` value-types — the access-log record borrows references to these via owned-String field projection at HCM-construction time, not at HCM-call time, so no lifetime coupling crosses the crate boundary). **No new permitted-foundations grants** under the recommended posture per parent-06 SPEC §6 architectural rule 1: `time = "0.3"` and `chrono = "0.4"` and `async_trait = "0.1"` are **NOT** on D-3.2's permitted-foundations list, and 06.2 does NOT add them. ISO-8601 timestamp emission is hand-rolled from `std::time::SystemTime` with a Gregorian calendar arithmetic helper (a ~50 LoC `epoch_seconds_to_ymd_hms_millis` function with golden tests; see signpost 1 in §6 below). The `Sink` trait is **deferred** per parent SPEC §3 D8.2 option (c) — `FileSink` ships concretely; trait + multi-sink dispatch defer to whichever later observability-family phase first ships a second sink type (gRPC ALS sink, stdout sink, or similar). Crate root `lib.rs` carries `#![forbid(unsafe_code)]` per D-3.8.

2. **`envoy-config` schema additions for `access_log:` on `HttpConnectionManagerConfig`.** In `crates/envoy-config/src/bootstrap.rs`: (a) extend `HttpConnectionManagerConfig` with an optional `access_log: Vec<AccessLogConfig>` field (default-empty; absent block parses cleanly as the empty Vec). (b) Introduce the `AccessLogConfig` struct mirroring Envoy's `envoy.config.accesslog.v3.AccessLog` proto (subset shipped: `name: String` + `typed_config: TypedConfig` — the existing typed_config envelope from phase 04.1 / 05.3, no new envelope shape needed). (c) Validator extension at the existing `validate_hcm` site: for each `AccessLogConfig` entry, gate on `name = "envoy.access_loggers.file"` AND the `typed_config`'s `@type` URL is exactly `type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog`; other `name`/`@type` combinations reject with `ConfigError::UnsupportedAccessLogType { actual: String }`. (d) Inside the `FileAccessLog` typed_config, extract the `path: String` field (the on-disk file path the sink writes to). Format-string customization is **OUT** of scope in 06.2 — even though the `FileAccessLog` proto carries a `format` field (and a `log_format` oneof), the validator in 06.2 ignores any user-supplied format strings and always emits the Envoy default format. Future format-string parsing defers per §4 below. The fuzz corpus extension is one new seed (`hcm_access_log_file.yaml`; a full bootstrap with one HCM listener whose `access_log:` carries one file-sink entry).

3. **HCM access-log wiring at the on-response-complete site.** `envoy-http1::HCMConfig` (the per-listener immutable config struct landed in 04.1, extended in 04.3) gains an optional `access_log: Vec<Arc<FileSink>>` field. At HCM-on-response-complete time (the existing `write_response` call site at `crates/envoy-http1/src/hcm.rs` per the 04.1-landed shape that 04.3 / 05.2 / 05.3 each consumed unchanged), the HCM captures request + response + timing + upstream-host state into an `AccessLogRecord` value and dispatches the record to each configured sink via `sink.emit(&record).await` (signature per D1 below). The dispatch is **fire-and-forget** per parent SPEC §6 architectural rule 4 (Rule 4 binds load-bearingly here in 06.2): emission errors logged via `tracing::warn!`, do NOT propagate up the response-write path, do NOT cause the HCM to retry or block, do NOT affect the response bytes that have already been written downstream. The H2 path inherits the wiring transparently via the `HCMConfig` type-alias from 05.2 D1 (`envoy_http2::HCMConfig = envoy_http1::HCMConfig`); the on-response-complete site at `crates/envoy-http2/src/hcm.rs` (the 05.2-landed listener-side dispatch) makes the symmetric `record + dispatch` call. **Stats coupling**: NONE in 06.2. The `http.<stat_prefix>.access_logs_total` counter that increments at queue-enter time per parent §6 Rule 4 lands in 06.3 D15.3, not 06.2. The 06.2 HCM dispatch site adds NO counter increments, NO gauge updates, NO registry interactions.

4. **Differential harness extensions for HTTP/1.1 + access-log assertion.** New `Driver::Http1WithAccessLog { method, path, host, expected_status, expected_body, expected_headers, expected_access_log_lines: Vec<AccessLogLineRule> }` variant on the existing `Driver` enum at `tests/differential/src/lib.rs` (sibling of 04.1's `Driver::Http1` / 04.2's `Driver::Http1ProbeList` / 05.2's `Driver::Http2` / 06.1's `Driver::AdminScrape`). Drives a single `GET /` HTTP/1.1 request via the existing 04.1-landed `drive_http1` helper (no new wire-driver helper in 06.2 — the access-log assertion is a post-request file read, not a wire-protocol extension); reads the configured access-log file path from the proxy's config; opens the file; tokenizes each line per the Envoy default-format token grammar (a hand-rolled tokenizer in `tests/differential/src/access_log.rs`, ~80 LoC, handles only the default format — format-string customization is OUT of scope per §4 below); matches each token against the per-token rule. `AccessLogLineRule` is a per-token rule enum (`Exact(String)` / `Iso8601Format` / `DurationMs` / `Wildcard` / `EnvoyOnly` / etc. — full enumeration in D4 below). Fixture `tests/fixtures/0012-access-log-file-sink/` ships 5 files (`envoy.yaml`, `envoy-rust.yaml`, `inputs/payload.bin`, `expectations.yaml`, `README.md`) per the 04.x / 05.x / 06.1 fixture shape. Docker-gated test at `tests/differential/tests/access_log_file_sink.rs` is a 7-line wrapper calling `differential::run_fixture("0012-access-log-file-sink")`.

5. **`BEHAVIOR_CONTRACT.md` `Access log field mapping` section first-time population.** Per parent SPEC §2.2 — the standing comment *"populated in phase 06 when access logs first ship"* is fulfilled here. One row per default-format token; 14 tokens total in the Envoy default format (the fixed sequence from parent §2.2). Each row records: token name, equivalence disposition (`value-exact` or `name-required, value-may-differ`), rationale, envoy-rust internal data source. Lands at the **06.2 first-task or first-fixture commit** (per parent §3 D12.2; the planner's recommended posture is at the first-fixture commit so the table lands cleanly with the deliverable that first asserts on the rules). The table draft is reproduced in §2 below verbatim — D5.2 below transcribes the §2 table into the BEHAVIOR_CONTRACT.md document at the appropriate landing commit.

**Cross-phase items closed at 06.2.** None directly inside the 06.2 surface. The cross-phase carryforwards inherited from the parent-06 SPEC §4 / 05.x REVIEW chains (phase-04.1 REVIEW M1/M2/M4, phase-04.1 REVIEW M5/M9 Cargo.lock cadence, phase-05.2 REVIEW I1 h2spec tarball SHA, phase-05.3 REVIEW I1/I2, phase-02.2 REVIEW M1) all stay deferred unchanged through 06.2 close.

**Cross-phase items unblocked but not closed at 06.2.** None.

**Scope-shape inheritance from the parent-06 brainstorm.** The brainstorm explicitly bounded 06.2 to: codec scaffold (the new `envoy-accesslog` crate's primitives only — `AccessLogRecord` + `FileSink` + `default_format::format` emitter; NOT a `Sink` trait, NOT multi-sink dispatch, NOT format-string customization, NOT JSON-format access logs, NOT access-log filtering); schema growth (one new field on `HttpConnectionManagerConfig` plus the `AccessLogConfig` struct; the validator gates on the file-sink type URL only); HCM dispatch (the on-response-complete site grows a fire-and-forget access-log emission; no stats coupling; H2 inherits via the `HCMConfig` type-alias); fixture (0012 only); harness extensions (`Driver::Http1WithAccessLog` + `AccessLogLineRule`); BEHAVIOR_CONTRACT.md edit (the `Access log field mapping` section's first-time population; 14 rows; ~50 LoC of doc-only diff). This bounding is reproduced verbatim in §4 below as 06.2's non-goals.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 06.2's feature surface (the 6-part gate per the canonical phase-done shape):

- **(a)** the new differential fixture `tests/fixtures/0012-access-log-file-sink/` is green at the Docker-gated CI level, with the CI run URL + the test result quoted inline in `PROGRESS.md`;
- **(b)** the 11 pre-existing differential fixtures `tests/fixtures/{0001-tcp-echo,0002-static-admin-ready,0003-tcp-proxy,0004-tls-downstream,0005-tls-upstream,0006-tls-sni,0007-http1-direct-response,0008-http1-router-upstream,0009-http2-direct-response,0010-http2-router-upstream,0011-admin-stats-prometheus}/` remain green at the Docker-gated CI level (they are not edited in 06.2; their fixtures were green at sub-phase-06.1 close and continue green);
- **(c)** the conformance suite `tests/conformance/h2spec/` continues at **≥95% pass** with `known-failures.txt` unchanged (06.2 does not engage H2-framing surfaces; the H2-side access-log integration via the `HCMConfig` type-alias is structurally exercised but does not change H2 codec behavior); the 06.2 state-4 verification re-runs h2spec to confirm no regression;
- **(d)** the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 06.2 with **one new seed** (`crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_access_log_file.yaml`; a full bootstrap with one HCM listener whose `access_log:` carries one entry of `name: envoy.access_loggers.file` + `typed_config: { @type: ".../v3.FileAccessLog", path: "/tmp/fuzz.log" }`); no new fuzz target ships in 06.2;
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job. The `cargo deny check` clearance is a likely no-op (no new permitted-foundations grants under the recommended posture; the new `envoy-accesslog` crate's deps are all already on the workspace's resolved-graph cleared list);
- **(f)** `REVIEW.md` for this sub-phase is approved per `superpowers:requesting-code-review`.

The 06.2 state-6 phase-done commit flips ROADMAP row `06.2` from `in-progress` to `done`. Parent row `06` stays `in-progress` (06.3 is the closing sub-phase per parent SPEC §3 D19.3; parent flips at 06.3's state-6 commit per the ROADMAP-schema invariant *"the parent flips to `done` only after all sub-phases are `done`"*). STATE.md advances to phase `06.3` lifecycle state 2 (06.3's SPEC was already landed at parent-06 state-2 alongside this SPEC; the next session runs `superpowers:writing-plans` scoped to sub-phase 06.3).

---

## 2. Behavior-contract scope for sub-phase 06.2

**Central deliverable.** The `Access log field mapping` section of `docs/envoy-rust/BEHAVIOR_CONTRACT.md` is **populated for the first time in the project's history** at this sub-phase. The section's standing comment *"populated in phase 06 when access logs first ship. Extended whenever a new filter adds new log-only fields."* is fulfilled at the 06.2 first-fixture commit (per D5.2 below). The §2 table here is the verbatim draft of the BEHAVIOR_CONTRACT.md row-for-row population — the planner copies these 14 rows + the prefatory rationale into BEHAVIOR_CONTRACT.md at the landing commit.

**Envoy default format reference.** The Envoy default access-log format (verifiable against upstream Envoy v1.33 docs at `https://www.envoyproxy.io/docs/envoy/v1.33.0/configuration/observability/access_log/usage#default-format`) is the fixed 14-token sequence (with one literal `"-"` separator stretch) that this sub-phase commits to reproducing exactly:

```
[%START_TIME%] "%REQ(:METHOD)% %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)% %PROTOCOL%" %RESPONSE_CODE% %RESPONSE_FLAGS% %BYTES_RECEIVED% %BYTES_SENT% %DURATION% %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)% "%REQ(X-FORWARDED-FOR)%" "%REQ(USER-AGENT)%" "%REQ(X-REQUEST-ID)%" "%REQ(:AUTHORITY)%" "%UPSTREAM_HOST%"
```

Tokens absent on a given record (e.g., `%REQ(USER-AGENT)%` when the request did not carry a `User-Agent:` header; `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%` on `direct_response` paths) emit `-` in their position per Envoy's substitution rule. Quoted tokens (e.g., `"%REQ(USER-AGENT)%"`) emit `"-"` (a literal `"-"` between the surrounding quotes). The `default_format::format` emitter in D1 below implements this substitution rule for every field of `AccessLogRecord` per the per-row mapping below.

### 2.1 Access log field mapping table — 06.2 first-time population (draft of BEHAVIOR_CONTRACT.md edit)

| Token | envoy-rust internal source | Equivalence disposition | Rationale |
|---|---|---|---|
| `%START_TIME%` | `AccessLogRecord.start_time: SystemTime`, formatted as `YYYY-MM-DDTHH:MM:SS.sssZ` (UTC, millisecond resolution) by the hand-rolled ISO-8601 emitter at `default_format::format_iso8601`. Captured at HCM `serve_connection` request-arrival time (i.e., immediately after the H1 codec produces the parsed `Request` value type, before any route-walk). | name-required, value-may-differ | Wall-clock non-determinism: both proxies stamp the response at slightly different instants (envoy-rust's HCM and Envoy's HCM are different processes/containers; their request-arrival timestamps differ by milliseconds at minimum). The harness asserts the field is present and parses-as-ISO-8601 (via the `AccessLogLineRule::Iso8601Format` rule per D4 below) but does NOT assert exact value. |
| `%REQ(:METHOD)%` | `AccessLogRecord.method: String`, sourced from `envoy_http1::codec::Request.method.as_str()` (e.g., `"GET"`, `"POST"`). The H2 path inherits via the request value-type translator landed in 05.2 D3 `request.rs` (which projects `:method` into `Request.method`). | value-exact | The harness sends a deterministic request; both proxies see the same method bytes. The `Request.method` field is HTTP method-token-set bound (`OPTIONS`/`GET`/`POST`/`HEAD`/`PUT`/`DELETE`/`TRACE`/`CONNECT`/`PATCH` per RFC 7231 §4); both proxies render it the same. |
| `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` | Envoy's substitution rule: emit `X-Envoy-Original-Path` request header value if present; else emit the `:path` pseudo-header (H2) / request-target (H1). envoy-rust internal source: `AccessLogRecord.path: String`, populated at HCM record-build time by checking `Request.headers` for `x-envoy-original-path` (lowercased per the 04.x lowercasing posture); if present, that value; else `Request.path` (the request-target the codec captured). | value-exact | Both proxies receive the same request bytes (the harness sends a fixed `GET /` with no `X-Envoy-Original-Path` header in fixture 0012's happy-path flow); both render the same path. The fallback semantic (`X-Envoy-Original-Path` first; `:path` second) is reproduced by envoy-rust's record-build logic per signpost 10 in §6 below. |
| `%PROTOCOL%` | `AccessLogRecord.protocol: String`, sourced from the codec at HCM dispatch time. For H1 dispatch (envoy_http1::HCM): `"HTTP/1.1"` (HTTP/1.0 not supported per 04.1's `httparse` posture; `httparse` rejects 1.0-only requests at the codec layer in 04.1's posture — actually httparse accepts both, but envoy-rust's request shape doesn't track minor-version explicitly, so the literal `"HTTP/1.1"` is emitted regardless; cross-checked at signpost 10 in §6 below). For H2 dispatch (envoy_http2::HCM, via the type-aliased HCMConfig): `"HTTP/2"`. | value-exact | The protocol is determined by which codec dispatched the request; both proxies emit the same string for the same dispatch. Fixture 0012 exercises H1 dispatch only, so the asserted value is `"HTTP/1.1"`. |
| `%RESPONSE_CODE%` | `AccessLogRecord.response_code: u16`, sourced from `envoy_http1::codec::Response.status` (which 04.1 stores as `u16` at `Response.status: u16`; verifiable at planner cross-check time in `crates/envoy-http1/src/codec.rs` at the `pub struct Response` definition). Rendered as the decimal form (e.g., `200`, `404`, `503`). | value-exact | Both proxies route the same request through the same VH/route/action; both produce the same response code. Fixture 0012's direct_response action returns `200`; both proxies' access-log lines render `200` in this position. |
| `%RESPONSE_FLAGS%` | `AccessLogRecord.response_flags: String`, populated at HCM record-build time from a pseudo-bitfield of envoy-rust HCM flags (none of which fire on the happy-path `direct_response` flow — see signpost below). The default value is the literal `"-"` (Envoy's "no flags" sentinel). 06.2 ships ONLY the no-flags case; non-`-` flag combinations defer to whichever phase first surfaces them (e.g., `UH` for "no healthy upstream" lands when health-checking ships; `URX` for "the request was rejected because the upstream connection was reset" lands when upstream connection management evolves). | value-exact (for the `-` no-flags case in fixture 0012; tightens per future flag introductions) | Fixture 0012's direct_response happy-path produces `-`; both proxies emit `-`. Future fixtures that exercise non-`-` flag combinations will need per-flag equivalence rules; that's a future-phase concern, not 06.2's. |
| `%BYTES_RECEIVED%` | `AccessLogRecord.bytes_received: u64`, populated at HCM record-build time from `Request.body.len() as u64`. The pre-04.x request body shape is `bytes::Bytes` (verifiable at planner cross-check time in `crates/envoy-http1/src/codec.rs`); its `len()` is the wire-byte count of the request body received from the downstream codec. Header bytes are NOT counted (Envoy's `%BYTES_RECEIVED%` token measures body bytes only per the v1.33 docs cross-check). | value-exact | Both proxies receive the same request body bytes (fixture 0012 sends a GET with no body — `%BYTES_RECEIVED% = 0`); both emit `0`. |
| `%BYTES_SENT%` | `AccessLogRecord.bytes_sent: u64`, populated at HCM record-build time from `Response.body.len() as u64`. Symmetric to `%BYTES_RECEIVED%`. | value-exact | Both proxies render the same response body bytes (fixture 0012's direct_response emits `"ok\n"` = 3 bytes); both emit `3`. |
| `%DURATION%` | `AccessLogRecord.duration: Duration`, captured at HCM record-build time from a `start.elapsed()` call where `start: Instant` was captured at `serve_connection` entry. Rendered as the decimal millisecond count (e.g., `5`, `12`, `42`). The `%DURATION%` token's units are **milliseconds** per the Envoy v1.33 docs cross-check (signpost 9 in §6 below confirms ms vs μs choice). | name-required, value-may-differ | Per-request wall-clock latency: the two proxies serve the request through different runtimes, different HCM implementations, different I/O subsystems; their durations diverge by measurement. The harness asserts the field is present and parses-as-non-negative-integer (via `AccessLogLineRule::DurationMs` per D4) but does NOT assert exact value. |
| `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%` | `AccessLogRecord.upstream_service_time: Option<Duration>`, populated at HCM record-build time by checking the response headers for `x-envoy-upstream-service-time` (case-insensitive). When present (router-proxy path per 04.3's emission), rendered as the decimal millisecond count of the header value. When absent (direct_response path per 04.1's posture; the header is never emitted on direct_response paths per 04.3 BEHAVIOR_CONTRACT.md row), rendered as the literal `-`. | name-required, value-may-differ (when present); value-exact `-` (when absent on direct_response paths) | The header value's equivalence disposition is inherited from the existing 04.3-landed `Header allow-list` row for `x-envoy-upstream-service-time` (which reads "name-required, value-may-differ" per BEHAVIOR_CONTRACT.md). Fixture 0012's direct_response path produces the absent case (`-`); both proxies emit `-`; value-exact match. Future fixtures exercising the router-proxy path will produce non-`-` values that diverge by measurement. |
| `%REQ(X-FORWARDED-FOR)%` | `AccessLogRecord.forwarded_for: Option<String>`, populated at HCM record-build time by reading `Request.headers` for `x-forwarded-for` (lowercased). When present, the header value verbatim (Envoy's `%REQ(...)%` token does NOT modify the value — it forwards the request-side header bytes). When absent, the literal `-`. | value-exact | The harness sends a deterministic request; if it includes an `X-Forwarded-For` header, both proxies see the same bytes and render the same value; if it omits the header, both render `-`. Fixture 0012's request omits this header; both proxies emit `-`. |
| `%REQ(USER-AGENT)%` | `AccessLogRecord.user_agent: Option<String>`, sourced symmetrically to `%REQ(X-FORWARDED-FOR)%` from `Request.headers` for `user-agent`. | value-exact | Same rationale as `%REQ(X-FORWARDED-FOR)%`. The harness's `drive_http1` helper sends a deterministic `User-Agent:` (or omits it cleanly); both proxies see the same bytes. |
| `%REQ(X-REQUEST-ID)%` | `AccessLogRecord.request_id: Option<String>`, sourced symmetrically. The 04.x-landed envoy-rust HCM does NOT inject `x-request-id` on the request side (per 04.3 SPEC §4 non-goal — `generate_request_id: false` posture; carryforward unchanged through 06.2). Envoy's HCM by default DOES inject; fixture 0012's `envoy.yaml` sets `generate_request_id: false` to align both proxies on the omit-injection posture (mirrors fixture 0008's pattern from 04.3). | value-exact | Both proxies omit injection; both proxies see the same request bytes (the harness's GET does not include `X-Request-ID` either); both emit `-`. |
| `%REQ(:AUTHORITY)%` | `AccessLogRecord.authority: Option<String>`, populated at HCM record-build time by reading the `Host:` header on the H1 path (which `envoy_http1::codec` produces from the request-target `Host:` line) OR the `:authority` pseudo-header on the H2 path (which `envoy_http2::request` synthesizes a `Host:` header from per 05.2 D3's `:authority → Host:` adapter). Either way, the value reaches `AccessLogRecord` via `Request.headers`'s `host` row (lowercased). | value-exact | Both proxies receive the same `Host:` header bytes for the same wire-level request; both render the same value. The harness in fixture 0012 sends `Host: envoy-rust.test` (per 04.x precedent); both proxies emit `"envoy-rust.test"`. |
| `%UPSTREAM_HOST%` | `AccessLogRecord.upstream_host: Option<String>`, populated at HCM record-build time by snapshotting the upstream endpoint that the router-arm dispatched to (the `addr:port` formatted string of the resolved upstream `SocketAddr`). When the request did NOT proxy upstream (direct_response path), the literal `-`. The format is `addr:port` (e.g., `127.0.0.1:8080`); see signpost 11 in §6 below for the formatting rule. | value-exact for `-` (direct_response path, fixture 0012); value-exact for resolved upstream when both proxies' `STRICT_DNS` resolution returns the same endpoint (deterministic in fixture-time); name-required, value-may-differ if the resolution is non-deterministic (multi-A-record) | Fixture 0012's direct_response path produces `-`; both proxies emit `-`. Future fixtures with a router-proxy path will need per-fixture endpoint determinism (matches the 04.3 fixture 0008 / 05.3 fixture 0010 posture: `STRICT_DNS` with a single-A-record resolution returns a single deterministic endpoint). |

**Header allow-list — unchanged in 06.2.** The 04.3-landed 3-row allow-list (`server`, `date`, `x-envoy-upstream-service-time`) is unedited in 06.2. Fixture 0012's response surface is the existing direct_response shape that 04.1 / 05.2 already exercised; no new response headers surface in 06.2 that would warrant an allow-list extension.

**Stat-name mapping — unchanged in 06.2.** The `http.<stat_prefix>.access_logs_total` counter that parent SPEC §3 D15.3 lands in 06.3 IS the natural extension of the `Stat-name mapping` section once access-log emission is wired; in 06.2 the counter does not exist yet, so no `Stat-name mapping` row is added in 06.2. The 06.1-landed representative-stats-subset rows (`listener.<name>.downstream_cx_total`, `cluster.<name>.upstream_cx_total`, `http.<stat_prefix>.downstream_rq_total`) stay unchanged in 06.2.

**xDS wire / Timing tolerances — untouched.** 06.2 does not engage xDS or timing-sensitive features.

**Equivalence-matrix engagement** (per `BEHAVIOR_CONTRACT.md` §7.2's row-by-row breakdown):

- **Row 1 (Response status), Row 2 (Response body), Row 3 (Response headers), Row 4 (HTTP/2 & HTTP/3 framing), Row 5 (TLS handshake), Row 6 (TLS cert validation), Row 8 (TCP-stream byte equivalence)** — N/A in 06.2 *as new engagements* (they were already engaged by predecessor sub-phases for fixture 0012's HTTP/1.1 direct_response surface; the rows continue to apply for the same reasons they applied at fixture 0007 / 0011).
- **Row 7 (Access log records)** — **engaged for the first time in the project's history.** The contract row reads *"Semantically equal after field-mapping"*. 06.2's harness `Driver::Http1WithAccessLog` reads the access-log file from each proxy and matches per-token rules against the per-row equivalence dispositions enumerated in the §2.1 table. The harness does NOT byte-compare lines; the per-token disposition determines the rule shape (Exact / Iso8601Format / DurationMs / Wildcard / etc.). The §2.1 table is the canonical source of truth for which token has which disposition; the BEHAVIOR_CONTRACT.md edit at the first-fixture commit lands the table verbatim.

---

## 3. Deliverables

This section enumerates the six 06.2 deliverables corresponding to the parent SPEC §3 D8.2–D13.2 projection, refined into per-deliverable PLAN-ready cadence. Each deliverable is numbered for cross-reference; cross-sub-phase architectural rules inherited from parent §6 are listed first for visibility.

### Cross-sub-phase architectural rules (inherited from parent SPEC §6)

These rules hold across all three sub-phases of parent phase 06; sub-phase 06.2 inherits them verbatim. Rules 1, 4, and the new Rule (06.2-local) about Sink trait deferral are load-bearing here.

**Rule 1 — `envoy-accesslog` is the SOLE workspace dep on any access-log surface.** No other workspace crate calls out to access-log primitives directly. Phase 06 introduces no new permitted-foundations grants under the recommended posture (no `time`, no `chrono`, no `async_trait`); ISO-8601 emission is hand-rolled. **Bearing on 06.2:** load-bearing. 06.2 introduces `crates/envoy-accesslog/Cargo.toml` with no entries beyond `tokio` / `bytes` / `tracing` / `thiserror` / `envoy-http1` (workspace path-dep). The HCM in `envoy-http1` consumes `envoy_accesslog::{AccessLogRecord, FileSink, default_format}`; the HCM does NOT call out to filesystem APIs directly, ISO-8601 emission code directly, or any access-log primitives outside `envoy_accesslog::*`. The H2 path inherits via the `HCMConfig` type-alias from 05.2 D1 (the alias `envoy_http2::HCMConfig = envoy_http1::HCMConfig` carries the new `access_log: Vec<Arc<FileSink>>` field transparently; `envoy-http2` does NOT add a direct dep on `envoy-accesslog`).

**Rule 2 — `envoy-accesslog` exports record / sink primitives only; the HCM at `envoy-http1` builds the record and dispatches.** `envoy-accesslog` does NOT know about HCM, listeners, clusters, or HTTP request/response semantics beyond the `Request` / `Response` value-type imports it consumes from `envoy-http1` at field-projection time. **Bearing on 06.2:** load-bearing. The `AccessLogRecord` struct's fields are POD-shaped (owned `String`s, primitive numerics, `Option<String>`s, `SystemTime`, `Duration`); record construction lives at the HCM consumer side, NOT inside `envoy-accesslog`. This preserves the dependency direction — `envoy-accesslog` is a foundation library, not an integration layer (mirrors `envoy-stats`'s posture from 06.1 / `envoy-http2`'s posture from 05.2 / `envoy-http1`'s posture from 04.1).

**Rule 3 — `Sink` trait is DEFERRED (option (c) per parent SPEC §3 D8.2).** `FileSink` ships concretely. Trait + multi-sink dispatch lands when N≥2 sink types exist (likely a later observability-family phase introducing a gRPC ALS sink, stdout sink, or similar). **Bearing on 06.2:** load-bearing. The HCM's `access_log: Vec<Arc<FileSink>>` field is typed concretely on `FileSink`, NOT on `Box<dyn Sink>` or `Arc<dyn Sink>`. The `FileSink::emit(&self, record: &AccessLogRecord) -> Result<(), AccessLogError>` is an inherent `async fn` (hand-rolled `Future`-returning if the planner picks Pin-Box; or `async fn` directly if the trait shape permits — see signpost 4 in §6 below). The trait deferral avoids the `async_trait = "0.1"` foundations grant that parent SPEC §3 D8.2 option (b) would have required.

**Rule 4 — Access-log emission is fire-and-forget at the HCM site.** Access-log emission errors must NOT affect the response-write path. The HCM dispatches the record to the configured sinks; sink errors are logged via `tracing::warn!`. The HCM does NOT await sink emission completion before writing the response. **Bearing on 06.2:** load-bearing. The HCM emits the access-log record AFTER `write_response` returns (the response bytes have already been pushed to the downstream codec); the emission is therefore inherently outside the response-write critical path. The planner picks between two patterns at PLAN-write time (see signpost 4 in §6 below): (a) `tokio::spawn` direct (fire-and-forget; the spawned task carries the cloned `Arc<FileSink>` + the owned `AccessLogRecord` and emits asynchronously; the HCM's per-request task returns immediately after spawn; emission errors logged inside the spawned task), or (b) synchronous-after-write (the HCM awaits `sink.emit().await?` after `write_response` returns; the per-request task duration extends by the sink emission latency; this is "fire-and-forget" semantically because emission errors are still mapped to `tracing::warn!` rather than propagating; but the dispatch is sequential, not concurrent). **Recommendation:** option (b) synchronous-after-write — simpler reasoning, no `Arc` cloning into a spawned task, no spawned-task-leak concerns, fixture 0012's direct_response path is small enough that the latency cost is negligible; option (a) becomes attractive only when sinks become I/O-heavy (gRPC ALS, network sinks) which is post-06.2. The signpost lays out both options for the planner; this SPEC's recommendation is (b).

**Rule 5 — Format is fixed to Envoy default format.** Format-string customization (`%REQ(...)` variants beyond the default-format set; `%START_TIME(format-string)%` etc.) defers to a later phase. **Bearing on 06.2:** load-bearing. The validator at D2.2 ignores any user-supplied `format` field on the `FileAccessLog` typed_config — it does NOT parse format strings, it does NOT validate format strings, it does NOT emit non-default-format access logs. The `default_format::format` emitter produces ONLY the fixed 14-token sequence per §2 above.

### D1.2 — New library crate `crates/envoy-accesslog/`

New library crate at `crates/envoy-accesslog/`; appended to root `Cargo.toml` `[workspace] members` alongside the existing `envoy-{admin, bin, cluster, config, http1, http2, listener, stats, tcp, tls}`, `tests/differential`, `tests/conformance/h2spec`, `tests/helpers/{tcp,tls,http1,http2}-echo-server` entries. Sole-dep-owner of any access-log surface per cross-sub-phase architectural rule 1.

**`Cargo.toml`:**

```toml
[package]
name = "envoy-accesslog"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_accesslog"
path = "src/lib.rs"

[dependencies]
tokio = { version = "1", features = ["fs", "io-util", "sync"] }
bytes = "1"
tracing = "0.1"
thiserror = "2"
envoy-http1 = { path = "../envoy-http1" }

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "test-util", "time"] }
tempfile = "3"   # only if planner picks tempfile for FileSink unit tests; otherwise ad-hoc tempdir via env::temp_dir()
```

The `tempfile = "3"` dev-dep is conditional on the planner's choice in signpost 6 of §6 below; if the planner uses `std::env::temp_dir()` + a deterministic-named sub-path with cleanup-in-test-Drop, no `tempfile` dep is needed. **Recommendation:** use `tempfile = "3"` as a dev-dep — it's already on D-3.2's permitted-foundations list (transitive surface in the workspace) and avoids reinvention. If `tempfile` is NOT already in the workspace's resolved graph at 06.2 task time, the planner falls back to ad-hoc tempdir.

**Module decomposition** (per parent SPEC §3 D8.2's projection):

```
crates/envoy-accesslog/src/
  lib.rs            // crate root: #![forbid(unsafe_code)]; public re-exports
  record.rs         // AccessLogRecord struct + builder
  sink.rs           // (placeholder; the Sink TRAIT does NOT ship in 06.2; this file is empty
                    //  or carries doc-only commentary explaining the deferral per Rule 3 above)
  file_sink.rs      // FileSink concrete impl (tokio fs append + tokio Mutex<File> serialization)
  default_format.rs // Envoy default-format emitter + ISO-8601 helper + Gregorian arithmetic
  error.rs          // AccessLogError typed-error enum
```

The `sink.rs` module is a **placeholder file** in 06.2 — it carries a doc-only header explaining "the `Sink` trait is deferred per parent-06 SPEC §3 D8.2 option (c); FileSink ships concretely in `file_sink.rs`. Future observability-family phases will ship the trait + multi-sink dispatch when N≥2 sinks exist." The planner may instead omit the file entirely and carry the deferral comment as a top-of-`lib.rs` doc comment; recommendation is to keep `sink.rs` as a placeholder for module-decomposition stability (so the trait can land in a later phase by editing `sink.rs` rather than introducing a new module).

**Public surface re-exported at `lib.rs`:**

```rust
#![forbid(unsafe_code)]

//! envoy-accesslog — access-log subsystem foundation: record value-type,
//! concrete file-sink, Envoy default-format emitter.
//!
//! Owns the workspace's only direct surface for access-log primitives. The
//! HCM at envoy-http1 builds AccessLogRecord values and dispatches via
//! FileSink::emit; no other workspace crate calls FileSink or the
//! default-format emitter directly.
//!
//! The Sink trait is intentionally NOT shipped in this version. See
//! parent-06 SPEC §3 D8.2 option (c) and 06.2 SPEC §3 architectural rule 3.
//! When N≥2 sink types exist (gRPC ALS sink, stdout sink, etc.), a
//! future phase will ship the trait + multi-sink dispatch in this crate.

pub mod record;
pub mod file_sink;
pub mod default_format;
mod error;
mod sink;  // placeholder; see module-level doc comment

pub use record::AccessLogRecord;
pub use file_sink::FileSink;
pub use error::AccessLogError;
```

**`AccessLogRecord` struct** (in `record.rs`; the 15-field struct per parent SPEC §3 D8.2):

```rust
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct AccessLogRecord {
    pub start_time: SystemTime,
    pub method: String,
    pub path: String,
    pub protocol: String,           // "HTTP/1.1" or "HTTP/2"
    pub response_code: u16,
    pub response_flags: String,     // "-" by default in 06.2
    pub bytes_received: u64,
    pub bytes_sent: u64,
    pub duration: Duration,
    pub upstream_service_time: Option<Duration>,
    pub forwarded_for: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub authority: Option<String>,
    pub upstream_host: Option<String>,
}
```

The struct is `Clone` (so the HCM can cheaply clone it for the fire-and-forget dispatch path if option (a) of Rule 4 is chosen), `Debug` (for `tracing::warn!` formatting on emission errors). It is NOT `PartialEq` (no equality comparison required at the API boundary; the `default_format::format` emitter produces a `String` and the harness asserts on the rendered string per the per-token rules). It does NOT implement `Default` — every field must be populated explicitly at HCM record-build time; defaulting silently could mask omissions.

**`FileSink` concrete impl** (in `file_sink.rs`):

```rust
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::sync::Mutex;
use tokio::io::AsyncWriteExt;

use crate::record::AccessLogRecord;
use crate::default_format::format;
use crate::error::AccessLogError;

pub struct FileSink {
    path: PathBuf,
    handle: Arc<Mutex<File>>,
}

impl FileSink {
    pub async fn new(path: PathBuf) -> Result<Self, AccessLogError> {
        // Open with append(true), create(true). Returns AccessLogError::Open
        // on filesystem failure (permissions, parent-directory-missing, etc.).
        let file = tokio::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .await
            .map_err(|source| AccessLogError::Open { path: path.clone(), source })?;
        Ok(Self { path, handle: Arc::new(Mutex::new(file)) })
    }

    pub async fn emit(&self, record: &AccessLogRecord) -> Result<(), AccessLogError> {
        let line = format(record);  // produces a String per the default format
        let mut file = self.handle.lock().await;
        file.write_all(line.as_bytes()).await.map_err(|source| AccessLogError::Write {
            path: self.path.clone(),
            source,
        })?;
        file.write_all(b"\n").await.map_err(|source| AccessLogError::Write {
            path: self.path.clone(),
            source,
        })?;
        // No flush — the HCM's fire-and-forget posture treats emission as
        // best-effort. The OS will flush on file close. The unit tests
        // explicitly drop the FileSink to force file close before reading.
        Ok(())
    }
}
```

The signature is `pub async fn emit(&self, record: &AccessLogRecord) -> Result<(), AccessLogError>` per the parent SPEC §3 D8.2 / 06.2 SPEC §3 rule 3 contract (option (c) — concrete inherent impl, NOT a trait method). The `tokio::sync::Mutex<File>` serializes writes across concurrent emissions on the same `Arc<FileSink>` (multiple HCM in-flight requests on the same listener share one `FileSink`; the mutex ensures their access-log lines do not interleave). Per signpost 3 in §6 below, the `Mutex<File>` posture is preferred over `File-per-emit` because the latter loses append-semantic atomicity guarantees on some filesystems.

**`default_format::format` emitter** (in `default_format.rs`):

```rust
use crate::record::AccessLogRecord;
use std::fmt::Write as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Format an AccessLogRecord per the Envoy default access-log format.
/// Output is a single line WITHOUT trailing newline (the FileSink writes
/// the newline separately so callers that build multi-record buffers can
/// control the newline placement).
pub fn format(record: &AccessLogRecord) -> String {
    let mut s = String::with_capacity(256);
    s.push('[');
    format_iso8601(&mut s, record.start_time);
    s.push_str("] \"");
    s.push_str(&record.method);
    s.push(' ');
    s.push_str(&record.path);
    s.push(' ');
    s.push_str(&record.protocol);
    s.push_str("\" ");
    write!(&mut s, "{}", record.response_code).unwrap();
    s.push(' ');
    s.push_str(&record.response_flags);
    s.push(' ');
    write!(&mut s, "{}", record.bytes_received).unwrap();
    s.push(' ');
    write!(&mut s, "{}", record.bytes_sent).unwrap();
    s.push(' ');
    write!(&mut s, "{}", record.duration.as_millis()).unwrap();
    s.push(' ');
    match &record.upstream_service_time {
        Some(d) => { write!(&mut s, "{}", d.as_millis()).unwrap(); }
        None => s.push('-'),
    }
    s.push_str(" \"");
    push_or_dash(&mut s, &record.forwarded_for);
    s.push_str("\" \"");
    push_or_dash(&mut s, &record.user_agent);
    s.push_str("\" \"");
    push_or_dash(&mut s, &record.request_id);
    s.push_str("\" \"");
    push_or_dash(&mut s, &record.authority);
    s.push_str("\" \"");
    push_or_dash(&mut s, &record.upstream_host);
    s.push('"');
    s
}

fn push_or_dash(s: &mut String, opt: &Option<String>) {
    match opt {
        Some(v) => s.push_str(v),
        None => s.push('-'),
    }
}

/// Hand-rolled ISO-8601 emitter: YYYY-MM-DDTHH:MM:SS.sssZ (UTC, ms resolution).
/// Uses Gregorian calendar arithmetic (proleptic) on the UNIX epoch seconds.
/// Defers to `epoch_seconds_to_ymd` for the date split.
fn format_iso8601(s: &mut String, t: SystemTime) {
    let dur = t.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let secs = dur.as_secs();
    let ms = dur.subsec_millis();
    let (year, month, day, hour, minute, second) = epoch_seconds_to_ymd_hms(secs);
    write!(s, "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
           year, month, day, hour, minute, second, ms).unwrap();
}

/// Gregorian calendar arithmetic helper. Splits an epoch-seconds value
/// into (year, month, day, hour, minute, second). Year range supported:
/// [1970, 9999] (the upper bound covers all conceivable wall-clock values
/// before the format breaks; lower bound is the UNIX epoch).
fn epoch_seconds_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    // Implementation: standard days-since-epoch arithmetic with leap-year
    // handling. Uses the formula from RFC 3339 / ISO 8601 specifications.
    // Has unit tests against known epochs (epoch 0, leap day boundaries,
    // century boundaries, year-2038 boundary, etc.) — see D1.2 unit tests.
    //
    // Roughly ~30 LoC of straightforward arithmetic; no algorithmic trick
    // needed since perf is not a concern (the emitter runs once per
    // request, not in a hot loop).
    todo!("planner fills in at 06.2 Task 2; see signpost 1 + signpost 2 in §6")
}
```

The `epoch_seconds_to_ymd_hms` body is ~30 LoC of straightforward Gregorian arithmetic; the planner picks between an inline body and a separate helper module per signpost 2 in §6 below (recommendation: inline in `default_format.rs` as a `fn`, with a separate `mod gregorian` only if the function grows beyond 50 LoC).

**`AccessLogError` enum** (in `error.rs`):

```rust
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AccessLogError {
    #[error("failed to open access log file at {path}: {source}")]
    Open { path: PathBuf, source: std::io::Error },

    #[error("failed to write access log line to {path}: {source}")]
    Write { path: PathBuf, source: std::io::Error },

    #[error("invalid access log file path: {path}")]
    InvalidPath { path: PathBuf },
}
```

**Unit tests appended** (~250 LoC across the modules; ~14 tests total):

In `record.rs::tests`:
1. `record_construction_full` — build an `AccessLogRecord` with every field populated; verify the struct round-trips through `Debug`.
2. `record_clone_is_deep_for_strings` — clone a record; mutate the clone's `method` field; verify the original is unchanged (Rust's `Clone` on `String` is deep-copy, so this is trivially satisfied; the test exists for documentation).

In `file_sink.rs::tests`:
3. `file_sink_writes_one_record` — open a `FileSink` to a tempdir path; emit one record; close; read the file; verify the contents match the formatter output + a trailing newline.
4. `file_sink_appends_multiple_records` — open one FileSink; emit 3 records sequentially; close; read; verify all 3 lines present in order + each terminated by `\n`.
5. `file_sink_serializes_concurrent_emissions` — open one `Arc<FileSink>`; spawn 10 concurrent emissions on the same Arc; await all; close; verify the file contains 10 lines (one per emission) with no interleaving (each line is one complete formatter output + `\n`).
6. `file_sink_emit_returns_error_on_invalid_path` — attempt to open a `FileSink` to a path whose parent directory does not exist; verify `AccessLogError::Open` is returned with the path.

In `default_format.rs::tests`:
7. `format_happy_path_direct_response` — build a record matching fixture 0012's direct_response surface (`method: "GET"`, `path: "/"`, `protocol: "HTTP/1.1"`, `response_code: 200`, `response_flags: "-"`, `bytes_received: 0`, `bytes_sent: 3`, `duration: Duration::from_millis(5)`, all `Option<String>` fields `None` except `authority: Some("envoy-rust.test".into())`, `upstream_host: None`); call `format(&record)`; assert the output matches the expected default-format byte sequence (modulo the ISO-8601 timestamp which is golden-tested separately).
8. `format_with_router_proxy_path` — build a record with `upstream_service_time: Some(Duration::from_millis(2))` + `upstream_host: Some("127.0.0.1:8080".into())`; verify those fields render correctly.
9. `format_5xx_response_with_flags` — build a record with `response_code: 503` + `response_flags: "UH"` (forward-compatibility test; 06.2's HCM does NOT emit non-`-` flags but the formatter must handle them); verify the output is `... 503 UH ...`.
10. `format_iso8601_epoch_zero` — `format_iso8601(&mut s, UNIX_EPOCH)`; verify `s == "1970-01-01T00:00:00.000Z"`.
11. `format_iso8601_known_date` — `format_iso8601` for a known date (e.g., `2024-02-29T12:34:56.789Z` — leap day boundary); verify the rendered string.
12. `epoch_seconds_to_ymd_hms_known_dates` — table-driven test with ~5 known epochs (epoch 0, March 1 2000 — Y2K leap, January 1 2038 — Y2K38 boundary, March 1 2024 — leap day, Dec 31 2099 — century boundary); verify the (y, mo, d, h, mi, s) tuple for each.
13. `epoch_seconds_to_ymd_hms_handles_far_future` — epoch corresponding to year 9999; verify it does not panic; verify the year is rendered correctly.
14. `format_utf8_edge_case_in_user_agent` — record with `user_agent: Some("Mozilla/5.0 (X11; Linux 中文)".into())`; verify the output contains the UTF-8 bytes verbatim (Envoy's default format does not escape UTF-8; envoy-rust matches).

**LoC estimate D1.2:** ~30 LoC `Cargo.toml` + workspace member registration + ~30 LoC `lib.rs` (re-exports + module-level doc) + ~50 LoC `record.rs` (struct definition + `Clone`/`Debug` derives + 2 unit tests) + ~80 LoC `file_sink.rs` (struct + `new` + `emit` + 4 unit tests) + ~150 LoC `default_format.rs` (`format` + `format_iso8601` + `epoch_seconds_to_ymd_hms` + `push_or_dash` + 8 unit tests) + ~30 LoC `error.rs` (`AccessLogError` enum) + ~5 LoC `sink.rs` placeholder. Total D1.2: **~375 LoC impl + ~250 LoC tests = ~625 LoC** (parent SPEC §3 D8.2 projected ~400 + ~250 = ~650; this matches within rounding).

### D2.2 — `envoy-config` schema additions for `access_log:` on HCM

Two coordinated edits in `crates/envoy-config/src/bootstrap.rs`:

**D2.2.a — `AccessLogConfig` struct + `HttpConnectionManagerConfig.access_log` field.** New struct in `bootstrap.rs`. Optional field (`Vec<AccessLogConfig>` defaulted to empty) on `HttpConnectionManagerConfig`. Subset of Envoy's `envoy.config.accesslog.v3.AccessLog` proto:

```rust
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AccessLogConfig {
    pub name: String,
    pub typed_config: TypedConfig,  // existing typed_config envelope from earlier phases
}
```

`HttpConnectionManagerConfig` gains:

```rust
#[serde(default)]
pub access_log: Vec<AccessLogConfig>,
```

(Default-empty Vec; absent block parses cleanly. The validator runs the per-entry gate only if non-empty.)

The `TypedConfig` envelope already exists from earlier phases (used for `HttpConnectionManager` itself, `TcpProxy`, `Router`, etc. per ADR-0014's pattern). 06.2 adds a new variant on the `TypedConfig` enum (or grows the existing matcher; the planner cross-checks at 06.2 Task 1 to determine the cleanest extension shape):

```rust
// Pseudocode; the exact TypedConfig enum shape lands at planner cross-check time.
TypedConfig::FileAccessLog(FileAccessLogTypedConfig),
```

Where:

```rust
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileAccessLogTypedConfig {
    #[serde(rename = "@type")]
    pub type_url: String,  // must be "type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog"
    pub path: String,      // on-disk file path
    // NOTE: Envoy's FileAccessLog proto carries `format` (string format),
    // `log_format` (oneof: text / json / etc.), `json_format`, `typed_json_format`.
    // 06.2 ignores all of them — format-string customization is OUT of scope.
    // The planner adds a `parse-and-ignore` posture per ADR-0026 if these
    // fields appear in fixture 0012's envoy.yaml; otherwise serde
    // `deny_unknown_fields` rejects, and the fixture omits them.
}
```

**D2.2.b — Validator extension at the existing `validate_hcm` site.** For each `AccessLogConfig` entry in the parsed HCM:
1. If `name != "envoy.access_loggers.file"`, reject with `ConfigError::UnsupportedAccessLogType { actual: name.clone() }`.
2. If the `typed_config`'s `@type` URL is NOT exactly `type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog`, reject with `ConfigError::UnsupportedAccessLogType { actual: type_url.clone() }`.
3. If `typed_config.path` is empty, reject with `ConfigError::InvalidAccessLogPath` (or reuse `AccessLogError::InvalidPath` if the planner picks the `envoy-config → envoy-accesslog` typed-error coupling path; recommendation is a new `ConfigError::InvalidAccessLogPath` variant in `envoy-config` to avoid the cross-crate typed-error coupling at the validator boundary).

**ConfigError extension in `crates/envoy-config/src/lib.rs`:** add new variants:

```rust
#[error("unsupported access log type: {actual}; only 'envoy.access_loggers.file' with @type ending in .FileAccessLog is supported in this version")]
UnsupportedAccessLogType { actual: String },

#[error("access log path must be non-empty")]
InvalidAccessLogPath,
```

Re-exports in `crates/envoy-config/src/lib.rs`'s `pub use bootstrap::{...}` block extend with `AccessLogConfig` and the `FileAccessLogTypedConfig` (or whichever typed-config variant accessor name lands).

**Validator unit tests appended** to `crates/envoy-config/src/bootstrap.rs::tests` (~6 new tests projected):

1. `parses_hcm_with_file_access_log` — full bootstrap with HCM `access_log: [{ name: "envoy.access_loggers.file", typed_config: { @type: ".../v3.FileAccessLog", path: "/tmp/access.log" } }]`; validator accepts; the parsed `HCMConfig.access_log[0].typed_config` round-trips with the expected path.
2. `parses_hcm_with_no_access_log_block` — HCM with no `access_log:` field; validator accepts; the parsed `HCMConfig.access_log` is the empty Vec.
3. `parses_hcm_with_empty_access_log_array` — HCM with `access_log: []`; validator accepts; the parsed `HCMConfig.access_log` is the empty Vec.
4. `rejects_hcm_with_unsupported_access_log_name` — HCM with `access_log: [{ name: "envoy.access_loggers.stdout", ... }]`; validator returns `UnsupportedAccessLogType { actual: "envoy.access_loggers.stdout" }`.
5. `rejects_hcm_with_unsupported_access_log_type_url` — HCM with `access_log: [{ name: "envoy.access_loggers.file", typed_config: { @type: ".../v3.UnknownAccessLog", path: "/tmp/access.log" } }]`; validator returns `UnsupportedAccessLogType { actual: ".../v3.UnknownAccessLog" }`.
6. `rejects_hcm_with_empty_access_log_path` — HCM with `access_log: [{ name: "envoy.access_loggers.file", typed_config: { @type: ".../v3.FileAccessLog", path: "" } }]`; validator returns `InvalidAccessLogPath`.

Plus 1 corpus-walk acceptance test mirroring 04.2's pattern: `fuzz_corpus_hcm_access_log_file_seed_parses` reads the new `hcm_access_log_file.yaml` seed via `include_str!` and confirms it parses cleanly.

**Fuzz corpus extension.** `crates/envoy-config/fuzz/corpus/parse_bootstrap/` gains 1 new seed:

- `hcm_access_log_file.yaml` — full bootstrap with one HCM listener whose `access_log:` carries one entry of `name: envoy.access_loggers.file` + `typed_config: { @type: type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog, path: /tmp/fuzz-access.log }` + single VH `domains: ["*"]` + single route `prefix: "/"` `direct_response { status: 200, body: { inline_string: "fuzz\n" } }` + `clusters: []`. The seed exercises the validator's accept-path on `AccessLogConfig`; the fuzzer never runs the FileSink (parse_bootstrap only exercises serde + the validator, not the runtime).

Allow-list entry `!corpus/parse_bootstrap/hcm_access_log_file.yaml` added to `crates/envoy-config/fuzz/.gitignore` (per the established phase-precedent for fuzz seed inclusion).

**LoC estimate D2.2:** ~80 LoC schema delta (`AccessLogConfig` struct + the `HttpConnectionManagerConfig.access_log` field + the `FileAccessLogTypedConfig` variant on `TypedConfig` + the 2 new ConfigError variants) + ~50 LoC validator path + ~80 LoC unit tests (6 new + 1 corpus-walk × ~12 LoC each) + ~25 LoC fuzz seed YAML. Total D2.2: **~235 LoC** (parent SPEC §3 D9.2 projected ~80 + ~50 + ~6 tests + ≥1 fuzz seed = ~150-200 LoC; this matches within drift expectations).

### D3.2 — HCM access-log wiring at the on-response-complete site

The core 06.2 runtime deliverable. `envoy_http1::HCMConfig` (the per-listener immutable config struct) gains an optional `access_log: Vec<Arc<envoy_accesslog::FileSink>>` field. The H2 path inherits via the `HCMConfig` type-alias from 05.2 D1 (`envoy_http2::HCMConfig = envoy_http1::HCMConfig`); both H1 and H2 listener-side HCMs see the new field through the same struct.

**`HCMConfig` extension in `crates/envoy-http1/src/hcm.rs`:**

```rust
// 06.2 NEW — added to the existing HCMConfig struct:
pub struct HCMConfig {
    // ... existing 04.x / 05.x fields unchanged ...
    pub access_log: Vec<std::sync::Arc<envoy_accesslog::FileSink>>,
}
```

The `Arc<FileSink>` is constructed at HCM-config-build time (`HCMConfig::from_parsed` or whichever 04.1-landed constructor handles config → runtime translation; the planner cross-checks at 06.2 Task 1) by reading `parsed_hcm.access_log` and calling `FileSink::new(path).await` for each entry. Note: `FileSink::new` is async (it opens the file); the HCM-config-build is therefore async too (or the FileSink construction is deferred to the first emission — recommendation is at config-build time so file-open errors surface at startup, not at first request).

**HCM-on-response-complete dispatch site.** The existing `serve_connection` function at `crates/envoy-http1/src/hcm.rs` (the per-connection state machine landed in 04.1, extended by 04.3 for the `BuildOutcome::Proxy` path, extended by 05.2 for the H2 path's symmetric site at `crates/envoy-http2/src/hcm.rs`) emits the access-log record AFTER the response has been written to the downstream codec. The exact dispatch site:

```rust
// At crates/envoy-http1/src/hcm.rs's serve_connection function, AFTER
// write_response returns successfully (or after the 502 fallback writes
// successfully) and BEFORE the next iteration of the keep-alive loop:

// Capture the timing window (start_time was captured at request-arrival).
let duration = request_arrival_instant.elapsed();
let now = SystemTime::now();  // wall-clock for %START_TIME% — actually,
                              // the planner captures this at request-arrival
                              // time via SystemTime::now() and stores both
                              // SystemTime + Instant; the SystemTime is for
                              // %START_TIME% rendering, the Instant is for
                              // %DURATION% measurement. See signpost 5 in §6.

if !config.access_log.is_empty() {
    let record = AccessLogRecord {
        start_time: request_arrival_systime,
        method: request.method.as_str().to_owned(),
        path: x_envoy_original_path_or_path(&request).to_owned(),
        protocol: "HTTP/1.1".to_owned(),
        response_code: response.status,
        response_flags: "-".to_owned(),  // 06.2 always emits "-"; non-"-" flags defer
        bytes_received: request.body.len() as u64,
        bytes_sent: response.body.len() as u64,
        duration,
        upstream_service_time: extract_upstream_service_time(&response),
        forwarded_for: get_header(&request, "x-forwarded-for"),
        user_agent: get_header(&request, "user-agent"),
        request_id: get_header(&request, "x-request-id"),
        authority: get_header(&request, "host"),
        upstream_host: upstream_endpoint_for_record,  // captured at router-arm dispatch time
    };
    for sink in &config.access_log {
        // Recommended posture (option (b) per Rule 4 above): synchronous-after-write.
        if let Err(err) = sink.emit(&record).await {
            tracing::warn!(error = ?err, "access log emission failed");
        }
        // Alternative posture (option (a)): tokio::spawn fire-and-forget:
        // let sink = sink.clone();
        // let record = record.clone();
        // tokio::spawn(async move {
        //     if let Err(err) = sink.emit(&record).await {
        //         tracing::warn!(error = ?err, "access log emission failed");
        //     }
        // });
    }
}
```

The `extract_upstream_service_time` helper reads the response headers for `x-envoy-upstream-service-time` (case-insensitive) and parses the value as a u64 milliseconds. The `x_envoy_original_path_or_path` helper reads `x-envoy-original-path` from the request headers; if present, returns that value; otherwise returns `request.path.as_str()`. Both helpers are private to `crates/envoy-http1/src/hcm.rs` (they don't escape the HCM module boundary; ~10 LoC each).

**The `upstream_endpoint_for_record` capture.** For direct_response paths, this is `None` (no upstream involved). For router-proxy paths (the `BuildOutcome::Proxy` arm landed in 04.3), the router-arm captures the resolved upstream `SocketAddr` (already captured at router-arm time for the upstream `Client::connect` call) and threads it through to the HCM record-build site. The threading shape is a return-value extension on the `BuildOutcome::Proxy` handler (or a per-call `Option<SocketAddr>` captured into a per-connection state slot; the planner picks at 06.2 Task 1 — recommendation is the return-value extension since it's lifetime-bound to the request and avoids per-connection state). Fixture 0012's direct_response path produces `None` for this field; the rendered `%UPSTREAM_HOST%` token is `-`. Future fixtures with router-proxy paths will populate this field with the formatted endpoint string.

**H2 inheritance via the HCMConfig type-alias.** The 05.2-landed `crates/envoy-http2/src/hcm.rs` consumes `envoy_http1::HCMConfig` directly via the type-alias (per 05.2 SPEC §3 architectural rule 2). The new `access_log` field on `HCMConfig` is therefore visible to the H2 HCM transparently; the H2 HCM's symmetric on-response-complete site (the per-stream `tokio::task` body in `serve_connection`'s stream loop, AFTER `send_data(.., end_of_stream=true)` writes the response) gains the same access-log dispatch block as the H1 path. The dispatch is identical; only the protocol field of the record differs (`"HTTP/2"` vs `"HTTP/1.1"`).

**Tests appended** to `crates/envoy-http1/src/hcm.rs::tests` and `crates/envoy-http2/src/hcm.rs::tests` (~6 tests projected):

In `envoy-http1/src/hcm.rs::tests` (~4 tests):
1. `hcm_with_no_access_log_does_not_touch_filesystem` — construct an HCM with `access_log: vec![]`; serve a request; verify no access-log file is created (verifiable by checking a tempdir before/after).
2. `hcm_with_file_access_log_writes_one_line_per_request` — construct an HCM with `access_log: vec![Arc::new(FileSink::new(tempdir/log).await?)]`; serve a request; verify the access-log file contains exactly one line matching the default format.
3. `hcm_with_file_access_log_emission_failure_does_not_fail_request` — construct an HCM with a `FileSink` whose underlying file has been replaced by a directory mid-test (or some other I/O-failure inducing posture); serve a request; verify the response is written successfully despite the emission failure; verify a `tracing::warn!` log line was emitted (using `tracing-test` or similar pattern; planner picks at 06.2 Task 1).
4. `hcm_records_protocol_as_http1_1_on_h1_path` — construct an HCM on the H1 path; verify the access-log line's `%PROTOCOL%` token is `HTTP/1.1`.

In `envoy-http2/src/hcm.rs::tests` (~2 tests):
5. `hcm_h2_with_file_access_log_writes_one_line_per_request` — construct an H2 HCM with `access_log: vec![Arc::new(FileSink::new(tempdir/log).await?)]`; drive an H2C request; verify the access-log file contains exactly one line.
6. `hcm_h2_records_protocol_as_http2_on_h2_path` — verify the access-log line's `%PROTOCOL%` token is `HTTP/2` on the H2 path.

**LoC estimate D3.2:** ~30 LoC `HCMConfig` field + helpers + ~70 LoC dispatch site (H1) + ~50 LoC dispatch site (H2; symmetric to H1 but per-stream) + ~50 LoC the timing-capture rewiring (Instant + SystemTime captures at request-arrival; threading the upstream SocketAddr through `BuildOutcome::Proxy`) + ~100 LoC unit tests (4 H1 + 2 H2). Total D3.2: **~300 LoC** (parent SPEC §3 D10.2 projected ~200 + ~100 = ~300; this matches).

### D4.2 — Differential harness extensions for HTTP/1.1 + access-log assertion + fixture 0012

Three coordinated edits to `tests/differential/`:

**D4.2.a — `Driver::Http1WithAccessLog` variant.** New variant on the existing `Driver` enum at `tests/differential/src/lib.rs`. Shape extends `Http1` with an `expected_access_log_lines` field:

```rust
// tests/differential/src/lib.rs Driver enum extension:
Http1WithAccessLog {
    method: String,
    path: String,
    host: String,
    expected_status: u16,
    expected_body: BodyRule,
    expected_headers: HeaderRule,
    expected_access_log_lines: Vec<AccessLogLineRule>,
}
```

The `Driver::Http1WithAccessLog` reuses the existing `drive_http1` async helper from 04.1 unchanged for the wire-protocol leg. The access-log assertion is a post-request step: after `drive_http1` returns and `assert_equivalence` passes for the response, the harness reads the access-log file from each proxy's config (the path is computed by the harness from the rendered envoy.yaml / envoy-rust.yaml — see signpost 8 in §6 below for the file-discovery mechanism), tokenizes each line, and matches token-by-token against the per-token rules.

**D4.2.b — `AccessLogLineRule` per-token rule.** New file at `tests/differential/src/access_log.rs` (~120 LoC). The per-token rule enum:

```rust
// tests/differential/src/access_log.rs
pub enum AccessLogLineRule {
    /// Token must match exactly (used for value-exact tokens per
    /// BEHAVIOR_CONTRACT.md `Access log field mapping`).
    Exact(String),

    /// Token must parse as ISO-8601 timestamp (YYYY-MM-DDTHH:MM:SS.sssZ).
    /// Used for %START_TIME% which is name-required, value-may-differ.
    Iso8601Format,

    /// Token must parse as a non-negative integer (decimal milliseconds).
    /// Used for %DURATION% and %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)% in
    /// the present-on-both-sides router-proxy case.
    DurationMs,

    /// Token may be anything (used for fields not covered by 06.2; reserved
    /// for forward-compat).
    Wildcard,

    /// Token expected to differ by emitter side; the harness captures both
    /// values and asserts both are present and have matching shape, but
    /// allows divergence.
    EnvoyOnly,  // expected only on Envoy's line; envoy-rust line carries "-"
                // (currently unused in 06.2 fixture 0012 since both proxies
                // emit symmetric records; reserved for future fixtures
                // exercising Envoy-side-only fields).
}
```

The harness also ships an `AccessLogTokenizer` (~80 LoC) that splits a default-format line into its 14 (sometimes 15, accounting for the `[%START_TIME%]` bracket-wrapping) component tokens. The tokenizer handles the brackets around `%START_TIME%`, the quotes around the request-line and the quoted REQ tokens, and the bare unquoted tokens. It does NOT handle format-string customization (formats other than the Envoy default reject with a parser error per signpost 8 in §6).

The `assert_access_log_lines_equivalent` harness function at `tests/differential/src/access_log.rs`:

```rust
pub fn assert_access_log_lines_equivalent(
    envoy_lines: &[String],
    envoy_rust_lines: &[String],
    rules: &[Vec<AccessLogLineRule>],  // one Vec<rule> per line
) -> Result<(), String> {
    if envoy_lines.len() != envoy_rust_lines.len() {
        return Err(format!(...));
    }
    if envoy_lines.len() != rules.len() {
        return Err(format!(...));
    }
    for ((envoy_line, envoy_rust_line), line_rules) in envoy_lines.iter()
        .zip(envoy_rust_lines.iter())
        .zip(rules.iter())
    {
        let envoy_tokens = tokenize(envoy_line)?;
        let envoy_rust_tokens = tokenize(envoy_rust_line)?;
        for (i, (envoy_tok, envoy_rust_tok, rule)) in envoy_tokens.iter()
            .zip(envoy_rust_tokens.iter())
            .zip(line_rules.iter())
            .enumerate()
        {
            apply_rule(rule, envoy_tok, envoy_rust_tok)
                .map_err(|e| format!("token {i}: {e}"))?;
        }
    }
    Ok(())
}
```

**D4.2.c — `run_fixture` dispatch arm on `Driver::Http1WithAccessLog`.** The existing `run_fixture` cascade in `tests/differential/src/lib.rs` grows a new arm dispatching `Driver::Http1WithAccessLog`. The new arm:
1. Drives the wire-protocol leg via `drive_http1` (reused unchanged from 04.1).
2. Asserts the response equivalence per `assert_equivalence` (reused unchanged from 04.1).
3. Reads the access-log file from each proxy's config:
   - For the upstream-Envoy side: the file path lives at `/tmp/<fixture-id>-envoy-access.log` (or whichever path the fixture's `envoy.yaml` declares; the harness extracts it by parsing the YAML — see signpost 8 in §6).
   - For the envoy-rust side: same shape; path lives at `/tmp/<fixture-id>-envoy-rust-access.log` per the fixture's `envoy-rust.yaml`.
   - Both files must exist after the request completes (the harness waits up to 5s for both files to appear; if either doesn't appear, the test fails with a descriptive error).
4. Reads the lines from each file (typically 1 line per line-rules entry; fixture 0012 sends 1 request and asserts on 1 line).
5. Calls `assert_access_log_lines_equivalent(envoy_lines, envoy_rust_lines, &expected_rules)`.

**Fixture `tests/fixtures/0012-access-log-file-sink/`** — 5 files mirroring 04.1's fixture-0007 + 06.1's fixture-0011 shape:

**`envoy.yaml`:**

```yaml
node: { id: envoy-rust-phase-06.2-fixture-0012, cluster: envoy-rust-phase-06.2 }
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
                stat_prefix: ingress_http
                codec_type: HTTP1
                generate_request_id: false
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0012-envoy-access.log
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
- no `admin` block.
- access-log `path: /tmp/0012-envoy-rust-access.log` instead.
- `generate_request_id: false` is omitted (envoy-rust does not inject `x-request-id`; field-set divergence intentional, mirrors 04.3 fixture 0008's pattern).

**`inputs/payload.bin`:** empty (0 bytes) — the GET has no request body.

**`expectations.yaml`:**

```yaml
driver:
  kind: http1_with_access_log
  method: GET
  path: "/"
  host: envoy-rust.test
  expected_status: 200
  expected_body:
    byte_exact: "ok\n"
  expected_headers:
    rule: set_equal_modulo_allow_list
  expected_access_log_lines:
    - tokens:
        - { rule: iso8601_format }                       # %START_TIME%
        - { rule: exact, value: "GET" }                  # %REQ(:METHOD)%
        - { rule: exact, value: "/" }                    # %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%
        - { rule: exact, value: "HTTP/1.1" }             # %PROTOCOL%
        - { rule: exact, value: "200" }                  # %RESPONSE_CODE%
        - { rule: exact, value: "-" }                    # %RESPONSE_FLAGS%
        - { rule: exact, value: "0" }                    # %BYTES_RECEIVED%
        - { rule: exact, value: "3" }                    # %BYTES_SENT%
        - { rule: duration_ms }                          # %DURATION%
        - { rule: exact, value: "-" }                    # %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%
        - { rule: exact, value: "-" }                    # %REQ(X-FORWARDED-FOR)%
        - { rule: exact, value: "-" }                    # %REQ(USER-AGENT)% (or wildcard if h1 client adds default)
        - { rule: exact, value: "-" }                    # %REQ(X-REQUEST-ID)%
        - { rule: exact, value: "envoy-rust.test" }      # %REQ(:AUTHORITY)%
        - { rule: exact, value: "-" }                    # %UPSTREAM_HOST%
```

The `%REQ(USER-AGENT)%` rule in the expectations may need to be `wildcard` if `drive_http1` adds a default user-agent (the planner cross-checks at fixture-write time; recommendation is to either set `User-Agent: 0012-fixture` explicitly in the harness request or to assert wildcard for that token; the table is a draft).

**`README.md`:** ~30 lines describing the fixture surface, the access-log file paths the fixture writes to, the per-token rules, and the cross-reference to phase 06.2 SPEC §3 D4.2.

**Docker-gated test:** `tests/differential/tests/access_log_file_sink.rs` — 7-line wrapper:

```rust
#[tokio::test]
async fn access_log_file_sink() {
    differential::run_fixture("0012-access-log-file-sink").await.expect("fixture green");
}
```

**Tests appended** to `tests/differential/src/lib.rs::tests`:

1. `tokenize_default_format_happy_path` — input a known default-format line; verify the tokenizer produces the 14 expected tokens.
2. `tokenize_handles_dash_in_quoted_position` — input a line where a quoted REQ token has value `"-"`; verify the tokenizer produces `-` for that position.
3. `assert_access_log_lines_equivalent_happy_path` — input matching envoy + envoy-rust lines + matching rules; verify Ok.
4. `assert_access_log_lines_equivalent_rejects_token_mismatch` — input differing tokens with `Exact` rule; verify Err.

**LoC estimate D4.2:** ~120 LoC harness extensions (`Driver::Http1WithAccessLog` variant + `AccessLogLineRule` enum + `AccessLogTokenizer` + `assert_access_log_lines_equivalent` + `run_fixture` dispatch arm) + ~80 LoC fixture YAMLs (envoy.yaml + envoy-rust.yaml) + ~30 LoC README + ~30 LoC expectations.yaml + 7 LoC Docker-gated test wrapper + ~100 LoC unit tests (4 new). Total D4.2: **~370 LoC** (parent SPEC §3 D11.2 projected ~400 + 5 fixture files; this matches).

### D5.2 — `BEHAVIOR_CONTRACT.md` `Access log field mapping` section first-time population

Per parent SPEC §2.2 + §3 D12.2. Lands at the **06.2 first-task or first-fixture commit** — recommendation is the first-fixture commit (Task corresponding to D4.2) so the BEHAVIOR_CONTRACT.md edit lands in lockstep with the harness rule shape that asserts on the table.

**Edit shape in `docs/envoy-rust/BEHAVIOR_CONTRACT.md`:** the existing `Access log field mapping` section's standing comment block (`> Populated in phase 06 when access logs first ship. Extended whenever a new filter adds new log-only fields.\n\n_(empty; populated starting phase 06)_`) is replaced by:

1. A short prefatory paragraph describing the Envoy default format (verbatim copy of the 14-token reference + sample line shape from §2 above).
2. The 14-row table from §2.1 above, transcribed verbatim (token | envoy-rust internal source | equivalence disposition | rationale).
3. A short closing paragraph noting "Format-string customization defers to a later phase. Format-string parsing (`%REQ(:METHOD)%`, `%START_TIME(format-string)%`, etc.) is OUT of scope in 06.2; the validator at `envoy-config` ignores user-supplied `format` and `log_format` fields on `FileAccessLog` typed_configs, and the emitter always produces the fixed default format."

**LoC estimate D5.2:** ~50 LoC of doc-only diff (the 14-row table + 2 short paragraphs). Total D5.2: **~50 LoC** (parent SPEC §3 D12.2 projected ~50; matches exactly).

### D6.2 — State-4 phase-done verification (no code)

State-4 phase-done verification per `BOOTSTRAP_PROMPT.md` §7.5, scoped to 06.2's surfaces:
- **(a)** fixture 0012 green at Docker-gated CI level; CI run URL + test result quoted inline in PROGRESS.md.
- **(b)** fixtures 0001-0011 green at Docker-gated CI level (unchanged in 06.2; verified by re-running the full Docker-gated suite).
- **(c)** h2spec ≥95% pass with `known-failures.txt` unchanged; re-run at state-4 to confirm no regression from access-log wiring.
- **(d)** `parse_bootstrap` fuzz target clean against the corpus extended with `hcm_access_log_file.yaml`; CI run URL + result quoted inline.
- **(e)** `cargo build/clippy/fmt/test/deny` clean on stable toolchain; CI run URL + result quoted inline.
- **(f)** REVIEW.md verdict approved.

PROGRESS.md at the state-4 verification commit captures all six gate elements with inline evidence.

**LoC estimate D6.2:** 0 LoC code; ~50 LoC doc-only PROGRESS.md / REVIEW.md edits at state-4 / state-5 commits respectively. Out of LoC budget (verification deliverable per parent SPEC §3 D13.2).

### LoC budget summary for §3

| Deliverable | Impl LoC | Test LoC | Total LoC |
|---|---|---|---|
| D1.2 (envoy-accesslog crate) | 375 | 250 | 625 |
| D2.2 (envoy-config schema) | 130 | 105 | 235 |
| D3.2 (HCM dispatch) | 200 | 100 | 300 |
| D4.2 (harness + fixture 0012) | 270 | 100 | 370 |
| D5.2 (BEHAVIOR_CONTRACT edit) | 50 | 0 | 50 |
| D6.2 (verification, no code) | 0 | 0 | 0 |
| **Total D1.2-D5.2** | **1025** | **555** | **~1580 LoC** |

The total **~1580 LoC** modestly exceeds the parent SPEC's ~1300 LoC projection (parent SPEC §3 sub-phase header: *"~1300 LoC, ~11 tasks"*); the drift is concentrated in D1.2 where the per-test breakdown (14 unit tests × ~15-20 LoC each) plus the 5 modules' boilerplate exceeds the parent's whole-crate ~650 estimate. Per parent SPEC §3 D8.2's drift-tolerance posture, the planner accepts the drift and PLANs against the SPEC-write-time estimate; the §6.1 split-gate's "~1500 LoC" guardrail is exceeded by ~5%, well under the 20% drift tolerance phase-04.3 / 05.2 absorbed without re-splitting (per parent-05 SPEC §5 rule "do not nest-split a sub-phase that was itself produced by a split"). The PLAN-writer at 06.2 state-2 records the chosen posture in PROGRESS Task 1.

---

## 4. Non-goals (deferred non-goals)

The following are out of scope for 06.2 and defer to other sub-phases or later phases. The list is a subset of parent-06 SPEC §4, scoped to items predictably tempting to fold into 06.2 by a planner reading only this SPEC.

**Deferred to sub-phase 06.3:**

- **Comprehensive stats wiring** at HCM/router/listener/cluster sites (per-response-class HCM counters, connection-lifetime gauges, upstream-side HCM counters, listener accept-failure counter). Parent SPEC §3 D15.3.
- **`http.<stat_prefix>.access_logs_total` counter.** This is the natural extension of 06.2's HCM access-log dispatch site (the counter increments at queue-enter time per parent §6 Rule 4); 06.3 lands it, NOT 06.2. The 06.2 HCM dispatch site adds NO counter increments.
- **Fixture 0011's `expectations.yaml` extension** — 06.1 lands fixture 0011 with the representative-stats-subset; 06.3 extends it to assert the comprehensive stats. 06.2 does NOT touch fixture 0011.
- **05.3 REVIEW I1 closure (Http2ClusterFromHttp1Listener parse-time validator gate).** Parent SPEC §3 D14.3 lands this as a Task-1 preamble in 06.3. 06.2 does NOT engage this surface.
- **Parent ROADMAP row `06` flip to `done`.** Happens at sub-phase 06.3's state-6 phase-done commit, not 06.2's.

**Deferred to later phases (per parent-06 SPEC §4 — items relevant to the access-log surface):**

- **Format-string customization.** The Envoy default format is hand-rolled in 06.2 with no parsing of user-supplied format strings. Format-string parsing (`%REQ(:METHOD)%`, `%START_TIME(%Y-%m-%dT%T.%3fZ)%`, `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` substitution-fallback semantics, `%FILTER_STATE(...)%`, `%DYNAMIC_METADATA(...)%`, `%REQ_WITHOUT_QUERY(...)%` etc.) defers to a later observability-family phase. The `validate_hcm` site at 06.2 D2.2.b ignores any `format` / `log_format` / `json_format` / `typed_json_format` fields on the FileAccessLog typed_config (parse-and-ignore per ADR-0026 if the planner finds those fields in real Envoy fixtures during 06.2 task time; otherwise serde `deny_unknown_fields` rejects them and the fixture omits them).
- **Sinks beyond `FileSink`.** `FileSink` only in 06.2. gRPC ALS sinks (`envoy.access_loggers.http_grpc`, `envoy.access_loggers.tcp_grpc`, `envoy.access_loggers.open_telemetry`), stdout sinks (`envoy.access_loggers.stdout`), stderr sinks defer to the Observability family. The `Sink` trait + multi-sink dispatch lands when N≥2 sinks exist.
- **JSON-format access logs.** `json_format` + `typed_json_format` are entirely out of scope. The validator rejects fixtures that set them (or parse-and-ignore per ADR-0026 if the rejection breaks fixture-equivalence with upstream Envoy that accepts but is configured for text-format).
- **Access-log filtering** (per-request access log filters: status-code ranges, header matchers, runtime fractions, response-flags filters, grpc-status-code filters, duration filters, and-of/or-of filter combinators, etc.). Envoy supports `filter` blocks on each AccessLog entry. 06.2 ships unfiltered access logs — every request emits a record. The `filter` field on `AccessLogConfig` defers (parse-and-ignore per ADR-0026 pattern if needed at fixture writeup time).
- **`%FILTER_STATE%` and `%DYNAMIC_METADATA%` access-log tokens.** Filter-state and dynamic-metadata machinery doesn't exist yet (defers to phase 07's filter-chain framework and beyond). Phase 06.2 ships the 14 fixed default-format tokens only.
- **Admin-side access logs.** The admin handler (06.1-landed) does not emit access logs even though `Admin.access_log_path` is parsed-and-ignored from 06.1 (per parent SPEC §3 D5.1). 06.2 does NOT extend the admin handler; admin-side access logs defer.
- **Per-route `disabled` / per-route access-log overrides.** Envoy supports per-route disabling of access logs and per-route overrides. 06.2 ships listener-level access-log only.
- **`%TRAILER(...)%` tokens.** HTTP/2 trailers don't exist in envoy-rust yet (deferred per 05.x SPEC §4). 06.2 has no trailer-derived tokens.
- **Buffered access-log emission.** 06.2 emits each line synchronously (or via fire-and-forget spawn per Rule 4 option (a)). Buffered emission with periodic flush defers (likely an Observability-family extension).
- **Async file rotation / log-rotation interaction.** 06.2's `FileSink` opens with `append(true)` and never re-opens; SIGHUP-style log rotation handling defers. External log rotators that move/truncate the file under envoy-rust will result in writes to the unlinked inode (UNIX semantics); the operator workaround (until an explicit rotation hook lands) is to restart envoy-rust after rotation.
- **TLS+Access-log interaction.** None anticipated. The access-log subsystem is codec-agnostic and TLS-agnostic. If a TLS field surfaces (e.g., `%DOWNSTREAM_TLS_VERSION%`), it lands when fixture demand surfaces.
- **gRPC-status-derived access-log fields.** `%GRPC_STATUS%`, `%RESPONSE_CODE_DETAILS%` etc. are not 06.2 surface (the 14 default-format tokens don't include them).
- **Phase 05.3 REVIEW I1 closure** (Http2ClusterFromHttp1Listener parse-time validator gate). Defers to 06.3 D14.3 as Task-1 preamble per parent SPEC §3.
- **Phase 05.3 REVIEW I2** (typed-error chain dissolution at H2 dispatch site). Not engaged by 06.2 surfaces; carries forward unchanged.
- **Phase 05.2 REVIEW I1** (h2spec tarball SHA-256 verification in CI). 06.2 may opportunistically close this if a state-4 task touches `.github/workflows/ci.yml`; otherwise carries forward unchanged. (06.2 does NOT anticipate touching the workflow file — the new `envoy-accesslog` crate's CI surface is covered by the existing `cargo test --workspace` step.)
- **Phase 05.2 REVIEW I2 / I3** (Http2Error variant rename, MalformedH2HeaderBlock split). Not engaged; carries forward unchanged.
- **Phase 04.1 REVIEW M5/M9** (Cargo.lock cadence ratification ADR). 06.2 introduces no new top-level Cargo deps under the recommended posture (the new `envoy-accesslog` crate's deps are all already in the workspace's resolved graph: `tokio`/`bytes`/`tracing`/`thiserror`/`envoy-http1` as workspace path-dep). M5/M9 carries forward unchanged. **If conditional ADR-0030 lands** (foundations grant for `time` or `async_trait`; explicitly NOT recommended), this would be a natural site to ratify a Cargo.lock cadence ADR. ADR-0030 is NOT projected to land per §7 below.
- **Phase 04.1 REVIEW M7** (TLS+H2 ALPN-driven dispatch generalization). 06.2 doesn't ship TLS or H2 surfaces; M7 carries forward unchanged.
- **Phase 02.2 REVIEW M1** (`*EchoBackend::Drop` polling loop blocks on `std::thread::sleep`). Standing carryforward; 06.2 does not parallelize `run_fixture` so M1 continues unchanged.
- **Phase 04.1 REVIEW M1/M2/M4** (`diff_headers` value-comparison; body-drain idle timeout; `strip_port` IPv6). All three may surface latently under specific access-log assertion patterns but fixture 0012 does not exercise duplicate response headers, body-drain stalls, or IPv6 hosts. M1/M2/M4 continue tracking forward unchanged.

**Not deferred — confirmed in scope for 06.2** (for clarity, since these have predictable confusion points):

- The `envoy-accesslog` crate ships in 06.2 with the 4 modules (`record`, `file_sink`, `default_format`, `error`) plus the `sink` placeholder. No `client.rs`, no `grpc_sink.rs`, no `stdout_sink.rs`.
- `tests/fixtures/0012-access-log-file-sink/` IS created in 06.2.
- `BEHAVIOR_CONTRACT.md` IS edited in 06.2 (per §2 / D5.2 above) — the `Access log field mapping` section's first-time population is the load-bearing doc-only delta of 06.2.
- The `HttpConnectionManagerConfig.access_log` field IS added in 06.2 (per D2.2 above).
- Both H1 and H2 HCM paths gain the access-log dispatch (per D3.2 above; H2 inherits via the `HCMConfig` type-alias from 05.2 D1) — but only the H1 path is asserted by fixture 0012; H2-side access-log fixture lands in a future phase.
- The `Sink` TRAIT does NOT ship in 06.2 (option (c) per parent SPEC §3 D8.2). `FileSink` is concrete; `HCMConfig.access_log: Vec<Arc<FileSink>>` is concretely typed.

---

## 5. HCM access-log wiring posture (D3.2 detail)

This section expands D3.2's runtime dispatch posture into PLAN-ready detail. Read alongside §3 architectural Rule 4 (fire-and-forget HCM emission).

**Dispatch site location.** The H1 dispatch site is `crates/envoy-http1/src/hcm.rs` inside `serve_connection`'s per-request loop body, AFTER `write_response` returns successfully and BEFORE the next iteration of the keep-alive loop (or before the connection close path on `Connection: close`). The H2 dispatch site is `crates/envoy-http2/src/hcm.rs` inside the per-stream `tokio::task` body that 05.2 D3 landed, AFTER `send_data(.., end_of_stream=true)` writes the response and BEFORE the spawned task drops cleanly.

**Record build mechanics.** The HCM captures a `SystemTime` and an `Instant` at request-arrival time (immediately after the codec produces the parsed `Request` value type, before any route-walk). The `SystemTime` is for `%START_TIME%` rendering (UTC wall-clock); the `Instant` is for `%DURATION%` measurement (monotonic; `start.elapsed()` at record-build time). Both captures are stored on the per-request stack (no per-connection state required).

The record-build site assembles the `AccessLogRecord` by:
1. `start_time: SystemTime` from the request-arrival capture.
2. `method: String` from `request.method.as_str().to_owned()`.
3. `path: String` from a small helper that returns the `x-envoy-original-path` request header value if present, else `request.path.clone()` (per §2 row for `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%`).
4. `protocol: String` — `"HTTP/1.1"` on H1, `"HTTP/2"` on H2 (the literal is determined by which HCM module the dispatch is happening in, not by the request shape).
5. `response_code: u16` from `response.status`.
6. `response_flags: String` — `"-"` literal in 06.2 (no flag emission per D3.2 above).
7. `bytes_received: u64` from `request.body.len() as u64`.
8. `bytes_sent: u64` from `response.body.len() as u64`.
9. `duration: Duration` from `start.elapsed()` where `start: Instant` was captured at request-arrival.
10. `upstream_service_time: Option<Duration>` — extracted from the response's `x-envoy-upstream-service-time` header value (parsed as u64 ms; rendered as `Duration::from_millis(v)` if present and parses cleanly; `None` otherwise).
11. `forwarded_for: Option<String>` — request-side `x-forwarded-for` header value if present, else `None`.
12. `user_agent: Option<String>` — same shape for `user-agent`.
13. `request_id: Option<String>` — same shape for `x-request-id`.
14. `authority: Option<String>` — request-side `host` header value if present, else `None` (the H2 path's `:authority → Host:` translator landed in 05.2 D3 ensures this field is populated whether the request came in on H1 or H2).
15. `upstream_host: Option<String>` — populated from the router-arm's resolved upstream `SocketAddr` (formatted as `addr:port`; see signpost 11 in §6 below for the formatting rule). `None` on direct_response paths.

**Dispatch posture choice — option (a) vs option (b).** Per §3 architectural Rule 4 above, the planner picks at PLAN-write time:
- **Option (a) — `tokio::spawn` fire-and-forget.** Each access-log emission is dispatched in a spawned `tokio::task` that owns a cloned `Arc<FileSink>` + an owned `AccessLogRecord` (cloned from the per-request stack). The HCM's per-request task returns immediately after the spawn; emission errors are logged inside the spawned task via `tracing::warn!`. Pros: emission latency does not extend the per-request handling latency; multiple concurrent emissions can interleave at the OS-level write boundary (mediated by the `tokio::sync::Mutex<File>` inside `FileSink`). Cons: spawned tasks must be cleanly shut down at envoy-bin shutdown time (otherwise the in-flight emissions may be lost); the additional `Arc::clone` + `record.clone()` per request adds a small per-request overhead; spawned-task lifecycle is harder to reason about in tests.
- **Option (b) — synchronous-after-write.** The HCM awaits `sink.emit(&record).await` after `write_response` returns. The per-request task duration extends by the sink emission latency. Pros: simpler reasoning; no Arc cloning into a spawned task; no spawned-task-leak concerns; tests can deterministically verify the access-log file contents after the request completes. Cons: emission latency adds to per-request latency (small for FileSink — typically sub-millisecond); future I/O-heavy sinks (gRPC ALS) would force a switch to option (a).

**Recommendation: option (b) synchronous-after-write.** Simpler reasoning posture; acceptable latency cost for FileSink (the only sink in 06.2); future migration to option (a) is mechanical (replace the await with a spawn) when I/O-heavy sinks land. The signpost 4 in §6 below records both options for the planner's reference; the SPEC's recommendation is (b).

**Emission failure posture.** Either option above maps emission errors to `tracing::warn!`-and-continue. The HCM does NOT retry, does NOT block, does NOT abort the request. If the sink's underlying file becomes unwritable mid-runtime (filesystem full, permissions changed, etc.), each subsequent emission logs a warn and continues; the request handling is unaffected. The `access_logs_total` counter that 06.3 D15.3 lands increments at queue-enter time (before the await), per parent §6 Rule 4 — emission failures do NOT deflate the count. (06.2 does not ship the counter; this is a forward-compat note for 06.3.)

**Stats coupling — NONE in 06.2.** The HCM access-log dispatch site adds NO counter increments, NO gauge updates, NO `envoy-stats` registry interactions. The 06.1-landed `envoy-stats::StatsRegistry` is not threaded through `envoy-accesslog`; the new `FileSink::emit` does not call into the registry. This isolation is intentional — it lets 06.3 add the `access_logs_total` counter as a clean, additive change to the HCM dispatch site without coupling the access-log subsystem to the stats subsystem.

---

## 6. Implementation signposts (open questions deferred to PLAN-write time)

Notes flagging predictable planner questions so the 06.2 planner resolves them in-plan rather than mid-execution. Inherits parent-06 SPEC §6 cross-sub-phase rules where they bind on 06.2, plus 06.2-local signposts.

**Signpost 1 — ISO-8601 emitter buffer shape (`String` vs `&mut [u8]`).** The `format_iso8601` helper in `default_format.rs` emits 24 ASCII bytes of `YYYY-MM-DDTHH:MM:SS.sssZ`. Two natural choices: (a) `fn format_iso8601(s: &mut String, t: SystemTime)` — appends to a reused `String` buffer; the format() function uses `write!(s, ...).unwrap()`. Pros: ergonomic; fits the format() function's `String`-building style. (b) `fn format_iso8601(buf: &mut [u8; 24], t: SystemTime)` — writes into a fixed-size byte array; format() copies 24 bytes from the array into its String buffer. Pros: no allocation; perfect-size pre-known; can be `const fn`-shaped if the planner cares. Cons: more cumbersome integration. **Recommendation: (a) `&mut String`.** Perf is not a concern (one emission per request, not in a hot loop); ergonomics + style match dominates.

**Signpost 2 — Gregorian calendar helper inline vs separate module.** The `epoch_seconds_to_ymd_hms` function is ~30 LoC of straightforward arithmetic. Two natural placements: (a) inline in `default_format.rs` as a private `fn`; format_iso8601 calls it. (b) separate `mod gregorian` (or `gregorian.rs`) with the function `pub(crate)`-visible. **Recommendation: (a) inline.** ~30 LoC doesn't justify a separate module; co-locating the arithmetic with its sole consumer (format_iso8601) keeps the doctrine surface tight. If the function grows beyond ~50 LoC during execution (e.g., the planner adds support for non-UTC time zones — which is OUT of scope per §4 above but the planner may end up with timezone-handling helpers anyway), the planner promotes to (b) at that time.

**Signpost 3 — `tokio::sync::Mutex<File>` vs `File`-per-emit.** The `FileSink::emit` body needs to serialize concurrent writes from concurrent HCM in-flight requests on the same listener. Two patterns: (a) `Arc<tokio::sync::Mutex<File>>` shared across emissions (the `FileSink` holds the Arc; emit() awaits the lock; writes; releases). Pros: append-semantic atomicity preserved (one writer at a time inside the process; the OS-level append() is atomic per write_all call so cross-process write interleaving is also handled at the inode level on append-mode files). Cons: lock contention under high concurrency. (b) `FileSink::emit` opens a fresh `File` per call (`OpenOptions::new().append(true).open(path).await`); writes; closes. Pros: no in-process lock; relies on OS-level append-mode atomicity. Cons: per-emit `open` syscall cost; some filesystems' append-mode atomicity is weaker than expected. **Recommendation: (a) `Arc<Mutex<File>>`.** Lock contention is negligible for FileSink (one write per request; mutex acquisition is sub-microsecond); the in-process serialization simplifies reasoning and avoids per-emit syscall overhead.

**Signpost 4 — HCM emission spawn (option (a)) vs synchronous-after-write (option (b)).** Per §3 Rule 4 + §5 above. **Recommendation: (b) synchronous-after-write.** The signpost is recorded both in §3 and §5 above plus here for cross-reference; the planner picks at PLAN-write time and records the choice in PROGRESS Task corresponding to D3.2.

**Signpost 5 — `AccessLogRecord` ownership (owned `String` vs `&str` borrows).** The struct fields are owned `String`s (per the §3 D1.2 definition). Alternative considered: borrow `&'a str` from the per-request `Request` / `Response` value types via a generic lifetime parameter on `AccessLogRecord<'a>`. **Rejected: owned `String`s.** Reasons: (a) the spawned-task posture (option (a) of signpost 4) requires owned data — borrowing across a spawn boundary requires either lifetime-static borrows (impossible for per-request data) or `Arc<str>` indirection (an extra allocation per emission). (b) Even under the synchronous-after-write posture (option (b)), the format() function consumes the record by reference (`&AccessLogRecord`); owning the strings inside the record gives the format() function a stable `&str` view via `&record.method.as_str()` etc. (c) Per-request allocation cost is negligible for the FileSink workload (Strings are 24 bytes each on 64-bit; total record allocation is ~15 × 24 = ~360 bytes plus the heap-allocated string content; both are sub-microsecond costs). The recommendation stands per the §3 D1.2 definition; the planner does not revisit at PLAN-write time.

**Signpost 6 — `FileSink` path validation.** The `FileSink::new(path)` constructor calls `tokio::fs::OpenOptions::new().append(true).create(true).open(&path).await`. It does NOT pre-validate the path (e.g., does not check that the parent directory exists, does not check path is absolute vs relative, does not check disk space). Errors surface as `AccessLogError::Open { path, source: io::Error }`. The validator at `envoy-config` D2.2.b checks ONLY that `path` is non-empty (rejecting empty strings); deeper validation defers to runtime open-time. Two questions: (a) Should `envoy-config` validate path-format more strictly (reject paths with embedded NUL bytes, reject paths longer than PATH_MAX, etc.)? **Recommendation: NO.** Defer to runtime; let the OS-level open() surface the error. (b) Should `FileSink::new` create missing parent directories (mkdir -p shape)? **Recommendation: NO.** Treat the operator's specification of `path` as authoritative; an unwritable path is an operator misconfiguration that surfaces at startup time. Both decisions can be revisited at REVIEW.md if empirical use during 06.2 task time surfaces a different need.

**Signpost 7 — `O_APPEND` semantics on existing files.** `tokio::fs::OpenOptions::new().append(true).create(true).open(...)` translates to the OS-level `O_APPEND | O_CREAT | O_WRONLY`. If the file already exists, append() positions the write cursor at the end of the existing content; subsequent emissions write past the existing tail. Two questions: (a) Should envoy-rust truncate existing log files at startup? **Recommendation: NO.** Envoy v1.33 does not truncate; both proxies must align on the append-not-truncate posture for fixture 0012 to be byte-equivalent across multiple runs. The fixture-0012 harness ensures a clean log file at fixture-start time by deleting any existing `/tmp/0012-*-access.log` before envoy-bin / Envoy starts. (b) Should envoy-rust handle log-rotation (an operator deletes and recreates the file mid-run)? **Recommendation: NO.** OS-level semantics under UNIX: writes after rotation continue to the unlinked inode (the old file stays alive as long as the process holds the FD; the rotated-out file is unreachable from the filesystem but data is not lost). This is the standard UNIX behavior; rotation hooks defer per §4 above.

**Signpost 8 — `AccessLogLineRule` tokenizer shape.** The harness tokenizer at `tests/differential/src/access_log.rs` parses the default-format line. The parser walks the line character-by-character, recognizing `[%START_TIME%]` brackets, `"..."` quoted-token boundaries, and unquoted-token whitespace separators. Two natural approaches: (a) hand-rolled state machine — ~80 LoC of `match (state, ch) { ... }` arms. (b) regex-based extraction — using the `regex = "1"` crate already in the workspace per ADR-0021 (in envoy-config, but extending its scope to the differential harness is a question); ~30 LoC of regex + capture groups. **Recommendation: (a) hand-rolled.** ADR-0021 narrowly scopes `regex` to `envoy-config` (header / route matching at config-load time); extending to the differential harness would expand the ADR-0021 surface and is unnecessary for the simple default-format grammar. The hand-rolled tokenizer is ~80 LoC, predictable, and parser-error-friendly.

**Signpost 9 — `%DURATION%` units (ms vs μs vs ns).** Envoy's documented default format renders `%DURATION%` in **milliseconds** per the v1.33 docs cross-check (verifiable at planner cross-check time against `https://www.envoyproxy.io/docs/envoy/v1.33.0/configuration/observability/access_log/usage#format-rules`). envoy-rust's `default_format::format` uses `record.duration.as_millis()` for the rendering. Two follow-up questions: (a) Should envoy-rust render fractional milliseconds (e.g., `1.5` for 1500μs)? **Recommendation: NO.** Envoy emits integer milliseconds; envoy-rust matches. (b) Should envoy-rust pre-saturate against a maximum (e.g., `u32::MAX` ms ≈ 49 days; no request should exceed this in practice)? **Recommendation: NO.** Use `u128 → str` rendering of `duration.as_millis()` (which returns u128 in Rust); both proxies will render the same value for the same request.

**Signpost 10 — `x-envoy-original-path` fallback to `:path` semantics.** Envoy's `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` token uses the `?` operator to fall back: if the first header (`X-Envoy-Original-Path`) is present on the request, emit its value; otherwise emit the second (`:path` pseudo-header / request-target). envoy-rust's record-build site reproduces this via:

```rust
fn x_envoy_original_path_or_path(req: &Request) -> &str {
    for (name, value) in req.headers.iter() {
        if name.eq_ignore_ascii_case("x-envoy-original-path") {
            return value.as_str();
        }
    }
    req.path.as_str()
}
```

The `eq_ignore_ascii_case` posture is defensive (envoy-rust normalizes headers to lowercase per the 04.x posture, so the case-insensitive check is technically redundant on the H1 path; it's load-bearing on the H2 path where the codec already lowercases per RFC 7540 §8.1.2 — but pre-aware-of-the-04.1-shape posture, the case-insensitive check is the minimum-surprise default). The H2 path's `:path` pseudo-header is translated into `Request.path` by 05.2 D3's `request.rs` adapter; the same fallback logic works on both H1 and H2. **No protocol-specific handling needed** — the envoy-rust normalization at the codec edge ensures the record-build site sees a uniform `Request` shape.

**Signpost 11 — `%UPSTREAM_HOST%` format (`addr:port` vs `ip:port` literal vs DNS-name:port).** Envoy's `%UPSTREAM_HOST%` token renders the resolved upstream host:port. For STRICT_DNS clusters, this is the resolved A-record IP:port literal (e.g., `127.0.0.1:8080`). For STATIC clusters, this is the literal-IP:port the config declares. envoy-rust's record-build site renders the captured `SocketAddr` via `format!("{}", socket_addr)` which produces the standard Rust `SocketAddr` Display impl (e.g., `127.0.0.1:8080` for IPv4, `[::1]:8080` for IPv6). **Recommendation: use the SocketAddr Display impl directly.** This matches Envoy's literal-IP:port rendering exactly for the IPv4 case (fixture 0012's direct_response path doesn't engage upstream resolution; future fixtures with router-proxy paths will engage). For IPv6, envoy-rust renders `[::1]:8080` per RFC 5952 / Rust's standard Display; Envoy renders the same shape per its v1.33 cross-check. If a divergence surfaces at fixture-write time for a future fixture, an entry lands in BEHAVIOR_CONTRACT.md `Access log field mapping` to record the disposition (likely `name-required, value-may-differ` if any IPv6 rendering edge case bites).

**Signpost 12 — Fuzz corpus seed minimal-vs-multi-sink.** The corpus seed `hcm_access_log_file.yaml` per D2.2 ships with ONE access-log entry. The validator's accept-path on the file-sink type URL is exercised by this one seed. Question: should the seed exercise multiple entries (e.g., two file-sink entries pointing to different paths)? **Recommendation: NO.** The validator gates on per-entry shape; a single-entry seed exercises the validator path completely. Multi-entry seeds defer to a future Sink-trait phase where multi-sink dispatch is the load-bearing question. The seed stays single-entry in 06.2.

**06.2-local additional signposts:**

**Signpost 13 — Request-arrival timing capture site.** The `start_time: SystemTime` and `start_instant: Instant` captures at request-arrival time live at the HCM `serve_connection` per-request loop body, immediately after the codec produces the parsed `Request` value type. The exact site lands at PLAN-write time (the planner cross-checks 04.1's `serve_connection` shape at 06.2 Task 1 for the post-`read_request` line); the pseudocode in §3 D3.2 above is shape-accurate.

**Signpost 14 — `Default` impl on `AccessLogRecord` — REJECTED.** Per §3 D1.2, the struct does NOT implement `Default`. Defaulting silently could mask field-population omissions at the HCM record-build site (e.g., a future code path forgets to populate `protocol` and the struct silently emits `""` for that field). The planner enforces the no-Default posture; the only construction path is full-field literal struct construction at the HCM site. The unit tests in D1.2 build records via full-field literal construction.

**Signpost 15 — Unit test logging capture.** D3.2 test 3 (`hcm_with_file_access_log_emission_failure_does_not_fail_request`) needs to assert that a `tracing::warn!` log line was emitted on emission failure. The planner picks between (a) `tracing-test = "0.2"` dev-dep (NOT currently in workspace; would require cargo deny check), (b) `tracing-subscriber` with a custom in-process layer (no new dep; ~30 LoC of test fixture), (c) skip the warn-line assertion and assert only that the request succeeded. **Recommendation: (b) — custom in-process layer**. Avoids a new dev-dep; the layer impl is mechanical. Reuse pattern across other tests if needed.

**Signpost 16 — `#![forbid(unsafe_code)]`** is added to `crates/envoy-accesslog/src/lib.rs` per D-3.8. No `unsafe` in 06.2.

**Signpost 17 — PLAN.md cadence — standalone pre-Task-1 commit.** Per the established phase-precedent (phase-04.3's `c02eea7`, phase-05.1 / 05.2 / 05.3 / 06.1 each), each sub-phase's planner commits PLAN.md cleanly at state-2 close-out, before any Task 1 commit. The 06.2 PLAN.md is committed standalone, not folded into the Task 1 commit.

**Signpost 18 — In-process integration backstops.** 06.2's fixture 0012 gains an in-process backstop at `crates/envoy-bin/tests/access_log_file_sink.rs` (sibling of 04.1's `http1_direct_response.rs` and 05.2's `http2_direct_response.rs`). The backstop spawns envoy-bin via `CARGO_BIN_EXE_envoy-bin` against an HCM with a file-sink config at a tempdir path; drives a single `GET /` request via the standard library (no fancy client needed for HTTP/1.1); reads the access-log file post-request; asserts the line tokens. ~120 LoC.

**Signpost 19 — `anyhow` boundary.** Tests in `crates/envoy-bin/tests/*` use `anyhow` per D-3.2 (binary-crate package). The `tests/differential/` crate continues `anyhow::Result<()>` returns. The new `crates/envoy-accesslog/` crate is library-only and uses `thiserror`-based `AccessLogError`; no `anyhow` usage inside `envoy-accesslog`.

**Signpost 20 — Cargo.lock cadence.** Per the established phase-precedent (phase-01 `4955252` etc., phase-04.x inline, phase-05.x inline). 06.2 introduces no new top-level Cargo deps under the recommended posture (the new `envoy-accesslog` crate's deps are all already in the workspace's resolved graph). Cargo.lock sync at scaffold time (Task corresponding to D1.2) is anticipated to be a no-op or minimal diff (only the new crate's `[[package]]` entry); the state-4 phase-done verification commit cross-checks the diff.

**Signpost 21 — `deny.toml` license allow-list — likely no-op.** The new `envoy-accesslog` crate's deps are all already covered by the existing allow-list. Cross-checked at state-4; an inline addition lands only if a new transitive crate brings a new license (none anticipated).

**Signpost 22 — Carryforwards from 06.1 — none active at 06.2 entry.** 06.1's REVIEW.md verdict is anticipated to be Approved with M-track follow-ups at most; awareness-only items don't bind on 06.2. Phase-04.1 REVIEW M-claim and the various 05.x carryforwards continue tracking unchanged.

---

## 7. ADR projection

Phase 06.2's ADR ledger entrance state is **ADR-0029** (landed at parent-06 state-2 alongside this SPEC; records the parent-06 split decision per parent SPEC §7). 06.2 projects the following ADRs:

### Conditional ADR-0030 (foundations grant for `async_trait = "0.1"` or `time = "0.3"`) — **NOT recommended**

- **Status:** **NOT projected to land.** This SPEC commits to the recommended posture per parent-06 SPEC §7's projection: **option (c) per D8.2** (defer the `Sink` trait until N≥2 sinks exist) + **hand-rolled ISO-8601 emitter** (no `time` / `chrono` dep). The conditional ADR-0030 number stays available in the ledger for whichever sub-phase first needs it.
- **When it would land:** if execution-time experience materially worsens beyond brainstorm-time estimates. Specifically:
  - If the hand-rolled `epoch_seconds_to_ymd_hms` proves more painful than estimated (e.g., the planner discovers that handling sub-millisecond resolution + IANA timezone names + leap-second handling forces the function to grow beyond ~80 LoC), the planner lands an in-execution ADR-0030 per D-3.5 narrowly scoped to `time = "0.3"`. **Not anticipated at SPEC-write time.** The 14 unit tests in D1.2 are sufficient to validate the hand-rolled emitter; test 12 (`epoch_seconds_to_ymd_hms_known_dates`) covers the load-bearing edge cases (leap years, century boundaries).
  - If the planner discovers at PLAN-write time that option (c) per D8.2 (defer the `Sink` trait) is materially worse than option (b) (introduce `async_trait`) — e.g., the HCM dispatch site becomes substantially uglier when the sink dispatch can't dyn-dispatch — the planner reconsiders. **Not anticipated at SPEC-write time.** The HCM dispatch site reads cleanly under option (c): `for sink in &config.access_log { sink.emit(&record).await; }` works with `Vec<Arc<FileSink>>` directly.
- **Provenance:** projected as conditional in parent-06 SPEC §7 (*"Conditional ADR-0030 (foundations grant for `time = "0.3"` or `async_trait = "0.1"`). Not pre-projected. The recommended posture is no foundations grants in phase 06."*). Lands at 06.2 Task time IF execution surfaces the need; otherwise the number stays available.

### Conditional ADR-0031 (Cargo.lock cadence ratification ADR) — **NOT recommended**

- **Status:** **NOT projected to land.** Phase-04.1 REVIEW M5/M9 carries forward unchanged unless ADR-0030 actually lands and forces a cadence pick. If ADR-0030 does NOT land (per the recommendation above), M5/M9 continues to phase 07.
- **Provenance:** projected as conditional in parent-06 SPEC §7. Conditional on ADR-0030 landing first.

### No additional ADRs anticipated for 06.2

The HCM access-log dispatch and the `envoy-accesslog` crate scaffold are mechanically scoped per the parent-06 brainstorm; no additional Y/N decision points are projected at execution time. If a Y/N decision surfaces during execution that isn't covered by ADR-0030 (e.g., a `BEHAVIOR_CONTRACT.md` allow-list extension forced by an unexpected access-log-related response header surface, or a `default_format::format` semantic edge case that diverges from Envoy's behavior in a way that warrants policy-grade documentation), the planner appends the next-sequential ADR (ADR-0030 or ADR-0031 depending on the prior conditional landings) at the time it lands.

**ADR-renumbering provenance discipline.** Per the established ledger discipline (parent-04's ADR-0020 + ADR-0021 landed without renumbering; parent-05's ADR-0022 + ADR-0023 landed without renumbering; conditional ADR-0024 and ADR-0025 from 05.2 landed at the next-sequential numbers per their actual landing sequence; ADR-0026 / ADR-0027 / ADR-0028 followed). If conditional ADR-0030 does not land in 06.2, its number stays available for 06.3 or later phases.

---

## 8. State-machine signposts for 06.2's own state-2 session

Sub-phase 06.2's state-2 session (the next session after this SPEC's state-1 close-out commit; per `SKILL_ROUTING.md` line 21: *"SPEC.md exists, PLAN.md does not → superpowers:writing-plans → output: PLAN.md → GATE: if PLAN.md > ~25 tasks OR > ~1500 LoC estimated → split into NN.1, NN.2, …; update ROADMAP + STATE; stop"*). The 06.2 state-2 session lands:

1. `docs/envoy-rust/phases/06.2-access-log/PLAN.md` — refining D1.2–D6.2 from §3 above into the per-task PLAN-ready cadence the project follows. **Estimated ~11 tasks** within the §6.1 split-gate (~25-task ceiling); the LoC estimate of ~1580 LoC marginally exceeds the ~1500 LoC split-gate by ~5% which is well within drift tolerance per parent-05 SPEC §5 *"do not nest-split a sub-phase that was itself produced by a split"*. The PLAN.md is committed standalone, not folded into the Task 1 commit per signpost 17 above.

2. `docs/envoy-rust/STATE.md` — at the PLAN.md commit (state-2 close-out): active phase id stays `06.2`; lifecycle state advances 2 → 3 (PLAN.md exists, implementation incomplete); next-skill advances to `superpowers:subagent-driven-development` per the user's standing preference (per auto-memory `feedback_execution_style`; matches 05.x / 06.1's posture).

3. `docs/envoy-rust/ROADMAP.md` — row `06.2` flips `status: planned` → `status: in-progress` (06.2 is now actively executing; mirrors the 05.x / 06.1 flip-on-state-2 posture per the `BOOTSTRAP_PROMPT.md` §4.1 invariant *"a phase enters `in-progress` only when STATE.md points at it"*).

The 06.2 state-2 session does NOT land per-task PROGRESS.md updates — those land at each Task's state-3 cadence per the established phase-precedent. The state-2 session writes PLAN.md only.

**Per-task tasks projected at PLAN-write time** (estimated ~11 tasks; the planner cross-checks at state-2 to confirm the breakdown):

1. Task 1 — Scaffold `crates/envoy-accesslog/` (Cargo.toml + lib.rs + module skeletons + workspace registration + `error.rs` + `record.rs` struct definition + record unit tests). Includes Cargo.lock sync.
2. Task 2 — `default_format::format` emitter + `format_iso8601` + `epoch_seconds_to_ymd_hms` + 8 unit tests in `default_format.rs`.
3. Task 3 — `FileSink` impl + 4 unit tests in `file_sink.rs`.
4. Task 4 — `envoy-config` schema additions (D2.2.a + D2.2.b) + 6 unit tests + 1 corpus-walk test + fuzz seed.
5. Task 5 — HCM access-log wiring on H1 path (D3.2 H1-side) + 4 unit tests in `envoy-http1::hcm::tests`.
6. Task 6 — HCM access-log wiring on H2 path (D3.2 H2-side via the type-aliased HCMConfig) + 2 unit tests in `envoy-http2::hcm::tests`.
7. Task 7 — In-process integration backstop at `crates/envoy-bin/tests/access_log_file_sink.rs`.
8. Task 8 — Differential harness extension (`Driver::Http1WithAccessLog` + `AccessLogLineRule` + tokenizer + dispatch arm) + 4 unit tests.
9. Task 9 — Fixture `tests/fixtures/0012-access-log-file-sink/` (5 files) + Docker-gated test wrapper.
10. Task 10 — BEHAVIOR_CONTRACT.md `Access log field mapping` first-time population (D5.2). May fold into Task 9's fixture commit if cadence demands; recommendation is a separate task to keep the doc-only edit reviewable in isolation.
11. Task 11 — State-4 phase-done verification (D6.2; PROGRESS.md captures all 6 gate elements with inline evidence; no code).

The Task ordering above is pre-PLAN-write; the planner may reorder (e.g., land Task 4 before Task 1 to scaffold the schema before the crate, mirroring 05.2's posture) at state-2 time.

**Sub-phase entry point.** After 06.2 state-2 lands, the next session enters phase 06.2 lifecycle state 3 — runs `superpowers:subagent-driven-development` per the user's standing preference, executes the PLAN.md tasks one-by-one, and the cycle continues through state 4 (phase-done verification), state 5 (REVIEW.md), state 6 (phase-done commit).

**Execution invariants (unchanged from parent-06 + the established phase-precedent):**
- Sub-phases ship strictly in order. 06.3 cannot start before 06.2's state-6 close-out commit per parent SPEC §5 sub-phase ordering invariant.
- 06.2 honors the phase-done gate from `BOOTSTRAP_PROMPT.md` §7.5 in full at its state-4.
- 06.2 produces its own REVIEW.md at state-5 per `superpowers:requesting-code-review`.
- The parent-06 state-6 close-out happens at 06.3's state-6 commit (the last sub-phase's commit also flips parent row 06 to `done`), per the ROADMAP-schema invariant in `BOOTSTRAP_PROMPT.md` §4.1.

---

## 9. Final commit message format (for state 6 of the 06.2 lifecycle)

The 06.2 phase-done commit flips ROADMAP row `06.2` `in-progress` → `done`. Parent row `06` stays `in-progress` (06.3 is the closing sub-phase). The commit format models the 04.x / 05.x sub-phase shape (e.g., 05.2's `phase 05.2: envoy-http2 + HCM-on-H2 + fixture 0009 + h2spec ≥95% [<ADR-NNNN, ...>]`):

```
phase 06.2: envoy-accesslog + Envoy default format + HCM access-log wiring + fixture 0012 [<ADR-NNNN, ...>]

New workspace member crates/envoy-accesslog/ ships as the workspace's
sole-dep-owner of the access-log surface per the cross-sub-phase
architectural rule established by parent-06 ADR-0029 (mirrors envoy-http1's
sole-owner-of-httparse posture from 04.1, envoy-tls's sole-owner-of-rustls
from 03.1, envoy-http2's sole-owner-of-h2 from 05.2, envoy-stats /
envoy-admin's sole-ownership posture from 06.1). Module decomposition:
record + file_sink + default_format + error (the sink trait module is a
placeholder; the trait is intentionally NOT shipped per parent-06 SPEC
§3 D8.2 option (c)). No new permitted-foundations grants — ISO-8601
emitter hand-rolled with a Gregorian calendar arithmetic helper + golden
tests; FileSink ships concretely; the Sink trait + multi-sink dispatch
defer to whichever later phase first ships a second sink type.

envoy-config schema additions: HttpConnectionManagerConfig.access_log:
Vec<AccessLogConfig>; AccessLogConfig with name + typed_config; the
validator gates on name = "envoy.access_loggers.file" + @type =
type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
and rejects others with ConfigError::UnsupportedAccessLogType. ~6 new
validator unit tests + 1 fuzz corpus seed (hcm_access_log_file.yaml).
Format-string customization is OUT of scope (the validator ignores any
user-supplied format/log_format/json_format fields on FileAccessLog
typed_configs).

HCM access-log wiring: HCMConfig grows access_log: Vec<Arc<FileSink>>;
HCM at on-response-complete time builds an AccessLogRecord (15 fields)
and dispatches synchronously-after-write to each configured sink;
emission errors logged via tracing::warn! per parent-06 SPEC §6
architectural Rule 4 (fire-and-forget; emission failures must NOT
affect the response-write path). The H2 path inherits the wiring
transparently via the HCMConfig type-alias from 05.2 D1.

BEHAVIOR_CONTRACT.md `Access log field mapping` section populated for
the first time in the project's history. 14 rows — one per Envoy
default-format token. Per-token equivalence dispositions: value-exact
for deterministic-load tokens (%REQ(:METHOD)%, %REQ(:AUTHORITY)%,
%PROTOCOL%, %RESPONSE_CODE%, %BYTES_RECEIVED%, %BYTES_SENT%,
%RESPONSE_FLAGS%-as-"-", %REQ(X-FORWARDED-FOR)%, %REQ(USER-AGENT)%,
%REQ(X-REQUEST-ID)%, %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%, %UPSTREAM_HOST%-
as-"-"); name-required-value-may-differ for wall-clock-non-deterministic
tokens (%START_TIME%, %DURATION%, %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%
inheriting the existing Header allow-list disposition). The fixed
default-format sequence is reproduced per Envoy v1.33's documentation.

Differential harness gains Driver::Http1WithAccessLog + AccessLogLineRule
(per-token rule: Exact / Iso8601Format / DurationMs / Wildcard /
EnvoyOnly) + a hand-rolled default-format tokenizer.

Fixture 0012-access-log-file-sink (5 files): HCM codec_type: HTTP1 +
direct_response 200 "ok\n" + access_log: [{ name: envoy.access_loggers.
file, typed_config: { @type: ".../v3.FileAccessLog", path: /tmp/0012-*-
access.log } }]. The harness reads each proxy's access-log file post-
request and asserts per-token equivalence.

ADR-0030 (CONDITIONAL — foundations grant for time = "0.3" or
async_trait = "0.1") explicitly NOT projected to land. The recommended
posture per parent-06 SPEC §7 is no foundations grants in phase 06; the
hand-rolled ISO-8601 emitter + the deferred Sink trait both honor
D-3.2's hand-rolled-from-scratch doctrine for access-log formatters and
sinks. ADR-0031 (Cargo.lock cadence) stays conditional on ADR-0030 and
is also NOT projected to land.

Phase-04.1 REVIEW M5/M9 (Cargo.lock cadence) carries forward unchanged
through 06.2; the new envoy-accesslog crate's deps are all already in
the workspace's resolved graph (no new top-level Cargo deps under the
recommended posture).

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (unchanged);
  tests/fixtures/0004-tls-downstream green (unchanged);
  tests/fixtures/0005-tls-upstream green (unchanged);
  tests/fixtures/0006-tls-sni green (unchanged);
  tests/fixtures/0007-http1-direct-response green (unchanged);
  tests/fixtures/0008-http1-router-upstream green (unchanged);
  tests/fixtures/0009-http2-direct-response green (unchanged);
  tests/fixtures/0010-http2-router-upstream green (unchanged);
  tests/fixtures/0011-admin-stats-prometheus green (unchanged);
  tests/fixtures/0012-access-log-file-sink green (NEW; HCM access-log
    file-sink dispatch end-to-end).
Conformance: tests/conformance/h2spec at ≥95% pass (carried forward
  from 05.2 baseline; no regression from access-log wiring).
```

The commit title does NOT carry the `[parent 06 done]` tag — 06.2 is not the closing sub-phase. The bracketed ADR list is at minimum empty (no ADRs anticipated to land in 06.2 under the recommended posture); if conditional ADR-0030 lands, the list becomes `[ADR-0030]`; if both ADR-0030 + ADR-0031 land, `[ADR-0030, ADR-0031]`.

ROADMAP row `06.2` flips `in-progress` → `done` at this commit. Parent row `06` stays `in-progress`. STATE.md advances to phase `06.3` lifecycle state 2 (06.3's SPEC was landed at parent-06 state-2 alongside this one); next-skill `superpowers:writing-plans` scoped to sub-phase 06.3 (comprehensive stats wiring + 05.3 I1 closure + parent-06 close per parent-06 SPEC §3 D14.3–D20.3). Phase-06's projected ADR ledger after this commit: ADR-0029 (parent-06 split decision; landed at parent-06 state-2 alongside this SPEC). Future ADRs from 06.3 land at the next-sequential numbers (ADR-0030+ if no 06.2-conditional ADR landed; ADR-0031+ if only one landed; ADR-0032+ if both landed).

---

## 10. State-machine commit (this commit — parent-06 state-2 close-out reference)

Parent-06 state-2 lands this SPEC alongside ADR-0029 (the parent-06 split decision) + the 06.1 SPEC + the 06.3 SPEC + the new ROADMAP rows for 06.1 / 06.2 / 06.3 + the parent ROADMAP row 06's `sub-phases` column update. STATE.md advances to phase `06.1` lifecycle state 1 (06.1 enters its own state-1 brainstorm cadence next). Per the established phase-precedent (parent-04 state-2 commit `1d9740d`, parent-05 state-2 commit `f1804a7`).

This SPEC (06.2) is NOT separately committed — it lands in the same commit as 06.1 / 06.3 SPECs + ADR-0029 + the ROADMAP / STATE updates. The commit's title format models the parent-04 / parent-05 state-2 commit shape.

**Sub-phase entry into 06.2 state-1.** After parent-06 state-2 lands, 06.1 enters state 1 first per the sub-phase ordering invariant (06.1 → 06.2 → 06.3). 06.2's state-1 session runs `superpowers:brainstorming` scoped to this SPEC's surface; lands its own state-1 close-out commit (no SPEC edit; STATE.md advances `06.2` lifecycle state 1 → 2; next-skill `superpowers:writing-plans`). 06.2's state-2 session lands PLAN.md per §8 above. Execution proceeds.

The sub-phase 06.2 PLAN.md / PROGRESS.md / REVIEW.md land in 06.2's own sub-phase execution window (after 06.1 closes); this SPEC is the design contract that the 06.2 state-2 planner consumes. The 06.1 SPEC and 06.3 SPEC are landed alongside this SPEC at parent-06 state-2 but stay unedited during 06.2 execution.
