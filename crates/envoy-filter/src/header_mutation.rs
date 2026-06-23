//! `envoy.filters.http.header_mutation` runtime filter.
//!
//! Hand-rolled per D-3.2 (*Every individual filter* is on the
//! Must-be-written-from-scratch list). Mutates the `headers` list of the
//! 07.1-landed `FilterRequest` / `FilterResponse` value types. Synchronous;
//! no async, no `dyn`-dispatch.

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

/// The `envoy.filters.http.header_mutation` runtime filter. Holds the
/// build-time-lowered request/response mutation lists. Per 07.2 SPEC §6
/// signpost 3 the lists are held directly (`Vec<RuntimeHeaderMutation>`),
/// not `Arc`-wrapped — the per-request `FilterPipeline` clone copies them;
/// cheap for 07.2's 2-4-entry fixture.
#[derive(Debug, Clone)]
pub struct HeaderMutationFilter {
    request_mutations: Vec<RuntimeHeaderMutation>,
    response_mutations: Vec<RuntimeHeaderMutation>,
}

/// One lowered mutation. `key` is lowercased once at build time so the
/// runtime hot path does no per-request case folding for Append, and the
/// Overwrite search compares against `to_ascii_lowercase()` of each existing
/// entry. Per 07.2 SPEC §6 signpost 4.
#[derive(Debug, Clone)]
struct RuntimeHeaderMutation {
    key: String,
    value: String,
    action: RuntimeAppendAction,
}

#[derive(Debug, Clone, Copy)]
enum RuntimeAppendAction {
    /// `APPEND_IF_EXISTS_OR_ADD` — push a new entry (RFC 7230 §3.2.2 permits
    /// duplicate field names; semantics are list-valued).
    Append,
    /// `OVERWRITE_IF_EXISTS_OR_ADD` — case-insensitive remove-then-push.
    Overwrite,
}

impl HeaderMutationFilter {
    /// Lower an `envoy_config::HeaderMutationConfig` into the runtime filter.
    /// The Task 2 validator already rejected unsupported `append_action`s and
    /// invalid keys at config-load time; `map_entry`'s re-check is
    /// defense-in-depth at the framework crate boundary.
    pub(crate) fn build_from_config(
        cfg: &envoy_config::HeaderMutationConfig,
    ) -> Result<Self, FilterError> {
        let request_mutations = cfg
            .mutations
            .request_mutations
            .iter()
            .map(map_entry)
            .collect::<Result<Vec<_>, _>>()?;
        let response_mutations = cfg
            .mutations
            .response_mutations
            .iter()
            .map(map_entry)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            request_mutations,
            response_mutations,
        })
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        apply_mutations(&mut req.headers, &self.request_mutations);
        Decision::Continue
    }

    pub(crate) fn encode_headers(&mut self, resp: &mut FilterResponse) -> Decision {
        apply_mutations(&mut resp.headers, &self.response_mutations);
        Decision::Continue
    }
}

/// Lower one config entry into a `RuntimeHeaderMutation`. Lowercases the key.
/// The unsupported `AppendAction`s are rejected here as defense-in-depth —
/// the Task 2 `envoy-config` validator is the earlier (and the
/// operator-facing) catch.
fn map_entry(
    entry: &envoy_config::HeaderMutationEntry,
) -> Result<RuntimeHeaderMutation, FilterError> {
    let action = match entry.append.append_action {
        envoy_config::AppendAction::AppendIfExistsOrAdd => RuntimeAppendAction::Append,
        envoy_config::AppendAction::OverwriteIfExistsOrAdd => RuntimeAppendAction::Overwrite,
        unsupported @ (envoy_config::AppendAction::AddIfAbsent
        | envoy_config::AppendAction::OverwriteIfExists) => {
            // `position: 0` is a placeholder — `map_entry` runs inside
            // `build_from_config` and has no access to the filter-chain
            // position. The operator-facing position is carried by the
            // `envoy-config` validator's typed errors (the primary catch);
            // this is the defense-in-depth re-check at the framework boundary.
            return Err(FilterError::UnsupportedFilterType {
                position: 0,
                name: format!("AppendAction::{unsupported:?}"),
            });
        }
    };
    Ok(RuntimeHeaderMutation {
        key: entry.append.header.key.to_ascii_lowercase(),
        value: entry.append.header.value.clone(),
        action,
    })
}

/// Apply a mutation list to a header vector in slice (= YAML declaration)
/// order. Per 07.2 SPEC §6 signpost 8: last Append appends last; for a given
/// key the last Overwrite wins (each Overwrite removes prior same-key entries).
fn apply_mutations(headers: &mut Vec<(String, String)>, mutations: &[RuntimeHeaderMutation]) {
    for mutation in mutations {
        match mutation.action {
            RuntimeAppendAction::Append => {
                // RFC 7230 §3.2.2: duplicate field names are permitted.
                headers.push((mutation.key.clone(), mutation.value.clone()));
            }
            RuntimeAppendAction::Overwrite => {
                // `mutation.key` is already lowercased at build time; case-fold
                // each existing entry's name for the removal scan.
                headers.retain(|(k, _v)| k.to_ascii_lowercase() != mutation.key);
                headers.push((mutation.key.clone(), mutation.value.clone()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{
        AppendAction, HeaderMutationConfig, HeaderMutationEntry, HeaderValue, HeaderValueOption,
        Mutations,
    };

    fn entry(key: &str, value: &str, action: AppendAction) -> HeaderMutationEntry {
        HeaderMutationEntry {
            append: HeaderValueOption {
                header: HeaderValue {
                    key: key.to_string(),
                    value: value.to_string(),
                },
                append_action: action,
            },
        }
    }

    fn cfg(
        request_mutations: Vec<HeaderMutationEntry>,
        response_mutations: Vec<HeaderMutationEntry>,
    ) -> HeaderMutationConfig {
        HeaderMutationConfig {
            mutations: Mutations {
                request_mutations,
                response_mutations,
            },
        }
    }

    fn req_with(headers: Vec<(&str, &str)>) -> FilterRequest {
        FilterRequest {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: headers
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: None,
            dynamic_metadata: std::collections::BTreeMap::new(),
        }
    }

    fn resp_with(headers: Vec<(&str, &str)>) -> FilterResponse {
        FilterResponse {
            status: 200,
            reason: None,
            headers: headers
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: bytes::Bytes::new(),
        }
    }

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn build_from_config_on_empty_mutations_returns_empty_filter() {
        let filter = HeaderMutationFilter::build_from_config(&cfg(vec![], vec![])).unwrap();
        assert_eq!(filter.request_mutations.len(), 0);
        assert_eq!(filter.response_mutations.len(), 0);
    }

    #[test]
    fn build_from_config_on_single_append_entry_lowercases_key_and_keeps_value() {
        let filter = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("X-Foo", "Bar", AppendAction::AppendIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        assert_eq!(filter.request_mutations.len(), 1);
        let m = &filter.request_mutations[0];
        assert_eq!(m.key, "x-foo"); // lowercased at build time
        assert_eq!(m.value, "Bar"); // value preserved verbatim
        assert!(matches!(m.action, RuntimeAppendAction::Append));
    }

    #[test]
    fn build_from_config_on_single_overwrite_entry_maps_action() {
        let filter = HeaderMutationFilter::build_from_config(&cfg(
            vec![],
            vec![entry("x-resp", "v", AppendAction::OverwriteIfExistsOrAdd)],
        ))
        .unwrap();
        assert_eq!(filter.response_mutations.len(), 1);
        assert!(matches!(
            filter.response_mutations[0].action,
            RuntimeAppendAction::Overwrite
        ));
    }

    #[test]
    fn build_from_config_on_unsupported_append_action_returns_err() {
        // Defense-in-depth: the envoy-config validator (07.2 Task 2) catches
        // these earlier, but `map_entry` re-checks at the framework boundary.
        let err = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-a", "v", AppendAction::AddIfAbsent)],
            vec![],
        ))
        .unwrap_err();
        assert!(matches!(err, FilterError::UnsupportedFilterType { .. }));
    }

    #[test]
    fn http_filter_instance_build_on_header_mutation_produces_header_mutation_variant() {
        let hf = envoy_config::HttpFilter {
            name: "envoy.filters.http.header_mutation".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::HeaderMutation(cfg(
                vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
                vec![],
            )),
        };
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let instance =
            crate::instance::HttpFilterInstance::build(&hf, &registry, "test_prefix").unwrap();
        assert!(matches!(
            instance,
            crate::instance::HttpFilterInstance::HeaderMutation(_)
        ));
    }

    #[test]
    fn append_on_absent_key_adds_entry() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-foo", "bar", AppendAction::AppendIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![]);
        assert!(matches!(f.decode_headers(&mut req), Decision::Continue));
        assert_eq!(req.headers, owned(&[("x-foo", "bar")]));
    }

    #[test]
    fn append_on_present_key_adds_duplicate() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-foo", "bar", AppendAction::AppendIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![("x-foo", "original")]);
        f.decode_headers(&mut req);
        assert_eq!(
            req.headers,
            owned(&[("x-foo", "original"), ("x-foo", "bar")])
        );
    }

    #[test]
    fn overwrite_on_absent_key_adds_entry() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-foo", "bar", AppendAction::OverwriteIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![]);
        f.decode_headers(&mut req);
        assert_eq!(req.headers, owned(&[("x-foo", "bar")]));
    }

    #[test]
    fn overwrite_on_present_key_replaces_with_exactly_one_entry() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-foo", "bar", AppendAction::OverwriteIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![("x-foo", "original")]);
        f.decode_headers(&mut req);
        assert_eq!(req.headers, owned(&[("x-foo", "bar")]));
    }

    #[test]
    fn overwrite_is_case_insensitive_on_the_existing_entry() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-foo", "bar", AppendAction::OverwriteIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        // existing entry has mixed-case name; Overwrite case-folds the match.
        let mut req = req_with(vec![("X-Foo", "original")]);
        f.decode_headers(&mut req);
        assert_eq!(req.headers, owned(&[("x-foo", "bar")]));
    }

    #[test]
    fn multiple_append_entries_apply_in_declaration_order() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![
                entry("x-a", "1", AppendAction::AppendIfExistsOrAdd),
                entry("x-b", "2", AppendAction::AppendIfExistsOrAdd),
                entry("x-a", "3", AppendAction::AppendIfExistsOrAdd),
            ],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![]);
        f.decode_headers(&mut req);
        assert_eq!(
            req.headers,
            owned(&[("x-a", "1"), ("x-b", "2"), ("x-a", "3")])
        );
    }

    #[test]
    fn multiple_overwrite_entries_last_for_a_key_wins() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![
                entry("x-a", "first", AppendAction::OverwriteIfExistsOrAdd),
                entry("x-a", "second", AppendAction::OverwriteIfExistsOrAdd),
            ],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![]);
        f.decode_headers(&mut req);
        assert_eq!(req.headers, owned(&[("x-a", "second")]));
    }

    #[test]
    fn mix_of_append_and_overwrite_applies_in_order() {
        // Append x-a:1, Append x-a:2, Overwrite x-a:final → Overwrite removes
        // both prior x-a entries, pushes one.
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![
                entry("x-a", "1", AppendAction::AppendIfExistsOrAdd),
                entry("x-a", "2", AppendAction::AppendIfExistsOrAdd),
                entry("x-a", "final", AppendAction::OverwriteIfExistsOrAdd),
            ],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![("x-keep", "kept")]);
        f.decode_headers(&mut req);
        assert_eq!(req.headers, owned(&[("x-keep", "kept"), ("x-a", "final")]));
    }

    #[test]
    fn empty_mutations_is_no_op_on_decode() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(vec![], vec![])).unwrap();
        let mut req = req_with(vec![("host", "example.com")]);
        let before = req.headers.clone();
        f.decode_headers(&mut req);
        assert_eq!(req.headers, before);
    }

    #[test]
    fn empty_mutations_is_no_op_on_encode() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(vec![], vec![])).unwrap();
        let mut resp = resp_with(vec![("content-length", "0")]);
        let before = resp.headers.clone();
        f.encode_headers(&mut resp);
        assert_eq!(resp.headers, before);
    }

    #[test]
    fn decode_headers_returns_continue_after_applying() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![]);
        assert!(matches!(f.decode_headers(&mut req), Decision::Continue));
    }

    #[test]
    fn encode_headers_returns_continue_after_applying() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![],
            vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
        ))
        .unwrap();
        let mut resp = resp_with(vec![]);
        assert!(matches!(f.encode_headers(&mut resp), Decision::Continue));
    }

    #[test]
    fn round_trip_via_filter_pipeline_decode() {
        // Build a real [HeaderMutation, Router] pipeline; decode_headers walks
        // declaration order; the request carries the stamp afterward.
        let filters = vec![
            envoy_config::HttpFilter {
                name: "envoy.filters.http.header_mutation".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::HeaderMutation(cfg(
                    vec![entry("x-foo", "bar", AppendAction::AppendIfExistsOrAdd)],
                    vec![],
                )),
            },
            envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            },
        ];
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let mut pipeline =
            crate::FilterPipeline::build_from_config(&filters, &registry, "test_prefix").unwrap();
        let mut req = req_with(vec![("host", "example.com")]);
        assert!(matches!(
            pipeline.decode_headers(&mut req),
            Decision::Continue
        ));
        assert!(req.headers.iter().any(|(k, v)| k == "x-foo" && v == "bar"));
    }

    #[test]
    fn iteration_order_on_encode_via_filter_pipeline() {
        // Reverse-iteration on encode reaches HeaderMutation after Router's
        // no-op. The response carries the response-side stamp afterward.
        let filters = vec![
            envoy_config::HttpFilter {
                name: "envoy.filters.http.header_mutation".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::HeaderMutation(cfg(
                    vec![],
                    vec![entry("x-resp", "stamp", AppendAction::AppendIfExistsOrAdd)],
                )),
            },
            envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            },
        ];
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let mut pipeline =
            crate::FilterPipeline::build_from_config(&filters, &registry, "test_prefix").unwrap();
        let mut resp = resp_with(vec![("content-length", "0")]);
        assert!(matches!(
            pipeline.encode_headers(&mut resp),
            Decision::Continue
        ));
        assert!(
            resp.headers
                .iter()
                .any(|(k, v)| k == "x-resp" && v == "stamp")
        );
    }
}
