# Phase 44 — `44-accesslog-upstream-host` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored via `superpowers:brainstorming`; the project is
> autonomous (`feedback_pick_recommendation` — converge + record the pick in an ADR, no human gate). This
> SPEC is the requirements contract; `PLAN.md` (the state-2 step, the NEXT session) turns it into TDD tasks.

## §0 — One-paragraph summary

**Differentially witness the `%UPSTREAM_HOST%` access-log command operator — the resolved upstream endpoint
`<ip>:<port>` — byte-exact on a real upstream, the gap fixture `0051` deliberately excluded.** `%UPSTREAM_HOST%`
has been IMPLEMENTED since phase 06 (`AccessLogRecord.upstream_host: Option<String>` set by the HCM at the
proxy-success arm to `endpoint.to_string()`), but it has NEVER been verified against upstream Envoy — every
access-log fixture `0040`-`0050` is `direct_response` (no upstream, `%UPSTREAM_HOST%` absent), and phase-43's
first proxy fixture `0051` EXCLUDED the `%UPSTREAM_HOST%` token because its `{{BACKEND_HOST}}`/STRICT_DNS
backend resolves to DIFFERENT addresses per-side (host-gateway IP on the Envoy-container side vs `127.0.0.1`
on the envoy-rust-host subject — structurally non-byte-identical). **This phase closes that gap** with a
proxy access-log fixture that routes via a `{{BACKEND_IP}}` STATIC cluster — the harness's shared-host-LAN-IP
mechanism (`discover_host_lan_ip()`, used by the consistent-hash LB fixtures `0036`/`0037`/`0038`) — so BOTH
proxies dial the IDENTICAL `<host-LAN-IP>:<port>` and render the IDENTICAL `%UPSTREAM_HOST%` line, asserted
cross-proxy-equal by the existing `http1_access_log_byte_exact` driver.

**This is the cheapest-strong VALID next leaf:** it reuses the phase-43 proxy-access-log fixture template + the
proven `{{BACKEND_IP}}` shared-IP machinery (ZERO new harness code expected), adds NO new operator / record
field / connection plumbing, and closes a GENUINE correctness gap — envoy-rust's `%UPSTREAM_HOST%` rendering
on a real upstream is currently UNVERIFIED differentially. It may surface a format mismatch (envoy-rust's
`endpoint.to_string()` vs Envoy's `<ip>:<port>`) — a real differential finding, not a trivially-green fixture.

**§6.2 FACTS (recon-LOCKED this state-1, captured live against `envoyproxy/envoy:v1.33.0`):** a request routed
to a STATIC cluster `127.0.0.1:13099` → `%UPSTREAM_HOST%` renders the resolved endpoint `127.0.0.1:13099`
(json single-op → `"127.0.0.1:13099"`). The harness resolves `{{BACKEND_IP}}` to ONE shared host-LAN-IP for
BOTH sides (`run_fixture` `lib.rs:3011-3013`), and `{{HTTP1_BACKEND_PORT}}` to the backend's actual port
(identical both sides) → the rendered `%UPSTREAM_HOST%` = `<host-LAN-IP>:<port>` is byte-identical cross-proxy
(the value is DYNAMIC per CI run but SHARED, and the access-log byte-exact driver asserts cross-proxy equality,
NOT against a static literal).

## §1 — Goal & differential surface
**Goal.** Differentially verify `%UPSTREAM_HOST%` byte-equivalent to upstream Envoy v1.33.0 on a real
upstream-routed request, under the differential contract (§7.2) on the **Access log records** dimension.

**Differential surface at phase end:**
- **Fixture `0052-accesslog-upstream-host`** (next free; baseline `0001`…`0051`): an H1 listener routing via a
  `{{BACKEND_IP}}` STATIC cluster (model the backend on the LB fixtures `0036`/`0037`; the access-log format +
  driver on `0051`); the file logger's `json_format` contains `%UPSTREAM_HOST%` (+ `%UPSTREAM_CLUSTER%` /
  `%RESPONSE_CODE_DETAILS%` as deterministic anchors). Both proxies dial the shared `<host-LAN-IP>:<port>`;
  the emitted line is byte-identical cross-proxy (`http1_access_log_byte_exact`, cross-proxy equality).
- **All `0001`–`0051` stay green simultaneously** — this phase adds NO operator + NO record-field change; it is
  a NEW fixture (+ possibly a small `%UPSTREAM_HOST%` format fix). No existing fixture changes behavior.

**Conformance:** h2spec ≥95% (unchanged). Fuzz: reuse `parse_bootstrap`/`accesslog_format_parse`; add a
`%UPSTREAM_HOST%` seed if not already covered. NO new fuzz target.

## §2 — Scope (minimum-viable)
### §2.1 IN scope
1. **Fixture `0052-accesslog-upstream-host`** (the core deliverable): a proxy access-log fixture using a
   `{{BACKEND_IP}}` STATIC cluster (shared host-LAN-IP, so `%UPSTREAM_HOST%` is byte-identical cross-proxy) +
   the `http1_access_log_byte_exact` driver, logging `%UPSTREAM_HOST%`. Paired `envoy.yaml`/`envoy-rust.yaml`/
   `expectations.yaml`/`README.md`.
2. **PLAN-VERIFY + FIX (if needed): the `%UPSTREAM_HOST%` rendering format.** envoy-rust renders
   `endpoint.to_string()` (`hcm.rs:994`); Envoy renders `<ip>:<port>`. **PLAN-VERIFY** they byte-match; IF they
   differ (e.g. IPv6 bracketing, a trailing component), fix envoy-rust's rendering to match Envoy's
   `%UPSTREAM_HOST%` — a small HCM/record-format change. If they already match, NO src/ change (fixture-only).
3. **The harness `{{BACKEND_IP}}` reuse** — confirm `Driver::Http1WithAccessLog` + `run_fixture` template
   `{{BACKEND_IP}}` (the shared host-LAN-IP) for the access-log driver as they do for the LB drivers (projected
   YES — `run_fixture`'s `{{BACKEND_IP}}` resolution is global, not driver-specific). NO new harness code projected.
4. **Tests / docs.** Fixture `0052` (cross-proxy-equal `%UPSTREAM_HOST%` line) + all `0001`-`0051` unchanged +
   an in-process backstop (if a format fix lands: assert envoy-rust renders the expected `<ip>:<port>`) + the
   BEHAVIOR_CONTRACT `%UPSTREAM_HOST%` row updated ("differentially witnessed by `0052`") + a fuzz seed if
   `%UPSTREAM_HOST%` is not already in the corpus.

### §2.2 DEFERRED non-goals
- **`%UPSTREAM_HOST%` on H2** — `0052` is H1 (the proven `http1_access_log_byte_exact` driver). An H2 proxy
  access-log fixture is a future phase (the H2 plumbing already sets `upstream_host`; only a fixture is owed).
- **M42-1's `%RESPONSE_CODE_DETAILS%` failure-path vocabulary** (connect-error/reset/503 details) — needs a
  failure-injection proxy fixture; its own future phase.
- **`%REQUEST_HEADERS_BYTES%` / `%ACCESS_LOG_TYPE%` / the gRPC-ALS/OTLP/tracing/tap surfaces** — each its own
  future phase.

## §3 — Open PLAN-write design calls (resolved at state-2)
1. **The `%UPSTREAM_HOST%` format match** — boot the `{{BACKEND_IP}}` fixture against envoy-rust + Envoy; if
   the `%UPSTREAM_HOST%` token differs, scope the format fix (else fixture-only).
2. **The `{{BACKEND_IP}}` STATIC-cluster wiring** — model on `0036`/`0037` (the LB fixtures using `{{BACKEND_IP}}`)
   for the cluster + the per-side `envoy.yaml`/`envoy-rust.yaml` deltas; confirm `run_fixture` spawns the
   `Http1EchoBackend` + templates `{{BACKEND_IP}}`/`{{HTTP1_BACKEND_PORT}}` for the access-log driver.
   **CRITICAL (spec-review M-3):** `0052`'s CLUSTER stanza must be taken from `0036`/`0037` (a STATIC cluster
   with `address: {{BACKEND_IP}}`) — do NOT clone `0051`'s cluster (a `STRICT_DNS` cluster with
   `{{BACKEND_HOST}}`, which re-introduces the per-side mismatch). Only the access-log FORMAT + the
   `kind: http1_access_log_byte_exact` driver come from `0051`. The recon port `127.0.0.1:13099` (§0/§6.2)
   is illustrative of the RENDER FORMAT (`<ip>:<port>`) ONLY — the fixture's actual address is the
   discovered shared host-LAN-IP, never loopback.
3. **Whether a `%UPSTREAM_HOST%` fuzz seed already exists** (check the corpus; add only if missing).
4. **The §6.1 split** — see §6.1 (projected NOT to fire — a fixture + at most a small format fix).

## §4 — Reuse map (what exists; do not rebuild)
- **`%UPSTREAM_HOST%` is ALREADY implemented** (phase 06): `Op::UpstreamHost`, `AccessLogRecord.upstream_host:
  Option<String>` set at the HCM proxy-success arm (`hcm.rs:994` `endpoint.to_string()`). This phase does NOT
  add it — it WITNESSES it.
- **The phase-43 proxy access-log fixture template** (`0051-accesslog-upstream-cluster`) + the
  `http1_access_log_byte_exact` driver + the marker-driven `Http1EchoBackend` auto-spawn — clone for `0052`.
- **The `{{BACKEND_IP}}` shared-host-LAN-IP machinery** (`run_fixture` `lib.rs:3011-3013` `discover_host_lan_ip()`;
  the LB fixtures `0036`/`0037`/`0038`) — use a `{{BACKEND_IP}}` STATIC cluster so `%UPSTREAM_HOST%` is shared.
- **The BEHAVIOR_CONTRACT `%UPSTREAM_HOST%` row** — update its differential-witness status; do not rebuild.

## §5 — Behavioral contract notes
- **The new axis (differential coverage, not a new operator):** `%UPSTREAM_HOST%` is witnessed byte-exact on a
  real upstream for the first time. The value is the resolved `<ip>:<port>` — DYNAMIC per run but byte-identical
  cross-proxy via the shared `{{BACKEND_IP}}`.
- **Byte-preservation:** NO operator/record-field change → all `0001`-`0051` stay byte-identical. If a
  `%UPSTREAM_HOST%` format fix lands, it changes ONLY the `%UPSTREAM_HOST%` rendering (witnessed by NO fixture
  before `0052`, so no regression).
- **Config validity:** unchanged (ADR-0049). NO new operator → `parse_format` unchanged.

## §6 — Process
### §6.1 — Split projection
NOT to fire. A NEW fixture (`0052`) + at most a small `%UPSTREAM_HOST%` format fix + a BEHAVIOR_CONTRACT update
+ a possible fuzz seed. **~50–150 LoC / ~3–5 tasks** — well under the §6.1 gate. **ADR-0102 reserved**
(projected NOT to fire).

### §6.2 — Empirical reconnaissance (the EXISTENCE/FEASIBILITY-CHECK ran at THIS state-1; the deep recon is state-2)
The state-1 brainstorm confirmed against live `envoyproxy/envoy:v1.33.0`: a routed request → `%UPSTREAM_HOST%`
= the resolved endpoint `<ip>:<port>` (e.g. `127.0.0.1:13099`); and read the harness — `run_fixture` resolves
`{{BACKEND_IP}}` to one shared host-LAN-IP for both sides, making `%UPSTREAM_HOST%` byte-identical cross-proxy.
The state-2 §6.2 recon pins the format-match (envoy-rust `endpoint.to_string()` vs Envoy `<ip>:<port>`) + the
fixture wiring. **ADR-0101 FIRES at THIS state-1** (the pick + the recon facts).

### §6.3 — Anti-deferral
No vague TODOs. The §2.1 fixture is built + asserted; every deferral is a §2.2 named non-goal.

## §7 — Acceptance (the §7.5 gate, previewed)
(a) fixture `0052` green (cross-proxy-equal `%UPSTREAM_HOST%` line) + (b) all `0001`-`0051` green + (c) h2spec
≥95% + (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean + (e) build/clippy/fmt/test/deny clean + (f)
`REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency; NO new `ConfigError` variant;
NO new `AccessLogRecord` field; NO new `Op` variant (this WITNESSES an existing operator); the only possible
`src/` change is a `%UPSTREAM_HOST%` format fix (PLAN-VERIFY — projected none).

---

_Pick locked by **ADR-0101** (phase-44 state-1 brainstorm): differentially witness `%UPSTREAM_HOST%` byte-exact
via a `{{BACKEND_IP}}` shared-host-LAN-IP proxy access-log fixture — closing the gap `0051` excluded. The §6.1
split is projected NOT to fire (**ADR-0102 reserved**). `PLAN.md` is authored the NEXT session (state-2) against
the ADR-0101-locked facts + the state-2 §6.2 recon (the format-match). §5.1: one state per session — this session
STOPS at the SPEC + ROADMAP row + ADR + STATE advance._
