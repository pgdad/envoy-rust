# Phase 43 — `43-accesslog-upstream-cluster` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans`. Steps use `- [ ]`. TDD per `superpowers:test-driven-development` on every task.

**Goal:** Add the `%UPSTREAM_CLUSTER%` access-log command operator (the matched cluster's config `name`) + stand up the FIRST proxy (upstream-routed) access-log differential fixture — byte-equivalent to upstream Envoy v1.33.0.

**Architecture:** The operator is an EXACT `%UPSTREAM_HOST%` mirror — a new `AccessLogRecord.upstream_cluster: Option<String>` field set by the HCM at the proxy arm (the cluster name is in hand as `BuildOutcome::Proxy { cluster }`), a new `Op::UpstreamCluster` (a `"UPSTREAM_CLUSTER"` no-arg keyword), its `render_op` arm, and its `encode_single_op` arm. The enabling infra is the first proxy access-log fixture: extend the `Driver::Http1WithAccessLog` differential driver to optionally start an `Http1EchoBackend` (`tests/differential/src/backend.rs:170` — the HTTP backend fixture `0008-http1-router-upstream` already dials) and template its address into the paired configs so the listener routes to a cluster (vs `direct_response`).

**Tech Stack:** Rust workspace; the hand-rolled `envoy-accesslog` command-operator engine; the `testcontainers` differential harness + its `Http1EchoBackend` / both-proxies-dial-same-backend pattern (fixture `0008`).

**§6.1 SPLIT DECISION — does NOT fire (kept WHOLE); ADR-0101 stays RESERVED.** The state-1 SPEC projected the split as LIKELY, but the state-2 recon found the enabling pieces ALREADY EXIST: the HTTP backend (`Http1EchoBackend`) + the both-proxies-dial-same-backend pattern (fixture `0008`). So the proxy access-log fixture is COMBINING existing pieces (the access-log driver + the `0008` backend wiring), not from-scratch infra — ~7 TDD tasks, under the §6.1 gate. This REFINES the SPEC's projection WITHOUT overturning an ADR-0100 §A-§C fact (those are the operator's existence/value), so NO §6.2-reconciliation ADR fires. **ADR-0101 stays available** if the driver work balloons at state-3.

**§6.2 LOCKED FACTS (recon ran THIS state-2 against live `envoyproxy/envoy:v1.33.0`):**
- **Strict no-arg grammar:** `%UPSTREAM_CLUSTER:3%` boot-fatals `UPSTREAM_CLUSTER does not take any parameters or length` — NO `(...)`, NO `:N` (the `%UPSTREAM_HOST%`/`%RESPONSE_CODE_DETAILS%` no-arg keyword class).
- **The value:** a `route: { cluster: my_backend_cluster }` → `%UPSTREAM_CLUSTER%` = `my_backend_cluster` (json single-op → quoted; mixed → `c=my_backend_cluster`); the same line shows `%RESPONSE_CODE_DETAILS%`→`via_upstream`. Absent (`null`/`-`) on a direct_response/local-reply path.
- **`%UPSTREAM_HOST%` determinism:** it renders the resolved backend ip:port → NON-deterministic byte-for-byte. **The mismatch is STRUCTURAL, not just port-ephemerality (plan-review finding):** fixture `0008`'s `{{BACKEND_HOST}}` resolves to DIFFERENT values per-side (the host-gateway IP on the upstream-Envoy-container side vs `127.0.0.1` on the envoy-rust-host-subprocess side) AND `{{HTTP1_BACKEND_PORT}}` is OS-ephemeral — so even a pinned port would still diverge on the IP. → Do NOT over-invest in pinning `%UPSTREAM_HOST%` byte-exact; the SAFE path is to EXCLUDE/normalize the `%UPSTREAM_HOST%` token from the byte-exact assertion (witness it loosely, e.g. via a `json_shape` rule). `%UPSTREAM_CLUSTER%` + `%RESPONSE_CODE_DETAILS%` ARE byte-exact (config/flow-deterministic).

---

## File Structure
- **Modify** `crates/envoy-accesslog/src/record.rs` — add `pub upstream_cluster: Option<String>` (after `upstream_host` `:83`); fix all `AccessLogRecord { … }` literals (`response_code_details: None`-style — `cargo build --workspace --all-targets` finds them).
- **Modify** `crates/envoy-accesslog/src/command_operator.rs` — `Op::UpstreamCluster` (after `Op::UpstreamHost` `:64`); `"UPSTREAM_CLUSTER"` in the no-arg keyword list + the dispatch (alongside `"UPSTREAM_HOST"` `:266`); the `render_op` arm `record.upstream_cluster.as_deref().unwrap_or(empty_or_dash)` (after `:530`).
- **Modify** `crates/envoy-accesslog/src/json_format.rs` — the `encode_single_op` arm `quote_opt(out, r.upstream_cluster.as_deref())` (after `Op::UpstreamHost` `:245`).
- **Modify** `crates/envoy-http1/src/hcm.rs` — at the proxy arm (`:863`, `BuildOutcome::Proxy { cluster: cluster_name, … }`) set a new `upstream_cluster_for_log = Some(cluster_name.clone())` (declared beside `upstream_host_for_log` `:835`); the record build (`:1196` area) reads it. **Set it at the proxy-ARM entry (where `cluster_name` is bound), NOT gated on upstream success** — Envoy sets `%UPSTREAM_CLUSTER%` whenever a cluster is selected (even on upstream failure), so this avoids an M42-1-style gap (a connect-failure 503 still renders the cluster name).
- **Modify** `crates/envoy-http2/src/hcm.rs` — the H2 proxy arm (`:~675` region) sets the cluster name into a new `upstream_cluster_for_log_h2`, threaded into `finalize_h2_stream` (mirror `response_code_details_for_log_h2` `:~846`); the record build (`:~929`) reads it.
- **Modify** `tests/differential/src/lib.rs` (the `Driver::Http1WithAccessLog` variant + its run-arm `:4960`) + `tests/differential/src/access_log.rs` — add an OPTIONAL backend (start an `Http1EchoBackend`, template its host-gateway address into the configs). **PLAN-VERIFY** the seam (a new `Driver::Http1ProxyWithAccessLog` variant is cleaner than an `Option<backend>` field on the already-large `Http1WithAccessLog` — `Driver` is near clippy's `large_enum_variant` threshold; a new variant may need boxing).
- **Create** `tests/fixtures/0051-accesslog-upstream-cluster/*` + `tests/differential/tests/access_log_upstream_cluster.rs`.
- **Modify** the `parse_bootstrap` fuzz corpus (a `%UPSTREAM_CLUSTER%` seed + `!`-un-ignore) + `docs/envoy-rust/BEHAVIOR_CONTRACT.md`.

> Before starting: read `command_operator.rs` for the `Op::UpstreamHost` precedent (enum `:64`, keyword `:266`, render `:530`) + `json_format.rs:245`; read the H1 proxy arm (`hcm.rs:863`) where `cluster_name` is bound; read fixture `0008-http1-router-upstream/` (the both-proxies-dial-`Http1EchoBackend` precedent) + `tests/differential/src/backend.rs:170` (`Http1EchoBackend`).

---

### Task 1: `upstream_cluster` record field
- [ ] **Step 1 — failing test.** In `record.rs` tests, assert `AccessLogRecord` has `upstream_cluster: Option<String>` (construct with `Some("my_backend_cluster".into())`).
- [ ] **Step 2 — run, verify FAIL.** `cargo test -p envoy-accesslog upstream_cluster`.
- [ ] **Step 3 — implement.** Add `pub upstream_cluster: Option<String>` after `upstream_host` (`:83`); fix every workspace `AccessLogRecord { … }` literal with `upstream_cluster: None` (after `upstream_host: …`). `cargo build --workspace --all-targets` until green.
- [ ] **Step 4 — PASS.** `cargo test -p envoy-accesslog`.
- [ ] **Step 5 — commit.** `feat(accesslog): AccessLogRecord.upstream_cluster field [phase43 T1]`

### Task 2: `Op::UpstreamCluster` parse + text render (+ json arm to compile)
- [ ] **Step 1 — failing tests** (mirror the `upstream_host`/`route_name` tests): `parse_format("%UPSTREAM_CLUSTER%")` → `[Op(UpstreamCluster)]`; `parse_format("%UPSTREAM_CLUSTER(x)%").is_err()` AND `parse_format("%UPSTREAM_CLUSTER:3%").is_err()` (the §6.2 strict-no-arg grammar); text: `upstream_cluster: Some("my_backend_cluster")` → `my_backend_cluster`, `None` → `-`, mixed `c=%UPSTREAM_CLUSTER%` → `c=my_backend_cluster` / `c=-`.
- [ ] **Step 2 — run, verify FAIL.**
- [ ] **Step 3 — implement.** `Op::UpstreamCluster` variant (after `Op::UpstreamHost` `:64`); add `"UPSTREAM_CLUSTER"` to the no-arg keyword list + `"UPSTREAM_CLUSTER" => Op::UpstreamCluster` to the dispatch (alongside `"UPSTREAM_HOST"` `:266`); the `render_op` arm (`unwrap_or(empty_or_dash)`, after `:530`). **`encode_single_op` has no wildcard → also add the `json_format.rs` arm `Op::UpstreamCluster => quote_opt(out, r.upstream_cluster.as_deref())` (after `:245`) in THIS commit to keep the crate compiling** (mirrors phase-42 T2).
- [ ] **Step 4 — PASS** + all existing `command_operator` tests green.
- [ ] **Step 5 — commit.** `feat(accesslog): %UPSTREAM_CLUSTER% operator parse + text render [phase43 T2]`

### Task 3: `%UPSTREAM_CLUSTER%` json single-op typed render
- [ ] **Step 1 — failing tests** (mirror `upstream_host`/`route_name` json tests): single-op present → `"my_backend_cluster"`; `None` → `null`; mixed `c=%UPSTREAM_CLUSTER%` → `"c=my_backend_cluster"` / `"c=-"`.
- [ ] **Step 2 — run, verify FAIL** (the impl landed in T2 → the tests are the deliverable here; note that in the commit).
- [ ] **Step 3 — implement.** (Already landed in T2 — if so, just the tests.)
- [ ] **Step 4 — PASS** + the phase-38/39/41/42 json tests green.
- [ ] **Step 5 — commit.** `feat(accesslog): %UPSTREAM_CLUSTER% json typed render [phase43 T3]`

### Task 4: HCM upstream-cluster plumbing (H1 + H2)
- [ ] **Step 1 — failing test.** An H1 request routed to a cluster → the built `AccessLogRecord.upstream_cluster == Some("<cluster>")`; a `direct_response` route → `None`. (Test at the HCM record-construction layer, mirroring `hcm_h1_sets_response_code_details_from_response_path`; H1 + H2.)
- [ ] **Step 2 — run, verify FAIL** (always `None`).
- [ ] **Step 3 — implement.** H1: declare `let mut upstream_cluster_for_log: Option<String> = None;` beside `upstream_host_for_log` (`:835`); at the proxy arm (`:863`, `BuildOutcome::Proxy { cluster: cluster_name, … }`) set `upstream_cluster_for_log = Some(cluster_name.clone());` (at the ARM entry, NOT gated on success — set it where `cluster_name` is bound, before the proxy attempt, so a later upstream failure still renders the cluster); the record build (`:1196` area) → `upstream_cluster: upstream_cluster_for_log`. H2: mirror — set `upstream_cluster_for_log_h2` at the H2 proxy arm; thread it into `finalize_h2_stream` (a new param after `response_code_details_for_log_h2`); the record build reads it. `cargo build --workspace --all-targets`.
- [ ] **Step 4 — PASS** + `cargo test -p envoy-http1 -p envoy-http2` (modulo the documented `…h2_handshake…` host-flake).
- [ ] **Step 5 — commit.** `feat(hcm): set upstream_cluster on the access-log record (H1+H2 proxy arm) [phase43 T4]`

### Task 5: proxy access-log differential driver (the enabling harness infra)
- [ ] **Step 1 — failing test.** A new differential test `access_log_upstream_cluster` (Docker-gated) that drives an upstream-routed access-log fixture and asserts the byte-exact `%UPSTREAM_CLUSTER%` token. It FAILS first because the access-log driver cannot route to a backend.
- [ ] **Step 2 — run, verify FAIL.**
- [ ] **Step 3 — implement.** Add a proxy-capable access-log driver path. **TRY THE CHEAP SEAM FIRST (plan-review finding):** in `run_fixture` (`lib.rs:~2779`) the `Http1EchoBackend` spawn (`:~3203`) is **marker-driven** — it is gated on the fixture YAML containing the `{{HTTP1_BACKEND_PORT}}` template marker, NOT on the `Driver` variant, and it runs BEFORE the `Driver` dispatch match. Since `Driver::Http1WithAccessLog` already routes through `run_fixture`, a `0051` whose config carries `{{HTTP1_BACKEND_PORT}}`/`{{BACKEND_HOST}}` should AUTO-spawn + AUTO-template the backend with ~zero new harness plumbing, and the existing access-log run-arm (`:4960`, which only drives the request + scrapes logs) is reused as-is. **So FIRST attempt: just author fixture `0051` with the backend markers + reuse `Driver::Http1WithAccessLog`.** ONLY if that proves insufficient, add a new `Driver::Http1ProxyWithAccessLog` variant (box it — `Driver` is near the `large_enum_variant` threshold). Either way, keep the existing direct_response access-log driver UNCHANGED (`0040`-`0050` stay green). Reuse the `{{BACKEND_HOST}}`/`{{HTTP1_BACKEND_PORT}}` host-gateway machinery from fixture `0008-http1-router-upstream` (both proxies dial ONE shared backend — `consistent-hash-lb-differential` memory).
- [ ] **Step 4 — the driver compiles + the existing access-log fixtures still pass** (`cargo build -p differential --tests`; run a direct_response access-log fixture in isolation).
- [ ] **Step 5 — commit.** `test(differential): proxy access-log driver (Http1EchoBackend) [phase43 T5]`

### Task 6: fixture `0051` + seed + BEHAVIOR_CONTRACT + gate
- [ ] **Step 1 — capture the live bytes.** Boot the paired proxy access-log config against `envoyproxy/envoy:v1.33.0` (a cluster → the backend; `json_format` with `%UPSTREAM_CLUSTER%` single-op + a mixed `c=%UPSTREAM_CLUSTER%` + `%RESPONSE_CODE_DETAILS%`); capture the byte-exact line. **Resolve `%UPSTREAM_HOST%`:** include it ONLY if its ip:port is pinned/deterministic via the host-gateway templating; else EXCLUDE it from the byte-exact line (witness loosely) per §2.2.
- [ ] **Step 2 — wire fixture `0051-accesslog-upstream-cluster`** (paired `envoy.yaml`/`envoy-rust.yaml`/`expectations.yaml`/`README.md`; model the backend wiring on fixture `0008` + the access-log format on `0050`). **Rebuild `cargo build -p envoy-bin` first** (the differential runs the DEBUG binary).
- [ ] **Step 3 — run `0051` in ISOLATION** (`cargo test -p differential access_log_upstream_cluster` — differential fixtures flake under parallel load; CI authoritative). Confirm `0001`-`0050` unaffected.
- [ ] **Step 4 — PASS.**
- [ ] **Step 5 — commit + local gate.** `test(differential): fixture 0051 %UPSTREAM_CLUSTER% proxy access-log + seed + BEHAVIOR_CONTRACT [phase43 T6]` (the `%UPSTREAM_CLUSTER%` `parse_bootstrap` seed + `!`-un-ignore in `crates/envoy-config/fuzz/.gitignore` + `git ls-files` check; NO new fuzz target; the BEHAVIOR_CONTRACT "Access log field mapping" `%UPSTREAM_CLUSTER%` row). Then run the local gate: `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt --check`, `cargo test --workspace`, `cargo deny check`.

---

## Acceptance (§7.5, re-run at state-4)
(a) `0051` green (byte-exact cluster-name + via_upstream line) + (b) all `0001`-`0050` green + (c) h2spec ≥95% + (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean (with the `%UPSTREAM_CLUSTER%` seed) — NO new target + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency; NO new `ConfigError` variant; ONE new `AccessLogRecord` field; ONE new `Op` variant; the proxy access-log driver is test-only (the ONLY `src/` behavior change is the HCM `upstream_cluster` assignment).

## Notes for the executor
- `%UPSTREAM_CLUSTER%` IS the `%UPSTREAM_HOST%` pattern — copy `Op::UpstreamHost`'s no-arg keyword + `render_op` `unwrap_or` + `encode_single_op` `quote_opt` arms, substituting `upstream_cluster`.
- **The differential value is `my_backend_cluster` (config-deterministic)**; `%RESPONSE_CODE_DETAILS%`→`via_upstream` is witnessed on the SAME line (advancing M42-1); `%UPSTREAM_HOST%` is loose/excluded if non-deterministic (§2.2).
- **Set `upstream_cluster` at the proxy-ARM entry, not on success** — avoids an M42-1-style gap.
- Host-networking: the differential needs BOTH proxies (Envoy container + envoy-rust host subprocess) to dial ONE shared backend address (host-gateway / `{{BACKEND_IP}}` — the `consistent-hash-lb-differential` + `differential-host-bridge-ip` memory notes apply; this host's backend-routing fixtures false-RED locally — CI is AUTHORITATIVE).
- Default-absent byte-preservation (the new `Option` field defaults `None` + the operator is new) keeps `0001`-`0050` green.

---

_Scope locked by **ADR-0100**. The §6.1 split does NOT fire (kept WHOLE — the recon found the `Http1EchoBackend` + both-proxies-dial pattern already exist; **ADR-0101 stays reserved**). The §6.2 recon (strict no-arg grammar; the `%UPSTREAM_HOST%` determinism) REFINED but did not overturn §A-§C → no §6.2-reconciliation ADR. The state-3 implementation is the next session._
