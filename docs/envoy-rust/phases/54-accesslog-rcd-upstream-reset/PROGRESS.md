# Phase 54 — `54-accesslog-rcd-upstream-reset` — Implementation Progress

> §5 state-3 implementation log. Each task is RED→GREEN→commit per
> `superpowers:test-driven-development`, executed via
> `superpowers:subagent-driven-development` (one fresh implementer subagent per
> task + a dedicated task-reviewer subagent per task; every task review came
> back **Approved**, 0 Critical / 0 Important across all five). Per-task
> discipline: cargo-fmt-check is local-authoritative; the Docker differential
> fixture `0062` is CI-authoritative (memory `envoy-rust-state4-ci-first-execution`)
> — `0062` is backend-spawning → expect LOCAL-RED on this dev host (memory
> `differential-host-bridge-ip-192-168-65-2`), GREEN on CI. State-4 verification
> (the §7.5 gate on CI) is the next session — NOT run in this session.

---

## Task 1 — §A rcd-set (unguarded) + §B derive migration + retire `reset_for_log` + extend the positive backstop ✅

**RED** (`cargo test -p envoy-http1 h1_upstream_reset_access_log_carries_uc_flag -- --nocapture`),
after extending the backstop's json_format + assertion to expect the deterministic
reset rcd:
```
assertion `left == right` failed: upstream-reset access-log line carries the deterministic reset rcd + rf:UC: "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"UC\"}\n"
  left: "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"UC\"}\n"
 right: "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}\n"
```
(confirms the reset path still emits the shared in-loop `via_upstream` rcd pre-change.)

**Implementation:** `crates/envoy-http1/src/hcm.rs` — §A: replaced the phase-53
`reset_for_log = matches!(final_outcome, Some(AttemptOutcome::Reset));` post-loop set
(`~:1196-1200`) with an `if matches!(final_outcome, Some(AttemptOutcome::Reset)) { response_code_details_for_log = Some("upstream_reset_before_response_started{connection_termination}".to_owned()); }` block (UNGUARDED in this task — Task 2 adds the
`!retry_limit_exceeded_for_log` guard). §B: added the
`Some("upstream_reset_before_response_started{connection_termination}") => { "UC" }`
braced match arm (single-line form exceeds 100 columns at this indentation — kept
braced per `cargo fmt`) after the `{overflow} => "UO"` arm; deleted the
`} else if reset_for_log { "UC"` branch; deleted the `reset_for_log` declaration +
its phase-53 comment block (`~:865-873`). §F: retargeted the record-build derive
comment and the rcd-match enumeration comment to describe the new rcd-derivation
(no longer "boolean-keyed").

**GREEN** (`cargo test -p envoy-http1 h1_upstream_reset_access_log_carries_uc_flag -- --nocapture`):
```
test hcm::tests::h1_upstream_reset_access_log_carries_uc_flag ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 149 filtered out
```
Full `envoy-http1` suite: `test result: ok. 150 passed; 0 failed`. `grep -rn "reset_for_log" crates/` → only the two retirement-documenting comments (no active code reference).

**Commit:** `e12825e` — `phase 54 §A+§B: set reset rcd {connection_termination} + migrate UC to rcd-match, retire reset_for_log [ADR-0111]`

**Task review:** ✅ Approved — 0 Critical / 0 Important / 1 cosmetic Minor (a stale
`:1055` in-loop-write line-reference inherited verbatim from the PLAN.md brief text,
shifted by the `reset_for_log` decl deletion; no functional impact, deferred to final
review triage).

---

## Task 2 — §A `!retry_limit_exceeded_for_log` guard + §G retry-exhausted-reset negative backstop (M53-3) ✅

**RED** (`cargo test -p envoy-http1 h1_retry_exhausted_reset_keeps_via_upstream_rcd_and_urx_flag -- --nocapture`),
a new test driving `retry_policy: { retry_on: "reset", num_retries: Some(1) }` against
an always-resetting backend:
```
assertion `left == right` failed: retry-exhausted reset keeps rcd:via_upstream + rf:URX (the §A guard):
  left:  "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"URX\"}\n"
 right: "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"URX\"}\n"
```
(confirms Task 1's unguarded §A incorrectly sets the deterministic rcd even when the
retry budget was exhausted — the M53-3 edge.)

**Implementation:** `crates/envoy-http1/src/hcm.rs` — changed the §A condition to
`if matches!(final_outcome, Some(AttemptOutcome::Reset)) && !retry_limit_exceeded_for_log { … }`; updated the accompanying comment to explain the guard preserves the
M53-3 edge (rcd stays `via_upstream`, flag renders `URX` — the derive's URX branch is
checked before the rcd-match).

**GREEN** (same test): `test result: ok. 1 passed; 0 failed`. Full `envoy-http1`
suite: `test result: ok. 151 passed; 0 failed` — both the Task-1 positive case
(pure reset → `{connection_termination}`/`UC`) and the Task-2 negative case
(retry-exhausted reset → `via_upstream`/`URX`) green simultaneously.

**Commit:** `1aa024f` — `phase 54 §A guard + §G: !retry_limit guard preserves M53-3 (retry-exhausted reset → via_upstream/URX) [ADR-0111]`

**Task review:** ✅ Approved — 0 Critical / 0 Important / 2 trivial Minors (brief's
own hyperbolic doc-comment phrasing copied verbatim; a pre-existing per-test
boilerplate-duplication convention already used throughout the file) — no action
needed.

---

## Task 3 — §C fixture `0062-accesslog-rcd-upstream-reset` + §D differential test ✅

**Implementation (no in-process RED/GREEN — a new differential fixture + thin test
wrapper; CI-authoritative, not locally drivable per memory `differential-host-bridge-ip-192-168-65-2`):**
created `tests/fixtures/0062-accesslog-rcd-upstream-reset/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
as a structural clone of fixture `0061` (the phase-53 accept-then-close `UC`-flag
witness — same `STRICT_DNS` cluster, NO `circuit_breakers`/`retry_policy`, the same
`{{BACKEND_HOST}}`/`{{CLOSE_BACKEND_PORT}}` markers, reusing the existing
`TcpCloseBackend` launch arm verbatim — NO new harness code), with the json_format
extended to add `rcd: "%RESPONSE_CODE_DETAILS%"` between `rc` and `rf`. The
`expectations.yaml` carries only `expected_status: 503` (the byte-exact expected line
is documented only in comments — the driver does pure cross-proxy equality, no
embedded static literal). Created `tests/differential/tests/access_log_rcd_upstream_reset.rs`
— a thin `differential::run_fixture(&dir)` wrapper, structurally identical to the
existing `access_log_rf_upstream_reset.rs` (fixture 0061's test).

**Compile/discover check** (`cargo test -p differential --test access_log_rcd_upstream_reset --no-run`):
```
Finished `test` profile [unoptimized + debuginfo] target(s)
  Executable tests/access_log_rcd_upstream_reset.rs (target/debug/deps/access_log_rcd_upstream_reset-e5da2563e4dcfc37)
```
Clean compile + discovery. **LOCAL-RED expected, NOT run to completion locally**
(backend-spawning → the host Docker bridge-IP flake); GREEN on CI is the §7.5 gate,
deferred to the state-4 verification session.

**Commit:** `c222ab4` — `phase 54 §C+§D: fixture 0062 + differential test (reset rcd {connection_termination}, byte-exact) [ADR-0111]`

**Task review:** ✅ Approved — 0 Critical / 0 Important / 2 cosmetic Minors (a
collapsed cross-reference comment in `envoy.yaml`; a "3 probes" vs sibling's "8
repeats" recon-rigor wording difference, both intentional/non-functional) — confirmed
near-byte-exact structural clone vs `0061` by direct diff.

---

## Task 4 — §E BEHAVIOR_CONTRACT updates (rcd row + `UC` clause inversion + anchor refresh) ✅

**Implementation (documentation-only, no code):** `docs/envoy-rust/BEHAVIOR_CONTRACT.md`
— **(spec-review M2)** INVERTED (fully replaced, not appended) the `%RESPONSE_FLAGS%`
row's `UC` clause from "like `URX`/`UF` — NOT derived from `%RESPONSE_CODE_DETAILS%`
… derived from the `reset_for_log` boolean" to "UNLIKE `URX`/`UF` … derived 1:1 from
`%RESPONSE_CODE_DETAILS%` = `upstream_reset_before_response_started{connection_termination}`
… witnessed byte-exact at phase 54 (ADR-0111), fixture **0062**"; updated the
witnessed-rcd-flag-set summary sentence (phase-53/0061 now reads "witnessed at phase
54 (M53-1 consumed, fixture 0062)"); extended the `%RESPONSE_CODE_DETAILS%` row with
the new reset-path set-site description + a fixture-0062 rationale sentence (mirroring
the existing `no_healthy_upstream`/`route_not_found`/`{overflow}` set-site entries) +
the updated default-absent fixture-list tail. **(spec-review M1)** refreshed every
stale `hcm.rs:1343` anchor in these rows — re-derived the CURRENT line via
`grep -n "response_flags: if retry_limit_exceeded_for_log" crates/envoy-http1/src/hcm.rs`
→ `:1376` (not the brief's suggested `:1366`, since Tasks 1-2's edits shifted the line
further than anticipated) — and used that verified-current number throughout.

**Verification** (`grep -n "reset_for_log\|hcm.rs:1343\|NOT logged this phase\|future phase (M53-1)" docs/envoy-rust/BEHAVIOR_CONTRACT.md`): no stale matches — the 4
remaining `reset_for_log` hits are all in explicit "was RETIRED" historical framing.

**Commit:** `9764c5b` — `phase 54 §E: BEHAVIOR_CONTRACT reset rcd row + UC clause inversion + :1343→:1376 anchor [ADR-0111]`

**Task review:** ✅ Approved — 0 Critical / 0 Important / 0 Minor. The reviewer
independently re-derived the `hcm.rs:1376` anchor against the live source and
confirmed it is genuinely the `response_flags:` derive head.

---

## Task 5 — §F exhaustive sweep + §3.2 byte-preservation re-grep + local verification ✅

**Verification + residual cleanup (no functional code change — comment/doc-prose
retargeting only):** `grep -rn "reset_for_log" crates/ docs/ tests/` initially
surfaced 5 occurrences in LIVE, currently-true-tense prose this phase's own Tasks 1
and 4 had introduced (`hcm.rs` ×2, `BEHAVIOR_CONTRACT.md` ×2, the new fixture-0062
README ×2, the new differential test doc-comment ×1) naming the now-retired
`reset_for_log` identifier in a "was retired" footnote — retargeted all five to say
"the phase-53 boolean discriminator was retired" (same meaning, no longer
string-matches the dead identifier). Left ARCHIVAL material untouched per this
codebase's established immutable-artifact convention (verified by `git log` precedent
that fixture READMEs and closed-phase planning docs are never revisited):
`ROADMAP.md`, `STATE_HISTORY.md`, `DECISIONS.md`, phase-53's own
`{SPEC,PLAN,REVIEW,PROGRESS}.md`, this phase's own `SPEC.md`/`PLAN.md`, `STATE.md`
(due its own wholesale refresh at the next state transition), and fixture `0061`'s
`README.md`. `grep -rl "CLOSE_BACKEND_PORT" tests/fixtures/*/envoy-rust.yaml` → only
`0061`/`0062` (confirmed). The Step-3 stray-comment grep's 3 hits were investigated
and confirmed to be unrelated `URX`/`UF` boolean-derivation comments (correct by
design, not stray UC references) — no fix needed.

**Local build/lint/format/test** (the locally-runnable §7.5 (e) subset):
```
cargo build --workspace --all-targets            → clean, Finished in 3.00s
cargo clippy --workspace --all-targets --all-features -- -D warnings  → clean, 0 warnings
cargo fmt --all -- --check                        → clean, exit 0
cargo test -p envoy-http1                         → test result: ok. 151 passed; 0 failed
cargo test -p envoy-accesslog                     → test result: ok. 98 passed; 0 failed
```
All clean, including both Task 1's and Task 2's new in-process backstops.

**Commit:** `c5a3a77` — `phase 54 §F: residual reset_for_log sweep + verification cleanup [ADR-0111]`

**Task review:** ✅ Approved — 0 Critical / 0 Important / 1 process Minor (the
brief's literal "NO matches anywhere" Step-1 wording is in tension with its own
worked archival-exception examples; the reviewer independently reproduced the
archival/live boundary by direct `git log`/`grep` and confirmed it correct). No code
action needed.

---

## Summary

All 5 PLAN.md tasks landed RED→GREEN→commit, each reviewed by a fresh task-reviewer
subagent (✅ Approved, 0 Critical / 0 Important across all five tasks; only cosmetic/
process Minors, none requiring rework). Final local state: `cargo build`/`clippy
-D warnings`/`fmt --check` clean; `cargo test -p envoy-http1` 151/151; `cargo test -p
envoy-accesslog` 98/98; `grep -rn "reset_for_log" crates/` and non-README `tests/`
clean (archival docs intentionally untouched). `#![forbid(unsafe_code)]` holds — no
`unsafe` introduced. NO new `Op`/`AccessLogRecord` field/crate/dependency/fuzz-target/
`ConfigError` variant/test-harness code. The `0062` differential fixture compiles and
is discovered (`--no-run` clean) but was NOT run to completion locally (expected
LOCAL-RED per the host's Docker bridge-IP flake, memory
`differential-host-bridge-ip-192-168-65-2`) — its green/red status on CI, plus
`cargo test --workspace`, `cargo deny check`, and h2spec, is the **state-4
verification** session's job (the next session, NOT this one — `§5.1` one state per
session).

## State-4 verification ✅ (§5 state-4 / §7.5 phase-done gate — `superpowers:verification-before-completion`)

Fresh re-run of the FULL §7.5 gate this session (`superpowers:verification-before-completion`
— evidence before claims). **CI is AUTHORITATIVE for the Docker differential** (memory
`envoy-rust-state4-ci-first-execution` + `differential-host-bridge-ip-192-168-65-2`).

### (a)-(e) Local gate — all GREEN except documented host-artifact LOCAL-REDs
Rebuilt the DEBUG `envoy-bin` (`cargo build -p envoy-bin` → Finished, EXIT 0) before the
differential (memory `differential-harness-uses-debug-envoy-bin`), then:
```
cargo build --workspace --all-targets                                 → Finished (EXIT 0)
cargo clippy --workspace --all-targets --all-features -- -D warnings  → Finished (EXIT 0)
cargo fmt --all -- --check                                            → clean (FMT_EXIT=0)
cargo test --workspace --no-fail-fast                                 → 4 failures, all documented host artifacts (below)
cargo deny check                                                      → advisories ok, bans ok, licenses ok, sources ok (DENY_EXIT=0)
```

`cargo test --workspace` (first run, default fail-fast) stopped at the first
alphabetically-ordered differential failure. Re-ran with `--no-fail-fast` to see the
complete local picture: exactly **4** local failures, all pre-existing documented
host-environment artifacts, NONE a regression from this phase's `hcm.rs` change:

1. **`access_log_rcd_upstream_reset`** (fixture `0062`, NEW this phase): local mismatch
   `envoy="...upstream_reset_before_response_started{remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:45203}",rf:"UF"` vs
   `envoy-rust="...upstream_reset_before_response_started{connection_termination}",rf:"UC"`.
   The upstream Envoy *container* cannot reach the sibling accept-then-close backend over
   this host's Docker bridge (sees a connect-failure) while envoy-rust reaches it directly
   (sees the genuine post-connect reset) — the documented host-bridge artifact (memory
   `differential-host-bridge-ip-192-168-65-2`), same failure class as fixture 0061's phase-53
   precedent.
2. **`access_log_rf_upstream_reset`** (fixture `0061`, PRE-EXISTING, phase 53): identical
   `UF`-vs-`UC` host-bridge mismatch — the SAME documented artifact, unchanged from the
   phase-53 state-4 record.
3. **`admin_config_dump_server_info`** (PRE-EXISTING): `backend::192.168.65.2:<port>::*`
   envoy-only cluster-stat lines — the documented `192.168.65.2` bridge-IP artifact (memory
   `differential-host-bridge-ip-192-168-65-2`), unrelated to this phase's surface.
4. **`xds_file_based_eds_fixture`** (PRE-EXISTING): "upstream Envoy never became accept-ready
   … Connection refused" — a Docker container startup race under parallel `cargo test` load
   (memory `differential-fixtures-flake-under-parallel-load` / `eds-fatal-startup-test-port-reuse-flake`
   class), unrelated to this phase's surface.

Every other test binary in the `--no-fail-fast` run reported `test result: ok` — no other
fixture, unit, or integration test regressed. `cargo deny check` warnings are the same
benign `license-not-encountered` allowances as every prior phase (MPL-2.0 /
Unicode-DFS-2016 / Zlib / 0BSD / BSD-2-Clause allowed but unused), not findings.

### Differential surface + conformance + fuzz — CI AUTHORITATIVE, GREEN
Origin `main` already carried the phase-54 state-3 commits (`e12825e`..`352c0c0`) from the
prior session's push — no new push was needed this session. Authoritative Linux CI run
**`28481385288`** @ code-HEAD **`352c0c0`** (the state-3 STATE-advance commit, confirmed via
`git fetch origin main` + `gh run list`) — both jobs `success`:
```
build + test + lint                                                            → success
fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse)   → success
```
Pulled the full job log (`gh run view 28481385288 --log`) and confirmed directly:
- **`cargo clippy --workspace --all-targets --all-features -- -D warnings`**: ran clean,
  no errors before the next step.
- **`cargo fmt --all -- --check`**: ran clean.
- **fixture `0062` GREEN on native Linux CI**: `test access_log_rcd_upstream_reset ... ok`
  — both proxies emit the byte-identical
  `{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`
  (the host-bridge artifact does NOT occur on CI; both proxies reach the sibling backend).
- **fixture `0061` GREEN on CI**: `test access_log_rf_upstream_reset ... ok` (unaffected by
  this phase's change, re-confirmed).
- **`admin_config_dump_server_info` GREEN on CI**: `test admin_config_dump_server_info ... ok`
  (confirms the local failure was purely the host-bridge-IP artifact, not a real regression).
- **`xds_file_based_eds_fixture` GREEN on CI**: `test xds_file_based_eds_fixture ... ok`
  (confirms the local failure was the Docker-startup-race flake, not a real regression).
- **all `0001`-`0061` green simultaneously**: the run reports **133 `test result: ok`** and
  **0 `test result: FAILED`** across the whole workspace (one more green than phase 53's 132,
  the net-new `0062` fixture) — the §A rcd-set / §B derive migration touch no existing GREEN
  fixture.
- **conformance h2spec ≥95% (unchanged)**: `test h2spec_pass_rate_gate ... ok` — NO HTTP/2
  codec change this phase, the gate is unmoved.
- **`cargo deny check` on CI**: `advisories ok, bans ok, licenses ok, sources ok` — same
  benign license-not-encountered warnings as local.
- **Fuzz: NONE new** — the fuzz job ran the existing 4 targets (`%RESPONSE_CODE_DETAILS%` is
  an existing operator; `ci.yml` unchanged; the new differential is an auto-discovered
  `#[tokio::test]`, not a fuzz target). Job `success`, 0 crashes.

### Disposition
§7.5 gate (a)-(e) MET on CI (the authoritative environment); (f) REVIEW.md is the next
session's job (state-5 code-review). No §7.5 check failed for a real reason — all 4
LOCAL-REDs are documented pre-existing host-environment artifacts (2 already-known from
phase 53's precedent record, 2 newly-confirmed-benign this session by cross-referencing the
CI green), none a regression from this phase's `hcm.rs` change. **No ADR fired** —
verification overturned no PLAN/SPEC fact (ADR-0112/ADR-0113 stay reserved-but-UNFIRED). No
re-implementation. → advance STATE to the §5 state-5 code-review (the SESSION AFTER). Per
§5.1, STOP here — do not chain to state-5 in this session.
