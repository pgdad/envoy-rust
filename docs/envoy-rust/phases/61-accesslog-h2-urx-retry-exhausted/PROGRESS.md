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

## State-4 verification

`superpowers:verification-before-completion`, the full §7.5 gate. Cold-started
at `HEAD` `4bc1589` (unchanged since the state-3 session; `git fetch origin
--prune` showed no divergence). Re-confirmed on disk: `docs/envoy-rust/phases/61-accesslog-h2-urx-retry-exhausted/PROGRESS.md`
carried all 5 tasks; ROADMAP row `61` `in-progress`; `DECISIONS.md` ADR-0118
matches the state-1 pick; `BEHAVIOR_CONTRACT.md`'s `%RESPONSE_FLAGS%` and
`%RESPONSE_CODE_DETAILS%` rows already carry the H2 `URX`/fixture-`0067`
witness sentences from Task 4.

### Local runs (this session, `HEAD` `4bc1589`)

**`cargo build --workspace --all-targets`** — clean:

```
    Compiling http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.41s
```

**`cargo clippy --workspace --all-targets --all-features -- -D warnings`** — clean, 0 warnings:

```
    Checking envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
    Checking differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Checking http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Checking envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.08s
```

**`cargo fmt --all -- --check`** — clean, exit 0, no output (no reflow needed).

**`cargo deny check`** — clean: `advisories ok, bans ok, licenses ok, sources ok`
(5 pre-existing `license-not-encountered` informational warnings for
allow-listed-but-unused license classes — `0BSD`/`BSD-2-Clause`/`MPL-2.0`/
`Unicode-DFS-2016`/`Zlib` — not errors, unchanged from prior phases).

**`cargo test --workspace --lib --bins`** (the non-Docker-gated unit-test
surface, run locally since this host's Docker differential runs are
non-deterministic under parallel load per memory
`differential-fixtures-flake-under-parallel-load`) — **all 21 test binaries
green, 0 failures**:

```
test result: ok. 151 passed; 0 failed; 2 ignored; ... (envoy-config)
test result: ok. 98 passed; 0 failed; ... (envoy-filter)
test result: ok. 97 passed; 0 failed; ... (envoy-listener)
test result: ok. 8 passed; 0 failed; ...
test result: ok. 160 passed; 0 failed; ... (envoy-http1)
test result: ok. 538 passed; 0 failed; ... (envoy-bin / integration)
test result: ok. 208 passed; 0 failed; ... (envoy-cluster)
test result: ok. 8 passed; 0 failed; ...
test result: ok. 157 passed; 0 failed; ... (envoy-xds)
test result: ok. 85 passed; 0 failed; 1 ignored; ... (envoy-http2 — includes
    h2_retry_limit_exceeded_access_log_carries_urx_flag GREEN, matching
    Task 1's local run)
test result: ok. 12 passed; 0 failed; ...
test result: ok. 41 passed; 0 failed; ... (envoy-admin)
test result: ok. 25 passed; 0 failed; ... (envoy-stats)
test result: ok. 11 passed; 0 failed; ... (envoy-tcp)
test result: ok. 15 passed; 0 failed; ... (envoy-tls)
test result: ok. 0 passed; 0 failed; ... (h2spec_conformance lib, no unit tests)
test result: ok. 9 passed; 0 failed; ... (health-aware-http1-backend helper)
test result: ok. 7 passed; 0 failed; ... (http1-echo-server helper)
test result: ok. 5 passed; 0 failed; ... (http2-echo-server helper)
test result: ok. 9 passed; 0 failed; ... (tcp-echo-server helper)
test result: ok. 5 passed; 0 failed; ... (tls-echo-server helper)
```

Zero `FAILED` lines anywhere in the full log. The Docker-gated differential
suite (`cargo test -p differential` under default parallelism, plus the
`h2spec_conformance` conformance suite) was **not** re-run raw locally this
session — per memory `envoy-rust-state4-ci-first-execution`, CI is the
practical/authoritative venue for that surface on this host, and commit
`4bc1589` already has a confirmed-green CI run (`28821162097`, triggered on
push, ~2 hours before this session). Pulled that run's logs instead of
re-running to avoid chasing local Docker flakes for a surface CI already
proved.

### CI evidence — run `28821162097` @ `4bc1589`

`gh run view 28821162097` (repo `pgdad/envoy-rust`):

```
✓ main ci · 28821162097
Triggered via push about 2 hours ago

JOBS
✓ build + test + lint in 6m9s (ID 85472905585)
✓ fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse, 30s each) in 4m4s (ID 85472905634)
```

Both jobs green. Pulled full logs (`gh run view --job=<id> --log`) and
extracted the differential/h2spec/deny sections:

**Fixture `0067` differential test — the new fixture this phase adds:**

```
     Running tests/access_log_h2_urx_retry_exhausted.rs (target/debug/deps/access_log_h2_urx_retry_exhausted-af32e53784b8f0f8)
INFO node registered node.id=envoy-rust-phase-61-fixture-0067 node.cluster=envoy-rust-phase-61
test access_log_h2_urx_retry_exhausted ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.58s
```

Cross-proxy-equal status `503` + whole-line
`{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"via_upstream","rf":"URX"}`
confirmed (the differential test's own byte-exact-access-log assertion is
what makes this `ok`).

**All pre-existing fixtures green simultaneously (the additivity
invariant):** the full `cargo test --workspace` CI step ran every
differential fixture test binary (each fixture is its own `#[test]`
binary under `tests/differential/tests/`); grepping the full CI log for
`test result:` shows **zero occurrences of anything but `0 failed`** across
every one of them (fixtures `0001`-`0067` inclusive) plus every unit-test
binary (`151 passed`/`98 passed`/`538 passed`/etc., matching the local
run above) — confirming `0001`-`0066` remain green alongside the new
`0067`. No `FAILED` string appears anywhere in the CI log.

**h2spec ≥95% gate:**

```
     Running unittests src/lib.rs (target/debug/deps/h2spec_conformance-9c22a19717b13d61)
running 0 tests
test result: ok. 0 passed; 0 failed; ...

     Running tests/h2spec_runner.rs (target/debug/deps/h2spec_runner-73485d2bad653f8a)
running 3 tests
test tests::parse_summary_line_extracts_pass_fail_counts ... ok
test tests::parse_h2spec_output_extracts_section_failure_ids ... ok
test h2spec_pass_rate_gate ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
```

`h2spec_pass_rate_gate` (the test that runs h2spec live against the built
`envoy-bin` and asserts the pass rate stays ≥95%, comparing against the
tracked `known-failures.txt` — per memory `h2spec-3-5-2-preface-host-sensitive`,
this file must never be trimmed from local evidence) passed. Expected — this
phase makes zero H2 codec/framing changes, only an access-log/retry-loop
change, so no new h2spec regression risk existed.

**`cargo deny check` (CI):** identical to the local run — `advisories ok,
bans ok, licenses ok, sources ok`, same 5 informational
`license-not-encountered` warnings, no errors.

**Fuzz job:** `parse_bootstrap` + `jwt_parse` + `cdn_loop_parse` +
`accesslog_format_parse`, 30s each — all `DONE` with 0 crashes (e.g.
`accesslog_format_parse`: `#2298253 DONE cov: 410 ft: 1932 corp: 464/239Kb
... Done 2298253 runs in 31 second(s)`, zero `ERROR`/`panic`/crash lines in
the full job log). No new fuzz target this phase (SPEC §I) — nothing new
to wire in; the existing 4 targets ran clean, confirming no regression.

### §7.5 gate disposition

| Gate item | Status |
|---|---|
| (a) new/changed fixtures green | ✅ fixture `0067` — CI run `28821162097` |
| (b) pre-existing fixtures still green | ✅ `0001`-`0066` — same CI run, zero `FAILED` |
| (c) conformance suites at threshold | ✅ h2spec `h2spec_pass_rate_gate` passed (≥95%; no new H2 framing risk) |
| (d) new fuzzer short-budget clean | N/A — no new fuzz target this phase; existing 4 targets ran clean |
| (e) build/clippy/fmt/test/deny clean | ✅ all five — local (this session) + CI, both clean |
| (f) `REVIEW.md` approved | pending — state-5 is a future session |

Items (a)-(e) are now satisfied. (f) is out of scope for state-4 (§5: state-5
code-review is a separate future session, not chained into this one).

No new ADR fired this session — verification surfaced zero divergence from
SPEC §A-§I; nothing to reconcile. `#![forbid(unsafe_code)]` holds (unchanged
by this phase). DECISIONS.md ledger head remains **ADR-0118**.

**State-4 verification COMPLETE.** The next session is the **§5 state-5
code-review** (`superpowers:requesting-code-review`) — a separate future
session per §5.1 (one state per session).
