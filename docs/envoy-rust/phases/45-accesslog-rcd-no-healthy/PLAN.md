# Phase 45 — `45-accesslog-rcd-no-healthy` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans`. Steps use `- [ ]`. TDD per `superpowers:test-driven-development`.

**Goal:** Differentially witness the FIRST FAILURE-path `%RESPONSE_CODE_DETAILS%` string — `no_healthy_upstream` — BYTE-EXACT on the no-healthy-upstream 503 path, by SETTING `Some("no_healthy_upstream")` at envoy-rust's H1 no-healthy synth arm (the detail is `None` today) and witnessing it via a NEW access-log fixture `0053`.

**Architecture:** A small code change + a new fixture. envoy-rust's H1 no-healthy arm already returns a byte-matching 503 + `no healthy upstream` body, and the access-log record is built unconditionally below the writer-arm match (`hcm.rs:1243`), so the access-log line is observable — it just renders `rcd:null` today because `response_code_details_for_log` is left `None` on the `pick()->None` path. The fix is a 3-line `else` branch at the `BuildOutcome::Proxy` arm. The fixture drives a `metadata_match` NO_FALLBACK subset-miss (the fixture-`0038` `/nope` pattern) → 503 on BOTH proxies, asserted cross-proxy-equal by the existing `http1_access_log_byte_exact` driver.

**Tech Stack:** the H1 HCM proxy arm (`crates/envoy-http1/src/hcm.rs`); the `lb_subset_config` NO_FALLBACK subset machinery (ADR-0074, fixture `0038`); the `Http1AccessLogByteExact` differential driver.

**§6.2 RECON — DONE (state-2):**
- **The detail string** is RESOLVED at state-1: live `envoyproxy/envoy:v1.33.0` emits `{"rc":503,"rcd":"no_healthy_upstream","rf":"UH"}` on the NO_FALLBACK subset-miss 503 — a clean brace-free deterministic constant.
- **The threading seam** is `else`-branch at the Proxy arm. `attempt.endpoint == None` is EXCLUSIVELY the no-healthy `pick()->None` path: `crates/envoy-http1/src/hcm.rs:438` is the ONLY `endpoint: None` return; every other `AttemptResult` (connect-fail `:599`, reset `:619`, real-response `:628`, overflow `:639`) carries `endpoint: Some`. The existing `if let Some(endpoint) = attempt.endpoint { … response_code_details_for_log = Some("via_upstream") }` (`hcm.rs:990-996`) gets an `else { response_code_details_for_log = Some("no_healthy_upstream".to_owned()) }`. NO `AttemptResult` struct change; NO change to the 503 status/body/headers/flags.
- **Fuzz coverage** already exists (`crates/envoy-config/fuzz/corpus/parse_bootstrap/response_code_details.yaml`, phase 42) → **SKIP** a new seed.
- **The fixture trigger** = a STATIC `subset_cluster` (`lb_subset_config { fallback_policy: NO_FALLBACK, subset_selectors: [{ keys: [stage] }] }`) with ONE endpoint carrying `metadata.filter_metadata.envoy.lb: { stage: prod }` at a literal unreachable address `127.0.0.1:1` (the endpoint is never dialed — the subset-miss 503 happens at routing time), and a single route `metadata_match: { filter_metadata: { envoy.lb: { stage: nonexistent } } }` → no subset → 503. Using a LITERAL `127.0.0.1:1` (NOT `{{BACKEND_IP}}`/`{{HTTP1_BACKEND_PORT}}`) keeps the two configs byte-identical with NO backend spawned and NO shared-IP machinery (the access-log line logs no `%UPSTREAM_HOST%`, so the endpoint address never appears).

---

## File Structure
- **Modify** `crates/envoy-http1/src/hcm.rs` — the `else` branch at the Proxy arm (`~:990-996`) setting `response_code_details_for_log = Some("no_healthy_upstream")` on `attempt.endpoint.is_none()`; + a `#[test]` backstop.
- **Create** `tests/fixtures/0053-accesslog-rcd-no-healthy/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}` — the NO_FALLBACK subset-miss fixture logging `%RESPONSE_CODE_DETAILS%`.
- **Create** `tests/differential/tests/access_log_rcd_no_healthy.rs` — the Docker-gated differential test (clone `access_log_upstream_host.rs`).
- **Modify** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — the `%RESPONSE_CODE_DETAILS%` row (`~:1031`): the no-healthy-upstream failure path now differentially witnessed → `no_healthy_upstream`.

> Before starting: read `crates/envoy-http1/src/hcm.rs:383-401` (`AttemptResult`), `:425-440` (the no-healthy arm), `:980-996` (the Proxy-arm `via_upstream` set-site — the seam), `:1240-1245` (the unconditional record build); `tests/fixtures/0038-lb-subset/envoy.yaml` (the NO_FALLBACK subset cluster + `/nope` route); `tests/fixtures/0052-accesslog-upstream-host/` (the access-log fixture template) + `tests/differential/tests/access_log_upstream_host.rs` (the differential-test template).

---

### Task 1: SET the H1 no-healthy `%RESPONSE_CODE_DETAILS%` detail + in-process backstop
**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (the Proxy-arm seam + a `#[test]`)

- [ ] **Step 1 — write the failing backstop test.** In the `hcm.rs` test module, add a `#[tokio::test]` that drives a request through the FULL H1 dispatch path (where the `AccessLogRecord` is built + the `else` seam lives) to a NO_FALLBACK subset-miss cluster (a `127.0.0.1:1` subset cluster like the fixture, so `pick()->None`), and asserts the emitted access-log line carries `rcd:"no_healthy_upstream"` AND the response is still status 503 with the `no healthy upstream` body (UNCHANGED). **Template (plan-review M1):** clone the FILE-ACCESS-LOG-CAPTURE harness `h1_access_log_reflects_post_encode_headers` (`hcm.rs:~5093`, which configures a `log_path` and reads the emitted json line) — NOT the synth-only unit test `synth_no_healthy_upstream_emits_19_byte_body_and_5_headers` (`:5026`, which only checks body/headers and never builds the record). Compose that file-capture style with the subset-miss cluster config so the captured json line is asserted to contain `no_healthy_upstream`.
- [ ] **Step 2 — run it to confirm it FAILS** (`cargo test -p envoy-http1 <test_name>`) — expected: the detail is `None` today, so the assertion fails.
- [ ] **Step 3 — implement the minimal change.** At the Proxy arm (`hcm.rs:~990`), change:
  ```rust
  if let Some(endpoint) = attempt.endpoint {
      upstream_host_for_log = Some(endpoint.to_string());
      response_code_details_for_log = Some("via_upstream".to_owned());
  }
  ```
  to add an `else` branch:
  ```rust
  } else {
      // phase 45 (ADR-0102): pick()->None is the no-healthy-upstream synth-503
      // path (the ONLY `endpoint: None` AttemptResult, hcm.rs:438). Envoy emits
      // %RESPONSE_CODE_DETAILS% = "no_healthy_upstream" here (state-1 recon).
      response_code_details_for_log = Some("no_healthy_upstream".to_owned());
  }
  ```
  NO other change — the 503 status/body/headers/flags and all stats are untouched.
- [ ] **Step 4 — run the backstop + the full `envoy-http1` suite** (`cargo test -p envoy-http1`) — expected: GREEN; no regression in the existing no-healthy / access-log / retry tests (the detail is additive).
- [ ] **Step 5 — commit.** `feat(http1): set %RESPONSE_CODE_DETAILS%=no_healthy_upstream on the no-healthy synth-503 path [phase45 T1]`

### Task 2: fixture `0053` + the differential test
**Files:**
- Create: `tests/fixtures/0053-accesslog-rcd-no-healthy/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/access_log_rcd_no_healthy.rs`

- [ ] **Step 1 — author the fixture.**
  - **`envoy.yaml`** — an HCM with a file access-logger `json_format: { rc: "%RESPONSE_CODE%", rcd: "%RESPONSE_CODE_DETAILS%", method: "%REQ(:METHOD)%", proto: "%PROTOCOL%" }` (log `path` → `/tmp/0053-envoy-mount/access.log`); a single route `match: { prefix: "/" }` → `route: { cluster: subset_cluster, metadata_match: { filter_metadata: { envoy.lb: { stage: nonexistent } } } }`; a STATIC `subset_cluster` with `lb_policy: ROUND_ROBIN` + `lb_subset_config { fallback_policy: NO_FALLBACK, subset_selectors: [{ keys: [stage] }] }` + ONE endpoint `socket_address: { address: 127.0.0.1, port_value: 1 }` with `metadata: { filter_metadata: { envoy.lb: { stage: prod } } }`; `node.id`/`cluster` → phase-45. (LITERAL `127.0.0.1:1` — NOT a `{{BACKEND_IP}}`/`{{HTTP1_BACKEND_PORT}}` marker; no backend spawns; the endpoint is never dialed.)
  - **`envoy-rust.yaml`** — IDENTICAL except the standard per-side deltas (the fixture-0038/0052 convention: `admin` 0.0.0.0→127.0.0.1 or dropped per the template; listener bind `0.0.0.0`→`127.0.0.1`; log `path` → `/tmp/0053-envoy-rust-mount/access.log`). The cluster + route + subset config are BYTE-IDENTICAL.
  - **`expectations.yaml`** — clone `0052/expectations.yaml`: `driver.kind: http1_access_log_byte_exact`, the `/tmp/0053-*` log paths, one probe `{ method: get, path: /, host: envoy-rust.test }`. Document the asserted line is `{"method":"GET","proto":"HTTP/1.1","rc":503,"rcd":"no_healthy_upstream"}` (keys sort by UTF-8 byte order — method, proto, rc, rcd), cross-proxy EQUALITY (the 503 is the no-healthy synth, deterministic on both sides). NOTE `%RESPONSE_CODE%` renders the json NUMBER `503` — this is CONFIRMED by precedent (plan-review M2): fixture `0047-accesslog-json-nested` already proves envoy-rust renders a bare `%RESPONSE_CODE%` json leaf as the number (not a quoted string), byte-identical to Envoy; and the driver asserts pure cross-proxy equality (no static literal), so any shape disagreement would surface as a clean differential RED, not a silent miss. (No special handling needed.)
  - **`README.md`** — adapt `0052/README.md` to phase 45 / ADR-0102: the FIRST failure-path `%RESPONSE_CODE_DETAILS%` witness; the NO_FALLBACK subset-miss trigger (fixture-0038 `/nope` pattern, NOT empty-endpoints which is boot-fatal); envoy-rust now sets `no_healthy_upstream` at the H1 no-healthy synth arm.
- [ ] **Step 2 — author the differential test.** Clone `tests/differential/tests/access_log_upstream_host.rs`; swap the doc-comment → `%RESPONSE_CODE_DETAILS%`/`no_healthy_upstream`/phase 45/ADR-0102, the fixture dir → `0053-accesslog-rcd-no-healthy`, the test fn → `access_log_rcd_no_healthy`.
- [ ] **Step 3 — rebuild the DEBUG binary FIRST** (`cargo build -p envoy-bin`) + `cargo build -p differential --tests`.
- [ ] **Step 4 — run it IN ISOLATION** (Docker-gated): `cargo test -p differential --test access_log_rcd_no_healthy -- --nocapture`. Expected: GREEN (both proxies emit the byte-identical 503 access-log line incl. `rcd:"no_healthy_upstream"`). If it REDs with a `rcd` MISMATCH (e.g. envoy-rust emits `null` or a different string, or Envoy emits a subset-specific variant), that is a real finding — investigate (the state-1 recon projected `no_healthy_upstream` on this exact trigger). Host networking/flake REDs (non-byte-mismatch) are CI-authoritative — note + proceed.
- [ ] **Step 5 — commit.** `test(differential): fixture 0053 %RESPONSE_CODE_DETAILS%=no_healthy_upstream byte-exact (no-healthy 503) [phase45 T2]`

### Task 3: BEHAVIOR_CONTRACT + (fuzz SKIP) + local gate
**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `%RESPONSE_CODE_DETAILS%` row `~:1031`)

- [ ] **Step 1 — BEHAVIOR_CONTRACT.** Update the `%RESPONSE_CODE_DETAILS%` row: the no-healthy-upstream failure path is now DIFFERENTIALLY WITNESSED byte-exact by fixture `0053` → `no_healthy_upstream` (set at the H1 no-healthy synth arm; the connect-failure/overflow failure details remain deferred — M45-2; H2 failure details deferred — M45-1). Keep the existing success-path (`direct_response`/`via_upstream`) notes.
- [ ] **Step 2 — fuzz seed (PLAN-VERIFY → SKIP).** Confirm `crates/envoy-config/fuzz/corpus/parse_bootstrap/response_code_details.yaml` already covers `%RESPONSE_CODE_DETAILS%` (it does, phase 42) → SKIP a new seed (note in the commit). NO new fuzz target.
- [ ] **Step 3 — local gate.** `cargo build --workspace` + `cargo clippy --workspace --all-targets` + `cargo fmt --check` + `cargo test --workspace` (modulo the documented `…h2_handshake…` host-flake + the differential parallel-load/bridge-IP flakes — CI authoritative) + `cargo deny check`.
- [ ] **Step 4 — commit.** `docs(behavior-contract): %RESPONSE_CODE_DETAILS% no-healthy failure path witnessed by 0053 (fuzz seed already present → skip) [phase45 T3]`

---

## Acceptance (§7.5, re-run at state-4)
(a) fixture `0053` green (cross-proxy-equal `rcd:"no_healthy_upstream"` on the 503) + (b) all `0001`-`0052` green simultaneously (additive — the detail is set on a previously-`None` arm, no existing no-healthy access-log fixture → byte-identical) + (c) h2spec ≥95% + (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `Op`/`AccessLogRecord` field/`ConfigError` variant; the ONLY `src/` change is the no-healthy `else`-branch detail set in `hcm.rs`.

## Notes for the executor
- **The trigger MUST be the NO_FALLBACK subset-miss, NOT empty-endpoints** (`endpoints: []` is boot-fatal in envoy-rust — `ConfigError::EmptyClusterEndpoints`, `bootstrap.rs:3193`).
- **The endpoint is never dialed** — use a LITERAL `127.0.0.1:1` (no `{{BACKEND_IP}}`/`{{HTTP1_BACKEND_PORT}}` marker, no backend spawn); the subset-miss 503 happens at routing time. This keeps both configs byte-identical without the shared-IP machinery.
- **`attempt.endpoint == None` is EXCLUSIVELY the no-healthy path** (`hcm.rs:438`) — the `else`-branch cannot mis-fire on connect-fail/overflow (those carry `endpoint: Some`).
- The differential runs the DEBUG `target/debug/envoy-bin` (rebuild before the fixture); run `0053` in isolation (parallel-load flake).
- Byte-preservation: the detail is additive (a previously-`None` field, no existing no-healthy access-log fixture) → all `0001`-`0052` stay byte-identical.
- **H2 is OUT of scope** (M45-1): the H2 no-healthy arm returns 502 (not 503), and no H2 access-log differential driver exists — do NOT touch the H2 path.

---

_Scope locked by **ADR-0102**. The §6.2 recon (this state-2) CONFIRMED the `no_healthy_upstream` string + the `else`-branch seam + the fuzz SKIP — REFINING but NOT overturning §A-§D → no §6.2-reconciliation ADR. The §6.1 split does NOT fire (~3 tasks; **ADR-0103 reserved**). The state-3 implementation is the next session._
