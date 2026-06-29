# Phase 51 — `51-accesslog-rf-retry-exhausted` Implementation Progress

> State-3 running log (`superpowers:executing-plans` + `superpowers:test-driven-development`,
> TDD per task — RED→GREEN→commit, inline in one session because the three tasks
> edit overlapping files [`hcm.rs` in T1/T3] and are strictly sequential). Append-only
> per-task entries. The full §7.5 verification gate (Docker differential `0059`
> green + all `0001`-`0058` byte-identical + h2spec + fuzz + deny, quoted in) runs
> at the state-4 session AFTER this one
> (`superpowers:verification-before-completion`); the Docker differential is
> CI-authoritative there (memory `envoy-rust-state4-ci-first-execution`).

**Goal:** Witness the FOURTH non-`-` `%RESPONSE_FLAGS%` value — `URX`
(UpstreamRetryLimitExceeded) — BYTE-EXACT on the H1 retry-limit-exceeded 503
path (`retry_policy:{retry_on:"5xx", num_retries:N}`, the ADR-0045 L9 path
returning the last upstream response verbatim), by (§A) declaring a
`retry_limit_exceeded_for_log` boolean and setting it `true` at envoy-rust's
retry-loop limit-exceeded exit (the same gate as
`upstream_rq_retry_limit_exceeded`), and (§B) prepending a boolean branch to the
phase-48/49/50 `%RESPONSE_FLAGS%` derive that renders `"URX"`. **The make-or-break
finding (recon-locked, ADR-0108):** `URX`'s `%RESPONSE_CODE_DETAILS%` is the
SHARED `via_upstream` (a real upstream 503, already matching Envoy) → NO rcd
change, and the flag is the FIRST not 1:1 with a unique rcd → it needs a
SEPARATE boolean discriminator. Additive → all `0001`-`0058` byte-identical.

**Scope lock:** ADR-0108. NO new `Op` / `AccessLogRecord` field / crate /
dependency / fuzz-target / `ConfigError` variant. NO `%RESPONSE_CODE_DETAILS%`
change (already matches Envoy). `#![forbid(unsafe_code)]` holds. H1-only (H2
deferred — M45-1).

**Live line numbers re-verified against disk before editing:** plan-write
citations were ACCURATE (no drift this phase). Anchors used: §A decl after
`response_code_details_for_log` (`hcm.rs:844`); §A set co-located with
`cluster.upstream_rq_retry_limit_exceeded().inc()` inside the `if final_retriable`
arm of the post-loop split (`hcm.rs:1126-1132`); §B derive wrapper at the
record-build site (the `response_flags:` field, `hcm.rs:1274` match). The new
backstop test was inserted immediately after the phase-50 model test
`h1_request_budget_overflow_access_log_carries_uo_flag`.

---

## Task 1 — §A boolean + §B derive wrapper (RED→GREEN) — ✅ DONE

- **RED:** Added `#[tokio::test(flavor = "multi_thread")]` backstop
  `h1_retry_limit_exceeded_access_log_carries_urx_flag` to
  `crates/envoy-http1/src/hcm.rs` `mod tests` (always-503 backend
  `spawn_fail_then_ok_upstream(503, 1000)` + `cluster_mgr_with_endpoint("backend",
  port)` + inline `HCMConfig` with a `/`-prefix route carrying
  `retry_policy{retry_on:"5xx", num_retries:1}` + a `{rc,rcd,rf}` `json_format`
  FileSink, `GET /` probe). Pre-change the final `assert_eq!` failed exactly as
  predicted (`cargo test -p envoy-http1
  h1_retry_limit_exceeded_access_log_carries_urx_flag`):
  - `left:  "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"-\"}\n"`
  - `right: "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"URX\"}\n"`
  (the `HTTP/1.1 503` status assert passed first → the harness wiring is correct;
  the failing point is the `rf` derive, confirming the rcd-match falls to
  `_ => "-"` because `via_upstream` is unmatched.)
- **GREEN — §A decl:** after `let mut response_code_details_for_log: Option<String>
  = None;` (`hcm.rs:844`), declared `let mut retry_limit_exceeded_for_log = false;`
  in the OUTER fn scope (visible at both the set-site and the derive), with a
  comment explaining it is the FIRST flag not 1:1 with a unique rcd.
- **GREEN — §A set:** inside the post-loop split's `if final_retriable` arm
  (`hcm.rs:1128`, co-located with `cluster.upstream_rq_retry_limit_exceeded().inc()`),
  set `retry_limit_exceeded_for_log = true;`. EXCLUDED (documented in-comment): the
  retry-BUDGET-blocked exit (gated out by `!retry_budget_blocked`) and the pre-loop
  request-budget overflow (`:932`, bypasses the loop → renders `UO`).
- **GREEN — §B derive:** wrapped the `response_flags:` derive (`hcm.rs:1274`) as
  `if retry_limit_exceeded_for_log { "URX" } else { <unchanged NR/UH/UO/`-` match> }
  .to_owned()`. The boolean is `Copy` (no borrow/move interaction with the rcd
  `String` moved into `response_code_details:` below); the `NR`/`UH`/`UO` arms are
  unreachable with it set (set only on the `via_upstream` L9 path) → byte-identical
  to phase 50.
- **Verify GREEN:** `cargo test -p envoy-http1 access_log_carries` → 9 passed (the
  new `…urx_flag` + the unchanged `…uo_flag`/`…uh_flag`/NR backstops). Full crate:
  `cargo test -p envoy-http1` → **147 passed; 0 failed** (the retry-counter tests
  `retry_limit_exceeded_path_always_503`/`retry_success_path_503_then_200`/budget
  tests byte-unaffected — the boolean defaults `false` on every non-L9 path).
- **Commit:** `88725cf` — "phase 51 T1: URX on the retry-limit-exceeded path — §A
  boolean + §B derive wrapper [ADR-0108]".

## Task 2 — fixture `0059` + differential test + harness backend wiring — ✅ DONE

- **Harness wiring (`tests/differential/src/lib.rs`, 2 fixture-name-gated edits):**
  (i) added `0059-accesslog-rf-retry-exhausted` to the `needs_health_aware_backend`
  allowlist; (ii) added a `/retry-exhausted=503` `--per-path` arm for `0059` in the
  SECOND `per_path` rebind. `retry_script` stays `None` for `0059` → the backend
  spawns via `spawn_with_retry_script(None, Some("/retry-exhausted=503"))` — a
  STATELESS always-503 path (both attempts 503).
- **Fixture `0059-accesslog-rf-retry-exhausted/` (4 files):** `envoy-rust.yaml`
  (binds `127.0.0.1`, no admin block; STRICT_DNS `{{BACKEND_HOST}}`/`{{BACKEND_PORT}}`
  `backend` cluster; a single `/retry-exhausted` route with
  `retry_policy{retry_on:"5xx",num_retries:1}`; a `{rc,rcd,rf}` `json_format`
  FileSink); `envoy.yaml` (binds `0.0.0.0`; **a LITERAL admin `port_value: 0`, NOT
  `{{ADMIN_PORT}}` — plan-review C1**, cloned from the same-driver
  `0058/envoy.yaml`); `expectations.yaml` (`http1_access_log_byte_exact` driver,
  one `GET /retry-exhausted` probe, `expected_status: 503`); `README.md`.
- **Differential test:** `tests/differential/tests/access_log_rf_retry_exhausted.rs`
  — structural clone of `access_log_rf_overflow.rs` → `0059`.
- **Verify:** `cargo build -p envoy-bin` (rebuilt the DEBUG binary the differential
  runs — memory `differential-harness-uses-debug-envoy-bin`) + `cargo build -p
  differential --tests` → clean. **`cargo test -p differential --test
  access_log_rf_retry_exhausted` → PASS** (Docker available locally; both Envoy
  v1.33.0 and envoy-rust emitted byte-identical
  `{"rc":503,"rcd":"via_upstream","rf":"URX"}`). NOTE: this local pass is a strong
  confidence signal, but the Docker differential remains CI-authoritative at the
  state-4 §7.5 gate (memory `envoy-rust-state4-ci-first-execution`).
- **Commit:** `25c656d` — "phase 51 T2: fixture 0059-accesslog-rf-retry-exhausted +
  differential test + health-aware backend wiring [ADR-0108]".

## Task 3 — §E BEHAVIOR_CONTRACT updates — ✅ DONE

- **`%RESPONSE_FLAGS%` row (`:1020`):** (a) added the `URX` per-flag equivalence
  rule — a config-deterministic single constant **NOT derived from
  `%RESPONSE_CODE_DETAILS%`** (the shared `via_upstream`), instead from the
  `retry_limit_exceeded_for_log` boolean set at the `upstream_rq_retry_limit_exceeded`
  loop-exit (cited by SYMBOL, not a raw line); (b) removed `URX` from the
  "Other non-`-` flags (`UF`/`DC`)" unwitnessed list; (c) added `+ URX
  retry-limit-exceeded case` to the value-exact parenthetical; (d) **fixed the stale
  "two witnessed failure paths" opening** (plan-review M2) → now "four witnessed
  failure paths" (`NR`/`UH`/`UO`/`URX`); (e) added the fixture-`0059` witnessing
  sentence.
- **Retry-limit-exceeded wire-shape note (`:389`):** appended the cross-link that
  fixture `0059` now witnesses `%RESPONSE_FLAGS% = URX` byte-exact, derived from the
  boolean (NOT the rcd).
- **Verify:** `cargo test -p envoy-http1` (147 passed) + `cargo test -p
  envoy-accesslog` (98 passed) — prose-only edits, no test references the changed
  lines; the `:1274` derive's `NR`/`UH`/`UO`/`-` arms and the `0056`/`0057`/`0058`
  expectations are untouched.
- **Commit:** `800c756` — "phase 51 T3: BEHAVIOR_CONTRACT — URX witnessed
  (boolean-derived, rcd unchanged) [ADR-0108]".

---

## Carry-forward consumption (record at state-3 close)

- **CONSUMED by phase 51:** the `URX` slice of **M45-2** (leaving the connect-failure
  `UF`/`DC` + the retry-BUDGET-overflow slices live).
- **Still live (NONE blocks):** M50-C (request-budget `max_requests` overflow UO/rcd
  — re-recon the rcd string before witnessing), M48-2, M42-1, M45-1 (H2 failure-path
  details + an H2 access-log differential driver), the connect-failure + retry-budget-
  overflow slices of M45-2, M40-1, M39-*, M38-*, CF-39-1, M37-*, M36-*, M34-*, M33-*,
  the empty-`metadata_match` doc-comment, M29-*, M30-*, the phase-31 cosmetics, the
  HTTP-filters-family (1)-(4).

## State-3 exit (this session)

The state-3 implementation is COMPLETE: all 3 PLAN tasks landed (RED→GREEN→commit
each), `PROGRESS.md` written, `STATE.md` advanced to state-4-next. The state-4
verification (`superpowers:verification-before-completion`, the full §7.5 gate) is
the SESSION AFTER. §5.1: ONE state per session. Push per-session + confirm CI (this
push touches `src/` + fixtures → CI runs cargo-fmt-check + the Docker differential
for the FIRST time at this push).

---

## State-4 verification (the §7.5 phase-done gate — `superpowers:verification-before-completion`)

> Separate session AFTER state-3. Ran the FULL BOOTSTRAP_PROMPT.md §7.5 gate.
> Local workspace gate (fmt/build/clippy/deny/test) + the **CI-authoritative**
> Docker differential / h2spec / fuzz (memory `envoy-rust-state4-ci-first-execution`).
> All outputs quoted verbatim below.

### (e) Local workspace gate

**`cargo fmt --all -- --check`** → **EXIT 0** (clean, no diff).

**`cargo deny check`** → **EXIT 0**: `advisories ok, bans ok, licenses ok, sources ok`
(only benign `license-not-encountered` warnings for unmatched allowances `0BSD` /
`BSD-2-Clause` / `MPL-2.0` / `Unicode-DFS-2016` / `Zlib` — informational, not errors).

**`cargo build --workspace --all-targets`** → **EXIT 0**:
`Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 12.06s`
(rebuilt the DEBUG `envoy-bin` the differential runs — memory
`differential-harness-uses-debug-envoy-bin`).

**`cargo clippy --workspace --all-targets --all-features -- -D warnings`** → **EXIT 0**:
`Finished \`dev\` profile … in 2.59s` (zero warnings → `-D warnings` satisfied).

**`cargo test --workspace`** → all crates GREEN **except two DOCUMENTED host-flakes**
(CI-authoritative — both GREEN on CI, see below):
- `envoy-http1` **147 passed; 0 failed** (the §F backstop
  `h1_retry_limit_exceeded_access_log_carries_urx_flag` GREEN).
- `envoy-accesslog` **98 passed; 0 failed**.
- `differential` lib **151 passed; 0 failed; 2 ignored**; the phase-51 surface test
  **`tests/access_log_rf_retry_exhausted.rs` (fixture `0059`) → 1 passed** (both
  proxies byte-identical `{"rc":503,"rcd":"via_upstream","rf":"URX"}` — Docker
  available locally) AND every other `access_log_rf_*` differential green.
- ALL other workspace crates (envoy-config 533, envoy-cluster 208, envoy-http2 lib
  77, …) **0 failed**.
- **Host-flake #1** — `differential --test admin_config_dump_server_info`:
  `envoy-only: ["backend::192.168.65.2:39625::…"]` — the documented bridge-IP route
  (memory `differential-host-bridge-ip-192-168-65-2`: this host routes the backend
  via `192.168.65.2`, NOT the allow-listed `192.168.65.254`/`172.17.0.1`); NOT a
  phase-51 regression (phase 51 touches no admin/config-dump surface). GREEN on CI.
- **Host-flake #2** — `envoy-http2 --lib
  client::tests::send_request_maps_h2_handshake_failure_to_typed_error`
  (`client.rs:551`): the documented h2-handshake host-flake (memory
  `envoyrust-h2-handshake-test-host-flake`: the handshake unexpectedly succeeds on
  this host's networking); pre-existing, NOT a phase-51 regression (no HTTP/2 change
  this phase). GREEN on CI.

### (a)/(b)/(c)/(d) CI-authoritative gate (Docker differential + h2spec + fuzz)

The state-3 work was already pushed; CI is the authority for the Docker differential,
h2spec, and fuzz (memory `envoy-rust-state4-ci-first-execution`). `gh run list`:

- HEAD **`2c46cd5`** (`deps: bump anyhow 1.0.102 → 1.0.103 to clear RUSTSEC-2026-0190`)
  → **CI SUCCESS** (5m42s): `✓ build + test + lint in 5m28s` (fmt ✓ clippy ✓ build ✓
  install h2spec ✓ **test [includes differential harness → Docker] ✓** cargo deny ✓)
  + `✓ fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse,
  30s each) in 4m3s`. This is the **CI-authoritative GREEN** for gate (a) the `0059`
  Docker differential, (b) `0001`-`0058` byte-identical, (c) h2spec ≥95% (NO HTTP/2
  codec change → unaffected), (d) the `parse_bootstrap`/`accesslog_format_parse`
  fuzz short-budget runs, AND (e) the full workspace build/clippy/fmt/test/deny.
- The intermediate state-3 commit **`9085e39`** showed CI `failure` — job breakdown:
  fmt ✓ clippy ✓ build ✓ install h2spec ✓ **test (includes differential harness →
  Docker) ✓** fuzz ✓ — the ONLY red was **`X cargo deny check`**, the freshly-published
  **RUSTSEC-2026-0190** advisory against `anyhow 1.0.102` (memory
  `cargo-deny-reds-on-unrelated-advisory` — a per-session push reds CI on an unrelated
  advisory; patch-bump the dep, NOT a phase regression). Cleared by the anyhow
  1.0.102→1.0.103 bump at HEAD `2c46cd5`. So gate items (a)/(b)/(c)/(d) ran **GREEN at
  `9085e39` already** and (e) deny is GREEN at HEAD.

### (f) REVIEW.md

State-5 output — the SESSION AFTER this one (`superpowers:requesting-code-review`).
NOT produced this session. §5.1: one state per session.

### Gate verdict

**§7.5 phase-done gate CLEAN.** Local fmt/build/clippy/deny EXIT 0; all
non-host-flake workspace tests GREEN (envoy-http1 147 incl. the URX backstop,
envoy-accesslog 98, the `0059` differential PASS); the 2 local failures are
DOCUMENTED CI-authoritative host-flakes (both GREEN on CI). CI HEAD `2c46cd5` fully
GREEN (Docker differential incl. `0059` + `0001`-`0058`, h2spec, fuzz, deny).
→ **Advance STATE to state-5-next** (`superpowers:requesting-code-review`).
