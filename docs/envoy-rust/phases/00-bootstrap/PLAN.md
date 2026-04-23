# Phase 00 — Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Do not skip to `superpowers:executing-plans` unless directed by the user.

**Goal:** Land the scaffolding and one green differential TCP-echo fixture so every later phase has a workspace, a CI pipeline, a pinned upstream Envoy, and a reusable differential harness.

**Architecture:** One binary crate (`crates/envoy-bin`) that runs a narrow echo proxy driven by a `-c <path>` YAML, plus one test crate (`tests/differential`) that orchestrates upstream Envoy via `testcontainers` and envoy-rust via subprocess and compares their responses on identical inputs. The differential harness exposes a single public async entrypoint (`run_fixture`) that every future phase will reuse.

**Tech Stack:** Rust 2024 on toolchain `1.95.0` (already pinned in `rust-toolchain.toml`), `tokio`, `serde` + `serde_yaml`, `anyhow`, `tracing` + `tracing-subscriber`, `testcontainers`. CI on GitHub Actions, `ubuntu-latest`, with `cargo-deny` enforcement.

**Scope check:** this is a single subsystem (bootstrap + harness + first fixture). Not splittable by independent subsystem.

**Size check:** 15 tasks; estimated ~700 net LoC of code + ~200 LoC of config/YAML/CI. Under both §6 thresholds (25 tasks / 1500 LoC). Do **not** split unless mid-execution any single task's sub-step count blows past ~10 and the executor judges a sub-phase cleaner.

**Doctrine reminders for every task:**
- **D-3.8 / Invariant 8:** every new Rust file that is a crate root (`lib.rs` or `main.rs`) starts with `#![forbid(unsafe_code)]`. Non-root modules do not need the attribute — the crate-wide forbid already covers them.
- **D-3.1 / `superpowers:test-driven-development`:** every code task follows the strict 5-step TDD cycle below. No code before a failing test.
- **D-3.2:** direct dependencies in every `Cargo.toml` must come only from the permitted-foundations list. Transitive leaks of forbidden crates (`hyper`, `tower`, etc.) are addressed in `deny.toml` with a `skip-tree` entry, not by adding direct deps.
- **D-3.4:** every artifact must be readable cold by a stranger.
- **D-3.5:** unlisted ambiguities → append a new ADR in `docs/envoy-rust/DECISIONS.md` and proceed.

**Verification at end of phase (§7.5 gate, scoped to this phase):**
```
cargo build   --workspace --all-targets
cargo clippy  --workspace --all-targets --all-features -- -D warnings
cargo fmt     --all -- --check
cargo test    --workspace
cargo deny    check
```
All green. No new fuzz targets this phase. No conformance suites this phase. Full log quoted into `docs/envoy-rust/phases/00-bootstrap/PROGRESS.md` per state 4 of the lifecycle.

**PROGRESS.md discipline:** the executor appends a 3–6 line entry to `docs/envoy-rust/phases/00-bootstrap/PROGRESS.md` at the end of every task (after commit), naming the task number, the commit SHA, and the exit status of any commands it ran. The file is created by the executor on its first task (the plan does not ask for a dedicated "seed PROGRESS.md" task).

---

## File Structure

New files (create):

| Path | Responsibility |
|---|---|
| `.github/workflows/ci.yml` | single-job CI on `ubuntu-latest` |
| `crates/envoy-bin/Cargo.toml` | binary crate manifest |
| `crates/envoy-bin/src/main.rs` | argv, tracing init, signal wiring, glue |
| `crates/envoy-bin/src/config.rs` | Bootstrap YAML types + parser + validation |
| `crates/envoy-bin/src/echo.rs` | `serve(listener, shutdown)` accept loop + per-connection echo + 5s drain |
| `tests/differential/Cargo.toml` | test crate manifest |
| `tests/differential/src/lib.rs` | `run_fixture` orchestrator + expectations/port/ready helpers + driver |
| `tests/differential/src/upstream.rs` | upstream Envoy testcontainers launcher + `UpstreamProxy` guard |
| `tests/differential/src/subject.rs` | envoy-rust subprocess launcher + `ChildGuard` |
| `tests/differential/tests/echo.rs` | `#[tokio::test] echo_fixture` acceptance test |
| `tests/fixtures/0001-tcp-echo/envoy.yaml` | upstream Envoy config (minus rendered port) |
| `tests/fixtures/0001-tcp-echo/envoy-rust.yaml` | envoy-rust config (same shape) |
| `tests/fixtures/0001-tcp-echo/inputs/payload.bin` | 18-byte deterministic payload |
| `tests/fixtures/0001-tcp-echo/expectations.yaml` | equivalence rules for this fixture |
| `tests/fixtures/0001-tcp-echo/README.md` | one-paragraph fixture description |

Files modified (amend):

| Path | Change |
|---|---|
| `Cargo.toml` | populate `members = ["crates/envoy-bin", "tests/differential"]` |
| `deny.toml` | add `skip-tree = [{ name = "testcontainers" }]` to `[bans]` |
| `docs/envoy-rust/DECISIONS.md` | append ADR-0002, ADR-0003, ADR-0004 |
| `docs/envoy-rust/ENVOY_TARGET.md` | resolve all TBD fields |

Files untouched but relied on:

- `rust-toolchain.toml` (already pins `1.95.0` + `rustfmt` + `clippy`).
- `docs/envoy-rust/MISSION.md` (not amended).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (no new rows).
- `docs/envoy-rust/ROADMAP.md` (status flipped to `done` by the phase-commit step after REVIEW, not by this plan).
- `docs/envoy-rust/STATE.md` (advanced by the phase-commit step after REVIEW, not by this plan).

## TDD Cycle (applies to every code task)

Every code task follows this exact 5-step cycle:

1. Write the failing test.
2. Run the test, confirm it fails **with the expected failure mode** (compilation error on new symbol, or assertion failure on expected behavior).
3. Write the minimal implementation.
4. Run the test, confirm it passes.
5. Commit.

Non-code tasks (ADRs, fixture YAMLs, CI workflow) skip to "write → verify → commit."

---

## Task 1 — Land ADR-0002: GitHub Actions as the CI provider

**Files:**
- Modify: `docs/envoy-rust/DECISIONS.md`

- [ ] **Step 1: Append ADR-0002 to `docs/envoy-rust/DECISIONS.md`**

Append (preserving the existing `ADR-0001` block; do **not** modify it):

```markdown

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
```

- [ ] **Step 2: Verify the file still parses as Markdown**

Run:
```bash
test -s docs/envoy-rust/DECISIONS.md && grep -q '^## ADR-0002' docs/envoy-rust/DECISIONS.md
```
Expected: exit status 0.

- [ ] **Step 3: Commit**

```bash
git add docs/envoy-rust/DECISIONS.md
git commit -m "phase 00: ADR-0002 — GitHub Actions as CI provider"
```

---

## Task 2 — Land ADR-0003: Rust edition 2024 for all workspace crates

**Files:**
- Modify: `docs/envoy-rust/DECISIONS.md`

- [ ] **Step 1: Append ADR-0003**

Append to `docs/envoy-rust/DECISIONS.md`:

```markdown

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
```

- [ ] **Step 2: Verify**

```bash
grep -q '^## ADR-0003' docs/envoy-rust/DECISIONS.md
```
Expected: exit status 0.

- [ ] **Step 3: Commit**

```bash
git add docs/envoy-rust/DECISIONS.md
git commit -m "phase 00: ADR-0003 — Rust edition 2024"
```

---

## Task 3 — Land ADR-0004 and resolve `ENVOY_TARGET.md`

**Files:**
- Modify: `docs/envoy-rust/DECISIONS.md`
- Modify: `docs/envoy-rust/ENVOY_TARGET.md`

- [ ] **Step 1: Resolve the pin fields**

Run each command and capture its output exactly. None of these values is allowed to be guessed — they come from these commands:

```bash
# 1. Pull the image and capture its Docker content digest.
docker pull envoyproxy/envoy:v1.33.0
docker inspect --format '{{index .RepoDigests 0}}' envoyproxy/envoy:v1.33.0
# Expected output shape: envoyproxy/envoy@sha256:<64 hex chars>
# Record the "sha256:<hex>" portion; this is the DIGEST.

# 2. Resolve the git SHA that tag v1.33.0 points to in the upstream repo.
git ls-remote --tags https://github.com/envoyproxy/envoy refs/tags/v1.33.0
# Output looks like: <40-hex-sha>\trefs/tags/v1.33.0  (or refs/tags/v1.33.0^{} for annotated tags).
# If both are present, use the peeled ref (refs/tags/v1.33.0^{}) — that is the commit SHA;
# record it as PROTO_TREE_COMMIT.

# 3. Release notes URL — always:
echo "https://github.com/envoyproxy/envoy/releases/tag/v1.33.0"
```

If any command fails (tag removed, registry down, network blocked), land an ADR-0005 that re-baselines to the next LTS (`v1.32.x`) and redo this task against that tag. Do **not** leave any field TBD.

- [ ] **Step 2: Rewrite `docs/envoy-rust/ENVOY_TARGET.md`**

Replace the entire file contents with (substituting `<DIGEST>` and `<PROTO_TREE_COMMIT>` with the values from Step 1):

```markdown
# Upstream Envoy Target Pin

> Pinned during phase 00. Upgrading this pin is its own phase per doctrine rule
> D-3.7 and supersedes ADR-0004 with a new ADR.

## Pin

- **Image:** `envoyproxy/envoy:v1.33.0`
- **Digest:** `<DIGEST>`
- **Upstream release notes:** https://github.com/envoyproxy/envoy/releases/tag/v1.33.0
- **Proto tree commit:** `<PROTO_TREE_COMMIT>`
- **xDS transport version:** v3

## How to refresh the pin

Upgrading the pin is its own phase per doctrine rule D-3.7. The refresh phase must:

1. Open a new phase in `ROADMAP.md` titled "Refresh upstream Envoy pin to <new-tag>", depending on the most recent trunk/feature phase.
2. Add an ADR that supersedes `ADR-0004`, naming the old digest, new digest, new tag, and any doctrine-surface changes in the release notes.
3. Re-run every existing differential fixture against the new image. Any red fixture is either a product fix (update envoy-rust) or a contract fix (update `BEHAVIOR_CONTRACT.md`, documented in the same or a follow-up ADR), never both silently — per doctrine rule D-3.3.
4. Update this file with the new fields and commit.

This file is otherwise not edited outside a refresh phase.
```

- [ ] **Step 3: Append ADR-0004**

Append to `docs/envoy-rust/DECISIONS.md`:

```markdown

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
```

- [ ] **Step 4: Verify**

```bash
grep -q '^## ADR-0004' docs/envoy-rust/DECISIONS.md
grep -q 'sha256:' docs/envoy-rust/ENVOY_TARGET.md
grep -q 'Proto tree commit:' docs/envoy-rust/ENVOY_TARGET.md
```
Expected: all three exit status 0, and neither file contains the literal string `TBD` anywhere.

- [ ] **Step 5: Commit**

```bash
git add docs/envoy-rust/DECISIONS.md docs/envoy-rust/ENVOY_TARGET.md
git commit -m "phase 00: ADR-0004 — pin envoyproxy/envoy:v1.33.0"
```

---

## Task 4 — Workspace scaffolding: both crate skeletons wired in

**Files:**
- Create: `crates/envoy-bin/Cargo.toml`
- Create: `crates/envoy-bin/src/main.rs`
- Create: `tests/differential/Cargo.toml`
- Create: `tests/differential/src/lib.rs`
- Modify: `Cargo.toml` (root)
- Modify: `deny.toml`

- [ ] **Step 1: Write `crates/envoy-bin/Cargo.toml`**

```toml
[package]
name = "envoy-bin"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[[bin]]
name = "envoy-bin"
path = "src/main.rs"

[dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "signal"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
```

- [ ] **Step 2: Write `crates/envoy-bin/src/main.rs` (skeleton)**

```rust
#![forbid(unsafe_code)]

fn main() {
    // Replaced by Task 8 with the real wiring. Exists so `cargo build` has
    // something to link.
}
```

- [ ] **Step 3: Write `tests/differential/Cargo.toml`**

```toml
[package]
name = "differential"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "differential"
path = "src/lib.rs"

[dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
testcontainers = "0.23"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "process", "time"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "process", "time"] }
```

- [ ] **Step 4: Write `tests/differential/src/lib.rs` (skeleton)**

```rust
#![forbid(unsafe_code)]

//! Differential test harness for envoy-rust. See
//! `docs/envoy-rust/phases/00-bootstrap/SPEC.md` §4 (D4) and
//! `docs/envoy-rust/BEHAVIOR_CONTRACT.md` for the contract this harness enforces.

// Public surface is populated by later tasks. This crate compiles on its own
// so the workspace-level green-build gate (D-3.6) holds after Task 4.
```

- [ ] **Step 5: Update the root workspace `Cargo.toml`**

Replace the entire `Cargo.toml` at the repo root with:

```toml
[workspace]
resolver = "2"
members = [
    "crates/envoy-bin",
    "tests/differential",
]

# Workspace members grow as later phases introduce crates. Phase 00 lands the
# binary (`crates/envoy-bin`) and the differential harness (`tests/differential`).
```

- [ ] **Step 6: Add `testcontainers` skip-tree to `deny.toml`**

`testcontainers` transitively depends on `bollard`, which depends on `hyper`. `hyper` is in the `[bans] deny` list (D-3.2). Without an exemption, `cargo deny check` fails. Replace the current `skip = []` / `skip-tree = []` lines in `deny.toml` with:

```toml
skip = []
skip-tree = [
    # testcontainers -> bollard -> hyper. D-3.2 forbids `hyper` as a DIRECT dep;
    # transitive via a permitted foundation (testcontainers) is acceptable.
    # If another crate later pulls hyper through a different path, this skip-tree
    # won't hide it — the ban still fires on non-testcontainers ancestry.
    { name = "testcontainers" },
]
```

No ADR required — D-3.2 explicitly allows transitive leaks from permitted foundations ("Transitive pulls through permitted foundations — notably `hyper` and `tower` via `tonic` — are allowed"), and `testcontainers` is a permitted foundation.

- [ ] **Step 7: Verify the whole green-build gate on an empty workspace**

Run, in order:
```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
```
Expected: all exit status 0. `cargo test --workspace` reports "running 0 tests" for both crates, which is fine (empty but well-formed).

If `cargo deny check` flags anything beyond the known `hyper` leak, triage per doctrine D-3.5: either extend `deny.toml` or land an ADR. Do **not** broaden the `allow` list blindly.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml deny.toml crates/envoy-bin tests/differential
git commit -m "phase 00: workspace skeleton — envoy-bin + differential crates"
```

---

## Task 5 — `envoy-bin/src/config.rs`: Bootstrap YAML types + parser + echo-filter validation

**Files:**
- Create: `crates/envoy-bin/src/config.rs`
- Modify: `crates/envoy-bin/src/main.rs` (add `mod config;`)

- [ ] **Step 1: Write the failing test (a unit test at the bottom of `config.rs`)**

Create `crates/envoy-bin/src/config.rs` with:

```rust
use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Bootstrap {
    pub static_resources: StaticResources,
}

#[derive(Debug, Deserialize)]
pub struct StaticResources {
    pub listeners: Vec<Listener>,
}

#[derive(Debug, Deserialize)]
pub struct Listener {
    #[allow(dead_code)]
    pub name: String,
    pub address: Address,
    pub filter_chains: Vec<FilterChain>,
}

#[derive(Debug, Deserialize)]
pub struct Address {
    pub socket_address: SocketAddress,
}

#[derive(Debug, Deserialize)]
pub struct SocketAddress {
    pub address: String,
    pub port_value: u16,
}

#[derive(Debug, Deserialize)]
pub struct FilterChain {
    pub filters: Vec<NetworkFilter>,
}

#[derive(Debug, Deserialize)]
pub struct NetworkFilter {
    pub name: String,
}

/// The only network filter name envoy-rust recognizes in phase 00.
pub const ECHO_FILTER: &str = "envoy.filters.network.echo";

pub fn parse_bootstrap(yaml: &str) -> Result<Bootstrap> {
    let bootstrap: Bootstrap =
        serde_yaml::from_str(yaml).context("parsing bootstrap YAML")?;
    validate(&bootstrap)?;
    Ok(bootstrap)
}

fn validate(bootstrap: &Bootstrap) -> Result<()> {
    let listeners = &bootstrap.static_resources.listeners;
    if listeners.is_empty() {
        bail!("bootstrap has no listeners; phase 00 requires exactly one");
    }
    if listeners.len() > 1 {
        bail!(
            "bootstrap has {} listeners; phase 00 supports exactly one",
            listeners.len()
        );
    }
    for listener in listeners {
        for chain in &listener.filter_chains {
            for filter in &chain.filters {
                if filter.name != ECHO_FILTER {
                    bail!(
                        "unsupported network filter '{}'; phase 00 accepts only '{}'",
                        filter.name,
                        ECHO_FILTER,
                    );
                }
            }
        }
    }
    Ok(())
}

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
    fn parses_minimal_bootstrap() {
        let b = parse_bootstrap(MINIMAL).expect("valid YAML");
        let port = b.static_resources.listeners[0].address.socket_address.port_value;
        assert_eq!(port, 10000);
        assert_eq!(b.static_resources.listeners[0].address.socket_address.address, "0.0.0.0");
    }

    #[test]
    fn rejects_non_echo_filter() {
        let yaml = MINIMAL.replace(
            "envoy.filters.network.echo",
            "envoy.filters.network.tcp_proxy",
        );
        let err = parse_bootstrap(&yaml).expect_err("must reject tcp_proxy");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("unsupported network filter"),
            "unexpected error message: {msg}",
        );
    }

    #[test]
    fn rejects_empty_listeners() {
        let yaml = "static_resources:\n  listeners: []\n";
        let err = parse_bootstrap(yaml).expect_err("must reject empty listeners");
        assert!(format!("{err:#}").contains("no listeners"));
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
        let err = parse_bootstrap(yaml).expect_err("must reject 2 listeners");
        assert!(format!("{err:#}").contains("phase 00 supports exactly one"));
    }

    #[test]
    fn rejects_malformed_yaml() {
        let err = parse_bootstrap("::: not yaml :::").expect_err("parser must fail");
        assert!(format!("{err:#}").contains("parsing bootstrap YAML"));
    }
}
```

Also update `crates/envoy-bin/src/main.rs` to register the module:

```rust
#![forbid(unsafe_code)]

mod config;

fn main() {
    // Replaced by Task 8 with the real wiring.
}
```

- [ ] **Step 2: Run the tests — they should fail the first time you write only the test block (i.e., write the tests module first, then run; but since we wrote everything at once above, this step verifies the tests actually pass)**

If you prefer a strict red-green cycle, do Step 1 in two commits: first land only the test module with `use super::*;` pointing at undefined symbols, confirm compile error, then add the `Bootstrap` types + `parse_bootstrap`. Either approach satisfies TDD as long as you watch the failure mode before landing the implementation.

Run:
```bash
cargo test -p envoy-bin --lib config
```
Expected if you did red-green in two passes: first run fails with `error[E0433]: failed to resolve: ...`; second run passes all five tests.

If you landed both halves in one commit (pragmatic for a small pure-function module), verify now:
```bash
cargo test -p envoy-bin --lib config
```
Expected: `test result: ok. 5 passed`.

- [ ] **Step 3: Run full clippy on the crate**

```bash
cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings
cargo fmt -p envoy-bin -- --check
```
Expected: both exit status 0. Fix formatting/lints before moving on; a phase cannot land dirty.

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-bin/src/config.rs crates/envoy-bin/src/main.rs
git commit -m "phase 00: envoy-bin config parser + echo-filter validation"
```

---

## Task 6 — `envoy-bin`: hand-rolled argv parser

**Files:**
- Modify: `crates/envoy-bin/src/main.rs` (add `parse_argv` + tests)

- [ ] **Step 1: Write the failing tests in `main.rs`**

Replace `crates/envoy-bin/src/main.rs` with:

```rust
#![forbid(unsafe_code)]

use std::path::PathBuf;

mod config;

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

/// Phase 00 accepts exactly one flag: `-c <path>` or `--config-path <path>`.
/// `clap` is deliberately avoided (not on the D-3.2 permitted-foundations list).
/// When argv grows past a single path, land an ADR and revisit.
pub fn parse_argv<I, S>(args: I) -> Result<PathBuf, ArgvError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut iter = args.into_iter().map(Into::into);
    // Drop argv[0] (program name).
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

fn main() {
    // Replaced by Task 8 with the real wiring.
}

#[cfg(test)]
mod argv_tests {
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
        matches!(err, ArgvError::Trailing(_));
    }
}
```

- [ ] **Step 2: Run and verify failure mode before implementation (optional two-pass split)**

If you did one commit with both tests + impl, skip to verification. Otherwise land the tests alone first and confirm they fail to compile before adding `parse_argv`.

Run:
```bash
cargo test -p envoy-bin --bin envoy-bin argv_tests
```
Expected after impl: `test result: ok. 6 passed`.

- [ ] **Step 3: Clippy + fmt**

```bash
cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings
cargo fmt -p envoy-bin -- --check
```
Expected: exit status 0.

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-bin/src/main.rs
git commit -m "phase 00: envoy-bin hand-rolled argv parser"
```

---

## Task 7 — `envoy-bin/src/echo.rs`: async TCP echo server with drain

**Files:**
- Create: `crates/envoy-bin/src/echo.rs`
- Modify: `crates/envoy-bin/src/main.rs` (add `mod echo;`)

- [ ] **Step 1: Write the failing test (an integration test inside the same file)**

Create `crates/envoy-bin/src/echo.rs` with:

```rust
use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio::time::timeout;

/// Graceful drain budget per D3 step 5 of the SPEC (5 seconds).
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Accept loop. Each accepted connection copies bytes from the read half to the
/// write half until the client half-closes, mirroring Envoy's
/// `envoy.filters.network.echo` filter.
///
/// Returns `Ok(())` after a clean drain on `shutdown`. Individual connection
/// errors are logged via `tracing::warn!` and do not propagate; a connection
/// failure never takes down the server.
pub async fn serve<F>(listener: TcpListener, shutdown: F) -> Result<()>
where
    F: Future<Output = ()>,
{
    let mut set: JoinSet<()> = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => {
                tracing::info!("shutdown signal received; closing listener");
                drop(listener);
                break;
            }
            accept = listener.accept() => {
                match accept {
                    Ok((stream, peer)) => {
                        tracing::debug!(%peer, "accepted connection");
                        set.spawn(async move {
                            if let Err(err) = echo_once(stream).await {
                                tracing::warn!(%peer, error = %err, "echo connection failed");
                            }
                        });
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "accept failed; continuing");
                    }
                }
            }
        }
    }

    // Drain: wait up to DRAIN_TIMEOUT for all in-flight echoes to finish.
    let in_flight = set.len();
    tracing::info!(in_flight, "draining in-flight connections");
    let drained = timeout(DRAIN_TIMEOUT, async {
        while set.join_next().await.is_some() {}
    })
    .await;
    if drained.is_err() {
        tracing::warn!("drain timeout; aborting remaining tasks");
        set.shutdown().await;
    }
    Ok(())
}

async fn echo_once(mut stream: tokio::net::TcpStream) -> Result<()> {
    let (mut reader, mut writer) = stream.split();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            writer.shutdown().await.ok();
            return Ok(());
        }
        writer.write_all(&buf[..n]).await?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::oneshot;

    async fn bind_random_local() -> TcpListener {
        TcpListener::bind(("127.0.0.1", 0)).await.expect("bind :0")
    }

    #[tokio::test]
    async fn echoes_single_payload_and_drains_on_shutdown() {
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

        let mut client = TcpStream::connect(addr).await.unwrap();
        let payload = b"hello, envoy-rust\n";
        client.write_all(payload).await.unwrap();
        client.shutdown().await.unwrap();
        let mut echoed = Vec::new();
        client.read_to_end(&mut echoed).await.unwrap();
        assert_eq!(echoed, payload);

        tx.send(()).unwrap();
        timeout(Duration::from_secs(5), server)
            .await
            .expect("server exits within drain window")
            .unwrap();
    }

    #[tokio::test]
    async fn handles_two_concurrent_connections() {
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

        let one = tokio::spawn(async move {
            let mut c = TcpStream::connect(addr).await.unwrap();
            c.write_all(b"AAA").await.unwrap();
            c.shutdown().await.unwrap();
            let mut out = Vec::new();
            c.read_to_end(&mut out).await.unwrap();
            out
        });
        let two = tokio::spawn(async move {
            let mut c = TcpStream::connect(addr).await.unwrap();
            c.write_all(b"BBBB").await.unwrap();
            c.shutdown().await.unwrap();
            let mut out = Vec::new();
            c.read_to_end(&mut out).await.unwrap();
            out
        });

        assert_eq!(one.await.unwrap(), b"AAA");
        assert_eq!(two.await.unwrap(), b"BBBB");

        tx.send(()).unwrap();
        timeout(Duration::from_secs(5), server).await.unwrap().unwrap();
    }
}
```

Update `crates/envoy-bin/src/main.rs` to register the module — replace the current file with:

```rust
#![forbid(unsafe_code)]

use std::path::PathBuf;

mod config;
mod echo;

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

fn main() {
    // Replaced by Task 8 with the real wiring.
}

#[cfg(test)]
mod argv_tests {
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
        matches!(err, ArgvError::Trailing(_));
    }
}
```

- [ ] **Step 2: Run and verify failure mode (optional two-pass)**

If you want a strict red-green, land the test module first with bodies referencing `serve` before the impl exists — compile will fail. Otherwise verify pass directly:

```bash
cargo test -p envoy-bin --bin envoy-bin echo::tests
```
Expected: `test result: ok. 2 passed`.

- [ ] **Step 3: Clippy + fmt**

```bash
cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings
cargo fmt -p envoy-bin -- --check
```
Expected: exit status 0.

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-bin/src/echo.rs crates/envoy-bin/src/main.rs
git commit -m "phase 00: envoy-bin TCP echo server with drain"
```

---

## Task 8 — `envoy-bin/src/main.rs`: wire argv → config → tracing → serve → signals

**Files:**
- Modify: `crates/envoy-bin/src/main.rs`

- [ ] **Step 1: Replace `fn main()` and add an `async fn run()` + signal future**

Replace the current `fn main()` in `crates/envoy-bin/src/main.rs` with the following. Keep the existing `parse_argv`, `ArgvError`, `mod config`, `mod echo`, and `argv_tests` blocks intact.

```rust
use anyhow::{Context, Result};
use std::net::SocketAddr;
use tokio::net::TcpListener;

fn main() -> std::process::ExitCode {
    match parse_argv(std::env::args()) {
        Ok(path) => {
            install_tracing();
            match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("building tokio runtime")
            {
                Ok(rt) => match rt.block_on(run(path)) {
                    Ok(()) => std::process::ExitCode::SUCCESS,
                    Err(err) => {
                        tracing::error!(error = ?err, "envoy-rust exited with error");
                        std::process::ExitCode::from(1)
                    }
                },
                Err(err) => {
                    eprintln!("{err:#}");
                    std::process::ExitCode::from(1)
                }
            }
        }
        Err(err) => {
            eprintln!("envoy-bin: {err}");
            std::process::ExitCode::from(2)
        }
    }
}

fn install_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};
    let filter = EnvFilter::try_from_env("ENVOY_RUST_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_target(false).init();
}

async fn run(config_path: std::path::PathBuf) -> Result<()> {
    let yaml = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config at {}", config_path.display()))?;
    let bootstrap = config::parse_bootstrap(&yaml)?;
    let sock = &bootstrap.static_resources.listeners[0].address.socket_address;
    let addr: SocketAddr = format!("{}:{}", sock.address, sock.port_value).parse()
        .with_context(|| format!("parsing address {}:{}", sock.address, sock.port_value))?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding to {addr}"))?;
    tracing::info!(%addr, "envoy-rust listening");
    echo::serve(listener, shutdown_signal()).await?;
    tracing::info!("envoy-rust exited cleanly");
    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM");
    let mut intr = signal(SignalKind::interrupt()).expect("install SIGINT");
    tokio::select! {
        _ = term.recv() => tracing::info!("SIGTERM received"),
        _ = intr.recv() => tracing::info!("SIGINT received"),
    }
}
```

Because `main()` is now synchronous and explicitly builds the runtime, the `tokio::main` attribute is **not** used. This avoids a subtle interaction between `ExitCode` and `#[tokio::main]` and keeps the signal-handling code readable.

- [ ] **Step 2: Verify the crate still builds and all earlier tests still pass**

```bash
cargo build -p envoy-bin --release
cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings
cargo fmt -p envoy-bin -- --check
cargo test -p envoy-bin
```
Expected: all green. The built binary lives at `target/release/envoy-bin` (or `target/debug/envoy-bin` for `cargo build -p envoy-bin`). Sanity-check with:

```bash
./target/release/envoy-bin
# Expected: prints "envoy-bin: expected exactly one of `-c <path>` or `--config-path <path>`" and exits with code 2.
echo "exit: $?"
```

- [ ] **Step 3: Commit**

```bash
git add crates/envoy-bin/src/main.rs
git commit -m "phase 00: envoy-bin main wiring — argv, tracing, serve, signal"
```

---

## Task 9 — `tests/differential/src/lib.rs`: expectations, port, ready, renderer

**Files:**
- Modify: `tests/differential/src/lib.rs`

- [ ] **Step 1: Write the failing unit tests + the helper functions**

Replace `tests/differential/src/lib.rs` with:

```rust
#![forbid(unsafe_code)]

//! Differential test harness for envoy-rust. Phase 00 surface: TCP echo.
//!
//! Contract: `run_fixture(fixture_dir)` starts upstream Envoy (via
//! testcontainers) and envoy-rust (via subprocess) against the fixture's paired
//! configs, drives the fixture's `inputs/payload.bin` at both, and asserts the
//! responses are byte-exact equal per `expectations.yaml`.

use std::io::Write;
use std::net::TcpListener as StdTcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub mod subject;
pub mod upstream;

/// Contents of `<fixture>/expectations.yaml`.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Expectations {
    pub equivalence: Equivalence,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Equivalence {
    pub response_body: BodyRule,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BodyRule {
    ByteExact,
}

pub fn load_expectations(path: &Path) -> Result<Expectations> {
    let yaml = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let parsed: Expectations = serde_yaml::from_str(&yaml)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(parsed)
}

/// Reserve a free TCP port on 127.0.0.1. Binds `:0`, reads the assigned port,
/// drops the listener, and returns the number.
///
/// TOCTOU: between the drop and the subsequent bind by envoy-rust, another
/// process on the host could grab this port. This is accepted for a
/// pre-production harness per SPEC §6 point 6. If CI flakes materialize, this
/// becomes its own split phase with a port-range reservation strategy.
pub fn reserve_port() -> Result<u16> {
    let listener = StdTcpListener::bind(("127.0.0.1", 0))
        .context("binding 127.0.0.1:0 to reserve a port")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Template-render a fixture YAML by substituting the literal `{{PORT}}` token.
pub fn render_yaml(template: &str, port: u16) -> String {
    template.replace("{{PORT}}", &port.to_string())
}

/// Write `content` to a new temp file in `dir` and return the path. The caller
/// is responsible for ensuring `dir` is already created.
pub fn write_temp(dir: &Path, name: &str, content: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path)
        .with_context(|| format!("creating {}", path.display()))?;
    f.write_all(content.as_bytes())?;
    Ok(path)
}

/// Poll `addr` with exponential backoff (starting at 50ms, doubling, capped at
/// 500ms) until a TCP connect succeeds or `budget` elapses. Returns `Err` on
/// timeout.
pub async fn wait_accept_ready(
    addr: std::net::SocketAddr,
    budget: Duration,
) -> Result<()> {
    let deadline = std::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match tokio::net::TcpStream::connect(addr).await {
            Ok(_) => return Ok(()),
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(err) => bail!("{addr} not accept-ready within {budget:?}: {err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn expectations_parse_byte_exact() {
        let yaml = "equivalence:\n  response_body: byte_exact\n";
        let e: Expectations = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(e.equivalence.response_body, BodyRule::ByteExact);
    }

    #[test]
    fn expectations_reject_unknown_rule() {
        let yaml = "equivalence:\n  response_body: sorta_equal\n";
        let r = serde_yaml::from_str::<Expectations>(yaml);
        assert!(r.is_err());
    }

    #[test]
    fn render_yaml_substitutes_all_port_tokens() {
        let t = "a: {{PORT}}\nb: {{PORT}}\n";
        assert_eq!(render_yaml(t, 9000), "a: 9000\nb: 9000\n");
    }

    #[test]
    fn reserve_port_returns_nonzero() {
        let p = reserve_port().unwrap();
        assert!(p > 0);
    }

    #[tokio::test]
    async fn wait_accept_ready_succeeds_for_listening_socket() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        wait_accept_ready(addr, Duration::from_secs(1)).await.unwrap();
    }

    #[tokio::test]
    async fn wait_accept_ready_times_out_for_closed_socket() {
        let listener = StdTcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        drop(listener);
        let result = wait_accept_ready(addr, Duration::from_millis(200)).await;
        assert!(result.is_err());
    }
}
```

Also create placeholder module files so the `pub mod subject;` / `pub mod upstream;` don't break the build — Tasks 10 and 11 will fill them in. Create both files with just the forbid attribute:

`tests/differential/src/subject.rs`:
```rust
// Populated by Task 11.
```

`tests/differential/src/upstream.rs`:
```rust
// Populated by Task 10.
```

- [ ] **Step 2: Run the tests**

```bash
cargo test -p differential --lib
```
Expected: `test result: ok. 6 passed`.

- [ ] **Step 3: Clippy + fmt**

```bash
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt -p differential -- --check
```
Expected: exit status 0.

- [ ] **Step 4: Commit**

```bash
git add tests/differential
git commit -m "phase 00: differential harness helpers — expectations, port, ready"
```

---

## Task 10 — `tests/differential/src/upstream.rs`: upstream Envoy testcontainers launcher

**Files:**
- Modify: `tests/differential/src/upstream.rs`

- [ ] **Step 1: Write the failing integration test + the launcher**

Replace `tests/differential/src/upstream.rs` with:

```rust
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{IntoContainerPort, Mount, WaitFor},
    runners::AsyncRunner,
};

/// Matches ADR-0004 / `docs/envoy-rust/ENVOY_TARGET.md`.
pub const IMAGE_NAME: &str = "envoyproxy/envoy";
pub const IMAGE_TAG: &str = "v1.33.0";
/// Container-internal listener port. Host-side port is assigned by
/// testcontainers at runtime and reported via `host_port()`.
pub const CONTAINER_PORT: u16 = 10000;

/// Running upstream Envoy. Dropping this handle stops the container.
pub struct UpstreamProxy {
    _container: ContainerAsync<GenericImage>,
    host_port: u16,
}

impl UpstreamProxy {
    pub fn host_port(&self) -> u16 {
        self.host_port
    }
}

/// Start upstream Envoy with `envoy_yaml_path` bind-mounted to
/// `/etc/envoy/envoy.yaml`. The caller must have already rendered any
/// `{{PORT}}` token in the YAML to `CONTAINER_PORT`.
pub async fn start(envoy_yaml_path: &Path) -> Result<UpstreamProxy> {
    let absolute = envoy_yaml_path
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", envoy_yaml_path.display()))?;
    let image = GenericImage::new(IMAGE_NAME, IMAGE_TAG)
        .with_exposed_port(CONTAINER_PORT.tcp())
        .with_wait_for(WaitFor::message_on_stderr("starting main dispatch loop"));
    let container = image
        .with_cmd(["-c", "/etc/envoy/envoy.yaml", "--log-level", "info"])
        .with_mount(Mount::bind_mount(
            absolute.to_string_lossy().to_string(),
            "/etc/envoy/envoy.yaml",
        ))
        .start()
        .await
        .context("starting upstream envoy container")?;
    let host_port = container
        .get_host_port_ipv4(CONTAINER_PORT.tcp())
        .await
        .context("reading host-mapped port from testcontainers")?;
    // Testcontainers reports the port as soon as Docker maps it; Envoy itself
    // may still be initializing. Give it a conservative extra second.
    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(UpstreamProxy {
        _container: container,
        host_port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_envoy_yaml() -> tempfile::NamedTempFile {
        // Smallest legal bootstrap that starts an echo listener. The container
        // listens on CONTAINER_PORT internally.
        let yaml = format!(
            r#"
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.echo.v3.Echo
"#,
            port = CONTAINER_PORT,
        );
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(yaml.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[tokio::test]
    #[ignore = "requires Docker; runs under `cargo test --workspace` in CI"]
    async fn starts_upstream_envoy_and_exposes_host_port() {
        let yaml = tmp_envoy_yaml();
        let proxy = start(yaml.path()).await.unwrap();
        assert!(proxy.host_port() > 0);
        // Validate accept-readiness via the library's own helper.
        let addr: std::net::SocketAddr =
            format!("127.0.0.1:{}", proxy.host_port()).parse().unwrap();
        crate::wait_accept_ready(addr, Duration::from_secs(15)).await.unwrap();
        drop(proxy);
    }
}
```

Add the `tempfile` dev-dependency to `tests/differential/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "process", "time"] }
```

- [ ] **Step 2: Run the test to confirm the code compiles and the `#[ignore]` test is skipped by default**

```bash
cargo test -p differential --lib
```
Expected: the new module compiles; the `#[ignore]`d test is reported as `1 ignored`.

- [ ] **Step 3: Run the ignored test manually to exercise the integration path (optional but recommended)**

```bash
cargo test -p differential --lib -- --ignored
```
Expected: pulls `envoyproxy/envoy:v1.33.0` if not cached; container starts; test passes. This run is the TDD "implementation passes" step for this module. If Docker is unavailable locally, skip this step and rely on CI to exercise it.

- [ ] **Step 4: Clippy + fmt**

```bash
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt -p differential -- --check
cargo deny check
```
Expected: all green. `cargo deny check` may surface new transitives from `testcontainers`; the `skip-tree` from Task 4 covers the hyper/tower path.

- [ ] **Step 5: Commit**

```bash
git add tests/differential
git commit -m "phase 00: differential — upstream envoy testcontainers launcher"
```

---

## Task 11 — `tests/differential/src/subject.rs`: envoy-rust subprocess + ChildGuard

**Files:**
- Modify: `tests/differential/src/subject.rs`

- [ ] **Step 1: Write the failing integration test + the launcher**

Replace `tests/differential/src/subject.rs` with:

```rust
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::process::{Child, Command};

/// A running envoy-rust subprocess. Dropping aborts it; calling `shutdown`
/// sends SIGTERM and waits for clean exit.
pub struct Subject {
    child: Option<Child>,
    port: u16,
}

impl Subject {
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Terminate the subprocess via SIGKILL (tokio's `start_kill`) and wait
    /// up to `budget` for it to exit. Graceful-drain on SIGTERM is covered by
    /// envoy-bin's own unit tests in Task 7; this harness path only needs the
    /// process to end deterministically between fixture runs.
    pub async fn shutdown(&mut self, budget: Duration) -> Result<()> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        child.start_kill().ok();
        match tokio::time::timeout(budget, child.wait()).await {
            Ok(Ok(_status)) => Ok(()),
            Ok(Err(err)) => bail!("waiting for envoy-rust: {err}"),
            Err(_) => bail!("envoy-rust did not exit within {budget:?}"),
        }
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Best-effort cleanup on test failure. SIGKILL + forget.
            let _ = child.start_kill();
        }
    }
}

/// Locate the envoy-bin binary built by `cargo test --workspace`. The test
/// crate does not declare envoy-bin as a dependency (no `artifact = "bin"` on
/// stable as of rustc 1.95.0), so we compute the path by convention:
/// `<workspace_root>/target/<profile>/envoy-bin`, honoring `CARGO_TARGET_DIR`.
pub fn locate_envoy_bin() -> Result<PathBuf> {
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
    let mut bin = target_dir.join(profile).join("envoy-bin");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    if !bin.exists() {
        bail!(
            "envoy-bin not found at {}; run `cargo build -p envoy-bin` or `cargo test --workspace`",
            bin.display(),
        );
    }
    Ok(bin)
}

pub async fn start(config_path: &Path, port: u16) -> Result<Subject> {
    let bin = locate_envoy_bin()?;
    let child = Command::new(&bin)
        .arg("-c")
        .arg(config_path)
        .env("ENVOY_RUST_LOG", "info")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawning {}", bin.display()))?;
    Ok(Subject {
        child: Some(child),
        port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn locate_envoy_bin_points_at_target_dir() {
        // This test assumes `cargo test --workspace` was the entry point, so
        // envoy-bin is already built. Under `cargo test -p differential` in
        // isolation it may fail — that is the documented caveat.
        if let Err(err) = locate_envoy_bin() {
            eprintln!(
                "skipping: {err}\n\
                 (run `cargo build -p envoy-bin` or use `cargo test --workspace`)",
            );
            return;
        }
        let p = locate_envoy_bin().unwrap();
        assert!(p.ends_with("envoy-bin") || p.ends_with("envoy-bin.exe"));
    }

    #[tokio::test]
    async fn starts_and_shuts_down_envoy_rust() {
        if locate_envoy_bin().is_err() {
            eprintln!("skipping: envoy-bin not built");
            return;
        }
        let port = crate::reserve_port().unwrap();
        let yaml = format!(
            r#"
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
"#,
        );
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(yaml.as_bytes()).unwrap();
        f.flush().unwrap();
        let mut subject = start(f.path(), port).await.unwrap();
        let addr = format!("127.0.0.1:{port}").parse().unwrap();
        crate::wait_accept_ready(addr, Duration::from_secs(5))
            .await
            .unwrap();
        subject.shutdown(Duration::from_secs(5)).await.unwrap();
    }
}
```

- [ ] **Step 2: Run the tests (from the workspace root so envoy-bin is built)**

```bash
cargo test --workspace subject::tests
```
Expected: both tests pass. If the first run after adding the file fails with "envoy-bin not found", run `cargo build -p envoy-bin` first and retry.

- [ ] **Step 3: Clippy + fmt**

```bash
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt -p differential -- --check
```
Expected: exit status 0.

- [ ] **Step 4: Commit**

```bash
git add tests/differential/src/subject.rs
git commit -m "phase 00: differential — envoy-rust subprocess + ChildGuard"
```

---

## Task 12 — Fixture 0001-tcp-echo: files

**Files:**
- Create: `tests/fixtures/0001-tcp-echo/envoy.yaml`
- Create: `tests/fixtures/0001-tcp-echo/envoy-rust.yaml`
- Create: `tests/fixtures/0001-tcp-echo/inputs/payload.bin`
- Create: `tests/fixtures/0001-tcp-echo/expectations.yaml`
- Create: `tests/fixtures/0001-tcp-echo/README.md`

This task does not need a TDD test cycle — the files are data consumed by the acceptance test in Task 14.

- [ ] **Step 1: Create the fixture directory and files**

```bash
mkdir -p tests/fixtures/0001-tcp-echo/inputs
```

Write `tests/fixtures/0001-tcp-echo/envoy.yaml` (upstream Envoy config — the container-internal port is substituted with the literal `{{PORT}}` token; the harness renders it to the `CONTAINER_PORT` constant at runtime):

```yaml
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.echo.v3.Echo
```

Write `tests/fixtures/0001-tcp-echo/envoy-rust.yaml` (envoy-rust config — structurally identical modulo the typed_config boilerplate that upstream Envoy requires but envoy-rust's narrow parser ignores):

```yaml
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
```

Write `tests/fixtures/0001-tcp-echo/inputs/payload.bin` (exact 18-byte payload; no trailing newline beyond the one inside the string). Use a one-shot Python invocation to guarantee the byte count:

```bash
python3 - <<'PY'
from pathlib import Path
Path("tests/fixtures/0001-tcp-echo/inputs/payload.bin").write_bytes(
    b"hello, envoy-rust\n"
)
PY
wc -c tests/fixtures/0001-tcp-echo/inputs/payload.bin
# Expected: 18 tests/fixtures/0001-tcp-echo/inputs/payload.bin
```

If Python is unavailable, use:
```bash
printf 'hello, envoy-rust\n' > tests/fixtures/0001-tcp-echo/inputs/payload.bin
wc -c tests/fixtures/0001-tcp-echo/inputs/payload.bin
```

Write `tests/fixtures/0001-tcp-echo/expectations.yaml`:

```yaml
equivalence:
  response_body: byte_exact
```

Write `tests/fixtures/0001-tcp-echo/README.md`:

```markdown
# Fixture 0001-tcp-echo

This fixture drives identical bytes at upstream Envoy's
`envoy.filters.network.echo` filter and at envoy-rust's phase-00 echo listener,
asserting byte-exact response body equivalence. It is the first differential
fixture in the project and establishes the harness contract for subsequent
TCP fixtures.

- **Payload:** `inputs/payload.bin` — 18 bytes of deterministic ASCII
  (`hello, envoy-rust\n`). Kept trivially inspectable.
- **Equivalence:** body byte-exact; no header/trailer/stat/timing clauses.
- **Port:** templated via `{{PORT}}`; rendered by the harness. envoy-rust binds
  the rendered port directly; upstream Envoy binds it inside the container and
  testcontainers host-maps to a random port.
```

- [ ] **Step 2: Verify the payload byte count and that the YAMLs are syntactically valid**

```bash
wc -c tests/fixtures/0001-tcp-echo/inputs/payload.bin
# Expected: 18
python3 -c 'import yaml,sys; [yaml.safe_load(open(p)) for p in sys.argv[1:]]' \
    tests/fixtures/0001-tcp-echo/envoy-rust.yaml \
    tests/fixtures/0001-tcp-echo/expectations.yaml
echo "yaml: $?"
# Expected: 0. (The upstream envoy.yaml contains {{PORT}} which is not valid
# YAML until rendered — do not validate it directly.)
```

- [ ] **Step 3: Commit**

```bash
git add tests/fixtures/0001-tcp-echo
git commit -m "phase 00: fixture 0001-tcp-echo files"
```

---

## Task 13 — `tests/differential/src/lib.rs`: `run_fixture` orchestrator + driver

**Files:**
- Modify: `tests/differential/src/lib.rs`

- [ ] **Step 1: Append the driver + orchestrator to `lib.rs`**

Append the following to the end of `tests/differential/src/lib.rs` (after the existing `#[cfg(test)] mod tests { ... }` block):

```rust
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Drive `payload` at `addr`: open TCP, write payload, half-close the write
/// side, read to EOF. Returns the echoed bytes.
pub async fn drive_tcp(addr: SocketAddr, payload: &[u8]) -> Result<Vec<u8>> {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    stream.write_all(payload).await?;
    stream.shutdown().await?;
    let mut out = Vec::with_capacity(payload.len());
    stream.read_to_end(&mut out).await?;
    Ok(out)
}

/// End-to-end run of one fixture. Panics-on-failure paths unwind through Drop
/// guards so the container and envoy-rust subprocess are cleaned up even on
/// assertion failure.
pub async fn run_fixture(fixture_dir: &Path) -> Result<()> {
    let expectations = load_expectations(&fixture_dir.join("expectations.yaml"))?;
    assert_eq!(
        expectations.equivalence.response_body,
        BodyRule::ByteExact,
        "phase 00 only understands response_body: byte_exact",
    );

    // Shared port number — upstream Envoy uses it inside the container's
    // namespace, envoy-rust binds it on the host.
    let host_port = reserve_port()?;

    // Render and materialize both configs in a temp directory.
    let tmp = tempfile::tempdir().context("creating fixture temp dir")?;
    let upstream_template =
        std::fs::read_to_string(fixture_dir.join("envoy.yaml"))
            .context("reading upstream envoy.yaml")?;
    let subject_template =
        std::fs::read_to_string(fixture_dir.join("envoy-rust.yaml"))
            .context("reading envoy-rust.yaml")?;
    let upstream_yaml = render_yaml(&upstream_template, upstream::CONTAINER_PORT);
    let subject_yaml = render_yaml(&subject_template, host_port);
    let upstream_path = write_temp(tmp.path(), "envoy.yaml", &upstream_yaml)?;
    let subject_path = write_temp(tmp.path(), "envoy-rust.yaml", &subject_yaml)?;

    // Start both proxies. Upstream first because it is slower to become ready.
    let upstream = upstream::start(&upstream_path).await?;
    let mut subject = subject::start(&subject_path, host_port).await?;

    let upstream_addr: SocketAddr =
        format!("127.0.0.1:{}", upstream.host_port()).parse()?;
    let subject_addr: SocketAddr =
        format!("127.0.0.1:{}", subject.port()).parse()?;

    // 10s accept-ready budget per SPEC §D4 step 4.
    let budget = Duration::from_secs(10);
    wait_accept_ready(upstream_addr, budget).await
        .context("upstream Envoy never became accept-ready")?;
    wait_accept_ready(subject_addr, budget).await
        .context("envoy-rust never became accept-ready")?;

    // Drive identical bytes at both and compare.
    let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
        .context("reading payload.bin")?;
    let upstream_out = drive_tcp(upstream_addr, &payload).await
        .context("upstream envoy drive")?;
    let subject_out = drive_tcp(subject_addr, &payload).await
        .context("envoy-rust drive")?;

    // Graceful subject shutdown so Drop doesn't SIGKILL unnecessarily.
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);

    if upstream_out != subject_out {
        bail!(
            "byte-exact body mismatch\n  upstream: {upstream_out:?}\n  subject:  {subject_out:?}",
        );
    }
    Ok(())
}
```

Add `tempfile` to the (runtime) dependencies of `tests/differential/Cargo.toml` — it's used by `run_fixture`, not just the tests — so move it out of dev-dependencies:

```toml
[dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
tempfile = "3"
testcontainers = "0.23"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "process", "time"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "process", "time"] }
```

- [ ] **Step 2: Verify compilation of the full workspace**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Expected: exit status 0.

The acceptance test for `run_fixture` itself is Task 14. Do **not** try to add a unit test for `run_fixture` here — the fixture file it would consume only exists once Task 14 lands (which comes with the `#[tokio::test]` integration point).

- [ ] **Step 3: Commit**

```bash
git add tests/differential
git commit -m "phase 00: differential — run_fixture orchestrator + driver"
```

---

## Task 14 — `tests/differential/tests/echo.rs`: the green acceptance test

**Files:**
- Create: `tests/differential/tests/echo.rs`

- [ ] **Step 1: Write the acceptance test**

Create `tests/differential/tests/echo.rs`:

```rust
use std::path::Path;

#[tokio::test]
async fn echo_fixture() -> anyhow::Result<()> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/0001-tcp-echo");
    differential::run_fixture(&fixture).await
}
```

This is the phase-done gate's (a) "all new differential fixtures are green" — if this test passes, the fixture is green.

- [ ] **Step 2: Run the test**

```bash
cargo test --workspace --test echo -- --nocapture
```
Expected: `test echo_fixture ... ok` with some tracing log output. First run pulls the `envoyproxy/envoy:v1.33.0` image (may take a minute). Subsequent runs are fast (cached container image, fresh container per test).

If this test fails:
- **Compare output mode:** use `--nocapture` to see upstream vs. subject bodies.
- **Port flake:** the TOCTOU window on `reserve_port` is known (SPEC §6 point 6). Retry once; if it still flakes, invoke `superpowers:systematic-debugging` before landing a workaround.
- **Docker unavailable:** cannot be solved in this phase — CI runs on `ubuntu-latest` with Docker-in-Docker enabled by default.
- **envoy-bin exited early:** re-run envoy-bin manually against the rendered `envoy-rust.yaml` (emitted into a temp dir by `run_fixture`; copy the path out of the test log) to see its stderr.

- [ ] **Step 3: Full phase-done gate dry-run**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
```
Expected: all exit status 0. Capture the output and keep it to paste into `PROGRESS.md` in the verification step (state 4 of the lifecycle, handled after this plan is complete).

- [ ] **Step 4: Commit**

```bash
git add tests/differential/tests/echo.rs
git commit -m "phase 00: differential — echo_fixture acceptance test"
```

---

## Task 15 — `.github/workflows/ci.yml`: GitHub Actions workflow

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Write the workflow**

```bash
mkdir -p .github/workflows
```

Create `.github/workflows/ci.yml`:

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
  build-test-lint:
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
```

- [ ] **Step 2: Validate the workflow file**

```bash
python3 -c 'import yaml,sys; yaml.safe_load(open(sys.argv[1]))' .github/workflows/ci.yml
echo "yaml: $?"
# Expected: 0
```

If `actionlint` is available locally, also run it for a deeper check. It is not required (CI will catch any issues on the first push).

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "phase 00: GitHub Actions CI workflow"
```

---

## Post-plan lifecycle steps (not implementation tasks — reference only)

After Task 15, the executor returns control to the state machine:

- **State 4 (verification):** invoke `superpowers:verification-before-completion`; re-run the phase-done gate (all five `cargo …` commands) and paste outputs into `docs/envoy-rust/phases/00-bootstrap/PROGRESS.md`.
- **State 5 (review):** invoke `superpowers:requesting-code-review`; produce `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`. If the review flags issues, return to **state 3** (not state 4) per §5.2.
- **State 6 (commit + advance):** squash-or-amend is **not** used; each task already committed. Instead:
  - Flip `docs/envoy-rust/ROADMAP.md` row `00` → `status = done`.
  - Advance `docs/envoy-rust/STATE.md`: active = `01-static-bootstrap-config`, next-skill = `superpowers:brainstorming`.
  - Land a final commit matching the SPEC §8 format:
    ```
    phase 00: Bootstrap [ADR-0002, ADR-0003, ADR-0004]

    Workspace skeleton, CI, toolchain + deny policy, reference Envoy pin,
    differential harness skeleton, and a TCP echo fixture exercised against
    upstream envoyproxy/envoy:v1.33.0.

    Differential surface: tests/fixtures/0001-tcp-echo green (byte-exact body).
    Conformance: none.
    ```

These three items are bookkeeping after the code is approved. They are **not** in the plan because a REVIEW failure could send the code back to state 3 and re-open the plan.

---

## Spec coverage audit (self-review)

| SPEC deliverable | Covered by |
|---|---|
| D1 — Workspace members + `forbid(unsafe_code)` | Task 4 (skeleton) + Task 5/7 (inherited by new files) + Task 9 (inherited) |
| D2 — `ENVOY_TARGET.md` fully populated | Task 3 |
| D3.1 — argv `-c` / `--config-path` only | Task 6 |
| D3.2 — Bootstrap YAML types + narrow parser | Task 5 |
| D3.2 — reject non-echo filter, exit 1 | Task 5 + Task 8 (exit code wiring) |
| D3.3 — TCP accept loop + echo | Task 7 |
| D3.4 — tracing subscriber from `ENVOY_RUST_LOG` | Task 8 (`install_tracing`) |
| D3.5 — SIGTERM/SIGINT drain, 5s grace | Task 7 (`serve` takes shutdown future; DRAIN_TIMEOUT=5s) + Task 8 (`shutdown_signal`) |
| D3.6 — single-listener only | Task 5 (`validate` rejects len > 1) |
| D4 — harness `run_fixture(&Path)` async entrypoint | Task 13 |
| D4 step 1 — read expectations.yaml | Task 9 (`load_expectations`) |
| D4 step 2 — testcontainers upstream Envoy | Task 10 |
| D4 step 3 — envoy-rust subprocess via binary path | Task 11 (`locate_envoy_bin` + `subject::start`) |
| D4 step 4 — poll until accept-ready | Task 9 (`wait_accept_ready`) |
| D4 step 5 — drive payload, half-close, compare | Task 13 (`drive_tcp` + `run_fixture`) |
| D4 step 6 — Drop-guard cleanup | Task 10 (`UpstreamProxy` drops container) + Task 11 (`Subject` Drop + kill_on_drop) |
| D4 step 7 — returns `Ok(())` on success | Task 13 |
| D4 — `tests/differential/tests/echo.rs` | Task 14 |
| D5 — fixture files | Task 12 |
| D6 — GitHub Actions CI | Task 15 |
| D7 — ADR-0002 | Task 1 |
| D7 — ADR-0003 | Task 2 |
| D7 — ADR-0004 | Task 3 |

**No gaps.** No placeholders in the plan. Type names (`Bootstrap`, `UpstreamProxy`, `Subject`, `run_fixture`, `drive_tcp`, `reserve_port`, `wait_accept_ready`, `render_yaml`, `load_expectations`, `Expectations`, `Equivalence`, `BodyRule`, `ECHO_FILTER`, `CONTAINER_PORT`, `IMAGE_NAME`, `IMAGE_TAG`, `DRAIN_TIMEOUT`) are consistent across tasks.
