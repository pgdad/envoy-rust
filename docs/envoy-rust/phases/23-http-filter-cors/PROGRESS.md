# Phase 23 (`23-http-filter-cors`) — PROGRESS

> Running execution log. The state-2 PLAN-write landed this skeleton + the Task-1 preamble (the §6.2 empirical-verification transcript + the PLAN-time SPEC corrections) in one standalone pre-Task-1 commit (the 06.2→22 cadence). State-3 execution appends one entry per completed task (`superpowers:subagent-driven-development`, SERIAL dispatch).

## Task ledger

| Task | Deliverable | Status |
|---|---|---|
| 1 | D1 — per-route `typed_per_filter_config` schema (`PerFilterConfig`/`CorsPolicy`/`CorsConfig` + `Route` de/serializer) | OPEN |
| 2 | D3 — `PerRouteConfigForAbsentFilter` validator (L7 stricter-reject) | OPEN |
| 3 | D4+D7 — `CorsFilter` runtime + 2 stats + BEHAVIOR_CONTRACT rows | OPEN |
| 4 | D5 — `HttpFilterInstance::Cors` + dispatch + `FilterPipeline::apply_route_config` | OPEN |
| 5 | D2 — H1 HCM route-early-resolution + threading (`resolve_route`) | OPEN |
| 6 | D2 — H2 HCM symmetric threading | OPEN |
| 7 | D8.1 — fixture `0031-http-filter-cors` + Docker wrapper | OPEN |
| 8 | D8.2 — `parse_bootstrap` fuzz seed (no new target) | OPEN |
| 9 | D8.3 — in-process backstop | OPEN |
| 10 | State-4 verification + STATE advance | OPEN |

---

## Task 1 preamble — §6.2 empirical verification + PLAN-time SPEC corrections

### §6.2 empirical verification (LOCAL Docker, `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd…`, 2026-06-09)

Ran upstream Envoy v1.33.0 locally under Docker (CORS has no virtiofs/inotify dependency — the phase-22 §6.2-local methodology). The canonical bootstrap: H1 listener + HCM with a `cors`+`router` filter chain + a route carrying a `CorsPolicy` via `typed_per_filter_config`. **The probe transcript below is the source of the PLAN "LOCKED-IN findings (L1–L7)".**

**Item 1 — config shape (CONFIRMED, gates D1).** Envoy accepted, with no error:
- filter-chain entry: `name: envoy.filters.http.cors`, `typed_config."@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors` (no fields).
- per-route: `typed_per_filter_config: { envoy.filters.http.cors: { "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy, allow_origin_string_match: [ { exact: "http://allowed.example.com" } ], allow_methods: "GET, POST, OPTIONS", allow_headers: "x-custom-header, content-type", expose_headers: "x-exposed-header", max_age: "3600", allow_credentials: true } }`. `max_age` is a **string**. `allow_origin_string_match` elements are `StringMatcher`.

**Item 2 — preflight (CONFIRMED 200, NOT 204).** `OPTIONS /` + `Origin: http://allowed.example.com` + `Access-Control-Request-Method: GET` →
```
HTTP/1.1 200 OK
access-control-allow-origin: http://allowed.example.com
access-control-allow-credentials: true
access-control-allow-methods: GET, POST, OPTIONS
access-control-allow-headers: x-custom-header, content-type
access-control-max-age: 3600
access-control-expose-headers: x-exposed-header
date: …
server: envoy
content-length: 0
```
`access-control-allow-origin` **echoes the request Origin** (not `*`). Minimal policy (only `allow_origin_string_match` + `allow_methods`) → preflight emits ONLY `access-control-allow-origin` + `access-control-allow-methods` (each header conditional on its config field). No `vary` header. **Preflight detection = `OPTIONS` ∧ `Origin` present ∧ `Access-Control-Request-Method` present**: an `OPTIONS`+`Origin` WITHOUT `Access-Control-Request-Method` returned 200 + `content-length: 3` + the upstream `ok` body + `access-control-allow-origin` — i.e. it was treated as an **actual request** (proxied + decorated), NOT short-circuited.

**Item 3 — actual-request decoration (CONFIRMED).** `GET /` + `Origin: http://allowed.example.com` → 200 + the upstream body, with `access-control-allow-origin: http://allowed.example.com` + `access-control-allow-credentials: true` + `access-control-expose-headers: x-exposed-header` added (NOT methods/headers/max-age). Minimal policy → only `access-control-allow-origin`.

**Item 4 — disallowed / no-origin (CONFIRMED).** `GET /` + `Origin: http://evil.example.com` → 200, NO `access-control-*`. `GET /` (no Origin) → 200, NO `access-control-*`. A disallowed-origin **preflight** (`OPTIONS` + evil Origin + ACRM) was NOT short-circuited — it proxied to the upstream (returned `ok`, no CORS headers).

**Item 5 — stat namespace (CONFIRMED).** `/stats` showed `http.ingress_http.cors.origin_valid` + `http.ingress_http.cors.origin_invalid` (HCM-prefixed, like rbac/fault/jwt_authn). A controlled sequence {allowed-preflight, allowed-GET, disallowed-GET, disallowed-preflight, no-origin-GET} → `origin_valid: 2`, `origin_invalid: 2`. So **each request with a present origin increments exactly one counter** (valid if matched, invalid if present-but-unmatched), preflight or actual; no-origin → neither.

**Item 6 — policy-for-absent-filter (DIVERGENCE).** A `CorsPolicy` on a route with NO `cors` filter in the chain → Envoy **accepts-and-ignores** (process stayed up; `GET` with allowed origin returned 200 with NO `access-control-*`; policy silently dropped). envoy-rust will diverge to **stricter all-fatal reject** (`ConfigError::PerRouteConfigForAbsentFilter`) per ADR-0049 / ADR-0054 item-6a precedent → recorded in BEHAVIOR_CONTRACT, backstop-only (the fixture has the filter present).

**THE CRITICAL TOPOLOGY FINDING (DIVERGENCE).** The FIRST verification attempt used a `direct_response` route with a route-level CorsPolicy: **CORS did not engage at all** (origin_valid stayed 0, no headers, preflight not short-circuited). Switching the route to `route: { cluster: backend }` (a real upstream) made route-level `typed_per_filter_config` work fully (preflight short-circuit + decoration). **Conclusion: Envoy's CORS filter requires a real upstream `RouteEntry` — it does NOT engage on `direct_response` routes.** The SPEC §3 D8.1 "(or a `direct_response` …)" option is INVALID; fixture 0031 must proxy to a cluster (the `http1-echo-server` helper, 0008/0030 pattern).

→ **ADR-0058 fires** at this PLAN-write commit for the two material divergences (the fixture-topology correction + the policy-for-absent-filter disposition). **ADR-0059 (split) does NOT fire** (single-phase, §6.1).

### PLAN-time SPEC corrections (read-only recon at HEAD `d5a8d0088`)

- **SC1** — `Route` has a HAND-ROLLED `Deserialize` (`bootstrap.rs:1323`) + `Serialize` (`:1403`); the SPEC's `#[serde(default)] pub typed_per_filter_config` snippet is wrong. D1 extends both hand-rolled impls (new map arm + unknown-field allow-list update + serialize-when-non-empty).
- **SC2** — `header_ci` is PRIVATE to `jwt_authn.rs:156`; duplicate the 5-line helper into `cors.rs` (the shared-util extraction is the standing-deferred M18-9/M21-3 item — do NOT do it here).
- **SC3** — `StringMatcher` (hand-rolled `Deserialize`; `matches()` at `matcher.rs:58`) reused verbatim for `allow_origin_string_match`; no changes.
- **SC4** — `HttpFilterTypedConfig` has 6 variants (`bootstrap.rs:741-765`); D5 adds the 7th (`Cors`). `HttpFilterInstance` (`instance.rs:30-61`) + its build/decode/encode dispatch (`:87-148`) gain Cors arms.
- **SC5** — synth decorators: H1 `decorate_filter_synth_response(resp, close)` (`hcm.rs:1407`), H2 `decorate_filter_synth_response_h2(resp)` (`response.rs:62`); the preflight `StopAndSend` 200 flows through them unchanged via the existing `(H2)RequestPath::SynthFromDecode` arms.
- **SC6** — H1 decode flow: pipeline clone `hcm.rs:600`, `mem::take` filter_req `:606`, decode `:612`, `build_response(&config,&req,close)` `:639`. H2: clone `:457`, decode `:465`, `build_response(&config.inner,&envoy_req,false)` `:483`. The `resolve_route` + `apply_route_config` calls go AFTER the clone, BEFORE the `mem::take`.

### Anchors confirmed accurate (recon)

`Route` `bootstrap.rs:1152`; `VirtualHost` `:1141`; `RouteConfiguration` `:1124`; `StringMatcher` `:1732`; `HttpFilterTypedConfig` `:741`; `HttpFilterInstance` `instance.rs:30`; build/decode/encode dispatch `:87/:116/:133`; `Decision`/`FilterResponse` `pipeline.rs:11`/`types.rs:42`; `FilterRequest` `types.rs:28`; Router decode no-op `router.rs:28`; `header_mutation` encode push `header_mutation.rs:74-128`; fault stats `register_counter` `fault.rs:44`; the 0030 fixture + wrapper + backstop layout; the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array `bootstrap.rs:~4387`; `RouteAction`/`RouteAction_Route` + `build_response` action dispatch.

---

_(execution entries appended below per task during state-3)_

---

## Task 1 — D1 per-route `typed_per_filter_config` schema — DONE (code commit `525346d05`)

**Deliverable:** `PerFilterConfig` (`@type`-tagged enum, sole `Cors(CorsPolicy)` variant) + `CorsPolicy` + near-empty `CorsConfig` in `envoy-config`, plus the `Route.typed_per_filter_config: BTreeMap<String, PerFilterConfig>` field wired through the HAND-ROLLED `Route` de/serializer (SC1).

**TDD:** 5 failing parse tests written first (`route_parses_typed_per_filter_config_cors`, `route_without_typed_per_filter_config_defaults_empty`, `cors_policy_rejects_unknown_field`, `route_rejects_unknown_top_level_key`, `cors_config_filter_chain_entry_is_near_empty`) → implemented → all green. 403 `envoy-config` tests pass; clippy + fmt clean.

**Files:** `crates/envoy-config/src/bootstrap.rs` (3 new types near `RouterConfig`; `Route` field; hand-rolled `visit_map` arm + duplicate-field guard + 4-key unknown-field allow-list `["match","direct_response","route","typed_per_filter_config"]`; serializer emits the map only when non-empty so existing fixtures round-trip byte-identical), `crates/envoy-config/src/lib.rs` (re-export `CorsConfig`/`CorsPolicy`/`PerFilterConfig`). **Mechanically-required fan-out:** the new non-defaulted `Route` field forced 36 struct-literal construction-site updates (`typed_per_filter_config: Default::default()`) — 26 in `crates/envoy-http1/src/hcm.rs`, 10 in `crates/envoy-http2/src/hcm.rs`; purely mechanical, no behavioral change (all 4 files committed together in `525346d05`).

**Notes / deviations:**
- **SC1 honored** — both hand-rolled impls extended (NOT replaced by derive); `Route` retains `#[derive(Debug, Clone, PartialEq)]` only. Verified by spec-compliance review.
- **`PerFilterConfig`/`CorsPolicy`/`CorsConfig` all derive `Serialize` too** (PLAN Step-6 NOTE chosen approach) — required so the serializer's `serialize_entry("typed_per_filter_config", …)` compiles; `#[serde(tag="@type")]` round-trips.
- **`cors_config_filter_chain_entry_is_near_empty` test adjusted** to `let _c = CorsConfig::default();` — a bare `CorsConfig` carries `deny_unknown_fields`, so directly deserializing `"@type": …`-tagged YAML into it would (correctly) reject the `@type` key; in production the `@type` is consumed by the enclosing `HttpFilterTypedConfig` `#[serde(tag="@type")]` (added Task 4), never reaching the inner struct. Documented inline.
- **SC3 honored** — `StringMatcher` reused verbatim (zero diff lines); `allow_origin_string_match: Vec<StringMatcher>`.

**Review:** spec-compliance review ✅ SPEC COMPLIANT (independently verified: exact derives/serde attrs, `max_age: Option<String>`, no stub variants, hand-rolled impls intact, conditional serialize length, mechanical fan-out, 403 tests green). Code-quality review reserved for the substantive tasks (3/4/5/6) per the PLAN execution-handoff.

---

## Task 2 — D3 `PerRouteConfigForAbsentFilter` validator (L7 stricter-reject) — DONE (code commit `f22578e19`)

**Deliverable:** new `ConfigError::PerRouteConfigForAbsentFilter { filter: String }` (`crates/envoy-config/src/lib.rs:531`) + a startup-fatal validator in `validate_hcm` (`crates/envoy-config/src/bootstrap.rs:~2693`) that collects the HCM's present filter names (`http_filters[].name`) and rejects any route `typed_per_filter_config` key not in that set. The ADR-0058 L7 divergence: Envoy accepts-and-ignores; envoy-rust all-fatal-rejects (ADR-0049 posture / ADR-0054 item-6a precedent).

**Placement (verified by spec review):** co-located with the existing `UnknownCluster` cluster-reference check, inside the per-route loop, AFTER the `route_config.is_none()` early-return gate. **Merge-ordering correctness traced:** for inline HCMs `route_config` is `Some` → runs immediately; for `rds`-configured HCMs `route_config` is `None` at parse (skips, no false positive), then `load_dynamic_resources` populates it and re-invokes `bootstrap::validate()` → `validate_hcm` post-merge (`lib.rs:~900`), so the check sees the EFFECTIVE merged route table. Inherits the same merge guarantees as `UnknownCluster`.

**TDD:** negative test `cors_per_route_config_without_cors_filter_is_fatal` (CorsPolicy on a route, only `router` in chain → `PerRouteConfigForAbsentFilter{filter:"envoy.filters.http.cors"}`) PASSES now; regression test `empty_typed_per_filter_config_does_not_trigger_validator` PASSES (guards all existing fixtures). 405 passed / 1 ignored; clippy + fmt + `cargo build --workspace` clean.

**Notes / deviations:**
- **`StringMatcher` structural validity** (D3 second half) is enforced automatically by `StringMatcher`'s hand-rolled `Deserialize` — no extra validator code (confirmed).
- **⚠ Task-4 carryover:** the positive test `cors_per_route_config_with_cors_filter_present_parses` is `#[ignore = "needs HttpFilterTypedConfig::Cors from Task 4"]` — a `cors` filter-chain entry can't parse until Task 4 registers `HttpFilterTypedConfig::Cors`. **Task 4 must un-ignore this test** and confirm it passes.

**Review:** spec-compliance review ✅ SPEC COMPLIANT (placement/merge-ordering traced, logic + variant style + tests verified, +213 lines purely additive, 405 tests green). Code-quality review reserved for the substantive tasks per the PLAN execution-handoff.

---

## Task 3 — D4+D7 `CorsFilter` runtime + 2 stats + BEHAVIOR_CONTRACT (UNWIRED) — DONE (code commit `ddb4cb87c`)

**Deliverable:** new `crates/envoy-filter/src/cors.rs` — `CorsFilter` (origin allow-match via reused `StringMatcher`, decode-side preflight short-circuit, encode-side actual-request decoration, 2 stats) + `CompiledCorsPolicy` lowering. Re-exported `pub use cors::CorsFilter;` from `lib.rs`. BEHAVIOR_CONTRACT.md gains the 23-entries CORS block. **Not yet wired into `HttpFilterInstance`/pipeline — that is Task 4.**

**Recon findings (confirmed against codebase, for downstream tasks):** `Counter` read accessor is `.value()`; `register_counter` → `Result<Arc<Counter>, _>`; `FilterRequest.body: Option<Bytes>`, `FilterResponse.body: Bytes` (NOT Option); `Decision::Continue` / `Decision::StopAndSend(FilterResponse)`; `FilterError::InvalidConfig { message: String }`; `StringMatcher.mode` + `.ignore_case` both `pub` (test uses `StringMatcher { mode: StringMatcherMode::Exact(..), ignore_case: false }`).

**Verified semantics implemented exactly (L2–L5):** 3-condition preflight detection (`OPTIONS` ∧ origin ∧ `access-control-request-method`); allowed preflight → `StopAndSend{200, empty body, 6 conditional headers in verified order}`; disallowed/no-ACRM → Continue; encode adds only allow-origin + allow-credentials(if) + expose-headers(if), never methods/headers/max-age; stats tick once per present origin (valid/invalid) BEFORE the short-circuit, no-origin ticks neither; `active_policy: None` → fully inert. `header_ci` duplicated locally per SC2.

**TDD:** 12 unit tests (10 + 2 added in code-quality fix), all green. clippy + fmt + `cargo build --workspace` clean.

**Reviews (two-stage, substantive task):**
- Spec-compliance ✅ SPEC COMPLIANT — all 6 verified-semantics points checked against code (preflight detection, 200/header-order, disallowed→Continue, encode-only-3, stats-both-paths, inert-None); BEHAVIOR_CONTRACT rows accurate.
- Code-quality → **Approve-with-minor-fixes**, all applied + commit amended (`aa3b7219f`→`ddb4cb87c`): **I-1** redundant double-clone in `encode_headers` removed; **M-1** `encode_headers` doc corrected (only allow-origin unconditional); **M-3** test `apply_route_config_route_without_cors_key_is_none`; **M-4** test `minimal_policy_encode_emits_only_allow_origin`.

**⚠ Task-4 carryovers (from code-quality M-5 / M-2):**
1. **Remove the module-level `#![allow(dead_code)]`** in `cors.rs` once Task 4 wires `CorsFilter` into `HttpFilterInstance` (it currently masks the unused filter; with the filter live it should be unnecessary — confirm clippy stays clean after removal).
2. **`irrefutable_let_patterns` lint:** `apply_route_config` has `let envoy_config::PerFilterConfig::Cors(p) = pfc;` (single-variant enum → irrefutable). It does not fire today (dead-code-suppressed) but WILL once the path is live. Fix at Task 4 with `let-else`/`if-let` + `unreachable!()`, or equivalent, so wiring lands clippy-clean.

---

## Task 4 — D5 `HttpFilterInstance::Cors` variant + dispatch + `FilterPipeline::apply_route_config` fan-out — DONE (code commit `08e4b6f92`)

**Deliverable:** wired the (Task-3) `CorsFilter` into the filter framework. `HttpFilterTypedConfig::Cors(CorsConfig)` (7th variant, `@type .../cors.v3.Cors`, `bootstrap.rs`) + `HttpFilterInstance::Cors(CorsFilter)` (7th variant) with build/decode/encode dispatch arms + a `pub(crate) apply_route_config` (no-op for the 6 non-Cors variants, delegates to `CorsFilter` for Cors) (`instance.rs`) + `FilterPipeline::apply_route_config` fan-out over `filters.iter_mut()` (`pipeline.rs`).

**Recon (for downstream tasks):** `FilterPipeline::build_from_config(&[HttpFilter], &Arc<StatsRegistry>, &str)`; `FilterPipeline.filters: Vec<HttpFilterInstance>`; `encode_headers(&mut self, resp_arg: &mut FilterResponse)`. The ONLY production exhaustive `match` over `HttpFilterTypedConfig` is `validate_http_filters` (`bootstrap.rs:2803`) — got a faithful `Cors` arm (name-consistency gate → `UnsupportedHttpFilter`; `CorsConfig` is empty so no `validate_*` call, matching the sibling pattern). `cargo build --workspace --all-targets` clean → all exhaustive matches covered (the phase-22 compile-time risk fully closed).

**Carryovers resolved (Task 3 → Task 4):** removed the module-level `#![allow(dead_code)]` from `cors.rs` (filter now live; build/clippy stay clean without it); fixed the `irrefutable_let_patterns` lint via `match pfc { PerFilterConfig::Cors(p) => … }`; un-ignored the Task-2 positive test `cors_per_route_config_with_cors_filter_present_parses` (now PASSES — the cors filter-chain entry parses).

**TDD:** new tests `cors_filter_chain_entry_parses_to_cors_variant` (bootstrap), `apply_route_config_then_preflight_short_circuits` + `apply_route_config_none_leaves_cors_inert` (pipeline — build a real cors+router pipeline, drive a preflight through the fan-out, assert short-circuit vs inert-None). envoy-config 407 / envoy-filter 96 green. WORKSPACE: `cargo build --workspace --all-targets` clean; `cargo clippy --workspace … -D warnings` clean; fmt clean.

**HCM/bin runtime verification (the D5 workspace-test gate) + a debugging detour:** `cargo test -p envoy-http1 -p envoy-http2 -p envoy-bin` initially FAILED one test — `upstream_h2_connection_pooling` (envoy-bin backstop, fixture-0021 sibling) at `upstream_h2_connection_pooling.rs:296` (`wait_ready(backend, 30s).expect("backend ready")`), reproducible even in isolation. **Root-caused via `superpowers:systematic-debugging` to the `project_flaky_access_log_fixture_0012` pre-build discipline, NOT a Task-4 regression:** `spawn_backend` runs `cargo run --manifest-path tests/helpers/http2-echo-server/Cargo.toml`; the helper was NOT pre-built, and its cold compile takes **3m42s** — far over the in-test 30s readiness budget. After `cargo build --manifest-path tests/helpers/http2-echo-server/Cargo.toml`, the test passes in **2.06s**. The failure is in backend-helper readiness, entirely orthogonal to the CORS variant wiring (which touches only envoy-config/envoy-filter). **Controller note for Tasks 5/6/7/9:** pre-build ALL `tests/helpers/*` BEFORE any `cargo test --workspace` / backstop / differential run (the helpers' cold compiles otherwise blow the in-test readiness budgets).

**Reviews (two-stage, substantive task):**
- Spec-compliance ✅ SPEC COMPLIANT — dispatch parity with the JwtAuthn sibling, fan-out reaches all filters, the `validate_http_filters` arm faithful (not over/under-built), all carryovers verified, 0 ignored tests.
- Code-quality ✅ (controller-read diff) — clean idiomatic wiring; the `match pfc` fix correct; the pipeline tests genuinely exercise the fan-out. No issues.

---

## Task 5 — D2 H1 HCM route-early-resolution + per-route-config threading — DONE (code commit `2852b9e7b`, amended)

**Deliverable:** new `pub fn resolve_route<'a>(config: &'a HCMConfig, req: &Request) -> Option<&'a envoy_config::Route>` in `crates/envoy-http1/src/hcm.rs` (resolves Host→vhost→route up-front, `None` on missing/empty Host / no vhost / no route) + threading in the H1 decode region: `let matched_route = resolve_route(&config, &req); pipeline.apply_route_config(matched_route);` placed AFTER the `(*config.filter_pipeline).clone()` and BEFORE the `mem::take` of `req` fields (SC6). `build_response(&config,&req,close)` UNCHANGED (re-matches internally; identical dispatch). `resolve_route` is `pub` for Task 6 H2 reuse.

**The mirror invariant (load-bearing, spec-reviewed):** `resolve_route` reuses the EXACT same matching helpers `build_response` uses — `find_header(&req.headers, headers::HOST).filter(|h| !h.is_empty())` → `strip_port` → `virtual_hosts.iter().find(|vh| vh_matches(vh, host))` → `routes.iter().find(|r| route_matches(r, &req.path, &req.headers))`. Verified line-by-line against `build_response`: identical host extraction, port strip, first-match vhost, first-match route. → the up-front resolution and `build_response`'s internal re-match select the SAME route (the 30-fixture regression-equivalence guarantee).

**⚠ LOAD-BEARING DEFECT FOUND IN REVIEW + FIXED (this commit):** `clone_route_config` (hcm.rs:215, the production hand-clone of `RouteConfiguration` since envoy-config's types aren't `Clone`) was setting `typed_per_filter_config: Default::default()` at line 239 — **silently DROPPING every route's CORS policy on the production config-load path** (a Task-1 mechanical-`Default::default()`-fan-out casualty; the struct literal was made to compile without cloning the source map). `resolve_route` reads `config.route_config` = the output of `clone_route_config`, so the policy would never reach the filter at runtime → fixture 0031 / the backstop would silently see no CORS. **Fixed:** line 239 → `typed_per_filter_config: r.typed_per_filter_config.clone()` (`PerFilterConfig` derives `Clone`). Confirmed it was the ONLY production occurrence (all 28 other `Default::default()` hits are in `mod tests`, ≥ line 1481). envoy-http2 has NO production drop (it reuses this `clone_route_config` via the shared `HCMConfig`). TDD: failing test `clone_route_config_preserves_typed_per_filter_config` (asserts the cloned route keeps the `envoy.filters.http.cors` key) → fix → pass.

**TDD:** `resolve_route` unit tests (matches-vh-and-route, none-on-empty-host, none-on-no-route-match, strips-port-from-host) + the `clone_route_config_preserves_typed_per_filter_config` regression test. `cargo test -p envoy-http1` = 104 passed / 0 failed. `cargo build --workspace --all-targets` clean; clippy `-D warnings` clean; fmt clean.

**Regression-equivalence (the prime obligation):** isolated `cargo test --workspace --exclude differential --exclude h2spec-conformance` → **25 suites OK, 1 environment artifact**: `upstream_h2_connection_pooling` (envoy-bin backstop) timed out at backend-readiness (30s) under `--workspace` due to NESTED-CARGO LOCK CONTENTION (the backstop shells out to `cargo run --manifest-path` for its helper, which contends with the parent workspace cargo's lock) — **passes standalone in 2.06s at this exact commit** (`cargo test -p envoy-bin --test upstream_h2_connection_pooling`), failure is backend-helper readiness (orthogonal to CORS/threading), corroborated by `project_flaky_access_log_fixture_0012`. NOT a regression. **Controller note for Tasks 6/9:** the per-task workspace regression check should treat the envoy-bin cargo-shelling backstops as standalone-verified (run them with `-p envoy-bin` alone), not inside `--workspace`, to avoid this nested-cargo artifact.

**Reviews:** spec-compliance ✅ SPEC COMPLIANT (the mirror invariant holds exactly; threading placement + borrow soundness confirmed; the `clone_route_config` drop surfaced HERE and fixed). Code-quality: clean (single-file, +193 lines + the 1-line clone fix; idiomatic; `build_response` untouched).
