# Phase 31 — `31-http-filter-cdn-loop` — PROGRESS

> **Lifecycle state 3 (implementation).** Running record authored by
> `superpowers:subagent-driven-development` (a fresh implementer subagent per task, SERIAL on `main`,
> each two-stage-reviewed: spec-compliance THEN code-quality via fresh `superpowers:code-reviewer`
> subagents, each committed separately). Scope locked by **ADR-0076**; §6.2 wire facts locked by
> **ADR-0077** (see `PLAN.md §A`). Read with zero prior context (D-3.4).

**Base SHA (state-3 start):** `e546c04d466ad6a15f1664209a7e038cfc659984` (`HEAD == origin/main`).

**Plan:** 7 tasks (1 parser → 2 config+validation → 3 filter → 4 fixture 0039 → 5 backstop →
6 fuzz → 7 BEHAVIOR_CONTRACT + close). See `PLAN.md`.

---

## Task log

<!-- Each task: status, commit SHA, two-stage review disposition, notes. Appended as tasks land. -->

### Task 1 — RFC 8586 CDN-Loop parser (the §A oracle) — ✅ COMPLETE

- **Commit:** `71e43cd9cd1b572dda7f94b8a148678a75a9ff2a` — `phase 31: Task 1 — RFC 8586 CDN-Loop parser (§A oracle) [ADR-0077]`.
- **Implemented:** `crates/envoy-filter/src/cdn_loop.rs` (new) + `pub mod cdn_loop;` in `lib.rs`. Pure, `unsafe`-free byte parser:
  `parse_cdn_loop(values: &[&[u8]]) -> Result<Vec<CdnInfo>, MalformedCdnLoop>` + `count_cdn_id(cdn_id, parsed) -> usize`
  + `CdnInfo { cdn_id: Vec<u8> }` (`is_empty_entry()`) + `MalformedCdnLoop` (thiserror unit error). 34 oracle tests; TDD FAIL→PASS shown; clippy/fmt clean.
- **Spec review:** ✅ compliant — all six §A.4 rules implemented AND each has a genuinely-asserting test; no over-build (no append logic / no param retention — correctly deferred to Task 3).
- **Code-quality review (`superpowers:code-reviewer`):** APPROVE — 0 Critical / 0 Important / 3 Minor (all optional doc niceties: a `parse_parameters` leading-`;` contract note; a `count_cdn_id` non-empty-needle doc note; an `is_empty_entry` coverage nicety). Panic-freedom on adversarial bytes empirically verified (exhaustive to len 4) — important for the Task 6 fuzz target.
- **Carry-forward to Task 3 (from implementer):** `CdnInfo` carries ONLY the trimmed id (enough to count). The empty-entry-preserving append (`a,` → `a,,mycdn.example`) must operate on the RAW coalesced header bytes directly (join original value(s) with `,{cdn_id}`), NOT via `CdnInfo` reserialization.
- **Locked nuance (pinned `quoted_value_comma_splits_per_locked`):** the list-split is a naive `,`-split per ADR-0077 §6.2, so a comma inside a quoted param value splits at list level → that fragment becomes an unterminated quote → malformed. Matches the locked decision.

### Task 2 — CdnLoopConfig schema + cdn_id validation (all-fatal) — ✅ COMPLETE

- **Commit:** `4acc071` — `phase 31: Task 2 — CdnLoopConfig schema + cdn_id validation (all-fatal) [ADR-0077]`.
- **Implemented:** `crates/envoy-config/src/bootstrap.rs`: `pub struct CdnLoopConfig { pub cdn_id: String, pub max_allowed_occurrences: u32 }` (`#[serde(deny_unknown_fields)]`, `max_allowed_occurrences` via `#[serde(default)]`); `HttpFilterTypedConfig::CdnLoop(CdnLoopConfig)` arm tagged `type.googleapis.com/envoy.extensions.filters.http.cdn_loop.v3.CdnLoopConfig`; local `const fn is_cdn_id_tchar` (independent copy of envoy-filter's predicate, commented — avoids a cross-crate dep); `validate_cdn_loop_config` wired into `validate_http_filters` (same site as Csrf/Buffer; reached via `parse_bootstrap`→`validate`→`validate_hcm`→`validate_http_filters`, boot-fatal). `lib.rs`: `ConfigError::CdnLoopEmptyCdnId { listener }` + `CdnLoopInvalidCdnId { listener, cdn_id }` + re-export. 9 new tests; `cargo test -p envoy-config` 477 passed; clippy/fmt clean.
- **Filter name string:** `envoy.filters.http.cdn_loop`.
- **Spec review:** ✅ compliant — `@type` byte-exact, default-0 + deny_unknown_fields + each invalid case tested, validation confirmed boot-fatal via the real parse path, NO new envoy-config→envoy-filter dep, no over-build.
- **Code-quality review:** APPROVE — 0 Critical / 0 Important / 3 Minor (optional: a full-path `max_allowed_occurrences` parse test; an explicit multibyte-`cdn_id` reject test; test-helper string-splice style). Sibling-convention fidelity excellent.

### Task 3 — CdnLoopFilter (9th HttpFilterInstance variant; decode count/append/reject) — ✅ COMPLETE

- **Commit:** `6ec91a6` — `phase 31: Task 3 — CdnLoopFilter (9th HttpFilterInstance variant) [ADR-0077]`.
- **Implemented:** `CdnLoopFilter` in `crates/envoy-filter/src/cdn_loop.rs` (`// ---- CdnLoopFilter ----` section, separate from the parser half) + wiring in `instance.rs` (variant decl, `build()` arm `HttpFilterTypedConfig::CdnLoop(cfg) => CdnLoopFilter::new(cfg)`, decode/encode arms; `apply_route_config` no-op via the existing `_ => {}`, comment updated). `LOOP_BODY` 44B / `MALFORMED_BODY` 35B consts; `loop_response()` (502 `Bad Gateway`) / `malformed_response()` (400 `Bad Request`), empty headers (decorators stamp content-type/length/server/connection downstream). Decode: collect all `cdn-loop` (case-insensitive) → coalesce → `parse_cdn_loop` (`Err`→400) → `count_cdn_id > max_allowed_occurrences` (strict `>`) →502 → else append comma-only on RAW bytes (empties preserved via `raw_values.join(b",")`+`,`+cdn_id; `retain_mut` keeps first matching key + drops rest), Continue. Encode inert. `CdnLoopFilter::new(cfg: &envoy_config::CdnLoopConfig) -> Self` (`pub(crate)`, infallible). 13 filter tests + dispatch test; `cargo test -p envoy-filter` 161 passed; clippy/fmt clean.
- **Spec review:** ✅ compliant — bodies byte-exact (44/35B, no newline), reason phrases correct, strict `>` boundary tested (max=1: 1→Continue, 2→502), comma-only raw-bytes append with empty preservation tested, multi-header coalescing for count AND append, case-insensitive name match, inert encode, exhaustive `build()` dispatch, no over-build.
- **Code-quality review:** APPROVE — 0 Critical / 0 Important / 4 Minor (all optional polish): (1) stale module doc-comment still says "parser" only — now parser+filter; (2) a `new_value.clone()` micro-clone in the `retain_mut` closure (cold path); (3) `encode_headers` doc lacks an ADR anchor; (4) `split_on_comma` one-line wrapper. `retain_mut` coalescing verified correct across zero/one/many matches + interleaved-header order preservation; no panic hazards on adversarial header sets.
- **CARRY-FORWARD to Task 4 (egress casing — the live-differential arbiter):** append-to-existing preserves the FIRST existing entry's key casing; add-when-absent pushes lowercase `cdn-loop`. The EXACT wire casing + append byte-shape is validated by Task 4's LOCAL echo-body differential against live Envoy v1.33.0 — **Task 3 may need a follow-up tweak if it diverges.** Also: appended value bridges raw bytes→String via `from_utf8_lossy` (lossless for valid-UTF-8 String-typed headers; confirm no non-UTF-8 path in the live fixture).
- **Phase-31 review carry-forwards (non-blocking Minors, weigh at state-5 / future touch):** T3-CQ-1 stale module doc; T3-CQ-2 retain_mut micro-clone; T3-CQ-3 encode doc anchor; T3-CQ-4 split_on_comma wrapper.

### Task 4 — fixture 0039-http-filter-cdn-loop (the STRONG differential) — ✅ COMPLETE

- **Commit:** `0611563af215ad17c6b8c5807cf0f2703cc29eb1` — `phase 31: Task 4 — fixture 0039-http-filter-cdn-loop differential (STRONG) [ADR-0077]`.
- **Implemented:** `tests/fixtures/0039-http-filter-cdn-loop/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}` + `tests/differential/tests/http_filter_cdn_loop.rs` (clone of `http_filter_csrf.rs`). H1 listener, HCM `[cdn_loop, router]` (`cdn_id: "mycdn.example"`, `max_allowed_occurrences: 0`) → real `http1-echo-server`. 5 probes, statuses `[200, 502, 200, 400, 200]`: P1 no-header→append bare; P2 self→502 (byte_exact 44B); P3 foreign→`othercdn.example,mycdn.example`; P4 malformed `"abc`→400 (byte_exact 35B); P5 trailing-comma→`othercdn.example,,mycdn.example` (empty preserved). Append proven cross-proxy via top-level `equivalence.response_body: byte_exact`; reject bodies pinned per-probe.
- **RAN LOCALLY GREEN** against live `envoyproxy/envoy:v1.33.0`: `test result: ok. 1 passed` — all 5 probes cross-proxy identical. 0013/0032 spot-check GREEN (individually / `--test-threads=1`).
- **NO Task-3 fix needed** — the append VALUE byte-shape (comma-only, empties preserved) matched live Envoy first try. The `http1-echo-server` lowercases reflected header names (`tests/helpers/http1-echo-server/src/main.rs:253`), so egress header-NAME casing is MASKED by this differential (not cross-proxy-proven) — only the appended VALUE is observed. This is fine (matches §A.2). **For Task 7 BEHAVIOR_CONTRACT:** lock the append value byte-shape; note the name-casing is not differentially pinned here.
- **Spec review:** ✅ compliant — 5 probes correct, P4 genuinely sends the literal `"abc` (malformed path, not degraded to a valid token), append observability is REAL (traced `run_http1_probe_bilateral`, not stubbed/ignored), byte-exact bodies 44/35 confirmed, chain order `[cdn_loop, router]` correct on both sides.
- **Code-quality review:** APPROVE — 0 Critical / 0 Important / 3 Minor (all cosmetic/in-convention): listener/stat_prefix naming tracks 0032 (not 0013) lineage; repeated prose across headers (0032 house style); §6.2 citation consistency. Minimal justified per-side divergence; no over-broad allow-lists (`connection` deliberately value-compared on rejects); clean clone, fmt/clippy clean.
- **State-4 note (harness):** running cdn_loop + csrf test binaries CONCURRENTLY can flake on Docker port contention (each passes individually / `--test-threads=1`) — a known harness characteristic, NOT a regression. Also: the harness runs the prebuilt `target/debug/envoy-bin`; rebuild (`cargo build -p envoy-bin`) before a local differential run after enum changes (CI rebuilds the workspace, so CI-only re-runs are unaffected).

### Task 5 — in-process backstop (parser edges + no-op witness) — ✅ COMPLETE

- **Commit:** `57cf0332548f1f3069200a0862e4e9587f44deca` — `phase 31: Task 5 — cdn_loop backstop (parser edges + no-op witness)`. TEST-ONLY (+232/-0), no production change → confirms Tasks 1/3 had no behavioral gap.
- **Implemented:** the headline **inert no-op witness** `no_cdn_loop_in_chain_leaves_cdn_loop_header_untouched` (`instance.rs` `mod tests`) — builds a LIVE `header_mutation+router` `FilterPipeline` (NO cdn_loop), drives 4 `cdn-loop` values (would-be self-loop / foreign / two malformed) through `decode_headers`, asserts `Continue` (never 400/502) + byte-unchanged header + the `x-witness` liveness mutation. **Empirically regression-proven** (prepending cdn_loop makes it FAIL) — independently re-confirmed by the spec reviewer. + 8 filter-level §A.4 edge tests (`cdn_loop.rs` `filter_tests`): case-variant→append, param-preserving append, param-ignoring loop→502, multi-header coalesce→**502**, empty-vs-malformed boundary, OWS-trim-but-raw-preserved, max=2 boundary. 161→170 tests; clippy/fmt clean.
- **Spec review:** ✅ compliant — witness genuine + regression-catching (independently verified), edges fill real gaps (no verbatim Task-1/3 dup), concrete assertions, no production change, no over-build.
- **Code-quality review:** APPROVE — 0 Critical / 0 Important / 3 Minor (stylistic, in-house-style): two multi-assertion tests (matches `buffer_pipeline_backstop` precedent); lowercase header-name construction; inline witness assertion. Naming "exemplary/intent-revealing"; idiomatic helper reuse.

### Task 6 — parse_bootstrap seed + cdn_loop_parse fuzz target — ✅ COMPLETE

- **Commit:** `e46518f` — `phase 31: Task 6 — parse_bootstrap seed + cdn_loop_parse fuzz target (§7.4)`. 7 files.
- **Implemented:** (1) NEW `cargo fuzz` scaffold for envoy-filter — `crates/envoy-filter/fuzz/{Cargo.toml,.gitignore,fuzz_targets/cdn_loop_parse.rs}` (mirrors `crates/envoy-jwt/fuzz/`: `#![no_main]`+`#![forbid(unsafe_code)]`, `libfuzzer-sys` 0.4, `envoy-filter` path dep, `[[bin]] cdn_loop_parse`, edition 2024). The target splits fuzz bytes on `b'\n'` → `Vec<&[u8]>` → `parse_cdn_loop(&slices)` → `count_cdn_id(b"mycdn.example", &parsed)` on Ok (exercises the multi-header coalescing surface; never panics). **Workspace isolation:** empty `[workspace]` table in the sub-crate + `"crates/envoy-filter/fuzz"` added to the root `Cargo.toml` `exclude` (double isolation, the jwt mechanism) → `cargo build --workspace`/clippy/`cargo deny` unaffected. (2) `parse_bootstrap` corpus seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/http_filter_cdn_loop.yaml` (valid standalone bootstrap with the cdn_loop filter; harness `{{…}}` tokens → concrete values) registered in `fuzz_corpus_seeds_parse_or_reject_cleanly` ("expected to parse" list, `// 31 Task 6`) + `.gitignore` allowlist; git-tracked.
- **Verified:** `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly` green; `crates/envoy-filter/fuzz` `cargo build` RC=0; `cargo build --workspace` RC=0. (`cargo fuzz` not installed locally → instrumented smoke run deferred to the state-4 CI gate, per discipline.)
- **Spec review:** ✅ compliant — target genuinely exercises the parser (not a no-op), isolation correct (double mechanism), seed valid/registered/tracked/in-the-parse-set, gates green.
- **Code-quality review:** APPROVE — 0 Critical / 0 Important / 2 Minor (non-actionable observations: seed carries a full router chain = house style not bloat; the `.gitignore` has no per-corpus allowlist because no committed corpus seed ships — appropriate). `let _ =` result-handling matches the jwt convention (no `black_box` needed — heap-allocating result has observable side effects).

### Task 7 — BEHAVIOR_CONTRACT cdn_loop subsection + state-3 close-out — ✅ COMPLETE (THIS commit)

- **Commit:** `phase 31: Task 7 — BEHAVIOR_CONTRACT cdn_loop row + state-3 close [ADR-0077]` (the state-3 close-out, authored by the controller — the STATE advance + relocation is controller work per the project doctrine; reviewed pre-commit for BEHAVIOR_CONTRACT accuracy + STATE relocation integrity).
- **Implemented:** (1) a BEHAVIOR_CONTRACT "HTTP filters" **cdn_loop subsection** (`docs/envoy-rust/BEHAVIOR_CONTRACT.md`, after the Buffer wire-shape block) — the §A facts: the filter overview + reject local-reply wire shapes (502 `The server has detected a loop between CDNs.` 44B / 400 `Invalid CDN-Loop header in request.` 35B, `connection: close` on rejects value-compared) + the COMMA-ONLY append byte-shape (empties preserved, egress NAME-casing not differentially pinned) + the strict RFC-7230/RFC-8586 parser grammar + the all-fatal config validity (`CdnLoopEmptyCdnId`/`CdnLoopInvalidCdnId`) + NO stat + deferred non-goals. (2) Finalized THIS `PROGRESS.md`. (3) Advanced STATE.md `31` state-2-complete/state-3-next → state-3-complete/state-4-next (next skill `superpowers:verification-before-completion`); the state-2 top-section blocks relocated VERBATIM to STATE_HISTORY.md per ADR-0035 (`TOTAL MISSING: 0`, 6 lines relocated, STATE.md → 122 lines) + appended the `### Phase-31 state-3 implementation` Notes subsection.
- **`cargo fmt --all -- --check`:** clean LOCALLY (the `envoy-rust-state4-ci-first-execution` discipline — pre-empts the mid-phase fmt red).

---

## State-3 summary

**ALL 7 PLAN tasks COMPLETE, each two-stage-reviewed → APPROVED (0 Critical / 0 Important across all 14 review passes; only non-blocking Minors).** Commit chain on `main`:

| Task | Commit | What |
|---|---|---|
| 1 | `71e43cd` | RFC 8586 `CDN-Loop` parser (`crates/envoy-filter/src/cdn_loop.rs` — the §A oracle) |
| 2 | `4acc071` | `CdnLoopConfig` + `@type` variant + all-fatal `cdn_id` validator (`crates/envoy-config`) |
| 3 | `6ec91a6` | `CdnLoopFilter` (9th `HttpFilterInstance` variant) decode count/append/reject |
| 4 | `0611563` | fixture `0039-http-filter-cdn-loop` — STRONG differential, RAN LOCALLY GREEN (5 probes) |
| 5 | `57cf033` | in-process backstop (parser edges + the regression-proven no-op witness) |
| 6 | `e46518f` | `parse_bootstrap` seed + the new `cdn_loop_parse` fuzz target (envoy-filter's first `fuzz/`) |
| 7 | (THIS) | BEHAVIOR_CONTRACT cdn_loop subsection + state-3 close |

**Key facts established:** the differential is byte-exact + DETERMINISTIC; NO Task-3 fix was needed (the append shape matched live Envoy first try); egress header-NAME casing is not differentially pinned (echo-server lowercases); `#![forbid(unsafe_code)]` holds throughout.

**Carry-forwards (NOT consumed — cdn_loop is an HTTP-filter fixture, not an LB hash-sweep):** empty-`metadata_match` doc-comment; M29-1/M29-2 + M30-1 (`Http1HashSweep` driver wording / `extract_marker`); M30-2 (`lb_policy` serde-default).

**Phase-31 internal review Minors (non-blocking, weigh at state-5):** T1 parser doc-notes; T3-CQ-1 stale module doc (now parser+filter); T3-CQ-2 `retain_mut` micro-clone; T3-CQ-3 encode doc anchor; T3-CQ-4 `split_on_comma` wrapper; T2/T4/T5/T6 cosmetic. None is an Envoy-equivalence divergence.

**Next session (state-4, per §5.1 — do NOT run this session):** `superpowers:verification-before-completion` — the §7.5 phase-done gate (39-fixture differential green on the AUTHORITATIVE Linux CI + h2spec ≥95% + both fuzzers clean + build/clippy/fmt/test/deny clean), quoting all outputs into this PROGRESS.md.


---

## State-4 verification (`superpowers:verification-before-completion`)

> Authored at the phase-31 state-4 §7.5 verification gate. **AUTHORITATIVE Linux CI run `27915239054`
> @ `a2051b2`** (the HEAD after the CI fuzz-wiring fix). All six §7.5 gates GREEN; quoted evidence below.

**One verification-driven fix landed:** the Task-6 `cdn_loop_parse` fuzz target was scaffolded but NOT
invoked by CI (the CI `fuzz` job ran only `parse_bootstrap` + `jwt_parse`). §7.4 requires every new
fuzzer to run short-budget in CI. Commit `a2051b2` (`phase 31: state-4 — wire cdn_loop_parse fuzz
target into CI (§7.4)`) adds the 30s `cdn_loop_parse` step + the `crates/envoy-filter/fuzz` cache
workspace + the job-name update (mirroring the jwt_parse precedent). CI-config only; no code change.

**§7.5 six-part gate — all GREEN (CI run `27915239054` @ `a2051b2`):**
- **(a) fixture `0039-http-filter-cdn-loop` green** — `test http_filter_cdn_loop_fixture ... ok` (build job).
- **(b) all `0001`–`0038` green SIMULTANEOUSLY** — the full `cargo test --workspace` ran all 39 differential
  fixture binaries; **0 non-zero-fail test results** across the entire build job (every `test result:`
  line carried `0 failed`).
- **(c) h2spec ≥95%** — `test h2spec_pass_rate_gate ... ok` (the h2spec-conformance ≥95% gate assertion).
- **(d) fuzz clean** — CI `fuzz` job ✓ (6m31s): `parse_bootstrap` + `jwt_parse` + the NEW `cdn_loop_parse`
  (`cargo +nightly fuzz run cdn_loop_parse -- -max_total_time=30`, no crash). LOCAL corroboration:
  `cdn_loop_parse` ran **6,756,700 runs in 31s, exit 0** (no panic on adversarial bytes).
- **(e) build/clippy/fmt/test/deny clean** — build job steps all ✓ (fmt, clippy, build, test, cargo deny
  check). LOCAL corroboration: `cargo build --workspace --all-targets` exit 0; `cargo fmt --all -- --check`
  clean (at the state-3 close).
- **(f) `REVIEW.md`** — the state-5 step (`superpowers:requesting-code-review`); NOT this session.

**CI flake encountered + cleared (NOT a phase-31 issue):** the first run of `27915239054` failed the build
job's `test` step at `happy_reload_flips_route_and_ticks_counters` (`crates/envoy-bin/tests/xds_rds_hot_reload.rs:476`)
with `ConnectionRefused` on envoy-bin admin-ready — a phase-26 RDS-hot-reload admin-readiness startup-race
flake, unrelated to cdn_loop (my commit touched ONLY `ci.yml`; the identical `cargo test --workspace`
passed on `583e7c2` 30 min earlier). Re-running the failed build job → GREEN, confirming the transient flake.
(Aligns with the `host-docker-desktop-virtiofs-no-inotify` / `envoy-rust-state4-ci-first-execution`
discipline: the hot-reload tests are timing-sensitive and CI-authoritative.)

**`#![forbid(unsafe_code)]` holds. STATE advanced → `31` state-4-complete/state-5-next** (next skill
`superpowers:requesting-code-review`); the superseded state-3 top-section blocks relocated verbatim to
STATE_HISTORY.md per ADR-0035.
