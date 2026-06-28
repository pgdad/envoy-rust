# Phase 49 — `49-accesslog-rf-no-healthy` — STATE-5 Code Review

**Reviewer:** fresh `superpowers:code-reviewer` subagent (no session history), dispatched over the phase-49 implementation diff `git diff 2843ddf 3aa3183` (7 files; the only `src/` change is the one-arm `match` extension of the H1 `%RESPONSE_FLAGS%` derive in `crates/envoy-http1/src/hcm.rs` plus the in-process backstop test; the rest is fixture `0057` [README + envoy-rust.yaml + envoy.yaml + expectations.yaml] / the differential test `access_log_rf_no_healthy.rs` / the BEHAVIOR_CONTRACT `%RESPONSE_FLAGS%` row) against the approved `SPEC.md` / `PLAN.md` / project doctrine. The reviewer was instructed to distrust the brief and independently verify every correctness claim against the live tree. The §7.5 verification gate was already green in CI (state-4: run `28336975751` — the Docker differential incl. `0057` + h2spec + fuzz + cargo-deny) — this review is correctness + quality, not a re-verification.

## Overall verdict: ✅ APPROVED (ready for state-6 close-out)

A surgical, additive, provably-1:1 single-arm derive extension — the thirteenth Observability access-log row and the SECOND non-`-` `%RESPONSE_FLAGS%` witness (`UH`/NoHealthyUpstream, the sibling of phase 48's `NR`). No defects at Critical or Important; two Minors, both line-citation drift in doc comments (the project's recurring M48-1/M49-1 lineage) — both non-blocking → carry-forward.

## Findings by severity

- **Critical:** none
- **Important:** none
- **Minor:** two (non-blocking → carry-forward as **M49-2** [derive-comment `route_not_found` citation drift] + **M49-3** [`:1232` derive-site citation drift across backstop docstring / differential header / README / expectations])

## Counts

`0 Critical / 0 Important / 2 Minor`

## What was verified (confidence: high)

**1. The derive is provably 1:1 with the no-healthy 503 path (the check that matters most).** Verified by exhaustive grep: `response_code_details_for_log` is set to `"no_healthy_upstream"` at EXACTLY one site (`hcm.rs:1000-1001`, the `pick()->None` no-healthy synth-503 arm). The only other RCD producers are `via_upstream` (`:995`) and the `BuildOutcome::Synth` sites threaded via the writer-arm at `:866` (`direct_response` / `route_not_found` / `None`). None aliases `no_healthy_upstream`, so the new `Some("no_healthy_upstream") => "UH"` arm fires iff that 503 path is taken — `UH` can neither spuriously appear on another path nor be missed on this one.

**2. Borrow/move soundness confirmed.** The `match response_code_details_for_log.as_deref()` shared borrow ends before the owned `String` is moved into the `response_code_details:` record field below (`:1266`). Independently type-checked clean (compiles + clippy `-D warnings`).

**3. The `route_not_found => "NR"` arm is preserved verbatim.** The two `synth_404` arms (`:1553` host-miss, `:1572` route-miss) still flow to `"NR"` unchanged — fixture `0056` and the phase-48 backstops remain byte-identical. The three-arm `match` did not alter `NR` behavior.

**4. The in-process backstop genuinely pins the byte-exact line and fails-first.** `h1_no_healthy_access_log_carries_uh_flag` asserts the exact `{"rc":503,"rcd":"no_healthy_upstream","rf":"UH"}\n` and was RED (`rf:"-"`) before the arm per PROGRESS. It also asserts the 503 status/body are unchanged, guarding additivity at the unit level.

**5. The `0001`-`0056` byte-identical invariant holds.** `grep -rln RESPONSE_FLAGS tests/fixtures/` confirms only `0012`/`0040`/`0046`/`0056` (+ new `0057`) log the operator; none of the prior four drives a no-healthy 503, and the no-healthy fixture `0053` does not log `%RESPONSE_FLAGS%`. Zero existing-fixture bytes change.

**6. Fixture `0057` fidelity is tight.** `diff envoy.yaml envoy-rust.yaml` shows exactly the documented benign per-side deltas (admin block, `0.0.0.0` vs `127.0.0.1` bind, mount path) and nothing else — route/cluster/subset/`json_format` are byte-identical. One probe `GET /`, `expected_status: 503`, pure cross-proxy equality on `rf:"UH"`.

**7. Scope discipline is clean.** Single `src/` change is the one-arm `match` extension; no new `Op`/`AccessLogRecord` field/enum variant/crate/dependency/fuzz-target/`ConfigError` variant; H1-only (H2 deferred — M45-1); `#![forbid(unsafe_code)]` untouched.

## Minors (non-blocking → carry-forward)

**M49-2 — derive-comment `route_not_found` citation drift (continues the M48-1 lineage).** The new derive comment (`hcm.rs:1227-1229`) states the `route_not_found` arms are at "host-miss `:1536` + route-miss `:1555`", but the actual `synth_404` sites that set `Some("route_not_found")` are `:1553` (host-miss) and `:1572` (route-miss) — both off by ~17 lines. These were carried over verbatim from the phase-48 comment without re-verification (the PLAN's "M48-1 ACTIONED" re-verified only the no-healthy/derive sites, not these carried-over `route_not_found` citations). Doc-comment-only; no behavioral effect. **Carry-forward; fix opportunistically when the no-route arms are next edited — or drop the absolute numbers in favor of the `synth_404` symbol, which does not drift.**

**M49-3 — `:1232` derive-site citation drift across docs.** The backstop docstring (`hcm.rs:5379`), the differential test header (`access_log_rf_no_healthy.rs:9`), the fixture `README.md`, and the `expectations.yaml` comment all cite the derive at `hcm.rs:1232`. Pre-edit the `if/else` was at `:1232`; post-edit the comment block grew and the `match` now begins at `:1237` (`:1232` now points into the comment). The BEHAVIOR_CONTRACT correctly used the stable `:1225` record-build anchor (M49-1 ACTIONED at state-3) — the other docs did not adopt that convention. Doc-comment-only. **Carry-forward; apply the `:1225` anchor convention uniformly (or use symbol references) when the surface is next touched.**

## Recommendations

- Apply the M49-1 `:1225` record-build anchor convention uniformly (backstop docstring, differential header, README, expectations comment) so future insertions above the derive don't re-stale every citation.
- Consider replacing absolute line numbers in long-lived doc comments with symbol references (`synth_404` arms, the `pick()->None` arm) — the entire drift class disappears.

## Outcome

APPROVED with 0 blocking issues. Phase 49 is ready to advance to **state-6 close-out** (`superpowers:finishing-a-development-branch`): flip ROADMAP row `49` → `done`, fold M49-2 + M49-3 (and the still-live carry-forwards M48-2 / M42-1 / M45-1 / M45-2 / older) forward, relocate the active-phase Notes subsection to STATE_HISTORY.md per ADR-0035. M48-1/M49-1 line-citation lineage CONTINUED (M49-2/M49-3 are the same surface class). M42-1 CONTINUED. `#![forbid(unsafe_code)]` holds.
