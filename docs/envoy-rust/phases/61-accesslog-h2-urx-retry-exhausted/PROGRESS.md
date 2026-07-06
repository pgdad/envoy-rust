# Phase 61 — Implementation Progress

> This is the §5 state-3 implementation session's running log. State-4
> verification (the full §7.5 gate, CI-authoritative for the Docker
> differential + h2spec) is a SEPARATE future session.

## Cold-start re-verification (before Task 1)

Re-confirmed every PLAN.md §3 PLAN-VERIFY citation against the live tree
(HEAD had moved to `9b53ab8` since the PLAN-write commit `3991f1c`, via an
unrelated sibling thread-per-core/io_uring perf merge that does not touch
`crates/envoy-http2/` or this phase's files):

- Locals block (`hcm.rs:536`-`545`): byte-exact match.
- Retry-loop post-loop split (`hcm.rs:810`-`825`, PLAN cited `:815`-`821`):
  byte-exact match (PLAN's own line-range was a sub-range of the block
  re-grepped; no drift in content).
- `finalize_h2_stream` call site (`hcm.rs:852`-`867`) + signature
  (`hcm.rs:881`-`908`): byte-exact match; confirmed exactly ONE call site.
- `%RESPONSE_FLAGS%` three-arm derive (`hcm.rs:985`-`990`): byte-exact
  match.
- Test insertion point: PLAN cited `hcm.rs:4474` (the closing brace of
  `h2_retry_limit_exceeded_path_always_503`) — live tree had it at `:4474`
  exactly (next doc-comment at `:4476`); zero drift.
- Differential harness lines (`tests/differential/src/lib.rs:3114`-`3120`
  allowlist, `:3179`-`3184` per-path arm): byte-exact match.
- Fixture `0067` confirmed still next-free (`0066` highest on disk).
- Baseline `cargo build -p envoy-http2 --tests` confirmed clean before any
  edit.

No drift found beyond what PLAN.md itself already anticipated. Proceeded
directly to Task 1.

## Task 1 — Retry-loop discriminator + threaded parameter + derive wrapper + in-process backstop (§A/§B/§C/§D/§G)

- **RED:** inserted `h2_retry_limit_exceeded_access_log_carries_urx_flag`
  immediately after `h2_retry_limit_exceeded_path_always_503`. Ran
  `cargo test -p envoy-http2 h2_retry_limit_exceeded_access_log_carries_urx_flag`
  — FAILED as expected: logged line
  `{"rc":503,"rcd":"via_upstream","rf":"-"}` (assertion expected
  `rf:"URX"`). Committed the RED test (`8f25f98`).
- **GREEN:** applied all four source edits — §A declare
  `retry_limit_exceeded_for_log_h2: bool = false` after the existing
  `*_for_log_h2` locals; §B set it `true` at the retry-loop's post-loop
  `final_retriable` limit-exceeded exit (same gate as the
  `upstream_rq_retry_limit_exceeded` counter); §C thread it as a new
  trailing parameter through `finalize_h2_stream`'s sole call site and
  signature; §D wrap the three-arm `%RESPONSE_FLAGS%` derive with an
  `if retry_limit_exceeded_for_log_h2 { "URX" } else { <existing match> }`.
  Re-ran the target test — PASSED
  (`{"rc":503,"rcd":"via_upstream","rf":"URX"}`).
- **Regression check:** `cargo test -p envoy-http2` — **85 passed, 0
  failed, 1 ignored.** Confirmed `h2_retry_limit_exceeded_path_always_503`
  (counters/headers/status) and every other retry/backstop test
  unaffected.
- Committed GREEN (`c1cad34`).

## Task 2 — Fixture `0067-accesslog-h2-urx-retry-exhausted`

Created all 4 files per PLAN.md's Step 1-4 content verbatim (`envoy.yaml`,
`envoy-rust.yaml`, `expectations.yaml`, `README.md`). Committed
(`14a76f3`).

**Follow-up fix (surfaced during Task 5 local verification, folded back
in as part of this task's scope):** local differential runs of the new
test found a REAL gap — `backend_cluster`'s `STRICT_DNS` resolution of
`{{BACKEND_HOST}}` could return an AAAA (IPv6) record unreachable from the
reference Envoy container on this host, producing a genuine
`upstream_reset_before_response_started{remote_connection_failure}` +
`rf:"URX,UF"` divergence instead of the expected
`via_upstream`/`rf:"URX"`. Fixture `0059` (the H1 `URX` precedent this
fixture is modeled on) already pins `dns_lookup_family: V4_ONLY` on its
`backend` cluster for exactly this reason (`envoy-config`'s
`DnsLookupFamily` schema, phase 05.4) — PLAN.md's Task 2 content dropped
this knob when crossing 0059's template with 0064-0066's H2C listener
shape. Added `dns_lookup_family: V4_ONLY` to `backend_cluster` in BOTH
`envoy.yaml` and `envoy-rust.yaml`. Confirmed fixed: 6 repeated local runs
post-fix show **zero byte-mismatches**; the only remaining local failures
are Docker container accept-ready timeouts (a pure container-startup
race, consistent with memory `differential-fixtures-flake-under-parallel-load`
— NOT a code or fixture defect). Committed (`3d739ae`).

## Task 3 — Differential test + harness backend-wiring (§E/§F)

Created `tests/differential/tests/access_log_h2_urx_retry_exhausted.rs`
(structural clone of `access_log_h2_rf_overflow.rs`, pointed at fixture
`0067`) and applied the 2-arm edit to `tests/differential/src/lib.rs`:
the `needs_health_aware_backend` allowlist gained the
`"0067-accesslog-h2-urx-retry-exhausted"` arm; the per-path
`/retry-exhausted=503` mapping gained the same fixture name alongside
`0059`. `cargo test -p differential --no-run` compiled clean. Committed
(`0a46aed`).

In-isolation local runs (`cargo test -p differential --test
access_log_h2_urx_retry_exhausted -- --test-threads=1`, repeated 6x after
the Task-2 V4_ONLY fix): 3 passed cleanly (byte-identical
`{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"via_upstream","rf":"URX"}`
on both sides), 3 failed on Docker accept-ready timeouts (host-environment
flake, not a byte-mismatch). Per PLAN.md's own Task 3 Step 4 note, the
Docker differential is CI-authoritative at state-4 — this local
inconsistency is not treated as a phase blocker.

## Task 4 — BEHAVIOR_CONTRACT.md updates (§H)

Updated the `%RESPONSE_FLAGS%` row's H2-witness sentence and the
`%RESPONSE_CODE_DETAILS%` row's H2-witness sentence per PLAN.md's exact
replace-blocks — both substrings located and confirmed unique before
editing. Committed (`a850468`).

## Task 5 — Local verification sweep

- `cargo clippy -p envoy-http2 -p differential --all-targets --all-features -- -D warnings`
  — clean, no warnings.
- `cargo fmt --all -- --check` — clean, no reflow (no fmt-fix commit
  needed).
- `cargo test --workspace --lib --bins` (the non-Docker-gated unit-test
  surface) — **every crate's `test result: ok`, 0 failed** across the
  full workspace (21 test binaries, hundreds of tests).
- Byte-preservation re-grep: `for f in 0009 0010 0018 0021 0064 0065
  0066; do grep -n retry_policy tests/fixtures/${f}-*/envoy-rust.yaml ||
  echo none; done` — **every fixture shows `(none)`**, re-confirming the
  additivity invariant holds.
- The full Docker-gated differential suite (`cargo test -p differential`
  under `--test-threads` default parallelism) is NOT locally
  deterministic on this host — this matches documented precedent (memory
  `differential-fixtures-flake-under-parallel-load`,
  `envoy-rust-state4-ci-first-execution`) and is NOT part of this
  session's local pre-flight per PLAN.md Task 5's own scope note. The
  Docker differential + h2spec + `0001`-`0066` all-green confirmation is
  CI-authoritative at state-4.

## Summary

All 5 PLAN.md tasks complete. One real defect surfaced and fixed during
local verification (missing `dns_lookup_family: V4_ONLY` on fixture
0067's cluster — a PLAN.md content gap, not an implementation bug) and
folded back into Task 2's commit history with its own follow-up commit.
No new ADR fired (no SPEC §A-§I item was overturned; the V4_ONLY fix is a
mechanical fixture correction matching pre-existing project convention,
not a new architectural decision). State-4 verification (the full §7.5
gate) is the next session.
