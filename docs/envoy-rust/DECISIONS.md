# envoy-rust Architecture Decision Records

> Append-only log. Landed ADRs are never edited; they are superseded by a new
> ADR that explicitly names the superseded ADR number. Format per D-3.5 of
> `MISSION.md`.

Each ADR uses the structure:

```
## ADR-NNNN: <title>

- Date: YYYY-MM-DD
- Status: proposed | accepted | superseded-by ADR-MMMM
- Context: <why the decision was needed>
- Options considered: <list, briefly>
- Decision: <the choice>
- Rationale: <why this choice over the others>
- Consequences: <what this implies, including follow-up ADRs if any>
```

---

## ADR-0001: Bootstrap prompt version pin

- Date: 2026-04-23
- Status: accepted
- Context: The project is driven end-to-end by `BOOTSTRAP_PROMPT.md`. Every later ADR and every phase artifact implicitly assumes a specific version of that prompt (wording of doctrine rules, numbering of sections, contents of the seeded roadmap). A future edit to the prompt must therefore be a deliberate event, not a silent drift.
- Options considered:
  - Pin to the SHA of the commit that introduced `BOOTSTRAP_PROMPT.md` as this ADR does.
  - Do not pin; rely on the current working-tree contents at bootstrap time.
  - Embed the prompt's contents inline in `MISSION.md`.
- Decision: Pin the prompt to the git SHA of the commit that last modified `BOOTSTRAP_PROMPT.md` at bootstrap time: `3ee76395238123f4b9214c8998907ee6c830d3e2`. `MISSION.md` also contains a verbatim copy of §§2 and 3 of the prompt at that SHA, so the mission and doctrine survive in-tree without rereading the prompt.
- Rationale: An ADR is the project's permanent, append-only record; referencing a SHA lets every subsequent artifact be traced to an exact prompt version. A later prompt edit becomes ADR-MMMM ("supersede the prompt pin to SHA X") and the change log is unambiguous.
- Consequences:
  - All subsequent phases assume the prompt at this SHA. Any behavior described by the prompt that this ADR log or a later ADR does not supersede is in force.
  - If `BOOTSTRAP_PROMPT.md` is edited, a new ADR must be landed referencing the new SHA and any doctrine deltas, and `MISSION.md` must be updated in the same commit.

---

## ADR-0002: GitHub Actions as the CI provider

- Date: 2026-04-23
- Status: accepted
- Context: Phase 00 needs a CI pipeline that runs the full green-build gate (`cargo build`, `cargo clippy`, `cargo fmt --check`, `cargo test --workspace`, `cargo deny check`) on every push and PR. Doctrine D-3.6 makes the green-build gate non-negotiable, so the CI choice is a foundation for every later phase.
- Options considered:
  - **GitHub Actions** — free for public OSS repos, first-class Rust toolchain action (`dtolnay/rust-toolchain`), mature cargo cache action (`Swatinem/rust-cache`), and docker-in-docker on `ubuntu-latest` runners (required for `testcontainers` → upstream Envoy).
  - **Buildkite** — requires self-hosted runners; upfront infra cost that does not pay back for a single-maintainer OSS project.
  - **Drone** — requires a hosted controller; comparable feature set to GH Actions without the ecosystem.
  - **GitLab CI** — would require mirroring the repo to GitLab; divergent source-of-truth risk.
- Decision: GitHub Actions.
- Rationale: zero infra cost, docker-in-docker on `ubuntu-latest` is exactly what the differential harness needs, and the ecosystem actions (`dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `taiki-e/install-action@cargo-deny`) cover every step of the phase-done gate without bespoke scripting.
- Consequences:
  - The single CI job lives at `.github/workflows/ci.yml`, running on `ubuntu-latest`. macOS and Windows runners are explicitly out of scope (Envoy's production posture is Linux; see SPEC §4).
  - A future "CI scale-out" phase (adding matrix runs, release workflows, etc.) lands as its own phase with its own ADR. The only artifact this ADR commits to is a single linting/build/test job.

---

## ADR-0003: Rust edition 2024 for all workspace crates

- Date: 2026-04-23
- Status: accepted
- Context: Every crate introduced starting in phase 00 declares an `edition = "…"` in its `Cargo.toml`. We must commit to a single edition at bootstrap to avoid a mass edition-bump phase later, which would touch every crate root's macro behavior, Pin/disjoint-borrow rules, and `expect`/`assume` forward-compatibility semantics.
- Options considered:
  - **Edition 2024** — stabilized in rustc 1.85. Our toolchain pin is 1.95.0 (D-3.9), well past the stabilization point. Tightens closure capture rules, `async fn` in traits, Pin semantics.
  - **Edition 2021** — the still-most-popular edition; safe and boring.
  - **Edition 2018** — legacy; picks up none of the last six years of ergonomics work.
- Decision: Edition 2024 for every workspace crate.
- Rationale: since the toolchain pin (1.95.0) strictly dominates the 2024 stabilization threshold (1.85), there is no compatibility cost. Future edition bumps become the next-edition problem, not the "we-already-skipped-three-editions" problem.
- Consequences:
  - Every new `Cargo.toml` this phase and later specifies `edition = "2024"`. The phase-00 workspace includes this in both `crates/envoy-bin/Cargo.toml` and `tests/differential/Cargo.toml`.
  - A future toolchain-bump phase (per D-3.9) must verify edition-2024 behavior on the new toolchain before landing.

---

## ADR-0004: Upstream Envoy pin — `envoyproxy/envoy:v1.33.0`

- Date: 2026-04-23
- Status: accepted
- Context: D-3.7 requires a pinned upstream Envoy image. `docs/envoy-rust/ENVOY_TARGET.md` is the on-disk record of the pin; this ADR is its decision trail.
- Options considered:
  - **`envoyproxy/envoy:v1.33.0`** — the newest stable release at bootstrap time (resolved during execution; confirmed present on Docker Hub and on `refs/tags/v1.33.0` upstream).
  - **`envoyproxy/envoy:v1.32.x`** — previous LTS line; more conservative; older xDS surface.
  - **`envoyproxy/envoy:main`** — moving target; incompatible with the discipline that pins are deterministic per D-3.7.
- Decision: `envoyproxy/envoy:v1.33.0`.
- Rationale: starting on the newest GA release is cheaper than a refresh phase 3 months later. A future refresh ADR will supersede this one with the then-current LTS, which remains normal operation per D-3.7.
- Consequences:
  - The exact sha256 digest and proto-tree commit SHA are recorded in `docs/envoy-rust/ENVOY_TARGET.md`, landed in the same commit as this ADR.
  - The differential harness (Task 11) hard-codes this image tag + digest when launching the upstream container via `testcontainers`.
  - Every fixture's `envoy.yaml` is written against the `v1.33.0` bootstrap schema.

---

## ADR-0005: cargo-deny exemptions for the testcontainers transitive chain

- Date: 2026-04-23
- Status: accepted
- Context: Phase 00 Task 4 landed the workspace skeleton, which pulls in `testcontainers = "0.23"` through `tests/differential`. `testcontainers` transitively pulls `bollard`, which in turn pulls `hyper`, `hyper-util`, `tower-service` (all three on the phase-00 `deny.toml` banned-direct-dependency list per doctrine D-3.2), plus `tokio-tar 0.3.1` and `rustls-pemfile 2.2.0` (both flagged by RustSec advisories). PLAN.md Task 4 prescribed `skip-tree = [{ name = "testcontainers" }]` to cover the hyper/tower leak; empirical testing against cargo-deny 0.19.4 shows `skip-tree` exempts subtrees from the `multiple-versions` check only — it does NOT exempt them from the `[bans] deny` list. A different mechanism is needed.
- Options considered:
  - **`wrappers` on each affected deny entry** — cargo-deny's documented mechanism for allowing a banned crate when it is pulled in by an enumerated allow-list of parent crates. Precise; matches the doctrine intent ("direct dep forbidden, transitive via permitted foundation allowed").
  - **Remove `hyper`, `hyper-util`, `tower-service` from the deny list entirely and enforce at workspace-import level** — the deny.toml comment at line 42–44 already prescribes this for the `tonic` case. Simpler, but loosens the doctrine's mechanical guarantee until workspace-level import lints are configured (no phase currently plans them).
  - **Upgrade `testcontainers` to `0.27.x`** — newer, but still depends on `bollard` with the same transitive graph; does not resolve either the bans or the advisories. Also a plan deviation with no benefit.
  - **Replace `testcontainers` with a bespoke Docker-daemon shim** — weeks of work; loses the maintained bollard-based runner. Out of scope for phase 00.
- Decision:
  1. Use `wrappers` on `hyper`, `hyper-util`, and `tower-service` in `deny.toml`, enumerating the bollard-chain parents that legitimately pull them (`bollard`, `hyper-named-pipe`, `hyper-rustls`, `hyper-util`, `hyperlocal`). The `skip-tree = [{ name = "testcontainers" }]` entry stays in place as a no-op guard for the `multiple-versions` check; it is annotated as such.
  2. Add two `[advisories].ignore` entries covering `RUSTSEC-2025-0111` (tokio-tar 0.3.1 PAX header file-smuggling vulnerability, CVE-2025-62518) and `RUSTSEC-2025-0134` (rustls-pemfile unmaintained), each annotated with the transitive path and the reason the exemption is safe.
- Rationale: `wrappers` is the cargo-deny-native mechanism for exactly this situation and keeps the blast radius narrow — only the bollard chain is exempted, non-testcontainers paths still fire the ban. Both RustSec advisories are on dev-test-harness-only dependencies with no safe upgrade currently available (both archived/unmaintained upstream; `testcontainers` upstream tracks the tokio-tar replacement at https://github.com/testcontainers/testcontainers-rs/issues). No production code depends on these crates (envoy-rust has no production code yet, and the differential harness is not shipped). The exemption is time-boxed: if `testcontainers` 0.28+ switches to `astral-tokio-tar` / `rustls-pki-types`'s pem parser, a future phase supersedes this ADR and removes the ignores.
- Consequences:
  - `deny.toml`'s `[bans] deny = [...]` is the authoritative direct-dep ban list. Its `wrappers` arms are the machine-checked allow-list for transitive chains. Future phases that add `tonic` (bringing its own `hyper`/`tower` leak) must extend `wrappers` with `tonic`-chain parents and update this ADR or supersede it with a follow-up. Transitive exemptions are never silent.
  - A future phase must revisit the two `[advisories].ignore` entries if/when `testcontainers` ships a bollard-free or bollard-updated release. A scheduled dependency audit is a good trigger; see the refresh-pin discipline in ADR-0004 for precedent.
  - This ADR documents that PLAN.md Task 4 Step 6's prescription was mechanically wrong (skip-tree does not exempt bans); subsequent phases must treat the `deny.toml` shape in this ADR as authoritative over the plan text.

---

## ADR-0006: Harness `drive_tcp` uses `read_exact(payload.len())` instead of half-close + `read_to_end`

- Date: 2026-04-23
- Status: accepted
- Context: Phase 00 SPEC §D4 point 5 prescribed that the differential harness's per-proxy TCP driver would `write_all(payload)` → `shutdown()` (half-close the write side) → `read_to_end()`. The first CI execution of the `echo_fixture` acceptance test on `ubuntu-latest` (workflow run `24855427288`, commit `2d81b53`) returned `upstream: []` (zero bytes) and `subject: "hello, envoy-rust\n"` (18 bytes). Root cause was traced to `envoyproxy/envoy@v1.33.0` in `source/common/network/connection_impl.cc`:
  - `ConnectionImpl::enable_half_close_` defaults to `false` (line 83 of that file at tag `v1.33.0`).
  - In `onReadReady` (lines 698–715), when `doRead` returns `end_stream_read_ = true` (client FIN) and `enable_half_close_` is false, the code path sets `result.action_ = PostIoAction::Close` and then calls `closeSocket(ConnectionEvent::RemoteClose)` immediately after dispatching `onRead` to the filter manager.
  - The `envoy.filters.network.echo` filter (`source/extensions/filters/network/echo/echo.cc`) echoes by calling `read_callbacks_->connection().write(data, end_stream)`. That write is queued in the connection's write buffer and flushed on a later event-loop iteration — but the `closeSocket(RemoteClose)` above executes in the same iteration and drops the pending write buffer.
  - There is no listener-level YAML surface to enable half-close semantics in v1.33.0. The `Listener` proto at that tag has no `enable_half_close` field (`enableHalfClose()` is a C++ `Connection` method only); the only network-filter with a YAML `enable_half_close` toggle is `envoy.filters.network.tcp_proxy`, which phase 00 does not use.
  - Envoy's own integration test for the echo filter (`test/extensions/filters/network/echo/echo_integration_test.cc`) confirms the intended client pattern: send data → wait for the data callback → `conn.close(ConnectionCloseType::FlushWrite)`. It does not half-close.
- Options considered:
  - **(A) Change `drive_tcp` to `write_all` → `read_exact(payload.len())` → graceful close.** Matches the echo filter's deterministic 1:1 byte-count contract, which is what phase 00's only fixture asserts (`response_body: byte_exact`). Neither fixture YAML nor the `envoy-bin` subject changes.
  - **(B) Replace the echo fixture with a `tcp_proxy`-based bootstrap that sets `enable_half_close: true` on both the TCP-proxy filter and a loopback upstream cluster.** Requires cluster-manager + upstream-cluster scaffolding that phase 00 explicitly defers to phase 02 per SPEC §4. Out of scope.
  - **(C) Keep half-close and switch the harness to a read-with-idle-timeout loop** (read until no bytes for N ms). Works around the symptom but does not match the echo filter's contract, and produces a new flake surface on slow CI.
  - **(D) Patch upstream Envoy to enable half-close on plain listeners.** Prohibited by doctrine D-3.2 (no FFI/patching of upstream) and by the differential mission (the contract is the contract — match Envoy, do not fork it).
- Decision: option (A). `drive_tcp` now writes the payload, reads exactly `payload.len()` bytes, then shuts down the write side and drops the stream.
- Rationale: the fix is mechanical, keeps the harness entrypoint reusable for future phases, touches only `tests/differential/src/lib.rs`, and restores the byte-exact differential equivalence without reaching for filters or clusters that phase 00 doesn't ship. The response-length assumption (`payload.len()`) is specific to the echo filter's 1:1 contract; when phase 02's TCP proxy fixture lands, it will either reuse the same 1:1 property (straight TCP proxy to an echoing upstream) or extend `expectations.yaml` with an explicit `response_length` declaration so `drive_tcp` can be re-used without another ADR.
- Consequences:
  - SPEC §D4 point 5's specific wording ("half-closes the write side, reads to EOF") is superseded by this ADR for the `drive_tcp` helper. The SPEC itself is an immutable historical artifact; the harness implementation follows this ADR.
  - `envoy-bin`'s echo loop (D3 step 3) continues to honor client half-close — it is the correct long-run behavior for Envoy-parity proxies, and the new `drive_tcp` exercises it implicitly (graceful `shutdown()` still fires before drop). No change to `envoy-bin`.
  - Neither fixture YAML changes. `tests/fixtures/0001-tcp-echo/envoy.yaml` remains the minimal echo-filter listener.
  - Phase 00's final-commit message bracketed ADR list extends from `[ADR-0002, ADR-0003, ADR-0004, ADR-0005]` to `[ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006]`.
  - If a future phase needs genuine half-close semantics in the harness (e.g. to test a filter that expects client FIN as a trigger), that phase lands a new ADR — likely selecting option (B) since by then the cluster manager will exist.

---

## ADR-0007: `drive_tcp` trailing-byte poll closes the ADR-0006 blind spot

- Date: 2026-04-23
- Status: accepted
- Context: ADR-0006 replaced the SPEC's half-close + `read_to_end` in `tests/differential/src/lib.rs::drive_tcp` with `write_all` → `read_exact(payload.len())` → graceful `shutdown()` + drop, matching Envoy v1.33.0's echo-filter contract. Phase 00 lifecycle state 5 review (REVIEW.md I1) identified a silent-pass class of bugs this introduced: `read_exact(payload.len())` ignores any bytes the peer writes after the first `payload.len()` echoed bytes. A subject that writes the echoed payload plus stray trailing bytes (a null terminator, a buffered write from a half-baked filter, a leaked handshake residue) returns a `Vec<u8>` of the correct length and the harness compares it green, even though BEHAVIOR_CONTRACT.md row 2 demands byte-exact equivalence including "no extra bytes." Doctrine D-3.3 says the contract is the contract; the harness as landed quietly narrowed the contract to "first N bytes match."
- Options considered:
  - **(a) Minimal trailing-byte poll.** After `read_exact(payload.len())`, do one short-deadline `read()` (e.g., 100ms via `tokio::time::timeout`) and bail if any bytes arrive. A compliant peer closes (`Ok(0)`) or stays silent until the deadline (timeout `Err`). No fixture or YAML changes required. Document in `drive_tcp`'s rustdoc alongside the ADR-0006 reference.
  - **(b) Introduce `response_length` in `Equivalence`.** Add an optional `response_length` field to `expectations.yaml` with a default of `payload.len()`, and wire `drive_tcp` to read that many bytes plus a trailing-byte poll. Prepares the grammar ADR-0006 anticipates for phase 02, but requires a schema bump with no phase-00 consumer.
  - **(c) Revert to `read_to_end` with an independent idle-timeout.** Superficially solves the byte-exactness problem, but is exactly option (C) that ADR-0006 rejected: it does not match the echo filter's deterministic contract and introduces a new flake surface on slow CI.
- Decision: option (a). `drive_tcp` gains a 100ms trailing-byte poll via `tokio::time::timeout(..., stream.read(&mut tail))` between `read_exact` and the graceful `shutdown()`. Any non-zero read bails with a descriptive error; `Ok(0)` (peer closed) and a timeout `Err` (peer is silent) are the two allowed outcomes.
- Rationale: option (a) is the narrowest fix that restores the byte-exact contract — it is mechanical, touches only `drive_tcp` and its rustdoc, needs no new dependency (tokio's `time::timeout` is already in scope), and composes cleanly with any future `response_length` grammar: when phase 02 introduces variable-length responses, the poll remains correct and only the `read_exact` length becomes data-driven. Option (b) is correct eventually but is schema surface with no phase-00 caller; per D-3.4 (ship the smallest thing that settles the contract) it belongs in the phase that first needs it. Option (c) was already refuted by ADR-0006.
- Consequences:
  - `drive_tcp` now fails any fixture where either proxy writes more than `payload.len()` bytes. The error message includes the offending address and the number of trailing bytes. This is a behavior change visible only when the subject misbehaves.
  - The 100ms poll adds a fixed per-connection latency of ~100ms in the idle case, because `drive_tcp` waits for the deadline when the peer is silent rather than closing. For phase 00's single-fixture workflow (two `drive_tcp` calls per fixture run) this is ~200ms, which is negligible relative to the upstream-Envoy container's multi-second start budget. A future phase with many fixtures or latency-sensitive CI may revisit this number or replace the timeout with a peek-based probe; that is a new ADR, not an edit to this one.
  - A new regression test `drive_tcp_rejects_trailing_bytes_after_echo` in `tests/differential/src/lib.rs::tests` proves the silent-pass is closed: a server that echoes `payload.len()` bytes and then writes `b"EXTRA"` now causes `drive_tcp` to return `Err` carrying `"trailing bytes"`. The companion `drive_tcp_round_trips_without_half_close` test still passes, proving the happy path is unchanged.
  - ADR-0006 remains in force for its original contribution (the `read_exact` strategy and the Envoy v1.33.0 analysis). This ADR does not supersede ADR-0006; it lands on top of it as the mitigation for the blind spot ADR-0006's "Consequences" acknowledged but did not close. Per the append-only doctrine (DECISIONS.md preamble, MISSION.md D-3.5) ADR-0006 is not edited.
  - Phase 00's final-commit bracketed ADR list extends from `[ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006]` to `[ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006, ADR-0007]`.

---

## ADR-0008: Extract `envoy-config` as a library crate

- Date: 2026-04-24
- Status: accepted
- Context: Phase 00's parser lives inline at `crates/envoy-bin/src/config.rs`. Phase 01 lands the first coverage-guided fuzz target (SPEC §D2, scheduled by phase-00 SPEC §6.2). `cargo-fuzz` requires its target crate to be a *library* — `envoy-bin` is a binary-only crate, so the parser must move. The parser is also the long-horizon seam every phase 02–08 plus the xDS family (§9) extends; isolating it behind a crate boundary now avoids a mass reshuffle later.
- Options considered:
  - Keep the parser inline in `envoy-bin` and fuzz a bin-sibling trampoline crate that `pub use`s `envoy_bin::config::parse_bootstrap`. Technically works; pollutes `envoy-bin` with a `[lib]` target carried only for fuzzing.
  - Extract `envoy-config` only; defer a future `envoy-admin` extraction to phase 08 when a real router exists.
  - Extract `envoy-config` + `envoy-admin` simultaneously. Over-scoped: there is no phase-01 admin router worth a crate boundary yet.
- Decision: extract `envoy-config` now. `envoy-admin` extraction stays deferred to phase 08.
- Rationale: the fuzz target drives the requirement today; the cross-phase parser seam is a standing concern either way. A single clean move now is cheaper than inline + trampoline now + rework in phase 02.
- Consequences:
  - `crates/envoy-config/` joins `[workspace] members`. `crates/envoy-bin/src/config.rs` is deleted; `envoy-bin` gains `envoy-config = { path = "../envoy-config" }` and drops its direct `serde`/`serde_yaml` deps.
  - The struct relocation lands verbatim aside from one SPEC-mandated relaxation: `listeners.len() ∈ {0, 1}` (admin-only configs are now valid) and a new `NoRuntime` error fires when both `admin` and `listeners` are empty.
  - Future parser surface (typed_config envelopes in phase 02, HCM in phase 04, xDS in §9) lands inside `envoy-config` rather than accreting in `envoy-bin`.

---

## ADR-0009: Permit `cargo-fuzz` and `libfuzzer-sys` as fuzz-only dev tooling

- Date: 2026-04-24
- Status: accepted
- Context: Doctrine D-3.2's permitted-foundations list enumerates runtime crates; it does not cover fuzzing tooling. Phase 00 SPEC §6.2 scheduled the first fuzz target for phase 01 (`parse_bootstrap`), and the project will add more targets phase-over-phase (HTTP/1.1 tokenizer phase 04, HTTP/2 codec phase 05, protobuf family, etc.). A single authoritative choice of fuzzer avoids per-phase re-litigation.
- Options considered:
  - `cargo-fuzz` + `libfuzzer-sys`. Most ergonomic Rust integration; ships as a cargo subcommand; uses libFuzzer under the hood; SanitizerCoverage-instrumented builds via `-Z` flags on nightly rustc.
  - `afl.rs`. Solid, but requires an out-of-tree setup flow and weaker integration with cargo workspaces.
  - `honggfuzz-rs`. Smaller community; fewer batteries-included examples for our use case.
  - `proptest` only. Property-based, not coverage-guided; valuable but a different tool (belongs in unit tests, not in the fuzz pipeline).
- Decision: `cargo-fuzz` + `libfuzzer-sys` as fuzz-only dev tooling; never a transitive dep of `envoy-bin` or `tests/differential`.
- Rationale: the one tool that reduces "new fuzz target" to "new `fuzz_target!(...)` file + one CI line" in a cargo-native workflow. The alternatives ask for per-fuzzer scaffolding we'd rebuild every phase.
- Consequences:
  - The fuzz subcrate (`crates/envoy-config/fuzz/`) is workspace-excluded per ADR-0010 to keep `libfuzzer-sys` out of the main build's dependency graph.
  - Future fuzz targets (HTTP/1.1, H2, protobuf, xDS) reuse this decision; no new ADR per target.
  - If `cargo deny check` flags a transitive license on `libfuzzer-sys` (historically Apache-2.0 + MIT + NCSA on LLVM runtime) that is not on the allow-list, the mitigation lands as a new ADR (likely ADR-0012) during execution — this ADR establishes the tooling choice, not the license surface.

---

## ADR-0010: Nightly Rust toolchain for fuzz-only invocation; stable pin untouched

- Date: 2026-04-24
- Status: accepted
- Context: `cargo-fuzz` requires nightly rustc for `-Zsanitizer=address` / `-Zcoverage-options` flags needed by libFuzzer's SanitizerCoverage instrumentation. D-3.9 pins `rust-toolchain.toml` at the repo root to stable `1.95.0`, and "upgrading the pin is its own phase." The fuzz job must not flip every `cargo build` and `cargo test` in the repo onto nightly.
- Options considered:
  - Bump `rust-toolchain.toml` to nightly. Rejected — breaks D-3.9 for the mainline build and every phase's stable CI gate.
  - Add a nested `rust-toolchain.toml` under `crates/envoy-config/fuzz/`. Rejected — that crate is workspace-excluded (ADR-0008 consequence); cargo toolchain-override semantics across workspace boundaries are surprising and brittle.
  - Use `cargo-bolero` or similar stable-wrappers. Rejected — libFuzzer-backed runs still require nightly for sanitizer coverage.
  - Invoke `cargo +nightly fuzz run ...` explicitly in a dedicated CI job; stable pin untouched.
- Decision: explicit `+nightly` invocation in a dedicated `fuzz` CI job (SPEC §D8). Developers running fuzz locally install nightly with `rustup toolchain install nightly` and run `cargo +nightly fuzz run parse_bootstrap` from `crates/envoy-config/`.
- Rationale: the cost is one `+nightly` prefix in CI and one README line; the benefit is that D-3.9 remains mechanically enforced for every mainline path and every developer-facing build stays on the pinned stable.
- Consequences:
  - `.github/workflows/ci.yml` gains a second job `fuzz` running `dtolnay/rust-toolchain@nightly` with `rust-src` (SanitizerCoverage requires libstd recompiles), `cargo install cargo-fuzz --locked`, and `cargo fuzz run parse_bootstrap -- -max_total_time=30` from `crates/envoy-config/`. A 30 s budget matches SKILL_ROUTING.md state 4's "short-budget CI run."
  - Developer docs (future) may add a one-line "install nightly + cargo-fuzz" block; not required in phase 01.
  - A future "scheduled long-budget nightly fuzz" becomes its own phase with its own ADR.

---

## ADR-0011: Phase 01 defers response-header equivalence to phase 04

- Date: 2026-04-24
- Status: accepted
- Context: `BEHAVIOR_CONTRACT.md`'s `Header allow-list` subsection is marked "populated starting phase 04" at bootstrap. Phase 01 is the first phase that returns an HTTP response (admin `/ready`). ROADMAP phase-01 summary — "config parses; admin `/ready` behaves like Envoy" — is silent on whether header equivalence is in scope. The fixture-0002 harness configuration therefore needs an explicit decision.
- Options considered:
  - Populate a phase-01 stub header allow-list (`server`, `date`) and assert header equivalence for everything else. Requires us to also allow-list `content-length` (identical values but still a comparison surface), `content-type`, `cache-control`, `x-content-type-options`, and `connection` — each with its own justification. Premature: phase 04 is where the HTTP/1.1 data-plane surfaces all the response-header questions worth answering.
  - Assert full headers with a fresh allow-list. ADR-heavy; high review cost; diverges the two proxies on `server:` without a clean path forward before phase 04 lands the HCM response-header pipeline.
  - Assert response status + body only for phase 01. The differential harness ignores headers. envoy-rust still emits a reasonable Envoy-shaped header block on every admin response for forward-compat (content-type, content-length, cache-control, x-content-type-options, server, date, connection) — the divergence on `server:` is tolerated until phase 04 populates the allow-list.
- Decision: option 3 — status + body equivalence only for phase 01.
- Rationale: the ROADMAP's "`/ready` behaves like Envoy" phrasing is behavioral, and status + body already pin the behavior (`200 OK` + `LIVE\n` vs. `404 Not Found` + the invalid-path message). Header equivalence is a framing concern that belongs in the phase that owns the framing (phase 04).
- Consequences:
  - `tests/fixtures/0002-static-admin-ready/expectations.yaml` only specifies `response_status: exact` + `response_body: byte_exact`. The harness's `drive_http_get` captures headers for debug tracing but they play no role in the equivalence diff.
  - `envoy-rust`'s admin response uses `server: envoy-rust` (deliberately not `envoy`). When phase 04 populates the header allow-list, `server` lands on it.
  - `BEHAVIOR_CONTRACT.md` is **not** edited in phase 01. All currently-empty subsections (`Header allow-list`, `Stat-name mapping`, `Access log field mapping`, `xDS wire state machine`, `Timing tolerances`) stay empty.

---

## ADR-0012: Nested nightly pin in the fuzz subcrate (narrowly supersedes ADR-0010)

- Date: 2026-04-24
- Status: accepted; narrowly supersedes ADR-0010 on the single sub-point of whether `crates/envoy-config/fuzz/rust-toolchain.toml` may exist. ADR-0010's main decision — the workspace-root `rust-toolchain.toml` stays on stable, and CI overrides it via an explicit `cargo +nightly fuzz run` invocation — remains in force.
- Context: phase 01's state-4 phase-done gate surfaced a real-world ergonomics gap that ADR-0010 had foreseen but not mitigated. When a developer runs `cd crates/envoy-config && cargo fuzz run parse_bootstrap` from an interactive shell, rustup walks up the directory tree looking for a `rust-toolchain.toml` and resolves to the stable repo-root pin (1.95.0); cargo-fuzz then fails with `error: the option Z is only accepted on the nightly compiler`. The workaround — typing `cargo +nightly fuzz run ...` every time — is cheap on paper and painful in practice, and is already wired into CI for exactly this reason (see `.github/workflows/ci.yml` line 79). During the phase-01 state-4 gate, commit `97c1576` added `crates/envoy-config/fuzz/rust-toolchain.toml` pinning nightly so that rustup selects nightly when cargo is invoked from inside the workspace-excluded fuzz subcrate. That commit was a time-pressure CI-adjacent fix and did not land with an ADR; phase-01 state-5 REVIEW §Issues/Important item I1 correctly flagged the drift between ADR-0010 (which had explicitly *rejected* this approach, see `DECISIONS.md` line 204) and the shipped implementation. Per D-3.5 (ADRs are append-only; drift is corrected by superseding ADRs, not by editing the original), this ADR lands the nested-pin decision on the record after the fact and narrowly overrides ADR-0010 on that single bullet.
- Options considered:
  - **CI-side `+nightly` only; no nested file.** The pre-fix state ADR-0010 chose. Requires every local developer running `cargo fuzz run` to type `+nightly` (or invoke through `rustup run nightly cargo fuzz run ...`). Correct and mechanically simple, but creates a persistent friction surface that will recur on every subsequent fuzz target (HTTP/1.1 in phase 04, HTTP/2 in phase 05, protobuf family, the xDS family). Rejected by this ADR on ergonomics grounds.
  - **Nested `rust-toolchain.toml` in the workspace-excluded fuzz subcrate, plus the ADR-0010 explicit `+nightly` in CI.** The actually-landed implementation. The nested file is a directory-scoped toolchain override that only applies when cargo is invoked from inside `crates/envoy-config/fuzz/` (which is workspace-excluded per ADR-0008, so it is never entered during a mainline `cargo build` / `cargo clippy` / `cargo test` of the workspace root). ADR-0010 rejected this option on "surprising and brittle cross-workspace-boundary semantics" grounds; the rejection has not held up under practice — rustup's `rust-toolchain.toml` resolution walks from the invocation directory upward, and a nested file at `crates/envoy-config/fuzz/` is the *first* match for any cargo invocation from that directory, which is exactly the behavior we want. CI remains authoritative: the `cargo +nightly fuzz run` invocation in `.github/workflows/ci.yml` explicitly overrides any file-based pin regardless of nesting, so the nested file cannot silently flip CI onto an unintended toolchain.
  - **Move `envoy-config/fuzz` into the workspace and bump the repo-root `rust-toolchain.toml` to nightly.** Rejected for the same reasons ADR-0010 already rejected it: D-3.9 would break on every mainline `cargo build` / `cargo clippy` / `cargo test`, and "upgrading the toolchain pin is its own phase." Not revisited here.
- Decision: option 2. `crates/envoy-config/fuzz/rust-toolchain.toml` is allowed to exist, pins nightly, and is the toolchain source of truth when cargo is invoked from inside the workspace-excluded fuzz subcrate. The repo-root `rust-toolchain.toml` stays at stable 1.95.0 (D-3.9 preserved verbatim). CI continues to use the explicit `cargo +nightly fuzz run` invocation established by ADR-0010, and remains the authoritative source of toolchain selection for the phase-done gate.
- Rationale: the nested file solves a real, repeatable local-dev friction surface at zero cost to the mainline build or to CI. ADR-0010's "surprising and brittle" framing was a pre-implementation concern that execution did not bear out: rustup's resolution is *directory-scoped* and *deterministic*, and because `crates/envoy-config/fuzz` is workspace-excluded (ADR-0008), no mainline cargo invocation ever enters that directory. The two-source-of-truth concern is real but narrow: CI pins via the explicit `+nightly` flag (flag wins over file per rustup's documented precedence), and the nested file is scoped to a single workspace-excluded subcrate. The alternative (typing `+nightly` on every local invocation) imposes a persistent tax on every future fuzz target this project will add.
- Consequences:
  - Developer ergonomics for `cargo fuzz run` from `crates/envoy-config/` improve — no `+nightly` prefix needed in shell sessions; rustup resolves nightly directly from the nested file.
  - Two sources of nightly-toolchain authority now exist for the fuzz subcrate: the CI flag (`+nightly` in `.github/workflows/ci.yml`) and the nested file. CI is authoritative for the phase-done gate: the `+nightly` flag overrides any file-based pin regardless of nesting, so no future edit to the nested file can flip CI onto a different toolchain silently. Any conflict is resolved in CI's favor by construction.
  - Future fuzz subcrates follow the same pattern by default. When phase 04 adds an HTTP/1.1 fuzz target, phase 05 adds an HTTP/2 codec fuzz target, or a later phase adds a protobuf / xDS fuzz target, each new fuzz subcrate ships a nested `rust-toolchain.toml` pinning nightly — no new ADR per target, same decision scope as ADR-0009 for the tooling choice.
  - D-3.9 remains mechanically enforced for every mainline path. The repo-root `rust-toolchain.toml` stays at stable 1.95.0. No `cargo build` / `cargo clippy` / `cargo test` / `cargo deny check` invocation from the workspace root ever resolves nightly.
  - If Rust ever ships a built-in workspace-exclude-aware toolchain override that lets the root `rust-toolchain.toml` declare "excluded subcrates get a different channel" inline, this ADR is a candidate for supersession by a single-file solution.
- Provenance: this ADR closes phase-01 state-5 REVIEW §Issues/Important I1 (`docs/envoy-rust/phases/01-static-bootstrap-config/REVIEW.md` lines 155–180). The nested file was introduced in commit `97c1576` ("phase 01: add fuzz/rust-toolchain.toml to pin nightly for cargo-fuzz"); the companion CI-side `+nightly` invocation in commit `20ffb5b` ("phase 01: use cargo +nightly fuzz run to override stable toolchain pin") remains the ADR-0010-decided authoritative override for CI. ADR-0010 is unedited per D-3.5's append-only doctrine.

---

## ADR-0013: Split phase 02 into sub-phases 02.1 and 02.2

- Date: 2026-04-24
- Status: accepted
- Context: Phase 02's SPEC (committed at SHA `50349da`, `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md`) estimates ~2060 LoC of net change (§5 table) across ~22 tasks. The task-count estimate stays under the `BOOTSTRAP_PROMPT.md` §6.1 task gate (~25), but the LoC estimate exceeds the §6.1 LoC gate (~1500) by ~37%. Either gate alone is sufficient to trigger a split per §6.1 (`estimates exceed ~1500 lines of code of net change`). The SPEC §5 line-item breakdown is tight (per-test averages of 15–37 LoC; no obvious helper-sharing or fixture-sharing that would compress ~560 LoC out of the estimate), so the LoC overage will not dissolve under careful plan-writing. SPEC §5 anticipated this outcome and designed a clean 02.1/02.2 cut along the natural dependency boundary of the phase's three new crates: data-model + parser (02.1) precedes listener + proxy filter + differential fixture (02.2).
- Options considered:
  - (i) Accept the LoC overage and write a single `PLAN.md` for phase 02. Rejected: §6.1 is explicitly phrased `triggered … if either threshold is crossed`, and doctrine D-3.6 makes green-build the non-negotiable endpoint — a plan that lands ~2000 LoC of mixed new-crate wiring historically splits under mid-execution pressure (§6.1's in-flight trigger when any single task's sub-steps blow past ~10 items), which is strictly worse than splitting at state 2.
  - (ii) Split at a custom boundary not anticipated in SPEC §5. Rejected: the `envoy-tcp` crate depends on both `envoy-listener` (for the `ConnectionHandler` trait) and `envoy-cluster` (for `ClusterHandle`). Splitting inside `envoy-config` (e.g., shipping `Cluster`/`LoadAssignment` types in a first cut and `TypedConfig`/`TcpProxyConfig` in a second) doubles the parser churn. Splitting inside `envoy-listener` or `envoy-tcp` is even worse — each crate is a single coherent responsibility. The SPEC-designed cut is the only boundary that both halves sit cleanly in.
  - (iii) Split at SPEC §5's designated boundary: 02.1 lands `envoy-config` schema extensions + `envoy-cluster` + `tcp-echo-server` helper + I3 chunked-decoder tests + fuzz-corpus seeds; 02.2 lands `envoy-listener` + `envoy-tcp` + `envoy-bin` wiring + harness extensions + fixture 0003 + I4 (admin cap) + M1 (TODO retarget).
- Decision: (iii). Split phase 02 at the SPEC §5-designated boundary. The two sub-phases are:
  - **02.1 — Config schema + cluster manager + echo-server helper.** Slug `02.1-config-cluster`. Inherits phase-02 SPEC §§D1 (envoy-cluster), D3 (envoy-config schema + 16 validator tests), D6 (tcp-echo-server), D9–I3 only (four `decode_chunked` unit tests), D10 (two new fuzz-corpus seeds). Acceptance: stable-toolchain CI green; fuzz short-budget CI green on extended corpus; fixtures `0001-tcp-echo` and `0002-static-admin-ready` remain green; no new fixture ships; no new differential fixture passes because `envoy-bin` runtime dispatch for `tcp_proxy` remains explicitly unimplemented (parser accepts the YAML; runtime returns `UnsupportedFilter` equivalent until 02.2 wires `envoy-listener` + `envoy-tcp`). Depends on phase `01`.
  - **02.2 — Listener + TCP proxy filter + fixture 0003 + remaining rollovers.** Slug `02.2-listener-tcp-proxy`. Inherits phase-02 SPEC §§D2 (envoy-listener), D4 (envoy-tcp), D5 (envoy-bin wiring + integration test), D7 (harness extensions: `TcpProxyBackend`, `render_yaml` backend-key substitution, upstream `with_host`), D8 (fixture `0003-tcp-proxy`), D9 excluding I3 (I4 admin 8 KiB cap tightening + M1 stale-TODO retarget only), D11 (CI — unchanged), D12 excluding ADR-0013-original (the host-gateway + half-close ADRs only). Acceptance: fixture `0003-tcp-proxy` green end-to-end; fixtures 0001/0002 still green; full phase-done gate per §7.5 of `BOOTSTRAP_PROMPT.md`. Depends on `02.1`.
- Rationale: the two halves have one clean direction of dependency (02.1 → 02.2); each half is individually under both §6 gates (02.1 ~980 LoC across ~13 tasks; 02.2 ~1060 LoC across ~14 tasks — both well inside the 1500 LoC / 25 task thresholds); each half lights up its own independently-reviewable artifact (02.1: `cargo test --workspace` green with extended config grammar + cluster manager + helper binary; 02.2: fixture 0003 green with all integration pieces wired). Phase-01 rollovers split naturally: I3 is harness-only (`tests/differential/src/lib.rs::tests`) and rides with 02.1's envoy-config/harness touches; I4 touches `envoy-bin::admin` and M1 is doc-only in `tests/differential/src/subject.rs` — both sit alongside 02.2's `envoy-bin` and harness work. The SPEC §5 boundary is the one the brainstorm already validated.
- Consequences:
  - `docs/envoy-rust/ROADMAP.md` row `02` flips `status` → `in-progress` and `sub-phases` → `02.1, 02.2`. Two new rows land with `status = planned`: row `02.1` depends-on `01`; row `02.2` depends-on `02.1`. Row `02`'s `status` flips to `done` only after both sub-phases have landed (per `ROADMAP.md` schema: "The parent flips to `done` only after all sub-phases are `done`.").
  - The three phase-02 ADRs projected in SPEC §7 shift numbering by +1 in-tree: SPEC §7's original ADR-0013 (YAML-native `typed_config` deserialization) lands with 02.1 as **ADR-0014**; SPEC §7's original ADR-0014 (host-docker + host-gateway) lands with 02.2 as **ADR-0015**; SPEC §7's original ADR-0015 (`enable_half_close: false` default) lands with 02.2 as **ADR-0016**. Each sub-phase SPEC cites this ADR for the renumbering and the sub-phase-scoped ADR text is rewritten in the sub-phase SPEC.
  - `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` remains in-tree unedited as the parent-phase design artifact committed at SHA `50349da`. For execution purposes it is superseded by the two sub-phase SPECs (`02.1-config-cluster/SPEC.md` and `02.2-listener-tcp-proxy/SPEC.md`). Readers consulting the parent SPEC must cross-reference both sub-phase SPECs to find the operative ADR numbers and deliverable scopes.
  - `docs/envoy-rust/STATE.md` now points at `02.1-config-cluster` at lifecycle state 2 (SPEC.md exists, PLAN.md does not). The next session runs `superpowers:writing-plans` scoped to sub-phase 02.1.
  - `docs/envoy-rust/STATE.md` line 76's projection ("adds an ADR documenting the split (next sequential, likely ADR-0016)") was a hypothetical anticipating a mid-execution split; the split is landed pre-execution at state 2 instead, so the split ADR takes the actual next-sequential number (0013), not 0016. The parenthetical is now obsolete and the refreshed STATE.md reflects the actual numbering.
  - No doctrine delta. This ADR is the mechanical application of `BOOTSTRAP_PROMPT.md` §6.2 by the plan-writer upon inspecting SPEC §5's LoC estimate against the §6.1 gate. No existing ADR is superseded.

---

## ADR-0014: YAML-native `typed_config` deserialization until the xDS/protos family lands

- Date: 2026-04-24
- Status: accepted
- Context: Sub-phase 02.1 is the first phase to surface Envoy's `typed_config` envelope (`envoy.filters.network.tcp_proxy`). The `envoy-protos` crate + `prost` / `prost-build` + upstream proto-tree vendoring were deferred at phase-00 bootstrap to the xDS family (ROADMAP §9). 02.1 must choose: bring the protos stack forward now, or ship a narrower shim.
- Options considered:
  - **(i) YAML-native — one Rust enum discriminated on the `@type` URL string literal, fields deserialized by serde.** Minimal surface, scoped to this sub-phase's needs. Grows one enum variant per filter across phases 04/05/06 until the xDS family ships.
  - **(ii) Bring `prost` + `envoy-protos` in as part of 02.1.** Pulls forward multi-phase proto-tree vendoring. Out of ROADMAP row-02 scope; would trigger a further split by itself.
  - **(iii) Non-Envoy `raw_config` YAML key.** Diverges `envoy.yaml` and `envoy-rust.yaml` on filter shape, breaking the fixture principle that configs are initially identical.
- Decision: (i). `TypedConfig` enum in `envoy-config::bootstrap` with a `#[serde(tag = "@type")]` discriminator; one variant for TCP proxy in 02.1; extended per filter across future phases.
- Rationale: keeps 02.1 within row-02 scope; defers the `envoy-protos` multi-phase work until it pays for itself. Reviewable by shape — a stranger reading the YAML can see which filters are supported.
- Consequences: unknown `@type` URLs reject at parse time via serde's tagged-enum default behavior. Every new filter in phase 04 / 05 / 06 extends the enum by one variant. An `envoy-protos` supersession ADR in the xDS family re-routes the `@type` URL to prost-generated message types in one sweep and retires this shim.
- Provenance: this ADR was projected as "ADR-0013" in parent-phase SPEC §7 (`docs/envoy-rust/phases/02-tcp-proxy/SPEC.md`, committed at SHA `50349da`) and renumbered to ADR-0014 by the phase-02 split decision (ADR-0013). The projected ADR-0014 (host-docker + host-gateway) and ADR-0015 (`enable_half_close: false` default) from the parent SPEC are renumbered to ADR-0015 and ADR-0016 respectively and land with sub-phase 02.2.

---

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

---

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

---
