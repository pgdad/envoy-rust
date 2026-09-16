# Phase 115 — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `envoy.filters.http.health_check` in non-pass-through mode — an HTTP filter that answers a downstream liveness probe at the proxy with an empty 200 — together with its access-log and stats surface, witnessed by new differential fixtures `0095-http-filter-health-check` (the wire) and `0096-http-filter-health-check-stats` (the counters).

**Architecture:** A thirteenth production `HttpFilterInstance` variant, `HealthCheck(HealthCheckFilter)`, decode-side only. It AND-folds the landed seven-mode `HeaderMatcher` over the request — feeding a `:path` matcher a one-entry view built from `FilterRequest::path`, because neither codec puts pseudo-headers into `FilterRequest::headers` — and on a match returns `Decision::StopAndSend` with a 200, an empty body and one `x-envoy-upstream-healthchecked-cluster` header whose value is the bootstrap `node.cluster`, stamped into the config by `validate_hcm`. A new `FilterResponse::details` field carries `health_check_ok` to both codecs' `%RESPONSE_CODE_DETAILS%`, and both codecs skip the `downstream_rq_Nxx` counters for a response carrying that detail.

**Tech Stack:** Rust 2024; `envoy-config` (serde + `serde_yaml` 0.9.34), `envoy-filter`, `envoy-stats`, `envoy-http1`, `envoy-http2`; the EXISTING `Driver::Http1ProbeList` and `Driver::AdminScrape` differential drivers.

**Spec:** `docs/envoy-rust/phases/115-http-filter-health-check/SPEC.md`. **Read it together with this plan — but read "SPEC corrections" below first: this PLAN-write measured seven SPEC claims to be wrong or incomplete, and `ADR-0201` is the forward correction. Where they disagree, this plan wins.**

---

## Global Constraints

- Upstream target is `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`. Every behavioural cell in this plan was MEASURED against it; container ownership was proved on every probe with `docker inspect <cid> --format '{{.Image}}'`, readiness was gated on a real HTTP response (never a TCP connect), and host ports came from a bind-then-release `socket.bind(('127.0.0.1', 0))`.
- `#![forbid(unsafe_code)]` in every crate root (D-3.8). No `unsafe` anywhere in this phase.
- **No new dependency, no new workspace crate, no new harness driver, no new fuzz target.** `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml` and `tests/differential/src/lib.rs` MUST be untouched — verified untouched on the prototype (`git diff --numstat` lists none of them). If a task appears to need one of them, STOP: the scope has drifted.
- Every task ends with `cargo build --workspace --all-targets`, `cargo fmt --all -- --check` and the named tests green. `cargo clippy --workspace --all-targets --all-features -- -D warnings` is green at every boundary **except the Task 3 boundary**, where it is DEFERRED to Task 4 by design (see Task 3). **These boundary claims were MEASURED on a second scratch tree that committed each task's end state separately** — this plan does not assert a per-task gate it only measured once at the end (`ADR-0194` DECISION 2).
- `FilterResponse` deliberately does NOT implement `Default`, so the new field is an `E0063` at every exhaustive literal — **exactly 18**, across three fixpoint rounds (Task 2). Literals using `..FilterResponse::test_200()` absorb it and must NOT be edited.
- **Never reproduce upstream's `path_through_mode` typo** (`SPEC.md` §2.1). Behaviour is the contract; error text is not (§7.4).
- Nothing is fixed (§6.3; `ADR-0165`) **except the single rider Task 1 takes deliberately and labels as such** — `CF-114-6`. No other carry-forward is consumed. Three pre-existing defects this PLAN-write MEASURED along the way are BANKED, not fixed (CF-115-8, CF-115-9 — a panic — and CF-115-10; see the carry-forward section).

---

## SPEC corrections this PLAN-write measured — read before Task 1

`SPEC.md` is landed and is NOT edited; `ADR-0201` carries every correction below forward.

1. **`SPEC.md` §4 item 3 would build a filter that NEVER matches `:path` (PV-1).** `SPEC.md` says the filter ANDs "over the `headers` matchers against the request, `:path` carrying its query string verbatim", which reads as `headers.iter().all(|m| m.matches(&req.headers))` — the exact shape of the landed `fault` gate. **Neither codec puts pseudo-headers into `FilterRequest::headers`**: H1 fills it from the wire header block and H2's `http_to_envoy_request` copies only regular headers. `FilterRequest::path`, however, carries the query string on BOTH codecs (H1: the raw request target; H2: `uri.path_and_query()`). So the query-string half of the SPEC is right and the matching half is unreachable. **Measured by mutation**: substituting `m.matches(&req.headers)` for the `:path` view turns **5 of 10** filter unit tests RED — every intercept cell. The filter therefore evaluates a `:path` matcher against a one-entry `[(":path", req.path)]` view (Task 3).
2. **`SPEC.md` §2.2's "the header is constantly empty here" is true only because its probe config had no `node:` block (PV-4).** MEASURED four cells: `node.cluster: hc-node-cluster` → `x-envoy-upstream-healthchecked-cluster: hc-node-cluster`; the same plus an unrelated static cluster → unchanged; `node.cluster: ""` → empty; no `node` → empty. The value is the LOCAL `node.cluster`, not any upstream cluster, and a request carrying the header with `SENTINEL` still gets `hc-node-cluster` back. Every fixture in this corpus that sets `node:` would therefore have gone RED on a filter that hard-coded the empty string. Task 4 stamps the value in `validate_hcm`.
3. **`SPEC.md` §4 item 4 / §8 PV-3's "28 literal construction sites" is a TEXT count; the compiler's `E0063` blast is 18.** The 28 lines of `FilterResponse {` decompose, re-derived at `74f2e12`, as 1 struct declaration + 8 function signatures (`-> FilterResponse {`) + 2 `impl FilterResponse {` headers + 17 literal lines — of which one (`header_mutation.rs`) is a `..FilterResponse::test_200()` functional update that absorbs the field. That leaves 16, plus **2 `Self { … }` literals inside `impl FilterResponse` that a text grep for `FilterResponse {` cannot see** = **18**. Swept in a fixpoint loop exactly as PV-3 warned: round 0 = 9 (`envoy-filter`), round 1 = 5 (`envoy-http1`), round 2 = 4 (`envoy-http2`), round 3 = 0.
4. **`SPEC.md` has NO stat surface at all, and upstream has one.** MEASURED on a fresh container with an admin listener: the filter REGISTERS eight counters at config load — `http.<stat_prefix>.health_check.{cached_response, degraded, failed, failed_cluster_empty, failed_cluster_not_found, failed_cluster_unhealthy, ok, request_total}` — and each intercept ticks `request_total`, `ok` and `http.<stat_prefix>.tracing.health_check`; a fall-through ticks none of them. **More consequentially, an intercepted request is NOT counted in `downstream_rq_2xx` or `downstream_rq_completed`** (it IS counted in `downstream_rq_total` and `downstream_rq_http1_total`), measured as a delta over exactly one fall-through (`downstream_rq_2xx` +1) against exactly one intercept (+0). envoy-rust already emits `downstream_rq_2xx`, so a filter shipped without the exclusion would CREATE a divergence on a landed, contracted stat. The counters and the exclusion are in scope (Tasks 3 and 5) and witnessed by a SECOND fixture, `0096` (Task 7); `tracing.health_check` is banked (CF-115-7) because envoy-rust has no `tracing.*` family at all.
5. **`SPEC.md` §4 item 5's single-filter fixture cannot hold its own cells.** One `health_check` configured with BOTH `:path exact /healthz` AND `x-probe exact yes` makes `GET /healthz` fall through, inverting cell 1. Fixture `0095` therefore chains TWO health-check filters — A carries the single `:path` matcher, B carries the two-matcher AND — which upstream accepts (verified by the fixture itself going GREEN on both proxies).
6. **`SPEC.md` is silent on pseudo-headers other than `:path`, and upstream matches them.** MEASURED: `:method exact POST` intercepts `POST /x` only; `:authority exact hc.test` and `host exact hc.test` both intercept only the request carrying `Host: hc.test`; `:scheme exact http` intercepts everything on a plaintext listener. envoy-rust cannot see them (correction 1), so a matcher naming any `:`-prefixed name other than `:path` is REJECTED at load (`ConfigError::UnsupportedHealthCheckPseudoHeader`, Task 4) rather than silently never matching. Banked as CF-115-6. `host` is a regular header in both codecs' `FilterRequest::headers` and needs no special case.
7. **`SPEC.md` §2.1's reject table is upstream-only and envoy-rust rejects MORE (PV-7).** Upstream ACCEPTS `pass_through_mode: true` and `pass_through_mode: true` + `cache_time`; envoy-rust rejects both, plus any `cache_time` and any `cluster_min_healthy_percentages`, boot-fatally — a recorded REJECT-direction divergence (`ADR-0049` fail-loud), each cell pinned by its own in-process test (Tasks 3 and 4).

---

## PV discharge

| PV | question | answer (MEASURED) | consequence |
|---|---|---|---|
| PV-1 | does envoy-rust present `:path` to the matcher with the query string? | the path string carries the query on both codecs; **the header list carries no `:path` at all** | correction 1; Task 3's `:path` view; mutation-proved |
| PV-2 | does the AND-fold over an EMPTY slice yield `true`? | yes — `Iterator::all` over an empty iterator is `true`, the same idiom the landed `fault` gate documents (`crates/envoy-filter/src/fault.rs`, `header_gate_matches`) | `empty_matcher_list_matches_everything` pins it |
| PV-3 | the `FilterResponse` blast radius | **18** literals, 3 fixpoint rounds (9/5/4) — not 28 | correction 3; Task 2's sweep |
| PV-4 | `x-envoy-upstream-healthchecked-cluster` with a cluster present | the value is `node.cluster`; an upstream cluster does not change it | correction 2; Task 4's stamping |
| PV-5 | is the admin `/healthcheck/fail` leg expressible? | **no, confirmed at `74f2e12`** — `Driver::AdminScrape`'s `PreRequest` is `{method, path, host, port_key}` with no expected status or body, the only method is `GET`, and the temporal order is fixed as `pre_requests` → `pre_admin_actions` → `scrapes`, so "probe, POST fail, probe again and assert 503" cannot be written | CF-115-2 stands unchanged |
| PV-6 | filter ORDER | `FilterPipeline::decode_headers` iterates `self.filters` in declaration order and returns at the first `StopAndSend` (`crates/envoy-filter/src/pipeline.rs`). Upstream MEASURED the same: `[health_check, fault(abort 100%)]` answers `GET /healthz` 200 and `GET /other` 503; `[fault, health_check]` answers both 503 | no design change; recorded |
| PV-7 | reject cells against envoy-rust | every reject cell has its own in-process test through `parse_bootstrap` (Tasks 3 and 4) | correction 7 |

Two adjacent cells were measured while discharging PV-6 and are BANKED, not built: upstream's `fault` abort logs `%RESPONSE_CODE_DETAILS%` = `fault_filter_abort` and `%RESPONSE_FLAGS%` = `FI` where envoy-rust logs `-` and `-` (CF-115-8 — this phase's `details` seam makes the RCD half a one-line change per filter), and upstream ROUTE `match.headers` entries naming `:path` / `:method` DO match (`GET /x` → the `:path`-matched route, `POST /y` → the `:method`-matched route) while envoy-rust's route walker never matches them (both fall to the catch-all, measured on `envoy-bin`) (CF-115-10).

---

## File Structure

| file | responsibility | change |
|---|---|---|
| `crates/envoy-http1/src/hcm.rs` | Task 1 rider; the `details` hand-off on the decode `StopAndSend` path; the per-class counter exclusion; in-process tests | modify |
| `crates/envoy-http2/src/hcm.rs` | the same hand-off and exclusion for H2; in-process tests | modify |
| `crates/envoy-filter/src/types.rs` | `FilterResponse::details` | modify |
| `crates/envoy-filter/src/{cors,jwt_authn,local_rate_limit,pipeline,router}.rs` | `details: None` at each `E0063` literal | modify |
| `crates/envoy-filter/src/health_check.rs` | `HealthCheckFilter` — matching, the local reply, the eight counters | create |
| `crates/envoy-filter/src/lib.rs` | `pub mod` + `pub use` | modify |
| `crates/envoy-filter/src/instance.rs` | the thirteenth production variant and its three arms | modify |
| `crates/envoy-config/src/bootstrap.rs` | `HealthCheckFilterConfig`; the typed-config variant; the validator; `node.cluster` stamping; tests | modify |
| `crates/envoy-config/src/lib.rs` | re-export + two `ConfigError` variants | modify |
| `tests/fixtures/0095-http-filter-health-check/{envoy,envoy-rust,expectations}.yaml`, `README.md` | the wire fixture | create |
| `tests/differential/tests/http_filter_health_check.rs` | its runner | create |
| `tests/fixtures/0096-http-filter-health-check-stats/{envoy,envoy-rust,expectations}.yaml`, `README.md` | the stats fixture | create |
| `tests/differential/tests/http_filter_health_check_stats.rs` | its runner | create |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | the contract section | modify |

**Task order is dependency-forced.** Task 1 goes first because it moves lines in `crates/envoy-http1/src/hcm.rs` and must not be entangled with a feature edit. Task 2's field must exist before Task 3's filter can set it. Task 3's config type must exist before Task 4 can make it reachable. Task 5's exclusion keys on `envoy_filter::health_check::HEALTH_CHECK_OK`, created in Task 3, and its end-to-end tests need Task 4's stamping. Tasks 6 and 7 need everything before them.

---

## §6.1 SPLIT GATE — MEASURED, DOES NOT FIRE

The gate is ~25 tasks OR ~1500 net LoC. **This plan is 8 tasks and MEASURED 1398 net LoC excluding `docs/`.**

The measurement is not a projection. Every code block in this plan is lifted VERBATIM from a scratch `git worktree` (created with `git worktree add --detach`, never `cp -r`, with its own `CARGO_TARGET_DIR`) in which the whole slice builds, passes `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`, turns BOTH new fixtures GREEN against both real proxies — with all five fixture mutations (V1–V5) verified RED at the predicted probe or stat and the unmutated control verified GREEN after an md5-checked restore — and ran the WHOLE workspace suite, differential harness included: **172 test binaries, 2339 passed, 6 failed** (so `passed + failed` = 2345, exactly the prediction below). **None of the six is this slice's.** Five are differential fixtures carrying the recorded host signature — `access_log_{h2_rcd,h2_uc,rcd,rf}_upstream_reset`, where upstream's container gets `Network is unreachable` over IPv6 to the host backend, and `admin_config_dump_server_info`'s address-bearing `/clusters` body — and all five were re-run on the UNMODIFIED `74f2e12` tree in its own target dir and FAILED there identically. The sixth, `client::tests::send_request_maps_h2_handshake_failure_to_typed_error`, is the recorded `envoy-http2` host flake: it passed 2/2 in isolation on the unmodified tree and passed at every one of the six per-task boundary runs below. CI on native Linux is authoritative for all six. `git diff --numstat` against `74f2e12` on that tree:

| file | insertions | deletions | net |
|---|---:|---:|---:|
| `crates/envoy-config/src/bootstrap.rs` | 282 | 0 | 282 |
| `crates/envoy-config/src/lib.rs` | 39 | 16 | 23 |
| `crates/envoy-filter/src/cors.rs` | 1 | 0 | 1 |
| `crates/envoy-filter/src/health_check.rs` | 274 | 0 | 274 |
| `crates/envoy-filter/src/instance.rs` | 43 | 0 | 43 |
| `crates/envoy-filter/src/jwt_authn.rs` | 2 | 0 | 2 |
| `crates/envoy-filter/src/lib.rs` | 2 | 0 | 2 |
| `crates/envoy-filter/src/local_rate_limit.rs` | 2 | 0 | 2 |
| `crates/envoy-filter/src/pipeline.rs` | 1 | 0 | 1 |
| `crates/envoy-filter/src/router.rs` | 1 | 0 | 1 |
| `crates/envoy-filter/src/types.rs` | 8 | 0 | 8 |
| `crates/envoy-http1/src/hcm.rs` | 182 | 19 | 163 |
| `crates/envoy-http2/src/hcm.rs` | 115 | 20 | 95 |
| `tests/differential/tests/http_filter_health_check.rs` | 29 | 0 | 29 |
| `tests/differential/tests/http_filter_health_check_stats.rs` | 25 | 0 | 25 |
| `tests/fixtures/0095-http-filter-health-check/README.md` | 67 | 0 | 67 |
| `tests/fixtures/0095-http-filter-health-check/envoy-rust.yaml` | 53 | 0 | 53 |
| `tests/fixtures/0095-http-filter-health-check/envoy.yaml` | 53 | 0 | 53 |
| `tests/fixtures/0095-http-filter-health-check/expectations.yaml` | 102 | 0 | 102 |
| `tests/fixtures/0096-http-filter-health-check-stats/README.md` | 45 | 0 | 45 |
| `tests/fixtures/0096-http-filter-health-check-stats/envoy-rust.yaml` | 40 | 0 | 40 |
| `tests/fixtures/0096-http-filter-health-check-stats/envoy.yaml` | 40 | 0 | 40 |
| `tests/fixtures/0096-http-filter-health-check-stats/expectations.yaml` | 47 | 0 | 47 |
| **TOTAL (23 files)** | **1453** | **55** | **1398** |

Per task, measured on the SECOND scratch tree that committed each task's end state separately:

| task | insertions | deletions | net | boundary gates, MEASURED on the per-task tree |
|---|---:|---:|---:|---|
| 1 — rider `CF-114-6` | 3 | 3 | 0 | build, fmt, clippy (160 `Checking`), tests: all green |
| 2 — `FilterResponse::details` | 124 | 24 | 100 | build, fmt, clippy, tests: all green |
| 3 — config type + filter | 376 | 16 | 360 | build, fmt, tests green; **clippy RED by design** (three `dead_code` errors, see Task 3) |
| 4 — reachable: variant, validator, stamping, instance | 264 | 0 | 264 | build, fmt, clippy, tests: all green |
| 5 — counter exclusion + end-to-end pins | 185 | 12 | 173 | build, fmt, clippy, tests: all green |
| 6 — fixture `0095` | 304 | 0 | 304 | build, fmt, clippy green; fixture GREEN cross-proxy |
| 7 — fixture `0096` | 197 | 0 | 197 | build, fmt, clippy green; fixture GREEN cross-proxy |
| 8 — `BEHAVIOR_CONTRACT.md` | — | — | excluded (`docs/`) | not prototyped |
| **TOTAL** | **1453** | **55** | **1398** | re-summed: 0+100+360+264+173+304+197 = **1398** ✓ |

Unit-test totals across `envoy-config` + `envoy-filter` + `envoy-http1` + `envoy-http2` on the per-task tree: **1305** (after Task 1) → **1307** (+2, Task 2) → **1323** (+16, Task 3) → **1331** (+8, Task 4) → **1333** (+2, Task 5), zero failures at every boundary; the two differential runners add the remaining 2 of the phase's 30 new tests.

**The margin is THIN and this plan says so.** 1398 against ~1500 leaves 102 (6.8%) lines. The project's recorded post-measurement drift is 1.00× (`112.1`), 1.07× (`113`), 1.10× (`112.2`) and 1.107× (`114`), and `112.2`'s 1.10× was caused ENTIRELY by tests drafted into the plan AFTER the prototype was priced. That mechanism is closed here mechanically: every fenced code block in this file was extracted by script from the measured tree, and a verification pass confirmed each fence occurs in that tree (41 `rust`/`yaml`/`markdown` fences: 40 occur byte-exact in the measured tree and the 41st — Task 3 Step 1's test module, closed early with its own `}` — occurs as an ordered line subsequence; 0 not found). **If you edit a code block in this plan, re-measure.** An executor who adds lines beyond the plan's blocks should expect to be asked why at state 5.

Calibration for the record — MEASURED-on-prototype estimates: `112.1` 1.00×, `112.2` 1.10×, `113` 1.07×, `114` 1.107×. PROJECTED estimates: `110.2` 1.33×, `110.1` 1.41×, `111` 1.66×, `114` 0.75× (high). `SPEC.md` §7 projected a central ≈1150–1300; the measurement is above that central because §7 did not know about corrections 2 and 4 (the stamping and the whole stat surface, ≈334 lines including fixture `0096`).

**The split was weighed and REJECTED.** The pre-declared cut (`115.1` in-process, `115.2` fixtures + contract + close) is clean and would be cheap to take, and the thin margin is a real argument for it. It is rejected because the gate is ~1500 net LoC and the MEASURED figure is under it, with the one known drift mechanism removed; splitting a measured-under phase on the strength of a calibration factor is exactly the error `ADR-0197` recorded (the phase-114 projection would have split a 938-line phase). **`ADR-0200`'s reservation of `ADR-0201` for a split is RELEASED and the number is consumed by this PLAN-write's ADR**, so `DECISIONS.md` gains no gap. `ADR-0202` is next free.

---

## Task 1: RIDER — re-attach `build_access_log_record`'s doc comment (CF-114-6)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing. **Zero behaviour change.**

**This is a deliberate rider, not a deliverable.** `CF-114-6` (phase-114 `REVIEW.md` F-1, `ADR-0199` DECISION 2): phase 114 spliced `effective_grpc_status` BETWEEN `build_access_log_record`'s doc comment and its `fn` line, so the helper opens with a paragraph describing a different function and the H1 record-build join point has no doc at all. `ADR-0200` (l) names it a legitimate rider because this phase touches the file. It is taken in its own commit, first. **Riders are not phases and are not costed as one** (`ADR-0192` DECISION 5); it is net 0 lines.

- [ ] **Step 1: Assert both anchors are unique**

```bash
grep -cF "/// Build the per-request access-log record (extracted verbatim from" crates/envoy-http1/src/hcm.rs
grep -cF "fn build_access_log_record(" crates/envoy-http1/src/hcm.rs
```
Expected: `1` and `1`.

- [ ] **Step 2: Move the three-line doc block**

Delete these three lines from directly above `/// Phase 114: the UNGATED effective gRPC status.`:

```rust
/// Build the per-request access-log record (extracted verbatim from
/// `serve_connection`'s factored access-log dispatch site, including the
/// `%RESPONSE_FLAGS%` derive block).
```

and insert the same three lines directly above `fn build_access_log_record(`, so that the blank line after `effective_grpc_status`'s closing `}` is followed by the doc block and then the `fn` line.

- [ ] **Step 3: Verify it is a pure move**

Run: `git diff --numstat crates/envoy-http1/src/hcm.rs`
Expected: `3	3	crates/envoy-http1/src/hcm.rs`.

- [ ] **Step 4: Gate and commit**

```bash
cargo build --workspace --all-targets && cargo fmt --all -- --check \
  && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 115 task 1: RIDER — re-attach build_access_log_record's doc comment (CF-114-6)"
```

---

## Task 2: The `FilterResponse::details` seam

**Files:**
- Modify: `crates/envoy-filter/src/types.rs`, and every `E0063` site in `crates/envoy-filter/src/{cors,jwt_authn,local_rate_limit,pipeline,router,types}.rs`
- Modify: `crates/envoy-http1/src/hcm.rs`, `crates/envoy-http2/src/hcm.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `pub details: Option<&'static str>` on `envoy_filter::FilterResponse`. Both HCMs copy it into `%RESPONSE_CODE_DETAILS%` on the decode-side `StopAndSend` path ONLY. The H2 test helper `h2_response_code_details_line(pipeline: Option<Arc<FilterPipeline>>, uri: &str) -> (http::HeaderMap, String, u64)` (headers, the log line, `downstream_rq_2xx`) is reused by Task 5.

**Behaviour-neutral for every landed filter:** all of them set `None`, which renders `-` exactly as today.

- [ ] **Step 1: Write the failing H1 test**

Insert immediately above `async fn h1_stop_and_send_at_encode_substitutes_wire_response()`'s `#[tokio::test]` line in `crates/envoy-http1/src/hcm.rs`:

```rust
    /// Phase 115: a decode-side filter's `FilterResponse::details` is the
    /// access-log record's `%RESPONSE_CODE_DETAILS%`; `None` renders `-`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_decode_stop_and_send_details_reach_the_access_log() {
        async fn line(details: Option<&'static str>) -> String {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("access.log");
            let format = envoy_accesslog::CompiledFormat::from_inline("d=%RESPONSE_CODE_DETAILS%")
                .expect("format parses");
            let sink = envoy_accesslog::FileSink::new(path.clone(), format, None)
                .await
                .expect("open sink");
            let stop_resp = envoy_filter::FilterResponse {
                status: 200,
                reason: None,
                headers: vec![],
                body: Bytes::new(),
                details,
            };
            let pipeline = Arc::new(envoy_filter::FilterPipeline::test_from_instances(vec![
                envoy_filter::HttpFilterInstance::test_stop_and_send_on_decode(stop_resp),
                envoy_filter::HttpFilterInstance::test_router(),
            ]));
            let config = hcm_config_with_pipeline(pipeline, "/", 200, "route\n").await;
            let mut config = Arc::into_inner(config).expect("sole owner");
            config.access_log = vec![Arc::new(sink)];
            let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
            let _ = drive(Arc::new(config), req).await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            tokio::fs::read_to_string(&path).await.unwrap_or_default()
        }
        assert_eq!(line(Some("seam_probe")).await.trim(), "d=seam_probe");
        assert_eq!(line(None).await.trim(), "d=-");
    }
```

- [ ] **Step 2: Write the failing H2 test and generalise the H2 helper**

In `crates/envoy-http2/src/hcm.rs`, REPLACE the whole existing `h2_response_code_details_line` function (doc comment included) with:

```rust
    /// phase 42 (ADR-0099): the H2 HCM sets `response_code_details` on the
    /// access-log record per response-path, threaded from `handle_one_stream`
    /// into `finalize_h2_stream`. A `direct_response` route over H2 →
    /// `%RESPONSE_CODE_DETAILS%` renders `direct_response`.
    async fn h2_response_code_details_line(
        pipeline: Option<Arc<envoy_filter::FilterPipeline>>,
        uri: &str,
    ) -> (http::HeaderMap, String, u64) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(
                path.clone(),
                envoy_accesslog::CompiledFormat::from_inline("d=%RESPONSE_CODE_DETAILS%")
                    .expect("format parses"),
                None,
            )
            .await
            .expect("open sink"),
        );
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "ingress_http_h2".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            access_log: vec![],
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: "dr".to_string(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            }),
            rds: None,
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mut built = Http1HCMConfig::from_config(
            &cfg,
            cluster_mgr,
            registry,
            None,
            Arc::new(RuntimeSnapshot::default()),
        )
        .await
        .expect("build HCM config");
        built.access_log = vec![sink];
        if let Some(p) = pipeline {
            built.filter_pipeline = p;
        }
        let config = Arc::new(built);
        let stats = Arc::clone(&config.stats);

        let (addr, _server) = spawn_h2_hcm(config).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri(uri)
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let (parts, mut body) = response_fut.await.expect("response").into_parts();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            let _ = body.flow_control().release_capacity(chunk.len());
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let line = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        (parts.headers, line, stats.downstream_rq_2xx.value())
    }
```

replace its one existing caller with:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn hcm_h2_sets_response_code_details_from_response_path() {
        let (_, line, _) = h2_response_code_details_line(None, "http://test.example/").await;
        assert!(
            line.contains("d=direct_response"),
            "direct_response route → %RESPONSE_CODE_DETAILS% renders `direct_response`; got: {}",
            line.trim()
        );
    }
```

and add directly below that caller:

```rust
    /// Phase 115: a decode-side filter's `FilterResponse::details` is the H2
    /// access-log record's `%RESPONSE_CODE_DETAILS%`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_decode_stop_and_send_details_reach_the_access_log() {
        let stop_resp = envoy_filter::FilterResponse {
            status: 200,
            reason: None,
            headers: vec![],
            body: bytes::Bytes::new(),
            details: Some("seam_probe"),
        };
        let pipeline = Arc::new(envoy_filter::FilterPipeline::test_from_instances(vec![
            envoy_filter::HttpFilterInstance::test_stop_and_send_on_decode(stop_resp),
            envoy_filter::HttpFilterInstance::test_router(),
        ]));
        let (_, line, _) =
            h2_response_code_details_line(Some(pipeline), "http://test.example/").await;
        assert_eq!(line.trim(), "d=seam_probe");
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p envoy-http1 -p envoy-http2 details_reach_the_access_log`
Expected: compile FAILURE — MEASURED for the H1 test alone: exactly one error, `` error[E0560]: struct `FilterResponse` has no field named `details` ``.

- [ ] **Step 4: Add the field**

In `crates/envoy-filter/src/types.rs`:

```rust
pub struct FilterResponse {
    pub status: u16,
    pub reason: Option<&'static str>,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
    /// Phase 115: the `%RESPONSE_CODE_DETAILS%` value of a filter's local
    /// reply. Read ONLY on the decode-side `StopAndSend` path, where both
    /// HCMs copy it into the access-log record; `None` renders `-`. Every
    /// filter that predates phase 115 sets `None` (unchanged behaviour); the
    /// encode side and the Continue write-back ignore it.
    pub details: Option<&'static str>,
}
```

- [ ] **Step 5: Sweep `E0063` to a FIXPOINT**

Cargo stops at the first crate that fails, so one build shows only the first crate's sites. Loop until the build is clean:

```bash
cargo build --workspace --all-targets --message-format=short 2>&1 \
  | grep -E "error\[E0063\]: missing field \`details\`" | sort -u
```

For each reported `file:line:col`, add `details: None,` as the LAST field of that struct literal. Work from the compiler's list, never a text match — a grep for `FilterResponse {` also hits function signatures, `impl` headers and a functional-update literal. Expected rounds, measured: **round 0 = 9** (`types.rs` ×2 — the two `Self { … }` literals in `static_reply` and `test_200` — `cors.rs`, `jwt_authn.rs` ×2, `local_rate_limit.rs` ×2, `pipeline.rs`, `router.rs`), **round 1 = 5** (`envoy-http1/src/hcm.rs`: the encode-side literal in `serve_connection` and four test literals), **round 2 = 4** (`envoy-http2/src/hcm.rs`: the encode-side literal in `finalize_h2_stream` and three test literals), **round 3 = 0**. Total **18**. Do NOT edit `header_mutation.rs`'s `..FilterResponse::test_200()` literal.

- [ ] **Step 6: Carry `details` through H1's decode short-circuit**

In `crates/envoy-http1/src/hcm.rs`, the `RequestPath` enum becomes:

```rust
enum RequestPath {
    Match(BuildOutcome),
    /// A decode-side filter's local reply, plus its `%RESPONSE_CODE_DETAILS%`
    /// (phase 115: `FilterResponse::details`).
    SynthFromDecode(Response, Option<&'static str>),
}
```

the `Decision::StopAndSend` arm that builds it:

```rust
            envoy_filter::Decision::StopAndSend(filter_resp) => {
                // Convert FilterResponse → codec-native Response.
                RequestPath::SynthFromDecode(
                    Response {
                        status: filter_resp.status,
                        reason: filter_resp.reason,
                        headers: filter_resp.headers,
                        body: filter_resp.body,
                    },
                    filter_resp.details,
                )
            }
```

the `SynthFromDecode` arm assigns the detail (keep the arm's existing comment block between these lines untouched):

```rust
            RequestPath::SynthFromDecode(resp, details) => {
                // 07.1 Task 6: decode-side filter short-circuit. Unreachable
                // under the Router-only 07.1 chain; lit by 07.2's HeaderMutation
                // (which never short-circuits via StopAndSend on production
                // paths) and by phase 09's LocalRateLimit filter (the first
                // production filter to emit StopAndSend with a sparse header
                // list). `upstream_host_for_log` stays None (no proxy attempt).
                outgoing = resp;
                // Phase 115: the filter's own details (e.g. `health_check_ok`).
                response_code_details_for_log = details.map(str::to_owned);
```

and the local's declaration loses its now-dead `= None` initialiser — every arm assigns it, and leaving the initialiser makes `rustc` warn `value assigned to response_code_details_for_log is never read`:

```rust
        // phase 42 (ADR-0099): per-request %RESPONSE_CODE_DETAILS%. Set by the
        // synth writer-arm (carries the BuildOutcome detail) and the
        // proxy-success arm (`via_upstream`); a decode-side filter synth copies
        // `FilterResponse::details` (phase 115).
        let mut response_code_details_for_log: Option<String>;
```

- [ ] **Step 7: The same for H2**

In `crates/envoy-http2/src/hcm.rs`:

```rust
enum H2RequestPath {
    Match(BuildOutcome),
    /// A decode-side filter's local reply, plus its `%RESPONSE_CODE_DETAILS%`
    /// (phase 115: `FilterResponse::details`).
    SynthFromDecode(Response, Option<&'static str>),
}
```

```rust
        envoy_filter::Decision::StopAndSend(filter_resp) => H2RequestPath::SynthFromDecode(
            Response {
                status: filter_resp.status,
                reason: filter_resp.reason,
                headers: filter_resp.headers,
                body: filter_resp.body,
            },
            filter_resp.details,
        ),
```

```rust
        H2RequestPath::SynthFromDecode(mut r, details) => {
            // 07.1 Task 7: decode-side filter short-circuit. Phase 11 D6:
            // decorate the filter-synth response with the standard H2 response
            // headers (closes 09 REVIEW M2 implementation arm).
            // `upstream_host_for_log_h2` stays None (no proxy attempt).
            crate::response::decorate_filter_synth_response_h2(&mut r);
            // Phase 115: the filter's own details (e.g. `health_check_ok`).
            response_code_details_for_log_h2 = details.map(str::to_owned);
            (r, None)
        }
```

```rust
    // phase 42 (ADR-0099): per-stream %RESPONSE_CODE_DETAILS%. Set by the synth
    // match arm (carries the BuildOutcome detail) and the proxy-success arm
    // (`via_upstream`); a decode-side filter synth copies
    // `FilterResponse::details` (phase 115).
    let mut response_code_details_for_log_h2: Option<String>;
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test -p envoy-http1 -p envoy-http2 details_reach_the_access_log`
Expected: `h1_decode_stop_and_send_details_reach_the_access_log ... ok` and `h2_decode_stop_and_send_details_reach_the_access_log ... ok`; `hcm_h2_sets_response_code_details_from_response_path` still passes.

- [ ] **Step 9: Mutation — prove the H1 test is not vacuous**

⚠ The text `response_code_details_for_log = details.map(str::to_owned);` occurs TWICE in `crates/envoy-http1/src/hcm.rs` (the `BuildOutcome::Synth` arm has the same line), so a plain `sed` mutates both. Target only the new one by its preceding comment:

```bash
cp crates/envoy-http1/src/hcm.rs /tmp/hcm1.bak
python3 - <<'EOF'
p = 'crates/envoy-http1/src/hcm.rs'
s = open(p).read()
a = ("// Phase 115: the filter's own details (e.g. `health_check_ok`).\n"
     "                response_code_details_for_log = details.map(str::to_owned);")
assert s.count(a) == 1
open(p, 'w').write(s.replace(a, a.replace("details.map(str::to_owned)", "details.and(None)")))
EOF
cargo test -p envoy-http1 h1_decode_stop_and_send_details_reach_the_access_log
cp /tmp/hcm1.bak crates/envoy-http1/src/hcm.rs && md5sum /tmp/hcm1.bak crates/envoy-http1/src/hcm.rs
```
Expected: a `Compiling envoy-http1` line, then FAILED with `left: "d=-"` / `right: "d=seam_probe"`; after the restore the two md5s are identical.

- [ ] **Step 10: Gate and commit**

```bash
cargo build --workspace --all-targets && cargo fmt --all -- --check \
  && cargo clippy --workspace --all-targets --all-features -- -D warnings \
  && cargo test -p envoy-filter -p envoy-http1 -p envoy-http2
git add crates/envoy-filter crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 115 task 2: FilterResponse::details — a filter's local reply carries %RESPONSE_CODE_DETAILS%"
```

---

## Task 3: `HealthCheckFilterConfig` and `HealthCheckFilter`

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs`, `crates/envoy-config/src/lib.rs`
- Create: `crates/envoy-filter/src/health_check.rs`
- Modify: `crates/envoy-filter/src/lib.rs`

**Interfaces:**
- Consumes: Task 2's `FilterResponse::details`; the landed `HeaderMatcher::{matches, compile_safe_regexes}` and `crate::error::register_counter`.
- Produces: `envoy_config::HealthCheckFilterConfig { pass_through_mode: bool, headers: Vec<HeaderMatcher>, cache_time: Option<serde_yaml::Value>, cluster_min_healthy_percentages: Option<serde_yaml::Value>, local_cluster: String }` (`local_cluster` is `#[serde(skip)]`); `envoy_filter::HealthCheckFilter::build_from_config(&HealthCheckFilterConfig, &Arc<StatsRegistry>, &str) -> Result<Self, FilterError>`; `pub const X_ENVOY_UPSTREAM_HEALTHCHECKED_CLUSTER` and `pub const HEALTH_CHECK_OK` in `envoy_filter::health_check`.

⚠ **The clippy leg is DEFERRED to Task 4 at this boundary, by design.** The config type is not yet reachable from a bootstrap (Task 4 adds the typed-config variant), so the filter has no production consumer and `-D warnings` reports its crate-private items as dead. MEASURED on the per-task tree: `cargo clippy` exits 101 with exactly THREE errors, all in `crates/envoy-filter/src/health_check.rs` — `constant PATH_PSEUDO_HEADER is never used`, `fields headers, local_cluster, request_total, and ok are never read`, and `methods decode_headers, encode_headers, and matches are never used` — and nothing else in the workspace. Build, fmt and this task's tests MUST be green. **Add no `#[allow]` and no `_` prefix** — that is the `ADR-0194` DECISION 1 remedy, chosen so a forgotten attribute cannot outlive the gap; Task 4 closes it and must show clippy exit 0. The alternative orderings were rejected because each lands an intermediate commit that PARSES a health-check config and silently ignores part of it — the window `ADR-0176` DECISION 2 forbids.

- [ ] **Step 1: Write the failing config schema tests**

Add this module to `crates/envoy-config/src/bootstrap.rs` directly above the `// phase 31 Task 2: cdn_loop filter config schema + cdn_id validation tests` banner (its three-line `// ----` banner block included):

```rust
// ---------------------------------------------------------------------------
// phase 115: health_check filter config schema tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod health_check_config_tests {
    use crate::HealthCheckFilterConfig;

    fn parse(yaml: &str) -> Result<HealthCheckFilterConfig, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    #[test]
    fn parses_pass_through_mode_and_headers() {
        let cfg = parse(
            "pass_through_mode: false\nheaders:\n  - name: \":path\"\n    string_match: { exact: /healthz }\n",
        )
        .expect("parses");
        assert!(!cfg.pass_through_mode);
        assert_eq!(cfg.headers.len(), 1);
        assert_eq!(cfg.headers[0].name, ":path");
        assert_eq!(cfg.local_cluster, "", "not a wire field; stamped later");
    }

    #[test]
    fn absent_headers_default_to_empty() {
        let cfg = parse("pass_through_mode: false\n").expect("parses");
        assert!(cfg.headers.is_empty());
    }

    #[test]
    fn pass_through_mode_is_required() {
        let err = parse("headers: []\n").expect_err("absent pass_through_mode");
        assert!(err.to_string().contains("pass_through_mode"), "{err}");
    }

    #[test]
    fn recognizes_the_two_rejected_fields() {
        let cfg = parse(
            "pass_through_mode: true\ncache_time: 5s\ncluster_min_healthy_percentages: { c: { value: 50 } }\n",
        )
        .expect("recognized, rejected later by the validator");
        assert!(cfg.cache_time.is_some());
        assert!(cfg.cluster_min_healthy_percentages.is_some());
    }

    #[test]
    fn rejects_unknown_field() {
        assert!(parse("pass_through_mode: false\nbogus: 1\n").is_err());
    }

    #[test]
    fn local_cluster_is_not_a_wire_field() {
        assert!(parse("pass_through_mode: false\nlocal_cluster: x\n").is_err());
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p envoy-config health_check_config_tests`
Expected: compile FAILURE — MEASURED: exactly one error, `` error[E0432]: unresolved import `crate::HealthCheckFilterConfig` ``.

- [ ] **Step 3: Add the config type**

In `crates/envoy-config/src/bootstrap.rs`, directly above `` /// `envoy.extensions.filters.http.set_metadata.v3.Config` (phase 33, ``:

```rust
/// `envoy.extensions.filters.http.health_check.v3.HealthCheck` (phase 115).
/// Only NON-pass-through mode is implemented: a request matching every
/// `headers` entry is answered at the proxy with an empty 200.
///
/// `pass_through_mode` is REQUIRED (absent is a serde missing-field error,
/// matching upstream's `HealthCheckValidationError.PassThroughMode`).
/// `cache_time` and `cluster_min_healthy_percentages` are RECOGNIZED so that
/// `validate_health_check_config` can reject them by name instead of serde
/// failing with an opaque unknown-field error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthCheckFilterConfig {
    pub pass_through_mode: bool,
    /// AND-combined; an EMPTY list matches every request (MEASURED).
    #[serde(default)]
    pub headers: Vec<HeaderMatcher>,
    #[serde(default)]
    pub cache_time: Option<serde_yaml::Value>,
    #[serde(default)]
    pub cluster_min_healthy_percentages: Option<serde_yaml::Value>,
    /// NOT a wire field. The bootstrap `node.cluster` (empty without a
    /// `node`), stamped by `validate_hcm`; the filter renders it as the
    /// `x-envoy-upstream-healthchecked-cluster` response header value.
    #[serde(skip)]
    pub local_cluster: String,
}
```

and add `HealthCheckFilterConfig,` to the `pub use bootstrap::{…}` list in `crates/envoy-config/src/lib.rs` directly after `HealthCheck,`, then run `cargo fmt --all` (it re-flows the whole list; that is expected).

- [ ] **Step 4: Run to verify the schema tests pass**

Run: `cargo test -p envoy-config health_check_config_tests`
Expected: `test result: ok. 6 passed`.

- [ ] **Step 5: Write the filter's failing tests**

Create `crates/envoy-filter/src/health_check.rs` containing ONLY this test module for now, and add `pub mod health_check;` after `pub mod header_to_metadata;` in `crates/envoy-filter/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::header_matcher_exact;
    use envoy_config::{HeaderMatcher, HealthCheckFilterConfig};

    fn cfg(headers: Vec<HeaderMatcher>) -> HealthCheckFilterConfig {
        HealthCheckFilterConfig {
            pass_through_mode: false,
            headers,
            cache_time: None,
            cluster_min_healthy_percentages: None,
            local_cluster: "hc-node-cluster".to_string(),
        }
    }

    fn filter(headers: Vec<HeaderMatcher>) -> HealthCheckFilter {
        let registry = Arc::new(StatsRegistry::new());
        HealthCheckFilter::build_from_config(&cfg(headers), &registry, "ingress_http")
            .expect("builds")
    }

    fn healthz() -> Vec<HeaderMatcher> {
        vec![header_matcher_exact(":path", "/healthz")]
    }

    /// `Some(response)` when intercepted, `None` when the request continues.
    fn run(
        f: &mut HealthCheckFilter,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
    ) -> Option<FilterResponse> {
        match f.decode_headers(&mut FilterRequest::test(method, path, headers)) {
            Decision::StopAndSend(resp) => Some(resp),
            Decision::Continue => None,
        }
    }

    #[test]
    fn matched_probe_is_answered_locally() {
        let resp = run(&mut filter(healthz()), "GET", "/healthz", &[]).expect("intercepted");
        assert_eq!(resp.status, 200);
        assert!(resp.body.is_empty());
        assert_eq!(
            resp.headers,
            vec![(
                "x-envoy-upstream-healthchecked-cluster".to_string(),
                "hc-node-cluster".to_string()
            )]
        );
        assert_eq!(resp.details, Some("health_check_ok"));
    }

    #[test]
    fn path_matcher_sees_the_query_string() {
        assert!(run(&mut filter(healthz()), "GET", "/healthz?x=1", &[]).is_none());
    }

    #[test]
    fn exact_path_is_exact_and_case_sensitive() {
        let mut f = filter(healthz());
        assert!(run(&mut f, "GET", "/healthz/", &[]).is_none());
        assert!(run(&mut f, "GET", "/healthZ", &[]).is_none());
        assert!(run(&mut f, "GET", "/other", &[]).is_none());
    }

    #[test]
    fn filter_is_method_agnostic() {
        assert!(run(&mut filter(healthz()), "POST", "/healthz", &[]).is_some());
    }

    #[test]
    fn empty_matcher_list_matches_everything() {
        assert!(run(&mut filter(vec![]), "GET", "/anything", &[]).is_some());
    }

    #[test]
    fn matchers_fold_as_and() {
        let mut both = healthz();
        both.push(header_matcher_exact("x-probe", "yes"));
        let mut f = filter(both);
        assert!(run(&mut f, "GET", "/healthz", &[]).is_none());
        assert!(run(&mut f, "GET", "/healthz", &[("x-probe", "yes")]).is_some());
        assert!(run(&mut f, "GET", "/other", &[("x-probe", "yes")]).is_none());
    }

    #[test]
    fn header_value_is_not_an_echo_of_the_request() {
        let resp = run(
            &mut filter(healthz()),
            "GET",
            "/healthz",
            &[("x-envoy-upstream-healthchecked-cluster", "SENTINEL")],
        )
        .expect("intercepted");
        assert_eq!(resp.headers[0].1, "hc-node-cluster");
    }

    #[test]
    fn bad_regex_is_build_fatal() {
        let bad = HeaderMatcher {
            name: "x-a".to_string(),
            mode: envoy_config::HeaderMatcherMode::SafeRegexMatch(envoy_config::SafeRegex {
                regex: "(".to_string(),
                compiled: None,
            }),
            invert_match: false,
        };
        let registry = Arc::new(StatsRegistry::new());
        assert!(HealthCheckFilter::build_from_config(&cfg(vec![bad]), &registry, "p").is_err());
    }

    #[test]
    fn registers_eight_counters_and_ticks_two_per_intercept() {
        let registry = Arc::new(StatsRegistry::new());
        let mut f =
            HealthCheckFilter::build_from_config(&cfg(healthz()), &registry, "ingress_http")
                .expect("builds");
        let names: Vec<String> = registry.snapshot().into_iter().map(|(n, _)| n).collect();
        for name in STAT_NAMES {
            let full = format!("http.ingress_http.health_check.{name}");
            assert!(names.contains(&full), "{full} registered at build time");
        }
        let value = |name: &str| {
            registry
                .register_counter(&format!("http.ingress_http.health_check.{name}"))
                .expect("same-kind re-register returns the handle")
                .value()
        };
        run(&mut f, "GET", "/other", &[]);
        assert_eq!(value("request_total"), 0, "a fall-through is not counted");
        run(&mut f, "GET", "/healthz", &[]);
        run(&mut f, "POST", "/healthz", &[]);
        assert_eq!((value("request_total"), value("ok")), (2, 2));
        assert_eq!(value("failed"), 0);
    }

    #[test]
    fn encode_headers_is_noop() {
        let mut resp = FilterResponse::test_200();
        assert!(matches!(
            filter(healthz()).encode_headers(&mut resp),
            Decision::Continue
        ));
    }
}
```

Run: `cargo test -p envoy-filter health_check::`
Expected: compile FAILURE — MEASURED at 18 errors, among them `` error[E0425]: cannot find type `HealthCheckFilter` in this scope `` and `` cannot find value `STAT_NAMES` in this scope ``; the rest are names (`Arc`, `Decision`, `FilterRequest`, `FilterResponse`) the implementation's own `use` lines bring into scope.

- [ ] **Step 6: Write the implementation above the test module**

Insert at the TOP of `crates/envoy-filter/src/health_check.rs`, above `#[cfg(test)]`:

```rust
//! The `envoy.filters.http.health_check` runtime filter — non-pass-through
//! mode (phase 115).
//!
//! Decode-side only. A request matching EVERY configured `HeaderMatcher` is
//! answered at the proxy — 200, empty body, one
//! `x-envoy-upstream-healthchecked-cluster` header — and never reaches the
//! route. Anything else continues down the chain. All of it MEASURED against
//! `envoyproxy/envoy:v1.33.0` (phase-115 `SPEC.md` §2 and `ADR-0201`):
//!
//! * the matcher list is AND-combined, and an EMPTY list matches everything;
//! * `:path` is matched WITH its query string, so `/healthz?x=1` does not
//!   match `exact: /healthz`;
//! * the filter is method-agnostic;
//! * the header value is the bootstrap `node.cluster`, never a request echo;
//! * the `http.<stat_prefix>.health_check.*` counters are all registered at
//!   build time, and an intercept ticks `request_total` and `ok`.

use std::sync::Arc;

use bytes::Bytes;
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

/// The response header every intercepted probe carries.
pub const X_ENVOY_UPSTREAM_HEALTHCHECKED_CLUSTER: &str = "x-envoy-upstream-healthchecked-cluster";

/// `%RESPONSE_CODE_DETAILS%` of an intercepted probe.
pub const HEALTH_CHECK_OK: &str = "health_check_ok";

/// The eight counters upstream registers under `http.<stat_prefix>.health_check.`
/// (MEASURED). Only `request_total` and `ok` move in non-pass-through mode;
/// the rest stay 0 (their triggers are CF-115-1 / CF-115-2 / CF-115-5).
const STAT_NAMES: [&str; 8] = [
    "cached_response",
    "degraded",
    "failed",
    "failed_cluster_empty",
    "failed_cluster_not_found",
    "failed_cluster_unhealthy",
    "ok",
    "request_total",
];

/// The one pseudo-header the filter can match. Neither codec puts
/// pseudo-headers into `FilterRequest::headers`, so a `:path` matcher is
/// evaluated against a one-entry view holding `FilterRequest::path` verbatim.
/// Every other `:`-prefixed name is rejected at config load (CF-115-6).
const PATH_PSEUDO_HEADER: &str = ":path";

/// The `envoy.filters.http.health_check` runtime filter.
#[derive(Debug, Clone)]
pub struct HealthCheckFilter {
    headers: Vec<envoy_config::HeaderMatcher>,
    local_cluster: String,
    request_total: Arc<Counter>,
    ok: Arc<Counter>,
}

impl HealthCheckFilter {
    /// Lower a config, compiling any `SafeRegex` on an owned clone so a bad
    /// pattern is build-fatal rather than a first-request panic, and register
    /// the eight `http.{hcm_stat_prefix}.health_check.*` counters.
    pub fn build_from_config(
        cfg: &envoy_config::HealthCheckFilterConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        let mut headers = cfg.headers.clone();
        for m in &mut headers {
            m.compile_safe_regexes()
                .map_err(|e| FilterError::InvalidConfig {
                    message: e.to_string(),
                })?;
        }
        let mut counters = Vec::with_capacity(STAT_NAMES.len());
        for name in STAT_NAMES {
            counters.push(crate::error::register_counter(
                registry,
                &format!("http.{hcm_stat_prefix}.health_check.{name}"),
            )?);
        }
        Ok(Self {
            headers,
            local_cluster: cfg.local_cluster.clone(),
            request_total: Arc::clone(&counters[7]),
            ok: Arc::clone(&counters[6]),
        })
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        if !self.matches(req) {
            return Decision::Continue;
        }
        self.request_total.inc();
        self.ok.inc();
        Decision::StopAndSend(FilterResponse {
            status: 200,
            reason: None,
            headers: vec![(
                X_ENVOY_UPSTREAM_HEALTHCHECKED_CLUSTER.to_string(),
                self.local_cluster.clone(),
            )],
            body: Bytes::new(),
            details: Some(HEALTH_CHECK_OK),
        })
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }

    /// AND over every matcher (`Iterator::all`, so an empty list is `true`).
    fn matches(&self, req: &FilterRequest) -> bool {
        let path_view = [(PATH_PSEUDO_HEADER.to_string(), req.path.clone())];
        self.headers.iter().all(|m| {
            if m.name.eq_ignore_ascii_case(PATH_PSEUDO_HEADER) {
                m.matches(&path_view)
            } else {
                m.matches(&req.headers)
            }
        })
    }
}

```

and add `pub use health_check::HealthCheckFilter;` after `pub use header_to_metadata::HeaderToMetadataFilter;` in `crates/envoy-filter/src/lib.rs`.

Run: `cargo test -p envoy-filter health_check::`
Expected: `test result: ok. 10 passed`.

- [ ] **Step 7: Mutation 1 — the naive reuse SPEC §4 item 3 implies (correction 1)**

```bash
F=crates/envoy-filter/src/health_check.rs; cp $F /tmp/hc.bak
grep -cF 'm.matches(&path_view)' $F          # must print 1
sed -i 's/m\.matches(&path_view)/m.matches(\&req.headers)/' $F
cargo test -p envoy-filter health_check::
cp /tmp/hc.bak $F && md5sum /tmp/hc.bak $F
```
Expected: a `Compiling envoy-filter` line, then **5 FAILED** — `matched_probe_is_answered_locally`, `filter_is_method_agnostic`, `header_value_is_not_an_echo_of_the_request`, `matchers_fold_as_and`, `registers_eight_counters_and_ticks_two_per_intercept` — and identical md5s after the restore.

- [ ] **Step 8: Mutation 2 — stripping the query string (the SPEC §2.3 cell-2 trap)**

```bash
F=crates/envoy-filter/src/health_check.rs; cp $F /tmp/hc.bak
grep -cF 'req.path.clone()' $F                # must print 1
sed -i "s/req\.path\.clone()/req.path.split('?').next().unwrap_or_default().to_string()/" $F
cargo test -p envoy-filter health_check::
cp /tmp/hc.bak $F && md5sum /tmp/hc.bak $F
```
Expected: exactly **1 FAILED**, `path_matcher_sees_the_query_string`; identical md5s after the restore.

- [ ] **Step 9: Gate (clippy deferred) and commit**

```bash
cargo build --workspace --all-targets && cargo fmt --all -- --check \
  && cargo test -p envoy-config health_check_config_tests && cargo test -p envoy-filter health_check::
git add crates/envoy-config/src crates/envoy-filter/src/health_check.rs crates/envoy-filter/src/lib.rs
git commit -m "phase 115 task 3: HealthCheckFilterConfig + HealthCheckFilter (matching, local reply, counters)"
```

---

## Task 4: Make the filter reachable — typed-config variant, validator, `node.cluster` stamping, instance wiring

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs`, `crates/envoy-config/src/lib.rs`
- Modify: `crates/envoy-filter/src/instance.rs`

**Interfaces:**
- Consumes: Task 3's `HealthCheckFilterConfig` and `HealthCheckFilter::build_from_config`.
- Produces: `HttpFilterTypedConfig::HealthCheck(HealthCheckFilterConfig)` (`@type` `type.googleapis.com/envoy.extensions.filters.http.health_check.v3.HealthCheck`, name `envoy.filters.http.health_check`); `ConfigError::UnsupportedHealthCheckField { listener: String, field: &'static str }` and `ConfigError::UnsupportedHealthCheckPseudoHeader { listener: String, name: String }`; `validate_hcm` gains a `local_cluster: &str` parameter; `HttpFilterInstance::HealthCheck(HealthCheckFilter)`.

- [ ] **Step 1: Write the failing validation and stamping tests**

Append to `mod health_check_config_tests` (after `local_cluster_is_not_a_wire_field`, before the module's closing `}`):

```rust
    /// A full bootstrap whose HCM chain is `[health_check, router]`; `body` is
    /// the health_check typed_config's fields, one per line, unindented.
    fn bootstrap_with(body: &str) -> String {
        let body: String = body
            .lines()
            .map(|l| format!("                      {l}\n"))
            .collect();
        format!(
            r#"
node: {{ id: n, cluster: hc-node-cluster }}
static_resources:
  listeners:
    - name: ingress_http
      address: {{ socket_address: {{ address: 0.0.0.0, port_value: 8080 }} }}
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
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: MAIN }} }}
                http_filters:
                  - name: envoy.filters.http.health_check
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.health_check.v3.HealthCheck
{body}                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#
        )
    }

    fn load(body: &str) -> Result<crate::Bootstrap, crate::ConfigError> {
        crate::parse_bootstrap(&bootstrap_with(body))
    }

    const PATH_MATCHER: &str =
        "headers:\n  - { name: \":path\", string_match: { exact: /healthz } }";

    #[test]
    fn accepts_non_pass_through_mode_with_a_path_matcher() {
        load(&format!("pass_through_mode: false\n{PATH_MATCHER}")).expect("loads");
    }

    #[test]
    fn accepts_absent_and_empty_headers() {
        load("pass_through_mode: false").expect("absent headers");
        load("pass_through_mode: false\nheaders: []").expect("empty headers");
    }

    #[test]
    fn rejects_the_three_unimplemented_fields() {
        for (body, field) in [
            ("pass_through_mode: true", "pass_through_mode: true"),
            ("pass_through_mode: false\ncache_time: 5s", "cache_time"),
            (
                "pass_through_mode: false\ncluster_min_healthy_percentages: { c: { value: 50 } }",
                "cluster_min_healthy_percentages",
            ),
        ] {
            match load(body) {
                Err(crate::ConfigError::UnsupportedHealthCheckField { field: f, .. }) => {
                    assert_eq!(f, field)
                }
                other => panic!("{body}: expected UnsupportedHealthCheckField, got {other:?}"),
            }
        }
    }

    #[test]
    fn rejects_pseudo_headers_other_than_path() {
        for name in [":method", ":authority", ":scheme"] {
            let body = format!(
                "pass_through_mode: false\nheaders:\n  - {{ name: \"{name}\", string_match: {{ exact: x }} }}"
            );
            assert!(
                matches!(
                    load(&body),
                    Err(crate::ConfigError::UnsupportedHealthCheckPseudoHeader { .. })
                ),
                "{name} must be rejected"
            );
        }
    }

    /// The stamped `local_cluster` of the (single) listener's health_check.
    fn stamped(b: &crate::Bootstrap) -> String {
        let chain = &b.static_resources.listeners[0].filter_chains[0];
        let Some(crate::bootstrap::TypedConfig::HttpConnectionManager(hcm)) =
            &chain.filters[0].typed_config
        else {
            panic!("HCM expected");
        };
        match &hcm.http_filters[0].typed_config {
            crate::HttpFilterTypedConfig::HealthCheck(c) => c.local_cluster.clone(),
            other => panic!("health_check expected, got {other:?}"),
        }
    }

    #[test]
    fn validation_stamps_node_cluster_into_the_filter() {
        let b = load("pass_through_mode: false").expect("loads");
        assert_eq!(stamped(&b), "hc-node-cluster");
    }

    #[test]
    fn absent_node_stamps_the_empty_string() {
        let yaml = bootstrap_with("pass_through_mode: false")
            .replace("node: { id: n, cluster: hc-node-cluster }\n", "");
        let b = crate::parse_bootstrap(&yaml).expect("loads without node");
        assert_eq!(stamped(&b), "");
    }

    #[test]
    fn runs_the_shared_header_matcher_validation() {
        let empty_name = "pass_through_mode: false\nheaders:\n  - { name: \"\", exact_match: x }";
        assert!(matches!(
            load(empty_name),
            Err(crate::ConfigError::EmptyHeaderName)
        ));
        let bad_regex = "pass_through_mode: false\nheaders:\n  - { name: x-a, safe_regex_match: { regex: \"(\" } }";
        assert!(matches!(
            load(bad_regex),
            Err(crate::ConfigError::InvalidRegex { .. })
        ));
    }
```

and add to `crates/envoy-filter/src/instance.rs`'s test module, directly above `fn builds_header_to_metadata_instance_and_writes`'s `#[test]`:

```rust
    #[test]
    fn builds_health_check_instance_and_intercepts() {
        let hf = envoy_config::HttpFilter {
            name: "envoy.filters.http.health_check".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::HealthCheck(
                envoy_config::HealthCheckFilterConfig {
                    pass_through_mode: false,
                    headers: vec![crate::types::header_matcher_exact(":path", "/healthz")],
                    cache_time: None,
                    cluster_min_healthy_percentages: None,
                    local_cluster: "c".to_string(),
                },
            ),
        };
        let mut inst = HttpFilterInstance::build(&hf, &test_registry(), "ingress_http")
            .expect("HealthCheck build succeeds");
        assert!(matches!(inst, HttpFilterInstance::HealthCheck(_)));
        let mut probe = FilterRequest::test("GET", "/healthz", &[]);
        assert!(matches!(
            inst.decode_headers(&mut probe),
            Decision::StopAndSend(_)
        ));
        let mut other = FilterRequest::test("GET", "/other", &[]);
        assert!(matches!(
            inst.decode_headers(&mut other),
            Decision::Continue
        ));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p envoy-config health_check_config_tests`
Expected: compile FAILURE — MEASURED at exactly 3 errors: `` error[E0599]: no variant named `UnsupportedHealthCheckField` found for enum `ConfigError` ``, the same for `UnsupportedHealthCheckPseudoHeader`, and `` no variant or associated item named `HealthCheck` found for enum `bootstrap::HttpFilterTypedConfig` ``.

- [ ] **Step 3: The `ConfigError` variants**

In `crates/envoy-config/src/lib.rs`, directly after `CdnLoopInvalidCdnId { listener: String, cdn_id: String },` and its blank line:

```rust
    /// Phase 115: a `health_check` filter sets something envoy-rust does not
    /// implement — `pass_through_mode: true`, `cache_time` or
    /// `cluster_min_healthy_percentages`. Startup-fatal (ADR-0049); upstream
    /// accepts the first (and `cache_time` alongside it), so this is a
    /// recorded REJECT-direction divergence (CF-115-1, CF-115-5).
    #[error(
        "health_check filter on listener `{listener}` sets `{field}`, which envoy-rust does not implement; only pass_through_mode: false is supported"
    )]
    UnsupportedHealthCheckField {
        listener: String,
        field: &'static str,
    },

    /// Phase 115: a `health_check` header matcher names a pseudo-header other
    /// than `:path`. The filter cannot see it; upstream matches `:method`,
    /// `:authority` and `:scheme` (MEASURED), so this is a recorded
    /// REJECT-direction divergence (CF-115-6).
    #[error(
        "health_check filter on listener `{listener}` matches pseudo-header `{name}`; only `:path` is supported"
    )]
    UnsupportedHealthCheckPseudoHeader { listener: String, name: String },
```

- [ ] **Step 4: The typed-config variant and its name**

In `crates/envoy-config/src/bootstrap.rs`, append to `enum HttpFilterTypedConfig` after `HeaderToMetadata(HeaderToMetadataConfig),` and a blank line:

```rust
    #[serde(
        rename = "type.googleapis.com/envoy.extensions.filters.http.health_check.v3.HealthCheck"
    )]
    HealthCheck(HealthCheckFilterConfig),
```

add `Self::HealthCheck(_) => "envoy.filters.http.health_check",` as the last arm of `expected_name`, and add as the last arm of `validate_http_filters`' `match &f.typed_config`:

```rust
            crate::HttpFilterTypedConfig::HealthCheck(cfg) => {
                validate_health_check_config(cfg, listener_name)?;
            }
```

- [ ] **Step 5: The validator**

Directly above `fn validate_set_metadata_config(`:

```rust
/// Phase 115: validate a `health_check` filter config. All boot-fatal
/// (ADR-0049). Upstream ACCEPTS `pass_through_mode: true`, and accepts
/// `cache_time` alongside it; envoy-rust implements only non-pass-through mode
/// and rejects both, plus `cluster_min_healthy_percentages` (CF-115-1,
/// CF-115-5). A matcher naming a pseudo-header other than `:path` is rejected
/// because the filter cannot see it (CF-115-6); upstream matches `:method`,
/// `:authority` and `:scheme` (MEASURED). Each matcher then runs the shared
/// `validate_header_matcher` gauntlet on a clone.
fn validate_health_check_config(
    cfg: &crate::HealthCheckFilterConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    let unsupported = |field| {
        Err(crate::ConfigError::UnsupportedHealthCheckField {
            listener: listener_name.to_string(),
            field,
        })
    };
    if cfg.pass_through_mode {
        return unsupported("pass_through_mode: true");
    }
    if cfg.cache_time.is_some() {
        return unsupported("cache_time");
    }
    if cfg.cluster_min_healthy_percentages.is_some() {
        return unsupported("cluster_min_healthy_percentages");
    }
    for hm in &cfg.headers {
        if hm.name.starts_with(':') && !hm.name.eq_ignore_ascii_case(":path") {
            return Err(crate::ConfigError::UnsupportedHealthCheckPseudoHeader {
                listener: listener_name.to_string(),
                name: hm.name.clone(),
            });
        }
        validate_header_matcher(&mut hm.clone())?;
    }
    Ok(())
}
```

- [ ] **Step 6: Stamp `node.cluster`**

In `validate`, directly after `let defer_cluster_refs = bootstrap.cds_configured_but_unloaded();` and its blank line:

```rust
    // 115: the bootstrap `node.cluster` (empty without a `node`, MEASURED to
    // match upstream), stamped into every `health_check` filter by
    // `validate_hcm`. Captured before the `&mut` listener loop below.
    let local_cluster = bootstrap
        .node
        .as_ref()
        .map_or_else(String::new, |n| n.cluster.clone());
```

Pass `&local_cluster,` as the new LAST argument of the single `validate_hcm(` call, add `local_cluster: &str,` as the new LAST parameter of `fn validate_hcm(`, and directly after `validate_http_filters(&hcm.http_filters, listener_name)?;` inside `validate_hcm`:

```rust
    for hf in &mut hcm.http_filters {
        if let crate::HttpFilterTypedConfig::HealthCheck(cfg) = &mut hf.typed_config {
            cfg.local_cluster = local_cluster.to_string();
        }
    }
```

`validate` runs at `parse_bootstrap` AND again at the post-merge re-validation in `load_dynamic_resources`, so LDS-delivered listeners are stamped too. `validate_hcm` has exactly one caller; seven parameters is under clippy's `too_many_arguments` threshold.

- [ ] **Step 7: Wire the instance**

In `crates/envoy-filter/src/instance.rs`: add `use crate::health_check::HealthCheckFilter;` after the `header_to_metadata` import; add the variant after `HeaderToMetadata(HeaderToMetadataFilter),`:

```rust
    /// Phase 115: the `envoy.filters.http.health_check` filter, non-pass-through
    /// mode (decode-side; answers a request matching every configured header
    /// matcher with an empty 200 carrying `x-envoy-upstream-healthchecked-cluster`
    /// and `%RESPONSE_CODE_DETAILS%` `health_check_ok`). No per-route config; 8
    /// stat counters registered under `http.{hcm_stat_prefix}.health_check.*`.
    HealthCheck(HealthCheckFilter),
```

add the `build` arm after the `HeaderToMetadata` arm:

```rust
            envoy_config::HttpFilterTypedConfig::HealthCheck(cfg) => {
                Ok(HttpFilterInstance::HealthCheck(
                    HealthCheckFilter::build_from_config(cfg, registry, hcm_stat_prefix)?,
                ))
            }
```

and add `HttpFilterInstance::HealthCheck(f) => f.decode_headers(req),` / `HttpFilterInstance::HealthCheck(f) => f.encode_headers(resp_arg),` after the `HeaderToMetadata` arms of `decode_headers` / `encode_headers`. `apply_route_config` needs nothing — its `_ => {}` arm covers it.

- [ ] **Step 8: Run to verify they pass**

Run: `cargo test -p envoy-config health_check_config_tests && cargo test -p envoy-filter health_check`
Expected: `13 passed` for `envoy-config`; `11 passed` for `envoy-filter` (10 filter + 1 instance).

- [ ] **Step 9: Gate — clippy is REQUIRED again and closes Task 3's deferral**

```bash
cargo build --workspace --all-targets && cargo fmt --all -- --check \
  && cargo clippy --workspace --all-targets --all-features -- -D warnings \
  && cargo test -p envoy-config -p envoy-filter
```
Expected: clippy exit **0** with at least one `Checking` line (an exit 0 with zero `Checking` lines is a cached no-op and proves nothing — `touch crates/envoy-filter/src/lib.rs` and re-run).

- [ ] **Step 10: Commit**

```bash
git add crates/envoy-config/src crates/envoy-filter/src/instance.rs
git commit -m "phase 115 task 4: health_check reachable — typed config, validator, node.cluster stamping, 13th HttpFilterInstance"
```

---

## Task 5: Upstream does not count an intercepted probe — the per-class counter exclusion, and the in-process end-to-end pins

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs`, `crates/envoy-http2/src/hcm.rs`

**Interfaces:**
- Consumes: `envoy_filter::health_check::HEALTH_CHECK_OK` (Task 3); Task 2's `h2_response_code_details_line`; Task 4's stamping.
- Produces: nothing new; `downstream_rq_{2,3,4,5}xx` are no longer ticked for a response whose `%RESPONSE_CODE_DETAILS%` is `health_check_ok`.

The detail string is the discriminator because it is set by exactly one producer (this filter) and is already live at both tick sites; no new flag is threaded.

- [ ] **Step 1: Write the failing tests**

H1 — insert directly above `async fn h1_stop_and_send_at_encode_substitutes_wire_response()`'s `#[tokio::test]` line:

```rust
    /// Phase 115: `envoy.filters.http.health_check` end to end on H1, from
    /// bootstrap YAML through `parse_bootstrap` (which stamps `node.cluster`)
    /// and `HCMConfig::from_config`. The wire and the access log are two
    /// independent readings of the same two requests.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_health_check_filter_end_to_end() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = dir.path().join("access.log");
        let yaml = format!(
            r#"
node: {{ id: n, cluster: hc-node-cluster }}
static_resources:
  listeners:
    - name: l
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 0 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: {log}
                      log_format:
                        text_format_source:
                          inline_string: "%REQ(:PATH)%|%RESPONSE_CODE_DETAILS%|%RESPONSE_FLAGS%|%BYTES_SENT%\n"
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: MAIN }} }}
                http_filters:
                  - name: envoy.filters.http.health_check
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.health_check.v3.HealthCheck
                      pass_through_mode: false
                      headers:
                        - {{ name: ":path", string_match: {{ exact: /healthz }} }}
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#,
            log = log.display()
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("bootstrap loads");
        let Some(envoy_config::TypedConfig::HttpConnectionManager(hcm)) =
            &bootstrap.static_resources.listeners[0].filter_chains[0].filters[0].typed_config
        else {
            panic!("HCM expected");
        };
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let config = Arc::new(
            HCMConfig::from_config(
                hcm,
                cluster_mgr_empty().await,
                Arc::clone(&registry),
                None,
                Arc::new(RuntimeSnapshot::default()),
            )
            .await
            .expect("HCM config builds"),
        );

        let probe =
            |path: &str| format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
        let hit = String::from_utf8(drive(Arc::clone(&config), probe("/healthz").as_bytes()).await)
            .unwrap();
        assert!(hit.starts_with("HTTP/1.1 200 "), "{hit}");
        assert!(
            hit.contains("\r\nx-envoy-upstream-healthchecked-cluster: hc-node-cluster\r\n"),
            "{hit}"
        );
        assert!(!hit.to_ascii_lowercase().contains("content-type"), "{hit}");
        assert!(hit.ends_with("\r\n\r\n"), "empty body: {hit}");

        let miss =
            String::from_utf8(drive(config, probe("/healthz?x=1").as_bytes()).await).unwrap();
        assert!(miss.ends_with("MAIN"), "query string falls through: {miss}");
        let count = |name: &str| {
            registry
                .register_counter(&format!("http.ingress_http.{name}"))
                .unwrap()
                .value()
        };
        assert_eq!(count("downstream_rq_total"), 2);
        assert_eq!(
            count("downstream_rq_2xx"),
            1,
            "the intercept is not counted"
        );
        assert_eq!(count("health_check.request_total"), 1);

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let lines = tokio::fs::read_to_string(&log).await.expect("log");
        assert_eq!(
            lines,
            "/healthz|health_check_ok|-|0\n/healthz?x=1|direct_response|-|4\n"
        );
    }
```

H2 — insert directly above `` /// Phase 115: a decode-side filter's `FilterResponse::details` is the H2 ``:

```rust
    /// Phase 115: `envoy.filters.http.health_check` on H2 (CF-115-4 pins the
    /// H2 cell in-process only): the `:path` matcher sees the query string, a
    /// match carries the stamped `local_cluster` and logs `health_check_ok`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_health_check_filter_intercepts_and_logs() {
        let cfg = envoy_config::HealthCheckFilterConfig {
            pass_through_mode: false,
            headers: vec![HeaderMatcher {
                name: ":path".to_string(),
                mode: HeaderMatcherMode::ExactMatch("/healthz".to_string()),
                invert_match: false,
            }],
            cache_time: None,
            cluster_min_healthy_percentages: None,
            local_cluster: "hc-node-cluster".to_string(),
        };
        let pipeline = envoy_filter::FilterPipeline::build_from_config(
            &[
                HttpFilter {
                    name: "envoy.filters.http.health_check".to_string(),
                    typed_config: HttpFilterTypedConfig::HealthCheck(cfg),
                },
                HttpFilter {
                    name: "envoy.filters.http.router".to_string(),
                    typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
                },
            ],
            &Arc::new(envoy_stats::StatsRegistry::new()),
            "ingress_http_h2",
        )
        .expect("pipeline builds");
        let pipeline = Arc::new(pipeline);
        let (headers, line, rq_2xx) =
            h2_response_code_details_line(Some(Arc::clone(&pipeline)), "http://x/healthz").await;
        assert_eq!(line.trim(), "d=health_check_ok");
        assert_eq!(rq_2xx, 0, "the intercept is not counted");
        assert_eq!(
            headers["x-envoy-upstream-healthchecked-cluster"],
            "hc-node-cluster"
        );
        assert!(!headers.contains_key("content-type"));
        let (_, line, rq_2xx) =
            h2_response_code_details_line(Some(pipeline), "http://x/healthz?x=1").await;
        assert_eq!(rq_2xx, 1, "a fall-through is counted");
        assert_eq!(
            line.trim(),
            "d=direct_response",
            "the query string falls through"
        );
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p envoy-http1 h1_health_check_filter_end_to_end && cargo test -p envoy-http2 h2_health_check_filter_intercepts_and_logs`
Expected: H1 FAILS on `the intercept is not counted` (`left: 2`, `right: 1`); H2 FAILS on `the intercept is not counted` (`left: 1`, `right: 0`). Every other assertion in both tests already passes — the wire and the log are Task 3/4's.

- [ ] **Step 3: The H1 exclusion**

Replace the `match response_status_for_log / 100 { … }` block under the `06.3 D15.3.a NEW — per-response-class HCM counters` comment in `serve_connection` (keep that comment) with:

```rust
        //
        // Phase 115 (MEASURED): a request the health_check filter answered is
        // NOT counted here.
        if response_code_details_for_log.as_deref()
            != Some(envoy_filter::health_check::HEALTH_CHECK_OK)
        {
            match response_status_for_log / 100 {
                2 => config.stats.downstream_rq_2xx.inc(),
                3 => config.stats.downstream_rq_3xx.inc(),
                4 => config.stats.downstream_rq_4xx.inc(),
                5 => config.stats.downstream_rq_5xx.inc(),
                _ => {}
            }
        }
```

- [ ] **Step 4: The H2 exclusion**

Replace the `match response_status_for_log / 100 { … }` block in `finalize_h2_stream` (keep its comment) with:

```rust
    //
    // Phase 115 (MEASURED): a request the health_check filter answered is NOT
    // counted here.
    if response_code_details_for_log_h2.as_deref()
        != Some(envoy_filter::health_check::HEALTH_CHECK_OK)
    {
        match response_status_for_log / 100 {
            2 => config.inner.stats.downstream_rq_2xx.inc(),
            3 => config.inner.stats.downstream_rq_3xx.inc(),
            4 => config.inner.stats.downstream_rq_4xx.inc(),
            5 => config.inner.stats.downstream_rq_5xx.inc(),
            _ => {}
        }
    }
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p envoy-http1 -p envoy-http2`
Expected: all green.

- [ ] **Step 6: Mutation**

```bash
F=crates/envoy-http1/src/hcm.rs; cp $F /tmp/h1.bak
grep -cF '!= Some(envoy_filter::health_check::HEALTH_CHECK_OK)' $F     # must print 1
sed -i 's/!= Some(envoy_filter::health_check::HEALTH_CHECK_OK)/!= Some("__never__")/' $F
cargo test -p envoy-http1 h1_health_check_filter_end_to_end
cp /tmp/h1.bak $F && md5sum /tmp/h1.bak $F
```
Expected: `Compiling envoy-http1`, then FAILED `the intercept is not counted` `left: 2` `right: 1`; identical md5s.

- [ ] **Step 7: Gate and commit**

```bash
cargo build --workspace --all-targets && cargo fmt --all -- --check \
  && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 115 task 5: an intercepted health-check probe is not counted in downstream_rq_Nxx; H1/H2 end-to-end pins"
```

---

## Task 6: Differential fixture `0095-http-filter-health-check`

**Files:**
- Create: `tests/fixtures/0095-http-filter-health-check/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/http_filter_health_check.rs`

**Interfaces:** consumes the whole in-process surface; produces nothing.

- [ ] **Step 1: `envoy.yaml`**

```yaml
# Phase 115: envoy.filters.http.health_check, non-pass-through mode.
# BYTE-IDENTICAL to envoy-rust.yaml. See README.md.
node: { id: fixture-0095, cluster: hc-fixture-cluster }
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
                        # The fall-through witness: a 4-byte body with a
                        # content-type. An intercepted probe has neither.
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "MAIN" }
                http_filters:
                  # Filter A — one `:path` matcher.
                  - name: envoy.filters.http.health_check
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.health_check.v3.HealthCheck
                      pass_through_mode: false
                      headers:
                        - name: ":path"
                          string_match: { exact: "/healthz" }
                  # Filter B — two matchers, which must BOTH match (AND).
                  - name: envoy.filters.http.health_check
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.health_check.v3.HealthCheck
                      pass_through_mode: false
                      headers:
                        - name: ":path"
                          string_match: { exact: "/both" }
                        - name: "x-probe"
                          string_match: { exact: "yes" }
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
```

- [ ] **Step 2: `envoy-rust.yaml` is a byte-identical copy**

```bash
cp tests/fixtures/0095-http-filter-health-check/envoy.yaml tests/fixtures/0095-http-filter-health-check/envoy-rust.yaml
cmp tests/fixtures/0095-http-filter-health-check/envoy.yaml tests/fixtures/0095-http-filter-health-check/envoy-rust.yaml
```
Expected: `cmp` silent. The `node:` block is on BOTH sides and `hc-fixture-cluster` is a value no YAML-1.1 parser booleanizes (the `0088` README records the trap an unquoted `y` sets). `admin` uses a literal `port_value: 0`: `{{ADMIN_PORT}}` is driver-gated and `Http1ProbeList` does not receive it.

- [ ] **Step 3: `expectations.yaml`**

```yaml
# Phase 115: ten sequential HTTP/1.1 probes against a backend-free,
# CLUSTER-FREE HCM listener whose chain is
#   [health_check A (:path exact /healthz),
#    health_check B (:path exact /both AND x-probe exact yes),
#    router]
# with a `prefix: "/"` direct_response catch-all answering `MAIN`.
#
# EVERY probe answers 200, so status alone cannot pass this fixture. The body
# decides: an intercepted probe has an EMPTY body (and no content-type), a
# fall-through answers `MAIN`. `set_equal_modulo_allow_list` additionally
# compares `x-envoy-upstream-healthchecked-cluster` VALUE-EXACT — it must be
# the bootstrap `node.cluster` (`hc-fixture-cluster`) on both proxies.
#
# The cells (MEASURED against envoyproxy/envoy:v1.33.0, SPEC.md §2 and
# ADR-0201):
#   p1       the baseline intercept
#   p2       `:path` INCLUDES the query string — the trap of the phase
#   p3/p4    exact is exact; the value match is case-sensitive
#   p5       a non-match continues to the route
#   p6       the filter is method-agnostic (POST with a body)
#   p7       the header value is proxy state, not a request echo
#   p8-p10   a two-entry matcher list folds as AND
driver:
  kind: http1_probe_list
  probes:
    - name: p1-healthz-intercepted
      method: get
      path: "/healthz"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: p2-query-string-falls-through
      method: get
      path: "/healthz?x=1"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "MAIN" }
      expected_headers: set_equal_modulo_allow_list
    - name: p3-trailing-slash-falls-through
      method: get
      path: "/healthz/"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "MAIN" }
      expected_headers: set_equal_modulo_allow_list
    - name: p4-case-sensitive-falls-through
      method: get
      path: "/healthZ"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "MAIN" }
      expected_headers: set_equal_modulo_allow_list
    - name: p5-other-falls-through
      method: get
      path: "/other"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "MAIN" }
      expected_headers: set_equal_modulo_allow_list
    - name: p6-post-with-body-intercepted
      method: post
      path: "/healthz"
      host: "envoy-rust.test"
      body: "abc"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: p7-request-header-is-not-echoed
      method: get
      path: "/healthz"
      host: "envoy-rust.test"
      extra_headers:
        - ["x-envoy-upstream-healthchecked-cluster", "SENTINEL"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: p8-and-both-matchers-intercepted
      method: get
      path: "/both"
      host: "envoy-rust.test"
      extra_headers:
        - ["x-probe", "yes"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: p9-and-path-alone-falls-through
      method: get
      path: "/both"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "MAIN" }
      expected_headers: set_equal_modulo_allow_list
    - name: p10-and-header-alone-falls-through
      method: get
      path: "/other"
      host: "envoy-rust.test"
      extra_headers:
        - ["x-probe", "yes"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "MAIN" }
      expected_headers: set_equal_modulo_allow_list
```

- [ ] **Step 4: The runner**

```rust
//! Phase 115 differential acceptance test for fixture
//! `0095-http-filter-health-check`: `envoy.filters.http.health_check` in
//! non-pass-through mode.
//!
//! Ten HTTP/1.1 probes at a backend-free, CLUSTER-FREE HCM listener whose chain
//! is two health_check filters ahead of the router, with a `direct_response`
//! catch-all answering `MAIN`. Every probe answers 200, so the body decides: an
//! intercepted probe is empty, a fall-through is `MAIN`. The cells witness that
//! `:path` includes the query string, that exact matching is exact and
//! case-sensitive, that the filter is method-agnostic, that the
//! `x-envoy-upstream-healthchecked-cluster` value is the bootstrap
//! `node.cluster` rather than a request echo, and that a two-entry matcher list
//! folds as AND.
//!
//! `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL. Docker-gated and
//! backend-free, so fully verifiable on a developer host.

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_health_check_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0095-http-filter-health-check");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 5: The README**

````markdown
# 0095 — `envoy.filters.http.health_check` (non-pass-through mode)

Phase **115** (`ADR-0200` pick, `ADR-0201` PLAN-write). Ten HTTP/1.1 probes against a
**backend-free, CLUSTER-FREE** HCM listener whose chain is

```
[ health_check A  (:path exact /healthz),
  health_check B  (:path exact /both  AND  x-probe exact yes),
  router ]
```

with a `prefix: "/"` `direct_response` catch-all answering the 4-byte body `MAIN`.

## Why it is not vacuous

**Every probe answers 200**, so status alone cannot pass it. An intercepted probe has an EMPTY
body and no `content-type`; a fall-through answers `MAIN` with `content-type: text/plain`. Each
probe asserts the body byte-exact, and `set_equal_modulo_allow_list` compares
`x-envoy-upstream-healthchecked-cluster` VALUE-exact on both proxies.

## What it witnesses

| probe | request | expected | rule |
|---|---|---|---|
| `p1` | `GET /healthz` | empty | the baseline intercept |
| `p2` | `GET /healthz?x=1` | `MAIN` | **`:path` includes the query string** — the trap of the phase |
| `p3` | `GET /healthz/` | `MAIN` | exact is exact |
| `p4` | `GET /healthZ` | `MAIN` | the value match is case-sensitive |
| `p5` | `GET /other` | `MAIN` | a non-match continues to the route |
| `p6` | `POST /healthz` + body | empty | the filter is method-agnostic |
| `p7` | `GET /healthz` + `x-envoy-upstream-healthchecked-cluster: SENTINEL` | empty, header = `hc-fixture-cluster` | the header is proxy state, not a request echo |
| `p8` | `GET /both` + `x-probe: yes` | empty | both matchers match |
| `p9` | `GET /both` | `MAIN` | the list is AND, not OR |
| `p10` | `GET /other` + `x-probe: yes` | `MAIN` | …in the other direction |

**The header value is the bootstrap `node.cluster`.** `SPEC.md` §2.2 recorded it as EMPTY; that
was measured on a config with no `node:` block. With `node.cluster` set it carries that string
(`ADR-0201`), so this fixture sets `node: { id: fixture-0095, cluster: hc-fixture-cluster }` —
on BOTH sides, with a value no YAML-1.1 parser booleanizes.

## Byte-identical configs

`envoy.yaml` and `envoy-rust.yaml` are **byte-identical** (`cmp` silent). This is a per-fixture
claim; re-derive it, do not inherit it.

## Running it

Backend-free (no `{{BACKEND_IP}}`), so it is fully verifiable on a developer host. The harness
runs the DEBUG `envoy-bin`, so rebuild it first:

```bash
cargo build -p envoy-bin
cargo test -p differential --test http_filter_health_check
```

A green run in a few seconds is normal for a backend-free fixture.

## Proof it is not vacuous

Each mutation was applied, `envoy-bin` rebuilt, the fixture run, and the file restored
(md5-verified). The driver aborts at the FIRST failing probe, so each red run names one probe.

| # | mutation | result |
|---|---|---|
| V1 | stamp an empty `local_cluster` in `validate_hcm` | `p1` REDs on `diff_headers` |
| V2 | strip the query string before matching `:path` | `p2` REDs (`MAIN` expected, empty returned) |
| V3 | fold the matchers with `any` instead of `all` | `p9` REDs (`MAIN` expected, empty returned) |
````

- [ ] **Step 6: Run it GREEN**

```bash
cargo build -p envoy-bin
cargo test -p differential --test http_filter_health_check
```
Expected: `test result: ok. 1 passed`, in a few seconds. **The harness runs the DEBUG `envoy-bin`** — a stale binary without Task 4 rejects `envoy.filters.http.health_check` as an unknown variant. A backend-free green in ~1–8 s is normal; to prove the upstream container really ran, poll `docker ps --format '{{.ID}} {{.Image}}'` during the run and look for `envoyproxy/envoy:v1.33.0` (measured: one container observed).

- [ ] **Step 7: Mutations V1–V3 (each: apply, `cargo build -p envoy-bin` showing a `Compiling` line for the mutated crate, run, restore, md5)**

| # | file | exact edit (assert the target occurs once first) | expected RED |
|---|---|---|---|
| V1 | `crates/envoy-config/src/bootstrap.rs` | `cfg.local_cluster = local_cluster.to_string();` → `cfg.local_cluster = String::new();` | `probe p1-healthz-intercepted: diff_headers` |
| V2 | `crates/envoy-filter/src/health_check.rs` | `req.path.clone()` → `req.path.split('?').next().unwrap_or_default().to_string()` | `probe p2-query-string-falls-through: subject body != expected` |
| V3 | `crates/envoy-filter/src/health_check.rs` | `self.headers.iter().all(\|m\| {` → `self.headers.iter().any(\|m\| {` | `probe p9-and-path-alone-falls-through: subject body != expected` |

The driver aborts at the FIRST failing probe, so each red names exactly one probe. After the last restore, rebuild `envoy-bin` and re-run the fixture GREEN — the unmutated control, from the same tree.

- [ ] **Step 8: Commit**

```bash
git add tests/fixtures/0095-http-filter-health-check tests/differential/tests/http_filter_health_check.rs
git commit -m "phase 115 task 6: fixture 0095-http-filter-health-check — ten probes, byte-identical configs"
```

---

## Task 7: Differential fixture `0096-http-filter-health-check-stats`

**Files:**
- Create: `tests/fixtures/0096-http-filter-health-check-stats/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/http_filter_health_check_stats.rs`

- [ ] **Step 1: `envoy.yaml`**

```yaml
# Phase 115: the envoy.filters.http.health_check STATS witness.
# BYTE-IDENTICAL to envoy-rust.yaml. See README.md.
node: { id: fixture-0096, cluster: hc-fixture-cluster }
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: {{ADMIN_PORT}} }
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
                            body: { inline_string: "MAIN" }
                http_filters:
                  - name: envoy.filters.http.health_check
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.health_check.v3.HealthCheck
                      pass_through_mode: false
                      headers:
                        - name: ":path"
                          string_match: { exact: "/healthz" }
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

`{{ADMIN_PORT}}` IS substituted for `Driver::AdminScrape`.

- [ ] **Step 2: `envoy-rust.yaml` — byte-identical copy, verified with `cmp` as in Task 6**

- [ ] **Step 3: `expectations.yaml`**

```yaml
# Phase 115: three HTTP/1.1 pre-requests — two intercepted `/healthz` probes
# and one `/other` fall-through — then a bilateral ABSOLUTE stat assertion.
#
# The non-zero entries are the witnesses (MEASURED against
# envoyproxy/envoy:v1.33.0, ADR-0201):
#   health_check.request_total / ok  tick once per INTERCEPT, never on a
#                                    fall-through;
#   downstream_rq_total              counts all three requests;
#   downstream_rq_2xx                counts ONLY the fall-through — an
#                                    intercepted probe is NOT counted.
#
# ⚠ `scrape_admin_stat` returns 0 for a name a proxy never registered, so the
# `value: 0` entry is NOT a presence witness; it pins that `failed` does not
# move. Presence of the six zero-valued counters is pinned in-process only.
driver:
  kind: admin_scrape
  pre_requests:
    - { method: GET, path: /healthz, host: envoy-rust.test, port_key: PORT }
    - { method: GET, path: /other, host: envoy-rust.test, port_key: PORT }
    - { method: GET, path: /healthz, host: envoy-rust.test, port_key: PORT }
  # The driver requires a non-empty `scrapes:`; this is fixture 0015's
  # /server_info sub-case verbatim. The witnesses are `expected_stats` below.
  scrapes:
    - path: /server_info
      expected_status: 200
      expected_content_type: "application/json"
      expected_body_rule:
        kind: json_shape
        required_keys: ["state"]
        value_may_differ_keys:
          - state
          - version
          - hot_restart_version
          - command_line_options
          - node
        allowlist_envoy_only_keys:
          - uptime_current_epoch
          - uptime_all_epochs
        allowlist_envoy_rust_only_keys:
          - uptime_current_epoch_seconds
          - uptime_all_epochs_seconds
  expected_stats:
    - { name: "http.ingress_http.health_check.request_total", value: 2 }
    - { name: "http.ingress_http.health_check.ok", value: 2 }
    - { name: "http.ingress_http.downstream_rq_total", value: 3 }
    - { name: "http.ingress_http.downstream_rq_2xx", value: 1 }
    - { name: "http.ingress_http.health_check.failed", value: 0 }
```

⚠ The driver REJECTS an empty `scrapes:` (`Driver::AdminScrape requires at least one sub-case`) — measured on the prototype, which is why fixture `0015`'s `/server_info` sub-case is carried.

- [ ] **Step 4: The runner**

```rust
//! Phase 115 differential acceptance test for fixture
//! `0096-http-filter-health-check-stats`: the stat surface of
//! `envoy.filters.http.health_check`.
//!
//! Two intercepted `/healthz` probes and one `/other` fall-through, then a
//! bilateral absolute stat assertion: `health_check.request_total` and
//! `health_check.ok` count intercepts only, `downstream_rq_total` counts all
//! three requests, and `downstream_rq_2xx` counts ONLY the fall-through —
//! upstream does not count a request the filter answered.
//!
//! `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL. Docker-gated and
//! backend-free.

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_health_check_stats_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0096-http-filter-health-check-stats");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 5: The README**

````markdown
# 0096 — `envoy.filters.http.health_check` stats

Phase **115** (`ADR-0201`). The stat-surface sibling of `0095`, on the existing
`Driver::AdminScrape`: three HTTP/1.1 pre-requests (`GET /healthz`, `GET /other`, `GET /healthz`)
against a backend-free, cluster-free HCM listener whose chain is `[health_check (:path exact
/healthz), router]`, then a bilateral ABSOLUTE stat assertion on both admin listeners.

## What it witnesses

| stat | value | rule (MEASURED against `envoyproxy/envoy:v1.33.0`) |
|---|---:|---|
| `http.ingress_http.health_check.request_total` | 2 | ticks once per INTERCEPT, never on a fall-through |
| `http.ingress_http.health_check.ok` | 2 | the same, in non-pass-through mode |
| `http.ingress_http.downstream_rq_total` | 3 | every request is counted |
| `http.ingress_http.downstream_rq_2xx` | 1 | **an intercepted probe is NOT counted** — only the fall-through is |
| `http.ingress_http.health_check.failed` | 0 | ⚠ NOT a presence witness (see below) |

⚠ `scrape_admin_stat` returns `0` for a name a proxy never registered, so a `value: 0` entry passes
even when the stat is ABSENT. Only the four non-zero entries are witnesses. Upstream registers all
eight `health_check.*` counters at config load; their PRESENCE on envoy-rust is pinned in-process
(`registers_eight_counters_and_ticks_two_per_intercept`), not here.

**Not witnessed, and not implemented:** upstream also ticks `http.ingress_http.tracing.health_check`.
envoy-rust has no `tracing.*` stat family at all (CF-115-7).

The `scrapes:` entry is fixture `0015`'s `/server_info` sub-case, present only because the driver
requires a non-empty list.

## Byte-identical configs

`envoy.yaml` and `envoy-rust.yaml` are byte-identical (`cmp` silent). Re-derive, do not inherit.

## Running it

```bash
cargo build -p envoy-bin
cargo test -p differential --test http_filter_health_check_stats
```

## Proof it is not vacuous

| # | mutation | result |
|---|---|---|
| V4 | the H1 per-class counter gate compares against a sentinel that never matches | `subject stat http.ingress_http.downstream_rq_2xx expected 1 got 3` |
| V5 | delete `self.request_total.inc();` | `subject stat http.ingress_http.health_check.request_total expected 2 got 0` |
````

- [ ] **Step 6: Run it GREEN, then mutations V4–V5**

```bash
cargo build -p envoy-bin
cargo test -p differential --test http_filter_health_check_stats
```

| # | file | exact edit | expected RED |
|---|---|---|---|
| V4 | `crates/envoy-http1/src/hcm.rs` | `!= Some(envoy_filter::health_check::HEALTH_CHECK_OK)` → `!= Some("__never__")` | `subject stat http.ingress_http.downstream_rq_2xx expected 1 got 3` |
| V5 | `crates/envoy-filter/src/health_check.rs` | delete the line `self.request_total.inc();` | `subject stat http.ingress_http.health_check.request_total expected 2 got 0` |

Each: assert the target occurs once, rebuild `envoy-bin`, run, restore, md5; then the unmutated control GREEN.

- [ ] **Step 7: Commit**

```bash
git add tests/fixtures/0096-http-filter-health-check-stats tests/differential/tests/http_filter_health_check_stats.rs
git commit -m "phase 115 task 7: fixture 0096-http-filter-health-check-stats — intercepts are counted by health_check, not by downstream_rq_2xx"
```

---

## Task 8: `BEHAVIOR_CONTRACT.md` — the health_check section

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md`

This is the only `docs/` change in the phase and is EXCLUDED from the §6.1 LoC gate.

- [ ] **Step 1: Locate the anchor and assert it is unique**

```bash
grep -cF '**H1 upstream connection-pool `Connection: close` single-use (ADR-0059).**' docs/envoy-rust/BEHAVIOR_CONTRACT.md
```
Expected: `1`. Insert the new blocks directly ABOVE that line, i.e. immediately after the `cdn_loop` blocks (`**cdn_loop stats — NONE** …`), which is where the HTTP-filter wire contracts live.

- [ ] **Step 2: Insert**

````markdown
**health_check filter, non-pass-through mode (ADR-0200 SPEC / ADR-0201).**

> The HTTP-filter family's eleventh row. `envoy.filters.http.health_check` answers a downstream
> liveness probe AT THE PROXY: a request matching every configured header matcher is
> short-circuited with a local reply and never reaches the route; anything else continues down
> the chain. Decode-side only, no per-route config. Differentially proven by fixtures
> `0095-http-filter-health-check` (ten probes, the wire) and `0096-http-filter-health-check-stats`
> (the counters) against `envoyproxy/envoy:v1.33.0`.

**health_check intercept wire shape (MEASURED).**

- Status **200**; body EMPTY (`content-length: 0`); **NO `content-type`** — the filter-synth
  decorators add it only for a non-empty body.
- One filter header, `x-envoy-upstream-healthchecked-cluster`, whose value is the bootstrap
  **`node.cluster`** — empty only when the bootstrap has no `node` (or an empty `cluster`). It is
  NOT an upstream cluster (an unrelated static cluster does not change it) and NOT an echo (a
  request carrying the header with any value still gets `node.cluster` back). envoy-rust stamps it
  into the filter config in `validate_hcm`, so LDS-delivered listeners carry it too.
- `server`, `date` and (under a `Connection: close` request) `connection: close`, as for every
  filter-synth reply. Compared set-equal modulo the header allow-list; the filter header is
  value-compared.
- Access log: `%RESPONSE_CODE_DETAILS%` = **`health_check_ok`**, `%RESPONSE_FLAGS%` = `-`,
  `%BYTES_SENT%` = `0`. It is the first landed filter to set a response-code detail; the value
  travels on `FilterResponse::details` and is read only on the decode-side `StopAndSend` path.

**health_check matching rule (MEASURED).**

- The `headers` list is AND-combined over the landed seven-mode `HeaderMatcher`; an absent or
  empty list matches EVERY request.
- **`:path` is matched WITH its query string**: `exact: /healthz` does not match `/healthz?x=1`.
  Exact is exact (`/healthz/` falls through) and the value match is case-sensitive (`/healthZ`
  falls through).
- The filter is method-agnostic (`POST /healthz` with a body is intercepted).
- Chain order is declaration order: `[health_check, fault(abort 100%)]` answers `/healthz` with
  200; `[fault, health_check]` answers it with the fault's 503.
- envoy-rust feeds a `:path` matcher `FilterRequest::path` verbatim (both codecs keep the query
  there); no codec puts pseudo-headers into the filter-visible header list.

**health_check stats (MEASURED).**

- Eight counters are REGISTERED at config load under `http.<stat_prefix>.health_check.`:
  `cached_response`, `degraded`, `failed`, `failed_cluster_empty`, `failed_cluster_not_found`,
  `failed_cluster_unhealthy`, `ok`, `request_total`. An intercept ticks `request_total` and `ok`;
  a fall-through ticks none. The other six stay `0` in non-pass-through mode.
- **An intercepted request is NOT counted in `downstream_rq_2xx`** (nor, upstream, in
  `downstream_rq_completed`); it IS counted in `downstream_rq_total`. envoy-rust skips its
  `downstream_rq_{2,3,4,5}xx` tick for a response whose detail is `health_check_ok`, on both
  codecs.

**health_check config validity (ALL BOOT-FATAL — ADR-0049).**

- `pass_through_mode` is REQUIRED on both sides. `@type` =
  `type.googleapis.com/envoy.extensions.filters.http.health_check.v3.HealthCheck`.
- **Recorded REJECT-direction divergences** — upstream ACCEPTS, envoy-rust rejects with
  `ConfigError::UnsupportedHealthCheckField`: `pass_through_mode: true` (CF-115-5), any
  `cache_time` (CF-115-5; upstream rejects it only when `pass_through_mode` is `false`, with a
  message that misspells the field `path_through_mode` — envoy-rust does NOT reproduce the typo,
  §7.4), and any `cluster_min_healthy_percentages` (CF-115-1).
- A matcher naming `:method`, `:authority`, `:scheme` or any other `:`-prefixed name except
  `:path` is rejected with `ConfigError::UnsupportedHealthCheckPseudoHeader`; upstream MATCHES all
  three (CF-115-6). `host` is an ordinary header on both sides.
- Each matcher runs the shared `HeaderMatcher` validation (empty name, bad regex, bad range).

**health_check — measured behaviour envoy-rust does NOT match.**

- After `POST /healthcheck/fail` on the admin listener upstream answers a probe with **503**
  and decorates NON-health-check responses too with `x-envoy-immediate-health-check-fail: true`
  and `connection: close` — an HCM-level behaviour no landed driver can witness (CF-115-2).
- Upstream ticks `http.<stat_prefix>.tracing.health_check` per intercept; envoy-rust has no
  `tracing.*` HCM stat family (CF-115-7).
- The H2 behaviour is pinned in-process only; there is no H2 differential witness (CF-115-4).

````

- [ ] **Step 3: Verify, and commit**

Run: `git diff --numstat docs/envoy-rust/BEHAVIOR_CONTRACT.md` — expected: additions only, deletions `0`.

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 115 task 8: BEHAVIOR_CONTRACT — the health_check filter section"
```

---

## Expected CI identity — a PREDICTION, not a measurement

The last recorded identity is `binaries=170 passed=2315 failed=0` (run `34688774583`, on `81993e3`). This phase adds **2** test binaries (the two differential runners) and **30** test functions (`envoy-config` 13, `envoy-filter` 11, `envoy-http1` 2, `envoy-http2` 2, `differential` 2), and removes none. **Predicted: `binaries=172 passed=2345 failed=0`.** ⚠ This is arithmetic on an unbuilt tree; state 4 measures. A renamed test adds ZERO (`ADR-0187`), and no test is renamed here.

---

## Carry-forwards this phase opens or amends

- **CF-115-1** (`cluster_min_healthy_percentages`), **CF-115-3** (the `not_health_check_filter` access-log arm — still the natural successor, and now cheaper: the filter it needs lands here), **CF-115-4** (the H2 differential witness — the H2 behaviour is pinned IN-PROCESS by Task 5's `h2_health_check_filter_intercepts_and_logs`, which narrows but does not close it) and **CF-115-5** (`pass_through_mode: true` + `cache_time`) — **unchanged**.
- **CF-115-2** — **unchanged, re-confirmed** by PV-5 at `74f2e12`.
- **CF-115-6 (NEW).** A matcher naming `:method`, `:authority`, `:scheme` or any other `:`-prefixed name except `:path` is rejected at load; upstream matches all three (MEASURED). A recorded REJECT-direction divergence.
- **CF-115-7 (NEW).** `http.<stat_prefix>.tracing.health_check` — upstream ticks it per intercept; envoy-rust has no `tracing.*` HCM stat family at all. Not witnessed (`0096`'s zero-for-absent trap would make a `value: 0` entry vacuous and a non-zero entry fail on envoy-rust).
- **CF-115-8 (NEW, pre-existing, MEASURED).** Landed filters' local replies carry no `%RESPONSE_CODE_DETAILS%`/`%RESPONSE_FLAGS%`. Measured for `fault`: upstream `fault_filter_abort` / `FI`, envoy-rust `-` / `-`. Task 2's seam makes the details half a one-field change per filter; the flags half needs a derive. Other filters' upstream values are unmeasured.
- **CF-115-9 (NEW, pre-existing, MEASURED — a PANIC).** A `fault` filter whose `headers` gate uses `safe_regex_match` LOADS (`parse_bootstrap` Ok), BUILDS (`FilterPipeline::build_from_config` Ok), and then PANICS on the first request carrying that header with `validator ensured HeaderMatcher SafeRegex compiled` (`crates/envoy-config/src/matcher.rs`): `validate_fault_config` never compiles the gate's regexes and `FaultFilter::build_from_config` clones them uncompiled. Reproduced by a throwaway test on the prototype tree; `crates/envoy-filter/src/fault.rs` is NOT touched by this phase, so it is not a rider. `HealthCheckFilter` compiles its matchers at build (Task 3) and is not affected. **The strongest successor candidate this PLAN-write found.**
- **CF-115-10 (NEW, pre-existing, MEASURED).** Route `match.headers` entries naming `:path` or `:method` never match in envoy-rust (the route walker passes only `req.headers`); upstream matches both.
- **CF-114-6 is CONSUMED** by Task 1. Every other previously banked carry-forward stands INTACT, including **CF-75-5** (`cdn_loop_parse` has 0 tracked corpus seeds) and the phase-112 ALPN cleanup rider, which is NOT taken and NOT re-costed.

---

## Self-review

**Spec coverage.** `SPEC.md` §4's six deliverables: 1 (config schema) → T3; 2 (validators) → T4; 3 (`health_check.rs` + the thirteenth variant) → T3 + T4; 4 (the `details` seam) → T2; 5 (fixture `0095`) → T6; 6 (the contract section) → T8. §5's six non-goals are each rejected fail-loud or banked: items 1–2 by T4's validator, item 3 by PV-5, item 4 by CF-115-3, item 5 by CF-115-4, item 6 by the validator's own message. Correction 4's stat surface adds T5 and T7, which no SPEC deliverable names; `ADR-0201` records that addition. Task 1 is a rider and maps to no deliverable, by design.

**Placeholder scan.** No `TBD`, no "similar to Task N", no "add appropriate validation". Every code step carries literal code; the three one-line edits Task 4 describes in prose (`expected_name`'s arm, the `validate_hcm` argument/parameter, the two instance dispatch arms) are fully specified in the sentence that names them.

**Type consistency.** `HealthCheckFilterConfig`, `HealthCheckFilter::build_from_config(cfg, registry, hcm_stat_prefix)`, `HEALTH_CHECK_OK`, `X_ENVOY_UPSTREAM_HEALTHCHECKED_CLUSTER`, `local_cluster`, `UnsupportedHealthCheckField { listener, field }`, `UnsupportedHealthCheckPseudoHeader { listener, name }`, `FilterResponse::details` and `h2_response_code_details_line(pipeline, uri) -> (HeaderMap, String, u64)` are spelled identically in every task that names them, and each is defined no later than its first consumer.

⚠ **What this plan does NOT claim.** The slice-level claims (every gate green on the whole slice, both fixtures green, V1–V5 red) were measured on the whole-slice tree. The boundary-level claims (which gate is green at which task) were measured on the per-task tree and are stated per task. Nothing here asserts a behaviour on a tree that was not built.
