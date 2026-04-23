# Phase 00 — Code Review

- **Reviewer:** superpowers:code-reviewer (subagent)
- **Range:** `b42f18d..e1771c3`
- **Date:** 2026-04-23
- **Lifecycle state:** 5 (review)
- **Verdict:** Approved with fixes

Phase 00 delivers a working TCP echo differential harness against pinned
upstream Envoy `v1.33.0` with all five gate commands green on CI (workflow run
`24856364702`). Spec conformance is strong, doctrine compliance is intact, and
the narrative (ADRs 0002–0006, PROGRESS.md) is rigorous. Two Important issues
need addressing before state 6 commit: one weakens the byte-exact differential
guarantee for an entire class of future bugs, and one is a broken unit-test
assertion. Remainder are Minor.

---

## Strengths

- **Narrative discipline is exemplary.** Every deviation is written down
  (`docs/envoy-rust/phases/00-bootstrap/PROGRESS.md`), each of the two in-phase
  course corrections (ADR-0005 for cargo-deny wrappers, ADR-0006 for the
  half-close fix) is grounded in concrete upstream evidence (cargo-deny 0.19.4
  behavior; `envoyproxy/envoy@v1.33.0`
  `source/common/network/connection_impl.cc` lines 83 and 698–715). The ADRs
  themselves enumerate rejected options with reasons, not just the chosen one.
- **ADR-0006's root-cause analysis** is the kind of investigation the project
  wants to see from the harness on every future divergence — file and line
  numbers at the pinned tag, confirmation of the envoy project's own client
  pattern in the echo integration test, and a D-3.2-grounded rejection of
  "patch upstream Envoy."
- **deny.toml `wrappers` lists** are mechanically correct against the actual
  `Cargo.lock` transitive graph: every parent in
  `crates.io` → `hyper-named-pipe`/`hyper-rustls`/`hyperlocal`/`bollard` is
  accounted for, matching the real edges in `Cargo.lock:572–635`. The denylist
  still fires on any non-bollard direct import (verified: removing a wrapper
  would immediately break CI).
- **`#![forbid(unsafe_code)]`** is present at both crate roots
  (`crates/envoy-bin/src/main.rs:1`, `tests/differential/src/lib.rs:1`). Submodules
  correctly do not repeat it; the crate-level forbid covers them.
- **D-3.2 permitted-foundations audit** of direct deps is clean: every direct
  dependency in both `Cargo.toml`s is on the MISSION.md §D-3.2 permitted list
  (`tokio`, `anyhow`, `serde`, `serde_yaml`, `tracing`, `tracing-subscriber`,
  `testcontainers`, plus `tempfile` which is stdlib-tier and used only for
  harness temp dirs).
- **Fixture layout is future-proof.** `tests/fixtures/0001-tcp-echo/` ships the
  five-file shape (`envoy.yaml`, `envoy-rust.yaml`, `inputs/payload.bin`,
  `expectations.yaml`, `README.md`) that the phase 02 TCP-proxy fixture will
  attach to without harness changes, per SPEC §6.5.
- **Concurrency hygiene in the subject's echo loop**
  (`crates/envoy-bin/src/echo.rs:20–63`): each accepted connection runs in a
  spawned `JoinSet` task, errors log via `tracing::warn!` and never panic the
  server, and drain timeout falls back to `set.shutdown().await` (hard abort)
  after 5s — that's the full graceful→forceful path the SPEC asks for in D3
  step 5.

---

## Issues

### Critical (must fix — blocks phase completion)

None. Nothing in the landed code violates a SPEC deliverable, breaks a
MISSION.md doctrine, silently corrupts the byte-exact differential
assertion for the shipped fixture, or introduces a correctness bug on the
hot path.

### Important (should fix before phase completion)

#### I1. `drive_tcp` cannot detect envoy-rust writing extra trailing bytes

- **File:** `tests/differential/src/lib.rs:109–119` (`drive_tcp`).
- **What's wrong.** ADR-0006 replaced the SPEC's half-close + `read_to_end`
  with `read_exact(payload.len())` + graceful `shutdown()` + drop. The fix is
  correct for matching Envoy v1.33.0's echo-filter contract, but it also
  weakens the differential assertion: `drive_tcp` now reads **exactly**
  `payload.len()` bytes and ignores anything after that.

  This creates a silent-pass class of bugs: if envoy-rust (now or in a future
  phase) writes `payload.len()` bytes of echo *plus additional trailing
  bytes* (e.g., a stray null terminator, a buffered write from a half-baked
  filter, a leaked handshake residue), `drive_tcp` never reads those extra
  bytes. `upstream_out` and `subject_out` are both `Vec<u8>` of length
  `payload.len()`, `upstream_out != subject_out` returns `false`, and the
  fixture is green.
- **Why it matters.** BEHAVIOR_CONTRACT.md row 2 is "Response body —
  byte-exact for deterministic handlers". "Byte-exact" includes "no extra
  bytes." Doctrine D-3.3 says the contract is the contract. The current
  harness quietly narrows the contract to "first N bytes match," where
  N is whatever the harness chose to read. That's spec drift.

  ADR-0006's "Consequences" section anticipates this shape — it proposes
  future fixtures declare an explicit `response_length` in
  `expectations.yaml` — but phase 00 ships the narrow read without any
  complementary check, so there is no red-fixture signal when this drift
  matters.
- **How to fix.** Any one of the following; recommend option (a):
  - (a) **Minimal, mechanical.** After `read_exact(payload.len())`, poll the
    socket with a tight deadline (e.g., 100ms) to confirm the peer closes
    or no further bytes arrive; bail if any extra bytes are read. Rough
    shape:
    ```rust
    let mut tail = [0u8; 64];
    match tokio::time::timeout(
        Duration::from_millis(100),
        stream.read(&mut tail),
    ).await {
        Ok(Ok(0)) | Err(_) => {} // peer closed or idle — expected
        Ok(Ok(n)) => bail!("{addr} sent {n} trailing bytes after echo"),
        Ok(Err(e)) => bail!("{addr} read error after echo: {e}"),
    }
    ```
    Document this in the `drive_tcp` rustdoc alongside the ADR-0006 reference.
  - (b) Pre-emptively land the `response_length` field in `Equivalence` with
    a default of `payload.len()` and wire `drive_tcp` to read that many
    bytes plus trailing-byte detection. This prepares the grammar ADR-0006
    anticipates without waiting for phase 02.
  - (c) Change `drive_tcp` to `write_all` → `read_to_end` with an
    independent idle-timeout (option C from ADR-0006), sized generously so
    it is not the steady-state flake source. Reject only if phase 00's two
    constraints are considered authoritative. Not recommended.

  Whichever option is chosen, update ADR-0006's "Consequences" and add a
  regression test in `tests/differential/src/lib.rs::tests` that mirrors the
  trailing-byte class — a server that writes `payload.len()` bytes and then
  `b"EXTRA"` — and asserts the harness fails the fixture.

#### I2. `rejects_duplicate_config_flag` test has no assertion

- **File:** `crates/envoy-bin/src/main.rs:171–175`.
- **What's wrong.**
  ```rust
  #[test]
  fn rejects_duplicate_config_flag() {
      let err = parse_argv(argv(&["-c", "/a", "-c", "/b"])).unwrap_err();
      matches!(err, ArgvError::Trailing(_));
  }
  ```
  `matches!(...)` evaluates to `bool` but is used as an expression
  statement. The result is silently discarded. The only check this test
  performs is that `parse_argv` returns `Err` at all — it does **not**
  assert the error is `Trailing(_)`. The test would pass even if the code
  returned `ArgvError::NoConfigFlag` or any other variant.
- **Why it matters.** `parse_argv` is the user-facing CLI contract. A test
  whose name reads "rejects duplicate config flag" but which actually tests
  "any error on duplicate config flag" is worse than no test — it gives
  false confidence that the parser handles the duplicate case explicitly.
  It also gets called out as a real-world example of the "don't trust
  test names, read assertions" class of bugs.
- **How to fix.**
  ```rust
  assert!(matches!(err, ArgvError::Trailing(_)), "got {err:?}");
  ```
  Then verify the test still passes. Optionally, tighten further by pattern
  matching on the path string (`Trailing(p) if p == "/b"`).

#### I3. `Subject::shutdown` uses SIGKILL, contradicting its doc

- **File:** `tests/differential/src/subject.rs:20–34`.
- **What's wrong.** The rustdoc says "sends SIGTERM and waits for clean
  exit," but the implementation calls `child.start_kill()` which on Unix
  sends `SIGKILL`. Net effect: the harness **never exercises**
  `envoy-bin`'s graceful-drain path (D3 step 5 of SPEC, implemented in
  `crates/envoy-bin/src/echo.rs:27–60`). That 5-second drain logic has unit
  tests in `echo::tests` but no integration coverage against the actual
  shipped binary.
- **Why it matters.** Two separate problems:
  1. The rustdoc misleads future readers — a stranger reading
     `Subject::shutdown` will assume they are testing the graceful-drain
     contract when they are not. This violates D-3.4.
  2. The end-to-end graceful-drain path across signal-handler +
     select-loop + JoinSet shutdown is only covered by unit tests that
     mock the shutdown future. SPEC §3 (implementation signpost 7) calls
     this out as *the* phase-00 reason the drain exists: "minimum the
     harness needs to terminate envoy-rust cleanly between test runs." The
     harness currently doesn't use it.
- **How to fix.** Either update the rustdoc to accurately say "SIGKILL; see
  commit-message rationale" *or* (preferred) switch to SIGTERM with a
  grace period and fall back to SIGKILL on timeout. The Unix-specific
  shape:
  ```rust
  // Graceful first: SIGTERM, then wait `budget`. If it does not exit,
  // escalate to SIGKILL. Keeps the harness exercising envoy-bin's drain
  // path (D3 step 5 of SPEC).
  use tokio::signal::unix as _; // for documentation only; we use libc via nix? avoid new deps
  if let Some(pid) = child.id() {
      // nix = permitted? No — not on D-3.2 list. Use libc via tokio::process?
      // Simplest path: tokio::process::Child does not expose a SIGTERM helper;
      // either add `nix` with a targeted ADR or send via std::process::Command
      // ("kill -TERM <pid>"). Both are ADR-scope additions. Alternative:
      // close stdin which envoy-bin can treat as a drain trigger — requires
      // envoy-bin change.
  }
  ```
  Because a clean fix introduces either a new direct dep (`nix`) or an
  `std::process::Command` child-invocation, this is plausibly a phase-01
  follow-up. At minimum for phase 00, **fix the rustdoc** and drop a TODO
  referencing the follow-up. Silent mismatch between doc and behavior is
  what violates D-3.4 here.

### Minor (nice-to-have; can defer)

#### M1. `parse_argv` accepts `-c --config-path` as path `"--config-path"`

- **File:** `crates/envoy-bin/src/main.rs:39–60`.
- **Observation.** `-c --config-path` is treated as a user-provided path
  whose literal value is the string `"--config-path"`. Slightly surprising
  UX; the hand-rolled parser does not recognize that the next token looks
  like a flag. Not a safety issue. Resolve when argv grows (phase 01
  or 08) and a real `clap`/`lexopt` ADR lands per SPEC §6.1. Add a test
  capturing the current behavior so the eventual ADR bump doesn't
  accidentally regress it.

#### M2. `Subject::shutdown` error message context is thin

- **File:** `tests/differential/src/subject.rs:29–33`. On
  `child.wait()` error or timeout, the bail messages don't include the PID
  or binary path. For CI triage that string is the only breadcrumb.
  Stringify the child id once at spawn time and include it in both
  `bail!()` arms.

#### M3. `expectations.yaml` schema is not `deny_unknown_fields`

- **File:** `tests/differential/src/lib.rs:22–37`. If a future fixture
  adds (say) `response_length: 18` under `equivalence:` by accident, serde
  silently drops it because `Expectations`/`Equivalence` don't set
  `#[serde(deny_unknown_fields)]`. Phase 00 ships only one fixture so this
  doesn't bite today, but ADR-0006's "Consequences" anticipates the
  grammar growing — set `deny_unknown_fields` now to get early-failure
  signal when unrecognized fields appear.

#### M4. `envoy-rust.yaml` and `envoy.yaml` diverge on bind address

- **Files:** `tests/fixtures/0001-tcp-echo/envoy-rust.yaml:6` binds
  `127.0.0.1`, `tests/fixtures/0001-tcp-echo/envoy.yaml:5` binds
  `0.0.0.0`. This is correct (envoy inside the container needs all-interface
  bind for Docker port-mapping; envoy-rust on the host wants loopback-only)
  but it is a difference between the paired configs — one that a reader of
  `README.md` would not anticipate. Add one sentence to
  `tests/fixtures/0001-tcp-echo/README.md` explaining the bind-address
  divergence, satisfying D-3.4 for the next reviewer.

#### M5. ENVOY_TARGET.md records a digest that is not enforced at pull

- **File:** `tests/differential/src/upstream.rs:37` uses
  `GenericImage::new(IMAGE_NAME, IMAGE_TAG)` — testcontainers pulls by tag
  only. The `sha256:56da5a…70c2` digest in
  `docs/envoy-rust/ENVOY_TARGET.md:9` is documentation, not a runtime
  pin. Under Docker's default pull policy a tag re-push on Docker Hub
  (unlikely for a GA Envoy release but not impossible) would silently bind
  tests to a different artifact. Consider one of: a startup digest-check
  step in the harness, a `testcontainers::GenericImage::with_digest`
  variant if one exists, or documenting in `ENVOY_TARGET.md` that the
  digest is an audit record rather than an enforcement mechanism. Not
  blocking; flagging for the first ENVOY_TARGET-refresh phase.

#### M6. `run_fixture` discards partial-read diagnostic info

- **File:** `tests/differential/src/lib.rs:166–171`. If `drive_tcp`
  returns an I/O error partway through `read_exact`, the error bubbles up
  with only the `.context("upstream envoy drive")` / `"envoy-rust drive"`
  tag — the number of bytes actually received is lost. On a future flake
  this is the most useful single number. Consider switching to
  `read_buf`-style accumulation or capturing `bytes_read` on the error
  path. Minor, and only material once flakes appear.

#### M7. `UpstreamProxy._container` field prefix-underscore is load-bearing

- **File:** `tests/differential/src/upstream.rs:21`. Prefixing a field
  with `_` conventionally means "don't warn about being unused," but in
  this case the field's drop is the shutdown mechanism (testcontainers
  `ContainerAsync` stops the container in its `Drop`). A reader could
  easily think the field is a vestige and remove it. Rename to
  `container` and annotate with `#[allow(dead_code)]` + a one-line
  comment: "Drop-on-scope-exit is the container shutdown path."

#### M8. `Cargo.toml` `resolver = "2"` with edition 2024

- **File:** `Cargo.toml:2`. Edition 2024 supports resolver `"3"`
  (which improves feature unification for workspaces with dev/bin targets).
  Not a defect on `"2"`, and the toolchain supports both; just noting
  that a future workspace-grooming phase could bump.

---

## Spec conformance matrix

| Deliverable | SPEC § | Landed? | Notes |
|---|---|---|---|
| D1 — Workspace members `crates/envoy-bin` + `tests/differential` | §2 D1 | Yes | `Cargo.toml:3–6`; `#![forbid(unsafe_code)]` present at both roots. |
| D1 — No HTTP / filter / `clap` deps in envoy-bin | §2 D1 | Yes | `crates/envoy-bin/Cargo.toml:12–18`: only permitted foundations. |
| D2 — `ENVOY_TARGET.md` fully populated | §2 D2 | Yes | Image, digest, release notes, proto tree SHA, xDS version all set. Digest resolved via Docker Hub API (local Docker IPv6 bug); PROGRESS.md Task 3 deviation. Acceptable. |
| D3.1 — argv `-c` / `--config-path` only, exit 2 on parse error | §2 D3.1 | Yes | `main.rs:39–60`; exit 2 via `ExitCode::from(2)` at `main.rs:86`. Minor weakness flagged in I2. |
| D3.2 — YAML parser for narrow Bootstrap shape | §2 D3.2 | Yes | `crates/envoy-bin/src/config.rs:1–77`; rejects non-echo filter with exit 1 via `run()` bubbling the `bail!`. |
| D3.3 — TCP accept loop + echo | §2 D3.3 | Yes | `crates/envoy-bin/src/echo.rs:20–76`. Uses explicit read/write loop per SPEC note. |
| D3.4 — `tracing-subscriber` + `ENVOY_RUST_LOG` | §2 D3.4 | Yes | `main.rs:91–96`. |
| D3.5 — SIGTERM/SIGINT + 5s drain | §2 D3.5 | Yes | `main.rs:117–125` + `echo.rs:52–61`. Drain path is not exercised by the harness in Issue I3, but the code itself conforms. |
| D3.6 — Single-listener only | §2 D3.6 | Yes | `config.rs:57–61` explicit rejection of >1 listener. |
| D4 — `run_fixture(Path) -> anyhow::Result<()>` | §2 D4 | Yes | `lib.rs:124`. Async (awaited) per the `tests/echo.rs` signature. |
| D4 step 1 — read `expectations.yaml` | §2 D4.1 | Yes | `lib.rs:125`. |
| D4 step 2 — testcontainers launch of pinned Envoy | §2 D4.2 | Yes | `upstream.rs:33–60`. Tag `v1.33.0` hardcoded at `upstream.rs:13`. Digest not enforced (M5). |
| D4 step 3 — envoy-rust spawn via `CARGO_BIN_EXE` | §2 D4.3 | Partial | Uses `locate_envoy_bin` fallback (`subject.rs:50–76`) because the `CARGO_BIN_EXE_envoy-bin` mechanism requires artifact-deps (unstable on 1.95.0). Functionally equivalent; documented in the source. Acceptable deviation, not called out as an ADR but the comment is clear enough. |
| D4 step 4 — 10s accept-ready budget, exp backoff | §2 D4.4 | Yes | `lib.rs:155–161` + `lib.rs:80–93`. |
| D4 step 5 — drive payload and assert byte-exact | §2 D4.5 | Superseded by ADR-0006 | `drive_tcp` uses `read_exact(payload.len())` not half-close + `read_to_end`. ADR-0006 documents and justifies. See Important I1 for the new concern this creates. |
| D4 step 6 — Drop-guarded cleanup | §2 D4.6 | Yes | `Subject::drop` at `subject.rs:37–44`, `UpstreamProxy` via `ContainerAsync::Drop` at `upstream.rs:20–21`. |
| D4 step 7 — Ok(()) on success | §2 D4.7 | Yes | `lib.rs:182`. |
| D4 — `tests/differential/tests/echo.rs` | §2 D4 | Yes | `tests/echo.rs:3–8`. Path via `concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/0001-tcp-echo")` equivalent. |
| D5 — five fixture files | §2 D5 | Yes | All present. Payload is 18 bytes (verified). |
| D6 — CI workflow, five gate commands, `ubuntu-latest` | §2 D6 | Yes | `.github/workflows/ci.yml:17–52`; all five commands wired. `concurrency.cancel-in-progress` added beyond SPEC — reasonable. |
| D7 — ADR-0002, ADR-0003, ADR-0004 | §2 D7 | Yes | `DECISIONS.md:40–89`. Plus ADR-0005 (cargo-deny correction) and ADR-0006 (harness fix) landed in-phase per D-3.5. |

No spec deliverable is missing from the landed diff. Two deliverables (D4
step 3, D4 step 5) are reached via documented alternatives; both alternatives
are sound.

---

## ADR assessment

- **ADR-0002 (GitHub Actions).** Sound. Alternatives (Buildkite, Drone,
  GitLab CI) are correctly characterized. Docker-in-docker on
  `ubuntu-latest` is genuinely the load-bearing property. Reflected in
  `.github/workflows/ci.yml`.

- **ADR-0003 (Rust edition 2024).** Sound. Toolchain pin 1.95.0 (verified at
  `rust-toolchain.toml:2`) strictly dominates edition-2024 stabilization
  (1.85). Both crates declare `edition = "2024"`. No compatibility cost.

- **ADR-0004 (Envoy `v1.33.0`).** Sound. Digest, proto tree commit, release
  notes URL are recorded in `ENVOY_TARGET.md`. Caveat: the digest is
  recorded but not enforced at pull time (M5). Harness hardcodes the tag at
  `upstream.rs:13`, matching the pin.

- **ADR-0005 (cargo-deny wrappers + advisory ignores).** Sound and well
  scoped. The wrappers list in `deny.toml:62–79` is mechanically correct
  against `Cargo.lock` (verified the enumerated parents —
  `bollard`, `hyper-named-pipe`, `hyper-rustls`, `hyper-util`, `hyperlocal`
  — are the actual direct-depender edges for each denied crate). The two
  advisory ignores name the exact advisory IDs and justify them with
  "dev-only testcontainers chain; no safe upgrade." If `testcontainers`
  moves to `astral-tokio-tar`, the advisory entries can be removed; if any
  non-bollard caller acquires a hyper dep, the ban fires. Scope creep is
  justified: without ADR-0005, CI couldn't go green, so the ADR is a
  phase-done blocker that landed correctly under D-3.5.

- **ADR-0006 (`drive_tcp` rewrite).** Rationale is correct and backed by
  evidence at the pinned Envoy tag. Decision (option A) is the minimum
  diff that restores the green build without reaching for
  cluster-manager scaffolding that phase 00 defers. **However**, ADR-0006
  does not address the trailing-byte blindspot the new `drive_tcp`
  introduces (see Important I1). The "Consequences" section anticipates
  future fixtures needing explicit `response_length`, but phase 00 ships
  the narrow-read harness with no complementary check or regression test
  for the trailing-byte class. Recommend amending ADR-0006's
  "Consequences" with the trailing-byte mitigation chosen for I1, or
  landing a follow-up ADR-0007 alongside the I1 fix.

---

## Recommendations

- **Process:** the PROGRESS.md "State 4" section is the single best artifact
  this phase produced — the combination of CI run URLs, quoted error
  output, and C++-source-line citations is exactly the D-3.4 shape. Make
  this a template for future verification entries.

- **Regression-test discipline going forward:** every harness behavior
  decision ADR (of which ADR-0006 is the first) should land with a unit
  test that *would have caught the symptom* if the pre-ADR code had
  shipped. `drive_tcp_round_trips_without_half_close` does this well. I1
  suggests the same discipline for the trailing-byte class — once fixed,
  ship a test that reproduces the silent-pass.

- **Future phase (01 or 02):** consider a small follow-up that replaces
  `Subject::shutdown`'s SIGKILL with a real SIGTERM + drain-and-escalate,
  so the harness exercises envoy-bin's graceful-drain path end-to-end.
  That likely requires adopting `nix` (not on D-3.2) under its own ADR.
  No phase-00 action beyond the rustdoc fix in I3.

- **`deny_unknown_fields`** on every YAML-parsed struct in
  `tests/differential/src/lib.rs` (Equivalence/Expectations) and in
  `crates/envoy-bin/src/config.rs`. Low-cost, catches a real class of
  bugs early, and costs nothing in phase 00 because neither schema has
  optional extension fields yet.

---

## Final verdict

Loop back to lifecycle state 3 with the two Important items (I1: `drive_tcp`
trailing-byte blindspot; I2: broken `rejects_duplicate_config_flag` assertion)
and the rustdoc portion of I3 (Subject::shutdown rustdoc mismatch; the
functional SIGKILL → SIGTERM switch can be a follow-up phase). Minors are
defer-or-batch. Once the two Importants are fixed and their regression tests
land, re-run state 4 verification (CI green) and re-enter state 5 for a final
pass; expectation is Approved on the next round. The phase is a commit or two
away from state 6.
