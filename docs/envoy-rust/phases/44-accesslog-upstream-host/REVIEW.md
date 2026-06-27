# Phase 44 — `44-accesslog-upstream-host` — STATE-5 Code Review

**Reviewer:** fresh `superpowers:code-reviewer` subagent (no session history), dispatched over the phase-44 diff `git diff 9100feb^..b8d6ea8` (6 files, +321/-1) against `SPEC.md` / `PLAN.md` / `PROGRESS.md` / project doctrine, plus the precedents `0051` / `0036` / `access_log_upstream_cluster.rs`.

## Overall verdict: ✅ APPROVED

A textbook FIXTURE-ONLY witness phase. The diff is exactly what the SPEC and PLAN promised (+321/-1, 6 files, **zero `src/` change**), every load-bearing invariant holds, and the §7.5 gate is CI-proven (run `28304511513`, `151 passed; 0 failed` + every fixture binary green).

## Findings by severity

- **Critical:** none
- **Important:** none
- **Minor:** none

The reviewer deliberately did not manufacture issues. Two correct-by-design, explicitly-documented items considered and dismissed (NOT findings):
- The `json_format` keys are authored `uh, uc, rcd, method, proto` in YAML, but the render path sorts by UTF-8 byte order at emit time (`method, proto, rcd, uc, uh`), as the README + `expectations.yaml` state. YAML key order is irrelevant. Correct.
- `lb_policy: ROUND_ROBIN` on a single-endpoint STATIC cluster is a no-op, but matches the precedent and is harmless. Correct.

## What was verified (confidence: high)

1. **Cluster** (`envoy.yaml:57-75`, `envoy-rust.yaml:37-47`): `type: STATIC`, a SINGLE endpoint at `{{BACKEND_IP}}:{{HTTP1_BACKEND_PORT}}` — NOT STRICT_DNS/`{{BACKEND_HOST}}` (the `0051` per-side-split trap), NOT `0036`'s two-backend `_1_PORT`/`_2_PORT` markers. The load-bearing correctness point — right.
2. **Per-side deltas**: a YAML diff shows EXACTLY the benign documented set (drop `admin`; listener `0.0.0.0`→`127.0.0.1`; `/tmp/0052-envoy-mount` vs `/tmp/0052-envoy-rust-mount`; drop `generate_request_id: false` + `request_headers_to_remove`). The **cluster stanzas are byte-identical** across sides; nothing in the delta perturbs the asserted access-log line. The byte-exact claim is sound.
3. **json_format**: both sides log `%UPSTREAM_HOST%` plus only deterministic cross-proxy anchors (`%UPSTREAM_CLUSTER%`, `%RESPONSE_CODE_DETAILS%`, `%REQ(:METHOD)%`, `%PROTOCOL%`) — no timestamp/duration/request-id/bytes operator that would break equality.
4. **expectations.yaml**: `kind: http1_access_log_byte_exact`, correct `/tmp/0052-*` paths, one sensible `GET /` probe, NO hard-coded ip:port literal (pure cross-proxy equality).
5. **Differential test** `access_log_upstream_host.rs`: faithful clone of the `0051` template — same Docker-gated `run_fixture` shape, correct fixture dir + fn name.
6. **BEHAVIOR_CONTRACT row** (`:1029`): accurate (differentially-witnessed-by-0052; `SocketAddr::to_string()` = `<ip>:<port>` IPv4-unbracketed; direct_response `-` note preserved).
7. **Doctrine**: `git diff --name-only` confirms NO `src/` file touched; no new `Op`/`AccessLogRecord` field/crate/dependency/fuzz-target/`ConfigError` variant; `#![forbid(unsafe_code)]` untouched. The fuzz-seed SKIP is justified (`json_format_logger.yaml` already carries `%UPSTREAM_HOST%`, plan-review M1). The no-`src/`-change invariant guarantees `0001`-`0051` stay byte-identical — CI run `28304511513` empirically proves it.

## Disposition

No Critical/Important findings to fold; no Minor findings to carry forward. Phase 44 is APPROVED for state-6 close-out (flip ROADMAP row `44` → `done`). The open carry-forward Minors from prior phases (M42-1, M39-*, M38-*, CF-39-1, M40-1, …) are untouched by this phase and stay live.
