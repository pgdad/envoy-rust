# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `32` - `32-accesslog-command-operators` (**Observability family opener**: the phase-06.2 hardcoded Envoy-v3 default-format emitter generalized into a configurable **command-operator substitution engine** + a per-`FileAccessLog` `log_format` text-format field + a curated DETERMINISTIC operator set). Lifecycle **state-4 verification COMPLETE / state-5-next** (`SPEC.md` + `PLAN.md` + `PROGRESS.md` present [scope locked by **ADR-0078**; §6.2 wire facts by **ADR-0079**]; the §7.5 phase-done gate (a)-(e) GREEN on the authoritative Linux CI; `REVIEW.md` absent -> the next step is the state-5 code-review `superpowers:requesting-code-review`). **ROADMAP rows `00`-`31` ALL `done`;** row `32` `in-progress`.
**slug:** `32-accesslog-command-operators`
**directory:** `docs/envoy-rust/phases/32-accesslog-command-operators/` - carries **`SPEC.md` + `PLAN.md` + `PROGRESS.md`** (state-1/2/3 outputs + the state-4 verification record). `REVIEW.md` not yet authored. `PROGRESS.md` records the 8 TDD task commits (`7917c8a`…`cb7a191`) + the state-4 §7.5 verification record (authoritative CI run `27941931062` → success). Fixture `0040` cross-proxy byte-exact differential GREEN (CI + locally); fixture `0012` byte-preserved; all `0001`-`0039` green simultaneously on CI (the engine is inert/default for every listener without a `log_format`).

**status:** **PHASE 32 STATE-4 VERIFICATION COMPLETE / state-5-next.** Ran `superpowers:verification-before-completion` — the §7.5 phase-done gate. The 9 state-3 commits (`7917c8a`…`ecb62d3`) were committed-but-unpushed at session start; pushed `783d29f..ecb62d3` → triggered the AUTHORITATIVE Linux CI run **`27941931062` (commit `ecb62d3`) → conclusion success**, every step green: **(a)** fixture 0040 `access_log_command_operators ... ok`; **(b)** `admin_config_dump_server_info ... ok` + ZERO failures in the 27.6k-line log → all `0001`-`0039` (incl. `0012` byte-identical witness) green alongside `0040`; **(c)** h2spec `h2spec_pass_rate_gate ... ok` (≥95%); **(d)** all 4 fuzz steps green (incl. the hand-wired `accesslog_format_parse`); **(e)** `fmt`/`clippy`/`build`/`test`/`cargo deny check` (→ `advisories ok, bans ok, licenses ok, sources ok`) all green. Local corroboration on this host matched EXCEPT fixture `0014` differential, root-caused (`superpowers:systematic-debugging`) as a **host-networking allow-list gap, NOT a phase-32 regression**: this Docker Desktop host routes the backend via `192.168.65.2` (host.docker.internal host-gateway) — a bridge IP in neither of fixture 0014's allow-list prefixes (`192.168.65.254` macOS-gvisor / `172.17.0.1` Linux-CI); phase 32 touches zero cluster/admin/listener code (`git diff --stat` + empty `text_lines|allow|admin` grep over the `lib.rs` diff) and 0014 has no access-log content → 0014's behavior is byte-identical to base, and it is GREEN on the authoritative CI (`172.17.0.1`). No source/fixture change made (state-4 = verify; the authoritative gate did not fail). Local fuzz also clean (accesslog 4.7M / parse_bootstrap 444k / jwt 9.5M / cdn_loop 10.7M runs, 0 crashes). **(f) `REVIEW.md` is the state-5 session.** `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0079** (count 80; **ADR-0080** reserved-but-UNFIRED). ADR-0014 in force; ADR-0028 open. Per §5.1 the NEXT session runs the phase-32 state-5 code-review (`superpowers:requesting-code-review`) — do NOT run it in this session.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 5 + `SKILL_ROUTING.md`: phase `32` is `in-progress` with `SPEC.md` + `PLAN.md` + `PROGRESS.md` present, the §7.5 gate (a)-(e) GREEN on the authoritative Linux CI (run `27941931062`), and `REVIEW.md` absent -> the next session runs **`superpowers:requesting-code-review`** — a fresh code-review of the phase-32 implementation (commits `7917c8a`…`ecb62d3`) against `SPEC.md`/`PLAN.md`/`BEHAVIOR_CONTRACT.md`, authoring `REVIEW.md` (§7.5 gate (f)). Then state-6 close-out (ROADMAP row `32` → `done` + STATE rollover). Do NOT chain past state-5 (§5.1). **NOTE for the reviewer:** the carry-forward Minors from the 8 per-task two-stage reviews are catalogued in `PROGRESS.md` (e.g. Task-1 C1/C2/C3 `command_operator.rs` polish); fold/dispose per the REVIEW.md.

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-32 state-4 verification COMPLETE — STATE → state-5-next (THIS commit):** the state-4 §7.5 phase-done gate (`BOOTSTRAP_PROMPT.md` §5 state 4 → state-5). Pushed the 9 committed-but-unpushed state-3 commits (`783d29f..ecb62d3`) → AUTHORITATIVE Linux CI run **`27941931062` → success** (gate (a)-(e) all green: fixture 0040 + all `0001`-`0039` incl. byte-identical `0012` differential; h2spec ≥95%; 4 fuzz targets 0 crashes incl. the new `accesslog_format_parse`; fmt/clippy/build/test/deny clean). Local `0014` differential RED root-caused as a host-only bridge-IP allow-list gap (`192.168.65.2` ∉ {`192.168.65.254`,`172.17.0.1`}), NOT a phase-32 regression — green on the authoritative CI; no source change. Verification record authored into `PROGRESS.md`. THIS docs-only commit advances STATE `32` state-3-complete/state-4-next → state-4-complete / state-5-next (the phase-32 state-3 top-section blocks demoted to `_Historical_` + RELOCATED to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9, leaving the breadcrumb). `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0079** (count 80; next ADR-0080). ADR-0014 in force; ADR-0028 open. Per §5.1 the NEXT session runs the phase-32 state-5 code-review (`superpowers:requesting-code-review`).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-22 (phase-32 **state-4 verification COMPLETE / state-5-next** — ran `superpowers:verification-before-completion`; the §7.5 phase-done gate. Pushed the committed-but-unpushed state-3 commits [`783d29f..ecb62d3`] → AUTHORITATIVE Linux CI run `27941931062` → success: gate (a)-(e) all green [fixture 0040 + all `0001`-`0039` incl. byte-identical `0012`; h2spec ≥95%; 4 fuzz targets 0 crashes incl. new `accesslog_format_parse`; fmt/clippy/build/test/deny clean]. Local `0014` differential RED root-caused [`superpowers:systematic-debugging`] as a host-only Docker-bridge-IP allow-list gap [`192.168.65.2` ∉ {`192.168.65.254`,`172.17.0.1`}], NOT a phase-32 regression — green on the authoritative CI; no source change. Verification record authored into `PROGRESS.md`. Advanced STATE `32` state-3-complete/state-4-next → state-4-complete / state-5-next [the phase-32 state-3 top-section blocks relocated to STATE_HISTORY.md per ADR-0035, leaving the breadcrumb]. `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0079** [count 80; ADR-0080 reserved-but-UNFIRED]. ADR-0014 in force; ADR-0028 open. The NEXT session runs the phase-32 state-5 code-review [`superpowers:requesting-code-review`].)

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

### Phase-32 carry-forwards (open Minors re-stated at the phase-32 brainstorm; NOT consumed by phase 32 — Observability does not touch their surfaces)

> These open Minors (relocated to STATE_HISTORY.md at the phase-31 close in the `### Phase-31 carry-forwards` block) are re-stated here so they stay visible. Phase 32 is the access-log command-operator formatter — it touches NONE of their surfaces (the LB hash-sweep differential driver / the config parser / cdn_loop), so none is consumed this phase; each remains live for the future phase that touches its surface.

- **empty-`metadata_match`→fallback doc-comment** — an optional one-line clarifying comment at `crates/envoy-cluster/src/subset.rs` (subset-LB fallback path). Fold when the subset-LB / LB family is next touched.
- **M29-1 / M29-2 + M30-1** — the shared `Http1HashSweep` differential driver's RING_HASH-worded `bail!` diagnostics/comments (cosmetic, failure-output-only) + the route-select driver's duplicated `extract_marker` (`tests/differential/src/lib.rs`). Fold WHEN the hash-sweep / route-select differential driver is next touched.
- **M30-2** — `Cluster.lb_policy` has NO serde default. Weigh `#[serde(default)]` ROUND_ROBIN in a future config-hardening phase.
- **phase-31 cosmetic Minors M-2 / M-3** — M-2 the `count_cdn_id` empty-needle doc note; M-3 the cdn_loop cluster (`retain_mut` micro-clone / encode doc anchor / `split_on_comma` wrapper / test-helper prose). Doc/cosmetic; fold on a future `cdn_loop` touch.

### HTTP-filters-family carry-forwards (from the `25.2` REVIEW.md - NOT yet consumed; weigh whenever the HTTP-filters family is re-entered)

> These were never obligations on the xDS phase 26; they remain live for whenever an HTTP-filters-family phase resumes.

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _(2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase._
