# Fixture 0085 — `HeaderMatcher` absence semantics on the ACCESS-LOG path: the D2 witness (sub-phase 75.2)

The **D2** cross-proxy witness for the `HeaderMatcher` ABSENCE rule landed by
sub-phase **75.1**, taken on the **ACCESS-LOG** path — the SECOND consumer of the
one shared matching engine, reached through the ADR-0150 `HeaderMatch` trait seam
(`LogFilter::Header { matcher } => matcher.matches(headers)` in
`crates/envoy-accesslog/src/filter.rs` dispatches to
`impl envoy_accesslog::HeaderMatch for HeaderMatcher` in
`crates/envoy-config/src/matcher.rs`, whose trait object is injected by
`compile_access_log_filter` in `crates/envoy-http1/src/hcm.rs`). The sibling
fixture `0084-headermatcher-absence-accesslog` witnesses **D1**; the ROUTE-path
witness of the same rule is `0083-headermatcher-absence-parity`.

**Why a SEPARATE fixture rather than a second sink in `0084`.** The byte-exact
access-log driver takes exactly ONE log file per side: `AccessLogPaths`
(`tests/differential/src/lib.rs`) is `{ envoy: String, envoy_rust: String }` under
`deny_unknown_fields`, and only the envoy-side parent directory is bind-mounted
into the container, so a second sink writing elsewhere would not even be visible to
the host. Corpus-wide, the maximum number of `envoy.access_loggers.file` sinks in
any single fixture config is **1**. One sink per fixture is therefore the only
available shape (ADR-0158) — this mirrors the existing sibling pair
`0081`/`0082`, which split the two polarities of the `metadata_filter` rule the
same way.

## What this proves

One H1 HCM listener, ONE `FileAccessLog` sink gated by
`header_filter: { header: { name: x-a, present_match: false } }` — a **plain,
NON-inverted, single-line** matcher — and ONE `direct_response` route
(`/x` → 200 `hi`). Two probes:

| # | request | matcher verdict | emitted? |
|---|---|---|---|
| 1 | `GET /x`, `x-a: v` | `PresentMatch(false)`: `(present == want)` = `(true == false)` → `false` | **DROPPED** |
| 2 | `GET /x`, **no** `x-a` | `(present == want)` = `(false == false)` → `true` | **KEPT** |

Probe 1 is the load-bearing one: it is **the D2 cell**. A pre-75.1 tree returned
`true` UNCONDITIONALLY for `PresentMatch(false)`, so it KEPT both probes — TWO
lines against upstream's ONE — and this fixture would fail its line-count
assertion.

**D2 is strictly worse than D1** because it fires on the simplest possible
spelling: no `invert_match` is needed. Before phase 75 it had NO behavioral test
anywhere in the tree.

The access-log file on EACH proxy holds EXACTLY ONE byte-identical line:

```
STATUS=200 PATH=/x
```

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
header VALUE counts as PRESENT. In particular **`present_match: false` means "the
header must be ABSENT"** — it is NOT "no presence requirement".

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
for the dropped probe; a one-sided keep fails the line-count assertion before the
byte compare is reached.

`clusters: []` — the only route is a `direct_response`, so no backend spawns.

## Per-side divergences (`envoy.yaml` ↔ `envoy-rust.yaml`)

| field | `envoy.yaml` | `envoy-rust.yaml` | why |
|---|---|---|---|
| `admin` | present (port 0) | absent | envoy-rust has no admin server in this fixture |
| listener bind | `0.0.0.0` | `127.0.0.1` | envoy-rust binds loopback in-harness |
| `generate_request_id` | `false` | omitted | upstream defaults it on; envoy-rust does not emit request-ids here |
| access-log path | `/tmp/0085-envoy-mount/access.log` | `/tmp/0085-envoy-rust-mount/access.log` | per-side mount dirs |

`codec_type: HTTP1` is written on **BOTH** sides and is **NOT** a divergence:
envoy-rust has no serde default for it under `deny_unknown_fields`, while upstream
would default to `AUTO` (ADR-0158 C3). The `header_filter` body, the log format and
the route table are byte-identical between the two sides.

## Two conflation traps

- **Trap A — two different `present_match` fields. This is the load-bearing trap
  for THIS fixture.** The field here is `HeaderMatcher.present_match`, whose
  MEASURED rule is `(present == want)`. `ValueMatcher.present_match` — the RBAC and
  access-log **METADATA** matcher, witnessed by fixture `0044` — is a DIFFERENT
  field on a DIFFERENT message, and for it **`present_match: false` NEVER
  matches**, even when the key is present. That is a DIFFERENT and **CORRECT**
  rule. Since 75.1 the two AGREE in three of four cells and differ in exactly ONE:

  | | `want = true` | `want = false` |
  |---|---|---|
  | PRESENT | `true` / `true` — agree | `false` / `false` — agree |
  | ABSENT | `false` / `false` — agree | **`false` / `true` — DIFFER** |

  (`ValueMatcher` verdict first, `HeaderMatcher` second.) **Do NOT unify them, and
  do NOT "fix" the `ValueMatcher` rule to match this one.**
- **Trap B — two different `invert` fields.** `HeaderMatcher.invert_match` is
  unrelated to `MetadataMatcher.invert` (carry-forward CF-74-1), which is MEASURED
  accepted-but-INERT upstream and stays boot-fatal here. "Implementing" it would
  CREATE a divergence.

## Cross-references

- **ADR-0156** — the phase-75 pick (§5 state-0/1) and its measured basis.
- **ADR-0157** — the §6.1 SPLIT of phase 75 into 75.1 + 75.2.
- **ADR-0158** — the parent's §6.2 reconciliation, including the single-log-file
  driver constraint that forced TWO fixtures rather than one.
- **ADR-0159** — sub-phase 75.1's §6.2 reconciliation.
- **ADR-0161** — sub-phase 75.2's §6.2 reconciliation.
- **Fixture `0084-headermatcher-absence-accesslog`** — the D1 sibling on this same
  path.
- **Fixture `0083-headermatcher-absence-parity`** — the ROUTE-path witness.
- **Fixture `0078-accesslog-header-filter`** — the shape stencil.
- **Fixture `0044`** — the `ValueMatcher.present_match` witness named in Trap A.
- `BEHAVIOR_CONTRACT.md`, the **Phase 75** block — the canonical statement of the
  polarity rule, both measured matrices, the mode-scoping guard (§D) and Trap A
  (§E).

## Deferred / out of scope

- **CF-72-2** — three REJECT-direction load-parity gaps: name-only
  `header: { name: x-a }`; `treat_missing_header_as_empty: true` (accepted AND
  HONORED upstream); and the top-level `contains_match` arm. None can appear in a
  fixture until implemented, because the config would not boot on the subject side.
- **CF-75-1** — `exact_match: ""` degenerates to a PRESENCE match upstream, while
  envoy-rust performs a literal empty-value exact comparison.
- **CF-75-2** — upstream comma-joins duplicate header values before value matching;
  envoy-rust matches only the FIRST occurrence.
- **P1, the mode-scoping guard** (`present_match: true` + `invert_match` + absent,
  which is FULL PARITY) is deliberately NOT fixtured here — it is already pinned
  in-process by `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream`
  and `invert_match_inverts_present_match_result`, and cross-proxy on the route path
  by `0083`. It is documented as §D of the `BEHAVIOR_CONTRACT.md` Phase 75 block.

The three carry-forwards are BANKED in `BEHAVIOR_CONTRACT.md`, not fixed here.
