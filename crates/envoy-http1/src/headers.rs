//! Case-insensitive header name lookup + canonical-form name constants.

pub const HOST: &str = "host";
pub const CONTENT_LENGTH: &str = "content-length";
pub const CONNECTION: &str = "connection";
pub const SERVER: &str = "server";
pub const DATE: &str = "date";
pub const TRANSFER_ENCODING: &str = "transfer-encoding";
pub const CONTENT_TYPE: &str = "content-type";

/// Find a header by name using case-insensitive comparison per HTTP/1.1 §3.2.
/// Returns the value of the first matching header, or `None`.
pub fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_header_is_case_insensitive() {
        let headers = vec![("Host".to_string(), "x".to_string())];
        assert_eq!(find_header(&headers, "host"), Some("x"));
        assert_eq!(find_header(&headers, "HOST"), Some("x"));
        assert_eq!(find_header(&headers, "HoSt"), Some("x"));
    }

    #[test]
    fn find_header_returns_none_on_missing() {
        let headers: Vec<(String, String)> = vec![];
        assert_eq!(find_header(&headers, "host"), None);

        let headers = vec![("X-Foo".to_string(), "1".to_string())];
        assert_eq!(find_header(&headers, "host"), None);
    }
}
