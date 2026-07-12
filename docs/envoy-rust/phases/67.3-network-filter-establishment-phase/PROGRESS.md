# Phase 67.3 — Implementation Progress (§5 state 3)

> Running log, appended per task by the `superpowers:executing-plans` executor.
> `PLAN.md` is authoritative for the task specs. This file records what was done,
> the TDD FAIL→PASS evidence, and the commit SHA per task. The state-4
> `superpowers:verification-before-completion` session runs the full §7.5 gate
> (PLAN Task 6) — NOT run here.

## Session: §5 state-3 implementation

Cold-started clean: `git status --porcelain` empty, branch `main`, `HEAD` at the
state-2 PLAN-write commit `b5fc211`, `git fetch origin --prune` showed no sibling
ahead → §5 state 3. STEP 0.5: the PLAN-write commit's CI run `29193771419` was
GREEN (`completed`/`success`) on the full SHA
`b5fc211c68fe50ea7daa3b24eb658fb6bf58acea`.

---

### Task 1 — `FirstByteGate` primitive + `handle_gated` default + `ChainHandler` rewire (D2 + D1 core) — DONE

- **Step 1 (test-first):** added `gate_admits_when_all_continue_and_denies_on_first_stop`
  to `crates/envoy-listener/src/lib.rs` `#[cfg(test)]` module.
- **Step 2 (confirm FAIL):** `cargo test -p envoy-listener gate_admits --no-run` →
  `error[E0433]: cannot find type FirstByteGate` / `GateOutcome` (4 errors). ✓ expected FAIL.
- **Step 3–5 (implement):**
  - Added `GateOutcome` enum (`ClientGoneEarly`/`SkippedCleanly`/`Admitted`/`Denied`)
    and `FirstByteGate` (owns the non-terminal filters; `run`/`run_for_test`/
    `evaluate_peek`/`evaluate_read_half`) just below the `NetworkFilter` trait.
  - Added the dyn-safe `ConnectionHandler::handle_gated(self: Arc<Self>, downstream, gate)`
    DEFAULT method (peek-gate → `handle`; `SkippedCleanly`/`Denied` → `close_with_drain`;
    `ClientGoneEarly` → drop).
  - Rewired `ChainHandler::handle` to build a `FirstByteGate` and delegate to
    `inner.handle_gated`. Deleted the old inline peek/loop (now in `evaluate_peek` +
    the default). Updated the `ChainHandler` doc comment to describe the delegation.
- **Step 6 (confirm PASS):** `cargo test -p envoy-listener --lib --no-fail-fast` →
  **53 passed; 0 failed**, incl. the new gate test and all `chain_handler_*`
  behavioral tests unchanged (they exercise the same observable behavior through
  the new delegation).
- **Step 7 (build + regression):** `cargo build -p envoy-bin` clean; full
  `cargo test -p envoy-listener` green.
- **Step 8 (commit):** `412e133` — "67.3 D2/D1: extract FirstByteGate + handle_gated
  default; ChainHandler delegates".
- **Invariants held:** `NetworkFilter` trait shape UNCHANGED (CF-67-3 deferred);
  echo/hcm inherit the default → byte-for-byte unchanged; `close_with_drain` /
  ADR-0124 untouched; `#![forbid(unsafe_code)]` holds.

---

### Task 2 — Refactor `TcpProxy::handle::<S>` into `connect_upstream()` + `relay()` (D4, no behavior change) — DONE

- **Step 1 (baseline):** `cargo test -p envoy-tcp --no-fail-fast` → **11 passed**
  (the plan estimated ~10; the crate has 11). Verified `cx_active_guard()` returns
  `envoy_cluster::ConnGaugeGuard`, which is `pub` and re-exported at the crate root
  (`crates/envoy-cluster/src/lib.rs:28`) — nameable from `envoy-tcp`, so no boxing
  workaround needed.
- **Step 2 (implement):**
  - Added `pub struct UpstreamConn { stream: Box<dyn AsyncReadWrite + Send + Unpin>,
    _cx_guard: ConnGaugeGuard, addr, cluster_name }` (fields private).
  - `connect_upstream(&self) -> Result<UpstreamConn, _>`: the establishment half
    (pick → `cx_active` guard → TCP connect → `cx_total().inc()` → optional upstream
    TLS handshake). Guard/tick placement preserved exactly.
  - `relay<D>(&self, downstream, up)`: the ADR-0016 half-close `select!` copy,
    unchanged from the old tail; `up` carries the guard, dropped on return.
  - `handle::<S>` now just `connect_upstream().await?` then `relay(...)`.
- **Step 3 (confirm PASS):** `cargo test -p envoy-tcp --no-fail-fast` → **11 passed;
  0 failed** — behavior identical (the regression suite is the guard).
- **Step 4 (commit):** `7be5bf2` — "67.3 D4: split TcpProxy::handle into
  connect_upstream() + relay()".
- **Invariants held:** ADR-0016 `select!` posture + `cx_active`/`cx_total`
  placement unchanged; `#![forbid(unsafe_code)]` holds.

---

### Task 3 — `TcpProxy::handle_gated` override + `relay_gated` + in-process C-1 witnesses (D1 + D3 + D4) — DONE

**§6.1 mid-execution valve was ARMED for this task.** Re-derived on contact with
reality: `relay_gated` came in at ~8 logical sub-steps (destructure `UpstreamConn`;
`into_split` downstream; `tokio::io::split` upstream; the phase-1 banner/gate
`select!` with the upstream-EOF-before-first-byte race branch; four outcome
branches: `ClientGoneEarly`, `SkippedCleanly|Denied` close, `Admitted(Some)`
re-inject+copy, `Admitted(None)` FIN). **Under the ~10 threshold → the valve did
NOT fire; no split.**

- **Step 1 (test-first, C-1 witness):** added `spawn_banner_backend` (server-first
  backend that writes `220 BANNER\r\n` on accept, records subsequent bytes) and
  `banner_reaches_a_client_that_sends_nothing_through_rbac_allow`.
- **Step 2 (confirm FAIL):** `cargo test -p envoy-tcp banner_reaches_a_client_that_sends_nothing`
  → **FAILED** with a 3s timeout ("banner must reach a byte-less client") — the
  default `handle_gated` peeks, so tcp_proxy never connected upstream. ✓ this is the
  C-1 defect the phase repairs.
- **Step 3 (implement):** verified `rust-toolchain.toml` pins `1.95.0` (≥1.74 →
  `std::io::Error::other` available). Added the `handle_gated` OVERRIDE
  (`self: Arc<Self>`; connect upstream → `relay_gated`) and the `relay_gated`
  inherent method: banner (upstream→downstream `copy`) races the
  `evaluate_read_half` gate on the downstream read half (`biased` select; the
  upstream-EOF branch finishes resolving the gate then falls through); then the
  four outcome branches. `into_split()` gives reunitable `Owned{Read,Write}Half` so
  the DENY path can `reunite` + `close_with_drain`.
- **Step 4 (confirm PASS):** the banner witness passes.
- **Step 5 (DENY + FIN witnesses):** added
  `deny_delivers_banner_then_closes_without_forwarding_the_byte` (banner delivered,
  first byte triggers DENY, backend's `recorded` handle asserts `Z` never arrived)
  and `dataless_fin_through_rbac_allow_reaches_backend_as_eof` (server-first banner
  read, client half-closes with no data, backend observes the propagated FIN as EOF).
- **Step 6 (build + PASS):** `cargo build -p envoy-bin` clean;
  `cargo test -p envoy-tcp --no-fail-fast` → **14 passed; 0 failed** (11 existing + 3
  new).
- **Step 7 (commit):** `23ff455` — "67.3 D1/D3: TcpProxy::handle_gated —
  establishment-then-gate; banner+DENY+FIN witnesses".
- **Invariants held:** DENY withholds the first byte from the upstream (W-4/R-2);
  ADR-0016 posture in the admit path; `#![forbid(unsafe_code)]` holds.

---

### Task 4 — Narrow the config-load rejection to TLS-downstream chains; re-message the variant (D5 + D6) — DONE

- **Live-tree verification (W-6 / plan's TLS note):** the composition check is at
  `bootstrap.rs:3228`; the TLS cert-content validation (`EmptyTlsCertificates` on
  empty `tls_certificates`) is at `:3144`, which runs FIRST. A `common_tls_context: {}`
  fixture would therefore fail with the wrong error. Moving the composition check
  earlier would break the terminal-not-last precedence the plan mandates, so instead
  the `chain_before_tcp_proxy_yaml_tls` helper supplies a filename-only leaf cert
  (`validate_data_source(Required::Filename)` checks only that a filename is present,
  not that the file exists), so the chain passes TLS validation and reaches the
  composition check.
- **Step 1 (test-first):** replaced `rejects_rbac_composed_with_tcp_proxy` with
  `plaintext_rbac_before_tcp_proxy_is_now_accepted` (must VALIDATE) +
  `tls_rbac_before_tcp_proxy_is_still_rejected` (composition variant, message names
  CF-67-7). Added the `chain_before_tcp_proxy_yaml_tls` helper.
- **Step 2 (confirm FAIL):** both FAILED — plaintext still rejected today; the TLS
  reject fired but its message named "phase 67.3" not "CF-67-7". (Confirmed the TLS
  fixture reaches the composition check — cert validation passed.)
- **Step 3 (implement):** added `&& chain.transport_socket.is_some()` to the
  rejection block at `:3228` and rewrote its comment (plaintext supported; TLS-only
  fail-loud; CF-67-7).
- **Step 4 (re-message):** rewrote the `UnsupportedNetworkFilterChainComposition`
  doc + `#[error]` (`lib.rs`) to name the TLS raw-TCP-accept / first-decrypted-byte
  ordering and CF-67-7, and to state the plaintext form is supported from 67.3.
- **Step 5 (precedence test):** renamed `terminal_not_last_error_wins_over_unsupported_composition`
  → `terminal_not_last_wins_for_echo_rbac_tcp_proxy` (plaintext `[echo, rbac, tcp_proxy]`
  no longer hits the composition rule; the `NetworkFilterNotTerminal` assertion is
  unchanged). Kept `lone_tcp_proxy_chain_is_still_accepted` (over-rejection guard).
- **Step 6 (confirm PASS):** `cargo test -p envoy-config --lib --no-fail-fast` →
  **587 passed; 0 failed**.
- **Step 7 (commit):** `7946aec` — "67.3 D5/D6: narrow [non-terminal, tcp_proxy]
  rejection to TLS chains (plaintext now accepted); CF-67-7".
- **Invariants held:** `is_terminal_network_filter` untouched; `filters: []` still
  accepted; over-rejection guards survive; `#![forbid(unsafe_code)]` holds.

---

### Task 5 — envoy-bin backstops over the real binary; BEHAVIOR_CONTRACT item 13 (D7 + D6 record) — DONE

- **Step 1 (delete + add plaintext/TLS pair):** deleted
  `rbac_before_tcp_proxy_is_rejected_at_config_load`; added a `spawn_banner_backend`
  helper (in-process server-first backend, records received bytes), a
  `rbac_tcp_proxy_cfg` config helper (plaintext `[rbac, tcp_proxy→backend]` +
  admin), and `plaintext_rbac_before_tcp_proxy_delivers_banner_to_a_byteless_client`
  (banner reaches a byte-less client; `cluster.backend.upstream_cx_total == 1`) +
  `tls_rbac_before_tcp_proxy_is_rejected_at_config_load` (`validate_config` rejects,
  message names CF-67-7 + both filters).
- **Step 2 (DENY + FIN matrix):** added
  `deny_before_tcp_proxy_delivers_banner_then_withholds_the_byte` (banner delivered,
  first byte → `tpd.rbac.denied == 1`, backend never records the byte) and
  `dataless_fin_ticks_allowed_for_tcp_proxy_but_not_echo` (a data-less FIN ticks
  `tpf.rbac.allowed == 1` for tcp_proxy, `ef.rbac.allowed == 0` for echo — the D3
  terminal-property asymmetry).
- **Systematic-debugging (state-5-flavored composition probe — memory
  `state5-must-probe-untested-compositions`):** the 3 new tcp_proxy backstops first
  FAILED reading `2` not `1`. Root cause: `wait_ready(data_addr)` opens a THROWAWAY
  probe connection to the `[rbac, tcp_proxy]` listener, which itself connects
  upstream (ticks `cx_total`) and, on its data-less FIN, evaluates the chain —
  polluting the counters. Fix: added `connect_when_ready` (retry-connect that KEEPS
  the stream), so the test client is the SOLE data connection. `wait_ready` is kept
  for admin only. Echo is immune (peek → `Ok(0)` → skip) but uses the same helper.
- **Step 3 (BEHAVIOR item 13):** rewrote the `tcp_proxy` bullet into the split
  outcome — plaintext = FULL PARITY (banner-to-byte-less, verdict on first byte /
  data-less FIN, DENY withholds the byte), TLS-downstream = RECORDED FAIL-LOUD
  DIVERGENCE owner CF-67-7 (raw-TCP-accept / first-decrypted-byte ordering). Updated
  the data-less-FIN paragraph (item §1) to "reproduced from 67.3". Item 14 / the
  `rbac.rs` HTTP-vs-L4 divergence untouched.
- **Step 4 (build + backstops):** `cargo build -p envoy-bin` clean;
  `cargo test -p envoy-bin --test network_filter_rbac --no-fail-fast` → **24 passed;
  0 failed**. Differential regression: `git diff` confirms NO `tests/fixtures/` or
  `known-failures.txt` edits — `0001`/`0071`/`0072`/`0073` unedited. Per the
  per-task discipline the Docker differential runs at the state-4 gate / CI
  (host-flaky, CI-authoritative — memories `envoy-rust-state4-ci-first-execution`,
  `differential-fixtures-flake-under-parallel-load`).
- **Step 5 (commit):** see below.
- **Invariants held:** every downstream read `READ_BUDGET`-bounded (M-2); ADR-0124
  drain tests unweakened; `#![forbid(unsafe_code)]` holds.

---

## State-3 exit summary

All five implementation tasks (T1–T5) landed with TDD FAIL→PASS evidence.
Task 6 (the full §7.5 verification gate) is the state-4 session's job — NOT run here
(§5.1: one state per session). Per-crate greens this session: envoy-listener 53,
envoy-tcp 14, envoy-config 587, envoy-bin/network_filter_rbac 24. §7.5 gate (d):
NO new fuzz target — network `rbac` parses nothing; satisfied by the pre-existing
`parse_bootstrap` target (RECORD explicitly at state-4). §6.1 mid-execution valve
was ARMED for T3's `relay_gated` and re-derived NOT to fire (~8 sub-steps < ~10).
Live carry-forwards unchanged: M-1 (not consumed — 67.3 doesn't touch CidrRange),
CF-67-6 (not folded — D8 opportunistic), **CF-67-7 (NEW — the TLS composition)**,
CF-67-3 (deferred). ADR ledger head unchanged at ADR-0135 (no new ADR this session;
ADR-0135 authored the resolutions at state-2).

---

## Session: §5 state-4 verification (`superpowers:verification-before-completion`)

Cold-started clean: `git status --porcelain` empty, branch `main`, `HEAD` at the
state-3-complete / STATE-advance commit `32d3804` = `origin/main`;
`git fetch origin --prune` showed no sibling ahead → §5 state 4.
**STEP 0.5 (CI confirmation, FULL 40-char SHA):** the code-complete commit's CI run
`29194938132` was GREEN (`completed`/`success`) on
`9b09bdcc93f8b8ba77eeacdaef86110867e8a143`; the docs-only STATE-advance commit
`32d3804c1e464ce321a10907508c5d9606417388` CI run `29195219802` also GREEN
(`completed`/`success`). This ran the full §7.5 gate (PLAN Task 6) and quotes every
command's output below (§5 state-4 discipline; never piped through `tail` — full
output redirected to files).

### Gate (e) — `cargo build --workspace --all-targets`

```
   Compiling envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
   Compiling http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.47s
```

**EXIT 0; 0 warnings** (`grep -c warning` → 0). `target/debug/envoy-bin` is fresh
for the differential/integration runs.

### Gate (e) — `cargo clippy --workspace --all-targets --all-features -- -D warnings`

```
    Checking envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Checking http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.92s
```

**EXIT 0; zero `^warning`/`^error`/`generated … warning` lines.** (NOT run this
phase before now, as flagged in the handoff — passed clean on first run this gate.)

### Gate (e) — `cargo fmt --all -- --check`

**EXIT 0; zero output** (no diff). Confirms the `9b09bdc` fmt fixup holds.

### Gate (e) — `cargo test --workspace --no-fail-fast`

Full output redirected to a file (memory `never-pipe-verification-runs-through-tail`).
Aggregate across **150** test-result lines: **1947 passed; 6 failed.**

The **6** failures are ALL in the documented CI-authoritative host-flake set —
none are in a phase-67.3-changed file (the phase touched `envoy-listener`,
`envoy-tcp`, `envoy-config/{bootstrap,lib}`, `envoy-bin/tests/network_filter_rbac`;
every one of those is GREEN, see below). Adjudicated per the memories, re-running
each in isolation to categorize:

| Failing test (crate) | Isolation | Class / memory |
|---|---|---|
| `access_log_h2_rcd_upstream_reset` (`differential`) | fails deterministically | upstream-reset witness fixture-0061/62/69/70; real Envoy can't reach the host-spawned close backend → reports `rf:"UF"` where envoy-rust correctly reports `rf:"UC"`. `tcpclosebackend-ipv6-unreachable-host-flake` |
| `access_log_h2_uc_upstream_reset` (`differential`) | fails deterministically | same reset-witness class |
| `access_log_rcd_upstream_reset` (`differential`) | fails deterministically | same reset-witness class |
| `access_log_rf_upstream_reset` (`differential`) | fails deterministically | same reset-witness class |
| `admin_config_dump_server_info` (`envoy-bin`) | **PASS in isolation** (`1 passed`) | parallel-load env flake (handoff-named) |
| `rds_route_to_unknown_cluster_is_fatal` (`envoy-bin`, `xds_file_based_rds`) | **PASS in isolation** (`1 passed`) | fatal-startup port-reuse race, `eds-fatal-startup-test-port-reuse-flake` class |

Confirmed failure cause on a re-run of `access_log_rf_upstream_reset` in isolation
(fixture `envoy-rust-phase-53-fixture-0061`):

```
fixture green: access log byte-exact mismatch: line 0 not byte-identical:
  envoy="{\"rc\":503,\"rf\":\"UF\"}" envoy-rust="{\"rc\":503,\"rf\":\"UC\"}"
```

i.e. **real Envoy** reports `UF` (upstream connect failure — the host-spawned close
backend is unreachable on this host), while **envoy-rust** produces the correct
`UC` (upstream connection termination). The divergence is a property of the host's
Docker/IPv6 networking, not of envoy-rust — exactly the documented flake.

**Cross-check (memory `local-red-set-varies-run-to-run`):** `local passed + failed
= 1947 + 6 = 1953` **==** CI run `29194938132` (`9b09bdc`, green) **1953 passed**.
Exact match → the 6 local REDs are precisely the tests that pass on CI; no real
regression, no missing/extra test.

**Phase-touched test binaries — all GREEN in this workspace run:**

```
envoy_config       => test result: ok. 587 passed; 0 failed
envoy_listener     => test result: ok.  53 passed; 0 failed
envoy_tcp          => test result: ok.  14 passed; 0 failed   (new banner/DENY/FIN witnesses)
network_filter_rbac=> test result: ok.  24 passed; 0 failed   (establishment backstops + FIN matrix)
```

Matches the state-3 exit greens exactly. **NOT a §5.2 re-entry** — the gate's real
signal is fully green; the 6 REDs are CI-authoritative host flakes.

### Gate (e) — `cargo deny check`

```
advisories ok, bans ok, licenses ok, sources ok
```

**EXIT 0.** (The five `license-not-encountered` warnings for unmatched allowances
in `deny.toml` are benign and pre-existing, not errors.) No fresh-advisory RED.

### Gate (a)/(b) — Differential surface

`git diff --name-only b5fc211..HEAD | grep -iE 'fixtures|known-failures|0001|0071|0072|0073'`
→ **NONE.** `0001`/`0071`/`0072`/`0073` and every other fixture UNEDITED; only
production code, tests, and docs changed. The Docker differential first runs at this
gate; the four upstream-reset witnesses that executed locally are the known
host-flake set above (memories `envoy-rust-state4-ci-first-execution`,
`differential-host-bridge-ip-192-168-65-2`, `tcpclosebackend-ipv6-unreachable-host-flake`) —
**CI is authoritative** and is green on `9b09bdc`.

### Gate (c) — Conformance

`known-failures.txt` UNEDITED (same `git diff` grep → NONE). `h2spec` corpus
unchanged; never trimmed (memory `h2spec-3-5-2-preface-host-sensitive`). CI-authoritative.

### Gate (d) — Fuzz

**Satisfied by the pre-existing `parse_bootstrap` target; NO new fuzz target.**
Network `rbac` parses nothing (the phase adds no config-parse surface — it splits an
in-process handler and narrows an existing rejection), so there is no new input to
fuzz. Recorded explicitly per the handoff (not skipped silently).

### State-4 exit summary

**The §7.5 six-gate checklist is GREEN.** build EXIT 0 / 0 warnings; clippy EXIT 0
clean; fmt EXIT 0; test 1947 passed with the 6 REDs adjudicated as the documented
CI-authoritative host-flake set (local `1953` == CI `1953 passed`); deny EXIT 0;
differential surface + `known-failures.txt` UNEDITED; gate (d) satisfied by
`parse_bootstrap`. No real regression → **NOT a §5.2 re-entry.** The state advances
to §5 state 5 (`superpowers:requesting-code-review` → `REVIEW.md`, next session).
Per §5.1 this session did NOT chain into state-5. No new ADR (ledger head ADR-0135).
Live carry-forwards unchanged: M-1, CF-67-6, **CF-67-7 (NEW — TLS composition)**,
CF-67-3.
