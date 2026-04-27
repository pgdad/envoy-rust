//! Error type for envoy-http1.

#[derive(Debug, thiserror::Error)]
pub enum Http1Error {
    #[error("malformed request line")]
    MalformedRequestLine,

    #[error("malformed header (bad token, missing colon, etc.)")]
    MalformedHeader,

    #[error("request headers exceed cap of {cap} bytes")]
    HeadersTooLarge { cap: usize },

    #[error("request body exceeds cap of {cap} bytes")]
    BodyTooLarge { cap: usize },

    #[error("unexpected EOF mid-message")]
    UnexpectedEof,

    #[error("io: {source}")]
    Io {
        #[source]
        source: std::io::Error,
    },
}

impl From<std::io::Error> for Http1Error {
    fn from(source: std::io::Error) -> Self {
        Self::Io { source }
    }
}
