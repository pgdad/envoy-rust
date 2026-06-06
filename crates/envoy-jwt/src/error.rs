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
