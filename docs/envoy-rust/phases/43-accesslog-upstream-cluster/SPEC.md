# Phase 43 — `43-accesslog-upstream-cluster` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored via `superpowers:brainstorming`; the project is
> autonomous (`feedback_pick_recommendation` — converge + record the pick in an ADR, no human gate). This
> SPEC is the requirements contract; `PLAN.md` (the state-2 step, the NEXT session) turns it into TDD tasks.
> **The pick was existence-verified at state-1** (the phases-41/42 lesson) against live `envoyproxy/envoy:v1.33.0`.

## §0 — One-paragraph summary

**Add the `%UPSTREAM_CLUSTER%` access-log command operator — the config `name` of the cluster the request
was routed to — AND, as its enabling infrastructure, the FIRST proxy (upstream-routed) access-log differential
fixture.** Phases 32/38/39/40/41/42 built the access-log command-operator engine + the text/json encoders +
the "operator backed by a new `Option<String>` record field" sub-vein (`%ROUTE_NAME%`, `%RESPONSE_CODE_DETAILS%`)
— but EVERY access-log fixture (`0040`-`0050`) drives a `direct_response` route (`clusters: []`, no backend),
so the upstream-routed access-log path has NEVER been differentially exercised. `%UPSTREAM_CLUSTER%` is
config-deterministic (the matched cluster's static `name`) and **its value is `null` on a `direct_response`
path**, so it REQUIRES a routed-upstream fixture — which makes this phase the natural home for the first
**proxy access-log fixture**. That fixture pays off across multiple operators at near-zero marginal cost: the
same upstream-routed log line ALSO witnesses, for the first time on a real upstream, the already-implemented
`%UPSTREAM_HOST%` (phase 06) and `%RESPONSE_CODE_DETAILS%`→`via_upstream` (phase 42) — the latter advancing
carry-forward **M42-1**.

**`%UPSTREAM_CLUSTER%` is the cheapest-STRONG VALID next leaf:** the operator itself is the proven
new-record-field-operator pattern (a new `AccessLogRecord.upstream_cluster: Option<String>` set by the HCM at
the proxy-success arm + a new `Op::UpstreamCluster` rendered EXACTLY like `%UPSTREAM_HOST%`); the cluster name
is ALREADY in hand at the HCM proxy arm (`BuildOutcome::Proxy { cluster, … }`). The genuinely NEW work is the
**enabling harness infra** (extend the access-log differential driver to route to a real backend — the harness
already has backend/upstream machinery used by other drivers). It is config-DETERMINISTIC (a static string —
NO formula to match, unlike `%REQUEST_HEADERS_BYTES%`), byte-exact, and opens a needed differential surface.

**§6.2 FACTS (recon-LOCKED this state-1, captured live against `envoyproxy/envoy:v1.33.0`):** a request routed
to `route: { cluster: my_backend_cluster }` → `%UPSTREAM_CLUSTER%` renders `my_backend_cluster` (json
single-op → quoted `"my_backend_cluster"`; mixed → `c=my_backend_cluster`); the same line shows
`%RESPONSE_CODE_DETAILS%`→`via_upstream`. On a `direct_response`/local-reply path `%UPSTREAM_CLUSTER%` is
ABSENT (→ `null` / `-` sentinel). I.e. an `Option<String>` whose `Some`→the cluster name, `None`→absent —
IDENTICAL to `%UPSTREAM_HOST%`.

## §1 — Goal & differential surface
**Goal.** Add `%UPSTREAM_CLUSTER%` to the access-log command-operator engine + stand up the first proxy
access-log fixture, behaviorally equivalent to upstream Envoy v1.33.0 under the differential contract (§7.2)
on the **Access log records** dimension — byte-exact whole-line for the curated deterministic set.

**Differential surface at phase end:**
- **Fixture `0051-accesslog-upstream-cluster`** (next free; baseline `0001`…`0050`): an H1 listener whose
  route forwards to a **real backend cluster** (the first access-log fixture with a non-empty `clusters:`);
  the file logger's format contains `%UPSTREAM_CLUSTER%` (single-op + mixed) plus — as near-free regression
  witnesses on the proxy path — `%UPSTREAM_HOST%` and `%RESPONSE_CODE_DETAILS%`. The driver issues a request;
  the emitted line shows the cluster name (byte-exact cross-proxy), with `via_upstream` and the upstream host.
  **The upstream-host value may be non-deterministic** (the ephemeral backend ip:port) — PLAN-VERIFY whether
  `%UPSTREAM_HOST%` must be normalized/excluded from the byte-exact assertion or pinned to a fixed backend
  address (the harness controls the backend), else witness it via a `json_shape`/normalized rule rather than
  byte-exact. `%UPSTREAM_CLUSTER%` + `%RESPONSE_CODE_DETAILS%` ARE byte-exact (config/flow-deterministic).
- **All `0001`–`0050` stay green simultaneously** — `%UPSTREAM_CLUSTER%` is a NEW operator + a NEW record
  field defaulting `None`; no existing fixture uses it; the existing render paths + record construction are
  byte-preserved (the new field is `Option<String>` defaulting `None`, appended last).

**Conformance:** h2spec ≥95% (unchanged — NO HTTP/2 codec change). Fuzz: the operator reuses
`accesslog_format_parse`/`parse_bootstrap`; add a `%UPSTREAM_CLUSTER%` seed. NO new fuzz target.

## §2 — Scope (minimum-viable)
### §2.1 IN scope
1. **The `upstream_cluster` record field.** Add `pub upstream_cluster: Option<String>` to `AccessLogRecord`
   (`crates/envoy-accesslog/src/record.rs`), mirroring `upstream_host: Option<String>`. Default `None`.
2. **The HCM upstream-cluster plumbing.** At the H1 + H2 proxy-success arm (where `upstream_host_for_log[_h2]
   = Some(endpoint)` and `response_code_details = Some("via_upstream")` are already set), set
   `upstream_cluster` to the matched cluster's `name` — ALREADY in hand as `BuildOutcome::Proxy { cluster, … }`
   (the cluster name string). Non-proxy paths (direct_response, error/filter synths) leave it `None`. (H2
   threads it as a new `finalize_h2_stream` parameter, mirroring `response_code_details_for_log_h2`.)
3. **The `Op::UpstreamCluster` operator.** Add `Op::UpstreamCluster` to the `Op` enum, a `"UPSTREAM_CLUSTER"`
   no-arg keyword dispatch (mirroring `%UPSTREAM_HOST%`; a `(...)`/`:N` suffix is **PLAN-VERIFY** — projected
   no-arg), a `render_op` arm (`record.upstream_cluster.as_deref().unwrap_or(empty_or_dash)`), and an
   `encode_single_op` arm (`quote_opt(out, record.upstream_cluster.as_deref())`) — all mirroring
   `Op::UpstreamHost`.
4. **The first proxy access-log fixture infra.** Extend the differential access-log driver
   (`Driver::Http1WithAccessLog` / `tests/differential/src/access_log.rs`) to OPTIONALLY route to a real
   backend cluster (reusing the harness's existing backend/upstream machinery — `tests/differential/src/
   backend.rs`/`upstream.rs`). **PLAN-VERIFY** the cleanest seam: a new driver variant vs. an opt-in backend
   field on `Http1WithAccessLog`. The backend must be deterministic enough for a byte-exact log line on the
   `%UPSTREAM_CLUSTER%`/`%RESPONSE_CODE_DETAILS%` tokens.
5. **Tests.** Fixture `0051` (byte-exact on the cluster-name + via_upstream tokens) + all `0001`–`0050`
   unchanged + an in-process backstop: present→the cluster name (text + json single-op quoted + mixed);
   absent→`-` sentinel / `null`; the record-default-`None` round-trip. Plus an `accesslog_format_parse`/
   `parse_bootstrap` `%UPSTREAM_CLUSTER%` seed + a BEHAVIOR_CONTRACT "Access log field mapping" note.

### §2.2 DEFERRED non-goals
- **`%UPSTREAM_HOST%` byte-exact assertion** — `%UPSTREAM_HOST%` is witnessed on the proxy fixture as a
  regression bonus, but its value (the ephemeral backend ip:port) may need normalization; if pinning/normalizing
  is non-trivial, witness it loosely (json_shape) or exclude it from the byte-exact line — full byte-exact
  upstream-host is a §2.2 deferral, NOT a phase-43 obligation.
- **M42-1's full `via_upstream` failure-detail vocabulary** — this phase witnesses `via_upstream` on the
  upstream-SUCCESS path (advancing M42-1) but does NOT add the connect-error/reset/503 failure details (those
  need failure-injection fixtures) — M42-1 stays open for that.
- **`%REQUEST_HEADERS_BYTES%` / `%ACCESS_LOG_TYPE%` / the gRPC-ALS/OTLP/tracing/tap surfaces** — other
  recon-VALID operators / sinks; each its own future phase.

## §3 — Open PLAN-write design calls (resolved at state-2)
1. **The proxy access-log driver seam** — new `Driver` variant vs. opt-in backend on `Http1WithAccessLog`;
   how the harness starts + addresses the backend deterministically (re-use `backend.rs`/`upstream.rs`).
2. **The `%UPSTREAM_HOST%` determinism** — pin the backend to a fixed address, or normalize/exclude the
   upstream-host token from the byte-exact assertion (§2.2).
3. **The operator suffix grammar** — confirm `%UPSTREAM_CLUSTER%` is no-arg (no `(...)`, no `:N`) at v1.33.0
   (PLAN-VERIFY by booting `%UPSTREAM_CLUSTER:3%`).
4. **The H2 proxy access-log path** — confirm the H2 driver can also route to a backend (or scope `0051` H1-only
   + an in-process H2 backstop, mirroring how earlier phases handled H2).
5. **The §6.1 split** — see §6.1 (projected to LIKELY fire — the new harness infra).

## §4 — Reuse map (what exists; do not rebuild)
- **The command-operator engine + the `Op::UpstreamHost` precedent** (`command_operator.rs` / `json_format.rs`:
  an `Option<String>` field → `render_op` `unwrap_or` + `encode_single_op` `quote_opt`) — `%UPSTREAM_CLUSTER%`
  is the SAME pattern; copy `Op::UpstreamHost`.
- **The `AccessLogRecord`** (`record.rs`: `upstream_host: Option<String>` to mirror) — add `upstream_cluster`.
- **The HCM proxy-success arm** (`crates/envoy-http1/src/hcm.rs` H1 assignment ~`:983-984` + the SEPARATE-CRATE
  H2 equivalent in `crates/envoy-http2/src/hcm.rs` ~`:682-683`, with `finalize_h2_stream` ~`:837` and the
  `*_for_log_h2` params ~`:846`/`:853` — where `upstream_host_for_log[_h2]` + `response_code_details =
  via_upstream` are set; `BuildOutcome::Proxy { cluster }` [`envoy-http1/src/hcm.rs:1383`] carries the cluster
  name) — set `upstream_cluster` alongside (two assignments: H1 + the H2 param-thread).
- **The harness backend/upstream machinery** (`tests/differential/src/backend.rs`/`upstream.rs`) + the
  access-log driver (`access_log.rs` / `Driver::Http1WithAccessLog`) — COMBINE them for the proxy fixture.
- **The fuzz corpora + BEHAVIOR_CONTRACT** — extend; no new fuzz target.

## §5 — Behavioral contract notes
- **The new axis (one operator + one record field, config-deterministic; + the first proxy access-log
  fixture):** `%UPSTREAM_CLUSTER%` reads the matched cluster's static `name` — deterministic, byte-exact.
- **Mirrors `%UPSTREAM_HOST%`:** an `Option<String>` → present quoted/rendered, absent → `null`/`-`.
- **Default-absent byte-preservation (load-bearing):** the new field defaults `None` + the operator is new →
  all `0001`-`0050` stay byte-identical.
- **Config validity:** an unknown operator stays boot-fatal via the EXISTING `parse_format`. All-fatal posture
  unchanged (ADR-0049).

## §6 — Process
### §6.1 — Split projection
**LIKELY to fire.** Unlike phases 41/42 (which cloned an existing direct_response fixture), this phase stands
up NEW harness infra (the first proxy access-log fixture — backend wiring into the access-log driver) ON TOP
of the one-operator leaf. The operator itself is ~6 tasks (~120-250 LoC, the proven pattern); the harness
extension may add materially more. **ADR-0100 is reserved**; the state-2 PLAN-write decides whether to SPLIT
(e.g. `43.1` the proxy-access-log harness infra + `43.2` the `%UPSTREAM_CLUSTER%` operator) or keep it whole.

### §6.2 — Empirical reconnaissance (the EXISTENCE-CHECK ran at THIS state-1; the deep recon is state-2)
The state-1 brainstorm ran a LIVE check against `envoyproxy/envoy:v1.33.0`: a route `{ cluster:
my_backend_cluster }` → `%UPSTREAM_CLUSTER%` = `my_backend_cluster` (json single-op quoted; mixed string),
`%RESPONSE_CODE_DETAILS%` = `via_upstream`. CONFIRMED VALID + config-deterministic. The state-2 §6.2 recon
pins the suffix grammar, the `%UPSTREAM_HOST%` determinism, and the H2 path. **ADR-0100 FIRES at THIS state-1**
(the pick + the recon facts + the §6.1-split projection).

### §6.3 — Anti-deferral
No vague TODOs. Every §2.1 item is implemented + tested; every deferral is a §2.2 named non-goal.

## §7 — Acceptance (the §7.5 gate, previewed)
(a) fixture `0051` green (byte-identical cluster-name + via_upstream line) + (b) all `0001`-`0050` green +
(c) h2spec ≥95% + (d) `accesslog_format_parse`/`parse_bootstrap` fuzz clean (with the `%UPSTREAM_CLUSTER%`
seed) — NO new target + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved.
`#![forbid(unsafe_code)]` holds; NO new crate/dependency; projected NO new `ConfigError` variant; ONE new
`AccessLogRecord` field (`upstream_cluster`); ONE new `Op` variant; the proxy access-log harness extension is
test-only (no `src/` behavior change beyond the one HCM assignment).

---

_Pick locked by **ADR-0100** (phase-43 state-1 brainstorm): `%UPSTREAM_CLUSTER%` + the first proxy access-log
fixture. The §6.1 split is projected to LIKELY fire (the new harness infra) — the state-2 PLAN-write decides.
`PLAN.md` is authored the NEXT session (state-2) against the ADR-0100-locked facts + the state-2 §6.2 recon.
§5.1: one state per session — this session STOPS at the SPEC + ROADMAP row + ADR + STATE advance._
