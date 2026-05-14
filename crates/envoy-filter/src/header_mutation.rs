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
// Fields are only read in `#[cfg(test)]` code at Task 3; the real readers
// land at Task 4's decode_headers / encode_headers implementations, which
// removes this attribute.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct HeaderMutationFilter {
    request_mutations: Vec<RuntimeHeaderMutation>,
    response_mutations: Vec<RuntimeHeaderMutation>,
}

/// One lowered mutation. `key` is lowercased once at build time so the
/// runtime hot path does no per-request case folding for Append, and the
/// Overwrite search compares against `to_ascii_lowercase()` of each existing
/// entry. Per 07.2 SPEC §6 signpost 4.
// Fields are only read in `#[cfg(test)]` code at Task 3; consumed by Task 4's
// apply_mutations implementation, which removes this attribute.
#[allow(dead_code)]
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

    /// Task 3 stub — returns `Continue`. Real semantics land at Task 4.
    pub(crate) fn decode_headers(&mut self, _req: &mut FilterRequest) -> Decision {
        Decision::Continue
    }

    /// Task 3 stub — returns `Continue`. Real semantics land at Task 4.
    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
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
        let instance = crate::instance::HttpFilterInstance::build(&hf, 0).unwrap();
        assert!(matches!(
            instance,
            crate::instance::HttpFilterInstance::HeaderMutation(_)
        ));
    }

    #[test]
    fn decode_headers_stub_returns_continue_at_task_3() {
        // Task 3 stubs decode/encode as Continue; Task 4 lands the real
        // mutation semantics. This test is REPLACED at Task 4.
        let mut filter = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        let mut req = FilterRequest {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: vec![],
            body: None,
        };
        assert!(matches!(
            filter.decode_headers(&mut req),
            Decision::Continue
        ));
    }

    #[test]
    fn encode_headers_stub_returns_continue_at_task_3() {
        // Replaced at Task 4.
        let mut filter = HeaderMutationFilter::build_from_config(&cfg(
            vec![],
            vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
        ))
        .unwrap();
        let mut resp = FilterResponse {
            status: 200,
            reason: None,
            headers: vec![],
            body: bytes::Bytes::new(),
        };
        assert!(matches!(
            filter.encode_headers(&mut resp),
            Decision::Continue
        ));
    }
}
