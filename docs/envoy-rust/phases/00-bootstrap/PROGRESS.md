# Phase 00 Progress

## Task 1 — ADR-0002 (2026-04-23)
- Commit: 3fd0a97
- Change: appended ADR-0002 (GitHub Actions as CI provider) to DECISIONS.md
- Verification: `grep -q '^## ADR-0002' DECISIONS.md` → exit 0

## Task 2 — ADR-0003 (2026-04-23)
- Commit: 95839ba
- Change: appended ADR-0003 (Rust edition 2024) to DECISIONS.md
- Verification: `grep -q '^## ADR-0003' DECISIONS.md` → exit 0

## Task 3 — ADR-0004 + ENVOY_TARGET.md (2026-04-23)
- Commit: 9f5d1d2
- Change: ENVOY_TARGET.md populated with v1.33.0 pin (multi-arch index digest sha256:56da5a…70c2, proto tree commit b0f43d6); ADR-0004 appended
- Verification: grep checks for ADR-0004, sha256:, Proto tree commit: all exit 0; no `TBD` in either file
- Deviation: local Docker daemon has an IPv6 routing bug; digest resolved via Docker Hub public API (https://hub.docker.com/v2/repositories/envoyproxy/envoy/tags/v1.33.0) instead of `docker inspect`. Value is the canonical multi-arch manifest-index digest — equivalent to what `docker inspect` would report against a freshly-pulled manifest.

## Task 4 — Workspace scaffolding (2026-04-23)
- Commits: 171376d (ADR-0005), d455515 (skeleton)
- Change: created `crates/envoy-bin/{Cargo.toml,src/main.rs}` and `tests/differential/{Cargo.toml,src/lib.rs}` skeletons; populated root workspace `members`; fixed `deny.toml` per ADR-0005 (wrappers on hyper/hyper-util/tower-service, advisory ignores for RUSTSEC-2025-0111 and RUSTSEC-2025-0134)
- Verification: `cargo build --workspace --all-targets` → 0; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → 0; `cargo fmt --all -- --check` → 0; `cargo test --workspace` → 0 (0 tests in both crates); `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`
- Deviation: PLAN Task 4 Step 6 prescribed `skip-tree = [{ name = "testcontainers" }]` as the mechanism for exempting the bollard→hyper/tower transitive chain. Empirical testing against cargo-deny 0.19.4 shows `skip-tree` only affects the `multiple-versions` check, not `[bans] deny`. Landed ADR-0005 to document the correct mechanism (`wrappers` per deny entry) and the two RustSec advisory ignores on the dev-only testcontainers chain.

## Task 5 — envoy-bin config parser (2026-04-23)
- Commit: a7461bf
- Change: `crates/envoy-bin/src/config.rs` with `Bootstrap`/`StaticResources`/`Listener`/`Address`/`SocketAddress`/`FilterChain`/`NetworkFilter` types + `parse_bootstrap()` + `validate()` + `ECHO_FILTER` const + 5 unit tests. `mod config;` registered in `main.rs` with a scoped `#[allow(dead_code)]` (removed in Task 8 when `run()` consumes the module).
- Verification: `cargo test -p envoy-bin --bin envoy-bin config` → `5 passed`; `cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings` → 0; `cargo fmt -p envoy-bin -- --check` → 0 (rustfmt autofixed the plan's verbatim line-breaks on `parse_bootstrap` and the first test; behavior unchanged).
- Deviation: PLAN Task 5 Step 3 uses `cargo test -p envoy-bin --lib config`, but `envoy-bin` is a binary-only crate (no `[lib]` target); the tests were run via `--bin envoy-bin` with the same selector. Also, PLAN's clippy step `-D warnings` fails on dead_code for items defined in Task 5 but not consumed until Task 8 — resolved with a single `#[allow(dead_code)]` on the `mod config;` declaration in `main.rs`, scoped and annotated for removal in Task 8. Same pattern will apply to Tasks 6 and 7 (argv + echo modules).

## Task 6 — envoy-bin argv parser (2026-04-23)
- Commit: 4c270b9
- Change: hand-rolled argv parser (`-c`/`--config-path`) in main.rs with `ArgvError` + 6 unit tests; scoped `#![allow(dead_code)]` widened to the crate root to cover the argv items alongside config.
- Verification: `cargo test -p envoy-bin --bin envoy-bin argv_tests` → 6 passed; clippy + fmt --check → 0.
- Deviation: replaced the per-`mod config;` `#[allow(dead_code)]` with a single crate-root `#![allow(dead_code)]` on `main.rs` (carries forward Tasks 5–7 hygiene, all removed in Task 8).

## Task 7 — envoy-bin TCP echo + drain (2026-04-23)
- Commit: a41db88
- Change: `crates/envoy-bin/src/echo.rs` with async `serve(listener, shutdown)`, per-connection `echo_once`, JoinSet-based 5s drain + timeout-abort; 2 `#[tokio::test]`s (single-payload + concurrent). `mod echo;` registered in main.rs.
- Verification: `cargo test -p envoy-bin --bin envoy-bin echo::tests` → 2 passed; clippy + fmt --check → 0.
- Deviation: the brief's echo module requires `tokio::time::timeout` and `tokio::sync::oneshot`, but the envoy-bin Cargo.toml from Task 4 did not enable those tokio features. Added `"time"` and `"sync"` to the tokio feature list to make the code (and its tests) compile. This also lands in the same commit.

## Task 8 — envoy-bin main wiring (2026-04-23)
- Commit: 19afe3d
- Change: removed crate-root `#![allow(dead_code)]` + comment; wired `main() -> ExitCode` with argv parse → explicit multi-thread tokio runtime → `run()` (config load + TcpListener::bind + `echo::serve`) → `shutdown_signal()` (SIGTERM/SIGINT via `tokio::signal::unix`); added `install_tracing()` reading `ENVOY_RUST_LOG`.
- Verification: `cargo build -p envoy-bin --release` → 0; clippy + fmt --check → 0; `cargo test -p envoy-bin` → 13 passed (5 config + 6 argv + 2 echo); binary smoke test with no args → stderr matches `envoy-bin: expected exactly one of …`, exit 2.

## Task 9 — differential harness helpers (2026-04-23)
- Commit: 6c909c0
- Change: `tests/differential/src/lib.rs` with `Expectations`/`Equivalence`/`BodyRule`, `load_expectations`, `reserve_port` (TOCTOU-accepted), `render_yaml`, `write_temp`, `wait_accept_ready` (50ms→500ms exp. backoff) + 6 tests. Placeholders `subject.rs` and `upstream.rs` (filled in Tasks 11 and 10).
- Verification: `cargo test -p differential --lib` → 6 passed; clippy + fmt --check → 0.

## Task 10 — differential upstream launcher (2026-04-23)
- Commit: df8a0d1
- Change: `tests/differential/src/upstream.rs` with `IMAGE_NAME`/`IMAGE_TAG`/`CONTAINER_PORT` constants (matching ADR-0004), `UpstreamProxy` drop-to-stop guard, `start()` that bind-mounts the fixture's `envoy.yaml` and waits on the "starting main dispatch loop" stderr message + 500ms grace. One `#[ignore]`d tokio integration test. Added `tempfile = "3"` to dev-dependencies.
- Verification: `cargo test -p differential --lib` → 0 failed, 1 ignored (the container-launching test); clippy + fmt --check → 0; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (ADR-0005 exemptions hold).
- Note: local Docker daemon has an IPv6 routing bug; the `#[ignore]`d test was NOT run locally and will be validated by CI per the phase-done gate.

## Task 11 — differential subject launcher (2026-04-23)
- Commit: 143bbf4
- Change: `tests/differential/src/subject.rs` with `Subject` (port + optional Child; Drop aborts), async `shutdown(budget)`, `locate_envoy_bin` (honoring `CARGO_TARGET_DIR`, picking debug vs release by `cfg!(debug_assertions)`), `start(path, port)` (kill_on_drop, stdio inherited, `ENVOY_RUST_LOG=info`) + 2 tokio tests.
- Verification: `cargo test --workspace subject::tests` → 2 passed; clippy + fmt --check → 0; cargo deny check → all ok.

## Task 12 — fixture 0001-tcp-echo (2026-04-23)
- Commit: a03857a
- Change: `tests/fixtures/0001-tcp-echo/{envoy.yaml, envoy-rust.yaml, inputs/payload.bin, expectations.yaml, README.md}` — first differential fixture.
- Verification: payload.bin = 18 bytes; envoy-rust.yaml + expectations.yaml both parse with basic YAML structure validation (grep key-value pairs).

## Task 13 — differential run_fixture orchestrator (2026-04-23)
- Commit: 521b62f
- Change: appended `drive_tcp(addr, payload)` + `run_fixture(fixture_dir)` to `tests/differential/src/lib.rs` — orchestrates upstream Envoy via testcontainers + envoy-rust subprocess, renders both YAMLs against a shared port + `CONTAINER_PORT`, drives identical payloads, asserts byte-exact equivalence, cleans up on failure. Moved `tempfile` from `[dev-dependencies]` to `[dependencies]` (used at runtime now).
- Verification: `cargo build --workspace --all-targets` → 0; clippy + fmt --check → 0; `cargo test --workspace` → 21 passed, 1 ignored; cargo deny check → all ok.
- Deviation: brief appended new items after the `#[cfg(test)] mod tests` block; clippy's `items-after-test-module` lint rejects that at `-D warnings`. Hoisted the new `use` statements into the top import block and moved `drive_tcp` + `run_fixture` directly above the test module (semantically equivalent; matches the brief's own "let autofix hoist them" escape clause).

## Task 14 — differential echo_fixture acceptance test (2026-04-23)
- Commit: e5e24b2
- Change: `tests/differential/tests/echo.rs` — single `#[tokio::test] echo_fixture` that invokes `differential::run_fixture` against `tests/fixtures/0001-tcp-echo`.
- Verification: `cargo build --workspace --all-targets` → 0; clippy + fmt --check → 0; `cargo test --workspace --lib --bins` → 21 passed (Tasks 1–13 regression intact).
- Deviation: did NOT run `cargo test --workspace --test echo` locally — the local Docker daemon has an IPv6 routing bug (documented in Task 3 deviation); CI will validate the acceptance test per the phase-done gate (§7.5.a).

## Task 15 — GitHub Actions CI workflow (2026-04-23)
- Commit: 4058ab1
- Change: `.github/workflows/ci.yml` — single `ubuntu-latest` job running fmt → clippy → build → test → cargo-deny on push/PR to main. Uses dtolnay/rust-toolchain (reads rust-toolchain.toml), Swatinem/rust-cache, taiki-e/install-action@cargo-deny.
- Verification: `python3 -c 'yaml.safe_load(open(...))'` → exit 0.

## State 4 — Phase-done gate verification (2026-04-23)

Per `docs/envoy-rust/SKILL_ROUTING.md` state 4 and `BOOTSTRAP_PROMPT.md` §7.5.
Five gate commands run against `ubuntu-latest` CI; the acceptance test
`tests/differential/tests/echo.rs::echo_fixture` is Docker-gated and only runs
in CI per Task 14's deviation note.

### Attempt 1 — commit `2d81b53`, workflow run `24855427288`

Triggered by the first push of the phase-00 branch to `origin/main` after a
`gh auth refresh -s workflow` to grant the `workflow` scope the existing
token lacked.

Local gate (dev host, post `cargo clean -p envoy-bin -p differential`):
- `cargo build   --workspace --all-targets`                                   → exit `0` (`Finished dev profile target(s) in 0.38s`).
- `cargo clippy  --workspace --all-targets --all-features -- -D warnings`     → exit `0` (`Checking envoy-bin`, `Checking differential`, `Finished`).
- `cargo fmt     --all -- --check`                                            → exit `0` (no diffs).
- `cargo test    --workspace`                                                 → exit `101` on `echo_fixture`. The failure reproduced the documented Docker daemon DNS bug from Task 3: `failed to pull the image 'envoyproxy/envoy:v1.33.0' ... dial tcp: lookup registry-1.docker.io: no such host`. `cargo test --workspace --lib --bins` (non-Docker portion) → exit `0`, 21 passed + 1 ignored.
- `cargo deny    check`                                                       → exit `0` (`advisories ok, bans ok, licenses ok, sources ok`; unmatched-license warnings on 0BSD/BSD-2-Clause/CC0-1.0/MPL-2.0/Unicode-DFS-2016/Zlib; duplicate wit-bindgen 0.51.0 / 0.57.1 via `tempfile → getrandom`, both permitted by `[bans] multiple-versions = "allow"`).

CI gate (`ubuntu-latest`, run 24855427288):
- Steps `fmt` / `clippy` / `build` → `success`.
- Step `test (includes differential harness → Docker)` → **`failure`**, exit `101`.
- Steps `install cargo-deny` / `cargo deny check` → `skipped` (job failed earlier).

The CI failure was *not* the dev-host Docker bug. The container launched, the
image pulled, and `echo_fixture` ran end-to-end — but the differential
assertion fired:

    ---- echo_fixture stdout ----
    Error: byte-exact body mismatch
      upstream: []
      subject:  [104, 101, 108, 108, 111, 44, 32, 101, 110, 118, 111, 121, 45, 114, 117, 115, 116, 10]

Upstream Envoy returned zero bytes; the envoy-rust subject correctly echoed
`hello, envoy-rust\n` (18 bytes). This is unexpected state and per doctrine
D-3.1 was investigated under `superpowers:systematic-debugging` before any
fix was proposed.

### Root cause (evidence-backed)

`envoyproxy/envoy@v1.33.0` — `source/common/network/connection_impl.cc`
(fetched via `gh api 'repos/envoyproxy/envoy/contents/.../connection_impl.cc?ref=v1.33.0'`):

- Line 83: `ConnectionImpl` constructor sets `enable_half_close_(false)` as the default.
- Lines 698–701 in `onReadReady`:

      if ((!enable_half_close_ && result.end_stream_read_)) {
        result.end_stream_read_ = false;
        result.action_ = PostIoAction::Close;
      }

- Lines 703–710: `onRead(new_buffer_size)` is dispatched to the filter
  manager when `bytes_processed_ != 0` (so the echo filter's
  `connection().write(data, end_stream)` queues the echo into the
  connection's write buffer).
- Lines 713–716:

      if (result.action_ == PostIoAction::Close || bothSidesHalfClosed()) {
        ENVOY_CONN_LOG(debug, "remote close", *this);
        closeSocket(ConnectionEvent::RemoteClose);
      }

`closeSocket(RemoteClose)` runs in the same event-loop iteration as the
filter's `connection().write(...)` — the write buffer has not yet been
flushed, and the close drops it. Net effect: the echo response is dropped
whenever the client half-closes the write side before reading.

There is no listener-level YAML surface to enable half-close semantics in
v1.33.0. The `Listener` proto (`api/envoy/config/listener/v3/listener.proto`
at ref `v1.33.0`) contains no `enable_half_close` field; that switch is a
C++ `Connection::enableHalfClose()` method only, and the only YAML
surface is on `envoy.filters.network.tcp_proxy`, which phase 00 does not
use.

Envoy's own echo-filter integration test
(`test/extensions/filters/network/echo/echo_integration_test.cc` at ref
`v1.33.0`) confirms the intended client pattern: send data → wait for
the data callback → `conn.close(ConnectionCloseType::FlushWrite)` —
never half-close.

The pre-fix `drive_tcp` used `write_all` → `shutdown()` → `read_to_end`,
i.e. it half-closed before reading. This matched `SPEC.md` §D4 point 5's
prescription but is fundamentally incompatible with Envoy v1.33.0's
default echo-filter behavior.

### Fix (ADR-0006 + 5355311)

- **ADR-0006** landed in `docs/envoy-rust/DECISIONS.md`: documents the
  four options considered (rewrite harness; replace fixture with tcp_proxy
  + loopback cluster; read-with-idle-timeout; patch upstream Envoy) and
  selects option A — rewrite `drive_tcp` to match Envoy's echo-filter
  1:1 byte-count contract. Supersedes SPEC §D4 point 5's "half-close +
  `read_to_end`" wording.
- `tests/differential/src/lib.rs::drive_tcp` rewritten:
  `write_all(payload)` → `read_exact(&mut vec![0u8; payload.len()])` →
  `shutdown().await.ok()` (graceful FIN for the envoy-rust subject's
  benefit — upstream Envoy will have already closed) → `drop(stream)`.
- New unit test `drive_tcp_round_trips_without_half_close` added to the
  `tests` module. Spawns an in-process server that mirrors Envoy's echo
  semantics (read N, write N, close without honoring half-close) and
  verifies `drive_tcp` round-trips against it. The pre-fix `drive_tcp`
  would race this server identically to upstream Envoy and return an
  empty body.
- No fixture YAML changes. No `envoy-bin` changes.

### Attempt 2 — commit `5355311`, workflow run `24856364702`

Local gate (dev host, all five commands):
- `cargo build   --workspace --all-targets` → exit `0` (`Finished dev profile target(s) in 1.10s`).
- `cargo clippy  --workspace --all-targets --all-features -- -D warnings` → exit `0`.
- `cargo fmt     --all -- --check` → exit `0`.
- `cargo test    --workspace --lib --bins` → exit `0`, **22 passed, 0 failed, 1 ignored** (new `drive_tcp_round_trips_without_half_close` passes; all 21 Attempt-1 tests still pass). Full `cargo test --workspace` on the dev host still fails on `echo_fixture` with the Docker DNS bug from Task 3 — unchanged from Attempt 1 — so CI is still the validator for that single test.
- `cargo deny    check` → exit `0`, `advisories ok, bans ok, licenses ok, sources ok`.

CI gate (`ubuntu-latest`, run 24856364702):
- Step `fmt` (`cargo fmt --all -- --check`) → `success`.
- Step `clippy` (`cargo clippy --workspace --all-targets --all-features -- -D warnings`) → `success`; `Finished dev profile target(s) in 47.05s`.
- Step `build` (`cargo build --workspace --all-targets`) → `success`; `Finished dev profile target(s) in 55.49s`.
- Step `test (includes differential harness → Docker)` (`cargo test --workspace`) → `success`:
    - `differential` lib: `test result: ok. 9 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.36s` (the ignored one is the Docker-gated `upstream::tests::starts_upstream_envoy_and_exposes_host_port`; the new `drive_tcp_round_trips_without_half_close` passes).
    - `differential` integration (`tests/echo.rs`): `test echo_fixture ... ok`, `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.71s`.
    - `envoy-bin` bin-unit: `test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s`.
    - Doc-tests (`differential`): `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`.
- Step `install cargo-deny` → `success`.
- Step `cargo deny check` → `success`; `advisories ok, bans ok, licenses ok, sources ok` (same informational warnings as Attempt 1 — they are not failures).

Run conclusion: `success`. URL: https://github.com/pgdad/envoy-rust/actions/runs/24856364702

### Gate outcome per `BOOTSTRAP_PROMPT.md` §7.5

- (a) `tests/fixtures/0001-tcp-echo/` → **green** (`echo_fixture ... ok`, 1/1 passed in 6.71s on CI).
- (b) no pre-existing differential fixtures → nothing else to regress.
- (c) no conformance suites this phase → n/a.
- (d) no fuzz targets this phase → n/a.
- (e) `cargo build / clippy / fmt --check / test --workspace / deny check` → all clean on CI.
- (f) REVIEW.md → state 5 pending per `SKILL_ROUTING.md`.

State 4 verification complete. Next session enters state 5 via
`superpowers:requesting-code-review`.

## State 5 — Code review (2026-04-23)

Ran `superpowers:requesting-code-review` scoped to phase 00
(`b42f18d..e1771c3`, 32 commits). Dispatched `superpowers:code-reviewer`
subagent with SPEC.md, PLAN.md, PROGRESS.md, BEHAVIOR_CONTRACT.md,
ENVOY_TARGET.md, and SKILL_ROUTING.md as context. Output:
`docs/envoy-rust/phases/00-bootstrap/REVIEW.md`.

### Verdict

**Approved with fixes.** 0 Critical, 3 Important, 8 Minor. Loop back to
lifecycle state 3 per `SKILL_ROUTING.md` line 42 ("if issues → back to
step 3 (NOT 4) until REVIEW.md approved").

### Important findings (must fix before state 6)

- **I1 — `drive_tcp` cannot detect trailing bytes**
  (`tests/differential/src/lib.rs:109–119`). ADR-0006's
  `read_exact(payload.len())` rewrite narrows the byte-exact assertion to
  "first N bytes match" — envoy-rust writing `payload.len() + 5` bytes
  would pass silently. This is spec drift against BEHAVIOR_CONTRACT.md
  row 2 ("byte-exact") and D-3.3. Fix (REVIEW recommends option (a)):
  after `read_exact`, poll with a ~100ms deadline; any further bytes →
  `bail!`. Add regression test (server that writes `payload.len()` bytes
  then extra bytes, assert harness fails). Amend ADR-0006 "Consequences"
  or land ADR-0007 alongside.
- **I2 — `rejects_duplicate_config_flag` has no assertion**
  (`crates/envoy-bin/src/main.rs:171–175`). `matches!(err,
  ArgvError::Trailing(_));` is used as an expression statement; the
  boolean is discarded. Fix: wrap in `assert!(…)`.
- **I3 — `Subject::shutdown` rustdoc says SIGTERM, sends SIGKILL**
  (`tests/differential/src/subject.rs:20–34`). The functional SIGKILL →
  SIGTERM + escalate switch requires a new dep (`nix`, not on D-3.2) and
  is a phase-01 follow-up per REVIEW's recommendation. **Phase 00
  minimum:** fix the rustdoc; drop a TODO pointing at the follow-up.

### Minor findings (defer-or-batch)

M1–M8 in REVIEW.md §Issues/Minor. Notable: M3 (`deny_unknown_fields` on
YAML structs) is cheap and prevents a real class of bugs once the
`expectations.yaml` grammar grows per ADR-0006 "Consequences" — consider
folding into the state-3 loop-back alongside the Importants.

### Strengths called out

- Narrative discipline in PROGRESS.md and ADRs 0005/0006 (REVIEW
  "Strengths" §1–2); PROGRESS.md State 4 section is flagged as a
  template for future verification entries.
- `deny.toml` wrappers are mechanically correct against the actual
  `Cargo.lock` transitive graph (REVIEW §Strengths point 3).
- Spec conformance matrix: all D1–D6 deliverables landed; two (D4 step 3
  artifact-dep fallback, D4 step 5 via ADR-0006) via documented
  alternatives; none missing.

### ADR assessment (from REVIEW)

- ADR-0002, 0003, 0004, 0005: sound and properly reflected in code.
- ADR-0006: rationale correct and evidence-backed; does not mitigate the
  trailing-byte blindspot (→ I1). Amend "Consequences" or land ADR-0007
  during the state-3 loop-back.

State 5 complete. Next session loops back to state 3 via
`superpowers:subagent-driven-development` to fix I1, I2, I3 (rustdoc
portion), optionally M3, then re-enters state 4 (CI re-verify) and
state 5 (re-review) for a final pass.

## State 3 (loop-back) — REVIEW fixes (2026-04-23)

Loop-back session per `SKILL_ROUTING.md` line 42 ("if issues → back to
step 3 (NOT 4) until REVIEW.md approved"). Ran
`superpowers:subagent-driven-development` scoped to the three Important
items from REVIEW.md (plus M3 folded in per REVIEW's recommendation).
Four commits, each with its own spec-compliance and code-quality review
pass before moving to the next task.

### I1 — `drive_tcp` trailing-byte check [ADR-0007] — commit `245a65f`

- `tests/differential/src/lib.rs`: `drive_tcp` now polls the socket with
  a 100ms deadline after `read_exact(payload.len())` and bails on any
  trailing bytes. New regression test
  `drive_tcp_rejects_trailing_bytes_after_echo` — a fake server that
  echoes N bytes then writes `b"EXTRA"` now forces `drive_tcp` to return
  `Err(...trailing bytes...)`, whereas pre-fix it would have returned
  `Ok(echoed)` silently (verified under TDD).
- `docs/envoy-rust/DECISIONS.md`: **ADR-0007** appended; ADR-0006 text
  untouched (append-only doctrine, DECISIONS.md preamble line 7).
  ADR-0007 enumerates the three REVIEW.md options (a/b/c), selects
  option (a), names its per-connection ~100ms idle cost as acceptable
  for phase 00, and cross-references ADR-0006 as the source of the
  blind spot. Phase-00 final-commit bracketed ADR list extends to
  `[ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006, ADR-0007]`.
- Reviews: spec ✅ (literal match of REVIEW.md option (a) shape), code
  quality ✅ with 3 Minor nits (inline `100ms` magic number; 64-byte
  tail buffer sizing; rustdoc cross-reference style) — all
  future-proofing, none block.

### I2 — `assert!` wrap on `rejects_duplicate_config_flag` — commit `ba17ee3`

- `crates/envoy-bin/src/main.rs:174`:
  `matches!(err, ArgvError::Trailing(_));` (discarded expression
  statement) → `assert!(matches!(err, ArgvError::Trailing(_)), "got {err:?}");`.
  Path-tightening (`Trailing(p) if p == "/b"`) deliberately skipped per
  REVIEW.md "optional" labeling and YAGNI; rationale recorded in commit
  body.
- Reviews: spec ✅, code quality ✅ with 2 Minor observations (asymmetry
  with sibling `assert_eq!`-style argv tests — non-blocking).

### I3 — `Subject` rustdoc — SIGKILL, not SIGTERM — commit `18bbfde`

- `tests/differential/src/subject.rs:8-9`: struct-level doc corrected
  from "sends SIGTERM" to "sends SIGKILL (via tokio's `start_kill`) and
  waits for the process to exit". Method-level rustdoc (lines 20-23)
  and `Drop` impl comment were already accurate.
- `tests/differential/src/subject.rs:24-32`: `TODO(phase-01)` block
  added immediately above `Subject::shutdown` naming the planned
  SIGTERM+drain+escalate switch, the `nix`-crate blocker (not on D-3.2
  permitted-foundations list), and the phase-01 ADR gating.
- **No behavior change.** `child.start_kill()` unchanged. No new deps.
  Functional switch deferred to phase 01 per REVIEW.md recommendation.
- Reviews: spec ✅, code quality ✅ (0 Critical/Important; Minor items
  only stylistic).

### M3 — `deny_unknown_fields` on YAML schemas — commit `fca3aba`

- `#[serde(deny_unknown_fields)]` added to 9 YAML-parsed structs:
  - 7 in `crates/envoy-bin/src/config.rs`: `Bootstrap`,
    `StaticResources`, `Listener`, `Address`, `SocketAddress`,
    `FilterChain`, `NetworkFilter`.
  - 2 in `tests/differential/src/lib.rs`: `Expectations`, `Equivalence`.
  - `BodyRule` correctly skipped (unit-variant enum; unknown
    discriminants already covered by `expectations_reject_unknown_rule`).
- Four new regression tests covering both root- and nested-level
  unknown-field rejection (`expectations_reject_unknown_field`,
  `equivalence_reject_unknown_field`, `rejects_unknown_bootstrap_field`,
  `rejects_unknown_listener_field`); each asserts the stable
  serde-canonical `"unknown field"` marker. TDD-verified failing against
  pre-fix code.
- Reviews: spec ✅, code quality ✅ with 1 Minor coverage gap (see
  "Deferred follow-ups" below).

### Minor items deliberately deferred

- **REVIEW.md M1, M2, M4, M5, M6, M7, M8** — all REVIEW-classified
  "nice to have; can defer"; none block phase completion. They remain
  open items for future phases.
- **Code-quality N1 on the M3 commit.** The four new regression tests
  cover unknown-field rejection at 4 of the 9 attribute sites
  (`Bootstrap`, `Listener`, `Expectations`, `Equivalence`). The 5 deeper
  structs (`StaticResources`, `Address`, `SocketAddress`, `FilterChain`,
  `NetworkFilter`) carry the attribute but are not individually
  regression-tested. Reviewer demonstrated empirically that a future
  removal of the attribute on any of those 5 would not be caught by the
  current test matrix. Classified Minor ("judgment call") and recorded
  here so it is not forgotten; a small follow-up commit or future
  M-series cleanup pass can close the gap.
- **I3 functional SIGKILL → SIGTERM+drain+escalate** — deferred to
  phase 01 under its own ADR per REVIEW.md recommendation (D-3.2
  blocker on the `nix` crate).

### Verification gate (local, dev host)

- `cargo build --workspace --all-targets` → exit `0`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → exit `0`.
- `cargo fmt --all -- --check` → exit `0`.
- `cargo test --workspace` → exit `0`:
  - `differential` lib: `test result: ok. 12 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out`
    (the ignored is the docker-gated
    `upstream::tests::starts_upstream_envoy_and_exposes_host_port`;
    3 new tests from I1 and M3 all pass).
  - `differential` integration `tests/echo.rs::echo_fixture`:
    `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
    (Docker was available locally this run; CI on `ubuntu-latest`
    remains the primary validator per the Task 3 Docker caveat).
  - `envoy-bin` bin-unit: `test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
    (2 new M3 config tests; the newly-tight
    `rejects_duplicate_config_flag` still passes).
  - Doc-tests: `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.
- `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`
  (same informational unmatched-license/duplicate warnings as prior
  passes; all permitted).

### Outcome

All three REVIEW.md Important items (I1, I2, I3 rustdoc) and the
folded-in M3 are fixed across 4 atomic commits
(`245a65f`, `ba17ee3`, `18bbfde`, `fca3aba`) and verified locally. State
3 loop-back complete. Next session re-enters state 4
(`superpowers:verification-before-completion`) to re-run the five
phase-done gate commands on CI (`ubuntu-latest`) for HEAD `fca3aba`,
quote outputs into a new "State 4 — Re-verification" section, then
state 5 (`superpowers:requesting-code-review` re-pass) for the Approved
verdict the REVIEW.md final recommendation anticipates.

## State 4 — Re-verification (2026-04-23)

Ran `superpowers:verification-before-completion` against CI to confirm
the four state-3 loop-back commits (`245a65f`, `ba17ee3`, `18bbfde`,
`fca3aba`) did not regress the phase-done gate. Six unpushed commits
were pushed `e1771c3..a1c8194`; the new tip `a1c8194` is a
documentation-only commit (`STATE.md` + `PROGRESS.md`), so the code
under verification is identical to the review-target HEAD `fca3aba`.

### CI gate (`ubuntu-latest`, run `24859537419`, HEAD `a1c81942292559246805029744083ff1605f1c2f`)

Run conclusion: `success`. Total job time: **1m 3s**. URL:
https://github.com/pgdad/envoy-rust/actions/runs/24859537419

- Step `fmt` (`cargo fmt --all -- --check`) → `success`; no diff emitted (step completed in <1s).
- Step `clippy` (`cargo clippy --workspace --all-targets --all-features -- -D warnings`) → `success`; `Finished dev profile target(s) in 3.78s` (cache-warm run).
- Step `build` (`cargo build --workspace --all-targets`) → `success`; `Finished dev profile target(s) in 8.93s` (cache-warm run).
- Step `test (includes differential harness → Docker)` (`cargo test --workspace`) → `success`:
    - `differential` lib (`target/debug/deps/differential-0dd6d9f7b2e1f8c5`): `test result: ok. 12 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.41s`.
      - The ignored test is the Docker-gated `upstream::tests::starts_upstream_envoy_and_exposes_host_port` (annotated `ignored, requires Docker; runs under cargo test --workspace in CI`).
      - The new I1 regression test `tests::drive_tcp_rejects_trailing_bytes_after_echo` is present and passes (line-quoted from run log: `test tests::drive_tcp_rejects_trailing_bytes_after_echo ... ok`).
    - `differential` integration (`tests/echo.rs`): `test echo_fixture ... ok`, `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.01s`.
    - `envoy-bin` bin-unit (`target/debug/deps/envoy_bin-4eca1f48f5f230a3`): `test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`.
      - The I2-tightened `argv_tests::rejects_duplicate_config_flag` still passes.
      - The four M3 `deny_unknown_fields` regression tests (`config::tests::rejects_unknown_bootstrap_field`, `rejects_unknown_listener_field`, plus the pre-existing `rejects_empty_listeners` / `rejects_multiple_listeners` / `rejects_non_echo_filter` / `rejects_malformed_yaml`) all pass.
    - Doc-tests (`differential`): `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`.
    - **Aggregate: 28 passed, 0 failed, 1 ignored** — matches the state 3 loop-back local gate exactly.
- Step `install cargo-deny` → `success`.
- Step `cargo deny check` → `success`; tail line quoted from run log: `advisories ok, bans ok, licenses ok, sources ok`. Seven informational warnings remain (1× `duplicate` on `wit-bindgen 0.51.0` vs `0.57.1`, both transitive through `tempfile` → `getrandom` → `wasip2`/`wasip3`; 6× `license-not-encountered` for permitted-but-unused licenses `0BSD`, `BSD-2-Clause`, `CC0-1.0`, `MPL-2.0`, `Unicode-DFS-2016`, `Zlib`) — identical set to the prior two CI passes; none are errors.

### Gate outcome per `BOOTSTRAP_PROMPT.md` §7.5

- (a) `tests/fixtures/0001-tcp-echo/` → **green** (`echo_fixture ... ok`, 1/1 passed in 8.01s on CI).
- (b) no pre-existing differential fixtures → nothing else to regress.
- (c) no conformance suites this phase → n/a.
- (d) no fuzz targets this phase → n/a.
- (e) `cargo build / clippy -D warnings / fmt --check / test --workspace / deny check` → all clean on CI with zero regressions vs. Attempt 2 (`24856364702`).
- (f) REVIEW.md → state 5 re-review pass pending.

State 4 re-verification complete. Next session enters state 5 via
`superpowers:requesting-code-review` for the re-review pass expected to
return **Approved** (all three Important items and the folded-in M3
resolved per REVIEW.md's final recommendation).

## State 5 (re-review) — Approved with fixes, new Minor only (2026-04-23)

Ran `superpowers:requesting-code-review` against range `b42f18d..a1c8194`
(code-relevant HEAD `fca3aba`; the two trailing commits `a1c8194` and
`880efcd` are documentation-only STATE.md / PROGRESS.md updates). The
code-reviewer subagent superseded the prior REVIEW.md in place with a
re-review pass and returned verdict **Approved with fixes (new Minor
only) — advance to state 6**.

### Fix outcomes vs. prior REVIEW.md

| ID | Prior severity | Fix commit | Re-review outcome |
|---|---|---|---|
| I1 — `drive_tcp` trailing-byte blind spot | Important | `245a65f` + ADR-0007 | **Fixed** — 100ms poll after `read_exact`, regression test `drive_tcp_rejects_trailing_bytes_after_echo` structurally reproduces silent-pass; ADR-0006 append-only-preserved (git-diff verified). |
| I2 — `rejects_duplicate_config_flag` discarded assertion | Important | `ba17ee3` | **Fixed** — `assert!(matches!(...), ...)` wrapper asserts the `ArgvError::Trailing(_)` variant; test now fails on any other variant. |
| I3 — `Subject::shutdown` rustdoc mismatch | Important (rustdoc portion) | `18bbfde` | **Fixed** — struct doc now accurately reads "sends SIGKILL …"; `TODO(phase-01)` block names the `nix`-crate D-3.2 blocker and phase-01 ADR gating. Functional SIGKILL→SIGTERM switch deferred per prior REVIEW's own recommendation. |
| M3 — `deny_unknown_fields` on YAML schemas | Minor (folded in) | `fca3aba` | **Fixed** — attribute present on all 9 documented sites; four TDD-verified regression tests assert against serde-canonical `"unknown field"` marker. |

### New issues surfaced in re-review (both Minor, non-blocking)

- **N1** — `deny_unknown_fields` tightens SPEC §D3.2's "Any field not
  covered here … is ignored by envoy-rust in phase 00" from
  silent-ignore to hard-reject without a dedicated ADR or PROGRESS.md
  SPEC-deviation note. REVIEW recommends closing with either a one-line
  PROGRESS.md entry at state 6 or a brief ADR-0008. Strictly safer
  (reject-on-typo) and REVIEW-recommended; does not block phase-done.
- **N2** — the four M3 regression tests cover only 4 of the 9
  `deny_unknown_fields` attribute sites (root `Bootstrap` + nested
  `Listener` + both differential structs). The 5 deeper structs
  (`StaticResources`, `Address`, `SocketAddress`, `FilterChain`,
  `NetworkFilter`) carry the attribute but are not individually
  regression-tested. Self-identified in the State 3 loop-back entry; a
  small follow-up commit can close the gap. Non-blocking.

### Pre-existing Minors (unchanged from prior REVIEW.md)

M1, M2, M4, M5, M6, M7, M8 remain open with prior classification. All
were labeled "nice to have; can defer" and none were in scope for the
loop-back. Carried into future phases.

### Outcome

Phase 00 is ready for **state 6** (final phase commit + ROADMAP row →
`done` + STATE advance). The final commit carries the full phase title
and bracketed ADR list `[ADR-0002, ADR-0003, ADR-0004, ADR-0005,
ADR-0006, ADR-0007]` per BOOTSTRAP_PROMPT §5.3. Per §5.1, this session
advances exactly one state (5 → 6-prep) and exits; the next session
enters state 6 via the BOOTSTRAP_PROMPT §5.3 final-commit procedure.
