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

---

## Task 2 — `envoy-config` `CodecType::HTTP2` accept-flip + `Http2OverTlsNotSupported`

- **Commit:** _(pending — set on commit; this task lands in a single commit per phase 05.2 PLAN convention)_
- **Deliverables:** D2.a (schema half) — `validate_hcm` narrows the HTTP2/HTTP3 rejection to HTTP3-only; HTTP2 is accepted on plaintext filter chains. New `ConfigError::Http2OverTlsNotSupported` rejects HTTP2 on filter chains carrying `transport_socket: envoy.transport_sockets.tls` (TLS+ALPN+H2 deferred per parent-05 SPEC §4). `validate_hcm` signature gains a `chain_has_tls: bool` parameter, plumbed at the single call site from `chain.transport_socket.as_ref().is_some_and(|ts| ts.name == crate::TLS_TRANSPORT_SOCKET)`.
- **ADR landed:** None (Task 2 is purely surface-narrowing; no decisions needed beyond what parent-05 SPEC §4 already settled).
- **Files modified:**
  - `crates/envoy-config/src/lib.rs` — appended `Http2OverTlsNotSupported` variant immediately before `UnsupportedCodecType`; updated the now-stale `UnsupportedCodecType` `#[error(...)]` message (was: `"…only AUTO and HTTP1 are supported in phase 04"`; now: `"…only AUTO, HTTP1, and HTTP2 are supported"` — phase-04 reference dropped because HTTP2 is accepted post-Task-2).
  - `crates/envoy-config/src/bootstrap.rs` — narrowed `validate_hcm` codec_type match arm; added 4-line TLS+HTTP2 rejection block; extended `validate_hcm` signature with `chain_has_tls: bool`; computed `chain_has_tls` at the per-chain loop in `validate`; deleted the pre-Task-2 `rejects_codec_type_http2` test (the new `parses_hcm_with_codec_type_http2` replaces it); appended `parses_hcm_with_codec_type_http2` and `rejects_hcm_with_codec_type_http2_on_tls_listener` tests.
  - `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_invalid_codec_type.yaml` — switched the `codec_type` from `HTTP2` to `HTTP3`. The seed exists to demonstrate "invalid codec_type rejected"; HTTP2 is now valid, so the seed file's pedagogical value moves to HTTP3. **Not in PLAN's Files list** — see Deviations.
  - `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` — this section appended.
- **LoC:** ~120 raw lines net (PLAN estimated ~80). The overshoot is dominated by the two new test bodies (long YAML literals — ~45 lines each, common to phase-05.2-style validator tests) and the 1-line fuzz-corpus seed adjustment. The non-test code delta is tight: the `validate_hcm` signature + body delta is ~10 lines (signature change + 1 doc-comment, codec_type match rewrite, 4-line TLS+HTTP2 block); the call-site delta is the 6-line `chain_has_tls` snapshot; the lib.rs delta is the 8-line `Http2OverTlsNotSupported` doc-comment + variant + 1-line `UnsupportedCodecType` message update. PLAN's ~80 estimate appears to have under-counted the test YAML body (each ~40 lines because PLAN used `parse_bootstrap` directly rather than the existing `make_hcm_listener_yaml` helper, which would have shrunk them; preserving PLAN's structure verbatim was the higher-value choice for traceability).
- **Verification:**

  Step 2.2 — `cargo test -p envoy-config parses_hcm_with_codec_type_http2 -- --nocapture` (failing-test confirmation, before validator change):
  ```
  thread 'bootstrap::tests::parses_hcm_with_codec_type_http2' (16040228) panicked at crates/envoy-config/src/bootstrap.rs:4682:47:
  parses: UnsupportedCodecType { got: HTTP2 }
  test bootstrap::tests::parses_hcm_with_codec_type_http2 ... FAILED
  test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 147 filtered out; finished in 0.00s
  ```
  Failed exactly as PLAN predicted (`UnsupportedCodecType { got: HTTP2 }`).

  Step 2.4 — same command after Steps 2.3+2.6+2.7 implementation:
  ```
  test bootstrap::tests::parses_hcm_with_codec_type_http2 ... ok
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 147 filtered out; finished in 0.00s
  ```

  Step 2.8 — both new tests pass together:
  ```
  $ cargo test -p envoy-config -- parses_hcm_with_codec_type_http2 rejects_hcm_with_codec_type_http2_on_tls_listener
  test bootstrap::tests::parses_hcm_with_codec_type_http2 ... ok
  test bootstrap::tests::rejects_hcm_with_codec_type_http2_on_tls_listener ... ok
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 147 filtered out; finished in 0.00s
  ```

  Step 2.11 — full crate test suite (post Step 2.9 deletion + fuzz-corpus fix):
  ```
  $ cargo test -p envoy-config 2>&1 | grep '^test result'
  test result: ok. 148 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
  Net delta: +1 (147 → 148). Two added (`parses_hcm_with_codec_type_http2`, `rejects_hcm_with_codec_type_http2_on_tls_listener`); one deleted (`rejects_codec_type_http2`). HTTP3-rejection coverage retained by the unchanged `rejects_codec_type_http3` test (lines ~3287 post-deletion).

  Step 2.12 — workspace-wide gates:
  ```
  $ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.30s

  $ cargo fmt --all -- --check
  (empty — clean)
  ```
  Note: initial `cargo fmt --all -- --check` flagged a let-else line-wrap inside the new `parses_hcm_with_codec_type_http2` test (rustfmt prefers expanded multi-line form over PLAN's single-line form). Applied `cargo fmt --all`; re-ran `cargo fmt --all -- --check` → clean. Test still passes after the reformat.

  Workspace-wide test sanity (validator signature change is internal but verified non-breaking):
  ```
  $ cargo test --workspace 2>&1 | grep -E '^test result' | sort -u
  (all "ok"; no failures across 30+ test binaries; 148 in envoy-config matches above)
  ```

- **Deviations from PLAN:**
  1. **PLAN Step 2.10 skipped — redundant with existing test (per PLAN's own escape clause).** The pre-Task-2 `rejects_codec_type_http3` test at `bootstrap.rs:3312` already does exactly what Step 2.10's `still_rejects_hcm_with_codec_type_http3` would add (uses `make_hcm_listener_yaml` + `parse_then_validate`, asserts `UnsupportedCodecType { got: CodecType::HTTP3 }`). PLAN explicitly permits skipping in this case; net test delta is therefore +1 (not +2 or +3 as PLAN allowed for).
  2. **Existing test name was `rejects_codec_type_http2` (without the `_hcm_with` infix), not `rejects_hcm_with_codec_type_http2` as PLAN stated.** Verified by `grep -n 'fn rejects_codec_type'`. Same body, same intent — deleted the test under its actual name. PLAN's name guess was off-by-an-infix; substantive equivalence preserved.
  3. **Fuzz-corpus seed `hcm_invalid_codec_type.yaml` updated (HTTP2 → HTTP3).** Not in PLAN's Files list (PLAN line 273-274). The seed file declares "invalid codec_type → must reject"; HTTP2 was the only invalid non-HTTP3 codec pre-Task-2. After the accept-flip, HTTP2 parses cleanly, so the seed silently became a happy-path file masquerading as a rejection seed and broke `fuzz_corpus_seeds_parse_or_reject_cleanly`. Switching the seed's codec to HTTP3 preserves the seed's pedagogical role (the only remaining "invalid" codec_type post-Task-2). One-line YAML diff; no fuzzer corpus widening.
  4. **`UnsupportedCodecType` `#[error(...)]` message updated to drop "phase 04" and include HTTP2.** Mentioned in pre-task instructions (the user's brief noted this would be needed). The message now reads `"unsupported codec_type: {got:?}; only AUTO, HTTP1, and HTTP2 are supported"` — phase-04 anchor removed because HTTP2 is post-Task-2 accepted; future HTTP3 work will revise again.
  5. **`validate_hcm` doc-comment extended with a `chain_has_tls` paragraph.** Two extra lines beyond PLAN's mechanical signature change, documenting the parameter's contract. Cost: +2 LoC; benefit: future readers don't have to grep the call site to learn what TLS-state the bool represents.
- **Carryforward note:** D2.a's runtime half lands later in phase 05.2 (the schema accepts HTTP2; no codec is wired into the connection-handling path until Tasks 5–9 land the `envoy-http2` crate's modules and Task 10 wires HCM dispatch). The `Http2OverTlsNotSupported` variant covers per-listener filter-chain TLS detection only; per-listener-filter (`tls_inspector`) state is not consulted (TLS termination still happens entirely in transport_socket). When TLS+ALPN+H2 lands in a later phase the variant retires by deletion (or relaxes its predicate) — its doc-comment names that future work explicitly.
