# Fixture 0080 — access-log `or_filter` (depth-2 recursion) (phase 73)

The FIFTH access-log FILTER witness (arm #5) AND the depth-2 recursion witness.
An `AccessLog` entry carrying `filter.or_filter.filters` gates the sink's
per-record emission on the boolean **OR** of its nested child predicates, where
one child is ITSELF an `and_filter` (depth-2). Sibling fixture 0079 covers the
flat `and_filter`; this fixture proves the recursion is observable cross-proxy.

## What this proves

One HCM listener with a `text_format_source` file sink filtered on
`or_filter: { filters: [ and_filter{[x-a=1, x-b=1]}, header_filter{x-c=1} ] }`,
and ONE `direct_response` route (`/x` → 200 `hi`). Three probes:

| # | request | nested `and{x-a,x-b}` | leaf `header{x-c}` | OR (any match)? | emitted? |
|---|---|---|---|---|---|
| 1 | `GET /x` `x-a:1` | false (`x-b` absent) | false (`x-c` absent) | no | **DROPPED** |
| 2 | `GET /x` `x-a:1 x-b:1` | true | false | yes | **KEPT** |
| 3 | `GET /x` `x-c:1` | false | true | yes | **KEPT** |

The access-log file on EACH proxy holds EXACTLY TWO byte-identical lines
(MEASURED, SPEC §0 R-0.5, graceful-stop flush):

```
STATUS=200 PATH=/x
STATUS=200 PATH=/x
```

`or_filter` emits iff **any** nested child predicate matches. The nested
`and_filter` (child 0) is itself evaluated recursively — witnessing
OR-of-(nested-AND, leaf) at depth 2. `filters` is PGV `min_items = 2` at every
level; children may be any `AccessLogFilter` (leaf OR another composition) to
arbitrary depth (no depth guard, matching upstream — CF-73-1).

> **Why the line does not echo `x-a`/`x-b`/`x-c`.** The composition gates on the
> `x-a`/`x-b`/`x-c` request headers (read from the raw request-header slice at
> the emit gate), but the log FORMAT renders only `STATUS`+`PATH`. envoy-rust's
> `%REQ(NAME)%` command operator supports only an allow-list of headers; a
> `%REQ(X-A)%` format would be boot-fatal on envoy-rust
> (`ConfigError::InvalidAccessLogFormat`). The keep/drop line COUNT + the
> byte-identical content are the differential witnesses (the 0078/0079 precedent;
> `BEHAVIOR_CONTRACT.md` §F and the phase-73 §D note).

## Probes / driver

`kind: http1_access_log_byte_exact` (`Http1AccessLogByteExact`) — reuses the
existing byte-exact access-log driver with NO harness change. The DROPPED probe
(`x-a:1` only) is FIRST and the KEPT probes (`x-a:1 x-b:1`, then `x-c:1`) are
LAST — the sound kept-LAST authoring convention (ADR-0147). Because the last
probe is KEPT, the driver's ordering-aware settle pays only the cheap
`CF70_3_SETTLE`. The assertion is PURE cross-proxy equality — both proxies must
agree on the two kept lines AND on the absence of any line for the dropped probe.

## Per-side divergences (`envoy.yaml` ↔ `envoy-rust.yaml`)

| field | `envoy.yaml` | `envoy-rust.yaml` | why |
|---|---|---|---|
| `admin` | present (port 0) | absent | envoy-rust has no admin server in this fixture |
| listener bind | `0.0.0.0` | `127.0.0.1` | envoy-rust binds loopback in-harness |
| `generate_request_id` | `false` | omitted | upstream defaults it on; envoy-rust does not emit request-ids here |
| access-log path | `/tmp/0080-envoy-mount/access.log` | `/tmp/0080-envoy-rust-mount/access.log` | per-side mount dirs |

## Cross-references

- ADR-0152 (phase-73 pick + scope + rejected alternatives), ADR-0153 (§6.2
  reconciliation, PV-1..PV-8 + the `%REQ(X-A)%`-is-boot-fatal format correction).
- Sibling fixture 0079 (`and_filter`, flat depth-1).
