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

---

## Task 3 — `envoy-config` `Http2ProtocolOptions` struct + validator (RFC 7540 ranges)

- **Commit:** _(pending — set on commit; this task lands in a single commit per phase 05.2 PLAN convention)_
- **Deliverables:** D2.b (schema half) — new `Http2ProtocolOptions` struct in `envoy-config::bootstrap` with 4 optional `u32` fields (`max_concurrent_streams`, `initial_stream_window_size`, `initial_connection_window_size`, `max_frame_size`) per parent-05 SPEC §6 signpost 2. New `http2_protocol_options: Option<Http2ProtocolOptions>` field on `HttpConnectionManagerConfig`. New `ConfigError::Http2ProtocolOptionsOutOfRange { field: &'static str, value: u32, range: (u32, u32) }` variant. `validate_hcm` extended with RFC 7540 §6.5.2 / §6.9.1 / §6.9.2 range checks (only run when `Some`; absent = h2-crate defaults at HCM construction time). 7 new validator unit tests: 2 happy-path (default + all-fields parse round-trip), 4 range-rejection (too-small max_frame_size; too-large max_frame_size; window_size 2^31; both window-size variants), 1 unknown-field (`hpack_table_size` rejected by `deny_unknown_fields`).
- **ADR landed:** None (Task 3 is a direct application of parent-05 SPEC §6 signpost 2 + RFC 7540 ranges; no decisions needed beyond what SPEC and the RFC settle).
- **Files modified:**
  - `crates/envoy-config/src/lib.rs` — appended `Http2ProtocolOptionsOutOfRange` variant immediately after `Http2OverTlsNotSupported`; added `Http2ProtocolOptions` to the `pub use bootstrap::{...}` re-export list (alphabetic position between `HeaderMatcherMode` and `HttpConnectionManagerConfig`).
  - `crates/envoy-config/src/bootstrap.rs` — added `Http2ProtocolOptions` struct just before `RouteConfiguration`; added `http2_protocol_options: Option<Http2ProtocolOptions>` field on `HttpConnectionManagerConfig` between `codec_type` and `route_config`; extended `validate_hcm` with the 3 range checks (max_frame_size has both lower- and upper-bound; the two window sizes have only upper-bound since min is 0); appended 7 tests + the `http2_options_yaml` helper to `tests` mod.
  - `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` — this section appended.
- **LoC:** ~210 raw lines. Breakdown: struct (35 lines incl. doc-comments); HCM field insertion (7 lines incl. doc-comment); re-export shuffle (1-line net); ConfigError variant (15 lines); validator extension (40 lines); 7 tests + helper (~110 lines, dominated by repeated YAML literals — happy-path tests at ~37 lines each with full bootstrap; helper at ~50 lines including the 4 conditional field push_strs and the format!). Matches PLAN's ~200 estimate (overshoot is the helper's flexibility scaffold).
- **Verification:**

  Step 3.2 — `cargo test -p envoy-config -- parses_hcm_http2_protocol_options` (failing-test confirmation, before struct exists):
  ```
  error[E0609]: no field `http2_protocol_options` on type `&bootstrap::HttpConnectionManagerConfig`
      --> crates/envoy-config/src/bootstrap.rs:4772:21
       |
  4772 |         assert!(hcm.http2_protocol_options.is_none());
       |                     ^^^^^^^^^^^^^^^^^^^^^^ unknown field
       |
       = note: available fields are: `stat_prefix`, `codec_type`, `route_config`, `http_filters`
  ```
  Failed exactly as PLAN predicted (no `http2_protocol_options` field; 2 errors, one per new test).

  Step 3.6 — same command after Steps 3.3/3.4/3.5:
  ```
  running 2 tests
  test bootstrap::tests::parses_hcm_http2_protocol_options_default ... ok
  test bootstrap::tests::parses_hcm_http2_protocol_options_all_fields ... ok

  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 148 filtered out; finished in 0.00s
  ```

  Step 3.7 — failing validator-range tests (after writing tests, before adding variant):
  ```
  error[E0599]: no variant named `Http2ProtocolOptionsOutOfRange` found for enum `ConfigError`
  ...
  error: could not compile `envoy-config` (lib test) due to 4 previous errors
  ```
  Compile-fails as PLAN predicted (4 errors, one per new test).

  Step 3.10 — same command after Steps 3.8 + 3.9:
  ```
  running 4 tests
  test bootstrap::tests::rejects_http2_protocol_options_max_frame_size_too_large ... ok
  test bootstrap::tests::rejects_http2_protocol_options_initial_stream_window_size_too_large ... ok
  test bootstrap::tests::rejects_http2_protocol_options_max_frame_size_too_small ... ok
  test bootstrap::tests::rejects_http2_protocol_options_initial_connection_window_size_too_large ... ok

  test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 150 filtered out; finished in 0.00s
  ```

  Step 3.11 — unknown-field rejection test:
  ```
  running 1 test
  test bootstrap::tests::rejects_http2_protocol_options_unknown_field ... ok

  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 154 filtered out; finished in 0.00s
  ```

  Step 3.12 — full crate test suite:
  ```
  $ cargo test -p envoy-config 2>&1 | grep '^test result'
  test result: ok. 155 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
  Net delta: +7 (148 → 155). All 7 new tests land green.

  Step 3.12 — workspace-wide gates:
  ```
  $ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.66s

  $ cargo fmt --all -- --check
  (empty — clean)
  ```
  Initial clippy run flagged `collapsible_if` on the 3 nested `if let Some(v) = ... { if <range-violation> { ... } }` blocks (the PLAN's note covered `manual_range_contains` but not `collapsible_if`; this clippy lint demands let-chain form on Rust 1.95). Applied let-chain rewrite (`if let Some(v) = ... && <pred> { ... }`) and used `(MIN..=MAX).contains(&v)` for the max_frame_size two-sided range to silence both lints in one stroke. Re-ran clippy → clean. Tests still pass after the rewrite (logic is identical).

  Workspace-wide test sanity (Task 3 schema change is internal to `envoy-config` but verified non-breaking):
  ```
  $ cargo test --workspace 2>&1 | grep -E '^test result' | sort -u
  (all "ok"; 155 in envoy-config matches above; no other crate test count changed)
  ```

- **Deviations from PLAN:**
  1. **Validator block uses let-chain + `RangeInclusive::contains` instead of PLAN's mechanical `if v < MIN || v > MAX` form.** PLAN's note at Step 3.9 mentioned this possibility for the `manual_range_contains` lint; on Rust 1.95 with this toolchain's clippy config, `collapsible_if` also fires on the nested `if let Some(v) = ... { if <pred> { ... } }` shape, so a let-chain rewrite was needed regardless. Net effect: identical control-flow + same return values; just shorter/idiomatic Rust. The 4 range-rejection tests + the unknown-field test all still pass on the rewritten validator, so semantics are unchanged.
  2. **Helper YAML body's `max_concurrent_streams` push_str collapsed to one line.** rustfmt preferred the single-line form (`opts_block.push_str(&format!("                  max_concurrent_streams: {v}\n"));`) over the multi-line form copied verbatim from PLAN line 853-857. Cosmetic only; helper still produces identical YAML.
- **Carryforward note:** Runtime use of `Http2ProtocolOptions` (consuming the 4 fields when constructing the `h2::server::Builder` inside `envoy-http2`) lands in Tasks 8–9. The schema-level field-naming and range-validator obligations are settled here. Future Envoy `Http2ProtocolOptions` field additions (allow_connect, hpack_table_size, override_stream_error_on_invalid_http_message, connection_keepalive, ...) extend the struct under `deny_unknown_fields` and may add new `Http2ProtocolOptionsOutOfRange` callsites; the variant's `field: &'static str` parameterization is built for that growth.
