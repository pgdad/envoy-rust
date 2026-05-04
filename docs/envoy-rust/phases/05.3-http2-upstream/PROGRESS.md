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
