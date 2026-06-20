//! 30 (ADR-0073/0074): metadata-based subset LB. §6.2-LOCKED (live Envoy v1.33.0):
//! group endpoints per selector by the tuple of its `keys`' values; a route
//! `metadata_match` selects the selector whose `keys` SET EQUALS the match's keys,
//! then the value-tuple → the subset; superset match (endpoint metadata ⊇ match);
//! fallback per `LbSubsetFallbackPolicy`. NO config is fatal (§A divergence #1).
#![allow(dead_code)] // consumed by Task 5; remove the allow there.
use std::collections::{BTreeMap, BTreeSet};

use envoy_config::{LbSubsetConfig, LbSubsetFallbackPolicy};

#[derive(Debug)]
pub(crate) struct SubsetIndex {
    fallback: LbSubsetFallbackPolicy,
    default_subset: Option<BTreeMap<String, String>>,
    // one map per selector: key-set -> (value-tuple -> endpoint indices)
    selectors: Vec<SelectorIndex>,
}

#[derive(Debug)]
struct SelectorIndex {
    keys: BTreeSet<String>,
    subsets: BTreeMap<Vec<String>, Vec<usize>>, // value-tuple (in `keys` order) -> indices
}

/// The resolved eligible set for one request.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Eligible {
    All, // no lb_subset_config, or ANY_ENDPOINT/empty-selectors fallthrough
    Some(Vec<usize>),
    None, // NO_FALLBACK with no matching subset -> 503
}

impl SubsetIndex {
    /// Build over `endpoint_metadata[i]` = the `envoy.lb` map of endpoint i (empty if absent).
    pub(crate) fn build(
        cfg: &LbSubsetConfig,
        endpoint_metadata: &[BTreeMap<String, String>],
    ) -> SubsetIndex {
        let mut selectors: Vec<SelectorIndex> = Vec::with_capacity(cfg.subset_selectors.len());
        for selector in &cfg.subset_selectors {
            let mut subsets: BTreeMap<Vec<String>, Vec<usize>> = BTreeMap::new();
            for (i, meta) in endpoint_metadata.iter().enumerate() {
                // Endpoint is placed under this selector iff it has a value for
                // EVERY key in the selector (Envoy parity); endpoints missing any
                // selector key are EXCLUDED from that selector's subsets.
                if selector.keys.iter().all(|k| meta.contains_key(k)) {
                    let tuple: Vec<String> =
                        selector.keys.iter().map(|k| meta[k].clone()).collect();
                    subsets.entry(tuple).or_default().push(i);
                }
            }
            selectors.push(SelectorIndex {
                keys: selector.keys.iter().cloned().collect(),
                subsets,
            });
        }
        SubsetIndex {
            fallback: cfg.fallback_policy,
            default_subset: cfg.default_subset.clone(),
            selectors,
        }
    }

    /// Find the selector whose key-set equals `m`'s keys, then look up the
    /// value-tuple (in that selector's `keys` order). Superset matching is already
    /// encoded at build time: an endpoint is in the subset iff its metadata is a
    /// superset of the tuple (the tuple is keyed ONLY on the selector's keys, so
    /// extra endpoint keys are simply not part of the tuple).
    fn lookup(&self, m: &BTreeMap<String, String>) -> Option<Vec<usize>> {
        let want: BTreeSet<&String> = m.keys().collect();
        let selector = self
            .selectors
            .iter()
            .find(|s| s.keys.iter().collect::<BTreeSet<&String>>() == want)?;
        // Build the value-tuple in the selector's `keys` order (BTreeSet iterates sorted).
        let tuple: Vec<String> = selector.keys.iter().map(|k| m[k].clone()).collect();
        selector.subsets.get(&tuple).cloned()
    }

    fn fallback(&self) -> Eligible {
        match self.fallback {
            LbSubsetFallbackPolicy::NoFallback => Eligible::None,
            LbSubsetFallbackPolicy::AnyEndpoint => Eligible::All,
            LbSubsetFallbackPolicy::DefaultSubset => match &self.default_subset {
                // §A: a missing/empty default_subset matches all endpoints.
                None => Eligible::All,
                Some(d) if d.is_empty() => Eligible::All,
                Some(d) => match self.lookup(d) {
                    Option::Some(idxs) => Eligible::Some(idxs),
                    Option::None => Eligible::None,
                },
            },
        }
    }

    /// Resolve the eligible endpoint set for a route `metadata_match` (None = no match config).
    pub(crate) fn resolve(&self, metadata_match: Option<&BTreeMap<String, String>>) -> Eligible {
        // 3. empty subset_selectors -> the layer is disabled -> Eligible::All (§A edge).
        if self.selectors.is_empty() {
            return Eligible::All;
        }
        // 1. metadata_match present (and non-empty): selector-key-set match then value-tuple.
        match metadata_match {
            Some(m) if !m.is_empty() => match self.lookup(m) {
                Option::Some(idxs) => Eligible::Some(idxs),
                Option::None => self.fallback(),
            },
            // 2. no metadata_match (or empty) -> fallback.
            _ => self.fallback(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::LbSubsetSelector;

    fn meta(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // §A endpoints: A = {stage:prod, version:v2} (index 0), B = {stage:canary, version:v1} (index 1).
    fn ab_endpoints() -> Vec<BTreeMap<String, String>> {
        vec![
            meta(&[("stage", "prod"), ("version", "v2")]),
            meta(&[("stage", "canary"), ("version", "v1")]),
        ]
    }

    fn cfg(
        fallback: LbSubsetFallbackPolicy,
        default_subset: Option<BTreeMap<String, String>>,
    ) -> LbSubsetConfig {
        LbSubsetConfig {
            fallback_policy: fallback,
            subset_selectors: vec![LbSubsetSelector {
                keys: vec!["stage".into()],
            }],
            default_subset,
        }
    }

    // ----- §A pinned regression oracle (live-Envoy v1.33.0 confirmed) -----

    #[test]
    fn oracle_prod_no_fallback_eligible_a() {
        let idx = SubsetIndex::build(
            &cfg(LbSubsetFallbackPolicy::NoFallback, None),
            &ab_endpoints(),
        );
        assert_eq!(
            idx.resolve(Some(&meta(&[("stage", "prod")]))),
            Eligible::Some(vec![0])
        );
    }

    #[test]
    fn oracle_canary_no_fallback_eligible_b() {
        let idx = SubsetIndex::build(
            &cfg(LbSubsetFallbackPolicy::NoFallback, None),
            &ab_endpoints(),
        );
        assert_eq!(
            idx.resolve(Some(&meta(&[("stage", "canary")]))),
            Eligible::Some(vec![1])
        );
    }

    #[test]
    fn oracle_prod_superset_match_eligible_a() {
        // A's metadata {stage:prod, version:v2} is a SUPERSET of the match {stage:prod}.
        // Grouping by the selector-key-tuple (stage) alone yields superset matching.
        let idx = SubsetIndex::build(
            &cfg(LbSubsetFallbackPolicy::NoFallback, None),
            &ab_endpoints(),
        );
        assert_eq!(
            idx.resolve(Some(&meta(&[("stage", "prod")]))),
            Eligible::Some(vec![0])
        );
    }

    #[test]
    fn oracle_nonexistent_no_fallback_none() {
        let idx = SubsetIndex::build(
            &cfg(LbSubsetFallbackPolicy::NoFallback, None),
            &ab_endpoints(),
        );
        assert_eq!(
            idx.resolve(Some(&meta(&[("stage", "nonexistent")]))),
            Eligible::None
        );
    }

    #[test]
    fn oracle_nonexistent_default_subset_prod_eligible_a() {
        let idx = SubsetIndex::build(
            &cfg(
                LbSubsetFallbackPolicy::DefaultSubset,
                Some(meta(&[("stage", "prod")])),
            ),
            &ab_endpoints(),
        );
        assert_eq!(
            idx.resolve(Some(&meta(&[("stage", "nonexistent")]))),
            Eligible::Some(vec![0])
        );
    }

    #[test]
    fn oracle_no_metadata_match_no_fallback_none() {
        let idx = SubsetIndex::build(
            &cfg(LbSubsetFallbackPolicy::NoFallback, None),
            &ab_endpoints(),
        );
        assert_eq!(idx.resolve(None), Eligible::None);
    }

    // ----- additional pinned cases from the Task 4 spec -----

    #[test]
    fn any_endpoint_resolves_to_all() {
        // {stage:nonexistent} under ANY_ENDPOINT -> Eligible::All ([A, B] marker).
        let idx = SubsetIndex::build(
            &cfg(LbSubsetFallbackPolicy::AnyEndpoint, None),
            &ab_endpoints(),
        );
        assert_eq!(
            idx.resolve(Some(&meta(&[("stage", "nonexistent")]))),
            Eligible::All
        );
        // no metadata_match under ANY_ENDPOINT also -> All.
        assert_eq!(idx.resolve(None), Eligible::All);
    }

    #[test]
    fn no_metadata_match_falls_back_per_policy() {
        // resolve(None) under DEFAULT_SUBSET {stage:prod} -> [A].
        let idx = SubsetIndex::build(
            &cfg(
                LbSubsetFallbackPolicy::DefaultSubset,
                Some(meta(&[("stage", "prod")])),
            ),
            &ab_endpoints(),
        );
        assert_eq!(idx.resolve(None), Eligible::Some(vec![0]));
    }

    #[test]
    fn empty_subset_selectors_resolves_to_all() {
        // §A disabled-layer edge: empty subset_selectors -> Eligible::All even under NO_FALLBACK.
        let cfg = LbSubsetConfig {
            fallback_policy: LbSubsetFallbackPolicy::NoFallback,
            subset_selectors: vec![],
            default_subset: None,
        };
        let idx = SubsetIndex::build(&cfg, &ab_endpoints());
        assert_eq!(
            idx.resolve(Some(&meta(&[("stage", "prod")]))),
            Eligible::All
        );
        assert_eq!(idx.resolve(None), Eligible::All);
    }

    #[test]
    fn default_subset_empty_matches_all() {
        // DEFAULT_SUBSET with empty/missing default -> Eligible::All.
        let idx = SubsetIndex::build(
            &cfg(LbSubsetFallbackPolicy::DefaultSubset, Some(BTreeMap::new())),
            &ab_endpoints(),
        );
        assert_eq!(
            idx.resolve(Some(&meta(&[("stage", "nonexistent")]))),
            Eligible::All
        );
    }
}
