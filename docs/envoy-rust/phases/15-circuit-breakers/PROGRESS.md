# Phase 15 (`15-circuit-breakers`) — PROGRESS

> Running execution log. The state-2 PLAN-write commit lands this skeleton + the Task 1
> preamble (the 04.3 → 14.2 standalone-PLAN-write cadence). The state-3 subagent-driven
> execution arc appends one subsection per task (Tasks 1–10); Task 11 is the state-5 review +
> state-6 close-out (later sessions).

---

## Task 1 preamble — PLAN-time SPEC corrections, §6.2 lock-ins, carryforward dispositions

This preamble records the facts the phase-15 state-2 PLAN-write established by reading the
ratified `SPEC.md` against the PLAN-time HEAD `35236eec3` (the phase-15 state-1 brainstorm) +
the HEAVY §6.2 Docker empirical verification. It is the authoritative cold-start context for the
state-3 implementer. **The SPEC is ratified and NOT edited; the §6.2 findings materially
re-scoped the phase — read ADR-0043 + PLAN §0.B/§0.C BEFORE implementing.**

### PLAN-write disposition

- **State 2 → state 3.** `PLAN.md` + this `PROGRESS.md` skeleton + this Task 1 preamble land in
  one standalone pre-Task-1 commit (mirrors 14.2 `9a56e85` / 13.2 `8c7d8a23`). The commit ALSO
  lands **ADR-0043** (the §6.2 re-scope — a cadence departure from the docs-only 14.x PLAN-writes,
  justified because §6.2 falsified the SPEC's central premise) + the ROADMAP row-15
  `planned → in-progress` flip + summary update + the STATE advance. Commit title carries
  **`[ADR-0043]`**.
- **§6.1 split-gate verdict: NO SPLIT.** The re-scoped PLAN is **11 tasks / ~1100–1250 LoC**
  (production ~430, tests/backstop ~520, fixture/docs ~300) — under the §6.1 ~1500-LoC /
  ~25-task gate. The reserved **ADR-0044 (split) does NOT fire.**
- **ADR posture: ADR-0043 lands at THIS commit** (DECISIONS.md ledger head `ADR-0042` → `ADR-0043`,
  count 44; next available `ADR-0044`). No further ADR expected in the 15 lifecycle.
- **Family/execution posture:** subagent-driven at state 3 (`feedback_execution_style`);
  implementers dispatched SERIALLY (`feedback_serial_subagent_dispatch`); recommendation picked
  at every borderline call (`feedback_pick_recommendation`, no fork — including the ADR-0043
  re-scope itself).

### The §6.2 finding that re-scoped the phase (ADR-0043; PLAN §0.B C-0 / §0.C)

The SPEC §1/§2.1/§2.3 premise — `max_connections:1` + `max_pending_requests:0` + K=2 concurrent
→ `{200, 503}` + `cluster.<name>.upstream_cx_overflow:1` — is **empirically FALSE** at
`envoyproxy/envoy:v1.33.0`:

1. **`max_pending_requests:0` rejects EVERY request** (K=1, K=2, sequential) with a 503; the
   pool never warms; the backend is never contacted; `upstream_cx_total:0`, `upstream_cx_overflow:0`.
   The live counter is **`upstream_rq_pending_overflow`**.
2. The overflow-503 body is **NON-EMPTY**: the 81-byte `upstream connect error or
   disconnect/reset before headers. reset reason: overflow` (no trailing newline) + the
   `x-envoy-overloaded: true` header (the wire surfacing of the access-log-only `UO` flag).
3. The bilateral `{200,503}` overflow fixture is **not achievable** without the deferred
   pending-request queue (Envoy queues a cap-overflow; envoy-rust 503s immediately).

**Re-scope (option c):** implement `max_pending_requests:0` FAITHFULLY (reject-on-establish →
`upstream_rq_pending_overflow` + the 81-byte 503) and build the bilateral fixture around a
SINGLE GET against a `max_pending_requests:0` cluster (both proxies 503; timing-robust, no
concurrency, no slow backend). RETAIN the `upstream_cx_overflow` + `circuit_breakers.default.cx_open`
observability (validated inert-0 bilaterally on fixtures 0020/0023; non-zero in-process only).
DROP the SPEC's `Driver::Http1Concurrent` + `--hold-ms` knob. See PLAN §0.A/§0.B/§0.C.

### PLAN-time SPEC corrections (PLAN §0.B; the 06.2 → 14.2 cadence)

- **C-0** — the load-bearing ADR-0043 re-scope (above).
- **C-1** — H2 overflow currently emits **502** (`synth_h2_502`, `http2/src/hcm.rs:368-380`),
  contradicting ADR-0042 §0's "both arms already 503". Task 5 corrects to 503. Pre-existing,
  differential-inert (no H2 overflow fixture).
- **C-2** — the existing `cluster_circuit_breakers_rejects_phase13_deferred_threshold_fields`
  test (`bootstrap.rs:9058`) uses `max_pending_requests: 5` to prove `deny_unknown_fields`;
  Task 1 re-points it to `max_requests: 5` (still deferred) + adds the new validator test.
- **C-3** — fixture renamed `0023-upstream-circuit-breaker-max-connections` →
  `0023-upstream-circuit-breaker-max-pending-requests`.
- **C-4** — `Driver::Http1Concurrent` (D6) + `--hold-ms` backend knob DROPPED (the fixture uses
  the existing `Driver::Http1KeepAlive` single-GET shape; the backstop uses `tokio::join!`).
- **C-5** — fuzz seed `cluster_circuit_breakers.yaml` extended IN PLACE with
  `max_pending_requests: 0` (already `.gitignore`-allow-listed; corpus stays 22; stays in the
  SUCCESS array — it parses clean).

### §6.2 empirical lock-ins (PLAN §0.C; locked — do NOT re-verify)

The 7-item findings: (1) `max_pending_requests:0` rejects all establish → `rq_pending_overflow`;
(2) `cx_overflow` is a cap-hit counter, name+semantics matched, value-bilateral only at-0;
(3) overflow-503 = 81-byte body + `x-envoy-overloaded:true`, `UO` access-log-only; (4) `cx_open`
at-cap inclusive (`1` when `cx_active == max_connections`); (5) `max_connections` per-cluster/
per-priority (single-endpoint fixture coincides); (6) full `circuit_breakers.*` = 10 always-emitted
gauges (envoy-rust emits only `default.cx_open`); (7) overflow-rejected request does not
increment `upstream_cx_total`.

### Carryforward dispositions (PLAN §0.D; none gate phase 15)

- Per-endpoint-vs-per-cluster `cx_open` reconciliation → future multi-endpoint phase
  (single-endpoint fixtures sidestep it).
- The `max_pending_requests > 0` pending-request QUEUE + the `{200,200}` bilateral overflow
  fixture → the deferred pending-queue phase.
- The standing multi-phase Minor inventory + the `upstream_cx_total` TCP carve-out + **ADR-0028**
  carry forward unchanged; phase 15 closes none.

### Task ledger (Tasks 1–10 land at state 3; one commit per task)

1. envoy-config schema + validator (`max_pending_requests`, accept 0 / reject >0; C-2 fix).
2. H1 pool `max_pending_requests:0` reject gate + `upstream_rq_pending_overflow`.
3. H1 pool `upstream_cx_overflow` counter + `circuit_breakers.default.cx_open` gauge.
4. H1 `synth_overflow()` 81-byte body + `x-envoy-overloaded` + router arms + BEHAVIOR_CONTRACT row.
5. H2 pool + HCM mirror (incl. the 502→503 overflow correction).
6. Fixture 0023 (single-GET `max_pending_requests:0` bilateral 503) + Docker wrapper.
7. Fixture 0020 inert-0 `upstream_cx_overflow` + `cx_open` assertions.
8. In-process backstop (pending-overflow path + cx-overflow path / `cx_open` both edges).
9. Fuzz seed + BEHAVIOR_CONTRACT stat rows + the overflow-model divergence note.
10. State-4 verification (incl. isolated-crate builds) + STATE advance.
11. (later) state-5 review → state-6 close-out.

---

## Task 1 — `envoy-config` schema + validator for `max_pending_requests` (commit `0c46b7bc1`)

**Landed.** `Thresholds` (`crates/envoy-config/src/bootstrap.rs`) gains `max_pending_requests: Option<u32>`
(`#[serde(default, skip_serializing_if = "Option::is_none")]`) + the `15 D1` doc note;
`ConfigError::UnsupportedNonZeroMaxPendingRequests { cluster, value }` added in
`crates/envoy-config/src/lib.rs` near the other circuit-breaker variants;
`validate_circuit_breakers` rejects `max_pending_requests > 0` (after the `max_connections` check,
using the surrounding let-chain idiom) while accepting `0`/absent. **C-2 fix:** the existing
`cluster_circuit_breakers_rejects_phase13_deferred_threshold_fields` test re-pointed from
`max_pending_requests: 5` → `max_requests: 5` (still proves `deny_unknown_fields` rejects a
still-deferred field). Two new validator tests added (`cluster_max_pending_requests_zero_accepted`,
`cluster_max_pending_requests_positive_rejected_by_validator`).

**TDD:** tests written first, confirmed failing (`no field max_pending_requests` /
`no variant UnsupportedNonZeroMaxPendingRequests`), then implemented to green.

**Verification (quoted):**
- `cargo test -p envoy-config` → `test result: ok. 287 passed; 0 failed; 0 ignored` (circuit_breaker
  filter: 7 passed).
- `cargo build -p envoy-config` (standalone, lock-in #14) → `Finished` (RC 0).
- `cargo fmt --all -- --check` → clean (RC 0).
- `git show --stat HEAD` → `2 files changed` (`bootstrap.rs` +89/-…, `lib.rs` +7) — ONLY the two
  scoped source files.

---

## Task 2 — H1 pool `max_pending_requests:0` reject gate + `upstream_rq_pending_overflow` (commit `1e37cf4bc`)

**Landed.** `crates/envoy-http1/src/pool.rs`: new `PoolError::PendingOverflow { cluster: String }`;
`H1Pool` gains `max_pending_requests: u32` + `rq_pending_overflow: Option<Arc<envoy_stats::Counter>>`;
`const DEFAULT_MAX_PENDING_REQUESTS: u32 = 1024`; `acquire()` reject gate fires on the
connect-on-miss path BEFORE the cap-check (`if self.max_pending_requests == 0 { inc; return
PendingOverflow }`) per lock-in #7; `for_bootstrap` sources `max_pending_requests` from the config
and registers `cluster.<name>.upstream_rq_pending_overflow` ONLY when `circuit_breakers.is_some()`
(inert-when-unconfigured, lock-in #4). New test `acquire_rejects_with_pending_overflow_when_max_pending_requests_zero`
proves rejection without dialing the backend + counter == 1.

**Two forced deviations (both correct, documented for the state-5 reviewer):**
1. **`Option<Arc<Counter>>` fallback (NOT the throwaway `Counter::new()`).** `envoy_stats::Counter::new()`
   is `pub(crate)` (`crates/envoy-stats/src/counter.rs:19`), so the PLAN Step 6 primary
   (`Arc::new(Counter::new())` throwaway) does not compile from envoy-http1. Used the PLAN-documented
   `Option` fallback: configured clusters → `Some(register_counter(...))`; unconfigured → `None`.
   The gate is dead for unconfigured clusters anyway (default 1024). **Carries to Task 3** — the
   `cx_overflow` Counter + `cx_open` Gauge will need the same `Option` treatment (`Gauge::new()` is
   likewise expected `pub(crate)`).
2. **`hcm.rs` also touched (+14 lines).** Adding the `PendingOverflow` enum variant made the H1
   router `pool.acquire(...)` match non-exhaustive (hard compile break failing the standalone-build
   gate). Added a MINIMAL `PendingOverflow` arm returning `synth_status(503, close)` (same as the
   current `Overflow` arm). **This is a placeholder — Task 4 refines BOTH arms to the byte-exact
   81-byte `synth_overflow` body.** Adding an enum variant matched non-exhaustively in the same
   crate is incompatible with a strict "pool.rs only" constraint.

**Verification (quoted):**
- `cargo test -p envoy-http1` → `test result: ok. 85 passed; 0 failed; 0 ignored`.
- `cargo build -p envoy-http1` (standalone, lock-in #14) → `Finished`.
- `cargo fmt --all -- --check` → clean (RC 0).
- `git show --stat HEAD` → `2 files changed` (`pool.rs` +132, `hcm.rs` +14) — only envoy-http1.
