# Phase 41 — `41-accesslog-route-name` — SPEC

> **Lifecycle state 1 (brainstorm output), RE-SCOPED at state-2.** Originally `41-accesslog-req-without-query`
> (ADR-0097). The state-2 §6.2 reconnaissance found `%REQ_WITHOUT_QUERY%` is **NOT a v1.33.0 access-log
> operator** (`error initializing config: Not supported field in StreamInfo: REQ_WITHOUT_QUERY` — like the
> non-existent `typed_json_format` of phase 38). The pick was VOID, so phase 41 **pivots to `%ROUTE_NAME%`**
> (recon-confirmed VALID) per **ADR-0098** (the §6.2 reconciliation, which FIRES with this material pivot).
> This SPEC is the re-scoped requirements contract; `PLAN.md` (the state-2 step) turns it into tasks.

## §0 — One-paragraph summary

**Add the `%ROUTE_NAME%` access-log command operator — the `name` of the route that matched the request.**
Phases 32/38/39/40 built the access-log command-operator engine + the text/json encoders over a FIXED
operator set that reads the EXISTING `AccessLogRecord` fields. `%ROUTE_NAME%` is the cheapest of the
not-yet-implemented operators whose data needs a small new record field: it renders the matched route's
config `name` (config-deterministic). This phase adds it: a new `AccessLogRecord.route_name: Option<String>`
field (set by the HCM at route-match), a new `Op::RouteName` variant (a no-arg keyword like `%PROTOCOL%`/
`%RESPONSE_CODE%`), its render arm, and an `encode_single_op` arm — all mirroring the EXISTING `%UPSTREAM_HOST%`
(`Op::UpstreamHost`, an `Option<String>` record field rendered via `quote_opt`).

**`%ROUTE_NAME%` is the cheapest-strong VALID next leaf** (after the state-1 pick `%REQ_WITHOUT_QUERY%`
proved non-existent at v1.33.0). It opens the "operators backed by a new record field" sub-vein (future:
`%UPSTREAM_CLUSTER%`, `%VIRTUAL_HOST_NAME%`, `%RESPONSE_CODE_DETAILS%`). It is config-DETERMINISTIC (the
route name is static config, unlike the timing operators), byte-exact, and reuses the entire command-operator
engine + the text/json encoders + the harness. The ONLY new code is one `Option<String>` record field + the
HCM route-name plumbing (the HCM ALREADY matches the route) + the new `Op` (mirroring `Op::UpstreamHost`).
**NO new connection plumbing, NO new request attribute, NO new crate/dependency, NO new `HttpFilterInstance`
variant**, and projected NO new `ConfigError` variant.

**§6.2 FACTS (recon-LOCKED by ADR-0098, captured live against `envoyproxy/envoy:v1.33.0`):** a NAMED route
(`name: myroute`) → `%ROUTE_NAME%` renders `myroute` (json single-op → quoted `"myroute"`; mixed → `r=myroute`);
an UNNAMED route → ABSENT (json single-op → `null`; mixed → the `-` sentinel `r=-`). I.e. `%ROUTE_NAME%`
behaves EXACTLY like `%UPSTREAM_HOST%` — an `Option<String>` whose `Some`→the name, `None`→absent.

## §1 — Goal & differential surface
**Goal.** Add `%ROUTE_NAME%` to the access-log command-operator engine, behaviorally equivalent to upstream
Envoy v1.33.0 under the differential contract (§7.2) on the **Access log records** dimension — byte-exact
whole-line for the curated deterministic set.

**Differential surface at phase end:**
- **Fixture `0049-accesslog-route-name`** (next free; baseline `0001`…`0048`): an H1 listener whose route
  config has a NAMED route (`name: <fixed>`); the file logger's format contains `%ROUTE_NAME%`. The driver
  issues a request; the emitted line shows the route name, byte-exact cross-proxy. (Optionally a second
  vhost/route to also witness a distinct name; the unnamed→absent case is in the backstop.)
- **All `0001`–`0048` stay green simultaneously** — `%ROUTE_NAME%` is a NEW operator + a NEW record field
  defaulting absent; no existing fixture uses it; the existing render paths + record construction are
  byte-preserved (the new field is `Option<String>` defaulting `None`, appended).

**Conformance:** h2spec ≥95% (unchanged). Fuzz: the operator reuses `accesslog_format_parse`/`parse_bootstrap`;
add a `%ROUTE_NAME%` seed. NO new fuzz target projected.

## §2 — Scope (minimum-viable)
### §2.1 IN scope
1. **The `route_name` record field.** Add `pub route_name: Option<String>` to `AccessLogRecord`
   (`crates/envoy-accesslog/src/record.rs`), mirroring `upstream_host: Option<String>`. Default `None`.
2. **The HCM route-name plumbing.** Where the HCM matches the route + builds the `AccessLogRecord`
   (`crates/envoy-http1/src/hcm.rs` — the record-construction site, and the H2 equivalent), set
   `route_name` to the matched route's config `name` if non-empty, else `None`. **PLAN-VERIFY** the route
   config struct exposes a `name` (add a `pub name: String` to the route struct in
   `crates/envoy-config/src/bootstrap.rs` if absent; serde-default empty → treated as unnamed/`None`).
3. **The `Op::RouteName` operator.** Add `Op::RouteName` to the `Op` enum (`command_operator.rs:36`), a
   `"ROUTE_NAME"` no-arg keyword dispatch (mirroring `%PROTOCOL%`/`%RESPONSE_CODE%`; a `(...)`/`:N` suffix
   is **PLAN-VERIFY** — projected: no-arg, like the other non-header operators), a render arm
   (`record.route_name.as_deref().unwrap_or("-")` in `render_op`, mirroring `Op::UpstreamHost`), and an
   `encode_single_op` arm (`quote_opt(out, record.route_name.as_deref())` — present→quoted, absent→`null`,
   mirroring `Op::UpstreamHost` at `json_format.rs`).
4. **Tests.** Fixture `0049` (byte-exact, named route) + all `0001`–`0048` unchanged + an in-process
   backstop: named→the name (text + json single-op quoted + mixed); unnamed→`-` sentinel (text/mixed) /
   `null` (json single-op); the record-default-`None` round-trip. Plus an `accesslog_format_parse`/
   `parse_bootstrap` seed and a BEHAVIOR_CONTRACT "Access log field mapping" `%ROUTE_NAME%` note.

### §2.2 DEFERRED non-goals
- **`%REQ_WITHOUT_QUERY%`** — NOT a v1.33.0 operator (ADR-0098 §A); VOID, removed from scope.
- **`%UPSTREAM_CLUSTER%` / `%VIRTUAL_HOST_NAME%` / `%RESPONSE_CODE_DETAILS%` / `%ACCESS_LOG_TYPE%` /
  `%REQUEST_HEADERS_BYTES%`** — other recon-VALID operators (each needs its own new record field / data);
  each its own future phase (this phase does ONE: `%ROUTE_NAME%`).
- **`sort_properties`/`content_type`, CF-39-1, the gRPC-ALS/OTLP/tracing/tap surfaces** — unchanged future homes.

## §3 — Open PLAN-write design calls (resolved at state-2)
1. **The route config `name`** — confirm the route struct exposes `name` (or add it); confirm an
   empty/missing name → `None` (the unnamed→absent recon behavior).
2. **The operator suffix grammar** — confirm `%ROUTE_NAME%` is no-arg (no `(...)`, no `:N`) at v1.33.0
   (projected; a `:N` truncate is **PLAN-VERIFY**).
3. **The H2 record-construction site** — the H2 HCM also builds the record; set `route_name` there too.
4. **The fixture-0049 shape** + the fuzz seed — §3 PLAN-write calls.
5. **The §6.1 split** — see §6.1 (projected NOT to fire).

## §4 — Reuse map (what exists; do not rebuild)
- **The command-operator engine** (`command_operator.rs`: the `Op` enum `:36`; the no-arg keyword dispatch
  [`%PROTOCOL%`/`%RESPONSE_CODE%` precedents]; `render_op`/`render_value_segments`; the `-` sentinel) — add
  ONE `Op::RouteName` variant + its keyword + its render arm, mirroring `Op::Protocol`/`Op::UpstreamHost`.
- **The `Op::UpstreamHost` precedent** (an `Option<String>` record field → `render_op` `unwrap_or("-")` +
  `encode_single_op` `quote_opt`) — `%ROUTE_NAME%` is the SAME pattern; copy it.
- **The `AccessLogRecord`** (`record.rs`: `upstream_host: Option<String>` to mirror) — add `route_name`.
- **The HCM record construction** (`crates/envoy-http1/src/hcm.rs` + the H2 equivalent) — set `route_name`
  from the matched route's `name`. The route match ALREADY happens; this reads the matched route's name.
- **The text/json encoders + harness** (`Driver::Http1WithAccessLog`/`AccessLogByteExactProbe`) — the
  `0040`/`0046`/`0047`/`0048` template for fixture `0049`. UNCHANGED.
- **The fuzz corpora + BEHAVIOR_CONTRACT** — extend; no new fuzz target.

## §5 — Behavioral contract notes
- **The new axis (one operator + one record field, config-deterministic):** `%ROUTE_NAME%` reads the matched
  route's static config name — deterministic (unlike the timing operators), byte-exact.
- **Mirrors `%UPSTREAM_HOST%`:** an `Option<String>` → present quoted/rendered, absent → `null` (json
  single-op) / `-` sentinel (text/mixed). No new rendering machinery.
- **Default-absent byte-preservation (the load-bearing proof):** the new `route_name` field defaults `None`
  and the operator is new → all `0001`-`0048` stay byte-identical.
- **Determinism / locality:** the line is a function ONLY of the (fixed) request + the static route config;
  observable on a normal request/response → fixture `0049` is authoritative on this host (NOT Linux-CI-only).
- **Config validity:** an unknown operator stays boot-fatal via the EXISTING `parse_format` (no new
  variant). All-fatal posture unchanged (ADR-0049).

## §6 — Process
### §6.1 — Split projection
NOT to fire. ONE record field + ONE `Op` variant (mirroring `Op::UpstreamHost`) + the HCM plumbing + one
fixture + backstop + seed + a BEHAVIOR_CONTRACT note. **~120–250 LoC / ~5–6 tasks** — under the gate.
**ADR-0099 reserved** for the split (projected NOT to fire).

### §6.2 — Empirical reconnaissance (DONE at this state-2; ADR-0098 FIRED)
The recon ran against live `envoyproxy/envoy:v1.33.0`: it (a) found `%REQ_WITHOUT_QUERY%` VOID (Not
supported field in StreamInfo) → the pivot; (b) confirmed `%ROUTE_NAME%` VALID — named route → the name
(json single-op quoted; mixed string), unnamed → absent (`null` / `-` sentinel). ADR-0098 locks these.
The remaining items (the route-config `name` exposure, the suffix grammar) are PLAN-VERIFY at PLAN-write.

### §6.3 — Anti-deferral
No vague TODOs. Every §2.1 item is implemented + tested; every deferral is a §2.2 named non-goal.

## §7 — Acceptance (the §7.5 gate, previewed)
(a) fixture `0049` green (byte-identical route-name line) + (b) all `0001`-`0048` green + (c) h2spec ≥95%
+ (d) `accesslog_format_parse`/`parse_bootstrap` fuzz clean (with the `%ROUTE_NAME%` seed) — NO new target
+ (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new
crate/dependency; projected NO new `ConfigError` variant; ONE new `AccessLogRecord` field (`route_name`).

---

_Re-scoped from `%REQ_WITHOUT_QUERY%` (VOID at v1.33.0) to `%ROUTE_NAME%` at the state-2 §6.2 recon. Scope
locked by **ADR-0097** (pick) as amended by **ADR-0098** (the §6.2 reconciliation — the pivot + the
ROUTE_NAME facts). The §6.1 split is projected NOT to fire (**ADR-0099 reserved**). `PLAN.md` is authored
THIS state-2 session against the ADR-0098-locked facts._
