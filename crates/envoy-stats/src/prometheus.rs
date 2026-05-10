//! envoy-stats Prometheus text-exposition emitter.
//!
//! Format per metric (per https://prometheus.io/docs/instrumenting/exposition_formats/):
//!
//! ```text
//! # HELP <name> <description>
//! # TYPE <name> counter|gauge
//! <name> <value>
//! ```
//!
//! `# HELP` lines are emitted as a generic placeholder in 06.1 per SPEC §6
//! signpost 15; richer per-metric descriptions defer to a later phase.
//! Names with dots / dashes are translated to underscores per Envoy's
//! prom-emitter convention; the `envoy_` prefix mirrors upstream.

use crate::registry::{StatHandle, StatsRegistry};
use bytes::BytesMut;
use std::fmt::Write as _;

/// Writes the registry's snapshot in Prometheus text-exposition format into
/// `w`. Names are sorted lexicographically per `StatsRegistry::snapshot`'s
/// BTreeMap-backed contract.
pub fn write_exposition(registry: &StatsRegistry, w: &mut BytesMut) {
    for (name, handle) in registry.snapshot() {
        let prom_name = to_prometheus_name(&name);
        let kind = handle.kind_str();
        // # HELP line — generic placeholder in 06.1 per SPEC §6 signpost 15.
        let _ = writeln!(w, "# HELP {prom_name} envoy-rust {kind}.");
        let _ = writeln!(w, "# TYPE {prom_name} {kind}");
        match handle {
            StatHandle::Counter(c) => {
                let _ = writeln!(w, "{prom_name} {}", c.value());
            }
            StatHandle::Gauge(g) => {
                let _ = writeln!(w, "{prom_name} {}", g.value());
            }
        }
    }
}

/// Translate an Envoy-style stat name (`listener.foo.downstream_cx_total`) to
/// a Prometheus-compliant name (`envoy_listener_foo_downstream_cx_total`).
/// Dots and dashes become underscores; leading `envoy_` prefix mirrors
/// upstream's emit-side convention.
fn to_prometheus_name(envoy_name: &str) -> String {
    let mut out = String::with_capacity(envoy_name.len() + 6);
    out.push_str("envoy_");
    for c in envoy_name.chars() {
        out.push(if c == '.' || c == '-' { '_' } else { c });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_exposition_empty_registry() {
        let reg = StatsRegistry::new();
        let mut buf = BytesMut::new();
        write_exposition(&reg, &mut buf);
        assert_eq!(buf.len(), 0, "empty registry → empty output");
    }

    #[test]
    fn write_exposition_single_counter() {
        let reg = StatsRegistry::new();
        let c = reg
            .register_counter("listener.foo.downstream_cx_total")
            .unwrap();
        c.add(5);
        let mut buf = BytesMut::new();
        write_exposition(&reg, &mut buf);
        let s = std::str::from_utf8(&buf).unwrap();
        let expected = "# HELP envoy_listener_foo_downstream_cx_total envoy-rust counter.\n\
                        # TYPE envoy_listener_foo_downstream_cx_total counter\n\
                        envoy_listener_foo_downstream_cx_total 5\n";
        assert_eq!(s, expected);
    }

    #[test]
    fn write_exposition_single_gauge() {
        let reg = StatsRegistry::new();
        let g = reg
            .register_gauge("cluster.svc.upstream_cx_active")
            .unwrap();
        g.set(-3);
        let mut buf = BytesMut::new();
        write_exposition(&reg, &mut buf);
        let s = std::str::from_utf8(&buf).unwrap();
        let expected = "# HELP envoy_cluster_svc_upstream_cx_active envoy-rust gauge.\n\
                        # TYPE envoy_cluster_svc_upstream_cx_active gauge\n\
                        envoy_cluster_svc_upstream_cx_active -3\n";
        assert_eq!(s, expected);
    }

    #[test]
    fn write_exposition_mixed_counter_and_gauge_lex_ordered() {
        let reg = StatsRegistry::new();
        let _ = reg.register_gauge("b.gauge").unwrap();
        let _ = reg.register_counter("a.counter").unwrap();
        let mut buf = BytesMut::new();
        write_exposition(&reg, &mut buf);
        let s = std::str::from_utf8(&buf).unwrap();
        // a.counter should appear before b.gauge per BTreeMap ordering.
        let a_pos = s.find("envoy_a_counter").expect("a present");
        let b_pos = s.find("envoy_b_gauge").expect("b present");
        assert!(a_pos < b_pos, "lex order: a < b");
    }

    #[test]
    fn write_exposition_dot_to_underscore() {
        let reg = StatsRegistry::new();
        let _ = reg
            .register_counter("http.ingress_http.downstream_rq_total")
            .unwrap();
        let mut buf = BytesMut::new();
        write_exposition(&reg, &mut buf);
        let s = std::str::from_utf8(&buf).unwrap();
        assert!(s.contains("envoy_http_ingress_http_downstream_rq_total"));
        // The `_http` segment in `ingress_http` survives unchanged (only dots/dashes translate).
    }

    #[test]
    fn write_exposition_dash_to_underscore() {
        let reg = StatsRegistry::new();
        let _ = reg
            .register_counter("cluster.svc-A.upstream_cx_total")
            .unwrap();
        let mut buf = BytesMut::new();
        write_exposition(&reg, &mut buf);
        let s = std::str::from_utf8(&buf).unwrap();
        assert!(s.contains("envoy_cluster_svc_A_upstream_cx_total"));
    }
}
