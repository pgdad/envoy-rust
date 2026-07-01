# Phase 55 — `55-accesslog-rf-overflow-request-budget` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Run every TDD step with `superpowers:test-driven-development`.

**Goal:** Differentially witness the request-budget (`circuit_breakers.thresholds.max_requests`) overflow arm of the ALREADY-witnessed `UO`/`upstream_reset_before_response_started{overflow}` combo at the access-log field level, via a NEW fixture `0063` — closing carry-forward **M50-C**. NO `crates/` change (PLAN-VERIFY reconfirmed the no-op claim this session).

**Architecture:** A single new fixture `0063-accesslog-rf-overflow-request-budget` combining fixture `0025`'s (phase 17) proven `STRICT_DNS`/`{{BACKEND_HOST}}`/`{{HTTP1_BACKEND_PORT}}`/`max_requests:0` cluster shape with fixture `0058`'s (phase 50) proven `{rc,rcd,rf}` `json_format` access-log shape, driven by the existing `http1_access_log_byte_exact` driver via a thin `run_fixture` differential test — plus a `BEHAVIOR_CONTRACT.md` update closing M50-C and folding the pre-existing M54-1 anchor fix. No source code changes: the request-budget tag (`hcm.rs:951`-`:952`) and the `{overflow} => "UO"` derive arm (`hcm.rs:1385`) already produce byte-exact output (reconfirmed this session against a freshly-built `target/debug/envoy-bin`).

**Tech Stack:** the `tests/differential` `run_fixture` harness (`http1_access_log_byte_exact` driver, the `{{HTTP1_BACKEND_PORT}}`-driven `Http1EchoBackend` auto-spawn, unconditional on marker presence — no fixture-name allowlist needed).

## Global Constraints

- **Load-bearing invariant:** all `0001`-`0062` differential fixtures stay BYTE-IDENTICAL. Fixture `0063` is purely ADDITIVE — no existing fixture combines `circuit_breakers.thresholds.max_requests` with a `json_format` access-log (re-grepped this session: only `0025` has `max_requests`, and it carries no `json_format` access-log block).
- **NO new** `Op` / `AccessLogRecord` field / crate / dependency / fuzz-target / `ConfigError` variant / test-harness code. **NO new in-process backstop** — `hcm.rs:7224` (`h1_request_budget_overflow_access_log_carries_uo_flag`, phase 50) already spawns a real listening backend and already asserts the exact byte-exact line in-process (reconfirmed passing this session).
- **The endpoint MUST be reachable.** An unreachable endpoint under `max_requests:0` produces `UF` (a real connect failure — the pre-existing `upstream_cx_total` prefetch divergence, ADR-0047/`BEHAVIOR_CONTRACT.md:401`), NOT `UO`. Fixture `0063` uses the `{{HTTP1_BACKEND_PORT}}`-spawned `Http1EchoBackend` (reachable), never a dead literal address.
- **`#![forbid(unsafe_code)]` holds** — no `unsafe`, no code touched.
- **Exact emitted line (reconfirmed this session against a freshly-built `target/debug/envoy-bin` on a reachable backend, matching the state-1 live-Envoy recon byte-for-byte):** `{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}` (keys sort by UTF-8 byte order: `rc` < `rcd` < `rf`; compact separators + ONE trailing `\n`, per ADR-0092/ADR-0094).
- **Scope locked by ADR-0112.** Ledger head ADR-0112; ADR-0113 reserved-but-unfired (§6.1 split, projected NOT to fire — confirmed by this PLAN's task count); ADR-0114 reserved (§6.2 reconciliation, lands inline ONLY if a §A/§B fact is overturned — none was during PLAN-VERIFY, see below).

---

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `tests/fixtures/0063-accesslog-rf-overflow-request-budget/` (`envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`) | §C fixture — `0025`'s cluster/backend shape + `0058`'s `json_format` shape | 1 |
| `tests/differential/tests/access_log_rf_overflow_request_budget.rs` | §D thin `run_fixture` differential test | 1 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | §E — close M50-C in the `%RESPONSE_FLAGS%` row (`:1020`) + fold M54-1 (5× `hcm.rs:1376`→`:1377` anchor fix in the same row) | 2 |

---

## PLAN-VERIFY summary (SPEC §3 — all four items re-run this session; NO fact overturned → ADR-0114 stays reserved)

- **§3.1 §A/§B no-op — CONFIRMED, twice over.** (a) Re-read `hcm.rs:913`-`:952` (the request-budget gate + `response_code_details_for_log = Some("upstream_reset_before_response_started{overflow}")` set at `:951`-`:952`) and `hcm.rs:1377`-`:1390` (the record-build `response_flags:` derive, with the `Some("upstream_reset_before_response_started{overflow}") => "UO"` arm at `:1385`) — content matches the SPEC's characterization exactly (the derive block shifted from the SPEC's cited `:1377`-`:1390` range by zero lines; unchanged since the phase-54 close-out commit `5ad70eb`). (b) Rebuilt `target/debug/envoy-bin` (`cargo build -p envoy-bin`) and drove a `GET /` against a live config combining a `STRICT_DNS` cluster (`circuit_breakers.thresholds:[{priority:DEFAULT,max_requests:0}]`, single endpoint at a REACHABLE `127.0.0.1` backend) with a `{rc,rcd,rf}` `json_format` access-log: emitted line `{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}` — BYTE-IDENTICAL to the SPEC's state-1 live-Envoy recon. **Confirmed: this PLAN adds NO `crates/` task.**
- **§3.2 fixture-additivity — CONFIRMED.** `grep -rl "max_requests" tests/fixtures/` returns only `0025-upstream-circuit-breaker-retry-budget/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}` (config files) and `0058-accesslog-rf-overflow/README.md` (a prose mention only — `0058`'s actual configs use `max_connections`/`max_pending_requests`, NOT `max_requests`). No fixture combines `max_requests` with a `json_format` access-log block. Fixture `0063` is additive, not a duplicate.
- **§3.3 existing backstop — CONFIRMED still passing.** `cargo test -p envoy-http1 h1_request_budget_overflow_access_log_carries_uo_flag -- --nocapture` → `test result: ok. 1 passed`. The test (`hcm.rs:7224`) spawns a real listening TCP backend (`spawn_fail_then_ok_upstream`) — not a dead port. No new backstop needed.
- **§3.4 fixture-number freedom — CONFIRMED.** `ls tests/fixtures/ | grep 0063` → no match; `0062` is the highest existing fixture number. `0063` is next-free.
- **Marker correction (found during this PLAN-VERIFY, does not overturn any §A-§G fact):** the SPEC's §C prose says fixture `0025`'s `rq_zero` cluster itself "auto-spawns an `Http1EchoBackend` … via the `{{HTTP1_BACKEND_PORT}}` marker". On inspection, `0025` actually uses the OLDER `{{BACKEND_HOST}}`/`{{BACKEND_PORT}}` marker pair, which resolves (via a fixture-name allowlist in `tests/differential/src/lib.rs`) to a `HealthAwareHttp1Backend`, not the generic `Http1EchoBackend`. This is a narrative imprecision in the SPEC about what `0025` itself does — it does NOT change this phase's own design: fixture `0063` uses `{{BACKEND_HOST}}`/`{{HTTP1_BACKEND_PORT}}` directly (the SAME marker pair already proven by fixture `0051-accesslog-upstream-cluster`, phase 43 — a `STRICT_DNS` cluster + json_format access-log combination, the closest existing template). `{{HTTP1_BACKEND_PORT}}` triggers the UNCONDITIONAL marker-only `Http1EchoBackend` spawn (`tests/differential/src/lib.rs:3209`) — simpler than `0025`'s pattern (no fixture-name-allowlist edit needed) and zero new harness code, consistent with the SPEC's "NO new harness code" invariant.

---

## Task 1: §C fixture `0063-accesslog-rf-overflow-request-budget` + §D differential test

**Files:**
- Create: `tests/fixtures/0063-accesslog-rf-overflow-request-budget/envoy-rust.yaml`
- Create: `tests/fixtures/0063-accesslog-rf-overflow-request-budget/envoy.yaml`
- Create: `tests/fixtures/0063-accesslog-rf-overflow-request-budget/expectations.yaml`
- Create: `tests/fixtures/0063-accesslog-rf-overflow-request-budget/README.md`
- Create: `tests/differential/tests/access_log_rf_overflow_request_budget.rs`

**Interfaces:**
- Consumes: the `{{PORT}}` / `{{BACKEND_HOST}}` / `{{HTTP1_BACKEND_PORT}}` markers (the `{{HTTP1_BACKEND_PORT}}` marker auto-spawns `Http1EchoBackend` via `tests/differential/src/lib.rs:3209`, unconditional on marker presence); the `http1_access_log_byte_exact` driver; `differential::run_fixture`.
- Produces: fixture `0063`, witnessed by `access_log_rf_overflow_request_budget.rs`.

> The cluster/backend shape is `0051`'s (`STRICT_DNS` + `{{BACKEND_HOST}}`/`{{HTTP1_BACKEND_PORT}}`, reachable echo backend) PLUS `circuit_breakers.thresholds:[{priority:DEFAULT,max_requests:0}]` (from `0025`'s `rq_zero` cluster). The json_format is `0058`'s `{rc,rcd,rf}` shape verbatim.

- [ ] **Step 1: Create `tests/fixtures/0063-accesslog-rf-overflow-request-budget/envoy-rust.yaml`:**

```yaml
node: { id: envoy-rust-phase-55-fixture-0063, cluster: envoy-rust-phase-55 }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0063-envoy-rust-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rcd: "%RESPONSE_CODE_DETAILS%"
                          rf: "%RESPONSE_FLAGS%"
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: rq_budget_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: rq_zero }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    # STRICT_DNS cluster with circuit_breakers.thresholds.max_requests:0 (the
    # request-budget gate, hcm.rs:913-952) and NO retry_policy. The single
    # endpoint is the SPAWNED reachable Http1EchoBackend ({{BACKEND_HOST}} =
    # host.docker.internal here, 127.0.0.1 on the subject side) at
    # {{HTTP1_BACKEND_PORT}} — the SAME marker pair as fixture 0051. The
    # request-budget gate REJECTS every request before any pool/backend
    # contact (envoy-rust) — the backend is dialed only by Envoy's own
    # connection-pool prefetch (the pre-existing ADR-0047 divergence,
    # BEHAVIOR_CONTRACT.md:401), never actually sent the request. Reachability
    # is load-bearing: a dead endpoint here would surface a real connect
    # failure (UF) instead of the overflow disposition (UO) on the Envoy side.
    - name: rq_zero
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_requests: 0
      load_assignment:
        cluster_name: rq_zero
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {{BACKEND_HOST}}
                      port_value: {{HTTP1_BACKEND_PORT}}
```

- [ ] **Step 2: Create `tests/fixtures/0063-accesslog-rf-overflow-request-budget/envoy.yaml`** (clone of Step 1, reference-Envoy per-side deltas: `0.0.0.0` bind + admin block + `/tmp/0063-envoy-mount/...`):

```yaml
node: { id: envoy-phase-55-fixture-0063, cluster: envoy-phase-55 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0063-envoy-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rcd: "%RESPONSE_CODE_DETAILS%"
                          rf: "%RESPONSE_FLAGS%"
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: rq_budget_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: rq_zero }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    # See envoy-rust.yaml for the request-budget trigger + reachability rationale.
    - name: rq_zero
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_requests: 0
      load_assignment:
        cluster_name: rq_zero
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {{BACKEND_HOST}}
                      port_value: {{HTTP1_BACKEND_PORT}}
```

- [ ] **Step 3: Create `tests/fixtures/0063-accesslog-rf-overflow-request-budget/expectations.yaml`:**

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0063-envoy-mount/access.log
    envoy_rust: /tmp/0063-envoy-rust-mount/access.log
  probes:
    # Probe 1: bare GET / routed to `rq_zero`, a STRICT_DNS cluster whose
    # circuit_breakers set max_requests:0 (the request-budget gate,
    # hcm.rs:913-952) and whose single endpoint is a REACHABLE spawned
    # Http1EchoBackend ({{HTTP1_BACKEND_PORT}}). The request-budget gate
    # rejects every request before any pool/backend dispatch (envoy-rust) →
    # the overflow synth-503, closing carry-forward M50-C (the SECOND of the
    # two set-sites phase 50/ADR-0107 tagged with the identical rcd string —
    # fixture 0058 witnesses only the pool-overflow arm; this fixture
    # witnesses the request-budget arm).
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). The overflow
    # synth-503 is deterministic on BOTH sides (reconfirmed this session
    # against a freshly-built target/debug/envoy-bin, byte-identical to the
    # state-1 live-Envoy recon).
    # state-0/state-2 recon (live v1.33.0 + current envoy-rust tree):
    #   {"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
    #   rc:  "%RESPONSE_CODE%"          → 503  (json NUMBER)
    #   rcd: "%RESPONSE_CODE_DETAILS%"  → "upstream_reset_before_response_started{overflow}"
    #   rf:  "%RESPONSE_FLAGS%"         → "UO"
    # Keys sort by UTF-8 byte order (ADR-0094 §A): rc, rcd, rf. Compact
    # separators + ONE trailing `\n` (ADR-0092 §E). Emitted line:
    #   {"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
    - method: get
      path: /
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 4: Create `tests/fixtures/0063-accesslog-rf-overflow-request-budget/README.md`:**

```markdown
# Fixture 0063 — access-log `%RESPONSE_FLAGS%`/`%RESPONSE_CODE_DETAILS%` overflow path, request-budget arm (byte-exact)

Closes carry-forward **M50-C** (phase 55, ADR-0112). Phase 50 (ADR-0107)
tagged BOTH the connection-pool overflow arms (`hcm.rs:508`/`:515`) AND the
request-budget (`max_requests`) overflow arm (`hcm.rs:951`-`:952`) with the
identical rcd string `upstream_reset_before_response_started{overflow}`,
feeding the same `UO` `%RESPONSE_FLAGS%` derive arm — but fixture `0058`
(phase 50) exercises ONLY the pool-overflow arm (`max_connections:1` /
`max_pending_requests:0` against a dead literal endpoint, no backend spawn).
This fixture witnesses the SECOND set-site: the request-budget arm, via the
code path `0058` cannot reach.

**This is NOT a new `%RESPONSE_FLAGS%` or `%RESPONSE_CODE_DETAILS%` value** —
`UO` and `upstream_reset_before_response_started{overflow}` are already
witnessed (phase 50, fixture `0058`). The request-budget *disposition* itself
(status 503 + body `"...reset reason: overflow"` + stats) is ALSO already
differentially proven by fixture `0025` (phase 17, ADR-0046/ADR-0047) at the
wire/stats level. This fixture's sole new contribution is the
`%RESPONSE_CODE_DETAILS%`/`%RESPONSE_FLAGS%` access-log rendering on the
request-budget arm specifically, which `0025` does not log (no `json_format`
access-log in that fixture).

## What this proves

On a request-budget (`max_requests:0`) rejection against a REACHABLE
endpoint, both proxies return a deterministic 503 and render
`%RESPONSE_CODE_DETAILS%` = `upstream_reset_before_response_started{overflow}`
+ `%RESPONSE_FLAGS%` = `UO`. envoy-rust's request-budget gate
(`try_acquire_request()`, `hcm.rs:913`-`:933`) rejects UNCONDITIONALLY before
any pool/backend contact, tagging the rcd at `hcm.rs:951`-`:952`; the SAME
derive arm (`hcm.rs:1385`) that already handles the pool-overflow arm maps it
to `"UO"`. NO source change was needed — reconfirmed at state-1 (this
project's SPEC session) and re-reconfirmed at state-2 (this session, against
a freshly-built `target/debug/envoy-bin`).

The assertion is **pure cross-proxy equality** — there is NO static expected
literal. The overflow synth-503 is deterministic on both sides.

## The `json_format` map (request-budget overflow route)

| key   | operator                  | rendered value                                       |
|-------|---------------------------|-------------------------------------------------------|
| `rc`  | `%RESPONSE_CODE%`         | `503` (json NUMBER)                                    |
| `rcd` | `%RESPONSE_CODE_DETAILS%` | `upstream_reset_before_response_started{overflow}`     |
| `rf`  | `%RESPONSE_FLAGS%`        | `UO`                                                   |

Keys sort by UTF-8 byte order (ADR-0094 §A): rc, rcd, rf; compact separators
+ ONE trailing `\n` (ADR-0092 §E). Emitted line:

```
{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
```

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below                                          |

```
{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
```

A single probe — the request-budget overflow path is a single pre-loop
rejection arm.

## The request-budget trigger (endpoint MUST be reachable)

`rq_zero` is `STRICT_DNS` ROUND_ROBIN (`dns_lookup_family: V4_ONLY`) with
`circuit_breakers.thresholds` set to `max_requests: 0` and ONE endpoint at
the SPAWNED, REACHABLE `Http1EchoBackend` (`{{BACKEND_HOST}}`:
`{{HTTP1_BACKEND_PORT}}` — the same marker pair as fixture `0051`). On every
`GET /`, the request-budget gate rejects the request with the overflow
synth-503 BEFORE any pool/backend dispatch on the envoy-rust side.

**Reachability is load-bearing, not incidental.** An UNREACHABLE endpoint
under `max_requests:0` instead produces `%RESPONSE_FLAGS%` = `UF` (a REAL
connect attempt) on live Envoy — this is the pre-existing, ALREADY-DOCUMENTED
`upstream_cx_total` connection-pool-prefetch divergence (ADR-0047,
`BEHAVIOR_CONTRACT.md:401`: Envoy prefetches a pool connection even on
reject; envoy-rust's `try_acquire_request()` rejects unconditionally before
any pool contact, regardless of reachability). Using the same
`{{HTTP1_BACKEND_PORT}}`-spawned reachable backend as `0051` (rather than a
dead literal address, the `0058` pattern) is what makes both proxies emit the
SAME `UO` disposition here.

## Per-side divergences

| Side       | bind address | admin block | access-log path                          |
|------------|--------------|-------------|-------------------------------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0063-envoy-mount/access.log`        |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0063-envoy-rust-mount/access.log`   |

The asserted line omits `%UPSTREAM_HOST%`, so the per-side `{{BACKEND_HOST}}`
divergence (`host.docker.internal` vs `127.0.0.1`) never appears in the
compared line — byte-identity holds regardless.

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0051/0053/0056/0057/0058/0059/0060/0061/0062) — drives the probe,
scrapes both files, asserts the scraped line count equals `probes.len()`, and
calls `access_log::assert_access_log_lines_byte_identical`. The
`{{HTTP1_BACKEND_PORT}}` marker triggers the UNCONDITIONAL `Http1EchoBackend`
launch arm in `run_fixture` (`tests/differential/src/lib.rs:3209`) — no new
harness code, no fixture-name allowlist entry needed.

## Cross-references

- ADR: ADR-0112 (phase-55 pick + scope — witness the request-budget overflow
  arm's access-log rendering byte-exact, closing M50-C, via a NEW fixture
  reusing `0025`'s cluster shape + `0058`'s json_format shape).
- Related fixtures: `0058` (`%RESPONSE_FLAGS%` = `UO`, the phase-50 sibling
  witnessing the pool-overflow arm — the SAME rcd string/flag, a DIFFERENT
  set-site), `0025` (phase 17, the pre-existing wire/stats-level proof of
  this exact request-budget disposition — this fixture adds ONLY the
  access-log-level witness), `0051` (the `STRICT_DNS`/`{{BACKEND_HOST}}`/
  `{{HTTP1_BACKEND_PORT}}` reachable-backend + json_format template this
  fixture's cluster/backend shape is built from).
- Consumes: M50-C. Also folds the pre-existing doc-only M54-1 (a
  `BEHAVIOR_CONTRACT.md` anchor off-by-one, unrelated to this fixture's own
  content) while the same contract row is being edited.
- Deferred: the H2 request-budget overflow path (M45-1: no H2 access-log
  differential driver), the unreachable-endpoint `upstream_cx_total`
  prefetch divergence (ADR-0047/`BEHAVIOR_CONTRACT.md:401` — a PRE-EXISTING
  known divergence, not new scope for this phase).
```

- [ ] **Step 5: Create the differential test `tests/differential/tests/access_log_rf_overflow_request_budget.rs`** (structural clone of `access_log_rf_overflow.rs` → `0063`):

```rust
//! Docker-gated differential test for fixture
//! 0063-accesslog-rf-overflow-request-budget.
//! Phase 55 (ADR-0112) — witnesses the request-budget (`max_requests`)
//! overflow arm's access-log rendering byte-exact, closing carry-forward
//! M50-C. Phase 50 (ADR-0107) tagged BOTH the pool-overflow arms
//! (`hcm.rs:508`/`:515`) and the request-budget arm (`hcm.rs:951`-`:952`)
//! with the identical rcd string
//! `upstream_reset_before_response_started{overflow}`, feeding the same `UO`
//! %RESPONSE_FLAGS% derive arm (`hcm.rs:1385`) — but fixture `0058` (phase
//! 50) exercises ONLY the pool-overflow arm. A STRICT_DNS cluster with
//! `circuit_breakers.thresholds.max_requests:0` and a single REACHABLE
//! endpoint (the `{{HTTP1_BACKEND_PORT}}`-spawned `Http1EchoBackend`, the
//! same marker pair as fixture 0051): the request-budget gate rejects every
//! request before any pool/backend dispatch → the overflow synth-503.
//! Reachability is load-bearing — a dead endpoint here would surface a real
//! connect failure (UF, the pre-existing ADR-0047 prefetch divergence)
//! instead of the overflow disposition (UO). Upstream Envoy v1.33 emits the
//! same output here (state-1/state-2 recon:
//! {"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
//! — byte-identical, reconfirmed against a freshly-built envoy-rust binary).
//! Drives `kind: http1_access_log_byte_exact` (a `GET /` probe,
//! `expected_status: 503`, json_format {rc, rcd, rf}); asserts the emitted
//! JSON line is byte-identical. PURE cross-proxy equality (deterministic on
//! both sides). H1-only (H2 deferred — M45-1). NO `crates/` change this
//! phase.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_overflow_request_budget() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0063-accesslog-rf-overflow-request-budget");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 6: Confirm the differential test compiles + is discovered (do NOT expect it green locally — CI is authoritative for backend-spawning fixtures on this dev host).**

Run: `cargo test -p differential --test access_log_rf_overflow_request_budget --no-run`
Expected: compiles cleanly.

- [ ] **Step 7: Commit.**

```bash
git add tests/fixtures/0063-accesslog-rf-overflow-request-budget/ tests/differential/tests/access_log_rf_overflow_request_budget.rs
git commit -m "phase 55 §C+§D: fixture 0063 + differential test (request-budget overflow UO, byte-exact) [ADR-0112]"
```

---

## Task 2: §E BEHAVIOR_CONTRACT update — close M50-C + fold M54-1

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — the `%RESPONSE_FLAGS%` row (`:1020`).

**Interfaces:** documentation only — no code.

- [ ] **Step 1: Close M50-C.** In the `%RESPONSE_FLAGS%` row, find the sentence: "The request-budget (`max_requests`) overflow UO is in-process-backstopped only — differential witness deferred (M50-C)." Replace it with:

```
The request-budget (`max_requests`) overflow UO is now ALSO differentially
witnessed byte-exact at the access-log level by fixture **0063**
(`0063-accesslog-rf-overflow-request-budget`, phase 55, ADR-0112) — the
SECOND of the two set-sites phase 50 (ADR-0107) tagged with the identical rcd
string (fixture 0058 witnesses only the pool-overflow arm); this CONSUMES
carry-forward **M50-C**. Fixture 0025 (phase 17, ADR-0046/ADR-0047) already
proves the SAME disposition at the wire/stats level; the pre-existing
`upstream_cx_total` connection-pool-prefetch divergence noted there
(`BEHAVIOR_CONTRACT.md:401`) is UNCHANGED by this fixture (it requires a
REACHABLE endpoint, unlike 0058's dead-literal-address topology, precisely to
avoid re-triggering that divergence).
```

- [ ] **Step 2: Fold M54-1 — fix the stale `hcm.rs:1376` anchor (appears 5×).** Within the SAME `%RESPONSE_FLAGS%` row, replace every occurrence of `hcm.rs:1376` with `hcm.rs:1377` (the current record-build `response_flags: if retry_limit_exceeded_for_log {` head line, reconfirmed this session — PLAN-VERIFY §3.1). The 5 occurrences are in the `NR`, `UH`, `URX`, `UF`, and `UC` per-flag clauses (the `UO` clause does not cite this anchor at all — leave it unchanged).

Run to locate them precisely before editing:
```bash
grep -n "hcm.rs:1376" docs/envoy-rust/BEHAVIOR_CONTRACT.md
```
Expected (before edit): 5 matches, all on line 1020.

- [ ] **Step 3: Verify the edits.**

Run:
```bash
grep -n "hcm.rs:1376\b" docs/envoy-rust/BEHAVIOR_CONTRACT.md
grep -c "hcm.rs:1377" docs/envoy-rust/BEHAVIOR_CONTRACT.md
grep -n "differential witness deferred (M50-C)" docs/envoy-rust/BEHAVIOR_CONTRACT.md
```
Expected: the first command returns NO matches (all `:1376` anchors in the row are gone); the second returns at least 5 (the row's replaced anchors — other rows/ADR prose may also mention `:1377` incidentally, that's fine); the third returns NO matches (the M50-C deferral sentence is fully replaced, not just appended to).

- [ ] **Step 4: Commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 55 §E: BEHAVIOR_CONTRACT closes M50-C (fixture 0063) + folds M54-1 (hcm.rs:1376→:1377 anchor ×5) [ADR-0112]"
```

---

## Task 3: Final verification sweep (local subset of §7.5)

**Files:** none modified (verification only).

**Interfaces:** none.

- [ ] **Step 1: Local build + lint + format + unit tests (the locally-runnable §7.5 (e) subset).**

Run:
```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test -p envoy-http1
cargo test -p envoy-accesslog
```
Expected: all clean. `h1_request_budget_overflow_access_log_carries_uo_flag` (the pre-existing backstop, unchanged) passes.

- [ ] **Step 2: Confirm the new differential test is discovered and no existing fixture regressed structurally.**

Run:
```bash
cargo test -p differential --test access_log_rf_overflow_request_budget --no-run
ls tests/fixtures/ | grep -c "^00"
```
Expected: the test compiles; the fixture count now includes `0063` (63 numbered fixture directories `0001`-`0063`, allowing for any pre-existing non-`00NN`-prefixed entries). (The full `cargo test --workspace`, `cargo deny check`, the `0063` differential itself, and h2spec are the state-4 verification gate on CI — `0063` spawns a backend so may be LOCAL-flaky on this dev host per memory `differential-host-bridge-ip-192-168-65-2`; CI is authoritative.)

- [ ] **Step 3: Final commit (only if Steps 1–2 surfaced residual edits; otherwise skip — expect no changes here).**

```bash
git add -A
git commit -m "phase 55: final verification sweep cleanup [ADR-0112]"
```

---

## Self-Review (writing-plans checklist)

- **Spec coverage:** §A/§B (no-op, reconfirmed) → PLAN-VERIFY summary, no task (correctly — nothing to implement); §C → Task 1 (Steps 1-4); §D → Task 1 (Step 5); §E → Task 2; §F (carry-forward closure) → Task 2 Step 1; §G (no new backstop, reconfirmed) → PLAN-VERIFY summary, no task. Fuzz → SKIP per SPEC §2 (no new operator/grammar). All §A-§G covered; none require a code task.
- **Placeholder scan:** every fixture/test file step shows complete file content; every command shows expected output. No TODO/TBD.
- **Type consistency:** N/A — no new Rust types/signatures this phase (fixture + doc only). Marker names (`{{BACKEND_HOST}}`, `{{HTTP1_BACKEND_PORT}}`, `{{PORT}}`) match the `0051` precedent verbatim.
- **§6.1 gate:** 3 tasks / ~4 new files (fixture dir + 1 differential test) + 1 doc edit — roughly 150-250 lines of new fixture/test/doc content, ZERO `crates/` LoC. Well under the ~25-task/~1500-LoC gate. **ADR-0113 stays reserved-but-unfired.** No split.
- **§6.2:** no §A-§G fact overturned during PLAN-VERIFY (source lines unchanged, byte-exact output reconfirmed twice, existing backstop still passes, fixture number still free — only a narrative marker-name imprecision in the SPEC's own prose was found, which does not change this phase's design). **ADR-0114 stays reserved** (not fired inline).
