# Phase 112 — TLS ALPN negotiation

> **Status:** §5 state-0/1 complete. `SPEC.md` is the only artifact in this
> directory. `PLAN.md` is §5 state 2 and belongs to a SEPARATE session.
>
> **Scoping ADR:** `ADR-0183` (`docs/envoy-rust/DECISIONS.md`).
> **ROADMAP row:** `112`, `status: planned`, under `### HTTP/3 + QUIC family`.

---

## §0. How to read this document

This phase opens the **HTTP/3 + QUIC family**, which has carried zero ROADMAP
rows since phase 00 seeded the heading. It does **not** build HTTP/3. It
discharges the family's first blocking prerequisite — TLS **ALPN** negotiation —
which QUIC mandates (`h3` is an ALPN protocol identifier) and which envoy-rust
today rejects at config-parse time.

That framing has an exact, one-pick-old precedent: `ADR-0181` filed *"HTTP/2
response trailer forwarding"* — not itself a gRPC feature — under
`### gRPC family` as that family's first blocking prerequisite. §2.1 argues the
filing explicitly rather than assuming it, because ALPN's nearest-term consumer
is HTTP/2-over-TLS, not HTTP/3, and that is a real objection.

Every figure below was measured at HEAD
`64609681b2de25c55db36c57cc0c13134d7eb7d2` unless it names another commit.
Figures produced by recon subagents were re-verified on disk by the main
session; §1.3 lists the ones that did **not** survive.

---

## §1. State-0 recon — the evidence this pick rests on

### §1.1 The divergence, MEASURED on BOTH proxies at this session

A scratch TLS `tcp_proxy` listener config was written to the session scratchpad
(never into the repo tree — `git status --porcelain` stayed clean throughout),
carrying a self-signed leaf and:

```yaml
common_tls_context:
  alpn_protocols: ["h2", "http/1.1"]
```

Upstream Envoy ran as a container on the pinned image, with the digest verified
**on the running container** as
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`
(matching `docs/envoy-rust/ENVOY_TARGET.md`), on host-loopback-mapped ports that
were asserted free before the run. The client was `openssl s_client -alpn`.

| client's ALPN offer | upstream Envoy v1.33.0 | envoy-rust @ `6460968` |
|---|---|---|
| `h2,http/1.1` | **`ALPN protocol: h2`** | **boot-fatal — no listener exists** |
| `http/1.1` | **`ALPN protocol: http/1.1`** | **boot-fatal — no listener exists** |
| `h3` (absent from the server list) | **`No ALPN negotiated`**, handshake SUCCEEDS | **boot-fatal — no listener exists** |
| *(client offers nothing)* | **`No ALPN negotiated`** | **boot-fatal — no listener exists** |

envoy-rust's exact failure on the byte-identical config:

```
parsing bootstrap YAML: static_resources.listeners[0].filter_chains[0]
.transport_socket: unknown field `alpn_protocols`,
expected `tls_certificates` or `validation_context`
```

**Five findings, each load-bearing:**

- **F1 — the divergence is TOTAL and BOOT-LEVEL, not a value divergence.**
  envoy-rust does not mis-negotiate ALPN; it refuses to start, so every cell in
  the table is unreachable. A fixture therefore cannot pass vacuously: before
  the phase, envoy-rust does not come up at all on the fixture config.

- **F2 — an ALPN MISMATCH does not fail the handshake.** RFC 7301 §3.2 permits
  a `no_application_protocol` fatal alert when no offered protocol is
  acceptable. Upstream Envoy declines to send one: the handshake completes with
  no protocol selected. **This is the single most likely cell to get wrong by
  reasoning from the RFC instead of from the measurement.** Whether `rustls`'
  `ServerConfig` shares that disposition is **not measured** — it is PV-2, and
  it is the phase's highest-risk unknown (§8).

- **F3 — selection followed SERVER preference.** The client offered
  `h2,http/1.1` and received `h2`, which is also first in the server's list — so
  this probe **does not discriminate** server-preference from client-preference.
  The discriminating probe (server list reversed against the same client offer)
  was NOT run. It is PV-1, and it decides one fixture cell.

- **F4 — the negative control is exact.** The identical config **minus the
  single `alpn_protocols` line** (`diff` reports exactly `19d18`) turns both
  positive cells into `No ALPN negotiated`. The field is doing the work.
  `listener.0.0.0.0_10000.downstream_cx_total` read `4` and `2` across the two
  runs, matching the probe counts exactly — the connections genuinely reached
  the Envoy under test rather than anything else on the host.

- **F5 — the FIRST attempt at this probe was a false green, and was
  discarded.** The initial `docker run` failed with
  `Bind for 0.0.0.0:18443 failed: port is already allocated` (exit 125), yet the
  `openssl` probes still returned a plausible `ALPN protocol: h2`. They had
  reached a **foreign service** — the concurrent workstream's
  `curl-world-tls-1` nginx container. The measurement was re-run on ports
  asserted free, with `docker ps --filter id=` and `docker inspect` proving the
  answering server was ours and on the pinned digest. **A probe that does not
  assert WHICH server answered is not a measurement.** This host runs a parallel
  workstream; the same shape will recur for any future port-binding probe.

### §1.2 The tree census (subagent recon, RE-VERIFIED on disk)

| claim | measurement | command shape |
|---|---|---|
| `alpn_protocols` is unimplemented | **7 hits** across `crates/`, every one a comment or the rejection test | `grep -rn -i alpn crates/ --include=*.rs` |
| the rejection mechanism | `CommonTlsContext` is a **2-field** struct with `#[serde(deny_unknown_fields)]` and *derived* `Serialize` | `bootstrap.rs:1184-1191` |
| the rejection is pinned | test `rejects_unknown_field_in_common_tls_context` | `bootstrap.rs:8853` |
| **the E0063 blast** | **5** construction sites workspace-wide (`bootstrap.rs` ×4 incl. decl, `envoy-config/src/lib.rs` ×1, `envoy-tcp/src/lib.rs` ×1, `envoy-tls/src/tests.rs` ×3; literal count 5) | `grep -rn 'CommonTlsContext {' crates/ --include=*.rs` |
| `CommonTlsContext` is SHARED | it is the type of `DownstreamTlsContext.common_tls_context` (`bootstrap.rs:1171`) **and** `UpstreamTlsContext.common_tls_context` (`:1177`) | — |
| the TLS crate surface | `crates/envoy-tls/src/lib.rs`, 380 lines: `DownstreamTls::{from_context,from_listener,accept}`, `UpstreamTls::{from_context,connect}` | — |
| the harness handshake seam | `drive_tls` already calls `tls.get_ref().1.peer_certificates()` for `expected_cn` | `tests/differential/src/lib.rs:1911` |
| QUIC-TLS is already compiled in | `Cargo.lock` pins **`rustls 0.23.39`**, whose `src/lib.rs:690` declares `pub mod quic;` with **no `#[cfg]`** and no `quic` feature in `[features]` | registry source, version resolved from `Cargo.lock` first |
| existing TLS fixtures | `0004-tls-downstream`, `0005-tls-upstream`, `0006-tls-sni` — **all three are `tcp_proxy`, zero HCM** | — |
| harness PKI | `tests/differential/src/tls.rs` generates a CA + leaves via `rcgen`; fixtures reference `{{LEAF_A_CERT_PATH}}` / `{{LEAF_A_KEY_PATH}}` | — |

**The positive-control discipline:** every "zero hits" claim above was paired
with a control on the same file set that returned non-zero. The `alpn` search
returned 7 (not 0), so no control was needed there; the `CommonTlsContext`
literal count was cross-checked against a per-file breakdown.

### §1.3 Corrections this session made to inherited or delegated figures

- **The `next-prompt.txt` handoff's headline candidate was HALF PHANTOM.** It
  nominated "the filter API's DATA/TRAILER hooks" as the gRPC family's second
  prerequisite. Measured: a **DATA hook buys zero capability today**, because
  both HCMs are fully buffered and `FilterRequest.body: Option<Bytes>` /
  `FilterResponse.body: Bytes` are already COMPLETE when `decode_headers` /
  `encode_headers` run — the `buffer` filter reads the whole body inside
  `decode_headers`. Twice ratified independently: phase-07 `SPEC.md:254` and
  `ADR-0062`. Only the TRAILER half is real. See §2.3(a).
- **The handoff's "cross-crate change reaching every one of the twelve landed
  filter modules" is wrong in kind.** `crates/envoy-filter/src/` has **no filter
  trait at all** (`grep -rn 'pub trait'` → **0**, against a 132-hit `pub`
  control); dispatch is a hand-written 14-variant enum + `match`. There is no
  E0046 blast to fear.
- **A recon proposal was DROPPED as fabricated config surface.** One agent
  proposed consuming a new trailer hook via a `header_mutation`
  `response_trailers_mutations` field. Upstream Envoy has no such field, so a
  differential fixture would fail at config-load on the reference side. Not
  carried into `ADR-0183` as a live option.
- **A recon ranking put `runtime_filter` first on a stale premise.** True that
  `ADR-0154` DECISION 7 rejected it because it "needs RTDS" and that premise is
  now false (phase 108 landed the runtime snapshot store, 109 landed a
  consumer). The correction is real and is recorded in `ADR-0183`; the
  candidate still loses on leverage.
- **My own first reading of the access-log candidate was wrong and is corrected
  here.** I initially read the H2 trailer block as live at the access-log record
  build. It is not: `finalize_h2_stream` **moves** `trailers` into
  `send_envoy_response` at `crates/envoy-http2/src/hcm.rs:1096` and builds the
  `AccessLogRecord` at `:1158`. Any `%GRPC_STATUS%`/`%TRAILER()%` phase must
  extract the values **before** the send. Recorded so phase 113 does not
  rediscover it.

---

## §2. Why this surface

### §2.1 It discharges a prerequisite `ADR-0177` named and this session re-derived

`ADR-0177` rejected the HTTP/3 + QUIC family opener on **three independently
measured blockers**. All three were re-derived from today's tree at this
session, and they are **not equal**:

| blocker | re-derived verdict | in scope here? |
|---|---|---|
| **(i)** merged listeners capped at one (`bootstrap.rs:3663`) | **TRUE but NARROWER than stated** — `Bootstrap::all_listeners` chains only static + dynamic listeners and does **not** count `admin:`. It constrains a TCP+UDP two-listener fixture; it does not touch a single TLS listener. | **No.** This phase adds no listener. |
| **(ii)** no transport-socket abstraction | **TRUE and WIDER than stated** — `ConnectionHandler::handle` takes a concrete `tokio::net::TcpStream` (13 impls, 51 `dyn` mentions); `UdpSocket` appears NOWHERE in `crates/` against a 113-file `TcpListener` control. | **No.** This phase touches no accept path. |
| **(iii)** ALPN is not wired, in either direction, and is actively REJECTED | **TRUE, verbatim** — and it is the only blocker that is simultaneously fully true, wholly independent of QUIC, and witnessable on today's TCP-only harness. | **YES — this is the phase.** |

`ADR-0181` added a fourth blocker that also re-derives TRUE: the `h3` framing
crate is **not** on D-3.2's permitted-foundations list (only `quinn` is), so a
real H3 slice must hand-write HTTP/3 framing *and* QPACK. A fifth is recorded
for the first time in `ADR-0183`: the differential harness maps upstream
container ports **TCP-only** (`tests/differential/src/upstream.rs:250/:256/
:336/:342`), though `testcontainers 0.23.3` does expose a `.udp()` form.

**`ADR-0177` and `ADR-0181` are NOT superseded. Their rejections STAND for every
slice that binds a UDP socket or speaks HTTP/3.** This phase narrows their
scope; it does not overturn them.

**On the filing, stated as an objection rather than glossed.** ALPN's
nearest-term consumer in this codebase is HTTP/2-over-TLS, not HTTP/3, and
`ROADMAP.md` has no HTTP/2 or TLS family heading to file it under — phases 03.x
and 05.x sit in the pre-heading MVP trunk. Two things justify filing it under
`### HTTP/3 + QUIC family` anyway: QUIC **mandates** ALPN (`h3` is an ALPN
identifier), so ALPN is a hard prerequisite of that family and merely an
*enabler* of H2-over-TLS; and `ADR-0177` itself enumerated ALPN as one of the
family's three blockers, so the family already claims it. The stop-condition
leg-(iii) movement is a **consequence** of the filing, not its motivation — the
phase's engineering value is identical under either filing.

### §2.2 The six properties, each measured

1. **Zero new dependencies.** No `Cargo.toml` changes. Contrast the WASM opener,
   which needs a MISSION.md-amending ADR before its first line of code
   (`grep -n -w 'wasmtime\|wasmer\|wasmi' docs/envoy-rust/MISSION.md` → **0**,
   against a 2-hit `quinn` control).
2. **A five-site config blast, not forty-two.** §1.2. Phase 111 measured 42 for
   the analogous move and deliberately routed around it (`ADR-0182`); here the
   field can simply be added, and `#[serde(default)]` keeps every existing
   fixture parsing unchanged.
3. **No new harness driver, backend, or container capability.** The witness is
   an `expected_alpn` field on the existing `Driver::TlsTcp` plus one accessor
   call on a value `drive_tls` already holds.
4. **The divergence is boot-fatal, so the fixture cannot pass vacuously.** §1.1
   F1. Compare `ADR-0123`'s recorded hazard, where a `ByteExact` body rule
   passed vacuously because both sides returned zero bytes.
5. **Deterministic.** The negotiated protocol is a pure function of two ordered
   lists. No PRNG, no timing, no contract relaxation — unlike `RANDOM` /
   `LEAST_REQUEST` (which additionally need `rand`, not on D-3.2's list) or
   `admission_control` (probabilistic).
6. **It moves TWO stop-condition legs.** Leg (ii) — an in-scope leaf is built.
   Leg (iii) — zero-row families go from **two** to **one**. Only a row under
   `### HTTP/3 + QUIC family` or `### WASM host family` can move leg (iii), and
   §2.3(e) measures those two as asymmetric.

### §2.3 What was rejected, and the measurement that killed each

Full detail is in `ADR-0183`. Summarised, with the measurement:

- **(a) The filter pipeline's data/trailer hooks (CF-111-1).** The data half is
  phantom (§1.3). The trailer half is real but carries two blockers no
  carry-forward names: `Decision` has exactly **two** variants and
  `StopAndSend(FilterResponse)` is semantically undefined on a trailer hook
  (headers and body are already on the wire); and **`crates/envoy-filter` has
  ZERO production `async`** — `grep -rn 'async fn\|await'` returns 2 hits, both
  under `#[cfg(test)]`, and tokio is a `[dev-dependencies]` entry only, which
  independently blocks 6 of the 21 absent HTTP filters. Estimated 1200–1500,
  split PROJECTED LIKELY, and no cheap *real* consumer. **CF-111-1 stays
  unconsumed.**
- **(b) `%GRPC_STATUS%` / `%TRAILER(name)%` access-log tokens (CF-111-4).** The
  strongest runner-up. **Also measured on both proxies this session:** upstream
  accepts all three tokens (`--mode validate` → `configuration OK`) and rejects
  a bogus `%NOT_A_REAL_TOKEN_XYZ%` with `Not supported field in StreamInfo`
  (so the probe is not vacuous); envoy-rust is boot-fatal on each
  (`unknown access-log operator keyword 'GRPC_STATUS'` / `'TRAILER'`), and
  `grep -rn 'GRPC_STATUS' crates/envoy-accesslog/src/` → **0** against a 42-hit
  `RESPONSE_CODE` control. Estimated 740–1110. **Rejected only on leverage:** it
  advances leg (ii) alone. **This is the strongest candidate for phase 113**, and
  §1.3 records the one obstacle it must carry.
- **(c) H2 REQUEST trailers (CF-111-3).** Symmetric seams already identified;
  ≈870–1140 reusing `TrailerBlock`, `build_trailer_map` and the phase-111
  harness. Rejected on leverage — a second helping of the phase-111 surface,
  leg (ii) only.
- **(d) File-based RTDS.** The in-source claim *"`rtds_layer` needs an xDS
  cluster"* (`bootstrap.rs:1042`) is measurably too strong: upstream's
  `rtds_layer` is `{name, rtds_config: ConfigSource}` over the **same**
  `ConfigSource` whose `path_config_source` arm phases 18–21/26/27 landed four
  times, and `crates/envoy-cluster/src/xds_watch.rs` is explicitly DOMAIN-FREE
  and reusable verbatim. ≈1200. Rejected on leverage; the correction is banked
  in `ADR-0183` so it is not re-inherited.
- **(e) The WASM host opener — the ONLY other leg-(iii) mover.** Rejected on a
  **governance** measurement, not on heaviness: no WASM engine is on D-3.2's
  permitted list, so D-3.5 requires a landed MISSION.md-amending ADR first;
  proxy-wasm conformance needs `proxy_on_request_body`/`proxy_on_response_body`,
  re-engaging (a); and D-3.3 would require both proxies to run the **same**
  `.wasm` binary, which no existing driver can mount. ≈2380 plan / ≈3950
  realistic, **mandatory 3-way split**. This asymmetry is the whole argument for
  taking the HTTP/3 prerequisite now.
- **(f) The drafted repo-health phase.** Every ratio in it re-measured stale
  (traps line **208047** chars not 125473; `STATE_HISTORY.md` **16661** lines;
  **52** anchored blocks not 30; naive-vs-anchored false-positive gap now **9**
  not 1). By its own §4 its differential surface is *None* and §7.5 gates
  (a)/(c)/(d) are vacuous by construction. Rejected for this phase; its
  underlying problem is real and growing at a measured **+4280 characters per §5
  state**, so it should be picked deliberately rather than deferred forever.
- **(g) Also weighed:** `runtime_filter` (≈800, stale rejection premise —
  §1.3); `custom_response` (≈800–1040, one of only three of 21 absent HTTP
  filters implementable on today's headers-only pipeline); CSRF
  `filter_enabled.runtime_key` (≈575, would flip a reject-direction divergence
  into parity) and `RuntimeUInt32` (≈300, but requires deleting or inverting
  `runtime_key_is_rtds_inert`, a LIVE pin on the do-not-touch list);
  `sni_cluster` (drags in a non-existent `tls_inspector` subsystem); priority LB
  (≈1200 plan / ≈2000 realistic); H1 response trailers CF-111-2 (blocker
  CONFIRMED — response-side chunked encoding is absent on all four legs and
  adding it is dominated by a **153-site** `Response {}` E0063 sweep for a
  framing discriminator that, unlike phase-111's trailers, cannot ride
  alongside the struct).

---

## §3. Scope — what this phase builds (design decisions D1–D8)

**D1 — the config surface is one field on one shared struct.**
`CommonTlsContext` gains `alpn_protocols: Vec<String>` with `#[serde(default)]`,
so every existing fixture and test parses unchanged. Because
`CommonTlsContext` is the type of **both** `DownstreamTlsContext` and
`UpstreamTlsContext` (`bootstrap.rs:1171` / `:1177`), the field appears on both
— which forces D2.

**D2 — BOTH sides are honored; neither parses-then-silently-ignores.** The
project's standing posture (`ADR-0049` all-fatal; `ADR-0176`'s explicit
"no landed state ever parses-then-silently-ignores") forbids landing the field
on `UpstreamTlsContext` while honoring it only downstream. The two options were
(a) honor both, or (b) honor downstream and add a targeted boot-fatal rejection
for a present upstream `alpn_protocols`. **(a) is chosen** — it is the smaller
diff (`rustls::ClientConfig` takes the same `Vec<Vec<u8>>`), it matches upstream
Envoy, and (b) would land a rejection this project would immediately want to
remove.

**D3 — the empty vector means "do not advertise ALPN".** Measured: removing the
`alpn_protocols` line entirely yields `No ALPN negotiated` on every offer
(§1.1 F4). `#[serde(default)]` therefore produces exactly the pre-phase
behaviour for every existing config, and the fixture's control cells assert it.

**D4 — an element must be a non-empty string.** ALPN protocol identifiers are
length-prefixed byte strings of 1..=255 bytes (RFC 7301 §3.1); a zero-length
element is unrepresentable on the wire. envoy-rust rejects it at config-load
with a new `ConfigError` variant. **Whether upstream Envoy rejects it, and with
what, is PV-3** — if upstream accepts it, D4 becomes a documented divergence
rather than parity, and the fixture must not probe it.

**D5 — selection order follows the SERVER's list.** `rustls`'
`ServerConfig::alpn_protocols` is documented to select by server preference,
and §1.1 F3 is consistent with that but does not discriminate it. **PV-1 runs
the discriminating probe** (server list reversed against the same client offer)
before the PLAN locks this cell.

**D6 — a mismatch is not an error.** Per §1.1 F2, upstream Envoy completes the
handshake with no protocol selected rather than sending
`no_application_protocol`. envoy-rust must match. **PV-2 measures whether
`rustls` does this by default**; if it sends the alert, this decision becomes
the phase's one piece of real engineering rather than a pass-through.

**D7 — the witness is an `expected_alpn` rule on the existing `Driver::TlsTcp`.**
Shape copied verbatim from phase 111's `expected_trailers` on `Driver::Http2`:
`#[serde(default)] expected_alpn: Option<AlpnRule>`, because the variant carries
`deny_unknown_fields` and the field must exist in Rust before any fixture YAML
may name it. `drive_tls` widens its return to carry the negotiated protocol,
read from `tls.get_ref().1.alpn_protocol()` — the same accessor it already uses
for `peer_certificates()`. Two call sites.

**D8 — the fixture is `0091-tls-alpn`, a `tcp_proxy` TLS listener.** It reuses
the `0004-tls-downstream` shape and the harness's `rcgen` PKI
(`{{LEAF_A_CERT_PATH}}` / `{{LEAF_A_KEY_PATH}}`) unchanged. **`tcp_proxy`, not
HCM** — all three existing TLS fixtures are `tcp_proxy`, and an HCM listener
would collide with `ConfigError::Http2OverTlsNotSupported` the moment `h2` is
negotiated, which is exactly the interaction this phase declines (CF-112-1).

---

## §4. Differential surface at phase end

New fixture `tests/fixtures/0091-tls-alpn/`, driven by `Driver::TlsTcp`, with
`envoy.yaml` and `envoy-rust.yaml` intended **byte-identical** (a divergence
would need its own ADR per §7.1). Cells:

| # | client offers | server lists | expected | source |
|---|---|---|---|---|
| 1 | `h2,http/1.1` | `h2,http/1.1` | `h2` | MEASURED §1.1 |
| 2 | `http/1.1` | `h2,http/1.1` | `http/1.1` | MEASURED §1.1 |
| 3 | `h3` | `h2,http/1.1` | none negotiated, handshake OK | MEASURED §1.1 |
| 4 | nothing | `h2,http/1.1` | none negotiated | MEASURED §1.1 |
| 5 | `h2,http/1.1` | `http/1.1,h2` | **PV-1 decides** | not yet measured |
| 6 | `h2,http/1.1` | *(field absent)* | none negotiated | MEASURED §1.1 F4 |

Cells 1–4 and 6 are measured today. **Cell 5 must not enter the fixture until
PV-1 has run.** Cell 6 is the control that makes the fixture a witness rather
than a tautology — it is the negative control of §1.1 F4 promoted into the
fixture, and it requires a second filter chain or a second listener, which is
the one shape question the PLAN must settle (see PV-5).

`BEHAVIOR_CONTRACT.md` gains an **ALPN** section recording the six cells, the
mismatch disposition (D6), the selection-order rule (D5) and every cell left
unmeasured (§8).

---

## §5. Non-goals — do NOT widen into these

Each is rejected fail-loud, not silently dropped (§6.3):

1. **Anything UDP, QUIC, `quinn`, HTTP/3 framing, QPACK or `h3spec`.** This
   phase adds no listener and no transport.
2. **Lifting `ConfigError::Http2OverTlsNotSupported` (`bootstrap.rs:4267`).**
   The reader will most expect this one. ALPN is that rejection's prerequisite
   and this phase makes lifting it *possible*, but the lift is an H2-over-TLS
   integration with its own wire surface and its own fixture. **CF-112-1.**
3. **`udp_listener_config`, `SocketAddress.protocol`, `http3_protocol_options`.**
   All absent today (`grep` → 0 against a 378-hit `socket_address` control) and
   all stay absent, rejected by `deny_unknown_fields`.
4. **`CodecType::HTTP3`.** It already *parses* and is already rejected in
   `validate_hcm` (`bootstrap.rs:4256-4262`). That rejection stays.
5. **ALPN-driven filter-chain matching** (`FilterChainMatch.application_protocols`).
   A separate matcher surface; not touched.
6. **Any change to `ConnectionHandler`, `Listener`, or the accept path.**
7. **Fixing any carry-forward.** §6.3 and `ADR-0165`: a phase banks, it never
   clears.

---

## §6. Carry-forwards this phase OPENS

- **CF-112-1** — `Http2OverTlsNotSupported` is NOT lifted. envoy-rust can
  advertise `h2` over TLS but still cannot serve HTTP/2 over TLS. The phase's
  most conspicuous deliberate gap, and the natural phase 113/114.
- **CF-112-2** — the **upstream** ALPN offer is honored (D2) and unit-tested,
  but is **not differentially witnessed**: no existing driver can report what a
  backend negotiated.
- **CF-112-3** — ALPN interaction with SNI-selected filter chains
  (fixture `0006-tls-sni`) is UNMEASURED.
- **CF-112-4** — whether upstream Envoy's ALPN list is per-filter-chain or
  per-listener when several chains disagree is UNMEASURED.
- **CF-112-5** — `rustls`' `ServerConfig` disposition on an ALPN mismatch is
  UNMEASURED (PV-2). If it sends `no_application_protocol` where Envoy does
  not, D6 becomes real work.

---

## §7. PLAN-VERIFY items — re-confirm FRESH at the state-2 PLAN-write

Each must be measured against the pinned image before the PLAN locks the
corresponding cell. **A PLAN's own code is a claim: run it.**

- **PV-1 (decides fixture cell 5)** — reverse the server list to
  `["http/1.1","h2"]`, offer `h2,http/1.1`, and record which is selected. This
  discriminates server-preference (D5) from client-preference. §1.1 F3 does
  **not** answer it.
- **PV-2 (decides D6, the highest-risk unknown)** — does `rustls`'
  `ServerConfig` complete the handshake with no protocol on an ALPN mismatch,
  or does it send `no_application_protocol`? Read the pinned `rustls 0.23.39`
  source (resolve the version from `Cargo.lock` first — this host's registry
  cache holds several versions of some crates) and confirm empirically.
- **PV-3 (decides D4)** — does upstream Envoy reject a zero-length
  `alpn_protocols` element? And a duplicate element? And an over-255-byte one?
  If it accepts where envoy-rust rejects, D4 is a divergence to document, not
  parity to assert.
- **PV-4** — does upstream Envoy accept `alpn_protocols` on an
  **`UpstreamTlsContext`** (not just downstream), and does it offer them on the
  upstream handshake? D2 assumes yes.
- **PV-5 (decides the fixture's shape)** — cell 6 needs a server with **no**
  `alpn_protocols`. Settle whether that is a second filter chain on the same
  listener, a second listener (**check the merged-listener cap of one at
  `bootstrap.rs:3663` first — it may forbid this**), or a second fixture. This
  is the one structural question the PLAN must answer, and the listener cap
  makes the obvious answer possibly illegal.
- **PV-6** — dry-run the exact fixture YAML against **BOTH** proxies before the
  PLAN locks it. CF-110-6/7 hid behind the fact that 0 of 40 `direct_response`
  fixtures used an empty body; a config shape that has never been run on both
  sides is an untested claim.
- **PV-7** — re-derive the E0063 site count (§1.2 says 5) at the PLAN-write
  commit, not at this one.
- **PV-8** — confirm `Driver::TlsTcp`'s `deny_unknown_fields` posture and that
  adding `#[serde(default)] expected_alpn` leaves all pre-112 fixtures parsing.

---

## §8. NOT MEASURED — stated explicitly per D-3.4

- Server-preference vs client-preference selection order (PV-1).
- `rustls`' mismatch disposition (PV-2, CF-112-5).
- Validation of malformed `alpn_protocols` elements on either side (PV-3).
- Upstream-side ALPN behaviour end-to-end (PV-4, CF-112-2).
- ALPN × SNI filter-chain selection (CF-112-3, CF-112-4).
- Whether Envoy emits any ALPN-related stat. The probe read
  `listener.0.0.0.0_10000.ssl.handshake: 0` even though four handshakes
  completed, which is **unexplained** — it may be that the stat is scoped
  differently, or that the `tcp_proxy` upstream failure (the probe pointed at
  `127.0.0.1:1`) tore the connection down before the stat ticked. **Do not
  assert any `ssl.*` stat in the fixture without measuring it first.**
- ALPN over the io_uring H1 path (`crates/envoy-http1/src/uring.rs`).

---

## §9. Size estimate and the §6.1 split gate

Bottom-up, per file. The estimate is deliberately given **before** the overrun
factor, then after it, because this project's SPEC-stage projections have
undershot three times running.

| file | work | net LoC |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | the field + doc; the D4 validator; invert `rejects_unknown_field_in_common_tls_context` into an accept test; new negative tests | 150 |
| `crates/envoy-config/src/lib.rs` | new `ConfigError` variant + doc | 20 |
| `crates/envoy-tls/src/lib.rs` | thread into `ServerConfig` (2 sites) + `ClientConfig` (1 site) + doc | 55 |
| `crates/envoy-tls/src/tests.rs` | 3 literal fixups + 4 unit tests (negotiated / not-offered / mismatch / absent) | 130 |
| `crates/envoy-tcp/src/lib.rs` | 1 literal fixup | 3 |
| `tests/differential/src/lib.rs` | `expected_alpn` on `Driver::TlsTcp`; `drive_tls` return widening (2 call sites); the assertion | 140 |
| `tests/differential/tests/tls_alpn.rs` | runner (mirrors `h2_response_trailers.rs`, 43 lines) | 45 |
| `tests/fixtures/0091-tls-alpn/` | `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md` | 250 |
| fuzz seed for the existing `parse_bootstrap` target | no NEW target — §7.4 needs no `ci.yml` edit | 2 |
| **code subtotal (`crates/` + `tests/`)** | | **≈573** |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | new ALPN section | 110 |
| **total incl. docs** | | **≈685** |

**Calibration, MEASURED at this session** with
`git diff --numstat <state-2> <state-3>` excluding `docs/`:

| phase | PLAN estimate | actual net LoC | factor |
|---|---|---|---|
| `110.2` | ≈615 | **817** | 1.33× |
| `110.1` | ≈912 | **1290** | 1.41× |
| `111` | ≈916 | **1525** | 1.66× |

Mean **1.47×**, worst **1.66×**. Applied to the ≈573 code subtotal:
**≈840 central, ≈950 at the worst observed factor.**

**§6.1 verdict: the split gate is PROJECTED NOT TO FIRE.** ~9–10 TDD tasks
against a ~25 ceiling; ≈840–950 against a ~1500 ceiling. It clears even at
**2.6×**, a factor this project has never recorded. **`ADR-0184` is
RESERVED-UNFIRED** for a split, per the `ADR-0176`/`0178`/`0182` discipline.

⚠ **The gate is the state-2 session's to adjudicate, on its own re-derived
bottom-up estimate — not on this one.** Phase 111 cleared the gate on a ≈916
SPEC-stage estimate and landed at 1525, which would have failed it. The
estimate above is a signal, not a finding.

---

## §10. Definition of done — the §7.5 gate, instantiated

- **(a)** fixture `0091-tls-alpn` green cross-proxy on every cell §4 admits,
  and **mutation-proved** — deleting the `alpn_protocols` threading must turn
  it RED, and the mutation target must be asserted to occur exactly once first.
- **(b)** all **90** pre-existing differential fixtures still green. The
  `#[serde(default)]` on both new fields (config and driver) is what makes this
  a design property rather than a hope.
- **(c)** h2spec at `PASS_RATE_GATE = 0.95` with `known-failures.txt`
  **untrimmed** (21 lines, md5 `19cd44d86a8b15d825f76c6e7b265e65`). ⚠ The local
  run self-skips silently — a local green needs `--nocapture`; CI is
  authoritative (`ADR-0163`).
- **(d)** no NEW fuzz target is required (the existing `parse_bootstrap` target
  covers the new config field), so **no `ci.yml` edit**. A new corpus seed needs
  an explicit `!`-un-ignore line — verify with `git ls-files`.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`,
  `cargo test --workspace`, `cargo deny check` all clean locally **and** in CI,
  quoted into `PROGRESS.md` at state 4.
- **(f)** `REVIEW.md` approved at state 5.

**The CI identity is `binaries=167 passed=2252 failed=0`** and has been
byte-identical since the phase-111 state-3 code push. A docs-only commit must
**not** move it; the state-3 code commit **must**.

---

## §11. Next state

**§5 state 2 — the `PLAN.md` write — is a SEPARATE session** (§5.1;
`ADR-0127`). It runs `superpowers:writing-plans`, works PV-1…PV-8 first, and
owns the §6.1 gate on its own re-derived estimate.

This session wrote `SPEC.md` and nothing else in this directory. It landed no
code, ran no `cargo` command, re-adjudicated no gate, and **fixed nothing**.
