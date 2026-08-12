//! 108.1 D3: the runtime snapshot store — the in-memory view of a parsed
//! `layered_runtime` block, shaped exactly as upstream Envoy's admin
//! `GET /runtime` exposes it.
//!
//! This module is the ENGINE; the serde schema it consumes (`LayeredRuntime`,
//! `RuntimeLayer`, `RuntimeValue`) lives in [`crate::bootstrap`]. That split
//! follows the landed `matcher.rs` precedent, where the `HeaderMatcher` schema
//! sits in `bootstrap.rs` and the matching engine sits in its own module.
//!
//! **The route `runtime_fraction` consumer is live as of 109.1** — the store's
//! first behavioral reader (`RuntimeSnapshot::route_fraction_gate`, evaluated
//! inside `envoy-http1`'s `route_matches`). 108.1 built the PRODUCER; 108.2
//! added the admin `GET /runtime` endpoint and the nine `runtime.*` stats that
//! observe it. The `RuntimeUInt32` (`status_code_filter`) and
//! `RuntimeFractionalPercent` (CSRF) consumers and RTDS remain unbuilt, so
//! every remaining "no runtime CONSUMER for this key" assertion in the tree
//! (incl. the test `runtime_key_is_rtds_inert`) stays true.
//!
//! All ordering is `BTreeMap`-canonical. That is not incidental: sibling 108.2's
//! differential fixture rests on `serde_json::Map` being a `BTreeMap` (the
//! workspace enables `preserve_order` nowhere), and a canonically-ordered store
//! keeps the renderer deterministic before `serde_json` is involved.

use crate::{LayeredRuntime, RuntimeLayer, RuntimeValue};
use std::collections::BTreeMap;

/// One runtime key's view across the layer stack.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeEntry {
    /// One slot per CONFIGURED layer, in config order, holding `""` where the
    /// key is absent from that layer (SPEC §2 N-6, MEASURED).
    pub layer_values: Vec<String>,
    /// The last NON-EMPTY slot (SPEC §2 N-7, MEASURED) — **not** the last slot.
    /// An explicitly-set empty string does NOT override a lower layer, and is
    /// indistinguishable on the wire from the key being absent from that layer.
    pub final_value: String,
}

/// The whole snapshot: the ordered layer names plus every flattened key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeSnapshot {
    /// Layer names in config order. For an ABSENT `layered_runtime` block this
    /// is empty; for a PRESENT but empty one it holds exactly one EMPTY STRING
    /// (SPEC §2 N-8, MEASURED) — see `from_bootstrap`.
    pub layer_names: Vec<String>,
    /// Flattened key → entry, canonically ordered.
    pub entries: BTreeMap<String, RuntimeEntry>,
}

impl RuntimeSnapshot {
    /// Backs upstream's `runtime.num_layers` stat: the count of CONFIGURED
    /// layers (SPEC §2 N-5).
    pub fn num_layers(&self) -> usize {
        self.layer_names.len()
    }

    /// Backs upstream's `runtime.num_keys` stat. MEASURED (SPEC §2 N-5): it
    /// counts FLATTENED LEAVES, not declared top-level YAML keys — a layer
    /// declaring 11 top-level keys, one of them a nested map holding two
    /// leaves, yields `num_keys: 12`. This is exactly `entries.len()`.
    pub fn num_keys(&self) -> usize {
        self.entries.len()
    }

    /// Build a snapshot from an ordered layer stack.
    ///
    /// `layer_names` is passed separately from `layers` because an ABSENT
    /// `layered_runtime` block and a PRESENT-but-empty one differ in their layer
    /// NAMES but not in their layer CONTENT (SPEC §2 N-8): upstream synthesizes
    /// one layer named the EMPTY STRING for the empty block, and that synthetic
    /// layer has no `RuntimeLayer` behind it. `from_bootstrap` owns that
    /// distinction; this function is a total function over whatever stack it is
    /// handed. **Invariant: `layer_names.len()` MUST equal `layers.len()` unless
    /// `layers` is empty**, in which case each key simply gets `layer_names.len()`
    /// empty slots — which is exactly the empty-block case.
    ///
    /// Two MEASURED rules, both easy to get wrong:
    /// - every key gets ONE SLOT PER CONFIGURED LAYER, `""` where absent, in
    ///   config order (N-6) — slot count is a property of the stack, not the key;
    /// - `final_value` is the last NON-EMPTY slot (N-7), NOT the last slot. An
    ///   explicitly-set `""` does not override a lower layer, and is
    ///   indistinguishable on the wire from absence.
    pub fn from_layers(layer_names: Vec<String>, layers: &[RuntimeLayer]) -> RuntimeSnapshot {
        let slot_count = layer_names.len();
        let flattened: Vec<BTreeMap<String, String>> = layers.iter().map(flatten_layer).collect();

        let mut entries: BTreeMap<String, RuntimeEntry> = BTreeMap::new();
        for per_layer in &flattened {
            for key in per_layer.keys() {
                entries.entry(key.clone()).or_insert_with(|| RuntimeEntry {
                    layer_values: vec![String::new(); slot_count],
                    final_value: String::new(),
                });
            }
        }

        for (index, per_layer) in flattened.iter().enumerate() {
            for (key, value) in per_layer {
                if let Some(entry) = entries.get_mut(key)
                    && let Some(slot) = entry.layer_values.get_mut(index)
                {
                    *slot = value.clone();
                }
            }
        }

        for entry in entries.values_mut() {
            // Last NON-EMPTY wins; an all-empty key keeps the empty string.
            entry.final_value = entry
                .layer_values
                .iter()
                .rev()
                .find(|v| !v.is_empty())
                .cloned()
                .unwrap_or_default();
        }

        RuntimeSnapshot {
            layer_names,
            entries,
        }
    }

    /// Build the snapshot a parsed `Bootstrap` implies. **This is the entry
    /// point sibling 108.2's admin `GET /runtime` renderer calls.**
    ///
    /// The absent-vs-empty distinction lives here and nowhere else (SPEC §2 N-8,
    /// MEASURED): no `layered_runtime:` block yields ZERO layers, while
    /// `layered_runtime: {}` or `layered_runtime: {layers: []}` yields ONE layer
    /// named the EMPTY STRING. Upstream synthesizes that layer internally, which
    /// is why it is created here rather than in the schema — a config-declared
    /// layer named `""` is boot-fatal (PGV `min_len 1`), so the synthetic layer
    /// deliberately bypasses `validate_layered_runtime`.
    pub fn from_bootstrap(bootstrap: &crate::Bootstrap) -> RuntimeSnapshot {
        let Some(lr): Option<&LayeredRuntime> = bootstrap.layered_runtime.as_ref() else {
            // Absent: zero layers, zero keys.
            return RuntimeSnapshot::default();
        };
        if lr.layers.is_empty() {
            // Present but empty, in EITHER spelling: one synthetic layer named
            // the empty string, and no keys.
            return RuntimeSnapshot::from_layers(vec![String::new()], &[]);
        }
        let names: Vec<String> = lr.layers.iter().map(|l| l.name.clone()).collect();
        RuntimeSnapshot::from_layers(names, &lr.layers)
    }

    /// 109.1: the store's FIRST typed lookup — resolve a route
    /// `runtime_fraction` to its deterministic gate per the SPEC §1.3 cascade,
    /// MEASURED against envoyproxy/envoy:v1.33.0 (23 cells: parent §1.1 + the
    /// V-8 closure §1.2):
    ///
    /// 1. key consulted and any entry starts with `"<key>."` → `MapShapedKey`;
    /// 2. key consulted, present, `final_value` parses as finite f64 `v`:
    ///    `v == 0` → Never; `v >= 100` → Always; `0 < v < 100` →
    ///    `NondeterministicValue`; `v < 0` → fall through to the default;
    /// 3. key absent / unparseable / non-finite / not consulted → the
    ///    `default_value`: numerator `0` → Never, `== denominator.value()` →
    ///    Always, else `NondeterministicDefault`.
    ///
    /// An empty `runtime_key` string is treated as not-consulted (upstream
    /// unmeasured; the absent-like reading, recorded in the PLAN).
    pub fn route_fraction_gate(
        &self,
        rf: &crate::RuntimeFractionalPercent,
    ) -> Result<FractionGate, FractionGateError> {
        if let Some(key) = rf.runtime_key.as_deref().filter(|k| !k.is_empty()) {
            let prefix = format!("{key}.");
            if self
                .entries
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(name, _)| name.starts_with(&prefix))
            {
                return Err(FractionGateError::MapShapedKey {
                    key: key.to_string(),
                });
            }
            if let Some(entry) = self.entries.get(key)
                && let Ok(v) = entry.final_value.parse::<f64>()
                && v.is_finite()
            {
                if v == 0.0 {
                    return Ok(FractionGate::Never);
                }
                if v >= 100.0 {
                    return Ok(FractionGate::Always);
                }
                if v > 0.0 {
                    return Err(FractionGateError::NondeterministicValue {
                        key: key.to_string(),
                        value: entry.final_value.clone(),
                    });
                }
                // v < 0: MEASURED → default_value (cells N1/N2); fall through.
            }
            // Absent key, unparseable or non-finite value: default (cells
            // 1, 2, 10, 11, B1-B3); fall through.
        }
        let p = &rf.default_value;
        if p.numerator == 0 {
            Ok(FractionGate::Never)
        } else if p.numerator == p.denominator.value() {
            Ok(FractionGate::Always)
        } else {
            Err(FractionGateError::NondeterministicDefault {
                numerator: p.numerator,
                denominator: p.denominator.value(),
            })
        }
    }

    /// 109.1: infallible request-path wrapper over [`Self::route_fraction_gate`].
    /// The `Err` arm is VALIDATED-UNREACHABLE in production — all three error
    /// classes are boot-fatal at every validation path (boot, post-merge, RDS
    /// reload) — and deliberately does NOT panic (the rds_watcher
    /// `unreachable!()` lesson, 76.2 I-1): it falls back to the
    /// `default_value`'s sign, which is total and deterministic.
    pub fn route_fraction_passes(&self, rf: &crate::RuntimeFractionalPercent) -> bool {
        match self.route_fraction_gate(rf) {
            Ok(FractionGate::Always) => true,
            Ok(FractionGate::Never) => false,
            Err(_) => rf.default_value.numerator != 0,
        }
    }
}

/// 109.1: the deterministic route-gate verdict of a validated
/// `RuntimeFractionalPercent`. `Always` = the runtime_fraction gate passes and
/// prefix/path/headers matching applies unchanged; `Never` = the route never
/// matches. There is no sampling arm by design — every nondeterministic input
/// is boot-fatal (CF-109-1/2, ADR-0176 DECISION 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FractionGate {
    Always,
    Never,
}

/// 109.1: the boot-fatal classes of the SPEC §1.3 evaluation cascade. The
/// three validation paths map these onto `ConfigError` variants with listener/
/// route context; the request path never sees them (`route_fraction_passes`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FractionGateError {
    /// CF-109-1 (WIDENED at ADR-0176): the consulted key's `final_value`
    /// parses as a finite f64 strictly between 0 and 100 — upstream samples
    /// per request (integer `50` GATED 27/33 over 60; float `1.5` GATED 1/40).
    NondeterministicValue { key: String, value: String },
    /// CF-109-2, the SNAPSHOT-PREFIX rule (SPEC §3 D3): some entry name starts
    /// with `"<key>."`, i.e. a map-shaped value was flattened at (or beside)
    /// the consulted key — a plain lookup would silently use `default_value`
    /// where upstream honors the map (pick cells 7-8).
    MapShapedKey { key: String },
    /// The `default_value` itself is non-deterministic: numerator neither `0`
    /// nor `== denominator.value()` (the house `selects_deterministic`
    /// discipline; upstream also accepts `>` — the recorded slightly-narrower
    /// divergence, parent SPEC §3 D2(a)).
    NondeterministicDefault { numerator: u32, denominator: u32 },
}

/// Flatten one layer's `static_layer` into dotted keys → stringified values.
///
/// Recurses to ARBITRARY depth (SPEC §2 N-4): `my.nested: {sub_key: v, deeper:
/// {leaf: w}}` yields `my.nested.sub_key` AND `my.nested.deeper.leaf`, and NO
/// entry for either intermediate map. An empty nested map therefore yields
/// nothing at all — it has no leaves.
///
/// TOTAL by construction: a layer with no `static_layer` arm yields an empty
/// map rather than panicking. The validator rejects such a layer at boot, but
/// 108.2 renders snapshots and must never panic on one.
///
/// **Not in scope, and recorded rather than silently mishandled (CF-108-3):** a
/// nested map containing `numerator` is NOT flattened like every other nested
/// map upstream — it is kept as ONE key whose value is the protobuf TEXT-FORMAT
/// dump of the Struct, complete with literal `\n`s. Matching that byte-for-byte
/// means reimplementing protobuf `DebugString`. This function flattens it like
/// any other map; the divergence is banked, not hidden.
pub fn flatten_layer(layer: &RuntimeLayer) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(map) = layer.static_layer.as_ref() {
        for (key, value) in map {
            flatten_into(key, value, &mut out);
        }
    }
    out
}

/// Recursive worker for [`flatten_layer`]. Mirrors the shape of the landed
/// `validate_json_format_value` recursive walk in `bootstrap.rs`.
fn flatten_into(prefix: &str, value: &RuntimeValue, out: &mut BTreeMap<String, String>) {
    match value {
        RuntimeValue::Map(inner) => {
            for (key, sub) in inner {
                flatten_into(&format!("{prefix}.{key}"), sub, out);
            }
        }
        scalar => {
            out.insert(prefix.to_string(), scalar.stringify());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a single `RuntimeLayer` from a YAML fragment.
    fn layer(yaml: &str) -> crate::RuntimeLayer {
        serde_yaml::from_str(yaml).expect("layer fragment must parse")
    }

    #[test]
    fn flatten_layer_recurses_to_arbitrary_depth_and_emits_no_intermediate_keys() {
        // SPEC §2 N-4, MEASURED against envoyproxy/envoy:v1.33.0:
        //   my.nested: {sub_key: v, deeper: {leaf: w}}
        // yields entries `my.nested.sub_key` AND `my.nested.deeper.leaf`, with
        // NO `my.nested` and NO `my.nested.deeper` entry. The parent SPEC
        // measured only ONE level; this recurses.
        let l = layer(
            r#"
name: l
static_layer:
  flat.key: top
  my.nested:
    sub_key: v
    deeper:
      leaf: w
"#,
        );
        let flat = flatten_layer(&l);
        let mut keys: Vec<&str> = flat.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["flat.key", "my.nested.deeper.leaf", "my.nested.sub_key"],
            "no intermediate map key may appear as an entry"
        );
        assert_eq!(flat["flat.key"], "top");
        assert_eq!(flat["my.nested.sub_key"], "v");
        assert_eq!(flat["my.nested.deeper.leaf"], "w");
    }

    #[test]
    fn flatten_layer_stringifies_every_scalar_and_keeps_the_empty_string() {
        // SPEC §2 N-3 / N-7. Table-driven: a new measured cell costs one line.
        let l = layer(
            r#"
name: l
static_layer:
  k.bool.t: true
  k.bool.f: false
  k.int: 42
  k.negint: -7
  k.float: 1.5
  k.str: hello
  k.empty: ""
  k.yaml11: y
"#,
        );
        let flat = flatten_layer(&l);
        for (key, expected) in [
            ("k.bool.t", "true"),
            ("k.bool.f", "false"),
            ("k.int", "42"),
            ("k.negint", "-7"),
            // CF-108-5: the ONLY float cell measured upstream.
            ("k.float", "1.5"),
            ("k.str", "hello"),
            // SPEC §2 N-7: an explicit "" IS an entry and IS counted.
            ("k.empty", ""),
            // CF-108-4: upstream (YAML 1.1) would render "true" here.
            ("k.yaml11", "y"),
        ] {
            assert_eq!(flat[key], expected, "flatten_layer key {key}");
        }
        assert_eq!(flat.len(), 8, "an empty-string value is still an entry");
    }

    #[test]
    fn flatten_layer_handles_absent_and_empty_static_layers() {
        // A layer whose static_layer is an empty map contributes no keys...
        let empty = layer("name: l\nstatic_layer: {}\n");
        assert!(flatten_layer(&empty).is_empty());

        // ...and neither does an EMPTY NESTED map, because it has no leaves.
        // SPEC §2 N-4: intermediate maps never produce entries of their own.
        let empty_nested = layer("name: l\nstatic_layer:\n  a.b: {}\n");
        assert!(
            flatten_layer(&empty_nested).is_empty(),
            "an empty nested map has no leaves and so yields no entry"
        );

        // A layer with NO static_layer arm at all contributes nothing. (The
        // validator rejects such a layer at boot; flatten_layer must still be
        // total, because 108.2 renders snapshots and must never panic.)
        let none = layer("name: l\n");
        assert!(flatten_layer(&none).is_empty());
    }

    #[test]
    fn from_layers_reproduces_the_measured_two_layer_transcript() {
        // SPEC §2 N-6 and N-7, MEASURED against envoyproxy/envoy:v1.33.0. TWO
        // static layers with distinct names are LEGAL, which is what makes
        // multi-layer precedence witnessable in 108.1 without the out-of-scope
        // admin_layer. The upstream response was, verbatim:
        //
        //   "shared.key":       {"layer_values":["from_base","from_override"],"final_value":"from_override"}
        //   "only.in.base":     {"layer_values":["base_val",""],              "final_value":"base_val"}
        //   "only.in.override": {"layer_values":["","over_val"],              "final_value":"over_val"}
        //   "empty.in.override":{"layer_values":["real_value",""],            "final_value":"real_value"}
        //   with "layers":["base_layer","override_layer"], num_layers 2, num_keys 4.
        let base = layer(
            r#"
name: base_layer
static_layer:
  shared.key: from_base
  only.in.base: base_val
  empty.in.override: real_value
"#,
        );
        let over = layer(
            r#"
name: override_layer
static_layer:
  shared.key: from_override
  only.in.override: over_val
  empty.in.override: ""
"#,
        );
        let snap = RuntimeSnapshot::from_layers(
            vec!["base_layer".to_string(), "override_layer".to_string()],
            &[base, over],
        );

        assert_eq!(snap.layer_names, vec!["base_layer", "override_layer"]);
        assert_eq!(snap.num_layers(), 2);
        assert_eq!(snap.num_keys(), 4);

        // Table-driven: (key, expected slots, expected final_value).
        for (key, slots, final_value) in [
            (
                "shared.key",
                vec!["from_base", "from_override"],
                "from_override",
            ),
            ("only.in.base", vec!["base_val", ""], "base_val"),
            ("only.in.override", vec!["", "over_val"], "over_val"),
            // THE rule most likely to be got wrong: an explicitly-set "" does
            // NOT override a lower layer. "last wins" would give "" here.
            ("empty.in.override", vec!["real_value", ""], "real_value"),
        ] {
            let e = snap
                .entries
                .get(key)
                .unwrap_or_else(|| panic!("missing {key}"));
            assert_eq!(e.layer_values, slots, "layer_values for {key}");
            assert_eq!(e.final_value, final_value, "final_value for {key}");
        }
    }

    #[test]
    fn from_layers_gives_every_key_one_slot_per_configured_layer() {
        // Slot COUNT is a property of the layer STACK, not of the key: a key
        // present in only one of three layers still gets three slots.
        let a = layer("name: a\nstatic_layer:\n  only.in.a: v\n");
        let b = layer("name: b\nstatic_layer: {}\n");
        let c = layer("name: c\nstatic_layer: {}\n");
        let snap = RuntimeSnapshot::from_layers(
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
            &[a, b, c],
        );
        let e = &snap.entries["only.in.a"];
        assert_eq!(e.layer_values, vec!["v", "", ""]);
        assert_eq!(e.final_value, "v");
        assert_eq!(snap.num_layers(), 3);
        assert_eq!(snap.num_keys(), 1);
    }

    #[test]
    fn from_layers_keeps_an_all_empty_key_as_an_entry_with_an_empty_final_value() {
        // SPEC §2 N-7, single-layer probe, MEASURED:
        //   my.empty.string.key: "" -> {"final_value":"","layer_values":[""]}
        // and it IS counted in num_keys.
        let only = layer("name: l\nstatic_layer:\n  my.empty.string.key: \"\"\n");
        let snap = RuntimeSnapshot::from_layers(vec!["l".to_string()], &[only]);
        let e = &snap.entries["my.empty.string.key"];
        assert_eq!(e.layer_values, vec![""]);
        assert_eq!(e.final_value, "");
        assert_eq!(snap.num_keys(), 1, "an all-empty key is still a key");
    }

    #[test]
    fn from_bootstrap_distinguishes_absent_from_empty_from_populated() {
        // SPEC §2 N-8, MEASURED against envoyproxy/envoy:v1.33.0:
        //   | config                          | /runtime                     | num_layers | num_keys |
        //   | no layered_runtime block        | {"entries":{},"layers":[]}   | 0          | 0        |
        //   | layered_runtime: {}             | {"entries":{},"layers":[""]} | 1          | 0        |
        //   | layered_runtime: { layers: [] } | {"entries":{},"layers":[""]} | 1          | 0        |
        // Upstream synthesizes ONE layer named the EMPTY STRING for both empty
        // spellings. Collapsing None and Some(empty) MINTS a divergence.
        let base = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let absent = crate::parse_bootstrap(base).expect("valid");
        let snap = RuntimeSnapshot::from_bootstrap(&absent);
        assert!(snap.layer_names.is_empty(), "absent block -> layers: []");
        assert_eq!(snap.num_layers(), 0);
        assert_eq!(snap.num_keys(), 0);

        for spelling in ["layered_runtime: {}\n", "layered_runtime:\n  layers: []\n"] {
            let b = crate::parse_bootstrap(&format!("{base}{spelling}")).expect("valid");
            let snap = RuntimeSnapshot::from_bootstrap(&b);
            assert_eq!(
                snap.layer_names,
                vec![String::new()],
                "an empty block synthesizes ONE layer named the empty string ({spelling:?})"
            );
            assert_eq!(snap.num_layers(), 1);
            assert_eq!(snap.num_keys(), 0);
        }

        // Populated: names come from config, in config order.
        let b = crate::parse_bootstrap(&format!(
            "{base}layered_runtime:\n  layers:\n  - name: base_layer\n    static_layer:\n      a.b: 1\n      n:\n        deep: x\n  - name: override_layer\n    static_layer:\n      a.b: 2\n"
        ))
        .expect("valid");
        let snap = RuntimeSnapshot::from_bootstrap(&b);
        assert_eq!(snap.layer_names, vec!["base_layer", "override_layer"]);
        assert_eq!(snap.num_layers(), 2);
        // SPEC §2 N-5: num_keys counts FLATTENED LEAVES — `a.b` plus `n.deep`.
        assert_eq!(snap.num_keys(), 2);
        assert_eq!(snap.entries["a.b"].layer_values, vec!["1", "2"]);
        assert_eq!(snap.entries["a.b"].final_value, "2");
        assert_eq!(snap.entries["n.deep"].layer_values, vec!["x", ""]);
        assert_eq!(snap.entries["n.deep"].final_value, "x");
    }

    #[test]
    fn from_bootstrap_counts_flattened_leaves_not_declared_keys() {
        // SPEC §2 N-5, MEASURED: a layer declaring 11 top-level YAML keys, one
        // of them a nested map holding TWO leaves, yields num_keys: 12 — and
        // that equals the `entries` object size exactly.
        let mut yaml = String::from(
            "admin:\n  address:\n    socket_address:\n      address: 127.0.0.1\n      port_value: 9901\nlayered_runtime:\n  layers:\n  - name: l\n    static_layer:\n",
        );
        for i in 0..10 {
            yaml.push_str(&format!("      k{i}: v{i}\n"));
        }
        yaml.push_str("      nested:\n        one: a\n        two: b\n");
        let b = crate::parse_bootstrap(&yaml).expect("valid");
        let snap = RuntimeSnapshot::from_bootstrap(&b);
        assert_eq!(snap.num_keys(), 12, "10 flat + 2 nested leaves");
        assert_eq!(snap.entries.len(), snap.num_keys());
        assert!(
            !snap.entries.contains_key("nested"),
            "no intermediate entry"
        );
        assert_eq!(snap.entries["nested.one"].final_value, "a");
    }

    /// 109.1 Task 1 helpers: build a snapshot from yaml layer fragments, and a
    /// RuntimeFractionalPercent literal.
    fn snap(layer_yamls: &[&str]) -> RuntimeSnapshot {
        let layers: Vec<crate::RuntimeLayer> = layer_yamls.iter().map(|y| layer(y)).collect();
        let names = layers.iter().map(|l| l.name.clone()).collect();
        RuntimeSnapshot::from_layers(names, &layers)
    }

    fn rf(
        numerator: u32,
        denominator: crate::DenominatorType,
        key: Option<&str>,
    ) -> crate::RuntimeFractionalPercent {
        crate::RuntimeFractionalPercent {
            default_value: crate::FractionalPercent {
                numerator,
                denominator,
            },
            runtime_key: key.map(str::to_string),
        }
    }

    /// 109.1 SPEC §1.3: the evaluation cascade, pinned against EVERY measured
    /// cell of §1.1 (13, re-run at the split) and §1.2 (10 V-8 closure cells),
    /// plus the §1.3/§7 derived edges. One measured cell = one table row.
    #[test]
    fn route_fraction_gate_pins_every_measured_cell() {
        use crate::DenominatorType::{Hundred, Million};
        use FractionGate::{Always, Never};
        let empty = RuntimeSnapshot::default();
        let one = |v: &str| snap(&[&format!("name: l\nstatic_layer:\n  gate.k: {v}\n")]);

        // (label, snapshot, rf, expected)
        let ok_cells: Vec<(
            &str,
            RuntimeSnapshot,
            crate::RuntimeFractionalPercent,
            FractionGate,
        )> = vec![
            (
                "cell 1: absent key, default 100 -> Always",
                empty.clone(),
                rf(100, Hundred, Some("gate.k")),
                Always,
            ),
            (
                "cell 2: absent key, default 0 -> Never",
                empty.clone(),
                rf(0, Hundred, Some("gate.k")),
                Never,
            ),
            (
                "cell 3: key 0 overrides default 100 -> Never",
                one("0"),
                rf(100, Hundred, Some("gate.k")),
                Never,
            ),
            (
                "cell 4: key 100, default 0 -> Always",
                one("100"),
                rf(0, Hundred, Some("gate.k")),
                Always,
            ),
            (
                "cell 6: quoted \"0\" parses like the integer -> Never",
                one("\"0\""),
                rf(100, Hundred, Some("gate.k")),
                Never,
            ),
            (
                "cell 9: integer value is numerator over HUNDRED, not the default's MILLION -> Always",
                one("100"),
                rf(0, Million, Some("gate.k")),
                Always,
            ),
            (
                "cell 10: unparseable -> default 100 -> Always",
                one("abc"),
                rf(100, Hundred, Some("gate.k")),
                Always,
            ),
            (
                "cell 11: unparseable -> default 0 -> Never (both directions)",
                one("abc"),
                rf(0, Hundred, Some("gate.k")),
                Never,
            ),
            (
                "cell 12: 200 >= 100 -> Always",
                one("200"),
                rf(0, Hundred, Some("gate.k")),
                Always,
            ),
            (
                "cell 13: two layers, base 100 override 0, last-wins final \"0\" -> Never",
                snap(&[
                    "name: base\nstatic_layer:\n  gate.k: 100\n",
                    "name: over\nstatic_layer:\n  gate.k: 0\n",
                ]),
                rf(100, Hundred, Some("gate.k")),
                Never,
            ),
            (
                "cell B1: bool true -> default 100 -> Always",
                one("true"),
                rf(100, Hundred, Some("gate.k")),
                Always,
            ),
            (
                "cell B2: bool true -> default 0 -> Never",
                one("true"),
                rf(0, Hundred, Some("gate.k")),
                Never,
            ),
            (
                "cell B3: bool false is NOT 0 -> default 100 -> Always",
                one("false"),
                rf(100, Hundred, Some("gate.k")),
                Always,
            ),
            (
                "cell F1: yaml 0.0 self-heals to \"0\" via Display -> parses as 0 -> Never",
                one("0.0"),
                rf(100, Hundred, Some("gate.k")),
                Never,
            ),
            (
                "cell F2: yaml 100.0 self-heals to \"100\" -> Always",
                one("100.0"),
                rf(0, Hundred, Some("gate.k")),
                Always,
            ),
            (
                "cell N1: -7 -> default 100 -> Always",
                one("-7"),
                rf(100, Hundred, Some("gate.k")),
                Always,
            ),
            (
                "cell N2: -7 -> default 0 -> Never (both directions)",
                one("-7"),
                rf(0, Hundred, Some("gate.k")),
                Never,
            ),
            // §1.3/§7 derived edges (recorded, upstream-unmeasured where noted):
            (
                "edge: empty-string value -> default (final_value last-NON-EMPTY rule)",
                one("\"\""),
                rf(100, Hundred, Some("gate.k")),
                Always,
            ),
            (
                "edge: NaN spelling -> non-finite -> default",
                one("NaN"),
                rf(0, Hundred, Some("gate.k")),
                Never,
            ),
            (
                "edge: inf spelling -> non-finite -> default",
                one("inf"),
                rf(100, Hundred, Some("gate.k")),
                Always,
            ),
            (
                "edge: negative float -0.5 -> v < 0 -> default",
                one("-0.5"),
                rf(100, Hundred, Some("gate.k")),
                Always,
            ),
            (
                "edge: exponent 1e6 parses >= 100 -> Always (recorded; excluded from fixtures)",
                one("1e6"),
                rf(0, Hundred, Some("gate.k")),
                Always,
            ),
            (
                "edge: -0.0 == 0.0 in IEEE -> Never",
                one("-0.0"),
                rf(100, Hundred, Some("gate.k")),
                Never,
            ),
            (
                "edge: no runtime_key at all -> pure default 100 -> Always",
                empty.clone(),
                rf(100, Hundred, None),
                Always,
            ),
            (
                "edge: empty runtime_key is not consulted -> default 0 -> Never",
                one("0"),
                rf(0, Hundred, Some("")),
                Never,
            ),
        ];
        for (label, s, r, expected) in ok_cells {
            assert_eq!(s.route_fraction_gate(&r), Ok(expected), "{label}");
        }

        // Boot-fatal cells (CF-109-1: 0 < v < 100; the WIDENED class includes
        // non-integral floats and float-shaped strings — MEASURED, §1.2).
        for (label, s, r) in [
            (
                "cell 5: integer 50 is per-request nondeterministic upstream",
                one("50"),
                rf(100, Hundred, Some("gate.k")),
            ),
            (
                "cell F3: 0.5 parses upstream (NOT default) -> boot-fatal here",
                one("0.5"),
                rf(100, Hundred, Some("gate.k")),
            ),
            (
                "cell F4: 1.5 parsed AND per-request sampled upstream (GATED 1/40)",
                one("1.5"),
                rf(0, Hundred, Some("gate.k")),
            ),
            (
                "cell S1: quoted \"0.5\" parses like the float",
                one("\"0.5\""),
                rf(100, Hundred, Some("gate.k")),
            ),
        ] {
            assert!(
                matches!(
                    s.route_fraction_gate(&r),
                    Err(FractionGateError::NondeterministicValue { ref key, .. }) if key == "gate.k"
                ),
                "{label}"
            );
        }

        // CF-109-2, the SNAPSHOT-PREFIX rule (cells 7/8 + the two conservative
        // edges analysed in SPEC §3 D3): consulted key K is fatal iff ANY entry
        // starts with "K.".
        let map_snap = snap(&[
            "name: l\nstatic_layer:\n  gate.k:\n    numerator: 0\n    denominator: HUNDRED\n",
        ]);
        for (label, s, r) in [
            (
                "cell 7: map value at consulted key, default 100",
                map_snap.clone(),
                rf(100, Hundred, Some("gate.k")),
            ),
            (
                "cell 8: map value at consulted key, default 0",
                map_snap.clone(),
                rf(0, Hundred, Some("gate.k")),
            ),
            (
                "edge: scalar K beside literal dotted sibling K.foo -> conservatively fatal (recorded)",
                snap(&["name: l\nstatic_layer:\n  gate.k: 100\n  gate.k.foo: 1\n"]),
                rf(0, Hundred, Some("gate.k")),
            ),
        ] {
            assert!(
                matches!(
                    s.route_fraction_gate(&r),
                    Err(FractionGateError::MapShapedKey { ref key }) if key == "gate.k"
                ),
                "{label}"
            );
        }
        // ...but a DIFFERENT key's dotted entries do NOT trip the prefix rule,
        // and a PREFIX-SHARING SIBLING (gate.k2) does not either ("gate.k" is
        // not a string-prefix of "gate.k2" WITH the dot).
        let sibling = snap(&["name: l\nstatic_layer:\n  gate.k2: 100\n  other.map.leaf: 1\n"]);
        assert_eq!(
            sibling.route_fraction_gate(&rf(100, Hundred, Some("gate.k"))),
            Ok(Always),
            "prefix rule must use \"K.\" — a sibling gate.k2 entry is NOT a gate.k map"
        );

        // Non-deterministic default_value (numerator neither 0 nor the
        // denominator value) is fatal whenever the default is REACHED —
        // directly (no key) or via the unparseable fallback.
        for (label, s, r) in [
            (
                "edge: default 50/HUNDRED, no key",
                empty.clone(),
                rf(50, Hundred, None),
            ),
            (
                "edge: default 150/HUNDRED reached via unparseable value",
                one("abc"),
                rf(150, Hundred, Some("gate.k")),
            ),
        ] {
            assert!(
                matches!(
                    s.route_fraction_gate(&r),
                    Err(FractionGateError::NondeterministicDefault { .. })
                ),
                "{label}"
            );
        }
    }

    /// 109.1 Task 1: the infallible request-path wrapper. Ok maps directly;
    /// the Err arm (validated-unreachable in production — all three error
    /// classes are boot-fatal at every validation path) falls back to the
    /// default_value's sign, total and panic-free.
    #[test]
    fn route_fraction_passes_is_total_and_maps_the_gate() {
        use crate::DenominatorType::Hundred;
        let empty = RuntimeSnapshot::default();
        let fifty = snap(&["name: l\nstatic_layer:\n  gate.k: 50\n"]);
        assert!(
            empty.route_fraction_passes(&rf(100, Hundred, Some("gate.k"))),
            "Always -> true"
        );
        assert!(
            !empty.route_fraction_passes(&rf(0, Hundred, Some("gate.k"))),
            "Never -> false"
        );
        // Err fallback: nondeterministic value, default 100 -> true; default 0 -> false.
        assert!(
            fifty.route_fraction_passes(&rf(100, Hundred, Some("gate.k"))),
            "Err fallback follows default sign (non-zero)"
        );
        assert!(
            !fifty.route_fraction_passes(&rf(0, Hundred, Some("gate.k"))),
            "Err fallback follows default sign (zero)"
        );
    }
}
