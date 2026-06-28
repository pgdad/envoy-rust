# Phase 47 — `47-accesslog-rcd-host-not-found` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans`. Steps use `- [ ]`. TDD per `superpowers:test-driven-development`.

**Goal:** Differentially witness the failure-path `%RESPONSE_CODE_DETAILS%` string — `route_not_found` — BYTE-EXACT on the HOST-miss (no-matching-virtual_host) 404 path, by SETTING `Some("route_not_found")` at envoy-rust's H1 no-matching-virtual_host `synth_404` arm (`hcm.rs:1535`; the detail is `None` today) and witnessing it via a NEW access-log fixture `0055`. **CONSUMES carry-forward M46-1.**

**Architecture:** A one-line code change + a new fixture — the exact phase-46 pattern, but on the OTHER `synth_404` arm. envoy-rust's H1 route-walk has TWO `synth_404` arms: the no-matching-route arm (`:1553`, phase 46 already set to `Some("route_not_found")`) and the no-matching-virtual_host (host-miss) arm (`:1535`, still `None` — THIS phase). The writer-arm threads the `BuildOutcome::Synth` detail → `response_code_details_for_log`, and the record is built unconditionally below the match, so the host-miss 404 line renders `rcd:null` today only because `:1535` is left `None`. The fixture drives a `direct_response` listener (no upstream, the `0054` template) whose vhost has a NON-wildcard `domains:["match.test"]`; a probe with `Host: nomatch.test` matches NO vhost → host-miss 404 `route_not_found`, asserted cross-proxy-equal by the existing `http1_access_log_byte_exact` driver.

**Tech Stack:** the H1 HCM route-walk (`crates/envoy-http1/src/hcm.rs`); the phase-42 `BuildOutcome::Synth(resp, Option<&'static str>)` detail-threading; the `Http1AccessLogByteExact` differential driver (which passes the probe `host:` verbatim).

**§6.2 RECON — DONE (state-1 + spec-review):**
- **The detail string** is RESOLVED at state-1: live `envoyproxy/envoy:v1.33.0` emits `{RCD=route_not_found, RC=404, RF=NR}` on a request whose `:authority` matches no `domains` entry — the same clean brace-free constant as the route-miss case.
- **The set-site** = `crates/envoy-http1/src/hcm.rs:1535` (the "no matching virtual_host" arm, after the `tracing::warn!(… "request rejected: no matching virtual_host")` block). The "no matching route" arm at `:1553`/`:1554` is ALREADY `Some("route_not_found")` (phase 46) — **DO NOT re-touch it.**
- **The host-miss trigger is wirable with NO harness change (spec-review-confirmed):** the `Http1AccessLogByteExact` driver (`tests/differential/src/lib.rs:5106-5131`) passes the probe `host:` verbatim → `drive_http1` writes `Host: {host}\r\n` literally (`:2015-2020`); `vh_matches` (`hcm.rs:~1594`, exact case-insensitive + `*`) returns `false` for a non-matching domain → `vh = None` → the `:1535` arm. The probe Host MUST be NON-EMPTY (the codec rejects missing/empty Host with `synth_400` at `~:1502-1512` before the vhost-walk).
- **Fuzz coverage** already exists (`crates/envoy-config/fuzz/corpus/parse_bootstrap/response_code_details.yaml`, phase 42) → **SKIP**.
- **Additive / byte-preserving:** all five existing rcd-logging fixtures (`0050`-`0054`) use `domains:["*"]` wildcard vhosts → none triggers a host-miss, so setting the previously-`None` host-miss detail changes ZERO existing-fixture bytes.

---

## File Structure
- **Modify** `crates/envoy-http1/src/hcm.rs` — the no-matching-virtual_host arm (`:1535`) `None` → `Some("route_not_found")`; + a `#[test]` file-capture backstop.
- **Create** `tests/fixtures/0055-accesslog-rcd-host-not-found/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}` — the host-miss 404 fixture logging `%RESPONSE_CODE_DETAILS%`.
- **Create** `tests/differential/tests/access_log_rcd_host_not_found.rs` — the Docker-gated differential test (clone `access_log_rcd_route_not_found.rs`).
- **Modify** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — the `%RESPONSE_CODE_DETAILS%` row (`~:1031`): the host-miss 404 failure path now ALSO differentially witnessed → `route_not_found`; **M46-1 CONSUMED**.

> Before starting: read `crates/envoy-http1/src/hcm.rs:1505-1560` (the route-walk + the two `synth_404` arms — confirm `:1535` is the "no matching virtual_host" one and `:1553`/`:1554` the already-set "no matching route" one), `:864-866` (the writer-arm), `:1247` (the unconditional record build); the phase-46 backstop `h1_route_miss_access_log_carries_route_not_found_rcd` (`hcm.rs:~5360`, the file-capture style to clone); `tests/fixtures/0054-accesslog-rcd-route-not-found/` (the `direct_response` access-log template — note `domains:["*"]`, `clusters:[]`, the per-side deltas) + `tests/differential/tests/access_log_rcd_route_not_found.rs` (the differential-test template).

---

### Task 1: SET the H1 host-miss `%RESPONSE_CODE_DETAILS%` detail + in-process backstop
**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (the `:1535` arm + a `#[test]`)

- [ ] **Step 1 — write the failing backstop test.** In the `hcm.rs` test module, add a `#[tokio::test]` that drives the FULL H1 dispatch path with a config whose vhost has a NON-wildcard `domains:["match.test"]` and a catch-all route, then sends a request with `Host: nomatch.test` (NON-EMPTY, matches no vhost) so the route-walk hits the no-matching-virtual_host `synth_404` arm, captures the emitted FILE access-log line, and asserts it carries `rcd:"route_not_found"` AND the response is status 404 (UNCHANGED). **Template:** clone the file-access-log-capture harness `h1_route_miss_access_log_carries_route_not_found_rcd` (`hcm.rs:~5360`) — same `log_path` file-capture style — but make the vhost `domains:["match.test"]` (NOT `["*"]`) and drive the request with `Host: nomatch.test` (the phase-46 sibling used `domains:["*"]` + a `/nomatch` path to hit the route-miss arm; this phase uses a non-matching HOST to hit the host-miss arm). The access-log `json_format` must include `%RESPONSE_CODE_DETAILS%`.
- [ ] **Step 2 — run it, confirm it FAILS** (`cargo test -p envoy-http1 <test_name> -- --nocapture`) — expected: the host-miss arm's detail is `None` today → the logged line shows `rcd:null`, assertion fails. Capture the output.
- [ ] **Step 3 — implement the one-line change.** At `crates/envoy-http1/src/hcm.rs:1535` (the arm AFTER `tracing::warn!(… "request rejected: no matching virtual_host")`), change `BuildOutcome::Synth(synth_404(close), None)` → `BuildOutcome::Synth(synth_404(close), Some("route_not_found"))` (optionally with a `// phase 47 (ADR-0104): %RESPONSE_CODE_DETAILS% = route_not_found on the host-miss 404 path` comment). **DO NOT touch the `:1553`/`:1554` route-miss arm** (already `Some("route_not_found")` from phase 46). NO other change — the 404 status/body/headers/flags + `synth_404` are untouched.
- [ ] **Step 4 — run the backstop + the full `envoy-http1` suite** (`cargo test -p envoy-http1`) — expected: GREEN; no regression (the detail is additive; existing route-walk/404/access-log tests unaffected — incl. the phase-46 route-miss backstop, which still passes since it hits the OTHER arm).
- [ ] **Step 5 — commit.** `feat(http1): set %RESPONSE_CODE_DETAILS%=route_not_found on the no-matching-virtual_host synth-404 path [phase47 T1]`

### Task 2: fixture `0055` + the differential test
**Files:**
- Create: `tests/fixtures/0055-accesslog-rcd-host-not-found/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/access_log_rcd_host_not_found.rs`

- [ ] **Step 1 — author the fixture** (clone `0054`'s `direct_response` listener shape):
  - **`envoy.yaml`** — HCM with `codec_type: HTTP1` + `generate_request_id: false`; a file access-logger `json_format: { rc: "%RESPONSE_CODE%", rcd: "%RESPONSE_CODE_DETAILS%", method: "%REQ(:METHOD)%", proto: "%PROTOCOL%" }` (log `path` → `/tmp/0055-envoy-mount/access.log`); a vhost `domains: ["match.test"]` (NON-wildcard — the ONLY change from `0054`'s `["*"]`) with a single catch-all route `match: { prefix: "/" }` → `direct_response: { status: 200, body: { inline_string: "ok\n" } }`; `clusters: []`; `node.id`/`cluster` → phase-47.
  - **`envoy-rust.yaml`** — IDENTICAL except the standard per-side deltas (match `0054`'s: the `admin` line, the listener bind `0.0.0.0`→`127.0.0.1`, the log `path` → `/tmp/0055-envoy-rust-mount/access.log`, drop `generate_request_id`). The vhost + route + json_format must be BYTE-IDENTICAL.
  - **`expectations.yaml`** — clone `0054`'s: `driver.kind: http1_access_log_byte_exact`, the `/tmp/0055-*` log paths, ONE probe `{ method: get, path: /, host: nomatch.test, expected_status: 404 }`. **The `host: nomatch.test` (NON-EMPTY, matches no `domains` entry) is the load-bearing trigger** → the host-miss 404. Document the asserted line `{"method":"GET","proto":"HTTP/1.1","rc":404,"rcd":"route_not_found"}` (keys re-sorted by the renderer; `rc` is the json NUMBER 404; authoring order in the json_format YAML is irrelevant), cross-proxy EQUALITY.
  - **`README.md`** — adapt `0054`'s to phase 47 / ADR-0104: the host-miss `%RESPONSE_CODE_DETAILS%` witness (CONSUMES M46-1); the host-miss trigger (a `domains:["match.test"]` vhost + a `Host: nomatch.test` probe → no vhost match → 404); envoy-rust now sets `route_not_found` at the H1 no-matching-virtual_host `synth_404` arm (`hcm.rs:1535`); both route-walk 404 arms now carry `route_not_found`.
- [ ] **Step 2 — author the differential test.** Clone `tests/differential/tests/access_log_rcd_route_not_found.rs`; swap the doc-comment → host-miss/phase 47/ADR-0104, the fixture dir → `0055-accesslog-rcd-host-not-found`, the test fn → `access_log_rcd_host_not_found`. Keep the Docker-gate cfg identical.
- [ ] **Step 3 — rebuild + compile:** `cargo build -p envoy-bin` (the differential runs `target/debug/envoy-bin`) + `cargo build -p differential --tests`. Both must succeed.
- [ ] **Step 4 — run IN ISOLATION (Docker-gated):** `cargo test -p differential --test access_log_rcd_host_not_found -- --nocapture`. Expected: GREEN (both proxies emit the byte-identical 404 line incl. `rcd:"route_not_found"`). If it REDs with an `rcd` MISMATCH (envoy-rust `null`/different, or Envoy not `route_not_found`), STOP + report (contradicts the recon). Host networking/flake REDs (non-byte-mismatch) are CI-authoritative — note + proceed.
- [ ] **Step 5 — commit.** `test(differential): fixture 0055 %RESPONSE_CODE_DETAILS%=route_not_found byte-exact (host-miss 404) [phase47 T2]`

### Task 3: BEHAVIOR_CONTRACT (consume M46-1) + (fuzz SKIP) + local gate
**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `%RESPONSE_CODE_DETAILS%` row `~:1031`)

- [ ] **Step 1 — BEHAVIOR_CONTRACT.** Update the `%RESPONSE_CODE_DETAILS%` row: the host-miss (no-matching-virtual_host) 404 path is now ALSO DIFFERENTIALLY WITNESSED byte-exact by fixture `0055` → `route_not_found` (set at the H1 `:1535` arm; **M46-1 CONSUMED** — both route-walk 404 arms now carry `route_not_found`). Keep the existing `direct_response`/`via_upstream`/`no_healthy_upstream`/route-miss-`route_not_found` notes; the connect/overflow (M45-2) + H2 (M45-1) deferrals stay.
- [ ] **Step 2 — fuzz seed (PLAN-VERIFY → SKIP).** Confirm `crates/envoy-config/fuzz/corpus/parse_bootstrap/response_code_details.yaml` already covers `%RESPONSE_CODE_DETAILS%` → SKIP (note in the commit). NO new fuzz target.
- [ ] **Step 3 — local gate.** `cargo build --workspace` + `cargo clippy --workspace --all-targets` + `cargo fmt --check` + `cargo test --workspace` (modulo the documented host-flakes — the `…h2_handshake…` + the differential parallel-load/bridge-IP + the `eds_cluster_with_neither_is_fatal` port-reuse flake — CI authoritative) + `cargo deny check`.
- [ ] **Step 4 — commit.** `docs(behavior-contract): %RESPONSE_CODE_DETAILS% host-miss 404 path witnessed by 0055; consumes M46-1 (fuzz seed already present → skip) [phase47 T3]`

---

## Acceptance (§7.5, re-run at state-4)
(a) fixture `0055` green (cross-proxy-equal `rcd:"route_not_found"` on the host-miss 404) + (b) all `0001`-`0054` green simultaneously (additive — byte-identical) + (c) h2spec ≥95% + (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `Op`/`AccessLogRecord` field/`ConfigError` variant; the ONLY `src/` change is the `:1535` `Some("route_not_found")` detail set. **M46-1 CONSUMED.**

## Notes for the executor
- **Edit ONLY `hcm.rs:1535`** (the "no matching virtual_host" arm). The `:1553`/`:1554` arm ("no matching route") is already `Some("route_not_found")` from phase 46 — leave it.
- **The probe Host `nomatch.test` MUST be NON-EMPTY** and match no `domains` entry → the host-miss arm. An EMPTY Host would hit the codec's `synth_400` guard before the vhost-walk (a different path — wrong).
- The differential runs the DEBUG `target/debug/envoy-bin` (rebuild before the fixture); run `0055` in isolation (parallel-load flake).
- Byte-preservation: additive (a previously-`None` field on the host-miss arm; no existing host-miss-404 access-log fixture) → all `0001`-`0054` stay byte-identical.
- **H2 + connect/overflow are OUT of scope** (M45-1 / M45-2).

---

_Scope locked by **ADR-0104**. The §6.2 recon (state-1 + spec-review) CONFIRMED the host-miss `route_not_found` string + the `:1535` set-site + the wirable trigger + the fuzz SKIP — REFINING but NOT overturning §A-§D → no §6.2-reconciliation ADR. The §6.1 split does NOT fire (~3 tasks; **ADR-0105 reserved**). The state-3 implementation is the next session._
