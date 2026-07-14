# Phase 69 — Upstream-robustness family: active **gRPC** health checking (`grpc_health_check`)

> **Status:** `in-progress` (§5 state-1 brainstorm output). This SPEC is the
> brainstorming deliverable for a stranger with zero prior context (D-3.4).
> Every load-bearing wire/behavior claim in §0 was MEASURED against the pinned
> reference `envoyproxy/envoy:v1.33.0` (D-3.3 / D-3.7) during the state-0 recon
> of this session; nothing here is asserted from memory or upstream source.
>
> **Pick + scope recorded in ADR-0138** (reclaimed — the lapsed phase-68 split
> reservation, per the lapsed-reservation convention; ledger head was ADR-0137).
> The next session is the §5 state-2 PLAN-write (`superpowers:writing-plans`) —
> do NOT implement from this SPEC.

---

## §0. State-0 recon — evidence (MEASURED this session against `envoyproxy/envoy:v1.33.0`)

gRPC health checking is the **THIRD** health-check checker type after HTTP
(phase 12) and TCP (phase 68). Like TCP, it is a **cheapest-strong-differential
leaf**: it reuses the ENTIRE phase-12/68 health machinery — the `envoy-health`
`Scheduler`, the `EndpointHealth` consecutive-success/failure state machine,
`pick()` unhealthy-exclusion, the ejection sweeper, the `cluster.<n>.health_check.*`
+ `membership_*` stat tree, AND the fixture-`0019`/`0074` downstream-503
ejection observable — adding only (a) the tiny `grpc_health_check` config
schema + validation, and (b) a gRPC-unary probe task over the EXISTING
`envoy-http2` client. It introduces **no new subsystem** (it composes the
existing health scheduler with the existing upstream-H2 client).

### R-0.1 — envoy-rust today supports HTTP + TCP health checks; gRPC is deferred fail-loud

Phases 12 (`12`/`12.1`/`12.2`) and 68 landed active **HTTP** and **TCP** health
checking. The config layer models the checker as two `#[serde(default)]
Option<_>` fields on `HealthCheck` (`http_health_check`, `tcp_health_check`;
`crates/envoy-config/src/bootstrap.rs:2430`-`2453`). The validator
`validate_health_checks` (`bootstrap.rs:4762`-`4830`) enforces the oneof: a
`HealthCheck` with BOTH → `ConfigError::BothHttpAndTcpHealthCheck`
(`lib.rs:746`), with NEITHER → `ConfigError::UnsupportedHealthCheckType`
(`lib.rs:738`). A `grpc_health_check:` key is currently REJECTED — there is a
pinning test at `bootstrap.rs:15188` feeding `grpc_health_check: {}` on a
non-H2 cluster and asserting `parse_bootstrap(...).is_err()` (repointed from
`tcp_health_check` at the phase-68 state-3). **This phase adds the gRPC checker
arm.**

### R-0.2 — the timing/threshold state machine, scheduler, ejection, and stats ALREADY exist and are checker-type-agnostic

- `HealthCheck` already parses the shared knobs `timeout` / `interval` /
  `healthy_threshold` / `unhealthy_threshold`; the gRPC checker only ADDS its
  own sub-message.
- `crates/envoy-health/` already provides `Scheduler` (`scheduler.rs`, spawns
  per-endpoint probe tasks + reaps its `JoinSet`; the checker-type dispatch is a
  `match (&http_cfg, &tcp_cfg)` at `scheduler.rs:115`-`155`), the probe loops
  (`probe.rs` — `probe_loop`/`tcp_probe_loop`, structurally identical: `interval`
  ticker + cancel-select + `attempt.inc()` / `Ok`→`success.inc()`+`record_success`
  / `Err`→`failure.inc()`+`record_failure`), and `HealthError` (`error.rs`).
- The `EndpointHealth` consecutive-success/failure state machine
  (`crates/envoy-cluster/src/health.rs:65,77`) + `pick()` exclusion + the
  ejection wiring already exist from `12.1`/`12.2`.
- The stat tree (R-0.5) is emitted by the shared health-check machinery, NOT by
  a specific checker — a gRPC checker witnesses the **identical** names.

### R-0.3 — LIVE-ENVOY: `grpc_health_check` REQUIRES an HTTP/2-upstream cluster (load-fatal otherwise)

Measured with `docker run … --mode validate -c cfg.yaml` (networking-free,
memory `mode-validate-probes-wire-shape-networking-free`). A `grpc_health_check`
on a plain (H1-default) cluster is **load-fatal**:

```
error initializing configuration '…': c cluster must support HTTP/2 for gRPC healthchecking
```

This holds for EVERY `grpc_health_check` form tested (`{}`, `{service_name}`,
`{authority}`, `{initial_metadata}`). The cluster becomes acceptable only when
upstream H2 is enabled. Two accepted forms:

| Cluster H2 config | Result |
|---|---|
| `typed_extension_protocol_options: { envoy.extensions.upstreams.http.v3.HttpProtocolOptions: { "@type": …HttpProtocolOptions, explicit_http_config: { http2_protocol_options: {} } } }` | **OK** (modern, no warning) |
| `http2_protocol_options: {}` (inline, deprecated) | **OK** + a `Deprecated field … http2_protocol_options` warning |

**envoy-rust already supports the modern form**: `crates/envoy-config` parses
`typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options`
(`bootstrap.rs:218`-`225`; validator arms at `lib.rs:304`-`328`), and there is
an upstream-H2 client + connection pool (`crates/envoy-http2/src/client.rs`,
`pool.rs`; test `crates/envoy-bin/tests/upstream_h2_connection_pooling.rs`).

**Load-bearing consequence for the fixture (R-0.6):** envoy-rust DEFERS the
H1-listener × H2-cluster dispatch — a config where an HCM `codec_type: HTTP1`
listener routes to an H2-upstream cluster is REJECTED at load
(`ConfigError` at `lib.rs:454`-`463`, "…H1-listener × H2-cluster dispatch is
deferred per ADR-0028"). Since a gRPC-HC cluster MUST be H2-upstream, the
downstream-ejection differential fixture CANNOT reuse `0074`'s H1 listener
verbatim — it needs an **H2 listener** (`codec_type: HTTP2`) → H2 cluster (see
§5 / PV-2).

### R-0.4 — LIVE-ENVOY: accepted `grpc_health_check` wire shape

With upstream H2 enabled, all forms validated `configuration '…' OK`:

| `grpc_health_check` value | Result | Meaning |
|---|---|---|
| `{}` | OK | probe the OVERALL server (empty gRPC service name `""`) |
| `{ service_name: "my.svc" }` | OK | probe the named gRPC health service |
| `{ authority: "hc.example.com" }` | OK | override the `:authority` header on the probe |
| `{ service_name: s, initial_metadata: [ { header: { key: x-hc, value: "1" } } ] }` | OK | attach request headers to the probe |

`GrpcHealthCheck` fields (upstream `envoy.config.core.v3.HealthCheck.GrpcHealthCheck`):
`service_name` (string; empty ⇒ overall server), `authority` (string; `:authority`
override), `initial_metadata` (repeated `HeaderValueOption`).

### R-0.5 — LIVE-ENVOY: the checker is a proto `oneof` — `http_health_check` + `grpc_health_check` together is REJECTED

A `HealthCheck` carrying BOTH `http_health_check: { path: /z }` and
`grpc_health_check: {}` fails at load with the SAME oneof error phase 68
measured for http+tcp:

```
Unable to parse JSON as proto … 'http_health_check' has already been set
(either directly or as part of a oneof)
```

envoy-rust models the checkers as independent `Option<_>` fields; the validator
must therefore reject "more than one present" as a fail-loud parity error (the
`bootstrap.rs` precedent: the phase-68 both-checkers rejection at
`bootstrap.rs:4770`-`4774`, generalized to "at most one of three").

### R-0.6 — LIVE-ENVOY: runtime behavior + stat tree (sibling-container gRPC health backend on a shared docker network; memory `state0-recon-backend-sibling-container`)

Four STATIC clusters, each one endpoint, H2-upstream, `interval: 1s, timeout:
1s, healthy_threshold: 1, unhealthy_threshold: 2`, probed against a Python
`grpc_health.v1` backend serving `"" = SERVING`, `svc.up = SERVING`,
`svc.down = NOT_SERVING`. `/clusters` `health_flags` after settle:

| Cluster | `grpc_health_check` | Backend status | `health_flags` | Outcome |
|---|---|---|---|---|
| `c_up` | `{}` (overall `""`) | SERVING | `healthy` | **healthy** |
| `c_named_up` | `{ service_name: "svc.up" }` | SERVING | `healthy` | **healthy** |
| `c_down` | `{ service_name: "svc.down" }` | NOT_SERVING | `/failed_active_hc` | **unhealthy** (app-level) |
| `c_dead` | `{}` → refused port | (connect refused) | `/failed_active_hc` | **unhealthy** (transport) |

Per-cluster stats (IDENTICAL tree to phase-12 HTTP-HC and phase-68 TCP-HC — this
phase witnesses the SAME names via the gRPC checker, adding **NO new stat names**):

- Healthy (`c_up`, `c_named_up`): `cluster.<n>.health_check.attempt: 1`,
  `.success: 1`, `.healthy: 1`, `.failure: 0`, `.network_failure: 0`;
  `membership_healthy: 1`, `membership_total: 1`.
- NOT_SERVING (`c_down`): `.attempt: 1`, `.failure: 1`, **`.network_failure: 0`**,
  `.success: 0`, `.healthy: 0`; `membership_healthy: 0`, `membership_total: 1`.
- Connect-refused (`c_dead`): `.attempt: 1`, `.failure: 1`,
  **`.network_failure: 1`**, `.success: 0`, `.healthy: 0`; `membership_healthy: 0`.

**Key gRPC-specific correctness facts:** (1) `HealthCheckResponse.status ==
SERVING(1)` ⇒ healthy; any other status (`NOT_SERVING`, `UNKNOWN`,
`SERVICE_UNKNOWN`) ⇒ failure. (2) A **NOT_SERVING** response is an
application-level `failure` that does NOT tick `network_failure` (the gRPC call
completed) — distinct from TCP's `receive`-mismatch, which times out. (3) A
**connect refusal** (or any transport failure) ticks BOTH `failure` +
`network_failure`, the same as HTTP/TCP. (4) An empty `grpc_health_check: {}`
probes the overall server (service name `""`).

### R-0.7 — the gRPC probe is a unary `grpc.health.v1.Health/Check` RPC over H2

The wire contract of the probe (to be reproduced by envoy-rust's probe task):
`POST /grpc.health.v1.Health/Check` on an H2 connection, `content-type:
application/grpc`, `te: trailers`; request body = a length-prefixed gRPC frame
(1 compression byte `0x00` + 4-byte big-endian length + a `HealthCheckRequest{
service: <name> }` protobuf); response = HTTP `:status 200` + `content-type:
application/grpc` + a gRPC frame (`HealthCheckResponse{ status: ServingStatus
}`) + the **`grpc-status` TRAILER** (`0` = OK). Verdict = (`grpc-status == 0`
AND `HealthCheckResponse.status == SERVING`) ⇒ healthy.

**Reuse gap (measured):** the existing `envoy-http2` client
(`client.rs:125`-`224`) sends request bodies but **never reads trailers** (zero
`trailers()` calls) — and `grpc-status` lives in a trailer. The single genuinely
new primitive is a **trailers-aware unary call** on top of the existing `h2`
client. The two health protobuf messages are one-field each
(`HealthCheckRequest{ string service = 1 }`, `HealthCheckResponse{ ServingStatus
status = 1 }`, enum `UNKNOWN=0/SERVING=1/NOT_SERVING=2/SERVICE_UNKNOWN=3`) and
are hand-encoded/decoded consistent with the repo's hand-rolled-proto ethos
(there is NO `prost`/`tonic`/`build.rs`/`.proto` in the tree; xDS is
file-based). See §2 / PV-3.

### R-0.8 — numbering

Next ROADMAP id is **69** (highest defined is `68`; `59`/`60`/`62` are
intentional gaps). Next fixture id is **0075** (`0074` is the last). Next ADR is
**ADR-0138** (reclaimed — ledger head `ADR-0137`; ADR-0138 was reserved-unfired
for the phase-68 split, which did not fire).

---

## §1. Goal

Land active **gRPC** health checking (`envoy.config.core.v3.HealthCheck.grpc_health_check`)
as the upstream-robustness family's THIRD health-check checker type,
behaviorally equivalent to `envoyproxy/envoy:v1.33.0` under the differential
contract (§7):

- A `grpc_health_check` cluster probes each endpoint with a unary
  `grpc.health.v1.Health/Check` RPC (over the cluster's upstream H2); a
  `SERVING` response ⇒ the endpoint is Healthy and LB-eligible; a non-`SERVING`
  status, a non-zero `grpc-status`, or a transport/connect failure ⇒ after
  `unhealthy_threshold` consecutive failures the endpoint is ejected.
- The gRPC checker REQUIRES the cluster to be H2-upstream (R-0.3); a
  `grpc_health_check` on a non-H2 cluster is a fail-loud `ConfigError`
  (parity with upstream's "must support HTTP/2 for gRPC healthchecking").
- More than one of `http_health_check` / `tcp_health_check` / `grpc_health_check`
  present ⇒ the oneof rejection (R-0.5).
- The SAME `cluster.<n>.health_check.*` + `membership_*` stat tree, now driven
  by the gRPC checker (NO new stat names).
- Reuse phase 12/68's scheduler, `EndpointHealth` state machine, ejection, and
  `pick()` exclusion unchanged.

**Differential surface at phase end:** a new fixture `0075` witnessing gRPC-HC
ejection byte-exact via the downstream-503 observable (an H2 listener, since
the H2-upstream cluster forbids an H1 listener — R-0.3/§5), plus in-process
coverage of the SERVING-healthy / NOT_SERVING-failure / connect-refuse paths
and the gRPC framing + trailers decode.

---

## §2. Scope

### 2.1 In scope

1. **Config schema (`crates/envoy-config`).** Add
   `HealthCheck.grpc_health_check: Option<GrpcHealthCheck>` (serde `default`,
   the third checker `Option` alongside `http_health_check`/`tcp_health_check`).
   New sub-type `GrpcHealthCheck { service_name: String (default ""),
   authority: String (default ""), initial_metadata: Vec<HeaderValueOption>
   (default) }`, `#[serde(deny_unknown_fields, default)]`. The `initial_metadata`
   shape reuses the existing `HeaderValueOption` type if present; otherwise its
   support is scoped MINIMAL (see 2.2 — full `initial_metadata` semantics are
   deferred; the fixture/tests do not require it).
2. **Validation (`crates/envoy-config`).** (a) Generalize the phase-68
   both-checkers oneof rejection (`validate_health_checks`, `bootstrap.rs:4770`)
   to "at most one of {http,tcp,grpc}" (an `is_some()` count > 1 → a fail-loud
   `ConfigError`; PV-4 decides whether to reuse/rename
   `BothHttpAndTcpHealthCheck` or add a general variant). (b) NEITHER present →
   the existing `UnsupportedHealthCheckType` path (message updated: gRPC now
   supported; custom still deferred). (c) **A `grpc_health_check` on a non-H2
   cluster → a new fail-loud `ConfigError`** mirroring upstream "cluster must
   support HTTP/2 for gRPC healthchecking" (the H2-requirement, R-0.3; PV-1 —
   the exact "is this cluster H2-upstream" predicate + the divergence-free
   error). (d) The shared timing/threshold validators apply unchanged. (e) The
   `bootstrap.rs:15188` pinning test is updated (it currently feeds
   `grpc_health_check` on a non-H2 cluster and asserts error — it STILL errors
   post-landing, now via the H2-requirement path, so it is re-pointed to the
   remaining deferred checker OR converted to assert the H2-requirement error;
   PV-4).
3. **gRPC-unary-over-H2 client primitive (`crates/envoy-http2`).** A
   trailers-aware unary call: send `POST /grpc.health.v1.Health/Check` with the
   gRPC-framed request body + gRPC headers, read the response body frame AND the
   `grpc-status` trailer (the missing piece, R-0.7). Placed in `envoy-http2`
   (the "sole user of `h2::client`" locality rule, `client.rs:2`). Reuses the
   existing `Client::connect`/handshake/pool scaffolding.
4. **gRPC health protobuf messages.** Hand-encode `HealthCheckRequest{ service }`
   and hand-decode `HealthCheckResponse{ status }` + the `ServingStatus` enum
   (~60 LoC, no `prost`; PV-3 confirms the hand-roll vs. introducing `prost` —
   default: hand-roll, consistent with the codebase).
5. **gRPC probe task (`crates/envoy-health`).** A `grpc_probe_once` +
   `grpc_probe_loop` (near-verbatim copy of `tcp_probe_loop`): open the H2
   connection to the endpoint (bounded by `timeout`), issue the unary Check,
   verdict = (`grpc-status == 0` AND response status `SERVING`) ⇒ `Ok(())`, else
   `Err`; feed the boolean into `EndpointHealth` exactly as HTTP/TCP do. A
   `GrpcProbeError` diagnostic enum mirroring `TcpProbeError`.
6. **Dispatch wiring (`crates/envoy-health` scheduler).** Widen the
   `match (&http_cfg, &tcp_cfg)` at `scheduler.rs:115` to a 3-tuple over
   `grpc_cfg`, adding the gRPC spawn arm. No change to the scheduler lifecycle,
   ejection sweeper, or `pick()`.
7. **Differential fixture `0075-upstream-grpc-health-check`.** An **H2** HCM
   listener (`codec_type: HTTP2`) → an H2-upstream cluster whose sole endpoint
   carries a `grpc_health_check` that FAILS (the cheapest deterministic failure:
   a connect-refused dead port, mirroring `0074` — no gRPC backend needed) →
   `healthy_panic_threshold: { value: 0 }` → after settle Unhealthy → synth-503
   `"no healthy upstream"` byte-exact, via a new `http2_after_settle` driver
   (PV-2 — only `http1_after_settle` exists today; the H2 driver mirrors it).
8. **In-process coverage (`envoy-health`/`envoy-http2`/`envoy-config`).** The
   SERVING → healthy path and the NOT_SERVING → failure path (against an
   in-process H2 gRPC-health mock built on the existing `envoy-http2` server
   codec — this exercises the gRPC framing + trailers decode differentially-blind
   but end-to-end), the connect-refuse → failure path, the gRPC message
   encode/decode, the both-/neither-checker rejections, and the H2-requirement
   rejection.
9. **`BEHAVIOR_CONTRACT.md`** — a `grpc_health_check` subsection (§6).
10. **`known-failures.txt` / conformance** — unchanged (no protocol conformance
    surface; never trimmed, memory `h2spec-3-5-2-preface-host-sensitive`).

### 2.2 Out of scope (deliberate, with rationale)

- **`custom_health_check`** — stays fail-loud `UnsupportedHealthCheckType`.
- **Full `initial_metadata` semantics** — the schema accepts it (deny_unknown
  would otherwise reject a valid config), but the probe attaching arbitrary
  request metadata beyond the mandatory gRPC headers is NOT differentially
  exercised (the fixture uses `{}`); in-process at most. PV-3 confirms whether
  `initial_metadata` must be threaded to pass the both-proxies wire compare (it
  is not, for the connect-refuse fixture — the connection never opens).
- **A gRPC backend differential helper** — NOT built. The differential uses the
  connect-refused failure (no backend), the phase-68 precedent (ADR-0137: the
  `0074` fixture used a refused port, with the payload-scan covered IN-PROCESS).
  The SERVING/NOT_SERVING gRPC-response paths are in-process (an `envoy-http2`
  server-codec mock), NOT differential — a deliberate, documented boundary
  (§6.3-clean: the connect-refuse fixture fully witnesses ejection; the
  response-status logic is witnessed in-process, not stubbed).
- **`reuse_connection`, `initial_jitter`/`interval_jitter`,
  `no_traffic_interval`, `unhealthy_interval`, `*_edge_interval`,
  `always_log_health_check_failures`, `event_log_path`, `tls_options`,
  `transport_socket_match_criteria`** — deferred; absent from fixtures.
- **Passive health checking / outlier interaction** — phase 14 owns outlier
  detection; untouched.
- **H1-listener × H2-cluster dispatch** — remains deferred (ADR-0028); this
  phase does NOT lift it (the fixture uses an H2 listener, R-0.3/§5).

### 2.3 §7.4 fuzz disposition

The `grpc_health_check` config surface reuses the `parse_bootstrap` parser (a
new sub-message). **Default projection:** a new `parse_bootstrap` corpus seed
carrying a `grpc_health_check` (H2 cluster), NO new fuzz target — the phase-68
precedent (ADR-0137). The gRPC MESSAGE decoder (`HealthCheckResponse` from
bytes) is a NEW byte-parser; **confirm at the state-2 PLAN-write** whether it
warrants a dedicated fuzz target (a `decode_health_check_response` target) or
whether in-process malformed-frame tests suffice. If a dedicated target is
added it MUST be wired into `ci.yml` by hand (memory
`new-fuzz-target-needs-a-ci-yml-step`) and its corpus seed un-ignored (memory
`fuzz-corpus-seed-gitignored-by-default`).

---

## §3. PLAN-VERIFY items (re-confirm against the live tree at the state-2 PLAN-write)

- **PV-1 — the H2-requirement predicate + error.** Confirm the exact
  envoy-rust representation of "this cluster is H2-upstream" (the parsed
  `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options`
  presence, `bootstrap.rs:218`-`225`) and author the new fail-loud `ConfigError`
  for `grpc_health_check` on a non-H2 cluster. Decide byte-parity vs. a native
  message (ADR-0049 permits native; parity nice-to-have). Confirm WHERE in
  `validate_health_checks` (or the cluster validator) the cluster's H2-ness is
  reachable.
- **PV-2 — the H2 differential driver + fixture.** Only `http1_after_settle`
  exists (`lib.rs:4917`). Author `http2_after_settle` (mirror it, driving an H2
  request against both proxies after `settle_ms`). Confirm the H2 synth-503
  "no healthy upstream" body is byte-identical across proxies (the phase-56..65
  H2 fixtures already exercise H2 503s). Measure the settle budget for
  connect-refuse ejection at `unhealthy_threshold: 2, interval: 1s` (≈`0074`'s
  3500 ms).
- **PV-3 — the gRPC wire primitive + message encoding.** Re-confirm against the
  live `envoy-http2` client: the exact request pseudo-headers/headers
  (`:method POST`, `:path /grpc.health.v1.Health/Check`, `content-type
  application/grpc`, `te trailers`), the 5-byte framing (1 compression byte + 4
  big-endian length), the trailers-read API on `h2` (`RecvStream::trailers()`),
  and the two message wire layouts. Decide hand-roll vs. `prost` (default:
  hand-roll — no proto toolchain in-tree). Confirm `initial_metadata` need
  (it is NOT needed for the connect-refuse fixture).
- **PV-4 — the oneof/neither validator restructure + pinning test.** Confirm the
  `bootstrap.rs:4770`-`4780` structure and how the "at most one of three" +
  "H2-required" arms slot in without disturbing the phase-68 arms. Decide the
  `bootstrap.rs:15188` pinning-test disposition (re-point to `custom_health_check`
  as the still-deferred checker, or convert to assert the H2-requirement error).
- **PV-5 — §6.1 size re-derivation.** Re-estimate net LoC / task count against
  the live tree (§8). The new gRPC wire primitive + trailers path + the H2
  differential driver + the in-process H2 gRPC mock push this ABOVE phase 68's
  ~1050. If > ~1500 LoC or > ~25 tasks, **split** (see §8 / ADR-0140-reserved):
  natural seam = 69.1 (config schema + H2-requirement + the gRPC-unary
  primitive + message codec + in-process probe) / 69.2 (scheduler dispatch +
  the `http2_after_settle` driver + fixture `0075` + BEHAVIOR_CONTRACT).
- **PV-6 — `timeout` bounding.** Confirm the gRPC probe's `timeout` bounds the
  WHOLE probe (H2 connect + handshake + request + response + trailers), matching
  the phase-68 TCP resolution (ADR-0137: `timeout` bounds the whole probe;
  cluster `connect_timeout` is NOT consulted by the checker). Re-measure whether
  a `grpc-timeout` request header is emitted (assert only what is measured).

---

## §4. Rejected / deferred alternatives (what this pick was chosen over)

- **Network-filters family remainder (`redis`/`thrift`/`mongo`/`kafka_broker`/
  `zookeeper`).** Each forces a payload-parsing `on_data` protocol subsystem
  (CF-67-3) — a large new codec per filter. Far above the
  cheapest-strong-differential bar.
- **`sni_cluster` network filter.** Needs a `tls_inspector` LISTENER-filter
  subsystem envoy-rust lacks (a whole new filter category). Deferred.
- **Load-balancing `least_request`/`random`.** Non-deterministic (P2C / random
  selection) — need a contract-relaxation ADR before a differential is possible.
- **CF-67-7 (the TLS `[rbac, tcp_proxy]` establishment ordering).** A deliberate
  fail-loud divergence owned by a future TLS-establishment phase; touches the
  sensitive TLS handler.
- **M68-1 alone (empty-hex `text:""` validator fix).** A ~2-line TCP-HC-surface
  polish with a degenerate config-acceptance-only observable — too thin for a
  standalone phase. Not consumed here (this phase does not touch the TCP payload
  validator); remains a live carry-forward (§10).

gRPC health checking wins: it is the natural THIRD checker after HTTP/TCP,
reuses the ENTIRE phase-12/68 health machinery + the upstream-H2 client + the
`0074` ejection observable (cheapest), is fully deterministic on the
downstream-503 observable via the connect-refuse failure (strong), and
introduces **no new subsystem** — it composes the existing health scheduler
with the existing H2 client (adding only a trailers-aware unary call). It
completes the HTTP/TCP/gRPC active-health-check trio named in the
upstream-robustness family (`BOOTSTRAP_PROMPT.md` §9).

---

## §5. Differential surface at phase end

- **NEW fixture `0075-upstream-grpc-health-check`** — green cross-proxy: an
  **H2** HCM listener (`codec_type: HTTP2`) → an H2-upstream cluster
  (`typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options`)
  whose sole endpoint carries a `grpc_health_check: {}` pointing at a
  refused/unbound port → `healthy_panic_threshold: { value: 0 }` → after
  `settle_ms` the endpoint is Unhealthy (connect-refuse → `failure` +
  `network_failure`, ejected after `unhealthy_threshold: 2`) → `pick() → None` →
  synth-503 `"no healthy upstream"` byte-exact (19 bytes, ADR-0037), via the new
  `http2_after_settle` driver + `set_equal_modulo_allow_list` headers (the
  `0074` discipline, adapted to H2).
- **All pre-existing fixtures `0001`–`0074` stay green** — the gRPC checker is
  inert unless a cluster configures `grpc_health_check`; no existing fixture
  does (§7.5 (b)).
- **In-process:** the SERVING-healthy + NOT_SERVING-failure paths (against an
  in-process `envoy-http2` gRPC-health mock — exercises framing + trailers +
  message decode end-to-end), the connect-refuse-failure path, the message
  encode/decode, and the fail-loud both-/neither-checker + H2-requirement
  rejections.

**Why the differential is connect-refuse (not a live gRPC backend):** the
strong, deterministic ejection observable needs no backend (mirrors `0074`,
ADR-0137). The SERVING/NOT_SERVING gRPC-response logic is witnessed in-process,
NOT stubbed — the connect-refuse fixture fully witnesses the ejection→503 path,
and the response-status verdict is exhaustively unit-tested (§6.3-clean).

---

## §6. `BEHAVIOR_CONTRACT.md` additions

A `grpc_health_check` subsection recording the MEASURED facts (R-0.3–R-0.7):
gRPC HC REQUIRES an H2-upstream cluster (else load-fatal "must support HTTP/2");
`{}` ⇒ overall server (`""`), else `service_name`; the checker is a unary
`grpc.health.v1.Health/Check`; `SERVING` ⇒ healthy, any other status /
`grpc-status != 0` ⇒ failure; NOT_SERVING ⇒ `failure` WITHOUT `network_failure`
(app-level), connect/transport failure ⇒ `failure` + `network_failure`; the
`http`/`tcp`/`grpc` oneof rejection; the shared `cluster.<n>.health_check.*` +
`membership_*` stat tree (unchanged names); the `healthy` / `/failed_active_hc`
`health_flags`.

---

## §7. ADR reservations

- **ADR-0138 (FIRED this session, reclaimed):** the phase-69 pick + scope +
  rejected alternatives (this SPEC's decisions).
- **ADR-0139 (reserved):** the §6.2 empirical-verification reconciliation at the
  state-2 PLAN-write (PV-1..PV-6 resolutions — H2-requirement predicate/error,
  the gRPC wire primitive + message codec decision, the `http2_after_settle`
  driver, the oneof/pinning-test restructure, `timeout` bounding).
- **ADR-0140 (reserved):** the §6.1 split, if PV-5 fires it (69.1 schema +
  primitive / 69.2 dispatch + fixture).

---

## §8. Estimated size (for the §6.1 split gate at state-2)

| Area | Net LoC (rough) |
|---|---|
| `envoy-config`: `GrpcHealthCheck` schema, 3-way oneof + H2-requirement + neither `ConfigError` arms, pinning-test update | ~150 |
| `envoy-http2`: trailers-aware unary gRPC call + 5-byte framing | ~180 |
| gRPC health message encode/decode (`HealthCheckRequest`/`Response` + `ServingStatus`) | ~60 |
| `envoy-health`: `grpc_probe_once`/`grpc_probe_loop` + `GrpcProbeError` | ~170 |
| scheduler 3-tuple dispatch + `grpc_cfg` extraction | ~50 |
| fixture `0075` (2 YAMLs + expectations + README) + NEW `http2_after_settle` driver | ~230 |
| in-process tests (SERVING/NOT_SERVING/refuse + framing + decode + rejections) incl. an in-process H2 gRPC-health mock | ~330 |
| `BEHAVIOR_CONTRACT.md` + ROADMAP/docs | ~70 |
| **Total** | **~1240 net LoC / ~14–18 tasks** |

Projected **at the upper edge** of the ~1500 LoC / ~25 task gate — heavier than
phase 68 (~1050) because of the NEW gRPC-unary-over-H2 primitive (trailers) +
the H2 differential driver + the in-process H2 gRPC mock. PV-5 re-derives at the
state-2 PLAN-write; **a 2-way split is a real possibility** (ADR-0140 held in
reserve): the clean seam is 69.1 (config schema + H2-requirement + the
gRPC-unary primitive + message codec + in-process probe coverage — no
differential) / 69.2 (scheduler dispatch + `http2_after_settle` driver + fixture
`0075` + BEHAVIOR_CONTRACT).

---

## §10. Carry-forwards NOT consumed by this pick (surviving phase 68's close)

None obligate this phase; each is owned by whatever future phase touches its
surface.

- **M68-1** — empty-hex `text:""` accepted vs Envoy `min_bytes:1` load-fatal
  (owner = the next phase touching the TCP-HC payload validator). **This phase
  does NOT touch the TCP payload validator.**
- **M68-2** — read error mislabeled `TcpProbeError::Send`
  (`crates/envoy-health/src/probe.rs:209`; cosmetic). **This phase adds a
  `GrpcProbeError`; it may opportunistically fix M68-2 if it touches the same
  file, but is not obligated.**
- **M-1** — the `CidrRange` `prefix_match` guard band (owner = next phase
  touching CidrRange). **Not touched here.**
- **CF-67-3** — payload-visible `on_data` network-filter iteration (deferred).
- **CF-67-5** — empty `filters: []` connection behavior.
- **CF-67-6** — bound `close_with_drain`'s drain (`delayed_close_timeout`).
- **CF-67-7** — the TLS `[rbac, tcp_proxy]` establishment ordering (owner = a
  future TLS-establishment phase).
- The older still-live Minors in `67.3/SPEC.md` §10 and the HTTP-filters-family
  carry-forwards (1)–(4) in `STATE.md` `## Notes`.
