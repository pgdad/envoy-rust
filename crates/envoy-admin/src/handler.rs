//! `AdminHandler` + `serve` free fn — Task 8 ships the real surface.

use crate::config::AdminConfig;
use crate::error::AdminError;
use envoy_stats::StatsRegistry;
use std::sync::Arc;

pub struct AdminHandler {
    _config: Arc<AdminConfig>,
    _registry: Arc<StatsRegistry>,
}

impl AdminHandler {
    pub fn new(config: Arc<AdminConfig>, registry: Arc<StatsRegistry>) -> Self {
        Self {
            _config: config,
            _registry: registry,
        }
    }
}

/// Placeholder; Task 8 ships the real implementation.
pub async fn serve(
    _listener: tokio::net::TcpListener,
    _handler: Arc<AdminHandler>,
    _shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), AdminError> {
    unimplemented!("Task 8 ships envoy_admin::serve")
}
