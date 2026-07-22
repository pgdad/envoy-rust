# Fixture 0079 — access-log `and_filter` (phase 73)

The FOURTH access-log FILTER witness (arm #4). An `AccessLog` entry carrying
`filter.and_filter.filters` gates the sink's per-record emission on the boolean
**AND** of its nested child predicates. Phase 70 built `status_code_filter`
(0076); phase 71 built `response_flag_filter` (0077); phase 72 built
`header_filter` (0078); this phase builds the recursive `and_filter` / `or_filter`
composition (0079 / 0080).

## What this proves

One HCM listener with a `text_format_source` file sink filtered on
`and_filter: { filters: [ header_filter{x-a=1}, header_filter{x-b=1} ] }`, and
ONE `direct_response` route (`/x` → 200 `hi`). Two probes:

| # | request | `x-a` | `x-b` | AND (all match)? | emitted? |
|---|---|---|---|---|---|
| 1 | `GET /x` | `1` | absent | no | **DROPPED** |
| 2 | `GET /x` | `1` | `1` | yes | **KEPT** |

The access-log file on EACH proxy holds EXACTLY ONE byte-identical line
(MEASURED, SPEC §0 R-0.3, graceful-stop flush):

```
STATUS=200 PATH=/x
```

`and_filter` emits iff **all** nested child predicates match; a single failing
child (here `x-b` absent) drops the record. `filters` is PGV `min_items = 2`.

> **Why the line does not echo `x-a`/`x-b`.** The `and_filter` gates on the
> `x-a`/`x-b` request headers (read from the raw request-header slice at the emit
> gate), but the log FORMAT renders only `STATUS`+`PATH`. envoy-rust's
> `%REQ(NAME)%` command operator supports only an allow-list of headers
> (`:method`/`:authority`/`:path`/`x-envoy-original-path`/`x-forwarded-for`/
> `user-agent`/`x-request-id`); a `%REQ(X-A)%` format would be boot-fatal on
> envoy-rust (`ConfigError::InvalidAccessLogFormat`). The keep/drop line COUNT +
> the byte-identical content are the differential witnesses (the 0078 precedent;
> `BEHAVIOR_CONTRACT.md` §F and the phase-73 §D note).

## Probes / driver

`kind: http1_access_log_byte_exact` (`Http1AccessLogByteExact`) — reuses the
existing byte-exact access-log driver with NO harness change (the driver already
carries `extra_headers` + `expect_logged`). The DROPPED probe (`x-a:1` only) is
FIRST and the KEPT probe (`x-a:1 x-b:1`) is LAST — the sound kept-LAST authoring
convention (ADR-0147). Because the last probe is KEPT, the driver's
ordering-aware settle pays only the cheap `CF70_3_SETTLE` (the long
`CF71_1_SETTLE` protects dropped-LAST fixtures like 0076). The assertion is PURE
cross-proxy equality — both proxies must agree on the kept line AND on the
absence of any line for the dropped probe.

## Per-side divergences (`envoy.yaml` ↔ `envoy-rust.yaml`)

| field | `envoy.yaml` | `envoy-rust.yaml` | why |
|---|---|---|---|
| `admin` | present (port 0) | absent | envoy-rust has no admin server in this fixture |
| listener bind | `0.0.0.0` | `127.0.0.1` | envoy-rust binds loopback in-harness |
| `generate_request_id` | `false` | omitted | upstream defaults it on; envoy-rust does not emit request-ids here |
| access-log path | `/tmp/0079-envoy-mount/access.log` | `/tmp/0079-envoy-rust-mount/access.log` | per-side mount dirs |

## Cross-references

- ADR-0152 (phase-73 pick + scope + rejected alternatives), ADR-0153 (§6.2
  reconciliation, PV-1..PV-8 + the `%REQ(X-A)%`-is-boot-fatal format correction).
- Sibling fixture 0080 (`or_filter`, depth-2 recursion).
