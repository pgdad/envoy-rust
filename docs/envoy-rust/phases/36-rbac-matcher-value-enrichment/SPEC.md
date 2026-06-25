# Phase 36 — `36-rbac-matcher-value-enrichment` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0087** (the phase-36 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

**Enrich the RBAC matcher VALUE surface — extending the dynamic-metadata-consumer vein phase 35
just opened, and folding the freshest carry-forward (M35-1).** Phase 35 landed the FIRST
dynamic-metadata consumer: the `envoy.filters.http.rbac` filter (phase 10) gained a string-only
`metadata` Permission/Principal condition (`MetadataMatcher { filter, path: [{ key }], value:
{ string_match: StringMatcher } }`) reading the per-request dynamic-metadata store a producer wrote
mid-chain. That MVP supported exactly ONE `ValueMatcher` variant (`string_match`) and left two
threads dangling. This phase closes both, with **NO new `HttpFilterInstance` variant** and **NO new
infrastructure** (it reuses the phase-10 RBAC engine, the 04.x `StringMatcher`, the phase-33 store,
the phase-34 `header_to_metadata` producer, and the decode-pipeline shared-`&mut FilterRequest`
threading unchanged):

1. **F1 — the `present_match` `ValueMatcher` variant** (ADR-0085 named it "the cheapest additive
   follow-up"): an RBAC `metadata` condition that matches on **key presence** (the value exists, any
   content) rather than a specific string. The differential is byte-exact and probe-controlled: a
   request whose header makes the producer write the metadata key → `present_match: true` matches →
   `200`; the header absent → the key absent → no match → `403` + `RBAC: access denied` (19 bytes).

2. **F2 — `safe_regex` `StringMatcher` compilation on the RBAC path (BOTH `header` and `metadata`
   values) — closes carry-forward M35-1**, which is a **latent runtime panic**: `StringMatcher::matches`
   (`crates/envoy-config/src/matcher.rs:87-91`) does `sr.compiled.as_ref().expect("validator ensured
   StringMatcher SafeRegex compiled")`, but the RBAC validation path
   (`validate_http_filters` → `validate_rbac_config` → the tree validators / `validate_metadata_matcher`)
   is an **immutable borrow** that never compiles `SafeRegex::compiled` (the documented NOTE at
   `crates/envoy-config/src/bootstrap.rs:4040-4044`). A `safe_regex` value in ANY RBAC matcher
   (a `Permission::Header`/`Principal::Header` `safe_regex_match`, or the phase-35 `metadata` value's
   `string_match.safe_regex`) therefore PANICS the first time a request reaches it. Phase 35 used
   `exact` in fixture `0043` so the bug never fired. This phase compiles RBAC SafeRegex at lowering
   time (the M35-1-named fix home, `crates/envoy-filter/src/rbac.rs`) — turning the panic into a
   working byte-exact differential: a `safe_regex` value that matches the resolved metadata/header
   string → `200`; one that does not → `403`.

The **differential is byte-exact, DETERMINISTIC, and LOCALLY observable** (a normal request/response,
no file-watch/reload trigger — NOT Linux-CI-only, unlike phases 26/27): the probe controls the request
header → controls the metadata written → controls the RBAC verdict, so both proxies return a
byte-identical `200` (route `direct_response` body) or `403` + `RBAC: access denied`. This phase
extends the metadata-consumer vein with the cheapest-strong increments while closing a known latent
panic. See ADR-0087 for the pick rationale and rejected alternatives (`json_format` access log — a
bigger Observability leaf with a JSON key-ordering/typing §6.2 risk that folds no carry-forward; the
other not-yet-shipped RBAC condition types `IP`/`SNI`/`url_path` — no metadata-vein continuity;
jwt_authn `payload_in_metadata` — another *producer*, not a consumer increment; non-string typed
`ValueMatcher` Values `null`/`double`/`bool`/`list`/`or` — a type-system increment best deferred until
a consumer needs a non-string value).

## §1 — Goal & differential surface

**Goal.** Extend the `rbac` HTTP filter's matcher-VALUE surface with (F1) the `present_match`
`ValueMatcher` variant on the phase-35 `metadata` condition and (F2) `safe_regex` `StringMatcher`
compilation on the RBAC path, behaviorally equivalent to upstream Envoy v1.33.0 under the differential
contract (§7.2 of `BOOTSTRAP_PROMPT.md`) on the **Response status** dimension (Exact: `200` vs `403`)
and the **Response body** dimension (byte-exact: the route's `direct_response` body vs the 19-byte
`RBAC: access denied`). The metadata that drives an F1/F2-metadata decision is written by an upstream
producer filter (phase-34 `header_to_metadata`) from a request header the probe controls; an
F2-header decision reads the request header directly — both making the verdict a deterministic
function of the (fixed) probe request + static config.

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0044-http-rbac-matcher-value-enrichment`** (next free number; baseline is `0001`…`0043`):
  an H1 listener whose route is a `direct_response` (projected — fully upstream-independent, byte-exact;
  **§6.2-VERIFY / §3.5**). The fixture exercises BOTH new capabilities with probe pairs that the
  **state-2 PLAN-write finalizes** (§3.5), projected as:
  - **F1 (`present_match`):** a `[header_to_metadata, rbac, router]` chain (the phase-35 producer→consumer
    topology) where the `header_to_metadata` filter writes `X-Tier` → `<namespace>:tier` and the RBAC
    `ALLOW` policy's `metadata` condition uses `value: { present_match: true }`. Probe (a) WITH
    `X-Tier: <any>` → key present → match → `200` + body; probe (b) header absent → key absent → no
    match → `403`. **Byte-identical cross-proxy** on both probes.
  - **F2 (`safe_regex`):** an RBAC `safe_regex` value (projected on the `metadata` `string_match` to
    keep the metadata-vein continuity; whether to ALSO add a `Permission::Header` `safe_regex_match`
    probe is a §3.5 call). Probe (c) a header whose extracted metadata value MATCHES the regex (e.g.
    `value: { string_match: { safe_regex: { regex: "prod|staging" } } }`, `X-Tier: staging`) → `200` +
    body; probe (d) a non-matching value (`X-Tier: dev`) → `403`. **Byte-identical cross-proxy.**
- **All 43 pre-existing fixtures `0001`–`0043` stay green simultaneously** — `present_match` is an
  additive `ValueMatcher` enum variant that no existing config uses, and the SafeRegex-compilation fix
  is behavior-preserving for every non-SafeRegex RBAC matcher (and a no-op for the route-config header
  walk, which already compiles SafeRegex). The phase-35 `metadata`-`exact` fixture `0043`, the `0017`
  rbac header-only fixture, `0012` default-format, `0041` set_metadata, and `0042` header_to_metadata
  all stay UNCHANGED. This is the load-bearing regression proof.

**Conformance:** h2spec pass-rate ≥95% (unchanged — no HTTP/2 codec change). No new conformance suite.
Fuzz: the existing `parse_bootstrap` target's reach extends to the new `present_match` `ValueMatcher`
config + a `safe_regex` RBAC matcher value (no NEW fuzz target — both reuse the existing
serde/`deny_unknown_fields` parse path + the existing `regex` compile path with no bespoke tokenizer);
the `accesslog_format_parse` target is UNCHANGED. Add `parse_bootstrap` seeds exercising a
`present_match` `metadata` condition and a `safe_regex` RBAC matcher value. Whether the matcher config
surface warrants its own dedicated fuzz target is a **§3 PLAN-write call** (projected NOT).

## §2 — Scope (minimum-viable)

Per §6.3 (no vague deferral): every capability is either IN this phase and tested, or an explicit
deferred non-goal with its own future home. Exact dispositions marked **§6.2-VERIFY** are empirically
locked at the state-2 PLAN-write (the phase-22/23/28/29/30/31/32/33/34/35 verify-at-PLAN-write
discipline); this SPEC states the projected shape.

### §2.1 IN scope

1. **F1 — the `present_match` `ValueMatcher` variant (config schema).** Add a `PresentMatch(bool)`
   variant to the `ValueMatcher` enum in `crates/envoy-config/src/bootstrap.rs` (the hand-rolled
   "exactly one map key" `Deserialize` visitor gains a `"present_match"` arm alongside the existing
   `"string_match"`; the `Serialize` impl gains the matching arm; `KEYS` grows to
   `["string_match", "present_match"]`). The MVP supports `string_match` and `present_match` ONLY; all
   other `ValueMatcher` oneof keys (`null_match`/`double_match`/`bool_match`/`list_match`/`or_match`)
   remain `unknown_field` → boot-fatal (the §2.2 non-string-Value deferral, stricter than Envoy —
   **§6.2-VERIFY**).
2. **F1 — the `present_match` runtime semantics.** `present_match` matches on KEY PRESENCE, not value
   content, so it CANNOT be evaluated by the existing `ValueMatcher::matches(value: &str)` (which only
   runs when a value is present). Restructure `eval_metadata` in `crates/envoy-filter/src/rbac.rs`
   (currently `…get(filter).and_then(|ns| ns.get(key)).is_some_and(|v| m.value.matches(v))`) so the
   `PresentMatch(want)` case compares **presence** against `want` (`present == want`, where `present =
   the key resolves to Some`), and the `StringMatch` case keeps the existing present-AND-value-matches
   semantics. The exact factoring (an `eval_metadata` `match` on the `ValueMatcher` variant, vs a
   `ValueMatcher::matches_presence(present: Option<&str>) -> bool` helper) is a §3 PLAN-write call.
   `present_match: false` is included for faithfulness (it is a single bool — no extra surface;
   **§6.2-VERIFY** that Envoy accepts/honors `false` AND what `false` MEANS). **Foot-gun (do NOT
   assume):** the repo ALREADY has a `present_match` concept with DIFFERENT `false`-semantics —
   `HeaderMatcherMode::PresentMatch` (`crates/envoy-config/src/matcher.rs:42-47`, the
   `route.v3.HeaderMatcher.present_match` field) treats `present_match: false` as "**no presence
   requirement → always true**", NOT "matches-when-absent". This `ValueMatcher.present_match` is a
   DISTINCT Envoy proto field (`type.matcher.v3.ValueMatcher.present_match`); its `false`-semantics must
   NOT be copied from the header precedent — they are locked empirically at §6.2 (§3.1/§3.2).
3. **F2 — `safe_regex` `StringMatcher` compilation on the RBAC path (closes M35-1).** Compile every
   `SafeRegex` reachable from an RBAC config into `SafeRegex::compiled` (`Arc<regex::Regex>`) at RBAC
   **lowering** time (`crates/envoy-filter/src/rbac.rs` `lower_permission`/`lower_principal` — the
   M35-1-named fix home, a `&mut`-capable / owned-rebuild site, unlike the immutable-borrow config
   validator). Coverage is BOTH RBAC matcher families that carry a `SafeRegex`:
   - the `Permission::Header` / `Principal::Header` `HeaderMatcher` `safe_regex_match` mode, and
   - the phase-35 `metadata` condition's `value: { string_match: { safe_regex } }`.
   The route-config header walk (`validate_header_matcher`, `crates/envoy-config/src/bootstrap.rs:4451`)
   already compiles SafeRegex and is UNCHANGED — F2 only adds the RBAC-path compilation that was
   missing. After F2, `StringMatcher::matches` / `HeaderMatcher::matches` on an RBAC SafeRegex value no
   longer panic. The exact mechanism (compile-on-lower-then-store vs a small mutable pre-pass over the
   `RbacConfig` before lowering; whether the config-validator NOTE at `bootstrap.rs:4040-4044` is
   updated to reflect the now-compiled state) is a §3 PLAN-write call. The malformed-regex disposition
   (an invalid `regex` pattern) is boot-fatal (ADR-0049; today the route-config walk already rejects a
   bad pattern at validate time — confirm the RBAC path rejects equivalently at boot, NOT at first
   request; **§6.2-VERIFY / §3.4**). NOTE: `lower_permission`/`lower_principal` are presently
   INFALLIBLE (`-> RuntimePermission`/`RuntimePrincipal`, not `Result`), so making a bad pattern
   boot-fatal requires EITHER making lowering fallible (thread a `Result` up to the
   `build_from_config` → `Result<Self, FilterError>` site) OR a small mutable pre-pass over the
   `RbacConfig` before lowering — a naïve in-`lower` `Regex::new().unwrap()` would re-introduce a
   (boot-time) panic and is NOT acceptable. The PLAN picks one (§3.4).
4. **Reuse (NO change).** The phase-10 RBAC decision matrix + the `403` + `b"RBAC: access denied"`
   local reply; the recursive `And/Or/Not` combinators; the phase-35 `MetadataMatcher` /
   `MetadataPathSegment` structs + single-segment `path` validator + `RbacMetadataMatcherInvalid`; the
   04.x `StringMatcher`/`SafeRegex` types + the `regex` permitted-foundation (ADR-0021); the phase-33
   dynamic-metadata store; the phase-34 `header_to_metadata` producer; the decode-pipeline shared-`&mut
   FilterRequest` threading (`pipeline.rs:77`). All UNCHANGED. The producer-before-consumer chain ORDER
   (`[header_to_metadata, rbac, …]`) is required for the metadata-driven probes (the load-bearing
   mechanism, §5).
5. **Tests.** Fixture `0044` (the F1 + F2 differentials above) + all `0001`–`0043` unchanged (the
   regression-equivalence witnesses; `0043` rbac-metadata-`exact` + `0017` rbac header-only +
   `0012`/`0041`/`0042` byte-identical) + an in-process backstop (the richer, deterministic complement,
   mirroring the phase-10/35 backstop split): `present_match: true` matches a present key / rejects an
   absent key; `present_match: false` inverts; `present_match` composes inside `and_rules`/`or_rules`/
   `not_rule` and works as a Principal as well as a Permission and inverts correctly under
   `action: DENY`; a `safe_regex` METADATA value matches/rejects the resolved string WITHOUT panicking;
   a `safe_regex` HEADER matcher value (`Permission::Header` `safe_regex_match`) matches/rejects WITHOUT
   panicking (the M35-1 panic-regression guard — this test MUST fail on the pre-fix tree); a malformed
   `regex` pattern is boot-fatal. Plus `parse_bootstrap` seeds (a `present_match` `metadata` condition;
   a `safe_regex` RBAC matcher value), and a BEHAVIOR_CONTRACT "Phase 36" subsection extending the
   phase-35 RBAC `metadata` notes (the `present_match` presence semantics + the now-compiled RBAC
   SafeRegex, superseding the M35-1 limitation note). The existing in-code test
   `rbac_metadata_value_safe_regex_is_parse_accepted` (`crates/envoy-config/src/bootstrap.rs:~12514`),
   which today documents the M35-1 "would panic at runtime" limitation, MUST be repurposed/updated by
   F2 (its comment becomes false once SafeRegex is compiled; it is the natural site to additionally
   assert boot-time compilation or the malformed-regex boot-fatal rejection).

### §2.2 DEFERRED non-goals (explicit; each names its future home)

- **The remaining non-string `ValueMatcher` variants** — Envoy's `envoy.type.matcher.v3.ValueMatcher`
  oneof also has `null_match`, `double_match`, `bool_match`, `list_match`, `or_match`. This phase adds
  `present_match` ONLY (it needs no typed value — just presence); the numeric/bool/list/or matchers ride
  on the shared **non-string-Value generalization** (the flat string-only store
  `BTreeMap<String, BTreeMap<String, String>>` has no typed values — the phase-33/34/35 deferral).
  Future additive increment once a consumer needs a typed value.
- **Multi-segment / nested metadata path** (`path: [{ key }, { key }, …]`) — the string-only store is
  flat, so the MVP still resolves a SINGLE path segment (`path.len() == 1`, the phase-35 boot-fatal
  validator UNCHANGED). Shared future home with the non-string-Value work.
- **`MetadataMatcher.invert`** — boolean negation of the match result; the existing `NotRule`/`NotId`
  combinator already expresses negation, so `invert` is redundant-but-faithful and deferred (the
  phase-35 deferral, unchanged). NOTE: `present_match: false` (this phase) is NOT `invert` — it is the
  faithful "matches-when-absent" semantics of the present matcher itself.
- **`safe_regex` for non-RBAC matchers** — already compiled by the route-config walk; F2 closes ONLY
  the RBAC-path gap. No other matcher family is broken.
- **The `google_re2` `safe_regex` engine knobs** (`max_program_size`, etc.) — envoy-rust uses the
  `regex` crate (ADR-0021) and ignores RE2-specific tuning; unchanged from the existing 04.2 posture.
- **Other not-yet-shipped RBAC condition types** — the phase-10 deferrals (source/destination IP &
  port, URL path, SNI, requested-server-name, direct-remote-IP, `url_path`, JWT-derived principals).
  Each its own future increment.
- **RBAC `shadow_rules` / shadow-evaluation stats**, **per-route `typed_per_filter_config` for `rbac`**,
  **metadata written by OTHER producers/consumers** (jwt_authn `payload_in_metadata`,
  ext_authz/ext_proc — gRPC-blocked under ADR-0014), and **the other Observability-family surfaces**
  (`json_format`/`typed_json_format`, gRPC ALS, OTLP, tracing, stats sinks, tap) — each its own future
  phase (the phase-35 deferral list, unchanged).

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)

These are decisions the state-2 PLAN-write makes after the §6.2 reconnaissance; the brainstorm
deliberately leaves them open:

1. **The `present_match` wire shape** — confirm v1.33.0 accepts `value: { present_match: true }` (and
   `false`) inside the RBAC `metadata` condition, the exact field name (`present_match`), and how it
   round-trips through `/config_dump`. Confirm whether `present_match: false` is accepted and what it
   means (matches-when-absent).
2. **The `present_match` match semantics + EXACT verdicts** — with `header_to_metadata` writing
   `X-Tier` → `<namespace>:tier`, confirm `present_match: true` ALLOWS the `X-Tier: <any>` probe (key
   present → `200` + body) and DENIES the header-absent probe (key absent → `403` + `RBAC: access
   denied`). Record the present-but-EMPTY-value disposition (does an empty extracted string count as
   "present"? — phase-34 §6.2 found present-but-empty headers → metadata UNSET, so this may be moot, but
   confirm). Confirm the `200`/`403` + body bytes are byte-identical between a hand-rolled replica and
   live Envoy.
3. **The `safe_regex` RBAC verdicts** — confirm a `safe_regex` value in an RBAC `metadata` `string_match`
   (and, if §3.5 includes it, a `Permission::Header` `safe_regex_match`) MATCHES the configured pattern
   against the resolved string and yields the byte-identical `200`/`403` + body across the probes.
   Confirm the regex dialect parity (RE2 vs the `regex` crate) for the chosen fixture pattern (keep the
   pattern in the portable common subset — e.g. simple alternation `prod|staging`; AVOID RE2-only or
   `regex`-only constructs — to keep the cross-proxy match identical).
4. **The config-validity dispositions** — (a) a `present_match` with a non-bool payload; (b) the other
   `ValueMatcher` oneof keys (`null_match` etc.) — boot-fatal (stricter than Envoy, projected) vs
   accept; (c) a malformed `safe_regex` `regex` pattern on the RBAC path — confirm it is boot-fatal (NOT
   a first-request panic) AFTER F2, matching the route-config walk's validate-time rejection. Whether a
   new `ConfigError` variant is needed beyond the existing RBAC/`RbacMetadataMatcherInvalid`/regex
   errors (projected NO — reuse the existing surfaces).
5. **The fixture-0044 shape** — `direct_response` (projected) vs a real `http1-echo-server` backend; the
   producer (`header_to_metadata`, projected — the request-driven version that makes the verdict
   probe-controlled); `action: ALLOW` single-policy with the `metadata` condition as a Permission vs
   Principal; the F1 probe pair (header-present → 200 / header-absent → 403) + the F2 probe pair
   (regex-match → 200 / regex-miss → 403); whether F1 and F2 share ONE fixture (two policies / two
   probe pairs) or whether the header-`safe_regex_match` case lives in the backstop only; whether to
   ALSO add a `%DYNAMIC_METADATA(ns:key)%` `log_format` to witness the resolved value byte-form
   (optional, reuses phase-32/33/34 verbatim).
6. **The harness** — the `200`/`403` status + body differential reuses the existing RBAC
   fixture-`0017`/`0043` driver/comparator (status-exact + body-byte-exact) + the header-controlling
   `extra_headers` probe (ALREADY confirmed at phases 34/35). No new comparator projected.
7. **The fuzz disposition** — confirm the existing `parse_bootstrap` target covers the new
   `present_match` config + a `safe_regex` RBAC matcher value (projected yes — same entry point + the
   existing `regex` compile path) and decide whether a dedicated target is warranted (projected NO) vs
   `parse_bootstrap` seeds only. The `accesslog_format_parse` target is UNCHANGED.
8. **The §6.1 split decision** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)

- **The phase-10 RBAC filter** (`crates/envoy-filter/src/rbac.rs`: the
  `RuntimePermission`/`RuntimePrincipal` enums incl. the phase-35 `Metadata(MetadataMatcher)` variant,
  the recursive `eval_permission`/`eval_principal`, `lower_permission`/`lower_principal`, the
  `eval_metadata` helper [phase 35], the decision matrix + the `403` + `b"RBAC: access denied"` local
  reply, the `RbacFilter` + `allowed`/`denied` stats) — phase 36 (F1) restructures `eval_metadata` for
  presence-vs-value and (F2) adds SafeRegex compilation in the lowering functions; everything else
  unchanged.
- **The RBAC config schema + the `ValueMatcher`/`MetadataMatcher` types** (`crates/envoy-config/src/bootstrap.rs`:
  the `ValueMatcher` hand-rolled `Deserialize`/`Serialize` [line ~1349, currently `string_match`-only],
  `MetadataMatcher`/`MetadataPathSegment`, the `Permission`/`Principal` "exactly one map key" visitors,
  `validate_metadata_matcher` [line ~4045] + its M35-1 NOTE) — phase 36 (F1) adds the `present_match`
  arm to the `ValueMatcher` visitor/serializer/`KEYS`; (F2) updates the validator NOTE to reflect the
  now-compiled RBAC SafeRegex.
- **The 04.x `StringMatcher` + `SafeRegex`** (`crates/envoy-config/src/bootstrap.rs`
  `StringMatcher`/`StringMatcherMode::SafeRegex(SafeRegex)`/`SafeRegex { compiled: Option<Arc<Regex>> }`;
  `StringMatcher::matches` / `ValueMatcher::matches` in `crates/envoy-config/src/matcher.rs`) — reused;
  the `.expect("validator ensured … compiled")` at `matcher.rs:90` is the panic F2 removes by ensuring
  the RBAC lowering compiles `SafeRegex::compiled` first. The `regex` crate (ADR-0021) is the engine; no
  new dep.
- **The `validate_header_matcher` route-config SafeRegex compiler** (`crates/envoy-config/src/bootstrap.rs:4451`)
  — the precedent for compiling `SafeRegex::compiled`; F2 mirrors its compile step on the RBAC lowering
  path (which currently does not run it). UNCHANGED itself.
- **The phase-33 dynamic-metadata store** (`crates/envoy-filter/src/types.rs`
  `FilterRequest.dynamic_metadata`, flat string-only `BTreeMap<String, BTreeMap<String, String>>`) —
  UNCHANGED; F1/F2-metadata READ it.
- **The phase-34 `header_to_metadata` producer** + **the decode pipeline's shared mutable request**
  (`crates/envoy-filter/src/pipeline.rs:77`) — UNCHANGED (no pipeline change); the load-bearing
  producer→consumer mid-pass mechanism for the metadata-driven probes.
- **The differential harness RBAC path** — the fixture-`0017`/`0043` structure + its status-exact +
  body-byte-exact comparator + the `extra_headers` request-header probe; the templates for fixture
  `0044`. No new comparator projected.
- **The `parse_bootstrap` fuzz corpus + its `ci.yml` step** + the BEHAVIOR_CONTRACT "HTTP filters"
  Phase-35 RBAC section — extend each; no new fuzz target projected.

## §5 — Behavioral contract notes

- **The load-bearing mechanism (mid-chain metadata visibility), unchanged from phase 35:** the decode
  pipeline threads ONE `&mut FilterRequest` through every filter in chain order (`pipeline.rs:77`). The
  `header_to_metadata` producer earlier in the chain writes `req.dynamic_metadata`; the RBAC filter
  later reads the SAME store. The fixture chain order `[header_to_metadata, rbac, router]` is REQUIRED
  for the F1/F2-metadata probes (producer before consumer).
- **F1 presence vs value (the new semantic axis):** `present_match` is the FIRST `ValueMatcher` that
  asks "does the key exist?" rather than "does the value equal X?". The implementation must NOT route
  it through `ValueMatcher::matches(value: &str)` (that only runs when a value is present, so it could
  never observe `present == false`). It is evaluated where presence is known — `eval_metadata`'s
  `get(filter).and_then(|ns| ns.get(key))` → `Option`. A `present_match: true` matches iff `Some`; the
  `present_match: false` meaning (matches-iff-`None`, vs the header-matcher precedent's
  always-true) is locked empirically at §6.2 — NOT assumed (see the §2.1.2 foot-gun).
- **F2 removes a latent panic (correctness, not just a feature):** before F2, a `safe_regex` value in
  ANY RBAC matcher reaches `StringMatcher::matches`'s `sr.compiled.as_ref().expect(...)` with
  `compiled == None` (the RBAC validator never compiled it) → **panic** on the first matching request.
  Fixture `0043` avoided it by using `exact`. F2 compiles RBAC SafeRegex at lowering time so `compiled`
  is `Some` before any request. The backstop's header-`safe_regex_match` test is the panic-regression
  guard (it MUST panic/fail on the pre-fix tree). Differentially, F2 turns the would-be panic into a
  normal byte-exact regex-match verdict.
- **Determinism / byte-exactness (the strong target):** every F1/F2 verdict is a function ONLY of the
  (fixed) probe request + static config — identical on both proxies. The present/absent (F1) and
  regex-match/miss (F2) probe PAIRS are the cross-proxy guards: the matching probe → `200` + the route
  `direct_response` body; the non-matching probe → `403` + `RBAC: access denied` (19 bytes) — a faulty
  implementation that mishandles presence, the regex compile, or the match fails one of the pair.
- **Regression-equivalence (the load-bearing proof):** `present_match` is an additive enum variant no
  existing config uses; the SafeRegex-compilation fix is behavior-preserving for every non-SafeRegex
  RBAC matcher and a no-op for the already-compiling route-config walk — so all 43 existing fixtures
  (incl. `0043` rbac-metadata-`exact`, `0017` rbac header-only, `0012`/`0041`/`0042`) stay green
  unchanged.
- **Filter discipline:** RBAC remains decode-side; the decision matrix + the local reply are UNCHANGED
  phase-10/35 behavior. F1/F2 only change WHICH inputs / HOW the existing matcher reads its value.
- **Config validity:** a malformed `present_match` / non-string-Value oneof key / malformed RBAC
  `safe_regex` pattern is startup-fatal where §6.2 shows Envoy rejects (ADR-0049 all-fatal; no reload
  path this phase). F2 must make the bad-regex rejection happen at BOOT, not at first request.
- **Differential locality:** the `200`/`403` response is observable on a normal request/response WITHOUT
  a file-watch/reload trigger → fixture `0044` runs and is authoritative on this Docker-Desktop host
  (NOT Linux-CI-only, unlike phases 26/27).

## §6 — Process

### §6.1 — Split projection (§6.1 gate)

A split is projected **NOT to fire**. The surface is two value-matcher enrichments on an EXISTING
filter (no new `HttpFilterInstance` variant, no new infrastructure): (F1) one `ValueMatcher` enum
variant + one `Deserialize`/`Serialize` arm + the `eval_metadata` presence restructure; (F2) compiling
RBAC `SafeRegex` at the two lowering functions (mirroring the existing route-config compiler) + the
validator-NOTE update; one fixture (`0044`) + the backstop (incl. the panic-regression guard) + the
BEHAVIOR_CONTRACT extension + the fuzz seeds. Estimate ~400–800 LoC / ~6–9 tasks, comparable to
`header_to_metadata`/phase-35. Well under the ~1500-LoC / ~25-task gate. **ADR-0088 is reserved** (for
the §6.2 reconciliation; it also covers the split if one fires — in which case the §6.2 reconciliation
takes the next-available number). A split fires only if §6.2 reveals `present_match` semantics or the
RBAC-SafeRegex-compilation restructure are far gnarlier than projected. The natural seam, if forced, is
`36.1` (F2 — RBAC SafeRegex compilation, a correctness slice, backstop-only, the panic-regression guard
+ all 43 existing fixtures green) / `36.2` (F1 — `present_match` + fixture `0044` + the
BEHAVIOR_CONTRACT extension + the seeds + close).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)

Like phases 22/23/28/29/30/31/32/33/34/35 (and unlike phases 26/27), this phase's behavior is **locally
observable** (no reload trigger). At the state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0` with an
H1 `direct_response` listener + a `[header_to_metadata, rbac, router]` chain, and:
1. RECORD the **`present_match` wire shape + semantics**: confirm `value: { present_match: true }` (and
   `false`) is accepted inside the RBAC `metadata` condition; confirm `true` ALLOWS the header-present
   probe (`200` + body) and DENIES the header-absent probe (`403` + `RBAC: access denied`); record the
   present-but-empty-value disposition and how `present_match` round-trips through `/config_dump`.
2. RECORD the **`safe_regex` RBAC verdicts**: with a `safe_regex` value on the `metadata` `string_match`
   (and, if §3.5 includes it, on a `Permission::Header` `safe_regex_match`), confirm a matching string →
   `200` + body and a non-matching string → `403` + `RBAC: access denied`, byte-identical between a
   hand-rolled replica and live Envoy; pick a regex pattern in the RE2 ∩ `regex`-crate portable subset.
3. RECORD the **config-validity dispositions**: the other `ValueMatcher` oneof keys
   (`null_match`/`double_match`/`bool_match`/`list_match`/`or_match`) accepted-vs-boot-fatal (envoy-rust
   stricter-reject, projected); a malformed RBAC `safe_regex` pattern boot-fatal on both.
4. Decide STRONG (cross-proxy byte-identical verdict + body for both F1 and F2 — expected); record a
   fallback only if some facet proves non-portable (e.g. a regex-dialect mismatch → narrow the fixture
   pattern).
**ADR-0088 (the reserved §6.2 reconciliation ADR) FIRES** at the PLAN-write if any of these materially
diverge from this SPEC's projection (notably the `present_match` wire shape/semantics, the present-empty
disposition, the regex-dialect parity, or the non-string-Value config-validity dispositions). `PLAN.md`
lands with the empirically-locked facts inline (no `[§6.2-PENDING]` projections — the
verify-at-PLAN-write discipline).

### §6.3 — Anti-deferral

No vague TODOs. Every §2.1 item is implemented + tested this phase; every deferral is a §2.2 named
non-goal with a future home. The `present_match` variant, the `eval_metadata` presence restructure, the
RBAC SafeRegex compilation, the fixture, and the backstop (incl. the panic-regression guard) are real
and differentially exercised — no stubs. The regression equivalence is proven by all 43 existing
fixtures (incl. `0043` + `0017` + `0012` + `0041` + `0042`) staying green unchanged.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)

(a) fixture `0044` green (cross-proxy byte-identical verdicts: F1 `present_match` header-present →
`200` + `direct_response` body / header-absent → `403` + `RBAC: access denied`; F2 `safe_regex`
regex-match → `200` + body / regex-miss → `403`) + (b) all of `0001`–`0043` green (incl. `0043`
rbac-metadata-`exact` + `0017` rbac header-only + `0012`/`0041`/`0042` byte-identical — the
regression-equivalence witnesses) + (c) h2spec ≥95% (unchanged — no HTTP/2 codec change) + (d) the
existing `parse_bootstrap` (+ `accesslog_format_parse`, unchanged) fuzz targets clean for the
short-budget CI run (with the new `present_match` + `safe_regex` RBAC seeds) — **NO new fuzz target**
(§3.7; confirm at state-2/3) + (e) `cargo build --workspace --all-targets` / `cargo clippy --workspace
--all-targets --all-features -- -D warnings` / `cargo fmt --all -- --check` / `cargo test --workspace`
/ `cargo deny check` all clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds (D-3.8). No
new crate, no new dependency (D-3.2). Carry-forward **M35-1 is CONSUMED** by F2 (the RBAC SafeRegex
panic is fixed + guarded).

---

_Scope locked by **ADR-0087**. **ADR-0088 is reserved** for the §6.2 reconciliation (state-2
PLAN-write). The §6.1 split is projected NOT to fire (**ADR-0089 reserved** for it). The state-2
PLAN-write is the next session (`superpowers:writing-plans`), which runs the §6.2 empirical
reconnaissance against live `envoyproxy/envoy:v1.33.0` and fires ADR-0088._
