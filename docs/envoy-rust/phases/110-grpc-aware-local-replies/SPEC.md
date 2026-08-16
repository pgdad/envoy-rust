# Phase 110 — gRPC family OPENER: gRPC-aware LOCAL REPLIES over HTTP/1.1 — the `application/grpc` request-detection rule, the HTTP→`grpc-status` mapping table, and `grpc-message` percent-encoding, witnessed by NEW cluster-free fixture `0089`

> Brainstorming output of the §5 state-0/1 next-phase pick session (ADR-0177).
> Written for a reader with ZERO prior context (D-3.4). Every inherited figure
> here is a CLAIM the state-2 PLAN-write must re-derive on disk before relying
> on it (§7); every upstream behaviour cited as MEASURED was probed against the
> pinned image at this session and is tabulated in §1.1–§1.3.

## §0. How to read this document

- §1 is the recon evidence: the upstream probe matrices (§1.1 the status
  mapping, §1.2 the detection rule, §1.3 the `grpc-message` encoding) and the
  tree census (§1.4). The probes were run by the MAIN session against
  `envoyproxy/envoy:v1.33.0`; the census was produced by read-only subagents
  and then RE-VERIFIED on disk by the main session, which corrected three of
  its figures (§1.5).
- §2 argues the pick against the cheapest-strong-differential bar, and — the
  load-bearing part — states exactly why the STANDING REJECTION of this family
  (ADR-0048, re-affirmed at ADR-0171(b) and ADR-0175) does not reach this
  slice.
- §3 is the scope (design decisions D1–D7); §5 the non-goals; §6 the
  carry-forward ledger.
- §7 lists the PLAN-VERIFY items the state-2 session must re-confirm fresh.
- §9 sizes the phase and projects the §6.1 split decision (ADR-0178 is
  RESERVED-UNFIRED for it).
- **This phase opens a ZERO-ROW family.** It is the first row ever placed under
  `### gRPC family`, and it takes stop-condition leg (iii) from THREE zero-row
  families to TWO.
- **This phase adds NO configuration surface.** It changes how locally
  generated responses are written. That is unusual for this project and it is
  what makes it cheap — there is no new config field, no new
  `deny_unknown_fields` arm, and no new fuzz target (§3 D7, §10 (d)).

## §1. State-0 recon — the evidence this pick rests on

### §1.1 The HTTP→`grpc-status` mapping matrix (MEASURED at this session, pinned image)

Probe harness: one HCM listener; one `direct_response` route per HTTP status,
each with a distinct body `B<status>`; plus an unmatched path exercising the
HCM's own 404 local reply. Docker port-mapped (`-p`, never `--network host`);
admin `/ready` awaited before probing; the image digest was verified as
`sha256:56da5afd…70c2` against `ENVOY_TARGET.md` before any probe. Every
request carried `content-type: application/grpc`; the control column repeats
the same request WITHOUT that header.

| direct_response status | gRPC request → | control (no gRPC ct) |
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
| (no route match → HCM 404) | `200` + `grpc-status: 12`, **NO `grpc-message`** | `404`, `content-length: 0` |

**The rule the whole phase rests on: the mapping is a SPARSE EIGHT-ENTRY table
over a DEFAULT of 2.** Only `400→13`, `401→16`, `403→7`, `404→12`, `429→14`,
`502→14`, `503→14`, `504→14` are special; **every other status maps to 2
(UNKNOWN)**, including the whole 2xx/3xx range and — counter-intuitively —
`500`, `501`, `405`, `409`, `412`, `413` and `499`.

On every gRPC-detected reply the wire shape is uniform and fully determined:

- HTTP status is rewritten to **`200`** regardless of the configured status;
- `content-type` is rewritten to **`application/grpc`**;
- the body is **DROPPED** and `content-length` becomes **`0`**;
- `grpc-status` carries the mapped code;
- `grpc-message` carries the ORIGINAL body text, percent-encoded (§1.3), and is
  **ABSENT ENTIRELY when the original body is empty** (measured on both the
  `direct_response` with no `body:` and the HCM's own unmatched-path 404).

Everything is a header. **`content-length: 0` on HTTP/1.1 means there is no
body and therefore no trailer section — nothing in this surface is a trailer.**
That single measured fact is what §2 turns on.

### §1.2 The gRPC-detection matrix (MEASURED at this session)

All against the same `direct_response` 404 route; `→ gRPC` means the transform
of §1.1 fired.

| request `content-type` | detected? |
|---|---|
| `application/grpc` | **YES** |
| `application/grpc+proto` | **YES** |
| `application/grpc+json` | **YES** |
| `application/grpc; charset=utf-8` | **NO** — a parameter DEFEATS detection |
| `APPLICATION/GRPC` | **NO** — the match is CASE-SENSITIVE |
| `application/grpc-web` | **NO** — gRPC-Web is a separate surface |
| `application/grpc-web+proto` | **NO** |
| `application/grpcfoo` | **NO** — not a bare prefix match |
| `application/json` | NO (control) |
| *(header absent)* | NO (control) |

**Derived rule: detected iff `content-type` is EXACTLY `application/grpc` or
begins with `application/grpc+`.** Nothing else. Two independent traps live
here and both are directly witnessable: a naive
`starts_with("application/grpc")` wrongly detects `application/grpcfoo` and
`application/grpc-web`, and a naive case-insensitive or parameter-tolerant
match wrongly detects `APPLICATION/GRPC` and `application/grpc; charset=utf-8`.

Also measured: detection is **METHOD-INSENSITIVE** (`GET`, `POST` and `PUT`
all transform identically) and **INDEPENDENT of `te: trailers`** — sending
`te: trailers` changes nothing, and sending it WITHOUT a gRPC content-type
does not trigger the transform.

### §1.3 The `grpc-message` encoding (MEASURED at this session)

A `direct_response` 404 whose body is the 25-byte string
`a b\ncontrol\ttab é %25 end` produced exactly:

```
grpc-message: a b%0Acontrol%09tab %C3%A9 %2525 end
```

with the control (non-gRPC) request returning that body byte-for-byte
(`xxd`-confirmed: `61 20 62 0a 63 6f 6e 74 72 6f 6c 09 74 61 62 20 c3 a9 20 25 32 35 20 65 6e 64`).

Derived rule: **SPACE (0x20) is PRESERVED**; `\n` (0x0A) → `%0A`; `\t` (0x09) →
`%09`; the UTF-8 bytes of `é` (0xC3 0xA9) → `%C3%A9` **per byte**; and a
literal `%` → `%25`, so the input `%25` renders as `%2525`. Hex digits are
UPPERCASE. This is the gRPC spec's `Status-Message` percent-encoding, and the
`%`-doubling cell is the one a hand-rolled encoder most often gets wrong.

### §1.4 The tree census (subagent recon, RE-VERIFIED on disk by the main session)

- **envoy-rust has NO gRPC handling anywhere on the proxy data path.** The
  only `grpc-status` hits in `crates/` are `crates/envoy-http2/src/grpc.rs`
  (8) and `crates/envoy-health/src/probe.rs` (6), and every one belongs to the
  active **health-check CLIENT** or its tests. `application/grpc` appears only
  as an OUTBOUND header on that client (`grpc.rs:183`) and in a test server
  (`probe.rs:539`). There is no HTTP→gRPC status mapping and no request-side
  gRPC detection: `grep -rni 'grpc_status\|GrpcStatus' --include=*.rs crates/`
  outside those two files yields exactly ONE hit, the deferral comment at
  `crates/envoy-config/src/bootstrap.rs:1296`. **Every cell of §1.1–§1.3
  currently diverges.**
- **The local-reply seam is a SINGLE funnel.** `synth_with(status, body, close)`
  (`crates/envoy-http1/src/hcm.rs:2239`) builds the five-header response that
  every H1 synth path returns, and it is called by `synth_direct_response`
  (`:2260`), `synth_status` (`:2269`), `synth_no_healthy_upstream` (`:2409`),
  `synth_overflow` (`:2424`), `synth_400` (`:2522`), `synth_404` (`:2525`) and
  `synth_501` (`:2528`). Its doc block warns that **header ORDER is
  load-bearing** because the differential harness byte-compares against
  upstream. **`synth_redirect` (`:2383`) deliberately does NOT reuse
  `synth_with`** (documented at `:2378`) — that exception is UNMEASURED here
  and is PLAN-VERIFY item V-3.
- **HTTP/2 is a genuinely separate family of generators** — `synth_h2_*`
  (`crates/envoy-http2/src/hcm.rs:192`, `:301`, `:391`, `:404`, `:646`, `:1205`
  and neighbours). H2 shares the ROUTE RESOLVER (`envoy-http2/src/hcm.rs:475`
  calls `envoy_http1::hcm::resolve_route`) but NOT the response synthesiser, so
  scoping this phase to H1 is a real boundary rather than a hopeful one (§3 D6).
- **The differential harness needs ZERO new machinery**, verified field by
  field rather than assumed:
  - `Driver::Http1ProbeList { probes: Vec<Http1Probe> }` —
    `tests/differential/src/lib.rs:119-121`, spelled `kind: http1_probe_list`
    in fixture YAML (`#[serde(rename_all = "snake_case")]`, `:37`), used by
    **14** fixtures.
  - `Http1Probe` carries **`extra_headers: Vec<(String, String)>`** (so the
    probe can SEND `content-type: application/grpc`), plus `expected_status`,
    `expected_body: Option<Http1BodyRule>` and
    `expected_headers: Option<Http1HeaderRule>`.
  - `Http1HeaderRule` has exactly one variant, `SetEqualModuloAllowList`, and
    `diff_headers` (`lib.rs`) asserts **(1)** the lower-cased header NAME SETS
    are equal and **(2)** for every name NOT in the allow-list, the VALUES are
    equal string-for-string. `HEADER_ALLOW_LIST` (`lib.rs:1189-1193`) holds
    exactly THREE `NameRequired` entries — `server`, `date`,
    `x-envoy-upstream-service-time`. **`grpc-status`, `grpc-message`,
    `content-type` and `content-length` are all OUTSIDE it and are therefore
    value-compared exactly.** A wrong code, a wrong encoding, a missing header
    or a spurious one each go RED.
  - `Http1BodyRule::ByteExact { body: String }` pins the empty body.
- **A cluster-free, backend-free template already exists.** Fixture `0088`
  carries `clusters: []` (`envoy.yaml:109`), its `envoy.yaml` and
  `envoy-rust.yaml` are **byte-identical** (md5 `d205936b…f51a` on both), and
  its only template token is `{{PORT}}` (`:5`). `0086` has the same shape.
  Backend-free fixtures are the ones that actually run GREEN on this
  development host, where backend-routing fixtures go RED on the
  `192.168.65.2` bridge and CI is the only authority.
- **Fixture numbering**: `git ls-files 'tests/fixtures/**' | cut -d/ -f3 |
  sort -u | wc -l` = **88**, highest `0088`, so this phase's fixture is
  **`0089`**. (The naive `git ls-files 'tests/fixtures/*/'` is a vacuous glob
  returning a clean-looking ZERO — not used here.)

### §1.5 Corrections this session made to inherited or delegated figures

Recorded because each would otherwise propagate (a handed count is a claim):

1. **The ROADMAP pipe-trap claim is part-right and part-wrong — and this
   SPEC's first attempt to correct it was itself wrong, so the corrected
   measurement is stated here in full.** MEASURED four ways over all 114
   rows: **9** rows carry an in-cell pipe (raw `|` count != 7 — `36`, `38`,
   `39`, `52`, `54`, `66`, `70`, `76`, `108`), so the inherited count of NINE
   is CORRECT; **5** of them escape at least one as `\|` (`38`, `66`, `70`,
   `76`, `108`), so the word UNESCAPED overstates it and "(incl. `76` and
   `108`)" MISLABELS those two, which are escaped and split cleanly; only
   **2** (`38`, `39`) actually mis-split on `' | '`; and **field 4 holds a
   valid status on all 114 rows**. A raw-pipe census and a `' | '`-split
   census answer DIFFERENT questions — filtering on "has `\|` OR mis-splits"
   silently misses `36`/`52`/`54`, which is how the understated "6" arose.
   ADR-0177's corresponding item carries that understated figure and is
   superseded by this one; the ADR is append-only and was not rewritten.
   The operational rule is unaffected: split on `' | '` WITH surrounding
   spaces, read status as FIELD 4, and never "fix" the rows.
2. **`Http1ProbeList` is used by 14 fixtures, not 13** (`109/SPEC.md:119`'s
   figure went stale when `0088` landed).
3. A subagent reported `Http1ProbeList` usage as "1 fixture" by grepping the
   Rust type name against fixture YAML; the YAML spelling is
   `kind: http1_probe_list`, which is the 14. Stated so the next session does
   not re-derive a believable zero from the wrong spelling.

## §2. Why this surface — the cheapest-strong-differential argument

### §2.1 Why the standing rejection of this family does not reach this slice

**This is the load-bearing paragraph of the pick, and it is a REFUTATION, so it
is stated with its evidence.** ADR-0048 rejected the gRPC family as
"blocked on a missing prerequisite … gRPC requires HTTP/2 trailer propagation
(the `grpc-status`/`grpc-message` trailers), and the code survey confirmed
trailers are discarded today … A gRPC family opener would have to be 'H2/H1
trailer plumbing'." That verdict has been re-affirmed roughly fifteen times,
most recently as ADR-0171 rejected-alternative (b) and again at the ADR-0175
pick. **The trailer premise is TRUE for the gRPC DATA PATH and FALSE for this
slice**, and the falsifying measurement is §1.1: every gRPC-detected local
reply is `content-length: 0` over HTTP/1.1, with `grpc-status` and
`grpc-message` delivered as ordinary **response HEADERS**. A response with no
body has no chunked framing and therefore no trailer section at all. The
`te: trailers` request header was measured to be irrelevant in both directions
(§1.2). **Nothing in §1.1–§1.3 requires reading, forwarding, or emitting a
single trailer**, and the tree's trailer gap (real, and confirmed: `envoy-http1`
discards them, `envoy-http2` has no trailer API) is untouched by this phase.

This is precisely the ADR-0171 DECISION 5 pattern — an inherited practice or
verdict is overturned only by a fresh measurement that contradicts it, and the
overturning is written down rather than quietly assumed.

### §2.2 The five properties, each measured

1. **It opens a ZERO-ROW family.** `ROADMAP.md:126` is the bare `### gRPC
   family` heading with a one-line prose summary, no table header and no rows.
   Landing row 110 there takes stop-condition leg (iii) from THREE zero-row
   families to TWO. **No non-family-opening candidate can move that leg at
   all**, and of the three zero-row families this is the only one whose opener
   needs no new dependency and no new subsystem (§2.3).
2. **Zero new dependencies and zero new harness machinery.** No `tonic`, no
   `prost`, no `quinn`, no `wasmtime`, no PRNG, no protobuf. The driver, the
   probe struct's `extra_headers`, the header rule and the cluster-free
   `direct_response` fixture shape all exist today (§1.4), verified field by
   field. `Cargo.toml` and `Cargo.lock` are untouched.
3. **Backend-free, cluster-free and fully deterministic.** Every cell is a
   static `direct_response` (or the HCM's own local reply) evaluated with no
   upstream, no clock, no sampling and no concurrency — so fixture `0089` is
   verifiable ON THIS DEVELOPMENT HOST, unlike any backend-routing fixture.
4. **One clean seam.** `synth_with` (`hcm.rs:2239`) is the single funnel every
   H1 local reply already passes through (§1.4), and H2's separate `synth_h2_*`
   family makes the H1-only boundary structural rather than aspirational.
5. **The divergence is total and the traps are sharp.** envoy-rust implements
   NONE of §1.1–§1.3 today, so every probe is a real witness rather than a
   confirmation of existing behaviour; and the detection rule (§1.2) plus the
   `%`-doubling encoder cell (§1.3) are exactly the cells a plausible
   implementation gets wrong.

### §2.3 What was rejected, and the measurement that killed each

- **(a) HTTP/3 + QUIC family opener.** Rejected on THREE independently
  measured blockers, not on general heaviness. (i) `crates/envoy-config/src/
  bootstrap.rs:3659-3666` caps merged listeners at **one**
  (`ConfigError::TooManyListeners`), so an accepted-but-inert UDP listener
  would leave the fixture with no TCP listener to probe. (ii) There is **no
  transport-socket abstraction** — `crates/envoy-tls/src/lib.rs:159-162` is
  `TcpStream`-concrete in both directions, so QUIC has nothing to plug into.
  (iii) **ALPN is not wired at all** (`alpn_protocols` is actively REJECTED by
  a test at `bootstrap.rs:8854-8883`), and QUIC mandates ALPN `h3`. `quinn` is
  doctrinally pre-approved (D-3.2) and absent from `Cargo.lock`, so the
  dependency is not the obstacle — the three missing subsystems are. Correctly
  rejected, now with specific citations rather than the inherited one-liner.
- **(b) The gRPC DATA path** (bridge, gRPC-Web, JSON transcoding, `grpc_stats`
  success/failure counters). Still correctly rejected on ADR-0048's grounds,
  which §2.1 leaves fully intact: they need trailers, and additionally the
  filter API is **headers-only** — `crates/envoy-filter/src/pipeline.rs`
  exposes `decode_headers` (`:88`) and `encode_headers` (`:105`) and there is
  no `decode_data`/`encode_data`/`decode_trailers` hook anywhere.
- **(c) WASM host family.** Unchanged: `ROADMAP.md:196` describes it as "its
  own multi-phase sub-project", and it needs an engine dependency that D-3.2's
  permitted-foundations list does not name — an ADR before a line of code.
- **(d) Continuing the Runtime family** — the `RuntimeUInt32`
  (`status_code_filter`) honoring consumer is the strongest of these and is
  genuinely cheap: `HCMConfig::from_config` already carries
  `runtime: Arc<RuntimeSnapshot>`. **Rejected because it moves leg (ii) only
  and leaves leg (iii) at three zero-row families**, and because its own
  pick-killing question is unmeasured (how upstream coerces a non-numeric or
  out-of-range `final_value` for a `RuntimeUInt32`). It remains the strongest
  non-family-opening candidate and its position is recorded, not lost.
- **(e) CSRF `filter_enabled` runtime honoring.** Same leg-(iii) objection,
  plus an unmeasured premise: upstream's `featureEnabled` overload for a
  `RuntimeFractionalPercent` may honor the config's OWN denominator rather
  than HUNDRED, in which case the "reuse `route_fraction_gate`" claim
  collapses into a fresh multi-cell measurement.
- **(f) Hot restart / graceful drain.** The drain MECHANISM exists
  (`crates/envoy-listener/src/drain.rs`, `POST /drain_listeners`) but has **no
  config surface at all** (`grep -n "drain" crates/envoy-config/src/
  bootstrap.rs` → zero hits) and the 5s budget is a hard-coded constant
  (`envoy-listener/src/lib.rs:22`). Upstream's `drain_time_s` is a
  COMMAND-LINE option, and `envoy-bin` parses only `-c/--config-path` — so the
  slice is not differentially witnessable by the existing fixture harness.
- **(g) The drafted repo-health phase `110`** in the gitignored operator
  scratch `.claude/drafts/` (the `STATE.md` traps-line trim). A legitimate
  candidate, and its problem is real and growing — but it lights up NO
  differential fixture, which scores lowest on the cheapest-strong-differential
  bar, and it moves neither mission leg. It stays available; this phase takes
  its provisional id, which was never reserved.
- **(h) `sni_cluster`** — unchanged and still correctly rejected: it needs a
  `tls_inspector` LISTENER-filter subsystem the tree wholly lacks
  (`Listener.listener_filters` is parse-and-ignore, `bootstrap.rs:609-615`).

## §3. Scope — what this phase builds (design decisions D1–D7)

**D1 — gRPC request detection, as a pure total function.** A helper (natural
home: `crates/envoy-http1/src/hcm.rs` beside the synth family, or a small
sibling module) answering `is_grpc_request(headers) -> bool` by the §1.2 rule:
the request `content-type` value is EXACTLY `application/grpc`, or it begins
with `application/grpc+`. **Byte-exact and CASE-SENSITIVE on the value**; no
parameter tolerance; no trimming beyond the HTTP header-value handling the
codec already performs. Header-NAME lookup remains case-insensitive as
everywhere else in the tree. No config surface, no new `ConfigError`.

**D2 — the mapping table, as a pure total function.** `http_to_grpc_status(u16)
-> u8` implementing §1.1: the eight special entries `400→13`, `401→16`,
`403→7`, `404→12`, `429→14`, `502→14`, `503→14`, `504→14`, and **2 for
everything else**. A `match` with an explicit `_ => 2` arm, unit-pinned against
every cell in §1.1 including the counter-intuitive ones (`500`, `501`, `405`,
`409`, `412`, `413`, `499` and the entire 2xx/3xx range → 2).

**D3 — the local-reply transform.** When D1 is true, the response produced by
the synth path becomes, per §1.1: status `200`; `content-type:
application/grpc`; body dropped; `content-length: 0`; `grpc-status: <D2>`; and
`grpc-message: <D4(original body)>` **only when the original body is
non-empty** — the header is ABSENT, not empty, otherwise. The five-header
order contract documented at `hcm.rs:2234-2238` is load-bearing and the PLAN
must decide the exact resulting order and pin it; §1.1's probes captured the
header SET and values, and V-2 requires the ORDER to be re-measured before the
fixture is frozen.

**D4 — `grpc-message` percent-encoding, as a pure total function.** Per §1.3:
encode each BYTE of the original body; bytes `0x20` (space) through `0x7E`
pass through UNCHANGED **except `%` (0x25), which becomes `%25`**; every other
byte becomes `%` + two UPPERCASE hex digits. Multi-byte UTF-8 is encoded
per-byte (`é` → `%C3%A9`). Unit-pinned on the exact §1.3 string, whose
`%25`→`%2525` cell is the discriminating case.

**D5 — the seam (constraint stated here; the CHOICE belongs to the PLAN).**
The transform needs one bit of REQUEST state (`is_grpc_request`) at a point
where the current signatures carry none: `synth_with(status, body, close)`
(`hcm.rs:2239`) and its seven callers are request-agnostic. The PLAN must pick
between threading the flag through the synth family, applying the transform
once at the single point where `BuildOutcome::Synth` is turned into the wire
response, or a post-pass over the built `Response`. **The binding constraint is
that ALL SEVEN `synth_with` callers must be covered identically** — a
partially-covered family is exactly the silent divergence class ADR-0049
exists to prevent — **and that `synth_redirect` (`:2383`), which does NOT go
through `synth_with`, must be explicitly decided rather than accidentally
excluded** (V-3).

**D6 — HTTP/1.1 ONLY, and the boundary is structural.** H2 owns a separate
`synth_h2_*` generator family (§1.4), so nothing in D5 silently leaks into it.
H2's gRPC local-reply shape is UNMEASURED (§8) — on H2 a headers-only response
may legitimately be a trailers-only frame sequence, which would re-engage the
trailer blocker §2.1 sidesteps. Scoping to H1 keeps this phase trailer-free by
construction. Opened as **CF-110-1**.

**D7 — fixture `0089-grpc-aware-local-replies`.** Cluster-free, backend-free,
`clusters: []`, `envoy.yaml` ≡ `envoy-rust.yaml` byte-identical, single
`{{PORT}}` token — the `0088`/`0086` template. One HCM listener; a route table
of `direct_response` routes, **each with its OWN distinct path** (the
BEHAVIOR_CONTRACT §G one-path-per-probe attribution rule) and its own body;
`kind: http1_probe_list`. Probes, one per §1.1/§1.2/§1.3 equivalence class and
all deterministic — the mapped statuses (`400`/`401`/`403`/`404`/`429`/`503`
at minimum, plus at least two default-arm witnesses such as `500` and `200`),
the paired NON-gRPC controls proving the transform does NOT fire, the
detection edges (`application/grpc+proto` positive; `application/grpc;
charset=utf-8`, `APPLICATION/GRPC`, `application/grpc-web` and
`application/grpcfoo` negative), the empty-body probe proving `grpc-message` is
ABSENT, and one `grpc-message` encoding probe carrying the §1.3 string. Each
probe sets `expected_status`, `expected_body: ByteExact { body: "" }` for the
gRPC cells, and `expected_headers: SetEqualModuloAllowList` — which, per §1.4,
value-compares `grpc-status`, `grpc-message`, `content-type` and
`content-length` exactly.

Also in scope: unit + mutation-targeted tests for D1/D2/D4 and the D5 seam at
every covered caller; a `## gRPC` section in `BEHAVIOR_CONTRACT.md` recording
§1.1–§1.3 as the canonical contract; and regression-equivalence over all 88
existing fixtures. **NOT in scope: any new fuzz target** — this phase adds no
parser and no config surface (§10 (d)).

## §4. Differential surface at phase end

- NEW fixture `0089-grpc-aware-local-replies` green cross-proxy
  (`http1_probe_list`, backend-free — locally runnable on this host).
- All 88 pre-existing differential fixtures still green. The CI identity
  `binaries=165, passed=2194, failed=0` moves only by this phase's new tests.
- No conformance-suite change: h2spec threshold untouched,
  `known-failures.txt` untouched at 21 lines / ONE real entry. No `tests/
  conformance/grpc/` is created — interop conformance is a later slice (§5).

## §5. Non-goals — do NOT widen into these

1. **HTTP/2 gRPC local replies** — CF-110-1 (D6). Unmeasured; may require
   trailers.
2. **Trailer reading, forwarding or emission of any kind** — the tree's trailer
   gap is real and stays exactly as it is. This phase must not add a trailer
   API; if a task finds itself needing one, the scope is wrong.
3. **Proxied/upstream gRPC responses.** This phase transforms LOCALLY GENERATED
   replies only. A response that came from an upstream is untouched — that is
   the gRPC bridge's surface, not this one.
4. **`envoy.filters.http.grpc_web`, the gRPC bridge, gRPC-JSON transcoding,
   `grpc_stats`** — the four §9-named data-path items, all still trailer-blocked
   (§2.3(b)).
5. **`grpc_status_filter` (access-log)** — reads the response TRAILER status;
   already measured and rejected as a vacuous differential at ADR-0154
   (DECISION 7 — its wire shape was measured, including the one-L enum
   spelling `CANCELED`, and it was rejected because it reads the gRPC response
   TRAILER status, which envoy-rust has no data path for).
6. **`fault.grpc_status` abort** — the deferral at `bootstrap.rs:1296` stays.
7. **gRPC interop conformance** (`tests/conformance/grpc/`) — needs a data path
   first.
8. **Any new config surface.** This phase adds no field, no validator and no
   `ConfigError` variant unless the PLAN discovers one is unavoidable, in which
   case it must say so explicitly.

## §6. Carry-forwards

- **OPENS CF-110-1** — HTTP/2 gRPC-aware local replies are UNBUILT and their
  upstream wire shape is UNMEASURED (headers-only vs trailers-only). *Unblocked
  by* a measurement phase; may pull in trailer plumbing.
- **OPENS CF-110-2** — proxied (upstream-originated) gRPC responses are
  untransformed and unmeasured; the gRPC bridge surface.
- **POSSIBLY OPENS CF-110-3** — the `synth_redirect` path (`hcm.rs:2383`),
  which bypasses `synth_with`. If V-3 measures upstream as transforming a
  `RedirectAction` reply for a gRPC request and this phase does not cover it,
  the gap is banked here rather than left silent.
- **CONTRIBUTES to the record without touching them:** the three-blocker
  citation set for the HTTP/3 opener (§2.3(a)) and the unmeasured
  pick-killing questions for the `RuntimeUInt32` (§2.3(d)) and CSRF (§2.3(e))
  candidates — each improves the next pick session's position on a candidate
  this session did not take (the ADR-0168/ADR-0171 contribution pattern).
- CF-109-1 (WIDENED)/2/3, CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, CF-72-2/
  CF-75-1, M71-6, CF-74-1/2/3/4/6, CF-73-1, the `109.2` REVIEW's M-1…M-8 +
  N-1…N-11, the `109.1` M-5 + N-1…N-6 set, the `108.2` M-2 + N-1…N-6 set and
  the HTTP-filters-family (1)-(4) are ALL unchanged by this pick and none is
  fixed here (§6.3; ADR-0165).

## §7. PLAN-VERIFY items — re-confirm FRESH at the state-2 PLAN-write

- **V-1** — re-run the load-bearing cells of §1.1 (the eight special statuses
  plus at least two default-arm witnesses) and all of §1.2 against the pinned
  image before transcribing any expectation into the fixture. The matrix is
  banked here; the fixture's exact YAML must still be dry-run end-to-end (the
  108.2 precedent: a dry-run is a CLAIM state 3 re-establishes).
- **V-2** — **MEASURE THE HEADER ORDER**, not just the set. §1.1 captured names
  and values; `hcm.rs:2234-2238` warns order is load-bearing and the harness
  byte-compares. Determine where `grpc-status`/`grpc-message` sit relative to
  `server`/`date`/`content-length`/`content-type`/`connection` before freezing
  D3.
- **V-3** — measure the `synth_redirect` path: does upstream apply the §1.1
  transform to a `RedirectAction` reply for a gRPC-detected request? Decide
  whether it is in scope or becomes CF-110-3. It is the one H1 synth path that
  bypasses `synth_with`.
- **V-4** — enumerate ALL callers of `synth_with` fresh (measured: seven at
  `:2260/:2269/:2409/:2424/:2522/:2525/:2528`) and confirm the D5 seam covers
  every one identically. Line numbers WILL have drifted; locate by TEXT.
- **V-5** — confirm H2 is genuinely untouched by the chosen seam: re-derive
  that `envoy-http2` uses its own `synth_h2_*` generators and that the shared
  `resolve_route` path carries no response synthesis.
- **V-6** — re-derive the §9 size estimate bottom-up and decide the §6.1 split
  (ADR-0178 reserved). Measure against LANDED phases, not this SPEC's
  projection (the 76.1 +50% / 109.1 +46% / 109.2 +52–58% lesson).
- **V-7** — confirm no EXISTING fixture or test asserts the current
  (non-transforming) behaviour for a request carrying a gRPC content-type;
  grep the fixture tree for `application/grpc` before assuming zero blast
  radius.
- **V-8** — measure the `grpc-message` encoding on at least one additional
  class before freezing D4: a body containing a `"` or a `\` , and a body
  containing a raw 0x7F byte, to confirm the "0x20–0x7E pass through except
  `%`" rule at both boundaries.
- **V-9** — decide and record whether `grpc-message` is emitted for the HCM's
  own local replies that DO carry a body (e.g. `synth_no_healthy_upstream`'s
  19-byte body, `synth_overflow`'s 81-byte body) — §1.1 measured
  `direct_response` bodies and the EMPTY 404, not these.
- **V-10** — confirm the fixture's chosen `content-type` negative cells survive
  the harness's own request-header handling (that `extra_headers` passes
  `APPLICATION/GRPC` through WITHOUT normalising its case, which would
  silently turn a negative cell vacuous).

## §8. NOT MEASURED — stated explicitly per D-3.4

- **HTTP/2 behaviour of every cell in §1.1–§1.3** (D6, CF-110-1). Headers-only
  vs trailers-only on H2 is the specific unknown.
- The `synth_redirect` / `RedirectAction` path (V-3).
- Whether upstream emits `grpc-message` for its own bodied local replies
  (V-9).
- Encoding behaviour at the 0x7E/0x7F boundary and for `"`/`\` (V-8).
- Whether any stat ticks on the gRPC local-reply path (no stat surface is in
  scope; none was probed).
- Behaviour when the request carries a gRPC content-type AND the route proxies
  to a real upstream (no backend existed in the probe harness) — CF-110-2.
- `application/grpc-web` handling beyond "does not trigger this transform".

## §9. Size estimate and the §6.1 split gate

Bottom-up: D1 detection + D2 mapping + D4 encoder ≈ 120–180 non-test; D5 seam
threading across seven callers ≈ 100–180 non-test; unit/mutation tests for the
three pure functions and the seam ≈ 350–450; fixture `0089` (two identical
YAMLs + expectations + README) ≈ 450–530 plus a ≈40-line test file;
`BEHAVIOR_CONTRACT.md` `## gRPC` section ≈ 80–110. **Projection ≈ 1140–1490 net
LoC excluding `docs/`.**

That sits just UNDER the ~1500 gate on its face, and the calibration says do
not trust it. Measured at this session from landed phases with
`git diff --numstat <base> <last-task> -- . ':(exclude)docs/'`: `108.1` **1128**,
`108.2` **854**, `109.1` **1726**, `109.2` **562** — median ≈ **991** — against
their own bottom-up projections of ≈1215, ≈905, ≈1180 and ≈745 respectively.
`109.1` overran by **+46%**, `76.1` by **+50%**, `109.2` by **+52…+58%**
whole-tree. Applying even the mildest of those to the low end of the projection
crosses 1500. **The §6.1 split is therefore PROJECTED LIKELY, not merely
possible, and ADR-0178 is RESERVED-UNFIRED for it.** The natural cut follows the
established foundation-slice precedent (`108.1`/`108.2`, `109.1`/`109.2`):
**`110.1`** = D1 + D2 + D4 + the D5 seam, witnessed ENTIRELY IN-PROCESS with no
new fixture, regression-equivalence over all 88 existing fixtures;
**`110.2`** = fixture `0089` + the `BEHAVIOR_CONTRACT.md` `## gRPC` section +
the parent close. **The state-2 PLAN-write owns the decision (V-6) and MUST
stop without writing a `PLAN.md` if it splits** (§6.2 step 7).

## §10. Definition of done — the §7.5 gate, instantiated

- (a) Fixture `0089` green cross-proxy on every probe.
- (b) All 88 pre-existing fixtures green (CI-authoritative for the
  backend-routing ones; `0089` itself is locally verifiable).
- (c) Conformance unchanged — no new suite, h2spec threshold untouched,
  `known-failures.txt` untouched at 21 lines / ONE real entry.
- (d) **No new fuzz target is required** — this phase introduces no parser,
  codec or filter and no config surface (§7.4's trigger does not fire). The
  five existing targets stay green. If the PLAN discovers a config surface is
  unavoidable after all, it must add a `parse_bootstrap` corpus seed WITH the
  explicit `.gitignore` `!` line and prove it tracked via `git ls-files`.
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`,
  `cargo test --workspace` and `cargo deny check` all clean at WORKSPACE scope.
- (f) `REVIEW.md` APPROVED.

## §11. Next state

This SPEC completes §5 state 0/1 for phase 110 (ROADMAP row added
`in-progress` UNDER `### gRPC family` — the first row that heading has ever
carried; directory `docs/envoy-rust/phases/110-grpc-aware-local-replies/` holds
`SPEC.md` ONLY — no `PLAN.md` exists yet and none may be written this session).
The NEXT session runs §5 state 2 (`superpowers:writing-plans`), a SEPARATE
session per §5.1 and ADR-0127, re-confirming V-1…V-10 fresh — most decisively
**V-2** (header ORDER, on which the fixture's byte-comparison rests), **V-3**
(the `synth_redirect` exception) and **V-6** (the split). ADR-0177 records this
pick; **ADR-0178 is RESERVED-UNFIRED** for the projected split.
