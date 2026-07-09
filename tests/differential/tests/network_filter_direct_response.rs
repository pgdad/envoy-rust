use std::path::Path;

#[tokio::test]
async fn network_filter_direct_response_fixture() -> anyhow::Result<()> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/0071-network-filter-direct-response");
    differential::run_fixture(&fixture).await
}
