# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `33` — `33-set-metadata-dynamic-metadata` (**Observability family: the dynamic-metadata critical-path unlock** that phase 32 enabled — the `envoy.filters.http.set_metadata` HTTP filter [a static-value metadata emitter] + a per-request **dynamic-metadata store** + the `%DYNAMIC_METADATA(namespace:key)%` access-log command-operator + fixture `0041`). Lifecycle **state-5 CODE-REVIEW COMPLETE / state-6-next** (`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md` all present; `REVIEW.md` verdict **APPROVE-WITH-MINORS** — no Critical/Important issues). The next session runs the state-6 deterministic close-out (flip ROADMAP row `33` → `done`, advance STATE, commit + push) — see `## Next expected skill`.
**slug:** `33-set-metadata-dynamic-metadata`
**directory:** `docs/envoy-rust/phases/33-set-metadata-dynamic-metadata/` (`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md` all present)

**status:** **PHASE 33 (`33-set-metadata-dynamic-metadata`) state-5 CODE-REVIEW COMPLETE — state-6 close-out next.** This session (`superpowers:requesting-code-review`) dispatched a fresh `superpowers:code-reviewer` subagent (crafted context, NOT session history) to review the phase-33 diff `0fde584..ca88f83` against the PLAN's **§A empirically-locked facts (A1–A6, ADR-0081)**. **Verdict: APPROVE-WITH-MINORS** (ready to merge: YES) — written to `REVIEW.md`. **No Critical, no Important issues.** The reviewer confirmed (by reading code + targeted unit-test checks): §A2 parser strictness (rejects `:N`/no-arg/1-seg/3+-seg), case-sensitive namespace/key (NOT lowercased), §A3/§A4 raw-unquoted render + absent `-`, the `allow_overwrite` per-key merge (Continue-only/encode-inert), the sound dual H1+H2 capture-before-drop (H2 via `finalize_h2_stream` param-thread), the real (non-mock) H1+H2 backstops + fixture `0041` present/absent anti-echo guard, and regression safety (additive default-empty store; M32-4 oracle proves the default format unperturbed). **Three Minors, all non-blocking carry-forwards:** **M33-1** an unnecessary `.clone()` at `crates/envoy-http1/src/hcm.rs:1211` (the H1 record-build local is single-use/last-use → could move, as H2 does; cosmetic, almost-always-empty `BTreeMap`) — fold when the H1 HCM record-build is next touched; **M33-2** doc-pointer line-number drift in `command_operator.rs`/`record.rs` (hardcoded `~1189`/`~888` vs actual `~1211`/`finalize_h2_stream`) — doc-only, fold on a future `hcm.rs` touch; **M33-3** (noted-only, NOT a defect) `parse_dynamic_metadata_op` matches the FIRST `)` — correct for the string-only single-level MVP (Envoy's grammar has the same behavior), a watch-item for the future nested-path deferral. No state-3 re-entry (no issues → no re-implementation per §5.2). `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target. **DECISIONS.md ledger head: ADR-0081** (count 82; next-available **ADR-0083**; ADR-0082 reserved-but-UNFIRED for the §6.1 split that did NOT fire). ADR-0014 in force; ADR-0028 open. The state-6 close-out is the NEXT session (the deterministic close per `BOOTSTRAP_PROMPT.md` §5 state 6: flip ROADMAP row `33` → `done`, advance STATE to the next phase / "awaiting next planning", commit + push) — see `## Next expected skill`.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 6 + `SKILL_ROUTING.md`: phase `33` is `in-progress` with `SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md` all present and `REVIEW.md` **APPROVED (APPROVE-WITH-MINORS, no Critical/Important)** -> the next session runs the **state-6 deterministic close-out** (no special skill mandated; the "Reviewed and approved" terminal step): (1) flip **ROADMAP row `33` → `done`** (currently `in-progress`); (2) advance STATE — close phase 33, set the active phase to the next ROADMAP pick or "awaiting next planning" (state-0/state-1), relocating the phase-33 `## Active phase` status + Notes + carry-forward narratives to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9; (3) carry forward the phase-33 Minors **M33-1** (the H1 `hcm.rs:1211` `.clone()`→move) + **M33-2** (the `command_operator.rs`/`record.rs` doc-pointer drift) into the open-Minors bucket (M33-3 is noted-only, no action); (4) ONE close-out commit (message `phase 33: <title> [ADR-0080, ADR-0081]`) + **push** (CI is the authoritative gate for the host-sensitive differential/h2spec set). **State-5 is DONE (do NOT re-review):** `REVIEW.md` verdict APPROVE-WITH-MINORS, no blocking issues; no state-3 re-entry. The §7.5 gate is GREEN (state-4, quoted in `PROGRESS.md` `## State-4 verification`); CI run `27985371447` (latest `main`, all `success`) is authoritative for the host-sensitive fixtures + h2spec. Still-live carry-forwards (NOT phase-33 obligations; weigh only if a future phase touches them): **M33-1 / M33-2** (NEW this phase, above); the empty-`metadata_match`→fallback doc-comment; M29-1/M29-2 + M30-1 (the `Http1HashSweep` driver diagnostics / duplicated `extract_marker`); M30-2 (`lb_policy` serde-default); the phase-31 cosmetic Minors M-2/M-3; the HTTP-filters-family (1)-(4) buffer carry-forwards. (The 6 M32 carry-forwards are CONSUMED — folded + landed at state-3.)

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-33 state-5 code-review — dispatch the reviewer, write REVIEW.md APPROVE-WITH-MINORS, advance STATE (THIS docs-only commit):** the state-5 code-review (`BOOTSTRAP_PROMPT.md` §5 state 5; `superpowers:requesting-code-review`). Dispatched a fresh `superpowers:code-reviewer` subagent (crafted context, NOT session history) to review the phase-33 diff `0fde584..ca88f83` against the PLAN's §A empirically-locked facts (A1–A6, ADR-0081). **Verdict: APPROVE-WITH-MINORS** (ready to merge: YES; no Critical, no Important) — written to `REVIEW.md`. Three non-blocking Minors carried forward: **M33-1** (H1 `hcm.rs:1211` `.clone()`→move), **M33-2** (doc-pointer line drift in `command_operator.rs`/`record.rs`), **M33-3** (noted-only: first-`)` parse, correct for the MVP). **THIS docs-only commit** writes `REVIEW.md` + advances STATE `33` state-4-complete → state-5-complete/state-6-next (the state-4 status + last-commit narratives RELOCATED to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9). No state-3 re-entry (no issues → no re-implementation, §5.2). `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target. **DECISIONS.md ledger head: ADR-0081** (count 82; next-available **ADR-0083**; ADR-0082 reserved-but-UNFIRED for the §6.1 split). ADR-0014 in force; ADR-0028 open. The state-6 close-out is the NEXT session (flip ROADMAP row `33` → `done`, advance STATE, commit + push).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-23 (phase-33 **state-5 CODE-REVIEW COMPLETE / state-6-next** — `REVIEW.md` written [`superpowers:requesting-code-review`; a fresh `superpowers:code-reviewer` subagent reviewed the diff `0fde584..ca88f83` against the PLAN §A facts]. **Verdict APPROVE-WITH-MINORS** [ready to merge: YES; no Critical, no Important]. The reviewer confirmed by code-reading + targeted unit checks: §A2 parser strictness [`:N`/no-arg/1-seg/3+-seg all rejected], case-sensitive keys, §A3/§A4 raw-unquoted render + absent `-`, the per-key `allow_overwrite` merge [Continue-only/encode-inert], the sound dual H1+H2 capture-before-drop [H2 via `finalize_h2_stream`], the real non-mock H1+H2 backstops + fixture `0041` present/absent anti-echo guard, and regression safety [additive default-empty store; M32-4 oracle]. Three non-blocking Minors carried forward: **M33-1** [H1 `hcm.rs:1211` `.clone()`→move], **M33-2** [doc-pointer line drift in `command_operator.rs`/`record.rs`], **M33-3** [noted-only: first-`)` parse, correct for the MVP]. THIS docs-only commit writes `REVIEW.md` + advances STATE `33` state-4-complete → state-5-complete/state-6-next [state-4 status + last-commit narratives relocated to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9]. No state-3 re-entry [no issues, §5.2]. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target. **DECISIONS.md ledger head: ADR-0081** [count 82; next-available ADR-0083; ADR-0082 reserved-but-UNFIRED §6.1 split]. ADR-0014 in force; ADR-0028 open. The state-6 close-out is the NEXT session [flip ROADMAP row `33` → `done`, advance STATE, commit + push].)

> Historical `## Last updated` notes — every superseded last-updated note (all closed phases + the active phase's prior sub-state notes) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Notes

> Historical Notes subsections for fully-closed phases 00-07 (ADR-numbering notes, per-phase rollovers, ADR ledgers, and the earlier-phase-carryforward + phase-00-deferral snapshots) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. State-6 close-out commits touch ROADMAP.md + STATE.md only and carry no code changes; the next session writes PLAN.md for the next active phase per `superpowers:writing-plans`.
- The reviewer's R2 disposition decision (option (a) retroactive split of 05.1 vs option (b) free-standing post-05.1 sub-phase) was settled at the 05.1 state-6 commit in favor of option (b); 05.4 is the chosen sibling sub-phase. Future-reviewers reading STATE.md should understand that 05.1 is structurally closed at the preamble landing; 05.4 is a SIBLING under parent-05, not a child of 05.1; and the execution order ran 05.1 → 05.4 → 05.2 → 05.3, with 05.3 the closing sub-phase that flips parent-05 to `done`.

> Historical Notes subsections for fully-closed phases 05.4 / 08 / 09 / 10 (brainstorm, split, PLAN-write, execution-arc, rollovers, and ADR-ledger narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phases 11–21 (brainstorm / split / PLAN-write / execution-arc + verification / code-review / rollovers narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 22 (brainstorm / PLAN-write / execution-arc + state-4 verification / code-review / rollovers narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 23 (state-1 brainstorm / state-2 PLAN-write / state-3 execution arc / state-4 verification gate / state-5 code-review narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 24 (state-1 brainstorm / state-2 PLAN-write narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed sub-phase 25.1 (state-2 PLAN-write / state-3 implementation / state-4 verification / state-5 code-review narratives) and for parent phase 25 (the `### Phase-25 state-1 brainstorm` pick + recon-finding narrative, relocated at the 25.2 state-6 close-out when parent `25` flipped to `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsection for fully-closed phase 26 (the `### Phase-26 state-1 brainstorm` pivot/rejected-alternatives/key-scoping narrative, relocated at the phase-26 state-6 close-out when row `26` flipped to `done`) is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 27 (the `### Phase-27 state-1 brainstorm` / `### Phase-27 state-2 PLAN-write` / `### Phase-27 state-4 verification` narratives + the now-consumed `### Phase-27 carry-forwards` [M26-1..M26-8] block, relocated at the phase-27 state-6 close-out when row `27` flipped to `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 28 (the `### Phase-28 state-1 brainstorm` / `### Phase-28 state-2 PLAN-write` / `### Phase-28 state-3 implementation + state-4 verification` / `### Phase-28 state-5 code review` narratives + the now-consumed `### Phase-28 carry-forwards` [M27-1..M27-3] block, relocated at the phase-28 state-6 close-out when row `28` flipped to `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 29 (the now-consumed `### Phase-29 carry-forwards` [M28-1..M28-3] block + the `### Phase-29 state-1 brainstorm` / `### Phase-29 state-2 PLAN-write` / `### Phase-29 state-3 implementation` / `### Phase-29 state-4 verification` / `### Phase-29 state-5 code review` narratives, relocated at the phase-29 state-6 close-out when row `29` flipped to `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 30 (the `### Phase-30 carry-forwards` [M29-1/M29-2, which fed phase 30 but were NOT consumed — they continue as phase-31 carry-forwards] block + the `### Phase-30 state-1 brainstorm` / `### Phase-30 state-2 PLAN-write` / `### Phase-30 state-3 implementation` / `### Phase-30 state-4 verification` / `### Phase-30 state-5 code review` narratives, relocated at the phase-30 state-6 close-out when row `30` flipped to `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 31 (the `### Phase-31 carry-forwards` [the empty-`metadata_match`→fallback doc-comment + M29-1/M29-2 + M30-1 + M30-2 — open Minors from the phase-30 REVIEW.md that fed phase 31 but were NOT consumed; they continue as carry-forwards for the next phase that touches the differential hash-sweep driver / the config parser] block + the `### Phase-31 state-1 brainstorm` / `### Phase-31 state-2 PLAN-write` / `### Phase-31 state-3 implementation` / `### Phase-31 state-4 verification` / `### Phase-31 state-5 code review` narratives, relocated at the phase-31 state-6 close-out when row `31` flipped to `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsection for fully-closed phase 32 (the `### Phase-32 carry-forwards` block — the open Minors that fed phase 32 but were NOT consumed: the empty-`metadata_match`→fallback doc-comment + M29-1/M29-2 + M30-1 + M30-2 + the phase-31 cosmetic Minors M-2/M-3; they continue as carry-forwards for the future phase that touches their surface — re-listed in `## Next expected skill` above alongside the 6 new phase-32 REVIEW.md Minors M32-1…M32-6), relocated at the phase-32 state-6 close-out when row `32` flipped `done`, is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

### Phase-33 carry-forwards (active phase; weigh + FOLD at the state-2 PLAN-write / state-3 implementation)

> Phase 33 touches `crates/envoy-accesslog/` (`command_operator.rs` gains `Op::DynamicMetadata`; `record.rs` gains the `dynamic_metadata` field) AND `crates/envoy-filter/` (the `set_metadata` filter + `FilterRequest.dynamic_metadata`) AND `crates/envoy-config/` (the `SetMetadata` filter config + validator). **State-3 UPDATE — the 6 phase-32 REVIEW.md Minors M32-1…M32-6 are CONSUMED (folded + landed):** **T1** (commit `433ab4f`) landed M32-1 (`enum Side`), M32-2 (empty-alt + `:0` strictness), M32-3 (named-field diagnostics), M32-6 (`render` pre-alloc); **T12** (commit `4a59579`) landed M32-4 (the looped default-equivalence oracle) and M32-5 (deleted the 0-byte `0040/inputs/payload.bin`). They are NO LONGER carry-forwards. The original list (retained for traceability):

- **M32-1** — `command_operator.rs` `Req`/`Resp` `side: &'static str` → an `enum Side { Req, Resp }` (the new `Op::DynamicMetadata` is a clean moment to land it).
- **M32-2** — `%REQ(:path?)%` empty-alt → `alt: Some("")` / `:0` truncate parser strictness.
- **M32-3** — `MalformedArgument` positional-tuple + partial `UnsupportedHeader`-alt reporting → named-field diagnostics.
- **M32-4** — the in-crate default-equivalence oracle rests on a single record → loop it over 5xx/router/utf8 records.
- **M32-5** — the vestigial 0-byte `tests/fixtures/0040-accesslog-command-operators/inputs/payload.bin`.
- **M32-6** — `render` fixed `String::with_capacity(256)` pre-alloc.

> Other still-live carry-forwards (weigh only if the surface is touched): the empty-`metadata_match`→fallback doc-comment (`crates/envoy-cluster/src/subset.rs`); M29-1/M29-2 + M30-1 (the `Http1HashSweep` driver diagnostics / duplicated `extract_marker`, `tests/differential/src/lib.rs`); M30-2 (`lb_policy` serde-default); the phase-31 cosmetic Minors M-2/M-3 (`cdn_loop`); the HTTP-filters-family (1)-(4) buffer carry-forwards below.

### HTTP-filters-family carry-forwards (from the `25.2` REVIEW.md - NOT yet consumed; weigh whenever the HTTP-filters family is re-entered)

> These were never obligations on the xDS phase 26; they remain live for whenever an HTTP-filters-family phase resumes.

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _(2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase._
