# Fixture 0078 — access-log `header_filter` (phase 72)

The THIRD access-log FILTER witness (arm #3). An `AccessLog` entry carrying
`filter.header_filter.header` gates the sink's per-record emission on whether a
named REQUEST HEADER matches a `HeaderMatcher`. Phase 70 built
`status_code_filter` (0076); phase 71 built `response_flag_filter` (0077); this
phase builds `header_filter`.

## What this proves

One HCM listener with a `text_format_source` file sink filtered on
`header_filter: { header: { name: x-log, string_match: { exact: "yes" } } }`, and
ONE `direct_response` route (`/x` → 200 `hi`). Two probes:

| # | request | `x-log` | matches `exact: "yes"`? | emitted? |
|---|---|---|---|---|
| 1 | `GET /x` | `no` (present-mismatch) | no | **DROPPED** |
| 2 | `GET /x` | `yes` (present-match) | yes | **KEPT** |

The access-log file on EACH proxy holds EXACTLY ONE byte-identical line
(MEASURED, ADR-0149 R-0.4, graceful-stop flush):

```
STATUS=200 PATH=/x
```

Present-mismatch AND absent both DROP (measured R-0.4); this fixture exercises
the present-mismatch drop.

> **Why the line does not echo `x-log`.** The `header_filter` gates on the
> `x-log` request header (read from the raw request-header slice at the emit
> gate), but the log FORMAT renders only `STATUS`+`PATH`. envoy-rust's
> `%REQ(NAME)%` command operator supports only an allow-list of headers
> (`:method`/`:authority`/`:path`/`x-envoy-original-path`/`x-forwarded-for`/
> `user-agent`/`x-request-id`) because the `AccessLogRecord` carries no arbitrary
> request-header map (SPEC §2.2 — this phase adds no new record field). A
> `%REQ(X-LOG)%` format would be boot-fatal on envoy-rust. The keep/drop decision
> is the differential witness; echoing the header value is a FORMATTER concern
> orthogonal to the header-FILTER this phase builds.

## Probes / driver

`kind: http1_access_log_byte_exact` (`Http1AccessLogByteExact`) — reuses the
existing byte-exact access-log driver with NO harness change for the positive
witness (the driver already carries `extra_headers` + `expect_logged`). The
DROPPED probe (`x-log: no`) is FIRST and the KEPT probe (`x-log: yes`) is LAST —
the sound kept-LAST authoring convention (ADR-0147). Because the last probe is
KEPT, the driver's ordering-aware settle pays only the cheap `CF70_3_SETTLE`
(the long `CF71_1_SETTLE` protects dropped-LAST fixtures like 0076). The
assertion is PURE cross-proxy equality — both proxies must agree on the kept
line AND on the absence of any line for the dropped probe.

## Per-side divergences (`envoy.yaml` ↔ `envoy-rust.yaml`)

| field | `envoy.yaml` | `envoy-rust.yaml` | why |
|---|---|---|---|
| `admin` | present (port 0) | absent | envoy-rust has no admin server in this fixture |
| listener bind | `0.0.0.0` | `127.0.0.1` | envoy-rust binds loopback in-harness |
| `generate_request_id` | `false` | omitted | upstream defaults it on; envoy-rust does not emit request-ids here |
| access-log path | `/tmp/0078-envoy-mount/access.log` | `/tmp/0078-envoy-rust-mount/access.log` | per-side mount dirs |

## Cross-references

- ADR-0148 (phase-72 pick + scope), ADR-0149 (§6.2 reconciliation, PV-1..PV-7),
  ADR-0150 (the `HeaderMatch` trait-object runtime seam).

## Deferred (NOT in this differential — pinned in-process / documented)

- **Absent-drop** and **`invert_match` + absent** parity: the in-tree shared
  engine keeps absent+invert (`mode_result ^ invert_match`), diverging from
  upstream (which drops it on BOTH the route and access-log paths) — carry-forward
  **CF-72-1**. The opener uses a NON-inverted matcher; the divergence is pinned
  in-process + documented in `BEHAVIOR_CONTRACT.md` §C, not exercised here.
- **Name-only `{name}`** and **`treat_missing_header_as_empty`**: upstream accepts
  both; the shared `HeaderMatcher` deserializer rejects both fail-loud (ADR-0049)
  — carry-forward **CF-72-2**. Pinned in-process, documented §D, not in this
  differential.
