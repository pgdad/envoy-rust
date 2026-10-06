# Phase 116 — SPEC

**Observability family: the `not_health_check_filter` access-log filter arm.** This is the **EIGHTH** of upstream Envoy's twelve `envoy.config.accesslog.v3.AccessLogFilter` `oneof` arms. It drops a record when the request was answered by the downstream health-check filter that phase 115 built. The phase lands four things:

- the empty-message `NotHealthCheckFilter` config surface;
- an eighth `AccessLogFilter` arm, with its validator and compile growth;
- `LogFilter::NotHealthCheck`;
- one new data axis, *"was this request a health check"*, carried to the `should_log` predicate on both codecs.

The witness is a NEW cluster-free, backend-free differential fixture, `0099-accesslog-not-health-check-filter`, run through the **EXISTING** `Driver::Http1AccessLogByteExact`. The phase adds no driver, no dependency, no fuzz target and no workspace crate.

- **Phase id:** `116`
- **Directory:** `docs/envoy-rust/phases/116-accesslog-not-health-check-filter/`
- **Depends on:** `06` (the access-log subsystem), `70` (the access-log FILTER subsystem and its harness support), `115` (the downstream `envoy.filters.http.health_check` filter, whose decision this arm reads)
- **State at this commit:** §5 state-0/1 is complete. This file exists and **`PLAN.md` does NOT**. The next session runs §5 state 2 (`superpowers:writing-plans`).
- **Scoping ADR:** `ADR-0205`.

---

## 1. Context — what a stranger needs to know

This repository is a from-scratch reimplementation of the Envoy Proxy in Rust. It is verified against upstream Envoy **v1.33.0** (image `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) by a differential test harness in `tests/differential/`. Read `BOOTSTRAP_PROMPT.md` §7 for the differential contract, and `docs/envoy-rust/BEHAVIOR_CONTRACT.md` for the equivalence rules.

Two landed surfaces make this phase small. Every `file:line` below was correct at this commit and is a PLAN-VERIFY item (§8, PV-1).

**Phases 70–74 and 114 built the access-log FILTER subsystem.** An access-log sink may carry a `filter:` that decides, per record, whether the record is emitted at all. Seven of upstream's twelve arms are implemented:

| landed arm | phase | runtime variant |
|---|---|---|
| `status_code_filter` | 70 | `LogFilter::StatusCode` |
| `response_flag_filter` | 71 | `LogFilter::ResponseFlag` |
| `header_filter` | 72 | `LogFilter::Header` |
| `and_filter` | 73 | `LogFilter::And` |
| `or_filter` | 73 | `LogFilter::Or` |
| `metadata_filter` | 74 | `LogFilter::Metadata` |
| `grpc_status_filter` | 114 | `LogFilter::GrpcStatus` |

The five unbuilt arms are `duration_filter`, `not_health_check_filter`, `traceable_filter`, `runtime_filter` and `extension_filter`. Each reads **0** occurrences across `crates/`, `tests/` and `BEHAVIOR_CONTRACT.md`. The built-arm controls, taken with the identical `grep -ro … | wc -l` invocation, are `grpc_status_filter` **46/10/6** and `response_flag_filter` **60/11/11**.

- **Config type.** `AccessLogFilter` is at `crates/envoy-config/src/bootstrap.rs:732`. It is a seven-field `Option` struct carrying `#[serde(default, deny_unknown_fields)]` (`:731`) and deriving `Default` (`:730`).
- **Oneof cardinality.** Serde does not enforce it. `validate_access_log_filter` (`bootstrap.rs:5908`) does, with a seven-field exhaustive destructure that has **no `..`**. It errors `ConfigError::AmbiguousAccessLogFilter` (`crates/envoy-config/src/lib.rs:475`) unless exactly one arm is set.
- **Runtime type.** `LogFilter` is at `crates/envoy-accesslog/src/filter.rs:68`, and its predicate `LogFilter::should_log` is at `filter.rs:121`. `FileSink::should_log` (`crates/envoy-accesslog/src/file_sink.rs:103`) returns `true` for a sink with no filter.
- **Predicate inputs.** `should_log` takes **five** inputs today: `status`, `response_flags`, request `headers`, `dynamic_metadata` and the phase-114 `grpc_status_code`. It has been widened once per data axis the family needed.
- **Config → runtime compile.** `compile_access_log_filter` (`crates/envoy-http1/src/hcm.rs:1880`) is a seven-tuple match.

**Phase 115 built the downstream health-check filter.** `HealthCheckFilter` is at `crates/envoy-filter/src/health_check.rs`. On a match, its `decode_headers` returns `Decision::StopAndSend` with `details: Some(HEALTH_CHECK_OK)`, where `HEALTH_CHECK_OK = "health_check_ok"` (`health_check.rs:31`). **Both HCMs already compute an "is this a health check" predicate from that value**, and they use it to exclude intercepted requests from the `downstream_rq_Nxx` counters:

- H1: `response_code_details_for_log.as_deref() != Some(envoy_filter::health_check::HEALTH_CHECK_OK)` at `crates/envoy-http1/src/hcm.rs:1554`;
- H2: the same test over `response_code_details_for_log_h2` at `crates/envoy-http2/src/hcm.rs:1133`.

That predicate is the data this arm needs. It is computed a few lines before each codec's single production `should_log` call (`envoy-http1/src/hcm.rs:1605` and `envoy-http2/src/hcm.rs:1236`).

**What is missing.** envoy-rust has no `not_health_check_filter` at all. `AccessLogFilter` carries `deny_unknown_fields`, so a config containing one is rejected by serde at boot rather than silently ignored. The divergence is total and boot-level.

---

## 2. The divergence — MEASURED on upstream at the state-0/1 pick

All measurements below were taken by the pick session directly against the pinned image. Container ownership was proved by `docker inspect <cid> --format '{{.Image}}'`, which returned `sha256:56da5afd…70c2`. Host ports came from a bound-then-released `socket.bind(('127.0.0.1',0))` and were mapped with `-p 127.0.0.1:<port>:<container-port>`. **Readiness was gated on a real HTTP 200 from a throwaway path on EACH listener**, never on a TCP connect: the phase-115 pick showed that a transport-level gate manufactures a confident false finding. File flushing was forced with `--file-flush-interval-msec 100`, and logs were read with `docker logs` after a 3-second wait.

### 2.1 Config acceptance — `--mode validate`, with a negative control

| access-log `filter:` | upstream Envoy v1.33.0 |
|---|---|
| `not_health_check_filter: {}`, alone and nested inside both `or_filter` and `and_filter` on one bootstrap | `configuration … OK` |
| `not_health_check_filter: { foo: 1 }` | **REJECTED**: `no such field: 'foo'`. The message is empty and closed. |
| `not_health_check_filter: ~` *(YAML null)* | **REJECTED**: `AccessLogValidationError.Filter … field: "filter_specifier", reason: is required`. ⚠ A null arm reads as an UNSET oneof, not as an empty message. |
| `not_health_check_filter: []` and `not_health_check_filter: true` | **REJECTED** (`invalid JSON`) |
| `not_health_check_filter: {}` plus a `status_code_filter` on the same filter | **REJECTED** (`oneof`): two arms set |
| `bogus_filter_xyz: {}` *(negative control)* | **REJECTED**: `no such field` |

The negative control is what makes the acceptance rows meaningful: `--mode validate` really does resolve the `AccessLogFilter` oneof rather than accepting any map. `--mode validate` exists only upstream; `envoy-bin` has no such flag.

### 2.2 The runtime rule

**Config.** Two listeners run on one upstream instance.

- **Listener A** has the chain `[rbac, health_check, router]`.
  - `rbac` is `action: DENY` with one policy whose principal is `header: { name: x-deny, present_match: true }`.
  - `health_check` is `pass_through_mode: false` with one matcher, `":path"` `string_match: { exact: "/healthz" }`.
  - The route is a `prefix: /` catch-all to `direct_response: { status: 200, body: "MAIN" }`.
- **Listener B** has the same route but **no health-check filter**.

**Sinks.** All sinks write to `/dev/stdout` with distinguishing prefixes, in the format `<PFX> %REQ(:METHOD)% %REQ(:PATH)% %RESPONSE_CODE% %RESPONSE_CODE_DETAILS%`.

- On listener A:
  - **AAA** has no filter (the control).
  - **BBB** has `not_health_check_filter: {}`.
  - **CCC** has `or_filter: [not_health_check_filter, header_filter{x-keep present}]`.
  - **DDD** has `and_filter: [not_health_check_filter, status_code_filter{GE 300}]`.
- On listener B:
  - **EEE** has no filter.
  - **FFF** has `not_health_check_filter: {}`.

| # | listener | request | wire response | AAA | BBB | CCC | DDD | EEE | FFF |
|---|---|---|---|---|---|---|---|---|---|
| 1 | A | `GET /healthz` | 200, `content-length: 0`, `x-envoy-upstream-healthchecked-cluster: probe-cluster` | `health_check_ok` | **dropped** | dropped | dropped | | |
| 2 | A | `POST /healthz` | 200, intercepted | `health_check_ok` | **dropped** | dropped | dropped | | |
| 3 | A | `GET /healthz?x=1` | 200 `MAIN` (falls through) | kept | **kept** | kept | dropped (<300) | | |
| 4 | A | `GET /other` | 200 `MAIN` | kept | **kept** | kept | dropped | | |
| 5 | A | `GET /healthz` + `x-keep: 1` | 200, intercepted | kept | dropped | **kept** (OR leg 2) | dropped | | |
| 6 | A | `GET /healthz` + `x-deny: 1` | **403** `RBAC: access denied` | `rbac_access_denied_matched_policy[deny-flagged]` | **kept** | kept | **kept** | | |
| 7 | A | `GET /ua` + `user-agent: Envoy/HC` | 200 `MAIN` | kept | **kept** | kept | dropped | | |
| 8 | B | `GET /healthz` | 200 `MAIN` | | | | | kept | **kept** |
| 9 | B | `GET /ua` + `user-agent: Envoy/HC` | 200 `MAIN` | | | | | kept | **kept** |
| 10 | B | `GET /int` + `x-envoy-internal: true` | 200 `MAIN` | | | | | kept | **kept** |

The two readiness probes, one per listener, were kept by every sink they reached. That gives a sixth and a seventh non-health-check witness for free.

Five rules fall out of the table. Each one is load-bearing:

1. **The arm drops EXACTLY the requests the health-check filter answered, and nothing else.** Rows 1 and 2 are dropped. Every other row on both listeners is kept, including row 3, whose path differs from the matcher's only by its query string. Row 2 shows the drop is method-agnostic, as the filter itself is.
2. **The marking comes from the filter's DECISION, not from the request's shape.**
   - Row 8 sends the same `GET /healthz` as row 1 to a listener with no health-check filter, and it is **kept**.
   - Rows 7 and 9 carry upstream's own active-health-checker user-agent `Envoy/HC`, and they are kept.
   - Row 10 carries `x-envoy-internal: true`, and it is kept.

   An implementation that keys on the path, the user-agent or any request header is wrong, and these rows catch it.
3. **The filter must actually RUN.** In row 6 an RBAC deny earlier in the chain answers the request before the health-check filter sees it. The path matches the health-check matcher, yet the record is **kept**. This is the cell that tells "the request matched the health-check config" apart from "the health-check filter answered the request". Only the second is correct.
4. **The arm composes like any other leaf.**
   - Row 5: inside `or_filter`, the intercepted record is rescued by the other leg.
   - Inside `and_filter` with `status_code_filter{GE 300}`, only row 6 (the 403) survives.

   No special case is needed in the composition arms.
5. **The arm has no configuration.** `NotHealthCheckFilter` is an empty message. §2.1 shows any field is rejected and a null is an unset oneof.

### 2.3 What envoy-rust does today

envoy-rust rejects the config at boot, in serde. `AccessLogFilter` (`bootstrap.rs:732`) carries `deny_unknown_fields` and has no field named `not_health_check_filter`. The divergence is boot-level rather than a value divergence, so the fixture's RED-before / GREEN-after is unambiguous and **cannot pass vacuously**: no behavioural cell in §2.2 is reachable on envoy-rust today.

---

## 3. Why this phase, and what it does not claim

`ADR-0205` records the full options set and the measurement that rejected each alternative. Three points belong here because a cold-starting reader may otherwise misread the choice.

**This arm's standing rejection has been discharged, and that was re-tested rather than inherited.**

- `ADR-0196` rejected-alternative (c) turned the arm down on a measured ground: *"no downstream health-check filter exists … so the arm would need a whole filter built first."*
- Phase 115 built that filter. `health_check` now reads **1347** occurrences in `crates/`.
- `CF-115-3` banks this arm as the natural successor.
- This session re-measured the whole runtime rule (§2.2) against the pin instead of trusting the phase-115 pick's six-request witness. It found two rules (§2.2 rules 2 and 3) that the earlier witness could not express, because that witness had no non-health-check request matching the health-check path.

**It is not an eighth copy of the phases-70–74 pattern, and that objection is answered on the merits.** `ADR-0192` raised a "repetition" objection against `runtime_filter`. The arms landed so far each read a value that sits on the record. This arm reads a **filter decision**, a fact about which component answered the request. It is the first access-log arm whose input is produced by an HTTP filter's control flow rather than by the request or the response. The design question is therefore real: *how does the predicate learn which filter answered?* §4 item 4 answers it by reusing a predicate the HCMs already compute.

**The arm has no grammar and no parameters, so the risk sits entirely in the data axis.** There is nothing to parse and nothing to validate beyond the empty message. If this phase goes wrong, it will be because the health-check bit is computed in the wrong place. Rules 2 and 3 of §2.2 are written to catch exactly that.

---

## 4. In scope — the deliverables

1. **`NotHealthCheckFilter` config struct** in `crates/envoy-config/src/bootstrap.rs`. It is empty, carries `#[serde(default, deny_unknown_fields)]` like every sibling leaf, and becomes an eighth `Option` arm on `AccessLogFilter` (`:732`), following the type's own doc-stated growth rule.
2. **Validation.** In `validate_access_log_filter` (`bootstrap.rs:5908`), the destructure and the `set_arms` array both grow from seven to eight. No new error variant is needed. The empty message has nothing to validate, and a field inside it is a serde rejection.
3. **`LogFilter::NotHealthCheck`** in `crates/envoy-accesslog/src/filter.rs` (`:68`), a unit variant, with its `should_log` arm `!is_health_check`. `envoy-accesslog` depends only on `tokio`, `bytes`, `tracing` and `thiserror`, so the bit must arrive as **plain data**, exactly as phase 114's status did.
4. **The health-check data axis.** A new boolean input reaches `should_log` on both codecs: true iff the request was answered by the health-check filter. **The recommended source is the predicate both HCMs already compute** (§1): `response_code_details == Some("health_check_ok")` at `envoy-http1/src/hcm.rs:1554` and `envoy-http2/src/hcm.rs:1133`. The access-log arm and the `downstream_rq_Nxx` exclusion would then rest on one definition. The PLAN-write chooses how to carry the bit, either a sixth `should_log` parameter or a field on `AccessLogRecord`, and records the choice. That is PLAN-VERIFY item **PV-2**.
   - ⚠ A **sixth `should_log` parameter** follows the phase-74 and phase-114 precedent, and phase 74's `T3` (`796450d`) is the template for doing it as its own **behaviour-neutral** task. Its cost is a mechanical sweep over **146** `should_log(` occurrences in four files (`envoy-accesslog/src/filter.rs` 80, `envoy-http1/src/hcm.rs` 56, `envoy-accesslog/src/file_sink.rs` 6, `envoy-http2/src/hcm.rs` 4). Only **two** of them are production sites.
   - ⚠ An **`AccessLogRecord` field** avoids the sweep, but it raises `E0063` at every literal `AccessLogRecord { … }` construction (**21** occurrences in `crates/` and `tests/` at this commit), and the predicate still needs a way to read it.

   Whichever the PLAN picks, the bit must be computed **after** the decode-side filter pass has decided the response, never from the request. §2.2 rules 2 and 3 are what a request-derived bit would fail.
5. **`compile_access_log_filter`** (`crates/envoy-http1/src/hcm.rs:1880`): the seven-tuple match grows to eight. Every existing arm's pattern gains a trailing `None`, so rustfmt reflow will inflate the diff beyond the logical change. Count it.
6. **The `AccessLogFilter` literal sweep.** Adding the eighth field raises `E0063` at every literal construction that does not use `..Default::default()`. **14** explicit `grpc_status_filter: None` initialisers exist at this commit (`envoy-http1/src/hcm.rs` 11, `envoy-config/src/bootstrap.rs` 3), plus any `grpc_status_filter: Some(…)` sites. They span two crates, so the sweep must run as a **FIXPOINT loop over the whole workspace**. Cargo stops at the first failing crate, so a `-p <crate>` run can look green while the workspace is broken.
7. **Fixture `0099-accesslog-not-health-check-filter`**: NEW, cluster-free and backend-free, through the EXISTING `Driver::Http1AccessLogByteExact`. See §6.
8. **`BEHAVIOR_CONTRACT.md`**: the §2.1 acceptance surface and the §2.2 runtime rule, added to the access-log filter section that phases 70–74 and 114 built.

---

## 5. Non-goals — rejected fail-loud or banked, each with the reason

1. **The other four unbuilt arms are OUT**, and each stays rejected on its own landed ground.
   - `duration_filter`: `ADR-0192` (d). The predicate is request DURATION, and `BEHAVIOR_CONTRACT.md` excludes timing from comparison by default.
   - `runtime_filter`: `ADR-0192` (c). A sampling predicate is differentially assertable only at its degenerate 0% and 100% cells.
   - `traceable_filter`: `ADR-0196` (d). There is no tracing decision anywhere in the tree.
   - `extension_filter`: `ADR-0196` (e). It needs an unbuilt extension-registry seam.

   None of those grounds is touched by this phase. Banked as **CF-116-1**.
2. **No HTTP/2 differential witness.** The arm and its data axis are specified for **both** codecs. The H2 predicate exists at `envoy-http2/src/hcm.rs:1133`, and the intercepted H2 reply already carries `health_check_ok` (`envoy-http2/src/hcm.rs:971`). What is out of scope is a second fixture through `Driver::Http2AccessLogByteExact`. The H2 behaviour must instead be **pinned by an in-process test**, so that a later phase lifting this boundary has to delete an explicit assertion rather than silently change behaviour. Banked as **CF-116-2**. This is consistent with `CF-114-3` and `CF-115-4`.
3. **Pass-through mode is NOT reopened.** Upstream marks a request as a health check in `pass_through_mode: true` too, even though it forwards the request. envoy-rust rejects `pass_through_mode: true` at boot (`CF-115-5`), so that cell cannot be expressed. Implementing pass-through mode is `CF-115-5`'s phase, not this one. When it lands, the health-check bit must be set by the filter's **match**, not by its local reply. That constraint is recorded here so a future implementer does not tie the bit to `StopAndSend`. **CF-115-5 stays BANKED and UNCONSUMED.**
4. **The `%RESPONSE_CODE_DETAILS%` gap on non-health-check filter replies is NOT fixed.** §2.2 row 6 shows upstream rendering `rbac_access_denied_matched_policy[deny-flagged]` for the RBAC deny. envoy-rust's RBAC filter sets no `details`, so it renders `-`. That pre-existing gap is unrelated to this arm. **It constrains the fixture (§6), and it is not a deliverable.** Banked as **CF-116-3** so the next session that touches RBAC sees it.
5. **No new dependency, harness driver, fuzz target or workspace crate.** `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml` and `tests/differential/src/lib.rs` are all expected to stay **untouched**. §7.4's *"parser, codec, or filter"* fuzz trigger is discharged by the pre-existing targets: this arm adds no parser, and its config is serde over an empty message. **If the PLAN-write finds it must touch any of those four files, the scope has drifted. Stop and re-scope.**
6. **Nothing is fixed.** Per §6.3 and `ADR-0165`, this phase consumes no unrelated carry-forward. The phase-112 ALPN rider stays a RIDER (`ADR-0192` DECISION 5). **Do not re-cost it.**

---

## 6. The differential witness — fixture `0099-accesslog-not-health-check-filter`

**Numbering re-derived at the pick.** `tests/fixtures/` holds **98** directories, and the highest is `0098-http-filter-health-check-transfer-encoding`. This phase's fixture is therefore `0099`.

**Driver: the EXISTING `Driver::Http1AccessLogByteExact`** (`tests/differential/src/lib.rs:170`), used by **32** landed fixtures' `expectations.yaml`. It already has the two properties this phase needs:

- `AccessLogByteExactProbe.expect_logged: bool` (`:1207`, default `true`) marks a probe whose record a filter must SUPPRESS on both proxies. Phase 70 (`ADR-0141`) added it.
- `AccessLogByteExactProbe.method` and `extra_headers` (`:1192` onward) let a probe send `POST` and `x-deny`.

The comparison takes exactly ONE log file per side (`AccessLogPaths`, `:1178`), so the fixture configures **one** filtered sink, `filter: { not_health_check_filter: {} }`.

**Config.** It mirrors §2.2 listener A: `[rbac DENY on x-deny present, health_check exact /healthz, router]`, with a `direct_response` catch-all. ⚠ **The sink's format must NOT render `%RESPONSE_CODE_DETAILS%`** while the RBAC row is in the probe set. The RBAC 403 is a KEPT row, and §5 non-goal 4 makes its details diverge (`rbac_access_denied_matched_policy[deny-flagged]` upstream, `-` on envoy-rust) for a reason unrelated to this arm. A format of `%REQ(:METHOD)% %REQ(:PATH)% %RESPONSE_CODE%` plus a constant is enough. The intercepted rows are dropped, so they render nothing anyway.

**Probe set.** Each probe uses a distinct (method, path, headers) combination so that every kept line can be attributed:

| # | method | path | extra headers | expected status | `expect_logged` | the rule it pins |
|---|---|---|---|---|---|---|
| 1 | GET | `/healthz` | — | 200 | **false** | rule 1, the decisive drop |
| 2 | POST | `/healthz` | — | 200 | **false** | rule 1, method-agnostic |
| 3 | GET | `/healthz?x=1` | — | 200 | **true** | rule 1, a near-miss falls through and is kept |
| 4 | GET | `/other` | — | 200 | **true** | the plain control |
| 5 | GET | `/healthz` | `x-deny: 1` | 403 | **true** | ⚠ rule 3, the filter must RUN |
| 6 | GET | `/ua` | `user-agent: Envoy/HC` | 200 | **true** | ⚠ rule 2, not keyed on the user-agent |

**Probes 5 and 6 make the fixture non-vacuous against the plausible wrong implementations.** Probes 1 and 2 already defeat an always-true arm, and probes 3, 4 and 6 defeat an always-false arm. Probe 5 defeats an arm that re-evaluates the health-check matcher against the request instead of reading the filter's decision. Probe 6 defeats one that keys on upstream's health-checker user-agent. §2.2 row 8 (no health-check filter on the listener) cannot fit in the same fixture, because one fixture holds one listener's chain. It is pinned in-process instead, and the PLAN may add a second fixture if it judges the cell worth one. That is **PV-5**.

**Local verifiability.** The fixture is cluster-free and backend-free, so it can be verified on the development host. Fixture `0095` is the nearest template (`clusters: []`, two byte-identical YAMLs, `{{PORT}}` the only token). `Http1AccessLogByteExact` is not one of the drivers that receive `{{ADMIN_PORT}}`, so the fixture must not reference that token. **PV-4** requires the exact YAML to be dry-run on BOTH proxies before any code is written.

---

## 7. Size estimate and the §6.1 split gate

Bottom-up, net lines of change, **excluding `docs/`**:

| piece | est. |
|---|---|
| `NotHealthCheckFilter` struct + the eighth `AccessLogFilter` arm + its doc comments | 15–25 |
| `validate_access_log_filter` eight-arm growth | 5–10 |
| the `AccessLogFilter` literal `E0063` sweep (14+ sites, 2 crates) | 15–35 |
| `compile_access_log_filter` eight-tuple growth, with rustfmt reflow | 15–35 |
| `LogFilter::NotHealthCheck` + its `should_log` arm | 15–25 |
| the health-check data axis (PV-2): the sixth-parameter route is 2 production sites + a ~144-site test sweep | 150–250 |
| unit and mutation tests across parse / validate / compile / predicate / both codecs' bit | 150–250 |
| fixture `0099` (4 files) + its runner test file | 200–260 |
| **code subtotal** | **≈ 565–890** |
| `BEHAVIOR_CONTRACT.md` section *(docs, excluded from the gate)* | 60–100 |

**Central ≈ 730 code lines**, against a gate of ~25 tasks or ~1500 net LoC. The task count, projected at 7–9, is nowhere near its limit.

**Calibration.** On this project, PROJECTED estimates have landed **1.33×** (`110.2`) and **1.66×** (`111`) over. MEASURED-on-a-prototype estimates have landed at 1.00×, 1.07× and 1.10× (`112.1`, `113`, `112.2`). The closest-shaped comparators are the landed access-log filter-arm phases, each measured over its full arc as `crates/` + `tests/` net: phase 71 **917**, phase 72 **1064**, phase 73 **873**, phase 114 **1038** (the phase-114 figure is from `ADR-0198`; the other three from `ADR-0196`). Phase 114 carried a 17-name token grammar this phase does not have, and phase 73 carried two arms. **⚠ The comparators sit ABOVE this estimate's top end.** That suggests the bottom-up figure is low, not that this phase is unusually cheap. At the worst recorded multiplier (1.66×) the central figure lands near **1210** and the top of the band near **1480**, just under the gate.

**Therefore: the §6.1 split is NOT projected, but it is not excluded, and NO ADR number is reserved for it.** The state-2 PLAN-write **must** re-derive the estimate on a prototype in a scratch worktree with its own `CARGO_TARGET_DIR`, and must re-measure **after the final edit to the plan** (`ADR-0189`: a measured estimate goes stale when the plan is edited afterwards). Per `ADR-0194` DECISION 2, **a whole-slice prototype validates the SLICE, never a TASK BOUNDARY**.

If the gate fires anyway, the cut follows the `114`/`115` pre-declared precedent:

- **`116.1`**: config surface, validator, `LogFilter::NotHealthCheck`, the data axis and the literal sweep, witnessed entirely in-process with no new fixture.
- **`116.2`**: fixture `0099`, the `BEHAVIOR_CONTRACT.md` section and the parent close.

---

## 8. PLAN-VERIFY items — what state 2 must measure, and why each is load-bearing

- **PV-1 — Re-derive every `file:line` in this document.** The anchors are:
  - `bootstrap.rs:730`/`:731`/`:732`/`:5908`;
  - `lib.rs:475`;
  - `filter.rs:68`/`:121`;
  - `file_sink.rs:103`;
  - `health_check.rs:31`;
  - `envoy-http1/src/hcm.rs:1554`/`:1605`/`:1880`;
  - `envoy-http2/src/hcm.rs:971`/`:1133`/`:1236`;
  - `tests/differential/src/lib.rs:170`/`:1178`/`:1192`/`:1207`.

  Locate each by **TEXT**, and assert that the anchor occurs **exactly once** before trusting the line it returns. Any edit to `hcm.rs` moves them.
- **PV-2 — Choose and record how the health-check bit reaches `should_log`.** The options are a sixth parameter or an `AccessLogRecord` field (§4 item 4). Also decide whether the bit is derived from the existing `details == health_check_ok` predicate or carried explicitly from the filter. ⚠ If it is derived from the details string, say in a doc comment that any future filter reusing `health_check_ok` would silently be treated as a health check by BOTH the counters and this arm. `FilterResponse::details`' own doc comment (`crates/envoy-filter/src/types.rs`) already warns about the counter half.
- **PV-3 — Confirm the bit is FALSE on every non-intercept path on both codecs.** Those paths are a fall-through to the route, an earlier filter's local reply (§2.2 row 6), a route miss and an upstream-routed response. Do this by mutation: force the bit `true` and show that fixture probes 3–6 go RED. **A test asserting absence passes vacuously against a placeholder, so plan the mutation as its red evidence.**
- **PV-4 — Dry-run the exact fixture `0099` YAML against BOTH proxies, end to end, before writing any code.** This is the highest-value item.
  - Confirm that envoy-rust's RBAC accepts the `action: DENY` + `present_match` principal shape and answers the probe-5 request with 403 on the wire.
  - Confirm that envoy-rust's 403 status and the kept line's rendered fields match upstream byte-for-byte under the chosen format.
  - Re-check the §5 non-goal 4 `%RESPONSE_CODE_DETAILS%` divergence before deciding the format.
- **PV-5 — Decide and record the witness for §2.2 row 8** (a listener with no health-check filter): a second fixture, or in-process only plus a carry-forward.
- **PV-6 — `not_health_check_filter: ~` on the envoy-rust side.** Upstream reads a YAML null as an UNSET oneof (§2.1). Under serde, an `Option<NotHealthCheckFilter>` will most likely also read `~` as `None`, and the validator would then reject it as "no filter variant is set", which is the same class of rejection. Measure it rather than assume it, and pin it with a test.
- **PV-7 — Re-measure the §7 estimate on a prototype**, and again after the final edit to `PLAN.md`.
- **PV-8 — Confirm that `Cargo.toml`, `Cargo.lock`, `ci.yml` and `tests/differential/src/lib.rs` are untouched** by the finished plan (§5 non-goal 5).

---

## 9. Close-out clause

When this phase reaches §5 state 6, the close-out flips its ROADMAP row to `done`.

⚠ **DERIVE the not-done set from `ROADMAP.md` at the close-out. Do not trust this sentence.** A close-out flips the **status cell only**. It splits each row on `' | '`, asserts six cells and the expected current status, replaces that one cell and re-joins. If this phase splits (§7), the parent row flips only once every sub-phase row is `done`.

Two `ROADMAP.md` rows carry unescaped in-cell pipes and split into 7 and 10 fields under `' | '`. They are append-only history and **must not be "fixed"**. Any census must split on `' | '` **with the spaces** and take the status from field **4**. Never filter on `NF == 6`, which silently drops exactly those two rows.

---

## 10. Carry-forwards opened by this phase

- **CF-116-1**: four `AccessLogFilter` arms remain unbuilt. `duration_filter` and `runtime_filter` carry `ADR-0192` (d)/(c); `traceable_filter` and `extension_filter` carry `ADR-0196` (d)/(e). (§5 non-goal 1.)
- **CF-116-2**: the HTTP/2 arm of `not_health_check_filter` has no differential witness, only an in-process pin. (§5 non-goal 2.)
- **CF-116-3**: envoy-rust's RBAC filter sets no `%RESPONSE_CODE_DETAILS%`. Upstream renders `rbac_access_denied_matched_policy[<policy>]`. The gap is pre-existing and was found by this pick's measurement. (§5 non-goal 4.)

**`CF-115-3` is CONSUMED by this phase.** It banked exactly this arm as phase 115's successor. **`CF-114-2` is AMENDED, not consumed**: its five-arm list shrinks to four, which CF-116-1 restates. Every other banked carry-forward carries forward **INTACT and UNCONSUMED**, including `CF-115-1`, `CF-115-2`, `CF-115-4` … `CF-115-16`, the phase-115 Minors, `CF-75-5` and the phase-112 ALPN rider.
