# 0098 — `health_check` framing composed with a later encode-side `transfer-encoding`

Phase **115**, §5.2 state-3 re-entry round 3 (`ADR-0204`; phase-115 `REVIEW-3.md` I3-1, cell
B7). Two HTTP/1.1 probes against a **backend-free, CLUSTER-FREE** HCM listener whose chain is

```
[ health_check     (:path exact /healthz),
  header_mutation  (response transfer-encoding: gzip, APPEND_IF_EXISTS_OR_ADD),
  router ]
```

with a `prefix: "/"` `direct_response` catch-all answering `MAIN`.

## What it witnesses

| probe | request | expected | rule |
|---|---|---|---|
| `p1` | `HEAD /healthz` | empty; `transfer-encoding: chunked` alone, NO `content-length` | the codec owns a headers-only reply's `transfer-encoding`: the stage-written `gzip` never reaches the wire |
| `p2` | `GET /healthz` | empty; `content-length: 0` alone, NO `transfer-encoding` | the non-`HEAD` twin |

MEASURED against `envoyproxy/envoy:v1.33.0` (`REVIEW-3.md` I3-1 B7, and by this fixture): upstream
sends `transfer-encoding: chunked` on `p1` and `content-length: 0` on `p2`. Before `ADR-0204`
envoy-rust sent `transfer-encoding: gzip` on `p1`.

**What discriminates is the `transfer-encoding` VALUE.** Both proxies send a `transfer-encoding`
on `p1`, so the header-NAME sets agree; `diff_headers` then compares each non-allow-listed
header's value, and `gzip` ≠ `chunked`. The driver reads a `HEAD` reply's head only (RFC 9110
§9.3.2), so `p1`'s `expected_body: ""` cannot fail either way (`REVIEW-3.md` M3-5).

## Why no `/other` probe

On a reply that is NOT headers-only envoy-rust keeps a stage-written `transfer-encoding`
(`CF-115-16` (d), pre-existing, not this filter's), and a `HEAD /other` reply also carries
envoy-rust's stray `direct_response` body (`CF-115-14`). An `/other` probe would red on a
divergence this phase does not own.

## Why `APPEND` and not `OVERWRITE`

One config carries one action. `APPEND_IF_EXISTS_OR_ADD` is upstream's proto default; the
`OVERWRITE_IF_EXISTS_OR_ADD` cell (A3) gives the same wire on both proxies and is pinned
in-process (`h1_health_check_intercept_replaces_a_stage_written_transfer_encoding`, both
actions).

## Byte-identical configs

`envoy.yaml` and `envoy-rust.yaml` are **byte-identical** (`cmp` silent). Re-derive it.

## Running it

```bash
cargo build -p envoy-bin
cargo test -p differential --test http_filter_health_check_transfer_encoding
```

A green run in about a second is normal for a backend-free fixture.

## Proof it is not vacuous

| # | tree | result |
|---|---|---|
| V1 | the unfixed `a1c1623` code (`envoy-bin` md5 `8f0daf79d7cecf7af4ab6f10f31ec61e`) | `p1` REDs on `diff_headers`: `transfer-encoding: envoy=chunked envoy-rust=gzip` |
| V2 | restore `settle_headers_only_framing`'s old rule (3) alone — keep a stage-written `transfer-encoding` when no `content-length` is present | `p1` REDs on `diff_headers` (a first attempt hit upstream's own accept-ready wait before any probe and was re-run; the restored `envoy-bin` md5 equals the pre-mutation one and the fixture re-ran GREEN) |
