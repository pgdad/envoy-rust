# Phase 39 — `39-accesslog-json-nested-values` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0093** (the phase-39 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

**Make the `json_format` access-log encoder RECURSIVE — support nested JSON objects and lists as
`json_format` values, the way Envoy's `SubstitutionFormatString.json_format` is a full
`google.protobuf.Struct`.** Phase 38 (ADR-0091/0092) landed `json_format` as a FLAT map: the config model
is `json_format: Option<BTreeMap<String, String>>` (`crates/envoy-config/src/bootstrap.rs:708`) and the
encoder `CompiledJsonFormat(BTreeMap<String, Vec<Segment>>)`
(`crates/envoy-accesslog/src/json_format.rs:14`) renders ONE flat sorted JSON object per request, with
per-value type inference (single numeric operator → unquoted number, single string operator → quoted
string, single absent operator → `null`, mixed/literal → quoted string) — all locked byte-exact against
v1.33.0 by ADR-0092. But Envoy's `json_format` is a `Struct`, so a value may itself be a **nested object**
(another key→value map) or a **list** (a sequence of values), to arbitrary depth — and phase 38
EXPLICITLY DEFERRED this ("Nested / non-string `json_format` values" — ADR-0092 Scope-correction; SPEC-38
§2.2). This phase lands it: a **recursive config value type** (string format | nested object | list), a
**recursive `CompiledJsonFormat`** that compiles + renders at every depth (sorted keys per object level,
list order preserved, the SAME phase-38 type-inference at each leaf), and a byte-exact nested-JSON-line
differential fixture.

**Nested `json_format` is the cheapest-strong next Observability leaf** — it is a strictly-larger
deepening of the SAME encoder phase 38 just shipped, with **ZERO new connection-context plumbing, ZERO new
request attribute, NO metadata store, NO producer chain, NO new `HttpFilterInstance` variant, NO new crate
or dependency, and NO new operator**. The entire phase-32 command-operator engine (`parse_format`,
`render_value_segments`, the deterministic operator set, the `-` sentinel, `:N` truncation), the
`AccessLogRecord` value-type (16 fields), the `FileSink`/`LogFormat` plumbing, the hand-rolled JSON
escaper, the per-`Op` type classifier (`encode_single_op`), and the `Driver::Http1WithAccessLog`/
`AccessLogByteExactProbe` differential harness are all REUSED. The only genuinely-new code is the
recursion: a recursive config enum + a recursive compiled enum + a recursive render. By contrast the
not-yet-shipped RBAC connection-context conditions need new socket-address/SNI plumbing threaded into
`FilterRequest`, and the remaining non-string `ValueMatcher` variants need a shared non-string-Value
generalization; both are heavier per unit of differential strength. See ADR-0093 for the pick rationale
and rejected alternatives.

**The differential is byte-exact, DETERMINISTIC, and LOCALLY observable** (an access-log file scrape on a
normal request/response, no file-watch/reload trigger — NOT Linux-CI-only): the probe drives a fixed
request → a fixed `AccessLogRecord` → a byte-identical NESTED JSON line on both proxies. The **load-bearing
differential richness is the recursion**: the same deterministic operators phase 38 proved byte-exact in a
FLAT object must now render byte-exact inside NESTED objects + lists — forcing envoy-rust to replicate
Envoy's exact recursive `Struct` shape (sorted keys at EACH object level, list order preserved, the SAME
per-leaf type inference, the compact separators, and the single trailing terminator on the whole
top-level object). The empirically-uncertain facets (above all the per-LEVEL key sorting, list-order
preservation, and whether non-string SCALAR config values — a literal YAML number/bool/null in the
`Struct`, not a format string — are emitted typed or stringified) are locked at the §6.2 reconnaissance
(ADR-0094 reserved); this SPEC states the projected shape.

## §1 — Goal & differential surface

**Goal.** Extend the `json_format` output mode on the `envoy.extensions.access_loggers.file.v3.FileAccessLog`
access logger to accept nested `Struct` values (objects + lists), behaviorally equivalent to upstream
Envoy v1.33.0 under the differential contract (§7.2 of `BOOTSTRAP_PROMPT.md`) on the **Access log records**
dimension — sharpened (as in phase 38) to **byte-exact whole-line** for the curated deterministic operator
set. The nested JSON line a request produces is a deterministic function of the (fixed) probe request +
the static recursive `json_format` config.

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0047-accesslog-json-nested`** (next free number; baseline is `0001`…`0046`): an H1 listener
  whose file access logger configures a `log_format.json_format` map containing at least ONE nested object
  and ONE list value over deterministic command-operator value strings — e.g.
  `{"request": {"method":"%REQ(:METHOD)%", "path":"%REQ(:PATH)%"}, "response": {"code":"%RESPONSE_CODE%",
  "flags":"%RESPONSE_FLAGS%"}, "proto":"%PROTOCOL%", "sizes": ["%BYTES_RECEIVED%","%BYTES_SENT%"]}` (the
  exact shape the state-2 PLAN-write finalizes, §3.5). The driver issues a request and scrapes the log
  file; the emitted line is a single nested JSON object compared **byte-exact** cross-proxy.
  **§6.2-VERIFY** the exact recursive shape (§3.2). Non-deterministic operators
  (`%START_TIME%`/`%DURATION%`/`%UPSTREAM_HOST%` etc.) are kept OUT of the byte-exact fixture line (the
  phase-32/38 `0040`/`0046` discipline — keep the differentiated line deterministic); the richer nested
  cases (escaping, absent-value, deep nesting) are proven by the in-process backstop.
- **Fixture `0046-accesslog-json-format` stays green UNCHANGED** — the flat-map case is the degenerate
  depth-1 instance of the recursive model; the phase-38 byte-exact flat line must be byte-identical after
  the recursion refactor (the load-bearing regression proof for the encoder).
- **All `0001`–`0045` stay green simultaneously** — nested `json_format` is an additive config shape that
  no existing logger uses; every existing access-log fixture (`0012` default, `0040` text-custom,
  `0041`/`0042` `%DYNAMIC_METADATA%`) keeps its existing logger and is **byte-identical**. The text/default
  path and the flat-JSON path are byte-preserved.

**Conformance:** h2spec pass-rate ≥95% (unchanged — no HTTP/2 codec change). No new conformance suite.
Fuzz: the recursive `json_format` config map reuses the serde/`deny_unknown_fields` parse path
(`parse_bootstrap`) and each leaf value string reuses the EXISTING `parse_format` engine
(`accesslog_format_parse`), so the existing fuzz targets cover the new surface. Add a `parse_bootstrap`
seed exercising a NESTED `json_format` logger. A **new** dedicated JSON-encoder fuzz target is projected
NOT warranted (the recursive encoder is a render-side assembler over already-fuzzed parse inputs) — a §3
PLAN-write call.

## §2 — Scope (minimum-viable)

Per §6.3 (no vague deferral): every capability is either IN this phase and tested, or an explicit deferred
non-goal with its own future home. Exact dispositions marked **§6.2-VERIFY** are empirically locked at the
state-2 PLAN-write (the verify-at-PLAN-write discipline); this SPEC states the projected shape.

### §2.1 IN scope

1. **The recursive `json_format` config model.** Replace the flat `json_format: Option<BTreeMap<String,
   String>>` (`bootstrap.rs:708`) with a recursive value type — projected an enum
   `JsonFormatValue = Format(String) | Object(BTreeMap<String, JsonFormatValue>) | Array(Vec<JsonFormatValue>)`
   — so `json_format: Option<BTreeMap<String, JsonFormatValue>>` (the TOP level stays a key→value map, as
   Envoy's `Struct` is). serde deserializes a YAML scalar string → `Format`, a YAML map → `Object`
   (`BTreeMap`, sorted), a YAML sequence → `Array` (`Vec`, ordered) — this dispatch needs
   `#[serde(untagged)]` on `JsonFormatValue` (or a small custom `Deserialize`); untagged is dependency-free
   (no new crate, D-3.2) but interacts with the parent's `deny_unknown_fields`, so the PLAN pins the exact
   derive (§3.4). **§6.2-VERIFY** the disposition of a
   **non-string scalar leaf** (a literal YAML number/bool/null appearing as a `Struct` value, NOT a format
   string) — Envoy may emit it typed verbatim, stringify it, or reject it; the projection is that genuine
   `json_format` leaves are format strings and a bare literal scalar is rendered as its
   `Format`-string-equivalent (§3.1). `deny_unknown_fields` and the exactly-one-of `{text_format_source,
   json_format}` validator (`ConfigError::AmbiguousLogFormat`, `bootstrap.rs:4391`) are UNCHANGED. The
   empty top-level map `{}` stays valid → `{}\n` (ADR-0092 §E).
2. **The recursive `CompiledJsonFormat` encoder (`envoy-accesslog`).** Widen
   `CompiledJsonFormat(BTreeMap<String, Vec<Segment>>)` (`json_format.rs:14`) to compile + render a
   recursive value — projected a `CompiledJsonValue = Leaf(Vec<Segment>) | Object(BTreeMap<String,
   CompiledJsonValue>) | Array(Vec<CompiledJsonValue>)`. Compilation runs `parse_format` on every leaf
   string (preserving the config-load error surfacing). `render` walks the tree: an `Object` emits
   `{` + sorted `"key":<value>` pairs + `}`; an `Array` emits `[` + ordered `<value>` items + `]`; a
   `Leaf` emits the EXISTING `encode_json_value` output (the phase-38 single-operator type inference
   number/string/null + the mixed/literal quoted-string path — UNCHANGED per leaf). The top-level object
   keeps the trailing `\n`. The EXISTING `json_escape_into` / `encode_single_op` / `quote*` helpers are
   REUSED verbatim. **§6.2-VERIFY**: (a) keys are sorted at EVERY object level (projected — the same
   `BTreeMap` byte-order as depth-1, ADR-0092 §A); (b) list element order is the CONFIG order, NOT sorted
   (projected — lists are ordered); (c) the SAME per-leaf type inference applies at depth (projected); (d)
   compact separators + the single top-level `\n` (projected — no `\n` between nested elements).
3. **The `FileSink` / `LogFormat` wiring (reuse, no change).** The `LogFormat = Text(CompiledFormat) |
   Json(CompiledJsonFormat)` enum (ADR-0092) is DEFINED in `crates/envoy-accesslog/src/log_format.rs`
   (re-exported at `lib.rs`) and HELD by `FileSink.format` (`file_sink.rs:37`); `FileSink::emit` renders +
   writes verbatim. The recursion is INTERNAL to `CompiledJsonFormat`; `LogFormat`, `FileSink::emit`, and
   the H1/H2 `compiled_log_format` wiring (`envoy-http1/src/hcm.rs:1254`, the `(None, Some(map))` json arm
   at `:1269`) are UNCHANGED EXCEPT that `CompiledJsonFormat::from_map` now takes the recursive map type —
   which ripples to its two call sites: the `hcm.rs:1269` `from_map(map)` call and the per-value validator
   loop in `bootstrap.rs` (§2.1.4). The fire-and-forget HCM dispatch is UNCHANGED.
4. **The config validator + `ConfigError`.** Each leaf value string is parsed at config-load via the
   EXISTING `envoy_accesslog::parse_format`; a malformed/unknown operator at ANY depth surfaces as the
   EXISTING `ConfigError::InvalidAccessLogFormat { detail }` — projected **NO new `ConfigError` variant**
   (the recursion adds no new cardinality rule; the `AmbiguousLogFormat` exactly-one-of and the empty-map
   acceptance are UNCHANGED). All validity failures are boot-fatal (ADR-0049; no reload path this phase).
   **§6.2-VERIFY** Envoy's disposition of a degenerate nesting (an empty nested object `{}` as a value, an
   empty list `[]`) — projected both valid (emit `{}` / `[]`).
5. **Tests.** Fixture `0047` (the byte-exact nested-JSON-line differential above) + `0046` byte-unchanged
   (the flat depth-1 regression witness) + all `0001`–`0045` unchanged + an in-process backstop (the
   richer, deterministic complement, mirroring the phase-38 fixture-vs-backstop split): a nested object;
   a list of operators; a list of nested objects; key-sorting AT a nested level (configure a nested object
   with non-alphabetical keys → assert sorted output); list order preserved (configure a list and assert
   config order, NOT sorted); the SAME per-leaf type inference at depth (a nested `%RESPONSE_CODE%` → an
   unquoted `200`; a nested absent operator → `null`); JSON-escaping a nested key and a nested value; an
   empty nested object `{}` and empty list `[]`; a deeply-nested (depth ≥3) object; and the flat-map
   round-trip byte-unchanged (same record, depth-1 nested model vs the phase-38 output). Plus a
   `parse_bootstrap` seed (a nested `json_format` logger) and a BEHAVIOR_CONTRACT "Access log field
   mapping" subsection update documenting the recursive shape + the locked §6.2 facts.

### §2.2 DEFERRED non-goals (explicit; each names its future home)

- **`json_format_options.sort_properties`** — Envoy's knob to TOGGLE the per-object key sorting. The
  observed v1.33.0 default IS sorted (ADR-0092 §A) and this phase hardcodes sorted at every level; the
  knob to disable sorting (emit config/insertion order) is an additive boolean for its own future phase.
- **`omit_empty_values`** — Envoy's knob to DROP keys whose rendered value is empty/absent. The MVP keeps
  every key (renders `null`/`-` per the phase-38 rules). Its own future phase.
- **`content_type`** — overriding the emitted content-type. Not surfaced (it does not affect the scraped
  log LINE — weakly differentially observable via a file scrape). Its own future phase.
- **Non-string SCALAR leaves emitted as native typed JSON** — IF §6.2 shows v1.33.0 emits a literal YAML
  number/bool/null `Struct` leaf as a typed JSON token (vs stringifying), the FULL typed-literal support
  is bounded to what the fixture/backstop needs this phase and any richer literal-typing edge is recorded
  as a named carry-forward (the MVP centers on format-string leaves nested in objects/lists, §3.1).
- **The deprecated `text_format` / top-level `format` scalar paths** — already deferred at phase 32/38
  (ADR-0078/0079); unchanged.
- **New `AccessLogRecord` fields / new operators** — nested `json_format` reuses the EXISTING 16-field
  record + the EXISTING phase-32/33 operator set verbatim. Any operator needing new record plumbing stays
  deferred to its own future phase.
- **The other Observability-family surfaces** (gRPC ALS, OTLP access log, OTel/Zipkin/Jaeger tracing,
  stats sinks, tap filter) — each its own future phase. The nested `json_format` encoder is a
  prerequisite/sibling for the future gRPC-ALS/OTLP structured sinks but does not build them.

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)

These are decisions the state-2 PLAN-write makes after the §6.2 reconnaissance; the brainstorm
deliberately leaves them open:

1. **The non-string scalar leaf disposition** (THE new §6.2 item) — with a live nested `json_format`,
   configure a value that is a bare YAML number (`42`), bool (`true`), and null (`~`) (NOT a format
   string) and record whether v1.33.0 emits it as a typed JSON token (`42`/`true`/`null`), stringifies it
   (`"42"`), or rejects it. This decides whether `JsonFormatValue` needs a `Scalar(typed)` arm or whether
   every non-object/non-array leaf is parsed as a `Format` string (the projection — genuine json_format
   leaves are format strings).
2. **The recursive wire shape** (§3.2) — with a live nested logger, record: keys sorted at EVERY object
   level (projected — `BTreeMap` byte-order, ADR-0092 §A applied recursively)? list element order = config
   order (projected, NOT sorted)? the SAME per-leaf type inference at depth (projected — `encode_single_op`
   unchanged)? compact separators with NO inter-element whitespace and only ONE trailing `\n` on the whole
   top-level object (projected)? Pin each; lock the fixture to the byte-exact subset.
3. **Empty-degenerate dispositions** (§3) — an empty nested object `{}` as a value, an empty list `[]`, a
   list containing an absent-operator leaf — record Envoy's emission (projected `{}`, `[]`, and `null`
   inside the list).
4. **The compiled-type factoring** — the recursive `CompiledJsonValue` enum shape (`Leaf`/`Object`/`Array`)
   and whether `CompiledJsonFormat` becomes `BTreeMap<String, CompiledJsonValue>` (the top level is always
   an object) or itself a `CompiledJsonValue::Object`. The existing `encode_json_value`/`encode_single_op`/
   `json_escape_into` leaf helpers are REUSED — confirm no leaf-level behavior changes (projected none; the
   recursion is purely structural).
5. **The fixture-`0047` shape** — the nested key set + operator set (deterministic-only: a `request`
   nested object, a `response` nested object, a `sizes` list, a top-level scalar); `direct_response` (fully
   upstream-independent like `0046`) vs a real backend (projected `direct_response`); reuse the existing
   `Driver::Http1WithAccessLog`/`AccessLogByteExactProbe` with a whole-line byte-exact compare (projected
   yes — strongest signal, matches `0046`).
6. **The harness** — confirm the existing `Driver::Http1WithAccessLog` + the byte-exact line scrape
   comparator compares a NESTED JSON line byte-exact with no new capability (projected none).
7. **The fuzz disposition** — confirm the existing `parse_bootstrap` (config parse, incl. the recursive
   map + the per-leaf `parse_format` validation) + `accesslog_format_parse` (the per-leaf format-string
   tokenizer) targets cover the nested surface (projected yes) and decide whether a dedicated
   recursive-encoder fuzz target is warranted (projected NO) vs seeds only. If a new target IS added, wire
   it into `ci.yml` in state-3 (the new-fuzz-target discipline — a new target is NOT auto-discovered).
8. **The §6.1 split decision** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)

- **The phase-38 `json_format` encoder** (`crates/envoy-accesslog/src/json_format.rs`:
  `CompiledJsonFormat` `:14`; `from_map` `:19`; `render` `:30`; the leaf helpers `encode_json_value` `:74`,
  `encode_single_op` `:96` [the per-`Op` number/string/null type classifier], `json_escape_into` `:51`,
  `quote`/`quote_opt` `:83`/`:89`) — phase 39 makes the OUTER structure (`from_map`/`render` + the config
  map type) recursive; the LEAF helpers (`encode_json_value` and everything it calls) are REUSED VERBATIM —
  a nested leaf renders identically to a depth-1 value.
- **The phase-32 command-operator engine** (`crates/envoy-accesslog/src/command_operator.rs`:
  `parse_format` `:161`; `render_value_segments`; the `Segment`/`Op` enums; the `-` sentinel; `:N`
  truncation) — UNCHANGED; reused to compile + render each leaf.
- **The `AccessLogRecord` value-type** (`crates/envoy-accesslog/src/record.rs`: 16 fields incl.
  `dynamic_metadata`) — READ verbatim by the recursive encoder exactly as the flat encoder reads it.
  UNCHANGED. NO new field.
- **`FileSink` / `LogFormat`** (the `LogFormat = Text | Json` enum DEFINED in
  `crates/envoy-accesslog/src/log_format.rs` and HELD by `FileSink.format` at
  `crates/envoy-accesslog/src/file_sink.rs:37`; `emit` rendering + writing VERBATIM) — UNCHANGED except
  `CompiledJsonFormat::from_map`'s argument type (which ripples to the `hcm.rs:1269` json arm and the
  `bootstrap.rs` validator call sites). The `From<CompiledFormat>`/`From<CompiledJsonFormat>` `Into`-bridge
  and the HCM dispatch are UNCHANGED.
- **The `SubstitutionFormatString` / `FileAccessLog` config** (`crates/envoy-config/src/bootstrap.rs`:
  `SubstitutionFormatString { text_format_source: Option<DataSourceInline>, json_format:
  Option<BTreeMap<String,String>> }` `:704-708`; the exactly-one validator `:4371-4399`
  [`ConfigError::AmbiguousLogFormat`]) — phase 39 changes ONLY the `json_format` value type
  (`String` → the recursive `JsonFormatValue`); the oneof, the validator, `deny_unknown_fields`, and the
  empty-map acceptance are UNCHANGED.
- **The access-log config validator** (`crates/envoy-config/src/bootstrap.rs:4371` region calling
  `parse_format` per `json_format` value) — phase 39 extends it to recurse the tree and call `parse_format`
  per LEAF, reusing the SAME `ConfigError::InvalidAccessLogFormat`.
- **The H1/H2 `compiled_log_format` wiring** (`crates/envoy-http1/src/hcm.rs:1254`, the `(None,
  Some(map))` json arm `:1269`; the H2 default site) — UNCHANGED except the map type.
- **The differential harness access-log path** (`tests/differential/src/access_log.rs`,
  `Driver::Http1WithAccessLog`, `AccessLogByteExactProbe`) — the byte-exact log-line scrape used by
  `0040`/`0046`; the template for fixture `0047` (whole-line byte-exact compare of a NESTED JSON object).
  No new comparator projected.
- **The `parse_bootstrap` + `accesslog_format_parse` fuzz corpora + their `ci.yml` steps** + the
  BEHAVIOR_CONTRACT "Access log field mapping" section — extend each; no new fuzz target projected.

## §5 — Behavioral contract notes

- **The new axis (recursion, not new data):** phase 38 added the FLAT JSON envelope over the record +
  engine. Phase 39 adds the RECURSIVE envelope — nested objects + lists of the SAME leaves through the SAME
  engine. It reads no new request attribute and stores no new state; the only new behavior is structural
  recursion in the config model + encoder.
- **The recursion byte-exactness (the load-bearing distinction):** the deterministic operators phase 38
  proved byte-exact in a FLAT object must now render byte-exact inside NESTED objects + lists. The
  implementation MUST replicate Envoy's exact recursive shape — sorted keys at EACH object level (ADR-0092
  §A applied recursively), list order = config order (NOT sorted), the SAME per-leaf type inference,
  compact separators, and ONE trailing `\n` on the whole top-level object (no inter-element `\n`). The
  fixture's whole-line byte-exact compare is the cross-proxy guard; the empirically-uncertain facets are
  locked at §6.2, NOT assumed.
- **Key sorting is per-LEVEL and byte-load-bearing:** each nested object's keys are independently sorted by
  UTF-8 byte order (the `BTreeMap` order). A list's element order is the config order (lists are ordered,
  NOT maps). Getting either wrong breaks the byte-exact differential.
- **Reuse, not reinvention:** every leaf's rendering (type inference number/string/null, the `-` sentinel,
  `:N` truncation, `%REQ(NAME?ALT)%` alternation, JSON escaping) is the phase-38 leaf path UNCHANGED; the
  recursion only adds the `{…}`/`[…]` structural envelope around already-correct leaves.
- **Determinism / byte-exactness (the strong target):** every nested JSON line is a function ONLY of the
  (fixed) probe request + the static recursive `json_format` config — identical on both proxies.
  Non-deterministic operators are kept OUT of the byte-exact fixture line (the `0040`/`0046` discipline)
  and proven only by the in-process backstop.
- **Regression-equivalence (the load-bearing proof):** the flat-map case is the degenerate depth-1
  instance of the recursive model — fixture `0046` (flat) must stay byte-identical after the recursion
  refactor, and all 45 pre-`0046` fixtures (text/default loggers, the JSON encoder is a sibling they never
  enter) stay green unchanged.
- **Config validity:** a malformed operator in ANY nested leaf is boot-fatal (the existing per-leaf
  `parse_format` validator, now recursive); the exactly-one-of `{text_format_source, json_format}` and the
  empty top-level map acceptance are UNCHANGED (ADR-0092 §E). Degenerate empty nested object/list
  dispositions are §6.2-locked (projected valid). All boot-fatal (ADR-0049; no reload this phase).
- **Differential locality:** the nested JSON line is observable on a normal request/response WITHOUT a
  file-watch/reload trigger → fixture `0047` runs and is authoritative on this Docker-Desktop host (NOT
  Linux-CI-only).

## §6 — Process

### §6.1 — Split projection (§6.1 gate)

A split is projected **NOT to fire**. The surface is a recursion refactor of ONE existing config value type
+ ONE existing output encoder over an EXISTING engine/record/sink/harness (no new request attribute, no new
infrastructure, no new operator): the `json_format` value type → a recursive enum + the serde derive; the
`CompiledJsonFormat` → a recursive compiled enum + a recursive `render` (the leaf helpers reused verbatim);
the per-leaf-recursive config validator; one fixture (`0047`) + the backstop + the BEHAVIOR_CONTRACT
update + the fuzz seed. Estimate **~350–600 LoC / ~6–8 tasks** — SMALLER than phase 38 (which added the
whole JSON envelope + the type classifier + the config oneof + the new `ConfigError` variant; phase 39
reuses ALL of that and adds only structural recursion). Well under the ~1500-LoC / ~25-task gate.
**ADR-0094 is reserved** for the §6.2 reconciliation; **ADR-0095 is reserved** for the split (projected NOT
to fire). A split fires only if §6.2 reveals the recursive shape is far gnarlier than projected — e.g.
Envoy applies a surprising per-level ordering, or non-string scalar leaves demand a full typed-literal
`Value` model that balloons the config type. The natural seam, if forced, is `39.1` (the recursive config
model + serde + the recursive validator + parse-layer tests) / `39.2` (the recursive encoder + fixture
`0047` + BEHAVIOR_CONTRACT + seed + close).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)

Like phases 22/23/28/29/30/31/32/33/34/35/36/37/38 (and unlike phases 26/27), this phase's behavior is
**locally observable** (no reload trigger). At the state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0`
with an H1 listener + a file access logger configured with a NESTED `log_format.json_format`, and:
1. RECORD the **non-string scalar leaf disposition** (§3.1, the new key fact): a value that is a bare YAML
   number / bool / null (not a format string) — emitted typed, stringified, or rejected? This decides
   whether `JsonFormatValue` needs a typed-`Scalar` arm.
2. RECORD the **recursive wire shape** (§3.2): keys sorted at EVERY object level? list element order =
   config order (NOT sorted)? the SAME per-leaf type inference at depth (a nested `%RESPONSE_CODE%` →
   unquoted `200`; a nested absent operator → `null`)? compact separators + ONE trailing `\n` on the whole
   top-level object (no inter-element `\n`)? Pin each; lock the fixture to the byte-exact subset.
3. RECORD the **degenerate-nesting dispositions** (§3.3): an empty nested object `{}` value, an empty list
   `[]`, a list with an absent-operator leaf — record Envoy's emission.
4. RECORD the **config-validity dispositions**: a malformed operator in a NESTED leaf (boot-fatal,
   reusing `InvalidAccessLogFormat`); confirm the exactly-one-of + empty-top-map behavior is UNCHANGED from
   phase 38.
5. Decide STRONG (cross-proxy byte-identical nested JSON line for the deterministic operator set —
   expected); record a fallback only if some facet proves non-portable (e.g. non-string scalar leaves
   demand a typed model → adjust the representation and record the carry-forward).
**ADR-0094 (the reserved §6.2 reconciliation ADR) FIRES** at the PLAN-write if any of these materially
diverge from this SPEC's projection (notably the non-string scalar disposition, the per-level sorting,
list-order, or the at-depth type inference). `PLAN.md` lands with the empirically-locked facts inline (no
`[§6.2-PENDING]` projections — the verify-at-PLAN-write discipline).

### §6.3 — Anti-deferral

No vague TODOs. Every §2.1 item is implemented + tested this phase; every deferral is a §2.2 named
non-goal with a future home. The recursive config model, the recursive encoder, the recursive validator,
the fixture, and the backstop are real and differentially exercised — no stubs. The regression equivalence
is proven by fixture `0046` (flat) + all `0001`–`0045` staying green unchanged.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)

(a) fixture `0047` green (cross-proxy byte-identical NESTED JSON access-log line for the curated
deterministic operator set) + (b) all of `0001`–`0046` green (incl. `0046` flat-JSON byte-identical — the
recursion-refactor regression witness — and `0012` default + `0040` text-custom + `0041`/`0042`
`%DYNAMIC_METADATA%`) + (c) h2spec ≥95% (unchanged — no HTTP/2 codec change) + (d) the existing
`parse_bootstrap` + `accesslog_format_parse` fuzz targets clean for the short-budget CI run (with the new
nested `json_format` seed) — **NO new fuzz target** (§3.7; confirm at state-2/3) + (e) `cargo build
--workspace --all-targets` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` /
`cargo fmt --all -- --check` / `cargo test --workspace` / `cargo deny check` all clean + (f) `REVIEW.md`
approved. `#![forbid(unsafe_code)]` holds (D-3.8). No new crate, no new dependency (D-3.2); projected NO
new `ConfigError` variant.

---

_Scope locked by **ADR-0093**. **ADR-0094 is reserved** for the §6.2 reconciliation (state-2 PLAN-write).
The §6.1 split is projected NOT to fire (**ADR-0095 reserved** for it). The state-2 PLAN-write is the next
session (`superpowers:writing-plans`), which runs the §6.2 empirical reconnaissance against live
`envoyproxy/envoy:v1.33.0` and fires ADR-0094._
