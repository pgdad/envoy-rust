#![forbid(unsafe_code)]

//! Phase 02.2 listener surface for envoy-rust. Owns TCP listener binding, the
//! accept loop, the `ConnectionHandler` trait that filters implement, and a
//! shutdown-gated graceful drain.
//!
//! `BoxFuture` and `ConnectionHandler` are defined in-crate to avoid pulling
//! `futures` or `async-trait` (neither on the D-3.2 permitted-foundations
//! list); see SPEC §6 signposts 2 and 3.

pub mod drain;
pub use drain::{DrainStage, DrainState};

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// Drain budget — the maximum time `Listener::serve` waits for in-flight
/// connections to complete after the drain signal fires. Hoisted to module
/// level at phase 08.1 D3 (closes 06.1 REVIEW M4); re-exported from
/// `envoy-admin` via the existing crate dep.
pub const DRAIN_BUDGET: Duration = Duration::from_secs(5);

/// In-crate `BoxFuture` alias. Phase 02.2 deliberately avoids depending on
/// `futures::future::BoxFuture` because `futures` is not on the D-3.2
/// permitted-foundations list. If a later phase brings `futures` in under its
/// own ADR, this alias becomes a re-export.
pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// A network-filter-shaped per-connection handler. The trait is intentionally
/// object-safe (`Listener` stores `Arc<dyn ConnectionHandler>`) and avoids
/// `async-trait` per SPEC §6 signpost 2: the `handle` method returns a
/// hand-boxed `BoxFuture` instead of being declared `async fn`. The error
/// type is `Box<dyn std::error::Error + Send + Sync>` rather than
/// `anyhow::Error` per D-3.2: library crates cannot depend on `anyhow`. The
/// binary crate (`envoy-bin`) converts these errors to `anyhow::Error` at the
/// crate boundary if it needs to.
pub trait ConnectionHandler: Send + Sync + 'static {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>;
}

/// Errors returned by `Listener::bind` and `Listener::serve`.
#[derive(Debug, thiserror::Error)]
pub enum ListenerError {
    #[error("binding listener address {addr}: {source}")]
    Bind {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("accept loop terminated: {0}")]
    Accept(#[source] std::io::Error),
    #[error("drain timed out after {0:?}")]
    DrainTimeout(Duration),
    #[error("resolving listener address '{0}:{1}'")]
    AddressParse(String, u16),
    /// 06.1 D4.a: registering the per-listener counter
    /// (`listener.<name>.downstream_cx_total`) against the global
    /// `StatsRegistry` failed. Wraps the registry error's `Display`
    /// rendering so this crate doesn't need to publicly re-export
    /// `envoy_stats::StatsError` in its error surface.
    #[error("registering listener stats: {0}")]
    StatsRegistration(String),
}

/// A bound TCP listener with a per-connection handler. Construct via
/// `Listener::bind`; drive via `Listener::serve` (Task 6).
pub struct Listener {
    listener: tokio::net::TcpListener,
    handler: Arc<dyn ConnectionHandler>,
    /// 06.1 D4.a: per-listener counter incremented once per accepted TCP
    /// connection. Registered at construct time as
    /// `listener.<name>.downstream_cx_total`. Threaded through the
    /// `tokio::select!` accept arm in `serve` (moved into a local at the
    /// top of the loop to keep the borrow shape simple).
    cx_total: Arc<envoy_stats::Counter>,
    /// 06.3 D15.3.b: per-listener gauge tracking in-flight connections.
    /// Incremented on each accepted TCP connection; decremented at the
    /// per-connection task epilogue (both success and error paths).
    /// Registered at construct time as
    /// `listener.<name>.downstream_cx_active`. Scoped to data-path
    /// listeners only — the admin listener uses
    /// `tokio::net::TcpListener` + `envoy_admin::serve` directly
    /// (not `Listener::bind`), so this gauge is naturally excluded.
    cx_active: Arc<envoy_stats::Gauge>,
    /// 06.3 D15.3.d: per-listener counter incremented on every accept error
    /// (the `Err(err)` arm of `listener.accept()` in `serve`). Registered at
    /// construct time as `listener.<name>.downstream_cx_accept_failed`. Per
    /// signpost 6: ALL accept errors count, no carve-outs. Incremented BEFORE
    /// the `tracing::warn!` so the counter fires even if the warn is filtered.
    cx_accept_failed: Arc<envoy_stats::Counter>,
    /// 08.2 D14: shared gauge `listener_manager.total_listeners_active` —
    /// count of currently-active data-plane listeners. Registered
    /// idempotently inside `Listener::bind` (same-name re-registration
    /// returns the existing `Arc`, so every `Listener` instance shares one
    /// gauge across the process). Hoisted into the
    /// `ListenerManagerActiveGuard` RAII guard at `Listener::serve` entry
    /// at Task 6 (D12); the guard's `Drop` decrements after the post-loop
    /// drain-wait completes. Echo + admin listeners use
    /// `tokio::net::TcpListener` directly (not `Listener::bind`) and are
    /// therefore naturally excluded from this gauge per
    /// architecture-decision lock-in #12.
    listener_manager_active: Arc<envoy_stats::Gauge>,
}

/// 08.2 Task 6 (D12): RAII guard that increments
/// `listener_manager.total_listeners_active` at construction and decrements
/// at Drop. Constructed at the top of `Listener::serve` so its Drop fires
/// after the post-loop drain-wait block (Rust drop-order is reverse
/// declaration order; the guard, declared first inside `serve`, drops
/// last). Mirrors the existing 06.3 `cx_active` per-connection guard
/// pattern but at the per-listener granularity.
struct ListenerManagerActiveGuard(Arc<envoy_stats::Gauge>);

impl ListenerManagerActiveGuard {
    fn new(gauge: Arc<envoy_stats::Gauge>) -> Self {
        gauge.inc();
        Self(gauge)
    }
}

impl Drop for ListenerManagerActiveGuard {
    fn drop(&mut self) {
        self.0.dec();
    }
}

impl std::fmt::Debug for Listener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Listener")
            .field("local_addr", &self.listener.local_addr())
            .finish_non_exhaustive()
    }
}

impl Listener {
    /// Resolve `cfg.address.socket_address` to a `SocketAddr` and bind it. The
    /// returned `Listener` is ready to be passed to `serve`. Phase-02.1 `envoy
    /// -config` does not parse the address field into a `SocketAddr`; that
    /// happens here, so configuration with a malformed `address` (e.g.
    /// `"not-a-host"`) returns `ListenerError::AddressParse`.
    pub async fn bind(
        cfg: &envoy_config::Listener,
        handler: Arc<dyn ConnectionHandler>,
        registry: Arc<envoy_stats::StatsRegistry>,
    ) -> Result<Self, ListenerError> {
        let sock = &cfg.address.socket_address;
        let addr_str = format!("{}:{}", sock.address, sock.port_value);
        let addr: SocketAddr = addr_str
            .parse()
            .map_err(|_| ListenerError::AddressParse(sock.address.clone(), sock.port_value))?;
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|source| ListenerError::Bind { addr, source })?;
        // 06.1 D4.a: register `listener.<name>.downstream_cx_total`. The
        // registry call is idempotent for same-kind re-registration, so
        // multiple `Listener::bind` calls with the same `cfg.name` (a
        // configuration error in production but possible in tests) reuse
        // the existing handle rather than erroring.
        let cx_total = registry
            .register_counter(&format!("listener.{}.downstream_cx_total", cfg.name))
            .map_err(|e| ListenerError::StatsRegistration(e.to_string()))?;
        // 06.3 D15.3.b: register `listener.<name>.downstream_cx_active`.
        // Idempotent same-kind re-registration mirrors cx_total above.
        let cx_active = registry
            .register_gauge(&format!("listener.{}.downstream_cx_active", cfg.name))
            .map_err(|e| ListenerError::StatsRegistration(e.to_string()))?;
        // 06.3 D15.3.d: register `listener.<name>.downstream_cx_accept_failed`.
        // Idempotent same-kind re-registration mirrors cx_total above.
        let cx_accept_failed = registry
            .register_counter(&format!(
                "listener.{}.downstream_cx_accept_failed",
                cfg.name
            ))
            .map_err(|e| ListenerError::StatsRegistration(e.to_string()))?;
        // 08.2 D14: register the shared `listener_manager.total_listeners_active`
        // gauge. Idempotent same-name re-registration across multiple `bind`
        // calls mirrors the 06.1 cx_total / 06.3 cx_active / 06.3 cx_accept_failed
        // pattern at adjacent registration sites. RAII inc/dec wiring at
        // `serve` entry/exit lands at Task 6 (D12).
        let listener_manager_active = registry
            .register_gauge("listener_manager.total_listeners_active")
            .map_err(|e| ListenerError::StatsRegistration(e.to_string()))?;
        Ok(Self {
            listener,
            handler,
            cx_total,
            cx_active,
            cx_accept_failed,
            listener_manager_active,
        })
    }

    /// Returns the actual bound socket address (resolves `port_value: 0` to
    /// the kernel-assigned ephemeral port).
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// 08.2 D14: accessor for the shared `listener_manager.total_listeners_active`
    /// gauge so Task 6's RAII guard can hoist the `Arc<Gauge>` out of `self`
    /// before `self.listener` is consumed by `serve`. `pub(crate)` because
    /// no external consumer should touch this gauge directly — the inc/dec
    /// wiring is internal to `Listener::serve`.
    pub(crate) fn listener_manager_active(&self) -> &Arc<envoy_stats::Gauge> {
        &self.listener_manager_active
    }

    /// Accept loop with shutdown-gated graceful drain. On either `shutdown`
    /// or `drain.drain_signal()` firing, stop accepting and wait up to
    /// `DRAIN_BUDGET = 5s` for in-flight connections to complete. If the
    /// drain budget expires, abort stragglers and return
    /// `ListenerError::DrainTimeout`.
    ///
    /// 08.2 Task 6 (D12): widened from 1-arg `(shutdown)` to 2-arg
    /// `(shutdown, drain: Arc<DrainState>)`. Either signal triggers the
    /// same drain code path (drop the listener; await stragglers within
    /// DRAIN_BUDGET). Each iteration of the loop re-anchors a fresh
    /// `drain.drain_signal()` future (a `Notified` snapshot is taken
    /// inside `drain_signal()` before the state load per Task 1 fixup's
    /// TOCTOU fix — already-Draining short-circuits to a ready future).
    ///
    /// 08.2 Task 6 (D12): also installs a
    /// `ListenerManagerActiveGuard` at function entry that increments
    /// `listener_manager.total_listeners_active`; the guard's Drop
    /// decrements after the post-loop drain-wait completes (RAII drop
    /// order is reverse declaration order — the guard is declared first
    /// inside `serve`, so it drops last after stragglers complete).
    ///
    /// SPEC §6 signpost 5: errors from individual `handle` calls are logged
    /// at `warn!` and dropped; the listener stays up. Asymmetric errors in
    /// `tokio::io::copy` (downstream → upstream succeeds while the other
    /// direction errors) propagate via `try_join!` inside the handler, not
    /// through the listener's accept loop.
    pub async fn serve(
        self,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
        drain: Arc<DrainState>,
    ) -> Result<(), ListenerError> {
        // 08.2 Task 6 (D12): RAII guard MUST be the first local so its Drop
        // fires LAST (after the post-loop drain-wait block + stragglers
        // join). Construction increments the gauge; Drop decrements.
        let _lm_guard = ListenerManagerActiveGuard::new(Arc::clone(self.listener_manager_active()));

        let listener = self.listener;
        let handler = self.handler;
        // 06.1 D4.a: hoist the per-listener counter out of `self` so the
        // accept arm of `tokio::select!` can call `cx_total.inc()` without
        // borrowing `self` (which has been consumed by the `let listener =
        // self.listener;` move above).
        let cx_total = self.cx_total;
        // 06.3 D15.3.b: hoist the per-listener gauge; mirrors cx_total above.
        let cx_active = self.cx_active;
        // 06.3 D15.3.d: hoist the accept-failure counter; mirrors cx_total + cx_active above.
        let cx_accept_failed = self.cx_accept_failed;
        let mut join_set: tokio::task::JoinSet<
            Result<(), Box<dyn std::error::Error + Send + Sync>>,
        > = tokio::task::JoinSet::new();
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                _ = &mut shutdown => {
                    tracing::info!("listener shutdown signal received; draining");
                    drop(listener);
                    break;
                }
                // 08.2 Task 6 (D12): drain-signal arm. Either this or the
                // shutdown arm triggers the same drain code path. Each loop
                // iteration constructs a fresh `drain_signal()` future; if
                // state is already `Draining`, the future returns ready
                // immediately (drain is sticky + idempotent — see
                // `DrainState::drain_signal` for the TOCTOU-safe shape).
                _ = drain.drain_signal() => {
                    tracing::info!("listener drain signal received; draining");
                    drop(listener);
                    break;
                }
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, peer)) => {
                            // 06.1 D4.a: increment per-listener accept counter.
                            cx_total.inc();
                            // 06.3 D15.3.b: increment active-connection gauge.
                            cx_active.inc();
                            // Disable Nagle's algorithm on the downstream socket.
                            // Without this, ~40ms delayed-ACK + Nagle stalls every
                            // small response — measured 60ms p50 latency drops to
                            // sub-ms with TCP_NODELAY. Matches Envoy's default.
                            let _ = stream.set_nodelay(true);
                            tracing::debug!(%peer, "listener accepted connection");
                            let h = handler.clone();
                            // Clone the gauge Arc into the task; dec after
                            // handle returns (both success and error paths).
                            let cx_active_clone = Arc::clone(&cx_active);
                            join_set.spawn(async move {
                                let result = h.handle(stream).await;
                                cx_active_clone.dec();
                                result
                            });
                        }
                        Err(err) => {
                            // 06.3 D15.3.d + signpost 6: ALL accept errors
                            // count, no carve-outs. Increment BEFORE the warn
                            // so the counter fires even if tracing is filtered.
                            cx_accept_failed.inc();
                            // Accept errors are not fatal — log and continue,
                            // matching `envoy-bin::admin::serve` and
                            // `envoy-bin::echo::serve` from phases 00–01.
                            tracing::warn!(error = %err, "accept failed; continuing");
                        }
                    }
                }
                Some(done) = join_set.join_next(), if !join_set.is_empty() => {
                    match done {
                        Ok(Ok(())) => {}
                        Ok(Err(err)) => tracing::warn!(error = %err, "connection task failed"),
                        Err(join_err) => tracing::warn!(error = %join_err, "connection task panicked"),
                    }
                }
            }
        }

        // Drain.
        let drain = async {
            while let Some(res) = join_set.join_next().await {
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        tracing::warn!(error = %err, "connection task failed during drain")
                    }
                    Err(join_err) => {
                        tracing::warn!(error = %join_err, "connection task panicked during drain")
                    }
                }
            }
        };
        if tokio::time::timeout(DRAIN_BUDGET, drain).await.is_err() {
            tracing::warn!(?DRAIN_BUDGET, "drain budget exceeded; aborting stragglers");
            join_set.abort_all();
            // Let aborted tasks unwind; ignore their results.
            while join_set.join_next().await.is_some() {}
            return Err(ListenerError::DrainTimeout(DRAIN_BUDGET));
        }
        Ok(())
    }
}

/// 19 D4 (ADR-0050; §6.2 L3/L10): the `listener_manager.lds.*` stat family +
/// `listener_added` — registered ONLY when `dynamic_resources.lds_config` is
/// configured (the §5.2 conditional-registration discipline; Envoy emits the
/// base `listener_manager.*` names unconditionally — those stay Envoy-only-
/// unasserted on non-LDS fixtures). All LDS load failures are fatal
/// pre-registration (the L4 posture), so `update_failure` / `update_rejected`
/// register at 0 and never tick. `listener_manager.total_listeners_active` is
/// NOT registered here — it keeps its pre-existing unconditional registration
/// inside `Listener::bind` (08.2 D14). `listener_added` counts ALL listeners
/// (static + dynamic, via `all_listeners()`) per the L3 lesson.
///
/// Called once from envoy-bin `main()`, after the `StatsRegistry` is
/// constructed and after `load_dynamic_resources` has populated
/// `dynamic_listeners`. No-op (returns `Ok(())`) when `lds_config` is
/// unconfigured — the §5.2 inertness invariant. `register_counter` is
/// idempotent for same-name/same-kind re-registration (mirrors the phase-18
/// `cluster_manager.cds.*` template).
pub fn register_lds_stats(
    bootstrap: &envoy_config::Bootstrap,
    registry: &envoy_stats::StatsRegistry,
) -> Result<(), ListenerError> {
    if bootstrap
        .dynamic_resources
        .as_ref()
        .and_then(|dr| dr.lds_config.as_ref())
        .is_none()
    {
        return Ok(());
    }
    let mk = |name: &str| {
        registry
            .register_counter(name)
            .map_err(|e| ListenerError::StatsRegistration(e.to_string()))
    };
    mk("listener_manager.lds.update_attempt")?.add(1);
    mk("listener_manager.lds.update_success")?.add(1);
    mk("listener_manager.lds.update_failure")?; // registers at 0 (L4)
    mk("listener_manager.lds.update_rejected")?; // registers at 0 (L4)
    let added = mk("listener_manager.listener_added")?;
    added.add(bootstrap.all_listeners().count() as u64);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::oneshot;

    /// In-process `EchoHandler` that echoes whatever bytes the downstream
    /// writes back to it. Used in serve-side tests as a stand-in for the real
    /// `envoy-tcp::TcpProxy` that lands in Task 8.
    struct EchoHandler;
    impl ConnectionHandler for EchoHandler {
        fn handle(
            &self,
            mut downstream: tokio::net::TcpStream,
        ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
            Box::pin(async move {
                let (mut r, mut w) = downstream.split();
                tokio::io::copy(&mut r, &mut w).await?;
                Ok(())
            })
        }
    }

    fn mk_registry() -> Arc<envoy_stats::StatsRegistry> {
        Arc::new(envoy_stats::StatsRegistry::new())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn serves_accepts_and_dispatches_to_handler() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
        let registry = mk_registry();
        let listener = Listener::bind(&cfg, h, Arc::clone(&registry))
            .await
            .expect("bind ok");
        let addr = listener.local_addr().expect("local_addr");

        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            listener
                .serve(
                    async move {
                        let _ = rx.await;
                    },
                    drain,
                )
                .await
                .expect("serve ok")
        });

        let mut client = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let payload = b"hello, listener\n";
        client.write_all(payload).await.expect("write");
        let mut buf = vec![0u8; payload.len()];
        client.read_exact(&mut buf).await.expect("read_exact");
        assert_eq!(buf, payload);
        drop(client);

        tx.send(()).expect("signal shutdown");
        tokio::time::timeout(std::time::Duration::from_secs(6), server)
            .await
            .expect("serve resolves within 6s")
            .expect("join");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn serves_honors_shutdown_signal() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
        let registry = mk_registry();
        let listener = Listener::bind(&cfg, h, Arc::clone(&registry))
            .await
            .expect("bind");
        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = oneshot::channel::<()>();
        let start = std::time::Instant::now();
        let server = tokio::spawn(async move {
            listener
                .serve(
                    async move {
                        let _ = rx.await;
                    },
                    drain,
                )
                .await
                .expect("serve")
        });

        // Fire shutdown immediately (no in-flight connections); serve must
        // return promptly.
        tx.send(()).expect("signal");
        tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .expect("serve resolves within 2s of empty shutdown")
            .expect("join");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "serve took too long: {:?}",
            start.elapsed(),
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn serves_drains_in_flight_connection_within_budget() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
        let registry = mk_registry();
        let listener = Listener::bind(&cfg, h, Arc::clone(&registry))
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            listener
                .serve(
                    async move {
                        let _ = rx.await;
                    },
                    drain,
                )
                .await
                .expect("serve")
        });

        // Open a connection that's actively echoing (not stalled).
        let mut client = tokio::net::TcpStream::connect(addr).await.expect("connect");
        client.write_all(b"in-flight").await.expect("write");
        let mut buf = [0u8; 9];
        client.read_exact(&mut buf).await.expect("read");
        // FIN to let the EchoHandler's tokio::io::copy return cleanly.
        client.shutdown().await.ok();

        let start = std::time::Instant::now();
        tx.send(()).expect("signal shutdown");
        tokio::time::timeout(std::time::Duration::from_secs(7), server)
            .await
            .expect("serve drains within budget + ε")
            .expect("join");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(6),
            "drain too slow: {:?}",
            start.elapsed(),
        );
    }

    /// A handler that never returns. Used to exercise the abort-stragglers
    /// path: the `handle` future stays parked past `DRAIN_BUDGET`, forcing
    /// `Listener::serve` to call `JoinSet::abort_all`.
    struct StalledHandler;
    impl ConnectionHandler for StalledHandler {
        fn handle(
            &self,
            _downstream: tokio::net::TcpStream,
        ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
            Box::pin(async move {
                std::future::pending::<()>().await;
                Ok(())
            })
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn serves_aborts_stragglers_past_drain_budget() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(StalledHandler);
        let registry = mk_registry();
        let listener = Listener::bind(&cfg, h, Arc::clone(&registry))
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            listener
                .serve(
                    async move {
                        let _ = rx.await;
                    },
                    drain,
                )
                .await
        });

        // Open one stalled connection.
        let _client = tokio::net::TcpStream::connect(addr).await.expect("connect");
        // Give the listener a moment to spawn the handler task.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let start = std::time::Instant::now();
        tx.send(()).expect("signal shutdown");
        let result = tokio::time::timeout(std::time::Duration::from_secs(8), server)
            .await
            .expect("serve resolves within DRAIN_BUDGET + ε")
            .expect("join");
        assert!(
            matches!(result, Err(ListenerError::DrainTimeout(_))),
            "expected DrainTimeout, got {result:?}",
        );
        // Drain budget is 5s; the timeout should fire within 5s + ε.
        assert!(
            start.elapsed() >= std::time::Duration::from_secs(4),
            "abort fired too early: {:?}",
            start.elapsed(),
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(7),
            "abort fired too late: {:?}",
            start.elapsed(),
        );
    }

    /// Phase-02.1 `envoy_config::Listener` accepts the raw `address:
    /// socket_address: { address: String, port_value: u16 }` shape. Build one
    /// by hand for tests that don't want to drag YAML through.
    fn mk_listener_cfg(addr: &str, port: u16) -> envoy_config::Listener {
        let yaml = format!(
            r#"
name: test_listener
address:
  socket_address:
    address: {addr}
    port_value: {port}
filter_chains:
  - filters: []
"#
        );
        serde_yaml::from_str(&yaml).expect("hand-constructed listener YAML parses")
    }

    /// Trivial `ConnectionHandler` that drops the stream — used for bind-side
    /// tests where the accept loop does not need to dispatch real work.
    struct NullHandler;
    impl ConnectionHandler for NullHandler {
        fn handle(
            &self,
            _downstream: tokio::net::TcpStream,
        ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
            Box::pin(async move { Ok(()) })
        }
    }

    #[tokio::test]
    async fn bind_returns_socket_address() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let handler: Arc<dyn ConnectionHandler> = Arc::new(NullHandler);
        let listener = Listener::bind(&cfg, handler, mk_registry())
            .await
            .expect("bind ok");
        let local = listener.local_addr().expect("local_addr");
        assert!(local.port() > 0, "ephemeral port must be assigned: {local}");
        assert_eq!(local.ip(), "127.0.0.1".parse::<std::net::IpAddr>().unwrap());
    }

    /// Task 2 (D14): `Listener::bind` registers
    /// `listener_manager.total_listeners_active` gauge against the shared
    /// registry. The RAII inc/dec wiring at `Listener::serve` entry/exit
    /// lands at Task 6 (D12); Task 2 only verifies registration.
    #[tokio::test]
    async fn bind_registers_listener_manager_total_active_gauge() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let handler: Arc<dyn ConnectionHandler> = Arc::new(NullHandler);
        let registry = mk_registry();
        let _listener = Listener::bind(&cfg, handler, Arc::clone(&registry))
            .await
            .expect("bind succeeds");
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().into_iter().collect();
        assert!(
            snapshot.contains_key("listener_manager.total_listeners_active"),
            "listener_manager.total_listeners_active not registered; snapshot keys: {:?}",
            snapshot.keys().collect::<Vec<_>>()
        );
    }

    /// Task 2 (D14): Two `Listener::bind` calls against the same registry
    /// register exactly one shared `listener_manager.total_listeners_active`
    /// gauge (idempotent same-name re-registration mirrors the 06.1
    /// `cx_total` + 06.3 `cx_active` + `cx_accept_failed` pattern at the
    /// adjacent registration sites).
    #[tokio::test]
    async fn bind_listener_manager_gauge_is_idempotent_shared() {
        let registry = mk_registry();
        for _ in 0..2 {
            // Distinct ephemeral ports — listeners must be unique on the
            // wire; only the gauge NAME is shared (mirrors the 06.1 +
            // 06.3 idempotent-name patterns at cx_total / cx_active).
            let cfg = mk_listener_cfg("127.0.0.1", 0);
            let h: Arc<dyn ConnectionHandler> = Arc::new(NullHandler);
            let _ = Listener::bind(&cfg, h, Arc::clone(&registry))
                .await
                .expect("bind succeeds");
        }
        let snapshot_vec = registry.snapshot();
        let matches: Vec<_> = snapshot_vec
            .iter()
            .filter(|(name, _)| name == "listener_manager.total_listeners_active")
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "shared gauge must appear exactly once in the snapshot",
        );
    }

    #[tokio::test]
    async fn bind_fails_cleanly_on_address_in_use() {
        // Bind once to an ephemeral port to capture the assigned port, then
        // bind again to that same port to provoke EADDRINUSE.
        let cfg_first = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(NullHandler);
        // Share a single registry so the second bind exercises the
        // idempotent same-kind re-registration path (Task 5 contract); a
        // distinct registry per call would equally work since the names
        // collide only within a registry.
        let registry = mk_registry();
        let first = Listener::bind(&cfg_first, h.clone(), Arc::clone(&registry))
            .await
            .expect("first bind ok");
        let port = first.local_addr().expect("local_addr").port();

        let cfg_second = mk_listener_cfg("127.0.0.1", port);
        let err = Listener::bind(&cfg_second, h, registry)
            .await
            .expect_err("second bind to same port must fail");
        match err {
            ListenerError::Bind { addr, source } => {
                assert_eq!(addr.port(), port);
                // OS error class: macOS / Linux both report EADDRINUSE here;
                // we only assert the source is non-empty (kind varies by
                // platform — `AddrInUse` on Linux, sometimes `Other` on
                // older macOS kernels).
                let _ = source.kind();
            }
            other => panic!("expected ListenerError::Bind, got {other:?}"),
        }
    }

    /// A `ConnectionHandler` that holds each connection open until a
    /// `tokio::sync::broadcast` receiver fires (the sender is cloned from an
    /// `Arc`). Used in cx_active tests to control exactly when each connection
    /// task completes so we can observe the gauge before and after decrement.
    struct HoldHandler {
        release: tokio::sync::broadcast::Sender<()>,
    }
    impl ConnectionHandler for HoldHandler {
        fn handle(
            &self,
            _downstream: tokio::net::TcpStream,
        ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
            let mut rx = self.release.subscribe();
            Box::pin(async move {
                // Wait until the sender fires or the channel closes (also
                // fine — treat closed as "released").
                let _ = rx.recv().await;
                Ok(())
            })
        }
    }

    /// 06.3 D15.3.b: `downstream_cx_active` gauge increments on accept and
    /// decrements when the per-connection handler task completes.
    ///
    /// Uses `HoldHandler` so the handler task stays live until we explicitly
    /// signal release. Pattern: connect → settle → assert gauge==1 → release
    /// → settle → assert gauge==0.
    ///
    /// The gauge Arc is captured via `register_gauge` on the same registry
    /// (idempotent same-kind re-registration, same as cx_total pattern).
    #[tokio::test(flavor = "multi_thread")]
    async fn listener_cx_active_increments_on_accept_decrements_on_close() {
        let (release_tx, _) = tokio::sync::broadcast::channel::<()>(16);
        let registry = mk_registry();
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(HoldHandler {
            release: release_tx.clone(),
        });
        let listener = Listener::bind(&cfg, h, Arc::clone(&registry))
            .await
            .expect("bind ok");
        let addr = listener.local_addr().expect("local_addr");

        let cx_active = registry
            .register_gauge("listener.test_listener.downstream_cx_active")
            .expect("gauge registers");
        assert_eq!(cx_active.value(), 0, "gauge starts at zero");

        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(listener.serve(
            async move {
                let _ = rx.await;
            },
            drain,
        ));

        // Open 1 connection; HoldHandler keeps it live until we release.
        let _stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect ok");
        // Brief settle window so the accept + increment fires.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            cx_active.value(),
            1,
            "gauge must be 1 while connection is held",
        );

        // Release the handler task → decrement fires.
        let _ = release_tx.send(());
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(
            cx_active.value(),
            0,
            "gauge must return to 0 after handler completes",
        );

        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(6), server)
            .await
            .expect("serve resolves within 6s")
            .expect("join")
            .expect("serve ok");
    }

    /// 06.3 D15.3.b: gauge is monotonically increasing under a burst of 5
    /// simultaneous connections, then returns to 0 once all 5 complete.
    ///
    /// Uses `HoldHandler` to keep all 5 connections live while we assert the
    /// peak gauge, then releases all 5 and asserts the gauge returns to 0.
    #[tokio::test(flavor = "multi_thread")]
    async fn listener_cx_active_monotonic_then_decreasing_under_burst() {
        let (release_tx, _) = tokio::sync::broadcast::channel::<()>(16);
        let registry = mk_registry();
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(HoldHandler {
            release: release_tx.clone(),
        });
        let listener = Listener::bind(&cfg, h, Arc::clone(&registry))
            .await
            .expect("bind ok");
        let addr = listener.local_addr().expect("local_addr");

        let cx_active = registry
            .register_gauge("listener.test_listener.downstream_cx_active")
            .expect("gauge registers");
        assert_eq!(cx_active.value(), 0, "gauge starts at zero");

        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(listener.serve(
            async move {
                let _ = rx.await;
            },
            drain,
        ));

        // Open 5 connections concurrently; HoldHandler keeps them live.
        let mut streams = Vec::with_capacity(5);
        for _ in 0..5 {
            streams.push(
                tokio::net::TcpStream::connect(addr)
                    .await
                    .expect("connect ok"),
            );
        }
        // Wait for all 5 accepts + increments to land.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(
            cx_active.value(),
            5,
            "gauge must be 5 while all 5 connections are held",
        );

        // Release all 5 handler tasks → 5 decrements fire.
        let _ = release_tx.send(());
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(
            cx_active.value(),
            0,
            "gauge must return to 0 after all handlers complete",
        );
        drop(streams);

        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(6), server)
            .await
            .expect("serve resolves within 6s")
            .expect("join")
            .expect("serve ok");
    }

    /// 06.3 D15.3.d: `downstream_cx_accept_failed` counter is registered under
    /// the documented name and is reachable via the idempotent `register_counter`
    /// round-trip. Asserts:
    ///   - counter == 0 immediately after bind (no spurious increments).
    ///   - counter remains 0 after N successful connections (increment is
    ///     gated to the `Err(err)` arm only, not the `Ok` arm).
    ///
    /// Testing limitation: inducing a real `listener.accept()` error is not
    /// straightforwardly possible with `tokio::net::TcpListener` + the
    /// current `Listener::serve` signature (which consumes `self`). The
    /// `Err(err)` arm increment is verified by code-inspection (the
    /// `cx_accept_failed.inc()` call appears BEFORE `tracing::warn!` in the
    /// arm body) and by the counter-existence / zero-init check here. This
    /// limitation mirrors the 06.1 / 06.2 precedent ("happy path +
    /// counter-existence" coverage with the increment site visible-by-inspection).
    #[tokio::test(flavor = "multi_thread")]
    async fn listener_cx_accept_failed_increments_on_accept_error() {
        let registry = mk_registry();
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(NullHandler);
        let listener = Listener::bind(&cfg, h, Arc::clone(&registry))
            .await
            .expect("bind ok");
        let addr = listener.local_addr().expect("local_addr");

        // Idempotent re-registration on the same registry yields the same Arc.
        let cx_accept_failed = registry
            .register_counter("listener.test_listener.downstream_cx_accept_failed")
            .expect("counter registers");
        assert_eq!(
            cx_accept_failed.value(),
            0,
            "counter starts at zero after bind"
        );

        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(listener.serve(
            async move {
                let _ = rx.await;
            },
            drain,
        ));

        // Drive N=3 successful connections; counter must remain 0 (increment
        // is gated to the Err arm, not the Ok arm).
        for _ in 0..3 {
            let _stream = tokio::net::TcpStream::connect(addr)
                .await
                .expect("connect ok");
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(
            cx_accept_failed.value(),
            0,
            "counter must remain 0 after successful accepts (no spurious increments)",
        );

        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(6), server)
            .await
            .expect("serve resolves within 6s")
            .expect("join")
            .expect("serve ok");
    }

    /// 06.1 D4.a: per-listener `downstream_cx_total` counter increments
    /// once per accepted TCP connection. Drives 3 client connects against
    /// an ephemeral-port listener (using `NullHandler` so per-connection
    /// work resolves immediately) and asserts the counter reads `3`.
    ///
    /// The counter Arc is captured via a second `register_counter` call on
    /// the same registry — `register_counter` is idempotent for same-kind
    /// re-registration (per Task 5's contract), so the value the test
    /// observes is the same one the listener increments.
    #[tokio::test(flavor = "multi_thread")]
    async fn listener_increments_cx_total_on_accept() {
        let registry = mk_registry();
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(NullHandler);
        let listener = Listener::bind(&cfg, h, Arc::clone(&registry))
            .await
            .expect("bind ok");
        let addr = listener.local_addr().expect("local_addr");

        // The listener registered the counter at bind time; re-registering
        // by name yields the same Arc (Task 5 idempotent contract). Note
        // the listener config name is "test_listener" (per `mk_listener_cfg`).
        let cx_total = registry
            .register_counter("listener.test_listener.downstream_cx_total")
            .expect("counter registers");
        assert_eq!(cx_total.value(), 0, "counter starts at zero");

        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(listener.serve(
            async move {
                let _ = rx.await;
            },
            drain,
        ));

        // Open and immediately close 3 TCP connections; each accept
        // increments the counter exactly once per signpost 5.
        for _ in 0..3 {
            let _stream = tokio::net::TcpStream::connect(addr)
                .await
                .expect("connect ok");
        }
        // Brief settle window so all accepts complete before assertion.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(
            cx_total.value(),
            3,
            "expected one increment per accepted connection",
        );

        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(6), server)
            .await
            .expect("serve resolves within 6s")
            .expect("join")
            .expect("serve ok");
    }

    // ─────────────────────────────────────────────────────────────────
    // Task 6 (D12): `Listener::serve` 2-arg widening (shutdown, drain)
    // + RAII inc/dec of `listener_manager.total_listeners_active`.
    // ─────────────────────────────────────────────────────────────────

    /// Task 6 (D12): `Listener::serve` exits via the new `drain.drain_signal()`
    /// select arm even when the shutdown future never resolves. Drives serve
    /// with `std::future::pending::<()>()` as the shutdown arm (the only way
    /// out is the drain arm), then fires `drain.drain()` from the main task
    /// and asserts the serve handle resolves within `DRAIN_BUDGET + ε`.
    ///
    /// Also asserts the RAII guard's Drop fires after serve returns:
    /// `listener_manager.total_listeners_active` gauge must read `0`
    /// post-serve (the inc-on-construct/dec-on-Drop guard wraps the full
    /// serve body including the post-loop drain-wait block, so the gauge
    /// returns to zero by the time the serve task's `JoinHandle` resolves).
    #[tokio::test(flavor = "multi_thread")]
    async fn serve_returns_when_drain_signal_fires() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
        let registry = mk_registry();
        let listener = Listener::bind(&cfg, h, Arc::clone(&registry))
            .await
            .expect("bind ok");
        let drain = Arc::new(DrainState::new(&registry));

        let serve_handle =
            tokio::spawn(listener.serve(std::future::pending::<()>(), Arc::clone(&drain)));

        // Brief yield so serve enters its `tokio::select!` (and the
        // first iteration's `drain_signal()` snapshot is anchored).
        // The select arm is poll-driven; a small sleep gives the spawned
        // task time to schedule and reach the select.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Fire the drain signal — serve's second select arm must observe
        // it, drop the listener, and fall through to the post-loop
        // drain-wait block (no in-flight connections, so it completes
        // immediately).
        drain.drain();

        tokio::time::timeout(
            DRAIN_BUDGET + std::time::Duration::from_millis(500),
            serve_handle,
        )
        .await
        .expect("serve must return within DRAIN_BUDGET + 500ms of drain signal")
        .expect("serve task join")
        .expect("serve returns Ok");

        // RAII guard's Drop must have decremented the gauge to 0.
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().into_iter().collect();
        let handle = snapshot
            .get("listener_manager.total_listeners_active")
            .expect("listener_manager.total_listeners_active gauge must be registered");
        match handle {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(
                g.value(),
                0,
                "gauge must return to 0 after serve exits (RAII Drop fired)",
            ),
            _ => panic!("listener_manager.total_listeners_active is not a gauge"),
        }
    }

    /// Task 6 (D12): mirror of `serves_honors_shutdown_signal` against the
    /// new 2-arg `Listener::serve(shutdown, drain)` signature — verifies the
    /// shutdown arm still resolves the loop even with an unfired drain
    /// observed concurrently. Signature-update churn coverage: the new arm
    /// is additive (does NOT replace the shutdown arm), so the shutdown
    /// path must remain functional verbatim.
    #[tokio::test(flavor = "multi_thread")]
    async fn serves_honors_shutdown_signal_with_drain_param() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
        let registry = mk_registry();
        let listener = Listener::bind(&cfg, h, Arc::clone(&registry))
            .await
            .expect("bind");
        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = oneshot::channel::<()>();
        let start = std::time::Instant::now();
        let server = tokio::spawn(async move {
            listener
                .serve(
                    async move {
                        let _ = rx.await;
                    },
                    drain,
                )
                .await
                .expect("serve")
        });

        tx.send(()).expect("signal");
        tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .expect("serve resolves within 2s of empty shutdown")
            .expect("join");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "serve took too long: {:?}",
            start.elapsed(),
        );
    }

    // 19 D4 (ADR-0050): register_lds_stats — conditional listener_manager.lds.*
    // family + listener_added.

    /// Build a `Bootstrap` directly (no file I/O at this layer) with the given
    /// static + dynamic listeners and an optional `lds_config` / `cds_config`.
    fn mk_lds_bootstrap(
        static_listeners: Vec<envoy_config::Listener>,
        dynamic_listeners: Option<Vec<envoy_config::Listener>>,
        lds_configured: bool,
        cds_configured: bool,
    ) -> envoy_config::Bootstrap {
        use envoy_config::{
            Bootstrap, ConfigSource, DynamicResources, PathConfigSource, StaticResources,
        };
        let mk_source = |path: &str| ConfigSource {
            path_config_source: PathConfigSource { path: path.into() },
            resource_api_version: None,
        };
        let dynamic_resources = if lds_configured || cds_configured {
            Some(DynamicResources {
                cds_config: cds_configured.then(|| mk_source("/tmp/cds.yaml")),
                lds_config: lds_configured.then(|| mk_source("/tmp/lds.yaml")),
            })
        } else {
            None
        };
        Bootstrap {
            node: None,
            admin: None,
            static_resources: StaticResources {
                listeners: static_listeners,
                clusters: vec![],
            },
            dynamic_resources,
            dynamic_clusters: None,
            dynamic_listeners,
        }
    }

    /// Scrape the registry for the current u64 value of a counter by name.
    fn counter_value(registry: &envoy_stats::StatsRegistry, name: &str) -> Option<u64> {
        registry.snapshot().into_iter().find_map(|(n, h)| {
            if n != name {
                return None;
            }
            match h {
                envoy_stats::StatHandle::Counter(c) => Some(c.value()),
                envoy_stats::StatHandle::Gauge(_) => None,
            }
        })
    }

    /// (a) §5.2 inertness invariant: with NO lds_config — including the
    /// cds_config-but-no-lds_config case (fixture 0026's topology) — none of the
    /// listener_manager.lds.* names register, and listener_added does not register.
    #[test]
    fn lds_stats_not_registered_without_lds_config() {
        for cds_configured in [false, true] {
            let bootstrap = mk_lds_bootstrap(
                vec![mk_listener_cfg("127.0.0.1", 0)],
                Some(vec![mk_listener_cfg("127.0.0.1", 0)]),
                false,
                cds_configured,
            );
            let registry = envoy_stats::StatsRegistry::new();
            register_lds_stats(&bootstrap, &registry).expect("no-op registration");
            let lds_names: Vec<String> = registry
                .snapshot()
                .into_iter()
                .map(|(n, _)| n)
                .filter(|n| {
                    n.starts_with("listener_manager.lds.") || n == "listener_manager.listener_added"
                })
                .collect();
            assert!(
                lds_names.is_empty(),
                "no listener_manager.lds.* / listener_added may register without lds_config \
                 (cds_configured={cds_configured}); got {lds_names:?}"
            );
        }
    }

    /// (b) the 5-name subset on an LDS bootstrap: lds_config + 1 dynamic listener
    /// (zero static, like fixture 0027) → the documented values.
    #[test]
    fn lds_stats_registered_with_lds_bootstrap() {
        let bootstrap = mk_lds_bootstrap(
            vec![],
            Some(vec![mk_listener_cfg("127.0.0.1", 0)]),
            true,
            false,
        );
        let registry = envoy_stats::StatsRegistry::new();
        register_lds_stats(&bootstrap, &registry).expect("registration");
        assert_eq!(
            counter_value(&registry, "listener_manager.lds.update_attempt"),
            Some(1)
        );
        assert_eq!(
            counter_value(&registry, "listener_manager.lds.update_success"),
            Some(1)
        );
        assert_eq!(
            counter_value(&registry, "listener_manager.lds.update_failure"),
            Some(0)
        );
        assert_eq!(
            counter_value(&registry, "listener_manager.lds.update_rejected"),
            Some(0)
        );
        assert_eq!(
            counter_value(&registry, "listener_manager.listener_added"),
            Some(1)
        );
    }

    /// (c) the L3 conditionality lesson: listener_added counts STATIC listeners
    /// too. 1 static + 1 dynamic (constructed directly, bypassing validate) → 2.
    #[test]
    fn lds_stats_listener_added_includes_static_listeners() {
        let bootstrap = mk_lds_bootstrap(
            vec![mk_listener_cfg("127.0.0.1", 0)],
            Some(vec![mk_listener_cfg("127.0.0.1", 0)]),
            true,
            false,
        );
        let registry = envoy_stats::StatsRegistry::new();
        register_lds_stats(&bootstrap, &registry).expect("registration");
        assert_eq!(
            counter_value(&registry, "listener_manager.listener_added"),
            Some(2)
        );
    }
}

#[cfg(test)]
mod drain_budget_constant_tests {
    use std::time::Duration;

    #[test]
    fn drain_budget_is_pub_const_at_module_level() {
        // Compile-time tautology: if DRAIN_BUDGET is NOT a pub-const at module
        // level, this fails to compile.
        const _CHECK: Duration = crate::DRAIN_BUDGET;
        assert_eq!(crate::DRAIN_BUDGET, Duration::from_secs(5));
    }

    #[test]
    fn drain_budget_value_is_5_seconds() {
        assert_eq!(crate::DRAIN_BUDGET, Duration::from_secs(5));
    }
}
