//! AccessLogError — typed error variants for the access-log
//! subsystem. Maps OS-level filesystem errors to crate-typed
//! variants for callers (the HCM consumer at `envoy-http1`) to
//! match on. The HCM does NOT propagate these errors up the
//! response-write path per parent-06 SPEC §6 architectural Rule 4
//! (fire-and-forget); they are logged via `tracing::warn!` and
//! discarded.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AccessLogError {
    /// `FileSink::new` failed to open the configured file path
    /// (permissions, parent-directory-missing, file-is-a-directory,
    /// etc.). Surfaces at startup when the HCMConfig is constructed.
    #[error("failed to open access log file at {path}: {source}")]
    Open {
        path: PathBuf,
        source: std::io::Error,
    },

    /// `FileSink::emit` failed to write a record to the file
    /// (filesystem full, file removed mid-runtime, etc.).
    /// Surfaces per-emission at runtime.
    #[error("failed to write access log line to {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },

    /// Path validation failed at `envoy-config` validator time
    /// (empty path). Reserved for future stricter validation
    /// per 06.2 SPEC §6 signpost 6 if the recommendation tightens.
    /// Currently not emitted from inside `envoy-accesslog` —
    /// `envoy-config`'s `ConfigError::InvalidAccessLogPath` is the
    /// surface variant (per Task 5).
    #[error("invalid access log file path: {path}")]
    InvalidPath { path: PathBuf },
}
