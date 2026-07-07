# Phase 61 — `61-accesslog-h2-urx-retry-exhausted` — REVIEW

**State-5 code review (`superpowers:requesting-code-review`).** A fresh `general-purpose` subagent, with no prior session context, independently re-derived every load-bearing claim in `SPEC.md`/`PLAN.md`/`PROGRESS.md` against the actual committed diff (`git diff c083c348f2f3772b8f923bcf843fd82ddf6b9644..115a445348caaef1adcc9ec36a6154b2745b31d5` — the full commit chain from the state-1 brainstorm through the state-4 verification bookkeeping) rather than trusting the authoring session's own narrative.

**Verdict: APPROVED — 0 Critical / 0 Important / 2 Minor (both non-blocking, informational only).**

---

### Strengths

- **The core mechanism is exactly as documented and minimal.** `crates/envoy-http2/src/hcm.rs`: `retry_limit_exceeded_for_log_h2: bool` is declared once (near the other `*_for_log_h2` locals, default `false`), set `true` in exactly one place — inside the same `if attempts > 1 && !retry_budget_blocked { if final_retriable { cluster.upstream_rq_retry_limit_exceeded().inc(); retry_limit_exceeded_for_log_h2 = true; } }` block — so the boolean and the counter provably co-fire. There is exactly one `finalize_h2_stream(` call site, so threading the new parameter could not silently miss a second call path.
- **True additivity.** The diff of `hcm.rs` contains exactly one deletion block — the original 4-line three-arm `match` — which reappears byte-for-byte unchanged inside the new `else` branch of the wrapper. The `NR`/`UH`/`UO` arms are untouched, and no other H2 fixture directory (`0009`/`0010`/`0018`/`0021`/`0064`/`0065`/`0066`) appears anywhere in the diff stat for this range.
- **Genuine TDD discipline.** Commit `8f25f98` (RED) adds only the new test (`h2_retry_limit_exceeded_access_log_carries_urx_flag`) with zero source changes — it could not have passed before `c1cad34` (GREEN), which adds precisely the boolean/threading/wrapper and nothing else. Verified both tests pass locally: `cargo test -p envoy-http2 h2_retry_limit_exceeded --lib` → both `h2_retry_limit_exceeded_path_always_503` and `h2_retry_limit_exceeded_access_log_carries_urx_flag` green.
- **The new backstop is correctly additive to, not a replacement of, the phase-16 test.** It's a genuinely new test; the existing `h2_retry_limit_exceeded_path_always_503` is untouched by the diff and still asserts status/counters/header. The pre-existing (also untouched) sibling `h2_retry_success_path_503_then_200` independently asserts `upstream_rq_retry_limit_exceeded().value() == 0` on the retry-succeeds path — since that's the same gate the new boolean shares, this test already demonstrates the boolean stays `false` when a retry succeeds, satisfying the "distinguish retry-succeeded from retry-exhausted" testing concern without needing a new dedicated assertion.
- **The mid-implementation `dns_lookup_family: V4_ONLY` fixture fix (`3d739ae`) is handled well.** It was applied to both `envoy.yaml` and `envoy-rust.yaml` for fixture 0067, it exactly matches fixture 0059's existing precedent, and the commit message gives a concrete, plausible root cause (AAAA-vs-A resolution divergence between the two containers) plus verification evidence (6 repeated local runs). This reads as attentive, not rushed — PLAN.md's Task 2 template simply crossed 0059 with 0064-0066's H2C shape and dropped the setting, and the gap was caught before being declared done.
- **Differential harness wiring is purely additive.** `tests/differential/src/lib.rs`: the `needs_health_aware_backend` allowlist gained one `||` arm for `"0067-…"`, and the per-path match's `0059` arm was folded into a shared `||` with `0067` mapping to the identical `/retry-exhausted=503` string — no behavior change to `0059`'s own path.
- **Documentation is consistent with precedent.** The `BEHAVIOR_CONTRACT.md` `%RESPONSE_FLAGS%` row's new H2-`URX` sentence explicitly says it is "NOT derivable from `%RESPONSE_CODE_DETAILS%`... the SAME non-rcd-derivable pattern H1 established at phase 51" — matching the wording style of the existing H1 `URX`/`UF` per-flag-equivalence entries. ADR-0118 in `DECISIONS.md` is thorough and its concurrency-guard reasoning (numbering around sibling-claimed-but-unfired ADR-0116/0117) is well explained.
- **Independent CI corroboration succeeded.** `gh run view 28821162097` and `gh run view 28828639799` both show `conclusion: success` on both jobs. `gh run view 28828639799 --json headSha` returned `115a445348caaef1adcc9ec36a6154b2745b31d5`, exactly this range's HEAD — the state-4 session's claim of a second green CI run at the final commit is independently confirmed, not just self-reported.
- **No stray footprint.** `#![forbid(unsafe_code)]` intact in `crates/envoy-http2/src/lib.rs`. The `Cargo.toml`/`Cargo.lock` diffs in the full range come entirely from unrelated concurrent sibling commits (thread-per-core/io_uring/SO_REUSEPORT perf work), not from any phase-61 commit — PLAN.md's "zero dependency changes" claim holds for the phase-61 slice itself. Local `cargo fmt --all -- --check` and `cargo clippy -p envoy-http2 --all-targets --all-features -- -D warnings` both clean.

### Issues

#### Critical (Must Fix)

None found.

#### Important (Should Fix)

None found.

#### Minor (Nice to Have)

1. **Review-range hygiene (not a phase-61 defect).** The base/head range (`c083c34..115a445`) contains an interleaved unrelated perf workstream (SO_REUSEPORT listener fan-out, thread-per-core/io_uring, hot-path alloc cuts, phase-62 idle-timeout scaffolding), pulled in via a sibling-branch merge that lands between the phase-61 commits in `git log` order. None of it touches any file relevant to phase 61 (fixtures, `envoy-http2/src/hcm.rs`, differential harness, BEHAVIOR_CONTRACT/DECISIONS phase-61 sections), so it did not affect the review's conclusions, but it required deliberately path-scoping every diff/show command rather than trusting `git diff --stat` at the given range wholesale. Future review handoffs for this repo should give a path-scoped range (or note the merge boundary explicitly) when concurrent sibling-phase work is interleaved in history.
2. **`DECISIONS.md` ADR-0118 entry is extremely long** (roughly 1,300 words in a single bullet-dense paragraph run-on style), consistent with prior ADRs in this file but harder to skim for load-bearing facts (§A-§I scope) versus process narrative (concurrency-guard numbering justification). Not a defect — matches established house style for this ledger — but a candidate for tightening if this ADR format is ever revisited.

### Recommendations

- No code or process changes are needed for this phase. The precedent-mirroring discipline (H1 phase-51 → H2 phase-61, same boolean-discriminator pattern, same wording template in BEHAVIOR_CONTRACT) is working well and should keep being followed for the remaining H2 `UF`/`UC` witnesses.
- Consider, for future phase reviews, requesting a path-scoped `git diff` (limited to the phase's own fixture/doc/source paths) up front rather than a raw commit-range diff, given this repo's pattern of concurrent sibling-session merges landing between a phase's own commits.

### Assessment

**Ready to merge?** Yes.

**Reasoning:** Every load-bearing claim in `SPEC.md`/`PLAN.md`/`PROGRESS.md` was independently re-derived from the diff and matches: single set-site co-gated with the existing counter, single threaded call site, additive derive wrapper leaving the three pre-existing arms byte-identical, a genuine RED→GREEN TDD pair, a new (not modified) backstop test whose assertions match the claimed JSON line, a faithful and well-justified fixture fix, purely additive differential-harness wiring, consistent BEHAVIOR_CONTRACT wording, zero touched pre-existing fixtures, no new dependencies attributable to this phase, and two independently-corroborated green CI runs (including one at the exact range HEAD). Local build/clippy/fmt/test all pass. No correctness, architecture, or test-coverage issues were found.

---

**Per §5.2:** since this review is APPROVED (0 Critical / 0 Important), the phase does NOT re-enter at state 3. Per §5.1 (one state per session), this session advances `STATE.md` to phase-61 state-5-code-review-complete and stops — the state-6 close-out (ROADMAP → `done`, STATE.md → "awaiting next planning") is a separate future session.
