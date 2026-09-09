# Phase 114 — SPEC

**Observability family: the `grpc_status_filter` access-log filter arm** — the **SEVENTH** of upstream Envoy's twelve `envoy.config.accesslog.v3.AccessLogFilter` `oneof` arms, gating a sink's per-record log emission on the request's effective gRPC status code. It lands the `GrpcStatusFilter` config surface (`statuses` + `exclude`), the case-insensitive canonical-name/numeric token validator, an **UNGATED** effective-gRPC-status derivation on the access-log record, and the fifth widening of the `should_log` predicate signature. Witnessed by NEW cluster-free, backend-free differential fixture `0094-accesslog-grpc-status-filter` through the **EXISTING** `Driver::Http1AccessLogByteExact` — zero new driver, zero new dependency, zero new fuzz target, zero new workspace crate.

- **Phase id:** `114`
- **Directory:** `docs/envoy-rust/phases/114-accesslog-grpc-status-filter/`
- **Depends on:** `06` (the access-log subsystem), `70` (the access-log FILTER subsystem and its harness support), `110` (the H1 HTTP→gRPC status map), `113` (the gRPC-status data path on the access-log record)
- **State at this commit:** §5 state-0/1 complete — this file exists, **`PLAN.md` does NOT**. The next session runs §5 state 2 (`superpowers:writing-plans`).
- **Scoping ADR:** `ADR-0196`.

---

## 1. Context — what a stranger needs to know

This repository is a from-scratch reimplementation of the Envoy Proxy in Rust, verified against upstream Envoy **v1.33.0** (image `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) by a differential test harness in `tests/differential/`. Read `BOOTSTRAP_PROMPT.md` §7 for the differential contract and `docs/envoy-rust/BEHAVIOR_CONTRACT.md` for the equivalence rules.

Three landed surfaces make this phase cheap. None of them should be re-derived from scratch, but every citation below is a PLAN-VERIFY item (§8, PV-1).

**Phases 70–74 built the access-log FILTER subsystem.** An access-log sink may carry a `filter:` that decides, per record, whether the record is emitted at all. This is a *log-emission predicate*, structurally distinct from the *formatter* engine phases 32–113 built. Six of upstream's twelve `AccessLogFilter` oneof arms are implemented:

| landed arm | phase | runtime variant |
|---|---|---|
| `status_code_filter` | 70 | `LogFilter::StatusCode` |
| `response_flag_filter` | 71 | `LogFilter::ResponseFlag` |
| `header_filter` | 72 | `LogFilter::Header` |
| `and_filter` | 73 | `LogFilter::And` |
| `or_filter` | 73 | `LogFilter::Or` |
| `metadata_filter` | 74 | `LogFilter::Metadata` |

The config type is `AccessLogFilter` at `crates/envoy-config/src/bootstrap.rs:731` — a six-field `Option` struct carrying `#[serde(default, deny_unknown_fields)]` at `:730`. Its own doc comment states the growth rule: *"filter-family phases add further `Option` arms here rather than reshaping the type."* Oneof cardinality is **not** serde-enforced; it is enforced by `validate_access_log_filter` (`bootstrap.rs:5739`), a six-field exhaustive destructure with **no `..`**, which errors `ConfigError::AmbiguousAccessLogFilter` (`crates/envoy-config/src/lib.rs:473`) unless exactly one arm is set. The runtime type is `LogFilter` at `crates/envoy-accesslog/src/filter.rs:68`; the config→runtime compile is `compile_access_log_filter` at `crates/envoy-http1/src/hcm.rs:1826`, a six-tuple match.

The predicate itself is `LogFilter::should_log` (`crates/envoy-accesslog/src/filter.rs:109`), reached through `FileSink::should_log` (`crates/envoy-accesslog/src/file_sink.rs:103`, which returns `true` for a sink with no filter). It takes **four** inputs today: `status: u16`, `response_flags: &str`, `headers: &[(String, String)]`, `dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>`. It has been widened once per data axis the family needed — phase 74's `T3: widen should_log with the dynamic-metadata store (behavior-neutral)` (commit `796450d`) is the exact precedent this phase follows.

**Phase 110 built the HTTP→gRPC status map.** `http_to_grpc_status` at `crates/envoy-http1/src/grpc.rs:64` is `pub(crate)` in `envoy-http1` and reads, verbatim:

```rust
pub(crate) fn http_to_grpc_status(status: u16) -> u8 {
    match status {
        400 => 13, 401 => 16, 403 => 7, 404 => 12,
        429 | 502 | 503 | 504 => 14,
        _ => 2,
    }
}
```

Its own doc warns against "improving" it with a range arm. `is_grpc_request` sits beside it at `:45`.

**Phase 113 built the gRPC-status field on the record.** `AccessLogRecord.grpc_status: Option<String>` at `crates/envoy-accesslog/src/record.rs:123` holds the **raw wire value** of the response `grpc-status` header, populated **only when the request was gRPC-detected**. On HTTP/2 it is hard-coded absent: `fn h2_grpc_status() -> Option<String> { None }` at `crates/envoy-http2/src/hcm.rs:1009`.

**What is missing.** envoy-rust has no `grpc_status_filter` at all. Because `AccessLogFilter` carries `deny_unknown_fields`, a config containing one is a **boot-fatal serde rejection**, not a silent ignore. The divergence is therefore total and boot-level.

---

## 2. The divergence — MEASURED on BOTH proxies at the state-0/1 pick

All upstream measurements below were taken by the pick session directly against the pinned image. Container ownership was proved by `docker inspect <cid> --format '{{.Image}}'` returning `sha256:56da5afd…70c2`, on a host port picked by a bound-then-released `socket.bind(('127.0.0.1',0))` and asserted free by a `/dev/tcp` pre-flight before binding with `-p 127.0.0.1:<port>:10000`.

### 2.1 Config acceptance — `--mode validate`, with a negative control

| access-log `filter:` | upstream Envoy v1.33.0 |
|---|---|
| `status_code_filter: {…}` *(positive control — a landed arm)* | `configuration … OK` |
| `grpc_status_filter: { statuses: [NOT_FOUND, INTERNAL] }` | `configuration … OK` |
| `grpc_status_filter: { statuses: [OK], exclude: true }` | `configuration … OK` |
| `grpc_status_filter: {}` *(no `statuses` at all)* | `configuration … OK` |
| `bogus_filter_xyz: { statuses: [OK] }` *(negative control)* | **REJECTED** — `no such field: 'bogus_filter_xyz'` at `…access_log[0].filter: message envoy.config.accesslog.v3.AccessLogFilter` |

The negative control is what makes this non-vacuous: `--mode validate` genuinely resolves the `AccessLogFilter` oneof rather than accepting any map. Note `--mode validate` is **upstream-only** — `envoy-bin` has no such flag and exits 2 on it.

### 2.2 The `statuses` token grammar — MEASURED, and NOT what a naive reader assumes

Each row is one `--mode validate` run with `statuses: [<token>]`:

| token | upstream | note |
|---|---|---|
| `OK` `CANCELED` `UNKNOWN` `INVALID_ARGUMENT` `DEADLINE_EXCEEDED` `NOT_FOUND` `ALREADY_EXISTS` `PERMISSION_DENIED` `RESOURCE_EXHAUSTED` `FAILED_PRECONDITION` `ABORTED` `OUT_OF_RANGE` `UNIMPLEMENTED` `INTERNAL` `UNAVAILABLE` `DATA_LOSS` `UNAUTHENTICATED` | **ACCEPT** (all 17) | the canonical set, codes 0–16 |
| `CANCELLED` *(two Ls)* | **REJECT** — `unknown enum value: 'CANCELLED'` | ⚠ the standing trap |
| `not_found` / `unimplemented` / `ok` / `Ok` / `oK` | **ACCEPT** | matching is **case-insensitive** |
| `NotFound` | **REJECT** — `INVALID_ARGUMENT` | ⚠ case-insensitivity does **not** imply camel-case: the **underscores are required** |
| `0` `5` `16` and the quoted `"5"` | **ACCEPT** | numeric tokens, int or string |
| `17` and `-1` | **REJECT** | the range is exactly 0–16 |
| `1.0` | **REJECT** — `unknown enum value: '1.0'` | non-integral |
| `TRUE` | **REJECT** — `invalid JSON` | ⚠ YAML 1.1 booleanizes `TRUE`, so it never reaches the enum parser |
| `[NOT_FOUND, NOT_FOUND]` | **ACCEPT** | duplicates permitted, no uniqueness bound |
| `[OK, NOT_FOUND]` | **ACCEPT** | multi-element |

**The precise rule is: case-insensitive match against the canonical `SCREAMING_SNAKE` name, or an integer literal in 0–16.** A validator built to a *guessed* rule manufactures a divergence in both directions — accepting `NotFound`, or rejecting `not_found`. This is exactly the `ADR-0184` PV-3 failure mode.

`ADR-0154` finding 7 already recorded part of this — *"no `min_items`, no uniqueness bound, enum spelling `CANCELED` one-L — `CANCELLED` is rejected; numeric tokens accepted"* — measured on the config surface only. This pick **corroborates** that finding and **extends** it with the case-insensitivity rule, the underscore requirement, the 0–16 bound and the `TRUE` YAML-1.1 trap, none of which any landed record contains.

### 2.3 The runtime rule — the finding that governs the whole design

Six sinks were configured on ONE upstream listener, all writing to `/dev/stdout` with distinguishing prefixes: **AAA** no filter (the control), **BBB** `statuses: [UNIMPLEMENTED]`, **CCC** `statuses: [UNKNOWN]`, **DDD** `statuses: [UNIMPLEMENTED], exclude: true`, **EEE** `grpc_status_filter: {}`, **FFF** `statuses: [OK]`. Routes are `direct_response` at chosen statuses; one route (`/okhdr`) adds an explicit `grpc-status: 0` response header. Each probe was sent twice, once with `content-type: application/grpc` and once plain. Logs were read via `docker logs` after a 12-second flush wait.

| probe | request `content-type` | `direct_response.status` | `%GRPC_STATUS%` (AAA) | sinks that KEPT the record | **derived filter status** |
|---|---|---|---|---|---|
| `/unimpl` | `application/grpc` | 404 | `Unimplemented` | BBB | **12** |
| `/internal` | `application/grpc` | 400 | `Internal` | DDD | **13** |
| `/unknown` | `application/grpc` | 200 | `Unknown` | CCC, DDD | **2** |
| `/okhdr` | `application/grpc` | 200 + hdr `grpc-status: 0` | `OK` | DDD, FFF | **0** |
| `/plain` | *(none)* | 404 | `-` | **BBB** | **12** |
| `/unknown` | *(none)* | 200 | `-` | CCC, DDD | **2** |
| `/internal` | *(none)* | 400 | `-` | DDD | **13** |
| `/okhdr` | *(none)* | 200 + hdr `grpc-status: 0` | `-` | DDD, FFF | **0** |

**Sink EEE (`grpc_status_filter: {}`) kept NOTHING on any of the eight probes.** An empty `statuses` set matches no record.

Four rules fall out, and each is load-bearing:

1. **The filter is NOT gated on the request being a gRPC request.** This is the single most important finding, and it is the opposite of `%GRPC_STATUS%`. Rows 5–8 are plain HTTP requests: `%GRPC_STATUS%` renders `-` on every one of them, yet the filter still computes a status and still discriminates. Row 5 is the decisive cell — a plain 404 with no gRPC anything was **kept by BBB (`[UNIMPLEMENTED]`)**.
2. **The status source is: the response `grpc-status` header if present, otherwise `http_to_grpc_status(response_code)`.** Rows 4 and 8 pin the header leg (an explicit `grpc-status: 0` yields 0 on both the gRPC and the plain request). Rows 5, 6, 7 pin the derivation leg, and they reproduce the phase-110 map exactly: 404→12, 400→13, 200→2 (the `_ => 2` default). Note upstream Envoy's map has **no** `200 => 0` arm; a plain 200 is `UNKNOWN`, not `OK`.
3. **`exclude: true` inverts the membership test**, and it inverts it over the *same* derived status. Sink DDD kept every probe whose derived status was not 12 and dropped both probes whose derived status was 12 — including the plain `/plain` 404. That is an **independent corroboration** of the derivation, from the opposite direction, on the same run.
4. **An empty `statuses` list keeps nothing.** It is not "match everything."

**This is the third independent observable to reproduce the phase-110 map.** `ADR-0177` measured it on the response *header*; phase 113 measured it through the *formatter*; this pick measures it through the *log-emission predicate*. Three different surfaces, one table.

### 2.4 What envoy-rust does today

Boot-fatal serde rejection. `AccessLogFilter` (`crates/envoy-config/src/bootstrap.rs:731`) carries `#[serde(default, deny_unknown_fields)]` at `:730` and exactly six fields, none named `grpc_status_filter`. Absence census re-derived at the pick, over all tracked `.rs` files: `grpc_status_filter` = **0**, against positive controls `status_code_filter` = **57 hits in 8 files** and `metadata_filter` = **74 hits in 8 files**. The six unimplemented arms return 0 under the identical command shape.

Because the divergence is boot-level rather than a value divergence, the fixture's RED-before / GREEN-after is unambiguous and **cannot pass vacuously**: every behavioural cell in §2.3 is unreachable on envoy-rust today.

---

## 3. Why this phase, and what it does not claim

The pick was made against the unbuilt-leaf inventory with four read-only recon subagents and the two-proxy measurements above. `ADR-0196` records the full options set with the measurement that killed each. Three points belong here because a cold-starting reader will otherwise mis-read the choice.

**This is the arm `CF-113-4` explicitly reserved, and its blocking premise is now measurably false.** `ADR-0154` finding 7 rejected `grpc_status_filter` as *"a vacuous differential"* on the ground that *"it reads the gRPC response TRAILER status and envoy-rust has no gRPC data plane."* `ADR-0183` recorded that premise stale. `ADR-0192`'s consequences opened **`CF-113-4`** with the standing instruction that *"a future session must RE-TEST that rejection's premise against its own slice rather than inheriting it."* This session did exactly that, and the premise fails on **both** of its clauses:

- *"reads the TRAILER status"* — **FALSE as a complete description.** §2.3 rows 4 and 8 show the response **header** leg firing, and rows 5–7 show the response-**code** derivation firing. Trailers are one source of three, and they are the only one this phase's H1 slice cannot reach.
- *"envoy-rust has no gRPC data plane"* — **FALSE since phase 110.** The HTTP→gRPC map is landed at `grpc.rs:64`, and phase 113 landed a gRPC status on the record.

**A vacuous differential is now impossible, and that is a measured claim rather than an assertion.** The `exclude: true` cell (§2.3 rule 3) discriminates in the opposite direction from the inclusion cells on the same records, and the plain-HTTP rows discriminate the derivation from the header. A wrong implementation cannot pass them all.

**`ADR-0192`'s "seventh repetition of the phases-70–74 pattern" objection is answered on the merits, not waved away.** That objection was raised against `runtime_filter` and it is a fair one to raise here. It does not hold, for a reason specific to this arm: the five landed arms all read a value the record **already carries** (`response_code`, `response_flags`, request headers, dynamic metadata). This arm reads a value that **does not exist anywhere in the tree in the shape the filter needs** — an *ungated* effective status, whereas `record.grpc_status` (`record.rs:123`) is *gated on `is_grpc_request`* and is a raw string rather than a code. §4 item 4 is therefore genuine new derivation work, not a sixth copy of a lookup. The two arms `ADR-0192` did reject stay rejected on their own grounds, restated in §5.

---

## 4. In scope — the deliverables

1. **`GrpcStatusFilter` config struct** in `crates/envoy-config/src/bootstrap.rs`, a seventh `Option` arm on `AccessLogFilter` (`:731`), following the type's own stated growth rule. Fields: `statuses` and `exclude: bool` (`#[serde(default)]`, defaulting `false`). `#[serde(default, deny_unknown_fields)]` on the struct, matching every sibling leaf.
2. **The token grammar** of §2.2 — case-insensitive canonical `SCREAMING_SNAKE` names **or** an integer in 0–16, duplicates permitted, out-of-range and non-integral and camel-case rejected. The PLAN-write must decide and record the serde shape that accepts both a bare `NOT_FOUND` and a bare `5` and a quoted `"5"` in one `Vec`; this is PLAN-VERIFY item **PV-2**.
3. **Validation** in `validate_access_log_filter` (`bootstrap.rs:5739`) — the destructure and the `set_arms` array both grow from six to seven, and a new fail-loud error for a bad token. The function has **no `..` in its destructure**, so the compiler forces the update.
4. **The UNGATED effective-status derivation.** ONE new field on `AccessLogRecord` (`crates/envoy-accesslog/src/record.rs`), populated at the record build on **both** codecs as *"the response `grpc-status` header if present, else `http_to_grpc_status(response_code)`."* It is **not** `Option`-shaped in the upstream rule — §2.3 shows a status is always defined — and it is **not** `record.grpc_status`, which is gated and would silently drop rows 5–8 of §2.3. Reusing `http_to_grpc_status` rather than duplicating the table is the point; `envoy-http2` already depends on `envoy-http1` and uses `envoy_http1::` at **76** sites, so widening `grpc.rs:64` from `pub(crate)` to `pub` gives both codecs one source of truth. That visibility change is a design decision and is PLAN-VERIFY item **PV-3**.
5. **`LogFilter::GrpcStatus { codes, exclude }`** in `crates/envoy-accesslog/src/filter.rs` (`:68`), plus its `should_log` arm. `envoy-accesslog` depends only on `tokio`, `bytes`, `tracing` and `thiserror` — deliberately, to avoid a cycle — so the status must arrive as **plain data**, not as an injected trait object the way `HeaderMatch`/`MetadataMatch` do. That is simpler than phase 72's or phase 74's seam, not harder.
6. **The fifth `should_log` widening** — a fifth parameter carrying the derived status, through `LogFilter::should_log` (`filter.rs:109`) and `FileSink::should_log` (`file_sink.rs:103`). This should be its own **behaviour-neutral** task, exactly as phase 74's `T3` (`796450d`) was. **Production call sites are exactly TWO** — `crates/envoy-http1/src/hcm.rs:1575` and `crates/envoy-http2/src/hcm.rs:1209`; the remaining 125 of the 127 `.should_log(` occurrences in `crates/` are test code, and the sweep over them is mechanical.
7. **`compile_access_log_filter`** (`crates/envoy-http1/src/hcm.rs:1826`) — the six-tuple match grows to seven.
8. **Fixture `0094-accesslog-grpc-status-filter`** — NEW, cluster-free and backend-free, through the EXISTING `Driver::Http1AccessLogByteExact`. See §6.
9. **`BEHAVIOR_CONTRACT.md`** — the §2.2 grammar and the §2.3 runtime rule, added to the access-log filter section phases 70–74 built.

---

## 5. Non-goals — rejected fail-loud, each with the reason

1. **The TRAILER source is OUT.** §2.3 measures the header leg and the response-code leg; the trailer leg is unreachable on this phase's HTTP/1.1 witness because `crates/envoy-http1/src/client.rs` discards response trailers behind chunked response encoding — pre-existing and already banked as **CF-111-2** (a ~153-site change). This phase neither closes nor widens it. Banked as **CF-114-1**. ⚠ Note this means the phase implements a **two-source** rule where upstream implements a three-source one; the implementation must be written so that adding the trailer source later is an added branch, not a reshape.
2. **The other five unbuilt arms are OUT** — and each stays rejected on its own landed ground, not on this phase's convenience. `duration_filter`: `ADR-0192` (d) rejected it because its predicate is request DURATION and `BEHAVIOR_CONTRACT.md` excludes timing from comparison by default; that ground is untouched here. `runtime_filter`: `ADR-0192` (c) rejected it on witness quality — a sampling predicate is only differentially assertable at the degenerate 0%/100% cells; also untouched. `not_health_check_filter` and `traceable_filter`: **measured absent from the entire ADR log** (`grep -ic` = 0 each, against `duration_filter` = 10 and `runtime_filter` = 9 under the identical shape), and both need a data axis the tree does not have — there is no downstream health-check filter (`health_check_filter` = 0 hits in `crates/`, control `envoy.filters.http` = 363) and no tracing decision anywhere (`traceable` = 0 hits, control `response_flags` = 38). `extension_filter`: never named in any ADR, any ROADMAP row or any phase document, and it needs an extension-registry seam for access-log filters that does not exist. Banked as **CF-114-2**.
3. **No HTTP/2 differential witness.** The derivation of §4 item 4 is specified for **both** codecs, because `response_code` is on the record on both and the correctness argument is the same. What is out of scope is a *second fixture* through `Driver::Http2AccessLogByteExact` to witness it cross-proxy. Whether the H2 response-header map is live at the H2 record build is **not** settled by this SPEC and is PLAN-VERIFY item **PV-4**; if it is not, H2 gets the response-code leg only and the header leg on H2 is banked. Either way the H2 behaviour must be **pinned by an in-process test**, so that a later phase lifting the boundary has to delete an explicit assertion rather than silently change behaviour. Banked as **CF-114-3**.
4. **`%GRPC_STATUS%` and `record.grpc_status` are NOT changed.** Phase 113's field is gated by design and its gate is differentially witnessed by fixture `0093`. This phase adds a second, differently-shaped value beside it; it must not "unify" them, and it must not relax the phase-113 gate. A PLAN that finds itself editing `record.grpc_status`'s population has drifted — stop and re-scope.
5. **No new config surface beyond the arm, no new dependency, no new harness driver, no new fuzz target, no new workspace crate.** `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml` and `tests/differential/src/lib.rs` are all expected to be **untouched**. §7.4's *"parser, codec, or filter"* fuzz trigger is discharged by the pre-existing targets: this arm adds no new *format-string* parser, and its config parsing is serde over a fixed enum. **If the PLAN-write finds it must touch any of those four files, that is a signal the scope drifted — stop and re-scope.**
6. **Nothing is fixed.** Per §6.3 and `ADR-0165`, this phase consumes no unrelated carry-forward. In particular **`CF-113-7`** — the two-hunk doc-comment move at `crates/envoy-http2/src/hcm.rs:989-1014`, where `h2_grpc_status()` was spliced above `finalize_h2_stream`'s `#[allow(clippy::too_many_arguments)]` and stole its 10-line doc comment — sits in a file this phase **does** touch (§4 item 6 changes `hcm.rs:1209`). It is therefore a legitimate **rider**, and the PLAN-write should take it as one, in its own commit, explicitly labelled. It is **not** a deliverable of this phase and its omission is not a defect. The phase-112 ALPN cleanup set (**CF-112-13**, whose blast radius is **five** internally-tagged unit variants and not the four its REVIEW names — the fifth is `HeaderRule::SetEqualModuloAllowList` at `tests/differential/src/lib.rs:1162`; plus **CF-112-14**, **CF-112-8**, **CF-112-9**, **CF-112-16**) is **NOT** taken: `ADR-0192` DECISION 5 measured it at ≈134 net lines and adjudicated it a rider for a phase touching *its* files, which these are not. **Do not re-cost it.**

---

## 6. The differential witness — fixture `0094-accesslog-grpc-status-filter`

**Numbering re-derived at the pick:** `tests/fixtures/` holds **93** directories (git-tracked and on-disk agree; zero stray non-directory entries), highest `0093-accesslog-grpc-status`, so this phase's is `0094`.

**Driver: the EXISTING `Driver::Http1AccessLogByteExact`** (`tests/differential/src/lib.rs:170`), used by **31** landed fixtures. Two of its properties are exactly what this phase needs, and both were verified at the pick:

- `AccessLogByteExactProbe.expect_logged: bool` (`:1198`, default `true`) — added by **phase 70 (`ADR-0141`)** precisely to mark *"a probe whose response is expected to be SUPPRESSED by an access-log filter (contributes no log line on EITHER proxy)."* Ten landed fixtures already use it. The line-count target is `expected_logged_count(probes)`, not `probes.len()`, and a `has_suppression` settle guard bails if either side's file grows past the kept count.
- `AccessLogByteExactProbe.extra_headers: Vec<(String, String)>` (`:1187`) — lets a probe send `content-type: application/grpc`.

**No driver change, no harness change.** The comparison is `assert_access_log_lines_byte_identical` and it takes **exactly ONE log file per side** (`AccessLogPaths { envoy, envoy_rust }`), so the fixture must configure **one** filtered sink, not a pair.

**The same fixture-design constraint phase 113 measured applies here and is inherited, not re-derived:** the fixture must **NOT** use route-level `response_headers_to_add`. envoy-rust's `Route` has no such field and `deny_unknown_fields` makes it boot-fatal, so a fixture written that way goes RED for a reason unrelated to gRPC. `grpc-status` values must be produced by the **phase-110 transform**, driven from `direct_response.status`. Note the consequence for §2.3's `/okhdr` cell: **the explicit-header leg is not directly expressible in this fixture** on the envoy-rust side, and the PLAN-write must either find another way to set the header or bank the header leg as differentially unwitnessed. This is PLAN-VERIFY item **PV-5** and it is the highest-risk item in the phase.

**Probe set** — one filtered sink, `filter: { grpc_status_filter: { statuses: [UNIMPLEMENTED, INTERNAL] } }`, each probe on a DISTINCT path because identical paths cannot attribute a kept log line:

| probe | path | `content-type` | `direct_response.status` | derived status | `expect_logged` |
|---|---|---|---|---|---|
| 1 | `/g-unimpl` | `application/grpc` | 404 | 12 | **true** |
| 2 | `/g-internal` | `application/grpc` | 400 | 13 | **true** |
| 3 | `/g-unknown` | `application/grpc` | 200 | 2 | **false** |
| 4 | `/g-unavail` | `application/grpc` | 503 | 14 | **false** |
| 5 | `/p-unimpl` | *(none)* | 404 | 12 | **true** ⚠ the ungated cell |
| 6 | `/p-internal` | *(none)* | 400 | 13 | **true** ⚠ |
| 7 | `/p-unknown` | *(none)* | 200 | 2 | **false** |
| 8 | `/g-param` | `application/grpc; charset=utf-8` | 404 | 12 | **true** ⚠ the `%GRPC_STATUS%` gate does NOT apply |

**Probes 5, 6 and 8 are what make this fixture non-vacuous**, and they are the reason it is not a copy of fixture `0093`. An implementation that reuses `record.grpc_status` — the obvious wrong move, since that field already exists and is already named `grpc_status` — goes RED on all three, because that field is `None` for a non-gRPC request. Probes 3, 4 and 7 are the suppression controls. The sink's `text_format` should render `%GRPC_STATUS%` and `%RESPONSE_CODE%` and the path, so the kept lines also witness that the *formatter's* gated value and the *filter's* ungated value genuinely differ on probes 5, 6 and 8.

**A second fixture is NOT proposed for `exclude: true`.** The byte-exact driver takes one log file per side, so an `exclude` witness needs its own fixture. The PLAN-write must decide whether to add `0095` or to cover `exclude` in-process only, and must record which — **PV-6**. Covering it in-process only is acceptable and should then be banked as a carry-forward rather than left unstated.

**Local-verifiability.** Cluster-free and backend-free, so it is verifiable on the development host, where backend-routing fixtures go RED against the `192.168.65.2` Docker bridge and CI is the only authority. Fixture `0088` is the cluster-free template (`clusters: []`, two byte-identical YAMLs, `{{PORT}}` the only token). `Http1AccessLogByteExact` is **not** one of the five drivers that receive `{{ADMIN_PORT}}`, so the fixture must not reference that token.

---

## 7. Size estimate and the §6.1 split gate

Bottom-up, net lines of change, **excluding `docs/`**:

| piece | est. |
|---|---|
| `GrpcStatusFilter` config struct + the `AccessLogFilter` seventh arm | 60–90 |
| the 17-name canonical table + case-insensitive + numeric token parse + the reject set | 120–170 |
| `validate_access_log_filter` seven-arm growth + the new fail-loud error variant | 50–80 |
| `LogFilter::GrpcStatus` + its `should_log` arm | 50–80 |
| `compile_access_log_filter` seventh tuple arm | 20–35 |
| the UNGATED derivation on the record + the `http_to_grpc_status` visibility widen + both record builds | 90–140 |
| the behaviour-neutral fifth-parameter `should_log` sweep (2 production sites, ~125 test sites) | 150–250 |
| unit and mutation tests across parse / validate / compile / predicate / derivation | 260–330 |
| fixture `0094` (4 files) + its runner test file | 230–290 |
| **code subtotal** | **≈1030–1465** |
| `BEHAVIOR_CONTRACT.md` section *(docs — excluded from the gate)* | 90–130 |

**Central ≈ 1250 code.** The §6.1 gate is **~25 tasks OR ~1500 net LoC**. The task count (projected 8–10) is not close. The LoC figure is.

**The calibration says the gate will probably fire.** Landed-vs-projected ratios, re-measured at this pick with `git diff --numstat <state-2 commit> <state-3 advance>` over `crates/` + `tests/`:

| phase | projected | landed code | ratio | how the estimate was made |
|---|---|---|---|---|
| `110.2` | 615 | **817** | **1.33×** | projected |
| `111` | 916 | **1525** | **1.66×** | projected |
| `112.1` | 551 | **549** | **1.00×** | **measured on a prototype** |
| `112.2` | 595 | **652** | **1.10×** | measured, then the plan drifted |
| `113` | 1092 | **1165** | **1.07×** | **measured on a prototype** |

The measured-method claim is now **three for three inside 1.10×**. At the projected band this phase lands **1370–2430** and **crosses the gate**; at the measured band it lands **1030–1610** and straddles it. The closest-shaped landed comparators are the filter-family phases themselves, measured at the pick as `crates/` + `tests/` net over the full landed arc: phase 70 **1716**, phase 71 **917**, phase 72 **1064**, phase 73 **873**, phase 74 **1981**. Two of the five exceed 1500.

**Therefore: the §6.1 split is PROJECTED LIKELY, and `ADR-0197` is RESERVED-UNFIRED for it.** The state-2 PLAN-write **must** re-derive the estimate on a prototype in a scratch worktree with its own `CARGO_TARGET_DIR`, and must re-measure **after the final edit to the plan** — `ADR-0189`'s figure of 595 landed at 652 because the plan was edited after the measurement, so a measured estimate goes stale in its *method claim*, not only in its number. ⚠ And per `ADR-0194` DECISION 2, **a whole-slice prototype validates the SLICE, never a TASK BOUNDARY**: a plan that measures once at the end must not then assert a per-task `-D warnings` gate.

If the gate fires, the natural cut follows the `110.1`/`110.2` and `112.1`/`112.2` precedent exactly:

- **`114.1`** — the config surface, the token grammar, the validator, `LogFilter::GrpcStatus`, the `should_log` widening and the ungated derivation. Witnessed **entirely in-process**, no new fixture, with regression-equivalence over all 93 existing fixtures.
- **`114.2`** — fixture `0094` (and `0095` if §6 PV-6 calls for it), the `BEHAVIOR_CONTRACT.md` section, and the parent-114 close.

Per §6.2 step 7 the PLAN-write **stops without writing a `PLAN.md`** if it splits.

---

## 8. PLAN-VERIFY items — what state 2 must measure, and why each is load-bearing

Each of these is a claim this SPEC either could not settle or settled in a way a fresh measurement should confirm.

- **PV-1 — Re-derive every `file:line` in this document.** `bootstrap.rs:730`/`:731`/`:5739`, `lib.rs:473`, `filter.rs:68`/`:109`, `file_sink.rs:103`, `record.rs:47`/`:123`, `grpc.rs:45`/`:64`, `envoy-http1/src/hcm.rs:1575`/`:1826`, `envoy-http2/src/hcm.rs:1009`/`:1209`, `tests/differential/src/lib.rs:170`/`:1183`/`:1187`/`:1198`/`:1162`. All were correct at HEAD when this SPEC was written; **any phase that edits `hcm.rs` moves them**, and a stale citation can land on a plausible wrong target. Locate by **TEXT**, and assert each anchor occurs **exactly once** before trusting the line it returns.
- **PV-2 — The serde shape for a mixed-token `Vec`.** §2.2 requires one list to hold bare identifiers, bare integers and quoted integers. Decide the representation (untagged enum? `Vec<serde_yaml::Value>` plus a validating pass? a custom `Deserialize`?) and record it. ⚠ **Check what a bare `TRUE`, `y`, `n`, `on` and `off` do on the envoy-rust side**: `serde_yaml` parses YAML **1.2** while Envoy parses YAML **1.1**, so the booleanization that made upstream reject `TRUE` may not fire here, manufacturing a REJECT-direction divergence.
- **PV-3 — The `http_to_grpc_status` visibility widen.** `grpc.rs:64` is `pub(crate)` in `envoy-http1`. `envoy-http2` depends on `envoy-http1` and uses `envoy_http1::` at 76 sites, so `pub` is available — but `crates/envoy-http1/src/grpc.rs:14-19` argues *against* widening that module's reach, and phase 113's PV-3 raised the same question for `is_grpc_request`. **A visibility widening is a design change and needs saying out loud.** Read what phase 113 decided and say whether this follows it or departs from it.
- **PV-4 — Is the response-header map live at the H2 record build?** §5 non-goal 3 turns on this. `h2_grpc_status()` at `envoy-http2/src/hcm.rs:1009` returns `None` because the trailer block is moved before the record build (`CF-113-2`); that is a statement about **trailers**, not headers, and it must not be assumed to extend. Measure it.
- **PV-5 — Dry-run the exact fixture `0094` YAML against BOTH proxies, end to end, before writing any code.** This is the single highest-value item. `ADR-0180` records that the equivalent dry-run for fixture `0089` found three divergences that would each have landed it RED for unrelated reasons. Re-check **CF-110-6** (envoy-rust emits `content-type` on an empty-body local reply where upstream emits none) and **CF-110-7** (`direct_response.body` is mandatory in envoy-rust, optional upstream). And settle §6's header-leg problem: either find an expressible way to set an explicit `grpc-status` response header on both proxies, or bank the header leg as differentially unwitnessed and say so.
- **PV-6 — Decide and record the `exclude: true` witness** (own fixture, or in-process only plus a carry-forward). §6.
- **PV-7 — Confirm the derived status is genuinely absent from the tree in the shape needed.** §3's argument rests on `record.grpc_status` being unusable here. Prove it by mutation: make the filter read `record.grpc_status` and assert probes 5, 6 and 8 of §6 go RED. **A test asserting absence passes vacuously against a placeholder; plan the mutation as its red evidence.**
- **PV-8 — Re-measure the §7 estimate on a prototype**, and re-measure again after the final edit to `PLAN.md` (§7).
- **PV-9 — Confirm `Cargo.toml` / `Cargo.lock` / `ci.yml` / `tests/differential/src/lib.rs` are untouched** by the finished plan (§5 non-goal 5).

---

## 9. Close-out clause

When the last unit of this phase reaches §5 state 6, the close-out flips this phase's ROADMAP row to `done`.

⚠ **Do not trust that sentence's row list — DERIVE the not-done set from `ROADMAP.md` at the close-out.** A close-out flips the **status cell only**, by splitting each row on `' | '`, asserting six cells and the expected current status, replacing that one cell and re-joining. This warning is not boilerplate: the equivalent clause in `112.2/SPEC.md` §9 named a row that had already flipped at its own earlier close-out, and so mis-stated two consecutive close-outs. If this phase splits (§7), the parent row flips only once every sub-phase row is `done`.

Two `ROADMAP.md` rows carry unescaped in-cell pipes and split into 7 and 10 fields under `' | '`. They are append-only history and **must not be "fixed"**; any census must drive off `' | '` **with spaces** and take status at field **4**, never `NF == 6` — that filter silently drops exactly those two rows.

---

## 10. Carry-forwards opened by this phase

- **CF-114-1** — the TRAILER source of the effective gRPC status is not implemented; the phase ships a two-source rule where upstream has three. Blocked behind the pre-existing `CF-111-2` (H1 response trailers are discarded behind chunked response encoding, a ~153-site change). (§5 non-goal 1.)
- **CF-114-2** — five `AccessLogFilter` arms remain unbuilt: `duration_filter` and `runtime_filter` (both with standing, still-valid `ADR-0192` rejections), and `not_health_check_filter`, `traceable_filter` and `extension_filter`, each of which needs a data axis or a registry seam the tree does not have and **none of which is named in any landed ADR**. (§5 non-goal 2.)
- **CF-114-3** — the HTTP/2 arm of `grpc_status_filter` has no differential witness, and whether its response-header leg is even reachable is unresolved (PV-4). (§5 non-goal 3.)
- **CF-114-4** — whichever of the `exclude: true` witness and the explicit-`grpc-status`-header witness the PLAN-write cannot express in fixture `0094` (PV-5, PV-6).

**`CF-113-4` is CONSUMED by this phase** — it reserved `grpc_status_filter` and required its rejection premise to be re-tested; §3 does exactly that and §2 acts on the result. Every other previously banked carry-forward carries forward **INTACT and UNCONSUMED**: CF-113-1/2/3 and CF-113-5…CF-113-13, CF-112-1…CF-112-19, the `112.1`/`112.2`/`111`/`110.x`/`109.x`/`108.2` REVIEW sets, CF-111-1…CF-111-9, CF-110-1…9, CF-109-1/2/3, CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6, CF-74-1/2/3/4/6, CF-73-1 and the HTTP-filters-family (1)–(4). **CF-111-4 stays consumed only in PART. CF-112-5 stays CLOSED. CF-112-8 Consequence 2 stays BANKED as structurally unwitnessable by this harness — do not try to build it as a fixture.** **CF-113-7 is available as a RIDER** to this phase because §4 item 6 touches its file (§5 non-goal 6); taking it is the PLAN-write's call, and not taking it is not a defect.
