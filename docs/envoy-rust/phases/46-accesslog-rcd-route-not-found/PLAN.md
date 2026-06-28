# Phase 46 — `46-accesslog-rcd-route-not-found` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans`. Steps use `- [ ]`. TDD per `superpowers:test-driven-development`.

**Goal:** Differentially witness the failure-path `%RESPONSE_CODE_DETAILS%` string — `route_not_found` — BYTE-EXACT on the route-miss 404 path, by SETTING `Some("route_not_found")` at envoy-rust's H1 no-matching-route `synth_404` arm (`hcm.rs:1553`; the detail is `None` today) and witnessing it via a NEW access-log fixture `0054`.

**Architecture:** A one-line code change + a new fixture. envoy-rust's H1 route-walk no-matching-route arm already returns a byte-matching 404 via `synth_404`, and the writer-arm threads the `BuildOutcome::Synth(resp, details)` detail to `response_code_details_for_log` (the phase-42 widening), with the access-log record built unconditionally below the match — so the 404 line renders `rcd:null` today only because the detail is left `None`. The fixture drives a `direct_response` listener (no upstream, the `0050` template) whose single route matches only `/specific`; a `/nomatch` probe misses → 404 `route_not_found`, asserted cross-proxy-equal by the existing `http1_access_log_byte_exact` driver.

**Tech Stack:** the H1 HCM route-walk (`crates/envoy-http1/src/hcm.rs`); the phase-42 `BuildOutcome::Synth(resp, Option<&'static str>)` detail-threading; the `Http1AccessLogByteExact` differential driver.

**§6.2 RECON — DONE (state-1 + state-2):**
- **The detail string** is RESOLVED at state-1: live `envoyproxy/envoy:v1.33.0` emits `{"rc":404,"rcd":"route_not_found","rf":"NR"}` on a vhost-matched/route-missed request — a clean brace-free deterministic constant.
- **The set-site** = `crates/envoy-http1/src/hcm.rs:1553` (the "no matching route" arm, preceded by the `tracing::warn!(… "request rejected: no matching route")` at `:1551`). The "no matching virtual_host" arm at `:1535` (preceded by `… "no matching virtual_host"` at `:1533`) is **NOT touched** (host-miss = M46-1, deferred — the fixture's `domains:["*"]` never exercises it).
- **Fuzz coverage** already exists (`crates/envoy-config/fuzz/corpus/parse_bootstrap/response_code_details.yaml`, phase 42) → **SKIP** a new seed.
- **Additive / byte-preserving:** fixtures `0050`-`0053` log `%RESPONSE_CODE_DETAILS%` but none probe a 404 route-miss (`0050` direct_response-200, `0051` cluster, `0052` upstream_host, `0053` no-healthy-503), so setting the previously-`None` route-miss detail changes ZERO existing-fixture bytes.

---

## File Structure
- **Modify** `crates/envoy-http1/src/hcm.rs` — the no-matching-route arm (`:1553`) `None` → `Some("route_not_found")`; + a `#[test]` file-capture backstop.
- **Create** `tests/fixtures/0054-accesslog-rcd-route-not-found/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}` — the route-miss 404 fixture logging `%RESPONSE_CODE_DETAILS%`.
- **Create** `tests/differential/tests/access_log_rcd_route_not_found.rs` — the Docker-gated differential test (clone `access_log_rcd_no_healthy.rs`).
- **Modify** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — the `%RESPONSE_CODE_DETAILS%` row (`~:1031`): the route-miss 404 failure path now differentially witnessed → `route_not_found`.

> Before starting: read `crates/envoy-http1/src/hcm.rs:1505-1560` (the route-walk + the two `synth_404` arms — confirm `:1553` is the "no matching route" one), `:864-866` (the writer-arm `BuildOutcome::Synth(resp, details)` set-site), `:1247` (the unconditional record build), `:1815` (`synth_404`); the phase-45 backstop `h1_no_healthy_access_log_carries_no_healthy_upstream_rcd` (`hcm.rs:~5255`, the file-capture style to clone); `tests/fixtures/0050-accesslog-response-code-details/` (the `direct_response` access-log template) + `tests/differential/tests/access_log_rcd_no_healthy.rs` (the differential-test template).

---

### Task 1: SET the H1 route-miss `%RESPONSE_CODE_DETAILS%` detail + in-process backstop
**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (the `:1553` arm + a `#[test]`)

- [ ] **Step 1 — write the failing backstop test.** In the `hcm.rs` test module, add a `#[tokio::test]` that drives the FULL H1 dispatch path (where the `AccessLogRecord` is built) with a config whose vhost (`domains:["*"]`) has a single route matching only `/specific`, then probes a NON-matching path (`/nomatch`) so the route-walk hits the no-matching-route `synth_404` arm, captures the emitted FILE access-log line, and asserts it carries `rcd:"route_not_found"` AND the response is status 404 (UNCHANGED). **Template:** clone the file-access-log-capture harness `h1_no_healthy_access_log_carries_no_healthy_upstream_rcd` (`hcm.rs:~5255`) — same `log_path` file-capture style — but swap the NO_FALLBACK-subset cluster config for a plain `direct_response` route table (a single `/specific` route) and probe `/nomatch`. The access-log `json_format` must include `%RESPONSE_CODE_DETAILS%`.
- [ ] **Step 2 — run it, confirm it FAILS** (`cargo test -p envoy-http1 <test_name> -- --nocapture`) — expected: the detail is `None` today → the logged line shows `rcd:null` (or `-`), assertion fails. Capture the output.
- [ ] **Step 3 — implement the one-line change.** At `crates/envoy-http1/src/hcm.rs:1553` (the arm AFTER `tracing::warn!(… "request rejected: no matching route")`), change `BuildOutcome::Synth(synth_404(close), None)` → `BuildOutcome::Synth(synth_404(close), Some("route_not_found"))`. **DO NOT touch the `:1535` arm** (the one after `"no matching virtual_host"`). NO other change — the 404 status/body/headers/flags + `synth_404` are untouched.
- [ ] **Step 4 — run the backstop + the full `envoy-http1` suite** (`cargo test -p envoy-http1`) — expected: GREEN; no regression (the detail is additive; existing 404/route-walk/access-log tests unaffected).
- [ ] **Step 5 — commit.** `feat(http1): set %RESPONSE_CODE_DETAILS%=route_not_found on the no-matching-route synth-404 path [phase46 T1]`

### Task 2: fixture `0054` + the differential test
**Files:**
- Create: `tests/fixtures/0054-accesslog-rcd-route-not-found/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/access_log_rcd_route_not_found.rs`

- [ ] **Step 1 — author the fixture** (clone `0050`'s `direct_response` listener shape):
  - **`envoy.yaml`** — HCM with `codec_type: HTTP1` + `generate_request_id: false`; a file access-logger `json_format: { rc: "%RESPONSE_CODE%", rcd: "%RESPONSE_CODE_DETAILS%", method: "%REQ(:METHOD)%", proto: "%PROTOCOL%" }` (log `path` → `/tmp/0054-envoy-mount/access.log`); a vhost `domains: ["*"]` with a SINGLE route `match: { prefix: "/specific" }` → `direct_response: { status: 200, body: { inline_string: "ok\n" } }`; `clusters: []`; `node.id`/`cluster` → phase-46.
  - **`envoy-rust.yaml`** — IDENTICAL except the standard per-side deltas (match `0050`'s envoy-rust.yaml: the `admin` line, the listener bind `0.0.0.0`→`127.0.0.1`, the log `path` → `/tmp/0054-envoy-rust-mount/access.log`, drop `generate_request_id` if `0050` does). The route table is byte-identical.
  - **`expectations.yaml`** — clone `0050`/`0053`: `driver.kind: http1_access_log_byte_exact`, the `/tmp/0054-*` log paths, ONE probe `{ method: get, path: /nomatch, host: envoy-rust.test, expected_status: 404 }`. Document the asserted line `{"method":"GET","proto":"HTTP/1.1","rc":404,"rcd":"route_not_found"}` (keys sort by UTF-8 byte order: method, proto, rc, rcd; `rc` is the json NUMBER 404 — fixture-0053 precedent), cross-proxy EQUALITY (the route-miss synth-404 is deterministic on both sides). **NOTE (plan-review Minor):** the `json_format` AUTHORING key-order in the YAML is IRRELEVANT — the renderer re-sorts keys by UTF-8 byte order at emit time (ADR-0094 §A), so authoring `{ rc, rcd, method, proto }` and asserting `{method,proto,rc,rcd}` is correct, not a mismatch; do NOT "fix" the apparent ordering. NOTE the probe path `/nomatch` does NOT match the `/specific` route → the route-walk's no-matching-route arm → 404 `route_not_found`.
  - **`README.md`** — adapt `0053`/`0050` to phase 46 / ADR-0103: the SECOND failure-path `%RESPONSE_CODE_DETAILS%` witness (after `no_healthy_upstream`); the route-miss trigger (a single `/specific` route + a `/nomatch` probe); envoy-rust now sets `route_not_found` at the H1 no-matching-route `synth_404` arm; the host-miss 404 detail deferred (M46-1).
- [ ] **Step 2 — author the differential test.** Clone `tests/differential/tests/access_log_rcd_no_healthy.rs`; swap the doc-comment → `%RESPONSE_CODE_DETAILS%`/`route_not_found`/phase 46/ADR-0103, the fixture dir → `0054-accesslog-rcd-route-not-found`, the test fn → `access_log_rcd_route_not_found`. Keep the Docker-gate cfg identical.
- [ ] **Step 3 — rebuild + compile:** `cargo build -p envoy-bin` (the differential runs `target/debug/envoy-bin`) + `cargo build -p differential --tests`. Both must succeed.
- [ ] **Step 4 — run IN ISOLATION (Docker-gated):** `cargo test -p differential --test access_log_rcd_route_not_found -- --nocapture`. Expected: GREEN (both proxies emit the byte-identical 404 line incl. `rcd:"route_not_found"`). If it REDs with an `rcd` MISMATCH (envoy-rust `null` or a different string), STOP + report (a real finding contradicting the recon). Host networking/flake REDs (non-byte-mismatch) are CI-authoritative — note + proceed.
- [ ] **Step 5 — commit.** `test(differential): fixture 0054 %RESPONSE_CODE_DETAILS%=route_not_found byte-exact (route-miss 404) [phase46 T2]`

### Task 3: BEHAVIOR_CONTRACT + (fuzz SKIP) + local gate
**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `%RESPONSE_CODE_DETAILS%` row `~:1031`)

- [ ] **Step 1 — BEHAVIOR_CONTRACT.** Update the `%RESPONSE_CODE_DETAILS%` row: the route-miss 404 failure path is now DIFFERENTIALLY WITNESSED byte-exact by fixture `0054` → `route_not_found` (set at the H1 no-matching-route `synth_404` arm; the host-miss 404 detail deferred M46-1; connect/overflow M45-2; H2 M45-1). Keep the existing `direct_response`/`via_upstream`/`no_healthy_upstream` notes.
- [ ] **Step 2 — fuzz seed (PLAN-VERIFY → SKIP).** Confirm `crates/envoy-config/fuzz/corpus/parse_bootstrap/response_code_details.yaml` already covers `%RESPONSE_CODE_DETAILS%` (it does, phase 42) → SKIP a new seed (note in the commit). NO new fuzz target.
- [ ] **Step 3 — local gate.** `cargo build --workspace` + `cargo clippy --workspace --all-targets` + `cargo fmt --check` + `cargo test --workspace` (modulo the documented `…h2_handshake…` host-flake + the differential parallel-load/bridge-IP flakes + the `eds_cluster_with_neither_is_fatal` port-reuse flake — CI authoritative) + `cargo deny check`.
- [ ] **Step 4 — commit.** `docs(behavior-contract): %RESPONSE_CODE_DETAILS% route-miss 404 path witnessed by 0054 (fuzz seed already present → skip) [phase46 T3]`

---

## Acceptance (§7.5, re-run at state-4)
(a) fixture `0054` green (cross-proxy-equal `rcd:"route_not_found"` on the 404) + (b) all `0001`-`0053` green simultaneously (additive — byte-identical) + (c) h2spec ≥95% + (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `Op`/`AccessLogRecord` field/`ConfigError` variant; the ONLY `src/` change is the `:1553` `Some("route_not_found")` detail set.

## Notes for the executor
- **Edit ONLY `hcm.rs:1553`** (the "no matching route" arm). The `:1535` arm ("no matching virtual_host") stays `None` — host-miss is M46-1, deferred; the fixture never hits it (`domains:["*"]`).
- **The probe path `/nomatch` MUST NOT match the route** `prefix: "/specific"` → the route-walk yields no match → `synth_404` with the new detail.
- The differential runs the DEBUG `target/debug/envoy-bin` (rebuild before the fixture); run `0054` in isolation (parallel-load flake).
- Byte-preservation: additive (a previously-`None` field on the route-miss arm; no existing route-miss-404 access-log fixture) → all `0001`-`0053` stay byte-identical.
- **H2 + host-miss are OUT of scope** (M45-1 / M46-1).

---

_Scope locked by **ADR-0103**. The §6.2 recon (state-1) CONFIRMED the `route_not_found` string + the `:1553` set-site + the fuzz SKIP — REFINING but NOT overturning §A-§D → no §6.2-reconciliation ADR. The §6.1 split does NOT fire (~3 tasks; **ADR-0104 reserved**). The state-3 implementation is the next session._
