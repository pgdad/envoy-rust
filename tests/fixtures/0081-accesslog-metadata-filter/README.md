# Fixture 0081 — access-log `metadata_filter` (the DYNAMIC-METADATA gate) (phase 74)

The SIXTH access-log FILTER witness (arm #6) and the FIRST to gate a sink on
**dynamic metadata** rather than on the response or the request headers. An
`AccessLog` entry carrying `filter.metadata_filter` emits a record iff the
request's dynamic metadata, resolved at `matcher.filter` → `matcher.path[0].key`,
matches `matcher.value`. Sibling fixture
`0082-accesslog-metadata-filter-key-not-found` covers the key-ABSENT arm.

## What this proves

One HCM listener with an `envoy.filters.http.header_to_metadata` filter mapping
the `x-a` request header into dynamic metadata `com.example:k`, a
`text_format_source` file sink filtered on

```yaml
metadata_filter:
  matcher:
    filter: com.example
    path:
      - key: k
    value:
      string_match: { exact: "1" }
```

and ONE `direct_response` route (`/x` → 200 `hi`). Three probes:

| # | request | `com.example:k` | branch taken | emitted? |
|---|---|---|---|---|
| 1 | `GET /x` `x-a: 2` | `"2"` | resolved → `exact: "1"` says **no** | **DROPPED** |
| 2 | `GET /x` (no `x-a`) | *absent* | unresolved → `match_if_key_not_found` **default `true`** | **KEPT** |
| 3 | `GET /x` `x-a: 1` | `"1"` | resolved → `exact: "1"` says **yes** | **KEPT** |

The access-log file on EACH proxy holds EXACTLY TWO byte-identical lines, in this
order (MEASURED, SPEC §0 R-0.3/R-0.4, graceful-stop flush):

```
STATUS=200 PATH=/x M=-
STATUS=200 PATH=/x M=1
```

The two kept lines are byte-DISTINCT, so the fixture pins line ORDER as well as
the count — a "logged the right number of records but the wrong ones" bug cannot
survive it.

**Probe 2 exists to make this fixture read the `match_if_key_not_found` wrapper
DEFAULT** (phase 74 §5.2 state-3 re-entry, `REVIEW.md` I-3). Originally `0081`
omitted the field but BOTH its probes sent `x-a`, so the key always resolved and
the default was never consulted, while sibling `0082` pins the field to explicit
`false` — meaning **no committed fixture exercised the `None → true` KEEP branch**,
the phase's headline observable. It was pinned only by envoy-rust asserting
against itself in-process: mutating `unwrap_or(true)` → `unwrap_or(false)` REDdened
a unit test but left every differential fixture green. It is now witnessed
cross-proxy here. Note this branch is exactly what `--mode validate` provably
CANNOT reach, because it is a proto3 `google.protobuf.BoolValue` default.

## The MEASURED decision rule (SPEC §0 R-0.3/R-0.4, ADR-0155)

```
resolved = dynamic_metadata[matcher.filter][matcher.path[0].key]
  None    (unresolved, or no matcher at all) => match_if_key_not_found
  Some(v)                                    => matcher.value.matches(v)
```

Probes 1 and 3 RESOLVE the key, so they take the `Some(v)` branch and the value
matcher alone decides. Probe 2 does NOT, so it takes the `None` branch.
`match_if_key_not_found` is deliberately **ABSENT** from this fixture, which is
precisely the point: absent means the MEASURED default `true`, so probe 2 is
KEPT. Sibling fixture `0082` sets the field explicitly to `false` and shows the
SAME no-`x-a` probe flipping to DROPPED — the two fixtures together witness both
polarities of the wrapper default cross-proxy.

> **`on_header_missing` must NOT be added to this fixture either.** The
> `header_to_metadata` rule carries `on_header_present` ONLY. envoy-rust requires
> a `value` on an `on_header_missing` block, and supplying one would WRITE
> `com.example:k` on the no-`x-a` probe — the key would RESOLVE, probe 2 would be
> decided by the VALUE matcher instead of the not-found policy, and the
> default-`true` witness would be silently vacated. This is the same trap
> documented in `0082`'s README (ADR-0155 PV-6).

> **Why the line CAN echo the gating value here** (unlike `0079`/`0080`).
> `%DYNAMIC_METADATA(namespace:key)%` is a SEPARATE command operator with its own
> parser and is **not** gated by `REQ_ALLOW_LIST`, so the format may render the
> gated value directly. A `%REQ(X-A)%` would be boot-fatal on envoy-rust
> (`ConfigError::InvalidAccessLogFormat` — `BEHAVIOR_CONTRACT.md` §F and the
> phase-73 §D note), which is why the sibling composition fixtures render only
> `STATUS`+`PATH`. The operator renders the raw unquoted value when present and
> `-` when either the namespace or the key is absent — **both renderings are
> witnessed directly by this fixture on both proxies** (probe 3 → `M=1`, probe 2
> → `M=-`).

> **`matcher.invert` may NOT be used in any fixture.** It is MEASURED
> accepted-but-INERT upstream on this path (reproduced twice; an `invertBOGUS`
> control is REJECTED, proving `invert` is a genuine recognised field the
> evaluation path then ignores). envoy-rust's `MetadataMatcher` has no `invert`
> field under `deny_unknown_fields`, so a config carrying it is BOOT-FATAL here —
> a load-parity gap in the REJECT direction, carry-forward **CF-74-1**.
> "Implementing" it would CREATE a divergence (ADR-0049 fail-loud posture). Note
> this is a DIFFERENT field on a DIFFERENT message from `HeaderMatcher.invert_match`
> (CF-72-1), whose divergence is mode-scoped.

## Probes / driver

`kind: http1_access_log_byte_exact`. Probe ordering follows the **kept-LAST**
convention (ADR-0147): the single DROPPED probe comes first and both KEPT probes
follow, so the LAST probe is KEPT and the driver's ordering-aware
`suppression_settle` pays the cheap 2 s `CF70_3_SETTLE` rather than the 12 s
`CF71_1_SETTLE`. `suppression_settle` inspects only `probes.last()`, so the cheap
settle would hold with the two kept probes in either order; probe 2 is placed
SECOND for a DIFFERENT reason — it pins the LINE ORDER (`M=-` before `M=1`).
`expected_logged_count` is therefore **2**. The assertion is PURE cross-proxy equality — both proxies must agree on
the two KEPT lines, in order, AND on the ABSENCE of any line for the
value-mismatching probe.

`clusters: []` — the only route is a `direct_response`, so no backend spawns.

## Per-side divergences

| | `envoy.yaml` | `envoy-rust.yaml` |
|---|---|---|
| `admin` | present (port 0) | omitted |
| listener bind | `0.0.0.0` | `127.0.0.1` |
| `generate_request_id` | `false` (explicit) | omitted (not modelled) |
| access-log mount | `/tmp/0081-envoy-mount/` | `/tmp/0081-envoy-rust-mount/` |

Everything else — including the entire `header_to_metadata` stanza, the log
format and the `metadata_filter` block — is byte-identical between the two sides.

## Cross-references

- **ADR-0154** — the phase-74 pick (§5 state-0/1).
- **ADR-0155** — the §6.2 empirical reconciliation (PV-1..PV-8), including the
  measured rejection of a `bool`-returning matcher seam.
- **Fixture `0082-accesslog-metadata-filter-key-not-found`** — the sibling that
  witnesses `match_if_key_not_found: false`.
- **Fixture `0042`** — the `header_to_metadata` producer precedent. NB `0042`
  carries an `on_header_missing` block WITH a `value`; `0082` must NOT (see its
  README).
- `BEHAVIOR_CONTRACT.md` "Phase 74" subsection.
