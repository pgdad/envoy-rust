# Phase 46 — `46-accesslog-rcd-route-not-found` — SPEC

**Pick (ADR-0103):** differentially WITNESS the failure-path `%RESPONSE_CODE_DETAILS%` string **`route_not_found`** BYTE-EXACT on the route-miss 404 path, and SET it at envoy-rust's H1 `synth_404` arm(s) (the detail is `None` today). The SECOND clean failure-path `%RESPONSE_CODE_DETAILS%` value (after phase 45's `no_healthy_upstream`), continuing carry-forward **M42-1**.

## §1 — Why this pick

Phase 45 (ADR-0102) witnessed the FIRST failure-path detail (`no_healthy_upstream`). The next-cheapest-STRONG clean deterministic failure detail is **`route_not_found`** — the state-1 recon (live `envoyproxy/envoy:v1.33.0`) confirmed: a request whose path matches a virtual_host but NO route within it → `{"rc":404,"rcd":"route_not_found","rf":"NR"}` — a CLEAN, BRACE-FREE, DETERMINISTIC constant. envoy-rust's H1 route-walk already returns a byte-matching **404** via `synth_404` (`crates/envoy-http1/src/hcm.rs:1535` "no matching route"; `:1518` "no matching virtual_host") but emits `BuildOutcome::Synth(synth_404(close), None)` → renders `null`/`-` for `%RESPONSE_CODE_DETAILS%` today.

This is the phase-45 pattern with an EVEN SIMPLER trigger — no `lb_subset_config` machinery, just a route table where the probe path matches no route. It avoids the rejected non-deterministic dispositions (connect-failure's `upstream_reset_before_response_started{...}` OS-brace, M45-2; the connect-fail 502-vs-503 status divergence).

## §2 — Scope (minimum-viable, ADR-0103)

**A small code change + a NEW fixture:**

- **§A — SET the `route_not_found` detail at the no-matching-ROUTE `synth_404` arm ONLY (the confirmed, witnessed arm).** envoy-rust has TWO `synth_404` sites in the H1 route-walk (`hcm.rs`): (i) no-matching-**virtual_host** (`:1535`); (ii) no-matching-**route** (`:1553`). **This phase changes ONLY the no-matching-route site (`:1553`):** `BuildOutcome::Synth(synth_404(close), None)` → `BuildOutcome::Synth(synth_404(close), Some("route_not_found"))`. The `BuildOutcome::Synth(resp, details)` already threads the `&'static str` detail to `response_code_details_for_log` at the writer-arm (`hcm.rs:864-866`, the phase-42 widening; exactly like `direct_response`/`via_upstream`), and the record is built unconditionally below the match (`hcm.rs:1247`) → the 404 access-log line carries the detail. NO change to the 404 status / body / headers / flags. **The no-matching-virtual_host arm (`:1535`) is NOT touched this phase** — the fixture's `domains: ["*"]` wildcard vhost ALWAYS matches Host, so only the route-miss arm is exercised + witnessed; setting the host-miss arm would be unwitnessed + unbacked (the host-miss detail at v1.33.0 was NOT cleanly captured at state-1 — it may differ). The host-miss `route_not_found` is **deferred as carry-forward M46-1** (set it + a witnessing fixture in a future phase after a clean recon).
- **§B — the fixture `0054-accesslog-rcd-route-not-found`** (paired `envoy.yaml`/`envoy-rust.yaml`/`expectations.yaml`/`README.md`): a `direct_response` listener (NO upstream — the phase-42 `0050` template) with a vhost `domains: ["*"]` + a SINGLE route matching only `prefix: "/specific"` (→ `direct_response` 200), and an access-log `json_format` logging `%RESPONSE_CODE_DETAILS%` (key `rcd`) + deterministic anchors (`%RESPONSE_CODE%`→`404`, `%REQ(:METHOD)%`, `%PROTOCOL%`); the probe requests a NON-matching path (e.g. `/nomatch`) → 404 `route_not_found`. The existing `http1_access_log_byte_exact` driver (cross-proxy EQUALITY — both emit `rcd:"route_not_found"`); the probe declares `expected_status: 404`.
- **§C — the differential test** `tests/differential/tests/access_log_rcd_route_not_found.rs` (a structural clone of `access_log_rcd_no_healthy.rs`).
- **§D — BEHAVIOR_CONTRACT** `%RESPONSE_CODE_DETAILS%` row update (`:1031`): the route-not-found failure path now witnessed → `route_not_found`.
- **§E — an in-process backstop** for the H1 no-matching-route `synth_404` detail (the route-miss arm now carries `route_not_found`), mirroring phase 45's file-capture backstop (`h1_no_healthy_access_log_carries_no_healthy_upstream_rcd`).
- **Fuzz:** `%RESPONSE_CODE_DETAILS%` already has fuzz coverage (`response_code_details.yaml`, phase 42) → **SKIP** (no new operator/key/target).

**NO new `Op` / `AccessLogRecord` field / crate / dependency / fuzz-target / `ConfigError` variant** — `%RESPONSE_CODE_DETAILS%` + the field exist since phase 42; this phase only SETS an existing field on the route-miss arm. Load-bearing invariant: additive → all `0001`-`0053` stay byte-identical (verified: fixtures `0050`-`0053` log `%RESPONSE_CODE_DETAILS%` but none probe a 404 route-miss — `0050` direct_response-200, `0051` cluster, `0052` upstream_host, `0053` no-healthy-503; the change sets a previously-`None` detail on the route-miss arm, which no existing fixture exercises).

## §3 — PLAN-VERIFY items (state-2 §6.2)
1. **The fixture trigger** — a `domains: ["*"]` vhost + a single `prefix: "/specific"` route (→ `direct_response` 200) + a `/nomatch` probe → 404 route-miss on BOTH proxies (clone the `0050` direct_response listener shape; no upstream/backend). The probe `expected_status: 404`; `%RESPONSE_CODE%` renders the json NUMBER `404` (the fixture-0053 precedent for the number-vs-string shape).
2. **The exact set-site** — edit ONLY the no-matching-route `synth_404` return (`hcm.rs:1553`, "no matching route"), NOT the no-matching-virtual_host return (`:1535`). Confirm the line numbers before editing.
3. **(RESOLVED at state-1 — confirmable now)** no existing fixture logs `%RESPONSE_CODE_DETAILS%` on a 404 route-miss (fixtures `0050`-`0053` log rcd but probe direct_response-200 / cluster / upstream_host / no-healthy-503 — none a route-miss 404), so the change is additive → `0001`-`0053` byte-identical; the `%RESPONSE_CODE_DETAILS%` fuzz seed already exists (`response_code_details.yaml`) → SKIP.
4. **(RESOLVED at state-1)** `route_not_found` is brace-free + deterministic; recon-confirmed `{"rc":404,"rcd":"route_not_found","rf":"NR"}` on the route-miss 404 at v1.33.0.

## §4 — Rejected / deferred
- **connect-failure / overflow details** (non-deterministic brace; 502-vs-503 status divergence) — deferred (**M45-2**).
- **The no-matching-virtual_host (host-miss) 404 detail** — deferred as **M46-1** (its v1.33.0 detail was not cleanly captured at state-1; set it + a witnessing fixture in a future phase after a clean recon; the fixture's `domains:["*"]` never exercises it).
- **H2 route-not-found detail** — deferred with the H2 access-log driver (**M45-1**); the H2 route-walk 404 arm is a separate codec path with no H2 access-log differential.
- `%RESPONSE_FLAGS%` (`NR`) — a separate operator; not logged in `0054` unless §6.2 confirms envoy-rust emits `NR` byte-identically (out of scope; keep the `json_format` to `rcd`/`rc`/`method`/`proto`).

## §5 — Acceptance (§7.5, re-run at state-4)
(a) fixture `0054` green (cross-proxy-equal `rcd:"route_not_found"` on the 404) + (b) all `0001`-`0053` green simultaneously (additive — byte-identical) + (c) h2spec ≥95% + (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target/`Op`/`AccessLogRecord` field/`ConfigError` variant; the ONLY `src/` change is setting `Some("route_not_found")` at the H1 `synth_404` arm(s).

## §6 — Process
- **§6.1 split:** projected NOT to fire (~3-5 tasks / ~40-120 LoC — one/two set-sites + a fixture + a test + a backstop + docs; comparable to phase 45). **ADR-0104 reserved-but-unfired** for the split.
- **§6.2 reconciliation:** the state-1 recon already CONFIRMED `route_not_found` on the route-miss; ADR-0103 needs NO §6.2-reconciliation ADR unless the state-2 recon overturns a §A-§D fact (e.g. the host-miss detail differs).
- Pick + §A-§D ground-truth locked by **ADR-0103** (reclaims the number ADR-0102 reserved for the phase-45 §6.1 split that did NOT fire — the lapsed-reservation convention).

_Scope locked by ADR-0103. The state-2 PLAN-write (`superpowers:writing-plans`) runs the §6.2 recon (PLAN-VERIFY items §3) and authors `PLAN.md`. The state-3 implementation is the session after._
