# Phase 111 — gRPC-family PREREQUISITE: HTTP/2 response TRAILER forwarding (upstream → downstream), witnessed by NEW fixture `0090`

> **Status:** `SPEC.md` written at the §5 state-0/1 next-phase pick. No `PLAN.md`
> yet — that is §5 state 2 and a separate session (§5.1; ADR-0127).
> **Scoping ADR:** `ADR-0181` (landed alongside this SPEC).
> **Reserved-unfired:** `ADR-0182`, for a §6.1 split. See §9.

---

## §0. How to read this document

You are a fresh session with zero prior context. Everything you need is here or
behind a `file:line` citation you can resolve yourself.

Three reading rules, each earned by a prior phase's mistake:

1. **Every claim below is a claim, including this SPEC's own.** Re-verify on
   disk before you rely on it. Locate things by TEXT (`grep`), never by a line
   number you inherited — line numbers in this project drift constantly, and a
   SPEC's own citations have been measured stale before (`110.2/REVIEW.md` M-1
   found six non-resolving citations inside a single landed review).
2. **The measurements in §1 were taken at this session against the pinned image
   `envoyproxy/envoy:v1.33.0`** (digest `sha256:56da5afd7df3…`, per
   `docs/envoy-rust/ENVOY_TARGET.md`). They are real, not projected. §7 names
   what must be re-measured anyway at the state-2 PLAN-write, and §8 names what
   was deliberately NOT measured.
3. **This phase's central risk is scope creep into the two adjacent gaps it
   deliberately does not close.** §5 names them explicitly. If a task finds
   itself editing `crates/envoy-filter/` or teaching HTTP/1.1 to emit chunked
   responses, the scope is wrong — stop and re-read §5.

---

## §1. State-0 recon — the evidence this pick rests on

### §1.1 The divergence, MEASURED on BOTH proxies at this session

A scratch HTTP/2 backend was written for this probe (in the session scratchpad,
never in the repo tree — the tree stayed clean throughout, verified by
`git status --porcelain`). It replies to any request with:

- status `200`
- headers `content-type: text/plain` and `trailer: x-trail-a` (the RFC 7230
  §4.4 *announce* header, naming only ONE of the two trailers it will send)
- one DATA frame carrying `BODY-OK`
- a trailer block: `x-trail-a: alpha` and `x-trail-b: beta` — one announced,
  one NOT announced.

The same H2 client then drove one `GET` through each proxy, each configured
with an HTTP/2 downstream listener and an HTTP/2 upstream cluster pointed at
that backend (the config shape of the landed fixture
`tests/fixtures/0010-http2-router-upstream/`).

| cell | upstream Envoy v1.33.0 | envoy-rust @ `d8eb34f` |
|---|---|---|
| status | `200` | `200` |
| `content-type` | `text/plain` | `text/plain` |
| `trailer` announce header | `x-trail-a` — **forwarded** | `x-trail-a` — **forwarded** |
| `server` | `envoy` | `envoy-rust` (allow-listed) |
| `date` | present | present (allow-listed) |
| `x-envoy-upstream-service-time` | present | present (allow-listed) |
| body | `BODY-OK` | `BODY-OK` |
| **response trailers** | **`x-trail-a=alpha`, `x-trail-b=beta`** | **NONE** |

**Four findings, each load-bearing:**

- **F1 — upstream Envoy forwards H2 response trailers with NO config knob.**
  The probe config carries no `http_protocol_options`, no `enable_trailers`, no
  HTTP/2 tuning of any kind. Trailers came through on a stock config. **This
  phase therefore adds NO new config surface**, which is what keeps it small —
  see §2.2 and the E0063 discussion in §1.2 F7.
- **F2 — Envoy forwards the UNANNOUNCED trailer too.** `x-trail-b` was never
  named in the `trailer:` header and was forwarded regardless. So the forward
  rule is not "forward what was announced"; it is "forward the trailer block".
- **F3 — the `trailer:` announce header is already at parity.** Both proxies
  pass it through as an ordinary response header. That cell needs no work; it
  is a pre-existing pass and must not regress.
- **F4 — every other compared cell is already equivalent.** The divergence is
  exactly ONE cell wide. This is the cheapest possible shape for a differential
  phase: a fixture that is green on every axis except the one under test.

**Negative control (the probe is not vacuous):** upstream Envoy's admin `/stats`
reported `cluster.backend.upstream_rq_200: 1`,
`cluster.backend.upstream_rq_total: 1` and `http.probe.downstream_rq_total: 1`
after the probe — the request genuinely traversed the proxy rather than being
served from anywhere else. (This control exists because a probe that silently
fails to reach the proxy returns a believable-looking result; see the standing
trap about a `docker ps` template error faking a zero.)

### §1.2 The tree census (subagent recon, RE-VERIFIED on disk by the main session)

Five read-only recon subagents surveyed the candidate families. Every figure
below was re-derived by the main session directly; where a subagent's number did
not survive, §1.3 records the correction.

- **F5 — the proxy data path plumbs no trailers, at all.** Across the whole
  `crates/` tree, `.trailers()` and `send_trailers` appear in exactly two
  places, and NEITHER is on the proxy relay path:
  - `crates/envoy-http2/src/grpc.rs:217` — `recv_stream.trailers()`, inside the
    one-shot gRPC *health-check client* (phase 69), and
  - `crates/envoy-health/src/probe.rs:551` — `send_trailers`, inside a *test
    server*.

  So the machinery is proven usable in-tree, and is simply not wired into
  proxying. `crates/envoy-http2/src/client.rs:193` drains
  `recv_stream.data()` and then builds an `envoy_http1::Response` from status +
  headers + body — it never asks for the trailer block. That is the whole cause
  of the `TRAILERS NONE` in §1.1.
- **F6 — the downstream emit seam is a single public function.**
  `crates/envoy-http2/src/response.rs:81` — `pub async fn send_envoy_response`,
  re-exported at `crates/envoy-http2/src/lib.rs:50`, called from exactly one
  production site, `crates/envoy-http2/src/hcm.rs:1043`. A one-function seam is
  the same shape phase 110.1 used for the local-reply transform, and it is why
  the emit half of this phase is small.
- **F7 — the response type is SHARED, and widening it is an ~82-site change.**
  `envoy-http2` does not own a response type; it uses `envoy_http1::Response`
  (`crates/envoy-http1/src/response.rs:13`), whose four fields are `status`,
  `reason`, `headers`, `body`. Adding a fifth field is a cross-crate `E0063`
  event at **82** struct-literal sites (39 in `envoy-http1`, 17 in
  `envoy-http2`, the rest in `tests/` and other crates). Phase 109.1 took a
  ~101-site fan-out of exactly this kind and landed **+46% over its own PLAN
  estimate** — the single worst estimate miss in the last ten phases. **D3 below
  therefore refuses to widen `Response`.**
- **F8 — HTTP/1.1 cannot express trailers today, for a reason unrelated to
  trailers.** `crates/envoy-http1/src/response.rs:18` states the downstream H1
  response body is `CL-framed in 04.1; chunked deferred.` H1 trailers require
  chunked framing (RFC 7230 §4.1.2), so an H1 trailer phase must FIRST land
  chunked *response encoding* — a framing change on every H1 response in the
  tree. The read side is likewise absent by design:
  `crates/envoy-http1/src/client.rs:588` ("trailers discarded") and `:595`
  ("04.3 ignores trailers … trailer forwarding deferred"). **This is the
  measurement that scopes phase 111 to HTTP/2 only** (§5, non-goal 1).
- **F9 — the filter pipeline is headers-only.** `crates/envoy-filter/src/pipeline.rs`
  exposes `decode_headers` (`:88`) and `encode_headers` (`:105`) and nothing
  else — no `decode_data`, no `encode_data`, no `*_trailers`. This is the gRPC
  family's SECOND prerequisite, and it is **not** this phase (§5, non-goal 3).
- **F10 — the harness cannot see trailers, on either side.** The differential
  H2 driver `drive_http2` (`tests/differential/src/lib.rs:2332`) never calls
  `.trailers()`; the harness's own H1 chunked decoder explicitly discards them
  (`tests/differential/src/lib.rs:2951` — "Last chunk — ignore optional
  trailers"). Across all of `tests/`, "trailer" occurs 7 times, every one
  incidental. **So the `Response trailers` row of the equivalence matrix
  (`docs/envoy-rust/BEHAVIOR_CONTRACT.md:18` — "Set-equal under the same
  allow-list discipline") has never once been exercised by any fixture.** This
  phase is its first witness.
- **F11 — the harness has the exact extension points this needs.**
  `drive_http2` is GET/OPTIONS-only and cannot send a request body
  (`tests/differential/src/lib.rs:2343`) — **and that limitation does not bind
  this phase**, because response trailers are witnessed on a plain `GET`. The
  H2 backend helper `tests/helpers/http2-echo-server/` already uses a
  flag-selected mode system (`--close-before-response`, at
  `tests/helpers/http2-echo-server/src/main.rs:58`), so a trailer-emitting mode
  is an additive flag, not new machinery.
- **F12 — the H2-listener × H2-upstream path is landed and green.** Fixture
  `tests/fixtures/0010-http2-router-upstream/` is exactly that topology, and its
  `envoy-rust.yaml` records the per-side divergences this fixture family uses
  (bind `127.0.0.1`, no `admin` block, no `generate_request_id`, no
  `request_headers_to_remove`, no `dns_lookup_family`).

### §1.3 Corrections this session made to inherited or delegated figures

Recorded per D-3.4/D-3.5 so the next session does not re-inherit the error:

- **C1 — the E0063 blast is 82 sites, not 182.** The main session's own first
  probe used `grep -rn 'Response {'`, which also matches `FilterResponse {`,
  `HttpResponse {` and `SendResponse {`, and returned **182**. A word-boundary
  re-count excluding those siblings measures **82**. The over-count was the main
  session's own, caught by re-measuring rather than by review. The design
  conclusion (D3) is unchanged, but it now rests on the true number.
- **C2 — the inherited "worst overrun ratio 1.50" is stale; it is 1.75.** The
  handoff into this session cited a landed-phase calibration of median 1.19 /
  worst 1.50. Re-measured across the last TEN landed phases (73, 74, 76.1, 76.2,
  108.1, 108.2, 109.1, 109.2, 110.1, 110.2): **median ≈ 1.32, worst 1.75**
  (phase 74, inflated by two code-review MUST-FIX re-entries). §9 prices this
  phase against the re-measured figures, not the inherited ones.
- **C3 — the Observability family is NOT 29 rows with work remaining.** Seven
  further `Observability family:` rows (64, 65, 70–74) are physically filed
  under the `### Deprecated / edge features` heading rather than under their own,
  bringing the true total to 36 — **all `done`**. The family has no open row.
  This is a mis-filing in `ROADMAP.md`, NOT a defect to fix here (§5, non-goal 7).
- **C4 — the access-log filter subsystem is further along than the handoff
  implies.** Six of the thirteen upstream `AccessLogFilter` oneof arms are
  landed (`status_code_filter`, `response_flag_filter`, `header_filter`,
  `and_filter`, `or_filter`, `metadata_filter` — `crates/envoy-accesslog/src/filter.rs`).

---

## §2. Why this surface — the cheapest-strong-differential argument

### §2.1 This phase discharges a prerequisite the project named three years of phases ago and has re-affirmed ever since

`ADR-0048` (the phase-18 xDS-family-opener pick) rejected the gRPC family with
this reasoning, quoted from `DECISIONS.md`:

> **(a) gRPC family** … Rejected — **blocked on a missing prerequisite**: gRPC
> requires HTTP/2 trailer propagation (the `grpc-status`/`grpc-message`
> trailers), and the code survey confirmed trailers are discarded today …
> **A gRPC family opener would have to be "H2/H1 trailer plumbing" — a
> prerequisite phase with a thin differential surface of its own.**

**The premise was re-tested at this session and is STILL TRUE** (F5, F9) — this
matters, because the standing lesson of this project is that a rejection
re-affirmed many times is not thereby true for *your* slice: `ADR-0177` found
precisely that when it measured that the trailer blocker did **not** reach
phase 110's local-reply surface. Here it does reach, and it still holds.

`ADR-0177` (the phase-110 pick, the most recent) then re-affirmed the block for
the data path specifically: the four §9-named gRPC data-path items —
`grpc_web`, the gRPC bridge, gRPC-JSON transcoding, `grpc_stats` — are *"all
still trailer-blocked"*. Phase 110's own SPEC lists as non-goal 2: *"Trailer
reading, forwarding or emission of any kind — the tree's trailer gap is real and
stays exactly as it is."*

So: **the gRPC family is 100% blocked behind two prerequisites, and this phase
is the first of them.** That is the strategic case. The rest of §2 is the
tactical one.

### §2.2 The six properties, each measured

1. **A real, single-cell, already-measured divergence** (§1.1) — not a
   projection. Both proxies were driven at this session.
2. **ZERO new config surface.** F1 measured that Envoy needs no knob, so
   envoy-rust needs none either. No new config field ⇒ no `deny_unknown_fields`
   validator, no boot-fatal reject matrix, and critically **no E0063 config
   fan-out** — the exact cost that blew phases 108.1/109.1 past their estimates.
3. **ZERO new dependencies.** The `h2` crate already provides both halves
   (`RecvStream::trailers`, `SendStream::send_trailers`) and is already a direct
   dependency of `envoy-http2`, already used for trailers in-tree at
   `crates/envoy-http2/src/grpc.rs:217` (F5).
4. **A one-function emit seam** (F6) and a one-function read site (F5), both
   already located.
5. **The harness extension is additive and precedented** (F11) — a new flag mode
   on an existing helper binary, and a `GET`-shaped driver that needs no request
   body.
6. **It lights up a contract row that has never been exercised** (F10) — the
   `Response trailers` line of the equivalence matrix becomes a witnessed rule
   instead of an aspiration.

### §2.3 What was rejected, and the measurement that killed each

Every alternative below was surveyed from disk at this session.

- **(a) The gRPC DATA path itself** (`grpc_web` / gRPC bridge / gRPC-JSON
  transcoding / `grpc_stats`). **Rejected — still genuinely blocked**, on TWO
  prerequisites, not one: the trailer gap (F5) *and* the headers-only filter API
  (F9). `grpc_stats` is the cheapest of the four (its stats primitives are
  already free in `envoy-stats`) and still needs both. This phase is the
  prerequisite; taking the data path first would mean building the prerequisite
  inside a phase scoped as something else.
- **(b) HTTP/1.1 trailer forwarding.** **Rejected on a measurement** — F8. H1
  trailers require chunked response *encoding*, which
  `crates/envoy-http1/src/response.rs:18` says is deferred. That is a framing
  change to every H1 response, with a blast radius far beyond this phase's
  budget, and it is independent of the gRPC unblock (gRPC is HTTP/2). Banked as
  **CF-111-2**.
- **(c) HTTP/3 + QUIC family opener** (zero ROADMAP rows). **Rejected on
  sizing** — re-confirmed at this session and consistent with ADR-0177's three
  measured blockers: the merged-listener cap of one, no transport-socket
  abstraction (`crates/envoy-tls/src/lib.rs` `accept`/`connect` are
  `TcpStream`-concrete), and ALPN actively rejected as an unknown field. There
  is no `UdpSocket` anywhere in `crates/`, and `ConnectionHandler::handle` takes
  a concrete `tokio::net::TcpStream` — a QUIC listener is a restructuring, not a
  variant. Estimated 2500–4000+ net LoC. Additionally the HTTP/3 *framing* crate
  (`h3`) is NOT on the D-3.2 permitted-foundations list — only `quinn` is.
- **(d) WASM host family opener** (zero ROADMAP rows). **Rejected on sizing AND
  governance** — `wasmtime` appears nowhere in `docs/envoy-rust/MISSION.md`'s
  permitted-foundations list (zero hits), so it needs a permitting ADR *before a
  line of code*; `ROADMAP.md` itself calls the family "its own multi-phase
  sub-project". Estimated 3000+ net LoC.
- **(e) `RuntimeUInt32` honoring** (Runtime family). **Rejected on a standing
  prohibition.** It has exactly one config surface today
  (`ComparisonFilter.value`, the access-log `status_code_filter`), and honoring
  it would flip the `runtime_key_is_rtds_inert` pin from an absence-pin to a
  parity test. That pin is on the standing do-not-touch list.
- **(f) CSRF runtime honoring.** **Rejected on a standing prohibition** — a
  present `runtime_key` on `filter_enabled` is boot-REJECTED by
  `ConfigError::UnsupportedRuntimeKeyedCsrfFilterEnabled` (ADR-0061 L6), and the
  CSRF rejects are on the same do-not-touch list.
- **(g) RTDS.** **Rejected on sizing** — the poll/mtime watcher
  (`crates/envoy-cluster/src/xds_watch.rs`) genuinely generalizes (RDS and EDS
  both ride it), but RTDS additionally needs an `rtds_layer` oneof arm that
  ADR-0049 currently rejects fail-loud, plus a snapshot **swap** path that does
  not exist — `RuntimeSnapshot` is built once at boot and all three consumers
  read a frozen `Arc`. Not "instantiate the existing watcher".
- **(h) Hot restart.** **Rejected — never built at all.** Phase 08 landed
  graceful *drain* only; `/server_info`'s `hot_restart_version` ships the literal
  `"disabled"`. This is a large greenfield lift sharing a family name with
  something already done.
- **(i) `sni_cluster`** (Network filters). **Rejected on a measured
  prerequisite** — it needs a `tls_inspector` LISTENER filter, and
  `Listener.listener_filters` is parsed as opaque `Vec<serde_yaml::Value>` and
  explicitly NOT executed. `TcpProxy` also binds its cluster once at startup,
  with no per-connection selection hook. ROADMAP row 67 already rejected it on
  the same grounds.
- **(j) Non-deterministic LB** (`random`, `least_request`). **Rejected — a
  standing, four-times-re-affirmed decision** (ADR-0069/0071/0073) that these are
  not bilaterally assertable under the harness's byte-exact selection model, with
  no alternative strategy yet proposed.
- **(k) Priority / locality / panic-threshold LB.** **Rejected on sizing** — a
  data-model change, not additive: `LocalityLbEndpoints` has no `locality` or
  `priority` field and carries `deny_unknown_fields`, and the runtime host set is
  a flat `Vec<SocketAddr>` with no priority partitioning.
- **(l) Remaining Observability items** (gRPC ALS, OTLP, tracing, stats sinks,
  tap). **Rejected — each is wholly ABSENT, not partial**, so each is a
  from-scratch opener; tracing in particular has no scaffolding whatsoever (no
  `traceparent`, no `x-b3-`, no `HttpConnectionManager.tracing` field). Note
  also C3: the family has no open row to pick up.
- **(m) The drafted repo-health phase** (`.claude/drafts/DRAFT-SPEC-110-…`, the
  `STATE.md` traps-line trim). **Rejected for this pick** — it is real
  maintenance debt (the traps line measures 177 602 characters, ~16× its
  post-ADR-0160-trim size) but it advances none of the mission's three
  stop-condition legs, and the operator has not chosen a landing path. It
  remains a live candidate for a later pick.
- **(n) Banked findings** (the `110.1`/`110.2` REVIEW Minors, CF-110-1…9, and
  the standing CF ledger). **Not a pick** — §6.3/ADR-0165: a phase banks, it
  never clears. They stay open and are an input, not an obligation.

---

## §3. Scope — what this phase builds (design decisions D1–D8)

**D1 — HTTP/2 only, response direction only, upstream → downstream.**
The surface is: trailers arriving from an HTTP/2 upstream on a proxied response
are forwarded to the HTTP/2 downstream client. Nothing else.

**D2 — Read site: `crates/envoy-http2/src/client.rs`, immediately after the
existing body drain.** The `while let Some(chunk_result) = recv_stream.data()`
loop at `:193` runs to completion; `recv_stream.trailers().await` is then valid
and yields `Option<http::HeaderMap>`. Convert to `Vec<(String, String)>`
preserving wire order, mirroring the header conversion already directly below
that loop (including its defensive skip of non-ASCII values).

**D3 — Trailers ride ALONGSIDE `Response`, never INSIDE it.** Do NOT add a
field to `envoy_http1::Response`. F7 measures that as an 82-site `E0063` change
across four crates for a value that only the HTTP/2 path can ever populate or
emit. Instead the upstream call returns the trailers as a separate value
(a `(Response, Option<Vec<(String, String)>>)` pair, or a small
`envoy-http2`-local struct) threaded to the emit seam. This is the same
containment discipline phase 110.1 applied when it kept `apply_grpc_local_reply`
out of the shared `build_response` because `envoy-http2` calls that function.
**This decision is the main reason the phase is affordable; do not undo it for
tidiness.**

**D4 — Emit site: `crates/envoy-http2/src/response.rs:81`, `send_envoy_response`.**
Widen it to accept the optional trailer block. Its end-of-stream logic changes in
exactly one way: when trailers are present, the final `send_data` must carry
`end_of_stream = false` and be followed by `send_stream.send_trailers(map)`;
when absent, behaviour is byte-identical to today. **The empty-body case needs
explicit care** — today an empty body takes a `send_response(.., end_of_stream=true)`
branch, which cannot be followed by trailers; with trailers present that branch
must not be taken.

**D5 — Forward the trailer block verbatim; do NOT consult the `trailer:`
announce header.** F2 measured that Envoy forwards an unannounced trailer, so
filtering by the announce header would be a divergence. F3 measured that the
announce header itself is already forwarded as an ordinary header by both
proxies — leave that path alone.

**D6 — No filter involvement.** Trailers bypass the filter pipeline entirely in
this phase. F9's headers-only API is not extended, not called with trailers, and
not modified. The relationship between trailers and encode-side filters is
`CF-111-1`, for the phase that closes the filter-API prerequisite.

**D7 — Harness: extend, do not rebuild.** Three additive changes:
(i) `drive_http2` (`tests/differential/src/lib.rs:2332`) reads
`.trailers()` after its body drain and surfaces them on its result type;
(ii) a trailer expectation rule on the H2 driver variant, compared with the same
set-equality-modulo-allow-list discipline `diff_headers` already applies to
headers (`docs/envoy-rust/BEHAVIOR_CONTRACT.md:18`);
(iii) a trailer-emitting mode on `tests/helpers/http2-echo-server/` selected by
a new flag, following the `--close-before-response` precedent at
`tests/helpers/http2-echo-server/src/main.rs:58`, plus the matching backend
struct in `tests/differential/src/backend.rs`.
**`drive_http2`'s GET/OPTIONS-only, no-request-body limitation is NOT lifted**
(F11) — response trailers are witnessed on a `GET`.

**D8 — Fixture `0090-h2-response-trailers`, modelled on `0010`.** The
H2-listener × H2-upstream topology of
`tests/fixtures/0010-http2-router-upstream/` (F12), with its per-side divergence
list carried over verbatim. The fixture must witness at least: an announced
trailer, an UNANNOUNCED trailer (F2), and a response that has trailers with a
NON-empty body. See §7 PV-3 for the empty-body and zero-trailer cases, which
must be measured before they are frozen into probes.

---

## §4. Differential surface at phase end

- **NEW fixture `0090-h2-response-trailers`** green cross-proxy: response
  trailers set-equal between upstream Envoy v1.33.0 and envoy-rust on an
  HTTP/2-in, HTTP/2-out proxied response.
- **All 89 pre-existing fixtures still green** (§7.5 gate (b)). D4's
  end-of-stream change is the regression risk to watch: every existing H2
  response takes the no-trailers path and must be byte-identical on the wire.
- **`BEHAVIOR_CONTRACT.md` gains a `## Response trailers` section** — the first
  population of the equivalence-matrix row at
  `docs/envoy-rust/BEHAVIOR_CONTRACT.md:18`, recording the forward rule (D5),
  the announce-header disposition (F3), the allow-list discipline, and every
  cell §7/§8 leave unmeasured.
- **Conformance:** `h2spec` continues to pass at its declared threshold, with
  `tests/conformance/h2spec/known-failures.txt` **untrimmed** — a standing
  prohibition. h2spec exercises HEADERS-after-DATA framing, so it is a genuine
  (if indirect) check on D4.
- **Fuzzing:** no new parser, codec or filter is introduced — `h2` owns the
  trailer framing — so §7.4 does not require a new fuzz target. If the PLAN
  concludes otherwise it must also add the `ci.yml` step, since fuzz targets are
  not auto-discovered.

---

## §5. Non-goals — do NOT widen into these

1. **HTTP/1.1 trailers, in either direction.** Blocked behind chunked response
   encoding (F8). → `CF-111-2`.
2. **REQUEST trailers** (downstream → upstream). Not measured, not needed by
   the gRPC unblock in this direction. → `CF-111-3`.
3. **Filter-API data/trailer hooks.** `decode_data`/`encode_data`/
   `decode_trailers`/`encode_trailers` are the gRPC family's SECOND prerequisite
   (F9) and are their own phase. Do not touch `crates/envoy-filter/`.
4. **Any gRPC data-path filter** — `grpc_web`, `grpc_http1_bridge`,
   `grpc_json_transcoder`, `grpc_stats`. All four remain blocked until non-goal 3
   also lands.
5. **The `%TRAILER(…)%` and `%GRPC_STATUS%` access-log command operators.** Both
   are ABSENT today and both read trailers, so both become *possible* after this
   phase — neither is *in* it. → `CF-111-4`.
6. **Trailers on locally-generated replies** (`direct_response`, synth 404/503,
   filter-generated). Upstream's behaviour there is unmeasured (§8) and the
   surface here is proxied responses only.
7. **Repairing `ROADMAP.md`'s mis-filed rows** (C3) or its two unescaped-pipe
   rows (38, 39). Both are known and deliberately left alone.
8. **Any new config surface** (F1), any new dependency, and any change to
   `HEADER_ALLOW_LIST` — which is 3 entries (`server`, `date`,
   `x-envoy-upstream-service-time`) and must stay so.
9. **The io_uring H1 path** (`crates/envoy-http1/src/uring.rs`), which already
   does not proxy chunked upstream responses.

---

## §6. Carry-forwards this phase OPENS

- **CF-111-1** — trailers bypass the filter pipeline entirely (D6). When the
  filter API grows data/trailer hooks, the interaction must be designed and
  measured; until then an encode-side filter cannot see or alter a trailer.
- **CF-111-2** — HTTP/1.1 trailer forwarding remains unbuilt, blocked behind
  chunked response encoding (F8). Both the read side
  (`crates/envoy-http1/src/client.rs:588`) and the write side
  (`crates/envoy-http1/src/response.rs:18`) are affected.
- **CF-111-3** — REQUEST trailers (downstream → upstream) remain unbuilt and
  unmeasured.
- **CF-111-4** — `%TRAILER(…)%` and `%GRPC_STATUS%` access-log operators remain
  absent; this phase makes them implementable but does not implement them.

**Carried forward UNCONSUMED** (§6.3; ADR-0165 — a phase banks, it never
clears): the `110.2` REVIEW's M-1…M-8 + N-1…N-12; the `110.1` REVIEW's M-1…M-9 +
N-1…N-10; CF-110-1…CF-110-9; CF-109-1/2/3; CF-108-1/2/3; CF-76-1; CF-75-2/3/4/5/6;
CF-72-2/CF-75-1; M71-6; CF-74-1/2/3/4/6; CF-73-1; the `109.2`, `109.1` and
`108.2` REVIEW sets; and the HTTP-filters-family (1)–(4).

---

## §7. PLAN-VERIFY items — re-confirm FRESH at the state-2 PLAN-write

The §6.2 empirical-reconciliation discipline. Each of these has changed a plan
before; measure them against the pinned image, do not reason about them.

- **PV-1 — Re-run the §1.1 probe end-to-end before planning around it.** It is
  this SPEC's foundation. Confirm both proxies still behave as the table says at
  the then-current HEAD.
- **PV-2 — Dry-run the EXACT `0090` YAML pair against BOTH proxies before
  freezing any probe.** This is the discipline that caught three unrelated
  divergences at the phase-110 PLAN-write (CF-110-6/7/8), each of which would
  have landed the fixture RED for reasons unrelated to its subject. A divergence
  no fixture can express is invisible to the gate.
- **PV-3 — Measure the four edge cells before writing probes for them:**
  (a) trailers on an EMPTY-body response; (b) a response announcing a trailer via
  `trailer:` that then sends NONE; (c) a trailer whose name duplicates a response
  header name; (d) a trailer block containing a name Envoy might strip
  (`content-length`, a `:`-pseudo-header, `transfer-encoding`, `te`). Envoy is
  known to sanitise some of these; envoy-rust currently sanitises none.
- **PV-4 — Confirm the harness's trailer comparison discipline.** `diff_headers`
  compares a SET of names and only the FIRST occurrence's value on a duplicate.
  Decide explicitly whether trailer comparison reuses that or needs multiset
  semantics, and record the decision.
- **PV-5 — Confirm the empty-body `send_response(end_of_stream=true)` branch
  (D4) is the only end-of-stream site.** Grep `send_response(` and `send_data(`
  across `crates/envoy-http2/` and enumerate every site; a missed one is a
  silent half-implementation.
- **PV-6 — Confirm no pre-existing fixture regresses.** Specifically, that a
  no-trailers response is byte-identical on the wire after D4.
- **PV-7 — Re-derive the E0063 count if D3 is ever revisited.** C1 records that
  a careless grep over-counts by ~2×.
- **PV-8 — Re-price the phase against landed-phase calibration** (C2: median
  1.32, worst 1.75), bottom-up, and apply the §6.1 gate honestly. See §9.

---

## §8. NOT MEASURED — stated explicitly per D-3.4

Do not assume any of these; they are open questions, not settled cells.

- Upstream's behaviour on trailers over **TLS**, and on an **HTTP/1.1
  downstream with an HTTP/2 upstream** (the ADR-0028 dispatch deferral, still
  OPEN, may make that combination unreachable anyway).
- Whether Envoy **strips or rewrites** any trailer name (PV-3(d)).
- Trailer behaviour on **non-200** proxied responses, on **retried** requests,
  and on responses that the **router** or a filter short-circuits.
- Whether Envoy emits trailers on **locally-generated** replies (non-goal 6).
- **Trailer ORDER** on the wire. The harness compares header *sets*, not order,
  so an order divergence would be invisible to the fixture regardless — the same
  blindness `110.2`'s contract §D records for header order.
- Any **stats** Envoy may tick for trailers. None were looked for.
- Whether `h2`'s `send_trailers` enforces any name validation that would turn a
  divergence into an error.

---

## §9. Size estimate and the §6.1 split gate

Bottom-up, net LoC **excluding `docs/`** (the measurement basis §1.3 C2 used):

| area | net LoC |
|---|---:|
| `envoy-http2` read site + type threading (D2, D3) | ≈120 |
| `envoy-http2` emit seam + end-of-stream logic (D4) | ≈120 |
| `envoy-http2` unit tests (incl. the empty-body and no-trailer cases) | ≈250 |
| harness: `drive_http2` + result type + expectation rule + match arms (D7 i/ii) | ≈200 |
| harness: `http2-echo-server` trailer mode + backend struct (D7 iii) | ≈130 |
| fixture `0090` (4 files) + its differential test file (D8) | ≈180 |
| **central estimate** | **≈1000** |

**Against the CURRENT §6.1 gate of ~25 tasks / ~1500 net LoC** — NOT the
unlanded `.claude/drafts/DRAFT-ADR-split-thresholds.md` proposal of ~35/~2500,
which binds nothing.

- Task count: projected 7–9, comfortably under ~25 (the last ten phases ranged
  5–12).
- LoC: ≈1000 central. At the re-measured **median 1.32× → ≈1320** (under the
  gate). At the re-measured **worst 1.75× → ≈1750** (over the gate).

**Verdict: the split is PROJECTED POSSIBLE, not projected-to-fire.** `ADR-0182`
is **RESERVED-UNFIRED** for it. If the state-2 PLAN-write's bottom-up re-derivation
exceeds ~1500, the natural seam is:
**`111.1`** = the `envoy-http2` read + thread + emit + in-process unit witness
(no fixture, no harness change — the shape 110.1 and 108.1 both used), and
**`111.2`** = the harness extension + backend mode + fixture `0090` +
the `BEHAVIOR_CONTRACT.md` section + the parent close.
That seam is clean because §1's F5/F6 locate the production change entirely
inside one crate, while every harness change is in `tests/`.

---

## §10. Definition of done — the §7.5 gate, instantiated

- **(a)** `0090-h2-response-trailers` green cross-proxy.
- **(b)** all 89 pre-existing fixtures still green (see §4 on the D4 regression
  risk).
- **(c)** `h2spec` passes at its declared threshold; `known-failures.txt`
  untrimmed.
- **(d)** no new fuzz target expected (§4); if one is added, its `ci.yml` step is
  added with it.
- **(e)** `cargo build --workspace --all-targets`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all
  clean.
- **(f)** `REVIEW.md` approved.

---

## §11. Next state

**§5 state 2 — the PLAN-write**, in a SEPARATE session (§5.1; ADR-0127: the
context that wrote this SPEC must not plan from it). That session runs
`superpowers:writing-plans`, produces `PLAN.md`, works §7's PV-1…PV-8 FIRST, and
applies the §6.1 split gate to its own bottom-up re-derivation. It does not
inherit §9's number — it re-derives it.

**It does NOT write code.** The first code lands at state 3.
