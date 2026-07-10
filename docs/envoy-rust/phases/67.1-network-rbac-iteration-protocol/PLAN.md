# Phase 67.1 — Network-filter chain iteration protocol + `envoy.filters.network.rbac` (`any`-matcher only) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task runs `superpowers:test-driven-development` — tests first, no exceptions (doctrine D-3.1).

**Goal:** Ship the Network-filters family's first NON-terminal filter, `envoy.filters.network.rbac` (restricted to the `any` matcher plus the and/or/not combinators), and with it the generic network-filter chain iteration protocol and the bilateral chain-termination rule — witnessed byte-exact AND stat-exact against upstream Envoy `v1.33.0` by new differential fixtures `0072` (DENY) and `0073` (ALLOW).

**Architecture:** A network filter chain is split at config-load time into a prefix of non-terminal filters and exactly one terminal filter (a rule the new `NetworkFilterChainNotTerminated` validation now enforces bilaterally). At runtime, `envoy-bin` builds `Vec<Arc<dyn NetworkFilter>>` from the prefix and wraps the terminal filter's existing `envoy_listener::ConnectionHandler` in a new `envoy_listener::ChainHandler`, which runs each `on_new_connection` hook once per accepted connection before delegating. `echo` and `direct_response` — which today each own a **standalone accept loop** — become plain `ConnectionHandler` implementations, so **all four** terminal network filters bind through the one pre-existing `envoy_listener::Listener` accept loop.

**Tech Stack:** Rust 2024 (toolchain pinned by `rust-toolchain.toml`), `tokio`, `serde`/`serde_yaml`, `thiserror`, `anyhow` (binary crate only), `testcontainers` (differential harness). **No new dependency is added.** One existing dependency gains one feature flag (`envoy-listener`'s `tokio` gains `io-util`).

---

## Global Constraints

Every task's requirements implicitly include this section.

- **D-3.8:** every workspace crate root (`lib.rs` / `main.rs`) keeps `#![forbid(unsafe_code)]`. No `unsafe` anywhere in this sub-phase.
- **D-3.2:** no new crate dependency. `envoy-listener`'s `tokio` features gain `io-util` (needed for `AsyncReadExt`/`AsyncWriteExt` in `close_with_drain`); nothing else changes in any `Cargo.toml`.
- **D-3.9:** do not touch `rust-toolchain.toml`.
- **D-3.7:** do not touch `docs/envoy-rust/ENVOY_TARGET.md`. The pinned reference is `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`.
- **`rbac` is NEVER added to `is_terminal_network_filter`** (`crates/envoy-config/src/bootstrap.rs:825-833`). Its **absence** from that predicate IS its non-terminality.
- **No `on_data` hook.** The protocol is `on_new_connection` only. Deferred as **CF-67-3**.
- **Naming hazard 1 — the filter.** `crates/envoy-filter/src/rbac.rs` (1270 lines) is `envoy.filters.**http**.rbac` — a *different feature sharing the name*. The network filter is a distinct type in a distinct namespace: `crates/envoy-bin/src/network_rbac.rs`, `envoy_config::NetworkRbacConfig`. Never edit `crates/envoy-filter/src/rbac.rs` in this sub-phase.
- **Naming hazard 2 — `NetworkFilter` means two different things.** `envoy_config::NetworkFilter` is the **config struct** `{ name: String, typed_config: Option<TypedConfig> }` (`bootstrap.rs:666`, re-exported at `lib.rs:29`). `envoy_listener::NetworkFilter` is Task 5's **runtime trait**. Task 10 uses both in the same function. **Always fully qualify them** (`envoy_config::NetworkFilter` / `envoy_listener::NetworkFilter`); never `use` either unqualified into `main.rs`. This plan's code does so throughout — preserve it.
- **Re-exports.** `envoy_config`'s crate root already re-exports `Action`, `Permission`, `PermissionSet`, `Principal`, `PrincipalSet`, `Policy`, `Rules`, `FilterChain`, `NetworkFilter`, `TypedConfig` (`lib.rs:14-38`). **Task 1 must ADD `NetworkRbacConfig` to that `pub use bootstrap::{…}` list**, or Tasks 9 and 10 cannot name it. `envoy_stats::Counter::value()` already exists (`crates/envoy-stats/src/counter.rs:33`).
- **Do not weaken or delete `post_eof_client_write_is_accepted_not_reset`** (`crates/envoy-bin/src/direct_response.rs`). It pins ADR-0124's drain and must survive Task 8's restructure.
- **Do not "fix" the `echo` `typed_config` asymmetry** (upstream requires it, envoy-rust forbids it — the ADR-0014 YAML shim behind fixture `0001`).
- **Do not re-open BLOCK-66-1** (ADR-0126): no `--quiet` on any nested `cargo run`, no removed pre-build, no widened 30 s readiness budget.
- **Never weaken a fixture. Never trim `tests/conformance/h2spec/known-failures.txt`** — this dev host scores invalid-preface 3.5/2 as PASS while CI fails it, so a locally-"fixed" list breaks CI. h2spec is a SKIP, not a pass, locally; do not install it.
- **Never pipe a verification run through `tail`** — it truncates the `failures:` block and destroys the failing test names.
- **`cargo test --workspace` exits 101 on this dev host and its bare form aborts at the first failing test binary — always add `--no-fail-fast`.** An invariant core of ~5 REDs (fixtures `0061`/`0062`/`0069`/`0070` + `admin_config_dump_server_info`) fails deterministically in isolation ⇒ environmental. **CI is authoritative.**
- **`cargo build -p envoy-bin` before ANY local differential run.** The harness executes `target/debug/envoy-bin` (not release), and this sub-phase adds a NEW config key AND a NEW filter name, so a stale binary REDs with `unsupported network filter` / `unknown field`.
- **Commit after every task** (frequent commits). Do not push until the whole plan is green locally; state-3 pushes are expected to red on CI at `cargo fmt --all -- --check` if you skip fmt.

---

## §0. §6.1 Split Gate — RE-DERIVED against the live tree at `8b91f89`. **The gate does NOT fire.**

`BOOTSTRAP_PROMPT.md` §6.1 splits when a plan exceeds **~25 numbered tasks** OR **~1500 net LoC**. `SPEC.md` §7 projected **~1455 LoC / ~13-15 tasks** and explicitly forbade inheriting that number. It was re-derived. It moved **down**, to **~1442 LoC / 14 tasks**, for one dominant reason recorded below.

| Deliverable | SPEC §7 | Re-derived | Why it moved |
|---|---:|---:|---|
| D1 `envoy-config`: const, `NetworkRbacConfig`, `TypedConfig` variant, validate arm + tests | ~118 | ~110 | — |
| D1/W-1 error-text generalization + `validate_rbac_rules` extraction | (in D1) | ~35 | Split out as its own task (T2). |
| D2 `NetworkFilterChainNotTerminated` + pre-pass + precedence/empty-chain tests | ~102 | ~102 | Confirmed. (+ M66-6's LDS test folded in.) |
| D3 CF-67-4 L4 leaf allow-list + `ConfigError` variant + tests | ~85 | ~110 | Two hand-written recursive walks (the `define_rbac_tree_validator!` macro cannot be reused: its shared body already matches all 7 arms and the L4 verdicts differ per-arm). |
| D4 `NetworkFilter` + `NetworkFilterStatus` + `ConnectionInfo` + `ChainHandler` + `close_with_drain` + `pending_tasks()` | ~120 | ~155 | `ChainHandler` and the M66-3 reaping witness live here. |
| **D5 `main.rs` dispatch refactor + accept-loop hoist** | **~280** | **~170** | **The dominant correction — see below.** |
| D6 `network_rbac.rs` engine + counters + unit tests | ~230 | ~230 | Confirmed. |
| D7 `tests/differential`: `expected_stats` on the raw-TCP driver family | ~180 | ~190 | + the shared bilateral-scrape helper extraction. |
| D8 fixtures `0072` + `0073` + 2 differential tests | ~140 | ~150 | Both fixtures need an `admin:` block. |
| D9 in-process backstops + negative config tests | ~140 | ~160 | + the empty-chain no-panic backstop. |
| D10 `BEHAVIOR_CONTRACT.md` rows + fuzz corpus seed | ~60 | ~65 | — |
| **Total** | **~1455** | **~1442** | |

**Why D5 shrank by ~110 LoC — the load-bearing live-tree finding.** `SPEC.md` D5 assumed the accept-loop hoist means *writing* a new hoisted accept loop. It does not. **`envoy_listener::accept_loop` already exists, and it already reaps** (`crates/envoy-listener/src/lib.rs:565`):

```rust
Some(done) = join_set.join_next(), if !join_set.is_empty() => { … }
```

`tcp_proxy` and HCM already flow through it via `bind_and_spawn_listener` (`crates/envoy-bin/src/main.rs:854-887`). **M66-3's non-reaping `JoinSet` defect exists ONLY in the two standalone loops** (`echo.rs:21-59`, `direct_response.rs:36-74`). So the hoist is **mostly deletion**: convert `echo`/`direct_response` into `ConnectionHandler` implementations, delete both standalone accept loops, and route all four terminal filters through the one loop that already reaps. **M66-3 is consumed by removal, not by repair** — and M66-4's doc-precision line ("Bounded by the caller's shutdown drain") becomes *literally true* rather than aspirational, because `Listener::serve`'s `DRAIN_BUDGET` + `abort_all()` now genuinely bound the per-connection read.

**Task count: 14.** Under ~25. **Net LoC: ~1442.** Under ~1500. **§6.1 does not fire.**

> **§6.1's mid-execution valve stays armed.** If any single task's sub-steps blow past ~10 items once contact with reality reveals complexity, STOP and split per §6.2. No task below exceeds 7 steps as written.

---

## §1. PLAN-VERIFY resolutions (SPEC §8, W-1 … W-6) — all re-confirmed fresh at `8b91f89`

### W-1 — RESOLVED: **generalize the message prefix** on the six reused RBAC `ConfigError` variants.

The six variants (`EmptyRbacPolicies`, `EmptyRbacPolicyPermissions`, `EmptyRbacPolicyPrincipals`, `EmptyRbacPermissionSet`, `EmptyRbacPrincipalSet`, `RbacTreeTooDeep`, `crates/envoy-config/src/lib.rs:457-505`) all render `"HCM listener {listener:?}: …"`. A network `rbac` filter **has no HCM**, so reusing them verbatim prints a false claim in an error a human reads.

**Decision:** change the rendered prefix from `"HCM listener {listener:?}: "` to `"listener {listener:?}: "` on **those six variants only**. Fields are unchanged; the variant shapes are unchanged.

**Blast radius: measured, and it is zero.** Every landed assertion over these six variants is *variant-shaped* (`matches!(err, ConfigError::EmptyRbacPermissionSet { .. })`, `ConfigError::EmptyRbacPolicies { ref listener } if listener == "ingress_http"` — `bootstrap.rs:13092-13188`). **No test asserts the rendered string.** §7.4 permits differing error text between the proxies (upstream's text differs from ours regardless), so this is an internal-quality call, not a parity call. The other twelve `"HCM listener"` variants in `lib.rs` are genuinely HCM-scoped and are **not touched**. Recorded in **ADR-0130**.

### W-2 — RESOLVED: **`envoy-listener` owns the protocol.**

`NetworkFilter`, `NetworkFilterStatus`, `ConnectionInfo` (and the new `ChainHandler` + `close_with_drain`) land in `crates/envoy-listener/src/lib.rs`, which already owns `ConnectionHandler` (`:38`) and the accept loop. Putting them in `envoy-bin` would make the protocol un-reusable by a future `envoy-listener`-side LDS path. `crates/envoy-bin/src/network_rbac.rs` *implements* the trait; it does not define it.

`ConnectionHandler::handle(&self, downstream: TcpStream)` takes no address arguments — and needs none. `ChainHandler` builds `ConnectionInfo` from `downstream.peer_addr()` / `downstream.local_addr()`. **The `ConnectionHandler` trait is not modified.**

### W-3 — RESOLVED: **reuse `KeepAliveExpectedStat` + the existing settle/scrape machinery.** Add ONE new `Driver` variant.

- Reuse `KeepAliveExpectedStat { name, value }` (`tests/differential/src/lib.rs:594`) verbatim — it is codec-agnostic, exactly as `Http2KeepAlive` already reuses it.
- Reuse `scrape_admin_stat(admin_addr, stat_name)` (`:2593`) and the plain `tokio::time::sleep(settle_ms)` settle.
- **Extract** the post-settle bilateral scrape loop (`:4763-4784`, duplicated in the `Http2KeepAlive` arm) into a shared `assert_expected_stats_bilaterally(...)` helper and call it from all three arms. DRY.
- **`needs_admin_port` (`:2922`) must gain the new arm**, and `port_key` (`:2861`) must map it to `"PORT"`. `needs_admin_port` is *also* what is passed to `upstream::start` as `expose_admin_port` (`:3836`) and what injects `ADMIN_PORT = upstream::ADMIN_CONTAINER_PORT` into the upstream template (`:3440-3445`). **Confirmed: adding the variant to that one `matches!` is sufficient to wire both sides.** It additionally gates on `{{ADMIN_PORT}}` appearing in a template, so **both `0072` and `0073` need an `admin:` block on both sides.**

**Do NOT add fields to `Driver::TcpEcho` / `Driver::TcpDirectResponse`.** They are serde **unit** variants (`driver: { kind: tcp_echo }`); adding `#[serde(default)]` fields turns them into struct variants, which breaks every landed `expectations.yaml` and five `matches!(e.driver, Driver::TcpEcho)` parse tests. Add a new variant instead:

```rust
Driver::TcpWithStats { probe: TcpProbeKind, settle_ms: u64, expected_stats: Vec<KeepAliveExpectedStat> }
enum TcpProbeKind { Echo, ReadToEof }
```

`Echo` calls the existing `drive_tcp` (write payload → `read_exact` → ADR-0006/0007 trailing-byte poll); `ReadToEof` calls the existing `drive_tcp_direct_response` (send nothing → read to EOF). No new wire driver is written.

### W-4 — RESOLVED: **hoist by deletion. Reuse `envoy_listener::Listener`; do not write a new accept loop.**

See §0. `echo` and `direct_response` become `ConnectionHandler` implementations and bind through the existing `bind_and_spawn_listener`. The chain pre-hook is inserted in exactly ONE place: `ChainHandler`, which wraps any `Arc<dyn ConnectionHandler>`. This works **uniformly for all four** terminal filters — `[rbac, echo]`, `[rbac, direct_response]`, `[rbac, tcp_proxy]`, `[rbac, http_connection_manager]` — with no per-arm special-casing. (A `network_chain.rs` accept loop private to `envoy-bin` was considered and rejected: it would have left `[rbac, tcp_proxy]` and `[rbac, hcm]` unhandled, i.e. a §6.3 stub, while duplicating a loop that already exists and already reaps.)

**Two consequences, both recorded in ADR-0130:**

1. **`echo` and `direct_response` listeners now emit the per-listener stat set** (`listener.<name>.downstream_cx_total` / `.downstream_cx_active` / `.downstream_cx_accept_failed`) and count in `listener_manager.total_listeners_active`, which `Listener::bind` registers. `crates/envoy-listener/src/lib.rs:118-121` documents their previous exclusion as "architecture-decision lock-in #12"; this supersedes it. **Blast radius measured: zero.** Neither fixture `0001` (`echo`) nor `0071` (`direct_response`) carries an `admin:` block, so no fixture scrapes stats on those listeners. `total_listeners_active` is asserted only by fixture `0027` and `xds_file_based_lds.rs` (both HCM listeners) and by fixture `0011`'s Prometheus name-set equality (an HCM listener). Fixtures `0072`/`0073` assert stats **by name** via `scrape_admin_stat`, never by set-equality, so the extra names are inert there. Moving toward emitting them is also *closer* to upstream Envoy, which counts every listener.
2. **Both listeners now participate in the graceful drain** (`/drain_listeners`) and in `SO_REUSEPORT` binding, which they did not before. This is strictly more Envoy-like and breaks no landed assertion.

**The ADR-0124 drain survives.** `direct_response_once`'s `write_all` → `flush` → `shutdown()` → drain-to-EOF → drop sequence is preserved exactly, with its tail factored into `envoy_listener::close_with_drain`. `post_eof_client_write_is_accepted_not_reset` moves with it, unweakened, and keeps its mutation-check doc comment.

### W-5 — RESOLVED: the corpus seed needs an explicit `!`-un-ignore line.

`crates/envoy-config/fuzz/.gitignore` opens with `corpus/parse_bootstrap/*` and then un-ignores each seed by name (54 lines today, last: `!corpus/parse_bootstrap/network_filter_direct_response.yaml`). Task 14 adds `!corpus/parse_bootstrap/network_filter_rbac.yaml` and **proves the seed tracked with `git ls-files`**.

### W-6 — RE-CONFIRMED (not re-probed), and both are pinned by tests.

- **Error precedence (R-5).** A chain violating both rules — `[echo, rbac]` — reports the **terminal-not-last** error. Reproduced naturally by scanning in order: the existing in-order terminal scan (`bootstrap.rs:3020-3029`) runs first and trips at index 0; the new chain-termination check runs *after* it. **Pinned by Task 3, step 1.**
- **`rules` omitted ⇒ INERT (R-4).** The connection is allowed and **neither counter increments**. Modelled as `rules: Option<Rules>`; `None` ⇒ no engine, no ticks. **Pinned by Task 9, step 1 (unit) and Task 13 (in-process, against the real binary).**

---

## §2. NEW live-tree finding — envoy-rust **PANICS at startup** on `filters: []`

Measured this session at `8b91f89`:

```
$ cargo run -q -p envoy-bin -- -c empty-chain.yaml
thread 'main' panicked at crates/envoy-bin/src/main.rs:219:14:
validator guarantees ≥1 filter
```

`main.rs:215-219` reads `.filter_chains.first().and_then(|c| c.filters.first()).expect("validator guarantees ≥1 filter")`. The validator does **not** guarantee that: `filters: []` is accepted (SPEC R-7, measured parity with upstream, which is what CLOSED **M66-5**). Upstream Envoy accepts the same config and **starts**; envoy-rust **crashes**.

M66-5 closed *config-load* parity. **Runtime parity was never checked, and it does not hold.** This sits exactly on D5's surface (the very `expect()` the dispatch refactor deletes), so it is fixed here rather than deferred.

**The fix must not invent unmeasured behavior.** What upstream Envoy does with a *connection* to an empty-chain listener was not probed, and this session does not re-probe (W-6 discipline). So envoy-rust: logs a `tracing::warn!` and **binds no data listener** for a first chain with no filters, exiting the dispatch block cleanly (the admin listener, spawned independently at `main.rs:730`, still serves). No panic, no guess about connection semantics. Recorded as a **divergence with no differential observable** in `BEHAVIOR_CONTRACT.md` (no fixture uses an empty chain), and the un-probed connect behavior is carried forward as **CF-67-5**. See **ADR-0130**.

---

## §3. ADR-0130 (claim it; it is unreserved)

Append `ADR-0130` to `docs/envoy-rust/DECISIONS.md` **with the state-2 PLAN-write commit** (D-3.5: decisions are written, not remembered). It records, in one ADR:

1. **W-1** — generalizing the six RBAC `ConfigError` message prefixes from `"HCM listener {listener:?}"` to `"listener {listener:?}"`; zero test blast radius (measured); §7.4 permits differing error text.
2. **W-4** — the hoist is **by deletion**: `envoy_listener::accept_loop` already exists and already reaps, so `echo`/`direct_response` become `ConnectionHandler`s and both standalone loops are removed. **M66-3 is consumed by removal.** The rejected alternative (a new `envoy-bin`-private accept loop) is named, with its §6.3 defect: it would leave `[rbac, tcp_proxy]` / `[rbac, hcm]` unhandled.
3. **The consequential stat-surface addition** — `echo`/`direct_response` listeners now emit `listener.<name>.downstream_cx_*` and count in `listener_manager.total_listeners_active`, superseding the `envoy-listener` lock-in-#12 comment. Blast radius measured as zero; the change is toward upstream parity.
4. **§2's startup panic** on `filters: []` — a *runtime* divergence that M66-5's *config-load* parity finding did not cover; the non-speculative fix (warn + bind nothing); and **CF-67-5** (probe upstream Envoy's connect behavior on an empty chain before asserting anything about it).
5. **The re-derived §6.1 estimate** (~1442 LoC / 14 tasks) and the explicit verdict that **the gate does not fire** for `67.1`.
6. **W-2/W-3/W-5/W-6** resolutions as above.

ADR-0130 supersedes no prior ADR. **ADR-0124 is untouched and must survive Task 8 intact.** **ADR-0129** (the split), **ADR-0128** (the pick + scope), **ADR-0123**, **ADR-0014**, **ADR-0049**, **ADR-0035** all remain in force. **ADR-0127**'s one-off §5.1 chaining override does **not** apply.

---

## §4. File Structure

**`crates/envoy-config/`**
- `src/lib.rs` — Modify. New `NETWORK_RBAC_FILTER` const (beside `DIRECT_RESPONSE_FILTER`, `:58`). Three new `ConfigError` variants. Six existing RBAC variants' message prefix generalized.
- `src/bootstrap.rs` — Modify. `NetworkRbacConfig` struct; `TypedConfig::NetworkRbac` variant; a `NETWORK_RBAC_FILTER` arm in the per-filter validation match; the chain-termination check in the phase-66 immutable pre-pass; `validate_rbac_rules` extracted from `validate_rbac_config`; `validate_network_rbac_config`; `validate_l4_permission` / `validate_l4_principal`.

**`crates/envoy-listener/`**
- `Cargo.toml` — Modify. `tokio` features gain `"io-util"`.
- `src/lib.rs` — Modify. `NetworkFilterStatus`, `NetworkFilter`, `ConnectionInfo`, `ChainHandler`, `close_with_drain`, `Listener::pending_tasks()` + its watch plumbing in `accept_loop`.

**`crates/envoy-bin/`**
- `src/echo.rs` — Rewrite. `serve()` + its accept loop DELETED; `EchoHandler: ConnectionHandler` added.
- `src/direct_response.rs` — Rewrite. `serve()` + its accept loop DELETED; `DirectResponseHandler: ConnectionHandler` added. ADR-0124 drain preserved via `close_with_drain`.
- `src/network_rbac.rs` — **Create.** `NetworkRbacFilter: NetworkFilter` + the four counters.
- `src/main.rs` — Modify. `mod network_rbac;`. Chain-splitting dispatch; all four terminal arms through `bind_and_spawn_listener`; the `filters: []` panic removed.
- `tests/network_filter_rbac.rs` — **Create.** In-process backstops against the real binary.

**`tests/differential/`**
- `src/lib.rs` — Modify. `Driver::TcpWithStats` + `TcpProbeKind`; `run_tcp_with_stats_arm`; `assert_expected_stats_bilaterally` extracted; `needs_admin_port` + `port_key` widened.
- `tests/network_filter_rbac_deny.rs`, `tests/network_filter_rbac_allow.rs` — **Create.**

**`tests/fixtures/0072-network-filter-rbac-deny/`, `tests/fixtures/0073-network-filter-rbac-allow/`** — **Create.** `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md` each; `0073` also `inputs/payload.bin`.

**`crates/envoy-config/fuzz/`** — `.gitignore` + `corpus/parse_bootstrap/network_filter_rbac.yaml` (**Create**).

**`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** — Modify. `## Network filters` section (starts `:229`).

---

## Task 1: Config surface — the `rbac` network filter (D1)

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (new const beside `:58`; one new `ConfigError` variant)
- Modify: `crates/envoy-config/src/bootstrap.rs` (`NetworkRbacConfig`; `TypedConfig::NetworkRbac` at `:674-686`; validation arm beside the `DIRECT_RESPONSE_FILTER` arm at `:3082-3094`)
- Test: `crates/envoy-config/src/bootstrap.rs` (`#[cfg(test)] mod tests`, beside the existing `DIRECT_RESPONSE_FILTER` parse tests at `:4924`, `:5021-5133`)

**Interfaces:**
- Consumes: nothing (first task).
- Produces:
  - `envoy_config::NETWORK_RBAC_FILTER: &str = "envoy.filters.network.rbac"`
  - `envoy_config::NetworkRbacConfig { pub stat_prefix: String, pub rules: Option<envoy_config::Rules> }`
  - `envoy_config::TypedConfig::NetworkRbac(NetworkRbacConfig)`
  - `envoy_config::ConfigError::EmptyNetworkRbacStatPrefix { listener: String }`
  - `bootstrap::validate_network_rbac_config(cfg: &NetworkRbacConfig, listener_name: &str) -> Result<(), ConfigError>` (private; Tasks 2 and 4 extend its body)

**Context an implementer needs.** `TypedConfig` is an `#[serde(tag = "@type", deny_unknown_fields)]` enum with three variants today (`TcpProxy`, `HttpConnectionManager`, `DirectResponse`). The per-filter validation loop is `for filter in &mut chain.filters { match filter.name.as_str() { … } }` at `bootstrap.rs:3030-3100`, whose final `_ =>` arm returns `ConfigError::UnsupportedFilter(filter.name.clone(), crate::ECHO_FILTER)` — so a new filter name is rejected until it gets an arm. `Rules` / `Policy` / `Permission` / `Principal` / `PermissionSet` / `PrincipalSet` already exist (`bootstrap.rs:1436-1700`), landed by the **HTTP** RBAC filter, and are reused verbatim. `Rules` derives `Clone`.

Note `stat_prefix` is REQUIRED by upstream's proto constraint (`RBACValidationError.StatPrefix: value length must be at least 1 characters`), so an **absent** `stat_prefix` is a serde missing-field error (surfacing as `ConfigError::Yaml`), while an **empty-string** `stat_prefix` needs the new explicit variant. Use `.is_empty()`, not `.trim().is_empty()` — upstream's `min_len 1` accepts `" "`.

- [ ] **Step 1: Write the failing tests**

Add to `crates/envoy-config/src/bootstrap.rs`'s `mod tests`:

```rust
/// 67.1 D1: a `[rbac, echo]` chain parses, and the rbac filter's typed_config
/// deserializes into `TypedConfig::NetworkRbac` with `rules` present.
#[test]
fn parses_network_rbac_filter_with_any_matcher() {
    let yaml = r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: rbac_probe
                rules:
                  action: DENY
                  policies:
                    p0:
                      permissions: [{ any: true }]
                      principals: [{ any: true }]
            - name: envoy.filters.network.echo
"#;
    let mut b: crate::Bootstrap = serde_yaml::from_str(yaml).expect("parses");
    crate::bootstrap::validate(&mut b).expect("validates");
    let f = &b.static_resources.listeners[0].filter_chains[0].filters[0];
    assert_eq!(f.name, crate::NETWORK_RBAC_FILTER);
    let Some(crate::TypedConfig::NetworkRbac(cfg)) = f.typed_config.as_ref() else {
        panic!("expected TypedConfig::NetworkRbac, got {:?}", f.typed_config);
    };
    assert_eq!(cfg.stat_prefix, "rbac_probe");
    let rules = cfg.rules.as_ref().expect("rules present");
    assert_eq!(rules.action, crate::Action::Deny);
    assert_eq!(rules.policies.len(), 1);
}

/// 67.1 D1 / SPEC R-3: `rules` is OPTIONAL. `stat_prefix` alone validates.
#[test]
fn parses_network_rbac_filter_with_rules_omitted() {
    let mut b = network_rbac_bootstrap(r#"stat_prefix: norules"#);
    crate::bootstrap::validate(&mut b).expect("rules is optional (SPEC R-3)");
    let f = &b.static_resources.listeners[0].filter_chains[0].filters[0];
    let Some(crate::TypedConfig::NetworkRbac(cfg)) = f.typed_config.as_ref() else {
        panic!("expected NetworkRbac");
    };
    assert!(cfg.rules.is_none(), "rules: None ⇒ INERT (SPEC R-4)");
}

/// 67.1 D1 / SPEC R-3: an EMPTY `stat_prefix` is rejected (upstream PGV min_len 1).
#[test]
fn rejects_network_rbac_with_empty_stat_prefix() {
    let mut b = network_rbac_bootstrap(r#"stat_prefix: """#);
    let err = crate::bootstrap::validate(&mut b).expect_err("empty stat_prefix rejected");
    assert!(
        matches!(err, crate::ConfigError::EmptyNetworkRbacStatPrefix { ref listener } if listener == "l0"),
        "got {err:?}",
    );
}

/// 67.1 D1: a MISSING `stat_prefix` is a serde missing-field error.
#[test]
fn rejects_network_rbac_with_missing_stat_prefix() {
    let yaml = network_rbac_yaml(r#"rules: { policies: {} }"#);
    let err = serde_yaml::from_str::<crate::Bootstrap>(&yaml)
        .expect_err("stat_prefix is required");
    assert!(err.to_string().contains("stat_prefix"), "got {err}");
}

/// 67.1 D1 / CF-67-1: `shadow_rules` is rejected LOUDLY by `deny_unknown_fields`.
#[test]
fn rejects_network_rbac_shadow_rules_field() {
    let yaml = network_rbac_yaml("stat_prefix: sp\n                shadow_rules: { policies: {} }");
    let err = serde_yaml::from_str::<crate::Bootstrap>(&yaml)
        .expect_err("shadow_rules is not modeled (CF-67-1)");
    assert!(err.to_string().contains("shadow_rules"), "got {err}");
}

/// 67.1 D1: the rbac filter without a typed_config is rejected.
#[test]
fn rejects_network_rbac_without_typed_config() {
    let yaml = r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
            - name: envoy.filters.network.echo
"#;
    let mut b: crate::Bootstrap = serde_yaml::from_str(yaml).expect("parses");
    let err = crate::bootstrap::validate(&mut b).expect_err("typed_config required");
    assert!(
        matches!(err, crate::ConfigError::MissingTypedConfig(crate::NETWORK_RBAC_FILTER)),
        "got {err:?}",
    );
}

/// 67.1 D1 / SPEC R-10: `rbac` is NON-TERMINAL — it must NOT be in
/// `is_terminal_network_filter`. Its ABSENCE from that predicate IS its
/// non-terminality. If a future edit adds it, `[rbac, echo]` starts failing
/// with `NetworkFilterNotTerminal` and this test catches it.
#[test]
fn network_rbac_is_not_a_terminal_filter() {
    assert!(!crate::bootstrap::is_terminal_network_filter(crate::NETWORK_RBAC_FILTER));
}

/// Build a bootstrap whose first filter is `rbac` with the given typed_config
/// body (already indented to 16 spaces), followed by a terminal `echo`.
fn network_rbac_yaml(typed_body: &str) -> String {
    format!(
        r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: 10000 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                {typed_body}
            - name: envoy.filters.network.echo
"#
    )
}

fn network_rbac_bootstrap(typed_body: &str) -> crate::Bootstrap {
    serde_yaml::from_str(&network_rbac_yaml(typed_body)).expect("parses")
}
```

`is_terminal_network_filter` is currently a private `fn`. Make it `pub(crate) fn` so the test can call it (it is already in the same crate; add `pub(crate)` to the signature at `bootstrap.rs:825`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config network_rbac 2>&1 | tee /tmp/t1.log`
Expected: compile error — `no variant named NetworkRbac`, `cannot find value NETWORK_RBAC_FILTER`, `no variant EmptyNetworkRbacStatPrefix`.

- [ ] **Step 3: Add the const and the `ConfigError` variant**

In `crates/envoy-config/src/lib.rs`, immediately after `DIRECT_RESPONSE_FILTER` (`:58`):

```rust
/// 67.1 (ADR-0128 / ADR-0129): the Network-filters family's FIRST NON-TERMINAL
/// filter. Deliberately ABSENT from `is_terminal_network_filter` — that absence
/// IS its non-terminality. NOT to be confused with `envoy.filters.http.rbac`
/// (`crates/envoy-filter/src/rbac.rs`), a different feature sharing the name.
pub const NETWORK_RBAC_FILTER: &str = "envoy.filters.network.rbac";
```

And, beside the other RBAC variants (`lib.rs:457-505`):

```rust
/// 67.1: a network `rbac` filter's `stat_prefix` is present but empty.
/// Upstream enforces `min_len 1` via a proto constraint
/// (`RBACValidationError.StatPrefix`). An ABSENT `stat_prefix` is a serde
/// missing-field error, not this variant.
#[error("listener {listener:?}: network rbac filter stat_prefix must be non-empty")]
EmptyNetworkRbacStatPrefix { listener: String },
```

- [ ] **Step 4: Add `NetworkRbacConfig`, the `TypedConfig` variant, and the validation arm**

In `crates/envoy-config/src/bootstrap.rs`, beside `RbacConfig` (`:1436`):

```rust
/// 67.1 D1: `envoy.extensions.filters.network.rbac.v3.RBAC` — the NETWORK
/// (L4) RBAC filter. Distinct from `RbacConfig` above, which is the HTTP
/// filter's config (`envoy.filters.http.rbac`); the two share the `Rules` /
/// `Policy` / `Permission` / `Principal` trees but nothing else.
///
/// `stat_prefix` is REQUIRED and non-empty (upstream proto constraint; SPEC
/// R-3). `rules` is OPTIONAL, and `None` means the filter is **INERT** — the
/// connection is allowed and NEITHER counter increments (SPEC R-4, measured).
/// Do NOT materialise a default `Rules { action: ALLOW }`: that would tick
/// `allowed` and produce a stat divergence with no body divergence.
///
/// `deny_unknown_fields` is what rejects `shadow_rules` /
/// `shadow_rules_stat_prefix` loudly (CF-67-1).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NetworkRbacConfig {
    pub stat_prefix: String,
    #[serde(default)]
    pub rules: Option<Rules>,
}
```

Add the `TypedConfig` variant (`:674-686`):

```rust
    /// 67.1 (ADR-0128/0129): the Network-filters family's first non-terminal filter.
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC")]
    NetworkRbac(NetworkRbacConfig),
```

Add a validation arm to the per-filter match, immediately after the `DIRECT_RESPONSE_FILTER` arm (`:3094`):

```rust
                    crate::NETWORK_RBAC_FILTER => {
                        // 67.1 D1: read-only — the rbac arm does not mutate
                        // typed_config, so `as_ref()` (not `as_mut()`).
                        let typed = filter.typed_config.as_ref().ok_or(
                            crate::ConfigError::MissingTypedConfig(crate::NETWORK_RBAC_FILTER),
                        )?;
                        let TypedConfig::NetworkRbac(cfg) = typed else {
                            return Err(crate::ConfigError::MissingTypedConfig(
                                crate::NETWORK_RBAC_FILTER,
                            ));
                        };
                        validate_network_rbac_config(cfg, &listener.name)?;
                    }
```

And the validator itself, beside `validate_rbac_config` (`:3799`). **Task 4 extends this body with the L4 leaf walk; Task 2 replaces the `validate_rbac_config` call with `validate_rbac_rules`.** Write it in its Task-1 shape now:

```rust
/// 67.1 D1: validate one NETWORK `rbac` filter config.
///   - `stat_prefix` non-empty (SPEC R-3);
///   - `rules: None` ⇒ INERT, nothing more to check (SPEC R-4);
///   - `rules: Some(_)` ⇒ the shared RBAC tree validations (empty sets, depth).
fn validate_network_rbac_config(
    cfg: &crate::NetworkRbacConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if cfg.stat_prefix.is_empty() {
        return Err(crate::ConfigError::EmptyNetworkRbacStatPrefix {
            listener: listener_name.to_string(),
        });
    }
    let Some(rules) = cfg.rules.as_ref() else {
        return Ok(()); // SPEC R-4: rules omitted ⇒ INERT.
    };
    validate_rbac_config(&crate::RbacConfig { rules: rules.clone() }, listener_name)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-config network_rbac 2>&1 | tee /tmp/t1.log`
Expected: `7 passed`. Then the whole crate: `cargo test -p envoy-config 2>&1 | tail -5` → **no regressions** (the pre-existing count is `548 passed`; expect `555 passed`).

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p envoy-config --all-targets -- -D warnings
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 67.1 task 1: envoy.filters.network.rbac config surface (D1)"
```

---

## Task 2: W-1 — generalize the six RBAC error prefixes; extract `validate_rbac_rules`

**Files:**
- Modify: `crates/envoy-config/src/lib.rs:457-505` (six `#[error(...)]` attributes)
- Modify: `crates/envoy-config/src/bootstrap.rs` (`validate_rbac_config` at `:3799`; `validate_network_rbac_config` from Task 1)
- Test: `crates/envoy-config/src/bootstrap.rs` (`mod tests`)

**Interfaces:**
- Consumes: Task 1's `validate_network_rbac_config`.
- Produces: `bootstrap::validate_rbac_rules(rules: &Rules, listener_name: &str) -> Result<(), ConfigError>` (private).

**Context.** Task 1's `validate_network_rbac_config` clones `Rules` into a throwaway `RbacConfig` purely to reach the shared validation. That is wasteful and couples the network filter to the HTTP filter's config struct. Extract the body.

The six variants render `"HCM listener {listener:?}: …"`. A network `rbac` filter has no HCM. Generalize the prefix to `"listener {listener:?}: …"` on **exactly these six**: `EmptyRbacPolicies`, `EmptyRbacPolicyPermissions`, `EmptyRbacPolicyPrincipals`, `EmptyRbacPermissionSet`, `EmptyRbacPrincipalSet`, `RbacTreeTooDeep`. Leave the other twelve `"HCM listener"` variants (`TokenBucketMaxTokensMustBePositive`, `InvalidTokenBucketFillInterval`, `UnsupportedLocalRateLimitStatusCode`, `RbacMetadataMatcherInvalid`, …) untouched — they are genuinely HCM-scoped. `RbacMetadataMatcherInvalid` is unreachable from a network `rbac` filter because Task 4 rejects `metadata` leaves outright.

**Blast radius is zero** and was measured: every landed assertion over the six is `matches!`-shaped, never a string comparison. §7.4 permits differing error text.

- [ ] **Step 1: Write the failing test**

```rust
/// 67.1 W-1 (ADR-0130): the six RBAC tree/empty-set errors are shared between
/// the HTTP filter and the NETWORK filter, so their message must NOT claim
/// "HCM listener" — a network `rbac` filter has no HCM. §7.4 permits differing
/// error text between the proxies, so this is an internal-quality guarantee.
#[test]
fn shared_rbac_errors_do_not_claim_hcm_scope() {
    let rendered = [
        crate::ConfigError::EmptyRbacPolicies { listener: "l0".into() }.to_string(),
        crate::ConfigError::EmptyRbacPolicyPermissions {
            listener: "l0".into(), policy_name: "p".into(),
        }.to_string(),
        crate::ConfigError::EmptyRbacPolicyPrincipals {
            listener: "l0".into(), policy_name: "p".into(),
        }.to_string(),
        crate::ConfigError::EmptyRbacPermissionSet {
            listener: "l0".into(), policy_name: "p".into(), path: "permissions[0]".into(),
        }.to_string(),
        crate::ConfigError::EmptyRbacPrincipalSet {
            listener: "l0".into(), policy_name: "p".into(), path: "principals[0]".into(),
        }.to_string(),
        crate::ConfigError::RbacTreeTooDeep {
            listener: "l0".into(), policy_name: "p".into(), depth: 17,
        }.to_string(),
    ];
    for msg in rendered {
        assert!(!msg.contains("HCM listener"), "leaked HCM scope: {msg}");
        assert!(msg.starts_with(r#"listener "l0""#), "unexpected prefix: {msg}");
    }
}

/// 67.1 W-1: a network rbac filter with an empty `policies` map surfaces the
/// SHARED `EmptyRbacPolicies` error, now correctly scoped.
#[test]
fn network_rbac_empty_policies_uses_shared_error() {
    let mut b = network_rbac_bootstrap("stat_prefix: sp\n                rules: { policies: {} }");
    let err = crate::bootstrap::validate(&mut b).expect_err("empty policies rejected");
    assert!(
        matches!(err, crate::ConfigError::EmptyRbacPolicies { ref listener } if listener == "l0"),
        "got {err:?}",
    );
    assert!(!err.to_string().contains("HCM"), "got {err}");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p envoy-config shared_rbac_errors_do_not_claim_hcm_scope network_rbac_empty_policies 2>&1 | tee /tmp/t2.log`
Expected: FAIL — `leaked HCM scope: HCM listener "l0": RBAC filter has no policies (rules.policies is empty)`.

- [ ] **Step 3: Generalize the six `#[error]` attributes**

In `crates/envoy-config/src/lib.rs`, replace the literal `HCM listener {listener:?}: ` with `listener {listener:?}: ` in exactly these six attributes. For example:

```rust
    /// 10: RBAC filter has no policies (rules.policies is empty).
    /// 67.1 (ADR-0130): message generalized — this error is shared with the
    /// NETWORK rbac filter, which has no HCM.
    #[error("listener {listener:?}: RBAC filter has no policies (rules.policies is empty)")]
    EmptyRbacPolicies { listener: String },
```

Verify none were missed:

```bash
grep -n 'HCM listener' crates/envoy-config/src/lib.rs | grep -i rbac
```
Expected: exactly ONE remaining line — `RbacMetadataMatcherInvalid` (genuinely HCM-scoped; unreachable from network rbac).

- [ ] **Step 4: Extract `validate_rbac_rules` and call it from both sites**

In `crates/envoy-config/src/bootstrap.rs`, rewrite `validate_rbac_config` as a thin wrapper and hoist its body:

```rust
/// Validate an RBAC policy tree. SHARED by the HTTP filter
/// (`validate_rbac_config`) and the NETWORK filter
/// (`validate_network_rbac_config`, 67.1 D1). Phase 10 (SPEC §3 D2):
///   - rules.policies non-empty
///   - per-policy permissions + principals non-empty
///   - recursive: empty AndRules/OrRules/AndIds/OrIds rejected
///   - recursive: depth ≤ RBAC_TREE_MAX_DEPTH
///
/// Its six `ConfigError` variants are scope-neutral (`listener {listener:?}`,
/// not `HCM listener`) precisely because both filters raise them — ADR-0130.
fn validate_rbac_rules(
    rules: &crate::Rules,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if rules.policies.is_empty() {
        return Err(crate::ConfigError::EmptyRbacPolicies {
            listener: listener_name.to_string(),
        });
    }
    for (policy_name, policy) in rules.policies.iter() {
        if policy.permissions.is_empty() {
            return Err(crate::ConfigError::EmptyRbacPolicyPermissions {
                listener: listener_name.to_string(),
                policy_name: policy_name.clone(),
            });
        }
        if policy.principals.is_empty() {
            return Err(crate::ConfigError::EmptyRbacPolicyPrincipals {
                listener: listener_name.to_string(),
                policy_name: policy_name.clone(),
            });
        }
        for (idx, perm) in policy.permissions.iter().enumerate() {
            validate_permission_tree(perm, listener_name, policy_name, &format!("permissions[{idx}]"), 1)?;
        }
        for (idx, prin) in policy.principals.iter().enumerate() {
            validate_principal_tree(prin, listener_name, policy_name, &format!("principals[{idx}]"), 1)?;
        }
    }
    Ok(())
}

/// Validate one HTTP RBAC filter config (`envoy.filters.http.rbac`).
fn validate_rbac_config(
    cfg: &crate::RbacConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    validate_rbac_rules(&cfg.rules, listener_name)
}
```

Then replace the final line of Task 1's `validate_network_rbac_config` — dropping the throwaway clone:

```rust
    validate_rbac_rules(rules, listener_name)
```

- [ ] **Step 5: Run to verify pass, and prove no regression**

```bash
cargo test -p envoy-config 2>&1 | tail -5
```
Expected: `557 passed; 0 failed`. Zero `HCM listener` assertions existed, so no existing test changes.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p envoy-config --all-targets -- -D warnings
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 67.1 task 2: W-1 scope-neutral RBAC error text; extract validate_rbac_rules [ADR-0130]"
```

---

## Task 3: The bilateral chain-termination rule (D2) — and M66-6's LDS test

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (one new `ConfigError` variant, beside `NetworkFilterNotTerminal` at `:87-91`)
- Modify: `crates/envoy-config/src/bootstrap.rs:2971` (the `for chain in &mut listener.filter_chains` loop header) and `:3020-3029` (the phase-66 immutable pre-pass)
- Test: `crates/envoy-config/src/bootstrap.rs` (`mod tests`)

**Interfaces:**
- Consumes: Task 1's `NETWORK_RBAC_FILTER` (the only non-terminal filter available to violate the rule with).
- Produces: `ConfigError::NetworkFilterChainNotTerminated { listener: String, chain_index: usize, last_filter: String }`.

**Context — the three rules and their precedence.** Upstream Envoy `--mode validate`, measured (SPEC R-1, R-5, R-7):

| Chain | Upstream Envoy |
|---|---|
| `[rbac, echo]` | `configuration OK` — rbac is non-terminal |
| `[echo, rbac]` | `terminal filter named envoy.filters.network.echo … must be the last filter` |
| `[rbac]` alone | `non-terminal filter named envoy.filters.network.rbac … is the last filter in a network filter chain` |
| `filters: []` | `configuration OK` — **the empty chain is ACCEPTED** |

`[echo, rbac]` violates **both** rules at once and reports the **terminal-not-last** one. An in-order scan reproduces this for free: `echo` at index 0 trips the terminal rule before the chain-termination rule is ever consulted. **So the new check goes AFTER the existing loop, never before it.**

The empty chain stays ACCEPTED (R-7) — this is measured parity, and it is what CLOSED **M66-5**. `chain.filters.last()` returning `None` gives that for free. **Do not add an emptiness check.**

The enclosing loop at `bootstrap.rs:2966-2971` iterates `static_listeners.chain(dynamic_listeners.iter_mut().flatten())`, so the rule automatically applies to **LDS-loaded dynamic listeners** too. **M66-6** is the missing *test* of that, on the very pre-pass this task edits. It is folded in here (Step 1's last test) — cheap, and it closes the carry-forward.

- [ ] **Step 1: Write the failing tests**

```rust
/// 67.1 D2 / SPEC R-1: a chain whose LAST filter is non-terminal is REJECTED.
/// The bilateral dual of `NetworkFilterNotTerminal`.
#[test]
fn rejects_chain_whose_last_filter_is_non_terminal() {
    let yaml = r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: sp
"#;
    let mut b: crate::Bootstrap = serde_yaml::from_str(yaml).expect("parses");
    let err = crate::bootstrap::validate(&mut b).expect_err("[rbac] alone must be rejected");
    assert!(
        matches!(
            err,
            crate::ConfigError::NetworkFilterChainNotTerminated {
                ref listener, chain_index: 0, ref last_filter,
            } if listener == "l0" && last_filter == crate::NETWORK_RBAC_FILTER
        ),
        "got {err:?}",
    );
}

/// 67.1 D2 / SPEC R-5: ERROR PRECEDENCE. `[echo, rbac]` violates BOTH rules
/// (echo is terminal-but-not-last; rbac is non-terminal-but-last). Upstream
/// reports the TERMINAL error. An in-order scan reproduces that naturally.
/// If a future edit moves the chain-termination check BEFORE the terminal
/// scan, this test catches it.
#[test]
fn terminal_not_last_error_wins_over_chain_not_terminated() {
    let yaml = r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: sp
"#;
    let mut b: crate::Bootstrap = serde_yaml::from_str(yaml).expect("parses");
    let err = crate::bootstrap::validate(&mut b).expect_err("rejected");
    assert!(
        matches!(err, crate::ConfigError::NetworkFilterNotTerminal { ref name, .. }
                 if name == crate::ECHO_FILTER),
        "terminal-not-last must WIN; got {err:?}",
    );
}

/// 67.1 D2 / SPEC R-7: an EMPTY `filters: []` chain is ACCEPTED — measured
/// parity with upstream Envoy (`configuration OK`). This CLOSES M66-5. The
/// chain-termination rule applies only to NON-EMPTY chains.
#[test]
fn empty_filter_chain_is_accepted() {
    let yaml = r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters: []
"#;
    let mut b: crate::Bootstrap = serde_yaml::from_str(yaml).expect("parses");
    crate::bootstrap::validate(&mut b).expect("empty chain is upstream parity (SPEC R-7)");
}

/// 67.1 D2 / SPEC R-1: `[rbac, echo]` — the happy chain — validates.
#[test]
fn accepts_non_terminal_rbac_before_terminal_echo() {
    let mut b = network_rbac_bootstrap("stat_prefix: sp");
    crate::bootstrap::validate(&mut b).expect("[rbac, echo] is valid (SPEC R-1)");
}

/// 67.1 D2: every existing single-terminal-filter chain still validates.
#[test]
fn single_terminal_filter_chains_still_validate() {
    for name in [
        crate::ECHO_FILTER,
        crate::DIRECT_RESPONSE_FILTER,
    ] {
        let typed = if name == crate::DIRECT_RESPONSE_FILTER {
            "\n              typed_config:\n                \"@type\": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config\n                response: { inline_string: \"x\" }"
        } else {
            ""
        };
        let yaml = format!(
            r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: 10000 }}
      filter_chains:
        - filters:
            - name: {name}{typed}
"#
        );
        let mut b: crate::Bootstrap = serde_yaml::from_str(&yaml).expect("parses");
        crate::bootstrap::validate(&mut b).unwrap_or_else(|e| panic!("{name} chain must validate: {e}"));
    }
}

/// 67.1 D2 — closes M66-6. The pre-pass iterates
/// `static_listeners.chain(dynamic_listeners)`, so BOTH terminal rules apply
/// to LDS-loaded dynamic listeners. Phase 66 landed the terminal rule with no
/// dynamic-listener test; this is it.
#[test]
fn network_filter_terminal_rules_apply_to_dynamic_lds_listeners() {
    let listener_yaml = r#"
name: dyn0
address:
  socket_address: { address: 127.0.0.1, port_value: 10000 }
filter_chains:
  - filters:
      - name: envoy.filters.network.rbac
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
          stat_prefix: sp
"#;
    let dynamic: crate::Listener = serde_yaml::from_str(listener_yaml).expect("parses");
    let mut b: crate::Bootstrap = serde_yaml::from_str(
        "static_resources: { listeners: [] }",
    )
    .expect("parses");
    b.dynamic_listeners = Some(vec![dynamic]);
    let err = crate::bootstrap::validate(&mut b)
        .expect_err("a dynamic listener's [rbac]-only chain must be rejected too");
    assert!(
        matches!(err, crate::ConfigError::NetworkFilterChainNotTerminated { ref listener, .. }
                 if listener == "dyn0"),
        "got {err:?}",
    );
}
```

> If `Bootstrap::dynamic_listeners` is not directly assignable from a test (check its visibility at `bootstrap.rs`), build the dynamic listener through the same helper `xds_file_based_lds.rs` uses, or mark the field `pub(crate)`. Do **not** weaken the assertion.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p envoy-config chain_not_terminated terminal_not_last empty_filter_chain non_terminal_rbac dynamic_lds 2>&1 | tee /tmp/t3.log`
Expected: compile error — `no variant NetworkFilterChainNotTerminated`; then `[rbac] alone must be rejected` FAILS (validate returns `Ok`).

- [ ] **Step 3: Add the `ConfigError` variant**

In `crates/envoy-config/src/lib.rs`, immediately after `NetworkFilterNotTerminal` (`:91`):

```rust
    /// 67.1 D2 (ADR-0128 / ADR-0129): a NON-EMPTY network filter chain whose
    /// LAST filter is non-terminal. The bilateral dual of
    /// `NetworkFilterNotTerminal`. Upstream Envoy: `non-terminal filter named
    /// <X> of type <X> is the last filter in a network filter chain.` (SPEC R-1)
    ///
    /// An EMPTY `filters: []` chain stays ACCEPTED — measured upstream parity
    /// (SPEC R-7), which is what closed carry-forward M66-5. This variant is
    /// unreachable for an empty chain by construction (`filters.last()` is None).
    #[error(
        "listener {listener:?} filter_chains[{chain_index}]: non-terminal filter {last_filter:?} is the last filter in a network filter chain"
    )]
    NetworkFilterChainNotTerminated {
        listener: String,
        chain_index: usize,
        last_filter: String,
    },
```

- [ ] **Step 4: Extend the pre-pass**

At `bootstrap.rs:2971`, add the index:

```rust
        for (chain_index, chain) in listener.filter_chains.iter_mut().enumerate() {
```

Then, in the phase-66 immutable pre-pass (`:3020-3029`), **after** the existing in-order terminal scan and **before** the mutating `for filter in &mut chain.filters` loop:

```rust
            let chain_len = chain.filters.len();
            for (idx, filter) in chain.filters.iter().enumerate() {
                if is_terminal_network_filter(&filter.name) && idx + 1 != chain_len {
                    return Err(crate::ConfigError::NetworkFilterNotTerminal {
                        name: filter.name.clone(),
                        position: idx + 1,
                        chain_len,
                    });
                }
            }
            // 67.1 D2 (SPEC R-1): the BILATERAL dual — a non-empty chain must
            // END in a terminal filter. Placed AFTER the scan above so the
            // terminal-not-last error WINS when a chain violates both rules at
            // once (`[echo, rbac]`) — measured upstream precedence, SPEC R-5.
            // An empty `filters: []` chain is ACCEPTED (SPEC R-7, upstream
            // parity, closes M66-5): `last()` is None and this check no-ops.
            if let Some(last) = chain.filters.last()
                && !is_terminal_network_filter(&last.name)
            {
                return Err(crate::ConfigError::NetworkFilterChainNotTerminated {
                    listener: listener.name.clone(),
                    chain_index,
                    last_filter: last.name.clone(),
                });
            }
```

- [ ] **Step 5: Run to verify pass, plus full-crate no-regression**

```bash
cargo test -p envoy-config 2>&1 | tail -5
```
Expected: `563 passed; 0 failed`. **If any pre-existing test now fails, it is configuring a chain that does not end in a terminal filter — read it before touching it.** No landed fixture does (every chain today is a single terminal filter).

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p envoy-config --all-targets -- -D warnings
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 67.1 task 3: bilateral network-filter chain-termination rule (D2); closes M66-6"
```

---

## Task 4: The CF-67-4 L4 leaf allow-list (D3)

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (one new `ConfigError` variant)
- Modify: `crates/envoy-config/src/bootstrap.rs` (`validate_network_rbac_config`; two new recursive walks beside `define_rbac_tree_validator!` at `:4087-4172`)
- Test: `crates/envoy-config/src/bootstrap.rs` (`mod tests`)

**Interfaces:**
- Consumes: Task 2's `validate_rbac_rules`; Task 1's `validate_network_rbac_config`.
- Produces: `ConfigError::UnsupportedNetworkRbacMatcher { listener: String, policy_name: String, arm: &'static str, path: String }`.

**Context — why this is in `67.1` and not `67.2`.** `67.1` reuses the existing `Permission` / `Principal` enums, whose arms today are `any` / `header` / `and_*` / `or_*` / `not_*` / `metadata` / `url_path`. Without a validation walk, envoy-rust would **ACCEPT** `[rbac(permissions:[{header:…}]), echo]` — a config upstream Envoy **REJECTS** (`error initializing configuration: Found header(name: ":path"…`, measured). That is a config-load divergence, and it would sit in `main` for the entire interval between the sub-phases. ADR-0129 moved CF-67-4 here for exactly this reason.

Measured verdicts (SPEC R-6), and envoy-rust's posture:

| Arm | Upstream Envoy (L4) | envoy-rust `67.1` | Why |
|---|---|---|---|
| `any` | accepts | **accepts** | parity |
| `and_rules`/`or_rules`/`not_rule`, `and_ids`/`or_ids`/`not_id` | accepts | **accepts** (recurse) | parity |
| `header` | **REJECTS** | **rejects** | **parity** |
| `url_path` | *accepts* (can never match at L4) | **rejects** | deliberate **fail-loud** divergence, ADR-0049 decision-2 (b) |
| `metadata` | *accepts* (can never match at L4) | **rejects** | same |

**No differential observable** — neither fixture uses them. Recorded in `BEHAVIOR_CONTRACT.md` (Task 14), never silent.

**Why not reuse `define_rbac_tree_validator!`.** That macro (`:4087-4148`) is instantiated for **both** enums and its shared body already matches all seven arms with `Ok(())` verdicts for `Any`/`Header`/`UrlPath`. The L4 verdicts differ per-arm and per-enum-field-name (`rules` vs `ids`), so the two walks are written by hand. They are ~25 lines each.

**Ordering is load-bearing.** `validate_rbac_rules` runs FIRST (it bounds tree depth at `RBAC_TREE_MAX_DEPTH = 16`); the L4 walk then recurses over a tree already proven shallow, so it cannot blow the stack. **`67.2` widens this allow-list**; the `header`/`url_path`/`metadata` rejections stay forever.

- [ ] **Step 1: Write the failing tests**

```rust
/// 67.1 D3 / CF-67-4 / SPEC R-6: a `header` matcher in a NETWORK rbac config is
/// REJECTED — PARITY with upstream Envoy, which rejects it at config load.
#[test]
fn network_rbac_rejects_header_permission() {
    let mut b = network_rbac_bootstrap(
        "stat_prefix: sp\n                rules:\n                  action: ALLOW\n                  policies:\n                    p0:\n                      permissions: [{ header: { name: \":path\", exact_match: \"/x\" } }]\n                      principals: [{ any: true }]",
    );
    let err = crate::bootstrap::validate(&mut b).expect_err("header rejected at L4");
    assert!(
        matches!(err, crate::ConfigError::UnsupportedNetworkRbacMatcher {
            ref policy_name, arm: "header", ref path, ..
        } if policy_name == "p0" && path == "permissions[0]"),
        "got {err:?}",
    );
}

/// 67.1 D3 / CF-67-4 / SPEC R-6: `url_path` is a deliberate FAIL-LOUD divergence
/// (ADR-0049 decision-2 (b)) — upstream ACCEPTS it, but it can never match at L4.
#[test]
fn network_rbac_rejects_url_path_principal_fail_loud() {
    let mut b = network_rbac_bootstrap(
        "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{ any: true }]\n                      principals: [{ url_path: { path: { exact: \"/x\" } } }]",
    );
    let err = crate::bootstrap::validate(&mut b).expect_err("url_path rejected at L4");
    assert!(
        matches!(err, crate::ConfigError::UnsupportedNetworkRbacMatcher {
            arm: "url_path", ref path, ..
        } if path == "principals[0]"),
        "got {err:?}",
    );
}

/// 67.1 D3 / CF-67-4: `metadata` — same fail-loud posture.
#[test]
fn network_rbac_rejects_metadata_permission_fail_loud() {
    let mut b = network_rbac_bootstrap(
        "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{ metadata: { filter: f, path: [{ key: k }], value: { string_match: { exact: v } } } }]\n                      principals: [{ any: true }]",
    );
    let err = crate::bootstrap::validate(&mut b).expect_err("metadata rejected at L4");
    assert!(
        matches!(err, crate::ConfigError::UnsupportedNetworkRbacMatcher { arm: "metadata", .. }),
        "got {err:?}",
    );
}

/// 67.1 D3: the walk RECURSES — a rejected leaf nested under combinators is
/// still caught, and the reported `path` names the exact position.
#[test]
fn network_rbac_rejects_header_nested_under_combinators() {
    let mut b = network_rbac_bootstrap(
        "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions:\n                        - not_rule:\n                            or_rules:\n                              rules:\n                                - any: true\n                                - header: { name: x, exact_match: y }\n                      principals: [{ any: true }]",
    );
    let err = crate::bootstrap::validate(&mut b).expect_err("nested header rejected");
    assert!(
        matches!(err, crate::ConfigError::UnsupportedNetworkRbacMatcher {
            arm: "header", ref path, ..
        } if path == "permissions[0].not_rule.rules[1]"),
        "got {err:?}",
    );
}

/// 67.1 D3: `any` + every combinator is ACCEPTED, arbitrarily nested. This is
/// the whole matcher surface `67.1` ships; `67.2` widens it.
#[test]
fn network_rbac_accepts_any_and_all_combinators() {
    let mut b = network_rbac_bootstrap(
        "stat_prefix: sp\n                rules:\n                  action: DENY\n                  policies:\n                    p0:\n                      permissions:\n                        - and_rules:\n                            rules:\n                              - any: true\n                              - not_rule: { any: false }\n                      principals:\n                        - or_ids:\n                            ids:\n                              - any: true\n                              - not_id: { any: false }",
    );
    crate::bootstrap::validate(&mut b).expect("any + combinators are the 67.1 surface");
}

/// 67.1 D3: the `67.2` arms do NOT exist — they are rejected as UNKNOWN KEYS by
/// the hand-rolled `impl_single_key_oneof!` deserializer, not stubbed
/// (BOOTSTRAP_PROMPT.md §6.3). This test pins that they cannot silently appear.
#[test]
fn network_rbac_connection_matcher_arms_do_not_exist_yet() {
    for arm in ["direct_remote_ip", "remote_ip", "source_ip"] {
        let yaml = network_rbac_yaml(&format!(
            "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{{ any: true }}]\n                      principals: [{{ {arm}: {{ address_prefix: 1.2.3.4, prefix_len: 32 }} }}]"
        ));
        let err = serde_yaml::from_str::<crate::Bootstrap>(&yaml)
            .expect_err("{arm} is a 67.2 arm and must not deserialize");
        assert!(err.to_string().contains(arm), "got {err}");
    }
}

/// 67.1 D3: depth is bounded BEFORE the L4 walk recurses, so `RbacTreeTooDeep`
/// wins over `UnsupportedNetworkRbacMatcher` on a deep tree with a bad leaf.
/// This pins the ordering that keeps the L4 walk stack-safe.
#[test]
fn network_rbac_depth_bound_precedes_the_l4_walk() {
    let mut inner = String::from("{ header: { name: x, exact_match: y } }");
    for _ in 0..20 {
        inner = format!("{{ not_rule: {inner} }}");
    }
    let mut b = network_rbac_bootstrap(&format!(
        "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{inner}]\n                      principals: [{{ any: true }}]"
    ));
    let err = crate::bootstrap::validate(&mut b).expect_err("too deep");
    assert!(
        matches!(err, crate::ConfigError::RbacTreeTooDeep { .. }),
        "depth bound must run first; got {err:?}",
    );
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p envoy-config network_rbac 2>&1 | tee /tmp/t4.log`
Expected: compile error — `no variant UnsupportedNetworkRbacMatcher`; the accept-tests pass, the reject-tests fail with `expect_err` on an `Ok`.

- [ ] **Step 3: Add the `ConfigError` variant**

```rust
    /// 67.1 D3 (CF-67-4, ADR-0129): a NETWORK `rbac` Permission/Principal leaf
    /// that an L4 filter cannot evaluate.
    ///
    /// `header` is rejected in PARITY with upstream Envoy, which rejects it at
    /// config load (`Found header(name: ":path"…`, SPEC R-6, measured).
    /// `url_path` and `metadata` are rejected as a deliberate FAIL-LOUD
    /// divergence (ADR-0049 decision-2 (b)): upstream ACCEPTS a matcher that can
    /// never match at L4. No differential observable — neither fixture uses them.
    ///
    /// `67.2` WIDENS the allow-list to admit the connection-level arms. These
    /// three rejections stay permanently.
    #[error(
        "listener {listener:?}: network rbac policy {policy_name:?} uses matcher {arm:?} at {path}, which cannot be evaluated at L4"
    )]
    UnsupportedNetworkRbacMatcher {
        listener: String,
        policy_name: String,
        arm: &'static str,
        path: String,
    },
```

(`&'static str` in a `ConfigError` field has precedent: `UnsupportedFilter(String, &'static str)`.)

- [ ] **Step 4: Write the two walks and call them**

In `crates/envoy-config/src/bootstrap.rs`, after the two `define_rbac_tree_validator!` invocations (`:4150-4172`):

```rust
/// 67.1 D3 (CF-67-4): reject every `Permission` leaf a NETWORK (L4) rbac filter
/// cannot evaluate. `any` is admitted; `and_rules` / `or_rules` / `not_rule`
/// recurse. `header` (parity), `url_path` and `metadata` (fail-loud) are rejected.
///
/// EXHAUSTIVE, with no `_ =>` catch-all: when `67.2` adds the connection-level
/// arms, this function MUST fail to compile until they are classified. Never
/// add a catch-all here.
///
/// Depth is NOT re-checked: `validate_rbac_rules` runs first and bounds the tree
/// at `RBAC_TREE_MAX_DEPTH`, so this recursion is stack-safe by construction.
fn validate_l4_permission(
    node: &crate::Permission,
    listener_name: &str,
    policy_name: &str,
    path: &str,
) -> Result<(), crate::ConfigError> {
    let reject = |arm: &'static str| {
        Err(crate::ConfigError::UnsupportedNetworkRbacMatcher {
            listener: listener_name.to_string(),
            policy_name: policy_name.to_string(),
            arm,
            path: path.to_string(),
        })
    };
    match node {
        crate::Permission::Any(_) => Ok(()),
        crate::Permission::Header(_) => reject("header"),
        crate::Permission::Metadata(_) => reject("metadata"),
        crate::Permission::UrlPath(_) => reject("url_path"),
        crate::Permission::AndRules(set) | crate::Permission::OrRules(set) => {
            for (idx, child) in set.rules.iter().enumerate() {
                validate_l4_permission(child, listener_name, policy_name, &format!("{path}.rules[{idx}]"))?;
            }
            Ok(())
        }
        crate::Permission::NotRule(child) => {
            validate_l4_permission(child, listener_name, policy_name, &format!("{path}.not_rule"))
        }
    }
}

/// 67.1 D3 (CF-67-4): the `Principal` twin of `validate_l4_permission`. The
/// set-wrapper field is `ids` (not `rules`) and the negation arm is `not_id`,
/// per the upstream proto — which is exactly why the shared
/// `define_rbac_tree_validator!` macro cannot express these verdicts.
/// EXHAUSTIVE, no catch-all: see `validate_l4_permission`.
fn validate_l4_principal(
    node: &crate::Principal,
    listener_name: &str,
    policy_name: &str,
    path: &str,
) -> Result<(), crate::ConfigError> {
    let reject = |arm: &'static str| {
        Err(crate::ConfigError::UnsupportedNetworkRbacMatcher {
            listener: listener_name.to_string(),
            policy_name: policy_name.to_string(),
            arm,
            path: path.to_string(),
        })
    };
    match node {
        crate::Principal::Any(_) => Ok(()),
        crate::Principal::Header(_) => reject("header"),
        crate::Principal::Metadata(_) => reject("metadata"),
        crate::Principal::UrlPath(_) => reject("url_path"),
        crate::Principal::AndIds(set) | crate::Principal::OrIds(set) => {
            for (idx, child) in set.ids.iter().enumerate() {
                validate_l4_principal(child, listener_name, policy_name, &format!("{path}.ids[{idx}]"))?;
            }
            Ok(())
        }
        crate::Principal::NotId(child) => {
            validate_l4_principal(child, listener_name, policy_name, &format!("{path}.not_id"))
        }
    }
}
```

> The nested-path test expects `permissions[0].not_rule.rules[1]` — that is `not_rule` → `or_rules`'s `rules[1]`. The `AndRules | OrRules` arm emits `.rules[{idx}]` for both, matching the `PermissionSet.rules` field name.

Then extend `validate_network_rbac_config`'s tail (from Task 1 / Task 2):

```rust
    validate_rbac_rules(rules, listener_name)?;
    // 67.1 D3 (CF-67-4): the L4 leaf allow-list. Runs AFTER the shared tree
    // validation, which bounds depth at RBAC_TREE_MAX_DEPTH — so these
    // recursions are stack-safe. `67.2` widens the allow-list.
    for (policy_name, policy) in rules.policies.iter() {
        for (idx, perm) in policy.permissions.iter().enumerate() {
            validate_l4_permission(perm, listener_name, policy_name, &format!("permissions[{idx}]"))?;
        }
        for (idx, prin) in policy.principals.iter().enumerate() {
            validate_l4_principal(prin, listener_name, policy_name, &format!("principals[{idx}]"))?;
        }
    }
    Ok(())
```

- [ ] **Step 5: Run to verify pass**

```bash
cargo test -p envoy-config 2>&1 | tail -5
```
Expected: `570 passed; 0 failed`. **CF-67-4 is now CLOSED.**

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p envoy-config --all-targets -- -D warnings
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 67.1 task 4: CF-67-4 L4 leaf allow-list for network rbac (D3)"
```

---

## Task 5: The network-filter iteration protocol (D4, part 1) — **CONSUMES CF-66-2**

**Files:**
- Modify: `crates/envoy-listener/Cargo.toml` (`tokio` features gain `"io-util"`)
- Modify: `crates/envoy-listener/src/lib.rs` (new items after the `ConnectionHandler` trait at `:38-43`)
- Test: `crates/envoy-listener/src/lib.rs` (`#[cfg(test)] mod tests`, `:764+`)

**Interfaces:**
- Consumes: nothing from Tasks 1-4.
- Produces:
  - `envoy_listener::NetworkFilterStatus { Continue, StopIteration }` (derives `Debug, Clone, Copy, PartialEq, Eq`)
  - `envoy_listener::ConnectionInfo { pub peer_addr: SocketAddr, pub local_addr: SocketAddr }` (derives `Debug, Clone, Copy, PartialEq, Eq`)
  - `envoy_listener::NetworkFilter: Send + Sync + 'static` with `fn on_new_connection(&self, conn: &ConnectionInfo) -> NetworkFilterStatus`
  - `envoy_listener::close_with_drain(stream: tokio::net::TcpStream) -> Result<(), std::io::Error>`

**Context — this task IS carry-forward CF-66-2.** ADR-0123 §2.2 deferred "a generic network-filter chain iteration protocol" with an explicit trigger: *"becomes necessary only alongside the first non-terminal network filter (`sni_cluster`, network `rbac`)."* That trigger has fired.

**`on_new_connection` is the ONLY hook.** Network RBAC decides once per connection, **before any downstream byte is read** (SPEC R-2, measured). There is no filter in this sub-phase that inspects payload. Adding an `on_data` hook with nothing to exercise it is precisely the `BOOTSTRAP_PROMPT.md` §6.3 anti-pattern; it is deferred as **CF-67-3** to the first payload-parsing network filter (`mongo_proxy` / `zookeeper_proxy` / `kafka_broker`).

**Why `envoy-listener` owns this (W-2).** It already owns `ConnectionHandler` (`:38`) and the accept loop. `envoy-bin` would make the protocol un-reusable by a future `envoy-listener`-side LDS path.

**`close_with_drain` is the `StopIteration` close, and it is ADR-0124's drain.** On DENY, upstream Envoy writes **zero bytes** and closes with a **clean EOF, never an RST**; a post-EOF client write is **accepted**; the client's already-sent bytes are discarded (SPEC R-2, measured). That is byte-for-byte the sequence `direct_response_once` already performs after its payload write (`direct_response.rs:86-102`). Factoring it here means Task 8 keeps ADR-0124's semantics by *calling* the shared helper, and `post_eof_client_write_is_accepted_not_reset` keeps passing. **A server that closes without draining sends an RST** — that is the whole point of the loop; do not "simplify" it away.

`envoy-listener`'s `tokio` features today are `["rt", "net", "macros", "time", "sync"]`. `AsyncWriteExt::shutdown` and `AsyncReadExt::read` need `"io-util"`. It is a feature of an already-permitted dependency (D-3.2), not a new dependency.

- [ ] **Step 1: Write the failing tests**

Append to `crates/envoy-listener/src/lib.rs`'s `mod tests`:

```rust
    /// 67.1 D4 (CF-66-2): a filter returning `Continue` does not close the
    /// connection; the status enum is `Copy` and comparable.
    #[test]
    fn network_filter_status_is_copy_and_eq() {
        let a = NetworkFilterStatus::Continue;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(NetworkFilterStatus::Continue, NetworkFilterStatus::StopIteration);
    }

    /// 67.1 D4: `NetworkFilter` is object-safe — it must be storable as
    /// `Arc<dyn NetworkFilter>` for `ChainHandler`'s filter list (Task 6).
    #[test]
    fn network_filter_is_object_safe() {
        struct AlwaysStop;
        impl NetworkFilter for AlwaysStop {
            fn on_new_connection(&self, _conn: &ConnectionInfo) -> NetworkFilterStatus {
                NetworkFilterStatus::StopIteration
            }
        }
        let f: Arc<dyn NetworkFilter> = Arc::new(AlwaysStop);
        let info = ConnectionInfo {
            peer_addr: "127.0.0.1:1".parse().unwrap(),
            local_addr: "127.0.0.1:2".parse().unwrap(),
        };
        assert_eq!(f.on_new_connection(&info), NetworkFilterStatus::StopIteration);
    }

    /// 67.1 D4 / SPEC R-2 (ADR-0124's drain, shared): `close_with_drain` sends a
    /// FIN with ZERO bytes written, and a client write issued AFTER it observes
    /// EOF is ACCEPTED, not reset. A server that closed without draining its read
    /// half would make the kernel send an RST and the second write would fail.
    ///
    /// DELETE THE DRAIN LOOP IN `close_with_drain` AND THIS TEST MUST FAIL.
    #[tokio::test(flavor = "multi_thread")]
    async fn close_with_drain_sends_clean_eof_and_accepts_post_eof_writes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            close_with_drain(stream).await.unwrap();
        });

        let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Bytes sent before the close are discarded, not echoed.
        c.write_all(b"PING-RBAC\n").await.unwrap();

        let mut out = Vec::new();
        c.read_to_end(&mut out).await.expect("clean EOF, not RST");
        assert!(out.is_empty(), "DENY writes zero bytes, got {out:?}");

        // Two writes: the first may be absorbed locally; a returning RST
        // surfaces on the second. Sleep between them so an RST can land.
        c.write_all(b"y").await.expect("first post-EOF write");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        c.write_all(b"y").await.expect("second post-EOF write must not be reset");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p envoy-listener network_filter close_with_drain 2>&1 | tee /tmp/t5.log`
Expected: compile error — `cannot find type NetworkFilterStatus`, `cannot find function close_with_drain`.

- [ ] **Step 3: Add the tokio feature**

In `crates/envoy-listener/Cargo.toml`:

```toml
tokio = { version = "1", features = ["rt", "net", "macros", "time", "sync", "io-util"] }
```

- [ ] **Step 4: Write the protocol**

In `crates/envoy-listener/src/lib.rs`, immediately after the `ConnectionHandler` trait (`:43`):

```rust
/// 67.1 D4 (CONSUMES carry-forward CF-66-2, on exactly the trigger ADR-0123 §2.2
/// named): the network-filter chain iteration protocol.
///
/// `Continue` hands the connection to the next filter, and ultimately to the
/// chain's TERMINAL filter. `StopIteration` closes the connection — via
/// [`close_with_drain`] — and the terminal filter never runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkFilterStatus {
    Continue,
    StopIteration,
}

/// The downstream connection facts a network filter may inspect at connection
/// establishment. Carries everything network `rbac`'s matcher arms need —
/// including phase `67.2`'s `direct_remote_ip` / `remote_ip` / `source_ip` /
/// `destination_port` / `destination_ip`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionInfo {
    pub peer_addr: SocketAddr,
    pub local_addr: SocketAddr,
}

/// A NON-TERMINAL network filter: it inspects an accepted connection and either
/// yields to the rest of the chain or stops it.
///
/// **There is deliberately NO `on_data` hook.** Every filter in phase 67.1
/// decides once per connection, before any downstream byte is read (phase-67
/// SPEC R-2, measured against `envoyproxy/envoy:v1.33.0`). Adding a mid-stream
/// hook with no filter to exercise it is the `BOOTSTRAP_PROMPT.md` §6.3
/// anti-pattern; it is carried forward as **CF-67-3** to the first
/// payload-parsing network filter (`mongo_proxy` / `zookeeper_proxy` /
/// `kafka_broker`).
///
/// TERMINAL network filters (`echo`, `tcp_proxy`, `http_connection_manager`,
/// `direct_response`) implement [`ConnectionHandler`] instead. The config
/// validator's `NetworkFilterChainNotTerminated` rule guarantees every non-empty
/// chain ends in exactly one of them, so a chain of `NetworkFilter`s always
/// terminates in a `ConnectionHandler`.
pub trait NetworkFilter: Send + Sync + 'static {
    fn on_new_connection(&self, conn: &ConnectionInfo) -> NetworkFilterStatus;
}

/// Close `stream` the way upstream Envoy closes a connection it refuses to
/// forward: write NOTHING, half-close (the client sees a clean EOF, never an
/// RST), then drain and discard the read half until the client closes.
///
/// The drain is ADR-0124's, and it is not optional. Closing a socket while
/// unread bytes sit in the receive queue makes the kernel send an RST, so a
/// client that writes after our FIN would see `BrokenPipe`/`ConnectionReset`.
/// Upstream Envoy ACCEPTS such a write — measured at 0 / 21 / 200 000 unread
/// bytes (`post_write=writes_ok`), and again on the network-`rbac` DENY path
/// (phase-67 SPEC R-2). envoy-rust drains to match.
///
/// Bounded by the caller: `Listener::serve`'s `DRAIN_BUDGET` aborts stragglers.
pub async fn close_with_drain(mut stream: tokio::net::TcpStream) -> Result<(), std::io::Error> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (mut reader, mut writer) = stream.split();
    writer.shutdown().await?;
    let mut sink = [0u8; 8192];
    loop {
        match reader.read(&mut sink).await {
            Ok(0) => break,    // client closed — done
            Ok(_) => continue, // discard and keep draining
            Err(_) => break,   // peer reset/error — nothing left to do
        }
    }
    Ok(())
}
```

`mod tests` needs `use tokio::io::{AsyncReadExt, AsyncWriteExt};` — it already has it (`:768`).

- [ ] **Step 5: Run to verify pass**

```bash
cargo test -p envoy-listener 2>&1 | tail -5
```
Expected: all pass, +3 tests.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p envoy-listener --all-targets -- -D warnings
git add crates/envoy-listener/Cargo.toml crates/envoy-listener/src/lib.rs
git commit -m "phase 67.1 task 5: network-filter iteration protocol (D4); consumes CF-66-2"
```

---

## Task 6: `ChainHandler` + the M66-3 reaping witness (D4, part 2) — **CONSUMES M66-3**

**Files:**
- Modify: `crates/envoy-listener/src/lib.rs` (`ChainHandler`; `Listener::pending_tasks()`; a `watch` publish inside `accept_loop` at `:499-597`; the `Listener` struct at `:80-130`; `bind_inner` at `:205`, `bind_per_worker` at `:247`, and `mk_multi_socket_listener` at `:795`)
- Test: `crates/envoy-listener/src/lib.rs` (`mod tests`)

**Interfaces:**
- Consumes: Task 5's `NetworkFilter`, `NetworkFilterStatus`, `ConnectionInfo`, `close_with_drain`; the existing `ConnectionHandler`.
- Produces:
  - `envoy_listener::ChainHandler::new(filters: Vec<Arc<dyn NetworkFilter>>, inner: Arc<dyn ConnectionHandler>) -> ChainHandler`, which `impl ConnectionHandler`
  - `Listener::pending_tasks(&self) -> usize` (test/observability accessor)

**Context — how M66-3 is consumed.** M66-3 is *"`serve()` never reaps completed `JoinSet` tasks and the per-connection read is unbounded, shared **verbatim** by `echo.rs:21-59` and `direct_response.rs:36-74`."*

**`envoy_listener::accept_loop` already reaps** (`crates/envoy-listener/src/lib.rs:565`):

```rust
Some(done) = join_set.join_next(), if !join_set.is_empty() => { … }
```

So M66-3 is consumed by **deleting** both standalone loops (Tasks 7 and 8) and routing all four terminal filters through this one. The phase-66 review demanded the two files be fixed **together**, or the *"echo is the structural model"* invariant breaks — deleting both in the same sub-phase does exactly that. The second half of M66-3 (the unbounded per-connection read) is then bounded by `Listener::serve`'s `DRAIN_BUDGET` + `join_set.abort_all()`, which is what **M66-4**'s doc line always claimed and can now truthfully claim.

**This task adds the witness the review asked for**: *"the `JoinSet` does not grow without bound across N sequential connections."* `cx_active` (the existing gauge) is decremented **inside** the spawned task before it completes, so `cx_active == 0` does **not** witness reaping — the `JoinSet` entry still lingers. A genuine witness needs `join_set.len()`. Publish it on a `tokio::sync::watch<usize>` after each select iteration and expose it as `Listener::pending_tasks()`.

**`ChainHandler` wraps any `ConnectionHandler`.** That is what makes `[rbac, echo]`, `[rbac, direct_response]`, `[rbac, tcp_proxy]` and `[rbac, http_connection_manager]` all work with no per-arm special-casing. It builds `ConnectionInfo` from the stream's own `peer_addr()` / `local_addr()`, so **`ConnectionHandler`'s signature is not changed**.

For a TLS listener the chain runs on the raw `TcpStream` **before** the TLS handshake (`ChainHandler` wraps `TlsAcceptingHandler`). For `67.1`'s matcher surface (`any` + combinators) and for `67.2`'s (peer/local addresses, which TLS does not alter) the verdict is identical either way. Documented, not assumed.

- [ ] **Step 1: Write the failing tests**

```rust
    /// 67.1 D4: `ChainHandler` runs each filter's `on_new_connection` in order
    /// and, when all return `Continue`, delegates to the terminal handler.
    #[tokio::test(flavor = "multi_thread")]
    async fn chain_handler_continue_delegates_to_terminal_handler() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Counting(Arc<AtomicUsize>);
        impl NetworkFilter for Counting {
            fn on_new_connection(&self, _c: &ConnectionInfo) -> NetworkFilterStatus {
                self.0.fetch_add(1, Ordering::SeqCst);
                NetworkFilterStatus::Continue
            }
        }
        let hits = Arc::new(AtomicUsize::new(0));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(Counting(Arc::clone(&hits))), Arc::new(Counting(Arc::clone(&hits)))],
            Arc::new(EchoHandler),
        ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            chain.handle(s).await.unwrap();
        });

        let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
        c.write_all(b"payload").await.unwrap();
        let mut buf = [0u8; 7];
        c.read_exact(&mut buf).await.expect("terminal echo ran");
        assert_eq!(&buf, b"payload");
        assert_eq!(hits.load(Ordering::SeqCst), 2, "both filters ran, in order");
    }

    /// 67.1 D4 / SPEC R-2: `StopIteration` closes the connection with ZERO bytes
    /// and a clean EOF, and THE TERMINAL FILTER NEVER RUNS.
    #[tokio::test(flavor = "multi_thread")]
    async fn chain_handler_stop_iteration_closes_and_skips_terminal() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Stop;
        impl NetworkFilter for Stop {
            fn on_new_connection(&self, _c: &ConnectionInfo) -> NetworkFilterStatus {
                NetworkFilterStatus::StopIteration
            }
        }
        struct Tripwire(Arc<AtomicBool>);
        impl ConnectionHandler for Tripwire {
            fn handle(&self, _d: tokio::net::TcpStream)
                -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
                self.0.store(true, Ordering::SeqCst);
                Box::pin(async move { Ok(()) })
            }
        }
        let ran = Arc::new(AtomicBool::new(false));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(Stop)],
            Arc::new(Tripwire(Arc::clone(&ran))),
        ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            chain.handle(s).await.unwrap();
        });

        let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
        c.write_all(b"discarded").await.unwrap();
        let mut out = Vec::new();
        c.read_to_end(&mut out).await.expect("clean EOF, not RST");
        assert!(out.is_empty(), "DENY writes zero bytes, got {out:?}");
        assert!(!ran.load(Ordering::SeqCst), "terminal handler must NOT run");
    }

    /// 67.1 D4: a filter that STOPS short-circuits — later filters do not run.
    #[tokio::test(flavor = "multi_thread")]
    async fn chain_handler_stop_short_circuits_later_filters() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Stop;
        impl NetworkFilter for Stop {
            fn on_new_connection(&self, _c: &ConnectionInfo) -> NetworkFilterStatus {
                NetworkFilterStatus::StopIteration
            }
        }
        struct Tripwire(Arc<AtomicBool>);
        impl NetworkFilter for Tripwire {
            fn on_new_connection(&self, _c: &ConnectionInfo) -> NetworkFilterStatus {
                self.0.store(true, Ordering::SeqCst);
                NetworkFilterStatus::Continue
            }
        }
        let ran = Arc::new(AtomicBool::new(false));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(Stop), Arc::new(Tripwire(Arc::clone(&ran)))],
            Arc::new(EchoHandler),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            chain.handle(s).await.unwrap();
        });
        let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut out = Vec::new();
        c.read_to_end(&mut out).await.unwrap();
        assert!(!ran.load(Ordering::SeqCst), "filters after a Stop must not run");
    }

    /// 67.1 D4: `ChainHandler` hands the filter the connection's REAL peer and
    /// local addresses, read from the accepted socket. `67.2`'s IP/port matcher
    /// arms depend on this being exact.
    #[tokio::test(flavor = "multi_thread")]
    async fn chain_handler_populates_connection_info_from_the_socket() {
        use std::sync::Mutex;
        struct Capture(Arc<Mutex<Option<ConnectionInfo>>>);
        impl NetworkFilter for Capture {
            fn on_new_connection(&self, c: &ConnectionInfo) -> NetworkFilterStatus {
                *self.0.lock().unwrap() = Some(*c);
                NetworkFilterStatus::Continue
            }
        }
        let seen = Arc::new(Mutex::new(None));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(Capture(Arc::clone(&seen)))],
            Arc::new(EchoHandler),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            chain.handle(s).await.unwrap();
        });
        let c = tokio::net::TcpStream::connect(addr).await.unwrap();
        let client_addr = c.local_addr().unwrap();
        drop(c);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let info = seen.lock().unwrap().expect("filter saw the connection");
        assert_eq!(info.local_addr, addr, "local_addr is the listener address");
        assert_eq!(info.peer_addr, client_addr, "peer_addr is the client address");
    }

    /// 67.1 — the M66-3 REGRESSION WITNESS.
    ///
    /// M66-3: "`serve()` never reaps completed `JoinSet` tasks", shared verbatim
    /// by the two standalone accept loops phase 67.1 DELETES (`echo.rs`,
    /// `direct_response.rs`). The surviving loop — `envoy_listener::accept_loop`
    /// — reaps via its `join_next()` select arm. This test proves it: after N
    /// sequential connections have completed, the `JoinSet` is EMPTY.
    ///
    /// `cx_active` cannot witness this: it is decremented INSIDE the spawned
    /// task, so it reads 0 while the JoinSet entry still lingers. Only
    /// `pending_tasks()` (which publishes `join_set.len()`) sees the difference.
    ///
    /// DELETE THE `join_next()` SELECT ARM IN `accept_loop` AND THIS TEST MUST FAIL
    /// (the count would climb to N).
    #[tokio::test(flavor = "multi_thread")]
    async fn sequential_connections_do_not_accumulate_joinset_tasks() {
        const N: usize = 50;
        let cfg = mk_listener_cfg("127.0.0.1", 0);
        let registry = mk_registry();
        let listener = Listener::bind(&cfg, Arc::new(NullHandler), Arc::clone(&registry))
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let pending = listener.pending_tasks_watch();

        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(listener.serve(async move { let _ = rx.await; }, drain));

        for _ in 0..N {
            let c = tokio::net::TcpStream::connect(addr).await.expect("connect");
            drop(c);
            tokio::task::yield_now().await;
        }
        // Give the accept loop time to observe every completion.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let leaked = *pending.borrow();
        assert!(
            leaked <= 1,
            "JoinSet leaked {leaked} completed tasks across {N} sequential connections \
             (non-reaping regression — see M66-3)",
        );

        tx.send(()).expect("shutdown");
        tokio::time::timeout(std::time::Duration::from_secs(7), server)
            .await.expect("serve resolves").expect("join").expect("serve ok");
    }
```

> The witness uses `pending_tasks_watch()` (a `watch::Receiver<usize>`) because `Listener::serve` **consumes** `self`; a `&self` accessor would be unusable once serving. `pending_tasks(&self) -> usize` is the convenience wrapper over `*watch.borrow()` and is what non-test callers would use before `serve`.
>
> `<= 1` rather than `== 0`: the select is not required to have polled `join_next()` for the very last connection before the assertion. The property under test is *bounded*, not *zero*: a non-reaping loop reads 50.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p envoy-listener chain_handler sequential_connections 2>&1 | tee /tmp/t6.log`
Expected: compile error — `cannot find type ChainHandler`, `no method pending_tasks_watch`.

- [ ] **Step 3: Write `ChainHandler`**

In `crates/envoy-listener/src/lib.rs`, after `close_with_drain`:

```rust
/// 67.1 D4/D5: the network-filter chain, expressed as a [`ConnectionHandler`]
/// that wraps another [`ConnectionHandler`].
///
/// On each accepted connection it runs every non-terminal filter's
/// `on_new_connection` in configured order. The first `StopIteration` closes the
/// connection via [`close_with_drain`] — zero bytes, clean EOF, no RST — and the
/// TERMINAL handler never runs. When every filter returns `Continue`, the
/// connection is handed to `inner`.
///
/// Because it wraps an arbitrary `Arc<dyn ConnectionHandler>`, ONE implementation
/// covers every terminal filter — `echo`, `direct_response`, `tcp_proxy` and
/// `http_connection_manager` — with no per-filter special-casing. The config
/// validator's `NetworkFilterChainNotTerminated` rule (67.1 D2) guarantees a
/// terminal handler always exists, so the iteration always terminates.
///
/// On a TLS listener this runs on the raw `TcpStream` BEFORE the TLS handshake
/// (`ChainHandler` wraps `TlsAcceptingHandler`). For the matcher arms that exist
/// — `any` + combinators here, peer/local addresses in `67.2` — the verdict is
/// identical either way, because TLS alters neither address.
pub struct ChainHandler {
    filters: Arc<[Arc<dyn NetworkFilter>]>,
    inner: Arc<dyn ConnectionHandler>,
}

impl ChainHandler {
    /// `filters` must contain only NON-terminal filters, in configured order.
    /// An empty `filters` list makes this handler transparent; callers should
    /// skip the wrapper entirely in that case (see `envoy-bin::main`).
    pub fn new(filters: Vec<Arc<dyn NetworkFilter>>, inner: Arc<dyn ConnectionHandler>) -> Self {
        Self { filters: filters.into(), inner }
    }
}

impl ConnectionHandler for ChainHandler {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let filters = Arc::clone(&self.filters);
        let inner = Arc::clone(&self.inner);
        Box::pin(async move {
            let conn = ConnectionInfo {
                peer_addr: downstream.peer_addr()?,
                local_addr: downstream.local_addr()?,
            };
            for filter in filters.iter() {
                if filter.on_new_connection(&conn) == NetworkFilterStatus::StopIteration {
                    // SPEC R-2: zero bytes, clean EOF, never an RST; the
                    // terminal filter never runs.
                    close_with_drain(downstream).await?;
                    return Ok(());
                }
            }
            inner.handle(downstream).await
        })
    }
}
```

- [ ] **Step 4: Publish `join_set.len()` from `accept_loop`**

Add a field to `Listener` (`:80-130`):

```rust
    /// 67.1: publishes `accept_loop`'s in-flight `JoinSet` length after every
    /// select iteration. The M66-3 reaping witness reads it; production code may
    /// use it for introspection. Distinct from `cx_active`, which is decremented
    /// INSIDE each connection task and therefore cannot observe an unreaped
    /// completed task.
    pending_tasks: tokio::sync::watch::Sender<usize>,
```

Initialise it in `bind_inner`, `bind_per_worker` (one per shard) and the test helper `mk_multi_socket_listener` with `tokio::sync::watch::channel(0usize).0`. Add the accessors:

```rust
impl Listener {
    /// In-flight connection tasks currently held by the accept loop's `JoinSet`.
    pub fn pending_tasks(&self) -> usize {
        *self.pending_tasks.borrow()
    }

    /// A receiver that keeps observing `pending_tasks` after `serve` consumes
    /// `self`. Used by the M66-3 reaping witness.
    pub fn pending_tasks_watch(&self) -> tokio::sync::watch::Receiver<usize> {
        self.pending_tasks.subscribe()
    }
}
```

Thread it into `accept_loop`'s signature (it is already `#[allow(clippy::too_many_arguments)]`) and publish at the bottom of each `loop` iteration, after the `select!`:

```rust
        // 67.1 (M66-3 witness): republish the in-flight task count after every
        // select iteration — including the `join_next()` arm that REAPS a
        // completed task. A loop without that arm would see this climb without
        // bound across sequential connections.
        let _ = pending_tasks.send(join_set.len());
```

In the `SO_REUSEPORT` fan-out branch of `serve`, clone the `watch::Sender` per loop (`pending_tasks.clone()`); each shard publishes its own socket's count. `serve` moves `self.pending_tasks` out alongside the other fields.

- [ ] **Step 5: Run to verify pass**

```bash
cargo test -p envoy-listener 2>&1 | tail -5
```
Expected: all pass, +5 tests. Confirm the witness is real by temporarily deleting the `Some(done) = join_set.join_next(), …` select arm: `sequential_connections_do_not_accumulate_joinset_tasks` must FAIL with `JoinSet leaked 50 completed tasks`. **Restore the arm.**

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p envoy-listener --all-targets -- -D warnings
git add crates/envoy-listener/src/lib.rs
git commit -m "phase 67.1 task 6: ChainHandler + JoinSet reaping witness (D4); consumes M66-3"
```

---

## Task 7: `echo` becomes a `ConnectionHandler` (D5, part 1)

**Files:**
- Rewrite: `crates/envoy-bin/src/echo.rs` (delete `serve()` + `DRAIN_TIMEOUT` + the `JoinSet` accept loop, `:1-60`; keep `echo_once`)
- Test: `crates/envoy-bin/src/echo.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `envoy_listener::{ConnectionHandler, BoxFuture}`.
- Produces: `crate::echo::EchoHandler` (unit struct) implementing `ConnectionHandler`.

**Context.** `echo::serve` (`echo.rs:20-60`) owns a standalone accept loop whose `JoinSet` **never reaps** — half of **M66-3**. `envoy_listener::accept_loop` already does everything this loop does, and reaps. Delete the loop; keep the per-connection body.

`echo_once` (`:62-73`) reads into an 8 KiB buffer and writes back until the client half-closes, then `writer.shutdown()`. That behavior is what fixture `0001` asserts byte-exact against upstream Envoy. **Do not change it.** In particular do not swap it for `tokio::io::copy` — that would not issue the trailing `shutdown()`, and `drive_tcp`'s ADR-0007 trailing-byte poll depends on the peer either closing or staying silent.

**The `echo` `typed_config` asymmetry stays.** Upstream Envoy REQUIRES `typed_config` on `envoy.filters.network.echo`; envoy-rust forbids it (`UnexpectedTypedConfig`). Fixture `0001`'s two sides differ accordingly (ADR-0014 YAML shim). **Do not "fix" it here.**

- [ ] **Step 1: Write the failing test**

Replace `echo.rs`'s `mod tests` — the two existing tests drive `serve()`, which is being deleted. Their *assertions* are preserved, now driven through `envoy_listener::Listener`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use envoy_listener::{ConnectionHandler, DrainState, Listener};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::oneshot;

    fn listener_cfg(port: u16) -> envoy_config::Listener {
        serde_yaml::from_str(&format!(
            r#"
name: echo_listener
address:
  socket_address:
    address: 127.0.0.1
    port_value: {port}
filter_chains:
  - filters:
      - name: envoy.filters.network.echo
"#
        ))
        .expect("hand-constructed listener YAML parses")
    }

    /// Spawn `EchoHandler` behind the SHARED `envoy_listener::Listener` accept
    /// loop — the same loop `tcp_proxy` and HCM use. 67.1 deleted `echo::serve`'s
    /// standalone, non-reaping loop (M66-3).
    async fn spawn() -> (std::net::SocketAddr, oneshot::Sender<()>) {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let handler: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
        let listener = Listener::bind(&listener_cfg(0), handler, Arc::clone(&registry))
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(listener.serve(async move { let _ = rx.await; }, drain));
        (addr, tx)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn echoes_single_payload_and_drains_on_shutdown() {
        let (addr, tx) = spawn().await;
        let mut client = TcpStream::connect(addr).await.unwrap();
        let payload = b"hello, envoy-rust\n";
        client.write_all(payload).await.unwrap();
        client.shutdown().await.unwrap();
        let mut echoed = Vec::new();
        client.read_to_end(&mut echoed).await.unwrap();
        assert_eq!(echoed, payload);

        tx.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(TcpStream::connect(addr).await.is_err(), "listener closed on shutdown");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn handles_two_concurrent_connections() {
        let (addr, _tx) = spawn().await;
        let one = tokio::spawn(async move {
            let mut c = TcpStream::connect(addr).await.unwrap();
            c.write_all(b"AAA").await.unwrap();
            c.shutdown().await.unwrap();
            let mut out = Vec::new();
            c.read_to_end(&mut out).await.unwrap();
            out
        });
        let two = tokio::spawn(async move {
            let mut c = TcpStream::connect(addr).await.unwrap();
            c.write_all(b"BBBB").await.unwrap();
            c.shutdown().await.unwrap();
            let mut out = Vec::new();
            c.read_to_end(&mut out).await.unwrap();
            out
        });
        assert_eq!(one.await.unwrap(), b"AAA");
        assert_eq!(two.await.unwrap(), b"BBBB");
    }

    /// 67.1: `EchoHandler` is a plain `ConnectionHandler`, so it composes under
    /// `ChainHandler` exactly as `tcp_proxy` and HCM do.
    #[tokio::test(flavor = "multi_thread")]
    async fn echo_handler_is_a_connection_handler() {
        let _: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
    }
}
```

`envoy-bin` needs `serde_yaml` in `[dev-dependencies]`? No — it is already a normal dependency (`Cargo.toml`), so `mod tests` can use it. `envoy_stats` and `envoy_listener` are likewise already dependencies.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p envoy-bin --lib 2>&1 | tee /tmp/t7.log` — actually `envoy-bin` is a **binary** crate, so its `mod tests` run under the bin target:
Run: `cargo test -p envoy-bin --bin envoy-bin echo 2>&1 | tee /tmp/t7.log`
Expected: compile error — `cannot find type EchoHandler`.

- [ ] **Step 3: Rewrite `echo.rs`**

Replace `echo.rs:1-60` (everything above `echo_once`) with:

```rust
//! `envoy.filters.network.echo` — a TERMINAL network filter.
//!
//! Each accepted connection copies bytes from the read half to the write half
//! until the client half-closes, mirroring upstream Envoy's
//! `envoy.filters.network.echo`.
//!
//! 67.1 (ADR-0130): the standalone accept loop this module used to own was
//! DELETED. `echo` is now a plain `envoy_listener::ConnectionHandler`, served by
//! the ONE shared `envoy_listener::Listener` accept loop that `tcp_proxy` and
//! HCM already used — which reaps its completed `JoinSet` tasks and bounds
//! in-flight connections by `DRAIN_BUDGET`. That deletion is how carry-forward
//! **M66-3** is consumed. `direct_response.rs` was converted in the same
//! sub-phase, preserving the "echo is the structural model" invariant the
//! phase-66 review required.

use envoy_listener::{BoxFuture, ConnectionHandler};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The terminal `echo` network filter, as a per-connection handler.
pub struct EchoHandler;

impl ConnectionHandler for EchoHandler {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        Box::pin(async move {
            echo_once(downstream).await?;
            Ok(())
        })
    }
}
```

Keep `echo_once` exactly as it is, changing only its visibility if needed (it stays private) and its signature to take an owned `TcpStream` (it already does):

```rust
async fn echo_once(mut stream: tokio::net::TcpStream) -> std::io::Result<()> {
    let (mut reader, mut writer) = stream.split();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            writer.shutdown().await.ok();
            return Ok(());
        }
        writer.write_all(&buf[..n]).await?;
    }
}
```

(The return type changes from `anyhow::Result<()>` to `std::io::Result<()>` because `ConnectionHandler`'s error is `Box<dyn Error + Send + Sync>`, which `std::io::Error` converts into. `anyhow` is no longer needed in this module.)

- [ ] **Step 4: Run to verify pass**

```bash
cargo test -p envoy-bin --bin envoy-bin echo 2>&1 | tail -5
```
Expected: `3 passed`. The crate will not fully build yet — `main.rs` still calls `echo::serve`. That is Task 10. To keep this task independently committable, apply the two-line `main.rs` change now:

```rust
            envoy_config::ECHO_FILTER => {
                bind_and_spawn_listener(
                    listener_cfg,
                    std::sync::Arc::new(echo::EchoHandler),
                    &registry,
                    listener_concurrency,
                    "echo",
                    bind_addr,
                    || tracing::info!(addr = %bind_addr, "envoy-rust listening (echo)"),
                    &token,
                    &drain,
                    &mut set,
                )
                .await?;
            }
```

- [ ] **Step 5: Prove no regression on the echo differential fixture**

```bash
cargo build -p envoy-bin            # the harness runs target/debug/envoy-bin
cargo test -p differential --test tcp_echo -- --nocapture 2>&1 | tail -20
```
Expected: fixture `0001` green. (If Docker is unavailable, note it and rely on CI; do **not** weaken the fixture.)

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p envoy-bin --all-targets -- -D warnings
git add crates/envoy-bin/src/echo.rs crates/envoy-bin/src/main.rs
git commit -m "phase 67.1 task 7: echo becomes a ConnectionHandler; delete its standalone accept loop (D5, M66-3)"
```

---

## Task 8: `direct_response` becomes a `ConnectionHandler` (D5, part 2) — **CONSUMES M66-4**

**Files:**
- Rewrite: `crates/envoy-bin/src/direct_response.rs` (delete `serve()` + `DRAIN_TIMEOUT` + the `JoinSet` accept loop, `:13-75`; rewrite `direct_response_once`'s tail to call `close_with_drain`)
- Modify: `crates/envoy-bin/src/main.rs` (the `DIRECT_RESPONSE_FILTER` arm, `:252-277`)
- Test: `crates/envoy-bin/src/direct_response.rs` (`mod tests`)

**Interfaces:**
- Consumes: `envoy_listener::{ConnectionHandler, BoxFuture, close_with_drain}`.
- Produces: `crate::direct_response::DirectResponseHandler { payload: Arc<[u8]> }` implementing `ConnectionHandler`, constructed via `DirectResponseHandler::new(payload: Arc<[u8]>)`.

**Context — ADR-0124 must survive this task intact.**

`direct_response_once` (`:77-104`) performs `write_all` → `flush` → `shutdown()` → **drain-to-EOF** → drop. That drain is ADR-0124: *"a client write issued AFTER it observes EOF is accepted, not reset"*, measured on upstream Envoy at 0, 21 and 200 000 unread bytes. A server that closed without draining would RST the client, which upstream Envoy does not do.

**`post_eof_client_write_is_accepted_not_reset` (`:165-180`) pins it and carries a mutation-check doc comment: "DELETE THE DRAIN LOOP … AND THIS TEST MUST FAIL." It must not be weakened, deleted, or have its assertions relaxed.** It survives verbatim, only re-plumbed through `Listener`.

Task 5's `envoy_listener::close_with_drain` **is** the `shutdown()` + drain-to-EOF tail. Calling it here is a pure factoring: same syscalls, same order.

**M66-4** is the doc-precision line at `:93-94` — *"Bounded by the caller's shutdown drain (DRAIN_TIMEOUT), exactly as `echo.rs` is."* Both halves of that sentence are now stale: `DRAIN_TIMEOUT` no longer exists in this module, and `echo.rs` no longer has a loop of its own. Rewrite it to name `envoy_listener::DRAIN_BUDGET` and `Listener::serve`'s `abort_all()`. **That rewrite consumes M66-4.**

- [ ] **Step 1: Write the failing test**

Rewrite `direct_response.rs`'s `mod tests`, preserving **all five** existing assertions and adding one:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use envoy_listener::{ConnectionHandler, DrainState, Listener};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::oneshot;

    fn listener_cfg(port: u16) -> envoy_config::Listener {
        serde_yaml::from_str(&format!(
            "name: dr_listener\naddress:\n  socket_address:\n    address: 127.0.0.1\n    port_value: {port}\nfilter_chains:\n  - filters: []\n"
        ))
        .expect("hand-constructed listener YAML parses")
    }

    /// 67.1: served by the SHARED `envoy_listener::Listener` accept loop. The
    /// standalone loop this module used to own was deleted (M66-3).
    async fn spawn(payload: &'static [u8]) -> (std::net::SocketAddr, oneshot::Sender<()>) {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let handler: Arc<dyn ConnectionHandler> =
            Arc::new(DirectResponseHandler::new(Arc::from(payload)));
        let listener = Listener::bind(&listener_cfg(0), handler, Arc::clone(&registry))
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(listener.serve(async move { let _ = rx.await; }, drain));
        (addr, tx)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn writes_payload_then_clean_eof() {
        let (addr, _tx) = spawn(b"hello-from-direct-response\n").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF, not RST");
        assert_eq!(out, b"hello-from-direct-response\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn empty_payload_writes_zero_bytes_then_closes() {
        // Phase-66 SPEC §0 R-0.7: Envoy with `response` omitted writes 0 bytes + closes.
        let (addr, _tx) = spawn(b"").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF");
        assert!(out.is_empty(), "expected zero bytes, got {out:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn client_that_writes_first_still_receives_payload() {
        // Phase-66 SPEC §0 R-0.5: Envoy ignores client input and still delivers.
        let (addr, _tx) = spawn(b"PAYLOAD\n").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(b"PING-NEVER-READ\n").await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF");
        assert_eq!(out, b"PAYLOAD\n");
    }

    /// MUTATION CHECK for the drain (ADR-0124 / phase-66 SPEC V-3).
    ///
    /// Upstream Envoy accepts a client write issued AFTER the client observes
    /// EOF (measured: `post_write=writes_ok` at 0 / 21 / 200_000 unread bytes).
    /// A server that closes without draining its read half sends an RST, and
    /// this write fails with BrokenPipe/ConnectionReset.
    ///
    /// 67.1 re-plumbed the drain into `envoy_listener::close_with_drain`.
    /// DELETE THAT DRAIN LOOP AND THIS TEST MUST FAIL.
    #[tokio::test(flavor = "multi_thread")]
    async fn post_eof_client_write_is_accepted_not_reset() {
        let (addr, _tx) = spawn(b"PAYLOAD\n").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF");
        assert_eq!(out, b"PAYLOAD\n");

        // Two writes: the first may be absorbed locally; a returning RST
        // surfaces on the second. Sleep between them so an RST can land.
        s.write_all(b"y").await.expect("first post-EOF write");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        s.write_all(b"y").await.expect("second post-EOF write must not be reset");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shutdown_signal_stops_the_accept_loop() {
        let (addr, tx) = spawn(b"x").await;
        let _ = TcpStream::connect(addr).await.unwrap();
        tx.send(()).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert!(TcpStream::connect(addr).await.is_err(), "listener must be closed");
    }

    /// 67.1: composes under `ChainHandler` like every other terminal filter.
    #[tokio::test(flavor = "multi_thread")]
    async fn direct_response_handler_is_a_connection_handler() {
        let _: Arc<dyn ConnectionHandler> = Arc::new(DirectResponseHandler::new(Arc::from(&b"x"[..])));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p envoy-bin --bin envoy-bin direct_response 2>&1 | tee /tmp/t8.log`
Expected: compile error — `cannot find type DirectResponseHandler`.

- [ ] **Step 3: Rewrite `direct_response.rs`**

Replace `:1-104` (module doc, imports, `DRAIN_TIMEOUT`, `serve`, `direct_response_once`) with:

```rust
//! `envoy.filters.network.direct_response` — the Network-filters family opener
//! (phase 66, ADR-0123), a TERMINAL network filter.
//!
//! On each accepted downstream connection the filter writes its configured
//! payload IMMEDIATELY — without reading or waiting for any client bytes — then
//! half-closes (FIN) and drains the read half until the client closes.
//! Empirically matched against `envoyproxy/envoy:v1.33.0` (phase-66 SPEC §0
//! R-0.5/R-0.7).
//!
//! 67.1 (ADR-0130): the standalone accept loop this module used to own was
//! DELETED, in the same sub-phase as `echo.rs`'s — preserving the "echo is the
//! structural model" invariant the phase-66 review required, and consuming
//! carry-forward **M66-3** by removal. `direct_response` is now a plain
//! `envoy_listener::ConnectionHandler` served by the ONE shared
//! `envoy_listener::Listener` accept loop.

use std::sync::Arc;

use envoy_listener::{BoxFuture, ConnectionHandler, close_with_drain};
use tokio::io::AsyncWriteExt;

/// The terminal `direct_response` network filter, as a per-connection handler.
pub struct DirectResponseHandler {
    payload: Arc<[u8]>,
}

impl DirectResponseHandler {
    /// `payload` may be empty — `response` omitted is a legal config
    /// (phase-66 SPEC §0 R-0.7) and yields a zero-byte write plus a clean close.
    pub fn new(payload: Arc<[u8]>) -> Self {
        Self { payload }
    }
}

impl ConnectionHandler for DirectResponseHandler {
    fn handle(
        &self,
        mut downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let payload = Arc::clone(&self.payload);
        Box::pin(async move {
            // Write the payload immediately; never read first.
            downstream.write_all(&payload).await?;
            downstream.flush().await?;

            // ADR-0124 (phase-66 SPEC V-3): half-close, then drain the read half
            // until the client closes. Closing the socket while unread bytes sit
            // in the receive queue makes the kernel send an RST, so a client that
            // writes after our FIN would see BrokenPipe/ConnectionReset. Upstream
            // Envoy accepts such a write (measured at 0 / 21 / 200_000 unread
            // bytes), so envoy-rust drains to match.
            //
            // 67.1 (consumes M66-4 — the stale doc-precision line this replaces):
            // the drain is bounded by `envoy_listener::DRAIN_BUDGET`. When
            // `Listener::serve` drains, a connection still parked in this loop
            // past the budget is aborted by the accept loop's `JoinSet::abort_all()`.
            // The previous wording named a module-local `DRAIN_TIMEOUT` and
            // `echo.rs`'s accept loop; neither exists any more.
            close_with_drain(downstream).await?;
            Ok(())
        })
    }
}
```

- [ ] **Step 4: Update `main.rs`'s `DIRECT_RESPONSE_FILTER` arm**

```rust
            envoy_config::DIRECT_RESPONSE_FILTER => {
                let Some(envoy_config::TypedConfig::DirectResponse(dr_cfg)) =
                    filter.typed_config.as_ref()
                else {
                    anyhow::bail!(
                        "validator guarantees a DirectResponse typed_config on {}",
                        envoy_config::DIRECT_RESPONSE_FILTER
                    );
                };
                // `response` omitted => empty payload (phase-66 SPEC §0 R-0.7).
                let payload: std::sync::Arc<[u8]> = dr_cfg
                    .response
                    .as_ref()
                    .map(|d| d.inline_string.as_bytes())
                    .unwrap_or(&[])
                    .into();
                let payload_len = payload.len();
                bind_and_spawn_listener(
                    listener_cfg,
                    std::sync::Arc::new(direct_response::DirectResponseHandler::new(payload)),
                    &registry,
                    listener_concurrency,
                    "direct_response",
                    bind_addr,
                    || tracing::info!(addr = %bind_addr, payload_len, "envoy-rust listening (direct_response)"),
                    &token,
                    &drain,
                    &mut set,
                )
                .await?;
            }
```

- [ ] **Step 5: Run to verify pass — and prove ADR-0124 survives**

```bash
cargo test -p envoy-bin --bin envoy-bin direct_response 2>&1 | tail -8
```
Expected: `6 passed`, including `post_eof_client_write_is_accepted_not_reset`.

Then the mutation check that gives that test its meaning: temporarily replace `close_with_drain(downstream).await?` with `downstream.shutdown().await?` (no drain). Re-run. **`post_eof_client_write_is_accepted_not_reset` MUST FAIL** with `BrokenPipe`/`ConnectionReset`. **Restore `close_with_drain`.** If it does not fail, the drain is not being exercised — stop and invoke `superpowers:systematic-debugging` before continuing.

Then the fixture:

```bash
cargo build -p envoy-bin
cargo test -p differential --test network_filter_direct_response 2>&1 | tail -20
```
Expected: fixture `0071` green.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p envoy-bin --all-targets -- -D warnings
git add crates/envoy-bin/src/direct_response.rs crates/envoy-bin/src/main.rs
git commit -m "phase 67.1 task 8: direct_response becomes a ConnectionHandler; ADR-0124 drain preserved (D5, M66-3, M66-4)"
```

---

## Task 9: The network RBAC engine (D6)

**Files:**
- Create: `crates/envoy-bin/src/network_rbac.rs`
- Modify: `crates/envoy-bin/src/main.rs` (add `mod network_rbac;`)
- Test: `crates/envoy-bin/src/network_rbac.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: Task 1's `envoy_config::{NetworkRbacConfig, Rules, Action, Permission, Principal}`; Task 5's `envoy_listener::{NetworkFilter, NetworkFilterStatus, ConnectionInfo}`; `envoy_stats::{StatsRegistry, Counter, StatsError}`.
- Produces: `crate::network_rbac::NetworkRbacFilter::new(cfg: &NetworkRbacConfig, registry: &StatsRegistry) -> Result<NetworkRbacFilter, StatsError>`, implementing `NetworkFilter`.

**Context — the semantics, exactly.**

- **Policy match:** a policy matches when **any** of its `permissions` matches **AND any** of its `principals` matches.
- **Verdict:** if **some** policy matches, the verdict is `rules.action`; otherwise it is the **inverse** of `rules.action`. So `action: ALLOW` + match ⇒ allow; `action: ALLOW` + no match ⇒ deny. `action: DENY` + match ⇒ deny; `action: DENY` + no match ⇒ allow.
- **`rules: None` ⇒ the filter is INERT** (SPEC R-4, measured): `Continue`, and **NEITHER counter increments**. Materialising a default `Rules { action: ALLOW, policies: {} }` and ticking `allowed` would be a **stat divergence with no body divergence** — invisible to a body-only fixture. Do not do it.
- **`Any(b) => *b`.** `any: false` never matches. This mirrors the landed HTTP RBAC evaluator exactly (`crates/envoy-filter/src/rbac.rs:59`, `RuntimeMatcher::Any(b) => *b`); it is not a new decision.
- **Counters:** `<stat_prefix>.rbac.allowed` and `.denied` increment; `<stat_prefix>.rbac.shadow_allowed` and `.shadow_denied` are **registered at 0 and never incremented**, so the stat *tree* matches upstream's shape (**CF-67-1**). All four register unconditionally, including when `rules` is `None` — SPEC R-4 shows upstream emits all four at 0 in that case.
- **DENY returns `StopIteration`**, and `ChainHandler` closes with zero bytes + clean EOF via `close_with_drain`. The engine never touches the socket.

`Registry::register_counter` (`crates/envoy-stats/src/registry.rs:45`) validates names against `[a-zA-Z_:][a-zA-Z0-9_:.-]*` and is idempotent by name. `envoy-config`'s `Rules` derives `Clone`.

**The two `match` statements below are EXHAUSTIVE with no `_ =>` catch-all.** When `67.2` adds the connection-level arms, they must fail to compile until each is implemented. **Never add a catch-all.**

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> ConnectionInfo {
        ConnectionInfo {
            peer_addr: "10.0.0.1:54321".parse().unwrap(),
            local_addr: "127.0.0.1:10000".parse().unwrap(),
        }
    }

    fn cfg(stat_prefix: &str, rules_yaml: Option<&str>) -> envoy_config::NetworkRbacConfig {
        let yaml = match rules_yaml {
            Some(r) => format!("stat_prefix: {stat_prefix}\nrules:\n{r}"),
            None => format!("stat_prefix: {stat_prefix}"),
        };
        serde_yaml::from_str(&yaml).expect("NetworkRbacConfig parses")
    }

    const ANY_POLICY: &str =
        "  policies:\n    p0:\n      permissions: [{ any: true }]\n      principals: [{ any: true }]";

    fn stat(reg: &envoy_stats::StatsRegistry, name: &str) -> u64 {
        reg.register_counter(name).expect("counter").value()
    }

    /// D6 / SPEC R-2: `action: ALLOW` + a matching policy ⇒ Continue, `allowed` ticks.
    #[test]
    fn allow_action_with_matching_policy_continues_and_ticks_allowed() {
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("a", Some(&format!("  action: ALLOW\n{ANY_POLICY}"))), &reg).unwrap();
        assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue);
        assert_eq!(stat(&reg, "a.rbac.allowed"), 1);
        assert_eq!(stat(&reg, "a.rbac.denied"), 0);
    }

    /// D6 / SPEC R-2: `action: DENY` + a matching policy ⇒ StopIteration, `denied` ticks.
    #[test]
    fn deny_action_with_matching_policy_stops_and_ticks_denied() {
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("d", Some(&format!("  action: DENY\n{ANY_POLICY}"))), &reg).unwrap();
        assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::StopIteration);
        assert_eq!(stat(&reg, "d.rbac.denied"), 1);
        assert_eq!(stat(&reg, "d.rbac.allowed"), 0);
    }

    /// D6: the verdict on NO match is the INVERSE of `action`.
    #[test]
    fn no_matching_policy_inverts_the_action() {
        let never = "  policies:\n    p0:\n      permissions: [{ any: false }]\n      principals: [{ any: true }]";
        let reg = envoy_stats::StatsRegistry::new();
        let allow = NetworkRbacFilter::new(&cfg("x", Some(&format!("  action: ALLOW\n{never}"))), &reg).unwrap();
        assert_eq!(allow.on_new_connection(&conn()), NetworkFilterStatus::StopIteration);
        assert_eq!(stat(&reg, "x.rbac.denied"), 1);

        let reg2 = envoy_stats::StatsRegistry::new();
        let deny = NetworkRbacFilter::new(&cfg("y", Some(&format!("  action: DENY\n{never}"))), &reg2).unwrap();
        assert_eq!(deny.on_new_connection(&conn()), NetworkFilterStatus::Continue);
        assert_eq!(stat(&reg2, "y.rbac.allowed"), 1);
    }

    /// D6: a policy matches only when SOME permission AND SOME principal match.
    #[test]
    fn policy_requires_both_a_permission_and_a_principal_match() {
        let half = "  policies:\n    p0:\n      permissions: [{ any: true }]\n      principals: [{ any: false }]";
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("h", Some(&format!("  action: ALLOW\n{half}"))), &reg).unwrap();
        assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::StopIteration,
                   "permission matched but principal did not ⇒ policy does not match");
    }

    /// D6: ANY policy matching is enough.
    #[test]
    fn any_matching_policy_decides() {
        let two = "  policies:\n    p0:\n      permissions: [{ any: false }]\n      principals: [{ any: true }]\n    p1:\n      permissions: [{ any: true }]\n      principals: [{ any: true }]";
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("m", Some(&format!("  action: DENY\n{two}"))), &reg).unwrap();
        assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::StopIteration);
    }

    /// D6: the combinators. `and` = all, `or` = any, `not` = negate; nested.
    #[test]
    fn combinators_and_or_not() {
        let pol = "  policies:\n    p0:\n      permissions:\n        - and_rules:\n            rules:\n              - any: true\n              - not_rule: { any: false }\n      principals:\n        - or_ids:\n            ids:\n              - any: false\n              - not_id: { any: false }";
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("c", Some(&format!("  action: ALLOW\n{pol}"))), &reg).unwrap();
        assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue);
        assert_eq!(stat(&reg, "c.rbac.allowed"), 1);
    }

    /// D6: an `and_rules` set with ONE non-matching child does not match.
    #[test]
    fn and_rules_requires_every_child() {
        let pol = "  policies:\n    p0:\n      permissions:\n        - and_rules:\n            rules:\n              - any: true\n              - any: false\n      principals: [{ any: true }]";
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("n", Some(&format!("  action: ALLOW\n{pol}"))), &reg).unwrap();
        assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::StopIteration);
    }

    /// D6 / SPEC R-4 — THE INERTNESS WITNESS (PLAN-VERIFY W-6).
    ///
    /// `rules` omitted ⇒ the filter is INERT: the connection is allowed and
    /// NEITHER counter increments. Measured against upstream Envoy: `allowed`
    /// stays 0, not 1. A naive default `Rules { action: ALLOW }` would tick
    /// `allowed` — a STAT divergence with NO body divergence, invisible to a
    /// body-only fixture.
    ///
    /// All four counters are still REGISTERED (at 0), so the stat tree matches.
    #[test]
    fn rules_omitted_is_inert_and_ticks_neither_counter() {
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("norules", None), &reg).unwrap();
        for _ in 0..3 {
            assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue);
        }
        assert_eq!(stat(&reg, "norules.rbac.allowed"), 0, "INERT: allowed must NOT tick");
        assert_eq!(stat(&reg, "norules.rbac.denied"), 0, "INERT: denied must NOT tick");
        assert_eq!(stat(&reg, "norules.rbac.shadow_allowed"), 0);
        assert_eq!(stat(&reg, "norules.rbac.shadow_denied"), 0);
    }

    /// D6 / CF-67-1: all four counters register even with rules present, and the
    /// two shadow counters NEVER tick (shadow policies are not modeled).
    #[test]
    fn shadow_counters_register_at_zero_and_never_tick() {
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("s", Some(&format!("  action: DENY\n{ANY_POLICY}"))), &reg).unwrap();
        for _ in 0..5 {
            let _ = f.on_new_connection(&conn());
        }
        assert_eq!(stat(&reg, "s.rbac.denied"), 5);
        assert_eq!(stat(&reg, "s.rbac.shadow_allowed"), 0);
        assert_eq!(stat(&reg, "s.rbac.shadow_denied"), 0);
    }

    /// D6: counters accumulate across connections.
    #[test]
    fn counters_accumulate_across_connections() {
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("acc", Some(&format!("  action: ALLOW\n{ANY_POLICY}"))), &reg).unwrap();
        for _ in 0..7 {
            assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue);
        }
        assert_eq!(stat(&reg, "acc.rbac.allowed"), 7);
    }
}
```

> `envoy_stats::Counter::value(&self) -> u64` already exists (`crates/envoy-stats/src/counter.rs:33`); verified at PLAN-write. No stats-crate change is needed by this task.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p envoy-bin --bin envoy-bin network_rbac 2>&1 | tee /tmp/t9.log`
Expected: compile error — `file not found for module network_rbac`.

- [ ] **Step 3: Write the engine**

Create `crates/envoy-bin/src/network_rbac.rs`:

```rust
//! `envoy.filters.network.rbac` — the Network-filters family's FIRST
//! NON-TERMINAL filter (phase 67.1, ADR-0128 / ADR-0129).
//!
//! **DO NOT CONFUSE THIS WITH `crates/envoy-filter/src/rbac.rs`**, which
//! implements `envoy.filters.http.rbac` — a DIFFERENT feature that shares the
//! name, operating on HTTP requests rather than L4 connections. The two share
//! only the `Rules` / `Policy` / `Permission` / `Principal` config trees.
//!
//! The filter decides ONCE per connection, at establishment, before any
//! downstream byte is read (phase-67 SPEC R-2, measured against
//! `envoyproxy/envoy:v1.33.0`). It inspects `peer_addr` / `local_addr` only, and
//! never reads the payload — which is why the iteration protocol needs only
//! `on_new_connection` and no `on_data` hook (deferred as CF-67-3).
//!
//! On DENY the filter returns `StopIteration`; `envoy_listener::ChainHandler`
//! then closes the connection with ZERO bytes written and a clean EOF, never an
//! RST (SPEC R-2). This module never touches the socket.
//!
//! Phase 67.1 supports the `any` matcher plus the `and`/`or`/`not` combinators.
//! The connection-level arms (`direct_remote_ip`, `remote_ip`, `source_ip`,
//! `destination_port`, `destination_ip`) land in `67.2`; they are NOT stubbed
//! here — they do not exist, and the config parser rejects them as unknown keys.

use std::sync::Arc;

use envoy_config::{Action, NetworkRbacConfig, Permission, Principal, Rules};
use envoy_listener::{ConnectionInfo, NetworkFilter, NetworkFilterStatus};

pub struct NetworkRbacFilter {
    /// `None` ⇒ the filter is INERT: allow, and tick NEITHER counter
    /// (SPEC R-4, measured). Never materialise a default `Rules` here.
    rules: Option<Rules>,
    allowed: Arc<envoy_stats::Counter>,
    denied: Arc<envoy_stats::Counter>,
}

impl NetworkRbacFilter {
    /// Registers the four `<stat_prefix>.rbac.*` counters. All four register
    /// unconditionally — including when `rules` is `None` — so the stat TREE
    /// matches upstream's shape, which emits all four at 0 for an inert filter
    /// (SPEC R-3, R-4).
    ///
    /// `shadow_allowed` / `shadow_denied` are registered and NEVER incremented:
    /// shadow policies are not modeled, and a `shadow_rules` config field is
    /// rejected loudly by `deny_unknown_fields` (CF-67-1).
    pub fn new(
        cfg: &NetworkRbacConfig,
        registry: &envoy_stats::StatsRegistry,
    ) -> Result<Self, envoy_stats::StatsError> {
        let p = &cfg.stat_prefix;
        let allowed = registry.register_counter(&format!("{p}.rbac.allowed"))?;
        let denied = registry.register_counter(&format!("{p}.rbac.denied"))?;
        registry.register_counter(&format!("{p}.rbac.shadow_allowed"))?;
        registry.register_counter(&format!("{p}.rbac.shadow_denied"))?;
        Ok(Self { rules: cfg.rules.clone(), allowed, denied })
    }
}

impl NetworkFilter for NetworkRbacFilter {
    fn on_new_connection(&self, conn: &ConnectionInfo) -> NetworkFilterStatus {
        // SPEC R-4: `rules` omitted ⇒ INERT. Allow, and tick NOTHING.
        let Some(rules) = self.rules.as_ref() else {
            return NetworkFilterStatus::Continue;
        };
        if engine_allows(rules, conn) {
            self.allowed.inc();
            NetworkFilterStatus::Continue
        } else {
            self.denied.inc();
            NetworkFilterStatus::StopIteration
        }
    }
}

/// Upstream Envoy's RBAC verdict: a policy matches when ANY permission matches
/// AND ANY principal matches; the engine's verdict is `action` when SOME policy
/// matches, and the INVERSE of `action` otherwise.
fn engine_allows(rules: &Rules, conn: &ConnectionInfo) -> bool {
    let matched = rules.policies.values().any(|policy| {
        policy.permissions.iter().any(|p| permission_matches(p, conn))
            && policy.principals.iter().any(|p| principal_matches(p, conn))
    });
    match rules.action {
        Action::Allow => matched,
        Action::Deny => !matched,
    }
}

/// EXHAUSTIVE, no `_ =>` catch-all. `67.2` adds `DestinationPort` /
/// `DestinationIp`; this must fail to compile until they are implemented, which
/// is the GOOD failure mode. **Never add a catch-all.**
///
/// `Any(b) => *b` — `any: false` never matches. Mirrors the landed HTTP RBAC
/// evaluator (`crates/envoy-filter/src/rbac.rs`, `RuntimeMatcher::Any(b) => *b`).
///
/// `Header` / `Metadata` / `UrlPath` are UNREACHABLE: `envoy-config`'s
/// `validate_l4_permission` (67.1 D3, CF-67-4) rejects them at config load. They
/// return `false` rather than panicking — a data-plane path must never panic —
/// with a `debug_assert!` to catch a validator regression in test builds.
fn permission_matches(p: &Permission, conn: &ConnectionInfo) -> bool {
    match p {
        Permission::Any(b) => *b,
        Permission::AndRules(set) => set.rules.iter().all(|c| permission_matches(c, conn)),
        Permission::OrRules(set) => set.rules.iter().any(|c| permission_matches(c, conn)),
        Permission::NotRule(inner) => !permission_matches(inner, conn),
        Permission::Header(_) | Permission::Metadata(_) | Permission::UrlPath(_) => {
            debug_assert!(false, "validate_l4_permission must reject this arm at config load");
            false
        }
    }
}

/// The `Principal` twin of [`permission_matches`]. EXHAUSTIVE, no catch-all:
/// `67.2` adds `DirectRemoteIp` / `RemoteIp` / `SourceIp`.
fn principal_matches(p: &Principal, conn: &ConnectionInfo) -> bool {
    match p {
        Principal::Any(b) => *b,
        Principal::AndIds(set) => set.ids.iter().all(|c| principal_matches(c, conn)),
        Principal::OrIds(set) => set.ids.iter().any(|c| principal_matches(c, conn)),
        Principal::NotId(inner) => !principal_matches(inner, conn),
        Principal::Header(_) | Principal::Metadata(_) | Principal::UrlPath(_) => {
            debug_assert!(false, "validate_l4_principal must reject this arm at config load");
            false
        }
    }
}
```

Add `mod network_rbac;` to `main.rs`'s module list (alphabetical: after `mod echo;`).

`envoy-config` must re-export `Action`, `Permission`, `Principal`, `Rules`, `NetworkRbacConfig` from its crate root. Check `crates/envoy-config/src/lib.rs`'s `pub use bootstrap::{…}` list; add any missing name.

- [ ] **Step 4: Run to verify pass**

```bash
cargo test -p envoy-bin --bin envoy-bin network_rbac 2>&1 | tail -8
```
Expected: `10 passed`.

- [ ] **Step 5: Prove the exhaustiveness guard is real**

Temporarily add `_ => false,` to `permission_matches`. `cargo build -p envoy-bin` still succeeds — and that is the regression `67.2` must never ship. **Remove it**, then confirm `cargo clippy -p envoy-bin --all-targets -- -D warnings` is clean (clippy flags unreachable-pattern only if a real arm is shadowed; the guard here is the *absence* of a catch-all, enforced by review and by the doc comment).

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p envoy-bin --all-targets -- -D warnings
git add crates/envoy-bin/src/network_rbac.rs crates/envoy-bin/src/main.rs crates/envoy-config/src/lib.rs
git commit -m "phase 67.1 task 9: network RBAC engine + 4 counters; rules:None is INERT (D6)"
```

---

## Task 10: `main.rs` chain dispatch + the `filters: []` startup panic (D5, part 3) — **ADR-0130 §2**

**Files:**
- Modify: `crates/envoy-bin/src/main.rs:211-350` (the `if let Some(listener_cfg)` dispatch block)
- Test: `crates/envoy-bin/tests/network_filter_rbac.rs` (created in Task 13; the empty-chain backstop is written there — this task's own test is the unit test below)

**Interfaces:**
- Consumes: Tasks 7-9's `echo::EchoHandler`, `direct_response::DirectResponseHandler`, `network_rbac::NetworkRbacFilter`; Task 6's `envoy_listener::ChainHandler`.
- Produces: `fn build_network_filter_chain(chain: &envoy_config::FilterChain, registry: &envoy_stats::StatsRegistry) -> Result<Vec<Arc<dyn NetworkFilter>>>` and the reshaped dispatch.

**Context — the interlock this task breaks and repairs.**

`main.rs:215-219` today reads only `filter_chains.first().and_then(|c| c.filters.first())`. That was safe **only because** phase 66's terminal validation made every ≥2-filter chain invalid (SPEC R-9). **`rbac` is the first non-terminal filter, so it breaks that interlock**: `[rbac, echo]` is now a valid config whose first filter is not the terminal one.

The new shape: **the terminal filter is the LAST filter** (Task 3's `NetworkFilterChainNotTerminated` guarantees a non-empty chain has one), and everything before it is a non-terminal filter to be built into the chain.

**And the `expect()` on line 219 is a live crash.** Measured this session:

```
$ cargo run -q -p envoy-bin -- -c empty-chain.yaml
thread 'main' panicked at crates/envoy-bin/src/main.rs:219:14:
validator guarantees ≥1 filter
```

The validator does **not** guarantee that. `filters: []` is ACCEPTED (SPEC R-7, measured parity with upstream — the finding that CLOSED **M66-5**). Upstream Envoy accepts the same config and **starts**; envoy-rust crashes. M66-5 closed *config-load* parity; *runtime* parity was never checked and does not hold.

**The fix must not invent unmeasured behavior.** What upstream Envoy does with a *connection* to an empty-chain listener was never probed, and this session does not re-probe. So: log a warning, bind no data listener, and let the admin listener (spawned independently at `main.rs:730`) keep serving. No panic; no guess about connection semantics. The un-probed connect behavior is carried forward as **CF-67-5**. See ADR-0130 §2.

- [ ] **Step 1: Write the failing tests**

Add a `#[cfg(test)] mod tests` to `main.rs` (or extend the existing one) for the pure chain-splitting logic:

```rust
#[cfg(test)]
mod chain_tests {
    use super::*;

    fn chain_from(yaml: &str) -> envoy_config::FilterChain {
        serde_yaml::from_str(yaml).expect("FilterChain parses")
    }

    /// 67.1 D5: `[rbac, echo]` splits into one non-terminal filter + a terminal
    /// `echo`. The terminal filter is the LAST one — never `filters.first()`.
    #[test]
    fn splits_chain_into_non_terminal_prefix_and_terminal_last() {
        let chain = chain_from(
            "filters:\n  - name: envoy.filters.network.rbac\n    typed_config:\n      \"@type\": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC\n      stat_prefix: sp\n  - name: envoy.filters.network.echo\n",
        );
        let (prefix, terminal) = split_chain(&chain).expect("non-empty chain has a terminal");
        assert_eq!(prefix.len(), 1);
        assert_eq!(prefix[0].name, envoy_config::NETWORK_RBAC_FILTER);
        assert_eq!(terminal.name, envoy_config::ECHO_FILTER);
    }

    /// 67.1 D5: a lone terminal filter yields an EMPTY prefix — the pre-67.1 shape.
    #[test]
    fn lone_terminal_filter_yields_empty_prefix() {
        let chain = chain_from("filters:\n  - name: envoy.filters.network.echo\n");
        let (prefix, terminal) = split_chain(&chain).expect("terminal present");
        assert!(prefix.is_empty());
        assert_eq!(terminal.name, envoy_config::ECHO_FILTER);
    }

    /// 67.1 D5 / SPEC R-7 / ADR-0130 §2: an EMPTY chain has no terminal filter.
    /// `split_chain` returns None; the caller must NOT panic. envoy-rust used to
    /// crash here (`main.rs:219`, `validator guarantees ≥1 filter`) on a config
    /// upstream Envoy ACCEPTS and STARTS.
    #[test]
    fn empty_chain_has_no_terminal_and_does_not_panic() {
        let chain = chain_from("filters: []\n");
        assert!(split_chain(&chain).is_none());
    }

    /// 67.1 D5: `build_network_filter_chain` constructs one `NetworkFilter` per
    /// non-terminal filter and registers its counters.
    #[test]
    fn builds_a_network_rbac_filter_from_the_prefix() {
        let chain = chain_from(
            "filters:\n  - name: envoy.filters.network.rbac\n    typed_config:\n      \"@type\": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC\n      stat_prefix: built\n      rules:\n        action: DENY\n        policies:\n          p0:\n            permissions: [{ any: true }]\n            principals: [{ any: true }]\n  - name: envoy.filters.network.echo\n",
        );
        let registry = envoy_stats::StatsRegistry::new();
        let (prefix, _) = split_chain(&chain).unwrap();
        let filters = build_network_filter_chain(&prefix, &registry).expect("builds");
        assert_eq!(filters.len(), 1);
        // Counters registered at construction, at 0.
        assert_eq!(registry.register_counter("built.rbac.denied").unwrap().value(), 0);
        assert_eq!(registry.register_counter("built.rbac.shadow_allowed").unwrap().value(), 0);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p envoy-bin --bin envoy-bin chain_tests 2>&1 | tee /tmp/t10.log`
Expected: compile error — `cannot find function split_chain` / `build_network_filter_chain`.

- [ ] **Step 3: Write the two helpers**

In `crates/envoy-bin/src/main.rs`, near `bind_and_spawn_listener` (`:854`):

```rust
/// 67.1 D5: split a network filter chain into its NON-TERMINAL prefix and its
/// TERMINAL last filter.
///
/// Before 67.1, `main` read `filters.first()` and ignored the rest — safe ONLY
/// because phase 66's terminal validation made every ≥2-filter chain invalid.
/// `envoy.filters.network.rbac` is the first NON-terminal filter, so that
/// interlock is gone: the terminal filter is the LAST one, and everything before
/// it is a filter to run per-connection.
///
/// `envoy-config`'s `NetworkFilterChainNotTerminated` rule (67.1 D2) guarantees
/// a NON-EMPTY chain ends in a terminal filter. An EMPTY `filters: []` chain is
/// ACCEPTED (SPEC R-7, upstream parity, closes M66-5) and has no terminal filter
/// at all — hence the `Option`. Returning `None` is NOT an error; see the caller.
fn split_chain(
    chain: &envoy_config::FilterChain,
) -> Option<(Vec<&envoy_config::NetworkFilter>, &envoy_config::NetworkFilter)> {
    let (terminal, prefix) = chain.filters.split_last()?;
    Some((prefix.iter().collect(), terminal))
}

/// 67.1 D5: construct one `envoy_listener::NetworkFilter` per non-terminal
/// filter, in configured order, registering its stats.
///
/// `envoy.filters.network.rbac` is the only non-terminal filter envoy-rust
/// supports today. The `_` arm is unreachable: the config validator's per-filter
/// match rejects every unknown name with `ConfigError::UnsupportedFilter`, and
/// every OTHER known name is terminal (so it would be the chain's last filter and
/// never appear in the prefix).
fn build_network_filter_chain(
    prefix: &[&envoy_config::NetworkFilter],
    registry: &envoy_stats::StatsRegistry,
) -> Result<Vec<std::sync::Arc<dyn envoy_listener::NetworkFilter>>> {
    let mut out: Vec<std::sync::Arc<dyn envoy_listener::NetworkFilter>> =
        Vec::with_capacity(prefix.len());
    for filter in prefix {
        match filter.name.as_str() {
            envoy_config::NETWORK_RBAC_FILTER => {
                let Some(envoy_config::TypedConfig::NetworkRbac(cfg)) = filter.typed_config.as_ref()
                else {
                    anyhow::bail!(
                        "validator guarantees a NetworkRbac typed_config on {}",
                        envoy_config::NETWORK_RBAC_FILTER
                    );
                };
                out.push(std::sync::Arc::new(
                    network_rbac::NetworkRbacFilter::new(cfg, registry)
                        .with_context(|| format!("registering stats for {}", cfg.stat_prefix))?,
                ));
            }
            other => anyhow::bail!(
                "non-terminal network filter '{other}' is not supported; \
                 the envoy-config validator should have rejected it at parse time",
            ),
        }
    }
    Ok(out)
}

/// 67.1 D5: wrap `inner` in the chain's non-terminal filters, if any. An empty
/// prefix returns `inner` untouched, so a lone-terminal-filter chain pays no
/// per-connection cost (no `peer_addr()`/`local_addr()` syscalls).
fn wrap_in_chain(
    filters: Vec<std::sync::Arc<dyn envoy_listener::NetworkFilter>>,
    inner: std::sync::Arc<dyn envoy_listener::ConnectionHandler>,
) -> std::sync::Arc<dyn envoy_listener::ConnectionHandler> {
    if filters.is_empty() {
        inner
    } else {
        std::sync::Arc::new(envoy_listener::ChainHandler::new(filters, inner))
    }
}
```

- [ ] **Step 4: Reshape the dispatch block**

Replace `main.rs:211-241`'s head. The `expect()` is deleted; the `match` now keys on the **terminal** filter's name; every arm ends by handing its `Arc<dyn ConnectionHandler>` through `wrap_in_chain` into `bind_and_spawn_listener`.

```rust
    if let Some(listener_cfg) = bootstrap.all_listeners().next() {
        let Some(chain) = listener_cfg.filter_chains.first() else {
            anyhow::bail!("listener {:?} has no filter_chains", listener_cfg.name);
        };

        // 67.1 D5 (ADR-0130 §2): an EMPTY `filters: []` chain is ACCEPTED by the
        // config validator — measured parity with upstream Envoy, which accepts
        // and STARTS on the same config (phase-67 SPEC R-7, the finding that
        // closed M66-5). envoy-rust used to PANIC here
        // (`validator guarantees ≥1 filter`). It now binds no data listener and
        // warns; the admin listener, spawned independently below, still serves.
        //
        // What upstream Envoy does with a CONNECTION to such a listener has not
        // been probed, so envoy-rust asserts nothing about it. Carried forward as
        // CF-67-5. Recorded in BEHAVIOR_CONTRACT.md as a divergence with no
        // differential observable — no fixture configures an empty chain.
        let Some((prefix, terminal)) = split_chain(chain) else {
            tracing::warn!(
                listener = %listener_cfg.name,
                "filter chain is empty; binding no data listener (upstream Envoy accepts \
                 this config and starts — see CF-67-5)"
            );
            return finish_without_data_listener(bootstrap, registry, token, drain, set).await;
        };

        let sock = &listener_cfg.address.socket_address;
        let bind_addr: SocketAddr = format!("{}:{}", sock.address, sock.port_value)
            .parse()
            .with_context(|| {
                format!("parsing listener address {}:{}", sock.address, sock.port_value)
            })?;

        let listener_concurrency = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);

        // 67.1 D5: the non-terminal prefix, built once at startup and shared by
        // every accepted connection.
        let chain_filters = build_network_filter_chain(&prefix, &registry)?;

        match terminal.name.as_str() {
            envoy_config::ECHO_FILTER => { /* … see below … */ }
            envoy_config::DIRECT_RESPONSE_FILTER => { /* … */ }
            envoy_config::TCP_PROXY_FILTER => { /* … */ }
            envoy_config::HCM_FILTER => { /* … */ }
            other => anyhow::bail!("unsupported terminal network filter '{other}'"),
        }
    }
```

> **Do not invent `finish_without_data_listener`.** The dispatch block is `if let Some(listener_cfg) = …`, followed by the admin spawn (`:730`) and the task-set join (`:764`). The empty-chain path must simply **skip the dispatch** and fall through. Restructure the `let Some((prefix, terminal)) = split_chain(chain) else { … }` as an `if let Some((prefix, terminal)) = split_chain(chain) { … dispatch … } else { tracing::warn!(…); }` inside the existing block. No new function, no early return.

Each arm changes only in how it reaches `bind_and_spawn_listener`:

```rust
            envoy_config::ECHO_FILTER => {
                let handler = wrap_in_chain(
                    chain_filters,
                    std::sync::Arc::new(echo::EchoHandler),
                );
                bind_and_spawn_listener(
                    listener_cfg, handler, &registry, listener_concurrency, "echo", bind_addr,
                    || tracing::info!(addr = %bind_addr, "envoy-rust listening (echo)"),
                    &token, &drain, &mut set,
                ).await?;
            }
```

`DIRECT_RESPONSE_FILTER` is identical modulo `DirectResponseHandler::new(payload)` (read `terminal.typed_config`, not `filter.typed_config`).

`TCP_PROXY_FILTER` keeps its existing body verbatim — the cluster lookup, the per-cluster upstream-TLS dispatch, the three-way downstream-TLS dispatch — and changes only its last two statements:

```rust
                let handler: std::sync::Arc<dyn envoy_listener::ConnectionHandler> =
                    match downstream_tls {
                        Some(tls) => std::sync::Arc::new(tls_handler::TlsAcceptingHandler { tls, inner: proxy }),
                        None => proxy,
                    };
                // 67.1: the chain runs on the raw TcpStream, BEFORE the TLS
                // handshake. For `any` (67.1) and the peer/local-address arms
                // (67.2) the verdict is identical either way — TLS alters neither
                // address.
                let handler = wrap_in_chain(chain_filters, handler);
                bind_and_spawn_listener(listener_cfg, handler, /* … unchanged … */).await?;
```

`HCM_FILTER` similarly: build its handler exactly as today, then `wrap_in_chain(chain_filters, handler)` immediately before `bind_and_spawn_listener`. **The thread-per-core `bind_per_worker` path (`main.rs:~490`) builds one handler per worker** — wrap each of them, cloning `chain_filters` per shard (`Arc<dyn NetworkFilter>` is cheap to clone; the counters are shared `Arc<Counter>`s, so N shards tick ONE counter set, which is correct).

All four arms read `terminal.typed_config`, never `filter.typed_config`. Delete the now-unused `let filter = …` binding.

- [ ] **Step 5: Run to verify pass, and prove the panic is gone**

```bash
cargo test -p envoy-bin --bin envoy-bin chain_tests 2>&1 | tail -5
```
Expected: `4 passed`.

Then reproduce §2's crash and confirm it is fixed:

```bash
cat > /tmp/empty-chain.yaml <<'EOF'
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters: []
EOF
timeout 5 cargo run -q -p envoy-bin -- -c /tmp/empty-chain.yaml 2>&1 | head -5
```
Expected: a `filter chain is empty; binding no data listener` warning and a clean run until the timeout. **No `panicked at` line.** Before this task, the same command printed `thread 'main' panicked at crates/envoy-bin/src/main.rs:219:14`.

Then the full `envoy-bin` suite and the two raw-TCP fixtures:

```bash
cargo build -p envoy-bin
cargo test -p envoy-bin --no-fail-fast 2>&1 | tail -20
cargo test -p differential --test tcp_echo --test network_filter_direct_response 2>&1 | tail -20
```
Expected: fixtures `0001` and `0071` green; `envoy-bin`'s known environmental REDs (`admin_config_dump_server_info`) unchanged.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p envoy-bin --all-targets -- -D warnings
git add crates/envoy-bin/src/main.rs
git commit -m "phase 67.1 task 10: main.rs chain dispatch; fix filters:[] startup panic [ADR-0130]"
```

---

## Task 11: `expected_stats` on the raw-TCP driver family (D7)

**Files:**
- Modify: `tests/differential/src/lib.rs` — the `Driver` enum (`:39`), `port_key` (`:2861-2908`), `needs_admin_port` (`:2922`), the dispatch `match` (`:3903`), the `Http1KeepAlive` arm's scrape loop (`:4763-4784`), the `Http2KeepAlive` arm's scrape loop
- Test: `tests/differential/src/lib.rs` (`#[cfg(test)] mod tests`, beside the `parses_tcp_direct_response_driver` test at `:6815`)

**Interfaces:**
- Consumes: the existing `drive_tcp` (`:1679`), `drive_tcp_direct_response` (`:1708`), `scrape_admin_stat` (`:2593`), `wait_accept_ready`, `KeepAliveExpectedStat` (`:594`).
- Produces:
  - `Driver::TcpWithStats { probe: TcpProbeKind, settle_ms: u64, expected_stats: Vec<KeepAliveExpectedStat> }`
  - `pub enum TcpProbeKind { Echo, ReadToEof }` (`#[serde(rename_all = "snake_case")]`)
  - `async fn assert_expected_stats_bilaterally(upstream_admin: SocketAddr, subject_admin: SocketAddr, expected: &[KeepAliveExpectedStat]) -> Result<()>`

**Context — why this is a hard requirement, not polish (SPEC R-8).**

`assert_body_rule`'s `ByteExact` is a bare `if envoy_body != rust_body { bail! }` (`tests/differential/src/lib.rs:6461`). A DENY fixture that asserts only *"both proxies returned zero bytes"* therefore **passes vacuously even if envoy-rust never implemented RBAC and simply failed to write.** `expected_stats` exists on **only three** `Driver` variants, all HTTP (`Http1AfterSettle`, `Http1KeepAlive`, `Http2KeepAlive`). The four raw-TCP/TLS variants carry **none**. Fixture `0071` escaped the trap only by carrying a non-empty payload; fixture `0072` cannot.

**`scrape_admin_stat` returns `Ok(0)` for an absent stat name.** That is what makes `<stat_prefix>.rbac.denied == 1` a real witness: if envoy-rust never registers the counter, the scrape yields 0 and the assertion fails. Conversely `.allowed == 0` passes vacuously if the name is absent — it is a consistency check, not the witness. Say so in the fixture READMEs.

**Do not add fields to `Driver::TcpEcho` / `Driver::TcpDirectResponse`.** They are serde **unit** variants (`driver: { kind: tcp_echo }`); adding `#[serde(default)]` fields turns them into struct variants, breaking every landed `expectations.yaml` and five `matches!(e.driver, Driver::TcpEcho)` parse tests (`:6800`, `:7859`, `:8143`, `:8319`).

**`needs_admin_port` does double duty.** It gates the subject's host-port reservation (`:2927`) AND is passed to `upstream::start` as `expose_admin_port` (`:3836`) AND injects `ADMIN_PORT = upstream::ADMIN_CONTAINER_PORT` into the upstream template (`:3440-3445`). Adding the new variant to that one `matches!` wires both sides. It additionally requires `{{ADMIN_PORT}}` to appear in a template — so **both fixtures need an `admin:` block on both sides**.

- [ ] **Step 1: Write the failing tests**

```rust
    /// 67.1 D7: the raw-TCP driver family gains `expected_stats`. Echo probe.
    #[test]
    fn parses_tcp_with_stats_echo_driver() {
        let yaml = r#"
driver:
  kind: tcp_with_stats
  probe: echo
  settle_ms: 500
  expected_stats:
    - { name: rbac_allow.rbac.allowed, value: 1 }
    - { name: rbac_allow.rbac.denied, value: 0 }
equivalence:
  response_body:
    kind: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        let Driver::TcpWithStats { probe, settle_ms, expected_stats } = &e.driver else {
            panic!("expected TcpWithStats, got {:?}", e.driver);
        };
        assert_eq!(*probe, TcpProbeKind::Echo);
        assert_eq!(*settle_ms, 500);
        assert_eq!(expected_stats.len(), 2);
        assert_eq!(expected_stats[0].name, "rbac_allow.rbac.allowed");
        assert_eq!(expected_stats[0].value, 1);
    }

    /// 67.1 D7: read-to-EOF probe — the DENY shape (send nothing, read to EOF).
    #[test]
    fn parses_tcp_with_stats_read_to_eof_driver() {
        let yaml = r#"
driver:
  kind: tcp_with_stats
  probe: read_to_eof
  settle_ms: 500
  expected_stats:
    - { name: rbac_deny.rbac.denied, value: 1 }
equivalence:
  response_body:
    kind: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        let Driver::TcpWithStats { probe, .. } = &e.driver else { panic!("expected TcpWithStats") };
        assert_eq!(*probe, TcpProbeKind::ReadToEof);
    }

    /// 67.1 D7: `TcpWithStats` needs an admin port on BOTH sides — the whole
    /// point of the variant. It uses the `{{PORT}}` data-listener convention.
    #[test]
    fn tcp_with_stats_needs_admin_port_and_uses_port_key() {
        let driver = Driver::TcpWithStats {
            probe: TcpProbeKind::ReadToEof,
            settle_ms: 0,
            expected_stats: vec![],
        };
        assert_eq!(port_key_for(&driver), "PORT");
        assert!(driver_needs_admin_port(&driver));
        // The pre-existing raw-TCP drivers still do NOT.
        assert!(!driver_needs_admin_port(&Driver::TcpEcho));
        assert!(!driver_needs_admin_port(&Driver::TcpDirectResponse));
    }

    /// 67.1 D7: the pre-existing UNIT variants still deserialize from a bare
    /// `kind:` with no fields. Adding fields to them would have broken every
    /// landed expectations.yaml.
    #[test]
    fn unit_raw_tcp_drivers_still_parse_without_fields() {
        let e: Expectations = serde_yaml::from_str("driver:\n  kind: tcp_echo\n").expect("parses");
        assert!(matches!(e.driver, Driver::TcpEcho));
        let e: Expectations =
            serde_yaml::from_str("driver:\n  kind: tcp_direct_response\n").expect("parses");
        assert!(matches!(e.driver, Driver::TcpDirectResponse));
    }
```

> `port_key_for` and `driver_needs_admin_port` do not exist yet. Extract them from the inline `match` at `:2861` and the inline `matches!` at `:2922` into two small free functions so they are testable, and call them from `run_fixture`. That extraction is part of Step 3.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p differential --lib tcp_with_stats unit_raw_tcp 2>&1 | tee /tmp/t11.log`
Expected: compile error — `no variant TcpWithStats`, `cannot find type TcpProbeKind`, `cannot find function port_key_for`.

- [ ] **Step 3: Add the variant, the probe enum, and the two extracted predicates**

In `tests/differential/src/lib.rs`, after `Driver::TcpDirectResponse` (`:48`):

```rust
    /// 67.1 D7 (phase-67 SPEC R-8): a raw-TCP probe WITH a post-settle bilateral
    /// admin-stat scrape — the first `expected_stats` on any non-HTTP driver.
    ///
    /// **This variant exists because `ByteExact` cannot witness a DENY.**
    /// `assert_body_rule`'s `ByteExact` is a bare `envoy_body != rust_body`
    /// check, so a fixture asserting "both proxies returned zero bytes" passes
    /// vacuously even if envoy-rust never implemented the filter and simply
    /// failed to write. The stats assertion is what makes fixture `0072` a
    /// witness rather than a vacuous pass.
    ///
    /// `probe` selects the wire shape; both reuse the existing raw-TCP drivers:
    ///   - `echo`        → `drive_tcp` (write `inputs/payload.bin`, read-exact,
    ///                     ADR-0007 trailing-byte poll). Fixture `0073`.
    ///   - `read_to_eof` → `drive_tcp_direct_response` (send nothing, read to
    ///                     EOF). Fixture `0072`.
    ///
    /// Requires `{{ADMIN_PORT}}` on BOTH sides (see `driver_needs_admin_port`).
    TcpWithStats {
        probe: TcpProbeKind,
        #[serde(default)]
        settle_ms: u64,
        #[serde(default)]
        expected_stats: Vec<KeepAliveExpectedStat>,
    },
```

And beside `KeepAliveExpectedStat` (`:594`):

```rust
/// 67.1 D7: which raw-TCP wire shape `Driver::TcpWithStats` drives. Both arms
/// delegate to a pre-existing driver function; no new wire driver is written.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TcpProbeKind {
    /// Write `inputs/payload.bin`, read exactly that many bytes back, then poll
    /// for trailing bytes (ADR-0006 / ADR-0007). The ALLOW shape.
    Echo,
    /// Send nothing; read to EOF. The DENY shape — a compliant peer writes zero
    /// bytes and half-closes.
    ReadToEof,
}
```

Extract the two predicates (replacing the inline `match` at `:2861` and `matches!` at `:2922`):

```rust
/// Which `{{…}}` token the fixture's data listener port substitutes into.
fn port_key_for(driver: &Driver) -> &'static str {
    match driver {
        Driver::HttpGet { .. } => "ADMIN_PORT",
        // Every other driver — including 67.1's `TcpWithStats` — drives a
        // `{{PORT}}` data listener; the admin listener is separately wired via
        // `{{ADMIN_PORT}}` (see `driver_needs_admin_port`).
        _ => "PORT",
    }
}

/// Does this driver need an admin listener exposed on BOTH proxies?
///
/// Gates three things at once: the subject's host admin-port reservation, the
/// upstream container's `expose_admin_port` (which maps `ADMIN_CONTAINER_PORT`),
/// and the `ADMIN_PORT` kv injected into the upstream template. `run_fixture`
/// ALSO requires `{{ADMIN_PORT}}` to appear in one of the templates.
///
/// 67.1 D7: `TcpWithStats` joins the three HTTP keep-alive/scrape drivers here —
/// its post-settle bilateral stat scrape is the whole reason it exists.
fn driver_needs_admin_port(driver: &Driver) -> bool {
    matches!(
        driver,
        Driver::AdminScrape { .. }
            | Driver::Http1KeepAlive { .. }
            | Driver::Http2KeepAlive { .. }
            | Driver::TcpWithStats { .. }
    )
}
```

> `port_key_for`'s `_ => "PORT"` collapses the ~20-arm explicit list at `:2861`. **Keep the explicit list** if you prefer — but then you MUST add `Driver::TcpWithStats { .. }` to it, and a future driver that forgets will silently get `"PORT"` anyway under the catch-all. The explicit list's value was that `HttpGet` is the lone exception; the catch-all makes that legible. Either is acceptable; do not do both.

Rewire `run_fixture`:

```rust
    let port_key = port_key_for(&expectations.driver);
    let needs_admin_port = driver_needs_admin_port(&expectations.driver)
        && (upstream_template.contains("{{ADMIN_PORT}}")
            || subject_template.contains("{{ADMIN_PORT}}"));
```

- [ ] **Step 4: Extract the bilateral scrape, and write the dispatch arm**

Extract from the `Http1KeepAlive` arm (`:4763-4784`), and call it from `Http1KeepAlive`, `Http2KeepAlive` and the new arm:

```rust
/// 13.1 D10, hoisted at 67.1 D7: scrape each named stat from BOTH admin
/// listeners and assert each side's value equals `stat.value` independently.
/// Cross-side consistency follows by transitivity.
///
/// `scrape_admin_stat` returns `Ok(0)` for a stat name the proxy never
/// registered. A `value: 0` assertion therefore passes vacuously when the name
/// is ABSENT; only a non-zero assertion is a real witness. Fixture READMEs must
/// say which of their assertions is the witness.
async fn assert_expected_stats_bilaterally(
    upstream_admin_addr: SocketAddr,
    subject_admin_addr: SocketAddr,
    expected_stats: &[KeepAliveExpectedStat],
) -> Result<()> {
    for stat in expected_stats {
        let upstream_value = scrape_admin_stat(upstream_admin_addr, &stat.name)
            .await
            .with_context(|| format!("upstream scraping stat {}", stat.name))?;
        let subject_value = scrape_admin_stat(subject_admin_addr, &stat.name)
            .await
            .with_context(|| format!("subject scraping stat {}", stat.name))?;
        anyhow::ensure!(
            upstream_value == stat.value,
            "upstream stat {} expected {} got {}", stat.name, stat.value, upstream_value,
        );
        anyhow::ensure!(
            subject_value == stat.value,
            "subject stat {} expected {} got {}", stat.name, stat.value, subject_value,
        );
    }
    Ok(())
}
```

Then the arm itself, modelled on `run_tcp_direct_response_arm` (`:4272`) plus `Http1KeepAlive`'s admin plumbing (`:4653-4670`):

```rust
/// `Driver::TcpWithStats` arm of `run_fixture` (67.1 D7). Drives ONE raw-TCP
/// probe against each proxy via a pre-existing driver, asserts body equivalence
/// through the standard cascade, then settles and scrapes both admin listeners.
async fn run_tcp_with_stats_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    probe: &TcpProbeKind,
    settle_ms: &u64,
    expected_stats: &[KeepAliveExpectedStat],
) -> Result<()> {
    let FixtureCtx {
        fixture_dir, expectations, upstream_addr, subject_addr, admin_host_port, budget, ..
    } = *ctx;

    let upstream_admin_port = upstream.host_admin_port().ok_or_else(|| {
        anyhow::anyhow!(
            "Driver::TcpWithStats requires the upstream container to expose its admin port; \
             either the fixture's envoy.yaml does not reference {{ADMIN_PORT}} or the harness \
             failed to wire `expose_admin_port = true`",
        )
    })?;
    let subject_admin_port = admin_host_port.ok_or_else(|| {
        anyhow::anyhow!(
            "Driver::TcpWithStats requires the subject's envoy-rust.yaml to reference \
             {{ADMIN_PORT}}; the harness only reserves a host admin port when one of the \
             templates contains the marker",
        )
    })?;
    let upstream_admin_addr: SocketAddr = format!("127.0.0.1:{upstream_admin_port}").parse()?;
    let subject_admin_addr: SocketAddr = format!("127.0.0.1:{subject_admin_port}").parse()?;
    wait_accept_ready(upstream_admin_addr, budget)
        .await
        .context("upstream admin listener never became accept-ready")?;
    wait_accept_ready(subject_admin_addr, budget)
        .await
        .context("envoy-rust admin listener never became accept-ready")?;

    let (upstream_out, subject_out) = match probe {
        TcpProbeKind::Echo => {
            let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
                .context("reading payload.bin")?;
            (
                drive_tcp(upstream_addr, &payload).await.context("upstream envoy drive")?,
                drive_tcp(subject_addr, &payload).await.context("envoy-rust drive")?,
            )
        }
        TcpProbeKind::ReadToEof => (
            drive_tcp_direct_response(upstream_addr).await.context("upstream envoy drive")?,
            drive_tcp_direct_response(subject_addr).await.context("envoy-rust drive")?,
        ),
    };

    // Single post-probe settle, covering stat-write visibility on BOTH sides
    // under the same Relaxed-ordering budget.
    tokio::time::sleep(Duration::from_millis(*settle_ms)).await;
    assert_expected_stats_bilaterally(upstream_admin_addr, subject_admin_addr, expected_stats)
        .await?;

    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    assert_equivalence(expectations, None, None, &upstream_out, &subject_out)?;
    Ok(())
}
```

> **Scrape BEFORE shutting the subject down.** `subject.shutdown()` kills the envoy-rust process and its admin listener with it. `run_tcp_direct_response_arm` shuts down before `assert_equivalence` because it needs no admin access; this arm must not copy that order.

And the dispatch entry, after the `Driver::TcpDirectResponse` arm (`:3907`):

```rust
        Driver::TcpWithStats { probe, settle_ms, expected_stats } => {
            run_tcp_with_stats_arm(&ctx, upstream, subject, probe, settle_ms, expected_stats).await?;
        }
```

- [ ] **Step 5: Run to verify pass**

```bash
cargo test -p differential --lib 2>&1 | tail -8
```
Expected: all parse tests pass, +4. (The Docker-gated fixture tests are Task 12.)

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p differential --all-targets -- -D warnings
git add tests/differential/src/lib.rs
git commit -m "phase 67.1 task 11: Driver::TcpWithStats — expected_stats on the raw-TCP driver family (D7)"
```

---

## Task 12: Fixtures `0072` (DENY) + `0073` (ALLOW) (D8)

**Files:**
- Create: `tests/fixtures/0072-network-filter-rbac-deny/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- Create: `tests/fixtures/0073-network-filter-rbac-allow/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md,inputs/payload.bin}`
- Create: `tests/differential/tests/network_filter_rbac_deny.rs`, `tests/differential/tests/network_filter_rbac_allow.rs`

**Interfaces:**
- Consumes: Task 11's `Driver::TcpWithStats` + `TcpProbeKind`; Tasks 1-10's full config→runtime path.
- Produces: two green differential fixtures. **This is `67.1`'s entire differential surface.**

**Context — the `any: true`-only lock, and why.**

Both fixtures use `permissions: [{ any: true }]` / `principals: [{ any: true }]` **only**. This is deliberate and locked by ADR-0128 decision (iv):

- `direct_remote_ip` would see the **Docker bridge address** — `192.168.65.2` on this dev host (memory `differential-host-bridge-ip-192-168-65-2`), and something else on CI.
- `destination_port` would have to match a `{{PORT}}` that **differs between the two proxies** by construction (upstream Envoy listens on `CONTAINER_PORT = 10000` inside its container; the subject listens on a host-reserved ephemeral port).

Neither is host-deterministic under the Docker harness. Every IP/port matcher is covered **in-process** in `67.2`, bound to `127.0.0.1` with a known port.

`action: ALLOW` / `action: DENY` over `any: true` **completely** witnesses both decision paths, both counters, and the whole iteration protocol. Nothing in `67.1` is a stub.

**Both fixtures need an `admin:` block on both sides** — `driver_needs_admin_port` gates on `{{ADMIN_PORT}}` appearing in a template. The upstream side binds `0.0.0.0:{{ADMIN_PORT}}` (substituted to `ADMIN_CONTAINER_PORT = 9901`, host-mapped by testcontainers); the subject binds `127.0.0.1:{{ADMIN_PORT}}` (a host-reserved ephemeral port).

**`0073` is the family's first differential proof that a non-terminal filter runs and then YIELDS to the terminal filter** — i.e. of the iteration protocol itself.

- [ ] **Step 1: Write the failing differential tests**

`tests/differential/tests/network_filter_rbac_deny.rs`:

```rust
use std::path::Path;

/// Phase 67.1 fixture 0072: `[rbac(action: DENY, any), echo]`.
///
/// The DENY path is VACUITY-PRONE: `ByteExact` is a bare inequality check, so
/// "both proxies returned zero bytes" would pass even if envoy-rust never
/// implemented RBAC and simply failed to write. The `rbac.denied == 1`
/// assertion in `expectations.yaml` is what makes this a witness.
#[tokio::test]
async fn network_filter_rbac_deny_fixture() -> anyhow::Result<()> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/0072-network-filter-rbac-deny");
    differential::run_fixture(&fixture).await
}
```

`tests/differential/tests/network_filter_rbac_allow.rs`:

```rust
use std::path::Path;

/// Phase 67.1 fixture 0073: `[rbac(action: ALLOW, any), echo]`.
///
/// The family's first differential proof that a NON-TERMINAL network filter runs
/// and then YIELDS to the terminal filter — i.e. of the chain iteration protocol
/// itself. The payload round-trips byte-exact through the terminal `echo`.
#[tokio::test]
async fn network_filter_rbac_allow_fixture() -> anyhow::Result<()> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/0073-network-filter-rbac-allow");
    differential::run_fixture(&fixture).await
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo build -p envoy-bin   # ALWAYS before a differential run
cargo test -p differential --test network_filter_rbac_deny 2>&1 | tail -20
```
Expected: FAIL — fixture directory does not exist.

- [ ] **Step 3: Write fixture `0072` (DENY)**

`tests/fixtures/0072-network-filter-rbac-deny/envoy.yaml`:

```yaml
admin:
  address:
    socket_address:
      address: 0.0.0.0
      port_value: {{ADMIN_PORT}}
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: rbac_deny
                rules:
                  action: DENY
                  policies:
                    deny_all:
                      permissions: [{ any: true }]
                      principals: [{ any: true }]
            - name: envoy.filters.network.echo
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.echo.v3.Echo
```

`tests/fixtures/0072-network-filter-rbac-deny/envoy-rust.yaml` — identical **except** the bind address and the `echo` filter's `typed_config`, which envoy-rust forbids (the pre-existing ADR-0014 YAML shim; upstream Envoy REQUIRES it):

```yaml
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {{ADMIN_PORT}}
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: rbac_deny
                rules:
                  action: DENY
                  policies:
                    deny_all:
                      permissions: [{ any: true }]
                      principals: [{ any: true }]
            - name: envoy.filters.network.echo
```

`tests/fixtures/0072-network-filter-rbac-deny/expectations.yaml`:

```yaml
# Phase 67.1 fixture 0072 — network RBAC, action: DENY.
#
# THE STATS ASSERTION IS THE WITNESS. `equivalence.response_body.byte_exact` is a
# bare `envoy_body != rust_body` check, so "both proxies returned zero bytes"
# passes VACUOUSLY even against an envoy-rust that never implemented RBAC and
# simply failed to write. `rbac_deny.rbac.denied == 1` is the assertion that
# cannot pass without a working filter: `scrape_admin_stat` returns 0 for a stat
# name the proxy never registered.
#
# `rbac_deny.rbac.allowed == 0` and the two shadow counters are CONSISTENCY
# checks, not witnesses — they pass vacuously on an absent name.
#
# `probe: read_to_eof` sends nothing and reads to EOF. On DENY both proxies
# write zero bytes and close with a clean EOF, never an RST (SPEC R-2, measured
# against envoyproxy/envoy:v1.33.0).
driver:
  kind: tcp_with_stats
  probe: read_to_eof
  settle_ms: 500
  expected_stats:
    - { name: rbac_deny.rbac.denied, value: 1 }
    - { name: rbac_deny.rbac.allowed, value: 0 }
    - { name: rbac_deny.rbac.shadow_allowed, value: 0 }
    - { name: rbac_deny.rbac.shadow_denied, value: 0 }
equivalence:
  response_body:
    kind: byte_exact
```

`README.md` — state, in prose: what the fixture asserts; that `any: true` is locked (bridge address / differing `{{PORT}}`); which assertion is the witness and which are vacuity-prone; that the post-EOF-write acceptance (ADR-0124's drain, applied to the DENY close) has **no differential observable** here and is pinned in-process by `close_with_drain_sends_clean_eof_and_accepts_post_eof_writes` and by `tests/network_filter_rbac.rs`; and that the `echo` `typed_config` asymmetry between the two sides is the pre-existing ADR-0014 shim, not a `67.1` divergence.

- [ ] **Step 4: Write fixture `0073` (ALLOW)**

Same two YAMLs with `stat_prefix: rbac_allow` and `action: ALLOW`, plus:

`tests/fixtures/0073-network-filter-rbac-allow/inputs/payload.bin` — the bytes `PING-RBAC\n` (10 bytes; matches the recon probe).

```bash
printf 'PING-RBAC\n' > tests/fixtures/0073-network-filter-rbac-allow/inputs/payload.bin
```

`expectations.yaml`:

```yaml
# Phase 67.1 fixture 0073 — network RBAC, action: ALLOW.
#
# The family's FIRST differential proof that a NON-TERMINAL network filter runs
# and then YIELDS to the terminal filter — i.e. of the chain iteration protocol.
# The payload round-trips byte-exact through the terminal `echo`.
#
# Unlike 0072, the body assertion here is NOT vacuity-prone: a non-empty payload
# must come back byte-for-byte. `rbac_allow.rbac.allowed == 1` additionally
# proves the filter RAN rather than being skipped, which the body alone cannot
# distinguish from a chain that ignored the rbac filter entirely.
driver:
  kind: tcp_with_stats
  probe: echo
  settle_ms: 500
  expected_stats:
    - { name: rbac_allow.rbac.allowed, value: 1 }
    - { name: rbac_allow.rbac.denied, value: 0 }
    - { name: rbac_allow.rbac.shadow_allowed, value: 0 }
    - { name: rbac_allow.rbac.shadow_denied, value: 0 }
equivalence:
  response_body:
    kind: byte_exact
```

`README.md` — as above, plus: the `allowed == 1` assertion is what distinguishes "the rbac filter ran and returned Continue" from "main.rs ignored every filter but the terminal one", which is exactly the pre-`67.1` behavior (SPEC R-9). Without it the body assertion alone would pass against the old `filters.first()` dispatch.

- [ ] **Step 5: Run both fixtures**

```bash
cargo build -p envoy-bin                     # NEW config key + NEW filter name: a stale binary REDs
cargo test -p differential --test network_filter_rbac_deny --test network_filter_rbac_allow 2>&1 | tail -30
```
Expected: both green.

Then confirm `0073`'s `allowed == 1` is a real witness: temporarily make `wrap_in_chain` return `inner` unconditionally (i.e. never build the chain). **`0073` must FAIL** on `subject stat rbac_allow.rbac.allowed expected 1 got 0`, and `0072` must FAIL on `denied expected 1 got 0`. **Restore `wrap_in_chain`.** If either still passes, the fixture is vacuous — stop and invoke `superpowers:systematic-debugging`.

Then the whole pre-existing differential surface (§7.5 gate (b)):

```bash
cargo test -p differential --no-fail-fast 2>&1 | tee /tmp/diff.log
```
Do **not** pipe through `tail` — read the `failures:` block in full. Expect the documented environmental REDs (`0061`/`0062`/`0069`/`0070`, plus parallel-load flakes); re-run any suspected flake **in isolation** before calling it a regression. CI is authoritative.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p differential --all-targets -- -D warnings
git add tests/fixtures/0072-network-filter-rbac-deny tests/fixtures/0073-network-filter-rbac-allow \
        tests/differential/tests/network_filter_rbac_deny.rs tests/differential/tests/network_filter_rbac_allow.rs
git status --short tests/fixtures/0073-network-filter-rbac-allow/inputs/   # payload.bin MUST be tracked
git commit -m "phase 67.1 task 12: fixtures 0072 (DENY) + 0073 (ALLOW) (D8)"
```

---

## Task 13: In-process backstops + negative config tests (D9)

**Files:**
- Create: `crates/envoy-bin/tests/network_filter_rbac.rs`

**Interfaces:**
- Consumes: everything. Spawns the real `envoy-bin` binary via `env!("CARGO_BIN_EXE_envoy-bin")`, mirroring `crates/envoy-bin/tests/network_filter_direct_response.rs`.
- Produces: the observable contract fixtures `0072`/`0073` cannot see in-process.

**Context.** `envoy-bin` is a **binary** crate, so `tests/` cannot import its modules. The established pattern (`tests/network_filter_direct_response.rs`) spawns the built binary against a temp config and drives it over TCP. `tests/common/mod.rs` provides `reserve_port()` and `wait_ready()`.

What the fixtures cannot see, and this file must:

1. **The DENY close is a clean EOF, and a post-EOF client write is ACCEPTED, not reset** (SPEC R-2). Fixture `0072`'s driver never writes after EOF. This is the DENY-path twin of ADR-0124's `post_eof_client_write_is_accepted_not_reset`.
2. **The client's already-sent bytes are discarded** on DENY (SPEC R-2). `0072`'s `read_to_eof` probe sends nothing.
3. **`rules` omitted ⇒ INERT** (SPEC R-4, PLAN-VERIFY W-6): the connection is allowed AND **neither counter increments**. No fixture covers this — a body-only fixture is blind to it, which is exactly the trap R-4 documents.
4. **Negative config**, exercised against the real binary's startup path: `[rbac]` alone, `[echo, rbac]` (precedence), `filters: []` (accepted, no panic — ADR-0130 §2), empty `stat_prefix`, and the three rejected L4 leaves.

- [ ] **Step 1: Write the failing tests**

```rust
//! Phase 67.1 backstops: boot the real `envoy-bin` with an
//! `envoy.filters.network.rbac` chain and assert the observable contract the
//! differential fixtures 0072/0073 cannot see in-process.
//!
//! The real cross-proxy assertions are the Docker-gated
//! `tests/differential/tests/network_filter_rbac_{deny,allow}.rs`.

use std::io::Write;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

mod common;
use common::{reserve_port, wait_ready};

fn spawn_envoy_bin(yaml: &str) -> (tokio::process::Child, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg).unwrap().write_all(yaml.as_bytes()).unwrap();
    let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c").arg(&cfg)
        .env("ENVOY_RUST_LOG", "warn")
        .stdout(Stdio::null()).stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");
    (child, dir)
}

/// Run `envoy-bin -c <yaml>` to completion and return (exit-ok, stderr).
/// Used by the negative-config tests: a rejected config exits non-zero fast.
fn validate_config(yaml: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg).unwrap().write_all(yaml.as_bytes()).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c").arg(&cfg)
        .stdout(Stdio::piped()).stderr(Stdio::piped())
        .output()
        .expect("run envoy-bin");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn rbac_echo_cfg(port: u16, stat_prefix: &str, rules_block: &str) -> String {
    format!(
        r#"
static_resources:
  listeners:
    - name: rbac_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: {stat_prefix}
{rules_block}
            - name: envoy.filters.network.echo
"#
    )
}

const DENY_ALL: &str = r#"                rules:
                  action: DENY
                  policies:
                    p0:
                      permissions: [{ any: true }]
                      principals: [{ any: true }]"#;

const ALLOW_ALL: &str = r#"                rules:
                  action: ALLOW
                  policies:
                    p0:
                      permissions: [{ any: true }]
                      principals: [{ any: true }]"#;

/// SPEC R-2: DENY writes ZERO bytes and closes with a CLEAN EOF — never an RST.
/// The client's already-sent bytes are DISCARDED.
#[tokio::test]
async fn deny_writes_zero_bytes_and_closes_cleanly_discarding_client_bytes() {
    let port = reserve_port();
    let (_child, _dir) = spawn_envoy_bin(&rbac_echo_cfg(port, "d", DENY_ALL));
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    wait_ready(addr, Duration::from_secs(10)).await.expect("listener up");

    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(b"PING-RBAC\n").await.unwrap();
    let mut out = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), s.read_to_end(&mut out))
        .await
        .expect("DENY must half-close within 5s")
        .expect("clean EOF, not RST");
    assert!(out.is_empty(), "DENY writes zero bytes; the terminal echo must NOT run. got {out:?}");
}

/// SPEC R-2 / ADR-0124's drain on the DENY path: a client write issued AFTER it
/// observes EOF is ACCEPTED, not reset. A server closing without draining its
/// read half would RST the client.
///
/// DELETE THE DRAIN LOOP IN `envoy_listener::close_with_drain` AND THIS TEST MUST FAIL.
#[tokio::test]
async fn deny_post_eof_client_write_is_accepted_not_reset() {
    let port = reserve_port();
    let (_child, _dir) = spawn_envoy_bin(&rbac_echo_cfg(port, "d", DENY_ALL));
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    wait_ready(addr, Duration::from_secs(10)).await.expect("listener up");

    let mut s = TcpStream::connect(addr).await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.expect("clean EOF");
    assert!(out.is_empty());

    s.write_all(b"y").await.expect("first post-EOF write");
    tokio::time::sleep(Duration::from_millis(50)).await;
    s.write_all(b"y").await.expect("second post-EOF write must not be reset");
}

/// SPEC R-2: ALLOW yields to the TERMINAL echo and the payload round-trips.
/// This is the iteration protocol, in-process.
#[tokio::test]
async fn allow_yields_to_the_terminal_echo_filter() {
    let port = reserve_port();
    let (_child, _dir) = spawn_envoy_bin(&rbac_echo_cfg(port, "a", ALLOW_ALL));
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    wait_ready(addr, Duration::from_secs(10)).await.expect("listener up");

    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(b"PING-RBAC\n").await.unwrap();
    s.shutdown().await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.expect("echo round-trip");
    assert_eq!(out, b"PING-RBAC\n");
}

/// SPEC R-4 (PLAN-VERIFY W-6) — THE INERTNESS WITNESS, against the real binary.
///
/// `rules` omitted ⇒ the filter is INERT: the connection is ALLOWED and NEITHER
/// counter increments. A naive default `Rules { action: ALLOW }` would tick
/// `allowed` — a STAT divergence with NO body divergence, invisible to a
/// body-only fixture. Scraped from the admin listener, which is why this config
/// carries an `admin:` block.
#[tokio::test]
async fn rules_omitted_is_inert_neither_counter_ticks() {
    let port = reserve_port();
    let admin_port = reserve_port();
    let yaml = format!(
        r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
{}"#,
        rbac_echo_cfg(port, "norules", "")
    );
    let (_child, _dir) = spawn_envoy_bin(&yaml);
    let data_addr = format!("127.0.0.1:{port}").parse().unwrap();
    let admin_addr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    wait_ready(admin_addr, Duration::from_secs(10)).await.expect("admin up");
    wait_ready(data_addr, Duration::from_secs(10)).await.expect("listener up");

    let mut s = TcpStream::connect(data_addr).await.unwrap();
    s.write_all(b"HELLO\n").await.unwrap();
    s.shutdown().await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.expect("allowed through to echo");
    assert_eq!(out, b"HELLO\n", "an inert filter allows the connection");

    tokio::time::sleep(Duration::from_millis(300)).await;
    let stats = common::http_get_body(admin_addr, "/stats").await.expect("scrape /stats");
    for (name, want) in [
        ("norules.rbac.allowed", "0"),
        ("norules.rbac.denied", "0"),
        ("norules.rbac.shadow_allowed", "0"),
        ("norules.rbac.shadow_denied", "0"),
    ] {
        let line = stats
            .lines()
            .find(|l| l.starts_with(&format!("{name}: ")))
            .unwrap_or_else(|| panic!("counter {name} must be REGISTERED (stat tree parity)"));
        assert_eq!(line, format!("{name}: {want}"), "INERT: {name} must not tick");
    }
}

/// SPEC R-1: a chain whose LAST filter is non-terminal is REJECTED at startup.
#[tokio::test]
async fn rbac_alone_is_rejected_at_startup() {
    let port = reserve_port();
    let yaml = format!(
        r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: {port} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: sp
"#
    );
    let (ok, stderr) = validate_config(&yaml);
    assert!(!ok, "[rbac] alone must be rejected");
    assert!(stderr.contains("non-terminal filter"), "got {stderr}");
}

/// SPEC R-5: ERROR PRECEDENCE. `[echo, rbac]` violates BOTH rules; the
/// TERMINAL-not-last error wins.
#[tokio::test]
async fn echo_before_rbac_reports_the_terminal_error() {
    let port = reserve_port();
    let yaml = format!(
        r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: {port} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: sp
"#
    );
    let (ok, stderr) = validate_config(&yaml);
    assert!(!ok, "[echo, rbac] must be rejected");
    assert!(
        stderr.contains("must be the last filter"),
        "terminal-not-last must WIN over chain-not-terminated; got {stderr}",
    );
    assert!(!stderr.contains("non-terminal filter"), "wrong error won; got {stderr}");
}

/// SPEC R-7 / ADR-0130 §2: `filters: []` is ACCEPTED (upstream parity) and must
/// NOT panic. envoy-rust used to crash here with
/// `validator guarantees ≥1 filter` while upstream Envoy accepts and starts.
#[tokio::test]
async fn empty_filter_chain_starts_without_panicking() {
    let admin_port = reserve_port();
    let yaml = format!(
        r#"
admin:
  address:
    socket_address: {{ address: 127.0.0.1, port_value: {admin_port} }}
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: 0 }}
      filter_chains:
        - filters: []
"#
    );
    let (mut child, _dir) = spawn_envoy_bin(&yaml);
    let admin_addr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("admin listener serves despite the empty data chain");
    assert!(child.try_wait().unwrap().is_none(), "process must still be alive (no panic)");
    child.kill().await.ok();
}

/// D1 / SPEC R-3: an EMPTY `stat_prefix` is rejected at startup.
#[tokio::test]
async fn empty_stat_prefix_is_rejected() {
    let (ok, stderr) = validate_config(&rbac_echo_cfg(reserve_port(), r#""""#, ""));
    assert!(!ok, "empty stat_prefix must be rejected");
    assert!(stderr.contains("stat_prefix"), "got {stderr}");
}

/// D3 / CF-67-4: the three L4-unevaluable leaves are rejected at startup.
/// `header` in PARITY with upstream Envoy; `url_path` and `metadata` as a
/// deliberate FAIL-LOUD divergence (ADR-0049 decision-2 (b)).
#[tokio::test]
async fn l4_unevaluable_matcher_leaves_are_rejected() {
    let cases = [
        ("header", r#"[{ header: { name: ":path", exact_match: "/x" } }]"#),
        ("url_path", r#"[{ url_path: { path: { exact: "/x" } } }]"#),
        (
            "metadata",
            r#"[{ metadata: { filter: f, path: [{ key: k }], value: { string_match: { exact: v } } } }]"#,
        ),
    ];
    for (arm, perms) in cases {
        let rules = format!(
            "                rules:\n                  action: ALLOW\n                  policies:\n                    p0:\n                      permissions: {perms}\n                      principals: [{{ any: true }}]"
        );
        let (ok, stderr) = validate_config(&rbac_echo_cfg(reserve_port(), "sp", &rules));
        assert!(!ok, "{arm} must be rejected at L4");
        assert!(stderr.contains(arm), "error must name the arm {arm}; got {stderr}");
    }
}
```

> `common::http_get_body(addr, path) -> Result<String>` may not exist in `tests/common/mod.rs`. Check first. If absent, add it there (a minimal `GET <path> HTTP/1.1` + read-to-EOF + split on `\r\n\r\n`) rather than duplicating it in this file — that module exists precisely to hold shared helpers.
>
> `validate_config` runs the binary to completion. For an ACCEPTED config the binary serves forever, so **never call it on a valid config.** All six uses above pass configs that must be rejected at startup.

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p envoy-bin --test network_filter_rbac 2>&1 | tee /tmp/t13.log
```
Expected: compile errors first (missing `http_get_body`), then failures.

- [ ] **Step 3: Add any missing `tests/common/mod.rs` helper**

Only `http_get_body`, if absent. Do not add anything else.

- [ ] **Step 4: Run to verify pass**

```bash
cargo build -p envoy-bin
cargo test -p envoy-bin --test network_filter_rbac -- --nocapture 2>&1 | tail -20
```
Expected: `9 passed`.

- [ ] **Step 5: Prove the two mutation checks bite**

- Replace `close_with_drain(downstream).await?` in `ChainHandler`'s `StopIteration` branch with `downstream.shutdown().await?`. **`deny_post_eof_client_write_is_accepted_not_reset` MUST FAIL.** Restore.
- Make `NetworkRbacFilter::on_new_connection` materialise a default `Rules` when `self.rules` is `None` (i.e. tick `allowed` and `Continue`). **`rules_omitted_is_inert_neither_counter_ticks` MUST FAIL** on `norules.rbac.allowed: 1`. Restore.

If either mutation passes, the test is not exercising what it claims — stop and invoke `superpowers:systematic-debugging`.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p envoy-bin --all-targets -- -D warnings
git add crates/envoy-bin/tests/network_filter_rbac.rs crates/envoy-bin/tests/common/mod.rs
git commit -m "phase 67.1 task 13: in-process backstops + negative config tests (D9)"
```

---

## Task 14: `BEHAVIOR_CONTRACT.md` + the fuzz corpus seed (D10)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `## Network filters` section, `:229-280`)
- Modify: `crates/envoy-config/fuzz/.gitignore`
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_rbac.yaml`

**Interfaces:**
- Consumes: everything above.
- Produces: the documentary record. **No code.**

**Context — §7.4 disposition: NO new fuzz target (locked by ADR-0128 §2.3, unchanged by ADR-0129).**

Doctrine §7.4: *"Every phase that introduces a parser, codec, or filter ships a `cargo fuzz` target."* This sub-phase introduces a **filter** — but one that **parses nothing**. Network `rbac` never reads a downstream byte (SPEC R-2); it inspects `peer_addr` / `local_addr` only. Its sole untrusted-input surface is the **bootstrap config parser**, already covered by the pre-existing `parse_bootstrap` target (`crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`, wired at `.github/workflows/ci.yml:77-124`), which reaches the new `TypedConfig::NetworkRbac` variant the moment it lands.

**Add a corpus seed instead.** Two mechanical traps:

- The corpus dir is `*`-ignored (`corpus/parse_bootstrap/*`). A new seed needs an explicit `!`-un-ignore line, and must be **proven tracked with `git ls-files`** — otherwise it is silently untracked and invisible to CI.
- A **new target** would need a hand-written `ci.yml` step. **Not applicable here** — but **the §7.5 gate (d) must be RECORDED EXPLICITLY at state-4 as "satisfied by the pre-existing `parse_bootstrap` target"**, not passed over in silence.

- [ ] **Step 1: Write the corpus seed**

`crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_rbac.yaml` — exercise every new parse path in one document: the `@type` URL, `stat_prefix`, `rules` present with `action`, `policies`, and each combinator, plus a terminal `echo`.

```yaml
static_resources:
  listeners:
    - name: rbac_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: rbac_probe
                rules:
                  action: DENY
                  policies:
                    p0:
                      permissions:
                        - and_rules:
                            rules:
                              - any: true
                              - not_rule: { any: false }
                      principals:
                        - or_ids:
                            ids:
                              - any: true
                              - not_id: { any: false }
            - name: envoy.filters.network.echo
```

- [ ] **Step 2: Un-ignore it and PROVE it is tracked**

Append to `crates/envoy-config/fuzz/.gitignore`, after the `network_filter_direct_response.yaml` line:

```gitignore
!corpus/parse_bootstrap/network_filter_rbac.yaml
```

Then:

```bash
git add crates/envoy-config/fuzz/.gitignore crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_rbac.yaml
git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_rbac.yaml
```
Expected: the path is printed. **An empty result means the `!`-line did not take and CI will never see the seed.**

- [ ] **Step 3: Verify the seed parses (it must be a VALID config to be a useful seed)**

```bash
cargo run -q -p envoy-bin -- -c crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_rbac.yaml &
sleep 2 && kill %1
```
Expected: no `panicked at`, no config error. (A corpus seed need not be valid to be *useful*, but a valid one exercises the deepest path.)

- [ ] **Step 4: Extend `BEHAVIOR_CONTRACT.md`**

In the `## Network filters` section, update the scope banner and **add a do-not-conflate banner for `rbac`** mirroring `direct_response`'s:

```markdown
## Network filters

> Opened by phase 66 (the Network-filters family's first row). Scope today:
> `echo`, `tcp_proxy`, `http_connection_manager`, `direct_response`, `rbac`.
>
> **Do not conflate** `envoy.filters.network.direct_response` (a network filter, which
> writes a payload on connection accept) with the HCM **route-level** `direct_response`
> action (phase 04, which returns an HTTP response for a matched route). …
>
> **Do not conflate** `envoy.filters.network.rbac` (phase 67.1 — an L4 filter that
> permits or denies a whole CONNECTION at establishment, before any byte is read)
> with `envoy.filters.http.rbac` (phase 10, `crates/envoy-filter/src/rbac.rs` — an
> HTTP filter that permits or denies a REQUEST). They are different features with the
> same name. They share the `Rules` / `Policy` / `Permission` / `Principal` config
> trees and nothing else. Every `rbac` row elsewhere in this document refers to the
> HTTP filter unless it says "network".
```

Then a new subsection, `### envoy.filters.network.rbac (phase 67.1, ADR-0128 / ADR-0129 / ADR-0130)`, with these rows — each naming its evidence:

1. **Decision timing.** Evaluated ONCE per connection, at establishment, **before any downstream byte is read**. Deterministic and timing-free. *(SPEC R-2; witnessed against `envoyproxy/envoy:v1.33.0`.)*
2. **DENY semantics.** Zero bytes written; clean EOF, **never an RST**; the client's already-sent bytes are discarded; a post-EOF client write is **accepted**. The terminal filter never runs. Differentially witnessed by fixture **`0072-network-filter-rbac-deny`** (body byte-exact **AND** `rbac_deny.rbac.denied == 1`). The post-EOF-write clause has **no differential observable** and is pinned in-process by `deny_post_eof_client_write_is_accepted_not_reset`.
3. **ALLOW semantics.** The connection proceeds to the terminal filter and the payload round-trips. Differentially witnessed by fixture **`0073-network-filter-rbac-allow`** — the family's first differential proof that a **non-terminal filter runs and then yields**, i.e. of the chain iteration protocol.
4. **Stats.** `<stat_prefix>.rbac.{allowed,denied,shadow_allowed,shadow_denied}`. `stat_prefix` is **required and non-empty** (upstream proto constraint); `rules` is **optional**. *(SPEC R-3.)*
5. **`rules` omitted ⇒ the filter is INERT.** The connection is allowed and **NEITHER counter increments** — `allowed` stays `0`, not `1`. All four counters are still registered at `0`, so the stat tree matches. *(SPEC R-4, measured.)* A default `Rules { action: ALLOW }` that ticked `allowed` would be a **stat divergence with no body divergence**. Pinned by `rules_omitted_is_inert_neither_counter_ticks`.
6. **Bilateral chain-termination rule.** Upstream Envoy rejects a chain whose **last** filter is non-terminal (`non-terminal filter named <X> … is the last filter in a network filter chain`), the dual of phase 66's "a terminal filter must be last." envoy-rust enforces the identical rule via `ConfigError::NetworkFilterChainNotTerminated`. *(SPEC R-1.)*
7. **Error precedence.** A chain violating **both** rules (`[echo, rbac]`) reports the **terminal-not-last** error on both proxies. *(SPEC R-5, measured.)*
8. **Empty chain — measured parity (closes M66-5).** `filters: []` is **accepted** by upstream Envoy (`configuration OK`) and by envoy-rust. The phase-66 review's intuition that Envoy rejects it was **wrong** — which is exactly why that review recorded envoy-rust's behavior and declined to assert Envoy's (D-3.3). **Runtime divergence, recorded (ADR-0130 §2):** envoy-rust binds **no data listener** for an empty chain and logs a warning; upstream Envoy binds one. What upstream does with a *connection* to it was never probed — carried forward as **CF-67-5**. **No differential observable**: no fixture configures an empty chain.
9. **Recorded divergence — L4 matcher leaves (CF-67-4).** envoy-rust rejects `header` in **parity** with upstream Envoy, which rejects it at config load (`Found header(name: ":path"…`). envoy-rust **also** rejects `url_path` and `metadata`, which upstream **accepts** even though they can never match at L4 — a deliberate **fail-loud** divergence per the ADR-0049 decision-2 (b) posture. **No differential observable** — neither fixture uses them. *(SPEC R-6, measured.)*
10. **Recorded divergence — `shadow_rules` (CF-67-1).** Upstream accepts `shadow_rules` / `shadow_rules_stat_prefix`; envoy-rust rejects them loudly at config load (serde `deny_unknown_fields`) and emits `shadow_allowed` / `shadow_denied` as constant `0` so the stat tree matches. **No differential observable.**
11. **Scope — matcher arms.** Phase 67.1 ships `any` plus the `and`/`or`/`not` combinators only. The connection-level arms (`direct_remote_ip`, `remote_ip`, `source_ip`, `destination_port`, `destination_ip`) land in phase **67.2** and are **not stubbed** — they do not exist, and the parser rejects them as unknown keys. `Action::LOG` is deferred (**CF-67-2**); `on_data`-time iteration is deferred (**CF-67-3**).
12. **Scope — per-listener stats (ADR-0130).** `echo` and `direct_response` listeners now emit `listener.<name>.downstream_cx_{total,active,accept_failed}` and count in `listener_manager.total_listeners_active`, because phase 67.1 routed them through the shared `envoy_listener::Listener` accept loop. This is **toward** upstream parity (Envoy counts every listener). No fixture asserts set-equality over those names on a raw-TCP listener.

- [ ] **Step 5: Verify no fixture regressed on the docs change**

Docs-only, but CI has **no `paths-ignore`** — a docs-only push DOES build. Run the config crate and the two new fixtures once more:

```bash
cargo test -p envoy-config 2>&1 | tail -3
cargo build -p envoy-bin
cargo test -p differential --test network_filter_rbac_deny --test network_filter_rbac_allow 2>&1 | tail -10
```

- [ ] **Step 6: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md crates/envoy-config/fuzz/.gitignore \
        crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_rbac.yaml
git commit -m "phase 67.1 task 14: BEHAVIOR_CONTRACT rows + parse_bootstrap corpus seed (D10)"
```

---

## §5. Definition of done for the state-3 implementation session

This plan is complete when all 14 tasks are committed and:

- `cargo build --workspace --all-targets` is clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` is clean.
- `cargo fmt --all -- --check` is clean. *(CI's most common red mid-phase — run it.)*
- `cargo test --workspace --no-fail-fast` shows only the documented environmental REDs.
- `cargo build -p envoy-bin` has been run, and fixtures `0072` + `0073` are green locally, with `0001` and `0071` still green.
- Both mutation checks in Task 13, step 5 have been performed and **restored**.
- `git ls-files` shows the fuzz corpus seed tracked.
- **`ADR-0130` is appended to `DECISIONS.md`** (it is claimed by the state-2 PLAN-write commit, per §3 above — the state-3 session must not re-number it).

**The state-4 verification session owns the §7.5 gate**, and must **record gate (d) explicitly** as *"satisfied by the pre-existing `parse_bootstrap` fuzz target; no new target — see ADR-0128 §2.3"*, not skip it silently.

**Carry-forward ledger at the end of `67.1`:**

- **CONSUMED:** `CF-66-2` (the iteration protocol — Tasks 5/6/10 *are* it), `M66-3` (both non-reaping accept loops **deleted**; the surviving loop's reaping is now witnessed), `M66-4` (the stale doc-precision line rewritten), `CF-67-4` (the L4 leaf allow-list), `M66-6` (the dynamic/LDS-listener terminal test, folded into Task 3).
- **CLOSED by recon, no code change:** `M66-5` (config-load parity on the empty chain).
- **OPENED:** `CF-67-5` — probe upstream Envoy's *connection* behavior on an empty `filters: []` chain before asserting anything about it (ADR-0130 §2).
- **STILL LIVE, none blocks:** `CF-67-1` (`shadow_rules`), `CF-67-2` (`Action::LOG`), `CF-67-3` (`on_data`-time iteration + buffering), `M66-7`, `CF-66-1`, `M64-2`, `M64-3`, `M65-1`, `M57-1`, `M55-1`, `M53-2`, `M53-3`, `M48-2`, `M42-1`, the `DC`/retry-budget-overflow slices of `M45-2`, the phase-58 candidate carry-forward, `M40-1`, `M39-1`/`M39-2`, `M38-1`/`M38-2`, `CF-39-1`, `M37-*`, `M36-*`, `M34-*`, `M33-*`, the empty-`metadata_match` doc-comment, `M29-*`/`M30-*`, the phase-31 cosmetics, and the HTTP-filters-family `(1)`-`(4)`.
- **DEFERRED to `67.2`:** the connection-level matcher arms + `CidrRange` + the three-site V-1 shared-enum fallout (`lower_permission`, `lower_principal`, `define_rbac_tree_validator!`).
- **Numbering: `M66-1` was never allocated.** The ledger advances monotonically and does not backfill. Do not "fix" the gap.
