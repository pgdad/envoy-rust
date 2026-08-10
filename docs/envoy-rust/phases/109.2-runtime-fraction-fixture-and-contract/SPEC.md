# Sub-phase 109.2 — Runtime CONSUMER slice 1b: differential fixture `0088-runtime-fraction-route-gating`, the `BEHAVIOR_CONTRACT.md` `## Runtime` consumer subsection, the decided-in M-1 correction, and the parent-109 close

> Split child of phase 109 (`109-runtime-fraction-route-gating`), fired at the
> §5 state-2 PLAN-write per §6.1 (**ADR-0176**; parent pick ADR-0175). Written
> for a reader with ZERO prior context (D-3.4). PRECONDITION: sibling
> **109.1** (`109.1-runtime-fraction-config-and-gate`) is `done` — the
> `RouteMatch.runtime_fraction` gate is live at both `route_matches` call
> sites, the three boot-fatal validators (CF-109-1/2/3) are wired at all
> three validation paths, and the typed lookup is unit-pinned against the
> full measured matrix. This slice adds the DIFFERENTIAL witness and the
> contract record, then closes parent row `109`.

## §1. The measured fixture contract (dry-run at the phase-109 state-2 split session)

The EXACT ten-route shape below was composed, `--mode validate`-checked and
served against the pinned `envoyproxy/envoy:v1.33.0` (digest verified) at the
split session; all ten probe expectations held across 3 independent passes,
fully deterministic. The envoy-rust side (the same YAML minus the
`runtime_fraction` blocks, since the field lands in 109.1) booted the debug
`envoy-bin`, served the probes, and rendered the gate keys in `/runtime`
(`gate.layered` `final_value: "0"` — last-wins live in the store). This
closes parent PLAN-VERIFY V-2: `Http1ProbeList`-shaped HCM traffic and
`layered_runtime` coexist on BOTH proxies. The dry-run is a CLAIM the 109.2
state-3 session re-establishes cross-proxy through the harness.

Routes (one path + one runtime key per probe — the `BEHAVIOR_CONTRACT.md` §G
attribution rule; each `direct_response` body distinct; final bare
catch-all):

| # | path | default_value | consulted key = static value | measured result |
|---|---|---|---|---|
| 1 | `/p-default-on` | 100/HUNDRED | `gate.absent.on` (ABSENT) | `P1-GATED` |
| 2 | `/p-default-off` | 0/HUNDRED | `gate.absent.off` (ABSENT) | `CATCH` |
| 3 | `/p-key-zero` | 100/HUNDRED | `gate.zero` = `0` | `CATCH` |
| 4 | `/p-key-hundred` | 0/HUNDRED | `gate.hundred` = `100` | `P4-GATED` |
| 5 | `/p-key-twohundred` | 0/HUNDRED | `gate.twohundred` = `200` | `P5-GATED` |
| 6 | `/p-quoted-zero` | 100/HUNDRED | `gate.quoted` = `"0"` | `CATCH` |
| 7 | `/p-unparseable` | 100/HUNDRED | `gate.abc` = `abc` | `P7-GATED` |
| 8 | `/p-two-layer` | 100/HUNDRED | `gate.layered` = base `100`, override `0` | `CATCH` |
| 9 | `/p-million` | 0/**MILLION** | `gate.million` = `100` | `P9-GATED` |
| 10 | `/p-catch` | (bare `prefix: "/"` catch-all) | — | `CATCH` |

`layered_runtime`: TWO static layers (`base_layer` carries the seven gate
keys incl. `gate.layered: 100`; `override_layer` carries only
`gate.layered: 0` — the precedence witness). All values integer or string —
map-shaped, fractional, bool and non-integral-float values are boot-fatal
after 109.1 (CF-109-1/2) and are witnessed by 109.1's in-process reject
tests, NOT here.

## §2. Scope

**D1 — fixture `0088-runtime-fraction-route-gating`.** Files `envoy.yaml`,
`envoy-rust.yaml`, `expectations.yaml`, `README.md` under
`tests/fixtures/0088-runtime-fraction-route-gating/`. Cluster-free,
backend-free, `Driver::Http1ProbeList`, ten probes with `expected_status:
200` + byte-exact `expected_body` per the §1 table. Shape: `node` + `admin`
(`{{ADMIN_PORT}}`) + one HCM listener (`{{PORT}}`) + `codec_type: HTTP1` +
the ten routes + router `http_filters` + `clusters: []` + the two-layer
`layered_runtime` — the fixture-0087 conventions. **Byte-identical YAMLs on
both sides is the goal and is BELIEVED ACHIEVABLE** (every construct is
modeled on both sides after 109.1; today exactly 1 of 87 fixture pairs is
byte-identical, and `0087` misses only on the echo-filter `@type` spelling
that `0088` does not use) — but it is a PLAN-VERIFY item (§4 X-1), not an
inherited fact.

**D2 — `expectations.yaml`.** Status + body per probe; no header
assertions beyond the existing `Http1ProbeList` defaults; no stats
assertions (the nine `runtime.*` stats are startup-set and unmoved — parent
§8; fixture `0087` already witnesses them).

**D3 — the `BEHAVIOR_CONTRACT.md` `## Runtime` consumer subsection.**
Records: the full 23-cell measured matrix (13 pick cells + the 10 V-8
closure cells from `109.1/SPEC.md` §1.2), the §1.3 evaluation cascade, the
three reject-direction carry-forwards (CF-109-1/2/3) with their unblock
conditions, and the fixture-`0088` pointer. Placed inside the existing
`## Runtime` section (`BEHAVIOR_CONTRACT.md:3162-3238`), AFTER the layer
grammar / `GET /runtime` material.

**D4 — the decided-in M-1 correction (ADR-0176; banked at the 108.2
REVIEW).** The contract's claim "`GET /runtime` … GET-only (POST → 405
`allow: GET` on both sides)" (`BEHAVIOR_CONTRACT.md:3180-3181`, and the
sibling claim near `:1379`) is MEASURED FALSE on the upstream side: upstream
v1.33.0 answers `POST /runtime` (and `DELETE /runtime`, and `POST
/config_dump`) with **200 + the full body** — it method-restricts NO
read-only admin endpoint (control: `GET /runtime_modify` → 405, so the probe
discriminates; measured at the 108.2 state-5 review). Correct both contract
sentences to record the TRUE asymmetry: envoy-rust 405s non-GET by the
deliberate 06.1/08 house convention; upstream serves them — a recorded,
tree-wide, fixture-unwitnessed reject-direction divergence. Also correct the
test doc at `crates/envoy-admin/src/endpoint.rs:3318-3319` ("GET-only on
BOTH sides" — same falsehood; shipping-code doc, editable). Locate all three
by TEXT — the line numbers WILL have drifted. The banked M-2 and N-1..N-6
stay banked (§6.3; ADR-0165) — M-1 alone was decided in because 109.2
legitimately rewrites the surrounding section.

**D5 — parent close.** At 109.2's state-6: ROADMAP rows `109.2` AND parent
`109` flip `done` together (the 76.2/108.2 two-row precedent — assert each
row's own starting status).

NOT edited: fixtures `0011`/`0087` (re-confirmed at the split session: zero
`runtime_fraction` hits; `0011`'s nine `runtime.*` allow-list entries remain
inert-but-harmless per the contract's fixture-0011 paragraph — the
set-difference argument re-read at the split session), `HEADER_ALLOW_LIST`
(3 entries), `known-failures.txt` (21 lines / ONE real entry), any landed
artifact (D-3.5).

## §3. Differential surface at sub-phase end

- NEW fixture `0088` green cross-proxy on all ten probes — backend-free,
  locally runnable (no `{{BACKEND_IP}}`, no host-RED class).
- All 87 pre-existing fixtures still green; CI identity moves only by the
  new differential test.
- Conformance unchanged (h2spec threshold untouched).

## §4. PLAN-VERIFY items for the 109.2 state-2 session

- **X-1** — byte-identical YAML feasibility: confirm the harness substitutes
  `{{ADMIN_PORT}}` + `{{PORT}}` for `Http1ProbeList` fixtures (0087 is
  `AdminScrape`; grep the substitution site in
  `tests/differential/src/lib.rs`), and that envoy-rust accepts the FULL
  upstream spelling (HCM `typed_config` `@type`, router filter, `admin`
  block). Fall back to the 0086 two-spelling convention ONLY on a measured
  gap, recorded in the fixture README.
- **X-2** — re-run the §1 dry-run against the pinned image before freezing
  `expectations.yaml` (the transcript above is a claim, not an inheritance),
  and re-establish the envoy-rust side WITH `runtime_fraction` present (the
  109.1-landed parser now accepts it).
- **X-3** — re-derive the fixture census (87 dirs, highest `0087`) — `0088`
  must still be the next free number.
- **X-4** — locate the three M-1 texts by their WORDS (contract twice,
  `endpoint.rs` once); line numbers drift.
- **X-5** — re-read the parent-109 `SPEC.md` §1.1 and `109.1/SPEC.md` §1.2
  matrices and transcribe the contract subsection from THEM, not from this
  file's summary.

## §5. Size estimate

Fixture YAMLs ≈ 210 (two ~105-line files, byte-identical), expectations ≈
80-120, README ≈ 60-80, contract subsection + M-1 ≈ 80-120, differential
test registration ≈ 10-20. **≈ 440-550 net LoC** — comfortably under the
gate; the risk axis is fixture-shape friction (X-1), not volume.

## §6. Next state

This SPEC is the §6.2 step-3 redistribution output. 109.2 enters its own §5
state-2 (`superpowers:writing-plans`) ONLY after sibling 109.1 is `done`.
Its state-6 closes parent row `109` (D5).
