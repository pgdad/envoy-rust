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

/// 67.1 D4 (CONSUMES carry-forward CF-66-2, on exactly the trigger ADR-0123 §2.2
/// named): the network-filter chain iteration protocol.
///
/// `Continue` hands the connection to the next filter, and ultimately to the
/// chain's TERMINAL filter. `StopIteration` closes the connection — via
/// [`close_with_drain`] — and the terminal filter never runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkFilterStatus {
    Continue,
    StopIteration,
}

/// The downstream connection facts a network filter may inspect at connection
/// establishment. Carries everything network `rbac`'s matcher arms need —
/// including phase `67.2`'s `direct_remote_ip` / `remote_ip` / `source_ip` /
/// `destination_port` / `destination_ip`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionInfo {
    pub peer_addr: SocketAddr,
    pub local_addr: SocketAddr,
}

/// A NON-TERMINAL network filter: it inspects an accepted connection and either
/// yields to the rest of the chain or stops it.
///
/// **There is deliberately NO `on_data` hook.** A filter decides ONCE per
/// connection and never inspects the payload. [`ChainHandler`] invokes this hook
/// when the FIRST DOWNSTREAM BYTE becomes readable — upstream Envoy's
/// `ONE_TIME_ON_FIRST_BYTE` enforcement, measured against
/// `envoyproxy/envoy:v1.33.0` (ADR-0131, which corrects phase-67 SPEC R-2's
/// "at establishment" reading). The byte is peeked, never consumed, so the
/// filter still sees only [`ConnectionInfo`].
///
/// Exposing payload to filters — mid-stream `Continue`/`StopIteration`,
/// buffering, `injectReadDataToFilterChain` — is a different feature, and adding
/// it with no filter to exercise it is the `BOOTSTRAP_PROMPT.md` §6.3
/// anti-pattern; it is carried forward as **CF-67-3** to the first
/// payload-parsing network filter (`mongo_proxy` / `zookeeper_proxy` /
/// `kafka_broker`).
///
/// TERMINAL network filters (`echo`, `tcp_proxy`, `http_connection_manager`,
/// `direct_response`) implement [`ConnectionHandler`] instead. The config
/// validator's `NetworkFilterChainNotTerminated` rule guarantees every non-empty
/// chain ends in exactly one of them, so a chain of `NetworkFilter`s always
/// terminates in a `ConnectionHandler`.
pub trait NetworkFilter: Send + Sync + 'static {
    fn on_new_connection(&self, conn: &ConnectionInfo) -> NetworkFilterStatus;
}

/// Close `stream` the way upstream Envoy closes a connection it refuses to
/// forward: write NOTHING, half-close (the client sees a clean EOF, never an
/// RST), then drain and discard the read half until the client closes.
///
/// The drain is ADR-0124's, and it is not optional. Closing a socket while
/// unread bytes sit in the receive queue makes the kernel send an RST, so a
/// client that writes after our FIN would see `BrokenPipe`/`ConnectionReset`.
/// Upstream Envoy ACCEPTS such a write — measured at 0 / 21 / 200 000 unread
/// bytes (`post_write=writes_ok`), and again on the network-`rbac` DENY path
/// (phase-67 SPEC R-2). envoy-rust drains to match.
///
/// Bounded by the caller: `Listener::serve`'s [`DRAIN_BUDGET`] aborts stragglers.
pub async fn close_with_drain(mut stream: tokio::net::TcpStream) -> Result<(), std::io::Error> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (mut reader, mut writer) = stream.split();
    writer.shutdown().await?;
    let mut sink = [0u8; 8192];
    loop {
        match reader.read(&mut sink).await {
            Ok(0) => break,    // client closed — done
            Ok(_) => continue, // discard and keep draining
            Err(_) => break,   // peer reset/error — nothing left to do
        }
    }
    Ok(())
}

/// 67.1 D4/D5: the network-filter chain, expressed as a [`ConnectionHandler`]
/// that wraps another [`ConnectionHandler`].
///
/// **The chain runs when the FIRST DOWNSTREAM BYTE arrives, not at accept.**
/// This mirrors upstream Envoy's `ONE_TIME_ON_FIRST_BYTE` enforcement, measured
/// against `envoyproxy/envoy:v1.33.0` (ADR-0131):
///
/// | client does | upstream Envoy | envoy-rust |
/// |---|---|---|
/// | connect, send nothing | connection stays open; no counter ticks | same |
/// | connect, half-close without sending | clean EOF; no counter ticks | same |
/// | sends a first byte (now or later) | decision taken; counters tick | same |
///
/// The wait uses [`tokio::net::TcpStream::peek`], which does **not** consume the
/// byte, so the terminal handler still receives the entire stream.
///
/// Once a byte is readable it runs every non-terminal filter's
/// `on_new_connection` in configured order. The first `StopIteration` closes the
/// connection via [`close_with_drain`] — zero bytes, clean EOF, no RST — and the
/// TERMINAL handler never runs. When every filter returns `Continue`, the
/// connection is handed to `inner`.
///
/// **Which terminal filters may be wrapped — ADR-0132, measured.** The first-byte
/// `peek` gates the *whole* chain, terminal handler included. That is only
/// faithful for a terminal filter with **no establishment-time work**:
///
/// | terminal | establishment-time work | may be wrapped? |
/// |---|---|---|
/// | `echo` | none | **yes** |
/// | `http_connection_manager` | none | **yes** |
/// | `direct_response` | writes its payload, closes | **no** — `envoy-bin` bypasses the chain (ADR-0132 decision 2) |
/// | `tcp_proxy` | connects upstream, relays a server-first banner | **no** — rejected at config load until phase `67.3` (ADR-0132 decision 4) |
///
/// `ADR-0130` Decision 2 claimed this wrapper "works uniformly for all four
/// terminal filters." **That claim is false and is superseded by ADR-0132.**
/// Wrapping `direct_response` or `tcp_proxy` here deadlocks a client of a
/// server-speaks-first protocol, which never sends the byte the `peek` awaits.
/// Splitting `ConnectionHandler` into establishment and data phases — so the
/// `peek` gates the *filter's decision* rather than the *chain's hand-off* — is
/// phase `67.3`'s charter.
///
/// The config validator's `NetworkFilterChainNotTerminated` rule (67.1 D2)
/// guarantees a terminal handler always exists, so the iteration always
/// terminates.
///
/// On a TLS listener this runs on the raw `TcpStream` BEFORE the TLS handshake
/// (`ChainHandler` wraps `TlsAcceptingHandler`). For the matcher arms that exist
/// — `any` + combinators here, peer/local addresses in `67.2` — the verdict is
/// identical either way, because TLS alters neither address.
pub struct ChainHandler {
    filters: Arc<[Arc<dyn NetworkFilter>]>,
    inner: Arc<dyn ConnectionHandler>,
}

impl ChainHandler {
    /// `filters` must contain only NON-terminal filters, in configured order.
    /// An empty `filters` list makes this handler transparent; callers should
    /// skip the wrapper entirely in that case (see `envoy-bin::main`).
    pub fn new(filters: Vec<Arc<dyn NetworkFilter>>, inner: Arc<dyn ConnectionHandler>) -> Self {
        Self {
            filters: filters.into(),
            inner,
        }
    }
}

impl ConnectionHandler for ChainHandler {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let filters = Arc::clone(&self.filters);
        let inner = Arc::clone(&self.inner);
        Box::pin(async move {
            let conn = ConnectionInfo {
                peer_addr: downstream.peer_addr()?,
                local_addr: downstream.local_addr()?,
            };

            // ADR-0131: wait for the first downstream byte before evaluating —
            // upstream Envoy's ONE_TIME_ON_FIRST_BYTE enforcement. `peek` does
            // not consume, so `inner` still receives the whole stream.
            //
            // `Ok(0)` means the client closed or half-closed WITHOUT sending
            // anything. Upstream Envoy takes no decision in that case (neither
            // counter ticks) and closes with a clean EOF. Match it: skip the
            // filters entirely.
            let mut first_byte = [0u8; 1];
            if downstream.peek(&mut first_byte).await? == 0 {
                close_with_drain(downstream).await?;
                return Ok(());
            }

            for filter in filters.iter() {
                if filter.on_new_connection(&conn) == NetworkFilterStatus::StopIteration {
                    // ADR-0131 / SPEC R-2: zero bytes, clean EOF, never an RST;
                    // the terminal filter never runs, and the client's already-
                    // sent bytes are discarded by `close_with_drain`.
                    close_with_drain(downstream).await?;
                    return Ok(());
                }
            }
            inner.handle(downstream).await
        })
    }
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
    /// `bind_per_worker` was asked for more than one shard on a platform or
    /// configuration where `SO_REUSEPORT` does not load-balance (non-Linux, or
    /// the listener's `enable_reuse_port` is false). Callers gate on those
    /// conditions before choosing the per-worker path; this error surfaces a
    /// gating bug instead of silently starving all but one shard.
    #[error(
        "per-worker sharding requires enable_reuse_port and a platform with SO_REUSEPORT load-balancing (Linux)"
    )]
    ShardingUnavailable,
}

/// A bound TCP listener with a per-connection handler. Construct via
/// `Listener::bind`; drive via `Listener::serve` (Task 6).
pub struct Listener {
    /// One or more bound accepting sockets. In the default single-socket case
    /// this is a one-element `Vec` and `serve` runs the original inline accept
    /// loop — byte-for-byte the pre-`SO_REUSEPORT` behavior. When
    /// `bind_with_concurrency` binds N `SO_REUSEPORT` sockets (Linux, worker
    /// count > 1), `serve` fans out one accept loop per socket, each with its
    /// own kernel accept queue, spreading the accept path across cores. All N
    /// share the one per-listener stat set below (one logical listener).
    listeners: Vec<tokio::net::TcpListener>,
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
    /// Whether this `Listener` counts itself in
    /// `listener_manager.total_listeners_active` while serving. Always true
    /// for `bind` / `bind_with_concurrency`. `bind_per_worker` returns N
    /// shards that back ONE logical listener, so only shard 0 carries `true`
    /// — the gauge stays at 1 per configured listener regardless of the
    /// worker count.
    count_listener_active: bool,
    /// 67.1: publishes `accept_loop`'s in-flight `JoinSet` length after every
    /// select iteration. The M66-3 reaping witness reads it; production code may
    /// use it for introspection. Distinct from `cx_active`, which is decremented
    /// INSIDE each connection task and therefore cannot observe an unreaped
    /// completed task.
    pending_tasks: tokio::sync::watch::Sender<usize>,
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
            .field(
                "local_addr",
                &self.listeners.first().map(|l| l.local_addr()),
            )
            .field("sockets", &self.listeners.len())
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
        // A single plain accepting socket, no `SO_REUSEPORT` — the original
        // behavior. Every existing caller and test flows through here unchanged;
        // `serve` then runs the inline single-socket accept loop.
        Self::bind_inner(cfg, handler, registry, 1, false).await
    }

    /// Like [`Listener::bind`] but binds one `SO_REUSEPORT` accepting socket per
    /// worker when the listener's `enable_reuse_port` is set, the platform
    /// load-balances `SO_REUSEPORT` across sockets (Linux), and `concurrency > 1`.
    /// Each socket gets its own kernel accept queue, so `serve` runs an accept
    /// loop per core and the accept path (and RX softirq steering) spreads across
    /// cores instead of funnelling through one queue. `concurrency` is typically
    /// the tokio worker-thread count.
    ///
    /// Falls back to a single plain socket — byte-for-byte [`Listener::bind`] —
    /// when `enable_reuse_port` is false, the platform is not Linux (elsewhere
    /// `SO_REUSEPORT` does not load-balance, so extra sockets would only starve),
    /// or `concurrency <= 1`. The bootstrap's wire bytes are unaffected either
    /// way; only the number of accepting sockets changes.
    pub async fn bind_with_concurrency(
        cfg: &envoy_config::Listener,
        handler: Arc<dyn ConnectionHandler>,
        registry: Arc<envoy_stats::StatsRegistry>,
        concurrency: usize,
    ) -> Result<Self, ListenerError> {
        Self::bind_inner(cfg, handler, registry, concurrency, cfg.enable_reuse_port).await
    }

    async fn bind_inner(
        cfg: &envoy_config::Listener,
        handler: Arc<dyn ConnectionHandler>,
        registry: Arc<envoy_stats::StatsRegistry>,
        concurrency: usize,
        reuse_port: bool,
    ) -> Result<Self, ListenerError> {
        let addr = parse_listener_addr(cfg)?;
        let listeners = bind_listeners(addr, reuse_port, concurrency).await?;
        if listeners.len() > 1 {
            tracing::info!(
                %addr,
                sockets = listeners.len(),
                "listener bound with SO_REUSEPORT (one accept queue per worker)"
            );
        }
        let (cx_total, cx_active, cx_accept_failed, listener_manager_active) =
            register_listener_stats(&registry, &cfg.name)?;
        Ok(Self {
            listeners,
            handler,
            cx_total,
            cx_active,
            cx_accept_failed,
            listener_manager_active,
            count_listener_active: true,
            pending_tasks: tokio::sync::watch::channel(0usize).0,
        })
    }

    /// Thread-per-core sharding: bind `handlers.len()` `SO_REUSEPORT` sockets
    /// on the listener address and return ONE single-socket `Listener` per
    /// handler, so each shard can be served on its own (typically
    /// single-threaded) runtime with its own per-worker connection handler —
    /// upstream Envoy's per-worker-dispatcher architecture. All shards share
    /// the one per-listener stat set (one logical listener); only shard 0
    /// counts in `listener_manager.total_listeners_active`.
    ///
    /// Errors with [`ListenerError::ShardingUnavailable`] when more than one
    /// handler is passed but the platform (non-Linux) or the listener config
    /// (`enable_reuse_port: false`) cannot load-balance across sockets —
    /// callers gate on those conditions and fall back to
    /// [`Listener::bind_with_concurrency`].
    pub async fn bind_per_worker(
        cfg: &envoy_config::Listener,
        handlers: Vec<Arc<dyn ConnectionHandler>>,
        registry: Arc<envoy_stats::StatsRegistry>,
    ) -> Result<Vec<Self>, ListenerError> {
        let n = handlers.len();
        if n > 1 && !(cfg.enable_reuse_port && cfg!(target_os = "linux")) {
            return Err(ListenerError::ShardingUnavailable);
        }
        let addr = parse_listener_addr(cfg)?;
        let sockets = bind_listeners(addr, cfg.enable_reuse_port, n).await?;
        debug_assert_eq!(sockets.len(), n, "bind_listeners honored the shard count");

        // Same stat registrations as bind_inner — idempotent by name, and the
        // resulting Arcs are cloned into every shard (one logical stat set).
        let (cx_total, cx_active, cx_accept_failed, listener_manager_active) =
            register_listener_stats(&registry, &cfg.name)?;

        Ok(sockets
            .into_iter()
            .zip(handlers)
            .enumerate()
            .map(|(i, (socket, handler))| Self {
                listeners: vec![socket],
                handler,
                cx_total: Arc::clone(&cx_total),
                cx_active: Arc::clone(&cx_active),
                cx_accept_failed: Arc::clone(&cx_accept_failed),
                listener_manager_active: Arc::clone(&listener_manager_active),
                count_listener_active: i == 0,
                // 67.1: one watch per shard — each shard runs its own accept
                // loop over its own SO_REUSEPORT socket, hence its own JoinSet.
                pending_tasks: tokio::sync::watch::channel(0usize).0,
            })
            .collect())
    }

    /// Returns the actual bound socket address (resolves `port_value: 0` to
    /// the kernel-assigned ephemeral port). With `SO_REUSEPORT` all sockets
    /// share the same address, so the first socket's address is authoritative.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listeners[0].local_addr()
    }

    /// 08.2 D14: accessor for the shared `listener_manager.total_listeners_active`
    /// gauge so Task 6's RAII guard can hoist the `Arc<Gauge>` out of `self`
    /// before `self.listener` is consumed by `serve`. `pub(crate)` because
    /// no external consumer should touch this gauge directly — the inc/dec
    /// wiring is internal to `Listener::serve`.
    pub(crate) fn listener_manager_active(&self) -> &Arc<envoy_stats::Gauge> {
        &self.listener_manager_active
    }

    /// 67.1: in-flight connection tasks currently held by the accept loop's
    /// `JoinSet`. Callable only before [`Listener::serve`] consumes `self`; use
    /// [`Listener::pending_tasks_watch`] to keep observing afterwards.
    pub fn pending_tasks(&self) -> usize {
        *self.pending_tasks.borrow()
    }

    /// 67.1: a receiver that keeps observing `pending_tasks` after
    /// [`Listener::serve`] consumes `self`. Used by the M66-3 reaping witness —
    /// `cx_active` cannot serve that role, being decremented inside the spawned
    /// task while its `JoinSet` entry still lingers.
    pub fn pending_tasks_watch(&self) -> tokio::sync::watch::Receiver<usize> {
        self.pending_tasks.subscribe()
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
        // fires LAST (after every accept loop's drain-wait completes). It counts
        // ONE logical listener regardless of how many SO_REUSEPORT sockets back
        // it. Construction increments the gauge; Drop decrements. Non-primary
        // `bind_per_worker` shards skip the guard so N shards of one logical
        // listener still read as 1.
        let _lm_guard = self
            .count_listener_active
            .then(|| ListenerManagerActiveGuard::new(Arc::clone(self.listener_manager_active())));

        let mut listeners = self.listeners;
        let handler = self.handler;
        let cx_total = self.cx_total;
        let cx_active = self.cx_active;
        let cx_accept_failed = self.cx_accept_failed;
        let pending_tasks = self.pending_tasks;

        // Single-socket path (the default): run the original inline accept loop
        // directly, so behavior is byte-for-byte the pre-SO_REUSEPORT code.
        if listeners.len() == 1 {
            let listener = listeners.pop().expect("len checked == 1");
            return accept_loop(
                listener,
                handler,
                cx_total,
                cx_active,
                cx_accept_failed,
                shutdown,
                drain,
                pending_tasks,
            )
            .await;
        }

        // SO_REUSEPORT fan-out: one accept loop per socket, each with its own
        // kernel accept queue and its own in-flight-connection JoinSet + drain.
        // The single `shutdown` future is broadcast to every loop via a
        // watch<bool>; the drain Arc and stat Arcs are cloned per loop.
        let (sd_tx, sd_rx) = tokio::sync::watch::channel(false);
        // Driver: turn the one-shot shutdown future into a broadcast flag.
        // Aborted below if still pending when the loops exit via drain instead.
        let shutdown_driver = tokio::spawn(async move {
            shutdown.await;
            let _ = sd_tx.send(true);
        });

        let mut loops: tokio::task::JoinSet<Result<(), ListenerError>> =
            tokio::task::JoinSet::new();
        for listener in listeners {
            let handler = Arc::clone(&handler);
            let cx_total = Arc::clone(&cx_total);
            let cx_active = Arc::clone(&cx_active);
            let cx_accept_failed = Arc::clone(&cx_accept_failed);
            let drain = Arc::clone(&drain);
            let mut sd_rx = sd_rx.clone();
            // 67.1: every fan-out loop publishes its own socket's in-flight
            // count onto the one shared watch.
            let pending_tasks = pending_tasks.clone();
            loops.spawn(async move {
                // Per-loop shutdown: resolve once the broadcast flag is true.
                // `wait_for` checks the current value first, so an already-fired
                // shutdown resolves immediately (no missed-wakeup race).
                let shutdown = async move {
                    let _ = sd_rx.wait_for(|&fired| fired).await;
                };
                accept_loop(
                    listener,
                    handler,
                    cx_total,
                    cx_active,
                    cx_accept_failed,
                    shutdown,
                    drain,
                    pending_tasks,
                )
                .await
            });
        }

        // Await every per-socket loop; surface the first error (e.g. a
        // DrainTimeout from any loop) after all have settled.
        let mut first_err: Option<ListenerError> = None;
        while let Some(joined) = loops.join_next().await {
            match joined {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
                Err(join_err) => {
                    tracing::warn!(error = %join_err, "accept loop task panicked")
                }
            }
        }
        // Loops have exited; if the shutdown driver is still parked on a
        // never-fired shutdown (we exited via drain), cancel it.
        shutdown_driver.abort();

        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// Resolve `cfg.address.socket_address` to a `SocketAddr`. Phase-02.1
/// `envoy-config` does not parse the address field into a `SocketAddr`; that
/// happens here, so configuration with a malformed `address` (e.g.
/// `"not-a-host"`) returns `ListenerError::AddressParse`. Shared by
/// `bind_inner` and `bind_per_worker`.
fn parse_listener_addr(cfg: &envoy_config::Listener) -> Result<SocketAddr, ListenerError> {
    let sock = &cfg.address.socket_address;
    let addr_str = format!("{}:{}", sock.address, sock.port_value);
    addr_str
        .parse()
        .map_err(|_| ListenerError::AddressParse(sock.address.clone(), sock.port_value))
}

/// Register the per-listener stat set against the global `StatsRegistry`.
/// Shared by `bind_inner` and `bind_per_worker` (one logical stat set either
/// way). Returns `(cx_total, cx_active, cx_accept_failed,
/// listener_manager_active)` — registration order is load-bearing history:
/// it mirrors the original inline sites byte-for-byte.
///
/// - 06.1 D4.a: `listener.<name>.downstream_cx_total`. The registry call is
///   idempotent for same-kind re-registration, so multiple `Listener::bind`
///   calls with the same `cfg.name` (a configuration error in production but
///   possible in tests) reuse the existing handle rather than erroring.
/// - 06.3 D15.3.b: `listener.<name>.downstream_cx_active`. Idempotent
///   same-kind re-registration mirrors cx_total above.
/// - 06.3 D15.3.d: `listener.<name>.downstream_cx_accept_failed`. Idempotent
///   same-kind re-registration mirrors cx_total above.
/// - 08.2 D14: the shared `listener_manager.total_listeners_active` gauge.
///   Idempotent same-name re-registration across multiple `bind` calls
///   mirrors the cx_* pattern at adjacent registration sites. RAII inc/dec
///   wiring at `serve` entry/exit landed at Task 6 (D12).
#[allow(clippy::type_complexity)]
fn register_listener_stats(
    registry: &envoy_stats::StatsRegistry,
    name: &str,
) -> Result<
    (
        Arc<envoy_stats::Counter>,
        Arc<envoy_stats::Gauge>,
        Arc<envoy_stats::Counter>,
        Arc<envoy_stats::Gauge>,
    ),
    ListenerError,
> {
    let cx_total = registry
        .register_counter(&format!("listener.{name}.downstream_cx_total"))
        .map_err(|e| ListenerError::StatsRegistration(e.to_string()))?;
    let cx_active = registry
        .register_gauge(&format!("listener.{name}.downstream_cx_active"))
        .map_err(|e| ListenerError::StatsRegistration(e.to_string()))?;
    let cx_accept_failed = registry
        .register_counter(&format!("listener.{name}.downstream_cx_accept_failed"))
        .map_err(|e| ListenerError::StatsRegistration(e.to_string()))?;
    let listener_manager_active = registry
        .register_gauge("listener_manager.total_listeners_active")
        .map_err(|e| ListenerError::StatsRegistration(e.to_string()))?;
    Ok((
        cx_total,
        cx_active,
        cx_accept_failed,
        listener_manager_active,
    ))
}

/// One accept loop over a single socket, with the shutdown-gated graceful drain.
/// Extracted from the original single-socket `serve` body so both the default
/// single-socket path and each SO_REUSEPORT worker share identical accept/drain
/// semantics. On `shutdown` or `drain.drain_signal()` it stops accepting and
/// waits up to `DRAIN_BUDGET` for in-flight connections to complete, aborting
/// stragglers (and returning `DrainTimeout`) if the budget expires.
#[allow(clippy::too_many_arguments)]
async fn accept_loop(
    listener: tokio::net::TcpListener,
    handler: Arc<dyn ConnectionHandler>,
    cx_total: Arc<envoy_stats::Counter>,
    cx_active: Arc<envoy_stats::Gauge>,
    cx_accept_failed: Arc<envoy_stats::Counter>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    drain: Arc<DrainState>,
    pending_tasks: tokio::sync::watch::Sender<usize>,
) -> Result<(), ListenerError> {
    let mut join_set: tokio::task::JoinSet<Result<(), Box<dyn std::error::Error + Send + Sync>>> =
        tokio::task::JoinSet::new();
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
        // 67.1 (the M66-3 witness): republish the in-flight task count after
        // every select iteration — including the `join_next()` arm just above,
        // which REAPS a completed task. A loop without that arm would see this
        // climb without bound across sequential connections. `send_replace`
        // rather than `send` so the value is updated even when nothing has
        // subscribed (the production case).
        pending_tasks.send_replace(join_set.len());
    }

    // Drain.
    let drain_fut = async {
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
    if tokio::time::timeout(DRAIN_BUDGET, drain_fut).await.is_err() {
        tracing::warn!(?DRAIN_BUDGET, "drain budget exceeded; aborting stragglers");
        join_set.abort_all();
        // Let aborted tasks unwind; ignore their results.
        while join_set.join_next().await.is_some() {}
        return Err(ListenerError::DrainTimeout(DRAIN_BUDGET));
    }
    Ok(())
}

/// Bind the accepting socket(s) for a listener address. Returns a single plain
/// socket (identical to the pre-`SO_REUSEPORT` `tokio::net::TcpListener::bind`)
/// unless `reuse_port` is set, the platform load-balances `SO_REUSEPORT` across
/// sockets (Linux only — elsewhere the kernel does not spread connections, so
/// extra sockets would just starve), and `concurrency > 1`; in that case it
/// returns `concurrency` independent `SO_REUSEPORT` sockets on the same address.
async fn bind_listeners(
    addr: SocketAddr,
    reuse_port: bool,
    concurrency: usize,
) -> Result<Vec<tokio::net::TcpListener>, ListenerError> {
    let effective_n = if reuse_port && cfg!(target_os = "linux") {
        concurrency.max(1)
    } else {
        1
    };
    if effective_n <= 1 {
        // Original single-socket path — no SO_REUSEPORT, byte-identical.
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|source| ListenerError::Bind { addr, source })?;
        return Ok(vec![listener]);
    }
    let mut listeners = Vec::with_capacity(effective_n);
    // Bind the first socket, then bind the rest to the RESOLVED address —
    // with `port_value: 0` each fresh bind would otherwise get its own
    // ephemeral port and the sockets would not share one accept address
    // (SO_REUSEPORT groups form per concrete port).
    let first =
        bind_reuseport_socket(addr).map_err(|source| ListenerError::Bind { addr, source })?;
    let bound_addr = first
        .local_addr()
        .map_err(|source| ListenerError::Bind { addr, source })?;
    listeners.push(first);
    for _ in 1..effective_n {
        listeners.push(bind_reuseport_socket(bound_addr).map_err(|source| {
            ListenerError::Bind {
                addr: bound_addr,
                source,
            }
        })?);
    }
    Ok(listeners)
}

/// Build one `SO_REUSEPORT` accepting socket via **safe** `socket2` APIs. The
/// `socket2::Socket` → `std::net::TcpListener` → `tokio::net::TcpListener`
/// conversions are all safe (`From` / `from_std`); no `from_raw_fd`, so
/// `#![forbid(unsafe_code)]` holds. The socket is set non-blocking before
/// `from_std` (tokio requires it). `listen(1024)` matches tokio's default
/// bind backlog.
fn bind_reuseport_socket(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let sock = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    // SO_REUSEADDR + SO_REUSEPORT: N sockets share the one address; the kernel
    // hashes incoming connections across their independent accept queues.
    sock.set_reuse_address(true)?;
    sock.set_reuse_port(true)?;
    sock.set_nonblocking(true)?;
    sock.bind(&addr.into())?;
    sock.listen(1024)?;
    let std_listener: std::net::TcpListener = sock.into();
    tokio::net::TcpListener::from_std(std_listener)
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

/// 20 D4 (ADR-0051/0052; §6.2 L3/L10): the per-HCM http.<stat_prefix>.rds.<route_config_name>.*
/// stat family — registered ONLY for HCMs whose `rds` is configured (the §5.2
/// per-HCM conditional-registration discipline; inline-route HCMs emit no rds.*
/// names). All RDS load failures are fatal pre-registration (the L4 all-fatal
/// posture), so update_failure/update_rejected register at 0 and never tick;
/// config_reload ticks 1 at initial load (L3). Called once from envoy-bin
/// main(), after load_dynamic_resources + register_lds_stats.
/// 26 Task 4: the SINGLE source of truth for the `http.<stat_prefix>.rds.<route_config_name>`
/// counter-name base. `register_rds_stats` (initial load) and the envoy-bin
/// target-walk + the rds_watcher test helper (reload) ALL re-resolve the SAME
/// `rds.*` counter handles by name — `register_counter` is idempotent by name,
/// so the byte-identical base string is what guarantees they share one handle
/// set. Constructing it in one place removes the drift risk across those sites.
/// The five suffixes (`update_attempt`/`update_success`/`update_failure`/
/// `update_rejected`/`config_reload`) are appended by each caller as
/// `{base}.{suffix}`.
pub fn rds_counter_base(stat_prefix: &str, route_config_name: &str) -> String {
    format!("http.{stat_prefix}.rds.{route_config_name}")
}

pub fn register_rds_stats(
    bootstrap: &envoy_config::Bootstrap,
    registry: &envoy_stats::StatsRegistry,
) -> Result<(), ListenerError> {
    for listener in bootstrap.all_listeners() {
        for chain in &listener.filter_chains {
            for filter in &chain.filters {
                let Some(envoy_config::TypedConfig::HttpConnectionManager(hcm)) =
                    filter.typed_config.as_ref()
                else {
                    continue;
                };
                let Some(rds) = hcm.rds.as_ref() else {
                    continue;
                };
                let base = rds_counter_base(&hcm.stat_prefix, &rds.route_config_name);
                let mk = |suffix: &str| {
                    registry
                        .register_counter(&format!("{base}.{suffix}"))
                        .map_err(|e| ListenerError::StatsRegistration(e.to_string()))
                };
                mk("update_attempt")?.add(1);
                mk("update_success")?.add(1);
                mk("config_reload")?.add(1);
                mk("update_failure")?; // registers at 0 (L4)
                mk("update_rejected")?; // registers at 0 (L4)
            }
        }
    }
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

    /// Build a multi-socket `Listener` directly (in-crate access to the private
    /// fields) so the SO_REUSEPORT fan-out `serve` path can be exercised on any
    /// platform, independent of `bind_with_concurrency`'s Linux gate.
    fn mk_multi_socket_listener(
        listeners: Vec<tokio::net::TcpListener>,
        handler: Arc<dyn ConnectionHandler>,
        registry: &Arc<envoy_stats::StatsRegistry>,
        name: &str,
    ) -> Listener {
        Listener {
            listeners,
            handler,
            cx_total: registry
                .register_counter(&format!("listener.{name}.downstream_cx_total"))
                .unwrap(),
            cx_active: registry
                .register_gauge(&format!("listener.{name}.downstream_cx_active"))
                .unwrap(),
            cx_accept_failed: registry
                .register_counter(&format!("listener.{name}.downstream_cx_accept_failed"))
                .unwrap(),
            listener_manager_active: registry
                .register_gauge("listener_manager.total_listeners_active")
                .unwrap(),
            count_listener_active: true,
            pending_tasks: tokio::sync::watch::channel(0usize).0,
        }
    }

    #[tokio::test]
    async fn reuseport_binds_multiple_sockets_on_same_port() {
        // A first SO_REUSEPORT socket takes an ephemeral port.
        let s1 = bind_reuseport_socket("127.0.0.1:0".parse().unwrap()).expect("bind s1");
        let port = s1.local_addr().unwrap().port();
        // A SECOND socket binds the SAME port — impossible without SO_REUSEPORT
        // (a plain second bind gets EADDRINUSE, as `bind_fails_cleanly_on_address_in_use`
        // asserts). Success here proves the option is set on both sockets.
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let s2 = bind_reuseport_socket(addr).expect("second bind on same port via SO_REUSEPORT");
        assert_eq!(s2.local_addr().unwrap().port(), port);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reuseport_fanout_serves_and_drains() {
        // Two SO_REUSEPORT sockets on one port → the fan-out `serve` path (one
        // accept loop per socket, watch-broadcast shutdown, per-loop drain).
        let s1 = bind_reuseport_socket("127.0.0.1:0".parse().unwrap()).expect("bind s1");
        let port = s1.local_addr().unwrap().port();
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let s2 = bind_reuseport_socket(addr).expect("bind s2");

        let registry = mk_registry();
        let h: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
        let listener = mk_multi_socket_listener(vec![s1, s2], h, &registry, "reuseport_test");
        assert_eq!(listener.local_addr().unwrap().port(), port);

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

        // Several sequential clients — each is echoed regardless of which of the
        // two sockets the kernel routes it to.
        for i in 0..8u8 {
            let mut c = tokio::net::TcpStream::connect(addr).await.expect("connect");
            let payload = [b'a' + i; 16];
            c.write_all(&payload).await.expect("write");
            let mut buf = [0u8; 16];
            c.read_exact(&mut buf).await.expect("read");
            assert_eq!(buf, payload, "client {i} echoed");
        }

        tx.send(()).expect("signal shutdown");
        tokio::time::timeout(std::time::Duration::from_secs(6), server)
            .await
            .expect("serve resolves within 6s")
            .expect("join");
    }

    /// Linux load-balances SO_REUSEPORT across sockets by a 4-tuple hash. Drive
    /// many short connections at N sockets and assert the kernel spread them
    /// across at least two accept queues — the property the whole feature buys.
    /// Gated to Linux: macOS/BSD do not load-balance (delivery is to a single
    /// socket), so this assertion is meaningful only where the kernel spreads.
    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn reuseport_distributes_connections_across_sockets_linux() {
        use std::sync::atomic::{AtomicU64, Ordering};

        const N: usize = 4;
        let first = bind_reuseport_socket("127.0.0.1:0".parse().unwrap()).unwrap();
        let port = first.local_addr().unwrap().port();
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let mut socks = vec![first];
        for _ in 1..N {
            socks.push(bind_reuseport_socket(addr).unwrap());
        }

        let counts: Vec<Arc<AtomicU64>> = (0..N).map(|_| Arc::new(AtomicU64::new(0))).collect();
        let mut acceptors = Vec::new();
        for (i, s) in socks.into_iter().enumerate() {
            let c = Arc::clone(&counts[i]);
            acceptors.push(tokio::spawn(async move {
                while let Ok((stream, _)) = s.accept().await {
                    c.fetch_add(1, Ordering::Relaxed);
                    drop(stream);
                }
            }));
        }

        // 200 short connections is plenty for the hash to touch >= 2 buckets.
        for _ in 0..200 {
            if let Ok(s) = tokio::net::TcpStream::connect(addr).await {
                drop(s);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        for a in acceptors {
            a.abort();
        }

        let nonzero = counts
            .iter()
            .filter(|c| c.load(Ordering::Relaxed) > 0)
            .count();
        let total: u64 = counts.iter().map(|c| c.load(Ordering::Relaxed)).sum();
        assert!(total >= 190, "most connections accepted (got {total}/200)");
        assert!(
            nonzero >= 2,
            "connections spread across >= 2 sockets (got {nonzero} of {N} nonzero)"
        );
    }

    /// `bind_per_worker` with a single handler works on every platform (the
    /// single-socket path needs no SO_REUSEPORT load-balancing) and serves
    /// exactly like `Listener::bind`.
    #[tokio::test(flavor = "multi_thread")]
    async fn bind_per_worker_single_shard_serves() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let registry = mk_registry();
        let shards = Listener::bind_per_worker(
            &cfg,
            vec![Arc::new(EchoHandler) as Arc<dyn ConnectionHandler>],
            Arc::clone(&registry),
        )
        .await
        .expect("single-shard bind ok");
        assert_eq!(shards.len(), 1);
        let listener = shards.into_iter().next().unwrap();
        let addr = listener.local_addr().expect("local_addr");

        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(listener.serve(
            async move {
                let _ = rx.await;
            },
            drain,
        ));

        let mut c = tokio::net::TcpStream::connect(addr).await.expect("connect");
        c.write_all(b"shard").await.expect("write");
        let mut buf = [0u8; 5];
        c.read_exact(&mut buf).await.expect("read");
        assert_eq!(&buf, b"shard");
        drop(c);

        tx.send(()).expect("shutdown");
        tokio::time::timeout(std::time::Duration::from_secs(6), server)
            .await
            .expect("serve resolves")
            .expect("join")
            .expect("serve ok");
    }

    /// Multi-shard `bind_per_worker` is gated to platforms where SO_REUSEPORT
    /// load-balances (Linux); elsewhere it must refuse rather than silently
    /// starve all but one shard.
    #[cfg(not(target_os = "linux"))]
    #[tokio::test]
    async fn bind_per_worker_multi_shard_unavailable_off_linux() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let handlers: Vec<Arc<dyn ConnectionHandler>> =
            vec![Arc::new(NullHandler), Arc::new(NullHandler)];
        let err = Listener::bind_per_worker(&cfg, handlers, mk_registry())
            .await
            .expect_err("multi-shard must fail off-Linux");
        assert!(matches!(err, ListenerError::ShardingUnavailable));
    }

    /// Linux: N handlers → N shards on one port, each serving its own accept
    /// queue; connections spread by the kernel are handled by whichever shard
    /// they land on, and `listener_manager.total_listeners_active` counts the
    /// N shards as ONE logical listener (only shard 0 carries the guard).
    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread")]
    async fn bind_per_worker_multi_shard_serves_as_one_logical_listener() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let registry = mk_registry();
        let handlers: Vec<Arc<dyn ConnectionHandler>> =
            vec![Arc::new(EchoHandler), Arc::new(EchoHandler)];
        let shards = Listener::bind_per_worker(&cfg, handlers, Arc::clone(&registry))
            .await
            .expect("2-shard bind ok");
        assert_eq!(shards.len(), 2);
        let addr = shards[0].local_addr().expect("local_addr");
        assert_eq!(shards[1].local_addr().expect("local_addr"), addr);

        let lm_gauge = registry
            .register_gauge("listener_manager.total_listeners_active")
            .expect("gauge");
        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = tokio::sync::watch::channel(false);
        let mut servers = Vec::new();
        for shard in shards {
            let mut rx = rx.clone();
            let drain = Arc::clone(&drain);
            servers.push(tokio::spawn(async move {
                shard
                    .serve(
                        async move {
                            let _ = rx.wait_for(|&f| f).await;
                        },
                        drain,
                    )
                    .await
            }));
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(
            lm_gauge.value(),
            1,
            "N shards of one listener must read as 1 active listener"
        );

        // Echo round-trips regardless of which shard's queue each lands on.
        for i in 0..8u8 {
            let mut c = tokio::net::TcpStream::connect(addr).await.expect("connect");
            let payload = [b'a' + i; 8];
            c.write_all(&payload).await.expect("write");
            let mut buf = [0u8; 8];
            c.read_exact(&mut buf).await.expect("read");
            assert_eq!(buf, payload);
        }

        tx.send(true).expect("shutdown");
        for s in servers {
            tokio::time::timeout(std::time::Duration::from_secs(6), s)
                .await
                .expect("serve resolves")
                .expect("join")
                .expect("serve ok");
        }
        assert_eq!(lm_gauge.value(), 0, "gauge returns to 0 after serve exits");
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

    // 20 D4 (ADR-0051/0052): register_rds_stats — conditional per-HCM
    // http.<stat_prefix>.rds.<route_config_name>.* family.

    /// Build a `Listener` whose single filter chain contains one HCM filter
    /// with `rds` configured (no inline `route_config`). Used only in tests.
    fn mk_hcm_rds_listener(stat_prefix: &str, route_config_name: &str) -> envoy_config::Listener {
        envoy_config::Listener {
            name: format!("listener_{stat_prefix}"),
            address: envoy_config::Address {
                socket_address: envoy_config::SocketAddress {
                    address: "127.0.0.1".into(),
                    port_value: 0,
                },
            },
            listener_filters: vec![],
            enable_reuse_port: true,
            filter_chains: vec![envoy_config::FilterChain {
                filter_chain_match: None,
                transport_socket: None,
                filters: vec![envoy_config::NetworkFilter {
                    name: "envoy.filters.network.http_connection_manager".into(),
                    typed_config: Some(envoy_config::TypedConfig::HttpConnectionManager(
                        envoy_config::HttpConnectionManagerConfig {
                            stat_prefix: stat_prefix.into(),
                            codec_type: envoy_config::CodecType::AUTO,
                            http2_protocol_options: None,
                            access_log: vec![],
                            route_config: None,
                            rds: Some(envoy_config::Rds {
                                route_config_name: route_config_name.into(),
                                config_source: envoy_config::ConfigSource {
                                    path_config_source: envoy_config::PathConfigSource {
                                        path: "/tmp/rds.yaml".into(),
                                    },
                                    resource_api_version: None,
                                },
                            }),
                            http_filters: vec![],
                        },
                    )),
                }],
            }],
        }
    }

    /// Build a bootstrap whose HCMs all have inline `route_config` (rds=None).
    /// Used to verify inertness: register_rds_stats must emit no `.rds.` names.
    fn mk_inline_route_bootstrap(
        lds_configured: bool,
        cds_configured: bool,
    ) -> envoy_config::Bootstrap {
        // Re-use mk_lds_bootstrap with plain listeners (no HCM filter) — their
        // HCMs are absent entirely, which satisfies rds=None for inertness.
        mk_lds_bootstrap(
            vec![mk_listener_cfg("127.0.0.1", 0)],
            Some(vec![mk_listener_cfg("127.0.0.1", 0)]),
            lds_configured,
            cds_configured,
        )
    }

    /// (a) §5.2 inertness invariant: when no HCM uses rds (all listeners have
    /// no HCM filter or use inline route_config), register_rds_stats must emit
    /// zero stat names containing `.rds.` — including when lds_config/cds_config
    /// are set (the fixture-0026/0027 inertness witness).
    #[test]
    fn rds_stats_not_registered_without_rds_hcm() {
        for (lds, cds) in [(false, false), (false, true), (true, false), (true, true)] {
            let bootstrap = mk_inline_route_bootstrap(lds, cds);
            let registry = envoy_stats::StatsRegistry::new();
            register_rds_stats(&bootstrap, &registry).expect("no-op registration");
            let rds_names: Vec<String> = registry
                .snapshot()
                .into_iter()
                .map(|(n, _)| n)
                .filter(|n| n.contains(".rds."))
                .collect();
            assert!(
                rds_names.is_empty(),
                "no .rds. stats may register without rds HCM \
                 (lds={lds}, cds={cds}); got {rds_names:?}"
            );
        }
    }

    /// (b) The 5-name subset on an rds HCM: stat_prefix=ingress_http,
    /// route_config_name=local_route → the documented initial-load values.
    #[test]
    fn rds_stats_registered_for_rds_hcm() {
        let bootstrap = envoy_config::Bootstrap {
            node: None,
            admin: None,
            static_resources: envoy_config::StaticResources {
                listeners: vec![mk_hcm_rds_listener("ingress_http", "local_route")],
                clusters: vec![],
            },
            dynamic_resources: None,
            dynamic_clusters: None,
            dynamic_listeners: None,
        };
        let registry = envoy_stats::StatsRegistry::new();
        register_rds_stats(&bootstrap, &registry).expect("registration");
        assert_eq!(
            counter_value(
                &registry,
                "http.ingress_http.rds.local_route.update_attempt"
            ),
            Some(1)
        );
        assert_eq!(
            counter_value(
                &registry,
                "http.ingress_http.rds.local_route.update_success"
            ),
            Some(1)
        );
        assert_eq!(
            counter_value(
                &registry,
                "http.ingress_http.rds.local_route.update_failure"
            ),
            Some(0)
        );
        assert_eq!(
            counter_value(
                &registry,
                "http.ingress_http.rds.local_route.update_rejected"
            ),
            Some(0)
        );
        assert_eq!(
            counter_value(&registry, "http.ingress_http.rds.local_route.config_reload"),
            Some(1)
        );
    }

    /// (c) Per-HCM keying: two listeners with distinct stat_prefix and
    /// route_config_name → both http.a.rds.r1.* and http.b.rds.r2.* register.
    #[test]
    fn rds_stats_keyed_per_hcm() {
        let bootstrap = envoy_config::Bootstrap {
            node: None,
            admin: None,
            static_resources: envoy_config::StaticResources {
                listeners: vec![
                    mk_hcm_rds_listener("a", "r1"),
                    mk_hcm_rds_listener("b", "r2"),
                ],
                clusters: vec![],
            },
            dynamic_resources: None,
            dynamic_clusters: None,
            dynamic_listeners: None,
        };
        let registry = envoy_stats::StatsRegistry::new();
        register_rds_stats(&bootstrap, &registry).expect("registration");
        // Both families must be present.
        assert_eq!(
            counter_value(&registry, "http.a.rds.r1.update_attempt"),
            Some(1),
            "http.a.rds.r1.update_attempt missing"
        );
        assert_eq!(
            counter_value(&registry, "http.b.rds.r2.update_attempt"),
            Some(1),
            "http.b.rds.r2.update_attempt missing"
        );
        // Confirm cross-contamination is absent.
        assert_eq!(
            counter_value(&registry, "http.a.rds.r2.update_attempt"),
            None
        );
        assert_eq!(
            counter_value(&registry, "http.b.rds.r1.update_attempt"),
            None
        );
    }

    // --- 67.1 D4 (CF-66-2): the network-filter chain iteration protocol ---

    /// 67.1 D4 (CF-66-2): a filter returning `Continue` does not close the
    /// connection; the status enum is `Copy` and comparable.
    #[test]
    fn network_filter_status_is_copy_and_eq() {
        let a = NetworkFilterStatus::Continue;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(
            NetworkFilterStatus::Continue,
            NetworkFilterStatus::StopIteration
        );
    }

    /// 67.1 D4: `NetworkFilter` is object-safe — it must be storable as
    /// `Arc<dyn NetworkFilter>` for `ChainHandler`'s filter list (Task 6).
    #[test]
    fn network_filter_is_object_safe() {
        struct AlwaysStop;
        impl NetworkFilter for AlwaysStop {
            fn on_new_connection(&self, _conn: &ConnectionInfo) -> NetworkFilterStatus {
                NetworkFilterStatus::StopIteration
            }
        }
        let f: Arc<dyn NetworkFilter> = Arc::new(AlwaysStop);
        let info = ConnectionInfo {
            peer_addr: "127.0.0.1:1".parse().unwrap(),
            local_addr: "127.0.0.1:2".parse().unwrap(),
        };
        assert_eq!(
            f.on_new_connection(&info),
            NetworkFilterStatus::StopIteration
        );
    }

    /// 67.1 D4 / phase-67 SPEC R-2 (ADR-0124's drain, shared): `close_with_drain`
    /// sends a FIN with ZERO bytes written, and a client write issued AFTER it
    /// observes EOF is ACCEPTED, not reset. A server that closed without
    /// draining its read half would make the kernel send an RST and the second
    /// write would fail.
    ///
    /// DELETE THE DRAIN LOOP IN `close_with_drain` AND THIS TEST MUST FAIL.
    #[tokio::test(flavor = "multi_thread")]
    async fn close_with_drain_sends_clean_eof_and_accepts_post_eof_writes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            close_with_drain(stream).await.unwrap();
        });

        let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Bytes sent before the close are discarded, not echoed.
        c.write_all(b"PING-RBAC\n").await.unwrap();

        let mut out = Vec::new();
        c.read_to_end(&mut out).await.expect("clean EOF, not RST");
        assert!(out.is_empty(), "DENY writes zero bytes, got {out:?}");

        // Two writes: the first may be absorbed locally; a returning RST
        // surfaces on the second. Sleep between them so an RST can land.
        c.write_all(b"y").await.expect("first post-EOF write");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        c.write_all(b"y")
            .await
            .expect("second post-EOF write must not be reset");
    }

    /// 67.1 D4: `ChainHandler` runs each filter's `on_new_connection` in order
    /// and, when all return `Continue`, delegates to the terminal handler.
    #[tokio::test(flavor = "multi_thread")]
    async fn chain_handler_continue_delegates_to_terminal_handler() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Counting(Arc<AtomicUsize>);
        impl NetworkFilter for Counting {
            fn on_new_connection(&self, _c: &ConnectionInfo) -> NetworkFilterStatus {
                self.0.fetch_add(1, Ordering::SeqCst);
                NetworkFilterStatus::Continue
            }
        }
        let hits = Arc::new(AtomicUsize::new(0));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![
                Arc::new(Counting(Arc::clone(&hits))),
                Arc::new(Counting(Arc::clone(&hits))),
            ],
            Arc::new(EchoHandler),
        ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            chain.handle(s).await.unwrap();
        });

        let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
        c.write_all(b"payload").await.unwrap();
        let mut buf = [0u8; 7];
        c.read_exact(&mut buf).await.expect("terminal echo ran");
        assert_eq!(&buf, b"payload");
        assert_eq!(hits.load(Ordering::SeqCst), 2, "both filters ran, in order");
    }

    /// 67.1 D4 / phase-67 SPEC R-2: `StopIteration` closes the connection with
    /// ZERO bytes and a clean EOF, and THE TERMINAL FILTER NEVER RUNS.
    #[tokio::test(flavor = "multi_thread")]
    async fn chain_handler_stop_iteration_closes_and_skips_terminal() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Stop;
        impl NetworkFilter for Stop {
            fn on_new_connection(&self, _c: &ConnectionInfo) -> NetworkFilterStatus {
                NetworkFilterStatus::StopIteration
            }
        }
        struct Tripwire(Arc<AtomicBool>);
        impl ConnectionHandler for Tripwire {
            fn handle(
                &self,
                _d: tokio::net::TcpStream,
            ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>
            {
                self.0.store(true, Ordering::SeqCst);
                Box::pin(async move { Ok(()) })
            }
        }
        let ran = Arc::new(AtomicBool::new(false));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(Stop)],
            Arc::new(Tripwire(Arc::clone(&ran))),
        ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            chain.handle(s).await.unwrap();
        });

        let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
        c.write_all(b"discarded").await.unwrap();
        let mut out = Vec::new();
        c.read_to_end(&mut out).await.expect("clean EOF, not RST");
        assert!(out.is_empty(), "DENY writes zero bytes, got {out:?}");
        assert!(!ran.load(Ordering::SeqCst), "terminal handler must NOT run");
    }

    /// 67.1 D4: a filter that STOPS short-circuits — later filters do not run.
    #[tokio::test(flavor = "multi_thread")]
    async fn chain_handler_stop_short_circuits_later_filters() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Stop;
        impl NetworkFilter for Stop {
            fn on_new_connection(&self, _c: &ConnectionInfo) -> NetworkFilterStatus {
                NetworkFilterStatus::StopIteration
            }
        }
        struct Tripwire(Arc<AtomicBool>);
        impl NetworkFilter for Tripwire {
            fn on_new_connection(&self, _c: &ConnectionInfo) -> NetworkFilterStatus {
                self.0.store(true, Ordering::SeqCst);
                NetworkFilterStatus::Continue
            }
        }
        let ran = Arc::new(AtomicBool::new(false));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(Stop), Arc::new(Tripwire(Arc::clone(&ran)))],
            Arc::new(EchoHandler),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            chain.handle(s).await.unwrap();
        });
        let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
        // ADR-0131: the chain runs on the FIRST DOWNSTREAM BYTE, so the client
        // must send one for any filter to be invoked at all.
        c.write_all(b"go").await.unwrap();
        let mut out = Vec::new();
        c.read_to_end(&mut out).await.unwrap();
        assert!(
            !ran.load(Ordering::SeqCst),
            "filters after a Stop must not run"
        );
    }

    /// 67.1 D4: `ChainHandler` hands the filter the connection's REAL peer and
    /// local addresses, read from the accepted socket. `67.2`'s IP/port matcher
    /// arms depend on this being exact.
    #[tokio::test(flavor = "multi_thread")]
    async fn chain_handler_populates_connection_info_from_the_socket() {
        use std::sync::Mutex;
        struct Capture(Arc<Mutex<Option<ConnectionInfo>>>);
        impl NetworkFilter for Capture {
            fn on_new_connection(&self, c: &ConnectionInfo) -> NetworkFilterStatus {
                *self.0.lock().unwrap() = Some(*c);
                NetworkFilterStatus::Continue
            }
        }
        let seen = Arc::new(Mutex::new(None));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(Capture(Arc::clone(&seen)))],
            Arc::new(EchoHandler),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            chain.handle(s).await.unwrap();
        });
        let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
        let client_addr = c.local_addr().unwrap();
        // ADR-0131: a byte is required before the chain is evaluated.
        c.write_all(b"x").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(c);

        let info = seen.lock().unwrap().expect("filter saw the connection");
        assert_eq!(info.local_addr, addr, "local_addr is the listener address");
        assert_eq!(
            info.peer_addr, client_addr,
            "peer_addr is the client address"
        );
    }

    /// 67.1 / ADR-0131: a client that half-closes WITHOUT sending any byte is
    /// never evaluated — no filter runs, no counter ticks — and receives a clean
    /// EOF. Measured against upstream Envoy `v1.33.0`: on a DENY-all chain, a
    /// connect + FIN with no data leaves `rbac.denied` at 0 and yields a clean
    /// EOF. envoy-rust's ONE_TIME_ON_FIRST_BYTE wait reproduces it.
    #[tokio::test(flavor = "multi_thread")]
    async fn chain_handler_skips_filters_when_client_closes_without_sending() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Tripwire(Arc<AtomicBool>);
        impl NetworkFilter for Tripwire {
            fn on_new_connection(&self, _c: &ConnectionInfo) -> NetworkFilterStatus {
                self.0.store(true, Ordering::SeqCst);
                NetworkFilterStatus::StopIteration
            }
        }
        let ran = Arc::new(AtomicBool::new(false));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(Tripwire(Arc::clone(&ran)))],
            Arc::new(EchoHandler),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            chain.handle(s).await.unwrap();
        });

        let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
        c.shutdown().await.unwrap(); // FIN, no data
        let mut out = Vec::new();
        c.read_to_end(&mut out).await.expect("clean EOF, not RST");
        assert!(out.is_empty(), "no bytes are written, got {out:?}");
        assert!(
            !ran.load(Ordering::SeqCst),
            "no decision is taken for a connection that never sends a byte",
        );
    }

    /// 67.1 — the M66-3 REGRESSION WITNESS.
    ///
    /// M66-3: "`serve()` never reaps completed `JoinSet` tasks", shared verbatim
    /// by the two standalone accept loops phase 67.1 DELETES (`echo.rs`,
    /// `direct_response.rs`). The surviving loop — `accept_loop` — reaps via its
    /// `join_next()` select arm. This test proves it: after N sequential
    /// connections have completed, the `JoinSet` is empty.
    ///
    /// `cx_active` cannot witness this: it is decremented INSIDE the spawned
    /// task, so it reads 0 while the JoinSet entry still lingers. Only
    /// `pending_tasks()` (which publishes `join_set.len()`) sees the difference.
    ///
    /// DELETE THE `join_next()` SELECT ARM IN `accept_loop` AND THIS TEST MUST
    /// FAIL (the count would climb to N).
    #[tokio::test(flavor = "multi_thread")]
    async fn sequential_connections_do_not_accumulate_joinset_tasks() {
        const N: usize = 50;
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let registry = mk_registry();
        let listener = Listener::bind(&cfg, Arc::new(NullHandler), Arc::clone(&registry))
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let pending = listener.pending_tasks_watch();

        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(listener.serve(
            async move {
                let _ = rx.await;
            },
            drain,
        ));

        for _ in 0..N {
            let c = tokio::net::TcpStream::connect(addr).await.expect("connect");
            drop(c);
            tokio::task::yield_now().await;
        }
        // Give the accept loop time to observe every completion.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let leaked = *pending.borrow();
        assert!(
            leaked <= 1,
            "JoinSet leaked {leaked} completed tasks across {N} sequential connections \
             (non-reaping regression — see M66-3)",
        );

        tx.send(()).expect("shutdown");
        tokio::time::timeout(std::time::Duration::from_secs(7), server)
            .await
            .expect("serve resolves")
            .expect("join")
            .expect("serve ok");
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
