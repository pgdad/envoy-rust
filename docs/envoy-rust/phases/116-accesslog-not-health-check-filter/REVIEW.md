# Phase 116 — the `not_health_check_filter` access-log FILTER arm — CODE REVIEW

> **§5 state 5.** This document is the state-5 output for phase `116`. It **closes §7.5 gate (f)**, the
> only gate the state-4 session left open. It was written by a THIRD context: the state-3 session
> implemented, the state-4 session graded, and neither may review (§5.1; `ADR-0127`).
>
> **Verdict: APPROVED.** §2 (Issues — Must Fix) is **EMPTY**, so the state machine advances to
> **state 6**, not back to state 3 (§5.2).
>
> **This review wrote no code, no test and no fixture, and edited no landed artifact.** Every finding
> below is **BANKED** as a carry-forward, not fixed (§6.3; `ADR-0165`).
>
> **Findings: 0 Critical, 0 Must-Fix, 3 Important, 8 Minor**, opening **`CF-116-4` … `CF-116-8`**. No ADR
> fires: an APPROVED verdict carries no design decision (the phase-115 round-4 precedent, `8cc1a43`), and
> the one standing figure this review corrects (the "twelve arms" inventory, I-1) is a record correction
> made here, not a decision.
>
> **The composition sweep found NO phase divergence.** Fourteen untested compositions were run against
> BOTH proxies — filters before and after `health_check` on the encode side, a gRPC request, HTTP/2,
> a route miss, `invert_match`, two health-check filters, a listener with none, four nested
> `and`/`or` sinks, `HEAD`, keep-alive and pipelining, admin `/healthcheck/fail`. On every cell the
> `not_health_check_filter` sink kept and dropped exactly the records upstream kept and dropped.
>
> **The three Important findings are one inventory error and two latent hazards, none of them a wrong
> value today.** (1) Upstream's `AccessLogFilter` oneof has **THIRTEEN** arms, not twelve —
> `log_type_filter` is MEASURED accepted by the pinned image and is absent from every phase-116
> inventory, so `CF-116-1`'s "four arms remain unbuilt" is really **five**. (2) The agreement between
> the `downstream_rq_Nxx` exclusion and the log decision — the reason the phase made ONE predicate — is
> **untested**: rewiring either codec's counter site to a request-path predicate leaves every test GREEN.
> (3) The bit is keyed on the details string `health_check_ok`; upstream ALSO drops its 503
> `health_check_failed` reply (MEASURED), so the phase that lands `CF-115-2` must widen the predicate or
> mint a divergence.

---

## §0 — How this review was conducted

### §0.1 — Scope

The code surface is the six task commits `41b3ceb` `59e693a` `ae5015c` `71b186b` `3777c05` `d7aeabe`,
diff base `6cd69df`. Re-derived at this session:

```
$ git diff --shortstat 6cd69df d7aeabe -- . ':(exclude)docs/'
 12 files changed, 1154 insertions(+), 176 deletions(-)        NET 978
$ git diff --numstat 6cd69df d7aeabe -- docs
90	0	docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

978 against `PLAN.md`'s MEASURED 978 is **1.000×**; the §6.1 gate (~25 tasks OR ~1500 net) does not fire
(6 tasks, 522 lines of margin).

⚠ **Out of scope:** the out-of-phase `deps:` commit `6b9b481` (`rustls` 0.23.45 → 0.23.46, one scoped
`deny.toml` ignore, `ADR-0207`). It was sanity-checked only: the `deny.toml` change is a single
advisory ID (`RUSTSEC-2024-0449`) with a reason naming the `serde_yaml` chain and `ADR-0207`. That is
narrowly scoped.

**Document precedence, later wins:** `SPEC.md` < `PLAN.md` / `ADR-0206` < `PROGRESS.md`. `ADR-0206`
corrects three `SPEC.md` claims (the derive that would load `[]`, the production `should_log` site count,
and the size). This review adjudicates against the latest binding statement of each claim.

### §0.2 — Method

Three read-only reviewers were dispatched in parallel, each in its OWN scratch subdirectory
(`scratchpad/dimA`, `dimB`, `dimC`), with zero-context briefs (D-3.4):

- **A — production-code correctness:** the hand-rolled `Deserialize`, every writer of the details local
  on both codecs, every access-log emission site, the recursion threading, the eight-tuple compile and
  the validator. A probe crate copied the visitor verbatim against the pinned `serde_yaml 0.9.34`.
- **B — differential composition probes:** fourteen configurations against `envoyproxy/envoy:v1.33.0`
  (image id checked on every run) and the landed debug `envoy-bin`, each with an unfiltered CONTROL sink
  beside the `not_health_check_filter` sink, readiness gated on a real HTTP response, containers named
  `p116-*` and removed afterwards. No neighbour container was touched.
- **C — test discrimination and record accuracy:** twelve mutations in its own worktree with its own
  absolute `CARGO_TARGET_DIR`, an unmutated control first, every `sed` target asserted unique; every
  `PROGRESS.md` figure and every contract and README claim re-derived.

⚠ **A SUBAGENT FINDING IS A CLAIM.** This main session re-verified each charged finding itself before
recording it:

- **I-1:** re-measured with its own `--mode validate` runs (positive, negative-in-arm and negative-arm
  controls; §3).
- **I-2:** re-ran mutation m11 in its own scratch worktree: control `256 passed; 0 failed`, mutation
  (with `Compiling envoy-http1` seen) `256 passed; 0 failed`, restored, worktree removed.
- **I-3:** read the reviewer's saved upstream and envoy-rust logs for cell 8a.
- **The Minors:** confirmed by `git grep` / `sed -n` on the tree.

The pre-existing observations in §6 that the main session did NOT re-run are labelled as such.

### §0.3 — The §7.5 gate was NOT re-run

Legs (a)–(e) were discharged at the state-4 gate on the post-`deps:` tree `6b9b481`, with real output
quoted in `PROGRESS.md`'s `# §5 STATE 4` section, and CI recorded `binaries=175 passed=2374 failed=0` on
both `6b9b481` (attempt 2) and the gate commit `2045809`. This review inherits them as data and
re-verified the figures they rest on:

| claim | source | re-derived here |
|---|---|---|
| net code lines | `ADR-0206`, `PROGRESS.md`: 978 | **978** (1154 / 176, 12 files) ✅ |
| `BEHAVIOR_CONTRACT.md` | `PROGRESS.md`: `90 0` | **`90 0`** ✅ |
| new tests | `PLAN.md`: 14 | **14** added `#[test]`/`#[tokio::test]`, **0** removed ✅ |
| per-task numstats | `PROGRESS.md` | 224/135, 54/4, 245/39, 288/1, 346/0 (reviewer C) ✅ |
| production `should_log` / `emit` sites | `ADR-0206`: two dispatch sites, five production `should_log` | exactly two emission sites (H1, H2), both passing the shared predicate; And/Or recursion and `FileSink` delegation thread it; no production literal `false` (reviewer A) ✅ |
| PV-8 untouched files | `PROGRESS.md` | the six task commits leave `Cargo.toml`, `Cargo.lock`, `.github`, `tests/differential/src/lib.rs` untouched ✅ |
| fixture pair | `PROGRESS.md` | exactly four harness hunks; the `filter:` block md5 `22ce799d…` on both sides (reviewer C) ✅ |
| fixture census | — | **99** fixture directories, **98** differential runner files ✅ |
| ADR head | `STATE.md`: `ADR-0207` | highest `^## ADR-` = **0207**; `ADR-0208` absent ✅ |
| ROADMAP | `STATE.md` | row `116` still `planned`, file line **198** ✅ |

Every figure reproduced. **No decision-carrying number in the record was contradicted.**

---

## §1 — Strengths

**The predicate reads the filter's DECISION, and it is the same value at all four call sites.**
`answered_by_health_check` (`crates/envoy-filter/src/health_check.rs:44`) is `details ==
Some("health_check_ok")`. On H1, `response_code_details_for_log` (the counter exclusion, `hcm.rs:1553`)
is moved unchanged into `record.response_code_details` (the `should_log` argument, `:1614`). On H2,
`response_code_details_for_log_h2` feeds both the counter test (`hcm.rs:1132`) and the record literal
read at `:1244`. Reviewer A enumerated every writer of the details local on both codecs. All of them are
in the decode-side dispatch. The encode pass starts a fresh `details: None` that is never written back.
`apply_grpc_local_reply` and `settle_headers_only_framing` do not touch details. No production filter
returns an encode-side replacement. And `health_check.rs:131` is the ONLY production producer of
`health_check_ok` — the main session confirmed `details: Some` occurs in exactly that one non-test line
across `crates/envoy-filter/src/`.

**The `[]` trap was caught at state 2 and is pinned.** The hand-rolled map-only visitor rejects `[]`,
`[1]`, `true`, `0`, `""` and `{foo: 1}`. Reviewer C's mutation m6 — swapping in a derived `Deserialize`,
with and without `deny_unknown_fields` — turns `not_health_check_filter_rejects_every_measured_reject`
RED (4/1) both ways. Reviewer A's probe crate added `{1: 2}`, `{~: 1}`, `{? x}`, `{<<: {}}` and
`!!seq []` (all rejected) and `null`/`Null`/`~`/JSON `null` (all an unset arm). Those match upstream's
measured classes.

**Every mutation of phase-116 code goes RED.** The mutation table is in §5. Both recursion arms (`And`
m1b, `Or` m1) are caught by the composition test. The `FileSink` delegation (m2) is caught by both codec
pins. The arm's polarity (m5) is caught at three layers. A user-agent-keyed bit (m9) is caught by the H1
pin. These pins are real end-to-end tests: they push real bootstrap YAML through `parse_bootstrap` →
`HCMConfig::from_config` → the real HCM, and they assert the sink file's exact contents.

**All the decisive cells are pinned in-process on BOTH codecs:**

- the RBAC-denied `/healthz` (kept);
- the fall-through `/healthz?x=1` (kept);
- a `/healthz` on a listener with no health_check filter (kept);
- an `Envoy/HC` user-agent (kept).

On H1 the pin also covers a `HEAD` intercept and pipelining in both orders.

**The landing was exact.** Every per-file and per-task numstat matched the plan to the line (1.000×).
Fixture `0099` went RED-before (boot-level), GREEN-after and was mutation-proved. No suppression was
added: `#[allow(` / `#[expect(` / `unsafe` / `#[ignore` = 0 on added lines (state 4).

**The composition sweep found the implementation at parity everywhere it could reach.** The 14-cell
table is in §4.

---

## §2 — Issues (Must Fix)

**None.** No finding is a wrong value against a measured upstream cell reachable by an in-scope config,
a broken test, or a false claim that changes what a later session would build. §5.2 does not fire.

---

## §3 — Important (banked)

### I-1 — upstream's `AccessLogFilter` oneof has THIRTEEN arms; `log_type_filter` is missing from every phase-116 inventory, so FIVE arms remain unbuilt, not four

**What.** The phase-116 artifacts all say "twelve arms":

- `SPEC.md` §1 (five unbuilt), §5 non-goal 1 and §10 `CF-116-1` ("four arms remain unbuilt");
- `BEHAVIOR_CONTRACT.md` §D (`:3732`, "**Four arms remain unbuilt**");
- the fixture README (`tests/fixtures/0099-…/README.md:4`, "the EIGHTH of upstream Envoy's twelve");
- the runner's module doc (`tests/differential/tests/accesslog_not_health_check_filter.rs:2`).

Upstream v1.33.0 accepts a thirteenth arm.

**Evidence (main session, pinned image, `--mode validate`, one bootstrap differing only in `filter:`):**

```
{ log_type_filter: { types: [DownstreamEnd] } }  ->  configuration '/cfg/e-5d4d17fb.yaml' OK
{ log_type_filter: { bogus: 1 } }                ->  … filter.log_type_filter: message envoy.config.accesslog.v3.LogTypeFilter … no such field: 'bogus'
{ bogus_filter_xyz: {} }                         ->  … message envoy.config.accesslog.v3.AccessLogFilter … no such field: 'bogus_filter_xyz'
```

The in-arm negative control shows that the validator resolved `LogTypeFilter` as a real message. The
arm-level control shows that the oneof is not a free map.

**Origin.** The arm appears in the phase-70 `SPEC.md` (1 occurrence). It has 0 occurrences in
`BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `crates/` and `tests/`. It dropped out of the inventory by phase
114's "twelve", and no ADR decided that.

**Why it matters.**

- No silent divergence exists: envoy-rust's `deny_unknown_fields` rejects the key at boot.
- But `CF-116-1` is the enumeration the next filter-arm pick reads, and it is incomplete.
- The stop-condition leg (ii) prose ("N of the twelve access-log filter arms unbuilt") inherits the wrong
  denominator.

**Banked as `CF-116-4`**, which **AMENDS `CF-116-1`**: five arms remain — `duration_filter`,
`runtime_filter`, `traceable_filter`, `extension_filter` **and `log_type_filter`**. The fifth has no
standing rejection on record. Its runtime semantics (the record's log type — downstream-end vs periodic
vs upstream records) are UNMEASURED, and a future pick must measure them before costing it.

### I-2 — the counter exclusion and the log decision are claimed to share ONE definition so they "cannot disagree", but nothing tests the agreement: a request-keyed counter site survives every test on both codecs

**What.** The design reason for `answered_by_health_check` (`ADR-0206`; the predicate's own doc, "so the
two can never disagree"; the contract's "the SAME predicate … so the two consumers cannot disagree") is
true on disk: all four call sites call it. But no test asserts it. The phase-116 pins never read the
counters. The phase-115 counter tests drive only an intercept and the `?x=1` fall-through, never the
RBAC-denied `/healthz` — and that is the one cell where a request-keyed predicate and the decision
disagree.

**Evidence.**

- **Reviewer C:** m11 (H1 counter site keyed on `req.path == "/healthz"`) → `envoy-http1` 256/0 GREEN.
  m12 (the H2 twin, on `envoy_req.path`) → `envoy-http2` 130/0 GREEN.
- **Re-run by this session in its own worktree:** unmutated control `test result: ok. 256 passed; 0
  failed`. Then the mutated tree, `Compiling envoy-http1 …` seen, read `test result: ok. 256 passed; 0
  failed`. The file was restored by byte copy and the worktree removed.
- By contrast, disabling the exclusion outright (m3/m4) IS caught, by the phase-115 end-to-end tests.

**Why it matters.** A future edit to either codec's counter site can re-introduce the very drift the
phase was designed to prevent, and nothing would catch it. **Remedy:** one assertion beside the RBAC row
in either pin (`downstream_rq_4xx == 1`, and `downstream_rq_2xx` excluding the intercepts). **Banked as
`CF-116-5`.**

### I-3 — the health-check bit is keyed on the details string `health_check_ok`; upstream ALSO drops its 503 `health_check_failed` reply, so the phase that lands `CF-115-2` must widen the predicate or mint a divergence

**What.** Upstream sets its health-check bit when the filter MATCHES, independently of which reply it
sends. Reviewer B's cell 8a sent admin `POST /healthcheck/fail` and then `GET /healthz`. The main
session read the saved logs (`scratchpad/dimB/runs/c8-4fff7e/`):

```
upstream CTL: GET /healthz 503 d=health_check_failed f=LH …      upstream NHC: (dropped)
rust     CTL: GET /healthz 200 d=health_check_ok f=- …           rust     NHC: (dropped)
```

Today the `not_health_check_filter` outputs agree, because envoy-rust never sends the 503. That is the
banked `CF-115-2`: `decode_headers` does not consult the failed state.

**The hazard.** When `CF-115-2` lands with a details string other than `health_check_ok`, two consumers
silently stop treating that reply as a health check:

- the `downstream_rq_Nxx` exclusion;
- this arm.

`answered_by_health_check`'s doc warns only in the other direction (another filter reusing
`health_check_ok`). ⚠ This is the same lesson `SPEC.md` §5 non-goal 3 recorded for pass-through mode:
the bit must follow the filter's MATCH, not one particular reply. The rule now has a second measured
instance.

**Banked as `CF-116-6`**, which **AMENDS `CF-115-2`**: its phase must carry the bit for every
health-check reply. That can be done by widening the predicate or by an explicit filter-carried bit (the
alternative `ADR-0206` rejected only for the single-reply world). It must also pin the 503 cell on both
consumers.

---

## §4 — The composition sweep (reviewer B), adjudicated

Every cell ran on BOTH proxies with a no-filter control sink (CTL) beside the `not_health_check_filter`
sink (NHC). The format rendered method, path, status, `%RESPONSE_CODE_DETAILS%`, `%RESPONSE_FLAGS%`,
`%PROTOCOL%` and user-agent.

| # | composition | NHC keep/drop |
|---|---|---|
| 1a | `[header_mutation(req+resp), cors, fault(header-gated abort), health_check, router]`, a CORS route policy | PARITY — the intercepts dropped and still decorated by the encode-side filters; preflight and fault 503 kept |
| 1b | `[local_ratelimit(1 token), health_check, router]` | PARITY — every 429 kept |
| 1c | `[health_check, local_ratelimit, header_mutation, router]` | PARITY |
| 2 | gRPC request on H1 (`content-type: application/grpc`), with and without `x-deny` | PARITY — intercepts dropped; status/headers also equal (200 + `grpc-status: 2`; deny → 200 + `grpc-status: 7`) |
| 3 | H2 prior knowledge: intercept, `?x=1`, `x-deny`, `POST`, gRPC, gRPC + deny, `HEAD` | PARITY on all 7 (gRPC header differences pre-existing, §6) |
| 4 | route table with no `/healthz` route; matcher with `invert_match` on `x-skip` | PARITY, byte-identical — a route-miss `/healthz` is intercepted and dropped; with `x-skip` it is a kept 404 `route_not_found` |
| 5a | two health_check filters (`/healthz`, `/livez`) | PARITY, byte-identical |
| 5b | `SPEC.md` row 8: no health_check filter | PARITY, byte-identical — everything kept |
| 6 | four nested sinks: `or[and[nhc, GE 300], x-keep]`, `or[nhc, GE 400]`, `and[nhc, user-agent exact Envoy/HC]`, `or[and[nhc, nhc], x-keep]` | PARITY row by row (only `d=` differs, `CF-116-3`) |
| 7 | H1 `HEAD`; 4 requests on one connection; POST-with-body then pipelined GET; slow and truncated bodies | PARITY, NHC log byte-identical |
| 7b | H2 client abort mid-body | NHC agrees for `/healthz` |
| 8a | admin `/healthcheck/fail` then `/healthz` | NHC byte-identical — but see I-3 |
| 8b | `connection: close`; HTTP/1.0 | PARITY; HTTP/1.0 differs on both sinks (pre-existing) |

**No cell diverges on the phase's own decision.** Every difference reproduces on the CTL sink, so none is
charged to this phase. They are listed in §6.

---

## §5 — Minor (banked as one set, `CF-116-7`)

- **M-1 — a TAGGED empty scalar loads as `{}`.** `not_health_check_filter: !!str` (likewise `!!int`,
  `!!seq`, `!!binary`, a bare `!!map`) yields `Some(NotHealthCheckFilter {})` under `serde_yaml 0.9.34`.
  An explicitly tagged empty plain scalar is not null, and `deserialize_map` accepts it as an empty map.
  Upstream most likely reads `""` and rejects it (unmeasured). This is a PRE-EXISTING class: a derived
  sibling struct behaves identically in reviewer A's probe. The input is exotic.
- **M-2 — a stale "SEVEN arms" doc.** `crates/envoy-config/src/bootstrap.rs:5864` (the
  `validate_access_logs` doc) still reads "Phases 70/71/72/73/74/114 give SEVEN arms". The phase updated
  the helper's doc beside it to EIGHT.
- **M-3 — an orphaned one-word doc line.** The `AccessLogFilter` doc carries `/// future` alone on a line
  (`bootstrap.rs` ~`:726`), an artefact of the reflow.
- **M-4 — the probe-6 "proved by MUTATION" claim over-reaches.** `BEHAVIOR_CONTRACT.md` §E and the fixture
  README call probes 5 and 6 the non-vacuity witnesses, "proved by MUTATION". None of the three recorded
  fixture mutations is defeated by probe 6 ALONE:

  | mutation | defeated by |
  |---|---|
  | forced `true` | any kept probe |
  | forced `false` | probes 1–2 |
  | path-derived | probes 3 and 5 |

  A user-agent-keyed bit IS caught, but in-process (reviewer C's m9 turns the H1 pin RED), not at the
  fixture.
- **M-5 — the H2 pin is narrower than its doc.** `crates/envoy-http2/src/hcm.rs:4837` says "the `SPEC.md`
  §2.2 cells". It omits rows 4 (`/other`) and 10 (`x-envoy-internal`) and a `HEAD` intercept. It drives
  intercept-then-route on one connection, never the reverse order the H1 pin also covers.
- **M-6 — the README omits probe 5 from the always-false list.** README `:65` lists "probes 3, 4 and 6"
  as defeating an always-false arm. Probe 5 (kept) does too. The wording is inherited from `SPEC.md` §6.
- **M-7 — the H1 pin doc over-claims "every cell".** The doc says it covers every cell one H1 listener can
  express. Row 9 (`Envoy/HC` on the no-health-check listener) is expressible and absent.
- **M-8 — the delegation is witnessed only above its crate.** `FileSink::should_log`'s delegation (m2) is
  caught only by the two codec pins. `envoy-accesslog`'s own tests pass `false` everywhere and stay GREEN
  under the mutation.

### The mutation table (reviewer C; scratch worktree at `d7aeabe`, own `CARGO_TARGET_DIR`)

Unmutated controls from the same tree: `envoy-accesslog` 137/0, `envoy-http1` 256/0, `envoy-http2` 130/0
(1 ignored), the config filter 5/0.

| # | mutation | result | caught by |
|---|---|---|---|
| m1 | `Or` recursion bit → `false` | RED 136/1 | `not_health_check_arm_composes_as_a_leaf` |
| m1b | `And` recursion bit → `false` | RED 136/1 | same |
| m2 | `FileSink` delegation passes `false` | accesslog GREEN; http1 RED 255/1; http2 RED 129/1 | both codec pins (M-8) |
| m3 | H1 counter exclusion disabled | RED 255/1 | phase-115 `h1_health_check_filter_end_to_end` |
| m4 | H2 counter exclusion disabled | RED 129/1 | phase-115 `h2_health_check_filter_intercepts_and_logs` |
| m5 | `NotHealthCheck => true` | RED 134/3, 254/2, 129/1 | arm tests, compile test, both pins |
| m6 | derived `Deserialize` (± `deny_unknown_fields`) | RED 4/1 both | `…_rejects_every_measured_reject` |
| m9 | H1 log bit OR'd with `user-agent == Envoy/HC` | RED 255/1 | H1 pin |
| m11 | H1 counter keyed on `req.path == "/healthz"` | **GREEN 256/0** (re-run by the main session: same) | none — I-2 |
| m12 | H2 counter keyed on `envoy_req.path == "/healthz"` | **GREEN 130/0** | none — I-2 |

The reviewer discarded two first-draft mutations that did not change the logic (an `!false && …` form,
and an `.or` over always-`Some` details) and re-ran the corrected forms shown. ⚠ A mutation that leaves
the logic intact reads as a vacuous test, and it must be thrown out, not recorded.

---

## §6 — Pre-existing observations, NOT charged (banked for visibility as `CF-116-8`)

These differ on the CONTROL sink too, so they predate this phase and do not touch its decision.

- **(a) Other filters set no details or flags.** `cors`, `fault` and `local_ratelimit` local replies set no
  `%RESPONSE_CODE_DETAILS%` and no `%RESPONSE_FLAGS%`. Upstream renders `cors_response`,
  `fault_filter_abort`/`FI` and `local_rate_limited`/`RL`. This generalises `CF-116-3` (RBAC) to every
  replying filter but `health_check`. **Main session confirmed the mechanism:** `details: Some` occurs on
  exactly one non-test line in `crates/envoy-filter/src/` — `health_check.rs:131`.
- **(b) No record on a client disconnect.** envoy-rust writes NO access-log record for a request whose
  client disconnects mid-body, on H1 or H2. Upstream logs `0 downstream_remote_disconnect DC`.
  `downstream_remote_disconnect` has 0 occurrences in `docs/` and `crates/`, so this is newly observed.
  *Reviewer-measured; not re-run by the main session.*
- **(c) Route chosen after `header_mutation`?** In cell 1a, envoy-rust appeared to select the route AFTER
  request-side `header_mutation` ran, where upstream selected before. *Reviewer-reported from one
  self-loop observation; not re-run by the main session — a hypothesis for whichever phase touches route
  caching.*
- **(d) Already banked:** H2 has no gRPC local-reply transform (phase-115 `REVIEW-2.md`; `CF-110-1`), and
  `codec_type: AUTO` refuses h2c prior knowledge (`CF-115-13`). HTTP/1.0 is served where upstream answers
  426 `low_version` (phase-115 `REVIEW.md`, `REVIEW-3.md`).

---

## §7 — Deliberate decisions verified, not re-litigated

- **A sixth `should_log` parameter over an `AccessLogRecord` field** (`ADR-0206` PV-2). Sound: the bit is
  a derived view of a field already on the record. A field would duplicate it across 21 literals. The
  sweep cost was paid once and is behaviour-neutral (Task 1).
- **Deriving the bit from details rather than carrying it explicitly.** Sound for the world that exists.
  Every loadable config makes "the filter answered" ≡ "details is `health_check_ok`": `pass_through_mode:
  true`, `cache_time` and `cluster_min_healthy_percentages` are boot errors, and the one producer is the
  intercept. I-3 records where that equivalence ends.
- **The H2 witness is in-process only** (`CF-116-2`, consistent with `CF-114-3`/`CF-115-4`). Reviewer B's
  cell 3 measured H2 keep/drop at parity on seven requests, so the banked gap hides no divergence on the
  cells probed.
- **`SPEC.md` row 8 is pinned in-process, with no second fixture** (PV-5). Reviewer B's cell 5b measured
  it byte-identical cross-proxy.
- **The validator's zero-arm rejection of `~`** is the same class as upstream's "`filter_specifier` is
  required" (PV-6). Error text is not part of the contract.
- **The five local Family-A reds and the varying Family-B readiness races** were classified alone at
  states 3 and 4, and CI is `failed=0`. They were not re-litigated.

---

## §8 — Carry-forwards for the state-6 close-out to bank

**Opened by this review.** Nothing here was fixed (§6.3; `ADR-0165`).

| id | what | where |
|---|---|---|
| **CF-116-4** | **AMENDS `CF-116-1`.** Upstream's oneof has THIRTEEN arms; `log_type_filter` is MEASURED accepted and absent from every phase-116 inventory. FIVE arms remain unbuilt; the fifth's runtime semantics are unmeasured. | `SPEC.md` §1/§5/§10; `BEHAVIOR_CONTRACT.md:3732`; fixture README `:4`; runner `:2` (I-1) |
| **CF-116-5** | The counter exclusion ↔ log decision agreement is untested; a request-keyed counter site survives on both codecs (m11/m12). One assertion beside the RBAC row closes it. | `envoy-http1/src/hcm.rs:1553`; `envoy-http2/src/hcm.rs:1132` (I-2) |
| **CF-116-6** | **AMENDS `CF-115-2`.** Upstream drops its 503 `health_check_failed` (`LH`) reply too; the `CF-115-2` phase must carry the bit for every health-check reply and pin the 503 cell on both consumers. | `crates/envoy-filter/src/health_check.rs:44` (I-3) |
| **CF-116-7** | The Minor set M-1 … M-8: tagged-empty-scalar acceptance (pre-existing class), the stale SEVEN-arms doc, the orphaned doc line, the probe-6 mutation over-claim, the H2 pin's narrower-than-documented cells, the README's probe-5 omission, the H1 pin's "every cell" over-claim, and the delegation witnessed only above its crate. | §5 |
| **CF-116-8** | Pre-existing, uncharged: other replying filters set no details/flags; no access-log record on a client mid-body disconnect (H1+H2, newly observed); a reviewer-reported route-after-`header_mutation` hypothesis. | §6 |

**Consumed by this phase, and it stays consumed:** `CF-115-3`.

**Carried forward INTACT:** `CF-116-1` (amended by `CF-116-4`), `CF-116-2`, `CF-116-3`; `CF-115-1`,
`CF-115-2` (amended by `CF-116-6`), `CF-115-4` … `CF-115-16`; the phase-115 Minors; `CF-114-1` …
`CF-114-21` (with `CF-114-2` amended at the pick); `CF-75-5` (re-observed: `cdn_loop_parse` still 0
tracked seeds); the phase-112 ALPN rider (`ADR-0192` DECISION 5, neither taken nor re-costed); the
`serde_yaml` replacement `ADR-0207` names as future work; and every older banked set.

---

## §9 — Assessment, and the §7.5 gate adjudicated

**Ready to merge: YES.** The phase implements the rule it measured. The design choice that mattered —
reading the filter's decision from the final details after the decode pass, never from the request — is
correct on both codecs. Every mutation of phase-116 code is caught, and fourteen untested compositions sit
at parity with upstream. The three Important findings are one wrong inventory denominator and two latent
hazards. None is a wrong value today.

**The pattern worth naming.** This phase's tests pin its OWN consumer thoroughly. What they do not pin is
the claim that motivated the shared predicate, namely that two consumers agree (I-2). Nor do they pin the
assumption underneath it, namely that one reply string is the whole of "a health check" (I-3). Both are
claims about a RELATIONSHIP rather than a value. Neither shows up in a mutation set built from the new
code alone, because the surviving mutations live at the phase-115 site. **A single-definition design
needs a test that reads BOTH consumers on the cell where a wrong definition would split them.**

### The §7.5 gate, all six adjudicated

- **(a) new/changed differential fixtures green** — PASS. `0099` is GREEN locally (states 3 and 4) and in
  CI (`2045809`, run `38716757551`).
- **(b) pre-existing fixtures still green** — PASS. CI `binaries=175 passed=2374 failed=0`. The local reds
  were classified alone.
- **(c) conformance at threshold** — PASS, CI-authoritative (`h2spec not found` = 0 in the CI job log).
- **(d) new fuzz target clean** — PASS, vacuously by letter (no new target). In substance, all five
  pre-existing targets exit 0 at the CI budget, and `parse_bootstrap` covers the new config surface.
- **(e) the five `cargo` legs** — PASS on the post-`deps:` tree after a forced dirty set, with a clippy
  negative control.
- **(f) `REVIEW.md` approved** — **PASS. This document.** §2 is empty.

**All six PASS. The phase is done under §7.5 and advances to §5 state 6.**

### Stop condition — re-derived from disk at this review. ALL THREE LEGS FALSE

- **Leg (i) — FALSE.**
  - `ROADMAP.md`: **124** rows — **123 `done`, 1 `planned`** (row `116`, file line 198, untouched by a
    review).
  - Status is field 4 of a `' | '` split over the `^\| [0-9]` rows. The field-count histogram is
    `{6: 122, 7: 1, 10: 1}`.
- **Leg (ii) — FALSE.**
  - **14** crates.
  - `quinn` / `wasmtime` / `tonic` / `opentelemetry` / `prost` = **0** of the manifests from
    `git ls-files '*Cargo.toml'`, against a `tokio` control of **19** by the identical invocation.
  - `histogram` = **0** over `crates/`, against `gauge` = **365** (`grep -ro … | wc -l`).
  - QUIC/HTTP-3, the gRPC data path, ADS/delta-xDS/SDS/RTDS, hot restart, stats sinks, the WASM host and
    **five** access-log filter arms (I-1) are unbuilt.
- **Leg (iii) — FALSE.**
  - **11** `### ` family headings, every one seeded at 0: 11/5/3/14/3/4/6/32/6/**0**/13, with **27**
    pre-heading rows, summing to 124.
  - `### WASM host family` carries zero rows.

**The mission is NOT complete. No `stop` file exists and none was created** (`ADR-0167` DECISION 2).

### Next state

**§5 state 6 — the CLOSE-OUT — a SEPARATE SESSION** (§5.1; `ADR-0127`). It flips `ROADMAP.md` row `116`
to `done` — **the status cell only** — relocates this phase's `STATE.md` Notes subsections to
`STATE_HISTORY.md` and banks `CF-116-4` … `CF-116-8`. ⚠ Row 116 is the LAST `planned` row, so the
close-out makes leg (i) TRUE. That does NOT complete the mission: legs (ii) and (iii) stay FALSE, and the
next-phase pick must evaluate all three. ⚠ A close-out adds no ADR and no Notes subsection, and its
`## Last commit` carries no PENDING line. ⚠ A landed `REVIEW.md` is never edited; a second round would
write `REVIEW-2.md`.
