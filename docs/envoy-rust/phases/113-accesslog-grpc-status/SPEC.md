# Phase 113 — SPEC

**Observability family: the `%GRPC_STATUS%` access-log command-operator family** — `%GRPC_STATUS%`, its three parenthesized format arguments `%GRPC_STATUS(CAMEL_STRING|SNAKE_STRING|NUMBER)%`, and the no-arg alias `%GRPC_STATUS_NUMBER%` — sourced from the response `grpc-status` **header** and gated on the request being a gRPC request, over the HTTP/1.1 local-reply surface that phase 110 built. Consumes **CF-111-4** in part. Witnessed by NEW cluster-free, backend-free differential fixture `0093-accesslog-grpc-status` through the **EXISTING** `Driver::Http1AccessLogByteExact` — zero new driver, zero new dependency, zero new config surface, zero new fuzz target.

- **Phase id:** `113`
- **Directory:** `docs/envoy-rust/phases/113-accesslog-grpc-status/`
- **Depends on:** `06` (the access-log subsystem), `32` (the command-operator engine), `110` (the H1 gRPC local-reply transform)
- **State at this commit:** §5 state-0/1 complete — this file exists, **`PLAN.md` does NOT**. The next session runs §5 state 2 (`superpowers:writing-plans`).
- **Scoping ADR:** `ADR-0192`.

---

## 1. Context — what a stranger needs to know

This repository is a from-scratch reimplementation of the Envoy Proxy in Rust, verified against upstream Envoy **v1.33.0** (image `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) by a differential test harness in `tests/differential/`. Read `BOOTSTRAP_PROMPT.md` §7 for the differential contract and `docs/envoy-rust/BEHAVIOR_CONTRACT.md` for the equivalence rules.

Two landed phases make this one cheap, and neither fact should be re-derived from scratch:

**Phase 32 built the access-log command-operator engine.** `crates/envoy-accesslog/src/command_operator.rs` parses a `text_format` string into `Vec<Segment>` and renders it against an `AccessLogRecord`. Today the engine knows **14** operator keywords: three argument-taking (`%REQ(...)%`, `%RESP(...)%`, `%DYNAMIC_METADATA(ns:key)%`, dispatched at `command_operator.rs:245-247`) and eleven no-arg (the `no_arg_op` table at `command_operator.rs:269-283`, which the code's own doc calls *"The SINGLE source of truth for the no-arg keyword set"*). Anything else is `FormatParseError::UnknownKeyword` and is **boot-fatal**.

The engine carries a stated architectural invariant, at `command_operator.rs:6-8`: *"ONLY the operators with a backing field on `AccessLogRecord` are accepted."* It is enforced mechanically — `parse_header_op` checks the requested header name against `REQ_ALLOW_LIST` (`:92`, 7 entries) or `RESP_ALLOW_LIST` (`:104`, exactly one entry, `x-envoy-upstream-service-time`) and rejects anything else with `FormatParseError::UnsupportedHeader`. `AccessLogRecord` (`crates/envoy-accesslog/src/record.rs:30`, 19 fields) holds **no header map at all**, request or response; every header-derived operator reads a *distilled* field. §5 non-goal 1 turns on this invariant.

**Phase 110 built the HTTP/1.1 gRPC-aware local-reply transform.** `crates/envoy-http1/src/grpc.rs` holds four `pub(crate)` functions: `is_grpc_request` (`:45`), `http_to_grpc_status` (`:64`), `grpc_message_encode` (`:89`) and `apply_grpc_local_reply` (`:140`). When a locally generated H1 reply answers a request whose `content-type` is gRPC, upstream Envoy rewrites the status to `200`, sets `content-type: application/grpc`, drops the body, and adds `grpc-status` (and `grpc-message` when the body was non-empty) as ordinary **response headers**. envoy-rust reproduces this; fixture `0089-grpc-aware-local-replies` is its differential witness. The mapping table and the detection rule are banked in `ADR-0177` DECISION 3 and in `BEHAVIOR_CONTRACT.md`'s `## gRPC` section.

**What is missing.** envoy-rust emits the `grpc-status` header but cannot *log* it. There is no `%GRPC_STATUS%` operator, no `%GRPC_STATUS_NUMBER%`, and no gRPC awareness in the access-log crate at all.

---

## 2. The divergence — MEASURED on BOTH proxies at the state-0/1 pick

All measurements below were taken by the pick session directly against the pinned image, container ownership proved by `docker inspect <cid> --format '{{.Image}}'` returning the pinned digest, on host ports asserted free by a `/dev/tcp` pre-flight before binding.

### 2.1 Config acceptance — `--mode validate`, with a negative control

| access-log `text_format` token | upstream Envoy v1.33.0 |
|---|---|
| `%GRPC_STATUS%` | `configuration ... OK` |
| `%GRPC_STATUS_NUMBER%` | `configuration ... OK` |
| `%GRPC_STATUS(CAMEL_STRING)%` | `configuration ... OK` |
| `%TRAILER(grpc-status)%` | `configuration ... OK` |
| `%RESPONSE_CODE%` *(positive control)* | `configuration ... OK` |
| `%NOT_A_REAL_TOKEN_XYZ%` *(negative control)* | **REJECTED** — `error initializing configuration ...: Not supported field in StreamInfo: NOT_A_REAL_TOKEN_XYZ` |

The negative control is what makes this non-vacuous: `--mode validate` genuinely parses the format string rather than accepting any text. Note `--mode validate` is **upstream-only** — `envoy-bin` has no such flag and exits 2 on it.

envoy-rust is **boot-fatal** on each of the three in-scope tokens (`FormatParseError::UnknownKeyword`). The divergence is therefore **total and boot-level**, not a value divergence: every behavioural cell below is unreachable on envoy-rust today, so the fixture's RED-before / GREEN-after is unambiguous and cannot pass vacuously.

Absence census, re-derived at the pick: `grep -rn 'GRPC_STATUS' crates/envoy-accesslog/src/` = **0** and `grep -rn 'TRAILER' crates/envoy-accesslog/src/` = **0**, against positive controls `RESPONSE_CODE` = **42**, `RESPONSE_CODE_DETAILS` = **16** and `DYNAMIC_METADATA` = **20** in the same directory with the same grep form.

### 2.2 The gating rule — the finding that governs the whole design

`%GRPC_STATUS%` is **gated on the REQUEST being a gRPC request**. It does *not* simply read the response header. Driven against a `direct_response` route carrying an explicit `grpc-status: 5` response header, with `%RESP(grpc-status)%` in the same log line as a witness that the header really is present:

| request `content-type` | `%GRPC_STATUS%` | `%RESP(grpc-status)%` |
|---|---|---|
| *(none — plain request)* | `-` | `5` |
| `application/grpc` | `NotFound` | `5` |
| `application/grpc+proto` | `NotFound` | `5` |
| `application/grpc; charset=utf-8` | `-` | `5` |
| `application/grpc-web` | `-` | `5` |
| `APPLICATION/GRPC` | `-` | `5` |

The `%RESP(grpc-status)%` column is `5` on **every** row, so the header is present throughout and the `-` rows are the gate firing, not a missing value. **This gate is exactly the predicate `crates/envoy-http1/src/grpc.rs:45 is_grpc_request` already implements** — `content-type` exactly `application/grpc` or beginning `application/grpc+`, case-sensitive, a parameter defeats it. A reviewer should note that this is a *measured* reuse, not an assumed one: the four negative spellings above are precisely the four traps `ADR-0177` DECISION 3(b) records for that function.

### 2.3 The rendering matrix — MEASURED, and to be banked verbatim

Driven with `direct_response` routes at each status and `content-type: application/grpc`, i.e. exercising the phase-110 mapping table through the access log. `%RESPONSE_CODE%` reads `200` on every row because the gRPC transform rewrites the status.

| `direct_response.status` | `%RESPONSE_CODE%` | `%GRPC_STATUS%` | `%GRPC_STATUS(SNAKE_STRING)%` | `%GRPC_STATUS_NUMBER%` |
|---|---|---|---|---|
| 200 | 200 | `Unknown` | `UNKNOWN` | `2` |
| 400 | 200 | `Internal` | `INTERNAL` | `13` |
| 401 | 200 | `Unauthenticated` | `UNAUTHENTICATED` | `16` |
| 403 | 200 | `PermissionDenied` | `PERMISSION_DENIED` | `7` |
| 404 | 200 | `Unimplemented` | `UNIMPLEMENTED` | `12` |
| 429 | 200 | `Unavailable` | `UNAVAILABLE` | `14` |
| 500 | 200 | `Unknown` | `UNKNOWN` | `2` |
| 501 | 200 | `Unknown` | `UNKNOWN` | `2` |
| 502 | 200 | `Unavailable` | `UNAVAILABLE` | `14` |
| 503 | 200 | `Unavailable` | `UNAVAILABLE` | `14` |
| 504 | 200 | `Unavailable` | `UNAVAILABLE` | `14` |

Three further cells, measured on an explicit `grpc-status` response header:

| response `grpc-status` header value | `%GRPC_STATUS%` | `%GRPC_STATUS(SNAKE_STRING)%` | `%GRPC_STATUS_NUMBER%` |
|---|---|---|---|
| `5` | `NotFound` | `NOT_FOUND` | `5` |
| `13` | `Internal` | `INTERNAL` | `13` |
| `99` *(numeric, outside the enum)* | `99` | `99` | `99` |
| `notanumber` *(unparseable)* | `-1` | `-1` | `-1` |

Four properties a naive implementation gets wrong, each stated because it is the discriminating case:

1. **The default format is `CAMEL_STRING`.** `%GRPC_STATUS%` with no argument renders `NotFound`, not `5` and not `NOT_FOUND`.
2. **`%GRPC_STATUS_NUMBER%` and `%GRPC_STATUS(NUMBER)%` agree** — both render the integer.
3. **An out-of-enum numeric value falls back to the NUMBER in every format**, including `CAMEL_STRING` and `SNAKE_STRING`. It is not `-`, not `Unknown`, and not an error.
4. **An unparseable value renders `-1` in every format** — the literal two characters, not `-` (absent) and not the raw string.

**This table independently corroborates the phase-110 mapping table through a different observable.** `ADR-0177` DECISION 3(a) recorded the HTTP→gRPC map as the sparse eight-entry set `400→13, 401→16, 403→7, 404→12, 429→14, 502→14, 503→14, 504→14` over a default of `2`, measured on the response *header*. Row-for-row, the access-log rendering above reproduces it exactly, including the counter-intuitive `500→2` and `501→2`. That is corroboration, not merely agreement, because the two measurements read different surfaces.

### 2.4 A cell that is structurally unreachable on this surface, and why that is fine

"A gRPC request whose response carries **no** `grpc-status` at all" was **not** measured, because on the local-reply surface it cannot arise: upstream Envoy's transform always adds the header, and so does envoy-rust's. The PLAN-write must not invent a rendering for it from reasoning; if a code path could reach it, that path is out of scope (see §5 non-goal 3) and the implementation should render absent (`-`), which is the engine's existing convention for an unpopulated `Option` field.

---

## 3. Why this phase, and what it does not claim

The pick was made against the unbuilt-leaf inventory with four read-only recon subagents and the two-proxy measurements above; `ADR-0192` records the full options set with the measurement that killed each. Three points belong here because a cold-starting reader will otherwise mis-read the choice.

**This is the runner-up `ADR-0183` named, taken for the reason `ADR-0183` predicted.** That ADR rejected these tokens *"only on leverage: it advances leg (ii) alone, where option (f) advances (ii) AND (iii)"*, and called them *"the strongest candidate for phase 113."* The leverage argument has since dissolved: stop-condition leg (iii) now has exactly **one** zero-row family left (`### WASM host family`), and `ADR-0183` option (e) measured that one unreachable without a `MISSION.md` amendment and a mandatory three-way split. No candidate available this session can move leg (iii), so leg (iii) cannot discriminate, and the ranking falls through to cost and witness quality — where this candidate wins.

**The inherited obstacle does not reach this slice, and that was re-derived rather than trusted.** `ADR-0183` recorded, correctly, that *"the trailer block is NOT live at the access-log record build"* — `finalize_h2_stream` moves `trailers` into `send_envoy_response` at `crates/envoy-http2/src/hcm.rs:1096` and builds the `AccessLogRecord` 62 lines later at `:1158`. Both line numbers were re-derived at this pick and both are correct at this commit. That obstacle is **entirely an HTTP/2 obstacle**, and this phase is HTTP/1.1-only, so it is not paid here — it is banked as **CF-113-2** together with the trailer source it blocks.

**On the H1 path the seam already exists and needs no new plumbing.** This is the measurement that sizes the phase, and it should be re-verified before the PLAN-write rather than inherited. In `crates/envoy-http1/src/hcm.rs`, inside `serve_connection` (declared `:750`):

- `:1491` — `crate::grpc::apply_grpc_local_reply(&mut outgoing, &req.headers);` applies the phase-110 transform.
- `:1545` — `let record = build_access_log_record(` builds the record, passing `req: &req` in `AccessLogRequestInfo` and `headers: &outgoing.headers` in `AccessLogResponseInfo` (`:1633`).
- Between the two, `outgoing` is **read only, never re-assigned** — the code's own comment at `:1497-1501` states that `outgoing` *"stays owned and alive through the access-log block below"* precisely so the record build can borrow `outgoing.headers`.

So both inputs this phase needs — the **request** headers for the gate and the **post-transform response** headers for the value — are already live, already correctly ordered, and already passed to the record builder. The H1 work is an extraction at the record build, not a threading change. Contrast phase 42 (`%RESPONSE_CODE_DETAILS%`), the closest-shaped precedent, which spent 87 lines on H1 threading and 109 on H2 threading because its value had to be carried from the point of origin.

---

## 4. In scope — the deliverables

1. **`GrpcStatusFormat`** — a small enum in `crates/envoy-accesslog/src/command_operator.rs` with variants `CamelString` (the default), `SnakeString`, `Number`.
2. **`Op::GrpcStatus { format: GrpcStatusFormat }`** and **`Op::GrpcStatusNumber`** — two new `Op` variants (taking the enum from 14 variants to 16). Both renderers (`render_op`, `encode_single_op`) are exhaustive `match`es, so the compiler forces both arms.
3. **Parsing.** A new `"GRPC_STATUS"` arm in `parse_operator`'s `match keyword` (`command_operator.rs:244-249`), alongside `REQ`/`RESP`/`DYNAMIC_METADATA`. It is the engine's **first operator with an OPTIONAL parenthesized argument**: `parse_operator` splits on the first `(` and hands `rest: Option<&str>`, so the arm must accept `None` (⇒ `CamelString`) and `Some("(CAMEL_STRING)")` / `Some("(SNAKE_STRING)")` / `Some("(NUMBER)")`, and reject any other argument. `"GRPC_STATUS_NUMBER"` is a plain one-line addition to the `no_arg_op` table; keyword matching is exact-string so the two do not collide.
4. **The gRPC status-code name table** — the canonical gRPC code→name mapping in both `CamelString` and `SnakeString` spellings, plus the two fallbacks measured in §2.3 (out-of-enum numeric ⇒ the number; unparseable ⇒ `-1`).
5. **`AccessLogRecord.grpc_status: Option<String>`** — ONE new field holding the **raw wire value** of the response `grpc-status` header, populated **only** when the request was gRPC-detected and `None` otherwise. The single field carries both the gate and the value, so the renderer stays a pure function of the record and the engine's "backed by a distilled field" invariant is preserved rather than breached. Storing the raw string (not a parsed integer) is what lets the renderer reproduce the `99` and `-1` cells.
6. **The H1 population site** — an extract at `crates/envoy-http1/src/hcm.rs:1545`'s `build_access_log_record`, gated on the request headers and reading the post-transform response headers, per §3.
7. **The H2 population site** — `grpc_status: None`, with a comment naming the boundary (§5 non-goal 3) and an in-process pin asserting it, so a later phase that lifts the boundary has to delete an explicit test rather than silently change behaviour.
8. **JSON encoder support** — `crates/envoy-accesslog/src/json_format.rs` `encode_single_op` arms. The PLAN-write must decide, and record, whether `%GRPC_STATUS_NUMBER%` takes the *typed* (unquoted-number) single-operator carve-out documented at `json_format.rs:203`; this is PLAN-VERIFY item **PV-4**.
9. **Fixture `0093-accesslog-grpc-status`** — NEW, **cluster-free and backend-free**, through the EXISTING `Driver::Http1AccessLogByteExact`. See §6.
10. **Fuzz corpus seeds** for the EXISTING `accesslog_format_parse` target (`crates/envoy-accesslog/fuzz/fuzz_targets/accesslog_format_parse.rs`), covering the new keywords and their malformed-argument forms.
11. **`BEHAVIOR_CONTRACT.md`** — extend the `## gRPC` section that phase 110.2 created with the §2.2 gate and the §2.3 matrix.

---

## 5. Non-goals — rejected fail-loud, each with the reason

1. **`%TRAILER(name)%` is OUT.** It is the other half of `CF-111-4` and it is deliberately not taken, on an architectural ground rather than a size one: a general trailer-name operator **breaks the engine's stated invariant** (`command_operator.rs:6-8`) that only operators with a backing distilled field are accepted. Serving it properly requires a full trailer *map* on `AccessLogRecord` — the thing the record has consciously never had for headers either — and that is a design change to the record's contract, not an added arm. It also cannot be witnessed on HTTP/1.1 at all (§5 non-goal 4). Banked as **CF-113-1**. `CF-111-4` is therefore **partially** consumed by this phase, and the PLAN-write and REVIEW must both say so rather than recording it closed.
2. **The HTTP/2 arm is OUT** — both the H2 trailer source for `%GRPC_STATUS%` and the `hcm.rs:1096`→`:1158` ownership fix it needs. Banked as **CF-113-2**. Note this is not merely a plumbing deferral: `CF-110-1` records that H2 has no gRPC local-reply transform at all (H2 owns a separate `synth_h2_*` generator family), so the H2 arm is a *different and larger* surface than the H1 arm, not a mirror of it.
3. **HTTP/1.1 PROXIED gRPC responses are OUT.** A real upstream that returns `grpc-status` as a response *header* would be read correctly by this implementation as a side effect; one that returns it in HTTP/1.1 chunked *trailers* would not, because `crates/envoy-http1/src/client.rs:588` discards trailers. That gap is pre-existing and already banked as **CF-111-2** (H1 response trailers are blocked behind chunked response *encoding*, a ~153-site change). This phase neither closes nor widens it, and the fixture does not probe it. Banked as **CF-113-3** so the interaction is recorded, not merely implied.
4. **`grpc_status_filter`, the 7th `AccessLogFilter` oneof arm, is OUT.** `ADR-0154` DECISION 7 rejected it as vacuous *"because it reads the gRPC response TRAILER status, which envoy-rust has no data path for"*, and `ADR-0183` flagged that premise as now stale. It is a genuine and now-adjacent follow-on, but it is a *filter* (a log-emission predicate) and this phase is a *formatter* — a different subsystem with its own config surface, its own validator and its own fixture. Banked as **CF-113-4**. ⚠ A future session must **re-test that rejection's premise against its own slice** rather than inheriting either the rejection or this note.
5. **No new config surface, no new dependency, no new harness driver, no new fuzz target, no new workspace crate.** `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml` and `tests/differential/src/lib.rs` are all expected to be untouched. §7.4's *"parser, codec, or filter"* fuzz trigger is satisfied by the pre-existing `accesslog_format_parse` target, which already covers the format-string parser this phase extends; adding a target would need a `ci.yml` step, and adding neither is the correct outcome here. **If the PLAN-write finds it must touch any of those files, that is a signal the scope drifted — stop and re-scope.**
6. **Nothing is fixed.** Per §6.3 and `ADR-0165`, this phase consumes no unrelated carry-forward. In particular the phase-112 ALPN cleanup set (**CF-112-13**, whose blast radius is **five** internally-tagged unit variants and not the four its REVIEW names — the fifth is `HeaderRule::SetEqualModuloAllowList` at `tests/differential/src/lib.rs:1162`; plus **CF-112-14**, **CF-112-8**, **CF-112-9**, **CF-112-16**) is **NOT** taken here. It was measured at this pick at **≈134 net lines, ~9% of the §6.1 LoC gate** — it is a *rider*, not a phase, and `ADR-0192` records that adjudication so a future pick does not re-cost it.

---

## 6. The differential witness — fixture `0093-accesslog-grpc-status`

**Numbering re-derived at the pick:** `tests/fixtures/` holds **92** directories, highest `0092-tls-alpn-server-preference`, so this phase's is `0093`.

**Driver: the EXISTING `Driver::Http1AccessLogByteExact`** (`tests/differential/src/lib.rs:170`), used by **30** landed fixtures. Its probe type `AccessLogByteExactProbe` (`:1183`) already carries `extra_headers: Vec<(String, String)>` — verified at the pick — which is exactly what lets a probe send `content-type: application/grpc`. Without that field this fixture would be impossible and the pick would have been dead. **No driver change, no harness change.**

**A hard fixture-design constraint, measured at the pick:** the fixture must **NOT** use route-level `response_headers_to_add`. envoy-rust has no such field — `Route` (`crates/envoy-config/src/bootstrap.rs:2295`) carries only `name` / `r#match` / `action` / `typed_per_filter_config`, and `response_headers_to_add` exists in the tree solely as a `LocalRateLimit` filter field (`:1713`). Under `deny_unknown_fields` a route-level use is **boot-fatal**, so a fixture written that way would go RED for a reason unrelated to gRPC. The `grpc-status` values must therefore be produced by the **phase-110 transform itself**, driven from `direct_response.status` — which is strictly better anyway, because it witnesses the landed mapping table through a new observable.

**Probe set** (each probe gets a DISTINCT path — identical paths cannot attribute a kept log line):

| probe | path | `content-type` | `direct_response.status` | expected `%GRPC_STATUS%` |
|---|---|---|---|---|
| 1 | `/g-ok` | `application/grpc` | 200 | `Unknown` |
| 2 | `/g-internal` | `application/grpc` | 400 | `Internal` |
| 3 | `/g-unauth` | `application/grpc` | 401 | `Unauthenticated` |
| 4 | `/g-denied` | `application/grpc` | 403 | `PermissionDenied` |
| 5 | `/g-unimpl` | `application/grpc` | 404 | `Unimplemented` |
| 6 | `/g-unavail` | `application/grpc` | 503 | `Unavailable` |
| 7 | `/g-default` | `application/grpc` | 501 | `Unknown` *(the counter-intuitive default cell)* |
| 8 | `/g-proto` | `application/grpc+proto` | 404 | `Unimplemented` *(the `+` suffix cell)* |
| 9 | `/g-param` | `application/grpc; charset=utf-8` | 404 | `-` *(parameter defeats the gate)* |
| 10 | `/g-web` | `application/grpc-web` | 404 | `-` *(not a gRPC content-type)* |
| 11 | `/g-upper` | `APPLICATION/GRPC` | 404 | `-` *(case-sensitive)* |
| 12 | `/g-plain` | *(none)* | 404 | `-` *(no gRPC content-type at all)* |

Probes 9–12 are the **negative controls**, and they are what stops the fixture passing vacuously: an implementation that ignores the gate and always reads the header would go RED on four probes. Probes 1–8 are the positive cells. `expected_status` is **200** on probes 1–8 (the transform rewrites the status) and the underlying status on 9–12; the PLAN-write must set each explicitly rather than relying on the `200` default.

The fixture's `text_format` should render all three spellings plus a `%RESPONSE_CODE%` anchor in one line, so a single byte-exact comparison covers the whole matrix.

**Local-verifiability.** Because the fixture is cluster-free and backend-free, it is verifiable on the development host, where backend-routing fixtures go RED against the `192.168.65.2` Docker bridge and CI is the only authority. Fixture `0088` is the cluster-free template (`clusters: []`, two byte-identical YAMLs, `{{PORT}}` the only token).

---

## 7. Size estimate and the §6.1 split gate

Bottom-up, net lines of change, **excluding `docs/`**:

| piece | est. |
|---|---|
| `GrpcStatusFormat` + two `Op` variants + the optional-argument parse arm + the `no_arg_op` entry | 110–150 |
| the code→name table (both spellings) + the out-of-enum and unparseable fallbacks | 90–130 |
| text `render_op` arms + JSON `encode_single_op` arms | 60–90 |
| `AccessLogRecord.grpc_status` + doc + the `E0063` sweep over **10** construction sites (2 production, 8 test) | 90–130 |
| the H1 extraction at the record build + in-process backstops | 70–110 |
| the H2 `None` + boundary comment + pin test | 30–50 |
| unit and mutation tests across parse / render / gate | 130–190 |
| fixture `0093` (4 files) + its runner test file | 230–280 |
| fuzz corpus seeds (existing target) | 5–15 |
| **code subtotal** | **≈815–1145** |
| `BEHAVIOR_CONTRACT.md` `## gRPC` extension *(docs — excluded from the gate)* | 90–120 |

**Central ≈ 980 code.** The §6.1 gate is **~25 tasks OR ~1500 net LoC**. On its face this clears it, and the task count (projected 8–10) is not close to the limit.

**But the calibration says do not trust that, and the PLAN-write owns the decision.** Landed-vs-projected ratios, re-measured at this pick with `git diff --numstat <state-2 commit> <state-4 gate commit> -- . ':(exclude)docs/'`:

| phase | projected | landed | ratio | how the estimate was made |
|---|---|---|---|---|
| `110.2` | 615 | **817** | **1.33×** | projected |
| `111` | 916 | **1525** | **1.66×** | projected |
| `112.1` | 551 | **549** | **1.00×** | **measured on a prototype** |
| `112.2` | 595 | **652** | **1.10×** | measured, then the plan drifted |

At the projected-estimate band this phase lands **1085–1900**; the top of that range **crosses the gate**. At the measured-estimate band it lands **815–1260** and clears it comfortably. The discriminator is method, not luck: `112.1` and `112.2` both prototyped their plans and both came in at ≈1.0×.

**Therefore: the §6.1 split is PROJECTED POSSIBLE, not unlikely, and `ADR-0193` is RESERVED-UNFIRED for it.** The state-2 PLAN-write **must** re-derive the estimate on a prototype in a scratch worktree with its own `CARGO_TARGET_DIR`, and must re-measure **after the final edit to the plan** — `ADR-0189`'s figure of 595 landed at 652 because the plan was edited after the measurement, so a measured estimate goes stale in its *method claim*, not only in its number. If the gate fires, the natural cut follows the `110.1`/`110.2` and `112.1`/`112.2` precedent exactly: **`113.1`** = the engine, the record field and the H1 seam, witnessed entirely in-process with no new fixture and regression-equivalence over all 92 existing fixtures; **`113.2`** = fixture `0093` + the `BEHAVIOR_CONTRACT.md` section + the parent close. Per §6.2 step 7 the PLAN-write **stops without writing a `PLAN.md`** if it splits.

---

## 8. PLAN-VERIFY items — what state 2 must measure, and why each is load-bearing

Each of these is a claim this SPEC either could not settle or settled in a way that a fresh measurement should confirm. The `110.2` and `112.1` PLAN-writes each discovered three divergences their landed SPECs did not contain; the point of this list is to make that discovery cheap rather than accidental.

- **PV-1 — Re-derive every `file:line` in this document.** `crates/envoy-http1/src/hcm.rs:1491` / `:1545` / `:1633`, `crates/envoy-http2/src/hcm.rs:1096` / `:1158`, `command_operator.rs:6-8` / `:92` / `:104` / `:244-249` / `:269-283`, `record.rs:30`, `grpc.rs:45` / `:64`, `bootstrap.rs:2295` / `:1713`, `tests/differential/src/lib.rs:170` / `:1183`. They were correct at HEAD when this SPEC was written; **any phase that edits `hcm.rs` moves them**, and a stale citation can land on a plausible wrong target. Locate by TEXT, and assert each anchor's occurrence count before trusting the line it returns.
- **PV-2 — Dry-run the exact fixture `0093` YAML against BOTH proxies end to end**, as `110.2` was required to. This is the single highest-value item: `ADR-0180` records that the equivalent dry-run found three divergences that would each have landed fixture `0089` RED for reasons unrelated to gRPC. In particular re-check `CF-110-6` (envoy-rust emits `content-type` on an empty-body local reply where upstream emits none) and `CF-110-7` (`direct_response.body` is mandatory in envoy-rust, optional upstream) — both bear directly on this fixture's YAML.
- **PV-3 — Confirm the §2.2 gate on envoy-rust's own predicate.** `is_grpc_request` is `pub(crate)` in `envoy-http1`, and `crates/envoy-http1/src/grpc.rs:14-19` argues *against* widening its reach. Decide and record whether the access-log extract calls it from within `envoy-http1` (preferred — the extract lives at the H1 record-build site, which is already inside that crate) or whether anything must become `pub`. **A visibility widening is a design change and needs saying out loud.**
- **PV-4 — The JSON typed carve-out.** Decide whether `%GRPC_STATUS_NUMBER%` renders as an unquoted JSON number under the single-operator typed path (`json_format.rs:203`) and whether `%GRPC_STATUS%` renders as a string. Measure upstream's `json_format` output rather than reasoning from the text format.
- **PV-5 — The canonical name table beyond the six measured codes.** §2.3 measures codes 2, 5, 7, 12, 13, 14 and 16. The gRPC canonical set has 17 codes (0–16). Measure the remaining spellings against upstream rather than recalling them — `ADR-0154` already records that the one-L `CANCELED` spelling is a live trap.
- **PV-6 — Whether `%GRPC_STATUS(...)%` accepts any further argument spellings** (lowercase? whitespace? an empty `()`?) and what upstream does with an invalid one. This decides the parse arm's reject set, and a validator built to a guessed rule manufactures a divergence — exactly the `ADR-0184` PV-3 failure mode, where a rule inferred rather than measured would have produced a rejection upstream does not make.
- **PV-7 — Re-measure the §7 estimate on a prototype** and re-measure again after the final edit to `PLAN.md` (§7).
- **PV-8 — Confirm `Cargo.toml` / `Cargo.lock` / `ci.yml` / `tests/differential/src/lib.rs` are untouched** by the finished plan (§5 non-goal 5).

---

## 9. Close-out clause

When the last unit of this phase reaches §5 state 6, the close-out flips this phase's ROADMAP row to `done`.

⚠ **Do not trust that sentence's row list — DERIVE the not-done set from `ROADMAP.md` at the close-out.** A close-out flips the **status cell only**, by splitting each row on `' | '`, asserting 6 cells and the expected current status, replacing that one cell and re-joining. This warning is not boilerplate: the equivalent clause in `112.2/SPEC.md` §9 named a row that had already flipped at its own earlier close-out, and so **mis-stated two consecutive close-outs**. If this phase splits (§7), the parent row flips only once every sub-phase row is `done`.

Rows 38 and 39 of `ROADMAP.md` carry unescaped in-cell pipes and split into 7 and 10 fields. They are append-only history and **must not be "fixed"**; any census must drive off `' | '` with spaces and take status at field **4**, never `NF == 6`.

---

## 10. Carry-forwards opened by this phase

- **CF-113-1** — `%TRAILER(name)%` is not implemented; `CF-111-4` is only **partially** consumed. Needs a trailer map on `AccessLogRecord`, which breaks the engine's backed-by-a-distilled-field invariant. (§5 non-goal 1.)
- **CF-113-2** — the HTTP/2 arm of `%GRPC_STATUS%` is absent, including the `crates/envoy-http2/src/hcm.rs:1096`→`:1158` ownership fix that would make the forwarded trailer block live at the record build. (§5 non-goal 2.)
- **CF-113-3** — HTTP/1.1 **proxied** gRPC responses are unwitnessed; a trailer-sourced `grpc-status` is unreadable behind the pre-existing `CF-111-2`. (§5 non-goal 3.)
- **CF-113-4** — `grpc_status_filter`, the 7th `AccessLogFilter` arm, remains unbuilt; `ADR-0154` DECISION 7's staleness is recorded but not acted on. (§5 non-goal 4.)

Every previously banked carry-forward carries forward **INTACT and UNCONSUMED** — the phase-112 family's CF-112-1…CF-112-19, the `112.1`/`112.2`/`111`/`110.x`/`109.x`/`108.2` REVIEW sets, CF-111-1…CF-111-9, CF-110-1…9, CF-109-1/2/3, CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6, CF-74-1/2/3/4/6, CF-73-1 and the HTTP-filters-family (1)–(4). **CF-112-5 stays CLOSED. CF-112-8 Consequence 2 stays BANKED as structurally unwitnessable by this harness — do not try to build it as a fixture.**
