# Fixture 0093 — `%GRPC_STATUS%` access-log command-operator family

Phase 113. The cross-proxy witness for `%GRPC_STATUS%`,
`%GRPC_STATUS(SNAKE_STRING)%` and `%GRPC_STATUS_NUMBER%` over the HTTP/1.1
local-reply surface that phase 110 built.

- **Driver:** the EXISTING `Driver::Http1AccessLogByteExact` (`kind:
  http1_access_log_byte_exact`). No new driver, no harness change.
- **Shape:** one H1 HCM listener; ONE `FileAccessLog` sink with NO `filter:`, so
  every probe logs; thirteen `direct_response` routes (twelve `path:` matches
  plus a `prefix: "/"` catch-all); `clusters: []`, **no backend spawns**.
- **Cluster-free and backend-free**, therefore verifiable on a development host
  rather than CI-only — backend-routing fixtures go RED locally against the
  `192.168.65.2` Docker bridge.

## The format string (identical on both sides)

```
GS=%GRPC_STATUS% SNAKE=%GRPC_STATUS(SNAKE_STRING)% NUM=%GRPC_STATUS_NUMBER% CODE=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n
```

All three spellings sit in ONE line, so a single byte-exact comparison covers
the whole rendering matrix. `%RESPONSE_CODE%` is the anchor showing whether the
phase-110 transform fired.

## What it witnesses

**Probes 1-8 (positive).** Each sends a gRPC `content-type` and drives a
different `direct_response.status` through the phase-110 HTTP→gRPC mapping
table, **corroborating that landed table through a NEW observable** — phase 110
measured it on the response *header*, this fixture reads it out of the *access
log*. It reproduces the sparse map exactly, including the two counter-intuitive
cells where `200` and `501` both fall through to `2`/`Unknown`. Probe 8 covers
the `application/grpc+proto` prefix cell.

**Probes 9-12 (negative).** The four content-type spellings the transform
rejects: a `; charset=utf-8` parameter, `application/grpc-web`, the uppercase
`APPLICATION/GRPC`, and no `content-type` at all. On these the transform does
**not** fire, so `%RESPONSE_CODE%` stays at the underlying `404` and all three
gRPC columns render the `-` sentinel.

## ⚠ What it does NOT witness — read this before citing it as coverage

**This fixture CANNOT witness the operator's request-side gate.** That is
measured, not argued: delete the `is_grpc_request` gate from
`build_access_log_record`, rebuild `envoy-bin`, and **this fixture stays GREEN**.

The reason is structural. On the local-reply surface the ONLY producer of a
response `grpc-status` header is the phase-110 transform — and that transform is
gated on the **same** `is_grpc_request` predicate. With the gate shut there is no
header for the operator to read either way, so a gated implementation and an
ungated one are indistinguishable here. No probe set on this surface can
separate them.

**The gate's only witness in the tree is**
`hcm::grpc_status_access_log_tests::gate_stays_shut_on_the_four_measured_negative_spellings`
in `crates/envoy-http1/src/hcm.rs`, which IS red under that same compiled
mutation. Do not weaken it, and do not record this fixture as covering the gate.

Probes 9-12 still earn their place — they witness that the transform did not
fire (`expected_status: 404`) and that the absent-value sentinel renders
correctly — but that is a statement about the *transform*, not about the
*operator's gate*.

Banked as **CF-113-6**. Witnessing the gate needs a PROXIED gRPC response, which
is blocked behind **CF-111-2** / **CF-113-3**.

## Fixture-design constraints (each MEASURED, each a way this fixture could have gone RED for a non-gRPC reason)

1. **No route-level `response_headers_to_add`.** It is the natural way to stage
   `grpc-status` values and it works upstream, but envoy-rust's `Route` has no
   such field and `deny_unknown_fields` is on, so it is **boot-fatal** here. The
   values are driven from `direct_response.status` through the phase-110
   transform instead — which is strictly better, because it makes the fixture
   corroborate the landed mapping table.
2. **`direct_response.body` is MANDATORY on every route.** It is optional
   upstream and required in envoy-rust (**CF-110-7**).
3. **The format string must NOT echo `content-type`.** `%REQ(NAME)%` is
   allow-list gated by the seven-name `REQ_ALLOW_LIST`, so `%REQ(CONTENT-TYPE)%`
   is boot-fatal. `:path` IS on that list, which is why probes are attributed by
   path.
4. **Every probe needs its own `path:`, and every `expected_status` must be set
   explicitly.** This driver asserts only a per-side line count plus whole-line
   cross-proxy equality; without distinct paths a permutation of the mapping
   would keep the count at 12 and pass. And probes 9-12 are 404, not the
   driver's `200` default.

## The two YAMLs

They differ in exactly four ways, all harness-mandated and none semantic: the
upstream side carries an `admin:` line and `generate_request_id: false`, binds
`0.0.0.0` vs `127.0.0.1`, and writes to `/tmp/0093-envoy-mount/access.log` vs
`/tmp/0093-envoy-rust-mount/access.log`. `{{PORT}}` is the only token.

## The twelve expected lines (byte-identical on both proxies)

```
GS=Unknown SNAKE=UNKNOWN NUM=2 CODE=200 PATH=/g-ok
GS=Internal SNAKE=INTERNAL NUM=13 CODE=200 PATH=/g-internal
GS=Unauthenticated SNAKE=UNAUTHENTICATED NUM=16 CODE=200 PATH=/g-unauth
GS=PermissionDenied SNAKE=PERMISSION_DENIED NUM=7 CODE=200 PATH=/g-denied
GS=Unimplemented SNAKE=UNIMPLEMENTED NUM=12 CODE=200 PATH=/g-unimpl
GS=Unavailable SNAKE=UNAVAILABLE NUM=14 CODE=200 PATH=/g-unavail
GS=Unknown SNAKE=UNKNOWN NUM=2 CODE=200 PATH=/g-default
GS=Unimplemented SNAKE=UNIMPLEMENTED NUM=12 CODE=200 PATH=/g-proto
GS=- SNAKE=- NUM=- CODE=404 PATH=/g-param
GS=- SNAKE=- NUM=- CODE=404 PATH=/g-web
GS=- SNAKE=- NUM=- CODE=404 PATH=/g-upper
GS=- SNAKE=- NUM=- CODE=404 PATH=/g-plain
```
