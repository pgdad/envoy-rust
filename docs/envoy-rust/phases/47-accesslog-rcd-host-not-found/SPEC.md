# Phase 47 — `47-accesslog-rcd-host-not-found` — SPEC

**Pick (ADR-0104):** differentially WITNESS the failure-path `%RESPONSE_CODE_DETAILS%` string **`route_not_found`** BYTE-EXACT on the **host-miss** (no-matching-virtual_host) 404 path, and SET it at envoy-rust's H1 no-matching-virtual_host `synth_404` arm (`hcm.rs:1535`; the detail is `None` today) — **CONSUMING carry-forward M46-1** (the host-miss 404 detail that phase 46 deliberately deferred). A direct phase-46 sibling: phase 46 witnessed the ROUTE-miss 404 (`route_not_found`) and scoped out the host-miss arm pending a clean recon — this phase closes it.

## §1 — Why this pick

Phase 46 (ADR-0103) set `Some("route_not_found")` at the no-matching-ROUTE `synth_404` arm (`hcm.rs:1553`) and witnessed it via fixture `0054`, but DEFERRED the no-matching-VIRTUAL_HOST arm (`hcm.rs:1535`, M46-1) because its v1.33.0 detail string was not cleanly captured at phase-46 state-1, and the `0054` `domains:["*"]` wildcard vhost never exercised it. The phase-47 state-1 recon (live `envoyproxy/envoy:v1.33.0`) CLOSES that gap: a request whose `:authority` matches NO `domains` entry → `{RCD=route_not_found, RC=404, RF=NR}` — the SAME clean, brace-free, deterministic constant as the route-miss case. So envoy-rust's host-miss arm (which emits `None` → renders `null` today) DIVERGES from Envoy's `route_not_found`; this phase closes the divergence + the owed M46-1.

This is the cheapest available pick that closes an owed carry-forward, recon-confirmed.

## §2 — Scope (minimum-viable, ADR-0104)

**A one-line code change + a NEW fixture (the phase-46 pattern):**

- **§A — SET the `route_not_found` detail at the no-matching-VIRTUAL_HOST `synth_404` arm (`hcm.rs:1535`).** Change `BuildOutcome::Synth(synth_404(close), None)` → `BuildOutcome::Synth(synth_404(close), Some("route_not_found"))` (the arm after `tracing::warn!(… "request rejected: no matching virtual_host")`). The writer-arm (`hcm.rs:864-866`) already threads the `BuildOutcome::Synth` detail → `response_code_details_for_log`, and the record is built unconditionally below the match (`hcm.rs:1247`) → the host-miss 404 access-log line carries the detail. NO change to the 404 status/body/headers/flags. **This CONSUMES M46-1.** (Now BOTH `synth_404` route-walk arms — `:1535` host-miss + `:1553` route-miss — carry `route_not_found`, matching Envoy.)
- **§B — the fixture `0055-accesslog-rcd-host-not-found`** (paired `envoy.yaml`/`envoy-rust.yaml`/`expectations.yaml`/`README.md`): a `direct_response` listener (the `0054` template, `clusters:[]`, no upstream) with a vhost `domains: ["match.test"]` (a NON-wildcard domain — NOT `["*"]`) + a single catch-all route, and an access-log `json_format` logging `%RESPONSE_CODE_DETAILS%` (key `rcd`) + deterministic anchors (`%RESPONSE_CODE%`→`404`, `%REQ(:METHOD)%`, `%PROTOCOL%`); the probe sends a Host that matches NO vhost domain (e.g. `host: nomatch.test`) → host-miss 404 `route_not_found`. The existing `http1_access_log_byte_exact` driver (cross-proxy EQUALITY — both emit `rcd:"route_not_found"`); the probe declares `expected_status: 404`.
- **§C — the differential test** `tests/differential/tests/access_log_rcd_host_not_found.rs` (a structural clone of `access_log_rcd_route_not_found.rs`).
- **§D — BEHAVIOR_CONTRACT** `%RESPONSE_CODE_DETAILS%` row update (`:1031`): the host-miss 404 path now ALSO witnessed → `route_not_found` (M46-1 CONSUMED).
- **§E — an in-process backstop** for the H1 `:1535` host-miss detail (the host-miss arm now carries `route_not_found`), mirroring phase 46's file-capture backstop.
- **Fuzz:** `%RESPONSE_CODE_DETAILS%` already has fuzz coverage (`response_code_details.yaml`, phase 42) → **SKIP**.

**NO new `Op` / `AccessLogRecord` field / crate / dependency / fuzz-target / `ConfigError` variant** — only SETS an existing field on the host-miss arm. Load-bearing invariant: additive → all `0001`-`0054` stay byte-identical (no existing fixture probes a HOST-miss with `%RESPONSE_CODE_DETAILS%` — `0054` is a route-miss with `domains:["*"]`; PLAN-VERIFY no other fixture sends a non-matching Host with rcd logged).

## §3 — PLAN-VERIFY items (state-2 §6.2)
1. **(RESOLVED at state-1 / spec-review) The host-miss trigger / harness probe-Host wiring is WIRABLE with NO harness change.** The `Http1AccessLogByteExact` driver (`tests/differential/src/lib.rs:5106-5131`) passes the probe's `host:` verbatim into `drive_http1`, which writes `Host: {host}\r\n` literally (`lib.rs:2015-2020`) — no override/default/rewrite. `vh_matches` (`hcm.rs:1594-1602`, exact case-insensitive + `*` wildcard) returns `false` for a non-matching domain → `vh = None` → the `:1535` arm. So a `domains:["match.test"]` vhost + a `host: nomatch.test` probe drives the host-miss arm on envoy-rust, and Envoy v1.33.0 emits `route_not_found` (state-1 recon). **NOTE:** the probe Host MUST be NON-EMPTY (the H1 codec at `hcm.rs:~1502-1512` rejects a missing/empty Host with `synth_400` BEFORE the vhost-walk; a non-empty non-matching Host like `nomatch.test` sails past that guard to `:1535`).
2. **The exact set-site** — edit ONLY the no-matching-virtual_host `synth_404` return (`hcm.rs:1535`, the arm after `"request rejected: no matching virtual_host"`). Phase 46 already set the route-miss arm `:1553`/`:1554` — DO NOT re-touch it; verify the line numbers before editing.
3. **(RESOLVED at spec-review) the fuzz SKIP + additive byte-preservation** — all five existing rcd-logging fixtures (`0050`-`0054`) use `domains:["*"]` wildcard vhosts with a matching probe Host → NONE triggers a host-miss, so setting the previously-`None` host-miss detail changes ZERO existing-fixture bytes → `0001`-`0054` byte-identical. The `%RESPONSE_CODE_DETAILS%` fuzz seed (`response_code_details.yaml`) already exists → SKIP.
4. **(RESOLVED at state-1)** the host-miss 404 detail is `route_not_found` (brace-free, deterministic; recon-confirmed `{RCD=route_not_found, RC=404, RF=NR}` on `Host: unknown.test`).

## §4 — Rejected / deferred
- **connect-failure / overflow details** (non-deterministic OS brace + the 502-vs-503 status divergence) — deferred (**M45-2**).
- **H2 host-miss / route-miss details** — deferred with the H2 access-log driver (**M45-1**); the H2 route-walk is a separate codec path with no H2 access-log differential.
- `%RESPONSE_FLAGS%` (`NR`) — a separate operator; not logged in `0055` unless §6.2 confirms envoy-rust emits `NR` byte-identically (out of scope; keep the `json_format` to `rcd`/`rc`/`method`/`proto`).

## §5 — Acceptance (§7.5, re-run at state-4)
(a) fixture `0055` green (cross-proxy-equal `rcd:"route_not_found"` on the host-miss 404) + (b) all `0001`-`0054` green simultaneously (additive — byte-identical) + (c) h2spec ≥95% + (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target/`Op`/`AccessLogRecord` field/`ConfigError` variant; the ONLY `src/` change is setting `Some("route_not_found")` at the H1 `:1535` host-miss `synth_404` arm. **M46-1 CONSUMED.**

## §6 — Process
- **§6.1 split:** projected NOT to fire (~3-5 tasks / ~30-100 LoC — one set-site + a fixture + a test + a backstop + docs; comparable to phase 46). **ADR-0105 reserved-but-unfired** for the split.
- **§6.2 reconciliation:** the state-1 recon already CONFIRMED `route_not_found` on the host-miss; ADR-0104 needs NO §6.2-reconciliation ADR unless the state-2 recon overturns a §A-§D fact (e.g. the harness can't drive a non-matching Host).
- Pick + §A-§D ground-truth locked by **ADR-0104** (reclaims the number ADR-0103 reserved for the phase-46 §6.1 split that did NOT fire — the lapsed-reservation convention).

_Scope locked by ADR-0104. The state-2 PLAN-write (`superpowers:writing-plans`) runs the §6.2 recon (PLAN-VERIFY items §3) and authors `PLAN.md`. The state-3 implementation is the session after._
