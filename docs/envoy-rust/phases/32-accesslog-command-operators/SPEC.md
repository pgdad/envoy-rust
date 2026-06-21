# Phase 32 — `32-accesslog-command-operators` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0078** (the phase-32 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

**Open the Observability family** by generalizing the phase-06.2 `envoy-accesslog` crate — which
today emits a **hardcoded** 14-operator Envoy-v3 default-format access-log line
(`crates/envoy-accesslog/src/default_format.rs` is a direct field-to-string concatenator, **not** a
parser) — into a real **command-operator substitution engine** driven by a configurable
`log_format` text-format string. The engine parses `%OPERATOR%` / `%OPERATOR(args)%` /
`%OPERATOR(args):N%` plus plain-text literals once at config-load time and evaluates the compiled
format against the existing 15-field `AccessLogRecord` per request. The phase ships a **curated
DETERMINISTIC operator set** and re-expresses the existing default format AS a parsed default-format
string fed through the new engine (so fixture `0012` stays green — the 07.1 foundation-slice
regression-equivalence pattern). The **differential is byte-exact and DETERMINISTIC**: a custom
`log_format` over the deterministic operators produces a **byte-identical access-log line on both
proxies**; the only non-determinism (timing / start-time) is handled by the existing
`Iso8601Format`/`DurationMs` per-token allow-list rules that fixture `0012` already proved. It is
**LOCALLY observable** (an access-log file scrape on a normal request/response via the existing
`Driver::Http1WithAccessLog` — no file-watch/reload trigger, NOT Linux-CI-only like phases 26/27).
This pick is on the **critical path**: the command-operator engine is the gating dependency for the
deferred `header_to_metadata`/`set_metadata` HTTP filters (their dynamic-metadata output is
differentially observable only via a future `%DYNAMIC_METADATA%` operator that slots additively into
this engine) and for the rest of the Observability family. See ADR-0078 for the pick rationale and
the rejected alternatives (the next HTTP filter — the easy header-only vein is exhausted and the
remainder are blocked; the remaining LB policies — non-deterministic or HC-dependent; a
config-hardening phase — low differential leverage; JSON access-log format — larger,
key-ordering-sensitive surface, defers).

## §1 — Goal & differential surface

**Goal.** Generalize `envoy-accesslog` into a configurable command-operator formatter and add the
`log_format` text-format config field to `FileAccessLog`, behaviorally equivalent to upstream Envoy
v1.33.0 under the differential contract (§7.2 of `BOOTSTRAP_PROMPT.md`) on the **Access log record**
dimension (semantically equal after field mapping; byte-exact per-token for the deterministic
operators, allow-listed for the timing operators).

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0040-accesslog-command-operators`** (next free number; baseline is `0001`…`0039`): an
  H1 listener whose file access logger configures a **custom `log_format`** string exercising the
  curated deterministic operators in a NON-default order with interleaved plain-text literals (e.g.
  a compact `method=%REQ(:METHOD)% path=%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)% proto=%PROTOCOL%
  code=%RESPONSE_CODE% flags=%RESPONSE_FLAGS% rx=%BYTES_RECEIVED% tx=%BYTES_SENT%
  ua="%REQ(USER-AGENT)%" auth=%REQ(:AUTHORITY)% up=%UPSTREAM_HOST%` — the EXACT operator set,
  literal punctuation, and whether one timing operator (`%START_TIME%`/`%DURATION%`) is included for
  an allow-listed proof are **§6.2-VERIFY / a §3 PLAN-write call**). The driver sends ≥2 probes
  (varying method/headers/path) and asserts the access-log lines are **cross-proxy byte-identical**
  for every deterministic token (`Exact` rule) and present-and-well-formed for any timing token
  (`Iso8601Format`/`DurationMs` rule). Routes to a real backend (the existing `http1-echo-server`)
  so `%UPSTREAM_HOST%`/`%RESP(...)%` are non-trivial, OR uses `direct_response` for a fully
  upstream-independent line — the §6.2 recon picks the shape that is byte-exact cross-proxy
  (`%UPSTREAM_HOST%` on a single-A STRICT_DNS / loopback backend is byte-exact; on multi-A it is
  per-side — the recon confirms).
- **Fixture `0012-access-log-file-sink` stays green UNCHANGED** — the existing default-format
  fixture is the **regression-equivalence witness** that the engine reproduces the hardcoded default
  format byte-for-byte once the default format is re-expressed as a parsed string (the
  foundation-slice-exercised-by-its-consumer proof). Its `envoy-rust.yaml` carries NO `log_format`
  (default format applies); its expectations are unchanged.
- **All 39 pre-existing fixtures `0001`–`0039` stay green simultaneously** — only listeners with a
  file access logger emit a log line at all, and the default-format path is byte-preserved; the
  command-operator engine is inert (default format) for every existing fixture (the load-bearing
  regression proof).

**Conformance:** h2spec pass-rate ≥95% (unchanged — no HTTP/2 codec change). No new conformance
suite. A **dedicated `accesslog_format_parse` fuzz target** over the new command-operator grammar
parser is added (§7.4 — a new parser ships a fuzz target; wire it into `ci.yml` in state-3 per the
new-fuzz-target discipline so the §7.5 gate (d) is met) + a `parse_bootstrap` config seed exercising
a `log_format`-bearing `FileAccessLog`.

## §2 — Scope (minimum-viable)

Per §6.3 (no vague deferral): every capability is either IN this phase and tested, or an explicit
deferred non-goal with its own future home. Exact dispositions marked **§6.2-VERIFY** are
empirically locked at the state-2 PLAN-write (the phase-22/23/28/29/30/31 verify-at-PLAN-write
discipline); this SPEC states the projected shape.

### §2.1 IN scope

1. **The command-operator parser (the engine / correctness gate).** A small parser
   (`crates/envoy-accesslog/`) that compiles a format string into a `Vec` of segments — plain-text
   literals interleaved with parsed command operators. The grammar: `%OPERATOR%`,
   `%OPERATOR(args)%` (args = the operator-specific parameter, e.g. a header name with an optional
   `?`-separated alternate and an optional `:N` max-length truncation: `REQ(NAME?ALT):N`), and a
   literal `%%` escape for a bare `%`. The exact grammar edges (the `?`-alt + `:N` truncation
   placement, `%%` escaping, how an unterminated/empty `%...%` is treated) are **§6.2-VERIFY**. A
   pinned unit oracle is the §A correctness anchor.
2. **The compiled-format evaluator.** Evaluate the compiled segment list against an
   `AccessLogRecord` to produce the log line. The hardcoded `default_format::format()` is
   **refactored** so the default format is expressed as a default-format STRING parsed by the same
   engine (or an equivalent compiled constant) — the default-format output stays **byte-identical**
   (fixture `0012` is the witness).
3. **Config — the `log_format` field.** Add the log-format text-format field to `FileAccessLog`
   (today `{ path }` only; serde `deny_unknown_fields`). The **exact wire path** — modern
   `log_format: { text_format_source: { inline_string: "<fmt>" } }` (the non-deprecated
   `core.v3.SubstitutionFormatString`) vs the deprecated flat `text_format`/`format` string — is
   **§6.2-VERIFY** (the recon picks the path Envoy v1.33.0 accepts and that round-trips through
   `/config_dump`; the modern `log_format.text_format_source.inline_string` is the projected
   primary). Absent `log_format` ⇒ the Envoy default format.
4. **Config validation.** A malformed format string (an unterminated `%...%`, an **unknown
   operator**, or a deferred-but-recognized operator used outside its support) is disposed per
   **§6.2-VERIFY** — projected **boot-fatal** (ADR-0049 all-fatal posture: reject at config-load
   with a new `ConfigError` variant) rather than accept-and-degrade. The recon records whether Envoy
   rejects an unknown `%FOO%` at boot or emits it literally / as empty.
5. **The curated DETERMINISTIC operator set.** Implement, byte-faithful to Envoy v1.33.0 (output
   forms **§6.2-VERIFY**): `%REQ(NAME)%` / `%REQ(NAME?ALT)%` / `%REQ(NAME):N%` (request header, with
   the `?`-alternate-header fallback + `:N` truncation), `%RESP(NAME)%` (response header, same
   grammar), `%PROTOCOL%`, `%RESPONSE_CODE%`, `%RESPONSE_FLAGS%`, `%BYTES_RECEIVED%`, `%BYTES_SENT%`,
   `%UPSTREAM_HOST%`. The pseudo-header request operators (`%REQ(:METHOD)%`, `%REQ(:AUTHORITY)%`,
   `%REQ(:PATH)%`) map onto the record's `method`/`authority`/`path` fields. **NOTE — the
   `AccessLogRecord` carries 15 specific fields, NOT a generic header map**: so `%REQ(NAME)%` /
   `%RESP(NAME)%` are realizable only against a **fixed name→field allow-list** (`:method`→`method`,
   `:authority`→`authority`, `:path`/`x-envoy-original-path`→`path`, `user-agent`→`user_agent`,
   `x-forwarded-for`→`forwarded_for`, `x-request-id`→`request_id`,
   `x-envoy-upstream-service-time`→`upstream_service_time`); a well-formed but **unsupported** header
   name (one with no backing field) is dispositioned at the PLAN-write (§3.3/§3.4 — fold into the
   absent-value / unknown-token call; a generic header map is §2.2-deferred new plumbing). The
   **absent-value rendering** (`-` vs empty string, and the interaction with a future
   `omit_empty_values`) is **§6.2-VERIFY** (today the default-format emitter renders absent optionals
   as `-`).
6. **The allow-listed non-deterministic operators.** `%START_TIME%` (ISO-8601, the existing
   `format_iso8601`) and `%DURATION%` (ms) remain available so the engine is general; in the new
   fixture they are asserted (if used) by the existing `Iso8601Format`/`DurationMs` allow-list rules
   — NOT by `Exact`. Whether `%START_TIME%` honors a format argument (`%START_TIME(%Y...)%`) is
   **§2.2-deferred** (the recon notes it; the default `%START_TIME%` is in scope).
7. **Tests.** Fixture `0040` (the differential above) + fixture `0012` unchanged (the
   regression-equivalence witness) + an in-process backstop (the parser oracle: literal/operator/
   `?`-alt/`:N`-truncation/`%%`-escape segmentation; the evaluator over a synthetic record incl. the
   absent-value rendering; the default-format round-trip byte-equality assertion) + a dedicated
   `accesslog_format_parse` fuzz target (+ its `ci.yml` step) + a `parse_bootstrap` seed with a
   `log_format`-bearing `FileAccessLog` + a BEHAVIOR_CONTRACT "Access log field mapping" extension
   documenting the operator grammar and the deterministic/non-deterministic classification.

### §2.2 DEFERRED non-goals (explicit; each names its future home)

- **`json_format` / `typed_json_format` access-log format** — a larger, key-ordering-sensitive
  surface; its own future Observability phase (the text-format engine is the byte-exact opener).
- **The non-chosen wire path** — whichever of the modern `log_format.text_format_source` /
  deprecated `text_format`/`format` the §6.2 recon does NOT pick; the other slots in additively
  later if a fixture needs it.
- **The `%DYNAMIC_METADATA(namespace:key)%` and `%FILTER_STATE(key)%` operators** — no filter emits
  dynamic metadata or filter state today; these slot additively into the engine once
  `header_to_metadata`/`set_metadata` (the future HTTP-filter-family phase this unlocks) land. THIS
  is the critical-path unlock the phase enables.
- **Operators needing new `AccessLogRecord` plumbing** — `%ROUTE_NAME%`, `%UPSTREAM_CLUSTER%`,
  `%REQUESTED_SERVER_NAME%` (SNI), `%RESPONSE_CODE_DETAILS%`, `%CONNECTION_TERMINATION_DETAILS%`,
  `%TRAILER(NAME)%`, the `%DOWNSTREAM_*ADDRESS%` family — each requires threading new state into the
  record (and several are per-side / non-byte-exact); each is an additive future operator.
- **Per-side address/timing operators beyond the allow-listed proof** — `%REQUEST_DURATION%`,
  `%RESPONSE_DURATION%`, `%RESPONSE_TX_DURATION%`, the downstream/upstream local/remote address
  operators; non-deterministic or per-side, deferred.
- **`SubstitutionFormatString` knobs** — `omit_empty_values`, `content_type`, custom `formatters`
  (the extension-formatter registry), `json_format_options`; deferred.
- **The other Observability-family surfaces** — gRPC ALS, the OTLP access-log sink, OTel/Zipkin/
  Jaeger/Datadog/XRay tracing, stats sinks, the tap filter; each its own future phase.

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)

These are decisions the state-2 PLAN-write makes after the §6.2 reconnaissance; the brainstorm
deliberately leaves them open:

1. **The `FileAccessLog` log-format wire path** — modern `log_format.text_format_source.inline_string`
   vs the deprecated flat `text_format`/`format` string (which does Envoy v1.33.0 accept and
   round-trip through `/config_dump`; which to model first).
2. **The command-operator grammar edges** — the exact `?`-alternate + `:N`-truncation syntax and
   placement (`REQ(NAME?ALT):N`), the `%%` escape, and how an unterminated/empty `%...%` is treated
   (literal vs config-error).
3. **The per-operator output byte forms** — the absent-value rendering (`-` vs empty), the
   truncation semantics (`:N` byte-count vs char-count; whether it applies to the alternate), the
   `%UPSTREAM_HOST%` form on direct_response vs a real backend, the `%RESPONSE_FLAGS%` form.
4. **The unknown/malformed-operator config-validity disposition** — boot-fatal (ADR-0049, projected)
   vs accept-and-emit-literal/empty (Envoy's actual behavior, recorded at the recon).
5. **The fixture-0040 shape** — `direct_response` (fully upstream-independent, byte-exact line) vs a
   real `http1-echo-server` backend (exercises `%UPSTREAM_HOST%`/`%RESP(...)%`); the exact
   `log_format` string; whether to include one allow-listed timing operator; the probe set.
6. **The default-format re-expression mechanism** — re-express the default as a parsed format STRING
   vs a compiled constant segment list; whichever keeps fixture `0012` byte-identical with the least
   risk (the recon/PLAN confirms the default format string verbatim).
7. **The harness comparator** — extend `tests/differential` with a GENERIC custom-format comparator
   (the existing `tokenize_default_format` is default-specific): either a whole-line `Exact` compare
   when the format uses only deterministic operators, or a configured per-segment rule list mirroring
   the `0012` `AccessLogLineRule` approach for a format that mixes deterministic + timing operators.
8. **The dedicated `accesslog_format_parse` fuzz target** scope (the format-string parser surface) +
   its `ci.yml` wiring step.
9. **The §6.1 split decision** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)

- **The `envoy-accesslog` crate** (`crates/envoy-accesslog/`: `record.rs` the 15-field
  `AccessLogRecord`, `default_format.rs` the hardcoded emitter + `format_iso8601`, `file_sink.rs` the
  async `FileSink`, `error.rs`) — the engine generalizes `default_format.rs`; the record, the
  ISO-8601 helper, and the sink are reused wholesale.
- **The H1 + H2 HCM on-response-complete access-log wiring** (`crates/envoy-http1/src/hcm.rs` ~1184
  + `crates/envoy-http2/src/hcm.rs` ~886 — populate the record, fire-and-forget to each sink) — the
  log line is produced from the SAME record; the only change is the sink formats via the compiled
  format instead of the hardcoded function. No new request/response state is threaded this phase
  (the operators that would need it are §2.2-deferred).
- **The `FileAccessLog` config + `validate_access_logs` validator + the `AccessLogTypedConfig` enum
  + the access-log `ConfigError` variants** (`crates/envoy-config/src/bootstrap.rs`) — extend with
  the `log_format` field + a format-string validator + one new `ConfigError` variant.
- **The differential harness access-log path** — `Driver::Http1WithAccessLog`, the
  `AccessLogLineRule` enum (`Exact`/`Iso8601Format`/`DurationMs`/`Wildcard`), the
  `tests/differential/src/access_log.rs` tokenizer + `assert_access_log_lines_equivalent`, and the
  `tests/fixtures/0012-access-log-file-sink/` structure — the template + the rule vocabulary for
  fixture `0040`; the generic custom-format comparator is the one harness addition.
- **The `parse_bootstrap` fuzz corpus** (incl. `hcm_access_log_file.yaml`) + the BEHAVIOR_CONTRACT
  "Access log field mapping" section (the 14 default-format token rows) — extend both.

## §5 — Behavioral contract notes

- **Determinism / byte-exactness (the strong target):** the same request/response → the same
  rendered log line on each proxy AND (strong target) the **same line on BOTH proxies** for every
  deterministic operator, because every deterministic operator's output is a function of
  wire-identical request/response state (method, path, headers, status, body byte counts) and static
  config — none of the per-side host-address / clock dependence of the timing operators (handled by
  the allow-list) or the LB cookie / consistent-hash cases.
- **Regression-equivalence (the load-bearing proof):** the default-format output is byte-preserved —
  fixture `0012` stays green UNCHANGED, and all 39 existing fixtures stay green (the engine is inert
  / default for every listener without a `log_format`). The default format is re-expressed through
  the engine without changing a single output byte.
- **Config validity:** a malformed/unknown-operator `log_format` is a startup-fatal parse error
  where §6.2 shows Envoy rejects (ADR-0049 all-fatal; no reload path this phase).
- **Differential locality:** the access-log line is observable WITHOUT a file-watch/reload trigger
  (the file-sink scrape on a normal request/response) → the fixture-`0040` differential runs and is
  authoritative on this Docker-Desktop host (NOT Linux-CI-only, unlike phases 26/27).

## §6 — Process

### §6.1 — Split projection (§6.1 gate)

A split is projected **NOT to fire**. The surface is one parser + one config field + the evaluator +
one fixture + one fuzz target — comparable to `cdn_loop` (~600–900 LoC) / `csrf` (~485) / `buffer`
(~267); estimate ~700–1100 LoC / ~7–9 tasks, well under the ~1500-LoC / ~25-task gate. **ADR-0080 is
reserved** for the split (fires only if the command-operator grammar / the default-format
re-expression proves far gnarlier than projected); the natural seam, if forced, is `32.1` (the
parser + compiled-format evaluator + the `log_format` config field + validation + the default-format
re-expression + backstop — a foundation slice, no new fixture, fixture `0012` the witness) / `32.2`
(the deterministic-operator wiring + fixture `0040` + the harness comparator + the fuzz target +
BEHAVIOR_CONTRACT + close).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)

Like phases 22/23/28/29/30/31 (and unlike phases 26/27), this phase's behavior is **locally
observable** (no reload trigger). At the state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0` with
an H1 listener + a file access logger configured with a custom `log_format`, and:
1. RECORD the **wire path** that v1.33.0 accepts (modern `log_format.text_format_source.inline_string`
   vs deprecated `text_format`/`format`) and how it round-trips through `/config_dump`.
2. RECORD the **per-operator output byte forms** for the curated set (`%REQ(...)%` with `?`-alt + `:N`
   truncation, `%RESP(...)%`, `%PROTOCOL%`, `%RESPONSE_CODE%`, `%RESPONSE_FLAGS%`, `%BYTES_RECEIVED%`,
   `%BYTES_SENT%`, `%UPSTREAM_HOST%`), the **absent-value rendering** (`-` vs empty), the truncation
   semantics, and the `%%`/unterminated-`%`/unknown-operator handling (literal vs config-fatal).
3. RECORD the config-validity disposition (an unknown `%FOO%` / a malformed format — boot-fatal vs
   accepted), and confirm the access-log line for a fixed request/response is **byte-identical**
   between a hand-rolled replica and live Envoy across ≥2 probes.
4. Decide STRONG (cross-proxy byte-identical for the deterministic operators — expected); record a
   fallback allow-list rule only if some operator proves non-portable (e.g. `%UPSTREAM_HOST%` on
   multi-A).
**ADR-0079 FIRES** at the PLAN-write if any of these materially diverge from this SPEC's projection
(notably the wire path, the absent-value rendering, the truncation/`?`-alt grammar, or the
config-validity disposition). `PLAN.md` lands with the empirically-locked facts inline (no
`[§6.2-PENDING]` projections — the verify-at-PLAN-write discipline), and re-confirms the default
format STRING verbatim so the re-expression is byte-exact.

### §6.3 — Anti-deferral

No vague TODOs. Every §2.1 item is implemented + tested this phase; every deferral is a §2.2 named
non-goal with a future home. The parser, the evaluator, the config field, the deterministic operator
set, the fixture, and the fuzz target are real and differentially exercised — no stubs. The
default-format re-expression is proven byte-identical by fixture `0012` staying green unchanged.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)

(a) fixture `0040` green + (b) all of `0001`–`0039` green (incl. `0012` UNCHANGED — the
regression-equivalence witness) + (c) h2spec ≥95% + (d) the new `accesslog_format_parse` fuzz target
(wired into `ci.yml`) + the `parse_bootstrap` seed clean for the short-budget CI run + (e) `cargo
build --workspace --all-targets` / `cargo clippy --workspace --all-targets --all-features -- -D
warnings` / `cargo fmt --all -- --check` / `cargo test --workspace` / `cargo deny check` all clean +
(f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds (D-3.8).

---

_Scope locked by **ADR-0078**. ADR-0079 reserved (§6.2 reconciliation), ADR-0080 reserved (§6.1
split). The state-2 PLAN-write is the next session (`superpowers:writing-plans`)._
