# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `34` — `34-header-to-metadata` (**Observability / HTTP-filters family: request-driven metadata** — the `envoy.filters.http.header_to_metadata` HTTP filter [request-header → dynamic-metadata extraction] + fixture `0042`, REUSING the phase-33 dynamic-metadata store + `%DYNAMIC_METADATA%` operator + H1/H2 threading UNCHANGED). Lifecycle **state-4 VERIFICATION COMPLETE / state-5-next** (`SPEC.md` + `PLAN.md` + `PROGRESS.md` present; `REVIEW.md` absent). The next session runs `superpowers:requesting-code-review` (the state-5 code-review → `REVIEW.md`) — see `## Next expected skill`.
**slug:** `34-header-to-metadata`
**directory:** `docs/envoy-rust/phases/34-header-to-metadata/` (`SPEC.md` + `PLAN.md` + `PROGRESS.md` present; `REVIEW.md` absent)

**status:** **PHASE 34 (`34-header-to-metadata`) state-4 VERIFICATION COMPLETE — state-5 code-review next.** This session (`superpowers:verification-before-completion`) ran the FULL §7.5 acceptance gate and quoted every command output into `PROGRESS.md` (the new `## §7.5 verification gate (state-4)` section). **§7.5 (a)–(e) are GREEN:** (a) fixture `0042` differential green LOCALLY (access-log file-scrape, locally authoritative; `cargo build -p envoy-bin` then `cargo test -p differential --test header_to_metadata -- --include-ignored` → **1 passed, 10.76s**; both present+missing probes byte-identical cross-proxy); (b)+(c) all `0001`–`0041` differential + h2spec ≥95% green on the AUTHORITATIVE Linux **CI run `28062068794` @ `7c19803` = `completed/success`** (its `build + test + lint` job ran the full `cargo test --workspace` incl. the complete Docker differential harness against `envoyproxy/envoy:v1.33.0`); (d) the EXISTING `parse_bootstrap` (+ `accesslog_format_parse`) fuzz targets clean on the CI `fuzz` job WITH the new `hcm_header_to_metadata.yaml` seed — **NO new fuzz target** (`ci.yml` UNCHANGED across the phase `bf42699^..7c19803`, verified; the seed is git-tracked); (e) fresh-local `cargo build --workspace --all-targets` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check` + `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`) all clean, + `cargo test --workspace --exclude differential` green (envoy-config 495 / envoy-filter 185 / envoy-http1 129 / envoy-http2 73+1 / envoy-cluster 160 / envoy-listener 36 / all others 0-fail). The lone local non-pass is the documented pre-existing H2 full-suite-contention RACE `envoy-http2 send_request_maps_h2_handshake_failure_to_typed_error` (unrelated to phase 34) — re-run in isolation this session → **PASS (1 passed)**; CI's full `cargo test --workspace` passed it too. **NO cross-crate clippy/fmt drift surfaced** (CI was already green per memory `envoy-rust-state4-ci-first-execution`). CI is authoritative for the host-sensitive differentials (admin-dump / backend-routing false-RED LOCALLY; h2spec §3.5/2 false-PASS-then-gate-RED locally — memories `differential-host-bridge-ip-192-168-65-2` / `host-docker-desktop-virtiofs-no-inotify` / `h2spec-3-5-2-preface-host-sensitive`). `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target. **DECISIONS.md ledger head: ADR-0084** (count 84; next-available **ADR-0085**). ADR-0014 in force; ADR-0028 open. Phase 33 CLOSED (CI-GREEN run `28021495767`); its open Minors **M33-1**/**M33-2** carry forward (phase 34 reused the threading UNCHANGED → NOT consumed). The §7.5 (f) gate — `REVIEW.md` approved — is the state-5 code-review (`superpowers:requesting-code-review`), the NEXT session (§5.1 one state per session) — see `## Next expected skill`.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 5 + `SKILL_ROUTING.md`: phase `34` is `in-progress` with `SPEC.md` + `PLAN.md` + `PROGRESS.md` present and `REVIEW.md` absent (the §7.5 gate (a)–(e) verified GREEN this state-4 session) -> the next session runs **`superpowers:requesting-code-review`** (the state-5 code-review → `REVIEW.md`). Review the phase-34 diff (the 7 task commits `bf42699`..`48f8086`) against `PLAN.md` + the §A locked facts (ADR-0084) + doctrine; if issues → back to state-3 (§5.2) until `REVIEW.md` approved; if approved → state-6 closes the phase (ROADMAP row `34` → `done`). **The state-5 code-review should note the cosmetic Minors the state-3 quality reviews surfaced-and-left** (NONE blocks the phase): (T5) the redundant function-scope `use tempfile::tempdir` + the inlined H1 pipeline-construction (no extracted helper); (T6) the README §A5-skipped note + `generate_request_id` "load-bearing" wording; (T7) the `A-missing`-vs-`A5` heading-numbering cosmetic. **State-4 is DONE (do NOT re-verify):** §7.5 (a)–(e) GREEN — fixture `0042` green locally (10.76s); CI run `28062068794` @ `7c19803` `completed/success` (full `cargo test --workspace` incl. the Docker differential + the `fuzz` job with the new seed, NO new target, `ci.yml` unchanged); fresh-local build/clippy/fmt/deny clean + `test --workspace --exclude differential` green; the H2 race re-confirmed a race (passes in isolation). Evidence quoted in `PROGRESS.md` `## §7.5 verification gate (state-4)`.

**Open carry-forward Minors (fold into the phase that next touches each surface; NONE blocks phase 34):**
- **M33-1** — unnecessary `.clone()` at `crates/envoy-http1/src/hcm.rs:1211` (the H1 record-build `dynamic_metadata` local is single-use/last-use → could move, as the H2 path does). **Phase 34 touches the H1 HCM record-build path only if it adds plumbing there — but it REUSES the existing threading UNCHANGED, so M33-1 is NOT necessarily folded this phase** (fold opportunistically if the H1 hcm.rs is edited). M33-2 (doc-pointer line drift in `command_operator.rs`/`record.rs`) likewise — doc-only.
- the empty-`metadata_match`→fallback doc-comment (`crates/envoy-cluster/src/subset.rs`); M29-1/M29-2 + M30-1; M30-2 (`Cluster.lb_policy` serde-default); the phase-31 M-2/M-3; the HTTP-filters-family (1)-(4) buffer carry-forwards. (The 6 M32 carry-forwards are CONSUMED.)

**Phase 33 is fully closed + CI-GREEN on the authoritative Linux run `28021495767` @ `72fe40d`. Phase 34's SPEC reuse claims (the store + `%DYNAMIC_METADATA%` operator + the FILTER-AGNOSTIC H1/H2 threading) were verified against the live code by the spec-reviewer — do NOT re-derive.**

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-34 state-4 verification — run the §7.5 gate, quote evidence into PROGRESS.md, advance STATE (THIS docs commit):** the state-4 verification gate (`BOOTSTRAP_PROMPT.md` §5 state 4; `superpowers:verification-before-completion`). Ran the FULL §7.5 gate FRESH against `HEAD = 7c19803` (clean tree) and quoted every command output into a new `## §7.5 verification gate (state-4)` section in `PROGRESS.md`. **§7.5 (a)–(e) GREEN:** (a) fixture `0042` differential → 1 passed (10.76s, locally authoritative); (b)+(c) all `0001`–`0041` + h2spec ≥95% green on the AUTHORITATIVE Linux **CI run `28062068794` @ `7c19803` = `completed/success`** (`build + test + lint` ran the full `cargo test --workspace` incl. the Docker differential; `fuzz` job ran `parse_bootstrap` [+ the new seed] + `accesslog_format_parse` clean); (d) NO new fuzz target (`ci.yml` UNCHANGED, seed git-tracked); (e) fresh-local build/clippy/fmt/deny clean + `test --workspace --exclude differential` green (config 495 / filter 185 / http1 129 / http2 73+1 / cluster 160 / …); the lone non-pass is the documented H2 full-suite-contention RACE — re-run in isolation → PASS. NO clippy/fmt drift surfaced. **THIS docs commit** (`PROGRESS.md` §7.5 append + STATE + STATE_HISTORY relocation per ADR-0035). `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO code change this session. **DECISIONS.md ledger head: ADR-0084** (count 84; next-available **ADR-0085**). ADR-0014 in force; ADR-0028 open. PUSHED + CI re-confirmed (docs-only). The state-5 code-review is the NEXT session (`superpowers:requesting-code-review`).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-23 (phase-34 **state-4 VERIFICATION COMPLETE / state-5-next** — ran the FULL §7.5 acceptance gate via `superpowers:verification-before-completion` and quoted every command output into `PROGRESS.md` [the new `## §7.5 verification gate (state-4)` section]. **§7.5 (a)–(e) GREEN:** (a) fixture `0042` differential 1 passed [10.76s, locally authoritative]; (b)+(c) `0001`–`0041` + h2spec ≥95% green on the AUTHORITATIVE Linux CI run `28062068794` @ `7c19803` [`completed/success`; full `cargo test --workspace` incl. the Docker differential + the `fuzz` job with the new seed]; (d) NO new fuzz target [`ci.yml` unchanged `bf42699^..7c19803`; seed git-tracked]; (e) fresh-local `build --workspace --all-targets` / `clippy --workspace -D warnings` / `fmt --all --check` / `deny check` clean + `test --workspace --exclude differential` green [config 495 / filter 185 / http1 129 / http2 73+1 / cluster 160]; the documented H2 full-suite-contention RACE re-ran in isolation → PASS. NO clippy/fmt drift. THIS docs commit [`PROGRESS.md` §7.5 append + STATE + STATE_HISTORY relocation per ADR-0035]; NO code change. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target. **DECISIONS.md ledger head: ADR-0084** [count 84; next-available ADR-0085]. ADR-0014 in force; ADR-0028 open. The state-5 code-review is the NEXT session [`superpowers:requesting-code-review`].)

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
