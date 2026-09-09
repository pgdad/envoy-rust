# Fixture 0094 — the `grpc_status_filter` access-log FILTER arm

Phase 114. The cross-proxy witness for `grpc_status_filter`, the **SEVENTH** of
upstream Envoy's twelve `envoy.config.accesslog.v3.AccessLogFilter` `oneof`
arms, after `status_code_filter` (70), `response_flag_filter` (71),
`header_filter` (72), `and_filter`/`or_filter` (73) and `metadata_filter` (74).

This is a **log-emission predicate**, not a formatter. It decides whether a
record is emitted at all, gating on the request's **effective gRPC status**.

- **Driver:** the EXISTING `Driver::Http1AccessLogByteExact` (`kind:
  http1_access_log_byte_exact`). No new driver, no harness change —
  `tests/differential/src/lib.rs` is untouched by this phase.
- **Shape:** one H1 HCM listener; ONE `FileAccessLog` sink carrying
  `filter: { grpc_status_filter: { statuses: [UNIMPLEMENTED, INTERNAL] } }`
  (codes 12 and 13); nine `direct_response` routes (eight `path:` matches plus a
  `prefix: "/"` catch-all); `clusters: []`, **no backend spawns**.
- **Cluster-free and backend-free**, therefore verifiable on a development host
  rather than CI-only — backend-routing fixtures go RED locally against the
  `192.168.65.2` Docker bridge.
- Eight probes on eight **distinct paths**; **five are kept**.

## The format string (identical on both sides)

```
PATH=%REQ(:PATH)% CODE=%RESPONSE_CODE% GS=%GRPC_STATUS%\n
```

`%GRPC_STATUS%` is phase 113's **GATED** value and it is rendered here on
purpose: on three of the five kept lines it prints the `-` sentinel, so the log
file itself witnesses that the formatter's gated value and the filter's ungated
value are different values.

The line does **not** echo `content-type`. `%REQ(NAME)%` is ALLOW-LIST gated by
the seven-name `REQ_ALLOW_LIST` in
`crates/envoy-accesslog/src/command_operator.rs`, so `%REQ(CONTENT-TYPE)%` would
be BOOT-FATAL. `:path` IS on that list, which is why every probe is attributed
by path.

## The four MEASURED rules

Measured against `envoyproxy/envoy:v1.33.0`
(`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) on
six sinks over one listener at the phase-114 pick, and re-measured at the
PLAN-write.

1. **The status source is the response `grpc-status` header if present, else
   `http_to_grpc_status(response_code)`** — the phase-110 map. Upstream's map has
   **no `200 => 0` arm**: a plain 200 derives `UNKNOWN` (2), the `_ => 2`
   default.
2. **The filter is NOT gated on the request being a gRPC request.** This is the
   OPPOSITE of `%GRPC_STATUS%` and it is the governing finding of the phase. A
   plain HTTP 404 with no gRPC content-type is **kept** by a sink filtered on
   `[UNIMPLEMENTED]`.
3. **`exclude: true` inverts the membership test**, over the same derived
   status.
4. **An empty `statuses` list keeps NOTHING.** It is not "match everything".

**Rules 3 and 4 are witnessed IN-PROCESS ONLY, not by this fixture, and that is
banked as `CF-114-4`.** The byte-exact driver takes exactly ONE log file per
side (`AccessLogPaths { envoy, envoy_rust }`), so an `exclude` witness would need
its own second fixture. `grpc_status_arm_exclude_inverts_over_the_same_code` and
`grpc_status_arm_empty_statuses_keeps_nothing` in
`crates/envoy-accesslog/src/filter.rs` pin them instead, the former over all 17
codes — more than a second fixture would have bought.

## Why the gRPC probes expect HTTP 200

Probes 1–4 send `content-type: application/grpc`. On a **locally generated**
reply that makes the **phase-110 transform** fire: it rewrites the HTTP status to
`200`, sets `content-type: application/grpc`, drops the body and emits a
`grpc-status` header carrying the mapped code. So their `expected_status` is
**200**, NOT the route's `direct_response.status`.

⚠ The landed `SPEC.md` §6 probe table gives the route's status for these probes.
That is wrong and `ADR-0197` correction 3 records it; this fixture follows the
measurement.

**This is also what makes probes 1 and 2 witness the HEADER leg for real.** The
transform emits `grpc-status: 12` and `grpc-status: 13` while rewriting the
logged response code to 200. An implementation that derived from the LOGGED
response code would compute `http_to_grpc_status(200) = 2` for both and DROP
them. `SPEC.md` §6 called the header leg inexpressible without route-level
`response_headers_to_add`; `ADR-0197` correction 4 shows it is expressible
exactly this way.

## Why probes 5, 6 and 8 make this fixture non-vacuous

These three are requests for which `%GRPC_STATUS%` renders the `-` sentinel —
the phase-113 field is `None` because the request was not gRPC-detected — and
which the filter **keeps anyway**:

| probe | path | request `content-type` | code | derived | kept |
|---|---|---|---|---|---|
| 5 | `/p-unimpl` | *(none)* | 404 | 12 | **yes** |
| 6 | `/p-internal` | *(none)* | 400 | 13 | **yes** |
| 8 | `/g-param` | `application/grpc; charset=utf-8` | 404 | 12 | **yes** |

Probe 8 is the second, independent witness: a `charset` parameter defeats the
phase-110 detector, so the transform does not fire and no header exists, yet the
filter still derives 12 from the response code.

**An implementation that reused `AccessLogRecord.grpc_status` — the obvious wrong
move, since that field already exists and is already named for this — goes RED
on all three.** That is not an argument; it was proved by MUTATION at the
state-3 implementation. Gating the derivation on `is_grpc_request` makes
envoy-rust emit **2** lines where **5** are expected, the three lost being
exactly probes 5, 6 and 8, with the unmutated control GREEN from the same tree.
Probes 3, 4 and 7 are the suppression controls.

## Authoring constraints (all inherited, all load-bearing)

1. **No route-level `response_headers_to_add`** — envoy-rust's hand-rolled
   `Route` visitor rejects the key by name, which is boot-fatal. `grpc-status`
   values must come from the phase-110 transform.
2. **`direct_response.body` is MANDATORY on every route** (`CF-110-7`;
   `DirectResponse.body` is a `DataSource`, not an `Option`).
3. **The format string must not echo `content-type`** (`%REQ(NAME)%` allow-list).
4. **Every probe gets a DISTINCT path** — identical paths cannot attribute a
   kept log line, and a regression permuting the mapping would keep the count
   right and still pass.
5. **The LAST probe is KEPT**, so the driver's ordering-aware suppression settle
   charges the cheap short wait rather than the long one.
6. **`{{PORT}}` is the only token.** `Http1AccessLogByteExact` is not one of the
   five drivers that receive `{{ADMIN_PORT}}`, so the upstream `admin:` block
   uses a literal `port_value: 0`.

## The two YAMLs

`envoy.yaml` is `envoy-rust.yaml` plus exactly **four harness hunks**, none of
them semantic, the same four every landed access-log byte-exact fixture carries
(`0076`, `0078`, `0081`, `0093`):

```
2d1    < admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
6c5    <       address: { socket_address: { address: 0.0.0.0,   port_value: {{PORT}} } }
       >       address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
14d12  <                 generate_request_id: false
22c20  <                       path: /tmp/0094-envoy-mount/access.log
       >                       path: /tmp/0094-envoy-rust-mount/access.log
```

**The `filter:` block is byte-identical on both sides** (verified by md5 over the
block, not by eye). The two log paths must live in different parent directories
because the driver bind-mounts only the upstream side's parent.

## The assertion

Pure **cross-proxy equality**: whole-line `==` between upstream Envoy v1.33.0 and
envoy-rust, plus an exact per-side line count. There is no static expected-line
literal in `expectations.yaml`. Both proxies must agree on **both halves** of
every filter decision — a one-sided suppression fails the count assertion before
the byte compare is reached.

Five lines per side, byte-identical, measured at the state-3 implementation:

```
PATH=/g-unimpl CODE=200 GS=Unimplemented
PATH=/g-internal CODE=200 GS=Internal
PATH=/p-unimpl CODE=404 GS=-
PATH=/p-internal CODE=400 GS=-
PATH=/g-param CODE=404 GS=-
```

The last three are the payload of the whole phase: the record was KEPT while
`%GRPC_STATUS%` rendered `-`.
