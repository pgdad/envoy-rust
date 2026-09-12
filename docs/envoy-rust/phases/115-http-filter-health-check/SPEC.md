# Phase 115 — SPEC

> **`envoy.filters.http.health_check` — the DOWNSTREAM health-check filter, non-pass-through mode.**
>
> Written at the §5 state-0/1 next-phase pick. The governing decision record is
> **`ADR-0200`**. This document is written for a reader with ZERO prior context
> (doctrine D-3.4): every number in it was MEASURED at the pick, against the
> `ENVOY_TARGET.md` pin, and every measurement names its scope and its control.
>
> **Nothing in this document is an implementation plan.** `PLAN.md` is written by
> a separate session (§5 state 2) and must discharge §8's PLAN-VERIFY items
> before writing a task list.

---

## 1. Context — what a stranger needs to know

envoy-rust is a from-scratch Rust reimplementation of the Envoy proxy, verified
phase by phase against upstream Envoy `v1.33.0` (image digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) by a
differential harness that drives identical inputs at both proxies and compares
the results under `BEHAVIOR_CONTRACT.md`.

**`envoy.filters.http.health_check` is an HTTP filter that answers downstream
health probes at the proxy itself.** It sits in the HCM filter chain ahead of the
router. When a request matches its configured header-matcher list, the filter
does not forward it: it short-circuits with a local reply. This is how an
operator gives a load balancer in front of Envoy a cheap liveness endpoint that
never touches an upstream.

**What envoy-rust has today, MEASURED at this pick over `crates/*/src/` (the
fuzz corpus, which is gitignored and full of mutated garbage tokens, is
deliberately OUT of this scope — state the scope with the number):**

| probe | count | reading |
|---|---|---|
| `health_check_filter` | **0** | the filter does not exist |
| `pass_through_mode` | **0** | its governing knob does not exist |
| `envoy.filters.http` (positive control, identical invocation) | **298** | the probe works |

**Twelve** production `HttpFilterInstance` variants are landed — `Router`,
`HeaderMutation`, `LocalRateLimit`, `Rbac`, `Fault`, `JwtAuthn`, `Cors`, `Csrf`,
`Buffer`, `CdnLoop`, `SetMetadata`, `HeaderToMetadata` — alongside two test-only
variants (`TestStopAndSendOnDecode` / `TestStopAndSendOnEncode`) that the census
must not silently fold in. The filter framework, the decode-side
`Decision::StopAndSend` short-circuit and the H1/H2 synthetic-response
decorators are therefore all in place. **This phase adds the THIRTEENTH
production variant.**

### 1.1 Why this filter and not another unbuilt leaf

Two reasons, both measured rather than argued.

**(a) It discharges a recorded blocker.** `ADR-0196` rejected the
`not_health_check_filter` access-log filter arm — one of the five remaining
`AccessLogFilter` oneof arms, banked as `CF-114-2` — on exactly this ground:
*"MEASURED: no downstream health-check filter exists … so the arm would need a
whole filter built first."* That rejection is still true and this phase is what
makes it false. No other candidate on the standing list discharges a blocker a
landed ADR names.

**(b) It is witnessable with an EXISTING driver.** The whole in-scope surface is
a plain HTTP/1.1 request/response cycle with no upstream, so
`Driver::Http1ProbeList` (landed at phase 04.2) drives it unchanged. Several
candidates rejected by `ADR-0196` fell specifically on needing new harness
infrastructure; this one needs none.

---

## 2. The divergence — MEASURED on BOTH sides at the state-0/1 pick

Every measurement below was taken this session against
`envoyproxy/envoy:v1.33.0`. Container ownership was proved per run by
`docker inspect <cid> --format '{{.Image}}'` returning the pin digest above, on
a host port obtained by a bind-then-release `socket.bind(('127.0.0.1', 0))` and
mapped with `-p 127.0.0.1:<port>:10000` (never `--network host`).

> ⚠ **A METHOD TRAP MET AND CORRECTED IN THIS SESSION — do not repeat it.** The
> first pass at §2.3's edge cells used a TCP-connect readiness check
> (`/dev/tcp/127.0.0.1/<port>`) with no settle wait. That check succeeds as soon
> as **Docker's port forwarder** is listening, which is *before Envoy's listener
> is up*, so every probe returned `curl: (52) Empty reply from server`. Read
> naively, that says *"an absent `headers:` list makes upstream silently drop
> every request"* — a believable, entirely FALSE finding that would have driven
> a wrong fail-loud rejection into this SPEC. Re-measured behind an **HTTP**
> readiness poll (drive a throwaway path until the status is not `000`), the same
> configs answer normally and say the OPPOSITE (§2.3 row 7). **Gate readiness on
> a real HTTP response, never on a TCP connect, and treat any all-cells-identical
> result as a harness artifact until a positive control says otherwise.**

### 2.1 Config acceptance — `--mode validate`, networking-free

`--mode validate` is an upstream-only facility (envoy-rust's `envoy-bin` has no
such flag); it probes wire shape and answers nothing about runtime semantics.

| config | exit | upstream's answer |
|---|---|---|
| `pass_through_mode: false` + one `headers` entry | **0** | `configuration '/c.yaml' OK` (baseline) |
| `pass_through_mode` ABSENT | **1** | `Proto constraint validation failed (HealthCheckValidationError.PassThrough…)` — the field is **REQUIRED** |
| `pass_through_mode: true` | **0** | accepted |
| `headers` ABSENT | **0** | accepted |
| `headers: []` | **0** | accepted |
| `pass_through_mode: false` + `cache_time: 5s` | **1** | `cache_time_ms must not be set when path_through_mode is disabled` |
| `pass_through_mode: true` + `cache_time: 5s` | **0** | accepted |

⚠ **The rejection message contains an upstream TYPO — `path_through_mode`, not
`pass_through_mode`.** It is quoted here exactly as upstream emits it. envoy-rust
must NOT reproduce the typo: `BEHAVIOR_CONTRACT.md` requires equivalence of
*behaviour*, and §7.4 is explicit that identical error TEXT is not required.
Recorded so that a future reader meeting this string does not "fix" it here.

### 2.2 The runtime rule — non-pass-through mode

One HCM listener, one `direct_response` catch-all route returning the 4-byte body
`MAIN`, the filter configured `pass_through_mode: false` with
`headers: [{ name: ":path", string_match: { exact: "/healthz" } }]`.

A MATCHED request gets, byte for byte:

```
HTTP/1.1 200 OK
x-envoy-upstream-healthchecked-cluster:
date: <rfc1123>
server: envoy
content-length: 0
```

— empty body, and **no `content-type` at all**, which is the header the
fall-through `direct_response` reply does carry. The
`x-envoy-upstream-healthchecked-cluster` header is present with an **EMPTY
value**.

⚠ **That header is NOT an echo.** A request carrying
`x-envoy-upstream-healthchecked-cluster: SENTINEL` still gets an empty value
back, so the filter renders proxy state, not request state. In non-pass-through
mode there is never an upstream, which is why it is constantly empty here; §8
PV-4 requires state 2 to check whether that survives when a cluster exists.

### 2.3 The matching rule — seven cells, one config

All seven driven against the same ready container.

| # | request | matched? | body length | rule it establishes |
|---|---|---|---|---|
| 1 | `GET /healthz` | **yes** | 0 | the baseline |
| 2 | `GET /healthz?x=1` | **no** | 4 (`MAIN`) | **`:path` INCLUDES the query string** |
| 3 | `GET /healthz/` | no | 4 | `exact` is exact — no trailing-slash tolerance |
| 4 | `GET /healthZ` | no | 4 | the VALUE match is case-SENSITIVE |
| 5 | `GET /other` | no | 4 | non-match CONTINUES down the chain to the route |
| 6 | `POST /healthz` (+ body) | **yes** | 0 | the filter is **method-agnostic** |
| 7 | any path, `headers` absent **or** `[]` | **yes** | 0 | an empty matcher list matches **EVERYTHING** |

With TWO entries (`:path` exact `/healthz` **and** `x-probe` exact `yes`):
`/healthz` alone falls through, `/healthz` + `x-probe: yes` is intercepted,
`/other` + `x-probe: yes` falls through. **The list is AND, not OR.**

⚠ **Cell 2 is the trap of this phase.** The matcher runs against the `:path`
pseudo-header *with the query string still attached*, so the obvious operator
config `exact: "/healthz"` silently stops matching the moment a load balancer
appends a cache-buster. It is a one-line fixture probe and it is the cell most
likely to be got wrong by an implementation that strips the query first.

### 2.4 Observability — the leverage leg, corroborated from two directions

The same listener carried TWO file access-log sinks: an unfiltered `ALL` sink and
a `NOTHC` sink whose only difference is `filter: { not_health_check_filter: {} }`.
Six requests were driven.

```
ALL|/__probe__|200|direct_response|-|4      NOTHC|/__probe__|200
ALL|/healthz|200|health_check_ok|-|0        (no NOTHC row)
ALL|/healthz?x=1|200|direct_response|-|4    NOTHC|/healthz?x=1|200
ALL|/healthz/|200|direct_response|-|4       NOTHC|/healthz/|200
ALL|/healthZ|200|direct_response|-|4        NOTHC|/healthZ|200
ALL|/other|200|direct_response|-|4          NOTHC|/other|200
```

`ALL` = **6** rows, `NOTHC` = **5**. The single dropped row is exactly the one
request the filter intercepted. Two independent readings of one run agree: the
wire response says the filter fired, and the filtered sink says the same request
carries the health-check flag. This is corroboration, not one measurement read
twice.

It also yields a new `%RESPONSE_CODE_DETAILS%` value — **`health_check_ok`** —
alongside `%RESPONSE_FLAGS%` `-` and `%BYTES_SENT%` `0`. envoy-rust already has a
landed RCD comparison surface (phases 42/46/47/54/65), so this is a value to
produce, not machinery to build.

### 2.5 The admin interaction — MEASURED, and deliberately OUT of scope

After `POST /healthcheck/fail` on the admin listener, `GET /healthz` returns
**503** with `x-envoy-immediate-health-check-fail: true`, `connection: close` and
`content-length: 0`; `POST /healthcheck/ok` restores the 200. ⚠ **The
`x-envoy-immediate-health-check-fail: true` header and `connection: close` also
decorate NON-health-check responses** — `GET /other` carried both — so this is an
HCM-level behaviour, materially wider than the filter itself. §5 banks it, and §8
PV-5 explains why.

### 2.6 Config portability — one file parses on BOTH sides

envoy-rust's landed `HeaderMatcher` implements all **seven** modes, `string_match`
among them (**68** hits in `crates/envoy-config/src/`), and upstream `v1.33.0`
still accepts the deprecated flat `exact_match:` form (`--mode validate` exit
**0**). Either spelling therefore parses on both sides, so the fixture needs **no
config divergence** and no ADR to explain one. The matcher is reused WHOLE — this
phase writes no matching logic of its own, which is the single largest reason its
size estimate sits below its closest comparators (§7).

---

## 3. Why this phase, and what it does not claim

**It does not claim novelty of mechanism.** The decode-side short-circuit is the
same shape as `rbac`, `fault`, `csrf` and `cdn_loop`. What is new is the seam in
§4 item 4 and the surface in §2.

**It does not claim to complete the mission.** At this pick all **122**
`ROADMAP.md` rows are `done` — stop-condition leg (i) TRUE for the **sixth** time
— while legs (ii) and (iii) are FALSE. `ROADMAP.md:58` and `ADR-0167` DECISION 2
govern: an all-`done` census measures the rows that EXIST, not the surface that
remains. **No `stop` file exists and none was created.**

**It does not claim to be the only good pick.** `ADR-0200` records the rejected
alternatives with the measurement each rests on.

---

## 4. In scope — the deliverables

1. **`envoy-config`: the `HealthCheckFilterConfig` schema.** `pass_through_mode`
   (required `bool` — absent is boot-fatal, matching §2.1 row 2),
   `headers: Vec<HeaderMatcher>` (the landed seven-mode matcher, reused whole),
   and `cache_time` / `cluster_min_healthy_percentages` RECOGNIZED-then-rejected
   so the narrowing surfaces as a precise `ConfigError`, not an opaque serde
   unknown-field error. This is the established recognize-then-reject style of
   `HashPolicy` and the cluster `HashFunction`.
2. **Validators, all boot-fatal:** `pass_through_mode` absent;
   `pass_through_mode: true`; `cache_time` present at all (upstream rejects it
   when `pass_through_mode` is `false`, and this phase implements only `false`);
   `cluster_min_healthy_percentages` present.
3. **`envoy-filter`: `health_check.rs` + the THIRTEENTH production
   `HttpFilterInstance` variant.** Decode-side only. AND over the `headers` matchers against the
   request, `:path` carrying its query string verbatim; on match
   `Decision::StopAndSend` with status 200, empty body and the single
   empty-valued `x-envoy-upstream-healthchecked-cluster` response header; on
   non-match `Decision::Continue`. An empty or absent matcher list matches
   everything (§2.3 cell 7).
4. **The `FilterResponse` response-code-details seam.** `FilterResponse`
   (`crates/envoy-filter/src/types.rs:50`) today carries `status`, `reason`,
   `headers`, `body` and **no details field**, and **no landed filter sets
   `%RESPONSE_CODE_DETAILS%` at all**. Carrying `health_check_ok` to the access
   log therefore needs a new field threaded to the H1 and H2
   `response_code_details_for_log` sites. ⚠ **This is a cross-crate change**:
   **28** literal `FilterResponse { … }` construction sites across **12** files
   in **three** crates (`envoy-filter` 19, `envoy-http1` 5, `envoy-http2` 4)
   will raise `E0063` at once. See §8 PV-3.
5. **Fixture `0095-http-filter-health-check`**, driven by the EXISTING
   `Driver::Http1ProbeList` — backend-free and cluster-free, with a
   `direct_response` catch-all as the fall-through witness. Probes are §2.3 cells
   1-6 plus the two-matcher AND cell.
6. **A `BEHAVIOR_CONTRACT.md` section** recording §2.1-§2.4 as the contract, and
   recording §2.5 as a measured behaviour envoy-rust does NOT match.

---

## 5. Non-goals — rejected fail-loud, each with its reason

1. **`pass_through_mode: true`** (and therefore `cache_time`). Forwarding the
   probe upstream and caching the verdict is a second, timing-dependent design
   whose witness would need a backend and a clock. Boot-fatal.
2. **`cluster_min_healthy_percentages`.** Needs cluster health state and an
   active health checker to move it; the witness would need a backend and would
   be non-deterministic at the boundary. Boot-fatal. Banked as **CF-115-1**.
3. **The admin `/healthcheck/fail` → 503 leg (§2.5).** Not rejected on merit — it
   is real, measured and desirable. It is out because **no landed driver can
   witness it**: `Driver::AdminScrape` has `pre_admin_actions` but its `scrapes`
   target the ADMIN listener, and its `pre_requests` grammar carries no
   `expected_status`/`expected_body`, so "probe the data plane, POST to admin,
   probe the data plane again and assert the 503" is not expressible. Admitting
   it means driver work, which is the cost that sank several `ADR-0196`
   candidates. Banked as **CF-115-2**.
4. **The `not_health_check_filter` access-log arm.** This phase UNBLOCKS it; it
   does not build it. Shipping both would merge two phases. Banked as
   **CF-115-3**, and it is the natural successor phase.
5. **Any HTTP/2 differential witness.** H1 only, consistent with every prior
   single-filter phase. Banked as **CF-115-4**.
6. **Reproducing upstream's `path_through_mode` typo** (§2.1). Behaviour is the
   contract; error text is not (§7.4).

---

## 6. The differential witness — fixture `0095-http-filter-health-check`

`Driver::Http1ProbeList`, one HCM listener, no cluster and no backend. The
fall-through route is a `direct_response` returning a fixed marker body, so every
probe's disposition is legible from the response alone: **length 0 with no
`content-type` = intercepted; the marker body = fell through**.

Probes: `/healthz` (intercepted), `/healthz?x=1` (falls through — the §2.3 cell-2
trap), `/healthz/`, `/healthZ`, `/other` (all fall through), `POST /healthz`
(intercepted — method-agnostic), and the two-matcher AND pair.

**Why it is non-vacuous.** Both dispositions return HTTP 200, so status alone
cannot pass this fixture: only the body and the header set discriminate. A filter
that never fires fails cells 1 and 6; a filter that always fires fails cells 2-5.
The §5.2/§7.4 discipline applies — state 3 must prove the fixture red under a
deliberate mutation, and `ADR-0194`'s rule stands that a mutation which touches
both the implementation and its test manufactures a false green.

---

## 7. Size estimate and the §6.1 split gate

**The §6.1 split is PROJECTED LIKELY. `ADR-0201` is RESERVED-UNFIRED for it.**

Calibration MEASURED at this pick as net `crates/` + `tests/` lines over each
phase's full landed arc — not inherited from any document:

| comparator | net LoC | shape |
|---|---|---|
| phase 24 `csrf` | **1900** | a new HTTP filter + one fixture |
| phase 31 `cdn_loop` | **1886** | a new HTTP filter + one fixture |
| phase 113 | 1165 | an access-log operator family |
| phase 114 | 1038 | an access-log filter arm |

**Both closest-shaped comparators land ≈1890, well over the ~1500 gate**, and
they do so independently. That is the warning.

The offsetting argument, and it is a real one: both of those phases wrote their
own matching logic — `cdn_loop` alone is a 934-line module built around an
RFC 8586 parser — whereas this phase reuses the landed seven-mode `HeaderMatcher`
whole (§2.6) and its runtime rule is a conjunction over that matcher plus one
static reply. The bespoke code is the config schema, the validators, the
`FilterResponse` seam and the tests.

Bottom-up central estimate **≈1150-1300** net. Against the calibration this
project has recorded — PROJECTED estimates running 1.33× and 1.66× UNDER, while
estimates MEASURED on a prototype landed 1.00×/1.07×/1.10× — the projected band
is **1150-2160** and crosses the gate.

⚠ **State 2 must MEASURE, not project**, on a scratch worktree with its own
`CARGO_TARGET_DIR`, and must re-measure **after the final edit to the plan** — a
measured estimate goes stale the moment the plan changes. Per `ADR-0194` a
whole-slice prototype validates the SLICE and never a TASK BOUNDARY.

**If the gate fires, the declared cut** follows the `110.1`/`110.2` and
`112.1`/`112.2` precedent:

- **`115.1`** — config schema, validators, `health_check.rs`, the thirteenth
  production `HttpFilterInstance` variant and the `FilterResponse` details seam, witnessed
  ENTIRELY in-process with no new fixture.
- **`115.2`** — fixture `0095`, the `BEHAVIOR_CONTRACT.md` section, and the
  parent close.

---

## 8. PLAN-VERIFY items — what state 2 must measure

Each is load-bearing: getting it wrong changes the design, not just a number.

- **PV-1 — the `:path` query-string rule end to end.** §2.3 cell 2 is measured on
  upstream only. Confirm envoy-rust's HCM presents `:path` to the matcher with
  the query string attached; if it strips it anywhere, cell 2 inverts and the
  fixture asserts the wrong thing.
- **PV-2 — the empty-matcher-list semantic.** §2.3 cell 7 says absent/`[]`
  matches everything. Confirm the landed `HeaderMatcher` AND-fold over an empty
  slice yields `true` and not `false`; an empty-conjunction convention that went
  the other way would invert the whole cell.
- **PV-3 — the `FilterResponse` blast radius.** Re-derive the **28** sites at the
  implementing commit rather than trusting this number, and sweep `E0063` in a
  **fixpoint loop**: cargo stops at the first failing crate, so round 0 is not
  the blast radius. A `-p <crate>` run will look green while the workspace is
  broken.
- **PV-4 — the `x-envoy-upstream-healthchecked-cluster` value.** Measured EMPTY
  with no cluster configured (§2.2). Re-measure with a cluster present. If it
  becomes non-empty, the response-header rule is conditional and the fixture must
  say which cell it is asserting.
- **PV-5 — whether §2.5's leg is genuinely inexpressible.** §5 item 3 rests on a
  driver-grammar reading. Re-read `Driver::AdminScrape` at the implementing
  commit. If it turns out expressible without driver work, that is a scope
  question for state 2 to raise in its own ADR — not something to smuggle in.
- **PV-6 — filter ORDER in the chain.** Every measurement here put `health_check`
  first. Confirm the landed pipeline runs decode-side filters in configured
  order, and record what a health_check placed AFTER another short-circuiting
  filter does.
- **PV-7 — the reject-direction cells against envoy-rust.** §2.1 is upstream-only.
  Each boot-fatal rejection in §4 item 2 needs its own envoy-rust assertion;
  `envoy-bin` has no `--mode validate`, so prove it with a throwaway test that
  parses the YAML.

---

## 9. Close-out clause

The phase is done when §7.5's six gates are met: fixture `0095` green, the other
**94** still green, the CI-authoritative conformance suites passing, any new fuzz
target clean, all five `cargo` legs clean, and `REVIEW.md` approved. The
`ROADMAP.md` row `115` status cell flips `planned` → `done` at the state-6
close-out and nothing else in that row is touched.

---

## 10. Carry-forwards opened by this phase

| id | what | why it is banked |
|---|---|---|
| **CF-115-1** | `cluster_min_healthy_percentages` | needs cluster health state and a backend (§5 item 2) |
| **CF-115-2** | the admin `/healthcheck/fail` → 503 leg, incl. `x-envoy-immediate-health-check-fail` on unrelated responses | no landed driver can witness it (§2.5, §5 item 3) |
| **CF-115-3** | the `not_health_check_filter` access-log arm | unblocked by this phase; the natural successor (§5 item 4) |
| **CF-115-4** | the H2 witness | H1 only, per precedent (§5 item 5) |
| **CF-115-5** | `pass_through_mode: true` + `cache_time` | a second, timing-dependent design (§5 item 1) |

Every carry-forward banked by earlier phases stands INTACT. This phase consumes
none and fixes none — a pick banks, it does not repair (§6.3; `ADR-0165`).
