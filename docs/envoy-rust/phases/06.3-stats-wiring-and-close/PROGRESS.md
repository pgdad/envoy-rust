# Phase 06.3 (`06.3-stats-wiring-and-close`) — PROGRESS

> Per-task narrative log appended at each substantive commit. Stranger-readable
> per D-3.4. Task 1 lands AT the state-2 standalone PLAN.md commit (no separate
> Task 1 commit per the standalone-PLAN cadence established by 06.1
> `505653d` and 06.2 `dc00750`).

## Task 1 — PROGRESS.md preamble + LoC drift posture + 5 SPEC corrections + 22 architecture decisions (state-2 commit)

This task lands AT the state-2 standalone PLAN.md commit. The remaining Tasks
2-12 land at state-3 each as their own commit.

### LoC drift posture (per BOOTSTRAP_PROMPT.md §6.1 + parent-06 SPEC §5 alternative (vi))

06.3 SPEC §3 projects ~770 LoC code+tests + ~80 review/state-6 overhead.
Task-count projection: 12 tasks. Both projections are comfortably under the
§6.1 split-gate (~25 tasks or ~1500 LoC of net change).

Per parent-06 SPEC §5 alternative (vi), 06.3 may NOT nest-split itself even
if execution-time drift pushes a task over its task-local budget — the
accept-drift posture is the established release valve. The 06.1 + 06.2
precedent ratifies this: 06.1 SPEC projected ~1300 LoC and PLAN landed
~2010 LoC; 06.2 SPEC projected ~1300 LoC and PLAN landed ~1875 LoC; both
honored the no-nest-split posture and absorbed the ~+50% PLAN-vs-SPEC
narrative-density growth without re-splitting.

The PLAN-write planner cross-checked the SPEC's 770-LoC code estimate against
the in-tree code surfaces touched by each task. The estimate holds at
PLAN-write time (no surprise scope discovered at code-read).

### PLAN-write SPEC corrections (recorded for the executor; 5 corrections)

Mirrors 06.1's 4 corrections + 06.2's 4 + 1 clarifying. Per D-3.5, the
SPEC remains in-tree unedited; corrections are recorded HERE so a stranger
reading PROGRESS catches the SPEC-vs-implementation diff:

1. **SPEC §3 D15.3.a wrongly co-locates per-class HCM counter increment with
   06.1's `downstream_rq_total` increment site.** Empirically the 06.1
   increment fires at request-entry time (`crates/envoy-http1/src/hcm.rs:251`,
   not at on-response-complete. Resolution: per-class counters land at the
   factored access-log dispatch site (post-`match outcome` block, lines 459+),
   after `response_status_for_log` is populated. 06.1's request-entry
   `downstream_rq_total.inc()` continues unchanged at line 251. PLAN Task 4
   names the exact insertion point.

2. **SPEC §3 D15.3.b's listener gauge claim needs to factor 06.1 D4.a's
   `let cx_total = self.cx_total;` hoist for the `tokio::select!` accept-arm
   capture.** Empirical at `crates/envoy-listener/src/lib.rs:143-160`. The
   new `cx_active` gauge follows the same hoist pattern. Per signpost 7 the
   gauge scopes to data-path listeners only — the planner threads a
   `count_active: bool` field through `ListenerConfig`, defaulting to `true`
   and overridden to `false` at envoy-bin's admin-listener construction.
   PLAN Task 5 names the exact wiring.

3. **SPEC §3 D15.3.c proposes adding `cluster: &ClusterHandle` to
   `write_proxied_response`** — straightforward at H1's call site
   (`crates/envoy-http1/src/hcm.rs:418-424`) but the H2 router-arm does NOT
   call `write_proxied_response` (it builds the downstream `Response` inline
   at `crates/envoy-http2/src/hcm.rs:280-318`, verified). Resolution: H2
   lands inline `upstream_rq_total.inc()` + `upstream_rq_5xx.inc()` at the
   proxy-resp construction site, parallel to the H1 helper's increments.
   PLAN Task 7 names both sites separately.

4. **SPEC §3 D14.3 validator scan reuses the existing
   `for vh in &mut hcm.route_config.virtual_hosts { for r in &mut vh.routes }`
   walk shape at `crates/envoy-config/src/bootstrap.rs:1346-1401`.** The new
   H1×H2 reachability check sits inside the existing `RouteAction::Route(ar)`
   arm at line 1387-1394 alongside the `UnknownCluster` check. No new walk
   structure; the cluster-name HashMap is built once at the start of the
   listener walk per signpost 1's eager single-pass recommendation. PLAN
   Task 2 sets out the exact code.

5. **SPEC §3 D15.3.b cluster-side gauge increment site is at the HCM
   proxy-arm call sites** (`crates/envoy-http1/src/hcm.rs:389-396` +
   `crates/envoy-http2/src/hcm.rs:222-244`), NOT inside `envoy-http1::Client`
   or `envoy-http2::Client`. Per parent-06 SPEC §6 Rule 2 (consumers
   increment), putting the increment inside the codec crates would couple
   them to the cluster-stats namespace. The decrement is RAII-style via
   `ConnGaugeGuard` from envoy-cluster (architecture decision 13). PLAN Task 6
   defines the RAII guard.

### Architecture decisions locked at PLAN-write time (22 decisions)

See PLAN.md "Architecture decisions locked at PLAN-write time (signpost
choices)" section for the full 22-entry table covering all 10 SPEC §7
signposts plus 12 PLAN-write-time decisions on adjacent concerns
(`access_logs_failed` sibling counter ships; TCP-proxy `cx_active` wired;
ConnGaugeGuard RAII; listener cx_active decrement via Arc<Gauge> clone in
spawned task; co-location of 06.2 REVIEW I1 fix with Task 4; etc.).

### Task-ordering rationale

Per PLAN.md "Task summary > Sequencing rationale": Task 2 (D14.3) first
per SPEC §5 close-out posture (mirrors 05.1 Task-1 / 05.3 Task-1 / 06.2
Task-1 preamble cadence); Task 3 (D18.3 harness) before Task 11 (D17.3
fixture) so the fixture references the new BodyRule fields; Tasks 4-8 wire
the comprehensive stats in per-stat-family order; Task 9 (06.1 REVIEW I1)
isolated mid-PLAN; Task 10 (D15.3.e + 06.2 REVIEW I2 diagnosis); Task 11
(D16.3 + D17.3 + 06.2 M3 doc fix) lands LAST among substantive tasks
(extends contract before allow-list per 06.1 REVIEW §7 R-1); Task 12
(D20.3) state-4 verification.

### Carryforwards closed in 06.3 (planned)

- **05.3 REVIEW I1** (closed at Task 2 via `ConfigError::Http2ClusterFromHttp1Listener` parse-time gate). Mirrors phase-05.1 Task-1's posture toward phase-02.1 REVIEW I3.
- **06.1 REVIEW I1** (closed at Task 9 via admin handler idle read timeout). Per user recommendation to fold opportunistically into 06.3 when it touches the admin handler surface.
- **06.2 REVIEW I1** (closed at Task 4 via H1 state-init tightening, mechanically co-located with per-class HCM counter wiring at the same `match outcome { ... }` block).
- **06.2 REVIEW I2** (closed at Task 10 via empirical diagnosis — tighten fixture 0012 expectations.yaml row 12 from `wildcard` to `exact: "-"`, observe outcome, update BEHAVIOR_CONTRACT.md row 12 OR commit the fixture tightening).
- **06.2 REVIEW M3** (closed at Task 11 via fixture 0012 README.md path correction; ~5 LoC).

### Standing carryforwards untouched in 06.3 (per parent-06 SPEC §4 + 06.1/06.2 REVIEW §4 inventories)

- 06.2 REVIEW M1 (`Http1Error::AccessLogOpen` source-chain typing) — indefinite.
- 06.2 REVIEW M2 (`BodyRule::ByteExact` literal-body assertion) — indefinite.
- 06.2 REVIEW M4 (`/tmp/0012-envoy-mount` process-shared path) — activates under nextest sharding.
- 06.2 REVIEW M5 (H2 access-log test 200ms sleep) — 02.2 M1 chain.
- 06.1 REVIEW I2 + M1 + M4 — phase 08.
- 06.1 REVIEW M2 / M3 / M5 / M6 — indefinite / 02.2 chain.
- 05.3 REVIEW I2 (typed-error chain dissolution at H2 dispatch) — defers to phase that next touches H2 router-arm.
- 05.2 REVIEW I1 + I2 + I3 — defers to whichever phase next touches `.github/workflows/ci.yml` or the h2 codec.
- 04.1 REVIEW M5/M9 (Cargo.lock cadence ratification) — couples with conditional ADR-0031.
- 04.1 REVIEW M7 (TLS+H2 ALPN dispatch generalization) — defers to phase that ships H2+TLS.
- 04.1 REVIEW M1/M2/M4 — defers to phase exercising duplicate response headers / stalled body / IPv6 Host.
- 02.2 REVIEW M1 (EchoBackend Drop polling) — standing.

### DECISIONS.md ledger head at state-2

**ADR-0029** (parent-06 split decision; landed at `1f7661a`). No ADRs landed
in 06.1 or 06.2; recommended posture honored. No new ADRs projected for 06.3
per SPEC §8. Conditional ADR-0030 (foundations grant) + ADR-0031 (Cargo.lock
cadence) stay available; recommendation per parent-06 SPEC §7 is no
foundations grants in phase 06.

### State-2 commit composition

This commit lands ONE doc-only commit:
1. `docs/envoy-rust/phases/06.3-stats-wiring-and-close/PLAN.md` (NEW; this PLAN).
2. `docs/envoy-rust/phases/06.3-stats-wiring-and-close/PROGRESS.md` (NEW; THIS file's Task 1 preamble).
3. `docs/envoy-rust/ROADMAP.md` (row 06.3 status: planned → in-progress).
4. `docs/envoy-rust/STATE.md` (advance Active phase to state-3-next; next-skill `superpowers:subagent-driven-development`).

NO code changes. NO new ADRs. NO test runs. NO CI push. Tasks 2-12 land at
state-3 each as their own commit.

Per BOOTSTRAP_PROMPT.md §5.1 "one state per session": this session lands the
state-2 commit and exits; the next session enters state 3 and executes Task 2
(D14.3 validator gate) via `superpowers:subagent-driven-development`.
