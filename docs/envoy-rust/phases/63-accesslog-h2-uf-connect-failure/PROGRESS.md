# Phase 63 — `63-accesslog-h2-uf-connect-failure` — PROGRESS

> State-3 implementation log. Records what landed per task, the exact files
> touched, and the local command outputs (Tasks 1/2/6). The state-4
> verification (the full §7.5 gate, run in CI) is a separate future session.

## Task 1 — H2 connect-failure synth status 502→503 (commit `3c6ef1d`)

Flipped the 2 existing 502-asserting tests to 503 (RED), declared
`synth_h2_connect_failure()` (adjacent to `synth_h2_502()`, `hcm.rs`), redirected
the SOLE `AcquireOutcome::ConnectFailure` match arm to call it, and fixed the 3
stale "returning 502" `tracing::warn!` strings (GREEN).

**File:** `crates/envoy-http2/src/hcm.rs`

RED (`cargo test -p envoy-http2 h2_connect_failure -- --nocapture`):
```
assertion `left == right` failed: downstream must be connect-failure synth-503
  left: 502
 right: 503
test result: FAILED. 0 passed; 2 failed; ...
```

GREEN (same command, post-fix):
```
test hcm::tests::h2_connect_failure_synth_does_not_tick_upstream_rq_5xx ... ok
test hcm::tests::h2_connect_failure_retried_on_connect_failure_policy ... ok
test result: ok. 2 passed; 0 failed; ...
```

Full `envoy-http2` suite (collateral check): `cargo test -p envoy-http2` →
**85 passed; 0 failed; 1 ignored.**

## Task 2 — thread `connect_failure_for_log_h2` + render `UF` + backstop (commit `2861a6e`)

Declared the discriminator boolean + the loop-scoped `final_outcome_h2` capture,
captured it every retry-loop iteration, set the boolean post-loop from the
final attempt's outcome, threaded it through `finalize_h2_stream`'s signature
and its sole call site, extended the `%RESPONSE_FLAGS%` derive with an
`else if connect_failure_for_log_h2 { "UF" }` branch ordered after `URX`, and
added a new in-process backstop `h2_connect_failure_access_log_carries_uf_flag`.

**File:** `crates/envoy-http2/src/hcm.rs`

RED (`cargo test -p envoy-http2 h2_connect_failure_access_log_carries_uf_flag -- --nocapture`):
```
assertion `left == right` failed: connect-failure access-log line carries rf:UF: "{\"rc\":503,\"rf\":\"-\"}\n"
  left: "{\"rc\":503,\"rf\":\"-\"}\n"
 right: "{\"rc\":503,\"rf\":\"UF\"}\n"
test result: FAILED. 0 passed; 1 failed; ...
```

GREEN (same command, post-fix): `test result: ok. 1 passed; 0 failed; ...`

Full `envoy-http2` suite (collateral check on NR/UH/UO/URX backstops):
`cargo test -p envoy-http2` → **86 passed; 0 failed; 1 ignored** (the new
backstop adds one to Task 1's 85). `h2_route_miss_access_log_carries_nr_flag`,
`h2_host_miss_access_log_carries_nr_flag`, `h2_no_healthy_access_log_carries_uh_flag`,
`h2_pool_overflow_access_log_carries_uo_flag`,
`h2_request_budget_overflow_access_log_carries_uo_flag`, and
`h2_retry_limit_exceeded_access_log_carries_urx_flag` all unchanged.

## Task 3 — fixture `0068-accesslog-h2-uf-connect-failure` (commit `3c46492`)

Created the 4-file fixture (`envoy.yaml`/`envoy-rust.yaml`/`expectations.yaml`/
`README.md`) — the `0066` H2C/H2-upstream shape minus `circuit_breakers`
(mirrors the `0060`-minus-`0058` H1 delta). YAML pair diff: the admin line,
bind-address, and log-path hunks, plus one comment-only hunk on the cluster
(the SAME 4-hunk shape fixture `0066` itself exhibits — 3 substantive + 1
cosmetic comment block).

## Task 4 — differential test `access_log_h2_uf_connect_failure.rs` (commit `84a4df7`)

Thin `differential::run_fixture` wrapper, structural clone of
`access_log_h2_rf_overflow.rs`. `cargo build -p differential --tests` — clean.
Rebuilt debug `envoy-bin` (`cargo build -p envoy-bin`) — clean.

**Ran locally (this host's Docker bridge worked for this run):** the new
`access_log_h2_uf_connect_failure` differential test PASSED —
`test access_log_h2_uf_connect_failure ... ok` (real Envoy v1.33.0 vs
envoy-rust, byte-exact `{"method":"GET","proto":"HTTP/2","rc":503,"rf":"UF"}`
on both sides). Per memory `envoy-rust-state4-ci-first-execution`, the
authoritative green/red verdict is still deferred to the state-4 CI gate; this
local pass is a bonus confirmation, not a substitute.

## Task 5 — BEHAVIOR_CONTRACT.md updates (commit `9683782`)

Updated the `%RESPONSE_FLAGS%` row's H2-witness sentence (now records `UF`
witnessed on H2 by fixture `0068`, ADR-0120, leaving only `UC` open in
carry-forward M56-1) and the `%RESPONSE_CODE_DETAILS%` row's H2-witness
sentence (records the H2 connect-failure rcd stays the shared `via_upstream`
+ non-deterministic transport-failure reason, `rf`=`UF` is the discriminating
signal, rcd omitted from the fixture).

## Task 6 — local verification sweep

- `cargo build --workspace --all-targets` → clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` →
  clean (the `#[allow(unused_assignments)]` on `final_outcome_h2` suppresses
  the expected lint, mirroring `final_retriable`).
- `cargo fmt --all -- --check` → one reflow needed (the `final_outcome_h2`
  type declaration line-wrapped); `cargo fmt --all` applied and committed
  separately (commit `c3a1ab7`, per memory
  `envoy-rust-state4-ci-first-execution`: CI is often red-at-fmt mid-phase).
- `cargo test --workspace --no-fail-fast` → 8 pre-existing target failures,
  ALL matching documented host-flake memory entries, NONE related to phase
  63's change:
  - `access_log_file_sink`, `access_log_json_format`, `admin_drain_listeners`,
    `admin_ready` — "upstream Envoy never became accept-ready" (Docker
    accept-ready flake on this host, memory `envoy-rust-state4-ci-first-execution`
    / `differential-fixtures-flake-under-parallel-load`).
  - `access_log_rcd_upstream_reset`, `access_log_rf_upstream_reset` — the
    documented `TcpCloseBackend IPv6-unreachable-host-flake` (fixtures
    0061/0062; real Envoy resolves the accept-then-close backend to an
    unreachable IPv6 address on this host, reporting `UF`/connect-failure
    instead of the intended reset/`UC`).
  - `admin_config_dump_server_info` — the documented
    `differential-host-bridge-ip-192-168-65-2` divergence (this host routes
    the backend via `192.168.65.2`, not the allow-listed bridge IP).
  - `envoy-http2 --lib` (`client::tests::send_request_maps_h2_handshake_failure_to_typed_error`)
    — the documented `envoyrust-h2-handshake-test-host-flake` (non-deterministic
    on this host); re-ran `cargo test -p envoy-http2` alone afterward and it
    PASSED (86/86), confirming the flake.
  - **This phase's own new artifacts** — the 2 flipped Task-1 tests, the new
    Task-2 backstop, AND the new differential test `access_log_h2_uf_connect_failure`
    — all GREEN, every run.
- `cargo deny check` → clean (`advisories ok, bans ok, licenses ok, sources ok`;
  5 pre-existing `license-not-encountered` informational warnings, not errors).
- Byte-preservation re-grep (`for f in 0009 0010 0018 0021 0064 0065 0066 0067`)
  → matches the PLAN §3 item 2 derivation exactly: `0021`'s `circuit_breakers`
  (headroom, reachable backend), `0065`'s `127.0.0.1:1` (comment-only, excluded
  pre-dial), `0066`'s `circuit_breakers` (pending-gate), `0067`'s
  `retry_policy` (real always-503 upstream) — NONE reaches
  `AcquireOutcome::ConnectFailure`; `0001`-`0067` stay byte-identical.

## ADR-0121

Stayed reserved-but-unfired. The §3 PLAN-VERIFY re-confirmation (done at
PLAN-write) confirmed all SPEC §A-§M facts held against the live tree; no
§6.2 reconciliation fired during implementation either — every replace-block
in PLAN.md matched the live tree's surrounding context exactly at edit time.

## Docker differential — deferred to state-4 CI (per PLAN, host limitation)

Per memory `envoy-rust-state4-ci-first-execution` /
`differential-host-bridge-ip-192-168-65-2`, this dev host's Docker
differential runs are not authoritative pass/fail gates — even though the new
`0068` fixture's differential test happened to pass locally this run
(reported above), the AUTHORITATIVE verdict for fixture `0068` green + all
`0001`-`0067` simultaneously green + h2spec ≥95% is the state-4 CI run, not
this session's local run.

## Commits this session (state-3 implementation)

```
3c6ef1d phase 63 task 1: H2 connect-failure synth status 502->503 (new helper + redirect + 2 tests + 3 warns) [ADR-0120]
2861a6e phase 63 task 2: thread connect_failure_for_log_h2 + render UF + in-process backstop [ADR-0120]
3c46492 phase 63 task 3: fixture 0068-accesslog-h2-uf-connect-failure (one probe, H2 rf:UF byte-exact) [ADR-0120]
84a4df7 phase 63 task 4: differential test access_log_h2_uf_connect_failure (fixture 0068) [ADR-0120]
9683782 phase 63 task 5: BEHAVIOR_CONTRACT rf/rcd rows — H2 UF witnessed (fixture 0068) [ADR-0120]
c3a1ab7 phase 63: cargo fmt [ADR-0120]
```

## Next: state-4 verification

The full §7.5 gate (Docker differential authoritative run, h2spec, fuzz —
none new this phase, build/clippy/fmt/test/deny in CI) via
`superpowers:verification-before-completion`, in a separate future session,
per §5.1 (one state per session).
