//! `Scheduler::spawn` — the entry point envoy-bin calls after building the
//! `ClusterManager`. Walks every cluster carrying `health_checks` (12.1 D2
//! validator guarantees 0 or 1 per cluster, HTTP-only); registers the 3
//! `cluster.<n>.health_check.{attempt,success,failure}` counters; spawns
//! one `probe_loop` per (cluster, endpoint) pair (the single-writer-per-endpoint
//! topology that the 12.1 M2 contract requires).
//!
//! `Scheduler::shutdown` cancels every running probe task via a shared
//! `CancellationToken` and awaits the JoinHandles. The envoy-bin runtime
//! wires the scheduler's cancellation to the existing `signal_token` so
//! SIGTERM/SIGINT triggers a clean drain.

use std::sync::Arc;

use envoy_cluster::{ClusterManager, EndpointHealth};
use envoy_config::Bootstrap;
use envoy_stats::{Counter, StatsRegistry};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::error::HealthError;
use crate::probe::{grpc_probe_loop, probe_loop, tcp_probe_loop};

/// 12.2: the active-HC scheduler. Holds the JoinHandles of every spawned
/// probe task. Drop without `shutdown()` is safe — the tasks observe the
/// runtime shutdown — but `shutdown()` is preferred for clean drain.
#[derive(Debug)]
pub struct Scheduler {
    handles: Vec<JoinHandle<()>>,
    cancel: CancellationToken,
}

impl Scheduler {
    /// 12.2: walk the bootstrap clusters with `health_checks` configured,
    /// register the 3 per-cluster counters, and spawn one `probe_loop` per
    /// (cluster, endpoint) pair. Returns a `Scheduler` holding the task
    /// handles. `cancel` is the shared shutdown token — `Scheduler::shutdown`
    /// or the caller cancelling `cancel` (via the envoy-bin signal token)
    /// terminates every loop at its next `tokio::select!` boundary.
    pub fn spawn(
        bootstrap: &Bootstrap,
        cluster_mgr: Arc<ClusterManager>,
        registry: Arc<StatsRegistry>,
        cancel: CancellationToken,
    ) -> Result<Self, HealthError> {
        let mut handles = Vec::new();
        for cfg in bootstrap.all_clusters() {
            // 12.1 D2 / 68 validator guarantees 0 or 1 HC entry, with exactly
            // one of `http_health_check` / `tcp_health_check` present.
            let hc = match cfg.health_checks.first() {
                Some(h) => h,
                None => continue,
            };

            // Register the 3 counters (one set per cluster).
            let attempt = register_counter(&registry, &cfg.name, "attempt")?;
            let success = register_counter(&registry, &cfg.name, "success")?;
            let failure = register_counter(&registry, &cfg.name, "failure")?;

            // Re-parse durations (12.1 D2 validator already accepted them
            // — defense-in-depth, identical-result on the success path).
            let interval_dur = envoy_config::parse_duration(&hc.interval).map_err(|message| {
                HealthError::InvalidDuration {
                    cluster: cfg.name.clone(),
                    field: "interval",
                    message,
                }
            })?;
            let probe_timeout = envoy_config::parse_duration(&hc.timeout).map_err(|message| {
                HealthError::InvalidDuration {
                    cluster: cfg.name.clone(),
                    field: "timeout",
                    message,
                }
            })?;

            // 68: select the checker type (validator guarantees exactly one).
            // Re-decode TCP payloads at spawn (defense-in-depth; the validator
            // already accepted them — the `parse_duration` precedent).
            let tcp_cfg = hc.tcp_health_check.as_ref().map(|tcp| {
                let send = tcp
                    .send
                    .as_ref()
                    .map(|p| p.decode().expect("validator-accepted send payload"));
                let receive: Vec<Vec<u8>> = tcp
                    .receive
                    .iter()
                    .map(|p| p.decode().expect("validator-accepted receive payload"))
                    .collect();
                (send, receive)
            });
            let http_cfg = hc.http_health_check.as_ref().map(|http| {
                (
                    http.host.clone().unwrap_or_else(|| cfg.name.clone()),
                    http.path.clone(),
                    http.expected_statuses.clone(),
                )
            });
            let grpc_cfg = hc.grpc_health_check.as_ref().map(|g| {
                let authority = if g.authority.is_empty() {
                    cfg.name.clone()
                } else {
                    g.authority.clone()
                };
                (authority, g.service_name.clone())
            });

            // Walk the resolved (addr, EndpointHealth) pairs from the
            // ClusterManager (the 12.2 `health_probe_targets` accessor).
            let handle = match cluster_mgr.get(&cfg.name) {
                Some(h) => h,
                None => continue, // defense-in-depth: validator+manager align
            };
            let targets = handle
                .health_probe_targets()
                .expect("HC-configured cluster has health_probe_targets");
            for (addr, endpoint_health) in targets {
                let cancel = cancel.clone();
                let a = Arc::clone(&attempt);
                let s = Arc::clone(&success);
                let f = Arc::clone(&failure);
                let eh: Arc<EndpointHealth> = endpoint_health;
                let h = match (&http_cfg, &tcp_cfg, &grpc_cfg) {
                    (Some((host, path, exp)), None, None) => {
                        let (host, path, exp) = (host.clone(), path.clone(), exp.clone());
                        tokio::spawn(async move {
                            probe_loop(
                                addr,
                                host,
                                path,
                                probe_timeout,
                                interval_dur,
                                exp,
                                eh,
                                a,
                                s,
                                f,
                                cancel,
                            )
                            .await;
                        })
                    }
                    (None, Some((send, receive)), None) => {
                        let (send, receive) = (send.clone(), receive.clone());
                        tokio::spawn(async move {
                            tcp_probe_loop(
                                addr,
                                send,
                                receive,
                                probe_timeout,
                                interval_dur,
                                eh,
                                a,
                                s,
                                f,
                                cancel,
                            )
                            .await;
                        })
                    }
                    (None, None, Some((authority, service))) => {
                        let (authority, service) = (authority.clone(), service.clone());
                        tokio::spawn(async move {
                            grpc_probe_loop(
                                addr,
                                authority,
                                service,
                                probe_timeout,
                                interval_dur,
                                eh,
                                a,
                                s,
                                f,
                                cancel,
                            )
                            .await;
                        })
                    }
                    // Validator guarantees exactly one checker present.
                    _ => unreachable!("validator guarantees exactly one health checker"),
                };
                handles.push(h);
            }
        }
        Ok(Scheduler { handles, cancel })
    }

    /// 12.2: cancel every probe task and await their JoinHandles. Returns
    /// once every task has exited at its next `tokio::select!` boundary.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        for h in self.handles {
            let _ = h.await;
        }
    }

    /// 12.2: test helper — count of spawned probe tasks.
    pub fn task_count(&self) -> usize {
        self.handles.len()
    }
}

fn register_counter(
    registry: &StatsRegistry,
    cluster: &str,
    kind: &'static str,
) -> Result<Arc<Counter>, HealthError> {
    registry
        .register_counter(&format!("cluster.{cluster}.health_check.{kind}"))
        .map_err(|e| HealthError::StatsRegistration {
            cluster: cluster.to_string(),
            message: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_cluster::from_bootstrap;
    use envoy_config::parse_bootstrap;

    const HC_BOOTSTRAP: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check:
            path: /healthz
            expected_statuses:
              - { start: 200, end: 201 }
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 60001 } }
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 60002 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;

    const NO_HC_BOOTSTRAP: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: plain
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: plain
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 60003 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;

    const GRPC_HC_BOOTSTRAP: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: grpc_hc_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      typed_extension_protocol_options:
        envoy.extensions.upstreams.http.v3.HttpProtocolOptions:
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          grpc_health_check:
            service_name: "envoy.service.health.v3.HealthCheck"
      load_assignment:
        cluster_name: grpc_hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 60021 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;

    const TCP_HC_BOOTSTRAP: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: tcp_hc_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 2
          tcp_health_check: { receive: [ { text: "50494e47" } ] }
      load_assignment:
        cluster_name: tcp_hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 60011 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;

    #[tokio::test]
    async fn spawns_tcp_probe_task_and_registers_counters() {
        let bootstrap = parse_bootstrap(TCP_HC_BOOTSTRAP).expect("parse");
        let registry = Arc::new(StatsRegistry::new());
        let cluster_mgr = Arc::new(
            from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("build"),
        );
        let cancel = CancellationToken::new();
        let scheduler = Scheduler::spawn(&bootstrap, cluster_mgr, registry.clone(), cancel.clone())
            .expect("scheduler");
        assert_eq!(
            scheduler.task_count(),
            1,
            "one TCP probe task for the single endpoint"
        );
        let snapshot = registry.snapshot();
        for kind in ["attempt", "success", "failure"] {
            let name = format!("cluster.tcp_hc_backend.health_check.{kind}");
            assert!(
                snapshot.iter().any(|(n, _)| n == &name),
                "registry must contain {name}"
            );
        }
        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn spawns_grpc_probe_task_and_registers_counters() {
        let bootstrap = parse_bootstrap(GRPC_HC_BOOTSTRAP).expect("parse");
        let registry = Arc::new(StatsRegistry::new());
        let cluster_mgr = Arc::new(
            from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("build"),
        );
        let cancel = CancellationToken::new();
        let scheduler = Scheduler::spawn(&bootstrap, cluster_mgr, registry.clone(), cancel.clone())
            .expect("scheduler");
        assert_eq!(
            scheduler.task_count(),
            1,
            "one gRPC probe task for the single endpoint"
        );
        let snapshot = registry.snapshot();
        for kind in ["attempt", "success", "failure"] {
            let name = format!("cluster.grpc_hc_backend.health_check.{kind}");
            assert!(
                snapshot.iter().any(|(n, _)| n == &name),
                "registry must contain {name}"
            );
        }
        // The endpoint is a dead port (nothing listening on 60021), so the
        // probe attempts and fails — assert the `attempt` counter ticks.
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let snapshot = registry.snapshot();
        let attempt = snapshot
            .iter()
            .find_map(|(n, h)| {
                if n == "cluster.grpc_hc_backend.health_check.attempt" {
                    match h {
                        envoy_stats::StatHandle::Counter(c) => Some(c.value()),
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .expect("attempt counter present");
        assert!(
            attempt >= 1,
            "attempt counter must have ticked at least once, got {attempt}"
        );
        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn spawns_one_task_per_hc_endpoint() {
        let bootstrap = parse_bootstrap(HC_BOOTSTRAP).expect("parse");
        let registry = Arc::new(StatsRegistry::new());
        let cluster_mgr = Arc::new(
            from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("build"),
        );
        let cancel = CancellationToken::new();
        let scheduler =
            Scheduler::spawn(&bootstrap, cluster_mgr, registry, cancel.clone()).expect("scheduler");
        assert_eq!(
            scheduler.task_count(),
            2,
            "one task per (cluster, endpoint)"
        );
        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn spawns_zero_tasks_when_no_hc_configured() {
        let bootstrap = parse_bootstrap(NO_HC_BOOTSTRAP).expect("parse");
        let registry = Arc::new(StatsRegistry::new());
        let cluster_mgr = Arc::new(
            from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("build"),
        );
        let cancel = CancellationToken::new();
        let scheduler =
            Scheduler::spawn(&bootstrap, cluster_mgr, registry, cancel.clone()).expect("scheduler");
        assert_eq!(scheduler.task_count(), 0, "no probe task for no-HC cluster");
        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_terminates_all_tasks() {
        let bootstrap = parse_bootstrap(HC_BOOTSTRAP).expect("parse");
        let registry = Arc::new(StatsRegistry::new());
        let cluster_mgr = Arc::new(
            from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("build"),
        );
        let cancel = CancellationToken::new();
        let scheduler =
            Scheduler::spawn(&bootstrap, cluster_mgr, registry, cancel.clone()).expect("scheduler");
        // Tasks loop on a 1s interval; shutdown via cancel must return promptly
        // (not wait for the next tick — `tokio::select!` exits cancel branch).
        let dur =
            tokio::time::timeout(std::time::Duration::from_secs(3), scheduler.shutdown()).await;
        assert!(dur.is_ok(), "shutdown returned within 3s");
    }

    #[tokio::test]
    async fn registers_three_counters_per_hc_cluster() {
        let bootstrap = parse_bootstrap(HC_BOOTSTRAP).expect("parse");
        let registry = Arc::new(StatsRegistry::new());
        let cluster_mgr = Arc::new(
            from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("build"),
        );
        let cancel = CancellationToken::new();
        let _scheduler =
            Scheduler::spawn(&bootstrap, cluster_mgr, registry.clone(), cancel).expect("scheduler");
        let snapshot = registry.snapshot();
        for kind in ["attempt", "success", "failure"] {
            let name = format!("cluster.hc_backend.health_check.{kind}");
            assert!(
                snapshot.iter().any(|(n, _)| n == &name),
                "registry must contain {name}; snapshot = {snapshot:?}"
            );
        }
    }

    #[tokio::test]
    async fn registers_no_counters_when_no_hc_configured() {
        let bootstrap = parse_bootstrap(NO_HC_BOOTSTRAP).expect("parse");
        let registry = Arc::new(StatsRegistry::new());
        let cluster_mgr = Arc::new(
            from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("build"),
        );
        let cancel = CancellationToken::new();
        let _scheduler =
            Scheduler::spawn(&bootstrap, cluster_mgr, registry.clone(), cancel).expect("scheduler");
        let snapshot = registry.snapshot();
        for kind in ["attempt", "success", "failure"] {
            let name = format!("cluster.plain.health_check.{kind}");
            assert!(
                !snapshot.iter().any(|(n, _)| n == &name),
                "registry must NOT contain {name} (no HC configured)"
            );
        }
    }
}
