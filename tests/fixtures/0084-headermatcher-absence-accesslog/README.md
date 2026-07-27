# Fixture 0084 — `HeaderMatcher` absence semantics on the ACCESS-LOG path: the D1 witness (sub-phase 75.2)

The **D1** cross-proxy witness for the `HeaderMatcher` ABSENCE rule landed by
sub-phase **75.1**, taken on the **ACCESS-LOG** path — the SECOND consumer of the
one shared matching engine, reached through the ADR-0150 `HeaderMatch` trait seam
(`LogFilter::Header { matcher } => matcher.matches(headers)` in
`crates/envoy-accesslog/src/filter.rs` dispatches to
`impl envoy_accesslog::HeaderMatch for HeaderMatcher` in
`crates/envoy-config/src/matcher.rs`, whose trait object is injected by
`compile_access_log_filter` in `crates/envoy-http1/src/hcm.rs`). The sibling
fixture `0085-headermatcher-absence-accesslog-present-polarity` witnesses **D2**;
the ROUTE-path witness of the same rule is `0083-headermatcher-absence-parity`.

## What this proves

One H1 HCM listener, ONE `FileAccessLog` sink gated by
`header_filter: { header: { name: x-a, exact_match: "v", invert_match: true } }`,
and ONE `direct_response` route (`prefix: "/"` → 200 `hi`). Three probes, **each on
its own path**:

| # | request | matcher verdict | emitted? |
|---|---|---|---|
| 1 | `GET /absent`, **no** `x-a` | `(ExactMatch, None)` short-circuits to `false`; `invert_match` NOT applied | **DROPPED** |
| 2 | `GET /valmatch`, `x-a: v` | `"v" == "v"` → `true`; `true ^ true` → `false` | **DROPPED** |
| 3 | `GET /valmiss`, `x-a: zzz` | `"zzz" == "v"` → `false`; `false ^ true` → `true` | **KEPT** |

Probe 1 is the load-bearing one: it is **the D1 cell**, the cell sub-phase 75.1
changed. A pre-75.1 tree computed `mode_result(false) ^ invert_match(true)` = KEEP
and would emit TWO lines here against upstream's ONE, failing the line-count
assertion — which is exactly why 75.2 was gated behind 75.1.

The access-log file on EACH proxy holds EXACTLY ONE byte-identical line:

```
STATUS=200 PATH=/valmiss
```

**The distinct paths are load-bearing, and are what the §5.2 state-3 re-entry added
(review finding I-1).** This driver asserts only (a) a per-side line COUNT and
(b) whole-line cross-proxy equality — there is no per-probe assertion and no
expected-line field. While all three probes shared `path: /x` every probe rendered
the byte-identical `STATUS=200 PATH=/x`, so the fixture could not attribute the
surviving line to a probe, and any regression that MOVED the keep between probes
passed GREEN. MEASURED: an engine with the `invert_match` XOR removed left this
fixture GREEN while turning FOUR in-process assertions RED. With `PATH=` naming the
probe, that same mutation now REDs here with
`envoy="STATUS=200 PATH=/valmiss" envoy-rust="STATUS=200 PATH=/valmatch"`.
`:path` is on the seven-name `REQ_ALLOW_LIST`, so this costs nothing at runtime —
unlike the gating header `x-a`, which is BOOT-FATAL to echo.

Probe 2 covers the value-MATCH half of the XOR, so together with probe 3 the
fixture pins both polarities of `invert_match` on a PRESENT header. It is NOT what
makes probe 1's silence attributable: the line COUNT alone already does that (an
always-log filter yields 3 lines, an always-drop 0), and the distinct paths now
attribute the kept line directly.

## The rule

```
present := the named header is present in the request
           (name matched case-insensitively; an EMPTY VALUE still counts as PRESENT)

if mode is present_match(want):
        result = (present == want) XOR invert_match
else if not present:
        result = false                    # <-- invert_match is NOT applied
else:
        result = mode_matches(value) XOR invert_match
```

`present_match(want)` is the ONLY mode evaluated with the header ABSENT; every
value mode short-circuits to `false` and `invert_match` is NOT applied. An EMPTY
header VALUE counts as PRESENT.

## Why the log line does not echo `x-a`

envoy-rust's `%REQ(NAME)%` command operator is ALLOW-LIST gated — `REQ_ALLOW_LIST`
in `crates/envoy-accesslog/src/command_operator.rs` has exactly seven entries
(`:method`, `:authority`, `:path`, `x-envoy-original-path`, `x-forwarded-for`,
`user-agent`, `x-request-id`) — so a `%REQ(X-A)%` operator is **BOOT-FATAL**
(`ConfigError::InvalidAccessLogFormat`). The gating header therefore cannot appear
in the rendered line. The witness is instead the keep/drop **LINE COUNT** plus
whole-line cross-proxy equality, exactly as in fixture `0078`.

## Probes / driver

`kind: http1_access_log_byte_exact` (`Driver::Http1AccessLogByteExact`, reused
with ZERO harness change). `expected_logged_count` = **1**. Probe ordering follows
the kept-LAST convention (ADR-0147), and **because the LAST probe is KEPT** the
driver's ordering-aware `suppression_settle` charges the cheap 2 s
`CF70_3_SETTLE` rather than the 12 s `CF71_1_SETTLE`. `suppression_settle`
inspects only `probes.last()`, so it is the identity of the LAST probe — not the
position of any other probe — that decides the settle.

The assertion is PURE cross-proxy equality: there is no expected-line field on
this driver. Both proxies must agree on the kept line AND on the ABSENCE of a line
for each dropped probe; a one-sided keep fails the line-count assertion before the
byte compare is reached.

`clusters: []` — the only route is a `direct_response`, so no backend spawns.

## Per-side divergences (`envoy.yaml` ↔ `envoy-rust.yaml`)

| field | `envoy.yaml` | `envoy-rust.yaml` | why |
|---|---|---|---|
| `admin` | present (port 0) | absent | envoy-rust has no admin server in this fixture |
| listener bind | `0.0.0.0` | `127.0.0.1` | envoy-rust binds loopback in-harness |
| `generate_request_id` | `false` | omitted | envoy-rust's `HttpConnectionManagerConfig` is `#[serde(deny_unknown_fields)]` and has NO such field, so writing it on the rust side would be BOOT-FATAL — it is omitted because it is unsupported, not because it is inert. Upstream defaults it ON, hence the explicit `false` there |
| access-log path | `/tmp/0084-envoy-mount/access.log` | `/tmp/0084-envoy-rust-mount/access.log` | per-side mount dirs |

`codec_type: HTTP1` is written on **BOTH** sides and is **NOT** a divergence:
envoy-rust has no serde default for it under `deny_unknown_fields`, while upstream
would default to `AUTO` (ADR-0158 C3). The `header_filter` body, the log format and
the route table are byte-identical between the two sides.

## Two conflation traps

- **Trap A — two different `present_match` fields.** `HeaderMatcher.present_match`
  (this fixture's family) is a DIFFERENT field on a DIFFERENT message from
  `ValueMatcher.present_match` (the RBAC / access-log **metadata** matcher,
  e.g. fixture `0044`), where **`present_match: false` NEVER matches** — a
  DIFFERENT and CORRECT rule. Since 75.1 the two AGREE in three of four cells and
  differ in exactly ONE (ABSENT × `want = false`). **Do NOT unify them**, and do
  not "fix" the `ValueMatcher` rule to match.
- **Trap B — two different `invert` fields.** `HeaderMatcher.invert_match` (this
  fixture) is unrelated to `MetadataMatcher.invert` (carry-forward CF-74-1), which
  is MEASURED accepted-but-INERT upstream and stays boot-fatal here.
  "Implementing" it would CREATE a divergence.

## Cross-references

- **ADR-0156** — the phase-75 pick (§5 state-0/1) and its measured basis.
- **ADR-0157** — the §6.1 SPLIT of phase 75 into 75.1 + 75.2.
- **ADR-0158** — the parent's §6.2 reconciliation, including the single-log-file
  driver constraint that forced TWO fixtures rather than one.
- **ADR-0159** — sub-phase 75.1's §6.2 reconciliation.
- **ADR-0161** — sub-phase 75.2's §6.2 reconciliation.
- **Fixture `0085-headermatcher-absence-accesslog-present-polarity`** — the D2
  sibling on this same path.
- **Fixture `0083-headermatcher-absence-parity`** — the ROUTE-path witness.
- **Fixture `0078-accesslog-header-filter`** — the shape stencil.
- `BEHAVIOR_CONTRACT.md`, the **Phase 75** block — the canonical statement of the
  polarity rule, both measured matrices, and the mode-scoping guard (its §D).

## Deferred / out of scope

- **CF-72-2** — three REJECT-direction load-parity gaps: name-only
  `header: { name: x-a }`; `treat_missing_header_as_empty: true` (accepted AND
  HONORED upstream); and the top-level `contains_match` arm. None can appear in a
  fixture until implemented, because the config would not boot on the subject side.
- **CF-75-1** — `exact_match: ""` degenerates to a PRESENCE match upstream, while
  envoy-rust performs a literal empty-value exact comparison.
- **CF-75-2** — upstream comma-joins duplicate header values before value matching;
  envoy-rust matches only the FIRST occurrence.

**CF-72-2 and CF-75-1 are BANKED in `BEHAVIOR_CONTRACT.md`** (the phase-72 `§D` and
`§G` records), not fixed here. **CF-75-2 is NOT in that file at all** — it is an open
carry-forward recorded in `docs/envoy-rust/STATE.md` and it needs its own measured
phase, because it spans all SIX value modes across all FIVE `HeaderMatcher`
consumers. It is not a regression: the PRESENCE axis this fixture pins is parity.
