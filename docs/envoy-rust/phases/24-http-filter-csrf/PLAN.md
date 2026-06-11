# Phase 24 (`24-http-filter-csrf`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (the project default per `feedback_execution_style`; SERIAL dispatch per `feedback_serial_subagent_dispatch` — never parallel) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. TDD per task (`superpowers:test-driven-development`). One code commit + one PROGRESS commit per task.

**Goal:** Land `envoy.filters.http.csrf` (decode-side cross-site-request-forgery protection — a modify-method source-vs-target origin guard returning a 403 on mismatch) as the eighth `HttpFilterInstance` variant, configured per-route via the phase-23 `typed_per_filter_config` infrastructure (the SECOND `PerFilterConfig` consumer, proving it generalizes additively).

**Architecture:** A single `CsrfPolicy` config message (`filter_enabled` REQUIRED + `additional_origins: [StringMatcher]`) is registered in `envoy-config` at BOTH the filter-chain level (`HttpFilterTypedConfig::Csrf`) and the per-route level (`PerFilterConfig::Csrf`). `CsrfFilter` (hand-rolled, in `envoy-filter`) compiles the **chain-level policy as a base** and, per-request, **replaces** it with the matched route's `CsrfPolicy` override (threaded via the phase-23 `apply_route_config` fan-out — NO HCM change). On decode, for `{POST,PUT,DELETE,PATCH}`, it compares the scheme-stripped `host[:port]` source origin (`Origin`, fallback `Referer`) against the target origin (`Host`/`:authority`) plus the `additional_origins` allow-list, short-circuiting a 403 `Invalid origin` local reply on mismatch (decorated by the existing H1/phase-11-H2 filter-synth helpers).

**Tech Stack:** Rust (pinned toolchain); `serde`/`serde_yaml` (config); the existing `envoy-filter` framework (07.1) + `StringMatcher` (04.x) + `FractionalPercent` (11) + `StatsRegistry` (06.x) + the H1 `decorate_filter_synth_response` / phase-11 H2 `decorate_filter_synth_response_h2` filter-synth helpers + the phase-23 per-route `apply_route_config` threading. **No new crate, no new dependency, no new fuzz target, no HCM change.**

---

## Scope check

Single subsystem (the `csrf` filter, consuming the phase-23 per-route-config infrastructure, both inside existing crates `envoy-config` + `envoy-filter`). **Single phase** — the §6.1 split gate did NOT fire (see "Split-gate decision" below). No sub-project decomposition needed.

## Split-gate decision (§6.1) — SINGLE PHASE, ADR-0062 unfired

The state-2 §6.2 empirical verification materialized the design as bounded. Unlike phase 23, phase 24 makes **NO shared-HCM-path change** (the route-early-resolution + `apply_route_config` threading landed at phase 23 and is reused verbatim — §0 of the SPEC). The §6.2-discovered divergences (the required `filter_enabled` / single `CsrfPolicy` type / the chain-base-route-replace effective-policy model / the scheme-stripped origin comparison) add modest surface (`RuntimeFractionalPercent` + the base-policy field + the `host_and_port` helper) but no cross-cutting risk. Refined estimate **~1000–1350 LoC / 7 tasks**, comfortably under both §6.1 thresholds (~1500 LoC / ~25 tasks). The split valve (`24.1` schema+wiring+validator / `24.2` filter+fixture) is **held in reserve but unused**; **ADR-0062 does NOT fire.**

## §6.2 empirical verification — LOCKED-IN findings (verified LOCALLY against `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd…`, 2026-06-11)

The full probe transcript is in the PROGRESS Task 1 preamble; the reconciliation is **ADR-0061** (three material divergences: L1 config shape, L3 origin computation, L6 chain-base/route-replace). The load-bearing lock-ins (each anchors a task):

- **L1 — config shape (item 1, DIVERGENCE → ADR-0061).** Envoy's CSRF filter uses ONE proto message `type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy` at BOTH levels. **`filter_enabled` is REQUIRED** (`--mode validate` → `CsrfPolicyValidationError.FilterEnabled: value is required` when absent at chain OR route level). The SPEC's near-empty `CsrfConfig {}` + `filter_enabled: Option<…>` is WRONG. Schema: `CsrfPolicy { filter_enabled: RuntimeFractionalPercent (required, NOT Option), additional_origins: Vec<StringMatcher> (default empty) }`; `RuntimeFractionalPercent { default_value: FractionalPercent (required), runtime_key: Option<String> }`. Both `HttpFilterTypedConfig::Csrf(CsrfPolicy)` and `PerFilterConfig::Csrf(CsrfPolicy)` carry the SAME `CsrfPolicy`. `additional_origins` elements are `StringMatcher`. `deny_unknown_fields` rejects `shadow_enabled`.
- **L2 — modify-method set (item 2, CONFIRMED).** `{POST,PUT,DELETE,PATCH}` guarded; `{GET,HEAD,OPTIONS,TRACE}` pass through (200, no stat). The guard runs only for the modify set; everything else is `Continue`.
- **L3 — origin computation (item 3, DIVERGENCE → ADR-0061).** Source AND target origins are reduced to **scheme-stripped `host[:port]`** (`Url::hostAndPort()`), NOT `scheme://host:port`. Verified: `Origin: http://additional.example.com` matches `exact: "additional.example.com"` (200) but NOT `exact: "http://additional.example.com:80"` (403) nor `exact: "additional.example.com:80"` (403); `suffix: "additional.example.com"` matches (200). So the source from `Origin: http://additional.example.com` is `additional.example.com` (no `:80` synthesized). Comparison: **source non-empty AND (source == target OR an `additional_origins` matcher matches source)** → valid. Source = `host_and_port(Origin)`, fallback `host_and_port(Referer)` (CONFIRMED: Referer-only same → 200, Referer-only evil → 403; `Origin` precedence over `Referer` CONFIRMED). Target = `host_and_port(Host`/`:authority)` (a bare `Host: localhost:10000` is used verbatim). **`host_and_port(v)`:** if `v` contains `"://"`, return the authority between `"://"` and the next `/` (or end); else return `v`.
- **L4 — 403 local reply (item 4, CONFIRMED byte-exact).** `403 Forbidden`, body `Invalid origin` (exactly **14 bytes, NO trailing newline**; `xxd`: `496e 7661 6c69 6420 6f72 6967 696e`), `content-type: text/plain`, `content-length: 14`, `server: envoy`, `date`. The H1 `decorate_filter_synth_response` (`hcm.rs:1454`) auto-adds `content-type: text/plain` for the non-empty body + stamps cl/server/date → `FilterResponse { status: 403, reason: Some("Forbidden"), headers: vec![], body: b"Invalid origin" }` reproduces Envoy byte-for-byte (the rbac `b"RBAC: access denied"` precedent verbatim).
- **L5 — stat namespace + semantics (item 5, CONFIRMED).** `http.<stat_prefix>.csrf.{request_valid, request_invalid, missing_source_origin}` (HCM-prefixed). MUTUALLY EXCLUSIVE, one tick per evaluated modify request: valid → `request_valid` only; present-but-disallowed source → `request_invalid` only; no source (no `Origin`, no `Referer`) → `missing_source_origin` only (NOT also `request_invalid`); safe methods → no stat. Controlled `{same,evil,additional,evil-GET,no-source}` POST/GET sequence yields `request_valid: 2, request_invalid: 1, missing_source_origin: 1`.
- **L6 — chain-base / route-replace model (item 6, DIVERGENCE → ADR-0061).** `filter_enabled` honored at deterministic 0%/100% (`numerator: 0` → passthrough; `numerator == denominator.value()` → enforce). **The chain-level `CsrfPolicy` is an always-applied BASE** — a route with NO override is STILL guarded by it (`/plain POST evil → 403`). **A per-route `CsrfPolicy` REPLACES the chain policy wholesale** (chain=100/route=0 → passthrough; chain=0/route=100 → enforce). Effective policy = route `CsrfPolicy` if present, else chain `CsrfPolicy`. This DIVERGES from the cors "inert when no route config" pattern → `CsrfFilter` compiles the chain policy as a base and falls back to it in `apply_route_config`. **envoy-rust disposition (ADR-0049 all-fatal):** reject non-deterministic `default_value` (`numerator ∉ {0, denominator.value()}`); reject a present `runtime_key` (no RTDS to honor it); reject `shadow_enabled` via `deny_unknown_fields` — all at BOTH chain + route level.
- **L7 — policy-for-absent-filter (item 7, CONFIRMED reuse).** The generic `PerRouteConfigForAbsentFilter` validator (`bootstrap.rs:2696`, iterating `route.typed_per_filter_config.keys()`) already rejects a `csrf` per-route policy with no `csrf` filter in the chain — reused verbatim, no new code. The no-override case falls back to the chain policy (L6), not passthrough.
- **L8 — fixture topology (the ADR-0058 L6 constraint, CONFIRMED).** Fixture 0032 proxies to a REAL upstream cluster (`http1-echo-server` helper); a valid CSRF modify request must reach an upstream to yield 200 (`direct_response` does not engage per-route filter config).

**ADR-0061 fires** at the PLAN-write commit (this commit) for L1 + L3 + L6. **ADR-0062 (split) does NOT fire.**

## PLAN-time SPEC corrections (verified by read-only recon at HEAD `bb58319ea`)

The SPEC's source anchors are accurate EXCEPT these, which the implementer must heed:

- **SC1 — `PerFilterConfig` is DERIVE-based, not hand-rolled.** `PerFilterConfig` (`bootstrap.rs:785-790`) is `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(tag = "@type")]` with one `Cors(CorsPolicy)` variant. Adding `Csrf(CsrfPolicy)` is a one-variant change — NO deserializer surgery (the SPEC §0 fact-2 worry about hand-rolled parsing is moot; the `Route` deserializer at `bootstrap.rs:1397` already parses the `typed_per_filter_config:` map into `BTreeMap<String, PerFilterConfig>` generically). **BUT** adding the variant breaks two exhaustive consumers (fix in Task 1): (a) `crates/envoy-filter/src/cors.rs:118-120` matches `match pfc { PerFilterConfig::Cors(p) => … }`; (b) `crates/envoy-config/src/bootstrap.rs:12908` has an irrefutable `let PerFilterConfig::Cors(p) = pfc;` in a test.
- **SC2 — `HttpFilterTypedConfig::Csrf` breaks TWO exhaustive matches; land it in the wiring task (Task 3), not Task 1.** Adding `HttpFilterTypedConfig::Csrf(CsrfPolicy)` (the chain variant at `bootstrap.rs:741-768`, currently 7 variants ending `Cors(CorsConfig)`) breaks (a) the `build` dispatch in `crates/envoy-filter/src/instance.rs:101-122` (matches `HttpFilterTypedConfig` exhaustively) and (b) the per-filter validator loop in `crates/envoy-config/src/bootstrap.rs:2803-2858` (matches `HttpFilterTypedConfig` exhaustively, each arm name-checking `f.name`). Because the `instance.rs` build arm needs `CsrfFilter` to exist, the chain variant lands in Task 3 (wiring) alongside `CsrfFilter` + the validator arm. **Task 1 adds only `PerFilterConfig::Csrf` (per-route) + the `CsrfPolicy`/`RuntimeFractionalPercent` structs.**
- **SC3 — `FractionalPercent` exists; `RuntimeFractionalPercent` does NOT.** `FractionalPercent { numerator: u32, denominator: DenominatorType (default HUNDRED) }` at `bootstrap.rs:649` has `selects_deterministic(&self) -> bool` (`numerator == denominator.value()`) — reuse it as `RuntimeFractionalPercent.default_value`. `RuntimeFractionalPercent { default_value: FractionalPercent, runtime_key: Option<String> }` is NEW (Task 1). YAML shape (verified): `filter_enabled: { default_value: { numerator: 100, denominator: HUNDRED }, runtime_key: "..." }`.
- **SC4 — `StringMatcher.matches(&str) -> bool`** at `crates/envoy-config/src/matcher.rs:58`; `StringMatcher { mode: StringMatcherMode, ignore_case: bool }` with `StringMatcherMode::Exact/Prefix/Suffix/SafeRegex/Contains`. Hand-rolled `Deserialize` at `bootstrap.rs:1868`. Reuse verbatim for `additional_origins` — no changes. (NB the matcher matches the scheme-stripped `host[:port]` source per L3.)
- **SC5 — `header_ci` is duplicated, now reaching N=3.** Private 5-line helpers at `crates/envoy-filter/src/jwt_authn.rs:156` and `cors.rs:236`. `csrf.rs` needs the same case-insensitive lookup over `Origin`/`Referer`/`Host`. Duplicate the 5-line helper into `csrf.rs` (the cheap, low-risk choice — the shared-util extraction is the standing-deferred M-track consolidation item; do NOT expand scope). Note the N=3 duplication in the `csrf.rs` module header.
- **SC6 — `FilterRequest`/`FilterResponse`/`Decision` shapes.** `FilterRequest { method: String, path: String, headers: Vec<(String,String)>, body: Option<Bytes> }`; `FilterResponse { status: u16, reason: Option<&'static str>, headers: Vec<(String,String)>, body: Bytes }` (from `crates/envoy-filter/src/types.rs`, used at `cors.rs:296`/`rbac.rs:188`). `Decision::{Continue, StopAndSend(FilterResponse)}` (`crates/envoy-filter/src/pipeline.rs`). CSRF is decode-only → `encode_headers` is the default `Continue` arm (no method needed; the `instance.rs` encode dispatch does NOT need a Csrf arm — confirm at Task 3 the encode match remains exhaustive, see SC7).
- **SC7 — `instance.rs` dispatch arms to add (Task 3).** `HttpFilterInstance` (`instance.rs:32`) has variants `Router/HeaderMutation/LocalRateLimit/Rbac/Fault/JwtAuthn/Cors(CorsFilter)` (+ test-only). The `build` match (`:101`) and `decode_headers` match (`:127`) and `apply_route_config` (`:167`, currently `if let HttpFilterInstance::Cors(f) = self`) all gain a `Csrf` arm. The `encode_headers` match (`:145`) gains a `Csrf(f) => f.encode_headers(resp)` arm **only because the match is exhaustive** — `CsrfFilter::encode_headers` returns `Continue` (decode-only filter; provide the trivial method to satisfy the exhaustive match, mirroring rbac's no-op encode at `rbac.rs:200`).
- **SC8 — the validator name-check arm (Task 3).** The per-filter loop at `bootstrap.rs:2852` shows the `Cors` arm pattern: `HttpFilterTypedConfig::Csrf(cfg) => { if f.name != "envoy.filters.http.csrf" { return Err(UnsupportedHttpFilter { name }) } validate_csrf_config(cfg, listener_name)?; }`. There is NO separate unsupported-filter reject-list to edit (the `@type`-tagged `deny_unknown_fields` enum rejects unknown `@type`s at deserialize; the name-check arm rejects a name/type mismatch). `validate_csrf_config` enforces the L6 dispositions (non-deterministic `default_value` + present `runtime_key` → fatal).

---

## File structure

| File | Responsibility | Task |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | `RuntimeFractionalPercent`, `CsrfPolicy`; `PerFilterConfig::Csrf` (T1); `HttpFilterTypedConfig::Csrf` + `validate_csrf_config` + route-level CsrfPolicy validation + the validator name-check arm (T3) | 1, 3 |
| `crates/envoy-config/src/lib.rs` | re-export `CsrfPolicy`/`RuntimeFractionalPercent`; new `ConfigError` variants (`UnsupportedNonDeterministicCsrfFilterEnabled`, `UnsupportedRuntimeKeyedCsrfFilterEnabled`) | 1, 3 |
| `crates/envoy-filter/src/cors.rs` | fix the `apply_route_config` match to remain exhaustive over `PerFilterConfig` (return `None` for the non-Cors arm) | 1 |
| `crates/envoy-filter/src/csrf.rs` (CREATE) | `CsrfFilter` + `CompiledCsrfPolicy` (chain-base + route-replace, origin compute, modify-method guard, 403 short-circuit, stats, `apply_route_config`, local `header_ci` + `host_and_port`) | 2 |
| `crates/envoy-filter/src/lib.rs` | re-export `CsrfFilter`; `mod csrf;` | 2 |
| `crates/envoy-filter/src/instance.rs` | `HttpFilterInstance::Csrf` variant + build/decode/encode/`apply_route_config` dispatch arms | 3 |
| `tests/fixtures/0032-http-filter-csrf/` (CREATE) | fixture (envoy.yaml / envoy-rust.yaml / inputs / expectations.yaml / README.md) | 4 |
| `tests/differential/src/lib.rs` | `Http1Method::Post` (+ `as_str` arm) | 4 |
| `tests/differential/tests/http_filter_csrf.rs` (CREATE) | Docker-gated wrapper | 4 |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_csrf_typed_per_filter_config.yaml` (CREATE) | fuzz seed | 5 |
| `crates/envoy-bin/tests/http_filter_csrf.rs` (CREATE) | in-process backstop | 6 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | csrf stat rows + 403 status/`Invalid origin` body row + scheme-stripped-origin + chain-base/route-replace + absent-filter divergence notes | 2 |

---

## Task 1: D1 — `CsrfPolicy` schema + `PerFilterConfig::Csrf` (per-route variant)

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (new types near the `PerFilterConfig`/`CorsPolicy` block ~785-821; add the `Csrf` variant to `PerFilterConfig` ~787)
- Modify: `crates/envoy-config/src/lib.rs` (re-export `CsrfPolicy`, `RuntimeFractionalPercent` — match the `CorsPolicy` re-export pattern)
- Modify: `crates/envoy-filter/src/cors.rs` (fix the `apply_route_config` match to stay exhaustive over `PerFilterConfig` — SC1)
- Modify: `crates/envoy-config/src/bootstrap.rs` test at `:12908` (irrefutable-`let` fix — SC1)

- [ ] **Step 1: Write failing tests** in `bootstrap.rs` `#[cfg(test)]` mod (mirror the `route_parses_typed_per_filter_config_cors` test):

```rust
#[test]
fn csrf_policy_parses_filter_enabled_and_additional_origins() {
    let yaml = r#"
filter_enabled:
  default_value: { numerator: 100, denominator: HUNDRED }
additional_origins:
- exact: "additional.example.com"
- suffix: ".trusted.example.com"
"#;
    let p: CsrfPolicy = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(p.filter_enabled.default_value.numerator, 100);
    assert!(p.filter_enabled.default_value.selects_deterministic());
    assert_eq!(p.filter_enabled.runtime_key, None);
    assert_eq!(p.additional_origins.len(), 2);
    assert!(p.additional_origins[0].matches("additional.example.com"));
}

#[test]
fn csrf_policy_requires_filter_enabled() {
    // filter_enabled has no #[serde(default)] → absence is a parse error.
    let yaml = r#"
additional_origins:
- exact: "additional.example.com"
"#;
    assert!(serde_yaml::from_str::<CsrfPolicy>(yaml).is_err());
}

#[test]
fn csrf_policy_rejects_shadow_enabled() {
    let yaml = r#"
filter_enabled:
  default_value: { numerator: 100, denominator: HUNDRED }
shadow_enabled:
  default_value: { numerator: 100, denominator: HUNDRED }
"#;
    assert!(serde_yaml::from_str::<CsrfPolicy>(yaml).is_err(), "deny_unknown_fields must reject shadow_enabled");
}

#[test]
fn route_parses_typed_per_filter_config_csrf() {
    let yaml = r#"
match: { prefix: "/" }
route: { cluster: backend }
typed_per_filter_config:
  envoy.filters.http.csrf:
    "@type": type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy
    filter_enabled:
      default_value: { numerator: 100, denominator: HUNDRED }
    additional_origins:
    - exact: "additional.example.com"
"#;
    let route: Route = serde_yaml::from_str(yaml).expect("parses");
    let pfc = route.typed_per_filter_config.get("envoy.filters.http.csrf").expect("csrf pfc present");
    match pfc {
        PerFilterConfig::Csrf(p) => {
            assert!(p.filter_enabled.default_value.selects_deterministic());
            assert_eq!(p.additional_origins.len(), 1);
        }
        other => panic!("expected Csrf, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config csrf_policy 2>&1 | tail -20`
Expected: FAIL — `CsrfPolicy`/`RuntimeFractionalPercent`/`PerFilterConfig::Csrf` undefined.

- [ ] **Step 3: Add the schema types + the per-route variant** in `bootstrap.rs` (adjacent to `CorsPolicy` at ~798):

```rust
/// `envoy.config.core.v3.RuntimeFractionalPercent`. A `default_value`
/// percentage plus an optional `runtime_key`. The csrf filter's `filter_enabled`
/// is of this type (REQUIRED — §6.2/ADR-0061 L1). envoy-rust honors only the
/// deterministic 0%/100% `default_value`; a present `runtime_key` is rejected
/// (no RTDS runtime layer — ADR-0061 L6, the ADR-0049 all-fatal posture).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFractionalPercent {
    pub default_value: FractionalPercent,
    #[serde(default)]
    pub runtime_key: Option<String>,
}

/// `envoy.extensions.filters.http.csrf.v3.CsrfPolicy` (phase 24, minimum-viable
/// per ADR-0060/0061). The SAME message is used at the filter-chain level
/// (`HttpFilterTypedConfig::Csrf`) AND the per-route level (`PerFilterConfig::Csrf`)
/// — ADR-0061 L1. `filter_enabled` is REQUIRED (no `#[serde(default)]`).
/// `additional_origins` reuses the 04.x `StringMatcher` and is matched against
/// the scheme-stripped `host[:port]` source origin (ADR-0061 L3). The deferred
/// `shadow_enabled` is rejected by `deny_unknown_fields`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CsrfPolicy {
    pub filter_enabled: RuntimeFractionalPercent,
    #[serde(default)]
    pub additional_origins: Vec<StringMatcher>,
}
```

Add the variant to `PerFilterConfig` (at `:787`, after the `Cors` variant):

```rust
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy")]
    Csrf(CsrfPolicy),
```

- [ ] **Step 4: Fix the two exhaustive `PerFilterConfig::Cors` consumers (SC1)**

In `crates/envoy-filter/src/cors.rs` `apply_route_config` (~`:116-121`), change the `.map(|pfc| match pfc { … })` so the match stays exhaustive — only the cors-keyed entry is ever a `Cors`, so map non-Cors to `None`:

```rust
    pub(crate) fn apply_route_config(&mut self, route: Option<&envoy_config::Route>) {
        self.active_policy = route
            .and_then(|r| r.typed_per_filter_config.get(CORS_FILTER_NAME))
            .and_then(|pfc| match pfc {
                envoy_config::PerFilterConfig::Cors(p) => Some(CompiledCorsPolicy::from(p)),
                _ => None,
            });
    }
```

In `crates/envoy-config/src/bootstrap.rs:12908`, change the irrefutable `let PerFilterConfig::Cors(p) = pfc;` to a refutable match:

```rust
    let PerFilterConfig::Cors(p) = pfc else { panic!("expected Cors variant") };
```

- [ ] **Step 5: Re-export the new types** in `crates/envoy-config/src/lib.rs` (next to the `CorsPolicy` re-export):

```rust
pub use bootstrap::{CsrfPolicy, RuntimeFractionalPercent};
```

- [ ] **Step 6: Run the schema tests + the WORKSPACE build (the exhaustive-match gate — `project_isolated_crate_build_blindspot`)**

Run: `cargo test -p envoy-config csrf_policy route_parses_typed_per_filter_config_csrf 2>&1 | tail -15`
Expected: PASS (4 tests).
Run: `cargo build --workspace 2>&1 | tail -5`
Expected: clean — confirms the `cors.rs` + `:12908` fixes kept every `PerFilterConfig` consumer exhaustive.
Run: `cargo test -p envoy-config 2>&1 | tail -5` (full crate — confirm no cors regression).
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs crates/envoy-filter/src/cors.rs
git commit -m "phase 24 Task 1: CsrfPolicy + RuntimeFractionalPercent schema + PerFilterConfig::Csrf [ADR-0061]"
```

---

## Task 2: D2 + D4 — `CsrfFilter` runtime + stats + BEHAVIOR_CONTRACT (unwired)

**Files:**
- Create: `crates/envoy-filter/src/csrf.rs`
- Modify: `crates/envoy-filter/src/lib.rs` (`mod csrf;` + `pub use csrf::CsrfFilter;`)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (csrf stat rows + 403/`Invalid origin` body row + scheme-stripped-origin + chain-base/route-replace notes)

The filter is built + unit-tested but NOT wired into `HttpFilterInstance` here (the `cors.rs` Task-3-scope precedent — the variant + dispatch land in Task 3). Suppress dead-code lints on the public items until Task 3 activates them (mirror the `cors.rs` module-header `#[allow]` posture if the build warns).

- [ ] **Step 1: Write the failing unit tests** in `csrf.rs` `#[cfg(test)]` mod. Cover: chain-base enforcement (no route override), route-replace (override 0% disables a 100% chain; override 100% enables a 0% chain), modify-method guard (POST guarded, GET passthrough), origin compute (same-origin valid, evil invalid, additional allowed, Referer fallback, Origin-precedence, missing-source), the 403 body bytes, the mutually-exclusive stats, and `host_and_port`. Skeleton (full bodies in the implementer's TDD pass — these are the load-bearing assertions):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{CsrfPolicy, RuntimeFractionalPercent, FractionalPercent, DenominatorType,
                       StringMatcher, StringMatcherMode};
    use envoy_stats::StatsRegistry;

    fn fe(n: u32) -> RuntimeFractionalPercent {
        RuntimeFractionalPercent { default_value: FractionalPercent { numerator: n, denominator: DenominatorType::Hundred }, runtime_key: None }
    }
    fn policy(n: u32, addl: &[&str]) -> CsrfPolicy {
        CsrfPolicy { filter_enabled: fe(n),
            additional_origins: addl.iter().map(|s| StringMatcher { mode: StringMatcherMode::Exact(s.to_string()), ignore_case: false }).collect() }
    }
    fn reg() -> Arc<StatsRegistry> { Arc::new(StatsRegistry::new()) }
    fn req(method: &str, headers: &[(&str,&str)]) -> FilterRequest {
        FilterRequest { method: method.into(), path: "/".into(),
            headers: headers.iter().map(|(k,v)|(k.to_string(),v.to_string())).collect(), body: None }
    }
    fn cval(r:&Arc<StatsRegistry>,s:&str)->u64 { r.register_counter(&format!("http.ingress_http.csrf.{s}")).unwrap().value() }

    // host_and_port (ADR-0061 L3)
    #[test] fn host_and_port_strips_scheme() {
        assert_eq!(host_and_port("http://additional.example.com"), "additional.example.com");
        assert_eq!(host_and_port("http://localhost:10000"), "localhost:10000");
        assert_eq!(host_and_port("http://localhost:10000/page?q=1"), "localhost:10000");
        assert_eq!(host_and_port("localhost:10000"), "localhost:10000"); // bare Host, used verbatim
        assert_eq!(host_and_port(""), "");
    }

    // chain-base: route WITHOUT override is guarded by the chain policy (L6)
    #[test] fn chain_base_guards_without_route_override() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(100, &[]), &r, "ingress_http").unwrap();
        f.apply_route_config(None); // no route override → chain base applies
        let d = f.decode_headers(&mut req("POST", &[("host","localhost:10000"),("origin","http://evil.example.com")]));
        match d { Decision::StopAndSend(resp) => { assert_eq!(resp.status, 403); assert_eq!(&resp.body[..], b"Invalid origin"); }
                  _ => panic!("expected 403") }
        assert_eq!(cval(&r,"request_invalid"), 1);
    }

    // route-replace: chain=100 + route override 0% → passthrough (L6)
    #[test] fn route_override_zero_disables_enforcing_chain() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(100, &[]), &r, "ingress_http").unwrap();
        let route = route_with_csrf(policy(0, &[]));
        f.apply_route_config(Some(&route));
        assert!(matches!(f.decode_headers(&mut req("POST", &[("host","h"),("origin","http://evil")])), Decision::Continue));
    }

    // route-replace: chain=0 + route override 100% → enforce (L6)
    #[test] fn route_override_hundred_enables_disabled_chain() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(0, &[]), &r, "ingress_http").unwrap();
        let route = route_with_csrf(policy(100, &[]));
        f.apply_route_config(Some(&route));
        assert!(matches!(f.decode_headers(&mut req("POST", &[("host","h"),("origin","http://evil")])), Decision::StopAndSend(_)));
    }

    // safe methods passthrough, no stat (L2)
    #[test] fn safe_methods_pass_without_stat() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(100, &[]), &r, "ingress_http").unwrap();
        f.apply_route_config(None);
        for m in ["GET","HEAD","OPTIONS","TRACE"] {
            assert!(matches!(f.decode_headers(&mut req(m, &[("host","h"),("origin","http://evil")])), Decision::Continue), "{m}");
        }
        assert_eq!(cval(&r,"request_valid"),0); assert_eq!(cval(&r,"request_invalid"),0); assert_eq!(cval(&r,"missing_source_origin"),0);
    }

    // modify set guarded (L2)
    #[test] fn modify_methods_guarded() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(100, &[]), &r, "ingress_http").unwrap();
        f.apply_route_config(None);
        for m in ["POST","PUT","DELETE","PATCH"] {
            assert!(matches!(f.decode_headers(&mut req(m, &[("host","h"),("origin","http://evil")])), Decision::StopAndSend(_)), "{m}");
        }
    }

    // same-origin valid; additional allowed; Referer fallback; Origin precedence; missing-source (L3,L5)
    #[test] fn origin_matrix_and_mutually_exclusive_stats() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(100, &["additional.example.com"]), &r, "ingress_http").unwrap();
        f.apply_route_config(None);
        let host = ("host","localhost:10000");
        assert!(matches!(f.decode_headers(&mut req("POST", &[host,("origin","http://localhost:10000")])), Decision::Continue)); // same
        assert!(matches!(f.decode_headers(&mut req("POST", &[host,("origin","http://additional.example.com")])), Decision::Continue)); // additional
        assert!(matches!(f.decode_headers(&mut req("POST", &[host,("referer","http://localhost:10000/p")])), Decision::Continue)); // referer fallback same
        assert!(matches!(f.decode_headers(&mut req("POST", &[host,("origin","http://localhost:10000"),("referer","http://evil/p")])), Decision::Continue)); // origin precedence
        assert!(matches!(f.decode_headers(&mut req("POST", &[host,("referer","http://evil/p")])), Decision::StopAndSend(_))); // referer evil
        assert!(matches!(f.decode_headers(&mut req("POST", &[host])), Decision::StopAndSend(_))); // missing source
        assert_eq!(cval(&r,"request_valid"), 4);
        assert_eq!(cval(&r,"request_invalid"), 1);
        assert_eq!(cval(&r,"missing_source_origin"), 1);
    }

    // deterministic-0% chain → passthrough (L6)
    #[test] fn filter_enabled_zero_passes_through() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(0, &[]), &r, "ingress_http").unwrap();
        f.apply_route_config(None);
        assert!(matches!(f.decode_headers(&mut req("POST", &[("host","h"),("origin","http://evil")])), Decision::Continue));
    }
}
```

(The implementer adds a `route_with_csrf(p: CsrfPolicy) -> envoy_config::Route` test helper mirroring `cors.rs`'s `apply_route_config_sets_policy_from_route` route construction, inserting the policy under the `"envoy.filters.http.csrf"` key.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-filter csrf 2>&1 | tail -20`
Expected: FAIL — `csrf` module / `CsrfFilter` undefined.

- [ ] **Step 3: Implement `csrf.rs`** (mirror `cors.rs`'s structure; the chain-base/route-replace is the key difference):

```rust
//! `envoy.filters.http.csrf` — decode-side cross-site-request-forgery guard.
//!
//! §6.2-verified against envoyproxy/envoy:v1.33.0 (phase-24 PLAN-write; ADR-0061).
//!
//! ## Behaviour summary
//! - The chain-level `CsrfPolicy` is an always-applied BASE; a per-route
//!   `CsrfPolicy` (threaded via `apply_route_config`) REPLACES it wholesale
//!   (ADR-0061 L6). The effective policy's `filter_enabled` gates enforcement.
//! - For `{POST,PUT,DELETE,PATCH}` (the modify set, L2): compute the
//!   scheme-stripped `host[:port]` source origin (`Origin`, fallback `Referer`)
//!   vs target (`Host`/`:authority`); valid iff source == target OR an
//!   `additional_origins` matcher matches source (L3). Invalid / missing-source
//!   → 403 `Invalid origin` (L4). Safe methods + deterministic-0% → Continue.
//! - Decode-side only; `encode_headers` is the trivial `Continue` arm.
//!
//! `header_ci` is duplicated from jwt_authn/cors (now N=3); the shared-util
//! extraction stays deferred (the standing M-track consolidation item).
use std::sync::Arc;

use bytes::Bytes;
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

const CSRF_FILTER_NAME: &str = "envoy.filters.http.csrf";
const MODIFY_METHODS: &[&str] = &["POST", "PUT", "DELETE", "PATCH"];
const FAILURE_BODY: &[u8] = b"Invalid origin"; // 14 bytes, no newline (ADR-0061 L4)

/// Build-time-lowered `CsrfPolicy`. `enabled` collapses `filter_enabled` to the
/// deterministic boolean (validated 0%/100% — ADR-0061 L6).
#[derive(Debug, Clone)]
struct CompiledCsrfPolicy {
    enabled: bool,
    additional_origins: Vec<envoy_config::StringMatcher>,
}

impl From<&envoy_config::CsrfPolicy> for CompiledCsrfPolicy {
    fn from(p: &envoy_config::CsrfPolicy) -> Self {
        Self {
            enabled: p.filter_enabled.default_value.selects_deterministic(),
            additional_origins: p.additional_origins.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CsrfFilter {
    request_valid: Arc<Counter>,
    request_invalid: Arc<Counter>,
    missing_source_origin: Arc<Counter>,
    /// Compiled chain-level policy (the always-applied BASE, ADR-0061 L6).
    base_policy: CompiledCsrfPolicy,
    /// The effective policy for the current request: the route override if the
    /// matched route carries one, else a clone of `base_policy`.
    active_policy: CompiledCsrfPolicy,
}

impl CsrfFilter {
    pub(crate) fn build_from_config(
        cfg: &envoy_config::CsrfPolicy,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        let reg = |suffix: &str| {
            registry
                .register_counter(&format!("http.{hcm_stat_prefix}.csrf.{suffix}"))
                .map_err(|e| FilterError::InvalidConfig { message: format!("StatsRegistry: {e}") })
        };
        let base = CompiledCsrfPolicy::from(cfg);
        Ok(Self {
            request_valid: reg("request_valid")?,
            request_invalid: reg("request_invalid")?,
            missing_source_origin: reg("missing_source_origin")?,
            active_policy: base.clone(),
            base_policy: base,
        })
    }

    /// Select the effective per-request policy (ADR-0061 L6): the route's
    /// `CsrfPolicy` override if present, else the chain-level base.
    pub(crate) fn apply_route_config(&mut self, route: Option<&envoy_config::Route>) {
        self.active_policy = route
            .and_then(|r| r.typed_per_filter_config.get(CSRF_FILTER_NAME))
            .and_then(|pfc| match pfc {
                envoy_config::PerFilterConfig::Csrf(p) => Some(CompiledCsrfPolicy::from(p)),
                _ => None,
            })
            .unwrap_or_else(|| self.base_policy.clone());
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        if !self.active_policy.enabled {
            return Decision::Continue; // deterministic-0% (L6)
        }
        if !MODIFY_METHODS.iter().any(|m| req.method == *m) {
            return Decision::Continue; // safe method (L2) — no stat
        }
        // Source origin: Origin, fallback Referer; reduced to host[:port] (L3).
        let source = header_ci(&req.headers, "origin")
            .or_else(|| header_ci(&req.headers, "referer"))
            .map(host_and_port)
            .filter(|s| !s.is_empty());
        let Some(source) = source else {
            self.missing_source_origin.inc();
            return Decision::StopAndSend(failure_response());
        };
        let target = header_ci(&req.headers, "host").map(host_and_port).unwrap_or("");
        let allowed = source == target
            || self.active_policy.additional_origins.iter().any(|m| m.matches(source));
        if allowed {
            self.request_valid.inc();
            Decision::Continue
        } else {
            self.request_invalid.inc();
            Decision::StopAndSend(failure_response())
        }
    }

    /// CSRF is decode-side only; encode is a no-op (the exhaustive-match arm, SC7).
    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }
}

fn failure_response() -> FilterResponse {
    FilterResponse {
        status: 403,
        reason: Some("Forbidden"),
        headers: Vec::new(),
        body: Bytes::from_static(FAILURE_BODY),
    }
}

/// Reduce an origin/host value to the scheme-stripped `host[:port]` authority
/// (Envoy `Url::hostAndPort()` semantics, ADR-0061 L3). If the value carries a
/// `scheme://` prefix, return the authority up to the next `/` (or end);
/// otherwise return the value unchanged (a bare `Host: h:p` is already an
/// authority). Borrowing — no allocation.
fn host_and_port(value: &str) -> &str {
    match value.split_once("://") {
        Some((_scheme, rest)) => rest.split(['/', '?', '#']).next().unwrap_or(""),
        None => value,
    }
}

/// Case-insensitive header lookup — duplicated from jwt_authn/cors per SC5 (N=3).
fn header_ci<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}
```

Add to `crates/envoy-filter/src/lib.rs`: `mod csrf;` + `pub use csrf::CsrfFilter;` (match the `cors` re-export).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-filter csrf 2>&1 | tail -15`
Expected: PASS (all csrf tests).
Run: `cargo build --workspace 2>&1 | tail -3`
Expected: clean (new module; `CsrfFilter` unwired but re-exported).

- [ ] **Step 5: Extend BEHAVIOR_CONTRACT.md** — under `Stat-name mapping` add the 3 csrf rows; under `Local reply` (or the existing filter-body section) add the `403 Forbidden` / `Invalid origin` (14 bytes, `content-type: text/plain`) row; add a note that csrf origin comparison is on scheme-stripped `host[:port]` (ADR-0061 L3), that the chain-level policy is an always-applied base a route policy replaces (L6), and the `PerRouteConfigForAbsentFilter` divergence applies to csrf (L7). Mirror the cors rows landed at phase 23.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-filter/src/csrf.rs crates/envoy-filter/src/lib.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 24 Task 2: CsrfFilter runtime + stats + BEHAVIOR_CONTRACT (unwired) [ADR-0061]"
```

---

## Task 3: D3 — `HttpFilterTypedConfig::Csrf` + `HttpFilterInstance::Csrf` wiring + validator

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`HttpFilterTypedConfig::Csrf(CsrfPolicy)` variant ~`:768`; `validate_csrf_config` fn; the validator name-check arm ~`:2858`; route-level CsrfPolicy validation in the route-walk)
- Modify: `crates/envoy-config/src/lib.rs` (new `ConfigError` variants)
- Modify: `crates/envoy-filter/src/instance.rs` (`HttpFilterInstance::Csrf` + build/decode/encode/`apply_route_config` arms)

- [ ] **Step 1: Write failing tests.** In `bootstrap.rs` tests — config acceptance + the L6 rejections:

```rust
#[test]
fn hcm_accepts_csrf_filter_chain_entry() {
    // A bootstrap with name=envoy.filters.http.csrf + a CsrfPolicy typed_config
    // (filter_enabled 100%) + a router terminus parses + validates clean.
    let bootstrap = bootstrap_with_csrf_chain(/* filter_enabled */ 100, /* runtime_key */ None);
    assert!(validate_bootstrap(&bootstrap).is_ok());
}

#[test]
fn rejects_non_deterministic_csrf_filter_enabled() {
    let bootstrap = bootstrap_with_csrf_chain(50, None); // numerator 50 of HUNDRED
    assert!(matches!(validate_bootstrap(&bootstrap),
        Err(ConfigError::UnsupportedNonDeterministicCsrfFilterEnabled { .. })));
}

#[test]
fn rejects_runtime_keyed_csrf_filter_enabled() {
    let bootstrap = bootstrap_with_csrf_chain(100, Some("csrf.enabled".into()));
    assert!(matches!(validate_bootstrap(&bootstrap),
        Err(ConfigError::UnsupportedRuntimeKeyedCsrfFilterEnabled { .. })));
}

#[test]
fn rejects_csrf_per_route_policy_for_absent_filter() {
    // A route carrying a csrf typed_per_filter_config but NO csrf filter in the
    // chain → the existing PerRouteConfigForAbsentFilter validator fires (L7).
    let bootstrap = bootstrap_csrf_route_without_chain_filter();
    assert!(matches!(validate_bootstrap(&bootstrap),
        Err(ConfigError::PerRouteConfigForAbsentFilter { ref filter }) if filter == "envoy.filters.http.csrf"));
}

#[test]
fn rejects_non_deterministic_csrf_route_override() {
    // A route-level CsrfPolicy with a fractional filter_enabled is also fatal.
    let bootstrap = bootstrap_csrf_route_override(50, None);
    assert!(matches!(validate_bootstrap(&bootstrap),
        Err(ConfigError::UnsupportedNonDeterministicCsrfFilterEnabled { .. })));
}
```

In `instance.rs` tests — the build + dispatch (mirror `builds_jwt_authn_instance_and_dispatches`):

```rust
#[test]
fn builds_csrf_instance_and_dispatches() {
    let registry = Arc::new(envoy_stats::StatsRegistry::new());
    let hf = /* HttpFilter { name: "envoy.filters.http.csrf", typed_config: HttpFilterTypedConfig::Csrf(csrf_policy_100()) } */;
    let instance = HttpFilterInstance::build(&hf, &registry, "ingress_http").expect("build");
    assert!(matches!(instance, HttpFilterInstance::Csrf(_)));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p envoy-config csrf 2>&1 | tail; cargo test -p envoy-filter builds_csrf 2>&1 | tail`
Expected: FAIL — `HttpFilterTypedConfig::Csrf` / `validate_csrf_config` / `ConfigError::Unsupported*Csrf*` / `HttpFilterInstance::Csrf` undefined.

- [ ] **Step 3: Add the chain variant + ConfigError variants + validator.** In `bootstrap.rs` `HttpFilterTypedConfig` (after `Cors(CorsConfig)` at `:767`):

```rust
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy")]
    Csrf(CsrfPolicy),
```

In `lib.rs` `ConfigError`:

```rust
    /// 24 D3 (ADR-0061 L6): a csrf `filter_enabled.default_value` is neither 0%
    /// nor 100%. envoy-rust honors only deterministic gating (the phase-11 fault
    /// precedent); fractional gating needs the unimplemented RTDS runtime layer.
    #[error("csrf filter_enabled on listener `{listener}` is non-deterministic (numerator must be 0 or the denominator value)")]
    UnsupportedNonDeterministicCsrfFilterEnabled { listener: String },

    /// 24 D3 (ADR-0061 L6): a csrf `filter_enabled.runtime_key` is present.
    /// envoy-rust has no RTDS runtime layer to honor it (the ADR-0049 all-fatal posture).
    #[error("csrf filter_enabled on listener `{listener}` has a runtime_key, which requires the unimplemented RTDS runtime layer")]
    UnsupportedRuntimeKeyedCsrfFilterEnabled { listener: String },
```

Add `validate_csrf_config` (near `validate_fault_config`):

```rust
fn validate_csrf_config(cfg: &crate::CsrfPolicy, listener: &str) -> Result<(), crate::ConfigError> {
    let fe = &cfg.filter_enabled;
    if fe.runtime_key.is_some() {
        return Err(crate::ConfigError::UnsupportedRuntimeKeyedCsrfFilterEnabled { listener: listener.to_string() });
    }
    let p = &fe.default_value;
    if p.numerator != 0 && p.numerator != p.denominator.value() {
        return Err(crate::ConfigError::UnsupportedNonDeterministicCsrfFilterEnabled { listener: listener.to_string() });
    }
    Ok(())
}
```

Add the validator name-check arm in the per-filter loop (after the `Cors` arm at `:2852`):

```rust
            crate::HttpFilterTypedConfig::Csrf(cfg) => {
                if f.name != "envoy.filters.http.csrf" {
                    return Err(crate::ConfigError::UnsupportedHttpFilter { name: f.name.clone() });
                }
                validate_csrf_config(cfg, listener_name)?;
            }
```

Also validate the **route-level** csrf overrides: in the route-walk that already runs `PerRouteConfigForAbsentFilter` (`bootstrap.rs:~2695`), for each `PerFilterConfig::Csrf(p)` value call `validate_csrf_config(p, listener_name)` (so a fractional/runtime-keyed route override is also fatal — the `rejects_non_deterministic_csrf_route_override` test). Keep the cors arm's behavior unchanged.

- [ ] **Step 4: Wire `HttpFilterInstance::Csrf`** in `instance.rs`:
  - Variant: add `Csrf(CsrfFilter)` (after `Cors(CorsFilter)` at `:59`).
  - `build` (`:121`, after the `Cors` arm): `HttpFilterTypedConfig::Csrf(cfg) => Ok(HttpFilterInstance::Csrf(CsrfFilter::build_from_config(cfg, registry, hcm_stat_prefix)?)),`
  - `decode_headers` (`:135`): `HttpFilterInstance::Csrf(f) => f.decode_headers(req),`
  - `encode_headers` (`:153`): `HttpFilterInstance::Csrf(f) => f.encode_headers(resp_arg),`
  - `apply_route_config` (`:167`): change the single `if let` to handle both filters, e.g.:
    ```rust
    match self {
        HttpFilterInstance::Cors(f) => f.apply_route_config(route),
        HttpFilterInstance::Csrf(f) => f.apply_route_config(route),
        _ => {}
    }
    ```
  - Import `CsrfFilter` (it's re-exported from the crate root, or `use crate::csrf::CsrfFilter;`).

- [ ] **Step 5: Run the tests + the WORKSPACE build (the exhaustive-match gate — SC2)**

Run: `cargo test -p envoy-config csrf 2>&1 | tail -10`
Expected: PASS (5 tests).
Run: `cargo test -p envoy-filter builds_csrf csrf 2>&1 | tail -10`
Expected: PASS.
Run: `cargo build --workspace 2>&1 | tail -3`
Expected: clean — confirms the `HttpFilterTypedConfig::Csrf` addition kept the `instance.rs` build match + the `bootstrap.rs` validator match exhaustive.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs crates/envoy-filter/src/instance.rs
git commit -m "phase 24 Task 3: HttpFilterInstance::Csrf wiring + validate_csrf_config [ADR-0061]"
```

---

## Task 4: D6.1 — fixture `0032-http-filter-csrf` + Docker wrapper + `Http1Method::Post`

**Files:**
- Create: `tests/fixtures/0032-http-filter-csrf/{envoy.yaml,envoy-rust.yaml,inputs/,expectations.yaml,README.md}`
- Modify: `tests/differential/src/lib.rs` (`Http1Method::Post` + `as_str` arm)
- Create: `tests/differential/tests/http_filter_csrf.rs`

- [ ] **Step 1: Add `Http1Method::Post`** in `tests/differential/src/lib.rs` (enum `:742` + `as_str` `:752`):

```rust
pub enum Http1Method {
    Get,
    Options,
    /// Phase-24 NEW: POST is required by the CSRF modify-method probes (fixture
    /// 0032). The H1 driver builds the request line from `method.as_str()`; POST
    /// probes carry no request body (the CSRF guard is header-only).
    Post,
}
// in as_str(): Http1Method::Post => "POST",
```

(No change to `drive_http2`'s `debug_assert` at `:1660` — fixture 0032 is H1-only; POST is never driven over H2 this phase.)

- [ ] **Step 2: Write the fixture configs.** `envoy.yaml` + `envoy-rust.yaml` (identical bootstrap — H1 listener, HCM with `http_filters: [csrf, router]`, the csrf chain entry `filter_enabled: { default_value: { numerator: 100, denominator: HUNDRED } }`, a single `prefix: "/"` route with `route: { cluster: backend }` + a `typed_per_filter_config[envoy.filters.http.csrf]` override carrying `filter_enabled` 100% + `additional_origins: [exact: "{{BACKEND_HOST}}:{{HTTP1_BACKEND_PORT}}"]`-shaped allowed origin, proxying to the `http1-echo-server` real upstream — the 0031 template variables). **The `additional_origins` matcher is scheme-stripped `host:port`** (ADR-0061 L3). The fixture's "additional-allowed" probe sends `Origin: http://{{additional-host}}` where `host_and_port` reduces to the matcher value. Model on `tests/fixtures/0031-http-filter-cors/envoy.yaml` verbatim except the filter block + route policy.

  > **Probe-origin design note (heeds ADR-0061 L3 + the differential determinism invariant 5.6):** the "same-origin" probe sets `Origin` to the target authority (the listener `Host` value the harness emits); the "additional-allowed" probe sets `Origin` to a DISTINCT host that the route's single `additional_origins` `exact:` matcher matches (scheme-stripped); the "evil" probe sets a host matching neither. Because both proxies see byte-identical config + headers, the 200/403 split is deterministic cross-proxy (no timing/crypto). Pick the `additional_origins` value and the probe `Origin` values so `host_and_port(Origin)` equals the matcher (e.g. matcher `exact: "additional.csrf.test"`, probe `Origin: http://additional.csrf.test`).

- [ ] **Step 3: Write `expectations.yaml`** using `Driver::Http1ProbeList` with 5 `Http1Probe` entries (the `Http1Probe` struct: `name`, `method`, `path`, `host`, `extra_headers`, `expected_status`, `expected_body`, `expected_headers`):

```yaml
driver:
  http1_probe_list:
    probes:
      - { name: "post-same-origin",  method: post, path: "/", host: "csrf.test", extra_headers: [["origin","http://csrf.test"]],            expected_status: 200, expected_headers: set_equal_modulo_allow_list }
      - { name: "post-evil-origin",  method: post, path: "/", host: "csrf.test", extra_headers: [["origin","http://evil.example.com"]],     expected_status: 403, expected_body: { kind: byte_exact, body: "Invalid origin" }, expected_headers: set_equal_modulo_allow_list }
      - { name: "post-additional",   method: post, path: "/", host: "csrf.test", extra_headers: [["origin","http://additional.csrf.test"]], expected_status: 200, expected_headers: set_equal_modulo_allow_list }
      - { name: "get-evil-safe",     method: get,  path: "/", host: "csrf.test", extra_headers: [["origin","http://evil.example.com"]],     expected_status: 200, expected_headers: set_equal_modulo_allow_list }
      - { name: "post-no-source",    method: post, path: "/", host: "csrf.test", extra_headers: [],                                          expected_status: 403, expected_body: { kind: byte_exact, body: "Invalid origin" }, expected_headers: set_equal_modulo_allow_list }
```

(Exact YAML key spelling/casing must match the serde tags — `http1_probe_list`, `byte_exact`, `set_equal_modulo_allow_list`. Cross-check against `tests/fixtures/0031-http-filter-cors/expectations.yaml` and confirm the `Host: csrf.test` value matches whatever the listener route `domains` accept and what `host_and_port` reduces the same-origin `Origin` to. The same-origin probe's `Origin: http://csrf.test` reduces to `csrf.test`, equal to `host_and_port("csrf.test")` = `csrf.test`.)

- [ ] **Step 4: Write the Docker-gated wrapper** `tests/differential/tests/http_filter_csrf.rs` (mirror `http_filter_cors.rs` verbatim, swapping the fixture dir + the probe-sequence doc comment for `[200,403,200,200,403]`).

- [ ] **Step 5: Write `README.md`** for the fixture (the 0031 README template: what the fixture exercises, the ADR-0061 L1/L3/L4/L6 + L8 lock-ins it depends on, the byte-exact 403 body rationale, the real-upstream-per-L8 note).

- [ ] **Step 6: Run the fixture (Docker-gated, local — `feedback_state4_runs_docker_differential`)**

Run (pre-build helpers first, never concurrent with cargo builds — `project_flaky_access_log_fixture_0012`):
```
cargo build -p tests-helpers 2>/dev/null; cargo test -p differential --no-run 2>&1 | tail -3
cargo test -p differential http_filter_csrf 2>&1 | tail -20
```
Expected: PASS — both proxies produce `[200,403,200,200,403]`; the two 403s carry byte-exact `Invalid origin`.

- [ ] **Step 7: Commit**

```bash
git add tests/fixtures/0032-http-filter-csrf tests/differential/src/lib.rs tests/differential/tests/http_filter_csrf.rs
git commit -m "phase 24 Task 4: fixture 0032-http-filter-csrf + Http1Method::Post [ADR-0061]"
```

---

## Task 5: D6.2 — `parse_bootstrap` fuzz seed

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_csrf_typed_per_filter_config.yaml`
- Modify: the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array (wherever the curated-seed list lives — grep `route_cors_typed_per_filter_config` to find it; add the csrf seed alongside)

- [ ] **Step 1: Find the seed-list test**

Run: `grep -rn "route_cors_typed_per_filter_config\|fuzz_corpus_seeds_parse_or_reject_cleanly" crates/envoy-config/`
Expected: the test enumerating curated seeds (the 09/10/11/22/23 cadence).

- [ ] **Step 2: Write the seed** — a full minimal bootstrap (H1 listener + HCM + `[csrf, router]` filters + a route with a csrf `typed_per_filter_config` override, `filter_enabled` 100% + one `additional_origins` exact matcher). Model on `route_cors_typed_per_filter_config.yaml`.

- [ ] **Step 3: Add the seed name to the SUCCESS array** in the seed-list test (it must parse-or-reject cleanly — this one parses clean).

- [ ] **Step 4: Run the seed test**

Run: `cargo test -p envoy-config fuzz_corpus_seeds 2>&1 | tail -8`
Expected: PASS (the new seed parses clean; no panic).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/route_csrf_typed_per_filter_config.yaml crates/envoy-config/src/
git commit -m "phase 24 Task 5: parse_bootstrap fuzz seed for csrf typed_per_filter_config"
```

---

## Task 6: D6.3 — in-process backstop

**Files:**
- Create: `crates/envoy-bin/tests/http_filter_csrf.rs` (mirror `crates/envoy-bin/tests/http_filter_cors.rs`)

- [ ] **Step 1: Write the backstop** — boot `envoy-bin` (H1) with a synthesized csrf bootstrap (chain `[csrf, router]`, route override `filter_enabled` 100% + `additional_origins: [exact: "additional.csrf.test"]`, proxying to a tiny in-process all-method-200 backend or the standing test backend helper — match the cors backstop's upstream pattern). Use the `tokio::process::Command` + `.kill_on_drop(true)` subprocess discipline (the 09 REVIEW M3 standing rule). Issue the 5 sequential probes over one keep-alive connection (or 5 connections) and assert each status + the two 403 bodies byte-exact (`Invalid origin`) — the phase-10 M1 lesson (assert OR disclose omission in PROGRESS). Note the N≥9 backstop-duplication in the file header (the shared-test-support-crate extraction stays deferred unless the implementer judges otherwise).

- [ ] **Step 2: Run the backstop STANDALONE (`project_workspace_test_nested_cargo_backstop_flake`)**

Run (pre-build helpers; run `-p envoy-bin` standalone, never under `--workspace`):
```
cargo build -p tests-helpers 2>/dev/null
cargo test -p envoy-bin http_filter_csrf 2>&1 | tail -20
```
Expected: PASS in ~2s.

- [ ] **Step 3: Commit**

```bash
git add crates/envoy-bin/tests/http_filter_csrf.rs
git commit -m "phase 24 Task 6: in-process csrf backstop"
```

---

## Task 7: State-4 verification + STATE advance (phase close prep)

> This task is the state-3→state-4 boundary prep, NOT the phase close. It runs the §7.5 gate suite locally + quotes evidence into PROGRESS, leaving the phase at state-4-complete / state-5-next (code review). The actual phase-close (ROADMAP `done` + STATE → AWAITING NEXT PLANNING) happens at state 6 after `REVIEW.md` is approved.

**Files:**
- Modify: `docs/envoy-rust/phases/24-http-filter-csrf/PROGRESS.md` (per-gate quoted evidence)
- Modify: `docs/envoy-rust/STATE.md` (advance to state-4-complete / state-5-next at the verification commit)

- [ ] **Step 1: Run the full §7.5 gate suite locally** (pre-build `tests/helpers/*`; never run the Docker suite concurrently with cargo builds — `project_flaky_access_log_fixture_0012`; run the envoy-bin backstop standalone — `project_workspace_test_nested_cargo_backstop_flake`). Quote each into PROGRESS:
  - `cargo build --workspace --all-targets`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` (first full clippy of the phase — `project_state3_arc_skips_clippy`)
  - `cargo build -p envoy-config -p envoy-filter` (the standalone-crate gate — `project_isolated_crate_build_blindspot`)
  - `cargo fmt --all -- --check`
  - `cargo test --workspace` (minus the envoy-bin backstop, run standalone)
  - `cargo deny check` (no-op delta — no new dependency)
  - `cargo test -p differential -p h2spec-conformance` (the 32-fixture Docker differential + h2spec ≥95% — `feedback_state4_runs_docker_differential`; pre-build `--no-run` first)
  - the `parse_bootstrap` fuzz target short-budget run (the new seed)
- [ ] **Step 2: Confirm gate (b)** — all 31 pre-existing fixtures (`0001`–`0031`) green simultaneously with `0032` (regression-equivalence; the additive enum arms are non-regressive — invariant 5.5).
- [ ] **Step 3: Push + capture the authoritative Linux CI run** (ADR-0049 Provenance — the AUTHORITATIVE differential evidence is the Linux CI run; record its URL + HEAD SHA + per-gate result in PROGRESS).
- [ ] **Step 4: Advance STATE.md** to state-4-complete / state-5-next; relocate superseded narrative to STATE_HISTORY.md (ADR-0035). Commit.

---

## Self-review (against the SPEC + ADR-0061, fresh eyes)

- **Spec coverage:** D1 (schema + per-route variant) → Task 1; D2 (`CsrfFilter` runtime) → Task 2; D3 (wiring + validator) → Task 3; D4 (stats + contract) → Task 2; D5 (reserved-empty) → n/a; D6.1 (fixture) → Task 4; D6.2 (fuzz seed) → Task 5; D6.3 (backstop) → Task 6; state-4 → Task 7. ✅ Every deliverable mapped.
- **§6.2 lock-ins:** L1 (config shape) → Tasks 1+3; L2 (modify set) → Task 2 `MODIFY_METHODS`; L3 (scheme-stripped origin) → Task 2 `host_and_port`; L4 (403 body) → Task 2 `failure_response` + Tasks 4/6 byte-exact; L5 (mutually-exclusive stats) → Task 2; L6 (chain-base/route-replace + dispositions) → Tasks 2+3; L7 (absent-filter reuse) → Task 3; L8 (real upstream) → Task 4. ✅
- **Type consistency:** `CsrfPolicy`/`RuntimeFractionalPercent`/`CompiledCsrfPolicy`/`host_and_port`/`failure_response`/`MODIFY_METHODS`/`FAILURE_BODY` used identically across Tasks 1-3-6; `build_from_config(cfg, registry, hcm_stat_prefix)` 3-arg signature matches the cors precedent; `FilterResponse { status, reason, headers, body }` matches `types.rs`. ✅
- **Exhaustive-match safety:** Task 1 fixes the two `PerFilterConfig` consumers (`cors.rs:118`, test `:12908`) + gates on `cargo build --workspace`; Task 3 fixes the two `HttpFilterTypedConfig` consumers (`instance.rs` build, `bootstrap.rs` validator) + gates on `cargo build --workspace` (the phase-22/23 exhaustive-match lesson). ✅
- **No placeholders:** every code step shows real code; commands have expected outputs. ✅

## Execution handoff

Per `feedback_execution_style` (subagent-driven is the project default) + `feedback_serial_subagent_dispatch` (SERIAL, never parallel): the NEXT session begins the state-3 implementation arc via `superpowers:subagent-driven-development`, dispatching one implementer subagent per task (Tasks 1→7) SERIALLY, with two-stage (spec-then-quality) review on the substantive tasks (the Task-2 `CsrfFilter` is the review centerpiece — the chain-base/route-replace model + the scheme-stripped origin compute), one code commit + one PROGRESS commit per task. This PLAN-write session ends after the standalone pre-Task-1 commit (PLAN.md + PROGRESS.md skeleton + Task-1 preamble + ROADMAP flip + STATE advance) per §5.1 (one state per session).
