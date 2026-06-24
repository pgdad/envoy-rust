# Fixture 0043 — RBAC `metadata` matcher over header-driven dynamic metadata (byte-exact)

The phase-35 witness fixture for the FIRST dynamic-metadata CONSUMER: an
HTTP RBAC filter whose Permission is a `metadata` matcher reading the
per-request dynamic-metadata store that an upstream `header_to_metadata`
PRODUCER populated in the same decode pass. An H1 `direct_response`
listener whose filter chain is

```
[envoy.filters.http.header_to_metadata, envoy.filters.http.rbac, envoy.filters.http.router]
```

drives 3 sequential `GET /` probes carrying (or omitting) the `x-tier`
request header. The harness asserts both proxies return a byte-identical
verdict — status + body + header-set — for each probe via the same
`http1_probe_list` driver fixture 0017 uses.

## The chain

The `header_to_metadata` producer extracts the `x-tier` request header
into dynamic metadata under namespace
`envoy.filters.http.header_to_metadata`, key `tier`. The `rbac` consumer
(`action: ALLOW`, one policy `tier_prod`) requires that metadata leaf to
string-match `"prod"` via a `metadata` Permission paired with an
`any: true` principal (so the Permission is the decision discriminator).

```yaml
permissions:
  - metadata:
      filter: envoy.filters.http.header_to_metadata
      path:
        - key: tier
      value:
        string_match: { exact: "prod" }
```

## §A-locked facts this fixture pins

- **§A1 — `metadata` matcher wire shape.** The Permission is
  `{ filter, path: [{ key }], value: { string_match: <StringMatcher> } }`.
  This shape round-trips verbatim against upstream Envoy v1.33.0 (verified
  at phase-35 state-2 §6.2 recon).

- **§A2 — `filter` ↔ producer-namespace correspondence + REQUIRED order.**
  The RBAC matcher's `filter` field MUST equal the producer's
  `metadata_namespace`; both are set explicitly to
  `envoy.filters.http.header_to_metadata` here. Producer-before-consumer
  chain order is REQUIRED: the `header_to_metadata` filter must appear
  BEFORE the `rbac` filter so the consumer reads what the producer wrote
  in the same decode pass. Reordering them (or omitting the producer)
  leaves the metadata leaf unset and the match always fails. NOTE: this
  chain-order/producer-omitted requirement is asserted by this doc and
  COVERED BY THE IN-PROCESS BACKSTOP (Task 4) — no probe here exercises
  the reorder/omit case. Probe 3 below leaves the leaf unset because the
  INPUT HEADER is absent (the producer still runs), a path distinct from
  the leaf being unset because the PRODUCER FILTER did not run.

- **§A3 — byte-exact verdicts + full StringMatcher reuse.** A match
  yields 200 + `"ok\n"` (3 bytes); a non-match yields 403 +
  `"RBAC: access denied"` (19 bytes, NO trailing newline, ADR-0034).
  The `value` is a full `StringMatcher` — all of its modes (`exact`,
  `prefix`, `suffix`, `contains`, `safe_regex`) are reused as-is from the
  existing RBAC string-match machinery; this fixture pins `exact` but the
  matcher is not restricted to it.

- **§A4 — config-validity divergence (boot-fatal).** An empty `filter`,
  a missing `value`, or an empty `path` is boot-fatal on both proxies
  (the matcher cannot resolve a namespace/leaf). Non-differential: the
  divergence is in error-message text, not in accept/reject.

- **§A5 — multi-segment `path` stricter-reject.** envoy-rust rejects a
  multi-segment `path` (more than one `{ key }` segment). Upstream Envoy
  walks nested `Struct` segments; envoy-rust supports only a single
  top-level key at v1.33.0 parity, so this is a STRICTER reject (config
  rejected at boot rather than silently no-matching). Non-differential
  for the present fixture's single-segment path.

- **§A6 — non-`string_match` value stricter-reject.** The `value` oneof
  admits other `ValueMatcher` modes upstream (e.g. `list_match`,
  `bool_match`, `present_match`); envoy-rust supports only `string_match`
  at this parity point and STRICTER-rejects the others at boot.

- **§A7 — deprecated-but-functional `metadata` field is non-differential.**
  At v1.33.0 the `metadata` Permission field is marked deprecated (the
  forward-looking path is the generic `Matcher`), but it remains fully
  FUNCTIONAL on upstream Envoy v1.33.0. Both proxies honour it
  identically, so the deprecation is non-differential here.

## The present + mismatch + absent probe TRIO (the anti-trivial guard)

| # | request                  | metadata `tier` | verdict | body                  |
|---|--------------------------|-----------------|---------|-----------------------|
| 1 | `GET /` + `x-tier: prod` | `prod`          | ALLOW   | `ok\n` (200)          |
| 2 | `GET /` + `x-tier: dev`  | `dev`           | DENY    | `RBAC: access denied` (403) |
| 3 | `GET /` (no `x-tier`)    | (unset)         | DENY    | `RBAC: access denied` (403) |

The trio is the anti-trivial guard. Probe 1 proves the match path fires.
Probe 2 proves a PRESENT-but-WRONG value still fails the `exact: "prod"`
match (this is not an allow-all — the metadata leaf is populated yet the
verdict is DENY). Probe 3 proves an ABSENT key (no `on_header_missing`
fallback is configured, so the producer leaves the leaf UNSET) also
fails the match. Probes 2 and 3 reach the SAME metadata-lookup path and
diverge only in whether the leaf is set-but-mismatched vs unset — both
correctly resolve to the default-Deny under `action: ALLOW`.

> NOTE: this fixture deliberately omits any `on_header_missing` rule on
> the `header_to_metadata` producer. Adding a fallback would populate the
> leaf for the absent-header probe and change probe 3's semantics.

## Per-side divergences

| Side       | bind address | admin block  | `generate_request_id` |
|------------|--------------|--------------|-----------------------|
| envoy      | `0.0.0.0`    | yes (port 0) | `false` (header-set parity) |
| envoy-rust | `127.0.0.1`  | omitted      | omitted (never injects) |

The per-side YAML asymmetry follows the fixture-0017 precedent. The HCM
body is otherwise identical between the two sides.

## Driver

`Driver::Http1ProbeList` (`kind: http1_probe_list`). Each probe runs an
independent H1 request/response cycle through `drive_http1` and applies
the per-probe equivalence cascade (status + byte-exact body +
`set_equal_modulo_allow_list` header set). The `extra_headers` field
injects the per-probe `x-tier` value (or `[]` for the absent probe).

## Cross-references

- ADR: ADR-0085 (scope / brainstorm), ADR-0086 (§6.2 reconciliation).
- Related fixtures: 0042 (`header_to_metadata` PRODUCER + `%DYNAMIC_METADATA%`
  byte-exact — the producer half), 0017 (RBAC `header` principal byte-exact —
  the `http1_probe_list` + 403-body baseline), 0007 (H1 direct_response baseline).
- ADR-0034: the `"RBAC: access denied"` 19-byte body (NO trailing newline).
