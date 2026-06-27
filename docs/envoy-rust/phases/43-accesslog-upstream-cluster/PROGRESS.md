# Phase 43 — `43-accesslog-upstream-cluster` — PROGRESS

> **Lifecycle state 3 (implementation output).** Routed via `superpowers:subagent-driven-development`
> (fresh implementer subagent per task-group, controller-reviewed between; one commit per PLAN task, T5+T6
> merged). Implements the `%UPSTREAM_CLUSTER%` access-log command operator + the FIRST proxy (upstream-routed)
> access-log differential fixture, per the APPROVED `PLAN.md`. **Commit range `e36e709`..`3d4a3ad`** (on `0567220`).

## Summary

All PLAN tasks landed (TDD: failing test → verify-fail → minimal-impl → verify-pass → commit). The
`%UPSTREAM_CLUSTER%` operator renders the config `name` of the cluster a request was routed to — a new
`AccessLogRecord.upstream_cluster: Option<String>` field set by the HCM at the proxy-ARM entry (from
`BuildOutcome::Proxy { cluster }`), an exact `%UPSTREAM_HOST%` mirror. **The first proxy access-log fixture
(`0051`) opens the upstream-routed access-log surface** (every prior access-log fixture `0040`-`0050` is
`direct_response`-only), and witnesses on one live line `%UPSTREAM_CLUSTER%`=`backend` + (advancing **M42-1**)
`%RESPONSE_CODE_DETAILS%`=`via_upstream`. **NO new crate/dependency/fuzz-target/`ConfigError` variant; ONE new
`AccessLogRecord` field; ONE new `Op` variant; the proxy access-log fixture needed ZERO new harness code.**
`#![forbid(unsafe_code)]` holds in all touched crates.

## Per-task evidence

| Task | Commit | What landed | Test evidence |
|---|---|---|---|
| **T1** record field | `e36e709` | `pub upstream_cluster: Option<String>` on `AccessLogRecord` (after `upstream_host`); every workspace literal gets `upstream_cluster: None`. | `record` field test RED→GREEN; `cargo build --workspace --all-targets` green. |
| **T2** parse + text render | `cef08bb` | `Op::UpstreamCluster` + `"UPSTREAM_CLUSTER"` no-arg keyword (rejects `(...)` AND `:N` — the §6.2 strict-no-arg grammar) + `render_op` arm `…unwrap_or(empty_or_dash)`; the `encode_single_op` json arm also landed here (`encode_single_op` has no wildcard). | `upstream_cluster_parses_as_no_arg_op` / `…_rejects_arg` / `…_text_renders_value_or_dash` RED→GREEN. |
| **T3** json typed render | `6e26177` | The 3 json single-op tests (the arm landed in T2). | `…_single_op_present/absent/mixed` GREEN. |
| **T4** HCM plumbing (H1+H2) | `bb2fdb7` | Set `upstream_cluster` at the **proxy-ARM entry** (`hcm.rs:880`, from `BuildOutcome::Proxy { cluster: cluster_name }`), **NOT gated on upstream success** — Envoy renders `%UPSTREAM_CLUSTER%` whenever a cluster is selected, deliberately AVOIDING an M42-1-style gap; H2 via a new `upstream_cluster_for_log_h2` `finalize_h2_stream` param. | `hcm_h1_sets_upstream_cluster_from_routed_cluster` + `hcm_h2_…` (REAL routed in-process upstream tests via `spawn_in_process_upstream`/`spawn_upstream_h2_server`) RED→GREEN; the `None` direct_response case asserted too. |
| **T5+T6** proxy fixture `0051` | `3d4a3ad` | **The first proxy access-log fixture — needed ZERO new harness code (the cheap seam):** the `Http1EchoBackend` spawn in `run_fixture` is MARKER-DRIVEN (`scan_needs_marker(…, "HTTP1_BACKEND_PORT")`, before the `Driver` dispatch), so a `0051` fixture carrying the `0008`-style `{{BACKEND_HOST}}`/`{{HTTP1_BACKEND_PORT}}` STRICT_DNS cluster markers + the `http1_access_log_byte_exact` driver auto-spawns the backend + auto-templates + drives + scrapes via the EXISTING `Driver::Http1WithAccessLog` arm. + the differential test + a `parse_bootstrap` fuzz seed (`upstream_cluster.yaml` + `!`-un-ignore, `git ls-files`-confirmed) + the BEHAVIOR_CONTRACT row. | **The `0051` differential ran GREEN against live `envoyproxy/envoy:v1.33.0`** — both proxies emitted the byte-identical line `{"method":"GET","mixed":"c=backend","proto":"HTTP/1.1","rcd":"via_upstream","uc":"backend"}` (live-captured; `envoy-bin` rebuilt first; `%UPSTREAM_HOST%` EXCLUDED — structural per-side host mismatch). |

## Local verification (state-3 close; the AUTHORITATIVE §7.5 gate runs at state-4)

- `cargo build --workspace` — green.
- `cargo test -p envoy-accesslog` — **98 passed, 0 failed**.
- `cargo test -p envoy-http1` — **138 passed, 0 failed**.
- `cargo test -p envoy-http2` — **78 passed, 0 failed, 1 ignored** (the ignored = the documented `…h2_handshake…`
  host-flake; CI-authoritative; NOT a regression — phase 43 does not touch the H2 client handshake path).
- `cargo test -p differential --test access_log_upstream_cluster` — **GREEN** (Docker-gated; byte-exact
  cross-proxy match; ran green on this host rather than hitting the documented bridge-IP false-RED).
- `#![forbid(unsafe_code)]` intact in `envoy-accesslog`/`envoy-http1`/`envoy-http2`; NO `Cargo.toml`/`Cargo.lock`
  change.

## Notable outcomes
- **The §6.1-WHOLE decision + the cheap-seam prediction were both validated:** T5+T6 required ZERO new harness
  code (the marker-driven backend spawn auto-handled the proxy fixture), so the proxy access-log fixture was a
  single commit — far cheaper than the state-1 SPEC's "LIKELY split" projection.
- **M42-1 ADVANCED:** `%RESPONSE_CODE_DETAILS%`=`via_upstream` is now differentially witnessed on a real
  upstream-success path (fixture `0051`). The failure-path detail vocabulary (connect-error/reset/503) still
  needs failure-injection fixtures — M42-1 stays open for that.

## Deferred to the state-4 §7.5 verification gate (per project discipline)
`cargo fmt --check`, `cargo clippy`, `cargo deny check`, the full differential suite `0001`-`0051`
simultaneously, h2spec, and the fuzz run are the state-4 gate (a)-(e) — NOT run at state-3. The local `0051`
differential pass + the green per-crate tests are state-3 evidence only; the AUTHORITATIVE evidence is the
Linux CI run quoted at state-4.

## Carry-forwards (NONE blocks)
**M42-1** (ADVANCED — `via_upstream` now witnessed on the upstream-success path; the failure-path vocabulary
still owed) + M39-1/M39-2 + M38-1/M38-2 + CF-39-1 + M40-1 + M37-*/M36-*/M34-*/M33-* + older. Phase 43 does not
touch `rbac.rs`.

_State-3 implementation COMPLETE. The next session is the state-4 §7.5 verification gate
(`superpowers:verification-before-completion`)._
