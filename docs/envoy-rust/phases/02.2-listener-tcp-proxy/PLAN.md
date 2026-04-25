# Phase 02.2 — Listener + TCP Proxy Filter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task (fresh subagent per task + two-stage review). Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **For every code-writing task:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — failing test first, verify fails, minimal implementation, verify passes, commit. No exceptions (doctrine D-3.1).
>
> **Source of truth:** `docs/envoy-rust/phases/02.2-listener-tcp-proxy/SPEC.md`. This plan operationalizes SPEC §§D1–D8. Where the plan and the SPEC disagree, the SPEC wins — flag the drift, land an ADR per D-3.5, and continue. The parent phase-02 SPEC at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (committed at SHA `50349da`) is preserved unedited as a historical artifact; for execution it is superseded by sub-phase SPECs (02.1's done; 02.2's the one this plan implements).

**Goal:** Land the two new library crates `envoy-listener` (bind/accept/drain) and `envoy-tcp` (TCP proxy filter), wire them into `envoy-bin`, extend the differential harness to support host-local backends, and ship the end-to-end fixture `0003-tcp-proxy` byte-exact green against upstream Envoy `v1.33.0`. Close phase-01 REVIEW §9 starter items I4 (admin 8 KiB header cap tightening) and M1 (stale `TODO(phase-01)` retarget).

**Architecture:** `crates/envoy-listener/` owns the listener binding, accept loop, and graceful drain. It exposes a `ConnectionHandler` trait whose `handle` method takes a `tokio::net::TcpStream` and returns a `BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>` — hand-boxed so we keep object-safety without pulling `async-trait` (not on the D-3.2 permitted-foundations list). `crates/envoy-tcp/` implements `ConnectionHandler` for `TcpProxy`: pick an upstream endpoint via the 02.1-landed `envoy_cluster::ClusterHandle`, dial it plaintext, run `tokio::io::copy` in both directions inside a `tokio::select!` so a clean EOF on either side immediately drops the other future and closes its write half (matches ADR-0016's `enable_half_close: false` posture; deliberate plan-time deviation from SPEC §D2 step 4's `try_join!` — see Task 8 header). `envoy-bin::main::run` dispatches on the listener's single filter — `envoy.filters.network.echo` keeps the phase-01 path; `envoy.filters.network.tcp_proxy` constructs a `TcpProxy` and serves under `Listener::serve`. The differential harness gains a `TcpProxyBackend` helper that locates and runs the phase-02.1 `tcp-echo-server` binary as a host subprocess; `render_yaml` learns three new substitution keys (`{{BACKEND_PORT}}`, `{{BACKEND_HOST}}`, plus the existing `{{PORT}}` for both fixtures); `run_fixture` detects `{{BACKEND_PORT}}` in either template and spawns the backend before launching the proxies; the upstream-Envoy testcontainers config gains `with_host("host.docker.internal", Host::HostGateway)` so the upstream container can reach the host-running backend (per ADR-0015).

**Tech stack:** Rust edition 2024 on pinned stable `1.95.0` (D-3.9). `tokio` (`rt`, `net`, `macros`, `time`, `sync`, `io-util`) is the async substrate. `thiserror` for typed library errors. `tracing` for spans/logs. `testcontainers = "0.23"` (already in scope from 02.1 via ADR-0005) for the upstream-Envoy container with the new `with_host` invocation. No new direct deps on the D-3.2 forbidden list; no new transitive licenses expected through `envoy-listener`'s and `envoy-tcp`'s `tokio` feature additions (covered transitively by `envoy-bin` already; `cargo deny check` re-runs in the state-4 gate).

---

## File structure (created / modified)

**Created:**

- `crates/envoy-listener/Cargo.toml`
- `crates/envoy-listener/src/lib.rs` (single-file crate; tests in `#[cfg(test)] mod tests`)
- `crates/envoy-tcp/Cargo.toml`
- `crates/envoy-tcp/src/lib.rs` (single-file crate; tests in `#[cfg(test)] mod tests`)
- `crates/envoy-bin/tests/tcp_proxy.rs` (Rust-native integration test — backstop)
- `tests/differential/src/backend.rs` (new module; `TcpProxyBackend` helper + 2 unit tests)
- `tests/differential/tests/tcp_proxy.rs` (Docker-gated acceptance test)
- `tests/fixtures/0003-tcp-proxy/envoy.yaml`
- `tests/fixtures/0003-tcp-proxy/envoy-rust.yaml`
- `tests/fixtures/0003-tcp-proxy/inputs/payload.bin` (copy of fixture 0001's payload — 18 bytes `b"hello, envoy-rust\n"`)
- `tests/fixtures/0003-tcp-proxy/expectations.yaml`
- `tests/fixtures/0003-tcp-proxy/README.md`
- `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PROGRESS.md` (appended once per task during execution)

**Modified:**

- Root `Cargo.toml` — add `crates/envoy-listener` and `crates/envoy-tcp` to `[workspace] members`. (`crates/envoy-cluster` and `tests/helpers/tcp-echo-server` are already there from 02.1.)
- `crates/envoy-bin/Cargo.toml` — add `envoy-cluster`, `envoy-listener`, `envoy-tcp` path deps.
- `crates/envoy-bin/src/main.rs` — construct `ClusterManager`; dispatch listener setup between `echo::serve` (echo filter) and `Listener::serve` + `TcpProxy` (tcp_proxy filter).
- `crates/envoy-bin/src/admin.rs` — apply I4 read-slice tightening; update existing `rejects_oversized_request_headers` test; add `accepts_requests_exactly_at_cap` test.
- `tests/differential/src/lib.rs` — add `pub mod backend;`; extend `render_yaml` per-driver substitution to include `{{BACKEND_PORT}}` + `{{BACKEND_HOST}}` when the template uses them; extend `run_fixture` to spawn a `TcpProxyBackend` when the template contains `{{BACKEND_PORT}}`; drop the dead `|| msg.contains("CRLF")` disjunct (REVIEW.md M3); add 3 new unit tests (`fixture_0003_expectations_parses_as_tcp_echo`, `render_yaml_substitutes_backend_keys_for_envoy_side`, `render_yaml_substitutes_backend_keys_for_envoy_rust_side`).
- `tests/differential/src/upstream.rs` — extend `start` to call `with_host("host.docker.internal", Host::HostGateway)` when fixture uses a backend (= when the rendered upstream YAML references `host.docker.internal`).
- `tests/differential/src/subject.rs` — retarget `TODO(phase-01)` comment per phase-01 REVIEW §9 M1 (doc-only; no functional change).
- `docs/envoy-rust/DECISIONS.md` — append ADR-0015 and ADR-0016 (Task 1).
- `docs/envoy-rust/ROADMAP.md` — at state 6 only, flip row `02.2` `status` → `done` AND row `02` (parent) `status` → `done` in the same commit (per ROADMAP schema).
- `docs/envoy-rust/STATE.md` — at state 6 only, advance to phase `03-tls-tcp` (lifecycle state 1; phase-03 directory does not yet exist; next-skill `superpowers:brainstorming`).
- `Cargo.lock` — sync as a dedicated commit (mirrors phase-01 `4955252` and phase-02.1 `dea4d16`) once Task 13's gate exposes drift.
- `deny.toml` — only if `cargo deny check` flips on a new transitive surface (expected: no — `tokio`'s feature graph is already in scope via `envoy-bin`).

**Note: not touched in 02.2.** `crates/envoy-cluster/`, `crates/envoy-config/`, `tests/helpers/tcp-echo-server/`, `crates/envoy-config/fuzz/`, parent `02-tcp-proxy/SPEC.md`, `02.1-config-cluster/` — all finalized in 02.1.

---

## Task index

Each task ends with a commit. Per phase-02.1 convention, follow each task commit with a `phase 02.2: progress note (task N)` commit that appends the matching PROGRESS.md section (commit SHA, change summary, verification output, any deviation). The phase-02.1 commit log shows this pattern: `535e6f9` (task 11) → `ddb1c2e` (progress note 11), `ef90cf3` (task 12) → `cadeaa6` (progress note 12), etc. Choose one cadence and keep it.

1. **ADRs 0015 + 0016 — host.docker.internal + enable_half_close:false defaults**
2. **Phase-01 rollover M1 — retarget stale `TODO(phase-01)` in `tests/differential/src/subject.rs`**
3. **Phase-01 rollover I4 — admin 8 KiB read-slice tightening + 2 boundary tests**
4. **Scaffold `crates/envoy-listener/` skeleton + workspace member**
5. **`envoy-listener::Listener::bind` + `ConnectionHandler` trait + `BoxFuture` + `ListenerError` + 2 tests**
6. **`envoy-listener::Listener::serve` + 5s drain budget + abort-stragglers + 4 tests**
7. **Scaffold `crates/envoy-tcp/` skeleton + workspace member**
8. **`envoy-tcp::TcpProxy` + `ConnectionHandler` impl + `TcpProxyError` + 4 tests**
9. **`envoy-bin` wiring — `ClusterManager` + filter dispatch + `crates/envoy-bin/tests/tcp_proxy.rs` integration test**
10. **`tests/differential/src/backend.rs` — `TcpProxyBackend` helper + 2 unit tests**
11. **Differential harness: `render_yaml` backend keys + `run_fixture` dispatch + upstream `with_host` + 3 unit tests + drop M3 disjunct**
12. **Fixture `tests/fixtures/0003-tcp-proxy/` (envoy.yaml, envoy-rust.yaml, inputs/payload.bin, expectations.yaml, README.md) + Docker-gated `tests/differential/tests/tcp_proxy.rs`**
13. **State 4 phase-done gate — run all 5 stable commands + CI fuzz job; quote outputs into PROGRESS.md**

Estimated total: ~14 tasks (with `Cargo.lock` sync potentially Task 14 follow-up if the gate exposes drift), ~1120 LoC. Both `BOOTSTRAP_PROMPT.md` §6.1 gates (~25 tasks / ~1500 LoC) hold comfortably. **Do not split 02.2 further.** If any single task balloons past ~10 sub-steps mid-execution, invoke `superpowers:systematic-debugging` before attempting a nested split — nested splits of an already-split sub-phase deserve a fresh root-cause read (per SPEC §5 closing paragraph).

---

### Task 1: ADRs 0015 + 0016 — host.docker.internal + enable_half_close:false

**Files:**
- Modify (append): `docs/envoy-rust/DECISIONS.md`
- Create: `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PROGRESS.md`

**Why first:** every subsequent task cites at least one of these ADRs. DECISIONS.md is append-only per D-3.5; land the rationale before the code that references it. ADR-0015 is referenced by Task 11 (upstream `with_host`) and Task 12 (fixture 0003 `{{BACKEND_HOST}}` divergence). ADR-0016 is referenced by Task 8 (`TcpProxy::handle` half-close posture) and Task 12 (fixture 0003 omits `enable_half_close`). Verify before starting that DECISIONS.md ends at ADR-0014; if any new ADR landed between phase 02.1 done and 02.2 start, both ADR numbers shift by +1 (per phase-02.1 REVIEW §4 recommendation #2). At the time this plan was written, DECISIONS.md ends at ADR-0014 and no in-flight ADR was anticipated, so 0015/0016 are the expected next-sequential numbers.

- [ ] **Step 1: Verify next-sequential ADR numbers.**

```bash
grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md
grep -n '^## ADR-00' docs/envoy-rust/DECISIONS.md | tail -3
```

Expected output: `14`; last three lines are `ADR-0012`, `ADR-0013`, `ADR-0014`. If any unexpected `ADR-00NN` appears, rebase this task's text by `+1` for each interloper before continuing — this is a mechanical renumber per SPEC §3 D8 + phase-02.1 REVIEW §4 recommendation #2. Update every cross-reference in this PLAN to the new numbers as part of Task 1 (search-and-replace `ADR-0015` and `ADR-0016`).

- [ ] **Step 2: Append ADR-0015 (`host.docker.internal` + `host-gateway`) to `docs/envoy-rust/DECISIONS.md`.**

Append after the final `---` of ADR-0014 using the structure mandated by DECISIONS.md lines 9–19. Use these exact field contents:

```markdown
## ADR-0015: Cross-container host reachability via `host.docker.internal` + `host-gateway`

- Date: 2026-04-25
- Status: accepted
- Context: Sub-phase 02.2's fixture `0003-tcp-proxy` exercises a TCP proxy whose upstream backend is the in-tree `tcp-echo-server` binary (landed in 02.1) running as a host process. The upstream Envoy container (started via `testcontainers` per ADR-0004/0005) and the envoy-rust host subprocess must both reach this single backend. Container-to-host networking is platform-dependent: Docker Desktop (macOS, Windows, Linux) resolves `host.docker.internal` natively; Linux bridge networks require `--add-host=host.docker.internal:host-gateway` to teach the container the hostname. `testcontainers = "0.23.3"` exposes this via `ImageExt::with_host(name: impl Into<String>, value: impl Into<Host>)` with `Host::HostGateway` (verified at `testcontainers::core::Host::HostGateway`).
- Options considered:
  - **(i) Always-on `host.docker.internal` injected via `with_host(..., Host::HostGateway)` on the upstream container.** Standardizes on one hostname across macOS dev, Linux dev, and `ubuntu-latest` CI. testcontainers handles the Docker-side plumbing.
  - **(ii) Runtime platform detection (`/.dockerenv`, `uname -r`, `docker info`) with `172.17.0.1` as a Linux-bridge fallback.** Two code paths; brittle against Docker config drift (rootless Docker reassigns the bridge IP).
  - **(iii) Run the backend inside a Docker container on a shared network.** Loses the "backend is a host process" property and pulls container-network management into every fixture's setup — premature complexity for a 1:1 echo backend.
- Decision: (i). The upstream-Envoy container gains `with_host("host.docker.internal", Host::HostGateway)` whenever the rendered upstream YAML references `host.docker.internal`. Fixture 0003's `envoy.yaml` references `host.docker.internal:{{BACKEND_PORT}}`; `envoy-rust.yaml` references `127.0.0.1:{{BACKEND_PORT}}`. The harness substitutes both keys per side via `render_yaml` (Task 11).
- Rationale: one code path across macOS dev, Linux dev, and `ubuntu-latest` CI; testcontainers already supports the API natively under the existing exemption from ADR-0005. The "configs are initially identical" fixture principle (phase-01 §3 fixture-grammar) is preserved because the `{{BACKEND_HOST}}` substitution map is per-side mechanics, not a YAML-level divergence.
- Consequences:
  - Every future fixture with a host-local backend follows the same pattern. Fixtures without a backend (0001, 0002) skip the `with_host` call (the harness gates it on whether the rendered YAML references `host.docker.internal`).
  - If a later phase needs a backend inside a Docker network (e.g., a multi-proxy topology), that phase lands a separate testcontainers-networking ADR. ADR-0015 covers single-backend host-process reachability only.
  - If `ubuntu-latest`'s Docker daemon ever refuses `host-gateway` (very unlikely; the feature has been GA since Docker CE 20.10 — see SPEC §6 signpost 4), the fallback is `172.17.0.1` (default Linux-bridge gateway) under a follow-up ADR. The `with_host` call would error at container start, surfacing the platform deficiency loudly rather than silently.
```

- [ ] **Step 3: Append ADR-0016 (`enable_half_close: false` default) to `docs/envoy-rust/DECISIONS.md`.**

```markdown
## ADR-0016: Phase 02 TCP proxy runs with Envoy's default `enable_half_close: false`

- Date: 2026-04-25
- Status: accepted
- Context: ADR-0006/0007 documented the upstream-Envoy half-close-drops-pending-writes subtlety for the echo filter and the subsequent `drive_tcp` `read_exact(payload.len())` + 100ms trailing-byte poll pattern. Sub-phase 02.2 introduces `envoy.filters.network.tcp_proxy`, which exposes a YAML-visible `enable_half_close: true` toggle (unlike the echo filter, which has none). Fixture 0003's client pattern (`drive_tcp`: write payload → `read_exact(payload.len())` → 100ms trailing poll → graceful `shutdown()` + drop) does not depend on FIN propagation between downstream and upstream, only on the deterministic 1:1 byte-count contract.
- Options considered:
  - **(i) Leave the default `false` on both `envoy.yaml` and the envoy-rust config.** Matches Envoy v1.33.0's tcp_proxy default; minimal fixture YAML; envoy-rust's `TcpProxy::handle` mirrors the posture by running plain `tokio::io::copy` in both directions and propagating EOF via drop.
  - **(ii) Set `true` on both sides.** Pre-positions for FIN-sensitive use cases at the cost of YAML and Rust code that doesn't yet matter.
  - **(iii) Set `true` on one side only.** Divergent behavior under identical inputs; violates the "configs are initially identical" fixture principle (modulo bind address and harness substitutions).
- Decision: (i). `enable_half_close` is absent from both `tests/fixtures/0003-tcp-proxy/envoy.yaml` and `envoy-rust.yaml`. envoy-rust's `TcpProxy::handle` (Task 8) is implemented to match: `tokio::io::copy` on both directions, EOF on either side propagates via drop of the write half.
- Rationale: matches Envoy v1.33.0's default tcp_proxy posture; `drive_tcp`'s 1:1 echo client pattern doesn't need half-close propagation; minimal fixture keeps reviewer diffing tight. The ADR-0006/0007 precedent — "narrow fix, leave the grammar for when it pays for itself" — applies to the YAML toggle here too.
- Consequences:
  - Phase 02.2's TCP proxy is explicitly *not* a drop-in for every Envoy `tcp_proxy` deployment; use cases depending on half-close propagation belong to a phase-later. A future fixture with an asymmetric-close requirement (one side writes, then expects the other side's FIN to trigger a response) lands its own ADR flipping the toggle and extending `TcpProxy` with a half-close-propagation mode. Until then, `enable_half_close` is a known non-surface.
  - SPEC §6 signpost 6 cautions against "defensively" including `enable_half_close: false` in the YAML — review should flag any future fixture or PR that adds a redundant `enable_half_close: false` key.
  - The `tokio::io::copy` propagation property (SPEC §6 signpost 5) is preserved: if downstream→upstream succeeds while upstream→downstream errors, `try_join!` returns the error and drops the surviving future; `Drop` on the write halves closes the sockets, which RSTs the open direction. That aligns with Envoy's behavior of closing both sides on an asymmetric error.
```

- [ ] **Step 4: Create `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PROGRESS.md` with a Task 1 section.**

Content:

```markdown
# Phase 02.2 Progress

## Task 1 — ADRs 0015 + 0016 (2026-04-25)

- Commit: <SHA>
- Change: appended ADR-0015 (cross-container host reachability via host.docker.internal + host-gateway) and ADR-0016 (phase 02 TCP proxy runs with Envoy's default enable_half_close: false) to DECISIONS.md.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 16 (ADR-0001 through ADR-0016).
```

Replace `<SHA>` with the commit hash from Step 6 (or land it in the matching `progress note (task 1)` follow-up commit per the phase-02.1 cadence).

- [ ] **Step 5: Verify DECISIONS.md.**

```bash
grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md
```

Expected: `16`.

```bash
grep -n '^## ADR-00' docs/envoy-rust/DECISIONS.md | tail -3
```

Expected (last 3 lines): `ADR-0014`, `ADR-0015`, `ADR-0016` in that order, with ascending line numbers.

- [ ] **Step 6: Commit.**

```bash
git add docs/envoy-rust/DECISIONS.md docs/envoy-rust/phases/02.2-listener-tcp-proxy/PROGRESS.md
git commit -m "phase 02.2: ADR-0015/0016 — host.docker.internal + enable_half_close defaults"
```

Then either amend the SHA into PROGRESS.md (phase-01 PLAN Task 1 idiom) or land a follow-up `phase 02.2: progress note (task 1)` commit (phase-02.1 PROGRESS idiom). Either is acceptable; pick one cadence and keep it for every subsequent task.

---

### Task 2: Phase-01 rollover M1 — retarget stale `TODO(phase-01)` in `tests/differential/src/subject.rs`

**Files:**
- Modify: `tests/differential/src/subject.rs:25–32`

**Why now:** doc-only, isolated, no test churn. Lands before any `tests/differential/src/lib.rs` work in Task 11 to keep that task's diff focused on harness behavior changes. Per SPEC §3 D6 / phase-01 REVIEW §9 M1.

- [ ] **Step 1: Read the current comment.**

```bash
sed -n '20,35p' tests/differential/src/subject.rs
```

Expected: the existing `// TODO(phase-01): switch to SIGTERM + drain-wait + SIGKILL-escalate …` block at lines 25–32.

- [ ] **Step 2: Replace the comment with an open-ended deferral note.**

Replace the entire block from `// TODO(phase-01): switch to SIGTERM …` through `// SIGTERM drain behavior is validated by the envoy-bin unit tests.` (lines 25–32) with:

```rust
    // TODO: switch to SIGTERM + drain-wait + SIGKILL-escalate so the harness
    // exercises envoy-bin's graceful-drain path. Sending POSIX signals to a
    // `tokio::process::Child` requires the `nix` crate (or equivalent
    // POSIX-signal surface), which is not on the D-3.2 permitted-foundations
    // list. Phase 00 deferred this to phase 01; phase 01 (and phase 02 across
    // 02.1 and 02.2) chose not to take `nix` either, so the deferral is
    // open-ended — no specific target phase. A future phase that genuinely
    // needs `nix` lands it under a new ADR and closes this TODO. Until then,
    // SIGTERM drain behavior is validated by the envoy-bin unit tests.
```

- [ ] **Step 3: Verify the build still passes (doc-only change).**

```bash
cargo build -p differential --all-targets
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

All three: exit 0.

- [ ] **Step 4: Commit.**

```bash
git add tests/differential/src/subject.rs
git commit -m "phase 02.2: retarget TODO(phase-01) to open-ended nix-crate deferral"
```

Append a Task 2 section to PROGRESS.md per the cadence chosen in Task 1.

---

### Task 3: Phase-01 rollover I4 — admin 8 KiB read-slice tightening + 2 boundary tests

**Files:**
- Modify: `crates/envoy-bin/src/admin.rs` (production change at line ~165; existing test update at line ~303; new test appended)

**Why now:** still a `crates/envoy-bin/` change, sitting alongside Task 9's `envoy-bin` wiring. Lands before the wiring task so Task 9's diff is focused on filter dispatch only. Per SPEC §3 D6 / phase-01 REVIEW §9 I4. SPEC §6 signpost 12: the unit-test delta is two tests, not one — the existing rejection test moves to exactly `MAX_REQUEST_HEAD + 1` bytes; the new acceptance test pins the cap-boundary behavior.

- [ ] **Step 1: Write the failing acceptance-at-cap test.**

Append to `crates/envoy-bin/src/admin.rs::tests` (the existing module starting at line 199), after `rejects_oversized_request_headers`:

```rust
    /// Phase 02.2 I4: a request whose total request-head length is exactly
    /// `MAX_REQUEST_HEAD` and which terminates with CRLF-CRLF must parse and
    /// produce a normal HTTP response (here, 404 because the path is unknown).
    /// Pre-tightening, the read could grow `buf` past `MAX_REQUEST_HEAD` by up
    /// to `scratch.len()` bytes before the loop's bound check fired, masking
    /// the real cap-boundary behavior. With the bounded read
    /// `stream.read(&mut scratch[..MAX_REQUEST_HEAD - buf.len()])`, the
    /// boundary is exact: 8192 bytes parse cleanly; 8193 bytes return 431.
    #[tokio::test]
    async fn accepts_requests_exactly_at_cap() {
        let listener = bind_random_local().await;
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            serve(listener, async move {
                let _ = rx.await;
            })
            .await
            .unwrap();
        });

        // Build a complete HTTP/1.1 request whose total wire length is
        // exactly MAX_REQUEST_HEAD = 8192 bytes, ending in CRLF-CRLF.
        let prefix = b"GET /unknown HTTP/1.1\r\nHost: x\r\nX-Pad: ";
        let suffix = b"\r\n\r\n";
        let pad_len = MAX_REQUEST_HEAD - prefix.len() - suffix.len();
        let mut req: Vec<u8> = Vec::with_capacity(MAX_REQUEST_HEAD);
        req.extend_from_slice(prefix);
        req.extend(std::iter::repeat_n(b'A', pad_len));
        req.extend_from_slice(suffix);
        assert_eq!(req.len(), MAX_REQUEST_HEAD);

        let resp = drive(addr, &req).await;
        let s = std::str::from_utf8(&resp).unwrap_or("<non-utf8>");
        assert!(
            s.starts_with("HTTP/1.1 404 Not Found\r\n"),
            "expected 404 at exact cap, got: {s:?}",
        );

        tx.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    }
```

- [ ] **Step 2: Run the new test against the current (untightened) admin code to verify it fails.**

```bash
cargo test -p envoy-bin admin::tests::accepts_requests_exactly_at_cap -- --nocapture
```

Expected: **FAIL** with timeout or `assertion failed` — pre-tightening, an 8192-byte request grows `buf` past the cap on the next read iteration before the parser sees CRLF-CRLF (the `let n = stream.read(&mut scratch).await?` line reads up to 1024 bytes per iteration without bounding to `MAX_REQUEST_HEAD`). The exact failure mode depends on TCP segmentation; the test acts as the regression gate. If the test happens to pass against the pre-fix code (because the OS handed all 8192 bytes in one TCP segment and the parser saw CRLF-CRLF in one parse attempt), still proceed — the read-slice tightening is a correctness improvement either way; Step 4's `rejects_oversized_request_headers` update is the harder regression gate.

- [ ] **Step 3: Tighten the read-slice bound in `handle_one`.**

In `crates/envoy-bin/src/admin.rs`, locate `handle_one` (currently lines 156–197). The read at line 165 reads:

```rust
        let n = stream.read(&mut scratch).await?;
```

Replace with:

```rust
        let remaining = MAX_REQUEST_HEAD - buf.len();
        let n = stream.read(&mut scratch[..remaining]).await?;
```

`remaining` is always `> 0` here because the `if buf.len() >= MAX_REQUEST_HEAD` early-return at lines 160–164 fires before the read. The bounded slice ensures `buf` never exceeds `MAX_REQUEST_HEAD` by even one byte, regardless of `scratch.len()`.

- [ ] **Step 4: Update `rejects_oversized_request_headers` to write exactly `MAX_REQUEST_HEAD + 1` bytes.**

The existing test at admin.rs lines ~303–347 builds a 9038-byte request. Replace its body-construction block with a `MAX_REQUEST_HEAD + 1` payload of pure header padding (no CRLF-CRLF), so the handler keeps reading until the cap fires:

```rust
        // Build a request-head of exactly MAX_REQUEST_HEAD + 1 bytes with no
        // terminating CRLF-CRLF, so the handler keeps reading until the cap
        // fires. The pre-fix code allowed `buf` to grow up to
        // `MAX_REQUEST_HEAD + scratch.len() - 1` bytes before the bound check
        // tripped; the post-fix bounded read pins the cap at exactly
        // MAX_REQUEST_HEAD.
        let prefix = b"GET /ready HTTP/1.1\r\nHost: x\r\nX-Big: ";
        let pad_len = MAX_REQUEST_HEAD + 1 - prefix.len();
        let mut req: Vec<u8> = Vec::with_capacity(MAX_REQUEST_HEAD + 1);
        req.extend_from_slice(prefix);
        req.extend(std::iter::repeat_n(b'A', pad_len));
        assert_eq!(req.len(), MAX_REQUEST_HEAD + 1);
```

The remainder of the test (writing the request, polling the response, asserting `HTTP/1.1 431 Request Header Fields Too Large`) is unchanged.

- [ ] **Step 5: Run all admin tests.**

```bash
cargo test -p envoy-bin admin::
```

Expected: `test result: ok. 8 passed; 0 failed` (the existing 7 — `serves_ready_live`, `a404s_unknown_path`, `a404s_non_get_ready`, `rejects_oversized_request_headers` (updated), `drain_exits_within_budget`, plus the 3 unit tests for `imf_fixdate_*` and `render_response_*` — and the new `accepts_requests_exactly_at_cap`). Confirm `imf_fixdate_*` plus `render_response_*` count brings the total to 9 if the original was 8; the exact pre-existing count from the unmodified file matters less than "all green."

If you discover the pre-existing count is `n`, expect `n + 1` after this task.

- [ ] **Step 6: Run the full workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

All three: exit 0.

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-bin/src/admin.rs
git commit -m "phase 02.2: tighten admin 8 KiB read cap (phase-01 I4 close-out)"
```

Append a Task 3 PROGRESS section.

---

### Task 4: Scaffold `crates/envoy-listener/` skeleton + workspace member

**Files:**
- Create: `crates/envoy-listener/Cargo.toml`
- Create: `crates/envoy-listener/src/lib.rs` (compiling stub; populated by Task 5/6)
- Modify: `Cargo.toml` (root)

**Why now:** Tasks 5, 6, 8, 9 all depend on `envoy-listener` existing as a workspace member. This task lands the minimum that compiles cleanly so subsequent tasks don't mix scaffolding with real code (mirrors phase-02.1 Task 5's envoy-cluster scaffolding cadence). Per SPEC §3 D1.

- [ ] **Step 1: Write `crates/envoy-listener/Cargo.toml`.**

```toml
[package]
name = "envoy-listener"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_listener"
path = "src/lib.rs"

[dependencies]
envoy-config = { path = "../envoy-config" }
thiserror = "2"
tokio = { version = "1", features = ["rt", "net", "macros", "time", "sync"] }
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["rt", "rt-multi-thread", "net", "macros", "time", "sync", "io-util"] }
```

The dev-deps re-declare `tokio` to add `rt-multi-thread` and `io-util` for the unit tests' multi-thread runtime + `AsyncReadExt`/`AsyncWriteExt`. Same pattern as phase-02.1's `tcp-echo-server`.

- [ ] **Step 2: Write `crates/envoy-listener/src/lib.rs` as a compiling stub.**

```rust
#![forbid(unsafe_code)]

//! Phase 02.2 listener surface for envoy-rust. Owns TCP listener binding,
//! the accept loop, the `ConnectionHandler` trait that filters implement, and
//! a shutdown-gated graceful drain. Public surface is populated by Tasks 5 and
//! 6 of `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PLAN.md`.
//!
//! `BoxFuture` and `ConnectionHandler` are defined in-crate to avoid pulling
//! `futures` or `async-trait` (neither on the D-3.2 permitted-foundations
//! list); see SPEC §6 signposts 2 and 3.
```

(Empty — no items yet. The compiling-stub keeps the crate valid as a workspace member while Tasks 5 and 6 land the real surface.)

- [ ] **Step 3: Add `crates/envoy-listener` to the root workspace.**

Edit the root `Cargo.toml` `[workspace] members` list to insert `crates/envoy-listener` alphabetically between `crates/envoy-cluster` and `crates/envoy-config`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/envoy-bin",
    "crates/envoy-cluster",
    "crates/envoy-config",
    "crates/envoy-listener",
    "tests/differential",
    "tests/helpers/tcp-echo-server",
]
exclude = [
    "crates/envoy-config/fuzz",
]
```

- [ ] **Step 4: Verify the workspace builds cleanly.**

```bash
cargo build --workspace --all-targets
```

Expected: a `Compiling envoy-listener v0.0.0 (.../crates/envoy-listener)` line, then `Finished dev profile target(s) in …s`. No warnings, no errors.

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: exit 0, `Finished`.

```bash
cargo fmt --all -- --check
```

Expected: exit 0, no diff.

```bash
cargo test -p envoy-listener
```

Expected: `test result: ok. 0 passed; 0 failed` (no tests yet).

- [ ] **Step 5: Commit.**

```bash
git add Cargo.toml crates/envoy-listener
git commit -m "phase 02.2: scaffold envoy-listener crate"
```

Append a Task 4 PROGRESS section. Do NOT stage `Cargo.lock` here — workspace-member additions update `Cargo.lock`, and the convention from phases 01 and 02.1 is a dedicated lockfile-sync commit before the state-6 phase-done commit (precedents: `4955252`, `dea4d16`).

---

### Task 5: `envoy-listener::Listener::bind` + `ConnectionHandler` trait + `BoxFuture` + `ListenerError` + 2 tests

**Files:**
- Modify: `crates/envoy-listener/src/lib.rs`

**Scope:** the public types and `Listener::bind` constructor. `Listener::serve` is stubbed with `unimplemented!()` (Task 6 lands the real body); we ship two tests that prove the bind side: `bind_returns_socket_address` and `bind_fails_cleanly_on_address_in_use`. Per SPEC §3 D1 (public surface) and §6 signposts 1, 2, 3.

**Plan-time deviation from SPEC §D1's `ListenerError`.** The SPEC lists 3 variants (`Bind`, `Accept`, `DrainTimeout`). The plan adds a 4th: `AddressParse(String, u16)`, fired when `cfg.address.socket_address` is not a parseable `SocketAddr`. The SPEC's `Bind { addr: SocketAddr, ... }` variant requires a pre-resolved `SocketAddr`, which we cannot construct on parse failure. Phase-02.1 envoy-config does not pre-parse listener addresses into `SocketAddr` (the `SocketAddress` struct holds `address: String, port_value: u16`), so `Listener::bind` is the first layer to surface a malformed address. The new variant mirrors the phase-02.1 `envoy-cluster::ClusterError::EndpointParse` precedent (cluster.rs:86–92), which solved the equivalent problem on the cluster side. Log this as plan drift in Task 5's PROGRESS entry; no ADR required (mechanical surface extension, not a doctrine delta). If the reviewer at state 5 prefers SPEC verbatim, a follow-up that has `envoy-bin::run` pre-parse and pass a typed `SocketAddr` to a renamed `Listener::bind_socket(addr, handler)` would dissolve the variant — defer to REVIEW.md.

- [ ] **Step 1: Write the failing test `bind_returns_socket_address`.**

Append to `crates/envoy-listener/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::Arc;

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
        let listener = Listener::bind(&cfg, handler).await.expect("bind ok");
        let local = listener.local_addr().expect("local_addr");
        assert!(local.port() > 0, "ephemeral port must be assigned: {local}");
        assert_eq!(local.ip(), "127.0.0.1".parse::<std::net::IpAddr>().unwrap());
    }
}
```

The test will not compile until Step 2 lands `Listener`, `ConnectionHandler`, `BoxFuture`, and the `bind`/`local_addr` methods. We're using `serde_yaml` to hand-build the `envoy_config::Listener` in the test; that requires `serde_yaml` as a dev-dep. The `envoy-listener` crate does not need it as a runtime dep.

- [ ] **Step 2: Add `serde_yaml` to dev-dependencies.**

Edit `crates/envoy-listener/Cargo.toml` `[dev-dependencies]`:

```toml
[dev-dependencies]
serde_yaml = "0.9"
tokio = { version = "1", features = ["rt", "rt-multi-thread", "net", "macros", "time", "sync", "io-util"] }
```

- [ ] **Step 3: Write `BoxFuture`, `ConnectionHandler`, `Listener`, `ListenerError`, `Listener::bind`, `Listener::local_addr`. Stub `serve` with `unimplemented!()`.**

Replace the stub doc-only `crates/envoy-listener/src/lib.rs` with:

```rust
#![forbid(unsafe_code)]

//! Phase 02.2 listener surface for envoy-rust. Owns TCP listener binding, the
//! accept loop, the `ConnectionHandler` trait that filters implement, and a
//! shutdown-gated graceful drain.
//!
//! `BoxFuture` and `ConnectionHandler` are defined in-crate to avoid pulling
//! `futures` or `async-trait` (neither on the D-3.2 permitted-foundations
//! list); see SPEC §6 signposts 2 and 3.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// In-crate `BoxFuture` alias. Phase 02.2 deliberately avoids depending on
/// `futures::future::BoxFuture` because `futures` is not on the D-3.2
/// permitted-foundations list. If a later phase brings `futures` in under its
/// own ADR, this alias becomes a re-export.
pub type BoxFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

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
}

/// A bound TCP listener with a per-connection handler. Construct via
/// `Listener::bind`; drive via `Listener::serve` (Task 6).
pub struct Listener {
    listener: tokio::net::TcpListener,
    handler: Arc<dyn ConnectionHandler>,
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
    ) -> Result<Self, ListenerError> {
        let sock = &cfg.address.socket_address;
        let addr_str = format!("{}:{}", sock.address, sock.port_value);
        let addr: SocketAddr = addr_str
            .parse()
            .map_err(|_| ListenerError::AddressParse(sock.address.clone(), sock.port_value))?;
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|source| ListenerError::Bind { addr, source })?;
        Ok(Self { listener, handler })
    }

    /// Returns the actual bound socket address (resolves `port_value: 0` to
    /// the kernel-assigned ephemeral port).
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Accept loop with shutdown-gated graceful drain. Lands in Task 6.
    pub async fn serve(
        self,
        _shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), ListenerError> {
        // PLAN Task 6: replace this stub with the JoinSet-based accept loop.
        let _ = self.listener;
        let _ = self.handler;
        unimplemented!("envoy_listener::Listener::serve lands in PLAN Task 6")
    }
}
```

- [ ] **Step 4: Run the test — expect it to pass.**

```bash
cargo test -p envoy-listener bind_returns_socket_address
```

Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 5: Write the `bind_fails_cleanly_on_address_in_use` test.**

Append to the `tests` module in `crates/envoy-listener/src/lib.rs`:

```rust
    #[tokio::test]
    async fn bind_fails_cleanly_on_address_in_use() {
        // Bind once to an ephemeral port to capture the assigned port, then
        // bind again to that same port to provoke EADDRINUSE.
        let cfg_first = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(NullHandler);
        let first = Listener::bind(&cfg_first, h.clone()).await.expect("first bind ok");
        let port = first.local_addr().expect("local_addr").port();

        let cfg_second = mk_listener_cfg("127.0.0.1", port);
        let err = Listener::bind(&cfg_second, h)
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
```

- [ ] **Step 6: Run all envoy-listener tests.**

```bash
cargo test -p envoy-listener
```

Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 7: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

All three: exit 0. Note that `serve` is `unimplemented!()` so any caller would `panic!` — but no caller exists yet (Task 9 is the first); clippy with `-D warnings` does not flag `unimplemented!` macros.

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-listener/Cargo.toml crates/envoy-listener/src/lib.rs
git commit -m "phase 02.2: envoy-listener — Listener::bind + ConnectionHandler trait"
```

Append a Task 5 PROGRESS section.

---

### Task 6: `envoy-listener::Listener::serve` + 5s drain budget + abort-stragglers + 4 tests

**Files:**
- Modify: `crates/envoy-listener/src/lib.rs`

**Scope:** the real `Listener::serve` body — `tokio::select!` between accept, shutdown, and `JoinSet::join_next`; on shutdown, drain in-flight connections within 5s, then `abort_all` and return `DrainTimeout`. Four tests: `serves_accepts_and_dispatches_to_handler`, `serves_honors_shutdown_signal`, `serves_drains_in_flight_connection_within_budget`, `serves_aborts_stragglers_past_drain_budget`. Per SPEC §3 D1 and §6 signpost 5.

- [ ] **Step 1: Write the failing test `serves_accepts_and_dispatches_to_handler`.**

Append to the `tests` module in `crates/envoy-listener/src/lib.rs`:

```rust
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

    #[tokio::test(flavor = "multi_thread")]
    async fn serves_accepts_and_dispatches_to_handler() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
        let listener = Listener::bind(&cfg, h).await.expect("bind ok");
        let addr = listener.local_addr().expect("local_addr");

        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            listener
                .serve(async move {
                    let _ = rx.await;
                })
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
```

Test will compile but fail at runtime because `serve` is `unimplemented!()`.

- [ ] **Step 2: Run the test — expect it to fail (panics on `unimplemented!`).**

```bash
cargo test -p envoy-listener serves_accepts_and_dispatches_to_handler
```

Expected: **FAIL** — panic at `unimplemented!("envoy_listener::Listener::serve lands in PLAN Task 6")`.

- [ ] **Step 3: Implement `Listener::serve`.**

Replace the stub `serve` body in `crates/envoy-listener/src/lib.rs` with:

```rust
    /// Accept loop with shutdown-gated graceful drain. On `shutdown`, stop
    /// accepting and wait up to `DRAIN_BUDGET = 5s` for in-flight connections
    /// to complete. If the drain budget expires, abort stragglers and return
    /// `ListenerError::DrainTimeout`.
    ///
    /// SPEC §6 signpost 5: errors from individual `handle` calls are logged
    /// at `warn!` and dropped; the listener stays up. Asymmetric errors in
    /// `tokio::io::copy` (downstream → upstream succeeds while the other
    /// direction errors) propagate via `try_join!` inside the handler, not
    /// through the listener's accept loop.
    pub async fn serve(
        self,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), ListenerError> {
        const DRAIN_BUDGET: Duration = Duration::from_secs(5);

        let listener = self.listener;
        let handler = self.handler;
        let mut join_set: tokio::task::JoinSet<
            Result<(), Box<dyn std::error::Error + Send + Sync>>,
        > = tokio::task::JoinSet::new();
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                _ = &mut shutdown => {
                    tracing::info!("listener shutdown signal received; draining");
                    drop(listener);
                    break;
                }
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, peer)) => {
                            tracing::debug!(%peer, "listener accepted connection");
                            let h = handler.clone();
                            join_set.spawn(async move { h.handle(stream).await });
                        }
                        Err(err) => {
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
        }

        // Drain.
        let drain = async {
            while let Some(res) = join_set.join_next().await {
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => tracing::warn!(error = %err, "connection task failed during drain"),
                    Err(join_err) => tracing::warn!(error = %join_err, "connection task panicked during drain"),
                }
            }
        };
        if tokio::time::timeout(DRAIN_BUDGET, drain).await.is_err() {
            tracing::warn!(?DRAIN_BUDGET, "drain budget exceeded; aborting stragglers");
            join_set.abort_all();
            // Let aborted tasks unwind; ignore their results.
            while join_set.join_next().await.is_some() {}
            return Err(ListenerError::DrainTimeout(DRAIN_BUDGET));
        }
        Ok(())
    }
```

- [ ] **Step 4: Run the test — expect it to pass.**

```bash
cargo test -p envoy-listener serves_accepts_and_dispatches_to_handler
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 5: Write the remaining three serve-side tests.**

Append to the `tests` module:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn serves_honors_shutdown_signal() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
        let listener = Listener::bind(&cfg, h).await.expect("bind");
        let (tx, rx) = oneshot::channel::<()>();
        let start = std::time::Instant::now();
        let server = tokio::spawn(async move {
            listener.serve(async move { let _ = rx.await; }).await.expect("serve")
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
            "serve took too long: {:?}", start.elapsed(),
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn serves_drains_in_flight_connection_within_budget() {
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let h: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
        let listener = Listener::bind(&cfg, h).await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            listener.serve(async move { let _ = rx.await; }).await.expect("serve")
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
            "drain too slow: {:?}", start.elapsed(),
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
        let listener = Listener::bind(&cfg, h).await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            listener.serve(async move { let _ = rx.await; }).await
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
            "abort fired too early: {:?}", start.elapsed(),
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(7),
            "abort fired too late: {:?}", start.elapsed(),
        );
    }
```

- [ ] **Step 6: Run all envoy-listener tests.**

```bash
cargo test -p envoy-listener
```

Expected: `test result: ok. 6 passed; 0 failed` (the 2 from Task 5 + 4 new).

- [ ] **Step 7: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

All three: exit 0.

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-listener/src/lib.rs
git commit -m "phase 02.2: envoy-listener — serve + drain + 4 tests"
```

Append a Task 6 PROGRESS section.

---

### Task 7: Scaffold `crates/envoy-tcp/` skeleton + workspace member

**Files:**
- Create: `crates/envoy-tcp/Cargo.toml`
- Create: `crates/envoy-tcp/src/lib.rs` (compiling stub)
- Modify: `Cargo.toml` (root)

**Why now:** Tasks 8 and 9 depend on `envoy-tcp` existing as a workspace member. Per SPEC §3 D2.

- [ ] **Step 1: Write `crates/envoy-tcp/Cargo.toml`.**

```toml
[package]
name = "envoy-tcp"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_tcp"
path = "src/lib.rs"

[dependencies]
envoy-cluster = { path = "../envoy-cluster" }
envoy-config = { path = "../envoy-config" }
envoy-listener = { path = "../envoy-listener" }
thiserror = "2"
tokio = { version = "1", features = ["rt", "net", "io-util", "macros"] }
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["rt", "rt-multi-thread", "net", "io-util", "macros", "time", "sync"] }
```

- [ ] **Step 2: Write `crates/envoy-tcp/src/lib.rs` as a compiling stub.**

```rust
#![forbid(unsafe_code)]

//! Phase 02.2 TCP proxy filter for envoy-rust. Implements
//! `envoy_listener::ConnectionHandler` for `TcpProxy`. Public surface is
//! populated by Task 8 of `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PLAN.md`.
//!
//! Half-close posture follows ADR-0016 (Envoy v1.33.0 default
//! `enable_half_close: false`): `tokio::io::copy` runs in both directions
//! and EOF on either side propagates via drop of the write half.
```

- [ ] **Step 3: Add `crates/envoy-tcp` to the root workspace.**

Edit root `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/envoy-bin",
    "crates/envoy-cluster",
    "crates/envoy-config",
    "crates/envoy-listener",
    "crates/envoy-tcp",
    "tests/differential",
    "tests/helpers/tcp-echo-server",
]
exclude = [
    "crates/envoy-config/fuzz",
]
```

- [ ] **Step 4: Verify build.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test -p envoy-tcp
```

All four: exit 0; `envoy-tcp` contributes 0 tests.

- [ ] **Step 5: Commit.**

```bash
git add Cargo.toml crates/envoy-tcp
git commit -m "phase 02.2: scaffold envoy-tcp crate"
```

Append a Task 7 PROGRESS section.

---

### Task 8: `envoy-tcp::TcpProxy` + `ConnectionHandler` impl + `TcpProxyError` + 4 tests

**Files:**
- Modify: `crates/envoy-tcp/src/lib.rs`

**Scope:** the public `TcpProxy` struct, `TcpProxyError` enum, `ConnectionHandler` impl with `handle`, and four tests:
- `proxies_payload_end_to_end` — round-trip a payload through `TcpProxy → in-process echo`.
- `proxies_closes_downstream_on_upstream_close` — upstream half-closes; downstream sees the echoed bytes + EOF.
- `proxies_closes_upstream_on_downstream_close` — downstream drops; upstream gets FIN and closes cleanly.
- `proxies_returns_err_on_upstream_connect_refused` — cluster points at `127.0.0.1:1`; assert `Err` wraps `TcpProxyError::UpstreamConnect`.

Per SPEC §3 D2 and §6 signposts 1, 2, 3, 5, 6.

**Plan-time deviation from SPEC §D2 step 4 (try_join → select).** SPEC §D2 step 4 prescribes `tokio::try_join!(d2u, u2d)`. Strict `tokio::try_join!` waits for *both* copies to complete (or for one to error); a clean EOF on one side parks the other side until it also closes. That semantic contradicts both ADR-0016 ("`enable_half_close: false` — close one direction → close both") and SPEC §D2's third test (`proxies_closes_upstream_on_downstream_close`: downstream FIN must propagate to upstream as FIN, but with strict `try_join!` the proxy waits for upstream to FIN first — chicken-and-egg). The plan resolves this by using `tokio::select!` over the two copy futures: whichever completes first (Ok or Err) wins; the other future is dropped, dropping its borrowed write half, which closes the corresponding TCP write side and propagates FIN. This matches Envoy v1.33.0's `enable_half_close: false` posture and SPEC §6 signpost 5's "asymmetric error → both sides close" property. Log this drift in Task 8's PROGRESS entry. If the state-5 reviewer prefers SPEC verbatim, the resolution is to land an ADR (likely ADR-0017 if no other ADR has interleaved) under D-3.5; the SPEC stays unedited per D-3.4. The `tokio::select!` variant remains a strict subset of the SPEC-prescribed test list — all four tests pass.

- [ ] **Step 1: Write the failing test `proxies_payload_end_to_end`.**

Append to `crates/envoy-tcp/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    /// Spawn an in-process echo server on an ephemeral port. Returns the
    /// bound address. The server task echoes a single connection's bytes
    /// back via `tokio::io::copy` and then closes.
    async fn spawn_echo() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let (mut r, mut w) = stream.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            }
        });
        addr
    }

    /// Build a single-endpoint `ClusterHandle` pointing at `addr`. Bypasses
    /// the YAML path so tests don't need a full `Bootstrap` for an ephemeral
    /// port.
    fn mk_handle(name: &str, addr: SocketAddr) -> envoy_cluster::ClusterHandle {
        // Use the YAML path so we go through `parse_bootstrap` + `from_bootstrap`,
        // mirroring how `envoy-bin` will build the manager in Task 9. This also
        // exercises the integration boundary `envoy-tcp` cares about.
        let yaml = format!(
            r#"
static_resources:
  listeners: []
  clusters:
    - name: {name}
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: {name}
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {ip}
                      port_value: {port}
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#,
            ip = addr.ip(),
            port = addr.port(),
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("valid YAML");
        let mgr = envoy_cluster::from_bootstrap(&bootstrap).expect("manager builds");
        mgr.get(name).expect("cluster present")
    }

    fn mk_cfg(cluster_name: &str) -> envoy_config::TcpProxyConfig {
        envoy_config::TcpProxyConfig {
            stat_prefix: "ingress_tcp".to_string(),
            cluster: cluster_name.to_string(),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_payload_end_to_end() {
        let upstream_addr = spawn_echo().await;
        let handle = mk_handle("backend", upstream_addr);
        let proxy = TcpProxy::new(handle, &mk_cfg("backend"));

        // We need a downstream `TcpStream` to feed `handle`. Spawn a listener
        // representing the downstream side, accept one connection, hand it to
        // `proxy.handle`. The "client" side connects to that listener, writes
        // the payload, reads the echoed response.
        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_arc: Arc<TcpProxy> = Arc::new(proxy);
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            envoy_listener::ConnectionHandler::handle(&*proxy_arc, stream)
                .await
                .expect("handle ok")
        });

        let mut client = TcpStream::connect(downstream_addr).await.expect("connect");
        let payload = b"end-to-end through tcp_proxy";
        client.write_all(payload).await.expect("write");
        let mut buf = vec![0u8; payload.len()];
        client.read_exact(&mut buf).await.expect("read_exact");
        assert_eq!(buf, payload);
        // Half-close the client to let the proxy's upstream copy task see EOF.
        client.shutdown().await.ok();
        drop(client);

        proxy_task.await.expect("proxy task joins");
        // Suppress unused-import warning for AtomicUsize when later tests stop
        // referring to it.
        let _ = AtomicUsize::new(0);
    }
}
```

The test references `TcpProxy::new`, `TcpProxy` (with `ConnectionHandler` impl), `envoy_listener`. `envoy_listener` is already a dep. Add `serde_yaml` to dev-deps for `parse_bootstrap`'s YAML — but `parse_bootstrap` already takes `&str`, so no extra dep is needed. Actually `envoy-config` is already a regular dep and re-exports `parse_bootstrap`, so the test compiles via `envoy_config::parse_bootstrap`.

- [ ] **Step 2: Run — expect compile failure (`TcpProxy` not yet defined).**

```bash
cargo test -p envoy-tcp proxies_payload_end_to_end
```

Expected: **FAIL** with `cannot find type/struct `TcpProxy``.

- [ ] **Step 3: Implement `TcpProxy`, `TcpProxyError`, and the `ConnectionHandler` impl.**

Replace the doc-only stub `crates/envoy-tcp/src/lib.rs` with:

```rust
#![forbid(unsafe_code)]

//! Phase 02.2 TCP proxy filter for envoy-rust. Implements
//! `envoy_listener::ConnectionHandler` for `TcpProxy`. Half-close posture
//! follows ADR-0016 (Envoy v1.33.0 default `enable_half_close: false`):
//! `tokio::io::copy` runs in both directions and EOF on either side
//! propagates via drop of the write half.

use std::net::SocketAddr;

use envoy_listener::{BoxFuture, ConnectionHandler};

/// Per-connection TCP proxy. Holds a cloneable `ClusterHandle` to the
/// upstream cluster and the cluster's name (carried separately for
/// diagnostics — `envoy-cluster` does not expose `Cluster::name()` in 02.1
/// per the REVIEW M1 deferral).
pub struct TcpProxy {
    cluster: envoy_cluster::ClusterHandle,
    cluster_name: String,
}

impl TcpProxy {
    pub fn new(cluster: envoy_cluster::ClusterHandle, cfg: &envoy_config::TcpProxyConfig) -> Self {
        Self {
            cluster,
            cluster_name: cfg.cluster.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TcpProxyError {
    #[error("no healthy endpoint available for cluster '{cluster}'")]
    NoHealthyEndpoint { cluster: String },
    #[error("connecting to upstream {addr}: {source}")]
    UpstreamConnect {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("bidirectional copy failed: {source}")]
    CopyFailed {
        #[source]
        source: std::io::Error,
    },
}

impl ConnectionHandler for TcpProxy {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let pick = self.cluster.pick_endpoint();
        let cluster_name = self.cluster_name.clone();
        Box::pin(async move {
            let addr = pick.ok_or_else(|| {
                Box::new(TcpProxyError::NoHealthyEndpoint {
                    cluster: cluster_name.clone(),
                }) as Box<dyn std::error::Error + Send + Sync>
            })?;

            let upstream = tokio::net::TcpStream::connect(addr)
                .await
                .map_err(|source| {
                    Box::new(TcpProxyError::UpstreamConnect { addr, source })
                        as Box<dyn std::error::Error + Send + Sync>
                })?;

            // Plan-time deviation from SPEC §D2 step 4: use `tokio::select!`
            // rather than `tokio::try_join!` so that an EOF on either side
            // immediately drops the other copy future, releasing the
            // corresponding write half and propagating FIN — matching ADR-
            // 0016's `enable_half_close: false` semantics. See PLAN Task 8
            // header for the full rationale.
            let (mut dr, mut dw) = downstream.into_split();
            let (mut ur, mut uw) = upstream.into_split();
            let result: Result<(), std::io::Error> = tokio::select! {
                res = tokio::io::copy(&mut dr, &mut uw) => res.map(|_| ()),
                res = tokio::io::copy(&mut ur, &mut dw) => res.map(|_| ()),
            };
            // Drop the read/write halves explicitly so the unused direction
            // closes promptly even before the function returns.
            drop((dr, dw, ur, uw));
            result.map_err(|source| {
                Box::new(TcpProxyError::CopyFailed { source })
                    as Box<dyn std::error::Error + Send + Sync>
            })?;

            tracing::debug!(%addr, cluster = %cluster_name, "tcp proxy connection complete");
            Ok(())
        })
    }
}
```

Note: `pick_endpoint()` is invoked synchronously *outside* the boxed future so we don't borrow `&self` across an await point. The cloned `cluster_name` and the `Option<SocketAddr>` are moved into the future.

- [ ] **Step 4: Run the test — expect it to pass.**

```bash
cargo test -p envoy-tcp proxies_payload_end_to_end
```

Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 5: Add the three remaining tests.**

Append to the `tests` module in `crates/envoy-tcp/src/lib.rs`:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_closes_downstream_on_upstream_close() {
        // Upstream "echo" server writes back the payload, then closes its
        // write side. The proxy's u2d copy returns EOF and drops `dw`,
        // closing the downstream's read side. The downstream client sees
        // the echoed bytes followed by EOF.
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let upstream_addr = upstream_listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (mut stream, _) = upstream_listener.accept().await.expect("accept");
            let mut buf = [0u8; 5];
            stream.read_exact(&mut buf).await.expect("read");
            stream.write_all(&buf).await.expect("write");
            stream.shutdown().await.ok();
            // Hold the read side open briefly so the downstream can drain.
            let mut tail = [0u8; 16];
            let _ = stream.read(&mut tail).await;
            drop(stream);
        });

        let handle = mk_handle("backend", upstream_addr);
        let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            envoy_listener::ConnectionHandler::handle(&*proxy, stream).await
        });

        let mut client = TcpStream::connect(downstream_addr).await.expect("connect");
        client.write_all(b"hello").await.expect("write");
        let mut echoed = [0u8; 5];
        client.read_exact(&mut echoed).await.expect("read_exact");
        assert_eq!(&echoed, b"hello");

        // After upstream closes, the proxy's u2d copy returns; eventual EOF
        // reaches the client.
        let mut tail = Vec::new();
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.read_to_end(&mut tail),
        )
        .await;
        assert!(tail.is_empty(), "expected EOF, got trailing bytes: {tail:?}");

        let _ = proxy_task.await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_closes_upstream_on_downstream_close() {
        // Downstream client drops without writing anything. The proxy's d2u
        // copy returns EOF on its first read; `try_join!` drops `u2d`,
        // dropping `uw` — upstream sees FIN. Upstream closes cleanly when
        // it tries to read.
        let upstream_seen_fin = Arc::new(tokio::sync::Notify::new());
        let upstream_seen_fin_signal = upstream_seen_fin.clone();
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let upstream_addr = upstream_listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (mut stream, _) = upstream_listener.accept().await.expect("accept");
            // Read until FIN; assert we observed EOF (Ok(0)).
            let mut buf = [0u8; 64];
            let n = stream.read(&mut buf).await.expect("read");
            assert_eq!(n, 0, "upstream expected to read EOF after downstream drop");
            upstream_seen_fin_signal.notify_one();
        });

        let handle = mk_handle("backend", upstream_addr);
        let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            envoy_listener::ConnectionHandler::handle(&*proxy, stream).await
        });

        let client = TcpStream::connect(downstream_addr).await.expect("connect");
        drop(client);

        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            upstream_seen_fin.notified(),
        )
        .await
        .expect("upstream observed FIN within 3s");
        let _ = proxy_task.await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_returns_err_on_upstream_connect_refused() {
        // 127.0.0.1:1 is reserved (kernel TCP RST) on every UNIX-like host;
        // upstream connect must fail loudly.
        let refused: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let handle = mk_handle("backend", refused);
        let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            envoy_listener::ConnectionHandler::handle(&*proxy, stream).await
        });

        let _client = TcpStream::connect(downstream_addr).await.expect("connect");
        let result = proxy_task.await.expect("proxy task joins");
        let err = result.expect_err("upstream connect must fail");
        let formatted = format!("{err}");
        assert!(
            formatted.contains("connecting to upstream 127.0.0.1:1"),
            "expected UpstreamConnect, got: {formatted}",
        );
    }
```

- [ ] **Step 6: Run all envoy-tcp tests.**

```bash
cargo test -p envoy-tcp
```

Expected: `test result: ok. 4 passed; 0 failed`.

- [ ] **Step 7: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

All three: exit 0.

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-tcp/src/lib.rs
git commit -m "phase 02.2: envoy-tcp — TcpProxy + ConnectionHandler impl + 4 tests"
```

Append a Task 8 PROGRESS section.

---

### Task 9: `envoy-bin` wiring — `ClusterManager` + filter dispatch + integration test

**Files:**
- Modify: `crates/envoy-bin/Cargo.toml` (add `envoy-cluster`, `envoy-listener`, `envoy-tcp` path deps)
- Modify: `crates/envoy-bin/src/main.rs` (filter dispatch in `run`)
- Create: `crates/envoy-bin/tests/tcp_proxy.rs` (Rust-native integration test, no Docker)

**Scope:** wire the new crates into `envoy-bin`. The single phase-02 listener can carry either `envoy.filters.network.echo` (phase-01 path: keep `echo::serve`) or `envoy.filters.network.tcp_proxy` (new: build a `TcpProxy`, pass it to `Listener::serve`). Construct `ClusterManager` once at startup. Per SPEC §3 D3.

- [ ] **Step 1: Write the failing integration test `crates/envoy-bin/tests/tcp_proxy.rs`.**

```rust
//! Phase 02.2 backstop: write a tcp_proxy config pointing at an in-process
//! tokio echo server, spawn `envoy-bin` as a subprocess, drive a payload
//! through the listener, assert byte-exact round-trip. Mirror of
//! `tests/admin_only.rs` from phase 01. The real differential assertion is
//! the Docker-gated `tests/differential/tests/tcp_proxy.rs` (Task 12).

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

async fn wait_ready(addr: SocketAddr, budget: Duration) {
    let deadline = std::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => return,
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(e) => panic!("listener never became ready at {addr}: {e}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn tcp_proxy_round_trips_through_envoy_bin() {
    // Spawn an in-process echo server as the upstream backend. (We do NOT
    // use the tcp-echo-server helper binary here — that's reserved for the
    // Docker-side differential harness in Task 12.)
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = backend_listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = backend_listener.accept().await else { return };
            tokio::spawn(async move {
                let (mut r, mut w) = stream.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });

    let listener_port = reserve_port();
    let yaml = format!(
        r#"
static_resources:
  listeners:
    - name: tcp_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listener_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {backend_ip}
                      port_value: {backend_port}
"#,
        backend_ip = backend_addr.ip(),
        backend_port = backend_addr.port(),
    );

    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg).unwrap().write_all(yaml.as_bytes()).unwrap();

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&cfg)
        .env("ENVOY_RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    let listener_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    wait_ready(listener_addr, Duration::from_secs(10)).await;

    let mut s = TcpStream::connect(listener_addr).await.unwrap();
    let payload = b"hello, tcp_proxy\n";
    s.write_all(payload).await.unwrap();
    let mut buf = vec![0u8; payload.len()];
    s.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, payload);
    s.shutdown().await.ok();
    drop(s);

    child.kill().await.ok();
    let _ = child.wait().await;
}
```

- [ ] **Step 2: Verify it fails to compile (or runs and fails) against the unmodified envoy-bin.**

```bash
cargo test -p envoy-bin --test tcp_proxy
```

Expected: **FAIL** — either at runtime when envoy-bin's parser accepts the config but `run()` doesn't dispatch on `tcp_proxy` (the listener never starts; `wait_ready` panics), or earlier if the parser rejects the config. Either way the test does not pass.

- [ ] **Step 3: Add path deps to `crates/envoy-bin/Cargo.toml`.**

```toml
[dependencies]
anyhow = "1"
envoy-cluster = { path = "../envoy-cluster" }
envoy-config = { path = "../envoy-config" }
envoy-listener = { path = "../envoy-listener" }
envoy-tcp = { path = "../envoy-tcp" }
httparse = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "signal", "time", "sync", "process"] }
tokio-util = { version = "0.7", features = ["default"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
```

- [ ] **Step 4: Modify `crates/envoy-bin/src/main.rs::run` to dispatch the listener filter.**

Replace the current listener-setup block (lines 70–81 of the existing `main.rs`) with:

```rust
    // Build the cluster manager once. Empty `clusters` is permitted at the
    // envoy-config validator (admin-only configs); the manager is empty in
    // that case and `tcp_proxy` filters reference clusters by name, which
    // the validator already verified exist (`ConfigError::UnknownCluster`).
    let cluster_mgr = std::sync::Arc::new(
        envoy_cluster::from_bootstrap(&bootstrap).context("building cluster manager")?,
    );

    if let Some(listener_cfg) = bootstrap.static_resources.listeners.first() {
        // The validator guarantees `filter_chains.len() ≥ 1` and at least one
        // filter; we read the single first filter (phase 02.2 supports one
        // filter per chain). Phase 07's filter chain framework will iterate.
        let filter = &listener_cfg
            .filter_chains
            .first()
            .and_then(|c| c.filters.first())
            .expect("validator guarantees ≥1 filter");

        let sock = &listener_cfg.address.socket_address;
        let bind_addr: SocketAddr = format!("{}:{}", sock.address, sock.port_value)
            .parse()
            .with_context(|| format!("parsing listener address {}:{}", sock.address, sock.port_value))?;

        match filter.name.as_str() {
            envoy_config::ECHO_FILTER => {
                let lst = TcpListener::bind(bind_addr)
                    .await
                    .with_context(|| format!("binding echo listener to {bind_addr}"))?;
                tracing::info!(addr = %bind_addr, "envoy-rust listening (echo)");
                let shutdown = token.clone();
                set.spawn(async move {
                    echo::serve(lst, async move { shutdown.cancelled().await }).await
                });
            }
            envoy_config::TCP_PROXY_FILTER => {
                // Validator already enforced that typed_config is the
                // TcpProxy variant and that the cluster exists. Use a
                // `let-else` rather than `.expect()` so a future validator
                // drift surfaces a typed bail() rather than a panic.
                // (Single-variant enum patterns are still refutable in
                // Rust, so a plain `let` binding will not compile here.)
                let Some(envoy_config::TypedConfig::TcpProxy(tp_cfg)) =
                    filter.typed_config.as_ref()
                else {
                    anyhow::bail!(
                        "filter '{}' missing typed_config; envoy-config validator should have rejected at parse time",
                        envoy_config::TCP_PROXY_FILTER,
                    );
                };
                let cluster = cluster_mgr
                    .get(&tp_cfg.cluster)
                    .expect("validator guarantees cluster present");
                let proxy = std::sync::Arc::new(envoy_tcp::TcpProxy::new(cluster, tp_cfg));
                let listener = envoy_listener::Listener::bind(listener_cfg, proxy)
                    .await
                    .with_context(|| format!("binding tcp_proxy listener to {bind_addr}"))?;
                tracing::info!(addr = %bind_addr, cluster = %tp_cfg.cluster, "envoy-rust listening (tcp_proxy)");
                let shutdown = token.clone();
                set.spawn(async move {
                    listener
                        .serve(async move { shutdown.cancelled().await })
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                });
            }
            other => {
                anyhow::bail!(
                    "filter '{other}' is not dispatchable; envoy-config should have rejected at parse time"
                );
            }
        }
    }
```

The `as_ref()` + `expect("validator guarantees …")` lines are intentional. The validator owns these invariants; if it ever drifts, `envoy-bin` fails fast at startup rather than silently misbehaving.

- [ ] **Step 5: Run the integration test — expect it to pass.**

```bash
cargo test -p envoy-bin --test tcp_proxy -- --nocapture
```

Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 6: Run the full envoy-bin test suite.**

```bash
cargo test -p envoy-bin
```

Expected: all admin tests + echo tests + the two integration tests (`admin_only.rs` and the new `tcp_proxy.rs`) green. Specifically `accepts_requests_exactly_at_cap` from Task 3 still passes.

- [ ] **Step 7: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo deny check
```

All four: exit 0. `cargo deny check` flags any new transitive license/ban surface from the three new path deps; the expectation is no flags (the path deps' graphs are subsets of `envoy-bin`'s already-vetted graph), but if a flag fires, land `deny.toml` updates under a new ADR mid-execution per D-3.5 (likely ADR-0017 if it trips, per SPEC §3 D8 contingency note).

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-bin/Cargo.toml crates/envoy-bin/src/main.rs crates/envoy-bin/tests/tcp_proxy.rs
git commit -m "phase 02.2: envoy-bin — wire ClusterManager + tcp_proxy dispatch"
```

Append a Task 9 PROGRESS section.

---

### Task 10: `tests/differential/src/backend.rs` — `TcpProxyBackend` helper + 2 unit tests

**Files:**
- Create: `tests/differential/src/backend.rs`
- Modify: `tests/differential/src/lib.rs` (add `pub mod backend;`)

**Scope:** the new harness module that locates and runs the workspace-built `tcp-echo-server` binary as a host subprocess on a reserved port, polls accept-readiness, and exposes a `Drop` that SIGKILLs the child. Two unit tests: `tcp_proxy_backend_spawns_and_echoes` (round-trip a payload) and `tcp_proxy_backend_drop_terminates_child` (drop, assert exit). Per SPEC §3 D4 and §6 signpost 8.

- [ ] **Step 1: Write `tests/differential/src/backend.rs` with the failing test first, then the helper.**

Following TDD: failing test in the same file as the to-be-written helper, both landing in one commit since they're tightly coupled.

```rust
//! `TcpProxyBackend` — spawns the workspace's `tcp-echo-server` binary as a
//! host subprocess on a reserved 127.0.0.1 port, used by fixture 0003-tcp-proxy
//! as the upstream backend that both proxies dial. See SPEC §3 D4 and SPEC
//! §6 signpost 8: cross-package `CARGO_BIN_EXE_*` is unavailable, so we
//! compute the path as `<workspace>/target/<profile>/tcp-echo-server`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::{reserve_port, wait_accept_ready};

/// A running `tcp-echo-server` host subprocess. Drop sends SIGKILL via
/// tokio's `start_kill` and waits up to 2s for the child to exit (matches
/// `tests/differential/src/subject.rs`'s SIGKILL posture per phase-01 M1's
/// open-ended `nix`-deferral).
pub struct TcpProxyBackend {
    port: u16,
    child: Option<tokio::process::Child>,
}

impl TcpProxyBackend {
    /// Reserve an ephemeral 127.0.0.1 port, locate the workspace's
    /// `tcp-echo-server` binary, spawn it with `--port <port>`, and wait
    /// until the listener accepts a TCP connection. Total readiness budget:
    /// 1s (matches `wait_accept_ready`'s exponential backoff defaults; see
    /// SPEC §6 signpost 8).
    pub async fn spawn() -> Result<Self> {
        let port = reserve_port().context("reserving backend port")?;
        let bin = locate_tcp_echo_server().context("locating tcp-echo-server binary")?;
        let child = tokio::process::Command::new(&bin)
            .arg("--port")
            .arg(port.to_string())
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning {} --port {port}", bin.display()))?;

        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
        wait_accept_ready(addr, Duration::from_secs(1))
            .await
            .with_context(|| format!("tcp-echo-server never became accept-ready on {addr}"))?;

        Ok(Self { port, child: Some(child) })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Hostname the upstream Envoy container uses to reach this backend.
    /// See ADR-0015. Always `host.docker.internal`; envoy-rust on the host
    /// reaches the same backend at `127.0.0.1`.
    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for TcpProxyBackend {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // SIGKILL via tokio's start_kill. Same posture as
            // tests/differential/src/subject.rs.
            let _ = child.start_kill();
            // Best-effort exit wait. Using `try_wait` in a 2s polling loop
            // because `Drop` cannot await; the spawned task pattern would
            // require a runtime handle we don't have here.
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                    Err(_) => return,
                }
            }
        }
    }
}

/// Locate the workspace's `tcp-echo-server` binary. Cargo's
/// `CARGO_BIN_EXE_<name>` is only set for tests in the same package as the
/// binary; we're in the cross-package `differential` crate, so we compute
/// the path by convention. See SPEC §6 signpost 8.
fn locate_tcp_echo_server() -> Result<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    // tests/differential → repo root is two parents up.
    let workspace_root = manifest
        .parent()
        .and_then(|p| p.parent())
        .context("walking up from CARGO_MANIFEST_DIR to workspace root")?;
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target"));
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    let mut bin = target_dir.join(profile).join("tcp-echo-server");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    if !bin.exists() {
        bail!(
            "tcp-echo-server not found at {}; run `cargo build -p tcp-echo-server` or `cargo test --workspace`",
            bin.display(),
        );
    }
    Ok(bin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    #[tokio::test(flavor = "multi_thread")]
    async fn tcp_proxy_backend_spawns_and_echoes() {
        // Skip if the helper binary isn't built — running
        // `cargo test -p differential` in isolation can hit this; the
        // workspace gate (`cargo test --workspace`) builds all binaries.
        if locate_tcp_echo_server().is_err() {
            eprintln!("skipping: tcp-echo-server not built");
            return;
        }
        let backend = TcpProxyBackend::spawn().await.expect("spawn ok");
        let port = backend.port();
        assert!(port > 0);
        assert_eq!(backend.container_host(), "host.docker.internal");

        let mut s = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        let payload = b"backend round-trip";
        s.write_all(payload).await.expect("write");
        let mut buf = vec![0u8; payload.len()];
        s.read_exact(&mut buf).await.expect("read");
        assert_eq!(buf, payload);
        drop(s);
        drop(backend);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tcp_proxy_backend_drop_terminates_child() {
        if locate_tcp_echo_server().is_err() {
            eprintln!("skipping: tcp-echo-server not built");
            return;
        }
        let backend = TcpProxyBackend::spawn().await.expect("spawn ok");
        let port = backend.port();
        // Sanity: the listener is up.
        let s = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect pre-drop");
        drop(s);

        // Drop the backend; the subprocess should exit. After drop, a fresh
        // connect attempt should fail (no listener).
        drop(backend);

        // Allow up to 3s for the child to exit + the kernel to release the port.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            match TcpStream::connect(("127.0.0.1", port)).await {
                Ok(_) if std::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Ok(_) => panic!(
                    "tcp-echo-server still listening on {port} 3s after Drop",
                ),
                Err(_) => return,
            }
        }
    }
}
```

- [ ] **Step 2: Add the module declaration to `tests/differential/src/lib.rs`.**

Edit `tests/differential/src/lib.rs` to add `pub mod backend;` next to the existing module declarations (after `pub mod subject;` and `pub mod upstream;`):

```rust
pub mod backend;
pub mod subject;
pub mod upstream;
```

- [ ] **Step 3: Run the new tests.**

```bash
cargo build -p tcp-echo-server  # ensure helper binary exists locally
cargo test -p differential backend::tests
```

Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 4: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

All three: exit 0.

- [ ] **Step 5: Commit.**

```bash
git add tests/differential/src/backend.rs tests/differential/src/lib.rs
git commit -m "phase 02.2: differential — TcpProxyBackend helper + 2 tests"
```

Append a Task 10 PROGRESS section.

---

### Task 11: Differential harness — `render_yaml` backend keys + `run_fixture` dispatch + upstream `with_host` + 3 unit tests + drop M3 disjunct

**Files:**
- Modify: `tests/differential/src/lib.rs` (`render_yaml`, `run_fixture`, drop M3 disjunct, 3 new unit tests)
- Modify: `tests/differential/src/upstream.rs` (extend `start` to call `with_host` when fixture uses a backend)

**Scope:** plug in the `TcpProxyBackend` from Task 10. Two-key extension to `render_yaml`'s effective contract: when a template references `{{BACKEND_PORT}}` it also needs `{{BACKEND_HOST}}`, substituted to `host.docker.internal` on the upstream-Envoy side and `127.0.0.1` on the envoy-rust side. `run_fixture` detects `{{BACKEND_PORT}}` in either rendered template and spawns a `TcpProxyBackend` whose port goes into both substitution maps. Upstream container start signals "needs host-gateway" by inspecting the rendered upstream YAML for `host.docker.internal`. Per SPEC §3 D4 and §6 signposts 4, 7, 11.

- [ ] **Step 1: Drop the dead `|| msg.contains("CRLF")` disjunct (REVIEW.md M3).**

Edit `tests/differential/src/lib.rs` lines 788–791:

```rust
        let err = super::decode_chunked(b"5hello").expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("missing CRLF"),
            "expected CRLF-missing error; got {msg}",
        );
```

(Drops the `|| msg.contains("CRLF")` second disjunct — `decode_chunked`'s only CRLF-related error path emits literally "missing CRLF", and the dead disjunct is a foot-gun per phase-02.1 REVIEW M3.)

- [ ] **Step 2: Verify the dead-disjunct tightening still passes.**

```bash
cargo test -p differential decode_chunked_truncated_size_line
```

Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 3: Write the failing test `render_yaml_substitutes_backend_keys_for_envoy_side`.**

Append to `tests/differential/src/lib.rs::tests`:

```rust
    #[test]
    fn render_yaml_substitutes_backend_keys_for_envoy_side() {
        // Upstream-Envoy rendering: {{BACKEND_HOST}} → host.docker.internal,
        // {{BACKEND_PORT}} → harness-reserved port. {{PORT}} → the listener port.
        let template = r#"
listeners: [{{PORT}}]
endpoint: {{BACKEND_HOST}}:{{BACKEND_PORT}}
"#;
        let got = render_yaml(
            template,
            &[
                ("PORT", "10000"),
                ("BACKEND_HOST", "host.docker.internal"),
                ("BACKEND_PORT", "31415"),
            ],
        );
        assert!(got.contains("listeners: [10000]"), "PORT not substituted: {got}");
        assert!(
            got.contains("endpoint: host.docker.internal:31415"),
            "BACKEND_{{HOST,PORT}} not substituted: {got}",
        );
    }

    #[test]
    fn render_yaml_substitutes_backend_keys_for_envoy_rust_side() {
        // envoy-rust-side rendering: {{BACKEND_HOST}} → 127.0.0.1.
        let template = r#"
listeners: [{{PORT}}]
endpoint: {{BACKEND_HOST}}:{{BACKEND_PORT}}
"#;
        let got = render_yaml(
            template,
            &[
                ("PORT", "20000"),
                ("BACKEND_HOST", "127.0.0.1"),
                ("BACKEND_PORT", "31415"),
            ],
        );
        assert!(got.contains("listeners: [20000]"), "PORT not substituted: {got}");
        assert!(
            got.contains("endpoint: 127.0.0.1:31415"),
            "BACKEND_HOST not substituted to 127.0.0.1: {got}",
        );
    }
```

`render_yaml` already accepts arbitrary `&[(&str, &str)]` kvs lists, so these tests pass on the existing implementation. They land here as forward-regression guards on the substitution map shape (Task 12 derives the actual fixture YAMLs from this contract).

- [ ] **Step 4: Run the new render_yaml tests — expect them to pass.**

```bash
cargo test -p differential render_yaml_substitutes_backend_keys_for_envoy_side render_yaml_substitutes_backend_keys_for_envoy_rust_side
```

Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 5: Write the failing test `fixture_0003_expectations_parses_as_tcp_echo`.**

The fixture's `expectations.yaml` doesn't exist yet (Task 12 creates it), so this test is for forward-regression. Mirror the `fixture_0001_expectations_parses_as_tcp_echo` shape from `tests/differential/src/lib.rs:802–811`:

Append to `tests/differential/src/lib.rs::tests`:

```rust
    #[test]
    fn fixture_0003_expectations_parses_as_tcp_echo() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests/fixtures/0003-tcp-proxy/expectations.yaml");
        let e = load_expectations(&path).expect("parses");
        assert!(matches!(e.driver, Driver::TcpEcho));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert_eq!(e.equivalence.response_status, None);
    }
```

- [ ] **Step 6: Run — expect failure (file not yet present).**

```bash
cargo test -p differential fixture_0003_expectations_parses_as_tcp_echo
```

Expected: **FAIL** with "reading … expectations.yaml" (file not found). The test will pass once Task 12 lands the fixture; we leave this test in place now as a forward gate.

To keep the workspace gate green between Task 11 and Task 12, follow either of these conventions:
1. **Skip-shaped wrapper:** wrap the test body in `if !path.exists() { eprintln!("skipping: fixture not yet landed"); return; }`. Phase-02.1 used this pattern in `subject.rs::starts_and_shuts_down_envoy_rust`. Once Task 12 lands, the test exercises the file.
2. **Land Tasks 11+12 as a back-to-back pair:** stage Task 11's diff, run gates against the combined Task 11+12 diff, commit both back-to-back. Phase-01 used this for tagged-driver-grammar + fixture 0001 migration (PLAN tasks 13–16).

Pick (1) so the workspace gate stays green between commits. Edit the test to:

```rust
    #[test]
    fn fixture_0003_expectations_parses_as_tcp_echo() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests/fixtures/0003-tcp-proxy/expectations.yaml");
        if !path.exists() {
            eprintln!("skipping: fixture 0003-tcp-proxy/expectations.yaml not yet landed (Task 12)");
            return;
        }
        let e = load_expectations(&path).expect("parses");
        assert!(matches!(e.driver, Driver::TcpEcho));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert_eq!(e.equivalence.response_status, None);
    }
```

Re-run; expect a "skipping" log and pass. Once Task 12 lands the file, the test exercises it.

- [ ] **Step 7: Extend `run_fixture` to spawn a `TcpProxyBackend` when the template references `{{BACKEND_PORT}}`.**

Replace the body of `tests/differential/src/lib.rs::run_fixture` (the function starting at the existing line ~331) with the following structure. Where the existing code reserved one host_port and built a single substitution map, extend to:

```rust
pub async fn run_fixture(fixture_dir: &Path) -> Result<()> {
    let expectations = load_expectations(&fixture_dir.join("expectations.yaml"))?;

    let host_port = reserve_port()?;

    let tmp = tempfile::tempdir().context("creating fixture temp dir")?;
    let upstream_template = std::fs::read_to_string(fixture_dir.join("envoy.yaml"))
        .context("reading upstream envoy.yaml")?;
    let subject_template = std::fs::read_to_string(fixture_dir.join("envoy-rust.yaml"))
        .context("reading envoy-rust.yaml")?;

    let upstream_port_str = upstream::CONTAINER_PORT.to_string();
    let subject_port_str = host_port.to_string();
    let port_key = match &expectations.driver {
        Driver::TcpEcho => "PORT",
        Driver::HttpGet { .. } => "ADMIN_PORT",
    };

    // Spawn a host-local backend if either template needs one. Holding the
    // backend in a binding outside the proxies' lifetime ensures the child
    // process outlives the fixture run; Drop fires after `run_fixture`'s
    // returns paths.
    let needs_backend = upstream_template.contains("{{BACKEND_PORT}}")
        || subject_template.contains("{{BACKEND_PORT}}");
    let _backend = if needs_backend {
        Some(backend::TcpProxyBackend::spawn().await.context("spawning backend")?)
    } else {
        None
    };
    let backend_port_str = _backend.as_ref().map(|b| b.port().to_string());

    let upstream_kvs: Vec<(&str, &str)> = {
        let mut v: Vec<(&str, &str)> = vec![(port_key, &upstream_port_str)];
        if let Some(bp) = backend_port_str.as_deref() {
            v.push(("BACKEND_PORT", bp));
            // Per ADR-0015: container-side reaches the host backend via
            // host.docker.internal (with the harness's with_host call below).
            v.push(("BACKEND_HOST", "host.docker.internal"));
        }
        v
    };
    let subject_kvs: Vec<(&str, &str)> = {
        let mut v: Vec<(&str, &str)> = vec![(port_key, &subject_port_str)];
        if let Some(bp) = backend_port_str.as_deref() {
            v.push(("BACKEND_PORT", bp));
            v.push(("BACKEND_HOST", "127.0.0.1"));
        }
        v
    };

    let upstream_yaml = render_yaml(&upstream_template, &upstream_kvs);
    let subject_yaml = render_yaml(&subject_template, &subject_kvs);
    let upstream_path = write_temp(tmp.path(), "envoy.yaml", &upstream_yaml)?;
    let subject_path = write_temp(tmp.path(), "envoy-rust.yaml", &subject_yaml)?;

    // The `host_uses_host_gateway` flag drives upstream::start to attach
    // `with_host("host.docker.internal", Host::HostGateway)` on the
    // testcontainers image (per ADR-0015). The flag is true exactly when the
    // upstream YAML actually references the hostname — silent when it
    // doesn't, so fixtures 0001 and 0002 stay unchanged.
    let host_uses_host_gateway = upstream_yaml.contains("host.docker.internal");
    let upstream = upstream::start(&upstream_path, host_uses_host_gateway).await?;
    let mut subject = subject::start(&subject_path, host_port).await?;

    let upstream_addr: SocketAddr = format!("127.0.0.1:{}", upstream.host_port()).parse()?;
    let subject_addr: SocketAddr = format!("127.0.0.1:{}", subject.port()).parse()?;

    let budget = Duration::from_secs(10);
    wait_accept_ready(upstream_addr, budget)
        .await
        .context("upstream Envoy never became accept-ready")?;
    wait_accept_ready(subject_addr, budget)
        .await
        .context("envoy-rust never became accept-ready")?;

    match &expectations.driver {
        Driver::TcpEcho => {
            let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
                .context("reading payload.bin")?;
            let upstream_out = drive_tcp(upstream_addr, &payload)
                .await
                .context("upstream envoy drive")?;
            let subject_out = drive_tcp(subject_addr, &payload)
                .await
                .context("envoy-rust drive")?;
            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);
            assert_equivalence(
                &expectations,
                None,
                None,
                &upstream_out,
                &subject_out,
            )?;
        }
        Driver::HttpGet { path, host } => {
            let upstream_resp = drive_http_get(upstream_addr, path, host)
                .await
                .context("upstream envoy http get")?;
            let subject_resp = drive_http_get(subject_addr, path, host)
                .await
                .context("envoy-rust http get")?;
            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);
            assert_equivalence(
                &expectations,
                Some(upstream_resp.status),
                Some(subject_resp.status),
                &upstream_resp.body,
                &subject_resp.body,
            )?;
        }
    }

    // _backend Drop fires here.
    Ok(())
}
```

- [ ] **Step 8: Extend `tests/differential/src/upstream.rs::start` to take a `host_gateway: bool` and apply `with_host` when true.**

Replace the existing `start` signature and body in `tests/differential/src/upstream.rs`:

```rust
/// Start upstream Envoy with `envoy_yaml_path` bind-mounted to
/// `/etc/envoy/envoy.yaml`. The caller must have already rendered any
/// `{{PORT}}` token in the YAML to `CONTAINER_PORT`.
///
/// `host_gateway = true` adds `with_host("host.docker.internal", Host::HostGateway)`
/// to the container image (per ADR-0015) — required when the fixture YAML
/// references `host.docker.internal` to reach a host-running backend.
/// `false` keeps the pre-02.2 behavior for fixtures that don't need
/// container-to-host reachability.
pub async fn start(envoy_yaml_path: &Path, host_gateway: bool) -> Result<UpstreamProxy> {
    let absolute = envoy_yaml_path
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", envoy_yaml_path.display()))?;
    let image = GenericImage::new(IMAGE_NAME, IMAGE_TAG)
        .with_exposed_port(CONTAINER_PORT.tcp())
        .with_wait_for(WaitFor::message_on_stderr("starting main dispatch loop"));
    let mut request = image
        .with_cmd(["-c", "/etc/envoy/envoy.yaml", "--log-level", "info"])
        .with_mount(Mount::bind_mount(
            absolute.to_string_lossy().to_string(),
            "/etc/envoy/envoy.yaml",
        ));
    if host_gateway {
        request = request.with_host(
            "host.docker.internal",
            testcontainers::core::Host::HostGateway,
        );
    }
    let container = request
        .start()
        .await
        .context("starting upstream envoy container")?;
    let host_port = container
        .get_host_port_ipv4(CONTAINER_PORT.tcp())
        .await
        .context("reading host-mapped port from testcontainers")?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(UpstreamProxy {
        _container: container,
        host_port,
    })
}
```

Update the `starts_upstream_envoy_and_exposes_host_port` integration test inside this file (lines 93–106) to pass `false` (it doesn't use a backend):

```rust
        let proxy = start(yaml.path(), false).await.unwrap();
```

Also import `ImageExt` if it isn't already imported via `use testcontainers::ImageExt;` — verify by inspecting the current imports at the top of `tests/differential/src/upstream.rs`. The existing imports line 5–9 already includes `testcontainers::{ContainerAsync, GenericImage, ImageExt, …}`, so the trait is in scope.

- [ ] **Step 9: Run all differential lib tests except the Docker-gated ones.**

```bash
cargo test -p differential --lib
```

Expected: all unit tests green. The new `fixture_0003_expectations_parses_as_tcp_echo` skips (not landed yet); the M3-tightened `decode_chunked_truncated_size_line` passes; both `render_yaml_substitutes_backend_keys_*` pass.

- [ ] **Step 10: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

All three: exit 0.

- [ ] **Step 11: Commit.**

```bash
git add tests/differential/src/lib.rs tests/differential/src/upstream.rs
git commit -m "phase 02.2: differential — backend keys + run_fixture dispatch + with_host [ADR-0015]"
```

Append a Task 11 PROGRESS section.

---

### Task 12: Fixture `0003-tcp-proxy` + Docker-gated `tests/differential/tests/tcp_proxy.rs`

**Files:**
- Create: `tests/fixtures/0003-tcp-proxy/envoy.yaml`
- Create: `tests/fixtures/0003-tcp-proxy/envoy-rust.yaml`
- Create: `tests/fixtures/0003-tcp-proxy/inputs/payload.bin` (copy of fixture 0001's payload.bin — 18 bytes `b"hello, envoy-rust\n"`)
- Create: `tests/fixtures/0003-tcp-proxy/expectations.yaml`
- Create: `tests/fixtures/0003-tcp-proxy/README.md`
- Create: `tests/differential/tests/tcp_proxy.rs`

**Scope:** the fixture itself + the Docker-gated acceptance test. Per SPEC §3 D5 and §6 signposts 6, 7, 9, 10.

- [ ] **Step 1: Create the fixture directory and copy `payload.bin`.**

```bash
mkdir -p tests/fixtures/0003-tcp-proxy/inputs
cp tests/fixtures/0001-tcp-echo/inputs/payload.bin tests/fixtures/0003-tcp-proxy/inputs/payload.bin
```

Verify:

```bash
xxd tests/fixtures/0003-tcp-proxy/inputs/payload.bin
```

Expected: 18 bytes `68 65 6c 6c 6f 2c 20 65 6e 76 6f 79 2d 72 75 73 74 0a` (`b"hello, envoy-rust\n"`). SPEC §6 signpost 10: reusing 0001's payload exactly minimizes cognitive load.

- [ ] **Step 2: Write `tests/fixtures/0003-tcp-proxy/envoy.yaml`.**

```yaml
node:
  id: envoy-rust-phase-02-subject
  cluster: envoy-rust-phase-02

admin:
  address:
    socket_address:
      address: 0.0.0.0
      port_value: 0

static_resources:
  listeners:
    - name: tcp_listener
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {{BACKEND_HOST}}
                      port_value: {{BACKEND_PORT}}
```

Note: `admin.port_value: 0` asks the kernel for an ephemeral port. SPEC §3 D5 contingency: if upstream Envoy v1.33.0 rejects this at runtime (boot-loop), land an ADR (likely ADR-0017 if it trips) introducing `{{ENVOY_ADMIN_PORT}}` and reserving an extra host port in the harness — do NOT silently change the schema. The plan's first try is the SPEC-prescribed shape.

`enable_half_close` is NOT set (per ADR-0016). SPEC §6 signpost 6 cautions against defensive `enable_half_close: false`.

- [ ] **Step 3: Write `tests/fixtures/0003-tcp-proxy/envoy-rust.yaml`.**

```yaml
node:
  id: envoy-rust-phase-02-subject
  cluster: envoy-rust-phase-02

static_resources:
  listeners:
    - name: tcp_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: {{BACKEND_PORT}}
```

Per SPEC §3 D5 divergences: listener bind `0.0.0.0` (container) vs. `127.0.0.1` (host subprocess); endpoint host `{{BACKEND_HOST}}` (templates to `host.docker.internal` on container side) vs. `127.0.0.1` literal on the host side; no admin block on envoy-rust side (phase 03+ scope per ADR-0011).

- [ ] **Step 4: Write `tests/fixtures/0003-tcp-proxy/expectations.yaml`.**

```yaml
driver:
  kind: tcp_echo
equivalence:
  response_body: byte_exact
```

- [ ] **Step 5: Write `tests/fixtures/0003-tcp-proxy/README.md`.**

```markdown
# Fixture 0003-tcp-proxy

This fixture drives an arbitrary byte payload through a listener configured with
`envoy.filters.network.tcp_proxy` → static cluster `backend` (one endpoint) → a
host-local `tcp-echo-server` helper process (the binary landed in phase 02.1).
Both upstream Envoy and envoy-rust dial the same backend.

The `driver.kind: tcp_echo` value refers to the harness's round-trip pattern
(write payload, read-exact, compare), not to Envoy's echo *filter* — reusing
the same `TcpEcho` driver across fixtures 0001 and 0003 proves that the harness
is data-plane-agnostic.

Cross-container host reachability is covered by ADR-0015; the
`{{BACKEND_HOST}}` divergence between `envoy.yaml` and `envoy-rust.yaml` is its
only non-harness divergence. Half-close posture is Envoy's v1.33.0 default
(`enable_half_close: false`), covered by ADR-0016.
```

- [ ] **Step 6: Re-run the forward-regression test from Task 11 to confirm the fixture now loads.**

```bash
cargo test -p differential fixture_0003_expectations_parses_as_tcp_echo
```

Expected: `test result: ok. 1 passed; 0 failed` — no longer "skipping" (the file exists).

- [ ] **Step 7: Write the Docker-gated acceptance test.**

```rust
//! Phase 02.2 differential acceptance test: drive a payload through a
//! tcp_proxy listener → static cluster → host-local tcp-echo-server backend.
//! Should produce identical bytes between upstream Envoy v1.33.0 and
//! envoy-rust. Docker-gated; in CI this runs on `ubuntu-latest` alongside
//! the phase-00 `echo_fixture` and phase-01 `admin_ready_fixture`.

use std::path::PathBuf;

#[tokio::test]
async fn tcp_proxy_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0003-tcp-proxy");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
```

Save as `tests/differential/tests/tcp_proxy.rs`. Note: this test is NOT marked `#[ignore]` — it follows the same pattern as `tests/differential/tests/echo.rs` and `tests/differential/tests/admin_ready.rs`, which run unconditionally and panic if Docker is unavailable. CI provides Docker; local dev without Docker sees the same failure mode as the existing two acceptance tests.

- [ ] **Step 8: Run the Docker-gated test (locally if Docker is available; otherwise skip and let CI verify).**

```bash
cargo test -p differential --test tcp_proxy
```

If Docker is available: expected to pass (full end-to-end byte round-trip through both proxies). If Docker is not available: expected to fail at upstream container start; this is the same behavior as `echo_fixture` / `admin_ready_fixture` in dev environments without Docker.

If the test fails for any reason OTHER than "Docker not available," debug per `superpowers:systematic-debugging`. Common failure modes to expect during execution:

- **`host-gateway` rejected by Docker.** Fall back to `172.17.0.1` under a new ADR (likely ADR-0017 if no other ADR has landed in execution); update `upstream::start` to take an enum or a literal IP address.
- **Upstream Envoy v1.33.0 rejects `admin.port_value: 0`.** Land ADR-0017 (or next-sequential) introducing `{{ENVOY_ADMIN_PORT}}`; reserve an extra port in `run_fixture` and substitute it.
- **TCP-echo-server binary path lookup fails.** Workspace-membership regression — verify `cargo build --workspace --all-targets` produces `target/debug/tcp-echo-server`.
- **TCP `127.0.0.1:1` connect-refused test (Task 8) flakes on some CI hosts.** Replace with a guaranteed-refused port (port 1 is reserved on every UNIX-like host; macOS / Linux behave the same here, but if a CI image has a lingering listener, swap for an OS-allocated port that's been dropped).

- [ ] **Step 9: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

The first three: exit 0. The fourth: all unit + bin tests pass; the Docker-gated `tcp_proxy_fixture` is excluded by `--lib --bins` and runs only via `cargo test --workspace` (CI).

- [ ] **Step 10: Commit.**

```bash
git add tests/fixtures/0003-tcp-proxy tests/differential/tests/tcp_proxy.rs
git commit -m "phase 02.2: fixture 0003-tcp-proxy [ADR-0015, ADR-0016]"
```

Append a Task 12 PROGRESS section.

---

### Task 13: State 4 phase-done gate

**Files:**
- Modify (append): `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PROGRESS.md`

**Per `docs/envoy-rust/SKILL_ROUTING.md` state 4.** Run the full local stable-toolchain gate, observe both CI jobs (build+test+lint, fuzz), quote outputs into PROGRESS.md. The plan does not advance ROADMAP.md or STATE.md here — those flip in state 6 (the phase-done commit), not now (BOOTSTRAP_PROMPT.md §5.1: one state per session).

If the gate exposes `Cargo.lock` drift (typical with the two new workspace members `envoy-listener` and `envoy-tcp`), land a dedicated `phase 02.2: sync Cargo.lock with phase 02.2 dep graph` commit immediately following Task 13's progress note. Phase-01 precedent: `4955252`. Phase-02.1 precedent: `dea4d16`.

- [ ] **Step 1: Run the local stable-toolchain gate, capturing each command's output.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
cargo deny check
```

Expected: all five exit 0. Quote tails into PROGRESS.md.

The `cargo test --workspace --lib --bins` count expands from 02.1's tally:
- `envoy-config`: 38 (unchanged, no schema changes in 02.2)
- `envoy-cluster`: 8 (unchanged, no cluster API changes in 02.2; phase-02.1 REVIEW M1 deferred)
- `envoy-listener`: 6 (Tasks 5–6: bind + 2 tests, serve + 4 tests)
- `envoy-tcp`: 4 (Task 8)
- `envoy-bin`: 18 (phase-01 base) + 1 (Task 3 `accepts_requests_exactly_at_cap`) = 19; the `tcp_proxy.rs` integration test runs separately
- `tcp-echo-server`: 8 (unchanged)
- `differential` lib: 26 (phase-02.1 close-out) + 2 (Task 10 backend tests) + 3 (Task 11 render_yaml + fixture_0003) = 31

Plus integration tests (`tests/admin_only.rs` from envoy-bin: 1; `tests/tcp_proxy.rs` from envoy-bin: 1) — these run via `cargo test --workspace`, not `--lib --bins`. The full integration count under `cargo test --workspace` adds the Docker-gated harness tests (`echo_fixture`, `admin_ready_fixture`, `tcp_proxy_fixture`).

- [ ] **Step 2: Trigger CI and observe both jobs.**

After committing Task 12's diff (already done), push the branch and observe the CI run:

```bash
git push origin <branch>
gh run list --workflow=ci.yml -L 1
gh run watch <run-id>
```

Expected: both `build + test + lint` (now also runs the new `tcp_proxy_fixture`) and `fuzz (parse_bootstrap, 30s)` jobs succeed. The fuzz job's behavior is unchanged from 02.1 (no schema changes in 02.2).

- [ ] **Step 3: If `Cargo.lock` is dirty, land a dedicated sync commit.**

```bash
git status
git diff Cargo.lock
git add Cargo.lock
git commit -m "phase 02.2: sync Cargo.lock with phase 02.2 dep graph"
```

The diff should add `[[package]]` stanzas for `envoy-listener v0.0.0` and `envoy-tcp v0.0.0` matching the deps declared in `crates/envoy-listener/Cargo.toml` and `crates/envoy-tcp/Cargo.toml`. No version bumps and no new transitive packages outside the existing tokio/thiserror/tracing graph; verify by `git diff` review before staging.

- [ ] **Step 4: Append the State-4 section to PROGRESS.md.**

Use the phase-02.1 PROGRESS State-4 section as the precedent shape. Quote the local-gate command outputs (per-crate test tails are the most informative), the CI run number + URL, and document any fix-during-gate commits (the goal is zero — phase 02.1 cleared on first attempt).

- [ ] **Step 5: Commit the PROGRESS update.**

```bash
git add docs/envoy-rust/phases/02.2-listener-tcp-proxy/PROGRESS.md
git commit -m "phase 02.2: state-4 phase-done gate verification (task 13)"
```

State 4 verification complete. Next session enters state 5 via `superpowers:requesting-code-review` (writing `REVIEW.md`); state 6 then ships the phase-done commit per `BOOTSTRAP_PROMPT.md` §5.3, flipping ROADMAP rows `02.2` and `02` (parent) to `done` in the same commit and advancing STATE.md to phase `03-tls-tcp` (lifecycle state 1).

---

## Out-of-plan execution contingencies

These are NOT plan steps; they are decision rules for situations the SPEC and plan jointly anticipate but cannot pin at planning time. Per D-3.5, execution lands an ADR and proceeds when any trigger fires.

1. **Upstream Envoy rejects `admin.port_value: 0` in fixture 0003.** Land an ADR (likely ADR-0017) introducing `{{ENVOY_ADMIN_PORT}}`; modify Task 11's `run_fixture` to reserve a second host port; modify Task 12's `envoy.yaml` to substitute `{{ENVOY_ADMIN_PORT}}`. Document in PROGRESS.md.

2. **Docker daemon refuses `host-gateway` on `ubuntu-latest`.** SPEC §6 signpost 4 marks this very unlikely. Fallback per ADR-0015 consequences: replace `with_host("host.docker.internal", Host::HostGateway)` with `with_host("host.docker.internal", Host::Addr(Ipv4Addr::new(172, 17, 0, 1).into()))` under a new ADR. Update `tests/differential/src/upstream.rs::start` accordingly.

3. **`cargo deny check` flips red on a new transitive surface.** Update `deny.toml` per ADR-0005's discipline (`wrappers` for direct-ban transitives, scoped `[advisories].ignore` with rationale for new advisories). Land under a new ADR.

4. **Test `proxies_returns_err_on_upstream_connect_refused` flakes.** Some CI runners may have a process listening on `127.0.0.1:1`. Replace with `reserve_port()` followed by an explicit `drop(listener)` (TOCTOU is acceptable per phase-01 SPEC §6 point 6) — get a port that's almost certainly free.

5. **ADR numbering shifts.** If any ADR-00NN lands during execution before Task 1, renumber 0015/0016 in lockstep at Task 1 Step 1; update every cross-reference in this PLAN, in fixture 0003 README, and in the final commit message.

6. **A task's scope balloons past ~10 sub-steps.** Invoke `superpowers:systematic-debugging` before splitting. Phase 02.2 has already been split (it's a sub-phase of 02); a nested split is not anticipated and deserves root-cause analysis (scope creep vs. planner overdecomposition).

---

## Final commit message format (state 6 — NOT this state)

The state-6 phase-done commit shape, per SPEC §9. Do NOT land this commit during plan execution; it lands at state 6 (after REVIEW.md is approved at state 5):

```
phase 02.2: Listener + TCP proxy filter + fixture 0003 [ADR-0015, ADR-0016]

Two new crates land the first real data-plane path: envoy-listener manages
bind/accept/drain with a shutdown-gated JoinSet; envoy-tcp implements the
TCP proxy filter via tokio::io::copy. envoy-bin wires a ClusterManager +
echo/tcp_proxy dispatch. Differential harness extends with TcpProxyBackend,
render_yaml backend-key substitution, run_fixture dispatch, and upstream
with_host("host.docker.internal", HostGateway). Fixture 0003-tcp-proxy lands
green end-to-end. Phase-01 REVIEW §9 starter items I4 (admin cap tightening)
and M1 (stale TODO retarget) close alongside. Parent phase 02 row flips
done in the same commit.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (byte-exact payload round-trip through
  tcp_proxy → STATIC cluster, one endpoint, to host-local tcp-echo-server).
Conformance: none.
```

The state-6 commit also flips:
- `docs/envoy-rust/ROADMAP.md` row `02.2` `status` → `done`.
- `docs/envoy-rust/ROADMAP.md` row `02` (parent) `status` → `done` (per the schema: parent flips when all sub-phases `done`; 02.1 is already `done`).
- `docs/envoy-rust/STATE.md` → active id `03`, slug `03-tls-tcp`, lifecycle state 1 (phase-03 directory does not yet exist), next-skill `superpowers:brainstorming`.
- Appends a final State-6 section to PROGRESS.md.
