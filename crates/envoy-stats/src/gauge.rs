//! envoy-stats `Gauge` primitive — settable / inc / dec `AtomicI64`-backed
//! gauge. Permits negative values (Envoy's `cluster_health` etc. report
//! signed deltas). `Ordering::Relaxed` per SPEC §6 signpost 3.

use std::sync::atomic::{AtomicI64, Ordering};

#[derive(Debug, Default)]
pub struct Gauge {
    value: AtomicI64,
}

impl Gauge {
    // `pub(crate)` per Task 3 discipline: the registry (Task 4) is the only
    // intended construction site; consumers receive `Arc<Gauge>` from it.
    // Until Task 4 lands, the constructor is exercised only by unit tests, so
    // the lib build sees it as dead code — allow it for this brief window.
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self {
            value: AtomicI64::new(0),
        }
    }

    pub fn set(&self, v: i64) {
        self.value.store(v, Ordering::Relaxed);
    }

    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec(&self) {
        self.value.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn value(&self) -> i64 {
        self.value.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn gauge_starts_at_zero() {
        let g = Gauge::new();
        assert_eq!(g.value(), 0);
    }

    #[test]
    fn gauge_set_then_inc_then_dec() {
        let g = Gauge::new();
        g.set(10);
        g.inc();
        g.dec();
        g.dec();
        assert_eq!(g.value(), 9);
    }

    #[test]
    fn gauge_under_torture() {
        // 4 inc threads × 10_000 ops + 4 dec threads × 10_000 ops → 0.
        let g = Arc::new(Gauge::new());
        let mut handles = Vec::with_capacity(8);
        for _ in 0..4 {
            let g2 = Arc::clone(&g);
            handles.push(std::thread::spawn(move || {
                for _ in 0..10_000 {
                    g2.inc();
                }
            }));
        }
        for _ in 0..4 {
            let g2 = Arc::clone(&g);
            handles.push(std::thread::spawn(move || {
                for _ in 0..10_000 {
                    g2.dec();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(g.value(), 0);
    }

    #[test]
    fn gauge_negative_value_permitted() {
        let g = Gauge::new();
        g.set(0);
        for _ in 0..5 {
            g.dec();
        }
        assert_eq!(g.value(), -5);
    }
}
