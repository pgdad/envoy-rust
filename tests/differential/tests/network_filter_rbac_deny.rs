use std::path::Path;

/// Phase 67.1 fixture 0072: `[rbac(action: DENY, any), echo]`.
///
/// The DENY path is VACUITY-PRONE: `ByteExact` is a bare inequality check, so
/// "both proxies returned zero bytes" would pass even if envoy-rust never
/// implemented RBAC and simply failed to write. The `rbac.denied == 1`
/// assertion in `expectations.yaml` is what makes this a witness.
#[tokio::test]
async fn network_filter_rbac_deny_fixture() -> anyhow::Result<()> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/0072-network-filter-rbac-deny");
    differential::run_fixture(&fixture).await
}
