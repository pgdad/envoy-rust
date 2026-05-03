# Phase 05.2 — Progress

Phase 05.2 PROGRESS log. SPEC at `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md` (committed at parent-05 state-2 SHA `f1804a7`); PLAN at `docs/envoy-rust/phases/05.2-http2-downstream/PLAN.md`. Tasks 1–14 land in numeric order; each task carries Commit / Deliverables / ADR landed / Files modified / LoC / Verification / Deviations / Carryforward sections per 05.4 PROGRESS.md precedent.

---

## Task 1 — `crates/envoy-http2/` scaffold + Cargo.lock sync + ADR-0027

- **Commit:** _(pending — fill in via post-hoc `phase 05.2: progress note (task 1)` if SHA needed cross-task)_
- **Deliverables:** New workspace member `crates/envoy-http2/` (Cargo.toml + src/lib.rs); Cargo.lock synced; ADR-0027 appended to DECISIONS.md; this PROGRESS.md created.
- **ADR landed:** ADR-0027 (`http = "1"` direct dep on `crates/envoy-http2/Cargo.toml`; narrow codec-edge translation grant parallel to ADR-0021's regex grant).
- **Files modified:**
  - `crates/envoy-http2/Cargo.toml` (created).
  - `crates/envoy-http2/src/lib.rs` (created).
  - `Cargo.toml` (root — `crates/envoy-http2` inserted alphabetically between `crates/envoy-http1` and `crates/envoy-listener`).
  - `Cargo.lock` (synced; `h2 v0.4.13` + `fnv v1.0.7` added as new top-level deps).
  - `docs/envoy-rust/DECISIONS.md` (ADR-0027 appended after ADR-0025).
  - `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` (this file, created).
- **LoC:** ~80 (23 Cargo.toml + 19 lib.rs + 1 root Cargo.toml + Cargo.lock diff + 30 ADR + ~7 PROGRESS preamble); matches PLAN §SPEC §6 signpost 28 LoC-budget posture (a) — accept the estimate.
- **Verification:**

  Step 1.1 — `cargo search h2 --limit 1`:
  ```
  h2 = "0.4.13"    # An HTTP/2 client and server
  ```
  Version is `0.4.x`; proceeded with `h2 = "0.4"` as specified in PLAN Step 1.1.

  Step 1.5 — `cargo build -p envoy-http2`:
  ```
  Locking 2 packages to latest compatible versions
    Adding fnv v1.0.7
    Adding h2 v0.4.13
  Compiling fnv v1.0.7
  Compiling slab v0.4.12
  Compiling tokio-util v0.7.18
  Compiling envoy-listener v0.0.0
  Compiling envoy-cluster v0.0.0
  Compiling h2 v0.4.13
  Compiling envoy-http1 v0.0.0
  Compiling envoy-http2 v0.0.0
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.80s
  ```

  Step 1.6 — `cargo deny check`:
  ```
  advisories ok, bans ok, licenses ok, sources ok
  ```
  Pre-existing `license-not-encountered` advisory-only warnings (0BSD, BSD-2-Clause, MPL-2.0, Unicode-DFS-2016, Zlib) unchanged from 05.4 baseline. No new license brought in by `h2` or `fnv`; both are MIT/Apache-2.0. No deny.toml changes needed.

  Step 1.9 — workspace-wide verification:
  ```
  $ cargo build --workspace --all-targets 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 10.63s

  $ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.29s

  $ cargo fmt --all -- --check 2>&1
  (empty — clean)
  ```

- **Deviations from PLAN:** None. Steps executed verbatim. `h2 = "0.4.13"` matches the projected `0.4.x` range. `fnv v1.0.7` is a transitive dep of `h2` (MIT/Apache-2.0); no deny.toml changes required. The `0BSD` `license-not-encountered` warning appears for the first time in the local `cargo deny` output (the 05.4 PROGRESS shows BSD-2-Clause, MPL-2.0, Unicode-DFS-2016, Zlib — four warnings; this run shows five including `0BSD`); however all five are `license-not-encountered` advisory-only (policy permits the license but no in-tree crate carries it) and do NOT represent a new license brought in by this task's new crates. Final line `advisories ok, bans ok, licenses ok, sources ok` is the gate; passes clean.
- **Carryforward note:** Modules `error.rs`, `request.rs`, `response.rs`, `codec.rs`, `hcm.rs` land in Tasks 5–9 respectively. `lib.rs` at this stage contains only the doc-comment and `#![forbid(unsafe_code)]`; no module declarations.
