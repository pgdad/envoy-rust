# Phase 02.2 REVIEW — Listener + TCP proxy filter + fixture 0003 + remaining rollovers

- **Base:** `d447f53` (phase 02.1 done commit — `phase 02.1: Config schema + cluster manager + echo-server helper [ADR-0014]`)
- **Head:** `02a9add` (phase 02.2 state-4 phase-done gate verification, task 13)
- **Files:** 24 changed (+4363 / -65). New crates: `crates/envoy-listener` (~424 LoC) and `crates/envoy-tcp` (~297 LoC). Two new ADRs (ADR-0015, ADR-0016) appended to `DECISIONS.md`. New differential module `tests/differential/src/backend.rs`, new acceptance test `tests/differential/tests/tcp_proxy.rs`, new in-process integration test `crates/envoy-bin/tests/tcp_proxy.rs`. New fixture `tests/fixtures/0003-tcp-proxy/` (5 files). PLAN.md (2705 lines), PROGRESS.md (128 lines).
- **Reviewed:** 2026-04-25
- **Verdict:** **Approved with fixes** — state 5 complete pending the §7 close-out (STATE.md text advance, mechanical). I1 closed in-phase via §7; M1–M4 tracked forward to phase 03 per §4.

---

## 1. Summary

Phase 02.2 delivers the first real data-plane path of the project. Two new library crates land: `envoy-listener` exposes a `Listener` + object-safe `ConnectionHandler` trait + `BoxFuture` alias built on `tokio::net::TcpListener` with a shutdown-gated `JoinSet` accept loop and 5-second `DRAIN_BUDGET`; `envoy-tcp` implements `TcpProxy` as a `ConnectionHandler` with bidirectional `tokio::io::copy` over the upstream/downstream split halves. `envoy-bin::main::run` now constructs a `ClusterManager` once and dispatches the listener's single filter on `envoy.filters.network.echo` (existing path) vs. `envoy.filters.network.tcp_proxy` (new path), producing a working TCP proxy from a static bootstrap. The differential harness gains `TcpProxyBackend` (host-subprocess `tcp-echo-server` with SIGKILL-on-Drop), per-side `{{BACKEND_HOST}}`/`{{BACKEND_PORT}}` substitution in `render_yaml`, `run_fixture` dispatch on the presence of `{{BACKEND_PORT}}`, and `upstream::start(yaml, host_gateway)` to apply `with_host("host.docker.internal", Host::HostGateway)` per ADR-0015. Fixture 0003-tcp-proxy ships green end-to-end. Phase-01 REVIEW §9 starter items I4 (admin 8 KiB header cap tightening) and M1 (stale `TODO(phase-01)` retarget) close alongside.

The work reads cleanly against doctrine on every axis I checked. **D-3.2 permitted foundations** are respected: `envoy-listener` adds only `envoy-config` (path), `tokio` (features `rt`, `net`, `macros`, `time`, `sync` — all permitted by tokio's wholesale entry on the D-3.2 list), `thiserror`, `tracing`; `envoy-tcp` adds `envoy-cluster`/`envoy-config`/`envoy-listener` (path), `tokio` (features `rt`, `net`, `io-util`, `macros`), `thiserror`, `tracing`. `envoy-bin/Cargo.toml` adds only the three workspace path-deps (`envoy-cluster`, `envoy-listener`, `envoy-tcp`); no new transitive crate surface. Cargo.lock diff confirms this — only the two new package stanzas plus envoy-bin's three path-deps under `dependencies`. **D-3.5 append-only ADRs**: `git diff d447f53..02a9add -- docs/envoy-rust/DECISIONS.md` shows zero `^-## ADR-` lines — ADR-0001 through ADR-0014 are byte-identical; ADR-0015 and ADR-0016 are appended in order. **D-3.8 unsafe-code**: `#![forbid(unsafe_code)]` at `crates/envoy-listener/src/lib.rs:1` and `crates/envoy-tcp/src/lib.rs:1`. **D-3.9 toolchain**: `rust-toolchain.toml` not in the diff range.

SPEC §3 deliverables D1–D7 all land in the described shape, with two well-disclosed plan-time deviations. **D1** (`envoy-listener`) ships the four-variant `ListenerError` matching SPEC §D1 plus a fourth `AddressParse(String, u16)` variant added at Task 5 to capture malformed `address:` strings (`envoy-config` keeps the field as `String` until bind time); the deviation is logged in PROGRESS.md and modeled on phase-02.1's `envoy-cluster::ClusterError::EndpointParse`. The six `Listener` tests land verbatim per SPEC's enumeration (`bind_returns_socket_address`, `bind_fails_cleanly_on_address_in_use`, `serves_accepts_and_dispatches_to_handler`, `serves_honors_shutdown_signal`, `serves_drains_in_flight_connection_within_budget`, `serves_aborts_stragglers_past_drain_budget`). **D2** (`envoy-tcp`) ships the three-variant `TcpProxyError` (`NoHealthyEndpoint`, `UpstreamConnect`, `CopyFailed`) and four tests (`proxies_payload_end_to_end`, `proxies_closes_downstream_on_upstream_close`, `proxies_closes_upstream_on_downstream_close`, `proxies_returns_err_on_upstream_connect_refused`). The bidirectional copy uses `tokio::select!` over two `tokio::io::copy` futures rather than SPEC §D2 step 4's `tokio::try_join!`; the deviation is explicitly justified by ADR-0016's `enable_half_close: false` posture (Task 8 PROGRESS note + inline comment at `crates/envoy-tcp/src/lib.rs:69-73`). The semantics are different in a load-bearing way — `select!` lets EOF on either side immediately drop the other copy future, which propagates FIN — and is the correct shape for the chosen ADR. **D3** (envoy-bin wiring) lands the `ClusterManager` build, the filter-name dispatch, and a non-Docker integration test at `crates/envoy-bin/tests/tcp_proxy.rs` that mirrors phase-01's `admin_only.rs`. **D4** (harness extensions) lands `TcpProxyBackend` with the precise binary lookup convention from SPEC §6 signpost 8 (`<workspace>/target/<profile>/tcp-echo-server`, with `CARGO_TARGET_DIR` honored), the per-side substitution maps in `run_fixture`, and `upstream::start`'s new `host_gateway: bool` flag. **D5** (fixture 0003) lands all five files with the right contents — `envoy.yaml` uses `{{BACKEND_HOST}}` (templates to `host.docker.internal`), `envoy-rust.yaml` uses literal `127.0.0.1`, `enable_half_close` is absent from both per ADR-0016, and `inputs/payload.bin` is a byte-identical copy of fixture 0001's 18-byte payload (`diff` returns clean). **D6** (rollovers I4 and M1) close in commits `4bd0e22` and `8aab844`. **D7** (CI workflow) requires no changes — the existing `build` job picks up the new crates automatically.

Gate evidence is solid. PROGRESS.md §"Task 13 / State 4" reports build/clippy/fmt/test/deny all clean on first attempt. Local spot-checks of `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` on HEAD `02a9add` both exit 0 — no fmt drift, no clippy warnings. Total test count of 114 (31 differential + 19 envoy-bin + 8 envoy-cluster + 38 envoy-config + 6 envoy-listener + 4 envoy-tcp + 8 tcp-echo-server) lines up with what the new crates and harness extensions add to the phase-02.1 baseline. The Cargo.lock sync at commit `2146014` is exactly what it should be: two new `[[package]]` stanzas (`envoy-listener` + `envoy-tcp`) plus three new entries inside `envoy-bin`'s dependency list. No version bumps, no transitive surface additions — verified by `git diff 02a9add~2..2146014 -- Cargo.lock`. The HEAD-vs-Cargo.lock-sync diff (`git diff 2146014..02a9add`) touches only `PROGRESS.md` (the Task 13 narrative).

The executor self-audited deviations cleanly. PROGRESS Task 5 names the `ListenerError::AddressParse` 4th variant; Task 8 names the `select!` vs. `try_join!` deviation and pins it to ADR-0016. Task 9 notes that the integration test passed against the un-wired `envoy-bin` because `echo::serve` echoes locally without needing the upstream backend round-trip — a correct self-flag, even if the test still serves as a regression gate after wiring. Task 11 closes phase-02.1 REVIEW M3 (the dead `|| msg.contains("CRLF")` disjunct) opportunistically. None of these deviations reads as undisclosed drift.

One mechanical issue surfaces. **STATE.md is stale by two lifecycle steps** — at HEAD `02a9add` it still reads "phase 02.2 lifecycle state 3 (PLAN.md exists, implementation incomplete)", which was the state 2→3 snapshot from commit `7504b86`. All 13 PLAN.md tasks are implementation-complete; the state-4 gate cleared; this REVIEW is the state-5 input. This is the *exact same* I2 finding from phase-02.1's REVIEW; the close-out shape is identical (advance STATE.md text in the same commit that lands REVIEW.md). Tracked as I1 below and remediated in §7.

---

## 2. Strengths

- **Doctrine conformance end-to-end.** `#![forbid(unsafe_code)]` at both new crate roots (`crates/envoy-listener/src/lib.rs:1`, `crates/envoy-tcp/src/lib.rs:1`); every new dep on the D-3.2 list (tokio + thiserror + tracing + serde_yaml at the root layer; workspace path-deps for the rest); append-only ADR ledger preserved (verified via `git diff d447f53..02a9add -- docs/envoy-rust/DECISIONS.md` — only ADR-0015 and ADR-0016 appended; ADR-0001–0014 byte-identical); root-toolchain pin untouched; parent SPEC `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` unedited (last touch SHA `50349da`, same as phase 02.1 baseline).

- **`envoy-listener::Listener::serve` is the textbook tokio shape.** The accept loop at `/Users/esa/git/envoy-rust/crates/envoy-listener/src/lib.rs:118-148` is a clean `tokio::select!` over the pinned shutdown future, `listener.accept()`, and `join_set.join_next()` (with the `if !join_set.is_empty()` guard correctly preventing the `Some(None)` arm from firing on an empty set). Accept errors are non-fatal (`tracing::warn!`-and-continue), consistent with phase-01's `echo::serve` and `admin::serve`. The drain at lines 151-170 honors the 5s `DRAIN_BUDGET` via `tokio::time::timeout`, and on expiry calls `abort_all` followed by a drain loop to let aborted tasks unwind before returning `ListenerError::DrainTimeout(DRAIN_BUDGET)`. Dropping the listener at line 122 on shutdown is the correct ordering — it stops new accepts before drain begins.

- **All four `ListenerError` branches are exercised.** `Bind` covered by `bind_fails_cleanly_on_address_in_use` (lines 397-423; the `AddrInUse` cross-platform note at lines 416-419 is proactive and correct); `DrainTimeout` covered by `serves_aborts_stragglers_past_drain_budget` (lines 314-355) with explicit time-window assertions (`>= 4s` and `< 7s`) bracketing the 5s budget. `AddressParse` is implicit (the bind tests use only valid `127.0.0.1` strings); `Accept` is implicit (covered by `tracing::warn!` continuation rather than a return). The omissions are reasonable — neither variant has an easy in-process trigger.

- **`envoy-tcp::TcpProxy::handle` correctly maps the ADR-0016 posture to the implementation.** Lines 69-79 use `tokio::select!` over the two `tokio::io::copy` futures rather than `tokio::try_join!`. The inline comment names the deviation, references ADR-0016, and explains the FIN-propagation property. The explicit `drop((dr, dw, ur, uw))` at line 80 forces the write halves closed before mapping the result to `TcpProxyError::CopyFailed`, which is the load-bearing step for FIN propagation. The three test branches each exercise a `TcpProxyError` variant: `proxies_payload_end_to_end` for the success path; `proxies_closes_downstream_on_upstream_close` and `proxies_closes_upstream_on_downstream_close` for asymmetric-close behavior; `proxies_returns_err_on_upstream_connect_refused` for `UpstreamConnect` (using the kernel-refused `127.0.0.1:1`). `NoHealthyEndpoint` is implicit — exercised through `Cluster::pick_endpoint() → None` only via in-tree `envoy-cluster` empty-cluster guards, which can't trigger here because the `mk_handle` fixture always lands one endpoint; the variant's wire format is verified by `cargo build` (the `#[error(...)]` string compiles).

- **Round-robin endpoint use through `cluster.pick_endpoint()`.** `crates/envoy-tcp/src/lib.rs:53` calls `self.cluster.pick_endpoint()` — the fetch-add round-robin picker phase 02.1 landed. No re-implementation of LB logic; the consumer site lives where SPEC §D2 placed it. The `cluster_name: String` carried separately on line 19 is the right shape given that phase 02.1 chose not to ship `Cluster::name()` per its own REVIEW M1 — the comment on lines 14-16 explicitly cross-references the deferral. No speculative API surface added.

- **envoy-bin wiring is shape-correct and validates the validator.** `/Users/esa/git/envoy-rust/crates/envoy-bin/src/main.rs:74-140` constructs `ClusterManager` once via `Arc`, then dispatches the single first filter per `filter.name.as_str()`. The `let-else` guard on `TypedConfig::TcpProxy` at lines 110-117 surfaces any validator gap (`bail!` with a clear error message); the `cluster_mgr.get(&tp_cfg.cluster).expect("validator guarantees cluster present")` at lines 118-120 documents the invariant boundary. The `anyhow::anyhow!(e)` conversion at line 131 correctly bridges `ListenerError → anyhow::Error` at the binary-crate boundary, consistent with D-3.2 (anyhow only in binaries).

- **I4 close-out (admin 8 KiB cap tightening) is exact.** `crates/envoy-bin/src/admin.rs:165-166` bounds the read slice to `(MAX_REQUEST_HEAD - buf.len()).min(scratch.len())`, which precludes a single read overshooting the cap. `rejects_oversized_request_headers` (lines 304-356) sends exactly `MAX_REQUEST_HEAD + 1 = 8193` bytes and asserts `HTTP/1.1 431` in the response prefix; the precondition `assert_eq!(req.len(), MAX_REQUEST_HEAD + 1)` at line 327 documents the boundary. The new `accepts_requests_exactly_at_cap` test (lines 450-486) builds a complete CRLF-CRLF-terminated HTTP/1.1 request whose total wire length is exactly 8192 bytes and asserts `HTTP/1.1 404 Not Found` (because the path is unknown) — proving the cap-boundary doesn't trip 431. The `assert_eq!(req.len(), MAX_REQUEST_HEAD)` at line 472 documents the boundary on the accept side. Together these two tests pin both sides of the boundary, exactly as SPEC §6 signpost 12 specified.

- **`TcpProxyBackend` lookup convention is correct and documented.** `tests/differential/src/backend.rs:91-117` walks two parents up from `CARGO_MANIFEST_DIR` (i.e., from `tests/differential/`) to the workspace root, honors `CARGO_TARGET_DIR`, picks `debug` or `release` per `cfg!(debug_assertions)`, adds `.exe` on Windows, and emits a clear error if the binary isn't built (with the `cargo test --workspace` recovery hint). The `cfg!(debug_assertions)` discrimination is the canonical convention — release-mode tests would set `debug_assertions = false`, picking up the right `target/release/` path. The skip-if-not-built fall-through in both unit tests (lines 130-133 and 152-156) is the correct pattern for cross-package binary lookup.

- **`TcpProxyBackend::Drop` SIGKILL semantics match `subject.rs`.** The Drop impl at lines 66-85 sends `start_kill()` then polls `try_wait()` in a 50ms-sleep loop with a 2s deadline. This is the same posture as `tests/differential/src/subject.rs:47-54` (also SIGKILL on Drop) — consistent and reflects the open-ended `nix`-deferral the M1 close-out documents. Under a `panic = "abort"` test failure, Drop wouldn't run at all, but the `kill_on_drop(true)` at line 39 also sets up tokio's child-process tracking to send SIGKILL on Drop via the runtime's reaping path; either way the child dies. Worth flagging as a Minor (M1 below) for awareness but not a blocker.

- **Differential harness `run_fixture` dispatch is mechanically right.** `tests/differential/src/lib.rs:354-396` checks both rendered templates for `{{BACKEND_PORT}}`; spawns `TcpProxyBackend` only when needed; binds the per-side substitution maps (`host.docker.internal` for envoy-side, `127.0.0.1` for envoy-rust-side); and gates `host_uses_host_gateway` on whether the *rendered* upstream YAML contains `host.docker.internal` (line 396). The detection-by-rendered-output approach is robust against future templates that might compute the host substitution differently. The `_backend` binding at line 365 holds the backend alive for the full `run_fixture` lifetime; Drop fires after both proxies have been torn down (the `// _backend Drop fires here.` comment at line 444 is a useful reader signpost).

- **ADR-0015 and ADR-0016 are well-formed.** Both follow the established structure (Date, Status, Context, Options considered with explicit rejections of (ii) and (iii), Decision, Rationale, Consequences). ADR-0015 names the `testcontainers` API surface (`with_host(name, Host::HostGateway)`), references ADR-0004/0005 for the testcontainers exemption, and pre-articulates the `172.17.0.1` fallback under a follow-up ADR if `host-gateway` ever fails. ADR-0016 explicitly cites SPEC §6 signposts 5 and 6, and warns reviewers against "defensively" adding `enable_half_close: false` to future fixtures — useful future-proofing.

- **Phase-02.1 rollover M3 closed in-phase.** Phase 02.1 REVIEW Minor M3 (drop the dead `|| msg.contains("CRLF")` disjunct in `decode_chunked_truncated_size_line`) was an opportunistic closure — Task 11's diff at `tests/differential/src/lib.rs:818-827` removes the disjunct and updates the assertion to `msg.contains("missing CRLF")` only. Clean closure with a self-noted attribution in PROGRESS.md.

- **Self-audited deviations are tight.** Task 5 documents `ListenerError::AddressParse` as a 4th variant and explains the rationale (envoy-config keeps `address` as `String` until bind time); Task 8 documents the `select!` vs. `try_join!` swap and references ADR-0016. No undisclosed drift.

- **CI gate cleared on first attempt.** PROGRESS Task 13 reports build/clippy/fmt/test/deny all clean with no fix-during-gate commits. Local spot-checks of `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` on HEAD `02a9add` both exit 0 — consistent with the claim. This matches phase-02.1's first-attempt-green pattern (a material improvement over phase-01's two state-4 fix rounds).

- **Commit-message hygiene matches phase-01/02.1 precedent.** ADR-tagged commits `435c6fa` (`phase 02.2: ADR-0015/0016 — host.docker.internal + enable_half_close defaults`) and `aa4187f` (`phase 02.2: differential — backend keys + run_fixture dispatch + with_host [ADR-0015]`) follow the `[ADR-NNNN]` pattern. The Cargo.lock sync at `2146014` follows the precedent set by `4955252` (phase-01) and `dea4d16` (phase-02.1) — single-file commit, narrative message naming the phase tasks that caused the drift. One commit per task + paired "progress note" commits keeps the cadence consistent.

---

## 3. Issues

### Critical

None.

### Important

**I1. `STATE.md` is stale by two lifecycle steps; must advance in the same commit that lands REVIEW.md.** *(Closed in-phase — see §7.)*

`/Users/esa/git/envoy-rust/docs/envoy-rust/STATE.md:10-13` at HEAD `02a9add` reads:

```
status: phase 02.2 lifecycle state 3 (PLAN.md exists, implementation incomplete) — SPEC.md landed at commit 1c38ca9 …; PLAN.md landed at commit ad90db6 (this session).
```

This was the state 2→3 snapshot from commit `7504b86`. All 13 PLAN.md tasks are implementation-complete; the state-4 gate cleared; this REVIEW is the state-5 input. The phase-02.1 precedent (REVIEW.md §3 I2 / §7 close-out) shows the same finding and the same shape of remediation: STATE.md advances in the same commit that lands REVIEW.md. Phase-01 `f436c29` is the deeper precedent.

*Why it matters:* STATE.md is the single-source-of-truth for "what next" per its own top-of-file docstring. A stranger cold-starting at HEAD `02a9add` and reading STATE.md would route to `superpowers:subagent-driven-development` for state-3 execution — already complete. Not a D-3.5 doctrine violation (BOOTSTRAP_PROMPT.md §5.1 is "one state per session," not "one STATE.md commit per state"), but a material readability regression vs. the phase-01/02.1 precedent.

*Fix:* State 5 closes with a commit that (a) lands this REVIEW.md and (b) advances STATE.md to state 5 (approved; implementation frozen; state-6 next). State 6 then lands the ROADMAP `02.2`+`02` flips and STATE advance to phase 03 per SPEC §8. Closed by this REVIEW.md commit — see §7.

### Minor

**M1. `TcpProxyBackend::Drop` polling loop blocks on `std::thread::sleep` from a tokio-runtime thread.** *(Tracked forward.)*

`/Users/esa/git/envoy-rust/tests/differential/src/backend.rs:73-83` polls `child.try_wait()` in a `std::thread::sleep(Duration::from_millis(50))` loop with a 2s deadline. The synchronous sleep is unavoidable in a `Drop` impl (which is `!Send` to the async runtime — Drop can't `await`), and the comment at lines 73-74 names the constraint correctly. However, if Drop fires while a tokio worker thread is parked on this `child`, the `std::thread::sleep` blocks that worker for up to 2s — depending on runtime configuration this can stall other concurrently-running tests' progress. The `kill_on_drop(true)` at line 39 means the runtime's signal-driver also reaps the child via SIGKILL on Drop, so the polling loop usually terminates on the first poll (`Ok(Some(_))`) and the worst-case 2s stall only happens if the child hasn't yet been reaped.

*Why it matters:* this is fine for the current usage (one fixture per `cargo test` invocation), but if a later phase parallelizes `run_fixture` across worker threads, a 2s Drop stall could compound. Phase 02.2's tests aren't parallelized that way, so no observed flake.

*Fix:* leave as-is for 02.2. If a later phase adds parallel fixture execution, switch the polling loop to spawn-detach a tokio task that calls `child.wait().await` — but that requires a `Handle::current()` reference at Drop time, which adds plumbing. Tracked forward as a "if and when fixture parallelism arrives" concern.

**M2. `proxies_returns_err_on_upstream_connect_refused` asserts on the formatted error string rather than the typed variant.**

`/Users/esa/git/envoy-rust/crates/envoy-tcp/src/lib.rs:289-296` asserts `formatted.contains("connecting to upstream 127.0.0.1:1")` rather than `matches!(*err.downcast::<TcpProxyError>(), Ok(TcpProxyError::UpstreamConnect { .. }))`. The current shape is correct — the error has been boxed as `Box<dyn std::error::Error + Send + Sync>` per the trait's signature and the formatted-string check verifies the Display impl matches the `#[error("connecting to upstream {addr}: {source}")]` literal. But a future change to the `#[error(...)]` literal would silently break the assertion's intent without breaking the test.

*Fix (optional):* leave as-is. The formatted-string assertion is reasonable for a boxed `dyn Error` return shape; a `downcast` would require `Box<dyn Any>` plumbing that isn't worth the noise. Flagging for awareness only.

**M3. `proxies_closes_downstream_on_upstream_close` has implicit timing on the upstream's "tail" read.**

`/Users/esa/git/envoy-rust/crates/envoy-tcp/src/lib.rs:199-202` writes back the echo, calls `stream.shutdown().await.ok()`, then reads up to 16 bytes into a `tail` buffer "to hold the read side open briefly so the downstream can drain." The exact wait is implicit — bounded by the downstream client's `read_to_end` timeout (2s) at line 222. Fine in practice, but the comment understates the dependency: the `read(&mut tail).await` is what keeps the upstream socket from being closed before the proxy's `select!` resolves. Test passes consistently and the timing is fine; flagging for future-readers.

*Fix:* leave as-is.

**M4. `Listener::serve`'s `JoinSet` type aliases a pretty-long generic.**

`/Users/esa/git/envoy-rust/crates/envoy-listener/src/lib.rs:113-115` declares `let mut join_set: tokio::task::JoinSet<Result<(), Box<dyn std::error::Error + Send + Sync>>> = tokio::task::JoinSet::new();`. The same `Result<(), Box<dyn std::error::Error + Send + Sync>>` shape recurs in the `ConnectionHandler::handle` return type, the `EchoHandler` test impl, and the `StalledHandler` test impl. A type alias `pub type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;` would reduce the noise without changing the public surface. Not a blocker — modern rustc handles the explicit form fine.

*Fix:* leave as-is. Phase 03 / 04 may revisit if a richer filter trait (phase 07) needs the alias for documentation.

---

## 4. Recommendations

**Forward to phase 03:**

1. **Close I1 in §7.** STATE.md advance. Mechanical text change; same shape as phase-02.1's §7 I2 close-out.

2. **Add `Cluster::name()` accessor when phase 03's TLS work or phase 06's stats first need it.** Currently `envoy-tcp` carries `cluster_name: String` separately (correct call given the deferral at phase-02.1 REVIEW M1). Whichever future phase first reaches for stat-name attribution or trace span attribution should add `pub(crate) fn Cluster::name(&self) -> &str` and remove the `#[allow(dead_code)]` on `Cluster.name`. The comment at `crates/envoy-tcp/src/lib.rs:14-16` already cross-references the deferral.

3. **Phase 03 ADR projection numbering should treat ADR-0017 as provisional.** Same caveat as phase-02.1 REVIEW §4: if a `cargo deny` trigger or doctrine-delta lands an interim ADR between 02.2 done and 03 start, the projected numbers shift.

**Forward to later phases (carry-over from phase-02.1 REVIEW §4):**

4. **`TypedConfig` enum will grow one variant per filter** across phases 04 (HTTP CM), 05 (HTTP/2), 06 (stats/access-logs). Carries over unchanged.

5. **Round-robin distribution-equivalence assertion** remains unit-test-only (parent-brainstorm Q1 decision). Carries over unchanged.

6. **If parallel fixture execution arrives** (a future phase's harness widening), revisit `TcpProxyBackend::Drop` per M1 above.

7. **`enable_half_close: true` flip-fixture** is the obvious follow-on per ADR-0016. Whichever phase first needs an asymmetric-close use case lands its own ADR + extends `TcpProxy` with an explicit half-close-propagation mode.

---

## 5. Files reviewed

Absolute paths opened during this review:

- `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md` (D-3.2 permitted-foundations list)
- `/Users/esa/git/envoy-rust/Cargo.toml`
- `/Users/esa/git/envoy-rust/Cargo.lock` (via `git diff`)
- `/Users/esa/git/envoy-rust/crates/envoy-listener/Cargo.toml`
- `/Users/esa/git/envoy-rust/crates/envoy-listener/src/lib.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-tcp/Cargo.toml`
- `/Users/esa/git/envoy-rust/crates/envoy-tcp/src/lib.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-bin/Cargo.toml`
- `/Users/esa/git/envoy-rust/crates/envoy-bin/src/main.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-bin/src/admin.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-bin/tests/tcp_proxy.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-cluster/src/cluster.rs` (verified `pick_endpoint`/`ClusterHandle` API)
- `/Users/esa/git/envoy-rust/tests/differential/src/lib.rs`
- `/Users/esa/git/envoy-rust/tests/differential/src/backend.rs`
- `/Users/esa/git/envoy-rust/tests/differential/src/upstream.rs`
- `/Users/esa/git/envoy-rust/tests/differential/src/subject.rs`
- `/Users/esa/git/envoy-rust/tests/differential/Cargo.toml` (no diff — already had testcontainers + tracing)
- `/Users/esa/git/envoy-rust/tests/differential/tests/tcp_proxy.rs`
- `/Users/esa/git/envoy-rust/tests/fixtures/0003-tcp-proxy/envoy.yaml`
- `/Users/esa/git/envoy-rust/tests/fixtures/0003-tcp-proxy/envoy-rust.yaml`
- `/Users/esa/git/envoy-rust/tests/fixtures/0003-tcp-proxy/expectations.yaml`
- `/Users/esa/git/envoy-rust/tests/fixtures/0003-tcp-proxy/README.md`
- `/Users/esa/git/envoy-rust/tests/fixtures/0003-tcp-proxy/inputs/payload.bin` (verified byte-identical to `tests/fixtures/0001-tcp-echo/inputs/payload.bin`, 18 bytes)
- `/Users/esa/git/envoy-rust/docs/envoy-rust/DECISIONS.md` (verified ADR-0015 + ADR-0016 appended; ADR-0001–0014 byte-identical via diff)
- `/Users/esa/git/envoy-rust/docs/envoy-rust/STATE.md` (verified stale per I1)
- `/Users/esa/git/envoy-rust/docs/envoy-rust/SKILL_ROUTING.md` (verified state-5 transition rules)
- `/Users/esa/git/envoy-rust/docs/envoy-rust/phases/02.2-listener-tcp-proxy/SPEC.md` (the design contract — full read)
- `/Users/esa/git/envoy-rust/docs/envoy-rust/phases/02.2-listener-tcp-proxy/PROGRESS.md`
- `/Users/esa/git/envoy-rust/docs/envoy-rust/phases/02.1-config-cluster/REVIEW.md` (shape precedent + rollover cross-reference)

Local commands run:

- `git -C /Users/esa/git/envoy-rust diff --stat d447f53..02a9add` → 24 files, +4363/-65.
- `git -C /Users/esa/git/envoy-rust log --oneline d447f53..02a9add` → 28 commits incl. 13 task + 12 progress-note + 1 ADR + 1 cargo.lock-sync + 1 state-4 verification.
- `git -C /Users/esa/git/envoy-rust diff d447f53..02a9add -- docs/envoy-rust/DECISIONS.md` → only +ADR-0015 +ADR-0016, zero `^-## ADR-` lines.
- `git -C /Users/esa/git/envoy-rust diff 02a9add~2..2146014 -- Cargo.lock` → exactly the two new package stanzas + envoy-bin's three new path-deps.
- `git -C /Users/esa/git/envoy-rust diff 2146014..02a9add` → only `PROGRESS.md` Task 13 narrative (no other drift).
- `git -C /Users/esa/git/envoy-rust diff d447f53..02a9add -- rust-toolchain.toml` → empty (toolchain pin untouched).
- `cargo fmt --all -- --check` → exit 0 (no output).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → exit 0 (no output beyond `Finished` line).
- `cargo test --workspace --lib --bins --no-run` → all targets build clean.
- `grep -n '#!\[forbid(unsafe_code)\]' crates/envoy-{listener,tcp}/src/lib.rs` → both at line 1.
- `grep -c '^## ADR-' docs/envoy-rust/DECISIONS.md` → 17 (16 ADRs + 1 template heading at line 10).
- `diff tests/fixtures/000{1-tcp-echo,3-tcp-proxy}/inputs/payload.bin` → identical (18 bytes each).

---

## 6. Initial verdict

**Approved with fixes** (initial review, HEAD `02a9add`).

No Critical blockers. One Important finding (I1, STATE.md stale by two lifecycle steps), mechanical and remediated in §7. Four Minor findings, all tracked forward to phase 03 or later — none touch production code or block state 5/6.

The two new crates (`envoy-listener`, `envoy-tcp`), the envoy-bin wiring, the I4 admin cap close-out, the differential harness extensions, the fixture 0003 surface, ADR-0015 + ADR-0016, and the phase-02.1 rollover M3 closure are all shape-correct, test-backed, and doctrine-compliant. The executor's self-audit discipline (Task 5 `AddressParse` 4th variant, Task 8 `select!` vs. `try_join!` deviation, Task 11 opportunistic M3 closure) and first-attempt CI-gate greenness are material positive signals. State 5 may complete in this session by committing REVIEW.md + the STATE-advance per §7; state 6 then lands the ROADMAP `02.2`+`02` flips, advances STATE.md to phase 03, and uses commit-message format `phase 02.2: Listener + TCP proxy filter + fixture 0003 [ADR-0015,ADR-0016]` per SPEC §9.

---

## 7. State-5 close-out — I1 remediation (2026-04-25)

I1 is a mechanical remediation (STATE.md text advance) that does not touch production code, does not alter doctrine, and does not change the review's technical findings. A narrow re-review by `superpowers:code-reviewer` is not warranted — the precedent (phase-02.1 §7 closing I2 in the same shape) already validated this close-out form.

### I1 — STATE.md advance

- Commit: this commit (lands alongside REVIEW.md).
- Diff: `docs/envoy-rust/STATE.md` — `status:` advanced from "state 3 (PLAN.md exists, implementation incomplete)" to "state 5 (REVIEW.md approved; state-6 next)"; "Next expected skill" rewritten for the state-6 phase-done gate; "Last commit" reference updated; "Last updated" stamp refreshed. No `Notes` section rewriting; rollover tracking (M1–M4) delegated to this REVIEW.md §3–§4.
- Phase-02.1 precedent followed: `379937b` shape (STATE-advance commit that lands REVIEW.md and flips STATE.md in one atomic move).

### M1–M4

Tracked forward per §3 and §4. None are state-5 or state-6 blockers.

### Final verdict

**Approved** (state 5 complete). HEAD is the commit landing this REVIEW.md + STATE-advance. Next session executes state 6: phase-done commit that flips ROADMAP rows `02.2` and `02` (parent) to `done` in the same commit, advances STATE.md to phase `03-tls-tcp` (lifecycle state 1, directory does not yet exist, next-skill `superpowers:brainstorming`), and uses commit-message format `phase 02.2: Listener + TCP proxy filter + fixture 0003 [ADR-0015,ADR-0016]` per SPEC §9 / `BOOTSTRAP_PROMPT.md` §5.3. State 6 does not require further review work.
