# envoy-rust Bootstrap Prompt — Design Spec

Date: 2026-04-23
Status: approved (brainstorming)
Deliverable: `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md`

---

## 1. Purpose

Produce a self-contained bootstrap prompt that, when loaded as the first user message into a fresh Claude Code session with the `superpowers` plugin active, drives an indefinite-duration, phase-based project to reimplement the Envoy Proxy in **Rust**, with every phase differentially verified against upstream Envoy.

This prompt is the Rust counterpart of `/Users/esa/git/envoy-go/BOOTSTRAP_PROMPT.md`. The Go prompt is treated as the canonical reference for structure, section ordering, doctrine enforcement verbs, and phase-lifecycle semantics. The Rust prompt replicates that structure verbatim everywhere that is language-neutral, and substitutes Rust-specific content in a small, enumerated set of locations.

## 2. Principle

**Structure identical, substance Rust.** The Go prompt's phase lifecycle state machine, six-part phase-done gate, MVP trunk ordering, feature families, and cold-start discipline are language-independent and are copied verbatim. The substitutions are confined to:

1. The docs path (`docs/envoy-go/` → `docs/envoy-rust/`)
2. The permitted-foundations list and forbidden list in doctrine D-3.2
3. The verification command set in §5 step 4 and §7.5
4. The repo layout in §4 (Cargo workspace, per-subsystem crates)
5. Two new doctrine rules unique to Rust: D-3.8 (`unsafe` policy) and D-3.9 (toolchain pin)
6. The first-session bootstrap scaffold in §10 (Cargo workspace files)

Everything else is mechanical string substitution of "Go" for "Rust" and identifier renames.

## 3. Repo Layout

The prompt's §4 tree is rewritten to a Cargo workspace with one crate per Envoy subsystem:

```
envoy-rust/
├── README.md
├── Cargo.toml                          # workspace root (members list grows per phase)
├── Cargo.lock
├── rust-toolchain.toml                 # pinned compiler version (D-3.9)
├── deny.toml                           # cargo-deny policy (license + advisory enforcement)
├── crates/
│   ├── envoy-bin/                      # the binary (src/main.rs)
│   ├── envoy-protos/                   # generated from upstream envoy .proto via prost-build + tonic-build
│   ├── envoy-listener/
│   ├── envoy-cluster/
│   ├── envoy-filter/                   # filter-chain engine (written from scratch)
│   ├── envoy-http/   envoy-tcp/   envoy-tls/
│   ├── envoy-xds/    envoy-admin/   envoy-stats/
│   ├── envoy-accesslog/   envoy-runtime/
│   └── ... (new crates added per phase)
├── tests/
│   ├── differential/                   # real-Envoy-vs-envoy-rust harness (testcontainers)
│   ├── conformance/                    # h2spec, h3spec, grpc-conformance drivers
│   ├── fixtures/                       # paired configs (envoy.yaml ↔ envoy-rust.yaml)
│   └── helpers/
└── docs/envoy-rust/
    ├── MISSION.md
    ├── ROADMAP.md
    ├── STATE.md
    ├── DECISIONS.md
    ├── ENVOY_TARGET.md
    ├── BEHAVIOR_CONTRACT.md
    ├── SKILL_ROUTING.md
    └── phases/
        ├── 00-bootstrap/
        │   ├── SPEC.md  PLAN.md  PROGRESS.md  REVIEW.md
        ├── 01-static-bootstrap-config/
        ├── …
        └── 99-archive/
```

### 3.1 Crate-naming convention

All workspace crates are kebab-case and prefixed `envoy-`. One subsystem per crate. A single phase typically adds one crate (or extends one existing crate).

### 3.2 Integration tests as workspace members

`tests/differential/`, `tests/conformance/`, and `tests/helpers/` are themselves workspace crates (they contain a `Cargo.toml`) so they can pull workspace crates as path dependencies. `tests/fixtures/` is data only — no crate manifest.

## 4. Doctrine D-3.2 (Rust) — Permitted / From-Scratch / Forbidden

### 4.1 Permitted foundations

| Category | Crate(s) | Scope notes |
|---|---|---|
| Async runtime | `tokio`, `tokio-util` | No std-lib async alternative exists; tokio is mandatory for `rustls`, `quinn`, `h2`. |
| Buffers | `bytes` | De facto standard for zero-copy buffer management. |
| HTTP/2 codec | `h2` | Codec only — never as a server runtime. Analogue of `golang.org/x/net/http2`. |
| HTTP/1 parser | `httparse` | Tokenizer only. |
| QUIC transport | `quinn` | Direct analogue of `quic-go`. |
| TLS | `rustls`, `webpki`, `rustls-pki-types`, `aws-lc-rs` | `aws-lc-rs` permitted as crypto provider. |
| Protobuf | `prost`, `prost-types`, `prost-build` | Message types and codegen. |
| gRPC | `tonic`, `tonic-build` | **Transport only.** All xDS state-machine logic — version/nonce tracking, ACK/NACK, ADS multiplexing, initial-fetch timeout, reconnection — is written from scratch. Relaxation of the Go prompt's "proto types only" rule, justified by the absence of a standalone Rust gRPC client. |
| Config | `serde`, `serde_yaml`, `serde_json` | |
| Logging | `tracing`, `tracing-subscriber` | |
| OpenTelemetry | `opentelemetry`, `opentelemetry_sdk`, `tracing-opentelemetry` | |
| Errors | `thiserror` (libraries), `anyhow` (binary crate only) | Libraries expose typed errors. |
| Test harness | `testcontainers` | Differential harness container orchestration. |

### 4.2 Must be written from scratch

Verbatim from the Go prompt (language-independent):

- Filter chain engine (network + HTTP filter iteration protocol)
- Listener manager, cluster manager
- xDS state machine (ADS/delta, ACK/NACK, version/nonce tracking)
- All load balancing algorithms
- Active health checking, outlier detection, circuit breakers
- Access log formatters and sinks
- Stats subsystem
- Admin API
- Runtime layer (RTDS consumer)
- Hot-restart / graceful-drain semantics
- Every individual filter (network and HTTP)

### 4.3 Forbidden, without exception

- `hyper` as a **direct** dependency, `hyper-util` — you may pull in `h2` from the hyper project for its codec. `hyper` may appear as a transitive dependency of `tonic`; you may not import or call `hyper` yourself.
- `axum`, `actix-web`, `warp`, `rocket`, `poem` — HTTP frameworks
- `pingora`, `sozu` — existing Rust proxy cores (the Traefik/Caddy analogue)
- `tower`, `tower-service`, `tower-http` as **direct** dependencies for filter-chain composition; permitted only transitively through `tonic`
- GPL-licensed code
- Vendoring or FFI-binding of Envoy C++ or BoringSSL from the Envoy codebase

## 5. New Doctrine Rules (Rust-only)

### 5.1 D-3.8 — `unsafe` is forbidden by default

Every workspace crate's root (`lib.rs` or `main.rs`) begins with `#![forbid(unsafe_code)]`. Opt-out is per-crate only and requires a landed ADR that names the specific need (e.g., perf-critical zero-copy slicing in a codec) and the exact module boundary inside which `unsafe` is permitted. Ad-hoc `unsafe` blocks are forbidden even inside opt-out crates: they must sit in the ADR-named module.

### 5.2 D-3.9 — Toolchain pin

`rust-toolchain.toml` at the repo root pins the compiler version (channel + version). All phases build against the pinned toolchain. Upgrading the pin is its own phase, with its own differential re-baselining and its own ADR — you may not bump the toolchain ad-hoc. This parallels D-3.7's `ENVOY_TARGET.md` discipline for upstream Envoy.

## 6. Verification Commands (§5 step 4 and §7.5)

The state machine's step 4 and the six-part phase-done gate retranslate to:

```
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
<differential suite for this phase's feature surface>
<conformance suites for this phase, at declared threshold>
<cargo fuzz run <target> for short budget, for any new fuzz target>
```

All command outputs are quoted into `PROGRESS.md` per `superpowers:verification-before-completion` discipline.

## 7. Fuzzing (§7.4)

Rust fuzzers use `cargo fuzz` (libFuzzer). Each crate that introduces a parser, codec, or filter ships a `fuzz/` subdirectory with one or more fuzz targets. CI runs short-budget fuzzes; nightly runs long-budget. Equivalence discipline for adversarial inputs is identical to the Go prompt: same class of response as upstream Envoy (matching status code and `x-envoy-local-reply` behavior); identical error text is not required.

## 8. §10 First-Session Bootstrap — Rust-specific Deltas

Step 2 of §10 creates, in addition to the language-neutral `docs/envoy-rust/` skeleton:

- `Cargo.toml` (workspace root, `[workspace]` with empty `members = []` — members are added when each phase first introduces a crate)
- `rust-toolchain.toml` (channel = stable, version pinned to the latest stable at bootstrap time)
- `deny.toml` (license allow-list excluding GPL; advisories enabled)

Step 3's scaffold commit message becomes:

```
bootstrap: envoy-rust project scaffold
```

Step 4 is unchanged: create `docs/envoy-rust/phases/00-bootstrap/` (empty), invoke `superpowers:brainstorming` scoped to phase 00.

## 9. MVP Trunk (§8) and Feature Families (§9)

Both sections copy over with **no content changes to phase titles, ordering, or feature family headings**. Phase 00's one-line description mentions Cargo workspace + `rust-toolchain.toml` + `deny.toml` instead of `go.mod`. All other rows are verbatim.

## 10. Differential Harness (§7.1) — Rust-specific Deltas

- The harness lives at `tests/differential/` as a Cargo workspace crate.
- Subject proxy: `cargo run -p envoy-bin --release -- -c <fixture>/envoy-rust.yaml` (release mode to match Envoy's optimization level).
- Reference proxy: upstream Envoy Docker image via `testcontainers` (Rust), tag pinned in `ENVOY_TARGET.md`.
- Fixture structure (`envoy.yaml`, `envoy-rust.yaml`, `inputs/`, `expectations.yaml`) is unchanged.
- The equivalence matrix in §7.2 is copied verbatim — it is language-independent.

## 11. Section-by-Section Mapping (Go → Rust)

| Go §  | Rust § | Changes                                                                 |
|-------|--------|-------------------------------------------------------------------------|
| §1    | §1     | Docs-path probe `docs/envoy-go/` → `docs/envoy-rust/` throughout.       |
| §2    | §2     | "Go" → "Rust"; ENVOY_TARGET.md path.                                    |
| §3.1  | §3.1   | Identical.                                                              |
| §3.2  | §3.2   | Full rewrite per §4 of this spec.                                       |
| §3.3  | §3.3   | Identical.                                                              |
| §3.4  | §3.4   | Identical (path update).                                                |
| §3.5  | §3.5   | Identical (path update).                                                |
| §3.6  | §3.6   | Identical.                                                              |
| §3.7  | §3.7   | Identical (path update).                                                |
| —     | §3.8   | **New:** `unsafe` forbidden by default.                                 |
| —     | §3.9   | **New:** `rust-toolchain.toml` pin discipline.                          |
| §4    | §4     | Tree replaced per §3 of this spec; new invariant on `forbid(unsafe_code)`. |
| §5    | §5     | Step 4 commands replaced per §6 of this spec.                           |
| §6    | §6     | Identical (same ~25-task / ~1500-LoC thresholds).                       |
| §7.1  | §7.1   | Rust harness per §10 of this spec.                                      |
| §7.2  | §7.2   | Identical (equivalence matrix is language-neutral).                     |
| §7.3  | §7.3   | Identical.                                                              |
| §7.4  | §7.4   | `cargo fuzz` replaces `go fuzz`; discipline identical.                  |
| §7.5  | §7.5   | Six-gate list unchanged; commands translated per §6 of this spec.       |
| §8    | §8     | Phase titles verbatim; phase 00 description mentions Cargo workspace.   |
| §9    | §9     | Verbatim (including `[scope TBD]` on zookeeper).                        |
| §10   | §10    | Scaffold additions per §8 of this spec; commit message updated.         |
| §11   | §11    | State machine verbatim; verification commands replaced.                 |
| §12   | §12    | Adds acceptance checks for D-3.8 and D-3.9 enforcement verbs.           |

## 12. Acceptance Self-Check Additions

The `BOOTSTRAP_PROMPT.md`'s own §12 (metadata for prompt authors) gains two new acceptance checks, in addition to the ones carried over from the Go prompt:

- D-3.8 (`unsafe` policy) appears with explicit enforcement verbs (`must`, `never`, `forbidden`).
- D-3.9 (toolchain pin) appears with explicit enforcement verbs and references `rust-toolchain.toml` by name.
- The permitted-foundations table in D-3.2 names every crate in §4.1 of this spec exactly.

## 13. Out of Scope for This Spec

- Writing the envoy-rust project itself. This spec produces only the `BOOTSTRAP_PROMPT.md` file. The prompt, when later executed, produces the project.
- Choosing the pinned upstream Envoy version; that is deferred to phase 00.
- Choosing the pinned Rust toolchain version; that is deferred to the first-session bootstrap (§10 Step 2).
- Any `/gsd-*` workflow integration. The prompt explicitly forbids `/gsd-*` (carried over from the Go prompt).

## 14. Deliverable

A single file at `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md`, self-contained, roughly the same size as the Go prompt (~520 lines). When loaded as the first user message into a fresh Claude Code session with `superpowers` active, it produces the §10 bootstrap scaffold and exits cleanly, and subsequent sessions resume from disk state per the phase lifecycle state machine.
