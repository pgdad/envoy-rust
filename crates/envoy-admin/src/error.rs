//! envoy-admin typed-error enum.

#[derive(Debug, thiserror::Error)]
pub enum AdminError {
    #[error("admin address {raw} is not a parseable SocketAddr: {source}")]
    BadAddress {
        raw: String,
        #[source]
        source: std::net::AddrParseError,
    },

    #[error("admin listener IO error: {0}")]
    Io(#[from] std::io::Error),
}
