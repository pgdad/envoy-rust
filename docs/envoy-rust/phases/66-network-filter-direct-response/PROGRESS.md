# Phase 66 — `envoy.filters.network.direct_response` — PROGRESS

> **Status:** §5 **state-3 implementation COMPLETE**. All 9 `PLAN.md` tasks landed, each TDD
> (red → green) per doctrine D-3.1 / `superpowers:test-driven-development`.
> **Next:** §5 state-4 verification (`superpowers:verification-before-completion`, the full
> §7.5 (a)-(f) gate) — a SEPARATE session, per §5.1 (one state per session).
> **The §7.5 gate was deliberately NOT run this session.**

This document is written for a stranger with zero prior context (doctrine D-3.4).

---

## Session preconditions (verified at cold-start)

- `git status --porcelain` clean; branch `main`; `HEAD` = `origin/main` = `5e3afb9` (the phase-66
  state-2 PLAN-write).
- `git fetch origin --prune` + `git ls-tree -r origin/main` confirmed **no sibling loop session had
  started the implementation**: no `PROGRESS.md`, no `crates/envoy-bin/src/direct_response.rs`.
  (The `*direct_response*` paths that DO exist on `origin/main` — `crates/envoy-bin/tests/http1_direct_response.rs`,
  `http2_direct_response.rs`, `tests/differential/tests/http{1,2}_direct_response.rs`,
  `fuzz/corpus/parse_bootstrap/hcm_direct_response_happy.yaml` — are the pre-existing HCM
  **route-level** `direct_response` action from phase 04. A DIFFERENT feature with the same name.)
- §5 state-3 detection rule confirmed: `SPEC.md` + `PLAN.md` present, `PROGRESS.md` / `REVIEW.md` absent.
- The STEP-0.5 concurrency guard (`git fetch origin --prune`, re-check `origin/main`) was re-run
  before **every** commit. `origin/main` never moved during this session.

## PLAN critical review (executing-plans Step 1) — every assumption re-verified against the live tree

| PLAN claim | Verified |
|---|---|
| `ConfigError` at `lib.rs:60`, **no** `src/error.rs` | CONFIRMED (`lib.rs:64` after drift) |
| `MissingTypedConfig(&'static str)` tuple variant exists | CONFIRMED (`lib.rs:77`) |
| `parse_bootstrap` parses **AND** validates; no `load_bootstrap_from_str` | CONFIRMED (`lib.rs:769-782`) |
| `DataSourceInline` is `deny_unknown_fields`, `inline_string` required | CONFIRMED — so `inline_bytes`/`filename` fail with `unknown field` at **deserialize** time |
| validate's filter loop is `for filter in &mut chain.filters` (HCM arm calls `as_mut()`) | CONFIRMED — terminal check MUST be an immutable pre-pass |
| `main.rs` mods `argv`/`echo`/`tls_handler` | CONFIRMED |
| `tests/common/mod.rs` exports `reserve_port`, carries module-wide `#![allow(dead_code)]` | CONFIRMED |
| `tempfile` is an `envoy-bin` dev-dependency | CONFIRMED |
| `Driver` enum, `run_tcp_echo_arm`, 5-arg `assert_equivalence`, `port_key` match | CONFIRMED |
| `fuzz/.gitignore` line 1 = `corpus/parse_bootstrap/*` + explicit `!` lines | CONFIRMED |

**No blockers, no plan gaps.** One naming hazard found and honored (below).

---

## Task-by-task log

### Task 1 — `envoy-config` schema — COMPLETE (commit `60f7f7d`)

TDD. **RED:** `cargo test -p envoy-config direct_response` → `cannot find value DIRECT_RESPONSE_FILTER`,
`no variant named DirectResponse`. **GREEN:** 3/3 new tests pass.

Added `DIRECT_RESPONSE_FILTER` const (`lib.rs`), `DirectResponseConfig { response: Option<DataSourceInline> }`
and `TypedConfig::DirectResponse` (`bootstrap.rs`).

**Naming hazard (load-bearing for future readers).** `envoy_config::DirectResponse` **already existed**
in the `pub use bootstrap::{…}` re-export list — it is the HCM **route-level** `direct_response`
action (phase 04). The new type is deliberately named `DirectResponseConfig` and there is **no
collision**. `TypedConfig::DirectResponse` (the enum *variant*) lives in a different namespace from
the `DirectResponse` *struct*. This is precisely the conflation `PLAN.md` Task 9 Step 2 warns about.

Per the PLAN's VERIFIED-ENTRY-POINTS note, this task's tests use `serde_yaml::from_str::<Bootstrap>`
(pure deserialization) — they pass *before* Task 2's validate arm exists, because `parse_bootstrap`
would still reject the filter name as `UnsupportedFilter`.

### Task 2 — `validate()` arm — COMPLETE (commit `c7e5650`)

TDD. **RED:** all 3 new tests fail with the predicted
`UnsupportedFilter("envoy.filters.network.direct_response", "envoy.filters.network.echo")`.
**GREEN:** 13 `direct_response`-matching tests pass (10 pre-existing route-level + 3 new).

### Task 3 — network-filter terminal validation — COMPLETE (commit `b937a18`)

TDD. **RED:** `no variant named NetworkFilterNotTerminal`, `cannot find function is_terminal_network_filter`.
**GREEN:** `cargo test -p envoy-config` → **548 passed, 0 failed**.

Added `ConfigError::NetworkFilterNotTerminal { name, position, chain_len }` (1-based `position`),
`is_terminal_network_filter(&str) -> bool` (a per-name predicate, NOT `chain.filters.len() <= 1` —
ADR-0123), and an **immutable pre-pass placed BEFORE** the existing mutating `for filter in &mut
chain.filters` loop. The ordering is doubly load-bearing: the mutating loop borrows `chain.filters`
mutably (the HCM arm calls `as_mut()`), and the pre-pass reproduces upstream Envoy's error
precedence — a `[direct_response, echo]` chain reports the TERMINAL error even when the trailing
filter is itself malformed.

**R-0.8 (safety) re-confirmed EMPIRICALLY, not just by the SPEC's fixture scan.** SPEC §0 R-0.8 only
scanned `tests/fixtures/**/*.yaml`. This session additionally ran the suites of every crate that
builds a multi-network-filter YAML inline:

```
cargo test -p envoy-admin -p envoy-cluster --lib      → 97 passed, 160 passed
cargo test -p envoy-bin --test tls_sni
             --test xds_file_based_lds
             --test xds_file_based_rds                → 1 passed, 6 passed, 8 passed
```

Zero regressions. The `0006-tls-sni` / `tls_sni.rs` shapes are two SEPARATE single-filter SNI
chains, exactly as R-0.8 claimed — not one two-filter chain.

### Task 4 — data plane (write → flush → FIN → **drain**) — COMPLETE (commit `d9c2ecd`)

TDD. **RED:** 5/5 tests fail on `todo!()` (`not yet implemented`). **GREEN:** 5/5 pass.

Created `crates/envoy-bin/src/direct_response.rs`, structurally parallel to `echo.rs`
(a standalone `serve()` accept loop + `JoinSet`, NOT a `ConnectionHandler` impl). Per connection:
`write_all(payload)` → `flush()` → `shutdown()` (FIN) → **read-and-discard until EOF** → drop.
Bounded by the existing `DRAIN_TIMEOUT` (5 s) — **no new timeout knob** (ADR-0124).

**PLAN Task 4 Step 5 — the MANDATORY mutation check — WAS PERFORMED AND IT BITES.**
With the drain loop commented out:

```
$ cargo test -p envoy-bin --bin envoy-bin post_eof_client_write_is_accepted_not_reset
test direct_response::tests::post_eof_client_write_is_accepted_not_reset ... FAILED
panicked at crates/envoy-bin/src/direct_response.rs:181:14:
second post-EOF write must not be reset: Os { code: 32, kind: BrokenPipe, message: "Broken pipe" }
test result: FAILED. 0 passed; 1 failed
```

The drain loop was then **restored** and the suite re-run to green (5 passed). ADR-0124's claim is
therefore *pinned by a test*, not merely asserted — which matters because fixture `0071` structurally
**cannot** catch a missing drain (its driver never writes after EOF). No escalation to
`superpowers:systematic-debugging` was needed.

**Transient lint, resolved by Task 5 (recorded for honesty).** At Task 4's commit boundary,
`cargo clippy -p envoy-bin --bin envoy-bin -- -D warnings` reported 3 `dead_code` errors
(`DRAIN_TIMEOUT`, `serve`, `direct_response_once` — nothing calls `serve` until Task 5 wires the
dispatch arm). This is structural to the PLAN's own task ordering (Task 4 Step 2 adds
`mod direct_response;`, Step 6 commits it; the caller arrives in Task 5). Tests were green at Task 4,
and clippy returned to **0 errors / 0 warnings** at Task 5. Doctrine D-3.6 is phase-level ("no *phase*
lands with lint errors") and the §7.5 gate runs at state-4 on the final tree, so this was accepted
rather than papered over with a temporary `#[allow]`.

### Task 5 — `main.rs` dispatch arm + in-process backstop — COMPLETE (commit `1398be6`)

TDD. **RED:** all 3 integration tests fail with the predicted `listener 127.0.0.1:NNNNN never became ready`
(`envoy-bin` bails at startup — no dispatch arm). **GREEN:** 3/3 pass.

Added the `DIRECT_RESPONSE_FILTER` arm to `match filter.name.as_str()`, building
`payload: Arc<[u8]>` from `dr_cfg.response.as_ref().map(|d| d.inline_string.as_bytes()).unwrap_or(&[])`
— so an omitted `response` yields the empty payload (SPEC §0 R-0.7). Created
`crates/envoy-bin/tests/network_filter_direct_response.rs` (payload + clean EOF; client-writes-first;
omitted-`response` zero-byte case).

Post-task: `cargo clippy -p envoy-bin --all-targets -- -D warnings` → **clean**; `cargo fmt -p envoy-bin -- --check` → clean.

### Task 6 — `Driver::TcpDirectResponse` — COMPLETE (commit `5e019e0`)

TDD. **RED:** `no variant named TcpDirectResponse`. **GREEN:** 1/1; clippy + fmt clean.

Added the `Driver::TcpDirectResponse` variant (YAML tag `kind: tcp_direct_response`),
`drive_tcp_direct_response(SocketAddr) -> Result<Vec<u8>>` (**the harness's first read-to-EOF raw-TCP
driver**, 5 s `tokio::time::timeout` around `read_to_end`), the `port_key` `{{PORT}}`-group arm,
`run_tcp_direct_response_arm`, and the `run_fixture` dispatch arm.

A *new* driver rather than a widened `TcpEcho` is required because `drive_tcp` writes a payload and
reads **exactly `payload.len()`** bytes via `read_exact` — deliberately not to EOF (ADR-0006 /
ADR-0007). `direct_response` ignores client input and writes a payload of its own length.

### Task 7 — fixture `0071` + differential test — COMPLETE (commit `abb0cf6`)

Rebuilt the **debug** binary first (`cargo build -p envoy-bin`) — the harness executes
`target/debug/envoy-bin`, and this phase adds both a new config key and a new filter name, so a stale
binary REDs with `unknown field` / `unsupported network filter`.

```
$ cargo test -p differential --test network_filter_direct_response
test network_filter_direct_response_fixture ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.84s
```

**The fixture is GREEN locally against the pinned upstream image** `envoyproxy/envoy:v1.33.0`
(present in the local Docker image store). This is the phase's deliverable: a deterministic,
timing-free, **byte-exact** cross-proxy witness. Both sides carry the **identical** `typed_config`
(no ADR-0014 shim needed); the only difference is the bind address (`0.0.0.0` vs `127.0.0.1`),
matching fixture `0001`'s convention. No `inputs/` dir — the driver sends nothing.
`expectations.yaml` needs no allow-list (SPEC V-6 confirmed).

The fixture `README.md` explicitly records what the fixture **cannot** catch (the ADR-0124 drain) and
points at the in-process mutation check that does.

### Task 8 — fuzz corpus seed — COMPLETE (commit `3018157`)

**No new fuzz target** (ADR-0123 §2.3): `direct_response` parses nothing — it never reads a byte from
the downstream socket — so its only untrusted-input surface is the bootstrap parser, already covered
by the pre-existing `parse_bootstrap` target. Because no new target is added, `.github/workflows/ci.yml`
needs **no new step**.

Added `crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_direct_response.yaml` plus its
explicit `!`-un-ignore line in `crates/envoy-config/fuzz/.gitignore` (line 1 is `corpus/parse_bootstrap/*`).
**Proven tracked** — the trap is that a `*`-ignored seed is invisible to CI and the gate is silently unmet:

```
$ git check-ignore -q …/network_filter_direct_response.yaml   → not ignored
$ git ls-files …/network_filter_direct_response.yaml
crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_direct_response.yaml
```

Short-budget fuzz run (mirrors `.github/workflows/ci.yml`):

```
$ cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30
#9209   DONE   cov: 14674 ft: 29749 corp: 2551/1652Kb exec/s: 101 rss: 342Mb
Done 9209 runs in 91 second(s)
```

No crash, no panic. **§7.5 gate (d) is satisfied by this pre-existing target** — the state-4 session
must RECORD that explicitly rather than silently skip the gate.

Only the seed + `.gitignore` were staged; the corpus entries libFuzzer generated during the run are
correctly `*`-ignored.

### Task 9 — `BEHAVIOR_CONTRACT.md` — COMPLETE (commit `4d5e8e9`)

Created a new `## Network filters` section (none existed) between `## LB selection` and
`## Header allow-list`, carrying the five items: (1) `direct_response` response semantics + the
fixture-`0071` witness; (2) the **ADR-0124 read-half drain** clause with the measured evidence
(`post_write=writes_ok` at 0 / 21 / 200 000 unread bytes) and an explicit note that it has no
differential observable and is pinned by the in-process mutation check; (3) the bilateral
network-filter terminal rule; (4) the recorded `DataSource`-arm divergence (**CF-66-1**); (5) the
pre-existing `echo` `typed_config` asymmetry scope note.

The section opens with a **do-not-conflate** banner separating this network filter from the
route-level `direct_response` action, because every other `direct_response` row in the document
refers to the latter. PLAN Step 2's contradiction scan confirmed no pre-existing row claims a
conflicting *network-filter* `direct_response` semantic.

---

## Post-implementation housekeeping (commit `414f72a`)

A pre-push `cargo fmt --all -- --check` caught drift: my manual line-wrap of the `DirectResponseConfig`
re-export in `crates/envoy-config/src/lib.rs` disagreed with rustfmt. Fixed with `cargo fmt --all`
(the ONLY file it touched) and committed separately. `cargo fmt --all -- --check` is now clean.

This is exactly the trap the project memory records as *"state-4 = CI's first real execution; CI is
often red-at-fmt mid-phase."* It was resolved at state-3 instead.

**Pre-push sanity (NOT the §7.5 gate):**

```
cargo build --workspace --all-targets   → exit 0
cargo fmt --all -- --check              → exit 0 (clean)
```

---

## §6.1 split gate — re-evaluated mid-execution, DOES NOT FIRE

`BOOTSTRAP_PROMPT.md` §6.1 also triggers a split *mid-execution* if any single task's sub-steps blow
past ~10 items on contact with reality. **None did.** Every task ran to its planned step count.
`ADR-0125` remains **unreserved and unfired**.

## Scope discipline

- **CONSUMES no carry-forward.** M64-2, M64-3, M57-1, M55-1, M53-2, M53-3, M48-2, M42-1, the
  `DC`/retry-budget-overflow slices of M45-2, the phase-58 candidate carry-forward, M40-1,
  M39-1/M39-2, M38-1/M38-2, CF-39-1, M37-*, M36-*, M34-*, M33-*, the empty-`metadata_match`
  doc-comment, M29-*/M30-*, the phase-31 cosmetics, and the HTTP-filters-family (1)-(4) all stay
  LIVE. **NONE blocks.**
- **CF-66-1** (`inline_bytes`/`filename` `DataSource` arms) and **CF-66-2** (no generic
  network-filter chain iteration protocol) stay OPEN, as opened by ADR-0123 §2.2.
- The `echo` `typed_config` asymmetry was **NOT** "fixed" (it is the pre-existing ADR-0014 shim).
- No fixture was weakened; `tests/conformance/h2spec/known-failures.txt` was **not** touched.
- `#![forbid(unsafe_code)]` holds (D-3.8). No new crate, no new dependency, no new fuzz target,
  no new timeout knob.
- **DECISIONS.md ledger head: ADR-0124.** Next-available **ADR-0125**, unreserved. No ADR was
  written this session (none was needed — the implementation matched the PLAN).

---

## CI adjudication of the state-3 push (`0201bf6`) — read this FIRST at state-4 [ADR-0125]

The state-3 push went red on CI. A full `superpowers:systematic-debugging` pass separated **two
distinct failures** that the tooling conflated. **Phase 66's own code is green on CI.**

### (1) Attempt 1 was runner starvation — the commit never executed

Run `29018695033` attempt 1 reported top-level `conclusion=failure` while both jobs reported
`conclusion=cancelled` after exactly 15m07s. Decisive fields:

| | attempt 1 (`0201bf6`) | control: run `29016561942` (`5e3afb9`) |
|---|---|---|
| `runner_name` | `""` — none assigned | `GitHub Actions 1000003159/60` |
| `steps` | `0` / `0` | `15` / `12` |
| logs | none (`gh run view --log` empty) | full |

No runner was ever assigned, so the workflow never checked out the repo, never compiled, never ran a
test. **Refuted:** the `fuzz` job's `timeout-minutes: 15` (a step timeout requires steps; there were
zero); `concurrency.cancel-in-progress` (an API query over the 12:30–13:00Z window returned exactly
ONE run); a sibling push (`origin/main` had not moved). `gh run watch --exit-status` surfaces this as
a plain "failure" — which is how an unattended loop is misled into debugging a diff that never ran.

### (2) Attempts 2–4 exposed a REAL, DETERMINISTIC failure — `upstream_h2_connection_pooling`

Once a runner attached, one test failed: `upstream_h2_connection_pooling`, at
`wait_ready(backend_addr, Duration::from_secs(30)).expect("backend ready")` with
`Os { code: 111, kind: ConnectionRefused }`, at **30.35s / 30.35s / 30.37s** across three reruns.
(The constant duration is just the timeout expiring — not, by itself, evidence of a deterministic
*cause*.)

**Phase 66 did not cause it:**
- The file is untouched by all 11 phase-66 commits.
- On attempt 2 — which actually executed — **every phase-66 surface passed**:
  `network_filter_direct_response_fixture ... ok` (fixture `0071` against the pinned
  `envoyproxy/envoy:v1.33.0`), `post_eof_client_write_is_accepted_not_reset ... ok` (the ADR-0124
  drain), all three in-process backstops, `parses_tcp_direct_response_driver ... ok`.
  `fmt` / `clippy` / `build` all green.
- The test passes locally standalone (3.83s) **and** under the full 36-binary `cargo test -p envoy-bin`
  suite (0 failures) on this exact tree.

**"Just rerun" is refuted, not assumed.** Three reruns → three identical failures.
`Swatinem/rust-cache` saves only on job success, so every rerun restores the same stale cache and
repeats the same work (`Post cargo cache` takes 0s on every attempt). The standing project memory's
"documented flake → `gh run rerun --failed` → green" prescription is **wrong for this signature**.

### (3) A second finding that changes how CI runs must be read

CI runs a bare `cargo test --workspace`, which **stops at the first failing test binary**. When this
test trips, everything after it never runs — the failing run contained **no `548 passed` line for the
`envoy-config` lib suite and no h2spec output at all**. A run reporting "only this one failure" has
therefore **NOT exercised the gate**. Never read it as "everything else passed."

### (4) Why the root cause could not be reached — and what was done about it

`spawn_backend` pipes the helper's stderr, and `.expect("backend ready")` then drops the child,
**discarding both its stderr and its exit status**. The decisive evidence is thrown away by the test
itself. Hypotheses the available evidence cannot separate: (a) the helper crashed or failed to bind
(a dead peer yields `ConnectionRefused` for the whole 30s, exactly as observed); (b) the nested
`cargo run --manifest-path` blocked — whose classic signature `Blocking waiting for file lock on
build directory` goes to the *child's* stderr and is therefore invisible in the CI log, so its
absence there proves nothing. A **"first-hit compile" explanation was actively REFUTED**: after a
`cargo build --workspace --all-targets`, an identical nested `cargo run` emits zero stderr and
recompiles nothing, and the CI log shows no `Compiling` during the test step.

Per **ADR-0125**, phase 66 therefore adds **bounded diagnostics** to that test — `try_wait()` for the
exit status (`None` = still running, separating "crashed" from "alive but not listening") plus up to
5 s of stderr via a `tokio::time::timeout`-bounded `read_to_end` — and panics with both. The 30s
budget and the fatality of the timeout are **unchanged**. The read must be bounded: a
live-but-not-listening child never closes stderr, so an unbounded `read_to_string` would hang the
test instead of failing it. Rejected: widening the budget (treats a symptom on the one theory the
evidence refutes), and weakening/`#[ignore]`-ing the test (forbidden).

Green locally after the change: `cargo test -p envoy-bin --test upstream_h2_connection_pooling`
1 passed (0.62s); `cargo clippy -p envoy-bin --all-targets -- -D warnings` clean;
`cargo fmt --all -- --check` clean.

### BLOCK-66-1 — §7.5 gate (e) is BLOCKED until this test passes

**The next CI run is expected to STILL FAIL — that is the point.** It will now print the helper's exit
status and stderr. **The state-4 session must read that output FIRST** and root-cause from it via
`superpowers:systematic-debugging` before touching anything else:

- stderr shows a bind failure → the fix is in `reserve_port()`'s TOCTOU window;
- stderr shows `Blocking waiting for file lock` → the fix is to stop nesting `cargo run` inside
  `cargo test`;
- child exited cleanly with empty stderr → hypothesis (a) is dead; move to the runner's networking.

Gate items **(a)/(c)/(d)** already have positive evidence from CI attempt 2 and the state-3 fuzz run.
Gate **(b)** is unproven because the suite aborted early. Gate **(e)** is blocked. No phase-66
production code is implicated — ADR-0123 and ADR-0124 stand unmodified.

---

## What the state-4 verification session must do

Run the full §7.5 (a)-(f) gate via `superpowers:verification-before-completion` and quote every
command's output into this file. **It was deliberately not run here** (§5.1: one state per session).

```bash
cargo build --workspace --all-targets                                > /tmp/gate-build.log 2>&1
cargo clippy --workspace --all-targets --all-features -- -D warnings > /tmp/gate-clippy.log 2>&1
cargo fmt --all -- --check                                           > /tmp/gate-fmt.log 2>&1
cargo test --workspace                                               > /tmp/gate-test.log 2>&1
cargo deny check                                                     > /tmp/gate-deny.log 2>&1
```

- **(a)** fixture `0071` green — already green locally this session; re-confirm.
- **(b)** all pre-existing fixtures still green.
- **(c)** `h2spec` pass-rate gate unchanged. **NEVER trim `known-failures.txt`** — local h2spec scores
  invalid-preface 3.5/2 as PASS while CI fails it, so a locally-"fixed" list breaks CI.
- **(d)** satisfied by the pre-existing `parse_bootstrap` fuzz target (ADR-0123 §2.3) —
  **record this explicitly, do not skip it silently.**
- **(e)** the five commands above clean.
- **(f)** `REVIEW.md` approved (that is state-5).

**Never pipe a gate run through `tail`** — it truncates the `failures:` block and destroys the failing
test names. Redirect to a file.

**Known LOCAL-RED expectations (environmental; CI is authoritative):** an invariant core of
`0061`/`0062`/`0069`/`0070` (close-backend) + `admin_config_dump_server_info`, plus a varying tail
under parallel load. Adjudicate by running the workspace suite 2-3× and diffing the failing SET. CI
carries documented startup-race flakes → `gh run rerun <id> --failed`. Escalate to
`superpowers:systematic-debugging` only if a rerun re-fails the SAME test deterministically.

**Confirming CI:** `gh run list --commit <short-sha>` silently returns `[]`. Use the full 40-char SHA.

---

## BLOCK-66-1 root-caused and FIXED [ADR-0126]

ADR-0125's bounded diagnostics fired on CI run `29023995517` and printed:

```
backend ready timed out after 30s at 127.0.0.1:44785
http2-echo-server exit status: None (None = still running)
http2-echo-server stderr:
                                    <-- empty
```

A **live** child, an **empty** stderr, a port that never opens. This matched **none** of ADR-0125's
three predicted readings (bind failure / file-lock message / clean exit) — the triage table was
wrong, so the root-cause pass started from the observation, not the table.

**Root cause (measured).** `spawn_backend` ran `cargo run --quiet --manifest-path <helper>`.

1. Under resolver v2, feature unification is computed over the **packages selected on the command
   line**. `--manifest-path <helper>` selects ONE package; CI's `cargo build --workspace
   --all-targets` selects all 22. Shared deps therefore resolve **different feature sets** —
   workspace `tokio` carries `process`/`test-util`/`fs` the helper's does not; `aws-lc-rs`/
   `aws-lc-sys` differ on `prebuilt-nasm`; so do `futures-util`, `rustix`, `serde_core`, `smallvec`,
   `syn`, `bitflags`.
2. Diffing the two `cargo build -Z unstable-options --unit-graph` outputs under a **recursive**
   fingerprint (each unit hashed over its own features *and* its dependencies' hashes — a flat
   per-unit feature comparison is unsound and first gave a false "nothing to rebuild"):
   **46 of the helper's 116 units have no counterpart in the workspace build**, and **7 are warmed
   by no sibling test**: `futures-util`, `tokio-util`, `h2`, `envoy-cluster`, `envoy-http1`,
   `envoy-http2`, and the bin.
3. `.spawn()` returns immediately, so that compile runs **inside `wait_ready`'s 30s budget**. The
   port opens only once it finishes.
4. `--quiet` suppresses `Compiling …` **and** `Blocking waiting for file lock on artifact directory`
   (verified by holding the lock and diffing quiet vs loud stderr). Those are the only two things a
   silent, live cargo can do — which fully explains `exit status: None` + empty stderr + no listener.
5. `Swatinem/rust-cache` saves **only on job success**, so once these units missed the cache the
   test timed out, the job failed, the cache was not saved, and the next run was cold again —
   turning an occasional flake into a **deterministic** failure that reruns provably cannot recover.

**Corrected.** The earlier hypothesis in this session — *"a nested `cargo run` after a workspace
build emits zero stderr and recompiles nothing"* — was **asserted, not measured, and is REFUTED**.
The absence of `Compiling`/`Blocking` in the CI log is explained by (4), and is not evidence that no
compile occurred. The pre-existing "port-reuse startup race" note is also wrong: no port is reused;
the port is never bound.

**Fix.** In `spawn_backend`: build the helper to completion **before** the readiness clock starts,
under its own bounded `PREBUILD_TIMEOUT = 240s` (loud on non-zero exit or elapse); then `cargo run`
the warm artifact. `--quiet` is passed **nowhere**, so a future stall describes itself. **The 30s
readiness budget and every assertion are unchanged** — the budget now covers only the helper's
startup, which is what it always claimed to mean.

**Verified.** `cargo fmt --all -- --check` clean; `cargo clippy -p envoy-bin --all-targets -D
warnings` zero; test passes warm (0.69s). With the chain cold, the pre-build emits **7** `Compiling`
lines and the subsequent `cargo run` emits **0** (`Finished in 0.04s`), port listening immediately —
the compile has left the readiness budget, which was the entire defect.

**Latent in four siblings** (warm chains only, none failing; NOT touched here — state-3 scope):
`upstream_connection_pooling`, `upstream_active_health_check`, `upstream_outlier_detection`, and
`tests/differential/src/backend.rs`. Recorded as carry-forward **M66-2**.

### CI confirmation of the ADR-0126 fix — BLOCK-66-1 CLEARED

Run `29025781624` (`b9f8f6ed87fc689d3cd5df4cfdf4bedfc9a5a9b9`): **green on attempt 1, no rerun.**
Both jobs got real runners (`fuzz` 12 steps, `build + test + lint` 15 steps).

The `test` step contains the quantitative confirmation. The new pre-build compiled exactly the
chain predicted above and reported:

```
Compiling envoy-config … envoy-filter … envoy-listener … envoy-cluster … envoy-http1 …
Compiling envoy-http2 … Compiling http2-echo-server v0.0.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 33.63s
test upstream_h2_connection_pooling ... ok
test result: ok. 1 passed; 0 failed; … finished in 34.38s
```

**33.63s of compiling, against a 30s readiness budget.** That compile used to run *inside* the
budget, overshooting it by ~3.6s — matching the observed timeouts to within a second
(30.35 / 30.35 / 30.37s). The root cause is therefore **measured, not merely argued**. And because
this run went green, `Swatinem/rust-cache` finally saved those units, breaking the self-perpetuating
cold-cache loop.

Side-evidence that the gate was finally exercised: `cargo test --workspace` no longer aborts early,
so `envoy-config`'s `548 passed` line and the h2spec conformance step appear for the first time in
four attempts, with **zero** `panicked at` / `test result: FAILED` lines across 29 413 log lines.

**Log-reading trap:** CI interleaves ANSI escapes between `Compiling` and the crate name, so
`grep "Compiling h2 "` silently returns nothing and reads as "no compile happened." Grep the crate
name alone.

**Status:** BLOCK-66-1 is CLEARED. §7.5 gate (e) is now assessable; (b) has positive CI evidence for
the first time. The state-4 verification session runs the full (a)-(f) gate.

---

# §5 state-4 VERIFICATION — the full §7.5 (a)-(f) gate

> Run via `superpowers:verification-before-completion`. Every command below was executed **this
> session** against `HEAD` = `origin/main` = `36db7e3d8bf7cc749d39fcb1c3acfa47ba1689fc`, working tree
> clean. Output is quoted verbatim. Per §5.1 the state-5 code-review was **NOT** run.
>
> **Headline: the gate PASSES.** Its authority is CI (memory `envoy-rust-state4-ci-first-execution`),
> and CI run **`29026523684`** at that exact SHA is **green on attempt 1**: **147/147 test binaries
> `ok`, 1828 passed, 0 failed, 0 `panicked at`**. Five differential fixtures are RED *on this
> development host only*, for two independently documented host-network defects; each is green on CI.
> The arithmetic closes exactly (below), which is what makes the local REDs adjudicable rather than
> merely asserted.

## Session preconditions (verified at cold-start)

- `git status --porcelain` → empty. Branch `main`. `HEAD` = `origin/main` = `36db7e3` (no sibling had
  advanced the state; `git fetch origin --prune` showed `0 0` ahead/behind).
- §5 state-4 detection rule confirmed: `SPEC.md` + `PLAN.md` + `PROGRESS.md` present, **`REVIEW.md`
  absent**. ROADMAP row `66` = `in-progress`.
- Docker 28.1.1 up; the local `envoyproxy/envoy:v1.33.0` digest is
  `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2` — **byte-identical to the
  `ENVOY_TARGET.md` pin** (D-3.7). 71 fixtures present (`0001`…`0071`).
- `cargo build -p envoy-bin` was run **first**, before any differential (memory
  `differential-harness-uses-debug-envoy-bin`): the harness executes `target/debug/envoy-bin`, and
  this phase added both a new config key and a new filter name.

---

## Gate (e) — the five commands

| Command | Exit | Evidence |
|---|---|---|
| `cargo build --workspace --all-targets` | **0** | `Finished` dev profile … `in 11.57s`; **0** `^warning`/`^error` lines |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **0** | `Finished` … `in 4.65s`; **0** warnings |
| `cargo fmt --all -- --check` | **0** | **0 bytes** of output |
| `cargo test --workspace` | **101** locally / **0 on CI** | see the adjudication below — the five REDs are environmental |
| `cargo deny check` | **0** | `advisories ok, bans ok, licenses ok, sources ok` |

`cargo deny check` also emits five `warning[license-not-encountered]` lines (`0BSD`, `BSD-2-Clause`,
`MPL-2.0`, `Unicode-DFS-2016`, `Zlib` — unmatched *allowances* in `deny.toml`, not violations). They
are pre-existing, are warnings not errors, and the command exits 0. No freshly-published advisory
fired this session (contrast memory `cargo-deny-reds-on-unrelated-advisory`).

### `cargo test --workspace` — the bare form aborts early, exactly as ADR-0126 warns

The literal gate command was run first, verbatim:

```
$ cargo test --workspace            # exit 101
...
test result: FAILED. 0 passed; 1 failed; ... finished in 2.72s
error: test failed, to rerun pass `-p differential --test access_log_h2_rcd_upstream_reset`
```

That run produced only **209 log lines and 4 `test result:` lines**. It stopped at the first failing
test *binary*, so `envoy-config`'s 548 tests, h2spec, and fixture `0071` **never executed**. This is
the load-bearing reading trap ADR-0125 §(3) / ADR-0126 record: *a run reporting one failure has not
exercised the gate.* The gate was therefore re-run with `--no-fail-fast` to enumerate the complete
failing SET.

### Adjudicating the local REDs — invariant core vs varying tail

Per memory `local-red-set-varies-run-to-run`, the suite was run **twice** with `--no-fail-fast` and
the failing SETs diffed; then every member of both sets was re-run **in isolation**.

```
RUN A  cargo test --workspace --no-fail-fast   exit=101   1823 passed, 5 failed, 147 binaries
RUN B  cargo test --workspace --no-fail-fast   exit=101   1818 passed, 10 failed, 147 binaries
```

| | Run A | Run B | In isolation |
|---|---|---|---|
| `access_log_rf_upstream_reset` (fixture `0061`) | FAIL | FAIL | **FAIL** (deterministic) |
| `access_log_rcd_upstream_reset` (`0062`) | FAIL | FAIL | **FAIL** (deterministic) |
| `access_log_h2_uc_upstream_reset` (`0069`) | FAIL | FAIL | **FAIL** (deterministic) |
| `access_log_h2_rcd_upstream_reset` (`0070`) | FAIL | FAIL | **FAIL** (deterministic) |
| `admin_config_dump_server_info` | FAIL | FAIL | **FAIL** (deterministic) |
| `http_filter_fault` | pass | FAIL | **pass** |
| `lb_ring_hash` | pass | FAIL | **pass** |
| `upstream_circuit_breaker` | pass | FAIL | **pass** |
| `upstream_connection_pooling_and_per_class_counters` | pass | FAIL | **pass** |
| `xds_file_based_cds` | pass | FAIL | **pass** |

The separation is clean and matches the documented model exactly:

- **Invariant core (5)** — fails in *both* runs *and* deterministically in isolation ⇒ environmental,
  load-independent. Precisely the set `STATE.md` and `PLAN.md` predict.
- **Varying tail (5)** — fails only under full-workspace parallel load, **passes in isolation** ⇒
  parallel-load flake (memory `differential-fixtures-flake-under-parallel-load`, which names
  `http_filter_fault` explicitly and warns the set is "not just the bridge-IP set"). Not regressions.

**None of the ten is a phase-66 surface**, and fixture `0071` is green in run A, run B, and in
isolation.

### Root cause of the invariant core — measured from the failure text, not assumed

Four of the five are the **close-backend** fixtures. Their panic text shows **upstream Envoy**, the
*reference* proxy, failing to reach the host-spawned backend over IPv6:

```
envoy="{"rc":503,"rcd":"upstream_reset_before_response_started{remote_connection_failure|
        immediate_connect_error:_Network_is_unreachable|
        remote_address:[fdc4:f303:9324::254]:40217}","rf":"UF"}"
envoy-rust="{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}"
```

Envoy reports a **connect failure** (`UF`, `Network_is_unreachable`) where envoy-rust correctly
reports a **reset** (`UC`). This is memory `tcpclosebackend-ipv6-unreachable-host-flake`, verbatim.
It is a property of *this host's container networking*, not of envoy-rust: phase 66 cannot have
changed how the reference Envoy container routes IPv6.

The fifth, `admin_config_dump_server_info`, diverges on `/clusters` with cluster stats present only
on the Envoy side, keyed on the host bridge address:

```
envoy-only:      ["backend::192.168.65.2:46133::canary::false",
                  "backend::192.168.65.2:46133::cx_active::0", ...]
envoy-rust-only: []
```

That is memory `differential-host-bridge-ip-192-168-65-2` (this host routes the backend via
`192.168.65.2`, outside the allow-listed `192.168.65.254`/`172.17.0.1`).

### The arithmetic closes — this is what makes the adjudication sound

```
local RUN A : 1823 passed +  5 failed = 1828   across 147 binaries
CI  29026523684: 1828 passed +  0 failed = 1828   across 147 binaries
```

The local tree and CI run the **same 1828 tests across the same 147 binaries**. The *only* delta is
the five environmental REDs, every one of which is `ok` on CI (quoted under gate (b)). Nothing is
hidden behind the early abort, and no test is missing on either side.

---

## Gate (a) — fixture `0071-network-filter-direct-response` is GREEN

Confirmed **four** times: run A, run B, standalone locally, and on CI.

```
$ cargo build -p envoy-bin && cargo test -p differential --test network_filter_direct_response
test network_filter_direct_response_fixture ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
```

CI (`29026523684`, ANSI-stripped): `test network_filter_direct_response_fixture ... ok`.

Every other phase-66 surface is green locally (run A) **and** on CI:

```
test direct_response::tests::writes_payload_then_clean_eof ... ok
test direct_response::tests::empty_payload_writes_zero_bytes_then_closes ... ok
test direct_response::tests::client_that_writes_first_still_receives_payload ... ok
test direct_response::tests::post_eof_client_write_is_accepted_not_reset ... ok   <-- the ADR-0124 drain
test direct_response::tests::shutdown_signal_stops_the_accept_loop ... ok
test direct_response_writes_payload_then_clean_eof ... ok
test direct_response_ignores_client_input ... ok
test direct_response_with_omitted_response_writes_zero_bytes ... ok
test bootstrap::tests::terminal_network_filter_must_be_last ... ok
test bootstrap::tests::echo_is_also_terminal ... ok
test bootstrap::tests::single_terminal_filter_chain_is_accepted ... ok
test bootstrap::tests::is_terminal_network_filter_covers_all_four_supported_names ... ok
test parses_tcp_direct_response_driver ... ok
```

The ADR-0124 drain was **not re-litigated** and its mutation check was left untouched, per the
standing instruction. Also green: `upstream_h2_connection_pooling ... ok` and
`upstream_h2_connection_pooling_fixture ... ok` — the ADR-0126 fix holds on this tree.

## Gate (b) — all pre-existing fixtures `0001`-`0070` still green (additivity invariant)

**CI is authoritative and says yes**: 147/147 binaries `ok`, `0` occurrences of `panicked at`,
`test result: FAILED`, or `error: test failed` across 28 758 log lines. The five targets that are RED
on this host are explicitly `ok` there:

```
test access_log_rf_upstream_reset ... ok
test access_log_rcd_upstream_reset ... ok
test access_log_h2_uc_upstream_reset ... ok
test access_log_h2_rcd_upstream_reset ... ok
test admin_config_dump_server_info ... ok
```

`test result: ok. 548 passed; 0 failed` for the `envoy-config` lib suite confirms the CI run reached
past the point where the old BLOCK-66-1 failure used to abort it.

Terminal validation touches no existing config (SPEC §0 R-0.8), re-confirmed empirically at state-3
and now re-confirmed by the untouched pre-existing fixture set.

## Gate (c) — h2spec pass-rate gate unchanged

**Recorded honestly: h2spec did NOT run locally.** `which h2spec` → not found, and
`tests/conformance/h2spec/tests/h2spec_runner.rs:20-31` `eprintln!`-skips (returning `ok`) when the
binary is absent — which is why the local line reads `test h2spec_pass_rate_gate ... ok` in **0.00s**.
A local `ok` here is a **skip, not a pass**, and must not be quoted as evidence.

It was deliberately **not** installed locally: this host scores invalid-preface 3.5/2 as PASS while CI
fails it, so a local run would mis-adjudicate the known-failures list (memory
`h2spec-3-5-2-preface-host-sensitive`).

**CI provisions the binary** (`.github/workflows/ci.yml:43-49`) and reports:

```
test h2spec_pass_rate_gate ... ok
```

Supporting evidence that the gate is *unchanged* rather than merely passing:

- `git diff --name-only 5e3afb9..HEAD -- tests/conformance/` → **empty**. `known-failures.txt` is
  **untouched** (21 lines) and `git status` on `tests/conformance/` is clean. **Nothing was trimmed.**
- Phase 66 changed **no H2 codec or framing code**. The full phase-66 file list contains no
  `crates/envoy-http2/src/**` entry; the only H2-adjacent file is
  `crates/envoy-bin/tests/upstream_h2_connection_pooling.rs`, a *test harness* changed by ADR-0125 /
  ADR-0126, which cannot move an h2spec score.

## Gate (d) — fuzz: NO new target; satisfied by the pre-existing `parse_bootstrap` (RECORDED, not skipped)

Per ADR-0123 §2.3, `direct_response` **parses nothing** — it never reads a byte from the downstream
socket — so its only untrusted-input surface is the bootstrap parser. Gate (d) is satisfied by the
pre-existing `parse_bootstrap` target. **This is recorded explicitly here, as PLAN.md, SPEC.md and
STATE.md all instruct, rather than passed over in silence.**

Mechanically verified this session:

```
$ git diff --name-only 5e3afb9..HEAD -- '*fuzz_targets*' '.github/workflows/ci.yml'
                          <-- empty: no new fuzz target, and ci.yml needs no new step
```

The four targets in the tree (`parse_bootstrap`, `jwt_parse`, `cdn_loop_parse`,
`accesslog_format_parse`) are all pre-existing and all already wired into the CI `fuzz` job
(`ci.yml:77-124`). Memory `new-fuzz-target-needs-a-ci-yml-step` therefore does not bite.

The corpus **seed** phase 66 added is proven tracked (memory `fuzz-corpus-seed-gitignored-by-default`
— a `*`-ignored seed is invisible to CI and the gate is silently unmet):

```
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_direct_response.yaml
crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_direct_response.yaml
$ git check-ignore -v <same path>          → not ignored
```

Fresh short-budget run this session (mirrors `ci.yml:106`):

```
$ cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30      # exit 0
Done 29063 runs in 92 second(s)
```

No crash, no panic. CI's `fuzz` job ran all four targets clean on this SHA
(`Done 196492 / 2338291 / 4148411 / 4539953 runs`, each in 31 s).

## Gate (f) — `REVIEW.md` approved

**NOT this session.** Gate (f) is §5 state-5 (`superpowers:requesting-code-review`), the next session,
per §5.1 (one state per session). `REVIEW.md` does not exist.

---

## Gate verdict

| Gate | Verdict | Authority |
|---|---|---|
| (a) fixture `0071` green | **PASS** | local ×3 + CI |
| (b) `0001`-`0070` still green | **PASS** | CI (147/147 `ok`); local REDs adjudicated environmental |
| (c) h2spec pass-rate unchanged | **PASS** | CI (skipped locally — recorded, not claimed) |
| (d) fuzz short-budget clean | **PASS** | no new target (ADR-0123 §2.3) + `parse_bootstrap` clean, local + CI |
| (e) five commands clean | **PASS** | 4/5 clean locally; `cargo test --workspace` clean on CI |
| (f) `REVIEW.md` approved | **state-5** | next session |

**No fixture was weakened. `tests/conformance/h2spec/known-failures.txt` was not touched. The
ADR-0124 drain and its mutation check were not re-litigated. The `echo` `typed_config` asymmetry was
not "fixed". No production code changed this session — the state-4 verification is read-only over the
tree.** `#![forbid(unsafe_code)]` holds (D-3.8). No escalation to `superpowers:systematic-debugging`
was required: every local RED resolved to an already-documented environmental cause under the
prescribed 2×-run + isolation adjudication, and none re-failed deterministically on CI.

**Carry-forwards unchanged.** Phase 66 CONSUMES none. **CF-66-1**, **CF-66-2** and **M66-2** (the
ADR-0126 compile-inside-a-readiness-budget hazard, latent in `upstream_connection_pooling`,
`upstream_active_health_check`, `upstream_outlier_detection`, `tests/differential/src/backend.rs`)
stay OPEN. **DECISIONS.md ledger head remains ADR-0126**; next-available **ADR-0127**, unreserved. **No
ADR was written this session** — none was needed; the gate ran to a verdict without an ambiguity.

**Next session: §5 state-5 code-review (`superpowers:requesting-code-review`) → `REVIEW.md`.**
