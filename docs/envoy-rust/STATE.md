# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `34` — `34-header-to-metadata` (**Observability / HTTP-filters family: request-driven metadata** — the `envoy.filters.http.header_to_metadata` HTTP filter [request-header → dynamic-metadata extraction] + fixture `0042`, REUSING the phase-33 dynamic-metadata store + `%DYNAMIC_METADATA%` operator + H1/H2 threading UNCHANGED). Lifecycle **state-5 CODE-REVIEW COMPLETE / state-6-next** (`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md` present). The next session runs the deterministic state-6 close-out (flip ROADMAP row `34` → `done`, advance STATE) — see `## Next expected skill`.
**slug:** `34-header-to-metadata`
**directory:** `docs/envoy-rust/phases/34-header-to-metadata/` (`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md` present)

**status:** **PHASE 34 (`34-header-to-metadata`) state-5 CODE-REVIEW COMPLETE — state-6 close-out next.** This session (`superpowers:requesting-code-review`) dispatched a fresh `superpowers:code-reviewer` subagent (crafted context, NOT session history) to review the phase-34 diff `4256e3e..48f8086` (the 7 task commits `bf42699`..`48f8086`) against `PLAN.md` + the §A empirically-locked facts (ADR-0084) + doctrine (D-3.2 / D-3.4 / D-3.8), and wrote `REVIEW.md`. **Verdict: APPROVE-WITH-MINORS** (ready for state-6 close) — **0 Critical, 0 Important.** The reviewer (with the reviewing session independently re-verifying the load-bearing extraction logic at `crates/envoy-filter/src/header_to_metadata.rs:21-49`) confirmed: the §A2/A3/A4 tri-state extraction is correct (present non-empty → static `value` else header value via `kv.value.clone().unwrap_or(header_value)`; present-but-empty → write nothing; absent → on_header_missing value; case-insensitive lookup; Continue-only / encode-inert); the §A5 (a)-(d) validator (boot-fatal, name-mismatch → `UnsupportedHttpFilter` checked FIRST) with a bonus symmetric empty-key-on-`on_header_missing` test; the §A1/A2 schema (`default_h2m_namespace` = the filter canonical name; `key` required; `deny_unknown_fields` rejects `cookie`/`remove`/`encode`; single-variant `HeaderToMetadataType` rejects `NUMBER`/`PROTOBUF_VALUE`); and the genuine (non-shallow) three-tier test pyramid (7 unit tests + REAL H1+H2 in-process backstops + the fixture-`0042` byte-exact cross-proxy differential with a true present/missing anti-echo guard). **Both ADR-0084 divergences (A2 default namespace; A3 static-value-wins) are CORRECT — NOT flagged as bugs.** Doctrine holds: no `unsafe`, no new crate/dependency/fuzz-target. **Three non-blocking phase-34 Minors:** **M34-1** (no unit test locks the same-namespace/key last-write-wins overwrite); **M34-2** (the A2 default namespace is exercised only at the config-parse layer, not the filter-execution layer); **M34-3** (the pre-surfaced cosmetics: the T5 redundant function-scope `use tempfile::tempdir`; the BEHAVIOR_CONTRACT `A-missing`-vs-`A5` heading-numbering quirk; the README `generate_request_id … load-bearing` overstatement). No state-3 re-entry (no Critical/Important → no re-implementation per §5.2). **§7.5 (f) (`REVIEW.md` approved) SATISFIED — the §7.5 phase-done gate is now COMPLETE.** Phase 33's open Minors **M33-1**/**M33-2** + the older carry-forwards remain live (phase 34 reused the threading UNCHANGED → NOT consumed). `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target. **DECISIONS.md ledger head: ADR-0084** (count 84; next-available **ADR-0085**). ADR-0014 in force; ADR-0028 open. The state-6 close-out is the NEXT session (the deterministic close per `BOOTSTRAP_PROMPT.md` §5 state 6: flip ROADMAP row `34` → `done`, advance STATE to the next phase / "awaiting next planning", commit + push) — see `## Next expected skill`.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 6 + `SKILL_ROUTING.md`: phase `34` is `in-progress` with `SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md` (APPROVE-WITH-MINORS) all present and the §7.5 gate (a)–(f) COMPLETE -> the next session runs the **deterministic state-6 close-out** (NO new skill invocation; the §5 state-6 mechanical close): (1) flip ROADMAP row `34` → `done`; (2) advance STATE to the next phase / "awaiting next planning" (relocate the phase-34 `## Active phase`/`## Last commit` narratives to STATE_HISTORY.md per ADR-0035, and relocate the closed-phase-34 Notes subsection); (3) commit (message `phase 34: <title> [ADR-0083, ADR-0084]`) + push + confirm CI green. **State-6 carries NO code changes (docs-only); one state per session (§5.1).** **State-5 is DONE (do NOT re-review):** `REVIEW.md` written, verdict **APPROVE-WITH-MINORS** (0 Critical / 0 Important; 3 non-blocking phase-34 Minors M34-1/M34-2/M34-3); no state-3 re-entry (no Critical/Important → no re-implementation, §5.2). The §7.5 phase-done gate is now COMPLETE (a)–(f).

**New phase-34 REVIEW.md Minors (open carry-forwards; NONE blocks; fold into the phase that next touches each surface):**
- **M34-1** — no unit test locks the same-namespace/key last-write-wins overwrite in `crates/envoy-filter/src/header_to_metadata.rs` (`multi_rule_composes` only covers distinct namespaces). Cheap test addition.
- **M34-2** — the A2 default-namespace (`envoy.filters.http.header_to_metadata`) is exercised only at the config-parse layer (`header_to_metadata_default_namespace_is_filter_name`), not at the filter-execution layer. A filter-execution unit test from a defaulted `metadata_namespace` would close the loop.
- **M34-3** — cosmetics: the T5 redundant function-scope `use tempfile::tempdir` (`crates/envoy-http1/src/hcm.rs`); the BEHAVIOR_CONTRACT `A-missing`-vs-`A5` heading-numbering quirk; the fixture README `generate_request_id … load-bearing` overstatement.

**Open carry-forward Minors (fold into the phase that next touches each surface; NONE blocks phase 34):**
- **M33-1** — unnecessary `.clone()` at `crates/envoy-http1/src/hcm.rs:1211` (the H1 record-build `dynamic_metadata` local is single-use/last-use → could move, as the H2 path does). **Phase 34 touches the H1 HCM record-build path only if it adds plumbing there — but it REUSES the existing threading UNCHANGED, so M33-1 is NOT necessarily folded this phase** (fold opportunistically if the H1 hcm.rs is edited). M33-2 (doc-pointer line drift in `command_operator.rs`/`record.rs`) likewise — doc-only.
- the empty-`metadata_match`→fallback doc-comment (`crates/envoy-cluster/src/subset.rs`); M29-1/M29-2 + M30-1; M30-2 (`Cluster.lb_policy` serde-default); the phase-31 M-2/M-3; the HTTP-filters-family (1)-(4) buffer carry-forwards. (The 6 M32 carry-forwards are CONSUMED.)

**Phase 33 is fully closed + CI-GREEN on the authoritative Linux run `28021495767` @ `72fe40d`. Phase 34's SPEC reuse claims (the store + `%DYNAMIC_METADATA%` operator + the FILTER-AGNOSTIC H1/H2 threading) were verified against the live code by the spec-reviewer — do NOT re-derive.**

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-34 state-5 code-review — dispatch the reviewer, write REVIEW.md APPROVE-WITH-MINORS, advance STATE (THIS docs-only commit):** the state-5 code-review (`BOOTSTRAP_PROMPT.md` §5 state 5; `superpowers:requesting-code-review`). Dispatched a fresh `superpowers:code-reviewer` subagent (crafted context, NOT session history) to review the phase-34 diff `4256e3e..48f8086` (the 7 task commits `bf42699`..`48f8086`) against `PLAN.md` + the §A empirically-locked facts (ADR-0084) + doctrine (D-3.2 / D-3.4 / D-3.8). **Verdict: APPROVE-WITH-MINORS** (0 Critical / 0 Important) — written to `REVIEW.md`. The reviewing session independently re-verified the load-bearing extraction logic (`crates/envoy-filter/src/header_to_metadata.rs:21-49`): §A2/A3/A4 tri-state correct (static-`value`-wins via `kv.value.clone().unwrap_or(header_value)`; present-but-empty → nothing; absent → on_header_missing value; case-insensitive; Continue-only). Both ADR-0084 divergences (A2 default namespace; A3 static-value-wins) confirmed CORRECT — NOT flagged. Three non-blocking phase-34 Minors carried forward: **M34-1** (no overwrite/last-write-wins unit test), **M34-2** (A2 default exercised only at the config layer), **M34-3** (the pre-surfaced cosmetics). **THIS docs-only commit** writes `REVIEW.md` + advances STATE `34` state-4-complete → state-5-complete / state-6-next (the state-4 `## Active phase`/`## Next expected skill`/`## Last commit`/`## Last updated` narratives RELOCATED to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9). No state-3 re-entry (no issues → no re-implementation, §5.2). §7.5 (f) SATISFIED — the phase-done gate is COMPLETE. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO code change this session. **DECISIONS.md ledger head: ADR-0084** (count 84; next-available **ADR-0085**). ADR-0014 in force; ADR-0028 open. The state-6 close-out is the NEXT session (flip ROADMAP row `34` → `done`, advance STATE, commit + push).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-23 (phase-34 **state-5 CODE-REVIEW COMPLETE / state-6-next** — dispatched a fresh `superpowers:code-reviewer` subagent [`superpowers:requesting-code-review`] to review the phase-34 diff `4256e3e..48f8086` against `PLAN.md` + the §A locked facts [ADR-0084] + doctrine; independently re-verified the extraction logic. **Verdict: APPROVE-WITH-MINORS** [0 Critical / 0 Important; 3 non-blocking phase-34 Minors M34-1/M34-2/M34-3]. Wrote `REVIEW.md`. Both ADR-0084 divergences [A2 default namespace; A3 static-value-wins] confirmed CORRECT. No state-3 re-entry [§5.2]. §7.5 (f) SATISFIED — the phase-done gate is COMPLETE [(a)–(f)]. THIS docs-only commit [`REVIEW.md` + STATE + STATE_HISTORY relocation per ADR-0035]; NO code change. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target. **DECISIONS.md ledger head: ADR-0084** [count 84; next-available ADR-0085]. ADR-0014 in force; ADR-0028 open. The state-6 close-out is the NEXT session [the deterministic §5 state-6 close: flip ROADMAP row `34` → `done`, advance STATE, commit + push].)

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
