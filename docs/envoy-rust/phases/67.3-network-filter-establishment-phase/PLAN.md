# Phase 67.3 — Network-filter establishment/data-phase split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split `envoy_listener::ConnectionHandler` into an establishment phase and a data phase so a terminal filter can do its establishment-time work (`tcp_proxy` connects upstream, relays a server-first banner) *before* the network-filter chain's first-byte gate resolves — making **plaintext** `[rbac, tcp_proxy]` behave (banner to a byte-less client; verdict on the first downstream byte *or* a data-less FIN; DENY withholds that byte from the upstream) — and narrow phase 67.1's fail-loud rejection of that composition to TLS-downstream chains only (which the D6 probe showed diverge).

**Architecture:** Add a default `ConnectionHandler::handle_gated(self: Arc<Self>, downstream, gate)` method. `ChainHandler` builds a `FirstByteGate` from its non-terminal filters and hands `(downstream, gate)` to the terminal handler. Handlers with no establishment-time work (`echo`, `http_connection_manager`) inherit the default (await the gate via a non-consuming `peek`, then `handle`) — byte-for-byte unchanged. `TcpProxy` OVERRIDES `handle_gated`: it connects upstream and starts the upstream→downstream relay first, then consults the gate on the first downstream byte *or a data-less FIN*, admitting or denying the downstream→upstream direction. `direct_response` keeps its 67.1 chain bypass. The gate is the reusable, filter-owned first-byte primitive extracted from `ChainHandler`; the `NetworkFilter` trait shape is unchanged and filters still never see payload (CF-67-3 stays deferred).

**Tech Stack:** Rust (pinned `rust-toolchain.toml`), tokio, `#![forbid(unsafe_code)]` per crate. Differential harness unchanged (regression-only). No new crate, dependency, or fuzz target.

## Global Constraints

- **No `unsafe`** — every crate root keeps `#![forbid(unsafe_code)]` (D-3.8).
- **The `NetworkFilter` trait shape MUST NOT change** — `on_new_connection(&ConnectionInfo) -> NetworkFilterStatus`, fired once per connection, no payload. **CF-67-3 stays deferred, scope unchanged** (no `on_data`, no buffering, no `injectReadDataToFilterChain`).
- **`echo` / `http_connection_manager` behavior is byte-for-byte UNCHANGED** — fixtures `0001`, `0072`, `0073` stay green with **zero edits**. Never weaken a fixture; never trim `known-failures.txt`.
- **Never add `rbac` to `is_terminal_network_filter`; never reject `filters: []`; never re-wrap `direct_response` in the chain** (it BYPASSES the chain — ADR-0132 decision 2, measured). Keep the `main.rs` `drop(chain_filters)` bypass at the `DIRECT_RESPONSE_FILTER` arm intact.
- **Never revert ADR-0131** (the RBAC *verdict* is a first-byte event) — ADR-0132 RE-CONFIRMS it. This phase changes only *what else* waits for that byte (nothing, for `echo`/`hcm`; only the downstream→upstream direction, for `tcp_proxy`).
- **Preserve ADR-0016** (`tcp_proxy` half-close: `tokio::select!` over the two copies) and the `cx_active` RAII guard / `cx_total` tick placement exactly.
- **Preserve ADR-0124** — `close_with_drain` and both `post_eof_client_write_is_accepted_not_reset` / `deny_post_eof_client_write_is_accepted_not_reset` tests stay unweakened.
- **Do NOT edit `crates/envoy-filter/src/rbac.rs` toward HTTP-accepts-L4 parity** (deliberate FAIL-LOUD divergence — BEHAVIOR item 14, ADR-0133). Do NOT re-litigate the mapped-prefix parity-accept options rejected by ADR-0134.
- **`cargo build -p envoy-bin` before ANY local backstop/differential run** — the harness + `tests/network_filter_rbac.rs` run `target/debug/envoy-bin`. `envoy-bin` is a BINARY crate — use `--bins`/`--test <name>`, NOT `--lib`.
- **`cargo test --workspace --no-fail-fast`; never pipe a verification run through `tail`. CI is authoritative.** `envoy-bin` writes `ConfigError` to STDOUT.
- **ROADMAP row edits escape literal `|` as `\|`** and preserve 6 cells; rows `36`/`38`/`39`/`52`/`54` are already malformed — do NOT "fix" them.
- **No new fuzz target** — network `rbac` parses nothing. The state-4 session RECORDS §7.5 gate (d) as *"satisfied by the pre-existing `parse_bootstrap` target; no new target."*

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `crates/envoy-listener/src/lib.rs` | The `ConnectionHandler` trait + the new `handle_gated` default method; the `FirstByteGate`/`GateOutcome` primitive; `ChainHandler` (rewired to delegate). | Modify |
| `crates/envoy-tcp/src/lib.rs` | `TcpProxy` refactored into `connect_upstream()` + `relay()`; the `handle_gated` override with the establishment/gate/data interleave; the in-process establishment backstops. | Modify |
| `crates/envoy-config/src/lib.rs` | Re-message `ConfigError::UnsupportedNetworkFilterChainComposition` (now TLS-only, owner CF-67-7). | Modify |
| `crates/envoy-config/src/bootstrap.rs` | Narrow the `[non-terminal, tcp_proxy]` rejection to TLS-downstream chains; update its unit tests (plaintext-accept, TLS-reject). | Modify |
| `crates/envoy-bin/tests/network_filter_rbac.rs` | Replace `rbac_before_tcp_proxy_is_rejected_at_config_load` with the plaintext-accept + TLS-reject pair; add the `[rbac, tcp_proxy]` establishment backstops (DENY byte-not-forwarded; FIN matrix). | Modify |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | Item 13's `tcp_proxy` row: plaintext → parity; TLS → recorded fail-loud divergence + CF-67-7. | Modify |

No new files. No new crate/dependency/fuzz target. `crates/envoy-bin/src/tls_handler.rs` is **untouched** — once TLS `[rbac, tcp_proxy]` is rejected at config load, `ChainHandler` never wraps `TlsAcceptingHandler`, so its (inherited) `handle_gated` is never invoked (D6 decision, ADR-0135).

---

## Interfaces (names later tasks rely on)

- `envoy_listener::GateOutcome` — `#[derive(Debug, Clone, Copy, PartialEq, Eq)] enum { ClientGoneEarly, SkippedCleanly, Admitted, Denied }`.
- `envoy_listener::FirstByteGate`:
  - `pub fn new(filters: Arc<[Arc<dyn NetworkFilter>]>) -> Self`
  - `pub async fn evaluate_peek(&self, s: &tokio::net::TcpStream, conn: &ConnectionInfo) -> std::io::Result<GateOutcome>`
  - `pub async fn evaluate_read_half(&self, r: &mut tokio::net::tcp::OwnedReadHalf, conn: &ConnectionInfo) -> std::io::Result<(GateOutcome, Option<u8>)>`
- `envoy_listener::ConnectionHandler::handle_gated(self: Arc<Self>, downstream: tokio::net::TcpStream, gate: FirstByteGate) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>` — dyn-safe default method (uses an `Arc<Self>` receiver so the returned future is `'static` without the trait knowing which fields to clone).
- `envoy_tcp::TcpProxy::connect_upstream(&self) -> Result<UpstreamConn, Box<dyn std::error::Error + Send + Sync>>`, where `struct UpstreamConn { stream: Box<dyn AsyncReadWrite + Send + Unpin>, _cx_guard: envoy_cluster::ConnGaugeGuard, addr: SocketAddr, cluster_name: String }`; and `TcpProxy::relay(&self, downstream, up)` (the plain bidirectional copy) + `TcpProxy::relay_gated(&self, downstream, up, gate, conn)` (the gated variant).

---

## Task 1 — The `FirstByteGate` primitive, the `handle_gated` default method, and the `ChainHandler` rewire (D1 core + D2)

**Files:**
- Modify: `crates/envoy-listener/src/lib.rs` (trait at `:38-43`; `ChainHandler` at `:170-240`; `close_with_drain` at `:106-119`)
- Test: `crates/envoy-listener/src/lib.rs` (`#[cfg(test)]` module — the existing chain tests around `:1100+`)

**Interfaces:**
- Produces: `GateOutcome`, `FirstByteGate`, `ConnectionHandler::handle_gated`.
- Consumes: `NetworkFilter`, `NetworkFilterStatus`, `ConnectionInfo`, `close_with_drain` (all existing).

- [ ] **Step 1: Write a failing test** that the gate returns `Denied` on the first `StopIteration` and `Admitted` when all filters `Continue`:

```rust
#[test]
fn gate_admits_when_all_continue_and_denies_on_first_stop() {
    use std::sync::Arc;
    struct Yes;
    impl NetworkFilter for Yes { fn on_new_connection(&self, _: &ConnectionInfo) -> NetworkFilterStatus { NetworkFilterStatus::Continue } }
    struct No;
    impl NetworkFilter for No { fn on_new_connection(&self, _: &ConnectionInfo) -> NetworkFilterStatus { NetworkFilterStatus::StopIteration } }
    let conn = ConnectionInfo { peer_addr: "10.0.0.1:1".parse().unwrap(), local_addr: "127.0.0.1:2".parse().unwrap() };
    let admit = FirstByteGate::new((vec![Arc::new(Yes) as Arc<dyn NetworkFilter>]).into());
    assert_eq!(admit.run_for_test(&conn), GateOutcome::Admitted);
    let deny = FirstByteGate::new((vec![Arc::new(Yes) as Arc<dyn NetworkFilter>, Arc::new(No)]).into());
    assert_eq!(deny.run_for_test(&conn), GateOutcome::Denied);
}
```

- [ ] **Step 2: Run it, confirm it fails to compile** (`FirstByteGate`/`GateOutcome` undefined).

Run: `cargo test -p envoy-listener gate_admits --no-run`
Expected: FAIL — `cannot find type FirstByteGate`.

- [ ] **Step 3: Add `GateOutcome` + `FirstByteGate`** just below `NetworkFilter` (after `:92`):

```rust
/// The verdict of the network-filter chain's first-byte gate (67.3 D2).
///
/// Extracted from `ChainHandler` so the gate is a reusable, filter-owned
/// primitive that a terminal handler can consult AFTER its establishment-time
/// work (upstream connect, banner relay), not only before the chain's hand-off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateOutcome {
    /// The client went away (reset/error) before its first byte. Drop the
    /// socket — it is already unusable — with no drain and no counter tick.
    ClientGoneEarly,
    /// `Ok(0)`: the client closed or half-closed WITHOUT sending. For a handler
    /// with no establishment-time work this is NOT an evaluation event
    /// (ADR-0132 / ADR-0131 case C): close cleanly, tick nothing.
    SkippedCleanly,
    /// Every non-terminal filter returned `Continue`.
    Admitted,
    /// A non-terminal filter returned `StopIteration`.
    Denied,
}

/// The reusable first-byte gate (67.3 D2). Owns the chain's NON-TERMINAL filters
/// and runs each one's `on_new_connection` once the first-byte event is observed.
/// The `NetworkFilter` trait shape is UNCHANGED and no payload is ever exposed:
/// `evaluate_peek` does not consume; `evaluate_read_half` reads exactly one byte
/// and returns it for RE-INJECTION. CF-67-3 (payload-visible iteration) stays
/// deferred.
pub struct FirstByteGate {
    filters: Arc<[Arc<dyn NetworkFilter>]>,
}

impl FirstByteGate {
    pub fn new(filters: Arc<[Arc<dyn NetworkFilter>]>) -> Self {
        Self { filters }
    }

    /// The shared core: run every filter in order; the first `StopIteration`
    /// denies. No I/O.
    fn run(&self, conn: &ConnectionInfo) -> GateOutcome {
        for filter in self.filters.iter() {
            if filter.on_new_connection(conn) == NetworkFilterStatus::StopIteration {
                return GateOutcome::Denied;
            }
        }
        GateOutcome::Admitted
    }

    /// Test-only shim over [`run`], so a unit test needs no live socket.
    #[cfg(test)]
    pub fn run_for_test(&self, conn: &ConnectionInfo) -> GateOutcome {
        self.run(conn)
    }

    /// Non-consuming first-byte gate for a handler with NO establishment-time
    /// work (`echo`, `http_connection_manager`, and the trait default). Waits
    /// for the first downstream byte via `peek` (ADR-0131). `Ok(0)` (byte-less
    /// close / data-less FIN) => `SkippedCleanly` — these handlers do NOT
    /// evaluate on a data-less FIN (D3, measured). `peek` Err => `ClientGoneEarly`.
    pub async fn evaluate_peek(
        &self,
        s: &tokio::net::TcpStream,
        conn: &ConnectionInfo,
    ) -> std::io::Result<GateOutcome> {
        let mut b = [0u8; 1];
        match s.peek(&mut b).await {
            Ok(0) => Ok(GateOutcome::SkippedCleanly),
            Ok(_) => Ok(self.run(conn)),
            Err(_) => Ok(GateOutcome::ClientGoneEarly),
        }
    }

    /// Consuming first-byte gate for a handler already in its data phase on a
    /// SPLIT stream (`tcp_proxy`). Reads exactly ONE byte from the read half.
    /// A data-less FIN (`Ok(0)`) STILL evaluates the chain for such a handler
    /// (D3, measured — downstream half-close propagation), returned as byte
    /// `None`. A real byte is returned as `Some(b)` for RE-INJECTION into the
    /// upstream copy (`peek` is unavailable on `OwnedReadHalf`). Read Err =>
    /// `ClientGoneEarly`.
    pub async fn evaluate_read_half(
        &self,
        r: &mut tokio::net::tcp::OwnedReadHalf,
        conn: &ConnectionInfo,
    ) -> std::io::Result<(GateOutcome, Option<u8>)> {
        use tokio::io::AsyncReadExt;
        let mut b = [0u8; 1];
        match r.read(&mut b).await {
            Ok(0) => Ok((self.run(conn), None)),
            Ok(_) => Ok((self.run(conn), Some(b[0]))),
            Err(_) => Ok((GateOutcome::ClientGoneEarly, None)),
        }
    }
}
```

- [ ] **Step 4: Add the `handle_gated` default method** to the `ConnectionHandler` trait (`:38-43`), keeping the existing `handle` signature:

```rust
pub trait ConnectionHandler: Send + Sync + 'static {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>;

    /// 67.3 D1: the establishment/data-phase-aware entry point. `ChainHandler`
    /// hands the connection here WITH a [`FirstByteGate`]. The DEFAULT — used by
    /// every handler with no establishment-time work (`echo`,
    /// `http_connection_manager`) — awaits the gate via a non-consuming `peek`
    /// and then delegates to `handle`, which is observationally identical to the
    /// pre-67.3 `ChainHandler` (ADR-0132 decision 1: for these handlers the
    /// chain's first-byte gate and the filter's verdict coincide).
    ///
    /// A handler that does establishment-time work (`tcp_proxy`) OVERRIDES this
    /// to run that work BEFORE awaiting the gate.
    ///
    /// The receiver is `self: Arc<Self>` (not `&self`) so the returned future is
    /// `'static` without the trait knowing which fields to clone — the same
    /// reason the concrete impls hand-clone their `Arc` fields before `Box::pin`.
    fn handle_gated(
        self: Arc<Self>,
        downstream: tokio::net::TcpStream,
        gate: FirstByteGate,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        Box::pin(async move {
            let conn = ConnectionInfo {
                peer_addr: downstream.peer_addr()?,
                local_addr: downstream.local_addr()?,
            };
            match gate.evaluate_peek(&downstream, &conn).await? {
                GateOutcome::Admitted => self.handle(downstream).await,
                GateOutcome::SkippedCleanly | GateOutcome::Denied => {
                    close_with_drain(downstream).await?;
                    Ok(())
                }
                GateOutcome::ClientGoneEarly => {
                    tracing::debug!("client went away before its first byte");
                    Ok(())
                }
            }
        })
    }
}
```

(`use std::sync::Arc;` is already imported at `:15`.)

- [ ] **Step 5: Rewire `ChainHandler::handle`** (`:187-240`) to build a gate and delegate. Replace the whole `impl ConnectionHandler for ChainHandler` body with:

```rust
impl ConnectionHandler for ChainHandler {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        // 67.3 D1: the chain now GATES THE FILTER'S DECISION, not the chain's
        // hand-off. Build the first-byte gate from the non-terminal filters and
        // hand it, with the connection, to the terminal handler's `handle_gated`.
        // For `echo`/`http_connection_manager` the default `handle_gated` peeks
        // exactly as this method used to — byte-for-byte unchanged (fixtures
        // `0072`/`0073`). For `tcp_proxy` the override connects upstream first.
        let gate = FirstByteGate::new(Arc::clone(&self.filters));
        let inner = Arc::clone(&self.inner);
        Box::pin(async move { inner.handle_gated(downstream, gate).await })
    }
}
```

Delete the old peek/loop body — it now lives in `FirstByteGate::evaluate_peek` + the default `handle_gated`. Update the `ChainHandler` doc comment (`:143-160`) to describe the gate delegation; leave the plaintext-parity statement for Task 4/BEHAVIOR.

- [ ] **Step 6: Run the listener test suite.** The behavioral tests (`chain_handler_skips_filters_when_client_closes_without_sending`, the continue/stop tests) must still pass unchanged — they exercise the same observable behavior through the new delegation. Adjust ONLY tests that reached into `ChainHandler`'s internal peek directly, if any.

Run: `cargo test -p envoy-listener --lib --no-fail-fast`
Expected: PASS — including the new `gate_admits_when_all_continue_and_denies_on_first_stop`.

- [ ] **Step 7: Full build + fixture regression** (echo/hcm path must be byte-for-byte unchanged).

Run: `cargo build -p envoy-bin && cargo test -p envoy-listener --no-fail-fast`
Expected: PASS.

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-listener/src/lib.rs
git commit -m "67.3 D2/D1: extract FirstByteGate + handle_gated default; ChainHandler delegates"
```

---

## Task 2 — Refactor `TcpProxy::handle::<S>` into `connect_upstream()` + `relay()` (D4, no behavior change)

**Files:**
- Modify: `crates/envoy-tcp/src/lib.rs` (`handle::<S>` at `:75-150`)
- Test: `crates/envoy-tcp/src/lib.rs` (the existing `#[cfg(test)]` tests — they are the regression guard)

**Interfaces:**
- Produces: `TcpProxy::connect_upstream(&self) -> Result<UpstreamConn, ...>`, `TcpProxy::relay<D>(&self, downstream: D, up: UpstreamConn)`, `struct UpstreamConn`.
- Consumes: `ClusterHandle::pick_endpoint/cx_active_guard/cx_total` (existing).

- [ ] **Step 1: Confirm the existing tests are green first** (this task must not change behavior).

Run: `cargo test -p envoy-tcp --no-fail-fast`
Expected: PASS (the 10 existing tests).

- [ ] **Step 2: Add `UpstreamConn` and split `handle::<S>`.** Replace the body of `handle::<S>` (`:75-150`) so its establishment half becomes `connect_upstream` and its data half becomes `relay`:

```rust
/// The established upstream side of a `tcp_proxy` connection: the (possibly
/// TLS-wrapped) stream, the RAII `cx_active` guard (held until relay ends), and
/// diagnostics. Produced by [`TcpProxy::connect_upstream`] at ESTABLISHMENT.
pub struct UpstreamConn {
    stream: Box<dyn AsyncReadWrite + Send + Unpin>,
    _cx_guard: envoy_cluster::ConnGaugeGuard,
    addr: SocketAddr,
    cluster_name: String,
}

impl TcpProxy {
    /// 67.3 D4: the ESTABLISHMENT half — pick an endpoint, hold the `cx_active`
    /// guard, TCP-connect, tick `cluster.<name>.upstream_cx_total`, and (if
    /// configured) run the upstream rustls handshake. Preserves ADR-0016 posture
    /// and the exact 06.x guard/tick placement. Returns the upstream side so the
    /// caller can interpose the first-byte gate before relaying (67.3 D1).
    pub async fn connect_upstream(
        &self,
    ) -> Result<UpstreamConn, Box<dyn std::error::Error + Send + Sync>> {
        let pick = self.cluster.pick_endpoint(None, None);
        let cluster_name = self.cluster_name.clone();
        let addr = pick.ok_or_else(|| {
            Box::new(TcpProxyError::NoHealthyEndpoint { cluster: cluster_name.clone() })
                as Box<dyn std::error::Error + Send + Sync>
        })?;
        let _cx_guard = self.cluster.cx_active_guard();
        let stream = tokio::net::TcpStream::connect(addr).await.map_err(|source| {
            Box::new(TcpProxyError::UpstreamConnect { addr, source })
                as Box<dyn std::error::Error + Send + Sync>
        })?;
        self.cluster.cx_total().inc();
        let stream: Box<dyn AsyncReadWrite + Send + Unpin> = match &self.upstream_tls {
            None => Box::new(stream),
            Some(tls) => {
                let tls_stream = tls.connect(stream).await.map_err(|source| {
                    Box::new(TcpProxyError::UpstreamTlsHandshake { source })
                        as Box<dyn std::error::Error + Send + Sync>
                })?;
                Box::new(tls_stream)
            }
        };
        Ok(UpstreamConn { stream, _cx_guard, addr, cluster_name })
    }

    /// 67.3 D4: the DATA half — the ADR-0016 bidirectional half-close copy.
    /// Unchanged from the old `handle::<S>` tail; `up` carries the `cx_active`
    /// guard, dropped when this returns.
    pub async fn relay<D>(
        &self,
        downstream: D,
        up: UpstreamConn,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        D: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let UpstreamConn { stream: upstream, _cx_guard, addr, cluster_name } = up;
        let (mut dr, mut dw) = tokio::io::split(downstream);
        let (mut ur, mut uw) = tokio::io::split(upstream);
        let result: Result<(), std::io::Error> = tokio::select! {
            res = tokio::io::copy(&mut dr, &mut uw) => res.map(|_| ()),
            res = tokio::io::copy(&mut ur, &mut dw) => res.map(|_| ()),
        };
        drop((dr, dw, ur, uw));
        result.map_err(|source| {
            Box::new(TcpProxyError::CopyFailed { source }) as Box<dyn std::error::Error + Send + Sync>
        })?;
        tracing::debug!(%addr, cluster = %cluster_name, "tcp proxy connection complete");
        Ok(())
    }

    /// 67.3 D4: `handle::<S>` now composes the two halves. Behavior is identical
    /// to the pre-67.3 straight-line body (the regression tests prove it).
    pub async fn handle<S>(&self, downstream: S) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let up = self.connect_upstream().await?;
        self.relay(downstream, up).await
    }
}
```

> Verify `envoy_cluster::ConnGaugeGuard` is the concrete type returned by `cx_active_guard()` (source: `crates/envoy-cluster/src/cluster.rs:816`) and is nameable from `envoy-tcp`. If it is not `pub`/nameable, box it as `_cx_guard: Box<dyn std::any::Any + Send>` or a small newtype — but do NOT change its Drop semantics (it decrements `upstream_cx_active`).

- [ ] **Step 3: Build + run the regression tests** (behavior must be identical).

Run: `cargo test -p envoy-tcp --no-fail-fast`
Expected: PASS (all 10 pre-existing tests: `proxies_payload_end_to_end`, `proxies_closes_*`, the TLS-downstream + upstream-TLS trio, etc.).

- [ ] **Step 4: Commit.**

```bash
git add crates/envoy-tcp/src/lib.rs
git commit -m "67.3 D4: split TcpProxy::handle into connect_upstream() + relay()"
```

---

## Task 3 — `TcpProxy::handle_gated` override: establishment → gate → data, with the in-process C-1 witnesses (D1 + D3 + D4)

**Files:**
- Modify: `crates/envoy-tcp/src/lib.rs` (`impl ConnectionHandler for TcpProxy` at `:178-204`)
- Test: `crates/envoy-tcp/src/lib.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Consumes: `FirstByteGate`, `GateOutcome`, `ConnectionInfo`, `close_with_drain`, `NetworkFilter`, `ChainHandler` (from `envoy_listener`); `connect_upstream`/`relay` (Task 2).
- Produces: `TcpProxy::handle_gated` override; `TcpProxy::relay_gated`.

> **§6.1 MID-EXECUTION VALVE IS ARMED FOR THIS TASK.** `relay_gated` interleaves a concurrent banner relay with the gate read, then branches on the verdict (admit-with-byte / admit-with-FIN / deny / client-gone). If, on contact with reality, its sub-steps blow past ~10 items (e.g. the "upstream EOFs before the client's first byte" race, or the `reunite` close path forces more structure than below), **STOP and split per §6.2 (ADR-0132's own precedent)** — do not cram it into a vague task.

- [ ] **Step 1: Write the failing C-1 witness** — a server-speaks-first backend's banner must reach a client that has sent NOTHING, through `[rbac(ALLOW), tcp_proxy]`. Add a banner backend helper and the test to the `#[cfg(test)]` module:

```rust
/// Spawn an in-process server-FIRST backend: writes `220 BANNER\r\n` immediately
/// on accept, then records every byte it subsequently receives. Returns its
/// address and the recording handle.
async fn spawn_banner_backend() -> (SocketAddr, Arc<tokio::sync::Mutex<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().unwrap();
    let recorded = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let rec = recorded.clone();
    tokio::spawn(async move {
        if let Ok((mut s, _)) = listener.accept().await {
            s.write_all(b"220 BANNER\r\n").await.ok();
            s.flush().await.ok();
            let mut buf = [0u8; 1024];
            loop {
                match s.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => rec.lock().await.extend_from_slice(&buf[..n]),
                }
            }
        }
    });
    (addr, recorded)
}

/// 67.3 D7 / C-1 REGRESSION WITNESS. `[rbac(ALLOW, any), tcp_proxy]` over a
/// server-first backend: the banner must reach a client that has SENT NOTHING.
/// Fails against the post-Task-2 code (the default `handle_gated` peeks, so
/// tcp_proxy never connects upstream and the banner never arrives).
#[tokio::test(flavor = "multi_thread")]
async fn banner_reaches_a_client_that_sends_nothing_through_rbac_allow() {
    use envoy_listener::{ChainHandler, ConnectionHandler, ConnectionInfo, NetworkFilter, NetworkFilterStatus};
    struct AllowAll;
    impl NetworkFilter for AllowAll { fn on_new_connection(&self, _: &ConnectionInfo) -> NetworkFilterStatus { NetworkFilterStatus::Continue } }

    let (backend_addr, _rec) = spawn_banner_backend().await;
    let handle = mk_handle("backend", backend_addr).await;
    let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));
    let chain: Arc<dyn ConnectionHandler> =
        Arc::new(ChainHandler::new(vec![Arc::new(AllowAll) as Arc<dyn NetworkFilter>], proxy));

    let dl = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let daddr = dl.local_addr().unwrap();
    let task = tokio::spawn(async move { let (s, _) = dl.accept().await.unwrap(); chain.handle(s).await });

    let mut client = TcpStream::connect(daddr).await.unwrap();
    // Client sends NOTHING. It must still receive the banner.
    let mut buf = [0u8; 12];
    tokio::time::timeout(std::time::Duration::from_secs(3), client.read_exact(&mut buf))
        .await
        .expect("banner must reach a byte-less client")
        .expect("read banner");
    assert_eq!(&buf, b"220 BANNER\r\n");
    drop(client);
    let _ = task.await;
}
```

- [ ] **Step 2: Run it, confirm it times out→fails** against the post-Task-2 code (tcp_proxy still uses the default `handle_gated` = peek, which blocks for a byte-less client).

Run: `cargo test -p envoy-tcp banner_reaches_a_client_that_sends_nothing --no-fail-fast`
Expected: FAIL — the 3s timeout fires ("banner must reach a byte-less client").

- [ ] **Step 3: Implement `TcpProxy::handle_gated` + `relay_gated`.** Add the override to `impl ConnectionHandler for TcpProxy` and a `relay_gated` inherent method:

```rust
impl ConnectionHandler for TcpProxy {
    fn handle(&self, downstream: tokio::net::TcpStream) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        // Unchanged — the lone-`tcp_proxy` (no chain) path.
        let cluster = self.cluster.clone();
        let cluster_name = self.cluster_name.clone();
        let upstream_tls = self.upstream_tls.clone();
        Box::pin(async move {
            let proxy = TcpProxy { cluster, cluster_name, upstream_tls };
            proxy.handle::<tokio::net::TcpStream>(downstream).await
        })
    }

    /// 67.3 D1/D3/D4: connect upstream at ESTABLISHMENT (before any downstream
    /// byte — banner reaches a byte-less client), then gate the DOWNSTREAM→UPSTREAM
    /// direction on the first byte OR a data-less FIN.
    fn handle_gated(
        self: Arc<Self>,
        downstream: tokio::net::TcpStream,
        gate: envoy_listener::FirstByteGate,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        Box::pin(async move {
            let conn = envoy_listener::ConnectionInfo {
                peer_addr: downstream.peer_addr()?,
                local_addr: downstream.local_addr()?,
            };
            let up = self.connect_upstream().await?; // establishment: cx_total ticks here
            self.relay_gated(downstream, up, gate, conn).await
        })
    }
}

impl TcpProxy {
    /// The gated relay (67.3 D3). Runs the upstream→downstream banner concurrently
    /// with the first-byte gate on the downstream read half; then:
    ///  - `Admitted(Some(b))`: re-inject `b` upstream, then the ADR-0016 select copy;
    ///  - `Admitted(None)` (data-less FIN): propagate FIN upstream, drain upstream→downstream;
    ///  - `Denied`: close downstream WITHOUT forwarding the byte upstream (R-2, W-4);
    ///  - `ClientGoneEarly`: drop.
    async fn relay_gated(
        &self,
        downstream: tokio::net::TcpStream,
        up: UpstreamConn,
        gate: envoy_listener::FirstByteGate,
        conn: envoy_listener::ConnectionInfo,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::io::AsyncWriteExt;
        let UpstreamConn { stream: upstream, _cx_guard, addr, cluster_name } = up;
        let (mut dr, mut dw) = downstream.into_split();  // OwnedReadHalf/OwnedWriteHalf: reunitable
        let (mut ur, mut uw) = tokio::io::split(upstream);

        // Phase 1: banner (upstream→downstream) runs WHILE we await the first
        // downstream byte / FIN. The gate reads `dr`; the banner writes `dw`;
        // disjoint halves, so no contention.
        let (outcome, first) = {
            let mut banner = std::pin::pin!(tokio::io::copy(&mut ur, &mut dw));
            let mut gate_fut = std::pin::pin!(gate.evaluate_read_half(&mut dr, &conn));
            tokio::select! {
                biased;
                g = &mut gate_fut => g?,
                r = &mut banner => {
                    // Upstream reached EOF before the client's first byte. ADR-0016:
                    // the connection closes. `dw` is dropped at scope end → client
                    // sees FIN. Finish resolving the gate, then fall through.
                    r.map_err(|source| Box::new(TcpProxyError::CopyFailed { source })
                        as Box<dyn std::error::Error + Send + Sync>)?;
                    gate_fut.await?
                }
            }
        };

        match outcome {
            envoy_listener::GateOutcome::ClientGoneEarly => Ok(()),
            envoy_listener::GateOutcome::SkippedCleanly | envoy_listener::GateOutcome::Denied => {
                // W-4 / R-2: on Denied the first byte MUST NOT reach the upstream.
                // Drop the upstream (guard fires), reunite the downstream halves,
                // and close it cleanly (zero bytes, clean EOF — close_with_drain).
                drop((ur, uw));
                let ds = dr.reunite(dw).map_err(|e| Box::new(std::io::Error::other(e))
                    as Box<dyn std::error::Error + Send + Sync>)?;
                envoy_listener::close_with_drain(ds).await?;
                Ok(())
            }
            envoy_listener::GateOutcome::Admitted => {
                match first {
                    Some(b) => {
                        // Re-inject the peeked byte, then the ADR-0016 select copy.
                        uw.write_all(&[b]).await.map_err(|source| Box::new(TcpProxyError::CopyFailed { source })
                            as Box<dyn std::error::Error + Send + Sync>)?;
                        let result: Result<(), std::io::Error> = tokio::select! {
                            res = tokio::io::copy(&mut dr, &mut uw) => res.map(|_| ()),
                            res = tokio::io::copy(&mut ur, &mut dw) => res.map(|_| ()),
                        };
                        drop((dr, dw, ur, uw));
                        result.map_err(|source| Box::new(TcpProxyError::CopyFailed { source })
                            as Box<dyn std::error::Error + Send + Sync>)?;
                    }
                    None => {
                        // Data-less FIN, ALLOW: propagate FIN upstream, drain the
                        // upstream→downstream direction to EOF.
                        uw.shutdown().await.ok();
                        let _ = tokio::io::copy(&mut ur, &mut dw).await;
                        drop((dr, dw, ur, uw));
                    }
                }
                tracing::debug!(%addr, cluster = %cluster_name, "tcp proxy gated connection complete");
                Ok(())
            }
        }
    }
}
```

> `downstream.into_split()` yields `OwnedReadHalf`/`OwnedWriteHalf`, which `evaluate_read_half` needs (`&mut OwnedReadHalf`) and which `reunite` can rejoin for `close_with_drain`. `close_with_drain`, `GateOutcome`, `FirstByteGate`, `ConnectionInfo` are all `pub` in `envoy_listener`. `std::io::Error::other` needs Rust ≥1.74 (the pinned toolchain is newer — verify against `rust-toolchain.toml`; if not, use `std::io::Error::new(ErrorKind::Other, e)`).

- [ ] **Step 4: Run the witness — it passes now.**

Run: `cargo test -p envoy-tcp banner_reaches_a_client_that_sends_nothing --no-fail-fast`
Expected: PASS.

- [ ] **Step 5: Add the DENY + FIN in-process witnesses** (envoy-tcp, still config-free): (a) `[rbac(DENY), tcp_proxy]` — banner delivered, then the client's first byte closes the connection and the backend NEVER records that byte; (b) a data-less FIN through `[rbac(ALLOW), tcp_proxy]` reaches the backend as EOF (upstream connected, FIN propagated). Use the `recorded` handle to assert the byte was withheld on DENY:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn deny_delivers_banner_then_closes_without_forwarding_the_byte() {
    use envoy_listener::{ChainHandler, ConnectionHandler, ConnectionInfo, NetworkFilter, NetworkFilterStatus};
    struct DenyAll;
    impl NetworkFilter for DenyAll { fn on_new_connection(&self, _: &ConnectionInfo) -> NetworkFilterStatus { NetworkFilterStatus::StopIteration } }
    let (backend_addr, rec) = spawn_banner_backend().await;
    let handle = mk_handle("backend", backend_addr).await;
    let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));
    let chain: Arc<dyn ConnectionHandler> =
        Arc::new(ChainHandler::new(vec![Arc::new(DenyAll) as Arc<dyn NetworkFilter>], proxy));
    let dl = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let daddr = dl.local_addr().unwrap();
    let task = tokio::spawn(async move { let (s,_) = dl.accept().await.unwrap(); chain.handle(s).await });

    let mut client = TcpStream::connect(daddr).await.unwrap();
    // Banner still delivered (upstream connected at establishment).
    let mut buf = [0u8; 12];
    tokio::time::timeout(std::time::Duration::from_secs(3), client.read_exact(&mut buf)).await.unwrap().unwrap();
    assert_eq!(&buf, b"220 BANNER\r\n");
    // Now send the first byte → DENY closes; the byte must not reach the backend.
    client.write_all(b"Z").await.ok();
    let mut tail = Vec::new();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), client.read_to_end(&mut tail)).await;
    drop(client);
    let _ = task.await;
    assert!(!rec.lock().await.contains(&b'Z'), "DENY must NOT forward the first byte upstream");
}
```

- [ ] **Step 6: Run + build.**

Run: `cargo build -p envoy-bin && cargo test -p envoy-tcp --no-fail-fast`
Expected: PASS (existing 10 + the new witnesses).

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-tcp/src/lib.rs
git commit -m "67.3 D1/D3: TcpProxy::handle_gated — establishment-then-gate; banner+DENY+FIN witnesses"
```

---

## Task 4 — Narrow the config-load rejection to TLS-downstream chains; re-message the variant (D5 + D6 decision)

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (rejection block `:3228-3241`; unit tests `:5853-5904`; helper `:5807`)
- Modify: `crates/envoy-config/src/lib.rs` (variant + `#[error]` doc `:130-150`)

**Interfaces:**
- Consumes: `FilterChain.transport_socket` (`bootstrap.rs:646`), `crate::TCP_PROXY_FILTER`.

- [ ] **Step 1: Write the failing tests** — plaintext `[rbac, tcp_proxy]` now VALIDATES; the TLS variant is still REJECTED. Replace `rejects_rbac_composed_with_tcp_proxy` (`:5853-5874`) with:

```rust
/// 67.3 D5/D6 (ADR-0135): the plaintext `[rbac, tcp_proxy]` composition is now
/// SUPPORTED (the establishment/data split). It must VALIDATE.
#[test]
fn plaintext_rbac_before_tcp_proxy_is_now_accepted() {
    let yaml = chain_before_tcp_proxy_yaml(RBAC_FILTER_YAML);
    let mut b: crate::Bootstrap = serde_yaml::from_str(&yaml).expect("parses");
    validate(&mut b).expect("plaintext [rbac, tcp_proxy] is supported from 67.3");
}

/// 67.3 D6 (ADR-0135, MEASURED): on a TLS-DOWNSTREAM listener upstream Envoy
/// establishes the upstream at raw-TCP accept and defers the RBAC verdict to the
/// first DECRYPTED byte — an ordering envoy-rust's TLS handler does not yet
/// reproduce. That composition stays fail-loud (owner CF-67-7).
#[test]
fn tls_rbac_before_tcp_proxy_is_still_rejected() {
    let yaml = chain_before_tcp_proxy_yaml_tls(RBAC_FILTER_YAML);
    let mut b: crate::Bootstrap = serde_yaml::from_str(&yaml).expect("parses");
    let err = validate(&mut b).expect_err("TLS [rbac, tcp_proxy] stays rejected until CF-67-7");
    assert!(matches!(err, crate::ConfigError::UnsupportedNetworkFilterChainComposition { .. }), "got {err:?}");
    assert!(err.to_string().contains("CF-67-7"), "message must name its owner; got {err}");
}
```

Add a `chain_before_tcp_proxy_yaml_tls` helper next to `chain_before_tcp_proxy_yaml` (`:5807`) that injects a `transport_socket` onto the single filter_chain:

```rust
fn chain_before_tcp_proxy_yaml_tls(prefix_filters: &str) -> String {
    // Reuse the plaintext builder, then splice a DownstreamTlsContext transport_socket
    // into the single filter_chain. The composition rejection fires in the per-chain
    // filter scan, so a minimal context is enough to reach it.
    let base = chain_before_tcp_proxy_yaml(prefix_filters);
    base.replace(
        "      filter_chains:\n        - filters:",
        "      filter_chains:\n        - transport_socket:\n            name: envoy.transport_sockets.tls\n            typed_config:\n              \"@type\": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext\n              common_tls_context: {}\n          filters:",
    )
}
```

> Verify the spliced literal matches `chain_before_tcp_proxy_yaml`'s exact indentation, and that the composition check (`:3228`) runs BEFORE the transport_socket cert validation (`:3131`/`:3375`). If cert validation fires first, either give the context a minimal valid inline cert or assert the error is specifically the composition variant. Adjust if `DownstreamTlsContext` requires a non-empty `common_tls_context`.

- [ ] **Step 2: Run, confirm both fail** (plaintext still rejected today; TLS message lacks "CF-67-7").

Run: `cargo test -p envoy-config plaintext_rbac_before_tcp_proxy tls_rbac_before_tcp_proxy --no-fail-fast`
Expected: FAIL.

- [ ] **Step 3: Narrow the rejection block** (`bootstrap.rs:3228-3241`) to TLS-downstream chains and rewrite the comment (`:3209-3227`):

```rust
            // 67.3 D5/D6 (ADR-0135): the plaintext `[non-terminal, tcp_proxy]`
            // composition is now SUPPORTED (the establishment/data split — a
            // terminal filter connects upstream and relays a server-first banner
            // BEFORE the chain's first-byte gate). Only the TLS-DOWNSTREAM case
            // stays fail-loud: the D6 probe measured that upstream Envoy
            // establishes the upstream at raw-TCP accept (before the handshake) and
            // takes the RBAC verdict on the first DECRYPTED byte — an ordering
            // envoy-rust's TLS handler does not yet reproduce. Owner: CF-67-7.
            //
            // Placed AFTER the terminal-position checks above so their errors keep
            // winning: `[echo, rbac, tcp_proxy]` still reports terminal-not-last.
            if chain_len >= 2
                && let Some(last) = chain.filters.last()
                && last.name == crate::TCP_PROXY_FILTER
                && chain.transport_socket.is_some()
            {
                let non_terminal = &chain.filters[chain_len - 2];
                return Err(crate::ConfigError::UnsupportedNetworkFilterChainComposition {
                    listener: listener.name.clone(),
                    chain_index,
                    non_terminal: non_terminal.name.clone(),
                    terminal: last.name.clone(),
                });
            }
```

- [ ] **Step 4: Re-message the variant** (`lib.rs:130-150`) — update the doc + `#[error]` to name the TLS establishment ordering and CF-67-7:

```rust
    /// A NON-TERMINAL filter (e.g. network `rbac`) precedes `tcp_proxy` on a
    /// **TLS-downstream** filter chain. The plaintext form is SUPPORTED from
    /// phase 67.3 (the establishment/data split); the TLS form stays fail-loud
    /// because upstream Envoy establishes the upstream at raw-TCP accept (before
    /// the handshake) and takes the RBAC verdict on the first DECRYPTED byte
    /// (MEASURED, ADR-0135), which envoy-rust's TLS handler does not yet
    /// reproduce. Owner: **CF-67-7**. A deliberate FAIL-LOUD divergence
    /// (`ADR-0049` decision-2 (b)); upstream Envoy accepts the config.
    #[error(
        "listener {listener:?} filter_chains[{chain_index}]: non-terminal filter {non_terminal:?} \
         before terminal filter {terminal:?} on a TLS-downstream chain is not yet supported — \
         upstream Envoy establishes the upstream at raw-TCP accept and takes the RBAC verdict on the \
         first decrypted byte (CF-67-7 owns this; upstream Envoy accepts this config; the plaintext \
         form IS supported from phase 67.3)"
    )]
    UnsupportedNetworkFilterChainComposition {
        listener: String,
        chain_index: usize,
        non_terminal: String,
        terminal: String,
    },
```

- [ ] **Step 5: Update the precedence test** `terminal_not_last_error_wins_over_unsupported_composition` (`:5885-5904`). `[echo, rbac, tcp_proxy]` on a *plaintext* chain no longer hits the composition rule — it still fails `NetworkFilterNotTerminal` (echo not last), so the assertion is unchanged; rename it and drop the "composition" framing:

```rust
/// 67.1 D2 / SPEC R-5: ERROR PRECEDENCE. `[echo, rbac, tcp_proxy]` violates the
/// terminal-not-last rule (echo is terminal but not last). That error fires from
/// the in-order scan regardless of 67.3's composition narrowing.
#[test]
fn terminal_not_last_wins_for_echo_rbac_tcp_proxy() {
    let prefix = format!("            - name: envoy.filters.network.echo\n{RBAC_FILTER_YAML}");
    let yaml = chain_before_tcp_proxy_yaml(&prefix);
    let mut b: crate::Bootstrap = serde_yaml::from_str(&yaml).expect("parses");
    let err = validate(&mut b).expect_err("[echo, rbac, tcp_proxy] must be rejected");
    assert!(matches!(err, crate::ConfigError::NetworkFilterNotTerminal { ref name, .. } if name == crate::ECHO_FILTER), "got {err:?}");
}
```

Keep `lone_tcp_proxy_chain_is_still_accepted` (`:5878`) unchanged (the over-rejection guard).

- [ ] **Step 6: Run the config tests + build.**

Run: `cargo test -p envoy-config --lib --no-fail-fast`
Expected: PASS.

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "67.3 D5/D6: narrow [non-terminal, tcp_proxy] rejection to TLS chains (plaintext now accepted); CF-67-7"
```

---

## Task 5 — envoy-bin backstops over the real binary; BEHAVIOR_CONTRACT item 13 (D7 + D6 record)

**Files:**
- Modify: `crates/envoy-bin/tests/network_filter_rbac.rs` (delete `rbac_before_tcp_proxy_is_rejected_at_config_load` `:731-776`; keep `tcp_proxy_alone_is_still_accepted`; add the establishment backstops)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (item 13 `tcp_proxy` row `:421-435`)

**Interfaces:**
- Consumes: `spawn_envoy_bin`, `validate_config`, `reserve_port`, `scrape_admin_stats`, `wait_ready`, `READ_BUDGET`, `VALIDATE_BUDGET` (existing helpers).

- [ ] **Step 1: Delete `rbac_before_tcp_proxy_is_rejected_at_config_load`** (`:731-776`) and add the plaintext-accept + TLS-reject pair over `target/debug/envoy-bin`. The plaintext test boots an in-process banner backend on `127.0.0.1:<port>`, points a plaintext `[rbac(ALLOW), tcp_proxy→backend]` listener (STRICT_DNS to `127.0.0.1:<port>`) at it, connects a client that sends nothing, and asserts (a) the banner arrives (bounded by `READ_BUDGET`), (b) `cluster.backend.upstream_cx_total == 1` via `scrape_admin_stats`. The TLS test asserts `validate_config` REJECTS the TLS-downstream variant with a message containing `CF-67-7` and both filter names.

> These boot the real binary → need `cargo build -p envoy-bin` (state-4 gate) and depend on Task 4 (plaintext now accepted). Every downstream read MUST be `READ_BUDGET`-bounded (M-2). Model the config on the file's existing `rbac_echo_cfg`/`rbac_hcm` helpers; add a `rbac_tcp_proxy_cfg(port, backend_port, action)` helper.

- [ ] **Step 2: Add the DENY + FIN matrix backstops (D3).**
  - DENY over the real binary: `[rbac(DENY), tcp_proxy]` → banner delivered, first byte closes, `rbac.denied` delta `== 1`, and the in-process backend never records the byte (mirrors Task 3's witness but end-to-end).
  - FIN matrix: a data-less FIN (connect + `shutdown(WR)` + no data) through `[rbac(ALLOW), tcp_proxy]` ticks `rbac.allowed` delta `== 1`; the SAME probe through `[rbac(ALLOW), echo]` ticks `rbac.allowed` delta `== 0` (re-confirms ADR-0131 case C / the peek-`Ok(0)` path — the asymmetry is a terminal-handler property, D3). Assert deltas via `scrape_admin_stats` baseline/after snapshots (ADR-0131 decision 4 discipline).

- [ ] **Step 3: Update BEHAVIOR_CONTRACT item 13's `tcp_proxy` row** (`:421-435`): change "RECORDED DIVERGENCE: REJECTED AT CONFIG LOAD … owner phase `67.3`" to the split outcome —
  - **plaintext `[rbac, tcp_proxy]` = FULL PARITY**: upstream connects at establishment (banner reaches a byte-less client); verdict on the first downstream byte or a data-less FIN; DENY withholds the byte from the upstream. Pinned by the new backstops + the in-process witnesses. Reference ADR-0135.
  - **TLS-downstream `[rbac, tcp_proxy]` = RECORDED FAIL-LOUD DIVERGENCE, owner CF-67-7**: MEASURED (ADR-0135, D6 probe) — upstream Envoy establishes the upstream at raw-TCP accept (before the handshake) and takes the verdict on the first decrypted byte; envoy-rust rejects the config until CF-67-7. Never silent.

  Do NOT touch item 14 or the `rbac.rs` HTTP-vs-L4 divergence.

- [ ] **Step 4: Build + run the backstops + the differential regression set.**

Run: `cargo build -p envoy-bin && cargo test -p envoy-bin --test network_filter_rbac --no-fail-fast`
Then: the differential regression set (`0001`/`0071`/`0072`/`0073` stay green, UNEDITED).
Expected: PASS (the host-flake set per the memories is CI-authoritative — adjudicate with `--no-fail-fast`, isolation, and the `local passed+failed == CI passed` cross-check).

- [ ] **Step 5: Commit.**

```bash
git add crates/envoy-bin/tests/network_filter_rbac.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "67.3 D7/D6: envoy-bin establishment backstops + FIN matrix; BEHAVIOR item 13 (plaintext parity, TLS CF-67-7)"
```

---

## Task 6 — Verification gate (state-4 preview — the state-4 session runs the full §7.5 gate)

Not executed at state-3; this is the checklist the state-4 `superpowers:verification-before-completion` session runs. Leave the tree passing it:

- [ ] `cargo build --workspace --all-targets`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo test --workspace --no-fail-fast` (adjudicate the known host-flake set against CI per the memories; never pipe through `tail`)
- [ ] `cargo deny check` (a fresh-advisory RED is a patch-bump, not a phase regression — memory `cargo-deny-reds-on-unrelated-advisory`)
- [ ] Differential surface: `0001`/`0071`/`0072`/`0073` green, **unedited**.
- [ ] §7.5 gate (d): RECORD "satisfied by the pre-existing `parse_bootstrap` target; no new fuzz target" — do NOT skip it silently.
- [ ] Conformance unchanged; never trim `known-failures.txt`.

---

## Deliverable→Task Traceability

| Deliverable | Task(s) |
|---|---|
| D1 `ConnectionHandler` establishment/data split (default method; echo/hcm/dr unchanged, tcp_proxy overrides) | 1, 3 |
| D2 reusable filter-owned first-byte gate (`NetworkFilter` shape unchanged; CF-67-3 deferred) | 1 |
| D3 per-terminal data-less-FIN semantics (handler property, not a name check) | 1 (`evaluate_read_half`), 3, 5 |
| D4 `TcpProxy` → `connect_upstream()` + `relay()` | 2, 3 |
| D5 remove 67.1's fail-loud rejection of PLAINTEXT `[rbac, tcp_proxy]` | 4 |
| D6 TLS composition — MEASURED (probe done at state-2); narrowed to fail-loud + CF-67-7 (ADR-0135) | 4, 5 |
| D7 in-process + envoy-bin backstops (banner-to-byte-less, DENY-not-forwarded, FIN matrix) | 3, 5 |
| D8 CF-67-6 (opportunistic) | NOT folded — stays open (see below) |

**D8 / CF-67-6 disposition:** the `close_with_drain` steady-state drain bound is **NOT** folded into 67.3. It is opportunistic per the SPEC ("not a commitment"), touches a different concern (drain timeout, not establishment ordering), and would add scope for no C-1 benefit. It **stays a live carry-forward.** `post_eof_client_write_is_accepted_not_reset` and its DENY twin remain unweakened (ADR-0124).

**M-1 disposition:** 67.3 does **not** touch the `CidrRange`/`prefix_match` surface (`crates/envoy-filter/src/rbac.rs`), so 67.2's M-1 (the `prefix_match` guard band) is **NOT consumed** — it stays a live carry-forward for the next phase that touches that surface.

---

## §6.1 Gate Re-derivation (state-2 duty — MUST re-derive, not inherit)

| Area | Net LoC (est.) |
|---|---|
| Task 1 — `FirstByteGate` + `GateOutcome` + `handle_gated` default + `ChainHandler` rewire | ~+90 |
| Task 2 — `connect_upstream()` + `relay()` refactor (mostly moved code) | ~+40 |
| Task 3 — `handle_gated` override + `relay_gated` + in-process witnesses | ~+150 |
| Task 4 — narrow rejection to TLS + re-message + test updates | ~-10 |
| Task 5 — envoy-bin backstops + BEHAVIOR item 13 | ~+160 |
| **Total** | **~430 net LoC, ~6 implementation tasks** |

Both comfortably under §6.1's thresholds (~1500 LoC OR ~25 tasks). **The gate does NOT fire.** This fresh derivation is lower than the SPEC's ~690 estimate because the D6 measurement let TLS stay fail-loud (no TLS-handler restructure), shrinking D6 from ~80 LoC to a config-condition + tests. **§6.1's mid-execution valve stays ARMED** — Task 3's `relay_gated` is the split candidate if its sub-steps blow up on contact with reality (ADR-0132's own precedent).

---

## Self-Review

- **Spec coverage:** D1–D7 each map to a task (table above); D8/CF-67-6 explicitly not folded (documented, ADR-0124 guard preserved); the SPEC's PLAN-VERIFY W-1…W-6 are resolved and recorded in ADR-0135 (W-1 `handle_gated`/`Arc<Self>`; W-2 gate-in-`FirstByteGate`, default preserves echo/hcm; W-3 confirmed echo/hcm `peek`→`Ok(0)`→`SkippedCleanly`; W-4 `Denied` drops the byte; W-5/D6 measured + narrowed; W-6 no orphaned test/row — the over-rejection guards survive, item 13 updated).
- **Placeholder scan:** every code step shows real code; the two TLS/helper splices carry an explicit "verify against the live tree" note, not a vague "add error handling."
- **Type consistency:** `FirstByteGate`, `GateOutcome`, `handle_gated(self: Arc<Self>, …)`, `UpstreamConn`, `connect_upstream`/`relay`/`relay_gated` are used identically across the interfaces block and Tasks 1–3.
- **Ordering hazard:** Task 3's in-process witnesses bypass config load, so they run before Task 4; the Task 5 envoy-bin backstops need plaintext `[rbac, tcp_proxy]` accepted, so they follow Task 4. Encoded in the task order.
