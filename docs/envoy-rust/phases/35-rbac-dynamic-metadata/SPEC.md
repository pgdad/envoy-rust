# Phase 35 — `35-rbac-dynamic-metadata` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0085** (the phase-35 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

**Land the FIRST dynamic-metadata CONSUMER — closing the produce→consume loop the phase-33/34 metadata
arc teed up.** Phases 32–34 built the metadata *producer* side: the access-log command-operator engine
(32), the per-request **dynamic-metadata store** + the `envoy.filters.http.set_metadata` static emitter +
the `%DYNAMIC_METADATA(namespace:key)%` operator (33), and the request-header-driven
`envoy.filters.http.header_to_metadata` emitter (34). Every metadata sink so far is the *access log*. This
phase makes dynamic metadata **consumable mid-chain by a filter decision**: it extends the EXISTING
phase-10 `envoy.filters.http.rbac` filter with a **`metadata` Permission/Principal condition** — a
`MetadataMatcher` that reads `req.dynamic_metadata` (populated earlier in the same decode pass by an
upstream producer filter, e.g. phase-34 `header_to_metadata`) and contributes to the RBAC allow/deny
decision. The mechanism is already in place and is the load-bearing fact for the pick: the decode
pipeline threads ONE `&mut FilterRequest` through every filter in chain order
(`crates/envoy-filter/src/pipeline.rs:77` — `for filter in self.filters.iter_mut()`), so a filter placed
AFTER `header_to_metadata` reads the `dynamic_metadata` it wrote. **This phase adds NO new
`HttpFilterInstance` variant** (RBAC is the existing phase-10 variant) and **NO new infrastructure** (the
store + producers + threading are reused unchanged) — it adds one matcher type to the RBAC config schema
+ one arm to the RBAC recursive evaluator. The **differential is byte-exact, DETERMINISTIC, and LOCALLY
observable**: the probe controls the request header → controls the metadata written → controls the RBAC
verdict, so both proxies return a **byte-identical response** — `200` + the route's `direct_response`
body when the metadata condition matches, or `403` + `RBAC: access denied` (19 bytes, the phase-10/ADR-0034
body) when it does not. RBAC dynamic-metadata conditions is chosen as the highest-leverage minimum-viable
slice because it is the PAYOFF the phase-33/34 ADRs explicitly named ("rbac dynamic-metadata conditions …
become reachable once metadata is emittable + loggable") — it proves the store generalizes from a logging
artifact to a routing-decision input, unlocking the metadata-consumer pattern (the gateway to the rest of
the HTTP-filters consumer vein). See ADR-0085 for the pick rationale and rejected alternatives
(`header_to_metadata` `response_rules` — a thin encode-side producer increment that lights up no new
vein and needs new encode-side metadata-capture plumbing; jwt_authn `payload_in_metadata` — another
*producer*, not the loop-closing consumer; `json_format` — a self-contained Observability leaf unlocking
no filter-family follow-up; non-string metadata Values — a type-system increment best deferred until a
consumer needs a non-string value).

## §1 — Goal & differential surface

**Goal.** Extend the `rbac` HTTP filter with a `metadata` condition (dynamic-metadata Permission/Principal
matching), behaviorally equivalent to upstream Envoy v1.33.0 under the differential contract (§7.2 of
`BOOTSTRAP_PROMPT.md`) on the **Response status** dimension (Exact: `200` vs `403`) and the **Response
body** dimension (byte-exact: the route's `direct_response` body vs the 19-byte `RBAC: access denied`).
The metadata that drives the decision is written by an upstream producer filter (phase-34
`header_to_metadata`) from a request header the probe controls — making the verdict a deterministic
function of the (fixed) probe request + static config.

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0043-http-rbac-dynamic-metadata`** (next free number; baseline is `0001`…`0042`): an H1
  listener whose filter chain is `[header_to_metadata, rbac, router]` and whose route is a
  `direct_response` (projected — fully upstream-independent, byte-exact; **§6.2-VERIFY / §3.5**). The
  `header_to_metadata` filter (configured exactly as in phase 34) extracts a request header (e.g.
  `X-Tier`) into a metadata namespace/key (e.g. `envoy.filters.http.header_to_metadata:tier` — the
  phase-34 default namespace per ADR-0084). The `rbac` filter has a single `action: ALLOW` policy whose
  Permission (or Principal) is a `metadata` matcher requiring that namespace/key to string-match `prod`.
  The fixture drives ≥2 probes: (a) a request **WITH** `X-Tier: prod` → metadata `tier=prod` → the
  metadata condition matches → RBAC allows → `200` + the `direct_response` body, **byte-identical
  cross-proxy**; (b) a request **WITH** `X-Tier: dev` (or the header absent) → metadata `tier=dev`/unset →
  the condition does NOT match → RBAC denies → `403` + `RBAC: access denied`, **byte-identical
  cross-proxy**. The exact `MetadataMatcher` wire shape (`filter` / `path: [{ key }]` / `value:
  { string_match }`), the `filter`-vs-namespace correspondence, the present-but-empty / absent-key match
  semantics, and the StringMatcher modes accepted are **§6.2-VERIFY / §3 PLAN-write calls**.
- **All 42 pre-existing fixtures `0001`–`0042` stay green simultaneously** — the new `metadata` matcher is
  an additive enum variant that no existing RBAC config (fixture `0017`) uses, so every existing RBAC
  decision is unchanged; the dynamic-metadata store, the producers, the operator, and the decode
  threading are UNCHANGED from phases 33/34. This is the load-bearing regression proof (including `0017`
  rbac header-only decisions, `0012` default-format, `0041` set_metadata, and `0042` header_to_metadata
  all UNCHANGED).

**Conformance:** h2spec pass-rate ≥95% (unchanged — no HTTP/2 codec change). No new conformance suite.
Fuzz: the existing `parse_bootstrap` target's reach extends to the new `metadata`-matcher RBAC config
(no NEW fuzz target — the matcher reuses the existing serde/`deny_unknown_fields` parse path with no
bespoke tokenizer); the `accesslog_format_parse` target is UNCHANGED (no operator change). Add a
`parse_bootstrap` config seed exercising a `[header_to_metadata, rbac]` chain with a `metadata`-matcher
policy. Whether the matcher config surface warrants its own dedicated fuzz target is a **§3 PLAN-write
call** (projected NOT).

## §2 — Scope (minimum-viable)

Per §6.3 (no vague deferral): every capability is either IN this phase and tested, or an explicit
deferred non-goal with its own future home. Exact dispositions marked **§6.2-VERIFY** are empirically
locked at the state-2 PLAN-write (the phase-22/23/28/29/30/31/32/33/34 verify-at-PLAN-write discipline);
this SPEC states the projected shape.

### §2.1 IN scope

1. **The RBAC `metadata` Permission + Principal condition (config schema).** Add a `Metadata(MetadataMatcher)`
   variant to BOTH the `Permission` and `Principal` enums in `crates/envoy-config/src/bootstrap.rs`
   (each via the existing hand-rolled `Deserialize` "exactly one map key" visitor — a new `"metadata"`
   key arm alongside `any`/`header`/`and_rules`/`or_rules`/`not_rule` and `any`/`header`/`and_ids`/
   `or_ids`/`not_id`). A new `MetadataMatcher { filter: String, path: Vec<MetadataPathSegment>, value:
   ValueMatcher }` struct + `MetadataPathSegment { key: String }` + a **string-only** `ValueMatcher`
   modeled as a thin enum whose ONLY MVP variant is `StringMatch(StringMatcher)` (reusing the 04.x
   `StringMatcher` verbatim) — all `#[serde(deny_unknown_fields)]`. The matcher resolves the metadata
   namespace from `filter` and the key from a **single-segment** `path` (the string-only store is flat —
   `BTreeMap<String, BTreeMap<String, String>>` — so a multi-segment/nested path is the §2.2 nested-path
   deferral, projected boot-fatal; **§6.2-VERIFY / §3.4**).
2. **The RBAC `metadata` condition (runtime evaluator).** Add `RuntimePermission::Metadata(MetadataMatcher)`
   + `RuntimePrincipal::Metadata(MetadataMatcher)` variants in `crates/envoy-filter/src/rbac.rs`, lowered
   from config by the existing `lower_permission` / `lower_principal`, plus an `eval` arm in
   `eval_permission` / `eval_principal` that reads `req.dynamic_metadata.get(&m.filter).and_then(|ns|
   ns.get(&m.path[0].key))` and applies the `StringMatcher` to the resolved value (absent namespace or
   absent key → no match — `false`). This composes with the existing recursive `AndRules`/`OrRules`/
   `NotRule` (and `AndIds`/`OrIds`/`NotId`) combinators and the `Header` matcher unchanged. The decision
   matrix is UNCHANGED (phase-10 §5.6: `(Allow, matched)`/`(Deny, !matched)` → `Continue`; else →
   `StopAndSend(403, "RBAC: access denied")`).
3. **Reuse (NO change).** The phase-33 dynamic-metadata store (`FilterRequest.dynamic_metadata`), the
   phase-34 `header_to_metadata` producer (the fixture's metadata source), the phase-33
   `%DYNAMIC_METADATA%` operator (OPTIONAL in the fixture `log_format` to additionally witness the
   resolved value — a §3.5 call), and the decode-pipeline shared-`&mut FilterRequest` threading
   (`pipeline.rs:77`) land UNCHANGED. The producer-before-consumer chain ORDER
   (`[header_to_metadata, rbac, …]`) is required and is the load-bearing mechanism (§5).
4. **Tests.** Fixture `0043` (the differential above) + all `0001`–`0042` unchanged (the
   regression-equivalence witnesses; `0017` rbac header-only + `0012`/`0041`/`0042` byte-identical) + an
   in-process backstop (the richer, deterministic complement, mirroring the phase-10/32/33/34 backstop
   split): the `metadata` matcher matches a present value / rejects an absent namespace / rejects an
   absent key / rejects a value mismatch / composes inside `and_rules`/`or_rules`/`not_rule` / works as a
   Principal as well as a Permission / inverts correctly under `action: DENY` / reads metadata a prior
   filter wrote in the same decode pass (the producer→consumer mid-chain thread). Plus a `parse_bootstrap`
   seed with a `[header_to_metadata, rbac]` `metadata`-matcher chain, and a BEHAVIOR_CONTRACT "HTTP
   filters" extension documenting the RBAC `metadata` condition, the `filter`→namespace correspondence,
   the single-segment path, the string-only ValueMatcher, and the absent/present match semantics.

### §2.2 DEFERRED non-goals (explicit; each names its future home)

- **Non-string `ValueMatcher` variants** — Envoy's `envoy.type.matcher.v3.ValueMatcher` is a oneof
  (`null_match`, `double_match`, `bool_match`, `present_match`, `string_match`, `list_match`, `or_match`).
  The MVP supports `string_match` ONLY (the string-only store has no typed values). `present_match` (key
  exists, any value) is the cheapest additive follow-up; the numeric/bool/list/or matchers ride on the
  shared non-string-Value generalization (the phase-33/34 deferral). Future additive increment.
- **Multi-segment / nested metadata path** (`path: [{ key }, { key }, …]`) — the string-only store is flat
  (one namespace → one key → one string), so the MVP resolves a SINGLE path segment; the nested path needs
  the phase-33 nested-`%DYNAMIC_METADATA(ns:key:sub…)%` / structured-Value generalization. Shared future
  home with the non-string-Value work.
- **`MetadataMatcher.invert`** — boolean negation of the match result; cheap additive follow-up (the
  existing `NotRule`/`NotId` combinator already expresses negation, so `invert` is redundant-but-faithful
  and deferred).
- **Other not-yet-shipped RBAC condition types** — the phase-10 deferrals (source/destination IP &
  port, URL path, SNI, requested-server-name, direct-remote-IP, `url_path`, JWT-derived principals, etc.).
  This phase adds ONLY the `metadata` condition; the rest remain their own future increments.
- **RBAC `shadow_rules` / shadow-evaluation stats** — the audit-only second policy set; out of scope.
- **Per-route `typed_per_filter_config` for `rbac`** — route-scoped RBAC override (the cors/csrf/buffer
  precedent); additive via the existing phase-23 `apply_route_config` hook later.
- **Metadata written by OTHER producers / consumers** — jwt_authn `payload_in_metadata` (a producer that
  would feed an RBAC `metadata` principal with JWT claims — a natural strong follow-up once it exists),
  ext_authz/ext_proc dynamic-metadata (gRPC-blocked under ADR-0014); each its own future phase.
- **The other Observability-family surfaces** — `json_format`/`typed_json_format`, gRPC ALS, the OTLP
  access-log sink, tracing, stats sinks, the tap filter; each its own future phase.

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)

These are decisions the state-2 PLAN-write makes after the §6.2 reconnaissance; the brainstorm
deliberately leaves them open:

1. **The `MetadataMatcher` wire shape** — confirm v1.33.0 accepts `metadata: { filter, path: [{ key }],
   value: { string_match: <StringMatcher> } }` as a `Permission`/`Principal` entry, the exact field names
   (`filter` vs `metadata_namespace`; the `path` segment shape `{ key: "..." }`; the `ValueMatcher.string_match`
   field name), and how it round-trips through `/config_dump`. Record whether `path` may be empty and
   whether a `value` is required.
2. **The `filter`→namespace correspondence** — confirm the `MetadataMatcher.filter` field is matched
   against the dynamic-metadata namespace key (our store's outer `BTreeMap` key, the value written by
   `header_to_metadata`'s `metadata_namespace`, default `envoy.filters.http.header_to_metadata` per
   ADR-0084). Confirm a custom `metadata_namespace` set on the producer is matchable by `filter`, and that
   the producer-before-consumer chain order is what Envoy requires for the metadata to be visible.
3. **The match semantics + EXACT verdicts** — for a present value the `string_match` modes accepted (reuse
   the full 04.x `StringMatcher` exact/prefix/suffix/contains/safe_regex, or restrict the MVP to `exact`?);
   the present-but-empty-value disposition; the absent-namespace and absent-key dispositions (both → no
   match → `403` for an `ALLOW` policy); and confirm the `200`/`403` + body bytes (`RBAC: access denied`,
   19 bytes) are byte-identical between a hand-rolled replica and live Envoy across the ≥2 probes.
4. **The config-validity disposition** — a `metadata` matcher with an empty `filter` / empty `path` / a
   multi-segment `path` / a non-`string_match` `value` / a `value` that fails to deserialize — boot-fatal
   (ADR-0049 all-fatal, projected) vs accept-and-degrade. Whether a new `ConfigError` variant is needed
   beyond the existing RBAC parse errors.
5. **The fixture-0043 shape** — `direct_response` (projected — fully upstream-independent, byte-exact
   `200` body) vs a real `http1-echo-server` backend; the producer (`header_to_metadata`, projected — the
   request-driven version that makes the verdict probe-controlled; `set_metadata` static is the simpler
   fallback if header-driven proves awkward); `action: ALLOW` single-policy with the `metadata` condition
   as a Permission vs Principal; the probe set (`X-Tier: prod` → 200 + `X-Tier: dev`/absent → 403, ≥2
   probes); whether to ALSO add a `%DYNAMIC_METADATA(ns:key)%` `log_format` to additionally witness the
   resolved value byte-form (optional, strengthens the differential; reuses phase-32/33 verbatim).
6. **The harness** — the `200`/`403` status + body differential reuses the existing RBAC fixture-`0017`
   driver/comparator (status-exact + body-byte-exact); confirm it slots in with the
   header-controlling probe (the `extra_headers` probe capability is ALREADY confirmed — see §4). If a
   `%DYNAMIC_METADATA%` `log_format` is added (§3.5), the `Driver::Http1AccessLogByteExact` path from
   phase 32/33/34 is reused too.
7. **The fuzz disposition** — confirm the existing `parse_bootstrap` target covers the new
   `metadata`-matcher RBAC config (projected yes — same `parse_bootstrap` entry point) and decide whether
   a dedicated target is warranted (projected NO — reuses the serde parse path) vs a `parse_bootstrap`
   seed only. The `accesslog_format_parse` target is UNCHANGED.
8. **The §6.1 split decision** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)

- **The phase-10 RBAC filter** (`crates/envoy-filter/src/rbac.rs`: the `RuntimePermission`/`RuntimePrincipal`
  enums [`Any`/`Header`/`AndRules`/`OrRules`/`NotRule` and `Any`/`Header`/`AndIds`/`OrIds`/`NotId`], the
  recursive `eval_permission`/`eval_principal(p, &FilterRequest)` evaluators, the `lower_permission`/
  `lower_principal` config→runtime lowering, the `decode_headers` decision matrix + the `403` +
  `b"RBAC: access denied"` local reply, the `RbacFilter` struct + `allowed`/`denied` stats) — phase 35
  adds ONE variant to each enum + ONE arm to each evaluator + ONE lowering arm; everything else unchanged.
- **The RBAC config schema** (`crates/envoy-config/src/bootstrap.rs`: the `RbacConfig`/`RbacPolicy`/
  `Permission`/`Principal` enums with their hand-rolled "exactly one map key" `Deserialize` visitors, the
  `PermissionSet`/`PrincipalSet` wrappers, `HttpFilterTypedConfig::Rbac`, the recursion-depth bound) —
  phase 35 adds one `"metadata"` arm to each visitor + the `MetadataMatcher`/`MetadataPathSegment`/
  `ValueMatcher` structs.
- **The 04.x `StringMatcher`** (`crates/envoy-config/src/bootstrap.rs`, `StringMatcher`/`StringMatcherMode`,
  exact/prefix/suffix/contains/safe_regex; reused verbatim by cors/csrf/rbac-header) — the MVP
  `ValueMatcher::StringMatch` payload; no new matcher engine.
- **The phase-33 dynamic-metadata store** (`crates/envoy-filter/src/types.rs`
  `FilterRequest.dynamic_metadata`, the flat string-only `BTreeMap<String, BTreeMap<String, String>>`) —
  UNCHANGED. The RBAC `metadata` matcher READS it; phase-34 `header_to_metadata` WRITES it.
- **The phase-34 `header_to_metadata` producer** (`crates/envoy-filter/src/header_to_metadata.rs` +
  `HeaderToMetadataConfig`) — the fixture's metadata source, configured exactly as phase 34; UNCHANGED.
- **The decode pipeline's shared mutable request** (`crates/envoy-filter/src/pipeline.rs:77`
  `decode_headers` iterates `self.filters.iter_mut()` passing ONE `&mut FilterRequest`, first
  `StopAndSend` short-circuits) — the load-bearing mechanism: a consumer filter reads what an
  earlier producer filter wrote. UNCHANGED (no pipeline change).
- **The differential harness RBAC path** — the fixture-`0017-http-filter-rbac` structure + its
  status-exact + body-byte-exact comparator + the request-header-setting probe capability
  (`extra_headers`, ALREADY confirmed at phase 34 — wired through `drive_http1` into the actual request),
  and OPTIONALLY the phase-32/33/34 `Driver::Http1AccessLogByteExact` if a `%DYNAMIC_METADATA%` `log_format`
  is added (§3.5) — the templates for fixture `0043`; no new comparator projected.
- **The `parse_bootstrap` fuzz corpus + its `ci.yml` step** + the BEHAVIOR_CONTRACT "HTTP filters"
  section — extend each; no new fuzz target projected.

## §5 — Behavioral contract notes

- **The load-bearing mechanism (mid-chain metadata visibility):** the decode pipeline threads ONE
  `&mut FilterRequest` through every filter in chain order (`pipeline.rs:77`). A producer filter
  (`header_to_metadata`) earlier in the chain writes `req.dynamic_metadata`; the RBAC filter later in the
  chain reads the SAME `req.dynamic_metadata` when it evaluates. The fixture chain order
  `[header_to_metadata, rbac, router]` is therefore REQUIRED (producer before consumer); a chain that
  placed RBAC first would see empty metadata (→ deny for an ALLOW policy). This is the first time a
  filter's decision depends on another filter's mid-pass output — proving the metadata store is a
  routing-decision input, not just a logging artifact.
- **Determinism / byte-exactness (the strong target):** the metadata value RBAC reads is derived ONLY from
  the request the harness controls (the `X-Tier` header value via `header_to_metadata`'s extraction) +
  static config, so the RBAC verdict (and thus the `200`/`403` + body) is a function ONLY of the (fixed)
  probe request + static config — identical on both proxies. **The present-match + absent/mismatch probe
  PAIR in the fixture is the cross-proxy guard:** the matching probe must resolve the extracted value via
  `req.dynamic_metadata.get(filter)?.get(key)` and string-match it (→ allow → 200 + `direct_response`
  body), and the non-matching probe must reach the same store path, fail the match, and deny (→ 403 +
  `RBAC: access denied`) — a faulty implementation that mishandles the lookup or the match fails one of
  the two. The richer combinator/Principal/DENY-inversion/multi-rule logic lives in the in-process
  backstop; the cross-proxy fixture proves Envoy's EXACT verdict + body bytes for the metadata-driven
  decision.
- **Regression-equivalence (the load-bearing proof):** the `metadata` matcher is an additive enum variant
  no existing config uses; the store + producers + operator + decode threading are UNCHANGED from
  phases 33/34 — so all 42 existing fixtures (incl. `0017` rbac header-only, `0012` default-format,
  `0041` set_metadata, `0042` header_to_metadata) stay green unchanged.
- **Filter discipline:** RBAC remains decode-side; on a matched ALLOW (or unmatched DENY) it returns
  `Continue`, else `StopAndSend(403)` — UNCHANGED phase-10 behavior. The new `metadata` condition only
  changes WHICH inputs the existing decision reads; it adds no response-side or request-mutation behavior.
- **Config validity:** a malformed `metadata` matcher is startup-fatal where §6.2 shows Envoy rejects
  (ADR-0049 all-fatal; no reload path this phase).
- **Differential locality:** the `200`/`403` response is observable on a normal request/response WITHOUT a
  file-watch/reload trigger → the fixture-`0043` differential runs and is authoritative on this
  Docker-Desktop host (NOT Linux-CI-only, unlike phases 26/27).

## §6 — Process

### §6.1 — Split projection (§6.1 gate)

A split is projected **NOT to fire**. The surface is one matcher type added to an EXISTING filter (no new
`HttpFilterInstance` variant): one config struct trio (`MetadataMatcher`/`MetadataPathSegment`/
`ValueMatcher`) + one `"metadata"` arm on each of the two RBAC `Deserialize` visitors + one runtime
variant + one `eval` arm + one lowering arm on each of Permission/Principal + one fixture + the backstop —
and it ADDS NO infrastructure (the store + producers + operator + threading are reused unchanged). Estimate
~500–900 LoC / ~7–9 tasks, comparable to `header_to_metadata`/`cdn_loop`/`csrf`. Well under the
~1500-LoC / ~25-task gate. **ADR-0086 is reserved** (for the §6.2 reconciliation; it also covers the
split if one fires — in which case the §6.2 reconciliation takes the next-available number). A split
fires only if §6.2 reveals the
`MetadataMatcher`/`ValueMatcher` wire shape or the match semantics are far gnarlier than projected — e.g.
the string-only MVP cannot produce a byte-exact differential without a typed Value or a multi-segment
path). The natural seam, if forced, is `35.1` (the config schema + the two visitor arms + the runtime
variant/eval/lowering — a foundation slice, NO new fixture, backstop-only, proven by all 42 existing
fixtures staying green) / `35.2` (fixture `0043` + the BEHAVIOR_CONTRACT extension + the fuzz seed +
close).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)

Like phases 22/23/28/29/30/31/32/33/34 (and unlike phases 26/27), this phase's behavior is **locally
observable** (no reload trigger). At the state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0` with an
H1 listener + a `[header_to_metadata, rbac, router]` chain where RBAC has a single `ALLOW` policy with a
`metadata` Permission/Principal, and:
1. RECORD the **`MetadataMatcher` wire shape** v1.33.0 accepts (the `metadata: { filter, path: [{ key }],
   value: { string_match } }` field names; whether `path` may be empty; whether `value` is required;
   whether the matcher is accepted under both `permissions` and `principals`), and how it round-trips
   through `/config_dump`.
2. RECORD the **`filter`→namespace correspondence + match semantics + EXACT verdicts**: with
   `header_to_metadata` writing `X-Tier`→`<namespace>:tier`, confirm an RBAC `metadata` matcher with
   `filter: <namespace>`, `path: [{ key: tier }]`, `value: { string_match: { exact: prod } }` ALLOWS the
   `X-Tier: prod` probe (`200` + body) and DENIES the `X-Tier: dev`/absent probe (`403` + `RBAC: access
   denied`); record the present-but-empty / absent-key / absent-namespace dispositions, the StringMatcher
   modes accepted, and confirm the response (status + body) is **byte-identical** between a hand-rolled
   replica and live Envoy across the ≥2 probes. Confirm the producer-before-consumer chain order is
   required.
3. RECORD the **config-validity disposition** (empty `filter`/`path`, multi-segment `path`, a
   non-`string_match` `value` — boot-fatal vs accepted).
4. Decide STRONG (cross-proxy byte-identical verdict + body — expected); record a fallback only if some
   facet proves non-portable.
**ADR-0086 (the reserved §6.2 reconciliation ADR) FIRES** at the PLAN-write if any of these materially
diverge from this SPEC's projection (notably the `MetadataMatcher`/`ValueMatcher` wire shape, the
`filter`→namespace correspondence, the match/absent semantics, or the config-validity disposition).
`PLAN.md` lands with the empirically-locked facts inline (no `[§6.2-PENDING]` projections — the
verify-at-PLAN-write discipline).

### §6.3 — Anti-deferral

No vague TODOs. Every §2.1 item is implemented + tested this phase; every deferral is a §2.2 named
non-goal with a future home. The matcher schema, the evaluator arms, the fixture, and the backstop are
real and differentially exercised — no stubs. The regression equivalence is proven by all 42 existing
fixtures (incl. `0017` + `0012` + `0041` + `0042`) staying green unchanged.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)

(a) fixture `0043` green (cross-proxy byte-identical verdict: `X-Tier: prod` → `200` + `direct_response`
body; `X-Tier: dev`/absent → `403` + `RBAC: access denied`) + (b) all of `0001`–`0042` green (incl. `0017`
rbac header-only + `0012`/`0041`/`0042` byte-identical — the regression-equivalence witnesses) + (c)
h2spec ≥95% (unchanged — no HTTP/2 codec change) + (d) the existing `parse_bootstrap` (+
`accesslog_format_parse`, unchanged) fuzz targets clean for the short-budget CI run (with the new
`metadata`-matcher seed) — **NO new fuzz target** (§3.7; confirm at state-2/3) + (e) `cargo build
--workspace --all-targets` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` /
`cargo fmt --all -- --check` / `cargo test --workspace` / `cargo deny check` all clean + (f) `REVIEW.md`
approved. `#![forbid(unsafe_code)]` holds (D-3.8). No new crate, no new dependency (D-3.2).

---

_Scope locked by **ADR-0085**. **ADR-0086 is reserved** for the §6.2 reconciliation (state-2 PLAN-write).
The §6.1 split is projected NOT to fire. The state-2 PLAN-write is the next session
(`superpowers:writing-plans`), which runs the §6.2 empirical reconnaissance against live
`envoyproxy/envoy:v1.33.0` and fires ADR-0086._
