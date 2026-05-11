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

pub mod default_format;
mod error;
pub mod file_sink;
pub mod record;
mod sink;

pub use error::AccessLogError;
pub use file_sink::FileSink;
pub use record::AccessLogRecord;
