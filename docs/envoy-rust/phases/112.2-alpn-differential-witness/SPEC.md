# Phase 112.2 — the ALPN differential witness + the contract section

> **Status:** created by the phase-112 §5 state-2 §6.1 SPLIT. `SPEC.md` is the
> only artifact in this directory. `PLAN.md` is §5 state 2 for THIS sub-phase
> and belongs to a SEPARATE session (§5.1; `ADR-0127`).
>
> **Split ADR:** `ADR-0184` (`docs/envoy-rust/DECISIONS.md`).
> **Parent SPEC:** `docs/envoy-rust/phases/112-tls-alpn-negotiation/SPEC.md`
> (548 lines, LANDED AND UNEDITABLE).
> **ROADMAP row:** `112.2`, `status: planned`, `depends-on: 112.1`.
> **Sibling (must land FIRST):** `112.1-alpn-config-and-rustls-wiring`.

---

## §0. How to read this document

The parent phase `112` opens the **HTTP/3 + QUIC family** by discharging its
first blocking prerequisite — TLS **ALPN** negotiation. It does **not** build
HTTP/3.

At the parent's §5 state-2 PLAN-write the §6.1 split gate **FIRED**.
`ADR-0184` records the arithmetic. Sibling `112.1` is the implementation half:
it lands `CommonTlsContext.alpn_protocols`, honors it on both the
`rustls::ServerConfig` and `rustls::ClientConfig` sides, and matches upstream
Envoy's mismatch disposition. **This sub-phase is the differential witness
half**, plus the `BEHAVIOR_CONTRACT.md` section and the parent-112 close.

**`112.1` must land before this sub-phase starts.** Every fixture here
configures `alpn_protocols`, and until `112.1` lands, envoy-rust is boot-fatal
on that field. The dependency is recorded in the ROADMAP `depends-on` column.

Every figure below was measured at HEAD `2a9712b3c1e1c9f32a0b27a295f2bdec0bb16b52`
unless it names another commit.

---

## §1. What this sub-phase builds

| # | deliverable |
|---|---|
| 1 | A client-side ALPN **offer** and an expected-ALPN **assertion** on the harness's existing TLS drivers |
| 2 | NEW fixture `tests/fixtures/0091-tls-alpn/` — cells 1–4, four probes against one listener |
| 3 | NEW fixture `tests/fixtures/0092-tls-alpn-server-preference/` — cell 5, the D5 selection-order witness |
| 4 | Cell 6 (the no-ALPN control) added to the EXISTING fixture `0004-tls-downstream` |
| 5 | `tests/differential/tests/tls_alpn.rs` — the runner for both new fixtures |
| 6 | `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — a new **ALPN** section |
| 7 | The parent-112 close-out (ROADMAP rows `112.1`, `112.2` and parent `112` → `done`) |

---

## §2. The cell table — every value MEASURED, and the two shape constraints

### §2.1 The six cells

Measured at the parent's §5 state-2 PLAN-write against the `ENVOY_TARGET.md`
pin `envoyproxy/envoy:v1.33.0`, digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`
verified on the running container, on loopback-mapped ports asserted free
before each run, with ownership proved by `docker ps --filter id=` plus a
`docker inspect` digest match.

| # | client offers | server lists | upstream Envoy | evidence | fixture |
|---|---|---|---|---|---|
| 1 | `h2,http/1.1` | `h2,http/1.1` | **`h2`** | 45/45 runs | `0091` |
| 2 | `http/1.1` | `h2,http/1.1` | **`http/1.1`** | 45/45 runs | `0091` |
| 3 | `h3` (no intersection) | `h2,http/1.1` | **none**, handshake **SUCCEEDS** | 45/45 runs | `0091` |
| 4 | *(nothing)* | `h2,http/1.1` | **none** | 45/45 runs | `0091` |
| 5 | `h2,http/1.1` | **`http/1.1,h2`** | **`http/1.1`** — the SERVER's first choice | 5/5 runs | `0092` |
| 6 | `h2,http/1.1` | *(field absent)* | **none** | parent SPEC §1.1 F4, exact `19d18` negative control | `0004` |

**Cell 5 is the whole reason `0092` exists.** The parent SPEC recorded that its
state-0 probe "does not discriminate" server-preference from client-preference,
because the selected protocol was first in **both** lists. The discriminating
probe was run at the PLAN-write session: with the server list **reversed** to
`["http/1.1","h2"]` and the client still offering `h2,http/1.1`, upstream Envoy
selected **`http/1.1`**. Selection follows the **server's** order. This also
agrees with `rustls`, whose `ServerConfig::alpn_protocols` is documented *"most
preferred first"* and whose selection loop iterates the server list — so the
cell is expected GREEN, and its value is that it would catch a silent
inversion.

### §2.2 Shape constraint 1 — a second LISTENER is ILLEGAL, so cells 5 and 6 cannot share `0091`

This is the parent's PV-5, and the parent SPEC flagged that "the obvious answer
may be illegal". It is.

`crates/envoy-config/src/bootstrap.rs`, in `validate()`:

```rust
let total_listeners = bootstrap.all_listeners().count();
if total_listeners > 1 {
    return Err(crate::ConfigError::TooManyListeners(total_listeners));
}
```

(at `:3663-3666` at HEAD `2a9712b`; the merged static+dynamic cap of **one**,
pinned by tests at `:7374` and `:19235`). A fixture may therefore carry exactly
one listener.

**A second FILTER CHAIN does not work either.** `DownstreamTls::from_listener`
(`crates/envoy-tls/src/lib.rs`) walks every filter chain but builds **one**
`rustls::ServerConfig` for the whole listener, and ALPN is a `ServerConfig`
property — per-chain ALPN is inexpressible without an architectural change this
phase declines, and upstream's own per-chain-vs-per-listener semantics are
**unmeasured** (**CF-112-4**). Building a fixture on unmeasured upstream
semantics would make a RED unattributable.

**Therefore: one server ALPN list per fixture.** Cells 1–4 share the list
`["h2","http/1.1"]` and live in `0091`. Cell 5 needs `["http/1.1","h2"]` and
gets `0092`. Cell 6 needs the field **absent** — and that is exactly what
`0004-tls-downstream` already is.

### §2.3 Shape constraint 2 — cell 6 rides on `0004-tls-downstream`, it does NOT get a third fixture

`tests/fixtures/0004-tls-downstream/` is already a TLS `tcp_proxy` listener
with **no** `alpn_protocols`, driven by `Driver::TlsTcp { sni: a.example.com }`.
Cell 6 is "client offers `h2,http/1.1`, server has no list, nothing is
negotiated" — which is `0004` plus a client offer and an assertion. That is a
three-line change to its `expectations.yaml` instead of a ~190-line third
fixture.

**Editing a landed fixture is precedented on this exact file:**
`git log --follow -- tests/fixtures/0004-tls-downstream/expectations.yaml`
shows `4e8956f phase 06.1: differential harness extensions` modifying it long
after phase 03.1 created it. Landed *phase artifacts* (`SPEC.md`, `PLAN.md`,
`PROGRESS.md`, `REVIEW.md`) are uneditable; fixtures are living test data.

⚠ **The `0004` edit changes the client's behaviour on a green fixture** — it
starts offering ALPN where it previously offered none. Upstream Envoy's cell-6
answer (`No ALPN negotiated`, handshake succeeds) is measured, and `112.1`'s
D6′.1 keeps a no-ALPN listener on the unchanged `TlsAcceptor` path, so the
change is expected inert. The PLAN must nonetheless treat `0004` as a
**changed** fixture under §7.5(a), not as a pre-existing one under (b).

### §2.4 Shape constraint 3 — the client's ALPN offer is an INPUT the harness does not have today

The parent SPEC's D7 says *"`drive_tls` widens its return to carry the
negotiated protocol"*. That is necessary but **not sufficient**: cells 1–4 vary
the **client's offer** against one server, so the offer must also become a
harness **input**.

Measured at HEAD `2a9712b`:

- `drive_tls` (`tests/differential/src/lib.rs:1910`) takes
  `(addr, payload, sni, root_store, expected_cn)` and builds its
  `rustls::ClientConfig` inline. It has **two** call sites, both inside
  `run_tls_tcp_arm` (`:4945` and `:4954`).
- `Driver::TlsTcp` (`:84`) carries `{ sni, expected_cn }` and drives exactly
  **one** probe, so four client offers would need four fixtures.
- `Driver::TlsTcpProbeList` (`:98`) + `TlsTcpProbe { sni, expected_cn }`
  (`:733`) already drive a **sequence** of independent TLS handshakes against a
  single listener, via `drive_tls_probes` (`:1985`), which enforces equivalence
  per probe rather than through a final `assert_equivalence`.

`TlsTcpProbeList` is therefore the right existing driver for `0091`: four
probes, one per client offer, against one server list. **No new driver is
introduced** — the parent SPEC's §2.2 property 3 ("no new harness driver,
backend, or container capability") holds, and the parent's D7 is refined rather
than contradicted.

`drive_tls`/`drive_tls_probes` already reach into the completed handshake for
`expected_cn` via `tls.get_ref().1.peer_certificates()`; `.alpn_protocol()` is
the same accessor on the same value.

### §2.5 PV-8 discharged — `#[serde(default)]` keeps every pre-112 fixture parsing

`Driver` carries `#[serde(tag = "kind", rename_all = "snake_case",
deny_unknown_fields)]` at `tests/differential/src/lib.rs:38`, and `TlsTcpProbe`
carries `#[serde(deny_unknown_fields)]` at `:732`. **The fields must therefore
exist in Rust before any fixture YAML may name them** — the same constraint
phase 111 met with `expected_trailers` on `Driver::Http2`. Every new field is
`#[serde(default)]`, so all pre-112 fixtures (`0004`'s `kind: tls_tcp` + `sni:`
included) parse unchanged.

---

## §3. Design decisions

**E1 — the client offer and the expectation are per-probe.** `TlsTcpProbe`
gains `#[serde(default)] client_alpn: Vec<String>` and
`#[serde(default)] expected_alpn: Option<AlpnRule>`; `Driver::TlsTcp` gains the
same two fields so `0004` can express cell 6 without changing driver kind.

**E2 — `AlpnRule` distinguishes "negotiated X" from "negotiated nothing".**
Cells 3, 4 and 6 all assert the *absence* of a protocol, which is a different
claim from "any protocol". A rule that could only express a positive value
would make those three cells unwriteable, and a `None` expectation that meant
"don't check" would make them silently vacuous. The rule must therefore have an
explicit negative arm.

**E3 — the assertion runs on EACH side independently, and equivalence is the
conjunction.** This is `drive_tls_probes`' established discipline: each proxy
must satisfy the per-probe `expected_alpn`, and both satisfying it *is* the
"both proxies negotiate the same protocol for the same offer" property. No
change to `assert_equivalence` is needed.

**E4 — both fixtures stay `tcp_proxy`, zero HCM.** All three existing TLS
fixtures are `tcp_proxy`, and an HCM listener would collide with
`ConfigError::Http2OverTlsNotSupported` (`bootstrap.rs:4267`) the moment `h2`
is negotiated — the interaction the parent phase explicitly declines
(**CF-112-1**).

**E5 — the fixtures reuse the harness's `rcgen` PKI unchanged.**
`tests/differential/src/tls.rs` generates a CA plus leaves at fixture-run time;
fixtures reference `{{LEAF_A_CERT_PATH}}` / `{{LEAF_A_KEY_PATH}}`. `0091` and
`0092` copy `0004-tls-downstream`'s shape.

**E6 — the fixtures MUST point `tcp_proxy` at a reachable backend.** Measured at
the PLAN-write session: with the upstream cluster pointing at an unreachable
address, upstream Envoy's ALPN cells go **non-deterministic** — the connection
teardown races the handshake, and a random cell returns `No ALPN negotiated`
on roughly 1 in 4 sequences. With a reachable sibling backend, 40 consecutive
handshakes were deterministic and `ssl.handshake` equalled
`downstream_cx_total` exactly. **This also explains the `ssl.handshake: 0`
reading the parent SPEC §8 recorded as unexplained.** The differential harness
supplies a real echo backend, so `0091`/`0092` inherit the deterministic
regime — but the PLAN must not "simplify" them into backend-free fixtures.

**E7 — assert no `ssl.*` stat.** The parent SPEC §8 says so and this session
did not measure an ALPN-specific stat. The fixtures assert the negotiated
protocol through the handshake accessor only.

---

## §4. Differential surface at sub-phase end

- **NEW `tests/fixtures/0091-tls-alpn/`** — `Driver::TlsTcpProbeList`, four
  probes (cells 1–4), server list `["h2","http/1.1"]`, `envoy.yaml` and
  `envoy-rust.yaml` intended byte-identical modulo the harness's existing
  `0004`-shaped address/admin differences.
- **NEW `tests/fixtures/0092-tls-alpn-server-preference/`** — one probe
  (cell 5), server list `["http/1.1","h2"]`.
- **CHANGED `tests/fixtures/0004-tls-downstream/expectations.yaml`** — cell 6.
- `BEHAVIOR_CONTRACT.md` gains an **ALPN** section recording all six cells, the
  mismatch disposition, the server-preference rule, the >255-byte validation
  boundary, and every cell left unmeasured.

**The fixtures cannot pass vacuously.** Before `112.1` lands, envoy-rust is
boot-fatal on `alpn_protocols` — measured at the PLAN-write session on the
exact proposed fixture YAML:

```
parsing bootstrap YAML: static_resources.listeners[0].filter_chains[0]
.transport_socket: unknown field `alpn_protocols`,
expected `tls_certificates` or `validation_context` at line 20 column 13
```

exit 1 — while the **identical file minus the three `alpn_protocols` lines**
(`diff` reports exactly `24,26d23`) boots cleanly and binds its listener. The
same YAML validates `configuration OK` on upstream Envoy. That is the parent's
PV-6 discharged on both proxies: the fixture's non-ALPN shape is already legal
for envoy-rust, and only the new field is fatal.

---

## §5. Non-goals

1. **Any change to `crates/`.** The config surface and the `rustls` wiring are
   sibling `112.1`'s entire scope. If this sub-phase needs a crate change, that
   is a signal `112.1` landed incomplete — raise it, do not absorb it.
2. **Anything UDP, QUIC, `quinn`, HTTP/3 framing, QPACK or `h3spec`.**
3. **Lifting `ConfigError::Http2OverTlsNotSupported`** (**CF-112-1**).
4. **A differential witness for the UPSTREAM side** (**CF-112-2**). No existing
   driver can report what a backend negotiated. The PLAN-write session
   demonstrated a shape that would work — an `openssl s_server`-style TLS
   backend that logs the client's offer — but building it is new harness
   capability, outside both the parent SPEC's scope and §6.2's "coherent slice
   **of the original**" rule.
5. **ALPN-driven filter-chain matching**, per-chain ALPN (**CF-112-4**), and
   ALPN × SNI (**CF-112-3**).
6. **A third fixture for cell 6.** §2.3.
7. **Fixing any carry-forward.** §6.3 and `ADR-0165`.

---

## §6. Carry-forwards

Inherited and still open: **CF-112-1**, **CF-112-2**, **CF-112-3**,
**CF-112-4**, and `112.1`'s **CF-112-6** (empty-element runtime behaviour) and
**CF-112-7** (io_uring H1 path). **CF-112-5 was closed by `112.1` §2.1.**

This sub-phase's close-out also closes parent row `112`. Per §6.3 and
`ADR-0165` it **banks** every carry-forward above and clears none.

---

## §7. Size estimate and the §6.1 gate

| file | work | net LoC | anchor |
|---|---|---|---|
| `tests/differential/src/lib.rs` | `AlpnRule`; `client_alpn` + `expected_alpn` on `TlsTcpProbe` and `Driver::TlsTcp`; `drive_tls` + `drive_tls_probes` offer plumbing and assertion; two dispatch arms | 250 | **measured: phase 111's analogous `expected_trailers` work added `277 0` to this exact file** (`git diff --numstat be1aaf1 111b34a`) |
| `tests/differential/tests/tls_alpn.rs` | runners for `0091` and `0092` | 45 | measured: `h2_response_trailers.rs` is 43; `tls_downstream.rs` is 19 |
| `tests/fixtures/0091-tls-alpn/` | `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml` (4 probes), `README.md`, `inputs/payload.bin` | 190 | **measured: phase 111's fixture `0090` totals 198** (70+52+49+27); the three TLS fixtures are 106–158 |
| `tests/fixtures/0092-tls-alpn-server-preference/` | same shape, one probe, shorter README | 160 | as above |
| `tests/fixtures/0004-tls-downstream/expectations.yaml` | cell 6 | 4 | trivial |
| **code subtotal** | | **649** | |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | new ALPN section | 130 | the parent SPEC's own figure (110), rounded up for the two extra cells |
| **total incl. docs** | | **779** | |

**Calibration, re-derived at this session** (`git diff --numstat <state-2>
<state-3> -- . ':(exclude)docs/**'`): `110.2` **817** vs 615 (**1.33×**),
`110.1` **1290** vs 912 (**1.41×**), `111` **1525** vs 916 (**1.66×**). Mean
**1.47×**, worst **1.66×**. Applied to the 649 code subtotal: **954 central,
1077 at the worst observed factor.**

**§6.1 verdict for `112.2`: the gate does NOT fire.** ~6 TDD tasks against a
~25 ceiling; 954–1077 against a ~1500 ceiling. ⚠ The gate is nonetheless the
`112.2` state-2 session's to adjudicate on its **own** re-derived estimate.

---

## §8. NOT MEASURED

- envoy-rust's side of every cell. All six cells are measured on **upstream
  Envoy only**; envoy-rust cannot parse the field until `112.1` lands. Cells 1,
  2, 4 and 6 are expected GREEN by construction; cell 5 is expected GREEN
  because `rustls` and Envoy agree on server preference; **cell 3 is the one
  that depends entirely on `112.1`'s D6′** — by default `rustls` sends a fatal
  `no_application_protocol` alert where Envoy does not. If `112.1` shipped D6′
  correctly, cell 3 is green; if it did not, cell 3 is the test that catches it.
- Any ALPN-related stat (E7).
- CF-112-2, CF-112-3, CF-112-4, CF-112-6, CF-112-7.

---

## §9. Definition of done — the §7.5 gate, instantiated

- **(a)** `0091-tls-alpn` and `0092-tls-alpn-server-preference` green
  cross-proxy on every cell §2.1 admits, `0004-tls-downstream` still green with
  its new cell-6 assertion, and **all three mutation-proved**: deleting the
  `alpn_protocols` line from a fixture's `envoy-rust.yaml`, or the
  `expected_alpn` assertion from the driver, must turn the fixture RED. Assert
  the mutation target occurs **exactly once** first — a mutation `sed` that hits
  both the implementation and the test fakes a GREEN and reads as "vacuous
  tests". Run an unmutated control from the same tree, force a rebuild, and
  gate on the `test result` line's existence rather than on the exit code
  (a compile error is not a mutation RED). Use a scratch worktree.
- **(b)** All **89** other pre-existing differential fixtures still green — 90 exist today and `0004` is
  a **changed** fixture and belongs to (a), not here).
- **(c)** h2spec at `PASS_RATE_GATE = 0.95` with `known-failures.txt`
  **untrimmed** (21 lines, md5 `19cd44d86a8b15d825f76c6e7b265e65`). ⚠ Locally
  it self-skips silently — a local green needs `--nocapture`; CI is
  authoritative (`ADR-0163`).
- **(d)** No new fuzz target and no `ci.yml` edit (`112.1` ships the corpus
  seed).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`,
  `cargo test --workspace`, `cargo deny check` clean locally **and** in CI,
  quoted into `PROGRESS.md` at state 4.
- **(f)** `REVIEW.md` approved at state 5.

⚠ **The differential fixtures flake under full-parallel `cargo test` on this
host** and pass in isolation; local RED sets vary run to run and **only
isolation classifies a RED**, never the failure text. Use `--no-fail-fast`,
redirect to a file (never `tail` — it truncates the `failures:` block), extract
failures from the `---- <name> stdout ----` markers, and leave a settle gap
between Docker-spawning isolation runs. CI is authoritative.

**Close-out.** This sub-phase's §5 state-6 flips ROADMAP rows `112.1`, `112.2`
**and parent `112`** to `done` — the status cell only.

---

## §10. Next state

**§5 state 2 for `112.2` — the `PLAN.md` write — is a SEPARATE session**
(§5.1; `ADR-0127`), and it may not begin until `112.1` has closed.

The split session wrote this `SPEC.md`, its sibling, `ADR-0184`, the ROADMAP
rows and the `STATE.md` advance, and **nothing else**. It landed no code and
**fixed nothing** (§6.3; `ADR-0165`).
