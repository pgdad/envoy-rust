//! 108.2 D5: the nine `runtime.*` stats, registered UNCONDITIONALLY at
//! process startup — upstream Envoy v1.33.0 emits all nine even on a config
//! with no `layered_runtime` block at all (SPEC §2, MEASURED).
//!
//! Kinds mirror upstream's `/stats/prometheus` `# TYPE` lines, MEASURED
//! against the pinned image (ADR-0174): four GAUGES
//! (`admin_overrides_active`, `deprecated_feature_seen_since_process_start`,
//! `num_keys`, `num_layers`) and five COUNTERS (`deprecated_feature_use`,
//! `load_error`, `load_success`, `override_dir_exists`,
//! `override_dir_not_exists`).
//!
//! Only `num_keys` and `num_layers` track config; `load_success: 1` and
//! `override_dir_not_exists: 1` fire unconditionally (MEASURED: `load_success`
//! stays `1` on a TWO-layer config — it counts loads, not layers); the other
//! five are `0` on any in-scope config. Values are set ONCE here — nothing
//! mutates the snapshot after startup in this slice (no RTDS, no
//! `/runtime_modify`, no override directory). As of 109.1 the route
//! `runtime_fraction` consumer READS the boot snapshot for behavior
//! (`RuntimeSnapshot::route_fraction_gate` inside `route_matches`); the
//! `RuntimeUInt32` (`status_code_filter`) and CSRF consumers and RTDS remain
//! unbuilt, so every remaining "no runtime CONSUMER for this key" assertion
//! stays true.

use envoy_config::Bootstrap;
use envoy_config::runtime::RuntimeSnapshot;
use envoy_stats::{StatsError, StatsRegistry};

/// Register the nine `runtime.*` stats and bind the two config-tracking
/// gauges to the snapshot the parsed bootstrap implies. Mirrors the
/// `register_lds_stats` / `register_rds_stats` startup cadence in `main.rs`;
/// like them it is called exactly once, before the admin listener serves.
pub fn register_runtime_stats(
    bootstrap: &Bootstrap,
    registry: &StatsRegistry,
) -> Result<(), StatsError> {
    // The same entry point the `/runtime` renderer uses (108.1 REVIEW M-5:
    // never `from_layers` directly), so the stats and the endpoint can never
    // disagree about the snapshot.
    let snapshot = RuntimeSnapshot::from_bootstrap(bootstrap);

    // Gauges (upstream `# TYPE ... gauge`).
    registry.register_gauge("runtime.admin_overrides_active")?;
    registry.register_gauge("runtime.deprecated_feature_seen_since_process_start")?;
    let num_keys = registry.register_gauge("runtime.num_keys")?;
    num_keys.set(i64::try_from(snapshot.num_keys()).unwrap_or(i64::MAX));
    let num_layers = registry.register_gauge("runtime.num_layers")?;
    num_layers.set(i64::try_from(snapshot.num_layers()).unwrap_or(i64::MAX));

    // Counters (upstream `# TYPE ... counter`).
    registry.register_counter("runtime.deprecated_feature_use")?;
    registry.register_counter("runtime.load_error")?;
    registry.register_counter("runtime.load_success")?.inc();
    registry.register_counter("runtime.override_dir_exists")?;
    registry
        .register_counter("runtime.override_dir_not_exists")?
        .inc();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_stats::StatHandle;

    const TWO_LAYER_YAML: &str = "\
node:
  id: t
  cluster: c
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 0
static_resources:
  listeners: []
  clusters: []
layered_runtime:
  layers:
    - name: base_layer
      static_layer:
        shared.key: from_base
        only.in.base: base_val
        nested:
          deep: x
    - name: override_layer
      static_layer:
        shared.key: from_override
        only.in.override: over_val
";

    /// Look a stat up in the registry snapshot; panic with the name on a miss
    /// so an absent registration reads as the failure it is (never `Ok(0)`).
    fn handle_for(registry: &StatsRegistry, name: &str) -> StatHandle {
        registry
            .snapshot()
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("stat {name} not registered"))
            .1
    }

    /// SPEC §2 (MEASURED on the fixture-0087 config): all nine names exist,
    /// with the four non-zero witnesses at their measured values and the
    /// measured counter/gauge kinds (upstream `/stats/prometheus` `# TYPE`
    /// lines — ADR-0174). `nested.deep` proves `num_keys` counts FLATTENED
    /// LEAVES: 3 base leaves + 1 override-only key = 4.
    #[test]
    fn registers_all_nine_runtime_stats_with_measured_values_and_kinds() {
        let registry = StatsRegistry::new();
        let b = envoy_config::parse_bootstrap(TWO_LAYER_YAML).expect("valid bootstrap");
        register_runtime_stats(&b, &registry).expect("register");

        for (name, value) in [
            ("runtime.admin_overrides_active", 0),
            ("runtime.deprecated_feature_seen_since_process_start", 0),
            ("runtime.num_keys", 4),
            ("runtime.num_layers", 2),
        ] {
            match handle_for(&registry, name) {
                StatHandle::Gauge(g) => assert_eq!(g.value(), value, "{name}"),
                StatHandle::Counter(_) => panic!("{name} must be a GAUGE (measured upstream kind)"),
            }
        }
        for (name, value) in [
            ("runtime.deprecated_feature_use", 0),
            ("runtime.load_error", 0),
            ("runtime.load_success", 1),
            ("runtime.override_dir_exists", 0),
            ("runtime.override_dir_not_exists", 1),
        ] {
            match handle_for(&registry, name) {
                StatHandle::Counter(c) => assert_eq!(c.value(), value, "{name}"),
                StatHandle::Gauge(_) => panic!("{name} must be a COUNTER (measured upstream kind)"),
            }
        }
    }

    /// SPEC §2 N-8 (MEASURED): the absent-vs-empty distinction reaches the
    /// stats — no block: `num_layers 0 / num_keys 0`; an empty block (either
    /// spelling): `num_layers 1 / num_keys 0`; and the unconditional pair
    /// (`load_success`, `override_dir_not_exists`) is 1 in every case.
    #[test]
    fn absent_and_empty_blocks_differ_in_num_layers_only() {
        let base = "admin:\n  address:\n    socket_address:\n      address: 127.0.0.1\n      port_value: 0\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        for (spelling, layers) in [
            ("", 0),
            ("layered_runtime: {}\n", 1),
            ("layered_runtime:\n  layers: []\n", 1),
        ] {
            let registry = StatsRegistry::new();
            let b = envoy_config::parse_bootstrap(&format!("{base}{spelling}")).expect("valid");
            register_runtime_stats(&b, &registry).expect("register");
            match handle_for(&registry, "runtime.num_layers") {
                StatHandle::Gauge(g) => {
                    assert_eq!(g.value(), layers, "num_layers for {spelling:?}")
                }
                StatHandle::Counter(_) => panic!("num_layers must be a gauge"),
            }
            match handle_for(&registry, "runtime.num_keys") {
                StatHandle::Gauge(g) => assert_eq!(g.value(), 0, "num_keys for {spelling:?}"),
                StatHandle::Counter(_) => panic!("num_keys must be a gauge"),
            }
            match handle_for(&registry, "runtime.load_success") {
                StatHandle::Counter(c) => assert_eq!(c.value(), 1),
                StatHandle::Gauge(_) => panic!("load_success must be a counter"),
            }
        }
    }
}
