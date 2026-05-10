//! envoy-stats `StatsRegistry` — hierarchical name → handle map over
//! `std::sync::RwLock<std::collections::BTreeMap<String, StatHandle>>`.
//!
//! `BTreeMap` over `HashMap` per SPEC §6 signpost 6: deterministic snapshot
//! ordering for diff-friendly Prometheus exposition. Lookup is O(log n) but
//! n is bounded at ~50–500 across the project's lifetime, so the cost is
//! negligible against the diff-stability benefit.

use crate::counter::Counter;
use crate::error::StatsError;
use crate::gauge::Gauge;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub enum StatHandle {
    Counter(Arc<Counter>),
    Gauge(Arc<Gauge>),
}

impl StatHandle {
    pub fn kind_str(&self) -> &'static str {
        match self {
            StatHandle::Counter(_) => "counter",
            StatHandle::Gauge(_) => "gauge",
        }
    }
}

#[derive(Debug, Default)]
pub struct StatsRegistry {
    map: RwLock<BTreeMap<String, StatHandle>>,
}

impl StatsRegistry {
    pub fn new() -> Self {
        Self {
            map: RwLock::new(BTreeMap::new()),
        }
    }

    /// Register or look up a counter under `name`. Idempotent for same-kind
    /// re-registration (returns the existing `Arc<Counter>`). Errors if a
    /// different-kind entry exists under the same name.
    pub fn register_counter(&self, name: &str) -> Result<Arc<Counter>, StatsError> {
        if !is_valid_name(name) {
            return Err(StatsError::InvalidName {
                name: name.to_string(),
                reason: "must match [a-zA-Z_:][a-zA-Z0-9_:.-]*",
            });
        }
        let mut map = self.map.write().expect("StatsRegistry RwLock poisoned");
        match map.get(name) {
            Some(StatHandle::Counter(arc)) => Ok(Arc::clone(arc)),
            Some(StatHandle::Gauge(_)) => Err(StatsError::ConflictingKind {
                name: name.to_string(),
                expected: "counter",
                got: "gauge",
            }),
            None => {
                let arc = Arc::new(Counter::new());
                map.insert(name.to_string(), StatHandle::Counter(Arc::clone(&arc)));
                Ok(arc)
            }
        }
    }

    /// Register or look up a gauge under `name`. Idempotent for same-kind.
    pub fn register_gauge(&self, name: &str) -> Result<Arc<Gauge>, StatsError> {
        if !is_valid_name(name) {
            return Err(StatsError::InvalidName {
                name: name.to_string(),
                reason: "must match [a-zA-Z_:][a-zA-Z0-9_:.-]*",
            });
        }
        let mut map = self.map.write().expect("StatsRegistry RwLock poisoned");
        match map.get(name) {
            Some(StatHandle::Gauge(arc)) => Ok(Arc::clone(arc)),
            Some(StatHandle::Counter(_)) => Err(StatsError::ConflictingKind {
                name: name.to_string(),
                expected: "gauge",
                got: "counter",
            }),
            None => {
                let arc = Arc::new(Gauge::new());
                map.insert(name.to_string(), StatHandle::Gauge(Arc::clone(&arc)));
                Ok(arc)
            }
        }
    }

    /// Snapshot the current name → handle pairs in lexicographic order.
    /// Re-snapshots on every call so writers may continue updating concurrently.
    pub fn snapshot(&self) -> Vec<(String, StatHandle)> {
        let map = self.map.read().expect("StatsRegistry RwLock poisoned");
        map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
}

/// Prometheus name rules: first char `[a-zA-Z_:]`; subsequent chars
/// `[a-zA-Z0-9_:.\-]*`. The `.` and `-` are intentionally permitted because
/// Envoy's stat tree uses dots as separators; the Prometheus emitter
/// translates dots / dashes to underscores at emission time.
fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let first_ok = first.is_ascii_alphabetic() || first == '_' || first == ':';
    if !first_ok {
        return false;
    }
    for c in chars {
        let ok = c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '-');
        if !ok {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_register_counter_returns_handle() {
        let reg = StatsRegistry::new();
        let c = reg
            .register_counter("listener.foo.downstream_cx_total")
            .unwrap();
        c.inc();
        assert_eq!(c.value(), 1);

        let snap = reg.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].0, "listener.foo.downstream_cx_total");
    }

    #[test]
    fn registry_register_counter_idempotent_same_kind() {
        let reg = StatsRegistry::new();
        let a = reg.register_counter("foo").unwrap();
        let b = reg.register_counter("foo").unwrap();
        assert!(
            Arc::ptr_eq(&a, &b),
            "idempotent registration must return the same Arc"
        );
        let snap = reg.snapshot();
        assert_eq!(snap.len(), 1);
    }

    #[test]
    fn registry_register_gauge_then_counter_same_name_errors() {
        let reg = StatsRegistry::new();
        let _ = reg.register_gauge("foo").unwrap();
        let err = reg.register_counter("foo").unwrap_err();
        match err {
            StatsError::ConflictingKind { name, expected, got } => {
                assert_eq!(name, "foo");
                assert_eq!(expected, "counter");
                assert_eq!(got, "gauge");
            }
            _ => panic!("expected ConflictingKind, got {err:?}"),
        }
    }

    #[test]
    fn registry_invalid_name_errors() {
        let reg = StatsRegistry::new();
        let err = reg.register_counter("bad name with spaces").unwrap_err();
        match err {
            StatsError::InvalidName { name, .. } => assert_eq!(name, "bad name with spaces"),
            _ => panic!("expected InvalidName, got {err:?}"),
        }
    }

    #[test]
    fn registry_snapshot_is_lexicographic() {
        let reg = StatsRegistry::new();
        let _ = reg.register_counter("b").unwrap();
        let _ = reg.register_counter("a").unwrap();
        let _ = reg.register_counter("c").unwrap();
        let names: Vec<String> = reg.snapshot().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn registry_concurrent_register_safe() {
        let reg = Arc::new(StatsRegistry::new());
        let mut handles = Vec::with_capacity(4);
        for t in 0..4 {
            let r = Arc::clone(&reg);
            handles.push(std::thread::spawn(move || {
                for i in 0..100 {
                    let name = format!("thread{t}.metric{i}");
                    let _ = r.register_counter(&name).unwrap();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(reg.snapshot().len(), 400);
    }

    #[test]
    fn is_valid_name_accepts_envoy_stat_shapes() {
        assert!(is_valid_name("listener.foo.downstream_cx_total"));
        assert!(is_valid_name("cluster.svc-A.upstream_cx_total"));
        assert!(is_valid_name("http.ingress_http.downstream_rq_total"));
        assert!(is_valid_name("a"));
        assert!(is_valid_name("_"));
        assert!(is_valid_name(":"));
    }

    #[test]
    fn is_valid_name_rejects_bad_shapes() {
        assert!(!is_valid_name(""));
        assert!(!is_valid_name(" "));
        assert!(!is_valid_name("with space"));
        assert!(!is_valid_name("1starts_with_digit"));
        assert!(!is_valid_name("contains/slash"));
        assert!(!is_valid_name("contains#hash"));
    }
}
