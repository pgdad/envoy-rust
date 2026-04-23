# envoy-rust Bootstrap Prompt Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a single self-contained Markdown file at `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md` that mirrors the structure of `/Users/esa/git/envoy-go/BOOTSTRAP_PROMPT.md` but drives a Rust reimplementation of the Envoy Proxy.

**Architecture:** The Go prompt is the canonical source. This plan produces the Rust counterpart by (1) copying language-neutral sections verbatim with `envoy-go` → `envoy-rust` string substitutions, (2) rewriting six specific locations with Rust-specific content, and (3) adding two new doctrine rules (D-3.8 `unsafe` policy, D-3.9 toolchain pin). The full delta is enumerated in the spec at `docs/superpowers/specs/2026-04-23-envoy-rust-bootstrap-prompt-design.md`.

**Tech Stack:** Markdown. Reference files:
- Source: `/Users/esa/git/envoy-go/BOOTSTRAP_PROMPT.md` (522 lines)
- Spec: `/Users/esa/git/envoy-rust/docs/superpowers/specs/2026-04-23-envoy-rust-bootstrap-prompt-design.md`
- Output: `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md`

**Working directory:** `/Users/esa/git/envoy-rust` (current repo).

**Commit convention:** one commit per completed task, message format `bootstrap-prompt: add §N …`.

---

## File Structure

Single file, no directories or imports:

- Create: `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md` (target size ~520–560 lines)

All tasks append to this file in order. Intermediate states are committed; the final file's §12 acceptance self-checks all pass before the final commit.

---

## Universal Substitution Rules

Every time a Go-prompt line is carried over, apply these string substitutions. Do NOT rewrite the underlying prose — only substitute.

1. `envoy-go` → `envoy-rust` (every occurrence, including path segments and identifier names)
2. `docs/envoy-go/` → `docs/envoy-rust/`
3. The phrase "in Go" → "in Rust"
4. The phrase "Go standard library" → "Rust standard library"
5. `go.mod / go.sum` → `Cargo.toml / Cargo.lock`
6. `cmd/envoy-go/` → `crates/envoy-bin/`
7. `internal/` → `crates/` (where it refers to the repo layout)
8. `go build, go vet, golangci-lint, go test ./...` (and any ordering variant of these four) → `cargo build --workspace --all-targets, cargo clippy --workspace --all-targets --all-features -- -D warnings, cargo fmt --all -- --check, cargo test --workspace, cargo deny check`
9. `go fuzz` / "Go fuzz target" → `cargo fuzz` / "`cargo fuzz` target"
10. `github.com/testcontainers/testcontainers-go` → `testcontainers` (the Rust crate)
11. `test/` (when referring to the repo-level integration test tree) → `tests/`

These substitutions are applied by every task that carries Go-prompt content forward.

---

## Task 1: Scaffold file + §1 Cold-Start + §2 Mission

**Files:**
- Create: `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md`

- [ ] **Step 1: Verify reference files exist**

Run:
```bash
test -f /Users/esa/git/envoy-go/BOOTSTRAP_PROMPT.md && \
test -f /Users/esa/git/envoy-rust/docs/superpowers/specs/2026-04-23-envoy-rust-bootstrap-prompt-design.md && \
echo OK
```
Expected: `OK`.

- [ ] **Step 2: Create the file with the preamble and TOC**

Write to `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md`:

```markdown
# envoy-rust Bootstrap Prompt

You are operating inside a fresh Claude Code session with the `superpowers` plugin active. You have just been handed this prompt as your first user message. It is the only instruction you will receive. There is no prior conversation history. There is no human available to clarify mid-task.

This prompt drives an indefinite-duration, phase-based project to reimplement the Envoy Proxy in Rust, with every phase's output verified against upstream Envoy via a differential test harness. Your job is to execute exactly one unit of progress — usually one phase, sometimes one sub-phase — advance the on-disk state, and exit cleanly. The next fresh session will read the same on-disk state and continue.

Read this entire prompt once before taking any action.

## Table of Contents

1. Cold-Start Procedure — do this first, every time
2. Mission and Non-Purposes
3. Operating Doctrine
4. On-Disk Artifact Layout
5. Phase Lifecycle State Machine
6. Phase Splitting Policy
7. Differential Test Contract
8. Seeded MVP Trunk (phases 00–08)
9. Feature Families (09+)
10. First-Session Bootstrap (runs only once, on an empty repo)
11. Skill Routing Appendix
12. Acceptance Self-Checks

---
```

- [ ] **Step 3: Append §1 Cold-Start**

Copy the Go prompt's §1 (its lines 26–65) verbatim with **Universal Substitutions** applied. The freshness-probe command becomes:

```bash
test -d docs/envoy-rust && echo EXISTS || echo FRESH
```

Every `docs/envoy-go/` path changes to `docs/envoy-rust/`. Step B's file list (MISSION.md, STATE.md, ROADMAP.md, DECISIONS.md, BEHAVIOR_CONTRACT.md, SKILL_ROUTING.md) is unchanged except for the enclosing path. Step C's file list (SPEC.md, PLAN.md, PROGRESS.md, REVIEW.md) is unchanged. All prose is carried over literally apart from the substitutions.

- [ ] **Step 4: Append §2 Mission and Non-Purposes**

Copy the Go prompt's §2 (its lines 68–84) verbatim with substitutions. §2.1's URL (`https://www.envoyproxy.io/`) is unchanged. `docs/envoy-go/ENVOY_TARGET.md` → `docs/envoy-rust/ENVOY_TARGET.md`. "in Go" → "in Rust". §2.2's bullets are copied verbatim with `docs/envoy-go/DECISIONS.md` → `docs/envoy-rust/DECISIONS.md`. Do NOT change "You are **not** reproducing Envoy's C++ source structure" — it is language-neutral.

- [ ] **Step 5: Verify headers present**

Run:
```bash
grep -c '^## ' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md
grep -q 'Reimplement the Envoy Proxy (https://www.envoyproxy.io/) in Rust' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "§2 mission OK"
grep -q 'test -d docs/envoy-rust && echo EXISTS || echo FRESH' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "§1 probe OK"
```
Expected: count ≥ 2 (one for §1, one for §2), and both OK lines print.

- [ ] **Step 6: Commit**

```bash
git add BOOTSTRAP_PROMPT.md
git commit -m "bootstrap-prompt: add preamble, §1 cold-start, §2 mission"
```

---

## Task 2: §3 Operating Doctrine (all 9 rules)

**Files:**
- Modify: `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md` (append §3)

- [ ] **Step 1: Append §3 header and intro**

Append:

```markdown
## 3. Operating Doctrine — hard constraints

These rules are non-negotiable. They are named by number so that ADRs and review comments can refer to them as `doctrine D-3.2`, etc.
```

- [ ] **Step 2: Append D-3.1 (identical to Go)**

Copy the Go prompt's D-3.1 (its lines 92–102) verbatim with substitutions. The skill-routing table rows are identical. The closing sentence `` `/gsd-*` commands are forbidden. If you find yourself reaching for one, re-read §1. `` is verbatim.

- [ ] **Step 3: Append D-3.2 (rewritten for Rust)**

Do NOT copy the Go D-3.2. Write this verbatim instead:

````markdown
### D-3.2 Hybrid implementation stance

**Permitted foundations:**
- Rust standard library.
- `tokio`, `tokio-util` — async runtime. Mandatory; no std-lib async alternative exists, and downstream foundations (`rustls`, `quinn`, `h2`, `tonic`) require it.
- `bytes` — zero-copy buffer management.
- `h2` — HTTP/2 codec (from the hyper project), used as a *low-level codec only*. Never as a server runtime. Direct analogue of Go's `golang.org/x/net/http2`.
- `httparse` — HTTP/1.1 tokenizer, used as a parser only.
- `quinn` — QUIC transport. Direct analogue of `quic-go`.
- `rustls`, `webpki`, `rustls-pki-types` — TLS stack. `aws-lc-rs` permitted as the crypto provider.
- `prost`, `prost-types`, `prost-build` — protobuf runtime and codegen.
- `tonic`, `tonic-build` — gRPC codec and transport. **Transport only.** All xDS state-machine logic — version/nonce tracking, ACK/NACK, ADS multiplexing, initial-fetch timeout, reconnection — is written from scratch. This is the `go-control-plane` "proto types only" analogue, relaxed slightly for gRPC transport because Rust has no standalone gRPC client crate.
- `serde`, `serde_yaml`, `serde_json` — config parsing.
- `tracing`, `tracing-subscriber` — structured logging.
- `opentelemetry`, `opentelemetry_sdk`, `tracing-opentelemetry` — OpenTelemetry SDK integration.
- `thiserror` — typed errors in library crates. `anyhow` permitted only in the binary crate (`envoy-bin`).
- `testcontainers` — differential harness container orchestration.

**Must be written from scratch** (one or more dedicated phases each):
- Filter chain engine (network + HTTP filter iteration protocol).
- Listener manager, cluster manager.
- xDS state machine (ADS/delta, ACK/NACK, version/nonce tracking).
- All load balancing algorithms.
- Active health checking, outlier detection, circuit breakers.
- Access log formatters and sinks.
- Stats subsystem.
- Admin API.
- Runtime layer (RTDS consumer).
- Hot-restart / graceful-drain semantics.
- Every individual filter (network and HTTP).

**Forbidden, without exception:**
- `hyper` as a **direct** dependency; `hyper-util`. You may pull in `h2` from the hyper project for its codec. `hyper` may appear only as a transitive dependency of `tonic`; you must not import or call `hyper` yourself.
- `axum`, `actix-web`, `warp`, `rocket`, `poem` — HTTP frameworks.
- `pingora`, `sozu` — existing Rust proxy cores (the Traefik/Caddy analogue).
- `tower`, `tower-service`, `tower-http` as **direct** dependencies for filter-chain composition; permitted only transitively through `tonic`.
- GPL-licensed code.
- Vendoring or FFI-binding of Envoy C++ or BoringSSL from the Envoy codebase.
````

- [ ] **Step 4: Append D-3.3 through D-3.7**

Copy the Go prompt's D-3.3, D-3.4, D-3.5, D-3.6, D-3.7 (its lines 135–153) verbatim with substitutions. Note specifically:

- D-3.3 references `docs/envoy-go/BEHAVIOR_CONTRACT.md` → `docs/envoy-rust/BEHAVIOR_CONTRACT.md`.
- D-3.4 references `docs/envoy-go/` → `docs/envoy-rust/`.
- D-3.5 references `docs/envoy-go/DECISIONS.md` → `docs/envoy-rust/DECISIONS.md`.
- D-3.7 references `docs/envoy-go/ENVOY_TARGET.md` → `docs/envoy-rust/ENVOY_TARGET.md`.
- Body text otherwise unchanged.

- [ ] **Step 5: Append D-3.8 (new)**

Append verbatim:

```markdown
### D-3.8 `unsafe` is forbidden by default (Rust-only)

Every workspace crate's root file (`lib.rs` or `main.rs`) must begin with `#![forbid(unsafe_code)]`. Opt-out is per-crate only and requires a landed ADR that names the specific need (for example, perf-critical zero-copy slicing in a codec) and the exact module boundary inside which `unsafe` is permitted. Ad-hoc `unsafe` blocks are forbidden even inside opt-out crates — they must sit inside the ADR-named module. Never grant a global crate exemption.
```

- [ ] **Step 6: Append D-3.9 (new)**

Append verbatim:

```markdown
### D-3.9 Toolchain pin (Rust-only)

`rust-toolchain.toml` at the repo root pins the compiler channel and version. All phases build against the pinned toolchain. Upgrading the pin is its own phase, with its own ADR and its own differential re-baselining — you must not bump the toolchain ad-hoc. This parallels D-3.7's `ENVOY_TARGET.md` discipline for upstream Envoy.
```

- [ ] **Step 7: Verify doctrine invariants**

Run:
```bash
grep -c '^### D-3\.' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md
grep -q '#!\[forbid(unsafe_code)\]' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "D-3.8 forbid-verb OK"
grep -q 'rust-toolchain.toml' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "D-3.9 pin OK"
grep -q '`tonic`, `tonic-build`' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "tonic permitted OK"
grep -q 'pingora' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "pingora forbidden OK"
```
Expected: count = 9 (D-3.1 through D-3.9), and all four OK lines print.

- [ ] **Step 8: Commit**

```bash
git add BOOTSTRAP_PROMPT.md
git commit -m "bootstrap-prompt: add §3 operating doctrine (D-3.1–D-3.9)"
```

---

## Task 3: §4 On-Disk Artifact Layout

**Files:**
- Modify: `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md` (append §4)

- [ ] **Step 1: Append §4 intro**

Append:

```markdown
## 4. On-Disk Artifact Layout

This is the only layout the project uses. Phase 00 creates it. Every subsequent phase adheres to it.
```

- [ ] **Step 2: Append the tree**

Append verbatim (inside a single fenced block):

```
envoy-rust/
├── README.md                        # short: what this is, how to resume a session
├── Cargo.toml                       # workspace root (members list grows per phase)
├── Cargo.lock
├── rust-toolchain.toml              # pinned compiler (D-3.9)
├── deny.toml                        # cargo-deny policy (license + advisory enforcement)
├── crates/
│   ├── envoy-bin/                   # the binary (src/main.rs)
│   ├── envoy-protos/                # generated from upstream envoy .proto via prost-build + tonic-build
│   ├── envoy-listener/
│   ├── envoy-cluster/
│   ├── envoy-filter/                # filter-chain engine (written from scratch)
│   ├── envoy-http/   envoy-tcp/   envoy-tls/
│   ├── envoy-xds/    envoy-admin/   envoy-stats/   envoy-accesslog/   envoy-runtime/   …
├── tests/
│   ├── differential/                # real-Envoy-vs-envoy-rust harness (testcontainers)
│   ├── conformance/                 # h2spec, h3spec, grpc-conformance drivers
│   ├── fixtures/                    # paired configs (envoy.yaml ↔ envoy-rust.yaml)
│   └── helpers/
└── docs/envoy-rust/
    ├── MISSION.md                   # copy of the prompt's mission + doctrine (stable)
    ├── ROADMAP.md                   # phase list with status column; append-only history
    ├── STATE.md                     # pointer: active phase, last commit, next action
    ├── DECISIONS.md                 # ADR log (numbered, append-only)
    ├── ENVOY_TARGET.md              # pinned upstream version + how to refresh the image
    ├── BEHAVIOR_CONTRACT.md         # what "behaviorally equivalent" means, per layer
    ├── SKILL_ROUTING.md             # which superpowers skill runs at which phase boundary
    └── phases/
        ├── 00-bootstrap/
        │   ├── SPEC.md              # brainstorming output
        │   ├── PLAN.md              # writing-plans output
        │   ├── PROGRESS.md          # running log, updated by executor
        │   └── REVIEW.md            # requesting-code-review output
        ├── 01-static-bootstrap-config/
        ├── 02-tcp-proxy/
        ├── …
        └── 99-archive/              # completed phases' artifacts can be moved here if docs/ grows
```

- [ ] **Step 3: Append §4.1 Invariants (1–7 carried over, plus new #8)**

Append the Go prompt's §4.1 invariants 1–7 (its lines 196–206) verbatim with substitutions. Then append invariant #8 (new):

```markdown
8. **Every workspace crate's root file** (`lib.rs` or `main.rs`) begins with `#![forbid(unsafe_code)]` unless an ADR grants an exemption. See D-3.8.
```

Append a trailing `---` separator.

- [ ] **Step 4: Verify layout invariants**

Run:
```bash
grep -q 'Cargo.toml                       # workspace root' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "tree OK"
grep -q 'crates/envoy-bin/' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "envoy-bin OK"
grep -q 'envoy-protos/' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "envoy-protos OK"
grep -c '^[0-9]\. \*\*' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md
```
Expected: three OK lines, and invariants count ≥ 8.

- [ ] **Step 5: Commit**

```bash
git add BOOTSTRAP_PROMPT.md
git commit -m "bootstrap-prompt: add §4 on-disk artifact layout"
```

---

## Task 4: §5 Phase Lifecycle State Machine

**Files:**
- Modify: `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md` (append §5)

- [ ] **Step 1: Append §5 intro and the state-machine fence**

Copy the Go prompt's §5 intro (its lines 210–212) verbatim. Then copy the fenced state-machine block (its lines 213–254) verbatim **except**: replace step 4's command line. The Go step 4 reads:

```
   → run: go build, go vet, golangci-lint, go test ./...,
          differential suite for phase's feature surface, conformance suites
```

Replace with:

```
   → run: cargo build --workspace --all-targets,
          cargo clippy --workspace --all-targets --all-features -- -D warnings,
          cargo fmt --all -- --check,
          cargo test --workspace,
          cargo deny check,
          differential suite for phase's feature surface, conformance suites
```

All other lines inside the fence are verbatim.

- [ ] **Step 2: Append §5.1, §5.2, §5.3**

Copy the Go prompt's §5.1 (state-machine reading notes), §5.2 (review-feedback re-entry), §5.3 (commit message format) verbatim with substitutions. §5.3's `phase NN: <title> [ADR-NNNN, ADR-MMMM, ...]` commit format is unchanged — it is language-neutral.

Append a trailing `---`.

- [ ] **Step 3: Verify state machine integrity**

Run:
```bash
grep -q 'cargo build --workspace --all-targets' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "cargo build OK"
grep -q 'cargo clippy --workspace --all-targets --all-features -- -D warnings' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "cargo clippy OK"
grep -q 'cargo deny check' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "cargo deny OK"
grep -cE '^[0-6]\. ' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md
! grep -q 'go vet' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "no Go commands OK"
```
Expected: three OK lines, state-machine step count ≥ 7 (0–6), "no Go commands OK".

- [ ] **Step 4: Commit**

```bash
git add BOOTSTRAP_PROMPT.md
git commit -m "bootstrap-prompt: add §5 phase lifecycle state machine"
```

---

## Task 5: §6 Splitting Policy + §7 Differential Test Contract

**Files:**
- Modify: `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md` (append §6 and §7)

- [ ] **Step 1: Append §6 verbatim**

Copy the Go prompt's §6 (its lines 283–306) verbatim with substitutions. Thresholds (~25 tasks, ~1500 LoC) are unchanged. The sub-phase directory path substitution applies (`docs/envoy-go/phases/` → `docs/envoy-rust/phases/`). Append a trailing `---`.

- [ ] **Step 2: Append §7.1 Harness architecture**

Copy the Go prompt's §7.1 (its lines 312–326) verbatim **except** the subject-proxy description sentence. The Go line reads:

> **Subject:** envoy-go built from the current tree, run as a subprocess.

Replace with:

> **Subject:** envoy-rust built from the current tree (`cargo run -p envoy-bin --release -- -c <fixture>/envoy-rust.yaml`), run as a subprocess.

Also, the top sentence "`test/differential/` hosts a Go test binary" becomes "`tests/differential/` hosts a Rust test crate". And "managed via `testcontainers-go`" becomes "managed via `testcontainers` (Rust)". The fixture-file list (`envoy.yaml`, `envoy-go.yaml`, `inputs/`, `expectations.yaml`) becomes (`envoy.yaml`, `envoy-rust.yaml`, `inputs/`, `expectations.yaml`).

- [ ] **Step 3: Append §7.2 Equivalence matrix verbatim**

Copy the Go prompt's §7.2 (its lines 328–342) **verbatim** with only substitution #2 applied (`docs/envoy-go/` → `docs/envoy-rust/`). The equivalence matrix table is entirely language-neutral.

- [ ] **Step 4: Append §7.3 Conformance suites verbatim**

Copy the Go prompt's §7.3 (its lines 344–351) verbatim with substitution rule #11 applied (`test/conformance/` → `tests/conformance/`).

- [ ] **Step 5: Append §7.4 Fuzzing (with fuzzer substitution)**

Copy the Go prompt's §7.4 (its lines 353–355) **except** replace "ships a Go fuzz target under `test/`" with "ships a `cargo fuzz` target under the relevant crate's `fuzz/` subdirectory". All other prose (short-budget CI, long-budget nightly, class-of-response discipline) is verbatim.

- [ ] **Step 6: Append §7.5 Phase-done gate (commands updated)**

Copy the Go prompt's §7.5 (its lines 357–368) verbatim **except** bullet (e). The Go (e) reads:

> (e) `go vet`, `golangci-lint run`, `go test ./...` are all clean,

Replace with:

> (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean,

Bullets (a)–(d) and (f) are verbatim. Append a trailing `---`.

- [ ] **Step 7: Verify differential section integrity**

Run:
```bash
grep -q 'cargo run -p envoy-bin --release' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "subject cmd OK"
grep -q 'envoy-rust.yaml' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "fixture rename OK"
grep -q 'cargo fuzz` target' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "fuzz rename OK"
grep -q 'proxy-wasm ABI conformance' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "proxy-wasm OK"
! grep -q 'envoy-go built from the current tree' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "no Go subject reference OK"
```
Expected: five OK lines.

- [ ] **Step 8: Commit**

```bash
git add BOOTSTRAP_PROMPT.md
git commit -m "bootstrap-prompt: add §6 splitting policy and §7 differential contract"
```

---

## Task 6: §8 MVP Trunk + §9 Feature Families

**Files:**
- Modify: `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md` (append §8 and §9)

- [ ] **Step 1: Append §8 intro and MVP table**

Copy the Go prompt's §8 intro paragraph (its lines 371–373) verbatim with substitutions (`docs/envoy-go/ROADMAP.md` → `docs/envoy-rust/ROADMAP.md`). Then copy the phase-00-through-08 table (its lines 375–386) verbatim **except** phase 00's title. The Go row 00 reads:

> | 00 | Bootstrap: repo layout, CI, Docker reference Envoy, differential harness skeleton, `ENVOY_TARGET.md` pin, trivial echo fixture | harness boots; one TCP echo fixture green |

Replace with:

> | 00 | Bootstrap: Cargo workspace layout, `rust-toolchain.toml`, `deny.toml`, CI, Docker reference Envoy, differential harness skeleton, `ENVOY_TARGET.md` pin, trivial echo fixture | harness boots; one TCP echo fixture green |

Rows 01–08 are verbatim.

- [ ] **Step 2: Append §8 closing paragraphs**

Copy the Go prompt's §8 closing paragraphs (its lines 387–389) verbatim with substitution (`envoy-go is a minimal but real proxy` → `envoy-rust is a minimal but real proxy`). Append a trailing `---`.

- [ ] **Step 3: Append §9 Feature Families verbatim**

Copy the Go prompt's §9 (its lines 391–407) verbatim with substitutions. Critical invariants for this section:

- The bracket `[scope TBD]` on zookeeper MUST remain (`zookeeper [scope TBD]`) — it is an explicit open question.
- All family bullets are copied unchanged.
- `docs/envoy-go/ROADMAP.md` → `docs/envoy-rust/ROADMAP.md`.

Append a trailing `---`.

- [ ] **Step 4: Verify trunk and families**

Run:
```bash
grep -q '00 | Bootstrap: Cargo workspace layout' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "phase 00 title OK"
grep -q 'rust-toolchain.toml' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "toolchain file OK"
grep -q 'zookeeper \[scope TBD\]' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "zookeeper bracket OK"
grep -cE '^\| 0[0-8] \|' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md
```
Expected: three OK lines, MVP-row count = 9 (phases 00–08).

- [ ] **Step 5: Commit**

```bash
git add BOOTSTRAP_PROMPT.md
git commit -m "bootstrap-prompt: add §8 MVP trunk and §9 feature families"
```

---

## Task 7: §10 First-Session Bootstrap

**Files:**
- Modify: `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md` (append §10)

- [ ] **Step 1: Append §10 intro and Step 1**

Copy the Go prompt's §10 intro (its lines 411–413) verbatim with substitution. Then copy Step 1 (its lines 415–422) verbatim **except** the sanity-check command line:

Replace:
```bash
test ! -d docs/envoy-go || { echo "NOT FRESH — stop"; exit 1; }
```
with:
```bash
test ! -d docs/envoy-rust || { echo "NOT FRESH — stop"; exit 1; }
```

- [ ] **Step 2: Append §10 Step 2 (scaffold creation, Rust additions inserted)**

Copy the Go prompt's Step 2 intro and items 1–7 (its lines 424–437) verbatim with substitutions, then append three new items (items 8, 9, 10) for the Rust scaffold additions. The full §10 Step 2 list becomes:

```markdown
### Step 2: Create the `docs/envoy-rust/` skeleton and workspace shell

Create, in this order:

1. `docs/envoy-rust/MISSION.md` — copy §§2 and 3 of this prompt verbatim (mission + doctrine). This makes the mission durable independently of this prompt file.
2. `docs/envoy-rust/ROADMAP.md` — create with:
   - A header explaining the schema (`id | title | depends-on | status | sub-phases | summary`).
   - Rows for phases 00 through 08, copied from §8 of this prompt, all with `status = planned`.
   - Family headings 09+ copied from §9, without rows under them yet.
3. `docs/envoy-rust/STATE.md` — points at phase 00, with `next-skill = superpowers:brainstorming`, and an explicit "last-updated" timestamp.
4. `docs/envoy-rust/DECISIONS.md` — seeded with `ADR-0001: bootstrap prompt version X committed at <git SHA>`. The SHA is the SHA of the BOOTSTRAP_PROMPT.md commit you are operating under; compute with `git log -1 --format=%H -- BOOTSTRAP_PROMPT.md`.
5. `docs/envoy-rust/ENVOY_TARGET.md` — empty placeholder with a one-line note: "To be filled during phase 00. Must pin an upstream Envoy Docker image by tag and SHA256."
6. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — skeleton populated with the equivalence matrix from §7.2 of this prompt, plus explicit empty subsections (`Header allow-list`, `Stat-name mapping`, `Access log field mapping`, `xDS wire state machine`, `Timing tolerances`) each marked "to be filled per-phase as needed."
7. `docs/envoy-rust/SKILL_ROUTING.md` — verbatim copy of §5's state machine (just the state machine block, not the surrounding prose).
8. `Cargo.toml` — workspace root, with `[workspace]` and `members = []` (empty initially; phases add members as they introduce crates). Include `resolver = "2"`.
9. `rust-toolchain.toml` — `[toolchain] channel = "stable"` pinned to the latest stable version available at bootstrap time, with `components = ["rustfmt", "clippy"]` and `profile = "minimal"`.
10. `deny.toml` — `cargo-deny` config with a license allow-list that explicitly excludes GPL variants, and `[advisories]` enabled.
```

- [ ] **Step 3: Append §10 Step 3 (updated commit message)**

Copy the Go prompt's Step 3 structure (its lines 439–444) but update the commit message. The Rust version becomes:

```markdown
### Step 3: Commit the scaffold as a single commit

```bash
git add docs/envoy-rust/ Cargo.toml rust-toolchain.toml deny.toml
git commit -m "bootstrap: envoy-rust project scaffold"
```
```

- [ ] **Step 4: Append §10 Steps 4 and 5**

Copy the Go prompt's Step 4 (its lines 446–448) verbatim with substitutions (`docs/envoy-go/phases/00-bootstrap/` → `docs/envoy-rust/phases/00-bootstrap/`). Copy Step 5 (its lines 450–452) verbatim with substitutions. Append a trailing `---`.

- [ ] **Step 5: Verify §10 scaffold inventory**

Run:
```bash
grep -q '8\. `Cargo.toml`' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "Cargo.toml item OK"
grep -q '9\. `rust-toolchain.toml`' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "toolchain item OK"
grep -q '10\. `deny.toml`' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "deny item OK"
grep -q 'bootstrap: envoy-rust project scaffold' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "commit msg OK"
! grep -q 'bootstrap: envoy-go project scaffold' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "no Go commit msg OK"
```
Expected: five OK lines.

- [ ] **Step 6: Commit**

```bash
git add BOOTSTRAP_PROMPT.md
git commit -m "bootstrap-prompt: add §10 first-session bootstrap"
```

---

## Task 8: §11 Skill Routing Appendix + §12 Acceptance Self-Checks

**Files:**
- Modify: `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md` (append §11 and §12)

- [ ] **Step 1: Append §11 intro**

Copy the Go prompt's §11 intro (its lines 456–458) verbatim with substitution (`docs/envoy-go/SKILL_ROUTING.md` → `docs/envoy-rust/SKILL_ROUTING.md`).

- [ ] **Step 2: Append §11 state-machine fence**

Copy the Go prompt's fenced state-machine block in §11 (its lines 460–501) verbatim **with the same step-4 replacement applied as in Task 4 Step 1**: replace the Go step-4 commands with the Rust cargo command block. The rest of the fence (steps 0–6 and Deviations section) is identical.

This appendix MUST be byte-identical to §5's fence (modulo surrounding prose). Per Go §5's closing paragraph: "If §5 and §11 ever diverge, §5 wins and §11 must be corrected."

Append a trailing `---`.

- [ ] **Step 3: Append §12 Acceptance Self-Checks**

Copy the Go prompt's §12 (its lines 505–518) verbatim with substitutions. The numbered checks 1–8 are copied. Append these additional checks after #8 (as #9, #10, #11):

```markdown
9. Doctrine rule D-3.8 appears with explicit enforcement verbs (`must`, `never`, `forbidden`) and names `#![forbid(unsafe_code)]`.
10. Doctrine rule D-3.9 appears with explicit enforcement verbs and names `rust-toolchain.toml`.
11. The permitted-foundations list in D-3.2 names every crate (`tokio`, `tokio-util`, `bytes`, `h2`, `httparse`, `quinn`, `rustls`, `webpki`, `rustls-pki-types`, `aws-lc-rs`, `prost`, `prost-types`, `prost-build`, `tonic`, `tonic-build`, `serde`, `serde_yaml`, `serde_json`, `tracing`, `tracing-subscriber`, `opentelemetry`, `opentelemetry_sdk`, `tracing-opentelemetry`, `thiserror`, `anyhow`, `testcontainers`).
```

- [ ] **Step 4: Append the final footer**

Append:

```markdown
---

*End of bootstrap prompt.*
```

- [ ] **Step 5: Verify §11 / §12 integrity**

Run:
```bash
grep -c 'cargo build --workspace --all-targets' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md
grep -q '*End of bootstrap prompt.*' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md && echo "footer OK"
grep -cE '^[0-9]+\. ' /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md
```
Expected: `cargo build` appears **at least twice** (once in §5, once in §11); footer OK prints; numbered-item count covers §12's 11 checks plus §10's 10 scaffold items at minimum.

- [ ] **Step 6: Commit**

```bash
git add BOOTSTRAP_PROMPT.md
git commit -m "bootstrap-prompt: add §11 skill routing appendix and §12 acceptance self-checks"
```

---

## Task 9: Final Acceptance Self-Checks + Verification Commit

**Files:**
- Modify (if issues found): `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md`

- [ ] **Step 1: Run the complete acceptance-check grep battery**

Run this script (outputs PASS/FAIL per check):

```bash
F=/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md

check() { if eval "$2"; then echo "PASS: $1"; else echo "FAIL: $1"; fi; }

# §12 acceptance checks from the Go prompt (carried over):
check "mission in Rust" "grep -q 'in Rust' $F"
check "superpowers-only references" "! grep -qE '/gsd-[a-z-]+' $F || true"
check "D-3.1 enforcement"  "grep -q 'must' $F"
check "D-3.2 permitted list" "grep -q 'tokio-util' $F"
check "D-3.3 differential" "grep -q 'behaviorally-equivalent' $F"
check "D-3.4 context isolation" "grep -q 'zero prior context' $F"
check "D-3.5 ADRs append-only" "grep -q 'append-only' $F"
check "D-3.6 green build" "grep -q 'green build' $F"
check "D-3.7 ENVOY_TARGET pin" "grep -q 'ENVOY_TARGET.md' $F"

# New Rust-specific acceptance checks:
check "D-3.8 forbid(unsafe_code)" "grep -q '#!\\[forbid(unsafe_code)\\]' $F"
check "D-3.8 enforcement verb" "grep -q 'forbidden' $F"
check "D-3.9 rust-toolchain.toml" "grep -q 'rust-toolchain.toml' $F"

# Permitted-foundations crate coverage (D-3.2):
for crate in tokio tokio-util bytes h2 httparse quinn rustls webpki rustls-pki-types aws-lc-rs prost prost-types prost-build tonic tonic-build serde serde_yaml serde_json tracing tracing-subscriber opentelemetry opentelemetry_sdk tracing-opentelemetry thiserror anyhow testcontainers; do
  check "crate: $crate" "grep -q '\`$crate\`' $F"
done

# Forbidden crates mentioned:
for f in hyper axum actix-web warp rocket pingora sozu tower tower-http; do
  check "forbidden: $f" "grep -q '\`$f\`' $F"
done

# State-machine duplicated in §5 and §11:
occurrences=$(grep -c '^0\. Phase not yet in ROADMAP.md' $F)
check "state machine appears twice (§5 and §11)" "[ $occurrences -eq 2 ]"

# Six-part phase-done gate:
for letter in '(a)' '(b)' '(c)' '(d)' '(e)' '(f)'; do
  check "phase-done gate $letter" "grep -q '$letter' $F"
done

# MVP rows 00–08:
for i in 00 01 02 03 04 05 06 07 08; do
  check "MVP row $i" "grep -qE '^\\| $i \\|' $F"
done

# Feature families with [scope TBD]:
check "zookeeper [scope TBD]" "grep -q 'zookeeper \\[scope TBD\\]' $F"

# No leftover 'envoy-go' references:
remaining=$(grep -c 'envoy-go' $F || echo 0)
check "no 'envoy-go' leftovers" "[ $remaining -eq 0 ]"

# No leftover 'go.mod' / 'go vet' / 'go test' references:
check "no go.mod" "! grep -q 'go.mod' $F"
check "no go vet" "! grep -q 'go vet' $F"
check "no go test" "! grep -q 'go test' $F"
check "no testcontainers-go" "! grep -q 'testcontainers-go' $F"

# Line count sanity check (target: 500–620 lines):
lines=$(wc -l < $F)
check "line count 500–620 (got $lines)" "[ $lines -ge 500 ] && [ $lines -le 620 ]"

echo "---"
echo "If any FAIL above, do NOT commit — fix the underlying section's content and re-run."
```

Expected: all `PASS` lines. Any `FAIL` line must be fixed by editing the appropriate section and re-running the script before proceeding.

- [ ] **Step 2: Fix any FAIL findings**

For each `FAIL` from Step 1, edit the specific section that owns the check. Re-run the Step 1 script until all checks `PASS`. If a check is fundamentally wrong (not the prompt), update this plan (but don't regress the prompt against the spec).

- [ ] **Step 3: Inspect line count and commit if shape is right**

Run:
```bash
wc -l /Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md
wc -l /Users/esa/git/envoy-go/BOOTSTRAP_PROMPT.md
```
Expected: Rust file is 500–620 lines (slightly longer than Go's 522 due to D-3.8, D-3.9, new §12 checks, and expanded §10 scaffold list).

- [ ] **Step 4: Final verification of tree consistency with design spec**

Cross-check against the spec's §11 mapping table. Run:
```bash
F=/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md
grep -c '^## ' $F  # top-level sections
grep -c '^### ' $F # subsections
```
Expected: `^## ` count = 12 (§1 through §12); `^### ` count ≥ 15 (D-3.1–D-3.9 plus §4.1, §5.1–§5.3, §6.1–§6.3, §7.1–§7.5, §10 Steps 1–5).

- [ ] **Step 5: Commit the verification result (amend if Step 2 fixes landed)**

If Step 2 produced any fixes, commit them:
```bash
git add BOOTSTRAP_PROMPT.md
git commit -m "bootstrap-prompt: fix acceptance-check findings"
```

If Step 2 had no fixes, skip the commit. In either case, leave the repo in a clean state:
```bash
git status
git log --oneline
```
Expected: clean working tree; commit history shows 8–10 commits (initial spec, §1–§2, §3, §4, §5, §6–§7, §8–§9, §10, §11–§12, optional fixup).

---

## Spec Coverage Self-Review (run BEFORE marking plan complete)

Re-read the spec at `docs/superpowers/specs/2026-04-23-envoy-rust-bootstrap-prompt-design.md` and verify every requirement is covered by a task above:

- Spec §2 (structure identical, substance Rust) → Tasks 1–8 collectively.
- Spec §3 (Cargo workspace layout) → Task 3.
- Spec §4.1 (permitted foundations table) → Task 2 Step 3.
- Spec §4.2 (must-write-from-scratch list) → Task 2 Step 3.
- Spec §4.3 (forbidden list) → Task 2 Step 3.
- Spec §5.1 (D-3.8 `unsafe` policy) → Task 2 Step 5.
- Spec §5.2 (D-3.9 toolchain pin) → Task 2 Step 6.
- Spec §6 (Rust verification commands) → Tasks 4 Step 1, 5 Step 6, 8 Step 2.
- Spec §7 (`cargo fuzz`) → Task 5 Step 5.
- Spec §8 (§10 scaffold additions + commit message) → Task 7 Steps 2–3.
- Spec §9 (MVP trunk + feature families verbatim) → Task 6.
- Spec §10 (differential harness Rust-specific changes) → Task 5 Step 2.
- Spec §11 (section-by-section mapping) → all tasks.
- Spec §12 (acceptance self-check additions) → Task 8 Step 3 + Task 9 Step 1.

If any spec requirement has no covering task, ADD a task (don't just note the gap).
