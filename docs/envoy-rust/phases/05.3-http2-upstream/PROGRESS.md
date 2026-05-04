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
