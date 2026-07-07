//! The `envoy.filters.http.set_metadata` filter (phase 33; §A-LOCKED against
//! envoyproxy/envoy:v1.33.0). Decode-side, Continue-only: merges each config
//! entry's static string `value` map into `req.dynamic_metadata` under the
//! entry's `metadata_namespace`, honoring `allow_overwrite`. Encode inert.
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

#[derive(Debug, Clone)]
pub struct SetMetadataFilter {
    metadata: Vec<envoy_config::MetadataEntry>,
}
impl SetMetadataFilter {
    pub(crate) fn new(cfg: &envoy_config::SetMetadataConfig) -> Self {
        Self {
            metadata: cfg.metadata.clone(),
        }
    }
    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        for entry in &self.metadata {
            let ns = req
                .dynamic_metadata
                .entry(entry.metadata_namespace.clone())
                .or_default();
            for (k, v) in &entry.value {
                if entry.allow_overwrite || !ns.contains_key(k) {
                    ns.insert(k.clone(), v.clone());
                }
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

    fn entry(ns: &str, kv: &[(&str, &str)], allow_overwrite: bool) -> envoy_config::MetadataEntry {
        envoy_config::MetadataEntry {
            metadata_namespace: ns.to_string(),
            value: kv
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            allow_overwrite,
        }
    }

    fn filter(entries: Vec<envoy_config::MetadataEntry>) -> SetMetadataFilter {
        SetMetadataFilter::new(&envoy_config::SetMetadataConfig { metadata: entries })
    }

    fn req() -> FilterRequest {
        FilterRequest::test("GET", "/", &[])
    }

    #[test]
    fn writes_value_under_namespace_and_continues() {
        let mut f = filter(vec![entry("envoy.test", &[("tier", "prod")], false)]);
        let mut r = req();
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(r.dynamic_metadata["envoy.test"]["tier"], "prod");
    }

    #[test]
    fn multi_namespace_multi_entry() {
        let mut f = filter(vec![
            entry("ns.one", &[("a", "1")], false),
            entry("ns.two", &[("b", "2")], false),
        ]);
        let mut r = req();
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(r.dynamic_metadata["ns.one"]["a"], "1");
        assert_eq!(r.dynamic_metadata["ns.two"]["b"], "2");
    }

    #[test]
    fn allow_overwrite_false_keeps_existing() {
        let mut f = filter(vec![entry("envoy.test", &[("tier", "prod")], false)]);
        let mut r = req();
        r.dynamic_metadata
            .entry("envoy.test".into())
            .or_default()
            .insert("tier".into(), "stage".into());
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(r.dynamic_metadata["envoy.test"]["tier"], "stage");
    }

    #[test]
    fn allow_overwrite_true_overwrites() {
        let mut f = filter(vec![entry("envoy.test", &[("tier", "prod")], true)]);
        let mut r = req();
        r.dynamic_metadata
            .entry("envoy.test".into())
            .or_default()
            .insert("tier".into(), "stage".into());
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(r.dynamic_metadata["envoy.test"]["tier"], "prod");
    }

    #[test]
    fn encode_is_inert() {
        let mut f = filter(vec![entry("envoy.test", &[("tier", "prod")], false)]);
        let mut resp = FilterResponse::test_200();
        assert!(matches!(f.encode_headers(&mut resp), Decision::Continue));
    }
}
