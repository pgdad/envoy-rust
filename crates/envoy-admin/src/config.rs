//! `AdminConfig` — parsed from `envoy_config::Admin` block.

use crate::error::AdminError;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AdminConfig {
    /// Bind address; sourced from `Bootstrap.admin.address.socket_address`.
    pub address: SocketAddr,

    /// Optional admin-side access log path; parsed from
    /// `Bootstrap.admin.access_log_path` per the ADR-0026 parse-and-ignore
    /// pattern. envoy-rust does NOT inspect or honor this field in 06.1;
    /// admin-side access logging defers indefinitely. Storing it allows
    /// fixtures with upstream Envoy admin configs to round-trip cleanly.
    pub access_log_path: Option<PathBuf>,
}

impl AdminConfig {
    pub fn from_envoy_config(admin: &envoy_config::Admin) -> Result<Self, AdminError> {
        let sock = &admin.address.socket_address;
        let raw = format!("{}:{}", sock.address, sock.port_value);
        let address = raw
            .parse::<SocketAddr>()
            .map_err(|source| AdminError::BadAddress {
                raw: raw.clone(),
                source,
            })?;
        let access_log_path = admin.access_log_path.clone().map(PathBuf::from);
        Ok(Self {
            address,
            access_log_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{Address, Admin, SocketAddress};

    fn admin_with(addr: &str, port: u16, log: Option<&str>) -> Admin {
        Admin {
            address: Address {
                socket_address: SocketAddress {
                    address: addr.to_string(),
                    port_value: port,
                },
            },
            access_log_path: log.map(|s| s.to_string()),
        }
    }

    #[test]
    fn from_envoy_config_round_trips_address() {
        let a = admin_with("127.0.0.1", 9901, None);
        let cfg = AdminConfig::from_envoy_config(&a).unwrap();
        assert_eq!(cfg.address, "127.0.0.1:9901".parse::<SocketAddr>().unwrap());
        assert_eq!(cfg.access_log_path, None);
    }

    #[test]
    fn from_envoy_config_carries_access_log_path() {
        let a = admin_with("127.0.0.1", 9901, Some("/tmp/admin.log"));
        let cfg = AdminConfig::from_envoy_config(&a).unwrap();
        assert_eq!(cfg.access_log_path, Some(PathBuf::from("/tmp/admin.log")));
    }

    #[test]
    fn from_envoy_config_rejects_unparseable_address() {
        let a = admin_with("not-a-host", 9901, None);
        let err = AdminConfig::from_envoy_config(&a).unwrap_err();
        match err {
            AdminError::BadAddress { raw, .. } => {
                assert_eq!(raw, "not-a-host:9901");
            }
            other => panic!("expected BadAddress, got {other:?}"),
        }
    }
}
