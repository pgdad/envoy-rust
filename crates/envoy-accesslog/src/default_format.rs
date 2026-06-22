//! Envoy default-format access-log line emitter.
//!
//! The default format is now re-expressed THROUGH the command-operator
//! engine: [`DEFAULT_FORMAT`] is the canonical Envoy default-format
//! STRING (verifiable against upstream Envoy v1.33's documentation at
//! the canonical access-log usage page), and `CompiledFormat::default()`
//! parses it once. Unlike the old hand-rolled concatenator, the default
//! STRING carries its OWN trailing `\n` — Envoy emits an `inline_string`
//! VERBATIM with no auto-appended newline, so `FileSink::emit` renders
//! the format verbatim and no longer appends a `\n` of its own.
//!
//! Token sequence (literal separators preserved):
//! `[%START_TIME%] "%REQ(:METHOD)% %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)% %PROTOCOL%"
//!  %RESPONSE_CODE% %RESPONSE_FLAGS% %BYTES_RECEIVED% %BYTES_SENT% %DURATION%
//!  %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)% "%REQ(X-FORWARDED-FOR)%"
//!  "%REQ(USER-AGENT)%" "%REQ(X-REQUEST-ID)%" "%REQ(:AUTHORITY)%" "%UPSTREAM_HOST%"\n`
//!
//! Tokens whose backing fields are `None` (or whose values are not
//! emitted by envoy-rust) render as a literal `-` per Envoy's
//! substitution rule. Quoted positions render as `"-"`.
//!
//! [`legacy_format`] (the old hand-rolled concatenator) is retained
//! ONLY as a `#[cfg(test)]` equivalence oracle: a test asserts that the
//! engine's rendering of [`DEFAULT_FORMAT`] equals `legacy_format(...)`
//! plus a trailing `\n`, proving the default-format re-expression stays
//! byte-identical to the prior output.

use std::fmt::Write as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// `AccessLogRecord` is only referenced by the `#[cfg(test)]` `legacy_format`
// oracle (and the test module); production code here is timestamp-only.
#[cfg(test)]
use crate::record::AccessLogRecord;

/// The canonical Envoy default-format access-log STRING, byte-for-byte
/// matching upstream Envoy v1.33. Note the trailing `\n`: Envoy's
/// default format carries its own newline, and the engine renders it
/// verbatim (no auto-appended `\n` at the sink).
pub const DEFAULT_FORMAT: &str = "[%START_TIME%] \"%REQ(:METHOD)% %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)% %PROTOCOL%\" %RESPONSE_CODE% %RESPONSE_FLAGS% %BYTES_RECEIVED% %BYTES_SENT% %DURATION% %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)% \"%REQ(X-FORWARDED-FOR)%\" \"%REQ(USER-AGENT)%\" \"%REQ(X-REQUEST-ID)%\" \"%REQ(:AUTHORITY)%\" \"%UPSTREAM_HOST%\"\n";

/// Hand-rolled Envoy default-format concatenator — retained ONLY as a
/// `#[cfg(test)]` equivalence oracle for the engine-based default-format
/// re-expression. Emits a single line WITHOUT a trailing newline (the
/// engine path adds the `\n` via [`DEFAULT_FORMAT`]).
#[cfg(test)]
fn legacy_format(record: &AccessLogRecord) -> String {
    let mut s = String::with_capacity(256);
    s.push('[');
    format_iso8601(&mut s, record.start_time);
    s.push_str("] \"");
    s.push_str(&record.method);
    s.push(' ');
    s.push_str(&record.path);
    s.push(' ');
    s.push_str(&record.protocol);
    s.push_str("\" ");
    write!(&mut s, "{}", record.response_code).unwrap();
    s.push(' ');
    s.push_str(&record.response_flags);
    s.push(' ');
    write!(&mut s, "{}", record.bytes_received).unwrap();
    s.push(' ');
    write!(&mut s, "{}", record.bytes_sent).unwrap();
    s.push(' ');
    write!(&mut s, "{}", record.duration.as_millis()).unwrap();
    s.push(' ');
    match &record.upstream_service_time {
        Some(d) => {
            write!(&mut s, "{}", d.as_millis()).unwrap();
        }
        None => s.push('-'),
    }
    s.push_str(" \"");
    push_or_dash(&mut s, &record.forwarded_for);
    s.push_str("\" \"");
    push_or_dash(&mut s, &record.user_agent);
    s.push_str("\" \"");
    push_or_dash(&mut s, &record.request_id);
    s.push_str("\" \"");
    push_or_dash(&mut s, &record.authority);
    s.push_str("\" \"");
    push_or_dash(&mut s, &record.upstream_host);
    s.push('"');
    s
}

#[cfg(test)]
fn push_or_dash(s: &mut String, opt: &Option<String>) {
    match opt {
        Some(v) => s.push_str(v),
        None => s.push('-'),
    }
}

/// Hand-rolled ISO-8601 emitter: `YYYY-MM-DDTHH:MM:SS.sssZ`
/// (UTC, millisecond resolution). Appends 24 ASCII bytes to `s`.
///
/// Defers to `epoch_seconds_to_ymd_hms` for the date split. No
/// timezone handling beyond UTC; no leap-second handling; the
/// fractional-second component is millisecond-truncated (`Duration::
/// subsec_millis`).
pub(crate) fn format_iso8601(s: &mut String, t: SystemTime) {
    let dur = t.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let secs = dur.as_secs();
    let ms = dur.subsec_millis();
    let (year, month, day, hour, minute, second) = epoch_seconds_to_ymd_hms(secs);
    write!(
        s,
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hour, minute, second, ms
    )
    .unwrap();
}

/// Gregorian calendar arithmetic helper. Splits an epoch-seconds
/// value into `(year, month, day, hour, minute, second)`.
///
/// Year range supported: `[1970, 9999]`. The upper bound covers all
/// conceivable wall-clock timestamps before the 4-digit-year ISO-
/// 8601 format breaks; the lower bound is the UNIX epoch.
///
/// Algorithm: standard days-since-epoch decomposition.
///   1. Split `secs` into `total_days = secs / 86_400` and
///      `time_of_day = secs % 86_400`.
///   2. `time_of_day` → `(hour, minute, second)` via 3600/60
///      division.
///   3. `total_days` → `(year, month, day)` via year-walk: subtract
///      days_in_year(year) iteratively until the remainder fits in
///      a single year; then walk months via days_in_month(month,
///      is_leap_year).
fn epoch_seconds_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let total_days = secs / 86_400;
    let time_of_day = secs % 86_400;

    let hour = (time_of_day / 3600) as u32;
    let minute = ((time_of_day % 3600) / 60) as u32;
    let second = (time_of_day % 60) as u32;

    // Year-walk from 1970.
    let mut year: u32 = 1970;
    let mut remaining_days = total_days;
    loop {
        let dy = days_in_year(year) as u64;
        if remaining_days < dy {
            break;
        }
        remaining_days -= dy;
        year += 1;
    }

    // Month-walk from January.
    let leap = is_leap_year(year);
    let mut month: u32 = 1;
    let mut remaining_days = remaining_days as u32;
    loop {
        let dm = days_in_month(month, leap);
        if remaining_days < dm {
            break;
        }
        remaining_days -= dm;
        month += 1;
    }

    let day = remaining_days + 1; // 1-indexed

    (year, month, day, hour, minute, second)
}

fn is_leap_year(year: u32) -> bool {
    // Predicate ordering is load-bearing: 4 → 100 → 400. Year 2100
    // is NOT a leap year (multiple of 100 AND not multiple of 400);
    // year 2000 IS a leap year (multiple of 400). The
    // `epoch_seconds_to_ymd_hms_known_dates` test exercises both.
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn days_in_year(year: u32) -> u32 {
    if is_leap_year(year) { 366 } else { 365 }
}

fn days_in_month(month: u32, leap: bool) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => unreachable!("month out of range: {}", month),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_baseline_record() -> AccessLogRecord {
        // Mirrors fixture 0012's direct_response surface: GET / → 200
        // "ok\n"; no upstream; no extra request headers.
        AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(5),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: Some("envoy-rust.test".into()),
            upstream_host: None,
        }
    }

    #[test]
    fn compiled_default_matches_legacy_concatenator() {
        let record = make_baseline_record();
        let legacy = legacy_format(&record);
        let engine = crate::command_operator::CompiledFormat::default().render(&record);
        assert_eq!(engine, format!("{legacy}\n"));
    }

    #[test]
    fn format_happy_path_direct_response() {
        let record = make_baseline_record();
        let line = legacy_format(&record);
        // The leading [...] is the ISO-8601 timestamp (golden-tested
        // separately in format_iso8601_epoch_zero). After the
        // closing `] `, the rest of the line is deterministic per
        // record's fields.
        assert!(
            line.starts_with("[1970-01-01T00:00:00.000Z] "),
            "line: {}",
            line
        );
        // The rest of the line: literal substitution per the record fields.
        let expected_suffix =
            "\"GET / HTTP/1.1\" 200 - 0 3 5 - \"-\" \"-\" \"-\" \"envoy-rust.test\" \"-\"";
        assert!(
            line.ends_with(expected_suffix),
            "line: {}\nexpected suffix: {}",
            line,
            expected_suffix
        );
    }

    #[test]
    fn format_with_router_proxy_path() {
        let mut record = make_baseline_record();
        record.upstream_service_time = Some(Duration::from_millis(2));
        record.upstream_host = Some("127.0.0.1:8080".into());
        record.response_code = 201;
        let line = legacy_format(&record);
        let expected_suffix = "\"GET / HTTP/1.1\" 201 - 0 3 5 2 \"-\" \"-\" \"-\" \"envoy-rust.test\" \"127.0.0.1:8080\"";
        assert!(
            line.ends_with(expected_suffix),
            "line: {}\nexpected suffix: {}",
            line,
            expected_suffix
        );
    }

    #[test]
    fn format_5xx_response_with_flags() {
        // Forward-compat: 06.2 always emits "-" for response_flags
        // at the HCM record-build site, but the formatter must
        // handle non-"-" flag tokens for future phases.
        let mut record = make_baseline_record();
        record.response_code = 503;
        record.response_flags = "UH".into();
        let line = legacy_format(&record);
        let expected_suffix =
            "\"GET / HTTP/1.1\" 503 UH 0 3 5 - \"-\" \"-\" \"-\" \"envoy-rust.test\" \"-\"";
        assert!(line.ends_with(expected_suffix), "line: {}", line);
    }

    #[test]
    fn format_iso8601_epoch_zero() {
        let mut s = String::new();
        format_iso8601(&mut s, UNIX_EPOCH);
        assert_eq!(s, "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn format_iso8601_known_date() {
        // 2024-02-29T12:34:56.789Z — leap day boundary.
        // 1709210096 is the epoch seconds for 2024-02-29T12:34:56Z
        // (verify against `date -ud '2024-02-29T12:34:56Z' +%s` = 1709210096).
        let t = UNIX_EPOCH + Duration::from_millis(1_709_210_096_789);
        let mut s = String::new();
        format_iso8601(&mut s, t);
        assert_eq!(s, "2024-02-29T12:34:56.789Z");
    }

    #[test]
    fn epoch_seconds_to_ymd_hms_known_dates() {
        // Table-driven test with known epochs.
        // Local alias keeps clippy::type_complexity quiet without
        // adding public surface to the module.
        type YmdHms = (u32, u32, u32, u32, u32, u32);
        let cases: &[(u64, YmdHms)] = &[
            // epoch 0
            (0, (1970, 1, 1, 0, 0, 0)),
            // 2000-03-01T00:00:00Z (just after Y2K leap day)
            // date -ud '2000-03-01T00:00:00Z' +%s = 951868800
            (951_868_800, (2000, 3, 1, 0, 0, 0)),
            // 2000-02-29T23:59:59Z (last second of Y2K leap day)
            (951_868_799, (2000, 2, 29, 23, 59, 59)),
            // 2024-02-29T12:34:56Z (current-era leap day)
            (1_709_210_096, (2024, 2, 29, 12, 34, 56)),
            // 2100-03-01T00:00:00Z (century year not a leap year)
            // date -ud '2100-03-01T00:00:00Z' +%s = 4107542400
            (4_107_542_400, (2100, 3, 1, 0, 0, 0)),
            // 2100-02-28T23:59:59Z (last second of Feb 2100; not a leap year)
            (4_107_542_399, (2100, 2, 28, 23, 59, 59)),
            // 2038-01-19T03:14:07Z (i32::MAX seconds; Y2K38 boundary)
            (2_147_483_647, (2038, 1, 19, 3, 14, 7)),
        ];
        for (secs, expected) in cases {
            let actual = epoch_seconds_to_ymd_hms(*secs);
            assert_eq!(actual, *expected, "secs={}", secs);
        }
    }

    #[test]
    fn epoch_seconds_to_ymd_hms_handles_far_future() {
        // Year 9999-12-31T23:59:59Z. Epoch seconds = approximately
        // 253402300799. The algorithm must not panic; the year
        // must render as 9999.
        let secs: u64 = 253_402_300_799;
        let (year, _, _, _, _, _) = epoch_seconds_to_ymd_hms(secs);
        assert_eq!(year, 9999);
    }

    #[test]
    fn format_utf8_edge_case_in_user_agent() {
        // Envoy's default format does not escape UTF-8 in REQ token
        // values; envoy-rust matches.
        let mut record = make_baseline_record();
        record.user_agent = Some("Mozilla/5.0 (X11; Linux 中文)".into());
        let line = legacy_format(&record);
        assert!(
            line.contains("\"Mozilla/5.0 (X11; Linux 中文)\""),
            "line: {}",
            line
        );
    }
}
