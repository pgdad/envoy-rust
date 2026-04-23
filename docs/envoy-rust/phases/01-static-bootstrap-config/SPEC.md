# Phase 01 — Static bootstrap config loader

- **Phase id:** `01`
- **Title:** Static bootstrap config loader (node, admin, static_resources skeleton)
- **Depends on:** `00` (done as of commit `e5afc35`).
- **Differential surface when done:** config parses; admin `/ready` behaves like Envoy. Fixture `0002-static-admin-ready` green against upstream `envoyproxy/envoy:v1.33.0`. Fixture `0001-tcp-echo` remains green (§7.5.b of `BOOTSTRAP_PROMPT.md`).
- **Seeded by:** `BOOTSTRAP_PROMPT.md` §8 row 01; `ROADMAP.md` row 01.

This spec is the design contract for phase 01. The next session converts it
into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` /
`SKILL_ROUTING.md`). It is intentionally concrete enough to be turned into a
plan by a stranger with zero prior context per doctrine D-3.4.

---

## 1. Goal and acceptance signal

**Goal.** Extend envoy-rust so that a realistic upstream Envoy static
bootstrap — with `node`, `admin`, and a `static_resources` skeleton — parses
cleanly, and envoy-rust starts an admin HTTP endpoint that responds to
`GET /ready` behaviorally-equivalent to upstream Envoy on the same input.
The phase also ships the project's first coverage-guided fuzz target,
scheduled by phase-00 signpost §6.2 ("first fuzz target lands with phase 01
(config parser)").

**Acceptance signal** — the phase-done gate from §7.5 of
`BOOTSTRAP_PROMPT.md`, scoped to this phase's feature surface:

- (a) the new differential fixture `tests/fixtures/0002-static-admin-ready/` is green;
- (b) the pre-existing differential fixture `tests/fixtures/0001-tcp-echo/` remains green, after a mechanical migration of its `expectations.yaml` to the tagged-driver grammar introduced in this phase (§D5);
- (c) no conformance suites run this phase (first one — `h2spec` — attaches in phase 05);
- (d) the new fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`-max_total_time=30`) on a dedicated nightly CI job;
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- (f) `REVIEW.md` for this phase is approved.

---

## 2. Behavior-contract scope for phase 01

Phase 01 exercises two equivalence dimensions from `BEHAVIOR_CONTRACT.md`
§7.2 / equivalent in `MISSION.md`:

- **Response status — Exact** (row 1 of the matrix). First use in the project.
- **Response body — Byte-exact for deterministic handlers** (row 2, continued from phase 00).

Phase 01 does **not** assert header equivalence on `/ready`. The
`BEHAVIOR_CONTRACT.md` `Header allow-list` subsection — marked "populated
starting phase 04" at bootstrap — stays empty at the end of phase 01.
Rationale is recorded as ADR-0011 (§D7); in short, phase 04 is where the
HTTP/1.1 data-plane surfaces all the response-header questions worth
answering, and populating a phase-01-only stub allow-list would be
churn. envoy-rust still emits a reasonable Envoy-shaped header block on
every admin response for forward-compat (§D3); the harness simply ignores
headers in phase 01.

No new rows, no new allow-list entries, no new mappings are added to
`BEHAVIOR_CONTRACT.md` in phase 01. The currently-empty subsections
(`Header allow-list`, `Stat-name mapping`, `Access log field mapping`,
`xDS wire state machine`, `Timing tolerances`) remain empty.

---

## 3. Deliverables

### D1 — New library crate `crates/envoy-config/`

Added to the root `Cargo.toml` `[workspace] members`. Owns the full
`Bootstrap` type tree and the parser.

- `crates/envoy-config/Cargo.toml`. `edition = "2024"`, `publish = false`,
  `license = "Apache-2.0"`. Dependencies from the D-3.2 permitted-foundations
  list only: `serde` (with `derive`), `serde_yaml`, `thiserror`.
- `crates/envoy-config/src/lib.rs` starts with `#![forbid(unsafe_code)]` per
  D-3.8. Re-exports the bootstrap module and the public surface:

    ```rust
    pub mod bootstrap;
    pub use bootstrap::*;

    pub const ECHO_FILTER: &str = "envoy.filters.network.echo";

    pub fn parse_bootstrap(yaml: &str) -> Result<Bootstrap, ConfigError>;

    #[derive(Debug, thiserror::Error)]
    pub enum ConfigError {
        #[error("parsing bootstrap YAML")]
        Yaml(#[from] serde_yaml::Error),
        #[error("bootstrap configures neither an admin endpoint nor a listener; envoy-rust has nothing to do")]
        NoRuntime,
        #[error("bootstrap has {0} listeners; phase 01 supports at most one")]
        TooManyListeners(usize),
        #[error("unsupported network filter '{0}'; envoy-rust accepts only '{1}'")]
        UnsupportedFilter(String, &'static str),
    }
    ```

- `crates/envoy-config/src/bootstrap.rs` hosts the type tree. All structs
  derive `Debug` + `Deserialize` and apply `#[serde(deny_unknown_fields)]`
  **except** `Node`. The shape:

    ```rust
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Bootstrap {
        #[serde(default)]
        pub node: Option<Node>,
        #[serde(default)]
        pub admin: Option<Admin>,
        #[serde(default)]
        pub static_resources: StaticResources,
    }

    #[derive(Debug, Deserialize)]
    // NOTE: Node deliberately omits deny_unknown_fields — Envoy's Node also
    // carries metadata, locality, user_agent_*, extensions, client_features,
    // listening_addresses, dynamic_parameters. Phase 01 accepts id + cluster
    // and silently ignores the rest. When xDS (§9 family) lands, Node is
    // either moved or tightened under a new ADR that names the fields then
    // semantically load-bearing.
    pub struct Node {
        pub id: String,
        pub cluster: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Admin {
        pub address: Address,
    }

    #[derive(Debug, Default, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct StaticResources {
        #[serde(default)]
        pub listeners: Vec<Listener>,
        #[serde(default)]
        pub clusters: Vec<Cluster>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Cluster {
        pub name: String,
        // Phase 02 extends with type, lb_policy, load_assignment, etc.
    }

    // Listener, Address, SocketAddress, FilterChain, NetworkFilter carry
    // forward verbatim from phase 00's crates/envoy-bin/src/config.rs.
    ```

- `validate()` relaxations vs. phase 00:
  - `listeners.len() ∈ {0, 1}` (phase 00 required exactly one; phase 01's
    admin-only fixture has zero). Two or more still rejects as
    `ConfigError::TooManyListeners`.
  - If `admin.is_none() && listeners.is_empty()` → reject as
    `ConfigError::NoRuntime`. Prevents a silent no-op startup.
  - Per-filter `ECHO_FILTER` check (rejecting `tcp_proxy`, etc.) carries
    forward unchanged.
  - `clusters` accepted with only `name`; no further validation (phase 02).

- Unit tests in `crates/envoy-config/src/bootstrap.rs`:
  - All seven tests from phase-00's `crates/envoy-bin/src/config.rs::tests`
    move in unchanged (`parses_minimal_bootstrap`, `rejects_non_echo_filter`,
    `rejects_empty_listeners` — renamed `rejects_empty_listeners_with_no_admin`
    to reflect the new validation rule, `rejects_multiple_listeners`,
    `rejects_malformed_yaml`, `rejects_unknown_bootstrap_field`,
    `rejects_unknown_listener_field`).
  - Seven new tests:
    - `parses_bootstrap_with_node_admin_empty_resources`
    - `parses_bootstrap_with_admin_only`  — zero listeners, admin present
    - `parses_bootstrap_with_clusters_stub`
    - `rejects_bootstrap_with_neither_admin_nor_listener` — `NoRuntime`
    - `rejects_unknown_admin_field` — `deny_unknown_fields` regression
    - `rejects_unknown_cluster_field` — `deny_unknown_fields` regression
    - `accepts_node_with_unmodeled_field` — proves Node's openness is intentional

`crates/envoy-bin/src/config.rs` is deleted in this phase.
`crates/envoy-bin/src/main.rs`'s `mod config;` becomes `use envoy_config::{...};`.

### D2 — Fuzz subcrate `crates/envoy-config/fuzz/`

Standard cargo-fuzz layout. **Workspace-excluded** (not in root
`Cargo.toml`'s `[workspace] members`; added to `[workspace] exclude`).

- `crates/envoy-config/fuzz/Cargo.toml`:

    ```toml
    [package]
    name = "envoy-config-fuzz"
    version = "0.0.0"
    edition = "2024"
    publish = false
    license = "Apache-2.0"

    [package.metadata]
    cargo-fuzz = true

    [dependencies]
    libfuzzer-sys = "0.4"
    envoy-config  = { path = ".." }

    [[bin]]
    name = "parse_bootstrap"
    path = "fuzz_targets/parse_bootstrap.rs"
    test = false
    doc = false
    bench = false
    ```

- `crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`:

    ```rust
    #![no_main]
    use libfuzzer_sys::fuzz_target;

    fuzz_target!(|data: &[u8]| {
        if let Ok(s) = std::str::from_utf8(data) {
            let _ = envoy_config::parse_bootstrap(s);
        }
    });
    ```

- `crates/envoy-config/fuzz/.gitignore`: `corpus/`, `artifacts/`.
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/minimal.yaml`: one seed
  file — the phase-00 fixture `envoy.yaml` with `{{PORT}}` replaced by a
  constant (e.g. `10000`). Gives the fuzzer a structurally-valid starting
  point so its mutations exercise real YAML and real Bootstrap shape.

The fuzz crate's root file (`fuzz_targets/parse_bootstrap.rs`) begins with
`#![no_main]`. Per D-3.8, `#![forbid(unsafe_code)]` is not written in a
`#![no_main]` crate root in the idiomatic cargo-fuzz layout; however, the
only `unsafe` in scope lives inside `libfuzzer-sys` (a permitted dev
dependency per ADR-0009) — no `unsafe` appears in our fuzz target code. If
clippy flags the missing forbid, pin an `#![forbid(unsafe_code)]` in
`fuzz_targets/parse_bootstrap.rs` directly.

### D3 — Admin HTTP endpoint in `envoy-bin`

New module `crates/envoy-bin/src/admin.rs`, public API mirrors
`echo::serve`:

```rust
pub async fn serve(listener: TcpListener, shutdown: impl Future<Output=()> + Send + 'static) -> Result<()>;
```

Implementation contract:

1. `serve` loops on `listener.accept()`, spawning each connection onto a
   `tokio::task::JoinSet` alongside the `shutdown` future. Matches
   `echo::serve`'s shape so the drain semantics are identical (5 s grace,
   timeout-abort) and reviewers can diff the two modules side by side.

2. Per-connection handler:
   1. Read request bytes into a growing `Vec<u8>` capped at 8 KiB. Repeatedly
      feed the buffer to `httparse::Request::parse`. Stop on
      `Status::Complete(n)` (request-head parsed), on buffer full (reply
      `431 Request Header Fields Too Large`, close), or on EOF mid-request
      (silent close).
   2. Dispatch on `(method, path)`:
      - `("GET", "/ready")` → `200 OK` with body `LIVE\n` (5 bytes).
      - any other `(method, path)` → `404 Not Found` with body
        `invalid path. admin commands are:\n  /ready\n`.
   3. Write the response, close the socket. `Connection: close` always; no
      keep-alive, no pipelining in phase 01 (that lives in phase 04's HCM).

3. Response framing is hand-rolled. A free helper
   `fn render_response(status: u16, reason: &str, body: &[u8]) -> Vec<u8>`
   emits:

    ```
    HTTP/1.1 {status} {reason}\r\n
    content-type: text/plain\r\n
    content-length: {body.len()}\r\n
    cache-control: no-cache, max-age=0\r\n
    x-content-type-options: nosniff\r\n
    server: envoy-rust\r\n
    date: {rfc7231_imf_fixdate(SystemTime::now())}\r\n
    connection: close\r\n
    \r\n
    {body bytes}
    ```

   `rfc7231_imf_fixdate` is hand-rolled in ~15 lines over
   `std::time::SystemTime::now()` → seconds-since-epoch → calendar split.
   Neither `chrono` nor `time` lands in phase 01 (not on D-3.2; we avoid the
   ADR cost until a phase genuinely needs them).

4. IO errors are logged at `warn!` and the connection is dropped. No retry.

5. Shutdown observance: `shutdown` is a future that resolves on SIGTERM /
   SIGINT. `serve` stops `accept()`ing once it fires and waits up to 5 s for
   in-flight connections (`JoinSet::join_next` with `tokio::time::timeout`),
   then aborts the remainder and returns.

6. `server:` header value is `envoy-rust`, deliberately **not** `envoy`. This
   diverges from upstream but does not break the fixture because phase 01
   asserts status + body only (ADR-0011). When phase 04 populates the header
   allow-list, `server` lands on it.

7. Unit tests in `crates/envoy-bin/src/admin.rs::tests`:
   - `serves_ready_live` — ephemeral port, raw TCP GET, assert `200` +
     `LIVE\n`.
   - `404s_unknown_path` — `GET /does-not-exist` → 404.
   - `404s_non_get_ready` — `POST /ready` → 404.
   - `rejects_oversized_request_headers` — >8 KiB header → 431.
   - `drain_exits_within_budget` — fire shutdown with one in-flight
     connection, assert `serve` returns within ~5 s.

What this phase deliberately does **not** implement on the admin endpoint:
chunked requests, HTTP/1.0, pipelining, `Expect: 100-continue`, CONNECT /
OPTIONS / TRACE methods, TLS on admin (not an upstream default), and the
phase-08 admin endpoints (`/stats`, `/clusters`, `/config_dump`,
`/server_info`, `/drain_listeners`). All of the above are phase 04 or phase
08 work.

### D4 — Binary entrypoint wiring

- `crates/envoy-bin/src/argv.rs` — new module, hosts `ArgvError` +
  `parse_argv` + `argv_tests`. Currently these live inline in `main.rs`; the
  extraction is a cleanup that sizes `main.rs` back to an orchestrator.
  (If the planner judges extraction churn outweighs the clarity win, leaving
  argv in `main.rs` is acceptable — spec-level guidance only.)

- `crates/envoy-bin/src/main.rs::run()` becomes:

    ```rust
    async fn run(config_path: PathBuf) -> Result<()> {
        let yaml = fs::read_to_string(&config_path).with_context(...)?;
        let bootstrap = envoy_config::parse_bootstrap(&yaml)?;

        if let Some(node) = bootstrap.node.as_ref() {
            tracing::info!(node.id = %node.id, node.cluster = %node.cluster,
                           "node registered");
        }

        let token = tokio_util::sync::CancellationToken::new();
        let signal_token = token.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            signal_token.cancel();
        });

        let mut set = tokio::task::JoinSet::new();

        if let Some(listener_cfg) = bootstrap.static_resources.listeners.first() {
            let addr = resolve_socket(&listener_cfg.address)?;
            let lst  = TcpListener::bind(addr).await.with_context(...)?;
            tracing::info!(%addr, "echo listener");
            let shutdown = token.clone();
            set.spawn(echo::serve(lst, async move { shutdown.cancelled().await }));
        }

        if let Some(admin_cfg) = bootstrap.admin.as_ref() {
            let addr = resolve_socket(&admin_cfg.address)?;
            let lst  = TcpListener::bind(addr).await.with_context(...)?;
            tracing::info!(%addr, "admin listener");
            let shutdown = token.clone();
            set.spawn(admin::serve(lst, async move { shutdown.cancelled().await }));
        }

        while let Some(res) = set.join_next().await {
            res.context("task panicked")??;
        }
        Ok(())
    }
    ```

- `tokio-util` is added to `crates/envoy-bin/Cargo.toml` for
  `CancellationToken` (permitted foundation per D-3.2). No feature flags
  needed beyond defaults.

- `echo::serve`'s signature stays as it is (takes `impl Future<Output=()>`);
  we materialize a cancellable future from the token at the call site.

- Exit codes unchanged from phase 00: `0` clean, `1` runtime error, `2`
  argv error. A `ConfigError` surfaces via `anyhow` → exit `1`.

- New integration test under `crates/envoy-bin/tests/admin_only.rs`: write
  a temp config with admin-only (zero listeners), spawn
  `env!("CARGO_BIN_EXE_envoy-bin")` with `-c <temp>`, open a TCP connection
  to the admin port, send `GET /ready`, assert response. This is a
  backstop — the differential fixture is the real contract.

### D5 — Differential harness grammar extension

- `tests/differential/src/lib.rs::Expectations` gains a tagged `driver`
  discriminator. Backward compatibility with phase 00 is a one-line migration
  to the 0001 fixture (§D6 below).

    ```rust
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Expectations {
        pub driver: Driver,
        pub equivalence: Equivalence,
    }

    #[derive(Debug, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    pub enum Driver {
        TcpEcho,
        HttpGet { path: String, host: String },
    }

    #[derive(Debug, Deserialize, Default)]
    #[serde(deny_unknown_fields)]
    pub struct Equivalence {
        #[serde(default)]
        pub response_status: Option<StatusRule>,
        #[serde(default)]
        pub response_body: Option<BodyRule>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "snake_case", deny_unknown_fields)]
    pub enum StatusRule { Exact }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "snake_case", deny_unknown_fields)]
    pub enum BodyRule { ByteExact }
    ```

- New harness helper appended to `tests/differential/src/lib.rs`:

    ```rust
    pub struct HttpResponse {
        pub status: u16,
        pub body: Vec<u8>,
        // headers captured for debug tracing but not part of equivalence
        // in phase 01 (ADR-0011).
    }

    pub async fn drive_http_get(
        addr: SocketAddr,
        path: &str,
        host: &str,
    ) -> Result<HttpResponse>;
    ```

  Implementation: open TCP, write
  `"GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"`,
  read bytes into a growing `Vec<u8>`, feed to `httparse::Response::parse`
  until `Status::Complete(n)`. Capture `status`. If `content-length` header
  is present, read exactly that many further bytes; otherwise, assuming
  `connection: close`, read to EOF. Return.

  `httparse` is added to `tests/differential/Cargo.toml` dependencies;
  permitted per D-3.2 ("HTTP/1.1 tokenizer, used as a parser only"). No ADR.

- `run_fixture` dispatches on `driver.kind`:
  - `TcpEcho` → existing code path unchanged (`drive_tcp` + ADR-0007
    trailing-byte poll on both proxies; byte-compare).
  - `HttpGet { path, host }` → `drive_http_get` on both proxies, compare
    per `equivalence`:
    - `response_status: exact` → assert equal status codes.
    - `response_body: byte_exact` → assert equal body bytes.
    If neither rule is configured, the compare is a no-op (degenerate; the
    validator in §D3 here could reject, but for simplicity it silently
    passes and logs a warning).

- Template substitution (`render_yaml` helper) learns an extra key so
  fixtures can template `{{ADMIN_PORT}}` independently of `{{PORT}}`:
  - `TcpEcho` driver → harness reserves 1 port, substitutes `{{PORT}}`.
  - `HttpGet` driver → harness reserves 1 port, substitutes `{{ADMIN_PORT}}`.
  The map is driver-keyed and kept tiny; future drivers may add their own
  port keys.

- New harness unit tests in `tests/differential/src/lib.rs::tests`:
  - `drive_http_get_round_trips` — spawn a tiny local HTTP server that
    writes a canned response; assert parsed status + body.
  - `drive_http_get_handles_content_length` — explicit `content-length`.
  - `drive_http_get_handles_connection_close_without_length` — server
    writes no `content-length`, closes after body; harness returns full
    body.
  - `drive_http_get_rejects_malformed_response` — server writes garbage;
    harness returns `Err` with a useful message.
  - `fixture_0001_expectations_parses_as_tcp_echo` — structural assertion
    that `tests/fixtures/0001-tcp-echo/expectations.yaml` loads and
    `driver` discriminates as `Driver::TcpEcho`. Proves the migration.

### D6 — Fixture migration: `tests/fixtures/0001-tcp-echo/`

`expectations.yaml` migrates from the phase-00 shape:

```yaml
equivalence:
  response_body: byte_exact
```

to the phase-01 tagged-driver shape:

```yaml
driver:
  kind: tcp_echo
equivalence:
  response_body: byte_exact
```

The payload, fixture config, `README.md`, and acceptance test are unchanged.
The `README.md` is supplemented with one line noting the driver-tag
migration and crossref to phase-01's ADR set.

### D7 — Fixture `tests/fixtures/0002-static-admin-ready/`

Files:

- `envoy.yaml`:

    ```yaml
    node:
      id: envoy-rust-phase-01-subject
      cluster: envoy-rust-phase-01

    admin:
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {{ADMIN_PORT}}

    static_resources:
      listeners: []
      clusters: []
    ```

  If Envoy v1.33.0 rejects this as-is (some admin schemas require
  `access_log_path`), the fixture supplies `access_log_path: /dev/null` and
  the fixture `README.md` records the divergence. Phase-01 plan execution
  resolves this against the real upstream container, not at planning time.

- `envoy-rust.yaml`:

    ```yaml
    node:
      id: envoy-rust-phase-01-subject
      cluster: envoy-rust-phase-01

    admin:
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {{ADMIN_PORT}}

    static_resources:
      listeners: []
      clusters: []
    ```

  Divergence from `envoy.yaml` is the bind address
  (`0.0.0.0` container vs. `127.0.0.1` local subprocess). The harness
  templates `{{ADMIN_PORT}}` side-specifically (host-mapped port for the
  container side, host-reserved port for the subject side) — same pattern
  as phase-00's `{{PORT}}` template. No per-fixture ADR needed; this is
  harness mechanics.

- `expectations.yaml`:

    ```yaml
    driver:
      kind: http_get
      path: /ready
      host: envoy-rust-phase-01
    equivalence:
      response_status: exact
      response_body: byte_exact
    ```

- No `inputs/` directory. The `http_get` driver is declarative (path + host
  live in `expectations.yaml`), so no payload file is needed. Phase-00's
  fixture still has `inputs/payload.bin` because its `tcp_echo` driver is
  payload-driven; per-driver asymmetry is intentional.

- `README.md` — one paragraph:

    > This fixture drives `GET /ready` at the admin endpoint of upstream
    > Envoy and envoy-rust, asserting that both return identical HTTP status
    > and response body. Header equivalence is intentionally out of scope
    > for phase 01 — the `BEHAVIOR_CONTRACT.md` header allow-list is
    > populated starting phase 04 per ADR-0011. This is the first
    > differential fixture to exercise the admin endpoint; subsequent
    > admin fixtures (phase 08 for `/stats`, `/clusters`, `/config_dump`,
    > and drain) reuse the `http_get` driver introduced here.

### D8 — CI workflow extension

`.github/workflows/ci.yml` gains a **second job** `fuzz`, running in
parallel with the existing stable-toolchain job (which we rename from the
default single job to `build` for clarity — stable `build/clippy/fmt/
test/deny-check` flow is untouched).

```yaml
jobs:
  build:            # existing job, unchanged except rename
    ...
  fuzz:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly
        with: { components: rust-src }
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: "crates/envoy-config/fuzz -> target"
      - run: cargo install cargo-fuzz --locked
      - run: cargo fuzz run parse_bootstrap -- -max_total_time=30
        working-directory: crates/envoy-config
```

30 s budget per `BOOTSTRAP_PROMPT.md` §5 state 4 ("short-budget CI run").
Any crash found (panic, sanitizer hit) fails the job. A future phase may
add a scheduled long-budget nightly CRON; that is out of scope for phase
01.

### D9 — ADRs to land during execution

Four ADRs, appended to `docs/envoy-rust/DECISIONS.md` in order:

- **ADR-0008 — Extract `envoy-config` as a library crate.** Motivation: the
  parser is the long-horizon seam that every subsequent phase (02–08 and
  the xDS family) mutates; cargo-fuzz requires a library crate parent
  (`envoy-bin` is a binary). Options considered: keep inline in `envoy-bin`
  with a fuzz-bin hack; extract `envoy-config` only; extract
  `envoy-config` + `envoy-admin` now. Decision: `envoy-config` only;
  `envoy-admin` extraction deferred to phase 08 when a real router exists.

- **ADR-0009 — Permit `cargo-fuzz` + `libfuzzer-sys` as fuzz-only dev
  dependencies.** Motivation: D-3.2's permitted-foundations list does not
  include a fuzzer; phase-00 SPEC §6 signpost scheduled the first fuzz
  target for phase 01; the project will use cargo-fuzz repeatedly
  (HTTP/1.1 in phase 04, HTTP/2 in phase 05, protobuf family, etc.).
  Options considered: cargo-fuzz + libfuzzer-sys; `afl.rs`;
  `honggfuzz-rs`; proptest-only (not coverage-guided, different tool).
  Decision: cargo-fuzz + libfuzzer-sys, dev-tooling only, never linked
  into `envoy-bin` or `tests/differential`. The fuzz subcrate is
  workspace-excluded per ADR-0010.

- **ADR-0010 — Nightly Rust toolchain, fuzz-only invocation.** Motivation:
  D-3.9 pins `rust-toolchain.toml` to stable 1.95.0; cargo-fuzz requires
  nightly for sanitizer `-Z` flags. Options considered: bump
  `rust-toolchain.toml` to nightly (rejected — breaks D-3.9 for mainline);
  add a nightly `rust-toolchain.toml` inside `crates/envoy-config/fuzz/`
  (rejected — that crate is workspace-excluded, brittle interaction with
  cargo's toolchain-override); `cargo-bolero` or other stable-wrapper
  (rejected — still nightly for coverage-guided mode); explicit
  `cargo +nightly fuzz run ...` invocation with no repo-level nightly pin.
  Decision: explicit `+nightly` invocation, installed in a dedicated CI
  job. The stable pin at root is untouched. D-3.9 is undisturbed for every
  `cargo build` and `cargo test` path.

- **ADR-0011 — Phase 01 defers response-header equivalence to phase 04.**
  Motivation: `BEHAVIOR_CONTRACT.md`'s `Header allow-list` subsection is
  marked "populated starting phase 04" at bootstrap. Phase 01 is the first
  phase that returns an HTTP response (admin `/ready`). ROADMAP's phase-01
  summary — "config parses; admin `/ready` behaves like Envoy" — is silent
  on whether header equivalence is in scope. Options considered: populate
  a phase-01 stub allow-list (ignore `server`, `date`) and assert the
  rest; assert full headers with a fresh allow-list (ADR-heavy); assert
  status + body only. Decision: assert status + body only. envoy-rust
  still emits a reasonable Envoy-shaped header block on every admin
  response for forward compat (the `server: envoy-rust` divergence is
  tolerated until phase 04 populates the allow-list). The harness ignores
  headers in phase 01.

Additional ADRs may be required during execution if Envoy v1.33.0's admin
schema rejects the fixture YAML as-is (e.g. demands `access_log_path`), or
if cargo-deny flips red on the `libfuzzer-sys` license surface. Handle per
doctrine D-3.5 when encountered.

---

## 4. Non-goals (deferred to later phases)

- Dynamic config / xDS — §9 family.
- TLS on admin — not an upstream default; never in scope.
- `/stats`, `/clusters`, `/config_dump`, `/server_info`, `/drain_listeners`,
  `/healthcheck/fail` — phase 08.
- HTTP/1.1 data-plane — phase 04. Admin's HTTP framing in phase 01 is
  intentionally scoped to `/ready` only and is not reused by the
  HCM.
- Keep-alive, pipelining, HTTP/1.0 fallback on admin — never (admin is
  `Connection: close`).
- Header equivalence in the differential contract — phase 04 (ADR-0011).
- Listener manager, cluster manager, TCP proxy filter, load balancing —
  phase 02.
- Filter chain iteration protocol, per-route config, extension registry —
  phase 07.
- Access logs, stats subsystem, Prometheus endpoint — phase 06.
- Conformance suites — first one (`h2spec`) lands with phase 05.
- Long-budget nightly fuzz CRON — a future, scheduled phase.
- `crates/envoy-admin/` extraction — phase 08 (ADR-0008 defers).
- Non-UTF-8 input paths into the parser — production reads files via
  `std::fs::read_to_string` which returns UTF-8; the fuzz target gates on
  UTF-8 to match.

---

## 5. Splitting guidance for the planner

If `PLAN.md` crosses either §6 threshold (~25 tasks or ~1500 LoC of estimated
net change), split phase 01 at exactly this boundary:

- **01.1 — Config crate + fuzz target.** Extract `envoy-config` (D1), land
  the fuzz subcrate (D2), migrate `envoy-bin` to consume `envoy-config`,
  land ADR-0008/0009/0010. Acceptance: all three stable CI steps (build /
  clippy / test) remain green; fuzz CI job green; fixture `0001-tcp-echo`
  untouched (no grammar migration yet).

- **01.2 — Admin endpoint + differential grammar.** Ship `admin.rs` (D3),
  wire it into `envoy-bin::run` (D4), extend the harness grammar (D5),
  migrate fixture `0001` to the new driver tag (D6), add fixture `0002`
  (D7), land ADR-0011. Acceptance: fixtures `0001` and `0002` both green;
  the full phase-done gate passes.

Do **not** pre-emptively split. Only split if the plan actually crosses the
threshold. The thresholds exist to catch overscoping, not to enforce a
shape.

---

## 6. Implementation signposts for the planner

These are guidance notes for the session that writes `PLAN.md`; they are
not themselves design choices, but they flag predictable planner questions
so the planner can resolve them in-plan rather than mid-execution.

1. **`envoy-config` lives under `crates/`, not the root.** The root
   `Cargo.toml` is a workspace manifest; members listed by path. The fuzz
   subcrate lives at `crates/envoy-config/fuzz/` and is excluded from the
   workspace — `cargo +nightly fuzz run ...` is invoked from
   `crates/envoy-config/`, picks up the nested manifest via `cargo-fuzz`'s
   own discovery, and compiles outside the workspace members.

2. **`serde_yaml` is deprecated upstream.** The crate (`serde_yaml 0.9`) is
   in maintenance mode. For phase 01 we continue to use it (matches phase
   00 and ADR-0003's edition-2024 posture), but a future phase should
   consider migrating to `serde_yml` or `serde-yaml-ng` under an ADR. This
   is explicitly out of scope for phase 01 — flagging it here so the
   planner does not rat-hole.

3. **Admin listener port on `127.0.0.1` for local subject.** `envoy-rust.yaml`
   binds `127.0.0.1`, not `0.0.0.0`, deliberately: the subject subprocess
   runs as the test user on the host, and binding `0.0.0.0` on a dev
   workstation is a needless attack surface. The upstream container still
   binds `0.0.0.0` because testcontainers maps a host port into the
   container namespace.

4. **`httparse` cap on request headers is 8 KiB in phase 01.** Upstream
   Envoy defaults to `max_request_headers_kb: 60` on its HCM but the admin
   endpoint is a separate code path. Envoy's admin has no documented size
   cap; 8 KiB is a deliberate phase-01 choice that avoids unbounded memory
   growth without over-designing. If phase 08 surfaces a need, revisit.

5. **UTF-8 gate in the fuzz target is intentional.** `envoy-config::parse_bootstrap`
   takes `&str`. Production reads via `std::fs::read_to_string` (which
   fails on non-UTF-8). Mirroring that contract in the fuzzer keeps false
   positives out (bad bytes never reach `serde_yaml` in production). If a
   future phase adds a bytes-oriented parser path (e.g., `parse_bootstrap_bytes`),
   it ships its own fuzz target.

6. **No `chrono` / `time` crate this phase.** The `date:` header's
   IMF-fixdate format is hand-rolled over `SystemTime::now()` (~15 lines).
   Adding a date-time crate is an ADR surface; defer until a phase has a
   real need (access logs in phase 06, maybe).

7. **`tokio-util::sync::CancellationToken` is the idiom for multi-task
   shutdown.** The phase-00 `echo::serve` signature accepts
   `impl Future<Output=()>`. `CancellationToken::cancelled()` returns
   exactly that. `tokio-util` is on D-3.2; no ADR.

8. **`Node` is the only struct in the project without
   `deny_unknown_fields`.** This asymmetry is deliberate (D1 inline
   comment; ADR-0008 consequences). Code review should flag any drift that
   either removes `deny_unknown_fields` elsewhere or adds it to `Node`
   without a superseding ADR.

9. **The `drive_http_get` helper is fixed-shape.** It writes a minimal
   canonical `GET path HTTP/1.1\r\nHost: host\r\nConnection: close\r\n\r\n`
   and reads a full response. It does **not** support POST, request
   bodies, headers beyond Host + Connection, or chunked responses. When a
   future fixture needs any of that, extend the helper (and its tests) in
   the phase that introduces the need.

10. **`deny.toml` may need a fresh `exceptions` entry** if `libfuzzer-sys`
    pulls in a license not on the allow-list (historically Apache-2.0; verify
    during execution via `cargo deny check`). Handle per doctrine D-3.5.

---

## 7. Artifacts this phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/01-static-bootstrap-config/PLAN.md`
- `docs/envoy-rust/phases/01-static-bootstrap-config/PROGRESS.md`
- `docs/envoy-rust/phases/01-static-bootstrap-config/REVIEW.md`
- `crates/envoy-config/Cargo.toml`
- `crates/envoy-config/src/lib.rs`
- `crates/envoy-config/src/bootstrap.rs`
- `crates/envoy-config/fuzz/Cargo.toml`
- `crates/envoy-config/fuzz/.gitignore`
- `crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/minimal.yaml`
- `crates/envoy-bin/src/admin.rs`
- `crates/envoy-bin/src/argv.rs` (optional — see §D4 guidance)
- `crates/envoy-bin/tests/admin_only.rs`
- `tests/fixtures/0002-static-admin-ready/envoy.yaml`
- `tests/fixtures/0002-static-admin-ready/envoy-rust.yaml`
- `tests/fixtures/0002-static-admin-ready/expectations.yaml`
- `tests/fixtures/0002-static-admin-ready/README.md`

Amended during execution:

- Root `Cargo.toml` — add `crates/envoy-config` to `[workspace] members`; add
  `crates/envoy-config/fuzz` to `[workspace] exclude`.
- `crates/envoy-bin/Cargo.toml` — add `envoy-config = { path = "../envoy-config" }`;
  add `tokio-util` (default features); add `httparse`; drop `serde` and
  `serde_yaml` (now owned by `envoy-config`).
- `crates/envoy-bin/src/main.rs` — consume `envoy-config`; spawn admin task;
  use `CancellationToken`; optional `mod argv;` extraction.
- `crates/envoy-bin/src/config.rs` — deleted.
- `tests/differential/Cargo.toml` — add `httparse`.
- `tests/differential/src/lib.rs` — tagged `Driver` grammar; `drive_http_get`;
  `run_fixture` dispatch.
- `tests/fixtures/0001-tcp-echo/expectations.yaml` — add `driver: { kind: tcp_echo }`.
- `tests/fixtures/0001-tcp-echo/README.md` — one-line migration note.
- `.github/workflows/ci.yml` — rename single job to `build`; add parallel `fuzz` job.
- `docs/envoy-rust/DECISIONS.md` — ADR-0008, ADR-0009, ADR-0010, ADR-0011 appended.
- `docs/envoy-rust/ROADMAP.md` — row 01 status → `done`.
- `docs/envoy-rust/STATE.md` — active → `02-tcp-proxy` (slug consistent with §4 of `BOOTSTRAP_PROMPT.md`), next-skill → `superpowers:brainstorming`.
- `deny.toml` — `exceptions` or allow-list entries only if `cargo deny check` flags a new license on `libfuzzer-sys`'s transitive chain.

---

## 8. Final commit message format (for state 6 of the lifecycle)

```
phase 01: Static bootstrap config loader + admin /ready [ADR-0008, ADR-0009, ADR-0010, ADR-0011]

The new envoy-config crate extracts and extends the Bootstrap schema (node,
admin, static_resources skeleton). envoy-bin gains a hand-rolled admin HTTP
endpoint serving GET /ready. The project's first cargo-fuzz target ships over
parse_bootstrap, invoked nightly-only in a dedicated CI job.

Differential surface: tests/fixtures/0001-tcp-echo green (post driver-tag migration);
  tests/fixtures/0002-static-admin-ready green (status + body equivalence on GET /ready).
Conformance: none.
```
