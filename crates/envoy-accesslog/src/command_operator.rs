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

use std::fmt::Write as _;

use thiserror::Error;

use crate::record::AccessLogRecord;

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

/// Which header side a `%REQ(...)%` / `%RESP(...)%` operator addresses. Threaded
/// through `parse_header_op` and surfaced in [`FormatParseError::UnsupportedHeader`]
/// so diagnostics name the correct side without a stray `&'static str`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Side {
    /// The request side (`%REQ(...)%`).
    Req,
    /// The response side (`%RESP(...)%`).
    Resp,
}

impl Side {
    /// The uppercase keyword form (`"REQ"` / `"RESP"`) used in diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Req => "REQ",
            Side::Resp => "RESP",
        }
    }
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

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
    #[error("operator '{keyword}' has a malformed argument: {detail}")]
    MalformedArgument { keyword: String, detail: String },

    /// A `REQ`/`RESP` header name (and its alt, if any) has no backing field.
    #[error(
        "unsupported {side} header '{name}' has no backing field (supported \
         {side} headers: {supported})"
    )]
    UnsupportedHeader {
        side: Side,
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
        "REQ" => parse_header_op(keyword, rest, Side::Req),
        "RESP" => parse_header_op(keyword, rest, Side::Resp),
        // Non-arg keywords: must NOT carry parens.
        "PROTOCOL" | "RESPONSE_CODE" | "RESPONSE_FLAGS" | "BYTES_RECEIVED" | "BYTES_SENT"
        | "UPSTREAM_HOST" | "START_TIME" | "DURATION" => {
            if rest.is_some() {
                return Err(FormatParseError::MalformedArgument {
                    keyword: keyword.to_string(),
                    detail: "this operator takes no '(...)' argument".to_string(),
                });
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
fn parse_header_op(keyword: &str, rest: Option<&str>, side: Side) -> Result<Op, FormatParseError> {
    let rest = rest.ok_or_else(|| FormatParseError::MalformedArgument {
        keyword: keyword.to_string(),
        detail: "requires a parenthesized header argument, e.g. REQ(:path)".to_string(),
    })?;
    debug_assert!(rest.starts_with('('));

    // Find the closing ')'.
    let close = rest.find(')').ok_or_else(|| FormatParseError::MalformedArgument {
        keyword: keyword.to_string(),
        detail: "missing closing ')' on the header argument".to_string(),
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
        return Err(FormatParseError::MalformedArgument {
            keyword: keyword.to_string(),
            detail: format!("unexpected trailing text '{after}' after ')'"),
        });
    };

    // Split ARG on the first '?' into name / alt; lowercase both. An empty
    // alternate (a `?` with nothing after it) is MALFORMED — not `alt: Some("")`.
    let (name, alt) = match arg.split_once('?') {
        Some((_, a)) if a.is_empty() => {
            return Err(FormatParseError::MalformedArgument {
                keyword: keyword.to_string(),
                detail: "empty alternate after '?'".to_string(),
            });
        }
        Some((n, a)) => (n.to_ascii_lowercase(), Some(a.to_ascii_lowercase())),
        None => (arg.to_ascii_lowercase(), None),
    };

    let allow_list = match side {
        Side::Req => REQ_ALLOW_LIST,
        Side::Resp => RESP_ALLOW_LIST,
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
        Side::Req => Op::Req {
            name,
            alt,
            truncate,
        },
        Side::Resp => Op::Resp {
            name,
            alt,
            truncate,
        },
    })
}

/// A parsed-and-validated access-log format ready for evaluation against an
/// [`AccessLogRecord`]. Wraps the `Vec<Segment>` produced by [`parse_format`];
/// every operator is already known to have a backing field (the parser rejected
/// unbacked names), so `render` is total and never fails.
///
/// The inner field is PRIVATE on purpose: external crates construct a
/// `CompiledFormat` via the `Default`/`from_inline` constructors added in a
/// later task (the default-format re-expression). Same-crate code (and these
/// tests) may use the tuple constructor directly.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledFormat(pub(crate) Vec<Segment>);

impl CompiledFormat {
    /// Parse and compile an inline format STRING into a `CompiledFormat`.
    /// Returns a [`FormatParseError`] (later surfaced as a config-load
    /// failure) if any operator is unknown, malformed, or unbacked.
    pub fn from_inline(s: &str) -> Result<Self, FormatParseError> {
        parse_format(s).map(CompiledFormat)
    }

    /// Evaluate every segment against `record` and concatenate into one line.
    ///
    /// `Literal` segments are emitted verbatim. `Op` segments resolve to their
    /// backing field per §B; absent `Option` fields render as the Envoy
    /// no-value sentinel `-`. `Req`/`Resp` apply `?`-alt fallback then `:N`
    /// byte-truncation to the resolved value.
    pub fn render(&self, record: &AccessLogRecord) -> String {
        // Data-driven pre-allocation (M32-6): size the buffer to the sum of the
        // literal segments' byte lengths (an exact lower bound) plus a small
        // operator allowance, instead of a fixed 256. The tuple shape is kept
        // (in-crate tests construct `CompiledFormat(vec)` directly), so the
        // literal length is summed on the fly here rather than precomputed.
        let literal_len: usize = self
            .0
            .iter()
            .map(|seg| match seg {
                Segment::Literal(s) => s.len(),
                Segment::Op(_) => 0,
            })
            .sum();
        let mut out = String::with_capacity(literal_len + 64);
        for seg in &self.0 {
            match seg {
                Segment::Literal(s) => out.push_str(s),
                Segment::Op(op) => render_op(&mut out, op, record),
            }
        }
        out
    }
}

impl Default for CompiledFormat {
    /// The Envoy default format, re-expressed through the engine by
    /// parsing [`crate::default_format::DEFAULT_FORMAT`] (which carries
    /// its own trailing `\n`). The constant is a compile-time-fixed,
    /// always-valid format string, so the parse cannot fail.
    fn default() -> Self {
        parse_format(crate::default_format::DEFAULT_FORMAT)
            .map(CompiledFormat)
            .expect("default format is valid")
    }
}

/// Render a single operator into `out`.
fn render_op(out: &mut String, op: &Op, record: &AccessLogRecord) {
    match op {
        Op::Protocol => out.push_str(&record.protocol),
        Op::ResponseCode => {
            let _ = write!(out, "{}", record.response_code);
        }
        Op::ResponseFlags => out.push_str(&record.response_flags),
        Op::BytesReceived => {
            let _ = write!(out, "{}", record.bytes_received);
        }
        Op::BytesSent => {
            let _ = write!(out, "{}", record.bytes_sent);
        }
        Op::Duration => {
            let _ = write!(out, "{}", record.duration.as_millis());
        }
        Op::StartTime => out.push_str(&crate::format_iso8601(record.start_time)),
        Op::UpstreamHost => out.push_str(record.upstream_host.as_deref().unwrap_or("-")),
        Op::Req {
            name,
            alt,
            truncate,
        } => {
            // REQ values are all borrowed `&str` from the record.
            let value = resolve_req(name, record)
                .or_else(|| alt.as_deref().and_then(|a| resolve_req(a, record)))
                .unwrap_or("-");
            out.push_str(truncate_bytes(value, *truncate));
        }
        Op::Resp {
            name,
            alt,
            truncate,
        } => {
            // RESP values are owned `String` (the only RESP field is rendered
            // from a `Duration` → decimal-ms string).
            let value = resolve_resp(name, record)
                .or_else(|| alt.as_deref().and_then(|a| resolve_resp(a, record)));
            let value = value.as_deref().unwrap_or("-");
            out.push_str(truncate_bytes(value, *truncate));
        }
    }
}

/// Resolve a REQ header `name` (already lowercased) to its backing field value.
/// Returns `None` for an absent `Option` field. Names not in `REQ_ALLOW_LIST`
/// also return `None`, but the parser already rejected those at parse time.
fn resolve_req<'a>(name: &str, record: &'a AccessLogRecord) -> Option<&'a str> {
    match name {
        ":method" => Some(&record.method),
        // `:path` and `x-envoy-original-path` both map to the already-resolved
        // `path` field (the record build site folds the original-path override
        // into `path` before record construction).
        ":path" | "x-envoy-original-path" => Some(&record.path),
        ":authority" => record.authority.as_deref(),
        "x-forwarded-for" => record.forwarded_for.as_deref(),
        "user-agent" => record.user_agent.as_deref(),
        "x-request-id" => record.request_id.as_deref(),
        _ => None,
    }
}

/// Resolve a RESP header `name` (already lowercased) to its backing field
/// value as an owned `String`. The only backed RESP header is
/// `x-envoy-upstream-service-time`, rendered as decimal milliseconds.
fn resolve_resp(name: &str, record: &AccessLogRecord) -> Option<String> {
    match name {
        "x-envoy-upstream-service-time" => record
            .upstream_service_time
            .map(|d| d.as_millis().to_string()),
        _ => None,
    }
}

/// Truncate `value` to AT MOST `n` BYTES (Envoy truncates by byte count, not
/// char count). To avoid panicking on a multi-byte UTF-8 boundary, we round
/// DOWN to the nearest char boundary at or below `n` bytes via
/// `str::floor_char_boundary` (stable since Rust 1.82; the pinned toolchain is
/// 1.95). For ASCII values — the common access-log case — this is exactly the
/// first `n` bytes. `None` means no truncation.
fn truncate_bytes(value: &str, truncate: Option<usize>) -> &str {
    match truncate {
        None => value,
        Some(n) => &value[..value.floor_char_boundary(n)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::AccessLogRecord;
    use std::time::{Duration, UNIX_EPOCH};

    // A fully-populated, deterministic record for evaluator tests. The record
    // intentionally has no `Default` impl, so every field is set explicitly.
    fn rec() -> AccessLogRecord {
        AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "POST".into(),
            path: "/p".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 16,
            bytes_sent: 433,
            duration: Duration::from_millis(0),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: Some("curl/8.20.0".into()),
            request_id: None,
            authority: Some("h:1".into()),
            upstream_host: Some("1.2.3.4:80".into()),
        }
    }

    #[test]
    fn default_format_parses_successfully() {
        // Guards the `.expect(...)` in `impl Default for CompiledFormat`:
        // the canonical default-format string must always parse.
        assert!(parse_format(crate::default_format::DEFAULT_FORMAT).is_ok());
    }

    #[test]
    fn renders_deterministic_line() {
        let f = parse_format(
            "m=%REQ(:METHOD)% code=%RESPONSE_CODE% ua=%REQ(USER-AGENT)% up=%UPSTREAM_HOST%",
        )
        .unwrap();
        assert_eq!(
            CompiledFormat(f).render(&rec()),
            "m=POST code=200 ua=curl/8.20.0 up=1.2.3.4:80"
        );
    }

    #[test]
    fn absent_header_renders_dash() {
        let f = parse_format("xff=%REQ(X-FORWARDED-FOR)%").unwrap(); // forwarded_for=None
        assert_eq!(CompiledFormat(f).render(&rec()), "xff=-");
    }

    #[test]
    fn truncate_is_byte_count() {
        let f = parse_format("%REQ(USER-AGENT):5%").unwrap();
        assert_eq!(CompiledFormat(f).render(&rec()), "curl/"); // first 5 bytes
    }

    #[test]
    fn alt_used_when_primary_absent() {
        let f = parse_format("%REQ(X-FORWARDED-FOR?USER-AGENT)%").unwrap(); // xff absent → ua
        assert_eq!(CompiledFormat(f).render(&rec()), "curl/8.20.0");
    }

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

    // RESP present: `x-envoy-upstream-service-time` renders the upstream
    // service time as decimal milliseconds via `as_millis()`.
    #[test]
    fn resp_upstream_service_time_present() {
        let mut r = rec();
        r.upstream_service_time = Some(Duration::from_millis(7));
        let f = parse_format("%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%").unwrap();
        assert_eq!(CompiledFormat(f).render(&r), "7");
    }

    // RESP absent: an unset `upstream_service_time` (the base `rec()`) renders
    // the Envoy no-value sentinel `-`.
    #[test]
    fn resp_upstream_service_time_absent() {
        let f = parse_format("%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%").unwrap();
        assert_eq!(CompiledFormat(f).render(&rec()), "-"); // upstream_service_time=None
    }

    // Multi-byte truncation rounds DOWN to a char boundary. "café" is 5 bytes
    // ("caf" = 3 bytes, "é" = bytes 3..5). Truncating to 4 bytes lands mid-"é",
    // so `floor_char_boundary(4)` = 3 → "caf" (no panic, no partial "é").
    #[test]
    fn truncate_multibyte_rounds_down_to_char_boundary() {
        let mut r = rec();
        r.user_agent = Some("café".into());
        let f = parse_format("%REQ(USER-AGENT):4%").unwrap();
        assert_eq!(CompiledFormat(f).render(&r), "caf"); // byte 4 is mid-"é" → floor to 3

        // Truncating to the full 5 bytes is a char boundary → the whole "café".
        let f = parse_format("%REQ(USER-AGENT):5%").unwrap();
        assert_eq!(CompiledFormat(f).render(&r), "café");
    }

    // alt + `:N`: the primary header is absent, so the alt resolves; the `:N`
    // truncation then applies to the alt-resolved value, not the primary.
    #[test]
    fn alt_resolved_value_is_truncated() {
        // xff absent → alt user-agent "curl/8.20.0" → truncate to 4 bytes.
        let f = parse_format("%REQ(X-FORWARDED-FOR?USER-AGENT):4%").unwrap();
        assert_eq!(CompiledFormat(f).render(&rec()), "curl");
    }

    // M32-2: an empty alternate after '?' (a '?' with nothing after) is
    // MALFORMED, not `alt: Some("")`.
    #[test]
    fn empty_alternate_is_error() {
        assert!(matches!(
            parse_format("%REQ(:PATH?)%").unwrap_err(),
            FormatParseError::MalformedArgument { .. }
        ));
    }

    // M32-2: pin `:0` semantics — a `:0` truncation is VALID and renders the
    // empty string for a present value (floor_char_boundary(0) = 0, total).
    #[test]
    fn truncate_zero_is_valid_and_empty() {
        let f = parse_format("%REQ(USER-AGENT):0%").expect("`:0` is a valid truncation");
        assert_eq!(CompiledFormat(f).render(&rec()), ""); // user_agent present, truncated to 0 bytes
    }
}
