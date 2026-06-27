# Phase 42 — `42-accesslog-response-code-details` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored via `superpowers:brainstorming`; the project is
> autonomous (`feedback_pick_recommendation` — converge + record the pick in an ADR, no human gate). This
> SPEC is the requirements contract; `PLAN.md` (the state-2 step, the NEXT session) turns it into TDD tasks.
> **The pick was existence-verified at state-1** (the phase-41 lesson) against live `envoyproxy/envoy:v1.33.0`
> BEFORE locking — see §6.2.

## §0 — One-paragraph summary

**Add the `%RESPONSE_CODE_DETAILS%` access-log command operator — the response-code-details string Envoy
attaches to the response (e.g. `via_upstream` for an upstream-routed reply, `direct_response` for a
`direct_response` route).** Phases 32/38/39/40/41 built the access-log command-operator engine + the text/json
encoders + the "operator backed by a new record field" sub-vein (`%ROUTE_NAME%`, phase 41). `%RESPONSE_CODE_DETAILS%`
is the next cheapest-strong VALID leaf in that sub-vein: a new `AccessLogRecord.response_code_details:
Option<String>` field (set by the HCM when the response is produced), a new `Op::ResponseCodeDetails`
variant (a no-arg keyword like `%RESPONSE_CODE%`/`%ROUTE_NAME%`), its `render_op` arm, and its
`encode_single_op` arm — **all mirroring the EXISTING `%UPSTREAM_HOST%`/`%ROUTE_NAME%`** (`Option<String>`
→ `render_op` `unwrap_or("-")`, `encode_single_op` `quote_opt`). Config/flow-deterministic, byte-exact.

**`%RESPONSE_CODE_DETAILS%` is the cheapest-strong VALID next leaf** (after the leading candidate
`%VIRTUAL_HOST_NAME%` proved NON-EXISTENT at v1.33.0 — §6.2). It re-uses the ENTIRE command-operator engine
+ the text/json encoders + the harness; the ONLY new code is one `Option<String>` record field + the HCM
plumbing (one assignment on the response-produced path) + the new `Op` (mirroring `Op::UpstreamHost`).
**NO new connection plumbing, NO new request attribute, NO new crate/dependency/fuzz-target, NO new
`HttpFilterInstance` variant**, and projected NO new `ConfigError` variant. **The detail-string vocabulary is
deliberately BOUNDED** to the deterministic value(s) the §2 fixture exercises (the `via_upstream`
upstream-routed-success path — see §2.1) — this is precisely how the pick stays cheapest-strong and avoids
the "rabbit hole of detail strings" the phase-41 ADR-0098 §B flagged. The full vocabulary (the local-reply
details, `route_not_found`, the many filter-generated details) is explicit future work (§2.2).

**§6.2 FACTS (recon-LOCKED this state-1, captured live against `envoyproxy/envoy:v1.33.0`):** an
upstream-routed `200` → `%RESPONSE_CODE_DETAILS%` renders `via_upstream` (json single-op → quoted
`"via_upstream"`; mixed → `d=via_upstream`); a `direct_response` route → `direct_response`. I.e. an
always-present-on-completion `String` that the MVP models as `Option<String>` (→ `quote_opt`/`unwrap_or("-")`),
EXACTLY like `%UPSTREAM_HOST%`/`%ROUTE_NAME%`.

## §1 — Goal & differential surface
**Goal.** Add `%RESPONSE_CODE_DETAILS%` to the access-log command-operator engine, behaviorally equivalent to
upstream Envoy v1.33.0 under the differential contract (§7.2) on the **Access log records** dimension —
byte-exact whole-line for the curated deterministic set.

**Differential surface at phase end:**
- **Fixture `0050-accesslog-response-code-details`** (next free; baseline `0001`…`0049`): an H1 listener
  whose route forwards to the harness upstream backend; the file logger's format contains
  `%RESPONSE_CODE_DETAILS%`. The driver issues a request; the emitted line shows the detail string
  (`via_upstream`, the upstream-routed-success path), byte-exact cross-proxy. **PLAN-VERIFY** whether
  envoy-rust ALSO supports a `direct_response` route cheaply enough to additionally witness the
  `direct_response` value (a stronger two-value differential) — if so, fold it in; else it is a §2.2 deferral.
- **All `0001`–`0049` stay green simultaneously** — `%RESPONSE_CODE_DETAILS%` is a NEW operator + a NEW record
  field defaulting absent; no existing fixture uses it; the existing render paths + record construction are
  byte-preserved (the new field is `Option<String>` defaulting `None`, appended last).

**Conformance:** h2spec ≥95% (unchanged — NO HTTP/2 codec change). Fuzz: the operator reuses
`accesslog_format_parse`/`parse_bootstrap`; add a `%RESPONSE_CODE_DETAILS%` seed. NO new fuzz target projected.

## §2 — Scope (minimum-viable)
### §2.1 IN scope
1. **The `response_code_details` record field.** Add `pub response_code_details: Option<String>` to
   `AccessLogRecord` (`crates/envoy-accesslog/src/record.rs`), mirroring `route_name`/`upstream_host:
   Option<String>`. Default `None`.
2. **The HCM response-code-details plumbing.** Where the HCM produces the response + builds the
   `AccessLogRecord` (`crates/envoy-http1/src/hcm.rs` — the record-construction site, and the H2 equivalent),
   set `response_code_details` to the detail string for the path taken: on the **upstream-routed success
   path** → `Some("via_upstream")` (the §6.2-locked value). **PLAN-VERIFY** the exact set of paths envoy-rust
   takes for the fixture(s) and which detail string each yields; any path not exercised by a §2.1 fixture is
   a §2.2 deferral. (The H2 record is built in a separate emit-fn that takes log values as PARAMETERS — thread
   a new `response_code_details_for_log_h2` param, mirroring the phase-41 `route_name_for_log_h2` plumbing.)
3. **The `Op::ResponseCodeDetails` operator.** Add `Op::ResponseCodeDetails` to the `Op` enum
   (`command_operator.rs`), a `"RESPONSE_CODE_DETAILS"` no-arg keyword dispatch (mirroring
   `%RESPONSE_CODE%`/`%ROUTE_NAME%`; a `(...)`/`:N` suffix is **PLAN-VERIFY** — projected no-arg like the
   other non-header operators), a render arm (`record.response_code_details.as_deref().unwrap_or("-")` in
   `render_op`, mirroring `Op::RouteName`), and an `encode_single_op` arm (`quote_opt(out,
   record.response_code_details.as_deref())` — present→quoted, absent→`null`, mirroring `Op::RouteName` at
   `json_format.rs`).
4. **Tests.** Fixture `0050` (byte-exact, the `via_upstream` line) + all `0001`–`0049` unchanged + an
   in-process backstop: present→the detail string (text + json single-op quoted + mixed); the
   record-default-`None` round-trip → `-` sentinel (text/mixed) / `null` (json single-op). Plus an
   `accesslog_format_parse`/`parse_bootstrap` seed and a BEHAVIOR_CONTRACT "Access log field mapping"
   `%RESPONSE_CODE_DETAILS%` note.

### §2.2 DEFERRED non-goals
- **`%VIRTUAL_HOST_NAME%`** — NOT a v1.33.0 access-log operator (§6.2; `Not supported field in StreamInfo:
  VIRTUAL_HOST_NAME`); VOID, removed from contention (it was the leading candidate; the existence-check
  vetoed it BEFORE the SPEC locked — the phase-41 lesson applied).
- **The full `response_code_details` vocabulary** — the local-reply details, `route_not_found`, the many
  filter-generated detail strings: the MVP renders ONLY the deterministic value(s) the §2.1 fixture(s)
  exercise (`via_upstream`; possibly `direct_response`). Each additional detail-string path is future work
  (set it as envoy-rust grows the corresponding response path). The operator ITSELF is complete; only the
  set of distinct values envoy-rust can produce is bounded.
- **`%UPSTREAM_CLUSTER%` / `%RESPONSE_CODE_DETAILS%`-adjacent / `%REQUEST_HEADERS_BYTES%` /
  `%ACCESS_LOG_TYPE%`** — other recon-VALID operators (each needs its own new record field / data); each its
  own future phase (this phase does ONE: `%RESPONSE_CODE_DETAILS%`).
- **`sort_properties`/`content_type`, CF-39-1, the gRPC-ALS/OTLP/tracing/tap surfaces** — unchanged future homes.

## §3 — Open PLAN-write design calls (resolved at state-2)
1. **The HCM response-path → detail-string mapping** — confirm (via the state-2 §6.2 recon + a read of the
   HCM) which response paths envoy-rust takes for the fixture(s) and the exact detail string each yields
   (`via_upstream` for the upstream-routed success; whether a `direct_response` route is cheaply witnessable).
2. **The operator suffix grammar** — confirm `%RESPONSE_CODE_DETAILS%` is no-arg (no `(...)`, no `:N`) at
   v1.33.0 (projected; a `:N` truncate is **PLAN-VERIFY**).
3. **The H2 record-construction site** — thread `response_code_details_for_log_h2` as a new emit-fn parameter
   (mirror the phase-41 `route_name_for_log_h2` precedent), NOT "set it alongside `upstream_host`".
4. **The fixture-0050 shape** (upstream backend reuse; optional second `direct_response` witness) + the fuzz
   seed — §3 PLAN-write calls.
5. **The §6.1 split** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)
- **The command-operator engine** (`command_operator.rs`: the `Op` enum; the no-arg keyword dispatch
  [`%RESPONSE_CODE%`/`%ROUTE_NAME%` precedents]; `render_op`/`render_value_segments`; the `-` sentinel) — add
  ONE `Op::ResponseCodeDetails` variant + its keyword + its render arm, mirroring `Op::RouteName`.
- **The `Op::RouteName`/`Op::UpstreamHost` precedent** (an `Option<String>` record field → `render_op`
  `unwrap_or("-")` + `encode_single_op` `quote_opt`) — `%RESPONSE_CODE_DETAILS%` is the SAME pattern; copy it.
- **The `AccessLogRecord`** (`record.rs`: `route_name`/`upstream_host: Option<String>` to mirror) — add
  `response_code_details`.
- **The HCM record construction** (`crates/envoy-http1/src/hcm.rs` + the H2 equivalent + the phase-41
  `route_name_for_log_h2` parameter-threading precedent) — set `response_code_details` on the response-produced
  path.
- **The text/json encoders + harness** (`Driver::Http1WithAccessLog`/`AccessLogByteExactProbe`) — the
  `0040`/`0046`/`0047`/`0048`/`0049` template for fixture `0050`. UNCHANGED.
- **The fuzz corpora + BEHAVIOR_CONTRACT** — extend; no new fuzz target.

## §5 — Behavioral contract notes
- **The new axis (one operator + one record field, flow-deterministic):** `%RESPONSE_CODE_DETAILS%` reads the
  detail string Envoy attaches when the response is produced — deterministic for a fixed request/route path,
  byte-exact.
- **Mirrors `%ROUTE_NAME%`/`%UPSTREAM_HOST%`:** an `Option<String>` → present quoted/rendered, absent → `null`
  (json single-op) / `-` sentinel (text/mixed). No new rendering machinery.
- **Default-absent byte-preservation (the load-bearing proof):** the new `response_code_details` field
  defaults `None` and the operator is new → all `0001`-`0049` stay byte-identical.
- **Determinism / locality:** the line is a function ONLY of the (fixed) request + the route/response path;
  observable on a normal request/response → fixture `0050` is authoritative on this host (NOT Linux-CI-only).
- **Config validity:** an unknown operator stays boot-fatal via the EXISTING `parse_format` (no new
  variant). All-fatal posture unchanged (ADR-0049).

## §6 — Process
### §6.1 — Split projection
NOT to fire. ONE record field + ONE `Op` variant (mirroring `Op::RouteName`) + the HCM plumbing (H1 + the H2
parameter-thread) + one fixture + backstop + seed + a BEHAVIOR_CONTRACT note. **~120–250 LoC / ~5–6 tasks** —
under the §6.1 gate. **ADR-0100 reserved** for the split (projected NOT to fire).

### §6.2 — Empirical reconnaissance (the EXISTENCE-CHECK ran at THIS state-1; the deep recon is state-2)
Per the phase-41 lesson (`%REQ_WITHOUT_QUERY%` proved VOID only at state-2, forcing a mid-state-2 pivot), the
state-1 brainstorm ran a LIVE existence-check against `envoyproxy/envoy:v1.33.0` (fresh host ports) BEFORE
locking the pick:
- **`%VIRTUAL_HOST_NAME%` (the leading candidate) is VOID** — the config boot-fatals `error initializing
  config: Not supported field in StreamInfo: VIRTUAL_HOST_NAME`. → vetoed; pivoted to the fallback.
- **`%RESPONSE_CODE_DETAILS%` is VALID + DETERMINISTIC** — boots; an upstream-routed `200` → `via_upstream`
  (json single-op quoted `"via_upstream"`; mixed `d=via_upstream`); a `direct_response` route →
  `direct_response`. → the locked pick.

The state-2 §6.2 recon (at PLAN-write) pins the remaining details: the suffix grammar (no-arg / `:N`), the
exact HCM-path → detail-string mapping for the fixture(s), and whether a `direct_response` second witness is
cheap. **ADR-0099 FIRES at THIS state-1** (the pick + the existence-veto of `%VIRTUAL_HOST_NAME%` + the
locked `%RESPONSE_CODE_DETAILS%` facts).

### §6.3 — Anti-deferral
No vague TODOs. Every §2.1 item is implemented + tested; every deferral is a §2.2 named non-goal.

## §7 — Acceptance (the §7.5 gate, previewed)
(a) fixture `0050` green (byte-identical response-code-details line) + (b) all `0001`-`0049` green + (c)
h2spec ≥95% + (d) `accesslog_format_parse`/`parse_bootstrap` fuzz clean (with the `%RESPONSE_CODE_DETAILS%`
seed) — NO new target + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved.
`#![forbid(unsafe_code)]` holds; NO new crate/dependency; projected NO new `ConfigError` variant; ONE new
`AccessLogRecord` field (`response_code_details`).

---

_Pick locked by **ADR-0099** (phase-42 state-1 brainstorm): `%VIRTUAL_HOST_NAME%` VOID at v1.33.0 → pivot to
`%RESPONSE_CODE_DETAILS%` (existence-verified VALID this state-1). The §6.1 split is projected NOT to fire
(**ADR-0100 reserved**). `PLAN.md` is authored the NEXT session (state-2) against the ADR-0099-locked facts +
the state-2 §6.2 recon. §5.1: one state per session — this session STOPS at the SPEC + ROADMAP row + ADR +
STATE advance._
