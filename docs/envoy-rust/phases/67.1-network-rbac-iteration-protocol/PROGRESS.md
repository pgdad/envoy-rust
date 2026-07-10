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

---

# Phase 67.1 — state-4 verification

> Written by the §5 **state-4 verification** session (`superpowers:verification-before-completion`).
> This section is APPENDED; the state-3 log above is unmodified. Every command below was run fresh
> in this session at `HEAD = cd874f607489260e606fe8e576326188e2d9c46b` (working tree clean, branch
> `main`, `origin/main` at the same SHA). **No code changed in this session** — state-4 verifies, it
> does not implement.
>
> **Result: §7.5 (a)-(e) are SATISFIED. (f) is not — `REVIEW.md` is the state-5 session's output.**
> **No new ADR was needed.** Ledger head remains **ADR-0131**; next available **ADR-0132**.

## Gate (e) — build, clippy, fmt, test, deny

### `cargo build --workspace --all-targets` — exit **0**

```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s
```

### `cargo clippy --workspace --all-targets --all-features -- -D warnings` — exit **0**

```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
```

**A cached `Finished` is weak evidence, so the lint was re-run against a forced recompile.** The
state-3 session's own hard-won lesson (Task 8: a mutation check reported a FALSE PASS from a stale
test binary) applies verbatim to a verification gate: an "exit 0" from a fully-cached invocation
proves only that cargo's fingerprints matched. After `touch`ing the four crate roots this phase
modified (`envoy-config/src/lib.rs`, `envoy-listener/src/lib.rs`, `envoy-bin/src/main.rs`,
`tests/differential/src/lib.rs`), clippy re-checked **14 units** and still exited **0** with zero
warnings:

```
    Checking envoy-config v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-config)
    Checking differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Checking envoy-listener v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-listener)
    Checking envoy-cluster v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-cluster)
    Checking envoy-filter v0.1.0 (/home/esa/git/envoy-rust/crates/envoy-filter)
    Checking envoy-tls v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-tls)
    Checking envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
    Checking envoy-tcp v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-tcp)
    Checking envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
    Checking envoy-health v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-health)
    Checking envoy-admin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-admin)
    Checking http1-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http1-echo-server)
    Checking envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Checking http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.78s
```

> Note for future sessions: `cargo clippy` emits `Checking`, not `Compiling`, for lib targets. Grep
> for **both** when auditing whether a lint or a mutation check actually re-ran.

### `cargo fmt --all -- --check` — exit **0**, zero bytes of output

### `cargo deny check` — exit **0**

```
advisories ok, bans ok, licenses ok, sources ok
```

Plus 5 pre-existing `warning[license-not-encountered]` advisories (`0BSD`, `BSD-2-Clause`,
`MPL-2.0`, `Unicode-DFS-2016`, `Zlib` — allow-listed in `deny.toml` but unmatched by the current
dependency set). Warnings, not errors; the check passes. **No freshly-published advisory fired this
session** (contrast RUSTSEC-2026-0190 / `anyhow`, which reddened an earlier phase's push).

### `cargo test --workspace --no-fail-fast` — exit **101** locally; **1886 passed, 6 failed, 9 ignored**

`--no-fail-fast` is mandatory: the bare form aborts at the first failing test *binary* and never
reaches the rest of the gate. Full output was redirected to a file, never piped through `tail`.

**All 6 REDs are adjudicated below. None is a phase-67.1 regression.** Each was re-run **in
isolation** to classify it per the standing rule (environmental ⇒ fails deterministically alone;
parallel-load flake ⇒ passes alone):

| # | Test | Fixture | Isolated re-run | Verdict |
|---|---|---|---|---|
| 1 | `access_log_rf_upstream_reset` | `0061` | **FAILED** (deterministic) | environmental |
| 2 | `access_log_rcd_upstream_reset` | `0062` | **FAILED** (deterministic) | environmental |
| 3 | `access_log_h2_uc_upstream_reset` | `0069` | **FAILED** (deterministic) | environmental |
| 4 | `access_log_h2_rcd_upstream_reset` | `0070` | **FAILED** (deterministic) | environmental |
| 5 | `admin_config_dump_server_info` | `0014` | **FAILED** (deterministic) | environmental |
| 6 | `xds_file_based_eds_fixture` | — | **PASSED** (`ok. 1 passed`) | parallel-load flake |

This is exactly the documented "invariant core of ~5 + a varying tail" shape.

**Why 1-4 are environmental, from the failure text itself:** it is **upstream Envoy**, not
envoy-rust, that produces the unexpected value. Envoy cannot reach the host-spawned close backend
and reports a connect failure against an IPv6 address:

```
envoy=     {"rc":503,"rcd":"upstream_reset_before_response_started{remote_connection_failure|
           immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:37791}","rf":"UF"}
envoy-rust={"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}
```

envoy-rust emits the **correct** `UC` / `{connection_termination}`; the reference proxy never
reached the backend to observe a reset. Host limitation, not a divergence.

**Why 5 is environmental:** the diff is entirely host-bridge endpoint identity —
`backend::192.168.65.2:40673::…` appears `envoy-only`. This dev host routes the backend via
`192.168.65.2`.

**Why 6 is a flake:** `upstream Envoy never became accept-ready … 127.0.0.1:55082 not accept-ready
within 10s: Connection refused` — the known ephemeral-port-reuse startup race under parallel `cargo
test` load. It passes cleanly on its own.

### The decisive cross-check: **CI passed = local passed + local failed**

CI's `cargo test --workspace` on this exact SHA reports **1892 passed, 0 failed**. Locally:
**1886 passed + 6 failed = 1892**. The six local REDs are precisely the six tests CI passes — the
set is fully accounted for, with **no test silently missing from either side**. CI is authoritative
(doctrine D-3.3 + D-3.6), and CI is green.

## Gate (a) — all new/changed differential fixtures are green

Run locally against the pinned reference image (`envoyproxy/envoy:v1.33.0`, digest verified in this
session as `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, D-3.7), after
`cargo build -p envoy-bin` (the harness executes `target/debug/envoy-bin`, not release):

```
Running tests/network_filter_rbac_deny.rs        test result: ok. 1 passed; 0 failed   (fixture 0072, NEW)
Running tests/network_filter_rbac_allow.rs       test result: ok. 1 passed; 0 failed   (fixture 0073, NEW)
Running tests/echo.rs                            test result: ok. 1 passed; 0 failed   (fixture 0001, echo restructured)
Running tests/network_filter_direct_response.rs  test result: ok. 1 passed; 0 failed   (fixture 0071, direct_response restructured)
```

And in CI on this SHA: `test network_filter_rbac_deny_fixture ... ok`,
`test network_filter_rbac_allow_fixture ... ok`.

In-process backstops for the same surface, all green:

```
Running crates/envoy-bin/tests/network_filter_rbac.rs           10 passed; 0 failed
Running crates/envoy-bin/tests/network_filter_direct_response.rs 3 passed; 0 failed
Running unittests crates/envoy-config/src/lib.rs               570 passed; 0 failed
Running unittests crates/envoy-listener/src/lib.rs               50 passed; 0 failed
Running unittests crates/envoy-bin/src/main.rs                   30 passed; 0 failed
Running unittests tests/differential/src/lib.rs                 156 passed; 0 failed; 2 ignored
```

## Gate (b) — all pre-existing differential fixtures are still green

Every pre-existing fixture passes except the five environmental REDs adjudicated above
(`0061`/`0062`/`0069`/`0070`/`0014`) and the one startup-race flake, **all six of which CI passes on
this SHA**. The `echo` (`0001`) and `direct_response` (`0071`) fixtures deserve explicit mention:
Tasks 7 and 8 rewrote both filters onto the new `ConnectionHandler` trait, so they are the
regression witnesses for the accept-loop hoist — and both are green.

## Gate (c) — conformance suites

**`h2spec` is the project's only §7.3 conformance suite.**

- **Locally it SKIPPED — it is not a local pass.** The `h2spec` binary is absent from this host and
  `h2spec_runner` `eprintln!`-skips by design (`tests/conformance/h2spec/tests/h2spec_runner.rs:24`).
  The binary reports `3 passed`, but the gate test `h2spec_pass_rate_gate` did no protocol work.
  Recording this honestly: **local h2spec evidence is worthless; CI is authoritative.**
- **In CI on this SHA it RAN and PASSED**: the `install h2spec` step fetches pinned h2spec `2.6.0`,
  and `test h2spec_pass_rate_gate ... ok`.
- **`tests/conformance/h2spec/known-failures.txt` is UNTOUCHED by phase 67.1** — verified:
  `git log --oneline f40a41e..HEAD -- tests/conformance/h2spec/known-failures.txt` returns **0
  commits**. It still carries its single active entry (`3.5/2`, the h2-codec invalid-preface
  foundation limitation). It was never trimmed. (This host scores `3.5/2` as a PASS while CI fails
  it, so trimming it from local evidence would break CI.)

Phase 67.1 adds no HTTP/2, HTTP/3, gRPC or WASM surface, so no other conformance suite is in scope.

## Gate (d) — fuzz — **RECORDED EXPLICITLY, not skipped in silence**

**§7.5 (d) is satisfied by the pre-existing `parse_bootstrap` fuzz target; NO new fuzz target was
added — see ADR-0128 §2.3.**

The rationale, restated so this stands alone: **network `rbac` parses nothing.** It peeks a single
byte that it never reads, and inspects `peer_addr` / `local_addr`. Its only untrusted-input surface
is the bootstrap config parser, which the `parse_bootstrap` target has covered since phase 01. §7.4
requires a new target only for a phase that "introduces a parser, codec, or filter" with a new
untrusted-input surface; this phase introduces none.

What phase 67.1 *did* add is a **corpus seed** exercising the new config keys through the deepest
parse path (`action: DENY`, nested `and_rules`/`or_ids`/`not_rule`/`not_id` combinators). It is
explicitly un-ignored and **proven tracked** — the fuzz corpus directory is `*`-ignored by default,
so a seed without an explicit `!` line is silently invisible to CI:

```
$ grep -n network_filter_rbac crates/envoy-config/fuzz/.gitignore
56:!corpus/parse_bootstrap/network_filter_rbac.yaml

$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_rbac.yaml
crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_rbac.yaml
```

CI's short-budget fuzz job ran clean on this SHA — all four targets, **zero crashes, zero leak or
`ERROR: libFuzzer` artifacts**:

```
fuzz parse_bootstrap          Done 180438 runs in 31 second(s)
fuzz jwt_parse                Done 4457438 runs in 31 second(s)
fuzz cdn_loop_parse           Done 4679313 runs in 31 second(s)
fuzz accesslog_format_parse   Done 2604836 runs in 31 second(s)
```

No `ci.yml` step was needed (memory `new-fuzz-target-needs-a-ci-yml-step` applies only to a NEW
target); the existing `fuzz` job already covers `parse_bootstrap` and picked the seed up.

## Gate (f) — `REVIEW.md` approved — **NOT SATISFIED, and must not be**

`REVIEW.md` does not exist. It is the output of the §5 **state-5** code-review session
(`superpowers:requesting-code-review`), which per §5.1 is a **separate session**. This session did
not chain into it. ADR-0127's one-off human-authorized override applies only to 5→6 and explicitly
names 4→5 as un-chainable.

## CI on the exact SHA

Confirmed with the **full 40-char SHA** (`gh run list --commit <short-sha>` silently returns `[]`):

```
$ gh run list --commit cd874f607489260e606fe8e576326188e2d9c46b --json databaseId,status,conclusion,headSha,workflowName
[{"conclusion":"success","databaseId":29093604633,"headSha":"cd874f607489260e606fe8e576326188e2d9c46b",
  "status":"completed","workflowName":"ci"}]
```

Both jobs genuinely executed — **not runner starvation** (`steps > 0` and a non-empty log; note
`runnerName` reads empty even on healthy runs, so it is not the discriminator):

| job | conclusion | steps |
|---|---|---|
| `build + test + lint` | success | 15 |
| `fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse, 30s each)` | success | 12 |

`gh run view 29093604633 --log` → **6,741,648 bytes**. `test result: FAILED` appears **zero** times.

## Verdict

**§7.5 (a), (b), (c), (d), (e) are satisfied.** (f) awaits the state-5 code-review.

- No code changed in this session. No fixture was weakened. `known-failures.txt` was not trimmed.
- No new ADR was required: nothing this session measured contradicts a landed decision.
- No carry-forward opened or closed. `CF-67-5` remains open and blocks nothing; `CF-67-1`,
  `CF-67-2`, `CF-67-3` and the long tail remain live.
- §6.1's mid-execution valve stays armed for a §5.2 re-entry at step 3 if `REVIEW.md` finds issues.
- **Ledger head: `ADR-0131`.** Next available: **`ADR-0132`**, unreserved.
