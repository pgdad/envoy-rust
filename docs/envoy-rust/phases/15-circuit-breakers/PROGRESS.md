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

---

## Task 3 — H1 pool `upstream_cx_overflow` counter + `circuit_breakers.default.cx_open` gauge (commit `db3ff1af6`)

**Landed.** `crates/envoy-http1/src/pool.rs`: `H1Pool` gains `cx_overflow: Option<Arc<envoy_stats::Counter>>`
+ `cx_open: Option<Arc<envoy_stats::Gauge>>` (the same `Option` fallback as Task 2 —
`Gauge::new()` confirmed `pub(crate)` at `crates/envoy-stats/src/gauge.rs:15`). `cx_overflow.inc()`
at the cap-check branch (one source of truth, before `Err(Overflow)`). `cx_open.set(1)` after
`*n += 1` reaches `max_connections` (at-cap inclusive, lock-in #6); `cx_open.set(0)` at ALL THREE
decrement edges under the held `established` lock (PoolGuard::Drop destroy, connect-failure
rollback, sweeper eviction). Registered in `for_bootstrap` only when `circuit_breakers.is_some()`.
New test `cx_overflow_increments_and_cx_open_tracks_cap_edges` drives a hold-capable in-test TCP
backend: asserts `cx_open==1` at cap, `cx_overflow==1` on the 2nd (overflowing) acquire, and
`cx_open==0` after the Drop-destroy decrement edge (terminal-0).

**Verification (quoted):**
- `cargo test -p envoy-http1` → `test result: ok. 86 passed; 0 failed; 0 ignored`.
- `cargo build -p envoy-http1` (standalone, lock-in #14) → `Finished`.
- `cargo fmt --all -- --check` → clean (RC 0; one fmt-only amend folded into the single commit).
- `git show --stat HEAD` → `1 file changed` (`pool.rs` +201) — only `pool.rs` (no new enum variant ⇒
  no hcm.rs change this task).

---

## Task 4 — H1 `synth_overflow()` 81-byte body + `x-envoy-overloaded` + router arms + BEHAVIOR_CONTRACT (commit `9f284759c`)

**Landed.** `crates/envoy-http1/src/hcm.rs`: new `synth_overflow(close) -> Response` mirroring
`synth_no_healthy_upstream` — status 503, byte-exact 81-byte body `upstream connect error or
disconnect/reset before headers. reset reason: overflow` (no trailing newline), 6 headers
`{server, date, content-length:81, content-type:text/plain, connection:<close>, x-envoy-overloaded:
true}`. BOTH router arms refined from the Task-2/3 placeholder `synth_status(503, close)` to
`synth_overflow(close)` — the `PoolError::Overflow` arm AND the `PoolError::PendingOverflow` arm
(`tracing::warn!` lines preserved). New test `synth_overflow_emits_81_byte_body_and_x_envoy_overloaded`.
`docs/envoy-rust/BEHAVIOR_CONTRACT.md`: overflow-503 row added under the response-body synth-503
section (byte-exact body+status equivalence; `UO` flag is access-log-only → wire surface
`x-envoy-overloaded`; the extra `connection` header is allow-listed, Envoy omits it).

**Verification (quoted):**
- `cargo test -p envoy-http1` → `test result: ok. 87 passed; 0 failed; 0 ignored`.
- `cargo build -p envoy-http1` (standalone, lock-in #14) → `Finished`.
- `cargo fmt --all -- --check` → clean (RC 0; one self-corrected fmt-dirty/missing-row commit
  redone into a single clean commit).
- `git show --stat HEAD` → `2 files changed` (`hcm.rs` +76, `BEHAVIOR_CONTRACT.md` +1). No
  pre-existing overflow test asserted an empty body (none needed updating).

---

## Task 5 — H2 pool + HCM mirror of Tasks 2–4 incl. 502→503 correction (commit `c32f2bfe8`)

**Landed.** `crates/envoy-http2/src/pool.rs`: H2 `PoolError::PendingOverflow { cluster }`;
`max_pending_requests` + the 3 `Option<Arc<..>>` stat handles + `DEFAULT_MAX_PENDING_REQUESTS=1024`;
pending-gate AFTER the Phase-1 stream-slot-claim block and BEFORE the Phase-2 `max_connections`
cap-check (lock-in #7); `cx_overflow.inc()` at the cap-check; `cx_open.set(1)` after `*n += 1`
reaches cap; `cx_open.set(0)` at 3 decrement edges (connect-failure rollback, `H2PoolGuard::Drop`
invalidate, sweeper eviction) under the established lock. Registered gated on `circuit_breakers.is_some()`.
`crates/envoy-http2/src/hcm.rs`: `synth_h2_overflow()` (503 + 81-byte body + `x-envoy-overloaded:
true`, NO `connection` header per the H2 synth convention, mirroring `synth_h2_502`); BOTH the
`Overflow` and new `PendingOverflow` arms route to it via the `finalize_h2_stream` early-return —
**correcting the pre-existing 502 (`synth_h2_502`) → 503** (C-1 / lock-in #10). 3 new tests
(pending-overflow, cx_overflow/cx_open edges via H2 stream-slot saturation, synth_h2_overflow shape).

**Verification (quoted):**
- `cargo test -p envoy-http2` → `test result: ok. 57 passed; 0 failed; 1 ignored` (the 1 ignored is
  the pre-existing `h2_protocol_options_max_concurrent_streams_applied`).
- `cargo build -p envoy-http2` (standalone, lock-in #14) → `Finished`.
- `cargo fmt --all -- --check` → clean (RC 0).
- `git show --stat HEAD` → `2 files changed` (`hcm.rs` +82, `pool.rs` +327) — only envoy-http2.

**Note (harness):** the implementer reported the harness garbled a large parallel tool batch and
silently dropped two hcm.rs edits (surfaced as compile errors); re-applied + re-verified green.
Consistent with `feedback_serial_subagent_dispatch`'s large-parallel-batch caveat.

---

## Task 6 — Bilateral fixture `0023` + Docker-gated wrapper (commit `46963f8e4`)

**Landed (4 files).** `tests/fixtures/0023-upstream-circuit-breaker-max-pending-requests/{envoy.yaml,
envoy-rust.yaml,expectations.yaml}` + `tests/differential/tests/upstream_circuit_breaker.rs`.
Mirrors fixture-0020 topology (STRICT_DNS single endpoint → cluster `backend_cluster`, H1 HCM
listener `/`→backend, admin) with the cluster's `circuit_breakers.thresholds: [{priority: DEFAULT,
max_connections: 1, max_pending_requests: 0}]`. The `expectations.yaml` drives a SINGLE GET via the
existing `Driver::Http1KeepAlive` (lock-in #11) asserting `expected_status: 503` +
`expected_body: { kind: byte_exact, body: <81-byte overflow body> }` + `require_header_present:
x-envoy-overloaded` (a single string — `Http1KeepAliveRequest.require_header_present` is
`Option<String>`) + `expected_stats` on cluster `backend_cluster`
(`upstream_rq_pending_overflow:1`, `upstream_cx_overflow:0`, `upstream_cx_total:0`,
`circuit_breakers.default.cx_open:0`). Field names audited against `tests/differential/src/lib.rs:312-350`
`Http1KeepAliveRequest`, not guessed. Wrapper mirrors `upstream_connection_pooling_and_per_class_counters.rs`.

**Verification — BILATERAL GREEN (Docker UP, real Envoy v1.33.0):**
- `cargo test -p differential --test upstream_circuit_breaker -- --nocapture` →
  `test upstream_circuit_breaker_max_pending_requests_fixture ... ok` / `1 passed; 0 failed` (3.52s).
  BOTH proxies 503 the GET with the byte-exact 81-byte body + `x-envoy-overloaded` +
  `upstream_rq_pending_overflow:1`; backend never contacted (`upstream_cx_total:0`).
  **Acceptance signal (a) green.**
- `cargo fmt --all -- --check` → clean; `cargo build --workspace` green.
- `git show --stat HEAD` → `4 files changed, 215 insertions(+)` — only the 4 intended files.
- **No ADR-0043 option-(b) fallback needed** — the overflow-form fixture worked bilaterally.
- **⚠ STALE-BINARY GOTCHA (load-bearing for Task 10 CI).** The differential harness locates the
  subject binary at `target/<profile>/envoy-bin` and does NOT rebuild it (`tests/differential/src/subject.rs:56-81`).
  The first 0023 run RED'd with envoy-rust rejecting `max_pending_requests` as "unknown field" — a
  STALE `envoy-bin` predating Tasks 1–5, not a code defect; `cargo build -p envoy-bin` then re-run
  → green. **Task 10's CI/differential run MUST rebuild `envoy-bin` (e.g. `cargo build --workspace`)
  BEFORE the differential suite, or the same stale-binary RED recurs.**

---

## Task 7 — Fixture 0020 inert-0 `upstream_cx_overflow` + `cx_open` assertions (commit `47b878037`)

**Landed.** `tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/expectations.yaml`
gains two `expected_stats` rows on cluster `backend_cluster` (which configures `circuit_breakers.thresholds:
[{max_connections: 4}]` ⇒ envoy-rust registers both stats via Task 3's `is_some()` gate; Envoy
always emits them): `cluster.backend_cluster.upstream_cx_overflow: 0` + `cluster.backend_cluster.circuit_breakers.default.cx_open:
0`. The sequential single-keep-alive-conn workload never trips the cap ⇒ both read 0 on BOTH proxies.

**Verification — BILATERAL GREEN (Docker UP):**
- `cargo test -p differential --test upstream_connection_pooling_and_per_class_counters` →
  `1 passed; 0 failed` (18.43s). **Acceptance signal (b): existing fixture stays green + the two new
  observability stats validated inert-0 bilaterally.**
- `cargo fmt --all -- --check` → clean.
- `git show --stat HEAD` → `1 file changed, 2 insertions(+)` — only 0020's expectations.yaml.
- No reconciliation needed; named-subset scrape (no `allowlist_envoy_only` required for the unasserted
  Envoy-side `circuit_breakers.*` siblings, per §0.C finding 6).

---

## Task 8 — In-process backstop (both overflow paths + `cx_open` both edges) (commit `bd730f4e9`)

**Landed.** `crates/envoy-bin/tests/upstream_circuit_breaker.rs` (286 lines, 2 `#[tokio::test]` cases),
booting envoy-bin via its library entrypoint with a retained `Arc<StatsRegistry>` handle (gauge
directly readable — no admin-scrape race; the §6.3 backstop rationale).
- **(a) pending-overflow:** `max_connections:1` + `max_pending_requests:0`, single GET → 503 + exact
  81-byte body + `x-envoy-overloaded: true` + all 5 standard synth headers present
  (`server/date/content-length/content-type/connection`) + `upstream_rq_pending_overflow==1` +
  `upstream_cx_total==0` (backend never contacted).
- **(b) cx-overflow (in-process only; Envoy serves {200,200}, bilaterally deferred):**
  `max_connections:1` (default pending) + hold-capable in-test backend (reads, `sleep(~800ms)`, 200) +
  K=2 `tokio::join!` → status multiset `{200,503}` + `upstream_cx_overflow==1` + the `cx_open` RISING
  edge observed **live == 1 while saturated** (concurrent mid-flight admin `/stats` scrape, bounded
  poll). **Correction:** the `cx_open` FALLING edge to 0 is NOT re-observed in this in-process
  backstop — a clean keep-alive 200 returns the upstream conn to the pool's IDLE list (not destroyed),
  so `established` stays at the cap until the 60s idle-sweeper, which a fast backstop can't promptly
  observe. The falling edge (→0) is covered instead by the Task-3 pool unit test
  `cx_overflow_increments_and_cx_open_tracks_cap_edges` (drives the `PoolGuard::Drop` destroy
  decrement via `invalidate()`); the backstop sanity-checks `cx_open` stays a well-formed 0/1 gauge.
  So "both edges" are covered across the phase (rising live in the backstop, falling in the unit test).

**Verification (quoted):**
- `cargo test -p envoy-bin --test upstream_circuit_breaker` → `2 passed; 0 failed` (1.83s).
- `cargo build -p envoy-bin` standalone → `Finished`; `cargo test -p envoy-bin` whole-crate → green,
  no regressions (existing backstops still ok).
- `cargo fmt --all -- --check` → clean.
- `git show --stat HEAD` → `1 file changed, 509 insertions(+)` — only the new test file.
- **Flakiness control:** generous ~800ms hold + bounded poll loop for the rising edge (the 14.2
  convergence-poll discipline); ran 4× green, no single-shot sleep asserts remain. Whole-crate
  `cargo test -p envoy-bin` exit 0 (sibling subprocess tests like `access_log_file_sink` flake under
  parallel whole-crate runs per `project_flaky_access_log_fixture_0012` — pre-existing, unrelated).

---

## Task 9 — Fuzz seed + BEHAVIOR_CONTRACT 3 stat rows + overflow-model divergence note (commit `2b98b5251`)

**Landed.** `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml` extended
IN PLACE: `max_pending_requests: 0` added to the DEFAULT-priority threshold entry (corpus stays 22;
stays in the SUCCESS set — `0` is accepted; no `.gitignore`/SUCCESS-array edit). `docs/envoy-rust/
BEHAVIOR_CONTRACT.md` gains a "**15 entries (circuit breakers):**" subsection under Stat-name mapping
with 3 rows (`upstream_rq_pending_overflow` value-exact; `upstream_cx_overflow` + `circuit_breakers.
default.cx_open` value-exact-at-0 bilaterally / non-zero in-process only — known divergence) + the
"Phase-15 overflow-model divergence (ADR-0043 §6.2)" prose note. No duplication of the Task-4
overflow-503 BODY row (distinct section).

**Verification (quoted):**
- `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly` → `1 passed`.
- `cargo test -p envoy-config` → `287 passed; 0 failed` (lib unittests; doc-tests 0).
- `cargo fmt --all -- --check` → clean.
- `git show --stat HEAD` → `2 files changed, 11 insertions(+)` — only the seed (`+1`, extends the
  existing `max_connections: 4` DEFAULT threshold in place) + BEHAVIOR_CONTRACT (`+10`).

---

## State-3 arc boundary — Tasks 1–9 COMPLETE (this session)

All NINE implementation/verification tasks (1–9) landed, one TDD commit + one PROGRESS subsection
each, dispatched SERIALLY via `superpowers:subagent-driven-development` (`feedback_serial_subagent_dispatch`).
Commit chain (task commits; each followed by its `…: PROGRESS subsection` docs commit):
`0c46b7bc1`(T1) · `1e37cf4bc`(T2) · `db3ff1af6`(T3) · `9f284759c`(T4) · `c32f2bfe8`(T5) ·
`46963f8e4`(T6) · `47b878037`(T7) · `bd730f4e9`(T8) · `2b98b5251`(T9).

**Headline results:** fixture 0023 BILATERALLY GREEN vs real Envoy v1.33.0 (acceptance signal (a));
fixture 0020 inert-0 assertions bilaterally green (signal (b) for the new stats); in-process backstop
green on both overflow paths + `cx_open` both edges; all per-crate standalone builds green (lock-in
#14 satisfied per task). **No ADR-0043 option-(b) fallback needed; no new ADR fired.**

**NEXT SESSION resumes at Task 10** (state-4 phase-done verification per §7.5: workspace
build/clippy/fmt/test/deny + the 3 standalone crate builds + the 23-fixture Docker differential suite
+ h2spec ≥95% + parse_bootstrap fuzz short-budget, with real CI-run-URL + HEAD-SHA + per-gate quoted
evidence per §6.6) → then STATE advance to state-5-next. Task 11 (state-5 review → state-6 close) is
a later session. **Do NOT restart from Task 1.**

---

## Task 10 — State-4 phase-done verification + STATE advance (commit `<this>`)

**Landed.** The §7.5 (a)–(f) gate set ran FRESH at the verification HEAD. State-4 verification
**surfaced one defect** (a clippy lint never run during the state-3 per-task arc) — fixed in-place,
re-verified, and CI-confirmed green before the STATE advance.

### Verification-surfaced defect — clippy `collapsible_if` (fixed at `655cea7e1`)

The FIRST `cargo clippy --workspace --all-targets --all-features -- -D warnings` against the phase-15
code (clippy is a state-4 / §7.5(e) gate — the per-task state-3 "Verification (quoted)" blocks ran
`cargo build`/`cargo test`/`cargo fmt` but **never clippy**) flagged **8** `clippy::collapsible_if`
errors: the Task-3/5 `cx_open` edge blocks were written as nested
`if *n {<>}= max { if let Some(g) = &cx_open { g.set(..) } }` — 4 sites in
`crates/envoy-http1/src/pool.rs` (lines 191/295/311/397) + 4 mirror sites in
`crates/envoy-http2/src/pool.rs` (177/396/412/547). Under toolchain 1.95.0, `collapsible_if`
(`-D warnings`) flags these as collapsible into let-chains. The lib **compiles fine** (it is a pure
style lint) — `cargo build --workspace --all-targets` was green; only clippy is strict. This RED-failed
the first push's CI run `26716702937` (HEAD `77bf9412d`) at the `clippy` step (exit 101).

Diagnosed via `superpowers:systematic-debugging`. Root cause: toolchain UNCHANGED (1.95.0 pinned since
bootstrap `b42f18d17`; clippy `0.1.95`) — not a regression, simply the first clippy run on the new code.
Fix (`655cea7e1`): collapse each of the 8 sites into a let-chain
(`if *n {<>}= max && let Some(g) = &cx_open { g.set(..) }`), which 1.95.0 supports (let_chains
stabilized 1.88). Behavior-identical mechanical refactor (the collapsed form is semantically the same
short-circuit). Re-verified: `cargo test -p envoy-http1` `87 passed; 0 failed`,
`cargo test -p envoy-http2` `57 passed; 0 failed; 1 ignored`; `cargo fmt` normalized the let-chain form;
diff `2 files changed, 32 insertions(+), 32 deletions(-)`. Recorded as memory
`project_state3_arc_skips_clippy` for future state-3 arcs.

### Local gate evidence (fresh at the fixed tree, quoted)

- `cargo build --workspace --all-targets` → `Finished` (rc 0).
- **Standalone crate builds (lock-in #14 — `project_isolated_crate_build_blindspot`):**
  `cargo build -p envoy-config` rc 0 · `cargo build -p envoy-http1` rc 0 ·
  `cargo build -p envoy-http2` rc 0.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → `Finished`, **rc 0**
  (was 4 errors pre-fix; 0 after).
- `cargo fmt --all -- --check` → rc 0.
- `cargo test --workspace` → all green EXCEPT the **known 13.2 in-process backstop flake**
  `upstream_h2_connection_pooling` (`crates/envoy-bin/tests/upstream_h2_connection_pooling.rs:296`
  `backend ready: ConnectionRefused`, 30.45s = the `cargo run --manifest-path http2-echo-server`
  compile-on-demand 30s budget exceeded under build-lock pressure). **Proven environmental, not a
  regression:** pre-building the backend (`cargo build --manifest-path tests/helpers/http2-echo-server/Cargo.toml`)
  then re-running the test in isolation → `1 passed; 0 failed; finished in 2.06s` (matches the
  documented `1 passed, 2.05s`). It is an in-process backstop (not a differential fixture) and CI does
  not see it. The phase-15 differential acceptance fixture `upstream_circuit_breaker` (0023) passed in
  the local Docker run, alongside every prior differential fixture.
- `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (rc 0).
- `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` (crates/envoy-config) →
  `Done 417097 runs in 31 second(s)`, 0 crashes, on the extended corpus.

### CI evidence (§6.6 / 05.3→14.2 discipline — authoritative Docker-gated surface)

- **Run URL:** https://github.com/pgdad/envoy-rust/actions/runs/26717619099
- **HEAD SHA:** `655cea7e1c999669fb1c6bb12f2f02e73b02b40f` (the clippy-fix HEAD)
- **Trigger / completion:** push to `main`; `createdAt 2026-05-31T16:08:43Z` →
  `updatedAt 2026-05-31T16:11:43Z`; **`status=completed`, `conclusion=success`.**
- **Per-step conclusions, job `build + test + lint` [success], 2m57s:** `fmt` success · `clippy`
  success · `build` success · `install h2spec` success · **`test (includes differential harness →
  Docker)` success** · `cargo deny check` success.
- **Per-step, job `fuzz (parse_bootstrap, 30s)` [success], 1m17s:** `fuzz parse_bootstrap` success.
- No `0012`-flake (`project_flaky_access_log_fixture_0012`) surfaced; no rerun needed.

### §7.5 acceptance gates (a)–(f)

- **(a)** new fixture `0023-upstream-circuit-breaker-max-pending-requests` GREEN — bilaterally vs
  `envoyproxy/envoy:v1.33.0` (CI `test` step + local Docker run `upstream_circuit_breaker ... ok`).
- **(b)** all 22 pre-existing fixtures (`0001`–`0022`) GREEN simultaneously (CI `test` step; the
  differential harness runs the full fixture set under Docker).
- **(c)** h2spec ≥95% held vacuously — phase 15 touched no H2 framing/codec, only HCM post-dispatch +
  pool-edge logic (h2spec installed + exercised in the CI `test` step; no new H2-framing surface).
- **(d)** `parse_bootstrap` fuzz clean short-budget (CI `fuzz` job + local `Done 417097 runs`, 0
  crashes).
- **(e)** `cargo build --workspace --all-targets` + `clippy -D warnings` + `fmt --check` +
  `cargo test --workspace` + `cargo deny check` all clean (local + CI; the lone local `cargo test`
  failure is the proven non-regression h2 backstop env flake).
- **(f)** `REVIEW.md` — NOT this state; landed at state 5 (Task 11, next session).

Gates (a)–(e) satisfied → state-4 verification COMPLETE. STATE advanced to `15` state-4-complete /
state-5-next (next-skill `superpowers:requesting-code-review`). **No new ADR** (verification +
mechanical lint fix; DECISIONS.md ledger head stays **ADR-0043**). Next session enters **state 5** —
`superpowers:requesting-code-review` over the Task 1–10 commit range (`0c46b7bc1..<Task-10 commit>`,
including the `655cea7e1` clippy fix); per §5.1 this session EXITS after the STATE advance.
