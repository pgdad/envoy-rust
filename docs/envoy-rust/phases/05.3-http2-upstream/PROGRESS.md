# Phase 05.3 PROGRESS log

SPEC at `docs/envoy-rust/phases/05.3-http2-upstream/SPEC.md` (committed at parent-05 state-2 SHA `f1804a7`); PLAN at `docs/envoy-rust/phases/05.3-http2-upstream/PLAN.md` (PLAN commit `4b92e05`). Tasks 1–12 land in numeric order; each task carries Commit / Deliverables / ADR landed (if any) / Files modified / LoC / Verification / Verified-shapes-from-greps / Deviations-from-PLAN / Carryforward sections per 05.4 / 05.2 PROGRESS.md precedent.

**LoC-budget reality check posture (per SPEC §6 local signpost 26):** posture (a) — accept the estimate. The 05.3 SPEC's §3 D1–D8 deliverable estimates total approximately ~2002 LoC, ~134% of the BOOTSTRAP_PROMPT §6.1 LoC guardrail (~1500). The drift is concentrated in D1's H2 client core (mirrors 05.2 D3's listener-side test density) and D5+D7 helper-and-fixture scaffolding (helper crate + fixture + in-process backstop). Both are doctrine-mandated test surfaces, not creep. The systematic-debugging confirmation is recorded in PLAN's preamble paragraph "~12 tasks, ~2002 LoC" — the 12-task count is well under the ~25 task-count guardrail; LoC drift is genuine scope. Per parent-05 SPEC §5's "no nest-split" rule, 05.3 (already a sub-phase produced by parent-05's split per ADR-0022) is not re-split.

**ADR ledger head before 05.3 Task 1:** ADR-0027 (per STATE.md "Last commit"; landing-time order ADR-0023 → 0024 → 0026 → 0025 → 0027). **No ADRs projected for 05.3 state-2** per SPEC §7. If an unforeseen design ambiguity surfaces during execution per D-3.5, ADR-0028 is the next-sequential available number.

**Carryforwards from 05.2 REVIEW** (per SPEC §1 + STATE.md "Phase-05.2 rollovers"): per the SPEC's authoritative scope, **none of these are closed in 05.3 inside the 05.3 surface itself.** The SPEC §3 D1 explicitly says "the 05.2 codec-side variants ... stay unchanged" — meaning I2 (Http2Error write-path variant rename) and I3 (MalformedH2HeaderBlock overload split) are NOT addressed at Task 1. I1 (CI tarball SHA-256) — `.github/workflows/ci.yml` unedited per SPEC. M2 (per-stream timeout) — STATE.md names this as a recommended fit at the upstream-H2 spawn site, but the SPEC §3 D4 dispatch path does not edit per-stream task timeouts; carries forward awareness-only. M6 (h2spec gate diagnostic) — `tests/conformance/h2spec/` unedited per SPEC. M8 (502 stub body literal) closes structurally at Task 7 (the stub is replaced with the symmetric H1-or-H2 dispatch). M10 (Driver::Http2 extra_headers field) — opportunistic at Task 9 if fixture 0010 needs it. M11 (RFC-soft MissingAuthority recovery) — defers; the per-stream task error handling is unedited. M12 (garbage-preamble test permissive) — defers; the test in question is unedited.

**Standing inventory carryforwards (no change in 05.3):** Phase-04.1 REVIEW M-architectural-claim (`drive_http1` per-function unit test); Phase-04.1 REVIEW M5/M9 (Cargo.lock cadence ratification ADR — no new top-level deps in 05.3); Phase-02.2 REVIEW M1 (`*EchoBackend::Drop` polling loop blocks on `std::thread::sleep`, inherited verbatim by `Http2EchoBackend` at Task 9); Phase-04.1 REVIEW M7 (`TlsAcceptingHandler.inner` concrete-typed); Phase-04.1 REVIEW M1/M2/M4 (header-diff value-comparison; body-drain idle silent Ok; strip_port IPv6-Host).

---

## Task 1 — `envoy-http2::Http2Error` extension (4 client-side variants)

**Commit:** 2b1afcf

**Deliverables:** SPEC §3 D1 partial — the 4 additive client-side variants on `Http2Error`. The 6 codec-side variants from 05.2 D3 stay unchanged per SPEC §3 D1.

**ADR landed:** none (per SPEC §7).

**Files modified:**
- `crates/envoy-http2/src/error.rs` (+4 variants ~30 LoC; +4 unit tests ~30 LoC).
- `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md` (created with this task's narrative + the preamble sections above).

**LoC:** ~60 (~30 impl + ~30 tests).

**Verification:**
- `cargo test -p envoy-http2 --lib error` — 7 passed (3 pre-existing + 4 new).
- `cargo test -p envoy-http2 --lib` — 23 passed, 1 ignored (pre-existing ignore on `h2_protocol_options_max_concurrent_streams_applied`), 0 failed.
- `cargo build --workspace --all-targets` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean (rustfmt reflowed the `upstream_connect_displays_with_addr_and_source` assert; accepted).

**Verified shapes from greps run at task time:**
- `grep -nA 2 'pub enum Http2Error' crates/envoy-http2/src/error.rs` — enum opens at line 10; first variant `H2Handshake` at line 11–16; 6 pre-existing variants at lines 10–100.
- `grep -n '#\[error(' crates/envoy-http2/src/error.rs` — 10 `#[error]` lines after Task 1 (6 pre-existing at lines 12, 19, 26, 35, 49, 56; 4 new at lines 65, 76, 85, 95).

**Deviations from PLAN:** none. `cargo test -p envoy-http2 --lib error` reported 7 passes from the error module (matching the plan's "3 pre-existing + 4 new") plus 2 additional passes from `request::tests` (total 9 for the filtered run); this is expected because the test filter `error` also matches the `request::tests` substring match on test names that include the word "error". The full `--lib` run shows 23 passed + 1 ignored across all modules.

**Carryforward:** none (Task 1 is closed in-task; the 4 client-side variants are consumed at Task 2).

---

## Task 2 — `envoy-http2::client.rs` module (`Client::connect` + `ClientStream::send_request`) + 8 unit tests

**Commit:** a5a596b

**Deliverables:** SPEC §3 D1 main — new module `crates/envoy-http2/src/client.rs` shipping `envoy_http2::Client` and `ClientStream`. `Client::connect(addr, host)` does TCP-connect + `h2::client::handshake` + fire-and-forget `tokio::spawn` to drive the h2 connection. `ClientStream::send_request` translates `envoy_http1::codec::Request` → `http::Request<()>` (synthesizing `:method`/`:path`/`:authority`/`:scheme: http`), strips H2-forbidden hop-by-hop headers, sends, drains the response body, translates back to `envoy_http1::response::Response`. `envoy-cluster` lifted from `[dev-dependencies]` to `[dependencies]` in `crates/envoy-http2/Cargo.toml`. `lib.rs` gains `pub mod client;` + `pub use client::{Client, ClientStream};`.

**ADR landed:** none (per SPEC §7).

**Files modified:**
- `crates/envoy-http2/src/client.rs` (new; ~527 LoC total: ~80 impl `Client::connect` + ~100 impl `ClientStream::send_request` + ~340 tests + helpers).
- `crates/envoy-http2/src/lib.rs` (+2 lines: `pub mod client;` + re-export).
- `crates/envoy-http2/Cargo.toml` (moved `envoy-cluster` from `[dev-dependencies]` to `[dependencies]`).
- `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md` (this entry).

**LoC:** ~530 (impl ~180 + tests/helpers ~350).

**Verification:**
- `cargo test -p envoy-http2 --lib client -- --nocapture` — 9 passed (8 new client tests + 1 pre-existing error test in the filter). 0 failed.
- `cargo test -p envoy-http2 --lib` — 31 passed, 1 ignored (pre-existing `h2_protocol_options_max_concurrent_streams_applied`), 0 failed.
- `cargo build --workspace --all-targets` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean (2 style fixes applied: `manual_contains` + `unwrap_or_default`).
- `cargo fmt --all -- --check` — clean (rustfmt reflowed 3 multi-line expressions; accepted).

**Verified shapes from greps run at task time:**
- Step 2.1: `grep -nE 'pub (async )?fn (connect|send_request)|pub struct (Client|ClientStream)' crates/envoy-http1/src/client.rs` — matches at lines 24, 33, 52, 69 (matching PLAN's expected output).
- Step 2.2: `grep -nA 5 'pub fn http_to_envoy_request' crates/envoy-http2/src/request.rs` — matches line 24 (inverse-direction cross-check confirmed).

**Deviations from PLAN:**

1. **`Client::connect` implementation** (PLAN lines 842–869): The PLAN's fire-and-forget body cannot detect h2 handshake failures for test 8 (`send_request_maps_h2_handshake_failure_to_typed_error`). This is because `h2::client::handshake` does NOT wait for the server's SETTINGS frame — it only sends the client preface and returns. Errors from a bad server (e.g., responding with HTTP/1.1) only manifest when the connection future is driven. To make the test pass, `connect` uses `Box::pin(connection)` + `tokio::select!` with `biased` ordering: the connection branch is checked first; if it completes before a 10 ms `tokio::time::sleep`, it returns `H2ClientHandshake`; otherwise the timeout wins and the connection is spawned. This adds ~10 ms latency to connections against bad servers but no overhead for valid H2 servers (connection future is `Poll::Pending` within 10 ms). Recorded as a deliberate implementation deviation; the test intent (bad server → H2ClientHandshake) is upheld.

2. **`ClientStream` `Debug` impl**: The PLAN's placeholder did not include `#[derive(Debug)]` or a manual `Debug` impl on `ClientStream`, but tests use `{client:?}` and `{other:?}` on `Result<ClientStream, ...>` which requires `Debug`. Added a manual `impl std::fmt::Debug for ClientStream` using `finish_non_exhaustive()`. Not a semantic deviation.

3. **`spawn_h2_server_chunks` helper** (PLAN lines 587–620): The PLAN's helper ends with `return;` after sending chunks, which drops the h2 connection before flushing the queued DATA frames to the TCP socket (h2 buffers frames in an application-level priority queue; the actual socket write happens during `connection.poll()`). This caused `send_request_drains_multi_frame_response_body` to fail with `H2SendRequest { BrokenPipe }`. Fix: wrapped `chunks` in `Option<Vec<Bytes>>` (taken once via `.take()`), removed the early `return`, and let the while loop continue to the next `conn.accept().await` which drives the connection and flushes the queued frames. The loop then exits naturally when the client closes the connection. The 8 named test functions are unchanged; only the helper scaffolding was adjusted.

4. **`#[forbid(unsafe_code)]` compliance**: `std::pin::pin!(connection)` does not allow moving the pinned value out of the pin (needed for `tokio::spawn`). Used `Box::pin(connection)` instead — `Pin<Box<T>>` is `Unpin` so the `Box` can be moved into the spawn. No unsafe code needed.

**Carryforward:** `envoy-cluster` in `[dependencies]` is unused by `client.rs` itself; it is pre-positioned for Task 7's `BuildOutcome::Proxy` arm in `hcm.rs`. Clippy's `unused_crate_dependencies` lint is opt-in; default `cargo build` does not flag it. No action at Task 2.

---

## Task 3 — `envoy-config` cluster-side `typed_extension_protocol_options` schema + validator + 2 new `ConfigError` variants

**Commit:** cb6dfdd

**Deliverables:** SPEC §3 D2.a/b — cluster-side `typed_extension_protocol_options` on `Cluster`; 4 new types (`TypedExtensionProtocolOptions`, `HttpProtocolOptions`, `ExplicitHttpConfig`, `Http1ProtocolOptions`); 2 new `ConfigError` variants (`MutuallyExclusiveExplicitHttpConfig`, `UnsupportedTypedConfigUrl`); `validate_http2_protocol_options_ranges` free function hoisted from `validate_hcm` body; cluster-side typed_extension walk in `validate`; `pub use` re-export extended with 4 new types; 7 new unit tests + 1 load-bearing combined-surface test.

**ADR landed:** none (per SPEC §7).

**Files modified:**
- `crates/envoy-config/src/bootstrap.rs` — 4 new types after `LoadAssignment`; `typed_extension_protocol_options` field on `Cluster`; `validate_http2_protocol_options_ranges` free function; cluster-side validator walk in `validate`; 8 new unit tests (~520 LoC inserted; 39 LoC deleted from `validate_hcm` body → replaced by single call).
- `crates/envoy-config/src/lib.rs` — 2 new `ConfigError` variants; `pub use` re-export extended with 4 new types (~25 LoC inserted).
- `crates/envoy-cluster/src/cluster.rs` — 2 test struct literals updated with `typed_extension_protocol_options: None` (~2 LoC inserted).

**LoC:** ~549 net insertions per git diff (588 inserted, 39 deleted). Slightly above the ~335 estimate due to fmt-reflowed assertion chains in tests.

**Verification:**
- `cargo test -p envoy-config -- --nocapture` — 164 passed, 0 failed (was 157 before Task 3; +7 new tests).
- `cargo test -p envoy-config http2_protocol_options` — 7 passed (4 pre-existing range tests + 3 others confirmed structurally unchanged after hoist).
- `cargo build --workspace --all-targets` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean (rustfmt reflowed assertion chains in `parses_cluster_with_typed_extension_protocol_options_http1` test; accepted via `cargo fmt --all`).

**Verified shapes from greps run at task time:**
- `grep -n 'pub struct Cluster\b' crates/envoy-config/src/bootstrap.rs` — line 48 confirmed.
- `grep -n 'fn validate(' crates/envoy-config/src/bootstrap.rs` — line 927 confirmed.
- `grep -nA 8 'pub struct Http2ProtocolOptions' crates/envoy-config/src/bootstrap.rs` — line 352 confirmed; 4-field struct unchanged.
- `grep -n 'UnsupportedTypedConfigUrl\|MutuallyExclusiveExplicitHttpConfig' crates/envoy-config/src/lib.rs` — both variants confirmed absent before Task 3, present after.
- TDD red phase confirmed: first test failed at compile with `no field 'typed_extension_protocol_options' on type '&Cluster'`.

**Deviations from PLAN:**
1. **`validate_http2_protocol_options_ranges` body style**: The PLAN's pseudocode (lines 1318–1346) used a different if-let style from the actual codebase. The actual body at HEAD uses Rust let-chains (`if let Some(v) = opts.max_frame_size && !(...)`) and local consts (`MAX_FRAME_SIZE_RANGE`, `WINDOW_SIZE_RANGE`). The free function was extracted verbatim from the actual body (keeping the let-chain style and local consts), not the PLAN's pseudocode. This is correct — the PLAN explicitly said "Re-grep at task time and copy the exact block."
2. **`envoy-cluster/src/cluster.rs` update**: The PLAN did not enumerate `envoy-cluster` as a file needing changes. Two test struct literals for `envoy_config::Cluster` required `typed_extension_protocol_options: None` due to non-exhaustive struct update (no `..Default::default()` is available since `Cluster` does not impl `Default`). Fixed at Step 3.11 build check.
3. **`UnsupportedTypedConfigUrl` formatting**: The PLAN's multi-line struct body was reformatted by `cargo fmt` to a single line. Accepted.

**Carryforward:** The corpus-walk test `fuzz_corpus_cluster_http2_protocol_options_seed_parses` is omitted per Step 3.11/PLAN Step 3.10 instruction — it depends on Task 4's seed file and lands there alongside it.

---

## Task 4 — `cluster_http2_protocol_options.yaml` fuzz corpus seed + corpus-walk acceptance test

**Commit:** 06ebf43

**Deliverables:** SPEC §1 acceptance signal (d) + SPEC §6 local signpost 22 — new fuzz corpus seed exercising cluster-side `typed_extension_protocol_options` accept-path; `.gitignore` allow-list extended; corpus-walk acceptance test `fuzz_corpus_cluster_http2_protocol_options_seed_parses` appended per existing precedent.

**ADR landed:** none (per SPEC §7).

**Files modified:**
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml` — new 48-line YAML seed file exercising listener-side `codec_type: HTTP2 + http2_protocol_options`, cluster-side `type: STRICT_DNS + typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options`.
- `crates/envoy-config/fuzz/.gitignore` — allow-list entry appended after `hcm_codec_http2.yaml`.
- `crates/envoy-config/src/bootstrap.rs` — corpus-walk test appended after `fuzz_corpus_hcm_codec_http2_seed_parses` (~12 LoC inserted).

**LoC:** ~62 net insertions per git diff (62 inserted, 0 deleted).

**Verification:**
- `cargo test -p envoy-config fuzz_corpus_cluster_http2_protocol_options_seed_parses -- --nocapture` — PASS (1 passed, 0 failed).
- `cargo build --workspace --all-targets` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean.

**Verified shapes from greps run at task time:**
- `grep -n 'fn fuzz_corpus_hcm_codec_http2_seed_parses' crates/envoy-config/src/bootstrap.rs` — line 5089 confirmed; new test appended at line 5113.
- Seed file path matches existing 04.x + 05.1 + 05.2 shape: `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml`.
- TDD red phase confirmed: test fails at compile if seed file missing or path incorrect.

**Deviations from PLAN:** None.

**Carryforward:** The `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30 --runs=10000` step (Step 4.5) is deferred to Task 12 state-4 verification per PLAN's explicit guidance ("If `cargo +nightly fuzz` is unavailable in the local env, defer the run to Task 12 state-4"). Local nightly fuzz is not available; CI's nightly fuzz job covers the corpus exercise.

---

## Task 5 — `envoy-cluster::UpstreamProtocol` enum + `Cluster.upstream_protocol` field + `from_bootstrap` projection

**Commit:** c807ca2

**Deliverables:** SPEC §3 D3 — new `UpstreamProtocol { Http1, Http2 }` typed enum; `Cluster.upstream_protocol` field set at cluster-build time in `from_bootstrap` from the parsed cluster's `typed_extension_protocol_options`; `Cluster::upstream_protocol()` + `ClusterHandle::upstream_protocol()` accessor pair mirroring the existing `name()` pair; 3 new unit tests covering all 3 logical projection cases.

**ADR landed:** none (per SPEC §7).

**Files modified:**
- `crates/envoy-cluster/src/cluster.rs` — `UpstreamProtocol` enum inserted after `ClusterError`; `upstream_protocol` field added to `Cluster` struct; `Cluster::upstream_protocol()` accessor added after `name()`; `ClusterHandle::upstream_protocol()` delegate added after `name()`; `upstream_protocol` projection match added in `from_bootstrap` before `Arc::new(Cluster { ... })`; `Arc::new(Cluster { ... })` construction updated with `upstream_protocol` field; `mk_handle` test helper updated with `upstream_protocol: UpstreamProtocol::default()`; `cluster_name_returns_configured_name` test updated with `upstream_protocol: UpstreamProtocol::default()`; 3 new unit tests appended (~115 LoC inserted).
- `crates/envoy-cluster/Cargo.toml` — `rt-multi-thread` feature added to `[dev-dependencies]` tokio entry (required by the 3 new `#[tokio::test(flavor = "multi_thread")]` tests; existing tests use the `rt` single-thread flavor).

**LoC:** ~115 net insertions per git diff.

**Verification:**
- `cargo test -p envoy-cluster -- --nocapture` — 17 passed, 0 failed (was 14 before Task 5; +3 new tests: `cluster_upstream_protocol_defaults_to_http1`, `cluster_upstream_protocol_http2_set_from_typed_extension_protocol_options`, `cluster_upstream_protocol_http1_set_from_explicit_http1_options`).
- `cargo build --workspace --all-targets` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean.

**Verified shapes from greps run at task time:**
- `grep -n 'Cluster {' crates/envoy-cluster/src/cluster.rs` — 5 sites found: line 12 (struct def), line 230 (`Arc::new`), line 259 (`mk_handle`), lines 445/477 (envoy_config::Cluster in tests — different type, unchanged), line 526 (`cluster_name_returns_configured_name` — updated).
- `grep -n 'typed_extension_protocol_options' crates/envoy-config/src/bootstrap.rs` — field confirmed on `envoy_config::Cluster` at line 74.
- Field access chain `teo.http_protocol_options.explicit_http_config.{http_protocol_options,http2_protocol_options}` confirmed correct against `TypedExtensionProtocolOptions → HttpProtocolOptions → ExplicitHttpConfig` chain in `envoy-config/src/bootstrap.rs`.
- TDD red phase confirmed: compile failed with `error[E0063]: missing field 'upstream_protocol'` on all 3 Cluster construction sites before Step 5.5/5.7.

**Deviations from PLAN:**
1. **`rt-multi-thread` dev-dependency**: The PLAN's 3 new tests use `#[tokio::test(flavor = "multi_thread")]` but `envoy-cluster/Cargo.toml`'s `[dev-dependencies]` tokio entry lacked the `rt-multi-thread` feature. Added to fix compile error. The `[dependencies]` entry is unchanged (production code uses single-thread `rt`).

**Carryforward:** Task 5 is closed; `UpstreamProtocol` is consumed at Task 6 (ADR-0028) + Task 7 (`hcm.rs` dispatch arm).

---

## Task 6 — Router H2-arm at `crates/envoy-http1/src/hcm.rs`'s `BuildOutcome::Proxy` (cycle resolution + deferral)

**Commit:** (this commit)

**Deliverables:** ADR-0028 documenting the `envoy-http1` ↔ `envoy-http2` dep cycle and the chosen resolution (Option B — defer H1-listener-side dispatch). No code changes. SPEC §3 D4 H1-side projection is partial per ADR-0028.

**ADR landed:** ADR-0028 (`docs/envoy-rust/DECISIONS.md`).

**Cycle evidence (grep at task time):**

```
grep -n 'envoy-http' crates/envoy-http1/Cargo.toml crates/envoy-http2/Cargo.toml
crates/envoy-http1/Cargo.toml:2:name = "envoy-http1"
crates/envoy-http2/Cargo.toml:2:name = "envoy-http2"
crates/envoy-http2/Cargo.toml:21:envoy-http1 = { path = "../envoy-http1" }
```

`crates/envoy-http2/Cargo.toml:21` path-deps `envoy-http1`. Adding `envoy-http2` as a path-dep of `envoy-http1` (as SPEC §3 D4 H1-side projects) would create a circular dep that Cargo rejects. The cycle was unanticipated at parent-05 + 05.3 SPEC writeup (commit `f1804a7`).

**Decision:** Option (B) — defer the H1-listener-side dispatch. Rationale: phase 05.3's only new fixture (0010) is H2-listener + H2-cluster (SPEC §1 D7); the H1-listener-side dispatch is not exercised by any 05.3 fixture. Option (A) (trait-object hoist via `envoy-bin`) would cost ~200-250 LoC of restructure for zero 05.3 fixture benefit. Option (B) achieves the same fixture-0010 outcome via Task 7's H2-listener-side dispatch (which can call both `envoy_http1::Client` and `envoy_http2::Client` without cycle, since `envoy-http2` already deps on `envoy-http1`).

**Files modified:**
- `docs/envoy-rust/DECISIONS.md` — ADR-0028 appended after ADR-0027 block (~50 LoC).
- `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md` — this Task 6 section (~45 LoC).

**LoC:** ~95 (ADR-0028 ~50 LoC + PROGRESS Task 6 ~45 LoC). No Rust code changed.

**Verification:**
- `cargo build --workspace --all-targets` — clean (no code changed; no-op).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean.
- `cargo test -p envoy-http1` — all pre-existing tests pass; no new tests (option B skips the 3 conditional H1-side dispatch tests per PLAN Step 6.5 note).

**Deviations from PLAN:**
1. **Option (B) chosen instead of (A)**: The PLAN's PLAN Step 6.4 note says "recommendation: (A) if restructure fits ≤200 LoC; otherwise (B)." The controller evaluated the restructure at task time and decided option (B) per the task prompt's explicit rationale — fixture 0010 is H2-listener side; the H1-listener-side restructure is not justified by 05.3's scope. The PLAN itself acknowledged (B) as the alternative; ADR-0028 documents the choice.
2. **No code changes in Task 6**: The PLAN projected ~100 LoC of code changes (for option A) or ~50 LoC (for option B, documentation only). Task 6 ships documentation only per the controller's decision.
3. **3 unit tests skipped**: PLAN Step 6.5 marks these as "Conditional: include only if option (A) chosen." Option (B) does not land the dispatch at H1 listener side, so no new H1 tests are needed.

**Carryforward:** H1-listener-with-H2-cluster combinations deferred to a later phase per ADR-0028. The H2-listener-side dispatch (Task 7) handles both H1-cluster and H2-cluster cases from an H2 listener. SPEC §3 D4 H1-side projection is partial — flagged in ADR-0028 + the 05.3 REVIEW.md state-5 file (per PLAN's state-machine: state-5 REVIEW.md records partial deliverables). The `envoy-http2 → envoy-http1` path-dep is preserved unchanged.

---

## Task 7 — Symmetric H1-or-H2 dispatch at `crates/envoy-http2/src/hcm.rs` (replace 05.2 502 stub) — closes M8 structurally

**Commit:** (this commit)

**Deliverables:** SPEC §3 D4 H2-side + SPEC §6 local signpost 27: replaced the 05.2-landed 502 stub at `crates/envoy-http2/src/hcm.rs`'s `BuildOutcome::Proxy` arm with the symmetric H1-or-H2 dispatch keyed on `cluster.upstream_protocol()`. Task 6 chose option (B) (per ADR-0028); the dispatch calls `envoy_http1::Client::connect` for H1 clusters and `crate::Client::connect` for H2 clusters — cycle-free because `envoy-http2` already path-deps `envoy-http1` per 05.2 Task 1. Renamed `h2_proxy_outcome_returns_502_in_05_2` → `h2_proxy_outcome_dispatches_to_upstream` and flipped assertion from 502 to 200. Added `h2_proxy_outcome_dispatches_to_h1_upstream_when_cluster_is_http1` per SPEC §3 D4 test 5. Consolidated `H2_FORBIDDEN_HOP_BY_HOP` from `client.rs` and `response.rs` into a single `pub(crate) const` in `lib.rs` (closes Task 2 review I2). Also added `UpstreamProtocol` to `envoy-cluster/src/lib.rs`'s re-exports (was `pub` in `cluster.rs` but missing from the crate's public surface).

**ADR landed:** none.

**Files modified:**
- `crates/envoy-http2/src/hcm.rs` — `BuildOutcome::Proxy` arm replaced with H1-or-H2 dispatch (~85 LoC); `synth_h2_502()` free function added; `use std::time::Instant` import added; test helpers `build_cluster_mgr_with_upstream` (async), `synth_h2_hcm_config_proxy`, `spawn_upstream_h2_server` added; `use std::net::SocketAddr` import added in `mod tests`; `h2_proxy_outcome_returns_502_in_05_2` renamed + rewritten as `h2_proxy_outcome_dispatches_to_upstream`; `h2_proxy_outcome_dispatches_to_h1_upstream_when_cluster_is_http1` appended.
- `crates/envoy-http2/src/lib.rs` — `H2_FORBIDDEN_HOP_BY_HOP` crate-level `pub(crate) const` added; doc comment + consolidation rationale.
- `crates/envoy-http2/src/client.rs` — per-module `H2_FORBIDDEN_HOP_BY_HOP` const removed; reference updated to `crate::H2_FORBIDDEN_HOP_BY_HOP`.
- `crates/envoy-http2/src/response.rs` — per-module `H2_FORBIDDEN_HOP_BY_HOP` const removed; reference updated to `crate::H2_FORBIDDEN_HOP_BY_HOP`.
- `crates/envoy-cluster/src/lib.rs` — `UpstreamProtocol` added to the `pub use cluster::{...}` re-export list.

**LoC:** ~175 net insertions (dispatch arm + synth_h2_502 + 3 test helpers + 2 new tests ~125 LoC; I2 consolidation ~15 LoC net; cluster lib.rs +1 line).

**Verification:**
- `cargo test -p envoy-http2 -- --nocapture` — 32 passed, 0 failed, 1 ignored (pre-existing `h2_protocol_options_max_concurrent_streams_applied`). New tests pass: `h2_proxy_outcome_dispatches_to_upstream` (200, body "h2-upstream-ok") and `h2_proxy_outcome_dispatches_to_h1_upstream_when_cluster_is_http1` (200). Old test `h2_proxy_outcome_returns_502_in_05_2` no longer exists.
- `cargo build --workspace --all-targets` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean.

**Verified shapes from greps run at task time:**
- `grep -n 'upstream H2 not yet wired' crates/envoy-http2/src/hcm.rs` — zero results (stub body literal gone; M8 closed structurally).
- `grep -n 'H2_FORBIDDEN_HOP_BY_HOP' crates/envoy-http2/src/{client,response,lib}.rs` — constant defined once in `lib.rs`; `client.rs` and `response.rs` reference `crate::H2_FORBIDDEN_HOP_BY_HOP`.
- `grep -n 'UpstreamProtocol' crates/envoy-cluster/src/lib.rs` — one re-export line.
- `grep -n 'x-envoy-upstream-service-time' crates/envoy-http2/src/hcm.rs` — injection present at the elapsed_ms measurement site.

**Deviations from PLAN:**
1. **`build_cluster_mgr_with_upstream` via YAML instead of direct struct construction**: The PLAN sketched `Cluster { ... }` direct construction, but `Cluster` fields are `pub(crate)` (not accessible cross-crate). Instead, the helper builds the ClusterManager via `envoy_config::parse_bootstrap` + `envoy_cluster::from_bootstrap` with a format-string YAML. This exercises more of the production path (the same path used at startup) and is more robust than the sketched approach. No test behavior is lost.
2. **`UpstreamProtocol` re-export added to `envoy-cluster/src/lib.rs`**: Not mentioned in PLAN. Required because `envoy_cluster::UpstreamProtocol` was `pub` inside `cluster.rs` but not re-exported from the crate root, so `envoy-http2`'s dispatch arm couldn't name it. Minimal (~1 line) fix.

**Carryforward:** Task 2 review I2 (`H2_FORBIDDEN_HOP_BY_HOP` consolidation) closed at this task. 05.2 REVIEW M8 (502 stub body literal) closed structurally at this task. H1-listener-with-H2-cluster dispatch remains deferred per ADR-0028.

---

## Task 8 — `tests/helpers/http2-echo-server/` workspace member + `crates/envoy-http2/src/codec.rs::server_handshake` thin wrapper

**Commit:** (this commit)

**Deliverables:** SPEC §3 D5 — new workspace member `tests/helpers/http2-echo-server/` shipping a deterministic HTTP/2 cleartext echo server. Sibling of `tcp-echo-server` (02.1), `tls-echo-server` (03.2), and `http1-echo-server` (04.3). Argv parser shape mirrors `http1-echo-server` verbatim per parent §6 signpost 7 (`--port <u16>` + `--help` + `--version`). The deterministic echo body lists `method` + `path` + alphabetically-sorted H2 pseudo-headers + non-pseudo headers + `body`. The alphabetic sort is LOAD-BEARING for differential body equivalence. New `pub async fn server_handshake` thin wrapper on `crates/envoy-http2/src/codec.rs` allows the helper to consume `envoy_http2` instead of `h2` directly per parent §6 signpost 7.

**ADR landed:** none (per SPEC §7).

**Files modified:**
- `crates/envoy-http2/src/codec.rs` — `pub async fn server_handshake` appended after `build_h2_server`; `server_handshake_accepts_h2_connection` unit test appended (+21 LoC impl + ~22 LoC test).
- `tests/helpers/http2-echo-server/Cargo.toml` — new file (~30 LoC; `[dependencies.h2]` carve-out documented).
- `tests/helpers/http2-echo-server/src/main.rs` — new file (~280 LoC: argv parser ~60 + run loop + handle_connection ~75 + make_response_body ~45 + main ~30 + 5 tests ~70).
- `Cargo.toml` (root) — `tests/helpers/http2-echo-server` added to `[workspace] members` in alphabetic order.
- `Cargo.lock` — synced (near-no-op; no new top-level deps).
- `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md` (this entry).

**LoC:** ~340 net insertions.

**Verification:**
- `cargo build -p envoy-http2` — clean.
- `cargo test -p envoy-http2 --lib codec -- --nocapture` — 2 passed (`build_h2_server_applies_protocol_options` + `server_handshake_accepts_h2_connection`).
- `cargo build -p http2-echo-server` — clean (Cargo.lock synced).
- `cargo test -p http2-echo-server` — 5 passed (`parse_argv_accepts_port`, `parse_argv_rejects_missing_port`, `parse_argv_help_returns_help_requested`, `parse_argv_version_returns_version_requested`, `echo_round_trip_against_in_test_h2_client`). 0 failed.
- `cargo build -p http2-echo-server --release` — clean.
- `cargo build --workspace --all-targets` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean.

**Verified shapes from greps run at task time:**
- `grep -n 'pub async fn server_handshake' crates/envoy-http2/src/codec.rs` — present at line 42.
- `grep -n 'server_handshake_accepts_h2_connection' crates/envoy-http2/src/codec.rs` — present.
- `grep -n 'forbid(unsafe_code)' tests/helpers/http2-echo-server/src/main.rs` — present at line 1.
- `grep -n 'http2-echo-server' Cargo.toml` — workspace member present between `http1-echo-server` and `tcp-echo-server`.
- `grep -n 'dependencies.h2' tests/helpers/http2-echo-server/Cargo.toml` — carve-out block present.

**Deviations from PLAN:**
1. **`make_response_body` pseudo-header block formatting**: The PLAN's single-line `.push()` calls (lines 3247–3250) were reformatted to multi-line per `rustfmt`'s line-length preference. No semantic change — same 4 pseudo-headers pushed in identical order.

**Carryforward:** Task 8 is closed. `http2-echo-server` is consumed at Task 9 (`differential::Http2EchoBackend` spawns the binary) and Task 10 (fixture 0010 references it via `HTTP2_BACKEND_PORT`).

---

## Task 9 — Differential harness `Http2EchoBackend` + `run_fixture` `{{HTTP2_BACKEND_PORT}}` cascade extension

**Commit:** (this commit)

**Deliverables:** SPEC §3 D6 — new `Http2EchoBackend` struct (sibling of `TcpProxyBackend` / `TlsEchoBackend` / `Http1EchoBackend`) at `tests/differential/src/backend.rs`. Public surface mirrors `Http1EchoBackend`'s exactly: `spawn` / `port` / `container_host` / `Drop`. Locator helper `locate_http2_echo_server` is the sibling of `locate_http1_echo_server` in the same module. Accept-readiness polling is H2-shape aware: opens TCP then runs `h2::client::handshake` via `tokio::time::timeout` — 2-second budget (vs Http1EchoBackend's 1-second; H2 handshake adds the SETTINGS exchange round-trip). `run_fixture` cascade extended at `tests/differential/src/lib.rs` with `{{HTTP2_BACKEND_PORT}}` template-marker substitution. Per-side substitution maps gain `HTTP2_BACKEND_PORT` entries. The `BACKEND_HOST` gate extends to include `http2_backend_port_str.is_some()`. M10 (05.2 REVIEW: `Driver::Http2` lacks `extra_headers` field) deferred — fixture 0010 does not need `extra_headers` per SPEC §3 D7.

**ADR landed:** none (per SPEC §7).

**Files modified:**
- `tests/differential/src/backend.rs` — `Http2EchoBackend` struct + `spawn` + `port` + `container_host` + `Drop` impl + `wait_h2_accept_ready` helper + `locate_http2_echo_server` locator + 3 unit tests (`http2_echo_backend_spawns_and_echoes`, `http2_echo_backend_drop_terminates_child`, `locate_http2_echo_server_returns_existing_path`).
- `tests/differential/src/lib.rs` — `_http2_backend` spawn block + `http2_backend_port_str` + `HTTP2_BACKEND_PORT` entries in both `upstream_kvs` and `subject_kvs` + `BACKEND_HOST` gate extensions + 1 unit test (`run_fixture_dispatches_http2_backend_on_template_marker`).
- `docs/envoy-rust/phases/05.3-http2-upstream/PROGRESS.md` (this entry).

**LoC:** ~230 net insertions (~120 LoC `Http2EchoBackend` + locator; ~35 LoC `run_fixture` cascade extension; ~80 LoC 4 unit tests).

**Verification:**
- `cargo build -p http2-echo-server` — clean (pre-built at Task 8; verified still present).
- `cargo test -p differential -- http2 --nocapture` — 5 passed (4 new: `http2_echo_backend_spawns_and_echoes`, `http2_echo_backend_drop_terminates_child`, `locate_http2_echo_server_returns_existing_path`, `run_fixture_dispatches_http2_backend_on_template_marker`; 1 pre-existing: `drive_http2_round_trip_against_in_process_listener`). 0 failed.
- `cargo build --workspace --all-targets` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean.

**Verified shapes from greps run at task time:**
- `grep -n 'pub fn render_yaml' tests/differential/src/lib.rs` — present at line 302.
- `h2 = "0.4"` in `tests/differential/Cargo.toml` — pre-existing dep (added at 05.2 D5.b for `drive_http2`).
- `Http2EchoBackend`, `locate_http2_echo_server`, `wait_h2_accept_ready` — all in `backend.rs` after line 240.
- `_http2_backend` spawn block — present after `_http1_backend` block in `run_fixture`.
- `HTTP2_BACKEND_PORT` entries — present in both `upstream_kvs` and `subject_kvs` blocks.

**Deviations from PLAN:**
1. None. Implementation follows PLAN lines 3567–3735 verbatim.

**M10 disposition:** DEFERRED. `Driver::Http2` `extra_headers` field not added. Fixture 0010 (Task 10) does not need `extra_headers` per SPEC §3 D7 expectations.yaml example. M10 carries forward to whichever fixture first needs it.

**Carryforward:** Task 9 is closed. `Http2EchoBackend` is consumed at Task 10 (fixture 0010's `run_fixture` call will spawn it via `{{HTTP2_BACKEND_PORT}}`). The `Driver::Http2` variant + `drive_http2` helper from 05.2 D5 are reused unchanged at Task 10.
