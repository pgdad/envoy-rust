# Phase 61 — `61-perf-listener-so-reuseport` — SPEC

**Pick (ADR-0118):** Give the listener an `SO_REUSEPORT` fan-out so the accept
path scales across cores. Today one accepting socket + one accept loop funnel
every downstream connection through a single kernel accept queue (and a single
RX-softirq-serviced FD); Envoy binds one `SO_REUSEPORT` socket per worker so the
kernel hashes incoming connections across N independent accept queues. This
phase closes that architectural gap: bind N reuseport sockets and run one accept
loop per socket. It is **byte-neutral to clients** (no wire or access-log byte
changes — the number of accepting sockets is invisible on the wire), so it adds
NO differential fixture; its acceptance is (a) all fixtures `0001`-`0065` stay
byte-identical and (b) a benchmark showing the accept path parallelizes on Linux
(even connection distribution across the N sockets + accept-throughput scaling)
with **no regression** on the single-socket path. Motivated by the same
`envoy-bin` flamegraph as phases 59–60: the proxy is I/O-bound (kernel network
I/O ~84% of self-time), and the single accept queue is the biggest proxy-side
scaling lever on a multi-core node.

> **Scope note (from the flamegraph analysis).** The flamegraph's inflated
> softirq was *partly* a cross-node kube-proxy hop — that is **infra, not an
> envoy-rust change** (fix via `externalTrafficPolicy: Local` + node vCPUs on the
> k8s Service, which lives in the control plane's M-B rendering / the E2E, not
> here). `SO_REUSEPORT` is the **code half** and helps single-node multi-core
> scaling regardless of the k8s path. This phase is only the code half.

> **State-0 recon (this session) — located live in the tree.**
> - `crates/envoy-listener/src/lib.rs` — `Listener` held one
>   `tokio::net::TcpListener`; `bind` did a plain `TcpListener::bind` (SO_REUSEADDR
>   only, no SO_REUSEPORT); `serve` ran one accept loop over it. Concurrency came
>   purely from tokio work-stealing over that one FD.
> - `crates/envoy-bin/src/main.rs` — two `Listener::bind` + `serve` sites (tcp_proxy
>   and HCM); the tokio runtime is `new_multi_thread().enable_all()` (worker count
>   = logical CPUs).
> - `crates/envoy-config/src/bootstrap.rs` — `Listener` is `deny_unknown_fields`
>   and had no `enable_reuse_port` field, so a bootstrap setting it would fail parse.
> - `socket2` is already in the dependency tree (transitively); `#![forbid(unsafe_code)]`
>   holds crate-wide, so the socket setup must use only safe socket2/std/tokio APIs.

## §1 — Goal & differential surface

**Goal.** Bind one `SO_REUSEPORT` socket per worker and run one accept loop per
socket, so the accept path (and RX softirq steering) spreads across cores —
**with zero change to any wire or access-log byte** and **no regression on the
single-socket path**.

**Differential surface at phase end: UNCHANGED.** No new fixture; no fixture
output changes. The socket count is invisible to clients; every response is
byte-identical. The differential harness (which drives `envoy-bin`) exercises the
reuseport bind path on Linux CI **by construction** and must stay green. h2spec
unchanged.

**Perf surface (opt-in throughput/distribution bound, this phase only).** A
self-contained `--release` bench (`crates/envoy-listener/tests/reuseport_bench.rs`,
`#[ignore]`d) measures accept throughput + per-socket distribution for 1 vs. N
reuseport sockets under connection churn. **Acceptance: on Linux the kernel
distributes connections across all N sockets (the mechanism) and accept
throughput scales; on the single-socket path / non-Linux there is no regression.**
Observed this session: on Linux (4-vCPU VM) the kernel splits connections evenly
across all N sockets every run (e.g. N=4 → `[65793,67904,66132,68060]`) and a
clean run scaled accept throughput 1.0→2.07×→3.66× for 1→2→4 sockets (absolute
throughput in the virtualized VM is noisy, so the even distribution is the
reproducible signal — a dedicated multi-core host / real load generator gives the
clean scaling curve, per the plan's methodology note). On macOS the kernel does
not load-balance `SO_REUSEPORT`, so N>1 is neutral (all connections to one socket,
same throughput) — and in production the fan-out is Linux-gated off there anyway.

## §2 — Scope (minimum-viable, ADR-0118)

**A config field + an N-socket bind + a fan-out accept loop + main.rs wiring +
tests + a bench — NO new fixture / `Op` / differential-harness change / runtime
crate beyond the already-in-tree `socket2` / `ConfigError` variant / fuzz target:**

- **§A — `envoy-config`: `Listener.enable_reuse_port: bool`.** Envoy's field
  (default **true** upstream). `#[serde(default = "default_enable_reuse_port")]`
  so a bootstrap that omits it keeps the field `true` without tripping
  `deny_unknown_fields`. Parse-and-store; the data plane consumes it. Parse tests
  (absent → true; explicit true/false round-trip).
- **§B — `envoy-listener`: `Listener` holds `Vec<TcpListener>`.** `bind` is
  **unchanged behavior** — one plain socket, no SO_REUSEPORT (every existing
  caller/test flows through it untouched). New `bind_with_concurrency(cfg, …,
  concurrency)` binds `concurrency` `SO_REUSEPORT` sockets **only when**
  `cfg.enable_reuse_port && cfg!(target_os = "linux") && concurrency > 1`;
  otherwise it falls back to the single plain socket (byte-identical). The socket
  setup (`bind_reuseport_socket`) uses **only safe** `socket2` APIs
  (`Socket::set_reuse_port` + `set_reuse_address` + `set_nonblocking`, then
  `socket2::Socket → std::net::TcpListener → tokio::net::TcpListener::from_std`,
  all `From`/`from_std` — no `from_raw_fd`), so `#![forbid(unsafe_code)]` holds.
- **§C — fan-out `serve`.** For a single socket, `serve` runs the **original
  inline accept loop verbatim** (extracted into `accept_loop`) — byte-for-byte the
  pre-change behavior. For N sockets it spawns one `accept_loop` per socket, each
  with its own kernel accept queue + in-flight-connection JoinSet + drain; the
  single `shutdown` future is broadcast to all loops via a `watch<bool>`, the
  drain `Arc` and stat `Arc`s are cloned per loop, and the
  `listener_manager.total_listeners_active` gauge still counts **one** logical
  listener. The first per-loop error (e.g. a `DrainTimeout`) is surfaced.
- **§D — `main.rs`.** Both `Listener::bind` sites call `bind_with_concurrency`
  with `concurrency = std::thread::available_parallelism()` (matching the tokio
  worker count). Non-Linux / single-core runs are byte-identical.
- **§E — tests + bench.** `envoy-config` parse tests (§A); `envoy-listener`:
  `reuseport_binds_multiple_sockets_on_same_port` (two sockets on one port bind —
  impossible without SO_REUSEPORT — cross-platform), `reuseport_fanout_serves_and_drains`
  (a 2-socket `Listener` serves + drains cleanly — exercises the fan-out path,
  cross-platform), and a **Linux-gated** `reuseport_distributes_connections_across_sockets_linux`
  (many connections spread across ≥2 sockets). Plus the `#[ignore]`d
  `reuseport_bench.rs` (accept throughput + distribution, 1 vs N).

**Load-bearing invariant:** all `0001`-`0065` stay byte-identical. §B/§C change
only *how many accepting sockets exist* and *which task runs the accept loop* —
never a wire byte. The single-socket path (the default off Linux / reuse_port off
/ 1 core) is the pre-change code; the drain state machine, stats, and
`bind_fails_cleanly_on_address_in_use` (plain `bind`, still one socket) are
untouched.

## §3 — PLAN-VERIFY items (state-2 §6.2)

1. **Re-confirm the two `main.rs` bind sites** and that `bind` (single-socket)
   stays the path for every existing test.
2. **Confirm `socket2` `set_reuse_port` needs `features = ["all"]`** and is
   available on Linux + macOS; confirm `Socket → std TcpListener` is a safe
   `From` (no `from_raw_fd`) so `#![forbid(unsafe_code)]` holds.
3. **Confirm the fan-out preserves drain semantics** — the extracted `accept_loop`
   is the original body; the watch-broadcast shutdown resolves for an
   already-fired shutdown (`wait_for` checks the current value); the lm gauge
   increments once.
4. **Confirm the Linux gate** (`cfg!(target_os = "linux")`) — non-Linux binds one
   socket, so macOS/BSD (no reuseport LB) never starve N-1 idle sockets.
5. **Re-confirm no differential fixture output changes** (byte-neutral to clients).

## §4 — Reuse map (what exists; do not rebuild)

- The `Listener` stats (idempotent-by-name registration), the drain state machine
  (`DrainState`/`DRAIN_BUDGET`), and the accept loop body — **reused**; §C extracts
  the loop into `accept_loop` and shares it across the 1- and N-socket paths.
- `socket2` (already in-tree) — added as a direct `envoy-listener` dep at the
  in-tree version; no new transitive graph.
- The tokio multi-thread runtime + `available_parallelism` — **reused** for the
  worker/socket count; no new `--concurrency` flag this phase.

## §5 — Behavioral contract notes

- **Wire / access-log bytes** — unchanged; socket count is invisible to clients.
  No BEHAVIOR_CONTRACT edit.
- **`enable_reuse_port`** — new parsed field, default true (Envoy fidelity);
  transparent to clients.
- **Timing** — the contract's default is "not compared"; this phase **opts into**
  a throughput/distribution bound for its own bench only (§1). No standing timing
  gate is added to the differential suite.
- `#![forbid(unsafe_code)]` holds (safe socket2/std/tokio conversions only).

## §6 — Process

- **§6.1 split — projected NOT to fire.** ~5 files (config field, listener, bin
  wiring, one test literal fix, new bench), no new harness/fixture/struct beyond
  the field. Under the gate.
- **§6.2 reconciliation** — reserved if §3's PLAN-write overturns a fact (e.g. a
  drain-semantics regression under the fan-out). Not expected (all 36 existing
  listener tests, incl. the three drain tests, pass unchanged).
- **Carry-forwards:** OPENS a `perf` carry-forward — an explicit `--concurrency`
  config knob (decouple socket count from `available_parallelism`), FreeBSD
  `SO_REUSEPORT_LB`, and per-worker CPU pinning are all follow-ups. CONSUMES none.
- Pick + §A–§E ground-truth locked by **ADR-0118** (the next-available number;
  phases 59/60 drafted ADR-0116/0117).

## §7 — Acceptance (§7.5, re-run at state-4)

(a) all `0001`-`0065` fixtures green **simultaneously and byte-identical** (the §2
invariant) + (b) the §E unit tests green (reuseport bind, fan-out serve+drain,
Linux distribution) + (c) all existing `envoy-listener` tests (incl. drain +
`address_in_use`) unchanged + (d) the §1 bench shows Linux distribution across all
N sockets + no single-socket regression + (e) h2spec unchanged + (f) no new fuzz
target + (g) build/clippy/fmt/test/deny clean; `#![forbid(unsafe_code)]` holds; no
new runtime crate beyond in-tree `socket2` / `Op` / `ConfigError` variant + (h)
`REVIEW.md` approved.

_Scope locked by ADR-0118. The state-2 PLAN-write re-confirms §3's PLAN-VERIFY
items (bind sites, socket2 safety + feature, drain-semantics preservation, the
Linux gate, byte-neutrality) and authors `PLAN.md`. The state-3 implementation is
the session after._
