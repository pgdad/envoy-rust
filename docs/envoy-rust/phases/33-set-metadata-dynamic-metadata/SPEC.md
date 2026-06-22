# Phase 33 — `33-set-metadata-dynamic-metadata` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0080** (the phase-33 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

**Land the dynamic-metadata critical-path unlock that phase 32 enabled.** Phase 32 generalized the
access-log emitter into a configurable command-operator engine (`crates/envoy-accesslog/src/command_operator.rs`)
and explicitly deferred the `%DYNAMIC_METADATA(namespace:key)%` operator because **no filter emits
dynamic metadata yet** (ADR-0078 §2.2). This phase lands the smallest end-to-end loop that makes
dynamic metadata real AND differentially observable: (1) a **per-request dynamic-metadata store**
threaded through the HTTP filter pipeline and copied into the `AccessLogRecord`; (2) the
**`envoy.filters.http.set_metadata` HTTP filter** — the simplest possible metadata emitter, which
merges an operator-configured static value into the request's dynamic metadata under a namespace
(the 11th `HttpFilterInstance` variant, decode-side, never short-circuits); and (3) the
**`%DYNAMIC_METADATA(namespace:key)%` access-log command-operator** that resolves and renders that
metadata into a custom `log_format`. The **differential is byte-exact, DETERMINISTIC, and LOCALLY
observable**: because the written value is static config, both proxies render a **byte-identical
access-log line** (an access-log file scrape on a normal request/response via the existing
`Driver::Http1AccessLogByteExact` — no file-watch/reload trigger, NOT Linux-CI-only). `set_metadata`
is chosen over `header_to_metadata` because its written value is a config literal → the trivial-est
filter, focusing the phase on the **reusable store + operator infrastructure** rather than filter
logic; `header_to_metadata` (request-header-driven metadata) is the explicit pure-additive follow-up
that reuses this store + operator unchanged. See ADR-0080 for the pick rationale and rejected
alternatives (`json_format` — a self-contained leaf that unlocks no further family; more deterministic
operators — incremental, lower leverage; the remaining LB policies — non-deterministic or HC-dependent;
a config-hardening phase — low differential leverage).

## §1 — Goal & differential surface

**Goal.** Add a per-request dynamic-metadata store, the `set_metadata` HTTP filter, and the
`%DYNAMIC_METADATA(namespace:key)%` access-log operator, behaviorally equivalent to upstream Envoy
v1.33.0 under the differential contract (§7.2 of `BOOTSTRAP_PROMPT.md`) on the **Access log record**
dimension (byte-exact for the deterministic `%DYNAMIC_METADATA%` render of a static-string metadata
value) and the **Response** dimension (the filter is decode-side-only and inert on the response — all
pre-existing fixtures unchanged).

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0041-http-set-metadata-dynamic-metadata`** (next free number; baseline is `0001`…`0040`):
  an H1 listener with a filter chain `[set_metadata, router]` and a file access logger whose custom
  `log_format` includes one or more `%DYNAMIC_METADATA(namespace:key)%` operators alongside curated
  phase-32 deterministic operators and literals. The `set_metadata` filter writes a configured
  static value (e.g. `%DYNAMIC_METADATA(envoy.test:tier)%` resolving to a configured `tier=prod`)
  under a namespace; the access-log line renders that value **byte-identically cross-proxy**. The
  fixture drives ≥2 probes including: (a) a present key (renders the configured value), and (b) an
  **absent** namespace/key reference (`%DYNAMIC_METADATA(envoy.test:missing)%` and/or
  `%DYNAMIC_METADATA(envoy.absent:k)%` → the absent-value render, projected `-`, **§6.2-VERIFY**).
  The exact `log_format` string, the operator arg-grammar separator, the byte form of a resolved
  string value (raw vs JSON-quoted), and the route shape (`direct_response` vs a real
  `http1-echo-server` backend) are **§6.2-VERIFY / §3 PLAN-write calls**.
- **All 40 pre-existing fixtures `0001`–`0040` stay green simultaneously** — `set_metadata` is INERT
  when not in a chain (the 07.1 foundation-slice regression-equivalence property — no existing chain
  contains it), the dynamic-metadata store is an additive empty-by-default field on `FilterRequest`
  and `AccessLogRecord` (no existing `log_format` references `%DYNAMIC_METADATA%`), and the
  command-operator engine is byte-preserved for every format that does not use the new operator
  (including the default format → **fixture `0012` stays byte-identical, UNCHANGED**). This is the
  load-bearing regression proof.

**Conformance:** h2spec pass-rate ≥95% (unchanged — no HTTP/2 codec change). No new conformance
suite. Fuzz: extend the existing `accesslog_format_parse` target's reach (the parser now accepts the
new `%DYNAMIC_METADATA(...)%` grammar — no NEW fuzz target required, the existing parser fuzzer
covers it) + a `parse_bootstrap` config seed exercising a `set_metadata` filter + a
`%DYNAMIC_METADATA%`-bearing `log_format`. Whether the `set_metadata` config surface warrants its own
dedicated fuzz target is a **§3 PLAN-write call** (projected NOT — `set_metadata` reuses the existing
serde/`deny_unknown_fields` parse path with no bespoke tokenizer).

## §2 — Scope (minimum-viable)

Per §6.3 (no vague deferral): every capability is either IN this phase and tested, or an explicit
deferred non-goal with its own future home. Exact dispositions marked **§6.2-VERIFY** are empirically
locked at the state-2 PLAN-write (the phase-22/23/28/29/30/31/32 verify-at-PLAN-write discipline);
this SPEC states the projected shape.

### §2.1 IN scope

1. **The per-request dynamic-metadata store.** A namespace→key→value map carried through the HTTP
   filter pipeline. Projected representation (a **§3 PLAN-write call**): a string-only
   `BTreeMap<String, BTreeMap<String, String>>` (outer key = metadata namespace, inner = key → string
   value) — a plain std type needing **no new crate and no shared Value enum** (a non-string Value
   enum is §2.2-deferred). Added as an additive field `dynamic_metadata` on `FilterRequest`
   (`crates/envoy-filter/src/types.rs`; default-empty so all existing pipeline call sites are
   unaffected) and as a sibling field on `AccessLogRecord` (`crates/envoy-accesslog/src/record.rs`).
   **The H1 and H2 HCMs have SEPARATE, independent record-build sites — H2 does NOT inherit the
   record-build from H1.** The H1 HCM copies the pipeline's `FilterRequest.dynamic_metadata` into the
   record at the H1 record-build site (`crates/envoy-http1/src/hcm.rs` ~1189). The H2 HCM has its OWN
   record-build site (`crates/envoy-http2/src/hcm.rs` ~888) and currently CONSUMES its `FilterRequest`
   (`filter_req`, built ~475) at `decode_headers` (~481), writing back only `method`/`path`/`headers`/
   `body` (~485-488) and DROPPING the rest — so `filter_req.dynamic_metadata` would be lost on the H2
   access-log path. The phase MUST therefore add a SECOND, symmetric metadata-threading site on H2:
   capture `filter_req.dynamic_metadata` before `filter_req` is dropped (~488) and populate the
   line-~888 record-build from it. This H2 threading is an explicit IN-scope task (an additive,
   surgical capture-and-forward, not a `build_response` change). Because fixture `0041` drives the
   H1-only `Driver::Http1AccessLogByteExact`, the H2 metadata-threading is verified THIS phase by an
   **H2 in-process backstop assertion** (decode a `set_metadata` chain on the H2 HCM, assert the
   built `AccessLogRecord.dynamic_metadata` carries the written value) — NOT deferred (no §6.3 gap).
2. **The `set_metadata` HTTP filter.** The 11th `HttpFilterInstance` variant
   (`crates/envoy-filter/`): a decode-side filter that, on `decode_headers`, merges its
   operator-configured static value(s) into `req.dynamic_metadata` under the configured namespace and
   returns `Decision::Continue` (it NEVER `StopAndSend`s — it is observability plumbing, not a gate).
   Encode-side is a no-op (`Continue`). Minimum-viable value shape: a flat map of string keys → string
   values under one namespace per configured entry (the exact `set_metadata` config wire shape —
   the older `metadata: [{ metadata_namespace, value: <Struct> }]` vs the newer
   `metadata: [{ key, value, allow_overwrite }]` form — is **§6.2-VERIFY / §3.1**).
3. **The `set_metadata` config schema + validation.** A `SetMetadataConfig` struct in
   `crates/envoy-config/src/bootstrap.rs` + a `HttpFilterTypedConfig::SetMetadata(SetMetadataConfig)`
   variant (`@type = type.googleapis.com/envoy.extensions.filters.http.set_metadata.v3.SetMetadata`,
   inserted in the existing sorted enum) + a `validate_http_filters` arm checking
   `name == "envoy.filters.http.set_metadata"` and the entry well-formedness. A malformed/unsupported
   config (e.g. an empty namespace, or — projected — a non-string metadata value while the string-only
   MVP holds) is disposed per **§6.2-VERIFY** — projected **boot-fatal** (ADR-0049 all-fatal posture)
   via a new or reused `ConfigError` variant (`crates/envoy-config/src/lib.rs`; reuse
   `UnsupportedHttpFilter` for the name mismatch; a new variant only if a metadata-shape error needs
   one — a §3 call).
4. **The `%DYNAMIC_METADATA(namespace:key)%` access-log command-operator.** A new
   `Op::DynamicMetadata { namespace: String, key: String, truncate: Option<usize> }` variant in
   `crates/envoy-accesslog/src/command_operator.rs`, parsed from the `%DYNAMIC_METADATA(ARG)%` grammar
   (the ARG separator and whether a deeper nested path `ns:key:sub…` is accepted are **§6.2-VERIFY /
   §3.2**; MVP supports the single-level `namespace:key`), composing with the existing `:N`-truncation
   suffix. It resolves `record.dynamic_metadata.get(namespace)?.get(key)` and renders the string value
   (the exact byte form of a resolved string value — **raw `prod` vs JSON-quoted `"prod"`** — is the
   key **§6.2-VERIFY / §3.3** risk); an absent namespace OR key renders the absent-value sentinel
   (projected `-`, **§6.2-VERIFY**). The `:N`-truncation suffix composes unconditionally (the
   `floor_char_boundary` machinery is reused as-is; the exact truncation byte-semantics on the
   resolved value are **§6.2-VERIFY / §3.2**, hence the `truncate: Option<usize>` struct field).
   Wired into `render_op` + the parse keyword table + the operator support matrix.
5. **Tests.** Fixture `0041` (the differential above) + all `0001`–`0040` unchanged (the
   regression-equivalence witnesses; `0012` is the byte-identical default-format witness) + an
   in-process backstop (the richer, deterministic complement, mirroring the phase-32 backstop split):
   the filter writes/overwrites metadata across namespaces/keys; the store threads to the record; the
   `%DYNAMIC_METADATA%` operator resolves present/absent/empty paths + the `:N`-truncation
   interaction; the parser accepts/round-trips the new operator and rejects malformed forms. Plus a
   `parse_bootstrap` seed with a `set_metadata` filter + a `%DYNAMIC_METADATA%`-bearing `log_format`,
   and a BEHAVIOR_CONTRACT "Access log field mapping" extension documenting the new operator, the
   metadata namespace/key resolution, and the string-value byte form + absent-value rendering.

### §2.2 DEFERRED non-goals (explicit; each names its future home)

- **The `header_to_metadata` HTTP filter** (`envoy.filters.http.header_to_metadata`) — the
  request-header-driven metadata emitter (rules / `on_header_present` / `on_header_missing` / value
  extraction + type conversion). It reuses THIS phase's dynamic-metadata store + `%DYNAMIC_METADATA%`
  operator UNCHANGED (a pure-additive `HttpFilterInstance` variant) and yields a stronger
  request-driven differential; its own future HTTP-filter-family phase. **This is the natural next
  pick after phase 33.**
- **Non-string metadata values** — number / bool / list / nested-struct metadata Values and the
  JSON-composite rendering of `%DYNAMIC_METADATA%` when the resolved value is non-scalar. The MVP
  store is string-only (`BTreeMap<…, String>`); the type generalizes to a `Value` enum when a
  consumer needs it. A future additive increment.
- **Nested-path metadata resolution** — `%DYNAMIC_METADATA(ns:key:sub:sub)%` deeper than the
  single-level `namespace:key` (needs the non-string nested-struct Value above). Additive once the
  Value enum lands; the MVP accepts only `namespace:key` (deeper paths § 6.2-disposed — projected
  config-fatal or absent-`-`).
- **`%FILTER_STATE(key)%`** — no filter writes filter-state (a distinct per-request store from dynamic
  metadata) today; a future operator + a filter-state store, paralleling this phase.
- **`typed_metadata`** — the typed (proto-`Any`) sibling of dynamic metadata; `set_metadata`'s
  `typed_metadata` field and any typed-metadata access-log surface are deferred.
- **Per-route `typed_per_filter_config` for `set_metadata`** — the route-scoped override (the
  cors/csrf/buffer precedent); additive via the existing phase-23 `apply_route_config` hook later.
- **`set_metadata` `allow_overwrite` advanced semantics** — if §6.2 shows the overwrite/merge rules
  are non-trivial, the MVP fixes a single deterministic disposition (projected last-writer-wins within
  a chain) and defers configurable overwrite nuance.
- **Metadata consumers other than the access log** — jwt_authn `payload_in_metadata`, rbac
  dynamic-metadata permission/principal conditions, ext_authz/ext_proc metadata — each reuses this
  store in its own future phase.
- **The other Observability-family surfaces** — `json_format`/`typed_json_format`, gRPC ALS, the OTLP
  access-log sink, OTel/Zipkin/Jaeger/Datadog/XRay tracing, stats sinks, the tap filter; each its own
  future phase.

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)

These are decisions the state-2 PLAN-write makes after the §6.2 reconnaissance; the brainstorm
deliberately leaves them open:

1. **The `set_metadata` config wire shape** — the older `metadata: [{ metadata_namespace, value:
   <Struct> }]` vs the newer `metadata: [{ key, value, allow_overwrite }]` form (which does Envoy
   v1.33.0 accept + round-trip through `/config_dump`; which to model first; the namespace under which
   the written value lands so `%DYNAMIC_METADATA(namespace:key)%` reads it back).
2. **The `%DYNAMIC_METADATA(ARG)%` arg grammar** — the path separator (Envoy uses `:`-separated
   `FILTER_NAMESPACE:KEY…`), whether a deeper nested path is accepted in v1.33.0, and the
   `:N`-truncation composition (truncation applies to the resolved string value).
3. **The byte form of a resolved metadata value (THE key differential risk)** — does
   `%DYNAMIC_METADATA(ns:key)%` render a string value as the raw `prod` or as the JSON-serialized
   `"prod"` (with quotes)? Record the EXACT bytes for a string value, and the absent-namespace /
   absent-key / empty-struct rendering (`-` vs empty vs `{}`).
4. **The config-validity disposition** — `set_metadata` with an empty/missing namespace or a
   non-string value (vs the string-only MVP); a `%DYNAMIC_METADATA%` with a deeper-than-MVP path —
   boot-fatal (ADR-0049, projected) vs accept-and-degrade. Whether a new `ConfigError` variant is
   needed beyond `UnsupportedHttpFilter` / `InvalidAccessLogFormat`.
5. **The store representation** — `BTreeMap<String, BTreeMap<String, String>>` (projected, string-only)
   vs a minimal Value enum from the start; where the type is declared (plain std `BTreeMap` declared
   independently on `FilterRequest` + `AccessLogRecord`, NO shared crate / NO shared Value type) to
   avoid a dependency cycle between `envoy-filter` and `envoy-accesslog`. The cycle is structurally
   avoided because the **HCM crates (`envoy-http1`/`envoy-http2`) are the SOLE copy site** — they
   already depend on BOTH `envoy-filter` (constructing `FilterRequest`) and `envoy-accesslog`
   (constructing `AccessLogRecord`), so the duplicate plain-`BTreeMap` declaration needs no edge
   between the two leaf crates.
6. **The fixture-0041 shape** — `direct_response` (fully upstream-independent, byte-exact line) vs a
   real `http1-echo-server` backend; the exact `log_format` string (which deterministic phase-32
   operators to combine with `%DYNAMIC_METADATA%`); the configured namespace/key/value; the probe set
   (present-key + absent-key, ≥2 probes); whether any timing operator is included (asserted by the
   existing allow-list rules, not `Exact`).
7. **The harness** — reuse `Driver::Http1AccessLogByteExact` + `assert_access_log_lines_byte_identical`
   from phase 32 verbatim (projected — the deterministic-only line needs no new comparator), confirming
   the new operator's render slots into the existing whole-line byte-exact path.
8. **The fuzz disposition** — confirm the existing `accesslog_format_parse` target covers the new
   `%DYNAMIC_METADATA(...)%` grammar (projected yes — same `parse_format` entry point) and decide
   whether `set_metadata`'s config surface warrants a dedicated target (projected NO — reuses the
   serde parse path) vs a `parse_bootstrap` seed only.
9. **The §6.1 split decision** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)

- **The phase-32 command-operator engine** (`crates/envoy-accesslog/src/command_operator.rs`:
  `parse_format`, the `Op` enum, `CompiledFormat::render`, `render_op`, the `REQ_ALLOW_LIST` /
  `RESP_ALLOW_LIST`, the `FormatParseError` taxonomy, the `floor_char_boundary` `:N` truncation) — the
  new `Op::DynamicMetadata` slots additively into the enum + the parse keyword table + `render_op`;
  the `:N`-truncation machinery is reused verbatim.
- **The `AccessLogRecord`** (`crates/envoy-accesslog/src/record.rs`, 15 fields) — gains ONE additive
  `dynamic_metadata` field populated from the pipeline's `FilterRequest`. **NOTE: there are TWO
  independent record-build sites** — H1 at `crates/envoy-http1/src/hcm.rs` ~1189 and H2 at
  `crates/envoy-http2/src/hcm.rs` ~888 (H2 does NOT inherit the record-build from H1; it builds its
  own record from a `FilterRequest` it currently drops after `decode_headers`). BOTH sites must be
  threaded — see §2.1 item 1.
- **The HTTP filter framework** (`crates/envoy-filter/`: the `HttpFilterInstance` enum with its 10
  production variants, `FilterRequest`/`FilterResponse`/`Decision`, the `decode_headers`/`encode_headers`
  pipeline iteration, the `build` / dispatch match arms) — `set_metadata` is the 11th variant following
  the `header_mutation`/`cdn_loop` add-a-decode-side-filter pattern verbatim (struct + `build_from_config`
  + `decode_headers` + 3 enum/dispatch wirings). `FilterRequest` gains the additive `dynamic_metadata`
  field.
- **The HTTP-filter config plumbing** (`crates/envoy-config/src/bootstrap.rs`: the
  `HttpFilterTypedConfig` `@type`-tagged enum [10 variants, cdn_loop the most recent], the
  `validate_http_filters` per-variant arm pattern, the `ConfigError` enum) — `set_metadata` adds one
  enum variant + one validator arm following the cdn_loop precedent.
- **The differential harness access-log path** — `Driver::Http1AccessLogByteExact`, the
  `AccessLogByteExactProbe` shape, `assert_access_log_lines_byte_identical` (all phase-32), and the
  `tests/fixtures/0040-accesslog-command-operators/` structure — the template for fixture `0041`; no
  new comparator projected.
- **The `parse_bootstrap` + `accesslog_format_parse` fuzz corpora + their `ci.yml` steps** + the
  BEHAVIOR_CONTRACT "Access log field mapping" section (the phase-32 grammar/classification tables) —
  extend each; no new fuzz target projected.

## §5 — Behavioral contract notes

- **Determinism / byte-exactness (the strong target):** the `set_metadata`-written value is a static
  config literal, so `%DYNAMIC_METADATA(ns:key)%` renders a value that is a function ONLY of static
  config — identical on both proxies → the whole access-log line is byte-identical cross-proxy (no
  per-side host-address or clock dependence). The absent-key path renders the absent sentinel
  identically on both. **The present-key + absent-key probe PAIR in the fixture is the cross-proxy
  guard against a non-store-backed (echo-the-config-literal) implementation:** the present probe must
  resolve a STORED value via `record.dynamic_metadata.get(ns)?.get(key)`, and the absent probe must
  resolve `-` from the SAME resolution path — a faulty implementation that hardcodes/echoes the
  configured value without round-tripping through the store would render the configured value (not
  `-`) on the absent probe and fail. The richer dynamic-flow proof (overwrite/multi-namespace logic +
  the H2 threading site) lives in the in-process backstop; the cross-proxy fixture proves Envoy's
  EXACT value byte-form + the resolution path.
- **Regression-equivalence (the load-bearing proof):** the dynamic-metadata store is an additive
  empty-default field; `set_metadata` is inert outside a chain that contains it; the command-operator
  engine is byte-preserved for every format not using `%DYNAMIC_METADATA%` — so all 40 existing
  fixtures (incl. `0012` default-format byte-identical) stay green unchanged.
- **Filter discipline:** `set_metadata` is decode-side, `Continue`-only (observability plumbing, never
  a request gate) and encode-side inert — it cannot change any response, so no response-dimension
  fixture regresses.
- **Config validity:** a malformed `set_metadata` config or a malformed `%DYNAMIC_METADATA%` operator
  is startup-fatal where §6.2 shows Envoy rejects (ADR-0049 all-fatal; no reload path this phase).
- **Differential locality:** the access-log line is observable WITHOUT a file-watch/reload trigger
  (the file-sink scrape on a normal request/response) → the fixture-`0041` differential runs and is
  authoritative on this Docker-Desktop host (NOT Linux-CI-only, unlike phases 26/27).

## §6 — Process

### §6.1 — Split projection (§6.1 gate)

A split is projected **NOT to fire**. The surface is one additive store field (threaded at TWO
independent record-build sites — H1 `hcm.rs` ~1189 AND H2 `hcm.rs` ~888, each a surgical
capture-and-forward) + one trivial decode-side filter (the 11th variant, following the cdn_loop add
pattern) + one config schema/variant/validator + one new command-operator + one fixture + the H1+H2
backstops; estimate ~900–1200 LoC / ~10–12 tasks (the dual threading site adds one task vs the
single-site projection), comparable to `cdn_loop`/`ring_hash`, still well under the ~1500-LoC /
~25-task gate. **ADR-0082 is reserved** for the split (fires only if the metadata-value byte-form / store
threading proves far gnarlier than projected — e.g. §6.2 reveals a non-scalar Value is unavoidable for
a byte-exact differential). The natural seam, if forced, is `33.1` (the dynamic-metadata store +
`FilterRequest`/`AccessLogRecord` field plumbing + the `set_metadata` filter + config + validator — a
foundation slice, NO new fixture, backstop-only, proven by all 40 existing fixtures staying green) /
`33.2` (the `%DYNAMIC_METADATA%` operator + fixture `0041` + the BEHAVIOR_CONTRACT extension + the
fuzz/seed + close).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)

Like phases 22/23/28/29/30/31/32 (and unlike phases 26/27), this phase's behavior is **locally
observable** (no reload trigger). At the state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0` with
an H1 listener + a `set_metadata` filter + a file access logger whose `log_format` uses
`%DYNAMIC_METADATA(...)%`, and:
1. RECORD the **`set_metadata` config wire shape** v1.33.0 accepts (`metadata_namespace`+`value` vs
   `key`+`value`+`allow_overwrite`), the namespace the value lands under, and how it round-trips
   through `/config_dump`.
2. RECORD the **`%DYNAMIC_METADATA(ARG)%` arg grammar** (the `:` path separator, nested-path
   acceptance, `:N`-truncation composition) and the **EXACT bytes** the operator renders for a string
   value (raw vs JSON-quoted) and for an absent namespace/key/empty value (`-` vs empty vs `{}`).
3. RECORD the **config-validity disposition** (a malformed `set_metadata` / a malformed or
   deeper-than-MVP `%DYNAMIC_METADATA%` — boot-fatal vs accepted), and confirm the access-log line for
   a fixed request is **byte-identical** between a hand-rolled replica and live Envoy across ≥2 probes
   (present key + absent key).
4. Decide STRONG (cross-proxy byte-identical for the static-value render — expected); record a
   fallback only if the value byte-form proves non-portable.
**ADR-0081 FIRES** at the PLAN-write if any of these materially diverge from this SPEC's projection
(notably the config wire shape, the value byte-form, the absent-value rendering, the arg-grammar
separator, or the config-validity disposition). `PLAN.md` lands with the empirically-locked facts
inline (no `[§6.2-PENDING]` projections — the verify-at-PLAN-write discipline).

### §6.3 — Anti-deferral

No vague TODOs. Every §2.1 item is implemented + tested this phase; every deferral is a §2.2 named
non-goal with a future home. The store, the `set_metadata` filter, the `%DYNAMIC_METADATA%` operator,
the fixture, and the backstop are real and differentially exercised — no stubs. The regression
equivalence is proven by all 40 existing fixtures (incl. `0012`) staying green unchanged.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)

(a) fixture `0041` green + (b) all of `0001`–`0040` green (incl. `0012` UNCHANGED — the
regression-equivalence witness) + (c) h2spec ≥95% + (d) the existing `accesslog_format_parse` +
`parse_bootstrap` fuzz targets (with the new-operator / `set_metadata` seeds) clean for the
short-budget CI run [confirm at state-2/3 whether any NEW fuzz target is warranted — projected NOT] +
(e) `cargo build --workspace --all-targets` / `cargo clippy --workspace --all-targets --all-features
-- -D warnings` / `cargo fmt --all -- --check` / `cargo test --workspace` / `cargo deny check` all
clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds (D-3.8).

---

_Scope locked by **ADR-0080**. ADR-0081 reserved (§6.2 reconciliation), ADR-0082 reserved (§6.1
split). The state-2 PLAN-write is the next session (`superpowers:writing-plans`)._
