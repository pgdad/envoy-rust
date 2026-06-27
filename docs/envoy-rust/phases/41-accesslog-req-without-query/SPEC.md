# Phase 41 — `41-accesslog-req-without-query` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0097** (the phase-41 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

**Add the `%REQ_WITHOUT_QUERY(NAME[?ALT])[:N]%` access-log command operator — like `%REQ%` but with the
`?query` string removed from the resolved value.** Phases 32/38/39/40 built the access-log command-operator
engine (`crates/envoy-accesslog/src/command_operator.rs`) + the text/json/recursive-json encoders + the
`omit_empty_values` knob over a fixed operator set (`%REQ%`, `%RESP%`, `%PROTOCOL%`, `%RESPONSE_CODE%`, …).
Envoy's access-log command-operator vocabulary also includes `REQ_WITHOUT_QUERY` — identical grammar to
`REQ` (a header name, an optional `?ALT` fallback, an optional `:N` byte-truncation) but the substituted
value has everything from the first `?` onward stripped (designed for `%REQ_WITHOUT_QUERY(:PATH)%`, which
emits the request path without its query string). This phase adds it: a new `Op::ReqWithoutQuery` variant
parsed by the EXISTING `parse_header_op` machinery, resolved by the EXISTING `resolve_req`, with a trivial
query-strip applied to the result.

**`%REQ_WITHOUT_QUERY%` is the cheapest-strong next leaf** — a single new operator over an engine + record
+ encoders + harness that ALL already exist. There is **NO new `AccessLogRecord` field, NO new connection
plumbing, NO new request attribute, NO new crate or dependency, NO new `HttpFilterInstance` variant**, and
projected NO new `ConfigError` variant — the value it strips (`record.path`, the `:path`/request-target)
is already in the record, and the query-strip is a one-line substring-before-`?` (the phase-37
`strip_query` precedent, `crates/envoy-filter/src/rbac.rs:96`). By contrast `json_format_options.
sort_properties` risks being an empty/no-op differential (the observed v1.33.0 default is already sorted),
the RBAC connection-context conditions need socket-address/SNI plumbing AND produce an environment-fragile
IP differential, and the gRPC-ALS/OTLP sinks need new transport infrastructure. See ADR-0097 for the pick
rationale and rejected alternatives.

**The differential is byte-exact, DETERMINISTIC, and LOCALLY observable** (an access-log file scrape on a
normal request/response, no file-watch/reload trigger — NOT Linux-CI-only): the probe drives a fixed
request with a query (`GET /p?x=1&y=2`) → a fixed `AccessLogRecord` → a byte-identical access-log line where
`%REQ_WITHOUT_QUERY(:PATH)%` renders `/p` (query stripped) on both proxies. The **load-bearing differential
richness is the query-strip semantics + its interaction with `:N` truncation and `?ALT` fallback**: the
fixture forces envoy-rust to replicate Envoy's exact strip point (first `?`), the strip-vs-truncate ORDER,
and the absent-value rendering — distinct from the existing `%REQ(:PATH)%` (which keeps the query).

## §1 — Goal & differential surface

**Goal.** Add the `%REQ_WITHOUT_QUERY%` operator to the access-log command-operator engine, behaviorally
equivalent to upstream Envoy v1.33.0 under the differential contract (§7.2 of `BOOTSTRAP_PROMPT.md`) on the
**Access log records** dimension — sharpened (as in phases 32/38/39/40) to **byte-exact whole-line** for the
curated deterministic operator set. The line a request produces is a deterministic function of the (fixed)
probe request + the static format config.

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0049-accesslog-req-without-query`** (next free number; baseline is `0001`…`0048`): an H1
  listener whose file access logger uses a format (text and/or json) containing `%REQ_WITHOUT_QUERY(:PATH)%`
  alongside a plain `%REQ(:PATH)%` (so the line shows BOTH the stripped and the full path). The driver
  issues `GET /p?x=1&y=2`; the emitted line is compared **byte-exact** cross-proxy: `%REQ_WITHOUT_QUERY
  (:PATH)%`→`/p`, `%REQ(:PATH)%`→`/p?x=1&y=2`. **§6.2-VERIFY** the exact strip point + the strip/truncate
  order (§3.2).
- **All `0001`–`0048` stay green simultaneously** — `%REQ_WITHOUT_QUERY%` is a NEW operator that no existing
  fixture uses; every existing access-log fixture (`0012` default, `0040` text-custom, `0046`/`0047` json,
  `0048` omit-empty) is unaffected and byte-identical. The existing operator render paths are byte-preserved.

**Conformance:** h2spec ≥95% (unchanged — no HTTP/2 codec change). Fuzz: the new operator's format string
reuses the EXISTING `accesslog_format_parse` tokenizer + `parse_bootstrap` config path; add a seed
exercising `%REQ_WITHOUT_QUERY(...)%`. NO new fuzz target projected (a §3 PLAN-write call).

## §2 — Scope (minimum-viable)

### §2.1 IN scope
1. **The `Op::ReqWithoutQuery` parse.** Add `Op::ReqWithoutQuery { name, alt, truncate }` (mirroring
   `Op::Req`, `command_operator.rs:40`). The `%REQ_WITHOUT_QUERY(...)%` keyword is parsed by the EXISTING
   `parse_header_op` grammar (the `(NAME[?ALT])[:N]` parse, the `REQ_ALLOW_LIST` validation — **§6.2-VERIFY**
   whether Envoy applies the same allow-list / whether non-`:path` names are accepted) — projected: add a
   keyword dispatch (`command_operator.rs:231` region, `"REQ" => …`) for `"REQ_WITHOUT_QUERY"` producing the
   new variant.
2. **The `Op::ReqWithoutQuery` render.** Resolve the header via the EXISTING `resolve_req` (with the `?ALT`
   fallback), STRIP the query (everything from the first `?`), then apply the `:N` byte-truncation.
   **§6.2-VERIFY** (§3.2): (a) the exact strip point (first `?` — projected); (b) the strip-vs-truncate
   ORDER (strip THEN truncate — projected); (c) whether the strip applies to the `?ALT` value too
   (projected yes — to whatever resolves); (d) the absent-value rendering (the `-` sentinel / `null` single
   typed — reuses the existing absent path); (e) the json single-operator TYPED classification (a string
   operator → quoted; absent → `null`) — reuses `encode_single_op` (projected: a NEW arm there for
   `ReqWithoutQuery`, mirroring `Req`).
3. **Tests.** Fixture `0049` (the byte-exact differential above) + all `0001`–`0048` unchanged + an
   in-process backstop: the strip on a `?`-bearing value; a no-`?` value (strip is a no-op); the `:N`
   truncation AFTER strip; the `?ALT` fallback then strip; absent → sentinel/null; the operator inside text
   AND json (single-op typed → quoted; multi-segment). Plus an `accesslog_format_parse`/`parse_bootstrap`
   seed and a BEHAVIOR_CONTRACT "Access log field mapping" `%REQ_WITHOUT_QUERY%` note.

### §2.2 DEFERRED non-goals (explicit; each names its future home)
- **`%RESP_WITHOUT_QUERY%`** — there is no response-side analogue in Envoy (responses have no query); N/A.
- **`json_format_options.sort_properties` / `content_type`** — the remaining `SubstitutionFormatString`
  knobs; their own future phases (sort_properties pending a §6.2 confirmation that it is observable).
- **CF-39-1 numeric-literal `json_format` leaves** — the protobuf-`double` formatting; unchanged carry-fwd.
- **Other not-yet-implemented operators** (`%ROUTE_NAME%`, `%UPSTREAM_CLUSTER%`, `%RESPONSE_CODE_DETAILS%`,
  `%GRPC_STATUS%`, `%START_TIME(fmt)%`, address operators, …) — each needs a new `AccessLogRecord` field /
  non-determinism handling; each its own future phase.
- **The other Observability surfaces** (gRPC ALS, OTLP, tracing, stats sinks, tap) — each its own future
  phase.

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)
1. **The strip point + order** (THE §6.2 item) — confirm Envoy strips at the FIRST `?` and applies `:N`
   truncation AFTER the strip (vs before). Pin against `GET /p?x=1` + a `%REQ_WITHOUT_QUERY(:PATH):3%` probe.
2. **The header scope + allow-list** — confirm whether `%REQ_WITHOUT_QUERY(NAME)%` for a NON-`:path` header
   is accepted (and just strips any `?` from its value) or restricted; confirm the `REQ_ALLOW_LIST` reuse.
3. **The `?ALT` interaction** — confirm the strip applies to the alt value when the primary is absent.
4. **The absent + json-typed disposition** — absent → the `-` sentinel (multi-segment) / `null` (json
   single-op); a present value → quoted string (json single-op). Reuses the existing `encode_single_op`
   classification (a new `ReqWithoutQuery` arm).
5. **The parse factoring** — a keyword dispatch reusing `parse_header_op` (projected) vs a dedicated parser;
   and whether a no-arg `%REQ_WITHOUT_QUERY%` (no `(...)`) is boot-fatal (projected yes, like `%REQ%`).
6. **The fixture-0049 shape** + the fuzz seed (no new target) — §3 PLAN-write calls.
7. **The §6.1 split decision** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)
- **The command-operator engine** (`command_operator.rs`: the `Op` enum `:36`; `Op::Req { name, alt,
  truncate }` `:40`; `parse_header_op` + the `(NAME[?ALT])[:N]` grammar + `REQ_ALLOW_LIST` `:78`; the keyword
  dispatch `:231`; `resolve_req` `:549`; `render_value_segments`/`render_op`; the `-` sentinel; the `:N`
  truncation `truncate_bytes`) — phase 41 ADDS one `Op` variant + its parse keyword + its render arm,
  REUSING `parse_header_op`/`resolve_req`/`truncate_bytes` verbatim; the query-strip is a new one-liner.
- **The phase-38/39 json encoder** (`json_format.rs`: `encode_single_op` `:224` — the typed classifier,
  with the `Op::Req` arm at `:253-262` to mirror; `encode_json_value`) — phase 41 adds a `ReqWithoutQuery`
  arm to `encode_single_op` (a string operator → quoted via `quote_opt`; absent → `null`), mirroring the
  `Req` arm. UNCHANGED otherwise.
- **The `AccessLogRecord`** (`record.rs`: `path` `:38` = the request-target/`:path`, WHICH INCLUDES the
  query) — READ verbatim; the strip happens at render. NO new field.
- **The query-strip precedent** (`crates/envoy-filter/src/rbac.rs:96` `strip_query` — substring before the
  first `?`, ADR-0090 §B) — the same trivial logic, re-implemented in `envoy-accesslog` (the crates are
  separate; do NOT add a cross-crate dependency — copy the one-liner).
- **The text/json encoders + `FileSink` + the differential harness** (`Driver::Http1WithAccessLog`,
  `AccessLogByteExactProbe`) — the `0040`/`0046`/`0047`/`0048` template for fixture `0049`. UNCHANGED.
- **The `accesslog_format_parse` + `parse_bootstrap` fuzz corpora + the BEHAVIOR_CONTRACT** — extend; no
  new fuzz target projected.

## §5 — Behavioral contract notes
- **The new axis (one operator, not new data):** phase 41 adds a new RENDERING of an EXISTING record field
  (`record.path` with the query stripped) — it reads no new request attribute and stores no new state.
- **The query-strip byte-exactness:** envoy-rust must strip at EXACTLY Envoy's strip point (the first `?`)
  and order strip-vs-truncate identically; the fixture's whole-line byte-exact compare (showing
  `%REQ_WITHOUT_QUERY(:PATH)%`→`/p` beside `%REQ(:PATH)%`→`/p?x=1&y=2`) is the cross-proxy guard.
- **Reuse, not reinvention:** the `(NAME[?ALT])[:N]` parse, the `resolve_req` resolution, the `:N`
  truncation, the absent-value sentinel, and the json typed classification are all the existing engine; only
  the variant + the query-strip + one parse-keyword + one `encode_single_op` arm are new.
- **Determinism / locality:** every line is a function ONLY of the (fixed) probe + the static config;
  observable on a normal request/response WITHOUT a reload trigger → fixture `0049` is authoritative on this
  Docker-Desktop host (NOT Linux-CI-only).
- **Regression-equivalence (the load-bearing proof):** `%REQ_WITHOUT_QUERY%` is a NEW operator no existing
  fixture/format uses; the existing operator render paths are byte-preserved → all `0001`-`0048` stay green.
- **Config validity:** a malformed `%REQ_WITHOUT_QUERY%` (no `(...)`, or an unknown header name if the
  allow-list applies) is boot-fatal via the EXISTING `parse_format` → `ConfigError::InvalidAccessLogFormat`
  (projected NO new variant). All-fatal posture unchanged (ADR-0049).

## §6 — Process
### §6.1 — Split projection (§6.1 gate)
A split is projected **NOT to fire**. The surface is ONE new `Op` variant + its parse keyword + its render
arm (resolve→strip→truncate) + one `encode_single_op` arm + one fixture (`0049`) + the backstop + a fuzz
seed + a BEHAVIOR_CONTRACT note. Estimate **~120–250 LoC / ~5–6 tasks** — well under the ~1500-LoC /
~25-task gate, comparable to / SMALLER than phase 40. **ADR-0098 reserved** for the §6.2 reconciliation;
**ADR-0099 reserved** for the split (projected NOT to fire — fires only if §6.2 reveals the
strip/truncate/allow-list semantics are far gnarlier than projected).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)
Like phases 32/38/39/40, locally observable. At the state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0`
with an H1 listener + a file logger using `%REQ_WITHOUT_QUERY(:PATH)%` (+ a `:N` variant + a non-`:path`
header variant), drive `GET /p?x=1&y=2`, and RECORD: (1) the **strip point** (first `?`); (2) the
**strip-vs-truncate order** (a `%REQ_WITHOUT_QUERY(:PATH):3%` on `/p?x=1` → `/p` [strip then truncate to 3]
vs `/p?` [truncate raw to 3]); (3) the **header scope / allow-list** (is a non-`:path` name accepted?); (4)
the **`?ALT`** strip; (5) the **absent + json-typed** rendering. **ADR-0098 FIRES** at the PLAN-write if any
materially diverge. `PLAN.md` lands with the locked facts inline (no `[§6.2-PENDING]` projections).

### §6.3 — Anti-deferral
No vague TODOs. Every §2.1 item is implemented + tested; every deferral is a §2.2 named non-goal. The
operator parse, render, fixture, and backstop are real and differentially exercised. The regression
equivalence is proven by all `0001`-`0048` staying green.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)
(a) fixture `0049` green (cross-proxy byte-identical line showing the query-stripped path beside the full
path) + (b) all `0001`-`0048` green + (c) h2spec ≥95% (unchanged) + (d) the `accesslog_format_parse` +
`parse_bootstrap` fuzz targets clean for the short-budget CI run (with the `%REQ_WITHOUT_QUERY%` seed) — NO
new fuzz target + (e) `cargo build`/`clippy -D warnings`/`fmt --check`/`test --workspace`/`deny check` all
clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds (D-3.8). No new crate, no new dependency
(D-3.2); projected NO new `ConfigError` variant; NO new `AccessLogRecord` field.

---

_Scope locked by **ADR-0097**. **ADR-0098 reserved** for the §6.2 reconciliation (state-2 PLAN-write). The
§6.1 split is projected NOT to fire (**ADR-0099 reserved**). The state-2 PLAN-write is the next session
(`superpowers:writing-plans`), which runs the §6.2 empirical reconnaissance against live
`envoyproxy/envoy:v1.33.0` and fires ADR-0098._
