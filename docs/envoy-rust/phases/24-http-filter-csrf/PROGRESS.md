# Phase 24 (`24-http-filter-csrf`) — PROGRESS

> Running execution log. The state-2 PLAN-write landed this skeleton + the Task-1 preamble (the §6.2 empirical-verification transcript + the PLAN-time SPEC corrections) in one standalone pre-Task-1 commit (the 06.2→23 cadence). State-3 execution appends one entry per completed task (`superpowers:subagent-driven-development`, SERIAL dispatch).

## Task ledger

| Task | Deliverable | Status |
|---|---|---|
| 1 | D1 — `CsrfPolicy` + `RuntimeFractionalPercent` schema + `PerFilterConfig::Csrf` (per-route) | DONE |
| 2 | D2+D4 — `CsrfFilter` runtime (chain-base/route-replace, scheme-stripped origin, 403) + 3 stats + BEHAVIOR_CONTRACT | OPEN |
| 3 | D3 — `HttpFilterTypedConfig::Csrf` + `HttpFilterInstance::Csrf` wiring + `validate_csrf_config` | OPEN |
| 4 | D6.1 — fixture `0032-http-filter-csrf` + Docker wrapper + `Http1Method::Post` | OPEN |
| 5 | D6.2 — `parse_bootstrap` fuzz seed (no new target) | OPEN |
| 6 | D6.3 — in-process backstop | OPEN |
| 7 | State-4 verification + STATE advance | OPEN |

---

## Task 1 preamble — §6.2 empirical verification + PLAN-time SPEC corrections

### §6.2 empirical verification (LOCAL Docker, `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd…`, 2026-06-11)

Ran upstream Envoy v1.33.0 locally under Docker 28.0.4 (CSRF has no virtiofs/inotify dependency — the phase-22/23 §6.2-local methodology). The canonical bootstrap: an H1 listener + HCM with a `csrf`+`router` filter chain + a route carrying a `CsrfPolicy` via `typed_per_filter_config`, proxying to a real all-method-200 backend (a Python sidecar container on a shared Docker user-defined network, addressed by container name — the host.docker.internal IPv6 path is unreliable on Docker Desktop macOS; this satisfies the ADR-0058 L6 real-upstream constraint). **The probe transcript below is the source of the PLAN "LOCKED-IN findings (L1–L8)" and ADR-0061.**

**Item 1 — config shape (DIVERGENCE → ADR-0061 L1, gates D1).** Envoy uses ONE proto message `type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy` at BOTH the filter-chain `http_filters[].typed_config` AND the per-route `typed_per_filter_config[envoy.filters.http.csrf]`. **`filter_enabled` is REQUIRED** at both:
- chain entry near-empty (no `filter_enabled`) → `--mode validate` FAILED: `Proto constraint validation failed (CsrfPolicyValidationError.FilterEnabled: value is required)`.
- per-route policy with no `filter_enabled` (only `additional_origins`) → same `CsrfPolicyValidationError.FilterEnabled: value is required`.
- chain + route both with `filter_enabled: { default_value: { numerator: 100, denominator: HUNDRED } }` → `configuration '/etc/envoy/envoy.yaml' OK`.
So the SPEC §3 D1 projection (near-empty `CsrfConfig {}` + `filter_enabled: Option<…>`) is WRONG. **Schema lock:** `CsrfPolicy { filter_enabled: RuntimeFractionalPercent (required, NOT Option), additional_origins: Vec<StringMatcher> (default) }`; `RuntimeFractionalPercent { default_value: FractionalPercent (required), runtime_key: Option<String> }`. Both `HttpFilterTypedConfig::Csrf(CsrfPolicy)` and `PerFilterConfig::Csrf(CsrfPolicy)` carry the SAME `CsrfPolicy`.

**Item 2 — modify-method set (CONFIRMED → L2).** With an `Origin: http://evil.example.com` cross-origin source against `Host: localhost:10000` (chain `filter_enabled` 100%, no additional_origins):
```
POST   evil -> 403   PUT  evil -> 403   DELETE evil -> 403   PATCH evil -> 403
GET    evil -> 200   HEAD evil -> 200   OPTIONS evil -> 200   TRACE evil -> 200
```
So `{POST,PUT,DELETE,PATCH}` are guarded; `{GET,HEAD,OPTIONS,TRACE}` pass through unconditionally. A custom method `FOOBAR` → `400` (Envoy's HTTP layer rejects the unknown method before CSRF — not observable), so the modify set is the explicit hardcoded `{POST,PUT,DELETE,PATCH}`.

**Item 3 — origin computation (DIVERGENCE → ADR-0061 L3).** Origins are reduced to **scheme-stripped `host[:port]`** (`Url::hostAndPort()`), NOT `scheme://host:port`. Probing `Origin: http://additional.example.com` against four `additional_origins` matcher shapes:
```
matcher = exact: "additional.example.com"            additional-origin POST -> 200
matcher = exact: "http://additional.example.com:80"  additional-origin POST -> 403
matcher = exact: "additional.example.com:80"          additional-origin POST -> 403
matcher = suffix: "additional.example.com"           additional-origin POST -> 200
```
So the source from `Origin: http://additional.example.com` is `additional.example.com` (host only — no `:80` synthesized). The matcher matches against this scheme-stripped value. Origin-source probes (POST, `Host: localhost:10000`, route `additional_origins: [exact: additional.example.com]`):
```
Origin: http://localhost:10000            -> 200 (same-origin, source==target)
Origin: http://additional.example.com     -> 200 (additional allowed)
(no Origin, no Referer)                    -> 403 (missing source)
Referer: http://localhost:10000/page      -> 200 (Referer fallback, same)
Referer: http://evil.example.com/page      -> 403 (Referer fallback, evil)
Origin: http://localhost:10000 + Referer: http://evil…  -> 200 (Origin precedence)
```
**Conclusion:** source = `host_and_port(Origin)`, fallback `host_and_port(Referer)`; `Origin` takes precedence when both present; target = `host_and_port(Host`/`:authority)` (a bare `Host: localhost:10000` is used verbatim — not a URL). Valid iff source non-empty AND (source == target OR an `additional_origins` matcher matches source). `host_and_port(v)`: if `v` has `"://"`, the authority between `"://"` and the next `/`; else `v`.

**Item 4 — the 403 local reply (CONFIRMED byte-exact → L4).** A guarded cross-origin POST:
```
HTTP/1.1 403 Forbidden
content-length: 14
content-type: text/plain
date: Thu, 11 Jun 2026 22:24:45 GMT
server: envoy
```
Body `xxd`: `00000000: 496e 7661 6c69 6420 6f72 6967 696e        Invalid origin` — exactly **14 bytes, NO trailing newline**. The existing H1 `decorate_filter_synth_response` (`hcm.rs:1454`) auto-adds `content-type: text/plain` for a non-empty body + stamps `content-length`/`server`/`date`, so `FilterResponse { status: 403, reason: Some("Forbidden"), headers: vec![], body: b"Invalid origin" }` reproduces this byte-for-byte (the rbac `b"RBAC: access denied"` precedent verbatim).

**Item 5 — stat namespace + semantics (CONFIRMED → L5).** `/stats` showed `http.ingress_http.csrf.{request_valid, request_invalid, missing_source_origin}` (HCM-prefixed, like rbac/fault/jwt/cors). A controlled stats-reset sequence (chain 100%, route override `additional_origins: [exact: additional.example.com]`):
```
baseline                missing:0 invalid:0 valid:0
1 POST same-origin 200  missing:0 invalid:0 valid:1
2 POST evil        403  missing:0 invalid:1 valid:1
3 POST additional  200  missing:0 invalid:1 valid:2
4 GET  evil        200  missing:0 invalid:1 valid:2   (safe method — no tick)
5 POST no-source   403  missing:1 invalid:1 valid:2
```
**MUTUALLY EXCLUSIVE, one tick per evaluated modify request:** valid → `request_valid` only; present-but-disallowed → `request_invalid` only; **no-source → `missing_source_origin` only (it does NOT also tick `request_invalid`)**; safe methods → no stat.

**Item 6 — chain-base / route-replace + filter_enabled disposition (DIVERGENCE → ADR-0061 L6).** Two-route probe (`/guarded` with a route override, `/plain` with NO override; chain `filter_enabled` 100%):
```
/guarded (override)  POST evil -> 403
/plain   (no override) POST evil -> 403   ← chain policy STILL guards a no-override route
/plain   (no override) POST same -> 200
```
`filter_enabled` deterministic + route-replace 2×2 matrix (POST evil):
```
chain=100, no route override   -> 403  (chain enforces)
chain=0,   no route override   -> 200  (deterministic-0% passthrough)
chain=100, route override=0    -> 200  (route REPLACES chain → 0% passthrough)
chain=0,   route override=100  -> 403  (route REPLACES chain → 100% enforce)
```
**Conclusion:** the chain-level `CsrfPolicy` is an always-applied BASE; a per-route `CsrfPolicy` REPLACES it wholesale (not a field-merge). Effective policy = route `CsrfPolicy` if present, else chain `CsrfPolicy`; its `filter_enabled` gates enforce-vs-passthrough. This DIVERGES from the cors "inert when no route config" pattern → `CsrfFilter` compiles the chain policy as a base and falls back to it in `apply_route_config`. **envoy-rust disposition (ADR-0049 all-fatal):** reject non-deterministic `default_value` (`numerator ∉ {0, denominator.value()}`); reject a present `runtime_key`; reject `shadow_enabled` via `deny_unknown_fields` — at both chain + route level.

**Item 7 — policy-for-absent-filter (CONFIRMED reuse → L7).** Not re-probed against Envoy (the existing generic `PerRouteConfigForAbsentFilter` validator at `bootstrap.rs:2696` iterates `route.typed_per_filter_config.keys()` and already covers any filter name, including `csrf`). The ADR-0058 L7 divergence (Envoy accepts-and-ignores; envoy-rust stricter-rejects) applies to csrf verbatim — reused, no new code. The no-override case falls back to the chain policy (L6).

**Item 8 — fixture topology (CONFIRMED reuse → L8).** The verification required a real all-method-200 backend to observe the 200/403 split; a valid CSRF modify request must reach an upstream to yield a 200. Fixture 0032 uses the `http1-echo-server` real upstream (the ADR-0058 L6 constraint), NOT `direct_response`.

→ **ADR-0061 FIRES** at this PLAN-write commit for the three material divergences (L1 config shape, L3 scheme-stripped origin, L6 chain-base/route-replace). **ADR-0062 (split) does NOT fire** (single-phase, §6.1 — 7 tasks, ~1000–1350 LoC).

### PLAN-time SPEC corrections (read-only recon at HEAD `bb58319ea`)

Eight mechanical corrections, recorded as SC1–SC8 in `PLAN.md`. The load-bearing ones:

- **SC1** — `PerFilterConfig` is DERIVE-based (not hand-rolled); adding `Csrf(CsrfPolicy)` is a one-variant change but breaks two exhaustive consumers (`cors.rs:118` match, `bootstrap.rs:12908` irrefutable `let`) — fixed in Task 1, gated on `cargo build --workspace`.
- **SC2** — `HttpFilterTypedConfig::Csrf` breaks the `instance.rs` build match + the `bootstrap.rs:2803` validator match; because the build arm needs `CsrfFilter` to exist, the chain variant lands in Task 3 (wiring), not Task 1.
- **SC3** — `FractionalPercent` (+ `selects_deterministic()`) exists at `bootstrap.rs:649`; `RuntimeFractionalPercent` is NEW (Task 1, wrapping `default_value: FractionalPercent` + `runtime_key: Option<String>`).
- **SC5** — `header_ci` duplication reaches N=3 (jwt_authn/cors/csrf); duplicate the 5-line helper, do not extract (the standing deferred M-track item).
- **SC7** — CSRF is decode-only, but `instance.rs`'s `encode_headers` match is exhaustive → `CsrfFilter::encode_headers` returns the trivial `Continue` (the rbac no-op-encode precedent).

(Full SC1–SC8 in `PLAN.md` → "PLAN-time SPEC corrections".)

---

## Execution log

### Task 1 — D1 schema + `PerFilterConfig::Csrf` — DONE (code commit `f97440f9`)

`superpowers:subagent-driven-development` (SERIAL): implementer subagent (TDD) + two-stage review (spec ✅ / code-quality). Landed in `crates/envoy-config`:
- **`RuntimeFractionalPercent { default_value: FractionalPercent, runtime_key: Option<String> }`** (NEW) + **`CsrfPolicy { filter_enabled: RuntimeFractionalPercent (REQUIRED), additional_origins: Vec<StringMatcher> (default) }`** in `bootstrap.rs`, both `#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)] #[serde(deny_unknown_fields)]` (ADR-0061 L1). Reuses existing `FractionalPercent::selects_deterministic()` + `StringMatcher`.
- **`PerFilterConfig::Csrf(CsrfPolicy)`** per-route variant (the SECOND `PerFilterConfig` consumer after `Cors`) with the `@type` rename `...csrf.v3.CsrfPolicy`.
- **SC1 exhaustive-consumer fixes:** `crates/envoy-filter/src/cors.rs` `apply_route_config` match → `.and_then(.. { Cors(p)=>Some, _=>None })`; the irrefutable `let PerFilterConfig::Cors(p) = pfc;` test in `bootstrap.rs` → refutable `let .. else { panic!() }`.
- Re-exported `CsrfPolicy` + `RuntimeFractionalPercent` from `lib.rs`.
- **Tests (5, all pass):** `csrf_policy_parses_filter_enabled_and_additional_origins`, `csrf_policy_requires_filter_enabled`, `csrf_policy_rejects_shadow_enabled`, `csrf_policy_parses_runtime_key`, `route_parses_typed_per_filter_config_csrf`.
- **Gates green:** `cargo test -p envoy-config` (full crate, no cors regression), `cargo build --workspace`, `cargo build -p envoy-config` (blind-spot), `cargo fmt --all -- --check` (clean after review-driven amend).
- **Review:** spec ✅ (exact scope, no over-build — chain-level `HttpFilterTypedConfig::Csrf` correctly deferred to Task 3). Code-quality flagged one Important (commit not fmt-clean) + Minor (no `runtime_key` round-trip test) → both fixed via commit amend.

_(state-3 appends one entry per completed task here)_
