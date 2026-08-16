# Sub-phase 110.1 — gRPC-aware local replies over HTTP/1.1: the three pure functions + the H1 local-reply seam, witnessed ENTIRELY IN-PROCESS

> Redistributed from `docs/envoy-rust/phases/110-grpc-aware-local-replies/SPEC.md`
> at the §5 state-2 PLAN-write, when the §6.1 split FIRED (**ADR-0178**).
> Written for a reader with ZERO prior context (D-3.4). Every upstream
> behaviour cited as MEASURED was probed at the split session against the
> `ENVOY_TARGET.md`-pinned `envoyproxy/envoy:v1.33.0` (digest
> `sha256:56da5afd…70c2`, verified by `docker image inspect` BEFORE any probe;
> all probe containers torn down afterwards). Every figure is still a CLAIM the
> state-2 PLAN-write must re-derive on disk (§6).

## §0. What this sub-phase is, in one paragraph

Upstream Envoy rewrites any **locally generated** reply when the request that
provoked it carries a gRPC `content-type`: the HTTP status becomes **`200`**,
`content-type` becomes **`application/grpc`**, the body is **DROPPED**,
`content-length` becomes **`0`**, a **`grpc-status`** header carries a mapped
code, and — only when the original body was non-empty — a **`grpc-message`**
header carries the original body percent-encoded. envoy-rust implements NONE of
it. **This sub-phase builds the whole behaviour for HTTP/1.1 and proves it
IN-PROCESS. It ships NO differential fixture** — sibling `110.2` does that.
**Everything in this surface is a response HEADER on a bodiless reply, so no
trailer is involved anywhere** (§2.1). This sub-phase adds **NO config
surface**, and therefore **no new `ConfigError` variant and no new fuzz
target**.

## §1. The measured upstream contract

All three matrices below were re-measured at the split session with a
**raw-socket** HTTP/1.1 client (one connection per probe, `connection: close`,
no reuse) rather than a header-dict client, because a dict client destroys
response header ORDER and CASE.

### §1.1 The HTTP→`grpc-status` mapping — a SPARSE EIGHT-ENTRY table over a DEFAULT of 2

Probe harness: one HCM listener, one `direct_response` route per status at its
own distinct path, each with body `B<status>`; every request carrying
`content-type: application/grpc`, plus a paired control request without it.

| configured status | gRPC request → | control (no gRPC ct) |
|---|---|---|
| 200 | `200` + `grpc-status: 2` | `200`, body `B200` |
| 201 | `200` + `grpc-status: 2` | `201` |
| 204 | `200` + `grpc-status: 2` | `204` |
| 301 | `200` + `grpc-status: 2` | `301` |
| **400** | `200` + **`grpc-status: 13`** | `400` |
| **401** | `200` + **`grpc-status: 16`** | `401` |
| **403** | `200` + **`grpc-status: 7`** | `403` |
| **404** | `200` + **`grpc-status: 12`** | `404` |
| 405 | `200` + `grpc-status: 2` | `405` |
| 408 | `200` + `grpc-status: 2` | `408` |
| 409 | `200` + `grpc-status: 2` | `409` |
| 412 | `200` + `grpc-status: 2` | `412` |
| 413 | `200` + `grpc-status: 2` | `413` |
| **429** | `200` + **`grpc-status: 14`** | `429` |
| 499 | `200` + `grpc-status: 2` | `499` |
| 500 | `200` + `grpc-status: 2` | `500` |
| 501 | `200` + `grpc-status: 2` | `501` |
| **502** | `200` + **`grpc-status: 14`** | `502` |
| **503** | `200` + **`grpc-status: 14`** | `503` |
| **504** | `200` + **`grpc-status: 14`** | `504` |

**The rule: only `400→13`, `401→16`, `403→7`, `404→12`, `429→14`, `502→14`,
`503→14`, `504→14` are special; EVERY other status maps to 2 (UNKNOWN)** —
including the whole 2xx/3xx range and, counter-intuitively, `500`, `501`,
`405`, `408`, `409`, `412`, `413` and `499`.

### §1.2 The detection rule — SHARP, with two independent traps

| request `content-type` | detected? |
|---|---|
| `application/grpc` | **YES** |
| `application/grpc+proto` | **YES** |
| `application/grpc+json` | **YES** |
| `application/grpc+` (bare, nothing after `+`) | **YES** |
| `application/grpc; charset=utf-8` | **NO** — a parameter DEFEATS it |
| `application/grpc;charset=utf-8` | **NO** — with or without the space |
| `APPLICATION/GRPC` | **NO** — CASE-SENSITIVE |
| `Application/Grpc` | **NO** |
| `application/grpc-web` | **NO** |
| `application/grpc-web+proto` | **NO** |
| `application/grpcfoo` | **NO** — not a bare prefix match |
| `application/json` | NO (control) |
| *(header absent)* | NO (control) |

**Derived rule: detected iff the `content-type` value is EXACTLY
`application/grpc` or begins with `application/grpc+`.** Nothing else.

Two traps, both directly witnessable: a naive
`starts_with("application/grpc")` wrongly detects `application/grpcfoo` and
`application/grpc-web`; a case-insensitive or parameter-tolerant match wrongly
detects `APPLICATION/GRPC` and `application/grpc; charset=utf-8`.

Also measured: detection is **METHOD-INSENSITIVE** (`GET`, `POST`, `PUT`
transform identically) and **INDEPENDENT of `te: trailers`** in both
directions.

> **One measured cell that is NOT a rule exception.** The value
> `application/grpc ` (with a TRAILING SPACE) **is** detected — but that is the
> HTTP codec stripping optional trailing whitespace (OWS) from the field value
> before anything sees it, not a tolerance in the matcher. **Do NOT build
> trailing-space tolerance into the comparison**; rely on the codec's existing
> OWS handling, exactly as every other header comparison in the tree does.

### §1.3 `grpc-message` percent-encoding — the SPEC-110 rule was WRONG at the boundary

Parent `110/SPEC.md` §3 D4 stated "bytes `0x20` (space) through `0x7E` pass
through UNCHANGED except `%`". **MEASURED FALSE: `~` (0x7E) IS ESCAPED.**

Probed with three bodies and their byte-exact controls:

| original body (control, `xxd`-confirmed) | `grpc-message` |
|---|---|
| `a b\ncontrol\ttab é %25 end` (`61 20 62 0a 63 6f 6e 74 72 6f 6c 09 74 61 62 20 c3 a9 20 25 32 35 20 65 6e 64`) | `a b%0Acontrol%09tab %C3%A9 %2525 end` |
| `q"b s\l t~t d<0x7F>d` (`71 22 62 20 73 5c 6c 20 74 7e 74 20 64 7f 64`) | `q"b s\l t%7Et d%7Fd` |
| `  ~ +,/:;=?@[]{}\|^`+"`"+`<>#&*()` (`20 20 7e 20 2b 2c 2f 3a 3b 3d 3f 40 5b 5d 7b 7d 7c 5e 60 3c 3e 23 26 2a 28 29`) | `%7E +,/:;=?@[]{}\|^`+"`"+`<>#&*()` |

**The corrected rule: a byte passes through UNCHANGED iff it is in
`0x20..=0x7D` AND is not `%` (0x25). Every other byte — i.e. every byte
`< 0x20`, every byte `>= 0x7E`, and `%` itself — becomes `%` followed by TWO
UPPERCASE hex digits.** Multi-byte UTF-8 is encoded PER BYTE (`é` → `%C3%A9`).
Confirmed pass-throughs include SPACE, `"`, `\`, and
`` +,/:;=?@[]{}|^`<>#&*() ``. Confirmed escapes include `~`→`%7E`,
`0x7F`→`%7F`, `\n`→`%0A`, `\t`→`%09`, and `%`→`%25` (so the input `%25`
renders as `%2525` — the discriminating cell for a hand-rolled encoder).

> The third row's `grpc-message` shows no leading spaces because the probe
> client left-trims field values per RFC 9110 OWS rules — an artifact of the
> CLIENT, not of Envoy. Do not read it as an encoder rule.

### §1.4 The wire shape, and the header ORDER

On every gRPC-detected H1 local reply:

- HTTP status → **`200`**, regardless of the configured status;
- `content-type` → **`application/grpc`**;
- body **DROPPED**, `content-length` → **`0`**;
- `grpc-status` → the §1.1 code;
- `grpc-message` → §1.3 of the ORIGINAL body, **ABSENT ENTIRELY (not empty)
  when that body was empty** — measured on both a `direct_response` with no
  `body:` and the HCM's own unmatched-path 404.

**MEASURED header order** (the parent SPEC's V-2, now closed):

```
[location,] content-type, grpc-status, [grpc-message,] date, server, connection, content-length
```

**…and the premise behind V-2 is REFUTED: the differential harness does NOT
compare header order.** `run_http1_probe_list_arm`
(`tests/differential/src/lib.rs`) compares status, then body, then calls
`diff_headers`, which builds a `BTreeSet` of LOWER-CASED header NAMES and
compares the sets, then compares VALUES for every name outside the 3-entry
`HEADER_ALLOW_LIST`. **Order is never read.** The
"header ORDER is load-bearing — the differential harness byte-compares against
upstream Envoy" sentence in `crates/envoy-http1/src/hcm.rs`'s `synth_with` doc
block is a HOUSE CONVENTION pinned by in-process tests, not a differential
gate. **Match upstream's order anyway** (the convention is good and the
in-process tests assert it), but know that a wrong order fails a unit test, not
the fixture.

### §1.5 The seam — MEASURED, and TWO inherited claims did not survive

Locate everything BY TEXT; the line numbers below WILL drift.

- **`synth_with(status, body, close) -> Response`**
  (`crates/envoy-http1/src/hcm.rs:2239`) builds the five standard headers in
  the fixed order `[server, date, content-length, content-type, connection]`.
  Its doc block (`:2230-2238`) carries the ORDER warning quoted in §1.4.
- **CORRECTION 1 — `synth_with` has FOUR direct callers, not seven.** The
  inherited "seven callers" list conflated two tiers. Direct: `:2262`
  (`synth_direct_response`), `:2270` (`synth_status`), `:2410`
  (`synth_no_healthy_upstream`), `:2425` (`synth_overflow`).
  `synth_400` (`:2522`), `synth_404` (`:2525`) and `synth_501` (`:2528`) are
  one-line wrappers over `synth_status` — indirect, depth-2.
  `grep -rn 'synth_with' crates/` returns 7 hits of which only 4 are calls; the
  other 3 are the definition and two doc mentions.
- **CORRECTION 2 — `synth_redirect` IS transformed upstream, so it is IN
  SCOPE.** `synth_redirect` (`:2383`) deliberately does NOT reuse `synth_with`
  (documented at `:2378`) because a redirect carries `location` and no
  `content-type`. The parent SPEC left its gRPC behaviour UNMEASURED (V-3) and
  reserved CF-110-3 for it. **MEASURED: a gRPC request to a `redirect:` route
  returns `200`, `location: <unchanged>`, `content-type: application/grpc`,
  `grpc-status: 2`, `content-length: 0`, no `grpc-message`** (the redirect body
  is empty). **The `location` header SURVIVES the transform.** So
  `synth_redirect` must be covered, and CF-110-3 is FREE — it is reassigned in
  §5.
- **The full H1 local-reply family that must be covered IDENTICALLY** — a
  partially-covered family is exactly the silent-divergence class ADR-0049
  exists to prevent:
  - via `build_response_in`: `synth_direct_response`, `synth_400`,
    `synth_404` (two sites), `synth_redirect`;
  - via `run_attempt` / pool failures: `synth_no_healthy_upstream`,
    `synth_status(503)` (four sites), `synth_overflow` (two sites);
  - in `serve_connection`: `synth_overflow` (request-budget), `synth_501`
    (chunked rejection);
  - in the io_uring worker (`crates/envoy-http1/src/uring.rs`): `synth_501`
    (`:285`), `synth_no_healthy_upstream` (`:312`), `synth_status(503)`
    (`:336`, `:387`).
- **TWO wire funnels, not one.** The tokio path converges on
  `hcm.rs:1468` (`Http1Response::write_to_buf`), guarded by an
  `if outgoing_direct` fast path at `:1457` (the zero-copy PROXIED-response
  path, never true for a synth). The io_uring worker has its OWN funnel —
  `write_owned` at `uring.rs:292/313/338/389` — which bypasses `hcm.rs:1468`
  entirely. **A post-pass installed at only one funnel silently misses the
  other.**
- **Request headers ARE in scope at every one of those sites.** `serve_connection`
  binds `req` (`hcm.rs:869`) and it is still live after the write (read at
  `:1531` by the access-log sink). `build_response_in` takes `req: &mut Request`;
  `run_attempt` takes `req: &Request`; the uring worker holds `req`. Only
  `synth_with` ITSELF has no request state — its parameters are
  `(status, body, close)`.

### §1.6 **`build_response` is SHARED WITH HTTP/2 — the parent SPEC's "structural boundary" is REFUTED**

This is the single most important finding for the seam, and it inverts parent
`110/SPEC.md` §3 D6.

`crates/envoy-http2/src/hcm.rs:518` calls **`envoy_http1::build_response`**,
which dispatches to `synth_direct_response` / `synth_400` / `synth_404` /
`synth_redirect` → `synth_with`. `envoy-http1/src/hcm.rs:2127-2128` documents
this: *"ONE arm serves BOTH codecs — H2 has no route-action dispatch of its own
and calls this function."* H2 owns its own synthesisers (`synth_h2_reset`
`:1210`, `synth_h2_connect_failure` `:1234`, `synth_h2_no_healthy_upstream`
`:1257`, `synth_h2_overflow` `:1280`) for UPSTREAM-FAILURE paths ONLY; it
shares H1's synths for ROUTE-DECISION paths.

**Consequence, binding on the PLAN: the transform MUST NOT be installed inside
`synth_with`, inside any `synth_*` wrapper, or inside `build_response` /
`build_response_in`.** Doing so would transform H2 route-decision replies
(direct_response, 400, 404, redirect) while leaving H2's own upstream-failure
replies untransformed — a PARTIALLY-covered family on the H2 wire, the exact
ADR-0049 class this sub-phase must avoid. **The transform belongs at the H1
wire funnels (both of them), applied only to responses known to be LOCALLY
generated.**

### §1.7 H2's shape is now MEASURED (still OUT OF SCOPE — CF-110-1)

Probed over h2c prior-knowledge at the same listener. Upstream applies the
**same** transform on HTTP/2, **headers-only in the HEADERS frame, with no
trailers anywhere** — including on `no healthy upstream`, which is an
upstream-failure path that envoy-rust serves from its own `synth_h2_*` family:

| probe | H2 gRPC result | H2 control |
|---|---|---|
| `direct_response` 404 | `200`, `content-type: application/grpc`, `grpc-status: 12`, `grpc-message: B404` | `404`, `content-length: 4`, body `B404` |
| `direct_response` 400 | `200`, `grpc-status: 13`, `grpc-message: B400` | `400` |
| `direct_response` 503 | `200`, `grpc-status: 14`, `grpc-message: B503` | `503` |
| empty-body 404 | `200`, `grpc-status: 12`, **no `grpc-message`** | `404` |
| unmatched path | `200`, `grpc-status: 12`, no `grpc-message` | `404` |
| `redirect:` route | `200`, `location`, `grpc-status: 2` | `301` + `location` |
| no-healthy-upstream | `200`, `grpc-status: 14`, `grpc-message: no healthy upstream` | `503`, `content-length: 19` |

**Two facts worth banking.** (1) **The trailer blocker does not reach H2 local
replies either** — this materially narrows CF-110-1 from "may require
trailers" to "measured headers-only". (2) **H2 omits `content-length`
ENTIRELY** where H1 emits `content-length: 0`, so the H1 transform is NOT
byte-portable to H2. Covering H2 additionally requires its own `synth_h2_*`
family, which is why it stays out of scope here.

### §1.8 Tree census — envoy-rust implements none of this, and has no encoder

- **No request-side gRPC detection and no HTTP→gRPC status mapping exists
  anywhere in `crates/`.** `grep -rn "grpc" crates/envoy-http1/src/` returns
  **ZERO** hits — the entire H1 crate, including the whole synth family, does
  not contain the string. The only gRPC code in the tree is the **outbound
  health-check CLIENT** (`crates/envoy-http2/src/grpc.rs`, 8 `grpc-status`
  hits; `crates/envoy-health/src/probe.rs`, 6) plus one config deferral comment
  at `crates/envoy-config/src/bootstrap.rs:1296`. `grpc-message` has **ZERO**
  hits in `crates/`.
- **No reusable percent-encoder exists.** Searched `%02X`/`%02x`/`{:02X}`/
  `percent`/`urlencode`/`pct_encode`/`percent_encode` across `crates/`: the only
  `{:02` hits are zero-padded DATE formatting (`envoy-http1/src/date.rs:138`,
  `envoy-accesslog/src/default_format.rs:110`); everything matching `percent`
  is `max_ejection_percent`/`FractionalPercent` config or access-log `%%`
  format-string parsing. `crates/envoy-filter/src/rbac.rs:71` explicitly
  documents that percent-DEcoding is NOT implemented. No dependency provides
  one (`grep -rn 'percent-encoding|urlencoding|form_urlencoded' --include=Cargo.toml .`
  → nothing). **The encoder is written from scratch.**

## §2. Why this slice, and why it is not blocked

### §2.1 The standing ADR-0048 rejection does not reach this surface

`DECISIONS.md` ADR-0048 rejected the gRPC family as blocked on HTTP/2 trailer
propagation (`grpc-status`/`grpc-message` as TRAILERS), and that verdict was
re-affirmed roughly fifteen times, most recently as ADR-0171
rejected-alternative (b) and at the ADR-0175 pick. **It is still entirely
correct for the gRPC DATA PATH.** It does not reach this slice, and the
falsifying measurement is §1.4: every gRPC-detected local reply is
`content-length: 0` over HTTP/1.1 with `grpc-status`/`grpc-message` as ordinary
response **HEADERS**, and a response with no body has no chunked framing and
therefore no trailer section at all. `te: trailers` was measured irrelevant in
both directions (§1.2). **ADR-0177 records this as a NARROWING of ADR-0048's
REACH established by measurement — NOT a supersession** (ADRs are append-only,
D-3.5). The tree's real trailer gap is untouched, and §5 non-goal 2 forbids
adding a trailer API.

### §2.2 Why this slice ships with no fixture

This is the foundation-slice precedent of `108.1` and `109.1`: the slice lands
the complete, working behaviour and proves it IN-PROCESS, and the differential
witness follows in the sibling. The cut is honest in the ADR-0176 DECISION 2
sense — there is **no intermediate landed state that parses something and then
silently ignores it**, because this sub-phase adds no config surface at all.
At `110.1`'s close the transform is fully live on every H1 local-reply path;
what is missing is only the cross-proxy witness.

## §3. Scope — design decisions

**D1 — `is_grpc_request`, a pure total function.** Answers the §1.2 rule over
the request headers: the `content-type` value is EXACTLY `application/grpc`, or
begins with `application/grpc+`. **Byte-exact and CASE-SENSITIVE on the
VALUE**; header-NAME lookup stays case-insensitive as everywhere else in the
tree. No parameter tolerance, no trailing-space tolerance (§1.2), no trimming
beyond what the codec already does. Natural home: a small sibling module of
`crates/envoy-http1/src/hcm.rs`, or beside the synth family.

**D2 — `http_to_grpc_status(u16) -> u8`, a pure total function.** The §1.1
table: the eight special entries and an explicit `_ => 2` arm. Unit-pinned on
every cell in §1.1, including the counter-intuitive ones.

**D3 — `grpc_message_encode(&[u8]) -> String`, a pure total function.** The
CORRECTED §1.3 rule: pass through `0x20..=0x7D` except `%`; escape everything
else as `%` + two UPPERCASE hex digits. Unit-pinned on all three §1.3 strings,
with `~`/`0x7F`/`%25` as the discriminating cells.

**D4 — the transform.** Given a locally generated `Response` and a true
`is_grpc_request`, produce: status `200`; `content-type: application/grpc`;
empty body; `content-length: 0`; `grpc-status: <D2 of the ORIGINAL status>`;
`grpc-message: <D3 of the ORIGINAL body>` **only when that body was non-empty**;
and **preserve any `location` header** (§1.5). The resulting order should match
§1.4 (a unit-test concern, not a differential one — §1.4).

**D5 — the seam.** The transform is applied at the **H1 wire funnels, BOTH of
them** (`hcm.rs:1457/1468` and `uring.rs` `write_owned` ×4), to responses known
to be LOCALLY generated, with `is_grpc_request` computed from the in-scope
request headers. **It MUST NOT be installed in `synth_with`, in any `synth_*`
wrapper, or in `build_response`/`build_response_in`** — those are shared with
HTTP/2 (§1.6). The PLAN must decide how "this response is a local reply" is
carried to the funnel; `BuildOutcome::Synth` marks the route-decision family,
but the `run_attempt`/pool/`serve_connection` local replies are NOT
`BuildOutcome::Synth` and must be marked too. **ALL of the §1.5 family must be
covered identically, `synth_redirect` included.**

**D6 — HTTP/1.1 ONLY.** H2 is out of scope and its shape is now MEASURED
(§1.7); it stays CF-110-1. The seam placement in D5 is what keeps H2 genuinely
untouched — verify it (§6 W-4).

**Also in scope:** unit + mutation-targeted tests for D1/D2/D3/D4 and for the
D5 seam at EVERY covered site, and regression-equivalence over all 88 existing
differential fixtures (nothing should move — §1.8 and the blast-radius
measurement in §4).

## §4. Blast radius — MEASURED ZERO

`grep -rn "application/grpc\|grpc-status\|grpc-message\|grpc_status\|te: trailers"` over:

- **all 88 fixture directories** → **zero hits**;
- **all 112 non-fixture files under `tests/`** (harness `src/` + all 88
  integration test files + `conformance/`) → **zero hits**.

**No existing fixture, expectation or test sends a downstream request carrying
a gRPC `content-type` or asserts anything about the response to one.** Nothing
in the differential corpus can RED because of this sub-phase. The only
pre-existing gRPC surface is the OUTBOUND health-check client (§1.8), which is
orthogonal — it builds gRPC requests toward upstreams and parses their
trailers, and its unit tests use in-process fake servers.

## §5. Non-goals — do NOT widen into these

1. **Any differential fixture.** Fixture `0089` belongs to sibling `110.2`.
2. **HTTP/2 gRPC local replies** — CF-110-1, shape measured in §1.7.
3. **Trailer reading, forwarding or emission of any kind.** The tree's trailer
   gap stays exactly as it is. If a task finds itself needing a trailer API,
   the scope is wrong.
4. **Proxied/upstream-originated gRPC responses** — CF-110-2. This sub-phase
   transforms LOCALLY GENERATED replies only.
5. **`envoy.filters.http.grpc_web`, the gRPC bridge, gRPC-JSON transcoding,
   `grpc_stats`** — the four §9-named data-path items, all still
   trailer-blocked.
6. **`grpc_status_filter` (access-log)** — reads the response TRAILER status;
   already measured and rejected as a vacuous differential at ADR-0154
   DECISION 7.
7. **`fault.grpc_status` abort** — the deferral at `bootstrap.rs:1296` stays.
8. **Any new config surface** — no field, no validator, no `ConfigError`
   variant. If the PLAN discovers one is unavoidable it must say so EXPLICITLY
   and add a `parse_bootstrap` corpus seed WITH the `.gitignore` `!` line,
   proven tracked via `git ls-files`.
9. **The `location`-on-`direct_response` divergence — NEW, banked as
   CF-110-3.** MEASURED at the split session: upstream emits a
   `location: <scheme>://<authority><path>` header on a `direct_response` whose
   status is **201 or 301**, in BOTH the gRPC and the control direction.
   envoy-rust's `synth_direct_response` does not. **This is PRE-EXISTING and
   ORTHOGONAL to gRPC** — it is not caused or fixed here. Its only bearing on
   this family is that sibling `110.2`'s fixture MUST NOT use a `201` or `3xx`
   `direct_response` cell, or it will RED for a reason unrelated to gRPC.
   (A `redirect:` route probe is fine — envoy-rust's `synth_redirect` already
   emits `location`.)

## §6. PLAN-VERIFY items — re-confirm FRESH at this sub-phase's state-2

- **W-1** — re-run the load-bearing cells of §1.1 (the eight special statuses
  plus at least two default-arm witnesses) and all of §1.2 against the pinned
  image. Verify the image digest with `docker image inspect` BEFORE probing and
  tear every container down after.
- **W-2** — re-derive the §1.5 seam census by TEXT: the FOUR direct
  `synth_with` callers, the three depth-2 wrappers, `synth_redirect`, the
  `run_attempt`/pool/`serve_connection` sites and the FOUR `uring.rs`
  `write_owned` sites. Line numbers WILL have drifted.
- **W-3** — re-confirm §1.6 on disk: that `envoy-http2/src/hcm.rs` calls
  `envoy_http1::build_response`, and therefore that the chosen seam placement
  does not transform any H2 response. This is the constraint that shapes D5;
  do not take it on trust from this document.
- **W-4** — after implementing, PROVE H2 is untouched: an in-process H2 test
  that sends a gRPC `content-type` to a `direct_response` route and asserts the
  response is UNtransformed (still the configured status, still `text/plain`,
  no `grpc-status`). Without this, the D5 constraint is unwitnessed.
- **W-5** — re-confirm the §1.3 boundary cells (`~` → `%7E`, `0x7F` → `%7F`,
  `"` and `\` pass through, `%25` → `%2525`) before freezing D3. The parent
  SPEC's rule was wrong here; do not re-inherit it.
- **W-6** — re-derive the §4 blast radius (zero hits across fixtures and the
  non-fixture test tree) before assuming regression-equivalence.
- **W-7** — confirm the `Response` type still exposes `headers:
  Vec<(String, String)>` and `body: Bytes` as the transform assumes, and that
  `serialize_response_head` still emits headers in vector order.

## §7. Size estimate

Bottom-up, docs-excluded, measured as `added − deleted` (the metric the four
landed calibration phases were measured under):

| bucket | estimate |
|---|---|
| D1 + D2 + D3 pure functions (non-test) | ≈ 90 |
| D4 transform (non-test) | ≈ 60 |
| D5 seam threading across both funnels (non-test) | ≈ 110 |
| Unit tests for D1/D2/D3/D4 | ≈ 390 |
| Seam tests at every covered site + the W-4 H2 negative | ≈ 250 |
| **Total** | **≈ 900 (range 780–1050)** |

Comfortably under the ~1500 gate. The calibration warns that the TEST half is
the dominant and least predictable term (`109.1` landed 81% test and overran
its PLAN projection by +46%), so the range's top end is the honest planning
number.

## §8. Definition of done — the §7.5 gate, instantiated

- (a) No new differential fixture — none is in scope. N/A.
- (b) All 88 pre-existing differential fixtures still green
  (CI-authoritative for the backend-routing ones).
- (c) Conformance unchanged — h2spec threshold untouched,
  `known-failures.txt` untouched at 21 lines / ONE real entry.
- (d) **No new fuzz target is required** — no parser, no codec, no filter, no
  config surface (§7.4's trigger does not fire). The five existing targets stay
  green.
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`,
  `cargo test --workspace` and `cargo deny check` all clean at WORKSPACE scope.
- (f) `REVIEW.md` APPROVED.

## §9. Carry-forwards

- **CF-110-1** — HTTP/2 gRPC-aware local replies are UNBUILT. Their upstream
  shape is now **MEASURED** (§1.7): headers-only, no trailers, and `content-length`
  OMITTED rather than `0`. Covering H2 needs both the shared `build_response`
  path and H2's own `synth_h2_*` family.
- **CF-110-2** — proxied (upstream-originated) gRPC responses are untransformed
  and unmeasured; the gRPC bridge surface.
- **CF-110-3 (REASSIGNED)** — upstream emits `location` on a `direct_response`
  with status `201` or `3xx`; envoy-rust does not. Pre-existing, orthogonal to
  gRPC, measured at the split session (§5 non-goal 9).
- CF-109-1 (WIDENED)/2/3, CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6,
  CF-72-2/CF-75-1, M71-6, CF-74-1/2/3/4/6, CF-73-1, the `109.2` REVIEW's
  M-1…M-8 + N-1…N-11, the `109.1` M-5 + N-1…N-6 set, the `108.2` M-2 +
  N-1…N-6 set and the HTTP-filters-family (1)-(4) are ALL unchanged and none is
  fixed here (§6.3; ADR-0165).

## §10. Next state

This sub-phase sits at §5 **state 1 complete** (`SPEC.md` exists, no
`PLAN.md`). The next session runs §5 **state 2** (`superpowers:writing-plans`)
for `110.1`, a SEPARATE session per §5.1 and ADR-0127, re-confirming
W-1…W-7 fresh. Sibling `110.2` (fixture `0089` + the `BEHAVIOR_CONTRACT.md`
`## gRPC` section + the parent-110 close) follows only after `110.1` is `done`.
