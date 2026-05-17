//! `envoy.filters.http.local_ratelimit` runtime filter (phase 09).
//!
//! Hand-rolled per D-3.2's "Every individual filter ... Must be written from
//! scratch" doctrine + the broader stats / accesslog / admin / drain
//! hand-roll posture across the MVP trunk. Token bucket lives at this
//! module's `TokenBucketState`; the filter struct + decode/encode glue
//! lands in Task 3.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Hand-rolled token-bucket primitive. `AtomicU64` for the live token count;
/// `Mutex<Instant>` for the last-fill timestamp. Lazy fill: tokens computed
/// at `try_acquire` time, NOT via a background refill task. Per phase-09 SPEC §5.2.
#[allow(dead_code)]
// wired up by Task 3's LocalRateLimitFilter runtime; exercised by unit tests in this module at Task 2.
#[derive(Debug)]
pub(crate) struct TokenBucketState {
    tokens: AtomicU64,
    last_fill_instant: Mutex<Instant>,
}

#[allow(dead_code)] // wired up by Task 3's LocalRateLimitFilter runtime; exercised by unit tests in this module at Task 2.
impl TokenBucketState {
    /// Construct a fresh bucket at full capacity (`max_tokens` tokens
    /// available immediately) with `last_fill_instant` set to `now`.
    pub(crate) fn new(max_tokens: u64) -> Self {
        Self {
            tokens: AtomicU64::new(max_tokens),
            last_fill_instant: Mutex::new(Instant::now()),
        }
    }

    /// Attempt to consume one token. Returns `true` on success (token
    /// consumed; request allowed to continue); `false` on failure (bucket
    /// empty post-refill; request would-be-rate-limited).
    ///
    /// Lazy fill: at call time, computes how many fill_intervals have
    /// elapsed since `last_fill_instant` and adds
    /// `intervals_elapsed * tokens_per_fill` to the live count (capped at
    /// `max_tokens`). Then atomically decrements by 1 via `compare_exchange`;
    /// on contention retries with re-load. Updates `last_fill_instant` only
    /// when at least one interval has actually elapsed AND a token was
    /// successfully consumed.
    pub(crate) fn try_acquire(
        &self,
        max_tokens: u64,
        tokens_per_fill: u64,
        fill_interval: Duration,
    ) -> bool {
        loop {
            let current = self.tokens.load(Ordering::Acquire);
            // Lazy fill: compute the post-refill count.
            let (available, new_last_fill) = if tokens_per_fill > 0 {
                let last_fill = *self
                    .last_fill_instant
                    .lock()
                    .expect("TokenBucketState last_fill_instant Mutex poisoned");
                let elapsed = last_fill.elapsed();
                let interval_nanos = fill_interval.as_nanos();
                let elapsed_nanos = elapsed.as_nanos();
                // Defensive: validator rejects 0 intervals, but the primitive
                // should still be sound. `checked_div` returns None on 0
                // divisor; treat as zero intervals elapsed.
                match elapsed_nanos.checked_div(interval_nanos) {
                    None | Some(0) => (current, last_fill),
                    Some(intervals_u128) => {
                        let intervals = intervals_u128 as u64;
                        let refilled =
                            current.saturating_add(intervals.saturating_mul(tokens_per_fill));
                        let capped = refilled.min(max_tokens);
                        let advance =
                            fill_interval.saturating_mul(intervals.min(u32::MAX as u64) as u32);
                        (capped, last_fill + advance)
                    }
                }
            } else {
                // tokens_per_fill == 0 → no refill; carry current.
                (
                    current,
                    *self
                        .last_fill_instant
                        .lock()
                        .expect("TokenBucketState last_fill_instant Mutex poisoned"),
                )
            };
            if available == 0 {
                return false;
            }
            let next = available - 1;
            // Single CAS — if it succeeds, we own the consumed token AND
            // the refill computation. Note: we CAS against `current` (the
            // pre-refill load), NOT `available`. If `available > current`
            // and CAS succeeds, the additional refilled tokens are
            // implicitly "credited" by jumping straight from `current` to
            // `next = available - 1`.
            match self
                .tokens
                .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    if tokens_per_fill > 0
                        && new_last_fill
                            != *self
                                .last_fill_instant
                                .lock()
                                .expect("TokenBucketState last_fill_instant Mutex poisoned")
                    {
                        *self
                            .last_fill_instant
                            .lock()
                            .expect("TokenBucketState last_fill_instant Mutex poisoned") =
                            new_last_fill;
                    }
                    return true;
                }
                Err(_) => {
                    // Concurrent acquire — re-load and retry.
                    continue;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn new_bucket_starts_at_capacity() {
        let state = TokenBucketState::new(3);
        assert_eq!(state.tokens.load(Ordering::Acquire), 3);
    }

    #[test]
    fn try_acquire_consumes_one_token_at_a_time() {
        let state = TokenBucketState::new(3);
        assert!(state.try_acquire(3, 0, Duration::from_secs(60)));
        assert!(state.try_acquire(3, 0, Duration::from_secs(60)));
        assert!(state.try_acquire(3, 0, Duration::from_secs(60)));
        assert!(!state.try_acquire(3, 0, Duration::from_secs(60)));
    }

    #[test]
    fn try_acquire_returns_false_on_empty_bucket_with_no_refill() {
        let state = TokenBucketState::new(0);
        assert!(!state.try_acquire(0, 0, Duration::from_secs(60)));
    }

    #[test]
    fn try_acquire_drains_then_recovers_after_sleep() {
        let state = TokenBucketState::new(2);
        // Drain.
        assert!(state.try_acquire(2, 1, Duration::from_millis(10)));
        assert!(state.try_acquire(2, 1, Duration::from_millis(10)));
        assert!(!state.try_acquire(2, 1, Duration::from_millis(10)));
        // Sleep ~30ms (3 intervals) → at least 1 token refilled (capped at max=2).
        std::thread::sleep(Duration::from_millis(35));
        assert!(state.try_acquire(2, 1, Duration::from_millis(10)));
    }

    #[test]
    fn try_acquire_refill_caps_at_max_tokens() {
        let state = TokenBucketState::new(1);
        // Drain.
        assert!(state.try_acquire(1, 5, Duration::from_millis(10)));
        // Sleep 100ms (10 intervals × 5 tokens_per_fill = 50 hypothetical
        // refill) — but capped at max=1.
        std::thread::sleep(Duration::from_millis(100));
        // Consume the 1 refilled token.
        assert!(state.try_acquire(1, 5, Duration::from_millis(10)));
        // Bucket should be empty again — no overflow.
        assert!(!state.try_acquire(1, 5, Duration::from_millis(10)));
    }

    /// REQUIRED per phase-09 SPEC §6.3: 8-thread × 10_000-acquire torture
    /// test. Asserts the sum of `true` returns across all tasks equals
    /// `min(N*M, max_tokens)` (initial fill, `tokens_per_fill = 0`).
    /// Verifies no token-double-count under `Ordering::AcqRel` concurrent
    /// CAS retry.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn token_bucket_concurrent_acquire_does_not_double_count() {
        const N_TASKS: u64 = 8;
        const M_ACQUIRES: u64 = 10_000;
        const MAX_TOKENS: u64 = 1000;

        let state = Arc::new(TokenBucketState::new(MAX_TOKENS));
        let success_count = Arc::new(AtomicU64::new(0));

        let mut handles = Vec::with_capacity(N_TASKS as usize);
        for _ in 0..N_TASKS {
            let state = Arc::clone(&state);
            let success_count = Arc::clone(&success_count);
            handles.push(tokio::spawn(async move {
                for _ in 0..M_ACQUIRES {
                    if state.try_acquire(MAX_TOKENS, 0, Duration::from_secs(60)) {
                        success_count.fetch_add(1, Ordering::AcqRel);
                    }
                }
            }));
        }
        for h in handles {
            h.await.expect("torture task completes");
        }

        let observed = success_count.load(Ordering::Acquire);
        let expected = std::cmp::min(N_TASKS * M_ACQUIRES, MAX_TOKENS);
        assert_eq!(
            observed, expected,
            "concurrent acquire double-counted or lost tokens: observed={observed}, expected={expected}"
        );
        // The bucket should be empty.
        assert_eq!(state.tokens.load(Ordering::Acquire), 0);
    }
}
