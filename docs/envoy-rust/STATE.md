# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase


**id:** **PHASE 39 (`39-accesslog-json-nested-values`) — STATE-3 IMPLEMENTATION COMPLETE (7 TDD tasks landed; build/clippy/fmt/test green + fixture 0047 byte-exact differential GREEN); STATE-4 VERIFICATION-GATE NEXT.** ROADMAP rows `00`-`38` are `done`; `39` is `in-progress`. The next session is the state-4 verification gate (`superpowers:verification-before-completion`) — see `## Next expected skill`.
**slug:** `39-accesslog-json-nested-values`
**directory:** `docs/envoy-rust/phases/39-accesslog-json-nested-values/` (contains `SPEC.md` + `PLAN.md` + `PROGRESS.md`)

**status:** **PHASE 39 — STATE-3 IMPLEMENTATION COMPLETE; STATE-4 VERIFICATION-GATE NEXT.** This session (the §5 state-3 implementation, routed via `superpowers:subagent-driven-development`, TDD per task) landed all 7 PLAN tasks (commit range `4dbde13`..`f2e4767`): T1 recursive `JsonFormatValue` config enum (`#[serde(untagged)]` {Null,Bool,Format,Array,Object}) + T2 recursive per-leaf validator / T3 accesslog-side `JsonValueInput` mirror + recursive `CompiledJsonValue` compile / T4 recursive byte-exact `render` (reuses the phase-38 leaf helpers VERBATIM; the §H authoritative line; folds M38-3 [empty value-string `""`] + M38-4 [`:N`/`?ALT`/`%DURATION%`/control-char in the nested path]) / T5 HCM `JsonFormatValue`→`JsonValueInput` bridge / T6 fixture `0047-accesslog-json-nested` byte-exact differential / T7 nested `parse_bootstrap` seed (`json_format_nested.yaml`) + BEHAVIOR_CONTRACT recursive subsection + carry-forward bookkeeping. **Implemented exactly to the ADR-0094 §A-§H locked facts** (per-level key sorting `BTreeMap`; list order `Vec`; at-depth type inference via the verbatim phase-38 leaf encoder; `bool`/`null` native-typed leaves; NUMERIC literals boot-rejected [CF-39-1]; compact + ONE trailing `\n`; malformed-nested-op boot-fatal via EXISTING `InvalidAccessLogFormat`). **VERIFIED by the orchestrator** (not just the implementing subagent): `cargo fmt --check` OK; `cargo build --workspace --all-targets` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; touched-crate tests green (envoy-accesslog 72 / envoy-config 531 / envoy-http1 132, 0 failed); BOTH differentials GREEN in isolation vs live `envoyproxy/envoy:v1.33.0` — **fixture `0047` byte-exact** (the new nested line) + **fixture `0046` byte-identical** (the flat regression witness — the recursion preserves depth-1 output). `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `ConfigError` variant; NO `Cargo.toml`/`Cargo.lock` change. The `### Phase-39 state-3 implementation` Notes subsection (below) carries the detail. **DECISIONS.md ledger head: ADR-0094** (count 94; next-available **ADR-0095**, reserved-but-unfired). ADR-0014 in force; ADR-0028 open; ADR-0049 governs config-validity. **ROADMAP row `39` `in-progress`; rows `00`-`38` `done`.** The next session is the state-4 verification gate — see `## Next expected skill`.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases incl. the phase-37 state-1..5 sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 4 + `SKILL_ROUTING.md`: phase 39's implementation is complete + not-yet-formally-verified → the next session is the **state-4 verification gate** (`superpowers:verification-before-completion`). Run the FULL §7.5 (a)-(e) gate and quote ALL command outputs into `PROGRESS.md`: (a) fixture `0047` green + (b) all `0001`-`0046` green [incl. `0046` flat byte-identical — the recursion-refactor regression witness] + (c) h2spec ≥95% [unchanged — no HTTP/2 change] + (d) the `parse_bootstrap` + `accesslog_format_parse` fuzz targets clean for the short-budget CI run [with the new `json_format_nested.yaml` seed; NO new target] + (e) `cargo build --workspace --all-targets` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` / `cargo fmt --all -- --check` / `cargo test --workspace` / `cargo deny check` all clean. The state-3 orchestrator already smoke-verified build/clippy/fmt/touched-tests + the `0047`/`0046` differentials locally GREEN — state-4 re-runs the FULL gate (the whole differential suite + h2spec + deny + fuzz) as the authoritative pass and quotes the evidence. Do NOT start the state-5 code-review in the same session — §5.1 one state per session.

**The full differential suite + h2spec are CI-authoritative** (this Docker-Desktop host false-REDs some fixtures under full-workspace parallel load / via the bridge IP — run new/affected fixtures in isolation; the documented host false-REDs [`admin_config_dump_server_info` bridge-IP; `envoy-http2` h2-handshake host-flake] are NOT regressions). The state-3 commits are pushed; confirm CI green.

**Open carry-forward Minors (NONE blocks):**
- **CF-39-1** (ADR-0094 §D) — NUMERIC literal `json_format` leaves boot-rejected (protobuf-`double` formatting deferred); a future phase replicates it. `bool`/`null` literals ARE supported.
- **M38-1** — the resolve+truncate chain is already shared between `encode_single_op` and the text `render_op` (both call `command_operator::{resolve_req,resolve_resp,truncate_bytes}`); treated as folded-equivalent (no further extraction — cosmetic with regression risk). **M38-2** (`%DYNAMIC_METADATA%` single-op JSON quoting) stays live (not in the §6.2 recon). M38-3/M38-4 FOLDED at T4.
- **M37-2/M37-1 + M36-1/M36-2/M36-3 + M34-* + M33-* + the empty-`metadata_match` doc-comment + M29-1/M29-2 + M30-1/M30-2 + the phase-31 cosmetics + the HTTP-filters-family (1)-(4)** — fold into the phase that next touches each surface (nested `json_format` does not touch `rbac.rs`).

**Phase 39 is at state-3-complete (implementation landed + locally verified GREEN).** Phases 38/37/36/35/34 closed + CI-GREEN. The next session is the state-4 verification gate (§5.1 one state per session).

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases incl. the phase-37 state-1..5 sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-39 state-3 implementation — 7 TDD tasks landed (commit range `4dbde13`..`f2e4767`) + STATE advance, push + confirm CI green (THIS STATE-advance commit):** the §5 state-3 implementation (routed via `superpowers:subagent-driven-development`, TDD per task). Landed the recursive `json_format` encoder: T1 `JsonFormatValue` config enum + T2 recursive validator (`4dbde13`) / T3 `JsonValueInput` mirror + `CompiledJsonValue` compile (`d89a68b`) / T4 recursive byte-exact `render` + M38-3/M38-4 folds (`70be3be`) / T5 HCM bridge (`54319ea`) / T6 fixture `0047` differential (`f8d6ec6`) / T7 nested fuzz seed + BEHAVIOR_CONTRACT (`f2e4767`). Implemented to ADR-0094 §A-§H exactly. **Orchestrator-VERIFIED GREEN:** fmt/build/clippy clean; envoy-accesslog 72 / envoy-config 531 / envoy-http1 132 tests pass; fixture `0047` byte-exact + `0046` byte-identical differential GREEN in isolation vs live `envoyproxy/envoy:v1.33.0`. **THIS commit is the docs-only STATE advance** (STATE + STATE_HISTORY) on top of the 6 implementation commits. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `ConfigError` variant. **DECISIONS.md ledger head: ADR-0094** (count 94). ADR-0014 in force; ADR-0028 open. The next session is the state-4 verification gate (`superpowers:verification-before-completion`).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases incl. the phase-37 state-1..5 sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-27 (phase-39 **STATE-3 IMPLEMENTATION COMPLETE — STATE-4 VERIFICATION-GATE NEXT**. Landed all 7 PLAN TDD tasks [commit range `4dbde13`..`f2e4767`]: recursive `JsonFormatValue` config enum + recursive validator / `JsonValueInput` mirror + `CompiledJsonValue` compile / recursive byte-exact `render` [reuses phase-38 leaf helpers VERBATIM; §H line; folds M38-3/M38-4] / HCM bridge / fixture `0047` differential / nested fuzz seed + BEHAVIOR_CONTRACT. Implemented to ADR-0094 §A-§H. **Orchestrator-verified GREEN:** fmt/build/clippy clean; envoy-accesslog 72 / envoy-config 531 / envoy-http1 132 tests pass; fixture `0047` byte-exact + `0046` byte-identical differential GREEN vs live `envoyproxy/envoy:v1.33.0`. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `ConfigError` variant. NEW carry-forward CF-39-1 [numeric literal leaves boot-rejected]; M38-3/M38-4 FOLDED; M38-1 folded-equivalent; M38-2 stays live. **ROADMAP row `39` `in-progress`; rows `00`-`38` `done`.** **DECISIONS.md ledger head: ADR-0094** [count 94; next-available ADR-0095]. ADR-0014 in force; ADR-0028 open. The next session is the state-4 verification gate [`superpowers:verification-before-completion`].)

> Historical `## Last updated` notes — every superseded last-updated note (all closed phases incl. the phase-37 state-1..5 sub-state notes) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Notes

> Historical Notes subsections for fully-closed phases 00-07 (ADR-numbering notes, per-phase rollovers, ADR ledgers, and the earlier-phase-carryforward + phase-00-deferral snapshots) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. Phase 39 is at state-3-complete (implementation landed + locally verified GREEN); the next session is the state-4 verification gate (`superpowers:verification-before-completion`) — run the FULL §7.5 (a)-(e) gate + quote evidence into `PROGRESS.md`; do NOT also start the state-5 code-review that session.
- The reviewer's R2 disposition decision (option (a) retroactive split of 05.1 vs option (b) free-standing post-05.1 sub-phase) was settled at the 05.1 state-6 commit in favor of option (b); 05.4 is the chosen sibling sub-phase. Future-reviewers reading STATE.md should understand that 05.1 is structurally closed at the preamble landing; 05.4 is a SIBLING under parent-05, not a child of 05.1; and the execution order ran 05.1 → 05.4 → 05.2 → 05.3, with 05.3 the closing sub-phase that flips parent-05 to `done`.
### Phase-39 state-3 implementation (active phase)

- **Pick (ADR-0093):** phase 39 = nested / non-string `json_format` values — the phase-38 `json_format` encoder made RECURSIVE (nested objects + lists + `bool`/`null` leaves). §6.2 facts LOCKED by ADR-0094 (§A per-level sort / §B list order / §C at-depth inference / §D scalar-typed [numeric DEFERRED CF-39-1] / §E compact+`\n` / §F empty `{}`/`[]` / §G boot-fatal nested-op / §H fixture-0047 line).
- **Implementation (7 TDD tasks, commit range `4dbde13`..`f2e4767`):** T1 `JsonFormatValue` (`#[serde(untagged)]` {Null,Bool,Format,Array,Object}; numeric → boot-reject) + T2 recursive validator / T3 accesslog-side `JsonValueInput` mirror (NO `envoy-accesslog`→`envoy-config` dep) + recursive `CompiledJsonValue` / T4 recursive `render` (phase-38 leaf helpers reused VERBATIM; folds M38-3/M38-4) / T5 HCM `JsonFormatValue`→`JsonValueInput` bridge / T6 fixture `0047` byte-exact differential / T7 `json_format_nested.yaml` fuzz seed + BEHAVIOR_CONTRACT.
- **Orchestrator verification (GREEN):** fmt/build/clippy clean; envoy-accesslog 72 / envoy-config 531 / envoy-http1 132 tests pass; fixture `0047` byte-exact + `0046` byte-identical differential GREEN vs live `envoyproxy/envoy:v1.33.0`. NO new crate/dependency/fuzz-target/`ConfigError` variant; `#![forbid(unsafe_code)]` holds.
- **Carry-forwards:** CF-39-1 (NEW — numeric literal leaves boot-rejected); M38-3/M38-4 FOLDED at T4; M38-1 folded-equivalent (resolve chain already shared); M38-2 + M37-*/M36-*/M34-*/M33-* + older stay live.
- **Next:** state-4 verification gate (the FULL §7.5 (a)-(e) — whole differential suite + h2spec + deny + fuzz — quoted into `PROGRESS.md`).


> Historical Notes subsection for fully-closed phase 38 (the `### Phase-38 state-2 PLAN-write (active phase)` narrative — phase 38 used a single active-phase Notes subsection for its whole arc; the state-3/4/5 sub-state narratives were superseded in the four-top-section archive blocks per-session, so only this state-2 subsection remained in `## Notes` at close — relocated at the phase-38 state-6 close-out when ROADMAP row `38` flipped `done`) is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


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

> Historical Notes subsection for fully-closed phases 33+34 (the `### Phase-33 carry-forwards` block — the active-phase carry-forwards Notes that lived through phases 33 and 34: the now-CONSUMED 6 phase-32 REVIEW.md Minors M32-1…M32-6 [folded + landed at the phase-33 state-3] + the "Other still-live carry-forwards" list [the empty-`metadata_match`→fallback doc-comment + M29-1/M29-2 + M30-1 + M30-2 + the phase-31 cosmetic Minors M-2/M-3 + the HTTP-filters-family (1)-(4) — those still-live ones re-listed in `## Next expected skill` above alongside the new phase-34 Minors M34-1/M34-2/M34-3 + the phase-33 M33-1/M33-2]), relocated at the phase-34 state-6 close-out when row `34` flipped `done`, is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 35 (the `### Phase-35 state-1 brainstorm` / `### Phase-35 state-2 PLAN-write` / `### Phase-35 state-3 implementation` / `### Phase-35 state-4 verification` / `### Phase-35 state-5 code-review` narratives, relocated at the phase-35 state-6 close-out when ROADMAP row `35` flipped `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsection for fully-closed phase 36 (the `### Phase-36 state-5 code-review` narrative — phase 36 used a rename-in-place Notes discipline, so the state-1..4 sub-state narratives were superseded in place each session and only the four top-section blocks were relocated per-session — plus the now-CLOSED detailed M35-1 carry-forward bullet [CONSUMED by phase-36 F2], relocated at the phase-36 state-6 close-out when ROADMAP row `36` flipped `done`) is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsection for fully-closed phase 37 (the `### Phase-37 state-3 implementation` active-phase narrative, relocated at the phase-37 state-6 close-out when ROADMAP row `37` flipped `done`) is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

### HTTP-filters-family carry-forwards (from the `25.2` REVIEW.md - NOT yet consumed; weigh whenever the HTTP-filters family is re-entered)

> These were never obligations on the xDS phase 26; they remain live for whenever an HTTP-filters-family phase resumes.

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _(2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase._
