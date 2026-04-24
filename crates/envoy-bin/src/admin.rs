//! Minimal admin HTTP endpoint for phase 01. Serves `GET /ready` → `200 OK`
//! with body `LIVE\n`; everything else returns `404 Not Found`. The framing
//! is hand-rolled (no `hyper`, no `axum` — doctrine D-3.2). Per ADR-0011,
//! phase 01's differential contract is status + body only, not headers, so
//! the `server:` header carries `envoy-rust` and diverges from upstream
//! Envoy's `envoy` string until phase 04 populates the header allow-list.

use std::time::SystemTime;

/// Render a complete HTTP/1.1 response with a `Connection: close` framing. The
/// body is written verbatim; the caller passes the exact bytes (including any
/// trailing newline). Headers:
///
/// - `content-type: text/plain`
/// - `content-length: {body.len()}`
/// - `cache-control: no-cache, max-age=0`
/// - `x-content-type-options: nosniff`
/// - `server: envoy-rust` (ADR-0011 divergence from upstream)
/// - `date: {IMF-fixdate}`
/// - `connection: close`
pub(crate) fn render_response(status: u16, reason: &str, body: &[u8]) -> Vec<u8> {
    render_response_at(status, reason, body, SystemTime::now())
}

pub(crate) fn render_response_at(
    status: u16,
    reason: &str,
    body: &[u8],
    now: SystemTime,
) -> Vec<u8> {
    let date = rfc7231_imf_fixdate(now);
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         content-type: text/plain\r\n\
         content-length: {len}\r\n\
         cache-control: no-cache, max-age=0\r\n\
         x-content-type-options: nosniff\r\n\
         server: envoy-rust\r\n\
         date: {date}\r\n\
         connection: close\r\n\
         \r\n",
        len = body.len(),
    );
    let mut out = head.into_bytes();
    out.extend_from_slice(body);
    out
}

/// RFC 7231 IMF-fixdate: `Sun, 06 Nov 1994 08:49:37 GMT`.
///
/// Hand-rolled over `SystemTime` to avoid depending on `chrono` or `time`
/// (not on D-3.2; a phase that genuinely needs date arithmetic — phase 06's
/// access logs, maybe — can land an ADR and the crate together). Valid for
/// any `SystemTime` at or after the Unix epoch.
pub(crate) fn rfc7231_imf_fixdate(t: SystemTime) -> String {
    const DOW: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MON: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let tod = (secs % 86_400) as u32;
    let hh = tod / 3600;
    let mm = (tod / 60) % 60;
    let ss = tod % 60;

    // 1970-01-01 was a Thursday; DOW above is offset accordingly so days=0
    // indexes to "Thu".
    let dow = DOW[days.rem_euclid(7) as usize];
    let (y, mo, d) = civil_from_days(days);
    format!(
        "{dow}, {d:02} {mon} {y:04} {hh:02}:{mm:02}:{ss:02} GMT",
        mon = MON[(mo - 1) as usize],
    )
}

/// Howard Hinnant's public-domain `civil_from_days` algorithm — converts the
/// day-count since the Unix epoch into a `(year, month, day)` triple.
/// Valid for the full `i64` range; we only feed it non-negative values.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146_096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn imf_fixdate_epoch_zero() {
        assert_eq!(
            rfc7231_imf_fixdate(UNIX_EPOCH),
            "Thu, 01 Jan 1970 00:00:00 GMT"
        );
    }

    #[test]
    fn imf_fixdate_known_1994() {
        // 1994-11-06 08:49:37 UTC — the RFC 7231 §7.1.1.1 example timestamp.
        // secs = 784111777 (verified independently).
        let t = UNIX_EPOCH + Duration::from_secs(784_111_777);
        assert_eq!(rfc7231_imf_fixdate(t), "Sun, 06 Nov 1994 08:49:37 GMT");
    }

    #[test]
    fn imf_fixdate_leap_year_boundary() {
        // 2000-02-29 12:00:00 UTC — the century leap-year that Hinnant's
        // algorithm gets right where the naive "divisible by 4" check fails.
        // secs = 951_825_600 (2000-02-29 12:00 UTC).
        let t = UNIX_EPOCH + Duration::from_secs(951_825_600);
        assert_eq!(rfc7231_imf_fixdate(t), "Tue, 29 Feb 2000 12:00:00 GMT");
    }

    #[test]
    fn render_response_has_expected_shape_and_body() {
        let t = UNIX_EPOCH + Duration::from_secs(784_111_777);
        let bytes = render_response_at(200, "OK", b"LIVE\n", t);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "status line: {s:?}");
        assert!(s.contains("content-length: 5\r\n"), "missing CL: {s:?}");
        assert!(s.contains("content-type: text/plain\r\n"));
        assert!(s.contains("server: envoy-rust\r\n"));
        assert!(s.contains("date: Sun, 06 Nov 1994 08:49:37 GMT\r\n"));
        assert!(s.contains("connection: close\r\n"));
        assert!(s.ends_with("\r\n\r\nLIVE\n"), "body/CRLF: {s:?}");
    }

    #[test]
    fn render_response_404_body_is_invalid_path_message() {
        let t = UNIX_EPOCH;
        let body = b"invalid path. admin commands are:\n  /ready\n" as &[u8];
        let bytes = render_response_at(404, "Not Found", body, t);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(s.contains(&format!("content-length: {}\r\n", body.len())));
        assert!(s.ends_with(std::str::from_utf8(body).unwrap()));
    }
}
