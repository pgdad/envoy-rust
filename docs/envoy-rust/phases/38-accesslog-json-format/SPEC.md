# Phase 38 — `38-accesslog-json-format` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0091** (the phase-38 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

**Add the `json_format` access-log encoder — a JSON-object output mode for the file access logger that
re-uses the phase-32 command-operator engine over the EXISTING `AccessLogRecord`, with ZERO new
connection-context plumbing and ZERO new request-attribute fields.** Phase 32 generalized the hard-coded
default-format emitter into a parsed `%COMMAND%` substitution engine (`crates/envoy-accesslog/src/
command_operator.rs`) and added a `FileAccessLog.log_format.text_format_source.inline_string` field that
emits ONE text line per request. Envoy's `SubstitutionFormatString` is a oneof: alongside
`text_format_source` it carries **`json_format`** (a `google.protobuf.Struct` — an ordered map of string
keys → per-value format strings), which emits ONE JSON object per request instead of a text line. Phase 32
**explicitly deferred `json_format`** (`crates/envoy-config/src/bootstrap.rs:682,695`; ADR-0078 §2.2).
This phase lands it: a `json_format` arm on `SubstitutionFormatString`, a JSON encoder in
`envoy-accesslog` that compiles each map value through the EXISTING `parse_format`/`CompiledFormat` engine
and assembles a single-line JSON object, and a byte-exact JSON-line differential fixture.

**`json_format` is the cheapest-strong next Observability leaf** — the entire phase-32 engine
(`parse_format`, `CompiledFormat::render`, the deterministic operator set, the absent-value `-` sentinel,
the `:N` truncation grammar), the `AccessLogRecord` value-type (16 fields incl. `dynamic_metadata`), the
`FileSink` plumbing, and
the existing `Driver`/`AccessLogByteExactProbe` differential harness path are all REUSED. There is **NO
new `HttpFilterInstance` variant, NO new request attribute, NO metadata store, NO producer chain, NO new
crate or dependency** — it is a new *output envelope* over an engine and a record that already exist. By
contrast the not-yet-shipped RBAC connection-context conditions need new socket-address/SNI plumbing
threaded into `FilterRequest`, and the remaining non-string `ValueMatcher` variants need a shared
non-string-Value generalization; both are heavier per unit of differential strength. See ADR-0091 for the
pick rationale and rejected alternatives.

**The differential is byte-exact, DETERMINISTIC, and LOCALLY observable** (an access-log file scrape on a
normal request/response, no file-watch/reload trigger — NOT Linux-CI-only, unlike phases 26/27): the
probe drives a fixed request → a fixed `AccessLogRecord` → a byte-identical JSON line on both proxies. The
**load-bearing differential richness is the JSON envelope**: the same deterministic operators that
fixture `0040` proved byte-exact as a *text line* must now render byte-exact inside a *JSON object* —
which forces envoy-rust to replicate Envoy's exact JSON shape (key ordering, value string-quoting, JSON
string escaping, compact separators, the trailing line terminator, and the absent-value rendering inside
a JSON value). The empirically-uncertain facets (above all **key ordering** and **string-vs-typed value
emission**) are locked at the §6.2 reconnaissance (ADR-0092 reserved); this SPEC states the projected
shape.

## §1 — Goal & differential surface

**Goal.** Add a `json_format` output mode to the `envoy.extensions.access_loggers.file.v3.FileAccessLog`
access logger, behaviorally equivalent to upstream Envoy v1.33.0 under the differential contract (§7.2 of
`BOOTSTRAP_PROMPT.md`) on the **Access log records** dimension (the contract's "Access log records:
semantically equal after field-mapping" row, sharpened here to **byte-exact whole-line** for the curated
deterministic operator set — the same standard fixture `0040` holds for the text format). The JSON line a
request produces is a deterministic function of the (fixed) probe request + static `json_format` config.

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0046-accesslog-json-format`** (next free number; baseline is `0001`…`0045`): an H1 listener
  whose file access logger configures a `log_format.json_format` map of keys → deterministic
  command-operator value strings (e.g. `{"method":"%REQ(:METHOD)%", "path":"%REQ(:PATH)%",
  "protocol":"%PROTOCOL%", "status":"%RESPONSE_CODE%", "bytes_rcvd":"%BYTES_RECEIVED%",
  "bytes_sent":"%BYTES_SENT%", "flags":"%RESPONSE_FLAGS%"}` — the exact key set/operator set the state-2
  PLAN-write finalizes, §3.5). The driver issues a request and scrapes the log file; the emitted line is a
  single JSON object compared **byte-exact** cross-proxy. **§6.2-VERIFY** the exact JSON shape (§3.2). The
  non-deterministic operators (`%START_TIME%`/`%DURATION%`/`%UPSTREAM_HOST%` etc.), if exercised at all,
  are proven by the in-process backstop, not by the byte-exact fixture (the fixture `0040` discipline —
  keep the differentiated line deterministic).
- **All 45 pre-existing fixtures `0001`–`0045` stay green simultaneously** — `json_format` is an additive
  config arm that no existing logger uses; every existing access-log fixture (`0012` default-format,
  `0040` text-custom-format, `0041`/`0042` `%DYNAMIC_METADATA%`) keeps its existing text logger and is
  **byte-identical**. The text/default code path is byte-preserved (the JSON encoder is a sibling that the
  default/text loggers never enter). This is the load-bearing regression proof.

**Conformance:** h2spec pass-rate ≥95% (unchanged — no HTTP/2 codec change). No new conformance suite.
Fuzz: the `json_format` value strings reuse the EXISTING `parse_format` engine, and the `json_format`
config map reuses the serde/`deny_unknown_fields` parse path, so the existing `parse_bootstrap` and
`accesslog_format_parse` fuzz targets cover the new surface. Add a `parse_bootstrap` seed exercising a
`json_format` logger (and, if §3 decides, an `accesslog_format_parse` seed over a JSON value string). A
**new** dedicated JSON-encoder fuzz target is projected NOT warranted (the encoder is a render-side
assembler over already-fuzzed parse inputs) — a §3 PLAN-write call.

## §2 — Scope (minimum-viable)

Per §6.3 (no vague deferral): every capability is either IN this phase and tested, or an explicit
deferred non-goal with its own future home. Exact dispositions marked **§6.2-VERIFY** are empirically
locked at the state-2 PLAN-write (the phase-22/23/28/29/30/31/32/33/34/35/36/37 verify-at-PLAN-write
discipline); this SPEC states the projected shape.

### §2.1 IN scope

1. **The `json_format` config schema.** Extend `SubstitutionFormatString`
   (`crates/envoy-config/src/bootstrap.rs:697-701`; `FileAccessLog` itself at `:687`) from a single-field
   struct
   (`text_format_source: DataSourceInline`) into a **oneof** carrying EITHER `text_format_source` (the
   existing inline text format) OR `json_format` (new). `json_format` models Envoy's
   `google.protobuf.Struct` as an **insertion-order-preserving** map of string key → string value (each
   value a command-operator format string) — projected `Vec<(String, String)>` (NOT `BTreeMap`, which
   would re-sort keys; key ordering is byte-load-bearing, §3.1/§3.2). Exactly-one-of
   `{text_format_source, json_format}` is enforced by a validator (the existing `DataSource`
   "exactly one of {filename, inline_string}" cardinality pattern, `bootstrap.rs:4440-4448`, is the
   template). **§6.2-VERIFY** the exact wire shape (the `json_format` map nesting under `log_format`; the
   `Struct` JSON/YAML form) round-trips through `/config_dump`, and whether Envoy preserves config key
   order (§3.1).
2. **The JSON encoder (`envoy-accesslog`).** Add a JSON output mode to the compiled-format representation
   — projected either a `CompiledFormat` enum widening (`Text(Vec<Segment>)` | `Json(Vec<(String,
   Vec<Segment>)>)`) or a sibling compiled type (a §3.4 factoring call). Each `json_format` value string
   is compiled through the EXISTING `parse_format` (`crates/envoy-accesslog/src/command_operator.rs:161`)
   so the same operator set, absent-value `-` sentinel, and `:N` truncation apply per value. The encoder
   assembles **one JSON object per request**: for each `(key, compiled-value)` in order, render the value
   via the existing `render`-equivalent against the `AccessLogRecord`, then emit `"key":"<value>"` with
   the value (and key) **JSON-string-escaped**, joined by `,`, wrapped in `{…}`, followed by the line
   terminator. **§6.2-VERIFY**: (a) plain `json_format` emits EVERY value as a JSON **string** (vs
   `typed_json_format`'s typed numbers/bools — DEFERRED, §2.2); (b) the exact separators (compact
   `{"k":"v","k2":"v2"}` vs spaced); (c) the trailing line terminator (`\n` after each object — JSONL); (d)
   the absent-value rendering inside a JSON value (the `-` sentinel string vs `null` vs empty vs omitted);
   (e) the JSON string-escaping rules (`"`, `\`, control chars, non-ASCII) (§3.2/§3.3).
3. **The `FileSink` wiring (reuse, minimal change).** `FileSink` already holds a single
   `format: CompiledFormat` and writes its `render` output VERBATIM (`crates/envoy-accesslog/src/
   file_sink.rs:33-122`, with the format string owning its own line terminator — the phase-32 verbatim
   refactor). The JSON encoder slots into the same `format` field (or its widened type); `FileSink::emit`
   renders + writes verbatim UNCHANGED. The fire-and-forget HCM dispatch site is UNCHANGED.
4. **The config validator + `ConfigError`.** Each `json_format` value string is parsed at config-load via
   the EXISTING `envoy_accesslog::parse_format` and a malformed/unknown operator surfaces as the EXISTING
   `ConfigError::InvalidAccessLogFormat { detail }` (`crates/envoy-config/src/lib.rs:363-366`,
   `bootstrap.rs:4362-4364`) — projected **NO new `ConfigError` variant** (the text-format validator is
   reused per-value; the exactly-one-of `{text_format_source, json_format}` cardinality and an empty
   `json_format` map may need a small variant — **§6.2-VERIFY** Envoy's disposition, §3.4). All validity
   failures are boot-fatal (ADR-0049; no reload path this phase).
5. **Tests.** Fixture `0046` (the byte-exact JSON-line differential above) + all `0001`–`0045` unchanged
   (the regression-equivalence witnesses; `0012` default + `0040` text-custom + `0041`/`0042`
   `%DYNAMIC_METADATA%` byte-identical) + an in-process backstop (the richer, deterministic complement,
   mirroring the phase-32 fixture-vs-backstop split): the JSON-object assembly for a multi-key format; key
   order preserved as configured; value JSON-escaping (a value containing `"`, `\`, a control char,
   non-ASCII); the absent-value rendering inside a JSON value (an operator with no data → the locked
   sentinel); a single-key and an empty-ish format; a value mixing a literal + an operator + a `:N`
   truncation; the default/text path byte-unchanged (a round-trip witness — same record, text vs JSON
   encoders). Plus a `parse_bootstrap` seed (a `json_format` logger) and a BEHAVIOR_CONTRACT "Access log
   field mapping" subsection documenting the JSON shape + the locked §6.2 facts.

### §2.2 DEFERRED non-goals (explicit; each names its future home)

- **`typed_json_format`** — Envoy's typed JSON encoder emits numbers/bools/null UNQUOTED when a value is a
  single typed operator (e.g. `%RESPONSE_CODE%` → `200` not `"200"`, `%BYTES_SENT%` → an integer). Plain
  `json_format` (all values as JSON strings) is the strictly-smaller leaf; `typed_json_format` is the
  natural additive follow-up over the SAME JSON encoder (it adds type-inference per value, no new
  envelope). Its own future phase.
- **Nested / non-string `json_format` values** — a `Struct` value that is itself a `Struct` or a list (a
  nested JSON object/array in the format) — deferred; the MVP `json_format` is a flat map of string-keyed
  string-valued operator format strings. Future increment with `typed_json_format` or its own slice.
- **`json_format_options` (sort_properties) + `omit_empty_values` + `content_type`** — Envoy's
  `SubstitutionFormatString` knobs for alphabetical key sorting, dropping empty values, and overriding the
  emitted content-type. The MVP locks the config key order (no `sort_properties`), keeps every key (no
  `omit_empty_values`), and does not surface `content_type`. Each is an additive knob for a future phase.
- **The deprecated `text_format` / top-level `format` scalar paths** — already deferred at phase 32
  (ADR-0078/0079); unchanged. The MVP supports the modern `text_format_source.inline_string` (existing) +
  `json_format` (new) arms only.
- **New `AccessLogRecord` fields / new operators** — `json_format` reuses the EXISTING 15-field record +
  the EXISTING phase-32/33 operator set verbatim. Any operator needing new record plumbing
  (route-name/cluster-name/SNI/`%RESPONSE_CODE_DETAILS%`/`%TRAILER%`/address operators) stays deferred to
  its own future phase, exactly as phase 32 deferred them.
- **The other Observability-family surfaces** (gRPC ALS, OTLP access log, OTel/Zipkin/Jaeger tracing,
  stats sinks, tap filter) — each its own future phase (the phase-32 deferral list, unchanged). The
  `json_format` encoder is a prerequisite/sibling for the gRPC-ALS/OTLP structured sinks but does not
  build them.

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)

These are decisions the state-2 PLAN-write makes after the §6.2 reconnaissance; the brainstorm
deliberately leaves them open:

1. **The `json_format` config key ordering** (THE key §6.2 item for byte-exactness) — confirm whether
   v1.33.0 emits `json_format` keys in the config/Struct insertion order (the projection → drives the
   `Vec<(String,String)>` insertion-order model) or alphabetically (would drive a sort + a simpler map).
   The YAML/JSON `Struct` parse must preserve whatever order Envoy emits. This single fact decides the
   config representation (insertion-order `Vec` vs sorted `BTreeMap`) and is load-bearing for the
   byte-exact fixture.
2. **The exact JSON wire shape** — with a live `json_format` logger, record: are ALL values JSON strings
   (plain `json_format`, the projection) including numeric operators (`%RESPONSE_CODE%` → `"200"`)? the
   separators (compact `{"k":"v","k2":"v2"}` — projected, vs spaced)? the trailing terminator (`\n` per
   object — projected)? the absent-value rendering inside a value (the `-` sentinel string — projected, vs
   `null`/empty/omitted)? Pin each against live Envoy and lock the fixture to the byte-exact subset.
3. **The JSON string-escaping rules** — confirm Envoy's escaping for a value containing `"`, `\`, control
   chars (`\n`, `\t`), the forward slash `/` (escaped or not), and non-ASCII (UTF-8 verbatim vs `\uXXXX`).
   The backstop locks the escaping; the fixture stays in the unambiguous-ASCII subset so the cross-proxy
   line is byte-identical regardless of any escaping edge (record any divergence as a named carry-forward).
4. **The compiled-format factoring + the `ConfigError` disposition** — (a) `CompiledFormat` enum widening
   (`Text`|`Json`) vs a sibling `CompiledJsonFormat` type held by an enum on `FileSink.format`; (b) whether
   the exactly-one-of `{text_format_source, json_format}` cardinality + an empty `json_format` map need a
   new `ConfigError` variant (e.g. `EmptyJsonFormat` / `AmbiguousLogFormat`) or reuse
   `InvalidAccessLogFormat` — projected reuse where possible, a small new variant only if Envoy's
   disposition forces it (**§6.2-VERIFY** Envoy's behavior on an empty `json_format` and on both arms set
   at once).
5. **The fixture-0046 shape** — the `json_format` key set + operator set (the deterministic-only subset;
   projected a handful of `%REQ(...)%`/`%RESPONSE_CODE%`/`%PROTOCOL%`/`%BYTES_*%`/`%RESPONSE_FLAGS%`
   keys); `direct_response` vs a real `http1-echo-server` backend (projected `direct_response` — fully
   upstream-independent like `0040`, unless a `%UPSTREAM_HOST%`-style key is wanted, which would force a
   backend and is projected to stay in the backstop); whether to reuse the existing
   `Driver::Http1WithAccessLog`/`AccessLogByteExactProbe` (`tests/differential/src/access_log.rs`) with a
   whole-line byte-exact JSON compare (projected yes) or add a JSON-aware comparator (projected NO — a
   whole-line byte-exact compare is the strongest signal and matches `0040`).
6. **The harness** — confirm the existing `Driver::Http1WithAccessLog` + the byte-exact line scrape
   comparator (used by `0040`/`0041`/`0042`) compares a JSON line byte-exact with no new capability
   (projected none). If a JSON-structural (order-insensitive) compare is ever needed it is a fallback only;
   the primary differential is whole-line byte-exact.
7. **The fuzz disposition** — confirm the existing `parse_bootstrap` (config parse, incl. the per-value
   `parse_format` validation) + `accesslog_format_parse` (the format-string tokenizer) targets cover the
   `json_format` surface (projected yes — the value strings ARE format strings the latter already fuzzes;
   the config map is the former's domain) and decide whether a dedicated JSON-encoder/render fuzz target is
   warranted (projected NO) vs seeds only. If a new target IS added, wire it into `ci.yml` in state-3 (the
   new-fuzz-target discipline — a new target is NOT auto-discovered).
8. **The §6.1 split decision** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)

- **The phase-32 command-operator engine** (`crates/envoy-accesslog/src/command_operator.rs`:
  `parse_format(s) -> Result<Vec<Segment>, FormatParseError>` `:161`; the `Segment`/`Op` enums `:24-91`;
  `CompiledFormat(Vec<Segment>)` `:403` with `from_inline` `:409` + `render(&AccessLogRecord) -> String`
  `:419` + the `Default` Envoy-default-text-format impl `:444`; the absent-value `-` sentinel + the `:N`
  truncation grammar) — phase 38 REUSES `parse_format` to compile EACH `json_format` value string and the
  per-segment render logic to evaluate each value; the JSON encoder is a NEW assembler around these,
  adding the `{…}` envelope + key strings + JSON escaping. The text/default render path is UNCHANGED.
- **The `AccessLogRecord` value-type** (`crates/envoy-accesslog/src/record.rs`: the 16 fields incl.
  `dynamic_metadata`) — READ verbatim by the JSON encoder exactly as the text encoder reads it. UNCHANGED.
  NO new field.
- **`FileSink`** (`crates/envoy-accesslog/src/file_sink.rs`: `new(path, format)` `:47`,
  `emit(&record)` `:97` rendering `self.format.render(record)` and writing it VERBATIM — the phase-32
  format-owns-its-own-newline refactor) — the JSON compiled format slots into the same `format` field (or
  its widened enum); `emit` is UNCHANGED. The fire-and-forget HCM dispatch (`envoy-http1::hcm`,
  `envoy-http2::hcm`) is UNCHANGED.
- **The `SubstitutionFormatString` / `FileAccessLog` config** (`crates/envoy-config/src/bootstrap.rs:
  669-709`: `AccessLogTypedConfig::FileAccessLog` `:669`, `FileAccessLog { path, log_format:
  Option<SubstitutionFormatString> }` `:687`, `SubstitutionFormatString { text_format_source }` `:697`,
  `DataSourceInline { inline_string }` `:707`) — phase 38 widens `SubstitutionFormatString` to a
  `{text_format_source | json_format}` oneof + the exactly-one validator; the rest is reused. The
  `DataSource` "exactly one of {filename, inline_string}" cardinality validator (`bootstrap.rs:4440-4448`)
  is the template.
- **The access-log config validator** (`crates/envoy-config/src/bootstrap.rs:4358-4366` calling
  `envoy_accesslog::parse_format(&fmt.text_format_source.inline_string)` and mapping to
  `ConfigError::InvalidAccessLogFormat`) — phase 38 extends it to iterate the `json_format` map and call
  `parse_format` per value, reusing the SAME `ConfigError::InvalidAccessLogFormat` (projected; §3.4).
- **The differential harness access-log path** (`tests/differential/src/access_log.rs`,
  `Driver::Http1WithAccessLog`, `AccessLogByteExactProbe` `tests/differential/src/lib.rs:1015`) — the
  byte-exact log-line scrape used by `0040`/`0041`/`0042`; the template for fixture `0046` (whole-line
  byte-exact compare of a JSON object). No new comparator projected.
- **The `parse_bootstrap` + `accesslog_format_parse` fuzz corpora + their `ci.yml` steps** + the
  BEHAVIOR_CONTRACT "Access log field mapping" section — extend each; no new fuzz target projected.

## §5 — Behavioral contract notes

- **The new axis (output ENVELOPE, not new data):** phases 32–37 added engine + operators + record data
  (the command-operator engine, the dynamic-metadata store, the `%DYNAMIC_METADATA%` operator) and RBAC
  conditions. Phase 38 adds a new *encoding* of the SAME record through the SAME engine — a JSON object
  instead of a text line. It reads no new request attribute and stores no new state.
- **The JSON-envelope byte-exactness (the load-bearing distinction):** the deterministic operators fixture
  `0040` proved byte-exact as a text line must now render byte-exact INSIDE a JSON object. The
  implementation MUST replicate Envoy's exact JSON shape — key ordering, all-values-as-strings (plain
  `json_format`), JSON string-escaping, compact separators, the per-object `\n` terminator, and the
  absent-value rendering inside a JSON value. The fixture's whole-line byte-exact compare is the
  cross-proxy guard; the empirically-uncertain facets are locked at §6.2, NOT assumed.
- **Key ordering is byte-load-bearing:** a JSON object's key order is part of the emitted bytes. The
  config representation MUST preserve whatever order Envoy emits (projected config insertion order →
  `Vec<(String,String)>`); a `BTreeMap` would silently re-sort and break the byte-exact differential
  unless §6.2 shows Envoy ALSO sorts. This is the single most important §6.2 fact (§3.1).
- **Reuse, not reinvention:** every operator's per-value rendering (absent-value `-`, `:N` truncation,
  `%REQ(NAME?ALT)%` alternation) is the phase-32 engine UNCHANGED; the JSON encoder only adds the envelope
  + escaping. A value-string that is a valid text format is a valid JSON value format.
- **Determinism / byte-exactness (the strong target):** every JSON line is a function ONLY of the (fixed)
  probe request + the static `json_format` config — identical on both proxies. Non-deterministic operators
  (`%START_TIME%`/`%DURATION%`/`%UPSTREAM_HOST%`) are kept OUT of the byte-exact fixture line (the `0040`
  discipline) and proven only by the in-process backstop.
- **Regression-equivalence (the load-bearing proof):** `json_format` is an additive config arm no existing
  logger uses; the text/default render path is byte-preserved (the JSON encoder is a sibling the
  text/default loggers never enter) — so all 45 existing fixtures (incl. `0012` default, `0040`
  text-custom, `0041`/`0042` dynamic-metadata) stay green unchanged.
- **Config validity:** an empty `json_format` map, both `{text_format_source, json_format}` arms set at
  once, and a malformed operator in any `json_format` value are startup-fatal where §6.2 shows Envoy
  rejects (ADR-0049 all-fatal; no reload path this phase). A malformed value-operator is rejected at BOOT
  (the existing per-value `parse_format` validator), not at first request.
- **Differential locality:** the JSON line is observable on a normal request/response WITHOUT a
  file-watch/reload trigger → fixture `0046` runs and is authoritative on this Docker-Desktop host (NOT
  Linux-CI-only, unlike phases 26/27).

## §6 — Process

### §6.1 — Split projection (§6.1 gate)

A split is projected **NOT to fire**. The surface is ONE new config arm + ONE new output encoder over an
EXISTING engine and record (no new request attribute, no new infrastructure): the `SubstitutionFormatString`
oneof widening + the exactly-one validator + the per-value `parse_format` validation loop; the JSON
encoder (compile-each-value + assemble-`{…}` + JSON-escape) in `envoy-accesslog`; the `FileSink`/compiled-
format factoring; one fixture (`0046`) + the backstop + the BEHAVIOR_CONTRACT extension + the fuzz seed.
Estimate ~400–700 LoC / ~6–9 tasks, comparable to phase-32 (which built the engine from scratch) but
SMALLER (the engine + config plumbing + harness already exist). Well under the ~1500-LoC / ~25-task gate.
**ADR-0092 is reserved** for the §6.2 reconciliation; **ADR-0093 is reserved** for the split (projected
NOT to fire). A split fires only if §6.2 reveals the JSON shape (key ordering / typed-value rules /
escaping) is far gnarlier than projected — e.g. Envoy's plain `json_format` turns out to type-infer values
(blurring the `typed_json_format` deferral). The natural seam, if forced, is `38.1` (config schema +
oneof + validator + the parse-layer tests) / `38.2` (the JSON encoder + `FileSink` wiring + fixture `0046`
+ BEHAVIOR_CONTRACT + seed + close).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)

Like phases 22/23/28/29/30/31/32/33/34/35/36/37 (and unlike phases 26/27), this phase's behavior is
**locally observable** (no reload trigger). At the state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0`
with an H1 listener + a file access logger configured with a `log_format.json_format` map, and:
1. RECORD the **`json_format` key ordering** (§3.1, THE key fact): configure keys in a deliberately
   NON-alphabetical order and observe whether the emitted JSON preserves config insertion order or sorts.
   This decides the config representation (`Vec<(String,String)>` vs `BTreeMap`).
2. RECORD the **exact JSON wire shape** (§3.2): are ALL values JSON strings (incl. `%RESPONSE_CODE%` →
   `"200"`, `%BYTES_SENT%` → `"3"`)? the separators (compact vs spaced)? the per-object trailing `\n`? the
   absent-value rendering inside a value (the `-` string vs `null`/empty/omitted)? Pin each; lock the
   fixture to the byte-exact subset.
3. RECORD the **JSON string-escaping** (§3.3): emit a value containing `"`, `\`, a control char, `/`, and
   a non-ASCII byte; record Envoy's escaping. Keep the fixture in the unambiguous-ASCII subset; lock the
   escaping in the backstop; record any divergence as a named carry-forward.
4. RECORD the **config-validity dispositions** (§3.4): an empty `json_format` map, both
   `{text_format_source, json_format}` arms set at once, an unknown key under `log_format`, and a
   malformed operator in a `json_format` value — boot-fatal on both (envoy-rust all-fatal, ADR-0049).
   Decide whether a new `ConfigError` variant is needed (projected: reuse `InvalidAccessLogFormat`; a
   small `EmptyJsonFormat`/`AmbiguousLogFormat` only if forced).
5. Decide STRONG (cross-proxy byte-identical JSON line for the deterministic operator set — expected);
   record a fallback only if some facet proves non-portable (e.g. Envoy type-infers values or sorts keys
   in a way that resists a `Vec` model → adjust the representation and record the carry-forward).
**ADR-0092 (the reserved §6.2 reconciliation ADR) FIRES** at the PLAN-write if any of these materially
diverge from this SPEC's projection (notably the key ordering, the string-vs-typed value emission, the
absent-value rendering, the escaping, or the config-validity dispositions). `PLAN.md` lands with the
empirically-locked facts inline (no `[§6.2-PENDING]` projections — the verify-at-PLAN-write discipline).

### §6.3 — Anti-deferral

No vague TODOs. Every §2.1 item is implemented + tested this phase; every deferral is a §2.2 named
non-goal with a future home. The `json_format` config arm, the JSON encoder, the validator, the fixture,
and the backstop are real and differentially exercised — no stubs. The regression equivalence is proven by
all 45 existing fixtures (incl. `0012` default + `0040` text-custom + `0041`/`0042` dynamic-metadata)
staying green unchanged.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)

(a) fixture `0046` green (cross-proxy byte-identical JSON access-log line for the curated deterministic
operator set) + (b) all of `0001`–`0045` green (incl. `0012` default-format + `0040` text-custom-format +
`0041`/`0042` `%DYNAMIC_METADATA%` byte-identical — the regression-equivalence witnesses) + (c) h2spec
≥95% (unchanged — no HTTP/2 codec change) + (d) the existing `parse_bootstrap` + `accesslog_format_parse`
fuzz targets clean for the short-budget CI run (with the new `json_format` seed) — **NO new fuzz target**
(§3.7; confirm at state-2/3) + (e) `cargo build --workspace --all-targets` / `cargo clippy --workspace
--all-targets --all-features -- -D warnings` / `cargo fmt --all -- --check` / `cargo test --workspace` /
`cargo deny check` all clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds (D-3.8). No new
crate, no new dependency (D-3.2).

---

_Scope locked by **ADR-0091**. **ADR-0092 is reserved** for the §6.2 reconciliation (state-2 PLAN-write).
The §6.1 split is projected NOT to fire (**ADR-0093 reserved** for it). The state-2 PLAN-write is the next
session (`superpowers:writing-plans`), which runs the §6.2 empirical reconnaissance against live
`envoyproxy/envoy:v1.33.0` and fires ADR-0092._
