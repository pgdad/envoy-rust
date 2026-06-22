//! Access-log line equivalence primitives for the differential
//! harness. Lands per 06.2 SPEC §3 D4.2.b + signpost 8 (hand-rolled
//! tokenizer per architecture decision 9; no `regex` dep).
//!
//! The tokenizer parses the Envoy default-format access-log line into
//! its 14 component tokens (with quoting/bracket awareness). The
//! per-token rule enum (`AccessLogLineRule`) drives the equivalence
//! check; the `assert_access_log_lines_equivalent` helper applies the
//! per-token rules across both proxies' lines.

use serde::Deserialize;

/// Per-token rule for the Envoy default-format access-log line.
/// One rule per token; the rules slot in the same 1:1 order as the
/// 14 tokens emitted by `envoy_accesslog::default_format::format`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "rule", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccessLogLineRule {
    /// Token must match `value` byte-for-byte. Used for `value-exact`
    /// tokens per BEHAVIOR_CONTRACT.md `Access log field mapping`
    /// (`%REQ(:METHOD)%`, `%RESPONSE_CODE%`, etc.).
    Exact { value: String },

    /// Token must parse as ISO-8601 `YYYY-MM-DDTHH:MM:SS.sssZ`.
    /// Used for `%START_TIME%` (name-required, value-may-differ).
    Iso8601Format,

    /// Token must parse as a non-negative integer (decimal
    /// milliseconds). Used for `%DURATION%` and present-on-both-
    /// sides `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%`.
    DurationMs,

    /// Token may be anything (used for fields not covered by 06.2;
    /// reserved for forward-compat).
    Wildcard,
}

/// Tokenize a single Envoy default-format access-log line into its
/// component tokens. Handles the `[%START_TIME%]` bracketing, the
/// `"..."` quoted-token boundaries, and the unquoted-token
/// whitespace separators.
///
/// The 14-token shape per Envoy v1.33's documented default format:
///
///   1. `%START_TIME%` (bracket-wrapped, e.g. `[2024-01-01T00:00:00.000Z]`)
///   2. `%REQ(:METHOD)%` (first word inside the quoted request-line)
///   3. `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` (second word)
///   4. `%PROTOCOL%` (third word; closing of the quoted request-line)
///   5. `%RESPONSE_CODE%` (unquoted)
///   6. `%RESPONSE_FLAGS%` (unquoted)
///   7. `%BYTES_RECEIVED%` (unquoted)
///   8. `%BYTES_SENT%` (unquoted)
///   9. `%DURATION%` (unquoted)
///   10. `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%` (unquoted)
///   11. `%REQ(X-FORWARDED-FOR)%` (quoted)
///   12. `%REQ(USER-AGENT)%` (quoted)
///   13. `%REQ(X-REQUEST-ID)%` (quoted)
///   14. `%REQ(:AUTHORITY)%` (quoted)
///   15. `%UPSTREAM_HOST%` (quoted)
///
/// Returns a Vec<String> of 15 tokens (the rule enum is 14-shape but
/// the bracket-wrapped START_TIME counts as one token, and the
/// quoted request-line yields 3 tokens for method/path/protocol).
pub fn tokenize_default_format(line: &str) -> Result<Vec<String>, String> {
    let mut tokens: Vec<String> = Vec::with_capacity(15);
    let bytes = line.as_bytes();
    let mut i = 0usize;

    // 1. START_TIME bracket.
    if i >= bytes.len() || bytes[i] != b'[' {
        return Err(format!("expected '[' at offset {}; line: {}", i, line));
    }
    i += 1;
    let start_time_begin = i;
    while i < bytes.len() && bytes[i] != b']' {
        i += 1;
    }
    if i >= bytes.len() {
        return Err(format!("unterminated '[...]' in line: {}", line));
    }
    tokens.push(line[start_time_begin..i].to_owned());
    i += 1; // skip ']'

    // Skip the space after ']'.
    if i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    } else {
        return Err(format!(
            "expected ' ' after ']' at offset {}; line: {}",
            i, line
        ));
    }

    // 2-4. Quoted request-line: "METHOD PATH PROTOCOL".
    if i >= bytes.len() || bytes[i] != b'"' {
        return Err(format!(
            "expected '\"' (request-line) at offset {}; line: {}",
            i, line
        ));
    }
    i += 1;
    let req_line_begin = i;
    while i < bytes.len() && bytes[i] != b'"' {
        i += 1;
    }
    if i >= bytes.len() {
        return Err(format!("unterminated request-line quote in line: {}", line));
    }
    let req_line = &line[req_line_begin..i];
    let parts: Vec<&str> = req_line.splitn(3, ' ').collect();
    if parts.len() != 3 {
        return Err(format!(
            "request-line did not split into 3 parts: {:?}",
            req_line
        ));
    }
    tokens.push(parts[0].to_owned()); // method
    tokens.push(parts[1].to_owned()); // path
    tokens.push(parts[2].to_owned()); // protocol
    i += 1; // skip closing '"'

    // 5-10. Six unquoted tokens (status, flags, bytes_received,
    // bytes_sent, duration, upstream_service_time).
    for _ in 0..6 {
        // Skip leading whitespace.
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        let tok_begin = i;
        while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'"' {
            i += 1;
        }
        if tok_begin == i {
            return Err(format!(
                "empty token at offset {} in line: {}",
                tok_begin, line
            ));
        }
        tokens.push(line[tok_begin..i].to_owned());
    }

    // 11-15. Five quoted tokens (forwarded_for, user_agent,
    // request_id, authority, upstream_host).
    for _ in 0..5 {
        // Skip leading whitespace.
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'"' {
            return Err(format!("expected '\"' at offset {}; line: {}", i, line));
        }
        i += 1;
        let tok_begin = i;
        while i < bytes.len() && bytes[i] != b'"' {
            i += 1;
        }
        if i >= bytes.len() {
            return Err(format!("unterminated quoted token in line: {}", line));
        }
        tokens.push(line[tok_begin..i].to_owned());
        i += 1; // skip closing '"'
    }

    Ok(tokens)
}

/// Apply a single per-token rule to a pair of envoy + envoy-rust
/// token values. Returns Err with a descriptive message on
/// mismatch.
pub fn apply_rule(rule: &AccessLogLineRule, envoy: &str, envoy_rust: &str) -> Result<(), String> {
    match rule {
        AccessLogLineRule::Exact { value } => {
            if envoy != value {
                return Err(format!("envoy token {:?} != expected {:?}", envoy, value));
            }
            if envoy_rust != value {
                return Err(format!(
                    "envoy-rust token {:?} != expected {:?}",
                    envoy_rust, value
                ));
            }
        }
        AccessLogLineRule::Iso8601Format => {
            for (side, tok) in [("envoy", envoy), ("envoy-rust", envoy_rust)] {
                if !is_iso8601_format(tok) {
                    return Err(format!(
                        "{} token {:?} does not match ISO-8601 YYYY-MM-DDTHH:MM:SS.sssZ",
                        side, tok
                    ));
                }
            }
        }
        AccessLogLineRule::DurationMs => {
            for (side, tok) in [("envoy", envoy), ("envoy-rust", envoy_rust)] {
                if tok.parse::<u64>().is_err() {
                    return Err(format!("{} token {:?} does not parse as u64 ms", side, tok));
                }
            }
        }
        AccessLogLineRule::Wildcard => {}
    }
    Ok(())
}

fn is_iso8601_format(s: &str) -> bool {
    // YYYY-MM-DDTHH:MM:SS.sssZ — exactly 24 ASCII bytes; positional
    // checks for separators.
    let b = s.as_bytes();
    if b.len() != 24 {
        return false;
    }
    if b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'.'
        || b[23] != b'Z'
    {
        return false;
    }
    let digit = |idx: usize| -> bool { b[idx].is_ascii_digit() };
    for &i in &[0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18, 20, 21, 22] {
        if !digit(i) {
            return false;
        }
    }
    true
}

/// Assert per-token equivalence across two sequences of access-log
/// lines (one per proxy). Each line in both sequences is tokenized
/// via `tokenize_default_format`; the per-token rules are applied
/// pairwise.
pub fn assert_access_log_lines_equivalent(
    envoy_lines: &[String],
    envoy_rust_lines: &[String],
    rules_per_line: &[Vec<AccessLogLineRule>],
) -> Result<(), String> {
    if envoy_lines.len() != envoy_rust_lines.len() {
        return Err(format!(
            "line count mismatch: envoy={} envoy-rust={}",
            envoy_lines.len(),
            envoy_rust_lines.len()
        ));
    }
    if envoy_lines.len() != rules_per_line.len() {
        return Err(format!(
            "rules-per-line count {} != lines count {}",
            rules_per_line.len(),
            envoy_lines.len()
        ));
    }
    for (line_idx, ((envoy_line, envoy_rust_line), line_rules)) in envoy_lines
        .iter()
        .zip(envoy_rust_lines.iter())
        .zip(rules_per_line.iter())
        .enumerate()
    {
        let envoy_tokens = tokenize_default_format(envoy_line)
            .map_err(|e| format!("line {}: envoy tokenize: {}", line_idx, e))?;
        let envoy_rust_tokens = tokenize_default_format(envoy_rust_line)
            .map_err(|e| format!("line {}: envoy-rust tokenize: {}", line_idx, e))?;
        if envoy_tokens.len() != line_rules.len() {
            return Err(format!(
                "line {}: envoy tokenized to {} tokens but {} rules supplied",
                line_idx,
                envoy_tokens.len(),
                line_rules.len()
            ));
        }
        if envoy_rust_tokens.len() != line_rules.len() {
            return Err(format!(
                "line {}: envoy-rust tokenized to {} tokens but {} rules supplied",
                line_idx,
                envoy_rust_tokens.len(),
                line_rules.len()
            ));
        }
        for (tok_idx, ((envoy_tok, envoy_rust_tok), rule)) in envoy_tokens
            .iter()
            .zip(envoy_rust_tokens.iter())
            .zip(line_rules.iter())
            .enumerate()
        {
            apply_rule(rule, envoy_tok, envoy_rust_tok)
                .map_err(|e| format!("line {} token {}: {}", line_idx, tok_idx, e))?;
        }
    }
    Ok(())
}

/// Phase 32 Task 6 (ADR-0079): whole-line byte-exact access-log
/// comparator. Unlike `assert_access_log_lines_equivalent` (which
/// tokenizes the Envoy default format and applies per-token rules), this
/// asserts each emitted line is byte-identical between upstream Envoy and
/// envoy-rust. It is the comparator for fixture 0040's custom
/// `log_format` of DETERMINISTIC command operators — every operator in
/// that format renders the same bytes on both proxies, so a whole-line
/// `==` is the strongest possible assertion.
///
/// Returns `Err` with a descriptive message on the first divergence:
/// a length mismatch (naming both counts) or a line mismatch (naming the
/// line index plus both values).
pub fn assert_access_log_lines_byte_identical(
    envoy: &[String],
    envoy_rust: &[String],
) -> Result<(), String> {
    if envoy.len() != envoy_rust.len() {
        return Err(format!(
            "line count mismatch: envoy={} envoy-rust={}",
            envoy.len(),
            envoy_rust.len()
        ));
    }
    for (idx, (envoy_line, envoy_rust_line)) in envoy.iter().zip(envoy_rust.iter()).enumerate() {
        if envoy_line != envoy_rust_line {
            return Err(format!(
                "line {} not byte-identical: envoy={:?} envoy-rust={:?}",
                idx, envoy_line, envoy_rust_line
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LINE: &str = "[2024-01-01T00:00:00.000Z] \"GET / HTTP/1.1\" 200 - 0 3 5 - \"-\" \"-\" \"-\" \"envoy-rust.test\" \"-\"";

    #[test]
    fn tokenize_default_format_happy_path() {
        let tokens = tokenize_default_format(SAMPLE_LINE).expect("ok");
        assert_eq!(tokens.len(), 15);
        assert_eq!(tokens[0], "2024-01-01T00:00:00.000Z");
        assert_eq!(tokens[1], "GET");
        assert_eq!(tokens[2], "/");
        assert_eq!(tokens[3], "HTTP/1.1");
        assert_eq!(tokens[4], "200");
        assert_eq!(tokens[5], "-");
        assert_eq!(tokens[6], "0");
        assert_eq!(tokens[7], "3");
        assert_eq!(tokens[8], "5");
        assert_eq!(tokens[9], "-");
        assert_eq!(tokens[10], "-");
        assert_eq!(tokens[11], "-");
        assert_eq!(tokens[12], "-");
        assert_eq!(tokens[13], "envoy-rust.test");
        assert_eq!(tokens[14], "-");
    }

    #[test]
    fn tokenize_handles_dash_in_quoted_position() {
        let line = "[2024-01-01T00:00:00.000Z] \"GET / HTTP/1.1\" 200 - 0 3 5 - \"-\" \"-\" \"-\" \"-\" \"-\"";
        let tokens = tokenize_default_format(line).expect("ok");
        assert_eq!(tokens[10], "-");
        assert_eq!(tokens[11], "-");
        assert_eq!(tokens[12], "-");
        assert_eq!(tokens[13], "-");
        assert_eq!(tokens[14], "-");
    }

    #[test]
    fn assert_access_log_lines_equivalent_happy_path() {
        let envoy = vec![SAMPLE_LINE.to_owned()];
        let envoy_rust = vec![SAMPLE_LINE.to_owned()];
        let rules = vec![vec![
            AccessLogLineRule::Iso8601Format, // START_TIME
            AccessLogLineRule::Exact {
                value: "GET".into(),
            },
            AccessLogLineRule::Exact { value: "/".into() },
            AccessLogLineRule::Exact {
                value: "HTTP/1.1".into(),
            },
            AccessLogLineRule::Exact {
                value: "200".into(),
            },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "0".into() },
            AccessLogLineRule::Exact { value: "3".into() },
            AccessLogLineRule::DurationMs, // DURATION
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact {
                value: "envoy-rust.test".into(),
            },
            AccessLogLineRule::Exact { value: "-".into() },
        ]];
        assert_access_log_lines_equivalent(&envoy, &envoy_rust, &rules).expect("ok");
    }

    #[test]
    fn assert_access_log_lines_equivalent_rejects_token_mismatch() {
        let envoy = vec![SAMPLE_LINE.to_owned()];
        let envoy_rust_diff = vec![
            "[2024-01-01T00:00:00.000Z] \"POST / HTTP/1.1\" 200 - 0 3 5 - \"-\" \"-\" \"-\" \"envoy-rust.test\" \"-\"".to_owned()
        ];
        let rules = vec![vec![
            AccessLogLineRule::Iso8601Format,
            AccessLogLineRule::Exact {
                value: "GET".into(),
            }, // mismatch (envoy-rust says POST)
            AccessLogLineRule::Exact { value: "/".into() },
            AccessLogLineRule::Exact {
                value: "HTTP/1.1".into(),
            },
            AccessLogLineRule::Exact {
                value: "200".into(),
            },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "0".into() },
            AccessLogLineRule::Exact { value: "3".into() },
            AccessLogLineRule::DurationMs,
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact {
                value: "envoy-rust.test".into(),
            },
            AccessLogLineRule::Exact { value: "-".into() },
        ]];
        let err = assert_access_log_lines_equivalent(&envoy, &envoy_rust_diff, &rules)
            .expect_err("expected mismatch");
        assert!(err.contains("envoy-rust token"), "err: {}", err);
    }

    // ---- assert_access_log_lines_byte_identical (Phase 32 Task 6) ----

    #[test]
    fn byte_identical_accepts_identical_sequences() {
        let envoy = vec![
            "m=GET p=/ proto=HTTP/1.1 code=200 flags=- rx=0 tx=3 ua=- xff=- auth=envoy-rust.test up=-"
                .to_owned(),
            "m=GET p=/ proto=HTTP/1.1 code=200 flags=- rx=0 tx=3 ua=curl/8.0 xff=203.0.113.7 auth=envoy-rust.test up=-"
                .to_owned(),
        ];
        let envoy_rust = envoy.clone();
        assert_access_log_lines_byte_identical(&envoy, &envoy_rust).expect("ok");
    }

    #[test]
    fn byte_identical_rejects_mutated_line() {
        let envoy = vec![
            "m=GET p=/ proto=HTTP/1.1 code=200 flags=- rx=0 tx=3 ua=- xff=- auth=envoy-rust.test up=-"
                .to_owned(),
        ];
        // envoy-rust diverges on a single byte (code=200 -> code=201).
        let envoy_rust = vec![
            "m=GET p=/ proto=HTTP/1.1 code=201 flags=- rx=0 tx=3 ua=- xff=- auth=envoy-rust.test up=-"
                .to_owned(),
        ];
        let err = assert_access_log_lines_byte_identical(&envoy, &envoy_rust)
            .expect_err("expected mismatch");
        assert!(err.contains("line 0"), "err: {}", err);
        assert!(err.contains("not byte-identical"), "err: {}", err);
    }

    #[test]
    fn byte_identical_rejects_length_mismatch() {
        let envoy = vec!["a".to_owned(), "b".to_owned()];
        let envoy_rust = vec!["a".to_owned()];
        let err = assert_access_log_lines_byte_identical(&envoy, &envoy_rust)
            .expect_err("expected mismatch");
        assert!(err.contains("line count mismatch"), "err: {}", err);
    }
}
