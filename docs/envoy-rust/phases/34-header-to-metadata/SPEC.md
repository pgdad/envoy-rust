# Phase 34 — `34-header-to-metadata` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0083** (the phase-34 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

**Land the request-driven dynamic-metadata emitter that consumes phase-33's infrastructure unchanged.**
Phase 33 built the per-request **dynamic-metadata store** (`dynamic_metadata` on `FilterRequest` +
`AccessLogRecord`), the `envoy.filters.http.set_metadata` filter (a STATIC-value emitter), and the
`%DYNAMIC_METADATA(namespace:key)%` access-log command-operator. This phase adds the
**`envoy.filters.http.header_to_metadata` HTTP filter** — the simplest **request-header-driven**
metadata emitter: the 12th `HttpFilterInstance` variant, decode-side, which (on `decode_headers`)
evaluates configured `request_rules` against the request headers and merges the extracted **string**
values into `req.dynamic_metadata` under a namespace/key. It **reuses the phase-33 store + the
`%DYNAMIC_METADATA%` operator UNCHANGED** — a pure-additive filter (no store change, no operator
change). The **differential is byte-exact, DETERMINISTIC, and LOCALLY observable**: the probe controls
the header value, so the metadata written (and the `%DYNAMIC_METADATA(ns:key)%` render in a custom
`log_format`) is a function only of the request the harness sends — both proxies emit a **byte-identical
access-log line** (the existing `Driver::Http1AccessLogByteExact` file scrape on a normal
request/response — no file-watch/reload trigger, NOT Linux-CI-only). `header_to_metadata` is chosen as
the pure-additive follow-up to `set_metadata` (the phase-33 SPEC §2.2 names it "the natural next pick
after phase 33") because it exercises the store/operator with a STRONGER request-driven value while
adding no new infrastructure — focusing the phase on the filter's extraction logic. See ADR-0083 for the
pick rationale and rejected alternatives (a metadata CONSUMER — jwt_authn `payload_in_metadata` / rbac
dynamic-metadata conditions — higher complexity and better served once a header-driven producer exists;
`json_format` — a self-contained Observability leaf that unlocks no filter-family follow-up; non-string
metadata Values — a type-system increment better deferred until a consumer needs it).

## §1 — Goal & differential surface

**Goal.** Add the `header_to_metadata` HTTP filter (request-header → dynamic-metadata extraction),
behaviorally equivalent to upstream Envoy v1.33.0 under the differential contract (§7.2 of
`BOOTSTRAP_PROMPT.md`) on the **Access log record** dimension (byte-exact for the deterministic
`%DYNAMIC_METADATA%` render of the header-derived string metadata value) and the **Response** dimension
(the filter is decode-side-only and inert on the response — all pre-existing fixtures unchanged).

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0042-http-header-to-metadata`** (next free number; baseline is `0001`…`0041`): an H1
  `direct_response` listener with a filter chain `[header_to_metadata, router]` and a file access logger
  whose custom `log_format` includes one or more `%DYNAMIC_METADATA(namespace:key)%` operators (reused
  verbatim from phase 33) alongside curated phase-32 deterministic operators and literals. The
  `header_to_metadata` filter is configured with a `request_rule` that extracts a request header (e.g.
  `X-Tier`) into `metadata_namespace:key` (e.g. `envoy.lb:tier`); the access-log line renders that
  header-derived value **byte-identically cross-proxy**. The fixture drives ≥2 probes including: (a) a
  request **WITH** the header present (`X-Tier: prod` → renders `prod`), and (b) a request **WITHOUT**
  the header (exercising `on_header_missing` → a static fallback value, or the absent-value `-` if no
  fallback is configured — **§6.2-VERIFY**). The exact `request_rules`/`Rule`/`KeyValuePair` wire shape,
  the default `metadata_namespace`, the `on_header_present` value-vs-static-override precedence, the
  `on_header_missing` semantics, and the byte form of the rendered value are **§6.2-VERIFY / §3
  PLAN-write calls**.
- **All 41 pre-existing fixtures `0001`–`0041` stay green simultaneously** — `header_to_metadata` is
  INERT when not in a chain (the 07.1 foundation-slice regression-equivalence property — no existing
  chain contains it), the dynamic-metadata store is UNCHANGED from phase 33 (no new field), and the
  command-operator engine + `%DYNAMIC_METADATA%` operator are byte-preserved (no operator change). This
  is the load-bearing regression proof (including `0012` default-format byte-identical and `0041`
  set_metadata byte-identical, both UNCHANGED).

**Conformance:** h2spec pass-rate ≥95% (unchanged — no HTTP/2 codec change). No new conformance suite.
Fuzz: the existing `parse_bootstrap` target's reach extends to the new `header_to_metadata` config
(no NEW fuzz target — `header_to_metadata` reuses the existing serde/`deny_unknown_fields` parse path
with no bespoke tokenizer); the `accesslog_format_parse` target is UNCHANGED (no operator change). Add a
`parse_bootstrap` config seed exercising a `header_to_metadata` filter + a `%DYNAMIC_METADATA%`-bearing
`log_format`. Whether the `header_to_metadata` config surface warrants its own dedicated fuzz target is a
**§3 PLAN-write call** (projected NOT).

## §2 — Scope (minimum-viable)

Per §6.3 (no vague deferral): every capability is either IN this phase and tested, or an explicit
deferred non-goal with its own future home. Exact dispositions marked **§6.2-VERIFY** are empirically
locked at the state-2 PLAN-write (the phase-22/23/28/29/30/31/32/33 verify-at-PLAN-write discipline);
this SPEC states the projected shape.

### §2.1 IN scope

1. **The `header_to_metadata` HTTP filter.** The 12th `HttpFilterInstance` variant
   (`crates/envoy-filter/src/header_to_metadata.rs`, new): a decode-side filter that, on
   `decode_headers`, evaluates each configured `request_rule` against the request headers and merges the
   resulting **string** value into `req.dynamic_metadata` under the rule's `metadata_namespace`→`key`,
   then returns `Decision::Continue` (it NEVER `StopAndSend`s — it is observability/routing-input
   plumbing, not a gate). Encode-side is a no-op (`Continue`). Per-rule extraction (string-only MVP):
   - **`on_header_present`** — when the configured `header` IS present in the request: write the header's
     **value** as the metadata value, OR the rule's static `value` override if configured (the
     value-vs-override precedence is **§6.2-VERIFY / §3.2**). Writes under `metadata_namespace`
     (projected default `envoy.lb` — **§6.2-VERIFY / §3.1**) → `key`.
   - **`on_header_missing`** — when the configured `header` is ABSENT: write the rule's static fallback
     `value` (which is REQUIRED for the missing branch, since there is no header value — **§6.2-VERIFY /
     §3.3**). If no `on_header_missing` is configured, nothing is written (the access-log
     `%DYNAMIC_METADATA%` then renders the absent `-`).
   Multiple `request_rules` compose (each writes its own namespace:key). Follows the `set_metadata.rs` /
   `cdn_loop.rs` add-a-decode-side-filter pattern verbatim (struct + `new` + `decode_headers` +
   inert `encode_headers` + 4 enum/dispatch wirings in `instance.rs`).
2. **The `header_to_metadata` config schema + validation.** A `HeaderToMetadataConfig { request_rules:
   Vec<Rule> }` + `Rule { header: String, on_header_present: Option<KeyValuePair>, on_header_missing:
   Option<KeyValuePair> }` + `KeyValuePair { metadata_namespace: String (serde default, projected
   `envoy.lb`), key: String, value: Option<String> }` in `crates/envoy-config/src/bootstrap.rs` (all
   `#[serde(deny_unknown_fields)]`; string-only `value` modeled as `Option<String>` — a non-string YAML
   scalar fails serde deserialization → boot-fatal in envoy-rust, a **documented stricter-than-Envoy**
   boundary [Envoy accepts non-string scalars; mirrors the phase-33 §A5 `set_metadata` value boundary],
   the §2.2 non-string-Value deferral; not differentially exercised — the fixture uses string values
   only) + a `HttpFilterTypedConfig::HeaderToMetadata(HeaderToMetadataConfig)`
   variant (`@type = type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config`,
   inserted in the existing `@type`-tagged enum) + a `validate_http_filters` arm checking
   `name == "envoy.filters.http.header_to_metadata"` and rule well-formedness. A malformed config (e.g.
   an empty `header`, an empty `key`, a rule with NEITHER `on_header_present` NOR `on_header_missing`, an
   `on_header_missing` with no `value`) is **boot-fatal** (ADR-0049 all-fatal posture) — reuse
   `UnsupportedHttpFilter` for the name mismatch; a new `ConfigError` variant (e.g.
   `HeaderToMetadataInvalidRule`) only if a rule-shape error needs one (a §3 call; the exact disposition
   set is **§6.2-VERIFY / §3.4**).
3. **Operator reuse (NO change).** The `%DYNAMIC_METADATA(namespace:key)%` command-operator
   (`crates/envoy-accesslog/src/command_operator.rs`) and the dynamic-metadata store
   (`FilterRequest`/`AccessLogRecord.dynamic_metadata`) land UNCHANGED from phase 33. This phase writes
   to the store via a new producer; it reads via the existing operator. The H1+H2 capture-before-drop
   threading (phase-33 T9/T10) is reused as-is — `header_to_metadata` writes to `req.dynamic_metadata`
   exactly like `set_metadata`, so the existing thread carries it to both record-build sites with NO new
   plumbing. (The H2 path is verified by an H2 in-process backstop, since fixture `0042` is H1-only.)
4. **Tests.** Fixture `0042` (the differential above) + all `0001`–`0041` unchanged (the
   regression-equivalence witnesses; `0012` default-format + `0041` set_metadata byte-identical) + an
   in-process backstop (the richer, deterministic complement, mirroring the phase-32/33 backstop split):
   the filter extracts a present header / applies the missing fallback / honors the static `value`
   override / composes multiple rules / threads to the record on both H1 AND H2. Plus a `parse_bootstrap`
   seed with a `header_to_metadata` filter + a `%DYNAMIC_METADATA%`-bearing `log_format`, and a
   BEHAVIOR_CONTRACT "HTTP filters" + "Access log field mapping" extension documenting the new filter,
   the header→metadata extraction, the default namespace, and the present/missing/override semantics.

### §2.2 DEFERRED non-goals (explicit; each names its future home)

- **`response_rules`** (the encode-side, response-header-driven metadata extraction) — `header_to_metadata`
  supports a symmetric `response_rules` list evaluated on `encode_headers`. The MVP is decode-side
  (`request_rules`) only; `response_rules` is a pure-additive encode-side increment in a future phase
  (it reuses the same `Rule`/`KeyValuePair` shape + the encode-side dispatch arm). Deferred because the
  request side is the higher-leverage, simpler-to-differential slice (the access-log render of a
  request-derived value is byte-exact without a response-path metadata consumer).
- **Typed (non-string) metadata values** — the `type: NUMBER | PROTOBUF_VALUE` extraction (and the
  `regex_value_rewrite` + `encode: BASE64` transforms that produce them). The MVP is string-only (the
  phase-33 store is `BTreeMap<…, String>`); these need the §2.2 (phase-33) Value-enum generalization.
  A future additive increment, shared with the phase-33 non-string-Value deferral.
- **`encode: BASE64`** — the base64 value encoding on extraction; deferred with the typed-value work.
- **`regex_value_rewrite`** — the `RegexMatchAndSubstitute` value rewrite on the extracted header value;
  a future additive increment (reuses the existing regex dep under ADR-0021).
- **The `remove: true` header mutation** — `header_to_metadata` can strip the matched header from the
  request after extraction. The MVP does NOT mutate headers (extraction is observation-only, keeping the
  upstream request byte-identical so no echo-server backend fixture is needed); `remove` is a future
  additive increment (a decode-side header mutation + an http1-echo-server backend differential).
- **Per-route `typed_per_filter_config` for `header_to_metadata`** — the route-scoped rule override (the
  cors/csrf/buffer precedent); additive via the existing phase-23 `apply_route_config` hook later.
- **Metadata consumers** — jwt_authn `payload_in_metadata`, rbac dynamic-metadata permission/principal
  conditions, ext_authz/ext_proc metadata; each reuses this store (now fed by a request-driven producer)
  in its own future phase.
- **The other Observability-family surfaces** — `json_format`/`typed_json_format`, gRPC ALS, the OTLP
  access-log sink, tracing, stats sinks, the tap filter; each its own future phase.

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)

These are decisions the state-2 PLAN-write makes after the §6.2 reconnaissance; the brainstorm
deliberately leaves them open:

1. **The `header_to_metadata` config wire shape + default namespace** — confirm the
   `request_rules: [{ header, on_header_present: { metadata_namespace, key, value, type }, on_header_missing:
   {…} }]` field names v1.33.0 accepts + round-trips through `/config_dump`, the **default
   `metadata_namespace`** when omitted (Envoy default `envoy.lb`), and which fields are required vs
   optional. **The MVP models ONLY the `type`-absent / `type: STRING` path** (a non-default
   `type: NUMBER | PROTOBUF_VALUE` is the §2.2 non-string-Value deferral → projected config-fatal in
   envoy-rust, stricter than Envoy, mirroring the phase-33 §A2 nested-path treatment); record whether
   `type` is omittable / defaults to `STRING` and the exact disposition of a non-default `type`.
2. **The `on_header_present` value precedence** — when both the header is present AND a static `value`
   is configured, does Envoy write the header value or the static `value`? Record the exact precedence
   and the byte form of the written value (raw string).
3. **The `on_header_missing` semantics** — does `on_header_missing` require a `value` (no header to read
   from)? Is it applied only when the header is fully absent (vs present-but-empty)? Record the
   present-but-empty-header disposition.
4. **The config-validity disposition** — `header_to_metadata` with an empty `header` / empty `key` / a
   rule with no action / an `on_header_missing` with no `value` / a non-string `value` — boot-fatal
   (ADR-0049, projected) vs accept-and-degrade. Whether a new `ConfigError` variant is needed beyond
   `UnsupportedHttpFilter`.
5. **The fixture-0042 shape** — `direct_response` (fully upstream-independent, byte-exact line) vs a real
   `http1-echo-server` backend (projected `direct_response`, matching 0041); the exact `log_format`
   string (which deterministic phase-32 operators to combine with `%DYNAMIC_METADATA%`); the configured
   header/namespace/key/fallback; the probe set (header-present + header-missing, ≥2 probes); whether any
   timing operator is included (asserted by the existing allow-list rules, not `Exact`).
6. **The harness** — reuse `Driver::Http1AccessLogByteExact` +
   `assert_access_log_lines_byte_identical` from phase 32/33 verbatim (projected — the deterministic-only
   line needs no new comparator), confirming the header-derived render slots into the existing whole-line
   byte-exact path. (The probe's ability to set an arbitrary request header — needed to drive the
   present/missing probes — is ALREADY a confirmed harness capability, not a §6.2 item; see §4.)
7. **The fuzz disposition** — confirm the existing `parse_bootstrap` target covers the new
   `header_to_metadata` config (projected yes — same `parse_bootstrap` entry point) and decide whether a
   dedicated target is warranted (projected NO — reuses the serde parse path) vs a `parse_bootstrap` seed
   only. The `accesslog_format_parse` target is UNCHANGED (no operator change).
8. **The §6.1 split decision** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)

- **The phase-33 dynamic-metadata store** (`crates/envoy-filter/src/types.rs` `FilterRequest.dynamic_metadata`
  + `crates/envoy-accesslog/src/record.rs` `AccessLogRecord.dynamic_metadata`, the string-only
  `BTreeMap<String, BTreeMap<String, String>>`) — UNCHANGED. `header_to_metadata` writes to
  `req.dynamic_metadata` exactly as `set_metadata` does.
- **The phase-33 `%DYNAMIC_METADATA(namespace:key)%` operator** (`crates/envoy-accesslog/src/command_operator.rs`:
  `Op::DynamicMetadata`, the parse arm, `render_op`) — UNCHANGED. Reads the store the new filter writes.
- **The phase-33 H1+H2 capture-before-drop threading** (`crates/envoy-http1/src/hcm.rs` ~792→~1211 +
  `crates/envoy-http2/src/hcm.rs` ~494→`finalize_h2_stream`) — UNCHANGED. Carries any
  `req.dynamic_metadata` (regardless of which filter wrote it) to both record-build sites.
- **The HTTP filter framework** (`crates/envoy-filter/`: the `HttpFilterInstance` enum [11 production
  variants, `set_metadata` the most recent], `FilterRequest`/`FilterResponse`/`Decision`, the
  `decode_headers`/`encode_headers` pipeline iteration, the `build` / dispatch match arms) —
  `header_to_metadata` is the 12th variant following the `set_metadata`/`cdn_loop`
  add-a-decode-side-filter pattern verbatim (struct + `new` + `decode_headers` + 4 enum/dispatch wirings).
- **The HTTP-filter config plumbing** (`crates/envoy-config/src/bootstrap.rs`: the `HttpFilterTypedConfig`
  `@type`-tagged enum [11 variants], the `validate_http_filters` per-variant arm pattern, the
  `ConfigError` enum + the phase-33 `SetMetadataConfig` precedent) — `header_to_metadata` adds one enum
  variant + one validator arm + the config structs following the `set_metadata` precedent.
- **The differential harness access-log path** — `Driver::Http1AccessLogByteExact`, the
  `AccessLogByteExactProbe` shape, `assert_access_log_lines_byte_identical` (phase-32/33), and the
  `tests/fixtures/0041-http-set-metadata-dynamic-metadata/` structure — the template for fixture `0042`;
  no new comparator projected. **CONFIRMED (a repo capability, not a §6.2 item):** `AccessLogByteExactProbe`
  already carries an `extra_headers: Vec<(String, String)>` field wired through `drive_http1`/`drive_http2`
  into the actual request — so the header-present / header-missing probes (a request WITH vs WITHOUT
  `X-Tier`) are expressible with the existing probe shape, no harness change.
- **The `parse_bootstrap` fuzz corpus + its `ci.yml` step** + the BEHAVIOR_CONTRACT "HTTP filters" /
  "Access log field mapping" sections — extend each; no new fuzz target projected.

## §5 — Behavioral contract notes

- **Determinism / byte-exactness (the strong target):** the metadata value `header_to_metadata` writes
  is derived ONLY from the request the harness controls (the header value, or a static config fallback),
  so `%DYNAMIC_METADATA(ns:key)%` renders a value that is a function ONLY of the (fixed) probe request +
  static config — identical on both proxies → the whole access-log line is byte-identical cross-proxy
  (no per-side host-address or clock dependence). The header-missing path renders the static fallback (or
  `-`) identically on both. **The header-present + header-missing probe PAIR in the fixture is the
  cross-proxy guard:** the present probe must resolve the EXTRACTED header value via
  `record.dynamic_metadata.get(ns)?.get(key)`, and the missing probe must resolve the fallback (or `-`)
  from the SAME store path — a faulty implementation that mishandles extraction or the missing branch
  fails one of the two. The richer extraction logic (value-vs-override, multi-rule, the H2 threading
  site) lives in the in-process backstop; the cross-proxy fixture proves Envoy's EXACT extracted-value
  byte-form + the store round-trip.
- **Regression-equivalence (the load-bearing proof):** the dynamic-metadata store + operator are
  UNCHANGED from phase 33; `header_to_metadata` is inert outside a chain that contains it — so all 41
  existing fixtures (incl. `0012` default-format + `0041` set_metadata byte-identical) stay green
  unchanged.
- **Filter discipline:** `header_to_metadata` is decode-side, `Continue`-only (observability/routing-input
  plumbing, never a request gate) and encode-side inert — it cannot change any response, and (MVP, no
  `remove`) it does not mutate the request headers either, so no response/request-dimension fixture
  regresses.
- **Config validity:** a malformed `header_to_metadata` config is startup-fatal where §6.2 shows Envoy
  rejects (ADR-0049 all-fatal; no reload path this phase).
- **Differential locality:** the access-log line is observable WITHOUT a file-watch/reload trigger (the
  file-sink scrape on a normal request/response) → the fixture-`0042` differential runs and is
  authoritative on this Docker-Desktop host (NOT Linux-CI-only, unlike phases 26/27).

## §6 — Process

### §6.1 — Split projection (§6.1 gate)

A split is projected **NOT to fire**. The surface is one trivial decode-side filter (the 12th variant,
following the `set_metadata` add pattern) + one config schema/variant/validator + one fixture + the
H1+H2 backstops — and it ADDS NO infrastructure (the store + operator + threading are reused unchanged
from phase 33). Estimate ~600–900 LoC / ~7–9 tasks, smaller than phase 33 (no store/operator/threading
work), comparable to `cdn_loop`/`csrf`. Well under the ~1500-LoC / ~25-task gate. **ADR-0084 is reserved**
for the split (fires only if the `request_rules`/`KeyValuePair` shape or the value-extraction semantics
prove far gnarlier than projected — e.g. §6.2 reveals the string-only MVP cannot produce a byte-exact
differential without a typed Value). The natural seam, if forced, is `34.1` (the filter + config +
validator + variant — a foundation slice, NO new fixture, backstop-only, proven by all 41 existing
fixtures staying green) / `34.2` (fixture `0042` + the BEHAVIOR_CONTRACT extension + the fuzz seed +
close).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)

Like phases 22/23/28/29/30/31/32/33 (and unlike phases 26/27), this phase's behavior is **locally
observable** (no reload trigger). At the state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0` with an
H1 listener + a `header_to_metadata` filter + a file access logger whose `log_format` uses
`%DYNAMIC_METADATA(...)%`, and:
1. RECORD the **`header_to_metadata` config wire shape** v1.33.0 accepts (`request_rules` / `Rule` /
   `on_header_present` / `on_header_missing` / `KeyValuePair` field names; the **default
   `metadata_namespace`** when omitted; the `type`/`encode`/`value` field handling for the string path),
   and how it round-trips through `/config_dump`.
2. RECORD the **extraction semantics + EXACT bytes**: for a present header (`X-Tier: prod`) the value
   written + rendered (raw `prod`); the `on_header_present` static-`value`-override precedence; the
   `on_header_missing` fallback render; the present-but-empty-header disposition; and confirm the
   access-log line for a fixed request is **byte-identical** between a hand-rolled replica and live Envoy
   across ≥2 probes (header present + header missing).
3. RECORD the **config-validity disposition** (empty `header`/`key`, a no-action rule, an
   `on_header_missing` with no `value`, a non-string `value` — boot-fatal vs accepted).
4. Decide STRONG (cross-proxy byte-identical for the header-derived render — expected); record a fallback
   only if the value byte-form proves non-portable.
**ADR-0084 FIRES** at the PLAN-write if any of these materially diverge from this SPEC's projection
(notably the config wire shape, the default namespace, the value-vs-override precedence, the
on_header_missing semantics, or the config-validity disposition). `PLAN.md` lands with the
empirically-locked facts inline (no `[§6.2-PENDING]` projections — the verify-at-PLAN-write discipline).

### §6.3 — Anti-deferral

No vague TODOs. Every §2.1 item is implemented + tested this phase; every deferral is a §2.2 named
non-goal with a future home. The filter, the config + validator, the fixture, and the backstop are real
and differentially exercised — no stubs. The regression equivalence is proven by all 41 existing fixtures
(incl. `0012` + `0041`) staying green unchanged.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)

(a) fixture `0042` green (cross-proxy byte-identical access-log line; header-present extracted value +
header-missing fallback/`-`) + (b) all of `0001`–`0041` green (incl. `0012` default-format + `0041`
set_metadata UNCHANGED — the regression-equivalence witnesses) + (c) h2spec ≥95% (unchanged — no HTTP/2
codec change) + (d) the existing `parse_bootstrap` (+ `accesslog_format_parse`, unchanged) fuzz targets
clean for the short-budget CI run (with the new `header_to_metadata` seed) — **NO new fuzz target**
(§3.7; confirm at state-2/3) + (e) `cargo build --workspace --all-targets` / `cargo clippy --workspace
--all-targets --all-features -- -D warnings` / `cargo fmt --all -- --check` / `cargo test --workspace` /
`cargo deny check` all clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds (D-3.8). No new
crate, no new dependency (D-3.2).

---

_Scope locked by **ADR-0083**. ADR-0084 reserved (§6.2 reconciliation). The §6.1 split is projected NOT to
fire. The state-2 PLAN-write is the next session (`superpowers:writing-plans`), which runs the §6.2
empirical reconnaissance and fires ADR-0084._
