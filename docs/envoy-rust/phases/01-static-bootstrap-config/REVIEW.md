# Phase 01 REVIEW

- Base: e5afc35
- Head: 33665f0 (initial review); c528872 (after I1 close-out)
- Files: 33 changed (+6421 / -456) initial; +2 files (+84 lines) in I1 close-out
- Reviewed: 2026-04-24 (initial); 2026-04-24 re-reviewed (I1 close-out)
- Verdict: **Approved** — state 5 complete. I1 closed in-phase via ADR-0012; I3/I4/M1 tracked forward to phase 02 per reviewer recommendation. See §9 for re-review close-out.

---

## 1. Summary

Phase 01 lands a well-scoped, honestly-reviewed implementation of the
static bootstrap config loader, the project's first coverage-guided fuzz
target, and an admin HTTP `/ready` endpoint. The phase-done gate passed
locally and in CI (run 24891070573, both `build` and `fuzz` jobs green).

The work conforms to doctrine D-3.* on every axis I checked: permitted
foundations are respected, `#![forbid(unsafe_code)]` is at every
library-crate root (with the documented `#![no_main]` fuzz-target
exception also forbidding unsafe), `cargo deny check` is clean, the
stable toolchain pin is untouched, and the four new ADRs (0008–0011)
are substantive and match the code that ships. TDD discipline is
visible throughout PROGRESS.md — red runs with specific compiler errors
precede each implementation step.

The three late CI-fix commits (`5b852ce`, `97c1576`, `20ffb5b`) are
genuine root-cause fixes, not patch-over-symptoms. The chunked-encoding
decoder was a real blind spot in `drive_http_get` exposed by upstream
Envoy v1.33.0's `/ready` response framing; the SPEC §6 signpost 9
caveat ("fixed-shape … extend when a future fixture needs it") is
reasonable precedent. The nested `fuzz/rust-toolchain.toml` plus
`cargo +nightly fuzz run` in CI is the right combined idiom: the
workspace-root stable pin survives intact (D-3.9 preserved), developer
ergonomics are restored for local fuzz runs, and CI explicitly overrides
rather than relying on rustup's toolchain-resolution fallback.

The admin framing (`render_response`, `rfc7231_imf_fixdate`,
`civil_from_days`) is correct. I verified the IMF-fixdate tests against
RFC 7231 §7.1.1.1 (1994-11-06 example) and the 2000-02-29 century
leap-year case — Howard Hinnant's algorithm is transcribed correctly,
including the `y + 1` adjustment when `m <= 2`. The day-of-week table
is correctly rotated so `days=0` maps to "Thu" (1970-01-01 was a
Thursday).

The differential harness grammar extension is faithful to SPEC §D5:
tagged `Driver` enum, `#[serde(deny_unknown_fields)]` and
`#[serde(tag = "kind", rename_all = "snake_case")]` applied consistently;
the per-driver port-key substitution in `render_yaml` cleanly generalizes
the phase-00 `{{PORT}}` template without breaking the 0001 fixture.

I am approving the phase. The follow-ups below are all minor or small
and can be absorbed into phase 02's normal cleanup work without
blocking the advance; nothing in this review is a correctness blocker.

---

## 2. Strengths

- **SPEC adherence is near-total.** Every deliverable D1–D9 landed in
  the described shape. `crates/envoy-config/src/bootstrap.rs` lines
  8–85 implement the full 10-struct tree with `deny_unknown_fields`
  everywhere except `Node` (line 25–29), exactly as SPEC §D1 and
  signpost 8 call for. The `Node` asymmetry carries an in-code
  comment (lines 19–24) that names the xDS-family tightening event,
  so future contributors won't silently tighten it.
- **Validate rules match SPEC §D1 verbatim.** `bootstrap.rs:87–108`
  — `TooManyListeners` for `>1`, `NoRuntime` for `admin.is_none() &&
  listeners.is_empty()`, per-filter `ECHO_FILTER` check. All three
  rules have dedicated regression tests at lines 272–308.
- **N2 closure is comprehensive.** `rejects_unknown_static_resources_field`
  through `rejects_unknown_network_filter_field`
  (`bootstrap.rs:379–446`) close the deny-unknown-fields gap identified
  by phase-00 REVIEW N2 at the five deeper structs. The three-probe
  `assert_unknown_field` helper (lines 312–324) is technically
  sound — the PLAN-defect deviation in PROGRESS is accurately labeled
  (Display of the outer `ConfigError::Yaml` variant does not flatten
  the inner serde_yaml error text; only Debug or `err.source()`
  does).
- **Admin IMF-fixdate is correct and well-tested.**
  `admin.rs:49–95` includes three unit tests covering the UNIX
  epoch, the RFC 7231 example timestamp, and the 2000 century
  leap-year (`imf_fixdate_leap_year_boundary`). The day-of-week
  table rotation is subtle; the epoch test pins it against drift.
- **Response framing shape matches SPEC §D3 step 3 byte-for-byte.**
  `render_response_has_expected_shape_and_body` (admin.rs:409–420)
  asserts the exact header block, CRLF-CRLF separator, and body.
  The `server: envoy-rust` divergence from upstream is documented
  inline (admin.rs:6–7) and cross-referenced to ADR-0011.
- **Graceful drain in `admin::serve` mirrors `echo::serve`.**
  `admin.rs:115–154` uses the same `tokio::select!` + `JoinSet` +
  `timeout(DRAIN_TIMEOUT, …)` shape as `echo.rs:20–60`. The 5s
  budget matches. Reviewers can diff the two modules side by side.
- **`main::run` coordination is clean.** `main.rs:48–105` spawns
  each listener onto a shared `JoinSet<Result<()>>` coordinated by
  a single `CancellationToken`. Admin-only, echo-only, and both-at-
  once configurations all work by construction. The integration
  test `admin_only.rs:36–86` backs the admin-only branch.
- **Four ADRs 0008–0011 are substantive.** Each has a real Options-
  Considered list, a decision-rationale that holds water, and
  consequences that are exercised by the committed code. ADR-0010
  specifically rejects the nested-rust-toolchain.toml approach, but
  the final landed implementation (`5b852ce`/`97c1576`) adds it anyway
  — see Important issue below on reconciling ADR-0010 with the
  realized code.
- **Fuzz target gate is correct.** `parse_bootstrap.rs:6–10` gates
  on `std::str::from_utf8` exactly as SPEC §6 signpost 5 prescribes,
  and the seed corpus (`minimal.yaml`) is a valid admin-only bootstrap
  that parses cleanly — verified by manual read against the schema.
- **Fuzz subcrate is correctly quarantined.** `Cargo.lock` does not
  list `libfuzzer-sys` (confirmed by grep); the dep only lives in
  the workspace-excluded `crates/envoy-config/fuzz/Cargo.toml`. The
  added `[workspace]` stanza on line 15–16 prevents cargo from walking
  up into the excluded subcrate during fuzz invocation, which was
  the root cause of the original CI failure.
- **`deny.toml` handling of the path-dep wildcard is the right idiom.**
  The `allow-wildcard-paths = true` addition (deny.toml:59) narrows
  the exemption to path dependencies only while keeping the
  `wildcards = "deny"` posture from ADR-0005. PROGRESS Task 5's own
  re-review correction trajectory (initially weakening to
  `wildcards = "warn"`, then tightening) was self-audited — that's
  the behavior the doctrine wants.
- **Differential harness `drive_http_get` handles three response
  framings correctly.** `lib.rs:186–288` — content-length, chunked
  (Envoy v1.33.0's `/ready`), and connection-close. The `decode_chunked`
  helper (lines 294–326) correctly handles chunk extensions
  (`split(';').next()`), the terminating zero-chunk, and trailer
  bytes (ignored). The check at line 315–321 prevents buffer
  overrun on truncated chunks.
- **Fixture 0002 adheres to SPEC §D7 exactly.** Divergence between
  `envoy.yaml` (`0.0.0.0`) and `envoy-rust.yaml` (`127.0.0.1`) is
  the sole difference; both templates share the `{{ADMIN_PORT}}`
  token; `expectations.yaml` asserts status + body only per
  ADR-0011; README matches the SPEC-prescribed text.
- **Integration test backstop works.** `admin_only.rs:36–86` spawns
  `envoy-bin` as a subprocess with an admin-only config, waits for
  readiness with exponential backoff (matching the phase-00
  `wait_accept_ready` shape), and asserts status + body. A solid
  stable-only gate even when Docker isn't available.
- **Argv extraction improves main.rs clarity.**
  `crates/envoy-bin/src/argv.rs:1–102` is self-contained; 6
  dedicated tests including the duplicate-`-c` case. `main.rs`
  dropped from 217 to 116 lines (PROGRESS Task 12).

---

## 3. Issues

### Critical

None.

### Important

**I1. ADR-0010 "rejected" option actually shipped (implementation drift).**
`DECISIONS.md:203–205` reads:

> Options considered:
> - … Add a nested `rust-toolchain.toml` under `crates/envoy-config/fuzz/`.
>   **Rejected** — that crate is workspace-excluded (ADR-0008
>   consequence); cargo toolchain-override semantics across workspace
>   boundaries are surprising and brittle.

But `crates/envoy-config/fuzz/rust-toolchain.toml` now exists (added in
commit `97c1576`). The CI job also uses the explicit `cargo +nightly`
prefix (`.github/workflows/ci.yml:79`), which is what ADR-0010 chose.
The committed implementation therefore uses **both** the rejected
option and the decided option simultaneously. Reading PROGRESS.md State-4
§Fixes Applied, the nested toolchain file is explicitly for "local
developer ergonomics" and the `+nightly` prefix is for CI — reasonable
in practice, but ADR-0010 as written tells the next reviewer the
nested file doesn't exist and shouldn't. Action: either add an
ADR-0012 that supersedes ADR-0010 narrowly on this point (describing
"both" as the decision and the reasoning — CI uses `+nightly`, local
dev picks up the nested toolchain override), or edit ADR-0010's
Options-Considered section to flip that bullet from "rejected" to
"accepted in combination with explicit `+nightly`". Per doctrine
D-3.5 (append-only ADRs), the first option — a new ADR — is the
doctrinally correct one. STATE.md notes only the existence of the
CI-fix commits; it does not reconcile the ADR drift.

**I2. `#![forbid(unsafe_code)]` missing on `crates/envoy-bin/src/main.rs`
other source files.** Only `main.rs` has the forbid attribute.
`admin.rs`, `argv.rs`, and `echo.rs` are modules under `main.rs`,
so the crate-root forbid inherits correctly — this is fine for
Rust semantics. But SPEC §D1 says "`#![forbid(unsafe_code)]` at
every library-crate root." For `envoy-bin` (a binary crate), the
crate root is `main.rs`; that's properly forbidden. `envoy-config`
and `differential` also correctly carry forbid at their lib.rs
roots. The fuzz target carries forbid at its own `#![no_main]` file
(`parse_bootstrap.rs:2`), which exceeds the SPEC §D2 relaxation.
All correct. I checked this because SPEC §D2 flagged the idiomatic
tension; the implementation resolves it the stronger way.
**No action required** — this is a strength noted as an "Important"
rather than a "Strength" because it's worth affirming on the record.

**I3. The `decode_chunked` helper lacks a unit test.**
`lib.rs:294–326` is a new helper introduced in commit `5b852ce`
to close the state-4 gate; it was motivated by Envoy v1.33.0's
chunked `/ready` framing. Its behavior is exercised transitively
by the `admin_ready_fixture` Docker-gated test — but that test only
runs in CI, and its pass/fail signal is an entire differential
run, not a focused chunked-decoder assertion. The helper has
several edge cases worth pinning: empty chunked stream
(`0\r\n\r\n`), chunk with extension (`5;ext=val\r\nhello\r\n0\r\n\r\n`),
truncated chunk, trailing bytes after the zero-chunk. Recommend
four unit tests at the tests module level of `tests/differential/src/lib.rs`
before phase 02 lands. The helper is 32 lines and self-contained;
cost is minimal. Precedent: the three `drive_http_get_*` tests
live in the same module.

**I4. Admin 8 KiB header cap effective ceiling is ~9 KiB.**
`admin.rs:160` checks `buf.len() >= MAX_REQUEST_HEAD` **before**
each read call, but the subsequent `scratch` read can be up to 1024
bytes (admin.rs:158). So a request with exactly 8191 bytes followed
by a 1024-byte read will land at 9215 bytes in the buffer before
the next iteration's cap check fires. The test
`rejects_oversized_request_headers` (admin.rs:302–347) writes 9000+37
= ~9037 bytes and observes the 431, which is well over 8192 — so
the test passes, but it doesn't pin the 8 KiB value to within a
kilobyte. SPEC §6 signpost 4 describes "8 KiB" as a "deliberate
phase-01 choice"; phase 08 may revisit. For phase 01, consider
either (a) document in the admin.rs rustdoc that the effective cap
is `MAX_REQUEST_HEAD + scratch.len()` in the worst case, or (b)
tighten the read to `MAX_REQUEST_HEAD - buf.len()` via a slice.
Option (b) is ~2 lines of change and removes the imprecision.
Not a correctness bug — the handler's memory is still bounded —
but worth tightening before a phase that actually cares.

### Minor

**M1. Stale `TODO(phase-01)` in `tests/differential/src/subject.rs:25–32`.**
Phase-00 REVIEW I3 handed the SIGKILL→SIGTERM functional switch to
phase 01 "under its own ADR." Phase 01 did not pick it up, and
STATE.md acknowledges the continued deferral. The review ground
rules say I3 is out of scope for phase 01. However, the TODO text
still reads "deferred to phase 01 under its own ADR," which is now
misleading for a stranger reading the code after phase 01 is closed.
Recommend updating the TODO block to point at the current deferral
target (likely phase 04, since that's when the HCM response-framing
pipeline lands and `nix`'s process-signaling surface is more likely
to arrive alongside other POSIX-ism needs). Small doc touch; no
functional change.

**M2. `tests/fixtures/0002-static-admin-ready/README.md` still contains
a forward-looking quirk note that could now be backfilled with
the observed reality.** Lines 24–28 say "If upstream Envoy rejects
this YAML at container start, add `access_log_path`…". The state-4
gate passed, so we now know v1.33.0 *doesn't* reject the YAML as
shipped. The hypothetical can be either deleted or downgraded to a
past-tense "checked on v1.33.0; no such field required" note. Not
critical, but the current text implies uncertainty that phase-01
execution has resolved.

**M3. `render_response` helpers are `pub(crate)` but only called from
within `admin.rs`.** `admin.rs:21, 25` — `render_response` and
`render_response_at` have `pub(crate)` visibility, but every caller
is inside the same module. `fn` (private) would be accurate. The
cost is imperceptible; the inconsistency is a readability nit. If
a later phase needs cross-module use (e.g., phase 08's `/stats`),
flipping back to `pub(crate)` is trivial.

**M4. `HttpResponse::headers` is captured but unused in equivalence.**
`lib.rs:173–179` — the `#[allow(dead_code)]` on `headers` is accurate
(ADR-0011 explicitly defers header equivalence to phase 04). The
field exists for debug tracing; consider logging it at `debug!` level
in `drive_http_get` on error paths to make the allocation pay for
itself, or document in the rustdoc that it's retained for future
use. Today the field is allocated on every response but never read
by anything except the test harness; that's harmless but wasteful.

**M5. `admin_ready_fixture` test's host path construction is repeated
from `echo_fixture`.** `tests/differential/tests/admin_ready.rs:10–13`
duplicates the `env!("CARGO_MANIFEST_DIR") + three join("..")`
pattern from `tests/differential/tests/echo.rs`. A small
`fixture_path(name: &str) -> PathBuf` helper in `tests/differential/src/lib.rs`
would remove the repetition before the third fixture arrives.
Not urgent for phase 01 with only two fixtures.

**M6. PLAN-text miscount in Task 9.** PROGRESS.md line 66 calls out
that PLAN line 1519 undercounts by 2 (missing the phase-00 echo
tests). Honest self-audit; noting here for the record because it's
the kind of thing that's easy to miss under pressure. The fact
that PROGRESS flags it is a strength, not a weakness.

### Suggestions

**S1. Consider adding a `parse_bootstrap_bytes` fuzz target in a
later phase.** SPEC §6 signpost 5 correctly gates the current fuzz
on UTF-8 (mirrors `read_to_string`). When a phase eventually adds
a bytes-oriented config path (xDS Protobuf? raw filesystem blob?),
a sibling fuzz target that doesn't gate on UTF-8 would complement.
The current scope is right for phase 01; this is just forward-
looking.

**S2. Once phase 02's TCP proxy fixture lands, revisit
`drive_tcp`'s `payload.len()` assumption.** ADR-0006/0007 are
currently correct for the echo filter's 1:1 byte contract, but
a TCP proxy that multiplexes or adds framing overhead will
invalidate `read_exact(payload.len())`. The `response_length`
grammar extension ADR-0007 flagged as option (b) is the natural
landing spot.

**S3. `argv::parse_argv`'s `Trailing` error for a second `-c` flag
is technically misnamed.** `argv.rs:43` — when the user passes
`-c /a -c /b`, the code pulls `-c` as arg, then `/b` as value, then
rejects with `Trailing("/b")`. The error message "unexpected
trailing argument: /b" is correct at the token level, but the
underlying error is "duplicate `-c` flag", which a user would
probably diagnose faster. A dedicated `DuplicateConfigFlag` variant
(or reusing `UnknownFlag("-c")` with a duplicate-specific message)
could land when argv grows past the one-flag MVP. Deferred until
then.

---

## 4. Requirements coverage

Against SPEC §1 (acceptance signal, six bullets a–f):

| Bullet | Requirement | Verdict | Evidence |
| --- | --- | --- | --- |
| (a) | fixture 0002 green | Met | CI run 24891070573, `admin_ready_fixture ... ok` |
| (b) | fixture 0001 green post-migration | Met | Same CI run, `echo_fixture ... ok` |
| (c) | no conformance suites | Met | None land this phase; phase 05 schedules `h2spec` |
| (d) | fuzz 30s clean | Met | Same CI run, `fuzz` job `success` |
| (e) | stable CI gate clean | Met | `build` job `success`; all 5 local commands exit 0 |
| (f) | REVIEW approved | This doc | Approved with follow-ups |

Against SPEC §3 deliverables D1–D9:

- D1 (envoy-config crate): **Met.** 10 structs, validate, 21 tests.
- D2 (fuzz subcrate): **Met.** Target, corpus, gitignore, workspace
  exclusion, CI invocation. The `+nightly` + nested toolchain dual
  approach needs the ADR reconciliation per I1.
- D3 (admin endpoint): **Met.** `/ready`, 404 fallback, 431 on
  oversized headers, hand-rolled IMF-fixdate, 5 unit tests. I4 is
  the one quibble.
- D4 (binary wiring): **Met.** `CancellationToken` + `JoinSet`,
  admin-only/echo-only/both all work, `argv.rs` extracted,
  `admin_only.rs` integration test green.
- D5 (harness grammar): **Met.** Tagged `Driver`, `drive_http_get`
  (3 framings), `render_yaml` generalized, `assert_equivalence`
  extracted. The chunked decoder extension is a legitimate use
  of SPEC §6 signpost 9's extension clause.
- D6 (fixture 0001 migration): **Met.** YAML shape updated, README
  migration note added, regression test
  `fixture_0001_expectations_parses_as_tcp_echo`.
- D7 (fixture 0002): **Met.** All four files present, divergence
  limited to bind address.
- D8 (CI workflow): **Met.** Parallel `build`/`fuzz` jobs, `+nightly`
  invocation, `Swatinem/rust-cache` `workspaces` field correct,
  `cargo install cargo-fuzz --locked` pinned.
- D9 (ADRs): **Met with caveat.** ADR-0008, 0009, 0010, 0011 landed.
  ADR-0010 has the implementation-drift described in I1.

Against BEHAVIOR_CONTRACT.md §7.2 usage:
- Row 1 (response status exact): first use lands in fixture 0002's
  `response_status: exact`. Correct.
- Row 2 (response body byte-exact): continues from phase 00; now
  exercised by both fixtures.
- Header allow-list, stat-name, access-log, xDS-wire, timing: all
  remain empty, consistent with ADR-0011.

---

## 5. Plan-deviation audit

PROGRESS.md flags ~11 deviations across tasks 1, 4, 5, 6, 9, 11, 13,
14, 15. Audit:

- **Task 1 (SHA-patch follow-up convention):** Plan-defect; switch is
  correct. No doctrine weakening.
- **Task 4 (`assert_unknown_field` 3-probe helper):** Plan-defect
  correctly identified; Display of the outer `ConfigError::Yaml`
  variant does not propagate the wrapped serde_yaml text. The
  3-probe fallback (`{err:?}`, `{err}`, `{err:#?}`) works because
  `Debug` of `thiserror::Error` walks `#[from]` source chains.
  Acceptable.
- **Task 5 Deviation 1 (thiserror 1 → 2):** Plan-defect; workspace
  was already on thiserror 2.x transitively, so the single-version
  collapse is the right move. API-compatible per confirmed tests.
- **Task 5 Deviation 2 (`allow-wildcard-paths`):** Initially weakened
  `wildcards = "warn"`, then re-tightened to `wildcards = "deny"` +
  `allow-wildcard-paths = true`. The re-review fix preserves
  ADR-0005's supply-chain stance exactly; `allow-wildcard-paths` is
  cargo-deny's documented idiom for path deps. No ADR required;
  the mechanism is narrower than the old setting. **Audit verdict:
  correct.**
- **Task 6 (seed file shape):** Admin-only rather than phase-00
  typed_config because `NetworkFilter::deny_unknown_fields`
  legitimately rejects the typed_config block. Plan-minor; choice
  of seed does not limit the fuzzer's search space meaningfully.
- **Task 9 (PLAN line 1519 miscount):** Cosmetic plan-prose error;
  code is correct.
- **Task 11 (tokio-util ["default"] features + process):**
  Deviation-as-declared; both permitted per D-3.2. Acceptable.
- **Task 13 (intentional red CI between Task 13 and Task 16):**
  The fixture's YAML shape and the harness grammar must update
  together; doing them in separate commits means the interleaving
  commits are red. This is the price of atomic-per-task commits,
  and PROGRESS flags it explicitly. Acceptable.
- **Task 14 (macOS RST in the test server helper):** Platform-specific
  shim; the core `drive_http_get` is unchanged. Acceptable.
- **Task 15 (PLAN test-count mismatch: 21 → actual 20):**
  Cosmetic plan-prose error; code is correct.

None of the labeled PLAN-defect deviations silently weakens doctrine.
Task 5 Deviation 2's trajectory (initial weakening → re-review
tightening) is exactly the audit loop that ADR-0005 demands. The
re-review fix commits referenced by PROGRESS (`0eae0b8`, `bf311e1`,
`93d4cbc`, `b0c06a1`, `8e40d96`, `a6c20ab`) show internal review
discipline.

---

## 6. Doctrine audit

- **D-3.1 (behavioral fidelity):** admin `/ready` returns a body
  that matches upstream's `LIVE\n`; status matches via the exact
  rule. Verified by CI fixture 0002.
- **D-3.2 (permitted foundations):** new deps are `httparse`
  (differential + envoy-bin), `tokio-util` (envoy-bin), `thiserror`
  bump 1 → 2 (envoy-config), `tempfile` (envoy-bin dev-dep). All
  on D-3.2 or previously present. Fuzz deps (`cargo-fuzz`,
  `libfuzzer-sys`) covered by ADR-0009 and isolated to the
  workspace-excluded fuzz subcrate; `Cargo.lock` confirms neither
  leaks into the main dep graph.
- **D-3.4 (designed for strangers):** SPEC, PLAN, PROGRESS, and
  ADRs each read cleanly without prior context. The `Node` open-schema
  asymmetry has a pin-comment (`bootstrap.rs:19–24`).
- **D-3.6 (no stubs/unimplemented/TODO gates):** one TODO remains,
  in `tests/differential/src/subject.rs:25` — inherited from
  phase 00, not introduced in phase 01; explicitly deferred per
  STATE.md and REVIEW ground rules. See M1.
- **D-3.7 (cargo-deny policy reflects every new dep):** `deny.toml`
  unchanged except for `allow-wildcard-paths = true`. The fuzz
  subcrate's `libfuzzer-sys` dep is outside the workspace and not
  audited here; ADR-0009 acknowledges the potential advisory
  surface explicitly.
- **D-3.8 (`#![forbid(unsafe_code)]`):** at every library-crate
  root (`envoy-config/src/lib.rs:1`, `envoy-bin/src/main.rs:1`,
  `differential/src/lib.rs:1`); at the fuzz target file
  (`parse_bootstrap.rs:2`), which is stricter than SPEC §D2
  relaxation allows. Good.
- **D-3.9 (stable toolchain pin at repo root):** `rust-toolchain.toml:2`
  still pins `1.95.0` stable. The nested
  `crates/envoy-config/fuzz/rust-toolchain.toml` correctly applies
  only when cargo is invoked from inside the excluded fuzz subcrate
  (verified by the workspace-excluded status and the explicit
  comment). D-3.9 is undisturbed for every `cargo build` / `cargo test`
  path. See I1 for the ADR-0010 drift note on whether the nested
  file is the same approach ADR-0010 rejected.

---

## 7. Recommendation

**Verdict: Approved with follow-ups.**

Phase 01 is done. Advance the state machine to state 6 (final commit
/ phase close) with these follow-ups tracked forward (none block
phase 02 start):

- **I1 (ADR-0010 reconciliation)** — land as a short ADR-0012 before
  phase 02 state 4 so the next phase's REVIEW doesn't hit the same
  drift. Expected size: ~30 lines in DECISIONS.md.
- **I3 (`decode_chunked` unit tests)** — add four tests in the
  early stretch of phase 02 (empty, extension, truncated, trailer).
- **I4 (admin 8 KiB cap tightening)** — a 2-line change; do it
  alongside the first phase-02 admin change (phase 02 touches
  admin thinly via the cluster-manager integration).
- **M1 (stale TODO)** — update during phase-02 first touch of
  `tests/differential/src/subject.rs`.
- **M2 (fixture 0002 README quirk note)** — optional; good hygiene.
- **M3, M4, M5, M6** — discretionary cleanup.
- **S1, S2, S3** — long-horizon; capture in phase-02 or phase-04
  PROGRESS notes as they become actionable.

The code is production-ready for its stated phase scope. The harness,
the admin endpoint, the fuzz target, and the config crate extraction
all hold up to scrutiny. The ADR trail matches the code except for
the narrow drift called out in I1. The state-4 gate is genuinely
green and the PROGRESS.md account of how it got there (including the
three late CI-fix commits) is an honest, well-documented record.

---

## 8. Files reviewed (absolute paths)

- `/Users/esa/git/envoy-rust/crates/envoy-config/Cargo.toml`
- `/Users/esa/git/envoy-rust/crates/envoy-config/src/lib.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-config/src/bootstrap.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-config/fuzz/Cargo.toml`
- `/Users/esa/git/envoy-rust/crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-config/fuzz/corpus/parse_bootstrap/minimal.yaml`
- `/Users/esa/git/envoy-rust/crates/envoy-config/fuzz/rust-toolchain.toml`
- `/Users/esa/git/envoy-rust/crates/envoy-bin/Cargo.toml`
- `/Users/esa/git/envoy-rust/crates/envoy-bin/src/main.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-bin/src/admin.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-bin/src/argv.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-bin/src/echo.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-bin/tests/admin_only.rs`
- `/Users/esa/git/envoy-rust/tests/differential/Cargo.toml`
- `/Users/esa/git/envoy-rust/tests/differential/src/lib.rs`
- `/Users/esa/git/envoy-rust/tests/differential/src/subject.rs`
- `/Users/esa/git/envoy-rust/tests/differential/tests/admin_ready.rs`
- `/Users/esa/git/envoy-rust/tests/fixtures/0001-tcp-echo/expectations.yaml`
- `/Users/esa/git/envoy-rust/tests/fixtures/0001-tcp-echo/README.md`
- `/Users/esa/git/envoy-rust/tests/fixtures/0002-static-admin-ready/envoy.yaml`
- `/Users/esa/git/envoy-rust/tests/fixtures/0002-static-admin-ready/envoy-rust.yaml`
- `/Users/esa/git/envoy-rust/tests/fixtures/0002-static-admin-ready/expectations.yaml`
- `/Users/esa/git/envoy-rust/tests/fixtures/0002-static-admin-ready/README.md`
- `/Users/esa/git/envoy-rust/.github/workflows/ci.yml`
- `/Users/esa/git/envoy-rust/deny.toml`
- `/Users/esa/git/envoy-rust/Cargo.toml`
- `/Users/esa/git/envoy-rust/Cargo.lock` (spot-checked)
- `/Users/esa/git/envoy-rust/rust-toolchain.toml`
- `/Users/esa/git/envoy-rust/docs/envoy-rust/DECISIONS.md` (ADR-0008…0011)
- `/Users/esa/git/envoy-rust/docs/envoy-rust/STATE.md`
- `/Users/esa/git/envoy-rust/docs/envoy-rust/phases/01-static-bootstrap-config/SPEC.md`
- `/Users/esa/git/envoy-rust/docs/envoy-rust/phases/01-static-bootstrap-config/PROGRESS.md`
- `/Users/esa/git/envoy-rust/docs/envoy-rust/phases/00-bootstrap/REVIEW.md` (for phase-00 deferrals)

---

## 9. State-5 re-review — I1 close-out (2026-04-24)

Narrow re-review by `superpowers:code-reviewer` of commits
`33665f0..c528872` (three docs-only commits addressing I1 only; other
Important / Minor findings remain tracked forward per explicit user
direction).

### Commits in scope

- `bda4e52` — `phase 01: ADR-0012 — nested nightly pin in fuzz subcrate narrowly supersedes ADR-0010 [ADR-0012]`
- `e32240c` — `phase 01: PROGRESS — state 5 re-review fix (ADR-0012 lands for I1)`
- `c528872` — `phase 01: PROGRESS — state 5 re-verification gate (CI run 24893585436)`

### Re-review checks

| Check | Result |
|---|---|
| Scope-creep (docs-only diff) | PASS — only `DECISIONS.md` (+21) and `PROGRESS.md` (+63); code / SPEC / PLAN / STATE / ROADMAP / CI-config / `rust-toolchain.toml` (root & nested) all untouched |
| Append-only doctrine D-3.5 (ADR-0010 unedited) | PASS — `git diff` shows a single hunk `@@ -228,3 +228,24 @@` in `DECISIONS.md`; ADR-0010 (lines 195–215) is byte-identical including its rejection bullet on line 204 |
| ADR-0012 well-formed (header, status, context, options, decision, rationale, consequences, provenance) | PASS — matches neighboring-ADR voice; supersession scope is explicit and narrow; ADR-0010's original "surprising and brittle" concern is addressed on its merits rather than dismissed |
| ADR-0012 technical claims (rustup directory-scoped resolution; `+toolchain` flag beats `rust-toolchain.toml` file) | PASS in substance — claim is correct per `rustup` documented precedence (`+toolchain` > `RUSTUP_TOOLCHAIN` > directory override > `rust-toolchain.toml`). Could be strengthened by a URL citation; non-blocking trivial polish |
| PROGRESS-entry honesty (pure-doctrine tidy framing; Re-verification gate evidence) | PASS |
| Gate-evidence verification | CONFIRMED — CI run `24893585436` on HEAD `e32240c` both jobs `success` (verified via `gh run view`); post-evidence PROGRESS-only commit `c528872` also green (run `24893679285`) though a PROGRESS-only commit cannot regress the build |
| Commit-message hygiene (SPEC §8-adjacent, `[ADR-0012]` tag on the landing commit, Co-Authored-By consistent with phase-01 precedent) | PASS |

### Re-review verdict

**I1 Closed — no new issues.** Main session advances REVIEW.md verdict and
proceeds to state 6. One trivial follow-up noted (rustup-book URL for the
`+toolchain`-beats-file precedence claim in ADR-0012 Rationale) is optional
polish, not a blocker; folded into phase-02 starter items if desired.

### Tracked forward (unchanged from initial review)

These items remain rollovers to phase 02 per the initial-review reviewer's
explicit recommendation; the user's state-5 remediation scope was I1 only:

- **I3** — add 4 unit tests for `decode_chunked` (empty, extension, truncated, trailer) in `tests/differential/src/lib.rs`.
- **I4** — tighten admin 8 KiB header cap from "effectively ~9 KiB" to an exact boundary in `crates/envoy-bin/src/admin.rs`.
- **M1** — retarget the stale `TODO(phase-01)` in `tests/differential/src/subject.rs:25–32` (the phase-00 I3 SIGKILL→SIGTERM deferral; phase 01 did not pick it up so the target moves to phase 04 or a later phase that actually takes the `nix` dep).

Other Minors (M2–M6) remain as "nice to have" references in this REVIEW.
They are not tracked forward as explicit phase-02 starter items.
