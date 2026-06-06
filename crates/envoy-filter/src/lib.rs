#![forbid(unsafe_code)]

//! HTTP filter chain iteration protocol.
//!
//! Hand-rolled per D-3.2's "Must be written from scratch" doctrine for
//! filter chain engines. Synchronous (non-async) iteration on the
//! already-buffered request/response shape established by 04.1 + 05.2.

pub mod error;
pub mod fault;
pub mod header_mutation;
pub mod instance;
pub mod jwt_authn;
pub mod local_rate_limit;
pub mod pipeline;
pub mod rbac;
pub mod router;
pub mod types;

pub use error::FilterError;
pub use fault::FaultFilter;
pub use header_mutation::HeaderMutationFilter;
pub use instance::HttpFilterInstance;
pub use jwt_authn::JwtAuthnFilter;
pub use local_rate_limit::LocalRateLimitFilter;
pub use pipeline::{Decision, FilterPipeline};
pub use rbac::RbacFilter;
pub use router::RouterTerminus;
pub use types::{FilterRequest, FilterResponse};
