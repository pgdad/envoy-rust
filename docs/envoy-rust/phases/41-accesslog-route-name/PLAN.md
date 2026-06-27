# Phase 41 — `41-accesslog-route-name` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans`. Steps use `- [ ]`. TDD per `superpowers:test-driven-development` on every task.

**Goal:** Add the `%ROUTE_NAME%` access-log command operator — the config `name` of the matched route — byte-equivalent to upstream Envoy v1.33.0.

**Architecture:** A new `AccessLogRecord.route_name: Option<String>` field (set by the HCM at route-match), a new `Op::RouteName` (a `"ROUTE_NAME"` no-arg keyword), its `render_op` arm, and its `encode_single_op` arm — all mirroring the EXISTING `%UPSTREAM_HOST%` (`Op::UpstreamHost`, an `Option<String>` field → `render_op` `unwrap_or("-")`, `encode_single_op` `quote_opt`). Config-deterministic, byte-exact.

**Tech Stack:** Rust workspace; the hand-rolled `envoy-accesslog` command-operator engine; the `testcontainers` differential harness.

**§6.2 LOCKED FACTS (ADR-0098 §C — the recon FIRED with the pivot from the VOID `%REQ_WITHOUT_QUERY%`):**
- `%ROUTE_NAME%` renders the matched route's config `name`: NAMED → the name (json single-op → quoted `"name"`; mixed → `r=name`); UNNAMED → ABSENT (json single-op → `null`; mixed/text → the `-` sentinel `r=-`).
- I.e. IDENTICAL to `%UPSTREAM_HOST%` — an `Option<String>` (`Some`→name, `None`→absent).

---

## File Structure
- **Modify** `crates/envoy-accesslog/src/record.rs` — add `pub route_name: Option<String>` (mirror `upstream_host`); update ALL `AccessLogRecord { … }` constructors/test-builders in the workspace with `route_name: None`.
- **Modify** `crates/envoy-accesslog/src/command_operator.rs` — add `Op::RouteName` to the `Op` enum (`:36`); a `"ROUTE_NAME"` no-arg keyword dispatch (mirror the `%PROTOCOL%`/`%RESPONSE_CODE%` no-arg keywords near `:231`); a `render_op` arm `record.route_name.as_deref().unwrap_or("-")` (mirror `Op::UpstreamHost`).
- **Modify** `crates/envoy-accesslog/src/json_format.rs` — add a `Op::RouteName` arm to `encode_single_op` (`:224`): `quote_opt(out, r.route_name.as_deref())` (present→quoted, absent→`null`; mirror the `Op::UpstreamHost` arm).
- **Modify** `crates/envoy-config/src/bootstrap.rs` — **PLAN-VERIFY**: confirm the route struct (the one matched in the HCM) exposes a `name`; if NOT, add `#[serde(default)] pub name: String,` to it (an empty name = unnamed → `None`).
- **Modify** `crates/envoy-http1/src/hcm.rs` (+ the H2 record-construction site) — where the `AccessLogRecord` is built after the route match, set `route_name` to the matched route's `name` if non-empty, else `None`.
- **Create** `tests/fixtures/0049-accesslog-route-name/*` + `tests/differential/tests/access_log_route_name.rs` (mirror `access_log_omit_empty.rs`).
- **Modify** the `parse_bootstrap` fuzz corpus (a `%ROUTE_NAME%` seed, distinct filename + `!`-un-ignore) + `docs/envoy-rust/BEHAVIOR_CONTRACT.md`.

> Before starting: read `command_operator.rs` for the `Op::UpstreamHost` precedent (the no-arg keyword + `render_op` arm) and `json_format.rs` for its `encode_single_op` arm — `%ROUTE_NAME%` copies that pattern exactly. Read the HCM record-construction site (where `upstream_host` is set) — set `route_name` alongside it.

---

### Task 1: `route_name` record field
- [ ] **Step 1 — failing test.** Assert `AccessLogRecord` has a `route_name: Option<String>` (a unit test constructing a record with `route_name: Some("r".into())`).
- [ ] **Step 2 — FAIL** (no field).
- [ ] **Step 3 — implement.** Add `pub route_name: Option<String>` (after `upstream_host`); fix all constructors (`route_name: None`). `cargo build --workspace` until green.
- [ ] **Step 4 — PASS.**
- [ ] **Step 5 — commit.** `feat(accesslog): AccessLogRecord.route_name field [phase41 T1]`

### Task 2: route config `name` exposure (the per-route `Route` struct — HAND-ROLLED serde)
> **CONFIRMED by review:** the per-route `bootstrap::Route` struct (`bootstrap.rs:1754`) has ONLY `r#match`, `action`, `typed_per_filter_config` — NO `name`. (`RouteConfiguration`/`VirtualHost` have `name`, but those are the wrong structs — `%ROUTE_NAME%` renders the matched ROUTE's name.) `Route` does **NOT derive serde** — it has a **hand-rolled `Deserialize`** (`bootstrap.rs:2012`, a `match key` visitor + an `unknown_field` allow-list at `:2072-2081`) and a **hand-rolled `Serialize`** (`:2116`). So `#[serde(default)]` is INERT; the field must be wired by hand.
- [ ] **Step 1 — failing test.** A route config `routes: [{ name: myroute, match: {prefix: "/"}, … }]` round-trips: the parsed `Route.name == "myroute"`; a route WITHOUT `name` → `Route.name == ""` (empty = unnamed). Also assert an EXISTING `name`-less config still parses (byte-preservation).
- [ ] **Step 2 — FAIL** (no `name` field; `name` key currently boot-fatals as an unknown field).
- [ ] **Step 3 — implement (4 hand-edits):** (i) add `pub name: String` to `struct Route`; (ii) add a `"name" => route_name = Some(map.next_value()?)` arm to the `Deserialize` visitor's `match key` + add `"name"` to the `unknown_field` allow-list (`:2072-2081`) — REQUIRED so a route `name` parses instead of boot-fataling; default to `String::new()` when absent; (iii) add `name` to the `Ok(Route { … })` constructor; (iv) emit `name` in the hand-rolled `Serialize` (only if non-empty, to keep config-dump byte-stable — match the existing optional-field serialize pattern).
- [ ] **Step 4 — blast radius:** `Route` has NO `Default` and is built by EXHAUSTIVE struct literal at ~60+ sites (the non-test `clone` path at `hcm.rs:300`, plus many config/HCM tests across `envoy-config`/`envoy-http1`/`envoy-http2`). A new non-optional `name: String` forces EVERY `Route { … }` literal to add `name: String::new()` (or a value). Use `cargo build --workspace --all-targets` iteratively to find them all; this is mechanical but high-touch (the LoC estimate's upper bound). Then `cargo test -p envoy-config` (all existing route-parse tests green).
- [ ] **Step 5 — commit.** `feat(config): expose per-route name (hand-rolled serde) [phase41 T2]`

### Task 3: `Op::RouteName` parse + text render
- [ ] **Step 1 — failing test.** `parse_format("%ROUTE_NAME%")` → `[Op(RouteName)]`; `render_value_segments` on a record with `route_name: Some("myroute")` → `myroute`; with `None` → `-`; mixed `"r=%ROUTE_NAME%"` → `r=myroute` / `r=-`.
- [ ] **Step 2 — FAIL.**
- [ ] **Step 3 — implement.** `Op::RouteName` variant; `"ROUTE_NAME" => Op::RouteName` no-arg keyword (mirror `%PROTOCOL%`; a `(...)` or `:N` is rejected like the other no-arg ops — confirm via the existing no-arg path); the `render_op` arm `r.route_name.as_deref().unwrap_or("-")`.
- [ ] **Step 4 — PASS** + all existing `command_operator` tests green.
- [ ] **Step 5 — commit.** `feat(accesslog): %ROUTE_NAME% operator parse + text render [phase41 T3]`

### Task 4: `%ROUTE_NAME%` json single-op typed render
- [ ] **Step 1 — failing test.** json single-op: `route_name: Some("myroute")` → `"myroute"` (quoted); `None` → `null`; mixed `"r=%ROUTE_NAME%"` → `"r=myroute"` / `"r=-"`.
- [ ] **Step 2 — FAIL.**
- [ ] **Step 3 — implement.** Add the `Op::RouteName` arm to `encode_single_op` (`quote_opt(out, r.route_name.as_deref())`).
- [ ] **Step 4 — PASS** + phase-38/39 json tests green (no regression).
- [ ] **Step 5 — commit.** `feat(accesslog): %ROUTE_NAME% json typed render [phase41 T4]`

### Task 5: HCM route-name plumbing (H1 + H2)
- [ ] **Step 1 — failing test.** A request matching a named route → the built `AccessLogRecord.route_name == Some(name)`; an unnamed route → `None`. (Test at the HCM record-construction layer, both H1 and H2.)
- [ ] **Step 2 — FAIL** (route_name always None).
- [ ] **Step 3 — implement.** H1: at `serve_connection`'s record build (`hcm.rs:1195-1212`, `upstream_host:` at `:1210`) set `route_name` from the `matched_route` (bound at `:729`, still live) — `Some(name)` if the matched route's `name` is non-empty, else `None`. H2: the H2 record is built in a SEPARATE emit-fn (`hcm.rs:810-917`) that receives `upstream_host_for_log_h2` as a PARAMETER (`:828/:917`) while `matched_route` is bound in `handle_one_stream` (`:469`); so ADD a new `route_name_for_log_h2` parameter, compute it at the `:469` match site (mirroring `upstream_host_for_log_h2`), and thread it through — NOT just "set it alongside `upstream_host`".
- [ ] **Step 4 — PASS** + `cargo build --workspace --all-targets`.
- [ ] **Step 5 — commit.** `feat(hcm): set route_name on the access-log record [phase41 T5]`

### Task 6: fixture `0049` + fuzz seed + BEHAVIOR_CONTRACT
- [ ] **Step 1 — failing test.** Wire `0049-accesslog-route-name` (reuse `Driver::Http1WithAccessLog` + `AccessLogByteExactProbe`, whole-line byte-exact): a route config with a NAMED route + a format containing `%ROUTE_NAME%` → the byte-exact line with the route name. Capture the live bytes first.
- [ ] **Step 2 — FAIL** (rebuild `cargo build -p envoy-bin` first — the differential runs the debug binary).
- [ ] **Step 3 — implement.** Author the paired configs (identical named route). Run `0049` in isolation; confirm `0001`-`0048` unaffected.
- [ ] **Step 4 — PASS.**
- [ ] **Step 5 — commit.** `test(differential): fixture 0049 %ROUTE_NAME% + seed + BEHAVIOR_CONTRACT [phase41 T6]` (the seed `route_name.yaml` distinct filename + `!`-un-ignore + `git ls-files` check; NO new fuzz target; the BEHAVIOR_CONTRACT `%ROUTE_NAME%` note); then run the local gate (build/clippy/fmt/test/deny).

---

## Acceptance (§7.5, re-run at state-4)
(a) `0049` green (byte-exact route-name line) + (b) all `0001`-`0048` green + (c) h2spec ≥95% + (d) fuzz clean (with the `%ROUTE_NAME%` seed) — NO new target + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency; NO new `ConfigError` variant; ONE new `AccessLogRecord` field.

## Notes for the executor
- `%ROUTE_NAME%` IS the `%UPSTREAM_HOST%` pattern — copy `Op::UpstreamHost`'s no-arg keyword + `render_op` `unwrap_or("-")` + `encode_single_op` `quote_opt` arms, substituting `route_name`.
- The state-1 pick `%REQ_WITHOUT_QUERY%` was VOID at v1.33.0 (ADR-0098 §A) — do NOT implement it.
- Default-absent byte-preservation (the new `Option` field defaults `None`) keeps `0001`-`0048` green.

---

_Scope amended by **ADR-0098** (the §6.2 pivot from the VOID `%REQ_WITHOUT_QUERY%` to `%ROUTE_NAME%`). The §6.1 split does NOT fire (**ADR-0099 reserved-but-unfired**). The state-3 implementation is the next session._
