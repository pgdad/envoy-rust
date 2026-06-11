# Phase 24 (`24-http-filter-csrf`) — SPEC

- **Phase id:** `24`
- **Slug:** `24-http-filter-csrf`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `3b7cf1384`, the phase-23 state-6 close-out commit; the "HTTP filters family" §9 heading carries five concrete rows — phase 09 `local_ratelimit` `done`, phase 10 `rbac` `done`, phase 11 `fault` `done`, phase 22 `jwt_authn` `done`, phase 23 `cors` `done`). **This SPEC's landing commit adds the sixth concrete row beneath the HTTP-filter-family heading**, with `status: planned` (invariant 4.1.2 — a new row enters `planned`; it flips to `in-progress` only when the NEXT session's state-2 PLAN-write points STATE at it per invariant 4.1.3).
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"HTTP filters family — header manipulation, cors, compression, fault, local+global rate limit, jwt_authn, rbac, ext_authz, ext_proc, oauth2, **csrf**, buffer, lua, wasm, adaptive concurrency, admission control, bandwidth limit."* This phase lands `envoy.filters.http.csrf` narrowed to the **minimum-viable per-route surface**: cross-site-request-forgery protection for state-changing ("modify") HTTP methods by checking the request's source origin (`Origin`, falling back to `Referer`) against the target origin (`Host`/`:authority`) plus an `additional_origins` allow-list matched with the existing `StringMatcher`, returning a 403 local reply on mismatch. **Phase 24's structural significance is that it is the SECOND consumer of the per-route `typed_per_filter_config` infrastructure landed at phase 23** — it proves that infrastructure generalizes additively (the §6.3 anti-pattern requires a tested consumer; phase 23 registered only the `Cors` variant). `filter_enabled` runtime fractional gating (honored only at the deterministic 0%/100% endpoints), `shadow_enabled` shadow mode, vhost-level config + the route>vhost precedence cascade, and `typed_per_filter_config` for filters OTHER than `cors`/`csrf` all defer per §4 below.
- **Position in the project:** the **sixth concrete HTTP-filter-family phase** (after phase-09 `local_ratelimit`, phase-10 `rbac`, phase-11 `fault`, phase-22 `jwt_authn`, phase-23 `cors`). The MVP trunk 00→08, the upstream-robustness family (12→17, complete in minimum-viable form), the xDS / dynamic-config filesystem-transport quartet (18 CDS / 19 LDS / 20 RDS / 21 EDS), and the five prior HTTP filters all stand closed as of HEAD `3b7cf1384`. Phase 24 amortizes the framework investment of phases 07/09/10/11/22/23 — most directly **phase 23's per-route `typed_per_filter_config` infrastructure** (the `PerFilterConfig` `@type`-tagged enum, the `resolve_route` HCM helper, the `FilterPipeline::apply_route_config` → `HttpFilterInstance::apply_route_config` → per-filter `apply_route_config` threading, and the `PerRouteConfigForAbsentFilter` validator) — plus the `Decision::StopAndSend(FilterResponse)` decode-side short-circuit (07/09/10/11/22), the per-filter `StatsRegistry` counter-wiring pattern, the `04.x` `StringMatcher` reuse, and the H1 `decorate_filter_synth_response` / phase-11 H2 `decorate_filter_synth_response_h2` filter-synth helpers.
- **depends-on:** `04` (HCM + route matching + `StringMatcher`), `07` (the parent filter-chain framework), `23` (the per-route `typed_per_filter_config` infrastructure CSRF is the second consumer of). Phase 24 extends the 07.1-landed `envoy-filter::FilterPipeline` + `HttpFilterInstance` enum with an **eighth** production variant (after `Router` at 07.1, `HeaderMutation` at 07.2, `LocalRateLimit` at 09, `Rbac` at 10, `Fault` at 11, `JwtAuthn` at 22, `Cors` at 23) and adds the **second** `PerFilterConfig` variant (after `Cors` at 23). Implicit (non-`depends-on`-field) dependencies: phase `05` (the H2 codec — the per-route-config threading is codec-shared and was landed symmetrically on H1 + H2 at phase 23, so CSRF is automatically available on H2 even though the phase-24 differential fixture is H1), phase `06` (the `StatsRegistry` + admin `/stats` surface the `csrf.*` counters land on). The 31-Docker-gated-fixture regression baseline established at phase-23 close (`0001-tcp-echo` through `0031-http-filter-cors`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **Brainstorm narrative:** see the "Phase-24 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + filter-pick rationale with the alternatives considered (buffer / per-route overrides of existing filters / compression / ext_authz / load-balancing) along the established scoring axes, and ADR-0060 for the scoping decision.

---

## 0. Critical scoping finding (READ FIRST) — the per-route-config infrastructure already exists; CSRF is a pure plug-in consumer

**The one architectural risk that made phase 23 borderline — the HCM resolving the matched route AFTER the filter pipeline ran — is GONE.** Phase 23 landed the cross-cutting fix: the HCM now resolves the route up-front (`resolve_route` at `crates/envoy-http1/src/hcm.rs:1195`, reused by `crates/envoy-http2/src/hcm.rs`) and threads it into the pipeline via `FilterPipeline::apply_route_config` (`crates/envoy-filter/src/pipeline.rs:66`) → `HttpFilterInstance::apply_route_config` (`crates/envoy-filter/src/instance.rs:167`) → each filter's `apply_route_config` (the `Cors` arm at `crates/envoy-filter/src/cors.rs:115`). **Phase 24 therefore makes NO change to the shared HCM request-processing path** — it adds a `Csrf` arm to the existing `PerFilterConfig` enum, the existing `HttpFilterInstance` enum + its `build`/`decode_headers`/`apply_route_config` dispatch, and a new `csrf.rs` filter module. This is the lightest filter phase since the framework was completed: it is a near-pure additive plug-in into machinery proven by phase 23's 31-fixture regression baseline.

Three reuse facts anchor the PLAN-write (established by read-only reconnaissance at HEAD `3b7cf1384`):

1. **`PerFilterConfig` exists and is `@type`-tagged for additive extension** (`crates/envoy-config/src/bootstrap.rs:787`), currently holding only the `Cors(CorsPolicy)` variant. Phase 24 adds `Csrf(CsrfPolicy)`. The hand-rolled `Route` deserializer (`bootstrap.rs:1404-1435`) already parses the `typed_per_filter_config:` map into `BTreeMap<String, PerFilterConfig>` — no deserializer surgery, only an enum variant.
2. **The per-route threading dispatch is already filter-generic** (`instance.rs:167` `apply_route_config` matches on the instance variant; `cors.rs:115` shows the per-filter shape: read `route.typed_per_filter_config.get(<filter-name>)`, match the `PerFilterConfig` arm, compile + store the active policy for this request's filter-instance clone). CSRF mirrors this exactly.
3. **The decode-side short-circuit + validator + helpers are all reusable verbatim:** `Decision::StopAndSend(FilterResponse)` (the 403 local reply, decorated by the existing `decorate_filter_synth_response{,_h2}` helpers — the rbac/fault/jwt 403/401 precedent); the `PerRouteConfigForAbsentFilter` all-fatal validator (`crates/envoy-config/src/lib.rs:531`, logic at `bootstrap.rs:2605/2695`) already rejects a per-route policy whose filter is absent from the chain — CSRF slots straight in by registering its filter name; the case-insensitive `header_ci` helper (`crates/envoy-filter/src/jwt_authn.rs:156` + the duplicate at `cors.rs:236`) for `Origin`/`Referer`/`Host` reads; the `04.x` `StringMatcher` (`bootstrap.rs:1815`, modes at `:1821`) for `additional_origins`.

---

## 1. Goal and acceptance signal

Phase 24 lands the `envoy.filters.http.csrf` filter (typed_config `@type = type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy`) as the **eighth `HttpFilterInstance` variant** (after Router, HeaderMutation, LocalRateLimit, Rbac, Fault, JwtAuthn, Cors) and the **seventh concrete pluggable feature filter** — configured per-route via the phase-23 `typed_per_filter_config` infrastructure (the second `PerFilterConfig` variant). The filter reads a `CsrfPolicy` attached to the matched route via `typed_per_filter_config` and, on the **decode side only**:

- **Modify method (state-changing):** for a request whose method is in the modify-method set (POST/PUT/DELETE/PATCH and any other non-safe method — the exact set is §6.2-verified), compute the **source origin** from the `Origin` header (falling back to `Referer` when `Origin` is absent) and the **target origin** from the `Host`/`:authority`. If the source origin equals the target origin OR matches one of the policy's `additional_origins` `StringMatcher` entries → the request is **valid** → `Decision::Continue` (the request proceeds; `csrf.request_valid` increments). If the source origin is present but matches neither → **invalid** → `Decision::StopAndSend(FilterResponse)` with HTTP **403** and the Envoy CSRF failure body (§6.2-verified bytes), decorated by the existing H1/phase-11-H2 filter-synth helpers (`csrf.request_invalid` increments). If no source origin is present at all on a modify method → **missing source origin** → 403 (§6.2-verified — Envoy treats a missing source origin on a modify method as invalid; `csrf.missing_source_origin` increments).
- **Safe method (GET/HEAD/OPTIONS/TRACE — the §6.2-verified non-modify set):** pass through unconditionally (`Decision::Continue`); CSRF does not evaluate, and touches no stat.
- **No policy on the route / filter disabled (`filter_enabled` deterministic 0%):** pass through unchanged.

**Differential surface added by phase 24:**

- **Fixture `0032-http-filter-csrf`** — bilateral assertion that both proxies, given an identical bootstrap with the `csrf` filter in the HTTP filter chain and a `CsrfPolicy` attached to a route via `typed_per_filter_config` (one `additional_origins` entry via `StringMatcher` exact-match), produce deterministic per-probe results on a multi-probe burst over an **HTTP/1.1** listener proxying to a real upstream cluster (the `http1-echo-server` helper — per the ADR-0058 lesson that a per-route filter which proceeds on the happy path needs a real upstream to yield a 200; the 0008/0030/0031 pattern): probe 1 (`POST /` with `Origin: <target>` — same-origin) → proxied 200 (`request_valid`); probe 2 (`POST /` with `Origin: <evil>`) → 403 + the failure body (byte-exact, §6.2) (`request_invalid`); probe 3 (`POST /` with `Origin: <additional-allowed>`) → proxied 200 (`request_valid`); probe 4 (`GET /` with `Origin: <evil>`) → proxied 200, unguarded (safe method); probe 5 (`POST /` with no `Origin` and no `Referer`) → 403 (`missing_source_origin`). The exact 403 status + failure-body bytes + `content-type`, the modify-method set, the source/target-origin computation (scheme+host+port; the `Referer` fallback), and the `csrf` stat namespace are **empirically verified at state-2 PLAN-write per §6.2** (the phase-10/11/22/23-ratified verify-at-PLAN-write discipline — bytes are NOT assumed).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0032-http-filter-csrf` green at Docker-gated CI.
- **(b)** All **31 pre-existing differential fixtures** (`0001-tcp-echo` through `0031-http-filter-cors`) **remain green simultaneously** at the same CI run (regression-equivalence per §7.5 (b)). Unlike phase 23, phase 24 makes NO shared-HCM-path change (§0), so the regression surface is limited to the additive enum/dispatch arms — but the full baseline is still re-run.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline). CSRF adds no H2-framing change; the state-4 verification re-runs `h2spec` as a non-regression check.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (one new seed for the csrf-filter + `typed_per_filter_config` bootstrap shape). No new parser/codec crate is introduced (CSRF reuses `StringMatcher`), so per §7.4 **no new fuzz target is required** — the new untrusted-input surface (the `CsrfPolicy` YAML inside the `typed_per_filter_config` map) is covered by the existing `parse_bootstrap` target via the new seed. The PLAN-writer confirms at §6.2.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean. **No new crate and no new dependency this phase** (CSRF is pure-Rust header inspection; no crypto, no codec, no external service) — `cargo deny check` is a no-op-delta. The standalone-crate builds (`project_isolated_crate_build_blindspot`) at state-4 cover the touched crates (`envoy-config`, `envoy-filter`).
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent through 06.1 / 07.x / … / 23).

---

## 2. Behavior-contract scope for phase 24

Phase 24 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x → 23 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — 3 new rows (projected; §6.2-verified)

Upstream Envoy's `csrf` filter emits per-route CSRF stats. The projected set (the exact rooting — HCM-prefixed like rbac/fault/jwt/cors — is **§6.2-verified**):

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<stat_prefix>.csrf.request_valid` (root §6.2-verified) | value-exact | Counter; one increment per evaluated modify-method request with a valid source origin. |
| `http.<stat_prefix>.csrf.request_invalid` (root §6.2-verified) | value-exact | Counter; one increment per evaluated modify-method request with a present-but-disallowed source origin. |
| `http.<stat_prefix>.csrf.missing_source_origin` (root §6.2-verified) | value-exact | Counter; one increment per evaluated modify-method request with no source origin (no `Origin`, no `Referer`). |

The differential fixture 0032 does **not** scrape `csrf` stats (it asserts the per-probe status + the 403 failure body); the counters are exercised by the in-process backstop + unit tests. This mirrors the phase-10/11/22/23 posture. The §2.1 stat rows + namespace + which-increments-when semantics (in particular whether `missing_source_origin` ALSO increments `request_invalid`, or is mutually exclusive) are §6.2-verified at PLAN-write.

### 2.2 "Local reply" extension — the 403 failure status + body (§6.2-verified)

Envoy's CSRF filter rejects an invalid (or missing-source) modify request with a **403** local reply and a fixed failure body (projected `"Invalid origin"` — §6.2-verified bytes + `content-type`). The exact status + body bytes + the presence/absence/value of `content-type` (heeding the ADR-0059 empty-body-`content-type`-omission rule and the phase-10 RBAC-body-off-by-one-byte lesson) are §6.2-verified and recorded in BEHAVIOR_CONTRACT at the exercising task. The decode-side `Decision::StopAndSend` flows through the existing H1 `decorate_filter_synth_response` + the phase-11 H2 `decorate_filter_synth_response_h2` helpers (server/date/content-length stamping), reused unchanged.

### 2.3 DECISIONS.md — ADR-0060 lands at THIS SPEC commit; ADR-0061 / ADR-0062 reserved

Phase 24 lands **ADR-0060** at THIS brainstorm commit — the scope lock (the pick + the minimum-viable scope + the alternatives-rejected analysis), mirroring the HTTP-filter-family / xDS-family brainstorm-ADR cadence (ADR-0048/0050/0051/0053/0055/0057). **ADR-0061 is reserved** for the §6.2 empirical-verification reconciliation at PLAN-write (most-likely trigger: the config shape / the modify-method set / the 403 failure-body bytes / the stat semantics). **ADR-0062 is reserved** for the §6.1 split (lands only if phase 24 splits into 24.1/24.2 — see §6.1; very unlikely given §0).

---

## 3. Deliverables

Phase 24's scope is enumerated as deliverables `D1`–`D6`. **The state-2 PLAN-writer organizes deliverables into tasks** (and evaluates the §6.1 split gate) — these are not 1:1 with tasks. Listed in roughly execution order; the SPEC constrains the surface, not the task order.

### D1 — `CsrfPolicy` schema + the second `PerFilterConfig` variant (the per-route-infra exercise)

At `crates/envoy-config/src/bootstrap.rs`, add the `Csrf(CsrfPolicy)` variant to the existing `PerFilterConfig` enum (`:787`) and the near-empty filter-chain `Csrf` entry to `HttpFilterTypedConfig` (`:741`), then define `CsrfPolicy`:

```rust
// Add to PerFilterConfig (bootstrap.rs:787) — the SECOND variant after Cors:
#[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy")]
Csrf(CsrfPolicy),

// New per-route policy (the exact field names/types + @type are §6.2-verified):
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CsrfPolicy {
    // The allow-list of source origins permitted in addition to the same-origin target.
    #[serde(default)] pub additional_origins: Vec<StringMatcher>,   // REUSE 04.x StringMatcher
    // filter_enabled: honored ONLY at deterministic 0%/100% (the phase-11 fault precedent);
    //   runtime_key-gated fractional gating is DEFERRED (§4). Exact shape + disposition §6.2.
    #[serde(default)] pub filter_enabled: Option<RuntimeFractionalPercent>,
    // DEFER (deny_unknown_fields rejects): shadow_enabled.
}

// Near-empty filter-chain entry (mirrors CorsConfig {} at bootstrap.rs:821):
pub struct CsrfConfig {}   // @type = …csrf.v3.CsrfPolicy on the chain entry — §6.2-verified
```

All structs carry `#[serde(deny_unknown_fields)]` per the established envoy-config discipline (rejects the deferred `shadow_enabled`). **The exact upstream config shape — in particular WHERE the policy must live (chain-level `CsrfPolicy` with a required `filter_enabled`, vs a per-route `typed_per_filter_config` override, and whether the filter constructs at all without a chain-level `filter_enabled`) — is the gating §6.2 item (item 1) and may force a minor schema reshape.** The phase's primary deliverable is the **per-route** path (the `PerFilterConfig::Csrf` variant); the filter-chain `Csrf` entry is near-empty, mirroring `CorsConfig`. The PLAN-writer confirms `StringMatcher` is reused verbatim (`bootstrap.rs:1815-1830` supports exact/prefix/suffix/safe_regex/contains, which matches `additional_origins`) and whether `RuntimeFractionalPercent` already exists in envoy-config or needs a minimal definition.

### D2 — `envoy-filter::CsrfFilter` runtime

New module `crates/envoy-filter/src/csrf.rs`. Hand-rolled per D-3.2 (one module per concrete filter; the 07.2/09/10/11/22/23 precedent), consuming `envoy-config` for the `CsrfPolicy`/`StringMatcher`/`RuntimeFractionalPercent` + `envoy-stats` for the counters. Module shape mirrors `cors.rs`:

```rust
pub struct CsrfFilter {
    request_valid: Arc<Counter>,
    request_invalid: Arc<Counter>,
    missing_source_origin: Arc<Counter>,
    active_policy: Option<CompiledCsrfPolicy>,  // set by apply_route_config before decode
}

impl CsrfFilter {
    pub(crate) fn build_from_config(
        cfg: &envoy_config::CsrfConfig,         // the near-empty filter-chain Csrf entry
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> { /* register the 3 counters; the 3-arg signature, no widening */ }

    // The phase-23 per-route threading hook (cors.rs:115 is the precedent):
    pub(crate) fn apply_route_config(&mut self, route: Option<&envoy_config::Route>) {
        // route.typed_per_filter_config.get("envoy.filters.http.csrf") → PerFilterConfig::Csrf(p)
        //   → compile (the StringMatchers + the filter_enabled deterministic gate) into active_policy.
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        // 0. No active policy (no per-route csrf config) OR filter_enabled deterministic-0% → Continue.
        // 1. If req.method NOT in the modify-method set (§6.2) → Continue (safe method, no eval, no stat).
        // 2. Compute source origin: header_ci("origin"); if absent, derive from header_ci("referer").
        //    Compute target origin from header_ci("host")/:authority (scheme+host+port — §6.2).
        // 3. If no source origin → missing_source_origin.inc(); StopAndSend(403, failure body).
        // 4. If source == target OR matches additional_origins → request_valid.inc(); Continue.
        //    Else → request_invalid.inc(); StopAndSend(403, failure body).
        unimplemented!()
    }
    // No encode_headers logic (CSRF is decode-side only — the default Continue arm).
}
```

**Signposts:** (a) the per-route policy flows in via the existing `apply_route_config` threading (the filter does NOT read the route itself — it is handed its resolved `CsrfPolicy` per request, exactly as `CorsFilter` is handed its `CorsPolicy`); (b) **decode-side only** — CSRF has no encode behavior (simpler than `cors`, the first decode-only per-route filter); (c) origin matching reuses `StringMatcher`; (d) header lookups reuse the case-insensitive `header_ci` helper — **the M-track `header_ci` duplication is now at N=3 (jwt_authn.rs / cors.rs / csrf.rs); the PLAN-writer SHOULD extract a single shared `header_ci` (or note the deferral in the module header)**; (e) the source/target-origin comparison semantics (scheme + host + port; how the target origin's scheme is determined for a plaintext H1 listener; the `Referer` → origin reduction) are §6.2-verified.

### D3 — `HttpFilterInstance::Csrf` variant + dispatch + validator reuse

- Extend `crates/envoy-config/src/bootstrap.rs::HttpFilterTypedConfig` (`:741`) with an eighth variant `Csrf(CsrfConfig)` (the near-empty filter-chain entry).
- Extend `crates/envoy-filter/src/instance.rs::HttpFilterInstance` (`:32`) with a `Csrf(CsrfFilter)` variant (the eighth); extend the `build` dispatch (calling `CsrfFilter::build_from_config(cfg, registry, hcm_stat_prefix)`), the `decode_headers` dispatch, and the `apply_route_config` dispatch (`:167` — add the `Csrf` arm mirroring the `Cors` arm). Re-export `CsrfFilter` from `crates/envoy-filter/src/lib.rs`. Remove the `envoy.filters.http.csrf` entry (if present) from the unsupported-filter reject list (`crates/envoy-filter/src/error.rs`).
- **Validator:** the existing `PerRouteConfigForAbsentFilter` all-fatal validator (`lib.rs:531`; logic at `bootstrap.rs:2605/2695`) already rejects a per-route policy whose filter name is absent from the HCM chain — phase 24 confirms `csrf` slots in (the validator walks the post-RDS-merge route table per the phase-23 close). Add any new `ConfigError` variant required for a non-deterministic `filter_enabled` (e.g. `ConfigError::UnsupportedRuntimeKeyedCsrfFilterEnabled` if Envoy permits a `runtime_key` that envoy-rust rejects — §6.2 decides; the ADR-0049 all-fatal posture is the default), each with positive + negative unit tests, exercised by the `parse_bootstrap` fuzz seed (D6.2).

### D4 — Stats wiring (3 counters) + BEHAVIOR_CONTRACT extension

At `CsrfFilter::build_from_config`, register `request_valid` + `request_invalid` + `missing_source_origin` `Counter` handles against the `Arc<StatsRegistry>` at the §6.2-verified namespace. Increment sites in `decode_headers` (per the valid/invalid/missing determination). The BEHAVIOR_CONTRACT extensions (§2.1 stat rows + §2.2 403 status/body) land at the tasks where each is first empirically exercised, per the 06.x → 23 cadence.

### D5 — (reserved — no new codec/crypto/HCM deliverable)

Unlike phase 23 (the cross-cutting HCM route-early-resolution change) or phase 22 (the `envoy-jwt` crypto crate), phase 24 introduces **no new crate, no new codec/crypto deliverable, and no shared-HCM change** — it is a pure additive plug-in into the phase-23 per-route-config machinery. This slot is intentionally empty.

### D6 — Fixture + harness + fuzz seed + in-process backstop

- **D6.1 — Fixture `tests/fixtures/0032-http-filter-csrf/`.** An **HTTP/1.1** listener proxying to a **real upstream cluster** (the `http1-echo-server` helper — per ADR-0058 L6: a per-route filter that proceeds on the happy path needs a real upstream to yield a 200; `direct_response` does NOT engage per-route filter config). Bootstrap: H1 HCM + the `csrf` filter in the chain + a route carrying a `CsrfPolicy` via `typed_per_filter_config` (1 `additional_origins` entry via `StringMatcher` exact-match) + router → the static cluster. Probe burst over one downstream connection: `[POST same-origin→200, POST evil-origin→403+body, POST additional-allowed→200, GET evil-origin→200, POST no-source→403]`. Asserts each probe's status + the 403 failure body byte-exact (§6.2). The harness needs an `Http1Method::Post` probe variant (the harness gained `Http1Method::Options` at phase 23 — extend the same enum); the PLAN-writer picks the multi-probe driver (`Driver::Http1ProbeList` or the keep-alive driver, with per-probe method + header control). Docker-gated wrapper `tests/differential/tests/http_filter_csrf.rs` (the 09/10/11/22/23 precedent), one `#[tokio::test]` invoking `run_fixture("0032-http-filter-csrf")`.

- **D6.2 — Fuzz seed.** One new `parse_bootstrap` corpus seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_csrf_typed_per_filter_config.yaml` (the csrf-filter + `typed_per_filter_config` bootstrap shape), extending the curated seed corpus by one (+ the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array, edited together per the 09/10/11/22/23 cadence). **No new fuzz target** (no new parser crate — CSRF reuses `StringMatcher`). The PLAN-writer confirms at §6.2.

- **D6.3 — In-process backstop.** New file `crates/envoy-bin/tests/http_filter_csrf.rs` mirroring `crates/envoy-bin/tests/http_filter_cors.rs` (the standing `tokio::process::Command` + `.kill_on_drop(true)` subprocess discipline from 09 REVIEW M3). Boots `envoy-bin` (H1) with a synthesized csrf bootstrap; issues the sequential probes (same-origin POST + evil-origin POST + additional-allowed POST + safe-method GET + no-source POST); asserts each status + the 403 failure body (heeds the phase-10 M1 backstop-header-assertion lesson — assert OR disclose omission in PROGRESS). **The extract-a-shared-test-support-crate item is now at N≥8 backstops** — the PLAN-writer notes the duplication in the file header (the consolidation stays deferred by the standing risk-managed decision unless the PLAN-writer judges otherwise). Per `project_workspace_test_nested_cargo_backstop_flake`, run the envoy-bin backstop standalone (`-p envoy-bin`) with helpers pre-built.

---

## 4. Out of scope (deferred non-goals)

Phase 24 explicitly does NOT land:

- **`filter_enabled` runtime fractional gating + `shadow_enabled` shadow mode.** The `filter_enabled` `RuntimeFractionalPercent` is honored **only at the deterministic 0%/100% endpoints** (the phase-11 fault deterministic-0%/100% precedent); a `runtime_key`-gated fractional value requires the RTDS runtime layer (unimplemented — Runtime + hot restart family) and is rejected/ignored per the §6.2 disposition. `shadow_enabled` (observe-only mode that logs but does not enforce, with its own shadow stats) defers entirely.
- **Vhost-level `typed_per_filter_config` + the route>vhost most-specific-config precedence cascade.** Phase 24 consumes **route-level** `typed_per_filter_config` only (consistent with phase 23, which deferred vhost-level). Envoy's `mostSpecificPerFilterConfig` route>vhost precedence resolution defers (the D1 schema admits vhost-level additively).
- **`typed_per_filter_config` for filters OTHER than `cors`/`csrf`.** The `PerFilterConfig` enum is general infrastructure; phase 24 registers + tests the **second** variant (`Csrf`). Per-route overrides of `fault`/`rbac`/`local_ratelimit`/`header_mutation` (and `buffer` as a future first-class consumer) defer to their own phases — each slots a variant into `PerFilterConfig` additively (the §6.3 anti-pattern is respected: no untested stub variants land).
- **CSRF on non-modify methods / custom modify-method configuration.** Phase 24 guards the standard modify-method set (§6.2-verified); Envoy does not expose a configurable method set for CSRF, so there is nothing to defer here beyond confirming the set.
- **Streaming/trailers CSRF interactions + gRPC-Web.** The gRPC family is still blocked on H2 trailers. Defer.

---

## 5. Architectural invariants

### 5.1 No new crate; the policy lives in `envoy-config`, the filter in `envoy-filter`

- **The `CsrfPolicy`/`CsrfConfig` schema + the `PerFilterConfig::Csrf` variant live in `crates/envoy-config/`** alongside the other shared config types. No new crate (unlike phase 22's `envoy-jwt`); CSRF is pure-Rust header inspection needing no foundation.
- **`CsrfFilter` lives at `crates/envoy-filter/src/csrf.rs`** (one module per concrete filter; the 07.2/09/10/11/22/23 pattern).
- **NO HCM change.** The per-route threading (`resolve_route` + `apply_route_config`) is reused verbatim from phase 23 — phase 24 adds only enum arms (§0).

### 5.2 Hand-rolled filter per D-3.2

The filter logic (modify-method classification, source/target-origin computation, origin matching, 403 short-circuit) is hand-rolled per D-3.2 (*"Every individual filter … Must be written from scratch"*). Origin matching reuses the existing `StringMatcher` (04.x). No new dependency.

### 5.3 Decode-side-only filter (the first decode-only per-route filter)

`CsrfFilter` acts **only on decode** (the modify-method guard + the 403 short-circuit). It is the first per-route-configured filter with no encode behavior (`cors` decorates on encode; `csrf` does not). The per-request `active_policy` is set by `apply_route_config` before `decode_headers`; no decode→encode state coupling is needed.

### 5.4 Per-route config (NOT filter-chain-level policy)

The CSRF policy source is the matched route's `typed_per_filter_config` (the second consumer of the phase-23 infra). The filter-chain `Csrf` entry is near-empty (it only declares the filter present). This is the higher-leverage choice that proves the per-route infrastructure generalizes (§0). The exact filter-chain-vs-route requirement (whether Envoy requires a chain-level `filter_enabled`) is the gating §6.2 item.

### 5.5 Regression-equivalence (additive, not cross-cutting)

Phase 24 adds enum arms + a new module; it does NOT touch the shared HCM request-processing path (§0). The `decode_headers`/`apply_route_config`/`build` dispatch sites match exhaustively on the instance variant, so the new `Csrf` arm is inert for every existing route (no route carries `csrf` config until configured). **All 31 pre-existing fixtures green simultaneously (gate (b))** is the proof obligation, but the blast radius is far smaller than phase 23's. Per the phase-22 lesson, a config-enum variant can break an exhaustive match masked by per-crate testing — run `cargo build --workspace` at the D1/D3 enum-variant tasks.

### 5.6 Determinism across both proxies (differential-testability invariant)

CSRF is a **pure function** of (request method, request `Origin`/`Referer`, request `Host`, the policy). Cross-proxy determinism holds because: (a) the policy is byte-identical config on both proxies; (b) origin matching via `StringMatcher` + the source/target comparison is deterministic; (c) there is no timing, no clock, no crypto, no body modification — the 403-vs-200 disposition + the failure-body bytes are fixed by the policy + the request headers. This makes CSRF fully differential-testable under §7.2 with zero timing sensitivity. Each counter increments exactly once per evaluated modify-method request.

### 5.7 H1 + H2 symmetric (the threading is codec-shared, already landed)

The `apply_route_config` threading + the filter operate on the codec-agnostic `FilterRequest`/`FilterResponse` abstraction (the 07.1 framework + ADR-0031; the phase-23 H1+H2 threading). The 403 `Decision::StopAndSend` is decorated by the existing per-codec helpers (H1 since 09; H2 since the phase-11 M2 close) — both reused unchanged. The phase-24 differential fixture is H1; the H1 backstop covers the H1 codec; the codec-shared threading + the phase-11-proven H2 decoration cover any H2 deployment without a phase-24 H2 fixture (the phase-11/22/23 posture). The state-4 `h2spec` ≥95% re-run confirms no H2 regression.

---

## 6. Implementation signposts for the planner

### 6.1 Split-gate evaluation (read first)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 tasks OR ~1500 LoC. Phase 24's SPEC-time surface estimate:

- D1 — `CsrfPolicy` schema + `PerFilterConfig::Csrf` + `HttpFilterTypedConfig::Csrf` + (maybe) `RuntimeFractionalPercent` (~90 LoC + ~110 LoC tests). ~1 task.
- D2 — `CsrfFilter` runtime (modify-method + origin compute + match + 403) (~170 LoC + ~180 LoC tests). ~1-2 tasks (the §6.2-discovered origin-computation nuance is the swing factor).
- D3 — `HttpFilterInstance::Csrf` variant + dispatch + validator confirm/extend (~70 LoC + ~80 LoC tests). ~1 task.
- D4 — stats wiring (3 counters) + BEHAVIOR_CONTRACT rows (~50 LoC + ~50 LoC tests). Co-located with D2.
- D6.1 — fixture 0032 + Docker wrapper + the `Http1Method::Post` harness extension (~120 LoC YAML + ~70 LoC wrapper/harness). ~1 task.
- D6.2 — fuzz seed (~30 LoC + corpus). ~0.5 task.
- D6.3 — in-process backstop (~200 LoC). ~1 task.
- State-4 verification + STATE-advance (~docs). ~1 task.

**SPEC-time projection: ~8-11 tasks; ~900-1300 LoC** (production ~500-650, tests ~600, fixture/harness/fuzz/doc ~250). This is **comfortably inside the §6.1 ~1500-LoC / ~25-task gate** and **lighter than phase 23** — the cross-cutting HCM change is already done (§0). **Recommended posture: single-phase.** The split valve is held in nominal reserve as `24.1` (D1 schema + the `PerFilterConfig::Csrf`/`HttpFilterInstance::Csrf` wiring + validator — a foundation slice, regression-proven) / `24.2` (the `CsrfFilter` runtime + stats + fixture 0032 + backstop + close), with the split ADR being **ADR-0062**, but the split is **very unlikely to fire** (the swing factor that made phase 23 borderline is gone).

### 6.2 Empirical verification at state-2 PLAN-write (the ratified verify-at-PLAN-write discipline)

Per the phase-10→23 verify-at-PLAN-write process improvement: the state-2 PLAN-writer empirically verifies the upstream wire shapes BEFORE locking PLAN lock-ins, by running `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) under Docker against the §3 D6.1 canonical bootstrap on an H1 listener — **this verification RUNS LOCALLY on macOS Docker** (CSRF has no virtiofs/inotify dependency; the phase-22/23 §6.2-local methodology applies). Verify:

1. **The exact Envoy config shape (do FIRST — it gates the schema D1):** confirm the filter-chain `Csrf` entry's `@type` + fields, AND the per-route `CsrfPolicy` `@type` (`…csrf.v3.CsrfPolicy`) + its exact field names/types (`additional_origins: [StringMatcher]`, `filter_enabled`, `shadow_enabled`) as attached via `typed_per_filter_config` on the route. **Critically: confirm whether Envoy requires a chain-level `filter_enabled` for the filter to construct, OR accepts a near-empty chain entry with the policy supplied purely per-route** (this determines whether `CsrfConfig` is `{}` like `CorsConfig` or must carry `filter_enabled`). Confirm `StringMatcher` is the `additional_origins` element type. This is a config-shape probe; it determines D1.
2. **The modify-method set:** drive POST/PUT/DELETE/PATCH + GET/HEAD/OPTIONS/TRACE with an evil origin; record exactly which methods CSRF evaluates (the non-safe set) vs passes through unconditionally. **Do NOT assume** the set.
3. **The source/target-origin computation:** confirm `Origin` is primary with `Referer` fallback; how the target origin is derived from `Host`/`:authority` (scheme + host + port; the scheme for a plaintext H1 listener); the exact comparison (scheme+host+port equality). Drive same-origin / cross-origin / `Referer`-only / `additional_origins`-match probes.
4. **The 403 local reply (status + body + headers):** drive a cross-origin POST; capture the exact status (403), the exact failure body bytes (projected `"Invalid origin"` — verify length + text), and the presence/absence/value of `content-type` (the ADR-0059 empty-vs-nonempty-body rule). Also capture the **missing-source-origin** disposition (same 403 + same body? a different body?). Record byte-exact → SPEC §2.2 lock-in.
5. **The stat namespace + semantics:** scrape `/stats` after each probe class; record the exact `csrf` stat names + their rooting (HCM-prefixed?) + which increments when — in particular whether a missing-source request increments `missing_source_origin` ONLY or also `request_invalid`, and whether safe methods touch any stat (§2.1).
6. **The `filter_enabled` / `shadow_enabled` disposition:** confirm the deterministic 100% (enforce) / 0% (passthrough) behavior; record envoy-rust's disposition for a `runtime_key`-gated fractional value (reject vs accept-and-treat-as-default) and for `shadow_enabled` (reject per `deny_unknown_fields`).
7. **The policy-for-absent-filter disposition:** confirm the existing `PerRouteConfigForAbsentFilter` all-fatal reject (landed at phase 23 / ADR-0058) applies to a `csrf` per-route policy with no `csrf` filter in the chain (projected: yes, reused verbatim). Also confirm the no-policy-on-route passthrough.

Each finding lands as a PLAN lock-in. **If any of items 1-6 differs materially from the SPEC projection, the reconciliation lands as inline ADR-0061 at the state-2 PLAN-write commit** (the phase-10 ADR-0034 / phase-22 ADR-0056 / phase-23 ADR-0058 inline-at-PLAN-write precedent). The next-available ADR after this brainstorm's ADR-0060 is **ADR-0061**.

### 6.3 The 06.x stats convention + 07.x BEHAVIOR_CONTRACT cadence

StatsRegistry registration at `build_from_config`; per-filter-instance Counter ownership; namespace §6.2-verified to upstream parity. Contract extensions (stat rows + 403 status/body) land at the task where each is first empirically exercised (the 06.x → 23 cadence), NOT at PLAN-write or SPEC time.

### 6.4 In-process backstop assertion (heeds the phase-10 M1 lesson)

D6.3 SHOULD assert each probe's status + the 403 failure body, OR explicitly disclose any omission in PROGRESS. Recommended: assert.

### 6.5 State-4 evidence discipline + isolated-crate build

Per the 05.3 → 23 chain: per-gate quoted evidence in PROGRESS at state-4 (real CI run URL + HEAD SHA + timestamp + per-gate output for all 5 stable-toolchain gates + each Docker-gated fixture + the h2spec gate + the fuzz target's iteration count). **Phase-24-specific:** (a) the standalone-crate builds (`project_isolated_crate_build_blindspot`) cover `cargo build -p envoy-config -p envoy-filter` (the touched crates); (b) gate (b) regression-equivalence (all 31 pre-existing fixtures green simultaneously) confirms the additive enum arms are non-regressive; (c) pre-build `tests/helpers/*` and never run the Docker suite concurrently with cargo builds (`project_flaky_access_log_fixture_0012`); run the envoy-bin backstop standalone (`project_workspace_test_nested_cargo_backstop_flake`); (d) `cargo deny check` is a no-op-delta (no new dependency). Per `project_state3_arc_skips_clippy`, the state-3 per-task verification runs `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK. **Run `cargo build --workspace` at the D1/D3 enum-variant tasks** (the phase-22/23 exhaustive-match lesson). Per `feedback_state4_runs_docker_differential`, the state-4 gate runs the full `cargo test -p differential -p h2spec-conformance` LOCALLY (Docker restored on this Mac; pre-build `--no-run` first), not only on Linux CI — though per ADR-0049 Provenance the AUTHORITATIVE differential evidence remains the Linux CI run.

### 6.6 PROGRESS.md skeleton + Task 1 preamble land alongside PLAN.md at state-2; subagent-driven execution at state-3

Per the 06.2 → 23 cadence: state-2 PLAN-write lands `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble in one standalone pre-Task-1 commit. State-3 executes via `superpowers:subagent-driven-development` (the `feedback_execution_style` default), SERIAL dispatch (`feedback_serial_subagent_dispatch` — never parallel), TDD per task, two-stage (spec-then-quality) review on the substantive tasks (the D2 filter is the review centerpiece), one code commit + one PROGRESS commit per task.

---

## 7. ADR posture

**ADR-0060 lands at THIS brainstorm commit** (the scope lock — see §0/§2.3). The DECISIONS.md ledger head moves from ADR-0059 to **ADR-0060**; the next-available number is **ADR-0061**.

Reserved conditional ADRs for state-2 / state-3:

- **ADR-0061 — §6.2 empirical-verification reconciliation.** Lands at the state-2 PLAN-write commit if any of the §6.2 items 1-6 (config shape / modify-method set / origin computation / 403 failure body / stat semantics / `filter_enabled` disposition) differs materially from this SPEC's projection. **Recommended posture: verify all at PLAN-write; land ADR-0061 inline if any diverges** (the config shape [chain-vs-route `filter_enabled`] and the modify-method set are the most likely triggers).
- **ADR-0062 — §6.1 split.** Lands only if the PLAN materializes over the §6.1 gate and phase 24 splits into 24.1/24.2 per §6.1. **Recommended posture: single-phase; split reserved but very unlikely** (the cross-cutting swing factor that made phase 23 borderline is gone).

At most one conditional ADR lands per commit (D-3.5 sequential numbering); if multiple fire, they take consecutive numbers.

---

## 8. State-machine signposts for the phase-24 state-2 session

- **Lifecycle state at session start:** State 2 (SPEC.md exists; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `docs/envoy-rust/phases/24-http-filter-csrf/PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble (standalone pre-Task-1 commit per the 06.x → 23 cadence).
- **Empirical verification at state 2 (per §6.2):** resolve the Envoy config shape FIRST (gates D1 — especially the chain-vs-route `filter_enabled` requirement); then verify the modify-method set / origin computation / 403 failure body / stat semantics / `filter_enabled` disposition against `envoyproxy/envoy:v1.33.0` (LOCAL Docker); land inline ADR-0061 if any diverges.
- **Split-gate evaluation:** §6.1. **Recommended: single-phase; split (ADR-0062) reserved but very unlikely.**
- **PLAN-time SPEC corrections:** the PLAN-writer reads this SPEC against HEAD `<state-1-commit-SHA>` and flags mechanical drift (the exact `PerFilterConfig`/`HttpFilterTypedConfig`/`HttpFilterInstance` shapes + their dispatch sites, the exact `StringMatcher` API + its match method, the `apply_route_config` threading signature, the `FilterRequest`/`FilterResponse` field shapes, the `Decision` variants, the `decorate_filter_synth_response{,_h2}` helpers, whether `RuntimeFractionalPercent` already exists) — corrections land in the PROGRESS Task 1 preamble per the 06.2 → 23 "N PLAN-write SPEC corrections" pattern.

---

## 9. Commit message format (for state 6 of the phase-24 lifecycle)

```
phase 24: envoy.filters.http.csrf (per-route typed_per_filter_config) + fixture 0032 [ADR-0060, ADR-00NN…]

<1-3 sentence summary>

Differential surface: fixture 0032-http-filter-csrf (H1); all 32 Docker-gated fixtures (0001-0032) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held; parse_bootstrap fuzz clean on its short-budget CI run.
```

The bracketed ADR list carries ADR-0060 (this brainstorm) + any §6.2/§split ADRs that fired. If phase 24 splits at state-2 into 24.1 + 24.2, the closing-sub-phase commit carries `[parent 24 done]` per the 07.2/08.2/12.2/13.2/14.2 closing-sub-phase precedent.

---

## 10. State-machine commit (this commit — phase-24 state-1 brainstorm close-out)

This SPEC is the state-1 output. The state-1 brainstorm commit (the state-0/1-collapsing cadence of phases 12-23) touches exactly five docs files:

- **CREATE** `docs/envoy-rust/phases/24-http-filter-csrf/SPEC.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — adds a new row beneath the "HTTP filters family" §9 heading, after the existing phase-23 row, `status: planned` (invariant 4.1.2 — a new row enters `planned`; no existing row flips; row 24 becomes `in-progress` only when the NEXT session's state-2 PLAN-write points STATE at it per invariant 4.1.3).
- **MODIFY** `docs/envoy-rust/DECISIONS.md` — append **ADR-0060** (the scoping decision + the minimum-viable scope + the alternatives-rejected analysis).
- **MODIFY** `docs/envoy-rust/STATE.md` — advance the Active-phase pointer from "AWAITING NEXT PLANNING" to `id: 24` / `slug: 24-http-filter-csrf` / `directory: docs/envoy-rust/phases/24-http-filter-csrf/` / state-1-complete / state-2-next; relocate the prior "AWAITING NEXT PLANNING" blocks verbatim to STATE_HISTORY.md per ADR-0035 / §4.1 invariant 9; rewrite `## Next expected skill` to the state-2 PLAN-write arc; append a `### Phase-24 state-1 brainstorm` Notes subsection; update `## Last commit` + `## Last updated`.
- **MODIFY** `docs/envoy-rust/STATE_HISTORY.md` — the ADR-0035 / §4.1 invariant-9 relocations of the superseded phase-23-close-out top-section blocks.

No code / fixture / Cargo / BEHAVIOR_CONTRACT change; no `unsafe`. The DECISIONS.md ledger head moves to **ADR-0060**. ADR-0014 remains in force; ADR-0028 remains open. ENVOY_TARGET.md + rust-toolchain.toml untouched. The brainstorm commit is docs-only → the CI run at this push is vacuous-green (the project's differential evidence remains the phase-23 state-4 CI anchor `27317400787` at code-HEAD `ff3721fd1`). Per `BOOTSTRAP_PROMPT.md` §5.1 (one state per session) this brainstorm session EXITS after this commit; the NEXT session writes `PLAN.md` (state 2 — `superpowers:writing-plans`, with the §6.2 empirical verification running LOCALLY).

**Commit message:**

```
phase 24: state-1 brainstorm — http-filter-csrf SPEC.md (HTTP-filter-family sixth phase; second per-route typed_per_filter_config consumer) [ADR-0060]
```

**Predecessor:** `3b7cf1384` — phase-23 state-6 close-out. **Origin/main:** `3b7cf1384` is HEAD; `origin/main` is 2 behind (the phase-23 state-5 REVIEW.md + state-6 close-out doc commits, unpushed — no CI obligation, both doc-only).

---

*End of SPEC. Phase 24 state-1 lifecycle complete on landing. The next session enters state 2 — writes PLAN.md per `superpowers:writing-plans`, resolves the Envoy CSRF config shape (the D1 gating probe — the chain-vs-route `filter_enabled` requirement), performs the §6.2 empirical verification LOCALLY (modify-method set / origin computation / 403 failure body / stat semantics / `filter_enabled` disposition), and evaluates the §6.1 split gate (single-phase recommended — the cross-cutting HCM change is already done).*
