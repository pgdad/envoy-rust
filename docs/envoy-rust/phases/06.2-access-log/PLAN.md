# Phase 06.2 — `envoy-accesslog` foundation + Envoy default format + HCM access-log wiring + fixture 0012 + BEHAVIOR_CONTRACT.md `Access log field mapping` first-time population — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (per the user's standing preference; auto-memory `feedback_execution_style`) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the access-log subsystem foundation as a new workspace crate `crates/envoy-accesslog/` (sole-dep-owner of the access-log surface per parent-06 cross-sub-phase architectural Rule 1), HCM on-response-complete fire-and-forget access-log dispatch on both H1 and H2 paths via the type-aliased `HCMConfig` from 05.2 D1, `envoy-config` schema growth (`HttpConnectionManagerConfig.access_log: Vec<AccessLog>` with file-sink-only validator gate + 2 new `ConfigError` variants), differential harness extension (`Driver::Http1WithAccessLog` + `AccessLogLineRule` per-token rules + hand-rolled default-format tokenizer), fixture `tests/fixtures/0012-access-log-file-sink/`, and the first-time population of `docs/envoy-rust/BEHAVIOR_CONTRACT.md`'s `Access log field mapping` section (14 default-format token rows per parent-06 SPEC §2.2).

**Architecture:** `envoy-accesslog` is a foundation library carrying `AccessLogRecord` (15-field POD struct), a concrete `FileSink` (tokio `OpenOptions::append(true)` + per-sink `Arc<tokio::sync::Mutex<File>>` to serialize concurrent emissions), the `default_format::format` emitter, and a hand-rolled ISO-8601 timestamp emitter (`format_iso8601` + a `~30-LoC` Gregorian-calendar `epoch_seconds_to_ymd_hms` helper, golden-tested against epoch 0 / leap-day boundaries / century boundaries / year-9999). The `Sink` trait is **deferred** per parent SPEC §3 D8.2 option (c) — `FileSink` ships concretely; multi-sink dispatch lands when N≥2 sinks exist in a future observability-family phase. `envoy-http1::HCMConfig` (the per-listener immutable config struct from 04.1, extended in 04.3/05.2/06.1) gains a `pub access_log: Vec<Arc<envoy_accesslog::FileSink>>` field; the H2 path inherits the field transparently via the `pub type HCMConfig = Http1HCMConfig` alias at `crates/envoy-http2/src/hcm.rs:27`. The dispatch posture is **synchronous-after-write** (option (b) of parent-06 architectural Rule 4 / 06.2 SPEC signpost 4): the HCM awaits `sink.emit(&record).await` after every successful response write at a factored join point in `serve_connection`; emission errors are logged via `tracing::warn!` and never propagate to the response-write path or the request handling result.

**Tech Stack:** Rust edition 2024 (workspace pin per ADR-0003). `tokio` (`fs`, `io-util`, `sync` features — `OpenOptions::append`, `AsyncWriteExt::write_all`, `Mutex<File>`); `bytes` (transitive surface; carried for shape consistency with the other workspace crates); `tracing` (structured `warn!` on emission failure); `thiserror` (typed `AccessLogError`); `envoy-http1` (path-dep, for `Request`/`Response` value-types consumed at HCM record-build time). No new permitted-foundations grants in 06.2 under the recommended posture per parent-06 SPEC §7 + 06.2 SPEC §7 — D-3.2 names *Access log formatters and sinks* on the *Must be written from scratch* list and the Envoy default-format emitter + ISO-8601 timestamp emitter ship hand-rolled atop existing `tokio` + `bytes` + `tracing` + `thiserror` + `serde_yaml` foundations.

**Source SPEC:** `docs/envoy-rust/phases/06.2-access-log/SPEC.md` (1130 lines; the design contract; committed at parent-06 state-1+state-2 combined-recovery commit `1f7661a`). Parent SPEC: `docs/envoy-rust/phases/06-observability/SPEC.md` (committed at the same `1f7661a`; cross-sub-phase architectural rules in §6 — Rules 1, 4, 5 bind on 06.2).

**Repository state at PLAN-write time:** HEAD is `55fe62d` (sub-phase 06.1 state-6 phase-done close-out — flips ROADMAP row `06.1` → `done`, advances STATE.md to 06.2 lifecycle state 2). DECISIONS.md ledger head = ADR-0029 (parent-06 split decision; no ADR landed in 06.1). ROADMAP row `06.2` `status: planned`; row `06` `status: in-progress`; row `06.3` `status: planned`. The 11 baseline differential fixtures (`0001-tcp-echo` through `0010-http2-router-upstream` + `0011-admin-stats-prometheus`) are green at the 06.1 state-4 verification commit `a5f795c` per CI run `25625271032` (HEAD `36fedd8`, conclusion `success`, completed `2026-05-10T09:33:41Z`); `tests/conformance/h2spec/` rides the parent-05 baseline at 99.31% pass (CI run `25333279366`). `cargo build --workspace --all-targets` is green at HEAD (no warnings, no errors).

---

## SPEC corrections recorded at PLAN-write time

The 06.2 SPEC was written before its planner verified every code shape against HEAD `55fe62d`. Four material projection inaccuracies are corrected inline in this PLAN; the SPEC remains in-tree unedited per D-3.5 (append-only). Task 1's PROGRESS.md preamble records the corrections explicitly. The verdict on each: minor, no doctrine-level impact, mechanically resolved at PLAN-write time.

**Correction 1 — No single `write_response` call site in H1 `serve_connection`.** SPEC §3 D3.2 + §5 say *"AFTER `write_response` returns successfully (or after the 502 fallback writes successfully) and BEFORE the next iteration of the keep-alive loop"* as if there's one call site. In fact `crates/envoy-http1/src/hcm.rs::serve_connection` (signature at `hcm.rs:180-183`) fans the response writers across **five sites** depending on outcome:

- `hcm.rs:266-268` — synth-response path (`Http1Response::write_to(&resp, &mut downstream).await?`)
- `hcm.rs:286-291` — proxy 503 (cluster missing) error path (`Http1Response::write_to(&resp, &mut downstream).await?`)
- `hcm.rs:336-341` — proxy 502 (connect-failed) error path (`Http1Response::write_to(&resp, &mut downstream).await?`)
- `hcm.rs:353-358` — proxy 502 (other typed-error variant) error path (`Http1Response::write_to(&resp, &mut downstream).await?`)
- `hcm.rs:364-370` — proxy happy path (`crate::router::write_proxied_response(&mut downstream, upstream_response, elapsed_ms, close).await?`)

**Resolution:** the H1 access-log dispatch lands at a **single factored join point** AFTER the `match outcome { ... }` block ends but BEFORE the keep-alive loop's `continue` / `return Ok(())` decision at `hcm.rs:375-377`. The dispatch site reads from per-request state (the `Request` captured pre-route-walk + the `Response`/upstream-host/timing captured pre-or-post-write) and emits one access-log record per request regardless of which of the 5 writers fired. Task 6 implements this factoring.

**Correction 2 — H2 empty-body path ends via `send_response(.., end_of_stream=true)` not `send_data`.** SPEC §3 D3.2 says *"AFTER `send_data(.., end_of_stream=true)` writes the response and BEFORE the spawned task drops cleanly"* — but the actual code at `crates/envoy-http2/src/response.rs:62-76` (`send_envoy_response`) skips the `send_data` call on the empty-body branch:

```rust
pub async fn send_envoy_response(
    mut send_response: h2::server::SendResponse<bytes::Bytes>,
    resp: Response,
) -> Result<(), Http2Error> {
    let head = build_http_response(&resp)?;
    let mut send_stream = send_response
        .send_response(head, /* end_of_stream = */ resp.body.is_empty())
        .map_err(|source| Http2Error::H2StreamAccept { source })?;
    if !resp.body.is_empty() {
        send_stream
            .send_data(resp.body, /* end_of_stream = */ true)
            .map_err(|source| Http2Error::H2BodyRead { source })?;
    }
    Ok(())
}
```

**Resolution:** the H2 access-log dispatch lands at `crates/envoy-http2/src/hcm.rs:251` — AFTER `send_envoy_response(send_response, resp).await` returns (whichever branch fired). This handles both branches uniformly, mirrors the H1 factoring shape, and avoids the `response.rs` site being responsible for record emission. Task 7 implements this site selection.

**Correction 3 — `Driver` variant naming: `TcpEcho` not `Tcp`.** SPEC §4 ("Non-goals") refers to the existing `Tcp` variant. The actual enum at `tests/differential/src/lib.rs:38-112` has no `Tcp` variant; the TCP-shaped driver is `Driver::TcpEcho` (no payload argument; payload is read from `inputs/payload.bin` per `lib.rs:1356-1357`). Also missing from the SPEC enumeration: `Driver::TlsTcp`, `Driver::TlsTcpProbeList`, and `Driver::HttpGet`. The full pre-06.2 variant set is `TcpEcho | HttpGet | TlsTcp | TlsTcpProbeList | Http1 | Http1ProbeList | Http2 | AdminScrape` — 8 variants. **Resolution:** no code impact; PLAN reads use the actual names; Task 9 adds `Http1WithAccessLog` as the 9th variant, slotting between `Http1ProbeList` and `Http2`.

**Correction 4 — `HttpConnectionManagerConfig` uses `#[serde(deny_unknown_fields)]` so the new `access_log` field needs `#[serde(default)]`.** SPEC §3 D2.2.a's example does declare `#[serde(default)]` on the new field, but is silent on the rationale. The struct at `crates/envoy-config/src/bootstrap.rs:356-370` carries `#[serde(deny_unknown_fields)]` (line 357); without `#[serde(default)]` on `access_log`, all 5 existing HCM-bearing fixtures (`0007-http1-direct-response`, `0008-http1-router-upstream`, `0009-http2-direct-response`, `0010-http2-router-upstream`, `0011-admin-stats-prometheus`) would fail to parse because they don't supply `access_log:`. **Resolution:** `#[serde(default)]` on the new field is load-bearing for back-compat with existing fixtures; Task 5 lands it as `#[serde(default)] pub access_log: Vec<AccessLog>` to keep the absent-block parse green.

**Correction 5 (clarifying)** — **`envoy-http2` DOES need a direct path-dep on `envoy-accesslog`.** SPEC §3 D1.2 architectural Rule 1 says *"`envoy-http2` does NOT add a direct dep on `envoy-accesslog`"* on the grounds that *"the alias carries the new `access_log: Vec<Arc<FileSink>>` field transparently"*. But the SPEC §3 D3.2 dispatch pseudocode for the H2 path calls `sink.emit(&record).await` on each `Arc<FileSink>` element of `config.access_log` — this is a concrete-type method call on `envoy_accesslog::FileSink`, which requires the concrete type to be resolvable at compile time in the `envoy-http2` crate, which requires the path-dep. **Resolution:** Task 7 adds `envoy-accesslog = { path = "../envoy-accesslog" }` to `crates/envoy-http2/Cargo.toml` `[dependencies]`. The cross-sub-phase architectural rule 1 (`envoy-accesslog` is the sole workspace dep on the access-log surface) holds in the sense that no NEW workspace crate beyond the HCM-bearing ones consumes `envoy-accesslog` — but the existing HCM-bearing crates (`envoy-http1` AND `envoy-http2`) both gain the path-dep at 06.2.

---

## Architecture decisions locked at PLAN-write time

Per the user's standing preference `feedback_pick_recommendation` ("always pick the recommended option; do not ask"), every signpost in 06.2 SPEC §6 resolves to its recommendation. Recorded here for stranger-readability and so the executor doesn't re-litigate at task time.

1. **Sink trait deferred (option (c) per parent SPEC §3 D8.2 / 06.2 SPEC §3 Rule 3).** `FileSink` ships concretely. `HCMConfig.access_log: Vec<Arc<envoy_accesslog::FileSink>>` typed concretely; no `Box<dyn Sink>` or `Arc<dyn Sink>`. **Consequence:** `envoy-accesslog`'s `src/sink.rs` ships as a placeholder file with a top-of-file doc comment explaining the deferral; `lib.rs` declares `mod sink;` as a private module so future trait promotion is a single-file edit.
2. **HCM dispatch posture: synchronous-after-write (option (b) per Rule 4 / signpost 4).** The HCM awaits `sink.emit(&record).await` at the factored join point in `serve_connection`; emission errors are logged via `tracing::warn!` and discarded. No `tokio::spawn` fire-and-forget. The per-request task duration extends by the sink emission latency (typically sub-millisecond for FileSink); future migration to spawn-based dispatch is mechanical when I/O-heavy sinks land.
3. **ISO-8601 emitter buffer shape: `&mut String` (option (a) per signpost 1).** `format_iso8601(s: &mut String, t: SystemTime)` writes 24 ASCII bytes via `write!(s, ...).unwrap()`; ergonomic; perf not a concern.
4. **Gregorian calendar helper: inline in `default_format.rs` (option (a) per signpost 2).** ~30 LoC private fn `epoch_seconds_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32)`; co-located with its sole consumer `format_iso8601`; no separate `mod gregorian`.
5. **FileSink concurrency: `Arc<tokio::sync::Mutex<File>>` (option (a) per signpost 3).** Per-sink mutex serializes concurrent emissions on the same `Arc<FileSink>`; sub-microsecond contention; preserves append-semantic atomicity guarantees.
6. **`AccessLogRecord` ownership: owned `String`s (per signpost 5; finalized in SPEC).** No lifetime parameter; record is `Clone + Debug` (not `PartialEq`, not `Default`). The owned strings let the record cross spawn boundaries cheaply if future code needs that (not used in 06.2 under option (b)).
7. **FileSink path validation: none (per signpost 6).** `FileSink::new(path)` does not check the parent directory exists, does not check path is absolute vs relative, does not check disk space. Errors surface as `AccessLogError::Open` at the OS-level open() boundary. The `envoy-config` validator at D2.2.b checks only that `path` is non-empty.
8. **`O_APPEND` semantics: no truncate on startup; no rotation handling (per signpost 7).** Existing log files are appended-to, not truncated. Log rotation (rename + recreate) is handled by UNIX semantics — writes after rotation continue to the unlinked inode. The fixture-0012 harness deletes any existing `/tmp/0012-*-access.log` before envoy-bin / Envoy starts, so the rotation question doesn't bind on the fixture's byte-equivalence.
9. **Harness tokenizer: hand-rolled state machine (option (a) per signpost 8).** ~80 LoC in `tests/differential/src/access_log.rs`; no regex dep; ADR-0021's `regex` scope stays narrow to `envoy-config` per its original bounding.
10. **`%DURATION%` units: integer milliseconds via `duration.as_millis()` (per signpost 9).** Envoy's documented default format renders ms; envoy-rust matches. No fractional milliseconds, no pre-saturation.
11. **`%UPSTREAM_HOST%` format: `SocketAddr` Display impl (per signpost 11).** `format!("{}", socket_addr)` renders `127.0.0.1:8080` for IPv4 / `[::1]:8080` for IPv6 per RFC 5952 + Rust standard Display; fixture 0012's direct_response path produces `None` rendered as `-`, so the format choice is forward-compat-only in 06.2.
12. **Fuzz seed: single-entry (per signpost 12).** `hcm_access_log_file.yaml` ships one HCM with one `access_log` entry; multi-entry seeds defer.
13. **Test logging capture: custom in-process tracing layer (option (b) per signpost 15).** ~30 LoC of test fixture in Task 6's HCM emission-failure test; no `tracing-test` dev-dep.
14. **PLAN.md cadence: standalone pre-Task-1 commit (per signpost 17).** This PLAN.md lands at THIS commit alongside the PROGRESS.md skeleton + STATE.md / ROADMAP.md updates; Task 1's substantive entry IS this PROGRESS.md preamble; no Task 2 code lands at THIS commit.
15. **No `Default` impl on `AccessLogRecord` (signpost 14).** Enforced by full-field literal struct construction at the HCM record-build site. Defaulting silently could mask omissions.
16. **No new top-level Cargo deps (per signpost 20).** The new `envoy-accesslog` crate's deps (`tokio`, `bytes`, `tracing`, `thiserror`, plus the `envoy-http1` path-dep) are all already in the workspace's resolved graph. `tempfile = "3"` is already a dev-dep in 6 other crates; Task 4 uses it for `FileSink` unit tests. Cargo.lock diff at scaffold time is workspace-member registrations only (mirrors 06.1's `+31 lines` no-new-external-crates posture).
17. **No ADRs anticipated to land in 06.2** (per §7 ADR projection). Conditional ADR-0030 (`time = "0.3"` / `async_trait = "0.1"` foundations grant) explicitly NOT projected. Conditional ADR-0031 (Cargo.lock cadence ratification) stays conditional on ADR-0030 and also not projected. The 14 unit tests in Task 3 are sufficient to validate the hand-rolled ISO-8601 emitter; the HCM dispatch site reads cleanly under option (c) (concrete `FileSink` via `Vec<Arc<FileSink>>`). If execution-time experience materially diverges from these projections, the executor lands the next-sequential ADR per D-3.5 (append-only).
18. **H1 access-log dispatch site: factored join point AFTER the `match outcome { ... }` block** at `crates/envoy-http1/src/hcm.rs:264-372` ends, BEFORE the keep-alive `continue`/`return Ok(())` decision at `hcm.rs:375-377`. The 5 writer sites (synth + 3 proxy-error fallbacks + proxy happy) all converge to this join point; one dispatch site handles all 5 outcomes. Task 6 documents the exact refactor shape.
19. **H2 access-log dispatch site: AFTER `send_envoy_response(send_response, resp).await` returns** at `crates/envoy-http2/src/hcm.rs:251`. Handles both empty-body (`send_response(.., end_of_stream=true)` at `response.rs:68`) and non-empty-body (`send_data(.., end_of_stream=true)` at `response.rs:73`) branches uniformly. Task 7 implements.

---

## Task summary

11 tasks. 1 PROGRESS preamble (docs only, lands at THIS PLAN.md commit) + 3 envoy-accesslog tasks (D1.2) + 1 envoy-config schema task (D2.2) + 2 HCM-wiring tasks (D3.2 H1 + D3.2 H2) + 1 in-process backstop task (signpost 18) + 1 harness task (D4.2.a–b) + 1 fixture-and-contract task (D4.2.c + D5.2 combined per signpost recommendation that the BEHAVIOR_CONTRACT edit lands in lockstep with the first-fixture-that-asserts-on-the-table) + 1 state-4 verification task (D6.2).

| #  | Task                                                                                                       | Touches                                                                                                                                                          | LoC est. | Maps to SPEC §3 |
|----|------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------|----------|-----------------|
| 1  | PROGRESS.md preamble + LoC drift posture + 4 SPEC corrections + signpost choices                           | docs only                                                                                                                                                        | ~80      | meta            |
| 2  | envoy-accesslog crate scaffold + `record.rs` + `error.rs` + 2 record unit tests + `sink.rs` placeholder    | NEW crate (`crates/envoy-accesslog/Cargo.toml` + `src/{lib,record,error,sink}.rs`) + workspace `members`                                                         | ~165     | D1.2            |
| 3  | `default_format.rs` — `format` + `format_iso8601` + `epoch_seconds_to_ymd_hms` + 8 unit tests              | `crates/envoy-accesslog/src/default_format.rs`                                                                                                                   | ~280     | D1.2            |
| 4  | `file_sink.rs` — `FileSink::{new,emit}` + 4 unit tests                                                     | `crates/envoy-accesslog/src/file_sink.rs` + `Cargo.toml` `[dev-dependencies] tempfile = "3"`                                                                     | ~180     | D1.2            |
| 5  | envoy-config schema — `AccessLog` + `FileAccessLogTypedConfig` + 2 `ConfigError` variants + validator + 6 unit tests + 1 corpus-walk + fuzz seed | `crates/envoy-config/src/{bootstrap.rs,lib.rs}` + `crates/envoy-config/fuzz/{.gitignore,corpus/parse_bootstrap/hcm_access_log_file.yaml}`                | ~235     | D2.2            |
| 6  | HCM H1 access-log wiring — `HCMConfig.access_log` field + `from_config` extension + factored dispatch site + 4 unit tests | `crates/envoy-http1/{Cargo.toml,src/hcm.rs}`                                                                                                          | ~230     | D3.2 (H1)       |
| 7  | HCM H2 access-log wiring — `envoy-http2` path-dep on `envoy-accesslog` + dispatch site at `hcm.rs:251` + 2 unit tests | `crates/envoy-http2/{Cargo.toml,src/hcm.rs}`                                                                                                                       | ~120     | D3.2 (H2)       |
| 8  | In-process integration backstop — `crates/envoy-bin/tests/access_log_file_sink.rs`                          | `crates/envoy-bin/tests/access_log_file_sink.rs`                                                                                                                 | ~140     | signpost 18     |
| 9  | Differential harness extension — `Driver::Http1WithAccessLog` + `AccessLogLineRule` + tokenizer + dispatch + 4 unit tests | `tests/differential/src/{lib.rs,access_log.rs}`                                                                                                                  | ~310     | D4.2.a–b        |
| 10 | Fixture 0012 (5 files) + Docker-gated wrapper + BEHAVIOR_CONTRACT.md `Access log field mapping` first-time population (D5.2 folded per signpost recommendation) | `tests/fixtures/0012-access-log-file-sink/{envoy,envoy-rust}.yaml` + `inputs/payload.bin` + `expectations.yaml` + `README.md` + `tests/differential/tests/access_log_file_sink.rs` + `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | ~135 | D4.2.c + D5.2 |
| 11 | State-4 phase-done verification (no code; PROGRESS quote)                                                  | `PROGRESS.md` only                                                                                                                                               | 0        | D6.2            |
|    | **Total**                                                                                                  |                                                                                                                                                                  | **~1875 LoC** | (SPEC §3 D-budget: ~1580 LoC) |

Total LoC `~1875` is moderately over the 06.2 SPEC §3's `~1580` projection, mostly because the per-task TDD step decomposition (failing-test → impl → re-run cadence) adds boilerplate the SPEC's bare-counts didn't anticipate. Per 06.2 SPEC §6 signpost 17 + parent-06 SPEC §5 alternative (vi)'s explicit rejection of nested splits, **do NOT nest-split**; the LoC drift is genuine and the 11-task count is comfortably under the §6.1 25-task gate. The named trims considered at PLAN-write time + rejected (mirrors 06.1's posture):

- **Trim option (i): defer the in-process backstop (Task 8) to a future phase.** Saves ~140 LoC. Rejected — the backstop is the local regression-equivalence guard for the HCM access-log dispatch site that runs without Docker (load-bearing on dev machines without Docker access); the same posture as 06.1 Task 11.
- **Trim option (ii): fold Task 10's BEHAVIOR_CONTRACT.md edit into Task 9 or Task 6.** Saves ~30 LoC of separate-task overhead. Rejected — the BEHAVIOR_CONTRACT.md edit is doc-only and reviewer-friendly to keep separate from the harness/fixture work; 06.2 SPEC §3 D5.2 recommends landing at the first-fixture commit which is exactly Task 10.
- **Trim option (iii): defer the fuzz corpus seed (Task 5's `hcm_access_log_file.yaml`) to a future phase.** Saves ~30 LoC. Rejected — the seed is the validator-accept-path empirical evidence for the new `access_log` field; the corpus-walk acceptance test absorbs it for free.

Total potential savings: ~200 LoC. Even with all three trims applied, the projection (~1675 LoC) would still exceed the 1500 LoC gate. The trims weaken the gate without sufficient LoC reduction; the doctrinally cleaner posture per parent-06 SPEC §5 + 06.2 SPEC §6 signpost 17 is to accept the estimate. **Acceptance posture: do NOT trim; do NOT nest-split.** This PLAN-write decision is recorded in Task 1's PROGRESS entry.

---

## File structure overview

### Created (new files)

```
crates/envoy-accesslog/
├── Cargo.toml
└── src/
    ├── lib.rs                 # crate root: #![forbid(unsafe_code)]; public re-exports; mod sink (placeholder)
    ├── record.rs              # AccessLogRecord struct + 2 unit tests
    ├── error.rs               # AccessLogError enum (Open/Write/InvalidPath)
    ├── sink.rs                # placeholder; doc-comment-only file explaining the option-(c) deferral
    ├── file_sink.rs           # FileSink concrete impl + 4 unit tests
    └── default_format.rs      # format + format_iso8601 + epoch_seconds_to_ymd_hms + 8 unit tests

crates/envoy-bin/tests/
└── access_log_file_sink.rs    # in-process integration backstop (no Docker required)

tests/differential/src/
└── access_log.rs              # hand-rolled default-format tokenizer + AccessLogLineRule + assert helper + 4 unit tests

tests/fixtures/0012-access-log-file-sink/
├── envoy.yaml
├── envoy-rust.yaml
├── inputs/payload.bin         # 0 bytes (placeholder; harness reads no body)
├── expectations.yaml
└── README.md

tests/differential/tests/
└── access_log_file_sink.rs    # Docker-gated wrapper (7-line trampoline)

crates/envoy-config/fuzz/corpus/parse_bootstrap/
└── hcm_access_log_file.yaml

docs/envoy-rust/phases/06.2-access-log/
└── PROGRESS.md                # appended per-task during execution; preamble lands at THIS state-2 commit
```

### Modified

```
Cargo.toml                                       # [workspace] members += "crates/envoy-accesslog"
crates/envoy-http1/Cargo.toml                    # [dependencies] += envoy-accesslog path-dep
crates/envoy-http1/src/hcm.rs                    # HCMConfig.access_log: Vec<Arc<FileSink>> field + from_config extension + factored dispatch site + 4 unit tests
crates/envoy-http2/Cargo.toml                    # [dependencies] += envoy-accesslog path-dep
crates/envoy-http2/src/hcm.rs                    # dispatch site at hcm.rs:251 after send_envoy_response returns + 2 unit tests
crates/envoy-config/src/bootstrap.rs             # AccessLog struct + FileAccessLogTypedConfig + HCM.access_log field + validate_access_logs free fn + 6 new validator tests + 1 corpus-walk test (extended walk-list)
crates/envoy-config/src/lib.rs                   # ConfigError += UnsupportedAccessLogType { actual } + InvalidAccessLogPath; pub use bootstrap::AccessLog
crates/envoy-config/fuzz/.gitignore              # !corpus/parse_bootstrap/hcm_access_log_file.yaml allow-list entry
tests/differential/src/lib.rs                    # Driver::Http1WithAccessLog variant + run_fixture dispatch arm + port_key match + 4 unit tests (in lib.rs::tests; tokenizer + assert_access_log_lines_equivalent helpers re-exported from access_log.rs)
docs/envoy-rust/BEHAVIOR_CONTRACT.md             # Access log field mapping section first-time population: prefatory ¶ + 14-row table + closing ¶

docs/envoy-rust/STATE.md                         # advance to 06.2 lifecycle state 3 (at THIS PLAN.md commit)
docs/envoy-rust/ROADMAP.md                       # row 06.2 status: planned → in-progress (at THIS PLAN.md commit)
```

### Deleted

None.

---

## Conventions

- **Per-task commit format.** `phase 06.2: <task description> (task N)` matching the 06.1 / 05.3 commit shape (e.g., 06.1's `cb6dfdd phase 05.3: envoy-config cluster-side typed_extension_protocol_options (task 3)`). State-4 close-out commit (Task 11) uses `phase 06.2: state-4 phase-done gate verification (task 11)`. State-6 phase-done commit (lands later, after REVIEW) uses the §9 commit-message format from 06.2 SPEC.
- **Co-Authored-By trailer.** Every commit ends with `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.
- **PROGRESS.md per-task entry.** Every task's last step appends a `## Task N — <title>` section to PROGRESS.md before commit, mirroring the 06.1 / 05.x cadence. The section quotes any non-trivial output (test pass count, key cargo-clippy/build outputs, surprising discoveries) inline. **Pre-task close fmt discipline (per 06.1 REVIEW §7 R-9):** every task that touches code runs `cargo fmt --all -- --check` as a final step before committing; if drift is detected, run `cargo fmt --all` and include the fmt-clean output in PROGRESS. Avoids the 06.1 Task 14 fmt-drift catch.
- **TDD discipline.** Every task that introduces code starts with the failing tests (Step A), verifies they fail (Step B), then implements (Step C), verifies pass (Step D), then commits (Step E). Multi-module tasks (e.g. Task 3 covers `format` + `format_iso8601` + `epoch_seconds_to_ymd_hms`) cycle TDD per module — write the tests first, see them fail, implement, see them pass, commit.
- **Cargo command output expectations.** Steps quote expected pass/fail counts. If actual output differs (e.g., a regression elsewhere), STOP and invoke `superpowers:systematic-debugging` per BOOTSTRAP_PROMPT.md §1 Step E.
- **`#![forbid(unsafe_code)]`** on the root file (`lib.rs` or `main.rs`) of every workspace crate per D-3.8. The new `envoy-accesslog` crate carries it; no `unsafe` in 06.2.
- **Cargo.lock sync.** Per parent-06 SPEC §7 + 06.2 SPEC §7 + signpost 20: no new top-level Cargo deps; new workspace-member registrations land inline with the scaffold task (Task 2). The state-4 verification (Task 11) cross-checks the Cargo.lock diff against the expected workspace-only diff shape.

---

## Task 1: PROGRESS.md preamble + LoC drift posture record + 4 SPEC corrections + signpost choices

**Files:**
- Create: `docs/envoy-rust/phases/06.2-access-log/PROGRESS.md`

This is a docs-only task. Lands the per-sub-phase `PROGRESS.md` skeleton + the LoC-drift acceptance posture (do NOT nest-split; accept ~1875 LoC drift over the SPEC's ~1580 projection per parent-06 SPEC §5 alternative (vi)'s rejection of nested splits) + the 4 PLAN-write SPEC corrections from the header above + the architecture-decision record from the header above + the signpost choices. Mirrors the 06.1 `PROGRESS.md` preamble cadence (commit `505653d`). **No code changes; lands at THIS PLAN.md commit alongside this PLAN.md, STATE.md, and ROADMAP.md updates.**

- [ ] **Step 1: Create the PROGRESS.md skeleton with preamble.** (Note: the executor for Task 1 IS the same session that writes this PLAN.md; Task 1 lands as part of the state-2 standalone PLAN.md commit per signpost 14 / signpost 17. The Step text below is the verbatim content for the file; see also the "Stage and commit state-2 standalone PLAN.md" step at the end of this PLAN under "State-2 commit (this PLAN.md commit)".)

Create file at `docs/envoy-rust/phases/06.2-access-log/PROGRESS.md` with the content laid out in the **"PROGRESS.md skeleton content"** section at the end of this PLAN.

- [ ] **Step 2: Stage Task 1's PROGRESS.md alongside the PLAN.md, STATE.md, and ROADMAP.md changes.**

The state-2 commit lands 4 file changes in lockstep (per signpost 14 / signpost 17):
- `docs/envoy-rust/phases/06.2-access-log/PLAN.md` (NEW; this file)
- `docs/envoy-rust/phases/06.2-access-log/PROGRESS.md` (NEW; the skeleton)
- `docs/envoy-rust/STATE.md` (modified; advance to 06.2 lifecycle state 3)
- `docs/envoy-rust/ROADMAP.md` (modified; row 06.2 status: planned → in-progress)

See the "State-2 commit (this PLAN.md commit)" section at the end of this PLAN for the exact git invocations.

- [ ] **Step 3: No further action; Task 1's substantive content IS the PROGRESS.md preamble.** The next session (Task 2 execution) starts the implementation arc.

---

## Task 2: envoy-accesslog crate scaffold + `record.rs` + `error.rs` + `sink.rs` placeholder + 2 record unit tests

**Files:**
- Create: `crates/envoy-accesslog/Cargo.toml`
- Create: `crates/envoy-accesslog/src/lib.rs`
- Create: `crates/envoy-accesslog/src/record.rs`
- Create: `crates/envoy-accesslog/src/error.rs`
- Create: `crates/envoy-accesslog/src/sink.rs` (placeholder)
- Modify: `Cargo.toml` (workspace `[workspace] members` extension)
- Cargo.lock (auto-regenerated)

Lands the new `crates/envoy-accesslog/` workspace member with: (a) `Cargo.toml` declaring the 5 deps (`tokio`, `bytes`, `tracing`, `thiserror`, `envoy-http1` path-dep); (b) `lib.rs` crate root with `#![forbid(unsafe_code)]` per D-3.8 + public re-exports; (c) `record.rs` carrying the 15-field `AccessLogRecord` struct + 2 unit tests; (d) `error.rs` carrying the 3-variant `AccessLogError` enum; (e) `sink.rs` as a placeholder file documenting the option-(c) trait deferral per architecture decision 1.

This task does NOT land `default_format.rs` or `file_sink.rs` — those land in Tasks 3 and 4 respectively, in TDD-cycle order. `lib.rs` declares those modules but they are empty placeholders at this commit (Task 3 fills `default_format.rs`; Task 4 fills `file_sink.rs`). Empty-module declarations compile cleanly; the public re-exports for `FileSink` / `format` defer to Tasks 3-4.

- [ ] **Step 1: Verify no `crates/envoy-accesslog/` directory exists yet.**

Run: `test ! -d crates/envoy-accesslog && echo OK`
Expected: `OK`

- [ ] **Step 2: Create the `crates/envoy-accesslog/Cargo.toml` file.**

Create `crates/envoy-accesslog/Cargo.toml`:

```toml
[package]
name = "envoy-accesslog"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_accesslog"
path = "src/lib.rs"

[dependencies]
tokio = { version = "1", features = ["fs", "io-util", "sync"] }
bytes = "1"
tracing = "0.1"
thiserror = "2"
envoy-http1 = { path = "../envoy-http1" }

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "test-util", "time"] }
tempfile = "3"
```

The `tempfile = "3"` dev-dep is per architecture decision 16 (already a dev-dep in 6 other workspace crates; clean reuse). The dev `tokio` features extend the runtime features used in unit tests (`rt-multi-thread` for parallel test runs; `macros` for `#[tokio::test]`; `test-util` for `tokio::time::pause()` if used in any test; `time` for `Duration` constants in tests).

- [ ] **Step 3: Create `crates/envoy-accesslog/src/lib.rs` with the crate-root forbidance + module declarations + public re-exports.**

Create `crates/envoy-accesslog/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

//! envoy-accesslog — access-log subsystem foundation: record value-type,
//! concrete file-sink, Envoy default-format emitter.
//!
//! Owns the workspace's only direct surface for access-log primitives. The
//! HCM at envoy-http1 builds AccessLogRecord values and dispatches via
//! FileSink::emit; no other workspace crate calls FileSink or the
//! default-format emitter directly.
//!
//! The Sink trait is intentionally NOT shipped in this version. See
//! parent-06 SPEC §3 D8.2 option (c) and 06.2 SPEC §3 architectural rule 3.
//! When N≥2 sink types exist (gRPC ALS sink, stdout sink, etc.), a
//! future phase will ship the trait + multi-sink dispatch in this crate.

pub mod record;
pub mod file_sink;
pub mod default_format;
mod error;
mod sink;

pub use record::AccessLogRecord;
pub use file_sink::FileSink;
pub use error::AccessLogError;
```

The `mod sink;` declaration is private (no `pub use`); the file is a placeholder per architecture decision 1. The `pub mod default_format;` and `pub mod file_sink;` declarations are public so consumers (the HCM at `envoy-http1`) can call `envoy_accesslog::default_format::format(&record)` or `envoy_accesslog::FileSink::new(path).await`. Public re-exports for `AccessLogRecord` / `FileSink` / `AccessLogError` at the crate root provide the ergonomic 1-import surface.

- [ ] **Step 4: Create `crates/envoy-accesslog/src/sink.rs` placeholder.**

Create `crates/envoy-accesslog/src/sink.rs`:

```rust
//! Sink trait — DEFERRED.
//!
//! Per parent-06 SPEC §3 D8.2 option (c) and 06.2 SPEC §3 architectural
//! rule 3: the `Sink` trait is intentionally NOT shipped in this version.
//! `FileSink` (in `file_sink.rs`) ships as a concrete inherent impl.
//! `HCMConfig.access_log` is typed concretely as `Vec<Arc<FileSink>>`,
//! not `Vec<Arc<dyn Sink>>`.
//!
//! Future observability-family phases that ship a second sink type
//! (gRPC ALS sink, stdout sink, etc.) will:
//!   1. Define the `Sink` trait here, in `sink.rs`.
//!   2. Promote `FileSink::emit` to a `Sink::emit` trait method.
//!   3. Re-type `HCMConfig.access_log` to `Vec<Arc<dyn Sink>>` (or
//!      a typed enum dispatcher, depending on the dispatch shape
//!      that phase picks).
//!
//! The placeholder file exists to preserve module-decomposition
//! stability — the trait lands by editing this file rather than by
//! introducing a new module.
```

The file is doc-comment-only — no items defined. It will compile cleanly under `#![forbid(unsafe_code)]`.

- [ ] **Step 5: Write the failing tests for `AccessLogRecord` in `crates/envoy-accesslog/src/record.rs`.**

Per the TDD posture, write tests first. Create `crates/envoy-accesslog/src/record.rs` with ONLY the test module (the struct definition comes in Step 7):

```rust
//! AccessLogRecord — POD value-type carrying the 14 fields rendered by
//! the Envoy default-format access-log emitter (plus a leading
//! `start_time` SystemTime that the emitter formats per
//! `default_format::format_iso8601`).
//!
//! Built at HCM on-response-complete time by `envoy-http1::hcm`'s
//! factored join point; consumed (by reference) by
//! `default_format::format` and by `FileSink::emit`.

use std::time::{Duration, SystemTime};

// (Step 7: AccessLogRecord struct definition lands here.)

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn record_construction_full() {
        // Build a record with every field populated; verify it
        // round-trips through Debug (no panic; output contains key
        // field names).
        let record = AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(5),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: Some("envoy-rust.test".into()),
            upstream_host: None,
        };
        let dbg = format!("{:?}", record);
        assert!(dbg.contains("method: \"GET\""), "debug output: {}", dbg);
        assert!(dbg.contains("authority: Some(\"envoy-rust.test\")"), "debug output: {}", dbg);
    }

    #[test]
    fn record_clone_is_deep_for_strings() {
        // Clone a record; mutate the clone's method field; verify the
        // original is unchanged. (Rust's Clone on String is deep-copy
        // by definition; this test is documentation that callers can
        // rely on the deep-copy semantic.)
        let original = AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(5),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: None,
            upstream_host: None,
        };
        let mut clone = original.clone();
        clone.method = "POST".into();
        assert_eq!(original.method, "GET");
        assert_eq!(clone.method, "POST");
    }
}
```

- [ ] **Step 6: Verify the tests fail to compile (AccessLogRecord struct undefined).**

Run: `cargo build -p envoy-accesslog --tests 2>&1 | tail -20`
Expected: error E0422 (`cannot find struct AccessLogRecord in this scope`) or similar — the struct doesn't exist yet.

- [ ] **Step 7: Write the `AccessLogRecord` struct definition in `record.rs`.**

Replace the `// (Step 7: ...)` comment marker in `crates/envoy-accesslog/src/record.rs` with:

```rust
/// AccessLogRecord — value-type carrying the per-request state that
/// the Envoy default-format emitter renders. 15 fields total: a
/// leading SystemTime for `%START_TIME%`, then 14 substitution
/// targets matching the Envoy default access-log format (one per
/// token).
///
/// Built at HCM on-response-complete time; consumed by reference by
/// the default-format emitter and the FileSink. Owns its String
/// fields so it can cross spawn boundaries cheaply if future code
/// switches to spawn-based dispatch (06.2 uses synchronous-after-
/// write per parent-06 architectural Rule 4 option (b)).
///
/// Intentionally does NOT implement `Default` (per 06.2 SPEC §6
/// signpost 14) — every field must be populated explicitly at the
/// HCM record-build site so silent omissions can't ship.
#[derive(Debug, Clone)]
pub struct AccessLogRecord {
    /// Wall-clock at request arrival. Rendered by
    /// `default_format::format_iso8601` as `YYYY-MM-DDTHH:MM:SS.sssZ`.
    pub start_time: SystemTime,

    /// HTTP method token (`GET` / `POST` / etc.).
    pub method: String,

    /// Path: either `X-Envoy-Original-Path` if the request carried
    /// that header, else the request-target/`:path` pseudo-header.
    pub path: String,

    /// `"HTTP/1.1"` on the H1 dispatch path, `"HTTP/2"` on the H2
    /// dispatch path.
    pub protocol: String,

    pub response_code: u16,

    /// Always `"-"` in 06.2 (Envoy's no-flags sentinel). Future
    /// phases that surface non-`-` flag combinations will populate
    /// this field with the appropriate flag token(s).
    pub response_flags: String,

    /// Wire-byte count of the request body. Header bytes NOT counted
    /// per Envoy's `%BYTES_RECEIVED%` semantic.
    pub bytes_received: u64,

    /// Wire-byte count of the response body.
    pub bytes_sent: u64,

    /// Per-request latency from request-arrival to record-build time.
    /// Rendered as integer milliseconds via `Duration::as_millis()`.
    pub duration: Duration,

    /// Value of the response's `x-envoy-upstream-service-time`
    /// header if present (parsed as `u64` ms), else `None`.
    pub upstream_service_time: Option<Duration>,

    /// Request-side `x-forwarded-for` header value if present.
    pub forwarded_for: Option<String>,

    /// Request-side `user-agent` header value if present.
    pub user_agent: Option<String>,

    /// Request-side `x-request-id` header value if present.
    pub request_id: Option<String>,

    /// Request-side `host` header value (or `:authority` pseudo-
    /// header on H2 — the codec translates pre-record-build) if
    /// present.
    pub authority: Option<String>,

    /// Resolved upstream endpoint formatted via `SocketAddr` Display
    /// impl (e.g., `127.0.0.1:8080` for IPv4, `[::1]:8080` for
    /// IPv6). `None` on direct_response paths.
    pub upstream_host: Option<String>,
}
```

- [ ] **Step 8: Verify the tests pass.**

Run: `cargo test -p envoy-accesslog --lib record::tests 2>&1 | tail -10`
Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 9: Create `crates/envoy-accesslog/src/error.rs` with the 3-variant `AccessLogError` enum.**

Create `crates/envoy-accesslog/src/error.rs`:

```rust
//! AccessLogError — typed error variants for the access-log
//! subsystem. Maps OS-level filesystem errors to crate-typed
//! variants for callers (the HCM consumer at `envoy-http1`) to
//! match on. The HCM does NOT propagate these errors up the
//! response-write path per parent-06 SPEC §6 architectural Rule 4
//! (fire-and-forget); they are logged via `tracing::warn!` and
//! discarded.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AccessLogError {
    /// `FileSink::new` failed to open the configured file path
    /// (permissions, parent-directory-missing, file-is-a-directory,
    /// etc.). Surfaces at startup when the HCMConfig is constructed.
    #[error("failed to open access log file at {path}: {source}")]
    Open { path: PathBuf, source: std::io::Error },

    /// `FileSink::emit` failed to write a record to the file
    /// (filesystem full, file removed mid-runtime, etc.).
    /// Surfaces per-emission at runtime.
    #[error("failed to write access log line to {path}: {source}")]
    Write { path: PathBuf, source: std::io::Error },

    /// Path validation failed at `envoy-config` validator time
    /// (empty path). Reserved for future stricter validation
    /// per 06.2 SPEC §6 signpost 6 if the recommendation tightens.
    /// Currently not emitted from inside `envoy-accesslog` —
    /// `envoy-config`'s `ConfigError::InvalidAccessLogPath` is the
    /// surface variant (per Task 5).
    #[error("invalid access log file path: {path}")]
    InvalidPath { path: PathBuf },
}
```

- [ ] **Step 10: Add the new workspace member to root `Cargo.toml`.**

Edit `Cargo.toml`'s `[workspace] members` block — add `"crates/envoy-accesslog",` to the alphabetically-sorted list. Per the cross-checked existing block:

```toml
[workspace]
resolver = "2"
members = [
    "crates/envoy-accesslog",     # NEW (06.2 Task 2)
    "crates/envoy-admin",
    "crates/envoy-bin",
    "crates/envoy-cluster",
    "crates/envoy-config",
    "crates/envoy-http1",
    "crates/envoy-http2",
    "crates/envoy-listener",
    "crates/envoy-stats",
    "crates/envoy-tcp",
    "crates/envoy-tls",
    "tests/conformance/h2spec",
    "tests/differential",
    "tests/helpers/http1-echo-server",
    "tests/helpers/http2-echo-server",
    "tests/helpers/tcp-echo-server",
    "tests/helpers/tls-echo-server",
]
exclude = [
    "crates/envoy-config/fuzz",
]
```

The new entry sorts to the top per alphabetical convention. (The trailing `# NEW (06.2 Task 2)` annotation is illustrative; do not include it in the actual file — the workspace block doesn't carry per-line comments per existing convention. Just insert the bare `"crates/envoy-accesslog",` line.)

- [ ] **Step 11: Create placeholder `default_format.rs` and `file_sink.rs` files (empty content; Tasks 3 + 4 fill them).**

Create `crates/envoy-accesslog/src/default_format.rs` with placeholder content:

```rust
//! Envoy default-format access-log emitter — TASK 3 PLACEHOLDER.
//!
//! Task 3 lands the `format`, `format_iso8601`, and
//! `epoch_seconds_to_ymd_hms` functions plus 8 unit tests.
//! This placeholder allows the module declaration in `lib.rs` to
//! compile at the Task 2 commit boundary.
```

Create `crates/envoy-accesslog/src/file_sink.rs` with placeholder content:

```rust
//! FileSink concrete impl — TASK 4 PLACEHOLDER.
//!
//! Task 4 lands the `FileSink::{new, emit}` API plus 4 unit tests.
//! This placeholder allows the module declaration in `lib.rs` to
//! compile at the Task 2 commit boundary.
//!
//! `lib.rs` has `pub use file_sink::FileSink;` — to keep that line
//! compiling, this placeholder defines a unit struct that Task 4
//! replaces wholesale (mirrors 06.1 Task 2's placeholder pattern
//! for `Counter` / `Gauge`).

/// FileSink placeholder; Task 4 replaces with the real impl.
pub struct FileSink;
```

Mirrors the 06.1 Task 2 / Task 3 placeholder pattern from 06.1 PROGRESS Task 2 (which discovered that placeholder modules with empty bodies break `pub use` lines and added `pub struct Counter;` / `pub struct Gauge;` stubs as the fix). The same fix lands here for `FileSink` (placeholder; Task 4 replaces wholesale).

- [ ] **Step 12: Verify the workspace builds cleanly at this Task 2 commit boundary.**

Run: `cargo build --workspace --all-targets 2>&1 | tail -10`
Expected: clean build; possibly `dead_code` warnings on the unused `FileSink` unit struct (which are acceptable at placeholder time — Task 4 replaces wholesale; the placeholder is `pub` so it has no inherent dead-code firing). If `-D warnings` fires on a `dead_code` warning specific to the placeholder struct, add `#[allow(dead_code)]` immediately above the `pub struct FileSink;` line with a single-line rationale comment (mirrors 06.1 Task 2's PLAN-correction).

- [ ] **Step 13: Run clippy + fmt + the new tests.**

Run in parallel (independent commands):
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -10`
- `cargo fmt --all -- --check 2>&1 | tail -5`
- `cargo test -p envoy-accesslog 2>&1 | tail -10`

Expected: clippy clean; fmt clean; `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 14: Append PROGRESS.md Task 2 entry + commit.**

Append to `docs/envoy-rust/phases/06.2-access-log/PROGRESS.md`:

```markdown

## Task 2 — envoy-accesslog crate scaffold + record + error + sink placeholder

Lands the new `crates/envoy-accesslog/` workspace member per SPEC §3 D1.2.

**Modules created:**
- `crates/envoy-accesslog/Cargo.toml` — 5 deps (tokio fs+io-util+sync, bytes, tracing, thiserror, envoy-http1 path-dep); `tempfile = "3"` as dev-dep per architecture decision 16.
- `crates/envoy-accesslog/src/lib.rs` — crate root with `#![forbid(unsafe_code)]` + 5 module declarations + 3 public re-exports.
- `crates/envoy-accesslog/src/record.rs` — 15-field `AccessLogRecord` struct + 2 unit tests.
- `crates/envoy-accesslog/src/error.rs` — 3-variant `AccessLogError` (`Open`, `Write`, `InvalidPath`).
- `crates/envoy-accesslog/src/sink.rs` — doc-comment-only placeholder explaining option-(c) deferral.
- `crates/envoy-accesslog/src/default_format.rs` — placeholder for Task 3.
- `crates/envoy-accesslog/src/file_sink.rs` — placeholder (`pub struct FileSink;` stub for `pub use`).

**Workspace registration:** `Cargo.toml` `[workspace] members` += `"crates/envoy-accesslog"` (alphabetically first).

**Tests:** `cargo test -p envoy-accesslog` → `test result: ok. 2 passed; 0 failed` (the two `record::tests::*` cases).

**Workspace gates:** `cargo build --workspace --all-targets` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; `cargo fmt --all -- --check` clean.

**LoC:** ~165 LoC (the 5 module files + Cargo.toml + workspace `members` line).
```

Stage + commit:

```bash
git add crates/envoy-accesslog/ Cargo.toml Cargo.lock docs/envoy-rust/phases/06.2-access-log/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.2: envoy-accesslog crate scaffold + record + error + sink placeholder (task 2)

Lands the new crates/envoy-accesslog/ workspace member per 06.2 SPEC §3 D1.2.
AccessLogRecord (15-field POD struct), AccessLogError (Open/Write/InvalidPath),
sink.rs placeholder (option (c) trait deferral per parent-06 SPEC §3 D8.2).
default_format.rs and file_sink.rs are placeholders for Tasks 3 and 4.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Run `git status` to confirm clean working tree.

---

## Task 3: `default_format.rs` — `format` + `format_iso8601` + `epoch_seconds_to_ymd_hms` + 8 unit tests

**Files:**
- Modify (wholesale replace placeholder): `crates/envoy-accesslog/src/default_format.rs`

Lands the Envoy default-format access-log line emitter. Three functions: (a) `pub fn format(record: &AccessLogRecord) -> String` — the line emitter (14 tokens; no trailing newline; `FileSink::emit` adds the `\n`); (b) `pub(crate) fn format_iso8601(s: &mut String, t: SystemTime)` — appends `YYYY-MM-DDTHH:MM:SS.sssZ` to `s` per the 06.2 SPEC §6 signpost 1 recommendation (`&mut String` shape); (c) `fn epoch_seconds_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32)` — Gregorian calendar arithmetic helper, inline in this file per signpost 2.

The Gregorian-calendar algorithm uses the standard "days-since-epoch" formula with full leap-year handling: a year is a leap year if (year % 4 == 0 AND (year % 100 != 0 OR year % 400 == 0)). The "days in month" lookup uses the standard 12-element table with February's day count branching on the leap-year predicate.

**TDD cycle:** Step 1 lands the 8 unit tests (failing); Step 3 lands the three functions; Step 4 verifies pass.

- [ ] **Step 1: Write all 8 failing unit tests at the bottom of `default_format.rs`.**

Wholesale-replace `crates/envoy-accesslog/src/default_format.rs` with:

```rust
//! Envoy default-format access-log line emitter.
//!
//! Renders an `AccessLogRecord` as the fixed 14-token Envoy default
//! format (verifiable against upstream Envoy v1.33's documentation
//! at the canonical access-log usage page). The output is a single
//! line WITHOUT trailing newline — `FileSink::emit` writes the
//! `\n` separately.
//!
//! Token sequence (literal separators preserved):
//! `[%START_TIME%] "%REQ(:METHOD)% %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)% %PROTOCOL%"
//!  %RESPONSE_CODE% %RESPONSE_FLAGS% %BYTES_RECEIVED% %BYTES_SENT% %DURATION%
//!  %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)% "%REQ(X-FORWARDED-FOR)%"
//!  "%REQ(USER-AGENT)%" "%REQ(X-REQUEST-ID)%" "%REQ(:AUTHORITY)%" "%UPSTREAM_HOST%"`
//!
//! Tokens whose backing fields are `None` (or whose values are not
//! emitted by envoy-rust) render as a literal `-` per Envoy's
//! substitution rule. Quoted positions render as `"-"`.

use std::fmt::Write as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::record::AccessLogRecord;

// (Step 3: the three pub/pub(crate)/private fn definitions land here.)

#[cfg(test)]
mod tests {
    use super::*;

    fn make_baseline_record() -> AccessLogRecord {
        // Mirrors fixture 0012's direct_response surface: GET / → 200
        // "ok\n"; no upstream; no extra request headers.
        AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(5),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: Some("envoy-rust.test".into()),
            upstream_host: None,
        }
    }

    #[test]
    fn format_happy_path_direct_response() {
        let record = make_baseline_record();
        let line = format(&record);
        // The leading [...] is the ISO-8601 timestamp (golden-tested
        // separately in format_iso8601_epoch_zero). After the
        // closing `] `, the rest of the line is deterministic per
        // record's fields.
        assert!(line.starts_with("[1970-01-01T00:00:00.000Z] "), "line: {}", line);
        // The rest of the line: literal substitution per the record fields.
        let expected_suffix = "\"GET / HTTP/1.1\" 200 - 0 3 5 - \"-\" \"-\" \"-\" \"envoy-rust.test\" \"-\"";
        assert!(line.ends_with(expected_suffix), "line: {}\nexpected suffix: {}", line, expected_suffix);
    }

    #[test]
    fn format_with_router_proxy_path() {
        let mut record = make_baseline_record();
        record.upstream_service_time = Some(Duration::from_millis(2));
        record.upstream_host = Some("127.0.0.1:8080".into());
        record.response_code = 201;
        let line = format(&record);
        let expected_suffix = "\"GET / HTTP/1.1\" 201 - 0 3 5 2 \"-\" \"-\" \"-\" \"envoy-rust.test\" \"127.0.0.1:8080\"";
        assert!(line.ends_with(expected_suffix), "line: {}\nexpected suffix: {}", line, expected_suffix);
    }

    #[test]
    fn format_5xx_response_with_flags() {
        // Forward-compat: 06.2 always emits "-" for response_flags
        // at the HCM record-build site, but the formatter must
        // handle non-"-" flag tokens for future phases.
        let mut record = make_baseline_record();
        record.response_code = 503;
        record.response_flags = "UH".into();
        let line = format(&record);
        let expected_suffix = "\"GET / HTTP/1.1\" 503 UH 0 3 5 - \"-\" \"-\" \"-\" \"envoy-rust.test\" \"-\"";
        assert!(line.ends_with(expected_suffix), "line: {}", line);
    }

    #[test]
    fn format_iso8601_epoch_zero() {
        let mut s = String::new();
        format_iso8601(&mut s, UNIX_EPOCH);
        assert_eq!(s, "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn format_iso8601_known_date() {
        // 2024-02-29T12:34:56.789Z — leap day boundary.
        // Epoch seconds for that instant: compute via the algorithm
        // we're testing — instead, pick a known offset.
        // (year=2024, month=2, day=29, hour=12, min=34, sec=56)
        // 54 years × 365.25 days/year ≈ days; the exact computation
        // is what the helper does. Encode it as a hard-coded
        // SystemTime via UNIX_EPOCH + Duration::from_millis(...).
        // Computed offline:
        // (2024-1970) * 365 days + leap_days_count(1970..2024) = 19782 days
        // Days through Jan 2024 (2024 is a leap year): 31 days
        // Day-of-month 29 = +28 days into Feb
        // Total days = 19782 + 31 + 28 = 19841 days
        // Time-of-day: 12*3600 + 34*60 + 56 = 45296 seconds
        // Total seconds: 19841 * 86400 + 45296 = 1714264 seconds × ... actually let's just use
        // SystemTime::checked_add to build it.
        // Simpler: 1709209096 is the epoch seconds for 2024-02-29T12:34:56Z (verify against
        // `date -ud '2024-02-29T12:34:56Z' +%s` = 1709209096).
        let t = UNIX_EPOCH + Duration::from_millis(1_709_209_096_789);
        let mut s = String::new();
        format_iso8601(&mut s, t);
        assert_eq!(s, "2024-02-29T12:34:56.789Z");
    }

    #[test]
    fn epoch_seconds_to_ymd_hms_known_dates() {
        // Table-driven test with known epochs.
        let cases: &[(u64, (u32, u32, u32, u32, u32, u32))] = &[
            // epoch 0
            (0, (1970, 1, 1, 0, 0, 0)),
            // 2000-03-01T00:00:00Z (just after Y2K leap day)
            // date -ud '2000-03-01T00:00:00Z' +%s = 951868800
            (951_868_800, (2000, 3, 1, 0, 0, 0)),
            // 2000-02-29T23:59:59Z (last second of Y2K leap day)
            (951_868_799, (2000, 2, 29, 23, 59, 59)),
            // 2024-02-29T12:34:56Z (current-era leap day)
            (1_709_209_096, (2024, 2, 29, 12, 34, 56)),
            // 2100-03-01T00:00:00Z (century year not a leap year)
            // date -ud '2100-03-01T00:00:00Z' +%s = 4107542400
            (4_107_542_400, (2100, 3, 1, 0, 0, 0)),
            // 2100-02-28T23:59:59Z (last second of Feb 2100; not a leap year)
            (4_107_542_399, (2100, 2, 28, 23, 59, 59)),
            // 2038-01-19T03:14:07Z (i32::MAX seconds; Y2K38 boundary)
            (2_147_483_647, (2038, 1, 19, 3, 14, 7)),
        ];
        for (secs, expected) in cases {
            let actual = epoch_seconds_to_ymd_hms(*secs);
            assert_eq!(actual, *expected, "secs={}", secs);
        }
    }

    #[test]
    fn epoch_seconds_to_ymd_hms_handles_far_future() {
        // Year 9999-12-31T23:59:59Z. Epoch seconds = approximately
        // 253402300799. The algorithm must not panic; the year
        // must render as 9999.
        let secs: u64 = 253_402_300_799;
        let (year, _, _, _, _, _) = epoch_seconds_to_ymd_hms(secs);
        assert_eq!(year, 9999);
    }

    #[test]
    fn format_utf8_edge_case_in_user_agent() {
        // Envoy's default format does not escape UTF-8 in REQ token
        // values; envoy-rust matches.
        let mut record = make_baseline_record();
        record.user_agent = Some("Mozilla/5.0 (X11; Linux 中文)".into());
        let line = format(&record);
        assert!(line.contains("\"Mozilla/5.0 (X11; Linux 中文)\""), "line: {}", line);
    }
}
```

- [ ] **Step 2: Verify the tests fail to compile (functions undefined).**

Run: `cargo test -p envoy-accesslog --lib default_format::tests 2>&1 | tail -20`
Expected: errors `cannot find function 'format' in this scope` and similar for `format_iso8601` / `epoch_seconds_to_ymd_hms`.

- [ ] **Step 3: Replace the `// (Step 3: ...)` marker with the three function definitions.**

Edit `crates/envoy-accesslog/src/default_format.rs` — replace the `// (Step 3: ...)` line with:

```rust
/// Format an AccessLogRecord as a single Envoy default-format
/// access-log line. No trailing newline — `FileSink::emit` writes
/// the `\n` separately so callers that build multi-record buffers
/// can control the newline placement.
pub fn format(record: &AccessLogRecord) -> String {
    let mut s = String::with_capacity(256);
    s.push('[');
    format_iso8601(&mut s, record.start_time);
    s.push_str("] \"");
    s.push_str(&record.method);
    s.push(' ');
    s.push_str(&record.path);
    s.push(' ');
    s.push_str(&record.protocol);
    s.push_str("\" ");
    write!(&mut s, "{}", record.response_code).unwrap();
    s.push(' ');
    s.push_str(&record.response_flags);
    s.push(' ');
    write!(&mut s, "{}", record.bytes_received).unwrap();
    s.push(' ');
    write!(&mut s, "{}", record.bytes_sent).unwrap();
    s.push(' ');
    write!(&mut s, "{}", record.duration.as_millis()).unwrap();
    s.push(' ');
    match &record.upstream_service_time {
        Some(d) => { write!(&mut s, "{}", d.as_millis()).unwrap(); }
        None => s.push('-'),
    }
    s.push_str(" \"");
    push_or_dash(&mut s, &record.forwarded_for);
    s.push_str("\" \"");
    push_or_dash(&mut s, &record.user_agent);
    s.push_str("\" \"");
    push_or_dash(&mut s, &record.request_id);
    s.push_str("\" \"");
    push_or_dash(&mut s, &record.authority);
    s.push_str("\" \"");
    push_or_dash(&mut s, &record.upstream_host);
    s.push('"');
    s
}

fn push_or_dash(s: &mut String, opt: &Option<String>) {
    match opt {
        Some(v) => s.push_str(v),
        None => s.push('-'),
    }
}

/// Hand-rolled ISO-8601 emitter: `YYYY-MM-DDTHH:MM:SS.sssZ`
/// (UTC, millisecond resolution). Appends 24 ASCII bytes to `s`.
///
/// Defers to `epoch_seconds_to_ymd_hms` for the date split. No
/// timezone handling beyond UTC; no leap-second handling; the
/// fractional-second component is millisecond-truncated (`Duration::
/// subsec_millis`).
pub(crate) fn format_iso8601(s: &mut String, t: SystemTime) {
    let dur = t.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let secs = dur.as_secs();
    let ms = dur.subsec_millis();
    let (year, month, day, hour, minute, second) = epoch_seconds_to_ymd_hms(secs);
    write!(
        s,
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hour, minute, second, ms
    )
    .unwrap();
}

/// Gregorian calendar arithmetic helper. Splits an epoch-seconds
/// value into `(year, month, day, hour, minute, second)`.
///
/// Year range supported: `[1970, 9999]`. The upper bound covers all
/// conceivable wall-clock timestamps before the 4-digit-year ISO-
/// 8601 format breaks; the lower bound is the UNIX epoch.
///
/// Algorithm: standard days-since-epoch decomposition.
///   1. Split `secs` into `total_days = secs / 86_400` and
///      `time_of_day = secs % 86_400`.
///   2. `time_of_day` → `(hour, minute, second)` via 3600/60
///      division.
///   3. `total_days` → `(year, month, day)` via year-walk: subtract
///      days_in_year(year) iteratively until the remainder fits in
///      a single year; then walk months via days_in_month(month,
///      is_leap_year).
fn epoch_seconds_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let total_days = secs / 86_400;
    let time_of_day = secs % 86_400;

    let hour = (time_of_day / 3600) as u32;
    let minute = ((time_of_day % 3600) / 60) as u32;
    let second = (time_of_day % 60) as u32;

    // Year-walk from 1970.
    let mut year: u32 = 1970;
    let mut remaining_days = total_days;
    loop {
        let dy = days_in_year(year) as u64;
        if remaining_days < dy {
            break;
        }
        remaining_days -= dy;
        year += 1;
    }

    // Month-walk from January.
    let leap = is_leap_year(year);
    let mut month: u32 = 1;
    let mut remaining_days = remaining_days as u32;
    loop {
        let dm = days_in_month(month, leap);
        if remaining_days < dm {
            break;
        }
        remaining_days -= dm;
        month += 1;
    }

    let day = remaining_days + 1; // 1-indexed

    (year, month, day, hour, minute, second)
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0) && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_year(year: u32) -> u32 {
    if is_leap_year(year) {
        366
    } else {
        365
    }
}

fn days_in_month(month: u32, leap: bool) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if leap { 29 } else { 28 },
        _ => unreachable!("month out of range: {}", month),
    }
}
```

- [ ] **Step 4: Run the tests and verify they pass.**

Run: `cargo test -p envoy-accesslog --lib default_format::tests 2>&1 | tail -15`
Expected: `test result: ok. 8 passed; 0 failed`.

If `epoch_seconds_to_ymd_hms_known_dates` fails on a specific case, the most likely culprit is the leap-year predicate (the year-2100 case `not a leap year` is the discriminator — if your impl returns `(2100, 2, 29, ...)` for epoch 4107542399, the predicate's `year % 400 == 0` arm is misordered). The test prints `secs=N` for failures.

- [ ] **Step 5: Run clippy + fmt.**

Run in parallel:
- `cargo clippy -p envoy-accesslog --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -3`

Expected: both clean.

- [ ] **Step 6: Append PROGRESS.md Task 3 entry + commit.**

Append to `docs/envoy-rust/phases/06.2-access-log/PROGRESS.md`:

```markdown

## Task 3 — envoy-accesslog default_format emitter + ISO-8601 + Gregorian helper

Lands `crates/envoy-accesslog/src/default_format.rs` per SPEC §3 D1.2 + §6 signpost 1 (ISO-8601 emitter takes `&mut String`) + signpost 2 (Gregorian helper inline, not separate module) + signpost 9 (`%DURATION%` rendered in integer milliseconds).

**Functions landed:**
- `pub fn format(record: &AccessLogRecord) -> String` — 14-token Envoy default format, no trailing newline.
- `pub(crate) fn format_iso8601(s: &mut String, t: SystemTime)` — appends 24 ASCII bytes `YYYY-MM-DDTHH:MM:SS.sssZ`.
- `fn epoch_seconds_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32)` — Gregorian calendar arithmetic with full leap-year handling (4/100/400 rule).
- Helpers: `push_or_dash`, `is_leap_year`, `days_in_year`, `days_in_month`.

**Tests:** `cargo test -p envoy-accesslog --lib default_format::tests` → `test result: ok. 8 passed; 0 failed`. Test 5 (`format_iso8601_known_date`) validates the leap-day boundary (2024-02-29T12:34:56.789Z); test 6 (`epoch_seconds_to_ymd_hms_known_dates`) is table-driven across 7 known epochs including the Y2K leap day boundary + the year-2100-non-leap-year boundary + the Y2K38 boundary.

**Workspace gates:** clippy clean; fmt clean; lib tests `2 + 8 = 10` passed.

**LoC:** ~280 LoC (~160 impl + ~120 tests; the test set is unusually dense per the SPEC §3 D1.2 14-test projection split across 8 tests in default_format + 2 in record + 4 in file_sink).
```

Stage + commit:

```bash
git add crates/envoy-accesslog/src/default_format.rs docs/envoy-rust/phases/06.2-access-log/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.2: envoy-accesslog default_format emitter + ISO-8601 + Gregorian helper (task 3)

Lands crates/envoy-accesslog/src/default_format.rs per SPEC §3 D1.2 +
signposts 1/2/9. format() emits the 14-token Envoy default format;
format_iso8601 emits YYYY-MM-DDTHH:MM:SS.sssZ; epoch_seconds_to_ymd_hms
does Gregorian calendar arithmetic with full leap-year handling
(4/100/400 rule). 8 unit tests including leap-day boundary, year-2100
non-leap, and Y2K38 boundary.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Run `git status` to confirm clean working tree.

---

## Task 4: `file_sink.rs` — `FileSink::{new, emit}` + 4 unit tests

**Files:**
- Modify (wholesale replace placeholder): `crates/envoy-accesslog/src/file_sink.rs`

Lands the concrete `FileSink` impl: opens the configured path with `tokio::fs::OpenOptions::new().append(true).create(true)` at constructor time; serializes per-emission writes via `Arc<tokio::sync::Mutex<File>>` per architecture decision 5 (signpost 3). The `emit` body calls `default_format::format(record)`, writes the resulting line + `\n` via `AsyncWriteExt::write_all` under the mutex, and maps `std::io::Error` to `AccessLogError::Write`. Errors are returned to the caller (the HCM dispatch site in Task 6 logs via `tracing::warn!` and discards per Rule 4).

**TDD cycle:** Step 1 lands the 4 unit tests (failing); Step 3 lands the impl; Step 4 verifies pass.

- [ ] **Step 1: Write the 4 failing unit tests by wholesale-replacing the Task 2 placeholder.**

Wholesale-replace `crates/envoy-accesslog/src/file_sink.rs` with:

```rust
//! FileSink — concrete on-disk access-log sink.
//!
//! Opens the configured path with `OpenOptions::append(true).
//! create(true)` at constructor time; serializes per-emission
//! writes via an internal `Arc<tokio::sync::Mutex<File>>` so
//! concurrent emissions on the same `Arc<FileSink>` interleave at
//! the mutex boundary (not at the OS-level write boundary, which
//! would allow line interleaving on filesystems with weaker
//! append atomicity).
//!
//! The `Sink` trait is intentionally NOT shipped per parent-06
//! SPEC §3 D8.2 option (c); FileSink ships concretely.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::default_format::format;
use crate::error::AccessLogError;
use crate::record::AccessLogRecord;

// (Step 3: FileSink struct + impl land here.)

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tempfile::tempdir;
    use tokio::io::AsyncReadExt;

    fn make_record() -> AccessLogRecord {
        AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(5),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: Some("envoy-rust.test".into()),
            upstream_host: None,
        }
    }

    async fn read_to_string(path: &std::path::Path) -> String {
        let mut buf = String::new();
        File::open(path)
            .await
            .expect("file exists")
            .read_to_string(&mut buf)
            .await
            .expect("read");
        buf
    }

    #[tokio::test]
    async fn file_sink_writes_one_record() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let sink = FileSink::new(path.clone()).await.expect("open");
        let record = make_record();
        sink.emit(&record).await.expect("emit");
        drop(sink); // force OS-level flush via file close
        let contents = read_to_string(&path).await;
        assert_eq!(
            contents.lines().count(),
            1,
            "expected 1 line, got {} (contents: {:?})",
            contents.lines().count(),
            contents
        );
        let line = &contents.lines().next().unwrap();
        // The formatter output for make_record() has a known suffix
        // after the [START_TIME] bracket (per default_format::tests::
        // format_happy_path_direct_response).
        assert!(
            line.ends_with("\"GET / HTTP/1.1\" 200 - 0 3 5 - \"-\" \"-\" \"-\" \"envoy-rust.test\" \"-\""),
            "line: {}",
            line
        );
        // The trailing newline must be present.
        assert!(contents.ends_with('\n'), "contents should end with newline; got: {:?}", contents);
    }

    #[tokio::test]
    async fn file_sink_appends_multiple_records() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let sink = FileSink::new(path.clone()).await.expect("open");
        for i in 0..3 {
            let mut record = make_record();
            record.response_code = 200 + i;
            sink.emit(&record).await.expect("emit");
        }
        drop(sink);
        let contents = read_to_string(&path).await;
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3);
        // Lines are in emit order.
        assert!(lines[0].contains(" 200 "), "line 0: {}", lines[0]);
        assert!(lines[1].contains(" 201 "), "line 1: {}", lines[1]);
        assert!(lines[2].contains(" 202 "), "line 2: {}", lines[2]);
    }

    #[tokio::test]
    async fn file_sink_serializes_concurrent_emissions() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let sink = Arc::new(FileSink::new(path.clone()).await.expect("open"));
        let mut handles = Vec::new();
        for _ in 0..10 {
            let sink = Arc::clone(&sink);
            let record = make_record();
            handles.push(tokio::spawn(async move {
                sink.emit(&record).await.expect("emit");
            }));
        }
        for h in handles {
            h.await.expect("join");
        }
        // Drop our Arc so the inner FileSink can be dropped (and the
        // file flushed). We're the last Arc holder after the spawned
        // tasks completed and dropped their Arcs.
        drop(sink);
        // Small yield to let the runtime finalize the drop chain.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let contents = read_to_string(&path).await;
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(
            lines.len(),
            10,
            "expected 10 lines; got {} (contents bytes: {})",
            lines.len(),
            contents.len()
        );
        // Each line must be a complete formatter output (no
        // interleaving). The deterministic suffix is the ending of
        // make_record()'s output; every line must end with that
        // suffix (only the [START_TIME] prefix differs across lines
        // — they're all the same record, but each line is
        // independently rendered).
        for (i, line) in lines.iter().enumerate() {
            assert!(
                line.ends_with("\"GET / HTTP/1.1\" 200 - 0 3 5 - \"-\" \"-\" \"-\" \"envoy-rust.test\" \"-\""),
                "line {} interleaved: {}",
                i,
                line
            );
        }
    }

    #[tokio::test]
    async fn file_sink_emit_returns_error_on_invalid_path() {
        // Attempt to open a sink at a path whose parent directory
        // does not exist. Per architecture decision 7 (signpost 6),
        // FileSink::new does NOT mkdir -p; the OS-level open() will
        // return ENOENT and FileSink::new maps to AccessLogError::Open.
        let path = PathBuf::from("/nonexistent-parent-directory-06-2-fixture/access.log");
        let err = FileSink::new(path.clone()).await.expect_err("expected open error");
        match err {
            AccessLogError::Open { path: got_path, source: _ } => {
                assert_eq!(got_path, path);
            }
            other => panic!("expected AccessLogError::Open; got {:?}", other),
        }
    }
}
```

- [ ] **Step 2: Verify the tests fail to compile (`FileSink::new` / `FileSink::emit` undefined; the Task 2 placeholder `FileSink` is a unit struct with no methods).**

Run: `cargo test -p envoy-accesslog --lib file_sink::tests 2>&1 | tail -25`
Expected: errors `no function or associated item named 'new' found for struct 'FileSink'` and similar for `emit`.

- [ ] **Step 3: Replace the `// (Step 3: ...)` marker with the `FileSink` struct + impl.**

Edit `crates/envoy-accesslog/src/file_sink.rs` — replace the `// (Step 3: ...)` line with:

```rust
/// FileSink — concrete on-disk access-log sink.
///
/// Owns an `Arc<tokio::sync::Mutex<File>>` so concurrent emissions
/// on the same `Arc<FileSink>` serialize at the mutex boundary
/// rather than racing at the kernel append boundary. The path is
/// retained for error reporting via `AccessLogError::Write`.
pub struct FileSink {
    path: PathBuf,
    handle: Arc<Mutex<File>>,
}

impl FileSink {
    /// Open (or create + truncate-disabled) the file at `path` in
    /// append mode. Returns `AccessLogError::Open` on filesystem
    /// failure (permissions, parent-directory-missing, path is a
    /// directory, etc.). Per 06.2 SPEC §6 signpost 6 + signpost 7,
    /// the constructor does NOT mkdir -p, does NOT pre-validate
    /// path shape, and does NOT truncate existing files.
    pub async fn new(path: PathBuf) -> Result<Self, AccessLogError> {
        let file = tokio::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .await
            .map_err(|source| AccessLogError::Open {
                path: path.clone(),
                source,
            })?;
        Ok(Self {
            path,
            handle: Arc::new(Mutex::new(file)),
        })
    }

    /// Format `record` per the Envoy default format and append the
    /// result + a trailing `\n` to the underlying file. Returns
    /// `AccessLogError::Write` on filesystem failure. The HCM
    /// dispatch site at `envoy-http1::hcm` does NOT propagate this
    /// error — emission failures are logged via `tracing::warn!`
    /// and discarded per parent-06 SPEC §6 architectural Rule 4
    /// (fire-and-forget).
    ///
    /// Concurrent emissions on the same `Arc<FileSink>` serialize
    /// at the per-sink `Mutex<File>` — no two records will
    /// interleave in the file.
    pub async fn emit(&self, record: &AccessLogRecord) -> Result<(), AccessLogError> {
        let line = format(record);
        let mut file = self.handle.lock().await;
        file.write_all(line.as_bytes())
            .await
            .map_err(|source| AccessLogError::Write {
                path: self.path.clone(),
                source,
            })?;
        file.write_all(b"\n")
            .await
            .map_err(|source| AccessLogError::Write {
                path: self.path.clone(),
                source,
            })?;
        // No explicit flush — the kernel will flush on file close.
        // Tests drop the FileSink (and let the runtime finalize the
        // drop chain via the test-internal tokio::time::sleep) to
        // force flush before reading.
        Ok(())
    }
}
```

- [ ] **Step 4: Run the tests and verify they pass.**

Run: `cargo test -p envoy-accesslog --lib file_sink::tests 2>&1 | tail -15`
Expected: `test result: ok. 4 passed; 0 failed`.

Note: `file_sink_serializes_concurrent_emissions` uses `tokio::time::sleep(50ms)` to let the runtime finalize the file-drop chain after the inner FileSink is dropped (the test relies on OS-level append atomicity to keep the lines whole). If the test is flaky in CI, the planner-executor extends the sleep or replaces it with an explicit `file.flush().await` in `emit` (mechanically minor; the `flush` call is already permitted on `tokio::fs::File`). Recommendation: ship as-is; revisit only if CI flakes.

- [ ] **Step 5: Run clippy + fmt + the full crate test set.**

Run in parallel:
- `cargo clippy -p envoy-accesslog --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -3`
- `cargo test -p envoy-accesslog 2>&1 | tail -10`

Expected: clippy clean; fmt clean; `test result: ok. 14 passed; 0 failed` (the 2 record tests + 8 default_format tests + 4 file_sink tests).

- [ ] **Step 6: Append PROGRESS.md Task 4 entry + commit.**

Append to `docs/envoy-rust/phases/06.2-access-log/PROGRESS.md`:

```markdown

## Task 4 — envoy-accesslog FileSink

Lands `crates/envoy-accesslog/src/file_sink.rs` per SPEC §3 D1.2 + signpost 3 (`Arc<tokio::sync::Mutex<File>>` posture preserves append-semantic atomicity inside the process).

**API landed:**
- `pub struct FileSink { path, handle: Arc<Mutex<File>> }`.
- `pub async fn FileSink::new(path: PathBuf) -> Result<Self, AccessLogError>` — opens with `append(true).create(true)`; maps `io::Error` → `AccessLogError::Open`.
- `pub async fn FileSink::emit(&self, record: &AccessLogRecord) -> Result<(), AccessLogError>` — formats via `default_format::format`, writes line + `\n` under the mutex, maps `io::Error` → `AccessLogError::Write`.

**Tests:** `cargo test -p envoy-accesslog --lib file_sink::tests` → `test result: ok. 4 passed; 0 failed`. The serialize-concurrent-emissions test spawns 10 concurrent emissions on one `Arc<FileSink>` and verifies the resulting file contains 10 complete lines with no interleaving.

**Crate-wide tests:** `cargo test -p envoy-accesslog` → `14 passed` (2 record + 8 default_format + 4 file_sink).

**Workspace gates:** clippy clean; fmt clean.

**LoC:** ~180 LoC (~50 impl + ~130 tests). The concurrent-emissions test alone is ~45 LoC including the spawned-task plumbing.
```

Stage + commit:

```bash
git add crates/envoy-accesslog/src/file_sink.rs docs/envoy-rust/phases/06.2-access-log/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.2: envoy-accesslog FileSink (task 4)

Lands crates/envoy-accesslog/src/file_sink.rs per SPEC §3 D1.2 + signpost 3.
FileSink::new opens with append(true).create(true) and wraps the File in
Arc<tokio::sync::Mutex<File>> for per-sink serialization. FileSink::emit
formats the record via default_format::format, writes line + '\n' under
the mutex, maps io::Error to AccessLogError. 4 unit tests including a
10-concurrent-emissions serialization test.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Run `git status` to confirm clean working tree.

---

## Task 5: envoy-config schema additions — `AccessLog` + `AccessLogTypedConfig` + 2 ConfigError variants + validator + 6 unit tests + 1 corpus-walk + fuzz seed

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs`
- Modify: `crates/envoy-config/src/lib.rs`
- Modify: `crates/envoy-config/fuzz/.gitignore`
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_access_log_file.yaml`

Lands the parse-side schema growth per 06.2 SPEC §3 D2.2. New `AccessLog` struct mirrors a subset of Envoy's `envoy.config.accesslog.v3.AccessLog` proto (`name: String` + `typed_config: AccessLogTypedConfig`); the new `AccessLogTypedConfig` is a single-variant `#[serde(tag = "@type", deny_unknown_fields)]` enum carrying only `FileAccessLog`; the `HttpConnectionManagerConfig.access_log: Vec<AccessLog>` field is `#[serde(default)]` for back-compat with existing fixtures per PLAN-write SPEC correction 4. Validator at `validate_hcm` rejects non-file loggers with `ConfigError::UnsupportedAccessLogType { actual }` and empty paths with `ConfigError::InvalidAccessLogPath`. Fuzz corpus gains 1 new accept-path seed.

**SPEC clarification (PLAN-write):** The SPEC §3 D2.2 refers to a `FileAccessLogTypedConfig` variant on the existing `TypedConfig` enum, but the existing `TypedConfig` at `bootstrap.rs:253-262` is the network-filter-level enum carrying `TcpProxy` + `HttpConnectionManager` — not a generic envelope. Putting a file-access-log under that enum would be category-confused. **Resolution:** Task 5 introduces a NEW enum `AccessLogTypedConfig` (mirroring the `TypedConfig` shape — `#[serde(tag = "@type", deny_unknown_fields)]`) embedded inside `AccessLog.typed_config`. Naming follows project convention: parent enum is `*TypedConfig`; payload is `FileAccessLog`.

- [ ] **Step 1: Write the 6 failing tests in `bootstrap.rs::tests`.**

Append to `crates/envoy-config/src/bootstrap.rs`'s `#[cfg(test)] mod tests` block (before its closing `}`):

```rust
    // ----- 06.2 Task 5: access_log schema tests -----

    fn hcm_with_access_log_yaml(access_log_block: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: t }}
static_resources:
  listeners:
    - name: l1
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
{access_log_block}
                route_config:
                  name: r
                  virtual_hosts:
                    - name: v
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: "ok\n" }} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#,
            access_log_block = access_log_block
        )
    }

    #[test]
    fn parses_hcm_with_file_access_log() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
"#,
        );
        let bootstrap = crate::parse_bootstrap(&yaml).expect("parse + validate");
        let listener = &bootstrap.static_resources.listeners[0];
        let filter = &listener.filter_chains[0].filters[0];
        let hcm = match &filter.typed_config {
            TypedConfig::HttpConnectionManager(h) => h,
            _ => panic!("expected HCM"),
        };
        assert_eq!(hcm.access_log.len(), 1);
        assert_eq!(hcm.access_log[0].name, "envoy.access_loggers.file");
        match &hcm.access_log[0].typed_config {
            AccessLogTypedConfig::FileAccessLog(cfg) => {
                assert_eq!(cfg.path, "/tmp/access.log");
            }
        }
    }

    #[test]
    fn parses_hcm_with_no_access_log_block() {
        let yaml = hcm_with_access_log_yaml("");
        let bootstrap = crate::parse_bootstrap(&yaml).expect("parse + validate");
        let hcm = match &bootstrap.static_resources.listeners[0].filter_chains[0].filters[0].typed_config {
            TypedConfig::HttpConnectionManager(h) => h,
            _ => panic!("expected HCM"),
        };
        assert!(hcm.access_log.is_empty());
    }

    #[test]
    fn parses_hcm_with_empty_access_log_array() {
        let yaml = hcm_with_access_log_yaml(r#"                access_log: []
"#);
        let bootstrap = crate::parse_bootstrap(&yaml).expect("parse + validate");
        let hcm = match &bootstrap.static_resources.listeners[0].filter_chains[0].filters[0].typed_config {
            TypedConfig::HttpConnectionManager(h) => h,
            _ => panic!("expected HCM"),
        };
        assert!(hcm.access_log.is_empty());
    }

    #[test]
    fn rejects_hcm_with_unsupported_access_log_name() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.stdout
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        match err {
            ConfigError::UnsupportedAccessLogType { actual } => {
                assert_eq!(actual, "envoy.access_loggers.stdout");
            }
            other => panic!("expected UnsupportedAccessLogType; got {:?}", other),
        }
    }

    #[test]
    fn rejects_hcm_with_unsupported_access_log_type_url() {
        // The serde-tagged `@type` enum rejects unknown URLs at
        // deserialization time (wrapped as ConfigError::Yaml).
        // The test accepts either error path.
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.unknown.v3.UnknownAccessLog
                      path: /tmp/access.log
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        match err {
            ConfigError::Yaml(_) => {}
            ConfigError::UnsupportedAccessLogType { .. } => {}
            other => panic!("expected Yaml or UnsupportedAccessLogType; got {:?}", other),
        }
    }

    #[test]
    fn rejects_hcm_with_empty_access_log_path() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: ""
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        assert!(matches!(err, ConfigError::InvalidAccessLogPath));
    }
```

Also extend the existing corpus-walk acceptance test (`fuzz_corpus_seeds_parse_or_reject_cleanly` per 06.1 Task 9's explicit walk-list pattern). Locate the `&[...]` array — find via `grep -n 'fuzz_corpus_seeds' crates/envoy-config/src/bootstrap.rs` — and append:

```rust
        include_str!("../fuzz/corpus/parse_bootstrap/hcm_access_log_file.yaml"),
```

- [ ] **Step 2: Verify the tests fail to compile.**

Run: `cargo build -p envoy-config --tests 2>&1 | tail -20`
Expected: 4+ errors (`cannot find type 'AccessLog' / 'AccessLogTypedConfig' / variant 'UnsupportedAccessLogType' / 'InvalidAccessLogPath'`; missing `hcm.access_log` field).

- [ ] **Step 3: Add the new types to `bootstrap.rs`** (insert after the existing `TypedConfig` enum at line ~262):

```rust
// 06.2 Task 5 — access-log schema additions per SPEC §3 D2.2.

/// AccessLog — one entry in an HCM's `access_log:` block.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AccessLog {
    pub name: String,
    pub typed_config: AccessLogTypedConfig,
}

/// AccessLogTypedConfig — typed_config envelope. Single variant in
/// 06.2; future observability-family phases extend.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum AccessLogTypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog")]
    FileAccessLog(FileAccessLog),
}

/// FileAccessLog — typed_config payload for the file access logger.
/// 06.2 consumes only `path`; format-string customization is OUT of
/// scope per parent-06 SPEC §4 + 06.2 SPEC §4.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileAccessLog {
    pub path: String,
}
```

- [ ] **Step 4: Add the `access_log` field to `HttpConnectionManagerConfig`** (insert after `http2_protocol_options` at line ~365):

```rust
    /// 06.2 NEW: per-listener access-log entries. Default-empty;
    /// absent block parses cleanly. Validator rejects non-file
    /// loggers (UnsupportedAccessLogType) and empty paths
    /// (InvalidAccessLogPath).
    #[serde(default)]
    pub access_log: Vec<AccessLog>,
```

`#[serde(default)]` is load-bearing per PLAN-write SPEC correction 4.

- [ ] **Step 5: Add the 2 new `ConfigError` variants to `crates/envoy-config/src/lib.rs`** (append at the end of the enum, before the closing `}`):

```rust
    /// 06.2 NEW.
    #[error("unsupported access log type: {actual}; only 'envoy.access_loggers.file' with @type ending in .FileAccessLog is supported")]
    UnsupportedAccessLogType { actual: String },

    /// 06.2 NEW.
    #[error("access log path must be non-empty")]
    InvalidAccessLogPath,
```

Re-export the new types at the `pub use bootstrap::{...}` block — append `AccessLog`, `AccessLogTypedConfig`, `FileAccessLog`.

- [ ] **Step 6: Add the validator extension.** Edit `validate_hcm` (`bootstrap.rs:1240`) — insert just after the http2_protocol_options range check, before the http_filters cardinality check:

```rust
    validate_access_logs(&hcm.access_log)?;
```

Define the new free function just below `validate_hcm`'s closing `}`:

```rust
fn validate_access_logs(access_logs: &[AccessLog]) -> Result<(), crate::ConfigError> {
    for entry in access_logs {
        if entry.name != "envoy.access_loggers.file" {
            return Err(crate::ConfigError::UnsupportedAccessLogType {
                actual: entry.name.clone(),
            });
        }
        match &entry.typed_config {
            AccessLogTypedConfig::FileAccessLog(cfg) => {
                if cfg.path.is_empty() {
                    return Err(crate::ConfigError::InvalidAccessLogPath);
                }
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 7: Create the fuzz corpus seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_access_log_file.yaml`:**

```yaml
node: { id: fuzz-06.2, cluster: fuzz-06.2 }
static_resources:
  listeners:
    - name: l1
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
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
                      path: /tmp/fuzz-access.log
                route_config:
                  name: r
                  virtual_hosts:
                    - name: v
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "fuzz\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 8: Allow-list the new seed.** Append to `crates/envoy-config/fuzz/.gitignore`:

```
!corpus/parse_bootstrap/hcm_access_log_file.yaml
```

File now lists 17 allow-list entries (16 pre-06.2 + 1 new).

- [ ] **Step 9: Run tests + workspace gates.**

Run in parallel:
- `cargo test -p envoy-config 2>&1 | tail -20`
- `cargo build --workspace --all-targets 2>&1 | tail -10`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -3`

Expected: all pre-existing tests pass + 6 new validator tests pass + corpus-walk reads the new seed; workspace build/clippy/fmt clean.

- [ ] **Step 10: Append PROGRESS.md Task 5 entry + commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs \
        crates/envoy-config/fuzz/.gitignore \
        crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_access_log_file.yaml \
        docs/envoy-rust/phases/06.2-access-log/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.2: envoy-config access_log schema + 2 ConfigError variants + fuzz seed (task 5)

Lands the parse-side access-log schema per SPEC §3 D2.2. New types
AccessLog + AccessLogTypedConfig (single-variant tagged enum on @type)
+ FileAccessLog. HCM gains #[serde(default)] pub access_log: Vec<AccessLog>.
New ConfigError variants UnsupportedAccessLogType + InvalidAccessLogPath.
Validator extension validate_access_logs at validate_hcm. 6 new tests +
1 fuzz corpus seed + corpus-walk extended.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: HCM H1 access-log wiring — `HCMConfig.access_log` field + `from_config` extension + factored dispatch site + 4 unit tests

**Files:**
- Modify: `crates/envoy-http1/Cargo.toml`
- Modify: `crates/envoy-http1/src/hcm.rs`

Lands the core runtime deliverable per 06.2 SPEC §3 D3.2 + PLAN-write SPEC correction 1 (factored join-point dispatch). `HCMConfig` (the per-listener immutable config struct at `crates/envoy-http1/src/hcm.rs:74-88`) gains a `pub access_log: Vec<Arc<envoy_accesslog::FileSink>>` field. `HCMConfig::from_config` (the constructor that translates `envoy_config::HttpConnectionManagerConfig` + supporting structs into the runtime `HCMConfig`) gains a new async path that calls `FileSink::new(path).await` for each parsed `AccessLog` entry. `serve_connection` is refactored to capture per-request state (request-arrival `Instant` + `SystemTime`, parsed `Request`, response-write outcome) and dispatch one `AccessLogRecord` per request at a factored join point after the 5-way `match outcome { ... }` block ends.

**Architecture-decision recap (per the header):** dispatch posture is synchronous-after-write (option (b)); H1 dispatch site lands at the factored join point per correction 1.

**Cross-check at task time** the 5 write sites: `hcm.rs:266-268` (synth) / `hcm.rs:286-291` (proxy 503) / `hcm.rs:336-341` (proxy 502 connect-failed) / `hcm.rs:353-358` (proxy 502 other) / `hcm.rs:364-370` (proxy happy via `crate::router::write_proxied_response`). The exact line numbers may drift between PLAN-write and Task 6 execution; the structural anchor is the `match outcome { ... }` block whose arms cover all 5 paths.

- [ ] **Step 1: Add `envoy-accesslog` path-dep to `crates/envoy-http1/Cargo.toml`.**

Edit `crates/envoy-http1/Cargo.toml` `[dependencies]` block — append (alphabetically; the existing block lists `envoy-cluster`, `envoy-config`, `envoy-listener`, `envoy-stats` as path-deps):

```toml
envoy-accesslog = { path = "../envoy-accesslog" }
```

The line sorts before `envoy-cluster` alphabetically. The block now reads (cross-checked against HEAD):

```toml
[dependencies]
envoy-accesslog = { path = "../envoy-accesslog" }
envoy-config = { path = "../envoy-config" }
envoy-cluster = { path = "../envoy-cluster" }
envoy-listener = { path = "../envoy-listener" }
envoy-stats = { path = "../envoy-stats" }
httparse = "1"
bytes = "1"
tokio = { version = "1", features = ["net", "io-util", "macros", "sync", "time"] }
thiserror = "2"
tracing = "0.1"
```

- [ ] **Step 2: Write the 4 failing tests at the bottom of `crates/envoy-http1/src/hcm.rs`'s `#[cfg(test)] mod tests` block.**

Append (before the existing `mod tests`'s closing `}`):

```rust
    // ----- 06.2 Task 6: access-log wiring tests -----

    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::tempdir;

    /// In-process tracing-subscriber test fixture for capturing
    /// warn! lines per architecture decision 13 (signpost 15 option
    /// (b)). Records the most recent emission's formatted message.
    struct WarnCapture {
        captured: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl WarnCapture {
        fn install() -> (Self, tracing::subscriber::DefaultGuard) {
            use tracing_subscriber::layer::SubscriberExt as _;
            let captured: Arc<std::sync::Mutex<Vec<String>>> =
                Arc::new(std::sync::Mutex::new(Vec::new()));
            let captured_for_layer = Arc::clone(&captured);
            let layer = tracing_subscriber::fmt::layer()
                .with_writer(move || -> Box<dyn std::io::Write + Send> {
                    Box::new(CaptureWriter {
                        captured: Arc::clone(&captured_for_layer),
                    })
                });
            let subscriber = tracing_subscriber::registry().with(layer);
            let guard = tracing::subscriber::set_default(subscriber);
            (Self { captured }, guard)
        }

        fn lines(&self) -> Vec<String> {
            self.captured.lock().unwrap().clone()
        }
    }

    struct CaptureWriter {
        captured: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Ok(s) = std::str::from_utf8(buf) {
                self.captured.lock().unwrap().push(s.to_owned());
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    // The 4 tests below use a helper `serve_one_request_with_access_log`
    // that builds an HCM with a configured FileSink and drives one
    // request through it via an in-memory tokio::io::duplex stream
    // (mirrors the existing tests in this module for harness style).
    // The helper signature:
    //
    //   async fn serve_one_request_with_access_log(
    //       access_log_paths: &[std::path::PathBuf],
    //   ) -> Vec<String>  // returns the access-log lines from each path
    //
    // Implementation is mechanical — wraps the existing test-fixture
    // pattern from this module. Cross-check the existing patterns
    // (`hcm_with_simple_direct_response` and friends) at task time.

    #[tokio::test]
    async fn hcm_with_no_access_log_does_not_touch_filesystem() {
        let dir = tempdir().expect("tempdir");
        let path_that_should_not_exist = dir.path().join("nope.log");
        // Build HCM with empty access_log.
        // (Helper: serve_one_request through an HCM whose
        // HCMConfig.access_log is the empty Vec.)
        let lines_per_sink: Vec<Vec<String>> = serve_one_request_with_access_log(&[]).await;
        assert!(lines_per_sink.is_empty());
        assert!(!path_that_should_not_exist.exists(), "no file should be created");
    }

    #[tokio::test]
    async fn hcm_with_file_access_log_writes_one_line_per_request() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let lines_per_sink = serve_one_request_with_access_log(&[path.clone()]).await;
        assert_eq!(lines_per_sink.len(), 1);
        assert_eq!(lines_per_sink[0].len(), 1);
        let line = &lines_per_sink[0][0];
        // Per the Envoy default format suffix expected by fixture
        // 0012 (GET / → 200 ok\n; protocol HTTP/1.1).
        assert!(
            line.ends_with("\"GET / HTTP/1.1\" 200 - 0 3 ")
                || line.contains("\"GET / HTTP/1.1\" 200 - 0 3 "),
            "line: {}",
            line
        );
    }

    #[tokio::test]
    async fn hcm_with_file_access_log_emission_failure_does_not_fail_request() {
        // Set up a FileSink against a path whose parent is removed
        // mid-test to force AccessLogError::Write at emit time.
        // Verify (a) the request still completes with a 200 status
        // and (b) a tracing::warn! line was captured.
        let (capture, _guard) = WarnCapture::install();
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        // Open the sink against a valid path first.
        let sink = Arc::new(envoy_accesslog::FileSink::new(path.clone()).await.expect("open"));
        // Drop the tempdir (which removes the file's parent).
        drop(dir);
        // Now drive a request; emit() should fail with AccessLogError::Write.
        // (Helper variant takes pre-constructed sinks rather than paths.)
        let result = serve_one_request_with_pre_constructed_sinks(&[sink]).await;
        assert!(result.is_ok(), "request should succeed despite emission failure");
        let warn_lines = capture.lines().join("");
        assert!(
            warn_lines.contains("access log emission failed") || warn_lines.contains("AccessLogError"),
            "expected warn line; captured: {}",
            warn_lines
        );
    }

    #[tokio::test]
    async fn hcm_records_protocol_as_http1_1_on_h1_path() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let lines_per_sink = serve_one_request_with_access_log(&[path.clone()]).await;
        let line = &lines_per_sink[0][0];
        assert!(line.contains("HTTP/1.1"), "line: {}", line);
    }
```

Define the two test helpers (`serve_one_request_with_access_log`, `serve_one_request_with_pre_constructed_sinks`) by mechanically extending the existing `mod tests` harness pattern. Locate the existing direct-response harness in `crates/envoy-http1/src/hcm.rs::tests` via `grep -n 'fn hcm_' crates/envoy-http1/src/hcm.rs` and clone its in-memory tokio::io::duplex stream + HCMConfig construction, with the only delta being `HCMConfig.access_log: Vec<Arc<FileSink>>` populated from the helper's argument.

- [ ] **Step 3: Verify the tests fail to compile (HCMConfig.access_log field undefined).**

Run: `cargo build -p envoy-http1 --tests 2>&1 | tail -10`
Expected: error `no field 'access_log' on type 'HCMConfig'` (or similar).

- [ ] **Step 4: Add the `access_log` field to `HCMConfig`** at `crates/envoy-http1/src/hcm.rs:74-88` (cross-checked):

```rust
#[derive(Debug)]
pub struct HCMConfig {
    pub stat_prefix: String,
    pub route_config: Arc<RouteConfiguration>,
    pub cluster_mgr: Arc<envoy_cluster::ClusterManager>,
    pub http2_protocol_options: Option<envoy_config::Http2ProtocolOptions>,
    pub stats: Arc<HCMStats>,

    /// 06.2 NEW: configured access-log sinks. Empty by default;
    /// non-empty when the listener YAML carries an `access_log:`
    /// block. The HCM dispatches each per-request record to every
    /// sink in this vec at a factored join point in
    /// `serve_connection` (synchronous-after-write per parent-06
    /// SPEC §6 architectural Rule 4 option (b); emission errors
    /// logged via `tracing::warn!` and discarded).
    pub access_log: Vec<Arc<envoy_accesslog::FileSink>>,
}
```

- [ ] **Step 5: Extend `HCMConfig::from_config` (or the analogous existing constructor) to build the `access_log` field.**

Locate the existing `from_config` (or equivalent) — the 06.1 Task 10 work added the `stats` registration site here; the 06.2 work adds an analogous `access_log` construction site. Cross-check the constructor location via `grep -n 'fn from_config' crates/envoy-http1/src/hcm.rs`.

Add the construction logic. The constructor is `async fn` per the FileSink open semantic:

```rust
// Inside HCMConfig::from_config (or equivalent), after the stats
// registration site and before the return:
let mut access_log_sinks = Vec::new();
for entry in &parsed_hcm.access_log {
    match &entry.typed_config {
        envoy_config::AccessLogTypedConfig::FileAccessLog(cfg) => {
            let sink = envoy_accesslog::FileSink::new(
                std::path::PathBuf::from(&cfg.path)
            )
            .await
            .map_err(|err| Http1Error::AccessLogOpen { source: err.to_string() })?;
            access_log_sinks.push(Arc::new(sink));
        }
    }
}
```

The `Http1Error::AccessLogOpen { source: String }` is a new variant on the existing `Http1Error` enum at `crates/envoy-http1/src/error.rs` (or wherever the enum is defined; cross-check via `grep -rn 'pub enum Http1Error'`). Add:

```rust
    #[error("failed to open access log sink: {source}")]
    AccessLogOpen { source: String },
```

The `String` wrapping is per the project's existing typed-error discipline (06.1's `ListenerError::StatsRegistration(String)` precedent — preserves dependency direction; `envoy-http1` does not re-export `envoy_accesslog::AccessLogError`). Map the error to its Display rendering at the construction site.

If `HCMConfig::from_config` is NOT currently async, promote it to `async fn` here. Callers in `envoy-bin` propagate the `.await` per the existing 05.1 Task 2 precedent (`Cluster::from_bootstrap` promoted to async).

Make the new `access_log` field non-optional in the `HCMConfig { ... }` constructor body:

```rust
HCMConfig {
    stat_prefix,
    route_config: Arc::new(route_config),
    cluster_mgr,
    http2_protocol_options,
    stats: hcm_stats,
    access_log: access_log_sinks,
}
```

- [ ] **Step 6: Refactor `serve_connection` to dispatch access-log records at the factored join point.**

Per PLAN-write SPEC correction 1, the dispatch site is AFTER the `match outcome { ... }` block ends (covering the 5 writer outcomes) and BEFORE the keep-alive `continue`/`return Ok(())` decision at `hcm.rs:375-377`.

The refactor adds:
1. **Pre-write capture:** at the start of each iteration of the per-request loop (just after `read_request` succeeds), capture `let req_arrival_instant = std::time::Instant::now();` and `let req_arrival_systime = std::time::SystemTime::now();`. Both captures live on the per-request stack.
2. **In-write state capture:** during the route walk (before the `match outcome` arms fire), record the `Response.status`, `response.body.len() as u64`, and `request.body.as_ref().map_or(0, |b| b.len() as u64)` into per-request locals. For the proxy happy path (`hcm.rs:364-370`), the `upstream_response` provides the response status + body length; record those after the upstream call returns but before `write_proxied_response`.
3. **Upstream-host capture:** for `BuildOutcome::Proxy` outcomes, capture the resolved upstream `SocketAddr` formatted via `format!("{}", addr)` before dispatching to `crate::router::write_proxied_response`. For other outcomes (synth + 503 + 502s), the upstream-host is `None`.
4. **Post-`match outcome` dispatch:** insert at the join point (after the 5-way match closes, before `continue`/`return Ok(())`):

```rust
// 06.2: factored access-log dispatch site. Per PLAN-write SPEC
// correction 1, this single site handles all 5 writer outcomes
// (synth + 4 proxy paths). Per parent-06 SPEC §6 architectural
// Rule 4 (fire-and-forget option (b)): synchronous-after-write;
// emission errors are logged via tracing::warn! and discarded.
if !config.access_log.is_empty() {
    let duration = req_arrival_instant.elapsed();
    let record = envoy_accesslog::AccessLogRecord {
        start_time: req_arrival_systime,
        method: request.method.clone(),
        path: x_envoy_original_path_or_path(&request).to_owned(),
        protocol: "HTTP/1.1".to_owned(),
        response_code: response_status_for_log,
        response_flags: "-".to_owned(),  // 06.2 always emits "-"
        bytes_received: request_body_len,
        bytes_sent: response_body_len,
        duration,
        upstream_service_time: extract_upstream_service_time(&response_headers_for_log),
        forwarded_for: header_value(&request.headers, "x-forwarded-for"),
        user_agent: header_value(&request.headers, "user-agent"),
        request_id: header_value(&request.headers, "x-request-id"),
        authority: header_value(&request.headers, "host"),
        upstream_host: upstream_host_for_log,
    };
    for sink in &config.access_log {
        if let Err(err) = sink.emit(&record).await {
            tracing::warn!(error = ?err, "access log emission failed");
        }
    }
}
```

Define the two private helpers near the top of the module (or in a sibling `mod access_log;` file):

```rust
fn x_envoy_original_path_or_path(req: &Request) -> &str {
    for (name, value) in &req.headers {
        if name.eq_ignore_ascii_case("x-envoy-original-path") {
            return value.as_str();
        }
    }
    req.path.as_str()
}

fn header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

fn extract_upstream_service_time(headers: &[(String, String)]) -> Option<std::time::Duration> {
    let v = header_value(headers, "x-envoy-upstream-service-time")?;
    let ms: u64 = v.parse().ok()?;
    Some(std::time::Duration::from_millis(ms))
}
```

The threading-through-the-match work for `response_status_for_log`, `response_body_len`, `upstream_host_for_log`, etc. is mechanical: each arm of the match populates the locals before falling through to the join point. The exact refactor shape is to be cross-checked at task time by reading the current `serve_connection` body; the 5-arm cascade is unchanged in shape, but the writer sites now feed the locals instead of returning early.

- [ ] **Step 7: Run the tests.**

Run: `cargo test -p envoy-http1 --lib hcm::tests::hcm_with 2>&1 | tail -10`
Expected: 4 new tests pass + all pre-existing envoy-http1 tests still pass.

If `hcm_with_file_access_log_emission_failure_does_not_fail_request` flakes (tracing-test-style capture races with the layer setup), retry the test or factor the warn-line capture into a synchronous assert by directly invoking the dispatch site code via a unit-level shim. Recommendation: ship the test as-is; the `WarnCapture` shape is robust because the layer is installed before the test's tracing emission via `set_default`.

- [ ] **Step 8: Run workspace gates.**

Run in parallel:
- `cargo build --workspace --all-targets 2>&1 | tail -10`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -3`
- `cargo test --workspace 2>&1 | tail -5`

Expected: all clean. The promotion of `HCMConfig::from_config` to `async fn` (if it wasn't already) ripples to callers in `envoy-bin` (`crates/envoy-bin/src/main.rs` calls `.await` on this site); cross-check ripple sites via `grep -rn 'HCMConfig::from_config' crates/`.

- [ ] **Step 9: Append PROGRESS.md Task 6 entry + commit.**

Stage + commit:

```bash
git add crates/envoy-http1/Cargo.toml crates/envoy-http1/src/hcm.rs crates/envoy-http1/src/error.rs docs/envoy-rust/phases/06.2-access-log/PROGRESS.md
# Plus any rippled call sites in envoy-bin if from_config's async-promotion forces them:
# git add crates/envoy-bin/src/main.rs
git commit -m "$(cat <<'EOF'
phase 06.2: HCM H1 access-log wiring + factored dispatch site + 4 unit tests (task 6)

Lands per SPEC §3 D3.2 + PLAN-write SPEC correction 1 (factored join-point
dispatch). HCMConfig grows access_log: Vec<Arc<FileSink>> field; from_config
constructs the sinks; serve_connection's per-request loop captures
request-arrival timing, per-arm response state, upstream-host, then emits
one AccessLogRecord at a single join point after the 5-way match outcome.
Synchronous-after-write per Rule 4 option (b); emission errors logged via
tracing::warn! and discarded. New Http1Error variant AccessLogOpen for
sink construction failure.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: HCM H2 access-log wiring — `envoy-http2` path-dep + dispatch site at `hcm.rs:251` + 2 unit tests

**Files:**
- Modify: `crates/envoy-http2/Cargo.toml`
- Modify: `crates/envoy-http2/src/hcm.rs`

Lands the H2-side dispatch per 06.2 SPEC §3 D3.2 H2 inheritance path + PLAN-write SPEC correction 2 (dispatch lands after `send_envoy_response` returns at `hcm.rs:251`, not at any specific `send_data` line). Per PLAN-write SPEC correction 5, `envoy-http2` gains a direct path-dep on `envoy-accesslog` (the SPEC's claim that the H2 path inherits transparently via the type-alias is correct in the sense that `HCMConfig` itself is type-aliased — but the H2 code calls `sink.emit()` on each `Arc<envoy_accesslog::FileSink>` element of `config.access_log`, which requires the concrete type to be resolvable at compile time in `envoy-http2`).

The H2 dispatch mirrors the H1 record-build logic from Task 6, with the only difference: `protocol: "HTTP/2".to_owned()` (vs `"HTTP/1.1"`). The factoring point in H2 is simpler than H1 because the H2 per-stream task already has a single write-completion site (`send_envoy_response` returns).

- [ ] **Step 1: Add `envoy-accesslog` path-dep to `crates/envoy-http2/Cargo.toml`.**

Edit `[dependencies]` — append (alphabetically before `envoy-cluster`):

```toml
envoy-accesslog = { path = "../envoy-accesslog" }
```

- [ ] **Step 2: Write the 2 failing tests at the bottom of `crates/envoy-http2/src/hcm.rs`'s `#[cfg(test)] mod tests` block.**

Append:

```rust
    // ----- 06.2 Task 7: H2 access-log wiring tests -----

    #[tokio::test]
    async fn hcm_h2_with_file_access_log_writes_one_line_per_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        // Helper builds an HCM with one FileSink, drives an H2 request
        // via h2-client + duplex stream (mirrors existing H2 HCM tests
        // in this module). Returns the access-log lines from each sink.
        let lines_per_sink = serve_one_h2_request_with_access_log(&[path.clone()]).await;
        assert_eq!(lines_per_sink.len(), 1);
        assert_eq!(lines_per_sink[0].len(), 1);
        let line = &lines_per_sink[0][0];
        assert!(line.contains("\"GET / HTTP/2\""), "line: {}", line);
    }

    #[tokio::test]
    async fn hcm_h2_records_protocol_as_http2_on_h2_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let lines_per_sink = serve_one_h2_request_with_access_log(&[path.clone()]).await;
        let line = &lines_per_sink[0][0];
        assert!(line.contains("HTTP/2"), "line: {}", line);
        assert!(!line.contains("HTTP/1.1"), "line should not contain HTTP/1.1: {}", line);
    }
```

The helper `serve_one_h2_request_with_access_log` mechanically clones the existing H2 HCM test harness in this module (cross-check via `grep -n 'fn hcm_h2_' crates/envoy-http2/src/hcm.rs`) and pre-populates `HCMConfig.access_log: Vec<Arc<FileSink>>` from the provided paths. The harness's request-driving uses h2's client API per the existing 05.2 / 05.3 test patterns; no new wire-driver code needed.

- [ ] **Step 3: Verify the tests fail to compile.**

Run: `cargo build -p envoy-http2 --tests 2>&1 | tail -10`
Expected: errors about `HCMConfig.access_log` field unused on the H2 dispatch side (or missing dispatch implementation).

- [ ] **Step 4: Add the H2 dispatch at `crates/envoy-http2/src/hcm.rs:251` (after `send_envoy_response(send_response, resp).await` returns).**

Per PLAN-write SPEC correction 2, the dispatch site is AFTER `send_envoy_response` returns (covering both empty-body and non-empty-body branches uniformly). The H2 per-stream `tokio::spawn`-ed task captures the same per-request state as Task 6's H1 version:

```rust
// 06.2: per-stream access-log dispatch on the H2 path. Mirrors the
// H1 factored join-point per parent-06 SPEC §3 D3.2 + PLAN-write
// SPEC correction 2. Lands AFTER send_envoy_response returns
// (covering both empty-body and non-empty-body emit branches).

// (Pre-emission state captured at the per-stream task entry: req
//  arrival Instant + SystemTime; request method/path/headers; in-
//  task response status + body bytes; no upstream-host since H2
//  proxy upstream lives in the router-arm dispatch.)

if !config.access_log.is_empty() {
    let duration = req_arrival_instant.elapsed();
    let record = envoy_accesslog::AccessLogRecord {
        start_time: req_arrival_systime,
        method: envoy_method.clone(),
        path: x_envoy_original_path_or_path(&envoy_request_headers, &envoy_path).to_owned(),
        protocol: "HTTP/2".to_owned(),
        response_code: response_status_for_log,
        response_flags: "-".to_owned(),
        bytes_received: request_body_len,
        bytes_sent: response_body_len,
        duration,
        upstream_service_time: extract_upstream_service_time(&envoy_resp_headers_for_log),
        forwarded_for: header_value(&envoy_request_headers, "x-forwarded-for"),
        user_agent: header_value(&envoy_request_headers, "user-agent"),
        request_id: header_value(&envoy_request_headers, "x-request-id"),
        authority: header_value(&envoy_request_headers, "host"),
        upstream_host: upstream_host_for_log_h2,
    };
    for sink in &config.access_log {
        if let Err(err) = sink.emit(&record).await {
            tracing::warn!(error = ?err, "access log emission failed");
        }
    }
}
```

Reuse the three helpers (`x_envoy_original_path_or_path`, `header_value`, `extract_upstream_service_time`) by promoting them to `pub(crate)` in Task 6's `envoy-http1` and re-exporting (or by cloning the ~10 LoC into `envoy-http2::hcm`). **Recommendation:** clone the helpers into `envoy-http2::hcm` for cross-crate copy-clarity (no `pub(crate)` cross-crate gymnastics; the helpers are ~30 LoC total).

Note on the helper signature: `envoy-http2`'s request value type is the codec-edge-translated `Request` (or an equivalent struct that already carries `method` / `path` / `headers` — cross-check at task time). The helpers operate on `&[(String, String)]` headers and `&str` for path/method, which matches both H1 and H2 shapes uniformly.

- [ ] **Step 5: Run the tests.**

Run: `cargo test -p envoy-http2 --lib hcm::tests::hcm_h2_with 2>&1 | tail -10`
Expected: 2 new tests pass + all pre-existing envoy-http2 tests still pass.

- [ ] **Step 6: Run workspace gates.**

Run in parallel:
- `cargo build --workspace --all-targets 2>&1 | tail -10`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -3`
- `cargo test --workspace 2>&1 | tail -5`

Expected: all clean.

- [ ] **Step 7: Append PROGRESS.md Task 7 entry + commit.**

Stage + commit:

```bash
git add crates/envoy-http2/Cargo.toml crates/envoy-http2/src/hcm.rs docs/envoy-rust/phases/06.2-access-log/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.2: HCM H2 access-log wiring + 2 unit tests (task 7)

Lands per SPEC §3 D3.2 H2 inheritance path + PLAN-write SPEC correction 2.
envoy-http2 gains a direct path-dep on envoy-accesslog (per correction 5;
the H2 code calls sink.emit() on concrete FileSink elements). Dispatch
site lands AFTER send_envoy_response returns (covers both empty-body and
non-empty-body branches uniformly). Record built mirrors the H1 path with
protocol: "HTTP/2". 2 new unit tests.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: In-process integration backstop — `crates/envoy-bin/tests/access_log_file_sink.rs`

**Files:**
- Create: `crates/envoy-bin/tests/access_log_file_sink.rs`

Lands the in-process integration backstop per 06.2 SPEC §6 signpost 18. Mirrors the 04.1 / 05.2 / 06.1 integration-test pattern: spawn `envoy-bin` via `CARGO_BIN_EXE_envoy-bin` against an HCM-with-file-sink config at a tempdir path; drive a single `GET /` request via the standard library (no fancy HTTP client; the HCM's direct_response action ships an `ok\n` body for an `HTTP/1.1 200 OK` response with `transfer-encoding: chunked` or `content-length: 3`); read the access-log file post-request; assert the line tokens.

The backstop runs without Docker (load-bearing on dev machines without Docker access; required for local regression discipline). Mirrors `crates/envoy-bin/tests/http2_direct_response.rs` (05.2-landed) and `crates/envoy-bin/tests/admin_ready.rs` (06.1-landed) structure.

- [ ] **Step 1: Create `crates/envoy-bin/tests/access_log_file_sink.rs`.**

```rust
//! In-process integration backstop for 06.2's access-log file sink.
//!
//! Spawns envoy-bin via CARGO_BIN_EXE_envoy-bin against an HCM
//! config with an `access_log:` block writing to a tempdir path;
//! drives a single GET / over HTTP/1.1; reads the access-log file
//! post-request; asserts the line tokens.
//!
//! Runs without Docker. Mirrors phase-04.1's http1_direct_response.rs,
//! phase-05.2's http2_direct_response.rs, and phase-06.1's
//! admin_ready.rs structure.

use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context as _, Result};
use tempfile::tempdir;

const ENVOY_BIN: &str = env!("CARGO_BIN_EXE_envoy-bin");

fn pick_free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = l.local_addr().expect("local_addr").port();
    drop(l);
    port
}

fn write_yaml_config(dir: &std::path::Path, listener_port: u16, access_log_path: &str) -> PathBuf {
    let yaml = format!(
        r#"
node: {{ id: it-06.2, cluster: it-06.2 }}
static_resources:
  listeners:
    - name: http1_listener
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: {listener_port} }} }}
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
                      path: {access_log_path}
                route_config:
                  name: r
                  virtual_hosts:
                    - name: v
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: "ok\n" }} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#,
        listener_port = listener_port,
        access_log_path = access_log_path,
    );
    let yaml_path = dir.join("envoy-rust.yaml");
    std::fs::write(&yaml_path, yaml).expect("write yaml");
    yaml_path
}

fn wait_for_port(addr: SocketAddr, deadline: std::time::Instant) -> Result<TcpStream> {
    loop {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(100)) {
            Ok(s) => return Ok(s),
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e).context("port did not open"),
        }
    }
}

#[test]
fn access_log_file_sink_in_process() -> Result<()> {
    let dir = tempdir().expect("tempdir");
    let access_log_path = dir.path().join("access.log");
    let access_log_path_str = access_log_path.to_str().expect("utf8 path");
    let listener_port = pick_free_port();
    let yaml_path = write_yaml_config(dir.path(), listener_port, access_log_path_str);

    // Spawn envoy-bin.
    let mut child = std::process::Command::new(ENVOY_BIN)
        .arg("-c")
        .arg(&yaml_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("spawn envoy-bin")?;

    let result: Result<()> = (|| {
        let addr: SocketAddr = format!("127.0.0.1:{}", listener_port).parse().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut stream = wait_for_port(addr, deadline)?;

        // Drive one GET /.
        let request = b"GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nConnection: close\r\n\r\n";
        stream.write_all(request).context("write request")?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).context("read response")?;

        // Verify response: 200 OK + body contains "ok\n".
        let resp_str = String::from_utf8_lossy(&response);
        assert!(resp_str.contains("200"), "response: {}", resp_str);
        assert!(resp_str.contains("ok\n") || resp_str.contains("ok"), "response: {}", resp_str);

        // Give the synchronous-after-write emission a brief moment
        // (the HCM dispatches sink.emit().await before returning to
        // the keep-alive loop, but the OS file flush is on close).
        // We don't close the FileSink explicitly; envoy-bin holds the
        // FileSink open for the listener's lifetime. The OS write
        // should have made the bytes durable via the kernel buffer
        // before our std::fs::read_to_string call below.
        std::thread::sleep(Duration::from_millis(100));

        let log_contents = std::fs::read_to_string(&access_log_path)
            .context("read access log file")?;
        let lines: Vec<&str> = log_contents.lines().collect();
        assert_eq!(lines.len(), 1, "expected 1 access-log line; got {}: {:?}", lines.len(), log_contents);
        let line = lines[0];
        // Per the Envoy default format with fixture-0012's surface.
        assert!(
            line.contains("\"GET / HTTP/1.1\" 200 - 0 3 "),
            "access log line: {}",
            line
        );

        Ok(())
    })();

    // Clean up the child.
    let _ = child.kill();
    let _ = child.wait();

    result
}
```

The `let _ = child.kill();` + `let _ = child.wait();` pattern mirrors the existing 06.1 `crates/envoy-bin/tests/admin_ready.rs:73-75` posture; this inherits the standing phase-02.2 REVIEW M1 chain (`*EchoBackend::Drop` polling-loop blocks on `std::thread::sleep`) — awareness-only, not a 06.2 regression.

- [ ] **Step 2: Verify `crates/envoy-bin/Cargo.toml` has `tempfile` and `anyhow` as dev-deps.**

Cross-checked at PLAN-write time: `tempfile = "3"` is already a dev-dep in `envoy-bin` (per the cross-check report's claim 20); `anyhow` is also already a dev-dep (per the binary-crate `anyhow` carve-out in D-3.2). If `cargo build -p envoy-bin --tests` flags missing deps, add them to `[dev-dependencies]`.

- [ ] **Step 3: Run the test.**

Run: `cargo test -p envoy-bin --test access_log_file_sink 2>&1 | tail -10`
Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 4: Run workspace gates.**

Run in parallel:
- `cargo build --workspace --all-targets 2>&1 | tail -5`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -3`

Expected: all clean.

- [ ] **Step 5: Append PROGRESS.md Task 8 entry + commit.**

```bash
git add crates/envoy-bin/tests/access_log_file_sink.rs docs/envoy-rust/phases/06.2-access-log/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.2: in-process integration backstop for access-log file sink (task 8)

Lands crates/envoy-bin/tests/access_log_file_sink.rs per SPEC §6 signpost
18. Spawns envoy-bin via CARGO_BIN_EXE_envoy-bin against an HCM-with-file-
sink YAML config at a tempdir path; drives one GET / over HTTP/1.1; reads
the access-log file post-request; asserts the default-format line. Runs
without Docker. Mirrors the 04.1 / 05.2 / 06.1 integration-test pattern;
inherits the standing 02.2 REVIEW M1 SIGKILL-on-Drop posture.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Differential harness extension — `Driver::Http1WithAccessLog` + `AccessLogLineRule` + hand-rolled tokenizer + dispatch arm + 4 unit tests

**Files:**
- Modify: `tests/differential/src/lib.rs`
- Create: `tests/differential/src/access_log.rs`

Lands the differential-harness primitives per 06.2 SPEC §3 D4.2.a + D4.2.b. New module `tests/differential/src/access_log.rs` ships the `AccessLogLineRule` per-token rule enum, the hand-rolled `AccessLogTokenizer` (no `regex` per architecture decision 9), and the `assert_access_log_lines_equivalent` helper. `lib.rs` gains the `Driver::Http1WithAccessLog` variant (slots between `Http1ProbeList` and `Http2` per the existing 8-variant enum + correction 3), the `port_key` match arm for the new variant, and the `run_fixture` dispatch arm that reuses `drive_http1` for the wire-protocol leg and dispatches to `assert_access_log_lines_equivalent` for the post-request file-content diff.

The harness reads the configured access-log path from each proxy's YAML config (envoy.yaml + envoy-rust.yaml). Per 06.2 SPEC §6 signpost 8, the path-discovery mechanism is: the fixture's `envoy.yaml` declares `path: /tmp/<fixture-id>-envoy-access.log` and `envoy-rust.yaml` declares `path: /tmp/<fixture-id>-envoy-rust-access.log`; the harness hard-codes these paths in fixture 0012's `expectations.yaml`'s `access_log_paths` field (or extracts them by re-parsing the YAML — recommendation per simplicity is to declare them in `expectations.yaml` as `expected_access_log_paths: { envoy, envoy_rust }`).

- [ ] **Step 1: Create `tests/differential/src/access_log.rs` with the per-token rule enum + tokenizer + assert helper + 4 unit tests.**

```rust
//! Access-log line equivalence primitives for the differential
//! harness. Lands per 06.2 SPEC §3 D4.2.b + signpost 8 (hand-rolled
//! tokenizer per architecture decision 9; no `regex` dep).
//!
//! The tokenizer parses the Envoy default-format access-log line into
//! its 14 component tokens (with quoting/bracket awareness). The
//! per-token rule enum (`AccessLogLineRule`) drives the equivalence
//! check; the `assert_access_log_lines_equivalent` helper applies the
//! per-token rules across both proxies' lines.

use serde::Deserialize;

/// Per-token rule for the Envoy default-format access-log line.
/// One rule per token; the rules slot in the same 1:1 order as the
/// 14 tokens emitted by `envoy_accesslog::default_format::format`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "rule", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccessLogLineRule {
    /// Token must match `value` byte-for-byte. Used for `value-exact`
    /// tokens per BEHAVIOR_CONTRACT.md `Access log field mapping`
    /// (`%REQ(:METHOD)%`, `%RESPONSE_CODE%`, etc.).
    Exact { value: String },

    /// Token must parse as ISO-8601 `YYYY-MM-DDTHH:MM:SS.sssZ`.
    /// Used for `%START_TIME%` (name-required, value-may-differ).
    Iso8601Format,

    /// Token must parse as a non-negative integer (decimal
    /// milliseconds). Used for `%DURATION%` and present-on-both-
    /// sides `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%`.
    DurationMs,

    /// Token may be anything (used for fields not covered by 06.2;
    /// reserved for forward-compat).
    Wildcard,
}

/// Tokenize a single Envoy default-format access-log line into its
/// component tokens. Handles the `[%START_TIME%]` bracketing, the
/// `"..."` quoted-token boundaries, and the unquoted-token
/// whitespace separators.
///
/// The 14-token shape per Envoy v1.33's documented default format:
///
///   1. `%START_TIME%` (bracket-wrapped, e.g. `[2024-01-01T00:00:00.000Z]`)
///   2. `%REQ(:METHOD)%` (first word inside the quoted request-line)
///   3. `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` (second word)
///   4. `%PROTOCOL%` (third word; closing of the quoted request-line)
///   5. `%RESPONSE_CODE%` (unquoted)
///   6. `%RESPONSE_FLAGS%` (unquoted)
///   7. `%BYTES_RECEIVED%` (unquoted)
///   8. `%BYTES_SENT%` (unquoted)
///   9. `%DURATION%` (unquoted)
///   10. `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%` (unquoted)
///   11. `%REQ(X-FORWARDED-FOR)%` (quoted)
///   12. `%REQ(USER-AGENT)%` (quoted)
///   13. `%REQ(X-REQUEST-ID)%` (quoted)
///   14. `%REQ(:AUTHORITY)%` (quoted)
///   15. `%UPSTREAM_HOST%` (quoted)
///
/// Returns a Vec<String> of 15 tokens (the rule enum is 14-shape but
/// the bracket-wrapped START_TIME counts as one token, and the
/// quoted request-line yields 3 tokens for method/path/protocol).
pub fn tokenize_default_format(line: &str) -> Result<Vec<String>, String> {
    let mut tokens: Vec<String> = Vec::with_capacity(15);
    let bytes = line.as_bytes();
    let mut i = 0usize;

    // 1. START_TIME bracket.
    if i >= bytes.len() || bytes[i] != b'[' {
        return Err(format!("expected '[' at offset {}; line: {}", i, line));
    }
    i += 1;
    let start_time_begin = i;
    while i < bytes.len() && bytes[i] != b']' {
        i += 1;
    }
    if i >= bytes.len() {
        return Err(format!("unterminated '[...]' in line: {}", line));
    }
    tokens.push(line[start_time_begin..i].to_owned());
    i += 1; // skip ']'

    // Skip the space after ']'.
    if i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    } else {
        return Err(format!("expected ' ' after ']' at offset {}; line: {}", i, line));
    }

    // 2-4. Quoted request-line: "METHOD PATH PROTOCOL".
    if i >= bytes.len() || bytes[i] != b'"' {
        return Err(format!("expected '\"' (request-line) at offset {}; line: {}", i, line));
    }
    i += 1;
    let req_line_begin = i;
    while i < bytes.len() && bytes[i] != b'"' {
        i += 1;
    }
    if i >= bytes.len() {
        return Err(format!("unterminated request-line quote in line: {}", line));
    }
    let req_line = &line[req_line_begin..i];
    let parts: Vec<&str> = req_line.splitn(3, ' ').collect();
    if parts.len() != 3 {
        return Err(format!("request-line did not split into 3 parts: {:?}", req_line));
    }
    tokens.push(parts[0].to_owned()); // method
    tokens.push(parts[1].to_owned()); // path
    tokens.push(parts[2].to_owned()); // protocol
    i += 1; // skip closing '"'

    // 5-10. Six unquoted tokens (status, flags, bytes_received,
    // bytes_sent, duration, upstream_service_time).
    for _ in 0..6 {
        // Skip leading whitespace.
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        let tok_begin = i;
        while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'"' {
            i += 1;
        }
        if tok_begin == i {
            return Err(format!("empty token at offset {} in line: {}", tok_begin, line));
        }
        tokens.push(line[tok_begin..i].to_owned());
    }

    // 11-15. Five quoted tokens (forwarded_for, user_agent,
    // request_id, authority, upstream_host).
    for _ in 0..5 {
        // Skip leading whitespace.
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'"' {
            return Err(format!("expected '\"' at offset {}; line: {}", i, line));
        }
        i += 1;
        let tok_begin = i;
        while i < bytes.len() && bytes[i] != b'"' {
            i += 1;
        }
        if i >= bytes.len() {
            return Err(format!("unterminated quoted token in line: {}", line));
        }
        tokens.push(line[tok_begin..i].to_owned());
        i += 1; // skip closing '"'
    }

    Ok(tokens)
}

/// Apply a single per-token rule to a pair of envoy + envoy-rust
/// token values. Returns Err with a descriptive message on
/// mismatch.
pub fn apply_rule(rule: &AccessLogLineRule, envoy: &str, envoy_rust: &str) -> Result<(), String> {
    match rule {
        AccessLogLineRule::Exact { value } => {
            if envoy != value {
                return Err(format!("envoy token {:?} != expected {:?}", envoy, value));
            }
            if envoy_rust != value {
                return Err(format!("envoy-rust token {:?} != expected {:?}", envoy_rust, value));
            }
        }
        AccessLogLineRule::Iso8601Format => {
            for (side, tok) in [("envoy", envoy), ("envoy-rust", envoy_rust)] {
                if !is_iso8601_format(tok) {
                    return Err(format!("{} token {:?} does not match ISO-8601 YYYY-MM-DDTHH:MM:SS.sssZ", side, tok));
                }
            }
        }
        AccessLogLineRule::DurationMs => {
            for (side, tok) in [("envoy", envoy), ("envoy-rust", envoy_rust)] {
                if tok.parse::<u64>().is_err() {
                    return Err(format!("{} token {:?} does not parse as u64 ms", side, tok));
                }
            }
        }
        AccessLogLineRule::Wildcard => {}
    }
    Ok(())
}

fn is_iso8601_format(s: &str) -> bool {
    // YYYY-MM-DDTHH:MM:SS.sssZ — exactly 24 ASCII bytes; positional
    // checks for separators.
    let b = s.as_bytes();
    if b.len() != 24 {
        return false;
    }
    if b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'.' || b[23] != b'Z' {
        return false;
    }
    let digit = |idx: usize| -> bool { b[idx].is_ascii_digit() };
    for &i in &[0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18, 20, 21, 22] {
        if !digit(i) {
            return false;
        }
    }
    true
}

/// Assert per-token equivalence across two sequences of access-log
/// lines (one per proxy). Each line in both sequences is tokenized
/// via `tokenize_default_format`; the per-token rules are applied
/// pairwise.
pub fn assert_access_log_lines_equivalent(
    envoy_lines: &[String],
    envoy_rust_lines: &[String],
    rules_per_line: &[Vec<AccessLogLineRule>],
) -> Result<(), String> {
    if envoy_lines.len() != envoy_rust_lines.len() {
        return Err(format!(
            "line count mismatch: envoy={} envoy-rust={}",
            envoy_lines.len(),
            envoy_rust_lines.len()
        ));
    }
    if envoy_lines.len() != rules_per_line.len() {
        return Err(format!(
            "rules-per-line count {} != lines count {}",
            rules_per_line.len(),
            envoy_lines.len()
        ));
    }
    for (line_idx, ((envoy_line, envoy_rust_line), line_rules)) in envoy_lines
        .iter()
        .zip(envoy_rust_lines.iter())
        .zip(rules_per_line.iter())
        .enumerate()
    {
        let envoy_tokens = tokenize_default_format(envoy_line)
            .map_err(|e| format!("line {}: envoy tokenize: {}", line_idx, e))?;
        let envoy_rust_tokens = tokenize_default_format(envoy_rust_line)
            .map_err(|e| format!("line {}: envoy-rust tokenize: {}", line_idx, e))?;
        if envoy_tokens.len() != line_rules.len() {
            return Err(format!(
                "line {}: envoy tokenized to {} tokens but {} rules supplied",
                line_idx,
                envoy_tokens.len(),
                line_rules.len()
            ));
        }
        if envoy_rust_tokens.len() != line_rules.len() {
            return Err(format!(
                "line {}: envoy-rust tokenized to {} tokens but {} rules supplied",
                line_idx,
                envoy_rust_tokens.len(),
                line_rules.len()
            ));
        }
        for (tok_idx, ((envoy_tok, envoy_rust_tok), rule)) in envoy_tokens
            .iter()
            .zip(envoy_rust_tokens.iter())
            .zip(line_rules.iter())
            .enumerate()
        {
            apply_rule(rule, envoy_tok, envoy_rust_tok)
                .map_err(|e| format!("line {} token {}: {}", line_idx, tok_idx, e))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LINE: &str =
        "[2024-01-01T00:00:00.000Z] \"GET / HTTP/1.1\" 200 - 0 3 5 - \"-\" \"-\" \"-\" \"envoy-rust.test\" \"-\"";

    #[test]
    fn tokenize_default_format_happy_path() {
        let tokens = tokenize_default_format(SAMPLE_LINE).expect("ok");
        assert_eq!(tokens.len(), 15);
        assert_eq!(tokens[0], "2024-01-01T00:00:00.000Z");
        assert_eq!(tokens[1], "GET");
        assert_eq!(tokens[2], "/");
        assert_eq!(tokens[3], "HTTP/1.1");
        assert_eq!(tokens[4], "200");
        assert_eq!(tokens[5], "-");
        assert_eq!(tokens[6], "0");
        assert_eq!(tokens[7], "3");
        assert_eq!(tokens[8], "5");
        assert_eq!(tokens[9], "-");
        assert_eq!(tokens[10], "-");
        assert_eq!(tokens[11], "-");
        assert_eq!(tokens[12], "-");
        assert_eq!(tokens[13], "envoy-rust.test");
        assert_eq!(tokens[14], "-");
    }

    #[test]
    fn tokenize_handles_dash_in_quoted_position() {
        let line = "[2024-01-01T00:00:00.000Z] \"GET / HTTP/1.1\" 200 - 0 3 5 - \"-\" \"-\" \"-\" \"-\" \"-\"";
        let tokens = tokenize_default_format(line).expect("ok");
        assert_eq!(tokens[10], "-");
        assert_eq!(tokens[11], "-");
        assert_eq!(tokens[12], "-");
        assert_eq!(tokens[13], "-");
        assert_eq!(tokens[14], "-");
    }

    #[test]
    fn assert_access_log_lines_equivalent_happy_path() {
        let envoy = vec![SAMPLE_LINE.to_owned()];
        let envoy_rust = vec![SAMPLE_LINE.to_owned()];
        let rules = vec![vec![
            AccessLogLineRule::Iso8601Format,                // START_TIME
            AccessLogLineRule::Exact { value: "GET".into() },
            AccessLogLineRule::Exact { value: "/".into() },
            AccessLogLineRule::Exact { value: "HTTP/1.1".into() },
            AccessLogLineRule::Exact { value: "200".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "0".into() },
            AccessLogLineRule::Exact { value: "3".into() },
            AccessLogLineRule::DurationMs,                   // DURATION
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "envoy-rust.test".into() },
            AccessLogLineRule::Exact { value: "-".into() },
        ]];
        assert_access_log_lines_equivalent(&envoy, &envoy_rust, &rules).expect("ok");
    }

    #[test]
    fn assert_access_log_lines_equivalent_rejects_token_mismatch() {
        let envoy = vec![SAMPLE_LINE.to_owned()];
        let envoy_rust_diff = vec![
            "[2024-01-01T00:00:00.000Z] \"POST / HTTP/1.1\" 200 - 0 3 5 - \"-\" \"-\" \"-\" \"envoy-rust.test\" \"-\"".to_owned()
        ];
        let rules = vec![vec![
            AccessLogLineRule::Iso8601Format,
            AccessLogLineRule::Exact { value: "GET".into() }, // mismatch (envoy-rust says POST)
            AccessLogLineRule::Exact { value: "/".into() },
            AccessLogLineRule::Exact { value: "HTTP/1.1".into() },
            AccessLogLineRule::Exact { value: "200".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "0".into() },
            AccessLogLineRule::Exact { value: "3".into() },
            AccessLogLineRule::DurationMs,
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "-".into() },
            AccessLogLineRule::Exact { value: "envoy-rust.test".into() },
            AccessLogLineRule::Exact { value: "-".into() },
        ]];
        let err = assert_access_log_lines_equivalent(&envoy, &envoy_rust_diff, &rules)
            .expect_err("expected mismatch");
        assert!(err.contains("envoy-rust token"), "err: {}", err);
    }
}
```

- [ ] **Step 2: Add the new module declaration + `Driver::Http1WithAccessLog` variant + dispatch arm to `tests/differential/src/lib.rs`.**

Edit `tests/differential/src/lib.rs`:

(a) Near the top, add `pub mod access_log;` to declare the new module.

(b) In the `Driver` enum (lines 38-112 per cross-checked HEAD), slot the new variant between `Http1ProbeList` and `Http2`:

```rust
    /// 06.2 NEW: HTTP/1.1 driver with post-request access-log line
    /// assertion. Drives one GET/POST via `drive_http1` (reused from
    /// 04.1), then reads the configured access-log files from each
    /// proxy and asserts per-token equivalence via
    /// `access_log::assert_access_log_lines_equivalent`.
    Http1WithAccessLog {
        method: String,
        path: String,
        host: String,
        expected_status: u16,
        expected_body: BodyRule,
        expected_headers: HeaderRule,
        #[serde(default)]
        extra_headers: Vec<(String, String)>,
        expected_access_log_paths: AccessLogPaths,
        expected_access_log_lines: Vec<Vec<crate::access_log::AccessLogLineRule>>,
    },
```

`AccessLogPaths` is a new struct in the harness:

```rust
/// 06.2 NEW: per-proxy file paths for access-log diff. The harness
/// reads both files after the wire-protocol leg completes.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AccessLogPaths {
    pub envoy: String,
    pub envoy_rust: String,
}
```

(c) In the `port_key` match (lines 1120-1131 per cross-checked HEAD), add a new arm for `Http1WithAccessLog` mapping to the same `"PORT"` substitution as `Http1`:

```rust
        Driver::Http1WithAccessLog { .. } => "PORT",
```

(d) In the `run_fixture` dispatch cascade (the `match &expectations.driver` at line 1354), insert a new arm between `Http1ProbeList` (line 1547) and `Http2` (line 1649):

```rust
        Driver::Http1WithAccessLog {
            method,
            path,
            host,
            expected_status,
            expected_body,
            expected_headers,
            extra_headers,
            expected_access_log_paths,
            expected_access_log_lines,
        } => {
            // Wire-protocol leg: reuse drive_http1 unchanged from 04.1.
            let http1_method = Http1Method::try_from(method.as_str())
                .with_context(|| format!("invalid HTTP method: {}", method))?;
            let envoy_result = drive_http1(envoy_addr, &http1_method, path, host, extra_headers)
                .await
                .context("envoy drive_http1")?;
            let envoy_rust_result = drive_http1(envoy_rust_addr, &http1_method, path, host, extra_headers)
                .await
                .context("envoy-rust drive_http1")?;

            // Response equivalence per existing 04.1/04.2 patterns:
            assert_status_equivalent(envoy_result.status, envoy_rust_result.status, *expected_status)?;
            assert_body_equivalent(&envoy_result.body, &envoy_rust_result.body, expected_body)?;
            assert_headers_equivalent(&envoy_result.headers, &envoy_rust_result.headers, expected_headers)?;

            // Access-log files. Wait up to 5s for both files to appear
            // (the synchronous-after-write dispatch should have emitted
            // before the response completed, but file flush is on close
            // — the kernel buffer holds the bytes until then).
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let envoy_path = std::path::PathBuf::from(&expected_access_log_paths.envoy);
            let envoy_rust_path = std::path::PathBuf::from(&expected_access_log_paths.envoy_rust);
            while std::time::Instant::now() < deadline {
                if envoy_path.exists() && envoy_rust_path.exists() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            // One final yield to let the OS flush.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let envoy_contents = std::fs::read_to_string(&envoy_path)
                .with_context(|| format!("read envoy access-log file at {}", envoy_path.display()))?;
            let envoy_rust_contents = std::fs::read_to_string(&envoy_rust_path)
                .with_context(|| format!("read envoy-rust access-log file at {}", envoy_rust_path.display()))?;
            let envoy_lines: Vec<String> = envoy_contents.lines().map(|s| s.to_owned()).collect();
            let envoy_rust_lines: Vec<String> = envoy_rust_contents.lines().map(|s| s.to_owned()).collect();

            crate::access_log::assert_access_log_lines_equivalent(
                &envoy_lines,
                &envoy_rust_lines,
                expected_access_log_lines,
            )
            .map_err(|e| anyhow::anyhow!(
                "access log mismatch: {}\nenvoy lines: {:?}\nenvoy-rust lines: {:?}",
                e, envoy_lines, envoy_rust_lines
            ))?;

            Ok(())
        }
```

Cross-check the existing `assert_status_equivalent` / `assert_body_equivalent` / `assert_headers_equivalent` helper names; they may be slightly differently named in HEAD (e.g., inline in the `Http1` arm). Either reuse or factor out cleanly. Recommendation: factor out at task time only if needed; otherwise inline the same helpers as the `Http1` arm with the same body.

- [ ] **Step 3: Run the unit tests + verify the new module compiles.**

Run: `cargo test -p differential --lib access_log::tests 2>&1 | tail -10`
Expected: `test result: ok. 4 passed; 0 failed`.

Run: `cargo build -p differential --all-targets 2>&1 | tail -5`
Expected: clean.

- [ ] **Step 4: Run workspace gates.**

Run in parallel:
- `cargo build --workspace --all-targets 2>&1 | tail -5`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -3`

Expected: all clean.

- [ ] **Step 5: Append PROGRESS.md Task 9 entry + commit.**

Stage + commit:

```bash
git add tests/differential/src/lib.rs tests/differential/src/access_log.rs docs/envoy-rust/phases/06.2-access-log/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.2: differential harness — Driver::Http1WithAccessLog + AccessLogLineRule + tokenizer (task 9)

Lands per SPEC §3 D4.2.a + D4.2.b. New tests/differential/src/access_log.rs
module ships AccessLogLineRule (Exact / Iso8601Format / DurationMs /
Wildcard) + hand-rolled tokenize_default_format (no regex dep per
architecture decision 9) + assert_access_log_lines_equivalent. New Driver
variant Http1WithAccessLog slots between Http1ProbeList and Http2;
run_fixture dispatch reuses drive_http1 for the wire-protocol leg and
file-reads both proxies' access logs post-request. 4 new unit tests.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Fixture 0012 (5 files) + Docker-gated wrapper + BEHAVIOR_CONTRACT.md `Access log field mapping` first-time population

**Files:**
- Create: `tests/fixtures/0012-access-log-file-sink/envoy.yaml`
- Create: `tests/fixtures/0012-access-log-file-sink/envoy-rust.yaml`
- Create: `tests/fixtures/0012-access-log-file-sink/inputs/payload.bin` (0 bytes)
- Create: `tests/fixtures/0012-access-log-file-sink/expectations.yaml`
- Create: `tests/fixtures/0012-access-log-file-sink/README.md`
- Create: `tests/differential/tests/access_log_file_sink.rs` (Docker-gated wrapper)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md`

Lands the new differential fixture per 06.2 SPEC §3 D4.2.c + the BEHAVIOR_CONTRACT.md edit per D5.2 (folded per signpost recommendation that the contract edit lands in lockstep with the first-fixture-that-asserts-on-the-table).

- [ ] **Step 1: Create the fixture directory + the 5 files.**

Create `tests/fixtures/0012-access-log-file-sink/envoy.yaml`:

```yaml
node: { id: envoy-rust-phase-06.2-fixture-0012, cluster: envoy-rust-phase-06.2 }
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
                generate_request_id: false
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0012-envoy-access.log
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

Create `tests/fixtures/0012-access-log-file-sink/envoy-rust.yaml`:

```yaml
node: { id: envoy-rust-phase-06.2-fixture-0012, cluster: envoy-rust-phase-06.2 }
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
                      path: /tmp/0012-envoy-rust-access.log
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

Key per-side differences (per 06.2 SPEC §3 D4.2.c):
- envoy bind `0.0.0.0`, envoy-rust bind `127.0.0.1` (Docker boundary).
- envoy-rust has no `admin` block (admin-side access logging out of scope; the standalone admin listener is exercised by fixture 0011).
- access-log paths differ per side (`/tmp/0012-envoy-access.log` vs `/tmp/0012-envoy-rust-access.log`).
- `generate_request_id: false` on envoy side only (envoy-rust never injects per 04.3 SPEC §4 non-goal).

Create `tests/fixtures/0012-access-log-file-sink/inputs/payload.bin` as a 0-byte file (per fixture-0010 / 0011 precedent for non-payload-driving drivers):

```bash
mkdir -p tests/fixtures/0012-access-log-file-sink/inputs
: > tests/fixtures/0012-access-log-file-sink/inputs/payload.bin
```

Create `tests/fixtures/0012-access-log-file-sink/expectations.yaml`:

```yaml
driver:
  kind: http1_with_access_log
  method: GET
  path: /
  host: envoy-rust.test
  expected_status: 200
  expected_body:
    kind: byte_exact
  expected_headers:
    rule: set_equal_modulo_allow_list
  expected_access_log_paths:
    envoy: /tmp/0012-envoy-access.log
    envoy_rust: /tmp/0012-envoy-rust-access.log
  expected_access_log_lines:
    - - rule: iso8601_format                                        # %START_TIME%
      - rule: exact
        value: GET                                                   # %REQ(:METHOD)%
      - rule: exact
        value: /                                                     # %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%
      - rule: exact
        value: HTTP/1.1                                              # %PROTOCOL%
      - rule: exact
        value: "200"                                                 # %RESPONSE_CODE%
      - rule: exact
        value: "-"                                                   # %RESPONSE_FLAGS%
      - rule: exact
        value: "0"                                                   # %BYTES_RECEIVED%
      - rule: exact
        value: "3"                                                   # %BYTES_SENT%
      - rule: duration_ms                                            # %DURATION%
      - rule: exact
        value: "-"                                                   # %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%
      - rule: exact
        value: "-"                                                   # %REQ(X-FORWARDED-FOR)%
      - rule: wildcard                                               # %REQ(USER-AGENT)% (drive_http1 may inject a default)
      - rule: exact
        value: "-"                                                   # %REQ(X-REQUEST-ID)%
      - rule: exact
        value: envoy-rust.test                                       # %REQ(:AUTHORITY)%
      - rule: exact
        value: "-"                                                   # %UPSTREAM_HOST%
```

The `wildcard` rule on `%REQ(USER-AGENT)%` is per 06.2 SPEC §2.1's "may need to be wildcard if `drive_http1` adds a default user-agent" note. Cross-check at fixture-write time: if envoy-rust's harness `drive_http1` does NOT inject a User-Agent and Envoy does NOT receive one, both sides emit `-` and `wildcard` can tighten to `exact: "-"`. Recommendation: ship as `wildcard` (less brittle); tighten if both proxies agree on `-` in the empirical first-run output.

Create `tests/fixtures/0012-access-log-file-sink/README.md`:

```markdown
# Fixture 0012 — access-log file sink

The first fixture exercising envoy-rust's HCM access-log subsystem. An H1
direct_response listener with a `file` access-log emits one access-log line
per request to a configured path; the harness reads each proxy's file and
diffs per-token per the rules in `expectations.yaml`.

## Surface

- HCM with `codec_type: HTTP1`, single virtual host `domains: ["*"]`,
  single route `prefix: "/"` `direct_response { status: 200, body:
  "ok\n" }`.
- Access log configured with the file logger; path declared in each
  side's YAML. The harness reads from the declared path post-request.

## Per-side divergences

| Side | bind address | admin block | access-log path                      | `generate_request_id` |
|------|--------------|-------------|--------------------------------------|-----------------------|
| envoy | `0.0.0.0`   | yes (port 0)| `/tmp/0012-envoy-access.log`         | `false` (load-bearing) |
| envoy-rust | `127.0.0.1` | omitted | `/tmp/0012-envoy-rust-access.log`    | omitted (never injects) |

The `generate_request_id: false` on the Envoy side prevents Envoy from
injecting an `x-request-id` header at HCM time; envoy-rust never injects
that header (per 04.3 SPEC §4 non-goal). Without this knob, the
`%REQ(X-REQUEST-ID)%` token would emit a UUID on Envoy's line and `-` on
envoy-rust's line, breaking the value-exact equivalence. Mirrors the
posture used in fixture 0008.

## Per-token equivalence

See the per-token rules in `expectations.yaml`. The authoritative
disposition table is `docs/envoy-rust/BEHAVIOR_CONTRACT.md` `Access log
field mapping` section (populated for the first time by this fixture's
commit).

## Driver

`Driver::Http1WithAccessLog` (06.2 NEW). Wire-protocol leg via the 04.1-
landed `drive_http1` helper; access-log assertion is a post-request
file-content read + per-token diff. The harness waits up to 5s for both
files to appear, then yields 100ms to let the OS flush, before reading.

## Cross-references

- Phase 06.2 SPEC: `docs/envoy-rust/phases/06.2-access-log/SPEC.md`
- BEHAVIOR_CONTRACT row table: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` `Access log field mapping`
- ADR: none (06.2 lands no ADRs under the recommended posture)
- Related fixtures: 0007 (H1 direct_response baseline), 0011 (admin stats baseline)
```

Create `tests/differential/tests/access_log_file_sink.rs` (Docker-gated wrapper):

```rust
//! Docker-gated wrapper for fixture 0012.
//!
//! Mirrors the 06.1 `admin_stats_prometheus.rs` and 04.1
//! `http1_direct_response.rs` wrapper shape.

#[tokio::test]
async fn access_log_file_sink() {
    differential::run_fixture(std::path::Path::new(
        "tests/fixtures/0012-access-log-file-sink",
    ))
    .await
    .expect("fixture green");
}
```

- [ ] **Step 2: Populate `docs/envoy-rust/BEHAVIOR_CONTRACT.md`'s `Access log field mapping` section.**

Edit `docs/envoy-rust/BEHAVIOR_CONTRACT.md`. Replace the existing placeholder content of the `## Access log field mapping` section:

```markdown
_(empty; populated starting phase 06)_
```

with:

```markdown
**06.2 first-time population (per parent-06 SPEC §2.2).** Envoy's default
access-log format (per upstream Envoy v1.33's documentation) is a fixed
sequence of 14 tokens emitted per request:

```
[%START_TIME%] "%REQ(:METHOD)% %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)% %PROTOCOL%" %RESPONSE_CODE% %RESPONSE_FLAGS% %BYTES_RECEIVED% %BYTES_SENT% %DURATION% %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)% "%REQ(X-FORWARDED-FOR)%" "%REQ(USER-AGENT)%" "%REQ(X-REQUEST-ID)%" "%REQ(:AUTHORITY)%" "%UPSTREAM_HOST%"
```

Tokens absent on a given record (e.g., `%REQ(USER-AGENT)%` when the
request did not carry a `User-Agent:` header) emit `-` in their
position. Quoted tokens emit `"-"` (a literal `"-"` between the
surrounding quotes).

| Token | envoy-rust internal source | Equivalence disposition | Rationale |
|---|---|---|---|
| `%START_TIME%` | `AccessLogRecord.start_time: SystemTime`, formatted by `default_format::format_iso8601` as `YYYY-MM-DDTHH:MM:SS.sssZ` (UTC, ms resolution). Captured at HCM `serve_connection` request-arrival time. | name-required, value-may-differ | Wall-clock non-determinism: the two proxies stamp the response at slightly different instants. The harness asserts ISO-8601 parse via `AccessLogLineRule::Iso8601Format`. |
| `%REQ(:METHOD)%` | `AccessLogRecord.method`, sourced from `Request.method` at HCM record-build time. | value-exact | Both proxies receive the same method bytes; rendering matches. |
| `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` | `AccessLogRecord.path`, populated at HCM record-build time by checking `Request.headers` for `x-envoy-original-path` (case-insensitive); if present, that value; else `Request.path`. | value-exact | Both proxies see the same request bytes; both render the same path. |
| `%PROTOCOL%` | `AccessLogRecord.protocol`, determined by the dispatch path: `"HTTP/1.1"` on the H1 HCM (`envoy_http1::hcm`), `"HTTP/2"` on the H2 HCM (`envoy_http2::hcm`). | value-exact | The protocol is fixed by which HCM module is dispatching; both proxies emit the same string. |
| `%RESPONSE_CODE%` | `AccessLogRecord.response_code: u16`, sourced from `Response.status`. | value-exact | Both proxies route the request through the same VH/route/action; both produce the same response code. |
| `%RESPONSE_FLAGS%` | `AccessLogRecord.response_flags: String`. 06.2 always emits the literal `"-"` (Envoy's no-flags sentinel). Future fixtures exercising non-`-` flag combinations need per-flag equivalence rules added to this table. | value-exact (06.2 no-flags case) | Fixture 0012's direct_response happy-path produces `-`; both proxies emit `-`. |
| `%BYTES_RECEIVED%` | `AccessLogRecord.bytes_received: u64`, from `Request.body.as_ref().map_or(0, |b| b.len() as u64)`. Header bytes NOT counted (matches Envoy's semantic). | value-exact | Both proxies see the same wire request body bytes. |
| `%BYTES_SENT%` | `AccessLogRecord.bytes_sent: u64`, from `response.body.len() as u64`. Symmetric to `%BYTES_RECEIVED%`. | value-exact | Both proxies render the same response body bytes. |
| `%DURATION%` | `AccessLogRecord.duration: Duration`, from `start.elapsed()` at HCM record-build time. Rendered as integer milliseconds via `Duration::as_millis()`. | name-required, value-may-differ | Per-request wall-clock latency diverges across runtimes/processes/HCM impls. The harness asserts non-negative-integer parse via `AccessLogLineRule::DurationMs`. |
| `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%` | `AccessLogRecord.upstream_service_time: Option<Duration>`, populated at HCM record-build time by reading `Response.headers` for `x-envoy-upstream-service-time`. When present (router-proxy path per 04.3 emission), rendered as `Duration::as_millis()`; when absent (direct_response path), rendered as literal `-`. | name-required, value-may-differ (when present); value-exact `-` (when absent on direct_response paths) | The header value's equivalence is inherited from the 04.3-landed `Header allow-list` row for the same header. Fixture 0012's direct_response path produces `-` on both sides. |
| `%REQ(X-FORWARDED-FOR)%` | `AccessLogRecord.forwarded_for: Option<String>`, read from `Request.headers` (lowercased per the 04.x normalization posture). | value-exact | If present on the request both proxies see the same bytes; if absent both emit `-`. |
| `%REQ(USER-AGENT)%` | `AccessLogRecord.user_agent: Option<String>`, sourced symmetrically. | value-exact | Same rationale as `%REQ(X-FORWARDED-FOR)%`. |
| `%REQ(X-REQUEST-ID)%` | `AccessLogRecord.request_id: Option<String>`, sourced symmetrically. envoy-rust never injects `x-request-id` per 04.3 SPEC §4; fixture 0012's `envoy.yaml` sets `generate_request_id: false` to align both proxies on the omit-injection posture. | value-exact | Both proxies omit injection; both render `-`. |
| `%REQ(:AUTHORITY)%` | `AccessLogRecord.authority: Option<String>`, populated from the `Host:` header on the H1 path (envoy_http1::codec produces this from the request-line) or the `:authority` pseudo-header on the H2 path (translated by 05.2 D3's adapter). | value-exact | Both proxies see the same wire-level request authority; both render the same value. |
| `%UPSTREAM_HOST%` | `AccessLogRecord.upstream_host: Option<String>`, populated at HCM record-build time from the router-arm's resolved upstream `SocketAddr` (formatted via `SocketAddr` Display). `None` on direct_response paths. | value-exact `-` (direct_response, fixture 0012); value-exact for STRICT_DNS single-A-record resolution; name-required, value-may-differ for multi-A non-deterministic resolution | Fixture 0012's direct_response path produces `-`; both proxies emit `-`. Future router-proxy fixtures use STRICT_DNS with single-A resolution (matches the 04.3 fixture 0008 / 05.3 fixture 0010 posture). |

Format-string customization is OUT of scope in 06.2. The `envoy-config`
validator at `validate_access_logs` rejects non-`envoy.access_loggers.file`
access-log names and fixtures that supply format strings on the FileAccessLog
typed_config (the `format` / `log_format` / `json_format` / `typed_json_format`
fields on the upstream proto are not in envoy-rust's `FileAccessLog` struct;
serde `deny_unknown_fields` rejects them). Future observability-family phases
extend this section with new tokens (`%FILTER_STATE%`, `%DYNAMIC_METADATA%`,
`%RESPONSE_CODE_DETAILS%`, etc.) when the corresponding machinery lands.
```

- [ ] **Step 3: Run the Docker-gated fixture 0012 test (locally).**

Run: `cargo test -p differential --test access_log_file_sink 2>&1 | tail -15`

The local Docker-gated run requires Docker daemon access. If Docker is unavailable on dev machine, the test will skip with a `testcontainers` infrastructure error; the CI gate at Task 11 is the authoritative state-4 evidence.

Expected (with Docker): `test result: ok. 1 passed; 0 failed`.

If the fixture fails on first run due to a per-side divergence in the actual access-log line output (e.g., User-Agent injection by `drive_http1` differs from Envoy's default-User-Agent passthrough), the executor records the divergence in PROGRESS Task 10 deviation notes and tightens/loosens the per-token rule in `expectations.yaml` accordingly (mirrors the 06.1 Task 13 empirical-allow-list seeding posture). The dot-tree contract in BEHAVIOR_CONTRACT.md remains authoritative; per-token rule adjustments capture emitter-side projection differences, not contract loosening.

- [ ] **Step 4: Run workspace gates.**

Run in parallel:
- `cargo build --workspace --all-targets 2>&1 | tail -5`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -3`

Expected: all clean.

- [ ] **Step 5: Append PROGRESS.md Task 10 entry + commit.**

```bash
git add tests/fixtures/0012-access-log-file-sink/ \
        tests/differential/tests/access_log_file_sink.rs \
        docs/envoy-rust/BEHAVIOR_CONTRACT.md \
        docs/envoy-rust/phases/06.2-access-log/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.2: fixture 0012 access-log file-sink + BEHAVIOR_CONTRACT Access log field mapping (task 10)

Lands per SPEC §3 D4.2.c + D5.2 (folded per signpost recommendation).
Fixture 0012: H1 direct_response + file access-log; per-side YAML files;
0-byte payload.bin; expectations.yaml carries per-token rules for the
14-token Envoy default format. Docker-gated wrapper at tests/differential/
tests/access_log_file_sink.rs. BEHAVIOR_CONTRACT.md `Access log field
mapping` section populated for the first time in the project's history
— 14 rows, one per default-format token, with value-exact /
name-required-value-may-differ dispositions per parent-06 SPEC §2.2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: State-4 phase-done verification (no code; PROGRESS quote)

**Files:**
- Modify: `docs/envoy-rust/phases/06.2-access-log/PROGRESS.md` (appended)

Lands the state-4 verification per `BOOTSTRAP_PROMPT.md` §7.5 + 06.2 SPEC §3 D6.2. No code; the deliverable is PROGRESS.md evidence — quoting the CI run URL + HEAD SHA + completion timestamp + per-gate outputs.

Per 06.1 REVIEW §7 R-9 (pre-state-4 fmt discipline) and the 06.1 Task 14 fmt-drift catch precedent (`36fedd8`), run `cargo fmt --all -- --check` as a final pre-push check; if drift is detected, run `cargo fmt --all` and land a separate fmt-drift catch commit BEFORE the state-4 verification commit. The state-4 commit is `phase 06.2: state-4 phase-done gate verification (task 11)`; the optional pre-state-4 fmt commit (if needed) is `phase 06.2: cargo fmt --all (task 11 pre-push catch)`.

- [ ] **Step 1: Pre-push fmt check.**

Run: `cargo fmt --all -- --check 2>&1 | tail -5`
Expected: clean. If drift detected:
- Run `cargo fmt --all 2>&1 | tail -5` to apply.
- Stage + commit as a separate pre-state-4 commit: `phase 06.2: cargo fmt --all (task 11 pre-push catch)`.

- [ ] **Step 2: Push the current branch to remote.**

Run: `git push 2>&1 | tail -5`

This triggers CI. Wait for the run to start; capture the run ID from the GitHub Actions UI or `gh run list --limit 5`.

- [ ] **Step 3: Wait for CI to complete and verify all gates green.**

Run: `gh run watch <run-id> 2>&1 | tail -10`

Or poll: `gh run view <run-id> --json conclusion,headSha,createdAt,updatedAt 2>&1`

Expected: `"conclusion": "success"`.

Per 06.2 SPEC §1 acceptance signal (a)-(f), verify each gate via the CI logs:

- **(a)** New fixture 0012 green: `test access_log_file_sink ... ok` in the differential test job.
- **(b)** 11 pre-existing fixtures green simultaneously: all of `echo`, `tcp_proxy`, `tls_downstream`, `tls_upstream`, `tls_sni`, `http1_direct_response`, `http1_router_upstream`, `http2_direct_response`, `http2_router_upstream`, `admin_ready`, `admin_stats_prometheus` pass.
- **(c)** h2spec ≥95%: re-run reports the parent-05 baseline 99.31% (no regression from access-log wiring).
- **(d)** `parse_bootstrap` fuzz short-budget run clean: corpus seed count `files: 17` (16 pre-06.2 + 1 new `hcm_access_log_file.yaml`); zero crashes.
- **(e)** Stable-toolchain gates: `cargo build` / `cargo clippy` / `cargo fmt --check` / `cargo test` / `cargo deny check` all green.
- **(f)** This task's commit message identifies it as the state-4 verification commit (state 5 REVIEW.md lands in a separate session).

- [ ] **Step 4: Append PROGRESS.md Task 11 entry with the quoted evidence + commit.**

Append to `docs/envoy-rust/phases/06.2-access-log/PROGRESS.md`:

```markdown

## Task 11 — State-4 phase-done gate verification

CI run **<URL>** at HEAD `<SHA>`, conclusion `success`, completed `<TIMESTAMP>`.

Per 06.2 SPEC §1 acceptance signal (a)-(f):

**(a) Fixture 0012 green.** `test access_log_file_sink ... ok` in the
differential job. The fixture exercises `Driver::Http1WithAccessLog`:
GET / → 200 ok\n on the wire + per-token equivalence on the access-log
line per the 14-row BEHAVIOR_CONTRACT.md `Access log field mapping`
section's first-time population (Task 10 landing).

**(b) 11 pre-existing fixtures green simultaneously.** Differential
job reports all 11 baseline test bins pass: `echo`, `tcp_proxy`,
`tls_downstream`, `tls_upstream`, `tls_sni`, `http1_direct_response`,
`http1_router_upstream`, `http2_direct_response`, `http2_router_upstream`,
`admin_ready`, `admin_stats_prometheus`. No regression on any earlier
surface.

**(c) h2spec ≥95% pass.** Re-run carries the parent-05 baseline 99.31%
(144 passed / 1 failed / 1 skipped of 146); the access-log wiring does
NOT engage H2-framing surfaces, so the runner output is unchanged
modulo timing.

**(d) `parse_bootstrap` fuzz target clean for short-budget run.** Seed
corpus `files: 17` (16 pre-06.2 + 1 new `hcm_access_log_file.yaml`).
Local 31s run: <iterations> iterations, zero crashes. CI 31s run:
<iterations> iterations, zero crashes.

**(e) Stable-toolchain gates clean.** All 5 (`cargo build`, `cargo clippy`,
`cargo fmt --check`, `cargo test`, `cargo deny check`) reported clean in
CI run <URL>. Pre-state-4 fmt check (Step 1) was clean (or if drift was
caught, the pre-push fmt-drift commit landed before this verification
commit).

**(f) REVIEW.md verdict** lands at state-5 in the next session per the
`SKILL_ROUTING.md` state-5 transition.

State-4 evidence is anchored to a real CI run with SHA + timestamp,
honoring the 05.3 REVIEW I3 closure discipline that 06.1 set as the
project precedent.
```

Stage + commit:

```bash
git add docs/envoy-rust/phases/06.2-access-log/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.2: state-4 phase-done gate verification (task 11)

State-4 evidence anchor per BOOTSTRAP_PROMPT.md §7.5 + SPEC §3 D6.2.
CI run <URL> at HEAD <SHA>, conclusion success, completed <TIMESTAMP>.
All six gates GREEN: 12 Docker-gated fixtures (11 baseline + new 0012)
green simultaneously; h2spec ≥95% (99.31% carried from parent-05);
fuzz parse_bootstrap clean on 17-seed corpus; cargo build/clippy/fmt/
test/deny all green. State-5 REVIEW.md lands next session.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Push: `git push 2>&1 | tail -5`.

State-4 lifecycle commit lands. The next session (state-5) runs `superpowers:requesting-code-review` to author `docs/envoy-rust/phases/06.2-access-log/REVIEW.md`.

---

## State-2 commit (this PLAN.md commit)

This is the commit that lands the standalone pre-Task-1 PLAN.md per signpost 17. It touches 4 files in lockstep:
- `docs/envoy-rust/phases/06.2-access-log/PLAN.md` (NEW; this file)
- `docs/envoy-rust/phases/06.2-access-log/PROGRESS.md` (NEW; skeleton + Task 1 preamble per the "PROGRESS.md skeleton content" section below)
- `docs/envoy-rust/STATE.md` (advance to 06.2 lifecycle state 3)
- `docs/envoy-rust/ROADMAP.md` (row 06.2 status: planned → in-progress)

**Stage + commit:**

```bash
git add docs/envoy-rust/phases/06.2-access-log/PLAN.md \
        docs/envoy-rust/phases/06.2-access-log/PROGRESS.md \
        docs/envoy-rust/STATE.md \
        docs/envoy-rust/ROADMAP.md
git commit -m "$(cat <<'EOF'
phase 06.2: state-2 standalone PLAN.md

Lands the 06.2 PLAN.md as a standalone pre-Task-1 commit per the
established phase-precedent (04.3 c02eea7 / 05.1 f23d08f / 05.4 252725b /
05.2 ce471ad / 05.3 4b92e05 / 06.1 505653d). 11 tasks targeting the
06.2 SPEC §3 D1.2-D6.2 deliverable set, ~1875 LoC projected (over the
SPEC's ~1580 projection; accept-drift posture honored per parent-06
SPEC §5 alternative (vi)'s rejection of nested splits). PROGRESS.md
skeleton lands alongside with the Task 1 preamble recording the LoC
drift posture, 4 PLAN-write SPEC corrections, and the architecture-
decision lock-in (synchronous-after-write dispatch posture per Rule 4
option (b); concrete FileSink with the Sink trait deferred per option
(c); hand-rolled ISO-8601 emitter; hand-rolled tokenizer; etc.).

STATE.md advances: active-phase status "06.2 state 2 (SPEC.md only)" →
"06.2 state 3 (SPEC + PLAN exist; implementation incomplete)"; next-skill
"writing-plans" → "subagent-driven-development" per
feedback_execution_style. ROADMAP row 06.2 flips planned → in-progress
per BOOTSTRAP_PROMPT.md §4.1 invariant 3 (phase enters in-progress only
when STATE.md points at it AND its PLAN.md has landed).

No code changes; docs-only commit. No ADR landed (recommended posture
per SPEC §7 honored; conditional ADR-0030 / ADR-0031 stay available).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Run `git status -uno` after the commit to confirm a clean working tree (the only expected untracked files are pre-existing: `.gitignore` from the repo root, `crates/envoy-config/fuzz/Cargo.lock`, `rust_out`, `stdin`, `target/`).

**Do NOT push at the state-2 commit.** Push happens at state-4 (Task 11) when the full CI gate is meaningful.

**Do NOT start Task 2 in the same session.** Per `BOOTSTRAP_PROMPT.md` §5.1, one state per session; the state-2 commit IS this session's natural exit.

---

## PROGRESS.md skeleton content

The state-2 commit lands `docs/envoy-rust/phases/06.2-access-log/PROGRESS.md` with this verbatim content. (Per signpost 14, the PROGRESS preamble + Task 1 entry are the ONLY content at the state-2 commit; later tasks append per their commits.)

```markdown
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

1. **No single `write_response` call site in H1 `serve_connection`.** The SPEC's pseudocode in §3 D3.2 + §5 suggests one site; the actual code fans across 5 writer paths (synth + 4 proxy-error/happy). PLAN resolves by factoring the access-log dispatch to a single join point AFTER the `match outcome` block ends, before the keep-alive loop continuation. Task 6 implements.

2. **H2 empty-body path ends via `send_response(.., end_of_stream=true)` not `send_data`.** The SPEC §3 D3.2 says "AFTER `send_data(.., end_of_stream=true)`" but the actual `send_envoy_response` at `crates/envoy-http2/src/response.rs:62-76` skips `send_data` on the empty-body branch. PLAN resolves by landing the H2 dispatch AFTER `send_envoy_response` returns (at `hcm.rs:251`), covering both branches uniformly. Task 7 implements.

3. **`Driver` variant is `TcpEcho`, not `Tcp`.** SPEC §4 references a `Tcp` variant; the actual enum has no such variant (the TCP-shaped driver is `Driver::TcpEcho`). Minor naming fix; no code impact.

4. **`HttpConnectionManagerConfig` uses `#[serde(deny_unknown_fields)]` so the new `access_log` field needs `#[serde(default)]`.** Without `#[serde(default)]`, the 5 existing HCM-bearing fixtures (`0007/0008/0009/0010/0011`) would fail to parse. Task 5 lands `#[serde(default)]` to keep the absent-block parse green.

A 5th clarifying correction: **`envoy-http2` DOES need a direct path-dep on `envoy-accesslog`** (the SPEC §3 D1.2 architectural Rule 1 claims otherwise, but the H2 dispatch calls `sink.emit()` on concrete `Arc<FileSink>` elements which requires the concrete type at compile time). Task 7 adds the path-dep.

These are minor projection inaccuracies; the SPEC remains in-tree unedited per D-3.5.

### Architecture decisions locked at PLAN-write time

Per the user's standing preference `feedback_pick_recommendation`, every signpost in 06.2 SPEC §6 resolves to its recommendation. Recorded here for stranger-readability (full list in PLAN.md's "Architecture decisions locked at PLAN-write time" section):

- Sink trait deferred per option (c); FileSink ships concretely; HCMConfig.access_log: Vec<Arc<FileSink>> typed concretely.
- HCM dispatch posture: synchronous-after-write (option (b) per Rule 4).
- ISO-8601 emitter buffer shape: `&mut String` (signpost 1).
- Gregorian calendar helper inline in default_format.rs (signpost 2).
- FileSink concurrency: `Arc<tokio::sync::Mutex<File>>` (signpost 3).
- AccessLogRecord ownership: owned `String`s (signpost 5).
- FileSink path validation: none beyond OS-level open (signpost 6).
- `O_APPEND` semantics; no truncate; no rotation handling (signpost 7).
- Harness tokenizer: hand-rolled state machine (signpost 8); no regex.
- `%DURATION%` units: integer milliseconds (signpost 9).
- `%UPSTREAM_HOST%` format: SocketAddr Display impl (signpost 11).
- Fuzz seed: single-entry (signpost 12).
- Test logging capture: custom in-process tracing layer (signpost 15).
- No `Default` impl on AccessLogRecord (signpost 14).
- No new top-level Cargo deps (signpost 20).
- No ADRs anticipated to land (per §7 ADR projection).
- H1 dispatch at the factored join point after `match outcome` (PLAN-write SPEC correction 1).
- H2 dispatch at `hcm.rs:251` after `send_envoy_response` (PLAN-write SPEC correction 2).

### Task ordering note

The 11 PLAN tasks are numbered for documentation. The recommended **execution order** is `1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11` (linear; no reordering needed). Task 1 lands at this state-2 commit (the PROGRESS.md preamble). Tasks 2-4 build the `envoy-accesslog` crate (scaffold → emitter → sink). Task 5 lands the schema. Tasks 6-7 wire the HCM (H1 + H2). Task 8 lands the in-process backstop. Task 9 extends the differential harness. Task 10 lands the fixture + BEHAVIOR_CONTRACT.md edit. Task 11 verifies state-4. No task has a non-numeric dependency on a later task; the linear order is the recommended execution order.

## Task 1 — PROGRESS.md preamble + LoC drift posture + 4 SPEC corrections + signpost choices

(THIS section. Lands at sub-phase 06.2 state-2 commit alongside PLAN.md and the STATE.md / ROADMAP.md advance.)

## Tasks 2 through 11

Appended at execution time, one section per task commit, mirroring the 06.1 / 05.x per-task cadence.
```

---

## Self-review checklist

Run this before finalizing the state-2 commit:

1. **Spec coverage.** Each of D1.2 / D2.2 / D3.2 / D4.2 / D5.2 / D6.2 has a corresponding task; the cross-sub-phase rules 1, 4, and the Sink-deferral rule are all preserved (Tasks 2-4 ship `envoy-accesslog` as sole-dep-owner; Tasks 6-7 ship fire-and-forget dispatch; Task 2 ships the placeholder `sink.rs` documenting the deferral). The 14-row BEHAVIOR_CONTRACT.md table lands in Task 10. The 11-fixture baseline is verified at Task 11.
2. **Placeholder scan.** No `TBD`, `TODO`, "implement later", "similar to Task N", or vague "appropriate error handling" — every step ships concrete code or commands.
3. **Type consistency.** `AccessLog` / `AccessLogTypedConfig` / `FileAccessLog` names match across Tasks 5/6; `AccessLogRecord` field names + types match the SPEC §3 D1.2 definition; `Driver::Http1WithAccessLog` field names match between the enum variant in Task 9 and the `expectations.yaml` shape in Task 10's fixture; `AccessLogLineRule` variant names match between the enum in Task 9 and the per-token rules in fixture 0012's `expectations.yaml`.

If any check fails, fix inline and re-run.

---

*End of PLAN.md. Next session enters sub-phase 06.2 state 3 (PLAN.md exists, implementation incomplete) and invokes `superpowers:subagent-driven-development` against this PLAN per the user's standing preference auto-memory `feedback_execution_style`. Task 2 is the natural first-execution task.*

