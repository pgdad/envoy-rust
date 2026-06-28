# Phase 48 — `48-accesslog-rf-no-route` — STATE-5 Code Review

**Reviewer:** fresh `superpowers:code-reviewer` subagent (no session history), dispatched over the phase-48 implementation diff `git diff 28e9fd1 8c62e5c` (10 files; the only `src/` change is the single field-expression edit in `crates/envoy-http1/src/hcm.rs` plus two backstop tests; the rest is fixture `0056` / differential test / BEHAVIOR_CONTRACT / STATE docs) against the approved `SPEC.md` / `PLAN.md` / project doctrine. The reviewer was instructed to distrust the brief and independently verify every correctness claim against the live tree. The §7.5 verification gate was already green in CI (state-4: differential incl. `0056` + h2spec + fuzz + cargo-deny) — this review is correctness + quality, not a re-verification.

## Overall verdict: ✅ APPROVED (ready for state-6 close-out)

A surgical, additive, provably-1:1 derive — the twelfth Observability access-log row and the FIRST non-`-` `%RESPONSE_FLAGS%` witness (`NR`/NoRoute). No defects at Critical or Important; two Minors (one documentation-citation polish, one informational note on pre-existing unreachable coupling) — both non-blocking → carry-forward.

## Findings by severity

- **Critical:** none
- **Important:** none
- **Minor:** two (non-blocking → carry-forward as **M48-1** [citation] + **M48-2** [informational coupling note])

## Counts

`0 Critical / 0 Important / 2 Minor`

## What was verified (confidence: high)

**1. The derive is provably 1:1 with the no-route 404 path (the check that matters most).** `response_flags: if response_code_details_for_log.as_deref() == Some("route_not_found") { "NR" } else { "-" }` at the single H1 record-build site. The reviewer confirmed `Some("route_not_found")` is produced at EXACTLY two sites in the entire codebase — the host-miss and route-miss `BuildOutcome::Synth(synth_404(...), Some("route_not_found"))` arms — threaded via the writer-arm at `hcm.rs:866` into `response_code_details_for_log`. No other `BuildOutcome::Synth` arm, the `SynthFromDecode` path (leaves the detail at its `None` init), or any other crate sets this string. `synth_404` is unconditionally status 404. **⇒ `NR` cannot appear on a non-no-route path, and neither no-route arm can miss it.**

**2. Borrow/move soundness confirmed.** The `.as_deref()` shared borrow ends before the owned `String` is moved into the `response_code_details:` field below. Independently type-checked clean (`cargo check -p envoy-http1`).

**3. Both backstops genuinely pin the byte-exact line and fail-first.** `h1_route_miss_access_log_carries_nr_flag` (wildcard vhost + single `/specific` route, probe `/nomatch`) and `h1_host_miss_access_log_carries_nr_flag` (non-wildcard `match.test` vhost, probe `Host: nomatch.test`) assert the logged line is byte-exact `{"rc":404,"rcd":"route_not_found","rf":"NR"}\n`. The two topologies are correctly distinct (route-miss vs host-miss arms). Pre-derive both would have rendered `"rf":"-"` ⇒ RED-before-GREEN is structurally valid.

**4. Fixture `0056` fidelity is tight.** `envoy.yaml` vs `envoy-rust.yaml` differ ONLY in the four benign per-side deltas (admin block, listener bind `0.0.0.0`→`127.0.0.1`, `generate_request_id: false`, the mount log path); the route table and the `json_format` block are byte-aligned. Probe 1 (`Host: match.test` `GET /nomatch`) hits route-miss; Probe 2 (`Host: nomatch.test` `GET /specific`) hits host-miss; both `expected_status: 404` is correct.

**5. The `0001`-`0055` byte-identical invariant holds.** Only fixtures `0012`/`0040`/`0046` reference `%RESPONSE_FLAGS%`, and all three probe happy-path 200/direct_response (no `route_not_found`) ⇒ the derive yields `-` and they stay byte-identical. `0012`'s static `value: "-"` pin (a 200 path) is unaffected.

**6. Scope discipline is exemplary.** No new `Op`/`AccessLogRecord` field, enum variant, variable, crate, dependency, fuzz-target, or `ConfigError` — a single field-expression change consuming the pre-existing `response_flags: String` field, the `Op::ResponseFlags` quote-render path, and the `route_not_found` detail. H1-only (H2 deferred — M45-1). `#![forbid(unsafe_code)]` holds.

## Minors (non-blocking → carry-forward)

**M48-1 — line-citation precision (continues the M47-1 lineage).** The derive comment, the fixture `expectations.yaml`, the differential test doc-comment, the fixture README, and the BEHAVIOR_CONTRACT row cite the no-route arms as `:1536` (host-miss) / `:1555` (route-miss). Those live lines are actually the `match`-heads (`let vh = match…` / `let route = match…`); the lines that actually set `Some("route_not_found")` are ~14 lines below at `:1550` / `:1569`. Citing the match-head is a defensible convention and not misleading, but a future touch could cite `:1550`/`:1569` (or annotate "match-head") for precision. **Carry-forward; fold when the no-route arms are next edited.**

**M48-2 — informational note on pre-existing (unreachable) encode-side coupling.** The encode-side `Decision::StopAndSend(replacement)` branch replaces `outgoing` but does not reset `response_code_details_for_log`. If a future encode filter ever substituted a response after a no-route `synth_404`, the access log would emit `NR` alongside the filter's (non-404) status. This is UNREACHABLE under the current Router-only chain, is NOT introduced by this change, and is identical to the pre-existing exposure of the `rcd`/`route_name` fields — the derive does not worsen it. Flagged only because this change folds `NR` into the same byte-exact conformance surface. **Carry-forward; worth a one-line note in a future filter-on-encode phase.**

## Outcome

APPROVED with 0 blocking issues. Phase 48 is ready to advance to **state-6 close-out** (`superpowers:finishing-a-development-branch`): flip ROADMAP row `48` → `done`, fold M48-1 + M48-2 (and the still-live carry-forwards) forward, relocate the active-phase Notes subsection to STATE_HISTORY.md per ADR-0035. M42-1 CONTINUED; M47-1 ACTIONED (superseded by M48-1 on the same surface). `#![forbid(unsafe_code)]` holds.
