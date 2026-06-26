# Phase 37 — `37-rbac-url-path-condition` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0089** (the phase-37 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

**Broaden the RBAC condition-type surface with the `url_path` Permission/Principal condition — a new,
self-contained, byte-exact matcher on the EXISTING phase-10 `envoy.filters.http.rbac` filter.** Phases
35 and 36 deepened the RBAC *matcher-VALUE* surface (the `metadata` condition, then `present_match` +
`safe_regex` on it). This phase widens a different axis: the set of *condition TYPES* the RBAC tree can
express. Today RBAC supports `any`/`header`/`and_*`/`or_*`/`not_*`/`metadata` (Permission and Principal,
`crates/envoy-config/src/bootstrap.rs:1409-1504`). Envoy's RBAC also has `url_path`, `destination_ip`,
`destination_port`, `source_ip`/`direct_remote_ip`/`remote_ip`, and `requested_server_name` (SNI). Of
these, **`url_path` is the only one that needs NO new connection-context plumbing** — it matches the
request **path** (the `:path` pseudo-header, with the query string stripped), which the filter already
holds as `FilterRequest.path` (`crates/envoy-filter/src/types.rs:28-30`). The IP/port/SNI conditions all
require threading the downstream socket address / TLS SNI into the filter (infrastructure the RBAC filter
does not have today), so they defer; `url_path` is the clean, high-reuse next leaf.

`url_path` is Envoy's `type.matcher.v3.PathMatcher` (`url_path: { path: { <StringMatcher> } }`). It
reuses the 04.x `StringMatcher` (exact/prefix/suffix/contains/safe_regex,
`crates/envoy-config/src/bootstrap.rs:2379-2399`) verbatim and the phase-36 **compile-RBAC-SafeRegex-at-
lowering** mechanism (`crates/envoy-filter/src/rbac.rs` `lower_permission`/`lower_principal`) so a
`safe_regex` `url_path` value does not re-introduce the M35-1-class runtime panic. **NO new
`HttpFilterInstance` variant, NO new infrastructure, NO metadata store, NO producer chain** (unlike the
metadata-consumer phases 35/36, `url_path` is self-contained — it reads `req.path` directly, so the
fixture is a plain `[rbac, router]` chain with no upstream producer).

**The differential is byte-exact, DETERMINISTIC, and LOCALLY observable** (a normal request/response, no
file-watch/reload trigger — NOT Linux-CI-only, unlike phases 26/27): the probe controls the request
path → controls the RBAC verdict, so both proxies return a byte-identical `200` (route `direct_response`
body) or `403` + `RBAC: access denied` (19 bytes). **The load-bearing differential richness is the
query-strip semantic**: Envoy matches `url_path` against the path with the `?query` removed, whereas
envoy-rust's existing route matcher compares `req.path` byte-for-byte WITH the query attached
(`crates/envoy-http1/src/hcm.rs:1510-1521`, the `path == p` exact compare; nothing strips the query
today — the codec stores the raw request-target verbatim, `crates/envoy-http1/src/codec.rs:104-106`). A
probe to `/allowed?x=1` with `url_path: { path: { exact: "/allowed" } }` therefore MATCHES under
`url_path` (query stripped) but would NOT under a byte-for-byte exact match of the whole request-target —
so the fixture proves we implemented genuine query-stripped path-extraction. See ADR-0089 for the pick
rationale and rejected alternatives.

## §1 — Goal & differential surface

**Goal.** Add the `url_path` condition type to the `rbac` HTTP filter's `Permission` AND `Principal`
enums, behaviorally equivalent to upstream Envoy v1.33.0 under the differential contract (§7.2 of
`BOOTSTRAP_PROMPT.md`) on the **Response status** dimension (Exact: `200` vs `403`) and the **Response
body** dimension (byte-exact: the route's `direct_response` body vs the 19-byte `RBAC: access denied`).
The path that drives a `url_path` decision is set by the probe's request line, making the verdict a
deterministic function of the (fixed) probe request + static config.

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0045-http-rbac-url-path`** (next free number; baseline is `0001`…`0044`): an H1 listener
  whose route is a `direct_response` (projected — fully upstream-independent, byte-exact;
  **§6.2-VERIFY / §3.5**) with a `[rbac, router]` chain (NO producer needed — `url_path` is
  self-contained). An `action: ALLOW` single-policy whose `url_path` condition uses an `exact` (or
  `prefix`) `StringMatcher` (projected; the §3.5 PLAN-write finalizes). The probe set the state-2
  PLAN-write finalizes (§3.5), projected as:
  - **Probe (a) — match:** GET `/allowed` → `url_path` matches → `200` + the route `direct_response`
    body. **Byte-identical cross-proxy.**
  - **Probe (b) — miss:** GET `/denied` → no match → `403` + `RBAC: access denied` (19 bytes).
    **Byte-identical cross-proxy.**
  - **Probe (c) — the query-strip discriminator (the load-bearing differential):** GET `/allowed?x=1` →
    `url_path` strips the `?x=1` → matches `/allowed` (exact) → `200` + body. This probe is what
    distinguishes a genuine `url_path` implementation from a naive whole-`:path` match (which would see
    `/allowed?x=1 ≠ /allowed` and DENY). **Byte-identical cross-proxy.** (Whether probe (c) uses `exact`
    [strict] or `prefix` [a different discriminator — `prefix: /allowed` matches `/allowed?x=1` even
    WITHOUT query-strip, so `exact` is the stronger query-strip witness] is a §3.5 call.)
- **All 44 pre-existing fixtures `0001`–`0044` stay green simultaneously** — `url_path` is an additive
  `Permission`/`Principal` enum variant that no existing config uses, and nothing else in the RBAC engine
  or the path/route machinery changes. The phase-35/36 RBAC `metadata` fixtures `0043`/`0044`, the `0017`
  rbac header-only fixture, and `0012`/`0041`/`0042` all stay UNCHANGED. This is the load-bearing
  regression proof.

**Conformance:** h2spec pass-rate ≥95% (unchanged — no HTTP/2 codec change). No new conformance suite.
Fuzz: the existing `parse_bootstrap` target's reach extends to the new `url_path` `Permission`/`Principal`
config (including a `safe_regex` `url_path` value, which reuses the existing `regex` compile path) — no
NEW fuzz target (it reuses the existing serde/`deny_unknown_fields` parse path with no bespoke
tokenizer); the `accesslog_format_parse` target is UNCHANGED. Add a `parse_bootstrap` seed exercising a
`url_path` RBAC condition. Whether the config surface warrants its own dedicated fuzz target is a §3
PLAN-write call (projected NOT).

## §2 — Scope (minimum-viable)

Per §6.3 (no vague deferral): every capability is either IN this phase and tested, or an explicit
deferred non-goal with its own future home. Exact dispositions marked **§6.2-VERIFY** are empirically
locked at the state-2 PLAN-write (the phase-22/23/28/29/30/31/32/33/34/35/36 verify-at-PLAN-write
discipline); this SPEC states the projected shape.

### §2.1 IN scope

1. **The `url_path` condition (config schema).** Add a `UrlPath(PathMatcher)` variant to BOTH the
   `Permission` enum (`crates/envoy-config/src/bootstrap.rs:1409-1417`, KEYS at `:1432-1439`) and the
   `Principal` enum (`:1491-1504`, KEYS at `:1514`) — the hand-rolled "exactly one map key"
   `Deserialize` visitors each gain a `"url_path"` arm; the matching `Serialize` arms; each `KEYS` array
   grows by `"url_path"`. A new `PathMatcher` config struct models Envoy's `type.matcher.v3.PathMatcher`,
   whose only in-scope rule is `path: StringMatcher` (`url_path: { path: { <StringMatcher> } }`). All
   StringMatcher modes (exact/prefix/suffix/contains/safe_regex) flow through. **§6.2-VERIFY** the exact
   wire shape (the `path` nesting, the `@type`-free inline form) round-trips through `/config_dump`.
   Confirm `url_path` is accepted under BOTH `permissions` and `principals` (**§6.2-VERIFY** — Envoy's
   `Principal` does carry `url_path`).
2. **The `url_path` runtime semantics.** Add `RuntimePermission::UrlPath(PathMatcher)` /
   `RuntimePrincipal::UrlPath(PathMatcher)` (or hold the inner `StringMatcher` directly — a §3 factoring
   call) and an `eval` arm in `eval_permission`/`eval_principal` (`crates/envoy-filter/src/rbac.rs:74-110`)
   that resolves the **query-stripped request path** and calls `StringMatcher::matches(stripped_path)`.
   The query-strip helper takes `req.path` (the full request-target,
   `crates/envoy-filter/src/types.rs:28-30`, e.g. `/allowed?x=1`) and returns the substring before the
   first `?` (e.g. `/allowed`). **§6.2-VERIFY** the EXACT path Envoy matches `url_path` against (§3.2):
   at minimum query-stripping; whether Envoy ALSO strips a `#fragment`, percent-decodes, or applies
   path-normalization (`//`, `/./`, `/../`, case) before matching is the key risk — the MVP projects
   "strip at the first `?`, no other normalization" and the fixture is locked so any residual
   normalization gap does NOT fire (use already-normalized paths; the phase-36 anchored-pattern
   precedent). Any divergence is recorded as a named carry-forward, explicitly OUT of phase-37 scope.
3. **`safe_regex` in a `url_path` value compiles at RBAC lowering (reuses phase 36's M35-1 fix).** A
   `url_path: { path: { safe_regex: { regex } } }` carries a `SafeRegex` whose `compiled` field must be
   filled BEFORE the first request, exactly as phase 36 does for the RBAC `header`/`metadata` SafeRegex
   (`crates/envoy-filter/src/rbac.rs` `lower_permission`/`lower_principal`, via `compile_safe_regexes()` —
   the fallible-lowering path that makes a malformed pattern boot-fatal, NOT a first-request panic). The
   `url_path` lowering arm calls the same compile step. The route-config header walk
   (`validate_header_matcher`, `crates/envoy-config/src/bootstrap.rs`) is UNCHANGED. **§6.2-VERIFY** a
   malformed `url_path` `safe_regex` pattern is boot-fatal on both proxies (§3.4).
4. **Reuse (NO change).** The phase-10 RBAC decision matrix + the `403` + `b"RBAC: access denied"` local
   reply (`crates/envoy-filter/src/rbac.rs`); the recursive `And/Or/Not` combinators; the 04.x
   `StringMatcher`/`SafeRegex` types + the `regex` permitted-foundation (ADR-0021); the phase-36
   compile-RBAC-SafeRegex-at-lowering path. All UNCHANGED. `url_path` is self-contained: NO dynamic-
   metadata store, NO `header_to_metadata` producer, NO producer-before-consumer chain ordering (the
   fixture chain is plain `[rbac, router]`).
5. **Tests.** Fixture `0045` (the match / miss / query-strip differential above) + all `0001`–`0044`
   unchanged (the regression-equivalence witnesses; `0043`/`0044` rbac-metadata + `0017` rbac header-only
   + `0012`/`0041`/`0042` byte-identical) + an in-process backstop (the richer, deterministic complement,
   mirroring the phase-10/35/36 backstop split): `url_path` `exact`/`prefix`/`suffix`/`contains` match +
   miss; the query-strip (a path WITH a `?query` matches a `url_path` `exact` of the query-LESS path); a
   path with NO query matches the same; `url_path` composes inside `and_rules`/`or_rules`/`not_rule`,
   works as a `Principal` as well as a `Permission`, and inverts correctly under `action: DENY`; a
   `safe_regex` `url_path` value matches/rejects the path WITHOUT panicking (the SafeRegex-compilation
   guard) using an ANCHORED pattern (M36-1, §2.2); a malformed `url_path` `safe_regex` pattern is
   boot-fatal; an empty/missing-`path` `PathMatcher` is boot-fatal (the config-validity guard). Plus a
   `parse_bootstrap` seed (a `url_path` RBAC condition) and a BEHAVIOR_CONTRACT "HTTP filters" subsection
   extending the RBAC notes (the `url_path` condition + its query-strip semantics).

### §2.2 DEFERRED non-goals (explicit; each names its future home)

- **The connection-context RBAC conditions** — `destination_ip`/`destination_port` (Permission),
  `source_ip`/`direct_remote_ip`/`remote_ip` (Principal), and `requested_server_name`/SNI (Permission).
  Each needs the downstream socket address (and, for SNI, the TLS handshake server-name) threaded into
  the filter — infrastructure the RBAC filter does NOT have today (`FilterRequest` carries only
  method/path/headers/body/dynamic_metadata). Their own future RBAC-condition phase(s) once a
  connection-context plumbing slice lands. `url_path` is chosen precisely because it alone among the
  not-yet-shipped condition types needs NONE of this.
- **`url_path` path-normalization beyond query-strip** — if §6.2 shows Envoy percent-decodes, strips a
  `#fragment`, or normalizes `//`/`/./`/`/../`/case before matching `url_path`, the MVP replicates ONLY
  the query-strip and locks the fixture to already-normalized paths so the residual gap does not fire
  (recorded as a named carry-forward). A faithful path-normalization slice is its own future increment
  (it is cross-cutting — route matching and the access log would share it).
- **Unanchored `safe_regex` partial-vs-full (M36-1, cross-cutting)** — envoy-rust's
  `StringMatcher::matches` SafeRegex uses `regex::is_match` (PARTIAL/substring) while Envoy `safe_regex`
  is RE2 FULL match (anchored). `url_path` shares this SafeRegex path, so the phase-37 fixture/backstop
  LOCK an anchored pattern (`^…$`, partial==full) exactly as phase 36 did; the proper full-match fix
  (anchoring at the `matcher.rs` SafeRegex compile) stays deferred to its own future phase. M36-1 is
  weighed here but NOT consumed (folding it would touch the route-config SafeRegex path too — out of this
  leaf's scope).
- **`MetadataMatcher.invert` / non-string `ValueMatcher` variants / multi-segment metadata path** — the
  phase-35/36 metadata-vein deferrals, unchanged (this phase widens the *condition-type* axis, not the
  metadata-VALUE axis).
- **RBAC `shadow_rules` / shadow-evaluation stats**, **per-route `typed_per_filter_config` for `rbac`**,
  and **the other Observability-family surfaces** (`json_format`/`typed_json_format`, gRPC ALS, OTLP,
  tracing, stats sinks, tap) — each its own future phase (the phase-35/36 deferral list, unchanged).

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)

These are decisions the state-2 PLAN-write makes after the §6.2 reconnaissance; the brainstorm
deliberately leaves them open:

1. **The `url_path` / `PathMatcher` wire shape** — confirm v1.33.0 accepts `url_path: { path: { <a
   StringMatcher: exact/prefix/suffix/contains/safe_regex> } }` inside an RBAC `permissions[]` AND a
   `principals[]` entry, and how it round-trips through `/config_dump`. Confirm the `PathMatcher` has no
   other in-scope rule beyond `path` (Envoy's `PathMatcher` is a single-field oneof at this pin).
2. **The EXACT path Envoy matches `url_path` against** (THE key §6.2 item) — with probes to `/allowed`,
   `/allowed?x=1`, `/allowed?` , `/allowed#frag` (if reachable through the H1 request line),
   `/al%6Cowed` (percent-encoded), `/allowed/../allowed`, `//allowed`, and a trailing-slash variant,
   record which ones a `url_path: { path: { exact: "/allowed" } }` policy ALLOWs vs DENYs on live Envoy.
   This pins whether Envoy does query-strip ONLY (the MVP projection) or also fragment-strip /
   percent-decode / path-normalization. Lock the fixture to the portable subset (already-normalized,
   query-strip-only) so the cross-proxy verdict is byte-identical; record any extra normalization as a
   named carry-forward.
3. **The `safe_regex` `url_path` verdicts** — confirm a `safe_regex` `url_path` value matches the
   configured pattern against the query-stripped path and yields the byte-identical `200`/`403` + body.
   Keep the fixture/backstop pattern in the RE2 ∩ `regex`-crate portable subset AND anchored (`^…$`) per
   M36-1 (§2.2).
4. **The config-validity dispositions** — (a) an empty `PathMatcher` / a `PathMatcher` with no `path`
   rule → boot-fatal (projected); whether a new `ConfigError` variant is needed (e.g.
   `RbacUrlPathMatcherInvalid`) or an existing one is reused — projected a small new variant mirroring
   `RbacMetadataMatcherInvalid` (`crates/envoy-config/src/lib.rs:488-496`); (b) the unknown `PathMatcher`
   sub-keys → `deny_unknown_fields` boot-fatal; (c) a malformed `url_path` `safe_regex` pattern →
   boot-fatal AFTER lowering compiles it (NOT a first-request panic), matching phase 36's RBAC SafeRegex
   treatment.
5. **The fixture-0045 shape** — `direct_response` (projected) vs a real `http1-echo-server` backend;
   `action: ALLOW` single-policy with the `url_path` condition as a Permission vs Principal (projected
   Permission, with the Principal path covered in the backstop); the `StringMatcher` mode for the fixture
   (`exact` — the strongest query-strip witness — vs `prefix`); the probe set (match / miss / query-strip
   — projected 3 probes; whether to add a Principal probe or a `safe_regex` probe vs leaving those to the
   backstop); whether to reuse the existing `0017`/`0043` RBAC driver/comparator with the path-varying
   probe (projected yes — the request-line path is already probe-controlled by the harness).
6. **The harness** — the `200`/`403` status + body differential reuses the existing RBAC
   fixture-`0017`/`0043`/`0044` driver/comparator (status-exact + body-byte-exact) with path-varying
   probes (the request line carries the path; ALREADY supported). Confirm no new comparator/driver
   capability is needed (projected none — unlike the metadata phases, no `extra_headers` producer probe
   is even required).
7. **The fuzz disposition** — confirm the existing `parse_bootstrap` target covers the new `url_path`
   config (projected yes — same entry point + the existing `regex` compile path) and decide whether a
   dedicated target is warranted (projected NO) vs a `parse_bootstrap` seed only. The
   `accesslog_format_parse` target is UNCHANGED.
8. **The §6.1 split decision** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)

- **The phase-10 RBAC filter** (`crates/envoy-filter/src/rbac.rs`: the
  `RuntimePermission`/`RuntimePrincipal` enums `:27-68`, the recursive `eval_permission`/`eval_principal`
  `:74-110`, `lower_permission`/`lower_principal` `:247-326` [phase-36 fallible + SafeRegex-compiling],
  the decision matrix + the `403` + `b"RBAC: access denied"` local reply, the `RbacFilter` +
  `allowed`/`denied` stats) — phase 37 adds ONE new variant (`UrlPath`) to each enum + one `eval` arm +
  one `lower` arm + the query-strip helper; everything else unchanged.
- **The RBAC config schema + the `Permission`/`Principal` "exactly one map key" visitors**
  (`crates/envoy-config/src/bootstrap.rs:1409-1550`) — phase 37 adds the `"url_path"` arm +
  `Serialize` arm + `KEYS` entry to EACH visitor, and a new `PathMatcher` struct (`{ path: StringMatcher
  }`). The phase-35 `MetadataMatcher` precedent is the template for a thin matcher struct + its
  validation.
- **The 04.x `StringMatcher` + `SafeRegex`** (`crates/envoy-config/src/bootstrap.rs:2287-2399`
  `StringMatcher`/`StringMatcherMode`/`SafeRegex { compiled: Option<Arc<Regex>> }`; `StringMatcher::matches`
  in `crates/envoy-config/src/matcher.rs:55-103`) — reused verbatim as the `PathMatcher.path` matcher.
  The `regex` crate (ADR-0021) is the engine; no new dep.
- **The phase-36 compile-RBAC-SafeRegex-at-lowering path** (`crates/envoy-filter/src/rbac.rs`
  `lower_permission`/`lower_principal` calling `compile_safe_regexes()`, the fallible lowering threaded to
  `build_from_config -> Result<…, FilterError>`) — the `url_path` lowering arm calls the same compile
  step so a `safe_regex` `url_path` value never hits the `matcher.rs:90`
  `.expect("validator ensured … compiled")` with `compiled == None`. UNCHANGED itself.
- **The `FilterRequest.path` field** (`crates/envoy-filter/src/types.rs:28-30`, the full request-target
  including any query; populated from the codec at `crates/envoy-http1/src/hcm.rs:775-777` /
  `crates/envoy-http1/src/codec.rs:104-106` and the H2 equivalent) — READ by `url_path` after
  query-stripping. UNCHANGED.
- **The `HttpFilterInstance::Rbac` dispatch** (`crates/envoy-filter/src/instance.rs:36-202`, the `build`
  arm `:151-153` and `decode_headers` arm `:186`) — UNCHANGED (no new variant; `url_path` is internal to
  the existing RBAC filter).
- **The differential harness RBAC path** — the fixture-`0017`/`0043`/`0044` structure + its status-exact
  + body-byte-exact comparator + the path-varying request-line probe; the templates for fixture `0045`.
  No new comparator projected.
- **The `parse_bootstrap` fuzz corpus + its `ci.yml` step** + the BEHAVIOR_CONTRACT "HTTP filters" RBAC
  section — extend each; no new fuzz target projected.

## §5 — Behavioral contract notes

- **The new axis (condition TYPE, not matcher VALUE):** phases 35/36 deepened WHAT an RBAC `metadata`
  value can match; phase 37 widens WHICH request attributes RBAC can condition on. `url_path` is the
  first request-attribute condition beyond `header` — and the only not-yet-shipped one that reads an
  attribute (`req.path`) the filter already holds, so it needs zero new plumbing.
- **The query-strip semantic (the load-bearing distinction):** Envoy matches `url_path` against the path
  WITHOUT the query string; envoy-rust's existing route matcher compares the full request-target
  (`req.path`) WITH the query. The implementation MUST strip the query before matching, and the fixture's
  query-bearing probe (`/allowed?x=1` → `200` under `url_path: { exact: "/allowed" }`) is the cross-proxy
  guard that proves it. The EXACT extent of Envoy's path-extraction (query-only vs also fragment /
  percent-decode / normalization) is locked empirically at §6.2 — NOT assumed (see §2.1.2 / §3.2). Any
  normalization beyond query-strip is fixture-avoided and recorded as a named carry-forward.
- **`safe_regex` reuses phase 36's fix (no new panic):** a `safe_regex` `url_path` value compiles at RBAC
  lowering exactly like a `header`/`metadata` SafeRegex, so it never reaches `matcher.rs:90`'s `.expect`
  with `compiled == None`. The backstop's `safe_regex`-`url_path` test guards this; an anchored pattern
  keeps partial==full per M36-1.
- **Determinism / byte-exactness (the strong target):** every verdict is a function ONLY of the (fixed)
  probe request line + static config — identical on both proxies. The match / miss / query-strip probe
  set is the cross-proxy guard: the matching probe → `200` + the route `direct_response` body; the
  non-matching probe → `403` + `RBAC: access denied` (19 bytes) — a faulty implementation that mishandles
  the query-strip or the StringMatcher fails one of the set.
- **Regression-equivalence (the load-bearing proof):** `url_path` is an additive `Permission`/`Principal`
  variant no existing config uses; nothing else in the RBAC engine or the path/route machinery changes —
  so all 44 existing fixtures (incl. `0043`/`0044` rbac-metadata, `0017` rbac header-only,
  `0012`/`0041`/`0042`) stay green unchanged.
- **Filter discipline:** RBAC remains decode-side; the decision matrix + the local reply are UNCHANGED
  phase-10 behavior. `url_path` only adds a new condition type the existing matcher tree can hold.
- **Config validity:** an empty/`path`-less `PathMatcher`, an unknown `PathMatcher` sub-key, and a
  malformed `url_path` `safe_regex` pattern are startup-fatal where §6.2 shows Envoy rejects (ADR-0049
  all-fatal; no reload path this phase). The bad-regex rejection must happen at BOOT (via the fallible
  lowering), not at first request.
- **Differential locality:** the `200`/`403` response is observable on a normal request/response WITHOUT
  a file-watch/reload trigger → fixture `0045` runs and is authoritative on this Docker-Desktop host
  (NOT Linux-CI-only, unlike phases 26/27).

## §6 — Process

### §6.1 — Split projection (§6.1 gate)

A split is projected **NOT to fire**. The surface is ONE new condition type on an EXISTING filter (no new
`HttpFilterInstance` variant, no new infrastructure): one `Permission` variant + one `Principal` variant
+ their `Deserialize`/`Serialize`/`KEYS` arms; a thin `PathMatcher` config struct (`{ path: StringMatcher
}`) + its validator; the two runtime `eval` arms + the two `lower` arms (reusing the phase-36 SafeRegex
compile) + the query-strip helper; one fixture (`0045`) + the backstop + the BEHAVIOR_CONTRACT extension
+ the fuzz seed. Estimate ~350–700 LoC / ~6–9 tasks, comparable to `header_to_metadata`/phase-36. Well
under the ~1500-LoC / ~25-task gate. **ADR-0090 is reserved** for the §6.2 reconciliation; **ADR-0091 is
reserved** for the split (projected NOT to fire). A split fires only if §6.2 reveals the `url_path`
path-extraction (normalization extent) is far gnarlier than projected. The natural seam, if forced, is
`37.1` (config schema + `PathMatcher` + validator + the parse-layer tests) / `37.2` (runtime `eval`/`lower`
+ query-strip + fixture `0045` + BEHAVIOR_CONTRACT + seed + close).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)

Like phases 22/23/28/29/30/31/32/33/34/35/36 (and unlike phases 26/27), this phase's behavior is
**locally observable** (no reload trigger). At the state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0`
with an H1 `direct_response` listener + a `[rbac, router]` chain, and:
1. RECORD the **`url_path` / `PathMatcher` wire shape**: confirm `url_path: { path: { exact: "/allowed" }
   }` (and `prefix`/`suffix`/`contains`/`safe_regex`) is accepted inside an RBAC `permissions[]` AND a
   `principals[]` entry, and how it round-trips through `/config_dump`.
2. RECORD the **EXACT path Envoy matches `url_path` against** (§3.2): sweep `/allowed`, `/allowed?x=1`,
   `/allowed?`, `/allowed#frag`, `/al%6Cowed`, `/allowed/../allowed`, `//allowed`, `/allowed/` against a
   `url_path: { path: { exact: "/allowed" } }` policy on live Envoy; record ALLOW/DENY for each. Pin the
   query-strip (vs also fragment/percent-decode/normalization) and lock the fixture to the portable
   subset.
3. RECORD the **`safe_regex` `url_path` verdicts** with an anchored portable pattern; confirm a matching
   path → `200` + body, a non-matching path → `403` + `RBAC: access denied`, byte-identical between a
   hand-rolled replica and live Envoy; confirm a malformed pattern is boot-fatal on both.
4. RECORD the **config-validity dispositions**: empty/`path`-less `PathMatcher`, unknown `PathMatcher`
   sub-key, malformed `url_path` `safe_regex` — boot-fatal on both (envoy-rust all-fatal, ADR-0049).
5. Decide STRONG (cross-proxy byte-identical verdict + body for match / miss / query-strip — expected);
   record a fallback only if some facet proves non-portable (e.g. Envoy normalizes the path beyond
   query-strip → narrow the fixture paths and record the carry-forward).
**ADR-0090 (the reserved §6.2 reconciliation ADR) FIRES** at the PLAN-write if any of these materially
diverge from this SPEC's projection (notably the path-extraction extent, the `Principal`-`url_path`
acceptance, the `PathMatcher` wire shape, or the config-validity dispositions). `PLAN.md` lands with the
empirically-locked facts inline (no `[§6.2-PENDING]` projections — the verify-at-PLAN-write discipline).

### §6.3 — Anti-deferral

No vague TODOs. Every §2.1 item is implemented + tested this phase; every deferral is a §2.2 named
non-goal with a future home. The `url_path` variants, the query-strip helper, the SafeRegex-at-lowering
compile arm, the fixture, and the backstop are real and differentially exercised — no stubs. The
regression equivalence is proven by all 44 existing fixtures (incl. `0043`/`0044` + `0017` + `0012` +
`0041` + `0042`) staying green unchanged.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)

(a) fixture `0045` green (cross-proxy byte-identical verdicts: `url_path` match → `200` +
`direct_response` body / miss → `403` + `RBAC: access denied` / query-strip probe → `200` + body) +
(b) all of `0001`–`0044` green (incl. `0043`/`0044` rbac-metadata + `0017` rbac header-only +
`0012`/`0041`/`0042` byte-identical — the regression-equivalence witnesses) + (c) h2spec ≥95% (unchanged
— no HTTP/2 codec change) + (d) the existing `parse_bootstrap` (+ `accesslog_format_parse`, unchanged)
fuzz targets clean for the short-budget CI run (with the new `url_path` RBAC seed) — **NO new fuzz
target** (§3.7; confirm at state-2/3) + (e) `cargo build --workspace --all-targets` / `cargo clippy
--workspace --all-targets --all-features -- -D warnings` / `cargo fmt --all -- --check` / `cargo test
--workspace` / `cargo deny check` all clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds
(D-3.8). No new crate, no new dependency (D-3.2). M36-1 (unanchored SafeRegex partial-vs-full) is weighed
but NOT consumed (fixture/backstop lock anchored patterns; the cross-cutting fix stays deferred).

---

_Scope locked by **ADR-0089**. **ADR-0090 is reserved** for the §6.2 reconciliation (state-2
PLAN-write). The §6.1 split is projected NOT to fire (**ADR-0091 reserved** for it). The state-2
PLAN-write is the next session (`superpowers:writing-plans`), which runs the §6.2 empirical
reconnaissance against live `envoyproxy/envoy:v1.33.0` and fires ADR-0090._
