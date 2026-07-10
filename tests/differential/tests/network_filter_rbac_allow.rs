use std::path::Path;

/// Phase 67.1 fixture 0073: `[rbac(action: ALLOW, any), echo]`.
///
/// The family's first differential proof that a NON-TERMINAL network filter runs
/// and then YIELDS to the terminal filter — i.e. of the chain iteration protocol
/// itself. The payload round-trips byte-exact through the terminal `echo`.
#[tokio::test]
async fn network_filter_rbac_allow_fixture() -> anyhow::Result<()> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/0073-network-filter-rbac-allow");
    differential::run_fixture(&fixture).await
}
