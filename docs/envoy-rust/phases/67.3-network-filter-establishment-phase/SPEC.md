# Phase 67.3 — the network-filter ESTABLISHMENT/DATA-phase split, and the correct `[rbac, tcp_proxy]` composition

> **Status:** SPEC authored at the phase-67.1 §5.2 state-3 re-entry, when **§6.1's mid-execution split
> valve FIRED** (`ADR-0132`). Next: §5 state-2 PLAN-write (`superpowers:writing-plans` → `PLAN.md`).
> **Parent phase:** `67` (`docs/envoy-rust/phases/67-network-filter-rbac/SPEC.md`), whose ROADMAP row now
> reads `sub-phases = 67.1, 67.2, 67.3`.
> **ROADMAP row:** `67.3` (`planned`, depends-on `67.1`).
> **Siblings:** `67.1-network-rbac-iteration-protocol` (the chain iteration protocol + network `rbac`),
> `67.2-network-rbac-connection-matchers` (the connection-level matcher arms).
> **Governing ADR:** `ADR-0132` (the C-1 correction, the measurement, and this split).

This document is written for a stranger with zero prior context (doctrine D-3.4). Every load-bearing
claim below was **measured** against the pinned upstream image, not inferred from Envoy source (D-3.3).

---

## §1. Why this phase exists

Phase `67.1` introduced the network-filter **chain iteration protocol** and the project's first
NON-TERMINAL network filter, `envoy.filters.network.rbac`. Its `envoy_listener::ChainHandler` waits for
the **first downstream byte** (a non-consuming `TcpStream::peek`) before running the filter chain — the
`ONE_TIME_ON_FIRST_BYTE` semantics measured in `ADR-0131`.

`67.1`'s state-5 code-review (`REVIEW.md`, finding **C-1**) established by measurement that this
`peek` was placed **one level too high**: it gates the *chain's hand-off to the terminal filter*, when
it should gate only *the RBAC filter's decision*.

**Upstream Envoy runs every filter's `onNewConnection` at connection establishment — including the
TERMINAL filter's — and defers only the RBAC verdict to the first downstream byte.**

For `echo` and `http_connection_manager` this is invisible: neither performs establishment-time work.
For `direct_response` and `tcp_proxy` it is not.

`67.1` repairs `direct_response` (it simply bypasses the chain — see §2 R-3). **`tcp_proxy` cannot be
repaired without splitting `ConnectionHandler` into an establishment phase and a data phase**, and that
split reaches three crates plus the TLS handler. `ADR-0132` fired `BOOTSTRAP_PROMPT.md` §6.1's
mid-execution valve and carved it here.

**Until this phase lands, `67.1` REJECTS `[rbac, tcp_proxy]` at config load, fail-loud** (`ADR-0049`
decision-2 (b) posture, `ConfigError::UnsupportedNetworkFilterChainComposition`). **This phase deletes
that rejection.** Rejecting a config upstream accepts is a recorded divergence; it is strictly better
than `67.1`'s shipped behavior, which was a **runtime deadlock**.

---

## §2. Measured evidence carried into this phase

Measured at the `67.1` state-3 re-entry against `envoyproxy/envoy:v1.33.0` (digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, per `ENVOY_TARGET.md`,
doctrine D-3.7), with `/stats` scraped **mid-flight** (while the client connection was still open) so the
counter's trigger is disambiguated rather than inferred. The `tcp_proxy` backend ran as a **sibling
container** on a shared docker network and spoke a banner **before** any client byte.

### R-1 — Per-terminal behavior of `[rbac(any), <terminal>]`

| terminal | connect, send nothing, stay open | connect + FIN, no data | connect + first byte | establishment-time work |
|---|---|---|---|---|
| `echo` | no tick; stays open | **no tick**; clean EOF | tick | **none** |
| `http_connection_manager` | no tick; stays open | **no tick**; clean EOF | tick (on the request) | **none** |
| `direct_response` | **payload written, clean EOF, NO tick** | same | same | **writes payload, closes** |
| `tcp_proxy` | no tick; **banner delivered; `cluster.<name>.upstream_cx_total: 1`** | **TICKS** | tick | **connects upstream** |

### R-2 — `tcp_proxy` specifics (this phase's whole subject)

- **The upstream connection is established BEFORE any downstream byte.** Scraped mid-flight on a
  byte-less, still-open client connection: `upstream_cx_total: 1`, `rbac.allowed: 0`.
- **The upstream's server-first bytes reach the client before any downstream byte.** The banner
  `220 BANNER\n` was delivered to a client that had sent nothing.
- **The verdict lands on the first downstream byte.** `[rbac(DENY), tcp_proxy]`: byte-less and open ⇒
  `denied: 0` and the banner already delivered; then the client sends one byte ⇒ `denied: 1` and the
  connection is closed (client reads a clean EOF).
- **A data-less FIN ALSO evaluates — for `tcp_proxy` only.** `connect + shutdown(WR)` with no payload
  ticks `allowed: 1` (ALLOW) / `denied: 1` (DENY). The same probe against `echo` and `hcm` ticks
  **nothing** (re-confirming `ADR-0131` case C). The natural reading is downstream half-close
  propagation: `tcp_proxy` enables it, so the FIN surfaces as a zero-byte end-of-stream read event the
  filter sees; the others do not. **Measured, not read from source.**
- **On DENY the first downstream byte must NOT reach the upstream.** Upstream's `onData` returns
  `StopIteration` before `tcp_proxy` sees the data. A "race the terminal and abort on DENY" design would
  forward that byte and is therefore rejected (`ADR-0132` decision 4 (b)).

### R-3 — Why `direct_response` is NOT this phase's problem

Upstream delivers the payload and closes **even under `action: DENY`**, with all four
`<stat_prefix>.rbac.*` counters at `0` — the terminal filter writes and closes before any `onData` can
fire, so the RBAC filter never evaluates. `67.1` reproduces this exactly by **bypassing the chain**
whenever the terminal filter is `direct_response`. Nothing is deferred here.

### R-4 — The blocking code fact

`envoy_listener::ConnectionHandler::handle(&self, downstream: TcpStream)` **fuses establishment and
data into one future that takes ownership of the socket.** There is no seam at which `ChainHandler` can
let `tcp_proxy` connect upstream and then interpose a first-byte gate.

Compounding it: `envoy_tcp::TcpProxy::handle::<S>` is **generic over the stream type** (`S: AsyncRead +
AsyncWrite`), because the same body serves plaintext and upstream-TLS. A generic `S` has no `peek`. The
`dyn ConnectionHandler for TcpProxy` impl (`crates/envoy-tcp/src/lib.rs:178-204`) does receive a
concrete `TcpStream`, but it immediately delegates to `handle::<TcpStream>`, inside which the upstream
connect (`:100`), the `upstream_cx_total` tick (`:110`) and the bidirectional `tokio::select!` copy
(`:136-143`) are one straight-line body.

### R-5 — TLS is a separate, UNMEASURED question

On a TLS listener, `ChainHandler` wraps `TlsAcceptingHandler`, so the chain runs on the raw `TcpStream`
**before** the TLS handshake. A TLS client always speaks first (the `ClientHello`), so the first-byte
`peek` never stalls there — the `67.1` deadlock is a **plaintext** `tcp_proxy` problem. But what upstream
Envoy does with `[rbac, tcp_proxy]` on a **TLS** listener — in particular whether the upstream connection
is established before or after the downstream handshake — **was never probed.** This phase must probe it
before asserting anything (the §6.3 / `CF-67-5` discipline).

---

## §3. Deliverables

### D1 — Split `ConnectionHandler` into an establishment phase and a data phase

A terminal filter must be able to perform its establishment-time work **before** the chain's first-byte
gate resolves. The exact shape is the PLAN's to settle (see §8 W-1), but it must:

- let `tcp_proxy` connect upstream (and tick `upstream_cx_total`) at establishment;
- let the upstream's server-first bytes flow downstream immediately;
- keep the downstream→upstream direction **closed** until the chain admits the connection;
- keep `echo` / `hcm` behavior **byte-for-byte unchanged** (they have no establishment work), so fixtures
  `0001`, `0072` and `0073` stay green with no edit;
- keep `direct_response`'s `67.1` bypass intact (or subsume it cleanly).

**Backward compatibility is a hard requirement:** `ConnectionHandler` is a phase-02 trait implemented by
`echo`, `direct_response`, `tcp_proxy`, `http_connection_manager` and `TlsAcceptingHandler`. A default
method (establishment = no-op) keeps every existing impl compiling and behaviorally identical.

### D2 — The first-byte gate becomes a reusable, filter-owned primitive

Extract from `ChainHandler` a gate that (a) waits for the first downstream byte **or** a data-less FIN,
(b) runs each non-terminal filter's `on_new_connection`, (c) yields `Continue | StopIteration`. It must
be callable **after** a terminal filter's establishment work, on a concrete `TcpStream`.

**The `NetworkFilter` trait shape must not change**, and filters must still never see payload — the byte
is peeked, never consumed. **`CF-67-3` (payload-visible `on_data`-time iteration + buffering) stays
deferred, with unchanged scope.** This phase is about *establishment ordering*, not payload exposure.

### D3 — The per-terminal data-less-FIN semantics (R-1, R-2)

A data-less FIN evaluates the chain for `tcp_proxy` and **not** for `echo` / `hcm`. Model it as a
property of the terminal handler (half-close propagation), not as a hard-coded filter-name check.

### D4 — Refactor `envoy_tcp::TcpProxy` into `connect_upstream()` + `relay()`

So the gate can be interposed between them (R-4). Preserve `ADR-0016`'s half-close posture (the
`tokio::select!` over the two copies) and the `cx_active` RAII guard / `cx_total` tick placement exactly.

### D5 — Delete `67.1`'s fail-loud rejection

Remove `ConfigError::UnsupportedNetworkFilterChainComposition` (and its tests), so `[rbac, tcp_proxy]` is
accepted again — now that it *behaves*. Remove the corresponding `BEHAVIOR_CONTRACT.md` divergence row.

### D6 — TLS: probe first, then decide (R-5)

Measure `[rbac, tcp_proxy]` on a TLS downstream listener against the pinned image before asserting
anything. If the pre-handshake chain placement diverges, either fix it or record it with an ADR and a
carry-forward. **Do not guess.**

### D7 — Tests

- **In-process backstop:** `[rbac(ALLOW), tcp_proxy]` against a **server-speaks-first** backend — the
  banner must reach a client that has sent nothing. *This is the regression witness for C-1; it fails
  against `67.1`'s code.*
- **In-process backstop:** `[rbac(DENY), tcp_proxy]` — the banner is delivered, then the first downstream
  byte closes the connection, `denied == 1`, and **the byte never reaches the backend**.
- **In-process backstop:** data-less FIN ticks for `tcp_proxy` and does **not** tick for `echo` / `hcm`.
- **Regression:** fixtures `0001`, `0071`, `0072`, `0073` stay green **unedited** (never weaken a fixture).
- A differential fixture is **optional and probably not host-deterministic** (a server-first backend under
  the Docker harness); the in-process backstops are the primary witness, as in `67.2`.

### D8 — Consider `CF-67-6` while you are here

`envoy_listener::close_with_drain` reads to client EOF with **no steady-state bound** (only
`DRAIN_BUDGET` at listener shutdown). Upstream's analogue is `delayed_close_timeout` (default `1s`). This
phase touches the same close paths; folding `CF-67-6` in is **opportunistic, not a commitment**.
**Do NOT weaken or delete `post_eof_client_write_is_accepted_not_reset` or its DENY twin** — the drain
itself is `ADR-0124` and is load-bearing.

---

## §4. Out of scope (deliberate)

- **`CF-67-3`** — payload-visible `on_data`-time iteration, mid-stream `Continue`/`StopIteration`,
  buffering, `injectReadDataToFilterChain`. Filters still never see bytes. Scope **unchanged** by this
  phase.
- **`CF-67-1`** (`shadow_rules`), **`CF-67-2`** (`Action::LOG`).
- **The connection-level matcher arms** — those are `67.2`.
- **`CF-67-5`** — upstream's *connection* behavior on an empty `filters: []` chain. Adjacent (this phase
  probes establishment-time terminal behavior) and **may** be closed opportunistically. Not a commitment.

---

## §5. §7.4 fuzz disposition — NO new fuzz target

This phase introduces no parser and no codec. Network `rbac` still parses nothing. `D5` **removes** a
`ConfigError` variant. The bootstrap config parser remains covered by the pre-existing `parse_bootstrap`
target (`.github/workflows/ci.yml`). **The state-4 session must RECORD §7.5 gate (d) explicitly** as
*"satisfied by the pre-existing `parse_bootstrap` target; no new target"* — not skip it in silence.

---

## §6. Differential surface at phase end

- Fixtures `0001` / `0071` / `0072` / `0073` stay green, **unedited**.
- No new differential fixture is expected (a server-speaks-first backend is not host-deterministic under
  the Docker harness — the `67.2` precedent). The witnesses are in-process.
- Conformance unchanged. `h2spec` remains the only §7.3 suite. **Never trim
  `tests/conformance/h2spec/known-failures.txt`** — this dev host scores invalid-preface `3.5/2` as PASS
  while CI fails it, so a locally-"fixed" list breaks CI (memory `h2spec-3-5-2-preface-host-sensitive`).

---

## §7. Estimated size

| Area | Net LoC (est.) |
|---|---|
| D1 `ConnectionHandler` establishment/data split (default method; 5 impls) | ~150 |
| D2 the reusable first-byte gate extracted from `ChainHandler` | ~90 |
| D3 per-terminal data-less-FIN semantics | ~60 |
| D4 `TcpProxy` → `connect_upstream()` + `relay()` | ~140 |
| D5 delete the fail-loud rejection + its tests + the contract row | ~-60 |
| D6 TLS probe + whatever it forces | ~80 |
| D7 backstops (server-first backend helper, DENY-does-not-forward, FIN matrix) | ~230 |
| **Total** | **~690** |

**~690 net LoC, ~8-10 TDD tasks.** Both comfortably under §6.1's thresholds (~1500 LoC OR ~25 tasks), so
the gate is **not** projected to fire — but the state-2 PLAN-write **must re-derive this rather than
inherit it**, and §6.1's mid-execution valve stays armed. (It fired once already, on exactly this work.)

---

## §8. PLAN-VERIFY items (re-confirm fresh against the live tree at the state-2 PLAN-write)

- **W-1.** The exact shape of the establishment/data split. A default trait method returning an
  "establishment token"? A second trait? A `handle_gated(stream, gate)` with a default that awaits the
  gate then calls `handle`? Weigh against: `TcpProxy::handle::<S>` is generic and cannot `peek` (R-4);
  `TlsAcceptingHandler` wraps the terminal and must not be broken.
- **W-2.** Where does the gate live so that `echo` / `hcm` keep byte-for-byte identical behavior with
  **zero** edits to fixtures `0072`/`0073`?
- **W-3.** Model D3's FIN asymmetry as a handler property. Confirm `echo`/`hcm` really do *not* tick on a
  data-less FIN in envoy-rust today (they must not — `peek` ⇒ `Ok(0)` ⇒ skip the chain).
- **W-4.** Does the DENY path forward the first byte upstream? It must not (R-2). Pin with a backstop
  whose backend records what it received.
- **W-5.** Probe the TLS composition (R-5, D6) **before** writing the plan's TLS task.
- **W-6.** Re-confirm that deleting `UnsupportedNetworkFilterChainComposition` (D5) leaves no orphaned
  test and no stale `BEHAVIOR_CONTRACT.md` row.

---

## §9. Standing traps (read before touching code)

1. **`cargo build -p envoy-bin` before ANY local differential run** — the harness executes
   `target/debug/envoy-bin`, not release (memory `differential-harness-uses-debug-envoy-bin`).
2. **Never pipe a verification run through `tail`** — it truncates the `failures:` block (memory
   `never-pipe-verification-runs-through-tail`).
3. **`cargo test --workspace`'s bare form aborts at the first failing test BINARY — always add
   `--no-fail-fast`.** An invariant core of ~5 REDs (`0061`/`0062`/`0069`/`0070` +
   `admin_config_dump_server_info`) fails deterministically in isolation on this dev host ⇒ environmental.
   **CI is authoritative.**
4. **Never weaken a fixture. Never trim `known-failures.txt`.**
5. **A MUTATION CHECK CAN LIE.** Grep the run for `Compiling`/`Checking` **of the crate you MUTATED**
   (memory `mutation-check-needs-forced-rebuild`). `cargo clippy` prints `Checking`, not `Compiling`.
6. **envoy-bin writes its `ConfigError` to STDOUT** (the tracing subscriber), not stderr.
7. **Do NOT add a `_ =>` catch-all** to the four exhaustive RBAC match sites; **do NOT add `rbac` to
   `is_terminal_network_filter`**; **do NOT reject `filters: []`**.
8. **Do NOT re-open BLOCK-66-1** (`ADR-0125`/`ADR-0126`): no `--quiet`, no removed pre-build, no widened
   30s budget.
9. **`ADR-0131` is CORRECT and must not be reverted** — the RBAC *verdict* really is a first-byte event.
   This phase changes only *what else* was made to wait for it.
10. **ROADMAP rows must escape literal `|` as `\|`.** Rows `36`/`38`/`39`/`52`/`54` are already malformed
    and must NOT be "fixed" (append-only). Verify any row edit with `re.split(r'(?<!\\)\|', line)[1:-1]`
    yielding exactly 6 cells. **Never use `awk -F'|'`.**

---

## §10. Carry-forward ledger

- **CONSUMED by `67.3`:** nothing yet — this phase *repairs* `C-1`, which is a review finding, not a
  carry-forward.
- **OPENED by `ADR-0132`: `CF-67-6`** (bound `close_with_drain`'s steady-state drain; upstream's analogue
  is `delayed_close_timeout`, default `1s`). **D8 may fold it in opportunistically. Not a commitment.**
- **`M66-3` is PARTIALLY consumed** (`ADR-0132` decision 5): `67.1` fixed the `JoinSet` non-reaping half;
  the *unbounded per-connection drain* half became `CF-67-6`. **Do not record `M66-3` as fully consumed.**
- **`CF-67-5`** (upstream's *connection* behavior on an empty `filters: []` chain) stays open; §4 says this
  phase may close it opportunistically.
- **`CF-67-3`** (payload-visible `on_data` iteration + buffering) stays deferred, **scope unchanged**.
- **Still live, none blocks:** `CF-67-1`, `CF-67-2`, `M66-7`, `CF-66-1`, `M64-2`, `M64-3`, `M65-1`,
  `M57-1`, `M55-1`, `M53-2`, `M53-3`, `M48-2`, `M42-1`, the `DC`/retry-budget-overflow slices of `M45-2`,
  the phase-58 candidate carry-forward, `M40-1`, `M39-1`/`M39-2`, `M38-1`/`M38-2`, `CF-39-1`, `M37-*`,
  `M36-*`, `M34-*`, `M33-*`, the empty-`metadata_match` doc-comment, `M29-*`/`M30-*`, the phase-31
  cosmetics, and the HTTP-filters-family (1)-(4).
- **Numbering: `M66-1` was never allocated.** The ledger advances monotonically and does not backfill.
