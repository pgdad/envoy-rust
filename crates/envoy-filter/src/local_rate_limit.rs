//! `envoy.filters.http.local_ratelimit` runtime filter (phase 09).
//!
//! Hand-rolled per D-3.2's "Every individual filter ... Must be written from
//! scratch" doctrine + the broader stats / accesslog / admin / drain
//! hand-roll posture across the MVP trunk. Token bucket lives at this
//! module's `TokenBucketState`; the `LocalRateLimitFilter` runtime struct
//! wraps it + threads 4 stats counters per SPEC §3 D6.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

/// The `envoy.filters.http.local_ratelimit` runtime filter.
///
/// Decode-only filter (per upstream Envoy v1.33 semantic + phase-09 SPEC
/// §5.4): consumes one token per decode-side invocation; on token exhaustion
/// short-circuits with a `Decision::StopAndSend` response (429 +
/// `x-envoy-ratelimited: true`). Encode-side is a no-op `Decision::Continue`.
///
/// Stat counters (4, per phase-09 SPEC §3 D6):
///   - `http_local_rate_limit.<stat_prefix>.enabled` — every decode-side invocation
///   - `http_local_rate_limit.<stat_prefix>.ok` — every `try_acquire` success
///   - `http_local_rate_limit.<stat_prefix>.rate_limited` — every `try_acquire` failure
///   - `http_local_rate_limit.<stat_prefix>.enforced` — every 429 emission
///
/// At phase-09 scope `enforced == rate_limited` (no `filter_enforced`
/// fractional-percent override); both are landed independently to match
/// upstream Envoy v1.33's stat tree exactly.
#[derive(Debug, Clone)]
pub struct LocalRateLimitFilter {
    // Read only by the `#[cfg(test)]` accessor `stat_prefix()`; retained on
    // production builds for diagnostic parity with upstream Envoy's filter
    // struct (the stat-name prefix is the single user-visible identifier
    // for the filter instance).
    #[allow(dead_code)]
    stat_prefix: String,
    bucket: Arc<TokenBucketState>,
    max_tokens: u64,
    tokens_per_fill: u64,
    fill_interval: Duration,
    response_headers_to_add: Vec<(String, String)>,
    enabled_counter: Arc<Counter>,
    ok_counter: Arc<Counter>,
    rate_limited_counter: Arc<Counter>,
    enforced_counter: Arc<Counter>,
}

impl LocalRateLimitFilter {
    /// Lower an `envoy_config::LocalRateLimitConfig` into the runtime filter
    /// and register the 4 stat counters against the StatsRegistry. Returns
    /// `FilterError::InvalidConfig` if `fill_interval` fails to parse
    /// (defense-in-depth — the envoy-config validator at
    /// `validate_local_rate_limit_config` is the primary gate).
    pub(crate) fn build_from_config(
        cfg: &envoy_config::LocalRateLimitConfig,
        registry: &Arc<StatsRegistry>,
    ) -> Result<Self, FilterError> {
        let fill_str =
            cfg.token_bucket
                .fill_interval
                .as_str()
                .ok_or_else(|| FilterError::InvalidConfig {
                    message:
                        "LocalRateLimit token_bucket.fill_interval must be a string (e.g. \"60s\")"
                            .to_string(),
                })?;
        let fill_interval =
            envoy_config::parse_duration(fill_str).map_err(|m| FilterError::InvalidConfig {
                message: format!("LocalRateLimit token_bucket.fill_interval: {m}"),
            })?;
        let max_tokens = cfg.token_bucket.max_tokens as u64;
        let tokens_per_fill = cfg.token_bucket.tokens_per_fill as u64;
        let response_headers_to_add = cfg
            .response_headers_to_add
            .iter()
            .map(|opt| (opt.header.key.clone(), opt.header.value.clone()))
            .collect();
        let enabled_counter = registry
            .register_counter(&format!(
                "http_local_rate_limit.{}.enabled",
                cfg.stat_prefix
            ))
            .map_err(|e| FilterError::InvalidConfig {
                message: format!("StatsRegistry: {e}"),
            })?;
        let ok_counter = registry
            .register_counter(&format!("http_local_rate_limit.{}.ok", cfg.stat_prefix))
            .map_err(|e| FilterError::InvalidConfig {
                message: format!("StatsRegistry: {e}"),
            })?;
        let rate_limited_counter = registry
            .register_counter(&format!(
                "http_local_rate_limit.{}.rate_limited",
                cfg.stat_prefix
            ))
            .map_err(|e| FilterError::InvalidConfig {
                message: format!("StatsRegistry: {e}"),
            })?;
        let enforced_counter = registry
            .register_counter(&format!(
                "http_local_rate_limit.{}.enforced",
                cfg.stat_prefix
            ))
            .map_err(|e| FilterError::InvalidConfig {
                message: format!("StatsRegistry: {e}"),
            })?;
        Ok(Self {
            stat_prefix: cfg.stat_prefix.clone(),
            bucket: Arc::new(TokenBucketState::new(max_tokens)),
            max_tokens,
            tokens_per_fill,
            fill_interval,
            response_headers_to_add,
            enabled_counter,
            ok_counter,
            rate_limited_counter,
            enforced_counter,
        })
    }

    pub(crate) fn decode_headers(&mut self, _req: &mut FilterRequest) -> Decision {
        self.enabled_counter.inc();
        if self
            .bucket
            .try_acquire(self.max_tokens, self.tokens_per_fill, self.fill_interval)
        {
            self.ok_counter.inc();
            Decision::Continue
        } else {
            self.rate_limited_counter.inc();
            self.enforced_counter.inc();
            let mut headers: Vec<(String, String)> =
                vec![("x-envoy-ratelimited".to_string(), "true".to_string())];
            headers.extend(self.response_headers_to_add.iter().cloned());
            Decision::StopAndSend(FilterResponse {
                status: 429,
                reason: Some("Too Many Requests"),
                headers,
                body: Bytes::new(),
            })
        }
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }

    /// Accessor for the configured stat_prefix (test-only convenience).
    #[cfg(test)]
    pub(crate) fn stat_prefix(&self) -> &str {
        &self.stat_prefix
    }
}

/// Hand-rolled token-bucket primitive. `AtomicU64` for the live token count;
/// `Mutex<Instant>` for the last-fill timestamp. Lazy fill: tokens computed
/// at `try_acquire` time, NOT via a background refill task. Per phase-09 SPEC §5.2.
#[derive(Debug)]
pub(crate) struct TokenBucketState {
    tokens: AtomicU64,
    last_fill_instant: Mutex<Instant>,
}

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

    // --- Task 3: LocalRateLimitFilter runtime tests ----------------------

    use crate::pipeline::Decision;
    use crate::types::{FilterRequest, FilterResponse};
    use envoy_stats::StatsRegistry;

    fn test_request() -> FilterRequest {
        FilterRequest {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: vec![("host".to_string(), "envoy-rust.test".to_string())],
            body: None,
        }
    }

    fn ok_cfg() -> envoy_config::LocalRateLimitConfig {
        envoy_config::LocalRateLimitConfig {
            stat_prefix: "phase_09".to_string(),
            token_bucket: envoy_config::TokenBucket {
                max_tokens: 2,
                tokens_per_fill: 0,
                fill_interval: serde_yaml::Value::String("60s".to_string()),
            },
            response_headers_to_add: Vec::new(),
            status: envoy_config::HttpStatus { code: 429 },
        }
    }

    #[test]
    fn build_from_config_succeeds_and_registers_counters() {
        let registry = Arc::new(StatsRegistry::new());
        let filter = LocalRateLimitFilter::build_from_config(&ok_cfg(), &registry)
            .expect("build_from_config succeeds");
        assert_eq!(filter.stat_prefix(), "phase_09");
        // The 4 counters are registered idempotently — registering again
        // returns the same Arc<Counter> via StatsRegistry's idempotence.
        let enabled = registry
            .register_counter("http_local_rate_limit.phase_09.enabled")
            .expect("enabled counter already registered");
        assert_eq!(enabled.value(), 0);
    }

    #[test]
    fn decode_headers_allows_request_under_limit_and_increments_ok_counter() {
        let registry = Arc::new(StatsRegistry::new());
        let mut filter =
            LocalRateLimitFilter::build_from_config(&ok_cfg(), &registry).expect("build");
        let mut req = test_request();
        let decision = filter.decode_headers(&mut req);
        assert!(matches!(decision, Decision::Continue));
        let enabled = registry
            .register_counter("http_local_rate_limit.phase_09.enabled")
            .unwrap();
        let ok = registry
            .register_counter("http_local_rate_limit.phase_09.ok")
            .unwrap();
        let rate_limited = registry
            .register_counter("http_local_rate_limit.phase_09.rate_limited")
            .unwrap();
        let enforced = registry
            .register_counter("http_local_rate_limit.phase_09.enforced")
            .unwrap();
        assert_eq!(enabled.value(), 1);
        assert_eq!(ok.value(), 1);
        assert_eq!(rate_limited.value(), 0);
        assert_eq!(enforced.value(), 0);
    }

    #[test]
    fn decode_headers_rate_limits_after_max_tokens_and_increments_rate_limited_enforced() {
        let registry = Arc::new(StatsRegistry::new());
        let mut filter =
            LocalRateLimitFilter::build_from_config(&ok_cfg(), &registry).expect("build");
        let mut req = test_request();
        // Drain the 2 tokens.
        assert!(matches!(
            filter.decode_headers(&mut req),
            Decision::Continue
        ));
        assert!(matches!(
            filter.decode_headers(&mut req),
            Decision::Continue
        ));
        // Third request is rate-limited.
        let decision = filter.decode_headers(&mut req);
        let resp = match decision {
            Decision::StopAndSend(r) => r,
            Decision::Continue => panic!("expected StopAndSend"),
        };
        assert_eq!(resp.status, 429);
        assert_eq!(resp.reason, Some("Too Many Requests"));
        assert!(
            resp.headers
                .iter()
                .any(|(k, v)| { k.eq_ignore_ascii_case("x-envoy-ratelimited") && v == "true" }),
            "x-envoy-ratelimited: true missing from headers: {:?}",
            resp.headers
        );
        assert!(resp.body.is_empty(), "rate-limited body must be empty");
        let enabled = registry
            .register_counter("http_local_rate_limit.phase_09.enabled")
            .unwrap();
        let ok = registry
            .register_counter("http_local_rate_limit.phase_09.ok")
            .unwrap();
        let rate_limited = registry
            .register_counter("http_local_rate_limit.phase_09.rate_limited")
            .unwrap();
        let enforced = registry
            .register_counter("http_local_rate_limit.phase_09.enforced")
            .unwrap();
        assert_eq!(enabled.value(), 3);
        assert_eq!(ok.value(), 2);
        assert_eq!(rate_limited.value(), 1);
        assert_eq!(enforced.value(), 1);
    }

    #[test]
    fn decode_headers_appends_configured_response_headers() {
        let registry = Arc::new(StatsRegistry::new());
        let mut cfg = ok_cfg();
        // max_tokens=1 + pre-drain → second request rate-limited.
        cfg.token_bucket.max_tokens = 1;
        cfg.response_headers_to_add = vec![envoy_config::HeaderValueOption {
            header: envoy_config::HeaderValue {
                key: "x-rate-limit-policy".to_string(),
                value: "phase-09".to_string(),
            },
            append_action: envoy_config::AppendAction::AppendIfExistsOrAdd,
        }];
        let mut filter = LocalRateLimitFilter::build_from_config(&cfg, &registry).expect("build");
        let mut req = test_request();
        let _ = filter.decode_headers(&mut req); // drain
        let resp = match filter.decode_headers(&mut req) {
            Decision::StopAndSend(r) => r,
            Decision::Continue => panic!("expected StopAndSend"),
        };
        assert!(
            resp.headers
                .iter()
                .any(|(k, v)| k == "x-envoy-ratelimited" && v == "true")
        );
        assert!(
            resp.headers
                .iter()
                .any(|(k, v)| k == "x-rate-limit-policy" && v == "phase-09")
        );
    }

    #[test]
    fn encode_headers_is_noop_continue() {
        let registry = Arc::new(StatsRegistry::new());
        let mut filter =
            LocalRateLimitFilter::build_from_config(&ok_cfg(), &registry).expect("build");
        let mut resp = FilterResponse {
            status: 200,
            reason: Some("OK"),
            headers: Vec::new(),
            body: bytes::Bytes::new(),
        };
        let decision = filter.encode_headers(&mut resp);
        assert!(matches!(decision, Decision::Continue));
        // No counter increments on encode.
        let enabled = registry
            .register_counter("http_local_rate_limit.phase_09.enabled")
            .unwrap();
        assert_eq!(enabled.value(), 0);
    }

    #[test]
    fn build_from_config_rejects_unparseable_fill_interval() {
        let registry = Arc::new(StatsRegistry::new());
        let mut cfg = ok_cfg();
        cfg.token_bucket.fill_interval = serde_yaml::Value::String("forever".to_string());
        let err = LocalRateLimitFilter::build_from_config(&cfg, &registry).unwrap_err();
        assert!(matches!(
            err,
            crate::error::FilterError::InvalidConfig { .. }
        ));
    }
}
