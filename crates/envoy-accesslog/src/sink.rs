//! Sink trait — DEFERRED.
//!
//! Per parent-06 SPEC §3 D8.2 option (c) and 06.2 SPEC §3 architectural
//! rule 3: the `Sink` trait is intentionally NOT shipped in this version.
//! `FileSink` (in `file_sink.rs`) ships as a concrete inherent impl.
//! `HCMConfig.access_log` is typed concretely as `Vec<Arc<FileSink>>`,
//! not `Vec<Arc<dyn Sink>>`.
//!
//! Future observability-family phases that ship a second sink type
//! (gRPC ALS sink, stdout sink, etc.) will:
//!   1. Define the `Sink` trait here, in `sink.rs`.
//!   2. Promote `FileSink::emit` to a `Sink::emit` trait method.
//!   3. Re-type `HCMConfig.access_log` to `Vec<Arc<dyn Sink>>` (or
//!      a typed enum dispatcher, depending on the dispatch shape
//!      that phase picks).
//!
//! The placeholder file exists to preserve module-decomposition
//! stability — the trait lands by editing this file rather than by
//! introducing a new module.
