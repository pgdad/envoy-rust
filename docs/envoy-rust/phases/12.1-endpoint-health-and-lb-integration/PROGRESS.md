# Phase 12.1 (`12.1-endpoint-health-and-lb-integration`) — PROGRESS

> Per-task narrative log. Appended at every task commit per the 06.2 / 06.3 / 07.x / 08.x /
> 09 / 10 / 11 cadence. State-2 PLAN-write lands this skeleton + the Task 1 preamble; state-3
> dispatch appends `### Task N — <name>` subsections in execution order.

---

## State-2 commit context

This commit (the state-2 standalone PLAN-write commit) lands:

- **CREATE** `docs/envoy-rust/phases/12.1-endpoint-health-and-lb-integration/PLAN.md` (the
  state-2 PLAN.md per `BOOTSTRAP_PROMPT.md` §5 state 2; 7 tasks; 20 architecture lock-ins;
  full `- [ ]` checkbox TDD steps with complete code per task).
- **CREATE** `docs/envoy-rust/phases/12.1-endpoint-health-and-lb-integration/PROGRESS.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — flip row `12.1` `status: planned` →
  `status: in-progress`. No other row touched (parent row `12` stays `in-progress`; `12.2`
  stays `planned`).
- **MODIFY** `docs/envoy-rust/STATE.md` — Active phase status; Next expected skill; Last
  commit; Last updated; new `### Phase-12.1 state-2 PLAN-write` subsection in Notes.

**Predecessor commit:** `4f9ba04` — `phase 12: state-2 split decision + sub-phase SPECs
(12.1/12.2) [ADR-0036, ADR-0037]` (the parent-12 state-2 split commit; immediate prologue;
HEAD == origin/main at this PLAN-write's prologue; CI run `26290931448` settled `success`).

**SPEC commit base:** `4f9ba04`. **This state-2 commit makes NO inline SPEC.md edits** — the
§6.2 empirical verification was completed at the parent-12 split (`4f9ba04`); the 12.1
PLAN-writer baked the locked facts into the PLAN lock-ins without re-running Docker.

**ROADMAP status before this commit:** row `12.1` `planned` (added at the parent-12 split).
**ROADMAP status after this commit:** row `12.1` `in-progress`.

**STATE.md "Active phase" status before:** `phase 12.1 lifecycle state 2 (SPEC.md exists; PLAN.md does NOT)`.
**STATE.md "Active phase" status after:** `phase 12.1 lifecycle state 2-complete / state-3-next (PLAN.md + PROGRESS.md skeleton + Task 1 preamble landed; first task implementation pending)`.

**DECISIONS.md status before AND after:** **ADR-0037** (count 38). **No ADR lands at this
state-2 commit** (SPEC §7 + PLAN lock-in #2 — 12.1 introduces no new crate, no foundations
grant, no wire-level contract revision; the `EndpointHealth` `Relaxed`-ordering choice is
covered by the existing `cluster.rs` `pick()` precedent). Next available number stays
**ADR-0038**.

**BEHAVIOR_CONTRACT.md status before AND after:** Unchanged at this commit. The 1 new
`Stat-name mapping` row (`cluster.<name>.membership_healthy`, under a new `**12.1 entries
(active health checking):**` header) lands at Task 5 per the 06.x → 11 cadence (contract
extensions land at the task where the surface is first wired, NOT at PLAN-write time). The 3
`health_check.{attempt,success,failure}` counter rows defer to 12.2 (lock-in #4).

**ENVOY_TARGET.md + rust-toolchain.toml:** Unchanged (D-3.7 / D-3.9).

---

## PLAN scope summary

- **7 tasks** per PLAN §File-Structure + tasks 1-7. Subagent-driven execution at state 3 per
  PLAN lock-in #18 + `feedback_execution_style`.
- **~900-1000 LoC projected** (production ~430, tests ~470, doc/corpus ~80) — comfortably
  under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task gate. The parent-12 split
  (ADR-0036) already absorbed the over-gate scope into 12.1 + 12.2; 12.1 does NOT nest-split
  (lock-in #1).
- **ZERO ADR landings** (lock-in #2; SPEC §7).
- **NO new differential fixture** — regression-equivalence via the 18 existing Docker-gated
  fixtures (`0001`-`0018`) staying green simultaneously proves the machinery is inert when
  `health_checks` is unconfigured (the 05.1/07.1 foundation-slice pattern; §5.4).

---

## Task 1 preamble

### §6.2 empirical-verification findings — locked at the parent-12 split (`4f9ba04`); NOT re-run at this PLAN-write

Per the 12.1 SPEC §2 + §6 + STATE.md `### Phase-12 state-2 split decision` + ADR-0037, the
HEAVY 6-item §6.2 verification against `envoyproxy/envoy:v1.33.0` was performed at the
parent-12 state-2 split commit `4f9ba04` (a Docker bridge network with a synthetic
health-aware backend + an active-HC Envoy; admin `/stats` + data-plane probes). The findings
binding 12.1 are LOCKED FACTS (PLAN lock-in #3); the 12.1 PLAN-writer baked them into the
PLAN without re-running Docker:

1. **Initial endpoint health state = Unhealthy/pending-until-first-pass** (MATCHED projection)
   → D3 `EndpointHealth` initial state = Unhealthy (lock-in #12); the inert-and-unexercised-seam
   consequence (12.1 ships no configured-HC traffic-serving fixture — the path stays inert
   until 12.2 wires the probe task + fixture).
2. **No-healthy-upstream synth body = `no healthy upstream` (19 bytes)** (DIVERGES → ADR-0037)
   — a **12.2 deliverable** (D6.2). 12.1 does NOT touch the synth-503 writer path
   (`hcm.rs:582`/`:918`); lock-in #14.
3. **Panic threshold: default 50%, strictly-below (`<`), `Percent { value: f64 }`, panic-mode
   round-robins over ALL** (MATCHED) → D1 `Percent` (lock-in #8) + D5 panic logic (lock-in #13).
4. **Health-check stat names `cluster.<name>.health_check.{attempt,success,failure}` +
   `membership_healthy`** (MATCHED) → D6; 12.1 wires the `membership_healthy` gauge, defers
   the 3 counters to 12.2 (lock-in #4).
5. **HTTP probe shape: default `expected_statuses` = exactly 200; `Int64Range` half-open
   `[start, end)`** (MATCHED) → D1 reuses `Int64Range` directly.
6. **Duration config shape: integer seconds only is the shared form** (DIVERGES from the
   parent fixture sketch's `0.5s`) → D1 reuses `parse_duration` as-is (rejects `0.5s`); D2's
   validator surfaces a sub-second `0.5s` as `InvalidHealthCheckTiming`. The integer-second
   fixture duration is a 12.2 concern (12.1 has no fixture).

### PLAN-write SPEC corrections (read against HEAD `4f9ba04`)

The 8 corrections in PLAN §1 (verified against HEAD): (1) config `Cluster` derives
`Debug, Serialize, Deserialize, PartialEq` (NO `Clone`; HAS `Serialize`) — the new structs
match this, not the parent SPEC's `Clone, Deserialize` sketch; (2) `http_health_check` is
made `Option<HttpHealthCheck>` (validator-required) to keep `UnsupportedHealthCheckType` a
live (constructed) variant — avoids a `dead_code` lint under `-D warnings`; (3) `ConfigError`
lives in `lib.rs:43`, `validate()` + `validate_health_checks` in `bootstrap.rs`; (4)
`parse_duration` at `bootstrap.rs:2289` is `Result<Duration, String>`, integer-only; (5)
`Int64Range` at `bootstrap.rs:1080` is `{ start: i64, end: i64 }`, half-open, validated via
`InvalidInt64Range`; (6) runtime `Cluster` at `cluster.rs:32` gains `endpoint_health` +
`panic_threshold`; `pick()` at `:129`, `pick_endpoint()` at `:152`; (7) `register_gauge ->
Arc<Gauge>`, `Gauge::{inc,dec,set,value}`; (8) `synth_status` at `hcm.rs:918` NOT touched at
12.1 (the no-healthy-body reconciliation is 12.2/ADR-0037). Two cross-crate compile-fix
surfaces are also pre-identified: adding fields to `envoy_config::Cluster` (Task 1) +
the runtime `Cluster` (Task 4) breaks the by-hand `Cluster { }` struct literals in
`crates/envoy-cluster/src/cluster.rs`, `crates/envoy-config/src/bootstrap.rs`,
`crates/envoy-http2/src/hcm.rs`, and `crates/envoy-bin/tests/http2_router_upstream.rs`
(lock-in #16 + #17).

### Carryforward disposition

12.1 engages **no** carryforward (lock-in #19). The 06.3 REVIEW I2 synthetic-backend
down-payment is a 12.2 deliverable (12.1 ships no fixture/harness). The inherited
carryforward inventory (parent SPEC standing list) carries forward UNCHANGED — 12.1 touches
no HTTP-filter file.

---

<!-- state-3 task subsections append below this line -->

## Phase-12.1 state-3 execution arc (Tasks 1-7)

All seven tasks were dispatched to fresh subagents with two-stage review (spec-compliance THEN
code-quality) per `feedback_execution_style` + the 06.x → 11 cadence; TDD per task; one commit
per task. Each task's `cargo fmt --all -- --check` was confirmed clean at its close (06.1 R-9).
No ADR landed (lock-in #2; ledger head stays ADR-0037, next available ADR-0038).

### Task 1 — D1 envoy-config schema + config-`Cluster`-literal compile-fix — `9baa877`

Added `HealthCheck` / `HttpHealthCheck` / `CommonLbConfig` / `Percent` structs (derive
`Debug, Serialize, Deserialize, PartialEq` + `deny_unknown_fields` — NO `Clone`, per lock-in #5)
adjacent to `Int64Range` in `bootstrap.rs`; added `health_checks: Vec<HealthCheck>` +
`common_lb_config: Option<CommonLbConfig>` (both `#[serde(default)]`) to `Cluster`; re-exported
the 4 types from `lib.rs`. `http_health_check` is `Option<HttpHealthCheck>` (lock-in #6 — keeps
`UnsupportedHealthCheckType` a live variant). 3 parse tests (positive HC parse, default
empty-vec/None, `deny_unknown_fields` rejection of `tcp_health_check`) — `test result: ok. 3
passed`. **Compile-fix refinement of lock-in #17:** only `crates/envoy-cluster/src/cluster.rs`
(2 literals) carried by-hand `envoy_config::Cluster {}` literals; the `typed_extension_protocol_options`
occurrences in `hcm.rs` + `http2_router_upstream.rs` are inside YAML strings, not Rust literals
(verified independently by the spec reviewer via `cargo build --workspace --all-targets` clean).
Spec ✅; code-quality **Approved** — the reviewer's "add `Clone`" Important finding was REJECTED
(contradicts the deliberate lock-in #5 / SPEC-correction #1 — match the on-disk `Cluster` cascade
exactly; `Clone` is a trivial non-breaking add later if 12.2 needs it, YAGNI now); minors
(glyph/tag-style/forward-ref doc) match the PLAN's prescribed comment style, left as-is. fmt clean.

### Task 2 — D2 `validate_health_checks` + 6 `ConfigError` variants — `1dd71ff`

Added the 6 variants (`UnsupportedMultipleHealthChecks`, `UnsupportedHealthCheckType`,
`InvalidHealthCheckThreshold {field}`, `InvalidHealthCheckTiming {field}`, `EmptyHealthCheckPath`,
`InvalidPanicThreshold {value}`) to `ConfigError` in `lib.rs`; added `validate_health_checks` in
`bootstrap.rs` (rejects >1 HC, missing http checker, zero thresholds, non-positive/sub-second
durations via `parse_duration`, empty path, inverted `expected_statuses` range via the existing
`InvalidInt64Range`, out-of-`[0,100]` panic value) called in the per-cluster loop of `validate()`.
clippy collapsed the panic-threshold guard into a let-chain (semantics preserved; proven by the
boundary tests). **Initial 10 validate_ tests + 2 added at code-quality review (M1: `unhealthy_threshold:0`;
M2: invalid `interval`) closing symmetric-branch coverage gaps** — `test result: ok. 40 passed`
(`validate_` filter). Spec ✅; code-quality **Approved** (M1/M2 closed via amend; M3 cluster-less
`InvalidInt64Range` left — consistent with the existing `HeaderMatcher.RangeMatch` usage, YAGNI).
`cargo test --workspace` green; fmt + clippy clean.

### Task 3 — D3 `EndpointHealth` state machine — `32cb44a`

New `crates/envoy-cluster/src/health.rs`: `EndpointHealth { state: AtomicU8, consecutive_success/
failure: AtomicU32, healthy/unhealthy_threshold: u32, membership_healthy: Arc<Gauge> }`. Initial
state Unhealthy (§6.2 item-1; gauge contributes 0). `record_success`/`record_failure` reset the
opposite counter, increment, and `inc()`/`dec()` the gauge ONLY on the transition edge (state-guarded;
no double-count). `Relaxed` everywhere (single-writer-per-endpoint; matches the `cluster.rs` cursor
precedent). `is_healthy()` reads the state. `mod health;` + `pub use health::EndpointHealth;` in
`lib.rs`. 5 unit tests (initial state, both flip directions, counter-reset-on-opposite, no-double-inc)
— `test result: ok. 5 passed`. Spec ✅ (every atomic confirmed `Relaxed`; reset/flip logic traced);
code-quality **Approved** — the two "Important" findings were both self-described by the reviewer as
"not a bug" (counter-reset ordering doc gap; unreset success counter — gauge is edge-guarded, wraps
in ~136 yrs); the threshold=0 concern is moot in production (Task 2's validator enforces thresholds
`>= 1`). Code matches the authoritative PLAN. fmt + clippy clean.

### Task 4 — D5 `pick()` unhealthy-exclusion + panic threshold + `from_bootstrap` wiring — `d713386`

Added `endpoint_health: Option<Vec<Arc<EndpointHealth>>>` + `panic_threshold: f64` to the runtime
`Cluster`. Rewrote `pick()`: the `None` arm is **byte-for-byte the phase-02 round-robin** (same
cursor `fetch_add(1, Relaxed)`, same `i % total` — the §5.4 inert-when-unconfigured invariant,
verified against `32cb44a:cluster.rs` by the spec reviewer); the `Some` arm computes `healthy_percent
= 100.0 * healthy/total`, panics (round-robin over ALL) when `healthy_percent < panic_threshold`
(strictly-below; `{value:0}`→0.0 disables), else round-robins the healthy-index subset, returning
`None` when the healthy set is empty and panic is not engaged. `from_bootstrap` registers
`cluster.<name>.membership_healthy` (mapped to `ClusterError::StatsRegistration`) + builds one
`Arc<EndpointHealth>` per endpoint (config thresholds; shared gauge) + reads `panic_threshold`
(default 50.0) when `health_checks` configured; `(None, 50.0)` else. The 2 in-crate test `Cluster {}`
literals fixed. **7 tests + 1 added at code-quality review** (`pick_round_robins_over_noncontiguous_healthy_subset`
— stresses `healthy_idx[i % len]` over a >1-element non-contiguous set) — `cargo test -p envoy-cluster`
`test result: ok. 34 passed`. **The stale `pick_endpoint` doc comment ("effectively infallible in
phase 02") was corrected** (code-quality Important finding — `None` is now genuinely reachable in
production; D-3.4). Spec ✅; code-quality **Approved**. `cargo build --workspace --all-targets` +
`cargo test --workspace` green; fmt + clippy clean (the dead-handle check, lock-in #4 — gauge is
held + used via `EndpointHealth`).

### Task 5 — D6 `membership_healthy` contract row + registration assertions — `8ea3877`

Added the `**12.1 entries (active health checking):**` block to BEHAVIOR_CONTRACT.md's `## Stat-name
mapping` (the `cluster.<name>.membership_healthy` gauge row; value-exact steady-state; reads 0 at
12.1; inert when unconfigured; the 3 `health_check.*` counters explicitly deferred to 12.2 per
lock-in #4). 2 registration tests (configured-HC → gauge present + reads 0; no-HC → no such gauge in
snapshot). **The positive test was hardened at code-quality review** (Important: `register_gauge` is
idempotent and creates a fresh 0-valued gauge on absence → the original `.expect()` form would
false-pass on a missing registration; rewritten to assert presence via `registry.snapshot()` FIRST,
then read the value via the idempotent re-register). `test result: ok. 36 passed` (envoy-cluster).
Spec ✅; code-quality **Approved** after the fix. fmt + clippy clean. No `from_bootstrap` wiring
change (Task 4 owns it).

### Task 6 — D-corpus `parse_bootstrap` seed `cluster_health_check.yaml` — `f5b2e39`

Created the success seed (valid HC config; INTEGER-second durations per §6.2 item-6; exercises panic
threshold + `http_health_check` + `expected_statuses` range), added its `.gitignore` allow-list entry,
and extended the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array — **all three in ONE commit**
(the 09/10/11 Task-6 lesson). Corpus 18 → 19 success seeds. `git check-ignore` empty (not ignored);
`git ls-files` lists it. `fuzz_corpus_seeds_parse_or_reject_cleanly` PASS. Spec ✅ (all 3 files
confirmed in the commit; seed parses+validates; first corpus seed with a `health_checks:` block —
non-redundant); code-quality **Approved** (lone Minor: `static_resources:` before `admin:` vs other
seeds' `admin:`-first convention — cosmetic on order-independent YAML input, left as-is). fmt clean.

### Task 7 — state-4 phase-done verification + STATE advance — THIS commit

Docs-only (PROGRESS + STATE). The §7.5 (a)–(e) gate was run fresh locally per
`superpowers:verification-before-completion` (evidence quoted below); STATE advanced to state-5-next.

**§7.5 gate evidence (fresh local run at HEAD `f5b2e39` + this docs commit):**
- **(e) 5 stable-toolchain gates — all clean:**
  - `cargo build --workspace --all-targets` → `Finished` (exit 0).
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` → `Finished`, no warnings.
  - `cargo fmt --all -- --check` → clean (no diff).
  - `cargo test --workspace` → **832 passed / 0 failed / 2 ignored** (phase-11 baseline 803/0/2 +
    29 new 12.1 tests: Task 1 ×3, Task 2 ×12, Task 3 ×5, Task 4 ×7, Task 5 ×2).
  - `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (the
    license-not-encountered notices are harmless unmatched-allowance warnings, unchanged from prior phases).
- **(a)/(b) differential regression-equivalence — GREEN:** `cargo test -p differential -- --include-ignored`
  → **126 passed / 0 failed / 0 ignored**, including all **18 Docker-gated fixtures** (`echo_fixture`,
  `tcp_proxy_fixture`, `tls_downstream_fixture`, `tls_sni_fixture`, `tls_upstream_fixture`,
  `http1_direct_response_fixture`, `http1_router_upstream_fixture`, `http2_direct_response_fixture`,
  `http2_router_upstream`, `admin_ready_fixture`, `admin_config_dump_server_info`, `admin_drain_listeners`,
  `admin_stats_prometheus`, `access_log_file_sink`, `http_filter_header_mutation_fixture`,
  `http_filter_local_rate_limit_fixture`, `http_filter_rbac_fixture`, `http_filter_fault_fixture`) all
  `... ok` vs `envoyproxy/envoy:v1.33.0`. This is the load-bearing 12.1 proof: the 18 fixtures stay
  green simultaneously with the health-check machinery present-but-inert (no fixture configures
  `health_checks`, so every `pick()` takes the `None` arm = byte-for-byte phase-02 round-robin).
- **(c) h2spec ≥95% — held vacuously:** 12.1 touched zero H2-framing code (only envoy-config,
  envoy-cluster, docs, fuzz corpus); the parent-05 baseline 99.31% is unaffected. No local re-run needed.
- **(d) `parse_bootstrap` fuzz on the 19-seed corpus — clean:** `cargo +nightly fuzz run parse_bootstrap
  -- -runs=200000` → `Done 200000 runs in 16 second(s)`, 0 crashes, exit 0 (cov 13303).

**Carryforward:** 12.1 engaged NO carryforward (lock-in #19); the 06.3 REVIEW I2 synthetic-backend
down-payment is a 12.2 deliverable. The inherited inventory carries forward unchanged.

**Review-surfaced fixes folded into the task commits (recovered in-phase per the two-stage-review
discipline):** Task 1 (rejected the `Clone` finding with rationale); Task 2 (M1/M2 symmetric-branch
tests); Task 4 (stale `pick_endpoint` doc + the non-contiguous-subset cycling test); Task 5 (hardened
the false-passing gauge-presence test). Zero blocking findings carried to state 5; the residual
awareness-only minors (Task 1 doc-style, Task 3 doc-precision, Task 2 M3, Task 6 seed key-order) are
the state-5 reviewer's to disposition.

**Next:** state 5 — `superpowers:requesting-code-review` over the range `2ac2356..` (the 7 task commits)
→ `REVIEW.md`.
