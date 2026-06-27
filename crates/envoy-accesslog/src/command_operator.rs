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
    /// `%ROUTE_NAME%` — the matched route's config `name` (phase 41). An
    /// `Option<String>` mirroring `UpstreamHost`: present → the name, absent
    /// (unnamed route) → the `-` sentinel / json `null`.
    RouteName,
    /// `%DYNAMIC_METADATA(namespace:key)%` — a single-level two-segment lookup
    /// into the per-request dynamic-metadata store (§A2-LOCKED). namespace/key
    /// are CASE-SENSITIVE (NOT lowercased). Carries NO `:N` truncation field —
    /// a trailing `:N` is boot-fatal in Envoy (`DYNAMIC_METADATA does not allow
    /// length to be specified.`), so the parser rejects it. A present scalar
    /// string value renders RAW, UNQUOTED (§A3); an absent namespace or key
    /// renders `-` (§A4).
    DynamicMetadata { namespace: String, key: String },
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
        "DYNAMIC_METADATA" => parse_dynamic_metadata_op(rest),
        // Non-arg keywords: must NOT carry parens.
        "PROTOCOL" | "RESPONSE_CODE" | "RESPONSE_FLAGS" | "BYTES_RECEIVED" | "BYTES_SENT"
        | "UPSTREAM_HOST" | "ROUTE_NAME" | "START_TIME" | "DURATION" => {
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
                "ROUTE_NAME" => Op::RouteName,
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
    let close = rest
        .find(')')
        .ok_or_else(|| FormatParseError::MalformedArgument {
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
        Some((_, "")) => {
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

/// Parse a `%DYNAMIC_METADATA(namespace:key)%` operator (§A2-LOCKED). `rest` is
/// the body slice starting at the opening `(` (or `None` if absent). Unlike
/// `parse_header_op`, this operator:
/// - REQUIRES a `(...)` argument (a no-arg `%DYNAMIC_METADATA%` is boot-fatal);
/// - REJECTS any trailing `:N` length suffix (boot-fatal in Envoy);
/// - requires EXACTLY two non-empty `:`-separated segments (the single-level MVP
///   — a 1-segment whole-namespace or a 3+-segment nested path is rejected);
/// - does NOT lowercase namespace/key (metadata keys are case-sensitive).
fn parse_dynamic_metadata_op(rest: Option<&str>) -> Result<Op, FormatParseError> {
    const KEYWORD: &str = "DYNAMIC_METADATA";

    let rest = rest.ok_or_else(|| FormatParseError::MalformedArgument {
        keyword: KEYWORD.to_string(),
        detail: "requires a (namespace:key) argument".to_string(),
    })?;
    debug_assert!(rest.starts_with('('));

    let close = rest
        .find(')')
        .ok_or_else(|| FormatParseError::MalformedArgument {
            keyword: KEYWORD.to_string(),
            detail: "missing closing ')' on the (namespace:key) argument".to_string(),
        })?;

    let arg = &rest[1..close];
    let after = &rest[close + 1..];

    // §A2: a trailing `:N` length suffix is boot-fatal — nothing may follow ')'.
    if !after.is_empty() {
        return Err(FormatParseError::MalformedArgument {
            keyword: KEYWORD.to_string(),
            detail: "does not accept a ':N' length suffix".to_string(),
        });
    }

    // Exactly two non-empty `:`-separated segments (single-level MVP).
    let mut parts = arg.split(':');
    let (namespace, key) = match (parts.next(), parts.next(), parts.next()) {
        (Some(ns), Some(k), None) if !ns.is_empty() && !k.is_empty() => (ns, k),
        _ => {
            return Err(FormatParseError::MalformedArgument {
                keyword: KEYWORD.to_string(),
                detail: "requires exactly 'namespace:key'".to_string(),
            });
        }
    };

    Ok(Op::DynamicMetadata {
        namespace: namespace.to_string(),
        key: key.to_string(),
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
pub struct CompiledFormat {
    pub(crate) segments: Vec<Segment>,
    /// `omit_empty_values` (ADR-0096 §B): when `true`, an absent operator renders
    /// as the empty string `""` instead of the `-` sentinel. Default `false`.
    pub(crate) omit_empty: bool,
}

impl CompiledFormat {
    /// Same-crate tuple-style constructor: wrap segments with `omit_empty=false`
    /// (the default-off path). Kept so the in-crate tests + the json/text default
    /// sites can construct a format from raw segments unchanged.
    pub(crate) fn new(segments: Vec<Segment>) -> Self {
        Self {
            segments,
            omit_empty: false,
        }
    }

    /// Parse and compile an inline format STRING into a `CompiledFormat`.
    /// Returns a [`FormatParseError`] (later surfaced as a config-load
    /// failure) if any operator is unknown, malformed, or unbacked. The
    /// resulting format has `omit_empty=false`; use [`Self::with_omit_empty`]
    /// to set the flag from `SubstitutionFormatString.omit_empty_values`.
    pub fn from_inline(s: &str) -> Result<Self, FormatParseError> {
        parse_format(s).map(CompiledFormat::new)
    }

    /// Builder setter for the `omit_empty_values` flag (ADR-0096 §B); the HCM
    /// bridge calls this from the config `SubstitutionFormatString`.
    pub fn with_omit_empty(mut self, omit_empty: bool) -> Self {
        self.omit_empty = omit_empty;
        self
    }

    /// Evaluate every segment against `record` and concatenate into one line.
    ///
    /// `Literal` segments are emitted verbatim. `Op` segments resolve to their
    /// backing field per §B; absent `Option` fields render as the Envoy
    /// no-value sentinel `-` (or, when `omit_empty`, the empty string `""` —
    /// ADR-0096 §B). `Req`/`Resp` apply `?`-alt fallback then `:N`
    /// byte-truncation to the resolved value.
    pub fn render(&self, record: &AccessLogRecord) -> String {
        render_value_segments(&self.segments, record, self.omit_empty)
    }
}

/// Render an arbitrary `&[Segment]` slice against `record` into one owned
/// `String` (the engine's shared text-render path). `CompiledFormat::render`
/// delegates here; the phase-38 `json_format` mixed/literal-value encoder reuses
/// it so the text semantics (absent `Option` → the `-` sentinel) stay identical.
///
/// Data-driven pre-allocation (M32-6): size the buffer to the sum of the literal
/// segments' byte lengths (an exact lower bound) plus a small operator allowance.
pub(crate) fn render_value_segments(
    segments: &[Segment],
    record: &AccessLogRecord,
    omit_empty: bool,
) -> String {
    let literal_len: usize = segments
        .iter()
        .map(|seg| match seg {
            Segment::Literal(s) => s.len(),
            Segment::Op(_) => 0,
        })
        .sum();
    let mut out = String::with_capacity(literal_len + 64);
    for seg in segments {
        match seg {
            Segment::Literal(s) => out.push_str(s),
            Segment::Op(op) => render_op(&mut out, op, record, omit_empty),
        }
    }
    out
}

impl Default for CompiledFormat {
    /// The Envoy default format, re-expressed through the engine by
    /// parsing [`crate::default_format::DEFAULT_FORMAT`] (which carries
    /// its own trailing `\n`). The constant is a compile-time-fixed,
    /// always-valid format string, so the parse cannot fail.
    fn default() -> Self {
        parse_format(crate::default_format::DEFAULT_FORMAT)
            .map(CompiledFormat::new)
            .expect("default format is valid")
    }
}

/// Render a single operator into `out`. `omit_empty` (ADR-0096 §B): when `true`,
/// an absent `Option` operator renders as the empty string `""` instead of the
/// `-` sentinel (the four substitution sites below).
fn render_op(out: &mut String, op: &Op, record: &AccessLogRecord, omit_empty: bool) {
    let empty_or_dash = if omit_empty { "" } else { "-" };
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
        Op::UpstreamHost => out.push_str(record.upstream_host.as_deref().unwrap_or(empty_or_dash)),
        Op::RouteName => out.push_str(record.route_name.as_deref().unwrap_or(empty_or_dash)),
        Op::DynamicMetadata { namespace, key } => out.push_str(
            record
                .dynamic_metadata
                .get(namespace)
                .and_then(|m| m.get(key))
                .map(String::as_str)
                .unwrap_or(empty_or_dash),
        ),
        Op::Req {
            name,
            alt,
            truncate,
        } => {
            // REQ values are all borrowed `&str` from the record.
            let value = resolve_req(name, record)
                .or_else(|| alt.as_deref().and_then(|a| resolve_req(a, record)))
                .unwrap_or(empty_or_dash);
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
            let value = value.as_deref().unwrap_or(empty_or_dash);
            out.push_str(truncate_bytes(value, *truncate));
        }
    }
}

/// Resolve a REQ header `name` (already lowercased) to its backing field value.
/// Returns `None` for an absent `Option` field. Names not in `REQ_ALLOW_LIST`
/// also return `None`, but the parser already rejected those at parse time.
pub(crate) fn resolve_req<'a>(name: &str, record: &'a AccessLogRecord) -> Option<&'a str> {
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
pub(crate) fn resolve_resp(name: &str, record: &AccessLogRecord) -> Option<String> {
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
pub(crate) fn truncate_bytes(value: &str, truncate: Option<usize>) -> &str {
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
            route_name: None,
            dynamic_metadata: std::collections::BTreeMap::new(),
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
            CompiledFormat::new(f).render(&rec()),
            "m=POST code=200 ua=curl/8.20.0 up=1.2.3.4:80"
        );
    }

    #[test]
    fn absent_header_renders_dash() {
        let f = parse_format("xff=%REQ(X-FORWARDED-FOR)%").unwrap(); // forwarded_for=None
        assert_eq!(CompiledFormat::new(f).render(&rec()), "xff=-");
    }

    // --- phase 41 (ADR-0098 §C): %ROUTE_NAME% — parse + text render ---

    #[test]
    fn route_name_parses_as_no_arg_op() {
        assert_eq!(parse_format("%ROUTE_NAME%").unwrap(), vec![Segment::Op(Op::RouteName)]);
    }

    #[test]
    fn route_name_rejects_paren_argument() {
        assert!(parse_format("%ROUTE_NAME(x)%").is_err());
    }

    #[test]
    fn route_name_text_renders_name_or_dash() {
        let mut named = rec();
        named.route_name = Some("myroute".into());
        assert_eq!(CompiledFormat::new(parse_format("%ROUTE_NAME%").unwrap()).render(&named), "myroute");
        assert_eq!(
            CompiledFormat::new(parse_format("r=%ROUTE_NAME%").unwrap()).render(&named),
            "r=myroute"
        );

        let mut absent = rec();
        absent.route_name = None;
        assert_eq!(CompiledFormat::new(parse_format("%ROUTE_NAME%").unwrap()).render(&absent), "-");
        assert_eq!(
            CompiledFormat::new(parse_format("r=%ROUTE_NAME%").unwrap()).render(&absent),
            "r=-"
        );
    }

    // --- phase 40 t2 (ADR-0096 §B): omit_empty sentinel swap on the TEXT path ---

    // A record with no upstream host and no forwarded-for — both absent ops.
    fn rec_no_upstream() -> AccessLogRecord {
        let mut r = rec();
        r.upstream_host = None;
        r.forwarded_for = None;
        r
    }

    #[test]
    fn omit_empty_swaps_dash_for_empty_in_multi_segment() {
        let segs = parse_format("up=%UPSTREAM_HOST% x=%REQ(X-FORWARDED-FOR)%").unwrap();
        // omit=false → the `-` sentinel; omit=true → the empty string (§B / CASE-3).
        assert_eq!(
            render_value_segments(&segs, &rec_no_upstream(), false),
            "up=- x=-"
        );
        assert_eq!(
            render_value_segments(&segs, &rec_no_upstream(), true),
            "up= x="
        );
    }

    #[test]
    fn truncate_is_byte_count() {
        let f = parse_format("%REQ(USER-AGENT):5%").unwrap();
        assert_eq!(CompiledFormat::new(f).render(&rec()), "curl/"); // first 5 bytes
    }

    #[test]
    fn alt_used_when_primary_absent() {
        let f = parse_format("%REQ(X-FORWARDED-FOR?USER-AGENT)%").unwrap(); // xff absent → ua
        assert_eq!(CompiledFormat::new(f).render(&rec()), "curl/8.20.0");
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
        assert_eq!(CompiledFormat::new(f).render(&r), "7");
    }

    // RESP absent: an unset `upstream_service_time` (the base `rec()`) renders
    // the Envoy no-value sentinel `-`.
    #[test]
    fn resp_upstream_service_time_absent() {
        let f = parse_format("%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%").unwrap();
        assert_eq!(CompiledFormat::new(f).render(&rec()), "-"); // upstream_service_time=None
    }

    // Multi-byte truncation rounds DOWN to a char boundary. "café" is 5 bytes
    // ("caf" = 3 bytes, "é" = bytes 3..5). Truncating to 4 bytes lands mid-"é",
    // so `floor_char_boundary(4)` = 3 → "caf" (no panic, no partial "é").
    #[test]
    fn truncate_multibyte_rounds_down_to_char_boundary() {
        let mut r = rec();
        r.user_agent = Some("café".into());
        let f = parse_format("%REQ(USER-AGENT):4%").unwrap();
        assert_eq!(CompiledFormat::new(f).render(&r), "caf"); // byte 4 is mid-"é" → floor to 3

        // Truncating to the full 5 bytes is a char boundary → the whole "café".
        let f = parse_format("%REQ(USER-AGENT):5%").unwrap();
        assert_eq!(CompiledFormat::new(f).render(&r), "café");
    }

    // alt + `:N`: the primary header is absent, so the alt resolves; the `:N`
    // truncation then applies to the alt-resolved value, not the primary.
    #[test]
    fn alt_resolved_value_is_truncated() {
        // xff absent → alt user-agent "curl/8.20.0" → truncate to 4 bytes.
        let f = parse_format("%REQ(X-FORWARDED-FOR?USER-AGENT):4%").unwrap();
        assert_eq!(CompiledFormat::new(f).render(&rec()), "curl");
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
        assert_eq!(CompiledFormat::new(f).render(&rec()), ""); // user_agent present, truncated to 0 bytes
    }

    // ── Task 8: %DYNAMIC_METADATA(namespace:key)% ───────────────────────────
    // §A2/A3/A4-LOCKED against envoyproxy/envoy:v1.33.0.

    // Parses to the two-segment variant WITHOUT lowercasing (case-sensitive).
    #[test]
    fn parses_dynamic_metadata() {
        let segs = parse_format("%DYNAMIC_METADATA(envoy.test:tier)%").unwrap();
        assert_eq!(
            segs,
            vec![Segment::Op(Op::DynamicMetadata {
                namespace: "envoy.test".into(),
                key: "tier".into(),
            })]
        );
    }

    // §A3: a present scalar string value renders RAW, UNQUOTED (`prod`, not `"prod"`).
    #[test]
    fn renders_present_metadata_raw_unquoted() {
        let mut r = rec();
        r.dynamic_metadata
            .entry("envoy.test".into())
            .or_default()
            .insert("tier".into(), "prod".into());
        let f = parse_format("%DYNAMIC_METADATA(envoy.test:tier)%").unwrap();
        assert_eq!(CompiledFormat::new(f).render(&r), "prod"); // no quotes
    }

    // §A4: an absent key OR an absent namespace renders the single dash `-`.
    #[test]
    fn renders_absent_key_and_namespace_dash() {
        let mut r = rec();
        r.dynamic_metadata
            .entry("envoy.test".into())
            .or_default()
            .insert("tier".into(), "prod".into());
        // Absent KEY in a present namespace → `-`.
        let f = parse_format("%DYNAMIC_METADATA(envoy.test:missing)%").unwrap();
        assert_eq!(CompiledFormat::new(f).render(&r), "-");
        // Absent NAMESPACE → `-`.
        let f = parse_format("%DYNAMIC_METADATA(envoy.absent:tier)%").unwrap();
        assert_eq!(CompiledFormat::new(f).render(&r), "-");
    }

    // §A2: a trailing `:N` length suffix is BOOT-FATAL on this operator.
    #[test]
    fn dynamic_metadata_rejects_truncation() {
        assert!(matches!(
            parse_format("%DYNAMIC_METADATA(envoy.test:tier):2%").unwrap_err(),
            FormatParseError::MalformedArgument { .. }
        ));
    }

    // §A2: a no-arg `%DYNAMIC_METADATA%` (no `(...)`) is rejected.
    #[test]
    fn dynamic_metadata_requires_arg() {
        assert!(matches!(
            parse_format("%DYNAMIC_METADATA%").unwrap_err(),
            FormatParseError::MalformedArgument { .. }
        ));
    }

    // §A2: a 1-segment (whole-namespace) or 3+-segment (nested) arg is rejected.
    #[test]
    fn dynamic_metadata_rejects_single_and_nested_segments() {
        assert!(matches!(
            parse_format("%DYNAMIC_METADATA(envoy.test)%").unwrap_err(),
            FormatParseError::MalformedArgument { .. }
        ));
        assert!(matches!(
            parse_format("%DYNAMIC_METADATA(a:b:c)%").unwrap_err(),
            FormatParseError::MalformedArgument { .. }
        ));
    }

    // §A2: namespace/key are CASE-SENSITIVE — they are NOT lowercased.
    #[test]
    fn dynamic_metadata_is_case_sensitive() {
        let mut r = rec();
        r.dynamic_metadata
            .entry("envoy.test".into())
            .or_default()
            .insert("Tier".into(), "prod".into());
        // lowercase `tier` does NOT match the stored `Tier` → `-`.
        let f = parse_format("%DYNAMIC_METADATA(envoy.test:tier)%").unwrap();
        assert_eq!(CompiledFormat::new(f).render(&r), "-");
        // exact-case `Tier` matches → `prod`.
        let f = parse_format("%DYNAMIC_METADATA(envoy.test:Tier)%").unwrap();
        assert_eq!(CompiledFormat::new(f).render(&r), "prod");
    }
}
