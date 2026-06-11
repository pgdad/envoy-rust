# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** _none_ — **phase `23` (`23-http-filter-cors`) is CLOSED.** The state-6 deterministic close-out landed at THIS commit: ROADMAP row `23` flipped `in-progress → done` (rows `00`–`23` are now ALL `done`; `23` is the highest defined row — there is no row 24 yet) and STATE advances to **AWAITING NEXT PLANNING**. There is **NO active phase directory**; the four phase-23 artifacts (`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md`) remain at `docs/envoy-rust/phases/23-http-filter-cors/`, CLOSED. The five most-recently-closed phase directories — 23, 22 `22-http-filter-jwt-authn`, 21 `21-xds-file-based-eds`, 20, 19 — each carry all 4 artifacts.

**status:** **AWAITING NEXT PLANNING — no active phase.** Phase 23 (`envoy.filters.http.cors` + the project's first per-route `typed_per_filter_config` infrastructure) closed APPROVED: the state-5 `REVIEW.md` verdict was 0 Critical / 0 Important / 7 Minor (M23-1…M23-7), all non-gating — folded into a future M-track sweep, NOT a §5.2 state-3 re-entry (the most valuable follow-up is M23-6 token-aware `Connection: close` detection bundled with M23-7 HTTP/1.0 implicit-close). The §7.5 gate was GREEN at state-4 (AUTHORITATIVE Linux CI anchor run `27317400787` at code-HEAD `ff3721fd1` — the 31-fixture Docker differential incl. the new `0031-http-filter-cors` + h2spec ≥95% + fuzz). The next session runs the **next-NEW-PHASE brainstorm** (`superpowers:brainstorming`) to SELECT + SCOPE phase `24` from `MISSION.md`'s remaining §9 feature-families — the HTTP-filters family still has ≈12 filters undone (csrf / buffer / compression / ext_authz / ext_proc / oauth2 / lua / wasm / adaptive-concurrency / admission-control / bandwidth-limit), and phase 23's per-route-config infra now unlocks the per-route-configured ones; other open families: network-filters, load-balancing, HTTP/3+QUIC, gRPC (still H2-trailers-blocked), the xDS gRPC/ADS transport, observability, runtime/hot-restart, WASM. That brainstorm creates `docs/envoy-rust/phases/24-<slug>/` + `SPEC.md` at lifecycle state-1. No code change this session (doc-only close-out); no new ADR — DECISIONS.md ledger head stays **ADR-0059** (count 60; next available **ADR-0060**). ADR-0014 in force; ADR-0028 open.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-23 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 states 0/1 + `SKILL_ROUTING.md`: with phase `23` CLOSED and no active phase directory, the next session runs the **next-NEW-PHASE brainstorm** — `superpowers:brainstorming` — to SELECT + SCOPE phase `24` from `MISSION.md`'s remaining §9 feature-family roadmap. Phase `23` is the highest defined ROADMAP row (rows run `00`…`23`; there is no row 24 yet), so the brainstorm both PICKS the next family/filter AND scopes it, creating `docs/envoy-rust/phases/24-<slug>/` + its `SPEC.md` at lifecycle state-1 (state 1 of the §5 machine). The 7 phase-23 Minors (M23-1…M23-7) are M-track follow-ups recorded in `REVIEW.md` — NOT next-session work. Per §5.1 (one state per session) that brainstorm is the NEXT session's single state.

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-23 state-6 deterministic close-out commit (THIS commit):** flips **ROADMAP row `23` `in-progress → done`** (parent-row schema invariant 4.1.2 — rows `00`–`23` are now ALL `done`) and advances **STATE → phase-23 CLOSED / AWAITING NEXT PLANNING** (rewrites `## Active phase` + `## Next expected skill` to point at the next-NEW-PHASE brainstorm `superpowers:brainstorming` for phase `24`), relocating the now-superseded phase-23 **state-5** top-section narrative + the full phase-23 `## Notes` subsections (state-1…state-5) verbatim to `STATE_HISTORY.md` (ADR-0035). **No code change** (doc-only: `ROADMAP.md` + `STATE.md` + `STATE_HISTORY.md`). **No new ADR** — DECISIONS.md ledger head stays **ADR-0059** (count 60; next available **ADR-0060**). ADR-0014 in force; ADR-0028 open. The §7.5 gate was GREEN at state-4 (CI anchor `27317400787` at code-HEAD `ff3721fd1`); the close-out is doc-only so there is NO re-CI obligation (the doc commits may be pushed for durability). Per §5.1 the NEXT session is the next-NEW-PHASE brainstorm for phase `24` (see `## Next expected skill`).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-11 (phase-23 **state-6 deterministic close-out** — flipped ROADMAP row `23` `in-progress → done` [rows `00`–`23` now ALL `done`] + advanced STATE → **phase-23 CLOSED / AWAITING NEXT PLANNING** [rewrote `## Active phase` + `## Next expected skill` to point at the next-NEW-PHASE brainstorm `superpowers:brainstorming` for phase `24`] + relocated the superseded phase-23 state-5 top-section narrative + the full phase-23 `## Notes` subsections [state-1…state-5] verbatim to `STATE_HISTORY.md` per ADR-0035. Doc-only [`ROADMAP.md` + `STATE.md` + `STATE_HISTORY.md`]; no code change; no new ADR; ledger head **ADR-0059** [count 60; next available **ADR-0060**]. ADR-0014 in force; ADR-0028 open. Per §5.1 the next-NEW-PHASE brainstorm is the NEXT session.)

> Historical `## Last updated` notes — every superseded last-updated note (all closed phases + the active phase's prior sub-state notes) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Notes

> Historical Notes subsections for fully-closed phases 00-07 (ADR-numbering notes, per-phase rollovers, ADR ledgers, and the earlier-phase-carryforward + phase-00-deferral snapshots) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. State-6 close-out commits (this commit) touch ROADMAP.md + STATE.md only and carry no code changes; the next session writes PLAN.md for the next active phase per `superpowers:writing-plans`.
- The reviewer's R2 disposition decision (option (a) retroactive split of 05.1 vs option (b) free-standing post-05.1 sub-phase) was settled at the 05.1 state-6 commit in favor of option (b); 05.4 is the chosen sibling sub-phase. Future-reviewers reading STATE.md should understand that 05.1 is structurally closed at the preamble landing; 05.4 is a SIBLING under parent-05, not a child of 05.1; and the execution order ran 05.1 → 05.4 → 05.2 → 05.3, with 05.3 the closing sub-phase that flips parent-05 to `done`.

> Historical Notes subsections for fully-closed phases 05.4 / 08 / 09 / 10 (brainstorm, split, PLAN-write, execution-arc, rollovers, and ADR-ledger narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phases 11–21 (brainstorm / split / PLAN-write / execution-arc + verification / code-review / rollovers narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 22 (brainstorm / PLAN-write / execution-arc + state-4 verification / code-review / rollovers narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 23 (state-1 brainstorm / state-2 PLAN-write / state-3 execution arc / state-4 verification gate / state-5 code-review narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.
