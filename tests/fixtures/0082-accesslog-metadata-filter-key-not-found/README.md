# Fixture 0082 — access-log `metadata_filter` with `match_if_key_not_found: false` (phase 74)

The sibling of fixture `0081-accesslog-metadata-filter`. Where `0081` witnesses
the **value** branch of the `metadata_filter` decision rule (the key resolves and
the value matcher decides), this fixture witnesses the **key-not-found** branch —
the observable that `--mode validate` provably cannot reach, because
`match_if_key_not_found` is a `google.protobuf.BoolValue` **wrapper** whose
default lives in the runtime, not the schema.

## What this proves

Same shape as `0081` (an `envoy.filters.http.header_to_metadata` filter mapping
`x-a` → `com.example:k`; a `text_format_source` file sink rendering
`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% M=%DYNAMIC_METADATA(com.example:k)%`;
one `direct_response` route `/x` → 200 `hi`), PLUS `match_if_key_not_found: false`
on the filter. Two probes:

| # | request | `com.example:k` | path resolves? | decided by | emitted? |
|---|---|---|---|---|---|
| 1 | `GET /x` (no `x-a`) | *(absent)* | **no** | `match_if_key_not_found: false` | **DROPPED** |
| 2 | `GET /x` `x-a: 1` | `"1"` | yes | `exact: "1"` → match | **KEPT** |

The access-log file on EACH proxy holds EXACTLY ONE byte-identical line
(MEASURED, SPEC §0 R-0.4, graceful-stop flush):

```
STATUS=200 PATH=/x M=1
```

## The polarity flip this fixture witnesses

`match_if_key_not_found` is a `google.protobuf.BoolValue` wrapper, so ABSENT and
explicit-`false` are DISTINCT on the wire, and the **measured default is `true`**.
Flipping the field is what makes the key-absent probe observable — MEASURED
against `envoyproxy/envoy:v1.33.0` on this exact config:

| `match_if_key_not_found` | real Envoy access log |
|---|---|
| **absent** (default `true`) | `STATUS=200 PATH=/x M=-`<br>`STATUS=200 PATH=/x M=1` — the key-absent record is **KEPT** (rendered `M=-`) |
| **`false`** (this fixture) | `STATUS=200 PATH=/x M=1` — the key-absent record is **DROPPED** |

envoy-rust models this as `Option<bool>` and resolves `None → true` at the HCM
compile step (`match_if_key_not_found.unwrap_or(true)`). Modelling the field as a
bare `bool` would collapse absent into `false` and DROP every key-absent record —
the exact opposite of upstream.

> **This fixture is non-vacuous — verified by mutation.** Deleting the
> `match_if_key_not_found: false` line from BOTH sides turns the fixture RED (the
> key-absent probe is then kept under the default `true`, so the log grows past
> the expected single line). The witness genuinely depends on the flag.

## Why `on_header_missing` is deliberately ABSENT (ADR-0155 PV-6)

The `header_to_metadata` rule here carries **`on_header_present` only**. This is
load-bearing, not an oversight:

- envoy-rust's `validate_header_to_metadata_config` requires a `value` on an
  `on_header_missing` block (`"on_header_missing for header '{}' requires a
  'value'"`), and the precedent fixture `0042` duly supplies `value: "missing"`.
- Cloning that block here would WRITE `com.example:k = "missing"` on the
  no-`x-a` probe. The key would then **RESOLVE**, the `exact: "1"` value matcher
  would say no, and the probe would be dropped by the **VALUE** path — not the
  key-not-found path.
- The fixture would still PASS (one line on each side) while silently testing the
  wrong thing, vacating the `match_if_key_not_found` witness entirely.

Omitting the block means a request without `x-a` writes nothing, so the key —
and indeed the whole `com.example` namespace — is genuinely absent. A missing
namespace behaves identically to a missing key (MEASURED R-0.4).

## Probes / driver

`kind: http1_access_log_byte_exact`. Probe ordering follows the **kept-LAST**
convention (ADR-0147): the DROPPED probe first, the KEPT probe last, so the
driver's ordering-aware `suppression_settle` pays the cheap 2 s `CF70_3_SETTLE`
rather than the 12 s `CF71_1_SETTLE`. The assertion is PURE cross-proxy equality —
both proxies must agree on the KEPT line AND on the ABSENCE of a line for the
key-absent probe.

`clusters: []` — the only route is a `direct_response`, so no backend spawns.

## Per-side divergences

| | `envoy.yaml` | `envoy-rust.yaml` |
|---|---|---|
| `admin` | present (port 0) | omitted |
| listener bind | `0.0.0.0` | `127.0.0.1` |
| `generate_request_id` | `false` (explicit) | omitted (not modelled) |
| access-log mount | `/tmp/0082-envoy-mount/` | `/tmp/0082-envoy-rust-mount/` |

Everything else — the `header_to_metadata` stanza, the log format and the whole
`metadata_filter` block including `match_if_key_not_found` — is byte-identical
between the two sides.

## Cross-references

- **ADR-0154** — the phase-74 pick (§5 state-0/1).
- **ADR-0155** — the §6.2 empirical reconciliation; PV-6 is the correction that
  produced the `on_header_missing` omission above.
- **Fixture `0081-accesslog-metadata-filter`** — the sibling value-branch witness.
- **Fixture `0042`** — the `header_to_metadata` producer precedent (and the source
  of the `on_header_missing` trap).
- `BEHAVIOR_CONTRACT.md` "Phase 74" subsection.
