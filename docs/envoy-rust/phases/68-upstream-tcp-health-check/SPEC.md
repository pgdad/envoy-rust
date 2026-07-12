# Phase 68 — Upstream-robustness family: active **TCP** health checking (`tcp_health_check`)

> **Status:** `in-progress` (§5 state-1 brainstorm output). This SPEC is the
> brainstorming deliverable for a stranger with zero prior context (D-3.4).
> Every load-bearing wire/behavior claim in §0 was MEASURED against the pinned
> reference `envoyproxy/envoy:v1.33.0` (D-3.3 / D-3.7) during the state-0 recon
> of this session; nothing here is asserted from memory or upstream source.
>
> **Pick + scope recorded in ADR-0136.** The next session is the §5 state-2
> PLAN-write (`superpowers:writing-plans`) — do NOT implement from this SPEC.

---

## §0. State-0 recon — evidence (MEASURED this session against `envoyproxy/envoy:v1.33.0`)

### R-0.1 — envoy-rust today supports HTTP health checks ONLY; TCP/gRPC are deferred fail-loud

Phase 12 (`12`/`12.1`/`12.2`) landed active **HTTP** health checking. The config
layer models the health checker as `HealthCheck.http_health_check: Option<HttpHealthCheck>`
(`crates/envoy-config/src/bootstrap.rs:2447`) and the validator surfaces any
non-HTTP checker as `ConfigError::UnsupportedHealthCheckType`
(`crates/envoy-config/src/lib.rs:732`, message `"cluster '{cluster}' health check
is not an http_health_check; phase 12 supports HTTP health checks only"`). Because
`HealthCheck` carries `#[serde(deny_unknown_fields)]`, a `tcp_health_check:` key is
rejected today (there is a pinning test `cluster_rejects_unknown_health_check_field`
at `bootstrap.rs:14956` feeding `tcp_health_check: {}`). **This phase adds the TCP
checker arm.**

### R-0.2 — the timing/threshold state machine, probe scheduler, ejection, and stats ALREADY exist (phase 12) and are checker-type-agnostic

- `HealthCheck` already parses the shared knobs `timeout` / `interval` /
  `healthy_threshold` / `unhealthy_threshold` (`bootstrap.rs:2432`); the
  TCP checker only ADDS its own probe-shape sub-message.
- `crates/envoy-health/` already provides `Scheduler` (`scheduler.rs:28`, spawns
  per-endpoint probe tasks + reaps its `JoinSet`), the probe abstraction
  (`probe.rs`), and `HealthError` (`error.rs`). This phase adds a TCP probe
  variant alongside the existing HTTP probe.
- The `EndpointHealth` consecutive-success/failure state machine + `pick()`
  unhealthy-exclusion + the ejection wiring already exist from `12.1`/`12.2`.
- The stat tree (see R-0.5) is emitted by the shared health-check machinery, NOT
  by the HTTP checker — a TCP checker witnesses the **identical** names.

The load-bearing consequence: this phase is a **cheapest-strong-differential**
leaf — it reuses phase 12's scheduler, state machine, ejection, stats, AND the
fixture-0019 differential harness, adding only (a) the TCP probe-shape schema +
validation and (b) the L4 probe task.

### R-0.3 — LIVE-ENVOY: `tcp_health_check` wire shape (`--mode validate`, networking-free)

Measured with `docker run … --mode validate -c cfg.yaml` (memory
`mode-validate-probes-wire-shape-networking-free`). All four accepted forms
returned `configuration '…' OK`:

| Config | Result | Meaning |
|---|---|---|
| `tcp_health_check: {}` | **OK** | empty ⇒ **connection-only** check (connect succeeds ⇒ healthy) |
| `tcp_health_check: { send: { text: "000102" }, receive: [ { text: "0304" } ] }` | **OK** | `send` is a single `Payload`; `receive` is a **repeated** `Payload` |
| `tcp_health_check: { receive: [ { text: "50494e47" } ] }` | **OK** | `receive`-only (no `send`): match against the server's unsolicited banner |
| `tcp_health_check: { send: { binary: "AAECAw==" }, receive: [ { binary: "AAECAw==" } ] }` | **OK** | `Payload` alt field `binary` = base64 bytes |

**`Payload` is a oneof `{ text | binary }`** where `text` is a **hex string** and
`binary` is base64. Measured load-fatal rejections (at config-load, before any
socket):

- `send: { text: "0" }` (odd length) → `error initializing configuration …:
  invalid hex string '0'`
- `send: { text: "zzzz" }` (non-hex) → `error initializing configuration …:
  invalid hex string 'zzzz'`

### R-0.4 — LIVE-ENVOY: the health checker is a proto `oneof` — `http_health_check` + `tcp_health_check` together is REJECTED

A `HealthCheck` carrying BOTH `http_health_check: { path: /z }` and
`tcp_health_check: {}` fails at load:

```
error initializing configuration: … 'http_health_check' has already been set
(either directly or as part of a oneof)
```

envoy-rust models the two as independent `Option<_>` fields; the validator must
therefore reject "both present" as a fail-loud divergence-free parity error
(the natural mapping of the upstream oneof onto two serde Options — the
`bootstrap.rs` precedent for oneof-as-two-Options).

### R-0.5 — LIVE-ENVOY: behavior + stat tree (sibling-container backend on a shared docker network; memory `state0-recon-backend-sibling-container`)

Four STATIC clusters, each one endpoint, `interval: 1s, timeout: 1s,
healthy_threshold: 1, unhealthy_threshold: 2`, probed against a Python banner
backend that sends `PING` on connect. `/clusters` `health_flags` after settle:

| Cluster | `tcp_health_check` | Backend | `health_flags` | Outcome |
|---|---|---|---|---|
| `c_match` | `receive: [{text:"50494e47"}]` (=`PING`) | sends `PING` | `healthy` | receive matched ⇒ **healthy** |
| `c_mismatch` | `receive: [{text:"504f4e47"}]` (=`PONG`) | sends `PING` | `/failed_active_hc/active_hc_timeout` | never matched ⇒ **times out** ⇒ unhealthy |
| `c_conn_only` | `{}` | any listener | `healthy` | connect succeeded ⇒ **healthy** |
| `c_dead` | `{}` → port 9999 (refused) | — | `/failed_active_hc` | connect refused ⇒ **unhealthy** |

Per-cluster stats (identical tree to phase-12 HTTP-HC — this phase witnesses the
SAME names via the TCP checker, adding NO new stat names):

- Healthy (`c_match`, `c_conn_only`): `cluster.<n>.health_check.attempt`↑,
  `.success`↑, `.healthy: 1`, `.failure: 0`, `.network_failure: 0`;
  `membership_healthy: 1`, `membership_total: 1`.
- Unhealthy (`c_mismatch` timeout, `c_dead` refuse): `.failure: 1`,
  `.network_failure: 1`, `.success: 0`, `.healthy: 0`; `membership_healthy: 0`,
  `membership_total: 1`.

**Key TCP-specific correctness facts:** (1) a `receive` match is a scan of the
inbound bytes for the configured payload(s); a payload that never arrives makes
the probe **hang until `timeout`** (counted `failure` + `network_failure`,
health_flag `active_hc_timeout`) rather than failing fast. (2) A **connect
refusal** is an immediate `failure` + `network_failure` (`/failed_active_hc`, no
`active_hc_timeout` suffix). (3) An empty check is connection-only.

### R-0.6 — the differential harness ALREADY witnesses HC ejection via a downstream-visible 503 (fixture 0019)

Fixture `0019-upstream-active-health-check` (phase 12.2) proves HTTP-HC ejection
NOT by the timing-sensitive counters but by the **downstream consequence**: an
HCM/router listener → a cluster whose sole endpoint fails the checker →
`healthy_panic_threshold: { value: 0 }` disables panic → after a `settle_ms:
3500` wait the endpoint is Unhealthy → `pick()` returns `None` → the H1 HCM fires
synth-503 body `"no healthy upstream"` (19 bytes, ADR-0037). The
`http1_after_settle` driver + `HealthAwareHttp1Backend` helper + the
`membership_healthy`-gauge assertion (`tests/differential/src/lib.rs:1662`) are
all reusable verbatim. **A cluster used by an HTTP router may carry a
`tcp_health_check`** — health checks are cluster-level and L4-agnostic to the
listener's traffic protocol (the checker probes the raw socket).

### R-0.7 — numbering

Next ROADMAP id is **68** (highest defined is `67`, with `67.1`/`67.2`/`67.3`;
`59`/`60`/`62` are intentional gaps). Next fixture id is **0074** (`0073` is the
last, phase 67). Next ADR is **ADR-0136** (ledger head `ADR-0135`; reclaimed by
this pick per the lapsed-reservation convention).

---

## §1. Goal

Land active **TCP** health checking (`envoy.config.core.v3.HealthCheck.tcp_health_check`)
as the upstream-robustness family's second health-check checker type, behaviorally
equivalent to `envoyproxy/envoy:v1.33.0` under the differential contract (§7):

- **Connection-only** checks (empty `tcp_health_check: {}`): connect success ⇒
  the endpoint is Healthy and LB-eligible; connect failure ⇒ after
  `unhealthy_threshold` consecutive failures the endpoint is ejected.
- **Send/receive** checks: optionally write the `send` payload, then scan the
  inbound bytes for the `receive` payload(s); match ⇒ healthy; no match within
  `timeout` ⇒ failure (`active_hc_timeout`).
- The SAME `cluster.<n>.health_check.*` + `membership_*` stat tree phase 12
  established, now driven by the TCP checker.
- Reuse phase 12's scheduler, `EndpointHealth` state machine, ejection, and
  `pick()` exclusion unchanged.

**Differential surface at phase end:** a new fixture `0074` witnessing TCP-HC
ejection byte-exact via the same downstream-503 observable as `0019`, plus
in-process coverage of the healthy connection-only and receive-match paths.

---

## §2. Scope

### 2.1 In scope

1. **Config schema (`crates/envoy-config`).** Add
   `HealthCheck.tcp_health_check: Option<TcpHealthCheck>` (serde `default`,
   alongside the existing `http_health_check`). New sub-types:
   - `TcpHealthCheck { send: Option<HealthCheckPayload>, receive: Vec<HealthCheckPayload> }`
     (`#[serde(deny_unknown_fields, default)]`).
   - `HealthCheckPayload` = a `{ text | binary }` oneof-as-two-Options; `text` is
     decoded from a **hex string** (odd-length / non-hex → a new fail-loud
     `ConfigError` mirroring upstream's `invalid hex string '…'`, R-0.3), `binary`
     from base64. Decoded to `Bytes`/`Vec<u8>` at parse time.
2. **Validation (`crates/envoy-config`).** (a) A `HealthCheck` with BOTH
   `http_health_check` AND `tcp_health_check` present → a new fail-loud
   `ConfigError` (the oneof, R-0.4). (b) A `HealthCheck` with NEITHER → the
   existing `UnsupportedHealthCheckType` path (unchanged). (c) The shared
   timing/threshold validators (interval/timeout/thresholds) apply unchanged. (d)
   The `UnsupportedHealthCheckType` message + the R-0.1 pinning test are updated
   to reflect that TCP is now supported (gRPC/custom still deferred).
3. **TCP probe task (`crates/envoy-health`).** A new probe variant: open a TCP
   connection to the endpoint (honoring `connect_timeout`/`timeout`); if `send`
   is set, write it; if `receive` is non-empty, read and scan for the payload(s)
   (a Healthy verdict on match, a `timeout`/EOF failure otherwise); if `receive`
   is empty, a successful connect ⇒ Healthy, then close. Feed the boolean
   outcome into the existing `EndpointHealth` state machine exactly as the HTTP
   probe does.
4. **Dispatch wiring (`crates/envoy-cluster` / wherever the checker type is
   selected).** Select the TCP probe when `tcp_health_check` is present; the HTTP
   path is unchanged. No change to the scheduler, ejection sweeper, or `pick()`.
5. **Differential fixture `0074-upstream-tcp-health-check`.** HCM/router listener
   → cluster with one endpoint carrying a `tcp_health_check` that FAILS (the
   cheapest deterministic failure: a connection-only check against a
   refusing/closed port, mirroring `0019`'s ejection) → after settle → synth-503
   `"no healthy upstream"` byte-exact, via the reused `http1_after_settle` driver.
   The exact backend/failure mode (dead-port connect-refusal vs a banner
   `receive`-mismatch timeout) is a state-2 decision (§3 PV-2).
6. **In-process coverage.** Unit tests in `envoy-health`/`envoy-config` for: the
   hex/base64 payload decode + fail-loud rejections; the both-checkers oneof
   rejection; the connection-only Healthy path; a receive-match Healthy path; a
   receive-mismatch/connect-refuse Unhealthy path.
7. **`BEHAVIOR_CONTRACT.md`** — a `tcp_health_check` subsection (§6).
8. **`known-failures.txt` / conformance** — unchanged (no protocol conformance
   surface; never trimmed, memory `h2spec-3-5-2-preface-host-sensitive`).

### 2.2 Out of scope (deliberate, with rationale)

- **gRPC health check** (`grpc_health_check`) and **custom health check** — stay
  fail-loud `UnsupportedHealthCheckType`. gRPC needs the health-check gRPC
  service protocol; a separate future phase.
- **`send`-only with no `receive`** semantics beyond "write then treat connect as
  success" — Envoy sends then (with empty `receive`) closes; covered, but no
  dedicated fixture. In-process only if cheap.
- **Multiple `receive` blocks ordering** — the schema accepts `Vec`, but the
  fixture uses ≤1 block; the multi-block in-order scan is an in-process unit test
  at most, not a differential (keeps the fixture deterministic).
- **`reuse_connection`, `unhealthy_interval`, `unhealthy_edge_interval`,
  `healthy_edge_interval`, `no_traffic_interval`, `initial_jitter`,
  `interval_jitter`, `always_log_health_check_failures`, `event_log_path`,
  `tls_options`, `transport_socket_match_criteria`** — all deferred; not needed
  for the connection-only + basic send/receive differential. Any that
  `deny_unknown_fields` would reject are simply absent from fixtures.
- **Passive health checking / outlier interaction** — phase 14 owns outlier
  detection; no cross-wiring changed here.

### 2.3 §7.4 fuzz disposition

The `tcp_health_check` config surface (hex/base64 payload decode) is a new parser
input. **Confirm at the state-2 PLAN-write** whether the existing
`parse_bootstrap` fuzz target already exercises `HealthCheck` sub-message decode
(likely yes — it fuzzes the whole bootstrap) and whether a `parse_bootstrap` seed
carrying a `tcp_health_check` (with a hex `send`/`receive`) is the correct §7.4
discharge, versus a dedicated hex-decode fuzz target. Default projection: a new
`parse_bootstrap` seed, no new fuzz target (the cdn_loop/csrf precedent — a config
sub-message reusing the bootstrap parser). If a dedicated target is added it MUST
be wired into `ci.yml` by hand (memory `new-fuzz-target-needs-a-ci-yml-step`) and
its corpus seed un-ignored (memory `fuzz-corpus-seed-gitignored-by-default`).

---

## §3. PLAN-VERIFY items (re-confirm against the live tree at the state-2 PLAN-write)

- **PV-1 — hex-decode error shape.** Confirm the exact envoy-rust `ConfigError`
  variant name + message for odd-length / non-hex `text` (upstream: `invalid hex
  string '<s>'`). Decide byte-for-byte message parity vs a native message (the
  ADR-0049 fail-loud posture permits a native message; parity is nice-to-have).
- **PV-2 — the fixture's deterministic failure mode.** Choose between (a) a
  connection-only check against a refusing/closed backend port (simplest;
  mirrors `0019`'s ejection; no new helper) and (b) a banner-backend
  `receive`-mismatch timeout (exercises the receive-scan path differentially but
  needs a TCP-banner backend helper + a longer settle for the timeout). Measure
  both settle budgets. Default: (a) for the ejection differential + an in-process
  receive-match test, unless (b) is cheap.
- **PV-3 — `send`/`receive` semantics precision.** Re-measure whether `send` is
  written once before the first read, and whether the `receive` scan is a
  contiguous match or an in-order multi-block "fuzzy" match — only to the extent
  the fixture/tests assert it. Assert only what is measured (D-3.3).
- **PV-4 — the both-checkers rejection site.** Confirm where in the phase-12
  validator the `http_health_check`-present branch lives, so the "both present"
  arm slots in without disturbing the existing `UnsupportedHealthCheckType`
  neither-present arm.
- **PV-5 — §6.1 size re-derivation.** Re-estimate net LoC / task count against
  the live tree (see §8). If > ~1500 LoC or > ~25 tasks, split (schema+validation
  / probe+wiring+fixture).
- **PV-6 — `connect_timeout` vs `timeout` interaction** for the TCP probe (which
  bounds the connect phase vs the receive phase). Measure/confirm at PLAN-write.

---

## §4. Rejected / deferred alternatives (what this pick was chosen over)

- **Network-filters family remainder (`redis`/`thrift`/`mongo`/`kafka_broker`/
  `zookeeper`).** Each forces a payload-parsing `on_data` protocol subsystem
  (CF-67-3 buffering) — a large new codec per filter. Far above the
  cheapest-strong-differential bar.
- **`sni_cluster` network filter.** Needs a `tls_inspector` LISTENER-filter
  subsystem envoy-rust lacks (a whole new filter category). Deferred.
- **`echo` network-filter differential fixture.** `echo` is ALREADY implemented
  (`crates/envoy-bin/src/echo.rs`, `EchoHandler`); a fixture-only phase adds no
  code — too thin.
- **gRPC health check.** Needs the gRPC health-check service protocol; heavier
  than the L4 TCP checker. A later upstream-robustness leaf.
- **Load-balancing `least_request`/`random`.** Non-deterministic (P2C / random
  selection) — need a contract-relaxation ADR before a differential is possible.
- **CF-67-7 (the TLS `[rbac, tcp_proxy]` establishment ordering).** A deliberate
  fail-loud divergence owned by a future TLS-establishment phase; not the
  cheapest strong differential and touches the sensitive TLS handler.

TCP health checking wins: it reuses the ENTIRE phase-12 health machinery + the
`0019` differential harness (cheapest), is fully deterministic on the
downstream-503 observable (strong), and needs no new subsystem.

---

## §5. Differential surface at phase end

- **NEW fixture `0074-upstream-tcp-health-check`** — green cross-proxy: an
  HCM/router listener → a cluster whose sole endpoint fails a `tcp_health_check`
  → `healthy_panic_threshold: { value: 0 }` → after `settle_ms` the endpoint is
  Unhealthy → synth-503 `"no healthy upstream"` byte-exact (19 bytes, ADR-0037),
  via the reused `http1_after_settle` driver + `set_equal_modulo_allow_list`
  headers (the `0019` discipline verbatim).
- **All pre-existing fixtures `0001`–`0073` stay green** — the TCP checker is
  inert unless a cluster configures `tcp_health_check`; no existing fixture does
  (§7.5 (b)).
- **In-process:** the healthy connection-only + receive-match paths, the
  fail-loud hex/base64 + both-checkers rejections.

---

## §6. `BEHAVIOR_CONTRACT.md` additions

A `tcp_health_check` subsection recording the MEASURED facts (R-0.3–R-0.5):
empty ⇒ connection-only; `send`/`receive` `Payload` = hex `text` | base64
`binary`; odd/non-hex `text` load-fatal; `http+tcp` oneof rejection; the shared
`cluster.<n>.health_check.*` + `membership_*` stat tree (unchanged names); the
`healthy` / `/failed_active_hc` / `/failed_active_hc/active_hc_timeout`
`health_flags`; receive-no-match ⇒ timeout-failure, connect-refuse ⇒
immediate-failure (both `failure` + `network_failure`).

---

## §7. ADR reservations

- **ADR-0136 (FIRED this session):** the phase-68 pick + scope + rejected
  alternatives (this SPEC's decisions).
- **ADR-0137 (reserved):** the §6.2 empirical-verification reconciliation at the
  state-2 PLAN-write (PV-1..PV-6 resolutions — hex-error shape, fixture failure
  mode, send/receive precision, connect/timeout interaction).
- **ADR-0138 (reserved):** the §6.1 split, if PV-5 fires it.

---

## §8. Estimated size (for the §6.1 split gate at state-2)

| Area | Net LoC (rough) |
|---|---|
| `envoy-config`: `TcpHealthCheck` + `HealthCheckPayload` schema, hex/base64 decode, both-checkers + hex-error `ConfigError` variants, validator arm | ~260 |
| `envoy-health`: TCP probe task (connect / optional send / receive-scan / timeout) | ~200 |
| dispatch wiring (checker-type selection) | ~80 |
| fixture `0074` (2 YAMLs + expectations + README) + harness reuse | ~150 |
| unit + in-process tests | ~300 |
| `BEHAVIOR_CONTRACT.md` + ROADMAP/docs | ~60 |
| **Total** | **~1050 net LoC / ~10–14 tasks** |

Projected **single-phase** (under the ~1500 LoC / ~25 task gate). PV-5 re-derives
at the state-2 PLAN-write; ADR-0138 held in reserve for a schema+validation /
probe+wiring+fixture split if reality exceeds the gate.

---

## §10. Carry-forwards NOT consumed by this pick (surviving phase 67's close)

None obligate this phase; each is owned by whatever future phase touches its
surface.

- **M-1** — the `CidrRange` `prefix_match` guard band (owner = next phase touching
  the CidrRange surface). **This phase does NOT touch CidrRange.**
- **CF-67-3** — payload-visible `on_data` network-filter iteration (deferred;
  owned by the first payload-parsing network filter).
- **CF-67-5** — empty `filters: []` connection behavior.
- **CF-67-6** — bound `close_with_drain`'s steady-state drain (`delayed_close_timeout`).
- **CF-67-7** — the TLS `[rbac, tcp_proxy]` establishment ordering (owner = a
  future TLS-establishment phase).
- The older still-live Minors in `67.3/SPEC.md` §10 and the HTTP-filters-family
  carry-forwards (1)–(4) in `STATE.md` `## Notes`.
