//! Hand-rolled IMF-fixdate writer (RFC 7231 §7.1.1.1).
//!
//! No external crate dep — `httpdate` would be the obvious off-the-shelf
//! choice but the parent-04 SPEC §3 D1 locks the hand-rolled approach so
//! 04.1 doesn't pre-emptively land an `httpdate` ADR. ~30 LoC.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Cached IMF-fixdate string, refreshed at most once per second. Match's
/// Envoy's behavior of regenerating the Date header at ~1 Hz rather than
/// per request — the format granularity is seconds, so per-request work
/// would just rebuild an identical string in the common case.
static DATE_CACHE: Mutex<Option<(u64, String)>> = Mutex::new(None);

/// Cached variant of `format_imf_fixdate(SystemTime::now())`. Returns the
/// same string for the entire wall-clock second; only the first caller
/// in a given second pays the format cost.
pub fn now_imf_fixdate() -> String {
    let now = SystemTime::now();
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut guard = DATE_CACHE.lock().expect("date cache poisoned");
    if let Some((cached_secs, ref cached_str)) = *guard {
        if cached_secs == secs {
            return cached_str.clone();
        }
    }
    let fresh = format_imf_fixdate(now);
    *guard = Some((secs, fresh.clone()));
    fresh
}

/// Format a `SystemTime` as an IMF-fixdate string per RFC 7231 §7.1.1.1.
///
/// Returns the canonical form: `Sun, 06 Nov 1994 08:49:37 GMT`.
///
/// Times before the Unix epoch return the epoch itself (defensive — clock
/// skew at boot can produce pre-epoch times briefly; the HCM never emits
/// such a header in practice).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn format_imf_fixdate(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Time of day.
    let sec = (secs % 60) as u32;
    let min = ((secs / 60) % 60) as u32;
    let hour = ((secs / 3600) % 24) as u32;

    // Days since 1970-01-01 (= Thursday, day 4 of week if Sun=0).
    let days = (secs / 86_400) as i64;
    let weekday_idx = ((days + 4).rem_euclid(7)) as usize; // 0=Sun..6=Sat
    let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

    // Civil date — Howard Hinnant's algorithm (`days_from_civil` inverse).
    // Source: http://howardhinnant.github.io/date_algorithms.html#civil_from_days
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };

    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        day_names[weekday_idx],
        d,
        month_names[(m - 1) as usize],
        year,
        hour,
        min,
        sec,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn formats_canonical_imf_fixdate() {
        // 784111777 seconds after the epoch = Sun, 06 Nov 1994 08:49:37 GMT.
        let t = UNIX_EPOCH + Duration::from_secs(784_111_777);
        assert_eq!(format_imf_fixdate(t), "Sun, 06 Nov 1994 08:49:37 GMT");
    }

    #[test]
    fn formats_unix_epoch() {
        assert_eq!(
            format_imf_fixdate(UNIX_EPOCH),
            "Thu, 01 Jan 1970 00:00:00 GMT"
        );
    }
}
