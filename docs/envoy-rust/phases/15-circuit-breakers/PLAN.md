# Phase 15 (`15-circuit-breakers`) — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`
> per `feedback_execution_style` auto-memory and per the established 06.x → 14.2 cadence.
> Tasks 1–11 implement the phase per `SPEC.md` **as revised by ADR-0043** (read §0.B FIRST —
> the SPEC's central premise was empirically falsified at PLAN-write; the PLAN encodes the
> corrected scope). Steps use `- [ ]` checkbox syntax. One commit per task (Tasks 1–10);
> Task 11 is the state-6 close-out (a later session). Dispatch implementer subagents
> **SERIALLY, never in parallel** (`feedback_serial_subagent_dispatch` — they race on `main`).

**Goal.** Make the **already-enforced `max_connections` circuit breaker observable + differentially
verified**, and faithfully implement the `max_pending_requests: 0` no-queue reject path. Land
(1) the `cluster.<name>.upstream_cx_overflow` counter + `cluster.<name>.circuit_breakers.default.cx_open`
gauge (the ADR-0042 observability subset), (2) a faithful `max_pending_requests: 0`
reject-on-establish gate + the `cluster.<name>.upstream_rq_pending_overflow` counter, (3) the
overflow-503 wire reconciliation (Envoy's byte-exact 81-byte `…reset reason: overflow` body +
`x-envoy-overloaded: true`), and (4) the project's bilateral fixture `0023` proving the
pending-overflow reject path — **timing-robust (single GET, no concurrency)**. With 15 landed,
the `max_connections` breaker is observable and the overflow-reject wire shape is differentially
green.

**Architecture.** The two observability stats + the new pending-reject gate live **inside the
existing H1/H2 pools** (`crates/envoy-http1/src/pool.rs`, `crates/envoy-http2/src/pool.rs`) at
the `acquire()` cap-check site — the SAME single-source-of-truth site that already enforces
`max_connections` (13.x). `upstream_cx_overflow` increments at the cap-check branch;
`circuit_breakers.default.cx_open` is an **edge-driven** gauge (`set(1)` when an `established`
increment reaches `max_connections`; `set(0)` at the decrement edges that drop below) — NOT
polled. The `max_pending_requests: 0` gate fires on the connect-on-miss path **before** the
cap-check (matching Envoy: a request that must establish a connection needs a pending slot;
with 0 slots it is rejected → `upstream_rq_pending_overflow` + 503, and `upstream_cx_overflow`
stays 0 because no connection demand reaches the cap). All three stats register only for
clusters that configure `circuit_breakers` (inert-when-unconfigured — the 14.1 conditional-
registration discipline; the 22 existing fixtures see zero behavior change). The overflow-503
wire reconciliation adds a `synth_overflow()` helper (adjacent to `synth_no_healthy_upstream`)
emitting Envoy's byte-exact body + `x-envoy-overloaded: true`, wired into BOTH the pending-
reject arm and the existing cap-overflow arm, on H1 AND H2 (the H2 overflow arm currently
mis-synthesizes a **502** — corrected to 503 here; §0.B C-1). **No new crate; no new top-level
Cargo dep; no `unsafe`.**

**Tech Stack.** Zero new top-level Cargo deps. Primitives: `parking_lot::Mutex` (the pools'
existing `established` lock — the gauge edges run under it), `std::sync::Arc<envoy_stats::{Counter,
Gauge}>`, `tokio` (existing). The differential harness reuses the EXISTING `Driver::Http1KeepAlive`
(14.2 already extended it with per-request body + header presence/absence assertions + a
post-settle stat scrape) — **no new harness Driver** (the SPEC's `Driver::Http1Concurrent` +
`--hold-ms` backend knob are DROPPED per ADR-0043 — the bilateral fixture needs neither
concurrency nor a slow backend). The in-process backstop drives concurrency directly with
`tokio::join!`. The `parse_bootstrap` fuzz corpus stays at 22 seeds (the existing
`cluster_circuit_breakers.yaml` is extended IN PLACE — already `.gitignore`-allow-listed).

---

## 0.A Architecture lock-ins

Settled at PLAN-write; subagents implement as written and do NOT re-litigate. Numbered for
cross-reference from PROGRESS.

1. **No split, no nest-split.** The re-scoped surface is **~11 tasks / ~1100–1250 LoC**
   (production ~430, tests/backstop ~520, fixture/docs ~300) — UNDER the `BOOTSTRAP_PROMPT.md`
   §6.1 ~1500-LoC / ~25-task gate. **The reserved split ADR-0044 does NOT fire.** Standalone
   PLAN posture per `feedback_pick_recommendation` (no fork).

2. **ADR-0043 lands at the state-2 PLAN-write commit** (this PLAN's commit), recording the §6.2
   empirical re-scope (the SPEC's `{200,503}`/`upstream_cx_overflow:1` premise is empirically
   false; the bilateral fixture pivots to the `max_pending_requests:0` reject path). DECISIONS.md
   ledger head becomes **ADR-0043** (count 44); next available **ADR-0044** (still reserved, now
   projected NOT to fire). The PLAN-write commit title carries **`[ADR-0043]`**. No further ADR
   is expected in the 15 lifecycle (a state-3 ADR fires only if execution surfaces a genuine
   unforeseen constraint — unlikely).

3. **The §6.2 empirical verification is DONE** (this PLAN-write's prologue; ratified by
   ADR-0043). **Do NOT re-run the Docker verification.** The locked facts (§0.C) the PLAN bakes
   in are authoritative; the projection values in the ratified SPEC §2.1/§2.2 are SUPERSEDED by
   §0.C where they conflict.

4. **Inert-when-unconfigured (regression-equivalence, acceptance gate (b)).** The three new
   stats (`upstream_cx_overflow`, `circuit_breakers.default.cx_open`, `upstream_rq_pending_overflow`)
   register ONLY inside `H1PoolManager::for_bootstrap` / `H2PoolManager::for_bootstrap` for
   clusters whose `cfg.circuit_breakers.is_some()`. A cluster without `circuit_breakers` gets
   NO such stat registered AND its pool's `max_pending_requests` defaults to the as-today value
   (1024 → the gate never fires). The 22 existing Docker-gated fixtures (0001–0022) see ZERO
   behavior change. **The `max_pending_requests` pool field defaults to `DEFAULT_MAX_PENDING_REQUESTS
   = 1024`** so an unconfigured/default cluster behaves exactly as 13.x.

5. **One-source-of-truth stat sites (the 06.x→14.x discipline).** `upstream_cx_overflow`
   increments at exactly ONE site per protocol — the pool cap-check branch (`H1Pool::acquire`
   `pool.rs:204`, `H2Pool::acquire` `pool.rs:307`), BEFORE the existing `return
   Err(PoolError::Overflow)`. `upstream_rq_pending_overflow` increments at exactly ONE site per
   protocol — the new pending-reject branch on the connect-on-miss path, BEFORE the cap-check.
   `cx_open` updates at exactly the `established`-count mutation edges (no polling). No double-
   counting.

6. **`cx_open` edge semantic — at-cap inclusive (§0.C finding 4).** `set(1)` when an
   `established` increment makes `*n == max_connections` (the cluster is AT the cap; the next
   connect would overflow). `set(0)` when an `established` decrement makes `*n < max_connections`.
   Concretely, in `acquire()` after `*n += 1` (`pool.rs:210`): `if *n >= self.max_connections {
   self.cx_open.set(1); }`. At EACH decrement edge — `PoolGuard::Drop` destroy path
   (`pool.rs:140-142`), connect-failure rollback (`pool.rs:217-220`), idle-sweeper eviction
   (`pool.rs:296-299`) — after the `saturating_sub`: `if *n < self.max_connections {
   self.cx_open.set(0); }`. The edge updates run UNDER the held `established` lock (already held
   at all four sites). **`cx_open` is a per-cluster gauge but `established` is per-endpoint
   (`HashMap<SocketAddr,u32>`)** — for the SINGLE-endpoint fixtures (0020, 0023, the backstop)
   they coincide exactly; the multi-endpoint reconciliation defers (§5.4 / §0.C finding 5 / the
   carryforward). The gauge is terminal-0 (returns to 0 after drain) so a post-settle scrape is
   deterministic.

7. **`max_pending_requests:0` reject ordering — pending-check BEFORE cap-check (§0.C finding 1).**
   In `acquire()`, after the idle-reuse block fails (no idle stream → must establish a new
   connection), the FIRST gate is `if self.max_pending_requests == 0 { self.rq_pending_overflow.inc();
   return Err(PoolError::PendingOverflow { cluster, .. }); }`, THEN the existing cap-check. This
   reproduces Envoy exactly: under `max_pending_requests:0` the very first cold connect-on-miss
   is rejected (`upstream_rq_pending_overflow` ticks; `upstream_cx_overflow` + `upstream_cx_total`
   stay 0; the pool never warms; the backend is never contacted). Under the default
   `max_pending_requests = 1024`, the gate is false → the path proceeds to the cap-check exactly
   as 13.x (the actual queue-when-at-cap behavior of `max_pending_requests>0` is the DEFERRED
   work — envoy-rust still 503s at-cap where Envoy would queue; that divergence is documented,
   not fixed, this phase).

8. **`PoolError` gains one variant: `PendingOverflow { cluster: String }`.** Distinct from
   `Overflow` (cap-hit) so the router arm can map it to the same `synth_overflow()` 503 (both
   are "overflow" local replies in Envoy). The H1 router arm at `hcm.rs:542` adds a
   `PoolError::PendingOverflow` match arm alongside `Overflow`, both → `synth_overflow(close)`.

9. **Overflow-503 wire shape — `synth_overflow()` helper (§0.C finding 3, D5).** A new helper
   adjacent to `synth_no_healthy_upstream` (`crates/envoy-http1/src/hcm.rs:1068`) emitting:
   status **503**, body the byte-exact **81-byte** `upstream connect error or disconnect/reset
   before headers. reset reason: overflow` (NO trailing newline), and the header set `{server,
   date, content-length: 81, content-type: text/plain, connection: <close>, x-envoy-overloaded:
   true}`. **The `x-envoy-overloaded: true` header is the wire surfacing of Envoy's `UO`
   response flag** (which is otherwise access-log-only — §0.C finding 3). Envoy itself omits the
   `connection` header on this reply; envoy-rust keeps its standard 5-header synth shape + adds
   `x-envoy-overloaded` (6 headers). The harness tolerates the extra `connection` header
   (allow-listed — the 0019/0022 synth-503 precedent is green with it). `synth_overflow` is
   called from BOTH the pending-reject arm AND the cap-overflow arm, on H1 AND H2.

10. **H2 overflow + pending arms emit 503, not 502 (§0.B C-1 — corrects ADR-0042 §0).** The H2
    router path (`crates/envoy-http2/src/hcm.rs:368-380`) currently maps `PoolError::Overflow`
    to `Err(String)` which funnels into `synth_h2_502()` (a **502** — contradicting ADR-0042
    §0's claim that "both router arms already reject overflow with a synth-503"). Task 5 routes
    both the `Overflow` and the new `PendingOverflow` arms to a 503-with-overflow-body
    (`synth_h2_overflow()`, the H2 sibling of `synth_overflow` — no `connection` header per the
    H2 synth convention, see `synth_h2_502` at `hcm.rs:672`), returned early via
    `finalize_h2_stream` (mirroring the existing `synth_h2_502` early-return at `hcm.rs:399-421`).
    No H2 differential fixture exercises this (fixture 0023 is H1) — it is guarded by H2 pool
    unit tests + the symmetry requirement; the correction lands for wire-faithfulness + H1/H2
    parity.

11. **The bilateral fixture uses the EXISTING `Driver::Http1KeepAlive` — no new harness Driver.**
    14.2 already extended `Http1KeepAlive` with per-request body + header presence/absence
    assertions + a post-settle `expected_stats` admin scrape (the 0022 shape). Fixture 0023
    drives a SINGLE GET (`requests: [{GET / expected_status:503 + 81-byte body +
    require_header_present:[x-envoy-overloaded]}]`) + `expected_stats` asserting
    `upstream_rq_pending_overflow:1` + `upstream_cx_overflow:0` + `upstream_cx_total:0` +
    `circuit_breakers.default.cx_open:0`. **No concurrency, no slow backend** (the backend is
    never contacted under `max_pending_requests:0`). The SPEC's `Driver::Http1Concurrent` +
    `--hold-ms` knob are DROPPED.

12. **The in-process backstop (Task 8) validates BOTH overflow paths + the non-zero
    `cx_overflow`/`cx_open` (§0.C findings 1+4; the 14.2 both-directions discipline).** It boots
    `envoy-bin` with a synthesized bootstrap + an in-process H1 backend, and asserts: **(a) the
    pending-overflow path** — a `max_pending_requests:0` cluster, single request → 503 + the
    81-byte body + `x-envoy-overloaded` + `upstream_rq_pending_overflow:1` + `upstream_cx_total:0`;
    **(b) the cx-overflow path** — a `max_connections:1` cluster (default pending) + K=2 concurrent
    (`tokio::join!`) requests against a hold-capable in-process backend → the `{200,503}` status
    multiset + `upstream_cx_overflow:1` + `cx_open` observed at `1` while saturated and `0` after
    drain. Path (b) is envoy-rust-internal (Envoy would serve `{200,200}` there — bilaterally
    deferred with the queue). Includes the 5-standard-header + `x-envoy-overloaded` presence
    assertion on the 503 (the 10/11/12.2/14.2 synth-header discipline). The backstop's hold-
    capable backend is a small in-test tokio TCP server that sleeps before responding (no
    dependency on the `health-aware-http1-backend` helper — the `--hold-ms` knob is dropped).

13. **No new top-level Cargo dep; no new workspace member; no `unsafe`.** Phase 15 touches
    `envoy-config`, `envoy-http1`, `envoy-http2`, `tests/differential`, `tests/fixtures`,
    `crates/envoy-bin/tests`. Every crate root keeps `#![forbid(unsafe_code)]`.

14. **Isolated-crate build discipline (`project_isolated_crate_build_blindspot`; SPEC §6.7).**
    `cargo build --workspace` can be GREEN while `cargo build -p <crate>` FAILS (feature
    unification). The Task 10 state-4 verification MUST run `cargo build -p envoy-config`,
    `cargo build -p envoy-http1`, `cargo build -p envoy-http2` STANDALONE (in addition to the
    workspace build) and quote each in PROGRESS.

15. **Subagent-driven execution at state 3** (`feedback_execution_style`), implementers
    dispatched SERIALLY (`feedback_serial_subagent_dispatch`). Per-task PROGRESS sections quote
    `cargo fmt --all -- --check` at every task close (06.1 R-9). State-4 evidence-discipline
    (05.3→14.2): real CI run URL + HEAD SHA + completion timestamp + per-gate quoted output.

---

## 0.B PLAN-time SPEC corrections (the 06.2 → 14.2 cadence; the SPEC is ratified + NOT edited)

These corrections reconcile the ratified SPEC against the PLAN-time HEAD (`35236eec3`) + the
§6.2 empirical findings. They live HERE + in the PROGRESS Task 1 preamble; the SPEC is not
mutated.

- **C-0 (the load-bearing re-scope — ADR-0043).** The SPEC §1/§2.1/§2.3 central premise (`cap=1`
  + `max_pending_requests:0` + K=2 concurrent → `{200,503}` + `upstream_cx_overflow:1`) is
  **empirically FALSE**. Under `max_pending_requests:0` Envoy rejects EVERY request (even K=1)
  with a 503 via `upstream_rq_pending_overflow`; `upstream_cx_overflow` + `upstream_cx_total`
  stay 0. The bilateral fixture pivots to a **single-GET `max_pending_requests:0` reject** (both
  proxies 503). The `upstream_cx_overflow`/`cx_open` observability is retained but validated
  inert-0 bilaterally + non-zero in-process only. See ADR-0043 + §0.C.
- **C-1 (H2 overflow emits 502, not 503).** Contradicting ADR-0042 §0 ("both router arms already
  reject overflow with a synth-503"), the H2 arm (`http2/src/hcm.rs:368-380`) maps
  `PoolError::Overflow` → `Err(String)` → `synth_h2_502()` (**502**). Task 5 corrects it to 503
  (lock-in #10). Pre-existing; no H2 fixture exercises it; differential-inert for the existing 22.
- **C-2 (the existing deny_unknown test must be re-pointed).** `bootstrap.rs::tests::
  cluster_circuit_breakers_rejects_phase13_deferred_threshold_fields` (`:9058`) currently uses
  `max_pending_requests: 5` to prove `deny_unknown_fields` rejects it. Once Task 1 adds the
  `max_pending_requests` field, that value parses + is rejected by the VALIDATOR instead. Task 1
  re-points this test to a still-deferred field (`max_requests: 5`) to keep testing
  `deny_unknown_fields`, AND adds a new validator test for `max_pending_requests > 0`.
- **C-3 (fixture rename).** `0023-upstream-circuit-breaker-max-connections` (SPEC) →
  **`0023-upstream-circuit-breaker-max-pending-requests`** (the subject is the
  `max_pending_requests:0` reject path). ROADMAP row 15 summary updates at this commit.
- **C-4 (D6 + `--hold-ms` dropped).** The SPEC's `Driver::Http1Concurrent` (D6) + the
  `health-aware-http1-backend --hold-ms` knob are DROPPED — the bilateral fixture needs neither
  (lock-in #11). The backend helper is UNTOUCHED this phase.
- **C-5 (D8 fuzz seed — extend in place, no count change).** The existing
  `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml` (already
  `.gitignore`-allow-listed at line 27) is extended with `max_pending_requests: 0`. Corpus stays
  at 22 seeds; NO `.gitignore` / NO `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array
  edit (the file is already listed). The seed must remain parse-CLEAN (`max_pending_requests:0`
  is accepted), so it stays in the success set.

---

## 0.C §6.2 empirical lock-ins (ADR-0043; locked — do NOT re-verify)

Captured from `envoyproxy/envoy:v1.33.0` at PLAN-write. Each is authoritative.

1. **`max_pending_requests:0` rejects ALL requests** (K=1, K=2, sequential) with a 503; the pool
   never warms; backend receives 0 requests; `upstream_cx_total:0`; `upstream_cx_overflow:0`.
   The live counter is `upstream_rq_pending_overflow` (= reject count).
2. **`upstream_cx_overflow`** ticks on a cap-HIT regardless of subsequent queueing. Control
   (default pending, cap=1, K=2): both served 200, `upstream_cx_overflow:1`, `upstream_cx_total:2`.
   So envoy-rust incrementing `upstream_cx_overflow` at its cap-check site is name-and-semantics
   matched; the cross-proxy VALUE matches on a cap-hit (both 1) but the downstream STATUS
   diverges (Envoy queues→200; envoy-rust 503s) — hence cx-overflow is asserted bilaterally only
   at inert-0 (fixtures 0020/0023, no cap-hit), non-zero in-process only.
3. **Overflow-503 wire shape:** status 503; body **81 bytes** byte-exact `upstream connect error
   or disconnect/reset before headers. reset reason: overflow` (no trailing `\n`); header
   `x-envoy-overloaded: true`; Envoy's full set `{x-envoy-overloaded, content-length:81,
   content-type:text/plain, date, server:envoy}` (no `connection`, no `x-envoy-upstream-service-time`).
   `UO` flag is access-log-only (wire surface = `x-envoy-overloaded`).
4. **`cx_open` is at-cap inclusive:** `cx_open=1` exactly when `upstream_cx_active ==
   max_connections`; `0` after drain. Only observable in a config that opens a connection (NOT
   the `max_pending_requests:0` config, where it never leaves 0).
5. **`max_connections` is per-cluster/per-priority** (stat namespace has no host component);
   single-endpoint fixture ⇒ per-cluster == per-endpoint (the bilateral-safe topology).
6. **Full `circuit_breakers.*` always-emitted gauge set (10):** `{default,high}.{cx_open,
   cx_pool_open, rq_open, rq_pending_open, rq_retry_open}` — both priorities emitted regardless
   of config. envoy-rust emits ONLY `default.cx_open` at phase-15 scope; the harness
   `Http1KeepAlive` driver scrapes only NAMED stats (no full-tree diff), so no
   `allowlist_envoy_only` enumeration is needed for fixture 0023 — but the BEHAVIOR_CONTRACT row
   (Task 9) records the Envoy-only siblings as deferred.
7. **The overflow-rejected request does NOT increment `upstream_cx_total`** (no connect
   attempted) — matches the existing `hcm.rs:542` comment, on both proxies.

---

## 0.D Carryforward dispositions (none gate phase 15)

- **Per-endpoint vs per-cluster `cx_open`** (SPEC §5.4; §0.C finding 5) — sidestepped by the
  single-endpoint fixtures; owner = a future multi-endpoint circuit-breaker phase. NOT
  reconciled here.
- **The `max_pending_requests > 0` pending-request QUEUE** (+ `rq_pending_open` /
  `upstream_rq_pending_active` / `upstream_rq_pending_total`) — deferred (ADR-0042 §4 / ADR-0043
  option d). The `{200,200}` queue-and-serve bilateral overflow fixture defers to that phase.
- **The standing multi-phase Minor inventory** (14.1 REVIEW M1/M2/M3/M7/M9; M-c1 `tokio-util`
  `["rt"]`-feature leanness; M-c2 `.lock().unwrap()` poisoning-hardening; M-c3 frozen-record
  "14"s) + the §6.9 per-class `upstream_rq_{2,3,4}xx` extension + the `upstream_cx_total`
  TCP-proxy carve-out + **ADR-0028** (H1-listener × H2-cluster dispatch deferral) — all carry
  forward unchanged; phase 15 closes none.

---

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | `Thresholds.max_pending_requests` field + validator | 1 |
| `crates/envoy-config/src/lib.rs` | `ConfigError::UnsupportedNonZeroMaxPendingRequests` | 1 |
| `crates/envoy-http1/src/pool.rs` | H1 pending-reject gate + 3 new stats + cx_open edges | 2, 3 |
| `crates/envoy-http1/src/hcm.rs` | `synth_overflow()` + H1 overflow/pending arms | 4 |
| `crates/envoy-http2/src/pool.rs` | H2 pending-reject gate + 3 new stats + cx_open edges | 5 |
| `crates/envoy-http2/src/hcm.rs` | `synth_h2_overflow()` + H2 overflow/pending arms (502→503) | 5 |
| `tests/fixtures/0023-upstream-circuit-breaker-max-pending-requests/` | bilateral fixture | 6 |
| `tests/differential/tests/upstream_circuit_breaker.rs` | Docker-gated wrapper | 6 |
| `tests/fixtures/0020-…/expectations.yaml` | inert-0 cx_overflow + cx_open assertions | 7 |
| `crates/envoy-bin/tests/upstream_circuit_breaker.rs` | in-process backstop (both paths) | 8 |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml` | fuzz seed +`max_pending_requests:0` | 9 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | 3 stat rows + overflow-503 row + divergence note | 4, 9 |

---

## Task 1: `envoy-config` schema + validator for `max_pending_requests`

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (the `Thresholds` struct `:1180-1185`; the
  `validate_circuit_breakers` fn `:2583-2613`; the test `:9058`)
- Modify: `crates/envoy-config/src/lib.rs` (add `ConfigError` variant near `:462`)

- [ ] **Step 1: Write the failing tests.** In the `#[cfg(test)] mod tests` block near the
  existing circuit-breaker tests (`bootstrap.rs:~8994`), add:

```rust
#[test]
fn cluster_max_pending_requests_zero_accepted() {
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - max_connections: 1
            max_pending_requests: 0
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 8080 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
    let bootstrap = crate::parse_bootstrap(yaml).expect("max_pending_requests:0 must parse+validate");
    assert_eq!(
        bootstrap.static_resources.clusters[0].circuit_breakers.as_ref().unwrap()
            .thresholds[0].max_pending_requests,
        Some(0)
    );
}

#[test]
fn cluster_max_pending_requests_positive_rejected_by_validator() {
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - max_connections: 1
            max_pending_requests: 5
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 8080 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("max_pending_requests>0 must be rejected");
    let msg = format!("{err:#}");
    assert!(msg.contains("max_pending_requests"), "got: {msg}");
}
```

  AND re-point the existing `cluster_circuit_breakers_rejects_phase13_deferred_threshold_fields`
  test (§0.B C-2): change its YAML field `max_pending_requests: 5` → `max_requests: 5` and its
  assertion substring `"max_pending_requests"` → `"max_requests"` (it still proves
  `deny_unknown_fields` rejects a STILL-deferred field).

- [ ] **Step 2: Run to verify failure.** Run: `cargo test -p envoy-config max_pending_requests`
  Expected: FAIL — `max_pending_requests` is an unknown field (parse error) on the accept test;
  field access does not compile until the struct grows it.

- [ ] **Step 3: Add the struct field.** In `crates/envoy-config/src/bootstrap.rs`, extend
  `Thresholds` (`:1180-1185`) and update its doc comment to note phase-15:

```rust
/// 13.1 D1: a single circuit-breaker threshold entry. See `CircuitBreakers`.
/// 15 D1: `max_pending_requests` added — accepts `0` ONLY (the no-queue carve-out;
/// matches Envoy's reject-on-establish behavior per ADR-0043 §6.2 finding 1).
/// `max_pending_requests > 0` (the pending-request queue) is rejected by the validator
/// and deferred. Other phase-13/15-deferred fields (`max_requests`, `max_retries`,
/// `max_connection_pools`, `track_remaining`, `retry_budget`) stay rejected by
/// `deny_unknown_fields`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<RoutingPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pending_requests: Option<u32>,
}
```

- [ ] **Step 4: Add the `ConfigError` variant.** In `crates/envoy-config/src/lib.rs` near the
  other circuit-breaker variants (`:445-462`), add (mirror the `InvalidMaxConnections` shape +
  its `#[error(...)]` attribute style):

```rust
/// 15 D2: `max_pending_requests > 0` (the pending-request queue) is deferred per ADR-0043;
/// only `max_pending_requests: 0` (no-queue) is supported at phase-15 scope.
#[error("cluster '{cluster}': max_pending_requests={value} is unsupported; only 0 (no-queue) is accepted at this scope")]
UnsupportedNonZeroMaxPendingRequests { cluster: String, value: u32 },
```

- [ ] **Step 5: Extend the validator.** In `validate_circuit_breakers` (`bootstrap.rs:2594-2611`),
  inside the `if let Some(t) = cb.thresholds.first()` block, after the `max_connections` check,
  add:

```rust
        if let Some(value) = t.max_pending_requests
            && value > 0
        {
            return Err(crate::ConfigError::UnsupportedNonZeroMaxPendingRequests {
                cluster: cluster.name.clone(),
                value,
            });
        }
```

- [ ] **Step 6: Run to verify pass.** Run: `cargo test -p envoy-config circuit_breaker max_pending`
  Expected: PASS (all circuit-breaker tests including the re-pointed deny-unknown test).
  Also run `cargo build -p envoy-config` (standalone, lock-in #14) — Expected: `Finished`.

- [ ] **Step 7: fmt + commit.** Run `cargo fmt --all -- --check` (Expected: clean). Then:

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 15 Task 1: max_pending_requests schema + validator (accept 0, reject >0) [ADR-0043]"
```

---

## Task 2: H1 pool — `max_pending_requests:0` reject gate + `upstream_rq_pending_overflow`

**Files:**
- Modify: `crates/envoy-http1/src/pool.rs` (`PoolError` `:28-36`; `H1Pool` struct `:45-71`;
  `H1Pool::new` `:152-172`; `acquire` `:177-234`; `H1PoolManager::for_bootstrap` `:340-398`)

- [ ] **Step 1: Write the failing test.** In `pool.rs::tests`, add a test that builds a pool with
  `max_connections: 1`, `max_pending_requests: 0` and asserts `acquire()` returns
  `PoolError::PendingOverflow` WITHOUT contacting the backend, and that the
  `upstream_rq_pending_overflow` counter reads 1. Mirror the existing `mk_pool` helper
  (`:424`); extend it (or add `mk_pool_full`) to pass `max_pending_requests` + return the new
  counter handle:

```rust
#[tokio::test]
async fn acquire_rejects_with_pending_overflow_when_max_pending_requests_zero() {
    // A backend that PANICS if contacted — proves the pool never connects.
    let (pool, _reg, rq_pending_overflow) = mk_pool_pending("c", /*max_conn*/ 1, /*max_pending*/ 0);
    let endpoint: SocketAddr = "127.0.0.1:1".parse().unwrap(); // unroutable; never dialed
    let err = pool.acquire(endpoint, "c").await.expect_err("must reject");
    assert!(matches!(err, PoolError::PendingOverflow { .. }));
    assert_eq!(rq_pending_overflow.value(), 1);
}
```

- [ ] **Step 2: Run to verify failure.** Run: `cargo test -p envoy-http1 pending_overflow`
  Expected: FAIL — `PoolError::PendingOverflow` + the `max_pending_requests` field + the
  `rq_pending_overflow` handle don't exist yet.

- [ ] **Step 3: Add the `PoolError` variant.** In `pool.rs:28-36`:

```rust
    /// Pool's `max_pending_requests` is 0 and a new connection must be established
    /// (no idle stream to reuse). Envoy reject-on-establish parity (ADR-0043 §6.2 finding 1).
    #[error("upstream pending-request overflow: cluster='{cluster}' (max_pending_requests=0)")]
    PendingOverflow { cluster: String },
```

- [ ] **Step 4: Add the field + counter + constructor params.** Add to `H1Pool`:
  `max_pending_requests: u32,` and `rq_pending_overflow: Arc<envoy_stats::Counter>,`. Add a
  module const `const DEFAULT_MAX_PENDING_REQUESTS: u32 = 1024;` near `DEFAULT_MAX_CONNECTIONS`
  (`:22`). Extend `H1Pool::new`'s signature with `max_pending_requests: u32` +
  `rq_pending_overflow: Arc<envoy_stats::Counter>` and set both fields.

- [ ] **Step 5: Insert the reject gate in `acquire()`.** After the idle-reuse block closes
  (`pool.rs:199`, the `}` ending the idle-reuse scope) and BEFORE the connect-on-miss cap-check
  block (`:201`), insert:

```rust
        // 15 D3 (lock-in #7): max_pending_requests:0 reject-on-establish. A new connection
        // must be established (no idle stream); under max_pending_requests:0 Envoy rejects
        // before any connect (ADR-0043 §6.2 finding 1). Fires BEFORE the cap-check so
        // upstream_cx_overflow stays 0 (no connection demand reaches the cap).
        if self.max_pending_requests == 0 {
            self.rq_pending_overflow.inc();
            return Err(PoolError::PendingOverflow {
                cluster: self.cluster_name.clone(),
            });
        }
```

- [ ] **Step 6: Register + source the counter + field in `for_bootstrap`.** In
  `H1PoolManager::for_bootstrap` (`pool.rs:358-395`), after the existing `max_connections`
  extraction, add (gated on `circuit_breakers` configured per lock-in #4):

```rust
            let max_pending_requests = cfg
                .circuit_breakers
                .as_ref()
                .and_then(|cb| cb.thresholds.first())
                .and_then(|t| t.max_pending_requests)
                .unwrap_or(DEFAULT_MAX_PENDING_REQUESTS);
            // 15 D3: register only when circuit_breakers configured (inert-when-unconfigured).
            let rq_pending_overflow = if cfg.circuit_breakers.is_some() {
                registry.register_counter(&format!(
                    "cluster.{}.upstream_rq_pending_overflow", cfg.name))?
            } else {
                // Unconfigured clusters get a throwaway unregistered counter (never incremented
                // because max_pending_requests defaults to 1024 → the gate never fires).
                Arc::new(envoy_stats::Counter::new())
            };
```

  Pass `max_pending_requests` + `rq_pending_overflow` into `H1Pool::new(...)`. (Confirm
  `envoy_stats::Counter::new()` is `pub`; if not, register under a per-cluster name
  unconditionally — but prefer the conditional registration so unconfigured clusters emit no
  such stat per lock-in #4. If `Counter::new()` is not public, gate the WHOLE pending logic so
  the unconfigured pool stores `None` for the handle and the gate short-circuits on
  `max_pending_requests==1024` before touching it.)

- [ ] **Step 7: Run to verify pass.** Run: `cargo test -p envoy-http1 pending_overflow` then
  `cargo test -p envoy-http1` — Expected: PASS (all H1 pool tests). `cargo build -p envoy-http1`
  standalone — Expected: `Finished`.

- [ ] **Step 8: fmt + commit.**

```bash
cargo fmt --all -- --check
git add crates/envoy-http1/src/pool.rs
git commit -m "phase 15 Task 2: H1 pool max_pending_requests:0 reject + upstream_rq_pending_overflow [ADR-0043]"
```

---

## Task 3: H1 pool — `upstream_cx_overflow` counter + `circuit_breakers.default.cx_open` gauge

**Files:**
- Modify: `crates/envoy-http1/src/pool.rs` (`H1Pool` struct; `new`; `acquire` cap-check `:204` +
  increment `:210`; the three decrement edges `:140-142`, `:217-220`, `:296-299`;
  `for_bootstrap`)

- [ ] **Step 1: Write the failing test.** Add a test: pool `max_connections: 1` (default
  pending), a hold-capable in-test backend (a `tokio::net::TcpListener` accept loop that holds
  the conn). Acquire one guard (connects, `cx_open` → 1), assert a SECOND `acquire()` returns
  `PoolError::Overflow` with `upstream_cx_overflow == 1`; drop the first guard, sweep/return,
  assert `cx_open` returns to 0. Mirror the existing pool tests' in-test TCP backend shape
  (`pool.rs::tests` already spawns `TcpListener`s):

```rust
#[tokio::test]
async fn cx_overflow_increments_and_cx_open_tracks_cap_edges() {
    let (backend_addr, _srv) = spawn_holding_backend().await; // accepts + holds
    let (pool, _reg, handles) = mk_pool_cb("c", /*max_conn*/ 1); // returns cx_overflow + cx_open
    let g1 = pool.acquire(backend_addr, "c").await.expect("first acquires");
    assert_eq!(handles.cx_open.value(), 1, "at cap after first connect");
    let err = pool.acquire(backend_addr, "c").await.expect_err("second overflows");
    assert!(matches!(err, PoolError::Overflow { .. }));
    assert_eq!(handles.cx_overflow.value(), 1);
    drop(g1); // returns to idle; established unchanged (still 1) → cx_open stays 1 until eviction
    // Force eviction via the destroy path: invalidate a fresh reuse, or sweep.
    // (Implementer: assert cx_open returns to 0 at the decrement edge — use invalidate()
    //  on a reused guard, which runs the Drop destroy path decrementing established.)
}
```

  (Implementer tunes the drain assertion to whichever decrement edge is cleanest to drive in a
  unit test — the `invalidate()` destroy path is the most direct; the sweeper path is also
  acceptable.)

- [ ] **Step 2: Run to verify failure.** Run: `cargo test -p envoy-http1 cx_overflow`
  Expected: FAIL — `cx_overflow` / `cx_open` handles don't exist.

- [ ] **Step 3: Add the handles.** Add to `H1Pool`: `cx_overflow: Arc<envoy_stats::Counter>,`
  and `cx_open: Arc<envoy_stats::Gauge>,`. Thread both through `H1Pool::new`.

- [ ] **Step 4: Increment `cx_overflow` at the cap-check + set `cx_open` at the increment edge.**
  In `acquire()` (`pool.rs:201-211`):

```rust
        {
            let mut est = self.established.lock();
            let n = est.entry(endpoint).or_insert(0);
            if *n >= self.max_connections {
                self.cx_overflow.inc(); // 15 D4 (lock-in #5): cap-hit count
                return Err(PoolError::Overflow {
                    cluster: self.cluster_name.clone(),
                    max: self.max_connections,
                });
            }
            *n += 1;
            if *n >= self.max_connections {
                self.cx_open.set(1); // 15 D4 (lock-in #6): at-cap inclusive
            }
        }
```

- [ ] **Step 5: Clear `cx_open` at the three decrement edges.** At each `saturating_sub` site,
  while the `established` lock is held, add `if *n < self.max_connections { self.cx_open.set(0); }`:
  - `PoolGuard::Drop` destroy path (`pool.rs:139-142`)
  - connect-failure rollback (`acquire`, `pool.rs:217-220`)
  - `sweep_once` eviction (`pool.rs:296-299`)

- [ ] **Step 6: Register the handles in `for_bootstrap`** (gated on `circuit_breakers` per
  lock-in #4, mirroring the Task-2 conditional pattern):

```rust
            let cx_overflow = if cfg.circuit_breakers.is_some() {
                registry.register_counter(&format!("cluster.{}.upstream_cx_overflow", cfg.name))?
            } else { Arc::new(envoy_stats::Counter::new()) };
            let cx_open = if cfg.circuit_breakers.is_some() {
                registry.register_gauge(&format!(
                    "cluster.{}.circuit_breakers.default.cx_open", cfg.name))?
            } else { Arc::new(envoy_stats::Gauge::new()) };
```

  Pass both into `H1Pool::new`. (Same `Counter::new()`/`Gauge::new()` publicness caveat as
  Task 2 — prefer conditional registration; fall back to an `Option` handle if the constructors
  aren't `pub`.)

- [ ] **Step 7: Run to verify pass.** `cargo test -p envoy-http1 cx_overflow cx_open` then
  `cargo test -p envoy-http1`. `cargo build -p envoy-http1` standalone — `Finished`.

- [ ] **Step 8: fmt + commit.**

```bash
cargo fmt --all -- --check
git add crates/envoy-http1/src/pool.rs
git commit -m "phase 15 Task 3: H1 pool upstream_cx_overflow counter + circuit_breakers.default.cx_open gauge [ADR-0043]"
```

---

## Task 4: H1 `synth_overflow()` helper + H1 router overflow/pending arms + BEHAVIOR_CONTRACT row

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (add `synth_overflow` near `synth_no_healthy_upstream`
  `:1068`; the overflow arm `:542-561`)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (overflow-503 row under the response-body/flag
  section)

- [ ] **Step 1: Write the failing test.** In `hcm.rs::tests`, add (mirror
  `synth_no_healthy_upstream_emits_19_byte_body_and_5_headers` at `:3254`):

```rust
#[test]
fn synth_overflow_emits_81_byte_body_and_x_envoy_overloaded() {
    let r = synth_overflow(true);
    assert_eq!(r.status, 503);
    assert_eq!(
        r.body.as_ref(),
        b"upstream connect error or disconnect/reset before headers. reset reason: overflow"
    );
    assert_eq!(r.body.len(), 81);
    let names: Vec<&str> = r.headers.iter().map(|(k, _)| k.as_str()).collect();
    assert!(names.iter().any(|n| n.eq_ignore_ascii_case("x-envoy-overloaded")));
    assert!(r.headers.iter().any(|(k, v)|
        k.eq_ignore_ascii_case("x-envoy-overloaded") && v == "true"));
    assert!(names.iter().any(|n| n.eq_ignore_ascii_case("content-length")));
}
```

- [ ] **Step 2: Run to verify failure.** `cargo test -p envoy-http1 synth_overflow` — FAIL
  (`synth_overflow` undefined).

- [ ] **Step 3: Add `synth_overflow`.** After `synth_no_healthy_upstream` (`hcm.rs:1088`):

```rust
/// 15 D5 (ADR-0043 §6.2 finding 3): the `max_connections` / `max_pending_requests:0`
/// overflow synth-503. Body is the byte-exact 81-byte Envoy local-reply
/// `upstream connect error or disconnect/reset before headers. reset reason: overflow`
/// (no trailing newline), plus the `x-envoy-overloaded: true` header (the wire surfacing
/// of Envoy's access-log-only `UO` response flag). Used at BOTH the pool cap-overflow arm
/// and the pending-overflow arm (`hcm.rs:542`).
fn synth_overflow(close: bool) -> Response {
    let body = Bytes::from_static(
        b"upstream connect error or disconnect/reset before headers. reset reason: overflow",
    );
    Response {
        status: 503,
        reason: None,
        headers: vec![
            (headers::SERVER.to_string(), DEFAULT_SERVER_NAME.to_string()),
            (headers::DATE.to_string(), now_imf_fixdate()),
            (headers::CONTENT_LENGTH.to_string(), body.len().to_string()),
            (headers::CONTENT_TYPE.to_string(), DEFAULT_CONTENT_TYPE.to_string()),
            (headers::CONNECTION.to_string(), connection_value(close).to_string()),
            ("x-envoy-overloaded".to_string(), "true".to_string()),
        ],
        body,
    }
}
```

- [ ] **Step 4: Wire both router arms.** In the H1 overflow handling (`hcm.rs:542-561`), change
  the `PoolError::Overflow` arm from `Err(synth_status(503, close))` to `Err(synth_overflow(close))`,
  and add a sibling `PoolError::PendingOverflow` arm also returning `Err(synth_overflow(close))`:

```rust
                                    Err(crate::pool::PoolError::Overflow { .. }) => {
                                        tracing::warn!(cluster = %cluster.name(), "pool overflow — 503");
                                        Err(synth_overflow(close))
                                    }
                                    Err(crate::pool::PoolError::PendingOverflow { .. }) => {
                                        tracing::warn!(cluster = %cluster.name(),
                                            "pending-request overflow (max_pending_requests:0) — 503");
                                        Err(synth_overflow(close))
                                    }
```

- [ ] **Step 5: Run to verify pass.** `cargo test -p envoy-http1 synth_overflow` then
  `cargo test -p envoy-http1` — PASS. (The existing overflow-arm unit tests, if any assert an
  empty body, are updated to the 81-byte body here.)

- [ ] **Step 6: Land the BEHAVIOR_CONTRACT overflow-503 row.** In
  `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, under the response-body / response-flag section, add a
  row: the `max_connections`/`max_pending_requests:0` overflow rejection emits a 503 with the
  byte-exact 81-byte `…reset reason: overflow` body + `x-envoy-overloaded: true`; equivalence =
  byte-exact body + status; note the `UO` flag is access-log-only (wire = `x-envoy-overloaded`)
  and that envoy-rust additionally emits a `connection` header (allow-listed; Envoy omits it).

- [ ] **Step 7: fmt + commit.**

```bash
cargo fmt --all -- --check
git add crates/envoy-http1/src/hcm.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 15 Task 4: H1 synth_overflow 81-byte body + x-envoy-overloaded + overflow/pending arms + BEHAVIOR_CONTRACT [ADR-0043]"
```

---

## Task 5: H2 pool + HCM — mirror Tasks 2–4 (incl. 502→503 overflow correction)

**Files:**
- Modify: `crates/envoy-http2/src/pool.rs` (`PoolError`; `H2Pool` struct; `new`; `acquire`
  pending-gate + cap-check `:304-314`; decrement edges; `for_bootstrap` `:507-516`)
- Modify: `crates/envoy-http2/src/hcm.rs` (`synth_h2_overflow` near `synth_h2_502` `:672`; the
  overflow arm `:368-380`; add a `PendingOverflow` arm; route both to 503 via early-return)

- [ ] **Step 1: Write the failing tests.** Mirror Tasks 2+3+4 for H2: (a) `acquire` with
  `max_pending_requests:0` → `PoolError::PendingOverflow` + `upstream_rq_pending_overflow:1`
  without connecting; (b) `max_connections:1` cap-hit → `Overflow` + `upstream_cx_overflow:1` +
  `cx_open` edges; (c) `synth_h2_overflow()` emits a 503 + the 81-byte body + `x-envoy-overloaded`.
  (Note `envoy-http2`'s `PoolError` is its own type — `crates/envoy-http2/src/pool.rs` — add the
  `PendingOverflow` variant there.)

- [ ] **Step 2: Run to verify failure.** `cargo test -p envoy-http2 overflow` — FAIL.

- [ ] **Step 3: Pool changes (mirror Tasks 2+3).** Add `PoolError::PendingOverflow { cluster:
  String }`; add `max_pending_requests`, `rq_pending_overflow`, `cx_overflow`, `cx_open` fields
  + `DEFAULT_MAX_PENDING_REQUESTS = 1024`; insert the pending-gate at the start of the
  connect-on-miss block (before the cap-check at `pool.rs:304-314`); `cx_overflow.inc()` at the
  cap-check; `cx_open.set(1)` after `*n += 1` reaches the cap; `cx_open.set(0)` at the H2
  decrement edges (connect-failure rollback `:320-323` + the H2 pool's conn-eviction site).
  Register all three stats in `H2PoolManager::for_bootstrap` (`:507-516`) gated on
  `circuit_breakers.is_some()`, sourcing `max_pending_requests` from the config.

- [ ] **Step 4: HCM `synth_h2_overflow` + 502→503 correction (lock-in #10, C-1).** Add
  `synth_h2_overflow()` (the H2 sibling of `synth_overflow` — same 81-byte body + status 503 +
  `x-envoy-overloaded: true`, WITHOUT the H1 `connection` header per the H2 synth convention,
  mirror `synth_h2_502` at `:672`). In the H2 dispatch (`hcm.rs:368-380`), change the
  `PoolError::Overflow` arm from `Err(format!(...))` (→ which funnels to `synth_h2_502`) to an
  EARLY return of `synth_h2_overflow()` via `finalize_h2_stream` (mirror the `synth_h2_502`
  early-return at `:399-421`), and add a sibling `PoolError::PendingOverflow` arm doing the same.
  This corrects the overflow status from 502 → 503.

- [ ] **Step 5: Run to verify pass.** `cargo test -p envoy-http2` — PASS.
  `cargo build -p envoy-http2` standalone — `Finished`.

- [ ] **Step 6: fmt + commit.**

```bash
cargo fmt --all -- --check
git add crates/envoy-http2/src/pool.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 15 Task 5: H2 pool pending/cx overflow stats + cx_open + synth_h2_overflow 503 (corrects 502) [ADR-0043]"
```

---

## Task 6: Bilateral fixture `0023` + Docker-gated wrapper

**Files:**
- Create: `tests/fixtures/0023-upstream-circuit-breaker-max-pending-requests/envoy.yaml`
- Create: `tests/fixtures/0023-upstream-circuit-breaker-max-pending-requests/envoy-rust.yaml`
- Create: `tests/fixtures/0023-upstream-circuit-breaker-max-pending-requests/expectations.yaml`
- Create: `tests/differential/tests/upstream_circuit_breaker.rs` (Docker-gated wrapper)

- [ ] **Step 1: Author the two configs.** Copy fixture 0020's `envoy.yaml` / `envoy-rust.yaml`
  topology (STRICT_DNS single endpoint, H1 HCM listener routing `/` → `backend`, admin block),
  changing the cluster's `circuit_breakers` to `thresholds: [{ priority: DEFAULT,
  max_connections: 1, max_pending_requests: 0 }]`. The backend is the existing
  `health-aware-http1-backend` (a fast 200 backend — it is NEVER contacted, since
  `max_pending_requests:0` rejects before connecting; any reachable backend address works).

- [ ] **Step 2: Author `expectations.yaml`.** Drive a SINGLE GET via the EXISTING
  `Driver::Http1KeepAlive` (lock-in #11), asserting the bilateral 503 + 81-byte body +
  `x-envoy-overloaded` presence + the post-settle stats:

```yaml
# Phase 15 fixture-0023: max_pending_requests:0 reject path (ADR-0043). A single GET against
# a max_pending_requests:0 cluster is rejected by BOTH proxies with a 503 + the 81-byte
# "...reset reason: overflow" body + x-envoy-overloaded:true; the backend is never contacted
# (upstream_cx_total:0). upstream_cx_overflow + cx_open stay inert-0 (no connection demand
# reaches the cap). Timing-robust: no concurrency, no slow backend.
driver:
  kind: http1_keep_alive
  requests:
    - method: GET
      path: /
      host: backend
      expected_status: 503
      expected_body:
        kind: exact   # implementer: match the 14.2 Http1KeepAlive body-rule shape
        value: "upstream connect error or disconnect/reset before headers. reset reason: overflow"
      require_header_present: ["x-envoy-overloaded"]
  settle_ms: 200
  expected_stats:
    - { name: cluster.backend.upstream_rq_pending_overflow,        value: 1 }
    - { name: cluster.backend.upstream_cx_overflow,                value: 0 }
    - { name: cluster.backend.upstream_cx_total,                   value: 0 }
    - { name: cluster.backend.circuit_breakers.default.cx_open,    value: 0 }
```

  (Implementer: align the `expected_body` / `require_header_present` field names with the actual
  14.2 `Http1KeepAliveRequest` shape — read `tests/differential/src/lib.rs:314-339` for the
  landed field names; the SPEC-correction B-3 from 14.2 added them.)

- [ ] **Step 3: Author the Docker-gated wrapper.** Mirror
  `tests/differential/tests/upstream_connection_pooling_and_per_class_counters.rs` verbatim
  (the `#[test]` gated on the Docker-available env + `run_fixture("0023-…")` shape).

- [ ] **Step 4: Run the fixture locally (Docker).** Run:
  `cargo test -p differential --test upstream_circuit_breaker -- --nocapture`
  Expected: PASS — both proxies 503 the GET with the 81-byte body + `x-envoy-overloaded` +
  `upstream_rq_pending_overflow:1`. (If RED on a header-set mismatch — e.g. the `connection`
  header — confirm the harness allow-lists it as the 0019/0022 synth-503 precedent does; if RED
  on the body, re-capture the exact bytes via `superpowers:systematic-debugging`. **Fallback if
  the pending-reject does not bilaterally match:** demote 0023 to the inert-0 form asserting only
  the two new stat names at 0 under a NON-overflow keep-alive workload, per ADR-0043 option (b);
  document in PROGRESS.)

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all -- --check
git add tests/fixtures/0023-upstream-circuit-breaker-max-pending-requests/ tests/differential/tests/upstream_circuit_breaker.rs
git commit -m "phase 15 Task 6: fixture 0023 max_pending_requests:0 bilateral overflow-503 + Docker wrapper [ADR-0043]"
```

---

## Task 7: Fixture 0020 inert-0 observability assertions

**Files:**
- Modify: `tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/expectations.yaml`

- [ ] **Step 1: Extend `expected_stats`.** Fixture 0020's cluster configures `circuit_breakers`
  (`max_connections: 4`) but its sequential keep-alive workload never trips the cap. Add two
  bilateral inert-0 assertions to its `expected_stats` (proving envoy-rust emits the
  Envoy-matching stat NAMES at 0):

```yaml
    - { name: cluster.backend_cluster.upstream_cx_overflow,             value: 0 }
    - { name: cluster.backend_cluster.circuit_breakers.default.cx_open, value: 0 }
```

- [ ] **Step 2: Run the fixture (Docker).** Run:
  `cargo test -p differential --test upstream_connection_pooling_and_per_class_counters`
  Expected: PASS — both proxies emit the two new stats at 0. (If envoy-rust does not emit
  `circuit_breakers.default.cx_open` at 0, confirm Task 3's gauge registers for the
  circuit-breaker-configured cluster.)

- [ ] **Step 3: Commit.**

```bash
git add tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/expectations.yaml
git commit -m "phase 15 Task 7: fixture 0020 inert-0 upstream_cx_overflow + cx_open bilateral assertions [ADR-0043]"
```

---

## Task 8: In-process backstop — both overflow paths + non-zero cx_overflow/cx_open

**Files:**
- Create: `crates/envoy-bin/tests/upstream_circuit_breaker.rs`

- [ ] **Step 1: Write the backstop.** Mirror `crates/envoy-bin/tests/upstream_connection_pooling.rs`
  (the boot-envoy-bin + in-process-backend shape). Two `#[tokio::test]`s:

  **(a) pending-overflow path** — boot envoy-bin with a `max_connections:1` +
  `max_pending_requests:0` cluster + a trivial in-process H1 backend; drive ONE GET; assert: 503
  status, body == the 81-byte `…reset reason: overflow`, `x-envoy-overloaded: true` present + the
  5 standard headers present; then scrape admin `/stats` and assert
  `cluster.<name>.upstream_rq_pending_overflow == 1`, `upstream_cx_overflow == 0`,
  `upstream_cx_total == 0`, `circuit_breakers.default.cx_open == 0`.

  **(b) cx-overflow path** — boot envoy-bin with a `max_connections:1` cluster (default pending)
  + an in-process HOLD-capable H1 backend (an in-test `tokio::net::TcpListener` accept loop that
  sleeps ~500ms before responding 200). Drive K=2 concurrent GETs via `tokio::join!`; assert the
  status MULTISET is `{200, 503}` (acquisition order is non-deterministic — assert the multiset,
  not a positional sequence); the 503's body == the 81-byte overflow body + `x-envoy-overloaded`;
  then scrape `/stats` and assert `upstream_cx_overflow == 1`. For the `cx_open` BOTH-edges
  observation (the 14.2 both-directions discipline), either (i) scrape `/stats` mid-flight (while
  one request holds the connection) asserting `circuit_breakers.default.cx_open == 1`, then
  post-settle asserting `== 0`; or (ii) if mid-flight scraping is timing-fragile, assert only the
  terminal `cx_open == 0` and rely on the Task-3 pool unit test for the rising edge. (Implementer
  picks; document the choice in PROGRESS.)

- [ ] **Step 2: Run.** `cargo test -p envoy-bin --test upstream_circuit_breaker -- --nocapture`
  Expected: PASS (both tests). (This backstop is the non-Docker validation of the non-zero
  `cx_overflow`/`cx_open` path — Envoy serves `{200,200}` there, so it is envoy-rust-internal.)

- [ ] **Step 3: Commit.**

```bash
cargo fmt --all -- --check
git add crates/envoy-bin/tests/upstream_circuit_breaker.rs
git commit -m "phase 15 Task 8: in-process backstop — pending-overflow + cx-overflow (cx_open both edges) [ADR-0043]"
```

---

## Task 9: Fuzz seed extension + BEHAVIOR_CONTRACT stat rows

**Files:**
- Modify: `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml`
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (Stat-name mapping section)

- [ ] **Step 1: Extend the fuzz seed in place (C-5).** Add `max_pending_requests: 0` to the
  existing seed's threshold entry:

```yaml
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 4
            max_pending_requests: 0
```

  No `.gitignore` edit (already allow-listed line 27); the seed stays in the
  `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array (it parses+validates cleanly —
  `max_pending_requests:0` is accepted).

- [ ] **Step 2: Verify the seed parses.** Run: `cargo test -p envoy-config fuzz_corpus_seeds`
  Expected: PASS (the seed is in the SUCCESS set + parses clean).

- [ ] **Step 3: Land the three Stat-name mapping rows.** In `docs/envoy-rust/BEHAVIOR_CONTRACT.md`
  under `## Stat-name mapping`, add:
  - `cluster.<name>.upstream_cx_overflow` — Counter; name-required + value-exact-at-0 bilaterally
    (fixtures 0020/0023 — no cap-hit). NON-zero cross-proxy value DIVERGES on the overflow path
    (Envoy queues the cap-overflow → may serve; envoy-rust 503s immediately) until the
    `max_pending_requests>0` queue lands — validated in-process only. Increment site: the pool
    cap-check branch.
  - `cluster.<name>.circuit_breakers.default.cx_open` — Gauge (0/1); name-required + value-exact-
    at-0 bilaterally; at-cap inclusive (`1` when `upstream_cx_active == max_connections`).
    Envoy-only siblings (`default.{cx_pool_open, rq_open, rq_pending_open, rq_retry_open}` +
    the entire `circuit_breakers.high.*` set) are deferred (envoy-rust does not enforce those
    breakers/priorities). Per-endpoint-vs-per-cluster reconciliation deferred (single-endpoint
    fixtures coincide).
  - `cluster.<name>.upstream_rq_pending_overflow` — Counter; **value-exact bilaterally** (the
    `max_pending_requests:0` reject count — fixture 0023 asserts 1). The DEFERRED siblings
    `upstream_rq_pending_active`/`upstream_rq_pending_total` + the `rq_pending_open` gauge belong
    to the deferred `max_pending_requests>0` queue.
  - A **divergence note**: under DEFAULT `max_pending_requests`, Envoy queues a `max_connections`
    overflow (pending) and serves it; envoy-rust has no pending queue and 503s the overflow
    immediately. The two proxies are NOT bilaterally equivalent on the at-cap overflow STATUS
    until the pending-request queue lands (deferred). `max_pending_requests:0` IS bilaterally
    equivalent (both reject-all-establish).

- [ ] **Step 4: Commit.**

```bash
cargo fmt --all -- --check
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 15 Task 9: fuzz seed max_pending_requests:0 + BEHAVIOR_CONTRACT cx_overflow/cx_open/rq_pending_overflow rows [ADR-0043]"
```

---

## Task 10: State-4 phase-done verification + STATE advance

**Files:**
- Modify: `docs/envoy-rust/phases/15-circuit-breakers/PROGRESS.md` (state-4 evidence subsection)
- Modify: `docs/envoy-rust/STATE.md` (advance to state-4-complete / state-5-next)

- [ ] **Step 1: Run the full §7.5 (a)–(e) gate set + isolated-crate builds (lock-in #14).** Run
  and quote each into PROGRESS:
  - `cargo build --workspace --all-targets`
  - `cargo build -p envoy-config` / `cargo build -p envoy-http1` / `cargo build -p envoy-http2`
    (STANDALONE — the `project_isolated_crate_build_blindspot` discipline)
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo fmt --all -- --check`
  - `cargo test --workspace`
  - `cargo deny check`
  - `cargo fuzz run parse_bootstrap` (short CI budget) on the extended corpus
  - The Docker-gated differential suite: fixture 0023 GREEN + all 22 prior fixtures (0001–0022)
    GREEN simultaneously (acceptance gates (a)+(b)); h2spec ≥95% (gate (c), vacuous — no H2
    framing touched).
- [ ] **Step 2: Quote the CI evidence** (05.3→14.2 discipline): real CI run URL + HEAD SHA +
  completion timestamp + per-gate quoted output. Re-run the known `0012` flake
  (`project_flaky_access_log_fixture_0012`) if it surfaces.
- [ ] **Step 3: Advance STATE.md** to `15` state-4-complete / state-5-next (next-skill
  `superpowers:requesting-code-review`). Demote the prior pointer to `_Historical_`; append a
  `### Phase-15 state-4 verification` Notes subsection.
- [ ] **Step 4: Commit.**

```bash
git add docs/envoy-rust/phases/15-circuit-breakers/PROGRESS.md docs/envoy-rust/STATE.md
git commit -m "phase 15 Task 10: state-4 phase-done verification + STATE advance [ADR-0043]"
```

---

## Task 11: State-5 code review → State-6 close-out (later sessions)

> Tasks 11+ are NOT part of this PLAN-write session. Recorded for the executing arc.

- **State 5** (`superpowers:requesting-code-review`): review the Task 1–10 commit range; write
  `REVIEW.md`. Named review focus: the `cx_open` edge-correctness (all four `established` edges
  set/clear the gauge; terminal-0); the `max_pending_requests:0` reject ordering (pending BEFORE
  cap so `cx_overflow` stays 0); the H2 502→503 correction parity with H1; the 81-byte body
  byte-exactness; the inert-when-unconfigured registration (the 22 fixtures unaffected). If
  Critical/Important → re-enter state 3 per §5.2.
- **State 6** (deterministic close-out): commit + flip ROADMAP row `15` `in-progress → done` +
  advance STATE.md to "awaiting next planning" + append the `### Phase-15 rollovers` Notes
  subsection. The close-out is a non-split top-level phase (row `15` has no sub-phases) — flips
  its own row alone. Commit title: `phase 15: circuit breakers — observability + max_pending_requests:0
  reject [ADR-0043]` (the §5.3 final-phase format).
