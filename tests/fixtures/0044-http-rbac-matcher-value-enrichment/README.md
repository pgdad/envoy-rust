# Fixture 0044 — RBAC matcher-VALUE enrichment: `present_match` + `safe_regex` (byte-exact)

The phase-36 witness fixture for BOTH phase-36 features in a single
cross-proxy differential: F1 (`present_match` `ValueMatcher` variant on an
RBAC `metadata` condition) and F2 (`safe_regex` `StringMatcher` on an RBAC
`metadata` value, compiled at lowering time). An H1 `direct_response`
listener whose filter chain is

```
[envoy.filters.http.header_to_metadata, envoy.filters.http.rbac, envoy.filters.http.router]
```

drives 4 sequential `GET /` probes, each carrying different request headers
to select which policy (if any) fires. The harness asserts both proxies
return a byte-identical verdict — status + body + header-set — for each
probe via the `http1_probe_list` driver (the same driver fixtures 0017 and
0043 use).

## The chain

The `header_to_metadata` producer extracts TWO request headers into dynamic
metadata under namespace `envoy.filters.http.header_to_metadata`:

- `x-tier` → key `tier` (drives F2: `safe_regex` match)
- `x-present` → key `present_probe` (drives F1: `present_match`)

The `rbac` consumer (`action: ALLOW`) has TWO policies that are OR'd — any
match yields ALLOW:

**Policy `f2_regex`** — F2 `safe_regex` on the `tier` key:

```yaml
permissions:
  - metadata:
      filter: envoy.filters.http.header_to_metadata
      path:
        - key: tier
      value:
        string_match:
          safe_regex: { regex: "^(prod|staging)$" }
principals:
  - any: true
```

**Policy `f1_present`** — F1 `present_match` on the `present_probe` key:

```yaml
permissions:
  - metadata:
      filter: envoy.filters.http.header_to_metadata
      path:
        - key: present_probe
      value:
        present_match: true
principals:
  - any: true
```

Producer-before-consumer chain order is REQUIRED: the `header_to_metadata`
filter must appear BEFORE the `rbac` filter so the consumer reads what the
producer wrote in the same decode pass.

## The 4-probe table

| probe | request headers                      | policy fired  | verdict | body                        |
|-------|--------------------------------------|---------------|---------|-----------------------------|
| a     | `x-tier: staging`                    | f2_regex      | ALLOW   | `ok\n` (200)                |
| b     | `x-tier: dev`                        | (none)        | DENY    | `RBAC: access denied` (403) |
| c     | `x-present: 1`, `x-tier: dev`        | f1_present    | ALLOW   | `ok\n` (200)                |
| d     | `x-tier: dev` (no `x-present`)       | (none)        | DENY    | `RBAC: access denied` (403) |

Probes a/b exercise F2 (safe_regex match/miss). Probes c/d exercise F1
(present_match present/absent). Probe b and probe d are the anti-trivial
deny guards: in probe b the `tier` leaf is populated but the value `dev`
does not match `^(prod|staging)$`; in probe d the `present_probe` leaf is
absent (no `x-present` header, no `on_header_missing` fallback configured).

## §A-locked facts this fixture pins

### §A1 — `present_match` semantics: `present && want`

`present_match: true` matches when the metadata leaf is PRESENT (the
`x-present` header was supplied so the producer wrote `present_probe` into
the store) AND `want` is `true`. `present_match: false` would require the
key to be absent — not exercised here. The runtime gate in `matches_resolved`
is: `Some(_) => *want, None => false` (for `want = true`, the only ALLOW
case is key-present). Probe c (present) → ALLOW; probe d (absent) → DENY.

### §A3b — ANCHORED pattern rationale

The `safe_regex` pattern is `^(prod|staging)$` — deliberately anchored.
Envoy's RE2 engine performs a FULL match by default; envoy-rust's `regex`
crate `is_match` performs a PARTIAL (substring) match. An unanchored pattern
`(prod|staging)` would produce a cross-proxy divergence on values like
`xprodx` (envoy-rust ALLOW, upstream Envoy DENY). The anchors `^` and `$`
make partial == full, eliminating the divergence for all 4 probes. The
unanchored-match gap is deferred as carry-forward M36-1 (not exercised here).

### §A1 (403 body) — 19-byte body, NO trailing newline

A miss under `action: ALLOW` (no policy fires) yields HTTP 403 with body
`"RBAC: access denied"` (19 bytes, NO trailing newline) — same as fixture
0043, same as fixture 0017. Pinned by ADR-0034.

## Per-side divergences

| Side       | bind address | admin block  | `generate_request_id` |
|------------|--------------|--------------|-----------------------|
| envoy      | `0.0.0.0`    | yes (port 0) | `false` (header-set parity) |
| envoy-rust | `127.0.0.1`  | omitted      | omitted (never injects) |

The per-side YAML asymmetry follows the fixture-0043/0017 precedent. The HCM
body is otherwise identical between the two sides.

## Driver

`Driver::Http1ProbeList` (`kind: http1_probe_list`). Each probe runs an
independent H1 request/response cycle through `drive_http1` and applies the
per-probe equivalence cascade (status + byte-exact body +
`set_equal_modulo_allow_list` header set). The `extra_headers` field injects
the per-probe header values.

## Cross-references

- ADR-0087: Phase 36 scope / brainstorm (F1 `present_match`, F2 `safe_regex`).
- ADR-0088: §6.2 reconciliation — §A1 `present && want` semantics, §A3b
  anchored-pattern rationale.
- Fixture 0043 (`0043-http-rbac-dynamic-metadata`): the metadata-consumer
  baseline that this fixture extends (single-policy `exact` match, 3 probes).
- ADR-0034: the `"RBAC: access denied"` 19-byte body (NO trailing newline).
