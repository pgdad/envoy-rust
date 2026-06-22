# Phase 32 — Access-log command-operator formatter — REVIEW

> **Lifecycle state 5 (code-review output).** Authored by `superpowers:requesting-code-review`
> (the §7.5 phase-done gate (f) artifact). This is a FRESH whole-phase review, independent of
> the 8 per-task two-stage reviews already recorded in `PROGRESS.md`. Read top-to-bottom with
> zero prior context (D-3.4).
>
> **Reviewed range:** `783d29f..ecb62d3` (the phase-32 state-3 implementation: 8 task commits
> `7917c8a`…`cb7a191` + the state-3-close STATE commit `ecb62d3`). Base `783d29f` = the
> phase-32 state-2 PLAN-write. Diff: **+1976 / −99** across 38 files.
> **Requirements baseline:** `SPEC.md` (scope locked by **ADR-0078**), `PLAN.md` (§A §6.2-LOCKED
> facts 1–7 + the §B operator matrix, locked by **ADR-0079**), and the
> `BEHAVIOR_CONTRACT.md` "Access log field mapping" extension.

---

## §0 — Verdict

**APPROVE-WITH-MINORS.** 0 Critical / 0 Important / 6 Minor (all carry-forward — none blocks).

The phase faithfully implements the empirically-locked §A facts 1–7 and the §B operator support
matrix. The production engine is panic-safe and fuzz-clean; the differential comparator is true
byte-equality with a hard line-count bail (no silent-pass path); fixture 0040 is deterministic-only;
the new `accesslog_format_parse` fuzz target is fully `ci.yml`-wired (all three edits); the
BEHAVIOR_CONTRACT §B matrix matches the code allow-list exactly. No scope creep beyond ADR-0078
§2.1; `#![forbid(unsafe_code)]` holds in every touched crate root. The §7.5 gate (a)–(e) is GREEN
on the authoritative Linux CI run `27941931062` @ `ecb62d3` (recorded in `PROGRESS.md`). This is the
**sixteenth consecutive clean state-5** (after 17–31).

Per `BOOTSTRAP_PROMPT.md` §5.2 (BLOCKING-only re-entry to state-3): there is no blocking defect, so
the phase advances to the state-6 close-out. The 6 Minors are cosmetic / correct-but-loose polish
catalogued below and carried forward (folding any of them would mutate source/fixtures and require a
fresh state-4 differential re-run — out of proportion to cosmetics; §5.2 reserves re-entry for
blocking defects).

## §1 — Review method

Two independent fresh-eyes `superpowers:code-reviewer` subagents (zero shared context, crafted
self-contained briefs) covered complementary facets of the full phase diff and converged:

- **Reviewer A — production code path:** the engine (`command_operator.rs` parser + evaluator),
  default re-expression (`default_format.rs`), `FileSink` verbatim-emit refactor (`file_sink.rs`),
  the config field + boot-fatal validator (`bootstrap.rs`/`lib.rs`), and the H1/H2 config→sink
  wiring (`hcm.rs`/`error.rs`). Reviewer A empirically probed the parser/evaluator (e.g. `café:4`→
  `caf` no panic; a 24-digit `:N`→`BadTruncate` not overflow; lone `%`→`UnterminatedOperator`).
  → **Ready to merge: Yes.** 0 Critical / 0 Important.
- **Reviewer B — test / differential-harness / fixture / fuzz / docs surface:** the comparator
  (`access_log.rs`), the `Http1AccessLogByteExact` driver (`lib.rs`), fixture 0040, the fuzz target
  + corpus + `ci.yml` wiring, the `parse_bootstrap` seed, and the BEHAVIOR_CONTRACT extension.
  → **Ready to merge: Yes.** 0 Critical / 0 Important.

## §2 — Strengths (verified, not asserted)

- **Spec fidelity, byte-for-byte.** Every §A fact was independently confirmed against the code:
  - **Fact 2** (absent → `-`, never empty): `%UPSTREAM_HOST%` no-upstream, `%RESPONSE_FLAGS%`
    clean-200, and every missing `%REQ/%RESP` Option all render a single `-`
    (`command_operator.rs` `render_op`/`resolve_req`/`resolve_resp`).
  - **Fact 4** (grammar): `:N` is a byte-count truncation applied to the *resolved* value (after
    `?`-alt fallback), rounded DOWN to a UTF-8 char boundary via `floor_char_boundary`; `%%`→`%`.
  - **Fact 5** (boot-fatal): unknown keyword, malformed/unterminated `%REQ(`, empty `%()%`, AND a
    stray/lone/trailing single `%` are ALL config-load errors. The lone-`%` case (the easy one to
    miss) is correctly rejected.
  - **Fact 7** (trailing newline): `FileSink::emit` no longer appends its own `\n`; `DEFAULT_FORMAT`
    carries the `\n`; `compiled_default_matches_legacy_concatenator` proves the default's net bytes
    are unchanged (= legacy concatenator + `\n`) → fixture 0012 byte-identical.
  - **§B allow-list**: `%REQ/%RESP(NAME)%` valid iff ≥1 branch (name or `?`-alt) is a record-backed
    field; a well-formed-but-unbacked name (`%REQ(X-CUSTOM)%`) errors. Header matching is
    case-insensitive ASCII.
- **Panic-safe, fuzz-correct.** Every parser slice is bounded by single-byte ASCII delimiters
  (`%`,`(`,`)`); truncation never splits mid-UTF-8; literal runs preserve non-ASCII bytes via `&str`
  slicing (no `as char` mojibake — the latent bug caught + regression-tested at Task-1 state-3
  review). The fuzz target is UTF-8-gated (parser takes `&str`), exercises `parse_format`, and has a
  meaningful 7-seed corpus (valid/simple/truncate+alt/`%%`/malformed/empty/multibyte).
- **No missed production sink site.** H2's `HCMConfig::wrap` composes the SAME
  `Arc<Http1HCMConfig>` built by `Http1HCMConfig::from_config` (the single production path through
  `compiled_log_format`); the H1 construction loop (`hcm.rs:206`) is the sole production sink builder.
  The PLAN's "verify §A H2 site" flag is correctly resolved: H2 inherits via `wrap` with zero new H2
  production code (only a test site changed). Confirmed independently by both reviewers.
- **Dependency direction safe.** `envoy-config` → `envoy-accesslog` is one-way (no cycle:
  `envoy-accesslog` has no `envoy-config` dep). The boot validator calls the real `parse_format`, so
  config-validity is the engine's own grammar — single source of truth.
- **Differential comparator cannot silently pass a divergence.** `assert_access_log_lines_byte_
  identical` is plain `!=` byte-equality (no trim/normalize/sort), preceded by a hard exact
  line-count check that `bail!`s on mismatch (catches both a missing AND a surplus line). A
  `wait_file_lines` timeout only `warn!`s — the subsequent exact `!=` line-count gate is the real
  failure path.
- **Fixture 0040 is genuinely deterministic.** The `log_format` (identical in `envoy.yaml` and
  `envoy-rust.yaml`) uses ONLY deterministic operators; NO `%START_TIME%`/`%DURATION%`/
  `%REQ(X-REQUEST-ID)%`/`%RESP(...)%` leaks in. `%UPSTREAM_HOST%`→`-` via `direct_response`
  structurally avoids the per-side `{{BACKEND_IP}}` hazard. Two probes vary headers (bare GET vs
  GET+user-agent+x-forwarded-for), exercising both the absent-`-` and present-value paths.
- **`ci.yml` fuzz wiring complete (the project's recurring hazard).** All THREE required edits are
  present and correct: (1) `fuzz:` job name, (2) rust-cache `workspaces:` += `crates/envoy-accesslog/
  fuzz -> target`, (3) the `cargo +nightly fuzz run accesslog_format_parse -- -max_total_time=30`
  step with the correct `working-directory`. §7.5 gate (d) is genuinely met, confirmed by the
  CI run's `accesslog_format_parse` step succeeding.
- **BEHAVIOR_CONTRACT is accurate + self-contained + corrects (not duplicates) stale prose.** The
  documented §B matrix matches the code allow-list byte-for-byte; the stale 06.2 "format
  customization OUT of scope" paragraph is marked superseded and rewritten to carve out the modern
  `log_format.text_format_source.inline_string` path while keeping json_format / deprecated
  text_format / top-level format out of scope.
- **Tests pin real behavior.** Engine error-path tests assert the specific `FormatParseError`
  variant via `matches!` (not just `.is_err()`); the config tests assert
  `ConfigError::InvalidAccessLogFormat` and a `deny_unknown_fields` (`inline_strings` typo →
  `ConfigError::Yaml`) case; multibyte truncation, alt+`:N`, and non-ASCII literals are covered.

## §3 — Issues

### §3.1 Critical (Must Fix)
**None.** Both reviewers actively hunted for a byte-divergence vs Envoy, a parser panic, a missed
boot-fatal case, and a scope violation; none was found.

### §3.2 Important (Should Fix)
**None.**

### §3.3 Minor (Nice to Have — all CARRY-FORWARD; dispositions below)

The three carry-forward Minors flagged from the Task-1 state-3 review (`PROGRESS.md` C1/C2/C3) were
independently re-assessed by Reviewer A — **each confirmed correct-but-cosmetic, hides no bug**:

- **M32-1 (C1) — stringly-typed `side: &'static str`.** `parse_header_op` dispatches on
  `match side { "REQ" => …, _ => … }` (`command_operator.rs`); the `_` arm conflates RESP with
  "anything else", so a future third caller with a typo would mis-dispatch to RESP. A 2-variant
  `enum Side { Req, Resp }` would make allow-list selection + `Op` construction total. Correct today
  (two literal call sites). **Disposition: carry-forward** — fold whenever `command_operator.rs` is
  next touched.
- **M32-2 (C2) — `%REQ(:path?)%` yields `alt: Some("")`; `:0` truncate accepted.** Empty alt
  resolves to `None`→`-` (harmless — the primary is always backed); `:0` renders `""`. Neither
  diverges from Envoy nor panics (within the fuzzer's never-panic contract). A stricter parser could
  reject an empty alt token. **Disposition: carry-forward** — cosmetic.
- **M32-3 (C3) — `MalformedArgument(String,String)` positional tuple + partial `UnsupportedHeader`
  reporting.** Named fields would match the `UnsupportedHeader { .. }` struct-variant style; and
  `UnsupportedHeader` reports only `name` even when it is the *alt* that is unbacked (the
  `name_backed || alt_backed` disposition is still correct — only the boot-message string is
  slightly imprecise). **Disposition: carry-forward** — cosmetic diagnostics.

Three further Minors surfaced fresh in this whole-phase review:

- **M32-4 — in-crate default-format equivalence rests on a single record.** In `default_format.rs`
  the four pre-existing tests now assert against the `#[cfg(test)]`-only `legacy_format`, and only
  `compiled_default_matches_legacy_concatenator` bridges legacy↔engine — using a single
  `make_baseline_record()`. So the engine's reproduction of the 5xx-flags and utf8-user-agent cases
  is not *directly* asserted in-crate. **Mitigation:** fixture 0012 is the real cross-proxy
  default-format witness (a regression cannot ship green). **Disposition: carry-forward** — optional
  fix: loop the equivalence assertion over the records the other four tests build.
- **M32-5 — vestigial empty `inputs/payload.bin`.** Fixture 0040's `inputs/payload.bin` is a 0-byte
  tracked file the `http1_access_log_byte_exact` driver never reads (probe bodies are unset). It
  matches the existing fixture-scaffolding convention (cf. 0007/0012, noted at the Task-6 state-3
  review #5), so it is deliberate, not a leak. **Disposition: carry-forward** — optional `git rm` on
  a future fixture-hygiene pass; not removed now to avoid a state-4 differential re-run for a 0-byte
  cosmetic.
- **M32-6 — `render` pre-allocates a fixed `String::with_capacity(256)` per call.** Negligible
  per-request over-allocation for short custom formats; mirrors the legacy emitter. **Disposition:
  carry-forward** — micro-optimization only.

> _Reviewer A separately confirmed the Task-5 re-parse-on-construction question is **not a defect**:
> `compiled_log_format` re-parses the boot-validated string once at HCM construction (not per
> request) via a real `.map_err` non-panic path — sanctioned redundancy with a defensive error
> surface, exactly as the PLAN specified._

## §4 — Requirements traceability (SPEC §2.1 / PLAN §A·§B)

| Requirement | Status |
|---|---|
| §2.1.1 command-operator parser (correctness gate) | ✅ `command_operator.rs` `parse_format` |
| §2.1.2 compiled-format evaluator + default re-expression byte-identical | ✅ `render` + `DEFAULT_FORMAT`; 0012 witness |
| §2.1.3 `FileAccessLog.log_format` modern wire path (Fact 1) | ✅ `SubstitutionFormatString.text_format_source.inline_string` |
| §2.1.4 boot-fatal config validation (Fact 5) | ✅ `ConfigError::InvalidAccessLogFormat` via real parser |
| §2.1.5 curated DETERMINISTIC operator set (§B matrix) | ✅ exact allow-list match |
| §2.1.6 allow-listed non-deterministic operators (`%START_TIME%`/`%DURATION%`) | ✅ engine-general, backstop-only |
| §2.1.7 fixture 0040 + 0012-unchanged + backstop + fuzz + parse_bootstrap seed + BEHAVIOR_CONTRACT | ✅ all present |
| §2.2 DEFERRED non-goals NOT implemented (json_format, %DYNAMIC_METADATA%, address ops, …) | ✅ no scope creep |
| §7.5 (a)–(e) gate | ✅ GREEN on authoritative CI `27941931062` @ `ecb62d3` |
| §7.5 (f) REVIEW.md | ✅ this document |
| `#![forbid(unsafe_code)]` (D-3.8) | ✅ all touched crate roots + fuzz target |

## §5 — Assessment

**Ready to merge: Yes (APPROVE-WITH-MINORS).** The production engine is behavior-faithful to the
empirically-locked §A/§B ground truth, panic-safe, fuzz-clean, and free of scope creep; the
differential harness, fixture 0040, fuzz wiring, and BEHAVIOR_CONTRACT are sound and accurate. The
six Minors are cosmetic / correct-but-loose and carry forward. No ADR added (review only; ledger head
unchanged at ADR-0079, ADR-0080 reserved-but-UNFIRED). Proceed to the state-6 close-out (flip ROADMAP
row `32` → `done` + STATE rollover).

---

_Phase-32 state-5 code-review COMPLETE. The next session runs the state-6 close-out per §5.1
(one state per session)._
