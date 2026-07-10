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

---

# Phase 67.1 — §5.2 state-3 RE-ENTRY (session 1 of N): recon + ADR-0132 + the §6.1 split

> Written by the §5.2 **state-3 re-entry** session (`superpowers:executing-plans`). `REVIEW.md`'s verdict
> was **NOT APPROVED**, so per §5.2 the phase resumed **implementation**, not verification.
> This section is APPENDED; the state-3 and state-4 logs above are unmodified.
>
> **Result: NO code changed. §6.1's MID-EXECUTION VALVE FIRED on the C-1 fix, and §6.2 carved the
> oversize task into a NEW sub-phase `67.3`.** `ADR-0132` lands with the measurement, the corrected
> model, and the split. **`67.1`'s remaining state-3 work is now tractable and is scoped below.**
> **Ledger head: `ADR-0132`.** Next available: **`ADR-0133`**, unreserved.

## What this session did, and why it changed no code

`REVIEW.md` C-1 required: *"Probe upstream before choosing a model, and probe ALL FOUR terminal
filters."* That recon is the first task of the re-entry, because the design is not decidable without it.
It was performed. **What it found made the fix larger than a task.**

## The recon — all four terminal filters, measured

Against the pinned image `envoyproxy/envoy:v1.33.0` @ `sha256:56da5afd…` (D-3.7), booted under
`docker run -p`; the `tcp_proxy` backend ran as a **sibling container** on a shared docker network,
speaking a banner **before** any client byte. `/stats` was scraped **mid-flight** — while the client
connection was still open — so the counter's trigger is disambiguated, not inferred.

`[rbac(any), <terminal>]`:

| terminal | connect, send nothing, stay open | connect + FIN, no data | connect + first byte | establishment work |
|---|---|---|---|---|
| `echo` | no tick; stays open | **no tick**; clean EOF | tick | **none** |
| `http_connection_manager` | no tick; stays open | **no tick**; clean EOF | tick (on the request) | **none** |
| `direct_response` | **payload written, clean EOF, NO tick** | same | same | **writes payload, closes** |
| `tcp_proxy` | no tick; **banner delivered; `upstream_cx_total: 1`** | **TICKS** | tick | **connects upstream** |

Plus: `[rbac(DENY), direct_response]` delivers the payload and closes cleanly with **all four counters at
`0`** — a DENY policy does **not** suppress the payload, because the terminal filter writes and closes
before any `onData` fires.

**Conclusion.** Upstream runs **every** filter's `onNewConnection` at establishment — the terminal
filter's included — and defers **only the RBAC verdict** to the first downstream byte. The `peek` belongs
to the **filter's decision point**, not the **chain's hand-off point**.

**Two further findings the state-5 review had not established:**
1. **`echo` and `hcm` are already exactly correct.** Their establishment work is nil, so `67.1`'s gate is
   observationally identical to the correct model — including the data-less-FIN no-tick. **No change.**
   `ADR-0131` stands and is re-confirmed; fixtures `0072`/`0073` are valid witnesses.
2. **A data-less FIN evaluates for `tcp_proxy` but NOT for `echo`/`hcm`.** The FIN semantic is a
   **per-terminal property** (half-close propagation), not a chain property. Nobody knew this.

## Why §6.1's mid-execution valve fired

`direct_response` is a trivial repair (bypass the chain; exact measured parity). **`tcp_proxy` is not.**
Faithful behavior needs the upstream connected at establishment, the server-first bytes flowing
immediately, the chain evaluated on the first downstream byte *or* a data-less FIN, and — on DENY — that
byte **never forwarded upstream**.

`ConnectionHandler::handle(&self, downstream: TcpStream)` **fuses establishment and data into one future
that owns the socket.** There is no seam. And `envoy_tcp::TcpProxy::handle::<S>` is **generic over the
stream type** (for upstream TLS), so it cannot `peek` at all; its connect / tick / bidirectional-copy body
must be split. The work reaches `envoy-listener` (the trait + `ChainHandler`), `envoy-tcp`, `envoy-bin`
(`TlsAcceptingHandler`, whose pre-handshake chain placement is itself **unmeasured**), plus the FIN
semantics and their tests.

**That is well past §6.1's "any single task's sub-steps blow up past ~10 items once contact with reality
reveals complexity."** Per **§6.2 step 1**: *"Stop. Do not continue … implementing the oversize task."*

## The split (§6.2, ADR-0132)

- **NEW `docs/envoy-rust/phases/67.3-network-filter-establishment-phase/SPEC.md`** — the
  establishment/data-phase split + the correct `[rbac, tcp_proxy]` composition. ~690 net LoC / ~8-10 tasks.
- **ROADMAP:** row `67.3` enters `planned` (depends-on `67.1`); parent row `67`'s `sub-phases` cell
  becomes `67.1, 67.2, 67.3`. Verified with an escape-aware split (6 cells). Parent `67` flips `done`
  only when **all three** sub-phases are `done`.
- **Until `67.3` lands, `[rbac, tcp_proxy]` is REJECTED AT CONFIG LOAD, fail-loud** (`ADR-0049`
  decision-2 (b)). A recorded divergence — upstream accepts it — and **strictly better than the shipped
  behavior, which is a runtime deadlock.** `67.3` deletes the rejection. This is **not** a §6.3 stub: it is
  a loud rejection plus a follow-on with its own ROADMAP row.

## `REVIEW.md` I-3, resolved by ADR-0132 decision 5

**`M66-3` is recorded as PARTIALLY consumed.** `67.1` fixed the `JoinSet` non-reaping half (witnessed by
`Listener::pending_tasks()`). It did **not** bound the drain: `close_with_drain` reads to client EOF with
no steady-state timeout, bounded only at shutdown by `DRAIN_BUDGET`. **That half becomes `CF-67-6`.**
`ADR-0124` is untouched; both post-EOF-write tests stay unweakened.

## `67.1`'s REMAINING state-3 scope (the next session — all normal-sized tasks)

1. **ADR-0132 decision 2** — `direct_response` **bypasses** the chain (exact measured parity, incl. DENY).
2. **ADR-0132 decision 4** — the `[rbac, tcp_proxy]` **fail-loud config-load rejection** + `BEHAVIOR_CONTRACT.md` row naming `67.3` as owner.
3. **`REVIEW.md` I-2** — `RbacMetadataMatcherInvalid` still renders `"HCM listener …"` for a network `rbac` filter (reproduced); its guarding comment has the validation order **backwards**. Generalize the message; fix the comment. **Do NOT reorder the L4 walk ahead of `validate_rbac_rules`** — the current order bounds tree depth first.
4. **`REVIEW.md` I-4** — `Listener::pending_tasks()` is last-writer-wins under the SO_REUSEPORT fan-out.
5. **`REVIEW.md` I-5** — composition tests: `[rbac, direct_response]` (cheapest) and `[rbac, hcm]`.
6. **The eight Minors** (M-1 … M-8).

Then a **separate** state-4 verification session, then a **separate** state-5 code-review. `REVIEW.md` is
superseded only by that later review — **never edited** (D-3.5).

## §6.1 valve — FIRED (contrast the original state-3, where it did not)

The original state-3 log above records *"No task's sub-steps blew past ~10 items."* That was true of the
plan as written. It stopped being true the moment C-1's measurement revealed that the terminal filter has
an establishment phase. **The valve exists for exactly this.**

## The methodological lesson (recorded in ADR-0132, repeated here on purpose)

`ADR-0131` fired because every probe in the `SPEC.md` R-2 recon happened to **send a payload first**.
**C-1 fired because every probe in the `ADR-0131` recon happened to use `echo`** — the one terminal filter
with no establishment-time behavior. **When a measurement generalizes over a population of one, it has not
been measured.** The §7.5 (a)-(e) gate was green and truthful throughout and could not have caught it.

---

# Phase 67.1 — §5.2 state-3 RE-ENTRY (session 2 of N): the C-1 repair lands

> Written by the §5.2 **state-3 re-entry** session (`superpowers:executing-plans`, with
> `superpowers:test-driven-development` on every task). `REVIEW.md`'s verdict is **NOT APPROVED**,
> so per §5.2 the phase is still in **implementation**, not verification.
> This section is APPENDED; the state-3, state-4 and re-entry-session-1 logs above are unmodified.
>
> **`REVIEW.md` was NOT edited** — a review is superseded only by a LATER review (D-3.5).
> **`DECISIONS.md` ledger head is still `ADR-0132`.** No new ADR was needed: every decision this
> session implements was already made and recorded by `ADR-0132`. **Next available: `ADR-0133`,
> unreserved.**
>
> **Per §5.1 this session did NOT chain into state-4.** `superpowers:verification-before-completion`
> was deliberately not run; the §7.5 (a)-(f) gate is a **separate, later** session.

## What this session did

`ADR-0132` (session 1) performed the C-1 recon, fired §6.1's mid-execution valve on the `tcp_proxy`
establishment/data-phase split, and carved that into sub-phase `67.3`. It changed **no code**, and
left `67.1` a scope of six normal-sized tasks. **All six are now implemented, TDD, one commit each.**

| # | commit | scope | source |
|---|---|---|---|
| 15 | `d066f72` | `direct_response` BYPASSES the chain | `ADR-0132` decision 2 + `REVIEW.md` I-5 |
| 16 | `62849e7` | `[rbac, tcp_proxy]` rejected at config load, fail-loud | `ADR-0132` decision 4 |
| 17 | `ab22231` | `RbacMetadataMatcherInvalid` is scope-neutral | `REVIEW.md` I-2 |
| 18 | `62ea186` | `pending_tasks()` is a TOTAL, not last-writer-wins | `REVIEW.md` I-4 |
| 19 | `4e43038` | the `[rbac, hcm]` composition tests | `REVIEW.md` I-5 + M-3 |
| 20 | `641ce42` | the eight Minors M-1 … M-8 | `REVIEW.md` §2 |

`REVIEW.md` **I-3 needed no code** — `ADR-0132` decision 5 already resolved it on the ledger
(`M66-3` recorded **PARTIALLY** consumed; the unbounded steady-state drain became **`CF-67-6`**).

## Task 15 — `direct_response` bypasses the chain (ADR-0132 decision 2)

**RED first, and the RED is the bug.** Two new in-process backstops failed against the pre-fix tree
with exactly the hang `REVIEW.md`'s R1/R2/R3 measured against `target/debug/envoy-bin`:

```
---- direct_response_delivers_payload_to_a_client_that_sends_nothing stdout ----
panicked at crates/envoy-bin/tests/network_filter_rbac.rs:362:10:
direct_response must write its payload without any client byte: Elapsed(())

---- deny_does_not_suppress_the_direct_response_payload stdout ----
panicked at crates/envoy-bin/tests/network_filter_rbac.rs:407:33:
DENY must still deliver (send_first_byte=false)
```

**The fix.** `crates/envoy-bin/src/main.rs`'s `DIRECT_RESPONSE_FILTER` arm no longer calls
`wrap_in_chain`; it hands the connection straight to `DirectResponseHandler`.
`build_network_filter_chain` still runs — that is what **REGISTERS** the four `<stat_prefix>.rbac.*`
counters at `0` so the stat tree matches — and the filters are then dropped, unused. That is the
point: with `direct_response` terminal, upstream never evaluates RBAC at all, because the terminal
filter writes and closes before any `onData` can fire. **A DENY policy does not suppress the
payload.** Exact measured parity, and a simplification rather than a special case.

Both tests are mutation checks: restore `wrap_in_chain` on that arm and they fail again.

**Two docs stated the now-falsified uniformity claim and were corrected alongside:**
`ChainHandler`'s rustdoc (which now carries the per-terminal wrappability table and names
`ADR-0130` Decision 2 as superseded) and `direct_response.rs`'s module doc (whose "writes its
payload IMMEDIATELY — without reading or waiting for any client bytes" contract was **false** under
the chain; the bypass makes it true again).

**`echo` and `http_connection_manager` were NOT touched.** Measured already correct. `ADR-0131` is
not reverted: the first-byte *verdict* stands.

## Task 16 — `[rbac, tcp_proxy]` rejected at config load, fail-loud (ADR-0132 decision 4)

New `ConfigError::UnsupportedNetworkFilterChainComposition { listener, chain_index, non_terminal,
terminal }`, raised in `validate()`'s network-filter chain pre-pass.

**Placed AFTER both terminal-position checks**, so their errors keep winning — `[echo, rbac,
tcp_proxy]` still reports terminal-not-last. Pinned by the new
`terminal_not_last_error_wins_over_unsupported_composition`.

**Only `tcp_proxy` is rejected.** `echo`/`hcm` do no establishment-time work; `direct_response`
bypasses the chain (task 15). Over-rejection guards land alongside so fixture `0003` cannot regress:
`lone_tcp_proxy_chain_is_still_accepted` (config layer) and `tcp_proxy_alone_is_still_accepted`
(against the real binary).

**The error message names phase `67.3`.** Never silent (ADR-0132 decision 4). `67.3` deletes both the
rejection and the variant.

**A methodological note, because it repeated the M-2 lesson before M-2 was fixed.** The RED run for
this task did not fail — **it hung.** `validate_config` ran an ACCEPTED config to completion, and a
valid config makes `envoy-bin` serve forever. That is precisely `REVIEW.md` M-2's failure mode, one
task early. The helper is now bounded (`VALIDATE_BUDGET`, `kill_on_drop`): past the budget it reports
"did not reject", and the caller's `assert!(!ok, …)` fails with a useful message. The RED then read:

```
---- rbac_before_tcp_proxy_is_rejected_at_config_load stdout ----
panicked at crates/envoy-bin/tests/network_filter_rbac.rs:601:5:
[rbac, tcp_proxy] must be rejected until 67.3
```

`BEHAVIOR_CONTRACT.md` gains **item 13**, the full measured per-terminal composition table, and its
**item 1** is corrected: the first-byte rule is a property of the RBAC **verdict**, not of the
chain's hand-off, and the data-less-FIN semantic is **per-terminal** (it evaluates for `tcp_proxy`,
not for `echo`/`hcm`) — owned by `67.3`.

## Task 17 — `RbacMetadataMatcherInvalid` is scope-neutral (REVIEW.md I-2)

**Reproduced first**, verbatim as the review described:

```
---- structurally_invalid_metadata_leaf_is_not_reported_as_an_hcm_error stdout ----
panicked: a network rbac filter has NO HCM; the message must be scope-neutral. got
error=HCM listener "rbac_listener": RBAC policy "p0" metadata matcher at permissions[0]
is invalid: metadata matcher `filter` must not be empty
```

The guarding comment claimed the variant was unreachable from a network `rbac` filter "because a
network rbac filter's `metadata` leaf is rejected outright by `validate_l4_permission` (67.1 D3)
before that error can be reached." **The validation order is the reverse.** `validate_rbac_rules`
runs FIRST and validates `Metadata` leaves structurally, so a malformed leaf (empty `filter`,
multi-segment `path`) raises this variant before the L4 walk ever sees it.

Fixed by generalizing the message to `"listener {listener:?}: …"` — a **seventh** shared
scope-neutral variant; it stays accurate for the HTTP filter, whose listener *is* an HCM listener.
**Zero test blast radius**: no test asserted the old text (the four phase-35 tests match on the
variant, not the string).

**The L4 walk was deliberately NOT reordered ahead of `validate_rbac_rules`.** That order is what
bounds tree depth before the L4 recursion descends the same tree — a stack-safety guarantee pinned by
`network_rbac_depth_bound_precedes_the_l4_walk`. The comment now says so, at both sites.

Two config-layer tests pin which validator owns which input:
`structurally_invalid_metadata_leaf_reports_a_scope_neutral_listener_error` (a mutation check:
restore `"HCM listener"` and it fails) and `well_formed_metadata_leaf_is_rejected_by_the_l4_walk_instead`.

## Task 18 — `pending_tasks()` is a TOTAL (REVIEW.md I-4)

`Listener::serve`'s SO_REUSEPORT fan-out `.clone()`d **one** `watch::Sender` per accept loop, and each
loop called `send_replace(join_set.len())` with **its own socket's** count. `watch::Sender` clones
share one channel, so the published value was neither a total nor stable — it flapped to whichever
loop wrote last — while the `pub` accessor documented it as "in-flight connection tasks."

Replaced by a `PendingTasks` aggregator: **one slot per accept loop**, each loop publishing only into
its own slot, the total recomputed and broadcast **under the lock**. A `std::sync::Mutex` (held across
no `.await`; taken once per accept and once per reap) rather than lock-free atomics, because the
publish is a read-modify-broadcast: two loops summing lock-free could each observe a stale peer and
the later `send_replace` would clobber the correct total.

**Mutation-checked, with the rebuild confirmed** (memory `mutation-check-needs-forced-rebuild`):
`Compiling envoy-listener` appears in the run, so the PASS is not a stale binary. Reverting `publish`
to write its own count instead of the sum fails with `left: 4, right: 7` — exactly the old
last-writer-wins reading.

The **M66-3 reaping witness is unaffected**: it uses the single-socket `Listener::bind` path, where
one slot is the identity. Pinned by `pending_tasks_single_slot_is_the_identity`. `bind_shards` keeps
its per-shard aggregator (one socket ⇒ one slot), which is what made the old inconsistency a smell.

**A latent panic surfaced while doing this, and it is worth recording.** The in-crate
`mk_multi_socket_listener` test helper hard-coded ONE slot while taking N sockets, so
`reuseport_fanout_serves_and_drains` panicked `index out of bounds: the len is 1 but the index is 1`
on a `tokio-rt-worker` — **but only when the kernel happened to steer the connection to socket #2.**
The first full run passed; the next failed. Sized from `listeners.len()`, and `PendingTasks::slot`
now `debug_assert`s the bound **at the seam where the slot is minted**, rather than leaving it to fire
nondeterministically deep inside a spawned accept loop. Re-run 5× green, and the fan-out test alone
5× green.

## Task 19 — the `[rbac, hcm]` composition tests (REVIEW.md I-5, and M-3)

I-5 named the process defect that let C-1 ship: `main.rs` wrapped all four terminal arms; the suite
exercised exactly one. `rbac` was composed with `echo` in **every fixture and every backstop, and with
nothing else, anywhere** — so the three untested combinations were exactly the three broken ones.

`[rbac, direct_response]` is covered by task 15. This adds the other composition `67.1` owns —
`[rbac, http_connection_manager]` — for both verdicts. The HCM routes to a `direct_response` route, so
it needs **no backend**:

- `rbac_before_hcm_evaluates_on_the_first_request` — ALLOW yields to the terminal HCM, the request is
  served `200`, `allowed` ticks exactly once.
- `deny_before_hcm_writes_nothing_and_ticks_denied_once` — DENY writes **zero bytes**, not even a
  `403`; clean EOF; `denied == 1`.

The second also **closes M-3**: in-process, `denied` was asserted `== 0` twice and never `== 1`, so
the positive tick rode entirely on the Docker-gated fixture `0072`. It now has a Docker-independent
witness.

**`[rbac, tcp_proxy]`'s POSITIVE test belongs to `67.3`.** `67.1` tests only that it is *rejected*.

## Task 20 — the eight Minors

- **M-1** — `shadow_counters_register_at_zero_and_never_tick` was **vacuous at the registration
  half**: its `stat()` helper called `register_counter`, which is **get-or-create**, so it would have
  minted each counter and read `0` even had `NetworkRbacFilter::new` registered nothing. A new
  non-creating `registered_stat()` reads `StatsRegistry::snapshot()` instead, and the test now also
  pins `allowed`. **Mutation-checked with the rebuild confirmed**: deleting the `shadow_allowed`
  registration fails with `counter s.rbac.shadow_allowed must be REGISTERED by NetworkRbacFilter::new`.
- **M-2** — three backstops called `read_to_end` with no timeout. Every read in the file is now bounded
  by a named `READ_BUDGET`. (See task 16: this failure mode bit this very session, in `validate_config`.)
- **M-3** — closed by task 19.
- **M-4** — documented at the site: because the chain-termination check precedes the per-filter
  allow-list loop, a chain ending in an **unknown** filter name reports
  `NetworkFilterChainNotTerminated` rather than `UnsupportedFilter` (an unknown name is by definition
  not terminal). Both fail loudly; only diagnostic precision is lost.
- **M-5** — `rules: { policies: {} }` is rejected (`EmptyRbacPolicies`), but **upstream's behavior for
  the NETWORK filter on that input was never measured** (SPEC R-3 measured only `rules` *omitted*).
  Recorded as `BEHAVIOR_CONTRACT.md` **item 9b** rather than left as an implied parity claim.
- **M-6** — `ChainHandler::handle` propagated a `peek` error as a task failure, so a client resetting
  before its first byte was logged by `accept_loop` as `connection task failed`. A reset-before-data is
  the `Ok(0)` case arriving rudely: no decision, no counter, and nothing to drain. Now `debug!` +
  `Ok(())`.
- **M-7** — nothing pinned that `action: LOG` — a real upstream RBAC action — is rejected rather than
  silently treated as ALLOW or DENY. New `log_action_and_unmodeled_rbac_fields_are_rejected` covers
  `LOG`, `enforcement_type` and `delay_deny`, locking **CF-67-2**'s boundary.
- **M-8** — only the first listener is served. **Pre-existing, not introduced by `67.1`.** Noted at
  `main.rs` so no future session reads `.next()` as new.

## Evidence (per-task; the §7.5 gate is state-4's, and was NOT run)

Every command below was run to completion with full output captured — **never piped through `tail`**
(memory `never-pipe-verification-runs-through-tail`), and always with `--no-fail-fast` (the bare
`cargo test --workspace` aborts at the first failing BINARY).

```
cargo test -p envoy-bin --test network_filter_rbac  → 18 passed; 0 failed
cargo test -p envoy-bin --bins                      → 30 passed; 0 failed
cargo test -p envoy-listener                        → 52 passed; 0 failed   (x5 runs, green each)
cargo test -p envoy-config                          → 575 passed; 0 failed
cargo test -p envoy-filter                          → 206 passed; 0 failed
cargo fmt --all -- --check                          → clean
cargo build --workspace --all-targets               → clean
cargo clippy --workspace --all-targets --all-features -- -D warnings → clean
```

The workspace `clippy` and `fmt --check` were run **as a guard against a red-at-fmt CI**, not as a
state-4 claim (memory `envoy-rust-state4-ci-first-execution`). **§7.5 (a)-(f) has NOT been executed
this session** — no Docker differential, no conformance suites, no `cargo deny`, no
`cargo test --workspace`. That is the next session's job.

## Scope discipline

**§6.1's mid-execution valve stayed ARMED all session and did NOT need to fire again.** No task's
sub-steps blew past ~10 items. The largest, task 18, touched one crate and one type.

**Nothing forbidden was done.** `ADR-0131` not reverted. `echo`/`hcm` untouched. No attempt to fix
`[rbac, tcp_proxy]` (that is `67.3`). No `_ =>` catch-all added to the four exhaustive RBAC match
sites. `rbac` not added to `is_terminal_network_filter`. `filters: []` still accepted. `ADR-0124`'s
drain untouched and **both** post-EOF-write tests unweakened. `crates/envoy-filter/src/rbac.rs` (the
HTTP filter sharing the name) never edited. No ROADMAP row changed. `REVIEW.md` never edited.

## Where `67.1` stands

**All of `REVIEW.md`'s blocking findings are addressed**: C-1 (tasks 15 + 16), I-2 (17), I-3
(`ADR-0132` decision 5, ledger-only), I-4 (18), I-5 (15 + 19), and M-1 … M-8 (19 + 20).

**§7.5 (f) is still UNMET** — a review is superseded only by a LATER review (D-3.5), and `REVIEW.md`
is untouched. **The next session runs state-4** (`superpowers:verification-before-completion`, full
§7.5 (a)-(f)), and a **separate** session after it runs the state-5 code-review. **Do not chain**
(§5.1; `ADR-0127` names 3→4 explicitly as un-chainable).

**Carry-forward ledger — unchanged by this session.** `CF-67-6` (bound `close_with_drain`'s
steady-state drain) stays open, as `ADR-0132` decision 5 recorded; this session did not touch the
drain. `CF-67-5` stays open. `CF-67-1`/`CF-67-2`/`CF-67-3` unchanged in scope — M-7 *pins* CF-67-2's
boundary, it does not consume it. `M66-3` remains **PARTIALLY** consumed. `M66-1` was never allocated;
the ledger does not backfill.

---

# Phase 67.1 — §5 STATE-4 (verification-before-completion) — the FULL §7.5 (a)-(f) gate

> Written by the §5 **state-4 verification** session (`superpowers:verification-before-completion`),
> per `BOOTSTRAP_PROMPT.md` §5 state 4 + `SKILL_ROUTING.md`. This section is standalone (D-3.4).
> **No code changed this session.** Cold-started clean at `HEAD` = `origin/main` = `a21c983` on
> branch `main` (`git status --porcelain` empty; `git fetch origin --prune` showed no sibling ahead —
> `origin/main` still `a21c983`). `67.1` is IMPLEMENTATION-COMPLETE; every `REVIEW.md` blocking finding
> landed across `d066f72`..`641ce42` (handoff `a21c983`). This session re-ran the **entire** §7.5 gate
> against the current tree, because the ORIGINAL state-4's (a)-(e) evidence is now STALE (six commits of
> new code landed after it). Every command was run to completion, full output captured, **never piped
> through `tail`**, and `cargo test` **always with `--no-fail-fast`**.

## Gate result summary

| Gate | Command | Result |
|---|---|---|
| (a) build | `cargo build --workspace --all-targets` | **PASS** (exit 0) |
| (e) clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **PASS** (exit 0) |
| (e) fmt | `cargo fmt --all -- --check` | **PASS** (exit 0) |
| (e) deny | `cargo deny check` | **PASS** (exit 0) |
| (e) test | `cargo test --workspace --no-fail-fast` | **1901 passed**; 6 environmental REDs (run 1) / 8 (run 2), all adjudicated below; phase surface GREEN |
| (b) differential | (part of the workspace test run; `target/debug/envoy-bin` fresh at `a21c983`) | phase fixtures `0072`/`0073`/`0071`/`0003` GREEN; only the documented environmental core REDs |
| (c) conformance | `h2spec` runner (part of the workspace test run) | **locally eprintln-SKIPPED** (h2spec binary absent on this host); CI-authoritative; `known-failures.txt` **NOT trimmed** |
| (d) fuzz | — | **NO new fuzz target** (SPEC §5); the new `ConfigError` variant is reached through the pre-existing `parse_bootstrap` target. Recorded, not skipped in silence. |
| (f) review | `REVIEW.md` approved | **UNMET, and stays unmet.** A review is superseded only by a LATER review (D-3.5). State-4 does not satisfy it; the SEPARATE state-5 code-review session does. |

## (a) `cargo build --workspace --all-targets`

```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s
EXIT_CODE=0
```

(No-op finish: the tree is clean at `a21c983` and `target/` was warm; cargo's freshness check confirms
the current source compiles — exit 0.)

## (e) `cargo clippy --workspace --all-targets --all-features -- -D warnings`

```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
EXIT_CODE=0
```

## (e) `cargo fmt --all -- --check`

```
EXIT_CODE=0
```

(No output — every file already formatted.)

## (e) `cargo deny check`

```
advisories ok, bans ok, licenses ok, sources ok
EXIT_CODE=0
```

No freshly-published advisory this session; no dep patch-bump needed (memory
`cargo-deny-reds-on-unrelated-advisory`).

## (e) `cargo test --workspace --no-fail-fast`

Run **twice** for set-stability (memory `local-red-set-varies-run-to-run`):

```
Run 1:  1901 passed;  6 failed   (passed+failed = 1907)
Run 2:  1899 passed;  8 failed   (passed+failed = 1907)
```

The **total (1907) is identical** across runs; only *which* subset flips red under full-workspace
parallel load varies. The passing count of the deterministic-pass set is stable.

### Phase-67.1 surface — GREEN in both runs (this is what state-4 must witness)

```
network_filter_rbac_allow  (fixture 0073) → ok. 1 passed; 0 failed
network_filter_rbac_deny   (fixture 0072) → ok. 1 passed; 0 failed
network_filter_direct_response (0071 diff)→ ok. 1 passed; 0 failed   (+ envoy-bin side: 3 passed)
tcp_proxy                  (fixture 0003) → ok. 1 passed; 0 failed
network_filter_rbac  (18 in-process backstops) → ok. 18 passed; 0 failed
```

`0071` (`direct_response` alone) and `0003` (`tcp_proxy` alone) were flagged as the two most plausibly
broken by last session's C-1 changes (the `direct_response` chain-bypass + the `[rbac, tcp_proxy]`
rejection). **Both are GREEN**, as are the phase's own `0072`/`0073` and all 18 backstops.

### Failing-set adjudication (`--no-fail-fast`, 2 runs, then each member re-run in ISOLATION)

**Deterministic environmental CORE (5) — fails in isolation, invariant across both runs, CI-authoritative:**

| Test | Signature | Memory |
|---|---|---|
| `access_log_h2_rcd_upstream_reset` | upstream `remote_connection_failure\|…Network_is_unreachable\|remote_address:[fdc4:…]:… rf:UF` vs rust `connection_termination rf:UC` | `tcpclosebackend-ipv6-unreachable-host-flake` |
| `access_log_h2_uc_upstream_reset` | same IPv6-close-backend-unreachable shape | ″ |
| `access_log_rcd_upstream_reset` | same | ″ |
| `access_log_rf_upstream_reset` | same | ″ |
| `admin_config_dump_server_info` (fixture `0014`) | `/clusters` diverges: envoy-only `backend::192.168.65.2:…` — backend routed via the non-allow-listed bridge IP | `differential-host-bridge-ip-192-168-65-2` |

Isolation (each fails deterministically):

```
access_log_h2_rcd_upstream_reset  => FAILED. 0 passed; 1 failed
access_log_h2_uc_upstream_reset   => FAILED. 0 passed; 1 failed
access_log_rcd_upstream_reset     => FAILED. 0 passed; 1 failed
access_log_rf_upstream_reset      => FAILED. 0 passed; 1 failed
admin_config_dump_server_info     => FAILED. 0 passed; 1 failed
```

**Parallel-load flakes — PASS in isolation (tail), come and go between runs:**

```
client::tests::send_request_maps_h2_handshake_failure_to_typed_error (envoy-http2)  [run 1]
  → isolation: ok. 1 passed; 0 failed         (memory envoyrust-h2-handshake-test-host-flake)
admin_ready_returns_200_post_migration (admin_ready)                                [run 2 only]
  → isolation: ok. 1 passed; 0 failed         (port-reuse startup race)
upstream_outlier_detection_consecutive_5xx_fixture (upstream_outlier_detection)     [run 2 only]
  → isolation: ok. 1 passed; 0 failed         ("…not accept-ready within 10s: Connection refused"
                                                 port-reuse startup race)
```

**None of the 5 core REDs, nor any of the 3 flakes, touches the phase surface** (network `rbac`,
`direct_response`, `tcp_proxy`, or the new `ConfigError::UnsupportedNetworkFilterChainComposition`).
The 5-member core is exactly the invariant set the handoff predicted (`0061`/`0062`/`0069`/`0070`
upstream-reset witnesses + `0014`). **CI is authoritative; these are not "fixed."**

## (b) Pre-existing differential fixtures still green (§7.5 b)

The workspace test build recompiled `target/debug/envoy-bin` fresh at `a21c983` (memory
`differential-harness-uses-debug-envoy-bin`), so the differential fixtures ran against the current
binary — no stale-`unknown field` RED. Docker is UP; the pinned image
`envoyproxy/envoy:v1.33.0` (digest `56da5afd7df3…`) is cached (D-3.7). Every differential fixture is
green except the 5-member environmental core above, and `67.1` changed **no existing config** (the new
chain-composition rejection fires only on `[<non-terminal>, tcp_proxy]`, which no existing fixture uses;
`0003`'s lone `tcp_proxy` is guarded and GREEN).

## (c) Conformance (§7.5 c)

`h2spec` is the only §7.3 suite. Its runner (`tests/conformance/h2spec/tests/h2spec_runner.rs`) is a
workspace member and ran inside the test workspace run:

```
test h2spec_pass_rate_gate ... ok            (finished in 0.00s → eprintln-SKIP path)
test tests::parse_h2spec_output_extracts_section_failure_ids ... ok
test tests::parse_summary_line_extracts_pass_fail_counts ... ok
test result: ok. 3 passed; 0 failed
```

`which h2spec` fails on this host, so the gate **eprintln-skips locally per phase-05.2 SPEC §3 D7**
(CI provisions the binary and runs the real ≥95% gate). `known-failures.txt` was **NOT trimmed** — this
host scores invalid-preface `3.5/2` as PASS while CI fails it, so a locally-"fixed" list breaks CI
(memory `h2spec-3-5-2-preface-host-sensitive`).

## (d) Fuzz (§7.5 d) — NO new target, recorded explicitly

Per SPEC §5 and ADR-0128 §2.3: network `rbac` **parses nothing** (it inspects `peer_addr`/`local_addr`
only, never a downstream byte — R-2), so it ships **no new `cargo fuzz` target**. Its sole
untrusted-input surface is the bootstrap config parser, already covered by the pre-existing
`parse_bootstrap` target (`crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`, wired at
`.github/workflows/ci.yml`), which reaches the new `TypedConfig::NetworkRbac` variant **and** the new
`ConfigError::UnsupportedNetworkFilterChainComposition` path the moment they land. **Gate (d) is
SATISFIED by the pre-existing target**, recorded here rather than passed over in silence.

## (f) `REVIEW.md` approved (§7.5 f) — UNMET, by design

The current `REVIEW.md` verdict is **NOT APPROVED** (its C-1 and I-2/I-4/I-5/M-1…M-8 were the state-3
re-entry's charter, now all landed). A review is superseded only by a **LATER** review (D-3.5);
`REVIEW.md` is **not edited** by this session. **(f) is therefore the one unmet gate**, and satisfying
it is the job of the SEPARATE state-5 code-review session — **NOT this one** (§5.1; `ADR-0127` names
3→4 and 4→5 explicitly as un-chainable).

## State-4 disposition

**(a)-(e) are GREEN on this host** (modulo the fully-adjudicated environmental REDs, which are
CI-authoritative). **(d) is satisfied** by the pre-existing fuzz target. **(f) is UNMET** and is the
next session's job. `67.1` is therefore **VERIFIED, NOT REVIEWED → §5 state 5.** No code, no fixture,
no `known-failures.txt`, no ROADMAP row, and no `REVIEW.md` was changed this session. §6.1's
mid-execution valve stayed ARMED and did not fire (state-4 writes no code). The next session runs
`superpowers:requesting-code-review` (a NEW review that supersedes `REVIEW.md`) — and, per the STATE.md
guidance, it should **probe the now-closed composition matrix**, not just re-read the code.
