# Phase 16 (`16-http-retries`) — PROGRESS

> Running log, updated by the executor on each task completion (the 06.2 → 15 cadence).
> One entry per PLAN task; quote the verifying command output. The state-3 arc runs
> `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK
> (NOT deferred to state-4) per `project_state3_arc_skips_clippy`.

**PLAN:** `docs/envoy-rust/phases/16-http-retries/PLAN.md`
**SPEC:** `docs/envoy-rust/phases/16-http-retries/SPEC.md`
**Scope ADRs:** ADR-0044 (minimum-viable retry scope + body-replay finding); ADR-0045 (§6.2 reconciliation — accept-and-ignore unknown tokens / per-attempt `upstream_rq_total` / `x-envoy-attempt-count` gated on `include_attempt_count_in_response`).

---

## State-2 PLAN-write (this commit)

- Performed the HEAVY SPEC §6.2 empirical verification against `envoyproxy/envoy:v1.33.0` (Docker; foreground general-purpose subagent). Findings L1–L11 locked into PLAN.md "§6.2 empirical lock-ins". Three material divergences (L2 unknown-token accept-and-ignore; L5 per-attempt `upstream_rq_total` + completing-only `upstream_rq_5xx` + Envoy-only `retry.*` sub-scope; L6 `x-envoy-attempt-count` gated on `include_attempt_count_in_response`) → **ADR-0045 landed**.
- Performed the PLAN-time SPEC-correction pass (read-only Explore subagent) against HEAD `0fa80aba9`. All SPEC §3 anchors confirmed except: `ConfigError` is in `lib.rs` (not `bootstrap.rs`); the deep-clone sites `clone_route_action`/`clone_route_config` must clone the new fields; `Driver` is in `tests/differential/src/lib.rs`; fuzz corpus is at 27 seeds (not 22). Corrections recorded in PLAN.md "PLAN-time SPEC corrections".
- Evaluated the §6.1 split gate against the §6.2-refined surface (~1450–1650 LoC / ~13 tasks) → **single un-split phase; ADR-0046 does NOT fire.**
- Flipped ROADMAP row `16` `planned → in-progress`. Advanced STATE.md to `16` state-2-complete / state-3-next.

## Task 1 — `envoy-config` schema (`RetryPolicy` + `retry_policy` field + `include_attempt_count_in_response`)

**Preamble (read before starting):**
- **Goal:** Add the `RetryPolicy` struct + `RouteAction_Route.retry_policy: Option<RetryPolicy>` (`crates/envoy-config/src/bootstrap.rs:953-955`) + `VirtualHost.include_attempt_count_in_response: bool` (`:916`, `#[serde(default)]` → false). TDD: serde round-trip + deny_unknown_fields rejection + vhost-flag tests first.
- **§6.2 lock-ins that bind this task:** L3 (`num_retries` = `Option<u32>`, default 1 resolved later; `retriable_status_codes` = `Vec<u32>`); L6 (the new `include_attempt_count_in_response` VirtualHost field is REQUIRED — `x-envoy-attempt-count` is gated on it, not automatic).
- **Anchors (verified at HEAD `0fa80aba9`):** `RouteAction_Route` `bootstrap.rs:953-955` (currently `cluster: String` only, `#[serde(deny_unknown_fields)]`); `VirtualHost` `:916`; `RouteAction` enum `:939`; `Route` `:923`. The deferred `retry_policy` fields (`per_try_timeout`, `retry_back_off`, `retry_priority`, `retry_host_predicate`, `host_selection_retry_max_attempts`, `retriable_headers`, `retriable_request_headers`, `rate_limited_retry_back_off`) are rejected automatically by `#[serde(deny_unknown_fields)]` on `RetryPolicy` — no explicit `ConfigError` variant needed.
- **Carry-forward warning for LATER tasks (not this one):** `clone_route_action` (`hcm.rs:240`, clones only `cluster` at `:249-250`) and `clone_route_config` (`hcm.rs:220`) MUST be updated (Task 4) to clone the new fields, or they are silently dropped on the `Arc<RouteConfiguration>` clone.
- **Verification:** `cargo test -p envoy-config retry_policy` (PASS) + `cargo clippy -p envoy-config --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check`.

_(Tasks 2–11 entries appended by the executor as each completes.)_

---

## Task 1 — `RetryPolicy` schema + `retry_policy` field + `include_attempt_count_in_response` (commit `3b0e23ecc`)

**Landed.** `crates/envoy-config/src/bootstrap.rs`: new `RetryPolicy` struct (`retry_on: String` +
`num_retries: Option<u32>` [L3 — default-1 resolution deferred to Task 2's `RetryConfig::from`] +
`retriable_status_codes: Vec<u32>`; `#[serde(deny_unknown_fields)]` rejects all deferred fields);
`RouteAction_Route` gains `retry_policy: Option<RetryPolicy>` (`Eq` derive dropped — `RetryPolicy`
is `PartialEq`-only, matching the `CircuitBreakers`/`Thresholds` house convention; verified no
consumer needs `RouteAction_Route: Eq`); `VirtualHost` gains
`include_attempt_count_in_response: bool` (`#[serde(default)]` → false; L6). 4 TDD serde tests
(parse-minimal / absent-yields-none / rejects-`per_try_timeout` / vhost-flag-true-and-absent).
`crates/envoy-config/src/lib.rs`: `RetryPolicy` re-export. **Workspace-compile fold-in (spec-review
finding):** the new required fields broke exhaustive struct literals downstream — fixed in the SAME
commit by FAITHFUL clones at `crates/envoy-http1/src/hcm.rs` `clone_route_config` (`:222`,
`include_attempt_count_in_response`) + `clone_route_action` (`:251`, `retry_policy`) (the PLAN-time
SPEC-correction CRITICAL deep-clone sites, discharged at Task 1 instead of Task 4) + inert defaults
(`false`/`None`) at the 16+8 H1 and 9+1 H2 `#[cfg(test)]` struct literals.

**Verification (quoted):**
- `cargo test -p envoy-config` → `test result: ok. 291 passed; 0 failed; 0 ignored` (287 + 4 new).
- `cargo test -p envoy-http1` → `test result: ok. 87 passed; 0 failed; 0 ignored`.
- `cargo test -p envoy-http2` → `test result: ok. 57 passed; 0 failed; 1 ignored`.
- `cargo build --workspace --all-targets` → `Finished` (clean).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean (exit 0).
- `cargo fmt --all -- --check` → clean (exit 0).
- `git show --stat HEAD` → 4 files changed (`bootstrap.rs` +98, `lib.rs` +10/-7, H1 `hcm.rs` +26, H2 `hcm.rs` +10).

**Two-stage review:** spec-compliance review surfaced the workspace break (Critical) → fixed +
re-verified; code-quality review **Approved** (zero Critical / zero Important; 2 Minor notes:
`Eq`-drop confirmed correct, deserialize-only tests match house style).
