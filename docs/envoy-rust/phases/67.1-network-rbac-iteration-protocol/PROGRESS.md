# Phase 67.1 — state-3 implementation PROGRESS

> Written by the §5 **state-3 implementation** session (`superpowers:executing-plans`, with
> `superpowers:test-driven-development` on every task). All 14 `PLAN.md` tasks are landed.
> **The §7.5 phase-done gate is NOT run here — it belongs to the state-4 verification session.**
> This file records what each task did, what it measured, and what it left for state-4.

**Result: all 14 tasks committed, TDD on every one. Two ADR-worthy discoveries landed
(`ADR-0131`), and one plan assumption was proven wrong by measurement.**

---

## Headline: `SPEC.md` R-2 was wrong, and fixture `0072` found it

`SPEC.md` §2 R-2 concluded that network RBAC *"decides once per connection, at establishment,
before any downstream byte is read"*, and `PLAN.md` promoted that to a hard rule (*"(2) Do NOT add
an `on_data` hook"*). **The inference was wrong**, because every probe in that recon sent a payload
before reading. Fixture `0072`, whose first draft sent nothing, hung against upstream Envoy.

Measured this session against the pinned image (`envoyproxy/envoy:v1.33.0` @
`sha256:56da5afd…`, D-3.7), chain `[rbac(DENY, any), echo]`, `/stats` scraped between cases:

| # | client behavior | upstream Envoy | `rbac.denied` |
|---|---|---|---|
| A | connect; send nothing; read (4 s) | connection stays **OPEN** | **0** |
| B | connect; send one byte; read | zero bytes, **clean EOF** | **1** |
| C | connect; half-close (FIN, no data); read | **clean EOF** | **0** |
| D | connect; idle 2 s; send one byte; read | zero bytes, **clean EOF** | **1** |

Symmetric for `action: ALLOW`: a connect+close with no data leaves `rbac.allowed` at **0**.

**Upstream evaluates on the FIRST DOWNSTREAM BYTE** (`ONE_TIME_ON_FIRST_BYTE`). envoy-rust now
matches: `ChainHandler` peeks (without consuming) for the first byte before running the chain, and
skips the chain entirely when the client closes without sending. **The `NetworkFilter` trait shape
is unchanged and filters still never see payload**, so **CF-67-3 remains correctly deferred**.
Recorded as **ADR-0131**, which also drove `TcpProbeKind::WriteThenReadToEof` and the switch to
**delta-based** `expected_stats`.

---

## Per-task log

| # | Task | Commit | Tests | Notes |
|---|---|---|---|---|
| 1 | Config surface (D1) | `f64d94d` | `envoy-config` 548 → **555** | See "Task 1" below. |
| 2 | W-1 error text + `validate_rbac_rules` | `531577e` | **557** | Zero blast radius, as measured at PLAN-write. |
| 3 | Bilateral chain-termination (D2) + M66-6 | `eb38266` | **563** | Empty chain stays accepted (R-7). |
| 4 | CF-67-4 L4 leaf allow-list (D3) | `c112583` | **570** | **CF-67-4 closed.** |
| 5 | Iteration protocol (D4) — CF-66-2 | `8b610b3` | `envoy-listener` +3 | `tokio` gains `io-util`. |
| 6 | `ChainHandler` + reaping witness — M66-3 | `83aeecc` | +5 | Mutation-verified. |
| 7 | `echo` → `ConnectionHandler` (D5) | `92cb478` | 3 | Fixture `0001` green. |
| 8 | `direct_response` → `ConnectionHandler` — M66-4 | `0230db2` | 6 | ADR-0124 drain preserved; fixture `0071` green. |
| 9 | `network_rbac.rs` engine (D6) | `b1a2b46` | 10 | Exhaustiveness probe-verified. |
| 10 | `main.rs` chain dispatch + `filters: []` panic | `7bb6ea5` | 5 | Panic reproduced then fixed. |
| 11 | `Driver::TcpWithStats` (D7) | `be4730b` | `differential` 152 → **156** | Bilateral scrape extracted + shared. |
| 12 | Fixtures `0072` + `0073` (D8) | `c5ab6ca` | 2 differential | **ADR-0131.** Both proven non-vacuous. |
| 13 | In-process backstops (D9) | `7cdf277` | 10 | Three mutation checks. |
| 14 | `BEHAVIOR_CONTRACT` + corpus seed (D10) | `9cac6b8` | — | Seed proven tracked + valid. |

### Task 1 — one pre-existing test had to move

`rejects_unknown_filter_name` (phase 02.1) used `envoy.filters.network.rbac` as its example of an
*unknown* filter name — its own comment said *"rbac lands in phase 09's network-filter family"*.
Task 1 made that name known. The placeholder moved to `envoy.filters.network.sni_cluster`, placed
**before** a terminal `echo`, because every unknown name is by definition non-terminal and Task 3's
chain-termination rule would otherwise reject the chain before the allow-list is consulted. **The
assertion itself is unchanged.**

### Task 6 — the M66-3 witness is real

`cx_active` cannot witness reaping: it is decremented *inside* the spawned task, so it reads 0 while
a completed `JoinSet` entry still lingers. `accept_loop` now publishes `join_set.len()` on a
`watch<usize>`, exposed as `Listener::pending_tasks()` / `pending_tasks_watch()`.

**Mutation check performed:** deleting the `Some(done) = join_set.join_next()` select arm makes
`sequential_connections_do_not_accumulate_joinset_tasks` fail with
`JoinSet leaked 50 completed tasks across 50 sequential connections`. Restored, re-verified green.

### Task 8 — a mutation check that lied

The ADR-0124 drain mutation (`close_with_drain` → bare `shutdown()`) first reported **PASS**, which
would have meant `post_eof_client_write_is_accepted_not_reset` was vacuous. It was a **stale test
binary**: cargo skipped the rebuild, so the *unmutated* code ran.

Diagnosed with `superpowers:systematic-debugging`. A standalone TCP probe established the kernel
semantics independently (no-drain ⇒ `EPIPE` on the second post-EOF write, even at 0 unread bytes),
and a forced rebuild showed the test failing correctly with `BrokenPipe`. **The drain is
load-bearing; the test is a genuine witness.**

> **Discipline for state-4 and beyond:** a mutation check must confirm that cargo recompiled **the
> crate that was mutated** (grep the run for `Compiling <that crate>`). Checking the wrong crate's
> `Compiling` line produced this false pass twice.

### Task 9 — the exhaustiveness guard is real, not aspirational

`permission_matches` / `principal_matches` carry no `_ =>` catch-all (verified: the only `_ =>` in
the file is inside a doc comment). **Probe performed:** adding a `Permission::DestinationPort(_)`
arm — simulating a `67.2` connection-level matcher — breaks the compile with
`error[E0004]: non-exhaustive patterns` at `validate_l4_permission` (`bootstrap.rs:4309`) and at the
shared tree validator. Probe reverted. That compile break is what will force `67.2` to classify
every new arm.

`clippy::only_used_in_recursion` is allowed on both, with a note: `conn` is unread by `67.1`'s arms
(`any` + combinators) and is read by `67.2`'s IP/port arms. Keeping it in the signature now is what
lets `67.2` add those arms without touching every call site.

### Task 10 — the `filters: []` panic, before and after

```
BEFORE:  thread 'main' panicked at crates/envoy-bin/src/main.rs:220:14:
         validator guarantees ≥1 filter

AFTER:   WARN filter chain is empty; binding no data listener (upstream Envoy accepts
              this config and starts — see CF-67-5) listener=l
```

Reproduced against the committed tree at `0230db2`, then fixed. No `panicked` line remains.

### Task 12 — both fixtures are non-vacuous, and proven so

`0072` (DENY) and `0073` (ALLOW) are **green against upstream Envoy**. Vacuity check performed:
making `wrap_in_chain` return `inner` unconditionally (i.e. never building the chain) REDs `0073`
with `subject stat rbac_allow.rbac.allowed delta expected 1 got 0 (baseline 0, final 0)` and REDs
`0072`. Restored, re-verified green.

**`expected_stats` are post-probe DELTAS, not absolute values** (ADR-0131 decision 4). `run_fixture`
opens a readiness `TcpStream::connect` to each proxy's data port before the probe; baselining after
a settle isolates the probe's own effect. The witness property survives: `scrape_admin_stat` yields
`Ok(0)` for a name the proxy never registered, so an unimplemented filter snapshots 0 and finishes
at 0 — a delta of 0, which fails any non-zero expectation.

> This was found the hard way. The absolute form first failed with
> `subject stat rbac_allow.rbac.allowed expected 1 got 2`, which I initially attributed to Docker's
> port-mapping absorbing the readiness connect. The real cause was the first-byte semantics.
> Deltas are kept anyway: they are correct regardless of what else touches the listener.

### Task 13 — three mutation checks, all observed to fail first

1. Remove `close_with_drain`'s drain loop → `deny_post_eof_client_write_is_accepted_not_reset`
   fails (`BrokenPipe`), at both the `envoy-listener` unit layer and the `envoy-bin` backstop layer.
2. Tick `allowed` when `rules` is `None` → `rules_omitted_is_inert_neither_counter_ticks` fails
   with `INERT: norules.rbac.allowed must not tick`.
3. Remove `ChainHandler`'s first-byte `peek` → `connection_that_sends_nothing_is_never_evaluated`
   fails.

All three restored and re-verified. `validate_config` captures **stdout and stderr**: envoy-bin's
tracing subscriber writes the `ConfigError` line to stdout, so a stderr-only capture saw nothing.

### Task 14 — §7.4 disposition, unchanged

**NO new fuzz target** (ADR-0128 §2.3). Network `rbac` parses nothing: it peeks one byte it never
reads and inspects `peer_addr` / `local_addr`. Its only untrusted-input surface is the bootstrap
config parser, already covered by the pre-existing `parse_bootstrap` target. A **corpus seed** was
added, un-ignored explicitly, and **proven tracked**:

```
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_rbac.yaml
crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_rbac.yaml
```

The seed is a **valid** config (envoy-bin starts on it: listener bound, no panic, no error), so it
exercises the deepest parse path.

> **The state-4 session must RECORD §7.5 gate (d) EXPLICITLY** as *"satisfied by the pre-existing
> `parse_bootstrap` fuzz target; no new target — see ADR-0128 §2.3"*, not skip it in silence.

---

## Local verification at the end of state-3 (NOT the §7.5 gate)

```
cargo fmt --all -- --check                                          CLEAN
cargo build --workspace --all-targets                               Finished
cargo clippy --workspace --all-targets --all-features -- -D warnings Finished (no warnings)
cargo test -p envoy-config                                          570 passed; 0 failed
cargo test -p envoy-listener                                        50 passed; 0 failed
cargo test -p envoy-bin (all targets)                               all green
cargo test -p differential --lib                                    156 passed; 0 failed; 2 ignored
cargo test -p differential --test echo                              1 passed  (fixture 0001)
cargo test -p differential --test network_filter_direct_response    1 passed  (fixture 0071)
cargo test -p differential --test network_filter_rbac_deny          1 passed  (fixture 0072, NEW)
cargo test -p differential --test network_filter_rbac_allow         1 passed  (fixture 0073, NEW)
cargo test -p envoy-bin --test network_filter_rbac                  10 passed (NEW)
```

**The full `cargo test --workspace --no-fail-fast`, `cargo deny check`, the whole pre-existing
differential surface, and the conformance suites are the STATE-4 session's job** (§7.5 (a)-(f)).
Expect the documented environmental REDs on this host (`0061`/`0062`/`0069`/`0070`, the
parallel-load flakes); **CI is authoritative**.

---

## §6.1 mid-execution valve — did NOT fire

No task's sub-steps blew past ~10 items. Task 12 grew when ADR-0131 surfaced (peek + a third probe
kind + fixture rework + doc corrections ≈ 7 items), which is inside the budget. Task count and net
LoC are unchanged from `PLAN.md` §0's re-derivation.

## Carry-forward ledger at the end of state-3

- **CONSUMED:** `CF-66-2` (the iteration protocol — Tasks 5/6/10 *are* it), `M66-3` (both
  non-reaping accept loops **deleted**; the surviving loop's reaping now witnessed), `M66-4` (the
  stale doc-precision line rewritten), `CF-67-4` (the L4 leaf allow-list), `M66-6` (the
  dynamic/LDS-listener terminal test, folded into Task 3).
- **CLOSED by recon, no code change:** `M66-5` (config-load parity on the empty chain).
- **OPENED:** `CF-67-5` — probe upstream Envoy's *connection* behavior on an empty `filters: []`
  chain before asserting anything about it (ADR-0130 §2). Blocks nothing.
- **STILL LIVE, none blocks:** `CF-67-1` (`shadow_rules`), `CF-67-2` (`Action::LOG`), `CF-67-3`
  (payload-visible `on_data`-time iteration + buffering — **scope unchanged by ADR-0131**),
  `M66-7`, `CF-66-1`, and the long tail.
- **DEFERRED to `67.2`:** the connection-level matcher arms + `CidrRange` + the three-site V-1
  shared-enum fallout (`lower_permission`, `lower_principal`, `define_rbac_tree_validator!`).
- **Numbering:** `M66-1` was never allocated. The ledger advances monotonically and does not
  backfill.

## ADRs

- **`ADR-0131` (NEW, this session):** the first-byte correction, the `peek`, the
  `WriteThenReadToEof` probe, and delta-based `expected_stats`. **Overturns `SPEC.md` R-2's
  "at establishment" clause**; the rest of R-2 stands and is re-confirmed.
- **`ADR-0130`** (the PLAN-write reconciliation) and **`ADR-0129`/`ADR-0128`** govern.
- **`ADR-0124` is UNTOUCHED** and survived the accept-loop hoist, as required.
- **Ledger head: `ADR-0131`.** Next available: **`ADR-0132`**, unreserved.
