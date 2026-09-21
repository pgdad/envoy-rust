# 0095 — `envoy.filters.http.health_check` (non-pass-through mode)

Phase **115** (`ADR-0200` pick, `ADR-0201` PLAN-write). Ten HTTP/1.1 probes against a
**backend-free, CLUSTER-FREE** HCM listener whose chain is

```
[ health_check A  (:path exact /healthz),
  health_check B  (:path exact /both  AND  x-probe exact yes),
  router ]
```

with a `prefix: "/"` `direct_response` catch-all answering the 4-byte body `MAIN`.

## Why it is not vacuous

**Every probe answers 200**, so status alone cannot pass it. An intercepted probe has an EMPTY
body and no `content-type`; a fall-through answers `MAIN` with `content-type: text/plain`. Each
probe asserts the body byte-exact, and `set_equal_modulo_allow_list` compares
`x-envoy-upstream-healthchecked-cluster` VALUE-exact on both proxies.

## What it witnesses

| probe | request | expected | rule |
|---|---|---|---|
| `p1` | `GET /healthz` | empty | the baseline intercept |
| `p2` | `GET /healthz?x=1` | `MAIN` | **`:path` includes the query string** — the trap of the phase |
| `p3` | `GET /healthz/` | `MAIN` | exact is exact |
| `p4` | `GET /healthZ` | `MAIN` | the value match is case-sensitive |
| `p5` | `GET /other` | `MAIN` | a non-match continues to the route |
| `p6` | `POST /healthz` + body | empty | the filter is method-agnostic |
| `p7` | `GET /healthz` + `x-envoy-upstream-healthchecked-cluster: SENTINEL` | empty, header = `hc-fixture-cluster` | the header is proxy state, not a request echo |
| `p8` | `GET /both` + `x-probe: yes` | empty | both matchers match |
| `p9` | `GET /both` | `MAIN` | the list is AND, not OR |
| `p10` | `GET /other` + `x-probe: yes` | `MAIN` | …in the other direction |

**The header value is the bootstrap `node.cluster`.** `SPEC.md` §2.2 recorded it as EMPTY; that
was measured on a config with no `node:` block. With `node.cluster` set it carries that string
(`ADR-0201`), so this fixture sets `node: { id: fixture-0095, cluster: hc-fixture-cluster }` —
on BOTH sides, with a value no YAML-1.1 parser booleanizes.

## Byte-identical configs

`envoy.yaml` and `envoy-rust.yaml` are **byte-identical** (`cmp` silent). This is a per-fixture
claim; re-derive it, do not inherit it.

## Running it

Backend-free (no `{{BACKEND_IP}}`), so it is fully verifiable on a developer host. The harness
runs the DEBUG `envoy-bin`, so rebuild it first:

```bash
cargo build -p envoy-bin
cargo test -p differential --test http_filter_health_check
```

A green run in a few seconds is normal for a backend-free fixture.

## Proof it is not vacuous

Each mutation was applied, `envoy-bin` rebuilt, the fixture run, and the file restored
(md5-verified). The driver aborts at the FIRST failing probe, so each red run names one probe.

| # | mutation | result |
|---|---|---|
| V1 | stamp an empty `local_cluster` in `validate_hcm` | `p1` REDs on `diff_headers` |
| V2 | strip the query string before matching `:path` | `p2` REDs (`MAIN` expected, empty returned) |
| V3 | fold the matchers with `any` instead of `all` | `p9` REDs (`MAIN` expected, empty returned) |
