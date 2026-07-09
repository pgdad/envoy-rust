# Phase 66 — `envoy.filters.network.direct_response` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. TDD per `superpowers:test-driven-development` on every task (doctrine D-3.1).

**Goal:** Open the Network-filters family by implementing `envoy.filters.network.direct_response`, differentially witnessed byte-exact via new fixture `0071`, and close the network-filter terminal-validation gap.

**Architecture:** Three seams, each independently testable. (1) `crates/envoy-config` gains a `TypedConfig::DirectResponse` variant, a `DIRECT_RESPONSE_FILTER` const, a validate arm, and a **terminal-filter pre-pass** guarded by a new `ConfigError::NetworkFilterNotTerminal`. (2) `crates/envoy-bin` gains a `direct_response` module — a standalone accept loop mirroring `echo.rs` (NOT a `ConnectionHandler` impl) — plus a fourth arm in the `main.rs` name-dispatch match. (3) `tests/differential` gains `Driver::TcpDirectResponse`, the harness's first read-to-EOF raw-TCP driver, plus fixture `0071`.

**Tech Stack:** Rust (pinned toolchain), `tokio` (net + io + `JoinSet`), `serde`/`serde_yaml`, `thiserror`, `anyhow` (binary crate only), `testcontainers` (differential harness). No new crate, no new dependency.

## Global Constraints

- **Doctrine D-3.8:** every workspace crate root begins with `#![forbid(unsafe_code)]`. No `unsafe` in this phase.
- **Doctrine D-3.6:** every task ends green. No task lands with failing tests, clippy warnings, or fmt drift.
- **Doctrine D-3.3:** upstream Envoy's observed behavior IS the contract. Do not read Envoy C++ source to decide equivalence.
- **Pinned reference image:** `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`. Do not change the pin (D-3.7).
- **The filter name string is exactly** `envoy.filters.network.direct_response`.
- **The typed_config `@type` URL is exactly** `type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config`.
- **Only the `inline_string` `DataSource` arm is supported.** `inline_bytes` and `filename` are rejected loudly (serde `deny_unknown_fields`). Carry-forward CF-66-1. Locked by ADR-0123 §2.2.
- **`response` is OPTIONAL.** Omitting it means an empty payload (a zero-byte write + clean close), matching Envoy exactly.
- **NO new `cargo fuzz` target** (ADR-0123 §2.3). §7.5 gate (d) is satisfied by the pre-existing `parse_bootstrap` target. A corpus **seed** is added instead (Task 8).
- **CI IS AUTHORITATIVE.** `cargo build -p envoy-bin` MUST be re-run before any local differential run — the harness executes `target/debug/envoy-bin`, and a stale binary REDs with `unknown field` / `unsupported network filter`.
- **Never weaken a fixture; never trim `tests/conformance/h2spec/known-failures.txt`.**
- Run verification output to a file; **never pipe a verification run through `tail`**.

---

## §6.1 Split Gate — evaluated, DOES NOT FIRE

**9 tasks, ~700 net LoC** in `crates/` + `tests/`, against thresholds of ~25 tasks / ~1500 LoC. No split. `ADR-0124` is therefore NOT consumed by a split — it is consumed by the §6.2 reconciliation below.

## §6.2 Empirical Reconciliation — FIRED, recorded as ADR-0124

SPEC §3's **V-3** (the unread-data RST hazard) was settled empirically at this PLAN-write, and it **resolved against the naive implementation**. Both a kernel-level experiment and a live-Envoy probe were run.

**Kernel experiment** (three server close disciplines × client-sends-first on/off × unread volumes 0 / 1 / 200 000 bytes). In every configuration the client received the full payload and a clean EOF — so the payload is *not* lost. But the disciplines diverge on a second observable, a client write issued **after** it observes EOF:

| server discipline | client post-EOF write |
|---|---|
| `write` → `close` (naive) | `BrokenPipeError` (an RST came back) |
| `write` → `shutdown(WR)` → `close` (= tokio `write_all` + `shutdown()` + drop) | `BrokenPipeError` |
| `write` → `shutdown(WR)` → **read to EOF (drain)** → `close` | accepted |

**Live-Envoy probe** against the pinned image, same three unread volumes (0 / 21 / 200 000 bytes):

```
[envoy-no-send]      unread=     0 got=27B payload_ok=True how=clean_eof post_write=writes_ok
[envoy-small-unread] unread=    21 got=27B payload_ok=True how=clean_eof post_write=writes_ok
[envoy-big-unread]   unread=200000 got=27B payload_ok=True how=clean_eof post_write=writes_ok
```

**Envoy accepts the post-EOF write in every case — it drains its read half.** The naive path does not. **Decision: envoy-rust MUST drain the read half** (`write_all` → `shutdown()` [FIN] → read-and-discard until EOF → drop). This is pinned by a mutation-check test in Task 4.

This does **not overturn** any SPEC §0 finding — R-0.5 (`payload written immediately; clean EOF; no RST` for a reading client) stands verbatim. It **refines** the implementation requirement and **adds** a BEHAVIOR_CONTRACT clause. Recorded as **ADR-0124** (landed with this PLAN, per the ADR-0037/0041/0043/0045/0047/0049 §6.2-at-PLAN-write cadence).

## PLAN-VERIFY items — re-confirmed FRESH against the live tree (commit `665d220`)

| Item | Result |
|---|---|
| **V-1** `TypedConfig` shape | CONFIRMED. `bootstrap.rs:673-681`: `#[derive(Debug, Serialize, Deserialize, PartialEq)]` + `#[serde(tag = "@type", deny_unknown_fields)]`, two variants, each with a `#[serde(rename = "<url>")]`. |
| **V-2** `ConfigError` site | **CORRECTED.** `crates/envoy-config/src/error.rs` does **not** exist. The enum is at **`crates/envoy-config/src/lib.rs:60`** (`#[derive(Debug, thiserror::Error)]`). Both tuple variants (`UnsupportedFilter(String, &'static str)`) and named-field variants (`CdsParseError { path, message }`) are established style. |
| **V-3** RST hazard | **RESOLVED — drain required.** See §6.2 above. ADR-0124 fires. |
| **V-4** driver sends bytes? | **Sends nothing.** Fixture `0071` uses a pure connect → read-to-EOF probe. The client-sends-first path is covered in-process (Task 4/5), not differentially. |
| **V-5** read-to-EOF timeout | 5 s, enforced with `tokio::time::timeout` around `read_to_end`. Precedent: `drive_tcp`'s 100 ms trailing-byte poll (`lib.rs:1682-1687`) and `subject.shutdown(Duration::from_secs(5))`. |
| **V-6** allow-list needed? | **No.** `expectations.yaml` needs only `equivalence.response_body.kind: byte_exact`. No headers, no status, no timing, no stats. |
| **V-7** `is_terminal` home + error fields | `is_terminal_network_filter(&str) -> bool` in `bootstrap.rs`; `NetworkFilterNotTerminal { name, position, chain_len }`. |
| **V-8** fuzz corpus un-ignore | CONFIRMED REQUIRED. `crates/envoy-config/fuzz/.gitignore` line 1 is `corpus/parse_bootstrap/*`, followed by ~50 explicit `!corpus/parse_bootstrap/<seed>.yaml` lines. A new seed needs its own `!` line. |

**Additional fresh finding (affects Task 3's ordering).** The validate loop at `bootstrap.rs:2973` is `for filter in &mut chain.filters` — it borrows mutably (the `HCM_FILTER` arm calls `typed_config.as_mut()`). The terminal check needs `chain.filters.len()` and the index, so it is implemented as a **separate immutable pre-pass before** the mutating loop. That also reproduces Envoy's error precedence: in the live probe, a chain of `[direct_response, echo]` reported the **terminal** error even though the trailing `echo` was itself misconfigured.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/envoy-config/src/lib.rs` | Add `DIRECT_RESPONSE_FILTER` const; add `ConfigError::NetworkFilterNotTerminal`. |
| `crates/envoy-config/src/bootstrap.rs` | Add `DirectResponseConfig`; add `TypedConfig::DirectResponse`; add `is_terminal_network_filter`; add validate arm + terminal pre-pass. |
| `crates/envoy-bin/src/direct_response.rs` | **Create.** Accept loop + per-connection write/FIN/drain. |
| `crates/envoy-bin/src/main.rs` | `mod direct_response;` + fourth dispatch arm. |
| `crates/envoy-bin/tests/network_filter_direct_response.rs` | **Create.** In-process backstop against the real `envoy-bin`. |
| `tests/differential/src/lib.rs` | Add `Driver::TcpDirectResponse`, `drive_tcp_direct_response`, `run_tcp_direct_response_arm`, `port_key` arm, dispatch arm. |
| `tests/fixtures/0071-network-filter-direct-response/` | **Create.** `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`. |
| `tests/differential/tests/network_filter_direct_response.rs` | **Create.** One-line fixture runner. |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_direct_response.yaml` | **Create.** Corpus seed. |
| `crates/envoy-config/fuzz/.gitignore` | Add the `!` un-ignore line. |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | Four new items (SPEC §6 + the ADR-0124 drain clause). |

---

## Task 1: Config schema — `DIRECT_RESPONSE_FILTER` + `TypedConfig::DirectResponse`

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (after the `HCM_FILTER` const, ~`:53`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (`TypedConfig` enum ~`:673-681`; new struct near `DataSourceInline` ~`:790`)
- Test: `crates/envoy-config/src/bootstrap.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `envoy_config::DIRECT_RESPONSE_FILTER: &'static str`; `envoy_config::TypedConfig::DirectResponse(DirectResponseConfig)`; `envoy_config::DirectResponseConfig { response: Option<DataSourceInline> }`.
- Consumes: existing `DataSourceInline { inline_string: String }` (`bootstrap.rs:790`, already `deny_unknown_fields`).

> **VERIFIED ENTRY POINTS (do not guess).** `crate::parse_bootstrap` **parses AND validates** — it calls `bootstrap::validate(&mut bootstrap)` internally (`lib.rs:769-782`). There is no `load_bootstrap_from_str`. So a `direct_response` filter is still rejected as `ConfigError::UnsupportedFilter` by `parse_bootstrap` until **Task 2** lands the validate arm.
> Therefore **this task's tests exercise pure deserialization** via `serde_yaml::from_str::<Bootstrap>(...)` (the established convention — 7 existing call sites in this file). Tasks 2 and 3 use `crate::parse_bootstrap`, matching `rejects_unknown_filter_name` (`bootstrap.rs:~4820`).

- [ ] **Step 1: Write the failing tests**

Append to `crates/envoy-config/src/bootstrap.rs`'s `mod tests`:

```rust
    #[test]
    fn direct_response_filter_parses_inline_string() {
        // Pure schema test: `parse_bootstrap` also validates, and the validate
        // arm does not exist until Task 2.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response:
                  inline_string: "hi\n"
"#;
        let bs: Bootstrap = serde_yaml::from_str(yaml).expect("deserializes");
        let f = &bs.static_resources.listeners[0].filter_chains[0].filters[0];
        assert_eq!(f.name, crate::DIRECT_RESPONSE_FILTER);
        let Some(TypedConfig::DirectResponse(dr)) = &f.typed_config else {
            panic!("expected DirectResponse typed_config");
        };
        assert_eq!(dr.response.as_ref().unwrap().inline_string, "hi\n");
    }

    #[test]
    fn direct_response_response_field_is_optional() {
        // Upstream Envoy validates `rc=0` with `response` omitted, and writes
        // zero bytes then closes. Phase-66 SPEC §0 R-0.7.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
"#;
        let bs: Bootstrap = serde_yaml::from_str(yaml).expect("deserializes");
        let f = &bs.static_resources.listeners[0].filter_chains[0].filters[0];
        let Some(TypedConfig::DirectResponse(dr)) = &f.typed_config else {
            panic!("expected DirectResponse typed_config");
        };
        assert!(dr.response.is_none());
    }

    #[test]
    fn direct_response_rejects_inline_bytes_and_filename() {
        // CF-66-1 (ADR-0123 §2.2): envoy-rust supports only the `inline_string`
        // DataSource arm and rejects the others LOUDLY. `DataSourceInline` is
        // `deny_unknown_fields`, so serde raises "unknown field" at deserialize
        // time — before validation runs at all.
        for arm in [r#"inline_bytes: "aGVsbG8=""#, r#"filename: "/tmp/x""#] {
            let yaml = format!(
                r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: {{ address: 127.0.0.1, port_value: 10000 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response: {{ {arm} }}
"#
            );
            let err = serde_yaml::from_str::<Bootstrap>(&yaml)
                .expect_err("must reject non-inline_string DataSource arm");
            assert!(
                err.to_string().contains("unknown field"),
                "expected an unknown-field rejection, got: {err}"
            );
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p envoy-config direct_response 2>&1 | tee /tmp/t1.log
```
Expected: FAIL to COMPILE — `cannot find value DIRECT_RESPONSE_FILTER in crate` / `no variant named DirectResponse found for enum TypedConfig`.

- [ ] **Step 3: Add the const**

In `crates/envoy-config/src/lib.rs`, immediately after the `HCM_FILTER` const (~`:53`):

```rust
/// The direct-response network filter name. envoy-rust accepts it as of phase
/// 66 — the Network-filters family opener. A TERMINAL filter (see
/// `is_terminal_network_filter`). See ADR-0123.
pub const DIRECT_RESPONSE_FILTER: &str = "envoy.filters.network.direct_response";
```

- [ ] **Step 4: Add the config struct + enum variant**

In `crates/envoy-config/src/bootstrap.rs`, immediately after `DataSourceInline` (~`:792`):

```rust
/// Models `envoy.extensions.filters.network.direct_response.v3.Config`.
///
/// `response` is OPTIONAL: upstream Envoy validates a `direct_response` filter
/// with the field omitted (`rc=0`) and then writes zero bytes before closing.
/// `None` therefore means "empty payload", not "invalid config" (phase-66 SPEC
/// §0 R-0.7).
///
/// Only the `inline_string` DataSource arm is modeled. Upstream Envoy also
/// accepts `inline_bytes` and `filename`; envoy-rust rejects both LOUDLY via
/// `DataSourceInline`'s `deny_unknown_fields` — the ADR-0049 decision-2 (b)
/// fail-loud posture. Recorded divergence, carry-forward CF-66-1 (ADR-0123).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DirectResponseConfig {
    #[serde(default)]
    pub response: Option<DataSourceInline>,
}
```

Then add the variant to `TypedConfig` (after the `HttpConnectionManager` variant):

```rust
    #[serde(
        rename = "type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config"
    )]
    DirectResponse(DirectResponseConfig),
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo test -p envoy-config direct_response 2>&1 | tee /tmp/t1.log
```
Expected: 3 passed (`direct_response_filter_parses_inline_string`, `direct_response_response_field_is_optional`, `direct_response_rejects_inline_bytes_and_filename`).

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 66: envoy-config gains DirectResponseConfig + TypedConfig::DirectResponse [ADR-0123]"
```

---

## Task 2: Validate arm — `direct_response` requires its typed_config

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (the network-filter match in `validate`, ~`:2974-3030`)
- Test: `crates/envoy-config/src/bootstrap.rs` (`mod tests`)

**Interfaces:**
- Consumes: Task 1's `DIRECT_RESPONSE_FILTER`, `TypedConfig::DirectResponse`.
- Produces: a validated `direct_response` filter; reuses the existing `ConfigError::MissingTypedConfig(&'static str)`.

Note: `crate::parse_bootstrap` parses AND validates (`lib.rs:769-782`). Before this task a `direct_response` filter is rejected by the `_ =>` arm as `ConfigError::UnsupportedFilter`, so Task 1's pure-deserialization tests pass but `parse_bootstrap` still fails. That is exactly what the first test below pins.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn direct_response_filter_validates() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response:
                  inline_string: "hi\n"
"#;
        crate::parse_bootstrap(yaml).expect("direct_response must validate");
    }

    #[test]
    fn direct_response_without_typed_config_is_rejected() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must require typed_config");
        assert!(
            matches!(err, crate::ConfigError::MissingTypedConfig(crate::DIRECT_RESPONSE_FILTER)),
            "got {err:?}"
        );
    }

    #[test]
    fn direct_response_with_wrong_typed_config_variant_is_rejected() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: x
                cluster: nope
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("wrong variant must be rejected");
        assert!(
            matches!(err, crate::ConfigError::MissingTypedConfig(crate::DIRECT_RESPONSE_FILTER)),
            "got {err:?}"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p envoy-config direct_response 2>&1 | tee /tmp/t2.log
```
Expected: `direct_response_filter_validates` FAILS with `UnsupportedFilter("envoy.filters.network.direct_response", ...)`.

- [ ] **Step 3: Add the validate arm**

In `crates/envoy-config/src/bootstrap.rs`, inside `match filter.name.as_str()`, immediately after the `crate::HCM_FILTER` arm and before the `_ =>` arm:

```rust
                    crate::DIRECT_RESPONSE_FILTER => {
                        // 66 (ADR-0123): the payload lives in typed_config; the
                        // filter is meaningless without it. `response` inside it
                        // is optional (empty payload) per SPEC §0 R-0.7.
                        let typed = filter.typed_config.as_ref().ok_or(
                            crate::ConfigError::MissingTypedConfig(crate::DIRECT_RESPONSE_FILTER),
                        )?;
                        let TypedConfig::DirectResponse(_) = typed else {
                            return Err(crate::ConfigError::MissingTypedConfig(
                                crate::DIRECT_RESPONSE_FILTER,
                            ));
                        };
                    }
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test -p envoy-config direct_response 2>&1 | tee /tmp/t2.log
```
Expected: 6 passed (3 from Task 1 + 3 new).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 66: validate() accepts envoy.filters.network.direct_response [ADR-0123]"
```

---

## Task 3: Network-filter terminal validation

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (`ConfigError`, ~`:60`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (new fn + pre-pass before the filter match, ~`:2973`)
- Test: `crates/envoy-config/src/bootstrap.rs` (`mod tests`)

**Interfaces:**
- Produces: `ConfigError::NetworkFilterNotTerminal { name: String, position: usize, chain_len: usize }`; `is_terminal_network_filter(name: &str) -> bool` (crate-internal).

**Why a predicate and not `chain.filters.len() <= 1`:** all four supported filters are terminal *today*, so the two rules are extensionally equal today — but the first non-terminal network filter (`sni_cluster`, network `rbac`) makes them diverge. Locked by ADR-0123.

**Empirical basis (SPEC §0 R-0.6):** `--mode validate` against the pinned image rejects a chain that places `direct_response`, `echo`, `tcp_proxy`, or `http_connection_manager` before another filter, with `terminal filter named <X> of type <X> must be the last filter in a network filter chain.` **Safety (SPEC §0 R-0.8):** zero existing fixtures/tests use a multi-filter chain, so nothing regresses.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn terminal_network_filter_must_be_last() {
        // SPEC §0 R-0.6: upstream Envoy rejects this exact shape.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response:
                  inline_string: "x"
            - name: envoy.filters.network.echo
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("terminal filter not last");
        match err {
            crate::ConfigError::NetworkFilterNotTerminal { name, position, chain_len } => {
                assert_eq!(name, crate::DIRECT_RESPONSE_FILTER);
                assert_eq!(position, 1);
                assert_eq!(chain_len, 2);
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn echo_is_also_terminal() {
        // All four supported network filters are terminal upstream (R-0.6).
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response:
                  inline_string: "x"
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("echo must be terminal too");
        assert!(
            matches!(err, crate::ConfigError::NetworkFilterNotTerminal { ref name, .. } if name == crate::ECHO_FILTER),
            "got {err:?}"
        );
    }

    #[test]
    fn single_terminal_filter_chain_is_accepted() {
        // Regression guard for R-0.8: every existing config is single-filter.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
"#;
        crate::parse_bootstrap(yaml).expect("single-filter chain must still validate");
    }

    #[test]
    fn is_terminal_network_filter_covers_all_four_supported_names() {
        for n in [
            crate::ECHO_FILTER,
            crate::TCP_PROXY_FILTER,
            crate::HCM_FILTER,
            crate::DIRECT_RESPONSE_FILTER,
        ] {
            assert!(is_terminal_network_filter(n), "{n} must be terminal");
        }
        assert!(!is_terminal_network_filter("envoy.filters.network.sni_cluster"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p envoy-config terminal 2>&1 | tee /tmp/t3.log
```
Expected: FAIL — `no variant named NetworkFilterNotTerminal`, `cannot find function is_terminal_network_filter`.

- [ ] **Step 3: Add the `ConfigError` variant**

In `crates/envoy-config/src/lib.rs`, inside `pub enum ConfigError`:

```rust
    /// 66 (ADR-0123): a TERMINAL network filter appeared before the end of its
    /// filter chain. Mirrors upstream Envoy, which rejects the same shape with
    /// "terminal filter named <X> of type <X> must be the last filter in a
    /// network filter chain." `position` is 1-based.
    #[error(
        "terminal network filter '{name}' at position {position} of {chain_len} must be the last filter in its network filter chain"
    )]
    NetworkFilterNotTerminal {
        name: String,
        position: usize,
        chain_len: usize,
    },
```

- [ ] **Step 4: Add the predicate**

In `crates/envoy-config/src/bootstrap.rs` (module scope, near the other helpers):

```rust
/// Is `name` a TERMINAL network filter — one that must be the LAST filter in
/// its chain?
///
/// Every network filter envoy-rust supports today is terminal, empirically
/// confirmed against `envoyproxy/envoy:v1.33.0` (phase-66 SPEC §0 R-0.6). This
/// is written as a per-name predicate rather than a `chain.filters.len() <= 1`
/// check so that the first NON-terminal network filter (`sni_cluster`, network
/// `rbac`) drops in without re-litigating the rule. See ADR-0123.
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

- [ ] **Step 5: Add the pre-pass**

In `validate`, immediately **before** the existing `for filter in &mut chain.filters {` loop (~`:2973`):

```rust
            // 66 (ADR-0123): terminal-filter pre-pass. Runs BEFORE the mutating
            // per-filter loop for two reasons: (a) that loop borrows
            // `chain.filters` mutably (the HCM arm calls `as_mut()`), and we
            // need the length + index here; (b) it reproduces upstream Envoy's
            // error precedence — a chain of [direct_response, echo] reports the
            // TERMINAL error even when the trailing filter is itself malformed.
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
```

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cargo test -p envoy-config 2>&1 | tee /tmp/t3.log
grep -E "^test result:" /tmp/t3.log
```
Expected: all `envoy-config` tests pass — the 4 new ones plus every pre-existing one (R-0.8 predicts zero regressions).

- [ ] **Step 7: Commit**

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 66: network-filter terminal validation (all four supported filters) [ADR-0123]"
```

---

## Task 4: `direct_response` data plane — write, FIN, **drain**

**Files:**
- Create: `crates/envoy-bin/src/direct_response.rs`
- Test: `crates/envoy-bin/src/direct_response.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `pub async fn serve(listener: TcpListener, payload: Arc<[u8]>, shutdown: impl Future<Output = ()>) -> anyhow::Result<()>`.
- Consumes: nothing from earlier tasks (Task 5 wires it to config).

**The drain is load-bearing — see §6.2 / ADR-0124.** The per-connection sequence is `write_all(payload)` → `flush()` → `shutdown()` (sends FIN) → **read-and-discard until EOF** → drop. Omitting the drain makes envoy-rust RST the client, which upstream Envoy does not. Step 1's `post_eof_write` test is the mutation check that pins it.

- [ ] **Step 1: Write the failing tests**

Create `crates/envoy-bin/src/direct_response.rs` containing ONLY the test module and the function signatures (empty bodies with `todo!()`), then:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::oneshot;

    async fn spawn(payload: &'static [u8]) -> (std::net::SocketAddr, oneshot::Sender<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind :0");
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = serve(listener, Arc::from(payload), async move { let _ = rx.await; }).await;
        });
        (addr, tx)
    }

    #[tokio::test]
    async fn writes_payload_then_clean_eof() {
        let (addr, _tx) = spawn(b"hello-from-direct-response\n").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF, not RST");
        assert_eq!(out, b"hello-from-direct-response\n");
    }

    #[tokio::test]
    async fn empty_payload_writes_zero_bytes_then_closes() {
        // SPEC §0 R-0.7: Envoy with `response` omitted writes 0 bytes + closes.
        let (addr, _tx) = spawn(b"").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF");
        assert!(out.is_empty(), "expected zero bytes, got {out:?}");
    }

    #[tokio::test]
    async fn client_that_writes_first_still_receives_payload() {
        // SPEC §0 R-0.5: Envoy ignores client input and still delivers.
        let (addr, _tx) = spawn(b"PAYLOAD\n").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(b"PING-NEVER-READ\n").await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF");
        assert_eq!(out, b"PAYLOAD\n");
    }

    /// MUTATION CHECK for the drain (ADR-0124 / SPEC V-3).
    ///
    /// Upstream Envoy accepts a client write issued AFTER the client observes
    /// EOF (measured: `post_write=writes_ok` at 0 / 21 / 200_000 unread bytes).
    /// A server that closes without draining its read half sends an RST, and
    /// this write fails with BrokenPipe/ConnectionReset.
    ///
    /// DELETE THE DRAIN LOOP IN `direct_response_once` AND THIS TEST MUST FAIL.
    #[tokio::test]
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

    #[tokio::test]
    async fn shutdown_signal_stops_the_accept_loop() {
        let (addr, tx) = spawn(b"x").await;
        let _ = TcpStream::connect(addr).await.unwrap();
        tx.send(()).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(TcpStream::connect(addr).await.is_err(), "listener must be closed");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Temporarily add `mod direct_response;` to `crates/envoy-bin/src/main.rs`, then:

```bash
cargo test -p envoy-bin --bin envoy-bin direct_response 2>&1 | tee /tmp/t4.log
```
Expected: FAIL — `todo!()` panics / `not yet implemented`.

- [ ] **Step 3: Write the implementation**

Replace the file body (keep the test module) with:

```rust
//! `envoy.filters.network.direct_response` — the Network-filters family opener
//! (phase 66, ADR-0123).
//!
//! On each accepted downstream connection the filter writes its configured
//! payload IMMEDIATELY — without reading or waiting for any client bytes — then
//! half-closes (FIN) and drains the read half until the client closes.
//! Empirically matched against `envoyproxy/envoy:v1.33.0` (SPEC §0 R-0.5/R-0.7).
//!
//! Shaped after `echo.rs`: a standalone accept loop, NOT a
//! `envoy_listener::ConnectionHandler` impl (that trait serves the tcp_proxy
//! and HCM arms).

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio::time::timeout;

/// Graceful drain budget on shutdown, mirroring `echo::DRAIN_TIMEOUT`.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Accept loop. Each accepted connection gets the configured payload, then a
/// FIN, then a read-half drain.
///
/// Returns `Ok(())` after a clean drain on shutdown. Individual connection
/// errors are logged via `tracing::warn!` and never propagate.
pub async fn serve(
    listener: TcpListener,
    payload: Arc<[u8]>,
    shutdown: impl Future<Output = ()>,
) -> Result<()> {
    let mut set: JoinSet<()> = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => {
                tracing::info!("shutdown signal received; closing listener");
                drop(listener);
                break;
            }
            accept = listener.accept() => {
                match accept {
                    Ok((stream, peer)) => {
                        tracing::debug!(%peer, "accepted connection");
                        let payload = Arc::clone(&payload);
                        set.spawn(async move {
                            if let Err(err) = direct_response_once(stream, &payload).await {
                                tracing::warn!(%peer, error = %err, "direct_response connection failed");
                            }
                        });
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "accept failed; continuing");
                    }
                }
            }
        }
    }

    let in_flight = set.len();
    tracing::info!(in_flight, "draining in-flight connections");
    let drained = timeout(DRAIN_TIMEOUT, async {
        while set.join_next().await.is_some() {}
    })
    .await;
    if drained.is_err() {
        tracing::warn!("drain timeout; aborting remaining tasks");
        set.shutdown().await;
    }
    Ok(())
}

async fn direct_response_once(mut stream: tokio::net::TcpStream, payload: &[u8]) -> Result<()> {
    let (mut reader, mut writer) = stream.split();

    // Write the payload immediately; never read first. An empty payload is a
    // legal config (SPEC §0 R-0.7) and yields a zero-byte write.
    writer.write_all(payload).await?;
    writer.flush().await?;

    // Half-close: the client observes a clean EOF here.
    writer.shutdown().await?;

    // ADR-0124 (SPEC V-3): drain the read half until the client closes.
    //
    // Closing the socket while unread bytes sit in the receive queue makes the
    // kernel send an RST, so a client that writes after our FIN would see
    // BrokenPipe/ConnectionReset. Upstream Envoy accepts such a write (measured
    // at 0 / 21 / 200_000 unread bytes), so envoy-rust drains to match. Bounded
    // by the caller's shutdown drain (DRAIN_TIMEOUT), exactly as `echo.rs` is.
    let mut sink = [0u8; 8192];
    loop {
        match reader.read(&mut sink).await {
            Ok(0) => break,      // client closed — done
            Ok(_) => continue,   // discard and keep draining
            Err(_) => break,     // peer reset/error — nothing left to do
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test -p envoy-bin --bin envoy-bin direct_response 2>&1 | tee /tmp/t4.log
grep -E "^test result:" /tmp/t4.log
```
Expected: 5 passed, 0 failed.

- [ ] **Step 5: Prove the mutation check bites**

Comment out the drain `loop { ... }` in `direct_response_once`, re-run:
```bash
cargo test -p envoy-bin --bin envoy-bin post_eof_client_write_is_accepted_not_reset 2>&1 | tee /tmp/t4-mutation.log
```
Expected: **FAIL** with `second post-EOF write must not be reset` (BrokenPipe / ConnectionReset). **Restore the drain loop** and re-run to green. If it does NOT fail, stop and invoke `superpowers:systematic-debugging` — the drain is then untested and the ADR-0124 claim is unpinned.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-bin/src/direct_response.rs crates/envoy-bin/src/main.rs
git commit -m "phase 66: direct_response data plane — write, FIN, drain read half [ADR-0124]"
```

---

## Task 5: Wire the dispatch arm + in-process backstop

**Files:**
- Modify: `crates/envoy-bin/src/main.rs` (`mod` list ~`:8-10`; dispatch match ~`:240`)
- Create: `crates/envoy-bin/tests/network_filter_direct_response.rs`

**Interfaces:**
- Consumes: `direct_response::serve` (Task 4); `envoy_config::DIRECT_RESPONSE_FILTER`, `envoy_config::TypedConfig::DirectResponse` (Tasks 1-2).

- [ ] **Step 1: Write the failing integration test**

Create `crates/envoy-bin/tests/network_filter_direct_response.rs`:

```rust
//! Phase 66 backstop: boot the real `envoy-bin` with a
//! `envoy.filters.network.direct_response` listener and assert the observable
//! contract the differential fixture 0071 cannot see in-process.
//!
//! The real cross-proxy assertion is the Docker-gated
//! `tests/differential/tests/network_filter_direct_response.rs`.

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

mod common;
use common::reserve_port;

fn spawn_envoy_bin(yaml: &str) -> (tokio::process::Child, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(yaml.as_bytes())
        .unwrap();
    let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&cfg)
        .env("ENVOY_RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");
    (child, dir)
}

fn cfg_for(port: u16, response_block: &str) -> String {
    format!(
        r#"
static_resources:
  listeners:
    - name: dr_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
{response_block}
"#
    )
}

/// Connect-with-retry: `direct_response` closes every connection, so the
/// shared `wait_ready` helper's probe is itself a full exchange. Retry until
/// the listener is up.
async fn connect_ready(addr: SocketAddr) -> TcpStream {
    for _ in 0..100 {
        if let Ok(s) = TcpStream::connect(addr).await {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("listener {addr} never became ready");
}

#[tokio::test(flavor = "multi_thread")]
async fn direct_response_writes_payload_then_clean_eof() {
    let port = reserve_port();
    let yaml = cfg_for(port, "                response:\n                  inline_string: \"hello-0071\\n\"");
    let (_child, _dir) = spawn_envoy_bin(&yaml);
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let mut s = connect_ready(addr).await;
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.expect("clean EOF, not RST");
    assert_eq!(out, b"hello-0071\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn direct_response_ignores_client_input() {
    // SPEC §0 R-0.5: a client that writes first still receives the payload.
    let port = reserve_port();
    let yaml = cfg_for(port, "                response:\n                  inline_string: \"hello-0071\\n\"");
    let (_child, _dir) = spawn_envoy_bin(&yaml);
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let mut s = connect_ready(addr).await;
    s.write_all(b"PING-NEVER-READ\n").await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.expect("clean EOF");
    assert_eq!(out, b"hello-0071\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn direct_response_with_omitted_response_writes_zero_bytes() {
    // SPEC §0 R-0.7: `response` omitted -> zero-byte write + clean close.
    let port = reserve_port();
    let yaml = cfg_for(port, "");
    let (_child, _dir) = spawn_envoy_bin(&yaml);
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let mut s = connect_ready(addr).await;
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.expect("clean EOF");
    assert!(out.is_empty(), "expected zero bytes, got {out:?}");
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p envoy-bin --test network_filter_direct_response 2>&1 | tee /tmp/t5.log
```
Expected: FAIL — `envoy-bin` exits at startup, `connect_ready` panics with `listener never became ready` (the dispatch arm does not exist, so `main` hits the `unreachable`/bail path).

- [ ] **Step 3: Declare the module**

In `crates/envoy-bin/src/main.rs`, alongside `mod echo;` (~`:9`), keeping alphabetical order:

```rust
mod direct_response;
```

- [ ] **Step 4: Add the dispatch arm**

In `main.rs`, inside `match filter.name.as_str()`, after the `ECHO_FILTER` arm:

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
                // `response` omitted => empty payload (SPEC §0 R-0.7).
                let payload: std::sync::Arc<[u8]> = dr_cfg
                    .response
                    .as_ref()
                    .map(|d| d.inline_string.as_bytes())
                    .unwrap_or(&[])
                    .into();
                let lst = TcpListener::bind(bind_addr)
                    .await
                    .with_context(|| format!("binding direct_response listener to {bind_addr}"))?;
                tracing::info!(addr = %bind_addr, payload_len = payload.len(), "envoy-rust listening (direct_response)");
                let shutdown = token.clone();
                set.spawn(async move {
                    direct_response::serve(lst, payload, async move { shutdown.cancelled().await })
                        .await
                });
            }
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo build -p envoy-bin 2>&1 | tail -3
cargo test -p envoy-bin --test network_filter_direct_response 2>&1 | tee /tmp/t5.log
grep -E "^test result:" /tmp/t5.log
```
Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-bin/src/main.rs crates/envoy-bin/tests/network_filter_direct_response.rs
git commit -m "phase 66: dispatch envoy.filters.network.direct_response in envoy-bin [ADR-0123]"
```

---

## Task 6: Differential driver — `Driver::TcpDirectResponse`

**Files:**
- Modify: `tests/differential/src/lib.rs` (`Driver` enum ~`:39`; `port_key` match ~`:2834`; dispatch match ~`:3873`; new `drive_tcp_direct_response` near `drive_tcp` ~`:1692`; new `run_tcp_direct_response_arm` near `run_tcp_echo_arm` ~`:4212`)

**Interfaces:**
- Produces: `Driver::TcpDirectResponse` (YAML tag `kind: tcp_direct_response`); `pub async fn drive_tcp_direct_response(addr: SocketAddr) -> Result<Vec<u8>>`.
- Consumes: existing `assert_equivalence`, `FixtureCtx`, `subject::Subject::shutdown`.

**Why a new driver (SPEC §0 R-0.9):** `drive_tcp` always writes a payload and then reads **exactly `payload.len()`** bytes (`read_exact`), deliberately not to EOF — see ADR-0006/ADR-0007. `direct_response` sends bytes of its own choosing and ignores client input, so that shape cannot express it.

- [ ] **Step 1: Write the failing test**

Add to `tests/differential/src/lib.rs`'s `mod tests`:

```rust
    #[test]
    fn parses_tcp_direct_response_driver() {
        let y = "driver:\n  kind: tcp_direct_response\nequivalence:\n  response_body:\n    kind: byte_exact\n";
        let e: Expectations = serde_yaml::from_str(y).expect("parses");
        assert!(matches!(e.driver, Driver::TcpDirectResponse));
    }
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p differential --lib parses_tcp_direct_response_driver 2>&1 | tee /tmp/t6.log
```
Expected: FAIL — `no variant named TcpDirectResponse`.

- [ ] **Step 3: Add the enum variant**

In `Driver` (`lib.rs:39`), directly after `TcpEcho`:

```rust
    /// 66 NEW (ADR-0123): raw-TCP connect -> send NOTHING -> read to EOF.
    ///
    /// The harness's first read-to-EOF raw-TCP driver. `TcpEcho`/`drive_tcp`
    /// cannot express `direct_response`: it writes a payload and reads exactly
    /// `payload.len()` bytes back (ADR-0006/ADR-0007), whereas
    /// `direct_response` ignores client input and writes a payload of its own
    /// length before closing.
    TcpDirectResponse,
```

- [ ] **Step 4: Add the driver fn**

Immediately after `drive_tcp` (~`:1692`):

```rust
/// Connect to `addr`, send NOTHING, and read until the peer closes.
///
/// `envoy.filters.network.direct_response` writes its configured payload the
/// moment a connection is accepted and then half-closes, so the whole response
/// is "everything until EOF". A missing EOF within the deadline is a contract
/// violation (the peer must close, not linger). Phase 66, SPEC §0 R-0.5.
pub async fn drive_tcp_direct_response(addr: SocketAddr) -> Result<Vec<u8>> {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let mut out = Vec::new();
    match tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut out)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => bail!("{addr} read error before EOF (reset?): {e}"),
        Err(_) => bail!("{addr} did not close within 5s; direct_response must half-close"),
    }
    drop(stream);
    Ok(out)
}
```

- [ ] **Step 5: Add the `port_key` arm**

In the `port_key` match (~`:2834`), add `TcpDirectResponse` to the `{{PORT}}` group:

```rust
        Driver::TcpEcho
        | Driver::TcpDirectResponse
        | Driver::TlsTcp { .. }
```

- [ ] **Step 6: Add the run arm + dispatch**

Next to `run_tcp_echo_arm` (~`:4212`):

```rust
/// `Driver::TcpDirectResponse` arm of `run_fixture`. No `inputs/` payload: the
/// probe sends nothing and reads to EOF on both proxies, then asserts the two
/// response bodies are byte-equal.
async fn run_tcp_direct_response_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
) -> Result<()> {
    let FixtureCtx {
        expectations,
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    let upstream_out = drive_tcp_direct_response(upstream_addr)
        .await
        .context("upstream envoy drive")?;
    let subject_out = drive_tcp_direct_response(subject_addr)
        .await
        .context("envoy-rust drive")?;
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    assert_equivalence(expectations, None, None, &upstream_out, &subject_out)?;
    Ok(())
}
```

And in the `match &expectations.driver` dispatch (~`:3873`), after the `TcpEcho` arm:

```rust
        Driver::TcpDirectResponse => {
            run_tcp_direct_response_arm(&ctx, upstream, subject).await?;
        }
```

- [ ] **Step 7: Run it to verify it passes**

```bash
cargo test -p differential --lib parses_tcp_direct_response_driver 2>&1 | tee /tmp/t6.log
cargo clippy -p differential --all-targets -- -D warnings 2>&1 | tail -3
```
Expected: 1 passed; clippy clean.

- [ ] **Step 8: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 66: Driver::TcpDirectResponse — first read-to-EOF raw-TCP driver [ADR-0123]"
```

---

## Task 7: Fixture `0071` + differential test

**Files:**
- Create: `tests/fixtures/0071-network-filter-direct-response/envoy.yaml`
- Create: `tests/fixtures/0071-network-filter-direct-response/envoy-rust.yaml`
- Create: `tests/fixtures/0071-network-filter-direct-response/expectations.yaml`
- Create: `tests/fixtures/0071-network-filter-direct-response/README.md`
- Create: `tests/differential/tests/network_filter_direct_response.rs`

**Interfaces:**
- Consumes: `Driver::TcpDirectResponse` (Task 6); the dispatch arm (Task 5).

**No `inputs/` directory** — the driver sends nothing. **No ADR-0014 YAML shim** — unlike `echo` (whose Envoy side needs a `typed_config` that envoy-rust forbids, SPEC §0 R-0.4), `direct_response` takes the identical `typed_config` on both sides. The only difference between the two files is the bind address, matching fixture `0001`'s convention.

- [ ] **Step 1: Write the fixture files**

`envoy.yaml`:
```yaml
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response:
                  inline_string: "envoy-rust direct_response fixture 0071\n"
```

`envoy-rust.yaml` (identical but `127.0.0.1`, per fixture `0001`'s convention):
```yaml
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response:
                  inline_string: "envoy-rust direct_response fixture 0071\n"
```

`expectations.yaml`:
```yaml
driver:
  kind: tcp_direct_response
equivalence:
  response_body:
    kind: byte_exact
```

`README.md`:
```markdown
# 0071 — `envoy.filters.network.direct_response`

The Network-filters family's first differential fixture (phase 66, ADR-0123).

**What it asserts.** Both proxies serve a listener whose sole network filter is
`envoy.filters.network.direct_response` with an `inline_string` payload. The
`tcp_direct_response` driver connects, **sends nothing**, and reads to EOF. The
two response bodies must be **byte-exact** equal.

**Why this is deterministic** (SPEC §0 R-0.5, witnessed against
`envoyproxy/envoy:v1.33.0`): the payload is written the moment the connection is
accepted, is byte-identical across connections, is unaffected by client input or
by client read timing, and the close is a clean EOF with no RST. No allow-list,
no timing tolerance, no stats assertion is required.

**Both sides carry the identical `typed_config`.** Unlike fixture `0001`
(`echo`), where upstream Envoy REQUIRES a `typed_config` that envoy-rust forbids
(the ADR-0014 YAML shim), `direct_response` needs no shim. The only difference
between `envoy.yaml` and `envoy-rust.yaml` is the bind address.
```

- [ ] **Step 2: Write the differential test**

`tests/differential/tests/network_filter_direct_response.rs`:
```rust
use std::path::Path;

#[tokio::test]
async fn network_filter_direct_response_fixture() -> anyhow::Result<()> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/0071-network-filter-direct-response");
    differential::run_fixture(&fixture).await
}
```

- [ ] **Step 3: Rebuild the debug binary, then run the fixture**

The differential harness executes `target/debug/envoy-bin`. This phase adds a new config key AND a new filter name, so a stale binary REDs with `unsupported network filter` / `unknown field`.

```bash
cargo build -p envoy-bin 2>&1 | tail -3
cargo test -p differential --test network_filter_direct_response 2>&1 | tee /tmp/t7.log
grep -E "^test result:|panicked" /tmp/t7.log
```
Expected: `test result: ok. 1 passed`. (Requires Docker + the pinned image.)

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/0071-network-filter-direct-response/ tests/differential/tests/network_filter_direct_response.rs
git commit -m "phase 66: fixture 0071 — direct_response byte-exact cross-proxy witness [ADR-0123]"
```

---

## Task 8: Fuzz corpus seed (§7.4 gate (d))

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_direct_response.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore`

**No new fuzz target** (ADR-0123 §2.3): `direct_response` never reads a byte from the downstream socket, so its only untrusted-input surface is the bootstrap parser, already covered by `parse_bootstrap`. **Because no new target is added, `.github/workflows/ci.yml` needs no new step.**

- [ ] **Step 1: Add the seed**

`crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_direct_response.yaml`:
```yaml
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 127.0.0.1
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response:
                  inline_string: "seed\n"
```

- [ ] **Step 2: Un-ignore it**

`crates/envoy-config/fuzz/.gitignore` line 1 is `corpus/parse_bootstrap/*`. Append, next to the other `!` lines:
```
!corpus/parse_bootstrap/network_filter_direct_response.yaml
```

- [ ] **Step 3: PROVE the seed is tracked**

A corpus seed that git ignores is invisible to CI and the gate is silently unmet.

```bash
git add crates/envoy-config/fuzz/.gitignore crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_direct_response.yaml
git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_direct_response.yaml
```
Expected: the path is printed. **If the output is empty, the `!` line is wrong — fix it before committing.**

- [ ] **Step 4: Run the fuzz target briefly**

```bash
cd crates/envoy-config/fuzz && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30 2>&1 | tail -5; cd -
```
Expected: `Done ... runs`, no crash. (Mirrors the CI step at `.github/workflows/ci.yml:106`.)

- [ ] **Step 5: Commit**

```bash
git commit -m "phase 66: fuzz corpus seed for direct_response typed_config [ADR-0123]"
```

---

## Task 9: `BEHAVIOR_CONTRACT.md` — four new items

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md`

The contract is the project's definition of "behaviorally equivalent" (D-3.3). Divergences must be **recorded, never silent**.

- [ ] **Step 1: Add the four items**

Add to the appropriate section (network-filter semantics; create a `## Network filters` section if none exists):

```markdown
### `envoy.filters.network.direct_response` (phase 66, ADR-0123 / ADR-0124)

1. **Response semantics.** On each accepted downstream connection the filter writes the configured
   `response` payload immediately — without reading or waiting for any client bytes — then closes
   the connection with a clean EOF (no RST). A missing or empty `response` yields a zero-byte write
   followed by a clean close. Output is byte-identical across connections and independent of client
   input and of client read timing. *(Witnessed against `envoyproxy/envoy:v1.33.0`; SPEC §0 R-0.5, R-0.7.)*

2. **Read-half drain (ADR-0124).** After sending FIN, both proxies continue to drain (read and
   discard) the downstream read half until the client closes. A client write issued AFTER it
   observes EOF is therefore **accepted, not reset** — measured on upstream Envoy at 0, 21, and
   200 000 unread bytes (`post_write=writes_ok`). envoy-rust matches. A server that closed without
   draining would RST the client, which upstream Envoy does not do.

3. **Network-filter terminal rule (bilateral).** All four network filters envoy-rust supports —
   `echo`, `tcp_proxy`, `http_connection_manager`, `direct_response` — are TERMINAL: each must be
   the last filter in its chain, and upstream Envoy rejects a config that places any of them before
   another network filter (`terminal filter named <X> ... must be the last filter in a network
   filter chain`). envoy-rust enforces the identical rule via
   `ConfigError::NetworkFilterNotTerminal`, where previously it silently ignored every filter after
   the first. *(SPEC §0 R-0.6.)*

4. **Recorded divergence — `DataSource` arms (CF-66-1).** Upstream Envoy accepts
   `response.inline_bytes` and `response.filename`; envoy-rust accepts only `response.inline_string`
   and rejects the other arms loudly at config load (serde `deny_unknown_fields`). Deliberate, per
   the ADR-0049 decision-2 (b) fail-loud posture. No differential observable — fixture `0071` uses
   `inline_string`.

5. **Scope note — `echo` `typed_config` asymmetry (pre-existing, unchanged).** Upstream Envoy
   REQUIRES `typed_config` on `envoy.filters.network.echo`; envoy-rust forbids it
   (`UnexpectedTypedConfig`). Fixture `0001`'s two sides differ accordingly (ADR-0014 YAML shim).
   `direct_response` introduces no such asymmetry — both sides of fixture `0071` are identical.
```

- [ ] **Step 2: Verify no contradiction was introduced**

```bash
grep -n "direct_response" docs/envoy-rust/BEHAVIOR_CONTRACT.md | head -20
```
Expected: only the new rows; no pre-existing row claims a conflicting `direct_response` semantic. (Note: the HCM route-level `direct_response` action is a DIFFERENT feature from this network filter — do not conflate them.)

- [ ] **Step 3: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 66: BEHAVIOR_CONTRACT — direct_response semantics, drain, terminal rule [ADR-0123, ADR-0124]"
```

---

## §7.5 Phase-done gate — what the state-4 verification must run

Do **not** run these at state-3; the state-4 session owns the gate.

```bash
cargo build --workspace --all-targets                                   > /tmp/gate-build.log 2>&1
cargo clippy --workspace --all-targets --all-features -- -D warnings    > /tmp/gate-clippy.log 2>&1
cargo fmt --all -- --check                                              > /tmp/gate-fmt.log 2>&1
cargo test --workspace                                                  > /tmp/gate-test.log 2>&1
cargo deny check                                                        > /tmp/gate-deny.log 2>&1
```

- **(a)** fixture `0071` green. **(b)** all pre-existing fixtures still green. **(c)** `h2spec` pass-rate gate unchanged. **(d)** satisfied by the pre-existing `parse_bootstrap` target (ADR-0123 §2.3) — **record this explicitly, do not skip it silently**. **(e)** the five commands above clean. **(f)** `REVIEW.md` approved.

**Never pipe a gate run through `tail`** — it truncates the `failures:` block and destroys the failing test names. Redirect to a file.

**Known LOCAL-RED expectations (environmental; CI is authoritative):** an invariant core of `0061`/`0062`/`0069`/`0070` (close-backend) + `admin_config_dump_server_info`, plus a varying tail under parallel load. Adjudicate by running the workspace suite 2-3× and diffing the failing SET. CI carries documented startup-race flakes → `gh run rerun <id> --failed`. Escalate to `superpowers:systematic-debugging` only if a rerun re-fails the SAME test deterministically.

**Confirming CI:** `gh run list --commit <short-sha>` silently returns `[]`. Use the full 40-char SHA, or `gh run list --limit 5 --json databaseId,headSha,status,conclusion` and match `headSha`.

---

## Self-Review

**Spec coverage.** SPEC §2.1 items (A) 1-5 → Tasks 1-3. (B) 6-7 → Tasks 4-5. (C) 8-10 → Tasks 6-7. (D) 11-12 → Task 5 (in-process backstop) + Tasks 1-3 (negative config tests, placed in `envoy-config` where the error surface lives). (E) 13-14 → Tasks 9 and 8. SPEC §3 V-1…V-8 → all resolved in the PLAN-VERIFY table. **No gaps.**

**Placeholder scan.** No "TBD"/"TODO"/"handle edge cases"/"similar to Task N". Every code step carries real code. One drafting error was caught and fixed during this self-review: an earlier draft assumed `parse_bootstrap` parsed *without* validating and invented a `load_bootstrap_from_str` entry point. Verified against `lib.rs:769-782` — `parse_bootstrap` **parses AND validates**, and no such helper exists. Task 1's tests were rewritten to use `serde_yaml::from_str::<Bootstrap>` (pure schema; they pass before the Task-2 validate arm exists), and Tasks 2-3 use `crate::parse_bootstrap`, matching the existing `rejects_unknown_filter_name` convention (`bootstrap.rs:~4820`).

**Type consistency.** `DIRECT_RESPONSE_FILTER` (const), `DirectResponseConfig { response: Option<DataSourceInline> }`, `TypedConfig::DirectResponse`, `NetworkFilterNotTerminal { name, position, chain_len }`, `is_terminal_network_filter(&str) -> bool`, `direct_response::serve(TcpListener, Arc<[u8]>, impl Future)`, `direct_response_once(TcpStream, &[u8])`, `Driver::TcpDirectResponse`, `drive_tcp_direct_response(SocketAddr) -> Result<Vec<u8>>`, `run_tcp_direct_response_arm`. Consistent across Tasks 1-9. `serve` takes `Arc<[u8]>` in Task 4 and Task 5 passes `Arc<[u8]>` built via `.into()`. ✅
