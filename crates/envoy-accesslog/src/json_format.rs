//! json_format — the `json_format` access-log encoder (ADR-0092). Compiles a
//! sorted `BTreeMap<String,String>` of key → command-operator value string into
//! a `CompiledJsonFormat` that renders ONE sorted JSON object per request,
//! type-inferring single-operator values (number / string / null) per the
//! v1.33.0 wire behavior. Hand-rolled JSON escaping (no new dependency, D-3.2).
use std::fmt::Write as _;

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

#[cfg(test)]
mod tests {
    use super::*;

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
