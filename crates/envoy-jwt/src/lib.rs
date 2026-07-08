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
#[cfg(any(test, feature = "test-util"))]
pub mod test_support;
mod verify;

pub use error::JwtError;
pub use jwks::JwkSet;
pub use verify::{VerifiedJwt, verify_rs256};
