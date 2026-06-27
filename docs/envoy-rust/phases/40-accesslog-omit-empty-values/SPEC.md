# Phase 40 — `40-accesslog-omit-empty-values` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0095** (the phase-40 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

**Add the `omit_empty_values` knob to `SubstitutionFormatString` — when `true`, the access logger OMITS
entries whose substituted value is "empty", instead of emitting the placeholder.** Envoy's
`SubstitutionFormatString` (the oneof that already carries `text_format_source` [phase 32] +
`json_format` [phases 38/39, now recursive]) has a sibling scalar `bool omit_empty_values` — a field
**empirically OBSERVED on the live v1.33.0 wire** during the phase-38 reconnaissance (ADR-0092 §C
enumerated the oneof roster `{text_format, json_format, text_format_source}` + the scalar knobs
`omit_empty_values`/`content_type`/`json_format_options`/`formatters`), so unlike the non-existent
`typed_json_format` (ADR-0092 §C) its EXISTENCE is ground-truth; only its SEMANTICS are the §6.2 question. Today every
`json_format` key is always emitted (an absent operator renders `null`, a mixed/literal value renders the
`-` sentinel inside a quoted string); with `omit_empty_values: true` Envoy DROPS the keys whose value is
empty. This phase adds that knob: a `omit_empty_values: bool` field on `SubstitutionFormatString` (serde
default `false`) and a drop-empty filter in the encoder render path, plus a byte-exact differential
fixture. The exact definition of "empty" (does it drop `null`? the `-` sentinel? an empty string? does it
apply recursively to nested objects/lists? does it affect the text path?) is the load-bearing
§6.2-reconnaissance question, locked at the state-2 PLAN-write (ADR-0096 reserved).

**`omit_empty_values` is the cheapest-strong next Observability leaf** — a single boolean config field + a
drop-empty pass over the EXISTING (phase-38/39) `CompiledJsonFormat` encoder + the EXISTING
`AccessLogRecord` + `FileSink` + the `Driver::Http1WithAccessLog`/`AccessLogByteExactProbe` differential
harness. There is **NO new connection plumbing, NO new request attribute, NO new operator, NO new crate or
dependency, NO new `HttpFilterInstance` variant**, and projected NO new `ConfigError` variant — it is a
new *render option* on an encoder that already exists. By contrast `json_format_options.sort_properties`
is a WEAKER differential (the observed v1.33.0 default is already sorted — phase 38 ADR-0092 §A — so the
knob only adds the un-sort path), `content_type` is not observable in the scraped log LINE, and the other
Observability surfaces (gRPC ALS, OTLP) need new sink infrastructure. See ADR-0095 for the pick rationale
and rejected alternatives.

**The differential is byte-exact, DETERMINISTIC, and LOCALLY observable** (an access-log file scrape on a
normal request/response, no file-watch/reload trigger — NOT Linux-CI-only): the probe drives a fixed
request → a fixed `AccessLogRecord` → a byte-identical access-log line (with the empty-valued keys
dropped) on both proxies. The **load-bearing differential richness is the drop-empty semantics**: a
`json_format` whose keys mix present + empty values, emitted with `omit_empty_values: true`, must drop
EXACTLY the keys Envoy drops, in the right order, with the right `{…}` envelope — forcing envoy-rust to
replicate Envoy's exact "empty" predicate (the §6.2 question) and its post-drop object assembly.

## §1 — Goal & differential surface

**Goal.** Add `omit_empty_values` to the `envoy.extensions.access_loggers.file.v3.FileAccessLog` access
logger's `SubstitutionFormatString`, behaviorally equivalent to upstream Envoy v1.33.0 under the
differential contract (§7.2 of `BOOTSTRAP_PROMPT.md`) on the **Access log records** dimension — sharpened
(as in phases 38/39) to **byte-exact whole-line** for the curated deterministic operator set. The line a
request produces is a deterministic function of the (fixed) probe request + the static config.

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0048-accesslog-omit-empty`** (next free number; baseline is `0001`…`0047`): an H1 listener
  whose file access logger configures a `json_format` map mixing present-valued keys (`%REQ(:METHOD)%`,
  `%PROTOCOL%`, `%RESPONSE_CODE%`) and empty/absent-valued keys (e.g. `%UPSTREAM_HOST%` on a
  `direct_response` route → absent; `%REQ(X-ABSENT)%` → absent) with **`omit_empty_values: true`**. The
  driver issues a request and scrapes the log file; the emitted line is a single JSON object with the
  empty keys DROPPED, compared **byte-exact** cross-proxy. **§6.2-VERIFY** the exact "empty" predicate +
  the post-drop assembly (§3.2). A companion `omit_empty_values: false` (or absent) sub-case confirms the
  same config emits ALL keys (the knob is the only difference).
- **All `0001`–`0047` stay green simultaneously** — `omit_empty_values` defaults `false`; every existing
  logger (incl. `0012` default, `0040` text-custom, `0046` flat-JSON, `0047` nested-JSON) is unaffected
  and byte-identical. The default-off path is byte-preserved. This is the load-bearing regression proof.

**Conformance:** h2spec ≥95% (unchanged — no HTTP/2 codec change). Fuzz: the new `bool` field reuses the
serde/`deny_unknown_fields` `parse_bootstrap` path; add a `parse_bootstrap` seed with
`omit_empty_values: true`. NO new fuzz target projected (a §3 PLAN-write call).

## §2 — Scope (minimum-viable)

### §2.1 IN scope
1. **The `omit_empty_values` config field.** Add `omit_empty_values: bool` (`#[serde(default)]` → `false`)
   to `SubstitutionFormatString` (`crates/envoy-config/src/bootstrap.rs:704-709`). `deny_unknown_fields`
   retained; it composes with EITHER arm (`text_format_source` or `json_format`). No new `ConfigError`
   variant (a plain bool; the exactly-one-of `{text_format_source, json_format}` validator is unchanged).
2. **The drop-empty render pass (`envoy-accesslog`).** Thread the `omit_empty_values` flag into the
   compiled format (projected: a field on `CompiledJsonFormat` / `LogFormat`, set at `from_map`/construction
   time). When `true`, the `json_format` render DROPS each entry whose value is "empty" per the §6.2-locked
   predicate; when `false`, render is UNCHANGED (the phase-38/39 path verbatim). **§6.2-VERIFY** (§3.2):
   (a) the exact "empty" predicate — is a value empty when it is `null` (absent single-operator)? the `-`
   sentinel (mixed/literal absent)? an empty string? all of these? (b) whether the drop applies RECURSIVELY
   (a nested object/list's empty entries dropped, and an emptied nested object itself dropped) or ONLY to
   the top-level object; (c) whether `omit_empty_values` ALSO affects the `text_format` path (and how — the
   `-` sentinel) or is json-only at this pin; (d) the post-drop separators/terminator (the surviving keys
   re-assembled compact, the single trailing `\n`).
3. **The `FileSink`/HCM wiring (reuse, minimal change).** `FileSink`/`LogFormat`/the HCM `compiled_log_format`
   pass the flag through to the compiled format; `FileSink::emit` is UNCHANGED. The fire-and-forget HCM
   dispatch is UNCHANGED.
4. **Tests.** Fixture `0048` (the byte-exact drop-empty differential) + all `0001`–`0047` unchanged (the
   default-off regression witnesses) + an in-process backstop: drop-empty over a present/absent key mix;
   the default-off round-trip (same config, flag off → all keys, flag on → empty dropped); the recursive
   disposition (§6.2-locked); an all-empty map → `{}` (or its §6.2 disposition); the empty predicate edges
   (`null` vs `-` vs `""`). Plus a `parse_bootstrap` seed (`omit_empty_values: true`) and a BEHAVIOR_CONTRACT
   "Access log field mapping" `omit_empty_values` subsection documenting the §6.2 facts.

### §2.2 DEFERRED non-goals (explicit; each names its future home)
- **`json_format_options.sort_properties`** — the per-object key-sort toggle; the observed default is
  sorted (ADR-0092 §A, hardcoded). Its own future phase.
- **`content_type`** — the emitted content-type override; not observable in the scraped log LINE. Its own
  future phase.
- **CF-39-1 numeric-literal `json_format` leaves** — the protobuf-`double` formatting (`1e+06`/`"1.5"`);
  unchanged carry-forward.
- **The deprecated `text_format` / top-level `format` scalar paths** — already deferred (ADR-0078/0079).
- **New `AccessLogRecord` fields / operators**, and **the other Observability surfaces** (gRPC ALS, OTLP,
  tracing, stats sinks, tap) — each its own future phase.
- **The `text_format` interaction with `omit_empty_values`** — IF §6.2 shows it materially changes the text
  path AND it is not cheap to mirror, it is bounded to what the fixture/backstop needs and any residual is
  a named carry-forward (the MVP centers on the json_format drop-empty, the strong differential).

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)
1. **The "empty" predicate** (THE §6.2 item) — pin what value Envoy treats as empty for `omit_empty_values`
   (null / the `-` sentinel / empty string / a combination), separately for a single-operator-typed value
   (number/string/null) and a mixed/literal value. Decides the drop predicate.
2. **Recursive vs top-level-only** — does the drop apply at every object level (and does an emptied nested
   object/list itself get dropped) or only the top-level map? Pin against a nested fixture.
3. **The text-format interaction** — does `omit_empty_values` change the `text_format` line at v1.33.0
   (e.g. the `-` rendering), or is it json-only? Pin; scope per §2.2.
4. **The flag plumbing factoring** — where the flag lives (a field on `CompiledJsonFormat`/`LogFormat` vs a
   render parameter) and whether `render` gains a variant or a branch. Projected a field set at construction.
5. **The fixture-0048 shape** — the key set (present + empty mix), `direct_response` (upstream-independent),
   reuse `Driver::Http1WithAccessLog`/`AccessLogByteExactProbe` whole-line byte-exact (projected yes).
6. **The fuzz disposition** — the existing `parse_bootstrap` covers the new bool; seed only, NO new target.
7. **The §6.1 split decision** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)
- **The phase-38/39 `json_format` encoder** (`crates/envoy-accesslog/src/json_format.rs`:
  `CompiledJsonFormat(BTreeMap<String, CompiledJsonValue>)` `:107`; `from_map` `:113`; `render` `:126` +
  `render_into` `:71`; the leaf helpers `encode_json_value`/`encode_single_op`/`json_escape_into`) — phase
  40 adds the drop-empty pass around these; the render-each-value logic is UNCHANGED (a value is rendered,
  then KEPT or DROPPED by the empty predicate). The flag-off path is the phase-38/39 render verbatim.
- **The phase-32 command-operator engine** (`command_operator.rs`: `parse_format`, `render_value_segments`,
  the `-` sentinel) — UNCHANGED; reused to render each value before the empty test.
- **The `AccessLogRecord`** (`record.rs`, 16 fields) — READ verbatim. UNCHANGED. NO new field.
- **`FileSink`/`LogFormat`** (`file_sink.rs` + `log_format.rs`) — pass the flag through; `emit` UNCHANGED.
- **The `SubstitutionFormatString`/`FileAccessLog` config** (`bootstrap.rs:704-709`) — add ONE `bool` field;
  the oneof + the exactly-one validator + `deny_unknown_fields` UNCHANGED.
- **The differential harness access-log path** (`tests/differential/.../access_log*.rs`,
  `Driver::Http1WithAccessLog`, `AccessLogByteExactProbe`) — the `0046`/`0047` template for fixture `0048`.
- **The `parse_bootstrap` fuzz corpus + the BEHAVIOR_CONTRACT "Access log field mapping" section** — extend
  each; no new fuzz target projected.

## §5 — Behavioral contract notes
- **The new axis (a render OPTION, not new data):** phase 40 adds a config knob that changes WHICH rendered
  entries survive into the emitted object — it reads no new request attribute and stores no new state.
- **Default-off byte-preservation (the load-bearing regression proof):** `omit_empty_values` defaults
  `false`; with the flag off the encoder is byte-identical to phases 38/39, so all `0001`-`0047` stay green.
- **The drop-empty byte-exactness:** with the flag on, envoy-rust must drop EXACTLY the keys Envoy drops
  (the §6.2 "empty" predicate) and re-assemble the surviving keys with the same compact separators + the
  single trailing `\n`. The fixture's whole-line byte-exact compare is the cross-proxy guard.
- **Determinism / locality:** every line is a function ONLY of the (fixed) probe + the static config;
  observable on a normal request/response WITHOUT a reload trigger → fixture `0048` is authoritative on this
  Docker-Desktop host (NOT Linux-CI-only).
- **Config validity:** `omit_empty_values` is a plain bool (`deny_unknown_fields` rejects typos); no new
  validity rule, no new `ConfigError` variant. All-fatal posture unchanged (ADR-0049).

## §6 — Process
### §6.1 — Split projection (§6.1 gate)
A split is projected **NOT to fire**. The surface is ONE bool config field + ONE drop-empty pass over the
EXISTING encoder (no new request attribute, no new infrastructure): the `SubstitutionFormatString` field +
the flag plumbing through `LogFormat`/`CompiledJsonFormat` + the drop-empty render branch; one fixture
(`0048`) + the backstop + the BEHAVIOR_CONTRACT extension + the fuzz seed. Estimate **~150–350 LoC / ~5–7
tasks** — well under the ~1500-LoC / ~25-task gate, SMALLER than phases 38/39. **ADR-0096 reserved** for
the §6.2 reconciliation; **ADR-0097 reserved** for the split (projected NOT to fire — fires only if §6.2
reveals the "empty" predicate / recursive disposition / text interaction is far gnarlier than projected).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)
Like phases 38/39, locally observable (no reload trigger). At the state-2 PLAN-write, stand up
`envoyproxy/envoy:v1.33.0` with an H1 listener + a file logger configured `omit_empty_values: true`, and
RECORD: (1) the **"empty" predicate** (§3.1) — which value(s) get dropped (null / `-` / empty string),
for single-operator-typed AND mixed/literal values; (2) **recursive vs top-level-only** (§3.2) — drop in
nested objects/lists? emptied nested container dropped?; (3) the **text-format interaction** (§3.3); (4)
the **all-empty disposition** (a map all of whose values are empty → `{}` vs omitted-object); (5) the
post-drop separators/terminator. **ADR-0096 FIRES** at the PLAN-write if any materially diverge from this
SPEC's projection. `PLAN.md` lands with the locked facts inline (no `[§6.2-PENDING]` projections).

### §6.3 — Anti-deferral
No vague TODOs. Every §2.1 item is implemented + tested; every deferral is a §2.2 named non-goal. The
config field, the drop-empty pass, the fixture, and the backstop are real and differentially exercised. The
regression equivalence is proven by all `0001`-`0047` (incl. `0046`/`0047`) staying green with the flag off.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)
(a) fixture `0048` green (cross-proxy byte-identical drop-empty line + the flag-off all-keys sub-case) +
(b) all `0001`-`0047` green (default-off regression witnesses, incl. `0046` flat + `0047` nested) + (c)
h2spec ≥95% (unchanged) + (d) the `parse_bootstrap` fuzz target clean for the short-budget CI run (with the
`omit_empty_values` seed) — NO new fuzz target + (e) `cargo build`/`clippy -D warnings`/`fmt --check`/`test
--workspace`/`deny check` all clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds (D-3.8). No
new crate, no new dependency (D-3.2); projected NO new `ConfigError` variant.

---

_Scope locked by **ADR-0095**. **ADR-0096 reserved** for the §6.2 reconciliation (state-2 PLAN-write). The
§6.1 split is projected NOT to fire (**ADR-0097 reserved**). The state-2 PLAN-write is the next session
(`superpowers:writing-plans`), which runs the §6.2 empirical reconnaissance against live
`envoyproxy/envoy:v1.33.0` and fires ADR-0096._
