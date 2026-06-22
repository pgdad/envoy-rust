# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `33` — `33-set-metadata-dynamic-metadata` (**Observability family: the dynamic-metadata critical-path unlock** that phase 32 enabled — the `envoy.filters.http.set_metadata` HTTP filter [a static-value metadata emitter] + a per-request **dynamic-metadata store** + the `%DYNAMIC_METADATA(namespace:key)%` access-log command-operator + fixture `0041`). Lifecycle **state-1 BRAINSTORM COMPLETE / state-2-next** (`SPEC.md` present, scope locked by **ADR-0080**; `PLAN.md` absent). The next session runs `superpowers:writing-plans` — see `## Next expected skill`.
**slug:** `33-set-metadata-dynamic-metadata`
**directory:** `docs/envoy-rust/phases/33-set-metadata-dynamic-metadata/` (`SPEC.md` present; `PLAN.md`/`PROGRESS.md`/`REVIEW.md` absent)

**status:** **PHASE 33 (`33-set-metadata-dynamic-metadata`) state-1 BRAINSTORM COMPLETE — state-2 PLAN-write next.** This session (`superpowers:brainstorming`) picked + scoped phase 33: created `docs/envoy-rust/phases/33-set-metadata-dynamic-metadata/SPEC.md`, appended **ADR-0080** (the pick + minimum-viable scope + rejected alternatives), added ROADMAP row `33` (`in-progress`), and advanced STATE AWAITING-NEXT-PLANNING → `33` state-1-complete/state-2-next. **Pick:** land the smallest end-to-end differentially-testable dynamic-metadata loop — (1) a per-request dynamic-metadata store (projected string-only `BTreeMap<String, BTreeMap<String, String>>`, additive on `FilterRequest` + `AccessLogRecord`, threaded at BOTH the H1 [`crates/envoy-http1/src/hcm.rs` ~1189] AND H2 [`crates/envoy-http2/src/hcm.rs` ~888] independent record-build sites); (2) the `set_metadata` filter (11th `HttpFilterInstance` variant, decode-side `Continue`-only); (3) the `%DYNAMIC_METADATA(namespace:key)%` operator (slots additively into the phase-32 engine). **Differential:** byte-exact cross-proxy access-log line (the static-config metadata value), DETERMINISTIC + LOCALLY observable (the `Driver::Http1AccessLogByteExact` file scrape, no reload trigger). `set_metadata` chosen over `header_to_metadata` (the latter the pure-additive follow-up reusing this store + operator). **§6.1 split projected NOT to fire** (~900–1200 LoC / ~10–12 tasks; ADR-0082 reserved). The SPEC was **spec-reviewed by a fresh subagent → 1 Critical (C-1: the H2 record-build is a SEPARATE site, NOT inherited from H1 — FOLDED) + 4 Minor (FOLDED)**; the SPEC, ADR-0080, and the ROADMAP row carry the corrected dual-site H2 threading. `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0080** (count 81; next-available **ADR-0081**, reserved for the §6.2 reconciliation; **ADR-0082** reserved for the §6.1 split). ADR-0014 in force; ADR-0028 open. The state-2 PLAN-write is the NEXT session (`superpowers:writing-plans`) — see `## Next expected skill`.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 2 + `SKILL_ROUTING.md`: phase `33` is `in-progress` with `SPEC.md` present and `PLAN.md` absent -> the next session runs **`superpowers:writing-plans`** to author `docs/envoy-rust/phases/33-set-metadata-dynamic-metadata/PLAN.md`. **FIRST run the §6.2 empirical reconnaissance LOCALLY** (the phase-22/23/28/29/30/31/32 verify-at-PLAN-write discipline) against live `envoyproxy/envoy:v1.33.0` — an H1 listener + a `set_metadata` filter + a file access logger whose `log_format` uses `%DYNAMIC_METADATA(...)%` — to LOCK (per SPEC §3/§6.2): the **`set_metadata` config wire shape** (older `metadata_namespace`+`value` vs newer `key`+`value`+`allow_overwrite`; the namespace the value lands under); the **`%DYNAMIC_METADATA(ARG)%` arg grammar** (`:` path separator, nested-path acceptance, `:N`-truncation composition); the **EXACT byte form of a resolved string metadata value** (raw `prod` vs JSON-quoted `"prod"` — THE key differential risk) + the absent-namespace/key rendering (`-` vs empty vs `{}`); and the **config-validity disposition** (malformed `set_metadata` / deeper-than-MVP `%DYNAMIC_METADATA%` — boot-fatal per ADR-0049 vs accept-and-degrade). **ADR-0081 FIRES** at the PLAN-write on any material divergence from the SPEC projection. GATE (§5 state 2): if `PLAN.md` > ~25 tasks OR > ~1500 LoC estimated → split into `33.1`/`33.2` (ADR-0082) + update ROADMAP/STATE + stop (projected NOT to fire — ~10–12 tasks / ~900–1200 LoC). **CARRY-FORWARDS the state-2/3 work should FOLD (phase 33 touches `crates/envoy-accesslog/` [command_operator + record] AND `crates/envoy-filter/`):** the **6 phase-32 REVIEW.md Minors M32-1…M32-6** (the `command_operator.rs`/accesslog/fixture-polish — `side: &'static str`→`enum Side`; empty-alt+`:0`; named error-field diagnostics; the in-crate default-equivalence single-record narrowing; the vestigial 0-byte `inputs/payload.bin`; the `render` 256-byte pre-alloc — **FOLD on this accesslog touch** per the next-prompt directive) — see `### Phase-33 carry-forwards` below. Other still-live carry-forwards (weigh if touched): the empty-`metadata_match`→fallback doc-comment; M29-1/M29-2 + M30-1 (the `Http1HashSweep` driver diagnostics / duplicated `extract_marker`); M30-2 (`lb_policy` serde-default); the phase-31 cosmetic Minors M-2/M-3; the HTTP-filters-family (1)-(4) buffer carry-forwards.

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-33 state-1 brainstorm — pick + scope phase 33 (THIS commit):** the state-1 NEW-PHASE brainstorm (`BOOTSTRAP_PROMPT.md` §5 state 0/1; `superpowers:brainstorming`). Picked + scoped phase `33-set-metadata-dynamic-metadata` (the Observability-family dynamic-metadata critical-path unlock). THIS docs-only commit (NO code change) creates `docs/envoy-rust/phases/33-set-metadata-dynamic-metadata/SPEC.md`; appends **ADR-0080** (the pick + minimum-viable scope + rejected alternatives + the §6.1 split projection) to DECISIONS.md; adds ROADMAP row `33` (`in-progress`); advances STATE AWAITING-NEXT-PLANNING → `33` state-1-complete/state-2-next (the phase-32-CLOSE top-section blocks demoted to `_Historical_` + RELOCATED to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9, leaving the breadcrumbs). The SPEC was spec-reviewed by a fresh subagent → 1 Critical (C-1: H2 has a SEPARATE record-build site at `crates/envoy-http2/src/hcm.rs` ~888, NOT inherited from H1 — the SPEC/ADR/ROADMAP corrected to a dual-site H2 threading task) + 4 Minor, all FOLDED. `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0080** (count 81; next-available **ADR-0081** reserved for the §6.2 reconciliation; **ADR-0082** reserved for the §6.1 split). ADR-0014 in force; ADR-0028 open. The state-2 PLAN-write is the NEXT session (`superpowers:writing-plans`).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-22 (phase-33 **state-1 BRAINSTORM COMPLETE / state-2-next** — the NEW-PHASE pick + scope [`superpowers:brainstorming`]. Picked `33-set-metadata-dynamic-metadata` [the Observability-family dynamic-metadata critical-path unlock: the `set_metadata` filter + a per-request dynamic-metadata store + the `%DYNAMIC_METADATA(namespace:key)%` access-log operator + fixture `0041`]. Created the SPEC; appended **ADR-0080** [pick + scope + rejected alternatives]; added ROADMAP row `33` [`in-progress`]; advanced STATE AWAITING-NEXT-PLANNING → `33` state-1-complete/state-2-next [the phase-32-CLOSE top-section blocks relocated to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9]. Spec-reviewed by a fresh subagent → 1 Critical [C-1, the H2 dual record-build site] + 4 Minor, all FOLDED. Docs-only commit [SPEC + DECISIONS.md + ROADMAP.md + STATE.md + STATE_HISTORY.md]. `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0080** [count 81; ADR-0081 reserved §6.2 reconciliation; ADR-0082 reserved §6.1 split]. ADR-0014 in force; ADR-0028 open. The state-2 PLAN-write is the NEXT session [`superpowers:writing-plans`].)

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

> Phase 33 touches `crates/envoy-accesslog/` (`command_operator.rs` gains `Op::DynamicMetadata`; `record.rs` gains the `dynamic_metadata` field) AND `crates/envoy-filter/` (the `set_metadata` filter + `FilterRequest.dynamic_metadata`) AND `crates/envoy-config/` (the `SetMetadata` filter config + validator). Per the next-prompt directive, **FOLD the 6 phase-32 REVIEW.md Minors M32-1…M32-6 on this accesslog touch**:

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
