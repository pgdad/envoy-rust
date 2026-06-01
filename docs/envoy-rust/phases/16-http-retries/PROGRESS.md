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

---

## Task 6 — stateful fail-then-succeed synthetic-backend harness knob (commit `1af2f69e0`)

**Landed.** `tests/helpers/health-aware-http1-backend/src/main.rs`: new `--retry-script PATH=fail:N`
CLI knob — for a scripted path, return **503 (body `fail\n`) for the first N requests then 200 (body
`ok\n`)**, with the counter keyed **per (source IP, path)** (`Mutex<HashMap<IpAddr, u64>>` per scripted
path; lock never held across an await). Per-source keying is load-bearing: **the differential harness
shares ONE backend between both proxies** (envoy-rust dials from `127.0.0.1`; Envoy-in-Docker from the
bridge/NAT gateway address), so a global counter would let the first proxy consume the fail budget and
the second proxy would never retry — per-source gives each proxy its own independent fail-then-succeed
sequence. Stateless `--per-path PATH=STATUS` unchanged (503 body remains `service unavailable\n` —
NOTE for Task 7: the always-503 limit-exceeded path body is `service unavailable\n` (20 bytes), NOT
`fail\n`). Retry-script arm takes precedence over `--per-path`/`/healthz`/default.
`tests/differential/src/backend.rs`: `spawn_with_retry_script(retry_script, per_path)`;
`spawn_with_per_path` delegates (existing callers unchanged). Real-process TDD test
`backend_retry_script_stateful_fail_then_succeed` + 4 parser unit tests.

**Verification (quoted):**
- `cargo test -p health-aware-http1-backend` → `test result: ok. 7 passed; 0 failed`.
- differential `backend_retry_script_stateful_fail_then_succeed` → `ok. 1 passed`.
- `cargo build --workspace --all-targets` → clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean; `cargo fmt --all -- --check` → clean.
- `git show --stat HEAD` → 2 files changed (helper `main.rs` + harness `backend.rs`).

**Two-stage review:** spec-compliance **✅ compliant** → review surfaced the **shared-backend topology
finding** (one backend, both proxies) → per-source-IP counter keying added in the amended commit;
code-quality **Approved** (zero Critical / zero Important; Minors: positional `Option<String>` params
acceptable for a test helper; **RESIDUAL RISK carried to Task 7:** Docker-side source-IP stability —
the design assumes Envoy's retry attempts present the SAME source IP to the host backend (true for
Docker Desktop NAT + Linux bridge, but unproven in this repo) — Task 7's first Docker run must confirm;
if it fails, the symptom is the retry-success path returning 503 bilaterally instead of 200).

---

## Task 7 — fixture `0024-upstream-retry-on-5xx` + Docker wrapper + cyclic retry-script (commit `d1f87a247`)

**Landed — and the Task-6 residual risk MATERIALIZED + was reconciled.** The first live Docker run
FAILED exactly as the Task-6 review warned: **macOS Docker Desktop NATs every container→host connection
to source IP `127.0.0.1`** (proven by a direct host-listener probe: `PEER ('127.0.0.1', …)` from inside
the Docker network) — identical to envoy-rust's source IP — so the per-source-IP counter keying
collapsed both proxies into one bucket; Envoy-in-Docker (driven first) burned the `fail:1` budget and
envoy-rust never retried (`x-envoy-attempt-count: 1`). **Reconciliation (controller decision, no fork):**
the `--retry-script PATH=fail:N` semantics changed to **cyclic windows** — a single global per-path
`AtomicU64`; request idx where `idx % (N+1) < N` → 503 (`fail\n`), else 200 (`ok\n`). For `fail:1`:
503,200,503,200,… Each proxy's retry pair (2 consecutive attempts) lands in its own window because the
keep-alive driver drives the proxies SEQUENTIALLY (verified structurally: a plain `for` loop over
`[upstream, subject]` in `lib.rs` — no `tokio::join`); NAT-immune; no harness topology change. The
latent fragility (a future parallel-drive refactor would interleave windows) is documented in the
helper + README.

**Fixture content:** `envoy.yaml`/`envoy-rust.yaml` structurally identical (H1 HCM `ingress_http`;
STRICT_DNS cluster `backend` + `dns_lookup_family: V4_ONLY` per L11; vhost
`include_attempt_count_in_response: true` per L6; routes `/retry-success` + `/retry-exhausted` each
`retry_policy: {retry_on: "5xx", num_retries: 1}`). Backend spawn:
`--retry-script /retry-success=fail:1 --per-path /retry-exhausted=503`. `expectations.yaml`
(`http1_keep_alive` driver): probe 1 → 200 + body `ok\n` + `x-envoy-attempt-count: 2` (value-exact);
probe 2 → 503 + body `service unavailable\n` (the last upstream 503 verbatim per L9) +
`x-envoy-attempt-count: 2`; 8 `expected_stats`: `cluster.backend.upstream_rq_retry: 2` /
`_retry_success: 1` / `_retry_limit_exceeded: 1` (L4) / `upstream_rq_total: 4` (per-attempt, L5) /
`upstream_rq_5xx: 1` (completing-only, L5) / `http.ingress_http.downstream_rq_2xx: 1` /
`downstream_rq_5xx: 1` / `downstream_rq_total: 2`. No `allowlist_envoy_only` needed (named-stat driver
ignores unasserted Envoy-only names — the 0022/0023 precedent). Harness additions:
`Http1HeaderValueRule`/`require_header_value` (value-exact header assertion, default-None — inert for
fixtures 0001–0023) + the 0024 spawn arm.

**THE DIFFERENTIAL RESULT (the phase's discriminating observable):**
- `cargo test -p differential --test upstream_retry` → **`test result: ok. 1 passed`** — both proxies
  agree bilaterally on BOTH probes (statuses, bodies, `x-envoy-attempt-count: 2`) AND all 8 stats, with
  ZERO expectation edits (the L4/L5/L6 reconciliation validated end-to-end against real Envoy v1.33.0).
- Regression fixtures re-run locally: 0020 → `ok. 1 passed`; 0022 → `ok. 1 passed`; 0023 → `ok. 1 passed`.

**Verification (quoted):**
- `cargo test -p health-aware-http1-backend` → `test result: ok. 9 passed` (cyclic-modulo unit tests added).
- harness self-test `backend_retry_script_stateful_fail_then_succeed` → ok (asserts the 503,200,503,200 cycle).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean; `cargo fmt --all -- --check` → clean.
- `git show --stat HEAD` → 8 files changed (+553/−85).

**Two-stage review:** spec-compliance **✅ compliant** (reviewer independently re-ran the differential —
PASS; found 3 stale per-source-design doc references → fixed + folded in); code-quality **Approved**
(zero Critical / zero Important; 1 Minor — the parallel-drive latent-fragility caution → added + folded
in). The cyclic-window decision is a test-harness-internal tactical choice (no ADR; documented here +
in the helper/README per the 12.x–15 harness-decision precedent).

---

## Task 8 — in-process retry backstop, success + limit-exceeded paths (commit `273d59be2`)

**Landed.** `crates/envoy-bin/tests/upstream_retry.rs` (single new file): boots `envoy-bin` (tempfile
bootstrap; `kill_on_drop(true)` + piped stderr per the phase-09 M3 discipline) with an H1 listener +
HCM (vhost `include_attempt_count_in_response: true`; routes `/retry-success` + `/retry-exhausted`
each `retry_policy: {retry_on: "5xx", num_retries: 1}`; STATIC cluster `backend`) + admin listener +
an in-process keep-alive stateful backend (cyclic fail:1 / always-503; clean accept loop, no panicking
unwraps). One test, one envoy-bin instance, cumulative counters (the 14.2/15 both-paths discipline):
**(a)** GET `/retry-success` → 200 + `x-envoy-attempt-count: 2` + retry=1/success=1/limit=0/total=2/
5xx=0; **(b)** GET `/retry-exhausted` → 503 + `x-envoy-attempt-count: 2` + cumulative retry=2/limit=1/
success=1/total=4/5xx=1. Stat settling via `poll_stat_until` (25 ms tick, 5 s budget — the phase-15
circuit-breaker backstop's stricter pattern, hardened in review). **H2-upstream-fork retry test
SKIPPED** (justified: no stateful H2 backend machinery exists — the only H2 helper is the always-200
`http2-echo-server`; the H2 retry path is covered structurally by Task 5 + bilaterally by fixture 0024).

**DISCOVERED CARRYFORWARD (pre-existing, NOT a phase-16 defect — for the state-5 review inventory):**
**H1-pool `Connection: close` re-pooling gap (phase 13.1).** The dispatch success arm
(`crates/envoy-http1/src/hcm.rs:462-476`) re-pools the upstream stream WITHOUT inspecting its
`Connection` header (`construct_proxied_response` strips-and-discards it at `router.rs:116`), and
`H1Pool::acquire` (`pool.rs:247-261`) does no liveness check on idle reuse — so a connection the
upstream marked `Connection: close` is handed out dead on the next request → send failure → `Reset` →
synth-502. **Differential gap vs real Envoy** (which never reuses such a connection), currently
invisible because every fixture/helper backend serves keep-alive. Confirmed PRE-EXISTING: the same
gap exists at `821d1f036` (pre-phase-16); `git diff 821d1f036..HEAD -- crates/envoy-http1/src/pool.rs`
is empty (phase 16 never touched the pool). Surfaced by Task 8's first backstop attempt (its backend
sent `Connection: close` → the retry got the dead pooled conn). Candidate fixes for a future phase:
(a) invalidate the PoolGuard in the success arm when the response carries `Connection: close`; (b)
acquire-time EOF/liveness detection. The backstop legitimately uses a keep-alive backend (its purpose
is retry behavior, not pool connection management).

**Verification (quoted):**
- `cargo test -p envoy-bin --test upstream_retry` → `test result: ok. 1 passed` (3 consecutive runs: 0.83 s / 1.33 s / 0.83 s).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean; `cargo fmt --all -- --check` → clean.
- `git show --stat HEAD` → 1 file changed (`upstream_retry.rs` +490).
- Note: the full `cargo test -p envoy-bin` suite has an environmental cold-compile flake in the
  PRE-EXISTING `upstream_h2_connection_pooling` test (its backend helper compiles on demand; cold
  compile can blow the 30 s readiness budget under load) — unrelated to this task; passes warm.

**Two-stage review:** spec-compliance **✅ compliant** (incl. the special investigation that confirmed
the Connection-close gap is pre-existing with file:line evidence); code-quality **Approved** → 1
Important (fixed 200 ms settle sleeps → `poll_stat_until` bounded polling, hardened + folded in) + 3
Minors (all pre-existing helper-copy conventions; no action).
