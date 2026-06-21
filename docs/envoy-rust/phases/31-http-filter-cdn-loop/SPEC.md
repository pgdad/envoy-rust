# Phase 31 — `31-http-filter-cdn-loop` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0076** (the phase-31 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

Continue the **HTTP-filters family** (after `local_ratelimit` 09 / `rbac` 10 / `fault` 11 /
`header_mutation` 13 / `jwt_authn` 22 / `cors` 23 / `csrf` / `buffer`) with
**`envoy.filters.http.cdn_loop`** — Envoy's RFC 8586 `CDN-Loop` request-header filter, the **9th
concrete feature filter**. For each request the filter parses the inbound `CDN-Loop` header (an
RFC 8586 comma-separated `cdn-info` list), counts how many times *this* CDN's configured `cdn_id`
already appears, and either **rejects the request** (a forwarding loop — count exceeds
`max_allowed_occurrences`, default 0) or **appends its `cdn_id`** to the `CDN-Loop` header and
forwards upstream. It is a **decode-side, header-only, fully self-contained** filter: no upstream
state, no per-route config, no cross-cutting dependency. The **differential is byte-exact and
DETERMINISTIC** — the appended `cdn_id` and the reject status/body are identical on both proxies
(none of the consistent-hash shared-endpoint-string friction of phases 28–30), observable as a
**normal request/response on this Docker-Desktop dev host** (no file-watch/reload trigger). The
phase reuses the entire existing H1/H2 filter pipeline + the decode-side `StopAndSend` reject
helpers + the request-header-append path; it adds **no new pipeline machinery**. See ADR-0076 for
the pick rationale and the rejected alternatives (`header_to_metadata` — output is internal dynamic
metadata, needs a cross-cutting `%DYNAMIC_METADATA%` access-log operator to observe; `stateful_session`
— cookie encodes the per-side host address, so it cannot be a byte-exact differential; the remaining
LB policies — non-deterministic or HC-dependent).

## §1 — Goal & differential surface

**Goal.** Implement Envoy's `envoy.filters.http.cdn_loop.v3.CdnLoopConfig` filter (RFC 8586
`CDN-Loop`: parse → count `cdn_id` → reject-on-loop or append-and-forward), behaviorally
equivalent to upstream Envoy v1.33.0 under the differential contract (§7.2 of
`BOOTSTRAP_PROMPT.md`).

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0039-http-filter-cdn-loop`** (next free number; baseline is `0001`…`0038`): an H1
  listener whose HCM filter chain carries the `cdn_loop` filter (`cdn_id: "<id>"`,
  `max_allowed_occurrences: 0`) ahead of the router, routing to one real backend (the existing
  `http1-echo-server` `--body-marker` backend, reflecting the received request headers). The driver
  sends, per probe, and asserts cross-proxy-identical results:
  1. **No `CDN-Loop` header** → `200`; the backend received `CDN-Loop: <cdn_id>` (the appended
     value — observed via the echo backend reflecting the request header).
  2. **`CDN-Loop: <cdn_id>` already present** (count `1` > `max_allowed_occurrences` `0`) →
     **reject** with the loop status + body (**§6.2-VERIFY** — projected `502`).
  3. **`CDN-Loop: <foreign-id>`** (count `0` of our `cdn_id` ≤ `0`) → `200`; the backend received
     `CDN-Loop: <foreign-id>, <cdn_id>` (appended after the existing entry — exact join formatting
     **§6.2-VERIFY**).
  4. **A malformed `CDN-Loop` header** → **reject** with the parse-error status + body
     (**§6.2-VERIFY** — projected `400`).
  Deterministic, header-only, zero timing/crypto/concurrency.
- **All 38 pre-existing fixtures `0001`–`0038` stay green simultaneously** — the `cdn_loop` filter
  is opt-in per filter chain; no existing listener configures it, and no existing filter reads
  `CDN-Loop`, so the filter is **inert when not in the chain** (the 07.1 foundation-slice
  regression-equivalence property — the load-bearing proof).

**Conformance:** h2spec pass-rate ≥95% (unchanged — no HTTP/2 codec change). No new conformance
suite. A new `parse_bootstrap` fuzz seed exercises the new config surface (`cdn_loop` filter entry);
whether to add a **dedicated fuzz target over the RFC-8586 `CDN-Loop` header parser** is a §3 open
PLAN-write call (the parser is the one non-trivial surface).

## §2 — Scope (minimum-viable)

Per §6.3 (no vague deferral): every capability is either IN this phase and tested, or an explicit
deferred non-goal with its own future home. Exact dispositions marked **§6.2-VERIFY** are
empirically locked at the state-2 PLAN-write (the phase-22/23/28/29/30 verify-at-PLAN-write
discipline); this SPEC states the projected shape.

### §2.1 IN scope

1. **Config — the filter config.** Add a `CdnLoopConfig { cdn_id: String, max_allowed_occurrences:
   u32 }` schema (`crates/envoy-config/src/bootstrap.rs`, with the other filter configs;
   `max_allowed_occurrences` serde-default `0`) and register a `CdnLoop` variant in the
   `@type`-tagged HTTP-filter config enum (`type.googleapis.com/envoy.extensions.filters.http.cdn_loop.v3.CdnLoopConfig`).
   `deny_unknown_fields`.
2. **Config validation.** `cdn_id` must be non-empty AND a valid RFC 8586 `cdn-id` (a single
   `cdn-info` token — i.e. it must parse as a `dquoted-string`-or-`token` and must NOT itself
   contain a comma/parameter that would make the appended header malformed) → a new `ConfigError`
   variant on violation. The exact strictness (does Envoy reject a `cdn_id` with a comma / invalid
   token at config-load, or only misbehave at request-time?) is **§6.2-VERIFY** (the ADR-0049
   fatal-only-where-Envoy-rejects posture).
3. **The CDN-Loop parser (the engine / correctness gate).** A small RFC 8586 `CDN-Loop` header
   parser — the header is a comma-separated list of `cdn-info`, each a `cdn-id` optionally followed
   by `;`-separated parameters; multiple `CDN-Loop` request headers combine as one comma-joined
   list (**§6.2-VERIFY** the exact multi-header + parameter handling). It exposes "count occurrences
   of `cdn_id`" and "is this header well-formed". A pinned unit oracle is the §A correctness anchor.
4. **The filter.** `CdnLoopFilter` — a new `HttpFilterInstance` variant (the 9th feature filter, in
   `crates/envoy-filter/`). **Decode path:** parse the `CDN-Loop` request header(s); if malformed →
   reject (parse-error reply); else count `cdn_id`; if `count > max_allowed_occurrences` → reject
   (loop reply) via the existing decode-side `Decision::StopAndSend`; else **append `cdn_id`** to the
   `CDN-Loop` request header (RFC 8586 append) and continue. **Encode path:** inert.
5. **Reject dispositions.** The loop reject and the malformed reject use the existing H1 + phase-11
   H2 filter-synth `StopAndSend` local-reply decorators. The exact status codes, response bodies,
   `x-envoy-*` headers, and response flags are **§6.2-VERIFY** (projected: loop → `502`, malformed →
   `400`; bodies recorded verbatim at the recon, the jwt_authn/cors per-class-body discipline).
6. **Stats.** Any per-filter stat Envoy emits for cdn_loop is **§6.2-VERIFY** — emitted only if §6.2
   confirms a portable namespace + deterministic values; otherwise none (the phase-21/24/28/29/30
   no-stat discipline; cdn_loop is projected to emit NO dedicated stat).
7. **Tests.** Fixture `0039` (the differential above) + an in-process backstop (the parser oracle;
   count/append/reject paths incl. the `max_allowed_occurrences > 0` boundary; the malformed-reject
   path; the inert-when-unconfigured no-op regression witness) + a `parse_bootstrap` fuzz seed (+ a
   dedicated header-parser fuzz target if §3 decides) + a BEHAVIOR_CONTRACT "HTTP filters" cdn_loop
   subsection.

### §2.2 DEFERRED non-goals (explicit; each names its future home)

- **Per-route `typed_per_filter_config` for cdn_loop** — the 23.x per-route filter-config
  infrastructure exists; a per-route cdn_loop override slots in additively in a future per-route
  pass (the cors/csrf precedent). MVP is filter-chain-level config only.
- **RFC 8586 `cdn-info` PARAMETER semantics beyond counting** — the filter counts `cdn_id`
  occurrences; it does not interpret or preserve per-`cdn-info` parameters beyond what faithful
  append/parse requires (**§6.2-VERIFY** how Envoy treats parameters on the matched `cdn_id`).
- **Other deterministic HTTP filters** (`compressor`/`decompressor` — byte-identical-gzip risk;
  `header_to_metadata`/`set_metadata` — internal-metadata output, need a `%DYNAMIC_METADATA%`
  access-log operator; `stateful_session` — non-byte-exact cookie; `grpc_*` — ADR-0014/gRPC-blocked;
  `ext_authz`/`ext_proc`/`lua`/`wasm`/`oauth2` — external services/engines/crypto) — each its own
  future HTTP-filter-family phase.
- **The CDN-Loop response-header / encode-side behavior** — RFC 8586 is a request-header protocol;
  cdn_loop is decode-only. No encode-side work.

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)

These are decisions the state-2 PLAN-write makes after the §6.2 reconnaissance; the brainstorm
deliberately leaves them open:

1. **The reject dispositions** — the exact loop status/body (projected `502`) + the exact
   malformed-header status/body (projected `400`) + response flags + any `x-envoy-*` headers, all
   recorded verbatim at the recon.
2. **The append formatting** — the exact byte form of the appended `CDN-Loop` header when an entry
   already exists (`<existing>, <cdn_id>` — comma-space? comma-only?) and when absent
   (`CDN-Loop: <cdn_id>`).
3. **The parser strictness** — what Envoy treats as a malformed `CDN-Loop` (vs a parse that simply
   finds zero `cdn_id` matches); the multi-`CDN-Loop`-header combination rule; `cdn-info` parameter
   handling; case sensitivity of `cdn-id` matching.
4. **The config-validity disposition** — does Envoy reject a malformed/empty `cdn_id` at config-load
   (fatal) or only at request-time? (the ADR-0049 posture).
5. **The observation mechanism for the append path** — confirm the differential backend reflects the
   received `CDN-Loop` request header into the response (the echo backend), or choose the precise
   probe that makes the appended value cross-proxy-observable.
6. **Whether to add a dedicated `cdn_loop_parse` fuzz target** over the RFC-8586 header parser, vs a
   `parse_bootstrap` config seed only.
7. **Any cdn_loop stat namespace** (§2.1.6) — §6.2-verified.
8. **The §6.1 split decision** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)

- **The `HttpFilterInstance` enum + the H1/H2 filter pipeline** (`crates/envoy-filter/src/instance.rs`
  + `pipeline.rs`; 8 feature filters already wired) — `CdnLoopFilter` is the 9th variant; the
  pipeline dispatch, the decode/encode hooks, and the build-from-config path are reused wholesale.
- **The decode-side `Decision::StopAndSend` + the H1/phase-11-H2 filter-synth local-reply
  decorators** (used by fault/jwt_authn/cors/csrf/local_rate_limit) — the loop + malformed rejects
  reuse this exact path; no new local-reply machinery.
- **The request-header mutation path** (`header_mutation.rs` appends/sets request headers) — the
  template for the `CDN-Loop` append.
- **The `@type`-tagged HTTP-filter config enum + the per-filter `*Config` serde pattern**
  (`crates/envoy-config/src/bootstrap.rs`) — extend with `CdnLoopConfig` following the
  `CorsPolicy`/`CsrfPolicy` shape.
- **The deterministic HTTP-filter fixture harness** (`tests/fixtures/0031-http-filter-cors/` /
  `0032-http-filter-csrf/` / `0033-http-filter-buffer/` + the `http1-echo-server` `--body-marker`
  backend + the differential driver) — the structural template for fixture 0039.
- **The `parse_bootstrap` fuzz corpus + the BEHAVIOR_CONTRACT "HTTP filters" section** — extend.

## §5 — Behavioral contract notes

- **Determinism:** the same `CDN-Loop` request input → the same decision (reject / append) and the
  same appended bytes, on each proxy and across requests, AND (strong target) the same result on
  BOTH proxies (the `cdn_id` + reject body are static config / fixed strings — cross-proxy
  byte-identical, with none of the per-side host-address dependence that weakened the LB cookie /
  consistent-hash cases).
- **Reject semantics (projected; §6.2-VERIFY):** `count(cdn_id) > max_allowed_occurrences` → loop
  reject (projected `502`); a malformed `CDN-Loop` header → parse-error reject (projected `400`);
  otherwise append `cdn_id` and forward.
- **Regression-equivalence:** every listener WITHOUT a `cdn_loop` filter behaves exactly as before
  (the inert-filter no-op proof — all 38 existing fixtures green; no existing filter reads
  `CDN-Loop`).
- **Config validity:** a malformed/empty `cdn_id` is a startup-fatal parse error where §6.2 shows
  Envoy rejects (ADR-0049 all-fatal; no reload path this phase).
- **Differential locality:** the cdn_loop behavior is observable WITHOUT a file-watch/reload trigger
  → the fixture-0039 differential runs and is authoritative on this Docker-Desktop host (NOT
  Linux-CI-only, unlike phases 26/27).

## §6 — Process

### §6.1 — Split projection (§6.1 gate)

A split is projected **NOT to fire**. cdn_loop is a small, self-contained, single-crate-centred
filter (one config struct + one parser + one filter variant + one fixture), comparable to `csrf`
(~485 LoC) / `buffer` (~267 LoC) — well under the ~1500-LoC / ~25-task gate. Estimate ~600–900 LoC
/ ~6–8 tasks. **ADR-0078 is reserved** for the split (fires only in the unlikely event the
RFC-8586 parser proves far gnarlier than projected); the natural seam, if forced, is `31.1`
(config + parser + validation + backstop) / `31.2` (filter variant + HCM wiring + fixture 0039 +
fuzz + BEHAVIOR_CONTRACT + close).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)

Like phases 22/23/28/29/30 (and unlike phases 26/27), this phase's behavior is **locally
observable** (no reload trigger). At the state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0`
with an H1 listener carrying the `cdn_loop` filter + one echo backend, and:
1. RECORD the four probe outcomes (the ground truth the differential asserts): the appended
   `CDN-Loop` value (no-header + foreign-id cases — exact join formatting), the loop-reject
   status/body/flags, the malformed-reject status/body/flags.
2. Verify the parser strictness (what is "malformed"), the multi-`CDN-Loop`-header combination rule,
   `cdn-info` parameter handling, `cdn-id` case sensitivity, the config-validity disposition
   (malformed/empty `cdn_id` — fatal vs accept), and any cdn_loop stat namespace.
3. Decide STRONG (cross-proxy byte-identical — expected, since every output is static config or a
   fixed reply body); record a fallback equivalence only if some disposition proves non-portable.
**ADR-0077 FIRES** at the PLAN-write if any of these materially diverge from this SPEC's projection
(notably the reject status codes/bodies or the append formatting). `PLAN.md` lands with the
empirically-locked facts inline (no `[§6.2-PENDING]` projections — the verify-at-PLAN-write
discipline).

### §6.3 — Anti-deferral

No vague TODOs. Every §2.1 item is implemented + tested this phase; every deferral is a §2.2 named
non-goal with a future home. The parser, the filter, the reject + append paths, and the fixture are
real and differentially exercised — no stubs.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)

(a) fixture `0039` green + (b) all of `0001`–`0038` green + (c) h2spec ≥95% + (d) the
`parse_bootstrap` fuzz seed (+ any new parser fuzz target) clean + (e) `cargo build --workspace
--all-targets` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` / `cargo
fmt --all -- --check` / `cargo test --workspace` / `cargo deny check` all clean + (f) `REVIEW.md`
approved. `#![forbid(unsafe_code)]` holds (D-3.8).

---

_Scope locked by **ADR-0076**. ADR-0077 reserved (§6.2 reconciliation), ADR-0078 reserved (§6.1
split). The state-2 PLAN-write is the next session (`superpowers:writing-plans`)._
