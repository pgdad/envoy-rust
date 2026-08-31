# Phase 112.1 — ALPN config surface + `rustls` wiring on both sides

> **Status:** created by the phase-112 §5 state-2 §6.1 SPLIT. `SPEC.md` is the
> only artifact in this directory. `PLAN.md` is §5 state 2 for THIS sub-phase
> and belongs to a SEPARATE session (§5.1; `ADR-0127`).
>
> **Split ADR:** `ADR-0184` (`docs/envoy-rust/DECISIONS.md`).
> **Parent SPEC:** `docs/envoy-rust/phases/112-tls-alpn-negotiation/SPEC.md`
> (548 lines, LANDED AND UNEDITABLE).
> **ROADMAP row:** `112.1`, `status: planned`, under `### HTTP/3 + QUIC family`.
> **Sibling:** `112.2-alpn-differential-witness` (the differential witness).

---

## §0. How to read this document

The parent phase `112` opens the **HTTP/3 + QUIC family** by discharging its
first blocking prerequisite — TLS **ALPN** negotiation. It does **not** build
HTTP/3: no listener, no UDP socket, no `quinn`, no QPACK, no `h3spec`.

At the parent's §5 state-2 PLAN-write the §6.1 split gate **FIRED** on the LoC
leg. `ADR-0184` records the arithmetic. This sub-phase is the **implementation**
half: the config surface, the `rustls` wiring on both the downstream and
upstream sides, and the mismatch disposition. It ships **no new differential
fixture** — the differential witness is sibling `112.2`.

A sub-phase that lands no new fixture is precedented and deliberate:
`ADR-0178` split phase 110 into `110.1` ("witnessed ENTIRELY IN-PROCESS, no
fixture") and `110.2` (the fixture), and `ADR-0176` split phase 109 the same
way. §4 below states which §7.5 gate carries the verification weight here, and
why it is a real gate rather than a vacuous one.

Every figure below was measured at HEAD `2a9712b3c1e1c9f32a0b27a295f2bdec0bb16b52`
unless it names another commit. The code files this sub-phase cites are not
touched by the split commit, so the line citations survive it; they are given
with their anchoring text so they can be relocated if a later commit moves them.

---

## §1. What this sub-phase builds

| # | deliverable | design decision |
|---|---|---|
| 1 | `CommonTlsContext.alpn_protocols: Vec<String>` with `#[serde(default)]` | D1 |
| 2 | The field is honored on the **downstream** side (`rustls::ServerConfig`) | D2a |
| 3 | The field is honored on the **upstream** side (`rustls::ClientConfig`) | D2b |
| 4 | An absent or empty list means "do not advertise ALPN" | D3 |
| 5 | Element validation: reject **only** an element longer than 255 bytes | D4′ |
| 6 | An ALPN **mismatch completes the handshake with no protocol selected**, and sends **no** `no_application_protocol` alert | D6′ |
| 7 | Unit tests for all of the above; a fuzz corpus seed for the existing `parse_bootstrap` target | — |

D4′ and D6′ carry a prime because both **replace** the parent SPEC's D4 and D6
on the strength of measurements taken at the PLAN-write session. §2 records
each measurement and its negative control.

---

## §2. The measurements this slice rests on

All five were taken at the parent's §5 state-2 PLAN-write session, against the
`ENVOY_TARGET.md` pin `envoyproxy/envoy:v1.33.0`, digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`
verified **on the running container** each time. Every Docker probe picked its
ports by bind-then-release, asserted them free with a `/dev/tcp` pre-flight,
bound them as `-p 127.0.0.1:<port>:<port>`, and proved ownership with
`docker ps --filter id=` plus a `docker inspect` digest match — because a
port-binding probe that does not assert **which** server answered is not a
measurement (the parent SPEC §1.1 F5 records a false green of exactly that
shape, answered by a foreign container on this host).

### §2.1 M-1 — `rustls` DOES send the alert Envoy does not. (parent PV-2; the parent's CF-112-5)

This is the finding that turns D6 from a pass-through into the slice's one
piece of real engineering, and the parent SPEC named it in advance as "the
phase's highest-risk unknown".

The pinned `rustls` version was resolved **from `Cargo.lock` first** — it is
**`rustls 0.23.39`** — because this host's registry cache holds several
versions of some crates and `ls -d …/<crate>-* | head -1` picks the wrong one.
In `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rustls-0.23.39/src/server/hs.rs`,
`ServerConnectionData::process_common` reads:

```rust
let our_protocols = &config.alpn_protocols;
if let Some(their_protocols) = &hello.protocols {
    cx.common.alpn_protocol = our_protocols
        .iter()
        .find(|ours| their_protocols.iter().any(|theirs| theirs.as_ref() == ours.as_slice()))
        .map(|bytes| ProtocolName::from(bytes.clone()));
    if let Some(selected_protocol) = &cx.common.alpn_protocol {
        /* … */
    } else if !our_protocols.is_empty() {
        return Err(cx.common.send_fatal_alert(
            AlertDescription::NoApplicationProtocol,
            Error::NoApplicationProtocol,
        ));
    }
}
```

Three consequences, each load-bearing for this sub-phase:

- **The mismatch case diverges by default.** Client offers ALPN, server list is
  non-empty, nothing intersects ⇒ rustls sends a **fatal
  `no_application_protocol` alert** and the handshake FAILS. Upstream Envoy
  completes the handshake with no protocol selected (measured — §2.2 cell 3).
  Left alone, this is a divergence the sub-phase would CREATE.
- **The client-offered-nothing case is parity for free.** When
  `hello.protocols` is `None` the whole block is skipped: no alert, no
  protocol, handshake succeeds. That matches Envoy.
- **Selection is by SERVER preference**, because the `find` iterates
  `our_protocols`. `ServerConfig::alpn_protocols` documents this in the same
  words: *"Protocol names we support, most preferred first. If empty we don't
  do ALPN at all."* That sentence also ratifies D3.

**The escape hatch exists in the already-pinned dependency tree, with ZERO new
dependencies.** `rustls 0.23.39` exposes `server::Acceptor`, `Accepted`,
`Accepted::client_hello()` and `Accepted::into_connection(Arc<ServerConfig>)`;
`ClientHello::alpn()` (`src/server/server_conn.rs:187`) returns
`Option<impl Iterator<Item = &[u8]>>`. The pinned **`tokio-rustls 0.26.4`**
(resolved from `Cargo.lock`) wraps this as `LazyConfigAcceptor`
(`src/server.rs:70`), `StartHandshake::client_hello()` (`:217`) and
`StartHandshake::into_stream(Arc<ServerConfig>)` (`:221`). D6′ is built on
these; both crates are already direct dependencies of `envoy-tls`.

### §2.2 M-2 — upstream Envoy's downstream cell table, re-measured and now DETERMINISTIC

Server list `alpn_protocols: ["h2", "http/1.1"]` on a TLS `tcp_proxy` listener,
**40 handshakes over 10 rounds**, then re-confirmed 5/5 on the exact fixture
config:

| # | client offers | upstream Envoy v1.33.0 | runs |
|---|---|---|---|
| 1 | `h2,http/1.1` | `ALPN protocol: h2` | 45/45 |
| 2 | `http/1.1` | `ALPN protocol: http/1.1` | 45/45 |
| 3 | `h3` (no intersection) | `No ALPN negotiated`, handshake **SUCCEEDS** | 45/45 |
| 4 | *(client offers nothing)* | `No ALPN negotiated` | 45/45 |

**A flake was found, root-caused, and eliminated — and it explains a cell the
parent SPEC §8 left "unexplained".** The parent's probe pointed `tcp_proxy` at
the unreachable `127.0.0.1:1`. Under that config the cells are
**non-deterministic**: a re-run of the parent's own three-probe sequence
returned `No ALPN negotiated` for a cell that should have negotiated, on 2 of
8 sequences, landing on a *different* cell each time. Envoy tears the
connection down when the upstream connect fails, and the teardown races the
handshake. Pointing `tcp_proxy` at a **reachable sibling container** made all
40 handshakes deterministic **and** made
`listener.<addr>.ssl.handshake` read **40**, exactly equal to
`downstream_cx_total`. The parent SPEC §8 recorded `ssl.handshake: 0` beside
four completed handshakes as unexplained; **it is explained: same teardown
race, same unreachable upstream.** Any probe or fixture on this surface must
use a reachable backend. The differential harness always supplies one, so the
sibling's fixture is not exposed to this.

### §2.3 M-3 — element validation: upstream rejects ONLY the over-long element (parent PV-3)

`--mode validate` against the pinned image. **The probe was proved non-vacuous
first**: the same config with `alpn_protocolz` is rejected with
`no such field: 'alpn_protocolz'` at
`…common_tls_context: message envoy.extensions.transport_sockets.tls.v3.CommonTlsContext`,
so the validator genuinely parses this exact struct.

| config | upstream Envoy | verdict vs the parent's D4 |
|---|---|---|
| `alpn_protocols: ["h2", "http/1.1"]` | exit 0, `configuration OK` | — |
| `alpn_protocols: []` | exit 0, **accepted** | — |
| `alpn_protocols: [""]` | exit 0, **ACCEPTED** | **D4 REFUTED** |
| `alpn_protocols: ["h2", ""]` | exit 0, **ACCEPTED** | **D4 REFUTED** |
| `alpn_protocols: ["h2", "h2"]` (duplicate) | exit 0, **ACCEPTED** | **D4 REFUTED** |
| element of 254 bytes | exit 0, accepted | — |
| element of 255 bytes | exit 0, accepted | — |
| element of **256** bytes | exit 1, **`Invalid ALPN protocol string`** | **D4 upheld, at this boundary only** |

The parent SPEC's D4 asserted "an element must be a non-empty string …
envoy-rust rejects it at config-load", and stated the contingency: *"if upstream
accepts it, D4 becomes a documented divergence rather than parity."* Upstream
**accepts** the empty element and the duplicate. D4′ therefore rejects **only**
`len > 255`, matching upstream exactly. Rejecting where upstream accepts would
manufacture a reject-direction divergence this phase does not need.

**The runtime behaviour of an accepted empty element was also measured**, so
the decision is informed rather than assumed: with a server list of
`["", "h2"]`, upstream Envoy negotiates **nothing at all** — not even `h2`,
which *is* in the list. One empty element poisons the whole list. Replicating
that quirk is out of scope; it is banked as **CF-112-6** (§6).

### §2.4 M-4 — upstream Envoy honors `alpn_protocols` on an `UpstreamTlsContext` (parent PV-4)

Two legs, both with controls.

- **Acceptance.** `--mode validate` on a cluster carrying
  `UpstreamTlsContext.common_tls_context.alpn_protocols: ["h2","http/1.1"]`
  returns `configuration OK` (exit 0). Control: the identical config with
  `alpn_protocolz` on the same struct is rejected, `no such field`, exit 1.
- **It is actually offered on the wire.** An `openssl s_server -alpn h2,http/1.1`
  in a sibling container, fronted by an Envoy whose cluster carried
  `alpn_protocols: ["http/1.1", "h2"]`, logged:

  ```
  ALPN protocols advertised by the client: http/1.1, h2
  ALPN protocols selected: h2
  ```

  Envoy offered **exactly the configured list, in the configured order**, and
  the connection genuinely happened (`cluster.backend.ssl.handshake: 1`,
  `ssl.connection_error: 0`).
- **Negative control, exact.** The identical config **minus the single
  `alpn_protocols` line** (`diff` reports exactly `26d25`) produced **no**
  `ALPN protocols advertised by the client` line at the server at all, while
  the handshake still succeeded.

D2b is therefore parity, not a guess.

### §2.5 M-5 — the E0063 blast is FOUR construction sites, not five (parent PV-7)

Re-derived at HEAD `2a9712b`, which is the correction the parent's PV-7 asked
for:

```
crates/envoy-config/src/bootstrap.rs:1186:pub struct CommonTlsContext {      <- the DECLARATION
crates/envoy-tls/src/tests.rs:135:            common_tls_context: envoy_config::CommonTlsContext {
crates/envoy-tls/src/tests.rs:240:        common_tls_context: envoy_config::CommonTlsContext {
crates/envoy-tls/src/tests.rs:454:            common_tls_context: envoy_config::CommonTlsContext {
crates/envoy-tcp/src/lib.rs:1189:            common_tls_context: envoy_config::CommonTlsContext {
```

The literal `CommonTlsContext {` count is **5**, exactly as the parent SPEC
§1.2 recorded — but one of the five is the struct declaration, which is not a
construction site and produces no `E0063`. **The blast is 4.** (The parent's
parenthetical "`bootstrap.rs` ×4, `envoy-config/src/lib.rs` ×1" describes the
distribution of all **9** `CommonTlsContext` *mentions*, not of the literals;
`envoy-config/src/lib.rs:19` is a re-export line and `bootstrap.rs` carries the
two field declarations at `:1171`/`:1177`, the struct at `:1186`, and a comment
at `:8856`.)

---

## §3. Design decisions

**D1 — one field on one shared struct.** `CommonTlsContext` gains
`alpn_protocols: Vec<String>` with `#[serde(default)]`. The struct carries
`#[serde(deny_unknown_fields)]` and a *derived* `Serialize`
(`bootstrap.rs:1184-1191`), so the field must exist in Rust before any config
may name it, and it will round-trip through `/config_dump` automatically.
`#[serde(default)]` is what keeps every existing fixture and test parsing
unchanged.

**D2 — BOTH sides are honored in THIS sub-phase; the upstream half is NOT
deferred to `112.2`.** `CommonTlsContext` is the type of **both**
`DownstreamTlsContext.common_tls_context` (`bootstrap.rs:1171`) and
`UpstreamTlsContext.common_tls_context` (`:1177`), so adding the field puts it
on both sides in the same commit. Landing it on `UpstreamTlsContext` while
honoring it only downstream would be a parses-then-silently-ignores state,
which `ADR-0049`'s all-fatal posture and `ADR-0176`'s explicit *"no landed
state ever parses-then-silently-ignores"* forbid. **This is the same
cut-line reasoning `ADR-0176` recorded when it moved the `runtime_fraction`
consumer threading into `109.1` rather than `109.2`.** It is the binding
constraint on where the split seam may fall, and §7 of `ADR-0184` argues it.

**D3 — an absent or empty list means "do not advertise ALPN".** `rustls`
documents `alpn_protocols` as *"If empty we don't do ALPN at all"* (§2.1), and
upstream Envoy behaves identically (parent SPEC §1.1 F4: removing the single
line turns both positive cells into `No ALPN negotiated`). `#[serde(default)]`
therefore reproduces exactly the pre-phase behaviour for every existing config.

**D4′ — reject an element ONLY when its length exceeds 255 bytes.** Per §2.3:
upstream accepts the empty element, the duplicate and the empty list, and
rejects only `len > 255`, with `Invalid ALPN protocol string`. envoy-rust adds
one `ConfigError` variant and rejects exactly that case at config-load, so the
reject sets coincide. **This replaces the parent SPEC's D4**, which assumed the
empty element was rejected upstream. RFC 7301 §3.1 makes a zero-length
identifier unrepresentable on the wire, which is why the parent's assumption
was reasonable — but upstream's actual behaviour is the contract (D-3.3), and
§2.3 measured it.

**D5 — selection order follows the SERVER's list.** Measured on BOTH sides.
Upstream Envoy: a server list of `["http/1.1", "h2"]` against a client offering
`h2,http/1.1` selected **`http/1.1`** — the server's first choice, not the
client's — on 5 of 5 rounds. `rustls`: the `find` in §2.1 iterates
`our_protocols`, and the field's own doc comment says *"most preferred first"*.
The two agree, so this sub-phase inherits parity by construction. **The
differential witness for D5 is `112.2`'s fixture `0092`**; this sub-phase
witnesses it in-process only.

**D6′ — a mismatch must complete the handshake with no protocol selected, and
this requires a `LazyConfigAcceptor` accept path.** Per §2.1, plain
`TlsAcceptor` cannot express it: rustls decides ALPN inside `process_common`
from `config.alpn_protocols`, and a `ResolvesServerCert` hook cannot change the
config that is already in force. The shape:

- `DownstreamTls` retains its ALPN-carrying `Arc<ServerConfig>` and, **only
  when the configured list is non-empty**, also builds a second
  `Arc<ServerConfig>` that is byte-for-byte the same except with
  `alpn_protocols` left empty, plus the configured list kept as
  `Vec<Vec<u8>>` for the intersection test.
- `accept()` drives `tokio_rustls::LazyConfigAcceptor`, reads
  `StartHandshake::client_hello().alpn()`, and hands `into_stream` the
  ALPN-carrying config when the client offered nothing or offered something
  that intersects, and the ALPN-free config when the client offered a
  non-empty set that does **not** intersect. rustls then skips the alert
  branch (`our_protocols.is_empty()`), completes the handshake, and selects
  nothing — which is Envoy's measured behaviour.

**D6′.1 — the `LazyConfigAcceptor` path is taken ONLY when a non-empty
`alpn_protocols` is configured.** When the list is empty — which is every
config in the tree today, including fixtures `0004-tls-downstream`,
`0005-tls-upstream` and `0006-tls-sni` — `accept()` keeps its current
`tokio_rustls::TlsAcceptor` code path unchanged. This is deliberate: it
confines the accept-path change to listeners that actually configure ALPN, so
§7.5 gate (b) (all 90 pre-existing fixtures still green) is a design property
rather than a hope. `accept()`'s signature is unchanged in both directions, so
no consumer of `DownstreamTls` is touched.

**D7 — the upstream side is a straight field assignment.**
`UpstreamTls::from_context` sets `ClientConfig::alpn_protocols` from the same
`Vec<String>`. `rustls`' client-side mismatch handling is not this project's to
police: the *server* chooses, and §2.4 measured Envoy offering the list
verbatim and the peer selecting from it.

---

## §4. Differential surface at sub-phase end — and which gate carries the weight

**No new differential fixture ships in `112.1`.** The differential witness is
sibling `112.2` (fixture `0091-tls-alpn` for the mismatch and negotiation
cells, `0092-tls-alpn-server-preference` for D5, and the cell-6 control on
`0004-tls-downstream`). This mirrors `ADR-0178`'s `110.1` and `ADR-0176`'s
`109.1`.

**§7.5 gate (b) — "all pre-existing differential fixtures are still green" —
is the load-bearing gate here, and it is a genuine one, not a vacuous one.**
This sub-phase rewrites `DownstreamTls::accept`, the accept path shared by
every TLS listener in the tree. Fixtures `0004-tls-downstream`,
`0005-tls-upstream` and `0006-tls-sni` exercise it directly, and D6′.1 exists
precisely so that they take the unchanged code path. A regression in the
rewrite lands those three RED. Compare `ADR-0123`'s recorded hazard, where a
`ByteExact` body rule passed vacuously because both sides returned zero bytes:
gate (b) here cannot pass vacuously, because the three fixtures already pass
today and any change in the accept path is directly observable in them.

In-process verification additionally covers every cell §2 measured, including
the D6′ mismatch case (a real client/server handshake pair asserting that the
handshake **succeeds** and `alpn_protocol()` is `None`) — the one cell that
would otherwise be a claim rather than a test.

---

## §5. Non-goals — do NOT widen into these

1. **Anything UDP, QUIC, `quinn`, HTTP/3 framing, QPACK or `h3spec`.** No
   listener, no transport.
2. **Any new differential fixture, and any change to
   `tests/differential/src/lib.rs` or to any file under `tests/fixtures/`.**
   That is sibling `112.2`'s entire scope; touching it here re-merges the
   split.
3. **Lifting `ConfigError::Http2OverTlsNotSupported`** (`bootstrap.rs:4267` —
   `:4266` is the `if matches!` guard, not the error). ALPN is that
   rejection's prerequisite and this sub-phase makes lifting it *possible*, but
   the lift is an H2-over-TLS integration with its own wire surface and its own
   fixture. **CF-112-1.**
4. **`udp_listener_config`, `SocketAddress.protocol`, `http3_protocol_options`,
   `CodecType::HTTP3`.** All absent or already rejected; all stay so.
5. **ALPN-driven filter-chain matching** (`FilterChainMatch.application_protocols`).
   A separate matcher surface.
6. **Any change to `ConnectionHandler`, `Listener`, or the accept loop.**
   `DownstreamTls::accept`'s signature does not change, so none is needed.
7. **Per-filter-chain ALPN.** `DownstreamTls::from_listener` builds ONE
   `ServerConfig` for the whole listener; ALPN is a `ServerConfig` property, so
   two chains disagreeing on `alpn_protocols` is inexpressible today. **CF-112-4**
   already banks the fact that upstream's per-chain-vs-per-listener semantics
   are unmeasured. The sub-phase honors the first TLS chain's list and does not
   pretend to more.
8. **Fixing any carry-forward.** §6.3 and `ADR-0165`: a phase banks, it never
   clears.

---

## §6. Carry-forwards

Inherited from the parent, all still open: **CF-112-1** (`Http2OverTlsNotSupported`
not lifted), **CF-112-2** (the upstream ALPN offer is honored and unit-tested but
not differentially witnessed — no driver can report what a backend negotiated),
**CF-112-3** (ALPN × SNI filter-chain selection unmeasured), **CF-112-4**
(per-chain vs per-listener ALPN unmeasured upstream). **CF-112-5 is CLOSED by
§2.1** — `rustls`' mismatch disposition is now measured, and D6′ answers it.

**NEW, opened by this sub-phase's own measurements:**

- **CF-112-6** — upstream Envoy **accepts** an empty `alpn_protocols` element
  and a duplicate element (§2.3), and at runtime a single empty element causes
  it to negotiate **nothing at all**, even for a protocol that is present later
  in the same list. envoy-rust accepts the same configs (D4′) but its runtime
  behaviour under an empty element is **not specified and not tested** — rustls
  would place a zero-length name in the extension. Unprobed by any fixture.
- **CF-112-7** — the parent SPEC §8 lists ALPN over the io_uring H1 path
  (`crates/envoy-http1/src/uring.rs`) as unmeasured. It stays unmeasured: this
  sub-phase changes only `DownstreamTls::accept`, and whether the io_uring path
  reaches that function is not established here.

---

## §7. Size estimate and the §6.1 gate

Bottom-up, per file, with every line anchored on a **measured** in-tree
comparable rather than on judgement. The parent SPEC's estimate is not
inherited: `ADR-0184` §D2 records that the parent's §9 table sums to **795**
while stating **≈573**, a 222-line arithmetic error.

| file | work | net LoC | anchor |
|---|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | field + doc; the D4′ >255 validator; invert `rejects_unknown_field_in_common_tls_context` (`:8853`) into an accept test; parse tests for present / absent / empty-list / empty-element / duplicate / 255 / 256 | 145 | the parent SPEC's own figure, unchallenged |
| `crates/envoy-config/src/lib.rs` | one `ConfigError` variant + doc | 15 | `TooManyListeners` at `:83` is a one-line variant |
| `crates/envoy-tls/src/lib.rs` | thread into 2 `ServerConfig` sites + 1 `ClientConfig` site (~30); the D6′ dual-config + `LazyConfigAcceptor` rewrite + a `TlsError` variant (~85) | 115 | `accept()` is 11 lines today; the rewrite replaces it |
| `crates/envoy-tls/src/tests.rs` | 3 literal fixups; 6 new tests (negotiated / server-preference / **mismatch-no-alert** / client-offers-nothing / empty-list / upstream `ClientConfig`) | 320 | **measured: the file's 16 tests have a MEDIAN of 65 lines each** (mean 59); these are real-handshake tests with temp PKI |
| `crates/envoy-tcp/src/lib.rs` | 1 literal fixup (`:1189`) | 3 | trivial |
| fuzz corpus seed for the existing `parse_bootstrap` target | no NEW target, so §7.4 needs no `ci.yml` edit; the seed needs an explicit `!`-un-ignore line, verified with `git ls-files` | 3 | — |
| **code subtotal** | | **601** | |

**Calibration, re-derived at this session** with
`git diff --numstat <state-2> <state-3> -- . ':(exclude)docs/**'`:
`110.2` **817** vs 615 (**1.33×**), `110.1` **1290** vs 912 (**1.41×**),
`111` **1525** vs 916 (**1.66×**). Mean **1.47×**, worst **1.66×**.

Applied to 601: **884 central, 998 at the worst observed factor.**

**§6.1 verdict for `112.1`: the gate does NOT fire.** ~7 TDD tasks against a
~25 ceiling; 884–998 against a ~1500 ceiling. It would need a **2.5×** factor
to fire, higher than anything this project has recorded. ⚠ The gate is
nonetheless the `112.1` state-2 session's to adjudicate on its **own**
re-derived estimate — that session must not inherit this table, for exactly the
reason `ADR-0184` fired.

---

## §8. NOT MEASURED — stated explicitly per D-3.4

- Whether Envoy emits any ALPN-specific stat. `ssl.handshake` is now explained
  (§2.2) but no `ssl.alpn*` stat was searched for. **Do not assert any `ssl.*`
  stat without measuring it first.**
- ALPN × SNI filter-chain selection (CF-112-3) and per-chain vs per-listener
  ALPN upstream (CF-112-4).
- envoy-rust's runtime behaviour under an empty `alpn_protocols` element
  (CF-112-6).
- ALPN over the io_uring H1 path (CF-112-7).
- Whether `LazyConfigAcceptor` changes observable timing or the
  `TlsError::Handshake` error text on a genuinely malformed ClientHello. The
  existing test `accept_returns_handshake_error_on_garbage_input`
  (`crates/envoy-tls/src/tests.rs:339`) pins that behaviour and must stay green;
  under D6′.1 it takes the unchanged path.

---

## §9. Definition of done — the §7.5 gate, instantiated

- **(a)** No new differential fixture, so this gate is vacuous by construction.
  It is discharged by sibling `112.2`, and §4 states why that is a deliberate
  split seam rather than a gap.
- **(b)** All **90** pre-existing differential fixtures still green — **the
  load-bearing gate for this sub-phase** (§4), and specifically
  `0004-tls-downstream`, `0005-tls-upstream` and `0006-tls-sni`, which exercise
  the rewritten accept path. `#[serde(default)]` on the new config field plus
  D6′.1's confinement of the rewrite are what make this a design property.
- **(c)** h2spec at `PASS_RATE_GATE = 0.95` with `known-failures.txt`
  **untrimmed** (21 lines, md5 `19cd44d86a8b15d825f76c6e7b265e65`). ⚠ The local
  run self-skips silently — a local green needs `--nocapture`; CI is
  authoritative (`ADR-0163`).
- **(d)** No NEW fuzz target, so **no `ci.yml` edit**. A new corpus seed needs
  an explicit `!`-un-ignore line — verify with `git ls-files`.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`,
  `cargo test --workspace`, `cargo deny check` all clean locally **and** in CI,
  quoted into `PROGRESS.md` at state 4.
- **(f)** `REVIEW.md` approved at state 5.

**The CI identity at the split commit is `binaries=167 passed=2252 failed=0`.**
A docs-only commit must not move it; this sub-phase's state-3 code commit
**must**.

**Mutation proof.** Because no fixture ships here, the mutation obligation
falls on the unit tests: deleting the `alpn_protocols` threading into
`ServerConfig` must turn the negotiation tests RED, and deleting the D6′
config-swap must turn the mismatch test RED. Assert the mutation target occurs
**exactly once** before mutating, force a rebuild (`grep 'Compiling envoy-tls'`),
and run an unmutated control from the same tree — a stale test binary gives a
FALSE PASS, and a compile error is not a mutation RED.

---

## §10. Next state

**§5 state 2 for `112.1` — the `PLAN.md` write — is a SEPARATE session**
(§5.1; `ADR-0127`). It runs `superpowers:writing-plans`, re-derives its own
bottom-up estimate rather than inheriting §7, and owns the §6.1 gate for this
sub-phase.

The split session wrote this `SPEC.md`, its sibling, `ADR-0184`, the ROADMAP
rows and the `STATE.md` advance, and **nothing else**. It landed no code, ran
no `cargo` build for the tree under test beyond the `PV-6` boot probe, and
**fixed nothing** (§6.3; `ADR-0165`).
