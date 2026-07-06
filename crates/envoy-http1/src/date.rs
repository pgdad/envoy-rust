//! Hand-rolled IMF-fixdate writer (RFC 7231 §7.1.1.1).
//!
//! No external crate dep — `httpdate` would be the obvious off-the-shelf
//! choice but the parent-04 SPEC §3 D1 locks the hand-rolled approach so
//! 04.1 doesn't pre-emptively land an `httpdate` ADR. ~30 LoC.

use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

thread_local! {
    /// Per-worker-thread cached IMF-fixdate string, refreshed at most once per
    /// second on the thread that observes the tick. Matches Envoy's behavior of
    /// regenerating the Date header at ~1 Hz rather than per request — the format
    /// granularity is seconds, so per-request work would just rebuild an identical
    /// string in the common case.
    ///
    /// Thread-local (not a global `Mutex`) so concurrent worker threads never
    /// contend on a shared lock: under the old global mutex, aggregate throughput
    /// *fell* as worker count rose (every response serialized on one lock). The
    /// per-second `format_imf_fixdate` cost is paid at most once per thread per
    /// second — a handful of tokio workers → negligible duplication — and every
    /// thread produces the identical string for a given second, so the emitted
    /// `date:` header is byte-for-byte unchanged.
    static DATE_CACHE: RefCell<Option<(u64, String)>> = const { RefCell::new(None) };
}

/// Cached variant of `format_imf_fixdate(SystemTime::now())`. Returns the
/// same string for the entire wall-clock second; only the first caller
/// on a given thread in a given second pays the format cost.
pub fn now_imf_fixdate() -> String {
    // Coarse realtime clock for the once-per-second cache check: the cache
    // granularity is a full second, so the ~4 ms coarse-clock granularity is
    // irrelevant, and the coarse read skips the hardware counter read that a
    // full `SystemTime::now()` pays (measurably hot on virtualized hosts).
    #[cfg(target_os = "linux")]
    let secs = {
        let ts = rustix::time::clock_gettime(rustix::time::ClockId::RealtimeCoarse);
        if ts.tv_sec < 0 { 0 } else { ts.tv_sec as u64 }
    };
    #[cfg(not(target_os = "linux"))]
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    DATE_CACHE.with_borrow_mut(|cache| {
        if let Some((cached_secs, ref cached_str)) = *cache
            && cached_secs == secs
        {
            return cached_str.clone();
        }
        let fresh = format_imf_fixdate(UNIX_EPOCH + std::time::Duration::from_secs(secs));
        *cache = Some((secs, fresh.clone()));
        fresh
    })
}


/// Coarse monotonic milliseconds for latency spans whose OUTPUT granularity
/// is already milliseconds (`x-envoy-upstream-service-time`). On Linux this
/// reads CLOCK_MONOTONIC_COARSE — no hardware counter read (the dominant
/// cost of `Instant::now()` on virtualized hosts) at the price of scheduler-
/// tick granularity (1–4 ms), which only quantizes a value that is itself
/// reported in whole milliseconds. Non-Linux falls back to `Instant`.
pub(crate) fn coarse_monotonic_ms() -> u128 {
    #[cfg(target_os = "linux")]
    {
        let ts = rustix::time::clock_gettime(rustix::time::ClockId::MonotonicCoarse);
        (ts.tv_sec.max(0) as u128) * 1000 + (ts.tv_nsec as u128) / 1_000_000
    }
    #[cfg(not(target_os = "linux"))]
    {
        use std::sync::OnceLock;
        static ANCHOR: OnceLock<std::time::Instant> = OnceLock::new();
        ANCHOR
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_millis()
    }
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
