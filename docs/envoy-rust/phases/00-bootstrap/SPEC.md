# Phase 00 — Bootstrap

- **Phase id:** `00`
- **Title:** Bootstrap: Cargo workspace layout, `rust-toolchain.toml`, `deny.toml`, CI, Docker reference Envoy, differential harness skeleton, `ENVOY_TARGET.md` pin, trivial echo fixture
- **Differential surface when done:** harness boots; one TCP echo fixture is green.
- **Depends on:** none.
- **Seeded by:** `BOOTSTRAP_PROMPT.md` §8 row 00.

This spec is the design contract for phase 00. The next session converts it into
`PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` /
`SKILL_ROUTING.md`). It is intentionally concrete enough to be turned into a
plan by a stranger with zero prior context per doctrine D-3.4.

---

## 1. Goal and acceptance signal

**Goal.** Produce a minimal envoy-rust that runs as a TCP echo proxy, managed
by a differential test harness that drives the same byte payload at upstream
Envoy and envoy-rust and asserts byte-exact response equivalence. Phase 00
lights up *all* the scaffolding that later phases depend on: workspace
structure, toolchain and license pins, upstream Envoy version pin, CI, and the
harness entrypoint.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`,
scoped to this phase's feature surface:

- (a) the new differential fixture `tests/fixtures/0001-tcp-echo/` is green;
- (b) no pre-existing differential fixtures exist, so nothing else to regress;
- (c) no conformance suites run this phase (none attach until HTTP/2 in phase 05);
- (d) no fuzz target ships this phase (no parser or codec yet);
- (e) `cargo build --workspace --all-targets`,
      `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
      `cargo fmt --all -- --check`,
      `cargo test --workspace`,
      and `cargo deny check` are all clean in CI;
- (f) `REVIEW.md` for this phase is approved.

---

## 2. Deliverables

### D1 — Workspace members and first crates

Add two workspace members to the root `Cargo.toml`: `crates/envoy-bin` and
`tests/differential`. Both crate root files begin with
`#![forbid(unsafe_code)]` per doctrine D-3.8.

- `crates/envoy-bin/Cargo.toml` — binary crate. Dependencies drawn only from
  the permitted-foundations list in D-3.2: `tokio` (features:
  `rt-multi-thread`, `macros`, `net`, `io-util`, `signal`), `serde` (with
  `derive`), `serde_yaml`, `anyhow`, `tracing`, `tracing-subscriber`.
  Explicitly no HTTP dependencies, no filter crates, no `clap`. See §6 below
  for why argv parsing is hand-rolled.
- `tests/differential/Cargo.toml` — test crate (`[[test]] name = "echo"`).
  Dependencies: `tokio`, `testcontainers`, `anyhow`, `tracing`,
  `tracing-subscriber`, `serde`, `serde_yaml`.

No other crates are introduced in phase 00. `crates/envoy-listener`,
`crates/envoy-cluster`, `crates/envoy-filter`, and every other crate listed in
§4 of `BOOTSTRAP_PROMPT.md` are phase 02 or later.

### D2 — `ENVOY_TARGET.md` fully populated

The empty placeholder is replaced with resolved values:

- **Image:** `envoyproxy/envoy:v1.33.0`
- **Digest:** the `sha256:<hex>` returned by `docker pull envoyproxy/envoy:v1.33.0` during execution. Must be recorded verbatim.
- **Upstream release notes:** link to the Envoy project's release announcement for `v1.33.0`, resolved during execution.
- **Proto tree commit:** the `envoyproxy/envoy` git SHA corresponding to tag `v1.33.0`, resolved during execution (`git ls-remote --tags https://github.com/envoyproxy/envoy refs/tags/v1.33.0`).
- **xDS transport version:** v3 (confirmed during execution).

Resolving each of these is an execution-time step, not a planning step. If
any of them cannot be resolved cleanly (e.g. the tag no longer exists), an ADR
must be landed that picks a different tag and re-baselines — the pin cannot be
"unresolved."

### D3 — `envoy-bin` stub binary

`crates/envoy-bin/src/main.rs` is a `#[tokio::main]` binary that:

1. Parses argv: expects exactly `-c <path>` or `--config-path <path>`. Anything
   else is a parse error logged via `tracing::error!` and a non-zero exit code
   (`2`). No other flags in this phase.
2. Reads the YAML file at that path into a narrow config type:

    ```
    struct Bootstrap {
        static_resources: StaticResources,
    }
    struct StaticResources {
        listeners: Vec<Listener>,
    }
    struct Listener {
        name: String,
        address: Address,
        filter_chains: Vec<FilterChain>,
    }
    struct Address {
        socket_address: SocketAddress,
    }
    struct SocketAddress {
        address: String, // "0.0.0.0"
        port_value: u16,
    }
    struct FilterChain {
        filters: Vec<NetworkFilter>,
    }
    struct NetworkFilter {
        name: String, // must equal "envoy.filters.network.echo"
    }
    ```

   Any field not covered here (including fields upstream Envoy requires at its
   schema level) is ignored by envoy-rust in phase 00. If a listener specifies
   a network filter whose `name` is not `envoy.filters.network.echo`, envoy-rust
   rejects the config at startup with a clear error and exits non-zero
   (`1`). This hard-stops later phases from accidentally silently-passing
   incomplete phase-00 envoy-rust against richer configs.
3. Binds a TCP listener on `address.port_value` of the first listener and
   enters an accept loop. For each accepted connection, echoes bytes from
   the client back to the client verbatim (`tokio::io::copy_bidirectional` is
   not needed — a simple `read` / `write` loop in one direction suffices,
   because Envoy's echo filter writes back the inbound byte stream on the same
   connection). The echo loop exits when the client closes its write half,
   at which point envoy-rust closes the connection.
4. Logs via `tracing` with a `tracing-subscriber::fmt` layer configured from
   the `ENVOY_RUST_LOG` env var (falling back to `info`).
5. On `SIGTERM` or `SIGINT`, stops accepting new connections and drains
   in-flight ones with a 5-second grace, then exits `0`. Signal handling uses
   `tokio::signal::unix`.
6. Supports only the single-listener config shape above. Multi-listener support
   arrives with phase 02's listener manager.

### D4 — `tests/differential/` harness skeleton

`tests/differential/src/lib.rs` exposes a reusable entrypoint:

```
pub fn run_fixture(fixture_dir: &std::path::Path) -> anyhow::Result<()>;
```

The function:

1. Reads `fixture_dir/expectations.yaml` (defines which equivalence rules
   apply — phase 00 uses only `response_body: byte_exact`).
2. Starts the reference container via `testcontainers`:
   image `envoyproxy/envoy:v1.33.0`, the `envoy.yaml` inside `fixture_dir`
   mounted to `/etc/envoy/envoy.yaml`, Envoy invoked with `-c
   /etc/envoy/envoy.yaml`. The listener port from the fixture is exposed and
   the host-side mapped port is captured.
3. Spawns envoy-rust via `std::process::Command::new(env!(
   "CARGO_BIN_EXE_envoy-bin"))` (set automatically by Cargo when the test
   crate depends on `envoy-bin` with `features = []` in dev-dependencies, or
   via a `[build-dependencies]` trick if that path doesn't work; the planner
   resolves the canonical mechanism). Passes `-c fixture_dir/envoy-rust.yaml`.
   Binds on a free port chosen at test time (the YAML is templated with the
   port before invocation).
4. Polls both proxies' TCP listeners until accept-ready (retry with backoff,
   10s budget).
5. Loads the single input file from `fixture_dir/inputs/payload.bin`, opens a
   TCP connection to each proxy, writes the payload, half-closes the write
   side, reads to EOF, asserts the two responses are byte-exact equal.
6. On any error or assertion failure, both proxies are terminated deterministically
   (child-process guard on Drop; container stop on Drop) so no zombie state
   leaks to the next test.
7. Returns `Ok(())` on success.

`tests/differential/tests/echo.rs` contains the single test:

```
#[tokio::test]
async fn echo_fixture() -> anyhow::Result<()> {
    differential::run_fixture(std::path::Path::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/0001-tcp-echo")
    )).await
}
```

### D5 — Echo fixture `tests/fixtures/0001-tcp-echo/`

Files in this fixture directory:

- `envoy.yaml` — minimal Envoy bootstrap with one listener on `:{{PORT}}`
  whose filter chain has a single `envoy.filters.network.echo` filter. The
  `{{PORT}}` token is substituted to a free port at harness runtime (the
  harness's responsibility, not the fixture's).
- `envoy-rust.yaml` — the same structural shape, same `{{PORT}}` token. For
  phase 00 the two files are structurally identical modulo Envoy-required
  boilerplate that envoy-rust's narrow config parser ignores.
- `inputs/payload.bin` — a deterministic byte sequence:
  `hello, envoy-rust\n` (18 bytes, UTF-8, no BOM). Keeps the first fixture
  trivially inspectable.
- `expectations.yaml`:

    ```
    equivalence:
      response_body: byte_exact
    ```

  (No header, trailer, stat, access-log, or timing clauses — phase 00 does
  not exercise any of those.)
- `README.md` — one-paragraph description: "This fixture drives identical
  bytes at upstream Envoy's `envoy.filters.network.echo` filter and at
  envoy-rust's phase-00 echo listener, asserting byte-exact response body
  equivalence. It is the first differential fixture in the project and
  establishes the harness contract for subsequent TCP fixtures."

### D6 — GitHub Actions CI

`.github/workflows/ci.yml`, triggering on `push` to `main` and on
`pull_request` against `main`. Single job on `ubuntu-latest` (Docker is
required for `testcontainers` → upstream Envoy; macOS and Windows runners are
out of scope for phase 00). Steps in order:

1. `actions/checkout@v4`.
2. Rust toolchain install pinned via `rust-toolchain.toml`
   (`dtolnay/rust-toolchain@stable` with no `toolchain:` input reads the
   file). Components: `rustfmt`, `clippy`.
3. `Swatinem/rust-cache@v2` for cargo cache.
4. `cargo fmt --all -- --check`.
5. `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
6. `cargo build --workspace --all-targets`.
7. `cargo test --workspace` (runs the differential harness; pulls the pinned
   Envoy image).
8. `cargo deny check` (install `cargo-deny` via `taiki-e/install-action@cargo-deny`).

No release step, no publishing, no artifacts uploaded. CI green is the only
exit criterion.

### D7 — ADRs to land during execution

Three ADRs are landed in `docs/envoy-rust/DECISIONS.md` as part of this
phase's execution, each with full context / options / rationale per the ADR
template:

- **ADR-0002 — GitHub Actions as the CI provider.** Rationale: free for OSS,
  mature Rust toolchain integration, docker-in-docker support on
  `ubuntu-latest` runners for `testcontainers`. Alternatives considered:
  self-hosted Buildkite, Drone, GitLab CI.
- **ADR-0003 — Rust edition 2024 for all workspace crates.** Rationale: edition
  2024 stabilized in rustc 1.85; our toolchain pin is 1.95.0; future-proofing
  the codebase at bootstrap avoids a mass-edition bump phase later.
- **ADR-0004 — Upstream Envoy pin: `envoyproxy/envoy:v1.33.0`.** Rationale:
  latest stable `v1.33.x` LTS line at the project's bootstrap date. SHA256
  and proto-tree SHA are resolved during execution and recorded in
  `ENVOY_TARGET.md`. Alternatives considered: earlier LTS (`v1.32.x`) —
  rejected because the differential harness should start on the newest
  surface the project will actually need to catch up to.

Additional ADRs may be required during execution if cargo-deny flags
unexpected transitive licenses, or if a permitted-foundation dependency pulls
in a forbidden transitive. Handle per doctrine D-3.5 when encountered.

---

## 3. Behavior-contract scope for phase 00

Phase 00 exercises exactly one equivalence dimension: **Response body —
byte-exact for deterministic handlers** (row 2 of the matrix in
`BEHAVIOR_CONTRACT.md` §7.2 / equivalent in `MISSION.md`). No new rows, no new
allow-list entries, no new mappings. The currently-empty subsections of
`BEHAVIOR_CONTRACT.md` (Header allow-list, Stat-name mapping, Access log field
mapping, xDS wire state machine, Timing tolerances) remain empty after phase
00.

---

## 4. Non-goals (deferred to later phases)

- Listener manager abstraction — phase 02.
- Cluster manager, load balancer, static cluster — phase 02.
- Filter chain iteration protocol, per-route config, extension registry — phase
  07. Phase 00 wires the single echo filter directly into `envoy-bin`'s main
  loop; phase 02 begins the generalization; phase 07 finishes it.
- TLS — phase 03.
- HTTP/1.1 — phase 04; HTTP/2 — phase 05; HTTP/3 — §9 family.
- Access logs, stats subsystem, Prometheus endpoint — phase 06.
- Admin API, graceful-drain semantics — phase 08.
- xDS subsystem — §9 family.
- Conformance suites — first one lands with phase 05 (`h2spec`).
- Fuzz targets — first one lands with phase 01 (config parser).
- Multi-listener support, multi-filter filter chains — deferred with the
  listener manager (phase 02) and filter framework (phase 07) respectively.
- macOS/Windows CI — no phase planned; Linux-only is the long-run intent,
  matching upstream Envoy's production posture.

---

## 5. Splitting guidance for the planner

If the executor's `PLAN.md` crosses either §6 threshold (~25 tasks or ~1500
LoC of estimated net change), split phase 00 at exactly this boundary:

- **00.1 — Scaffolding.** Workspace finalization (member list, edition pin in
  each crate's Cargo.toml, `#![forbid(unsafe_code)]` lines), CI workflow,
  `ENVOY_TARGET.md` pin resolution, ADR-0002/0003/0004 landed. Differential
  surface: none yet. Acceptance: `cargo build --workspace --all-targets`,
  `cargo deny check`, and CI workflow all green on an otherwise-empty
  workspace.
- **00.2 — Echo fixture + harness.** `envoy-bin` stub, `tests/differential/`
  harness, `tests/fixtures/0001-tcp-echo/`. Acceptance: the echo fixture is
  green on the harness under upstream Envoy `v1.33.0`.

Do **not** pre-emptively split. Only split if the plan actually crosses the
threshold. The thresholds exist to catch overscoping, not to enforce a
shape.

---

## 6. Implementation signposts for the planner

These are guidance notes for the session that writes `PLAN.md`; they are not
themselves design choices, but they flag predictable planner questions so the
planner can resolve them in-plan rather than mid-execution.

1. **Argv parsing is hand-rolled.** `clap` is not on the D-3.2
   permitted-foundations list and adopting it would require a new ADR. Phase
   00 only needs `-c <path>` / `--config-path <path>`; hand-rolling in ~20
   lines is cheaper than landing an ADR. When argv grows (expected phase 01 or
   08), the question is revisited with a proper ADR.

2. **`cargo deny check` might flip red when crates land.** `tokio` pulls in
   `mio`, which is MIT/Apache. `testcontainers` pulls in `bollard`, which is
   Apache-2.0. `tracing` pulls in `tracing-core` which is MIT. The `deny.toml`
   allow-list covers these. But the transitive set is volatile; if a new
   license surfaces, either add it to the allow-list (with an ADR if it's
   weaker than Apache/MIT/BSD — e.g. MPL is already allowed, CDDL would need
   deliberation) or file a `exceptions` entry with justification.

3. **`tests/differential/` is **not** a unit of `cargo test`'s default run
   discovery unless it is a workspace member.** It must be listed in the root
   `Cargo.toml` `[workspace] members`. The crate lives at `tests/differential`
   but it is a *normal* crate with a test target; `tests/` is just a directory
   name, not Cargo's implicit integration-test directory.

4. **The harness must not read upstream Envoy source** to derive
   expectations. All expectations are encoded declaratively in
   `expectations.yaml` and interpreted by the harness. If a future fixture
   needs an assertion the harness cannot express, the grammar of
   `expectations.yaml` is extended and `BEHAVIOR_CONTRACT.md` documents the
   new dimension — per doctrine D-3.3.

5. **The harness entrypoint is designed for reuse.** Phase 02's TCP proxy
   fixture must attach as `tests/fixtures/0002-tcp-proxy/` with no harness
   changes — only a new expectations.yaml and possibly a new equivalence
   predicate in `BEHAVIOR_CONTRACT.md`.

6. **Port selection.** Fixtures template `{{PORT}}` rather than hardcoding,
   so tests can run on any free port in CI. The harness picks the port
   (e.g. by binding `:0`, querying the assigned port, closing, and passing
   that port to both proxies — with the standard TOCTOU caveat accepted for
   a pre-production harness; if flakes materialize, revisit with a port-range
   reservation strategy).

7. **Graceful-drain on SIGTERM/SIGINT** is a phase-00 deliverable (D3 step 5)
   because it's the minimum the harness needs to terminate envoy-rust cleanly
   between test runs. It is *not* the full Envoy hot-restart / graceful-drain
   semantics of phase 08 — just accept-loop shutdown + 5s drain.

---

## 7. Artifacts this phase produces

Created during execution (relative to the repo root):

- `docs/envoy-rust/phases/00-bootstrap/PLAN.md`
- `docs/envoy-rust/phases/00-bootstrap/PROGRESS.md`
- `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`
- `.github/workflows/ci.yml`
- `crates/envoy-bin/Cargo.toml`
- `crates/envoy-bin/src/main.rs`
- `tests/differential/Cargo.toml`
- `tests/differential/src/lib.rs`
- `tests/differential/src/harness.rs` (optional — extract of `lib.rs`)
- `tests/differential/tests/echo.rs`
- `tests/fixtures/0001-tcp-echo/envoy.yaml`
- `tests/fixtures/0001-tcp-echo/envoy-rust.yaml`
- `tests/fixtures/0001-tcp-echo/inputs/payload.bin`
- `tests/fixtures/0001-tcp-echo/expectations.yaml`
- `tests/fixtures/0001-tcp-echo/README.md`

Amended during execution:

- Root `Cargo.toml` — workspace members list.
- `docs/envoy-rust/ENVOY_TARGET.md` — resolved pin.
- `docs/envoy-rust/DECISIONS.md` — ADR-0002, ADR-0003, ADR-0004 appended.
- `docs/envoy-rust/ROADMAP.md` — row 00 status → `done`.
- `docs/envoy-rust/STATE.md` — active → `01-static-bootstrap-config`, next-skill → `superpowers:brainstorming`.

---

## 8. Final commit message format (for state 6 of the lifecycle)

```
phase 00: Bootstrap [ADR-0002, ADR-0003, ADR-0004]

Workspace skeleton, CI, toolchain + deny policy, reference Envoy pin,
differential harness skeleton, and a TCP echo fixture exercised against
upstream envoyproxy/envoy:v1.33.0.

Differential surface: tests/fixtures/0001-tcp-echo green (byte-exact body).
Conformance: none.
```
