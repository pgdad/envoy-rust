# Phase 22 (`22-http-filter-jwt-authn`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. SERIAL dispatch only (`feedback_serial_subagent_dispatch` — never parallel implementers; they race on shared `main`). TDD per task (`superpowers:test-driven-development`). Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK (`project_state3_arc_skips_clippy` — clippy is otherwise first seen at the state-4 gate). One code commit + one PROGRESS commit per task.

**Goal:** Land `envoy.filters.http.jwt_authn` (RS256, inline local JWKS, single-provider rules, minimum-viable) as the sixth `HttpFilterInstance` variant, isolating all JWT crypto/parsing in a new leaf crate `envoy-jwt` (using `aws-lc-rs`), with a byte-exact-to-Envoy 401/403 failure surface, fixture `0030-http-filter-jwt-authn`, an in-process backstop, and a new fuzz target.

**Architecture:** A new leaf crate `envoy-jwt` owns base64url decode + JWKS parse + RS256 verify + claim validation behind a small interface (depends ONLY on `aws-lc-rs` + `serde`/`serde_json` + `thiserror`). `envoy-config` gains the `JwtAuthnConfig` schema + a validator that calls `envoy_jwt::JwkSet::parse` for fail-fast JWKS validation (path-dep on `envoy-jwt`). `envoy-filter` gains `JwtAuthnFilter` (hand-rolled rule selection + token extraction + the `JwtError`→(status, body, `www-authenticate`) mapping; path-dep on `envoy-jwt`). The 401/403 `Decision::StopAndSend` flows through the existing H1/H2 filter-synth decoration helpers UNCHANGED.

**Tech Stack:** Rust 1.95.0, `aws-lc-rs` 1.16.3 (`PublicKeyComponents::verify`, no feature flag, no DER assembly), `serde`/`serde_json`, `thiserror`, `bytes`, `tokio` (dev), `testcontainers` (differential harness), `cargo fuzz` (libfuzzer).

---

## §6.2 empirical lock-ins (verified LOCALLY against `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd…`, 2026-06-06)

These are the authoritative wire facts. They REPLACE the SPEC's projections where they differ; the divergences are reconciled by **ADR-0056** (landing at this PLAN-write commit). Do NOT re-assume any body string — use these exact bytes.

**L1 — the crypto-API path (the split-gate swing factor; resolved CLEAN).** `aws_lc_rs::rsa::PublicKeyComponents { n, e }.verify(&aws_lc_rs::signature::RSA_PKCS1_2048_8192_SHA256, message, signature)` verifies RS256 directly from raw big-endian modulus/exponent. **No feature flag** (available with default features), **no DER assembly** (~15 LoC). Two constraints: (a) the modulus must be **2048..=8192 bits** (the algorithm constant enforces it — the fixture/test key MUST be RSA-2048, never smaller); (b) leading `0x00` bytes of `n`/`e` must be stripped (JWKS supplies unsigned big-endian; `aws-lc-rs` rejects a leading-zero first byte). → **The `envoy-jwt` crate is small; phase 22 ships SINGLE-PHASE (no split; ADR-0057 does NOT fire).**

**L2 — the byte-exact failure taxonomy.** Each failure class → (HTTP status, body byte-exact, `www-authenticate`). Bodies have **NO trailing newline**; `content-type: text/plain`. `www-authenticate` is `Bearer realm="{realm}"` for the *missing* class and `Bearer realm="{realm}", error="invalid_token"` for ALL other classes (`{realm}` = L3).

| Class (envoy-rust `JwtError` / filter cause) | Status | Body (exact) | len |
|---|---|---|---|
| Missing token / non-`Bearer ` scheme (filter-level) | 401 | `Jwt is missing` | 14 |
| `NotInForm` (≠3 segments, empty sig, base64-undecodable segment, alg=none) | 401 | `Jwt is not in the form of Header.Payload.Signature with two dots and 3 sections` | 79 |
| `BadHeaderJson` (header b64-ok but not JSON) | 401 | `Jwt header is an invalid JSON` | 29 |
| `BadPayloadJson` (payload b64-ok but not JSON) | 401 | `Jwt payload is an invalid JSON` | 30 |
| `NoMatchingKey` (kid not in JWKS **or** alg≠RS256) | 401 | `Jwks doesn't have key to match kid or alg from Jwt` | 50 |
| `VerificationFails` (signature invalid) | 401 | `Jwt verification fails` | 22 |
| `IssuerMismatch` (token `iss` ≠ provider issuer) | 401 | `Jwt issuer is not configured` | 28 |
| `Expired` (`now >= exp`) | 401 | `Jwt is expired` | 14 |
| `NotYetValid` (`now < nbf`) | 401 | `Jwt not yet valid` | 17 |
| `AudienceNotAllowed` (no `aud` ∩ provider audiences) | **403** | `Audiences in Jwt are not allowed` | 32 |

**L3 — the `www-authenticate` realm is DYNAMIC** = `http://` + the verbatim `Host` request header + the request path (e.g. `Host: envoy.test`, `GET /` → `realm="http://envoy.test/"`). Port-independent of the listener; scheme hardcoded `http` (plaintext minimum scope; TLS-jwt scheme defers). Because it's reproducible byte-exactly as `format!("http://{host}{path}")`, the differential fixture drives a **fixed `Host`** so the value is identical across proxies → **value-exact** (enforced automatically by `set_equal_modulo_allow_list`, which compares non-allow-listed headers byte-exact).

**L4 — no-matching-rule disposition = ALLOW.** A request whose path matches NO `rules[]` entry passes through (200) and increments `allowed`. (Confirmed: `allowed: 2` after one valid + one no-rule probe.)

**L5 — stat namespace = `http.<hcm_stat_prefix>.jwt_authn.{allowed,denied}`** (HCM-prefixed; SPEC projection CONFIRMED). `allowed` counts BOTH verification-success AND no-matching-rule pass-through. `denied` counts every failure (including the 403 audience case). Envoy ALSO emits 5 siblings envoy-rust does NOT at minimum scope (`cors_preflight_bypassed`, `jwks_fetch_success`, `jwks_fetch_failed`, `jwt_cache_hit`, `jwt_cache_miss`) — Envoy-only-unasserted (no set-diff on the `Http1ProbeList`/named-stat scrape; documented in BEHAVIOR_CONTRACT).

**L6 — on-success `forward` default = strip Authorization.** Envoy's provider `forward` default is `false` ⇒ the `Authorization` header is STRIPPED from the request before it continues upstream; `forward: true` keeps it. envoy-rust mirrors: default `false`, strip on success.

**L7 — `aud` accepts a single string OR an array of strings** (JWT `aud` claim is `string | string[]`). Provider `audiences` empty ⇒ no audience check.

---

## File structure

**Created:**
- `crates/envoy-jwt/Cargo.toml`, `crates/envoy-jwt/src/lib.rs`, `crates/envoy-jwt/src/error.rs`, `crates/envoy-jwt/src/base64url.rs`, `crates/envoy-jwt/src/jwks.rs`, `crates/envoy-jwt/src/verify.rs` — the leaf crypto/parse crate.
- `crates/envoy-jwt/fuzz/Cargo.toml`, `crates/envoy-jwt/fuzz/fuzz_targets/jwt_parse.rs`, `crates/envoy-jwt/fuzz/.gitignore`, `crates/envoy-jwt/fuzz/corpus/jwt_parse/*` — the new fuzz target (§7.4).
- `crates/envoy-filter/src/jwt_authn.rs` — the runtime filter.
- `crates/envoy-bin/tests/http_filter_jwt_authn.rs` — the in-process backstop.
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_jwt_authn_filter.yaml` — the new bootstrap fuzz seed.
- `tests/fixtures/0030-http-filter-jwt-authn/{README.md,envoy.yaml,envoy-rust.yaml,expectations.yaml,inputs/}` — the differential fixture (+ committed static tokens/JWKS + a documented `gen.py`).
- `tests/differential/tests/http_filter_jwt_authn.rs` — the Docker-gated wrapper.

**Modified:**
- `Cargo.toml` (workspace `members`: add `crates/envoy-jwt`, `crates/envoy-jwt/fuzz`).
- `crates/envoy-config/Cargo.toml` (path-dep on `envoy-jwt`), `crates/envoy-config/src/bootstrap.rs` (schema + variant + validator arm + `validate_jwt_authn_config`), `crates/envoy-config/src/lib.rs` (re-exports + `ConfigError` variants).
- `crates/envoy-filter/Cargo.toml` (path-dep on `envoy-jwt`), `crates/envoy-filter/src/instance.rs` (the `JwtAuthn` variant + 3 dispatch arms), `crates/envoy-filter/src/lib.rs` (module + re-export).
- `crates/envoy-config/fuzz/.gitignore` + the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS list in `crates/envoy-config/src/bootstrap.rs`.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (jwt_authn stat rows + the 401/403 failure-body table + `www-authenticate`).

---

## Task 1: `envoy-jwt` crate scaffold + base64url decoder

**Files:**
- Create: `crates/envoy-jwt/Cargo.toml`, `crates/envoy-jwt/src/lib.rs`, `crates/envoy-jwt/src/error.rs`, `crates/envoy-jwt/src/base64url.rs`
- Modify: `Cargo.toml` (workspace `members`)

- [ ] **Step 1: Create the crate manifest** `crates/envoy-jwt/Cargo.toml`

```toml
[package]
name = "envoy-jwt"
version = "0.1.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_jwt"
path = "src/lib.rs"

[dependencies]
aws-lc-rs = "1.16"          # PublicKeyComponents::verify — default features suffice (ADR-0055/0056 L1)
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"

[dev-dependencies]
# RS256 test-token signing in unit tests is done via aws-lc-rs's signing API.
```

- [ ] **Step 2: Add the crate to the workspace** — in `Cargo.toml`, add `"crates/envoy-jwt",` to `members` (alongside the other `crates/*` entries, before the `tests/*` block) and `"crates/envoy-jwt/fuzz",` immediately after the existing `"crates/envoy-config/fuzz",` line (Task 4 fills the fuzz crate; adding the member now is harmless only if the dir exists — so add the `crates/envoy-jwt/fuzz` member in Task 4, NOT here. In THIS task add only `"crates/envoy-jwt",`).

- [ ] **Step 3: Write the failing test** in `crates/envoy-jwt/src/base64url.rs`

```rust
//! Hand-rolled base64url (RFC 4648 §5, no padding) decoder. JWTs and JWKS use
//! unpadded URL-safe base64; we avoid a `base64` crate dependency (§5.2).

/// Decode an unpadded base64url string. Rejects `=` padding and any character
/// outside the URL-safe alphabet (`A-Za-z0-9-_`).
pub(crate) fn decode(s: &str) -> Result<Vec<u8>, ()> {
    fn val(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        } as u32)
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4 + 3);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &c in s.as_bytes() {
        buf = (buf << 6) | val(c).ok_or(())?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes_known_vectors() {
        // "foobar" base64url unpadded = "Zm9vYmFy"
        assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
        // empty
        assert_eq!(decode("").unwrap(), b"");
        // a JWT header {"alg":"RS256","typ":"JWT"} encodes without '=' padding
        let hdr = decode("eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9").unwrap();
        assert_eq!(&hdr, br#"{"alg":"RS256","typ":"JWT"}"#);
    }

    #[test]
    fn rejects_padding_and_non_alphabet() {
        assert!(decode("Zm9vYmFy=").is_err(), "padding rejected");
        assert!(decode("@@@").is_err(), "non-alphabet rejected");
        assert!(decode("a b").is_err(), "space rejected");
    }
}
```

- [ ] **Step 4: Create the error module** `crates/envoy-jwt/src/error.rs`

```rust
//! The JWT failure taxonomy. Maps 1:1 to upstream Envoy v1.33's jwt_authn
//! failure classes (§6.2 L2; the HTTP-wire body bytes + statuses live in the
//! `envoy-filter` jwt_authn module, not here — this crate is HTTP-agnostic).

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum JwtError {
    /// A `local_jwks` JSON that does not parse as a non-empty RSA JWKS
    /// (config-load-time fatal — ADR-0049 all-fatal posture).
    #[error("invalid JWKS")]
    InvalidJwks,
    /// Token is not exactly 3 non-empty dot-separated segments, or a segment
    /// is not base64url-decodable.
    #[error("Jwt is not in the form of Header.Payload.Signature")]
    NotInForm,
    #[error("Jwt header is an invalid JSON")]
    BadHeaderJson,
    #[error("Jwt payload is an invalid JSON")]
    BadPayloadJson,
    /// No JWKS key matches the token's `kid`, or the token `alg` is not RS256.
    #[error("Jwks doesn't have key to match kid or alg from Jwt")]
    NoMatchingKey,
    #[error("Jwt verification fails")]
    VerificationFails,
    #[error("Jwt issuer is not configured")]
    IssuerMismatch,
    #[error("Jwt is expired")]
    Expired,
    #[error("Jwt not yet valid")]
    NotYetValid,
    #[error("Audiences in Jwt are not allowed")]
    AudienceNotAllowed,
}
```

- [ ] **Step 5: Create the crate root** `crates/envoy-jwt/src/lib.rs`

```rust
#![forbid(unsafe_code)]

//! Phase 22 (`envoy-jwt`): JWT (RS256) signature verification + JWKS parsing +
//! registered-claim validation, isolated behind a small leaf interface. The
//! ONLY crate that depends on `aws-lc-rs` directly (the D-3.2 permitted crypto
//! provider, reinterpreted by ADR-0055 to cover JWT signature verification).
//! HTTP-agnostic: returns a typed `JwtError` taxonomy; the `envoy-filter`
//! jwt_authn module maps each class to its Envoy-faithful 401/403 body.

mod base64url;
pub mod error;
mod jwks;
mod verify;

pub use error::JwtError;
pub use jwks::JwkSet;
pub use verify::{VerifiedJwt, verify_rs256};
```

> Note: `jwks` and `verify` modules don't exist yet — Steps 2/3 of Tasks 2 and 3 create them. To keep THIS task's build green, temporarily declare `mod jwks;`/`mod verify;` only after their files exist. For Task 1's commit, include `src/jwks.rs` and `src/verify.rs` as minimal stubs:
> ```rust
> // crates/envoy-jwt/src/jwks.rs (Task 1 stub — Task 2 fills it)
> pub struct JwkSet;
> ```
> ```rust
> // crates/envoy-jwt/src/verify.rs (Task 1 stub — Task 3 fills it)
> pub struct VerifiedJwt;
> pub fn verify_rs256() {}
> ```
> Adjust `lib.rs` `pub use` lines to match the stubs (no args) for Task 1; Tasks 2/3 replace the stubs and restore the real signatures. (Alternatively, fold Tasks 1–3 into one commit if the executor prefers — they are one crate. The plan keeps them separate for review granularity; the stub approach keeps each commit green.)

- [ ] **Step 6: Run the tests**

Run: `cargo test -p envoy-jwt`
Expected: PASS (base64url tests green; stub modules compile).

- [ ] **Step 7: Clippy + commit**

Run: `cargo clippy -p envoy-jwt --all-targets -- -D warnings` (expect clean)
```bash
git add Cargo.toml Cargo.lock crates/envoy-jwt/
git commit -m "phase 22 Task 1: envoy-jwt crate scaffold + base64url decoder"
```

---

## Task 2: `envoy-jwt` JWKS parsing (`JwkSet::parse`)

**Files:**
- Modify: `crates/envoy-jwt/src/jwks.rs` (replace the stub)

- [ ] **Step 1: Write the failing test** at the bottom of `crates/envoy-jwt/src/jwks.rs`

```rust
#[cfg(test)]
mod tests {
    use super::JwkSet;
    use crate::JwtError;

    // A minimal valid RSA JWKS (n/e are real base64url — small but well-formed JSON).
    const JWKS: &str = r#"{"keys":[{"kty":"RSA","kid":"k1","use":"sig","alg":"RS256",
        "n":"sXch4i4X...","e":"AQAB"}]}"#;

    #[test]
    fn parses_rsa_key() {
        let set = JwkSet::parse(JWKS).expect("valid jwks");
        assert_eq!(set.keys().len(), 1);
        assert_eq!(set.keys()[0].kid.as_deref(), Some("k1"));
        assert_eq!(set.keys()[0].e, vec![0x01, 0x00, 0x01]); // "AQAB" => 65537, leading zero already absent
    }

    #[test]
    fn rejects_non_json() {
        assert_eq!(JwkSet::parse("not json").unwrap_err(), JwtError::InvalidJwks);
    }

    #[test]
    fn rejects_empty_keyset() {
        assert_eq!(JwkSet::parse(r#"{"keys":[]}"#).unwrap_err(), JwtError::InvalidJwks);
    }

    #[test]
    fn skips_non_rsa_keys_but_errors_if_none_remain() {
        let only_oct = r#"{"keys":[{"kty":"oct","k":"AAAA"}]}"#;
        assert_eq!(JwkSet::parse(only_oct).unwrap_err(), JwtError::InvalidJwks);
    }
}
```

> The `n` in the literal above is a placeholder — replace it with a real base64url RSA-2048 modulus when authoring (generate via the Task 10 `gen.py`, or use `openssl`). The structural tests (`rejects_non_json`, `rejects_empty_keyset`, `skips_non_rsa`) do not need a real modulus and will pass regardless.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p envoy-jwt jwks`
Expected: FAIL (stub `JwkSet` has no `parse`/`keys`).

- [ ] **Step 3: Replace the stub** `crates/envoy-jwt/src/jwks.rs`

```rust
//! Inline JWKS (RSA-only) parsing. A JWKS is `{"keys":[{kty,kid,n,e,...}]}`;
//! we keep only `kty == "RSA"` keys, base64url-decode `n`/`e`, and strip any
//! leading 0x00 byte (aws-lc-rs's PublicKeyComponents rejects leading zeros —
//! §6.2 L1).

use serde::Deserialize;

use crate::base64url;
use crate::error::JwtError;

/// One RSA public key from a JWKS, with `n`/`e` as big-endian bytes (no leading
/// zeros).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsaKey {
    pub kid: Option<String>,
    pub n: Vec<u8>,
    pub e: Vec<u8>,
}

/// A parsed set of RSA public keys built from an inline JWKS JSON string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JwkSet {
    keys: Vec<RsaKey>,
}

#[derive(Deserialize)]
struct RawJwks {
    keys: Vec<RawJwk>,
}

#[derive(Deserialize)]
struct RawJwk {
    kty: String,
    #[serde(default)]
    kid: Option<String>,
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    e: Option<String>,
}

fn strip_leading_zeros(mut v: Vec<u8>) -> Vec<u8> {
    let first = v.iter().position(|&b| b != 0).unwrap_or(v.len());
    v.drain(..first);
    v
}

impl JwkSet {
    /// Parse an inline JWKS JSON string. Errors (`JwtError::InvalidJwks`) on
    /// non-JSON, missing `keys`, an RSA key missing `n`/`e` or with
    /// undecodable `n`/`e`, or an empty resulting RSA key set.
    pub fn parse(jwks_json: &str) -> Result<Self, JwtError> {
        let raw: RawJwks = serde_json::from_str(jwks_json).map_err(|_| JwtError::InvalidJwks)?;
        let mut keys = Vec::new();
        for k in raw.keys {
            if k.kty != "RSA" {
                continue; // ES/oct/etc. defer (§4); skip silently
            }
            let n = base64url::decode(k.n.as_deref().ok_or(JwtError::InvalidJwks)?)
                .map_err(|_| JwtError::InvalidJwks)?;
            let e = base64url::decode(k.e.as_deref().ok_or(JwtError::InvalidJwks)?)
                .map_err(|_| JwtError::InvalidJwks)?;
            keys.push(RsaKey {
                kid: k.kid,
                n: strip_leading_zeros(n),
                e: strip_leading_zeros(e),
            });
        }
        if keys.is_empty() {
            return Err(JwtError::InvalidJwks);
        }
        Ok(JwkSet { keys })
    }

    pub fn keys(&self) -> &[RsaKey] {
        &self.keys
    }
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p envoy-jwt jwks`
Expected: PASS.

- [ ] **Step 5: Clippy + commit**

Run: `cargo clippy -p envoy-jwt --all-targets -- -D warnings`
```bash
git add crates/envoy-jwt/src/jwks.rs
git commit -m "phase 22 Task 2: envoy-jwt JWKS (RSA) parsing"
```

---

## Task 3: `envoy-jwt` RS256 verify + claim validation (`verify_rs256`)

**Files:**
- Modify: `crates/envoy-jwt/src/verify.rs` (replace the stub), `crates/envoy-jwt/src/lib.rs` (restore real `pub use`)

This is a substantive review centerpiece (the crypto orchestration). Two-stage (spec-then-quality) review at state-3.

- [ ] **Step 1: Write the failing tests** at the bottom of `crates/envoy-jwt/src/verify.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::JwkSet;
    use aws_lc_rs::rsa::KeySize;
    use aws_lc_rs::signature::{KeyPair, RsaKeyPair};
    use aws_lc_rs::encoding::AsBigEndian;

    // Build a real RSA-2048 keypair, return (jwks_json, sign_fn).
    fn keypair() -> (RsaKeyPair, String) {
        let kp = RsaKeyPair::generate(KeySize::Rsa2048).expect("gen");
        let pk = kp.public_key();
        let comps: aws_lc_rs::rsa::PublicKeyComponents<Vec<u8>> =
            pk.as_be_bytes().expect("components"); // n/e big-endian
        let b64 = |b: &[u8]| {
            use base64_test::b64url; // see helper below
            b64url(b)
        };
        let jwks = format!(
            r#"{{"keys":[{{"kty":"RSA","kid":"k1","n":"{}","e":"{}"}}]}}"#,
            b64(&comps.n),
            b64(&comps.e)
        );
        (kp, jwks)
    }

    fn sign(kp: &RsaKeyPair, header_payload: &str) -> String {
        use aws_lc_rs::signature::RsaEncoding;
        let mut sig = vec![0u8; kp.public_modulus_len()];
        kp.sign(
            &aws_lc_rs::signature::RSA_PKCS1_SHA256,
            &aws_lc_rs::rand::SystemRandom::new(),
            header_payload.as_bytes(),
            &mut sig,
        )
        .expect("sign");
        base64_test::b64url(&sig)
    }

    // tiny base64url encoder for tests only
    mod base64_test {
        pub fn b64url(b: &[u8]) -> String {
            const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut out = String::new();
            for chunk in b.chunks(3) {
                let n = chunk.len();
                let b0 = chunk[0] as u32;
                let b1 = if n > 1 { chunk[1] as u32 } else { 0 };
                let b2 = if n > 2 { chunk[2] as u32 } else { 0 };
                let triple = (b0 << 16) | (b1 << 8) | b2;
                out.push(A[((triple >> 18) & 63) as usize] as char);
                out.push(A[((triple >> 12) & 63) as usize] as char);
                if n > 1 { out.push(A[((triple >> 6) & 63) as usize] as char); }
                if n > 2 { out.push(A[(triple & 63) as usize] as char); }
            }
            out
        }
        pub fn jwt(kid: &str, alg: &str, payload: &str) -> String {
            let h = format!(r#"{{"alg":"{alg}","kid":"{kid}","typ":"JWT"}}"#);
            format!("{}.{}", b64url(h.as_bytes()), b64url(payload.as_bytes()))
        }
    }

    fn make_token(kp: &RsaKeyPair, alg: &str, payload: &str) -> String {
        let hp = base64_test::jwt("k1", alg, payload);
        let sig = sign(kp, &hp);
        format!("{hp}.{sig}")
    }

    const ISS: &str = "testing@secure.istio.io";

    #[test]
    fn valid_token_verifies() {
        let (kp, jwks) = keypair();
        let set = JwkSet::parse(&jwks).unwrap();
        let tok = make_token(&kp, "RS256", r#"{"iss":"testing@secure.istio.io","aud":["a"],"exp":4102444800}"#);
        let v = verify_rs256(&tok, &set, ISS, &["a".to_string()], 1_700_000_000).unwrap();
        assert_eq!(v.iss.as_deref(), Some(ISS));
    }

    #[test]
    fn tampered_signature_fails() {
        let (kp, jwks) = keypair();
        let set = JwkSet::parse(&jwks).unwrap();
        let mut tok = make_token(&kp, "RS256", r#"{"iss":"testing@secure.istio.io","exp":4102444800}"#);
        tok.pop();
        tok.push(if tok.ends_with('A') { 'B' } else { 'A' });
        assert_eq!(verify_rs256(&tok, &set, ISS, &[], 1_700_000_000).unwrap_err(), JwtError::VerificationFails);
    }

    #[test]
    fn expired_and_nbf_and_issuer_and_audience() {
        let (kp, jwks) = keypair();
        let set = JwkSet::parse(&jwks).unwrap();
        // expired
        let t = make_token(&kp, "RS256", r#"{"iss":"testing@secure.istio.io","exp":1500000000}"#);
        assert_eq!(verify_rs256(&t, &set, ISS, &[], 1_700_000_000).unwrap_err(), JwtError::Expired);
        // not yet valid
        let t = make_token(&kp, "RS256", r#"{"iss":"testing@secure.istio.io","exp":4102444800,"nbf":4102444800}"#);
        assert_eq!(verify_rs256(&t, &set, ISS, &[], 1_700_000_000).unwrap_err(), JwtError::NotYetValid);
        // issuer mismatch
        let t = make_token(&kp, "RS256", r#"{"iss":"wrong","exp":4102444800}"#);
        assert_eq!(verify_rs256(&t, &set, ISS, &[], 1_700_000_000).unwrap_err(), JwtError::IssuerMismatch);
        // audience not allowed
        let t = make_token(&kp, "RS256", r#"{"iss":"testing@secure.istio.io","aud":"x","exp":4102444800}"#);
        assert_eq!(verify_rs256(&t, &set, ISS, &["y".to_string()], 1_700_000_000).unwrap_err(), JwtError::AudienceNotAllowed);
    }

    #[test]
    fn structural_and_alg_errors() {
        let (kp, jwks) = keypair();
        let set = JwkSet::parse(&jwks).unwrap();
        assert_eq!(verify_rs256("abc", &set, ISS, &[], 0).unwrap_err(), JwtError::NotInForm);
        assert_eq!(verify_rs256("a.b", &set, ISS, &[], 0).unwrap_err(), JwtError::NotInForm);
        // non-RS256 alg => NoMatchingKey (Envoy folds unsupported alg into key-match)
        let t = make_token(&kp, "HS256", r#"{"iss":"testing@secure.istio.io"}"#);
        assert_eq!(verify_rs256(&t, &set, ISS, &[], 1_700_000_000).unwrap_err(), JwtError::NoMatchingKey);
    }
}
```

> The test helper signs with `aws_lc_rs::signature::RsaKeyPair` and extracts public components via `public_key().as_be_bytes()` (the `AsBigEndian<PublicKeyComponents<Vec<u8>>>` impl). If the exact 1.16.3 method names differ at authoring time (`as_be_bytes` vs `as_big_endian` vs `public_modulus`/`public_exponent`), adjust the helper — the production code in Step 3 only consumes `JwkSet` + `PublicKeyComponents::verify`, which Task L1 confirmed. Note the PROGRESS Task-1 preamble flags this as the one signing-API detail to confirm at authoring time.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p envoy-jwt verify`
Expected: FAIL (stub has no real `verify_rs256`/`VerifiedJwt`).

- [ ] **Step 3: Replace the stub** `crates/envoy-jwt/src/verify.rs`

```rust
//! RS256 verification + registered-claim validation. Time is INJECTED
//! (`now_unix`) so callers control the clock (runtime passes `SystemTime::now`;
//! tests pass fixed values; the differential fixture uses far-future/past `exp`
//! so the clock is cross-proxy-irrelevant). Pure compute, no I/O, no async.

use aws_lc_rs::rsa::PublicKeyComponents;
use aws_lc_rs::signature::RSA_PKCS1_2048_8192_SHA256;
use serde::Deserialize;

use crate::base64url;
use crate::error::JwtError;
use crate::jwks::JwkSet;

/// The verified + validated JWT a caller may inspect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedJwt {
    pub iss: Option<String>,
    pub aud: Vec<String>,
    pub exp: Option<i64>,
    pub nbf: Option<i64>,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    #[serde(default)]
    kid: Option<String>,
}

#[derive(Deserialize)]
struct Claims {
    #[serde(default)]
    iss: Option<String>,
    #[serde(default)]
    aud: Option<Aud>,
    #[serde(default)]
    exp: Option<i64>,
    #[serde(default)]
    nbf: Option<i64>,
}

/// `aud` is `string | string[]` per JWT (§6.2 L7).
#[derive(Deserialize)]
#[serde(untagged)]
enum Aud {
    One(String),
    Many(Vec<String>),
}

impl Aud {
    fn into_vec(self) -> Vec<String> {
        match self {
            Aud::One(s) => vec![s],
            Aud::Many(v) => v,
        }
    }
}

/// Verify an RS256 JWT against `jwks` and validate `iss`/`aud`/`exp`/`nbf`.
/// `allowed_audiences` empty ⇒ the audience check is skipped (§6.2 L7).
pub fn verify_rs256(
    token: &str,
    jwks: &JwkSet,
    expected_issuer: &str,
    allowed_audiences: &[String],
    now_unix: i64,
) -> Result<VerifiedJwt, JwtError> {
    // 1. Exactly 3 non-empty segments.
    let mut it = token.split('.');
    let (h, p, s) = match (it.next(), it.next(), it.next(), it.next()) {
        (Some(h), Some(p), Some(s), None) if !h.is_empty() && !p.is_empty() && !s.is_empty() => {
            (h, p, s)
        }
        _ => return Err(JwtError::NotInForm),
    };
    // 2. Decode + parse header.
    let header_bytes = base64url::decode(h).map_err(|_| JwtError::NotInForm)?;
    let header: Header = serde_json::from_slice(&header_bytes).map_err(|_| JwtError::BadHeaderJson)?;
    // 3. Decode + parse payload.
    let payload_bytes = base64url::decode(p).map_err(|_| JwtError::NotInForm)?;
    let claims: Claims =
        serde_json::from_slice(&payload_bytes).map_err(|_| JwtError::BadPayloadJson)?;
    // 4. Decode signature.
    let sig = base64url::decode(s).map_err(|_| JwtError::NotInForm)?;
    // 5. alg must be RS256 (else NoMatchingKey — Envoy folds unsupported alg here).
    if header.alg != "RS256" {
        return Err(JwtError::NoMatchingKey);
    }
    // 6. Candidate keys: by kid if the header names one, else all keys.
    let candidates: Vec<&crate::jwks::RsaKey> = match &header.kid {
        Some(kid) => jwks
            .keys()
            .iter()
            .filter(|k| k.kid.as_deref() == Some(kid.as_str()))
            .collect(),
        None => jwks.keys().iter().collect(),
    };
    if candidates.is_empty() {
        return Err(JwtError::NoMatchingKey);
    }
    // 7. Verify the signature over the `header.payload` signing input.
    let signing_input = &token.as_bytes()[..h.len() + 1 + p.len()];
    let verified = candidates.iter().any(|k| {
        PublicKeyComponents {
            n: k.n.as_slice(),
            e: k.e.as_slice(),
        }
        .verify(&RSA_PKCS1_2048_8192_SHA256, signing_input, &sig)
        .is_ok()
    });
    if !verified {
        return Err(JwtError::VerificationFails);
    }
    // 8. Issuer.
    if claims.iss.as_deref() != Some(expected_issuer) {
        return Err(JwtError::IssuerMismatch);
    }
    // 9. exp / nbf (injected clock).
    if let Some(exp) = claims.exp
        && now_unix >= exp
    {
        return Err(JwtError::Expired);
    }
    if let Some(nbf) = claims.nbf
        && now_unix < nbf
    {
        return Err(JwtError::NotYetValid);
    }
    // 10. Audience.
    let aud: Vec<String> = claims.aud.map(Aud::into_vec).unwrap_or_default();
    if !allowed_audiences.is_empty() {
        let ok = aud.iter().any(|a| allowed_audiences.iter().any(|x| x == a));
        if !ok {
            return Err(JwtError::AudienceNotAllowed);
        }
    }
    Ok(VerifiedJwt {
        iss: claims.iss,
        aud,
        exp: claims.exp,
        nbf: claims.nbf,
    })
}
```

> The `let Some(x) = ... && cond` let-chain syntax is stable on Rust 1.95.0. If the executor's clippy flags it, rewrite as nested `if let { if cond { } }`.

- [ ] **Step 4: Restore real `pub use` in `crates/envoy-jwt/src/lib.rs`** (already correct from Task 1 if you wrote the real signatures; ensure `pub use verify::{VerifiedJwt, verify_rs256};` matches).

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p envoy-jwt`
Expected: PASS (all base64url + jwks + verify tests).

- [ ] **Step 6: Clippy + commit**

Run: `cargo clippy -p envoy-jwt --all-targets -- -D warnings`
```bash
git add crates/envoy-jwt/src/verify.rs crates/envoy-jwt/src/lib.rs Cargo.lock
git commit -m "phase 22 Task 3: envoy-jwt RS256 verify + iss/aud/exp/nbf validation"
```

---

## Task 4: `envoy-jwt` fuzz target (§7.4)

**Files:**
- Create: `crates/envoy-jwt/fuzz/Cargo.toml`, `crates/envoy-jwt/fuzz/fuzz_targets/jwt_parse.rs`, `crates/envoy-jwt/fuzz/.gitignore`, `crates/envoy-jwt/fuzz/corpus/jwt_parse/{jwks.json,token.txt,empty}`
- Modify: `Cargo.toml` (add `"crates/envoy-jwt/fuzz"` to `members`)

- [ ] **Step 1: Create the fuzz manifest** `crates/envoy-jwt/fuzz/Cargo.toml` (mirror `crates/envoy-config/fuzz/Cargo.toml`)

```toml
[package]
name = "envoy-jwt-fuzz"
version = "0.0.0"
publish = false
edition = "2024"
license = "Apache-2.0"

[package.metadata]
cargo-fuzz = true

[dependencies]
libfuzzer-sys = "0.4"
envoy-jwt = { path = ".." }

[[bin]]
name = "jwt_parse"
path = "fuzz_targets/jwt_parse.rs"
test = false
doc = false
bench = false
```

- [ ] **Step 2: Create the fuzz target** `crates/envoy-jwt/fuzz/fuzz_targets/jwt_parse.rs`

```rust
#![no_main]
#![forbid(unsafe_code)]

use libfuzzer_sys::fuzz_target;

// Split the input on the first NUL: bytes before = JWKS JSON, after = a token.
// Both surfaces (JwkSet::parse and verify_rs256) must never panic — only return
// a JwtError on malformed input.
fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else { return };
    let (jwks_str, token) = match s.split_once('\u{0}') {
        Some((a, b)) => (a, b),
        None => (s, ""),
    };
    if let Ok(set) = envoy_jwt::JwkSet::parse(jwks_str) {
        let _ = envoy_jwt::verify_rs256(token, &set, "iss", &[], 0);
    }
});
```

- [ ] **Step 3: Seed corpus + `.gitignore`** — create `crates/envoy-jwt/fuzz/.gitignore`:

```
target/
artifacts/
corpus/jwt_parse/*
!corpus/jwt_parse/jwks.json
!corpus/jwt_parse/token.txt
!corpus/jwt_parse/empty
```

Create the 3 seed files: `corpus/jwt_parse/empty` (0 bytes); `corpus/jwt_parse/jwks.json` (a real RSA JWKS — copy the Task 10 JWKS, then a `\0`, then a valid token); `corpus/jwt_parse/token.txt` (a `\0`-prefixed garbage token to exercise the verify path with no JWKS).

- [ ] **Step 4: Add the workspace member** — `Cargo.toml` `members`: add `"crates/envoy-jwt/fuzz",` after `"crates/envoy-config/fuzz",`.

- [ ] **Step 5: Build the fuzz target + short run**

Run: `cargo +nightly fuzz build jwt_parse` (in `crates/envoy-jwt/fuzz`), then a smoke run: `cargo +nightly fuzz run jwt_parse -- -runs=50000 -max_total_time=30`
Expected: builds; no crash. (CI runs the short-budget version at state-4; the nightly toolchain is required only for `cargo fuzz`, matching the `envoy-config/fuzz` precedent.)

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/envoy-jwt/fuzz/
git commit -m "phase 22 Task 4: envoy-jwt fuzz target (JWKS/JWT parse + verify)"
```

---

## Task 5: `envoy-config` schema — `JwtAuthnConfig` + `HttpFilterTypedConfig::JwtAuthn`

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (config structs + enum variant), `crates/envoy-config/src/lib.rs` (re-exports)

- [ ] **Step 1: Write the failing test** in `crates/envoy-config/src/bootstrap.rs` `#[cfg(test)]` module (near the other HTTP-filter parse tests)

```rust
#[test]
fn parses_jwt_authn_filter_config() {
    let yaml = r#"
name: envoy.filters.http.jwt_authn
typed_config:
  "@type": type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication
  providers:
    provider1:
      issuer: "testing@secure.istio.io"
      audiences: ["aud1"]
      local_jwks:
        inline_string: '{"keys":[]}'
      forward: true
  rules:
    - match: { prefix: "/" }
      requires: { provider_name: "provider1" }
"#;
    let hf: crate::HttpFilter = serde_yaml::from_str(yaml).expect("parses");
    match hf.typed_config {
        crate::HttpFilterTypedConfig::JwtAuthn(cfg) => {
            assert_eq!(cfg.providers.len(), 1);
            let p = &cfg.providers["provider1"];
            assert_eq!(p.issuer, "testing@secure.istio.io");
            assert_eq!(p.audiences, vec!["aud1".to_string()]);
            assert!(p.forward);
            assert_eq!(cfg.rules.len(), 1);
            assert_eq!(cfg.rules[0].requires.provider_name, "provider1");
        }
        other => panic!("expected JwtAuthn, got {other:?}"),
    }
}

#[test]
fn jwt_provider_forward_defaults_false() {
    let yaml = r#"
issuer: "i"
local_jwks: { inline_string: "{}" }
"#;
    let p: crate::JwtProvider = serde_yaml::from_str(yaml).expect("parses");
    assert!(!p.forward, "forward defaults false (§6.2 L6 — strip Authorization)");
    assert!(p.audiences.is_empty());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p envoy-config parses_jwt_authn`
Expected: FAIL (no `JwtAuthn` variant / `JwtProvider` type).

- [ ] **Step 3: Add the config structs** in `crates/envoy-config/src/bootstrap.rs` (after `FaultConfig`/`FaultAbort`, before `HttpFilterTypedConfig` at line 690). `DataSource` (line 556) + `RouteMatch` (line 1386) are REUSED.

```rust
/// `envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication` config
/// (phase 22, minimum-viable per ADR-0055/SPEC §3). RS256, inline `local_jwks`,
/// a single `provider_name` per matched rule, default `Authorization: Bearer`
/// extraction, `iss`/`aud`/`exp`/`nbf` validation. Deferred fields
/// (`requirement_map`, `bypass_cors_preflight`, `strip_failure_response`, …)
/// are rejected by `deny_unknown_fields`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JwtAuthnConfig {
    pub providers: std::collections::BTreeMap<String, JwtProvider>,
    #[serde(default)]
    pub rules: Vec<RequirementRule>,
}

/// One JWT provider. `local_jwks` reuses the existing `DataSource`
/// (inline_string only at phase-22 scope — the validator enforces it).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JwtProvider {
    pub issuer: String,
    #[serde(default)]
    pub audiences: Vec<String>,
    pub local_jwks: DataSource,
    /// Envoy default `false` ⇒ strip `Authorization` upstream on success
    /// (§6.2 L6). Deferred: remote_jwks, from_headers/params/cookies,
    /// forward_payload_header, payload_in_metadata, claim_to_headers,
    /// clock_skew_seconds, jwks_cache_duration, jwt_cache_config.
    #[serde(default)]
    pub forward: bool,
}

/// One `{ match: RouteMatch, requires: JwtRequirement }` rule. `match` reuses
/// the 04.x `RouteMatch`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RequirementRule {
    pub r#match: RouteMatch,
    pub requires: JwtRequirement,
}

/// Minimum-viable single-provider requirement. Deferred: `requires_any`,
/// `requires_all`, `allow_missing`, `allow_missing_or_failed`, named
/// requirements (rejected by `deny_unknown_fields`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JwtRequirement {
    pub provider_name: String,
}
```

- [ ] **Step 4: Add the enum variant** in `HttpFilterTypedConfig` (after the `Fault` variant, line 710):

```rust
    #[serde(
        rename = "type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication"
    )]
    JwtAuthn(JwtAuthnConfig),
```

- [ ] **Step 5: Re-export the new types** in `crates/envoy-config/src/lib.rs` (alongside the other `pub use bootstrap::{... FaultConfig ...}` re-exports — find the existing filter-config re-export line and append `JwtAuthnConfig, JwtProvider, RequirementRule, JwtRequirement`).

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test -p envoy-config parses_jwt_authn jwt_provider_forward`
Expected: PASS.

- [ ] **Step 7: Clippy + commit**

Run: `cargo clippy -p envoy-config --all-targets -- -D warnings`
```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 22 Task 5: envoy-config JwtAuthnConfig schema + HttpFilterTypedConfig::JwtAuthn"
```

---

## Task 6: `envoy-config` validator — `validate_jwt_authn_config` + arm + ConfigError

**Files:**
- Modify: `crates/envoy-config/Cargo.toml` (path-dep on `envoy-jwt`), `crates/envoy-config/src/bootstrap.rs` (validator + arm), `crates/envoy-config/src/lib.rs` (ConfigError variants)

- [ ] **Step 1: Add the path-dep** in `crates/envoy-config/Cargo.toml` `[dependencies]`:

```toml
envoy-jwt = { path = "../envoy-jwt" }
```

- [ ] **Step 2: Add ConfigError variants** in `crates/envoy-config/src/lib.rs` `ConfigError` enum (after the existing HTTP-filter-related variants):

```rust
    #[error("jwt_authn filter on listener `{listener}` has no providers; at least one is required")]
    JwtAuthnNoProviders { listener: String },
    #[error("jwt_authn rule on listener `{listener}` references unknown provider `{provider_name}`")]
    JwtAuthnUnknownProvider { listener: String, provider_name: String },
    #[error("jwt_authn provider `{provider}` on listener `{listener}` has an invalid or non-inline local_jwks")]
    JwtAuthnInvalidJwks { listener: String, provider: String },
```

- [ ] **Step 3: Write the failing test** in `crates/envoy-config/src/bootstrap.rs` `#[cfg(test)]`

```rust
#[test]
fn jwt_authn_validator_rejects_empty_providers() {
    let cfg = crate::JwtAuthnConfig {
        providers: std::collections::BTreeMap::new(),
        rules: vec![],
    };
    let err = validate_jwt_authn_config(&cfg, "l0").unwrap_err();
    assert!(matches!(err, crate::ConfigError::JwtAuthnNoProviders { .. }));
}

#[test]
fn jwt_authn_validator_rejects_dangling_provider_ref() {
    let mut providers = std::collections::BTreeMap::new();
    providers.insert("p1".to_string(), crate::JwtProvider {
        issuer: "i".to_string(),
        audiences: vec![],
        local_jwks: crate::DataSource { filename: None, inline_string: Some(VALID_JWKS.to_string()) },
        forward: false,
    });
    let cfg = crate::JwtAuthnConfig {
        providers,
        rules: vec![crate::RequirementRule {
            r#match: crate::RouteMatch { prefix: Some("/".to_string()), path: None, headers: vec![] },
            requires: crate::JwtRequirement { provider_name: "nope".to_string() },
        }],
    };
    assert!(matches!(validate_jwt_authn_config(&cfg, "l0").unwrap_err(),
        crate::ConfigError::JwtAuthnUnknownProvider { .. }));
}

#[test]
fn jwt_authn_validator_rejects_bad_jwks() {
    let mut providers = std::collections::BTreeMap::new();
    providers.insert("p1".to_string(), crate::JwtProvider {
        issuer: "i".to_string(),
        audiences: vec![],
        local_jwks: crate::DataSource { filename: None, inline_string: Some("not json".to_string()) },
        forward: false,
    });
    let cfg = crate::JwtAuthnConfig { providers, rules: vec![] };
    assert!(matches!(validate_jwt_authn_config(&cfg, "l0").unwrap_err(),
        crate::ConfigError::JwtAuthnInvalidJwks { .. }));
}

#[test]
fn jwt_authn_validator_accepts_valid() {
    let mut providers = std::collections::BTreeMap::new();
    providers.insert("p1".to_string(), crate::JwtProvider {
        issuer: "i".to_string(),
        audiences: vec![],
        local_jwks: crate::DataSource { filename: None, inline_string: Some(VALID_JWKS.to_string()) },
        forward: false,
    });
    let cfg = crate::JwtAuthnConfig {
        providers,
        rules: vec![crate::RequirementRule {
            r#match: crate::RouteMatch { prefix: Some("/".to_string()), path: None, headers: vec![] },
            requires: crate::JwtRequirement { provider_name: "p1".to_string() },
        }],
    };
    assert!(validate_jwt_authn_config(&cfg, "l0").is_ok());
}
```

> `VALID_JWKS` is a `const &str` real RSA-2048 JWKS — add it near the test module (reuse the Task 10 JWKS). Define once at the top of the test module.

- [ ] **Step 4: Run to verify it fails**

Run: `cargo test -p envoy-config jwt_authn_validator`
Expected: FAIL (no `validate_jwt_authn_config`).

- [ ] **Step 5: Implement the validator** in `crates/envoy-config/src/bootstrap.rs` (near `validate_fault_config`)

```rust
/// Validate a jwt_authn filter config (phase 22, minimum-viable). All errors
/// are config-load-time fatal (ADR-0049 all-fatal posture).
pub(crate) fn validate_jwt_authn_config(
    cfg: &crate::JwtAuthnConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if cfg.providers.is_empty() {
        return Err(crate::ConfigError::JwtAuthnNoProviders {
            listener: listener_name.to_string(),
        });
    }
    for (name, provider) in &cfg.providers {
        let jwks = provider.local_jwks.inline_string.as_deref().ok_or_else(|| {
            crate::ConfigError::JwtAuthnInvalidJwks {
                listener: listener_name.to_string(),
                provider: name.clone(),
            }
        })?;
        envoy_jwt::JwkSet::parse(jwks).map_err(|_| crate::ConfigError::JwtAuthnInvalidJwks {
            listener: listener_name.to_string(),
            provider: name.clone(),
        })?;
    }
    for rule in &cfg.rules {
        if !cfg.providers.contains_key(&rule.requires.provider_name) {
            return Err(crate::ConfigError::JwtAuthnUnknownProvider {
                listener: listener_name.to_string(),
                provider_name: rule.requires.provider_name.clone(),
            });
        }
        // RouteMatch structural validity (exactly one of prefix/path) reuses the
        // existing route-match validation.
        validate_route_match(&rule.r#match, listener_name)?;
    }
    Ok(())
}
```

> `validate_route_match` is the existing 04.x route-match validator. Grep for the function that enforces "exactly one of `prefix`/`path`" (it returns `ConfigError::UnsupportedRouteMatcher`). If it is named differently or not separately callable, inline the same check: `match (m.prefix.is_some(), m.path.is_some()) { (true,false)|(false,true) => Ok(()), _ => Err(ConfigError::UnsupportedRouteMatcher { matcher: "jwt_authn rule" }) }`. Flag the exact name in the PROGRESS Task-6 entry.

- [ ] **Step 6: Add the validator arm** in `validate_http_filters` (after the `Fault` arm, line 2684):

```rust
            crate::HttpFilterTypedConfig::JwtAuthn(cfg) => {
                if f.name != "envoy.filters.http.jwt_authn" {
                    return Err(crate::ConfigError::UnsupportedHttpFilter {
                        name: f.name.clone(),
                    });
                }
                validate_jwt_authn_config(cfg, listener_name)?;
            }
```

- [ ] **Step 7: Run to verify it passes**

Run: `cargo test -p envoy-config jwt_authn`
Expected: PASS.

- [ ] **Step 8: Clippy + commit**

Run: `cargo clippy -p envoy-config --all-targets -- -D warnings`
```bash
git add crates/envoy-config/Cargo.toml crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs Cargo.lock
git commit -m "phase 22 Task 6: envoy-config jwt_authn validator (providers/refs/JWKS) + validate_http_filters arm"
```

---

## Task 7: `envoy-filter::JwtAuthnFilter` runtime + stats + JwtError→wire map

**Files:**
- Modify: `crates/envoy-filter/Cargo.toml` (path-dep on `envoy-jwt`), `crates/envoy-filter/src/lib.rs` (module + re-export)
- Create: `crates/envoy-filter/src/jwt_authn.rs`
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (jwt_authn rows)

This is a substantive review centerpiece (the wire mapping + stats + Authorization strip). Two-stage review at state-3.

- [ ] **Step 1: Add the path-dep** in `crates/envoy-filter/Cargo.toml` `[dependencies]`:

```toml
envoy-jwt = { path = "../envoy-jwt" }
```

- [ ] **Step 2: Write the failing tests** at the bottom of the new `crates/envoy-filter/src/jwt_authn.rs` (mirror `fault.rs`'s test harness; build a real RSA keypair + signed tokens — reuse the Task 3 test helper pattern, or boot from a committed test JWKS + tokens via a small `#[cfg(test)]` helper module). Cover:
  - valid token (matched rule) → `Decision::Continue`; `allowed` incremented; `authorization` header REMOVED when `forward=false`.
  - missing token → `StopAndSend{status:401, body:b"Jwt is missing"}`, `www-authenticate: Bearer realm="http://envoy.test/"` (no `error=`); `denied` incremented.
  - tampered → 401 `Jwt verification fails`, `www-authenticate` has `, error="invalid_token"`.
  - expired → 401 `Jwt is expired`.
  - wrong audience → **403** `Audiences in Jwt are not allowed`.
  - no-matching-rule (path not covered) → `Decision::Continue`; `allowed` incremented.
  - `forward=true` keeps the `authorization` header on success.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use envoy_stats::StatsRegistry;
    use std::sync::Arc;
    // ... build_cfg(jwks, issuer, audiences, forward, rule_prefix) helper that
    //     returns an envoy_config::JwtAuthnConfig; req(headers, path) helper.

    #[test]
    fn missing_token_401_with_realm_only_www_authenticate() {
        let registry = Arc::new(StatsRegistry::new());
        let mut f = JwtAuthnFilter::build_from_config(&cfg_one_provider(), &registry, "ingress_http").unwrap();
        let mut r = req(vec![("host".into(), "envoy.test".into())], "/");
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 401);
                assert_eq!(resp.reason, Some("Unauthorized"));
                assert_eq!(resp.body.as_ref(), b"Jwt is missing");
                let wa = resp.headers.iter().find(|(k, _)| k == "www-authenticate").unwrap();
                assert_eq!(wa.1, r#"Bearer realm="http://envoy.test/""#);
            }
            Decision::Continue => panic!("expected 401"),
        }
        assert_eq!(registry.register_counter("http.ingress_http.jwt_authn.denied").unwrap().value(), 1);
    }
    // ... remaining tests per the bullet list above ...
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p envoy-filter jwt_authn`
Expected: FAIL (module/type absent).

- [ ] **Step 4: Implement the filter** `crates/envoy-filter/src/jwt_authn.rs`

```rust
//! The `envoy.filters.http.jwt_authn` runtime filter (phase 22, minimum-viable).
//!
//! Decode-side authentication gate: selects the first `rules[]` entry whose
//! `RouteMatch` matches the request, extracts the JWT from `Authorization:
//! Bearer`, verifies RS256 against the rule's provider JWKS, and validates
//! `iss`/`aud`/`exp`/`nbf` (`envoy-jwt`). On success: `Decision::Continue`,
//! `allowed.inc()`, and (when the provider's `forward` is false, the default)
//! the `Authorization` header is stripped (§6.2 L6). On failure: `denied.inc()`
//! and a `Decision::StopAndSend` 401/403 with the Envoy-faithful body + a
//! `www-authenticate` header. A request matching NO rule is allowed (§6.2 L4).
//! The standard response headers are decorated by the existing HCM filter-synth
//! helpers (H1 `decorate_filter_synth_response`; H2
//! `decorate_filter_synth_response_h2`) — unchanged.

use std::sync::Arc;

use bytes::Bytes;
use envoy_jwt::{JwkSet, JwtError};
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

#[derive(Debug, Clone)]
struct CompiledProvider {
    issuer: String,
    audiences: Vec<String>,
    jwks: Arc<JwkSet>,
    forward: bool,
}

#[derive(Debug, Clone)]
struct CompiledRule {
    r#match: envoy_config::RouteMatch,
    provider: Arc<CompiledProvider>,
}

#[derive(Debug, Clone)]
pub struct JwtAuthnFilter {
    rules: Arc<Vec<CompiledRule>>,
    allowed: Arc<Counter>,
    denied: Arc<Counter>,
}

impl JwtAuthnFilter {
    pub(crate) fn build_from_config(
        cfg: &envoy_config::JwtAuthnConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        // Compile providers once (parse each JWKS — the validator already
        // proved they parse, but we keep the parsed handle here).
        let mut compiled: std::collections::BTreeMap<String, Arc<CompiledProvider>> =
            std::collections::BTreeMap::new();
        for (name, p) in &cfg.providers {
            let jwks_json = p.local_jwks.inline_string.as_deref().ok_or_else(|| {
                FilterError::InvalidConfig {
                    message: format!("jwt_authn provider {name}: local_jwks not inline"),
                }
            })?;
            let jwks = JwkSet::parse(jwks_json).map_err(|_| FilterError::InvalidConfig {
                message: format!("jwt_authn provider {name}: invalid JWKS"),
            })?;
            compiled.insert(
                name.clone(),
                Arc::new(CompiledProvider {
                    issuer: p.issuer.clone(),
                    audiences: p.audiences.clone(),
                    jwks: Arc::new(jwks),
                    forward: p.forward,
                }),
            );
        }
        let mut rules = Vec::with_capacity(cfg.rules.len());
        for r in &cfg.rules {
            let provider = compiled
                .get(&r.requires.provider_name)
                .ok_or_else(|| FilterError::InvalidConfig {
                    message: format!("jwt_authn rule references unknown provider {}", r.requires.provider_name),
                })?
                .clone();
            rules.push(CompiledRule {
                r#match: r.r#match.clone(),
                provider,
            });
        }
        let reg = |suffix: &str| {
            registry
                .register_counter(&format!("http.{hcm_stat_prefix}.jwt_authn.{suffix}"))
                .map_err(|e| FilterError::InvalidConfig {
                    message: format!("StatsRegistry: {e}"),
                })
        };
        Ok(Self {
            rules: Arc::new(rules),
            allowed: reg("allowed")?,
            denied: reg("denied")?,
        })
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        // 1. First matching rule. No match ⇒ allow (§6.2 L4).
        let Some(rule) = self
            .rules
            .iter()
            .find(|r| route_match_matches(&r.r#match, &req.path, &req.headers))
        else {
            self.allowed.inc();
            return Decision::Continue;
        };
        let provider = rule.provider.clone();

        // 2. Extract `Authorization: Bearer <token>`. Non-Bearer / missing ⇒ Missing (§6.2 L2).
        let token = bearer_token(&req.headers);
        let realm = realm(&req.path, &req.headers);

        let Some(token) = token else {
            self.denied.inc();
            return missing_reply(&realm);
        };

        // 3. Verify (real clock; far-future/past exp makes it cross-proxy-irrelevant).
        let now = now_unix();
        match envoy_jwt::verify_rs256(token, &provider.jwks, &provider.issuer, &provider.audiences, now) {
            Ok(_) => {
                self.allowed.inc();
                if !provider.forward {
                    strip_authorization(&mut req.headers);
                }
                Decision::Continue
            }
            Err(e) => {
                self.denied.inc();
                error_reply(&e, &realm)
            }
        }
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue // decode-only gate
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn header_ci<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn bearer_token(headers: &[(String, String)]) -> Option<&str> {
    header_ci(headers, "authorization").and_then(|v| v.strip_prefix("Bearer "))
}

fn strip_authorization(headers: &mut Vec<(String, String)>) {
    headers.retain(|(k, _)| !k.eq_ignore_ascii_case("authorization"));
}

fn realm(path: &str, headers: &[(String, String)]) -> String {
    let host = header_ci(headers, "host").unwrap_or("");
    format!("http://{host}{path}")
}

/// First-matching-rule path+header evaluation, mirroring the HCM
/// `route_matches` (prefix XOR path, AND-combined header matchers).
fn route_match_matches(m: &envoy_config::RouteMatch, path: &str, headers: &[(String, String)]) -> bool {
    let path_ok = match (&m.prefix, &m.path) {
        (Some(p), None) => path.starts_with(p),
        (None, Some(p)) => path == p,
        _ => false,
    };
    path_ok && m.headers.iter().all(|hm| hm.matches(headers))
}

fn www_authenticate(realm: &str, with_error: bool) -> (String, String) {
    let v = if with_error {
        format!(r#"Bearer realm="{realm}", error="invalid_token""#)
    } else {
        format!(r#"Bearer realm="{realm}""#)
    };
    ("www-authenticate".to_string(), v)
}

fn missing_reply(realm: &str) -> Decision {
    Decision::StopAndSend(FilterResponse {
        status: 401,
        reason: Some("Unauthorized"),
        headers: vec![www_authenticate(realm, false)],
        body: Bytes::from_static(b"Jwt is missing"),
    })
}

/// Map a `JwtError` to its Envoy-faithful (status, body) + `www-authenticate`
/// (all non-missing classes carry `error="invalid_token"`). Bytes verified at
/// §6.2 L2.
fn error_reply(e: &JwtError, realm: &str) -> Decision {
    let (status, body): (u16, &'static [u8]) = match e {
        JwtError::NotInForm => (401, b"Jwt is not in the form of Header.Payload.Signature with two dots and 3 sections"),
        JwtError::BadHeaderJson => (401, b"Jwt header is an invalid JSON"),
        JwtError::BadPayloadJson => (401, b"Jwt payload is an invalid JSON"),
        JwtError::NoMatchingKey => (401, b"Jwks doesn't have key to match kid or alg from Jwt"),
        JwtError::VerificationFails => (401, b"Jwt verification fails"),
        JwtError::IssuerMismatch => (401, b"Jwt issuer is not configured"),
        JwtError::Expired => (401, b"Jwt is expired"),
        JwtError::NotYetValid => (401, b"Jwt not yet valid"),
        JwtError::AudienceNotAllowed => (403, b"Audiences in Jwt are not allowed"),
        // InvalidJwks is a config-load error and never reaches the data path.
        JwtError::InvalidJwks => (401, b"Jwt verification fails"),
    };
    let reason = if status == 403 { "Forbidden" } else { "Unauthorized" };
    Decision::StopAndSend(FilterResponse {
        status,
        reason: Some(reason),
        headers: vec![www_authenticate(realm, true)],
        body: Bytes::from_static(body),
    })
}
```

- [ ] **Step 5: Wire the module + re-export** in `crates/envoy-filter/src/lib.rs` — add `mod jwt_authn;` and `pub use jwt_authn::JwtAuthnFilter;` (mirror the `fault` module lines).

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test -p envoy-filter jwt_authn`
Expected: PASS.

- [ ] **Step 7: Extend BEHAVIOR_CONTRACT.md** — append a `**22 entries (jwt_authn filter):**` block after the `**20 entries (file-based RDS):**` section (and its paragraphs). Include: (a) the 2 stat rows `http.<hcm_stat_prefix>.jwt_authn.{allowed,denied}` (value-exact; `allowed` counts success + no-rule pass-through; `denied` counts each 401/403; the 5 Envoy-only siblings unasserted, per §6.2 L5); (b) a "Response body — jwt_authn 401/403 local replies" subsection with the full L2 table (each class → status + byte-exact body + len + `www-authenticate` form); (c) a Header allow-list note that `www-authenticate` is value-exact because the realm is reproduced as `http://<Host><path>` and the differential fixture drives a fixed Host (§6.2 L3). Reference ADR-0055 + ADR-0056.

- [ ] **Step 8: Clippy + commit**

Run: `cargo clippy -p envoy-filter --all-targets -- -D warnings`
```bash
git add crates/envoy-filter/Cargo.toml crates/envoy-filter/src/jwt_authn.rs crates/envoy-filter/src/lib.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md Cargo.lock
git commit -m "phase 22 Task 7: envoy-filter JwtAuthnFilter (rule select, RS256 verify, 401/403 wire map, stats) + BEHAVIOR_CONTRACT"
```

---

## Task 8: `HttpFilterInstance::JwtAuthn` variant + dispatch

**Files:**
- Modify: `crates/envoy-filter/src/instance.rs`

- [ ] **Step 1: Write the failing test** in `crates/envoy-filter/src/instance.rs` `#[cfg(test)]` (mirror the existing per-variant build tests)

```rust
#[test]
fn builds_jwt_authn_instance_and_dispatches() {
    let registry = std::sync::Arc::new(StatsRegistry::new());
    let hf = envoy_config::HttpFilter {
        name: "envoy.filters.http.jwt_authn".to_string(),
        typed_config: envoy_config::HttpFilterTypedConfig::JwtAuthn(jwt_authn_cfg_for_test()),
    };
    let mut inst = HttpFilterInstance::build(&hf, &registry, "ingress_http").unwrap();
    assert!(matches!(inst, HttpFilterInstance::JwtAuthn(_)));
    // missing token → StopAndSend 401
    let mut req = FilterRequest { method: "GET".into(), path: "/".into(),
        headers: vec![("host".into(), "envoy.test".into())], body: None };
    assert!(matches!(inst.decode_headers(&mut req), Decision::StopAndSend(r) if r.status == 401));
    // encode is no-op
    let mut resp = FilterResponse { status: 200, reason: None, headers: vec![], body: bytes::Bytes::new() };
    assert!(matches!(inst.encode_headers(&mut resp), Decision::Continue));
}
```

> `jwt_authn_cfg_for_test()` builds a one-provider `JwtAuthnConfig` with a real inline JWKS and a `prefix: "/"` rule. Define it in the test module.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p envoy-filter builds_jwt_authn`
Expected: FAIL (no `JwtAuthn` variant).

- [ ] **Step 3: Add the variant + 3 dispatch arms** in `crates/envoy-filter/src/instance.rs`:
  - Enum (after `Fault(FaultFilter),`, line 43): `JwtAuthn(JwtAuthnFilter),` (import `use crate::jwt_authn::JwtAuthnFilter;` at the top).
  - `build` (after the `Fault` arm): 
    ```rust
    envoy_config::HttpFilterTypedConfig::JwtAuthn(cfg) => Ok(HttpFilterInstance::JwtAuthn(
        JwtAuthnFilter::build_from_config(cfg, registry, hcm_stat_prefix)?,
    )),
    ```
  - `decode_headers` (after the `Fault` arm): `HttpFilterInstance::JwtAuthn(f) => f.decode_headers(req),`
  - `encode_headers` (after the `Fault` arm): `HttpFilterInstance::JwtAuthn(f) => f.encode_headers(resp_arg),`

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p envoy-filter`
Expected: PASS.

- [ ] **Step 5: Clippy + commit**

Run: `cargo clippy -p envoy-filter --all-targets -- -D warnings`
```bash
git add crates/envoy-filter/src/instance.rs
git commit -m "phase 22 Task 8: HttpFilterInstance::JwtAuthn variant + build/decode/encode dispatch"
```

---

## Task 9: `parse_bootstrap` fuzz seed

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_jwt_authn_filter.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore`, `crates/envoy-config/src/bootstrap.rs` (`fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS list)

- [ ] **Step 1: Create the seed** `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_jwt_authn_filter.yaml` — a full minimal bootstrap with an H1 HCM whose `http_filters` is `[jwt_authn, router]` (one provider, inline JWKS, one `prefix: "/"` rule). Use a real RSA-2048 JWKS inline (the Task 10 JWKS). This must PARSE successfully.

- [ ] **Step 2: Add it to the `.gitignore` allow-list** in `crates/envoy-config/fuzz/.gitignore` — add `!corpus/parse_bootstrap/hcm_jwt_authn_filter.yaml` alongside the other `!corpus/parse_bootstrap/hcm_*_filter.yaml` allow entries.

- [ ] **Step 3: Add it to the SUCCESS array** in `fuzz_corpus_seeds_parse_or_reject_cleanly` (`crates/envoy-config/src/bootstrap.rs:4270`) — add `"fuzz/corpus/parse_bootstrap/hcm_jwt_authn_filter.yaml",` after the `hcm_fault_filter.yaml` entry. (This grows the curated seed corpus per the SPEC 32→33 accounting; the exact in-test count constant — if any — must be bumped. Read the test body and update the count assertion if present.)

- [ ] **Step 4: Run the corpus test**

Run: `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly`
Expected: PASS (the new seed parses).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/fuzz/.gitignore crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_jwt_authn_filter.yaml crates/envoy-config/src/bootstrap.rs
git commit -m "phase 22 Task 9: parse_bootstrap fuzz seed hcm_jwt_authn_filter.yaml (corpus 32->33)"
```

---

## Task 10: Differential fixture `0030-http-filter-jwt-authn` + Docker wrapper

**Files:**
- Create: `tests/fixtures/0030-http-filter-jwt-authn/{README.md,envoy.yaml,envoy-rust.yaml,expectations.yaml,inputs/gen.py,inputs/jwks.json,inputs/valid.jwt,inputs/tampered.jwt,inputs/expired.jwt,inputs/wrong_aud.jwt}`
- Create: `tests/differential/tests/http_filter_jwt_authn.rs`

- [ ] **Step 1: Generate the static test data** — author `inputs/gen.py` (a documented one-time generator; commit it for reproducibility) that creates an RSA-2048 keypair, writes `jwks.json` (kid `k1`, `kty:RSA`, `n`/`e` base64url), and signs four tokens (all `iss: testing@secure.istio.io`, `kid: k1`):
  - `valid.jwt`: `aud:["jwt-fixture-aud"]`, `exp: 4102444800` (year 2100).
  - `tampered.jwt`: `valid.jwt` with the last signature char flipped.
  - `expired.jwt`: `exp: 1500000000` (year 2017), else like valid.
  - `wrong_aud.jwt`: `aud:["other-aud"]`, `exp: 4102444800` (valid signature/issuer, wrong audience → 403).

  Run `python3 inputs/gen.py` once; commit the outputs. **The key must be RSA-2048** (the `RSA_PKCS1_2048_8192_SHA256` floor — §6.2 L1). Document in the README that the tokens are static and deterministic forever (far-future/past `exp` ⇒ no clock sensitivity).

- [ ] **Step 2: Write `envoy.yaml`** (upstream Envoy) and `envoy-rust.yaml` (envoy-rust) — both an H1 listener, HCM `stat_prefix: ingress_http`, `http_filters: [jwt_authn, router]`, the jwt_authn provider (`issuer: testing@secure.istio.io`, `audiences: ["jwt-fixture-aud"]`, `local_jwks.inline_string: <single-line jwks.json>`, default `forward`), one rule `match {prefix: "/"} requires {provider_name: provider1}`, and a router → `direct_response {status: 200, body {inline_string: "ok\n"}}` for `prefix: "/"`. Per-side asymmetry only in admin/bind addresses (the 0018 precedent). The inline JWKS is byte-identical on both sides.

- [ ] **Step 3: Write `expectations.yaml`** — driver `http1_probe_list` (NOT http2 — the filter is codec-agnostic; H1 is simplest), 5 probes, all `host: envoy.test`, `path: /`:

```yaml
driver:
  kind: http1_probe_list
  probes:
    - name: valid
      method: GET
      path: /
      host: envoy.test
      extra_headers: [["authorization", "Bearer <CONTENTS OF inputs/valid.jwt>"]]
      expected_status: 200
      expected_body: { byte_exact: "ok\n" }
      expected_headers: { set_equal_modulo_allow_list: {} }
    - name: missing
      method: GET
      path: /
      host: envoy.test
      expected_status: 401
      expected_body: { byte_exact: "Jwt is missing" }
      expected_headers: { set_equal_modulo_allow_list: {} }
    - name: tampered
      method: GET
      path: /
      host: envoy.test
      extra_headers: [["authorization", "Bearer <CONTENTS OF inputs/tampered.jwt>"]]
      expected_status: 401
      expected_body: { byte_exact: "Jwt verification fails" }
      expected_headers: { set_equal_modulo_allow_list: {} }
    - name: expired
      method: GET
      path: /
      host: envoy.test
      extra_headers: [["authorization", "Bearer <CONTENTS OF inputs/expired.jwt>"]]
      expected_status: 401
      expected_body: { byte_exact: "Jwt is expired" }
      expected_headers: { set_equal_modulo_allow_list: {} }
    - name: wrong-audience
      method: GET
      path: /
      host: envoy.test
      extra_headers: [["authorization", "Bearer <CONTENTS OF inputs/wrong_aud.jwt>"]]
      expected_status: 403
      expected_body: { byte_exact: "Audiences in Jwt are not allowed" }
      expected_headers: { set_equal_modulo_allow_list: {} }

equivalence:
  response_status: exact
  response_body: byte_exact
```

> `set_equal_modulo_allow_list: {}` enforces `www-authenticate` byte-exact across proxies (it is NOT on the allow-list) — this is the value-exact realm check (§6.2 L3). If the allow-list needs `server`/`date`/`connection` entries (the 0018 precedent), copy them from `tests/fixtures/0018-http-filter-fault/expectations.yaml`. Confirm the exact `http1_probe_list` driver key name + the `extra_headers` shape against `tests/differential/src/lib.rs` (`Http1Probe`) at authoring; the recon found `Http1ProbeList` (04.2) exists.

- [ ] **Step 4: Write the README** — fixture title, the probe burst `[200, 401(missing), 401(verify-fails), 401(expired), 403(audience)]`, the byte-exact bodies + `www-authenticate` per probe, the static-token rationale (no clock sensitivity), the inline-JWKS-on-both-sides note, and the per-side YAML asymmetry. Reference ADR-0055/0056.

- [ ] **Step 5: Write the Docker-gated wrapper** `tests/differential/tests/http_filter_jwt_authn.rs` (mirror `tests/differential/tests/http_filter_fault.rs`):

```rust
use std::path::PathBuf;

#[tokio::test]
async fn http_filter_jwt_authn_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0030-http-filter-jwt-authn");
    differential::run_fixture(&dir).await.expect("fixture passes");
}
```

- [ ] **Step 6: Run the fixture locally** (Docker; the image is already pulled). Pre-build `tests/helpers/*` first; never run the Docker suite concurrently with cargo builds (`project_flaky_access_log_fixture_0012`).

Run: `cargo test -p differential http_filter_jwt_authn_fixture -- --nocapture`
Expected: PASS (both proxies produce identical per-probe status + body + headers). If `www-authenticate` diverges, capture both values and reconcile (most likely a realm-construction or scheme detail — adjust `realm()` in Task 7 or move `www-authenticate` to the allow-list as a documented fallback).

- [ ] **Step 7: Commit**

```bash
git add tests/fixtures/0030-http-filter-jwt-authn/ tests/differential/tests/http_filter_jwt_authn.rs
git commit -m "phase 22 Task 10: fixture 0030-http-filter-jwt-authn (H1; valid/missing/tampered/expired/wrong-aud) + Docker wrapper"
```

---

## Task 11: In-process backstop

**Files:**
- Create: `crates/envoy-bin/tests/http_filter_jwt_authn.rs`

- [ ] **Step 1: Write the backstop** (mirror `crates/envoy-bin/tests/http_filter_fault.rs`) — boot `envoy-bin` (H1) via `tokio::process::Command` with `.kill_on_drop(true)` on a synthesized jwt_authn bootstrap (one provider, inline JWKS committed/embedded as a `const &str`, one `prefix: "/"` rule, router → direct_response 200). Issue sequential `GET /` probes with `Host: envoy.test` and varying `Authorization`, asserting the FULL §6.2 L2 surface this phase can drive deterministically:
  - valid token → 200, body `ok\n`.
  - no Authorization → 401, body `Jwt is missing`, `www-authenticate: Bearer realm="http://envoy.test/"` (assert presence + exact value — heeds the phase-10 M1 backstop-header lesson).
  - tampered → 401, body `Jwt verification fails`, `www-authenticate` has `, error="invalid_token"`.
  - expired → 401, body `Jwt is expired`.
  - wrong-audience → 403, body `Audiences in Jwt are not allowed`.
  - malformed (`Bearer not.a.jwt`) → 401, body `Jwt header is an invalid JSON`.

  Embed the valid/tampered/expired/wrong-aud tokens + the JWKS as `const &str` (copy from the Task 10 `inputs/`). Note in the file header the M21-3/M18-9 extract-a-shared-test-support-crate item is now at N≥6 backstops (consolidation stays deferred per the standing risk-managed decision).

- [ ] **Step 2: Run the backstop**

Run: `cargo test -p envoy-bin --test http_filter_jwt_authn -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Clippy + commit**

Run: `cargo clippy -p envoy-bin --all-targets -- -D warnings`
```bash
git add crates/envoy-bin/tests/http_filter_jwt_authn.rs
git commit -m "phase 22 Task 11: in-process jwt_authn backstop (6 probes incl. 403 audience + malformed)"
```

---

## Task 12: State-4 phase-done verification + STATE/ROADMAP advance

> This is the state-4 session's task (`superpowers:verification-before-completion`), NOT part of the state-3 execution arc. Run it after all of Tasks 1–11 land.

**Files:**
- Modify: `docs/envoy-rust/phases/22-http-filter-jwt-authn/PROGRESS.md` (the verification block), `docs/envoy-rust/STATE.md` (advance state-4-complete/state-5-next)

- [ ] **Step 1: Run the full §7.5 gate suite** and quote ALL outputs into PROGRESS:
  - `cargo build --workspace --all-targets`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo fmt --all -- --check`
  - `cargo test --workspace`
  - `cargo deny check` (load-bearing: confirms the now-direct `aws-lc-rs` dep passes license/advisory policy — it's already transitive, so the policy admits it; verify).
  - The 4+1 standalone-crate builds (`project_isolated_crate_build_blindspot`), **including `cargo build -p envoy-jwt`** and `cargo build -p envoy-filter`.
  - `cargo +nightly fuzz run jwt_parse` (short-budget) AND the `parse_bootstrap` short-budget run.
  - The Docker-gated differential suite (fixture 0030 + all 29 pre-existing 0001–0029) on Linux CI; `h2spec` ≥95%.

- [ ] **Step 2: Push and confirm the single CI run is green** for all gates (a)–(e) simultaneously. Record the CI run ID + HEAD SHA + timestamp in PROGRESS. If the documented `access_log_file_sink`/`0011`/`0022` flake family appears, `gh run rerun --failed` clears it (`project_flaky_access_log_fixture_0012`) — not a regression.

- [ ] **Step 3: Advance STATE.md** to phase-22 state-4-complete / state-5-next; rewrite `## Next expected skill` to the state-5 code review (`superpowers:requesting-code-review`); append a `### Phase-22 state-3 execution arc + state-4 verification` Notes subsection. Commit (docs-only). The NEXT session runs the state-5 review.

---

## Self-review (run against the SPEC §3 deliverables)

- **D1 (envoy-jwt crate)** → Tasks 1–3 (scaffold+base64url, JWKS, verify). ✔
- **D2 (envoy-config schema)** → Task 5 (`JwtAuthnConfig`+variant; reuses existing `DataSource`+`RouteMatch`). ✔
- **D3 (envoy-config validator)** → Task 6 (`validate_jwt_authn_config` + arm + ConfigError). ✔
- **D4 (JwtAuthnFilter runtime)** → Task 7. ✔
- **D5 (HttpFilterInstance variant)** → Task 8. ✔
- **D6 (reserved — no HCM writer-path work)** → confirmed: the H1/H2 decoration helpers are reused unchanged (Task 7 Step 7 notes; no codec change). ✔
- **D7 (stats + BEHAVIOR_CONTRACT)** → Task 7 (stats wiring + contract rows). ✔
- **D8.1 (fixture 0030 + wrapper + static inputs)** → Task 10. ✔
- **D8.2 (fuzz seeds: parse_bootstrap seed + new envoy-jwt fuzz target)** → Tasks 9 + 4. ✔
- **D8.3 (in-process backstop)** → Task 11. ✔
- State-4 verification + STATE advance → Task 12. ✔

**SPEC §0 findings honored:** jwt_authn self-matches via `rules[]` (no per-route config — Task 7 `route_match_matches`); crypto isolated in `envoy-jwt` with `aws-lc-rs` direct (Tasks 1–3). **§6.1 split-gate:** evaluated — the crypto-API path is CLEAN (no DER assembly), keeping `envoy-jwt` small; phase ships SINGLE-PHASE (no split; ADR-0057 not fired). **§6.2 divergences** (audience-403, dynamic realm, body-string corrections, no-rule-allow, forward-strip) reconciled by ADR-0056 at this PLAN-write.

**Type consistency check:** `JwtError` variants (Task 1) ↔ `error_reply` arms (Task 7) ↔ `verify_rs256` returns (Task 3) all match (`NotInForm`, `BadHeaderJson`, `BadPayloadJson`, `NoMatchingKey`, `VerificationFails`, `IssuerMismatch`, `Expired`, `NotYetValid`, `AudienceNotAllowed`, `InvalidJwks`). `build_from_config(cfg, &Arc<StatsRegistry>, &str)` matches the fault/rbac 3-arg signature (Tasks 7, 8). `FilterResponse { status, reason, headers, body }` field names match `fault.rs` (verified at recon).
