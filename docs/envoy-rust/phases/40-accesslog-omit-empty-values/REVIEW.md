# Phase 40 — `40-accesslog-omit-empty-values` — REVIEW

> **Lifecycle state 5 (code-review output).** Routed via `superpowers:requesting-code-review`;
> performed by a fresh `superpowers:code-reviewer` subagent with precisely-crafted context (the
> implementation diff + SPEC + PLAN + ADR-0096 §A–§E — NOT session history). Reviews the phase-40
> implementation (commit range `86971ce`..`0114afa`, diff `cccaaaf..0114afa`).

## Verdict: **APPROVE** — 0 Critical / 0 Important / 1 new Minor (M40-1; non-blocking, carry-forward)

The implementation is a surgical, minimal-surface change exactly as ADR-0096 §B/Decision prescribed: one
`bool` config field + one `omit_empty: bool` thread into the existing `render_value_segments`, NO
key-filter logic, NO new error/crate/fuzz-target. The reviewer built + ran `cargo test -p envoy-accesslog
-p envoy-config -p envoy-http1` (77 / 532 / 134 pass, 0 fail) + `cargo fmt --check` clean.

## Verification against ADR-0096 §A–§E (all UPHELD)
- **§A (no key-drop)** — no key-filtering logic added; `CompiledJsonFormat::render`/`render_into` emit
  every key. Proven by `omit_empty_default_off_round_trip_byte_unchanged` + the 0048 line (5 keys emit).
- **§B (swap `-`→`""` in multi-segment, BOTH formats)** — `command_operator.rs:494`
  `let empty_or_dash = if omit_empty { "" } else { "-" };` + all FOUR absent sites swap (`UpstreamHost`
  :511, `DynamicMetadata` :518, `Req` :528, `Resp` :540). Text via `CompiledFormat::render`; json via
  `encode_json_value`→`render_value_segments` (`json_format.rs:206`). Both carry the flag.
- **§C (`encode_single_op` UNCHANGED)** — the highest-risk point, UPHELD: `json_format.rs:200` calls
  `encode_single_op(out, op, r)` with NO `omit_empty` param; the single-`[Segment::Op]` branch is
  untouched; the swap is confined to the multi-segment `else` branch. Proven by
  `omit_empty_leaves_single_op_null_unchanged` + `compiled_log_format_threads_omit_empty_values_json`.
- **§D (recursive)** — `render_into` threads `omit_empty` to every `Array` item + `Object` value. Proven
  by `omit_empty_applies_recursively_single_op_null_at_depth` (`{"arr":["a=",null],"nested":{"mixed":"v=",
  "single":null}}`).
- **§E (all-single-absent → `null`; plain bool; NO new `ConfigError`)** — `omit_empty_values: bool`
  `#[serde(default)]` (`bootstrap.rs:715`); `deny_unknown_fields` unchanged; no `ConfigError` variant.

## Doctrine checks (all pass)
- `#![forbid(unsafe_code)]` holds. **No new crate/dependency** (`Cargo.toml`/`Cargo.lock` zero diff).
  **No new fuzz target** (the `omit_empty_values.yaml` seed lands in the EXISTING `parse_bootstrap` corpus;
  `.gitignore` un-ignore line present; seed git-tracked — both the new-fuzz-target and corpus-gitignore
  traps avoided). **No new `ConfigError` variant.**
- **Default-off byte-preservation:** `omit_empty` defaults `false` at every construction site
  (`CompiledFormat::new`/`from_inline`/`Default`, `CompiledJsonFormat::from_map`, all H1/H2 test literals).
- **H2 wiring:** H2 reuses `envoy_http1::HCMConfig` + H1's `compiled_log_format`, so the H1 wiring
  (`hcm.rs:1268,1282`) covers H2 too; the two `omit_empty_values: false` sites in `envoy-http2/src/hcm.rs`
  are test-struct construction. Correct, not a gap.
- **TDD:** each of the 6 commits pairs tests with implementation.

## Findings

**Critical:** none.

**Important:** none.

**Minor (new carry-forward; NONE blocks):**
- **M40-1** (doc-accuracy) — `tests/fixtures/0048-accesslog-omit-empty/README.md` (the "Flag-off control"
  section) + `expectations.yaml` imply fixture `0047-accesslog-json-nested` byte-proves the flag-OFF `-`
  sentinel "for the same recursive-json shape" cross-proxy. But `0047`'s map has NO multi-segment leaf with
  an absent operator (its only absent op `blist[2]: "%UPSTREAM_HOST%"` is a SINGLE op → `null`, the §C
  path; its only multi-segment leaf `mtop: "code-%RESPONSE_CODE%"` has a PRESENT op). So `0047` proves
  default-off byte-preservation generally but does NOT differentially prove the `-`-sentinel-in-multi-
  segment (`up=-`) behavior cross-proxy — only the flag-ON `""` swap (0048) is fixture-proven against live
  Envoy; the flag-off `up=-`/`x=-` control is documented + covered by the in-process backstop
  (`omit_empty_swaps_dash_in_multi_segment_json_leaf`, omit=false arm), not run as a differential.
  **Correctness is NOT at risk** (the backstop covers it). Doc-accuracy only; fold by softening the README
  wording when the next phase touches the access-log fixtures.

## Strengths
- The §C carve-out is implemented as an OMISSION (the single-op branch simply never receives the flag)
  rather than a conditional — the most robust way to guarantee "single absent op stays `null`".
- The tuple-struct → named-field-struct migration (`CompiledFormat`/`CompiledJsonFormat`) with a
  `new`/`with_omit_empty` builder pair is clean; ~15 in-crate call sites keep working via `::new`; the
  builder default-off is the byte-preservation guard.
- Test coverage hits every axis: §B text + json swap, §C carve-out (both crates), §D recursion, default-off
  byte-identity, config round-trip/default. Fixture 0048 simultaneously proves §A (5 keys), §B (`up=`/`x=`),
  and §C (`single_up:null`).
- ADR-0096 §A–§E faithfully transcribed into `BEHAVIOR_CONTRACT.md` with the authoritative live-captured
  lines (both flag-on and flag-off control).

---

_Reviewed at state-5. **APPROVE** (0 Critical / 0 Important / 1 new Minor M40-1 carried forward,
non-blocking). The §7.5 (a)-(e) gate was GREEN at state-4 (authoritative CI `28297297375` @ `c4f95b1`
`completed/success`). With (f) `REVIEW.md` APPROVE, the full §7.5 (a)-(f) gate is COMPLETE → the next
session is the state-6 phase-close (flip ROADMAP row `40` → `done`, advance STATE to awaiting-next-planning)._
