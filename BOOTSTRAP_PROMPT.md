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

## 1. Cold-Start Procedure — do this FIRST, every time

This is the only section you must read before acting. If any step here contradicts a later section, this section wins.

**Step A — Determine project state.** Run:

```bash
test -d docs/envoy-rust && echo EXISTS || echo FRESH
```

- If output is `FRESH`: you are the first session. Jump to §10 (First-Session Bootstrap). Do not read intermediate sections first; §10 is self-contained.
- If output is `EXISTS`: continue to Step B.

**Step B — Read the persistent state, in this order, in full:**

1. `docs/envoy-rust/MISSION.md`
2. `docs/envoy-rust/STATE.md`
3. `docs/envoy-rust/ROADMAP.md`
4. `docs/envoy-rust/DECISIONS.md`
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md`
6. `docs/envoy-rust/SKILL_ROUTING.md`

If any of those files is missing, treat the repo as corrupted. Invoke `superpowers:systematic-debugging` with the specific missing file as the symptom before any other action. Do not attempt to recreate the file from memory — it must be reconstructed from git history or the human must be notified via a `CORRUPTED.md` file at repo root, and you exit.

**Step C — Read the active phase's artifacts.** `STATE.md` names the active phase directory (e.g. `phases/04-http-1.1/`). Read, in full:

1. `docs/envoy-rust/phases/<active>/SPEC.md` (if present)
2. `docs/envoy-rust/phases/<active>/PLAN.md` (if present)
3. `docs/envoy-rust/phases/<active>/PROGRESS.md` (if present)
4. `docs/envoy-rust/phases/<active>/REVIEW.md` (if present)

**Step D — Match your state against the Phase Lifecycle State Machine (§5) and invoke exactly the skill it indicates.** No other action first. Not a quick peek at the code. Not a `git status`. The skill invocation IS your first action.

**Step E — On unexpected state** (e.g. `STATE.md` says "in-progress" but no phase directory exists, or `PLAN.md` exists but `PROGRESS.md` claims completion without a REVIEW.md): do not improvise. Invoke `superpowers:systematic-debugging` on the specific discrepancy before any other action.

**Never**:
- Never skip Step B to "save time." The project has no conversation memory — disk is the only memory.
- Never take any file-mutating action before Step D.
- Never invent facts "from context" — if it's not on disk, it does not exist for you.

---

## 2. Mission and Non-Purposes

### 2.1 Mission

Reimplement the Envoy Proxy (https://www.envoyproxy.io/) in Rust, feature-complete relative to the upstream version pinned in `docs/envoy-rust/ENVOY_TARGET.md`, such that every implemented surface produces behaviorally-equivalent output to upstream Envoy under the differential test contract defined in §7.

The project is executed as an open-ended sequence of phases, each phase self-contained enough to run in a fresh session with zero prior context. Every phase ends with a green build, green tests, a green differential suite for the feature surface covered so far, and a committed review.

### 2.2 Non-purposes

- You are **not** reproducing Envoy's C++ source structure, naming, or internal ABI.
- You are **not** chasing byte-for-byte wire equivalence where the differential contract (§7) does not require it.
- You are **not** free to skip skills, tests, or reviews under time pressure. Phase splitting (§6) is the only release valve.
- You are **not** authorized to use `/gsd-*` commands. They do not belong to this project.
- You are **not** resolving ambiguities by asking a human mid-phase. Write an ADR in `docs/envoy-rust/DECISIONS.md` and proceed.

---

## 3. Operating Doctrine — hard constraints

These rules are non-negotiable. They are named by number so that ADRs and review comments can refer to them as `doctrine D-3.2`, etc.

### D-3.1 Superpowers-first process

| Situation | Required skill |
|---|---|
| Any design artifact about to be written | `superpowers:brainstorming` |
| Any implementation task about to start | `superpowers:writing-plans` first, then `superpowers:executing-plans` or `superpowers:subagent-driven-development` |
| Any implementation task inside a plan | `superpowers:test-driven-development` — tests first, no exceptions |
| Any claim of "done" about to be made | `superpowers:verification-before-completion` |
| Any phase about to be committed as complete | `superpowers:requesting-code-review` |
| Any unexpected state, test failure, or harness divergence | `superpowers:systematic-debugging` — before you propose a fix |

`/gsd-*` commands are forbidden. If you find yourself reaching for one, re-read §1.

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

### D-3.3 Differential correctness beats internal fidelity

A phase ships when its feature surface produces output behaviorally-equivalent to upstream Envoy on the same config and inputs, as mechanically defined by `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (§7). You must not read Envoy source to decide what "equivalent" means — the contract is the contract.

### D-3.4 Context isolation is the primary design constraint

Every artifact you write — SPEC, PLAN, PROGRESS, REVIEW, ADR — must be readable by a stranger with zero prior context. Never write "as discussed earlier" or "remember to…". If a fact matters across sessions, it must live in `docs/envoy-rust/`. This is non-negotiable; phases that violate it will be unreviewable and must be rewritten.

### D-3.5 Decisions are written, not remembered

When you hit an ambiguity not already settled in `docs/envoy-rust/DECISIONS.md`, append a new ADR (next sequential `ADR-NNNN`), state the options considered, state the choice, state the rationale, and proceed. ADRs are append-only: never edit a landed ADR; supersede it with a new one that explicitly names the superseded ADR number.

### D-3.6 Every phase is a green build

No phase lands with failing unit tests, failing differential fixtures, failing conformance checks, lint errors, or build errors. Your only release valve is phase splitting (§6). Splitting is cheap, expected, and encouraged.

### D-3.7 Version pinning

The reference Envoy version (Docker image tag + SHA) lives in `docs/envoy-rust/ENVOY_TARGET.md`. All fixtures, proto versions, and behavior contracts reference that pin. Upgrading the pin is its own phase, with its own differential re-baselining; you may not change the pin ad-hoc.

### D-3.8 `unsafe` is forbidden by default (Rust-only)

Every workspace crate's root file (`lib.rs` or `main.rs`) must begin with `#![forbid(unsafe_code)]`. Opt-out is per-crate only and requires a landed ADR that names the specific need (for example, perf-critical zero-copy slicing in a codec) and the exact module boundary inside which `unsafe` is permitted. Ad-hoc `unsafe` blocks are forbidden even inside opt-out crates — they must sit inside the ADR-named module. Never grant a global crate exemption.

### D-3.9 Toolchain pin (Rust-only)

`rust-toolchain.toml` at the repo root pins the compiler channel and version. All phases build against the pinned toolchain. Upgrading the pin is its own phase, with its own ADR and its own differential re-baselining — you must not bump the toolchain ad-hoc. This parallels D-3.7's `ENVOY_TARGET.md` discipline for upstream Envoy.

---
