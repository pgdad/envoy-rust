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
