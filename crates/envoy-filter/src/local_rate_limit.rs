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
/// short-circuits with a `Decision::StopAndSend` response (429 + body
/// `"local_rate_limited"` matching upstream Envoy v1.33's source-hardcoded
/// default per ADR-0033). Encode-side is a no-op `Decision::Continue`. The 5
/// standard HTTP/1.1 response headers (`server`, `date`, `content-length`,
/// `content-type`, `connection`) are decorated onto the synth response by
/// the H1 HCM's `decorate_filter_synth_response` helper at the writer-arm
/// site; the filter only emits the operator-configured
/// `response_headers_to_add` entries.
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
        let enabled_counter = crate::error::register_counter(
            registry,
            &format!("http_local_rate_limit.{}.enabled", cfg.stat_prefix),
        )?;
        let ok_counter = crate::error::register_counter(
            registry,
            &format!("http_local_rate_limit.{}.ok", cfg.stat_prefix),
        )?;
        let rate_limited_counter = crate::error::register_counter(
            registry,
            &format!("http_local_rate_limit.{}.rate_limited", cfg.stat_prefix),
        )?;
        let enforced_counter = crate::error::register_counter(
            registry,
            &format!("http_local_rate_limit.{}.enforced", cfg.stat_prefix),
        )?;
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
            // Upstream Envoy v1.33's `envoy.filters.http.local_ratelimit` emits
            // a 429 response with body `"local_rate_limited"` (source-hardcoded;
            // no configurable `response_body` field on the proto) and NO
            // `x-envoy-ratelimited` header (that header belongs to the global
            // ratelimit filter and to router-side response-flag handling, not
            // to local_ratelimit). Per ADR-0033 envoy-rust matches the upstream
            // wire shape exactly. The standard HTTP/1.1 response headers
            // (server / date / content-length / content-type / connection) are
            // decorated onto the synth response by the H1 HCM's
            // `decorate_filter_synth_response` helper at the writer-arm site —
            // the filter only emits the operator-configured
            // `response_headers_to_add` entries.
            let headers: Vec<(String, String)> = self.response_headers_to_add.clone();
            Decision::StopAndSend(FilterResponse {
                status: 429,
                reason: Some("Too Many Requests"),
                headers,
                body: Bytes::from_static(b"local_rate_limited"),
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
    /// `max_tokens`). The `last_fill_instant` guard is acquired ONCE and
    /// held across the whole refill-compute + token compare-and-store, so
    /// every mutating access is serialized — no read-then-write window
    /// between computing `new_last_fill` and storing it. `last_fill_instant`
    /// advances only when at least one interval has actually elapsed AND a
    /// token was successfully consumed (`new_last_fill == last_fill`
    /// otherwise, so the store is a no-op).
    pub(crate) fn try_acquire(
        &self,
        max_tokens: u64,
        tokens_per_fill: u64,
        fill_interval: Duration,
    ) -> bool {
        // Single lock acquisition per call; the guard doubles as the mutual
        // exclusion for the `tokens` update (all writers go through here),
        // so a plain store suffices — no CAS retry loop.
        let mut last_fill = self
            .last_fill_instant
            .lock()
            .expect("TokenBucketState last_fill_instant Mutex poisoned");
        let current = self.tokens.load(Ordering::Acquire);
        let (available, new_last_fill) = Self::refill(
            current,
            *last_fill,
            max_tokens,
            tokens_per_fill,
            fill_interval,
        );
        if available == 0 {
            return false;
        }
        self.tokens.store(available - 1, Ordering::Release);
        *last_fill = new_last_fill;
        true
    }

    /// Pure lazy-fill arithmetic: given the live count and the last-fill
    /// timestamp, return the post-refill count (capped at `max_tokens`) and
    /// the advanced last-fill timestamp. Whole intervals only — the
    /// timestamp advances by `intervals * fill_interval`, not to `now`, so
    /// the fractional remainder keeps accruing. `tokens_per_fill == 0` (and
    /// a defensive `fill_interval == 0`: validator rejects it, but the
    /// primitive should still be sound — `checked_div` returns `None` on a
    /// 0 divisor) → no refill; carry both inputs through unchanged.
    fn refill(
        current: u64,
        last_fill: Instant,
        max_tokens: u64,
        tokens_per_fill: u64,
        fill_interval: Duration,
    ) -> (u64, Instant) {
        if tokens_per_fill == 0 {
            return (current, last_fill);
        }
        let elapsed_nanos = last_fill.elapsed().as_nanos();
        match elapsed_nanos.checked_div(fill_interval.as_nanos()) {
            None | Some(0) => (current, last_fill),
            Some(intervals_u128) => {
                let intervals = intervals_u128 as u64;
                let refilled = current.saturating_add(intervals.saturating_mul(tokens_per_fill));
                let capped = refilled.min(max_tokens);
                let advance = fill_interval.saturating_mul(intervals.min(u32::MAX as u64) as u32);
                (capped, last_fill + advance)
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

    #[test]
    fn refill_arithmetic_across_simulated_elapsed_intervals() {
        let interval = Duration::from_secs(10);
        // Simulate 3 full elapsed intervals by backdating last_fill. The
        // sub-interval time between here and the `refill` call is ~ns —
        // far below the 10s interval — so exactly 3 intervals are observed.
        let last_fill = Instant::now()
            .checked_sub(interval * 3)
            .expect("monotonic clock is at least 30s old");
        // 3 intervals × 2 tokens_per_fill on top of 1 live token = 7;
        // last_fill advances by WHOLE intervals only (3 × 10s), not to now.
        let (available, new_last_fill) = TokenBucketState::refill(1, last_fill, 10, 2, interval);
        assert_eq!(available, 7);
        assert_eq!(new_last_fill, last_fill + interval * 3);
        // Cap: 9 + 3×2 = 15 → capped at max_tokens = 10.
        let (capped, _) = TokenBucketState::refill(9, last_fill, 10, 2, interval);
        assert_eq!(capped, 10);
        // tokens_per_fill == 0 → no refill; count AND timestamp carried.
        let (unchanged, ts) = TokenBucketState::refill(5, last_fill, 10, 0, interval);
        assert_eq!(unchanged, 5);
        assert_eq!(ts, last_fill);
    }

    /// REQUIRED per phase-09 SPEC §6.3: 8-thread × 10_000-acquire torture
    /// test. Asserts the sum of `true` returns across all tasks equals
    /// `min(N*M, max_tokens)` (initial fill, `tokens_per_fill = 0`).
    /// Verifies no token-double-count under concurrent acquires (serialized
    /// by the single `last_fill_instant` guard held across compare+store).
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
        FilterRequest::test("GET", "/", &[("host", "envoy-rust.test")])
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
        // ADR-0033 (upstream Envoy v1.33 parity): the filter emits NO
        // `x-envoy-ratelimited` header (that header belongs to the global
        // ratelimit filter, not local_ratelimit) and the 5 standard HTTP/1.1
        // response headers are decorated by the H1 HCM's
        // `decorate_filter_synth_response` helper, not by the filter. The
        // filter's own header list is empty when no `response_headers_to_add`
        // is configured.
        assert!(
            resp.headers.is_empty(),
            "filter headers must be empty when no response_headers_to_add configured (ADR-0033); got {:?}",
            resp.headers
        );
        // ADR-0033: upstream Envoy v1.33's local_ratelimit emits body
        // `"local_rate_limited"` (18 bytes; source-hardcoded). envoy-rust
        // emits the same bytes for bilateral parity.
        assert_eq!(
            resp.body.as_ref(),
            b"local_rate_limited",
            "rate-limited body must be upstream-parity `local_rate_limited`"
        );
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
        // ADR-0033 (upstream Envoy v1.33 parity): the filter does NOT emit
        // `x-envoy-ratelimited`. Operator-configured `response_headers_to_add`
        // entries land verbatim. Body matches upstream parity
        // (`local_rate_limited`).
        assert_eq!(resp.headers.len(), 1, "headers: {:?}", resp.headers);
        assert!(
            resp.headers
                .iter()
                .any(|(k, v)| k == "x-rate-limit-policy" && v == "phase-09")
        );
        assert_eq!(resp.body.as_ref(), b"local_rate_limited");
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
