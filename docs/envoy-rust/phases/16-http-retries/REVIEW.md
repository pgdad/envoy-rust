# Phase 16 (`16-http-retries`) — REVIEW

- **Lifecycle state:** state 5 (verified → reviewed). This document is the state-5 output.
- **Skill:** `superpowers:requesting-code-review` (per `BOOTSTRAP_PROMPT.md` §5 state 5 + `SKILL_ROUTING.md`).
- **Review range:** `821d1f036..04749aff0` — the Task 1–11 state-3 execution arc + the Task-11
  state-4 verification, atop the state-2 PLAN-write `821d1f036` (excluded as the base). The CODE
  commits reviewed: `3b0e23ecc` (T1 `RetryPolicy` schema + `retry_policy` field +
  `include_attempt_count_in_response` + the deep-clone fold-in) → `2511d7be7` (T2
  `RetryConfig`/`RetryOn`/`AttemptOutcome` tokenization + `validate_retry_policy`) → `dbabd526a`
  (T3 the 3 cluster retry counters, inert-at-0) → `546b06973` (T4 H1 retry loop + per-attempt
  `upstream_rq_total` + `x-envoy-attempt-count` + back-off) → `5c65fc173` (T5 H2 retry loop mirror
  + the ConnectFailure-vs-Reset classification fix) → `1af2f69e0` (T6 stateful fail-then-succeed
  backend knob) → `d1f87a247` (T7 fixture 0024 + Docker wrapper + the cyclic retry-script
  reconciliation) → `273d59be2` (T8 in-process backstop, both paths) → `8cbd8a9ba` (T9 fuzz seed,
  corpus 27→28) → `7dc83e292` (T10 BEHAVIOR_CONTRACT rows). The PROGRESS-subsection / STATE-advance
  commits in the range carry NO code. **The review additionally produced ONE in-review fix commit
  `995445b52`** (see §2) which is part of the reviewed final state.
- **Pre-review HEAD:** `04749aff0` (== `origin/main` at review start; `git rev-list
  --left-right --count HEAD...origin/main` → `0  0`).
- **Method:** 4 read-only code-review subagents, one per concern-cluster, dispatched **SERIALLY**
  (`feedback_serial_subagent_dispatch`), each reading the actual on-disk diff (`git diff` /
  `git show` + `Read` + `Grep`) and returning per-cluster verdicts. The controller independently
  re-verified every Important claim against the code and the pre-phase-16 base
  (`git show 821d1f036:…`) before accepting it, dispatched the in-review fix (TDD: RED → fix →
  GREEN), re-ran the §7.5 stable-toolchain gates locally post-fix, and re-attested the Docker
  differential evidence from the state-4 CI record.
- **Verdict: APPROVED** (zero Critical / **one Important — FIXED IN-REVIEW at `995445b52` +
  re-verified** / 8 non-gating Minors M16-1…M16-8 carried). Mirrors the 14.2 state-5 `c1b2e022e`
  precedent (Important findings resolved in-review, then approved).

---

## 1. The named review focus (STATE.md state-5 charter — the items this session MUST verify)

### 1.1 The L5 per-attempt `upstream_rq_total` / completing-only `upstream_rq_5xx` reconciliation — **VERIFIED CORRECT for real-upstream paths; ONE Important defect found on the synth paths → FIXED IN-REVIEW (§2)**

**The per-attempt `upstream_rq_total` model is correct on both protocols.** The tick is gated on
`attempt.upstream_response` (H1 `crates/envoy-http1/src/hcm.rs` retry loop; H2
`crates/envoy-http2/src/hcm.rs` mirror) — it fires once per attempt that received a real upstream
response, and never on connect-failure / reset / overflow / no-healthy synth paths. This exactly
matches the pre-phase-16 increment placement (inside `router::construct_proxied_response`, only
reachable on the real-response arm), so 1-attempt requests count identically. Fixture 0020
(`upstream_rq_total: 10`, `upstream_rq_5xx: 3` — all real upstream responses) and fixture 0022
(asserts only `outlier_detection.*` names) are provably unaffected; both were re-confirmed green at
the state-4 Docker run and at CI anchor `26761833864`.

**The completing-only `upstream_rq_5xx` model had a defect on the synth paths** — found
independently by the H1 and H2 cluster reviewers and confirmed by the controller against
`git show 821d1f036:…`: the post-loop tick was unconditional on the completing status, so the four
synth-5xx local replies (no-healthy synth-503, connect-failure synth-502, reset synth-502,
overflow/pending-overflow synth-503) ticked `upstream_rq_5xx` where the pre-phase-16
empirically-verified baseline never did. See §2 for the in-review fix.

**Inertness for 1-attempt requests now holds on every path** (real-upstream AND synth) after the
fix: with `retry_policy: None`, `max_retries == 0`, the loop runs exactly once, no retry counter
ticks, no `x-envoy-attempt-count` is emitted, and the counter side-effects are byte-identical to
the pre-phase-16 dispatch.

### 1.2 H1/H2 sibling parity of the retry loops — **VERIFIED CORRECT**

`run_h2_attempt`/`H2AttemptResult` is a structurally exact local mirror of H1's
`run_attempt`/`AttemptResult` (no cross-crate dependency; the `AttemptOutcome` classifier is shared
from `envoy-config`). Verified line-for-line equivalent: identical loop boundary
(`attempts <= max_retries`), identical `upstream_response`-gated `upstream_rq_total` tick, identical
`endpoint.is_some()`-gated `record_response`, identical post-loop `rq_5xx` gate (post-fix) +
`attempts > 1` XOR split + gated `x-envoy-attempt-count`, identical `RetryConfig::backoff` reuse.
**The T5 review-driven ConnectFailure-vs-Reset fix is correct and complete:** all 3 H2 connect
sites (H1-fork `Client::connect` Err; H2-pool `PoolError::Connect`; pool-None per-call connect Err)
classify as `AttemptOutcome::ConnectFailure`; only `send_request` errors classify as `Reset` —
matching H1 exactly. The TDD sibling test pair
`{h2_,}connect_failure_retried_on_connect_failure_policy` exists and passes on both protocols.
The H1-vs-H2 upstream-protocol fork lives INSIDE `run_h2_attempt`, so the H2 loop is genuinely
protocol-agnostic (a retry on an H2-protocol cluster re-acquires from the H2 pool).

### 1.3 Loop boundary semantics — **VERIFIED CORRECT**

`attempts <= max_retries` with `attempts` incremented at loop top: `num_retries: 1` → exactly 2
attempts; `retry_policy: None` (`max_retries == 0`) → exactly 1 attempt; explicit
`num_retries: Some(0)` → exactly 1 attempt. The post-loop XOR (`attempts > 1` →
`final_retriable ? limit_exceeded : success`) fires exactly one of the two outcome counters, only
when at least one retry happened, never on 1-attempt requests. Status classification boundaries per
L1: `5xx` = `(500..=599)`; `gateway-error` = exactly 502/503/504 (a 500 does NOT match);
`retriable_status_codes` consulted only when its token is present. Back-off
(`RetryConfig::backoff`) applies between attempts only, never after the breaking attempt, and is
overflow-safe at any attempt count (shift guard before the `<<`).

### 1.4 The cyclic retry-script harness reconciliation (T7) — **VERIFIED CORRECT; latent fragility properly documented**

The cyclic-window logic (`idx % (N+1) < N` → 503) is correct for `fail:1` (503,200,503,200,…), and
the load-bearing assumption — each proxy's retry pair lands in its own window — was verified
against the actual driver: `tests/differential/src/lib.rs` drives the proxies via a plain
sequential `for` loop over `[upstream, subject]` (no `tokio::join`), and the `/retry-exhausted`
path is served by the stateless `--per-path` arm so it can never desync the `/retry-success`
counter. The latent fragility (a future parallel-drive refactor would interleave windows) is
documented in BOTH required places: the helper source (`tests/helpers/health-aware-http1-backend/
src/main.rs` CAUTION block) and the fixture README. Fixture 0024's arithmetic matches every
lock-in: statuses 200/503, bodies `ok\n` / `service unavailable\n` (L9 last-upstream-verbatim),
`x-envoy-attempt-count: 2` value-exact on both probes (L6), and the 8 `expected_stats`
(`upstream_rq_retry: 2` / `_success: 1` / `_limit_exceeded: 1` per L4; `upstream_rq_total: 4`
per-attempt and `upstream_rq_5xx: 1` completing-only per L5; downstream 1/1/2). Both fixture
configs are structurally identical modulo documented divergences and set
`dns_lookup_family: V4_ONLY` (L11) + `include_attempt_count_in_response: true` (L6). The new
`require_header_value` harness field is `#[serde(default)]` → genuinely inert for fixtures
0001–0023 (grep-confirmed: only 0024 uses it).

### 1.5 The gated `x-envoy-attempt-count` emission (L6) — **VERIFIED CORRECT**

Emitted on the downstream response ONLY when the matched VirtualHost sets
`include_attempt_count_in_response: true`; value = total attempts; absent without the flag
regardless of `retry_policy` presence. The flag is faithfully threaded through the H1 deep-clone
sites (`clone_route_config`/`clone_route_action` — the ADR-0045 silent-drop risk, discharged at
T1) and the H2 walk. Fixture 0024 asserts the header value-exact (`2`) bilaterally on both probes —
the L6 reconciliation validated end-to-end against real Envoy v1.33.0 with zero expectation edits.
(One Minor: the synth-path emission extrapolation is not differentially verified — M16-3.)

### 1.6 The discovered carryforwards — **DISPOSITIONS RECORDED (§4)**

All four arc-discovered carryforwards verified accurately characterized (including
grep/`git diff`-level confirmation that the H1-pool `Connection: close` gap is pre-existing —
`git diff 821d1f036..HEAD -- crates/envoy-http1/src/pool.rs` is empty). Dispositions in §4.

---

## 2. The Important finding + the in-review fix (`995445b52`)

**Finding (Important; found by reviewers 2 + 3 independently; controller-confirmed against the
pre-phase-16 base):** the post-loop completing-response `upstream_rq_5xx` tick was unconditional on
the completing status —

```rust
if final_response.status / 100 == 5 {
    cluster.upstream_rq_5xx().inc();
}
```

— at H1 `crates/envoy-http1/src/hcm.rs` and its H2 mirror. Because the synth local replies are
5xx (no-healthy synth-503, connect-failure synth-502, reset synth-502, overflow/pending-overflow
synth-503), every such 1-attempt request now ticked `cluster.<name>.upstream_rq_5xx` where the
pre-phase-16 baseline (increment inside `router::construct_proxied_response`, only reachable on
real upstream responses) did not. **This violated the ADR-0045 L5 lock-in** ("both inert (== today)
for 1-attempt requests") and the project's regression-equivalence discipline (D-3.3: the
empirically-verified contract is authoritative; an unverified behavioral change must not land
silently). The defect was uncaught because no fixture asserts `upstream_rq_*` on a synth path
(0019 asserts no stats; 0022 asserts only `outlier_detection.*`; 0023 asserts
`upstream_rq_pending_overflow`/`upstream_cx_*` but not `upstream_rq_5xx`; 0020/0024 drive only
real upstream responses).

**Disposition decision:** restore the verified baseline (gate the tick on a real completing
upstream response) rather than keep the unconditional tick. Rationale: D-3.3 — whether real Envoy
charges `upstream_rq_5xx` on router-generated local replies is **empirically unverified** (the
ADR-0045 §6.2 Docker run only exercised real upstream responses); the verified pre-phase-16
baseline is the contract. If a future phase verifies Envoy's synth-path counting and finds it
charges these codes, that change lands with its own empirical evidence + ADR.

**The fix (commit `995445b52`, TDD, both protocols in one commit preserving mirror parity):**

- The loop now breaks with `(attempt.response, attempt.upstream_response)`; the tick is gated:
  `if completing_upstream_response && final_response.status / 100 == 5 { … }` (H1 + H2 identical).
- TDD regression tests (RED before the fix — both read `rq_5xx == 1`; GREEN after):
  `connect_failure_synth_does_not_tick_upstream_rq_5xx` (H1) +
  `h2_connect_failure_synth_does_not_tick_upstream_rq_5xx` (H2) — a `retry_policy: None` route to a
  kernel-refused endpoint (`127.0.0.1:1`) → synth-502 → assert `upstream_rq_5xx == 0` AND
  `upstream_rq_total == 0`.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` L5 paragraph gained one clarifying sentence (synthetic
  local replies do not tick `upstream_rq_5xx`; pre-phase-16 baseline preserved).

**Post-fix re-verification (controller-run, quoted in §5):** envoy-http1 92/0, envoy-http2 62/0/1,
the in-process backstop 1/0 (unaffected — its completing responses are real upstream responses),
workspace + standalone `-p envoy-http1`/`-p envoy-http2` builds clean, clippy `-D warnings` clean,
fmt clean. The fix changes NO differentially-asserted observable (no fixture asserts synth-path
`upstream_rq_5xx`), so the 24-fixture Docker surface is unaffected; the post-fix CI run at this
review commit's push is the anchoring evidence.

---

## 3. Cluster verdicts

| # | Concern cluster | Tasks | Verdict | Critical | Important | Minor |
|---|---|---|---|---|---|---|
| 1 | `envoy-config` schema + `RetryConfig` tokenization + validator + fuzz seed | T1, T2, T9 | **CLEAN** | 0 | 0 | 2 (M16-1, M16-2) |
| 2 | Cluster retry counters + H1 retry loop + per-attempt counting + `x-envoy-attempt-count` | T3, T4 | **With fixes → FIXED** | 0 | 1 (§2 — fixed `995445b52`) | 2 (M16-3, M16-4) |
| 3 | H2 retry loop mirror + H1/H2 sibling parity | T5 | **With fixes → FIXED** | 0 | 1 (same as cluster 2 — fixed `995445b52`) | 2 (M16-5, M16-6) |
| 4 | Harness + fixture 0024 + in-process backstop + BEHAVIOR_CONTRACT | T6, T7, T8, T10 | **CLEAN** | 0 | 0 | 2 (M16-7, M16-8) |

Cluster-1 highlights: schema/serde shape exactly per PLAN + L3; tokenizer faithful to L2
accept-and-ignore; `is_retriable` boundaries exact per L1; back-off overflow-safe; the fuzz-seed
`.gitignore`/SUCCESS-array atomic edit consistent. Cluster-4 highlights: cyclic-window logic proven
correct under the sequential driver; fixture arithmetic matches every lock-in; backstop
timing-robust (`poll_stat_until` bounded polling) with correct cumulative assertions;
BEHAVIOR_CONTRACT additions are pure-additive with accurate code anchors.

---

## 4. Carryforward dispositions + Minor findings (non-gating)

### 4.1 Arc-discovered carryforwards (from PROGRESS Task 11; reviewed + dispositioned)

1. **H1-pool `Connection: close` re-pooling gap** (PRE-EXISTING, phase 13.1; surfaced by T8).
   Confirmed pre-existing (`git diff 821d1f036..HEAD -- crates/envoy-http1/src/pool.rs` empty).
   The dispatch success arm re-pools without inspecting the upstream's `Connection` header and
   `H1Pool::acquire` does no liveness check on idle reuse. Differential gap vs real Envoy;
   currently invisible (every fixture backend serves keep-alive). **Disposition: carries to a
   future pool-hardening phase (no named owner).** The retry loop marginally raises its relevance
   (a retry could acquire the dead pooled conn → spurious `Reset`), strengthening the case for the
   pool-hardening follow-up. Candidate fixes documented in PROGRESS Task 8.
2. **H2-pool send-failure no-invalidate asymmetry** (PRE-EXISTING; found by this review's cluster
   3 — new finding, M16-5). H1's `run_attempt` calls `g.invalidate()` on send failure; H2's
   `run_h2_attempt` does not (the pre-phase-16 H2 code had the identical pattern, so NOT a
   phase-16 regression). A retry could re-acquire a poisoned H2 pooled connection. **Disposition:
   groups with carryforward 1 (the same pool-liveness family); carries to the same future
   pool-hardening phase.**
3. **Cyclic retry-script latent fragility** (T7): a future parallel-drive harness refactor would
   interleave the cyclic windows. Documented in the helper source + fixture README + this REVIEW.
   **Disposition: carries as a standing harness caution (no action until a parallel-drive refactor
   is proposed).**
4. **H2-upstream-fork retry coverage gap** (T5/T8): the retry-on-H2-protocol-cluster path is
   covered structurally (the fork lives inside `run_h2_attempt`) + bilaterally (fixture 0024's H2
   listener is the 0018 precedent) but not by a direct in-process test (no stateful H2 backend
   helper exists). **Disposition: carries; a future H2-focused phase or the pool-hardening phase
   adds a stateful H2 test backend.**
5. **CI readiness-flake family 0011 + 0012** (T9's CI run): startup-probe budget on loaded
   runners; re-run clears it; not a regression (memory `project_flaky_access_log_fixture_0012`).
   **Disposition: carries unchanged.**

### 4.2 Minor findings (M16-1 … M16-8; none gating; carried with no named owner)

| # | Finding | File | Why non-gating |
|---|---|---|---|
| M16-1 | `validate_retry_policy(_route)` is an intentionally-infallible placeholder called for ALL route variants (incl. `DirectResponse`) | `crates/envoy-config/src/bootstrap.rs` | Deliberate PLAN Task 2 symmetry placeholder; future semantic checks must not assume proxy-route-only input |
| M16-2 | `retriable_status_codes: Vec<u32>` vs `status: u16` width — entries > 65535 are silently dead | `crates/envoy-config/src/bootstrap.rs` | Mirrors the Envoy proto (`repeated uint32`); no valid HTTP status exceeds 599 |
| M16-3 | `x-envoy-attempt-count` emission on synth paths (e.g. `: 1` on no-healthy 503) is an unverified extrapolation; the code comment overstates the verification basis | `crates/envoy-http1/src/hcm.rs` (+ H2 mirror) | Not differentially asserted (no fixture exercises a synth path with the vhost flag set); flag for the next retry-adjacent §6.2 verification |
| M16-4 | `#[allow(unused_assignments)]` on `final_retriable` | `crates/envoy-http1/src/hcm.rs` (+ H2 mirror) | Cosmetic; the loop structure makes the initial value genuinely unused |
| M16-5 | H2 send-failure does not invalidate the pooled guard (pre-existing; H1 does) | `crates/envoy-http2/src/hcm.rs` | Pre-existing pool-liveness asymmetry; grouped with carryforward 4.1.2 |
| M16-6 | H2 pushes `"x-envoy-attempt-count"` as a string literal; H1 uses the `X_ENVOY_ATTEMPT_COUNT` const | `crates/envoy-http2/src/hcm.rs` | Values identical; cosmetic parity nit |
| M16-7 | Backstop accept-loop task has no shutdown signal / leaked `JoinHandle` | `crates/envoy-bin/tests/upstream_retry.rs` | Benign for a short-lived test process; matches the deliberate `std::mem::forget(tmp)` posture |
| M16-8 | In-process backend cyclic logic uses N=1-specific `idx.is_multiple_of(2)` vs the helper's general `idx % (N+1) < N` | `crates/envoy-bin/tests/upstream_retry.rs` | Equivalent for the only N the backstop uses (N=1) |

### 4.3 Standing multi-phase Minor inventory (inherited; not engaged by phase 16)

The 14.1 REVIEW M-track items, M-c1 (`tokio-util` `["rt"]`-leanness), M-c2 (`.lock().unwrap()`
poison-hardening), M-c3 (frozen-record "14"s), the §6.9 per-class `upstream_rq_{2,3,4}xx`
extension, the `upstream_cx_total` TCP-proxy carve-out, the phase-15 rollovers (per-endpoint
`cx_open` reconciliation; `max_pending_requests > 0` pending queue; `max_requests`/`max_retries`/
`track_remaining`/multi-priority — which phase 16's retries now UNBLOCK), and ADR-0028 all carry
forward unchanged.

---

## 5. §7.5 phase-done gate re-attestation

The state-4 verification (PROGRESS Task 11) ran gates (a)–(e) ALL GREEN at HEAD `b13168134` with CI
anchor `26761833864` (`completed / success`). This review re-attests that record and adds the
post-fix local re-verification:

| Gate | State-4 evidence (HEAD `b13168134`, CI `26761833864`) | Post-fix re-verification (HEAD `995445b52`, local) |
|---|---|---|
| (a) fixture 0024 green | `ok. 1 passed` (Docker, bilateral, zero expectation edits) | Unaffected (fix changes no differentially-asserted observable); anchored at this commit's CI push |
| (b) 23 pre-existing fixtures green | All 24 green simultaneously, one `cargo test --workspace` run + CI | Unaffected (no fixture asserts synth-path `upstream_rq_5xx`); 0020/0022 inertness now ALSO unit-tested |
| (c) h2spec ≥95% | CI-anchored (phase 16 touches no H2 framing) | Unaffected |
| (d) fuzz clean | `Done 200000 runs`, cov 14261, 0 crashes, 28-seed corpus | Unaffected (no envoy-config change in the fix) |
| (e) 5 stable gates | build/clippy/fmt/test (971/0/2)/deny all clean | build clean; clippy `-D warnings` clean; fmt clean; envoy-http1 **92/0** (+1 test); envoy-http2 **62/0/1** (+1 test); backstop 1/0 |
| standalone builds (lock-in #14) | 4/4 clean | `-p envoy-http1` + `-p envoy-http2` re-run clean (the 2 crates the fix touches) |
| (f) REVIEW.md approved | — | **THIS document — APPROVED** |

The post-fix Docker-gated CI run (triggered by this review commit's push) is the final anchoring
evidence for (a)/(b) at the post-fix HEAD; the fix is provably outside the differential surface
(synth-path-only counting, asserted by no fixture), so a green run is expected. If that CI run
fails on any differential fixture, re-enter state 3 per §5.2 before the state-6 close-out.

---

## 6. ADR projection

**No new ADR.** The in-review fix IMPLEMENTS ADR-0045 L5 correctly (restores its stated 1-attempt
inertness constraint) rather than changing any decision. Ledger head stays **ADR-0045** (count 46;
next available **ADR-0046**). **ADR-0028** (H1-listener × H2-cluster dispatch deferral) remains
OPEN — phase 16 does not engage it.

---

## 7. Verdict + next state

**APPROVED.** Zero Critical; the single Important finding (the L5 synth-path `upstream_rq_5xx`
over-count) was fixed in-review at `995445b52` with TDD regression tests on both protocols and
re-verified; 8 non-gating Minors (M16-1…M16-8) + 5 carryforward dispositions are recorded above
with no named owner.

Per `BOOTSTRAP_PROMPT.md` §5 state 6 + §5.1 (one state per session), the **next session performs
the state-6 deterministic close-out**: flip ROADMAP row `16` `in-progress → done` (a non-split
top-level phase flips its own row alone), advance STATE.md to "awaiting next planning", append the
`### Phase-16 rollovers` Notes subsection, and land the §5.3-format final phase commit
(`phase 16: HTTP retry policy … [ADR-0044, ADR-0045]`).
