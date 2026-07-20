#![forbid(unsafe_code)]

//! envoy-accesslog — access-log subsystem foundation: record value-type,
//! concrete file-sink, Envoy default-format emitter.
//!
//! Owns the workspace's only direct surface for access-log primitives. The
//! HCM at envoy-http1 builds AccessLogRecord values and dispatches via
//! FileSink::emit; no other workspace crate calls FileSink or the
//! default-format emitter directly.
//!
//! The Sink trait is intentionally NOT shipped in this version. See
//! parent-06 SPEC §3 D8.2 option (c) and 06.2 SPEC §3 architectural rule 3.
//! When N≥2 sink types exist (gRPC ALS sink, stdout sink, etc.), a
//! future phase will ship the trait + multi-sink dispatch in this crate.

pub mod command_operator;
pub mod default_format;
mod error;
pub mod file_sink;
pub mod filter;
mod json_format;
mod log_format;
pub mod record;
mod sink;

pub use command_operator::{CompiledFormat, FormatParseError, parse_format};
pub use error::AccessLogError;
pub use file_sink::FileSink;
pub use filter::{FilterOp, HeaderMatch, LogFilter, StatusCodeComparison};
pub use json_format::{CompiledJsonFormat, JsonValueInput};
pub use log_format::LogFormat;
pub use record::AccessLogRecord;

use std::time::SystemTime;

/// Public wrapper around the internal `default_format::format_iso8601`
/// `&mut String`-writer. Returns a freshly-allocated `String` in the canonical
/// 24-byte `YYYY-MM-DDTHH:MM:SS.sssZ` shape.
///
/// Phase 08.1 D13a: surfaces the ISO-8601 emitter for cross-crate consumers
/// (`envoy-admin` uses this for `server_info`'s `uptime_current_epoch` and for
/// the `/stats` JSON timestamps). The internal `pub(crate) fn` writer
/// (`&mut String, SystemTime`) stays internal-only — only this allocating
/// wrapper is public.
pub fn format_iso8601(t: SystemTime) -> String {
    let mut s = String::new();
    default_format::format_iso8601(&mut s, t);
    s
}

#[cfg(test)]
mod public_format_iso8601_tests {
    use super::format_iso8601;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn epoch_zero_renders_canonical_shape() {
        let s = format_iso8601(UNIX_EPOCH);
        assert_eq!(s, "1970-01-01T00:00:00.000Z");
        assert_eq!(s.len(), 24, "canonical 24-byte shape");
    }

    #[test]
    fn known_date_renders_correctly() {
        // 2024-02-29T12:34:56.789Z — leap day boundary; mirrors the
        // internal-test golden case at `default_format::tests::
        // format_iso8601_known_date`.
        let t = UNIX_EPOCH + Duration::from_millis(1_709_210_096_789);
        let s = format_iso8601(t);
        assert_eq!(s, "2024-02-29T12:34:56.789Z");
    }
}
