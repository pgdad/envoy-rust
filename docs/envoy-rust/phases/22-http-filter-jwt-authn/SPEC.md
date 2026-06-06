# Phase 22 (`22-http-filter-jwt-authn`) — SPEC

- **Phase id:** `22`
- **Slug:** `22-http-filter-jwt-authn`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `a63d5a66d`, the phase-21 state-6 close-out commit; the "HTTP filters family" §9 heading carries three concrete rows — phase 09 `local_ratelimit` `done`, phase 10 `rbac` `done`, phase 11 `fault` `done`). **This SPEC's landing commit adds the fourth concrete row beneath the HTTP-filter-family heading**, with `status: planned`.
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"HTTP filters family — header manipulation, cors, compression, fault, local+global rate limit, **jwt_authn**, rbac, ext_authz, ext_proc, oauth2, csrf, buffer, lua, wasm, adaptive concurrency, admission control, bandwidth limit."* This phase lands `envoy.filters.http.jwt_authn` narrowed to the **minimum-viable RS256-with-inline-JWKS surface**: a single required provider per matched rule, default `Authorization: Bearer` token extraction, and `iss`/`aud`/`exp`/`nbf` claim validation. Remote JWKS fetch, JWKS caching, ES256/HS256/other algorithms, the `requires_any`/`requires_all`/`allow_missing`/`allow_missing_or_failed` requirement combinators, custom token sources, payload forwarding, and per-route `typed_per_filter_config` all defer per §4 below.
- **Position in the project:** the **first post-xDS-quartet feature phase** and the **fourth concrete HTTP-filter-family phase** (after phase-09 `local_ratelimit`, phase-10 `rbac`, phase-11 `fault`). The MVP trunk 00→08, the upstream-robustness family (12→17, complete in minimum-viable form), and the xDS / dynamic-config filesystem-transport quartet (18 CDS / 19 LDS / 20 RDS / 21 EDS, all `done`) all stand closed as of HEAD `a63d5a66d`. Phase 22 amortizes the framework + helper investment of phases 07/09/10/11 (the `Decision::StopAndSend(FilterResponse)` decode-side short-circuit discipline; the H1 `decorate_filter_synth_response` + the phase-11 H2 `decorate_filter_synth_response_h2` filter-synth decoration helpers, now at parity across codecs; the per-filter `StatsRegistry` counter-wiring pattern; the `04.x` `RouteMatch` reuse) **and is the first phase in the project to perform application-layer cryptographic verification** (JWT RS256 signature verification) — which is what motivates the new isolated `envoy-jwt` crate (§5.1) and the `aws-lc-rs` foundations reinterpretation locked by **ADR-0055**.
- **depends-on:** `07` (the parent filter-chain framework). Phase 22 extends the 07.1-landed `envoy-filter::FilterPipeline` + `HttpFilterInstance` enum with a sixth production variant (after `Router` at 07.1, `HeaderMutation` at 07.2, `LocalRateLimit` at 09, `Rbac` at 10, `Fault` at 11). Implicit (non-`depends-on`-field) dependencies, per the ROADMAP schema convention that the field captures only direct ROADMAP-row dependencies: phase `04` (the `RouteMatch` type reused for `rules[].match`; the HTTP/1.1 codec + HCM + router data-plane), phase `05` (the H2 codec — exercised by the codec-agnostic filter even though the phase-22 differential fixture is H1), phase `06` (the `StatsRegistry` + admin `/stats` surface the `jwt_authn.*` counters land on). The 29-Docker-gated-fixture regression baseline established at phase-21 close (`0001-tcp-echo` through `0029-xds-file-based-eds`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **Brainstorm narrative:** see the "Phase-22 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + filter-pick rationale with the alternatives considered (cors / compression / load-balancing / xDS-watching / SDS / gRPC / EDS-CDS-composition) along the established scoring axes, and ADR-0055 for the scoping decision.

---

## 0. Critical scoping findings (READ FIRST) — jwt_authn is self-contained (needs NO per-route config) but is the project's first crypto-in-a-filter

Two findings shaped the phase pick + scope and must anchor the PLAN-write:

1. **jwt_authn does its own request matching and is therefore self-contained.** Upstream Envoy's `JwtAuthentication` carries a top-level `rules: []RequirementRule`, each a `{ match: RouteMatch, requires: JwtRequirement }`. The filter consults its OWN `rules[].match` against the request — it does **not** read per-route `typed_per_filter_config`. This is load-bearing: **per-route `typed_per_filter_config` does not exist in envoy-config today** (`Route` at `crates/envoy-config/src/bootstrap.rs:1098-1104` and `VirtualHost` carry zero per-filter config; grep-confirmed empty at the brainstorm HEAD). The `cors` filter — whose policy IS per-route in Envoy — was rejected for phase 22 precisely because it would force a cross-cutting per-route-config framework lift (a likely split); jwt_authn was chosen because it sidesteps that entirely by reusing the existing `04.x` `RouteMatch` (`bootstrap.rs:1386`) for its self-contained `rules[]`. The per-route `typed_per_filter_config` infrastructure remains deferred to its own future phase (the natural cors close site — §4).

2. **Phase 22 is the first application-layer cryptographic verification in the project.** JWT RS256 verification = RSA-PKCS1-SHA256 over the `header.payload` signing input against a JWKS-supplied RSA public key. `aws-lc-rs` — listed in `BOOTSTRAP_PROMPT.md` D-3.2 as *"permitted as the crypto provider"* and already present in `Cargo.lock` transitively (via `rustls`/`tokio-rustls`'s `aws-lc-rs` feature, used by `envoy-tls`/`envoy-tcp`/`envoy-bin`) — supplies the RSA verification primitive. **ADR-0055 (landed at THIS brainstorm commit) reinterprets the D-3.2 grant to cover JWT signature verification (not only TLS), makes `aws-lc-rs` a DIRECT dependency of the new isolated `envoy-jwt` crate, and confirms no forbidden dependency is pulled** (base64url decoding is hand-rolled — ~40 lines, no `unsafe`, no new crate; JSON via the already-permitted `serde_json`). This is the only genuinely novel foundations engagement in the phase; everything else reuses the 07/09/10/11 filter machinery.

---

## 1. Goal and acceptance signal

Phase 22 lands the `envoy.filters.http.jwt_authn` filter (per upstream Envoy v1.33's documented filter name; typed_config `@type = type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication`) as the **sixth `HttpFilterInstance` variant** (after Router, HeaderMutation, LocalRateLimit, Rbac, Fault) and the **fifth concrete pluggable feature filter**. The filter, configured with one or more JWT `providers` (each carrying an inline local JWKS) and a list of `rules` mapping a `RouteMatch` to a single required provider, evaluates each request: it selects the first matching rule, extracts the JWT from the `Authorization: Bearer` header, verifies the RS256 signature against the provider's JWKS, and validates the `iss`/`aud`/`exp`/`nbf` claims. On success the request proceeds to the next filter (`Decision::Continue`); on any failure the filter short-circuits via `Decision::StopAndSend(FilterResponse)` with HTTP 401 + the Envoy-faithful failure body + the `WWW-Authenticate` header, decorated with the standard response headers by the existing HCM filter-synth decoration helpers.

**Differential surface added by phase 22:**

- **Fixture `0030-http-filter-jwt-authn`** — bilateral assertion that both proxies, given an identical bootstrap with a `jwt_authn` filter configured with one RS256 provider (inline JWKS RSA public key, `issuer: testing@secure.istio.io` or the §6.2-confirmed canonical test issuer), one audience, and one rule (`prefix: "/"` → `provider_name`), produce the deterministic per-probe status + body sequence on a multi-probe burst over an **HTTP/1.1** listener: probe 1 (`Authorization: Bearer <valid-token>`) → 200 (the downstream `direct_response`/proxied body); probe 2 (no `Authorization`) → 401 (`Jwt is missing`); probe 3 (`Authorization: Bearer <signature-tampered-token>`) → 401 (`Jwt verification fails`); probe 4 (`Authorization: Bearer <expired-token>`) → 401 (`Jwt is expired`). The valid/tampered/expired tokens are **static pre-generated test data committed under the fixture's `inputs/`** (a fixed RSA test keypair; the valid token carries a far-future `exp` so both proxies accept it deterministically and forever; the expired token carries a past `exp` so both reject it deterministically) — **no runtime key generation, no clock sensitivity**. The exact 401 body bytes per failure class, the `WWW-Authenticate` header value, the `jwt_authn` stat namespace, and the on-success token-forwarding disposition are **empirically verified at state-2 PLAN-write per §6.2** (the phase-10/11-ratified verify-at-PLAN-write process improvement — the phase-10 RBAC body was off by one byte, so failure-body bytes are NOT assumed).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0030-http-filter-jwt-authn` green at Docker-gated CI.
- **(b)** All **29 pre-existing differential fixtures** (`0001-tcp-echo` through `0029-xds-file-based-eds`) **remain green simultaneously** at the same CI run (regression-equivalence per `BOOTSTRAP_PROMPT.md` §7.5 (b)).
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline). Phase 22 does not touch the H2 framing path (the filter is codec-agnostic; the differential fixture is H1; the phase-11 H2 filter-synth decoration helper is reused unchanged) — the state-4 verification re-runs `h2spec` to confirm no regression.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (one new seed for the jwt_authn bootstrap shape; curated seed corpus extends 32 → 33). **A second new fuzz target is recommended (§6.x): `parse_jwks` / `verify_jwt` over the new `envoy-jwt` parsing+verification surface** — per `BOOTSTRAP_PROMPT.md` §7.4 (every phase that introduces a parser/codec ships a fuzz target); the JWT/JWKS parser is a new untrusted-input surface. The PLAN-writer decides the exact target shape at §6.2.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean. **`cargo deny check` is load-bearing this phase** — the new direct `aws-lc-rs` dependency must pass the license/advisory policy (it is already a transitive dep, so the policy already admits it; the state-4 verification confirms). **The 4 standalone-crate builds** (`project_isolated_crate_build_blindspot`) at state-4 must include the new `envoy-jwt` crate (`cargo build -p envoy-jwt`).
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent through 06.1 / 07.x / 08.x / 09 / 10 / 11 / … / 21 — fixture inheritance is a regression vector).

---

## 2. Behavior-contract scope for phase 22

Phase 22 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x → 21 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — 2 new rows (projected; §6.2-verified)

Upstream Envoy's `envoy.filters.http.jwt_authn` emits a per-filter stat scope with at minimum `<prefix>.jwt_authn.allowed` and `<prefix>.jwt_authn.denied` counters (plus per-provider/JWKS-cache stats that defer alongside their features). At phase-22 minimum-viable scope, two counters are wired (projected; the exact namespace root — whether `http.<hcm_stat_prefix>.jwt_authn.*` like fault/rbac, or a top-level `jwt_authn.*` scope — is **§6.2-verified**, since jwt_authn's upstream stat rooting differs from the HCM-prefixed filters in some Envoy versions):

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<hcm_stat_prefix>.jwt_authn.allowed` (namespace §6.2-verified) | value-exact | Counter; one increment per request that passes JWT verification (or that matches no rule, if the no-rule disposition is "allow" — §6.2). |
| `http.<hcm_stat_prefix>.jwt_authn.denied` (namespace §6.2-verified) | value-exact | Counter; one increment per request the filter rejects with 401. |

**Namespace empirical-verification signpost:** the recommended state-1 projection is the `http.<hcm_stat_prefix>.jwt_authn.*` shape (the fault/rbac precedent). The state-2 PLAN-writer empirically verifies the exact namespace against `envoyproxy/envoy:v1.33.0` + admin `/stats` scrape before locking; if reality differs (e.g. a top-level `jwt_authn.*` root or per-provider sub-scopes), the SPEC §2.1 + D7 revision lands via the inline reconciliation ADR-0056 at PLAN-write per D-3.5.

The differential fixture 0030 does **not** scrape jwt_authn stats (it asserts the per-probe status + body sequence + the `WWW-Authenticate` header on 401s); the counters are exercised by the in-process backstop (D8.3) + unit tests. This mirrors the phase-10/11 posture.

### 2.2 "Response body — 401 failure local replies" extension — N rows (§6.2-verified)

Upstream Envoy's jwt_authn returns distinct, source-hardcoded plain-text bodies per failure class (e.g. `Jwt is missing`, `Jwt verification fails`, `Jwt is expired`, `Jwt issuer is not configured`, `Audiences in Jwt are not allowed`, `Jwt header is an invalid JWT`). At phase-22 scope the fixture exercises the missing / verification-fails / expired classes; each asserted body's **exact bytes are §6.2-verified** (the phase-10 RBAC-body-off-by-one-byte lesson makes this load-bearing — the projections above are NOT assumed correct). A "Response body — jwt_authn 401 local replies" subsection lands in BEHAVIOR_CONTRACT at the task that first exercises each body, recording the byte-exact body + the accompanying `WWW-Authenticate` header value per failure class.

### 2.3 "Header allow-list" extension — `WWW-Authenticate` (§6.2-confirmed)

Envoy's jwt_authn 401 responses carry a `WWW-Authenticate` header (value §6.2-verified — Envoy emits a `Bearer realm="…"` or `Bearer error="invalid_token"` shape). If the header value is identical across proxies (envoy-rust emits the same static value), it is asserted value-exact and needs no allow-list row; if it carries an implementation-identifying difference, a Header allow-list row lands. The recommended posture is value-exact (envoy-rust mirrors Envoy's static value verbatim) — confirmed at §6.2.

### 2.4 DECISIONS.md — ADR-0055 lands at THIS SPEC commit; ADR-0056 reserved for PLAN-write

Unlike the phase-09/10/11 filter brainstorms (which landed no ADR at state-1 because they fit cleanly in the existing foundations), phase 22 lands **ADR-0055** at THIS brainstorm commit — the crypto/foundations reinterpretation (`aws-lc-rs` for JWT signature verification + the new `envoy-jwt` crate) and the scope lock are genuine decisions per D-3.5, mirroring the xDS-family brainstorm-ADR cadence (ADR-0048/0050/0051/0053). **ADR-0056 is reserved** for the §6.2 empirical-verification reconciliation at PLAN-write (most-likely trigger: the 401 body bytes / the stat namespace / the on-success forwarding disposition).

---

## 3. Deliverables

Phase 22's scope is enumerated as deliverables `D1`–`D8`. **The state-2 PLAN-writer organizes deliverables into tasks** (and evaluates the §6.1 split gate) — these are not 1:1 with tasks. Listed in roughly execution order; the SPEC constrains the surface, not the task order.

### D1 — new `envoy-jwt` crate (JWT/JWKS parsing + RS256 verification + claim validation)

A new workspace member `crates/envoy-jwt/` (added to `Cargo.toml` `members`), isolating all cryptographic + JWT/JWKS-parsing logic behind a small, well-bounded interface — mirroring the `envoy-health` crate-isolation precedent (a new crate owning one capability with a clean DAG). Crate root `#![forbid(unsafe_code)]` per D-3.8. Direct dependencies: `aws-lc-rs` (RS256 verification — the ADR-0055 grant), `serde`/`serde_json` (JWT header/claims + JWKS JSON — permitted), `thiserror` (typed errors — permitted), `bytes` (optional). **No `base64` crate** — base64url decode is hand-rolled (a small pure-compute decoder; ~40 LoC + tests). Proposed surface:

```rust
#![forbid(unsafe_code)]

/// A parsed JWKS (one or more RSA public keys), built from an inline JWKS JSON string.
pub struct JwkSet { /* keys: Vec<RsaKey> (kid, n, e) */ }
impl JwkSet { pub fn parse(jwks_json: &str) -> Result<Self, JwtError>; }

/// The verified-and-validated claims a caller may inspect (iss/aud/exp/nbf + the raw payload).
pub struct VerifiedJwt { /* iss, aud: Vec<String>, exp, nbf, raw_payload_json */ }

/// Verify an RS256 JWT against a JWKS and validate registered claims.
/// `now_unix` is injected (no ambient clock) so the caller controls time → testability.
pub fn verify_rs256(
    token: &str,
    jwks: &JwkSet,
    expected_issuer: &str,
    allowed_audiences: &[String],   // empty ⇒ audience check skipped
    now_unix: i64,
) -> Result<VerifiedJwt, JwtError>;

/// The failure taxonomy — maps 1:1 to Envoy's 401 failure classes (§6.2 body bytes).
pub enum JwtError {
    Missing, MalformedToken, UnsupportedAlgorithm, NoMatchingKey,
    SignatureInvalid, IssuerMismatch, AudienceNotAllowed, Expired, NotYetValid, /* … */
}
```

**Signposts:** (a) **time is injected** (`now_unix: i64`) — the filter passes a real clock at runtime, the unit tests pass fixed values, and the differential fixture uses far-future/past `exp` so the clock never matters cross-proxy. (b) **JWKS→RSA-public-key construction** is the fiddliest crypto step: a JWKS RSA key is `{kty:"RSA", n:<base64url>, e:<base64url>}`; verification needs these as an `aws-lc-rs` RSA public key. The PLAN-writer §6.2-verifies whether the pinned `aws-lc-rs` exposes `rsa::PublicKeyComponents { n, e }` (the clean path — verify directly from raw modulus/exponent) or whether a small DER-assembly of `RSAPublicKey ::= SEQUENCE { INTEGER n, INTEGER e }` is needed (~40 LoC, no `unsafe`). Either path is bounded; this is the single load-bearing crypto-API uncertainty to resolve at PLAN-write. (c) **RS256 only** at phase-22 scope — the JWT header `alg` must be `RS256`; any other algorithm (`ES256`, `HS256`, `none`, …) → `UnsupportedAlgorithm` 401 (ES256/HS256 defer per §4).

### D2 — `envoy-config` schema extension

At `crates/envoy-config/src/bootstrap.rs`, extend `HttpFilterTypedConfig` (currently `Router`/`HeaderMutation`/`LocalRateLimit`/`Rbac`/`Fault` at `bootstrap.rs:690-711`) with a sixth variant `JwtAuthn(JwtAuthnConfig)` (typed_config `@type = type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication`). The config struct mirrors upstream v1.33's `JwtAuthentication`, narrowed to minimum-viable:

```rust
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JwtAuthnConfig {
    pub providers: BTreeMap<String, JwtProvider>,   // REQUIRED, ≥1
    #[serde(default)]
    pub rules: Vec<RequirementRule>,                // REQUIRED ≥1 at phase-22 minimum
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JwtProvider {
    pub issuer: String,                             // REQUIRED
    #[serde(default)]
    pub audiences: Vec<String>,                     // OPTIONAL; empty ⇒ no aud check
    pub local_jwks: DataSource,                     // REQUIRED (inline_string only at phase 22)
    #[serde(default)]
    pub forward: bool,                              // default per §6.2 (Envoy default = false)
    // DEFER (deny_unknown_fields rejects): remote_jwks, from_headers, from_params,
    //   from_cookies, forward_payload_header, payload_in_metadata, claim_to_headers,
    //   clock_skew_seconds, jwks_cache_duration, jwt_cache_config, …
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RequirementRule {
    pub r#match: RouteMatch,                        // REUSE 04.x RouteMatch (bootstrap.rs:1386)
    pub requires: JwtRequirement,                   // minimum: { provider_name }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JwtRequirement {
    pub provider_name: String,                      // minimum-viable single-provider requirement
    // DEFER: requires_any, requires_all, allow_missing, allow_missing_or_failed
}

// DataSource { inline_string } — a minimal data-source type (inline only at phase 22);
//   if no DataSource type exists in envoy-config yet, author a minimal one here, reusable
//   by future SDS/filter work. PLAN-write confirms whether one already exists.
```

All structs carry `#[serde(deny_unknown_fields)]` per the established envoy-config discipline (rejects the deferred forward-looking fields). `RouteMatch` reuses the existing `04.x` type directly. The PLAN-writer confirms at §6.2 whether a `DataSource`/`inline_string` shape already exists to reuse vs. author a minimal one.

### D3 — `envoy-config` validator extension

At `crates/envoy-config/src/bootstrap.rs::validate_http_filters`, add a `JwtAuthn` arm calling `validate_jwt_authn_config(cfg) -> Result<(), ConfigError>`. Checks (minimum-viable):

- `providers` is non-empty (`ConfigError::JwtAuthnNoProviders`).
- each `rules[].requires.provider_name` names an existing entry in `providers` (`ConfigError::JwtAuthnUnknownProvider { provider_name }` — a dangling reference is operator error caught at config load, mirroring the cluster-reference-resolution discipline).
- each provider's `local_jwks.inline_string` parses as a JWKS at config load (fail-fast `ConfigError::JwtAuthnInvalidJwks` — calls `envoy_jwt::JwkSet::parse`; a malformed JWKS is a startup-fatal config error, consistent with envoy-rust's all-fatal config posture).
- each `rules[].match` is a structurally valid `RouteMatch` (delegates to the existing `RouteMatch` parse-time validation).

New `ConfigError` variants land here (consolidatable at the PLAN-writer's discretion), each carrying `listener: String` per the envoy-config error-context discipline, each with positive + negative unit tests, exercised by the `parse_bootstrap` fuzz target (the new seed per D8.2). The `envoy.filters.http.jwt_authn` name is currently in the unsupported-filter reject list at `crates/envoy-filter/src/error.rs` (`UnsupportedHttpFilter`); D2+D3+D4+D5 collectively move it from rejected to supported — the PLAN-writer removes the reject entry + its test at the appropriate task.

### D4 — `envoy-filter::JwtAuthnFilter` runtime

New module `crates/envoy-filter/src/jwt_authn.rs`. Hand-rolled per D-3.2 (one module per concrete filter; the 07.2/09/10/11 precedent), consuming `envoy-jwt` for the crypto + `envoy-config` for the config/`RouteMatch` + `envoy-stats` for the counters. Module shape:

```rust
#![forbid(unsafe_code)]   // inherited from crate root

pub struct JwtAuthnFilter {
    rules: Vec<CompiledRule>,                       // (RouteMatch, provider_name) in order
    providers: BTreeMap<String, CompiledProvider>,  // (issuer, audiences, JwkSet, forward)
    allowed: Arc<Counter>,
    denied: Arc<Counter>,
}

impl JwtAuthnFilter {
    pub(crate) fn build_from_config(
        cfg: &envoy_config::JwtAuthnConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> { /* parse each provider's JWKS once; register counters */ }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        // 1. Select the FIRST rule whose match matches req (path + headers). §6.2: no-match disposition.
        // 2. Extract token from `Authorization: Bearer <token>` (default source; §4 custom-source defer).
        // 3. envoy_jwt::verify_rs256(token, &provider.jwks, &provider.issuer, &provider.audiences, now).
        // 4. Ok  → self.allowed.inc(); (forward per §6.2 disposition) Decision::Continue.
        //    Err → self.denied.inc(); Decision::StopAndSend(FilterResponse {
        //            status: 401, headers: vec![("www-authenticate", <§6.2>)],
        //            body: Bytes::from_static(<§6.2 body per JwtError class>) }).
        unimplemented!()
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue                            // decode-only at phase-22 scope
    }
}
```

**Signposts:** (a) **JWKS parsed once at build time** (in `build_from_config`), not per-request — the per-request path is parse-token + verify-signature + validate-claims (pure compute, no I/O, no async; remote JWKS would be the first async lift — deferred §4). (b) **first-match rule selection** per Envoy semantics; the no-matching-rule disposition (allow vs require-nothing) is §6.2-verified. (c) **token extraction** is the default `Authorization: Bearer ` prefix strip only (case-insensitive header name via the existing header-access helper; custom `from_headers`/`from_params`/`from_cookies` defer §4). (d) **clock** is injected from a real `SystemTime::now()` → unix-seconds at the call site (the fixture's far-future/past `exp` makes it cross-proxy-irrelevant).

### D5 — `HttpFilterInstance::JwtAuthn` variant + dispatch

Extend `crates/envoy-filter/src/instance.rs::HttpFilterInstance` with a `JwtAuthn(JwtAuthnFilter)` variant; extend the `build` dispatch (calls `JwtAuthnFilter::build_from_config(cfg, registry, hcm_stat_prefix)` — reusing the phase-10 3-arg threading unchanged, no further signature widening) + the `decode_headers`/`encode_headers` dispatch arms. New variant lands after `Fault` and before the `#[cfg(feature = "test-util")]` block (the 09/10/11 placement precedent). Re-export `JwtAuthnFilter` from `crates/envoy-filter/src/lib.rs`. The decode-side `Decision::StopAndSend` 401 flows through the existing H1 `decorate_filter_synth_response` (`crates/envoy-http1/src/hcm.rs`) and — for any H2 deployment — the phase-11 `decorate_filter_synth_response_h2` helper, **both unchanged** (no new HCM writer-path work this phase; the phase-11 M2 close put both codecs at parity).

### D6 — (reserved — no codec/HCM writer-path work)

Unlike phase 11 (which added the H2 decoration helper), phase 22 introduces **no new HCM writer-path deliverable** — the filter-synth decoration helpers for both codecs already exist and are reused. This slot is intentionally empty; the crypto crate (D1) is phase 22's structural centerpiece in place of phase 11's D6.

### D7 — Stats wiring (2 counters) + BEHAVIOR_CONTRACT extension

At `JwtAuthnFilter::build_from_config`, register `allowed` + `denied` `Counter` handles against the `Arc<StatsRegistry>` at the §6.2-verified namespace (projected `http.<hcm_stat_prefix>.jwt_authn.{allowed,denied}`). Increment sites in `decode_headers` (`allowed.inc()` on verification success / `denied.inc()` on each 401). The BEHAVIOR_CONTRACT extensions (§2.1 stat rows + §2.2 401-body rows + §2.3 `WWW-Authenticate`) land at the tasks where each is first empirically exercised, per the 06.x → 21 cadence.

### D8 — Fixture + harness + fuzz seeds + in-process backstop

- **D8.1 — Fixture `tests/fixtures/0030-http-filter-jwt-authn/`.** An **HTTP/1.1** listener (the filter is codec-agnostic; H1 reuses `Driver::Http1ProbeList`, the simplest deterministic multi-probe driver — no new harness driver needed, unlike phase 11's H2 `Http2ProbeList`). Bootstrap: H1 HCM + the jwt_authn filter (1 RS256 provider with inline JWKS, 1 audience, 1 rule `prefix: "/"` → provider) + router → a static cluster (or a `direct_response: { status: 200, body: "ok\n" }` to keep the data plane trivial). **Static test data committed under `inputs/`:** the RSA test keypair's public JWKS (inline in both `envoy.yaml` + `envoy-rust.yaml`), and three pre-signed tokens (valid far-future-`exp`, signature-tampered, expired-past-`exp`). The token/keypair generation is a one-time author step (documented in the fixture README per the fixture-divergence discipline); the committed tokens are static → deterministic forever. Probe burst: `[200, 401(missing), 401(verify-fails), 401(expired)]`. Asserts each 401 body byte-exact (§6.2) + the `WWW-Authenticate` header + the 200 body. Docker-gated wrapper `tests/differential/tests/http_filter_jwt_authn.rs` (the 09/10/11 precedent), one `#[tokio::test]` invoking `run_fixture("0030-http-filter-jwt-authn")`.

  **Pre-existing-Envoy-test-key signpost:** Envoy's own jwt_authn tests ship a well-known RSA test key + canonical tokens (`issuer: testing@secure.istio.io`, the Istio sample JWKS). The PLAN-writer §6.2-verifies whether reusing that canonical keypair/JWKS (so the fixture's tokens validate identically on upstream Envoy AND envoy-rust) is the cleanest path vs. minting a fresh keypair — the only requirement is that BOTH proxies are configured with the SAME JWKS and driven with the SAME tokens (the differential contract compares outputs on identical inputs).

- **D8.2 — Fuzz seeds.** (i) New `parse_bootstrap` corpus seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_jwt_authn_filter.yaml` (the bootstrap shape above), extending the curated seed corpus 32 → 33 (+ the `.gitignore` allow-list + the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array, edited together per the 09/10/11/21 cadence). (ii) **A new fuzz target over the `envoy-jwt` parse/verify surface** (recommended `crates/envoy-jwt/fuzz/` — `parse_jwks` and/or `verify_jwt` harness) per `BOOTSTRAP_PROMPT.md` §7.4 (the JWT/JWKS parser is a new untrusted-input surface; malformed tokens/JWKS must not panic and must produce the `JwtError`/401 class). The PLAN-writer decides the exact target(s) + seed corpus at §6.2; the state-4 short-budget CI run covers them.

- **D8.3 — In-process backstop.** New file `crates/envoy-bin/tests/http_filter_jwt_authn.rs` mirroring `crates/envoy-bin/tests/http_filter_fault.rs`/`http_filter_rbac.rs` (the standing `tokio::process::Command` + `.kill_on_drop(true)` subprocess discipline from 09 REVIEW M3). Boots `envoy-bin` (H1) with a synthesized jwt_authn bootstrap; issues sequential `GET /` probes with varying `Authorization` values; asserts the `[200, 401, 401, 401]` status sequence + each 401 body (§6.2 bytes) + the `WWW-Authenticate` header presence on 401 probes + the 200 body (heeds the phase-10 M1 backstop-header-assertion lesson — include the assertion OR disclose its omission in PROGRESS). **The M21-3 / M18-9 extract-a-shared-test-support-crate item is now at N≥6 backstops** — the PLAN-writer notes the duplication in the file header (the consolidation stays deferred by the standing risk-managed decision unless the PLAN-writer judges otherwise).

---

## 4. Out of scope (deferred non-goals)

Phase 22 explicitly does NOT land:

- **Remote JWKS (`remote_jwks` — fetch from an HTTP/cluster URI) + JWKS caching/refresh (`jwks_cache_duration`).** Requires an async fetch against a configured cluster + cache lifecycle + the first I/O in the JWT path. Phase 22 is inline-`local_jwks` only. Defers to a JWT-enrichment phase (and engages the cluster/HTTP-client machinery already present from 12.x/13.x).
- **Algorithms other than RS256 (`ES256`/`ES384`/`RS384`/`RS512`/`PS256`/`HS256`/`EdDSA`).** RS256 is the dominant production algorithm and the cleanest single first cut. Other algorithms (and the symmetric `oct`/HMAC JWKS key type) defer; the `envoy-jwt` `verify_*` surface is authored to admit them additively.
- **Requirement combinators (`requires_any`, `requires_all`, `allow_missing`, `allow_missing_or_failed`) + `requirement_name`/named requirements.** Phase 22 supports a single `provider_name` requirement per matched rule. The combinators defer (they layer on the single-provider verify primitive).
- **Custom token sources (`from_headers` with custom name/`value_prefix`, `from_params`, `from_cookies`).** Phase 22 extracts from the default `Authorization: Bearer ` header only. Custom sources defer.
- **Payload forwarding / metadata (`forward_payload_header`, `payload_in_metadata`, `claim_to_headers`, `header_in_metadata`).** Phase 22 forwards the request per the §6.2-confirmed default `forward` disposition; it does not inject decoded-claim headers or populate dynamic metadata. Defers.
- **`clock_skew_seconds` configurability.** Phase 22 uses Envoy's default clock skew (§6.2); configurable skew defers. The differential fixture's far-future/past `exp` makes skew cross-proxy-irrelevant regardless.
- **JWT result caching (`jwt_cache_config`).** A per-token verification cache is a performance optimization with no differential-observable effect at deterministic scope. Defers.
- **Per-route `typed_per_filter_config` for jwt_authn (and the per-route-config framework generally).** Phase 22's jwt_authn sources its config exclusively from the filter-chain-level entry and self-matches via `rules[]`. The per-route `typed_per_filter_config` infrastructure remains unbuilt (the §0 finding) and is the natural **cors close site** — a future HTTP-filter-family phase builds the per-route lookup primitive (the gating architectural change) and lands cors as its first consumer.
- **gRPC JWT (gRPC-status failure responses) + `bypass_cors_preflight`.** Defer (the gRPC family is still blocked on H2 trailers; CORS is unbuilt).
- **Runtime-keyed enablement.** The RTDS runtime layer is unimplemented (Runtime + hot restart family). Defers.

---

## 5. Architectural invariants

### 5.1 Crate boundaries — the new `envoy-jwt` crate isolates crypto

- **`envoy-jwt` (NEW workspace member) is the sole owner of JWT/JWKS parsing + RS256 verification + claim validation.** Clean DAG: `envoy-jwt` depends on `aws-lc-rs` + `serde`/`serde_json` + `thiserror` only — it does NOT depend on `envoy-config`/`envoy-filter`/`envoy-http*` (it is a leaf crypto/parse crate, like `envoy-stats` is a leaf). This mirrors the `envoy-health` isolation precedent and keeps the only `aws-lc-rs`-direct-dependent application code (the JWT crypto) behind one auditable boundary per D-3.4 (well-bounded units). `#![forbid(unsafe_code)]` at the crate root (D-3.8 — no exemption; `aws-lc-rs` encapsulates the FFI internally).
- **`JwtAuthnFilter` lives at `crates/envoy-filter/src/jwt_authn.rs`** (one module per concrete filter; the 07.2/09/10/11 pattern). `envoy-filter` gains a new path-dep on `envoy-jwt` (the only new workspace path-dep this phase).
- **The config types (`JwtAuthnConfig`/`JwtProvider`/`RequirementRule`/`JwtRequirement` + any minimal `DataSource`) live in `crates/envoy-config/`** alongside the other shared config types. `envoy-config`'s validator (D3) calls `envoy_jwt::JwkSet::parse` for fail-fast JWKS validation → `envoy-config` gains a path-dep on `envoy-jwt` (a leaf dep; no cycle, since `envoy-jwt` depends on neither).

### 5.2 Hand-rolled filter per D-3.2; crypto via the permitted provider per ADR-0055

The filter logic (rule selection, token extraction, claim validation, 401 emission) is hand-rolled per D-3.2 (*"Every individual filter ... Must be written from scratch"*). The RS256 signature primitive is delegated to `aws-lc-rs` (the D-3.2 *permitted crypto provider*, reinterpreted by ADR-0055 to cover JWT signature verification, not only TLS) — this is the from-scratch/permitted-foundation boundary: envoy-rust writes the JWT/JWKS parsing, the verification orchestration, and the claim logic; it does not hand-roll RSA/SHA-256 primitives (which would be both error-prone and a worse-than-foundation result per D-3.2). base64url decode IS hand-rolled (trivial, no dep). **No forbidden dependency is pulled; no new foundations grant beyond the ADR-0055 reinterpretation is required.**

### 5.3 Decode-side authentication filter (encode no-op)

`JwtAuthnFilter::encode_headers` is a no-op (`Decision::Continue`) — JWT authentication is a decode-side gate. The encode-side method exists per the 07.x framework symmetry.

### 5.4 Filter-chain config only (NOT per-route)

The sole config source is the filter-chain-level entry; jwt_authn self-matches via `rules[]` (§0/§4). No per-route `typed_per_filter_config` is built or consumed.

### 5.5 Determinism across both proxies (differential-testability invariant)

JWT verification is a **pure function** of (token, JWKS, expected issuer, allowed audiences, current time). Cross-proxy determinism holds because: (a) the token + JWKS are byte-identical static fixture data on both proxies; (b) RS256 verification is deterministic (same signing input + same public key → same accept/reject); (c) the `exp`/`nbf` checks are made cross-proxy-irrelevant by the fixture's far-future-`exp` valid token + past-`exp` expired token (no clock-sensitivity window). This is what makes jwt_authn fully differential-testable under `BOOTSTRAP_PROMPT.md` §7.2 — unlike a filter whose outcome depends on wall-clock-near-`exp` timing (which the fixture deliberately avoids). The `allowed`/`denied` counters increment exactly once per request (one increment per terminal decision; no double-counting).

### 5.6 Statelessness across requests

`JwtAuthnFilter` carries no mutable per-request state (the JWKS + compiled rules + providers are immutable post-`build_from_config`; the `jwt_cache` that would add per-token state defers per §4). Per `Clone` of `HttpFilterInstance`, each per-request pipeline-clone shares the `Arc<Counter>` handles + the `Arc`-wrapped immutable provider/rule data (the PLAN-writer wraps the compiled providers/JWKS in `Arc` to keep the per-request clone cheap — the JWKS RSA keys are not trivially `Clone`-cheap).

### 5.7 H1 + H2 symmetric (filter-layer codec-agnostic; no new writer-path work)

The filter operates on the codec-agnostic `FilterRequest`/`FilterResponse` abstraction (the 07.1 framework + ADR-0031). The 401 `Decision::StopAndSend` is decorated by the existing per-codec helpers (H1 since 09; H2 since the phase-11 M2 close) — both reused unchanged. The phase-22 differential fixture is H1; the H1 backstop covers the H1 codec; the codec-agnostic filter + the phase-11-proven H2 decoration cover any H2 deployment without a phase-22 H2 fixture (a standalone H2 jwt fixture is unnecessary — the same posture phase 11 took for its H1 path).

---

## 6. Implementation signposts for the planner

### 6.1 Split-gate evaluation (read first)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 tasks OR ~1500 LoC. Phase 22's SPEC-time surface estimate:

- D1 — `envoy-jwt` crate (JWKS parse + base64url + RS256 verify + claim validation + `JwtError`) (~350 LoC + ~300 LoC unit tests). ~2 tasks.
- D2 — envoy-config schema (`JwtAuthnConfig` + `JwtProvider` + `RequirementRule` + `JwtRequirement` + `DataSource`) (~140 LoC + ~120 LoC tests). ~1 task.
- D3 — envoy-config validator (provider-ref + JWKS-parse + RouteMatch) (~90 LoC + ~120 LoC tests). ~1 task or co-located with D2.
- D4 — `JwtAuthnFilter` runtime (rule select + token extract + verify orchestration + 401 emit) (~200 LoC + ~200 LoC tests). ~1 task.
- D5 — `HttpFilterInstance::JwtAuthn` variant + dispatch + `error.rs` reject removal (~50 LoC + ~30 LoC tests). ~1 task.
- D7 — stats wiring (2 counters) + BEHAVIOR_CONTRACT rows (~40 LoC + ~40 LoC tests). Co-located with D4.
- D8.1 — fixture 0030 + Docker wrapper + static token/JWKS inputs (~120 LoC YAML + ~60 LoC wrapper + token data). ~1 task.
- D8.2 — fuzz seeds (`parse_bootstrap` seed + the new `envoy-jwt` fuzz target) (~60 LoC + corpus). ~1 task.
- D8.3 — in-process backstop (~190 LoC). ~1 task.
- State-4 verification + STATE-advance (~docs). ~1 task.

**SPEC-time projection: ~11-13 tasks; ~1350-1650 LoC** (production ~870, tests ~750, fixture/harness/fuzz/doc ~300). The phase sits **at the upper edge of the §6.1 ~1500-LoC gate** (the crypto crate is the swing factor). **Recommended posture: single-phase, BUT the split valve is held in clear reserve** — if the §6.2 PLAN-write materializes the crypto crate (the JWKS-RSA-key DER-assembly path, or the second fuzz target) clearly over ~1600 LoC / past ~14 tasks, split into **`22.1` (the `envoy-jwt` crate + the envoy-config schema/validator + the `envoy-jwt` fuzz target, proven via unit tests — a foundation slice with no new fixture, the 12.1/14.1 pattern)** and **`22.2` (the `JwtAuthnFilter` + `HttpFilterInstance` variant + stats + fixture 0030 + backstop + parent-22 close)**. The split ADR would be **ADR-0057** (ADR-0056 is reserved for the §6.2 reconciliation). The crypto-foundation-uncertainty (the JWKS→`aws-lc-rs`-key path) makes this the most split-likely HTTP-filter phase to date — the PLAN-writer decides AFTER resolving the §6.2 crypto-API question.

### 6.2 Empirical verification at state-2 PLAN-write (the ratified verify-at-PLAN-write discipline)

Per the phase-10→21 verify-at-PLAN-write process improvement: the state-2 PLAN-writer empirically verifies the upstream wire shapes BEFORE locking PLAN lock-ins, by running `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) under Docker against the §3 D8.1 canonical bootstrap on an H1 listener — **this verification RUNS LOCALLY on macOS Docker** (jwt_authn has no virtiofs/inotify dependency; the §6.2 of phases 18-21's initial-load methodology applies). Verify:

1. **The crypto-API path (do FIRST — it gates the split decision):** confirm whether the pinned `aws-lc-rs` exposes `rsa::PublicKeyComponents { n, e }.verify(...)` (the clean direct-from-modulus/exponent path) or whether DER-assembly of the RSA public key is needed. This determines the `envoy-jwt` LoC and the §6.1 split call. (This is a code/dependency probe, not a Docker probe.)
2. **The 401 failure bodies (per class):** drive missing-token / tampered-signature / expired / wrong-issuer / wrong-audience requests; hex-dump the exact 401 response body bytes per failure class. **Do NOT assume the projections** (`Jwt is missing` / `Jwt verification fails` / `Jwt is expired` / …) — the phase-10 RBAC-body-off-by-one-byte experience makes this load-bearing. Record each byte-exact body + the SPEC §2.2 lock-in.
3. **The `WWW-Authenticate` header:** record the exact value Envoy emits on a jwt_authn 401 (§2.3); decide value-exact vs. allow-list.
4. **The stat namespace:** scrape `/stats` post-allow + post-deny; record the exact `jwt_authn` stat names + their rooting (HCM-prefixed vs. top-level vs. per-provider) (§2.1).
5. **The on-success forwarding disposition:** confirm whether Envoy (with the default `forward`) strips or forwards the `Authorization` header upstream on a verified request, and lock the envoy-rust default to match. (Determines the D4 success-path behavior + any upstream-request assertion in the fixture/backstop.)
6. **The no-matching-rule disposition:** confirm what Envoy does for a request matching NO `rules[]` entry (allow vs. deny) and lock envoy-rust to match.
7. **The canonical test keypair/JWKS choice (§3 D8.1 signpost):** decide whether to reuse Envoy's well-known Istio test JWKS/issuer (so the committed tokens validate identically on both proxies) — the only hard requirement is identical JWKS + identical tokens on both sides.

Each finding lands as a PLAN lock-in. **If any of items 2-6 differs materially from the SPEC projection, the reconciliation lands as inline ADR-0056 at the state-2 PLAN-write commit** (the phase-10 ADR-0034 / phase-21 ADR-0054 inline-at-PLAN-write precedent). The next-available ADR after this brainstorm's ADR-0055 is **ADR-0056**.

### 6.3 The 06.x stats convention + 07.x BEHAVIOR_CONTRACT cadence

StatsRegistry registration at `build_from_config`; per-filter-instance Counter ownership; namespace §6.2-verified to upstream parity. Contract extensions (stat rows + 401-body rows + `WWW-Authenticate`) land at the task where each is first empirically exercised (the 06.x → 21 cadence), NOT at PLAN-write or SPEC time.

### 6.4 In-process backstop header/body assertion (heeds the phase-10 M1 lesson)

D8.3 SHOULD assert the `WWW-Authenticate` header presence + the byte-exact 401 bodies on the 401 probes (and the 200 body on the allow probe), OR explicitly disclose any omission in PROGRESS. Recommended: assert.

### 6.5 State-4 evidence discipline + isolated-crate build + deny check

Per the 05.3 → 21 chain: per-gate quoted evidence in PROGRESS at state-4 (real CI run URL + HEAD SHA + timestamp + per-gate output for all 5 stable-toolchain gates + each Docker-gated fixture + the h2spec gate + every fuzz target's iteration count). **Phase-22-specific:** (a) the **4 standalone-crate builds** (`project_isolated_crate_build_blindspot`) MUST add `cargo build -p envoy-jwt` (and `-p envoy-filter`) — feature unification can hide a missing per-crate feature on the new crate; (b) `cargo deny check` explicitly confirms the now-direct `aws-lc-rs` dep passes the license/advisory policy; (c) pre-build `tests/helpers/*` and never run the Docker suite concurrently with cargo builds (`project_flaky_access_log_fixture_0012`); (d) the new `envoy-jwt` fuzz target runs its short-budget CI iteration. Per `project_state3_arc_skips_clippy`, the state-3 per-task verification runs `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK (clippy is otherwise first seen at the state-4 gate).

### 6.6 PROGRESS.md skeleton + Task 1 preamble land alongside PLAN.md at state-2; subagent-driven execution at state-3

Per the 06.2 → 21 cadence: state-2 PLAN-write lands `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble in one standalone pre-Task-1 commit. State-3 executes via `superpowers:subagent-driven-development` (the `feedback_execution_style` default), SERIAL dispatch (`feedback_serial_subagent_dispatch` — never parallel), TDD per task, two-stage (spec-then-quality) review on the substantive tasks (the `envoy-jwt` crypto crate D1 + the filter D4 are the substantive review centerpieces), one code commit + one PROGRESS commit per task.

---

## 7. ADR posture

**ADR-0055 lands at THIS brainstorm commit** (the scoping + crypto-foundations reinterpretation — see §0/§2.4/§5.2). The DECISIONS.md ledger head moves from ADR-0054 to **ADR-0055**; the next-available number is **ADR-0056**.

Reserved conditional ADRs for state-2 / state-3:

- **ADR-0056 — §6.2 empirical-verification reconciliation.** Lands at the state-2 PLAN-write commit if any of the §6.2 items 2-6 (401 body bytes / `WWW-Authenticate` / stat namespace / on-success forwarding / no-matching-rule disposition) differs materially from this SPEC's projection. **Recommended posture: verify all at PLAN-write; land ADR-0056 inline if any diverges** (the 401-body verification is the most likely trigger, per the phase-10 precedent).
- **ADR-0057 — §6.1 split.** Lands only if the PLAN materializes over the §6.1 gate and phase 22 splits into 22.1/22.2 per §6.1. **Recommended posture: single-phase; split reserved.**

At most one conditional ADR lands per commit (D-3.5 sequential numbering); if multiple fire, they take consecutive numbers.

---

## 8. State-machine signposts for the phase-22 state-2 session

- **Lifecycle state at session start:** State 2 (SPEC.md exists; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `docs/envoy-rust/phases/22-http-filter-jwt-authn/PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble (standalone pre-Task-1 commit per the 06.x → 21 cadence).
- **Empirical verification at state 2 (per §6.2):** resolve the crypto-API path FIRST (gates the split call); then verify the 401 bodies / `WWW-Authenticate` / stat namespace / forwarding / no-rule disposition against `envoyproxy/envoy:v1.33.0` (LOCAL Docker); land inline ADR-0056 if any diverges.
- **Split-gate evaluation:** §6.1. **Recommended: single-phase; split (22.1 crypto+schema / 22.2 filter+fixture) reserved (ADR-0057)** — decide after the §6.2 crypto-API resolution.
- **PLAN-time SPEC corrections:** the PLAN-writer reads this SPEC against HEAD `<state-1-commit-SHA>` and flags mechanical drift (the exact `HttpFilterTypedConfig` insertion site, the exact `RouteMatch` shape + its `matches` signature, the `FilterRequest`/`FilterResponse` field shapes, the `Decision` variants, whether a `DataSource`/`inline_string` type already exists, the exact `aws-lc-rs` RSA verification API in the pinned version) — corrections land in the PROGRESS Task 1 preamble per the 06.2 → 21 "N PLAN-write SPEC corrections" pattern.

---

## 9. Commit message format (for state 6 of the phase-22 lifecycle)

```
phase 22: envoy.filters.http.jwt_authn (RS256, inline JWKS, single-provider rules) + envoy-jwt crate + fixture 0030 [ADR-0055, ADR-00NN…]

<1-3 sentence summary>

Differential surface: fixture 0030-http-filter-jwt-authn (H1); all 30 Docker-gated fixtures (0001-0030) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held (no H2 framing change); the new envoy-jwt fuzz target clean on its short-budget CI run.
```

The bracketed ADR list carries ADR-0055 (this brainstorm) + any §6.2/§split ADRs that fired. If phase 22 splits at state-2 into 22.1 + 22.2, the closing-sub-phase commit carries `[parent 22 done]` per the 07.2/08.2/12.2/13.2/14.2 closing-sub-phase precedent.

---

## 10. State-machine commit (this commit — phase-22 state-1 brainstorm close-out)

This SPEC is the state-1 output. The state-1 brainstorm commit (the state-0/1-collapsing cadence of phases 12-21) touches exactly four docs files:

- **CREATE** `docs/envoy-rust/phases/22-http-filter-jwt-authn/SPEC.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — adds a new row beneath the "HTTP filters family" §9 heading, after the existing phase-11 row, `status: planned` (invariant 4.1.2 — a new row enters `planned`; no existing row flips; row 22 becomes `in-progress` only when the NEXT session's state-2 PLAN-write points STATE at it per invariant 4.1.3).
- **MODIFY** `docs/envoy-rust/DECISIONS.md` — append **ADR-0055** (the scoping + `aws-lc-rs`-for-JWT foundations reinterpretation + the new `envoy-jwt` crate + the minimum-viable scope + the alternatives-rejected analysis).
- **MODIFY** `docs/envoy-rust/STATE.md` — advance the Active-phase pointer from "AWAITING NEXT PLANNING" to `id: 22` / `slug: 22-http-filter-jwt-authn` / `directory: docs/envoy-rust/phases/22-http-filter-jwt-authn/` / state-1-complete / state-2-next; demote the prior "AWAITING NEXT PLANNING" block to `_Historical_`; rewrite `## Next expected skill` to the state-2 PLAN-write arc; append a `### Phase-22 state-1 brainstorm` Notes subsection (family-pick + filter-pick rationale + alternatives + scoring); update `## Last commit` + `## Last updated`. Preserve all prior Notes subsections verbatim per D-3.5 + D-3.4.

No code / fixture / Cargo / BEHAVIOR_CONTRACT change; no `unsafe`. The DECISIONS.md ledger head moves to **ADR-0055**. ADR-0014 remains in force; ADR-0028 remains open. ENVOY_TARGET.md + rust-toolchain.toml untouched. The brainstorm commit is docs-only → the CI run at this push is vacuous-green. Per `BOOTSTRAP_PROMPT.md` §5.1 (one state per session) this brainstorm session EXITS after this commit; the NEXT session writes `PLAN.md` (state 2 — `superpowers:writing-plans`, with the §6.2 empirical verification running LOCALLY).

**Commit message:**

```
phase 22: state-1 brainstorm — http-filter-jwt-authn SPEC.md (HTTP-filter-family fourth phase; first crypto-in-a-filter; new envoy-jwt crate) [ADR-0055]
```

**Predecessor:** `a63d5a66d` — phase-21 state-6 close-out. **Origin/main:** `a63d5a66d` (local + origin in sync as of this commit's prologue).

---

*End of SPEC. Phase 22 state-1 lifecycle complete on landing. The next session enters state 2 — writes PLAN.md per `superpowers:writing-plans`, resolves the `aws-lc-rs` crypto-API path (the split-gate swing factor), performs the §6.2 empirical verification LOCALLY (401 bodies / `WWW-Authenticate` / stat namespace / forwarding / no-rule disposition), and evaluates the §6.1 split gate.*
