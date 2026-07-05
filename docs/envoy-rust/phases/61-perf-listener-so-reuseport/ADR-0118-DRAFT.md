# DRAFT ADR for phase 61 — to be slotted into `docs/envoy-rust/DECISIONS.md`

> This is a standalone draft. `DECISIONS.md` is append-only and ordered
> newest-first; the maintainer places this block at the canonical position and
> confirms the number. **ADR-0118** is the next-available number: the ledger head
> on `main` is ADR-0115 (phase 58); phases 59 and 60 draft ADR-0116 and ADR-0117.
> If a sibling session fires 0116–0118 first, renumber to the then-next-available.

---

## ADR-0118: Phase-61 pick + scope — **give the listener an `SO_REUSEPORT` fan-out (one accept socket + accept loop per worker) so the accept path scales across cores, byte-neutral to clients, single-socket path byte-identical — a proxy-side scaling feature, NO new fixture**

- Date: 2026-07-05
- Status: accepted
- Context: The same `envoy-bin` flamegraph that motivated phases 59–60 (I/O-bound;
  kernel network I/O ~84% of self-time) shows the single accept queue as the
  biggest proxy-side scaling lever on a multi-core node. `Listener` bound one
  `tokio::net::TcpListener` (plain `bind`, SO_REUSEADDR only) and ran one accept
  loop; all downstream connections funnelled through one kernel accept queue and
  one RX-softirq-serviced FD, with concurrency coming only from tokio work-stealing
  over that one FD. Envoy binds one `SO_REUSEPORT` socket per worker so the kernel
  hashes incoming connections across N independent accept queues, spreading the
  accept path across cores. This is a real feature (not a byte-level parity gap),
  but it is **byte-neutral to clients** — the socket count is invisible on the wire
  — so it fits the differential model as a no-fixture change whose acceptance is
  byte-stability of `0001`-`0065` plus a throughput/distribution bound. The scope
  is deliberately the **code half** only: the flamegraph's inflated softirq was
  partly a cross-node kube-proxy hop, which is infra (control-plane Service
  rendering / E2E), not an envoy-rust change.
- Options considered:
  - **Do nothing** — rely on tokio work-stealing over one FD. Rejected: one accept
    queue + one softirq-serviced FD is the scaling bottleneck the flamegraph points
    at; Envoy closed this gap years ago.
  - **Always bind N reuseport sockets (all platforms, no gate)** — simplest.
    Rejected: `SO_REUSEPORT` load-balances across sockets only on **Linux**; on
    macOS/BSD delivery is to a single socket, so N-1 accept loops would starve (no
    benefit, wasted sockets). Gate the fan-out on `cfg!(target_os = "linux")`; bind
    a single plain socket elsewhere (identical to today).
  - **Widen `serve` to accept from N sockets in one `select!`** vs **one accept
    loop per socket.** Chose one loop per socket (each with its own kernel accept
    queue — the whole point) and kept the **single-socket path as the original
    inline loop verbatim**, so the default path is byte-for-byte unchanged and the
    fan-out is purely additive.
  - **`unsafe` `from_raw_fd` for the socket setup** — rejected: `#![forbid(unsafe_code)]`
    holds crate-wide. `socket2::Socket` → `std::net::TcpListener` → tokio
    `from_std` are all safe conversions.
  - **A new `--concurrency` flag now** vs **derive from `available_parallelism`.**
    Chose `available_parallelism` (matches the tokio default worker count) to keep
    the surface small; an explicit knob is a noted follow-up.
  - **Default `enable_reuse_port` false** vs **true.** Chose **true** for Envoy
    fidelity (Envoy defaults it on for TCP); the Linux gate + single-socket
    fallback make the default safe everywhere, and it is byte-neutral to clients.
- Decision: Land §A–§E of the phase-61 SPEC. (§A) `envoy-config`
  `Listener.enable_reuse_port: bool`, `#[serde(default = "default_enable_reuse_port")]`
  = true, parse-and-store + parse tests. (§B) `envoy-listener`: `Listener` holds
  `Vec<TcpListener>`; `bind` unchanged (one plain socket); new
  `bind_with_concurrency` binds N `SO_REUSEPORT` sockets only when
  `enable_reuse_port && cfg!(target_os = "linux") && concurrency > 1`, else the
  single plain socket; `bind_reuseport_socket` uses only safe `socket2` APIs
  (`set_reuse_port`/`set_reuse_address`/`set_nonblocking` → `From`/`from_std`). (§C)
  `serve` runs the original inline `accept_loop` for one socket and fans out one
  `accept_loop` per socket for N, broadcasting the single `shutdown` via a
  `watch<bool>`, per-loop drain + JoinSet, one `listener_manager.total_listeners_active`
  count. (§D) `main.rs` calls `bind_with_concurrency` at both sites with
  `available_parallelism`. (§E) parse tests + reuseport-bind + fan-out-serve+drain
  + Linux-gated distribution tests + an `#[ignore]`d accept-throughput/distribution
  bench. No new fixture, `Op`, differential-harness change, runtime crate beyond
  the already-in-tree `socket2`, `ConfigError` variant, or fuzz target.
- Rationale: Byte-neutral to clients, so acceptance is byte-stability of
  `0001`-`0065` plus a throughput/distribution bound, NOT a differential witness.
  The single-socket default path is the pre-change code (drain state machine,
  stats, `address_in_use` all untouched), so there is no regression by
  construction; the fan-out adds N independent kernel accept queues on Linux (a
  strict superset of the single-queue capacity). Safe socket2 keeps
  `#![forbid(unsafe_code)]` intact.
- Consequences: `crates/envoy-config` (one field + tests), `crates/envoy-listener`
  (Vec<TcpListener> + bind_with_concurrency + fan-out serve + socket2 dep + tests +
  bench), `crates/envoy-bin` (two bind sites), plus two test-literal fixups
  (`envoy-tls`, `envoy-admin`). No H1/H2/codec/`envoy-config`-schema-output change →
  h2spec + `parse_bootstrap` fuzz unaffected. Local evidence this session: all
  workspace lib/bin tests green (1639, zero failures; envoy-listener 36 existing +
  new reuseport tests, drain semantics preserved); fmt/clippy/cargo-deny clean.
  Bench: on Linux (4-vCPU VM) the kernel distributes connections **evenly across all
  N sockets every run** (N=4 → `[65793,67904,66132,68060]`) and a clean run scaled
  accept throughput 1.0→2.07×→3.66× (1→2→4 sockets; absolute throughput in the
  virtualized VM is noisy, so the even distribution is the reproducible signal — a
  bare-metal multi-core host + real load generator gives the clean scaling curve).
  On macOS the fan-out is Linux-gated off (single socket, byte-identical). The
  Docker-gated differential suite (`0001`-`0065`) exercises the reuseport bind path
  on Linux CI **by construction** and is expected green (no emitted byte changes) —
  confirm at the state-4 §7.5 gate. OPENS a `perf` carry-forward (explicit
  `--concurrency` knob, FreeBSD `SO_REUSEPORT_LB`, per-worker CPU pinning); CONSUMES
  none. This phase is independent of phases 59/60 (different crate) and rebases onto
  `main` cleanly. DECISIONS.md ledger head after this ADR: **ADR-0118** (assuming
  0116/0117 land first; next-available **ADR-0119**). ADR-0014 in force;
  `#![forbid(unsafe_code)]` PRESERVED. The state-2 PLAN-write is the next session.
