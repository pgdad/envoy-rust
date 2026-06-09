# Phase 23 (`23-http-filter-cors`) — SPEC

- **Phase id:** `23`
- **Slug:** `23-http-filter-cors`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `bd3cfd644`, the phase-22 state-6 close-out commit; the "HTTP filters family" §9 heading carries four concrete rows — phase 09 `local_ratelimit` `done`, phase 10 `rbac` `done`, phase 11 `fault` `done`, phase 22 `jwt_authn` `done`). **This SPEC's landing commit adds the fifth concrete row beneath the HTTP-filter-family heading**, with `status: planned` (invariant 4.1.2 — a new row enters `planned`; it flips to `in-progress` only when the NEXT session's state-2 PLAN-write points STATE at it per invariant 4.1.3).
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"HTTP filters family — header manipulation, **cors**, compression, fault, local+global rate limit, jwt_authn, rbac, ext_authz, ext_proc, oauth2, csrf, buffer, lua, wasm, adaptive concurrency, admission control, bandwidth limit."* This phase lands `envoy.filters.http.cors` narrowed to the **minimum-viable per-route surface**: origin allow-matching via the existing `StringMatcher`, the CORS preflight short-circuit (decode-side `OPTIONS` → 200 + `access-control-*` headers), and the actual-request response decoration (encode-side `access-control-allow-origin` + friends). **Phase 23's structural centerpiece is NOT the filter — it is the per-route `typed_per_filter_config` infrastructure the filter is the first consumer of** (the deferred prerequisite called out in ADR-0055 / SPEC §0 as "the natural cors close site"). Filter-`enabled`/`shadow_enabled` runtime gating, `allow_private_network_access`, vhost-level config + the route>vhost precedence cascade, and `typed_per_filter_config` for filters OTHER than `cors` all defer per §4 below.
- **Position in the project:** the **fifth concrete HTTP-filter-family phase** (after phase-09 `local_ratelimit`, phase-10 `rbac`, phase-11 `fault`, phase-22 `jwt_authn`). The MVP trunk 00→08, the upstream-robustness family (12→17, complete in minimum-viable form), the xDS / dynamic-config filesystem-transport quartet (18 CDS / 19 LDS / 20 RDS / 21 EDS), and the four prior HTTP filters all stand closed as of HEAD `bd3cfd644`. Phase 23 amortizes the framework investment of phases 07/09/10/11/22 (the `Decision::StopAndSend(FilterResponse)` decode-side short-circuit; the phase-07.2 `header_mutation` **encode-side** response-header-mutation precedent — the first relevant precedent for an encode-side filter; the per-filter `StatsRegistry` counter-wiring pattern; the `04.x` `StringMatcher` reuse) **and is the first phase in the project to build per-route (route-scoped) filter configuration** — which is what makes it the gating unlock for `cors` AND the future per-route-configured filters (`csrf`, `buffer`, and per-route overrides of the existing `fault`/`rbac`/`local_ratelimit`).
- **depends-on:** `07` (the parent filter-chain framework) `04` (HCM + route matching — the per-route-config infrastructure threads the matched route into the filter pipeline). Phase 23 extends the 07.1-landed `envoy-filter::FilterPipeline` + `HttpFilterInstance` enum with a seventh production variant (after `Router` at 07.1, `HeaderMutation` at 07.2, `LocalRateLimit` at 09, `Rbac` at 10, `Fault` at 11, `JwtAuthn` at 22) AND changes the HCM request-processing ordering so the matched route is resolved before / available to the filter pipeline (§0). Implicit (non-`depends-on`-field) dependencies: phase `05` (the H2 codec — the per-route-config threading lands symmetrically on H1 + H2 even though the phase-23 differential fixture is H1), phase `06` (the `StatsRegistry` + admin `/stats` surface the `cors.*` counters land on). The 30-Docker-gated-fixture regression baseline established at phase-22 close (`0001-tcp-echo` through `0030-http-filter-jwt-authn`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **Brainstorm narrative:** see the "Phase-23 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + filter-pick rationale with the alternatives considered (jwt_authn combinators / compression / load-balancing / xDS-watching / network-filters / SDS / gRPC) along the established scoring axes, and ADR-0057 for the scoping decision.

---

## 0. Critical scoping findings (READ FIRST) — CORS needs per-route config, AND the HCM resolves the route AFTER the filter pipeline runs today

Three findings (established by a read-only reconnaissance at the brainstorm HEAD `bd3cfd644`) shaped the phase pick + scope and MUST anchor the PLAN-write:

1. **CORS is genuinely per-route in modern Envoy — its policy lives in `typed_per_filter_config`, not the filter-chain entry.** Upstream Envoy v1.33's `cors` HTTP filter takes an essentially-empty filter-chain-level `Cors` message (`type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors` — only `filter_enabled`/`shadow_enabled` runtime knobs, both deferred §4); the actual policy (`CorsPolicy`: `allow_origin_string_match`, `allow_methods`, …) is attached **per route / per virtual_host via `typed_per_filter_config`**. This is load-bearing: **per-route `typed_per_filter_config` does not exist in envoy-config today.** `Route` (`crates/envoy-config/src/bootstrap.rs:1152-1158`), `VirtualHost` (`:1141-1149`), and `RouteConfiguration` (`:1124-1137`) carry zero per-filter config; `typed_per_filter_config`/`per_filter_config` are grep-empty workspace-wide. Phase 22 chose `jwt_authn` precisely BECAUSE it self-matches via its own `rules[]` and sidesteps this; phase 23 takes on the deferred infrastructure head-on, with `cors` as its first (and, this phase, only) consumer.

2. **The HCM runs the filter pipeline BEFORE it resolves the matched route.** Today the H1 HCM calls `pipeline.decode_headers(&mut filter_req)` at `crates/envoy-http1/src/hcm.rs:612`, and only AFTER a `Decision::Continue` does `build_response(&config, &req, …)` (`:1163-1228`) match the virtual_host (`:1180-1184`) then the route (`:1199-1202`). The H2 HCM mirrors this (`crates/envoy-http2/src/hcm.rs:465-486` → its `build_response`). **So a per-route-config filter cannot see its matched route during `decode_headers` today** — the route is not yet known. This is the cross-cutting architectural change phase 23 must make: **resolve the matched route up-front and thread it (and its per-filter config) into the filter pipeline** so the CORS filter can read its `CorsPolicy` on both the decode (preflight) and encode (actual-request) sides. Crucially, this is the architectural lift ADR-0055 warned about when deferring `cors` — it is real, but bounded, and it is a strictly-better ordering (Envoy itself resolves the route early and exposes it to filters).

3. **The route-early-resolution change is regression-safe (the 07.1 foundation-slice property).** No existing filter reads the matched route — the Router terminus is a decode-side no-op (`crates/envoy-filter/src/router.rs:28-34`) and the route action is dispatched inside `build_response`, which can REUSE an already-resolved route rather than re-matching. Therefore resolving the route earlier and merely making it AVAILABLE to the pipeline changes NO existing behavior: all 30 pre-existing fixtures stay green (regression-equivalence, the 05.1/07.1/12.1/14.1 foundation-slice pattern). This property is what makes the §6.1 split clean if the PLAN-write fires it (§6.1).

---

## 1. Goal and acceptance signal

Phase 23 lands the `envoy.filters.http.cors` filter (typed_config `@type = type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors`) as the **seventh `HttpFilterInstance` variant** (after Router, HeaderMutation, LocalRateLimit, Rbac, Fault, JwtAuthn) and the **sixth concrete pluggable feature filter** — together with **the per-route `typed_per_filter_config` infrastructure** it is the first consumer of. The filter reads a `CorsPolicy` attached to the matched route via `typed_per_filter_config` and:

- **Preflight (decode-side):** a request whose method is `OPTIONS` carrying both an `origin` header and an `access-control-request-method` header, where `origin` matches the policy's `allow_origin_string_match`, is short-circuited via `Decision::StopAndSend(FilterResponse)` with HTTP 200 (§6.2-verified status) + the `access-control-allow-origin` / `access-control-allow-methods` / `access-control-allow-headers` / `access-control-max-age` (+ optional `access-control-allow-credentials`) headers and an empty body, decorated by the existing H1/phase-11-H2 filter-synth helpers.
- **Actual request (encode-side):** a non-preflight request carrying an `origin` that matches the policy proceeds (`Decision::Continue`) and, on the response, has `access-control-allow-origin` (+ optional `access-control-expose-headers` / `access-control-allow-credentials`) added in `encode_headers` (the phase-07.2 `header_mutation` encode-side precedent).
- **Origin not allowed / no origin / no policy on the route:** pass through unchanged (no CORS headers).

**Differential surface added by phase 23:**

- **Fixture `0031-http-filter-cors`** — bilateral assertion that both proxies, given an identical bootstrap with the `cors` filter in the HTTP filter chain and a `CorsPolicy` attached to a route via `typed_per_filter_config` (one allowed origin via `StringMatcher` exact-match, an `allow_methods` set, an `allow_headers` set, a `max_age`), produce deterministic per-probe results on a multi-probe burst over an **HTTP/1.1** listener: probe 1 (a preflight `OPTIONS /` with `Origin: <allowed>` + `Access-Control-Request-Method: GET`) → 200 + the `access-control-*` header set (byte-exact, §6.2); probe 2 (a `GET /` with `Origin: <allowed>`) → the proxied/`direct_response` 200 + `access-control-allow-origin: <allowed>` on the response; probe 3 (a `GET /` with `Origin: <disallowed>`) → the proxied 200 with **NO** `access-control-allow-origin`; probe 4 (a `GET /` with no `Origin`) → the proxied 200, unchanged. The exact preflight status, the exact `access-control-*` header names + values + formatting (comma-separated method/header lists; whether allow-origin echoes the request origin or emits `*`), the `cors` stat namespace, and the disallowed-origin disposition are **empirically verified at state-2 PLAN-write per §6.2** (the phase-10/11/22-ratified verify-at-PLAN-write discipline — header bytes are NOT assumed).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0031-http-filter-cors` green at Docker-gated CI.
- **(b)** All **30 pre-existing differential fixtures** (`0001-tcp-echo` through `0030-http-filter-jwt-authn`) **remain green simultaneously** at the same CI run (regression-equivalence per `BOOTSTRAP_PROMPT.md` §7.5 (b)) — load-bearing this phase because the route-early-resolution change (§0 finding 2) touches the shared HCM request-processing path of EVERY HTTP fixture.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline). The per-route-config threading lands on both H1 + H2 HCMs (the change is codec-shared); the state-4 verification re-runs `h2spec` to confirm no H2-framing regression.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (one new seed for the cors-filter + `typed_per_filter_config` bootstrap shape; curated seed corpus extends by one). No new parser/codec crate is introduced (CORS reuses `StringMatcher`), so per `BOOTSTRAP_PROMPT.md` §7.4 **no new fuzz target is required** — the new untrusted-input surface (the `typed_per_filter_config` map + `CorsPolicy` YAML) is covered by the existing `parse_bootstrap` target via the new seed. The PLAN-writer confirms at §6.2.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean. **No new crate and no new dependency this phase** (CORS is pure-Rust header manipulation; no crypto, no codec, no external service) — `cargo deny check` is a no-op-delta. The standalone-crate builds (`project_isolated_crate_build_blindspot`) at state-4 cover the touched crates (`envoy-config`, `envoy-filter`, `envoy-http1`, `envoy-http2`).
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent through 06.1 / 07.x / … / 22 — fixture inheritance is a regression vector, and never more so than this phase given the shared-HCM-path change).

---

## 2. Behavior-contract scope for phase 23

Phase 23 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x → 22 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — 2 new rows (projected; §6.2-verified)

Upstream Envoy's `cors` filter emits per-route CORS stats — at minimum `cors.origin_valid` and `cors.origin_invalid` counters (the exact rooting — whether under the HCM stat prefix like the other filters, a top-level `cors.*`, or a per-vhost scope — is **§6.2-verified**, since CORS's upstream stat rooting historically differs from the HCM-prefixed filters):

| Stat name | Equivalence | Rationale |
|---|---|---|
| `<root>.cors.origin_valid` (root §6.2-verified) | value-exact | Counter; one increment per request with an allowed `origin`. |
| `<root>.cors.origin_invalid` (root §6.2-verified) | value-exact | Counter; one increment per request with a present-but-disallowed `origin`. |

The differential fixture 0031 does **not** scrape `cors` stats (it asserts the per-probe status + `access-control-*` header set); the counters are exercised by the in-process backstop + unit tests. This mirrors the phase-10/11/22 posture. The §2.1 stat rows + namespace are §6.2-verified at PLAN-write.

### 2.2 "Header allow-list" extension — the `access-control-*` response headers (§6.2-verified)

The CORS response headers (`access-control-allow-origin`, `access-control-allow-methods`, `access-control-allow-headers`, `access-control-expose-headers`, `access-control-max-age`, `access-control-allow-credentials`) are emitted **identically by both proxies** when configured with the same `CorsPolicy` and driven with the same `Origin` (they are a pure function of the policy + the request origin). They are therefore asserted **value-exact** under the existing `set_equal_modulo_allow_list` header discipline — **no allow-list row** (they are not implementation-identifying). The byte-exact values + formatting (the comma-separated `allow_methods`/`allow_headers` list rendering; whether `access-control-allow-origin` echoes the request `Origin` or emits `*`; the `access-control-max-age` integer rendering) are **§6.2-verified** (the phase-10 RBAC-body-off-by-one-byte lesson makes assumption-free capture load-bearing). A "Response headers — cors `access-control-*`" subsection lands in BEHAVIOR_CONTRACT at the task that first exercises them.

### 2.3 "Preflight local reply" extension — status + body (§6.2-verified)

Envoy's CORS preflight short-circuit returns a status (projected 200 — §6.2-verified, could be 204) with an empty body and the `access-control-*` headers. The exact status + the presence/absence of a body + any `content-length: 0` are §6.2-verified and recorded in BEHAVIOR_CONTRACT at the exercising task.

### 2.4 DECISIONS.md — ADR-0057 lands at THIS SPEC commit; ADR-0058 / ADR-0059 reserved

Phase 23 lands **ADR-0057** at THIS brainstorm commit — the scope lock + the per-route-config-infrastructure decision (the HCM route-early-resolution change + the `typed_per_filter_config` design) is a genuine architectural decision per D-3.5, mirroring the xDS-family / phase-22 brainstorm-ADR cadence. **ADR-0058 is reserved** for the §6.2 empirical-verification reconciliation at PLAN-write (most-likely trigger: the preflight status/headers / the allow-origin echo-vs-`*` / the stat namespace / the exact Envoy config shape). **ADR-0059 is reserved** for the §6.1 split (lands only if phase 23 splits into 23.1/23.2 — see §6.1).

---

## 3. Deliverables

Phase 23's scope is enumerated as deliverables `D1`–`D8`. **The state-2 PLAN-writer organizes deliverables into tasks** (and evaluates the §6.1 split gate) — these are not 1:1 with tasks. Listed in roughly execution order; the SPEC constrains the surface, not the task order.

### D1 — per-route `typed_per_filter_config` schema (the structural centerpiece)

At `crates/envoy-config/src/bootstrap.rs`, add a `typed_per_filter_config` field to `Route` (and the schema scaffolding for the future vhost-level — but **populate/consume only the route-level this phase**, §4). The field is a map keyed by filter name (`String`) → a typed per-filter config enum, mirroring the existing `@type`-tagged YAML idiom used by `HttpFilterTypedConfig` (`bootstrap.rs:741-765`) and the xDS-family envelopes (phases 18-21):

```rust
// On Route (bootstrap.rs:1152-1158):
#[serde(default)]
pub typed_per_filter_config: BTreeMap<String, PerFilterConfig>,

// New enum — the per-route counterpart to HttpFilterTypedConfig; only Cors registered this phase:
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "@type")]
pub enum PerFilterConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy")]
    Cors(CorsPolicy),
    // future: HeaderMutation, Fault, Rbac, LocalRateLimit per-route overrides (deferred §4)
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorsPolicy {
    pub allow_origin_string_match: Vec<StringMatcher>,   // REUSE 04.x StringMatcher (bootstrap.rs:1732)
    #[serde(default)] pub allow_methods: Option<String>,        // comma-separated, e.g. "GET, POST"
    #[serde(default)] pub allow_headers: Option<String>,
    #[serde(default)] pub expose_headers: Option<String>,
    #[serde(default)] pub max_age: Option<String>,             // seconds as a string per Envoy
    #[serde(default)] pub allow_credentials: Option<bool>,
    // DEFER (deny_unknown_fields rejects): filter_enabled, shadow_enabled,
    //   allow_private_network_access, forward_not_matching_preflights
}
```

All structs carry `#[serde(deny_unknown_fields)]` per the established envoy-config discipline (rejects the deferred forward-looking fields). The `PerFilterConfig` enum uses the `@type`-tagged pattern so future filters slot in additively. **The exact upstream `CorsPolicy` field names/types + the `@type` URI are §6.2-verified** (Envoy's `Cors` filter-entry shape vs the per-route `CorsPolicy` shape). The PLAN-writer confirms whether `StringMatcher` is reused verbatim (it should be — `bootstrap.rs:1732-1752` supports exact/prefix/suffix/safe_regex/contains, which matches `allow_origin_string_match`).

### D2 — the HCM route-early-resolution + per-route-config threading (the cross-cutting change)

The §0 finding 2 change. Both `crates/envoy-http1/src/hcm.rs` and `crates/envoy-http2/src/hcm.rs`: resolve the matched route (virtual_host + route, the `build_response` matching logic at H1 `:1180-1202`) **before / as part of** invoking the filter pipeline, and thread the matched route's `typed_per_filter_config` into the pipeline so each filter can read its per-route config on both decode + encode. `build_response` is refactored to REUSE the already-resolved route rather than re-matching (no double-match; identical dispatch). The threading mechanism (the PLAN-writer chooses the cleanest shape — e.g. a route-context handle on `FilterRequest`/the pipeline-invocation call, or a per-filter `set_route_config` step before `decode_headers`) MUST:

- be **inert for every filter that does not consume per-route config** (Router/HeaderMutation/LocalRateLimit/Rbac/Fault/JwtAuthn) — they ignore it; their behavior is byte-identical → regression-equivalence (§0 finding 3, the acceptance gate (b));
- handle the **no-route-matched** path (today's synth-404 at `build_response`) without invoking per-route config (a 404'd request has no route → no CORS policy);
- be symmetric across H1 + H2 (the pipeline abstraction is codec-agnostic per 07.1 / ADR-0031).

**Signpost (the load-bearing design question for the PLAN-write):** the cleanest threading is to resolve the route up-front, look up `route.typed_per_filter_config.get("envoy.filters.http.cors")`, and hand the resolved `Option<&CorsPolicy>` to the CORS filter instance before its decode/encode runs — WITHOUT widening every filter's method signature. The PLAN-writer designs this to minimize the blast radius (the regression surface is every HTTP fixture). A route-context field threaded through the existing `FilterRequest`/`FilterResponse` (or a pipeline-level "current route config" set once per request) is preferred over per-filter signature churn.

### D3 — `envoy-config` validator extension

At `crates/envoy-config/src/bootstrap.rs` route/filter validation, add per-route-config validation: each `typed_per_filter_config` key naming `cors` requires the `cors` filter to be present in the HTTP filter chain (a CORS policy on a route with no `cors` filter is operator error — `ConfigError::PerRouteConfigForAbsentFilter { filter }` or similar, §6.2/PLAN-writer's exact shape); each `CorsPolicy.allow_origin_string_match` entry is a structurally-valid `StringMatcher` (delegates to the existing `StringMatcher` validation). New `ConfigError` variants land here (consolidatable at the PLAN-writer's discretion), each with positive + negative unit tests, exercised by the `parse_bootstrap` fuzz target (the new seed per D8.2). The exact "policy-for-absent-filter" disposition (fatal-reject vs accept-and-ignore) is **§6.2-verified** against Envoy and recorded (ADR-0049's all-fatal config posture is the projected default).

### D4 — `envoy-filter::CorsFilter` runtime

New module `crates/envoy-filter/src/cors.rs`. Hand-rolled per D-3.2 (one module per concrete filter; the 07.2/09/10/11/22 precedent), consuming `envoy-config` for the `CorsPolicy`/`StringMatcher` + `envoy-stats` for the counters. Module shape:

```rust
pub struct CorsFilter {
    // the per-route policy is supplied per-request via the D2 threading, NOT held here;
    // the filter instance holds only the stat handles + the resolved-policy slot for this request.
    origin_valid: Arc<Counter>,
    origin_invalid: Arc<Counter>,
    active_policy: Option<CompiledCorsPolicy>,  // set by D2 threading before decode/encode
}

impl CorsFilter {
    pub(crate) fn build_from_config(
        cfg: &envoy_config::CorsConfig,        // the (near-empty) filter-chain Cors entry
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> { /* register origin_valid/origin_invalid counters */ }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        // 0. No active per-route policy → Decision::Continue (no CORS on this route).
        // 1. Read `origin` (header_ci); if absent → Continue.
        // 2. If origin matches allow_origin_string_match: origin_valid.inc(); else origin_invalid.inc().
        // 3. Preflight = method==OPTIONS && has `access-control-request-method`.
        //    If preflight && origin allowed → Decision::StopAndSend(FilterResponse {
        //       status: 200 (§6.2), headers: [access-control-allow-origin, -methods, -headers,
        //       -max-age, (-allow-credentials)], body: empty }).
        //    Else (non-preflight, or disallowed) → Decision::Continue (encode-side handles actual req).
        unimplemented!()
    }

    pub(crate) fn encode_headers(&mut self, resp: &mut FilterResponse) -> Decision {
        // For a non-preflight request whose origin was allowed: add access-control-allow-origin
        // (+ -expose-headers, -allow-credentials) to resp.headers (the header_mutation push pattern).
        // No active policy / disallowed origin / no origin → Continue unchanged.
        Decision::Continue
    }
}
```

**Signposts:** (a) the per-route policy flows in via the D2 threading (the filter does NOT read the route itself — it is handed its resolved `CorsPolicy` per request); (b) the decode side handles **preflight** (short-circuit) AND records the origin_valid/invalid stat; (c) the encode side handles the **actual-request** response decoration (the first non-trivial encode-side filter logic — the phase-07.2 `header_mutation` encode pattern, `crates/envoy-filter/src/header_mutation.rs:74-128`, is the precedent: `resp.headers.push((k, v))`); (d) origin matching reuses `StringMatcher`; (e) header lookups use the established case-insensitive `header_ci` helper (`crates/envoy-filter/src/jwt_authn.rs:156-161` precedent — the PLAN-writer extracts/reuses it).

### D5 — `HttpFilterInstance::Cors` variant + dispatch + filter-chain `Cors` config

Extend `crates/envoy-config/src/bootstrap.rs::HttpFilterTypedConfig` (`:741-765`) with a seventh variant `Cors(CorsConfig)` (the near-empty filter-chain entry, `@type = …cors.v3.Cors`). Extend `crates/envoy-filter/src/instance.rs::HttpFilterInstance` (`:30-61`) with a `Cors(CorsFilter)` variant; extend the `build` dispatch (`:87-114`, calling `CorsFilter::build_from_config(cfg, registry, hcm_stat_prefix)` — the same 3-arg threading, no signature widening), the `decode_headers` dispatch (`:116-131`), and the `encode_headers` dispatch (`:133-148`). The D2 route-config threading sets the `active_policy` on the `Cors` instance before its decode/encode runs. Re-export `CorsFilter` from `crates/envoy-filter/src/lib.rs`. Remove the `envoy.filters.http.cors` entry (if present) from the unsupported-filter reject list (`crates/envoy-filter/src/error.rs`). The decode-side `Decision::StopAndSend` 200 preflight flows through the existing H1 `decorate_filter_synth_response` + the phase-11 H2 `decorate_filter_synth_response_h2` helpers, **both reused unchanged**.

### D6 — (reserved — no new codec/crypto deliverable)

Unlike phase 11 (H2 decoration helper) or phase 22 (the `envoy-jwt` crypto crate), phase 23 introduces **no new crate and no new codec/crypto deliverable** — the structural centerpiece is the D1/D2 per-route-config infrastructure inside the existing crates. This slot is intentionally empty.

### D7 — Stats wiring (2 counters) + BEHAVIOR_CONTRACT extension

At `CorsFilter::build_from_config`, register `origin_valid` + `origin_invalid` `Counter` handles against the `Arc<StatsRegistry>` at the §6.2-verified namespace. Increment sites in `decode_headers` (per the origin-allowed/​disallowed determination). The BEHAVIOR_CONTRACT extensions (§2.1 stat rows + §2.2 `access-control-*` header rows + §2.3 preflight status/body) land at the tasks where each is first empirically exercised, per the 06.x → 22 cadence.

### D8 — Fixture + harness + fuzz seed + in-process backstop

- **D8.1 — Fixture `tests/fixtures/0031-http-filter-cors/`.** An **HTTP/1.1** listener (the filter is codec-agnostic; H1 reuses the simplest deterministic multi-probe driver — `Driver::Http1ProbeList` or the existing keep-alive driver, the PLAN-writer picks; the four probes need varying method + `Origin` headers). Bootstrap: H1 HCM + the `cors` filter in the chain + a route carrying a `CorsPolicy` via `typed_per_filter_config` (1 allowed origin via `StringMatcher` exact-match, an `allow_methods`, an `allow_headers`, a `max_age`) + router → a static cluster (or a `direct_response: { status: 200, body: "ok\n" }` to keep the data plane trivial). Probe burst: `[preflight-OPTIONS(allowed)→200+headers, GET(allowed)→200+allow-origin, GET(disallowed)→200+no-allow-origin, GET(no-origin)→200+unchanged]`. Asserts the preflight status + the `access-control-*` header set byte-exact (§6.2) + the actual-request `access-control-allow-origin` presence/absence. Docker-gated wrapper `tests/differential/tests/http_filter_cors.rs` (the 09/10/11/22 precedent), one `#[tokio::test]` invoking `run_fixture("0031-http-filter-cors")`.

- **D8.2 — Fuzz seed.** One new `parse_bootstrap` corpus seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_cors_typed_per_filter_config.yaml` (the cors-filter + `typed_per_filter_config` bootstrap shape), extending the curated seed corpus by one (+ the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array, edited together per the 09/10/11/22 cadence). **No new fuzz target** (no new parser crate — CORS reuses `StringMatcher`; the new untrusted surface is the `typed_per_filter_config` map, covered by `parse_bootstrap`). The PLAN-writer confirms at §6.2.

- **D8.3 — In-process backstop.** New file `crates/envoy-bin/tests/http_filter_cors.rs` mirroring `crates/envoy-bin/tests/http_filter_jwt_authn.rs` (the standing `tokio::process::Command` + `.kill_on_drop(true)` subprocess discipline from 09 REVIEW M3). Boots `envoy-bin` (H1) with a synthesized cors bootstrap; issues sequential probes (preflight + allowed-GET + disallowed-GET + no-origin-GET); asserts the preflight status + headers + the actual-request `access-control-allow-origin` presence/absence (heeds the phase-10 M1 backstop-header-assertion lesson — assert OR disclose omission in PROGRESS). **The M18-9/M21-3/M22 extract-a-shared-test-support-crate item is now at N≥7 backstops** — the PLAN-writer notes the duplication in the file header (the consolidation stays deferred by the standing risk-managed decision unless the PLAN-writer judges otherwise).

---

## 4. Out of scope (deferred non-goals)

Phase 23 explicitly does NOT land:

- **Filter `enabled` / `shadow_enabled` runtime fractional gating.** The `Cors` filter-chain entry's `filter_enabled`/`shadow_enabled` `RuntimeFractionalPercent` knobs require the RTDS runtime layer (unimplemented — Runtime + hot restart family). Phase 23's CORS is always-on when a route carries a policy. Defers.
- **`allow_private_network_access` + `forward_not_matching_preflights`.** Niche preflight behaviors. Defer.
- **Vhost-level `typed_per_filter_config` + the route>vhost most-specific-config precedence cascade.** Phase 23 builds + consumes **route-level** `typed_per_filter_config` only; the policy is attached to the route. Vhost-level config and Envoy's `mostSpecificPerFilterConfig` route>vhost>filter precedence resolution defer (the D1 schema is shaped to admit vhost-level additively).
- **`typed_per_filter_config` for filters OTHER than `cors`.** The `PerFilterConfig` enum + the D2 threading are general infrastructure, but phase 23 registers + tests **only** the `Cors` variant. Per-route overrides of `fault`/`rbac`/`local_ratelimit`/`header_mutation` (and `csrf`/`buffer` as future first-class consumers) defer to their own phases — each slots a variant into `PerFilterConfig` additively (the §6.3 anti-pattern is respected: no untested stub variants land).
- **The deprecated inline `route.cors` `CorsPolicy` field.** Modern Envoy attaches CORS via `typed_per_filter_config`; the legacy inline route field is deprecated. Phase 23 implements only the `typed_per_filter_config` path. Defers (likely never — deprecated).
- **CORS shadow/observe-only mode + the per-policy stats beyond `origin_valid`/`origin_invalid`.** Defer.
- **Streaming/trailers CORS interactions + gRPC-Web CORS (`bypass_cors_preflight`).** The gRPC family is still blocked on H2 trailers. Defer.

---

## 5. Architectural invariants

### 5.1 No new crate; per-route config lives in `envoy-config`, the filter in `envoy-filter`

- **The per-route `typed_per_filter_config` schema (`PerFilterConfig`, `CorsPolicy`, `CorsConfig`) lives in `crates/envoy-config/`** alongside the other shared config types — it is config schema, not runtime. No new crate (unlike phase 22's `envoy-jwt`); CORS is pure-Rust header manipulation needing no foundation.
- **`CorsFilter` lives at `crates/envoy-filter/src/cors.rs`** (one module per concrete filter; the 07.2/09/10/11/22 pattern).
- **The D2 route-early-resolution + per-route-config threading lives in the HCMs** (`crates/envoy-http1/src/hcm.rs` + `crates/envoy-http2/src/hcm.rs`) + the `envoy-filter` pipeline abstraction (`pipeline.rs`/`instance.rs`) — it is the one cross-cutting change, and it is shared by both codecs.

### 5.2 Hand-rolled filter per D-3.2

The filter logic (origin matching, preflight detection + short-circuit, response decoration) is hand-rolled per D-3.2 (*"Every individual filter … Must be written from scratch"*). Origin matching reuses the existing `StringMatcher` (04.x). No new dependency.

### 5.3 Bidirectional filter (the first genuinely encode-active feature filter)

`CorsFilter` is the first feature filter with **non-trivial logic on BOTH decode (preflight short-circuit + origin-stat) AND encode (actual-request response decoration)** — `header_mutation` (07.2) mutates on encode but is operator-static; CORS's encode behavior is request-origin-dependent. The decode→encode coupling (the decode side determines origin-allowed; the encode side decorates) requires the per-request `active_policy` + the origin-allowed determination to survive from decode to encode within one request's filter-instance lifetime (the `HttpFilterInstance` is cloned per request per the 07.1 framework — the PLAN-writer confirms the decode-set/encode-read state lives in the per-request clone, not shared).

### 5.4 Per-route config (NOT filter-chain-level policy)

The CORS policy source is the matched route's `typed_per_filter_config` (§0/§4). The filter-chain `Cors` entry is near-empty (it only declares the filter present). No filter-chain-level policy is consumed.

### 5.5 Regression-equivalence is the load-bearing invariant (the route-early-resolution change)

The D2 change to the shared HCM request-processing ordering MUST be behavior-preserving for every non-CORS path: resolving the route earlier and exposing it to the pipeline changes NO existing filter's behavior (no existing filter reads the route — §0 finding 3), and `build_response` reuses the resolved route for identical action dispatch. **All 30 pre-existing fixtures green simultaneously (gate (b)) is the proof obligation** — this is why phase 23's regression surface is the whole HTTP data plane, and why the PLAN-writer designs D2 to minimize blast radius (§3 D2 signpost).

### 5.6 Determinism across both proxies (differential-testability invariant)

CORS is a **pure function** of (request method, request `origin`, the policy). Cross-proxy determinism holds because: (a) the policy is byte-identical config on both proxies; (b) origin matching via `StringMatcher` is deterministic; (c) there is no timing, no clock, no crypto, no body modification — the preflight status + the `access-control-*` header values are fixed by the policy + the request origin. This makes CORS fully differential-testable under `BOOTSTRAP_PROMPT.md` §7.2 with zero timing sensitivity. The `origin_valid`/`origin_invalid` counters increment exactly once per request with a present `origin`.

### 5.7 H1 + H2 symmetric (per-route-config threading is codec-shared)

The D2 threading + the filter operate on the codec-agnostic `FilterRequest`/`FilterResponse` abstraction (the 07.1 framework + ADR-0031). The 200 preflight `Decision::StopAndSend` is decorated by the existing per-codec helpers (H1 since 09; H2 since the phase-11 M2 close) — both reused unchanged. The phase-23 differential fixture is H1; the H1 backstop covers the H1 codec; the codec-shared threading + the phase-11-proven H2 decoration cover any H2 deployment without a phase-23 H2 fixture (the phase-11/22 posture). **The H2 HCM route-early-resolution change is verified non-regressive by h2spec ≥95% + all H2 fixtures (0009/0010/0018/0021) staying green.**

---

## 6. Implementation signposts for the planner

### 6.1 Split-gate evaluation (read first)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 tasks OR ~1500 LoC. Phase 23's SPEC-time surface estimate:

- D1 — per-route `typed_per_filter_config` schema (`PerFilterConfig` + `CorsPolicy` + `CorsConfig` + the `Route` field) (~150 LoC + ~150 LoC tests). ~1-2 tasks.
- D2 — the HCM route-early-resolution + per-route-config threading (H1 + H2; the `build_response` reuse refactor; the pipeline threading) (~250-400 LoC + ~150 LoC tests). **~2-3 tasks — the swing factor.**
- D3 — envoy-config validator (policy-for-absent-filter + StringMatcher) (~70 LoC + ~100 LoC tests). ~1 task or co-located with D1.
- D4 — `CorsFilter` runtime (origin match + preflight + encode decoration) (~180 LoC + ~180 LoC tests). ~1 task.
- D5 — `HttpFilterTypedConfig::Cors` + `HttpFilterInstance::Cors` variant + dispatch + reject removal (~70 LoC + ~40 LoC tests). ~1 task.
- D7 — stats wiring (2 counters) + BEHAVIOR_CONTRACT rows (~40 LoC + ~40 LoC tests). Co-located with D4.
- D8.1 — fixture 0031 + Docker wrapper (~110 LoC YAML + ~60 LoC wrapper). ~1 task.
- D8.2 — fuzz seed (~30 LoC + corpus). ~0.5 task.
- D8.3 — in-process backstop (~190 LoC). ~1 task.
- State-4 verification + STATE-advance (~docs). ~1 task.

**SPEC-time projection: ~10-13 tasks; ~1100-1500 LoC** (production ~700-900, tests ~660, fixture/harness/fuzz/doc ~250). The phase sits **at the upper edge of the §6.1 ~1500-LoC gate** — the D2 cross-cutting HCM change (H1 + H2) is the swing factor (analogous to the phase-22 crypto-crate swing). **Recommended posture: single-phase, BUT the split valve is held in clear reserve** — if the §6.2 PLAN-write materializes D2 clearly over ~1500 LoC / past ~14 tasks (e.g. the H1/H2 threading needs more refactor than projected, or the per-route-config resolution needs the vhost cascade after all), split into **`23.1` (the per-route `typed_per_filter_config` schema D1 + the HCM route-early-resolution + threading D2, proven via regression-equivalence on all 30 existing fixtures — a foundation slice with NO new fixture, the 05.1/07.1/12.1/14.1 pattern, regression-safe per §0 finding 3)** and **`23.2` (the `CorsFilter` D4 + the `HttpFilterInstance` variant D5 + stats D7 + fixture 0031 + backstop + parent-23 close)**. The split ADR would be **ADR-0059**. The D2 cross-cutting change makes this a genuinely split-plausible phase — the PLAN-writer decides AFTER the §6.2 verification materializes the D2 blast radius.

### 6.2 Empirical verification at state-2 PLAN-write (the ratified verify-at-PLAN-write discipline)

Per the phase-10→22 verify-at-PLAN-write process improvement: the state-2 PLAN-writer empirically verifies the upstream wire shapes BEFORE locking PLAN lock-ins, by running `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) under Docker against the §3 D8.1 canonical bootstrap on an H1 listener — **this verification RUNS LOCALLY on macOS Docker** (CORS has no virtiofs/inotify dependency; the phase-22 §6.2-local methodology applies). Verify:

1. **The exact Envoy config shape (do FIRST — it gates the schema D1):** confirm the filter-chain `Cors` entry's `@type` + fields (near-empty?) AND the per-route `CorsPolicy` `@type` (`…cors.v3.CorsPolicy`) + its exact field names/types (`allow_origin_string_match`, `allow_methods`, `allow_headers`, `expose_headers`, `max_age`, `allow_credentials`) as attached via `typed_per_filter_config` on the route. Confirm `StringMatcher` is the `allow_origin_string_match` element type. This is a config-shape probe; it determines D1.
2. **The preflight response (status + headers + body):** drive a preflight `OPTIONS` with an allowed `Origin` + `Access-Control-Request-Method`; capture the exact status (200 vs 204), the exact `access-control-allow-origin` value (echo the request origin? `*`?), the `access-control-allow-methods` / `access-control-allow-headers` formatting (comma+space? comma?), the `access-control-max-age` rendering, the presence of `access-control-allow-credentials`, and the body (empty? `content-length: 0`?). **Do NOT assume** (the phase-10 RBAC-body-off-by-one-byte lesson). Record byte-exact → SPEC §2.2/§2.3 lock-in.
3. **The actual-request response decoration:** drive a `GET` with an allowed `Origin`; capture which `access-control-*` headers Envoy adds to the upstream/`direct_response` response (`access-control-allow-origin`, `access-control-expose-headers`, `access-control-allow-credentials`).
4. **The disallowed-origin + no-origin dispositions:** confirm Envoy adds NO `access-control-*` headers for a present-but-disallowed origin and for a no-origin request (and that the request still proceeds 200).
5. **The stat namespace:** scrape `/stats` post-allowed + post-disallowed; record the exact `cors` stat names + their rooting (HCM-prefixed vs top-level vs per-vhost) (§2.1).
6. **The policy-for-absent-filter disposition:** confirm whether Envoy rejects (fatal) a `CorsPolicy` on a route when the `cors` filter is absent from the chain, or accepts-and-ignores — lock envoy-rust to match (projected all-fatal per ADR-0049). Also confirm the no-policy-on-route passthrough.

Each finding lands as a PLAN lock-in. **If any of items 2-6 differs materially from the SPEC projection, the reconciliation lands as inline ADR-0058 at the state-2 PLAN-write commit** (the phase-10 ADR-0034 / phase-22 ADR-0056 inline-at-PLAN-write precedent). The next-available ADR after this brainstorm's ADR-0057 is **ADR-0058**.

### 6.3 The 06.x stats convention + 07.x BEHAVIOR_CONTRACT cadence

StatsRegistry registration at `build_from_config`; per-filter-instance Counter ownership; namespace §6.2-verified to upstream parity. Contract extensions (stat rows + `access-control-*` header rows + preflight status/body) land at the task where each is first empirically exercised (the 06.x → 22 cadence), NOT at PLAN-write or SPEC time.

### 6.4 In-process backstop header assertion (heeds the phase-10 M1 lesson)

D8.3 SHOULD assert the preflight `access-control-*` headers + the actual-request `access-control-allow-origin` presence/absence, OR explicitly disclose any omission in PROGRESS. Recommended: assert.

### 6.5 State-4 evidence discipline + isolated-crate build

Per the 05.3 → 22 chain: per-gate quoted evidence in PROGRESS at state-4 (real CI run URL + HEAD SHA + timestamp + per-gate output for all 5 stable-toolchain gates + each Docker-gated fixture + the h2spec gate + the fuzz target's iteration count). **Phase-23-specific:** (a) the standalone-crate builds (`project_isolated_crate_build_blindspot`) cover `cargo build -p envoy-config -p envoy-filter -p envoy-http1 -p envoy-http2` (the touched crates); (b) **gate (b) regression-equivalence is the headline** — all 30 pre-existing fixtures green simultaneously proves the D2 shared-HCM-path change is non-regressive; (c) pre-build `tests/helpers/*` and never run the Docker suite concurrently with cargo builds (`project_flaky_access_log_fixture_0012`); (d) `cargo deny check` is a no-op-delta (no new dependency). Per `project_state3_arc_skips_clippy`, the state-3 per-task verification runs `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK (clippy is otherwise first seen at the state-4 gate). **The D2 cross-cutting change makes a workspace-wide `cargo build` + `cargo test --workspace` mandatory at every D2/D5 task** (the phase-22 LESSON: a config-enum variant broke an exhaustive match masked by per-crate testing — run `cargo build --workspace` at config-variant + HCM-threading tasks).

### 6.6 PROGRESS.md skeleton + Task 1 preamble land alongside PLAN.md at state-2; subagent-driven execution at state-3

Per the 06.2 → 22 cadence: state-2 PLAN-write lands `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble in one standalone pre-Task-1 commit. State-3 executes via `superpowers:subagent-driven-development` (the `feedback_execution_style` default), SERIAL dispatch (`feedback_serial_subagent_dispatch` — never parallel), TDD per task, two-stage (spec-then-quality) review on the substantive tasks (the D2 HCM threading + the D4 filter are the review centerpieces), one code commit + one PROGRESS commit per task.

---

## 7. ADR posture

**ADR-0057 lands at THIS brainstorm commit** (the scoping + per-route-config-infrastructure decision — see §0/§2.4/§5.1). The DECISIONS.md ledger head moves from ADR-0056 to **ADR-0057**; the next-available number is **ADR-0058**.

Reserved conditional ADRs for state-2 / state-3:

- **ADR-0058 — §6.2 empirical-verification reconciliation.** Lands at the state-2 PLAN-write commit if any of the §6.2 items 2-6 (preflight status/headers / actual-request decoration / disallowed disposition / stat namespace / policy-for-absent-filter disposition) differs materially from this SPEC's projection. **Recommended posture: verify all at PLAN-write; land ADR-0058 inline if any diverges** (the preflight status + the allow-origin echo-vs-`*` are the most likely triggers).
- **ADR-0059 — §6.1 split.** Lands only if the PLAN materializes over the §6.1 gate and phase 23 splits into 23.1/23.2 per §6.1. **Recommended posture: single-phase; split reserved** (the D2 cross-cutting change is the swing factor — decide after §6.2 materializes the blast radius).

At most one conditional ADR lands per commit (D-3.5 sequential numbering); if multiple fire, they take consecutive numbers.

---

## 8. State-machine signposts for the phase-23 state-2 session

- **Lifecycle state at session start:** State 2 (SPEC.md exists; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `docs/envoy-rust/phases/23-http-filter-cors/PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble (standalone pre-Task-1 commit per the 06.x → 22 cadence).
- **Empirical verification at state 2 (per §6.2):** resolve the Envoy config shape FIRST (gates D1); then verify the preflight status/headers / actual-request decoration / disallowed disposition / stat namespace / policy-for-absent-filter against `envoyproxy/envoy:v1.33.0` (LOCAL Docker); land inline ADR-0058 if any diverges.
- **Split-gate evaluation:** §6.1. **Recommended: single-phase; split (23.1 per-route-config-infra+HCM-threading / 23.2 filter+fixture) reserved (ADR-0059)** — decide after the §6.2 D2-blast-radius materialization.
- **PLAN-time SPEC corrections:** the PLAN-writer reads this SPEC against HEAD `<state-1-commit-SHA>` and flags mechanical drift (the exact `Route`/`HttpFilterTypedConfig` shapes, the exact `StringMatcher` API + its match method, the `FilterRequest`/`FilterResponse` field shapes, the `Decision` variants, the exact H1/H2 `build_response` route-matching call sites + the `decorate_filter_synth_response{,_h2}` helpers) — corrections land in the PROGRESS Task 1 preamble per the 06.2 → 22 "N PLAN-write SPEC corrections" pattern.

---

## 9. Commit message format (for state 6 of the phase-23 lifecycle)

```
phase 23: envoy.filters.http.cors + per-route typed_per_filter_config + fixture 0031 [ADR-0057, ADR-00NN…]

<1-3 sentence summary>

Differential surface: fixture 0031-http-filter-cors (H1); all 31 Docker-gated fixtures (0001-0031) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held (the H2 HCM route-early-resolution change verified non-regressive); parse_bootstrap fuzz clean on its short-budget CI run.
```

The bracketed ADR list carries ADR-0057 (this brainstorm) + any §6.2/§split ADRs that fired. If phase 23 splits at state-2 into 23.1 + 23.2, the closing-sub-phase commit carries `[parent 23 done]` per the 07.2/08.2/12.2/13.2/14.2 closing-sub-phase precedent.

---

## 10. State-machine commit (this commit — phase-23 state-1 brainstorm close-out)

This SPEC is the state-1 output. The state-1 brainstorm commit (the state-0/1-collapsing cadence of phases 12-22) touches exactly four docs files:

- **CREATE** `docs/envoy-rust/phases/23-http-filter-cors/SPEC.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — adds a new row beneath the "HTTP filters family" §9 heading, after the existing phase-22 row, `status: planned` (invariant 4.1.2 — a new row enters `planned`; no existing row flips; row 23 becomes `in-progress` only when the NEXT session's state-2 PLAN-write points STATE at it per invariant 4.1.3).
- **MODIFY** `docs/envoy-rust/DECISIONS.md` — append **ADR-0057** (the scoping + per-route-config-infrastructure decision + the minimum-viable scope + the alternatives-rejected analysis).
- **MODIFY** `docs/envoy-rust/STATE.md` — advance the Active-phase pointer from "AWAITING NEXT PLANNING" to `id: 23` / `slug: 23-http-filter-cors` / `directory: docs/envoy-rust/phases/23-http-filter-cors/` / state-1-complete / state-2-next; relocate the prior "AWAITING NEXT PLANNING" blocks verbatim to STATE_HISTORY.md per ADR-0035 / §4.1 invariant 9; rewrite `## Next expected skill` to the state-2 PLAN-write arc; append a `### Phase-23 state-1 brainstorm` Notes subsection; update `## Last commit` + `## Last updated`.

No code / fixture / Cargo / BEHAVIOR_CONTRACT change; no `unsafe`. The DECISIONS.md ledger head moves to **ADR-0057**. ADR-0014 remains in force; ADR-0028 remains open. ENVOY_TARGET.md + rust-toolchain.toml untouched. The brainstorm commit is docs-only → the CI run at this push is vacuous-green. Per `BOOTSTRAP_PROMPT.md` §5.1 (one state per session) this brainstorm session EXITS after this commit; the NEXT session writes `PLAN.md` (state 2 — `superpowers:writing-plans`, with the §6.2 empirical verification running LOCALLY).

**Commit message:**

```
phase 23: state-1 brainstorm — http-filter-cors SPEC.md (HTTP-filter-family fifth phase; first per-route typed_per_filter_config infra) [ADR-0057]
```

**Predecessor:** `bd3cfd644` — phase-22 state-6 close-out. **Origin/main:** `bd3cfd644` (local + origin in sync as of this commit's prologue).

---

*End of SPEC. Phase 23 state-1 lifecycle complete on landing. The next session enters state 2 — writes PLAN.md per `superpowers:writing-plans`, resolves the Envoy CORS config shape (the D1 gating probe), performs the §6.2 empirical verification LOCALLY (preflight status/headers / actual-request decoration / disallowed disposition / stat namespace / policy-for-absent-filter), and evaluates the §6.1 split gate (the D2 cross-cutting HCM change is the swing factor).*
