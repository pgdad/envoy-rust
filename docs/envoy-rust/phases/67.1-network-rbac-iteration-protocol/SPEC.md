# Phase 67.1 — the network-filter chain iteration protocol + `envoy.filters.network.rbac` (`any`-matcher only)

> **Status:** SPEC authored at the phase-67 §6.1 split (`ADR-0129`). Next: §5 state-2 PLAN-write
> (`superpowers:writing-plans` → `PLAN.md`).
> **Parent phase:** `67` (`docs/envoy-rust/phases/67-network-filter-rbac/SPEC.md`), split into
> `67.1` + `67.2` per **ADR-0129**. Parent scope locked by **ADR-0128**.
> **ROADMAP row:** `67.1` (parent row `67`, `sub-phases = 67.1, 67.2`).
> **Sibling:** `67.2-network-rbac-connection-matchers` — the connection-level matcher arms.
> **Estimated size:** ~1455 net LoC / ~13-15 TDD tasks (both under the §6.1 gate).

This document is written for a stranger with zero prior context (doctrine D-3.4). Every load-bearing
claim is either quoted from the parent SPEC's measured state-0 recon (re-cited here so this file
stands alone) or was re-confirmed against the live tree at commit `6cfa8be` during the phase-67
state-2 PLAN-write/split session.

---

## §1. Goal

Ship the Network-filters family's **first non-terminal filter**, `envoy.filters.network.rbac`,
restricted to the **`any` matcher plus the and/or/not combinators**, and with it land the two
architectural pieces a non-terminal filter makes unavoidable:

1. the generic **network-filter chain iteration protocol** (carry-forward **CF-66-2**), and
2. the **bilateral chain-termination rule** — upstream Envoy rejects a chain whose *last* filter is
   non-terminal, the dual of the "a terminal filter must be last" rule phase 66 landed.

Witness DENY and ALLOW **byte-exact AND stat-exact** against upstream Envoy via new fixtures `0072`
and `0073`.

`67.1` is **fully exercised and stub-free** (BOOTSTRAP_PROMPT.md §6.3): `action: ALLOW` / `action:
DENY` over `permissions: [{any: true}]` / `principals: [{any: true}]` completely witnesses both
decision paths, the counters, and the whole iteration protocol. The connection-level matcher arms
(`direct_remote_ip`, `destination_port`, …) are **not stubbed here — they do not exist here**; the
config parser rejects them as unknown keys, and `67.2` adds them.

---

## §2. Measured evidence carried into this sub-phase

All rows below were measured at the phase-67 state-0 recon against the pinned image
`envoyproxy/envoy:v1.33.0` (digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, per
`docs/envoy-rust/ENVOY_TARGET.md`, doctrine D-3.7). They are quoted verbatim from the parent SPEC §0
so this file needs no cross-read.

### R-1 — `rbac` is NON-TERMINAL, and a chain must END in a terminal filter

`--mode validate` against the pinned image:

| Config chain | Result |
|---|---|
| `[rbac, echo]` | **`configuration OK`** — rbac is **non-terminal** |
| `[echo, rbac]` | `Error: terminal filter named envoy.filters.network.echo of type envoy.filters.network.echo must be the last filter in a network filter chain.` |
| `[rbac]` alone | **`Error: non-terminal filter named envoy.filters.network.rbac of type envoy.filters.network.rbac is the last filter in a network filter chain.`** |

Row 3 is the **bilateral dual** of phase 66's rule. envoy-rust does not enforce it. It was
structurally undiscoverable at phase 66, which had no non-terminal filter to violate it with.

### R-2 — DENY and ALLOW are deterministic

Live drive, listener `:10000`, chain `[rbac, echo]`, policy `permissions:[{any:true}]
principals:[{any:true}]`; client connects, writes `PING-RBAC\n`, reads. Verbatim:

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

On DENY, Envoy writes **zero bytes** and closes with a **clean EOF — no RST**; a post-EOF client
write is accepted; the client's already-sent bytes are discarded. On ALLOW, the connection proceeds
to the terminal `echo` and the payload round-trips. **The decision is taken once per connection, at
establishment, before any downstream byte is read** — which is why the protocol needs only an
`on_new_connection` hook and no `on_data` hook.

### R-3 — Stat names; `stat_prefix` required, `rules` optional

Counters: `<stat_prefix>.rbac.allowed`, `.denied`, `.shadow_allowed`, `.shadow_denied`.

- Omitting `stat_prefix` → `Proto constraint validation failed (RBACValidationError.StatPrefix: value length must be at least 1 characters)`. **Required, non-empty.**
- A `rbac` filter with only `stat_prefix` → `configuration OK`. **`rules` is optional.**

### R-4 — `rules` omitted ⇒ the filter is INERT (NOT "default action ALLOW, counted")

Runtime probe, `[rbac(stat_prefix only, no rules), echo]`:

```
  bytes=b'HELLO\n'  -> ALLOWED (echo round-tripped)
    norules.rbac.allowed: 0
    norules.rbac.denied: 0
    norules.rbac.shadow_allowed: 0
    norules.rbac.shadow_denied: 0
```

The connection is allowed and **NEITHER counter increments** — `allowed` stays `0`, not `1`.
**Model `rules: Option<Rules>` with `None ⇒ no engine, no counter ticks`.** Materialising a default
`Rules { action: ALLOW, policies: {} }` and ticking `allowed` would be a **stat divergence with no
body divergence** — invisible to a body-only fixture. (This resolved parent PLAN-VERIFY **V-6**.)

### R-5 — Error precedence: terminal-not-last WINS

A chain violating BOTH rules at once — `[echo, rbac]` (`echo` terminal-but-not-last AND `rbac`
non-terminal-but-last) — reports:

```
Error: terminal filter named envoy.filters.network.echo of type envoy.filters.network.echo must be the last filter in a network filter chain.
```

A single **in-order scan** over the chain reproduces this naturally: `echo` at index 0 trips the
terminal rule before the chain-termination rule is ever consulted. (This resolved parent PLAN-VERIFY
**V-3**.) **Pin it with a test.**

### R-6 — Which matcher arms L4 RBAC accepts, and the `header` rejection

`--mode validate`, one arm at a time, in a `[rbac, echo]` chain:

| Arm | As `principals:` | As `permissions:` |
|---|---|---|
| `any: true` | OK | OK |
| `not_rule` / `not_id`, `and_*`, `or_*` | OK | OK |
| `direct_remote_ip` / `remote_ip` / `source_ip` | **OK** | n/a |
| `destination_port` / `destination_ip` | n/a | **OK** |
| `url_path: {path:{exact:…}}` | **OK** (accepted, unmatchable at L4) | **OK** (same) |
| `header: {name:":path", …}` | **REJECTED** | **REJECTED** |

The `header` rejection is verbatim: `error initializing configuration: Found header(name: ":path"…`.

**This is why CF-67-4 lands in `67.1`, not `67.2` (an ADR-0129 change from the parent SPEC's
projected shape).** `67.1` reuses the existing `Permission`/`Principal` enums, whose arms today are
`any`/`header`/`and_*`/`or_*`/`not_*`/`metadata`/`url_path`. Without a validation walk, envoy-rust
would **accept** `[rbac(permissions:[{header:…}]), echo]` — a config upstream Envoy **rejects**. That
is a config-load divergence, and it would sit in `main` for the whole interval between `67.1` and
`67.2`. `67.1` therefore ships the L4 leaf allow-list (§3 D3); `67.2` widens it.

### R-7 — The empty chain is ACCEPTED by BOTH proxies (measured parity, closes M66-5)

`filters: []` on a network filter chain → upstream `configuration OK`. envoy-rust already accepts it.
**Parity, not a divergence.** The new chain-termination rule (§3 D2) must therefore apply only to
**non-empty** chains. (The phase-66 reviewer's intuition that Envoy rejects it was wrong — which is
exactly why that review recorded envoy-rust's behavior and declined to assert Envoy's. D-3.3.)

### R-8 — The differential harness has NO stats assertion on any raw-TCP driver

`tests/differential/src/lib.rs` `Driver` has 20 variants. `expected_stats` (a
`Vec<KeepAliveExpectedStat>`, defined at `:594`) exists on **only three**, all HTTP:
`Http1AfterSettle`, `Http1KeepAlive`, `Http2KeepAlive` (`:213`, `:258`). The four raw-TCP/TLS
variants — `TcpEcho` (`:40`), `TcpDirectResponse` (`:48`), `TlsTcp` (`:54`), `TlsTcpProbeList`
(`:68`) — carry **none**. `needs_admin_port` (`:2922`) likewise gates on only
`AdminScrape | Http1KeepAlive | Http2KeepAlive`.

**This is load-bearing.** `assert_body_rule`'s `ByteExact` is a bare `if envoy_body != rust_body`, so
a DENY fixture asserting only "both proxies returned zero bytes" **passes vacuously even if
envoy-rust never implemented RBAC and simply failed to write.** The DENY fixture is a real witness
only if it also asserts `<stat_prefix>.rbac.denied == 1`. **Extending the raw-TCP driver family with
`expected_stats` is an in-scope hard requirement, not polish.** (Fixture `0071` escaped this trap
only by carrying a non-empty payload.)

### R-9 — Runtime dispatch is a hardcoded 4-arm match over `filters.first()`

Re-confirmed at `6cfa8be`:

- `crates/envoy-bin/src/main.rs:215-219` — `.filter_chains.first().and_then(|c| c.filters.first())`:
  only the FIRST filter of the FIRST chain is read.
- `main.rs:241` — `match filter.name.as_str()` with exactly four arms: `ECHO_FILTER` (`:242`),
  `DIRECT_RESPONSE_FILTER` (`:252`), `TCP_PROXY_FILTER` (`:278`), `HCM_FILTER` (`:337`).
- `echo::serve` (`crates/envoy-bin/src/echo.rs:20`) and `direct_response::serve`
  (`crates/envoy-bin/src/direct_response.rs:31`) each own a **standalone accept loop**;
  `tcp_proxy`/HCM go through `envoy_listener::ConnectionHandler`
  (`crates/envoy-listener/src/lib.rs:38`) via the `bind_and_spawn_listener` helper (`main.rs:323`).
- `filters.first()` is safe **today only because** phase 66's terminal validation makes any
  ≥2-filter chain invalid. **A non-terminal filter breaks that interlock** — precisely why ADR-0123
  §2.2 deferred **CF-66-2** "to the first non-terminal network filter."

### R-10 — `is_terminal_network_filter` was designed for this drop-in

`crates/envoy-config/src/bootstrap.rs:825-833`:

```rust
fn is_terminal_network_filter(name: &str) -> bool {
    matches!(
        name,
        crate::ECHO_FILTER
            | crate::TCP_PROXY_FILTER
            | crate::HCM_FILTER
            | crate::DIRECT_RESPONSE_FILTER
    )
}
```

Its doc comment reads: *"written as a per-name predicate rather than a `chain.filters.len() <= 1`
check so that the first NON-terminal network filter (`sni_cluster`, network `rbac`) drops in without
re-litigating the rule."* **`rbac` must NOT be added to this predicate** — its absence IS its
non-terminality.

The phase-66 immutable pre-pass lives at `bootstrap.rs:3020-3029`, immediately before the mutating
`for filter in &mut chain.filters` loop at `:3030`.

### R-11 — envoy-rust's stats registry

`crates/envoy-stats/src/registry.rs:45` —
`pub fn register_counter(&self, name: &str) -> Result<Arc<Counter>, StatsError>`. Counters are
name-registered, so `<stat_prefix>.rbac.allowed` / `.denied` drop in with **no new stats machinery**.

### R-12 — NAMING HAZARD: `crates/envoy-filter/src/rbac.rs` is the **HTTP** RBAC filter

`crates/envoy-filter/src/rbac.rs` (1270 lines, `pub struct RbacFilter` at `:99`) implements
`envoy.filters.http.rbac` — **a different feature that shares the name**, exactly the
`direct_response` conflation hazard phase 66 hit and documented. `crates/envoy-filter/` models HTTP
filters only: `HttpFilterInstance` has 20 variants, every one an `envoy.filters.http.*`, and the
crate contains **no `trait`** and **no network-filter abstraction**.

The phase-67 network filter must be a **distinct type in a distinct namespace**
(`crates/envoy-bin/src/network_rbac.rs` + `NetworkRbacConfig`), and `BEHAVIOR_CONTRACT.md`'s
`## Network filters` section must gain a **do-not-conflate banner for `rbac`** mirroring the one it
already carries for `direct_response`.

### R-13 — Fixture + ADR numbering

Next free fixture ids are **`0072`** and **`0073`** (`tests/fixtures/` tops out at `0071`).
`DECISIONS.md` ledger head after the split is **ADR-0129**.

---

## §3. Deliverables

### D1 — Config surface: the `rbac` network filter (`crates/envoy-config/`)

- New const `NETWORK_RBAC_FILTER: &str = "envoy.filters.network.rbac"` in `src/lib.rs` alongside
  `ECHO_FILTER` (`:45`) / `TCP_PROXY_FILTER` (`:49`) / `HCM_FILTER` (`:53`) /
  `DIRECT_RESPONSE_FILTER` (`:58`).
- New `TypedConfig::NetworkRbac(NetworkRbacConfig)` variant keyed on `@type` =
  `type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC`.
- New `NetworkRbacConfig { stat_prefix: String, rules: Option<Rules> }`, `#[serde(deny_unknown_fields)]`.
  `stat_prefix` **required and non-empty** (R-3). `rules` **optional** (R-3). `deny_unknown_fields`
  is what rejects `shadow_rules` loudly (**CF-67-1**).
- `validate()` gains a `NETWORK_RBAC_FILTER` arm requiring a `TypedConfig::NetworkRbac`, mirroring
  the existing `DIRECT_RESPONSE_FILTER` arm.
- **`rbac` is NOT added to `is_terminal_network_filter`** (R-10).
- **Reuses** the existing `Rules` / `Policy` / `Permission` / `Principal` / `PermissionSet` /
  `PrincipalSet` types (`bootstrap.rs:1436-1700`) and their existing empty-set + `RBAC_TREE_MAX_DEPTH`
  (16) validations.

**PLAN-VERIFY W-1.** The existing empty-set / depth `ConfigError` variants
(`EmptyRbacPolicies`, `EmptyRbacPolicyPermissions`, `EmptyRbacPolicyPrincipals`,
`EmptyRbacPermissionSet`, `EmptyRbacPrincipalSet`, `RbacTreeTooDeep` — `lib.rs:457-505`) all render
their message with the literal prefix **`"HCM listener {listener:?}: …"`**. Reusing them verbatim for
a **network** `rbac` filter emits a misleading `HCM listener` string for a filter that has no HCM.
The PLAN must decide: generalize the message wording, add network-scoped sibling variants, or accept
the wording with a recorded rationale. **§7.4 permits differing error text, so this is an
internal-quality call, not a parity call — but it must be a decision, not an accident.**

### D2 — The bilateral chain-termination rule (`crates/envoy-config/`)

- New `ConfigError::NetworkFilterChainNotTerminated { listener, chain_index, last_filter }`.
- Every **non-empty** network filter chain must END in a terminal filter (R-1). An **empty**
  `filters: []` chain stays **ACCEPTED** — measured upstream parity (R-7), not an oversight.
- Implemented in the **same immutable pre-pass** phase 66 added at `bootstrap.rs:3020-3029`, as a
  single in-order scan so the **terminal-not-last error wins** when both rules are violated (R-5).

**Tests (all in `crates/envoy-config/`):**
- `[rbac]` alone → `NetworkFilterChainNotTerminated`.
- `[echo, rbac]` → `NetworkFilterNotTerminal` (**precedence pinned — R-5**).
- `[rbac, echo]` → OK.
- `filters: []` → OK (**R-7 parity pinned**).
- Every existing single-terminal-filter chain still OK (no regression).

### D3 — The L4 leaf allow-list (**CF-67-4**, moved here by ADR-0129)

A validation walk over a network `rbac` filter's `Permission`/`Principal` trees rejecting every leaf
except `any` (the combinators `and_rules`/`or_rules`/`not_rule` / `and_ids`/`or_ids`/`not_id` recurse
and are allowed).

- Rejecting `header` is **parity with upstream Envoy** (R-6, measured).
- Rejecting `url_path` and `metadata` is a **deliberate fail-loud divergence** (ADR-0049 decision-2
  (b)): upstream *accepts* a matcher that can never match at L4. **No differential observable** —
  neither fixture uses them. Recorded in `BEHAVIOR_CONTRACT.md`, never silent.
- New `ConfigError::UnsupportedNetworkRbacMatcher { listener, policy_name, arm, path }`.

**`67.2` widens this allow-list** to admit `direct_remote_ip` / `remote_ip` / `source_ip` /
`destination_port` / `destination_ip`. The `header`/`url_path`/`metadata` rejections stay forever.

**This deliverable CLOSES CF-67-4.**

### D4 — The network-filter chain iteration protocol (**CONSUMES CF-66-2**)

A **connection-establishment-only** protocol. Network RBAC evaluates once per connection, before any
downstream byte is read (R-2), so exactly one hook is needed:

```rust
pub enum NetworkFilterStatus { Continue, StopIteration }

pub trait NetworkFilter: Send + Sync {
    fn on_new_connection(&self, conn: &ConnectionInfo) -> NetworkFilterStatus;
}
```

`ConnectionInfo` carries the downstream `peer_addr: SocketAddr` and `local_addr: SocketAddr` —
everything R-6's arms need, including `67.2`'s.

**`on_data` is deliberately OUT of scope.** No filter in this phase inspects payload. Adding the hook
with no filter to exercise it is the §6.3 anti-pattern. Deferred as **CF-67-3** to the first
payload-parsing network filter (`mongo_proxy` / `zookeeper_proxy` / `kafka_broker`).

**PLAN-VERIFY W-2.** Which crate owns the trait? `envoy-listener` already owns `ConnectionHandler`
(`crates/envoy-listener/src/lib.rs:38`) and is the natural home; `envoy-bin` is the alternative but
would make the protocol un-reusable by a future `envoy-listener`-side LDS path. **Recommendation:
`envoy-listener`.** Settle at PLAN time.

### D5 — `main.rs` dispatch refactor + accept-loop hoist (**CONSUMES M66-3, M66-4**)

Refactor `main.rs`'s dispatch from *"read `filters.first()`"* to *"**iterate the chain**"*:

> Run each non-terminal filter's `on_new_connection` per accepted connection. On `StopIteration`,
> close the connection with a clean EOF and stop. When all return `Continue`, hand the connection to
> the terminal filter.

D2's chain-termination validation guarantees a terminal filter exists, so the iterator always
terminates in a handler.

**V-2 (the parent SPEC's biggest architectural risk), resolved.** `echo::serve` and
`direct_response::serve` each own an accept loop (R-9), while `tcp_proxy`/HCM go through
`ConnectionHandler`. **Hoist the accept loop out once**, making `echo` and `direct_response`
per-connection handlers. This:

- puts the loop in exactly one place, so the chain pre-hook is inserted once, not twice;
- turns **M66-3** (the `JoinSet` never reaps completed tasks — `echo.rs:21-59` and
  `direct_response.rs:36-74` share the defect **verbatim** — and the per-connection read is
  unbounded) into a **single** fix rather than two, which is exactly the joint repair unit the
  phase-66 review demanded; and
- preserves the "echo is the structural model" invariant that review insisted must not be broken by
  fixing one loop alone.

**M66-4** — the `direct_response.rs:93-94` doc-precision line ("Bounded by the caller's shutdown
drain") — is rewritten while that file is being restructured anyway.

**DO NOT weaken or delete `post_eof_client_write_is_accepted_not_reset`.** ADR-0124's drain
(`write_all` → `flush` → `shutdown()` → drain-to-EOF → drop, `direct_response.rs:82-102`) is pinned
by that test and must survive the restructure intact.

**DO NOT "fix" the `echo` `typed_config` asymmetry** (upstream requires it, envoy-rust forbids it —
the pre-existing ADR-0014 YAML shim behind fixture `0001`).

### D6 — The filter engine (`crates/envoy-bin/src/network_rbac.rs`, NEW)

- The policy engine over `ConnectionInfo`, for `any` + `and_*`/`or_*`/`not_*` only.
- Envoy's RBAC semantics: a policy matches when **any** permission matches **and** **any** principal
  matches; the engine's verdict is `action` if some policy matches, else the inverse of `action`.
- The two counters `<stat_prefix>.rbac.allowed` / `.denied` via `Registry::register_counter` (R-11).
- **`rules: None` ⇒ no engine, no counter ticks** (R-4). **Pin with a test.**
- **`shadow_allowed` / `shadow_denied` are emitted as constant `0`** so the stat tree matches
  upstream's shape (**CF-67-1**); `shadow_rules` as a config field is rejected by
  `deny_unknown_fields` (D1).
- DENY closes the connection with **zero bytes written and a clean EOF, never an RST** (R-2).

### D7 — Harness: `expected_stats` on the raw-TCP driver family (R-8)

Extend the raw-TCP driver family so a DENY fixture is not a vacuous empty-vs-empty comparison.

**PLAN-VERIFY W-3.** Reuse `KeepAliveExpectedStat` (`tests/differential/src/lib.rs:594`) and the
existing post-settle bilateral admin-scrape machinery (`:4763`, `scrape_admin_stat`), or add a new
struct? **Recommendation: reuse both** — the scrape helper is already generic over
`(admin_addr, stat_name)` and the settle is a plain `tokio::time::sleep(settle_ms)`.
`needs_admin_port` (`:2922`) must gain the raw-TCP arms, and it additionally gates on
`{{ADMIN_PORT}}` appearing in a template — so **fixture `0072`/`0073`'s Envoy side needs an `admin:`
block** to expose `/stats`. Confirm the `{{ADMIN_PORT}}` token discipline at PLAN time.

### D8 — Fixtures `0072` + `0073`

- **`tests/fixtures/0072-network-filter-rbac-deny/`** — `[rbac(action: DENY, any), echo]`. The probe
  connects, writes a payload, reads to EOF. Asserts **zero bytes byte-exact on both sides** AND
  `<stat_prefix>.rbac.denied == 1`, `.allowed == 0`. **The stats assertion is what makes this a
  witness rather than a vacuous pass (R-8).**
- **`tests/fixtures/0073-network-filter-rbac-allow/`** — `[rbac(action: ALLOW, any), echo]`. The
  payload round-trips **byte-exact** through the terminal `echo`; `.allowed == 1`, `.denied == 0`.
  This is also the family's **first differential proof that a non-terminal filter runs and then
  yields to the terminal filter** — i.e. the iteration protocol itself.

Both use **`any: true` ONLY**. This is deliberate and locked (parent PLAN-VERIFY V-4):
`direct_remote_ip` would see the Docker bridge address (`192.168.65.2` on this dev host — memory
`differential-host-bridge-ip-192-168-65-2`) and `destination_port` would see a `{{PORT}}` that
**differs between the two proxies**. Every IP/port matcher is covered **in-process** in `67.2`, bound
to `127.0.0.1` with a known port.

### D9 — In-process backstops (`crates/envoy-bin/tests/`)

- DENY: zero bytes, clean EOF, no RST; a post-EOF client write is accepted.
- ALLOW: payload round-trips through the terminal `echo`.
- `rules` omitted ⇒ INERT: connection allowed, **neither counter increments** (R-4).
- `StopIteration` closes the connection and the terminal filter never runs.
- Negative config: `[rbac]` alone rejected; `[echo, rbac]` rejected with the **terminal-not-last**
  error (precedence, R-5); `filters: []` accepted (R-7); `stat_prefix` missing/empty rejected;
  `header` / `url_path` / `metadata` leaves rejected (D3).
- **M66-3 regression witness:** the hoisted accept loop reaps completed `JoinSet` tasks (the set does
  not grow without bound across N sequential connections).

### D10 — Documentation + fuzz corpus seed

- `BEHAVIOR_CONTRACT.md` `## Network filters` (its section starts at `:229`) gains: the `rbac`
  do-not-conflate banner (R-12); DENY/ALLOW semantics + stat names; the **bilateral
  chain-termination rule**; the **error-precedence** rule (R-5); the `rules`-omitted **INERT** rule
  (R-4); the empty-chain and `response: {}` **measured-parity** findings (R-7, closing **M66-5**);
  and the `header`-rejected / `url_path`-accepted **asymmetry** with envoy-rust's fail-loud posture
  (R-6, **CF-67-4**).
- **Fuzz corpus seed** for the new `typed_config` shape. **NO new fuzz target** — see §5.

---

## §4. Out of scope (deliberate)

- **The connection-level matcher arms** (`CidrRange`, `direct_remote_ip`, `remote_ip`, `source_ip`,
  `destination_port`, `destination_ip`) → **`67.2`**. Not stubbed here: they simply do not exist, and
  the hand-rolled single-key-oneof deserializer (`impl_single_key_oneof!`, `bootstrap.rs:1627`,
  `:1675`) rejects them as unknown keys.
- **`shadow_rules` and the shadow counters.** Both counters emitted as constant `0` so the stat tree
  matches; the config field is rejected loudly. Carried as **CF-67-1**.
- **`Action::LOG`.** Audit-only, never enforces. Carried as **CF-67-2**.
- **`on_data`-time filter iteration** (mid-stream `Continue`/`StopIteration`, buffering,
  `injectReadDataToFilterChain`). No filter here reads payload (R-2). Carried as **CF-67-3**.
- **M66-6** (the missing dynamic/LDS-listener terminal test) is *adjacent* — this sub-phase edits the
  same validation pre-pass — and the PLAN **may** fold it in opportunistically. **Not a commitment.**
  **M66-7** stays live.

---

## §5. §7.4 fuzz disposition — NO new fuzz target (locked by ADR-0128 §2.3)

Doctrine §7.4: *"Every phase that introduces a parser, codec, or filter ships a `cargo fuzz`
target."* This sub-phase introduces a **filter**, but one that **parses nothing** — network RBAC
never reads a downstream byte (R-2); it inspects `peer_addr` / `local_addr` only. Its sole
untrusted-input surface is the **bootstrap config parser**, already covered by the pre-existing
`parse_bootstrap` target (`crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`, wired at
`.github/workflows/ci.yml:77-124`), which reaches the new `TypedConfig::NetworkRbac` variant the
moment it lands.

**Add a corpus seed instead.** Two mechanical traps:

- The fuzz corpus dir is `*`-ignored. A new seed needs an explicit `!`-un-ignore line in
  `crates/envoy-config/fuzz/.gitignore`, **proven tracked with `git ls-files`** (memory
  `fuzz-corpus-seed-gitignored-by-default`).
- A NEW target would need a hand-written `ci.yml` step (memory `new-fuzz-target-needs-a-ci-yml-step`)
  — **not applicable here**, but the **§7.5 gate (d) must be RECORDED EXPLICITLY at state-4 as
  "satisfied by the pre-existing `parse_bootstrap` target"**, not passed over in silence.

---

## §6. Differential surface at sub-phase end

- **NEW fixture `0072-network-filter-rbac-deny`** — zero bytes byte-exact + `rbac.denied == 1` /
  `.allowed == 0` on both proxies.
- **NEW fixture `0073-network-filter-rbac-allow`** — payload byte-exact through the terminal `echo` +
  `.allowed == 1` / `.denied == 0` on both proxies.
- **NEW harness capability** — `expected_stats` on the raw-TCP driver family (R-8).
- All pre-existing fixtures `0001`-`0071` stay green (§7.5(b)). The new chain-termination rule
  affects **no existing config**: every existing chain is a single terminal filter, and an empty
  chain stays accepted (R-7).
- Conformance: unchanged. `h2spec` remains the only §7.3 suite; its pass-rate gate must stay green.
  **Never trim `tests/conformance/h2spec/known-failures.txt`** — this dev host scores invalid-preface
  3.5/2 as PASS while CI fails it, so a locally-"fixed" list breaks CI (memory
  `h2spec-3-5-2-preface-host-sensitive`).

---

## §7. Estimated size — §6.1 does NOT fire for this sub-phase

| Area | Net LoC (est.) |
|---|---|
| D1 `envoy-config`: const, `NetworkRbacConfig`, `TypedConfig` variant, validate arm + tests | ~118 |
| D2 `NetworkFilterChainNotTerminated` + pre-pass extension + precedence/empty-chain tests | ~102 |
| D3 CF-67-4 L4 leaf allow-list walk + `ConfigError` variant + tests | ~85 |
| D4 `NetworkFilter` trait + `NetworkFilterStatus` + `ConnectionInfo` | ~120 |
| D5 `main.rs` dispatch refactor + accept-loop hoist (consumes **M66-3**, **M66-4**) | ~280 |
| D6 `network_rbac.rs` engine + 2 counters + unit tests | ~230 |
| D7 `tests/differential`: `expected_stats` on the raw-TCP driver family | ~180 |
| D8 fixtures `0072` + `0073` + 2 differential tests | ~140 |
| D9 in-process backstops + negative config tests | ~140 |
| D10 `BEHAVIOR_CONTRACT.md` rows + fuzz corpus seed | ~60 |
| **Total** | **~1455** |

**~1455 net LoC, ~13-15 TDD tasks.** Both under the §6.1 thresholds (~1500 LoC OR ~25 tasks). The LoC
figure is **close to the gate** — the state-2 PLAN-write must re-derive it, and §6.1's
**mid-execution** split valve remains available if any single task's sub-steps blow past ~10 items.

---

## §8. PLAN-VERIFY items (re-confirm fresh against the live tree at the state-2 PLAN-write)

Line numbers above were read at `6cfa8be`; **line drift is routine**.

- **W-1.** The `"HCM listener {listener:?}: …"` message prefix on the six reused RBAC `ConfigError`
  variants (`lib.rs:457-505`) is HTTP-specific. Decide: generalize, add network-scoped siblings, or
  accept with a recorded rationale. (D1.)
- **W-2.** Which crate owns `NetworkFilter` / `NetworkFilterStatus` / `ConnectionInfo`.
  Recommendation: `envoy-listener` (it already owns `ConnectionHandler`). (D4.)
- **W-3.** The `expected_stats` extension for raw-TCP drivers: reuse `KeepAliveExpectedStat` + the
  existing settle/scrape machinery (recommended), or add a new struct? Which admin endpoint does the
  raw-TCP arm scrape? Confirm fixture `0072`'s Envoy side needs an `admin:` block and that
  `needs_admin_port` gates correctly on the `{{ADMIN_PORT}}` token. (D7.)
- **W-4.** The exact shape of the hoisted accept loop: does the chain iterator wrap the listener (one
  accept loop that runs the filter chain, then dispatches to a terminal handler), or does each
  terminal `serve()` take an `Arc<[Box<dyn NetworkFilter>]>`? **Recommendation: hoist the loop.**
  Confirm the ADR-0124 drain and `post_eof_client_write_is_accepted_not_reset` survive intact. (D5.)
- **W-5.** Confirm the fuzz-corpus `.gitignore` un-ignore line lands and `git ls-files` shows the new
  seed. (§5.)
- **W-6.** Re-confirm R-5 (precedence) and R-4 (`rules`-omitted INERT) are pinned by tests — both
  were resolved empirically at the parent state-1 recon and need **re-confirmation, not re-probing**.

---

## §9. Standing traps (read before touching code)

1. **`cargo build -p envoy-bin` before ANY local differential run** — the harness executes
   `target/debug/envoy-bin`, not release, and this sub-phase adds a NEW config key AND a NEW filter
   name, so a stale binary REDs with `unsupported network filter` / `unknown field` (memory
   `differential-harness-uses-debug-envoy-bin`).
2. **Never pipe a verification run through `tail`** — it truncates the `failures:` block and destroys
   the failing test names a state-4 gate must adjudicate (memory
   `never-pipe-verification-runs-through-tail`).
3. **`cargo test --workspace` exits 101 on this dev host and its bare form aborts at the first
   failing test binary** — always add `--no-fail-fast`. An invariant core of ~5 REDs
   (`0061`/`0062`/`0069`/`0070` + `admin_config_dump_server_info`) fails deterministically in
   isolation ⇒ environmental. **CI is authoritative.**
4. **Never weaken a fixture. Never trim `known-failures.txt`.** h2spec is a SKIP, not a pass, locally.
5. **Do NOT re-open BLOCK-66-1** (ADR-0126): no `--quiet`, no removed pre-build, no widened 30s
   budget.
6. **ROADMAP rows must escape literal `|` as `\|`.** Rows `36`/`38`/`39`/`52`/`54` are already
   malformed and must NOT be "fixed" (append-only, D-3.5). Verify any row edit with an escape-aware
   split: `re.split(r'(?<!\\)\|', line)[1:-1]` must yield exactly 6 cells. **Never use `awk -F'|'`.**
7. **Confirm CI with the FULL 40-char SHA** — `gh run list --commit <short-sha>` silently returns
   `[]` (memory `gh-run-list-commit-needs-full-sha`). CI has no `paths-ignore`; docs-only pushes DO
   build.

---

## §10. Carry-forward ledger

- **CONSUMED by `67.1`:** **CF-66-2** (the iteration protocol — D4/D5 *is* it), **M66-3** (the
  `JoinSet` non-reaping + unbounded per-connection drain shared verbatim by `echo.rs:21-59` and
  `direct_response.rs:36-74`), **M66-4** (the `direct_response.rs:93-94` doc-precision line),
  **CF-67-4** (the L4 matcher-arm posture — D3, moved here from `67.2` by ADR-0129).
- **CLOSED by the parent SPEC's recon R-0.11, no code change: M66-5** (see R-7).
- **OPENED by ADR-0128, still live:** **CF-67-1** (`shadow_rules`), **CF-67-2** (`Action::LOG`),
  **CF-67-3** (`on_data`-time iteration + buffering).
- **DEFERRED to `67.2`:** the connection-level matcher arms + `CidrRange` + the V-1 HTTP-RBAC-enum
  fallout.
- **Still live, none blocks:** M66-6, M66-7, CF-66-1, M64-2, M64-3, M65-1, M57-1, M55-1, M53-2,
  M53-3, M48-2, M42-1, the `DC`/retry-budget-overflow slices of M45-2, the phase-58 candidate
  carry-forward, M40-1, M39-1/M39-2, M38-1/M38-2, CF-39-1, M37-*, M36-*, M34-*, M33-*, the
  empty-`metadata_match` doc-comment, M29-*/M30-*, the phase-31 cosmetics, and the
  HTTP-filters-family (1)-(4).
- **Numbering: M66-1 was never allocated.** The ledger advances monotonically and does not backfill.
  Do not "fix" the gap.
