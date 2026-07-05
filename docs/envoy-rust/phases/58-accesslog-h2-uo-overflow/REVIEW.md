# Phase 58 — `58-accesslog-h2-uo-overflow` — REVIEW

> State-5 code-review, executed via `superpowers:requesting-code-review`. A
> fresh `general-purpose` subagent (no session history) reviewed the diff
> range **`ee67685`..`70f257e`** (the state-2 PLAN-write commit through the
> last state-3 implementation commit — a `cargo fmt` fixup; the state-4
> verification commit `d3e4e49` is docs-only, no code diff to review) against
> `SPEC.md`/`PLAN.md`/`PROGRESS.md`, instructed to independently RE-DERIVE —
> not trust — every load-bearing claim, mirroring the phase-57 state-5
> precedent's rigor.

## Verdict

**✅ APPROVED — 0 Critical / 0 Important / 2 Minor** (both informational,
neither blocks merge).

## Strengths

- **The core fix is correct and provably exhaustive.** `H2AttemptResult` is
  constructed at exactly 5 sites in `crates/envoy-http2/src/hcm.rs` (lines
  191, 372, 389, 400, 411). Only the `AcquireOutcome::Overflow` arm (line 411,
  fed by both `PoolError::Overflow` and `PoolError::PendingOverflow`) produces
  `endpoint: Some(_), outcome: None`; every other `endpoint: Some` path
  (`Response`, `Reset`, `ConnectFailure`) carries `Some(...)`. The
  `pick()->None` case that also has `outcome: None` carries `endpoint: None`,
  so it never reaches the caller-loop's `if let Some(endpoint) =
  attempt.endpoint` branch that the new discriminator lives in. The
  discriminator is therefore genuinely unique and exhaustive, not just
  "checked and happened to work."
- **The request-budget arm (§B) genuinely bypasses the retry loop.** Confirmed
  by reading `hcm.rs:613`-`654`: the `if let BudgetAcquisition::Rejected =
  request_acquire` arm returns a value directly (no call to
  `run_h2_attempt`); the entire retry loop, including the discriminator from
  §A, lives only in the `else` branch. The direct tag is correctly placed and
  cannot double-fire with §A's logic.
- **The three-arm `%RESPONSE_FLAGS%` derive is safe.** It's a `match` (not a
  chained `if`), so arm order is irrelevant to correctness, and `_ => "-"` is
  present as the final catch-all — an unrecognized/future rcd string
  degrades to the no-flags sentinel, never panics.
- **The two in-process backstops exercise genuinely distinct set-sites**,
  verified by reading both configs: `h2_pool_overflow_access_log_carries_uo_flag`
  wires a real `H2PoolManager` with `max_connections:1/max_pending_requests:0`
  and an H2-upstream cluster (hits §A via `PoolError::PendingOverflow`);
  `h2_request_budget_overflow_access_log_carries_uo_flag` uses
  `spawn_h2_hcm`'s `pool: None` path with `max_requests:0` and a plain
  (non-H2-upstream) cluster (hits §B, never touches the pool). Both drive a
  real TCP listener + real `h2::client::handshake`, not mocks.
- **Fixture 0066 genuinely triggers `PendingOverflow`, not the cap-overflow.**
  Read `pool.rs:329`-`377`: the `max_pending_requests == 0` check fires
  unconditionally before the Phase-2 `max_connections` cap check on any
  connect-on-miss, so `max_connections: 1` in the fixture is pure headroom
  (as the README claims) and the pending-gate is what actually fires.
- **The additivity claim holds, and holds for the right reason.**
  Independently re-grepped `circuit_breakers` across the cited H2 fixtures —
  only `0021` hits, with `max_connections: 4` only (headroom, no
  `max_pending_requests`/`max_requests`). The reviewer also broadened the
  search to *every* fixture with `circuit_breakers` workspace-wide (0025,
  0063, 0020, 0023, 0058, etc.) and confirmed those are all `codec_type:
  HTTP1` listeners, which dispatch to a completely different crate
  (`envoy-http1`, per `main.rs:401`-`405`) and never touch this diff's code
  at all — so the narrower H2-only grep the PLAN used was the right scope,
  not an oversight.
- **BEHAVIOR_CONTRACT.md edits are factually accurate** and consistent with
  the rest of the document (correctly states no status-code fix was needed
  this phase, unlike phases 50/57; correctly attributes `UO` to both
  set-sites).
- **Both cited CI runs verified green** via `gh run view`: `28753634192` →
  `conclusion: success`, `headSha: ee0ae95...` (matches); `28754877400` →
  `conclusion: success`, `headSha: d3e4e49...` (matches).
- **Zero deviation from PLAN.md** — the reviewer diffed the plan's prescribed
  code blocks against the actual committed diff; they are verbatim identical,
  including comments and line placement.
- No `unsafe` code, no new `unwrap()`/`expect()` outside test code, no
  fallibility introduced by the production-code changes (both are pure
  string assignments).

## Issues

### Critical (Must Fix)

None.

### Important (Should Fix)

None.

### Minor (Nice to Have)

1. **Test boilerplate duplication.** The two new backstop tests
   (`crates/envoy-http2/src/hcm.rs`, approximately `:2050`-`:2258` and
   `:2230`-`:2416`) each inline a full
   `HttpConnectionManagerConfig`/`RouteConfiguration`/`VirtualHost`/`Route`
   literal that is ~90% identical to each other and to at least one
   pre-existing helper (`h2_hcm_config_with_access_log` at `:2551`,
   `synth_h2_hcm_config_proxy` at `:1322`). However, this is the established
   idiom throughout this exact test module — phase 56/57's
   `h2_no_healthy_access_log_carries_uh_flag` (`:2813`) and
   `h2_route_miss_access_log_carries_nr_flag` (`:2922`) do the identical
   thing, and `PLAN.md` explicitly calls this out as "matching the
   established in-file precedent of inlining rather than adding shared infra
   for one-off pool-wired tests." Not a phase-58-introduced regression, just
   a standing cost of the codebase's test-authoring convention; a future
   cleanup phase could extract a small builder if the pattern keeps
   repeating.
2. **The overflow discriminator also silently subsumes the
   `PoolError::Overflow` (cap-overflow) arm, not just `PendingOverflow`.**
   Both `PoolError` variants map to `AcquireOutcome::Overflow` →
   `H2AttemptResult{outcome: None}`, so the new `"UO"` tagging is correct for
   both, but the differential fixture (`0066`) and both in-process backstops
   only exercise the `PendingOverflow` sub-case. This is explicitly and
   honestly scoped out in the PLAN/README (the fixture only needs to prove
   one path since both funnel through the same discriminator), so it's not a
   defect — just worth flagging that the cap-overflow (`max_connections`
   reached with `max_pending_requests` nonzero) sub-path is unwitnessed even
   in-process. Low risk given the shared code path, but a nice-to-have for a
   future phase's completeness.

## Recommendations

None required before merge. If a future phase wants to close remaining gaps,
`PLAN.md`'s own carry-forward notes (**M56-1** for `URX`/`UF`/`UC`, and the
deferred request-budget differential fixture) already capture the two most
relevant follow-ups.

## Assessment

**Ready to merge?** Yes.

**Reasoning:** Every load-bearing claim in the review brief was independently
re-derived against the current tree (not trusted from `PROGRESS.md`) and held
up: the discriminator is provably exhaustive, the two arms are structurally
distinct and non-overlapping, the derive is panic-safe, the fixture triggers
the intended pool arm, the additivity grep is correct and complete once
cross-crate dispatch is accounted for, the `BEHAVIOR_CONTRACT.md` prose is
accurate, and both cited CI runs are genuinely green at the claimed SHAs. The
implementation is a verbatim match to the reviewed PLAN with no scope creep
or drift.

## Carry-forwards

No new carry-forward opened by this review (both Minors are informational,
folded as notes rather than tracked M-numbers — neither blocks and neither
introduces new scope beyond what `PLAN.md`/`SPEC.md` already anticipated).
Pre-existing carry-forwards (**M56-1** narrowed to `URX`/`UF`/`UC` + the H2
request-budget arm's own differential fixture, **M57-1**, **M55-1**,
**M53-2**, **M53-3**, and older) are unaffected by this phase and remain as
listed in `STATE.md`.

**Next session:** the §5 state-6 close-out (`superpowers:finishing-a-development-branch`)
— flip ROADMAP row `58` to `done`, relocate the phase-58 Notes subsections,
advance `STATE.md` to "awaiting next planning." Per `BOOTSTRAP_PROMPT.md`
§5.1 (one state per session) and memory `closeout-and-pick-are-separate-sessions`,
close-out and the next-phase pick are separate sessions — do NOT pick the
next phase in the close-out session.
