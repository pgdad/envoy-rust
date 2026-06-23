# Phase 34 — `header_to_metadata` — PROGRESS (state-3 implementation)

> **Lifecycle state 3 (implementation).** Driven by `superpowers:subagent-driven-development`:
> a fresh implementer subagent per task, followed by a two-stage review (spec-compliance, then
> code-quality) per task. Every task was TDD (failing test → run-fail → minimal impl → run-pass →
> commit). Read with zero prior context (D-3.4). The §A locked facts (ADR-0084) were honored
> exactly — in particular the TWO MATERIAL divergences: **(A2)** the default `metadata_namespace`
> is `envoy.filters.http.header_to_metadata` (the filter canonical name), NOT `envoy.lb`; **(A3)**
> the on_header_present static `value` WINS over the present header value.

## Summary

All 7 PLAN.md tasks landed, each spec-reviewed ✅ and code-quality-reviewed ✅ (with the few
review findings folded back in before moving on). The `envoy.filters.http.header_to_metadata`
filter is the 12th `HttpFilterInstance` variant: decode-side, `Continue`-only, extracting request
headers into `req.dynamic_metadata`. It REUSES the phase-33 dynamic-metadata store, the
`%DYNAMIC_METADATA(ns:key)%` access-log operator, and the filter-agnostic H1/H2 capture-before-drop
threading UNCHANGED (`envoy-accesslog` untouched). The cross-proxy byte-exact differential
(fixture `0042`) passes green locally.

**Sequencing note (executed as planned):** Tasks 1→4 ran contiguously with a per-crate gate. T1's
new `HttpFilterTypedConfig::HeaderToMetadata` enum variant made the `build` match in
`crates/envoy-filter/src/instance.rs` (exhaustive over that enum, no catch-all) non-exhaustive →
the `envoy-filter` crate did not compile until T4 closed the match. Because T3's module tests live
in `envoy-filter` and cannot run until that crate compiles, **T3 and T4 were implemented together**
(two commits, gated on the crate compiling green after T4). T1/T2 were gated on
`cargo test -p envoy-config`. This is the phase-33 T4-T7 red-window precedent.

## Per-task log

### T1 — `HeaderToMetadataConfig` schema + `HttpFilterTypedConfig::HeaderToMetadata` variant
**Commit `bf42699`.** Files: `crates/envoy-config/src/{bootstrap.rs,lib.rs}`.
Added `HeaderToMetadataConfig` / `HeaderToMetadataRule` / `HeaderToMetadataKeyValue` /
`HeaderToMetadataType` (single `STRING` variant, `SCREAMING_SNAKE_CASE`) + `default_h2m_namespace()`
(returns `"envoy.filters.http.header_to_metadata"` — A2) + the `@type
…header_to_metadata.v3.Config` enum variant + re-exports. All structs `#[serde(deny_unknown_fields)]`;
`key` required (no default). 3 TDD tests (parse; A2 default-namespace; `deny_unknown_fields` rejects
the deferred `remove` field). A minimal name-check stub arm was added to the in-crate
`validate_http_filters` match (forced-compile necessity; the full validator is T2).
**Gate:** `cargo test -p envoy-config` → 489 passed. Spec ✅ / quality ✅ (fmt drift fixed in-amend).

### T2 — `validate_http_filters` arm + `ConfigError::HeaderToMetadataInvalidRule`
**Commit `d27a268`.** Files: `crates/envoy-config/src/{lib.rs,bootstrap.rs}`.
Added the `ConfigError::HeaderToMetadataInvalidRule { listener, detail }` variant; extended the
`HeaderToMetadata` arm to call `validate_header_to_metadata_config`, which enforces §A5 (a)-(d):
empty `header` / no-action / empty `key` (on EITHER action) / `on_header_missing` without `value`
— all boot-fatal (ADR-0049). Name mismatch → `UnsupportedHttpFilter`. 6 TDD tests through
`parse_bootstrap` (incl. the symmetric empty-key-on-`on_header_missing` test added per the
quality review). **Gate:** `cargo test -p envoy-config header_to_metadata` → 9 passed; full crate 495.
Spec ✅ / quality ✅.

### T3 — `HeaderToMetadataFilter` (decode-side extraction)
**Commit `204bad3`.** Files: created `crates/envoy-filter/src/header_to_metadata.rs`; modified
`crates/envoy-filter/src/lib.rs`. Decode-side, `Continue`-only. Per rule: case-insensitive header
lookup (`eq_ignore_ascii_case`); present non-empty → write static `value` if set else the header
value (A3 `kv.value.clone().unwrap_or(header_value)`); present-but-empty → write nothing (A4);
absent → `on_header_missing` value. Encode inert. 7 unit tests covering all §A behaviors. Mirrors
`set_metadata.rs` verbatim.

### T4 — `HttpFilterInstance::HeaderToMetadata` wiring (12th variant)
**Commit `9f32579`.** File: `crates/envoy-filter/src/instance.rs`. Added the variant + `build` arm
(closing the non-exhaustive match) + `decode_headers`/`encode_headers` dispatch arms; NO new
`apply_route_config` arm (falls through the existing `_ => {}`; comment updated). 1 instance test.
**Gate (red window closed):** `cargo test -p envoy-filter` → 185 passed;
`cargo build -p envoy-http1 -p envoy-http2 -p envoy-bin --all-targets` clean.
Spec ✅ (T3+T4) / quality ✅ (T3+T4). A small clippy/doc cleanup (test-module `#[cfg(test)]`,
`collapsible_if`, the restored §A5d `// validated` comment) was folded back into the T2/T3 commits
via autosquash so each commit is independently clippy-clean under `-D warnings`.

### T5 — H1 + H2 in-process backstops
**Commit `cac76b7`.** Files: `crates/envoy-http1/src/hcm.rs`, `crates/envoy-http2/src/hcm.rs`.
Two backstops (`h1_/h2_header_to_metadata_threads_into_access_log`) mirroring the phase-33
`*_dynamic_metadata_threads_into_access_log` tests, swapping the chain to `[header_to_metadata,
router]` (rule `x-tier`→`envoy.lb:tier`), driving a request with `x-tier: prod`, asserting the
rendered access-log line `"prod / -\n"` (present key → `prod`; an unwritten key → `-`).
**Both passed on FIRST RUN** — confirming the PLAN's reuse claim: the filter-agnostic threading
carries `header_to_metadata`'s output with NO new plumbing. **Gate:** `cargo test -p envoy-http1`
→ 129; `cargo test -p envoy-http2` → 74. Spec ✅ / quality ✅.
(Observed: the pre-existing `send_request_maps_h2_handshake_failure_to_typed_error` H2 test is a
known race — fails under full-suite contention, passes in isolation/on rerun; unrelated to phase 34.)

### T6 — Fixture `0042` differential (header-present + header-missing probes)
**Commit `9c403b5`.** Files: `tests/fixtures/0042-http-header-to-metadata/{envoy.yaml,
envoy-rust.yaml,expectations.yaml,README.md}`; `tests/differential/tests/header_to_metadata.rs`.
H1 `direct_response` 200, chain `[header_to_metadata, router]`, log_format
`"m=%REQ(:METHOD)% tier=%DYNAMIC_METADATA(envoy.lb:tier)% missns=%DYNAMIC_METADATA(envoy.absent:k)%\n"`,
rule `x-tier`→`envoy.lb:tier` with `on_header_present: {ns: envoy.lb, key: tier}` +
`on_header_missing: {ns: envoy.lb, key: tier, value: "missing"}` (QUOTED — a bare `none`/`null`/`~`
would parse as YAML null → boot-fatal). TWO probes via `AccessLogByteExactProbe.extra_headers`:
present (`x-tier: prod` → `m=GET tier=prod missns=-`) + absent (→ `m=GET tier=missing missns=-`).
The present/missing pair is the anti-echo guard. **Gate (Docker, `envoyproxy/envoy:v1.33.0`):**
`cargo test -p differential --test header_to_metadata -- --include-ignored` → 1 passed (10.7s),
both probes byte-identical cross-proxy. `cargo build -p envoy-bin` was run first (stale-binary
`unknown filter` lesson). Spec ✅ / quality ✅.

### T7 — BEHAVIOR_CONTRACT extension + `parse_bootstrap` fuzz seed
**Commit `48f8086`.** Files: `docs/envoy-rust/BEHAVIOR_CONTRACT.md`;
`crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_to_metadata.yaml`;
`crates/envoy-config/fuzz/.gitignore`. Documented the filter (A1 wire shape; A2 default namespace;
A3 static-value-wins + raw unquoted bytes; A4 missing/empty semantics; A5 boot-fatal validity;
§2.2 deferrals incl. `cookie`/`remove`/`encode`/typed-values/`regex_value_rewrite`/`response_rules`/
per-route; a determinism-classification paragraph). Added a minimal VALID `parse_bootstrap` seed
(concrete ports, `[header_to_metadata, router]` chain + `%DYNAMIC_METADATA%` logger), un-ignored in
`.gitignore` (the corpus dir is `*`-ignored by default — the seed is git-tracked, verified via
`git ls-files`). **NO new fuzz target, NO ci.yml change** (the existing `parse_bootstrap` job
auto-includes new corpus files). Seed parse verified `Ok` via a throwaway test (removed).
Spec ✅ (incl. the git-tracked check) / quality ✅ (A1/A4 consistency + determinism paragraph +
`cookie` bullet folded in via amend).

## Local pre-push verification (state-3 sanity; the FORMAL §7.5 gate is the state-4 session)

- `cargo fmt --all -- --check` → clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.
- `cargo test --workspace --exclude differential --no-fail-fast` → all green (`envoy-config` 495,
  `envoy-filter` 185, `envoy-http1` 129, `envoy-http2` 74, + all other crates 0 failures).
- Fixture `0042` (`header_to_metadata` differential) → green locally (10.7s).
- **KNOWN host-sensitive false-RED (NOT a regression):** `admin_config_dump_server_info` (an
  admin-dump differential) false-REDs on this Docker-Desktop host (bridge IP `192.168.65.2`); CI is
  authoritative (memories `differential-host-bridge-ip-192-168-65-2`,
  `host-docker-desktop-virtiofs-no-inotify`). Phase 34 touches no admin/config_dump code.

`#![forbid(unsafe_code)]` holds (D-3.8). No new crate, no new dependency, no new fuzz target (D-3.2).

## State

**state-3 implementation COMPLETE.** The §7.5 acceptance gate (full verification +
`superpowers:verification-before-completion`, with all command outputs quoted) is the SEPARATE
state-4 session (§5.1 one state per session). The 7 task commits + this docs commit are pushed;
CI is the authoritative gate from state-3 onward (the phase-33 lesson).

---

## §7.5 verification gate (state-4)

> **Lifecycle state 4 (verification).** Driven by `superpowers:verification-before-completion`
> (evidence before assertions). Every command below was run FRESH this session against `HEAD =
> 7c19803` (working tree clean) and its output quoted verbatim. The authoritative Linux gate is
> **CI run `28062068794` @ `7c19803` = `completed/success`** (2 jobs, both green):
> - **`build + test + lint`** — `success`, started 22:42:48Z / completed 22:47:25Z (4m37s): the full
>   `cargo build --workspace --all-targets` + `cargo clippy … -D warnings` + `cargo fmt --all --check`
>   + **`cargo test --workspace`** (incl. the FULL differential harness → Docker `envoyproxy/envoy:v1.33.0`)
>   + `cargo deny check`.
> - **`fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse, 30s each)`** —
>   `success`, started 22:42:47Z / completed 22:46:46Z (3m59s): the EXISTING `parse_bootstrap` target
>   (now WITH the new `hcm_header_to_metadata.yaml` seed) + the unchanged `accesslog_format_parse` —
>   **NO new fuzz target**.
>
> CI is authoritative for the host-sensitive items (b)/(c) and the full differential suite; the
> backend-routing / upstream / **admin-dump** fixtures false-RED on THIS Docker-Desktop host (bridge
> IP `192.168.65.2`) and h2spec §3.5/2 false-PASS-then-gate-REDs locally — NOT regressions (memories
> `differential-host-bridge-ip-192-168-65-2`, `host-docker-desktop-virtiofs-no-inotify`,
> `h2spec-3-5-2-preface-host-sensitive`). The fixture-`0042` access-log file-scrape is locally
> authoritative and was re-run green this session.

### (a) fixture `0042` differential — GREEN (locally authoritative)

`cargo build -p envoy-bin` first (stale-binary `unknown filter` lesson), then:

```
$ cargo test -p differential --test header_to_metadata -- --include-ignored
     Running tests/header_to_metadata.rs (target/debug/deps/header_to_metadata-64631224c0ceb457)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.76s
```

Both probes (header-present `x-tier: prod` → `m=GET tier=prod missns=-`; header-absent →
`m=GET tier=missing missns=-`) byte-identical cross-proxy. ✅

### (b) all `0001`–`0041` differential — GREEN (CI-authoritative)

CI run `28062068794`'s `build + test + lint` job ran the FULL `cargo test --workspace` (incl. the
complete differential harness against Docker `envoyproxy/envoy:v1.33.0`) green. The filter is INERT
outside its chain; the store / `%DYNAMIC_METADATA%` operator / H1/H2 threading are byte-preserved
(incl. `0012` default-format + `0041` set_metadata byte-identical). ✅

### (c) h2spec ≥95% — GREEN (CI-authoritative; unchanged — no HTTP/2 codec change)

No HTTP/2 codec change this phase. CI green. Locally the in-tree h2spec runner harness:

```
$ # (from the workspace test run below)
     Running tests/h2spec_runner.rs (target/debug/deps/h2spec_runner-cca743de2fbe3d4c)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

(The §3.5/2 preface known-failure is host-sensitive locally — CI is authoritative per memory
`h2spec-3-5-2-preface-host-sensitive`.) ✅

### (d) fuzz — `parse_bootstrap` + `accesslog_format_parse` clean, NO new target (CI-authoritative)

CI run `28062068794`'s `fuzz` job = `success` (3m59s), running the existing `parse_bootstrap` (with
the new `hcm_header_to_metadata.yaml` seed) + `accesslog_format_parse` (unchanged), 30s each.
Confirmed NO `ci.yml` change across the phase + the seed is git-tracked:

```
$ git diff --name-only bf42699^..7c19803 -- .github/workflows/ci.yml
ci.yml UNCHANGED across phase 34
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_to_metadata.yaml
crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_to_metadata.yaml
```
✅

### (e) build / clippy / fmt / test / deny — all clean (local fresh + CI-authoritative)

```
$ cargo build --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.53s

$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.09s

$ cargo fmt --all -- --check
FMT_EXIT=0        # no diff

$ cargo deny check
advisories ok, bans ok, licenses ok, sources ok
DENY_EXIT=0       # the 4 `license-not-encountered` lines are benign unmatched-allowance warnings
```

`cargo test --workspace` — locally run as `--exclude differential` (the full differential suite
false-REDs on this host; CI ran the complete `cargo test --workspace` green). All unit/integration
crates pass: `envoy-config` 495, `envoy-filter` 185, `envoy-http1` 129, `envoy-http2` 73+1, `envoy-cluster`
160, `envoy-listener` 36, `envoy-stats` 25, `envoy-tls` 15, `envoy-jwt` 12, `envoy-tcp` 11, `envoy-health`
8, all others 0-fail.

The lone local non-pass is the documented pre-existing H2 full-suite-contention RACE
`envoy-http2 client::tests::send_request_maps_h2_handshake_failure_to_typed_error` (unrelated to
phase 34) — re-run in isolation this session to confirm it is a race, not a regression:

```
$ cargo test -p envoy-http2 send_request_maps_h2_handshake_failure_to_typed_error
test client::tests::send_request_maps_h2_handshake_failure_to_typed_error ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 74 filtered out; finished in 0.00s
```

CI's full `cargo test --workspace` (no isolation harness) also passed it. ✅

### (f) `REVIEW.md` approved — DEFERRED to state-5

The code-review (`REVIEW.md`, `superpowers:requesting-code-review`) is the SEPARATE state-5 session
(§5.1 one state per session). Not part of this gate.

### Disposition

§7.5 (a)–(e) are **GREEN** (CI run `28062068794` + the local fixture-`0042` differential + the fresh
local build/clippy/fmt/test/deny sweep). `#![forbid(unsafe_code)]` holds (D-3.8); NO new crate /
dependency / fuzz-target (D-3.2). State-4 verification COMPLETE → advance STATE to
**state-4-complete / state-5-next** (`## Next expected skill` = `superpowers:requesting-code-review`).

## State

**state-4 verification COMPLETE.** §7.5 (a)–(e) green (evidence quoted above; CI `28062068794`
authoritative). The state-5 code-review (`REVIEW.md`) is the next session.
