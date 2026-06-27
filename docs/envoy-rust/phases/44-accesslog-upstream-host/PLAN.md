# Phase 44 — `44-accesslog-upstream-host` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans`. Steps use `- [ ]`. TDD per `superpowers:test-driven-development`.

**Goal:** Differentially witness the `%UPSTREAM_HOST%` access-log command operator — the resolved upstream endpoint `<ip>:<port>` — BYTE-EXACT on a real upstream (the gap fixture `0051` excluded), via a NEW proxy access-log fixture `0052`.

**Architecture:** A FIXTURE-ONLY phase (witness an EXISTING operator — `%UPSTREAM_HOST%` has been implemented since phase 06). Fixture `0052-accesslog-upstream-host` routes through a `{{BACKEND_IP}}` shared-host-LAN-IP STATIC cluster (the harness's `discover_host_lan_ip()` mechanism, used by the LB fixtures `0036`/`0037`/`0038`) so BOTH proxies dial the IDENTICAL `<host-LAN-IP>:<port>` and render the IDENTICAL `%UPSTREAM_HOST%`, asserted cross-proxy-EQUAL by the existing `http1_access_log_byte_exact` driver. The access-log format + driver clone `0051`; the STATIC `{{BACKEND_IP}}` cluster comes from `0036`/`0037` (NOT `0051`'s STRICT_DNS).

**Tech Stack:** the `testcontainers` differential harness; the `Http1EchoBackend` + `{{BACKEND_IP}}`/`discover_host_lan_ip()` machinery (`run_fixture` `tests/differential/src/lib.rs:3011-3013`); the `Http1AccessLogByteExact` driver (`assert_access_log_lines_byte_identical` `access_log.rs:305`).

**§6.2 FORMAT-MATCH RECON — DONE (the gating call): FIXTURE-ONLY, no `src/` change.** envoy-rust renders `%UPSTREAM_HOST%` via `Some(endpoint.to_string())` where `endpoint: std::net::SocketAddr` (`crates/envoy-http1/src/hcm.rs:391`/`:994`). `SocketAddr::to_string()` (Display) renders `<ip>:<port>` (IPv4: `127.0.0.1:8080`) — byte-IDENTICAL to Envoy's `%UPSTREAM_HOST%` `<ip>:<port>` (the BEHAVIOR_CONTRACT row already states "value-exact for STRICT_DNS single-A-record resolution"). The differential uses an IPv4 host-LAN-IP (`{{BACKEND_IP}}`), so no IPv6 bracketing edge. → NO `%UPSTREAM_HOST%` format fix needed; the phase is the fixture + docs. **There is NO static expected literal** (the `<host-LAN-IP>:<port>` is DYNAMIC per CI run) — the assertion is PURE cross-proxy equality.

---

## File Structure
- **Create** `tests/fixtures/0052-accesslog-upstream-host/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}` — the proxy access-log fixture (STATIC `{{BACKEND_IP}}` cluster + a `json_format` logging `%UPSTREAM_HOST%`).
- **Create** `tests/differential/tests/access_log_upstream_host.rs` — the Docker-gated differential test (clone `access_log_upstream_cluster.rs`).
- **Modify** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — update the `%UPSTREAM_HOST%` row (line ~1029): now DIFFERENTIALLY WITNESSED byte-exact by fixture `0052` via a `{{BACKEND_IP}}` shared-host-LAN-IP STATIC cluster.
- **(PLAN-VERIFY) Modify** the fuzz corpus — add a `%UPSTREAM_HOST%` `parse_bootstrap` seed ONLY IF one does not already cover the operator (`grep -rl UPSTREAM_HOST crates/*/fuzz/corpus/`; the existing `0040`/`0051` bootstrap seeds may already include it). NO new fuzz target.

> Before starting: read `tests/fixtures/0051-accesslog-upstream-cluster/` (the access-log format + `http1_access_log_byte_exact` driver template) + `tests/fixtures/0036-lb-ring-hash/envoy.yaml`'s `clusters:` stanza (the STATIC `{{BACKEND_IP}}` precedent) + `tests/differential/tests/access_log_upstream_cluster.rs` (the differential-test template).

---

### Task 1: fixture `0052` + the differential test (the core deliverable)
**Files:**
- Create: `tests/fixtures/0052-accesslog-upstream-host/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/access_log_upstream_host.rs`

- [ ] **Step 1 — author the fixture.**
  - **`envoy.yaml`** — clone `0051/envoy.yaml`'s HCM + access-log block, BUT (i) change the `json_format` to log `%UPSTREAM_HOST%` (+ `%UPSTREAM_CLUSTER%` + `%RESPONSE_CODE_DETAILS%` + `%REQ(:METHOD)%` + `%PROTOCOL%` as deterministic anchors — all cross-proxy-shared); (ii) REPLACE the cluster with a **STATIC** cluster modeled on `0036`/`0037`: `type: STATIC`, ONE endpoint `socket_address: { address: {{BACKEND_IP}}, port_value: {{HTTP1_BACKEND_PORT}} }` (do NOT clone `0051`'s `STRICT_DNS`/`{{BACKEND_HOST}}` cluster — that re-introduces the per-side mismatch); the route forwards to that cluster; `node.id`/`cluster` → phase-44; the log `path` → `/tmp/0052-envoy-mount/access.log`.
  - **`envoy-rust.yaml`** — same as `envoy.yaml` EXCEPT the documented per-side deltas (match `0051`'s envoy-rust.yaml deltas: drop `admin:`, listener `127.0.0.1`, the log `path` → `/tmp/0052-envoy-rust-mount/access.log`, drop `generate_request_id: false` + `request_headers_to_remove` if present). Keep the SAME STATIC `{{BACKEND_IP}}`/`{{HTTP1_BACKEND_PORT}}` cluster.
  - **`expectations.yaml`** — clone `0051/expectations.yaml`: `driver.kind: http1_access_log_byte_exact`, the `/tmp/0052-*` log paths, one probe `{ method: get, path: /, host: envoy-rust.test }`. Document that the assertion is CROSS-PROXY EQUALITY (no static literal — `%UPSTREAM_HOST%` = the DYNAMIC-but-SHARED `<host-LAN-IP>:<port>`); the comment notes the expected SHAPE `<ip>:<port>`.
  - **`README.md`** — adapt `0051/README.md` to `%UPSTREAM_HOST%` (phase 44, ADR-0101): the phase that CLOSES the `%UPSTREAM_HOST%` gap `0051` left; the `{{BACKEND_IP}}` shared-host-LAN-IP STATIC cluster makes `%UPSTREAM_HOST%` byte-identical cross-proxy; `%UPSTREAM_HOST%` has been implemented since phase 06 (`SocketAddr::to_string()` = `<ip>:<port>` = Envoy's format).
- [ ] **Step 2 — author the differential test.** Clone `tests/differential/tests/access_log_upstream_cluster.rs`, swap the doc-comment → `%UPSTREAM_HOST%`/phase 44/ADR-0101, the fixture dir → `0052-accesslog-upstream-host`, the test fn → `access_log_upstream_host`.
- [ ] **Step 3 — rebuild the DEBUG binary FIRST** (`cargo build -p envoy-bin` — the differential runs `target/debug/envoy-bin`) + `cargo build -p differential --tests` (the test compiles).
- [ ] **Step 4 — run it IN ISOLATION** (Docker-gated): `cargo test -p differential --test access_log_upstream_host -- --nocapture`. Expected: GREEN (both proxies emit the byte-identical line incl. the shared `%UPSTREAM_HOST%`). **Host note:** this dials a real backend over the host-gateway; if it false-REDs with a host-networking/backend-dial error (NOT a byte-mismatch), that is the documented CI-authoritative flake (the `consistent-hash-lb`/bridge-IP memory) — note + proceed (CI authoritative). If it REDs with a `%UPSTREAM_HOST%` BYTE-MISMATCH between the two proxies, that IS a real finding (envoy-rust's format diverges) — investigate (would mean the §6.2 format-match assessment was wrong).
- [ ] **Step 5 — commit.** `test(differential): fixture 0052 %UPSTREAM_HOST% byte-exact (shared-IP proxy access-log) [phase44 T1]`

### Task 2: BEHAVIOR_CONTRACT update + (PLAN-VERIFY) fuzz seed + local gate
**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `%UPSTREAM_HOST%` row ~`:1029`)
- (PLAN-VERIFY) the fuzz corpus

- [ ] **Step 1 — BEHAVIOR_CONTRACT.** Update the `%UPSTREAM_HOST%` row: it is now DIFFERENTIALLY WITNESSED byte-exact by fixture `0052` (phase 44, ADR-0101) via a `{{BACKEND_IP}}` shared-host-LAN-IP STATIC cluster — both proxies dial the IDENTICAL `<host-LAN-IP>:<port>` and render the IDENTICAL `%UPSTREAM_HOST%` (`SocketAddr::to_string()` = `<ip>:<port>` = Envoy's format), asserted cross-proxy-equal. (Keep the existing `-`-on-direct_response note.)
- [ ] **Step 2 — fuzz seed (PLAN-VERIFY).** `grep -rl 'UPSTREAM_HOST' crates/envoy-config/fuzz/corpus/ crates/envoy-accesslog/fuzz/corpus/`. IF no seed exercises `%UPSTREAM_HOST%` in a `parse_bootstrap` access-log `json_format`, add `crates/envoy-config/fuzz/corpus/parse_bootstrap/upstream_host.yaml` (a full bootstrap with `%UPSTREAM_HOST%` in an access-log `json_format`; distinct filename + `!`-un-ignore line in `crates/envoy-config/fuzz/.gitignore`; `git ls-files` to confirm tracked). IF already covered, SKIP (note in the commit). NO new fuzz target.
- [ ] **Step 3 — local gate.** `cargo build --workspace` + `cargo clippy --workspace --all-targets` + `cargo fmt --check` + `cargo test --workspace` (modulo the documented `…h2_handshake…` host-flake + the differential parallel-load/bridge-IP flakes — CI authoritative) + `cargo deny check`.
- [ ] **Step 4 — commit.** `docs(behavior-contract): %UPSTREAM_HOST% differentially witnessed by 0052 (+ fuzz seed if missing) [phase44 T2]`

---

## Acceptance (§7.5, re-run at state-4)
(a) fixture `0052` green (cross-proxy-equal `%UPSTREAM_HOST%` line) + (b) all `0001`-`0051` green simultaneously (NO operator/field change → byte-identical) + (c) h2spec ≥95% + (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency; NO new `ConfigError` variant; **NO new `AccessLogRecord` field; NO new `Op` variant** (witnesses an existing operator); NO `src/` change (the §6.2 format-match is byte-exact via `SocketAddr::to_string()`).

## Notes for the executor
- **FIXTURE-ONLY phase** — `%UPSTREAM_HOST%` already exists (phase 06); the §6.2 recon confirmed `SocketAddr::to_string()` = `<ip>:<port>` = Envoy's format → NO `src/` change. If the `0052` differential REDs with a `%UPSTREAM_HOST%` byte-mismatch (not a host-networking flake), THAT contradicts the recon — STOP + investigate (a real format finding).
- **The cluster MUST be STATIC `{{BACKEND_IP}}` (from `0036`/`0037`), NOT `0051`'s STRICT_DNS `{{BACKEND_HOST}}`** — the latter re-introduces the per-side host-mismatch that made `0051` exclude `%UPSTREAM_HOST%`. **(plan-review M2)** Take the CLUSTER SHAPE (STATIC + `address: {{BACKEND_IP}}`) from `0036`, but the PORT marker from `0051`/`0008` — `0052` is SINGLE-backend, so use ONE endpoint with `port_value: {{HTTP1_BACKEND_PORT}}` (the single-backend marker that drives the `Http1EchoBackend` auto-spawn); do NOT copy `0036`'s two-backend `{{HTTP1_BACKEND_1_PORT}}`/`{{HTTP1_BACKEND_2_PORT}}` markers.
- **(plan-review M1)** The `%UPSTREAM_HOST%` fuzz seed ALREADY EXISTS (`crates/envoy-config/fuzz/corpus/parse_bootstrap/json_format_logger.yaml` carries `%UPSTREAM_HOST%` in a `json_format` access-log) — Task 2 Step 2's grep will hit it → SKIP the new seed (note in the commit); do NOT add one.
- **No static expected literal** — `%UPSTREAM_HOST%` is dynamic-but-shared; the assertion is cross-proxy equality.
- The differential runs the DEBUG `target/debug/envoy-bin` (rebuild before the fixture); both proxies must dial ONE shared host-gateway backend (the `consistent-hash-lb`/bridge-IP memory — CI authoritative); run `0052` in isolation (parallel-load flake).
- Byte-preservation: NO operator/record-field change → all `0001`-`0051` stay byte-identical.

---

_Scope locked by **ADR-0101**. The §6.2 format-match recon (this state-2) confirmed FIXTURE-ONLY (`SocketAddr::to_string()` = Envoy's `<ip>:<port>`) — REFINING but NOT overturning §A-§C → no §6.2-reconciliation ADR. The §6.1 split does NOT fire (~2 tasks; **ADR-0102 reserved**). The state-3 implementation is the next session._
