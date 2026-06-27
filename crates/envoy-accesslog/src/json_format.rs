//! json_format — the `json_format` access-log encoder (ADR-0092). Compiles a
//! sorted `BTreeMap<String,String>` of key → command-operator value string into
//! a `CompiledJsonFormat` that renders ONE sorted JSON object per request,
//! type-inferring single-operator values (number / string / null) per the
//! v1.33.0 wire behavior. Hand-rolled JSON escaping (no new dependency, D-3.2).
use std::fmt::Write as _;

use crate::command_operator::{render_value_segments, Op, Segment};
use crate::record::AccessLogRecord;

/// Append `s` to `out` with JSON string-body escaping (ADR-0092 §D — matches
/// serde_json: short escapes for `\b \t \n \f \r \" \\`; `\u00XX` for other C0
/// controls; non-ASCII emitted as verbatim UTF-8; `/` NOT escaped). The caller
/// supplies the surrounding `"`.
pub(crate) fn json_escape_into(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{0009}' => out.push_str("\\t"),
            '\u{000A}' => out.push_str("\\n"),
            '\u{000C}' => out.push_str("\\f"),
            '\u{000D}' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
}

/// Encode ONE compiled value (`&[Segment]`) as a JSON value token into `out`
/// (ADR-0092 §B). Single-operator → the operator's native JSON type (numeric →
/// unquoted number; string present → quoted+escaped; absent → `null`).
/// Otherwise (literals / multi-segment / literal-only) → a quoted+escaped string
/// rendered through the existing engine (absent operator → the `-` sentinel).
pub(crate) fn encode_json_value(out: &mut String, segments: &[Segment], r: &AccessLogRecord) {
    if let [Segment::Op(op)] = segments {
        encode_single_op(out, op, r);
    } else {
        let s = render_value_segments(segments, r); // existing render semantics, `-` for absent
        quote(out, &s);
    }
}

fn quote(out: &mut String, s: &str) {
    out.push('"');
    json_escape_into(out, s);
    out.push('"');
}

fn quote_opt(out: &mut String, v: Option<&str>) {
    match v {
        Some(s) => quote(out, s),
        None => out.push_str("null"),
    }
}

fn encode_single_op(out: &mut String, op: &Op, r: &AccessLogRecord) {
    use crate::command_operator::{resolve_req, resolve_resp, truncate_bytes};
    match op {
        // numeric, always present → unquoted number
        Op::ResponseCode => {
            let _ = write!(out, "{}", r.response_code);
        }
        Op::BytesReceived => {
            let _ = write!(out, "{}", r.bytes_received);
        }
        Op::BytesSent => {
            let _ = write!(out, "{}", r.bytes_sent);
        }
        Op::Duration => {
            let _ = write!(out, "{}", r.duration.as_millis());
        }
        // always-present strings → quoted
        Op::Protocol => quote(out, &r.protocol),
        Op::ResponseFlags => quote(out, &r.response_flags),
        Op::StartTime => quote(out, &crate::format_iso8601(r.start_time)),
        // Option-backed → null when absent, else quoted
        Op::UpstreamHost => quote_opt(out, r.upstream_host.as_deref()),
        Op::DynamicMetadata { namespace, key } => quote_opt(
            out,
            r.dynamic_metadata
                .get(namespace)
                .and_then(|m| m.get(key))
                .map(String::as_str),
        ),
        Op::Req {
            name,
            alt,
            truncate,
        } => {
            let v = resolve_req(name, r)
                .or_else(|| alt.as_deref().and_then(|a| resolve_req(a, r)))
                .map(|s| truncate_bytes(s, *truncate));
            quote_opt(out, v);
        }
        Op::Resp {
            name,
            alt,
            truncate,
        } => {
            let owned =
                resolve_resp(name, r).or_else(|| alt.as_deref().and_then(|a| resolve_resp(a, r)));
            let v = owned.as_deref().map(|s| truncate_bytes(s, *truncate));
            quote_opt(out, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_operator::parse_format;
    use crate::record::AccessLogRecord;
    use std::time::{Duration, UNIX_EPOCH};

    // Deterministic record mirroring `command_operator::tests::rec()`.
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
            dynamic_metadata: std::collections::BTreeMap::new(),
        }
    }

    fn enc(value_fmt: &str, r: &AccessLogRecord) -> String {
        let mut out = String::new();
        encode_json_value(&mut out, &parse_format(value_fmt).unwrap(), r);
        out
    }

    #[test]
    fn single_numeric_operator_emits_unquoted_number() {
        let r = rec();
        assert_eq!(enc("%RESPONSE_CODE%", &r), "200");
        assert_eq!(enc("%BYTES_SENT%", &r), "433");
        assert_eq!(enc("%BYTES_RECEIVED%", &r), "16");
        assert_eq!(enc("%DURATION%", &r), "0");
    }

    #[test]
    fn single_string_operator_emits_quoted_string() {
        let r = rec();
        assert_eq!(enc("%REQ(:METHOD)%", &r), "\"POST\"");
        assert_eq!(enc("%PROTOCOL%", &r), "\"HTTP/1.1\"");
        assert_eq!(enc("%RESPONSE_FLAGS%", &r), "\"-\"");
    }

    #[test]
    fn single_absent_operator_emits_null() {
        let r = rec(); // forwarded_for None, upstream_service_time None
        assert_eq!(enc("%REQ(X-FORWARDED-FOR)%", &r), "null");
        assert_eq!(enc("%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%", &r), "null");
        let mut r2 = rec();
        r2.upstream_host = None;
        assert_eq!(enc("%UPSTREAM_HOST%", &r2), "null");
    }

    #[test]
    fn mixed_or_literal_emits_quoted_string_with_dash_sentinel() {
        let r = rec();
        assert_eq!(enc("code-%RESPONSE_CODE%", &r), "\"code-200\""); // mixed → string
        assert_eq!(enc("x=%REQ(X-FORWARDED-FOR)%", &r), "\"x=-\""); // absent op inside string → `-`
        assert_eq!(enc("1", &r), "\"1\""); // literal-only → quoted string
    }

    #[test]
    fn escapes_per_json_rules() {
        let cases = [
            ("ab", "ab"),                 // plain
            ("a\"b", "a\\\"b"),           // quote
            ("a\\b", "a\\\\b"),           // backslash
            ("a\nb", "a\\nb"),            // newline short escape
            ("a\tb", "a\\tb"),            // tab short escape
            ("a\u{0001}b", "a\\u0001b"),  // other C0 control → \u00XX
            ("a/b", "a/b"),               // forward slash NOT escaped
            ("café", "café"),             // non-ASCII verbatim UTF-8
        ];
        for (input, want) in cases {
            let mut out = String::new();
            json_escape_into(&mut out, input);
            assert_eq!(out, want, "input {input:?}");
        }
    }
}
