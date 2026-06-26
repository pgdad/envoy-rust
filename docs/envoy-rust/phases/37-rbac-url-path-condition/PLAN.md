# Phase 37 — `37-rbac-url-path-condition` — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.
> TDD per `superpowers:test-driven-development` on EVERY task — test first, watch it fail, implement minimal, watch it pass, commit.
> Read this top-to-bottom with zero prior context (D-3.4). The SPEC is `SPEC.md` (sibling); the empirical
> ground truth is **ADR-0090** in `docs/envoy-rust/DECISIONS.md` (read §A–§D before Task 1).

**Goal:** Add the `url_path` condition type to the RBAC filter's `Permission` AND `Principal` enums on the
existing phase-10 `envoy.filters.http.rbac` filter — matching the request path with the `?query` stripped —
behaviorally equivalent to upstream Envoy v1.33.0 (the query-strip semantic is the load-bearing differential).

**Architecture:** A thin `PathMatcher { path: StringMatcher }` config struct + a `UrlPath` variant on both RBAC
enums. The runtime evaluator strips everything from the first `?` of `FilterRequest.path` and applies the 04.2
`StringMatcher`. A `safe_regex` `url_path` value compiles at RBAC lowering (the phase-36 fallible-lowering path,
UNCHANGED) so a malformed pattern is boot-fatal, not a first-request panic. NO new `HttpFilterInstance` variant,
NO new infrastructure, NO metadata store, NO producer chain, NO new `ConfigError` variant, NO new crate/dependency.

**Tech Stack:** Rust (workspace), `serde`/`serde_yaml` (config), the `regex` crate (ADR-0021, SafeRegex engine),
`testcontainers` (differential harness vs `envoyproxy/envoy:v1.33.0`), `cargo fuzz` (`parse_bootstrap` seed only).

**§6.1 split GATE result:** NOT split. ~7 TDD tasks / ~350–550 LoC net (ADR-0090 §Consequences) — under the
~25-task / ~1500-LoC gate. **ADR-0091 stays UNFIRED.**

**Empirical lock (ADR-0090, do NOT re-derive):**
- **§B (the key risk):** Envoy matches `url_path` against the request-target with **everything from the first `?`
  removed, and NOTHING else** — no percent-decode, no `..`/`//`/trailing-slash normalization, no case-fold. EXACTLY
  the MVP projection. → `strip_query(p) = p.split('?').next().unwrap_or(p)`.
- **§A:** `url_path: { path: { <StringMatcher> } }` accepted under BOTH `permissions[]` AND `principals[]`;
  `/config_dump` round-trips it verbatim; `PathMatcher`'s only rule is a REQUIRED `path`.
- **§C:** `safe_regex` is RE2 FULL-match against the query-stripped path; LOCK anchored `^…$` patterns (M36-1).
- **§D:** empty `PathMatcher` / empty `StringMatcher` / unknown sub-key / malformed regex are **boot-fatal on BOTH
  proxies** — all map to existing error paths (serde-parse error, or `FilterError::InvalidConfig` at lowering).
  **NO new `ConfigError` variant.**
- **R1 (carry-forward M37-1):** `/allowed#frag` → Envoy **400** at the H1 codec (never reaches `url_path`) → OUT of
  the fixture. **R2:** §D is envoy-rust backstop boot-fatal tests + ADR-0090's record of Envoy's matching reject
  (the harness does not differentially probe boot-failure configs).

---

## File map (what each task creates/modifies)

| File | Responsibility | Tasks |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | `PathMatcher` struct; `Permission::UrlPath`/`Principal::UrlPath` variants + visitors + KEYS + validator arms | 1,2,3 |
| `crates/envoy-config/src/lib.rs` | export `PathMatcher` | 1 |
| `crates/envoy-filter/src/rbac.rs` | `RuntimePermission::UrlPath`/`RuntimePrincipal::UrlPath` + eval arms + lower arms + `strip_query` helper + backstop tests | 2,3,4,5 |
| `tests/fixtures/0045-http-rbac-url-path/` | `envoy.yaml`/`envoy-rust.yaml`/`expectations.yaml`/`README.md` | 6 |
| `tests/differential/tests/rbac_url_path.rs` | NEW per-fixture test wrapper (there is NO manifest/glob — each fixture has a hand-written `#[tokio::test]` calling `differential::run_fixture`) | 6 |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_url_path.yaml` + `crates/envoy-config/fuzz/.gitignore` | `parse_bootstrap` seed + un-ignore line | 7 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | RBAC `url_path` subsection (query-strip semantic) | 7 |

---

## Task 1: `PathMatcher` config struct

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (add struct near `MetadataMatcher`, ~line 1336)
- Modify: `crates/envoy-config/src/lib.rs` (export, the `pub use bootstrap::{…}` list ~line 14-37)
- Test: inline `#[cfg(test)]` in `crates/envoy-config/src/bootstrap.rs`

ADR-0090 §D: a thin DERIVED struct suffices — the required `path` field + `deny_unknown_fields` + the inner
`StringMatcher` visitor's own "missing mode key" error cover §D cases 1–3. NO hand-rolled visitor.

- [ ] **Step 1: Write the failing tests** (add to the `bootstrap.rs` tests module)

```rust
#[test]
fn path_matcher_parses_exact_and_round_trips() {
    let pm: crate::PathMatcher = serde_yaml::from_str("path: { exact: \"/allowed\" }").unwrap();
    assert_eq!(
        pm.path,
        StringMatcher { mode: StringMatcherMode::Exact("/allowed".into()), ignore_case: false }
    );
    // round-trips through serde_yaml
    let s = serde_yaml::to_string(&pm).unwrap();
    let pm2: crate::PathMatcher = serde_yaml::from_str(&s).unwrap();
    assert_eq!(pm, pm2);
}

#[test]
fn path_matcher_empty_is_missing_path_error() {
    // §D case 1: `url_path: {}` → empty PathMatcher → missing `path`.
    let err = serde_yaml::from_str::<crate::PathMatcher>("{}").unwrap_err().to_string();
    assert!(err.contains("path"), "want missing-path error, got: {err}");
}

#[test]
fn path_matcher_empty_string_matcher_is_missing_mode_error() {
    // §D case 2: `url_path: { path: {} }` → StringMatcher with no mode key.
    let err = serde_yaml::from_str::<crate::PathMatcher>("path: {}").unwrap_err().to_string();
    assert!(err.contains("mode key"), "want missing-mode error, got: {err}");
}

#[test]
fn path_matcher_unknown_subkey_is_denied() {
    // §D case 3: `url_path: { foo: bar }` → deny_unknown_fields.
    let err = serde_yaml::from_str::<crate::PathMatcher>("foo: bar").unwrap_err().to_string();
    assert!(err.contains("foo") || err.contains("unknown"), "want unknown-field error, got: {err}");
}
```

- [ ] **Step 2: Run, verify they fail to compile** — `cargo test -p envoy-config path_matcher` → FAIL (`crate::PathMatcher` not found).

- [ ] **Step 3: Add the struct** (`bootstrap.rs`, immediately after `MetadataPathSegment`, ~line 1342)

```rust
/// Envoy `type.matcher.v3.PathMatcher` (phase 37). The only in-scope rule is
/// `path` (a `StringMatcher`); RBAC `url_path` matches the request path with the
/// `?query` stripped (ADR-0090 §B). A thin derived struct — the required `path`
/// field + `deny_unknown_fields` + the inner StringMatcher's "missing mode key"
/// error make an empty/path-less/unknown-subkey `PathMatcher` boot-fatal,
/// matching Envoy (ADR-0090 §D); no hand-rolled visitor needed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathMatcher {
    pub path: StringMatcher,
}
```

- [ ] **Step 4: Export it** — add `PathMatcher,` to the `pub use bootstrap::{…}` list in `lib.rs` (alphabetical: after `PathConfigSource`, before `PerFilterConfig`).

- [ ] **Step 5: Run tests** — `cargo test -p envoy-config path_matcher` → PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 37: PathMatcher config struct (RBAC url_path) [ADR-0090]"
```

---

## Task 2: `Permission::UrlPath` end-to-end (config + runtime + eval + lower + query-strip)

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` — `Permission` enum (~1409), its `Deserialize` visitor + `KEYS` (~1432-1464), `validate_permission_tree` (~3953)
- Modify: `crates/envoy-filter/src/rbac.rs` — `RuntimePermission` (~27), `eval_permission` (~74), `lower_permission` (~247), new `strip_query` helper
- Test: inline tests in both files

> This task spans both crates because adding `Permission::UrlPath` makes the exhaustive matches in
> `validate_permission_tree` (envoy-config) and `lower_permission` (envoy-filter) non-exhaustive — both must be
> updated for the workspace to compile. Do the whole task before committing so every commit is workspace-green.

- [ ] **Step 1: Write the failing config-parse test** (`bootstrap.rs` tests)

```rust
#[test]
fn permission_parses_url_path_and_json_round_trips() {
    // Parse from YAML (the config-load surface).
    let p: crate::Permission =
        serde_yaml::from_str("url_path: { path: { exact: \"/allowed\" } }").unwrap();
    assert!(matches!(&p, crate::Permission::UrlPath(pm)
        if pm.path == StringMatcher { mode: StringMatcherMode::Exact("/allowed".into()), ignore_case: false }));
    // Round-trip through JSON — NOT YAML. `serde_yaml` 0.9 serializes these
    // externally-tagged hand-rolled enums as `!url_path` TAG syntax, which the
    // hand-rolled Deserialize visitor does NOT accept on reparse (it expects a
    // map). /config_dump is a JSON surface anyway. Precedent + rationale:
    // `rbac_metadata_permission_json_round_trips` at bootstrap.rs:~12214.
    let s = serde_json::to_string(&p).unwrap();
    assert!(s.contains("url_path"));
    let p2: crate::Permission = serde_json::from_str(&s).unwrap();
    assert_eq!(p, p2);
}
```

- [ ] **Step 2: Write the failing filter eval test** (`rbac.rs` tests; reuse the `req_with` helper and add a `req_with_path`)

```rust
fn req_with_path(path: &str) -> FilterRequest {
    FilterRequest { method: "GET".into(), path: path.into(), headers: vec![],
        body: None, dynamic_metadata: std::collections::BTreeMap::new() }
}

#[test]
fn url_path_permission_exact_matches_and_strips_query() {
    let sm = StringMatcher { mode: StringMatcherMode::Exact("/allowed".into()), ignore_case: false };
    let p = RuntimePermission::UrlPath(sm);
    assert!(eval_permission(&p, &req_with_path("/allowed")));        // match
    assert!(eval_permission(&p, &req_with_path("/allowed?x=1")));    // query stripped (ADR-0090 §B)
    assert!(eval_permission(&p, &req_with_path("/allowed?")));       // empty query stripped
    assert!(!eval_permission(&p, &req_with_path("/denied")));        // miss
    assert!(!eval_permission(&p, &req_with_path("/allowed/")));      // trailing slash significant
}
```

- [ ] **Step 3: Run, verify red** — `cargo test -p envoy-config permission_parses_url_path` and `cargo test -p envoy-filter url_path_permission` → FAIL (variant/missing types).

- [ ] **Step 4a: Add the config variant** — `Permission` enum (`bootstrap.rs:1421`, after `Metadata`):

```rust
    #[serde(rename = "url_path")]
    UrlPath(PathMatcher),
```
add `"url_path",` to the `KEYS` array (~1438), and the visitor arm (~1463, after the `metadata` arm):
```rust
                    "url_path" => Permission::UrlPath(map.next_value::<PathMatcher>()?),
```

- [ ] **Step 4b: Add the validator leaf arm** — `validate_permission_tree` (`bootstrap.rs`, after the `Metadata` arm ~3984), mirroring `Header(_) => Ok(())` (no semantic check; SafeRegex compiles at lowering per ADR-0090 §D):

```rust
        crate::Permission::UrlPath(_) => Ok(()),
```

- [ ] **Step 4c: Add the `strip_query` helper** (`rbac.rs`, free fn near `eval_permission`):

```rust
/// Phase 37: extract the path Envoy matches `url_path` against — the request
/// target with everything from the first `?` removed (ADR-0090 §B: query-strip
/// ONLY; no percent-decode / dot-segment / slash / case normalization). Envoy's
/// `#fragment` is rejected at the H1 codec (400) before it reaches here (R1/M37-1).
fn strip_query(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}
```

- [ ] **Step 4d: Add the runtime variant + eval arm** — `RuntimePermission` (`rbac.rs:43`, after `Metadata`):

```rust
    /// Phase 37: url_path condition. Holds the inner StringMatcher directly
    /// (the PathMatcher wrapper is trivial). Matches the query-stripped req.path.
    UrlPath(envoy_config::StringMatcher),
```
and in `eval_permission` (after the `Metadata` arm):
```rust
        RuntimePermission::UrlPath(sm) => sm.matches(strip_query(&req.path)),
```

- [ ] **Step 4e: Add the lower arm** — `lower_permission` (`rbac.rs`, after the `Metadata` arm ~281), reusing the phase-36 fallible compile:

```rust
        envoy_config::Permission::UrlPath(pm) => {
            let mut sm = pm.path.clone();
            sm.compile_safe_regex().map_err(|e| FilterError::InvalidConfig { message: e.to_string() })?;
            RuntimePermission::UrlPath(sm)
        }
```

- [ ] **Step 5: Run the workspace** — `cargo test -p envoy-config permission_parses_url_path && cargo test -p envoy-filter url_path_permission` → PASS; `cargo build --workspace` → clean.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-filter/src/rbac.rs
git commit -m "phase 37: Permission::UrlPath variant + query-stripped eval + fallible lowering [ADR-0090]"
```

---

## Task 3: `Principal::UrlPath` end-to-end (symmetric)

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` — `Principal` enum (~1491), visitor + `KEYS` (~1514-1538), `validate_principal_tree` (~4031)
- Modify: `crates/envoy-filter/src/rbac.rs` — `RuntimePrincipal` (~53), `eval_principal` (~101), `lower_principal` (~290)
- Test: inline tests in both files

- [ ] **Step 1: Write the failing tests**

```rust
// bootstrap.rs
#[test]
fn principal_parses_url_path() {
    let p: crate::Principal = serde_yaml::from_str("url_path: { path: { prefix: \"/api\" } }").unwrap();
    assert!(matches!(p, crate::Principal::UrlPath(_)));
}

// rbac.rs
#[test]
fn url_path_principal_matches_query_stripped() {
    let sm = StringMatcher { mode: StringMatcherMode::Exact("/allowed".into()), ignore_case: false };
    let p = RuntimePrincipal::UrlPath(sm);
    assert!(eval_principal(&p, &req_with_path("/allowed?x=1")));
    assert!(!eval_principal(&p, &req_with_path("/denied")));
}
```

- [ ] **Step 2: Run, verify red** — `cargo test -p envoy-config principal_parses_url_path && cargo test -p envoy-filter url_path_principal` → FAIL.

- [ ] **Step 3: Implement** — mirror Task 2 for `Principal`:
  - `Principal::UrlPath(PathMatcher)` variant + `#[serde(rename = "url_path")]`; `"url_path",` in `KEYS` (~1514); visitor arm `"url_path" => Principal::UrlPath(map.next_value::<PathMatcher>()?),` (~1538).
  - `validate_principal_tree`: `crate::Principal::UrlPath(_) => Ok(()),` (after the `Metadata` arm ~4034).
  - `RuntimePrincipal::UrlPath(envoy_config::StringMatcher)` variant; `eval_principal` arm `RuntimePrincipal::UrlPath(sm) => sm.matches(strip_query(&req.path)),`; `lower_principal` arm (clone + `compile_safe_regex()?` + `RuntimePrincipal::UrlPath(sm)`).

- [ ] **Step 4: Run** — both tests PASS; `cargo build --workspace` clean.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-filter/src/rbac.rs
git commit -m "phase 37: Principal::UrlPath variant (symmetric) [ADR-0090]"
```

---

## Task 4: backstop — StringMatcher modes, composition, DENY-inversion, anchored safe_regex

**Files:**
- Test: `crates/envoy-filter/src/rbac.rs` tests module

ADR-0090 §C: LOCK anchored `^…$` patterns (M36-1 — partial==full). These exercise the runtime through
`build_from_config` + `decode_headers` where useful (DENY-inversion needs the filter decision matrix).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn url_path_all_string_modes() {
    use StringMatcherMode::*;
    for (mode, path, want) in [
        (Prefix("/api".into()),    "/api/users", true),
        (Prefix("/api".into()),    "/v2/users",  false),
        (Suffix("/health".into()), "/svc/health", true),
        (Suffix("/health".into()), "/svc/ready",  false),
        (Contains("admin".into()), "/x/admin/y", true),
        (Contains("admin".into()), "/x/user/y",  false),
    ] {
        let p = RuntimePermission::UrlPath(StringMatcher { mode, ignore_case: false });
        assert_eq!(eval_permission(&p, &req_with_path(path)), want, "path={path}");
    }
}

#[test]
fn url_path_composes_and_inverts_under_deny() {
    // Build an action: DENY policy whose permission is `not_rule { url_path exact /allowed }`
    // and principal any:true. DENY+match(of not) inverts: /allowed → matched-by-inner →
    // not_rule false → policy no-match → DENY-action no-match → ALLOW (200);
    // /other → inner false → not_rule true → policy match → DENY → 403.
    use envoy_config::*;
    let url = Permission::UrlPath(PathMatcher {
        path: StringMatcher { mode: StringMatcherMode::Exact("/allowed".into()), ignore_case: false } });
    let cfg = RbacConfig { rules: Rules { action: Action::Deny, policies: [(
        "p0".to_string(),
        Policy { permissions: vec![Permission::NotRule(Box::new(url))],
                 principals: vec![Principal::Any(true)] })].into_iter().collect() } };
    let registry = std::sync::Arc::new(StatsRegistry::new());
    let mut f = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").unwrap();
    assert!(matches!(f.decode_headers(&mut req_with_path("/allowed")), Decision::Continue));
    assert!(matches!(f.decode_headers(&mut req_with_path("/other")), Decision::StopAndSend(_)));
}

#[test]
fn url_path_composes_in_and_or_rules() {
    // SPEC §2.1.5: url_path composes inside and_rules / or_rules.
    let url = |p: &str| RuntimePermission::UrlPath(
        StringMatcher { mode: StringMatcherMode::Prefix(p.into()), ignore_case: false });
    // and_rules: BOTH prefixes must match.
    let and = RuntimePermission::AndRules(vec![url("/api"), url("/api/v2")]);
    assert!(eval_permission(&and, &req_with_path("/api/v2/users")));
    assert!(!eval_permission(&and, &req_with_path("/api/v1/users")));
    // or_rules: EITHER prefix matches.
    let or = RuntimePermission::OrRules(vec![url("/api"), url("/admin")]);
    assert!(eval_permission(&or, &req_with_path("/admin/x")));
    assert!(!eval_permission(&or, &req_with_path("/public/x")));
}

#[test]
fn url_path_anchored_safe_regex_matches_without_panic() {
    // ADR-0090 §C: anchored ^/allowed/[0-9]+$ ; compiles at lowering, no first-request panic.
    use envoy_config::*;
    let sr = StringMatcher {
        mode: StringMatcherMode::SafeRegex(SafeRegex { regex: "^/allowed/[0-9]+$".into(), compiled: None }),
        ignore_case: false };
    let cfg = RbacConfig { rules: Rules { action: Action::Allow, policies: [(
        "p0".to_string(),
        Policy { permissions: vec![Permission::UrlPath(PathMatcher { path: sr })],
                 principals: vec![Principal::Any(true)] })].into_iter().collect() } };
    let registry = std::sync::Arc::new(StatsRegistry::new());
    let mut f = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").unwrap();
    assert!(matches!(f.decode_headers(&mut req_with_path("/allowed/42")), Decision::Continue));
    assert!(matches!(f.decode_headers(&mut req_with_path("/allowed/42?q=1")), Decision::Continue)); // query-strip
    assert!(matches!(f.decode_headers(&mut req_with_path("/allowed/xx")), Decision::StopAndSend(_)));
    assert!(matches!(f.decode_headers(&mut req_with_path("/allowed")), Decision::StopAndSend(_)));   // full-anchor
}
```

> **Note:** Verify the exact `RbacConfig`/`Rules`/`Policy` constructor shapes against `rbac.rs` existing tests
> (e.g. `build_from_config_allow_with_header_principal_creates_filter` ~line 595) and adjust field names if they
> differ (`policies` may be a `BTreeMap`).

- [ ] **Step 2: Run, verify red** then implement nothing new (behavior already exists from Tasks 2/3) — these are pure backstop tests. If a test fails on a real behavior gap, fix the runtime, not the test.

- [ ] **Step 3: Run** — `cargo test -p envoy-filter url_path` → all PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-filter/src/rbac.rs
git commit -m "phase 37: url_path backstop — modes, composition, DENY-inversion, anchored safe_regex [ADR-0090]"
```

---

## Task 5: backstop — config-validity boot-fatal (ADR-0090 §D)

**Files:**
- Test: `crates/envoy-config/src/bootstrap.rs` tests (parse-layer §D cases 1–3) + `crates/envoy-filter/src/rbac.rs` tests (lowering §D case 4)

Task 1 already covered the bare `PathMatcher` parse errors; this task asserts the SAME rejections THROUGH a full
RBAC `Permission`/policy context (the real boot path) + the malformed-regex lowering reject.

- [ ] **Step 1: Write the failing/guarding tests**

```rust
// bootstrap.rs — §D 1-3 through a Permission
#[test]
fn rbac_url_path_empty_and_unknown_are_boot_fatal() {
    assert!(serde_yaml::from_str::<crate::Permission>("url_path: {}").is_err());            // §D1
    assert!(serde_yaml::from_str::<crate::Permission>("url_path: { path: {} }").is_err());  // §D2
    assert!(serde_yaml::from_str::<crate::Permission>("url_path: { foo: bar }").is_err());  // §D3
}

// rbac.rs — §D 4: malformed safe_regex → build_from_config Err (boot-fatal, NOT first-request panic)
#[test]
fn url_path_malformed_safe_regex_is_build_error() {
    use envoy_config::*;
    let bad = StringMatcher {
        mode: StringMatcherMode::SafeRegex(SafeRegex { regex: "[".into(), compiled: None }),
        ignore_case: false };
    let cfg = RbacConfig { rules: Rules { action: Action::Allow, policies: [(
        "p0".to_string(),
        Policy { permissions: vec![Permission::UrlPath(PathMatcher { path: bad })],
                 principals: vec![Principal::Any(true)] })].into_iter().collect() } };
    let registry = std::sync::Arc::new(StatsRegistry::new());
    assert!(matches!(RbacFilter::build_from_config(&cfg, &registry, "ingress_http"),
                     Err(FilterError::InvalidConfig { .. })));
}
```

- [ ] **Step 2: Run** — `cargo test -p envoy-config rbac_url_path_empty && cargo test -p envoy-filter url_path_malformed` → these should PASS off Tasks 1–3 (pure guards). If red, the gap is real → fix the implementation.

- [ ] **Step 3: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-filter/src/rbac.rs
git commit -m "phase 37: url_path config-validity boot-fatal backstop (ADR-0090 §D) [ADR-0090]"
```

---

## Task 6: differential fixture `0045-http-rbac-url-path`

**Files:**
- Create: `tests/fixtures/0045-http-rbac-url-path/envoy.yaml`
- Create: `tests/fixtures/0045-http-rbac-url-path/envoy-rust.yaml`
- Create: `tests/fixtures/0045-http-rbac-url-path/expectations.yaml`
- Create: `tests/fixtures/0045-http-rbac-url-path/README.md`
- Create: `tests/differential/tests/rbac_url_path.rs` (the per-fixture `#[tokio::test]` wrapper — REQUIRED; there is no manifest/glob, so without this file fixture 0045 NEVER runs and acceptance gate (a) is vacuously green)

Template off `tests/fixtures/0043-http-rbac-dynamic-metadata/` (the `http1_probe_list` driver already supports
per-probe `path` — ADR-0090 §5; NO new comparator/driver/`extra_headers`). The chain is plain `[rbac, router]`
(NO `header_to_metadata` producer). Drop the route to a single `direct_response { status: 200, body: "ok\n" }`.

- [ ] **Step 1: Write `envoy.yaml`** (upstream reference) — copy `0043/envoy.yaml`, then: remove the
  `header_to_metadata` filter; replace the `metadata` permission with `url_path: { path: { exact: "/allowed" } }`;
  keep `action: ALLOW`, principal `any: true`, route `direct_response{status:200, body:{inline_string:"ok\n"}}`,
  `codec_type: HTTP1`, `clusters: []`, the admin block, and `generate_request_id` posture per the 0043 upstream side.

- [ ] **Step 2: Write `envoy-rust.yaml`** — the byte-identical HCM body modulo the fixture-0043 envoy-rust deltas
  (bind `127.0.0.1`, no admin block, NO `generate_request_id`, the `{{PORT}}` token).

- [ ] **Step 3: Write `expectations.yaml`** (the 3-probe path-varying burst)

```yaml
# Phase 37: url_path RBAC condition. 3 path-varying probes prove the
# query-stripped path match is byte-identical cross-proxy. Probe (c) /allowed?x=1
# is the load-bearing discriminator: Envoy strips the query (ADR-0090 §B), so it
# matches url_path:{exact:/allowed} and returns 200 — a naive whole-:path compare
# would 403 it. 403 body is "RBAC: access denied" (19 bytes, no newline, ADR-0034).
# Reuses the fixture-0043 http1_probe_list driver (per-probe path; NO extra_headers).
driver:
  kind: http1_probe_list
  probes:
    - name: probe-1-allow-exact
      method: get
      path: /allowed
      host: envoy-rust.test
      expected_status: 200
      expected_body: { kind: byte_exact, body: "ok\n" }
      expected_headers: set_equal_modulo_allow_list
    - name: probe-2-deny-miss
      method: get
      path: /denied
      host: envoy-rust.test
      expected_status: 403
      expected_body: { kind: byte_exact, body: "RBAC: access denied" }
      expected_headers: set_equal_modulo_allow_list
    - name: probe-3-allow-query-strip
      method: get
      path: /allowed?x=1
      host: envoy-rust.test
      expected_status: 200
      expected_body: { kind: byte_exact, body: "ok\n" }
      expected_headers: set_equal_modulo_allow_list
```

- [ ] **Step 4: Write `README.md`** — explain the query-strip differential (cite ADR-0090 §B), the `[rbac, router]`
  chain (no producer), the 3 probes, and that `#fragment`/normalization are OUT (R1/M37-1, ADR-0090).

- [ ] **Step 5: Create the per-fixture test wrapper** — `tests/differential/tests/rbac_url_path.rs` (there is NO
  manifest and NO glob: every fixture is run by a dedicated hand-written `#[tokio::test]` — see
  `tests/differential/tests/rbac_dynamic_metadata.rs`). Without this file the fixture NEVER executes:

```rust
//! Phase 37 differential acceptance test: the RBAC `url_path` condition. Drive 3
//! path-varying GET probes through an HCM `[rbac, router]` chain whose RBAC
//! `action: ALLOW` single policy matches `url_path: { path: { exact: "/allowed" } }`:
//!   - probe 1: GET /allowed     -> match              -> 200 + "ok\n"
//!   - probe 2: GET /denied      -> no match           -> 403 + "RBAC: access denied"
//!   - probe 3: GET /allowed?x=1 -> query stripped (ADR-0090 §B) -> match -> 200 + "ok\n"
//! Probe 3 is the load-bearing discriminator: a naive whole-:path compare would 403 it.
//! 403 body is "RBAC: access denied" (19 bytes, no newline, ADR-0034). LOCALLY
//! authoritative (no reload trigger). Docker-gated by the harness at the cluster level.

use std::path::PathBuf;

#[tokio::test]
async fn rbac_url_path() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0045-http-rbac-url-path");
    differential::run_fixture(&dir).await.expect("fixture passes");
}
```

- [ ] **Step 6: Build the debug binary first** (per the differential-harness-uses-debug-binary discipline — a new
  config key needs a fresh debug `envoy-bin` or the differential REDs with stale `unknown field`):

```bash
cargo build -p envoy-bin
```

- [ ] **Step 7: Run the fixture differentially**

```bash
cargo test -p differential rbac_url_path
```
Expected: PASS (Envoy `v1.33.0` container vs envoy-rust subprocess; all 3 probes byte-identical).
> Filter by the TEST NAME `rbac_url_path` — NOT `0045` (test names are non-numeric; `0045` matches zero tests and
> cargo would exit 0 = a FALSE green).
> NOTE (host caveat): per the project memory, some differential fixtures false-RED on this Docker-Desktop host
> under parallel load / bridge-IP routing. This is a `direct_response` fixture (no backend), so the bridge-IP set
> does not apply, but if it false-REDs, run it in isolation and treat CI as authoritative (state-4 gate).

- [ ] **Step 8: Commit**

```bash
git add tests/fixtures/0045-http-rbac-url-path/ tests/differential/tests/rbac_url_path.rs
git commit -m "phase 37: fixture 0045-http-rbac-url-path + differential wrapper (match/miss/query-strip) [ADR-0090]"
```

---

## Task 7: fuzz seed + BEHAVIOR_CONTRACT + final §7.5 gate

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_url_path.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (un-ignore line)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (RBAC `url_path` subsection)

- [ ] **Step 1: Write the fuzz seed** — a full bootstrap with an RBAC `url_path` condition incl. a `safe_regex`
  value (so the seed reaches the `regex` compile path). Template off the existing
  `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_metadata.yaml`; swap the `metadata` permission for
  `url_path: { path: { safe_regex: { regex: "^/allowed/[0-9]+$" } } }`.

- [ ] **Step 2: Un-ignore the seed** — add to `crates/envoy-config/fuzz/.gitignore` (after the `hcm_rbac_metadata`
  / `rbac_safe_regex` lines):

```
!corpus/parse_bootstrap/hcm_rbac_url_path.yaml
```
Then VERIFY it is tracked (the corpus dir is `*`-ignored by default — a missing `!` line leaves the seed invisible to CI):
```bash
git add crates/envoy-config/fuzz/.gitignore crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_url_path.yaml
git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_url_path.yaml   # must print the path
```
> NO new fuzz target (reuses `parse_bootstrap` — ADR-0090 §7); NO `ci.yml` change.

- [ ] **Step 3: Short-budget fuzz run** (the §7.5 (d) gate)

```bash
cd crates/envoy-config/fuzz && cargo fuzz run parse_bootstrap -- -runs=200000 -max_total_time=60
```
Expected: no crash; the new seed is exercised.

- [ ] **Step 4: Extend BEHAVIOR_CONTRACT** — add a subsection after the Phase 35/36 RBAC metadata subsections
  (~line 1344+ in `docs/envoy-rust/BEHAVIOR_CONTRACT.md`):

```markdown
### Phase 37 (ADR-0089/0090): the RBAC `url_path` Permission/Principal condition

> Phase 37 adds the `url_path` condition type (Envoy `type.matcher.v3.PathMatcher`,
> `url_path: { path: { <StringMatcher> } }`) to BOTH the RBAC `Permission` and
> `Principal` enums on the existing phase-10 filter. `url_path` matches the request
> path with the `?query` STRIPPED (ADR-0090 §B: query-strip ONLY — Envoy applies NO
> percent-decode / dot-segment / slash-merge / case normalization by default at
> v1.33.0). The cross-proxy witness is **fixture 0045** (`0045-http-rbac-url-path`):
> `/allowed`→200, `/denied`→403+`RBAC: access denied` (19B), `/allowed?x=1`→200 (the
> query-strip discriminator). `safe_regex` is RE2 FULL-match against the stripped path
> (anchored patterns are portable; M36-1). Config-validity (empty/path-less PathMatcher,
> unknown sub-key, malformed regex) is boot-fatal on BOTH proxies (ADR-0090 §D).
> CARRY-FORWARD M37-1: `#fragment` in the request-target is rejected at the H1 codec
> (400) before url_path matching — a separate codec surface, OUT of phase-37 scope.
```

- [ ] **Step 5: Run the FULL §7.5 phase-done gate** (quote every output into `PROGRESS.md` at state-4 — this is the
  state-3-implementation task list; the dedicated state-4 verification session runs the full gate, but run it here
  to confirm green before handing off):

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
cargo build -p envoy-bin && cargo test -p differential   # all 45 fixtures (0001-0045) green
```
Expected: all clean; fixture 0045 green; 0001–0044 still green (the regression-equivalence proof).

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/fuzz/ docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 37: url_path parse_bootstrap seed + BEHAVIOR_CONTRACT subsection [ADR-0090]"
```

---

## Acceptance (the §7.5 phase-done gate, previewed — verified at state-4, reviewed at state-5)

(a) fixture `0045` green (cross-proxy byte-identical: `/allowed`→200+`ok\n` / `/denied`→403+`RBAC: access denied`
/ `/allowed?x=1`→200+`ok\n`). (b) all of `0001`–`0044` green (regression equivalence — `url_path` is additive, no
existing config uses it). (c) h2spec ≥95% unchanged (no HTTP/2 change). (d) `parse_bootstrap` (+ the unchanged
`accesslog_format_parse`) fuzz clean for the short-budget CI run with the new `url_path` seed — NO new fuzz target.
(e) `cargo build --workspace --all-targets` / `clippy -D warnings` / `fmt --check` / `test --workspace` /
`deny check` all clean. (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/
`HttpFilterInstance` variant/`ConfigError` variant. M36-1 anchored-locked (NOT consumed). New carry-forward
**M37-1** (codec `#`-handling, R1).

_Scope locked by ADR-0089; ground truth locked by **ADR-0090** (§A–§D). §6.1 split did NOT fire (ADR-0091 UNFIRED).
The state-3 implementation is the next session (`superpowers:executing-plans` / `subagent-driven-development`)._
