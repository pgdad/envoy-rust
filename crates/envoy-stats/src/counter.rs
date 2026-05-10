//! envoy-stats `Counter` primitive — increment-only `AtomicU64`-backed counter.
//!
//! `Counter::inc()` and `Counter::add(n)` use `Ordering::Relaxed` per SPEC §6
//! signpost 3: stats values are read-only at scrape time and the program does
//! not synchronize control flow on stats values; no happens-before contract
//! is needed. `Test 4` (multi-thread torture) verifies the Relaxed ordering
//! is sound under realistic load.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct Counter {
    value: AtomicU64,
}

impl Counter {
    // `pub(crate)` per Task 3 discipline: the registry (Task 4) is the only
    // intended construction site; consumers receive `Arc<Counter>` from it.
    // Until Task 4 lands, the constructor is exercised only by unit tests, so
    // the lib build sees it as dead code — allow it for this brief window.
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    pub fn value(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn counter_starts_at_zero() {
        let c = Counter::new();
        assert_eq!(c.value(), 0);
    }

    #[test]
    fn counter_inc_increments() {
        let c = Counter::new();
        c.inc();
        c.inc();
        c.inc();
        assert_eq!(c.value(), 3);
    }

    #[test]
    fn counter_add_increments_by_n() {
        let c = Counter::new();
        c.add(7);
        assert_eq!(c.value(), 7);
        c.add(13);
        assert_eq!(c.value(), 20);
    }

    #[test]
    fn counter_inc_under_torture() {
        // 8 threads × 10_000 inc each → expected total 80_000.
        let c = Arc::new(Counter::new());
        let mut handles = Vec::with_capacity(8);
        for _ in 0..8 {
            let c2 = Arc::clone(&c);
            handles.push(std::thread::spawn(move || {
                for _ in 0..10_000 {
                    c2.inc();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(c.value(), 80_000);
    }
}
