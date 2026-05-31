# Phase 15 (`15-circuit-breakers`) — REVIEW

- **Lifecycle state:** state 5 (verified → reviewed). This document is the state-5 output.
- **Skill:** `superpowers:requesting-code-review` (per `BOOTSTRAP_PROMPT.md` §5 state 5 + `SKILL_ROUTING.md`).
- **Review range:** `0c46b7bc1..d2558fc3e` — the Task 1–10 state-3 execution arc + the Task-10
  state-4 verification/clippy-fix, atop the state-2 PLAN-write `c699ccfae` (excluded as the base).
  The CODE commits reviewed: `0c46b7bc1` (T1 config schema+validator) → `1e37cf4bc` (T2 H1 pending
  gate + `PendingOverflow` + `upstream_rq_pending_overflow`) → `db3ff1af6` (T3 H1 `upstream_cx_overflow`
  + edge-driven `cx_open` gauge) → `9f284759c` (T4 H1 `synth_overflow` + both router arms) →
  `c32f2bfe8` (T5 H2 pool+HCM mirror incl. the C-1 502→503 correction) → `46963f8e4` (T6 fixture
  0023 + Docker wrapper) → `47b878037` (T7 fixture 0020 inert-0 extension) → `bd730f4e9` (T8
  in-process backstop) → `2b98b5251` (T9 fuzz-seed in-place extension + BEHAVIOR_CONTRACT rows) →
  `655cea7e1` (T10 clippy `collapsible_if` → let-chain refactor at 8 `cx_open` sites). The
  PROGRESS-subsection / PROGRESS-accuracy / STATE-advance commits in the range carry NO code.
- **Pre-review HEAD:** `d2558fc3e` (== `origin/main`; `git rev-list --left-right --count HEAD...origin/main` → `0  0`).
- **Method:** 4 read-only code-review subagents, one per concern-cluster, dispatched **SERIALLY**
  (`feedback_serial_subagent_dispatch`), each reading the actual on-disk diff (`git show` + `Read`
  + `Grep`) and returning per-cluster verdicts. The controller synthesized this REVIEW and
  performed independent out-of-band verification (the §7.5 gate re-attestation against the CI
  record + a literal `wc -c` body byte-count + a grep of the inert-registration gate).
- **Verdict: APPROVED** (zero Critical / zero Important / Minor-only follow-ups carried). Mirrors
  the 14.1 state-5 `Approved with M-track follow-ups` shape (`e0ba8d01`).

---

## 1. The named review focus (PLAN Task 11 — the items this session MUST verify)

### 1.1 `cx_open` edge-correctness (the 8 gauge sites) — **VERIFIED CORRECT**

The `circuit_breakers.default.cx_open` gauge is **edge-driven, not polled**, and every edge runs
under the same `established` lock that guards the counter it reacts to (no gauge/counter race; no
lock held across `.await` — the connect always happens after the lock scope closes). The four
`established`-mutation edges in each pool all set/clear consistently:

- **H1** (`crates/envoy-http1/src/pool.rs`): rising `:295-299` (lock at `:280`), rollback
  `:311-315` (`:306`), Drop-destroy `:191-195` (`:186`), sweeper `:397-401` (`:391`).
- **H2** (`crates/envoy-http2/src/pool.rs`): rising `:396-400` (`:381`), rollback `:412-416`
  (`:407`), Drop-invalidate `:177-181` (`:172`), sweeper `:547-551` (`:541`).

The at-cap comparison is **inclusive** and correct: rising uses `*n >= max_connections` after
`*n += 1`; falling uses `*n < max_connections` after `saturating_sub`. With `max_connections == 1`:
connect → `n == 1` → `set(1)`; decrement → `n == 0` → `set(0)`. **Terminal-0 holds** — when
`established` drains to 0 the gauge is 0; it cannot get stuck at 1. The unit test
`cx_overflow_increments_and_cx_open_tracks_cap_edges` (H1 `:828`, H2 mirror) asserts the gauge
rises to 1 at cap, stays 1 across an overflow attempt, and returns to 0 at the Drop decrement edge
— covering the rising edge, at-cap hold, AND the falling edge.

### 1.2 `max_pending_requests:0` reject ordering — **VERIFIED CORRECT**

The pending gate fires in `acquire()` **after** the idle/slot-reuse phase but **before** the
cap-check (`crates/envoy-http1/src/pool.rs:270-277`, `crates/envoy-http2/src/pool.rs:370-377`;
comment at H1 `:266` documents the ordering). Consequences confirmed:

- A request hitting an **existing idle** connection/stream returns early (H1 `:248-261`, H2
  `:338-361`) and **never** trips the gate — the gate is on the connect-on-miss path only.
- On the pending-reject path `upstream_cx_overflow` is **never reached**, so it stays **0** — no
  connection demand reaches the cap (the central ordering invariant). `upstream_rq_pending_overflow`
  increments at exactly ONE site each (H1 `:271-273`, H2 `:371-373`) before returning
  `PoolError::PendingOverflow`.

### 1.3 H2 502→503 overflow-arm correction (C-1) — **VERIFIED CORRECT + PARITY WITH H1**

The H2 overflow arm now emits **503** (`crates/envoy-http2/src/hcm.rs:744`) via `synth_h2_overflow()`
through a `finalize_h2_stream` early-return (mirroring the established `synth_h2_502` early-return
at `:241-245`). The old **502 is fully gone from the overflow path** — `502` now appears in the H2
HCM only in historical comments (`:374`, `:376`) and on the genuine no-healthy/connect/send-fail
paths (`synth_h2_502` at `:242`, `:445`). Both H2 arms are wired: `Overflow` (`:384`) and
`PendingOverflow` (`:408`). Parity with H1 holds (same 503, same body, same `x-envoy-overloaded`).

### 1.4 81-byte overflow-503 body byte-exactness + `x-envoy-overloaded` — **VERIFIED CORRECT**

The body literal `upstream connect error or disconnect/reset before headers. reset reason: overflow`
is **exactly 81 bytes** (controller re-verified independently: `printf '%s' '…' | wc -c` → `81`),
no trailing newline, correct punctuation. Built via `Bytes::from_static(b"…")` in both
`crates/envoy-http1/src/hcm.rs:1116-1118` and `crates/envoy-http2/src/hcm.rs:740-742` — the two
literals are byte-identical. `content-length` is set from `body.len().to_string()` (H1 `:1125`,
H2 `:749`), so the declared length can never drift from the emitted bytes. `x-envoy-overloaded: true`
present on both; the H1-conventional `connection` header is present on H1 and correctly **omitted**
on H2 (H2-illegal connection-specific header). Both H1 router arms (`Overflow` `:560`,
`PendingOverflow` `:574`) route to `synth_overflow`. H1 unit test
`synth_overflow_emits_81_byte_body_and_x_envoy_overloaded` asserts status 503, exact body bytes,
`len()==81`, and the header.

### 1.5 Inert-when-unconfigured stat registration — **VERIFIED CORRECT (22 fixtures unaffected)**

All three stats register ONLY when `cfg.circuit_breakers.is_some()` — confirmed at
`crates/envoy-http1/src/pool.rs:479` (`upstream_rq_pending_overflow`), `:494` (`upstream_cx_overflow`),
`:502` (`circuit_breakers.default.cx_open`), and the H2 mirror. The handles are
`Option<Arc<Counter/Gauge>>`; a `None` handle is a silent no-op. A cluster without `circuit_breakers`
registers NO stat AND its pool's `max_pending_requests` defaults to `DEFAULT_MAX_PENDING_REQUESTS =
1024` so the gate never fires — so the 22 pre-existing Docker-gated fixtures (0001–0022) see **zero
behavior change** (acceptance gate (b)). Fixture 0020 (which DOES configure `circuit_breakers`,
`max_connections: 4`) proves the registered-but-inert path: the new bilateral `== 0` assertions on
`upstream_cx_overflow` + `cx_open` read genuinely-registered zeros, not absent stats.

### 1.6 The `655cea7e1` clippy let-chain refactor — **VERIFIED BEHAVIOR-IDENTICAL**

The refactor collapsed nested `if *n >= max { if let Some(g) = &cx_open { g.set(v) } }` into the
let-chain `if *n >= max && let Some(g) = &cx_open { g.set(v) }` at all 8 sites (4 per pool). `&&`
short-circuits identically to the nested `if`; the comparison operators (`>=` rising, `<` falling)
and the set values (`1` rising, `0` falling) are unchanged at every site (verified line-by-line
against the Task-3/T5 originals). `envoy-http1` 87 + `envoy-http2` 57 crate tests pass. This is a
mechanical, behavior-preserving collapse.

---

## 2. Cluster verdicts (all CLEAN)

| Cluster | Scope (commits) | Verdict |
|---|---|---|
| A — config schema + validator | T1 `0c46b7bc1` | CLEAN |
| B — pool stats + pending gate + let-chain | T2/T3/T5/T10 `1e37cf4bc`,`db3ff1af6`,`c32f2bfe8`,`655cea7e1` | CLEAN |
| C — overflow-503 wire reconciliation | T4/T5 `9f284759c`,`c32f2bfe8` | CLEAN |
| D — fixtures + backstop + fuzz seed | T6/T7/T8/T9 `46963f8e4`,`47b878037`,`bd730f4e9`,`2b98b5251` | CLEAN |

**Selected strengths.** Validator boundary is exactly right (accept `None`, accept `Some(0)`,
reject `Some(n>0)` via the `value > 0` let-chain guard — no off-by-one; `bootstrap.rs:2619-2626`),
and the C-2 deny-unknown re-point genuinely still rejects (`max_requests` is never a struct field).
Single-source-of-truth counters (one `inc()` site each). Fixture 0023 is timing-robust by
construction: a single keep-alive GET trips the gate on the first connect-on-miss (no idle
connection to bypass it), the reject fires in pool `acquire()` *before* any connect so the backend
address is irrelevant — no concurrency or slow-backend flake surface. The in-process backstop drives
both paths end-to-end through `envoy-bin` (not mocked); the cx-overflow K=2 path is deterministic —
`max_connections:1` + an 800ms backend hold guarantees the second `acquire` sees `established == cap`
with no idle stream, so it must overflow, and the `{200,503}` multiset is order-insensitive
(`sort_unstable`). The fuzz seed's in-place `max_pending_requests: 0` edit genuinely flips the seed
from the `None` short-circuit into the validator's accept-path evaluation (a real new parse-path
exercise, corpus stays 22).

---

## 3. Minor findings (non-gating; carried forward — no named owner)

None gate the phase. Recorded for future phases:

- **M15-1 (config test gap).** No dedicated accept-absent (`None`) test and the reject test only
  asserts `msg.contains("max_pending_requests")`. The None branch is implicitly covered by existing
  `max_connections`-only fixtures. Optional: add a dedicated None-accept assertion + assert the
  "only 0" phrasing. (`crates/envoy-config/src/bootstrap.rs` tests.)
- **M15-2 (DRY — cross-crate body literal).** The 81-byte overflow string is duplicated 3× with no
  shared const (`envoy-http1/src/hcm.rs:1117` helper, `:3346` test; `envoy-http2/src/hcm.rs:741`
  helper). Bytes are currently identical and verified, but a future edit to one could silently
  desync H1↔H2. Optional: hoist to a single `pub const OVERFLOW_BODY: &[u8]` in a shared location.
- **M15-3 (H2 wire-shape test gap).** No H2 unit test for `synth_h2_overflow` (consistent with the
  pre-existing `synth_h2_502` convention — not a regression). Optional: mirror the H1 test (assert
  503, `len()==81`, `x-envoy-overloaded`, and crucially **no `connection` header**) to lock the H2
  wire shape against cross-crate drift.
- **M15-4 (harness caveat — absent-as-0).** `tests/differential/src/lib.rs:1832` `scrape_admin_stat`
  returns `Ok(0)` for an absent stat, so a differential inert-0 assertion is only non-vacuous
  *because* the stat happens to be registered (true for 0020/0023, verified via the
  `circuit_breakers.is_some()` gate). The in-process backstop already distinguishes present-and-0
  from absent (`assert_stat` panics on absent). Optional: a presence-required differential variant,
  or a comment in 0020's expectations noting the inert-0 guarantee rides on registration.
- **M15-5 (backstop timing constants).** The cx-overflow backstop's 800ms hold / 600ms poll margins
  are load-bearing (600 < 800 keeps the mid-flight `cx_open == 1` scrape inside the hold window).
  Sound today; first thing to widen if CI slows. Optional: a comment pinning *why* 600 < 800.

### Process note (NOT a defect) — recorded per memory `project_state3_arc_skips_clippy`

The state-3 per-task arc ran build/test/fmt but **not** clippy, so 8 `clippy::collapsible_if` lints
(the Task-3/T5 `cx_open` nested-`if` blocks) first surfaced at the state-4 gate (fixed in
`655cea7e1`, behavior-identical). Future state-3 arcs should run the full
`cargo clippy --workspace --all-targets --all-features -- -D warnings` per task.

---

## 4. Carryforward dispositions (none engaged by phase 15; PLAN §0.D)

- **Per-endpoint-vs-per-cluster `cx_open` reconciliation** — the gauge is one per-cluster but
  `established` is a per-endpoint `HashMap`; with multiple endpoints under one circuit-breaker gauge
  the falling edge of endpoint B could clobber endpoint A's still-at-cap `set(1)`. **Correct only
  because every `circuit_breakers`-configured fixture (0020/0021/0023) is single-endpoint** (lock-in
  #6, documented deferral). NOT a phase-15 defect; owner = a future multi-endpoint phase. A future
  fix sums `est.values()` rather than testing the single just-mutated entry. **Carried forward.**
- **`max_pending_requests > 0` pending-request QUEUE** — deferred; phase 15 implements only the
  `:0` no-queue reject path (the validator rejects `>0` so nothing is silently under-implemented).
- **The `{200,200}` bilateral overflow fixture** — deferred pending-queue phase.
- **Standing multi-phase Minor inventory** (14.1 REVIEW M1/M2/M3/M7/M9; M-c1 `tokio-util` `["rt"]`
  leanness; M-c2 `.lock().unwrap()` poison-hardening; M-c3 frozen-record "14"s), the §6.9 per-class
  `upstream_rq_{2,3,4}xx` extension, the `upstream_cx_total` TCP-proxy carve-out, and **ADR-0028**
  (H1-listener × H2-cluster dispatch deferral, REMAINS OPEN) — all carried forward unchanged; phase
  15 engages none.

---

## 5. §7.5 phase-done gate re-attestation (from the CI record — Docker NOT re-run)

Re-attested against CI run `26717619099` (HEAD `655cea7e1`, `conclusion=success`,
`2026-05-31T16:08:43Z`; the commit carrying all phase-15 code) per the 14.2 evidence-discipline
(`c1b2e022e`) — `BOOTSTRAP_PROMPT.md` §0.C / next-prompt mandate: do NOT re-run the Docker
differential.

- **(a) new/changed differential fixtures green:** fixture 0023 (`max_pending_requests:0` →
  bilateral 503 + 81-byte body + `x-envoy-overloaded` + `upstream_rq_pending_overflow:1`) +
  fixture 0020's new inert-0 assertions. **Satisfied** (CI `test (Docker differential)` step success).
- **(b) pre-existing fixtures still green:** all 22 prior fixtures (0001–0022) bilaterally green vs
  `envoyproxy/envoy:v1.33.0`; the inert-when-unconfigured registration gate (verified §1.5) means
  zero behavior change. **Satisfied.**
- **(c) conformance threshold:** h2spec ≥95% (vacuous — no H2 framing touched). **Satisfied.**
- **(d) fuzz short-budget:** `parse_bootstrap` `Done 417097 runs`, 0 crashes, on the extended
  22-seed corpus. **Satisfied.**
- **(e) build/clippy/fmt/test/deny:** `cargo build --workspace --all-targets`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean; the 3
  standalone-crate builds (`-p envoy-config`/`-p envoy-http1`/`-p envoy-http2`) green (lock-in #14,
  the `project_isolated_crate_build_blindspot` guard). **Satisfied.** (Known env flakes: the 13.2
  `upstream_h2_connection_pooling` in-process backstop env flake + the `0012` access-log CI flake —
  both proven environmental, CI-green; not regressions.)
- **(f) REVIEW.md approved:** THIS document. **Satisfied.**

The Task-10 docs/STATE commit `d2558fc3e` (current HEAD) is also CI-green (run `26717813408`,
`conclusion=success`).

---

## 6. ADR projection

**No new ADR.** A code review is docs-only and projects no ADR (PLAN lock-in #2); it surfaces an
ADR only on a genuine unforeseen constraint — none arose. DECISIONS.md ledger head stays **ADR-0043**
(next available **ADR-0044**, the reserved split that did NOT fire). `DECISIONS.md` is unmodified in
the review range. **ADR-0028 remains OPEN** (owner = a follow-up foundations-pivot phase, NOT 15).

---

## 7. Verdict + next state

**APPROVED.** All production-code logic is sound: the edge-driven `cx_open` gauge is correct at all
8 sites with verified terminal-0, the `max_pending_requests:0` gate fires before the cap-check so
`upstream_cx_overflow` stays 0 on the pending-reject path, the H2 502→503 correction lands with
H1 parity, the overflow body is byte-exact 81 bytes identical across H1/H2 with the correct
per-protocol header set, the inert-when-unconfigured registration leaves the 22 prior fixtures
unchanged, and the clippy let-chain refactor is behavior-identical. The five Minor findings are
non-gating optional follow-ups with no named owner. **Zero Critical / zero Important** — no state-3
re-entry required (`BOOTSTRAP_PROMPT.md` §5.2 does not trigger).

**Next state — state 6 (deterministic close-out), a LATER session** (per `BOOTSTRAP_PROMPT.md` §5.1
one-state-per-session; the 14.1/14.2 precedents stop after REVIEW.md lands). The state-6 session
flips ROADMAP row `15` `in-progress → done` (a non-split top-level phase flips its own row alone),
advances STATE.md to "awaiting next planning", appends the `### Phase-15 rollovers` Notes subsection,
and commits with the §5.3 final-phase format: `phase 15: circuit breakers — observability +
max_pending_requests:0 reject [ADR-0043]`. The ROADMAP is NOT flipped in this state-5 commit.
