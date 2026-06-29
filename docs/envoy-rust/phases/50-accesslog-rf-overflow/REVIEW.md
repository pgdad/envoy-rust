# Phase 50 — `50-accesslog-rf-overflow` — REVIEW

**State-5 code-review** (`superpowers:requesting-code-review` → a FRESH `superpowers:code-reviewer` subagent, independent of the implementation session's context). **Verdict: ✅ APPROVED — Ready to merge: YES.**

- **Range reviewed:** `git diff b57820b..b200ac3` (6 commits: `49814f7` T1 §A pool-overflow `outcome:None` discriminator + §B derive arm, `e74264a` T2 §A′ request-budget arm tag, `8653138` T1/T2 comment line-citation precision, `ba5b206` T3 fixture `0058` + differential test, `e30586f` T4 BEHAVIOR_CONTRACT §E, `b200ac3` docs-only state marker). State-4 verification commit `0ff0d7f` correctly EXCLUDED from range.
- **Reviewed against:** `SPEC.md` (§2 §A-§F, §3 PLAN-VERIFY, §5 acceptance), `PLAN.md` (4 tasks), `PROGRESS.md`, and project doctrine (additive/byte-identical `0001`-`0057` invariant, `#![forbid(unsafe_code)]`, NO new `Op`/`AccessLogRecord` field/crate/dependency/fuzz-target/`ConfigError` variant, exact-string coupling, discriminator soundness, borrow discipline, M49-2/M49-3 line-citation precision).

## Outcome

| Severity | Count | Blocking? |
|----------|-------|-----------|
| Critical | 0 | — |
| Important | 0 | — |
| Minor | 3 | No (all advisory / already-tracked) |

**No code changes required.** The reviewer independently verified every load-bearing claim against the tree (not on trust).

## Strengths (reviewer-confirmed, independently verified against the tree)

- **Discriminator soundness — sound.** `grep` of all `AttemptResult` constructions confirms `outcome: None` appears at EXACTLY two sites — `hcm.rs:439` (`endpoint:None`, no-healthy) and `hcm.rs:640` (`endpoint:Some`, overflow). Inside the `if let Some(endpoint)` branch (`:1000`), the no-healthy arm is excluded by the `Some` guard, so `attempt.outcome.is_none()` is UNIQUELY the pool-overflow result (success `:600` / reset `:620` / connect-fail `:629` all carry non-`None` outcomes). SPEC §3.1 / PLAN recon holds exactly.
- **Exact-string coupling — byte-identical.** `grep -on "upstream_reset_before_response_started{overflow}"` shows the two set-sites (`hcm.rs:933` budget arm, `:1021` pool discriminator) and the derive arm (`:1277`) are character-for-character identical, braces included → NO silent `rf:"-"` mismatch risk.
- **Additive invariant — holds.** `0058` is the ONLY fixture combining an overflow config with an `access_log` sink; every other `endpoint:Some` path resolves to `Some(Response)` → still `via_upstream`; the new derive arm is an exact-string match. `0001`-`0057` cannot change a byte. The `0058` paired-YAML divergence matches the `0057` reference shape (admin + bind addr + mount path) exactly.
- **Borrow discipline correct.** `:1274` `.as_deref()` shared borrow ends at `.to_owned()` (`:1280`) before the owned `String` moves into `response_code_details:` (`:1304`) — the phase-48/49 pattern.
- **Doctrine invariants hold.** No `unsafe` (only the literal `#![forbid(unsafe_code)]` token); no Cargo.toml/Cargo.lock change → no new crate/dep; no new `Op`/`AccessLogRecord` field/`ConfigError` variant/fuzz-target.
- **PLAN fidelity exact, including documented deviations.** The `BudgetAcquisition` path is `envoy_cluster::` (not the `envoy_config::` plan prose) — honestly recorded in PROGRESS Task 2. The `0058` paired-YAML divergence shape correction is recorded in PROGRESS Task 3.
- **Line-citation precision (M49-2/M49-3) actioned.** In-code derive comment (`:1259-1272`) + budget-arm comment (`:924-931`) cite current post-edit anchors; BEHAVIOR_CONTRACT overflow row re-anchored `:542`/`:569` → `:508`/`:515`.

## Minors (advisory — NO action taken, with reasoning per `superpowers:receiving-code-review`)

1. **`hcm.rs:1007` comment cites `hcm.rs:640`** for the overflow `AttemptResult`. **Verified correct as of this HEAD** (the reviewer confirmed `outcome:None` is at `:640` on disk now). Advisory only — flagged because `:640` was the one anchor in that comment not in PROGRESS's post-T2 re-verified anchor list. No drift exists; no action needed unless a future edit shifts `run_attempt`. → **Accepted as-is (currently accurate).**
2. **`0058` README not content-verified beyond existence.** Non-load-bearing prose; the four load-bearing files (envoy.yaml, envoy-rust.yaml, expectations.yaml, differential test) were read in full and are correct; PROGRESS Task 3 reports the README as a `0057`-structure clone. → **Accepted (non-load-bearing).**
3. **M50-C — the request-budget (`max_requests`) overflow arm (`hcm.rs:932`)** is tagged with the same `upstream_reset_before_response_started{overflow}` rcd as the pool path, but that string was recon-confirmed against live Envoy ONLY on the `max_pending_requests` pool path; the budget arm is in-process-backstopped only, NOT differentially witnessed. A sound, DISCLOSED deferral — already tracked as carry-forward **M50-C** (the unverified part is the rcd STRING, not just the `UO` flag: re-recon before witnessing). → **Accepted (already tracked); no action.**

## Assessment (verbatim verdict)

**Ready to merge? YES.** The implementation is a faithful, minimal realization of the ADR-0107 scope. The `attempt.outcome.is_none()` discriminator is provably 1:1 with the pool-overflow result; the §B exact-string arm is byte-identical to both set-sites; the additive invariant holds structurally (`0058` is the only overflow+access-log fixture); `#![forbid(unsafe_code)]` holds with no new crate/dep/`Op`/field/fuzz-target/`ConfigError` variant; the only `src/` deltas are the two set-sites + one derive arm (plus two backstop tests + comment re-anchoring). The diff matches the PLAN's four tasks exactly, the two documented plan-prose deviations are honestly recorded in PROGRESS, and the M50-C deferral is correctly scoped and tracked. State-4 already CI-green on `b200ac3` (run `28365357127`, both jobs success). No blocking or important issues; the three minors are advisory.

---

_State-5 complete → phase 50 advances to **state-6 close-out** NEXT (the SESSION AFTER, per §5.1 + memory `closeout-and-pick-are-separate-sessions`): flip ROADMAP row `50` → `done`, relocate the active-phase Notes to `STATE_HISTORY.md`, STATE → awaiting-next-planning. Carry-forward consumption at close: phase 50 CONSUMES the `UO` slice of M45-2 (+ the overflow-rcd-deterministic refinement) + M49-3 (overflow-row + `:1225` anchor) + M49-2 (derive-comment `synth_404` citations); NEW carry-forward M50-C; M48-2/M42-1/M45-1/connect-failure-M45-2/older stay live._
