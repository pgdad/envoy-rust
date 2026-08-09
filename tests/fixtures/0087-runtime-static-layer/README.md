# Fixture 0087 — runtime static layer (`GET /runtime` + the nine `runtime.*` stats)

Sub-phase 108.2 D6 (phase-108 runtime family opener; ADR-0171/0172/0173/0174).
The first differential of the runtime subsystem: both proxies parse the SAME
two-layer `layered_runtime` block and must serve equivalent `GET /runtime`
snapshots and equal `runtime.*` stat values.

## Shape

- **Backend-free, CLUSTER-FREE** (`clusters: []`) — fully verifiable on the
  development host (no `192.168.65.2` bridge-IP exposure). The echo listener
  exists ONLY because `run_fixture`'s data-plane accept-ready wait needs a
  `{{PORT}}` listener; no traffic is driven at it.
- **Driver:** `admin_scrape`, `pre_requests: []`, TWO `/runtime` scrapes
  (`BodyRule::JsonShape` permits one `required_subtree` per rule — scrape 1
  anchors the whole 14-entry `entries` object, scrape 2 anchors `layers`),
  plus nine bilateral `expected_stats` assertions (the 108.2 harness
  extension).
- **Config divergence between the two YAMLs** (fixture-0001 precedent): the
  envoy-rust listener uses the NAME-ONLY echo filter spelling — envoy-rust's
  `typed_config` enum does not model the echo `@type`. Everything else,
  including the whole `layered_runtime` block, is byte-identical.

## What is witnessed

1. Scalar stringification (`true`/`false`/`42`/`-7`/`1.5`/`hello`/`"42"`/`""`).
2. Arbitrary-depth flattening (`diff.nested.deeper.leaf`; no intermediate
   `diff.nested` entry — its absence from the EXPECTED subtree is asserted
   because the subtree comparison is whole-object equality).
3. Two-layer slot ordering (`layer_values` in config order), the `""`-absent
   marker, and last-NON-EMPTY-wins precedence (`empty.in.override` —
   "last wins" would return `""`).
4. `layers` names in config order.
5. **Stat witnesses — the vacuous-pass ledger** (lib.rs:4504-4507 rule):
   `num_keys: 14` (flattened LEAVES: 13 declared + 1 override-only — NOT the
   13 top-level declared keys), `num_layers: 2`, `load_success: 1`,
   `override_dir_not_exists: 1` are the FOUR real witnesses. The five
   `value: 0` entries pass vacuously when the name is absent; their
   envoy-rust presence is pinned in-process by
   `envoy-bin::runtime_stats::registers_all_nine_runtime_stats_with_measured_values_and_kinds`.

## Deliberately excluded (recorded, not witnessed)

- Unquoted `y`/`n`/`on`/`off` (CF-108-4, ADR-0173: YAML 1.1 booleanizes
  upstream, YAML 1.2 does not here).
- Floats other than `1.5` (CF-108-5, ADR-0174: upstream preserves the raw
  SOURCE TEXT — `1e6`→`"1e6"`, `1.50`→`"1.50"` — envoy-rust renders `f64`
  Display; `1.5` is the Display-stable agreeing cell). Non-finite floats
  likewise (`".nan"` upstream vs `"NaN"` here).
- `numerator`-bearing nested maps (CF-108-3: protobuf text-format upstream).
- The `envoy.reloadable_features.` prefix (non-fatal `envoy_bug` stderr noise).
- `POST /runtime_modify` (CF-108-2: upstream POST-only / 405-on-GET;
  envoy-rust 404).
- NOT-MEASURED upstream cells: same-layer flattened-key collision (M-1),
  empty nested map value (M-7), empty/dot-bearing key segments (N-5),
  explicit-null `static_layer:` (N-4).

## Expected values

Measured against `envoyproxy/envoy:v1.33.0`
(`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`)
with this fixture's exact config at the 108.2 PLAN-write (2026-08-08):
1508-byte responses, per-request key-order shuffle (distinct md5s), 14
entries, `layers: ["base_layer","override_layer"]`, and the nine stat values
in `expectations.yaml`. Re-measured live by every fixture run — the YAML
carries the values, this README carries the provenance.
