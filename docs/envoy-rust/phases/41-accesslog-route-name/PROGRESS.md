# Phase 41 — `41-accesslog-route-name` — Implementation Progress (state-3)

**Goal delivered:** the `%ROUTE_NAME%` access-log command operator — the config
`name` of the matched route — byte-equivalent to upstream Envoy v1.33.0
(ADR-0098 §C). The original state-1 pick `%REQ_WITHOUT_QUERY%` was VOID at
v1.33.0 (ADR-0098 §A) and was NOT implemented.

`%ROUTE_NAME%` is the `%UPSTREAM_HOST%` pattern: an `Option<String>` (NAMED route
→ the name; UNNAMED → absent → `-` sentinel in text/mixed, json `null` in a
single-operator leaf).

## Tasks (all 6 complete, strict TDD per task)

| Task | What | Commit |
|------|------|--------|
| T1 | `AccessLogRecord.route_name: Option<String>` field (mirror `upstream_host`) | `1f344b5` |
| T2 | expose per-route `bootstrap::Route.name` (hand-rolled serde: 5 manual edits) | `3558868` |
| T3 | `Op::RouteName` variant + `"ROUTE_NAME"` no-arg keyword + `render_op` text arm | `fe6772c` |
| T4 | `%ROUTE_NAME%` json single-op typed render (`encode_single_op` `quote_opt` arm) | `70639ab` |
| T5 | HCM route-name plumbing — H1 (`serve_connection`) + H2 (`route_name_for_log_h2` param threaded from the `handle_one_stream` match site) | `229c673` |
| T6 | fixture `0049` + differential test + fuzz seed + BEHAVIOR_CONTRACT | `2c7aff5`, fmt `991a006` |

## Locked-fact compliance (ADR-0098 §C)

- `record.rs`: `pub route_name: Option<String>` after `upstream_host`.
- `command_operator.rs`: `Op::RouteName`; `"ROUTE_NAME"` added to the no-arg
  keyword group (rejects a `(...)` arg like its siblings); `render_op` arm
  `record.route_name.as_deref().unwrap_or(empty_or_dash)`.
- `json_format.rs`: `Op::RouteName => quote_opt(out, r.route_name.as_deref())`.
- `bootstrap.rs` **M41-A** (hand-rolled serde — `#[serde(default)]` is INERT):
  (i) `pub name: String` on `struct Route`; (ii) a `"name"` arm in the visitor
  `match key` + `"name"` added to the `unknown_field` allow-list (so a route
  `name` key no longer boot-fatals); default `String::new()` when absent; (iii)
  `name: name.unwrap_or_default()` in the `Ok(Route { … })` constructor; (iv)
  emit `name` in the hand-rolled `Serialize` ONLY when non-empty (config-dump
  byte-stability for the default-empty case).
- HCM **M41-B** plumbing: H1 sets `route_name` directly at the
  `serve_connection` record build from the live `matched_route` (empty name →
  `None`). H2 computes `route_name_for_log_h2` at the `handle_one_stream` match
  site and threads it as a new `finalize_h2_stream` parameter (NOT just set
  alongside `upstream_host`).

## Blast radius (M41-B) — literals touched

- `AccessLogRecord { … }` exhaustive literals: **9** sites got `route_name: None`
  (across `record.rs`, `command_operator.rs`, `default_format.rs`,
  `file_sink.rs`, `json_format.rs`, `envoy-http1/hcm.rs`, `envoy-http2/hcm.rs`)
  — found iteratively via `cargo build --workspace --all-targets`.
- `Route { … }` exhaustive literals (no `Default`): **53** sites got
  `name: …` — 1 production deep-clone at `envoy-http1/hcm.rs:300`
  (`name: r.name.clone()` — preserves names across the RDS-snapshot clone) + 52
  test literals (`name: String::new()`) across `envoy-config`/`envoy-filter`/
  `envoy-http1`/`envoy-http2`. (PLAN estimated ~60+; actual 53.)

## Differential (fixture 0049) — byte-exact, LIVE-CAPTURED

Live bytes captured FIRST from `envoyproxy/envoy:v1.33.0` (Docker, fresh host
port 13000 to avoid a stray `envoy-bin` on 10000), a NAMED route `name: myroute`
with `%ROUTE_NAME%` in a single-op leaf and a mixed leaf:

```
{"method":"GET","proto":"HTTP/1.1","rn":"r=myroute","single_rn":"myroute"}
```

`single_rn:"myroute"` (single-op → quoted) + `rn:"r=myroute"` (mixed → string),
keys UTF-8-sorted, one trailing `\n` — EXACTLY ADR-0098 §C.

- Rebuilt the **debug** `envoy-bin` before the differential (the harness runs
  `target/debug/envoy-bin`).
- `access_log_route_name` ran in **isolation** (`--test-threads=1`): **PASS**
  (byte-identical both sides).
- Regression: `access_log_omit_empty` (0048) + `access_log_json_format` ran in
  isolation: **PASS** (still byte-exact). Fixtures `0001`-`0048` carry no named
  route and no `%ROUTE_NAME%`, so default-absent byte-preservation holds.

## Fuzz seed (T6)

`crates/envoy-config/fuzz/corpus/parse_bootstrap/route_name.yaml` (distinct
filename) + its own `!`-un-ignore line in `crates/envoy-config/fuzz/.gitignore`.
Verified `git ls-files` tracks it; verified it parses via `parse_bootstrap`
(throwaway test, GREEN, then reverted). NO new fuzz target — the existing
`parse_bootstrap` CI step (`.github/workflows/ci.yml:106`) runs the whole corpus
dir, so the seed is auto-included.

## Local gate (state-3, run at the end)

- `cargo build --workspace --all-targets` → **clean**.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → **clean** (no warnings).
- `cargo fmt --all -- --check` → **clean** (after `991a006`).
- `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (the
  pre-existing `Zlib` license-not-encountered warning is unrelated).
- `cargo test --workspace` → **1 failure: `admin_config_dump_server_info`** — a
  documented HOST false-RED (the backend resolves to this host's bridge IP
  `192.168.65.2`, not the allow-listed IP; MEMORY.md "Differential host bridge
  IP 192.168.65.2"). It is a cluster/admin config-dump test UNTOUCHED by phase
  41; fails identically in isolation; CI is authoritative. All other suites
  GREEN (envoy-accesslog 81, envoy-config 533, envoy-http1 135, envoy-http2 75,
  plus the rest of the workspace).

## Constraints honored

`#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target/`ConfigError`
variant; ONE new `AccessLogRecord` field (`route_name`); ONE new `Op` variant
(`RouteName`); ONE new `bootstrap::Route` field (`name`).

## Acceptance (§7.5) status

(a) `0049` green (byte-exact) ✔ · (b) `0001`-`0048` green (isolation spot-check +
default-absent preservation) ✔ · (c) h2spec — re-run at state-4 gate (CI) · (d)
fuzz clean with the `%ROUTE_NAME%` seed, NO new target ✔ · (e)
build/clippy/fmt/test/deny clean (test: 1 documented host false-RED only) ✔ ·
(f) `REVIEW.md` — state-5.
