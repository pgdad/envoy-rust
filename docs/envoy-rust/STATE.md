# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** **NONE — AWAITING NEXT PLANNING.** Phase `33` (`33-set-metadata-dynamic-metadata`) is **CLOSED** (state-6 close-out 2026-06-23; ROADMAP row `33` flipped `done`). **`33` is the highest defined ROADMAP row → there is no queued next phase.** The next session is a **new-phase pick + state-1 brainstorm** (`superpowers:brainstorming`) that appends the next ROADMAP row(s) and authors the new phase's `SPEC.md` — see `## Next expected skill`.
**slug:** _(none — no active phase)_
**directory:** _(none — the most recently closed phase is `docs/envoy-rust/phases/33-set-metadata-dynamic-metadata/`, `SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md` all present, ROADMAP `done`)_

**status:** **PHASE 33 (`33-set-metadata-dynamic-metadata`) CLOSED — state-6 close-out COMPLETE; AWAITING NEXT PLANNING.** This session (the state-6 deterministic close, `BOOTSTRAP_PROMPT.md` §5 state 6) closed phase 33: pushed the 15 unpushed phase-33 commits to `origin/main`, confirmed the **AUTHORITATIVE Linux CI run `28021495767` @ `72fe40d` is `success`** (`build + test + lint` + `fuzz` jobs — phase-33 code's FIRST real CI execution: fixture `0041` cross-proxy byte-exact + all `0001`-`0040` green + the byte-identical `0012` witness + h2spec ≥95% [§3.5/2 known-failure correct on CI] + the 4 fuzz targets 0 crashes + build/clippy/fmt/test/deny clean), flipped ROADMAP row `33` → `done`, and advanced STATE to "awaiting next planning" (relocating the phase-33 `## Active phase` narratives to STATE_HISTORY.md per ADR-0035). **Phase-33 delivered:** the dynamic-metadata critical-path unlock — the per-request dynamic-metadata store (`FilterRequest`+`AccessLogRecord` fields), the `envoy.filters.http.set_metadata` filter (11th `HttpFilterInstance` variant, decode-side `Continue`-only) + config (`@type …v3.Config`) + validator + `ConfigError::SetMetadataEmptyNamespace`, `Op::DynamicMetadata` (no `truncate`; `:N`/no-arg/non-two-segment fatal; raw unquoted; absent `-`; case-sensitive), the dual H1+H2 capture-before-drop threading + backstops, fixture `0041` + BEHAVIOR_CONTRACT + fuzz seeds, the 6 M32 carry-forwards CONSUMED. **REVIEW.md verdict: APPROVE-WITH-MINORS** (0 Critical / 0 Important / 3 Minor). **§A facts locked by ADR-0081** (two MATERIAL §6.2 divergences from the SPEC projection: `@type …v3.Config` not `…v3.SetMetadata`; no `:N` truncation on `%DYNAMIC_METADATA%`). The **§6.1 split did NOT fire** (ADR-0082 reserved-but-UNFIRED). `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target. **NEW open Minors carried forward: M33-1** (H1 `crates/envoy-http1/src/hcm.rs:1211` `.clone()`→move) + **M33-2** (doc-pointer line drift in `command_operator.rs`/`record.rs`). **DECISIONS.md ledger head: ADR-0081** (count 82; next-available **ADR-0083**). ADR-0014 in force; ADR-0028 open. **ROADMAP rows `00`-`33` ALL `done`.** The next session picks the next phase + runs its state-1 brainstorm — see `## Next expected skill`.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 0/1 + §10 + `SKILL_ROUTING.md`: **no active phase** — phase `33` is CLOSED (ROADMAP `done`) and `33` is the highest defined ROADMAP row, so there is NO queued next phase. The next session runs a **new-phase pick + state-1 brainstorm** (`superpowers:brainstorming`): pick the next Observability/HTTP-filters-family target, APPEND its ROADMAP row(s), author the new phase's `SPEC.md` under `docs/envoy-rust/phases/NN-<slug>/`, and fire the next-available ADR (**ADR-0083**) for the scope/pick decision. **Natural next pick (per the phase-33 SPEC §2.2 deferral):** **`header_to_metadata`** (`envoy.filters.http.header_to_metadata`) — the request-header-driven metadata emitter that REUSES this phase's dynamic-metadata store + `%DYNAMIC_METADATA%` operator UNCHANGED (a pure-additive `HttpFilterInstance` variant) and yields a stronger request-driven differential; explicitly named "the natural next pick after phase 33". Other live veins: the rest of the Observability family (`json_format`/`typed_json_format`, gRPC ALS, OTLP sink, tracing, stats sinks, tap); non-string metadata Values + nested-path `%DYNAMIC_METADATA%`; metadata consumers (jwt_authn `payload_in_metadata`, rbac dynamic-metadata conditions, ext_authz/ext_proc). The brainstorm picks per the §6 leverage criteria.

**Open carry-forward Minors (fold into the phase that next touches each surface; NONE blocks):**
- **M33-1** (NEW) — unnecessary `.clone()` at `crates/envoy-http1/src/hcm.rs:1211` (the H1 record-build `dynamic_metadata` local is single-use/last-use → could move, as the H2 path does). Fold when the H1 HCM record-build is next touched.
- **M33-2** (NEW) — doc-pointer line drift: `command_operator.rs`/`record.rs` doc comments hardcode `~1189`/`~888` vs the actual `~1211`/`finalize_h2_stream`. Doc-only; fold on a future `hcm.rs` touch. (M33-3 was noted-only — no action.)
- the empty-`metadata_match`→fallback doc-comment (`crates/envoy-cluster/src/subset.rs`); M29-1/M29-2 + M30-1 (the `Http1HashSweep` driver diagnostics / duplicated `extract_marker`); M30-2 (`Cluster.lb_policy` serde-default); the phase-31 cosmetic Minors M-2/M-3; the HTTP-filters-family (1)-(4) buffer carry-forwards. (The 6 M32 carry-forwards are CONSUMED.)

**Phase 33 is fully closed + CI-GREEN on the authoritative Linux run `28021495767` @ `72fe40d` — do NOT re-verify.**

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-33 state-6 close-out — push, confirm CI green, flip ROADMAP `33` → done, advance STATE to awaiting-next-planning (THIS commit):** the state-6 deterministic close (`BOOTSTRAP_PROMPT.md` §5 state 6, the "Reviewed and approved" terminal step). Pushed the 15 unpushed phase-33 commits `4e9e7f6..72fe40d` to `origin/main`; the push triggered the **AUTHORITATIVE Linux CI run `28021495767` @ `72fe40d`** — phase-33 code's FIRST real CI execution — which completed **`success`** (`build + test + lint` 4m20s + `fuzz` 7m49s): fmt/clippy/build/`test` [the full differential incl. fixture `0041` + all `0001`-`0040` on CI's `172.17.0.1` — the host-sensitive admin-dump/upstream/h2spec set GREEN here unlike locally] / deny clean, all 4 fuzz targets 0 crashes. Then flipped **ROADMAP row `33` → `done`** (with the close-out block citing CI run `28021495767`) and advanced STATE to "awaiting next planning" (phase-33 `## Active phase` status + last-commit narratives RELOCATED to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9). **THIS docs-only close-out commit** (ROADMAP + STATE + STATE_HISTORY). Phase 33 CLOSED; **ROADMAP rows `00`-`33` ALL `done`** (`33` is the highest defined row). `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target. **DECISIONS.md ledger head: ADR-0081** (count 82; next-available **ADR-0083**; ADR-0082 reserved-but-UNFIRED for the §6.1 split that did NOT fire). ADR-0014 in force; ADR-0028 open. The next session is a new-phase pick + state-1 brainstorm (`superpowers:brainstorming`; natural next pick: `header_to_metadata`).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-23 (phase-33 **CLOSED — state-6 close-out COMPLETE; AWAITING NEXT PLANNING**. Pushed the 15 phase-33 commits `4e9e7f6..72fe40d`; the **AUTHORITATIVE Linux CI run `28021495767` @ `72fe40d` is `success`** [phase-33 code's first real CI execution — `build + test + lint` + `fuzz` jobs: fixture `0041` cross-proxy byte-exact + all `0001`-`0040` green incl. the `0012` witness + h2spec ≥95% + the 4 fuzz targets 0 crashes + build/clippy/fmt/test/deny clean]. Flipped **ROADMAP row `33` → `done`** [close-out block cites CI `28021495767`] and advanced STATE to "awaiting next planning" [phase-33 `## Active phase` status + last-commit narratives relocated to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9]. Phase 33 delivered the dynamic-metadata critical-path unlock [store + `set_metadata` filter + `%DYNAMIC_METADATA%` operator + dual H1+H2 threading + fixture `0041`]; **REVIEW APPROVE-WITH-MINORS** [0C/0I/3 Minor]; §A locked by **ADR-0081**; §6.1 split UNFIRED [ADR-0082 reserved]. NEW open Minors **M33-1** [H1 `hcm.rs:1211` `.clone()`→move] + **M33-2** [doc-pointer drift] carried forward. **ROADMAP rows `00`-`33` ALL `done`** [`33` highest defined row]. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target. **DECISIONS.md ledger head: ADR-0081** [count 82; next-available ADR-0083]. ADR-0014 in force; ADR-0028 open. The next session is a new-phase pick + state-1 brainstorm [`superpowers:brainstorming`; natural next pick `header_to_metadata`].)

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
