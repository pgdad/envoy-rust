# Phase 51 — `51-accesslog-rf-retry-exhausted` — REVIEW

**State-5 code-review** (`superpowers:requesting-code-review` → a FRESH `superpowers:code-reviewer` subagent, independent of the implementation session's context). **Verdict: ✅ APPROVED — Ready to merge: YES.**

- **Range reviewed:** `git diff d928012..1bda589` (base = phase-51 state-2 PLAN commit `d928012`; code starts at `88725cf`). Implementation commits: `88725cf` (T1 §A boolean + §B derive wrapper), `890a2cc` (T1 rustfmt of the backstop `assert_eq!`), `25c656d` (T2 fixture `0059` + differential test + the 2 fixture-name-gated backend-wiring edits in `tests/differential/src/lib.rs`), `800c756` (T3 BEHAVIOR_CONTRACT §E). The later commits `9085e39`/`2c46cd5`/`1bda589` are PROGRESS.md/STATE/deps housekeeping (the `2c46cd5` anyhow 1.0.102→1.0.103 bump cleared the unrelated RUSTSEC-2026-0190 advisory) — correctly NON-load-bearing for the code surface.
- **Reviewed against:** `SPEC.md` (§2 §A-§F, §3 PLAN-VERIFY, §5 acceptance), `PLAN.md` (3 tasks), `PROGRESS.md`, `BEHAVIOR_CONTRACT.md` §E, and project doctrine (additive/byte-identical `0001`-`0058` invariant, `#![forbid(unsafe_code)]`, NO new `Op`/`AccessLogRecord` field/crate/dependency/fuzz-target/`ConfigError` variant, discriminator soundness, NO rcd change, borrow discipline, test-not-mock).

## Outcome

| Severity | Count | Blocking? |
|----------|-------|-----------|
| Critical | 0 | — |
| Important | 0 | — |
| Minor | 1 | No (spec-sanctioned doc-anchor convention — already-tracked discipline) |

**No code changes required.** The reviewer independently verified every load-bearing claim against the tree (not on trust).

## Strengths (reviewer-confirmed, independently verified against the tree)

- **Scope discipline exemplary.** The `src/` change is exactly the SPEC/PLAN-locked shape: one `let mut retry_limit_exceeded_for_log = false;` local (`hcm.rs:852`), one set-site (`:1152`), one derive wrapper (`:1305`). NO new `Op`/`AccessLogRecord` field, NO new crate/dep, NO new fuzz target, NO `ConfigError` variant; `#![forbid(unsafe_code)]` untouched.
- **Set-site provably 1:1 with the L9 path (invariant #2).** At `hcm.rs:1136-1156` the boolean is set inside `if attempts > 1 && !retry_budget_blocked { if final_retriable { … } }`, co-located with `cluster.upstream_rq_retry_limit_exceeded().inc()`. The reviewer traced the loop body (`:1064-1112`): the only normal `break` is reached when `final_retriable && attempts <= max_retries` is false or after a `BudgetAcquisition::Rejected` sets `retry_budget_blocked = true`. Therefore at the post-loop split `final_retriable == true && attempts > 1 && !retry_budget_blocked` can only mean `attempts > max_retries` — the budget genuinely consumed with a retriable final attempt. **No false-positive** (boolean can't set without real exhaustion) and **no false-negative** (every L9 exit hits this exact gate, the same gate as the counter Envoy increments for `URX`).
- **Excluded paths correctly excluded.** The retry-BUDGET-blocked exit is gated out by `!retry_budget_blocked` (`:1136`); the pre-loop request-budget overflow bypasses the loop entirely (already tagged `…{overflow}` → `UO`). Both correctly deferred (M45-2 / phase-50), matching SPEC §4. The in-code comment `:1148-1151` documents this accurately.
- **Genuinely additive — no rcd change (invariants #1, #3).** The derive wrapper (`:1305-1318`) only prepends `if retry_limit_exceeded_for_log { "URX" } else { <verbatim NR/UH/UO/`-` match> }`; the inner arms are byte-identical to the prior code. No `response_code_details_for_log` arm touched; `via_upstream` already matches Envoy. Boolean defaults `false` + no existing access-log fixture carries a `retry_policy` (PLAN §6.2 disjointness proof) → `0001`-`0058` render identically.
- **Borrow/move sound (invariant #6).** The new `bool` is `Copy`, read by value in the `if` condition; `response_code_details_for_log.as_deref()` remains a shared borrow that ends before the owned `String` moves into the `response_code_details:` field. No shadowing, no shortcut; `.to_owned()` applied to the unified `&str` if/else result — correct.
- **Test quality real, not a mock (invariant #5).** The backstop `h1_retry_limit_exceeded_access_log_carries_urx_flag` (`hcm.rs:7223+`) drives a real always-503 upstream (`spawn_fail_then_ok_upstream(503, 1000)`), a real `retry_policy{retry_on:"5xx", num_retries:1}`, a real `FileSink` + `CompiledJsonFormat`, and asserts the wire response is `HTTP/1.1 503` AND the logged line is exactly `{"rc":503,"rcd":"via_upstream","rf":"URX"}\n`. It exercises the actual loop-exit; PROGRESS records the fail-first state (`rf:"-"`). The differential test + fixture drive both proxies through the health-aware backend (`needs_health_aware_backend` allowlist + `/retry-exhausted=503` per-path arm both added for `0059`); the reference `envoy.yaml` correctly uses a LITERAL `port_value: 0` (plan-review C1), avoiding the unresolved-`{{ADMIN_PORT}}` trap.

## Minors (advisory — NO action taken, with reasoning per `superpowers:receiving-code-review`)

1. **`BEHAVIOR_CONTRACT.md:1020` — the new `URX` rule cites the H1 record-build derive as `(hcm.rs:1225)` while the live `response_flags:` derive is at `hcm.rs:1305`.** This is NOT a defect introduced by this phase: it is the established carry-forward anchor convention — the NR/UH/UO rules in the same cell already cite `:1225`, and SPEC §E explicitly instructed "KEEP the `:1225` record-build-site anchor convention." It is therefore internally consistent and within the documented M48-1/M49-3 discipline. The new `URX` rule should move together with the NR/UH/UO rules if the project ever reconciles these anchors. → **Accepted as-is (spec-sanctioned; batched into the existing anchor-reconciliation carry-forward — see M51-1 below). No action this phase.**

## Recommendations

- None blocking. Optionally, a future doc-hygiene pass could replace the shared `hcm.rs:1225` anchor (now used by NR/UH/UO/URX) with the actual `:1305` derive line or a symbolic anchor, since the in-code comments already use symbolic references (the `upstream_rq_retry_limit_exceeded` gate) and only the contract retains the raw line. Tracked as **M51-1**.

## Assessment (verbatim verdict)

**Ready to merge? YES.** All six load-bearing invariants are verified in the tree: the set-site is provably 1:1 with the L9 retry-limit-exceeded exit (no false-positive/negative), the change is strictly additive (rcd untouched, `0001`-`0058` byte-identical), scope is exactly the locked one-boolean-plus-wrapper, the backstop and differential tests exercise real paths, and the borrow discipline is sound. The single finding is a pre-existing, spec-sanctioned doc-anchor convention, not a code or behavior defect. State-4 already CI-green on HEAD `2c46cd5` (Docker differential `0059`+`0001`-`0058` + h2spec + fuzz + deny). No blocking or important issues.

---

_State-5 complete → phase 51 advances to **state-6 close-out** NEXT (the SESSION AFTER, per §5.1 + memory `closeout-and-pick-are-separate-sessions`): flip ROADMAP row `51` → `done`, relocate the active-phase Notes to `STATE_HISTORY.md`, STATE → awaiting-next-planning. Carry-forward consumption at close: phase 51 CONSUMES the `URX` slice of M45-2 (leaving the connect-failure `UF`/`DC` + retry-BUDGET-overflow slices live); NEW carry-forward **M51-1** (the `%RESPONSE_FLAGS%`-rule line-anchor `:1225`→`:1305` reconciliation, to be batched with the NR/UH/UO anchors); M50-C / M48-2 / M42-1 / M45-1 / older stay live._
