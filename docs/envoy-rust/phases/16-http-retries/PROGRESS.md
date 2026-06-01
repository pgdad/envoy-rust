# Phase 16 (`16-http-retries`) — PROGRESS

> Running log, updated by the executor on each task completion (the 06.2 → 15 cadence).
> One entry per PLAN task; quote the verifying command output. The state-3 arc runs
> `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK
> (NOT deferred to state-4) per `project_state3_arc_skips_clippy`.

**PLAN:** `docs/envoy-rust/phases/16-http-retries/PLAN.md`
**SPEC:** `docs/envoy-rust/phases/16-http-retries/SPEC.md`
**Scope ADRs:** ADR-0044 (minimum-viable retry scope + body-replay finding); ADR-0045 (§6.2 reconciliation — accept-and-ignore unknown tokens / per-attempt `upstream_rq_total` / `x-envoy-attempt-count` gated on `include_attempt_count_in_response`).

---

## State-2 PLAN-write (this commit)

- Performed the HEAVY SPEC §6.2 empirical verification against `envoyproxy/envoy:v1.33.0` (Docker; foreground general-purpose subagent). Findings L1–L11 locked into PLAN.md "§6.2 empirical lock-ins". Three material divergences (L2 unknown-token accept-and-ignore; L5 per-attempt `upstream_rq_total` + completing-only `upstream_rq_5xx` + Envoy-only `retry.*` sub-scope; L6 `x-envoy-attempt-count` gated on `include_attempt_count_in_response`) → **ADR-0045 landed**.
- Performed the PLAN-time SPEC-correction pass (read-only Explore subagent) against HEAD `0fa80aba9`. All SPEC §3 anchors confirmed except: `ConfigError` is in `lib.rs` (not `bootstrap.rs`); the deep-clone sites `clone_route_action`/`clone_route_config` must clone the new fields; `Driver` is in `tests/differential/src/lib.rs`; fuzz corpus is at 27 seeds (not 22). Corrections recorded in PLAN.md "PLAN-time SPEC corrections".
- Evaluated the §6.1 split gate against the §6.2-refined surface (~1450–1650 LoC / ~13 tasks) → **single un-split phase; ADR-0046 does NOT fire.**
- Flipped ROADMAP row `16` `planned → in-progress`. Advanced STATE.md to `16` state-2-complete / state-3-next.

## Task 1 — `envoy-config` schema (`RetryPolicy` + `retry_policy` field + `include_attempt_count_in_response`)

**Preamble (read before starting):**
- **Goal:** Add the `RetryPolicy` struct + `RouteAction_Route.retry_policy: Option<RetryPolicy>` (`crates/envoy-config/src/bootstrap.rs:953-955`) + `VirtualHost.include_attempt_count_in_response: bool` (`:916`, `#[serde(default)]` → false). TDD: serde round-trip + deny_unknown_fields rejection + vhost-flag tests first.
- **§6.2 lock-ins that bind this task:** L3 (`num_retries` = `Option<u32>`, default 1 resolved later; `retriable_status_codes` = `Vec<u32>`); L6 (the new `include_attempt_count_in_response` VirtualHost field is REQUIRED — `x-envoy-attempt-count` is gated on it, not automatic).
- **Anchors (verified at HEAD `0fa80aba9`):** `RouteAction_Route` `bootstrap.rs:953-955` (currently `cluster: String` only, `#[serde(deny_unknown_fields)]`); `VirtualHost` `:916`; `RouteAction` enum `:939`; `Route` `:923`. The deferred `retry_policy` fields (`per_try_timeout`, `retry_back_off`, `retry_priority`, `retry_host_predicate`, `host_selection_retry_max_attempts`, `retriable_headers`, `retriable_request_headers`, `rate_limited_retry_back_off`) are rejected automatically by `#[serde(deny_unknown_fields)]` on `RetryPolicy` — no explicit `ConfigError` variant needed.
- **Carry-forward warning for LATER tasks (not this one):** `clone_route_action` (`hcm.rs:240`, clones only `cluster` at `:249-250`) and `clone_route_config` (`hcm.rs:220`) MUST be updated (Task 4) to clone the new fields, or they are silently dropped on the `Arc<RouteConfiguration>` clone.
- **Verification:** `cargo test -p envoy-config retry_policy` (PASS) + `cargo clippy -p envoy-config --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check`.

_(Tasks 2–11 entries appended by the executor as each completes.)_

---

## Task 1 — `RetryPolicy` schema + `retry_policy` field + `include_attempt_count_in_response` (commit `3b0e23ecc`)

**Landed.** `crates/envoy-config/src/bootstrap.rs`: new `RetryPolicy` struct (`retry_on: String` +
`num_retries: Option<u32>` [L3 — default-1 resolution deferred to Task 2's `RetryConfig::from`] +
`retriable_status_codes: Vec<u32>`; `#[serde(deny_unknown_fields)]` rejects all deferred fields);
`RouteAction_Route` gains `retry_policy: Option<RetryPolicy>` (`Eq` derive dropped — `RetryPolicy`
is `PartialEq`-only, matching the `CircuitBreakers`/`Thresholds` house convention; verified no
consumer needs `RouteAction_Route: Eq`); `VirtualHost` gains
`include_attempt_count_in_response: bool` (`#[serde(default)]` → false; L6). 4 TDD serde tests
(parse-minimal / absent-yields-none / rejects-`per_try_timeout` / vhost-flag-true-and-absent).
`crates/envoy-config/src/lib.rs`: `RetryPolicy` re-export. **Workspace-compile fold-in (spec-review
finding):** the new required fields broke exhaustive struct literals downstream — fixed in the SAME
commit by FAITHFUL clones at `crates/envoy-http1/src/hcm.rs` `clone_route_config` (`:222`,
`include_attempt_count_in_response`) + `clone_route_action` (`:251`, `retry_policy`) (the PLAN-time
SPEC-correction CRITICAL deep-clone sites, discharged at Task 1 instead of Task 4) + inert defaults
(`false`/`None`) at the 16+8 H1 and 9+1 H2 `#[cfg(test)]` struct literals.

**Verification (quoted):**
- `cargo test -p envoy-config` → `test result: ok. 291 passed; 0 failed; 0 ignored` (287 + 4 new).
- `cargo test -p envoy-http1` → `test result: ok. 87 passed; 0 failed; 0 ignored`.
- `cargo test -p envoy-http2` → `test result: ok. 57 passed; 0 failed; 1 ignored`.
- `cargo build --workspace --all-targets` → `Finished` (clean).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean (exit 0).
- `cargo fmt --all -- --check` → clean (exit 0).
- `git show --stat HEAD` → 4 files changed (`bootstrap.rs` +98, `lib.rs` +10/-7, H1 `hcm.rs` +26, H2 `hcm.rs` +10).

**Two-stage review:** spec-compliance review surfaced the workspace break (Critical) → fixed +
re-verified; code-quality review **Approved** (zero Critical / zero Important; 2 Minor notes:
`Eq`-drop confirmed correct, deserialize-only tests match house style).

---

## Task 2 — `RetryConfig` + `retry_on` tokenization (accept-and-ignore) + `validate_retry_policy` (commit `2511d7be7`)

**Landed.** `crates/envoy-config/src/bootstrap.rs`: `AttemptOutcome` enum (Response/ConnectFailure/Reset),
`RetryOn` 5-flag struct, `RetryConfig` resolved type with `impl From<&RetryPolicy>` (comma tokenize +
trim + skip-empty; UNKNOWN tokens silently ignored per L2; `num_retries.unwrap_or(1)` per L3) and
`is_retriable(status, outcome)` classifier (`5xx` = 500..=599; `gateway-error` = 502..=504 ONLY per L1;
`retriable-status-codes` list membership; ConnectFailure/Reset purely outcome-driven). Infallible
`validate_retry_policy(route)` hook wired per-route into the `validate_hcm` route walk (sibling of
`validate_header_matcher`). `lib.rs` re-exports `AttemptOutcome`/`RetryConfig`/`RetryOn`. 3 TDD tests.
**PLAN deviations (both improvements):** `From` trait impl instead of inherent `fn from` (avoids clippy
`should_implement_trait`); `matches!(status, 502..=504)` instead of `502 | 503 | 504` (avoids clippy
`manual_range_patterns`).

**Verification (quoted):**
- `cargo test -p envoy-config` → `test result: ok. 294 passed; 0 failed; 0 ignored` (291 + 3 new).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean (exit 0).
- `cargo fmt --all -- --check` → clean (exit 0).
- `git show --stat HEAD` → 2 files changed (`bootstrap.rs` +144, `lib.rs` +8/-8).

**Two-stage review:** spec-compliance **✅ compliant** (first pass); code-quality **Approved** (zero
Critical / zero Important; Minors: the `is_retriable(status, outcome)` dummy-status-on-ConnectFailure
API shape kept as PLAN-specified; boundary-case test gaps noted as obviously-correct-by-inspection).

---

## Task 3 — cluster `upstream_rq_retry{,_success,_limit_exceeded}` counters, inert-at-0 (commit `dbabd526a`)

**Landed.** `crates/envoy-cluster/src/cluster.rs`: 3 `pub(crate) Arc<envoy_stats::Counter>` fields next
to `upstream_rq_total`/`upstream_rq_5xx`; **unconditional registration** in `from_bootstrap` (every
cluster, inert at 0 — PLAN Task 3 lock-in; names byte-exact `cluster.<name>.upstream_rq_retry` /
`_retry_success` / `_retry_limit_exceeded`); accessors on `Cluster` + `ClusterHandle` delegates
mirroring the existing counter accessor shape; registry-level TDD test
`from_bootstrap_registers_upstream_rq_retry_counters_at_zero` (snapshot presence + 0-values + accessors).
No increments (Tasks 4/5).

**Verification (quoted):**
- `cargo test -p envoy-cluster` → `test result: ok. 71 passed; 0 failed; 0 ignored` (70 + 1 new).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean (exit 0).
- `cargo fmt --all -- --check` → clean (exit 0).
- `git show --stat HEAD` → 1 file changed (`cluster.rs` +188).

**Two-stage review:** spec-compliance **✅ compliant** (first pass); code-quality **Approved** (zero
Critical / zero Important). The reviewer concretely ruled out the unconditional-registration risk
against the fixture-0011 Prometheus set-diff: fixture 0011 declares `clusters: []`, so the per-cluster
loop registers nothing there; all other stat fixtures (0020/0021/0022/0023) use named-stat assertions
immune to extra registered names.

---

## Task 4 — H1 retry loop + per-attempt `upstream_rq_total` + `x-envoy-attempt-count` + back-off (commit `546b06973`)

**Landed.** `crates/envoy-http1/src/hcm.rs`: the `BuildOutcome::Proxy` arm of `serve_connection` is now
a retry loop over an extracted per-attempt helper `run_attempt(...) -> AttemptResult { response,
endpoint: Option<SocketAddr>, outcome: Option<AttemptOutcome>, upstream_response: bool }` (pick →
pool/per-call acquire → send → receive → classify). Counter reconciliation (L5): `upstream_rq_total`
ticks PER upstream-response attempt inside the loop (gated on `upstream_response` — connect-failures/
synths do NOT tick, preserving the BEHAVIOR_CONTRACT bypass); `upstream_rq_5xx` ticks ONCE post-loop on
the COMPLETING response; both moved OUT of `router::construct_proxied_response` (now side-effect-free).
Retry counters (L4): `upstream_rq_retry` per retry fired; post-loop XOR `retry_success`/
`retry_limit_exceeded` on `attempts > 1`. `record_response` per attempt for every picked endpoint
(connect-failure + overflow record; pick()→None does not — L8 / 14.x bypass preserved). Connect-failure
arm participates in retry under `retry_on: connect-failure`; post-connect send/recv failure classified
`AttemptOutcome::Reset` (Envoy-faithful; inert unless `retry_on: reset`). `x-envoy-attempt-count`
emitted gated on the matched vhost `include_attempt_count_in_response` (L6); `X_ENVOY_ATTEMPT_COUNT`
const in router.rs. Back-off = `RetryConfig::backoff(attempt)` in `envoy-config` (exp base 25 ms cap
250 ms, no jitter — L7; shared with Task 5). `BuildOutcome::Proxy` carries `retry_config:
Option<RetryConfig>` + `include_attempt_count_in_response: bool`; H2's destructure ignores them via
`..` (Task 5 wires the H2 loop). 3 TDD tests (success-retry 503→200; limit-exceeded always-503;
no-retry regression) against a stateful in-test `AtomicUsize`-counted backend.

**Verification (quoted):**
- `cargo test -p envoy-http1` → `test result: ok. 90 passed; 0 failed; 0 ignored` (87 + 3 new).
- `cargo test -p envoy-config` → `test result: ok. 295 passed` (+1 backoff test); `-p envoy-cluster` → `71 passed`; `-p envoy-http2` → `57 passed; 1 ignored`.
- `cargo build --workspace --all-targets` + `cargo build -p envoy-http1` (standalone) → clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean (exit 0).
- `cargo fmt --all -- --check` → clean (exit 0).
- `git show --stat HEAD` → 4 files changed (+698/−291).

**Two-stage review:** spec-compliance **✅ compliant** (verified per-attempt counting semantics, loop
boundary `attempts <= max_retries` → exactly 2 attempts at `num_retries: 1`, no-retry path
byte-identical, router.rs test repurposing sound). Code-quality first pass **With fixes** → 2 Important
(inverted `_cx_active` ConnGaugeGuard drop ordering vs the documented stream-drops-first invariant;
proxy-arm size ~320 lines / 7-8 nesting levels ahead of the Task-5 H2 mirror) + 2 Minor (vestigial
`cluster` param; garbled comment) → ALL FIXED in the amended commit (guard declared before the handle
binding → drops last; `run_attempt` extraction → proxy arm ~128 lines; param dropped; comment
rewritten) → re-review **Approved** (no new issues).

**CI:** run `26749898283` (HEAD `107899920`) `completed / success` — all 23 Docker-gated fixtures +
fuzz + 5 stable-toolchain gates green with the per-attempt counting reconciliation in place.

---

## Task 5 — H2 retry loop + per-attempt counting + `x-envoy-attempt-count` (commit `5c65fc173`)

**Landed.** `crates/envoy-http2/src/hcm.rs`: `handle_one_stream`'s `BuildOutcome::Proxy` arm is now a
retry loop over an extracted `run_h2_attempt(...) -> H2AttemptResult` helper (a faithful local mirror
of H1's `run_attempt`/`AttemptResult` — no envoy-http1 dependency). The H1-vs-H2 upstream-protocol fork
lives INSIDE the helper (internal `AcquireOutcome` enum: `Sent(Result)` / `ConnectFailure` /
`Overflow`), so the loop is protocol-agnostic — a retry on an H2-protocol cluster re-acquires from the
H2 pool. Per-attempt accounting identical to H1 (L5 `upstream_rq_total` gated on `upstream_response`;
L8 `record_response` gated on `endpoint.is_some()`; post-loop completing-only `upstream_rq_5xx` + L4
XOR split + L6 gated `x-envoy-attempt-count`); back-off REUSES `envoy_config::RetryConfig::backoff`.
Body replay = per-attempt `envoy_req.body.clone()` of the buffered `Bytes` (ADR-0044 §0). Existing
synth shapes (pick-none 502 / overflow 503 / connect-fail 502) preserved byte-exact. 3 TDD tests
(success / limit-exceeded / no-retry regression; all on the H1-upstream fork — the H2-upstream fork is
covered structurally + by pre-existing `max_retries==0` tests) + 1 fix-driven test (below).

**Verification (quoted):**
- `cargo test -p envoy-http2` → `test result: ok. 61 passed; 0 failed; 1 ignored` (57 + 4 new).
- `cargo test -p envoy-http1` → `test result: ok. 91 passed` (90 + 1 new sibling test); `-p envoy-config` → `295 passed`; `-p envoy-cluster` → `71 passed`.
- `cargo build --workspace --all-targets` → clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean (exit 0); `cargo fmt --all -- --check` → clean.
- `git show --stat HEAD` → 2 files changed (H2 `hcm.rs` +756/−268; H1 `hcm.rs` +53 [sibling test only]).

**Two-stage review:** spec-compliance **✅ compliant** (mirror fidelity verified line-by-line; H2-upstream
fork retry path verified structurally; body-replay per-attempt clone verified; counting
single-source-of-truth; pre-existing H2 synth/bypass behavior preserved vs base). Code-quality first
pass **With fixes** → 1 Important: **H2 collapsed connect-failure into `Reset`** (H1 distinguishes
`ConnectFailure` vs `Reset`) — an observable cross-protocol asymmetry under `retry_on:
connect-failure`-without-`reset` → FIXED in the amended commit (H2 `AcquireOutcome` restructured to
mirror H1: all 3 connect sites → `ConnectFailure`; only `send_request` errors → `Reset`), proven by a
NEW TDD sibling test pair `{h2_,}connect_failure_retried_on_connect_failure_policy` (kernel-refused
`127.0.0.1:1` endpoint; H2 test RED before the fix [retry never fired], GREEN after; H1 test green
before AND after, proving H1 was already correct) + 2 Minor (comment alignment — fixed; H2-upstream-fork
retry not directly tested — noted for Task 8).
