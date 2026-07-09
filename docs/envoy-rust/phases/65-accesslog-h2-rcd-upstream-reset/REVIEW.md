# Phase 65 — `65-accesslog-h2-rcd-upstream-reset` — REVIEW

> Produced by the §5 **state-5 code-review** session (`superpowers:requesting-code-review`),
> 2026-07-09. Scope locked by **ADR-0122**.
>
> **STEP 0 (disk is authoritative):** `git status --porcelain` clean; branch `main`;
> `HEAD` = `origin/main` = `321a196` (the phase-65 state-4 verification commit).
> `git fetch origin --prune` confirmed no sibling autonomous-loop session had
> advanced phase 65 (`REVIEW.md` absent; ROADMAP row `65` still `in-progress`).
>
> Per `BOOTSTRAP_PROMPT.md` §5.1 (one state per session) the state-6 close-out was
> deliberately NOT run.

## Verdict

**0 Critical / 0 Important / 1 Minor.** The reviewer's literal "Ready to merge?"
answer was **"With fixes (one trivial doc-precision Minor)"**, recorded verbatim
below without softening.

**§5.2 disposition: the phase does NOT re-enter state 3.** Rationale, stated
explicitly rather than assumed:

- `superpowers:requesting-code-review` prescribes "Fix Critical issues
  immediately / Fix Important issues before proceeding / **Note Minor issues for
  later**". There are zero Critical and zero Important issues.
- The standing project precedent is the phase-64 `REVIEW.md`, which landed
  **APPROVED with 0 Critical / 0 Important / 2 Minor** (M64-2, M64-3); those Minors
  became carry-forwards and phase 64 closed at state-6 without re-entry. §5's "if
  issues → back to step 3 … until `REVIEW.md` approved" has therefore never been
  read as "zero Minors".
- The single Minor is **pure doc prose in `BEHAVIOR_CONTRACT.md`** — no code, no
  test, no fixture, no logged value is affected.

The Minor is recorded as new carry-forward **M65-1** (below). It is a genuine
defect *introduced by this phase* (grammatically), not a pre-existing cosmetic
like M64-2, so it should be folded in at the earliest convenient docs-touching
session rather than allowed to age.

## Method

A **fresh `general-purpose` subagent with NO prior session context** was dispatched
over the full phase-65 range **`d5b6dd4..321a196`** (9 commits: the six PLAN task
commits, the state-3 pre-flight + CI-adjudication commits, and the state-4
verification commit). It was handed `SPEC.md` / `PLAN.md` / `PROGRESS.md` /
ADR-0122 / the two touched `BEHAVIOR_CONTRACT.md` rows, plus the doctrine rules
D-3.2 / D-3.4 / D-3.5 / D-3.8 and ADR-0094 §A, and was **explicitly instructed not
to trust commit messages, `PROGRESS.md` claims, or the dispatcher's summary** — it
had to re-derive every load-bearing claim from the current source, using the landed
H1 phase-54 implementation as a reference oracle. Its review was read-only (tree
confirmed unmodified afterwards: `git status --porcelain` empty, `HEAD` unmoved).

It was given eight specific claims to verify, **including the doc-precision Minor
the state-4 gate had already flagged — presented as a candidate to CONFIRM OR
REJECT independently**, not as a finding to rubber-stamp.

### Dispatcher-side re-verification (agent reports are not evidence)

Per `superpowers:verification-before-completion`, the reviewer's load-bearing
claims were independently re-run by this session:

```
$ grep -rn "reset_for_log_h2" crates/
(no output — exit 1)

$ grep -n "fn finalize_h2_stream" -A40 crates/envoy-http2/src/hcm.rs   # params
961:async fn finalize_h2_stream(
    retry_limit_exceeded_for_log_h2: bool,
    connect_failure_for_log_h2: bool,
) -> Result<(), Http2Error> {

$ grep -n "finalize_h2_stream(" crates/envoy-http2/src/hcm.rs
930:    finalize_h2_stream(      <-- the single call site
961:async fn finalize_h2_stream(

$ cargo test -p envoy-http2 --lib h2_upstream_reset_access_log_carries_uc_flag
test hcm::tests::h2_upstream_reset_access_log_carries_uc_flag ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 88 filtered out

$ cargo test -p envoy-http2 --lib h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx
test hcm::tests::h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 88 filtered out
```

Exactly **two** `_for_log_h2: bool` parameters remain on `finalize_h2_stream`
(down from three), with exactly one matching call site. Confirmed.

---

## Reviewer verdict (verbatim)

### Strengths

- **§A set-site ordering is correct and genuinely overriding** (`crates/envoy-http2/src/hcm.rs:892-899`). The guarded rcd-set sits in the post-loop reconciliation block, *after* `retry_limit_exceeded_for_log_h2` is computed (line 858) and `connect_failure_for_log_h2` (line 874), *after* the in-loop `via_upstream` write (line 758), and *before* `finalize_h2_stream` (line 930) which hosts the derive. So the guard reads an already-assigned value and the set wins over `via_upstream`. Exactly as claimed.
- **Guard + replay safety are sound.** Pure-reset (`final_outcome_h2 == Some(Reset)`, `!retry_limit_exceeded`) → `{connection_termination}` → `UC`. Retry-exhausted reset → guard false → rcd stays `via_upstream`, and the derive's `URX`-first ordering (line 1084) renders `URX`. Reset-retried-to-success sets `final_outcome_h2 = Some(Response)` (captured on the last attempt, line 780), so `matches!(…Reset)` is false → no set → replay-safe. Faithful to the H1 phase-54 guarantee.
- **`reset_for_log_h2` is completely retired from code.** `grep -rn "reset_for_log_h2" crates/` yields nothing; the declaration + its comment block are gone (no dangling comment at lines 538-577); `finalize_h2_stream` now takes exactly two booleans (lines 993/997) with a matching two-arg call site (lines 944-945). `cargo clippy -p envoy-http2 --all-targets --all-features -- -D warnings` exits 0 — no `unused_variable`/`unused_mut`/dead-code left behind.
- **Both in-process backstops are real and non-vacuous.** `h2_upstream_reset_access_log_carries_uc_flag` (line 4989) asserts the *full* line including the rcd (line 5066). The required negative test `h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx` (line 5078) uses a genuine `retry_on:"reset"`, `num_retries:1` policy against the new `spawn_upstream_h2_reset_server_multi()` helper, which accepts *unbounded* connections and resets *every* stream — so the retry's second connection is accepted and reset (yielding `URX`, not a `ConnectionRefused`→`UF`). Both pass locally. The multi-accept loop is non-blocking (one detached task per connection) with no hang/race risk.
- **Additivity holds.** Of the 17 fixtures logging `%RESPONSE_CODE_DETAILS%`, only `0062` (H1, `CLOSE_BACKEND_PORT`) and the new `0070` (H2) drive a close backend. `0067` logs rcd but drives retry-exhausted-to-a-real-503 (final outcome `Response`, not `Reset`), so §A cannot fire. No existing fixture both logs an rcd and drives an H2 pure-reset — `0069` logs no rcd and stays byte-identical.
- **Fixture `0070` is clean.** `diff 0069 vs 0070 envoy.yaml` = only node id, mount path, and the added `rcd:` key. `envoy.yaml` vs `envoy-rust.yaml` (ignoring comments) differ *only* by the admin block, bind address (`0.0.0.0` vs `127.0.0.1`), and mount path — `{{BACKEND_HOST}}`/`{{H2_CLOSE_BACKEND_PORT}}` markers are identical on both sides. `expectations.yaml` asserts cross-proxy equality with `expected_status: 503`. The `{{H2_CLOSE_BACKEND_PORT}}` auto-spawn arm exists at `tests/differential/src/lib.rs:3345` and was **not** edited in the range; `tests/differential/Cargo.toml` has zero `[[test]]` entries, so the new test auto-discovers.
- **§E inversion done correctly.** The H2-`UC` clause in the `%RESPONSE_FLAGS%` row was genuinely *inverted* ("On H2, `UC` is now derived EXACTLY as on H1 … derives 1:1 from that rcd … The phase-64 boolean discriminator was RETIRED"), not appended to a self-contradictory older claim. No remaining active-state prose describes H2 `UC` as boolean-derived. The `0069` README's `reset_for_log_h2` mention is correctly preserved as backward-looking phase-64 history with an additive phase-65 update note.
- **No scope creep:** no `Cargo.toml`/`ci.yml` changes, no new dependency, no `unsafe` (the one `grep` hit is prose containing `#![forbid(unsafe_code)]`). The net `crates/` change is a simplification.

### Issues

#### Critical (Must Fix)
None.

#### Important (Should Fix)
None.

#### Minor (Nice to Have)

- **`docs/envoy-rust/BEHAVIOR_CONTRACT.md:1020` — `%RESPONSE_FLAGS%` row internally contradicts `:1031` on when M56-1 closed.** CONFIRMED (this is the state-4 candidate Minor; I verified it independently rather than taking it on trust). At base `d5b6dd4`, the `UC`@0069 sentence ended "…ordered AFTER `UF` in the derive — **CLOSING carry-forward M56-1**", where the em-dash correctly bound the M56-1 closure to phase 64's sixth-flag witness. Phase 65's Task 5 inserted a new clause ("*phase 65 (ADR-0122) has since CONSUMED M64-1 — … the boolean discriminator + its `finalize_h2_stream` parameter were RETIRED*") *between* that narrative and the "— **CLOSING carry-forward M56-1**" clause, so the em-dash now grammatically binds the M56-1 closure to the phase-65 retirement. The sibling `%RESPONSE_CODE_DETAILS%` row (`:1031`), `STATE.md`, and the phase-64 close-out all say M56-1 closed at **phase 64**. Purely cosmetic (no `%RESPONSE_FLAGS%`/rcd value affected), but the contract is the canonical reference for today's rules. *Fix:* re-word so the M56-1 closure re-attaches to phase 64, e.g. break the phase-65 retirement into its own sentence, or change the trailing clause to "(M56-1 was closed at phase 64)". Note the implementer already documented this precisely in `STATE.md:108` and `PROGRESS.md` as a finding deliberately deferred to this review — appropriate handling under the "fixes re-enter at state-3, no state-chaining" discipline.

### Recommendations

- The PLAN Task 6 Step 2 grep-over-`tests/` deviation was resolved **correctly**: the `0069` README hit is backward-looking phase-64 narrative that D-3.4/D-3.5 orders left verbatim; SPEC §F scopes the sweep to `crates/envoy-http2/src/`, and only the over-broad PLAN Task 6 grep collided with doctrine. The implementer preserved the history and corrected only the one stale active-state cross-reference, additively. No change needed.
- When fixing the one Minor above, that single edit fully clears the finding — no other contract prose is stale.

### Assessment

**Ready to merge?** With fixes (one trivial doc-precision Minor).

**Reasoning:** The implementation is a faithful, well-tested H2 analogue of the phase-54 H1 work — the guarded rcd-set, the `UC`-from-rcd migration, the complete boolean retirement, the required negative-guard backstop, and the additive fixture are all correct and verified (clippy clean, backstops green, additivity and harness-reuse confirmed). The only defect is a cosmetic grammatical re-attachment in one BEHAVIOR_CONTRACT row that the implementer already flagged for this review; it has zero behavioral impact but should be corrected before merge since the contract is the canonical reference.

---

## New carry-forward

- **M65-1 (Minor, doc-precision, INTRODUCED by phase 65).** `docs/envoy-rust/BEHAVIOR_CONTRACT.md`'s `%RESPONSE_FLAGS%` row (`:1020`) now grammatically attaches the clause `— **CLOSING carry-forward M56-1**` to phase 65's boolean retirement, contradicting the `%RESPONSE_CODE_DETAILS%` row (`:1031`), `STATE.md`, and the phase-64 close-out, all of which say **M56-1 closed at phase 64**. Provenance is established, not assumed: the clause is **pre-existing phase-64 text** (`git show d5b6dd4:docs/envoy-rust/BEHAVIOR_CONTRACT.md` contains it), where the em-dash bound correctly to phase 64's sixth-flag `UC` witness; phase 65's Task-5 insertion landed *between* the narrative and the clause. Zero behavioral impact — no `%RESPONSE_FLAGS%` or `%RESPONSE_CODE_DETAILS%` value changes, no fixture affected. **Fix:** one-line re-wording of that row so the M56-1 closure re-attaches to phase 64 (e.g. split the phase-65 retirement into its own sentence, or replace the trailing clause with "(M56-1 was closed at phase 64)"). **Disposition:** fold in at the next docs-touching session (the state-6 close-out is a natural, in-scope home — it already edits `ROADMAP.md`/`STATE.md`); it is a doc-prose one-liner on a row this phase already owns, and does NOT warrant a full §5.2 state-3 re-entry given 0 Critical / 0 Important.

## Carry-forward disposition

- **M64-1 → CONSUMED** by this phase (the §A rcd-set + §B/§F derive migration landed at state-3; the differential witness was DISCHARGED at the state-4 gate — fixture `0070` is `... ok` on CI run `28986078817` @ `f66fe9c`).
- **M65-1 → NEW** (above).
- **M64-2 stays LIVE** — the `hcm.rs:236` stale `synth_h2_502` comment. Phase 65's PLAN offered an opportunistic fold-in *if the comment region was touched*; it was not, so the fold-in did NOT happen.
- **M64-3, M57-1, M55-1, M53-2, M53-3, M48-2, M42-1,** the `DC`/retry-budget-overflow slices of **M45-2**, the phase-58 candidate carry-forward, **M40-1, M39-1/M39-2, M38-1/M38-2, CF-39-1**, M37-*, M36-*, M34-*, M33-*, the empty-`metadata_match` doc-comment, M29-*/M30-*, the phase-31 cosmetics, and the HTTP-filters-family (1)-(4) all stay live. **NONE blocks.**

## Process facts

- **NO new ADR fired this session.** No §6.2 reconciliation; no §A-§G fact overturned. **ADR-0123 remains reserved-but-UNFIRED.**
- **DECISIONS.md ledger head: ADR-0122.** ADR-0014 in force; ADR-0028 open; ADR-0049 governs config-validity.
- `#![forbid(unsafe_code)]` holds; no `unsafe` anywhere in the diff.
- No new crate / dependency / `Op` / `AccessLogRecord` field / `ConfigError` variant / test-harness / `ci.yml` change.
- **NO `%RESPONSE_FLAGS%` value changed** — the witnessed H2 flag stays `UC`; only its DERIVATION moved from the boolean to the rcd.

**Next session = §5 state-6 close-out** (flip ROADMAP row `65` → `done`; relocate the
phase-65 Notes subsection to `STATE_HISTORY.md`; `STATE.md` → "awaiting next
planning"). Per §5.1, one state per session — the close-out was deliberately NOT run
here, and per memory `closeout-and-pick-are-separate-sessions` the next-phase state-0/1
pick is a SEPARATE session again after that.
