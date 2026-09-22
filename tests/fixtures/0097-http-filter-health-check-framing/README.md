# 0097 — `health_check` framing composed with a later encode-side filter

Phase **115**, §5.2 state-3 re-entry round 2 (`ADR-0203`; phase-115 `REVIEW-2.md` I2-1, cell
3a). One HTTP/1.1 probe against a **backend-free, CLUSTER-FREE** HCM listener whose chain is

```
[ health_check     (:path exact /healthz),
  header_mutation  (response content-length: 7, OVERWRITE_IF_EXISTS_OR_ADD),
  router ]
```

with a `prefix: "/"` `direct_response` catch-all answering `MAIN`.

## What it witnesses

| probe | request | expected | rule |
|---|---|---|---|
| `p1` | `HEAD /healthz` | empty; `content-length: 7` and NO `transfer-encoding` | a headers-only reply's framing is settled LAST: the mutation, which runs in the encode pass AFTER the intercept, writes the only framing header |

MEASURED against `envoyproxy/envoy:v1.33.0` on the raw wire: `content-length: 7`,
`x-envoy-upstream-healthchecked-cluster`, `server`, `connection: close`, no body bytes. The
round-1 fix (`e569f5b`) chose `transfer-encoding: chunked` at the decode-side intercept, so the
reply carried BOTH framing headers — a regression from the pre-fix tree, which was at parity.

## Why only one probe

Every `GET` reply on this listener carries `content-length: 7` over a 0- or 4-byte body, so a
content-length read cannot terminate on it; the `GET` cell is pinned in-process instead
(`h1_health_check_head_intercept_keeps_a_later_content_length_alone`). A `HEAD /other` control is
not listed for the reason `0095`'s README gives: the driver reads a HEAD reply's head only, so it
cannot see envoy-rust's stray `direct_response` body (`CF-115-14`) and would read as a parity
witness it does not give.

## Byte-identical configs

`envoy.yaml` and `envoy-rust.yaml` are **byte-identical** (`cmp` silent). Re-derive it.

## Running it

```bash
cargo build -p envoy-bin
cargo test -p differential --test http_filter_health_check_framing
```

A green run in about a second is normal for a backend-free fixture.

## Proof it is not vacuous

| # | mutation | result |
|---|---|---|
| V1 | restore the `e569f5b` ordering — settle the H1 framing at the decode site, before the encode pass | `p1` REDs on `diff_headers` |
