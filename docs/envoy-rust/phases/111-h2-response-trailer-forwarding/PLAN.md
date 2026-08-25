# Phase 111 — HTTP/2 response TRAILER forwarding (upstream → downstream) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD per `superpowers:test-driven-development` — the failing test is written and RUN-TO-FAIL before the implementation.

**Goal:** Forward the HTTP/2 response trailer block arriving from an HTTP/2 upstream to the HTTP/2 downstream client, discharging the first of the gRPC family's two blocking prerequisites, witnessed by new differential fixture `0090-h2-response-trailers`.

**Architecture:** The trailers are read from `h2::RecvStream::trailers()` at the upstream client immediately after the existing body drain, carried as an `Option<Vec<(String, String)>>` **alongside** the shared `envoy_http1::Response` (never as a field on it), threaded through `crates/envoy-http2/src/hcm.rs` to the single downstream emit seam `send_envoy_response`, where the end-of-stream fork becomes three-way so a trailer HEADERS frame can follow the DATA frame. The differential harness grows a trailer-observing `drive_http2`, a trailer expectation rule reusing the existing `diff_headers`, and a `--trailers` mode on the existing H2 backend helper.

**Tech Stack:** Rust (pinned toolchain per `rust-toolchain.toml`), `h2 0.4.16` (already a direct dependency of `envoy-http2`; provides both `RecvStream::trailers` and `SendStream::send_trailers`), `http` (`HeaderMap`), `tokio`, `bytes`, `testcontainers` (differential harness). **ZERO new dependencies. ZERO new config surface.**

**Spec:** `docs/envoy-rust/phases/111-h2-response-trailer-forwarding/SPEC.md` (read it alongside this plan — this plan argues from it and **corrects it in three places**, see §2).

---

## Global Constraints

Every task's requirements implicitly include this section.

- **Upstream reference pin:** `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2` (`docs/envoy-rust/ENVOY_TARGET.md`). Never changed here.
- **`#![forbid(unsafe_code)]`** at every crate root (doctrine D-3.8). No `unsafe` in this phase.
- **No new dependency** may be added to any `Cargo.toml`, and `Cargo.lock` must not change.
- **No new config surface.** Upstream Envoy forwards trailers on a stock config with no knob (§1, F1 re-confirmed as PV-1). envoy-rust adds no field, so no `deny_unknown_fields` validator work and no boot-fatal reject matrix.
- **`HEADER_ALLOW_LIST` stays at exactly 3 entries** — `server`, `date`, `x-envoy-upstream-service-time` (`tests/differential/src/lib.rs:1189-1193`, re-verified at this PLAN-write). `location` and `content-type` are ABSENT and stay absent.
- **Do not touch `crates/envoy-filter/`.** The headers-only filter API is the gRPC family's SECOND prerequisite and its own phase.
- **Do not touch `crates/envoy-http1/`** except where a compile error forces it (this plan's design produces none — see §4 D-PLAN-2).
- **Do not touch the io_uring H1 path** (`crates/envoy-http1/src/uring.rs`).
- **Never trim** `tests/conformance/h2spec/known-failures.txt`.
- **Landed work is uneditable:** phases 00–74, 75.x, 76.x, and the entire `108`/`109`/`110` families, `REVIEW.md`s included.
- **Do not fix any banked finding** (§6.3; ADR-0165). The `110.1`/`110.2` REVIEW Minors, CF-110-1…9 and the whole standing CF ledger stay OPEN.
- **Do not repair** `ROADMAP.md`'s mis-filed `Observability family:` rows or its two unescaped-pipe rows (38, 39).
- **Locate every code site by TEXT (`grep`), never by an inherited line number.** Every `file:line` in this plan was resolved at HEAD `82e2e75a503f47272af3a362ad4c54486f96d13c`; they drift.

---

## §0. State at this PLAN-write

- HEAD `82e2e75a503f47272af3a362ad4c54486f96d13c`, branch `main`, `git status --porcelain` empty, `origin/main` in sync (`git fetch origin --prune` run bare, exit 0).
- Phase directory held `SPEC.md` ONLY; this plan is the §5 state-2 output.
- Fixture census **89** (`ls -d tests/fixtures/*/ | wc -l`), differential test files **89**, `grep -rin trailer tests/ | wc -l` = **7** (all incidental). All three re-derived here, all three CONFIRM the SPEC.

---

## §1. PV-1 … PV-8 — the empirical reconciliation (§6.2), run FIRST

All measurements below were taken by the main session at this PLAN-write against the pinned image, on a purpose-built raw-HTTP/2 probe rig (backend + client written directly on `http2::Framer` + `hpack`, so **no library-side validation or filtering** could mask a result). The backend ran as a **sibling container** on a dedicated Docker network (upstream Envoy reaches it by container name; envoy-rust reaches the same container through its published port — so **both proxies saw the identical backend**). The rig lived only in the session scratchpad; `git status --porcelain` was empty before and after.

**Negative control (the probe is not vacuous):** upstream Envoy's admin `/stats` reported `cluster.backend.upstream_rq_total: 18`, `upstream_rq_200: 17`, `upstream_rq_500: 1`, `http.ingress_http2.downstream_rq_total: 18` — exactly the 18 requests driven through it. The probe genuinely traversed the proxy.

### PV-1 — the §1.1 divergence, RE-RUN end-to-end: **CONFIRMED**

| cell | upstream Envoy v1.33.0 | envoy-rust @ `82e2e75` |
|---|---|---|
| status | `200` | `200` |
| `content-type` | `text/plain` | `text/plain` |
| `trailer` announce header | `x-trail-a` forwarded | `x-trail-a` forwarded |
| body | `BODY-OK` | `BODY-OK` |
| `server` / `date` / `x-envoy-upstream-service-time` | present | present (all three allow-listed) |
| **response trailers** | **`x-trail-a=alpha`, `x-trail-b=beta`** | **NONE** |

The announced trailer AND the unannounced one both come through upstream. F1/F2/F3/F4 all hold. The divergence is exactly one cell wide.

### PV-2 — dry-run of the EXACT `0090` YAML pair against BOTH proxies

The candidate pair was materialised by substituting the template tokens into `tests/fixtures/0010-http2-router-upstream/envoy.yaml` and `envoy-rust.yaml` verbatim. **Both proxies booted clean** — no boot-fatal, no unknown-field reject, no config-shape divergence. The `0010` topology transfers to `0090` without modification.

**But the dry-run surfaced THREE divergences the landed `SPEC.md` does not contain** — the same class of find that CF-110-6/7/8 were at the phase-110 PLAN-write, and each would have landed fixture `0090` RED for a reason unrelated to its subject. They are §3's CF-111-5, CF-111-6 and CF-111-8 and they are why §4 D-PLAN-6 shrinks the fixture's probe set.

### PV-3 — the four edge cells, plus nine more. **MEASURED, not reasoned.**

Every row below is a real request through both proxies. "after fix" is what the design in §4 predicts.

| # | probe | upstream Envoy v1.33.0 | envoy-rust today | in fixture `0090`? |
|---|---|---|---|---|
| 1 | announced + unannounced trailer | both forwarded | none | **YES — the subject** |
| 2 | **trailers on an EMPTY body** | both forwarded | none | no (unit-tested; see D-PLAN-3) |
| 3 | announces `trailer:` then sends NONE | 0 trailers | 0 trailers | no — already at parity |
| 4 | trailer name duplicating a response-header name | header and trailer BOTH kept, separate | header only | no |
| 5 | same trailer name twice (`x-multi: one`, `x-multi: two`) | both forwarded, wire order kept | none | **no — CF-111-8** |
| 6 | five trailers | all five, order kept | none | no |
| 7 | **non-200 (`500`) response with trailers** | forwarded | none | no (SPEC §8 had this UNMEASURED — now measured) |
| 8 | empty trailer HEADERS block (zero fields) | 0 trailers, clean END_STREAM | 0 trailers, clean | no |
| 9 | trailer value with space + comma (`a b c, d`) | forwarded verbatim | none | no |
| 10 | `content-length` in the trailer block | **FORWARDED verbatim** | none | no |
| 11 | `te: trailers` in the trailer block | **FORWARDED verbatim** | none | no |
| 12 | `host` in the trailer block | **FORWARDED verbatim** | none | no |
| 13 | `connection` / `transfer-encoding` / `upgrade` / `keep-alive` / `proxy-connection` / `te: gzip` (each probed **separately**) | block DROPPED, `200` + full body + `RST_STREAM(NO_ERROR)` | **`503`, empty body** | **no — CF-111-5** |
| 14 | `:status` pseudo-header in the trailer block | block DROPPED, `200` + body + `RST_STREAM(NO_ERROR)` | `200` + body, no error, no trailers | **no — CF-111-6** |

**PV-3(d) is answered, and the answer is not "Envoy strips some names".** Envoy forwards `content-length`, `te: trailers` and `host` *verbatim*. What it rejects is the **h2 connection-specific set** and **pseudo-headers**, and it rejects them by dropping the WHOLE block and resetting the stream — not by stripping the offending name. Upstream's own accounting agrees: `cluster.backend.http2.rx_messaging_error: 4` and `http2.tx_reset: 4`, exactly the four reset probes.

**Row 13 root-caused.** envoy-rust's `503` has nothing to do with trailer forwarding. `h2`'s **receive-side** validation errors the DATA stream, which the existing drain loop in `crates/envoy-http2/src/client.rs` maps to `Http2Error::H2RecvBody`; the log line is verbatim:

```
WARN H2 listener: upstream dispatch failed — emitting 503
     error=client-side H2 response body read failed: stream error detected: unspecific protocol error detected
```

This happens **today, with no trailer code in the tree**, entirely inside the `h2` codec. It is pre-existing, out of scope, and banked as CF-111-5.

### PV-4 — trailer comparison semantics. **DECIDED: reuse `diff_headers` unchanged.**

`diff_headers` (`tests/differential/src/lib.rs:1204`, read in full at this PLAN-write) builds a `BTreeSet<String>` of ASCII-lowercased names and compares those sets; for each name **not** on the allow-list it compares only the **first** occurrence's value (`.iter().find(...)`). Duplicate names therefore collapse in the set and a second value is invisible.

Measured consequence (PV-3 row 5): Envoy forwards duplicate trailer names and preserves both. A set comparison cannot assert that multiplicity.

**Decision:** reuse `diff_headers` verbatim. It is exactly what `docs/envoy-rust/BEHAVIOR_CONTRACT.md:18` mandates — *"Response trailers | Set-equal under the same allow-list discipline"* — and it costs ZERO new comparison code. Writing a multiset `diff_trailers` would diverge from the contract row's own wording and add code no fixture can justify. The cost is that duplicate-name multiplicity is unassertable, which is why PV-3 row 5 is **excluded** from fixture `0090` and banked as **CF-111-8**.

### PV-5 — every end-of-stream site in `crates/envoy-http2/`. **CENSUS COMPLETE: 4 production sites, 2 of them downstream.**

| `file:line` | fn | call | `end_of_stream` | class |
|---|---|---|---|---|
| `crates/envoy-http2/src/response.rs:87` | `send_envoy_response` | `SendResponse::send_response` | `resp.body.is_empty()` | **PROD — downstream emit** |
| `crates/envoy-http2/src/response.rs:91` | `send_envoy_response` | `SendStream::send_data` | `true` | **PROD — downstream emit** |
| `crates/envoy-http2/src/client.rs:179` | `ClientStream::send_request` | `SendStream::send_data` | `true` | PROD — upstream REQUEST direction (never carries downstream response trailers) |
| `crates/envoy-http2/src/grpc.rs:195` | `grpc_health_check_call` | `SendStream::send_data` | `true` | PROD — upstream REQUEST direction (health-check client, not the relay path) |

Everything else is `#[cfg(test)]`. There are **ZERO production `send_trailers` and ZERO production `send_reset` calls** in the crate.

**Nothing bypasses the seam.** `send_envoy_response` has exactly one production caller — `crates/envoy-http2/src/hcm.rs:1043`, inside `finalize_h2_stream`, which itself has exactly one caller, `crates/envoy-http2/src/hcm.rs:930`, inside `handle_one_stream`. Local replies, `synth_h2_502`, `synth_h2_overflow`, `SynthFromDecode` and the encode-filter `StopAndSend` short-circuit **all funnel through it**. A change there misses nothing. (The one path that skips it is dropping `send_response` on an early `Err`, which h2 turns into an implicit RST — no trailers can apply.)

**The h2 ordering rule is enforced, not advisory.** `h2 0.4.16` `src/proto/streams/send.rs` returns `UserError::UnexpectedFrameType` once END_STREAM has been sent, so `send_data(.., true)` followed by `send_trailers` is an error. The empty-body branch `send_response(head, true)` has the same problem. **The fork must become three-way** (§4 D-PLAN-3).

**A trailers-only response (no DATA at all) is legal and is already proven in-tree** — `crates/envoy-http2/src/grpc.rs:436-439` builds `send_response(resp, false)` followed directly by `send_trailers(..)` in a passing test.

### PV-6 — no pre-existing fixture regresses

The no-trailers path must stay byte-identical. Measured today at parity on both proxies for a normal response (`/no-trailers`) and for an **empty-body** response (`/empty-no-trailers`). Both are the shape every one of the 89 existing fixtures takes. The regression witness is structural: **fixture `0010-http2-router-upstream` is itself the no-trailers H2-in/H2-out case** and stays untouched, so gate (b) over all 89 fixtures *is* the regression test. Task 1's unit tests additionally pin both no-trailer branches directly.

### PV-7 — the E0063 blast, RE-DERIVED. **CORRECTED: 42, not 82.**

```
$ git grep -nE 'Response \{' -- '*.rs' | wc -l                                     # naive
181
$ git grep -nE '(^|[^A-Za-z0-9_])Response[[:space:]]*\{' -- '*.rs' | wc -l          # anchored
81
$ git grep -nE '(^|[^A-Za-z0-9_])Response[[:space:]]*\{' -- '*.rs' \
    | grep -cE '(->|struct |impl |enum )'                                          # signatures, NOT initializers
39
$ git grep -nE '(^|[^A-Za-z0-9_])Response[[:space:]]*\{' -- '*.rs' \
    | grep -vE '(->|struct |impl |enum )' | wc -l                                  # true struct literals
42
```

The naive form over-counts by matching `DirectResponse {` (69), `FilterResponse {` (27), `HttpResponse {` (2), `Http1Response {` (1), `HealthCheckResponse {` (1). The **anchored** form still over-counts, because 38 of its hits are `-> Response {` / `-> envoy_http1::Response {` function signatures (23 of them in `crates/envoy-admin/src/endpoint.rs` alone) plus the 1 `pub struct Response {` declaration — none of which can raise E0063.

**True count: 42 struct literals**, in 4 crates: `envoy-http1` 22, `envoy-http2` 12, `envoy-admin` 7, `tests/helpers/http1-echo-server` 1. **Zero** use struct-update (`..`) syntax, and `Response` has no `Default` impl, so all 42 are hard E0063 sites.

**D3 (trailers ride alongside, never as a field) is UPHELD on the corrected number**, and for a second reason the SPEC does not give: `Response` carries `#[derive(Debug, Clone, PartialEq, Eq)]` (`crates/envoy-http1/src/response.rs`), so a fifth field silently changes the meaning of every whole-`Response` equality assertion in the suite. 42 sites across four crates — two of which (`envoy-admin`, a test helper) have nothing to do with HTTP/2 — versus ~106 net LoC contained inside one crate. See §2 for the ADR.

### PV-8 — re-pricing against landed-phase calibration. **CONFIRMED: median 1.32, worst 1.75.**

Re-derived across the last ten landed phases (73, 74, 76.1, 76.2, 108.1, 108.2, 109.1, 109.2, 110.1, 110.2), estimate = each PLAN's own bottom-up total, actual = `git diff --numstat <state-2 commit> <close-out> -- . ':(exclude)docs/'` summed as additions − deletions.

| phase | est. | tasks | actual | ratio |
|---|---:|---:|---:|---:|
| 73 | ~670 | 8 | 873 | 1.30 |
| 74 | ~1135 | 10 | 1981 | **1.75** |
| 76.1 | ≈515 | 8 | 774 | 1.50 |
| 76.2 | ≈1312 | 12 | 1568 | 1.20 |
| 108.1 | ≈1215 | 9 | 1128 | 0.93 |
| 108.2 | ≈905 | 6 | 854 | 0.94 |
| 109.1 | ≈1180 | 7 | 1726 | 1.46 |
| 109.2 | ≈745 | 5 | 562 | 0.75 |
| 110.1 | ≈912 | 8 | 1290 | 1.41 |
| 110.2 | ≈615 | 7 | 817 | 1.33 |

Median **1.32**, max **1.75**, **7 of 10 overran**, actuals **562–1981**, tasks **5–12**. Every sub-claim of the inherited calibration CONFIRMED. Three actuals were independently re-measured by the main session and matched exactly: `110.2` → `817`, `110.1` → `1290`, `109.1` → `1726`. Phase 74's 1.75 is inflated by two §5.2 code-review re-entries (at its state-3 head it reads 1607 / 1.42), and that is real, unavoidable phase cost a planner must price.

---

## §2. Corrections this PLAN makes to the landed `SPEC.md` / `ADR-0181` → **ADR-0182 FIRES**

`ADR-0182` was RESERVED-UNFIRED for a §6.1 split. **The split does NOT fire (§6).** Per the state-2 handoff, the reserved number is therefore used for the empirical reconciliation, which is material:

1. **The E0063 blast is 42 sites, not 82.** `SPEC.md` §1.2 F7, `SPEC.md` §3 D3 and `ADR-0181` DECISION 4 all cite **82** ("39 in `envoy-http1`, 17 in `envoy-http2`"). That figure correctly stripped the sibling-type contamination that produced the earlier 182, but did **not** strip the 39 `-> Response {` / `struct Response {` signature lines. The true per-crate split is `envoy-http1` 22 / `envoy-http2` 12 / `envoy-admin` 7 / helper 1. **The design conclusion is unchanged and is now additionally supported by the `PartialEq`/`Eq` derive argument.**
2. **Three new divergences, measured at this PLAN-write** (CF-111-5, CF-111-6, CF-111-8 — §3). None is in `SPEC.md`. Two of them (5, 6) would have landed fixture `0090` RED for reasons unrelated to trailer forwarding had the fixture probed those cells; the third makes a probe unassertable.
3. **Two `SPEC.md` §8 "NOT MEASURED" cells are now MEASURED:** trailer behaviour on a **non-200** proxied response (Envoy forwards; PV-3 row 7), and **whether Envoy strips or rewrites any trailer name** (it does not — it forwards `content-length`, `te: trailers` and `host` verbatim, and rejects the connection-specific/pseudo set by dropping the whole block and resetting; PV-3 rows 10–14). A third is newly measured beyond §8's list: **Envoy has trailer stats and they do not tick** (CF-111-7).

The ADR is written at the same commit as this plan.

---

## §3. Carry-forwards this PLAN OPENS (in addition to the SPEC's CF-111-1…4)

- **CF-111-5** — an upstream H2 response whose trailer block contains any h2 connection-specific name (`connection`, `transfer-encoding`, `upgrade`, `keep-alive`, `proxy-connection`, or `te` with a value other than `trailers`) makes envoy-rust return **`503` with an empty body**, where upstream Envoy returns **`200` + the full body + `RST_STREAM(NO_ERROR)`**. Root cause is `h2`'s receive-side validation inside the existing body-drain loop (`crates/envoy-http2/src/client.rs`), reached before any trailer code; **pre-existing and independent of this phase**. Excluded from fixture `0090`.
- **CF-111-6** — a trailer block containing a pseudo-header (`:status`, …): Envoy drops the whole block and resets; envoy-rust's `h2` accepts the stream and (today) ignores trailers entirely. **After this phase lands, envoy-rust would forward the block's surviving non-pseudo fields while Envoy forwards nothing** — a divergence this phase would CREATE. Excluded from fixture `0090`; closing it needs a decision on whether to mirror Envoy's drop-and-reset, which is a behaviour change beyond forwarding.
- **CF-111-7** — upstream Envoy exposes `http2.trailers` and `cluster.<name>.http2.trailers` stats and **both stayed `0`** across eight trailer-forwarding responses. No stat parity to chase and no stat may be asserted by fixture `0090`; whether any Envoy stat ever ticks for trailers is unresolved.
- **CF-111-8** — duplicate trailer NAMES (`x-multi: one`, `x-multi: two`) are forwarded and preserved by Envoy, but the harness's `diff_headers` collapses names into a set and compares only the first value, so multiplicity is **unassertable by any fixture**. Excluded from fixture `0090`. Closing it needs multiset comparison semantics, which would diverge from `BEHAVIOR_CONTRACT.md:18`'s "Set-equal" wording and so needs its own ADR.
- **CF-111-9** — trailer **wire ORDER** is preserved by Envoy (measured across a five-trailer block) but is doubly invisible: `HeaderMap` iteration order is not insertion order, and the harness compares sets. Unassertable, same blindness `110.2`'s contract §D records for header order.

**Carried forward UNCONSUMED** (§6.3; ADR-0165 — a phase banks, it never clears): the SPEC's CF-111-1…4; the `110.2` REVIEW's M-1…M-8 + N-1…N-12; the `110.1` REVIEW's M-1…M-9 + N-1…N-10; CF-110-1…CF-110-9; CF-109-1/2/3; CF-108-1/2/3; CF-76-1; CF-75-2/3/4/5/6; CF-72-2/CF-75-1; M71-6; CF-74-1/2/3/4/6; CF-73-1; the `109.2`, `109.1` and `108.2` REVIEW sets; and the HTTP-filters-family (1)–(4).

---

## §4. Design decisions

**D-PLAN-1 — Task order is EMIT → READ → THREAD, not read → thread → emit.**
The SPEC describes the data flow read-first, but that order gives Tasks 1 and 2 no observable behaviour and therefore no honest TDD RED (a threading-only task can only assert that a value moved). Emitting first means every task has a real failing test: Task 1 tests the emit fork directly against an in-process h2 client; Task 2 tests the read against an in-process h2 server; Task 3 lights the two ends up end-to-end. This is the only deviation from the SPEC's narrative order and it changes no design.

**D-PLAN-2 — Trailers ride ALONGSIDE `envoy_http1::Response` as `Option<Vec<(String, String)>>`.**
SPEC D3, upheld on the corrected 42-site E0063 measurement (§1 PV-7) plus the `PartialEq`/`Eq` derive hazard. `None` means "no trailer block" (the overwhelmingly common case); `Some(vec![])` is not produced. `Vec<(String, String)>` rather than `http::HeaderMap` so it matches the shape `Response.headers` already uses and preserves duplicates and wire order for free. **Do not undo this for tidiness.**
The in-tree precedent is phase 110.1's `outgoing_local: bool` sidecar in `crates/envoy-http1/src/hcm.rs`: a per-request local threaded beside the response and consumed at exactly one emit funnel. Its recorded rationale applies verbatim — installing the behaviour *inside* the funnel rather than at its callers makes the coverage structural, so a local-reply site added later cannot forget it.

**D-PLAN-3 — The emit fork becomes three-way.** Measured requirement, not a style choice (§1 PV-5):

| body | trailers | frames |
|---|---|---|
| empty | none | `send_response(head, end_of_stream = true)` — **unchanged from today** |
| empty | present | `send_response(head, false)` then `send_trailers(map)`, **no DATA frame** |
| non-empty | none | `send_response(head, false)` then `send_data(body, true)` — **unchanged from today** |
| non-empty | present | `send_response(head, false)` then `send_data(body, false)` then `send_trailers(map)` |

The empty-body-with-trailers row is **not a corner case — it is the gRPC main case** (a gRPC trailers-only response has an empty body by construction), which is the whole point of this prerequisite. It is legal and proven in-tree at `crates/envoy-http2/src/grpc.rs:436-439`.

**D-PLAN-4 — NO defensive strip on the trailer emit path.** The instinct is to mirror `H2_FORBIDDEN_HOP_BY_HOP` (`crates/envoy-http2/src/lib.rs`), which `build_http_response` applies to the header block. **Measurement says that code would be unreachable.** `h2`'s send-side `check_headers` rejects exactly `connection`, `transfer-encoding`, `upgrade`, `keep-alive`, `proxy-connection`, and `te` ≠ `trailers` — and PV-3 row 13 measured that a block containing ANY of those six is rejected by `h2` on the **receive** side first, so it never reaches the emit seam (envoy-rust returns 503 before that point). A strip would be untestable dead code and §6.3 forbids exactly that. **Record the reasoning in a doc comment at the emit site so a reviewer does not re-raise it**, and bank the asymmetry as CF-111-5.

**D-PLAN-5 — Trailers are CLEARED on every locally-generated response.** SPEC non-goal 6 says trailers on locally-generated replies are out of scope, and upstream's behaviour there is unmeasured. Under D-PLAN-2 the trailers are a *separate* local, so a `Decision::StopAndSend` replacement or a synth response would otherwise **inherit the upstream's trailers** — the reverse of the field-on-`Response` hazard, and just as wrong. The `StopAndSend` arm in `finalize_h2_stream` already rebuilds `resp` as a fresh literal (`crates/envoy-http2/src/hcm.rs`, the `envoy_filter::Decision::StopAndSend(replacement)` arm); the trailers local must be set to `None` alongside it, and Task 4 pins that with its own test.

**D-PLAN-6 — Fixture `0090` witnesses ONE probe: an announced trailer AND an unannounced trailer on a non-empty-body `200`.** That is the whole measured divergence (PV-1) and the cheapest shape that closes it. Deliberately EXCLUDED, each on a measurement:
- forbidden-name trailers → envoy-rust 503s today (CF-111-5) — the fixture would be RED for an unrelated reason;
- pseudo-header trailers → this phase would CREATE the divergence (CF-111-6);
- duplicate trailer names → unassertable under `diff_headers` (CF-111-8);
- empty-body-with-trailers, non-200-with-trailers, five-trailer order → real and measured, but each needs a *second* backend mode or a second fixture; they are covered by unit tests at the emit seam instead, which is where the logic lives;
- any stat assertion → CF-111-7.
**The no-trailers regression witness is fixture `0010-http2-router-upstream`, untouched**, plus the other 88 fixtures under gate (b).

**D-PLAN-7 — The trailer backend is a new `--trailers` mode on the EXISTING helper**, following the `--close-before-response` precedent (`tests/helpers/http2-echo-server/src/main.rs`), and it keeps the existing deterministic **echo** body shape. Consequently fixture `0090` must copy fixture `0010`'s two body-stabilising suppressions **verbatim** — `generate_request_id: false` and the six-entry `request_headers_to_remove` list on the upstream side only — or the echoed body diverges between proxies for reasons unrelated to trailers.

**D-PLAN-8 — Trailer send failure gets its own error variant, `H2SendTrailers`.** Reusing `H2BodyRead` would compound a misnomer the code already flags. Adding a variant is safe here — checked at this PLAN-write: `crates/envoy-http2/` and `crates/envoy-bin/` contain **no** `unreachable!()` or `unimplemented!()`, and the only `match` sites on `Http2Error` outside `error.rs` are non-exhaustive `Err(..)` patterns in tests. Task 1 re-runs that grep before adding the variant.

---

## §5. File structure

| file | disposition | responsibility after this phase |
|---|---|---|
| `crates/envoy-http2/src/response.rs` | modify | Downstream emit seam; owns the three-way end-of-stream fork and the `Vec<(String,String)>` → `HeaderMap` conversion |
| `crates/envoy-http2/src/client.rs` | modify | Upstream read; owns `recv_stream.trailers()` and the `HeaderMap` → `Vec<(String,String)>` conversion |
| `crates/envoy-http2/src/hcm.rs` | modify | Threads the trailers local from the upstream attempt to the emit seam; clears it on every local-reply path |
| `crates/envoy-http2/src/error.rs` | modify | New `H2SendTrailers` variant |
| `tests/helpers/http2-echo-server/src/main.rs` | modify | New `--trailers` mode |
| `tests/differential/src/backend.rs` | modify | New `Http2TrailersBackend` |
| `tests/differential/src/lib.rs` | modify | `DriveHttp1Result.trailers`; `drive_http2` reads them; `Driver::Http2.expected_trailers`; `Http1TrailerRule`; `{{HTTP2_TRAILERS_BACKEND_PORT}}` |
| `tests/fixtures/0090-h2-response-trailers/` | **create** | The differential witness (4 files) |
| `tests/differential/tests/h2_response_trailers.rs` | **create** | The 19-line auto-discovered runner |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | modify | New `## Response trailers` section |

**No `Cargo.toml`, no `Cargo.lock`, no `ci.yml`, no `deny.toml` change.** Differential tests are auto-discovered (`tests/differential/Cargo.toml` has no `[[test]]` sections; `.github/workflows/ci.yml` runs plain `cargo test --workspace`), so the new runner needs no manifest edit — but it **does** move the CI test-binary count from **166 to 167**, which the state-4 verification must expect rather than treat as a regression.

---

## §6. Size estimate and the §6.1 split gate

Bottom-up, **re-derived at this PLAN-write** from the enumerated edit sites; net LoC **excluding `docs/`** (the house metric, per §1 PV-8). §9 of the SPEC estimated ≈1000; that figure is **not inherited**.

| area | net LoC |
|---|---:|
| `response.rs` — emit seam three-way fork, `Vec`→`HeaderMap` conversion, doc comment | 25 |
| `client.rs` — read site, `HeaderMap`→`Vec` conversion, return-type change | 25 |
| `hcm.rs` — threading: 5 declarations + 17 production call/construction sites + local-reply clears | 45 |
| `error.rs` — `H2SendTrailers` variant | 6 |
| `client.rs` — 5 in-crate test call-site updates | 5 |
| `envoy-http2` unit tests — emit fork ×4 branches, read ×3, hcm end-to-end, StopAndSend suppression | 300 |
| `drive_http2` + `DriveHttp1Result` + `drive_http1` literal | 60 |
| `Driver::Http2.expected_trailers` + `Http1TrailerRule` + dispatch + `run_http2_arm` comparison | 25 |
| `{{HTTP2_TRAILERS_BACKEND_PORT}}` plumbing (scan + spawn + 4 kv/guard sites) | 25 |
| differential unit tests (trailer diff, YAML round-trip) | 60 |
| `backend.rs` — `Http2TrailersBackend` + smoke test | 75 |
| `http2-echo-server` — `--trailers` mode + argv/wire tests | 100 |
| fixture `0090` — `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md` | 145 |
| `tests/differential/tests/h2_response_trailers.rs` | 20 |
| **CENTRAL ESTIMATE** | **≈916** |

**Task count: 10.**

### The gate verdict

> §6.1: splitting is triggered if `PLAN.md` exceeds **~25 numbered tasks** OR estimates exceed **~1500 lines of net change**.

- Tasks: **10** — well under ~25.
- Net LoC: **≈916** — under ~1500.

**THE §6.1 GATE DOES NOT FIRE. Phase 111 is NOT split. `111.1`/`111.2` are NOT created.**

**Risk stated honestly rather than hidden.** At the re-measured median 1.32× this lands ≈1209 (under the gate); at the worst-observed 1.75× it lands ≈1603 (over). The gate reads on the *estimate*, and the estimate clears it — but the worst case does not, so this is a phase to watch, not a comfortable one. Two considerations decided against a precautionary split:
1. **The §5 lifecycle costs six sessions per phase regardless of size.** Phase 110's split is the completed datapoint: it fired the gate at a centred ≈1600 and its two slices landed at 1290 and 817 — **twelve sessions for ≈2100 net LoC**. Splitting an ≈916 phase into two ≈460 slices would spend twelve sessions on work one phase does in six, for slices well below any threshold.
2. **The seam the SPEC proposes is real but the halves are lopsided.** SPEC §9's seam puts ~406 LoC in `111.1` (production + unit tests) and ~510 in `111.2` (harness + fixture + contract). Neither half approaches the gate.
**If contact with reality pushes a single task's sub-steps past ~10 items, §6.1's mid-execution trigger still applies** and the state-3 session must split then. The most likely candidate is Task 3 (the `hcm.rs` threading, 17 call sites).

---

## §7. Tasks

Ten tasks. Each ends with an independently testable deliverable and a commit. Commit messages use the phase's `phase 111: <what>` prefix.

**Before Task 1, and once only:** confirm the tree is clean and the debug binary is current.

```bash
git status --porcelain          # must be empty
git rev-parse HEAD              # record it; every file:line below was resolved at 82e2e75
cargo build --workspace --all-targets
```

---

### Task 1: Downstream emit seam — `send_envoy_response` learns to send trailers

**Files:**
- Modify: `crates/envoy-http2/src/error.rs` (new `H2SendTrailers` variant)
- Modify: `crates/envoy-http2/src/response.rs` (`send_envoy_response`, new `build_trailer_map`, module doc)
- Modify: `crates/envoy-http2/src/hcm.rs` (the single call site — passes `None` for now)
- Test: `crates/envoy-http2/src/response.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `pub async fn send_envoy_response(send_response: h2::server::SendResponse<bytes::Bytes>, resp: Response, trailers: Option<Vec<(String, String)>>) -> Result<(), Http2Error>`
  - `fn build_trailer_map(trailers: &[(String, String)]) -> Result<http::HeaderMap, Http2Error>` (private to `response.rs`)
  - `Http2Error::H2SendTrailers { source: h2::Error }`

- [ ] **Step 1: Confirm adding an error variant is safe**

The standing trap is that widening a returnable error set lands in a caller's `unreachable!()`, and gate (e) is blind to it. Re-run the check rather than trusting this plan:

```bash
git grep -n 'unreachable!\|unimplemented!' -- 'crates/envoy-http2/**/*.rs' 'crates/envoy-bin/**/*.rs'
git grep -n 'match .*Http2Error' -- '*.rs'
```

Expected: no `unreachable!`/`unimplemented!` hits, and no exhaustive `match` on `Http2Error` outside `crates/envoy-http2/src/error.rs`. If either expectation fails, STOP and handle the new arm explicitly before continuing.

- [ ] **Step 2: Write the failing tests**

Add to `crates/envoy-http2/src/response.rs`'s existing `mod tests`. The `round_trip` helper is new — `build_http_response` alone cannot witness which FRAMES were sent, and the frame sequence is the entire subject of this task.

```rust
    /// Drive `send_envoy_response` over a real in-process H2 connection and
    /// return what the client actually observed on the wire. The end-of-stream
    /// fork is only visible here: `build_http_response` sees headers, never
    /// frames.
    async fn round_trip(
        resp: Response,
        trailers: Option<Vec<(String, String)>>,
    ) -> (u16, Vec<u8>, Vec<(String, String)>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (tcp, _peer) = listener.accept().await.unwrap();
            let mut conn = h2::server::handshake(tcp).await.unwrap();
            if let Some(accepted) = conn.accept().await {
                let (_req, send_response) = accepted.unwrap();
                send_envoy_response(send_response, resp, trailers)
                    .await
                    .expect("send_envoy_response must succeed");
            }
            while conn.accept().await.is_some() {}
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, connection) = h2::client::handshake(tcp).await.unwrap();
        let conn_task = tokio::spawn(async move {
            let _ = connection.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://probe.local/")
            .body(())
            .unwrap();
        let (response_fut, _tx) = send_request.send_request(req, true).unwrap();
        let response = response_fut.await.unwrap();
        let status = response.status().as_u16();
        let mut body_stream = response.into_body();
        let mut body = Vec::new();
        while let Some(chunk) = body_stream.data().await {
            let chunk = chunk.unwrap();
            body.extend_from_slice(&chunk);
            let _ = body_stream.flow_control().release_capacity(chunk.len());
        }
        // MUST be awaited before aborting the connection task, or the trailer
        // HEADERS frame is never pumped off the socket.
        let observed: Vec<(String, String)> = body_stream
            .trailers()
            .await
            .unwrap()
            .map(|map| {
                map.iter()
                    .map(|(n, v)| (n.as_str().to_string(), v.to_str().unwrap().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        conn_task.abort();
        server.abort();
        (status, body, observed)
    }

    fn sorted(mut v: Vec<(String, String)>) -> Vec<(String, String)> {
        v.sort();
        v
    }

    #[tokio::test]
    async fn trailers_follow_a_non_empty_body() {
        let resp = synth_response(200, vec![("content-type", "text/plain")], b"BODY-OK");
        let (status, body, trailers) = round_trip(
            resp,
            Some(vec![
                ("x-trail-a".to_string(), "alpha".to_string()),
                ("x-trail-b".to_string(), "beta".to_string()),
            ]),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body, b"BODY-OK");
        assert_eq!(
            sorted(trailers),
            vec![
                ("x-trail-a".to_string(), "alpha".to_string()),
                ("x-trail-b".to_string(), "beta".to_string()),
            ]
        );
    }

    /// The gRPC main case, not a corner: a trailers-only response has an empty
    /// body by construction. Today's `send_response(head, end_of_stream=true)`
    /// branch makes any following frame a `UserError::UnexpectedFrameType`.
    #[tokio::test]
    async fn trailers_follow_an_empty_body_with_no_data_frame() {
        let resp = synth_response(200, vec![("content-type", "application/grpc")], b"");
        let (status, body, trailers) = round_trip(
            resp,
            Some(vec![("grpc-status".to_string(), "0".to_string())]),
        )
        .await;
        assert_eq!(status, 200);
        assert!(body.is_empty(), "expected no DATA frame, got {body:?}");
        assert_eq!(
            trailers,
            vec![("grpc-status".to_string(), "0".to_string())]
        );
    }

    /// PV-6 regression pin: the no-trailers non-empty-body path must be
    /// byte-identical to today.
    #[tokio::test]
    async fn no_trailers_non_empty_body_is_unchanged() {
        let resp = synth_response(200, vec![("content-type", "text/plain")], b"BODY-OK");
        let (status, body, trailers) = round_trip(resp, None).await;
        assert_eq!(status, 200);
        assert_eq!(body, b"BODY-OK");
        assert!(trailers.is_empty(), "got unexpected trailers {trailers:?}");
    }

    /// PV-6 regression pin: the no-trailers EMPTY-body path keeps its
    /// `end_of_stream = true` HEADERS frame.
    #[tokio::test]
    async fn no_trailers_empty_body_is_unchanged() {
        let resp = synth_response(204, vec![], b"");
        let (status, body, trailers) = round_trip(resp, None).await;
        assert_eq!(status, 204);
        assert!(body.is_empty());
        assert!(trailers.is_empty(), "got unexpected trailers {trailers:?}");
    }

    /// PV-3 rows 10-12: Envoy forwards `content-length`, `te: trailers` and
    /// `host` in a trailer block verbatim, and `h2`'s send-side `check_headers`
    /// permits all three. This pins that we do NOT strip them (D-PLAN-4).
    #[tokio::test]
    async fn trailer_names_envoy_forwards_are_not_stripped() {
        let resp = synth_response(200, vec![("content-type", "text/plain")], b"BODY-OK");
        let (_status, _body, trailers) = round_trip(
            resp,
            Some(vec![
                ("content-length".to_string(), "7".to_string()),
                ("te".to_string(), "trailers".to_string()),
                ("host".to_string(), "example.com".to_string()),
            ]),
        )
        .await;
        assert_eq!(sorted(trailers).len(), 3);
    }

    /// Duplicate trailer names must both reach the wire (Envoy preserves them
    /// — PV-3 row 5). `HeaderMap::append`, not `insert`.
    #[tokio::test]
    async fn duplicate_trailer_names_are_both_emitted() {
        let resp = synth_response(200, vec![("content-type", "text/plain")], b"BODY-OK");
        let (_status, _body, trailers) = round_trip(
            resp,
            Some(vec![
                ("x-multi".to_string(), "one".to_string()),
                ("x-multi".to_string(), "two".to_string()),
            ]),
        )
        .await;
        assert_eq!(sorted(trailers), vec![
            ("x-multi".to_string(), "one".to_string()),
            ("x-multi".to_string(), "two".to_string()),
        ]);
    }
```

- [ ] **Step 3: Run the tests to verify they FAIL**

```bash
cargo test -p envoy-http2 --lib response::tests 2>&1 | tee /tmp/t1-red.txt
grep -E 'test result' /tmp/t1-red.txt
```

Expected: a COMPILE error — `send_envoy_response` takes 2 arguments, not 3. That compile failure is a legitimate RED for a signature-changing task (a compile error is NOT a valid mutation-check RED, but it IS the correct TDD RED when the test names an interface that does not exist yet). **Do not gate on the exit code alone — read the error and confirm it names the arity/type, not something unrelated.**

- [ ] **Step 4: Add the error variant**

In `crates/envoy-http2/src/error.rs`, after the `H2BodyRead` variant:

```rust
    /// Writing the downstream trailer block via `h2::SendStream::send_trailers`
    /// failed. Distinct from `H2BodyRead` — whose name is already a misnomer
    /// when applied to a body WRITE — so a trailer failure is attributable
    /// without widening that misnomer further. Phase 111.
    #[error("HTTP/2 trailer send failed: {source}")]
    H2SendTrailers {
        #[source]
        source: h2::Error,
    },
```

- [ ] **Step 5: Implement the three-way fork**

In `crates/envoy-http2/src/response.rs`, replace `send_envoy_response` and add `build_trailer_map` above it:

```rust
/// Translate a trailer block into an `http::HeaderMap` for
/// `h2::SendStream::send_trailers`.
///
/// # No hop-by-hop strip here, deliberately
///
/// `build_http_response` strips `crate::H2_FORBIDDEN_HOP_BY_HOP` from the
/// HEADER block. The trailer block gets no such strip, and that is a measured
/// decision rather than an oversight (phase 111, D-PLAN-4): `h2` rejects
/// exactly `connection` / `transfer-encoding` / `upgrade` / `keep-alive` /
/// `proxy-connection` / `te` != `trailers` on the RECEIVE side too, so an
/// upstream block containing any of them fails in
/// `ClientStream::send_request`'s drain loop and never reaches this function.
/// A strip here would be unreachable, untestable code (§6.3). The
/// receive-side asymmetry against upstream Envoy — which drops the block and
/// resets the stream where envoy-rust returns 503 — is banked as CF-111-5.
///
/// `append`, not `insert`: upstream Envoy preserves duplicate trailer names
/// and so must we.
fn build_trailer_map(trailers: &[(String, String)]) -> Result<http::HeaderMap, Http2Error> {
    let mut map = http::HeaderMap::with_capacity(trailers.len());
    for (name, value) in trailers {
        let name_lc = name.to_ascii_lowercase();
        let header_name = HeaderName::from_bytes(name_lc.as_bytes())
            .map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
        let header_value =
            HeaderValue::from_str(value).map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
        map.append(header_name, header_value);
    }
    Ok(map)
}

pub async fn send_envoy_response(
    mut send_response: h2::server::SendResponse<bytes::Bytes>,
    resp: Response,
    trailers: Option<Vec<(String, String)>>,
) -> Result<(), Http2Error> {
    let head = build_http_response(&resp)?;
    let trailer_map = match trailers {
        Some(t) => Some(build_trailer_map(&t)?),
        None => None,
    };
    let body_empty = resp.body.is_empty();
    // Three-way fork. `h2` returns `UserError::UnexpectedFrameType` for any
    // frame sent after END_STREAM, so END_STREAM may only ride the LAST frame
    // we intend to send. A trailers-only response (empty body + trailers)
    // sends NO DATA frame at all — legal, and the gRPC main case.
    let mut send_stream = send_response
        .send_response(
            head,
            /* end_of_stream = */ body_empty && trailer_map.is_none(),
        )
        .map_err(|source| Http2Error::H2StreamAccept { source })?;
    if !body_empty {
        send_stream
            .send_data(resp.body, /* end_of_stream = */ trailer_map.is_none())
            .map_err(|source| Http2Error::H2BodyRead { source })?;
    }
    if let Some(map) = trailer_map {
        send_stream
            .send_trailers(map)
            .map_err(|source| Http2Error::H2SendTrailers { source })?;
    }
    Ok(())
}
```

Also update the module doc at the top of `response.rs` — the line reading
`//!   - `send_envoy_response(send_response, resp)` — drives the actual H2 wire`
gains the third parameter and a sentence naming the three-way fork.

- [ ] **Step 6: Update the single production call site**

In `crates/envoy-http2/src/hcm.rs`, locate by text:

```rust
    let send_result = send_envoy_response(send_response, resp).await;
```

and make it:

```rust
    // Phase 111 Task 1: the trailer channel exists but is not yet fed —
    // Task 3 threads the upstream trailers into this argument.
    let send_result = send_envoy_response(send_response, resp, None).await;
```

- [ ] **Step 7: Run the tests to verify they PASS**

```bash
cargo test -p envoy-http2 --lib response::tests 2>&1 | tee /tmp/t1-green.txt
grep -E 'test result' /tmp/t1-green.txt
```

Expected: `test result: ok.` with 0 failed, and the six new tests present. **Assert the count is non-zero** — `0 passed; N filtered out` is a false green.

- [ ] **Step 8: Prove the whole crate still builds and its tests pass**

```bash
cargo test -p envoy-http2 2>&1 | tee /tmp/t1-crate.txt
grep -E 'test result' /tmp/t1-crate.txt
```

- [ ] **Step 9: Commit**

```bash
git add crates/envoy-http2/src/error.rs crates/envoy-http2/src/response.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 111 task 1: send_envoy_response emits a trailer block (three-way end-of-stream fork)"
```

---

### Task 2: Upstream read site — `ClientStream::send_request` returns the trailer block

**Files:**
- Modify: `crates/envoy-http2/src/client.rs`
- Modify: `crates/envoy-http2/src/hcm.rs` (2 production call sites — mechanical, keeps compiling)
- Test: `crates/envoy-http2/src/client.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing from Task 1 (the two halves meet in Task 3).
- Produces: `pub async fn send_request(&mut self, request: Request) -> Result<(Response, Option<Vec<(String, String)>>), Http2Error>`

- [ ] **Step 1: Write the failing tests**

Add to `crates/envoy-http2/src/client.rs`'s `mod tests`. A trailer-emitting in-process server is needed; model it on the existing `spawn_h2_server` in the same module, and on the trailers precedent in `crates/envoy-http2/src/grpc.rs`'s tests.

```rust
    /// Spawn an in-process h2 server that answers with `200` + `body` and then
    /// the given trailer block. `trailers: &[]` means "send none" — the
    /// no-trailer control.
    async fn spawn_h2_server_with_trailers(
        body: &'static str,
        trailers: &'static [(&'static str, &'static str)],
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (tcp, _peer) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            let mut conn = match h2::server::handshake(tcp).await {
                Ok(c) => c,
                Err(_) => return,
            };
            while let Some(result) = conn.accept().await {
                let (req, mut send_response) = match result {
                    Ok(p) => p,
                    Err(_) => return,
                };
                let (_parts, mut recv) = req.into_parts();
                while let Some(chunk) = recv.data().await {
                    let chunk = match chunk {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    let _ = recv.flow_control().release_capacity(chunk.len());
                }
                let head = http::Response::builder()
                    .status(200)
                    .header("content-type", "text/plain")
                    .body(())
                    .unwrap();
                let mut send_stream = match send_response.send_response(head, false) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let _ = send_stream.send_data(Bytes::from_static(body.as_bytes()), trailers.is_empty());
                if !trailers.is_empty() {
                    let mut map = http::HeaderMap::new();
                    for (n, v) in trailers {
                        map.append(
                            http::HeaderName::from_static(n),
                            http::HeaderValue::from_static(v),
                        );
                    }
                    let _ = send_stream.send_trailers(map);
                }
            }
        });
        (addr, handle)
    }

    #[tokio::test]
    async fn send_request_returns_the_upstream_trailer_block() {
        let (addr, srv) =
            spawn_h2_server_with_trailers("BODY-OK", &[("x-trail-a", "alpha"), ("x-trail-b", "beta")])
                .await;
        let mut stream = Client::connect(addr, "probe.local").await.unwrap();
        let req = Request {
            method: "GET".to_string(),
            path: "/".to_string(),
            version: HttpVersion::Http11,
            headers: vec![("host".to_string(), "probe.local".to_string())],
            body: Bytes::new(),
        };
        let (resp, trailers) = stream.send_request(req).await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(&resp.body[..], b"BODY-OK");
        let mut got = trailers.expect("trailer block must be present");
        got.sort();
        assert_eq!(
            got,
            vec![
                ("x-trail-a".to_string(), "alpha".to_string()),
                ("x-trail-b".to_string(), "beta".to_string()),
            ]
        );
        srv.abort();
    }

    #[tokio::test]
    async fn send_request_returns_none_when_upstream_sends_no_trailers() {
        let (addr, srv) = spawn_h2_server_with_trailers("BODY-OK", &[]).await;
        let mut stream = Client::connect(addr, "probe.local").await.unwrap();
        let req = Request {
            method: "GET".to_string(),
            path: "/".to_string(),
            version: HttpVersion::Http11,
            headers: vec![("host".to_string(), "probe.local".to_string())],
            body: Bytes::new(),
        };
        let (resp, trailers) = stream.send_request(req).await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(&resp.body[..], b"BODY-OK");
        assert!(
            trailers.is_none(),
            "a trailerless response must yield None, got {trailers:?}"
        );
        srv.abort();
    }
```

**Note on the `Request` literal:** the exact field set of `envoy_http1::codec::Request` must be copied from the existing tests in the same module rather than from this plan — read one of the five existing `send_request` tests and mirror its construction verbatim. If it differs from the shape above, the existing tests are authoritative.

- [ ] **Step 2: Run the tests to verify they FAIL**

```bash
cargo test -p envoy-http2 --lib client::tests 2>&1 | tee /tmp/t2-red.txt
```

Expected: a compile error on the destructuring `let (resp, trailers) = ...` — `send_request` returns `Response`, not a tuple. Read the message and confirm it names the tuple mismatch.

- [ ] **Step 3: Implement the read**

In `crates/envoy-http2/src/client.rs`, change the signature (locate by text `pub async fn send_request`):

```rust
    /// Returns the proxied response and, if the upstream sent one, its
    /// TRAILER block in wire order.
    ///
    /// The trailers ride ALONGSIDE `Response` rather than as a field on it:
    /// `envoy_http1::Response` is shared across four crates with 42 struct-
    /// literal sites and a `PartialEq`/`Eq` derive, and only the HTTP/2 path
    /// can ever populate or emit a trailer block (phase 111, D-PLAN-2).
    pub async fn send_request(
        &mut self,
        request: Request,
    ) -> Result<(Response, Option<Vec<(String, String)>>), Http2Error> {
```

Then, immediately after the existing body-drain loop's closing brace and BEFORE the `(g)` status-range guard (so a trailer block is read even on a status the guard would reject — and, more importantly, so `recv_stream` is still alive):

```rust
        // (f2) Phase 111: read the trailer block. `h2` only resolves this once
        // `data()` has returned `None`, which the drain loop above guarantees.
        // `Ok(None)` is the common case — a response with no trailers.
        let trailers: Option<Vec<(String, String)>> = recv_stream
            .trailers()
            .await
            .map_err(|source| Http2Error::H2RecvBody { source })?
            .map(|map| {
                let mut out: Vec<(String, String)> = Vec::with_capacity(map.len());
                for (name, value) in map.iter() {
                    // Same defensive posture as the header conversion below:
                    // skip a non-ASCII value rather than failing the response.
                    let Ok(value_str) = value.to_str() else {
                        continue;
                    };
                    out.push((name.as_str().to_string(), value_str.to_string()));
                }
                out
            });
```

and change the tail from `Ok(Response { .. })` to:

```rust
        Ok((
            Response {
                status,
                reason: None,
                headers,
                body: body_bytes.freeze(),
            },
            trailers,
        ))
```

- [ ] **Step 4: Fix the in-crate call sites so the crate compiles**

Two production sites in `crates/envoy-http2/src/hcm.rs` — locate by text (`client_stream_mut().send_request(` and the no-pool `.send_request(out_req)`), and make each discard the trailers **for now**, with a marker Task 3 will consume:

```rust
        // Phase 111 Task 2: trailers read but not yet threaded; Task 3 carries
        // them to the emit seam.
        .map(|(r, _trailers)| r)
```

Plus the five `#[cfg(test)]` call sites inside `client.rs` — locate them with `grep -n 'send_request(' crates/envoy-http2/src/client.rs` and destructure `(resp, _)` at each.

- [ ] **Step 5: Run the tests to verify they PASS**

```bash
cargo test -p envoy-http2 2>&1 | tee /tmp/t2-green.txt
grep -E 'test result' /tmp/t2-green.txt
```

Expected `test result: ok.`, 0 failed, with a non-zero passed count.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-http2/src/client.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 111 task 2: ClientStream::send_request returns the upstream H2 trailer block"
```

---

### Task 3: Thread the trailers from the upstream attempt to the emit seam

This is the largest single task and the most likely candidate for §6.1's **mid-execution** split trigger. If any step below blows past ~10 sub-steps on contact with reality, stop and split per §6.2.

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs`
- Test: `crates/envoy-http2/src/hcm.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: Task 1's `send_envoy_response(.., .., Option<Vec<(String, String)>>)`; Task 2's `send_request -> (Response, Option<Vec<(String, String)>>)`.
- Produces: `finalize_h2_stream(.., resp: Response, trailers: Option<Vec<(String, String)>>, ..)` — the trailers parameter is inserted immediately after `resp` so the two travel together at every hop.

The five declarations to change, all in `crates/envoy-http2/src/hcm.rs`, **located by text** (line numbers here were resolved at `82e2e75` and will drift as you edit):

| locate by | at `82e2e75` | change |
|---|---|---|
| `enum AcquireOutcome {` … `Sent(Result<envoy_http1::Response, String>)` | `:242`, `:245` | payload becomes `Result<(envoy_http1::Response, Option<Vec<(String, String)>>), String>` |
| `struct H2AttemptResult {` | `:141` | new field `trailers: Option<Vec<(String, String)>>` with a doc comment |
| `let (final_response, completing_upstream_response): (Response, bool) = loop {` | `:716` | becomes a 3-tuple carrying the trailers |
| `let resp: Response = match request_path {` | `:568` | becomes `let (resp, resp_trailers): (Response, Option<Vec<(String, String)>>) = match request_path {` |
| `async fn finalize_h2_stream(` | `:961` | new `trailers` parameter after `resp` (the fn already carries `#[allow(clippy::too_many_arguments)]`, so no new lint) |

- [ ] **Step 1: Write the failing end-to-end test**

Add to `crates/envoy-http2/src/hcm.rs`'s `mod tests`. Model the upstream server on the existing `spawn_upstream_h2_server` helper in the same module — **read it and mirror its shape**; the sketch below shows only the trailer delta.

```rust
    /// Phase 111: an upstream H2 trailer block must reach the downstream
    /// client unchanged. This is the in-process twin of differential fixture
    /// `0090-h2-response-trailers`.
    #[tokio::test]
    async fn h2_forwards_upstream_response_trailers_downstream() {
        // Upstream: 200 + `content-type` + `trailer: x-trail-a` announce
        // header + body `BODY-OK` + trailers `x-trail-a: alpha` (announced)
        // and `x-trail-b: beta` (NOT announced). Envoy forwards both, so the
        // rule under test is "forward the block", not "forward what was
        // announced".
        let upstream = spawn_upstream_h2_server_with_trailers(
            "BODY-OK",
            &[("x-trail-a", "alpha"), ("x-trail-b", "beta")],
        )
        .await;

        let observed = drive_one_h2_request_through_hcm(upstream.addr, "/").await;

        assert_eq!(observed.status, 200);
        assert_eq!(observed.body, b"BODY-OK");
        assert!(
            observed
                .headers
                .iter()
                .any(|(n, v)| n == "trailer" && v == "x-trail-a"),
            "the `trailer:` announce header is a pre-existing pass and must not regress"
        );
        let mut trailers = observed.trailers.clone();
        trailers.sort();
        assert_eq!(
            trailers,
            vec![
                ("x-trail-a".to_string(), "alpha".to_string()),
                ("x-trail-b".to_string(), "beta".to_string()),
            ],
            "both the announced AND the unannounced trailer must be forwarded"
        );
    }

    /// PV-6 regression pin at the HCM level: a trailerless upstream response
    /// must reach the client with no trailer block at all.
    #[tokio::test]
    async fn h2_trailerless_upstream_response_forwards_no_trailers() {
        let upstream = spawn_upstream_h2_server_with_trailers("BODY-OK", &[]).await;
        let observed = drive_one_h2_request_through_hcm(upstream.addr, "/").await;
        assert_eq!(observed.status, 200);
        assert_eq!(observed.body, b"BODY-OK");
        assert!(
            observed.trailers.is_empty(),
            "expected no trailers, got {:?}",
            observed.trailers
        );
    }
```

Two helpers are needed and neither exists yet:
- `spawn_upstream_h2_server_with_trailers(body, trailers)` — copy the existing `spawn_upstream_h2_server` in this module and give its response path the same `send_data(.., trailers.is_empty())` + `send_trailers(map)` shape Task 2's client-side helper uses.
- `drive_one_h2_request_through_hcm(addr, path)` — the module already drives the HCM over an in-process H2 client in several tests; reuse whichever existing helper does that and **extend its observation type with a `trailers: Vec<(String, String)>` field**, reading `body_stream.trailers().await` after the body drain and BEFORE any connection abort. If no reusable helper exists, write one modelled on the closest existing test.

- [ ] **Step 2: Run the test to verify it FAILS**

```bash
cargo test -p envoy-http2 --lib h2_forwards_upstream_response_trailers 2>&1 | tee /tmp/t3-red.txt
grep -E 'test result|panicked|assertion' /tmp/t3-red.txt
```

Expected: a genuine assertion failure — the observed trailer vector is EMPTY while two were expected. **This must be an assertion failure, not a compile error** — if it does not compile, the helper sketch is wrong; fix the helper, not the assertion. Confirm a `test result:` line exists (a compile error is not a RED).

- [ ] **Step 3: Widen `AcquireOutcome::Sent`**

```rust
        // The upstream connected and send_request resolved (Ok = real response
        // plus its trailer block, if any; Err = post-connect send/recv failure
        // to be classified as Reset).
        Sent(Result<(envoy_http1::Response, Option<Vec<(String, String)>>), String>),
```

Then remove the `.map(|(r, _trailers)| r)` markers Task 2 left at the two H2 forks, and **wrap the H1 fork** — which shares this enum and has no trailers — as `(r, None)`. Locate all three by text.

- [ ] **Step 4: Add the `H2AttemptResult` field**

```rust
    /// Phase 111: the upstream response's TRAILER block, if it sent one.
    /// `None` on every synth/local path — a locally-generated response never
    /// carries upstream trailers (D-PLAN-5).
    trailers: Option<Vec<(String, String)>>,
```

Then supply it at all **five** literal sites (`grep -n 'H2AttemptResult {' crates/envoy-http2/src/hcm.rs` — at `82e2e75` they are `:191`, `:372`, `:390`, `:403`, `:414`). Exactly ONE of them — the successful proxied-response arm, the one destructuring `AcquireOutcome::Sent(Ok(..))` — passes the real trailers; **the other four pass `None`**.

- [ ] **Step 5: Widen the retry-loop tuple**

```rust
                    let (final_response, final_trailers, completing_upstream_response): (
                        Response,
                        Option<Vec<(String, String)>>,
                        bool,
                    ) = loop {
```

and its single `break`:

```rust
                        break (
                            attempt.response,
                            attempt.trailers,
                            attempt.upstream_response,
                        );
```

- [ ] **Step 6: Widen the `request_path` match and its four arm tails**

```rust
    let (resp, resp_trailers): (Response, Option<Vec<(String, String)>>) = match request_path {
```

Four arm tails change (locate each by text):
- the `BuildOutcome::Synth` arm tail `r` → `(r, None)`
- the request-budget-rejected arm tail `overflow_resp` → `(overflow_resp, None)`
- the proxy arm tail `outgoing` → `(outgoing, final_trailers)` (with `let mut outgoing = final_response;` gaining a sibling `let outgoing_trailers = final_trailers;` if that reads more clearly)
- the `SynthFromDecode` arm tail `r` → `(r, None)`

**Every local-reply arm passes `None`.** That is D-PLAN-5 and it is load-bearing: a synth 404/503 must not inherit an upstream trailer block.

- [ ] **Step 7: Thread through `finalize_h2_stream`**

Add the parameter after `resp`:

```rust
    mut resp: Response,
    /// Phase 111: the upstream response's trailer block, forwarded verbatim
    /// to the downstream client. `None` on every locally-generated response.
    trailers: Option<Vec<(String, String)>>,
```

update the single call site (`finalize_h2_stream(` at `:930`) to pass `resp_trailers`, and change the emit call from Task 1's placeholder to:

```rust
    let send_result = send_envoy_response(send_response, resp, trailers).await;
```

- [ ] **Step 8: Run the tests to verify they PASS**

```bash
cargo test -p envoy-http2 2>&1 | tee /tmp/t3-green.txt
grep -E 'test result' /tmp/t3-green.txt
```

Expected `test result: ok.` with 0 failed across the whole crate.

- [ ] **Step 9: Prove the change did not break other crates**

Adding a field to a struct and widening an enum payload are cross-crate events in general; this design keeps them crate-local, so **verify that claim** rather than asserting it:

```bash
cargo build --workspace --all-targets 2>&1 | tail -20
```

Expected: clean. A `-p envoy-http2` build alone would NOT prove this.

- [ ] **Step 10: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 111 task 3: thread upstream H2 trailers to the downstream emit seam"
```

---

### Task 4: Locally-generated responses carry NO trailers

Task 3 wires `None` into the local-reply arms. This task pins that behaviour with its own tests so a later refactor cannot quietly reverse it — the failure mode is invisible on every existing fixture, because no fixture has trailers at all.

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (the encode-filter `StopAndSend` arm)
- Test: `crates/envoy-http2/src/hcm.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: Task 3's `finalize_h2_stream(.., trailers, ..)`.
- Produces: no new public surface.

- [ ] **Step 1: Read the encode-filter short-circuit and confirm the hazard**

```bash
grep -n 'Decision::StopAndSend' -A 12 crates/envoy-http2/src/hcm.rs
```

The arm rebuilds `resp` as a fresh `Response { status, reason, headers, body }` literal from the filter's replacement. Under D-PLAN-2 the trailers live in a SEPARATE local, so they are **not** discarded by that rebuild — they would survive and be emitted on a response the upstream never sent. That is the inverse of the field-on-`Response` hazard and just as wrong.

- [ ] **Step 2: Write the failing test**

```rust
    /// Phase 111 D-PLAN-5: an encode-side filter that REPLACES the response
    /// must not inherit the upstream's trailer block. The trailers ride
    /// alongside `Response`, so the `StopAndSend` rebuild does not drop them
    /// on its own — this pins the explicit clear.
    #[tokio::test]
    async fn h2_encode_filter_stop_and_send_drops_upstream_trailers() {
        let upstream = spawn_upstream_h2_server_with_trailers(
            "BODY-OK",
            &[("x-trail-a", "alpha")],
        )
        .await;
        // Build the HCM with an encode-side filter that short-circuits with a
        // replacement response. Mirror whichever existing test in this module
        // installs a StopAndSend encode filter — reuse its filter fixture
        // rather than writing a new one.
        let observed =
            drive_one_h2_request_through_hcm_with_stop_and_send(upstream.addr, "/").await;
        assert!(
            observed.trailers.is_empty(),
            "a filter-replaced response must carry no upstream trailers, got {:?}",
            observed.trailers
        );
    }
```

**If no existing test in the module installs a `StopAndSend` encode filter**, the module doc at the `finalize_h2_stream` site records that no phase-11 filter takes that path. In that case build the smallest filter fixture that returns `Decision::StopAndSend` — **without touching `crates/envoy-filter/`** (a test-local implementation of the existing trait is not a change to the filter API).

- [ ] **Step 3: Run the test to verify it FAILS**

```bash
cargo test -p envoy-http2 --lib stop_and_send_drops_upstream_trailers 2>&1 | tee /tmp/t4-red.txt
grep -E 'test result|assertion' /tmp/t4-red.txt
```

Expected: an assertion failure showing `x-trail-a` leaked onto the filter-replaced response.

- [ ] **Step 4: Implement the clear**

In the `envoy_filter::Decision::StopAndSend(replacement)` arm of `finalize_h2_stream`, alongside the existing `resp = Response { .. }` rebuild:

```rust
            // Phase 111 D-PLAN-5: the filter REPLACED the response, so the
            // upstream's trailer block no longer describes what is being sent.
            // SPEC non-goal 6 — trailers on locally-generated replies are out
            // of scope and upstream's behaviour there is unmeasured.
            trailers = None;
```

(the `trailers` parameter must be `mut` for this).

- [ ] **Step 5: Run the tests to verify they PASS**

```bash
cargo test -p envoy-http2 2>&1 | tee /tmp/t4-green.txt
grep -E 'test result' /tmp/t4-green.txt
```

- [ ] **Step 6: Mutation-check the clear (it is a one-line guard, so prove it is not vacuous)**

Use a scratch worktree — a mutation edit in the main tree collides with anything else running, and a stale test binary gives a FALSE PASS.

```bash
git worktree add /tmp/mut-111-t4 HEAD
cd /tmp/mut-111-t4
# assert the target text occurs EXACTLY ONCE before mutating
grep -c 'trailers = None;' crates/envoy-http2/src/hcm.rs      # must print 1
sed -i 's/            trailers = None;/            \/\/ MUTATED/' crates/envoy-http2/src/hcm.rs
touch crates/envoy-http2/src/lib.rs                            # force a real rebuild
cargo test -p envoy-http2 --lib stop_and_send_drops_upstream_trailers 2>&1 | tee /tmp/t4-mut.txt
grep -E 'Compiling envoy-http2|test result' /tmp/t4-mut.txt
cd - && git worktree remove --force /tmp/mut-111-t4
```

Expected: a `Compiling envoy-http2` line (proving no stale binary), a `test result:` line (proving it ran rather than failing to compile), and `FAILED`. A GREEN here means the mutation is misaimed or the test is vacuous — investigate before proceeding.

- [ ] **Step 7: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 111 task 4: locally-generated H2 responses carry no upstream trailers"
```

---

### Task 5: A trailer-emitting H2 backend the harness can spawn

**Files:**
- Modify: `tests/helpers/http2-echo-server/src/main.rs`
- Modify: `tests/differential/src/backend.rs`
- Test: both files' `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `http2-echo-server --port <u16> [--trailers]` — in `--trailers` mode the response keeps its existing deterministic echo body and gains the header `trailer: x-trail-a` plus a trailer block of `x-trail-a: alpha` and `x-trail-b: beta`.
  - `pub struct Http2TrailersBackend` with `pub async fn spawn() -> Result<Self>`, `pub fn port(&self) -> u16`, `pub fn container_host(&self) -> &'static str`.

**The trailer block is fixed, not configurable.** One announced trailer and one unannounced one is exactly the measured divergence (PV-1) and exactly what fixture `0090` asserts. A configurable block would be surface no fixture uses.

- [ ] **Step 1: Write the failing helper tests**

In `tests/helpers/http2-echo-server/src/main.rs`'s `mod tests`, mirroring the existing `parse_argv_accepts_close_before_response`:

```rust
    #[test]
    fn parse_argv_accepts_trailers() {
        let args = vec![
            "--port".to_string(),
            "8080".to_string(),
            "--trailers".to_string(),
        ];
        let parsed = parse_argv(&args).expect("parses");
        assert_eq!(parsed.port, 8080);
        assert!(parsed.trailers);
        assert!(!parsed.close_before_response);
    }

    #[test]
    fn parse_argv_defaults_trailers_off() {
        let args = vec!["--port".to_string(), "8080".to_string()];
        let parsed = parse_argv(&args).expect("parses");
        assert!(!parsed.trailers);
    }
```

**Both existing `assert_eq!(args, Args { .. })` literals in that module will stop compiling** once `Args` gains a field — that is expected; add `trailers: false` to each.

- [ ] **Step 2: Run to verify FAIL**

```bash
cargo test -p http2-echo-server 2>&1 | tee /tmp/t5-red.txt
```

Expected: compile error — no field `trailers` on `Args`.

- [ ] **Step 3: Implement the flag and the mode**

`Args` gains `trailers: bool`. `parse_argv`'s closure gains a branch alongside the existing one:

```rust
        } else if args[*i] == "--trailers" {
            trailers = true;
            *i += 1;
            Ok(true)
```

`print_help`'s usage line becomes:

```
  http2-echo-server --port <u16> [--close-before-response] [--trailers]
```

The accept loop's dispatch gains a third branch, and the response path in the trailers mode changes ONLY in its tail — the `make_response_body` echo shape is untouched, because fixture `0090` inherits fixture `0010`'s byte-exact body comparison:

```rust
            let response = http::Response::builder()
                .status(200)
                .header("content-type", "text/plain")
                // RFC 7230 §4.4 announce header. Deliberately names only ONE
                // of the two trailers sent: upstream Envoy forwards the
                // UNANNOUNCED one too, so the rule under test is "forward the
                // block", not "forward what was announced" (phase 111 PV-1).
                .header("trailer", "x-trail-a")
                .body(())
                .unwrap();
            let mut send_stream = match send_response.send_response(response, false) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "send_response failed");
                    return;
                }
            };
            // end_of_stream = false: the trailer HEADERS frame follows.
            if let Err(e) = send_stream.send_data(Bytes::from(response_body), false) {
                tracing::warn!(error = %e, "send_data failed");
                return;
            }
            let mut trailers = http::HeaderMap::new();
            trailers.append("x-trail-a", http::HeaderValue::from_static("alpha"));
            trailers.append("x-trail-b", http::HeaderValue::from_static("beta"));
            if let Err(e) = send_stream.send_trailers(trailers) {
                tracing::warn!(error = %e, "send_trailers failed");
            }
```

- [ ] **Step 4: Write the failing backend-struct test**

In `tests/differential/src/backend.rs`'s `mod tests`, mirroring the existing `http2_echo_backend_spawns_and_echoes`:

```rust
    #[tokio::test]
    async fn http2_trailers_backend_spawns_and_emits_trailers() {
        let backend = match crate::backend::Http2TrailersBackend::spawn().await {
            Ok(b) => b,
            // Same graceful-skip posture as the sibling helper tests when the
            // helper binary has not been built.
            Err(_) => return,
        };
        assert!(backend.port() > 0);
        assert_eq!(backend.container_host(), "host.docker.internal");
    }
```

- [ ] **Step 5: Implement `Http2TrailersBackend`**

A near-copy of the existing `Http2CloseBackend` in the same file — same `spawn_helper_backend` call, same `wait_h2_accept_ready` readiness poll, same `Drop`/`kill_and_reap`:

```rust
/// A running `http2-echo-server --trailers` host subprocess — echoes the
/// deterministic body like `Http2EchoBackend`, and additionally announces
/// `trailer: x-trail-a` and emits a trailer block of `x-trail-a: alpha`
/// (announced) plus `x-trail-b: beta` (NOT announced). Phase 111: the
/// upstream for fixture `0090-h2-response-trailers`, the first fixture to
/// exercise the `Response trailers` row of the equivalence matrix.
pub struct Http2TrailersBackend {
    port: u16,
    child: Option<tokio::process::Child>,
}

impl Http2TrailersBackend {
    pub async fn spawn() -> Result<Self> {
        let (port, child, addr) = spawn_helper_backend(
            "http2-echo-server",
            "reserving h2 trailers-backend port",
            &[std::ffi::OsStr::new("--trailers")],
            " --trailers",
        )
        .await?;
        wait_h2_accept_ready(addr, Duration::from_secs(2))
            .await
            .with_context(|| {
                format!("http2-echo-server --trailers never became h2-accept-ready on {addr}")
            })?;

        Ok(Self {
            port,
            child: Some(child),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Per ADR-0015 + 05.1 STRICT_DNS posture: always `host.docker.internal`.
    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for Http2TrailersBackend {
    fn drop(&mut self) {
        kill_and_reap(&mut self.child);
    }
}
```

- [ ] **Step 6: Run to verify PASS**

```bash
cargo test -p http2-echo-server 2>&1 | tee /tmp/t5-helper.txt
cargo test -p differential --lib backend 2>&1 | tee /tmp/t5-backend.txt
grep -E 'test result' /tmp/t5-helper.txt /tmp/t5-backend.txt
```

- [ ] **Step 7: Commit**

```bash
git add tests/helpers/http2-echo-server/src/main.rs tests/differential/src/backend.rs
git commit -m "phase 111 task 5: http2-echo-server --trailers mode + Http2TrailersBackend"
```

---

### Task 6: The differential driver OBSERVES trailers

**Files:**
- Modify: `tests/differential/src/lib.rs`
- Test: `tests/differential/src/lib.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: Task 5's `--trailers` helper (for the round-trip test).
- Produces: `DriveHttp1Result` gains `pub trailers: Vec<(String, String)>` — empty when the response carried no trailer block.

**Two traps, both measured at this PLAN-write:**
1. `conn_handle.abort()` in `drive_http2` runs after the header loop. **`body_stream.trailers().await` must be inserted BEFORE that abort**, or the trailer HEADERS frame is never pumped off the socket and the driver silently reports zero trailers — a false green on the very cell under test.
2. `DriveHttp1Result` has exactly **two** struct-literal sites (one in `drive_http1`, one in `drive_http2`). The `drive_http1` one supplies `Vec::new()`: the harness's H1 chunked decoder explicitly discards trailers, and CF-111-2 leaves H1 trailers unbuilt.

- [ ] **Step 1: Write the failing test**

```rust
    /// Phase 111: `drive_http2` must surface a response trailer block. Without
    /// this the harness cannot express the phase's only divergence, and a
    /// divergence no fixture can express is invisible to the gate.
    #[tokio::test]
    async fn drive_http2_surfaces_response_trailers() {
        let backend = match crate::backend::Http2TrailersBackend::spawn().await {
            Ok(b) => b,
            Err(_) => return, // helper binary not built — same skip posture as siblings
        };
        let addr: std::net::SocketAddr =
            format!("127.0.0.1:{}", backend.port()).parse().unwrap();
        let result = drive_http2(addr, &Http1Method::Get, "/", "probe.local", &[])
            .await
            .expect("drive");
        assert_eq!(result.status, 200);
        let mut names: Vec<String> = result.trailers.iter().map(|(n, _)| n.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["x-trail-a".to_string(), "x-trail-b".to_string()]);
    }

    /// The trailerless control: a plain echo backend must yield an EMPTY
    /// trailer vector, never a spurious entry.
    #[tokio::test]
    async fn drive_http2_reports_no_trailers_when_none_sent() {
        let backend = match crate::backend::Http2EchoBackend::spawn().await {
            Ok(b) => b,
            Err(_) => return,
        };
        let addr: std::net::SocketAddr =
            format!("127.0.0.1:{}", backend.port()).parse().unwrap();
        let result = drive_http2(addr, &Http1Method::Get, "/", "probe.local", &[])
            .await
            .expect("drive");
        assert!(result.trailers.is_empty(), "got {:?}", result.trailers);
    }
```

- [ ] **Step 2: Run to verify FAIL**

```bash
cargo test -p differential --lib drive_http2_surfaces_response_trailers 2>&1 | tee /tmp/t6-red.txt
```

Expected: compile error — no field `trailers` on `DriveHttp1Result`.

- [ ] **Step 3: Implement**

Add the field:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveHttp1Result {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Phase 111: the response TRAILER block in wire order; EMPTY when the
    /// response carried none. Always empty from `drive_http1` — the H1
    /// chunked decoder discards trailers and H1 trailer forwarding is
    /// unbuilt (CF-111-2).
    pub trailers: Vec<(String, String)>,
}
```

In `drive_http2`, immediately after the body-drain `while` loop and **before** the `drop(send_request); conn_handle.abort();` block:

```rust
    // Phase 111: read the trailer block. `h2` resolves this only once `data()`
    // has returned `None`, which the drain loop above guarantees — and it MUST
    // be awaited before `conn_handle.abort()` below, or the trailer HEADERS
    // frame is never pumped off the socket.
    let mut trailers: Vec<(String, String)> = Vec::new();
    if let Some(map) = body_stream.trailers().await.context("H2 response trailers")? {
        for (n, v) in map.iter() {
            let value_str = v.to_str().with_context(|| {
                format!("non-UTF-8 H2 response trailer value for `{}`", n.as_str())
            })?;
            trailers.push((n.as_str().to_string(), value_str.to_string()));
        }
    }
```

and add `trailers` to `drive_http2`'s result literal. Add `trailers: Vec::new()` to `drive_http1`'s literal.

- [ ] **Step 4: Run to verify PASS**

```bash
cargo test -p differential --lib drive_http2 2>&1 | tee /tmp/t6-green.txt
grep -E 'test result' /tmp/t6-green.txt
```

- [ ] **Step 5: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 111 task 6: drive_http2 surfaces the response trailer block"
```

---

### Task 7: The differential harness COMPARES trailers

**Files:**
- Modify: `tests/differential/src/lib.rs`
- Test: `tests/differential/src/lib.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: Task 6's `DriveHttp1Result.trailers`.
- Produces:
  - `pub enum Http1TrailerRule { SetEqualModuloAllowList }` — externally-tagged unit variant, exact copy of `Http1HeaderRule`'s shape.
  - `Driver::Http2` gains `#[serde(default)] expected_trailers: Option<Http1TrailerRule>`.

**Design (PV-4, decided): reuse `diff_headers` verbatim.** No `diff_trailers`. `BEHAVIOR_CONTRACT.md:18` says *"Set-equal under the same allow-list discipline"* and `diff_headers` is that discipline. The cost — duplicate trailer-name multiplicity is unassertable — is banked as CF-111-8, and fixture `0090` avoids the cell.

**Why extend `Driver::Http2` rather than add a new `Driver` variant:** the existing variant already carries `method`/`path`/`host`/`expected_*` and `#[serde(default)]` keeps all 89 existing `expectations.yaml` files deserializing unchanged. A new variant would force mandatory new arms in `port_key_for` and the main `run_fixture` dispatch (both are exhaustive, no `_` arm) for no gain.

**`deny_unknown_fields` is on every `Driver` variant** — so the Rust field must land BEFORE any fixture YAML mentions `expected_trailers`, which is why Task 9 comes after this one.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn diff_headers_accepts_equal_trailer_sets() {
        let envoy = vec![
            ("x-trail-a".to_string(), "alpha".to_string()),
            ("x-trail-b".to_string(), "beta".to_string()),
        ];
        let rust = vec![
            ("x-trail-b".to_string(), "beta".to_string()),
            ("x-trail-a".to_string(), "alpha".to_string()),
        ];
        diff_headers(&envoy, &rust, HEADER_ALLOW_LIST).expect("order must not matter");
    }

    #[test]
    fn diff_headers_rejects_a_missing_trailer() {
        let envoy = vec![
            ("x-trail-a".to_string(), "alpha".to_string()),
            ("x-trail-b".to_string(), "beta".to_string()),
        ];
        let rust = vec![("x-trail-a".to_string(), "alpha".to_string())];
        let err = diff_headers(&envoy, &rust, HEADER_ALLOW_LIST)
            .expect_err("a dropped trailer must fail the diff");
        assert!(format!("{err}").contains("x-trail-b"), "got {err}");
    }

    #[test]
    fn diff_headers_rejects_a_differing_trailer_value() {
        let envoy = vec![("x-trail-a".to_string(), "alpha".to_string())];
        let rust = vec![("x-trail-a".to_string(), "WRONG".to_string())];
        diff_headers(&envoy, &rust, HEADER_ALLOW_LIST)
            .expect_err("a differing trailer value must fail the diff");
    }

    #[test]
    fn parses_http2_expectations_with_expected_trailers() {
        let yaml = r#"
driver:
  kind: http2
  method: get
  path: /
  host: envoy-rust.test
  expected_status: 200
  expected_headers: set_equal_modulo_allow_list
  expected_trailers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
"#;
        let parsed = load_expectations(yaml).expect("parses");
        match parsed.driver {
            Driver::Http2 {
                ref expected_trailers,
                ..
            } => assert_eq!(
                *expected_trailers,
                Some(Http1TrailerRule::SetEqualModuloAllowList)
            ),
            ref other => panic!("wrong driver: {other:?}"),
        }
    }

    /// Every one of the 89 pre-existing fixtures omits the key; it must
    /// deserialize to `None` rather than failing `deny_unknown_fields`.
    #[test]
    fn http2_expectations_without_expected_trailers_default_to_none() {
        let yaml = r#"
driver:
  kind: http2
  method: get
  path: /
  host: envoy-rust.test
  expected_status: 200
equivalence:
  response_status: exact
"#;
        let parsed = load_expectations(yaml).expect("parses");
        match parsed.driver {
            Driver::Http2 {
                ref expected_trailers,
                ..
            } => assert!(expected_trailers.is_none()),
            ref other => panic!("wrong driver: {other:?}"),
        }
    }
```

**Check `load_expectations`'s actual signature before writing these** — it may take a path rather than a string. If so, mirror whichever existing `parses_expectations_*` test in the module does a YAML round-trip.

- [ ] **Step 2: Run to verify FAIL**

```bash
cargo test -p differential --lib expected_trailers 2>&1 | tee /tmp/t7-red.txt
```

Expected: compile error — no `Http1TrailerRule`, no `expected_trailers` field.

- [ ] **Step 3: Implement the rule type**

Next to `Http1HeaderRule`:

```rust
/// Phase 111 NEW: trailer equivalence rule for `Driver::Http2`. Externally-
/// tagged unit variant, same shape as `Http1HeaderRule`, so the fixture YAML
/// reads `expected_trailers: set_equal_modulo_allow_list`.
///
/// Trailer comparison deliberately reuses `diff_headers` and
/// `HEADER_ALLOW_LIST` — `BEHAVIOR_CONTRACT.md`'s equivalence matrix specifies
/// "Set-equal under the same allow-list discipline" for response trailers. The
/// consequence is that duplicate trailer-name multiplicity is not asserted
/// (CF-111-8).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Http1TrailerRule {
    SetEqualModuloAllowList,
}
```

- [ ] **Step 4: Extend `Driver::Http2`, the dispatch, and `run_http2_arm`**

Variant gains:

```rust
        /// Phase 111 NEW: compare the response TRAILER blocks. Omitted by every
        /// pre-111 fixture, hence `#[serde(default)]`.
        #[serde(default)]
        expected_trailers: Option<Http1TrailerRule>,
```

The `Driver::Http2 { .. }` destructure in `run_fixture`'s dispatch gains `expected_trailers`, and passes it to `run_http2_arm`, whose signature gains `expected_trailers: &Option<Http1TrailerRule>`. At the end of `run_http2_arm`, directly after the existing `expected_headers` block:

```rust
    // Trailers: per-driver allow-list diff between envoy ↔ envoy-rust. Phase
    // 111 — the first exercise of the `Response trailers` row of the
    // equivalence matrix, unwitnessed since phase 00 seeded it.
    if matches!(
        expected_trailers,
        Some(Http1TrailerRule::SetEqualModuloAllowList)
    ) {
        diff_headers(
            &upstream_resp.trailers,
            &subject_resp.trailers,
            HEADER_ALLOW_LIST,
        )
        .context("diff_trailers (set_equal_modulo_allow_list)")?;
    }
```

- [ ] **Step 5: Run to verify PASS**

```bash
cargo test -p differential --lib 2>&1 | tee /tmp/t7-green.txt
grep -E 'test result' /tmp/t7-green.txt
```

- [ ] **Step 6: Prove no existing fixture broke on `deny_unknown_fields`**

```bash
cargo build --workspace --all-targets 2>&1 | tail -5
```

- [ ] **Step 7: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 111 task 7: expected_trailers rule on Driver::Http2, compared via diff_headers"
```

---

### Task 8: `{{HTTP2_TRAILERS_BACKEND_PORT}}` fixture-token plumbing

**Files:**
- Modify: `tests/differential/src/lib.rs`

**Interfaces:**
- Consumes: Task 5's `Http2TrailersBackend`.
- Produces: the `{{HTTP2_TRAILERS_BACKEND_PORT}}` template token, usable from any fixture's `envoy.yaml` / `envoy-rust.yaml`.

A fixture reaches a backend by TOKEN, not by driver — the harness scans the config templates for markers and spawns only what is needed. The exact precedent is `H2_CLOSE_BACKEND_PORT` (phase 64); copy its four-site shape.

- [ ] **Step 1: Add the scan + spawn + port binding**

Next to the existing `needs_h2_close_backend` block:

```rust
    // Phase 111: the trailer-emitting upstream for fixture 0090. Distinct from
    // {{HTTP2_BACKEND_PORT}} (the plain echo backend) — the trailer block is a
    // spawn-time mode on the same helper binary.
    let needs_h2_trailers_backend =
        scan_needs_marker(&backend_scan_sources, "HTTP2_TRAILERS_BACKEND_PORT");
    let _h2_trailers_backend: Option<crate::backend::Http2TrailersBackend> =
        if needs_h2_trailers_backend {
            Some(
                crate::backend::Http2TrailersBackend::spawn()
                    .await
                    .context("spawning Http2TrailersBackend")?,
            )
        } else {
            None
        };
    let h2_trailers_backend_port_str = _h2_trailers_backend.as_ref().map(|b| b.port().to_string());
```

**The binding order matters** — the `_h2_trailers_backend` binding is the process keep-alive; dropping it kills the child. Place it with its siblings, not later.

- [ ] **Step 2: Add the FOUR substitution sites**

There are two kv-push blocks (upstream side and subject side), each followed by a `.is_some()` guard chain that decides whether `BACKEND_HOST` is pushed. **All four need the new token** — locate each by the neighbouring `H2_CLOSE_BACKEND_PORT` text:

```rust
        // Phase 111: the trailer-emitting backend port.
        if let Some(tp) = h2_trailers_backend_port_str.as_deref() {
            v.push(("HTTP2_TRAILERS_BACKEND_PORT", tp.to_string()));
        }
```

and in each guard chain:

```rust
            || h2_trailers_backend_port_str.is_some()
```

**Missing either guard is the silent failure mode**: the port token renders but `{{BACKEND_HOST}}` does not, and the fixture fails with an unsubstituted token reaching the config parser rather than with a trailer mismatch.

- [ ] **Step 3: Verify the plumbing compiles and the suite is unaffected**

```bash
cargo build --workspace --all-targets 2>&1 | tail -5
cargo test -p differential --lib 2>&1 | tee /tmp/t8.txt
grep -E 'test result' /tmp/t8.txt
```

There is no unit test for the token itself — it is exercised for the first time by fixture `0090` in Task 9, which is its real test.

- [ ] **Step 4: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 111 task 8: {{HTTP2_TRAILERS_BACKEND_PORT}} fixture-token plumbing"
```

---

### Task 9: Fixture `0090-h2-response-trailers` — the differential witness

**Files:**
- Create: `tests/fixtures/0090-h2-response-trailers/envoy.yaml`
- Create: `tests/fixtures/0090-h2-response-trailers/envoy-rust.yaml`
- Create: `tests/fixtures/0090-h2-response-trailers/expectations.yaml`
- Create: `tests/fixtures/0090-h2-response-trailers/README.md`
- Create: `tests/differential/tests/h2_response_trailers.rs`

**Interfaces:**
- Consumes: Tasks 5–8 (backend, driver, rule, token) and Tasks 1–4 (the forwarding itself).
- Produces: fixture `0090`, green cross-proxy. Fixture count 89 → 90; differential test binaries 166 → **167**.

**No `inputs/` directory** — the H2 driver does not read one, and the newest fixture (`0089`) has none either.

- [ ] **Step 1: Create `envoy.yaml`**

Copied from `tests/fixtures/0010-http2-router-upstream/envoy.yaml` with ONLY the cluster's backend token changed. **`generate_request_id: false` and the six-entry `request_headers_to_remove` list are load-bearing** (D-PLAN-7): they are what make the echoed body byte-equal across proxies.

```yaml
# Phase 111 fixture 0090 — HTTP/2 response TRAILER forwarding (H2C end-to-end).
# Topology copied verbatim from fixture 0010 (H2 listener x H2 upstream); the
# ONLY delta is the cluster endpoint, which points at the trailer-emitting
# backend mode ({{HTTP2_TRAILERS_BACKEND_PORT}}) instead of the plain echo one.
#
# The backend answers 200 + `trailer: x-trail-a` (the RFC 7230 4.4 announce
# header, naming ONE of two) + the deterministic echo body + a trailer block of
# `x-trail-a: alpha` AND `x-trail-b: beta`. Upstream Envoy forwards BOTH, so
# the rule under test is "forward the block", not "forward what was announced".
#
# generate_request_id + request_headers_to_remove are inherited from 0010 and
# are load-bearing: they keep the echoed body byte-equal across both proxies.

node: { id: envoy-rust-phase-111-fixture-0090, cluster: envoy-rust-phase-111 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http2_listener
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http2
                codec_type: HTTP2
                generate_request_id: false
                route_config:
                  name: local_route
                  request_headers_to_remove:
                    - x-forwarded-for
                    - x-forwarded-proto
                    - x-request-id
                    - x-envoy-expected-rq-timeout-ms
                    - x-envoy-internal
                    - x-envoy-external-address
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      dns_lookup_family: V4_ONLY
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP2_TRAILERS_BACKEND_PORT}} } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
```

- [ ] **Step 2: Create `envoy-rust.yaml`**

```yaml
# envoy-rust per-side divergences from envoy.yaml (carried over verbatim from
# fixture 0010, which records this same list):
#   - bind 127.0.0.1 instead of 0.0.0.0 (no Docker indirection).
#   - no admin block.
#   - request_headers_to_remove omitted (envoy-rust does not inject these).
#   - generate_request_id omitted (envoy-rust does not inject x-request-id).
#   - dns_lookup_family omitted (envoy-rust ignores the field at runtime per
#     05.4 D2 — only the upstream-Envoy side observes V4_ONLY).
#
# NO trailer-related config on either side: upstream Envoy forwards H2 response
# trailers with no knob at all (phase 111, ADR-0181 DECISION 3 / PV-1), so this
# phase adds ZERO new config surface.

node: { id: envoy-rust-phase-111-fixture-0090, cluster: envoy-rust-phase-111 }
static_resources:
  listeners:
    - name: http2_listener
      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http2
                codec_type: HTTP2
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP2_TRAILERS_BACKEND_PORT}} } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
```

- [ ] **Step 3: Create `expectations.yaml`**

```yaml
driver:
  kind: http2
  method: get
  path: /
  host: envoy-rust.test
  expected_status: 200
  expected_headers: set_equal_modulo_allow_list
  expected_trailers: set_equal_modulo_allow_list

equivalence:
  response_status: exact
  response_body:
    kind: byte_exact
```

**Deliberately no per-driver `expected_body`.** Fixture `0010` pins the echo body byte-for-byte in its own `expected_body`; here the cross-proxy `equivalence.response_body: byte_exact` is what matters, and hard-coding the echoed request shape a second time would make this fixture fail on any unrelated request-header change. If the state-3 session wants the stronger pin, it must take the body string from an ACTUAL run, never from this plan.

- [ ] **Step 4: Create `README.md`**

Model it on `tests/fixtures/0010-http2-router-upstream/README.md`. It must state: the surface under test (H2 response trailer forwarding, upstream → downstream); the backend mode and its exact trailer block; that the announce header names only one of the two trailers **on purpose**; the per-side config divergences; that this is the **first fixture ever to exercise the `Response trailers` row** of the equivalence matrix; and the cells deliberately NOT probed here with their carry-forward ids (CF-111-5 forbidden names, CF-111-6 pseudo-headers, CF-111-8 duplicate names, CF-111-7 stats, CF-111-9 order).

- [ ] **Step 5: Create the runner**

```rust
//! Docker-gated differential test for fixture 0090-h2-response-trailers.
//!
//! Phase 111: HTTP/2 response TRAILER forwarding, upstream → downstream. The
//! trailer-emitting backend answers with one ANNOUNCED trailer
//! (`x-trail-a`, named in the `trailer:` response header) and one
//! UNANNOUNCED trailer (`x-trail-b`); upstream Envoy forwards both, so this
//! fixture witnesses "forward the block", not "forward what was announced".
//!
//! First exercise of the `Response trailers` row of the equivalence matrix in
//! `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, unwitnessed since phase 00 seeded it.

use std::path::PathBuf;

#[tokio::test]
async fn h2_response_trailers() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0090-h2-response-trailers");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 6: Run the fixture and verify it is GREEN**

```bash
cargo build -p envoy-bin          # the harness runs the DEBUG binary; a stale one REDs with `unknown field`
cargo test -p differential --test h2_response_trailers -- --nocapture 2>&1 | tee /tmp/t9.txt
grep -E 'test result' /tmp/t9.txt
```

Expected `test result: ok. 1 passed`. **Assert the count** — `0 passed; 1 filtered out` is a false green.

If it goes RED, before diagnosing anything else confirm the divergence is about TRAILERS: `grep -i 'trailer' /tmp/t9.txt`. A failure naming `BACKEND_HOST`, an unsubstituted `{{...}}` token, or `unknown field` is a Task 8 plumbing bug or a stale binary, not a forwarding bug.

- [ ] **Step 7: Prove the fixture is not vacuous**

A fixture that passes because BOTH sides return zero trailers is worthless, and it is exactly what this fixture looked like before Task 3. Prove the assertion bites, in a scratch worktree:

```bash
git worktree add /tmp/mut-111-t9 HEAD
cd /tmp/mut-111-t9
grep -c 'trailers = None;' crates/envoy-http2/src/hcm.rs   # sanity: the Task-4 clear is still there
# Mutate the FORWARD, not the clear: make the emit seam drop the block.
grep -n 'if let Some(map) = trailer_map {' crates/envoy-http2/src/response.rs   # must be exactly 1 hit
sed -i 's/    if let Some(map) = trailer_map {/    if let Some(map) = None::<http::HeaderMap>.or(trailer_map).filter(|_| false) {/' crates/envoy-http2/src/response.rs
touch crates/envoy-http2/src/lib.rs
cargo build -p envoy-bin 2>&1 | grep -E 'Compiling envoy-http2|error' | head
cargo test -p differential --test h2_response_trailers 2>&1 | tee /tmp/t9-mut.txt
grep -E 'test result' /tmp/t9-mut.txt
cd - && git worktree remove --force /tmp/mut-111-t9
```

Expected: a `Compiling envoy-http2` line, a `test result:` line, and **FAILED**. A GREEN means the fixture cannot see the trailer block at all — the most likely cause is Task 6's `trailers()` read sitting AFTER `conn_handle.abort()`.

- [ ] **Step 8: Commit**

```bash
git add tests/fixtures/0090-h2-response-trailers/ tests/differential/tests/h2_response_trailers.rs
git commit -m "phase 111 task 9: differential fixture 0090-h2-response-trailers"
```

---

### Task 10: `BEHAVIOR_CONTRACT.md` gains a `## Response trailers` section

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md`

**Interfaces:**
- Consumes: everything measured in §1 of this plan.
- Produces: no code surface. **This task contributes 0 to the net-LoC budget** (the house metric excludes `docs/`), which is why it is last and cannot be traded away for size.

The file's equivalence matrix already carries the row `| Response trailers | Set-equal under the same allow-list discipline |`. Until this phase that row was an aspiration no fixture exercised. This section records the rule it now has, and — just as importantly — every cell that remains unmeasured.

- [ ] **Step 1: Locate the insertion point by TEXT**

```bash
grep -n 'Response trailers' docs/envoy-rust/BEHAVIOR_CONTRACT.md
grep -n '^## Header allow-list' docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

Place `## Response trailers` adjacent to `## Header allow-list`, since it reuses that allow-list. **The file is 4305 lines and exceeds the read limit — page it, do not read it whole.**

- [ ] **Step 2: Write the section**

It must state, each as a claim traceable to §1 of this plan:

1. **The forward rule (D5/PV-1):** the trailer block is forwarded verbatim, upstream → downstream, on HTTP/2 only. The `trailer:` announce header is NOT consulted — an unannounced trailer is forwarded too, measured on upstream Envoy v1.33.0.
2. **The announce header (F3/PV-1):** forwarded as an ordinary response header by both proxies; pre-existing parity, not a phase-111 behaviour.
3. **Comparison discipline (PV-4):** set-equality of lowercased trailer names plus value-exact match for names not on the 3-entry `HEADER_ALLOW_LIST`, i.e. literally `diff_headers`. Duplicate-name multiplicity is NOT compared (CF-111-8), and trailer ORDER is NOT compared (CF-111-9).
4. **Scope (D1):** HTTP/2 responses only. HTTP/1.1 trailers are unbuilt in both directions and blocked behind chunked response encoding (CF-111-2). REQUEST trailers are unbuilt (CF-111-3). Trailers bypass the filter pipeline entirely (CF-111-1). Locally-generated replies carry no trailers (D-PLAN-5).
5. **Measured upstream behaviours that envoy-rust does NOT match** — stated plainly rather than omitted:
   - a trailer block containing `connection` / `transfer-encoding` / `upgrade` / `keep-alive` / `proxy-connection` / `te` ≠ `trailers`: Envoy returns `200` + body + `RST_STREAM(NO_ERROR)` with the block dropped; envoy-rust returns `503` (CF-111-5, pre-existing, inside the `h2` codec);
   - a trailer block containing a pseudo-header: Envoy drops the block and resets; envoy-rust forwards the surviving fields (CF-111-6).
6. **Measured upstream behaviours that ARE matched, recorded so nobody re-derives them:** `content-length`, `te: trailers` and `host` are forwarded verbatim in a trailer block; trailers are forwarded on a non-200 response; trailers are forwarded on an EMPTY-body response; an empty trailer HEADERS block yields no trailers and no error.
7. **Stats (CF-111-7):** upstream Envoy exposes `http2.trailers` and `cluster.<name>.http2.trailers`; both stayed `0` across eight trailer-forwarding responses. No stat parity is asserted.
8. **Still unmeasured** (carried from `SPEC.md` §8): trailers over TLS; trailers on retried or router-short-circuited responses; trailers on locally-generated replies upstream; whether `h2`'s validation ever turns a divergence into an error on a block Envoy would accept.

- [ ] **Step 3: Verify the contract and the code agree**

The contract says the allow-list is the 3-entry one; confirm the code still says so, because the two are meant to move in lockstep:

```bash
grep -n 'HEADER_ALLOW_LIST' -A 6 tests/differential/src/lib.rs | head -12
```

Expected: exactly `server`, `date`, `x-envoy-upstream-service-time`. **If this phase somehow changed it, that violates a Global Constraint — stop.**

- [ ] **Step 4: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 111 task 10: BEHAVIOR_CONTRACT gains the Response trailers section"
```

---

## §8. Self-review

Run against `SPEC.md` with fresh eyes after the plan was complete.

**Spec coverage.** Every SPEC section maps to a task:

| SPEC element | task |
|---|---|
| D1 (H2 only, response direction) | scope of Tasks 1–3 |
| D2 (read site after the body drain) | Task 2 |
| D3 (alongside, never a field) | D-PLAN-2; Tasks 2, 3 |
| D4 (emit site, end-of-stream) | Task 1 |
| D5 (forward verbatim, ignore the announce header) | Task 1; witnessed by Task 9 |
| D6 (no filter involvement) | honoured by omission; pinned by Task 4 |
| D7 (harness: extend, do not rebuild) | Tasks 5, 6, 7, 8 |
| D8 (fixture `0090` modelled on `0010`) | Task 9 |
| §4 differential surface | Task 9 |
| §4 `BEHAVIOR_CONTRACT.md` section | Task 10 |
| §4 conformance / fuzzing | no new target needed — `h2` owns the trailer framing and no parser, codec or filter is introduced (§7.4). `h2spec` is unchanged and `known-failures.txt` untrimmed. **If the state-3 session concludes a fuzz target IS needed, it must add the `ci.yml` step in the same task — fuzz targets are not auto-discovered.** |
| §5 non-goals 1–9 | Global Constraints; none is entered |
| §6 CF-111-1…4 | carried unconsumed (§3) |
| §7 PV-1…PV-8 | §1, all eight worked |
| §9 estimate + gate | §6, re-derived bottom-up rather than inherited |
| §10 definition of done | §9 below |

**Placeholder scan.** No `TBD`, no `implement later`, no "add appropriate error handling", no "similar to Task N", and no step that says what to do without showing how. Three places tell the implementer to READ an existing in-tree helper rather than reproducing it here (Task 2's `Request` literal, Task 3's two HCM test helpers, Task 4's `StopAndSend` filter fixture) — that is deliberate and is the opposite of a placeholder: those shapes are long, they drift, and a stale copy in this plan would be worse than a pointer. Each says exactly which existing item to mirror and what to do if it does not exist.

**Type consistency.** The trailer type is `Option<Vec<(String, String)>>` at every production hop (Tasks 1, 2, 3, 4) and `Vec<(String, String)>` (empty, not `Option`) in the harness result type (Task 6) — the asymmetry is intentional: production must distinguish "no trailer block" from "an empty one" to pick the end-of-stream branch, while the harness only ever compares sets. `build_trailer_map` is named consistently in Task 1's implementation and its doc comment. `Http1TrailerRule::SetEqualModuloAllowList` is spelled identically in Task 7's type, its dispatch, and Task 9's `expectations.yaml`. `Http2TrailersBackend` is spelled identically in Tasks 5, 6 and 8.

**Two gaps found and fixed inline while reviewing:** (i) the `drive_http1` struct literal must also gain the new field or Task 6 will not compile — now called out explicitly; (ii) Task 8's `.is_some()` guard chains appear TWICE, once per proxy side, and missing one produces an unsubstituted-token failure rather than a trailer failure — now called out with its symptom.

---

## §9. Definition of done — the §7.5 gate, instantiated

- **(a)** `0090-h2-response-trailers` green cross-proxy.
- **(b)** all **89** pre-existing fixtures still green. The regression risk is Task 1's end-of-stream fork; both no-trailer branches are pinned by unit tests and by fixture `0010` itself.
- **(c)** `h2spec` passes at its declared threshold with `known-failures.txt` **untrimmed**. Locally the h2spec gate can self-skip silently — a local green needs `--nocapture`; CI is authoritative.
- **(d)** no new fuzz target expected (§7.4); if one is added, its `ci.yml` step lands with it.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

**Expected CI movement at state 4:** test binaries **166 → 167** (the new `h2_response_trailers.rs` runner is auto-discovered), and the `passed` identity rises by the new tests. A docs-only commit must not move it; this phase's code commits must.

---

## §10. Next state

**§5 state 3 — implementation**, in a SEPARATE session (§5.1; ADR-0127: the context that wrote this plan must not execute it unreviewed). That session runs `superpowers:executing-plans` or `superpowers:subagent-driven-development`, does TDD per task, and appends to `PROGRESS.md` on each task completion.

**The §6.1 gate did NOT fire here, so there is no `111.1`/`111.2`.** State 3 operates on phase `111` whole.
