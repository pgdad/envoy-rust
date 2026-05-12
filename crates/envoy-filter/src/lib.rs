#![forbid(unsafe_code)]

//! HTTP filter chain iteration protocol.
//!
//! Hand-rolled per D-3.2's "Must be written from scratch" doctrine for
//! filter chain engines. Synchronous (non-async) iteration on the
//! already-buffered request/response shape established by 04.1 + 05.2.

pub mod error;
pub mod instance;
pub mod pipeline;
pub mod router;

pub use error::FilterError;
pub use instance::HttpFilterInstance;
pub use pipeline::{Decision, FilterPipeline};
pub use router::RouterTerminus;
