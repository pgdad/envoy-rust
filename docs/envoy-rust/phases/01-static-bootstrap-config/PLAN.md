# Phase 01 — Static Bootstrap Config Loader Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task (fresh subagent per task + two-stage review). Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **For every code-writing task:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — failing test first, verify fails, minimal implementation, verify passes, commit. No exceptions (doctrine D-3.1).
>
> **Source of truth:** `docs/envoy-rust/phases/01-static-bootstrap-config/SPEC.md`. This plan operationalizes SPEC §§D1–D9. Where the plan and the SPEC disagree, the SPEC wins — flag the drift, land an ADR per D-3.5, and continue.

**Goal:** Ship the `envoy-config` crate (full `Bootstrap` type tree + fuzz target), a hand-rolled admin HTTP endpoint serving `GET /ready`, and the differential-harness grammar extension that lets fixture `0002-static-admin-ready` assert status + body equivalence against upstream Envoy `v1.33.0`.

**Architecture:** A new library crate `crates/envoy-config/` owns parsing. `envoy-bin` loses `src/config.rs` and gains `src/admin.rs` — a minimal `tokio` accept-loop that `httparse`s each request, dispatches `(method, path)` to a hand-rolled `HTTP/1.1` responder with a `Connection: close` framing, and drains under a `tokio-util::sync::CancellationToken` shared with the existing echo listener. The differential harness grows a tagged `Driver` enum (`tcp_echo` | `http_get`) so fixture `0001` migrates declaratively and fixture `0002` plugs in without harness-shape churn.

**Tech stack:** Rust edition 2024 on pinned stable `1.95.0` (D-3.9); `tokio` + `tokio-util` for async and cancellation; `httparse` as an HTTP/1.1 tokenizer (new direct dep for `envoy-bin` and `tests/differential`, permitted by D-3.2); `serde` + `serde_yaml` for parsing; `thiserror` for the new `ConfigError` enum; `cargo-fuzz` + `libfuzzer-sys` nightly-only for coverage-guided parser fuzzing (dev tooling; landed under ADR-0009/0010).

---

## File structure (created / modified / deleted)

**Created:**
- `crates/envoy-config/Cargo.toml`
- `crates/envoy-config/src/lib.rs`
- `crates/envoy-config/src/bootstrap.rs`
- `crates/envoy-config/fuzz/Cargo.toml`
- `crates/envoy-config/fuzz/.gitignore`
- `crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/minimal.yaml`
- `crates/envoy-bin/src/admin.rs`
- `crates/envoy-bin/src/argv.rs`
- `crates/envoy-bin/tests/admin_only.rs`
- `tests/fixtures/0002-static-admin-ready/envoy.yaml`
- `tests/fixtures/0002-static-admin-ready/envoy-rust.yaml`
- `tests/fixtures/0002-static-admin-ready/expectations.yaml`
- `tests/fixtures/0002-static-admin-ready/README.md`
- `tests/differential/tests/admin_ready.rs`
- `docs/envoy-rust/phases/01-static-bootstrap-config/PROGRESS.md` (appended by each task during execution)

**Modified:**
- Root `Cargo.toml` — add `crates/envoy-config` to `[workspace] members`; add `crates/envoy-config/fuzz` to `[workspace] exclude`.
- `crates/envoy-bin/Cargo.toml` — add `envoy-config`, `tokio-util`, `httparse`; drop `serde`, `serde_yaml`.
- `crates/envoy-bin/src/main.rs` — consume `envoy-config`; extract argv into `mod argv;`; add `mod admin;`; rewrite `run()` to spawn echo + admin under a shared `CancellationToken` + `JoinSet`.
- `tests/differential/Cargo.toml` — add `httparse`.
- `tests/differential/src/lib.rs` — tagged `Driver` enum; `drive_http_get`; dispatch in `run_fixture`; per-driver port templating.
- `tests/fixtures/0001-tcp-echo/expectations.yaml` — prepend `driver: { kind: tcp_echo }`.
- `tests/fixtures/0001-tcp-echo/README.md` — one-line migration note.
- `.github/workflows/ci.yml` — rename existing job to `build`; add parallel `fuzz` job.
- `docs/envoy-rust/DECISIONS.md` — append ADR-0008, ADR-0009, ADR-0010, ADR-0011.
- `docs/envoy-rust/ROADMAP.md` — flip phase-01 row `status` → `done` (at state 6 only).
- `docs/envoy-rust/STATE.md` — advance to phase 02 at state 1 (at state 6 only).
- `deny.toml` — only if `cargo deny check` flips on `libfuzzer-sys`'s transitive licenses; handle per D-3.5 (likely a new `exceptions` or `[licenses].allow` entry under an ADR-0012 if needed).

**Deleted:**
- `crates/envoy-bin/src/config.rs` (contents subsumed by `crates/envoy-config/src/bootstrap.rs`).

---

## Task index

Each task ends with a commit. PROGRESS.md gets a new section per task in the phase-00 style (task id, commit SHA, change summary, verification output, any deviation).

1. **ADRs 0008 / 0009 / 0010 — the config-crate-extraction + fuzz-tooling trio**
2. **Scaffold `crates/envoy-config/` skeleton + workspace member**
3. **`envoy-config::bootstrap` — full type tree (Bootstrap, Node, Admin, StaticResources, Cluster, Listener, Address, SocketAddress, FilterChain, NetworkFilter)**
4. **`envoy-config::{parse_bootstrap, ConfigError, validate}` — parsing + relaxed validation + the 14-test matrix (+ N2 closure tests)**
5. **Delete `envoy-bin/src/config.rs`; retarget `envoy-bin` at `envoy-config`**
6. **Scaffold `crates/envoy-config/fuzz/` subcrate + workspace-exclude + corpus seed**
7. **CI workflow: rename single job to `build`; add parallel `fuzz` job**
8. **ADR-0011 — defer response-header equivalence to phase 04**
9. **`envoy-bin::admin::{render_response, rfc7231_imf_fixdate}` — response framing + IMF-fixdate helper**
10. **`envoy-bin::admin::serve` — accept loop + per-connection handler + drain; 5 unit tests**
11. **Wire admin into `envoy-bin::main::run` — `CancellationToken` + `JoinSet`; `tests/admin_only.rs` integration test**
12. **Extract `argv.rs` from `main.rs`**
13. **Harness grammar: tagged `Driver` + refactored `Expectations`/`Equivalence`; regression tests**
14. **`tests/differential::drive_http_get` + `HttpResponse` + 4 unit tests**
15. **`run_fixture` dispatch on `Driver::{TcpEcho, HttpGet}` + per-driver port templating**
16. **Migrate `tests/fixtures/0001-tcp-echo/` to tagged driver grammar; regression-verify `echo_fixture`**
17. **Create `tests/fixtures/0002-static-admin-ready/` (envoy.yaml, envoy-rust.yaml, expectations.yaml, README.md)**
18. **`tests/differential/tests/admin_ready.rs` acceptance test**
19. **Phase-done gate (state 4): run all 5 stable commands + CI fuzz job; quote outputs into PROGRESS.md**

---

### Task 1: ADRs 0008 / 0009 / 0010 — config-crate-extraction + fuzz-tooling trio

**Files:**
- Modify (append): `docs/envoy-rust/DECISIONS.md`
- Create: `docs/envoy-rust/phases/01-static-bootstrap-config/PROGRESS.md`

**Why first:** every subsequent task cites at least one of these ADRs. DECISIONS.md is append-only per D-3.5; land the rationale before the code that references it (`crates/envoy-config` is justified by ADR-0008, the fuzz subcrate by ADR-0009, and the `+nightly` CI invocation by ADR-0010).

- [ ] **Step 1: Append ADR-0008 (`envoy-config` extraction) to `docs/envoy-rust/DECISIONS.md`.**

Append after the final `---` of ADR-0007 using the structure mandated by DECISIONS.md lines 9–19. Use these exact field contents (paraphrased from SPEC §D9 — keep the Options list to three items):

```markdown
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
```

- [ ] **Step 2: Append ADR-0009 (`cargo-fuzz` + `libfuzzer-sys` as dev-only tooling) to `docs/envoy-rust/DECISIONS.md`.**

```markdown
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
```

- [ ] **Step 3: Append ADR-0010 (nightly toolchain, fuzz-only invocation) to `docs/envoy-rust/DECISIONS.md`.**

```markdown
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
```

- [ ] **Step 4: Create `docs/envoy-rust/phases/01-static-bootstrap-config/PROGRESS.md` with a Task 1 section.**

Content:

```markdown
# Phase 01 Progress

## Task 1 — ADRs 0008 / 0009 / 0010 (2026-04-24)

- Commit: <SHA>
- Change: appended ADR-0008 (envoy-config crate extraction), ADR-0009 (cargo-fuzz + libfuzzer-sys as dev tooling), ADR-0010 (nightly toolchain for fuzz-only invocation) to DECISIONS.md.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 10 (ADR-0001 through ADR-0010).
```

Replace `<SHA>` with the actual commit hash after Step 6.

- [ ] **Step 5: Verify DECISIONS.md parses and the ADR sequence is intact.**

```bash
grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md
```
Expected output: `10`

```bash
grep -n '^## ADR-00' docs/envoy-rust/DECISIONS.md
```
Expected output (last 3 lines): `ADR-0008`, `ADR-0009`, `ADR-0010` in that order, with ascending line numbers.

- [ ] **Step 6: Commit.**

```bash
git add docs/envoy-rust/DECISIONS.md docs/envoy-rust/phases/01-static-bootstrap-config/PROGRESS.md
git commit -m "phase 01: ADR-0008/0009/0010 — envoy-config extraction + fuzz tooling"
```

Then patch PROGRESS.md's `<SHA>` placeholder to the commit hash and amend:

```bash
SHA=$(git rev-parse --short HEAD)
sed -i.bak "s/<SHA>/$SHA/" docs/envoy-rust/phases/01-static-bootstrap-config/PROGRESS.md
rm docs/envoy-rust/phases/01-static-bootstrap-config/PROGRESS.md.bak
git add docs/envoy-rust/phases/01-static-bootstrap-config/PROGRESS.md
git commit --amend --no-edit
```

(Amending is allowed here because the commit has not yet been pushed; this is the sole exception to the "prefer new commits" heuristic per the project's PROGRESS-SHA bookkeeping convention.)

---

### Task 2: Scaffold `crates/envoy-config/` skeleton + workspace member

**Files:**
- Create: `crates/envoy-config/Cargo.toml`
- Create: `crates/envoy-config/src/lib.rs`
- Create: `crates/envoy-config/src/bootstrap.rs` (empty module placeholder; fleshed out in Task 3)
- Modify: `Cargo.toml` (root)

**Why now:** tasks 3, 4, 5, 6 all need the crate to exist. This task lands the minimum that compiles cleanly so later tasks don't mix scaffolding with real code.

- [ ] **Step 1: Write `crates/envoy-config/Cargo.toml`.**

```toml
[package]
name = "envoy-config"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_config"
path = "src/lib.rs"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
thiserror = "1"
```

- [ ] **Step 2: Write `crates/envoy-config/src/lib.rs` as a compiling stub.**

```rust
#![forbid(unsafe_code)]

//! Phase 01 config surface for envoy-rust. Owns the `Bootstrap` type tree and
//! the `parse_bootstrap` entrypoint (fleshed out in Task 4). Phase 00's inline
//! parser in `crates/envoy-bin/src/config.rs` is superseded by this crate.
//!
//! See `docs/envoy-rust/DECISIONS.md` ADR-0008 for the extraction rationale.

pub mod bootstrap;
```

- [ ] **Step 3: Write `crates/envoy-config/src/bootstrap.rs` as an empty compiling module.**

```rust
//! Bootstrap schema — populated in Task 3.
```

- [ ] **Step 4: Add `crates/envoy-config` to the root workspace.**

Edit the root `Cargo.toml` `[workspace] members` list to read:

```toml
[workspace]
resolver = "2"
members = [
    "crates/envoy-bin",
    "crates/envoy-config",
    "tests/differential",
]
```

Task 6 adds `[workspace] exclude` for the fuzz subcrate — don't do it here.

- [ ] **Step 5: Verify the workspace builds cleanly.**

```bash
cargo build --workspace --all-targets
```
Expected: `Finished dev profile target(s) in …s` with a line `Compiling envoy-config v0.0.0 (…/crates/envoy-config)` in the output. No warnings, no errors.

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```
Expected: exit 0, `Finished`.

```bash
cargo fmt --all -- --check
```
Expected: exit 0, no diff.

```bash
cargo test --workspace
```
Expected: `envoy-config` contributes `test result: ok. 0 passed; 0 failed` (no tests yet); existing envoy-bin + differential tests continue to pass.

- [ ] **Step 6: Commit.**

```bash
git add Cargo.toml crates/envoy-config
git commit -m "phase 01: scaffold envoy-config crate [ADR-0008]"
```

Append a Task 2 section to PROGRESS.md with the commit SHA, a one-line summary, and the `cargo test --workspace` tail (0 failures). Amend the PROGRESS.md entry into the commit per Task 1 Step 6's SHA-patch idiom (or land a follow-up `phase 01: progress note (task 2)` commit — either is acceptable; pick one convention and keep it for every task).

---

### Task 3: `envoy-config::bootstrap` — full type tree

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs`

**Scope:** the `Bootstrap`/`Node`/`Admin`/`StaticResources`/`Cluster`/`Listener`/`Address`/`SocketAddress`/`FilterChain`/`NetworkFilter` struct definitions from SPEC §D1. Pure types + derives; no parsing entrypoint yet (Task 4 lands `parse_bootstrap`, `ConfigError`, `validate`). TDD checkpoint: serde-deserialize test per group, proving the shape parses the phase-00 MINIMAL and the phase-01 SPEC fixture YAML.

- [ ] **Step 1: Write the failing test `parses_phase00_minimal_into_bootstrap`.**

Append to `crates/envoy-config/src/bootstrap.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
"#;

    #[test]
    fn parses_phase00_minimal_into_bootstrap() {
        let b: Bootstrap = serde_yaml::from_str(MINIMAL).expect("valid YAML");
        assert!(b.node.is_none());
        assert!(b.admin.is_none());
        assert_eq!(b.static_resources.listeners.len(), 1);
        let sock = &b.static_resources.listeners[0].address.socket_address;
        assert_eq!(sock.address, "0.0.0.0");
        assert_eq!(sock.port_value, 10000);
        assert_eq!(b.static_resources.clusters.len(), 0);
    }
}
```

- [ ] **Step 2: Run it; verify it fails with "cannot find type `Bootstrap`".**

```bash
cargo test -p envoy-config bootstrap::tests::parses_phase00_minimal_into_bootstrap
```
Expected: compile error, `error[E0412]: cannot find type `Bootstrap` in this scope` (or similar).

- [ ] **Step 3: Write the type tree above the `#[cfg(test)]` block.**

Replace the Task 2 placeholder with:

```rust
//! Bootstrap schema — the phase-01 `envoy.yaml` surface. See SPEC §D1 and
//! ADR-0008. All structs derive `Debug` + `Deserialize` and carry
//! `#[serde(deny_unknown_fields)]` except `Node`, which is deliberately open
//! (SPEC §D1 inline comment).

use serde::Deserialize;

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

// NOTE: Node deliberately omits `deny_unknown_fields`. Upstream Envoy's Node
// also carries metadata, locality, user_agent_*, extensions, client_features,
// listening_addresses, dynamic_parameters. Phase 01 accepts id + cluster and
// silently ignores the rest. When xDS (§9 family) lands, Node is either moved
// or tightened under a new ADR that names the fields then semantically
// load-bearing. (See SPEC §6 signpost 8.)
#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listener {
    #[allow(dead_code)]
    pub name: String,
    pub address: Address,
    pub filter_chains: Vec<FilterChain>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Address {
    pub socket_address: SocketAddress,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocketAddress {
    pub address: String,
    pub port_value: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterChain {
    pub filters: Vec<NetworkFilter>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkFilter {
    pub name: String,
}
```

- [ ] **Step 4: Re-run the test; verify it passes.**

```bash
cargo test -p envoy-config bootstrap::tests::parses_phase00_minimal_into_bootstrap
```
Expected: `test result: ok. 1 passed; 0 failed; 0 ignored`.

- [ ] **Step 5: Add a second failing test for the phase-01 admin-only shape.**

Append inside the `tests` module:

```rust
const ADMIN_ONLY: &str = r#"
node:
  id: envoy-rust-phase-01-subject
  cluster: envoy-rust-phase-01

admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901

static_resources:
  listeners: []
  clusters: []
"#;

#[test]
fn parses_admin_only_bootstrap() {
    let b: Bootstrap = serde_yaml::from_str(ADMIN_ONLY).expect("valid YAML");
    let node = b.node.expect("node present");
    assert_eq!(node.id, "envoy-rust-phase-01-subject");
    assert_eq!(node.cluster, "envoy-rust-phase-01");
    let admin = b.admin.expect("admin present");
    assert_eq!(admin.address.socket_address.address, "127.0.0.1");
    assert_eq!(admin.address.socket_address.port_value, 9901);
    assert_eq!(b.static_resources.listeners.len(), 0);
    assert_eq!(b.static_resources.clusters.len(), 0);
}
```

Run it:

```bash
cargo test -p envoy-config bootstrap::tests::parses_admin_only_bootstrap
```
Expected: `test result: ok. 1 passed; 0 failed`. (Type tree already supports this shape; this is a regression check that the `node`/`admin`/empty-`static_resources` combination works end-to-end.)

- [ ] **Step 6: Run the full crate tests + lint gate.**

```bash
cargo test -p envoy-config
cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
All three expected: exit 0. Test output: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 01: envoy-config bootstrap type tree [ADR-0008]"
```

Append PROGRESS.md Task 3 section with commit SHA and the 2-test tail.

---

### Task 4: `envoy-config::{parse_bootstrap, ConfigError, validate}` — parsing + relaxed validation + the 14-test matrix (+ N2 closure tests)

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (add `parse_bootstrap`, `ConfigError`, `ECHO_FILTER` re-export)
- Modify: `crates/envoy-config/src/bootstrap.rs` (internal `validate` fn + the full test matrix)

**Why:** this task supplies the public API the binary crate will consume in Task 5. It implements the three SPEC §D1 validation relaxations: (a) `listeners.len() ∈ {0, 1}`; (b) `admin.is_none() && listeners.is_empty()` → `NoRuntime`; (c) per-filter `ECHO_FILTER` check carries forward. It also closes phase-00 **N2** by adding per-struct `deny_unknown_fields` regression tests for the 5 deeper structs (`StaticResources`, `Address`, `SocketAddress`, `FilterChain`, `NetworkFilter`) — trivial per STATE.md line 87–90.

**Test matrix (19 tests total):**

The 14 SPEC §D1 tests:
- `parses_minimal_bootstrap` (moved from `envoy-bin/src/config.rs`; shape-adjusted for `static_resources` default)
- `rejects_non_echo_filter`
- `rejects_empty_listeners_with_no_admin` (renamed from phase-00 `rejects_empty_listeners`)
- `rejects_multiple_listeners`
- `rejects_malformed_yaml`
- `rejects_unknown_bootstrap_field`
- `rejects_unknown_listener_field`
- `parses_bootstrap_with_node_admin_empty_resources`
- `parses_bootstrap_with_admin_only` (already landed as `parses_admin_only_bootstrap` in Task 3 — merge, or keep as a rename)
- `parses_bootstrap_with_clusters_stub`
- `rejects_bootstrap_with_neither_admin_nor_listener`
- `rejects_unknown_admin_field`
- `rejects_unknown_cluster_field`
- `accepts_node_with_unmodeled_field`

Plus the 5 N2 closure tests:
- `rejects_unknown_static_resources_field`
- `rejects_unknown_address_field`
- `rejects_unknown_socket_address_field`
- `rejects_unknown_filter_chain_field`
- `rejects_unknown_network_filter_field`

Note the Task-3-introduced `parses_phase00_minimal_into_bootstrap` and `parses_admin_only_bootstrap` are structural shape tests (serde-only); the Task-4 `parses_minimal_bootstrap` and `parses_bootstrap_with_admin_only` exercise `parse_bootstrap` end-to-end including `validate`. Keep both — they cover different surfaces.

- [ ] **Step 1: Write the failing end-to-end test `parses_minimal_bootstrap`.**

Append to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
#[test]
fn parses_minimal_bootstrap() {
    let b = crate::parse_bootstrap(MINIMAL).expect("valid");
    assert_eq!(b.static_resources.listeners.len(), 1);
    assert_eq!(
        b.static_resources.listeners[0]
            .address
            .socket_address
            .port_value,
        10000
    );
}
```

```bash
cargo test -p envoy-config bootstrap::tests::parses_minimal_bootstrap
```
Expected: `error[E0433]: failed to resolve: could not find `parse_bootstrap` in the crate root` or similar compile failure.

- [ ] **Step 2: Implement `parse_bootstrap` + `ConfigError` + `ECHO_FILTER` in `lib.rs`.**

Replace `crates/envoy-config/src/lib.rs` with:

```rust
#![forbid(unsafe_code)]

//! Phase 01 config surface for envoy-rust. Owns the `Bootstrap` type tree and
//! the `parse_bootstrap` entrypoint. See `docs/envoy-rust/DECISIONS.md`
//! ADR-0008 for the extraction rationale.

pub mod bootstrap;

pub use bootstrap::{
    Address, Admin, Bootstrap, Cluster, FilterChain, Listener, NetworkFilter, Node,
    SocketAddress, StaticResources,
};

/// The only network filter name envoy-rust recognizes in phase 01.
pub const ECHO_FILTER: &str = "envoy.filters.network.echo";

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

pub fn parse_bootstrap(yaml: &str) -> Result<Bootstrap, ConfigError> {
    let bootstrap: Bootstrap = serde_yaml::from_str(yaml)?;
    bootstrap::validate(&bootstrap)?;
    Ok(bootstrap)
}
```

Add `validate` at the bottom of `crates/envoy-config/src/bootstrap.rs`, above the `#[cfg(test)]` block:

```rust
pub(crate) fn validate(bootstrap: &Bootstrap) -> Result<(), crate::ConfigError> {
    let listeners = &bootstrap.static_resources.listeners;
    if listeners.len() > 1 {
        return Err(crate::ConfigError::TooManyListeners(listeners.len()));
    }
    if bootstrap.admin.is_none() && listeners.is_empty() {
        return Err(crate::ConfigError::NoRuntime);
    }
    for listener in listeners {
        for chain in &listener.filter_chains {
            for filter in &chain.filters {
                if filter.name != crate::ECHO_FILTER {
                    return Err(crate::ConfigError::UnsupportedFilter(
                        filter.name.clone(),
                        crate::ECHO_FILTER,
                    ));
                }
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Re-run the failing test; verify it passes.**

```bash
cargo test -p envoy-config bootstrap::tests::parses_minimal_bootstrap
```
Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 4: Add the remaining 13 SPEC §D1 tests + 5 N2 closure tests in one batch (keep the tests module ordered logically: parses, rejects, N2-regression).**

Append to the `tests` module (each test is literal; do not paraphrase):

```rust
// --- Positive parses ---

#[test]
fn parses_bootstrap_with_node_admin_empty_resources() {
    let yaml = r#"
node:
  id: id-1
  cluster: cluster-1
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
    let b = crate::parse_bootstrap(yaml).expect("valid");
    assert_eq!(b.node.as_ref().unwrap().id, "id-1");
    assert_eq!(b.admin.as_ref().unwrap().address.socket_address.port_value, 9901);
    assert!(b.static_resources.listeners.is_empty());
    assert!(b.static_resources.clusters.is_empty());
}

#[test]
fn parses_bootstrap_with_admin_only() {
    let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
    let b = crate::parse_bootstrap(yaml).expect("valid");
    assert!(b.node.is_none());
    assert!(b.admin.is_some());
    assert!(b.static_resources.listeners.is_empty());
}

#[test]
fn parses_bootstrap_with_clusters_stub() {
    let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: cluster_0
"#;
    let b = crate::parse_bootstrap(yaml).expect("valid");
    assert_eq!(b.static_resources.clusters.len(), 1);
    assert_eq!(b.static_resources.clusters[0].name, "cluster_0");
}

#[test]
fn accepts_node_with_unmodeled_field() {
    // Node deliberately omits deny_unknown_fields (SPEC §D1 inline comment).
    // Upstream Envoy's Node also carries metadata + locality + etc.
    let yaml = r#"
node:
  id: id-1
  cluster: cluster-1
  metadata: { labels: { tier: edge } }
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
    let b = crate::parse_bootstrap(yaml).expect("valid");
    assert_eq!(b.node.as_ref().unwrap().id, "id-1");
}

// --- Negative validation ---

#[test]
fn rejects_non_echo_filter() {
    let yaml = MINIMAL.replace(
        "envoy.filters.network.echo",
        "envoy.filters.network.tcp_proxy",
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
    assert!(matches!(err, crate::ConfigError::UnsupportedFilter(_, _)), "got {err:?}");
}

#[test]
fn rejects_empty_listeners_with_no_admin() {
    let yaml = "static_resources:\n  listeners: []\n";
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert!(matches!(err, crate::ConfigError::NoRuntime), "got {err:?}");
}

#[test]
fn rejects_bootstrap_with_neither_admin_nor_listener() {
    // Same as rejects_empty_listeners_with_no_admin but via an empty doc.
    let err = crate::parse_bootstrap("{}").expect_err("must reject");
    assert!(matches!(err, crate::ConfigError::NoRuntime), "got {err:?}");
}

#[test]
fn rejects_multiple_listeners() {
    let yaml = r#"
static_resources:
  listeners:
    - name: a
      address: { socket_address: { address: 0.0.0.0, port_value: 1 } }
      filter_chains: [{ filters: [{ name: envoy.filters.network.echo }] }]
    - name: b
      address: { socket_address: { address: 0.0.0.0, port_value: 2 } }
      filter_chains: [{ filters: [{ name: envoy.filters.network.echo }] }]
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert!(matches!(err, crate::ConfigError::TooManyListeners(2)), "got {err:?}");
}

#[test]
fn rejects_malformed_yaml() {
    let err = crate::parse_bootstrap("::: not yaml :::").expect_err("must fail");
    assert!(matches!(err, crate::ConfigError::Yaml(_)), "got {err:?}");
}

// --- deny_unknown_fields regressions (SPEC §D1 + phase-00 N2 closure) ---

fn assert_unknown_field(err: crate::ConfigError) {
    let msg = err.to_string();
    let full = format!("{err:#}");
    assert!(
        msg.contains("unknown field") || full.contains("unknown field"),
        "expected `unknown field` in error; got {full}"
    );
}

#[test]
fn rejects_unknown_bootstrap_field() {
    let yaml = format!("{MINIMAL}\nbogus_field: true\n");
    let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
    assert_unknown_field(err);
}

#[test]
fn rejects_unknown_admin_field() {
    let yaml = r#"
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
  bogus: 1
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert_unknown_field(err);
}

#[test]
fn rejects_unknown_cluster_field() {
    let yaml = r#"
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
static_resources:
  clusters:
    - name: cluster_0
      bogus: 1
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert_unknown_field(err);
}

#[test]
fn rejects_unknown_listener_field() {
    let yaml = r#"
static_resources:
  listeners:
    - name: listener_0
      bogus_listener_field: true
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert_unknown_field(err);
}

// --- N2 closure: 5 deeper structs (STATE.md lines 87–90) ---

#[test]
fn rejects_unknown_static_resources_field() {
    let yaml = r#"
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
static_resources:
  bogus_sr_field: 1
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert_unknown_field(err);
}

#[test]
fn rejects_unknown_address_field() {
    let yaml = r#"
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
    bogus_addr_field: 1
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert_unknown_field(err);
}

#[test]
fn rejects_unknown_socket_address_field() {
    let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
      bogus_sa_field: 1
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert_unknown_field(err);
}

#[test]
fn rejects_unknown_filter_chain_field() {
    let yaml = r#"
static_resources:
  listeners:
    - name: listener_0
      address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
      filter_chains:
        - filters: [{ name: envoy.filters.network.echo }]
          bogus_fc_field: 1
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert_unknown_field(err);
}

#[test]
fn rejects_unknown_network_filter_field() {
    let yaml = r#"
static_resources:
  listeners:
    - name: listener_0
      address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
              bogus_nf_field: 1
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert_unknown_field(err);
}
```

- [ ] **Step 5: Run the full envoy-config test suite.**

```bash
cargo test -p envoy-config
```
Expected: `test result: ok. 21 passed; 0 failed; 0 ignored` — 2 from Task 3 + 19 from this task.

- [ ] **Step 6: Lint + fmt gate.**

```bash
cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Both expected: exit 0.

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 01: parse_bootstrap + ConfigError + N2 closure tests [ADR-0008]"
```

Append PROGRESS.md Task 4 section: commit SHA; the 21-test passing tail; explicit mention that the `rejects_unknown_{static_resources,address,socket_address,filter_chain,network_filter}_field` tests close phase-00 N2 (STATE.md lines 87–90).

---

### Task 5: Delete `envoy-bin/src/config.rs`; retarget `envoy-bin` at `envoy-config`

**Files:**
- Delete: `crates/envoy-bin/src/config.rs`
- Modify: `crates/envoy-bin/Cargo.toml` (add `envoy-config`; drop `serde`, `serde_yaml`)
- Modify: `crates/envoy-bin/src/main.rs` (swap `mod config;` → `use envoy_config::...`)

**Why now:** Task 4 has just delivered the public API. `envoy-bin::main::run` must not continue to reference the phase-00 inline parser once the replacement exists, because both would be compiled and the binary would have two `Bootstrap` types. Keep this task narrow — no admin wiring yet (Task 11), no argv extraction (Task 12). The `run()` body still spawns only the echo listener.

- [ ] **Step 1: Edit `crates/envoy-bin/Cargo.toml`.**

Replace the `[dependencies]` block with:

```toml
[dependencies]
anyhow = "1"
envoy-config = { path = "../envoy-config" }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "signal", "time", "sync"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
```

`serde` and `serde_yaml` are intentionally removed — they arrive transitively via `envoy-config`. `tokio-util` is added in Task 11 (scoped to that task's `CancellationToken` introduction); do NOT preemptively add it here.

- [ ] **Step 2: Delete the old inline parser.**

```bash
git rm crates/envoy-bin/src/config.rs
```

- [ ] **Step 3: Edit `crates/envoy-bin/src/main.rs` to consume `envoy-config`.**

Apply these targeted changes (retain everything else verbatim from the current phase-00 file):

- Remove the line `mod config;`.
- In `run()`, replace `let bootstrap = config::parse_bootstrap(&yaml)?;` with `let bootstrap = envoy_config::parse_bootstrap(&yaml)?;` and add `use anyhow::Context as _;` at the top of the `use` block if it is not already imported (it already is via `anyhow::{Context, Result}`).
- The `run()` body still only destructures `bootstrap.static_resources.listeners[0].address.socket_address` because admin wiring is deferred to Task 11.
- Because `envoy_config::parse_bootstrap` returns `Result<Bootstrap, ConfigError>` (not `anyhow::Result<_>`), the `?` operator works: `ConfigError: std::error::Error` and `anyhow::Error: From<E> where E: std::error::Error + Send + Sync + 'static`. Confirm this explicitly by compile, not by eye.

The resulting `main.rs` differs from the current file by exactly two lines: `mod config;` is deleted, and the one `config::parse_bootstrap` call becomes `envoy_config::parse_bootstrap`.

- [ ] **Step 4: Run the envoy-bin test suite.**

```bash
cargo test -p envoy-bin
```
Expected: `test result: ok. 6 passed; 0 failed` (the 6 argv tests, unchanged). The phase-00 `config::tests` module (5–7 tests including the M3 regressions) is gone from this crate because the source file is gone; its contents moved to `envoy-config::bootstrap::tests` in Task 4. The full-workspace test count therefore stays balanced: envoy-bin loses 7, envoy-config gains 21 — net +14.

- [ ] **Step 5: Workspace-wide gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
```
All expected: exit 0. `cargo test --workspace` tail should show the phase-00 `differential::tests` + `echo_fixture` still passing in CI (Docker-gated locally per phase-00 PROGRESS.md Task 14 deviation).

- [ ] **Step 6: Commit.**

```bash
git add crates/envoy-bin/Cargo.toml crates/envoy-bin/src/main.rs
git commit -m "phase 01: envoy-bin consumes envoy-config [ADR-0008]"
```

Append PROGRESS.md Task 5 section. Cross-reference Task 4's N2 closure if the reviewer asks where the old `config::tests` went.

---

### Task 6: Scaffold `crates/envoy-config/fuzz/` subcrate + workspace-exclude + corpus seed

**Files:**
- Create: `crates/envoy-config/fuzz/Cargo.toml`
- Create: `crates/envoy-config/fuzz/.gitignore`
- Create: `crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/minimal.yaml`
- Modify: `Cargo.toml` (root — add `[workspace] exclude`)

**Why:** Task 7's CI job needs this in place. The subcrate is workspace-excluded per ADR-0009/0010 so `libfuzzer-sys` is not compiled by the stable `build` job.

- [ ] **Step 1: Write `crates/envoy-config/fuzz/Cargo.toml`.**

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
envoy-config = { path = ".." }

[[bin]]
name = "parse_bootstrap"
path = "fuzz_targets/parse_bootstrap.rs"
test = false
doc = false
bench = false
```

- [ ] **Step 2: Write `crates/envoy-config/fuzz/.gitignore`.**

```
corpus/parse_bootstrap/*
!corpus/parse_bootstrap/minimal.yaml
artifacts/
target/
```

(The negation keeps the single committed seed file under version control while ignoring any coverage-discovered corpus additions locally. `artifacts/` holds any libFuzzer crash reproductions.)

- [ ] **Step 3: Write `crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`.**

```rust
#![no_main]
#![forbid(unsafe_code)]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = envoy_config::parse_bootstrap(s);
    }
});
```

The UTF-8 gate is deliberate (SPEC §6 signpost 5): production reads configs via `std::fs::read_to_string` which already fails on non-UTF-8 inputs, so non-UTF-8 bytes never reach `serde_yaml` in production.

- [ ] **Step 4: Write the corpus seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/minimal.yaml`.**

Use the phase-00 fixture YAML with `{{PORT}}` replaced by the constant `10000` (SPEC §D2 directs):

```yaml
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.echo.v3.Echo
```

Wait — the `typed_config` block uses unknown fields that `deny_unknown_fields` on `NetworkFilter` will reject. Task 4's `NetworkFilter` struct only models `name`. The seed file must parse under the phase-01 grammar. Replace the seed with the admin-only shape from SPEC §D7, with a constant port:

```yaml
node:
  id: envoy-rust-fuzz-seed
  cluster: envoy-rust-fuzz-seed

admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 10000

static_resources:
  listeners: []
  clusters: []
```

This is the richest shape that (a) parses cleanly under the phase-01 grammar, (b) exercises `Option<Node>`, `Option<Admin>`, `StaticResources::default()`, and `SocketAddress`, and (c) is stable under corpus mutation.

- [ ] **Step 5: Add the fuzz subcrate to `[workspace] exclude` in the root `Cargo.toml`.**

The full root `Cargo.toml` after this step:

```toml
[workspace]
resolver = "2"
members = [
    "crates/envoy-bin",
    "crates/envoy-config",
    "tests/differential",
]
exclude = [
    "crates/envoy-config/fuzz",
]

# Workspace members grow as later phases introduce crates.
```

- [ ] **Step 6: Verify the main workspace is unaffected.**

```bash
cargo build --workspace --all-targets
cargo test --workspace
```
Expected: both exit 0 with the same test counts as Task 5 (no tests added to the main workspace by this task).

- [ ] **Step 7: (Optional — nightly + cargo-fuzz installed locally) smoke-run the fuzz target.**

If and only if the developer has `rustup toolchain install nightly && cargo install cargo-fuzz` in place:

```bash
cd crates/envoy-config
cargo +nightly fuzz run parse_bootstrap -- -max_total_time=5
```
Expected: the seed is consumed and the run exits after ~5s with `Done …` and no crash. If this fails for local reasons (no nightly, no Docker analogue), treat it as advisory — the CI `fuzz` job (Task 7) is the authoritative validator.

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-config/fuzz Cargo.toml
git commit -m "phase 01: scaffold envoy-config-fuzz subcrate [ADR-0009, ADR-0010]"
```

Append PROGRESS.md Task 6 section with the commit SHA and (optionally) the local `cargo +nightly fuzz run -- -max_total_time=5` tail. Note explicitly that the seed was simplified to the admin-only shape because `NetworkFilter`'s `deny_unknown_fields` rejects the phase-00 fixture's `typed_config` block — deviation from SPEC §D2's "phase-00 fixture with `{{PORT}}` replaced" wording; record it as a SPEC-minor deviation, not a drift (no ADR needed).

---

### Task 7: CI workflow — rename single job to `build`; add parallel `fuzz` job

**Files:**
- Modify: `.github/workflows/ci.yml`

**Why:** the fuzz subcrate is workspace-excluded, so the existing `cargo build --workspace` never invokes it. A dedicated CI job runs the short-budget fuzz under `+nightly` per ADR-0010.

- [ ] **Step 1: Rename the current single job; add a second job `fuzz`.**

Replace `.github/workflows/ci.yml` with:

```yaml
name: ci

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

permissions:
  contents: read

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

jobs:
  build:
    name: build + test + lint
    runs-on: ubuntu-latest
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v4

      - name: install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
          # `toolchain:` is intentionally omitted so dtolnay reads
          # rust-toolchain.toml (D-3.9 pin, currently 1.95.0).

      - name: cargo cache
        uses: Swatinem/rust-cache@v2

      - name: fmt
        run: cargo fmt --all -- --check

      - name: clippy
        run: cargo clippy --workspace --all-targets --all-features -- -D warnings

      - name: build
        run: cargo build --workspace --all-targets

      - name: test (includes differential harness → Docker)
        run: cargo test --workspace

      - name: install cargo-deny
        uses: taiki-e/install-action@v2
        with:
          tool: cargo-deny

      - name: cargo deny check
        run: cargo deny check

  fuzz:
    name: fuzz (parse_bootstrap, 30s)
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@v4

      - name: install nightly Rust toolchain
        uses: dtolnay/rust-toolchain@nightly
        with:
          components: rust-src

      - name: cargo cache (fuzz subcrate)
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: "crates/envoy-config/fuzz -> target"

      - name: install cargo-fuzz
        run: cargo install cargo-fuzz --locked

      - name: fuzz parse_bootstrap
        working-directory: crates/envoy-config
        run: cargo fuzz run parse_bootstrap -- -max_total_time=30
```

Two jobs run in parallel. If `fuzz` fails (panic, sanitizer hit), the run is red; the existing `build` job is unaffected.

- [ ] **Step 2: Sanity-check the YAML parses.**

```bash
python3 -c 'import yaml, sys; yaml.safe_load(open(".github/workflows/ci.yml"))' && echo ok
```
Expected: `ok`.

- [ ] **Step 3: Commit.**

```bash
git add .github/workflows/ci.yml
git commit -m "phase 01: CI parallel fuzz job [ADR-0010]"
```

- [ ] **Step 4: Push + watch the run.**

The CI validation itself belongs to state 4 (Task 19). Before pushing, confirm `cargo deny check` is still green locally (the `install cargo-fuzz` step pulls crates into `~/.cargo` not the workspace, so cargo-deny is unaffected). If it flips red (e.g. an unmapped license on `libfuzzer-sys`'s transitive chain), **stop** and land ADR-0012 per D-3.5 before continuing — see SPEC §6 signpost 10.

Append PROGRESS.md Task 7 section with the CI workflow YAML diff and a note that the `fuzz` job's pass/fail moves to Task 19's phase-done section.

---

### Task 8: ADR-0011 — defer response-header equivalence to phase 04

**Files:**
- Modify (append): `docs/envoy-rust/DECISIONS.md`

**Why:** admin code in Tasks 9–11 emits an HTTP response block that diverges from upstream Envoy on the `server:` header (`envoy-rust` vs. `envoy`). ADR-0011 records that the fixture-0002 equivalence contract is status + body only, not headers. Landing it before the admin code means the code reviewer has the rationale in-tree as they read `admin.rs`.

- [ ] **Step 1: Append ADR-0011 to `docs/envoy-rust/DECISIONS.md`.**

```markdown
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
```

- [ ] **Step 2: Verify the ADR sequence is intact.**

```bash
grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md
```
Expected: `11`.

- [ ] **Step 3: Commit.**

```bash
git add docs/envoy-rust/DECISIONS.md
git commit -m "phase 01: ADR-0011 — defer header equivalence to phase 04"
```

Append PROGRESS.md Task 8 entry.

---

### Task 9: `envoy-bin::admin::{render_response, rfc7231_imf_fixdate}` — response framing + IMF-fixdate helper

**Files:**
- Create: `crates/envoy-bin/src/admin.rs` (populated incrementally; Task 10 adds `serve`)
- Modify: `crates/envoy-bin/src/main.rs` (add `mod admin;`)
- Modify: `crates/envoy-bin/Cargo.toml` (add `httparse`)

**Why:** isolate the two pure helpers behind TDD before the async accept loop in Task 10 lands. `render_response` is the entire response framing; `rfc7231_imf_fixdate` is the date serializer. Both are sync + allocation-only — cheap to test exhaustively.

- [ ] **Step 1: Add `httparse` to `crates/envoy-bin/Cargo.toml`.**

Amend `[dependencies]`:

```toml
[dependencies]
anyhow = "1"
envoy-config = { path = "../envoy-config" }
httparse = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "signal", "time", "sync"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
```

`tokio-util` is still **not** added — it only arrives in Task 11 with `CancellationToken`.

- [ ] **Step 2: Create `crates/envoy-bin/src/admin.rs` with the skeleton + `render_response` failing test.**

Initial file contents:

```rust
//! Minimal admin HTTP endpoint for phase 01. Serves `GET /ready` → `200 OK`
//! with body `LIVE\n`; everything else returns `404 Not Found`. The framing
//! is hand-rolled (no `hyper`, no `axum` — doctrine D-3.2). Per ADR-0011,
//! phase 01's differential contract is status + body only, not headers, so
//! the `server:` header carries `envoy-rust` and diverges from upstream
//! Envoy's `envoy` string until phase 04 populates the header allow-list.

use std::time::SystemTime;

/// Render a complete HTTP/1.1 response with a `Connection: close` framing. The
/// body is written verbatim; the caller passes the exact bytes (including any
/// trailing newline). Headers:
///
/// - `content-type: text/plain`
/// - `content-length: {body.len()}`
/// - `cache-control: no-cache, max-age=0`
/// - `x-content-type-options: nosniff`
/// - `server: envoy-rust` (ADR-0011 divergence from upstream)
/// - `date: {IMF-fixdate}`
/// - `connection: close`
pub(crate) fn render_response(status: u16, reason: &str, body: &[u8]) -> Vec<u8> {
    render_response_at(status, reason, body, SystemTime::now())
}

pub(crate) fn render_response_at(
    status: u16,
    reason: &str,
    body: &[u8],
    now: SystemTime,
) -> Vec<u8> {
    let date = rfc7231_imf_fixdate(now);
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         content-type: text/plain\r\n\
         content-length: {len}\r\n\
         cache-control: no-cache, max-age=0\r\n\
         x-content-type-options: nosniff\r\n\
         server: envoy-rust\r\n\
         date: {date}\r\n\
         connection: close\r\n\
         \r\n",
        len = body.len(),
    );
    let mut out = head.into_bytes();
    out.extend_from_slice(body);
    out
}

/// RFC 7231 IMF-fixdate: `Sun, 06 Nov 1994 08:49:37 GMT`.
///
/// Hand-rolled over `SystemTime` to avoid depending on `chrono` or `time`
/// (not on D-3.2; a phase that genuinely needs date arithmetic — phase 06's
/// access logs, maybe — can land an ADR and the crate together). Valid for
/// any `SystemTime` at or after the Unix epoch.
pub(crate) fn rfc7231_imf_fixdate(t: SystemTime) -> String {
    const DOW: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MON: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let tod = (secs % 86_400) as u32;
    let hh = tod / 3600;
    let mm = (tod / 60) % 60;
    let ss = tod % 60;

    // 1970-01-01 was a Thursday; DOW above is offset accordingly so days=0
    // indexes to "Thu".
    let dow = DOW[days.rem_euclid(7) as usize];
    let (y, mo, d) = civil_from_days(days);
    format!(
        "{dow}, {d:02} {mon} {y:04} {hh:02}:{mm:02}:{ss:02} GMT",
        mon = MON[(mo - 1) as usize],
    )
}

/// Howard Hinnant's public-domain `civil_from_days` algorithm — converts the
/// day-count since the Unix epoch into a `(year, month, day)` triple.
/// Valid for the full `i64` range; we only feed it non-negative values.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146_096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn imf_fixdate_epoch_zero() {
        assert_eq!(
            rfc7231_imf_fixdate(UNIX_EPOCH),
            "Thu, 01 Jan 1970 00:00:00 GMT"
        );
    }

    #[test]
    fn imf_fixdate_known_1994() {
        // 1994-11-06 08:49:37 UTC — the RFC 7231 §7.1.1.1 example timestamp.
        // secs = 784111777 (verified independently).
        let t = UNIX_EPOCH + Duration::from_secs(784_111_777);
        assert_eq!(rfc7231_imf_fixdate(t), "Sun, 06 Nov 1994 08:49:37 GMT");
    }

    #[test]
    fn imf_fixdate_leap_year_boundary() {
        // 2000-02-29 12:00:00 UTC — the century leap-year that Hinnant's
        // algorithm gets right where the naive "divisible by 4" check fails.
        // secs = 951_825_600 (2000-02-29 12:00 UTC).
        let t = UNIX_EPOCH + Duration::from_secs(951_825_600);
        assert_eq!(rfc7231_imf_fixdate(t), "Tue, 29 Feb 2000 12:00:00 GMT");
    }

    #[test]
    fn render_response_has_expected_shape_and_body() {
        let t = UNIX_EPOCH + Duration::from_secs(784_111_777);
        let bytes = render_response_at(200, "OK", b"LIVE\n", t);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "status line: {s:?}");
        assert!(s.contains("content-length: 5\r\n"), "missing CL: {s:?}");
        assert!(s.contains("content-type: text/plain\r\n"));
        assert!(s.contains("server: envoy-rust\r\n"));
        assert!(s.contains("date: Sun, 06 Nov 1994 08:49:37 GMT\r\n"));
        assert!(s.contains("connection: close\r\n"));
        assert!(s.ends_with("\r\n\r\nLIVE\n"), "body/CRLF: {s:?}");
    }

    #[test]
    fn render_response_404_body_is_invalid_path_message() {
        let t = UNIX_EPOCH;
        let body = b"invalid path. admin commands are:\n  /ready\n" as &[u8];
        let bytes = render_response_at(404, "Not Found", body, t);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(s.contains(&format!("content-length: {}\r\n", body.len())));
        assert!(s.ends_with(std::str::from_utf8(body).unwrap()));
    }
}
```

- [ ] **Step 3: Register `mod admin;` in `crates/envoy-bin/src/main.rs`.**

Below `mod echo;`, add:

```rust
mod admin;
```

Add a crate-root `#[allow(dead_code)]` on `mod admin;` temporarily until Task 11 consumes it (same pattern phase 00 used for `config`, `argv`, `echo` per phase-00 PROGRESS.md Task 6):

```rust
#[allow(dead_code)]
mod admin;
```

- [ ] **Step 4: Run the envoy-bin test suite.**

```bash
cargo test -p envoy-bin admin::tests
```
Expected: `test result: ok. 5 passed; 0 failed` — the 3 IMF tests + the 2 render_response tests.

```bash
cargo test -p envoy-bin
```
Expected: 11 tests passing (6 argv + 5 admin).

- [ ] **Step 5: Lint + fmt gate.**

```bash
cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Both expected: exit 0.

- [ ] **Step 6: Commit.**

```bash
git add crates/envoy-bin/Cargo.toml crates/envoy-bin/src/admin.rs crates/envoy-bin/src/main.rs
git commit -m "phase 01: admin render_response + IMF-fixdate helper"
```

Append PROGRESS.md Task 9 entry with the 5-test tail and the SPEC-mandated `server: envoy-rust` divergence (ADR-0011).

---

### Task 10: `envoy-bin::admin::serve` — accept loop + per-connection handler + drain; 5 unit tests

**Files:**
- Modify: `crates/envoy-bin/src/admin.rs`

**Why:** lands the real async surface. Mirrors `echo::serve`'s shape (shared `JoinSet` + 5 s drain + timeout-abort) so the reviewer can diff the two side by side (SPEC §D3 point 1). Handler is the sequential read-until-`Complete(n)` httparse loop from SPEC §D3 point 2.

- [ ] **Step 1: Write the first failing test `serves_ready_live`.**

Append to `crates/envoy-bin/src/admin.rs::tests`:

```rust
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

async fn bind_random_local() -> TcpListener {
    TcpListener::bind(("127.0.0.1", 0)).await.expect("bind :0")
}

/// Drive one connection: open, write `req`, read all bytes until EOF, return.
async fn drive(addr: std::net::SocketAddr, req: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(req).await.expect("write");
    stream.shutdown().await.ok();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read");
    buf
}

#[tokio::test]
async fn serves_ready_live() {
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

    let resp = drive(addr, b"GET /ready HTTP/1.1\r\nHost: x\r\n\r\n").await;
    let s = std::str::from_utf8(&resp).unwrap();
    assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "{s:?}");
    assert!(s.ends_with("LIVE\n"), "{s:?}");

    tx.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .expect("drain within 5s")
        .unwrap();
}
```

- [ ] **Step 2: Run; verify compile failure (`cannot find function `serve``).**

```bash
cargo test -p envoy-bin admin::tests::serves_ready_live
```
Expected: compile error, `error[E0425]: cannot find function `serve` in this scope`.

- [ ] **Step 3: Implement `serve` + the per-connection handler above the `#[cfg(test)]` block.**

Insert (beneath `civil_from_days`, above `#[cfg(test)]`):

```rust
use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tokio::time::timeout;

/// Graceful drain budget — same 5s window `echo::serve` honors.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Per-connection request-head buffer cap (SPEC §6 signpost 4).
const MAX_REQUEST_HEAD: usize = 8 * 1024;

/// Accept loop. Each accepted connection is passed to `handle_one` on a
/// `JoinSet`. On `shutdown`, stop accepting and wait up to `DRAIN_TIMEOUT`
/// for in-flight handlers; then abort. Mirrors `echo::serve`.
pub async fn serve<F>(listener: TcpListener, shutdown: F) -> Result<()>
where
    F: Future<Output = ()>,
{
    let mut set: JoinSet<()> = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => {
                tracing::info!("admin shutdown signal received; closing listener");
                drop(listener);
                break;
            }
            accept = listener.accept() => {
                match accept {
                    Ok((stream, peer)) => {
                        tracing::debug!(%peer, "admin accepted connection");
                        set.spawn(async move {
                            if let Err(err) = handle_one(stream).await {
                                tracing::warn!(%peer, error = %err, "admin connection failed");
                            }
                        });
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "admin accept failed; continuing");
                    }
                }
            }
        }
    }

    let in_flight = set.len();
    tracing::info!(in_flight, "admin draining in-flight connections");
    let drained = timeout(DRAIN_TIMEOUT, async {
        while set.join_next().await.is_some() {}
    })
    .await;
    if drained.is_err() {
        tracing::warn!("admin drain timeout; aborting remaining tasks");
        set.shutdown().await;
    }
    Ok(())
}

async fn handle_one(mut stream: TcpStream) -> Result<()> {
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut scratch = [0u8; 1024];
    loop {
        if buf.len() >= MAX_REQUEST_HEAD {
            let resp = render_response(431, "Request Header Fields Too Large", b"");
            stream.write_all(&resp).await.ok();
            return Ok(());
        }
        let n = stream.read(&mut scratch).await?;
        if n == 0 {
            // EOF mid-request — silent close per SPEC §D3 point 2.1.
            return Ok(());
        }
        buf.extend_from_slice(&scratch[..n]);

        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut req = httparse::Request::new(&mut headers);
        match req.parse(&buf) {
            Ok(httparse::Status::Complete(_)) => {
                let method = req.method.unwrap_or("");
                let path = req.path.unwrap_or("");
                let (status, reason, body): (u16, &str, &[u8]) = match (method, path) {
                    ("GET", "/ready") => (200, "OK", b"LIVE\n"),
                    _ => (
                        404,
                        "Not Found",
                        b"invalid path. admin commands are:\n  /ready\n",
                    ),
                };
                let resp = render_response(status, reason, body);
                stream.write_all(&resp).await?;
                return Ok(());
            }
            Ok(httparse::Status::Partial) => continue,
            Err(_) => {
                // Malformed request line / headers — silent close.
                return Ok(());
            }
        }
    }
}
```

- [ ] **Step 4: Re-run the test; verify it passes.**

```bash
cargo test -p envoy-bin admin::tests::serves_ready_live
```
Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 5: Add the remaining 4 admin tests.**

Append to `admin::tests`:

```rust
#[tokio::test]
async fn a404s_unknown_path() {
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

    let resp = drive(addr, b"GET /does-not-exist HTTP/1.1\r\nHost: x\r\n\r\n").await;
    let s = std::str::from_utf8(&resp).unwrap();
    assert!(s.starts_with("HTTP/1.1 404 Not Found\r\n"), "{s:?}");
    assert!(
        s.contains("invalid path. admin commands are:\n  /ready\n"),
        "{s:?}"
    );

    tx.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn a404s_non_get_ready() {
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

    let resp = drive(addr, b"POST /ready HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n\r\n").await;
    let s = std::str::from_utf8(&resp).unwrap();
    assert!(s.starts_with("HTTP/1.1 404 Not Found\r\n"), "{s:?}");

    tx.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn rejects_oversized_request_headers() {
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

    // Build a request-head larger than MAX_REQUEST_HEAD (8 KiB) with no CRLF
    // terminator, so the handler keeps reading until the cap fires.
    let mut req: Vec<u8> = b"GET /ready HTTP/1.1\r\nHost: x\r\nX-Big: ".to_vec();
    req.extend(std::iter::repeat_n(b'A', 9000));
    let resp = drive(addr, &req).await;
    let s = std::str::from_utf8(&resp).unwrap_or("<non-utf8>");
    assert!(
        s.starts_with("HTTP/1.1 431 Request Header Fields Too Large\r\n"),
        "{s:?}"
    );

    tx.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn drain_exits_within_budget() {
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

    // Open a connection that never completes a request (no CRLF terminator).
    // Fire shutdown; serve() should still return within the 5s drain budget.
    let client = tokio::spawn(async move {
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(b"GET /ready HTTP/1.1\r\n").await.ok();
        // Hold open past the drain window; the in-flight handler reads a byte
        // short of `Complete(n)` and yields; shutdown races the client close.
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        drop(s);
    });

    // Give the server a moment to accept.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    tx.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(6), server)
        .await
        .expect("serve returns within drain budget")
        .unwrap();
    client.abort();
}
```

Rename the earlier 404 tests to start with an `a` so cargo's alphabetic ordering puts them after `serves_ready_live`; this is cosmetic and optional. If it feels churny, drop the rename — test order is irrelevant to correctness.

- [ ] **Step 6: Run the full envoy-bin suite.**

```bash
cargo test -p envoy-bin
```
Expected: `test result: ok. 15 passed; 0 failed` — 6 argv + 5 admin unit (render/IMF from Task 9) + 4 admin async tests from this task. Adjust counts if you add the 5th `serves_ready_live` to the ticker count (total becomes 15 not 14 — verify empirically, don't trust this comment).

- [ ] **Step 7: Lint + fmt gate.**

```bash
cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Both expected: exit 0.

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-bin/src/admin.rs
git commit -m "phase 01: admin::serve accept loop + handler + drain"
```

Append PROGRESS.md Task 10 entry with the full test tail (5 admin-async tests passing) and a note on the drain observability: the `drain_exits_within_budget` test empirically verifies SPEC §D3 point 5's 5 s timeout.

---

### Task 11: Wire admin into `envoy-bin::main::run` — `CancellationToken` + `JoinSet`; `tests/admin_only.rs` integration test

**Files:**
- Modify: `crates/envoy-bin/Cargo.toml` (add `tokio-util`)
- Modify: `crates/envoy-bin/src/main.rs` (rewrite `run()`)
- Create: `crates/envoy-bin/tests/admin_only.rs`

**Why:** Task 9 + Task 10 landed the admin module under `#[allow(dead_code)]`. This task consumes it from `run()` and exercises the wiring end-to-end via a bin-level integration test. After this task, the phase-01 subject binary can serve `/ready` and keep echoing on a separate listener, per SPEC §D4.

- [ ] **Step 1: Add `tokio-util` to `crates/envoy-bin/Cargo.toml`.**

Amend `[dependencies]`:

```toml
[dependencies]
anyhow = "1"
envoy-config = { path = "../envoy-config" }
httparse = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "signal", "time", "sync"] }
tokio-util = { version = "0.7", features = ["default"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
```

`tokio-util` default features include `sync` (where `CancellationToken` lives). Permitted foundation per D-3.2; no ADR.

- [ ] **Step 2: Rewrite `run()` in `crates/envoy-bin/src/main.rs`.**

Replace the current `run()` (the single-listener echo body) with the SPEC §D4 shape. The full diff is localized to `run()`; leave `main()`, `install_tracing()`, `shutdown_signal()`, and the argv module in place.

```rust
async fn run(config_path: std::path::PathBuf) -> Result<()> {
    let yaml = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config at {}", config_path.display()))?;
    let bootstrap = envoy_config::parse_bootstrap(&yaml)?;

    if let Some(node) = bootstrap.node.as_ref() {
        tracing::info!(
            node.id = %node.id,
            node.cluster = %node.cluster,
            "node registered",
        );
    }

    let token = tokio_util::sync::CancellationToken::new();
    let signal_token = token.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        signal_token.cancel();
    });

    let mut set: tokio::task::JoinSet<Result<()>> = tokio::task::JoinSet::new();

    if let Some(listener_cfg) = bootstrap.static_resources.listeners.first() {
        let sock = &listener_cfg.address.socket_address;
        let addr: SocketAddr = format!("{}:{}", sock.address, sock.port_value)
            .parse()
            .with_context(|| format!("parsing address {}:{}", sock.address, sock.port_value))?;
        let lst = TcpListener::bind(addr)
            .await
            .with_context(|| format!("binding echo listener to {addr}"))?;
        tracing::info!(%addr, "envoy-rust listening (echo)");
        let shutdown = token.clone();
        set.spawn(async move {
            echo::serve(lst, async move { shutdown.cancelled().await }).await
        });
    }

    if let Some(admin_cfg) = bootstrap.admin.as_ref() {
        let sock = &admin_cfg.address.socket_address;
        let addr: SocketAddr = format!("{}:{}", sock.address, sock.port_value)
            .parse()
            .with_context(|| format!("parsing admin address {}:{}", sock.address, sock.port_value))?;
        let lst = TcpListener::bind(addr)
            .await
            .with_context(|| format!("binding admin listener to {addr}"))?;
        tracing::info!(%addr, "envoy-rust listening (admin)");
        let shutdown = token.clone();
        set.spawn(async move {
            admin::serve(lst, async move { shutdown.cancelled().await }).await
        });
    }

    while let Some(res) = set.join_next().await {
        res.context("task panicked")??;
    }
    tracing::info!("envoy-rust exited cleanly");
    Ok(())
}
```

Also remove the crate-root `#[allow(dead_code)]` on `mod admin;` (admin is now consumed).

- [ ] **Step 3: Verify envoy-bin compiles + unit tests still pass.**

```bash
cargo build -p envoy-bin
cargo test -p envoy-bin
```
Both expected: exit 0. Unit test count unchanged from Task 10.

- [ ] **Step 4: Write the failing integration test `tests/admin_only.rs`.**

Create `crates/envoy-bin/tests/admin_only.rs`:

```rust
//! End-to-end bin test: write an admin-only config, spawn the `envoy-bin`
//! binary as a subprocess, and verify it serves `GET /ready`. This is a
//! backstop for the main contract — the real differential assertion is the
//! fixture-0002 acceptance test in Task 18.

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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
            Err(e) => panic!("admin never became ready at {addr}: {e}"),
        }
    }
}

#[tokio::test]
async fn admin_only_config_serves_ready() {
    let port = reserve_port();
    let yaml = format!(
        r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {port}

static_resources:
  listeners: []
  clusters: []
"#
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

    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    wait_ready(addr, Duration::from_secs(10)).await;

    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(b"GET /ready HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    s.shutdown().await.ok();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).await.unwrap();
    let text = std::str::from_utf8(&buf).unwrap();
    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "status: {text:?}");
    assert!(text.ends_with("LIVE\n"), "body: {text:?}");

    child.kill().await.ok();
    let _ = child.wait().await;
}
```

Add `tempfile` to `[dev-dependencies]` of `crates/envoy-bin/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 5: Run the integration test.**

```bash
cargo test -p envoy-bin --test admin_only
```
Expected: `test result: ok. 1 passed; 0 failed` in ~1 s. If it hangs, the handler's EOF-on-empty-read branch is unreachable because the client has not half-closed — re-check SPEC §D3 point 2.1 and the `s.shutdown()` call in the test.

- [ ] **Step 6: Full workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
```
All expected: exit 0. The new integration test raises the envoy-bin test count by 1.

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-bin/Cargo.toml crates/envoy-bin/src/main.rs crates/envoy-bin/tests/admin_only.rs
git commit -m "phase 01: wire admin into envoy-bin::run + admin_only integration"
```

Append PROGRESS.md Task 11 entry with the `admin_only_config_serves_ready` tail and a note that `tokio-util::sync::CancellationToken` is the new dep (D-3.2 permitted).

---

### Task 12: Extract `argv.rs` from `main.rs`

**Files:**
- Create: `crates/envoy-bin/src/argv.rs`
- Modify: `crates/envoy-bin/src/main.rs`

**Why:** `main.rs` has grown to carry argv + echo wiring + admin wiring + run(). Extraction per SPEC §D4 sizes `main.rs` back to an orchestrator (~70 LoC) and mirrors the `echo`/`admin`/`config`-era modular layout. SPEC §D4 marks this optional — skip the task entirely if the extraction churn feels wrong to the executor; mention the skip in PROGRESS.md.

- [ ] **Step 1: Create `crates/envoy-bin/src/argv.rs` by moving `ArgvError` + `parse_argv` + the `argv_tests` module from `main.rs`.**

File contents (copy-paste from the current `main.rs` with no semantic changes; `impl Display`, `impl Error`, the 6 tests):

```rust
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum ArgvError {
    NoConfigFlag,
    UnknownFlag(String),
    MissingValue(String),
    Trailing(String),
}

impl std::fmt::Display for ArgvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoConfigFlag => write!(
                f,
                "expected exactly one of `-c <path>` or `--config-path <path>`",
            ),
            Self::UnknownFlag(flag) => write!(f, "unknown argument: {flag}"),
            Self::MissingValue(flag) => write!(f, "{flag} requires a path argument"),
            Self::Trailing(arg) => write!(f, "unexpected trailing argument: {arg}"),
        }
    }
}

impl std::error::Error for ArgvError {}

/// Phase 01 accepts exactly one flag: `-c <path>` or `--config-path <path>`.
/// `clap` is deliberately avoided (not on the D-3.2 permitted-foundations list).
/// When argv grows past a single path, land an ADR and revisit.
pub fn parse_argv<I, S>(args: I) -> Result<PathBuf, ArgvError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut iter = args.into_iter().map(Into::into);
    let _ = iter.next();
    let mut path: Option<PathBuf> = None;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-c" | "--config-path" => {
                let value = iter.next().ok_or(ArgvError::MissingValue(arg.clone()))?;
                if path.is_some() {
                    return Err(ArgvError::Trailing(value));
                }
                path = Some(PathBuf::from(value));
            }
            other => return Err(ArgvError::UnknownFlag(other.to_string())),
        }
    }
    path.ok_or(ArgvError::NoConfigFlag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        std::iter::once("envoy-bin")
            .chain(args.iter().copied())
            .map(ToOwned::to_owned)
            .collect()
    }

    #[test]
    fn accepts_short_flag() {
        let p = parse_argv(argv(&["-c", "/etc/envoy-rust.yaml"])).unwrap();
        assert_eq!(p, PathBuf::from("/etc/envoy-rust.yaml"));
    }

    #[test]
    fn accepts_long_flag() {
        let p = parse_argv(argv(&["--config-path", "/tmp/e.yaml"])).unwrap();
        assert_eq!(p, PathBuf::from("/tmp/e.yaml"));
    }

    #[test]
    fn rejects_missing_flag() {
        assert_eq!(parse_argv(argv(&[])), Err(ArgvError::NoConfigFlag));
    }

    #[test]
    fn rejects_missing_value() {
        assert_eq!(
            parse_argv(argv(&["-c"])),
            Err(ArgvError::MissingValue("-c".into())),
        );
    }

    #[test]
    fn rejects_unknown_flag() {
        assert_eq!(
            parse_argv(argv(&["--foo", "bar"])),
            Err(ArgvError::UnknownFlag("--foo".into())),
        );
    }

    #[test]
    fn rejects_duplicate_config_flag() {
        let err = parse_argv(argv(&["-c", "/a", "-c", "/b"])).unwrap_err();
        assert!(matches!(err, ArgvError::Trailing(_)), "got {err:?}");
    }
}
```

- [ ] **Step 2: In `main.rs`, delete the moved code and add `mod argv;`.**

Remove `ArgvError`, `impl Display for ArgvError`, `impl Error for ArgvError`, `parse_argv`, and `#[cfg(test)] mod argv_tests { ... }`. Add at the top (alongside `mod echo; mod admin;`):

```rust
mod argv;
```

Update the `main()` call site:

```rust
match argv::parse_argv(std::env::args()) {
```

- [ ] **Step 3: Run tests.**

```bash
cargo test -p envoy-bin
```
Expected: same total count as after Task 11. The 6 argv tests move from `main.rs::argv_tests` to `argv::tests`; total is unchanged.

- [ ] **Step 4: Lint + fmt + workspace gate.**

```bash
cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
```
All expected: exit 0.

- [ ] **Step 5: Commit.**

```bash
git add crates/envoy-bin/src/argv.rs crates/envoy-bin/src/main.rs
git commit -m "phase 01: extract argv.rs from main.rs"
```

Append PROGRESS.md Task 12 entry.

If the executor judges this extraction is unnecessary churn (e.g. `main.rs` now fits on one screen), skip the task: PROGRESS.md records "Task 12 — argv.rs extraction skipped per SPEC §D4 'guidance only' escape clause; `main.rs` is <100 LoC" and the task index is renumbered accordingly.

---

### Task 13: Harness grammar — tagged `Driver` + refactored `Expectations`/`Equivalence`; regression tests

**Files:**
- Modify: `tests/differential/src/lib.rs`

**Why:** the phase-00 `Expectations` only models `response_body`. Fixture 0002 needs a `driver` discriminator and a `response_status` rule. SPEC §D5 lands both in one grammar bump; this task makes the change structurally (types + serde) before Task 14's `drive_http_get` helper and Task 15's dispatch touch `run_fixture`. Fixture 0001's YAML still uses the old shape — Task 16 migrates it after this task's regression tests prove the `kind: tcp_echo` form deserializes correctly.

- [ ] **Step 1: Write failing tests for the new grammar.**

Before modifying any types, append to `tests/differential/src/lib.rs::tests`:

```rust
#[test]
fn expectations_parse_tcp_echo_driver() {
    let yaml = r#"
driver:
  kind: tcp_echo
equivalence:
  response_body: byte_exact
"#;
    let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
    assert!(matches!(e.driver, Driver::TcpEcho));
    assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
    assert_eq!(e.equivalence.response_status, None);
}

#[test]
fn expectations_parse_http_get_driver() {
    let yaml = r#"
driver:
  kind: http_get
  path: /ready
  host: envoy-rust-phase-01
equivalence:
  response_status: exact
  response_body: byte_exact
"#;
    let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
    match e.driver {
        Driver::HttpGet { path, host } => {
            assert_eq!(path, "/ready");
            assert_eq!(host, "envoy-rust-phase-01");
        }
        _ => panic!("unexpected driver: {:?}", e.driver),
    }
    assert_eq!(e.equivalence.response_status, Some(StatusRule::Exact));
    assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
}

#[test]
fn expectations_reject_unknown_driver_kind() {
    let yaml = r#"
driver:
  kind: quantum_bogon
equivalence:
  response_body: byte_exact
"#;
    let r: Result<Expectations, _> = serde_yaml::from_str(yaml);
    assert!(r.is_err(), "quantum_bogon must not parse: {r:?}");
}
```

Run:

```bash
cargo test -p differential tests::expectations_parse_tcp_echo_driver
```
Expected: compile error because `Driver`, `StatusRule` do not yet exist, and `equivalence.response_body` is not an `Option`. Good — baseline failure recorded.

- [ ] **Step 2: Update the grammar types in `tests/differential/src/lib.rs`.**

Replace the existing `Expectations`/`Equivalence`/`BodyRule` block (between `pub mod subject;` and `pub fn load_expectations`) with the tagged-driver shape:

```rust
/// Contents of `<fixture>/expectations.yaml`. See SPEC §D5.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Expectations {
    pub driver: Driver,
    #[serde(default)]
    pub equivalence: Equivalence,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Driver {
    TcpEcho,
    HttpGet { path: String, host: String },
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Equivalence {
    #[serde(default)]
    pub response_status: Option<StatusRule>,
    #[serde(default)]
    pub response_body: Option<BodyRule>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StatusRule {
    Exact,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BodyRule {
    ByteExact,
}
```

The `rename_all = "snake_case"` + `deny_unknown_fields` on each enum prevents e.g. `byteexact` or `byte_Exact` silently accepting.

- [ ] **Step 3: Update the existing phase-00 `tests::expectations_*` tests to the new shape.**

Several phase-00 tests in the same module break because they assert the old `Expectations` layout. Minimal diff:

- `expectations_parse_byte_exact` — rewrite to wrap the `response_body` assertion in `Some(...)`; add a `driver: { kind: tcp_echo }` prefix to the yaml body.
- `expectations_reject_unknown_rule` — update yaml to include the `driver:` prefix; still asserts parse failure.
- `expectations_reject_unknown_field` + `equivalence_reject_unknown_field` — same yaml prefix.

Target shapes (inline into the tests module, replacing the pre-existing versions):

```rust
#[test]
fn expectations_parse_byte_exact() {
    let yaml = "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: byte_exact\n";
    let e: Expectations = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
    assert!(matches!(e.driver, Driver::TcpEcho));
}

#[test]
fn expectations_reject_unknown_rule() {
    let yaml = "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: sorta_equal\n";
    let r = serde_yaml::from_str::<Expectations>(yaml);
    assert!(r.is_err());
}

#[test]
fn expectations_reject_unknown_field() {
    let yaml = "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: byte_exact\nfoo: bar\n";
    let err = serde_yaml::from_str::<Expectations>(yaml)
        .expect_err("must reject unknown top-level field");
    let msg = err.to_string();
    assert!(msg.contains("unknown field"), "unexpected: {msg}");
}

#[test]
fn equivalence_reject_unknown_field() {
    let yaml = "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: byte_exact\n  extra: true\n";
    let err = serde_yaml::from_str::<Expectations>(yaml)
        .expect_err("must reject unknown nested field");
    let msg = err.to_string();
    assert!(msg.contains("unknown field"), "unexpected: {msg}");
}
```

- [ ] **Step 4: Run the differential lib tests.**

```bash
cargo test -p differential --lib
```
Expected: 12 pre-existing lib tests (adjusted) + 3 new grammar tests = 15 lib tests pass. `run_fixture`'s body still references `expectations.equivalence.response_body` without the `Option` unwrap — this step is about types only. Do NOT touch `run_fixture` yet; if rustc complains, Step 5 papers over with `Option::unwrap_or(BodyRule::ByteExact)` temporarily.

If `run_fixture` fails to compile: patch it with the minimum change (`.response_body.unwrap_or(BodyRule::ByteExact)`) to let the lib build. Task 15 rewrites `run_fixture` properly.

- [ ] **Step 5: Lint + fmt + workspace gate.**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
```
All expected: exit 0. Fixture 0001's `expectations.yaml` is still in the phase-00 shape, which now fails to parse under the new grammar — Task 16 migrates it. Until then, the `echo_fixture` acceptance test breaks. Gate this task's commit on `cargo test -p differential --lib` passing; the broken `echo_fixture` is expected and documented in the commit body.

- [ ] **Step 6: Commit.**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 01: harness grammar — tagged Driver enum"
```

Commit body should call out that `tests/differential/tests/echo.rs::echo_fixture` is temporarily broken pending Task 16's fixture migration — the test relies on the 0001 YAML being the old shape. This is the only instance in phase 01 where a commit intentionally lands red-on-CI state; Task 16 restores green immediately. Append PROGRESS.md Task 13 with that deviation note.

---

### Task 14: `tests/differential::drive_http_get` + `HttpResponse` + 4 unit tests

**Files:**
- Modify: `tests/differential/Cargo.toml` (add `httparse`)
- Modify: `tests/differential/src/lib.rs` (append `HttpResponse` + `drive_http_get` + 4 unit tests)

**Why:** the harness needs an HTTP/1.1 client that mirrors the admin endpoint's minimum request shape. SPEC §D5 specifies the exact form. This task adds the helper in isolation; Task 15 plugs it into `run_fixture`.

- [ ] **Step 1: Add `httparse` to `tests/differential/Cargo.toml`.**

```toml
[dependencies]
anyhow = "1"
httparse = "1"
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
tempfile = "3"
testcontainers = "0.23"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "process", "time"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
```

- [ ] **Step 2: Write the failing test `drive_http_get_round_trips`.**

Append to `tests/differential/src/lib.rs::tests`:

```rust
#[tokio::test]
async fn drive_http_get_round_trips() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Read the request (we don't parse — just drain until CRLFCRLF).
        let mut buf = [0u8; 512];
        let mut read = Vec::new();
        loop {
            let n = stream.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            read.extend_from_slice(&buf[..n]);
            if read.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\nconnection: close\r\n\r\nLIVE\n")
            .await
            .unwrap();
        drop(stream);
    });

    let resp = drive_http_get(addr, "/ready", "x").await.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"LIVE\n");
    server.await.unwrap();
}
```

Run:

```bash
cargo test -p differential tests::drive_http_get_round_trips
```
Expected: compile error, `cannot find function `drive_http_get``.

- [ ] **Step 3: Implement `HttpResponse` + `drive_http_get`.**

Append to `tests/differential/src/lib.rs` (above the `run_fixture` definition):

```rust
/// Decoded HTTP/1.1 response. Headers are captured for debug tracing but play
/// no part in the phase-01 equivalence diff (ADR-0011).
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    #[allow(dead_code)]
    pub headers: Vec<(String, Vec<u8>)>,
}

/// Open a TCP connection to `addr`, issue a minimal `GET` for `path` with
/// `Host: host`, and parse the response. Supports `content-length`-framed and
/// `connection: close`-framed responses only; that is enough for phase 01's
/// admin surface (SPEC §6 signpost 9).
pub async fn drive_http_get(addr: SocketAddr, path: &str, host: &str) -> Result<HttpResponse> {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Connection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await?;
    stream.flush().await.ok();

    let mut buf = Vec::with_capacity(2048);
    let mut scratch = [0u8; 2048];
    let head_end;
    loop {
        let n = stream.read(&mut scratch).await?;
        if n == 0 {
            bail!("{addr} closed before a response head was received");
        }
        buf.extend_from_slice(&scratch[..n]);

        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut resp = httparse::Response::new(&mut headers);
        match resp.parse(&buf) {
            Ok(httparse::Status::Complete(n)) => {
                head_end = n;
                let status = resp
                    .code
                    .ok_or_else(|| anyhow::anyhow!("missing response status code"))?;
                let mut captured_headers: Vec<(String, Vec<u8>)> = Vec::new();
                let mut content_length: Option<usize> = None;
                let mut connection_close = false;
                for h in resp.headers.iter() {
                    captured_headers.push((h.name.to_ascii_lowercase(), h.value.to_vec()));
                    if h.name.eq_ignore_ascii_case("content-length") {
                        let s = std::str::from_utf8(h.value)?.trim();
                        content_length = Some(s.parse()?);
                    } else if h.name.eq_ignore_ascii_case("connection") {
                        let s = std::str::from_utf8(h.value)?.trim();
                        if s.eq_ignore_ascii_case("close") {
                            connection_close = true;
                        }
                    }
                }

                // Drain the body.
                let body = match content_length {
                    Some(cl) => {
                        let mut body = Vec::with_capacity(cl);
                        let already = &buf[head_end..];
                        let take = already.len().min(cl);
                        body.extend_from_slice(&already[..take]);
                        if body.len() < cl {
                            let remaining = cl - body.len();
                            let mut rest = vec![0u8; remaining];
                            stream.read_exact(&mut rest).await?;
                            body.extend(rest);
                        }
                        body
                    }
                    None if connection_close => {
                        let mut body = Vec::new();
                        body.extend_from_slice(&buf[head_end..]);
                        stream.read_to_end(&mut body).await?;
                        body
                    }
                    None => bail!(
                        "{addr} response has neither `content-length` nor `connection: close`; \
                         drive_http_get does not support keep-alive in phase 01",
                    ),
                };

                return Ok(HttpResponse {
                    status,
                    body,
                    headers: captured_headers,
                });
            }
            Ok(httparse::Status::Partial) => continue,
            Err(e) => bail!("{addr} response parse error: {e}"),
        }
    }
}
```

- [ ] **Step 4: Re-run the test; verify it passes.**

```bash
cargo test -p differential tests::drive_http_get_round_trips
```
Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 5: Add the remaining 3 unit tests.**

Append:

```rust
#[tokio::test]
async fn drive_http_get_handles_explicit_content_length() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let _ = tokio::io::copy(
            &mut tokio::io::empty(),
            &mut tokio::io::BufWriter::new(&mut s),
        )
        .await;
        s.write_all(
            b"HTTP/1.1 404 Not Found\r\ncontent-length: 4\r\n\r\nNOPE"
        )
        .await
        .unwrap();
        // Hold open long enough for the client to read_exact the 4 bytes.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(s);
    });

    let resp = drive_http_get(addr, "/x", "h").await.unwrap();
    assert_eq!(resp.status, 404);
    assert_eq!(resp.body, b"NOPE");
}

#[tokio::test]
async fn drive_http_get_handles_connection_close_without_length() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        s.write_all(b"HTTP/1.1 200 OK\r\nconnection: close\r\n\r\nhello-close")
            .await
            .unwrap();
        drop(s);
    });

    let resp = drive_http_get(addr, "/x", "h").await.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"hello-close");
}

#[tokio::test]
async fn drive_http_get_rejects_malformed_response() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        s.write_all(b"this is not a valid http response\r\n\r\n")
            .await
            .unwrap();
        drop(s);
    });

    let err = drive_http_get(addr, "/x", "h")
        .await
        .expect_err("malformed must fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("parse") || msg.contains("invalid"), "got: {msg}");
}
```

- [ ] **Step 6: Full diff lib test run.**

```bash
cargo test -p differential --lib
```
Expected: 15 pre-existing tests (incl. 3 grammar from Task 13) + 4 new from this task = 19 lib tests, 0 failed, 1 ignored (the Docker-gated upstream test).

- [ ] **Step 7: Lint + fmt gate.**

```bash
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Both expected: exit 0.

- [ ] **Step 8: Commit.**

```bash
git add tests/differential/Cargo.toml tests/differential/src/lib.rs
git commit -m "phase 01: drive_http_get + HttpResponse in differential"
```

Append PROGRESS.md Task 14 entry.

---

### Task 15: `run_fixture` dispatch on `Driver::{TcpEcho, HttpGet}` + per-driver port templating

**Files:**
- Modify: `tests/differential/src/lib.rs`

**Why:** Task 13 added the Driver types; Task 14 added the HTTP client; this task wires them into the end-to-end `run_fixture` orchestrator. Per SPEC §D5, the two dispatch branches are:

- `TcpEcho` — pre-existing `drive_tcp` flow with `{{PORT}}` template substitution.
- `HttpGet { path, host }` — new `drive_http_get` flow with `{{ADMIN_PORT}}` template substitution.

After this task, `run_fixture` supports both drivers, but the 0001 fixture's YAML is still in the old grammar and its `echo_fixture` acceptance test is still red. Task 16 migrates 0001 and re-greens it.

- [ ] **Step 1: Replace the `render_yaml` signature to template per-driver keys.**

Old signature: `pub fn render_yaml(template: &str, port: u16) -> String`.

New: generic over `&[(key, value)]` so each caller supplies the keys that fixture expects. Replacement:

```rust
/// Template-render a fixture YAML by substituting literal `{{KEY}}` tokens.
/// The `kvs` list is the set of tokens to replace; any `{{…}}` token not in
/// `kvs` is left untouched so a typo surfaces as a parser error rather than
/// silently rendering to the empty string.
pub fn render_yaml(template: &str, kvs: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (k, v) in kvs {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}
```

Update the one pre-existing test to the new shape:

```rust
#[test]
fn render_yaml_substitutes_all_port_tokens() {
    let t = "a: {{PORT}}\nb: {{PORT}}\n";
    assert_eq!(
        render_yaml(t, &[("PORT", "9000")]),
        "a: 9000\nb: 9000\n"
    );
}
```

Add a driver-keyed test:

```rust
#[test]
fn render_yaml_substitutes_admin_port_key() {
    let t = "address: 127.0.0.1\nport: {{ADMIN_PORT}}\n";
    assert_eq!(
        render_yaml(t, &[("ADMIN_PORT", "9901")]),
        "address: 127.0.0.1\nport: 9901\n"
    );
}
```

- [ ] **Step 2: Rewrite `run_fixture` to dispatch on `Driver`.**

Replace the pre-existing `run_fixture` body:

```rust
pub async fn run_fixture(fixture_dir: &Path) -> Result<()> {
    let expectations = load_expectations(&fixture_dir.join("expectations.yaml"))?;

    // Reserve one port; use the driver-specific token to substitute into the
    // rendered configs. Upstream Envoy runs inside the container namespace and
    // listens on upstream::CONTAINER_PORT; envoy-rust listens on the host's
    // reserved port.
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
    let upstream_yaml = render_yaml(
        &upstream_template,
        &[(port_key, &upstream_port_str)],
    );
    let subject_yaml = render_yaml(
        &subject_template,
        &[(port_key, &subject_port_str)],
    );
    let upstream_path = write_temp(tmp.path(), "envoy.yaml", &upstream_yaml)?;
    let subject_path = write_temp(tmp.path(), "envoy-rust.yaml", &subject_yaml)?;

    let upstream = upstream::start(&upstream_path).await?;
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
                /* upstream status */ None,
                /* subject status  */ None,
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

    Ok(())
}

fn assert_equivalence(
    expectations: &Expectations,
    upstream_status: Option<u16>,
    subject_status: Option<u16>,
    upstream_body: &[u8],
    subject_body: &[u8],
) -> Result<()> {
    if matches!(expectations.equivalence.response_status, Some(StatusRule::Exact)) {
        match (upstream_status, subject_status) {
            (Some(u), Some(s)) if u == s => {}
            (u, s) => bail!(
                "response status mismatch under `response_status: exact`\n  \
                 upstream: {u:?}\n  subject:  {s:?}"
            ),
        }
    }
    if matches!(expectations.equivalence.response_body, Some(BodyRule::ByteExact))
        && upstream_body != subject_body
    {
        bail!(
            "byte-exact body mismatch\n  upstream: {upstream_body:?}\n  subject:  {subject_body:?}",
        );
    }
    // Neither rule configured → silently pass + log a warning (SPEC §D5).
    if expectations.equivalence.response_status.is_none()
        && expectations.equivalence.response_body.is_none()
    {
        tracing::warn!(
            "fixture has neither response_status nor response_body equivalence rule — running as a smoke test"
        );
    }
    Ok(())
}
```

- [ ] **Step 3: Delete the hard-coded `BodyRule::ByteExact` assertion at the top of the old `run_fixture` (that line is gone in the rewrite; confirm by grep).**

```bash
grep -n 'BodyRule::ByteExact' tests/differential/src/lib.rs
```
Expected output: two hits total — the enum variant definition, and the `matches!` in `assert_equivalence`. No `assert_eq!(... BodyRule::ByteExact, ...)` line remains at function top level.

- [ ] **Step 4: Run the lib tests.**

```bash
cargo test -p differential --lib
```
Expected: 21 passed, 1 ignored — 19 from Task 14 + 2 new from Step 1 (adjust count if the removed `render_yaml` old-signature test stays the same because it was updated). If a pre-existing test used the 1-arg `render_yaml(template, 9000)` form, update it to the new `&[("PORT", "9000")]` form.

- [ ] **Step 5: Lint + fmt gate.**

```bash
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Both expected: exit 0.

- [ ] **Step 6: Commit.**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 01: run_fixture dispatch on Driver + per-driver port templating"
```

Append PROGRESS.md Task 15 entry. Note that the `echo_fixture` integration test is still broken pending Task 16 (same red-state caveat as Task 13); this is the last commit where that's true.

---

### Task 16: Migrate `tests/fixtures/0001-tcp-echo/` to tagged driver grammar; regression-verify `echo_fixture`

**Files:**
- Modify: `tests/fixtures/0001-tcp-echo/expectations.yaml`
- Modify: `tests/fixtures/0001-tcp-echo/README.md`

**Why:** restores green `echo_fixture`. Minimal mechanical migration.

- [ ] **Step 1: Rewrite `tests/fixtures/0001-tcp-echo/expectations.yaml`.**

New contents:

```yaml
driver:
  kind: tcp_echo
equivalence:
  response_body: byte_exact
```

- [ ] **Step 2: Append a migration note to `tests/fixtures/0001-tcp-echo/README.md`.**

Append at the end (do not replace prior content):

```markdown

## Phase 01 migration

The `expectations.yaml` grammar acquired a tagged `driver:` discriminator in
phase 01 (SPEC §D5). The fixture's behavior is unchanged — it still drives
`inputs/payload.bin` at both proxies and asserts byte-exact bodies. The only
shape change is the new `driver: { kind: tcp_echo }` stanza.

Related ADRs: ADR-0008 (envoy-config extraction), ADR-0011 (header equivalence
deferred to phase 04).
```

- [ ] **Step 3: Run the full workspace test suite.**

```bash
cargo test --workspace
```
Expected (locally, without Docker): 21 differential lib tests pass, 1 ignored (Docker-gated); `echo_fixture` integration test fails locally on the Docker DNS bug per phase-00 PROGRESS.md Task 14 — this is expected. The real green signal comes from CI (Task 19).

Locally-verifiable check that the migration at least type-parses:

```bash
cargo test -p differential --lib tests::expectations_parse_tcp_echo_driver -- --exact
```
Expected: passes.

A dry-run against the fixture file (parse, don't execute):

```bash
cargo test -p differential --lib tests::fixture_0001_expectations_parses_as_tcp_echo -- --exact
```
…if this test exists. SPEC §D5 prescribes it as a structural regression; add it in Step 4.

- [ ] **Step 4: Add a structural regression test proving the on-disk fixture 0001 parses as `Driver::TcpEcho`.**

Append to `tests/differential/src/lib.rs::tests`:

```rust
#[test]
fn fixture_0001_expectations_parses_as_tcp_echo() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0001-tcp-echo/expectations.yaml");
    let e = load_expectations(&path).expect("parses");
    assert!(matches!(e.driver, Driver::TcpEcho));
    assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
    assert_eq!(e.equivalence.response_status, None);
}
```

The `../../tests/fixtures/0001-tcp-echo/...` path is relative to `tests/differential/Cargo.toml` (at `tests/differential/`). Adjust if the project layout differs — `env!("CARGO_MANIFEST_DIR")` is the authoritative anchor.

- [ ] **Step 5: Run the regression.**

```bash
cargo test -p differential --lib tests::fixture_0001_expectations_parses_as_tcp_echo -- --exact
```
Expected: passes.

- [ ] **Step 6: Commit.**

```bash
git add tests/fixtures/0001-tcp-echo/expectations.yaml tests/fixtures/0001-tcp-echo/README.md tests/differential/src/lib.rs
git commit -m "phase 01: migrate fixture 0001 to tagged driver grammar"
```

Append PROGRESS.md Task 16 entry. Note that the `echo_fixture` integration test is back to green in CI; the red-state commits from Tasks 13/15 are cleared by this one.

---

### Task 17: Create `tests/fixtures/0002-static-admin-ready/`

**Files:**
- Create: `tests/fixtures/0002-static-admin-ready/envoy.yaml`
- Create: `tests/fixtures/0002-static-admin-ready/envoy-rust.yaml`
- Create: `tests/fixtures/0002-static-admin-ready/expectations.yaml`
- Create: `tests/fixtures/0002-static-admin-ready/README.md`

**Why:** data-only task. Fixture is the input to Task 18's acceptance test.

- [ ] **Step 1: Write `tests/fixtures/0002-static-admin-ready/envoy.yaml`.**

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

SPEC §D7 notes upstream Envoy may demand `access_log_path: /dev/null` in the admin block. At execution time, if the upstream container rejects this YAML (testcontainers surfaces it as a stderr log + ready-check timeout), add that field and record the divergence in the fixture README — per D-3.5, no ADR needed for a pure Envoy-schema fit.

- [ ] **Step 2: Write `tests/fixtures/0002-static-admin-ready/envoy-rust.yaml`.**

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

- [ ] **Step 3: Write `tests/fixtures/0002-static-admin-ready/expectations.yaml`.**

```yaml
driver:
  kind: http_get
  path: /ready
  host: envoy-rust-phase-01
equivalence:
  response_status: exact
  response_body: byte_exact
```

- [ ] **Step 4: Write `tests/fixtures/0002-static-admin-ready/README.md`.**

```markdown
# Fixture 0002 — Static admin `/ready`

This fixture drives `GET /ready` at the admin endpoint of upstream Envoy
(`envoyproxy/envoy:v1.33.0`) and envoy-rust, asserting that both return
identical HTTP status (`200 OK`) and response body (`LIVE\n`). Header
equivalence is intentionally out of scope for phase 01 — the
`BEHAVIOR_CONTRACT.md` header allow-list is populated starting phase 04
(ADR-0011).

The `envoy.yaml` and `envoy-rust.yaml` differ only in bind address: upstream
Envoy runs inside a container and binds `0.0.0.0`; the envoy-rust subject
runs as a host subprocess and binds `127.0.0.1` (SPEC §6 signpost 3). Both
templates use the same `{{ADMIN_PORT}}` token; the harness substitutes each
side's reserved port independently.

This is the first fixture to use the `http_get` driver introduced by SPEC
§D5. Future admin fixtures (phase 08 for `/stats`, `/clusters`,
`/config_dump`, and drain) reuse the driver.

No `inputs/` directory: `http_get`'s payload (path + host) lives in
`expectations.yaml`.

## Known Envoy v1.33.0 quirks

If upstream Envoy rejects this YAML at container start (stderr shows a
validation error), add `access_log_path: /dev/null` to the `admin:` block —
some Envoy releases require that field on admin bootstraps. Record the fix
in this file's PROGRESS.md deviation note, not as an ADR.
```

- [ ] **Step 5: Verify the fixture files parse with the harness loaders.**

```bash
cargo test -p differential --lib tests::expectations_parse_http_get_driver -- --exact
```
Expected: passes (already added in Task 13 and re-run here as a smoke check).

Optional deeper check (only if you want fixture-specific regressions):

```bash
cargo test -p differential --lib tests::fixture_0002_expectations_parses_as_http_get -- --exact 2>/dev/null || true
```

If you add a `fixture_0002_expectations_parses_as_http_get` test mirroring Task 16's `fixture_0001_...` regression, land it in this task alongside the fixture files.

- [ ] **Step 6: Commit.**

```bash
git add tests/fixtures/0002-static-admin-ready
git commit -m "phase 01: fixture 0002-static-admin-ready [ADR-0011]"
```

Append PROGRESS.md Task 17 entry with the fixture file paths and sizes.

---

### Task 18: `tests/differential/tests/admin_ready.rs` acceptance test

**Files:**
- Create: `tests/differential/tests/admin_ready.rs`

**Why:** the real differential contract. Mirrors the phase-00 `echo.rs` structure — one `#[tokio::test]` that calls `differential::run_fixture` against the fixture dir. The actual equivalence comparison is inside `run_fixture` (from Task 15).

- [ ] **Step 1: Write the acceptance test.**

```rust
//! Phase 01 differential acceptance test: GET /ready on the admin endpoint
//! should produce identical status + body between upstream Envoy v1.33.0 and
//! envoy-rust. Docker-gated; in CI this runs on `ubuntu-latest` alongside the
//! phase-00 `echo_fixture` test.

use std::path::PathBuf;

#[tokio::test]
async fn admin_ready_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0002-static-admin-ready");
    differential::run_fixture(&dir).await.expect("fixture passes");
}
```

- [ ] **Step 2: Verify it compiles.**

```bash
cargo build --workspace --all-targets
```
Expected: exit 0.

- [ ] **Step 3: Run it locally (Docker required).**

```bash
cargo test -p differential --test admin_ready
```
Expected on a host with a working Docker daemon: `test admin_ready_fixture ... ok`. On hosts with the IPv6-DNS bug from phase-00 PROGRESS.md Task 3, expect the documented container-pull failure — CI is the authoritative validator (Task 19).

- [ ] **Step 4: Lint + fmt gate.**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Both expected: exit 0.

- [ ] **Step 5: Commit.**

```bash
git add tests/differential/tests/admin_ready.rs
git commit -m "phase 01: admin_ready differential acceptance test [ADR-0011]"
```

Append PROGRESS.md Task 18 entry. If the test passed locally, quote the tail; if it was Docker-skipped per the phase-00 host bug, note "CI-only validation per phase-00 Task 14 convention."

---

### Task 19: Phase-done gate (state 4) — run all 5 stable commands + CI fuzz job; quote outputs into PROGRESS.md

**Files:**
- Modify (append): `docs/envoy-rust/phases/01-static-bootstrap-config/PROGRESS.md`

**Why:** this is lifecycle state 4 per `SKILL_ROUTING.md`. The skill to invoke in the NEXT session (after all 18 task commits land) is `superpowers:verification-before-completion`. Because this plan is executed by `superpowers:subagent-driven-development`, treat Task 19 as the terminal "now verify" task — keep it in-plan so the executor does not forget to quote CI output into PROGRESS.md.

- [ ] **Step 1: Push all phase-01 commits to `origin/main` (or the phase-01 branch, if one is in use).**

```bash
git push
```

- [ ] **Step 2: Watch the CI run.**

```bash
gh run watch
```

Expected: both `build` and `fuzz` jobs green. If `fuzz` fails on a libfuzzer-sys license or an advisory on a transitive dep, **stop** and land ADR-0012 per D-3.5 with the appropriate `deny.toml` exception, commit, re-push, re-watch.

- [ ] **Step 3: Run the five local stable-toolchain gate commands, capturing tails for PROGRESS.md.**

```bash
cargo build --workspace --all-targets 2>&1 | tail -5
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5
cargo fmt --all -- --check && echo fmt ok
cargo test --workspace --lib --bins 2>&1 | tail -20
cargo deny check 2>&1 | tail -10
```

- [ ] **Step 4: Fetch the CI run conclusion and URL.**

```bash
gh run list --limit 1 --json databaseId,conclusion,url
```

Expected: `conclusion: success`, `url: https://github.com/.../actions/runs/<id>`.

- [ ] **Step 5: Append a "State 4 — Phase-done gate verification" section to PROGRESS.md.**

Quote the 5 command tails verbatim (include exit codes), the CI run URL + conclusion, and the gate outcome per BOOTSTRAP_PROMPT §7.5:

```markdown
## State 4 — Phase-done gate verification (2026-04-24)

Per `docs/envoy-rust/SKILL_ROUTING.md` state 4.

### Local gate (dev host)

- `cargo build --workspace --all-targets` → exit 0 (`Finished dev profile target(s) in …s`).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → exit 0 (`Finished`).
- `cargo fmt --all -- --check` → exit 0 (`fmt ok`).
- `cargo test --workspace --lib --bins` → exit 0 (`N passed; 0 failed; 1 ignored`).
- `cargo deny check` → exit 0 (`advisories ok, bans ok, licenses ok, sources ok`).

### CI gate (`ubuntu-latest`, run <ID>, HEAD <SHA>)

Run conclusion: `success`. URL: <URL>

- `build` job steps: fmt, clippy, build, test, install cargo-deny, cargo deny check → all `success`.
- `fuzz` job steps: nightly toolchain install, cargo-fuzz install, `cargo fuzz run parse_bootstrap -- -max_total_time=30` → `success`.

### Gate outcome per `BOOTSTRAP_PROMPT.md` §7.5

- (a) `tests/fixtures/0002-static-admin-ready/` → green (`admin_ready_fixture ... ok`).
- (b) `tests/fixtures/0001-tcp-echo/` → green (`echo_fixture ... ok`) post-migration.
- (c) no conformance suites this phase → n/a.
- (d) fuzz target `parse_bootstrap` → 30 s clean run, no crashes.
- (e) local stable-toolchain gate → all clean.
- (f) REVIEW.md → state 5 pending.

State 4 verification complete. Next session enters state 5 via
`superpowers:requesting-code-review`.
```

- [ ] **Step 6: Commit.**

```bash
git add docs/envoy-rust/phases/01-static-bootstrap-config/PROGRESS.md
git commit -m "phase 01: state 4 — phase-done gate verified"
```

- [ ] **Step 7: Stop.**

Do NOT advance to state 5 in this session — SKILL_ROUTING.md line 17–48 reserves that for the NEXT session, which invokes `superpowers:requesting-code-review` to produce `REVIEW.md`. The STATE.md update to "state 5" happens at the end of this session per the same file's "Any session mutating project state must end by updating this file" rule — see the final "Plan completion" section below.

---

## Plan completion

After all 19 tasks land and PROGRESS.md is written through state 4, the session that finishes this plan must:

1. **Update `docs/envoy-rust/STATE.md`** to record that phase 01 has advanced to lifecycle state 4 (verified; not yet reviewed) and that the next expected skill is `superpowers:requesting-code-review`. Use the phase-00 STATE.md shape (see git history `880efcd` for the analogous state-6 advance).
2. **Commit** that STATE.md advance as `phase 01: STATE advance to state 4 (verified; REVIEW next)`.
3. **Stop.** Per BOOTSTRAP_PROMPT.md §5.1, each session moves exactly one lifecycle state forward; this plan takes phase 01 from state 2 (SPEC landed) through state 4 (verified). States 5 (review) and 6 (final commit + ROADMAP flip + STATE advance to phase 02) are the next two sessions.

If the §5 splitting gate fires retrospectively during execution (e.g. an ADR's rewrite balloons a task past the threshold), stop the current task and split phase 01 at SPEC §5's pre-identified 01.1 / 01.2 cut — do not negotiate the split after the fact.

---

## Self-review

Spec coverage map:

- SPEC §D1 (envoy-config crate) → Tasks 2, 3, 4, 5.
- SPEC §D2 (fuzz subcrate) → Task 6.
- SPEC §D3 (admin HTTP endpoint) → Tasks 9, 10.
- SPEC §D4 (binary entrypoint wiring) → Tasks 11, 12.
- SPEC §D5 (differential harness grammar) → Tasks 13, 14, 15.
- SPEC §D6 (fixture 0001 migration) → Task 16.
- SPEC §D7 (fixture 0002) → Task 17; acceptance test at Task 18.
- SPEC §D8 (CI workflow) → Task 7.
- SPEC §D9 (ADRs) → Tasks 1, 8 (ADR-0008/0009/0010/0011); ADR-0012 conditional on cargo-deny surface (Task 7 Step 4 + Task 19 Step 2).

Phase-done gate map (BOOTSTRAP_PROMPT.md §7.5):

- (a) fixture 0002 green → Task 18 + Task 19.
- (b) fixture 0001 green → Task 16 + Task 19.
- (c) no conformance suites → noted in SPEC §1; no task needed.
- (d) fuzz target clean → Task 7 (CI wiring) + Task 19 (validate).
- (e) local stable gate → Task 19.
- (f) REVIEW.md → next session (state 5); out of plan scope.

Splitting-gate self-check (§6.1 of BOOTSTRAP_PROMPT.md / SPEC §5):

- Task count: 19, well under the ~25 bound.
- Estimated net LoC change: ADRs ~300 lines, envoy-config crate ~200 lines of code + ~300 lines of tests, fuzz subcrate ~40 lines + seed, admin.rs ~200 lines + ~200 lines of tests, main.rs delta ~70 lines, argv.rs ~100 lines (moved), differential grammar + drive_http_get + dispatch ~250 lines + ~200 lines of tests, fixtures ~50 lines, CI ~30 lines. Total ≈ 1900 lines — over the ~1500 bound on the surface, but roughly half is tests + ADR prose. Per SPEC §5's "thresholds exist to catch overscoping, not to enforce a shape," and given that SPEC §5 pre-identified the split without pre-emptively applying it, the plan is kept unified. If the executor finds a single task crossing its own bite-sized budget (e.g. Task 4 expanding beyond ~250 LoC of tests because of a serde-version quirk), split phase 01 at the SPEC §5 cut line immediately per the deviation protocol.
- Cross-task type consistency: `ConfigError` name/variants match across Tasks 4, 5, 11; `Driver::{TcpEcho, HttpGet { path, host }}` variant names match across Tasks 13, 14, 15, 16, 17; `HttpResponse { status, body, headers }` fields match between Tasks 14 and 15; `render_yaml` new signature consistent between its definition (Task 15) and both call sites in `run_fixture` (Task 15).

Placeholder scan: no "TBD", "fill in later", "similar to Task N", or untyped "add error handling" instructions remain.

---
