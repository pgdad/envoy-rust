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

## ADR-0017: Split phase 03 into sub-phases 03.1 and 03.2

- Date: 2026-04-25
- Status: accepted
- Context: Phase 03's SPEC (committed at SHA `a3f3474`, `docs/envoy-rust/phases/03-tls-tcp/SPEC.md`) estimates ~2845 LoC of net change (§5 table) across ~27 tasks for a single phase 03. The LoC estimate exceeds `BOOTSTRAP_PROMPT.md` §6.1's gate (~1500) by ~90% and the task-count estimate marginally exceeds the §6.1 task gate (~25). Either gate alone is sufficient to trigger a split per §6.1 (`triggered if either threshold is crossed`). The SPEC §5 line-item breakdown is tight (cert-loader + ServerConfig builder + per-side validator suites + harness PKI + three new fixtures across two new differential surfaces; per-test averages are 30–50 LoC), so the LoC overage will not dissolve under careful plan-writing. Parent-phase SPEC §5 anticipated this outcome and designed a clean 03.1/03.2 cut along the foundation-vs-extensions boundary that mirrors parent phase 02's pre-split posture (parent-phase-02 SPEC at SHA `50349da`; ADR-0013 formalized that split at the state-2 plan-writer session).
- Options considered:
  - (i) Accept the LoC overage and write a single `PLAN.md` for phase 03. Rejected: §6.1 is explicitly phrased `triggered … if either threshold is crossed`, and doctrine D-3.6 makes green-build the non-negotiable endpoint — a plan that lands ~2800 LoC of mixed new-crate wiring + new helper-binary scaffolding + three new fixtures across two new differential surfaces historically splits under mid-execution pressure (§6.1's in-flight trigger when any single task's sub-steps blow past ~10 items), which is strictly worse than splitting at state 2.
  - (ii) Split at a custom boundary not anticipated in parent-SPEC §5. Rejected: the foundation-vs-extensions cut is the only boundary that lets each half stand alone with one direction of dependency. Splitting inside `envoy-tls` (e.g., shipping the cert-loader + `ServerConfig` builder + single-cert resolver in a first cut and the SNI resolver + `ClientConfig` builder in a second) leaves the first cut without a runnable downstream-TLS fixture (single-cert downstream needs the `ServerConfig` builder + listener wiring + envoy-bin dispatch end-to-end). Splitting inside the harness (PKI in cut A, `Driver::TlsTcp` in cut B) is even worse — the PKI is unconsumed without the driver.
  - (iii) Split at parent-SPEC §5's designated boundary: 03.1 lands the `envoy-tls` foundation (cert loader, `ServerConfig` builder with single-cert resolver, `ClientConfig` builder, crypto-provider install) + envoy-config schema additions for TransportSocket / DownstreamTlsContext / UpstreamTlsContext / CommonTlsContext / TlsCertificate / CertificateValidationContext / DataSource (full schema, plus the optional `transport_socket` field on `Cluster`) + envoy-listener TLS dispatch via a `TlsAcceptingHandler` adapter in envoy-bin + envoy-tcp generic-stream lift over `AsyncRead + AsyncWrite + Unpin + Send + 'static` + envoy-bin wiring for downstream TLS termination + harness `tls.rs` with `TlsTestPki` + `Driver::TlsTcp` + `drive_tls` + render_yaml leaf-A/CA substitution + run_fixture dispatch + fixture 0004 (single-cert downstream TLS, plaintext upstream); 03.2 lands the SNI multi-cert `ResolvesServerCert` + `from_listener` constructor + UpstreamTls consumer wiring + envoy-bin upstream-TLS + multi-cert dispatch + harness `Driver::TlsTcpProbeList` + `drive_tls_probes` + `TlsEchoBackend` + `tls-echo-server` helper crate + fixtures 0005 (upstream TLS origination + wire-level SNI) + 0006 (multi-cert SNI cert selection). Each sub-phase carries one direction of dependency (03.1 → 03.2) and is individually under both §6.1 gates (03.1 ~1400 LoC across ~13 tasks; 03.2 ~1445 LoC across ~14 tasks).
- Decision: (iii). Split phase 03 at parent-SPEC §5's designated boundary. The two sub-phases are:
  - **03.1 — `envoy-tls` foundation + downstream TLS termination + fixture 0004.** Slug `03.1-tls-foundation-downstream`. Inherits parent-SPEC §§D1 (envoy-tls 03.1 portion: `DownstreamTls` struct + impl, `UpstreamTls` struct + impl as library code with unit tests, cert/key loader, single-cert `ResolvesServerCert`, crypto-provider install), D2 (envoy-config schema 03.1 portion: `TransportSocket` envelope, `TransportSocketTypedConfig` enum, `DownstreamTlsContext`, `UpstreamTlsContext`, `CommonTlsContext`, `TlsCertificate`, `CertificateValidationContext`, `DataSource`, optional `transport_socket` field on `Cluster`; ~10 validator tests; 3 new fuzz-corpus seeds), D3 (envoy-listener TLS dispatch — full; `TlsAcceptingHandler` adapter in envoy-bin per signpost 3 option α), D4 03.1 portion (envoy-tcp generic-stream lift over `AsyncRead + AsyncWrite + Unpin + Send + 'static`; 4 new envoy-tcp tests touching the generic shape), D5 03.1 portion (envoy-bin wiring + crypto-provider install + per-filter-chain TlsAcceptingHandler dispatch + integration test `crates/envoy-bin/tests/tls_downstream.rs`), D6 03.1 portion (harness `tests/differential/src/tls.rs` with `TlsTestPki::generate`, `Driver::TlsTcp { sni, expected_cn }` variant, `drive_tls`, render_yaml keys for leaf-A and CA paths, run_fixture dispatch on `{{LEAF_*_PATH}}` / `{{CA_PATH}}` substitution, upstream container-mount via `with_copy_to_container`; 4 harness unit tests), D8 fixture 0004 (single-cert downstream TLS, plaintext upstream backend), D10 ADR-0018 + ADR-0019 (renumbered from parent-SPEC §7's projected ADR-0017 + ADR-0018). Acceptance: stable-toolchain CI green; fuzz short-budget CI green on extended corpus; fixtures `0001-tcp-echo`, `0002-static-admin-ready`, `0003-tcp-proxy` remain green; new fixture `0004-tls-downstream` green end-to-end. Depends on phase `02` (parent done at `f04e21a`).
  - **03.2 — Upstream TLS origination + multi-cert SNI cert selection + `tls-echo-server` helper + fixtures 0005 + 0006.** Slug `03.2-tls-upstream-sni`. Inherits parent-SPEC §§D1 (envoy-tls 03.2 portion: `SniResolver` SNI-keyed `ResolvesServerCert`; `DownstreamTls::from_listener` constructor; ~5 new unit tests), D2 03.2 portion (envoy-config `FilterChainMatch` struct + `server_names` field; validator extensions for `MultipleListenersWithOverlappingSni` + `MultipleCatchAllFilterChains`; ~6 new validator tests; 2 new fuzz-corpus seeds), D4 03.2 portion (UpstreamTls consumer wiring; envoy-cluster or envoy-bin upstream-TLS plumbing per parent-SPEC D4 alternative; 3 new envoy-tcp tests), D5 03.2 portion (envoy-bin multi-cert + upstream TLS wiring; integration tests `tls_upstream.rs` + `tls_sni.rs`), D6 03.2 portion (`Driver::TlsTcpProbeList`, `drive_tls_probes`, `TlsEchoBackend`; 2 new harness unit tests), D7 (`tests/helpers/tls-echo-server/` helper binary crate; ~120 LoC impl + ~5 unit tests), D8 fixtures 0005 (upstream TLS origination with wire-level SNI) + 0006 (multi-cert SNI cert selection on downstream listener). Acceptance: stable-toolchain CI green; fuzz short-budget CI green; fixtures 0001/0002/0003/0004 still green; fixtures `0005-tls-upstream` and `0006-tls-sni` green end-to-end; phase-03 parent ROADMAP row flips to `done` in 03.2's final commit (ROADMAP-schema invariant: parent flips to `done` only after all sub-phases are `done`). Depends on `03.1`.
- Rationale: the two halves have one clean direction of dependency (03.1 → 03.2); each half is individually under both §6.1 gates (03.1 ~1400 LoC / ~13 tasks; 03.2 ~1445 LoC / ~14 tasks — both well inside the 1500 LoC / 25 task thresholds); each half lights up its own independently-reviewable artifact (03.1: fixture 0004 green with envoy-tls foundation + downstream TLS termination on a single-cert listener; 03.2: fixtures 0005 + 0006 green with upstream TLS origination on the wire and multi-cert SNI cert selection on the downstream listener). Parent-phase brainstorm Q4 already validated this fixture distribution: fixture 0004 in 03.1 proves the envoy-tls scaffold works end-to-end on the smallest TLS surface; fixtures 0005 + 0006 in 03.2 layer wire-level SNI and multi-cert resolution on top. The boundary is the one the parent brainstorm explicitly designed.
- Consequences:
  - `docs/envoy-rust/ROADMAP.md` row `03` keeps `status` = `in-progress` (already flipped at the state-1 close-out commit `4c36dcf`) and gains `sub-phases` = `03.1, 03.2`. Two new rows land with `status = planned`: row `03.1` depends-on `02`; row `03.2` depends-on `03.1`. Row `03`'s `status` flips to `done` only after both sub-phases have landed (per `ROADMAP.md` schema: "The parent flips to `done` only after all sub-phases are `done`.").
  - The three phase-03 ADRs projected in parent-SPEC §7 shift numbering by −2 / +1 to fit the actual landed sequence. Parent-SPEC §7's projected ADR-0019 (split decision) is the first to land — at this commit, state 2 plan-writer time — and takes the next-sequential number, **ADR-0017** (this ADR). Parent-SPEC §7's projected ADR-0017 (`rcgen` + `tempfile` permitted as dev-test-harness-only foundations) is renumbered to **ADR-0018** and lands at 03.1 task 1. Parent-SPEC §7's projected ADR-0018 (`tokio-rustls` + `rustls-pemfile` covered by the rustls foundations grant) is renumbered to **ADR-0019** and lands at 03.1 task 1. Each sub-phase SPEC cites this ADR for the renumbering scheme and rewrites each expected ADR's text with its actual number.
  - `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` remains in-tree unedited as the parent-phase design artifact committed at SHA `a3f3474`. For execution purposes it is superseded by the two sub-phase SPECs (`03.1-tls-foundation-downstream/SPEC.md` and `03.2-tls-upstream-sni/SPEC.md`). Readers consulting the parent SPEC must cross-reference both sub-phase SPECs to find the operative ADR numbers and deliverable scopes. This mirrors the parent-phase-02 SPEC's posture under ADR-0013 (last touched at SHA `50349da`).
  - `docs/envoy-rust/STATE.md` now points at `03.1-tls-foundation-downstream` at lifecycle state 2 (SPEC.md exists, PLAN.md does not). The next session runs `superpowers:writing-plans` scoped to sub-phase 03.1.
  - Per the parent-phase-02 split-commit precedent (commit `1c38ca9`, which landed ADR-0013 + ROADMAP + STATE + both sub-phase SPECs in one commit), state 2's plan-writer redistributes parent-SPEC content into fresh sub-phase SPECs as part of this same commit (per `BOOTSTRAP_PROMPT.md` §6.2 step 3 "Redistribute spec content — each sub-phase gets its own SPEC.md"). The post-commit STATE points at 03.1 at lifecycle state 2, not at state 1 — the `BOOTSTRAP_PROMPT.md` §5.1 "one state per session" rule is preserved because the §6.2 split protocol *is* the state-2 work (writing PLAN.md is replaced by writing the split artifacts and stopping).
  - No doctrine delta. This ADR is the mechanical application of `BOOTSTRAP_PROMPT.md` §6.2 by the plan-writer upon inspecting parent-SPEC §5's LoC estimate against the §6.1 gate. No existing ADR is superseded.

---

## ADR-0018: `rcgen` and `tempfile` permitted as dev-test-harness-only foundations

- Date: 2026-04-25
- Status: accepted
- Context: Phase 03 is the first phase to need test certificates. TLS test infrastructure recurs across phases 03–08+ (HTTP/1.1 over TLS, H2 over TLS, mTLS, etc.). Static in-tree PEMs were considered and rejected per the parent-phase brainstorm Q2 decision (poor refresh ergonomics, expiry concerns, multi-leaf cert generation gets unwieldy). `rcgen` is the maintained Rust-native cert generator; `tempfile` is the canonical per-test-run tmpdir manager. Neither is on the D-3.2 permitted-foundations list at phase-02.2 close.
- Options considered: (i) static in-tree PEMs (rejected, parent-brainstorm Q2); (ii) `rcgen` + `tempfile` on the permitted list as **dev-test-harness-only** (decision); (iii) script-generated PEMs committed to the repo (rejected, parent-brainstorm Q2: worst-of-both-worlds — refresh friction *and* in-tree drift).
- Decision: add `rcgen = "0.13"` and `tempfile = "3"` to the permitted-foundations list with the **dev-test-harness-only** annotation. Mirrors ADR-0009's posture for `cargo-fuzz` + `libfuzzer-sys`. Never a transitive of `envoy-bin` or any non-test workspace crate. Restricted to: `tests/differential/` dev-deps; `tests/helpers/tls-echo-server/` dev-deps (lands in 03.2); `crates/envoy-tls/` dev-deps (for unit-test PKI); `crates/envoy-bin/` dev-deps (for the in-process integration test); `crates/envoy-tcp/` dev-deps (for the TLS-flavored unit tests).
- Rationale: one-time foundations grant beats per-phase ADR churn; rcgen is the Rust-ecosystem default; tempfile is ubiquitous test-infra. Test-only restriction preserves D-3.2's spirit for runtime code.
- Consequences: future TLS-cert-using phases (04 HCM-over-TLS, 05 H2-over-TLS, mTLS phases, etc.) reuse this decision without per-phase ADRs. `cargo deny check` may flag the rcgen license (Apache-2.0 OR MIT — both on the deny.toml allow-list) or its transitive deps; if so, the deny.toml is updated alongside ADR-0018's landing. If a future phase needs cert generation in *runtime* code (e.g., hot-restart cert rotation), that phase lands a new ADR superseding the dev-test-harness-only restriction.
- Provenance: this ADR was projected as "ADR-0017" in parent-phase-03 SPEC §7 (`docs/envoy-rust/phases/03-tls-tcp/SPEC.md`, committed at SHA `a3f3474`) and renumbered to ADR-0018 by the phase-03 split decision (ADR-0017). The projected ADR-0018 (tokio-rustls + rustls-pemfile) is renumbered to ADR-0019 and lands alongside this ADR in the same Task-1 commit.

---

## ADR-0019: `tokio-rustls` and `rustls-pemfile` covered by the rustls foundations grant

- Date: 2026-04-25
- Status: accepted
- Context: D-3.2 lists `rustls`, `webpki`, `rustls-pki-types`, and "`aws-lc-rs` permitted as the crypto provider," but does not name `tokio-rustls` or `rustls-pemfile` explicitly. Both are mechanically necessary to use rustls inside a tokio runtime / load PEMs from disk; both ship from the rustls org.
- Options considered: (i) treat both as covered implicitly by the rustls grant — risks ambiguity for downstream phases; (ii) land an ADR formalizing the extension (decision); (iii) hand-roll the async glue and PEM parser — reinvents wheels D-3.2 explicitly tells us not to.
- Decision: extend D-3.2's "rustls + aws-lc-rs permitted as the crypto provider" grant to cover `tokio-rustls = "0.26"` and `rustls-pemfile = "2"`. Both are runtime-permitted (not dev-only); rcgen + tempfile from ADR-0018 stay dev-only.
- Rationale: removes ambiguity for downstream phases. Both crates are first-party in the rustls ecosystem; treating them as part of the same foundation is the cheapest, most honest formalization.
- Consequences: envoy-tls's `Cargo.toml` lists both as direct deps. `tls-echo-server`'s `Cargo.toml` (lands in 03.2) lists both. Neither is allowed in `envoy-listener` or `envoy-cluster` — those crates remain rustls-free per D1's "envoy-tls is the only crate with rustls deps" architectural rule.
- Provenance: this ADR was projected as "ADR-0018" in parent-phase-03 SPEC §7 (`docs/envoy-rust/phases/03-tls-tcp/SPEC.md`, committed at SHA `a3f3474`) and renumbered to ADR-0019 by the phase-03 split decision (ADR-0017). Lands alongside ADR-0018 in the same Task-1 commit.

---

## ADR-0020: Split phase 04 into sub-phases 04.1, 04.2, and 04.3

- Date: 2026-04-26
- Status: accepted
- Context: Phase 04 ("HTTP connection manager (HTTP/1.1) + route match + router filter + direct_response") was projected as a single phase by `BOOTSTRAP_PROMPT.md` §8 row 04. The parent-04 state-1 brainstorm (commit `805433e`, parent SPEC at `docs/envoy-rust/phases/04-http1/SPEC.md`) resolved two cascading scope decisions that, taken together, push the phase past the §6.1 split-gate (~25 tasks / ~1500 LoC): (a) all 7 of Envoy's `HeaderMatcher` modes are in-scope (`exact_match`, `prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`, `present_match`, `string_match` via the modern generic tagged-union, plus `invert_match: bool`), and (b) `RouteMatch` supports `prefix` + `path` + `headers` axes. The matcher fan-out alone is sized at ~1300 LoC + a new permitted-foundations dep (`regex`, landing under ADR-0021); combined with the codec library + HCM scaffold + minimal routing + `direct_response` fixture (~1500 LoC), a single-sub-phase 04.α (downstream-everything) would have hit ~2300+ LoC.
- Options considered: (i) **single phase** — rejected, exceeds §6.1 gates by ~50% in LoC and ~25% in task count; (ii) **two-way split by traffic direction** (04.α = downstream + matchers + direct_response; 04.β = upstream + router proxy + fixture 0008) — rejected, 04.α still ~2300+ LoC; would force a nested split of 04.α; (iii) **two-way split with deferred matchers** (04.α = downstream + minimal matchers + direct_response; 04.β = upstream + remaining matchers + router proxy + fixture 0008) — rejected, scatters the matcher fan-out across two sub-phases for arbitrary scope-fit reasons; (iv) **nested split of 04.α** (04 → 04.α → 04.α.1 / 04.α.2; 04 → 04.β) — rejected, three-level numbering is awkward (`04.α.1`); BOOTSTRAP_PROMPT.md §6.1 explicitly flags nested splits of an already-split sub-phase as suspicious and prescribes `superpowers:systematic-debugging` first; (v) **3-way flat split** by surface boundary (decision) — chosen.
- Decision: split phase 04 into three sub-phases by surface boundary, not by traffic direction:
  - **04.1 (`04.1-hcm-direct-response`)** — `envoy-http1` codec library + HCM as a network filter + `route_config` schema (RouteConfiguration / VirtualHost / Route with `prefix` + `path` matchers; multi-VH with `domains: ["*"]` or exact match) + `direct_response` action (`status` + `body.inline_string`) + harness `Driver::Http1` + `drive_http1` + fixture `0007-http1-direct-response`. Plaintext only. ~1500 LoC, ~17 tasks.
  - **04.2 (`04.2-route-matchers`)** — `RouteMatch.headers: Vec<HeaderMatcher>` + all 7 `HeaderMatcher` modes + `StringMatcher` tagged union + `invert_match: bool`. **ADR-0021** (`regex = "1"` permitted as a foundation for header / route matching) lands at 04.2 Task 1. Validator + ~25 unit tests + 1 fuzz seed extension. NO new fixture (matchers are config-side; differential property exercised via 04.1's fixture 0007 amended in 04.2 to use a non-trivial matcher route). ~1300 LoC, ~14 tasks.
  - **04.3 (`04.3-router-upstream`)** — `envoy-http1::Client` (per-connection HTTP/1.1 client; no pooling) + router filter's `Route(RouteAction_Route)` arm (proxy to cluster) + new helper crate `tests/helpers/http1-echo-server` + harness `Http1EchoBackend` + fixture `0008-http1-router-upstream` + opportunistic close-out of the multi-phase `Cluster::name()` carryforward (M1 chain from phase-02.1 REVIEW). BEHAVIOR_CONTRACT.md adds `x-envoy-upstream-service-time` to the Header allow-list. ~1500 LoC, ~17 tasks.
- Rationale: the 3-way flat split is unusual (only phase 04 takes it; phases 02 and 03 used 2-way splits). The cost is one extra sub-phase row in the ROADMAP. The benefit is avoiding the nested-split anti-pattern (BOOTSTRAP_PROMPT.md §6.1 + the phase-03.2 SPEC §5 closing paragraph both flag nested splits as a `systematic-debugging` trigger). Each of the three sub-phases fits comfortably under the §6.1 gates. The split boundary is by surface (codec/HCM → matchers → upstream), not strictly by traffic direction; this is a reasonable accommodation for the matcher-fan-out scope.
- Consequences: parent SPEC at `docs/envoy-rust/phases/04-http1/SPEC.md` was committed at parent-04 state-1 (commit `805433e`, the previous commit) projecting this split; sub-phase SPECs (`04.1-hcm-direct-response/SPEC.md`, `04.2-route-matchers/SPEC.md`, `04.3-router-upstream/SPEC.md`) land alongside this ADR in this same parent-04 state-2 commit. ROADMAP gains 3 new rows (`04.1`, `04.2`, `04.3`) at this commit; row `04`'s `sub-phases` column was already populated as `04.1, 04.2, 04.3` at the state-1 commit. Sub-phases ship strictly in order (04.1 → 04.2 → 04.3) — they cannot be parallelized because 04.2 amends 04.1's fixture 0007 (adding a header-matcher route to demonstrate matcher production-use) and 04.3 extends both the schema (`RouteAction_Route` variant) and the runtime (router filter's proxy arm). Parent ROADMAP row `04` flips `done` at sub-phase 04.3's state-6 phase-done commit, mirroring phase 03's `ca81226`-shape close-out. Phase 04's projected ADR ledger after this commit: ADR-0020 (this ADR; landed at this commit), ADR-0021 (`regex`; lands at 04.2 Task 1).
- Provenance: this ADR was projected as the next-sequential available ADR number in parent-phase-04 SPEC §7 (`docs/envoy-rust/phases/04-http1/SPEC.md`, committed at SHA `805433e`). Unlike phase-03's split which renumbered three projected ADRs (per ADR-0017's provenance footer), phase-04's split lands cleanly at ADR-0020 with no renumbering needed (ADR-0019 was the latest ADR before this commit; no inter-ADR landings have occurred between phase-03's close at `ca81226` and this commit).

---

## ADR-0021: `regex` permitted as a foundation for header / route matching

- Date: 2026-04-27
- Status: accepted
- Context: Phase 04.2 lands all 7 of Envoy's `HeaderMatcher` modes — `exact_match`, `prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`, `present_match`, `string_match` (with the `StringMatcher` tagged union which itself has a `safe_regex` variant). Two of those modes — `safe_regex_match` and `string_match.safe_regex` — require a regex implementation. The Rust `regex` crate is the de-facto ecosystem default (RE2-compatible NFA engine, no backtracking, no catastrophic regex blow-ups; well-maintained; first-party `rust-lang` org). Not on the D-3.2 permitted-foundations list at phase-03.2 close (ADR-0019 was the latest ADR; the latest pre-04 permitted-foundations grant covered `tokio-rustls` + `rustls-pemfile` under the rustls grant).
- Options considered: (i) **defer `safe_regex_match` to a later phase** — rejected; the parent-04 brainstorm decision (per ADR-0020's context section + parent-04 SPEC §3 D6.2) was to land all 7 HeaderMatcher modes in 04.2 coherently; deferring one mode would scatter the matcher coverage across phases for arbitrary reasons; (ii) **hand-roll a regex engine** — rejected; reinvents wheels D-3.2 explicitly tells us not to; the `regex` crate is mature and ecosystem-standard; (iii) **add `regex = "1"` to the permitted-foundations list narrowly scoped to header / route matching at config-load time** (decision); (iv) **add `regex = "1"` to the permitted-foundations list with broad scope** — rejected; D-3.2's spirit is one-foundation-per-purpose; broader scopes warrant their own scope-extension ADRs at the time the broader use surfaces.
- Decision: extend the D-3.2 permitted-foundations list to cover `regex = "1"` as a runtime dep on `crates/envoy-config/`, narrowly scoped to `HeaderMatcher::SafeRegex` + `StringMatcher::SafeRegex` compilation at config-load time. NOT permitted for general-purpose use elsewhere; future filter-framework regex needs (URL path templates in a future router-knob phase, header-rewrite patterns in a future filter-framework phase, Lua filter `string.find` in a future Lua-filter phase) require an explicit scope-extension ADR that names this ADR and broadens the grant.
- Rationale: removes the per-phase-ADR churn that would otherwise dog later regex-using phases (HCM-internal regex would still warrant its own ADR if/when it surfaces — the narrow scope here is deliberate). `regex` is the Rust-ecosystem default; treating its first use as the foundation grant is the cheapest, most honest formalization. Compiling regexes at config-load time (validator pass) means unparseable patterns are caught before any request is served.
- Consequences: `crates/envoy-config/Cargo.toml`'s `[dependencies]` section gains `regex = "1"` at this commit. `Cargo.lock` gains `regex` + transitive surface (`regex-syntax`, `aho-corasick`, `memchr`) as a dedicated commit at the 04.2 state-4 phase-done gate per established phase precedent (phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85633e7`). `cargo deny check` requires the `Unlicense` license to be added to `deny.toml`'s `[licenses] allow` list (transitive `aho-corasick` + `memchr` are MIT/Unlicense dual-licensed); that addition lands in this same Task-1 commit. `regex` itself is dual-licensed MIT/Apache-2.0 (already on the allow-list since phase 00); `regex-syntax` is MIT/Apache-2.0 (already covered). Future scope-extension ADRs that broaden the grant (HCM internal regex, filter-framework regex) name this ADR explicitly.
- Provenance: this ADR was projected as the next-sequential available ADR number in parent-04 SPEC §7 (`docs/envoy-rust/phases/04-http1/SPEC.md`, committed at SHA `805433e`); ADR-0020 (parent-04 split decision) lands at parent-04 state-2 commit `1d9740d`; ADR-0021 lands at this commit (04.2 Task 1).

---
