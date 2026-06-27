# Phase 38 — `38-accesslog-json-format` — Code Review (state-5)

> **Skill:** `superpowers:requesting-code-review` (state-5). A fresh `superpowers:code-reviewer`
> subagent was dispatched with CRAFTED context (the phase-38 implementation diff `5fbb966..44f177a`,
> `SPEC.md` [scope locked by ADR-0091], `PLAN.md` + `PROGRESS.md` [9 TDD tasks, empirical ground truth
> locked by ADR-0092 §A–§F], and the fidelity invariants: §A sorted-key BTreeMap order, §B single-operator
> type-inference, §D compact-separators/serde_json-default escaping/trailing `\n`, §E config-validity
> dispositions, §F byte-exact fixture line, the text-path byte-freeze, and the hard scope guards — no new
> crate/dependency, one new `ConfigError` variant, `#![forbid(unsafe_code)]`) — NOT session history (D-3.4
> context isolation). The orchestrator independently re-read the full diff, `SPEC.md`, and `PLAN.md` and
> verified every load-bearing claim.

## Verdict: **APPROVE** (0 Critical, 0 Important, 6 Minor — 4 new carry-forwards + 2 documented/pre-existing notes)

§7.5 phase-done gate (f) — `REVIEW.md` approved — is SATISFIED. Combined with the state-4-verified
(a)–(e) (PROGRESS.md `## State-4 Verification Gate (§7.5)`: build/clippy/fmt/deny exit 0; `cargo test
--workspace` GREEN modulo the documented host false-REDs [`admin_config_dump_server_info` bridge-IP
`192.168.65.2`; `envoy-http2` handshake host-flake; the parallel-load differential flakes — all PASS in
isolation]; both fuzz targets `parse_bootstrap` [new `json_format_logger.yaml` seed] + `accesslog_format_parse`
100000 runs no crash; differential `access_log_json_format` (`0046`) byte-exact GREEN vs live Envoy v1.33.0
+ all `0001`–`0045` GREEN with `0012`/`0040`/`0041`/`0042` byte-identical; h2spec unaffected — H2 codec
untouched; CI green on `5665c3e`), the FULL §7.5 phase-done gate is COMPLETE. Per §5.2 routing, APPROVE with
0 Critical / 0 Important → NO state-3 re-entry; advance to state-5-complete / state-6-next (the deterministic
ROADMAP row `38` → `done` flip is the SEPARATE state-6 session). The four new Minors (M38-1..M38-4) are
non-blocking carry-forwards.

---

## Review surface

**The phase-38 implementation diff: `5fbb966..44f177a`** (`5fbb966` = the state-2 PLAN-write commit, last
commit before any code; `44f177a` = the state-3 implementation tip — `5665c3e` (HEAD) is the state-4
doc-only marker [PROGRESS/STATE/STATE_HISTORY], OUT of review scope). The implementation landed as a
per-task commit series `56184a9`..`d88a865` (T1 oneof, T2 validator + `AmbiguousLogFormat`, T3 escaper, T4
typed value encoder, T5 `CompiledJsonFormat`, T6 `LogFormat`+`FileSink`, T7 HCM wiring, T8 fixture 0046,
T9 BEHAVIOR_CONTRACT + seed), then `c0b4a2c` (fmt) + `44f177a` (PROGRESS/STATE advance). The reviewed
source/test/fixture files (`git diff --stat 5fbb966..44f177a`, 19 files, +1183/-72):

- `crates/envoy-config/src/bootstrap.rs` — `SubstitutionFormatString` widened to the `{text_format_source |
  json_format}` oneof (`BTreeMap<String,String>` = §A sorted; `deny_unknown_fields` retained); the
  exactly-one-of validator in `validate_access_logs` (both-set/neither-set → `AmbiguousLogFormat`, empty map
  valid, malformed value → `InvalidAccessLogFormat`); the in-code parse/validity tests.
- `crates/envoy-config/src/lib.rs` — the one new `ConfigError::AmbiguousLogFormat { detail }` variant + re-exports.
- `crates/envoy-accesslog/src/json_format.rs` (NEW) — `CompiledJsonFormat`, `encode_json_value` /
  `encode_single_op` (the §B type-inference), `json_escape_into` (the §D escaper), the unit tests.
- `crates/envoy-accesslog/src/command_operator.rs` — `render_value_segments` extraction (text path delegates;
  M32-6 capacity carry-forward preserved); `pub(crate)` visibility for `resolve_req`/`resolve_resp`/`truncate_bytes`.
- `crates/envoy-accesslog/src/log_format.rs` (NEW) — `LogFormat { Text | Json }` enum + `render` + `From` impls.
- `crates/envoy-accesslog/src/file_sink.rs` — `format: LogFormat`; `new(path, impl Into<LogFormat>)`; `emit` unchanged.
- `crates/envoy-accesslog/src/lib.rs` — module wiring + re-exports (`CompiledJsonFormat`, `LogFormat`, `FormatParseError`).
- `crates/envoy-http1/src/hcm.rs` — `compiled_log_format` → `LogFormat` (Text|Json arm).
- `crates/envoy-http2/src/hcm.rs` — mechanical `json_format: None` compile-fixes in test ctors (no behavior change).
- `tests/fixtures/0046-accesslog-json-format/` (envoy.yaml + envoy-rust.yaml + expectations.yaml + README)
  + `tests/differential/tests/access_log_json_format.rs`.
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/json_format_logger.yaml` (+ the `.gitignore` `!`-un-ignore line).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` `json_format` subsection (§A–§F).

## Verification (orchestrator + reviewer; matches the state-4 gate evidence)

- `git diff 5fbb966..44f177a` read in full; the reviewer inspected the live source of each changed file
  (not just diff hunks) and pinned each ADR-0092 fact against the implementation.
- **Scope guards confirmed by diff** (`git diff --stat`/`grep`): NO `Cargo.toml`/`Cargo.lock` dependency
  change (no new crate, no new dependency — D-3.2; the JSON escaper is hand-rolled); exactly ONE new
  `ConfigError` variant (`AmbiguousLogFormat`); NO new cargo-fuzz target (the seed reuses `parse_bootstrap`);
  NO `unsafe` added (`#![forbid(unsafe_code)]` holds in all four touched crate roots — D-3.8).
- Fuzz seed `git ls-files`-tracked (the gitignored-corpus trap avoided via the `!`-un-ignore line).
- **The §2.2 deferrals are respected:** no `typed_json_format` field (type-inference is folded into plain
  `json_format` per ADR-0092 §B/§C — a sanctioned reconciliation, see Recommendations); no nested/non-string
  values; no `json_format_options`/`omit_empty_values`/`content_type`; no new operators/record fields.

---

## Strengths (file:line)

- **Faithful ADR-0092 §F byte-exact line.** `CompiledJsonFormat::render` (`crates/envoy-accesslog/src/
  json_format.rs`) emits `{`, comma-joined `"key":value`, `}\n` with compact separators; the unit test
  `renders_authoritative_fixture_line` pins the exact §F bytes including `"status":200` (unquoted),
  `"upstream":null`, and `"mixed":"code-200"`.
- **Type-inference matches §B precisely.** `encode_json_value` classifies on the single-segment-op shape
  `[Segment::Op(op)]`; `encode_single_op` routes numeric ops (`ResponseCode`/`BytesReceived`/`BytesSent`/
  `Duration`) to unquoted numbers, always-present strings to quoted, Option-backed ops to `null`-when-absent;
  the mixed/literal path falls through to `render_value_segments` (the `-` sentinel engine) then quotes.
- **Byte-freeze of the text path is structurally sound.** The `render_value_segments` extraction
  (`command_operator.rs`) has `CompiledFormat::render` delegate to it, carrying the M32-6 `literal_len + 64`
  capacity pre-alloc verbatim; text-path semantics unchanged (all pre-existing `command_operator`/`file_sink`
  tests green per PROGRESS; all 45 differential fixtures byte-identical per the state-4 gate).
- **Escaper matches §D / serde_json defaults.** `json_escape_into` handles short escapes, `\u00XX` for other
  C0, non-ASCII verbatim, `/` unescaped; the 8-case `escapes_per_json_rules` test pins each rule. Both keys
  and string values route through it.
- **Config model + validator correct.** `SubstitutionFormatString` (`bootstrap.rs`) is the oneof with
  `BTreeMap` (sorted = §A) retaining `deny_unknown_fields`; the exactly-one-of validator maps
  both-set/neither-set → `AmbiguousLogFormat`, empty map → valid, malformed value → `InvalidAccessLogFormat`
  — exactly §E. Five targeted tests drive full `parse_bootstrap`.
- **Genuine cross-proxy fixture.** `0046` configures keys in deliberately NON-sorted config order and the
  expected output is sorted — so the differential genuinely proves both proxies sort identically. The unit
  test pins envoy-rust to §F and the differential pins envoy-rust == live Envoy, transitively pinning
  Envoy == §F.
- **Constraint discipline verified:** `#![forbid(unsafe_code)]` in all touched crates; no Cargo.toml
  additions; exactly one new `ConfigError` variant; fuzz seed un-ignored and git-tracked.

## Issues

### Critical
None. No correctness divergence from ADR-0092, no byte-freeze regression, no scope/constraint violation found.

### Important
None blocking.

### Minor

- **M38-1 (maintainability): duplicated Req/Resp resolve+truncate logic across two render paths.**
  `encode_single_op` (`crates/envoy-accesslog/src/json_format.rs`) re-implements the `resolve_req`/
  `resolve_resp` + `or_else(alt)` + `truncate_bytes` chain that already exists in `render_op`
  (`command_operator.rs`). The JSON variant needs the `Option` (to choose `null` vs quoted) where the text
  variant collapses to `-`, so some divergence is justified — but the two will drift if a future operator
  changes its resolution. *Fix (optional, fold into the next `json_format`/`command_operator`-touching
  phase):* a shared `pub(crate) fn resolve_req_value(op, record) -> Option<String>` helper both call, with
  the `-` vs `null` decision left to each caller. Mitigated today by tests; not blocking.

- **M38-2 (carry-forward): `%DYNAMIC_METADATA%` single-op JSON classification is unverified cross-proxy and
  diverges from the text path.** `encode_single_op` (`json_format.rs`) QUOTES a present metadata value,
  whereas the text path renders it raw-unquoted (`command_operator.rs`, §A3). Documented in PROGRESS +
  BEHAVIOR_CONTRACT §B as not-separately-recon'd / backstop-only, and NOT in fixture `0046` — acceptable per
  the plan. Since the metadata store holds only `String` values, numeric type-inference is structurally
  impossible; the only open question is present-quoting vs raw. *Fix:* confirm cross-proxy in a future phase,
  or downgrade the carry-forward once observed.

- **M38-3 (coverage): empty value string (`""`) edge untested.** `parse_format("")` yields an empty `Vec`,
  which `encode_json_value` falls through to the else-branch → `""` (empty quoted string). Plausibly correct
  (Envoy emits an empty string) but neither tested nor recon'd. *Fix:* a one-line unit assertion.

- **M38-4 (coverage): single-op JSON test gaps.** No unit test exercises `%REQ(...):N%` truncation or `?ALT`
  fallback IN the typed JSON path (only the text path), nor a non-zero `%DURATION%` (only `0`), nor a
  control-char/non-ASCII byte inside a RENDERED value (the escaper is tested only in isolation). The encode
  logic reuses proven helpers so risk is low; a couple of assertions would harden the typed path directly.

## Deferred / pre-existing — correctly handled (NOT bugs)

- **H1 both-set branch prefers text rather than mirroring the validator** (`crates/envoy-http1/src/hcm.rs`
  `compiled_log_format`: `(Some(ds), _) => text`). Unreachable — the envoy-config validator (Task 2) rejects
  both-set before build; this is intentional defense-in-depth per PLAN Task 7, and a comment explains it.
  Purely cosmetic asymmetry; NOT a new carry-forward.
- **H2 config-driven `log_format` (text or JSON) remains unwired — pre-existing, out of scope.** The only
  `FileSink::new` in `crates/envoy-http2/src/hcm.rs` is the test helper, always `CompiledFormat::default()`;
  the phase-38 H2 edits are mechanical `json_format: None` compile-fixes. An H2 listener configured with
  `json_format` would not render JSON — but this matches the SPEC's "H2 = default site" posture and the
  phase-32 baseline (H2 already ignores configured `text_format_source`). NOT a phase-38 regression; flagged
  only so it is consciously confirmed as a pre-existing limitation.

## Recommendations

- **Accept the SPEC→ADR deviation as correctly handled.** SPEC §2.2 originally projected plain `json_format`
  as all-strings with `typed_json_format` deferred, but the §6.2 recon (ADR-0092 §B/§C) found v1.33.0
  type-infers inherently and `typed_json_format` is not a v1.33.0 field. SPEC §6.1 explicitly anticipated
  this as a possible split trigger; the team chose to FOLD type-inference into this phase rather than split
  (ADR-0093 NOT fired). Documented in PLAN, PROGRESS, and BEHAVIOR_CONTRACT §C; the added code is modest —
  a sanctioned reconciliation, not scope creep.
- Address M38-1/M38-3/M38-4 opportunistically in the next phase touching the access-log surface; none gate
  the merge.
- Keep M38-2 tracked for the eventual `typed_json_format`/non-string-metadata follow-up phase.

## Assessment

**Ready to merge: Yes.** The implementation matches every empirically-locked ADR-0092 fact (§A sorted keys,
§B type-inference, §D escaping/separators/trailing `\n`, §E validity, §F byte-exact line), preserves the
text path byte-for-byte via a clean delegating refactor, and honors all hard constraints (no new
crate/dependency, exactly one new `ConfigError` variant, `#![forbid(unsafe_code)]` intact, fixture `0046` +
45 regression witnesses green per the state-4 gate). The remaining items are minor maintainability/coverage
polish and a documented in-scope carry-forward. §7.5 gate (f) SATISFIED → advance to state-6.

---

## Carry-forward Minors after phase 38

**NEW this phase:** **M38-1** (duplicated Req/Resp resolve+truncate across the text/JSON render paths — fold
a shared `resolve_*_value` helper into the next `command_operator`/`json_format`-touching phase) + **M38-2**
(`%DYNAMIC_METADATA%` single-op JSON quoting unverified cross-proxy / diverges from the raw text path —
confirm or downgrade in the `typed_json_format`/non-string-metadata follow-up) + **M38-3** (empty value
string `""` JSON edge untested) + **M38-4** (typed-JSON-path test gaps: `:N` truncation / `?ALT` / non-zero
`%DURATION%` / control-char in a rendered value).

**Unchanged open carry-forwards (none blocks; `json_format` does NOT touch `rbac.rs`):** M37-2 + M37-1 +
M36-1 + M36-2 + M36-3 + M34-1/M34-2/M34-3 + M33-1/M33-2 + the empty-`metadata_match` doc-comment +
M29-1/M29-2 + M30-1/M30-2 + the phase-31 cosmetics + the HTTP-filters-family (1)-(4). **M35-1 remains
CLOSED** (consumed by phase-36 F2).

---

_Reviewed against `SPEC.md` (scope locked by ADR-0091), `PLAN.md` + `PROGRESS.md` (empirical ground truth
locked by ADR-0092 §A–§F), and `BEHAVIOR_CONTRACT.md` `json_format` subsection. APPROVE (0 Critical, 0
Important, 4 new Minor M38-1..M38-4 + 2 documented/pre-existing notes). §7.5 gate (f) SATISFIED — the
phase-done gate is COMPLETE; the state-6 phase-close (ROADMAP row 38 → `done` + STATE awaiting-next-planning)
is the next session._
