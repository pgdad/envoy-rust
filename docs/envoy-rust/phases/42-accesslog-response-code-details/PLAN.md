# Phase 42 — `42-accesslog-response-code-details` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this plan task-by-task. Steps use `- [ ]` checkboxes. TDD per `superpowers:test-driven-development` on every task.

**Goal:** Add the `%RESPONSE_CODE_DETAILS%` access-log command operator — the response-code-details string Envoy attaches to the reply — byte-equivalent to upstream Envoy v1.33.0.

**Architecture:** A new `AccessLogRecord.response_code_details: Option<String>` field (set by the HCM per response-path), a new `Op::ResponseCodeDetails` (a strict no-arg `"RESPONSE_CODE_DETAILS"` keyword), its `render_op` arm, and its `encode_single_op` arm — all mirroring the EXISTING `%ROUTE_NAME%`/`%UPSTREAM_HOST%` (`Option<String>` → `render_op` `unwrap_or("-")`, `encode_single_op` `quote_opt`). The HCM tags the detail at the point the response-path is known: the `direct_response` synth arm → `Some("direct_response")`, the proxy-success arm → `Some("via_upstream")`, all other arms (error synths, filter-synths) → `None` (deferred vocabulary). Config/flow-deterministic, byte-exact.

**Tech Stack:** Rust workspace; the hand-rolled `envoy-accesslog` command-operator engine; the shared `envoy_http1::build_response`/`BuildOutcome` (reused by both H1 and H2 HCMs); the `testcontainers` differential harness.

**§6.2 LOCKED FACTS (recon ran THIS state-2 against live `envoyproxy/envoy:v1.33.0`; the state-1 existence-check already locked §A/§B of ADR-0099):**
- **Strict no-arg grammar:** `%RESPONSE_CODE_DETAILS:3%` boot-fatals `error initializing config: RESPONSE_CODE_DETAILS does not take any parameters or length`. So `%RESPONSE_CODE_DETAILS%` takes NO `(...)` arg and NO `:N` truncation — identical to the `%ROUTE_NAME%`/`%PROTOCOL%`/`%RESPONSE_CODE%` no-arg keyword class (the shared no-arg parse arm already rejects both).
- **The values:** a `direct_response` route → `%RESPONSE_CODE_DETAILS%` = `direct_response`; an upstream-routed `200` → `via_upstream`. As a json single-operator value → quoted (`"direct_response"`); in a mixed/multi-segment value (`d=%RESPONSE_CODE_DETAILS%`) → `d=direct_response`.
- **The differential fixture path = `direct_response`** (NOT `via_upstream`): the access-log fixture family (`0040`/`0046`/`0047`/`0048`/`0049`) uses a `direct_response` route with `clusters: []` (NO upstream backend). Cloning `0049`'s structure with `%RESPONSE_CODE_DETAILS%` yields the byte-exact `direct_response` line with ZERO new backend infra. `via_upstream` is implemented (one line in each proxy-success arm) + render-unit-tested, but NOT differentially witnessed (no upstream fixture in this family); the broader detail vocabulary (the 4xx/5xx/`route_not_found`/filter-synth details) is §2.2 deferred.

---

## File Structure
- **Modify** `crates/envoy-accesslog/src/record.rs` — add `pub response_code_details: Option<String>` (mirror `route_name`, after it at `:83`); update ALL `AccessLogRecord { … }` constructors/test-builders in the workspace with `response_code_details: None` (the compiler finds them — `cargo build --workspace --all-targets`).
- **Modify** `crates/envoy-accesslog/src/command_operator.rs` — add `Op::ResponseCodeDetails` to the `Op` enum (near `Op::RouteName` `:68`); a `"RESPONSE_CODE_DETAILS"` no-arg keyword dispatch (alongside the `"ROUTE_NAME"` arm in the no-arg match `:239-254`); a `render_op` arm `record.response_code_details.as_deref().unwrap_or(empty_or_dash)` (mirror `Op::RouteName` `:517`).
- **Modify** `crates/envoy-accesslog/src/json_format.rs` — add an `Op::ResponseCodeDetails` arm to `encode_single_op` (`:246` area): `quote_opt(out, r.response_code_details.as_deref())` (present→quoted, absent→`null`; mirror the `Op::RouteName` arm).
- **Modify** `crates/envoy-http1/src/hcm.rs` — (i) extend `BuildOutcome::Synth(Response)` → `BuildOutcome::Synth(Response, Option<&'static str>)` (the enum at `:1368`); (ii) tag the 5 construction sites: `synth_direct_response` (`:1525`) → `Some("direct_response")`, the 4 error synths (`synth_501` `:809`, `synth_400` `:1476`, `synth_404` `:1501`/`:1519`) → `None`; (iii) the writer-arm `BuildOutcome::Synth` match (`:855`) reads the detail into a new `response_code_details_for_log`; (iv) the proxy-success arm (`:978`, where `upstream_host_for_log = Some(endpoint)`) → `Some("via_upstream")`; (v) declare `let mut response_code_details_for_log: Option<String> = None;` beside `upstream_host_for_log` (`:835`); (vi) the record build (`:1211` area) sets `response_code_details: response_code_details_for_log`.
- **Modify** `crates/envoy-http2/src/hcm.rs` — mirror: the `BuildOutcome::Synth(r)` match arm (`:537`) reads the detail; the proxy-success arm (`:675`, `upstream_host_for_log_h2 = Some(endpoint)`) → `Some("via_upstream")`; compute `response_code_details_for_log_h2` in `handle_one_stream` and thread it as a new `finalize_h2_stream` parameter (mirror `route_name_for_log_h2` at `:473`/`:811`/`:840`/`:930`); the record build (`:929` area) sets `response_code_details`.
- **Create** `tests/fixtures/0050-accesslog-response-code-details/*` (clone `0049`'s direct_response structure) + `tests/differential/tests/access_log_response_code_details.rs` (mirror `access_log_route_name.rs`).
- **Modify** the `parse_bootstrap` fuzz corpus (a `%RESPONSE_CODE_DETAILS%` seed — a full bootstrap with the operator in an `access_log` `json_format`, exercising parse + compile through the boot path; distinct filename + an explicit `!`-un-ignore line in `crates/envoy-config/fuzz/.gitignore` since the corpus dir is `*`-gitignored; `git ls-files` to confirm it is tracked) + `docs/envoy-rust/BEHAVIOR_CONTRACT.md`. This mirrors the phase-41 `route_name.yaml` precedent EXACTLY (single corpus, proven to satisfy §7.5 (d)). NO new fuzz target. _(Optionally ALSO seed `accesslog_format_parse` for extra parser coverage — a 2nd `!`-un-ignore in `crates/envoy-accesslog/fuzz/.gitignore` — but the `parse_bootstrap` seed alone satisfies the gate.)_

> Before starting: read `command_operator.rs` for the `Op::RouteName` precedent (the no-arg keyword + `render_op` arm) and `json_format.rs` for its `encode_single_op` arm — `%RESPONSE_CODE_DETAILS%` copies that pattern exactly. Read the H1 writer-arm match (`:853-1114`) + the proxy-success site (`:978`) + the record build (`:1196-1221`), and the H2 `finalize_h2_stream` param-thread (`:828-930`).

---

### Task 1: `response_code_details` record field
- [ ] **Step 1 — failing test.** In `record.rs` tests, assert `AccessLogRecord` has a `response_code_details: Option<String>` (construct a record with `response_code_details: Some("via_upstream".into())`).
- [ ] **Step 2 — run, verify FAIL** (no field). Run: `cargo test -p envoy-accesslog response_code_details`. Expected: compile error (unknown field).
- [ ] **Step 3 — implement.** Add `pub response_code_details: Option<String>` (after `route_name` `:83`); fix all constructors with `response_code_details: None`. Iterate `cargo build --workspace --all-targets` until green (it finds every `AccessLogRecord { … }` literal — H1 `:1196`/`:1816`, H2 `:929`, accesslog/test builders).
- [ ] **Step 4 — run, verify PASS.** Run: `cargo test -p envoy-accesslog`.
- [ ] **Step 5 — commit.** `feat(accesslog): AccessLogRecord.response_code_details field [phase42 T1]`

### Task 2: `Op::ResponseCodeDetails` parse + text render
- [ ] **Step 1 — failing test.** `parse_format("%RESPONSE_CODE_DETAILS%")` → `[Op(ResponseCodeDetails)]`; `render_value_segments` on a record with `response_code_details: Some("direct_response")` → `direct_response`; with `None` → `-`; mixed `"d=%RESPONSE_CODE_DETAILS%"` → `d=direct_response` / `d=-`. Plus a no-arg-rejection test: `parse_format("%RESPONSE_CODE_DETAILS:3%")` and `"%RESPONSE_CODE_DETAILS(x)%"` are ERRORS (mirror `route_name_rejects_paren_argument`) — the §6.2-locked strict no-arg grammar.
- [ ] **Step 2 — run, verify FAIL.** Run: `cargo test -p envoy-accesslog response_code_details`. Expected: FAIL (no `Op::ResponseCodeDetails`).
- [ ] **Step 3 — implement.** Add `Op::ResponseCodeDetails` variant; `"RESPONSE_CODE_DETAILS" => Op::ResponseCodeDetails` in the no-arg keyword match (alongside `"ROUTE_NAME"` `:239-254` — the shared no-arg arm rejects `(...)` and `:N`); the `render_op` arm `record.response_code_details.as_deref().unwrap_or(empty_or_dash)` (mirror `Op::RouteName` `:517`).
- [ ] **Step 4 — run, verify PASS** + all existing `command_operator` tests green. Run: `cargo test -p envoy-accesslog`.
- [ ] **Step 5 — commit.** `feat(accesslog): %RESPONSE_CODE_DETAILS% operator parse + text render [phase42 T2]`

### Task 3: `%RESPONSE_CODE_DETAILS%` json single-op typed render
- [ ] **Step 1 — failing test.** json single-op: `response_code_details: Some("direct_response")` → `"direct_response"` (quoted); `None` → `null`; mixed `"d=%RESPONSE_CODE_DETAILS%"` → `"d=direct_response"` / `"d=-"`.
- [ ] **Step 2 — run, verify FAIL.** Run: `cargo test -p envoy-accesslog json`. Expected: FAIL (no arm).
- [ ] **Step 3 — implement.** Add the `Op::ResponseCodeDetails` arm to `encode_single_op` (`quote_opt(out, r.response_code_details.as_deref())`; mirror `Op::RouteName` `:246`).
- [ ] **Step 4 — run, verify PASS** + the phase-38/39/41 json tests green (no regression). Run: `cargo test -p envoy-accesslog`.
- [ ] **Step 5 — commit.** `feat(accesslog): %RESPONSE_CODE_DETAILS% json typed render [phase42 T3]`

### Task 4: `BuildOutcome::Synth` detail tag + H1 plumbing (direct_response + via_upstream)
- [ ] **Step 1 — failing test.** An H1 request matching a `direct_response` route → the built `AccessLogRecord.response_code_details == Some("direct_response")`; (if the existing H1 proxy test harness is readily reusable) a proxied request → `Some("via_upstream")`. Test at the HCM record-construction layer (mirror `hcm_h1_sets_route_name_from_matched_route` `:3919`).
- [ ] **Step 2 — run, verify FAIL** (always `None`). Run: `cargo test -p envoy-http1 response_code_details`.
- [ ] **Step 3 — implement.**
  - Extend `BuildOutcome::Synth(Response)` → `BuildOutcome::Synth(Response, Option<&'static str>)` (`:1368`).
  - Tag the construction sites: `synth_direct_response` (`:1525`) → `BuildOutcome::Synth(synth_direct_response(dr, close), Some("direct_response"))`; the 4 error synths (`:809`/`:1476`/`:1501`/`:1519`) → `, None)`.
  - Declare `let mut response_code_details_for_log: Option<String> = None;` beside `upstream_host_for_log` (`:835`).
  - The writer-arm `BuildOutcome::Synth(resp, details)` match (`:855`): `outgoing = resp; response_code_details_for_log = details.map(str::to_owned);`.
  - The proxy-success arm (`:978`, where `upstream_host_for_log = Some(endpoint.to_string())`): add `response_code_details_for_log = Some("via_upstream".to_owned());`.
  - The `SynthFromDecode` arm (`:1100`, filter-synth) → leave `None` (deferred).
  - The record build (`:1211` area) → `response_code_details: response_code_details_for_log,`.
  - `cargo build --workspace --all-targets` until green (the `BuildOutcome::Synth` shape change also lights the H2 match arm `:537` — fix it in Task 5; a temporary `, _` or compile error there is expected until T5).
- [ ] **Step 4 — run, verify PASS** (H1 direct_response sets the detail) + `cargo test -p envoy-http1`.
- [ ] **Step 5 — commit.** `feat(hcm): tag response_code_details on the H1 access-log record (direct_response + via_upstream) [phase42 T4]`

### Task 5: H2 plumbing (the `response_code_details_for_log_h2` parameter-thread)
- [ ] **Step 1 — failing test.** An H2 request matching a `direct_response` route → the built `AccessLogRecord.response_code_details == Some("direct_response")` (mirror `hcm_h2_sets_route_name_from_matched_route` `:2322`).
- [ ] **Step 2 — run, verify FAIL.** Run: `cargo test -p envoy-http2 response_code_details`.
- [ ] **Step 3 — implement.** In `handle_one_stream`: declare `let mut response_code_details_for_log_h2: Option<String> = None;` beside `let mut upstream_host_for_log_h2` (`:533`). The `BuildOutcome::Synth(r)` arm (`:537`) is EXPRESSION-position (`=> r`) — convert it to a block: `BuildOutcome::Synth(r, details) => { response_code_details_for_log_h2 = details.map(str::to_owned); r }`. Set the proxy-success arm (`:675`, `upstream_host_for_log_h2 = Some(endpoint)`) → also `response_code_details_for_log_h2 = Some("via_upstream".to_owned());`. Add a new `response_code_details_for_log_h2` parameter to `finalize_h2_stream` (mirror `route_name_for_log_h2` at `:840`); pass it at the call site (`:811` area); assign it at the record build (`:929` area) → `response_code_details: response_code_details_for_log_h2,`.
- [ ] **Step 4 — run, verify PASS** + `cargo build --workspace --all-targets` (all `BuildOutcome::Synth` consumers green).
- [ ] **Step 5 — commit.** `feat(hcm): set response_code_details on the H2 access-log record [phase42 T5]`

### Task 6: fixture `0050` + fuzz seed + BEHAVIOR_CONTRACT + gate
- [ ] **Step 1 — failing test.** Wire `0050-accesslog-response-code-details` (reuse `Driver::Http1WithAccessLog` + `AccessLogByteExactProbe`, whole-line byte-exact; mirror `access_log_route_name.rs`): clone `0049`'s `direct_response` config (a `direct_response` route, `clusters: []`) with a format containing `%RESPONSE_CODE_DETAILS%` (single-op + a mixed `d=%RESPONSE_CODE_DETAILS%`) → the byte-exact line with `direct_response`. **Capture the live bytes first** (boot the paired config against `envoyproxy/envoy:v1.33.0`).
- [ ] **Step 2 — run, verify FAIL** (rebuild `cargo build -p envoy-bin` FIRST — the differential runs the DEBUG binary; a stale binary REDs with `unknown field`/old behavior).
- [ ] **Step 3 — implement.** Author the paired `envoy.yaml`/`envoy-rust.yaml`/`expectations.yaml`/`README.md` (identical direct_response route + format). Run `0050` in isolation; confirm `0001`-`0049` unaffected.
- [ ] **Step 4 — run, verify PASS.** Run: `cargo test -p differential access_log_response_code_details` (in isolation — the differential fixtures flake under full-workspace parallel load; CI is authoritative).
- [ ] **Step 5 — commit + local gate.** `test(differential): fixture 0050 %RESPONSE_CODE_DETAILS% + seed + BEHAVIOR_CONTRACT [phase42 T6]` (the seed `response_code_details.yaml` distinct filename + `!`-un-ignore line + `git ls-files` check; NO new fuzz target; the BEHAVIOR_CONTRACT `%RESPONSE_CODE_DETAILS%` note in the "Access log field mapping" subsection). Then run the local gate: `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt --check`, `cargo test --workspace`, `cargo deny check`.

---

## Acceptance (§7.5, re-run at state-4)
(a) `0050` green (byte-exact `direct_response` line) + (b) all `0001`-`0049` green + (c) h2spec ≥95% (NO HTTP/2 codec change) + (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean (with the `%RESPONSE_CODE_DETAILS%` seed) — NO new target + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency; NO new `ConfigError` variant; ONE new `AccessLogRecord` field; ONE new `Op` variant; the `BuildOutcome::Synth` 2-tuple change is internal (no public-API/config-surface change).

## Notes for the executor
- `%RESPONSE_CODE_DETAILS%` IS the `%ROUTE_NAME%` pattern — copy `Op::RouteName`'s no-arg keyword + `render_op` `unwrap_or` + `encode_single_op` `quote_opt` arms, substituting `response_code_details`.
- **The differential value is `direct_response`** (the access-log fixture family is direct_response-only, no backend). `via_upstream` is implemented (proxy-arm one-liner, both codecs) + render-unit-tested but NOT differentially witnessed; the error-synth/filter-synth/`route_not_found` details are §2.2 DEFERRED (those paths set `None` → render `-`/`null`, exercised by no fixture).
- **Two state-1 spec-review Minors (folded here):** (1) the absent→`null`/`-` backstop is an INTERNAL-MODEL artifact (`%RESPONSE_CODE_DETAILS%` is always-present-on-completion at v1.33.0 for the witnessed paths; the `None` arm gives byte-preservation + covers the deferred paths, NOT an upstream-witnessed value); (2) the fuzz seed needs the explicit `!`-un-ignore line or it is silently untracked + invisible to CI (`git ls-files` to confirm).
- Default-absent byte-preservation (the new `Option` field defaults `None` + the operator is new) keeps `0001`-`0049` green.
- The `BuildOutcome::Synth` shape change is SHARED (H1 owns the enum; H2 reuses it via `envoy_http1::build_response`) — the compiler lights both consumers (H1 writer-arm `:855`, H2 match `:537`); `cargo build --workspace --all-targets` finds every site.

---

_Scope locked by **ADR-0099** (the phase-42 pick + the §A/§B existence facts). The §6.2 recon at THIS state-2 confirmed the strict no-arg grammar + the direct_response fixture path (NOT via_upstream — the access-log fixture family has no backend); these REFINE the SPEC's "via_upstream natural fixture" projection WITHOUT overturning a §A-§C fact, so NO §6.2-reconciliation ADR fires (ADR-0100 stays reserved-but-unfired for the §6.1 split, which does NOT fire). The state-3 implementation is the next session._
