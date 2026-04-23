use std::path::Path;

#[tokio::test]
async fn echo_fixture() -> anyhow::Result<()> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/0001-tcp-echo");
    differential::run_fixture(&fixture).await
}
