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
