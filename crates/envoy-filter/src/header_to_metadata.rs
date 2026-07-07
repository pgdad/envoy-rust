//! The `envoy.filters.http.header_to_metadata` filter (phase 34; §A-LOCKED against
//! envoyproxy/envoy:v1.33.0). Decode-side, Continue-only: for each request_rule, extract the
//! matched request header's value (or the rule's static `value` override — §A3 precedence) into
//! `req.dynamic_metadata[namespace][key]`; an absent header applies on_header_missing's static
//! `value`; a present-but-empty header writes nothing (§A4). Encode inert.
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse, header_ci};

#[derive(Debug, Clone)]
pub struct HeaderToMetadataFilter {
    rules: Vec<envoy_config::HeaderToMetadataRule>,
}

impl HeaderToMetadataFilter {
    pub(crate) fn new(cfg: &envoy_config::HeaderToMetadataConfig) -> Self {
        Self {
            rules: cfg.request_rules.clone(),
        }
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        for rule in &self.rules {
            let found = header_ci(&req.headers, &rule.header).map(str::to_owned);
            let action = match found.as_deref() {
                Some(v) if !v.is_empty() => rule
                    .on_header_present
                    .as_ref()
                    .map(|kv| (kv, v.to_string())),
                Some(_) => None, // present-but-empty → nothing (§A4)
                None => rule
                    .on_header_missing
                    .as_ref()
                    // validated (§A5d): on_header_missing always carries a `value`; unwrap_or_default is unreachable
                    .map(|kv| (kv, kv.value.clone().unwrap_or_default())),
            };
            if let Some((kv, header_value)) = action {
                let to_write = kv.value.clone().unwrap_or(header_value); // §A3: static value wins
                req.dynamic_metadata
                    .entry(kv.metadata_namespace.clone())
                    .or_default()
                    .insert(kv.key.clone(), to_write);
            }
        }
        Decision::Continue
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(ns: &str, key: &str, value: Option<&str>) -> envoy_config::HeaderToMetadataKeyValue {
        envoy_config::HeaderToMetadataKeyValue {
            metadata_namespace: ns.to_string(),
            key: key.to_string(),
            value: value.map(|s| s.to_string()),
            r#type: envoy_config::HeaderToMetadataType::String,
        }
    }

    fn rule(
        header: &str,
        on_header_present: Option<envoy_config::HeaderToMetadataKeyValue>,
        on_header_missing: Option<envoy_config::HeaderToMetadataKeyValue>,
    ) -> envoy_config::HeaderToMetadataRule {
        envoy_config::HeaderToMetadataRule {
            header: header.to_string(),
            on_header_present,
            on_header_missing,
        }
    }

    fn filter(rules: Vec<envoy_config::HeaderToMetadataRule>) -> HeaderToMetadataFilter {
        HeaderToMetadataFilter::new(&envoy_config::HeaderToMetadataConfig {
            request_rules: rules,
        })
    }

    fn req(headers: Vec<(&str, &str)>) -> FilterRequest {
        FilterRequest::test("GET", "/", &headers)
    }

    #[test]
    fn present_writes_header_value_and_continues() {
        let mut f = filter(vec![rule(
            "x-tier",
            Some(kv("envoy.lb", "tier", None)),
            None,
        )]);
        let mut r = req(vec![("x-tier", "prod")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(r.dynamic_metadata["envoy.lb"]["tier"], "prod");
    }

    #[test]
    fn present_static_value_overrides_header() {
        // §A3: static `value` in on_header_present overrides the actual header value
        let mut f = filter(vec![rule(
            "x-tier",
            Some(kv("envoy.lb", "tier", Some("forced"))),
            None,
        )]);
        let mut r = req(vec![("x-tier", "prod")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(r.dynamic_metadata["envoy.lb"]["tier"], "forced");
    }

    #[test]
    fn missing_writes_fallback() {
        // header absent → on_header_missing fires with its value
        let mut f = filter(vec![rule(
            "x-tier",
            None,
            Some(kv("envoy.lb", "tier", Some("dflt"))),
        )]);
        let mut r = req(vec![]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(r.dynamic_metadata["envoy.lb"]["tier"], "dflt");
    }

    #[test]
    fn present_but_empty_writes_nothing() {
        // §A4: present-but-empty → neither action fires
        let mut f = filter(vec![rule(
            "x-tier",
            Some(kv("envoy.lb", "tier", None)),
            Some(kv("envoy.lb", "tier", Some("dflt"))),
        )]);
        let mut r = req(vec![("x-tier", "")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert!(r.dynamic_metadata.is_empty());
    }

    #[test]
    fn case_insensitive_header_match() {
        // config header name in upper-case, request uses lower-case
        let mut f = filter(vec![rule(
            "X-Tier",
            Some(kv("envoy.lb", "tier", None)),
            None,
        )]);
        let mut r = req(vec![("x-tier", "prod")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(r.dynamic_metadata["envoy.lb"]["tier"], "prod");
    }

    #[test]
    fn multi_rule_composes() {
        // two rules → two namespaces written
        let mut f = filter(vec![
            rule("x-tier", Some(kv("envoy.lb", "tier", None)), None),
            rule("x-env", Some(kv("envoy.routing", "env", None)), None),
        ]);
        let mut r = req(vec![("x-tier", "prod"), ("x-env", "us-east-1")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(r.dynamic_metadata["envoy.lb"]["tier"], "prod");
        assert_eq!(r.dynamic_metadata["envoy.routing"]["env"], "us-east-1");
    }

    #[test]
    fn encode_is_inert() {
        let mut f = filter(vec![rule(
            "x-tier",
            Some(kv("envoy.lb", "tier", None)),
            None,
        )]);
        let mut resp = FilterResponse::test_200();
        assert!(matches!(f.encode_headers(&mut resp), Decision::Continue));
    }
}
