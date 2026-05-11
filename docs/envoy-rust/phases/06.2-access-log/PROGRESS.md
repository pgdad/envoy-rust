# Phase 06.2 — Implementation Progress

Per-task narrative log for sub-phase 06.2 (`envoy-accesslog` foundation + Envoy default-format access-log emitter + HCM access-log wiring + fixture 0012 + BEHAVIOR_CONTRACT.md `Access log field mapping` first-time population). Mirrors the 06.1 / 05.x PROGRESS.md cadence (one section per task; appended at task commit time; quotes meaningful command output inline).

The companion artifacts:
- **SPEC.md** — `docs/envoy-rust/phases/06.2-access-log/SPEC.md` (1130 lines; committed at the parent-06 state-1+state-2 combined-recovery commit `1f7661a`; the design contract).
- **PLAN.md** — `docs/envoy-rust/phases/06.2-access-log/PLAN.md` (committed at this sub-phase's state-2 commit, alongside this PROGRESS.md skeleton; the per-task task list).

## PLAN-write posture (recorded at sub-phase 06.2 state-2 commit, before any task commits)

### LoC drift posture (per 06.2 SPEC §6 signpost 17 + parent-06 SPEC §5 alternative (vi))

The 06.2 SPEC's §3 D1.2-D5.2 deliverable estimates total **~1580 LoC**; the PLAN-time refinement to 11 tasks projects **~1875 LoC** (a ~20% drift over the SPEC's projection, mostly from TDD step decomposition adding boilerplate the SPEC's bare-counts did not anticipate). Per parent-06 SPEC §5 alternative (vi):

> Nested splits of an already-split sub-phase are explicitly rejected. The PLAN-write planner accepts the LoC drift and proceeds.

The 11-task count is comfortably under the §6.1 25-task gate; the LoC overage is genuine (concentrated in D1.2's multi-module envoy-accesslog decomposition with thorough golden-test surface for the hand-rolled ISO-8601 + Gregorian helper, and D4.2.b's hand-rolled tokenizer per architecture decision 9). The named trims listed in PLAN's "Task summary" section — (i) defer in-process backstop, (ii) fold BEHAVIOR_CONTRACT edit into Task 9, (iii) defer fuzz seed — were considered at PLAN-write time and **not applied** (the trims weaken the gate without sufficient LoC reduction; the doctrinally cleaner posture is to accept the estimate). **Acceptance posture: do NOT trim; do NOT nest-split.** This PROGRESS entry is the documented record of the planner's decision per the established 06.1 / 05.x cadence.

### PLAN-write SPEC corrections (recorded for the executor)

The PLAN.md's preamble section "SPEC corrections recorded at PLAN-write time" lists 4 minor projection inaccuracies in the 06.2 SPEC that the planner verified against HEAD `55fe62d`. Reproduced here for stranger-readability:

1. **No single `write_response` call site in H1 `serve_connection`.** The SPEC's pseudocode in §3 D3.2 + §5 suggests one site; the actual code at `crates/envoy-http1/src/hcm.rs` fans across 5 writer paths (synth + 4 proxy-error/happy: `hcm.rs:266-268`, `:286-291`, `:336-341`, `:353-358`, `:364-370`). PLAN resolves by factoring the access-log dispatch to a **single join point** AFTER the `match outcome` block ends, BEFORE the keep-alive loop continuation at `hcm.rs:375-377`. Task 6 implements.

2. **H2 empty-body path ends via `send_response(.., end_of_stream=true)` not `send_data`.** The SPEC §3 D3.2 says "AFTER `send_data(.., end_of_stream=true)`" but the actual `send_envoy_response` at `crates/envoy-http2/src/response.rs:62-76` skips `send_data` on the empty-body branch (`send_response(head, end_of_stream=resp.body.is_empty())` at line 68 ends the stream when the body is empty). PLAN resolves by landing the H2 dispatch AFTER `send_envoy_response` returns (at `crates/envoy-http2/src/hcm.rs:251`), covering both branches uniformly. Task 7 implements.

3. **`Driver` variant is `TcpEcho`, not `Tcp`.** SPEC §4 references a `Tcp` variant; the actual enum at `tests/differential/src/lib.rs:38-112` has no such variant (the TCP-shaped driver is `Driver::TcpEcho`; the full pre-06.2 variant set is `TcpEcho | HttpGet | TlsTcp | TlsTcpProbeList | Http1 | Http1ProbeList | Http2 | AdminScrape` — 8 variants). Minor naming fix; no code impact.

4. **`HttpConnectionManagerConfig` uses `#[serde(deny_unknown_fields)]` so the new `access_log` field needs `#[serde(default)]`.** SPEC §3 D2.2.a's example does declare `#[serde(default)]` but the rationale is silent. The struct at `crates/envoy-config/src/bootstrap.rs:356-370` carries `#[serde(deny_unknown_fields)]`; without `#[serde(default)]` on `access_log`, the 5 existing HCM-bearing fixtures (`0007/0008/0009/0010/0011`) would fail to parse. Task 5 lands `#[serde(default)]` to keep the absent-block parse green.

**5th clarifying correction (recorded for the executor):** **`envoy-http2` DOES need a direct path-dep on `envoy-accesslog`.** The SPEC §3 D1.2 architectural Rule 1 says *"`envoy-http2` does NOT add a direct dep on `envoy-accesslog`"* on the grounds that *"the alias carries the new `access_log: Vec<Arc<FileSink>>` field transparently"*. But the SPEC §3 D3.2 dispatch pseudocode for the H2 path calls `sink.emit(&record).await` on each `Arc<FileSink>` element of `config.access_log` — a concrete-type method call on `envoy_accesslog::FileSink`, which requires the concrete type to be resolvable at compile time in `envoy-http2`, which requires the path-dep. Task 7 adds `envoy-accesslog = { path = "../envoy-accesslog" }` to `crates/envoy-http2/Cargo.toml`'s `[dependencies]`.

These are minor projection inaccuracies; the SPEC remains in-tree unedited per D-3.5.

### Architecture decisions locked at PLAN-write time

Per the user's standing preference `feedback_pick_recommendation`, every signpost in 06.2 SPEC §6 resolves to its recommendation. Recorded here for stranger-readability (full list in PLAN.md's "Architecture decisions locked at PLAN-write time" section):

- **Sink trait deferred** per option (c); FileSink ships concretely; `HCMConfig.access_log: Vec<Arc<FileSink>>` typed concretely; `crates/envoy-accesslog/src/sink.rs` ships as a doc-comment-only placeholder.
- **HCM dispatch posture: synchronous-after-write** (option (b) per Rule 4); the HCM awaits `sink.emit().await` at the factored join point; emission errors logged via `tracing::warn!` and discarded.
- **ISO-8601 emitter buffer shape:** `&mut String` (signpost 1).
- **Gregorian calendar helper inline** in `default_format.rs` (signpost 2).
- **FileSink concurrency:** `Arc<tokio::sync::Mutex<File>>` (signpost 3).
- **`AccessLogRecord` ownership:** owned `String`s (signpost 5).
- **FileSink path validation:** none beyond OS-level open (signpost 6).
- **`O_APPEND` semantics:** no truncate; no rotation handling (signpost 7).
- **Harness tokenizer:** hand-rolled state machine (signpost 8); no regex dep.
- **`%DURATION%` units:** integer milliseconds via `Duration::as_millis()` (signpost 9).
- **`%UPSTREAM_HOST%` format:** `SocketAddr` Display impl (signpost 11).
- **Fuzz seed:** single-entry (signpost 12).
- **Test logging capture:** custom in-process tracing layer (signpost 15).
- **No `Default` impl on `AccessLogRecord`** (signpost 14).
- **No new top-level Cargo deps** (signpost 20); the new `envoy-accesslog` crate's deps (`tokio`/`bytes`/`tracing`/`thiserror`/`envoy-http1` path-dep) are all already in the workspace's resolved graph; `tempfile = "3"` already a dev-dep in 6 other workspace crates.
- **No ADRs anticipated to land** in 06.2 (per §7 ADR projection); conditional ADR-0030 / ADR-0031 stay available.
- **H1 dispatch site:** factored join point AFTER the `match outcome` block in `serve_connection` (PLAN-write SPEC correction 1).
- **H2 dispatch site:** at `crates/envoy-http2/src/hcm.rs:251` AFTER `send_envoy_response` returns (PLAN-write SPEC correction 2).

### Task ordering note

The 11 PLAN tasks are numbered for documentation. The recommended **execution order** is `1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11` (linear; no reordering needed). Task 1 lands at this state-2 commit (the PROGRESS.md preamble). Tasks 2-4 build the `envoy-accesslog` crate (scaffold → emitter → sink). Task 5 lands the `envoy-config` schema. Tasks 6-7 wire the HCM (H1 + H2). Task 8 lands the in-process backstop. Task 9 extends the differential harness. Task 10 lands the fixture + BEHAVIOR_CONTRACT.md edit. Task 11 verifies state-4. No task has a non-numeric dependency on a later task; the linear order is the recommended execution order.

## Task 1 — PROGRESS.md preamble + LoC drift posture + 4 SPEC corrections + signpost choices

(THIS section. Lands at sub-phase 06.2 state-2 commit alongside PLAN.md and the STATE.md / ROADMAP.md advance.)

## Tasks 2 through 11

Appended at execution time, one section per task commit, mirroring the 06.1 / 05.x per-task cadence.
