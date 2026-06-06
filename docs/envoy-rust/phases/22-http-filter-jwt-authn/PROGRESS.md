# Phase 22 (`22-http-filter-jwt-authn`) — PROGRESS

> Running log, updated on each task completion (one PROGRESS commit per task,
> after the task's code commit). State-2 PLAN-write landed `PLAN.md` + this
> skeleton + the Task-1 preamble in one standalone pre-Task-1 commit (the
> 06.2 → 21 cadence). State-3 executes Tasks 1–11 via
> `superpowers:subagent-driven-development` (SERIAL dispatch,
> `feedback_serial_subagent_dispatch`; TDD per task; clippy PER TASK per
> `project_state3_arc_skips_clippy`). State-4 runs Task 12
> (`superpowers:verification-before-completion`).

---

## State-2 PLAN-write summary

- **Predecessor HEAD:** `7b0281d6b` (phase-22 state-1 brainstorm). This PLAN-write commit is the next; it flips ROADMAP row `22` `planned → in-progress` (invariant 4.1.3) and advances STATE to `22` state-2-complete / state-3-next.
- **Split-gate (§6.1): NOT triggered → SINGLE-PHASE.** The §6.2 crypto-API resolution (lock-in L1) found the CLEAN `aws_lc_rs::rsa::PublicKeyComponents::verify` path (no DER assembly, no feature flag, ~15 LoC) — the swing factor resolved small. The PLAN is 12 tasks / ~1350–1500 LoC, inside the ~25-task / ~1500-LoC gate. **ADR-0057 (split) does NOT fire.**
- **ADR-0056 FIRES** (the §6.2 empirical reconciliation — appended to DECISIONS.md at this commit). Multiple material divergences from the SPEC projections (see the lock-in summary below).

## §6.2 empirical lock-ins (LOCAL Docker, `envoyproxy/envoy:v1.33.0` digest `sha256:56da5afd…`, 2026-06-06)

The full table lives in `PLAN.md` (L1–L7) + ADR-0056. Headlines:

- **L1 (crypto API):** `PublicKeyComponents { n, e }.verify(&RSA_PKCS1_2048_8192_SHA256, msg, sig)` — direct, no DER. Constraints: modulus **2048..=8192 bits** (test key MUST be RSA-2048); strip leading `0x00` from `n`/`e`.
- **L2 (failure taxonomy):** 10 byte-exact classes. **Audience-not-allowed → 403** (all others 401). Body strings corrected vs SPEC §2.2 ("Jwt header is an invalid **JSON**", not "JWT"; issuer → "Jwt issuer is not configured"; malformed-form → the 79-byte "…two dots and 3 sections"; unsupported-alg folds into "Jwks doesn't have key to match kid or alg from Jwt").
- **L3 (`www-authenticate`):** DYNAMIC = `Bearer realm="http://<Host-header><path>"` (+ `, error="invalid_token"` for all non-missing classes). Reproducible byte-exact → value-exact with a fixed-Host fixture.
- **L4:** no-matching-rule → ALLOW (200, counts in `allowed`).
- **L5:** stat namespace `http.<hcm_stat_prefix>.jwt_authn.{allowed,denied}` CONFIRMED; 5 Envoy-only siblings unasserted.
- **L6:** `forward` default `false` ⇒ strip `Authorization` on success.
- **L7:** `aud` is `string | string[]`; empty provider audiences ⇒ no aud check.

## PLAN-write SPEC corrections (mechanical drift flagged against HEAD `7b0281d6b`)

1. **`DataSource` ALREADY EXISTS** at `crates/envoy-config/src/bootstrap.rs:556` (`{ filename, inline_string }`) — REUSE for `local_jwks` (SPEC §D2 left this open; resolved: do NOT author a new type).
2. **`RouteMatch`** at `bootstrap.rs:1386` is `{ prefix: Option<String>, path: Option<String>, headers: Vec<HeaderMatcher> }`; its HCM evaluator `route_matches` (`crates/envoy-http1/src/hcm.rs:1248`) is H1-private — the filter re-implements the same ~6-line prefix-XOR-path + AND-headers logic over `FilterRequest` (Task 7 `route_match_matches`; `HeaderMatcher::matches` at `matcher.rs:19` is public and reused).
3. **`HttpFilterTypedConfig`** at `bootstrap.rs:692` is `#[serde(tag="@type", deny_unknown_fields)]` with 5 variants — a jwt_authn `@type` currently FAILS TO PARSE (unknown variant); adding the variant IS the enablement (there is no separate runtime reject-list to remove; the per-arm `f.name` check in `validate_http_filters` at `bootstrap.rs:2628` is the only name guard). Task 1 should grep for any existing `jwt_authn` reject test to update.
4. **Fixture number is `0030`** (29 pre-existing 0001–0029). (A recon agent transiently mislabeled it `0022`; the SPEC + this PLAN use `0030`.)
5. **`build_from_config(cfg, &Arc<StatsRegistry>, &str)`** — the phase-10 3-arg `hcm_stat_prefix` threading is reused UNCHANGED (no signature widening); confirmed against `fault.rs:39` + `instance.rs:74`.
6. **aws-lc-rs TEST signing API** (`RsaKeyPair::generate` / `public_key().as_be_bytes()` for `PublicKeyComponents<Vec<u8>>`) is the ONE detail to confirm at Task-3 authoring — the *production* verify path (`PublicKeyComponents::verify`) is confirmed (L1). If method names differ in 1.16.3, adjust the test helper only.
7. **Fuzz corpus count:** the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array currently lists ~28 named seeds (+ `minimal.yaml`); the SPEC's "32→33" counts the full curated corpus differently. Task 9 reads the actual test + `.gitignore` and increments BOTH the allow-list and the SUCCESS array (and any in-test count constant) correctly — do not trust a projected number.
8. **`http1_probe_list` driver** key + `extra_headers` shape: confirm the exact serde key against `tests/differential/src/lib.rs` (`Http1ProbeList`/`Http1Probe`) at Task-10 authoring.

---

## Task ledger

| Task | Title | State | Code commit | PROGRESS commit |
|---|---|---|---|---|
| — | state-2 PLAN-write (PLAN.md + this skeleton + Task-1 preamble + ADR-0056) | **done (this commit)** | — | — |
| 1 | envoy-jwt scaffold + base64url | **done** | `745cf8da1` | (this commit) |
| 2 | envoy-jwt JWKS parse | **done** | `ea02c3b7d` | (this commit) |
| 3 | envoy-jwt RS256 verify + claim validation | **done** | `747590a2e` | (this commit) |
| 4 | envoy-jwt fuzz target | **done** | `e514d88f2` | (this commit) |
| 5 | envoy-config schema + variant | **done** | `6d6a7f38e` | (this commit) |
| 6 | envoy-config validator + arm + ConfigError | pending | — | — |
| 7 | envoy-filter JwtAuthnFilter + stats + wire map + BEHAVIOR_CONTRACT | pending | — | — |
| 8 | HttpFilterInstance::JwtAuthn variant + dispatch | pending | — | — |
| 9 | parse_bootstrap fuzz seed | pending | — | — |
| 10 | fixture 0030 + Docker wrapper + static inputs | pending | — | — |
| 11 | in-process backstop | pending | — | — |
| 12 | state-4 verification + STATE advance | pending | — | — |

---

## Task 1 preamble (pre-execution notes for the first state-3 subagent)

**Goal:** create the `envoy-jwt` workspace member (crate root `#![forbid(unsafe_code)]`, D-3.8 — no exemption; `aws-lc-rs` encapsulates its FFI internally), the `JwtError` taxonomy, and the hand-rolled base64url decoder, all green via TDD.

**Watch-outs:**
- Add ONLY `"crates/envoy-jwt",` to the workspace `members` in Task 1; add `"crates/envoy-jwt/fuzz",` in Task 4 (the dir must exist for the member to resolve).
- `lib.rs` declares `mod jwks;` + `mod verify;` which Tasks 2/3 fill — Task 1 commits minimal stubs so each commit stays green (PLAN Task 1 Step 5 note), OR the executor may fold Tasks 1–3 into one commit (one crate) — reviewer's call; the PLAN keeps them separate for granularity.
- base64url: reject `=` padding and any non-`[A-Za-z0-9-_]` char (JWT/JWKS are unpadded URL-safe). Tests cover known vectors + rejection.
- `edition = "2024"` (matches `envoy-health`); `aws-lc-rs = "1.16"` default features (L1 — no feature flag needed).
- Per `project_state3_arc_skips_clippy`, run `cargo clippy -p envoy-jwt --all-targets -- -D warnings` before the Task-1 commit.

**Commit shape:** one code commit `phase 22 Task 1: …` then one PROGRESS commit updating this ledger row + appending a Task-2 preamble.

**Task 1 outcome (controller verification):** code commit `745cf8da1` — 8 files, only `crates/envoy-jwt` added to workspace `members` (the `crates/envoy-jwt/fuzz` member correctly deferred to Task 4). Crate root carries `#![forbid(unsafe_code)]` (D-3.8, no exemption). `JwtError` (10 variants), hand-rolled base64url decoder (`#[allow(dead_code)]` targeted on `decode` while the stubs are in place — Task 2 consumes it), `jwks.rs`/`verify.rs` minimal stubs. `cargo test -p envoy-jwt` 2/2 green; `cargo clippy -p envoy-jwt --all-targets -- -D warnings` clean; fmt clean. `aws-lc-rs` 1.16 builds natively. Not a review centerpiece → controller diff-verification only (per the phase-16→21 cadence; centerpieces are Tasks 3 + 7).

---

## Task 2 preamble (pre-execution notes for the second state-3 subagent)

**Goal:** replace the `jwks.rs` stub with real inline-JWKS (RSA-only) parsing: `JwkSet::parse(&str) -> Result<Self, JwtError>` keeping only `kty == "RSA"` keys, base64url-decoding `n`/`e`, stripping any leading `0x00` byte (§6.2 L1 — `aws-lc-rs` `PublicKeyComponents` rejects leading zeros), `JwtError::InvalidJwks` on non-JSON / missing `keys` / missing-or-undecodable `n`/`e` / empty resulting RSA set; `keys()` accessor returning `&[RsaKey]` where `RsaKey { kid: Option<String>, n: Vec<u8>, e: Vec<u8> }`. TDD: structural tests (`rejects_non_json`, `rejects_empty_keyset`, `skips_non_rsa_keys_but_errors_if_none_remain`, `parses_rsa_key`) — the structural ones need no real modulus; for `parses_rsa_key` a real base64url RSA-2048 `n` is ideal but a well-formed small `n` suffices for the `e == [0x01,0x00,0x01]` ("AQAB") assertion + key count. The decoder's `#[allow(dead_code)]` from Task 1 can stay or be removed once `jwks.rs` consumes `base64url::decode` (resolve whatever clippy reports). Clippy `-p envoy-jwt` per task.

**Watch-outs:** the `decode` consumption removes the Task-1 dead-code condition — re-check clippy. `RsaKey`/`JwkSet` derive `Debug, Clone, PartialEq, Eq`. Full code is in PLAN.md Task 2 Step 3 (controller pastes it into the subagent prompt verbatim).

**Commit shape:** one code commit `phase 22 Task 2: …`; controller does the PROGRESS commit.

**Task 2 outcome (controller verification):** code commit `ea02c3b7d` — 2 files (`jwks.rs` filled; `base64url.rs` shed its now-redundant `#[allow(dead_code)]` since `JwkSet::parse` consumes `decode`). TDD honored (test failed on stub, passed after impl). `RsaKey { kid, n, e }` + `JwkSet::{parse, keys}`; RSA-only filter, leading-zero strip (§6.2 L1), `InvalidJwks` on non-JSON/missing-keys/undecodable/empty. The plan's placeholder `n` (`"sXch4i4X..."`, contains a `.`) was replaced with valid base64url `"sXche4iX"` (only key-count/kid/`e==[1,0,1]` are asserted). `cargo test -p envoy-jwt` 6/6; clippy `-p envoy-jwt` gate re-verified clean by controller; fmt clean. Not a centerpiece → controller diff-verification only.

---

## Task 3 preamble (pre-execution notes — REVIEW CENTERPIECE #1)

**Goal:** replace the `verify.rs` stub with the real `verify_rs256(token, &JwkSet, expected_issuer, &[String] allowed_audiences, now_unix: i64) -> Result<VerifiedJwt, JwtError>` — the crypto orchestration. Production verify path is the §6.2-L1-confirmed `aws_lc_rs::rsa::PublicKeyComponents { n, e }.verify(&RSA_PKCS1_2048_8192_SHA256, signing_input, sig)`. Order of checks (PLAN Task 3 Step 3): (1) exactly 3 non-empty dot segments else `NotInForm`; (2/3) b64-decode+JSON header/payload (`BadHeaderJson`/`BadPayloadJson`; decode failure → `NotInForm`); (4) b64-decode sig; (5) `alg != "RS256"` → `NoMatchingKey` (Envoy folds unsupported-alg here); (6) candidate keys by `kid` (else all), empty → `NoMatchingKey`; (7) verify over `header.payload` signing input, fail → `VerificationFails`; (8) issuer mismatch → `IssuerMismatch`; (9) `now>=exp` → `Expired`, `now<nbf` → `NotYetValid`; (10) audience (empty allowed_audiences ⇒ skip; §6.2 L7, `aud` is `string|string[]`). `VerifiedJwt { iss, aud, exp, nbf }`.

**THE ONE AUTHORING RISK (PROGRESS correction #6):** the TEST signing helper uses `aws-lc-rs` 1.16.3 signing APIs (`RsaKeyPair::generate(KeySize::Rsa2048)`, `public_key().as_be_bytes()` → `PublicKeyComponents<Vec<u8>>`, `kp.sign(&RSA_PKCS1_SHA256, &SystemRandom::new(), msg, &mut sig)`). If exact 1.16.3 method/trait names differ (`as_be_bytes` vs `as_big_endian`; `AsBigEndian` import path; `KeySize` path), the implementer ADJUSTS THE TEST HELPER ONLY — the production path (`PublicKeyComponents::verify`) is L1-confirmed and must NOT change. The test key MUST be RSA-2048 (the `RSA_PKCS1_2048_8192_SHA256` 2048-bit floor). The let-chain `if let Some(x)=.. && cond` syntax is stable on 1.95.0 but if clippy flags it, rewrite as nested `if let`.

**Review:** this is review centerpiece #1 → full TWO-STAGE review (spec-compliance subagent THEN code-quality subagent) after the implementer's self-review, with re-review loops until both pass. Clippy `-p envoy-jwt` per task. Also restore the real `pub use verify::{VerifiedJwt, verify_rs256};` in `lib.rs` (it already points there from Task 1; confirm signatures line up).

**Commit shape:** one code commit `phase 22 Task 3: …`; controller does the PROGRESS commit after both reviews pass.

**Task 3 outcome (REVIEW CENTERPIECE #1 — full two-stage review):** code commit `747590a2e` (amended once to fold in recommended test coverage). TDD honored (26 compile-errors on the stub at Step 2 → green at Step 5). **Production verify path is the §6.2-L1-locked `PublicKeyComponents{n,e}.verify(&RSA_PKCS1_2048_8192_SHA256, signing_input, sig)` — UNCHANGED.** All 10 checks present in spec order with correct `JwtError` variants. **Authoring-risk resolution (PROGRESS correction #6):** the plan's guessed test-signing API `pk.as_be_bytes()`/`AsBigEndian` does NOT exist for RSA in `aws-lc-rs` 1.16.3 — the TEST helper (only) uses `PublicKeyComponents::from(pk)` (the `From<&PublicKey>` impl, gated by the default `ring-io` feature). Production path untouched; test key is RSA-2048. **Spec review: ✅ SPEC COMPLIANT** (all 10 checks, no extra API, tests sign real RSA-2048 tokens, slice `&token.as_bytes()[..h.len()+1+p.len()]` correct). **Code-quality review: ✅ APPROVED** (0 Critical/0 Important; slice-panic path — the key fuzz worry — statically confirmed safe; 6 advisory Minors). Folded M3 (no-`kid`→all-keys branch test), M4 (multi-element `aud` partial-intersection test), M1 (drop redundant `.as_str()`), M6 (`iss`-always-`Some` doc) into the amend → **12 tests pass**; clippy `-p envoy-jwt --all-targets -D warnings` clean (controller re-verified). Deferred advisory Minors M2 (split the 4-assert test) + M5 (`nbf==now` boundary test) — non-blocking.

---

## Task 4 preamble (pre-execution notes for the fourth state-3 subagent)

**Goal:** add the `envoy-jwt` fuzz target (§7.4) over the JWKS/JWT parse+verify surface — mirroring the existing `crates/envoy-config/fuzz/` crate. Create `crates/envoy-jwt/fuzz/{Cargo.toml, fuzz_targets/jwt_parse.rs, .gitignore, corpus/jwt_parse/*}` and add `"crates/envoy-jwt/fuzz",` to the workspace `members` (NOW the dir exists). The target: split input on first NUL → bytes-before = JWKS JSON, bytes-after = token; `if let Ok(set) = JwkSet::parse(jwks_str) { let _ = verify_rs256(token, &set, "iss", &[], 0); }`. `#![no_main] #![forbid(unsafe_code)]`. Both surfaces must NEVER panic — only return `JwtError` (Task 3 quality review statically confirmed the slice path is panic-safe; the fuzzer is the dynamic backstop). 3 seed files: `empty` (0 bytes), `jwks.json` (real RSA JWKS + `\0` + valid token), `token.txt` (`\0`-prefixed garbage token).

**Watch-outs:** mirror `crates/envoy-config/fuzz/Cargo.toml` for manifest shape (`cargo-fuzz = true`, `libfuzzer-sys = "0.4"`, `[[bin]]` with `test=false doc=false bench=false`, `edition = "2024"`). `cargo fuzz` needs the NIGHTLY toolchain (the envoy-config/fuzz precedent). Build via `cargo +nightly fuzz build jwt_parse` then a smoke run `cargo +nightly fuzz run jwt_parse -- -runs=50000 -max_total_time=30` — expect builds + no crash. The real RSA JWKS for the seed can be generated however convenient (or reuse the `PublicKeyComponents::from` approach in a throwaway), but a committed real RSA-2048 JWKS is ideal so the verify path actually executes; if generating one is awkward at this task, a syntactically-valid small RSA JWKS that `JwkSet::parse` accepts is acceptable for corpus-seed purposes (the fuzzer explores from there). Task 10 later commits the canonical fixture JWKS. NOTE: this task may need the nightly toolchain installed; if `cargo +nightly fuzz` is unavailable, report it — the build can still be validated structurally and the smoke-run deferred to the state-4 CI gate (Task 12).

**Commit shape:** one code commit `phase 22 Task 4: …`; controller does the PROGRESS commit.

**Task 4 outcome (controller verification):** code commit `e514d88f2` (amended once by controller). **PLAN CORRECTION #9 (mechanical drift vs disk):** PLAN Task 4 Step 4 said add `crates/envoy-jwt/fuzz` to workspace `members` — WRONG. The actual repo pattern (verified) is `exclude = [...]` with the fuzz crate carrying its own empty `[workspace]` block (the `crates/envoy-config/fuzz` precedent), so cargo-fuzz builds it standalone via `--manifest-path` and it is NOT pulled into the main `cargo build --workspace`. Implemented as `exclude`. **PLAN CORRECTION #10:** the fuzz `Cargo.lock` is NOT tracked (the config-fuzz precedent — left untracked) → controller added `Cargo.lock` to `crates/envoy-jwt/fuzz/.gitignore` + `git rm --cached` the lock the subagent had staged, then amended. Fuzz target `jwt_parse` splits input on first NUL (JWKS | token), runs `JwkSet::parse` then `verify_rs256` — both must never panic. `cargo +nightly fuzz build jwt_parse` succeeds; 50k-run smoke clean (0 crashes). 3 seeds committed (`empty`/`jwks.json`/`token.txt`). Main workspace still resolves (`cargo build -p envoy-jwt` + `cargo metadata` clean). Not a centerpiece → controller diff-verification only. The state-4 gate (Task 12) re-runs the short-budget CI fuzz.

---

## Task 5 preamble (pre-execution notes for the fifth state-3 subagent)

**Goal:** add the `envoy-config` schema for jwt_authn — `JwtAuthnConfig { providers: BTreeMap<String, JwtProvider>, rules: Vec<RequirementRule> }`, `JwtProvider { issuer, audiences: Vec<String> (default), local_jwks: DataSource, forward: bool (default false) }`, `RequirementRule { r#match: RouteMatch, requires: JwtRequirement }`, `JwtRequirement { provider_name: String }` — all `#[serde(deny_unknown_fields)]` — plus the `HttpFilterTypedConfig::JwtAuthn(JwtAuthnConfig)` variant (`@type` = `type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication`), and re-exports in `lib.rs`. NO validator yet (that is Task 6). **REUSE the existing `DataSource` (`bootstrap.rs:556`, `{ filename, inline_string }`) and `RouteMatch` (`bootstrap.rs:1386`, `{ prefix: Option<String>, path: Option<String>, headers: Vec<HeaderMatcher> }`) — do NOT author new types** (PLAN corrections #1/#2). TDD: `parses_jwt_authn_filter_config` (full YAML through `HttpFilter`) + `jwt_provider_forward_defaults_false`.

**Watch-outs:** confirm the exact line anchors before editing — `HttpFilterTypedConfig` is at `bootstrap.rs:692` per correction #3, `#[serde(tag="@type", deny_unknown_fields)]` with 5 existing variants; insert the JwtAuthn variant after `Fault`. A jwt_authn `@type` currently FAILS to parse (unknown variant) — adding the variant IS the enablement; grep for any existing test that asserts jwt_authn is rejected and update it. Re-exports: find the existing `pub use bootstrap::{… FaultConfig …}` line in `lib.rs` and append `JwtAuthnConfig, JwtProvider, RequirementRule, JwtRequirement`. Full code in PLAN.md Task 5 (controller pastes verbatim). Clippy `-p envoy-config` per task. NOTE: Task 5 adds NO dependency on `envoy-jwt` (that path-dep arrives in Task 6 with the validator).

**Commit shape:** one code commit `phase 22 Task 5: …`; controller does the PROGRESS commit.

**Task 5 outcome (controller verification):** code commit `6d6a7f38e` — 2 files. Real anchors CONFIRMED: `DataSource` `bootstrap.rs:556` `{ filename, inline_string }` (derives Debug/Clone/Serialize/Deserialize/PartialEq) + `RouteMatch` `:1386` `{ prefix, path, headers }` (same derives) — both REUSED (corrections #1/#2 hold; no new types). Added the 4 structs (`JwtAuthnConfig`/`JwtProvider`/`RequirementRule`/`JwtRequirement`, all `deny_unknown_fields`), the `HttpFilterTypedConfig::JwtAuthn` variant (`@type` …`jwt_authn.v3.JwtAuthentication`, `:764`), 4 re-exports in `lib.rs`. **A jwt_authn `@type` previously failed to parse (unknown variant); no reject-test existed (grep empty).** The `validate_http_filters` match required a new arm — added a STOPGAP `JwtAuthn(_) => {}` at `:2740` (Task 6 replaces it with the real validator arm). `cargo test -p envoy-config` 394/394 (incl. the 2 new parse tests); clippy `-p envoy-config` clean. Not a centerpiece → controller diff-verification only.

---

## Task 6 preamble (pre-execution notes for the sixth state-3 subagent)

**Goal:** add the jwt_authn validator + wire it in. (1) Add path-dep `envoy-jwt = { path = "../envoy-jwt" }` to `crates/envoy-config/Cargo.toml` `[dependencies]` (the new `envoy-config → envoy-jwt` edge — a clean leaf DAG). (2) Add 3 `ConfigError` variants in `lib.rs`: `JwtAuthnNoProviders { listener }`, `JwtAuthnUnknownProvider { listener, provider_name }`, `JwtAuthnInvalidJwks { listener, provider }`. (3) Implement `pub(crate) fn validate_jwt_authn_config(cfg, listener_name) -> Result<(), ConfigError>` (near `validate_fault_config`): reject empty providers; for each provider require `local_jwks.inline_string` present AND `envoy_jwt::JwkSet::parse` succeeds (else `JwtAuthnInvalidJwks`); for each rule require the `provider_name` exists in providers (else `JwtAuthnUnknownProvider`) + validate the `RouteMatch` structurally. (4) REPLACE the Task-5 stopgap `JwtAuthn(_) => {}` arm at `bootstrap.rs:2740` with the real arm: check `f.name == "envoy.filters.http.jwt_authn"` (else `UnsupportedHttpFilter`) then call `validate_jwt_authn_config`. TDD: `jwt_authn_validator_rejects_empty_providers`, `_rejects_dangling_provider_ref`, `_rejects_bad_jwks`, `_accepts_valid` (PLAN Task 6 Step 3 — needs a `VALID_JWKS` `const &str` real RSA-2048 JWKS in the test module; Task 10 commits the canonical one, but Task 6 needs a real parseable RSA JWKS NOW — generate an RSA-2048 JWKS or reuse one; a syntactically valid RSA JWKS that `JwkSet::parse` ACCEPTS is required since the `_accepts_valid`/`_rejects_dangling` tests call the real validator which calls `JwkSet::parse`).

**Watch-outs:** the route-match structural validator — grep for the existing fn enforcing "exactly one of prefix/path" (likely returns `ConfigError::UnsupportedRouteMatcher`); if not separately callable, inline `match (m.prefix.is_some(), m.path.is_some()) { (true,false)|(false,true) => Ok(()), _ => Err(UnsupportedRouteMatcher{...}) }` and FLAG the exact name found in the PROGRESS Task-6 entry. The `_rejects_dangling_provider_ref` test constructs a `DataSource { filename: None, inline_string: Some(VALID_JWKS) }` and `RouteMatch { prefix: Some("/"), path: None, headers: vec![] }` — confirm those exact field names/shapes. `cargo build -p envoy-config` MUST stay green AND `cargo build -p envoy-jwt` independently (per `project_isolated_crate_build_blindspot`). Clippy `-p envoy-config` per task. Full code in PLAN.md Task 6.

**Commit shape:** one code commit `phase 22 Task 6: …`; controller does the PROGRESS commit.
