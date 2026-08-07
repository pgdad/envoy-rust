//! 108.1 D3: the runtime snapshot store — the in-memory view of a parsed
//! `layered_runtime` block, shaped exactly as upstream Envoy's admin
//! `GET /runtime` exposes it.
//!
//! This module is the ENGINE; the serde schema it consumes (`LayeredRuntime`,
//! `RuntimeLayer`, `RuntimeValue`) lives in [`crate::bootstrap`]. That split
//! follows the landed `matcher.rs` precedent, where the `HeaderMatcher` schema
//! sits in `bootstrap.rs` and the matching engine sits in its own module.
//!
//! **Nothing reads this store yet.** 108.1 builds the PRODUCER; sibling 108.2
//! adds the admin `GET /runtime` endpoint and the nine `runtime.*` stats that
//! observe it. This slice deliberately wires NEITHER the `RuntimeUInt32`
//! (`status_code_filter`) NOR the `RuntimeFractionalPercent` (CSRF) consumer, so
//! every existing "no runtime subsystem" assertion in the tree stays true.
//!
//! All ordering is `BTreeMap`-canonical. That is not incidental: sibling 108.2's
//! differential fixture rests on `serde_json::Map` being a `BTreeMap` (the
//! workspace enables `preserve_order` nowhere), and a canonically-ordered store
//! keeps the renderer deterministic before `serde_json` is involved.

use crate::{RuntimeLayer, RuntimeValue};
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
}
