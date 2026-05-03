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
- **Post-review fixup:** Post-Task-3 review fixup: switched `rejects_http2_protocol_options_unknown_field` from `matches!(err, ConfigError::Yaml(_))` to the file-local `assert_unknown_field(err)` helper for consistency with the 4 existing unknown-field tests at lines 1643-1703 (per code-quality reviewer follow-up I1).
- **Carryforward note:** Runtime use of `Http2ProtocolOptions` (consuming the 4 fields when constructing the `h2::server::Builder` inside `envoy-http2`) lands in Tasks 8–9. The schema-level field-naming and range-validator obligations are settled here. Future Envoy `Http2ProtocolOptions` field additions (allow_connect, hpack_table_size, override_stream_error_on_invalid_http_message, connection_keepalive, ...) extend the struct under `deny_unknown_fields` and may add new `Http2ProtocolOptionsOutOfRange` callsites; the variant's `field: &'static str` parameterization is built for that growth.

---

## Task 4 — Fuzz corpus seed `hcm_codec_http2.yaml` + `.gitignore` allow-list + corpus-walk acceptance test

- **Commit:** _(pending — set on commit; this task lands in a single commit per phase 05.2 PLAN convention)_
- **Deliverables:** D2 fuzz signal — new fuzz corpus seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml` exercising HCM `codec_type: HTTP2` + listener-side `http2_protocol_options` (all 4 fields populated with mid-range values) through the existing `parse_bootstrap` fuzz target. Allow-list entry appended to `crates/envoy-config/fuzz/.gitignore`. 1 new corpus-walk acceptance test `fuzz_corpus_hcm_codec_http2_seed_parses` verifying the seed parses cleanly through the schema landed in Tasks 2–3 (asserts `CodecType::HTTP2` + `max_concurrent_streams == Some(100)`).
- **ADR landed:** None (Task 4 is mechanical fuzz-corpus extension; no decisions needed beyond what SPEC §6 signpost 25 already settled).
- **Files modified:**
  - `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml` (created) — 32-line YAML mirroring PLAN Step 4.3 verbatim.
  - `crates/envoy-config/fuzz/.gitignore` — 1-line allow-list entry appended after `!corpus/parse_bootstrap/strict_dns_cluster.yaml` (the prior 13th entry); seeds block now 14 entries.
  - `crates/envoy-config/src/bootstrap.rs` — appended `fuzz_corpus_hcm_codec_http2_seed_parses` test inside `tests` mod just before the `http2_options_yaml` helper.
  - `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` — this section appended.
- **LoC:** ~50 (32 seed YAML + 1 gitignore line + ~17 test incl. doc-comment + this PROGRESS section). PLAN estimated ~30 (seed ~25 + gitignore ~1 + test ~12); modest overshoot driven by rustfmt's let-else reflow expanding the test to 17 lines (expected pattern; same reflow happened in Task 2). YAML size matches PLAN line-for-line.
- **Verification:**

  Step 4.2 — `cargo test -p envoy-config fuzz_corpus_hcm_codec_http2_seed_parses -- --nocapture` (failing-test confirmation, before seed file exists):
  ```
  error: couldn't read `crates/envoy-config/src/../fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml`: No such file or directory (os error 2)
      --> crates/envoy-config/src/bootstrap.rs:5004:20
  error: could not compile `envoy-config` (lib test) due to 1 previous error
  ```
  Failed exactly as PLAN predicted (`include_str!` compile error; seed file absent).

  Step 4.5 — same command after Steps 4.3 + 4.4:
  ```
  running 1 test
  test bootstrap::tests::fuzz_corpus_hcm_codec_http2_seed_parses ... ok

  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 155 filtered out; finished in 0.00s
  ```

  Step 4.6 — skipped per PLAN (local environment lacks `cargo +nightly fuzz`; CI fuzz job at Task 14 will exercise the seed).

  Step 4.7 — `git status crates/envoy-config/fuzz/corpus/parse_bootstrap/`:
  ```
  Untracked files:
        crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml
  ```
  Seed appears as new (not ignored); allow-list entry working.

  Workspace gates (post-fmt):
  ```
  $ cargo build --workspace --all-targets 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.02s

  $ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.19s

  $ cargo fmt --all -- --check
  (empty — clean)

  $ cargo test -p envoy-config 2>&1 | grep '^test result'
  test result: ok. 156 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
  Net delta: +1 (155 → 156). The new corpus-walk test passes.

  Note: initial `cargo fmt --all -- --check` flagged a let-else reflow inside the new test (rustfmt prefers expanded multi-line form for the chained `.typed_config.as_ref().unwrap()` selector). Applied `cargo fmt --all`; re-ran check → clean. Test still passes after the reformat. Same rustfmt nudge as Task 2 Step 2.12.

- **Deviations from PLAN:** None. Steps executed verbatim. The PLAN's reference test name `fuzz_corpus_hcm_route_to_cluster_seed_parses` is preserved in the new test's doc-comment for traceability even though no per-seed test by that name exists in `bootstrap.rs` today (the existing corpus-walk uses a single loop test `fuzz_corpus_seeds_parse_or_reject_cleanly`); the doc-comment still anchors the "04.x corpus-walk acceptance pattern" intent. The new `fuzz_corpus_hcm_codec_http2_seed_parses` is the first per-seed accept-test in the file — a stricter variant that asserts on parsed content, not just parse-or-reject. Future tasks may consolidate by either (a) extending the loop test with content-asserting branches, or (b) adding more per-seed tests next to this one; PLAN does not prescribe.
- **Carryforward note:** The new seed exercises the schema landed in Tasks 2–3 and the parent `parse_bootstrap` fuzz target. CI fuzz job at Task 14 will run the existing `parse_bootstrap` target against the full corpus including this seed; no action needed before then. When `validate_hcm` gains additional HTTP2-related range checks (e.g., future `connection_keepalive` sub-message range bounds), this seed remains valid because all 4 currently-checked fields use mid-range values well inside the RFC 7540 windows. The `fuzz_corpus_seeds_parse_or_reject_cleanly` loop test does NOT yet list this seed; intentional — the per-seed `fuzz_corpus_hcm_codec_http2_seed_parses` test is stricter (content assertions) and serves a different role. A future cleanup task may add the seed to the loop's "expected to parse" list as a redundant gate.
- **Post-review fixup:** Two Minor code-quality findings closed in a single fixup commit: (M1) the new test's doc-comment now points at the actual cohort-level loop test `fuzz_corpus_seeds_parse_or_reject_cleanly` (line 2274) instead of the non-existent `fuzz_corpus_hcm_route_to_cluster_seed_parses`; (M5) the seed `fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml` is now registered in the cohort loop's expected-parse list as belt-and-suspenders defense in depth.

---

## Task 5 — `crates/envoy-http2/src/error.rs` (`Http2Error` typed-error enum)

- **Commit:** _(pending — set on commit; this task lands in a single commit per phase 05.2 PLAN convention)_
- **Deliverables:** D3 error.rs — new `error` submodule under `envoy-http2` housing the `Http2Error` enum (6 variants per parent-05 SPEC §3 D3). Three source-preserving variants wrap `h2::Error` via `#[source]` (`H2Handshake`, `H2StreamAccept`, `H2BodyRead`); three pure-shape variants carry no source (`MissingAuthority`, `MalformedH2HeaderBlock`, `BadStatusCode { status: u16 }`). `Http2Error` re-exported at crate root via `pub use error::Http2Error`. Initially 2 unit tests covered the 2 distinct Display shapes for the non-wrapping variants (unit variant `MissingAuthority` + struct variant `BadStatusCode { status }`); a post-review fixup added a 3rd test (`h2_handshake_displays_with_source`) covering the `{source}` Display shape shared by `H2Handshake` / `H2StreamAccept` / `H2BodyRead`. `h2::Error` has a public construction path via `impl From<h2::Reason>` (h2 0.4.13 re-exports `Reason` publicly), so the Display-with-source shape is testable; the smoke test exercises it via `h2::Reason::PROTOCOL_ERROR.into()`. Picking one of the three wrapping variants is sufficient — they share the same `#[error("...{source}")]` Display shape, so coverage of one is signal for all three.
- **ADR landed:** None (Task 5 directly implements parent-05 SPEC §3 D3; no decisions needed beyond what SPEC settles).
- **Files modified:**
  - `crates/envoy-http2/src/error.rs` (created) — 75 lines incl. doc-comments and tests.
  - `crates/envoy-http2/src/lib.rs` — appended `mod error;` + `pub use error::Http2Error;` at module scope after the existing doc-comment block (4 added lines including blank lines). Note on placement: PLAN Step 5.1 said "Insert before the closing 05.3-projected doc comment"; the doc-comment block is one contiguous `//! ...` header (lines 3–18), so inserting code mid-block would syntactically break the file. Placed the mod declaration after the doc-comment per the PLAN's own escape clause ("standard interpretation: place at the top of the file's code section, AFTER the doc-comment block").
  - `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` — this section appended.
- **LoC:** ~80 raw lines (75 error.rs + 4 lib.rs delta + this PROGRESS section). Matches PLAN's ~60 estimate within reasonable margin; the ~15-line overshoot is rustfmt-induced multi-line forms (the `MissingAuthority` `#[error("...")]` attribute exceeded rustfmt's 100-col threshold and reflowed to 3 lines; the `assert!` in `missing_authority_displays_descriptively` likewise reflowed to 4 lines from PLAN's 1-line form). Functional content matches PLAN line-for-line.
- **Verification:**

  Step 5.2 — `cargo test -p envoy-http2` (failing-test confirmation, before enum exists, with only the test module in `error.rs`):
  ```
  error[E0432]: unresolved import `error::Http2Error`
    --> crates/envoy-http2/src/lib.rs:22:9
     |
  22 | pub use error::Http2Error;
     |         ^^^^^^^^^^^^^^^^^ no `Http2Error` in `error`

  error[E0432]: unresolved import `super::Http2Error`
   --> crates/envoy-http2/src/error.rs:5:9
    |
  5 |     use super::Http2Error;
    |         ^^^^^^^^^^^^^^^^^ no `Http2Error` in `error`

  error: could not compile `envoy-http2` (lib test) due to 2 previous errors
  ```
  Failed exactly as PLAN predicted (`Http2Error` not defined; both lib and lib-test fail to compile because the `pub use` re-export and the test's `use super::Http2Error` both reference the not-yet-existent enum).

  Step 5.4 — same command after Step 5.3:
  ```
  running 2 tests
  test error::tests::missing_authority_displays_descriptively ... ok
  test error::tests::bad_status_code_displays_value ... ok

  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
  Suite goes 0 → 2 (the crate had no tests pre-Task-5).

  `cargo build -p envoy-http2`:
  ```
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.41s
  ```

  Workspace gates:
  ```
  $ cargo build --workspace --all-targets 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.67s

  $ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.60s

  $ cargo fmt --all -- --check
  (empty — clean, after `cargo fmt --all` reflowed two lines per the LoC note)
  ```

  Workspace-wide test sanity (no other crate's test count changed; envoy-http2 went 0 → 2):
  ```
  $ cargo test --workspace 2>&1 | grep -E '^test result' | sort | uniq -c
  (no failures across all test binaries; 2 envoy-http2 unit tests visible)
  ```

- **Deviations from PLAN:**
  1. **`mod error;` placed AFTER the lib.rs doc-comment block, not "before the closing 05.3-projected paragraph" mid-comment.** PLAN Step 5.1's wording is ambiguous because `lib.rs`'s entire `//! ...` header (lines 3–18) is a contiguous doc-comment block ending with the 05.3-projected paragraph; a `mod error;` declaration cannot syntactically appear inside a `//! ...` block. PLAN itself anticipates this in the pre-task context: "the standard interpretation is: place the mod declaration at the top of the file's code section, AFTER the doc-comment block". Followed the standard interpretation. The doc-comment block is preserved intact.
  2. **rustfmt reflowed 2 lines.** The `MissingAuthority` `#[error(...)]` attribute exceeded the 100-col line limit and rustfmt expanded it to a 3-line form. The first `assert!(s.contains(...), "...")` likewise expanded to 4 lines. Functionally identical; same as the rustfmt nudges noted in Tasks 2 and 4.
- **Carryforward note:** Module slots `request.rs` (Task 6), `response.rs` (Task 7), `codec.rs` (Task 8), and `hcm.rs` (Task 9) will use `Http2Error` as their typed-error type. The 3 `h2::Error`-wrapping variants get exercised at those task boundaries (handshake → Task 9; stream accept → Task 9; body read → Task 6). The 3 pure-shape variants similarly: `MissingAuthority` gates the `:authority` → `Host:` synthesis (Task 6), `MalformedH2HeaderBlock` is defense-in-depth at the same site, and `BadStatusCode` gates the response status emission (Task 7). The `From<h2::Error>` blanket impl is intentionally absent — call sites must pick the right variant per failure context. The `BadStatusCode { status: u16 }` parameter type is `u16` (not `http::StatusCode`) because the variant exists precisely for emit-time values that escape the type-state guard; using `u16` keeps the failure path representable.
- **Post-review fixup:** Added `h2_handshake_displays_with_source` test (Display-with-source shape coverage for H2Handshake/H2StreamAccept/H2BodyRead, which share the `{source}` Display attribute) and corrected the Task 5 rationale that erroneously claimed `h2::Error` has no public constructor (it does, via `From<h2::Reason>`). Closes code-quality reviewer M1.

---

## Task 6 — `crates/envoy-http2/src/request.rs` (`http_to_envoy_request` adapter + 2 tests)

- **Commit:** _(pending — set on commit; this task lands in a single commit per phase 05.2 PLAN convention)_
- **Deliverables:** D3 request.rs — new `request` submodule under `envoy-http2` housing the `http_to_envoy_request` adapter that translates an `http::Request<Bytes>` (post-body-drain shape produced by the runtime caller in Task 9) into an `envoy_http1::codec::Request` value type. Pseudo-header mapping per parent-05 SPEC §6 signpost 12: `:method` → `Request.method`, `:path` → `Request.path` (preserving query string), `:authority` → synthesized as a `Host:` row appended at the bottom of `Request.headers` (per cross-sub-phase architectural rule 3, required by the existing 04.x Host-driven route-walk), `:scheme` → ignored. `Request.version` is set to `HttpVersion::Http11` because the route-walk treats requests uniformly post-codec-edge; H2 framing concerns stay inside `envoy-http2`. The adapter is `pub` (entry point for the future Task 9 HCM dispatch). 2 unit tests cover (a) header preservation through translation (lowercase-name + value pass-through) and (b) `:authority` → `Host:` synthesis. `MissingAuthority` is the failure mode when neither `parts.uri.authority()` nor a `Host:` header exists; `MalformedH2HeaderBlock` covers non-UTF-8 header values (defense-in-depth — h2 normally catches these earlier).
- **ADR landed:** None (Task 6 directly applies parent-05 SPEC §6 signpost 12 + cross-sub-phase architectural rule 3; no decisions needed beyond what SPEC settles).
- **Files modified:**
  - `crates/envoy-http2/src/request.rs` (created) — 149 lines incl. doc-comments + 2 unit tests + 1 build-helper.
  - `crates/envoy-http2/src/lib.rs` — appended `pub mod request;` at module scope. rustfmt reordered the two module declarations into alphabetical form (`mod error;` first, then `pub mod request;`) on `cargo fmt --all`; preserved.
  - `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` — this section appended.
- **LoC:** ~150 raw lines (149 request.rs + 1 lib.rs delta + this PROGRESS section). PLAN estimated ~110 (impl ~80 + 2 tests ~30); the ~40-line overshoot is dominated by the 14-line module-level `//!` doc-comment block (PLAN budgeted impl-only at 80 incl. doc) and the 23-line `build_request` helper which encodes the H2-specific Uri-mutation pattern (`http://{authority}{uri}`) needed to mirror `parts.uri.authority()` from real h2 traffic. Functional content matches PLAN line-for-line.
- **Verification:**

  Step 6.2 — `cargo test -p envoy-http2` (failing-test confirmation, before `http_to_envoy_request` exists, with only the test module in `request.rs`):
  ```
  error[E0425]: cannot find function `http_to_envoy_request` in this scope
    --> crates/envoy-http2/src/request.rs:45:19
     |
  45 |         let out = http_to_envoy_request(req).expect("translates");
     |                   ^^^^^^^^^^^^^^^^^^^^^ not found in this scope

  error[E0425]: cannot find function `http_to_envoy_request` in this scope
    --> crates/envoy-http2/src/request.rs:68:19
     |
  68 |         let out = http_to_envoy_request(req).expect("translates");
     |                   ^^^^^^^^^^^^^^^^^^^^^ not found in this scope
  ```
  Failed exactly as PLAN predicted (function not defined; both new tests fail to resolve the symbol).

  Step 6.4 — same command after Step 6.3:
  ```
  running 5 tests
  test error::tests::h2_handshake_displays_with_source ... ok
  test error::tests::bad_status_code_displays_value ... ok
  test error::tests::missing_authority_displays_descriptively ... ok
  test request::tests::http_to_envoy_request_synthesizes_host_from_authority ... ok
  test request::tests::http_to_envoy_request_lowercases_headers ... ok

  test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
  Suite goes 3 → 5 (Task 5 left it at 3 post-fixup; Task 6 adds 2).

  Workspace gates (post-fmt + post-doc-list-fix):
  ```
  $ cargo build --workspace --all-targets 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.71s

  $ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.64s

  $ cargo fmt --all -- --check
  (empty — clean)
  ```

  Workspace-wide test sanity: all green; envoy-http2 went 3 → 5 (matches per-crate run above); no other crate's test count moved.

- **Deviations from PLAN:**
  1. **rustfmt reordered `mod error;` and `pub mod request;` alphabetically.** PLAN Step 6.1 said "Insert immediately above the `mod error;` line" (i.e., `pub mod request;` first). After running `cargo fmt --all`, rustfmt produced the canonical `mod error;\npub mod request;` ordering (alphabetical, matching the standard library and most workspace conventions). Functionally identical; module-resolution order is independent of declaration order.
  2. **Doc-list continuation lines de-indented from 17 spaces to 4 spaces.** Initial clippy run flagged `clippy::doc_overindented_list_items` on the `:authority` bullet's continuation lines (PLAN's verbatim text used 17-space indent to align under the back-tick; clippy 1.95 demands list-item-relative 4-space indent). Reduced to 4-space continuation; meaning preserved.
  3. **Second test's `build_request(...)` call collapsed to one line by rustfmt.** PLAN's verbatim test body used multi-line argument form for the second test call too; rustfmt deemed it short enough for the 100-col single-line form and reflowed. Functionally identical; same kind of cosmetic rustfmt nudge as Tasks 2/4/5.
- **Carryforward note:** The adapter is consumed by Task 9 (downstream H2C HCM dispatch) — that task drains `h2::RecvStream` into `Bytes` then hands the `http::Request<Bytes>` here. Task 7 (response emitter) operates on the symmetric output side; both share `Http2Error` as the typed-error currency. The `MalformedH2HeaderBlock` defense-in-depth path is exercised here for the first time (non-UTF-8 header value); the variant's other intended use (structurally invalid pseudo-headers — missing `:method` or `:path`) is gated by the `http::Request` type itself, which refuses to construct without those fields, so the adapter doesn't need to re-check. Future H2 trailers translation (not in 05.2 SPEC §4 scope) would extend this module with a sibling adapter; the current shape leaves room.
- **Post-review fixup:** Six review findings closed in a single fixup commit. (I1) `MalformedH2HeaderBlock` doc-comment broadened to cover non-UTF-8 header values. (I2) Empty `Host:` value tightened to raise `MissingAuthority`. (I3) Added 2 failure-path tests (`http_to_envoy_request_missing_authority_returns_error`, `http_to_envoy_request_non_utf8_header_value_returns_error`). (M1) Added `pub use request::http_to_envoy_request;` re-export at crate root for symmetry with `Http2Error`. (M2) Removed dead `let _: &HeaderMap = req.headers();` line in `build_request` test helper + corresponding `HeaderMap` import. (M3) Added rationale comment for the `"/"` path fallback. Closes code-quality reviewer I1 + I2 + I3 + M1 + M2 + M3 on Task 6.

---

## Task 7 — `crates/envoy-http2/src/response.rs` (`build_http_response` + `send_envoy_response` adapters + 2 tests)

- **Commit:** _(pending — set on commit; this task lands in a single commit per phase 05.2 PLAN convention)_
- **Deliverables:** D3 response.rs — new `response` submodule under `envoy-http2` housing two adapters: `build_http_response` (pure function: `&envoy_http1::Response` → `http::Response<()>` carrying status + headers; H2-forbidden hop-by-hop headers stripped) and `send_envoy_response` (async: drives the actual H2 wire emission via `h2::server::SendResponse::send_response` + `SendStream::send_data`). H2-forbidden hop-by-hop strip per RFC 7540 §8.1.2.2 + cross-sub-phase architectural rule 4: `connection`, `transfer-encoding`, `upgrade`, `keep-alive`, `proxy-connection`. Header names lowercased before emission per RFC 7540 §8.1.2 (parent §6 signpost 11 — defense-in-depth; the h2 crate would reject uppercase names). 2 unit tests cover (a) hop-by-hop strip preserves non-forbidden headers and (b) status + content-type are preserved on the translation. Both `build_http_response` + `send_envoy_response` re-exported at crate root per the M1 convention established in Task 6.
- **ADR landed:** None (Task 7 directly applies parent-05 SPEC §3 D3 + cross-sub-phase architectural rule 4 + RFC 7540 §8.1.2 / §8.1.2.2; no decisions needed beyond what SPEC + the RFC settle).
- **Files modified:**
  - `crates/envoy-http2/src/response.rs` (created) — 99 lines incl. doc-comments + 2 unit tests + 1 build-helper.
  - `crates/envoy-http2/src/lib.rs` — appended `pub mod response;` after `pub mod request;` and `pub use response::{build_http_response, send_envoy_response};` after the existing `pub use request::http_to_envoy_request;` re-export. Mirrors the M1 convention from Task 6.
  - `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` — this section appended.
- **LoC:** ~105 raw lines (99 response.rs + 2 lib.rs delta + this PROGRESS section). PLAN estimated ~120 (impl ~80 + hop-by-hop strip ~10 + 2 tests ~30); the implementation came in slightly under because the hop-by-hop strip is a 4-element `const &[&str]` + a single `contains` check (5 effective lines) rather than a separate helper. Functional content matches PLAN line-for-line.
- **Verification:**

  Step 7.2 — `cargo test -p envoy-http2` (failing-test confirmation, before `build_http_response` exists, with only the test module in `response.rs`):
  ```
  error[E0432]: unresolved import `response::build_http_response`
   --> crates/envoy-http2/src/lib.rs:25:21
    |
  25 | pub use response::{build_http_response, send_envoy_response};
    |                     ^^^^^^^^^^^^^^^^^^^ no `build_http_response` in `response`

  error[E0425]: cannot find function `build_http_response` in this scope
    --> crates/envoy-http2/src/response.rs:36:25
     |
  36 |         let http_resp = build_http_response(&resp).expect("builds");
     |                         ^^^^^^^^^^^^^^^^^^^ not found in this scope
  ```
  Failed exactly as PLAN predicted (function not defined; both new tests + the crate-root re-export fail to resolve the symbol).

  Step 7.4 — same command after Step 7.3:
  ```
  running 9 tests
  test error::tests::h2_handshake_displays_with_source ... ok
  test error::tests::missing_authority_displays_descriptively ... ok
  test error::tests::bad_status_code_displays_value ... ok
  test request::tests::http_to_envoy_request_missing_authority_returns_error ... ok
  test response::tests::envoy_response_to_http2_strips_h2_forbidden_headers ... ok
  test response::tests::envoy_response_to_http2_preserves_status_and_body ... ok
  test request::tests::http_to_envoy_request_non_utf8_header_value_returns_error ... ok
  test request::tests::http_to_envoy_request_synthesizes_host_from_authority ... ok
  test request::tests::http_to_envoy_request_lowercases_headers ... ok

  test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
  Suite goes 7 → 9 (Task 6 left it at 7 post-fixup; Task 7 adds 2). Matches PLAN's Step 7.4 expected count exactly.

  Workspace gates:
  ```
  $ cargo build --workspace --all-targets 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.80s

  $ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.60s

  $ cargo fmt --all -- --check
  (empty — clean)
  ```

  Workspace-wide test sanity: all green; envoy-http2 went 7 → 9 (matches per-crate run above); no other crate's test count moved.

- **Deviations from PLAN:**
  1. **Test helper `synth_response` augmented with `reason: None`.** PLAN Step 7.2's verbatim test helper omits the `reason` field, but `envoy_http1::Response` carries `pub reason: Option<&'static str>` (verified at task time via `grep -nA 6 'pub struct Response' crates/envoy-http1/src/response.rs`: 4 fields — `status`, `reason`, `headers`, `body`). Without `reason: None` the helper fails to compile. Added `reason: None,` between `status` and `headers` in the struct literal; otherwise verbatim. The reason field is not exercised by either H2 test (H2 has no reason-phrase concept — only `:status`), so `None` is the right neutral value; the H1 codec falls back to a built-in canonical-reason table for `None`.
  2. **Crate-root re-export added: `pub use response::{build_http_response, send_envoy_response};`.** Adopting the M1 convention established in the Task 6 review fixup (which added `pub use request::http_to_envoy_request;` for symmetry with `Http2Error`). Both functions are public entry points downstream consumers (Task 9 HCM) will reach for; root-level re-export saves them an inner-path import. Task description called this out as a small departure from the literal PLAN.
  3. **rustfmt collapsed the `HeaderValue::from_str(value)` mapping to a 2-line form.** PLAN's verbatim impl used a 3-line form (`let header_value = HeaderValue::from_str(value)\n    .map_err(|_| Http2Error::MalformedH2HeaderBlock)?;`); rustfmt deemed it short enough for the 100-col 2-line form and reflowed. Functionally identical; same kind of cosmetic rustfmt nudge as Tasks 2/4/5/6.
- **Carryforward note:** Both adapters are consumed by Task 9 (downstream H2C HCM dispatch) — `build_http_response` is the pure status+headers translation step, exercised here in the unit tests; `send_envoy_response` is the async wire emission step, integration-tested in Task 9. The `Http2Error::H2BodyRead` variant name's "Read" suffix is a misnomer when applied to body WRITE (response emission); the doc-comment on `send_envoy_response` flags the future cleanup but defers it per SPEC §6 local signpost 21 (variant rename would touch the H2BodyRead read-side call sites in Task 9 too; deferring keeps the variant set stable across Tasks 5–9). The `H2_FORBIDDEN_HOP_BY_HOP` const list is the H2-side counterpart of the H1-side hop-by-hop handling that the H1 codec already does on the request path; symmetric coverage. Future H2 trailers emission (not in 05.2 SPEC §4 scope) would extend this module with a sibling `send_envoy_trailers` adapter; the current shape leaves room.
- **Post-review fixup:** Five review findings closed in a single fixup commit. (I1) `MalformedH2HeaderBlock` doc-comment broadened a third time to cover invalid header NAMES (Task 7's third trigger via `HeaderName::from_bytes`). (I2) `send_envoy_response` doc-comment now enumerates BOTH misnomers — `H2StreamAccept` for response-head-send + `H2BodyRead` for body-write — under the deferred-cleanup ledger. (M3) Added 2 failure-path tests covering `BadStatusCode` (status 99) and `MalformedH2HeaderBlock` (non-token header name). (M2) Added inline comment marking the intentional drop of `resp.reason` (RFC 7540 §8.1.2.4). (M4) Restored the load-bearing comment in `envoy_response_to_http2_preserves_status_and_body` explaining why the body-bytes assertion is delegated to integration tests. Closes code-quality reviewer I1 + I2 + M2 + M3 + M4 on Task 7.

## Task 8 — `crates/envoy-http2/src/codec.rs` (`Http2Codec` adapter / `h2::server::Builder` configurer)

- **Commit:** _(pending — set on commit; this task lands in a single commit per phase 05.2 PLAN convention)_
- **Deliverables:** D3 codec.rs — new `codec` submodule under `envoy-http2` housing a single thin adapter `build_h2_server(Option<&Http2ProtocolOptions>) -> h2::server::Builder` that maps the four 05.2-supported `Http2ProtocolOptions` fields onto the corresponding `h2::server::Builder` setters (`max_concurrent_streams`, `initial_window_size`, `initial_connection_window_size`, `max_frame_size`). Absent options leave the field at the `h2`-crate default. Centralizes the configuration shape so the HCM (Task 9) and the future Client (05.3) share it; only the listener-side Builder is mapped in 05.2 — the client-side `h2::client::Builder` mapping defers to 05.3 alongside `client.rs`. 1 unit smoke test covers both the configured-builder path and the `None` (defaults) path; the behavioral wire-effect verification of `max_concurrent_streams` is delegated to Task 9's `hcm.rs` integration test per PLAN. `build_h2_server` re-exported at crate root per the M1 convention established in Task 6.
- **ADR landed:** None (Task 8 directly applies parent-05 SPEC §3 D3; no decisions needed beyond what SPEC settles).
- **Files modified:**
  - `crates/envoy-http2/src/codec.rs` (created) — 55 lines incl. doc-comments + impl + 1 smoke test.
  - `crates/envoy-http2/src/lib.rs` — added `pub mod codec;` and `pub use codec::build_h2_server;`. rustfmt re-sorted both module declarations and re-exports alphabetically (`codec` before `error`/`request`/`response`).
  - `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` — this section appended.
- **LoC:** ~58 raw lines (55 codec.rs + 3 lib.rs delta after rustfmt's alpha-sort). PLAN estimated ~80 (impl ~60 + 1 unit test ~20); the implementation came in slightly under because the smoke test is intentionally minimal (the wire-effect test is delegated to Task 9 per PLAN) and the impl's `if let Some(...)` ladder is tight.
- **Verification:**

  Step 8.2 — `cargo test -p envoy-http2` (failing-test confirmation, before `build_h2_server` exists, with only the test module in `codec.rs` plus the crate-root `pub use`):
  ```
  error[E0432]: unresolved import `codec::build_h2_server`
    --> crates/envoy-http2/src/lib.rs:28:21
     |
  28 | pub use codec::build_h2_server;
     |         ^^^^^^^^^^^^^^^^^^^^^^ no `build_h2_server` in `codec`

  error[E0425]: cannot find function `build_h2_server` in this scope
    --> crates/envoy-http2/src/codec.rs:23:24
  error[E0425]: cannot find function `build_h2_server` in this scope
    --> crates/envoy-http2/src/codec.rs:24:32
  ```
  Failed exactly as PLAN predicted (function not defined; both call sites + the crate-root re-export fail to resolve the symbol).

  Step 8.4 — same command after Step 8.3:
  ```
  running 12 tests
  test codec::tests::build_h2_server_applies_protocol_options ... ok
  test error::tests::bad_status_code_displays_value ... ok
  test error::tests::missing_authority_displays_descriptively ... ok
  test error::tests::h2_handshake_displays_with_source ... ok
  test response::tests::build_http_response_rejects_invalid_status_code ... ok
  test request::tests::http_to_envoy_request_missing_authority_returns_error ... ok
  test response::tests::build_http_response_rejects_invalid_header_name ... ok
  test request::tests::http_to_envoy_request_synthesizes_host_from_authority ... ok
  test request::tests::http_to_envoy_request_lowercases_headers ... ok
  test request::tests::http_to_envoy_request_non_utf8_header_value_returns_error ... ok
  test response::tests::envoy_response_to_http2_preserves_status_and_body ... ok
  test response::tests::envoy_response_to_http2_strips_h2_forbidden_headers ... ok

  test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
  Suite goes 11 → 12 (Task 7 left it at 11 post-fixup; Task 8 adds 1). Matches PLAN's Step 8.4 expected count exactly.

  Workspace gates:
  ```
  $ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.59s

  $ cargo fmt --all -- --check
  (empty — clean, after rustfmt's alpha-sort of the module + re-export lines was applied)
  ```

  Workspace-wide test sanity: all green; envoy-http2 went 11 → 12 (matches per-crate run above); no other crate's test count moved.

- **h2 setter signature verification (PLAN's escape clause):** Confirmed at task time against `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/h2-0.4.13/src/server.rs`. All four setters take `&mut self, u32` and return `&mut Self`:
  ```
  692:    pub fn initial_window_size(&mut self, size: u32) -> &mut Self {
  726:    pub fn initial_connection_window_size(&mut self, size: u32) -> &mut Self {
  764:    pub fn max_frame_size(&mut self, max: u32) -> &mut Self {
  846:    pub fn max_concurrent_streams(&mut self, max: u32) -> &mut Self {
  ```
  Setter names match PLAN exactly; `&mut Self` return is not `#[must_use]`-annotated, so the bare-statement call form (`builder.max_concurrent_streams(v);`) compiles cleanly without unused-result warnings. No call-site adjustments needed.

- **Deviations from PLAN:**
  1. **rustfmt re-sorted module declarations + re-exports alphabetically.** PLAN Step 8.1 said "insert after `pub mod response;`" and "after `pub use response::{...};`"; rustfmt enforces alphabetical ordering for both `mod` and `use` blocks, so the final ordering is `pub mod codec; mod error; pub mod request; pub mod response;` and `pub use codec::build_h2_server; pub use error::Http2Error; pub use request::http_to_envoy_request; pub use response::{build_http_response, send_envoy_response};`. Same kind of cosmetic rustfmt nudge as in Tasks 2/4/5/6/7. Functionally identical to the PLAN's intent.

- **Carryforward note:** `build_h2_server` is consumed by Task 9's HCM, which calls it on connection accept to obtain a configured `h2::server::Builder` and then drives `Builder::handshake(io)` to obtain a `h2::server::Connection`. The 05.2 SPEC §3 D3 contract is now in place: HCM does NOT touch `Http2ProtocolOptions` directly — it passes `listener.http2_protocol_options.as_ref()` straight to `build_h2_server`, keeping the option-to-setter mapping in one place. When 05.3 adds `client.rs`, a sibling `build_h2_client(Option<&Http2ProtocolOptions>) -> h2::client::Builder` will live in this same module, sharing the field-by-field mapping shape; the listener-side / client-side `Http2ProtocolOptions` type is intentionally a single struct (cf. SPEC §3 D2.b — Envoy's upstream + downstream H2 use the same `Http2ProtocolOptions` proto, just attached to different config nodes). The smoke test compiles-only; the wire-effect verification (a peer observing `SETTINGS_MAX_CONCURRENT_STREAMS = 50` after handshake) lands in Task 9's `hcm.rs` `h2_protocol_options_max_concurrent_streams_applied` integration test.
- **Post-review fixup:** One Minor finding closed: (M2) added inline comment at `codec.rs:20-22` documenting the field rename `initial_stream_window_size` (envoy-config proto-canonical name) → `initial_window_size` (h2 setter, no `_stream_` infix). Closes code-quality reviewer M2 on Task 8.

## Task 9 — `crates/envoy-http2/src/hcm.rs` (HCM ConnectionHandler impl + 8 unit tests)

- **Commit:** _(pending — set on commit; this task lands in a single commit per phase 05.2 PLAN convention)_
- **Deliverables:** D3 hcm.rs — new `hcm` submodule under `envoy-http2` housing the `HCM` struct + `ConnectionHandler` impl. The HCM consumes `envoy_http1::HCMConfig` (re-exported as a type alias `envoy_http2::HCMConfig` for ergonomic naming, per cross-sub-phase architectural rule 2) and dispatches per-stream through the existing 04.x route-walk + `envoy_http1::hcm::build_response` + `BuildOutcome` arms. Per-connection driver: `h2::server::handshake` (configured via `build_h2_server(config.http2_protocol_options.as_ref())`); per-stream: `tokio::spawn` direct (parent §6 signpost 6, fire-and-forget; per-stream errors logged, not propagated). The `BuildOutcome::Synth(Response)` arm goes through the existing `send_envoy_response` (Task 7); the `BuildOutcome::Proxy { .. }` arm STUBS a 502 with a doctrine-line body (no cluster names, defense-in-depth) per SPEC §6 local signpost 21 — the real upstream H2 dispatch lands in 05.3 D13.3. The trait shape (BoxFuture-returning, NOT async-trait) mirrors `envoy_listener::ConnectionHandler` per SPEC §6 local signpost 19. 8 unit tests; 7 pass, 1 `#[ignore]` (test 8 — see Deviations).
- **ADR landed:** None (Task 9 directly applies parent-05 SPEC §3 D3 + cross-sub-phase architectural rule 2; no decisions needed beyond what SPEC settles).
- **Files modified:**
  - `crates/envoy-http2/src/hcm.rs` (created) — ~340 lines incl. doc-comments + impl + 8 tests.
  - `crates/envoy-http2/src/lib.rs` — added `pub mod hcm;` and `pub use hcm::{HCM, HCMConfig};`. rustfmt re-sorted `mod`/`use` blocks alphabetically.
  - `crates/envoy-http2/Cargo.toml` — added `envoy-cluster = { path = "../envoy-cluster" }` to `[dev-dependencies]` (test-only consumer of `ClusterManager::empty()`).
  - `crates/envoy-http1/src/hcm.rs` — visibility lift: `BuildOutcome` (line 311) and `build_response` (line 316) lifted from `pub(crate)` to `pub` for cross-crate consumption by envoy-http2's HCM. `HCMConfig` extended with `pub http2_protocol_options: Option<envoy_config::Http2ProtocolOptions>` (4th field); `HCMConfig::from_config` populates it from `cfg.http2_protocol_options.clone()`. Six in-test `HCMConfig { ... }` literal constructions in the existing `tests` module updated to include `http2_protocol_options: None,` (none of the 04.x tests exercise this field).
  - `crates/envoy-http1/src/lib.rs` — extended `pub use hcm::{HCM, HCMConfig};` to `pub use hcm::{BuildOutcome, HCM, HCMConfig, build_response};` (rustfmt-sorted).
  - `crates/envoy-config/src/bootstrap.rs` — added `Clone` to `Http2ProtocolOptions`'s derive list (prerequisite for the `cfg.http2_protocol_options.clone()` call inside `HCMConfig::from_config`).
  - `crates/envoy-cluster/src/cluster.rs` — added `pub fn empty() -> Self` to `impl ClusterManager` (test-shaped constructor consumed by envoy-http2's hcm.rs test fixtures; the runtime path still goes through `from_bootstrap`).
  - `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` — this section appended.
- **LoC:** ~430 raw lines (hcm.rs ~340 incl. tests; envoy-http1 visibility lift + HCMConfig extension + 7 test-literal updates ~12; envoy-http1/lib.rs re-export ~1; envoy-cluster `empty()` ~13 incl. doc; envoy-config Clone derive ~1; envoy-http2/lib.rs +2 lines after rustfmt; Cargo.toml +1 line). PLAN estimated ~440; actual matches within margin.
- **Verification:**

  Step 9.5 — `cargo test -p envoy-http2 -- --nocapture`:
  ```
  running 20 tests
  test hcm::tests::h2_protocol_options_max_concurrent_streams_applied ... ignored, h2-crate client-side observability of peer SETTINGS_MAX_CONCURRENT_STREAMS is not deterministically surfaced ...
  test codec::tests::build_h2_server_applies_protocol_options ... ok
  test error::tests::bad_status_code_displays_value ... ok
  test error::tests::missing_authority_displays_descriptively ... ok
  test error::tests::h2_handshake_displays_with_source ... ok
  test request::tests::http_to_envoy_request_lowercases_headers ... ok
  test request::tests::http_to_envoy_request_synthesizes_host_from_authority ... ok
  test request::tests::http_to_envoy_request_missing_authority_returns_error ... ok
  test request::tests::http_to_envoy_request_non_utf8_header_value_returns_error ... ok
  test response::tests::envoy_response_to_http2_strips_h2_forbidden_headers ... ok
  test response::tests::envoy_response_to_http2_preserves_status_and_body ... ok
  test response::tests::build_http_response_rejects_invalid_status_code ... ok
  test response::tests::build_http_response_rejects_invalid_header_name ... ok
  test hcm::tests::h2_handshake_completes_against_in_process_listener ... ok
  test hcm::tests::h2_get_resolves_to_direct_response_synth ... ok
  test hcm::tests::h2_authority_header_synthesizes_host_for_route_walk ... ok
  test hcm::tests::h2_two_requests_share_one_tcp_connection ... ok
  test hcm::tests::h2_response_strips_hop_by_hop_headers_defensively ... ok
  test hcm::tests::h2_proxy_outcome_returns_502_in_05_2 ... ok
  test hcm::tests::h2_handshake_fails_on_garbage_preamble ... ok

  test result: ok. 19 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s
  ```
  Suite goes 12 → 20 (Task 8 left it at 12; Task 9 adds 8: 7 PASS + 1 `#[ignore]`). Matches PLAN's Step 9.5 expected count.

  Per-test status:
  1. `h2_handshake_completes_against_in_process_listener` — PASS.
  2. `h2_get_resolves_to_direct_response_synth` — PASS.
  3. `h2_authority_header_synthesizes_host_for_route_walk` — PASS (specific VH wins over `*` catch-all).
  4. `h2_two_requests_share_one_tcp_connection` — PASS.
  5. `h2_response_strips_hop_by_hop_headers_defensively` — PASS (synth_direct_response emits `connection: keep-alive`; build_http_response strips it; client observes none of `connection`/`transfer-encoding`/`upgrade`/`keep-alive`/`proxy-connection`).
  6. `h2_proxy_outcome_returns_502_in_05_2` — PASS.
  7. `h2_handshake_fails_on_garbage_preamble` — PASS (1s timeout; observed `Ok(0)` peer-side after h2 codec rejects the preamble).
  8. `h2_protocol_options_max_concurrent_streams_applied` — `#[ignore]` per PLAN's escape clause (Step 9.5). The h2-0.4 client API does not expose peer SETTINGS_MAX_CONCURRENT_STREAMS in a deterministic way that lets a unit test assert the cap shape without racing the response loop. The codec-edge of the same setter is already covered in `codec.rs::build_h2_server_applies_protocol_options` (Task 8). The wire-effect verification will be picked up by the Docker-gated differential test in 05.2 Task 12 (`tests/differential/tests/http2_protocol_options.rs` — already in the PLAN at Task 12).

  Workspace gates:
  ```
  $ cargo build --workspace --all-targets 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.93s

  $ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.96s

  $ cargo fmt --all -- --check
  (empty — clean, after rustfmt's alpha-sort of the import + re-export lines was applied)

  $ cargo test --workspace 2>&1 | grep -E 'test result' | awk ...
  passed: 368 failed: 0 ignored: 2
  ```
  Workspace tests went from 339 (pre-Task-9 baseline carried in PLAN) + 12 (envoy-http2 pre-Task-9) = 351-ish to 368 passing across all crates: envoy-http1 stayed at 43 (no regression from the visibility lift / HCMConfig extension); envoy-http2 went 12 → 19 (passing) + 1 `#[ignore]`; no other crate's count moved.

- **Verified shapes (greps run at task time):**
  - `BuildOutcome` enum: 2 variants, `Synth(Response)` and `Proxy { cluster: String }` (PLAN's anticipatory 3-variant `Reject(Response)` reference is folded into `Synth` per PLAN's own settling note).
  - `RouteAction::Route(RouteAction_Route)` is the variant name (NOT `Cluster`); the PLAN's pseudo-code referenced `RouteAction::Route { cluster: "backend" }` which is the struct-shorthand misread — actual variant is tuple-style `RouteAction::Route(RouteAction_Route { cluster: ... })`. Test 6 uses the tuple shape correctly.
  - `Http2ProtocolOptions` was missing `Clone`; added per Step 9.1.6 instruction.
  - `ClusterManager::empty()` did not exist; added per Step 9.1.5 instruction.
  - `HCMConfig::from_config` body uses explicit field assignments (no `..Default::default()` shorthand); the new `http2_protocol_options` field needed an explicit `cfg.http2_protocol_options.clone()` line.

- **Deviations from PLAN:**
  1. **Test 8 (`h2_protocol_options_max_concurrent_streams_applied`) `#[ignore]`-marked** with a doctrine reason rather than failing the suite. PLAN explicitly permits this at Step 9.5 / the Step 9.3 elaboration guidance. The `#[ignore = "..."]` reason describes (a) the h2-crate observability gap and (b) the codec-side coverage already in place at codec.rs. Differential coverage of the same setter lands in Task 12.
  2. **rustfmt re-sorted `use envoy_http1::{...}` and the lib.rs `pub use hcm::{...}` blocks alphabetically** (`build_response` migrated to lowercase-after-uppercase). Same kind of cosmetic rustfmt nudge as Tasks 2/4/5/6/7/8. Functionally identical to PLAN's intent.
  3. **`envoy-cluster` added as `[dev-dependencies]` of `envoy-http2`** (not in the PLAN's file list). Required for the test fixture's `envoy_cluster::ClusterManager::empty()` call. Test-only; runtime dependency surface unchanged.

- **Carryforward note:** The HCM-on-H2 dispatch contract is now in place. `envoy-bin` Task 10 wires the new `envoy_http2::HCM` into the listener-walk site at `crates/envoy-bin/src/main.rs:207` HCM arm with H1-vs-H2 branching on `hcm_cfg.codec_type`. The `BuildOutcome::Proxy` 502 stub is the only surface that 05.3 D13.3 must replace; everything else (handshake, stream accept, route-walk, synth, header strip, body emit, hop-by-hop strip) is final-shape. Future H2 trailers emission, body forwarding for chunked-request-body, and upstream H2 origination all extend this module without disturbing the 05.2 contract.
- **Post-review fixup:** Four code-quality findings closed in a single fixup commit. (I1) Added struct-level doc-comment on `HCMConfig` explaining that the struct is shared across H1+H2 dispatch paths per cross-sub-phase architectural rule 2 (clarifies why `http2_protocol_options` lives in the envoy-http1 crate). (I3) Removed the redundant `tracing::warn!` in `serve_h2_connection`'s accept-error arm (the wrapped `Http2Error::H2StreamAccept` is already logged by the listener on return; double-logging eliminated). (M2) Introduced a `TestServer` RAII guard in `hcm::tests` that calls `JoinHandle::abort()` on drop, fixing the per-test listener-task leak. (M3) Marked `ClusterManager::empty()` `#[doc(hidden)]` to discourage production misuse while keeping it callable for test fixtures. Closes code-quality reviewer I1+I3+M2+M3 on Task 9. (I2 deferred per reviewer recommendation; M4/M5 acceptably deferred to 05.3 / future test-tightening.)

## Task 10 — `envoy-bin` HCM-on-H2 wiring + in-process integration test

- **Commit:** _(pending — set on commit; this task lands in a single commit per phase 05.2 PLAN convention)_
- **Deliverables:** Listener-walk dispatch wiring at `crates/envoy-bin/src/main.rs:207` HCM_FILTER arm gains H1-vs-H2 dispatch on `hcm_cfg.codec_type`. AUTO/HTTP1 continue through the existing `envoy_http1::HCM { config }` shape; HTTP2 routes to `envoy_http2::HCM::new(hcm_config)` (the same `Arc<envoy_http1::HCMConfig>` is consumed via the type-alias re-export from Task 9). HTTP3 is `unreachable!` (validator rejected at parse time per Task 2's accept-flip). The TLS-detect-and-bail (04.1 Task 11) is now gated by `matches!(codec_type, AUTO | HTTP1)` so the H2 path is never funneled into the H1-only `TlsAcceptingHandler`; the H2+TLS combination is already rejected upstream by the validator's `Http2OverTlsNotSupported` (Task 2). The `tracing::info!` at listener-bind time gains a `codec_type = ?hcm_cfg.codec_type` field for operability. Local in-process integration test `crates/envoy-bin/tests/http2_direct_response.rs` spawns `envoy-bin` via `CARGO_BIN_EXE_envoy-bin` against a minimal HCM-direct_response config with `codec_type: HTTP2`, drives a single `GET /` via `h2::client::handshake`, and asserts status 200 + body `"ok\n"`. `kill_on_drop(true)` posture per SPEC §6 local signpost 22.
- **ADR landed:** None (Task 10 directly applies the parent §6 signpost 22 dispatch contract + SPEC §6 local signposts 18 and 22; no new decisions).
- **Files modified:**
  - `crates/envoy-bin/Cargo.toml` — added `envoy-http2 = { path = "../envoy-http2" }` to `[dependencies]` (alphabetic insertion between `envoy-http1` and `envoy-listener`); added `bytes = "1"`, `h2 = "0.4"`, `http = "1"` to `[dev-dependencies]` (consumed by the new integration test).
  - `crates/envoy-bin/src/main.rs` — replaced the single-arm `Arc::new(envoy_http1::HCM { ... })` construction at line 222 with a `match hcm_cfg.codec_type { AUTO|HTTP1 => http1::HCM, HTTP2 => http2::HCM::new(hcm_config), HTTP3 => unreachable!() }`. Gated the TLS-detect-and-bail by `matches!(codec_type, AUTO | HTTP1)`. Added `codec_type` to the listener-bind `tracing::info!`. rustfmt rewrapped the `match` expression to break across `match hcm_cfg.codec_type {` (one extra indent level over the verbatim PLAN block, functionally identical).
  - `crates/envoy-bin/tests/http2_direct_response.rs` (created) — ~130 lines incl. doc-header + `reserve_port` / `wait_ready` helpers + the `#[tokio::test]` body. Verbatim from PLAN lines 2571-2703 with one tweak: dropped `mut` on the `child` binding (rustc/clippy `unused_mut` warning — `child` is consumed by the bare `drop(child)` SIGKILL line, never mutated).
  - `Cargo.lock` — auto-regenerated to reflect the new `envoy-bin → envoy-http2` runtime edge plus `bytes`, `h2`, `http` dev-edges (already present transitively via other workspace crates; no new versions resolved).
  - `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` — this section appended.
- **LoC:** ~165 raw lines across all files. envoy-bin/main.rs delta ~32 (match block + comment + matches! gate + codec_type tracing field). Cargo.toml +5 (1 dep + 3 dev-deps + alphabetic-insertion churn). Integration test ~130. Cargo.lock auto. PROGRESS section ~25. PLAN estimated ~160; actual ~165 within margin.
- **Verification:**

  Step 10.4 — `cargo test -p envoy-bin --test http2_direct_response -- --nocapture`:
  ```
  running 1 test
  test http2_direct_response_round_trip ... ok

  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.77s
  ```
  envoy-bin spawn → listener bind on 127.0.0.1:<reserved-port> → `wait_ready` connect-loop (~50ms first poll) → `h2::client::handshake` → `GET /` with `:authority: envoy-rust.test` → response status 200 + body `"ok\n"`. End-to-end round-trip via the new HCM-on-H2 dispatch arm.

  Step 10.5 — workspace gates:
  ```
  $ cargo build --workspace --all-targets 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.81s

  $ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -1
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.58s

  $ cargo fmt --all -- --check
  (empty — clean, after rustfmt's match-expression rewrap was applied)

  $ cargo test --workspace 2>&1 | grep -E '^test result:' | aggregate
  passed: 369 failed: 0 ignored: 2
  ```
  Workspace tests went from 368 (Task 9 baseline) to 369: the new `http2_direct_response_round_trip` integration test adds exactly one. envoy-http1 stayed at 43; envoy-http2 stayed at 19 + 1 ignored; no other crate's count moved. The H1 integration test (`http1_direct_response_round_trip`) continues to pass — confirms the `matches!(AUTO|HTTP1)` gate did not regress the existing TLS-detect-and-bail or the H1 dispatch arm.

- **Verified shapes (greps run at task time):**
  - `crates/envoy-bin/src/main.rs` line 207 HCM arm matched the PLAN's quoted shape exactly (variable names `hcm_cfg`, `hcm_config`, `bind_addr`, `cluster_mgr`, `token`, `set` — all match). The pre-existing TLS-detect-and-bail at lines 235-241 was unconditional; the new wiring gates it by `matches!(codec_type, AUTO | HTTP1)`.
  - `envoy_http2::HCM::new(Arc<HCMConfig>) -> Self` confirmed at `crates/envoy-http2/src/hcm.rs:36` (Task 9 contract).
  - `envoy_http2::HCMConfig` is a type alias for `envoy_http1::HCMConfig` per `crates/envoy-http2/src/hcm.rs:26`, so the same `Arc<envoy_http1::HCMConfig>` constructed at line 217 flows into both dispatch arms without any conversion.
  - `bytes`, `h2`, `http` were absent from envoy-bin's `[dev-dependencies]`; all three added (bytes for `BytesMut::new()`, h2 for `client::handshake`, http for `Request::builder`).
  - The `http1_direct_response.rs` integration test was the binary-locate / retry-loop pattern source; the new H2 test mirrors it (same `reserve_port`, `wait_ready`, `CARGO_BIN_EXE_envoy-bin`, `kill_on_drop(true)` shape).

- **Deviations from PLAN:**
  1. **`http = "1"` added to `[dev-dependencies]`** (not explicitly listed in PLAN's `Cargo.toml` step). PLAN named only `tempfile`, `bytes`, `anyhow`, `h2`. The integration test body uses `http::Request::builder()` directly — `h2` does not re-export `http`, so the dev-dep is mandatory for the test to compile. Same `http = "1"` version that envoy-http2 already pins in its own `[dependencies]`; no version skew.
  2. **`mut` removed from `let mut child = ...`** in the integration test — rustc emits `unused_mut` warning under the `[warn(unused)]` workspace lint, which `-D warnings` turns into an error. PLAN's verbatim block had `let mut child` because the H1 sibling test does explicit `child.kill().await` + `child.stderr.take()` for stderr-on-failure post-mortem; the H2 test instead uses bare `drop(child)` (SIGKILL via `kill_on_drop(true)`) with no method calls on `child`, so the `mut` is genuinely unused. Functionally identical to PLAN's intent.
  3. **rustfmt match-expression rewrap** in main.rs HCM arm. PLAN's verbatim block writes the match on one line (`let hcm: ... = match hcm_cfg.codec_type {`); rustfmt breaks it across two lines (`let hcm: ... =\n        match hcm_cfg.codec_type {`) when the type annotation pushes the line past the 100-column limit. Functionally identical; same kind of cosmetic rustfmt nudge as Tasks 2/4/5/6/7/8/9.

- **Carryforward note:** Phase 05.2 envoy-rust-only backstops are now complete. The HCM-on-H2 dispatch site is reachable end-to-end (envoy-bin → envoy-listener → envoy-http2::HCM → h2::server → route-walk → direct_response synth → h2 SendStream); the `BuildOutcome::Proxy` 502 stub remains the only surface that 05.3 D13.3 must replace. Task 11 picks up the gitignore allow-list entry for the new `e2e_http2_*.yaml` config-fuzz seeds + Task 12 lays in the Docker-gated differential equivalence test (`tests/differential/tests/http2_direct_response.rs`) against upstream Envoy.
- **Post-review fixup:** Two Important findings closed in a single fixup commit. (I1) Converted `CodecType::HTTP3 => unreachable!(...)` to `anyhow::bail!(...)` at the HCM dispatch site, matching the surrounding "validator should have rejected this" posture (six other `anyhow::bail!` sites in main.rs). A future validator regression now surfaces as a clean config-load error rather than a SIGABRT-style process panic. (I2) Added a defensive symmetric H2+TLS bail immediately after the existing H1 TLS-detect-and-bail. The envoy-config validator's `Http2OverTlsNotSupported` (Task 2) already rejects TLS+HTTP2 at parse time; this runtime guard exists so a future validator regression cannot silently bind a non-functional plaintext H2 listener on a port the operator expected to be TLS-protected. Closes code-quality reviewer I1+I2 on Task 10. (M1+M2 quality-of-life improvements deferred to a 05.3 follow-up if needed; M3-M6 awareness-only.)

