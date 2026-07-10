# Phase 67 — `envoy.filters.network.rbac` + the network-filter chain iteration protocol

> **Status:** state-1 brainstorm complete (this document). Next: §5 state-2 PLAN-write
> (`superpowers:writing-plans`).
> **Pick + scope locked by ADR-0128** (`docs/envoy-rust/DECISIONS.md`).
> **ROADMAP row:** `67`.
> **Phase directory:** `docs/envoy-rust/phases/67-network-filter-rbac/`.
> **§6.1 split is projected to FIRE at the state-2 PLAN-write. ADR-0129 is RESERVED for it.**
> See §8.

This document is written for a stranger with zero prior context (doctrine D-3.4). Every
load-bearing claim below was established this session by reading the live tree or by driving the
pinned upstream Envoy image; none is recalled from memory.

---

## §0. State-0 recon — evidence

Two recon tracks ran: a code-read of the live tree at commit `4505016`, and a live-Envoy drive of
the pinned image `envoyproxy/envoy:v1.33.0` (digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, per
`docs/envoy-rust/ENVOY_TARGET.md`, doctrine D-3.7).

### R-0.1 — `envoy.filters.network.rbac` is NON-TERMINAL, and a chain must END in a terminal filter

`--mode validate` against the pinned image:

| Config chain | Result |
|---|---|
| `[rbac, echo]` | **`configuration OK`** — rbac is **non-terminal** |
| `[echo, rbac]` | `Error: terminal filter named envoy.filters.network.echo of type envoy.filters.network.echo must be the last filter in a network filter chain.` |
| `[rbac]` alone | **`Error: non-terminal filter named envoy.filters.network.rbac of type envoy.filters.network.rbac is the last filter in a network filter chain.`** |

The third row is the **bilateral dual** of the terminal rule phase 66 landed, and envoy-rust does
**not** enforce it. Phase 66 added *"a terminal filter must be last"*; upstream Envoy ALSO enforces
*"a non-terminal filter must NOT be last"* — equivalently, **every network filter chain must end in
a terminal filter.** This phase closes that half of the gap. It is the first time envoy-rust has any
non-terminal network filter, so the rule is untestable before now.

### R-0.2 — LIVE-ENVOY: DENY and ALLOW are deterministic and byte-exact

Booted the pinned image with `docker -p` port-mapping (per memory `state0-recon-docker-needs-port-mapping`:
`--network host` does not share the host net namespace here), listener `:10000`, admin `:9901`,
chain `[rbac, echo]`, policy `permissions:[{any:true}] principals:[{any:true}]`. Client connects,
writes `PING-RBAC\n`, reads. Verbatim:

```
===== action: DENY  =====
  bytes=b'' len=0 how=clean_eof
  post_write=writes_ok
    rbac_probe.rbac.allowed: 0
    rbac_probe.rbac.denied: 1
    rbac_probe.rbac.shadow_allowed: 0
    rbac_probe.rbac.shadow_denied: 0

===== action: ALLOW =====
  bytes=b'PING-RBAC\n' len=10 how=open_after_data
  post_write=writes_ok
    rbac_probe.rbac.allowed: 1
    rbac_probe.rbac.denied: 0
    rbac_probe.rbac.shadow_allowed: 0
    rbac_probe.rbac.shadow_denied: 0
```

**Findings.** On DENY, Envoy writes **zero bytes** and closes with a **clean EOF — no RST** (a
post-EOF client write is accepted, exactly as phase 66's `direct_response` drain established for
that filter). The client's already-sent bytes are discarded. On ALLOW, the connection proceeds to
the terminal `echo` filter and the payload round-trips. The decision is taken **once per
connection, at connection establishment**, before any downstream byte is read — which is why no
`onData`-time iteration is required (§2.2).

### R-0.3 — Stat names and the required `stat_prefix`

Counters are `<stat_prefix>.rbac.allowed`, `.denied`, `.shadow_allowed`, `.shadow_denied`
(R-0.2, verbatim from `/stats`). Further `--mode validate` probes:

- **`stat_prefix` is REQUIRED**: omitting it →
  `Proto constraint validation failed (RBACValidationError.StatPrefix: value length must be at least 1 characters)`.
- **`rules` is OPTIONAL**: a `rbac` filter with only `stat_prefix` → `configuration OK`.

### R-0.4 — Which `Principal` / `Permission` arms L4 RBAC accepts (config-time)

`--mode validate`, one arm at a time, in a `[rbac, echo]` chain:

| Arm | As `principals:` | As `permissions:` |
|---|---|---|
| `any: true` | OK | OK |
| `direct_remote_ip: {address_prefix, prefix_len}` | **OK** | n/a |
| `remote_ip: {…}` | **OK** | n/a |
| `source_ip: {…}` | **OK** | n/a |
| `destination_port: <u16>` | n/a | **OK** |
| `destination_ip: {…}` | n/a | **OK** |
| `not_rule` / `not_id`, `and_*`, `or_*` | OK | OK |
| `url_path: {path:{exact:…}}` | **OK** (accepted, but unmatchable at L4) | **OK** (same) |
| `header: {name:":path", …}` | **REJECTED** | **REJECTED** |

The `header` rejection is verbatim: `error initializing configuration: Found header(name: ":path"…`.
So upstream Envoy **rejects `header` matchers in a network RBAC filter at config load**, while
**accepting `url_path`** — an asymmetry that looks accidental but is the observed contract (D-3.3:
the contract is what Envoy does). §6 records envoy-rust's posture on both.

### R-0.5 — envoy-rust already models the RBAC policy tree, but ONLY with HTTP-shaped arms

- `crates/envoy-config/src/lib.rs:30-31` re-exports `Permission`, `PermissionSet`, `Policy`,
  `Principal`, `PrincipalSet`, `RbacConfig` — all landed by the **HTTP** RBAC filter phase.
- `RbacConfig { rules: Rules }`; `Rules { action: Action (default ALLOW), policies: BTreeMap<String, Policy> }`.
- **`Principal` arms today:** `any`, `header`, `and_ids`, `or_ids`, `not_id`, `metadata`, `url_path`.
- **`Permission` arms today:** `any`, `header`, `and_rules`, `or_rules`, `not_rule`, `metadata`, `url_path`.
- **There is NO `direct_remote_ip`, `remote_ip`, `source_ip`, `destination_port`, or `destination_ip`
  arm, and no `CidrRange` type.** The connection-level arms R-0.4 shows L4 RBAC actually needs must
  be added. Reuse is therefore **partial, not free**.
- `ConfigError` already carries `EmptyRbacPolicies`, `EmptyRbacPolicyPermissions`,
  `EmptyRbacPolicyPrincipals`, `EmptyRbacPermissionSet`, `EmptyRbacPrincipalSet`, and an
  `RBAC_TREE_MAX_DEPTH` (16) guard (`lib.rs:459-500`). Those validations are reusable as-is.

### R-0.6 — NAMING HAZARD: `crates/envoy-filter/src/rbac.rs` already exists — it is the **HTTP** RBAC filter

`crates/envoy-filter/src/rbac.rs` (1270 lines, `pub struct RbacFilter` at `:99`) implements
`envoy.filters.http.rbac`, a **different feature that shares the name** — exactly the
`direct_response` conflation hazard phase 66 hit and documented. `crates/envoy-filter/` models
**HTTP filters only**: `HttpFilterInstance` (`instance.rs`) has 20 variants, every one an
`envoy.filters.http.*`, and the crate contains **no `trait`** and **no network-filter abstraction**.
The phase-67 network filter must be a **distinct type in a distinct namespace**, and
`BEHAVIOR_CONTRACT.md`'s `## Network filters` section must gain a do-not-conflate banner for `rbac`
mirroring the one it already carries for `direct_response`.

### R-0.7 — Runtime dispatch is still a hardcoded 4-arm match over `filters.first()`

- `crates/envoy-bin/src/main.rs:218` — `.and_then(|c| c.filters.first())`: only the FIRST filter of
  the FIRST chain is read.
- `main.rs:241` — `match filter.name.as_str()` with exactly four arms: `ECHO_FILTER` (`:242`),
  `DIRECT_RESPONSE_FILTER` (`:252`), `TCP_PROXY_FILTER` (`:278`), `HCM_FILTER` (`:337`).
- `echo::serve` (`echo.rs:20`) and `direct_response::serve` (`direct_response.rs:31`) each own a
  **standalone accept loop**; `tcp_proxy`/HCM go through `envoy_listener::ConnectionHandler`
  (`crates/envoy-listener/src/lib.rs:38`).
- `filters.first()` is safe **today only because** phase 66's terminal validation makes any
  ≥2-filter chain invalid. **A non-terminal filter breaks that interlock**, which is precisely why
  ADR-0123 §2.2 deferred **CF-66-2** (the generic chain iteration protocol) "to the first
  non-terminal network filter." That filter is arriving now.

### R-0.8 — `is_terminal_network_filter` was designed for exactly this drop-in

`crates/envoy-config/src/bootstrap.rs:825` — its doc comment reads: *"written as a per-name
predicate rather than a `chain.filters.len() <= 1` check so that the first NON-terminal network
filter (`sni_cluster`, network `rbac`) drops in without re-litigating the rule."* **`rbac` must NOT
be added to this predicate** — its absence is what makes it non-terminal.

### R-0.9 — The differential harness has NO stats assertion on any raw-TCP driver

`tests/differential/src/lib.rs` `Driver` has 20 variants. `expected_stats` (a
`Vec<KeepAliveExpectedStat>`) exists on **only three**, all HTTP: `Http1AfterSettle`,
`Http1KeepAlive`, `Http2KeepAlive`. The four raw-TCP/TLS variants — `TcpEcho`,
`TcpDirectResponse`, `TlsTcp`, `TlsTcpProbeList` — carry **none**.

**This is load-bearing.** A DENY fixture asserts "both proxies returned zero bytes." Per the
phase-66 `REVIEW.md`, `assert_body_rule`'s `ByteExact` is a bare `if envoy_body != rust_body`, so an
**empty-vs-empty comparison passes vacuously** — a DENY fixture on body alone would stay green even
if envoy-rust never implemented RBAC at all and simply failed to write. **The DENY fixture is only a
real witness if it also asserts `<stat_prefix>.rbac.denied == 1`.** Extending the raw-TCP driver
family with `expected_stats` is therefore an in-scope deliverable, not a nicety.

### R-0.10 — envoy-rust's stats registry

`crates/envoy-stats/src/registry.rs:45` — `pub fn register_counter(&self, name: &str) -> Result<Arc<Counter>, StatsError>`
(and `register_gauge` at `:69`, `snapshot` at `:94`). Counters are name-registered, so
`<stat_prefix>.rbac.allowed` / `.denied` drop in with no new stats machinery.

### R-0.11 — Two phase-66 carry-forwards are DISCHARGED by this recon (both were UNWITNESSED)

The phase-66 `REVIEW.md` opened **M66-5** with two config edges, recording envoy-rust's behavior and
**deliberately declining to assert Envoy's**, because neither had been probed. Both were probed this
session, and the reviewer's intuition on the first was **wrong**:

| Edge | envoy-rust | upstream Envoy (`--mode validate`, this session) | Verdict |
|---|---|---|---|
| Empty network chain `filters: []` | **accepted** | **`configuration OK` — ACCEPTED** | **Parity. NOT a divergence.** |
| `direct_response` with `response: {}` | **rejected** (`missing field inline_string`) | **REJECTED** — `Proto constraint validation failed (ConfigValidationError.Response: … field: "specifier", reason: is required)` | **Parity of class.** Both reject; message text differs (§7.4 permits that). |

**M66-5 is therefore CLOSED by this SPEC**, not carried into phase 67's implementation. See ADR-0128.


### R-0.12 — Numbering

Next free fixture ids are **`0072`** and **`0073`** (`tests/fixtures/` tops out at `0071`).
`DECISIONS.md` heading ledger head is **ADR-0127**; `grep -c '^## ADR-0128'` → `0`, so **ADR-0128**
is free and is claimed by this SPEC. **ADR-0129** is RESERVED for the projected §6.1 split (§8).

### R-0.13 — Error precedence, and the `rules`-omitted filter is INERT (not "allow-all")

Two further `--mode validate` / runtime probes, run this session so the PLAN need not:

**Error precedence.** A chain violating BOTH rules at once — `[echo, rbac]`, where `echo` is a
terminal filter that is not last AND `rbac` is a non-terminal filter that IS last — reports:

```
Error: terminal filter named envoy.filters.network.echo of type envoy.filters.network.echo must be the last filter in a network filter chain.
```

So the **terminal-not-last error wins**. Ordering the two checks as a single in-order scan over the
chain reproduces this naturally: `echo` at index 0 trips the terminal rule before the
chain-termination rule is ever consulted. (This resolves PLAN-VERIFY **V-3**.)

**`rules` omitted → the filter is INERT.** Runtime probe, `[rbac(stat_prefix only, no rules), echo]`:

```
  bytes=b'HELLO\n'  -> ALLOWED (echo round-tripped)
    norules.rbac.allowed: 0
    norules.rbac.denied: 0
    norules.rbac.shadow_allowed: 0
    norules.rbac.shadow_denied: 0
```

The connection is allowed **and NEITHER counter increments** — `allowed` stays `0`, not `1`. The
filter is inert, not "default action ALLOW, counted". A naive implementation that materialises a
default `Rules { action: ALLOW, policies: {} }` and ticks `allowed` would produce a **stat divergence
with no body divergence**, which fixture `0073`'s stats assertion would catch but a body-only fixture
would not. (This resolves PLAN-VERIFY **V-6**.)

---

## §1. Goal

Ship the Network-filters family's **first non-terminal filter**, `envoy.filters.network.rbac`, and
with it the **generic network-filter chain iteration protocol** (carry-forward **CF-66-2**) that a
non-terminal filter makes unavoidable. Close the **second half** of the network-filter validation
gap: upstream Envoy rejects a chain that does not END in a terminal filter (R-0.1), and envoy-rust
does not. Witness DENY and ALLOW **byte-exact and stat-exact** against upstream Envoy via new
fixtures `0072` and `0073`.

---

## §2. Scope

### 2.1 In scope

**(A) Config surface — `crates/envoy-config/`**

1. New const `NETWORK_RBAC_FILTER = "envoy.filters.network.rbac"` in `src/lib.rs` alongside
   `ECHO_FILTER` / `TCP_PROXY_FILTER` / `HCM_FILTER` / `DIRECT_RESPONSE_FILTER` (`lib.rs:45-58`).
2. New `TypedConfig::NetworkRbac(NetworkRbacConfig)` variant keyed on `@type` =
   `type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC`.
3. New `NetworkRbacConfig { stat_prefix: String, rules: Option<Rules> }`, `deny_unknown_fields`.
   `stat_prefix` is **required** and non-empty (R-0.3); `rules` is **optional** (R-0.3).
   **Reuses** the existing `Rules` / `Policy` / `Permission` / `Principal` / `PermissionSet` /
   `PrincipalSet` types (R-0.5) and their existing empty-set + `RBAC_TREE_MAX_DEPTH` validations.
4. `validate()` gains a `NETWORK_RBAC_FILTER` arm requiring a `TypedConfig::NetworkRbac`.
5. **`rbac` is NOT added to `is_terminal_network_filter`** (R-0.8) — its absence IS its
   non-terminality.
6. **New: chain-termination validation.** A new `ConfigError::NetworkFilterChainNotTerminated
   { listener, chain_index, last_filter }`. Every non-empty network filter chain must END in a
   terminal filter (R-0.1). Implemented in the **same immutable pre-pass** phase 66 added before the
   mutating `for filter in &mut chain.filters` loop. **An empty `filters: []` chain stays ACCEPTED**
   — that is upstream parity, established at R-0.11, not an oversight.
7. **New connection-level matcher arms** (R-0.4), with a new `CidrRange { address_prefix: IpAddr,
   prefix_len: u8 }`:
   - `Principal::DirectRemoteIp(CidrRange)`, `Principal::RemoteIp(CidrRange)`,
     `Principal::SourceIp(CidrRange)`.
   - `Permission::DestinationPort(u16)`, `Permission::DestinationIp(CidrRange)`.

**(B) The network-filter chain iteration protocol — CF-66-2**

8. A **connection-establishment-only** iteration protocol. Network RBAC evaluates **once per
   connection, before any downstream byte is read** (R-0.2), so the protocol needs exactly one hook:

   ```rust
   pub enum NetworkFilterStatus { Continue, StopIteration }
   pub trait NetworkFilter: Send + Sync {
       fn on_new_connection(&self, conn: &ConnectionInfo) -> NetworkFilterStatus;
   }
   ```
   `ConnectionInfo` carries the downstream `peer_addr` and `local_addr` — everything R-0.4's arms
   need. **`on_data` is deliberately OUT of scope** (§2.2): no filter in this phase inspects payload.
9. `main.rs` dispatch is refactored from "read `filters.first()`" to "**iterate the chain**: run each
   non-terminal filter's `on_new_connection` per accepted connection; on `StopIteration`, close the
   connection with a clean EOF and stop; when all return `Continue`, hand the connection to the
   terminal filter." Chain-termination validation (item 6) guarantees a terminal filter exists.

**(C) Data plane — `crates/envoy-bin/`**

10. New `crates/envoy-bin/src/network_rbac.rs`: the policy engine over `ConnectionInfo`, plus the
    two counters `<stat_prefix>.rbac.allowed` / `.denied` via `Registry::register_counter` (R-0.10).
11. DENY closes the connection with **zero bytes written and a clean EOF, never an RST** (R-0.2).

**(D) Differential surface**

12. **Extend the raw-TCP driver family with `expected_stats`** (R-0.9) so a DENY fixture is not a
    vacuous empty-vs-empty comparison. This is a hard requirement, not polish.
13. New fixture `tests/fixtures/0072-network-filter-rbac-deny/` — `[rbac(DENY any), echo]`; the probe
    connects, writes, reads to EOF; asserts **zero bytes byte-exact on both sides** AND
    `<stat_prefix>.rbac.denied == 1`, `.allowed == 0`.
14. New fixture `tests/fixtures/0073-network-filter-rbac-allow/` — `[rbac(ALLOW any), echo]`; the
    payload round-trips **byte-exact**; asserts `.allowed == 1`, `.denied == 0`.

**(E) In-process backstops — `crates/envoy-bin/tests/`**

15. Connection-level matcher tests bound to `127.0.0.1` (deterministic peer IP, unlike the
    Docker-bridge case — see §3 V-4): `direct_remote_ip` match/no-match, `destination_port`
    match/no-match, `not_id`/`and_ids`/`or_ids` composition, and default-action behavior when
    `rules` is omitted.
16. Negative config tests: a chain ending in a non-terminal filter is REJECTED
    (`NetworkFilterChainNotTerminated`); `stat_prefix` missing/empty is REJECTED; an empty
    `filters: []` chain is still ACCEPTED (upstream parity, R-0.11).

**(F) Carry-forward consumption**

17. **CONSUMES CF-66-2** (the iteration protocol — this phase's item 8/9 IS it).
18. **CONSUMES M66-3** — `serve()` never reaps completed `JoinSet` tasks and the per-connection read
    is unbounded, shared verbatim by `echo.rs:21-59` and `direct_response.rs:36-74`. This phase
    restructures **both** accept loops for the iteration protocol, so it is the correct and natural
    unit of repair, and repairing both together preserves the "echo is the structural model"
    invariant that the phase-66 review said must not be broken by fixing one alone.
19. **CONSUMES M66-4** — the `direct_response.rs:93-94` doc-precision line ("Bounded by the caller's
    shutdown drain") is rewritten while that file is being restructured anyway.
20. **CLOSES M66-5** — discharged by recon R-0.11, with no code change. Recorded in ADR-0128 and in
    `BEHAVIOR_CONTRACT.md` as measured parity.

**(G) Documentation**

21. `BEHAVIOR_CONTRACT.md` `## Network filters` gains: the `rbac` do-not-conflate banner (R-0.6); the
    DENY/ALLOW semantics + stat names; the **bilateral chain-termination rule**; the empty-chain and
    `response: {}` parity findings (R-0.11); and the `header`-rejected / `url_path`-accepted
    asymmetry (R-0.4) with envoy-rust's posture.
22. Fuzz corpus seed for the new `typed_config` shape (§2.3).

### 2.2 Out of scope (deliberate, with rationale)

- **`shadow_rules` and the `shadow_allowed` / `shadow_denied` counters.** Envoy emits them always
  (R-0.2 shows both at `0`), but a shadow policy never affects the wire. envoy-rust will emit both
  counters as constant `0` so the stat tree matches, and will **reject a `shadow_rules` field**
  loudly. Carried forward as **CF-67-1**.
- **`Action::LOG`.** Audit-only, never enforces; already deferred once (the `Action` enum's doc
  comment at `bootstrap.rs` cites "phase-10 SPEC §4"). Carried forward as **CF-67-2**.
- **`on_data`-time filter iteration** (`Continue` / `StopIteration` mid-stream, buffering, and the
  `injectReadDataToFilterChain` machinery). No filter in this phase reads payload (R-0.2). Adding
  the hook with no filter to exercise it would violate §6.3 ("no incomplete stubs that differential
  tests can't exercise"). Carried forward as **CF-67-3** — the first payload-inspecting network
  filter (`mongo_proxy`, `zookeeper_proxy`, `kafka_broker`) needs it.
- **`metadata`, `url_path`, and `header` matcher arms in the network context.** Envoy *rejects*
  `header` and *accepts* `url_path` at L4 (R-0.4). envoy-rust will **reject all three** in a network
  RBAC config — matching Envoy exactly on `header`, and diverging on `url_path`/`metadata` in the
  fail-loud direction (ADR-0049 decision-2 (b)), where Envoy accepts a matcher that can never match.
  **No differential observable** (neither fixture uses them). Recorded in `BEHAVIOR_CONTRACT.md`,
  not silent. Carried forward as **CF-67-4**.
- **`sni_cluster`.** It needs the downstream SNI without terminating TLS, i.e. a `tls_inspector`
  **listener filter** — a concept envoy-rust does not have at all (`crates/envoy-config/src/lib.rs:230`
  explicitly defers it: *"would require `tls_inspector` listener filter, deferred to a later
  phase"*). Two new subsystems in one phase. **Rejected as this phase's pick** (§4).
- **M66-6 and M66-7** (the missing dynamic/LDS terminal test; the cosmetics). M66-6 is *adjacent* —
  this phase edits the same validation pre-pass — and the PLAN may fold in the dynamic-listener test
  opportunistically, but it is not a commitment. Both stay LIVE.

### 2.3 §7.4 fuzz disposition

Doctrine §7.4: *"Every phase that introduces a parser, codec, or filter ships a `cargo fuzz`
target."* This phase introduces a **filter**, but one that **parses nothing** — network RBAC never
reads a downstream byte (R-0.2); it inspects `peer_addr` / `local_addr` only. Its sole
untrusted-input surface is the **bootstrap config parser**, already covered by the pre-existing
`parse_bootstrap` target (`crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`, wired at
`.github/workflows/ci.yml:77-124`), which reaches the new `TypedConfig::NetworkRbac` variant and the
new `CidrRange` parser the moment they land.

**Decision: no new fuzz target.** Add a **corpus seed** for the new `typed_config` shape instead.
This mirrors ADR-0123 §2.3 exactly. Two mechanical traps the PLAN must honor:

- the fuzz corpus dir is `*`-ignored — a new seed needs an explicit `!`-un-ignore line in
  `crates/envoy-config/fuzz/.gitignore`, **proven tracked with `git ls-files`**;
- a NEW target would need a hand-written `ci.yml` step — not applicable here, but the §7.5(d) gate
  **must be recorded explicitly as "satisfied by the pre-existing `parse_bootstrap` target"**, not
  passed over in silence.

This is a §7.4 interpretation and is therefore recorded in **ADR-0128**, not decided silently.

---

## §3. PLAN-VERIFY items (re-confirm against the live tree at the state-2 PLAN-write)

Every line number above was read this session, but the state-2 PLAN-write MUST re-confirm them fresh
(line drift is routine) and MUST resolve the following:

- **V-1.** The exact `Rules` / `Policy` / `Permission` / `Principal` shapes and their serde attribute
  sets, and whether adding new arms to `Principal` / `Permission` perturbs the **HTTP** RBAC filter
  (`crates/envoy-filter/src/rbac.rs`, 1270 lines) — the two now share one enum. **If the HTTP filter
  must exhaustively match on the enum, adding arms is a breaking change to it.** Determine whether
  the HTTP filter should reject the new L4-only arms (mirroring Envoy's own HTTP-vs-L4 split) and
  where. **This is the single biggest unknown in the phase.**
- **V-2. (Load-bearing architectural risk.)** The `main.rs` dispatch refactor. `echo::serve` and
  `direct_response::serve` each **own their accept loop**, while `tcp_proxy`/HCM go through
  `ConnectionHandler` (R-0.7). Inserting a per-connection pre-hook across both shapes is the phase's
  main structural cost. Settle: does the chain iterator wrap the listener (one accept loop that runs
  the filter chain, then dispatches to a terminal handler), or does each terminal `serve()` take an
  `Arc<[Box<dyn NetworkFilter>]>`? **Recommendation: hoist the accept loop out**, so `echo` /
  `direct_response` become per-connection handlers and the loop lives once. That change is also what
  makes **M66-3** (the `JoinSet` non-reaping, shared by both loops) a single fix rather than two.
- **V-3. RESOLVED at state-1 (R-0.13) — re-confirm only.** Precedence was probed: a chain violating
  BOTH rules (`[echo, rbac]`) reports the **terminal-not-last** error. An in-order scan over the chain
  reproduces this naturally (index 0's `echo` trips the terminal rule first), so
  `NetworkFilterChainNotTerminated` belongs in the **same** phase-66 immutable pre-pass, evaluated
  after the per-filter terminal check within one pass. The PLAN must still decide the exact code
  shape and pin the precedence with a test.
- **V-4.** Fixture determinism for the connection-level arms. `direct_remote_ip` sees the **Docker
  bridge address** inside the differential harness, which on this host is `192.168.65.2` and is
  explicitly a known host-fragility (memory `differential-host-bridge-ip-192-168-65-2`). Likewise a
  `destination_port` permission must match the `{{PORT}}`-substituted listener port, which **differs
  between the two proxies**. **Recommendation (and the SPEC's default): fixtures `0072`/`0073` use
  `any: true` only** — deterministic, host-independent — and every IP/port matcher is covered by the
  in-process backstops (§2.1 E), which bind `127.0.0.1` with a known port. Confirm at PLAN time.
- **V-5.** The `expected_stats` extension for raw-TCP drivers: reuse `KeepAliveExpectedStat` and the
  existing admin-scrape settle machinery, or add a new struct? Which admin endpoint/port does the
  raw-TCP arm scrape, and does the fixture need an `admin:` block on both sides (fixture `0072`'s
  Envoy side will need `admin.address` to expose `/stats`)?
- **V-6. RESOLVED at state-1 (R-0.13).** With `rules` omitted the filter is **INERT**: the connection
  is allowed and **neither `allowed` nor `denied` increments**. Model `rules: Option<Rules>` as
  `None ⇒ no policy engine, no counter ticks` — **do NOT** materialise a default
  `Rules { action: ALLOW, policies: {} }` and tick `allowed`, which would be a stat divergence with
  no body divergence. The PLAN must pin this with a test.
- **V-7.** Whether `source_ip` is merely a deprecated alias of `direct_remote_ip` upstream (it
  validates, R-0.4). If so, model it as an alias rather than a third code path.
- **V-8.** `CidrRange`'s `address_prefix` type (string vs `IpAddr`) and IPv6 handling; and whether
  `prefix_len` is a bare integer or Envoy's `{value: N}` `UInt32Value` wrapper.
- **V-9.** Confirm the fuzz-corpus `.gitignore` un-ignore line lands and `git ls-files` shows the new
  seed (R-0.12 / §2.3).

---

## §4. Rejected / deferred alternatives (the options this pick was chosen over)

1. **`sni_cluster`, the other candidate non-terminal filter.** Requires the downstream SNI *without*
   TLS termination, i.e. a `tls_inspector` **listener filter**. envoy-rust has **no listener-filter
   concept whatsoever** — `crates/envoy-config/src/lib.rs:230` defers it by name, and
   `bootstrap.rs:607` merely tolerates a `listener_filters:` block without modelling it. Picking
   `sni_cluster` means shipping the listener-filter subsystem **and** the network-filter iteration
   protocol **and** the filter, in one phase. **Rejected: strictly heavier than `rbac` for the same
   CF-66-2 payoff.** It remains the natural follow-on once listener filters exist.
2. **`redis_proxy` / `thrift_proxy`.** Both are **terminal** filters, so neither forces CF-66-2 —
   they would leave the family's central architectural gap open. Both are also full protocol codecs
   (hundreds of LoC of parsing plus a fuzz target), i.e. strictly more work for strictly less
   architectural progress.
3. **`mongo_proxy` / `zookeeper_proxy` / `kafka_broker`.** Non-terminal, so they *do* force CF-66-2 —
   but they **also** force **CF-67-3** (`on_data`-time iteration with buffering), because each parses
   the payload stream. `rbac` needs only the connection-establishment hook (R-0.2), which is a strict
   subset. **Deliberate ordering: land the connection-time protocol first with the filter that needs
   nothing more, then extend to `on_data` for the payload-parsing filters.**
4. **xDS file-based CDS hot-reload.** Deferred by ADR-0065, re-deferred by ADR-0067 and ADR-0123.
   `ClusterManager { clusters: HashMap<String, Arc<Cluster>> }` is a plain immutable map with no
   map-level swap primitive; the real cost is cluster-lifecycle churn (pool spawn/teardown,
   health-check probe-task lifecycle, outlier-sweeper lifecycle, in-flight-request safety on a
   removed cluster). ~800-1200 LoC with a near-certain split. **Still a good multi-sub-phase arc, and
   nothing about it decays by waiting.**
5. **LDS hot-reload.** Heavier still: no listener registry exists (`main.rs:210` serves only the
   FIRST listener), sockets bind once, and an update implies rebind + drain.
6. **The non-deterministic LB policies (`least_request` / `random`).** Require a contract-relaxation
   ADR FIRST — §7 demands exact equality on deterministic flows. Unchanged.
7. **The `DC` downstream-disconnect `%RESPONSE_FLAGS%` value.** Timing-dependent; **stays REJECTED**,
   as at every consideration through ADR-0123. This session's recon surfaced nothing that changes it.
8. **The cheap doc-only carry-forward leaves** (M64-2, M57-1, M53-2, M64-3, M65-1, M66-4, M66-7).
   They light up **no differential surface**. Phases 53/54/64/65/66 each added a real cross-proxy
   witness. M66-4 is nonetheless **consumed here** (§2.1 F) because this phase rewrites the very file
   it lives in — a fold-in, not a phase.

---

## §5. Differential surface at phase end

- **NEW fixture `0072-network-filter-rbac-deny`** — `[rbac(action: DENY, any), echo]`. The probe
  connects, writes a payload, and reads to EOF. **Zero bytes** on both proxies (byte-exact), a clean
  EOF (no RST), **and** `<stat_prefix>.rbac.denied == 1` / `.allowed == 0` on both. The stats
  assertion is what makes this a witness rather than a vacuous empty-vs-empty pass (R-0.9).
- **NEW fixture `0073-network-filter-rbac-allow`** — `[rbac(action: ALLOW, any), echo]`. The payload
  round-trips **byte-exact** through the terminal `echo`, and `.allowed == 1` / `.denied == 0`. This
  fixture is also the family's **first differential proof that a non-terminal filter runs and then
  yields to the terminal filter** — i.e. the iteration protocol itself.
- **NEW harness capability** — `expected_stats` on the raw-TCP driver family (R-0.9).
- All pre-existing fixtures `0001`-`0071` stay green (§7.5(b)). The new chain-termination rule
  affects **no existing config**: every existing chain is a single terminal filter (phase-66 R-0.8,
  re-confirmed by that phase's own empirical sweep), and an empty chain stays accepted (R-0.11).
- Conformance: unchanged. `h2spec` remains the only §7.3 suite; its pass-rate gate must stay green.
  **Never trim `known-failures.txt`.**

---

## §6. `BEHAVIOR_CONTRACT.md` additions

1. **Do-not-conflate banner for `rbac`.** `envoy.filters.network.rbac` (this L4 filter, which
   allows/denies a *connection*) is a different feature from `envoy.filters.http.rbac` (the HTTP
   filter at `crates/envoy-filter/src/rbac.rs`, which allows/denies a *request*). They share a name
   and, in envoy-rust, a config policy tree — but not a code path. Mirrors the `direct_response`
   banner phase 66 added.
2. **DENY semantics.** On a denied connection, the filter writes **zero bytes** and closes with a
   **clean EOF — never an RST**; bytes the client already sent are discarded; a post-EOF client write
   is accepted. *(Witnessed live against `envoyproxy/envoy:v1.33.0`; SPEC §0 R-0.2.)*
3. **ALLOW semantics.** The connection proceeds to the chain's terminal filter unchanged.
4. **Stats.** `<stat_prefix>.rbac.allowed` and `<stat_prefix>.rbac.denied` are exact on deterministic
   flows. `shadow_allowed` / `shadow_denied` are emitted as constant `0` (envoy-rust supports no
   shadow rules — CF-67-1). `stat_prefix` is required and non-empty.
5. **Network-filter chain termination (bilateral, completing the phase-66 rule).** A network filter
   chain must END in a terminal filter: upstream rejects a chain whose last filter is non-terminal
   with `non-terminal filter named <X> … is the last filter in a network filter chain`, exactly as it
   rejects a terminal filter that is not last. envoy-rust now enforces both directions
   (`ConfigError::NetworkFilterNotTerminal` + `ConfigError::NetworkFilterChainNotTerminated`).
   *(SPEC §0 R-0.1.)*
6. **Measured parity — an empty network filter chain is ACCEPTED by both proxies** (`filters: []` →
   `configuration OK` upstream). This **closes carry-forward M66-5(a)**, which had recorded
   envoy-rust's acceptance while explicitly declining to assert Envoy's. *(SPEC §0 R-0.11.)*
7. **Measured parity — `direct_response` with `response: {}` is REJECTED by both proxies** (upstream:
   proto `specifier is required`; envoy-rust: serde `missing field inline_string`). Same class,
   different text, which §7.4 permits. This **closes carry-forward M66-5(b)**. *(SPEC §0 R-0.11.)*
8. **Recorded divergence — L4 matcher arms (CF-67-4).** Upstream **rejects** `header` matchers in a
   network RBAC config and **accepts** `url_path` (which can never match at L4). envoy-rust rejects
   `header` (parity), and **also rejects `url_path` and `metadata`** (fail-loud, per the ADR-0049
   decision-2 (b) posture). No differential observable — neither fixture uses them.
9. **A `rbac` filter with `rules` omitted is INERT.** The connection is allowed and **neither
   `<stat_prefix>.rbac.allowed` nor `.denied` increments** — the counters stay at `0`. This is *not*
   "default action ALLOW, counted": a proxy that ticked `allowed` would diverge on stats while
   agreeing on the body. *(Witnessed live; SPEC §0 R-0.13.)*
10. **Network-filter validation error precedence.** When a chain violates both rules at once (a
    terminal filter that is not last, followed by a non-terminal filter that is last), both proxies
    report the **terminal-not-last** error, not the chain-not-terminated one. *(SPEC §0 R-0.13.)*

---

## §7. ADR reservations

- **ADR-0128 — FIRED this session.** Phase-67 pick + scope (this SPEC), including the §7.4
  no-new-fuzz-target disposition (§2.3), the §2.2 out-of-scope divergences, the carry-forward
  consumption of **CF-66-2 / M66-3 / M66-4**, and the **closure of M66-5** on measured evidence.
- **ADR-0129 — RESERVED, unfired.** To fire at the state-2 PLAN-write for the **§6.1 split**, which
  §8 projects as **LIKELY**. If the split does not fire, ADR-0129 lapses and is reclaimed by the next
  new-phase pick, per the standing lapsed-reservation convention.

---

## §8. Estimated size — the §6.1 split is projected to FIRE

| Area | Net LoC (est.) |
|---|---|
| `envoy-config`: const, `NetworkRbacConfig`, `TypedConfig` variant, validate arm | ~90 |
| `envoy-config`: `NetworkFilterChainNotTerminated` + pre-pass extension + tests | ~110 |
| `envoy-config`: `CidrRange` + 5 new `Principal`/`Permission` arms + V-1 fallout on HTTP RBAC | ~270 |
| Network-filter iteration protocol (`NetworkFilter` trait, `NetworkFilterStatus`, `ConnectionInfo`) | ~120 |
| `main.rs` dispatch refactor + hoisting the accept loop (consumes **M66-3**, **M66-4**) | ~280 |
| `envoy-bin/src/network_rbac.rs` engine + counters + unit tests | ~260 |
| `tests/differential`: `expected_stats` on the raw-TCP driver family | ~180 |
| fixtures `0072` + `0073` (4 files each) + 2 differential tests | ~140 |
| `envoy-bin/tests/` in-process backstops + negative config tests | ~230 |
| `BEHAVIOR_CONTRACT.md` rows + fuzz corpus seed | ~60 |
| **Total** | **~1740** |

**~1740 net LoC, ~16-20 TDD tasks.** The LoC estimate **exceeds the ~1500 threshold**; the task count
does not exceed ~25. Per §6.1 ("**either** threshold"), **the split fires.** The projected shape:

- **`67.1` — the protocol + the filter, `any`-matcher only.** Items (A)1-6, (B)8-9, (C)10-11,
  (D)12-14, (E)16, (F)17-20, (G)21-22. Ships fixtures `0072`/`0073`, the chain-termination rule, the
  iteration protocol, and a fully-exercised RBAC filter. **No stubs** — §6.3 satisfied, because
  `action: ALLOW/DENY` over `any: true` is completely witnessed by the two fixtures.
- **`67.2` — the connection-level matcher arms.** Item (A)7, item (E)15, and the CF-67-4 rejections:
  `CidrRange`, `direct_remote_ip` / `remote_ip` / `source_ip`, `destination_port` /
  `destination_ip`, plus the V-1 HTTP-RBAC-enum fallout, witnessed in-process (V-4 shows the IP/port
  arms are not host-deterministic under the Docker harness).

**The state-2 PLAN-write owns the split decision** (§6.1 triggers at step 2) and must re-derive the
estimate rather than inherit this one. If it splits, it creates `67.1`/`67.2` directories,
redistributes this SPEC, updates `ROADMAP.md` (row `67` becomes a parent with
`sub-phases = 67.1, 67.2`) and `STATE.md`, fires **ADR-0129**, and exits — per §6.2.
