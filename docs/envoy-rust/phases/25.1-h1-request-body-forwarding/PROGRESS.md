# Phase 25.1 — PROGRESS

> Running log for the `25.1-h1-request-body-forwarding` implementation arc
> (state-3, subagent-driven per `feedback_execution_style`). Append one entry
> per task on completion (the 06.2→24 cadence). PLAN: `PLAN.md`. SPEC: `SPEC.md`.
> Scope ADRs: ADR-0062 (parent), ADR-0063 (§6.2 reconciliation), ADR-0064 (split).

---

## Task-1 preamble (state-2 PLAN-write — the §6.2 verification transcript + the recon facts)

This preamble is written at the state-2 PLAN-write (the split commit), BEFORE any state-3 implementation. It preserves (a) the parent phase-25 §6.2 empirical verification transcript (ADR-0063 Provenance pointer) and (b) the H1-body-path recon facts the PLAN relies on. A state-3 implementer reads this FIRST.

### A. The §6.2 empirical verification (ADR-0063) — transcript

Ran LOCALLY against `envoyproxy/envoy:v1.33.0` (image id `56da5afd7df3`, the ADR-0058 digest) on macOS Docker 28.0.4 on 2026-06-13. Methodology: `project_docker_sidecar_backend_for_62_verify` (sidecar `mccutchen/go-httpbin:v2.15.0` upstream on a shared Docker network — `host.docker.internal` is IPv6-only on macOS Docker → 503s). Scratch configs were in `/tmp/buf62` (`validate-ok.yaml`, `validate-absent.yaml`, `validate-bogus.yaml`, `live.yaml`); the container is torn down.

1. **413 over-limit local reply (chain `max_request_bytes: 10`, POST 11 bytes):**
   ```
   HTTP/1.1 413 Payload Too Large
   content-length: 17
   content-type: text/plain
   date: …
   server: envoy
   connection: close

   Payload Too Large            ← body, 17 bytes, NO trailing newline
   ```
   `xxd`: `50 61 79 6c 6f 61 64 20 54 6f 6f 20 4c 61 72 67 65`. Byte-identical at a per-route `BufferPerRoute`-lowered limit (`/low`, max 4, POST 5 bytes → same 413 + same body).
2. **`Buffer` shape:** `@type type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer`; `max_request_bytes: 10` (plain int, `UInt32Value`) → `--mode validate` `OK`.
3. **`BufferPerRoute` shape:** `@type …buffer.v3.BufferPerRoute`; oneof `{ disabled: true }` AND `{ buffer: { max_request_bytes: 4 } }` → both `--mode validate` `OK`.
4. **Stats — NONE (the ADR-0063 trigger):** `/stats` after 8×2xx + 2×413 showed NO `http.<prefix>.buffer.*` counters. The 2 over-limit rejections appeared ONLY in `http.ingress_http.downstream_rq_too_large = 2` (+ `downstream_rq_4xx = 2`, `downstream_rq_response_before_rq_complete = 2`, `downstream_rq_rx_reset = 2`). → SPEC §2.1 buffer-stats row DROPPED (25.2 wires no buffer stats). Not a 25.1 concern.
5. **No-body GET → 200** passthrough.
6. **Boundary strictly `>`:** chain max 10 → 9 B 200 / 10 B 200 / 11 B 413; per-route max 4 → 3 B 200 / 4 B 200 / 5 B 413.
7. **Absent-filter:** Envoy ACCEPTS a `BufferPerRoute` per-route config whose `buffer` filter is absent from the chain (config `OK`, inert — `buffer` is compiled-in); the bogus-`@type` negative control DID fail hard (`could not find @type`). envoy-rust's generic `PerRouteConfigForAbsentFilter` validator rejects it; 25.2 reuses that verbatim (cors/csrf precedent). Not a 25.1 concern.
8. **Real-upstream constraint (ADR-0058 L6) — CONFIRMED:** within-limit POST (5 B < 10) reached the live upstream, body `hello` echoed in the reflected `data` field → 200. So a within-limit body must reach a REAL upstream to 200 → 25.2's fixture 0033 uses a real `http1-echo-server`. **This is the property 25.1 enables and 25.2 differentially proves.**

Residual (deferred to the 25.2 PLAN-write): the absent/`0`/malformed `max_request_bytes` disposition was not probed (valid limits 10 and 4 only).

### B. The H1-body-path recon (code-HEAD `9b0e7b925`) — what 25.1 changes

- `run_attempt(config, cluster, cluster_name, req: &Request, host_header, close)` `hcm.rs:315-321`; builds `out_req` `:349-357`; the always-empty `body: Some(Bytes::new())` `:356` → becomes `req.body.clone()` (Task 1 Step 5).
- Per-request handler: `body_len = parse_content_length(&req.headers)?` `:597`; `chunked` `:598`; `let mut req = req;` `:615`; boundary conversion `filter_req` with `body: req.body.take()` `:635`; write-back `:642`; head-advance + discard-drain `:676-697` → the body read is RELOCATED before `:631` and the discard-drain is removed (Task 1 Steps 4 + 6).
- `FilterRequest { …, body: Option<Bytes> }` `crates/envoy-filter/src/types.rs:28-35`.
- Regression key: `body_len == 0` for every existing fixture (bodyless or `content-length: 0`) → the relocated read is a body-wise no-op → all 32 fixtures stay green.

### C. State-machine position

This commit is the parent-25 state-2 PLAN-write + the §6.1 split (ADR-0064): it lands the parent SPEC's two sub-phase SPECs, this 25.1 `PLAN.md` + `PROGRESS.md`, ADR-0063 + ADR-0064, the ROADMAP split, and the STATE advance to `25.1` state-2-complete / state-3-next. Per §5.1 the NEXT session BEGINS state-3 (`superpowers:subagent-driven-development`) on Task 1.

---

## Task log

- [x] **Task 1** — H1 forwards the Content-Length request body upstream. **DONE** (code commit `20ce64010`).
  - **Implemented** (single file `crates/envoy-http1/src/hcm.rs`, +127/−26): (1) added an in-process **recording** upstream test helper returning `(port, Arc<Mutex<Vec<u8>>>)` that loop-reads with a 200 ms per-read timeout so a body arriving in a later TCP segment is still captured; (2) added the failing→passing test `h1_forwards_request_body_upstream` (multi-thread; drives an H1 `POST /submit` with `Content-Length: 11` body `hello world`, asserts the upstream stream `ends_with("hello world")` — the real forwarded bytes, not just a status); (3) relocated the Content-Length body read to BEFORE the filter-pipeline boundary conversion (head-advance `buf.advance(consumed)` once, then accumulate `body_len` bytes — in-buffer `from_buf` + socket tail — into a `BytesMut`, freeze → `req.body = Some(...)`; `body_len == 0` → `Bytes::new()` no-op); (4) forwarded `out_req.body = req.body.clone()` per attempt (replay-safe, ADR-0044) replacing `Some(Bytes::new())`; (5) removed the post-response discard-drain. The boundary conversion `req.body.take()` + write-back are unchanged; the access-log `request_body_len` (from `body_len`) is unchanged.
  - **Deviation (justified):** the helper is named `spawn_recording_upstream`, not the PLAN's `spawn_capturing_upstream` — a pre-existing `spawn_capturing_upstream` (`hcm.rs:2565`) already exists with an incompatible single-read `(u16, JoinHandle<Vec<u8>>)` signature that cannot satisfy the loop-capture contract. Body is verbatim to PLAN; the only change is the name + the one-token call site. Spec reviewer confirmed the collision.
  - **Tests:** `h1_forwards_request_body_upstream` FAILED pre-change (empty body at upstream), PASSES post-change; full `envoy-http1` suite **106 passed; 0 failed** (the `body_len == 0` no-op invariant holds for every existing test).
  - **Review:** spec-compliance ✅ (all 5 requirements present, no over/under-build, rename justified); code-quality **APPROVED** (0 Critical / 0 Important). Two **Minor** observations recorded, both out of this foundation-slice's locked scope: (a) `BytesMut::with_capacity(body_len)` eagerly allocates the client-claimed Content-Length up front — faithful to PLAN Step 4 and no Content-Length cap exists anywhere in `envoy-http1` today (the removed drain also read `body_len` bytes, in 4 KB chunks); a future Content-Length bound is a possible follow-up. (b) a chunked request carrying a bogus Content-Length now buffers those bytes into `req.body` before the `:694` 501 — behavior-preserving (the 501 path never forwards `req.body`), no regression.
- [x] **Task 2** — Regression invariants (chunked 501 / bodyless / keep-alive pipelining / retry replay). **DONE** (code commit `2fcf8fce`).
  - **Implemented** (test-module-only in `crates/envoy-http1/src/hcm.rs`, +220/−0; ZERO production lines): four REAL regression tests (no stub/comment fallback) + two test-only helpers. Tests — (1) `h1_chunked_request_still_501_after_body_forwarding` (chunked → no Content-Length → `body_len==0` → asserts downstream `HTTP/1.1 501 `); (2) `h1_bodyless_get_unchanged_after_body_forwarding` (bodyless GET proxies 200; asserts NO body bytes follow the head terminator upstream via `split("\r\n\r\n").nth(1).is_empty()`); (3) `h1_keep_alive_two_bodied_posts_do_not_bleed` (two pipelined POSTs `/one`+`aaa`, `/two`+`bbbb` on ONE TcpStream; asserts both bodies + both request lines reached the upstream — proves request 1's body did not bleed into request 2's parse, the buffer-advance correctness of the relocated read); (4) `h1_retried_post_replays_body` (fail-then-succeed backend 503→200, `num_retries: Some(1)` / `retry_on: "5xx"`; asserts `matches("replayme").count() == 2` AND `matches("POST /r ...").count() == 2` — the body appears on BOTH the failed attempt and the retry, genuinely proving per-attempt `req.body.clone()` replay).
  - **New test-only helpers (both justified — no prior equivalent):** `drive_keep_alive(config, requests) -> Vec<Vec<u8>>` (~`:1739`, mirrors the existing `drive` bind/serve/connect idiom; writes all requests on ONE connection, reads to EOF, splits responses on the `HTTP/1.1 ` status-line boundary; the trailing request carries `Connection: close`); `spawn_fail_then_ok_recording_upstream` (~`:2766`, fuses `spawn_recording_upstream`'s shared-buffer loop-record with an `AtomicUsize` status selector — connection 0 → 503, rest → 200). All four tests use `spawn_recording_upstream` (the Task-1 name), not the pre-existing `spawn_capturing_upstream`.
  - **Tests:** full `envoy-http1` suite **110 passed; 0 failed** (106→110, +4); each new test passes individually; clippy `-p envoy-http1 --tests` clean.
  - **Review:** spec-compliance ✅ (all 4 real tests present, test-only diff, Test 4 genuinely proves replay, Test 3 truly single-connection); code-quality **APPROVED** (0 Critical / 0 Important; reviewer walked the retry happen-before chain and confirmed no shared-buffer race, and verified 20/20 consecutive multi-thread runs green — no flake risk). Three **Minor** nits (non-blocking, deferred): a `debug_assert` that `drive_keep_alive`'s last request carries `Connection: close`; a comment that the status-line split assumes response bodies never contain `HTTP/1.1 `; ~10 lines of read-loop duplication between the two recording helpers.
- [x] **Task 3** — BEHAVIOR_CONTRACT note + isolated-crate & workspace verification. **DONE** (docs commit `2b97667de`; plus fixup `c98dafcf5`).
  - **Implemented:** added the `## Request body forwarding (HTTP/1.1)` note to `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (between the synth-503 section and `## Header allow-list`) — H1 now forwards Content-Length-delimited request bodies upstream (read into a `Bytes` before the pipeline, exposed as `FilterRequest.body`, cloned per attempt → replay-safe); closes the phase-04.3 gap; chunked/streaming remains a non-goal (501); H2 unchanged.
  - **State-3 verification gates — ALL GREEN:** `cargo build -p envoy-http1` (isolated-crate, per `project_isolated_crate_build_blindspot`) ✅; `cargo build --workspace --all-targets` ✅; `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅ (no warnings — clippy is NOT in per-task state-3 verification per `project_state3_arc_skips_clippy`, run here at Task 3); `cargo fmt --all -- --check` ✅ (after the fixup below); `cargo test --workspace` ✅ — the `envoy-http1` suite is 110/110; the only 2 failures were `differential::backend::tcp_proxy_backend_{spawns_and_echoes,drop_terminates_child}` missing a 1 s accept-ready budget under the loaded concurrent workspace run (the readiness-timeout flake family, `project_flaky_access_log_fixture_0012`) — both pass **2/2 standalone** (`cargo test -p differential tcp_proxy_backend`, 0.12 s), unrelated to this HTTP/1.1 change.
  - **fmt fixup (`c98dafcf5`, standalone, test-only):** `cargo fmt --check` flagged cosmetic rustfmt reflows in the Task 1-2 `#[cfg(test)]` additions to `crates/envoy-http1/src/hcm.rs` (per-task state-3 verification runs build/test but not fmt). Landed as its own transparent commit (purely cosmetic; no production/behavioral change) to keep this Task-3 docs commit clean.
  - **Note:** the Docker 32-fixture differential, `cargo deny check`, and the fuzz short-run are the state-4 gate (NEXT session, `superpowers:verification-before-completion`) — NOT run this state.

---

## State-3 close-out

All 3 tasks complete (code + PROGRESS commits each; plus one fmt fixup). Phase `25.1` is at **state-3-complete / state-4-next**. Commit chain at close: `20ce64010` (Task 1 code) → `60ed197ea` (Task 1 PROGRESS) → `2fcf8fcee` (Task 2 code) → `6fcb96e44` (Task 2 PROGRESS) → `c98dafcf5` (fmt fixup) → `2b97667de` (Task 3 docs) → this PROGRESS commit. Per §5.1 the NEXT session runs state-4 `superpowers:verification-before-completion` (the full §7.5 gate incl. the Docker 32-fixture differential LOCALLY per `feedback_state4_runs_docker_differential`, with the AUTHORITATIVE Linux CI anchor per ADR-0049; mind the flake family `project_flaky_access_log_fixture_0012` + the nested-cargo backstop flake). NO new fixture is added by `25.1`; the body-forwarding capability is differentially proven by `25.2`'s fixture `0033`.

---

## State-4 verification (§7.5 phase-done gate) — ALL GREEN

State-4 `superpowers:verification-before-completion` ran the full §7.5 phase-done gate at code-HEAD `ca023ce48` (the state-3 implementation chain is its ancestry — the load-bearing `hcm.rs` change landed in `20ce64010`). Evidence before assertions; every command was run fresh this session.

### (e) Static + workspace gates — GREEN (run LOCALLY at HEAD `ca023ce48`)

- `cargo fmt --all -- --check` → clean (no diff).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → **0 warnings** (`Finished` clean).
- `cargo build --workspace --all-targets` → ok.
- `cargo build -p envoy-http1` (isolated-crate, per `project_isolated_crate_build_blindspot`) → ok.
- `cargo deny check` → **`advisories ok, bans ok, licenses ok, sources ok`** (the `Zlib`/`Unicode-DFS-2016` lines are benign unmatched-license-allowance warnings, non-fatal — identical on CI).
- `cargo test -p envoy-http1` (the changed crate) → **110 passed; 0 failed** (incl. `h1_forwards_request_body_upstream` + the 4 Task-2 regression tests).
- `cargo test -p envoy-bin` (backstops standalone, per `project_workspace_test_nested_cargo_backstop_flake`) → emitted 11 passing `test result: ok` blocks (0 failed) then a backstop shelling nested `cargo run` hung past the ~2 s expectation — the known nested-cargo backstop flake, NOT a regression; envoy-bin is authoritatively GREEN under `cargo test --workspace` on Linux CI (run `27476243121`, below).

### (a)(b)(c) Differential + conformance — GREEN (run LOCALLY this session, Docker 28.0.4 + `envoyproxy/envoy:v1.33.0`)

- Pre-built `cargo test -p differential -p h2spec-conformance --no-run` first (exit 0); the workspace + helper binaries (`envoy-bin` + the 5 `tests/helpers/*` echo-servers) were already built by `cargo build --workspace --all-targets`, so the Docker run raced no cargo build (per `project_flaky_access_log_fixture_0012`).
- `cargo test -p differential -p h2spec-conformance` → **`DIFFERENTIAL_EXIT=0`**, **zero failures / zero panics**. The differential unit suite `136 passed; 0 failed; 2 ignored` (the 2 ignored are the Docker-requiring unit probes that run under `--workspace`; `tcp_proxy_backend_{spawns_and_echoes,drop_terminates_child}` PASSED this run — no readiness-timeout flake). The 32 pre-existing Docker-gated fixtures (`0001`–`0032`) ran as their integration-test binaries, **all `1 passed; 0 failed`** (the load-bearing regression-equivalence invariant: all 32 green SIMULTANEOUSLY — the relocated H1 body read is a body-wise no-op for every bodyless / `content-length: 0` fixture). **`h2spec_pass_rate_gate ... ok`** (h2spec conformance ≥95%).

### (d) Fuzz — covered (no new fuzz target this phase)

`25.1` introduces NO new parser/codec and NO new fuzz target (it forwards client-supplied bytes), so §7.5(d) requires no new short-run. The existing fuzzers (`parse_bootstrap` + `jwt_parse`, 30 s each) ran GREEN in the CI fuzz job `81215959916` at this HEAD.

### AUTHORITATIVE Linux CI anchor (ADR-0049 Provenance) — FRESH, GREEN

**Run `27476243121` at code-HEAD `ca023ce48`** (`gh run view 27476243121` → `completed success`, both jobs green: `build + test + lint` 3m41s + `fuzz` 2m5s). `ci.yml` runs the complete §7.5 gate on Linux — `fmt --check` + `clippy -D warnings` + `build --workspace --all-targets` + `cargo test --workspace` (which **includes** the Docker differential harness + `h2spec-conformance`) + `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`) + the two fuzzers. The differential genuinely engaged Docker (every fixture integration test — `http1_router_upstream`, `tcp_proxy`, `tls_downstream`, `http_filter_*`, … — `1 passed; 0 failed`); `envoy-http1` `110 passed`; whole job `0 failed`. **This supersedes the stale anchor `27457698815`** (which was at code-HEAD `9b0e7b925`, PREDATING the `25.1` `hcm.rs` change) as the project's differential evidence of record. The local differential above is corroborating; the Linux CI run is authoritative.

### §7.5 gate disposition

(a) no new/changed fixture → the body-forwarding capability is proven by the in-process `h1_forwards_request_body_upstream` test (differentially proven later by `25.2`'s `0033`); (b) all 32 pre-existing fixtures green simultaneously ✅ (local + CI); (c) h2spec ≥95% ✅; (d) no new fuzzer; existing fuzzers green ✅; (e) build/clippy/fmt/test/deny clean ✅; (f) `REVIEW.md` approved — that is state 5, the NEXT session. **States (a)–(e) are GREEN; the state-4 gate is COMPLETE.**

Per §5.1 (one state per session) this session advances `STATE.md` → state-4-complete / state-5-next (next expected skill `superpowers:requesting-code-review`) and EXITS. It does NOT begin the state-5 code review.
