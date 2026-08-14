# 0088 — route `match.runtime_fraction` gating

Sub-phase **109.2** (ADR-0175 pick, ADR-0176 split). The **first differential witness of
`runtime_fraction` in the corpus**, and the first fixture combining `Http1ProbeList` traffic
with a `layered_runtime` block.

Ten HTTP/1.1 probes against a **backend-free, CLUSTER-FREE** HCM listener (`clusters: []`,
`direct_response` routes only), requiring identical `(status, body, header-set-modulo-allow-list)`
between upstream Envoy `v1.33.0` and envoy-rust.

Nine routes carry a `match.runtime_fraction`; a **two-static-layer** `layered_runtime` block
supplies the consulted values. The tenth route is a bare `prefix: "/"` catch-all. Each probe has
a **DISTINCT `path:`** (the attribution rule, `BEHAVIOR_CONTRACT.md` "Why every probe carries a
DISTINCT `path:`") **and each gated route answers a DISTINCT body** `P<N>-GATED`, so **the
response body IS the gate's verdict**: a gate that wrongly PASSES answers `P<N>-GATED` where
`CATCH` is expected, and a gate that wrongly BLOCKS answers `CATCH` where `P<N>-GATED` is
expected. Both directions are covered — five probes expect a gated body, five expect `CATCH`.

## What it witnesses

The deterministic subset of the **23-cell** matrix MEASURED against `envoyproxy/envoy:v1.33.0`
(parent `109/SPEC.md` §1.1 — 13 cells; `109.1/SPEC.md` §1.2 — 10 V-8 closure cells):

| probe | path | `default_value` | consulted key = static value | expected body | rule witnessed |
|---|---|---|---|---|---|
| `p1` | `/p-default-on` | 100/HUNDRED | `gate.absent.on` (**ABSENT**) | `P1-GATED` | absent key ⇒ `default_value` honoured |
| `p2` | `/p-default-off` | 0/HUNDRED | `gate.absent.off` (**ABSENT**) | `CATCH` | …and in the OTHER direction |
| `p3` | `/p-key-zero` | 100/HUNDRED | `gate.zero` = `0` | `CATCH` | a consulted key **OVERRIDES** the default |
| `p4` | `/p-key-hundred` | 0/HUNDRED | `gate.hundred` = `100` | `P4-GATED` | …and in the OTHER direction |
| `p5` | `/p-key-twohundred` | 0/HUNDRED | `gate.twohundred` = `200` | `P5-GATED` | `v >= 100` always passes |
| `p6` | `/p-quoted-zero` | 100/HUNDRED | `gate.quoted` = `"0"` | `CATCH` | a quoted numeric string parses like the integer |
| `p7` | `/p-unparseable` | 100/HUNDRED | `gate.abc` = `abc` | `P7-GATED` | an unparseable value falls back to `default_value` |
| `p8` | `/p-two-layer` | 100/HUNDRED | `gate.layered` = base `100`, override `0` | `CATCH` | **last-layer-wins `final_value`** |
| `p9` | `/p-million` | 0/**MILLION** | `gate.million` = `100` | `P9-GATED` | **an integer value is the numerator over HUNDRED, NOT over the default's denominator** |
| `p10` | `/p-catch` | (bare `prefix: "/"` catch-all) | — | `CATCH` | the ungated control |

**`p9` is the load-bearing cell.** Under the wrong reading (the runtime value scaled by the
default's denominator) a value of `100` against a `0/MILLION` default is a ~10⁻⁴ event per
request — a divergence **no 0/100 fixture could ever catch**. Its witness comes from the
CONSULTED value, not the default: proved by mutation (see below).

## Deliberately absent cells, and why

Every **per-request-NONDETERMINISTIC** cell — integer `50`, floats `0.5` / `1.5`, the quoted
`"0.5"`, i.e. the whole `0 < v < 100` class — is **boot-fatal** in envoy-rust under **CF-109-1
(WIDENED)** and is witnessed IN-PROCESS by 109.1's reject tests, never here. Likewise the
map-shaped consulted key (**CF-109-2**, the snapshot-prefix rule) and `runtime_fraction` inside
`jwt_authn` rules (**CF-109-3**). **A fixture cannot witness a config that refuses to boot** —
there is no wire behaviour to compare.

**No float spelling that is not Display-stable may EVER enter this or any fixture.** Upstream
renders runtime floats as raw **SOURCE TEXT** (`1.50` → `"1.50"`, `1e6` → `"1e6"`, `.nan` →
`".nan"`) while envoy-rust renders `f64` Display — CF-108-5, closed by **ADR-0174**. Only
Display-stable spellings are safe; this fixture's values are all integers or plain strings.

## Two shape decisions, both MEASURED (not inherited)

**There is no `node:` block.** Every other probe-list fixture carries `node: { id: x, cluster: y }`
— **on the envoy-rust side only**, and that asymmetry is not cosmetic. Upstream parses **YAML 1.1**,
which booleanizes the unquoted `y`, and `node.cluster` is a protobuf **string** field, so upstream
rejects the config at boot:

```
invalid JSON in envoy.config.bootstrap.v3.Bootstrap @ node.cluster: string … unexpected character: 't'
```

with the rendered JSON showing `"cluster":true`. `0088` omits `node:` entirely, which BOTH proxies
accept — **and that omission is exactly what makes the two YAMLs byte-identical.**

**Admin uses a literal `port_value: 0`, not `{{ADMIN_PORT}}`.** The `{{ADMIN_PORT}}` substitution
is **driver-gated**: `driver_needs_admin_port` (`tests/differential/src/lib.rs`) matches only
`AdminScrape` / `Http1KeepAlive` / `Http2KeepAlive` / `TcpWithStats` — **`Http1ProbeList` is
absent** — and `render_yaml` leaves an unmatched token UNTOUCHED by design, so a literal
`{{ADMIN_PORT}}` would reach the parser and fail as an address. This follows the `0083`/`0086`
convention. `{{PORT}}` **is** substituted for this driver.

## Byte-identical configs

`envoy.yaml` and `envoy-rust.yaml` are **BYTE-IDENTICAL** (126 lines each, `cmp` silent). Of the
87 pre-existing pairs exactly **one** (`0027-xds-file-based-lds`) is byte-identical, so `0088` is
the **SECOND** such pair. **This is a per-fixture claim, re-derived here — never a tree property**;
any later fixture must re-derive it rather than inherit this sentence.

## Running it

Backend-free: there is **no `{{BACKEND_IP}}` marker**, so no backend container spawns and the
fixture is **NOT** in the class that REDs on a developer host over the `192.168.65.2` bridge-IP
routing. It is fully verifiable locally.

```bash
cargo build -p envoy-bin      # the harness runs the DEBUG binary; a stale pre-109.1
                              # binary rejects `runtime_fraction` as an unknown field
cargo test -p differential --test runtime_fraction_route_gating
```

Cold ≈ 8 s, warm ≈ 1.1 s. **A backend-free fixture completing in ~1-3 s is NORMAL, not a silent
skip** — if you want proof the containers really ran, poll `docker ps` during the run and resolve
by container/image ID (and use a `docker ps` format field that actually exists: an invalid
`{{.ImageID}}` makes every poll line a template error and fakes a clean "no containers" reading).

## Proof it is not vacuous

Both mutations were run in-place and reverted byte-exactly (md5-verified):

| # | mutation | result |
|---|---|---|
| V1 | `override_layer`'s `gate.layered: 0` → `100`, BOTH yamls | probe `p8-two-layer-last-wins` REDs (`CATCH` expected, `P8-GATED` returned) — the fixture witnesses last-layer-wins precedence, not merely the base layer |
| V2 | `p9`'s `runtime_key` → an ABSENT key so the `0/MILLION` default decides | probe `p9-integer-is-numerator-over-hundred` REDs (`P9-GATED` expected, `CATCH` returned) — p9's witness comes from the CONSULTED value |

Note the driver **aborts at the FIRST failing probe**, so one red run names exactly ONE probe;
do not infer a second cell's state from a single red run.
