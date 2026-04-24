#![forbid(unsafe_code)]

//! Phase 01 config surface for envoy-rust. Owns the `Bootstrap` type tree and
//! the `parse_bootstrap` entrypoint. See `docs/envoy-rust/DECISIONS.md`
//! ADR-0008 for the extraction rationale.

pub mod bootstrap;

pub use bootstrap::{
    Address, Admin, Bootstrap, Cluster, FilterChain, Listener, NetworkFilter, Node, SocketAddress,
    StaticResources,
};

/// The only network filter name envoy-rust recognizes in phase 01.
pub const ECHO_FILTER: &str = "envoy.filters.network.echo";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("parsing bootstrap YAML")]
    Yaml(#[from] serde_yaml::Error),
    #[error(
        "bootstrap configures neither an admin endpoint nor a listener; envoy-rust has nothing to do"
    )]
    NoRuntime,
    #[error("bootstrap has {0} listeners; phase 01 supports at most one")]
    TooManyListeners(usize),
    #[error("unsupported network filter '{0}'; envoy-rust accepts only '{1}'")]
    UnsupportedFilter(String, &'static str),
}

pub fn parse_bootstrap(yaml: &str) -> Result<Bootstrap, ConfigError> {
    let bootstrap: Bootstrap = serde_yaml::from_str(yaml)?;
    bootstrap::validate(&bootstrap)?;
    Ok(bootstrap)
}
