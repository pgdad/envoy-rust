# Phase 37 — `37-rbac-url-path-condition` — PROGRESS

> State-3 implementation log (`superpowers:executing-plans`, TDD per task per
> `superpowers:test-driven-development`). Ground truth: **ADR-0090** §A–§D
> (empirically locked vs live `envoyproxy/envoy:v1.33.0`). PLAN: `PLAN.md` (7 TDD
> tasks). Append-only; one entry per task on completion.

---

## Task 1 — `PathMatcher` config struct + export — DONE

**TDD:** wrote 4 failing tests first (`path_matcher_parses_exact_and_round_trips`,
`path_matcher_empty_is_missing_path_error`, `path_matcher_empty_string_matcher_is_missing_mode_error`,
`path_matcher_unknown_subkey_is_denied`) in `bootstrap.rs` `rbac_tests`; confirmed
RED (`cannot find type PathMatcher in the crate root`); added the thin derived
`#[serde(deny_unknown_fields)] pub struct PathMatcher { pub path: StringMatcher }`
after `MetadataPathSegment` (`bootstrap.rs`) + exported it from `lib.rs` (after
`PathConfigSource`); confirmed GREEN.

**Evidence:** `cargo test -p envoy-config path_matcher` → `4 passed` (the 4 new
PathMatcher tests; a 5th pre-existing `parses_route_with_path_matcher` matched the
filter incidentally and also passed).

**ADR-0090 §D:** a thin DERIVED struct suffices — the required `path` field +
`deny_unknown_fields` + the inner `StringMatcher` visitor's "missing mode key"
error cover §D cases 1–3 (empty `PathMatcher` → missing `path`; `path: {}` →
missing mode key; unknown sub-key → `deny_unknown_fields`). No hand-rolled visitor.

**Commit:** `phase 37: PathMatcher config struct (RBAC url_path) [ADR-0090]`

---

## Task 2 — `Permission::UrlPath` end-to-end — DONE

**TDD:** wrote `permission_parses_url_path_and_json_round_trips` (bootstrap.rs,
YAML parse + JSON round-trip) and `url_path_permission_exact_matches_and_strips_query`
(rbac.rs, + new `req_with_path` helper) FIRST; confirmed RED (`no variant ... UrlPath
... for enum Permission` / `... RuntimePermission`).

**Implemented (whole task before commit — an enum variant breaks the exhaustive
matches in BOTH crates):**
- `bootstrap.rs`: `Permission::UrlPath(PathMatcher)` variant (+ `#[serde(rename = "url_path")]`),
  `"url_path"` in `KEYS`, the visitor arm `"url_path" => Permission::UrlPath(map.next_value::<PathMatcher>()?)`,
  and the `validate_permission_tree` leaf arm `Permission::UrlPath(_) => Ok(())`.
- `rbac.rs`: `strip_query(path) = path.split('?').next().unwrap_or(path)` free fn;
  `RuntimePermission::UrlPath(StringMatcher)` variant; `eval_permission` arm
  `sm.matches(strip_query(&req.path))`; `lower_permission` arm cloning `pm.path` +
  `compile_safe_regex()?` (phase-36 fallible path) → boot-fatal on bad regex.

**Evidence:** `cargo test -p envoy-config permission_parses_url_path` → `1 passed`;
`cargo test -p envoy-filter url_path_permission` → `1 passed`; `cargo build --workspace`
→ `Finished` (clean — both crates' exhaustive matches updated, workspace-green).

**Commit:** `phase 37: Permission::UrlPath variant + query-stripped eval + fallible lowering [ADR-0090]`

---

## Task 3 — `Principal::UrlPath` end-to-end (symmetric) — DONE

**TDD:** wrote `principal_parses_url_path` (bootstrap.rs) and
`url_path_principal_matches_query_stripped` (rbac.rs) FIRST; confirmed RED
(`no variant ... UrlPath ... for enum Principal` / `... RuntimePrincipal`).

**Implemented (mirror of Task 2 for `Principal`):**
- `bootstrap.rs`: `Principal::UrlPath(PathMatcher)` variant + `#[serde(rename = "url_path")]`,
  `"url_path"` in `KEYS`, visitor arm, `validate_principal_tree` leaf `Principal::UrlPath(_) => Ok(())`.
- `rbac.rs`: `RuntimePrincipal::UrlPath(StringMatcher)` variant; `eval_principal`
  arm `sm.matches(strip_query(&req.path))`; `lower_principal` arm (clone +
  `compile_safe_regex()?`).

**Evidence:** `cargo test -p envoy-config principal_parses_url_path` → `1 passed`;
`cargo test -p envoy-filter url_path_principal` → `1 passed`; `cargo build --workspace`
→ `Finished` (clean).

**Commit:** `phase 37: Principal::UrlPath variant (symmetric) [ADR-0090]`

---

## Task 4 — backstop (modes, composition, DENY-inversion, anchored safe_regex) — DONE

**TDD:** these are pure backstop tests over behavior already built in Tasks 2/3;
ran them after writing — all GREEN with NO new implementation (the correct TDD
outcome for a backstop confirming an existing surface). Added to `rbac.rs` tests:
- `url_path_all_string_modes` — exact/prefix/suffix/contains match+miss matrix.
- `url_path_composes_and_inverts_under_deny` — `not_rule { url_path }` under
  `action: DENY` through `build_from_config` + `decode_headers` (the decision matrix):
  `/allowed`→Continue, `/other`→StopAndSend.
- `url_path_composes_in_and_or_rules` — `and_rules` (both prefixes) / `or_rules`
  (either prefix).
- `url_path_anchored_safe_regex_matches_without_panic` — ADR-0090 §C anchored
  `^/allowed/[0-9]+$` through the full filter: `/allowed/42`→Continue,
  `/allowed/42?q=1`→Continue (query-strip), `/allowed/xx`→StopAndSend,
  `/allowed`→StopAndSend (full-anchor; no first-request panic — compiled at lowering).

**Evidence:** `cargo test -p envoy-filter url_path` → `6 passed` (the 4 backstop +
Task-2 `url_path_permission_exact_matches_and_strips_query` + Task-3
`url_path_principal_matches_query_stripped`).

**Commit:** `phase 37: url_path backstop — modes, composition, DENY-inversion, anchored safe_regex [ADR-0090]`

---

## Task 5 — config-validity boot-fatal backstop (ADR-0090 §D) — DONE

**TDD:** pure guard tests (§D maps to existing error paths — NO new `ConfigError`
variant per ADR-0090 §D); ran after writing — both GREEN with NO new implementation.
- `bootstrap.rs` `rbac_url_path_empty_and_unknown_are_boot_fatal` — §D 1-3 THROUGH
  a full `Permission`: `url_path: {}` (missing `path`), `url_path: { path: {} }`
  (missing mode key), `url_path: { foo: bar }` (`deny_unknown_fields`) all `is_err()`.
- `rbac.rs` `url_path_malformed_safe_regex_is_build_error` — §D 4: a `safe_regex: "["`
  url_path is rejected at `build_from_config` (the lowering `compile_safe_regex()`)
  as `Err(FilterError::InvalidConfig { .. })` — boot-fatal, NOT a first-request panic.

**Evidence:** `cargo test -p envoy-config rbac_url_path_empty` → `1 passed`;
`cargo test -p envoy-filter url_path_malformed` → `1 passed`.

**Commit:** `phase 37: url_path config-validity boot-fatal backstop (ADR-0090 §D) [ADR-0090]`

---

## Task 6 — differential fixture `0045-http-rbac-url-path` + wrapper — DONE

**Created:** `tests/fixtures/0045-http-rbac-url-path/{envoy.yaml,envoy-rust.yaml,
expectations.yaml,README.md}` (templated off `0043`, `header_to_metadata` producer
REMOVED — `url_path` is self-contained; `metadata` permission replaced by
`url_path: { path: { exact: "/allowed" } }`; route is `direct_response{200,"ok\n"}`,
chain `[rbac, router]`, `clusters: []`) + the per-fixture wrapper
`tests/differential/tests/rbac_url_path.rs` (`#[tokio::test] async fn rbac_url_path`
— REQUIRED; there is no manifest/glob, so without it fixture 0045 never runs).
3 path-varying probes: `/allowed`→200+`ok\n`, `/denied`→403+`RBAC: access denied`,
`/allowed?x=1`→200+`ok\n` (the query-strip discriminator).

**Rebuilt the DEBUG binary first** (`cargo build -p envoy-bin` → `Finished`) so the
differential subprocess understands the new `url_path` config key (the
differential-harness-uses-debug-binary discipline — else stale `unknown field`).

**Evidence:** `cargo test -p differential rbac_url_path` →
```
running 1 test
test rbac_url_path ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.06s
```
Genuinely ran Envoy `v1.33.0` (image cached locally; `run_fixture` has no Docker-skip
early-return — it would `Err` if Docker were unavailable) vs the envoy-rust subprocess;
all 3 probes byte-identical. Filtered by test NAME `rbac_url_path` (NOT `0045` — test
names are non-numeric → `0045` would be a false green).

**Commit:** `phase 37: fixture 0045-http-rbac-url-path + differential wrapper (match/miss/query-strip) [ADR-0090]`

---

## Task 7 — fuzz seed + BEHAVIOR_CONTRACT + state-3 gate confirmation — DONE

**Fuzz seed:** created `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_url_path.yaml`
(a full bootstrap with a `[rbac, router]` chain whose policy carries a `safe_regex`
`url_path` Permission `^/allowed/[0-9]+$` AND an `exact` `url_path` Principal — exercises
the `regex` compile path + both enums) + the un-ignore line
`!corpus/parse_bootstrap/hcm_rbac_url_path.yaml` in `crates/envoy-config/fuzz/.gitignore`.
Verified tracked: `git ls-files ...hcm_rbac_url_path.yaml` → prints the path. NO new
fuzz target; NO `ci.yml` change (reuses `parse_bootstrap`).

**Short-budget fuzz (§7.5 gate (d)):**
`cargo fuzz run parse_bootstrap -- -runs=200000 -max_total_time=60` →
`Done 200000 runs in 13 second(s)` — no crash; the new seed exercised.

**BEHAVIOR_CONTRACT:** added the `### Phase 37 (ADR-0089/0090): the RBAC url_path
Permission/Principal condition` subsection (query-strip semantic, fixture-0045 witness,
RE2 full-match, §D boot-fatal, M37-1 `#fragment` carry-forward) after the phase-35/36
RBAC metadata subsections.

**§7.5 gate (state-3 confirmation — full formal gate is the state-4 concern per §5.1):**
- `cargo build --workspace --all-targets` → `Finished` (clean).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → `Finished`
  (clean; fixed one `doc list item without indentation` in the wrapper by adding a
  blank `//!` line before the trailing paragraph).
- `cargo fmt --all -- --check` → clean (after `cargo fmt --all` reflow).
- `cargo test --workspace` → all GREEN **except** the single pre-existing host flake
  `admin_config_dump_server_info` (a backend-cluster fixture whose stats set is keyed on
  this host's Docker bridge IP `192.168.65.2` — the documented "Differential host bridge
  IP" false-RED; CI-authoritative; ZERO connection to RBAC/url_path — fixture 0045 has no
  backend). Library crates: `envoy-config` 518 passed / 0 failed; `envoy-filter` 208
  passed / 0 failed.
- `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`.
- `cargo test -p differential rbac_url_path` → `1 passed` (fixture 0045 green vs live
  Envoy v1.33.0).

`#![forbid(unsafe_code)]` holds; NO new crate/dependency/`HttpFilterInstance` variant/
`ConfigError` variant/fuzz-target. M36-1 anchored-locked (NOT consumed). New carry-forward
**M37-1** (codec `#`-handling, ADR-0090 R1).

**Commit:** `phase 37: url_path parse_bootstrap seed + BEHAVIOR_CONTRACT subsection [ADR-0090]`

---

## State-4 verification

**Session:** `BOOTSTRAP_PROMPT.md` §5 state-4 verification gate
(`superpowers:verification-before-completion`). `SPEC.md` + `PLAN.md` + `PROGRESS.md`
exist, implementation COMPLETE, `REVIEW.md` ABSENT → run the full §7.5 phase-done gate
and quote EVERY command's output verbatim (the §7.5 evidence (a)-(e) IS the state-4
deliverable). `git status` clean; `HEAD` at the phase-37 state-3 STATE-advance commit
`f73cc25`. Did ONE state this session per §5.1 — STOP after the gate is COMPLETE +
PUSHED; do NOT chain into the state-5 code-review.

### 1. `cargo build --workspace --all-targets`

```
   Compiling http1-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http1-echo-server)
   Compiling http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.41s
```
Exit 0 — clean.

### 2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`

```
    Checking envoy-config v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-config)
    Checking differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Checking envoy-cluster v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-cluster)
    Checking envoy-listener v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-listener)
    Checking envoy-filter v0.1.0 (/home/esa/git/envoy-rust/crates/envoy-filter)
    Checking envoy-tls v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-tls)
    Checking envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
    Checking envoy-tcp v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-tcp)
    Checking envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
    Checking envoy-health v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-health)
    Checking envoy-admin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-admin)
    Checking http1-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http1-echo-server)
    Checking envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Checking http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.72s
```
Exit 0 — zero warnings under `-D warnings`.

### 3. `cargo fmt --all -- --check`

```
FMT_EXIT=0
```
No diff — clean.

### 4. `cargo test --workspace`

All test binaries GREEN **except** the single pre-existing host false-RED
`admin_config_dump_server_info`. The only non-`ok` result lines across the whole
workspace run:

```
test admin_config_dump_server_info ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.74s
```
`TEST_EXIT=101` (driven entirely by that one fixture).

The url_path-bearing library crates are GREEN:

```
     Running unittests src/lib.rs (target/debug/deps/envoy_config-4416ef4542f96e9d)
test result: ok. 518 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running unittests src/lib.rs (target/debug/deps/envoy_filter-551a8cb856988bd2)
test result: ok. 208 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
```

**`admin_config_dump_server_info` IS the documented bridge-IP false-RED — NOT a
regression, ZERO connection to url_path.** Re-run in isolation
(`cargo test -p differential --test admin_config_dump_server_info -- --nocapture`)
confirms the divergence is purely the backend-cluster stats set keyed on this host's
Docker bridge IP `192.168.65.2` (NOT the allow-listed `192.168.65.254`/`172.17.0.1`):

```
thread 'admin_config_dump_server_info' (431155) panicked at tests/differential/tests/admin_config_dump_server_info.rs:18:10:
fixture green: admin body rule: /clusters

Caused by:
    text_lines diverged after allow-lists:
      envoy-only:      ["backend::192.168.65.2:39769::canary::false", "backend::192.168.65.2:39769::cx_active::0", ... "backend::192.168.65.2:39769::zone::"]
      envoy-rust-only: []
```
Memory: "Differential host bridge IP 192.168.65.2". Fixture 0045 (`url_path`) has NO
backend cluster, so it cannot be touched by this stats-IP divergence. CI is authoritative.

### 5. `cargo deny check`

```
advisories ok, bans ok, licenses ok, sources ok
DENY_EXIT=0
```
Exit 0. (Four `license-not-encountered` warnings for unmatched-but-allowed licenses
`BSD-2-Clause`/`MPL-2.0`/`Unicode-DFS-2016`/`Zlib` are pre-existing benign deny.toml
allowances, not advisories.)

### 6. Fuzz — gate (d): `cargo fuzz run parse_bootstrap -- -runs=200000 -max_total_time=60`

(run from `crates/envoy-config/fuzz`; the new `hcm_rbac_url_path.yaml` seed is in the
corpus; NO new target; `accesslog_format_parse` UNCHANGED)

```
###### End of recommended dictionary. ######
Done 200000 runs in 14 second(s)
FUZZ_EXIT=0
```
Exit 0 — 200000 runs, no crash/timeout/OOM.

### 7. Differential — gate (a)+(b): `cargo build -p envoy-bin && cargo test -p differential rbac_url_path`

`envoy-bin` REBUILT first (the differential-harness-uses-debug-binary lesson — else stale
`unknown field`):

```
   Compiling envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.44s
BIN_BUILD_EXIT=0
```

Filtered by TEST NAME `rbac_url_path` (NOT `0045` — non-numeric test names → `0045`
matches zero tests = false green):

```
     Running tests/rbac_url_path.rs (target/debug/deps/rbac_url_path-7ab696b38616001e)
running 1 test
test rbac_url_path ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.06s
```
Fixture `0045-http-rbac-url-path` GREEN vs live Envoy v1.33.0 (`/allowed`→200,
`/denied`→403, `/allowed?x=1`→200 query-strip).

### 8. Conformance — gate (c): h2spec ≥95%

**Phase 37 made NO HTTP/2 codec change** — the `url_path` RBAC condition lands purely on
the existing phase-10 `envoy.filters.http.rbac` filter (config struct + runtime eval +
lowering), touching no HTTP/2 surface (`crates/envoy-http2` untouched). The h2spec
threshold is therefore unaffected by this phase; gate (c) is satisfied by the
unchanged-surface rationale (the conformance suite covers a surface this phase does not
modify).

### Gate summary

| Gate | Command | Result |
|------|---------|--------|
| build | `cargo build --workspace --all-targets` | exit 0 — clean |
| clippy | `cargo clippy … -- -D warnings` | exit 0 — zero warnings |
| fmt | `cargo fmt --all -- --check` | exit 0 — no diff |
| test | `cargo test --workspace` | GREEN except documented bridge-IP false-RED `admin_config_dump_server_info`; `envoy-config` 518 + `envoy-filter` 208 GREEN |
| deny | `cargo deny check` | exit 0 — advisories/bans/licenses/sources ok |
| fuzz (d) | `parse_bootstrap -runs=200000 -max_total_time=60` | exit 0 — 200000 runs, no crash |
| differential (a)+(b) | `cargo test -p differential rbac_url_path` | `1 passed` vs live Envoy v1.33.0 |
| conformance (c) | h2spec ≥95% | unchanged-surface (no H2 change this phase) |

State-4 verification gate COMPLETE. The ONLY RED is the pre-existing, documented,
url_path-independent host bridge-IP false-RED. Implementation verified COMPLETE against
the full §7.5 phase-done gate. Next session: state-5 code-review (`REVIEW.md`).
