# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `34` — `34-header-to-metadata` (**Observability / HTTP-filters family: request-driven metadata** — the `envoy.filters.http.header_to_metadata` HTTP filter [request-header → dynamic-metadata extraction] + fixture `0042`, REUSING the phase-33 dynamic-metadata store + `%DYNAMIC_METADATA%` operator + H1/H2 threading UNCHANGED). Lifecycle **state-1 BRAINSTORM COMPLETE / state-2-next** (`SPEC.md` present; `PLAN.md` + `PROGRESS.md` + `REVIEW.md` absent). The next session runs `superpowers:writing-plans` (the state-2 PLAN-write) — see `## Next expected skill`.
**slug:** `34-header-to-metadata`
**directory:** `docs/envoy-rust/phases/34-header-to-metadata/` (`SPEC.md` present; `PLAN.md` + `PROGRESS.md` + `REVIEW.md` absent)

**status:** **PHASE 34 (`34-header-to-metadata`) state-1 BRAINSTORM COMPLETE — state-2 PLAN-write next.** This session (`superpowers:brainstorming`) picked phase 34 (the next phase after the phase-33 close; `33` had been the highest ROADMAP row → "awaiting next planning") and authored its `SPEC.md`. **Pick: `header_to_metadata`** — the request-header-driven dynamic-metadata emitter that the phase-33 SPEC §2.2 + ADR-0080 explicitly named "the natural next pick after phase 33". **Scope: the thin reuse-slice** (ADR-0083): the 12th `HttpFilterInstance` variant, decode-side `Continue`-only, `request_rules` only, string-only `on_header_present` (header value or static `value` override) + `on_header_missing` (static fallback), merging into `req.dynamic_metadata` under `metadata_namespace`→`key`; + a `HeaderToMetadataConfig` + a `HttpFilterTypedConfig::HeaderToMetadata` variant (`@type …header_to_metadata.v3.Config`) + a `validate_http_filters` arm; + fixture `0042` (H1 `direct_response`, `[header_to_metadata, router]` chain, `%DYNAMIC_METADATA%`-bearing `log_format`, header-present + header-missing probes via the existing `AccessLogByteExactProbe.extra_headers`). **REUSES the phase-33 store + `%DYNAMIC_METADATA(ns:key)%` operator + H1/H2 capture-before-drop threading UNCHANGED** (no new infrastructure — `envoy-accesslog` untouched; the threading already carries any `req.dynamic_metadata`). A STRONGER, request-driven, byte-exact + DETERMINISTIC + LOCALLY-observable differential than `set_metadata`'s static literal. **DEFERRED (§2.2):** `response_rules`, typed (NUMBER/PROTOBUF) values + the non-string Value-enum, `encode: BASE64`, `regex_value_rewrite`, the `remove` header-mutation, per-route config, metadata consumers. **SPEC spec-reviewed by a fresh subagent → APPROVE-WITH-MINORS** (0 Critical / 0 Important / 3 presentational Minors — all FOLDED in-review: the `extra_headers` harness fact moved from §6.2 to the §4 reuse map as confirmed; the string-only-value stricter-than-Envoy clarification; the `type`-field MVP clarification). **§6.1 split projected NOT to fire** (~600–900 LoC / ~7–9 tasks — smaller than phase 33, adds no infrastructure). `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target projected. **DECISIONS.md ledger head: ADR-0083** (this session's pick+scope ADR; count 82 → 83; next-available **ADR-0084**, reserved for the §6.2 reconciliation). ADR-0014 in force; ADR-0028 open. Phase 33 is CLOSED (CI-GREEN at run `28021495767` @ `72fe40d`); its open Minors **M33-1**/**M33-2** carry forward. The state-2 PLAN-write is the NEXT session (`superpowers:writing-plans`), which runs the §6.2 LOCAL reconnaissance against `envoyproxy/envoy:v1.33.0` and fires ADR-0084 — see `## Next expected skill`.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 2 + `SKILL_ROUTING.md`: phase `34` is `in-progress` with `SPEC.md` present and `PLAN.md` absent -> the next session runs **`superpowers:writing-plans`** (the state-2 PLAN-write). It MUST: (1) run the **§6.2 empirical reconnaissance LOCALLY** against `envoyproxy/envoy:v1.33.0` (an H1 listener + a `header_to_metadata` filter + a file access logger with a `%DYNAMIC_METADATA%`-bearing `log_format`) to lock the open SPEC §3 calls — the `request_rules`/`Rule`/`KeyValuePair` wire shape, the default `metadata_namespace` (`envoy.lb`), the `on_header_present` value-vs-static-override precedence, the `on_header_missing` semantics, the present-but-empty-header disposition, the byte form of the extracted value, and the config-validity disposition; (2) **fire ADR-0084** with the locked §A facts (or record CONFIRMATIONS if all projections hold); (3) write `PLAN.md` with the empirically-locked facts inline (no `[§6.2-PENDING]` — the verify-at-PLAN-write discipline) + the TDD task breakdown (projected ~7–9 tasks). Read the phase-33 `PLAN.md` as the structural template (its `## §A` empirically-locked-facts section + the per-task TDD format). **State-1 is DONE (do NOT re-brainstorm):** `SPEC.md` is written + spec-reviewed APPROVE-WITH-MINORS (the 3 Minors folded in-review); ADR-0083 fired (the pick+scope lock); ROADMAP row `34` is `in-progress`.

**Open carry-forward Minors (fold into the phase that next touches each surface; NONE blocks phase 34):**
- **M33-1** — unnecessary `.clone()` at `crates/envoy-http1/src/hcm.rs:1211` (the H1 record-build `dynamic_metadata` local is single-use/last-use → could move, as the H2 path does). **Phase 34 touches the H1 HCM record-build path only if it adds plumbing there — but it REUSES the existing threading UNCHANGED, so M33-1 is NOT necessarily folded this phase** (fold opportunistically if the H1 hcm.rs is edited). M33-2 (doc-pointer line drift in `command_operator.rs`/`record.rs`) likewise — doc-only.
- the empty-`metadata_match`→fallback doc-comment (`crates/envoy-cluster/src/subset.rs`); M29-1/M29-2 + M30-1; M30-2 (`Cluster.lb_policy` serde-default); the phase-31 M-2/M-3; the HTTP-filters-family (1)-(4) buffer carry-forwards. (The 6 M32 carry-forwards are CONSUMED.)

**Phase 33 is fully closed + CI-GREEN on the authoritative Linux run `28021495767` @ `72fe40d`. Phase 34's SPEC reuse claims (the store + `%DYNAMIC_METADATA%` operator + the FILTER-AGNOSTIC H1/H2 threading) were verified against the live code by the spec-reviewer — do NOT re-derive.**

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-34 state-1 brainstorm — pick `header_to_metadata`, author SPEC.md, fire ADR-0083, append ROADMAP row, advance STATE (THIS docs-only commit):** the state-1 new-phase brainstorm (`BOOTSTRAP_PROMPT.md` §5 state 0→1 + §10; `superpowers:brainstorming`). Picked phase 34 (`34-header-to-metadata`) — the request-header-driven dynamic-metadata emitter the phase-33 SPEC named "the natural next pick" — at the **thin reuse-slice** scope (request_rules only, string-only, decode-side; reuses the phase-33 store + `%DYNAMIC_METADATA%` operator + H1/H2 threading UNCHANGED). Authored `docs/envoy-rust/phases/34-header-to-metadata/SPEC.md` (§0–§7, the verify-at-PLAN-write discipline); **fired ADR-0083** (the pick+scope lock); **appended ROADMAP row `34`** (`in-progress`). The SPEC was spec-reviewed by a fresh subagent → **APPROVE-WITH-MINORS** (0 Critical / 0 Important / 3 presentational Minors, all FOLDED in-review). **THIS docs-only commit** (SPEC.md + ADR-0083 in DECISIONS.md + ROADMAP row 34 + STATE + STATE_HISTORY relocation per ADR-0035). One state per session (§5.1): state-1 is brainstorm/SPEC ONLY — the state-2 PLAN-write is the NEXT session (writing-plans NOT invoked this session). `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target projected. **DECISIONS.md ledger head: ADR-0083** (count 82 → 83; next-available **ADR-0084**, reserved for the §6.2 reconciliation). ADR-0014 in force; ADR-0028 open. Phase 33 CLOSED + CI-GREEN (run `28021495767` @ `72fe40d`). The state-2 PLAN-write is the NEXT session (`superpowers:writing-plans`).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-23 (phase-34 **state-1 BRAINSTORM COMPLETE / state-2-next** — picked phase `34-header-to-metadata` [`superpowers:brainstorming`; the request-header-driven dynamic-metadata emitter the phase-33 SPEC named "the natural next pick"] at the **thin reuse-slice** scope [request_rules only, string-only, decode-side `Continue`-only; REUSES the phase-33 store + `%DYNAMIC_METADATA%` operator + H1/H2 threading UNCHANGED]. Authored `docs/envoy-rust/phases/34-header-to-metadata/SPEC.md`; **fired ADR-0083** [pick+scope]; **appended ROADMAP row `34`** [`in-progress`]. SPEC spec-reviewed by a fresh subagent → **APPROVE-WITH-MINORS** [0C/0I/3 presentational Minors, all FOLDED in-review: the `extra_headers` harness fact moved §6.2→§4-confirmed; the string-only-value stricter-than-Envoy note; the `type`-field MVP note]. THIS docs-only commit [SPEC + ADR-0083 + ROADMAP row 34 + STATE + STATE_HISTORY relocation per ADR-0035]. One state per session [§5.1]: writing-plans NOT invoked — the state-2 PLAN-write is the NEXT session. §6.1 split projected NOT to fire [~7–9 tasks]. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target projected. **DECISIONS.md ledger head: ADR-0083** [count 83; next-available ADR-0084, reserved §6.2 reconciliation]. ADR-0014 in force; ADR-0028 open. Phase 33 CLOSED + CI-GREEN [run `28021495767` @ `72fe40d`]. The state-2 PLAN-write is the NEXT session [`superpowers:writing-plans`].)

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
