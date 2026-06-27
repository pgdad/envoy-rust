//! json_format — the `json_format` access-log encoder (ADR-0092). Compiles a
//! sorted `BTreeMap<String,String>` of key → command-operator value string into
//! a `CompiledJsonFormat` that renders ONE sorted JSON object per request,
//! type-inferring single-operator values (number / string / null) per the
//! v1.33.0 wire behavior. Hand-rolled JSON escaping (no new dependency, D-3.2).
use std::fmt::Write as _;

use crate::command_operator::{FormatParseError, Op, Segment, parse_format, render_value_segments};
use crate::record::AccessLogRecord;

/// An accesslog-side MIRROR of `envoy_config::JsonFormatValue` — the recursive
/// `json_format` config value (ADR-0094 §A–§D). This crate must NOT depend on
/// `envoy-config` (the dependency direction is `envoy-config` → `envoy-accesslog`;
/// a reverse edge would be a cycle), so the caller (`envoy-http1`'s HCM bridge)
/// maps the config `JsonFormatValue` into this mirror at the `from_map` call site.
///
/// Variants: `Null`/`Bool` literal leaves (emitted native-typed, §D);
/// `Format` a command-operator string (compiled per-leaf, §C); `Array` an
/// ordered list (config order, §B); `Object` a nested map (keys sorted, §A).
/// NUMERIC literals are NOT representable (rejected at config-parse — CF-39-1).
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValueInput {
    Null,
    Bool(bool),
    Format(String),
    Array(Vec<JsonValueInput>),
    Object(std::collections::BTreeMap<String, JsonValueInput>),
}

/// A compiled `json_format` value (ADR-0094). `Leaf` holds the phase-38 compiled
/// command-operator segments (rendered via the EXISTING `encode_json_value`
/// leaf helper, VERBATIM); `Bool`/`Null` carry literal leaves emitted native-typed
/// (§D); `Object` sorts keys at every level (§A); `Array` preserves config order (§B).
#[derive(Debug, Clone, PartialEq)]
pub enum CompiledJsonValue {
    Null,
    Bool(bool),
    Leaf(Vec<Segment>),
    Array(Vec<CompiledJsonValue>),
    Object(std::collections::BTreeMap<String, CompiledJsonValue>),
}

impl CompiledJsonValue {
    /// Compile a `JsonValueInput` tree: `Format`→`Leaf(parse_format(s)?)`,
    /// `Bool`/`Null` carried verbatim, `Object`/`Array` recurse (returning the
    /// first `FormatParseError`, surfaced at config-load as `InvalidAccessLogFormat`).
    fn compile(value: &JsonValueInput) -> Result<Self, FormatParseError> {
        Ok(match value {
            JsonValueInput::Null => CompiledJsonValue::Null,
            JsonValueInput::Bool(b) => CompiledJsonValue::Bool(*b),
            JsonValueInput::Format(s) => CompiledJsonValue::Leaf(parse_format(s)?),
            JsonValueInput::Array(items) => CompiledJsonValue::Array(
                items
                    .iter()
                    .map(CompiledJsonValue::compile)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            JsonValueInput::Object(map) => {
                let mut compiled = std::collections::BTreeMap::new();
                for (k, v) in map {
                    compiled.insert(k.clone(), CompiledJsonValue::compile(v)?);
                }
                CompiledJsonValue::Object(compiled)
            }
        })
    }

    /// Render this value as a JSON token into `out` (ADR-0094 §C/§D/§E/§F). The
    /// recursion is purely structural; `Leaf` defers to the phase-38
    /// `encode_json_value` helper VERBATIM. No inter-element/inter-level `\n`.
    ///
    /// `omit_empty` (ADR-0096 §B/§D) threads recursively to every `Leaf`: a
    /// multi-segment leaf's absent operator renders as `""` (not `-`). The
    /// single-operator-typed path (`encode_single_op` → `null`) is UNAFFECTED (§C).
    fn render_into(&self, out: &mut String, record: &AccessLogRecord, omit_empty: bool) {
        match self {
            CompiledJsonValue::Null => out.push_str("null"),
            CompiledJsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            CompiledJsonValue::Leaf(segments) => {
                encode_json_value(out, segments, record, omit_empty)
            }
            CompiledJsonValue::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.render_into(out, record, omit_empty);
                }
                out.push(']');
            }
            CompiledJsonValue::Object(map) => {
                out.push('{');
                for (i, (key, value)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push('"');
                    json_escape_into(out, key);
                    out.push_str("\":");
                    value.render_into(out, record, omit_empty);
                }
                out.push('}');
            }
        }
    }
}

/// A compiled `json_format`: a top-level sorted (BTreeMap) key → recursive
/// `CompiledJsonValue` (ADR-0094 §A). `render` assembles ONE sorted JSON object
/// per record (the top level is always an object, as Envoy's `Struct` is).
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledJsonFormat {
    map: std::collections::BTreeMap<String, CompiledJsonValue>,
    /// `omit_empty_values` (ADR-0096 §B/§D): threaded recursively into every
    /// `Leaf`'s multi-segment render. Default `false`. The single-op `null` path
    /// (`encode_single_op`) is UNAFFECTED (§C).
    omit_empty: bool,
}

impl CompiledJsonFormat {
    /// Compile each value via `CompiledJsonValue::compile` (recursing nested
    /// objects/lists, compiling each `Format` leaf). Returns the first
    /// `FormatParseError` (surfaced at config-load as `InvalidAccessLogFormat`).
    /// The resulting format has `omit_empty=false`; use [`Self::with_omit_empty`].
    pub fn from_map(
        map: &std::collections::BTreeMap<String, JsonValueInput>,
    ) -> Result<Self, FormatParseError> {
        let mut compiled = std::collections::BTreeMap::new();
        for (k, v) in map {
            compiled.insert(k.clone(), CompiledJsonValue::compile(v)?);
        }
        Ok(Self {
            map: compiled,
            omit_empty: false,
        })
    }

    /// Builder setter for the `omit_empty_values` flag (ADR-0096 §B); the HCM
    /// bridge calls this from the config `SubstitutionFormatString`.
    pub fn with_omit_empty(mut self, omit_empty: bool) -> Self {
        self.omit_empty = omit_empty;
        self
    }

    /// Render ONE sorted JSON object + trailing `\n` (ADR-0094 §A/§E). The
    /// nested structure is emitted inline; only this top-level render appends
    /// the single `\n`.
    pub fn render(&self, record: &AccessLogRecord) -> String {
        let mut out = String::with_capacity(64 + self.map.len() * 16);
        out.push('{');
        for (i, (key, value)) in self.map.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('"');
            json_escape_into(&mut out, key);
            out.push_str("\":");
            value.render_into(&mut out, record, self.omit_empty);
        }
        out.push_str("}\n");
        out
    }
}

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
pub(crate) fn encode_json_value(
    out: &mut String,
    segments: &[Segment],
    r: &AccessLogRecord,
    omit_empty: bool,
) {
    if let [Segment::Op(op)] = segments {
        // §C carve-out: the single-operator-typed path is UNAFFECTED by omit_empty
        // (an absent single op stays `null`, NOT `""`).
        encode_single_op(out, op, r);
    } else {
        // §B: a multi-segment / literal leaf renders an absent op as `""` when
        // omit_empty (else the `-` sentinel).
        let s = render_value_segments(segments, r, omit_empty);
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
        Op::RouteName => quote_opt(out, r.route_name.as_deref()),
        Op::ResponseCodeDetails => quote_opt(out, r.response_code_details.as_deref()),
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
            upstream_cluster: None,
            route_name: None,
            response_code_details: None,
            dynamic_metadata: std::collections::BTreeMap::new(),
        }
    }

    fn enc(value_fmt: &str, r: &AccessLogRecord) -> String {
        let mut out = String::new();
        encode_json_value(&mut out, &parse_format(value_fmt).unwrap(), r, false);
        out
    }

    // Record matching the fixture-0046 probe (ADR-0092 §F): GET /, HTTP/1.1, 200,
    // flags "-", bytes_received 0, bytes_sent 3, upstream_host None.
    fn fixture_record() -> AccessLogRecord {
        AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(0),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: None,
            upstream_host: None,
            upstream_cluster: None,
            route_name: None,
            response_code_details: None,
            dynamic_metadata: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn renders_authoritative_fixture_line() {
        let r = fixture_record();
        let mut map = std::collections::BTreeMap::new();
        for (k, v) in [
            ("method", "%REQ(:METHOD)%"),
            ("path", "%REQ(:PATH)%"),
            ("protocol", "%PROTOCOL%"),
            ("status", "%RESPONSE_CODE%"),
            ("flags", "%RESPONSE_FLAGS%"),
            ("bytes_rcvd", "%BYTES_RECEIVED%"),
            ("bytes_sent", "%BYTES_SENT%"),
            ("upstream", "%UPSTREAM_HOST%"),
            ("mixed", "code-%RESPONSE_CODE%"),
        ] {
            map.insert(k.to_string(), JsonValueInput::Format(v.to_string()));
        }
        let cjf = CompiledJsonFormat::from_map(&map).unwrap();
        assert_eq!(
            cjf.render(&r),
            "{\"bytes_rcvd\":0,\"bytes_sent\":3,\"flags\":\"-\",\"method\":\"GET\",\"mixed\":\"code-200\",\"path\":\"/\",\"protocol\":\"HTTP/1.1\",\"status\":200,\"upstream\":null}\n"
        );
    }

    #[test]
    fn empty_map_renders_empty_object() {
        let empty: std::collections::BTreeMap<String, JsonValueInput> =
            std::collections::BTreeMap::new();
        let cjf = CompiledJsonFormat::from_map(&empty).unwrap();
        assert_eq!(cjf.render(&fixture_record()), "{}\n"); // ADR-0092 §E
    }

    #[test]
    fn key_is_json_escaped() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "a\"b".to_string(),
            JsonValueInput::Format("%PROTOCOL%".to_string()),
        );
        let cjf = CompiledJsonFormat::from_map(&map).unwrap();
        assert_eq!(cjf.render(&fixture_record()), "{\"a\\\"b\":\"HTTP/1.1\"}\n");
    }

    #[test]
    fn from_map_rejects_malformed_operator() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "a".to_string(),
            JsonValueInput::Format("%NOPE%".to_string()),
        );
        assert!(CompiledJsonFormat::from_map(&map).is_err());
    }

    // --- Phase 39 T3 (ADR-0094): recursive compile via JsonValueInput mirror ---

    #[test]
    fn from_map_compiles_recursive_tree() {
        let mut obj = std::collections::BTreeMap::new();
        obj.insert(
            "method".to_string(),
            JsonValueInput::Format("%REQ(:METHOD)%".to_string()),
        );
        obj.insert(
            "code".to_string(),
            JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        let mut map = std::collections::BTreeMap::new();
        map.insert("obj".to_string(), JsonValueInput::Object(obj));
        map.insert(
            "list".to_string(),
            JsonValueInput::Array(vec![
                JsonValueInput::Format("%PROTOCOL%".to_string()),
                JsonValueInput::Bool(true),
                JsonValueInput::Null,
            ]),
        );
        map.insert("flag".to_string(), JsonValueInput::Bool(false));
        map.insert("missing".to_string(), JsonValueInput::Null);
        let cjf = CompiledJsonFormat::from_map(&map).expect("recursive compile Ok");
        // Bool/Null carried verbatim; Format compiled to Leaf.
        assert_eq!(cjf.map["flag"], CompiledJsonValue::Bool(false));
        assert_eq!(cjf.map["missing"], CompiledJsonValue::Null);
        assert!(matches!(cjf.map["obj"], CompiledJsonValue::Object(_)));
        assert!(matches!(cjf.map["list"], CompiledJsonValue::Array(_)));
    }

    #[test]
    fn from_map_rejects_malformed_nested_operator() {
        // ADR-0094 §G: a malformed operator in a NESTED leaf returns the error.
        let mut obj = std::collections::BTreeMap::new();
        obj.insert(
            "b".to_string(),
            JsonValueInput::Format("%NOPE%".to_string()),
        );
        let mut map = std::collections::BTreeMap::new();
        map.insert("a".to_string(), JsonValueInput::Object(obj));
        assert!(CompiledJsonFormat::from_map(&map).is_err());
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

    // --- phase 41 (ADR-0098 §C): %ROUTE_NAME% json typed render ---

    #[test]
    fn route_name_single_op_present_emits_quoted_string() {
        let mut r = rec();
        r.route_name = Some("myroute".into());
        assert_eq!(enc("%ROUTE_NAME%", &r), "\"myroute\"");
    }

    #[test]
    fn route_name_single_op_absent_emits_null() {
        let mut r = rec();
        r.route_name = None;
        assert_eq!(enc("%ROUTE_NAME%", &r), "null");
    }

    #[test]
    fn route_name_mixed_emits_quoted_string_with_dash_sentinel() {
        let mut named = rec();
        named.route_name = Some("myroute".into());
        assert_eq!(enc("r=%ROUTE_NAME%", &named), "\"r=myroute\"");

        let mut absent = rec();
        absent.route_name = None;
        assert_eq!(enc("r=%ROUTE_NAME%", &absent), "\"r=-\"");
    }

    // --- phase 42 (ADR-0099): %RESPONSE_CODE_DETAILS% json typed render ---

    #[test]
    fn response_code_details_single_op_present_emits_quoted_string() {
        let mut r = rec();
        r.response_code_details = Some("direct_response".into());
        assert_eq!(enc("%RESPONSE_CODE_DETAILS%", &r), "\"direct_response\"");
    }

    #[test]
    fn response_code_details_single_op_absent_emits_null() {
        let mut r = rec();
        r.response_code_details = None;
        assert_eq!(enc("%RESPONSE_CODE_DETAILS%", &r), "null");
    }

    #[test]
    fn response_code_details_mixed_emits_quoted_string_with_dash_sentinel() {
        let mut detailed = rec();
        detailed.response_code_details = Some("direct_response".into());
        assert_eq!(
            enc("d=%RESPONSE_CODE_DETAILS%", &detailed),
            "\"d=direct_response\""
        );

        let mut absent = rec();
        absent.response_code_details = None;
        assert_eq!(enc("d=%RESPONSE_CODE_DETAILS%", &absent), "\"d=-\"");
    }

    #[test]
    fn escapes_per_json_rules() {
        let cases = [
            ("ab", "ab"),                // plain
            ("a\"b", "a\\\"b"),          // quote
            ("a\\b", "a\\\\b"),          // backslash
            ("a\nb", "a\\nb"),           // newline short escape
            ("a\tb", "a\\tb"),           // tab short escape
            ("a\u{0001}b", "a\\u0001b"), // other C0 control → \u00XX
            ("a/b", "a/b"),              // forward slash NOT escaped
            ("café", "café"),            // non-ASCII verbatim UTF-8
        ];
        for (input, want) in cases {
            let mut out = String::new();
            json_escape_into(&mut out, input);
            assert_eq!(out, want, "input {input:?}");
        }
    }

    // --- Phase 39 T4 (ADR-0094 §A–§H): recursive byte-exact render ---

    fn fmt(s: &str) -> JsonValueInput {
        JsonValueInput::Format(s.to_string())
    }
    fn obj(pairs: &[(&str, JsonValueInput)]) -> JsonValueInput {
        let mut m = std::collections::BTreeMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v.clone());
        }
        JsonValueInput::Object(m)
    }
    fn top(pairs: &[(&str, JsonValueInput)]) -> CompiledJsonFormat {
        let mut m = std::collections::BTreeMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v.clone());
        }
        CompiledJsonFormat::from_map(&m).unwrap()
    }

    #[test]
    fn renders_authoritative_nested_fixture_line() {
        // ADR-0094 §H — the live-captured CASE-1 byte-exact line. Keys sorted at
        // BOTH levels (§A); list order preserved (§B); at-depth type inference
        // (§C: nested %RESPONSE_CODE%→200, %UPSTREAM_HOST% absent in list→null,
        // mixed→"code-200"); compact separators + ONE trailing \n (§E).
        let r = fixture_record();
        let cjf = top(&[
            ("zouter", fmt("%PROTOCOL%")),
            (
                "arequest",
                obj(&[
                    ("method", fmt("%REQ(:METHOD)%")),
                    ("zpath", fmt("%REQ(:PATH)%")),
                    ("aaa", fmt("%RESPONSE_CODE%")),
                ]),
            ),
            (
                "blist",
                JsonValueInput::Array(vec![
                    fmt("%REQ(:METHOD)%"),
                    fmt("%RESPONSE_CODE%"),
                    fmt("%UPSTREAM_HOST%"),
                ]),
            ),
            ("mtop", fmt("code-%RESPONSE_CODE%")),
        ]);
        assert_eq!(
            cjf.render(&r),
            "{\"arequest\":{\"aaa\":200,\"method\":\"GET\",\"zpath\":\"/\"},\"blist\":[\"GET\",200,null],\"mtop\":\"code-200\",\"zouter\":\"HTTP/1.1\"}\n"
        );
    }

    #[test]
    fn flat_round_trip_byte_unchanged_through_recursive_encoder() {
        // The phase-38 flat line (fixture 0046) must render BYTE-IDENTICAL through
        // the recursive encoder — depth-1 == flat (the regression witness).
        let r = fixture_record();
        let cjf = top(&[
            ("method", fmt("%REQ(:METHOD)%")),
            ("path", fmt("%REQ(:PATH)%")),
            ("protocol", fmt("%PROTOCOL%")),
            ("status", fmt("%RESPONSE_CODE%")),
            ("flags", fmt("%RESPONSE_FLAGS%")),
            ("bytes_rcvd", fmt("%BYTES_RECEIVED%")),
            ("bytes_sent", fmt("%BYTES_SENT%")),
            ("upstream", fmt("%UPSTREAM_HOST%")),
            ("mixed", fmt("code-%RESPONSE_CODE%")),
        ]);
        assert_eq!(
            cjf.render(&r),
            "{\"bytes_rcvd\":0,\"bytes_sent\":3,\"flags\":\"-\",\"method\":\"GET\",\"mixed\":\"code-200\",\"path\":\"/\",\"protocol\":\"HTTP/1.1\",\"status\":200,\"upstream\":null}\n"
        );
    }

    #[test]
    fn per_level_keys_sorted_independently() {
        // §A — each object level sorts by UTF-8 byte order independently.
        let r = fixture_record();
        let cjf = top(&[(
            "outer",
            obj(&[
                ("zzz", fmt("%PROTOCOL%")),
                ("aaa", fmt("%RESPONSE_CODE%")),
                ("mmm", fmt("%REQ(:METHOD)%")),
            ]),
        )]);
        assert_eq!(
            cjf.render(&r),
            "{\"outer\":{\"aaa\":200,\"mmm\":\"GET\",\"zzz\":\"HTTP/1.1\"}}\n"
        );
    }

    #[test]
    fn list_order_preserved_not_sorted() {
        // §B — list order = config order (NOT sorted).
        let r = fixture_record();
        let cjf = top(&[(
            "l",
            JsonValueInput::Array(vec![
                fmt("%PROTOCOL%"),
                fmt("%REQ(:METHOD)%"),
                fmt("%RESPONSE_CODE%"),
            ]),
        )]);
        assert_eq!(cjf.render(&r), "{\"l\":[\"HTTP/1.1\",\"GET\",200]}\n");
    }

    #[test]
    fn bool_and_null_literal_leaves_native_typed() {
        // §D — bool/null literal leaves emit native-typed (unquoted).
        let r = fixture_record();
        let cjf = top(&[
            ("f", JsonValueInput::Bool(false)),
            ("n", JsonValueInput::Null),
            ("t", JsonValueInput::Bool(true)),
        ]);
        assert_eq!(cjf.render(&r), "{\"f\":false,\"n\":null,\"t\":true}\n");
    }

    #[test]
    fn empty_nested_object_and_list() {
        // §F — empty {} → {}, empty [] → [].
        let r = fixture_record();
        let cjf = top(&[
            (
                "e",
                JsonValueInput::Object(std::collections::BTreeMap::new()),
            ),
            ("l", JsonValueInput::Array(vec![])),
        ]);
        assert_eq!(cjf.render(&r), "{\"e\":{},\"l\":[]}\n");
    }

    #[test]
    fn absent_operator_leaf_in_list_is_null() {
        // §F — an absent-operator leaf in a list → null in place.
        let mut r = fixture_record();
        r.upstream_host = None;
        let cjf = top(&[(
            "l",
            JsonValueInput::Array(vec![fmt("%UPSTREAM_HOST%"), fmt("%RESPONSE_CODE%")]),
        )]);
        assert_eq!(cjf.render(&r), "{\"l\":[null,200]}\n");
    }

    #[test]
    fn depth_three_nesting() {
        // Deep nesting (depth-3), at-depth numeric inference still applies.
        let r = fixture_record();
        let cjf = top(&[("d1", obj(&[("d2", obj(&[("d3", fmt("%RESPONSE_CODE%"))]))]))]);
        assert_eq!(cjf.render(&r), "{\"d1\":{\"d2\":{\"d3\":200}}}\n");
    }

    #[test]
    fn list_of_objects() {
        // CASE-5 — a list of objects (each object internally sorted).
        let r = fixture_record();
        let cjf = top(&[(
            "objlist",
            JsonValueInput::Array(vec![obj(&[
                ("k", fmt("%REQ(:METHOD)%")),
                ("z", fmt("%PROTOCOL%")),
            ])]),
        )]);
        assert_eq!(
            cjf.render(&r),
            "{\"objlist\":[{\"k\":\"GET\",\"z\":\"HTTP/1.1\"}]}\n"
        );
    }

    #[test]
    fn nested_key_and_value_escaping() {
        // Reuse the phase-38 escaping at depth: a nested key + value both escaped.
        let r = fixture_record();
        let cjf = top(&[("o", obj(&[("a\"b", fmt("x\ty"))]))]);
        assert_eq!(cjf.render(&r), "{\"o\":{\"a\\\"b\":\"x\\ty\"}}\n");
    }

    #[test]
    fn empty_value_string_leaf_renders_empty_quoted() {
        // M38-3 fold — an empty value-string leaf ("") → "".
        let r = fixture_record();
        let cjf = top(&[("e", fmt(""))]);
        assert_eq!(cjf.render(&r), "{\"e\":\"\"}\n");
        // and nested:
        let cjf2 = top(&[("o", obj(&[("e", fmt(""))]))]);
        assert_eq!(cjf2.render(&r), "{\"o\":{\"e\":\"\"}}\n");
    }

    // --- phase 40 t3 (ADR-0096 §B/§C/§D): omit_empty in the json render ---

    fn top_omit(pairs: &[(&str, JsonValueInput)], omit_empty: bool) -> CompiledJsonFormat {
        let mut m = std::collections::BTreeMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v.clone());
        }
        CompiledJsonFormat::from_map(&m)
            .unwrap()
            .with_omit_empty(omit_empty)
    }

    #[test]
    fn omit_empty_swaps_dash_in_multi_segment_json_leaf() {
        // §B — a multi-segment leaf with an absent op: omit=false "pre--", omit=true "pre-".
        let mut r = fixture_record();
        r.forwarded_for = None;
        let off = top_omit(&[("e", fmt("pre-%REQ(X-FORWARDED-FOR)%"))], false);
        assert_eq!(off.render(&r), "{\"e\":\"pre--\"}\n");
        let on = top_omit(&[("e", fmt("pre-%REQ(X-FORWARDED-FOR)%"))], true);
        assert_eq!(on.render(&r), "{\"e\":\"pre-\"}\n");
    }

    #[test]
    fn omit_empty_leaves_single_op_null_unchanged() {
        // §C — a single absent op routes through encode_single_op → null under BOTH.
        let mut r = fixture_record();
        r.upstream_host = None;
        let off = top_omit(&[("u", fmt("%UPSTREAM_HOST%"))], false);
        assert_eq!(off.render(&r), "{\"u\":null}\n");
        let on = top_omit(&[("u", fmt("%UPSTREAM_HOST%"))], true);
        assert_eq!(on.render(&r), "{\"u\":null}\n"); // NOT "" — §C carve-out
    }

    #[test]
    fn omit_empty_applies_recursively_single_op_null_at_depth() {
        // §D / CASE-4 — nested objects + lists: mixed leaves get the swap, single-op
        // leaves stay null at depth.
        let mut r = fixture_record();
        r.forwarded_for = None;
        let cjf = top_omit(
            &[
                (
                    "nested",
                    obj(&[
                        ("mixed", fmt("v=%REQ(X-FORWARDED-FOR)%")),
                        ("single", fmt("%REQ(X-FORWARDED-FOR)%")),
                    ]),
                ),
                (
                    "arr",
                    JsonValueInput::Array(vec![
                        fmt("a=%REQ(X-FORWARDED-FOR)%"),
                        fmt("%REQ(X-FORWARDED-FOR)%"),
                    ]),
                ),
            ],
            true,
        );
        assert_eq!(
            cjf.render(&r),
            "{\"arr\":[\"a=\",null],\"nested\":{\"mixed\":\"v=\",\"single\":null}}\n"
        );
    }

    #[test]
    fn omit_empty_default_off_round_trip_byte_unchanged() {
        // The phase-39 nested fixture line must be byte-identical with omit=false.
        let r = fixture_record();
        let cjf = top_omit(
            &[
                ("zouter", fmt("%PROTOCOL%")),
                (
                    "arequest",
                    obj(&[
                        ("method", fmt("%REQ(:METHOD)%")),
                        ("zpath", fmt("%REQ(:PATH)%")),
                        ("aaa", fmt("%RESPONSE_CODE%")),
                    ]),
                ),
                (
                    "blist",
                    JsonValueInput::Array(vec![
                        fmt("%REQ(:METHOD)%"),
                        fmt("%RESPONSE_CODE%"),
                        fmt("%UPSTREAM_HOST%"),
                    ]),
                ),
                ("mtop", fmt("code-%RESPONSE_CODE%")),
            ],
            false,
        );
        assert_eq!(
            cjf.render(&r),
            "{\"arequest\":{\"aaa\":200,\"method\":\"GET\",\"zpath\":\"/\"},\"blist\":[\"GET\",200,null],\"mtop\":\"code-200\",\"zouter\":\"HTTP/1.1\"}\n"
        );
    }

    #[test]
    fn m38_4_typed_path_gaps_in_nested_position() {
        // M38-4 fold — exercise the typed-leaf path edges INSIDE a nested object:
        //  - %REQ(...):N% truncation
        //  - %REQ(MISSING?:METHOD)% alternate (?ALT)
        //  - a non-zero %DURATION% (numeric)
        //  - a control char in a rendered value (escaped via the leaf path)
        let mut r = rec(); // method POST, user_agent curl/8.20.0
        r.duration = std::time::Duration::from_millis(42);
        r.user_agent = Some("a\u{0001}b".into());
        let cjf = top(&[(
            "o",
            obj(&[
                ("trunc", fmt("%REQ(:METHOD):2%")),       // "POST" → "PO"
                ("alt", fmt("%REQ(X-MISSING?:METHOD)%")), // absent → :METHOD → "POST"
                ("dur", fmt("%DURATION%")),               // 42 (unquoted)
                ("ctrl", fmt("%REQ(USER-AGENT)%")),       // "ab" escaped
            ]),
        )]);
        assert_eq!(
            cjf.render(&r),
            "{\"o\":{\"alt\":\"POST\",\"ctrl\":\"a\\u0001b\",\"dur\":42,\"trunc\":\"PO\"}}\n"
        );
    }
}
