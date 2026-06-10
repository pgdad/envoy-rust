# Phase 23 (`23-http-filter-cors`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (the project default per `feedback_execution_style`; SERIAL dispatch per `feedback_serial_subagent_dispatch` — never parallel) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. TDD per task (`superpowers:test-driven-development`). One code commit + one PROGRESS commit per task.

**Goal:** Land `envoy.filters.http.cors` (origin allow-matching, the decode-side preflight short-circuit, and the encode-side actual-request response decoration) as the seventh `HttpFilterInstance` variant, **together with** the per-route `typed_per_filter_config` infrastructure it is the first consumer of.

**Architecture:** A new `PerFilterConfig` `@type`-tagged enum (only the `Cors` variant registered this phase) + a `CorsPolicy` schema live in `envoy-config` on a new `Route.typed_per_filter_config` map. The HCM (H1 + H2) resolves the matched route **up-front** (before the filter pipeline runs) via a shared `resolve_route` helper and threads the route's per-filter config into the pipeline through a new `FilterPipeline::apply_route_config` fan-out — inert for every non-CORS filter, so all 30 pre-existing fixtures stay green (the 07.1 foundation-slice property). `CorsFilter` (hand-rolled, in `envoy-filter`) reads its per-request `CorsPolicy`, short-circuits allowed preflights with a 200 + `access-control-*` headers, and decorates allowed actual-request responses on encode.

**Tech Stack:** Rust (pinned toolchain); `serde`/`serde_yaml` (config); the existing `envoy-filter` framework (07.1) + `StringMatcher` (04.x) + `StatsRegistry` (06.x) + the H1 `decorate_filter_synth_response` / phase-11 H2 `decorate_filter_synth_response_h2` filter-synth helpers. **No new crate, no new dependency, no new fuzz target.**

---

## Scope check

Single subsystem (the `cors` filter + its per-route-config prerequisite, both inside existing crates). **Single phase** — the §6.1 split gate did NOT fire (see "Split-gate decision" below). No sub-project decomposition needed.

## Split-gate decision (§6.1) — SINGLE PHASE, ADR-0059 unfired

The state-2 §6.2 empirical verification materialized the D2 cross-cutting HCM change (the swing factor) as **bounded**: the chosen threading mechanism — a `FilterPipeline::apply_route_config(Option<&Route>)` fan-out that defaults to a no-op for every non-CORS filter — adds **zero** signature churn to `decode_headers`/`encode_headers`/`build`, and leaves `build_response` **byte-identical** (the up-front `resolve_route` is an independent helper; `build_response` re-matches internally with the same `vh_matches`/`route_matches` functions → identical dispatch → trivial 30-fixture regression-equivalence). Refined estimate **~1100–1450 LoC / 10 tasks**, under both §6.1 thresholds (~1500 LoC / ~25 tasks). The split valve (`23.1` schema+threading / `23.2` filter+fixture) is **held in reserve but unused**; **ADR-0059 does NOT fire.**

## §6.2 empirical verification — LOCKED-IN findings (verified LOCALLY against `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd…`, 2026-06-09)

The full probe transcript is in the PROGRESS Task 1 preamble. The load-bearing lock-ins (each anchors a task):

- **L1 — config shape (item 1, CONFIRMED).** Filter-chain entry: `name: envoy.filters.http.cors`, `typed_config."@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors` (no fields used). Per-route policy: `typed_per_filter_config: { envoy.filters.http.cors: { "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy, allow_origin_string_match: [<StringMatcher>], allow_methods: <str>, allow_headers: <str>, expose_headers: <str>, max_age: <str>, allow_credentials: <bool> } }`. `allow_origin_string_match` elements are `StringMatcher` (`exact:` verified). `max_age` is a **string**. All other projected fields confirmed.
- **L2 — preflight response (item 2, CONFIRMED + refined).** Status **200** (NOT 204). Empty body, `content-length: 0`. Preflight detection = `method == OPTIONS` **AND** an `origin` header is present **AND** an `access-control-request-method` header is present (an `OPTIONS`+`origin` request WITHOUT `access-control-request-method` is treated as an **actual request** — proxied + decorated, NOT short-circuited). Headers emitted (only when origin ALLOWED), each conditional on its config field being set:
  - `access-control-allow-origin: <the request Origin, echoed verbatim>` (NOT `*`; always present when allowed)
  - `access-control-allow-credentials: true` (only if `allow_credentials: true`)
  - `access-control-allow-methods: <allow_methods verbatim>` (only if `allow_methods` set)
  - `access-control-allow-headers: <allow_headers verbatim>` (only if `allow_headers` set)
  - `access-control-max-age: <max_age verbatim>` (only if `max_age` set)
  - `access-control-expose-headers: <expose_headers verbatim>` (only if `expose_headers` set)
  - No `vary` header is emitted. A **disallowed-origin** preflight is NOT short-circuited — it proxies through to the upstream (returns the upstream response, no CORS headers).
- **L3 — actual-request decoration (item 3, CONFIRMED + refined).** For a non-preflight request with an **allowed** origin, the encode side adds to the upstream response: `access-control-allow-origin: <echoed origin>` (always), `access-control-allow-credentials: true` (only if configured), `access-control-expose-headers: <verbatim>` (only if configured). It does **NOT** add allow-methods / allow-headers / max-age (those are preflight-only).
- **L4 — disallowed / no-origin (item 4, CONFIRMED).** Present-but-disallowed origin and no-origin requests proceed unchanged (200, no `access-control-*` headers).
- **L5 — stat namespace (item 5, CONFIRMED).** `http.<stat_prefix>.cors.origin_valid` + `http.<stat_prefix>.cors.origin_invalid` (HCM-prefixed, identical rooting to rbac/fault/jwt_authn — NOT a top-level `cors.*`). **`origin_valid` +1 per request with a present, allowed origin** (preflight OR actual); **`origin_invalid` +1 per request with a present, disallowed origin** (preflight OR actual); a no-origin request increments neither. Empirically: a sequence {allowed-preflight, allowed-GET, disallowed-GET, disallowed-preflight, no-origin-GET} yields `origin_valid: 2`, `origin_invalid: 2`.
- **L6 — fixture topology DIVERGENCE → ADR-0058 (items 2-4, MATERIAL).** **Envoy's CORS filter does NOT engage on a `direct_response` route** (a `direct_response` route has no upstream `RouteEntry`; the per-route CORS policy is silently ignored — origin_valid stays 0, no headers emitted, the preflight is NOT short-circuited). **The SPEC §3 D8.1 "(or a `direct_response` … to keep the data plane trivial)" option is INVALID.** Fixture 0031 **must proxy to a real upstream cluster** via the existing `http1-echo-server` helper (the 0008 / 0030 pattern). Verified: with `route: { cluster: backend }`, route-level `typed_per_filter_config` works (preflight short-circuit + decoration both fire); with `direct_response`, neither fires.
- **L7 — policy-for-absent-filter DIVERGENCE → ADR-0058 (item 6, MATERIAL).** Envoy **accepts-and-ignores** a `CorsPolicy` on a route when the `cors` filter is absent from the chain (process stays up, request proceeds 200, policy silently dropped). **envoy-rust diverges to a stricter all-fatal reject** (`ConfigError::PerRouteConfigForAbsentFilter`), consistent with the ADR-0049 all-fatal config posture and the ADR-0054 item-6a stricter-reject precedent. Backstop-only (the fixture has the filter present, so this never affects the differential). The no-policy-on-route passthrough is the trivial default (filter finds no policy → Continue).

**ADR-0058 fires** at the PLAN-write commit (this commit) for L6 + L7. **ADR-0059 (split) does NOT fire.**

## PLAN-time SPEC corrections (verified by read-only recon at HEAD `d5a8d0088`)

The SPEC's source anchors are accurate EXCEPT these, which the implementer must heed:

- **SC1 — `Route` has a HAND-ROLLED deserializer.** `Route` derives only `#[derive(Debug, Clone, PartialEq)]` (NOT `Deserialize`/`Serialize`); it has a hand-rolled `impl<'de> Deserialize for Route` (`bootstrap.rs:1323-1401`, `visit_map` recognizing `match`/`direct_response`/`route`) and a hand-rolled `impl Serialize for Route` (`bootstrap.rs:1403-1418`). The SPEC §3 D1 `#[serde(default)] pub typed_per_filter_config` snippet is therefore WRONG — D1 must **extend the hand-rolled deserializer** (add a `typed_per_filter_config` arm + add it to the `unknown_field` allow-list `&["match", "direct_response", "route", "typed_per_filter_config"]`) and the **hand-rolled serializer** (emit the map when non-empty).
- **SC2 — `header_ci` is PRIVATE to jwt_authn** (`crates/envoy-filter/src/jwt_authn.rs:156-161`, `fn header_ci`, not `pub`). `CorsFilter` needs the same case-insensitive header lookup; duplicate the 5-line helper into `cors.rs` (the cheap, low-risk choice — a shared-util extraction across N filters is the standing-deferred M18-9/M21-3 consolidation item; do NOT expand scope to do it here).
- **SC3 — `StringMatcher` has a hand-rolled `Deserialize`** (`bootstrap.rs:1759+`) and a `matches(&self, value: &str) -> bool` method at `crates/envoy-config/src/matcher.rs:58`. It deserializes from the field-name-oneof YAML (`exact:`/`prefix:`/`suffix:`/`safe_regex:`/`contains:` + optional `ignore_case`). Reuse it verbatim for `allow_origin_string_match` — no changes needed.
- **SC4 — `HttpFilterTypedConfig` has 6 variants** (`bootstrap.rs:741-765`: Router/HeaderMutation/LocalRateLimit/Rbac/Fault/JwtAuthn). D5 adds the 7th (`Cors`). The `HttpFilterInstance` enum (`instance.rs:30-61`) likewise gains the 7th variant; `build`/`decode_headers`/`encode_headers` dispatch (`instance.rs:87-148`) gain Cors arms.
- **SC5 — `decorate_filter_synth_response` signatures.** H1: `fn decorate_filter_synth_response(resp: &mut Response, close: bool)` at `crates/envoy-http1/src/hcm.rs:1407`. H2: `fn decorate_filter_synth_response_h2(resp: &mut Response)` at `crates/envoy-http2/src/response.rs:62`. The decode-side `StopAndSend` preflight 200 flows through these unchanged (they stamp server/date/content-length on the synth response) — confirmed reachable via the existing `RequestPath::SynthFromDecode` arm (H1 `hcm.rs:638-646`) / `H2RequestPath::SynthFromDecode` arm (H2 `hcm.rs:482-490`).
- **SC6 — H1+H2 decode flow.** H1: pipeline cloned at `hcm.rs:600`, `filter_req` built via `mem::take` of `req` fields at `:606-611`, `pipeline.decode_headers` at `:612`, `build_response(&config, &req, close)` at `:639`. H2: pipeline cloned at `hcm.rs:457`, `filter_req` built from `envoy_req` at `:458-464`, `decode_headers` at `:465`, `build_response(&config.inner, &envoy_req, false)` at `:483` (H2 imports H1's `build_response`). The `resolve_route` call + `apply_route_config` must run AFTER the pipeline clone but BEFORE the `mem::take` (which empties `req`/`envoy_req` path+headers).

---

## File structure

| File | Responsibility | Task |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | `PerFilterConfig` enum, `CorsPolicy`, `CorsConfig`; `Route.typed_per_filter_config` field + hand-rolled de/serializer extension; `HttpFilterTypedConfig::Cors`; new `ConfigError` variants; absent-filter validator | 1, 2, 4 |
| `crates/envoy-filter/src/cors.rs` (CREATE) | `CorsFilter` + `CompiledCorsPolicy` (origin match, preflight short-circuit, encode decoration, stats, `apply_route_config`, local `header_ci`) | 3 |
| `crates/envoy-filter/src/instance.rs` | `HttpFilterInstance::Cors` variant + build/decode/encode/`apply_route_config` dispatch | 4 |
| `crates/envoy-filter/src/pipeline.rs` | `FilterPipeline::apply_route_config` fan-out | 4 |
| `crates/envoy-filter/src/lib.rs` | re-export `CorsFilter` | 3 |
| `crates/envoy-http1/src/hcm.rs` | `resolve_route` helper (shared) + H1 up-front route resolution + `apply_route_config` call | 5 |
| `crates/envoy-http2/src/hcm.rs` | H2 up-front route resolution + `apply_route_config` call (reuses `envoy_http1::resolve_route`) | 6 |
| `tests/fixtures/0031-http-filter-cors/` (CREATE) | fixture (envoy.yaml / envoy-rust.yaml / inputs / expectations.yaml / README.md) | 7 |
| `tests/differential/tests/http_filter_cors.rs` (CREATE) | Docker-gated wrapper | 7 |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_cors_typed_per_filter_config.yaml` (CREATE) | fuzz seed | 8 |
| `crates/envoy-bin/tests/http_filter_cors.rs` (CREATE) | in-process backstop | 9 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | cors stat rows + `access-control-*` header rows + preflight status/body row | 3 |

---

## Task 1: D1 — per-route `typed_per_filter_config` schema (`envoy-config`)

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (Route struct ~1152; hand-rolled Route deserializer ~1323; serializer ~1403; new types near the `HttpFilterTypedConfig` block ~765 or adjacent to `Route`)
- Modify: `crates/envoy-config/src/lib.rs` (re-export `PerFilterConfig`, `CorsPolicy`, `CorsConfig` if the crate re-exports config types — match the existing `CorsConfig`/`JwtAuthnConfig` re-export pattern)

- [ ] **Step 1: Write failing tests** in `bootstrap.rs` `#[cfg(test)]` mod (mirror the existing `jwt_authn`/`fault` parse tests):

```rust
#[test]
fn route_parses_typed_per_filter_config_cors() {
    let yaml = r#"
match: { prefix: "/" }
route: { cluster: backend }
typed_per_filter_config:
  envoy.filters.http.cors:
    "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy
    allow_origin_string_match:
      - exact: "http://allowed.example.com"
    allow_methods: "GET, POST, OPTIONS"
    allow_headers: "x-custom-header, content-type"
    expose_headers: "x-exposed-header"
    max_age: "3600"
    allow_credentials: true
"#;
    let route: Route = serde_yaml::from_str(yaml).expect("parses");
    let pfc = route
        .typed_per_filter_config
        .get("envoy.filters.http.cors")
        .expect("cors per-filter config present");
    let PerFilterConfig::Cors(p) = pfc;
    assert_eq!(p.allow_origin_string_match.len(), 1);
    assert!(p.allow_origin_string_match[0].matches("http://allowed.example.com"));
    assert_eq!(p.allow_methods.as_deref(), Some("GET, POST, OPTIONS"));
    assert_eq!(p.allow_headers.as_deref(), Some("x-custom-header, content-type"));
    assert_eq!(p.expose_headers.as_deref(), Some("x-exposed-header"));
    assert_eq!(p.max_age.as_deref(), Some("3600"));
    assert_eq!(p.allow_credentials, Some(true));
}

#[test]
fn route_without_typed_per_filter_config_defaults_empty() {
    let yaml = r#"
match: { prefix: "/" }
route: { cluster: backend }
"#;
    let route: Route = serde_yaml::from_str(yaml).expect("parses");
    assert!(route.typed_per_filter_config.is_empty());
}

#[test]
fn cors_policy_rejects_unknown_field() {
    // deferred forward-looking fields rejected by deny_unknown_fields
    let yaml = r#"
match: { prefix: "/" }
route: { cluster: backend }
typed_per_filter_config:
  envoy.filters.http.cors:
    "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy
    allow_origin_string_match: [ { exact: "x" } ]
    allow_private_network_access: true
"#;
    assert!(serde_yaml::from_str::<Route>(yaml).is_err());
}

#[test]
fn route_rejects_unknown_top_level_key() {
    let yaml = r#"
match: { prefix: "/" }
route: { cluster: backend }
bogus_key: 1
"#;
    assert!(serde_yaml::from_str::<Route>(yaml).is_err());
}

#[test]
fn cors_config_filter_chain_entry_is_near_empty() {
    let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors
"#;
    let _c: CorsConfig = serde_yaml::from_str(yaml).expect("empty Cors entry parses");
}
```

- [ ] **Step 2: Run, verify FAIL** — `cargo test -p envoy-config route_parses_typed_per_filter_config_cors` → FAIL (types undefined).

- [ ] **Step 3: Add the types** near the `Route` block in `bootstrap.rs`. Use `BTreeMap` (deterministic ordering) and import it (`use std::collections::BTreeMap;` if not already present):

```rust
/// 23 D1: per-route filter configuration map, keyed by filter name
/// (`envoy.filters.http.cors`, …) → a `@type`-tagged per-filter config.
/// The per-route counterpart to the filter-chain `HttpFilterTypedConfig`.
/// Only the `Cors` variant is registered + consumed this phase; future
/// filters slot in additively (§6.3 anti-pattern: no untested stub variants).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "@type")]
pub enum PerFilterConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy")]
    Cors(CorsPolicy),
}

/// 23 D1: the per-route CORS policy (attached via `typed_per_filter_config`).
/// `allow_origin_string_match` reuses the 04.x `StringMatcher`. Deferred
/// fields (`filter_enabled`, `shadow_enabled`, `allow_private_network_access`,
/// `forward_not_matching_preflights`) are rejected by `deny_unknown_fields`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorsPolicy {
    pub allow_origin_string_match: Vec<StringMatcher>,
    #[serde(default)]
    pub allow_methods: Option<String>,
    #[serde(default)]
    pub allow_headers: Option<String>,
    #[serde(default)]
    pub expose_headers: Option<String>,
    #[serde(default)]
    pub max_age: Option<String>,
    #[serde(default)]
    pub allow_credentials: Option<bool>,
}

/// 23 D5: the near-empty filter-chain `Cors` entry (declares the filter present).
/// The actual policy lives per-route in `CorsPolicy`. The deferred
/// `filter_enabled`/`shadow_enabled` runtime knobs are rejected by
/// `deny_unknown_fields`.
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CorsConfig {}
```

- [ ] **Step 4: Add the field to `Route`** (the struct at ~1152):

```rust
pub struct Route {
    pub r#match: RouteMatch,
    pub action: RouteAction,
    /// 23 D1: per-route filter configuration (e.g. the CORS policy). Empty
    /// when the route carries no `typed_per_filter_config:` map.
    pub typed_per_filter_config: BTreeMap<String, PerFilterConfig>,
}
```

- [ ] **Step 5: Extend the hand-rolled `Route` deserializer** (`visit_map`, ~1349). Add a local `let mut typed_per_filter_config: Option<BTreeMap<String, PerFilterConfig>> = None;`, a match arm, the unknown-field allow-list update, and the construction:

```rust
// new local alongside r#match/direct_response/route_action:
let mut typed_per_filter_config: Option<BTreeMap<String, PerFilterConfig>> = None;

// new match arm inside `while let Some(key) ...`:
"typed_per_filter_config" => {
    if typed_per_filter_config.is_some() {
        return Err(M::Error::duplicate_field("typed_per_filter_config"));
    }
    typed_per_filter_config =
        Some(map.next_value::<BTreeMap<String, PerFilterConfig>>()?);
}

// update the unknown_field allow-list (the `other =>` arm):
other => {
    return Err(M::Error::unknown_field(
        other,
        &["match", "direct_response", "route", "typed_per_filter_config"],
    ));
}

// at construction (after the action match):
Ok(Route {
    r#match,
    action,
    typed_per_filter_config: typed_per_filter_config.unwrap_or_default(),
})
```

- [ ] **Step 6: Extend the hand-rolled `Route` serializer** (~1403). Emit the map only when non-empty (keeps fixture round-trips clean):

```rust
let len = 2 + usize::from(!self.typed_per_filter_config.is_empty());
let mut map = serializer.serialize_map(Some(len))?;
map.serialize_entry("match", &self.r#match)?;
match &self.action {
    RouteAction::DirectResponse(dr) => map.serialize_entry("direct_response", dr)?,
    RouteAction::Route(ar) => map.serialize_entry("route", ar)?,
}
if !self.typed_per_filter_config.is_empty() {
    map.serialize_entry("typed_per_filter_config", &self.typed_per_filter_config)?;
}
map.end()
```

> NOTE: `PerFilterConfig` derives `Deserialize` only (not `Serialize`). The serializer above serializes the *map* — but the value type must be `Serialize` for `serialize_entry` to compile. Add `Serialize` to `PerFilterConfig`'s derives and provide `#[serde(rename = ...)]` on the variant for the `@type` round-trip, OR (simpler, since fixtures are authored by hand and serialization round-trip of per-filter-config is not required by any consumer) gate the serializer branch behind a manual `Serialize` impl. **Chosen approach: derive `Serialize` on `PerFilterConfig`, `CorsPolicy`, and `CorsConfig`** (matches the `RouteAction_Route`/`RetryPolicy` precedent of `#[derive(Serialize, Deserialize)]`); the `#[serde(tag = "@type")]` round-trips. Update Step 3's derives to `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]` accordingly, and add `#[serde(rename_all = "snake_case")]` is NOT needed (fields already snake_case). Re-run the Step-1 tests to confirm parse still passes.

- [ ] **Step 7: Run tests, verify PASS** — `cargo test -p envoy-config typed_per_filter_config` and the new tests all PASS.

- [ ] **Step 8: clippy + fmt** (per `project_state3_arc_skips_clippy`):
Run: `cargo clippy -p envoy-config --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`. Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 23 Task 1: per-route typed_per_filter_config schema (PerFilterConfig + CorsPolicy + CorsConfig) + Route de/serializer extension"
```

---

## Task 2: D3 — validators (policy-for-absent-filter fatal + StringMatcher delegation)

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (the `ConfigError` enum + the bootstrap-level validation that walks listeners/HCM/routes — locate where existing route/filter validation lives, e.g. `validate_http_filters` and the route-walk; mirror the jwt_authn provider-ref validator)

- [ ] **Step 1: Add the `ConfigError` variant.** Locate the `ConfigError` enum (the `UnsupportedAccessLogType`/jwt variants are the precedent) and add:

```rust
/// 23 D3 (ADR-0058 / L7): a route carries `typed_per_filter_config` for a
/// filter that is NOT present in the enclosing HCM's http_filters chain.
/// envoy-rust rejects this as startup-fatal (the ADR-0049 all-fatal posture);
/// upstream Envoy accepts-and-ignores (recorded divergence, BEHAVIOR_CONTRACT).
#[error("route per-filter config names filter {filter:?} which is absent from the HTTP filter chain")]
PerRouteConfigForAbsentFilter { filter: String },
```

- [ ] **Step 2: Write failing tests** (in the bootstrap test mod). Build a minimal bootstrap YAML string with a CorsPolicy on a route but only the router filter in the chain; assert `parse_bootstrap` errs with the new variant. Also a positive test (cors filter present → parses). Mirror the existing jwt_authn validator tests:

```rust
#[test]
fn cors_per_route_config_without_cors_filter_is_fatal() {
    let yaml = MINIMAL_HCM_BOOTSTRAP_WITH_CORS_POLICY_BUT_NO_CORS_FILTER; // helper const, see Step 4
    let err = parse_bootstrap(yaml).unwrap_err();
    assert!(matches!(err, ConfigError::PerRouteConfigForAbsentFilter { ref filter } if filter == "envoy.filters.http.cors"));
}

#[test]
fn cors_per_route_config_with_cors_filter_present_parses() {
    let yaml = MINIMAL_HCM_BOOTSTRAP_WITH_CORS_POLICY_AND_CORS_FILTER;
    assert!(parse_bootstrap(yaml).is_ok());
}
```

- [ ] **Step 2b: Run, verify FAIL.**

- [ ] **Step 3: Implement the validator.** In the bootstrap-level validation pass (where each HCM's filter chain + routes are walked — find the existing route validation that runs per virtual_host/route), after collecting the set of filter names present in the HCM's `http_filters`, walk every route's `typed_per_filter_config` keys and reject any key not in the present-filter set:

```rust
// for each HCM filter chain: collect present filter names
let present: std::collections::BTreeSet<&str> =
    http_filters.iter().map(|hf| hf.name.as_str()).collect();
// for each route reachable from this HCM's route_config:
for vh in &route_config.virtual_hosts {
    for route in &vh.routes {
        for filter_name in route.typed_per_filter_config.keys() {
            if !present.contains(filter_name.as_str()) {
                return Err(ConfigError::PerRouteConfigForAbsentFilter {
                    filter: filter_name.clone(),
                });
            }
        }
    }
}
```

> The `StringMatcher` structural validity (D3's second half) is enforced automatically by `StringMatcher`'s hand-rolled `Deserialize` (an invalid matcher fails to parse) — no extra validator code needed. Note this in PROGRESS.
> Threading note: the route-config a `rds`-configured HCM uses is resolved post-merge; if the existing validation pass runs pre-merge for RDS HCMs, run the absent-filter check on the effective (post-merge) route_config to avoid false negatives. For inline-route HCMs (fixture 0031) it runs directly. Confirm against the existing route-reference validator's placement and co-locate.

- [ ] **Step 4: Add the test-helper consts** (the two bootstrap YAML strings) in the test mod.

- [ ] **Step 5: Run tests, verify PASS.**

- [ ] **Step 6: clippy + fmt + workspace build** (a new `ConfigError` variant can break exhaustive matches in `envoy-bin`/elsewhere — per the phase-22 lesson):
Run: `cargo build --workspace`, `cargo clippy -p envoy-config --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`. Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 23 Task 2: PerRouteConfigForAbsentFilter validator (ADR-0058 L7 stricter-reject)"
```

---

## Task 3: D4 + D7 — `CorsFilter` runtime + stats + BEHAVIOR_CONTRACT (unwired)

**Files:**
- Create: `crates/envoy-filter/src/cors.rs`
- Modify: `crates/envoy-filter/src/lib.rs` (add `mod cors;` + `pub use cors::CorsFilter;`)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (cors rows)

- [ ] **Step 1: Write `cors.rs` with `CompiledCorsPolicy`, `CorsFilter`, and failing unit tests.** The filter holds per-request state (the pipeline is cloned per request per ADR-0031, so decode→encode state lives in the clone). Structure:

```rust
//! `envoy.filters.http.cors` — origin allow-matching, the decode-side
//! preflight short-circuit, and the encode-side actual-request decoration.
//! §6.2-verified against envoyproxy/envoy:v1.33.0 (phase-23 PLAN-write).

use std::sync::Arc;

use bytes::Bytes;
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

const CORS_FILTER_NAME: &str = "envoy.filters.http.cors";

/// A `CorsPolicy` lowered for per-request use (the `StringMatcher`s are reused
/// from envoy-config; the header strings are cloned for cheap encode emission).
#[derive(Debug, Clone)]
pub(crate) struct CompiledCorsPolicy {
    allow_origin: Vec<envoy_config::StringMatcher>,
    allow_methods: Option<String>,
    allow_headers: Option<String>,
    expose_headers: Option<String>,
    max_age: Option<String>,
    allow_credentials: bool,
}

impl From<&envoy_config::CorsPolicy> for CompiledCorsPolicy {
    fn from(p: &envoy_config::CorsPolicy) -> Self {
        Self {
            allow_origin: p.allow_origin_string_match.clone(),
            allow_methods: p.allow_methods.clone(),
            allow_headers: p.allow_headers.clone(),
            expose_headers: p.expose_headers.clone(),
            max_age: p.max_age.clone(),
            allow_credentials: p.allow_credentials.unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CorsFilter {
    origin_valid: Arc<Counter>,
    origin_invalid: Arc<Counter>,
    /// Set per-request by `apply_route_config` (D2 threading). `None` → the
    /// matched route carries no CORS policy → the filter is inert.
    active_policy: Option<CompiledCorsPolicy>,
    /// Set during decode when an allowed, non-preflight origin is seen; consumed
    /// on encode to decorate the actual-request response. `None` → no decoration.
    decorate_origin: Option<String>,
}

impl CorsFilter {
    pub(crate) fn build_from_config(
        _cfg: &envoy_config::CorsConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        let reg = |suffix: &str| {
            registry
                .register_counter(&format!("http.{hcm_stat_prefix}.cors.{suffix}"))
                .map_err(|e| FilterError::InvalidConfig {
                    message: format!("StatsRegistry: {e}"),
                })
        };
        Ok(Self {
            origin_valid: reg("origin_valid")?,
            origin_invalid: reg("origin_invalid")?,
            active_policy: None,
            decorate_origin: None,
        })
    }

    /// D2 threading: set the per-request policy from the matched route.
    pub(crate) fn apply_route_config(&mut self, route: Option<&envoy_config::Route>) {
        self.active_policy = route
            .and_then(|r| r.typed_per_filter_config.get(CORS_FILTER_NAME))
            .map(|pfc| {
                let envoy_config::PerFilterConfig::Cors(p) = pfc;
                CompiledCorsPolicy::from(p)
            });
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        let Some(policy) = self.active_policy.as_ref() else {
            return Decision::Continue;
        };
        let Some(origin) = header_ci(&req.headers, "origin") else {
            return Decision::Continue; // no Origin → not a CORS request
        };
        let origin = origin.to_string();
        let allowed = policy.allow_origin.iter().any(|m| m.matches(&origin));
        if allowed {
            self.origin_valid.inc();
        } else {
            self.origin_invalid.inc();
        }
        let is_preflight = req.method.eq_ignore_ascii_case("OPTIONS")
            && header_ci(&req.headers, "access-control-request-method").is_some();
        if is_preflight && allowed {
            return Decision::StopAndSend(build_preflight_response(policy, &origin));
        }
        if allowed && !is_preflight {
            self.decorate_origin = Some(origin); // remember for encode
        }
        Decision::Continue
    }

    pub(crate) fn encode_headers(&mut self, resp: &mut FilterResponse) -> Decision {
        let (Some(policy), Some(origin)) =
            (self.active_policy.as_ref(), self.decorate_origin.take())
        else {
            return Decision::Continue;
        };
        resp.headers
            .push(("access-control-allow-origin".to_string(), origin));
        if policy.allow_credentials {
            resp.headers.push((
                "access-control-allow-credentials".to_string(),
                "true".to_string(),
            ));
        }
        if let Some(eh) = &policy.expose_headers {
            resp.headers
                .push(("access-control-expose-headers".to_string(), eh.clone()));
        }
        Decision::Continue
    }
}

/// Build the §6.2-verified preflight 200 local reply (empty body). The
/// access-control-* headers are emitted in the verified order; server/date/
/// content-length are stamped later by the H1/H2 filter-synth decorators.
fn build_preflight_response(policy: &CompiledCorsPolicy, origin: &str) -> FilterResponse {
    let mut headers: Vec<(String, String)> = Vec::with_capacity(6);
    headers.push(("access-control-allow-origin".to_string(), origin.to_string()));
    if policy.allow_credentials {
        headers.push((
            "access-control-allow-credentials".to_string(),
            "true".to_string(),
        ));
    }
    if let Some(m) = &policy.allow_methods {
        headers.push(("access-control-allow-methods".to_string(), m.clone()));
    }
    if let Some(h) = &policy.allow_headers {
        headers.push(("access-control-allow-headers".to_string(), h.clone()));
    }
    if let Some(a) = &policy.max_age {
        headers.push(("access-control-max-age".to_string(), a.clone()));
    }
    if let Some(e) = &policy.expose_headers {
        headers.push(("access-control-expose-headers".to_string(), e.clone()));
    }
    FilterResponse {
        status: 200,
        reason: None,
        headers,
        body: Bytes::new(),
    }
}

/// Case-insensitive header lookup (duplicated from jwt_authn.rs:156 per SC2 —
/// shared-util extraction is the standing-deferred M18-9/M21-3 item).
fn header_ci<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}
```

Failing unit tests (same file, `#[cfg(test)] mod tests`):

```rust
fn policy(creds: bool) -> envoy_config::CorsPolicy {
    use envoy_config::{StringMatcher, StringMatcherMode};
    envoy_config::CorsPolicy {
        allow_origin_string_match: vec![StringMatcher {
            mode: StringMatcherMode::Exact("http://allowed.example.com".to_string()),
            ignore_case: false,
        }],
        allow_methods: Some("GET, POST, OPTIONS".to_string()),
        allow_headers: Some("x-custom-header, content-type".to_string()),
        expose_headers: Some("x-exposed-header".to_string()),
        max_age: Some("3600".to_string()),
        allow_credentials: if creds { Some(true) } else { None },
    }
}
fn filter_with(p: &envoy_config::CorsPolicy) -> CorsFilter {
    let reg = Arc::new(StatsRegistry::new());
    let mut f = CorsFilter::build_from_config(&envoy_config::CorsConfig::default(), &reg, "ingress_http").unwrap();
    f.active_policy = Some(CompiledCorsPolicy::from(p));
    f
}
fn req(method: &str, headers: Vec<(&str, &str)>) -> FilterRequest {
    FilterRequest {
        method: method.to_string(),
        path: "/".to_string(),
        headers: headers.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        body: None,
    }
}

#[test]
fn preflight_allowed_short_circuits_200_with_headers() {
    let p = policy(true);
    let mut f = filter_with(&p);
    let d = f.decode_headers(&mut req("OPTIONS", vec![
        ("origin", "http://allowed.example.com"),
        ("access-control-request-method", "GET"),
    ]));
    match d {
        Decision::StopAndSend(r) => {
            assert_eq!(r.status, 200);
            assert!(r.body.is_empty());
            let h = |n: &str| r.headers.iter().find(|(k,_)| k == n).map(|(_,v)| v.as_str());
            assert_eq!(h("access-control-allow-origin"), Some("http://allowed.example.com"));
            assert_eq!(h("access-control-allow-credentials"), Some("true"));
            assert_eq!(h("access-control-allow-methods"), Some("GET, POST, OPTIONS"));
            assert_eq!(h("access-control-allow-headers"), Some("x-custom-header, content-type"));
            assert_eq!(h("access-control-max-age"), Some("3600"));
            assert_eq!(h("access-control-expose-headers"), Some("x-exposed-header"));
        }
        Decision::Continue => panic!("expected preflight short-circuit"),
    }
}

#[test]
fn preflight_disallowed_origin_continues() {
    let p = policy(true);
    let mut f = filter_with(&p);
    let d = f.decode_headers(&mut req("OPTIONS", vec![
        ("origin", "http://evil.example.com"),
        ("access-control-request-method", "GET"),
    ]));
    assert!(matches!(d, Decision::Continue));
}

#[test]
fn options_without_acrm_is_actual_request_not_preflight() {
    let p = policy(false);
    let mut f = filter_with(&p);
    // OPTIONS + allowed origin but NO access-control-request-method → actual request
    let d = f.decode_headers(&mut req("OPTIONS", vec![("origin", "http://allowed.example.com")]));
    assert!(matches!(d, Decision::Continue));
    let mut resp = FilterResponse { status: 200, reason: None, headers: vec![], body: Bytes::new() };
    f.encode_headers(&mut resp);
    assert!(resp.headers.iter().any(|(k,v)| k == "access-control-allow-origin" && v == "http://allowed.example.com"));
}

#[test]
fn actual_request_allowed_decorates_on_encode() {
    let p = policy(true);
    let mut f = filter_with(&p);
    assert!(matches!(f.decode_headers(&mut req("GET", vec![("origin", "http://allowed.example.com")])), Decision::Continue));
    let mut resp = FilterResponse { status: 200, reason: None, headers: vec![], body: Bytes::new() };
    f.encode_headers(&mut resp);
    let h = |n: &str| resp.headers.iter().find(|(k,_)| k == n).map(|(_,v)| v.as_str());
    assert_eq!(h("access-control-allow-origin"), Some("http://allowed.example.com"));
    assert_eq!(h("access-control-allow-credentials"), Some("true"));
    assert_eq!(h("access-control-expose-headers"), Some("x-exposed-header"));
    // actual-request decoration NEVER adds methods/headers/max-age:
    assert!(h("access-control-allow-methods").is_none());
    assert!(h("access-control-max-age").is_none());
}

#[test]
fn disallowed_origin_no_decoration() {
    let p = policy(true);
    let mut f = filter_with(&p);
    f.decode_headers(&mut req("GET", vec![("origin", "http://evil.example.com")]));
    let mut resp = FilterResponse { status: 200, reason: None, headers: vec![], body: Bytes::new() };
    f.encode_headers(&mut resp);
    assert!(resp.headers.is_empty());
}

#[test]
fn no_origin_no_action_no_stats() {
    let p = policy(true);
    let mut f = filter_with(&p);
    assert!(matches!(f.decode_headers(&mut req("GET", vec![])), Decision::Continue));
    let mut resp = FilterResponse { status: 200, reason: None, headers: vec![], body: Bytes::new() };
    f.encode_headers(&mut resp);
    assert!(resp.headers.is_empty());
}

#[test]
fn no_active_policy_is_inert() {
    let reg = Arc::new(StatsRegistry::new());
    let mut f = CorsFilter::build_from_config(&envoy_config::CorsConfig::default(), &reg, "ingress_http").unwrap();
    // active_policy left None
    assert!(matches!(f.decode_headers(&mut req("OPTIONS", vec![
        ("origin", "http://allowed.example.com"), ("access-control-request-method", "GET")])), Decision::Continue));
}

#[test]
fn stats_tick_once_per_present_origin() {
    let p = policy(true);
    let mut f = filter_with(&p);
    f.decode_headers(&mut req("GET", vec![("origin", "http://allowed.example.com")]));   // valid +1
    f.decode_headers(&mut req("GET", vec![("origin", "http://evil.example.com")]));      // invalid +1
    f.decode_headers(&mut req("GET", vec![]));                                            // neither
    assert_eq!(f.origin_valid.value(), 1);
    assert_eq!(f.origin_invalid.value(), 1);
}
```

> Confirm the `Counter` read accessor name (`.value()` vs `.get()`) against `crates/envoy-stats` before finalizing the stats test; match the existing fault/rbac stats-test accessor. Confirm `StringMatcher`/`StringMatcherMode` are `pub` and re-exported from `envoy_config` (used in the test constructor); if `StringMatcherMode` is not public, construct the matcher via `serde_yaml::from_str::<StringMatcher>("exact: http://allowed.example.com")` instead.

- [ ] **Step 2: Run, verify FAIL** (module not declared / types undefined).

- [ ] **Step 3: Wire the module** — `crates/envoy-filter/src/lib.rs`: add `mod cors;` and `pub use cors::CorsFilter;` (match the jwt_authn re-export).

- [ ] **Step 4: Run tests, verify PASS** — `cargo test -p envoy-filter cors`.

- [ ] **Step 5: BEHAVIOR_CONTRACT.md** — append a `**23 entries (CORS filter)**` block under "Stat-name mapping" (the 2 cors counters, value-exact, §6.2-verified namespace `http.<stat_prefix>.cors.{origin_valid,origin_invalid}`, increment-on-present-origin semantics), a "Response headers — cors `access-control-*`" subsection (the 6 headers, value-exact, NOT allow-listed; the preflight set vs the actual-request set per L2/L3; the echo-origin disposition), and a "Preflight local reply" note (status 200, empty body, content-length: 0 stamped by the synth decorators; disallowed/no-ACRM proxy through). Cross-reference ADR-0058.

- [ ] **Step 6: clippy + fmt** — `cargo clippy -p envoy-filter --all-targets --all-features -- -D warnings`; `cargo fmt --all -- --check`. Clean.

- [ ] **Step 7: Commit**

```bash
git add crates/envoy-filter/src/cors.rs crates/envoy-filter/src/lib.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 23 Task 3: CorsFilter runtime (preflight short-circuit + encode decoration + 2 stats) + BEHAVIOR_CONTRACT cors rows"
```

---

## Task 4: D5 — `Cors` variants + dispatch + `apply_route_config` fan-out

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`HttpFilterTypedConfig::Cors(CorsConfig)`)
- Modify: `crates/envoy-filter/src/instance.rs` (`HttpFilterInstance::Cors` + build/decode/encode/`apply_route_config` arms)
- Modify: `crates/envoy-filter/src/pipeline.rs` (`FilterPipeline::apply_route_config`)

- [ ] **Step 1: Write failing tests.** (a) In `bootstrap.rs`: a filter-chain `cors` entry parses to `HttpFilterTypedConfig::Cors`. (b) In `instance.rs`: `build` produces `HttpFilterInstance::Cors`; `apply_route_config` sets the policy. (c) In `pipeline.rs`: a `cors`+`router` pipeline, after `apply_route_config(Some(&route_with_cors_policy))`, short-circuits an allowed preflight; with `apply_route_config(None)` it is inert.

```rust
// pipeline.rs test:
#[test]
fn apply_route_config_then_preflight_short_circuits() {
    let filters = vec![
        envoy_config::HttpFilter { name: "envoy.filters.http.cors".into(),
            typed_config: envoy_config::HttpFilterTypedConfig::Cors(envoy_config::CorsConfig::default()) },
        envoy_config::HttpFilter { name: "envoy.filters.http.router".into(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(envoy_config::RouterConfig {}) },
    ];
    let mut pipeline = FilterPipeline::build_from_config(&filters, &Arc::new(StatsRegistry::new()), "ingress_http").unwrap();
    let route: envoy_config::Route = serde_yaml::from_str(
        "match: { prefix: \"/\" }\nroute: { cluster: backend }\ntyped_per_filter_config:\n  envoy.filters.http.cors:\n    \"@type\": type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy\n    allow_origin_string_match: [ { exact: \"http://a.test\" } ]\n    allow_methods: \"GET\"\n").unwrap();
    pipeline.apply_route_config(Some(&route));
    let mut req = FilterRequest { method: "OPTIONS".into(), path: "/".into(),
        headers: vec![("origin".into(), "http://a.test".into()), ("access-control-request-method".into(), "GET".into())], body: None };
    assert!(matches!(pipeline.decode_headers(&mut req), Decision::StopAndSend(r) if r.status == 200));
}

#[test]
fn apply_route_config_none_leaves_cors_inert() {
    // same pipeline; apply_route_config(None) → preflight passes through to Router → Continue
    // ...build identical pipeline...
    pipeline.apply_route_config(None);
    let mut req = /* same preflight req */;
    assert!(matches!(pipeline.decode_headers(&mut req), Decision::Continue));
}
```

- [ ] **Step 2: Run, verify FAIL.**

- [ ] **Step 3: Add `HttpFilterTypedConfig::Cors`** (`bootstrap.rs:741-765`):

```rust
#[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors")]
Cors(CorsConfig),
```

- [ ] **Step 4: Add the `HttpFilterInstance` arms** (`instance.rs`):

```rust
// variant (after JwtAuthn):
/// Phase-23: the `envoy.filters.http.cors` filter (decode-side preflight
/// short-circuit + encode-side actual-request decoration; the per-route
/// CorsPolicy is supplied via `apply_route_config`; 2 stat counters under
/// `http.{hcm_stat_prefix}.cors.{origin_valid,origin_invalid}`).
Cors(CorsFilter),

// build arm:
envoy_config::HttpFilterTypedConfig::Cors(cfg) => Ok(HttpFilterInstance::Cors(
    CorsFilter::build_from_config(cfg, registry, hcm_stat_prefix)?,
)),

// decode_headers arm:
HttpFilterInstance::Cors(f) => f.decode_headers(req),

// encode_headers arm:
HttpFilterInstance::Cors(f) => f.encode_headers(resp_arg),
```

Add `use crate::cors::CorsFilter;` to instance.rs imports.

- [ ] **Step 5: Add `apply_route_config` to `HttpFilterInstance`** (new method, after `encode_headers`):

```rust
/// Phase-23 D2: thread the matched route's per-filter config into the
/// per-request filter instance. No-op for every filter that does not consume
/// per-route config (Router/HeaderMutation/LocalRateLimit/Rbac/Fault/JwtAuthn);
/// only `Cors` reads it.
pub(crate) fn apply_route_config(&mut self, route: Option<&envoy_config::Route>) {
    if let HttpFilterInstance::Cors(f) = self {
        f.apply_route_config(route);
    }
}
```

- [ ] **Step 6: Add `FilterPipeline::apply_route_config`** (`pipeline.rs`, after `build_from_config`):

```rust
/// Phase-23 D2: fan the matched route's per-filter config out to each filter
/// instance before the decode pass. Inert for all non-CORS filters → the
/// 07.1 foundation-slice property (all pre-existing fixtures unchanged).
pub fn apply_route_config(&mut self, route: Option<&envoy_config::Route>) {
    for filter in self.filters.iter_mut() {
        filter.apply_route_config(route);
    }
}
```

- [ ] **Step 7: Run tests, verify PASS** — `cargo test -p envoy-config -p envoy-filter`.

- [ ] **Step 8: WORKSPACE BUILD + WORKSPACE TEST** (D5 adds a `HttpFilterTypedConfig` variant → breaks any exhaustive match over it; per the phase-22 lesson run the full workspace, not just touched crates):
Run: `cargo build --workspace --all-targets`, `cargo test --workspace`. Expected: clean (fix any newly-non-exhaustive match — search for `match .*typed_config` / `HttpFilterTypedConfig::` across the workspace; a config-dump/serialize path may need a `Cors` arm).

- [ ] **Step 9: clippy + fmt** — `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo fmt --all -- --check`. Clean.

- [ ] **Step 10: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-filter/src/instance.rs crates/envoy-filter/src/pipeline.rs
git commit -m "phase 23 Task 4: HttpFilterInstance::Cors variant + dispatch + FilterPipeline::apply_route_config fan-out"
```

---

## Task 5: D2 — H1 HCM route-early-resolution + per-route-config threading

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (new `resolve_route` helper; the decode-region threading at ~600-639)

- [ ] **Step 1: Write a failing integration test** (in `hcm.rs` test mod, using the `test-util` feature). Build an HCM config with a `cors`+`router` chain and a route carrying a CorsPolicy; drive an H1 request representation through the decode path and assert (a) an allowed preflight short-circuits 200, (b) an allowed GET gets `access-control-allow-origin` on the response, (c) a request to a route with NO cors policy is unaffected (regression). Mirror the existing HCM decode integration tests. If the HCM decode path is only exercisable end-to-end via a socket, place the assertion in the Task 9 backstop instead and note it here; add at minimum a `resolve_route` unit test:

```rust
#[test]
fn resolve_route_matches_vh_and_route() {
    let config = test_hcm_config_with_one_route(); // helper: vh "*", route prefix "/"
    let req = test_request_get("/", "localhost");
    let route = resolve_route(&config, &req).expect("route resolves");
    assert!(matches!(route.action, envoy_config::RouteAction::Route(_) | envoy_config::RouteAction::DirectResponse(_)));
}

#[test]
fn resolve_route_none_on_no_vh_match() {
    let config = test_hcm_config_with_one_route();
    let req = test_request_get("/", ""); // empty host → no vh
    assert!(resolve_route(&config, &req).is_none());
}
```

- [ ] **Step 2: Run, verify FAIL** (`resolve_route` undefined).

- [ ] **Step 3: Add the `resolve_route` helper** to `hcm.rs` (pub, so H2 reuses it; it reuses the same `vh_matches`/`route_matches`/`strip_port`/`find_header` functions `build_response` uses, guaranteeing identical matching):

```rust
/// Phase-23 D2: resolve the matched route up-front (vh-match + route-match),
/// for threading per-route filter config into the pipeline BEFORE the decode
/// pass. Returns `None` for missing/empty Host, no matching vh, or no matching
/// route — the no-route paths carry no per-route config (a 404'd request has no
/// CORS policy). Shares `vh_matches`/`route_matches` with `build_response`, so
/// the up-front resolution and `build_response`'s internal re-match are
/// guaranteed identical (the 30-fixture regression-equivalence guarantee).
pub fn resolve_route<'a>(
    config: &'a HCMConfig,
    req: &Request,
) -> Option<&'a envoy_config::Route> {
    let host_raw = find_header(&req.headers, headers::HOST).filter(|h| !h.is_empty())?;
    let host = strip_port(host_raw);
    let vh = config
        .route_config
        .virtual_hosts
        .iter()
        .find(|vh| vh_matches(vh, host))?;
    vh.routes
        .iter()
        .find(|r| route_matches(r, &req.path, &req.headers))
}
```

> Confirm the exact type name the H1 HCM config uses (`HCMConfig` vs `Http1HCMConfig` — the recon showed `build_response(config: &HCMConfig, …)`; use whatever `build_response` takes) and that `route_config`, `vh_matches`, `route_matches`, `strip_port`, `find_header`, `headers::HOST` are in scope. `Route` must be `pub` in envoy-config (it is).

- [ ] **Step 4: Thread it in the decode region** (~600, AFTER `let mut pipeline = (*config.filter_pipeline).clone();` and BEFORE the `mem::take` that builds `filter_req`):

```rust
let mut pipeline = (*config.filter_pipeline).clone();
// Phase-23 D2: resolve the matched route up-front and thread its per-filter
// config into the pipeline before decode (inert for every non-CORS filter).
let matched_route = resolve_route(&config, &req);
pipeline.apply_route_config(matched_route);
// (existing) build filter_req via mem::take, then decode_headers ...
```

> `matched_route` borrows `config`; `apply_route_config` clones the policy into the Cors instance, so the borrow ends before the `mem::take` of `req`. `build_response(&config, &req, close)` at ~639 stays UNCHANGED (re-matches internally; identical result).

- [ ] **Step 5: Run tests, verify PASS.**

- [ ] **Step 6: WORKSPACE BUILD + WORKSPACE TEST** (per the phase-22 lesson — the HCM decode path is shared by every HTTP fixture; the regression surface is the whole H1 data plane):
Run: `cargo build --workspace --all-targets`, `cargo test --workspace`. Expected: clean (all existing envoy-http1 + envoy-bin tests still green — the regression-equivalence proof obligation).

- [ ] **Step 7: clippy + fmt** — `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo fmt --all -- --check`. Clean.

- [ ] **Step 8: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 23 Task 5: H1 HCM route-early-resolution + apply_route_config threading (resolve_route helper)"
```

---

## Task 6: D2 — H2 HCM symmetric threading

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (the decode region ~457-465; reuse `envoy_http1::resolve_route`)

- [ ] **Step 1: Write a failing test** (H2 decode integration test, `test-util` feature — mirror the H1 one if the H2 HCM has an analogous test harness; else add the assertion to the backstop and note it). At minimum assert the H2 decode path calls `apply_route_config` (a regression test that an existing H2 fixture's decode is unchanged + a cors preflight short-circuits over H2 if an H2 integration harness exists).

- [ ] **Step 2: Run, verify FAIL** (or note no-unit-harness and rely on Task 9 + h2spec).

- [ ] **Step 3: Thread it** in the H2 decode region (~457, AFTER `let mut pipeline = (*config.inner.filter_pipeline).clone();` and BEFORE the `mem::take` building `filter_req` from `envoy_req`):

```rust
let mut pipeline = (*config.inner.filter_pipeline).clone();
// Phase-23 D2: symmetric to H1 — resolve the route up-front and thread its
// per-filter config in. H2's `envoy_req` is an `envoy_http1::Request`, and H2
// already imports H1's `build_response`, so reuse `envoy_http1::resolve_route`.
let matched_route = envoy_http1::resolve_route(&config.inner, &envoy_req);
pipeline.apply_route_config(matched_route);
// (existing) build filter_req via mem::take of envoy_req fields, then decode ...
```

> Confirm `envoy_http1::resolve_route` is importable from envoy-http2 (it is — H2 already imports `build_response` from envoy-http1) and that `config.inner` is the `HCMConfig` type `resolve_route` expects (the recon showed `build_response(&config.inner, &envoy_req, false)` at H2 hcm.rs:483). Add the `use envoy_http1::resolve_route;` import or call it fully-qualified.

- [ ] **Step 4: Run tests, verify PASS.**

- [ ] **Step 5: WORKSPACE BUILD + WORKSPACE TEST + h2spec note** — `cargo build --workspace --all-targets`, `cargo test --workspace`. Clean. (h2spec ≥95% is re-run at the state-4 gate, Task 10 — note here that the H2 HCM ordering change must be verified non-regressive there.)

- [ ] **Step 6: clippy + fmt** — `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo fmt --all -- --check`. Clean.

- [ ] **Step 7: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 23 Task 6: H2 HCM route-early-resolution + apply_route_config threading (reuses envoy_http1::resolve_route)"
```

---

## Task 7: D8.1 — fixture `0031-http-filter-cors` + Docker wrapper

> **L6 LOCK-IN:** the fixture MUST proxy to a real upstream cluster (the `http1-echo-server` helper) — `direct_response` does NOT engage CORS. Mirror fixture `0008-http1-router-upstream` / `0030-http-filter-jwt-authn` for the cluster + backend wiring.

**Files:**
- Create: `tests/fixtures/0031-http-filter-cors/envoy.yaml`
- Create: `tests/fixtures/0031-http-filter-cors/envoy-rust.yaml` (identical filter/route config; per-side cluster shape matching the 0008/0030 backend-helper convention)
- Create: `tests/fixtures/0031-http-filter-cors/inputs/` (the 4-probe driver inputs — match the 0030 `inputs/` shape)
- Create: `tests/fixtures/0031-http-filter-cors/expectations.yaml`
- Create: `tests/fixtures/0031-http-filter-cors/README.md`
- Create: `tests/differential/tests/http_filter_cors.rs`

- [ ] **Step 1: Author the fixture configs.** Copy the 0030 fixture's listener/cluster/backend scaffold (H1 listener + `http1-echo-server` cluster). Add the `cors` filter (before `router`) in `http_filters`, and attach the `CorsPolicy` to the route via `typed_per_filter_config` (allowed origin `http://allowed.example.com` via `exact:`, `allow_methods: "GET, POST, OPTIONS"`, `allow_headers: "x-custom-header, content-type"`, `max_age: "3600"`). The route is `route: { cluster: <echo-cluster> }` (NOT direct_response). Match the per-side cluster/DNS conventions the 0008/0030 fixtures use (the `Driver::Http1*` backend helper, `dns_lookup_family: V4_ONLY` / host-gateway as those fixtures do).

- [ ] **Step 2: Choose the driver + author `inputs/` + `expectations.yaml`.** Use the multi-probe H1 driver the 0030 fixture uses (`Driver::Http1ProbeList` or the keep-alive list driver — match 0030). Four probes:
  1. `OPTIONS /` + `Origin: http://allowed.example.com` + `Access-Control-Request-Method: GET` → status **200**, body empty, headers include `access-control-allow-origin: http://allowed.example.com`, `access-control-allow-methods: GET, POST, OPTIONS`, `access-control-allow-headers: x-custom-header, content-type`, `access-control-max-age: 3600` (value-exact, per L2).
  2. `GET /` + `Origin: http://allowed.example.com` → status 200, body = the echo backend's body, header `access-control-allow-origin: http://allowed.example.com` present (L3).
  3. `GET /` + `Origin: http://evil.example.com` → status 200, NO `access-control-allow-origin` (L4).
  4. `GET /` (no Origin) → status 200, NO `access-control-*` headers (L4).
  `expectations.yaml`: assert per-probe status + the `access-control-*` header set value-exact (no allow-list for these — they're a pure function of policy+origin per L2/L3); `server`/`date`/`x-envoy-upstream-service-time` stay allow-listed as in 0030. The fixture does NOT scrape `cors` stats (the backstop + unit tests cover those, per SPEC §2.1).

- [ ] **Step 3: Write the Docker wrapper** `tests/differential/tests/http_filter_cors.rs` (copy `http_filter_jwt_authn.rs` verbatim, retarget the path):

```rust
use std::path::PathBuf;

#[tokio::test]
async fn http_filter_cors_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0031-http-filter-cors");
    differential::run_fixture(&dir).await.expect("fixture passes");
}
```

- [ ] **Step 4: README.md** — document the 4 probes, the L6 "must use a real cluster, not direct_response" rationale (cross-ref ADR-0058), and the value-exact `access-control-*` disposition.

- [ ] **Step 5: Pre-build helpers, then run the fixture** (per `project_flaky_access_log_fixture_0012` — pre-build `tests/helpers/*`; NEVER run the Docker suite concurrently with cargo builds):

```bash
cargo build -p http1-echo-server   # or whichever helper the fixture uses
cargo build -p envoy-bin --release
cargo test -p differential http_filter_cors -- --nocapture
```

Expected: PASS (both proxies agree on all 4 probes).

- [ ] **Step 6: Commit**

```bash
git add tests/fixtures/0031-http-filter-cors/ tests/differential/tests/http_filter_cors.rs
git commit -m "phase 23 Task 7: fixture 0031-http-filter-cors (4 probes, real upstream cluster per ADR-0058 L6) + Docker wrapper"
```

---

## Task 8: D8.2 — `parse_bootstrap` fuzz seed

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_cors_typed_per_filter_config.yaml`
- Modify: `crates/envoy-config/src/bootstrap.rs` (`fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array ~4387)

- [ ] **Step 1: Author the seed** — a full minimal bootstrap with an H1 HCM, a `cors`+`router` filter chain, and a route carrying a `CorsPolicy` via `typed_per_filter_config` (exercises the new untrusted surface: the `typed_per_filter_config` map + `CorsPolicy`). Model it on the working §6.2 `envoy3.yaml` (route-level config + cluster route), trimmed to envoy-rust's schema (no admin/node requirements beyond what `parse_bootstrap` needs — copy the shape of an existing corpus seed like `route_retry_policy.yaml`).

- [ ] **Step 2: Add it to the SUCCESS array** in `fuzz_corpus_seeds_parse_or_reject_cleanly` (append `"fuzz/corpus/parse_bootstrap/route_cors_typed_per_filter_config.yaml",`).

- [ ] **Step 3: Run the corpus test** — `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly`. Expected: PASS (the new seed parses cleanly). **No new fuzz target** (CORS reuses `StringMatcher`; the new surface is covered by the existing `parse_bootstrap` target — confirm in PROGRESS).

- [ ] **Step 4: clippy + fmt + commit**

```bash
cargo fmt --all -- --check
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/route_cors_typed_per_filter_config.yaml crates/envoy-config/src/bootstrap.rs
git commit -m "phase 23 Task 8: parse_bootstrap fuzz seed for cors typed_per_filter_config (no new target)"
```

---

## Task 9: D8.3 — in-process backstop

**Files:**
- Create: `crates/envoy-bin/tests/http_filter_cors.rs` (mirror `crates/envoy-bin/tests/http_filter_jwt_authn.rs`: `tokio::process::Command` + `.kill_on_drop(true)`, boot `envoy-bin` with a synthesized bootstrap, sequential probes)

- [ ] **Step 1: Write the backstop** — copy the jwt_authn backstop skeleton (port reservation, bootstrap-YAML `format!`, readiness wait, the probe helper, `dump_stderr_and_kill`). The bootstrap proxies to a small in-test backend (reuse the backstop backend pattern from 0030, or a `tokio` one-shot listener returning 200 `ok\n`). Probes + assertions (heeds the phase-10 M1 lesson — ASSERT the headers, per SPEC §6.4):
  - preflight allowed → 200, body empty, `access-control-allow-origin: http://allowed.example.com` + `-allow-methods` + `-allow-headers` + `-max-age` present (value-exact).
  - GET allowed → 200, `access-control-allow-origin: http://allowed.example.com` present.
  - GET disallowed → 200, NO `access-control-allow-origin`.
  - GET no-origin → 200, NO `access-control-*`.
  - (negative, envoy-rust-only) a SECOND boot with a CorsPolicy on a route but NO cors filter in the chain → assert the process exits non-zero / fails readiness (the L7 `PerRouteConfigForAbsentFilter` fatal-reject). If wiring a second boot is heavy, assert this via a `parse_bootstrap` unit test instead (already covered in Task 2) and note the backstop omission in PROGRESS per §6.4.
  - Optionally scrape `/stats` and assert `http.<prefix>.cors.origin_valid`/`origin_invalid` reflect the probe sequence (per L5).
- Add a file-header note: the M18-9/M21-3/M22 shared-test-support-crate consolidation is now at **N≥8** backstops; it stays deferred by the standing risk-managed decision (do NOT extract it here).

- [ ] **Step 2: Run** — pre-build, then `cargo test -p envoy-bin http_filter_cors -- --nocapture`. Expected: PASS.

- [ ] **Step 3: clippy + fmt + commit**

```bash
cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git add crates/envoy-bin/tests/http_filter_cors.rs
git commit -m "phase 23 Task 9: in-process CORS backstop (preflight + decoration + disallowed/no-origin; ADR-0058 L7 absent-filter negative path)"
```

---

## Task 10: State-4 verification + STATE advance (phase close prep)

> Per `superpowers:verification-before-completion` + `BOOTSTRAP_PROMPT.md` §7.5. This task runs the full gate, quotes evidence into PROGRESS, then advances STATE to state-4-complete / state-5-next (the code-review session is separate). Do NOT skip the standalone-crate builds.

**Files:**
- Modify: `docs/envoy-rust/phases/23-http-filter-cors/PROGRESS.md` (quoted gate evidence)
- Modify: `docs/envoy-rust/STATE.md` (+ `STATE_HISTORY.md` relocation)

- [ ] **Step 1: Run the five stable-toolchain gates** and quote each into PROGRESS:
```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check          # no-op-delta: no new dependency this phase
```

- [ ] **Step 2: Standalone-crate builds** (per `project_isolated_crate_build_blindspot` — a green workspace build can mask a per-crate feature-unification failure):
```bash
cargo build -p envoy-config
cargo build -p envoy-filter
cargo build -p envoy-http1
cargo build -p envoy-http2
```

- [ ] **Step 3: Differential + conformance + fuzz** (pre-build helpers first; never concurrent with cargo builds, per `project_flaky_access_log_fixture_0012`). The headline gate (b): **all 31 fixtures (0001–0031) green simultaneously** at one CI run. Run the full Docker differential suite + h2spec (≥95%, verifying the H2 HCM ordering change is non-regressive) + the `parse_bootstrap` short-budget fuzz on the extended corpus. Quote the CI run URL + HEAD SHA + per-gate output into PROGRESS (the 05.3→22 evidence discipline). If the local Mac Docker run is used for pre-flight, the authoritative evidence is the Linux CI run (ADR-0049 Provenance).

- [ ] **Step 4: Advance STATE.md** to `23` state-4-complete / state-5-next; rewrite `## Next expected skill` to the state-5 code-review arc (`superpowers:requesting-code-review`); relocate the superseded state-2/3 narrative to `STATE_HISTORY.md` (ADR-0035 / §4.1 inv. 9, byte-for-byte); update `## Last commit` + `## Last updated`. Commit:

```bash
git add docs/envoy-rust/phases/23-http-filter-cors/PROGRESS.md docs/envoy-rust/STATE.md docs/envoy-rust/STATE_HISTORY.md
git commit -m "phase 23 Task 10: state-4 verification COMPLETE — §7.5 gates green; STATE→state-5-next"
```

---

## Self-review (against the SPEC, fresh eyes)

- **D1** (per-route schema) → Task 1 ✓ (with SC1 hand-rolled-deserializer correction). **D2** (HCM threading) → Tasks 4 (fan-out) + 5 (H1) + 6 (H2) ✓. **D3** (validator) → Task 2 ✓ (L7 fatal-reject). **D4** (CorsFilter) → Task 3 ✓. **D5** (variant + dispatch) → Task 4 ✓. **D6** (reserved-empty) → n/a ✓. **D7** (stats + contract) → Task 3 ✓. **D8.1** (fixture) → Task 7 ✓ (L6 cluster correction). **D8.2** (fuzz seed) → Task 8 ✓. **D8.3** (backstop) → Task 9 ✓. State-4 → Task 10 ✓.
- **Acceptance gates (a)-(f):** (a) fixture 0031 → Task 7/10; (b) all 30 prior fixtures green → Tasks 5/6 regression discipline + Task 10; (c) h2spec ≥95% → Task 6/10; (d) parse_bootstrap fuzz → Task 8/10; (e) the 5 stable gates → Task 10; (f) REVIEW.md → the separate state-5 session. ✓
- **Placeholder scan:** no TBD/TODO/"handle errors" placeholders; all code shown. The few "confirm against the codebase" notes (Counter accessor name, `HCMConfig` type name, `StringMatcherMode` visibility, the H2 unit-harness availability) are explicit verification steps, not deferred work. ✓
- **Type consistency:** `apply_route_config(Option<&envoy_config::Route>)` is identical across `FilterPipeline`/`HttpFilterInstance`/`CorsFilter`. `build_from_config(cfg, registry, hcm_stat_prefix)` matches the 3-arg dispatch. `CompiledCorsPolicy::from(&CorsPolicy)`, `resolve_route(&HCMConfig, &Request) -> Option<&Route>`, `build_preflight_response(&CompiledCorsPolicy, &str)` consistent. ✓
- **§6.1 split gate:** evaluated, single-phase (10 tasks, ~1100–1450 LoC). **ADR-0058** fires (L6 + L7). **ADR-0059** unfired. ✓

## Execution handoff

State-3 execution (the next session) uses **`superpowers:subagent-driven-development`** (the `feedback_execution_style` default), **SERIAL** dispatch (`feedback_serial_subagent_dispatch` — never parallel; the implementer subagents share `main`), fresh subagent per task, two-stage (spec-then-quality) review on the substantive tasks (the D2 HCM threading — Tasks 4/5/6 — and the D4 filter — Task 3 — are the review centerpieces), one code commit + one PROGRESS commit per task.
