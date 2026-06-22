//! command_operator — the access-log format-string PARSER (the correctness
//! gate of the Observability command-operator engine).
//!
//! Parses an Envoy-style access-log format string (literals interleaved with
//! `%OPERATOR%` substitutions) into a `Vec<Segment>` of literals and typed
//! `Op` variants. ONLY the operators with a backing field on
//! [`crate::record::AccessLogRecord`] are accepted; anything else (unknown
//! keyword, unsupported header name, malformed `%` run) is a typed
//! [`FormatParseError`] that will later surface as a config-load failure.
//!
//! This module is the PARSER only. The evaluator (render against a record),
//! the default-format re-expression, the config field, and the HCM wiring are
//! later tasks in this phase. Do NOT add `render`/`CompiledFormat` here.

use thiserror::Error;

/// One piece of a parsed format string: either a literal run of text or a
/// typed substitution operator.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    /// Literal text emitted verbatim (already `%%`-unescaped and coalesced).
    Literal(String),
    /// A typed substitution operator.
    Op(Op),
}

/// The supported command operators. Carries ONLY operators that map to a
/// backing field on [`crate::record::AccessLogRecord`]. `Req`/`Resp` carry the
/// (lowercased) header `name`, an optional `alt` fallback name, and an optional
/// `:N` truncation length.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// `%START_TIME%`
    StartTime,
    /// `%REQ(NAME[?ALT])[:N]%` — request-side header.
    Req {
        name: String,
        alt: Option<String>,
        truncate: Option<usize>,
    },
    /// `%RESP(NAME[?ALT])[:N]%` — response-side header.
    Resp {
        name: String,
        alt: Option<String>,
        truncate: Option<usize>,
    },
    /// `%PROTOCOL%`
    Protocol,
    /// `%RESPONSE_CODE%`
    ResponseCode,
    /// `%RESPONSE_FLAGS%`
    ResponseFlags,
    /// `%BYTES_RECEIVED%`
    BytesReceived,
    /// `%BYTES_SENT%`
    BytesSent,
    /// `%DURATION%`
    Duration,
    /// `%UPSTREAM_HOST%`
    UpstreamHost,
}

/// REQ-side header names (lowercased) that have a backing field on
/// [`crate::record::AccessLogRecord`]. A `%REQ(...)%` operator is only valid if
/// its `name` (or `alt`) appears here.
pub const REQ_ALLOW_LIST: &[&str] = &[
    ":method",
    ":authority",
    ":path",
    "x-envoy-original-path",
    "x-forwarded-for",
    "user-agent",
    "x-request-id",
];

/// RESP-side header names (lowercased) that have a backing field on
/// [`crate::record::AccessLogRecord`].
pub const RESP_ALLOW_LIST: &[&str] = &["x-envoy-upstream-service-time"];

/// A failure parsing an access-log format string. The detail strings are
/// human-readable because they will later feed a config-load error message.
#[derive(Debug, Error, PartialEq)]
pub enum FormatParseError {
    /// A `%` was opened but never closed by a matching `%`.
    #[error("unterminated '%' operator (no closing '%') in format string")]
    UnterminatedOperator,

    /// A `%...%` run with an empty body (`%%` is the literal escape, not this).
    #[error("empty operator '%%...%%' (no keyword between the '%' delimiters)")]
    EmptyOperator,

    /// A keyword that is not one of the supported command operators.
    #[error("unknown access-log operator keyword '{0}'")]
    UnknownKeyword(String),

    /// `REQ`/`RESP` used without the required parenthesized header arg, or a
    /// non-`REQ`/`RESP` keyword given an argument it does not take.
    #[error("operator '{0}' has a malformed argument: {1}")]
    MalformedArgument(String, String),

    /// A `REQ`/`RESP` header name (and its alt, if any) has no backing field.
    #[error(
        "unsupported {side} header '{name}' has no backing field (supported \
         {side} headers: {supported})"
    )]
    UnsupportedHeader {
        side: &'static str,
        name: String,
        supported: String,
    },

    /// A `:N` truncation suffix that is not a valid decimal `usize`.
    #[error("invalid ':N' truncation length '{0}' (expected a non-negative integer)")]
    BadTruncate(String),
}

/// Parse an access-log format string into a `Vec<Segment>`.
///
/// `%` opens an operator; a following `%` (i.e. `%%`) is the escape for a
/// literal `%`. Otherwise the text strictly between the opening and matching
/// closing `%` is the operator body. Adjacent literals are coalesced.
pub fn parse_format(s: &str) -> Result<Vec<Segment>, FormatParseError> {
    let bytes = s.as_bytes();
    let mut segments: Vec<Segment> = Vec::new();
    let mut literal = String::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'%' {
            // Accumulate a maximal run of non-'%' bytes and push it as one
            // slice of the original `&str`, preserving multibyte UTF-8 chars.
            // (`%` is ASCII, so the run boundaries are char boundaries.)
            let run_start = i;
            while i < bytes.len() && bytes[i] != b'%' {
                i += 1;
            }
            literal.push_str(&s[run_start..i]);
            continue;
        }

        // bytes[i] == '%'
        if i + 1 < bytes.len() && bytes[i + 1] == b'%' {
            // `%%` → literal '%'.
            literal.push('%');
            i += 2;
            continue;
        }

        // Opening '%' of an operator: find the matching closing '%'.
        let body_start = i + 1;
        let mut j = body_start;
        while j < bytes.len() && bytes[j] != b'%' {
            j += 1;
        }
        if j >= bytes.len() {
            // No closing '%'. Covers both `50%done` and `x%` and `x=%REQ(`.
            return Err(FormatParseError::UnterminatedOperator);
        }

        let body = &s[body_start..j];
        // Flush any pending literal before the operator segment.
        if !literal.is_empty() {
            segments.push(Segment::Literal(std::mem::take(&mut literal)));
        }
        segments.push(Segment::Op(parse_operator(body)?));
        i = j + 1;
    }

    if !literal.is_empty() {
        segments.push(Segment::Literal(literal));
    }
    Ok(segments)
}

/// Parse a single operator body (the text strictly between two `%`).
fn parse_operator(body: &str) -> Result<Op, FormatParseError> {
    if body.is_empty() {
        return Err(FormatParseError::EmptyOperator);
    }

    // Split on the first '(' — the part before is the KEYWORD.
    let (keyword, rest) = match body.find('(') {
        Some(p) => (&body[..p], Some(&body[p..])),
        None => (body, None),
    };

    if keyword.is_empty() {
        return Err(FormatParseError::EmptyOperator);
    }

    match keyword {
        "REQ" => parse_header_op(keyword, rest, "REQ"),
        "RESP" => parse_header_op(keyword, rest, "RESP"),
        // Non-arg keywords: must NOT carry parens.
        "PROTOCOL" | "RESPONSE_CODE" | "RESPONSE_FLAGS" | "BYTES_RECEIVED" | "BYTES_SENT"
        | "UPSTREAM_HOST" | "START_TIME" | "DURATION" => {
            if rest.is_some() {
                return Err(FormatParseError::MalformedArgument(
                    keyword.to_string(),
                    "this operator takes no '(...)' argument".to_string(),
                ));
            }
            Ok(match keyword {
                "PROTOCOL" => Op::Protocol,
                "RESPONSE_CODE" => Op::ResponseCode,
                "RESPONSE_FLAGS" => Op::ResponseFlags,
                "BYTES_RECEIVED" => Op::BytesReceived,
                "BYTES_SENT" => Op::BytesSent,
                "UPSTREAM_HOST" => Op::UpstreamHost,
                "START_TIME" => Op::StartTime,
                "DURATION" => Op::Duration,
                _ => unreachable!("matched above"),
            })
        }
        other => Err(FormatParseError::UnknownKeyword(other.to_string())),
    }
}

/// Parse a `REQ`/`RESP` operator: `KEYWORD(ARG)` optionally followed by `:N`.
/// `rest` is the body slice starting at the opening `(` (or `None` if absent).
fn parse_header_op(
    keyword: &str,
    rest: Option<&str>,
    side: &'static str,
) -> Result<Op, FormatParseError> {
    let rest = rest.ok_or_else(|| {
        FormatParseError::MalformedArgument(
            keyword.to_string(),
            "requires a parenthesized header argument, e.g. REQ(:path)".to_string(),
        )
    })?;
    debug_assert!(rest.starts_with('('));

    // Find the closing ')'.
    let close = rest.find(')').ok_or_else(|| {
        FormatParseError::MalformedArgument(
            keyword.to_string(),
            "missing closing ')' on the header argument".to_string(),
        )
    })?;

    let arg = &rest[1..close];
    let after = &rest[close + 1..];

    // Anything after ')' must be a `:N` truncation (or empty).
    let truncate = if after.is_empty() {
        None
    } else if let Some(num) = after.strip_prefix(':') {
        if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
            return Err(FormatParseError::BadTruncate(num.to_string()));
        }
        Some(
            num.parse::<usize>()
                .map_err(|_| FormatParseError::BadTruncate(num.to_string()))?,
        )
    } else {
        return Err(FormatParseError::MalformedArgument(
            keyword.to_string(),
            format!("unexpected trailing text '{after}' after ')'"),
        ));
    };

    // Split ARG on the first '?' into name / alt; lowercase both.
    let (name, alt) = match arg.split_once('?') {
        Some((n, a)) => (n.to_ascii_lowercase(), Some(a.to_ascii_lowercase())),
        None => (arg.to_ascii_lowercase(), None),
    };

    let allow_list = match side {
        "REQ" => REQ_ALLOW_LIST,
        _ => RESP_ALLOW_LIST,
    };

    // Valid iff at least one resolvable branch (name, else alt) is backed.
    let name_backed = allow_list.contains(&name.as_str());
    let alt_backed = alt.as_deref().is_some_and(|a| allow_list.contains(&a));
    if !name_backed && !alt_backed {
        return Err(FormatParseError::UnsupportedHeader {
            side,
            name: name.clone(),
            supported: allow_list.join(", "),
        });
    }

    Ok(match side {
        "REQ" => Op::Req {
            name,
            alt,
            truncate,
        },
        _ => Op::Resp {
            name,
            alt,
            truncate,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Literals + a simple operator.
    #[test]
    fn parses_literal_and_operator() {
        let segs = parse_format("code=%RESPONSE_CODE% done").unwrap();
        assert_eq!(
            segs,
            vec![
                Segment::Literal("code=".into()),
                Segment::Op(Op::ResponseCode),
                Segment::Literal(" done".into()),
            ]
        );
    }
    // %% escape → literal '%'.
    #[test]
    fn double_percent_is_literal() {
        assert_eq!(
            parse_format("a%%b").unwrap(),
            vec![Segment::Literal("a%b".into())]
        );
    }
    // REQ with pseudo-header, alt, and :N truncation.
    #[test]
    fn parses_req_with_alt_and_truncate() {
        let segs = parse_format("%REQ(X-MISSING?:PATH):5%").unwrap();
        assert_eq!(
            segs,
            vec![Segment::Op(Op::Req {
                name: "x-missing".into(),
                alt: Some(":path".into()),
                truncate: Some(5),
            })]
        );
    }
    // Boot-fatal cases.
    #[test]
    fn lone_percent_is_error() {
        assert!(matches!(
            parse_format("50%done").unwrap_err(),
            FormatParseError::UnterminatedOperator
        ));
    }
    #[test]
    fn trailing_percent_is_error() {
        assert!(matches!(
            parse_format("x%").unwrap_err(),
            FormatParseError::UnterminatedOperator
        ));
    }
    #[test]
    fn empty_operator_is_error() {
        assert!(matches!(
            parse_format("%()%").unwrap_err(),
            FormatParseError::EmptyOperator
        ));
    }
    #[test]
    fn unterminated_is_error() {
        assert!(matches!(
            parse_format("x=%REQ(").unwrap_err(),
            FormatParseError::UnterminatedOperator
        ));
    }
    #[test]
    fn unknown_operator_is_error() {
        assert!(matches!(
            parse_format("%TOTALLY_UNKNOWN%").unwrap_err(),
            FormatParseError::UnknownKeyword(_)
        ));
    }
    // Unsupported (well-formed) header name → error (no backing field).
    #[test]
    fn unsupported_req_header_is_error() {
        assert!(matches!(
            parse_format("%REQ(X-CUSTOM)%").unwrap_err(),
            FormatParseError::UnsupportedHeader { .. }
        ));
    }
    // Non-ASCII literal text must round-trip faithfully (no mojibake).
    #[test]
    fn literal_preserves_non_ascii() {
        assert_eq!(
            parse_format("café%PROTOCOL%").unwrap(),
            vec![Segment::Literal("café".into()), Segment::Op(Op::Protocol),]
        );
    }
    // Non-ASCII mixed with the `%%` escape.
    #[test]
    fn literal_non_ascii_with_percent_escape() {
        assert_eq!(
            parse_format("a€%%b").unwrap(),
            vec![Segment::Literal("a€%b".into())]
        );
    }
}
