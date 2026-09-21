# Phase 115 — PROGRESS

> §5 **state 3** — the implementation of
> `docs/envoy-rust/phases/115-http-filter-health-check/PLAN.md`.
> Appended to on each task completion, with REAL quoted command output. Written
> for a stranger with zero prior context (D-3.4).
>
> **The unit:** `envoy.filters.http.health_check` in **non-pass-through mode** —
> an HTTP filter that answers a downstream liveness probe AT THE PROXY with an
> empty 200 carrying one `x-envoy-upstream-healthchecked-cluster` header, instead
> of forwarding the request. It is the THIRTEENTH production
> `HttpFilterInstance` variant. Witnessed by TWO new differential fixtures:
> `0095-http-filter-health-check` (the wire) and
> `0096-http-filter-health-check-stats` (the counters).
>
> **Where `PLAN.md` and `SPEC.md` disagree, `PLAN.md` wins** (`ADR-0201`, which
> corrects seven landed `SPEC.md` claims and adds the stat surface the SPEC
> omits).

---

## Session preconditions, re-derived from disk before Task 1

`git rev-parse HEAD` = `04661b7b862faac52e55633871f47d0fea975ec3`, the phase-115
§5 state-2 **CI-record** commit; `git status --porcelain` empty at entry; branch
`main`; `git fetch origin --prune` exit **0** (run BARE, not through a pipe) with
`origin/main` at the SAME SHA.

**No outstanding CI record is inherited, detected STRUCTURALLY** — the
`## Last commit` block of `STATE.md` was read and it already carries a
CI-confirmed answer: run `35153241604`, attempt 1, `success`, identity
`binaries=170 passed=2315 failed=0` on `0cb1f677745705bb254a0a4eb91b0dc372f13c0e`.
The block was never grepped for the outstanding-record marker text, which the
Standing-traps series quotes.

**The §5 state-3 detection rule was re-derived, not inherited:** the phase
directory holds `SPEC.md` (414 lines) and `PLAN.md` (2264 lines) and NO
`PROGRESS.md` and NO `REVIEW.md` before this file was created.

`git check-ignore next-prompt.txt` exit **0**, run STANDALONE — it IS ignored and
is never staged.

**Baseline `cargo build --workspace --all-targets` exit 0** before any edit
(`Finished \`dev\` profile … in 26.19s`).

### The three stop-condition legs, re-measured from disk — ALL THREE FALSE

- **Leg (i) — FALSE.** `ROADMAP.md`: **123 rows / 122 `done` / 1 `planned`**, the
  `planned` row being `115` at file line **78**. Driven from the `^\| [0-9]`
  prefix with status at field **4** of a `' | '` (WITH the spaces) split; the
  status buckets SUM to the row count (122 + 1 = 123 ✓). The `' | '` field-count
  histogram is **{6: 121, 7: 1, 10: 1}**, so the forbidden `NF == 6` filter reads
  **121** — it drops the two rows carrying unescaped in-cell pipes. Those are
  append-only history and were NOT "fixed". This session does not touch
  `ROADMAP.md`, so leg (i) is unchanged at its end.
- **Leg (ii) — FALSE.** **14** crates
  (`envoy-{accesslog,admin,bin,cluster,config,filter,health,http1,http2,jwt,listener,stats,tcp,tls}`).
  `envoy-http3` / `envoy-grpc` / `envoy-wasm` / `envoy-protos` / `envoy-runtime` /
  `envoy-xds` all re-confirmed absent by `test -d`. Over the **28** manifests of
  `git ls-files '*Cargo.toml'`: `quinn` / `wasmtime` / `tonic` / `opentelemetry` /
  `prost` = **0** each. **Positive control, IDENTICAL invocation shape: `tokio` =
  19 of 28.** `histogram` = **0** over `crates/` against a `gauge` control of
  **365** occurrences by `grep -rIo 'gauge' crates/` (**352** scoped to
  `crates/*/src/`) — **the method and the scope are stated WITH the number**
  because `ADR-0200` recorded that same control as 462/449 and it does not
  reproduce.
- **Leg (iii) — FALSE.** **11** `### ` family headings reading
  **11 / 5 / 3 / 14 / 3 / 4 / 6 / 31 / 6 / 0 / 13** with **27** pre-heading rows,
  summing to 123 ✓. Censused from a single `/^### /` rule that SEEDS EVERY
  HEADING AT 0, because a naive per-row `awk` never emits the zero-row one. The
  one zero-row family is **`### WASM host family`**. The filing defect is intact
  and was NOT repaired: rows titled `Observability family:` sit physically under
  `### Deprecated / edge features`.

**No `stop` file exists and none was created** (`ADR-0167` DECISION 2).

---

## Task 1 — RIDER: re-attach `build_access_log_record`'s doc comment (CF-114-6)

**Commit:** `phase 115 task 1: RIDER — re-attach build_access_log_record's doc comment (CF-114-6)`

**What it is.** Not a deliverable. Phase 114 spliced `effective_grpc_status`
BETWEEN `build_access_log_record`'s three-line doc comment and its `fn` line
(`crates/envoy-http1/src/hcm.rs`), so the gRPC-status helper opened with a
paragraph describing a different function and the H1 record-build join point had
no doc at all. `ADR-0200` (l) names it a legitimate rider because this phase
touches that file; `ADR-0192` DECISION 5 says riders are not costed as phases.
**Zero behaviour change, net 0 lines.**

**Step 1 — both anchors asserted unique** (the phase-113 `fn check_alpn`
precedent: a bare name can match a test function whose name merely begins with
it):

```
$ grep -cF "/// Build the per-request access-log record (extracted verbatim from" crates/envoy-http1/src/hcm.rs
1
$ grep -cF "fn build_access_log_record(" crates/envoy-http1/src/hcm.rs
1
```

**Step 2 — the move.** The three `///` lines were deleted from directly above
`/// Phase 114: the UNGATED effective gRPC status.` (file line 1656) and inserted
directly above `fn build_access_log_record(`, after the blank line that follows
`effective_grpc_status`'s closing `}`.

**Step 3 — proved a pure move.** `PLAN.md` predicts numstat `3 3`, and it is not
taken on faith that a numstat is a property of the edit (the phase-114 state-3
trap: a pure block move rendered `10 14`). Here the prediction held exactly:

```
$ git diff --numstat crates/envoy-http1/src/hcm.rs
3	3	crates/envoy-http1/src/hcm.rs
```

and the diff is literally the same three lines out and the same three lines in —
one hunk at `@@ -1653,9 +1653,6 @@` removing them and one at `@@ -1673,6 +1670,9 @@`
adding them, with no other line touched.

**Step 4 — gate.** All three legs green on the landed tree:

```
$ cargo build --workspace --all-targets      → Finished `dev` profile … in 26.19s   (exit 0)
$ cargo fmt --all -- --check                 → FMT_EXIT=0   (no output)
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
                                             → Finished … in 9.68s   CLIPPY_EXIT=0
```

⚠ The clippy leg was gated on the **`Checking` COUNT, not the exit code** — a
cached clippy exits 0 having done nothing. This run did real work: **22
`Checking` lines** (plus 3 `Compiling` for the `rustls 0.23.45` /
`aws-lc-sys 0.45.0` bump that `0cb1f67` landed) and **zero diagnostics**. The
count differs from the `PLAN.md` per-task table's `160 Checking` because that
figure was measured on a cold scratch worktree with its own `CARGO_TARGET_DIR`;
the invariant is non-zero, not the absolute.

**Deviation from `PLAN.md`, stated rather than absorbed:** Task 1's `git add`
list names only `crates/envoy-http1/src/hcm.rs`. This file — `PROGRESS.md` — is
staged with it, because §5 state 3 requires appending to `PROGRESS.md` on each
task completion and the phase-114 rider commit `2cf0830` set exactly that
precedent (`10 10` on the code file + the new `PROGRESS.md`). `PROGRESS.md` is
`docs/` and is excluded from the §6.1 LoC accounting.

---

## Task 2 — The `FilterResponse::details` seam

**Commit:** `phase 115 task 2: FilterResponse::details — a filter's local reply carries %RESPONSE_CODE_DETAILS%`

**What it is.** A new `pub details: Option<&'static str>` on
`envoy_filter::FilterResponse`, copied by BOTH HCMs into the access-log record's
`%RESPONSE_CODE_DETAILS%` on the decode-side `StopAndSend` path ONLY. Every
filter that predates phase 115 sets `None`, which renders `-` exactly as before,
so the task is behaviour-neutral for the landed tree. Task 3's health-check
filter is its first real producer.

**Steps 1–2 — the failing tests, written FIRST (TDD).** An H1 test
(`h1_decode_stop_and_send_details_reach_the_access_log`) drives a decode-side
`StopAndSend` through a `%RESPONSE_CODE_DETAILS%`-only file sink twice, once with
`Some("seam_probe")` and once with `None`, asserting `d=seam_probe` and `d=-`.
The H2 side required generalising the landed `h2_response_code_details_line`
helper to take an optional pipeline and a URI and to return
`(http::HeaderMap, String, u64)` — headers, log line and `downstream_rq_2xx` —
which Task 5 reuses; its one existing caller
(`hcm_h2_sets_response_code_details_from_response_path`) was rewritten to the new
signature and a second caller
(`h2_decode_stop_and_send_details_reach_the_access_log`) added.

⚠ The H1 insertion point was checked for the phase-113/114 doc-comment-theft
class before splicing: the line above the anchor `#[tokio::test]` is blank and
the one above that is a closing `}`, so nothing's docblock was stolen.

**Step 3 — RED, and for the right reason.**

```
$ cargo test -p envoy-http1 -p envoy-http2 details_reach_the_access_log
      1 error: could not compile `envoy-http1` (lib test) due to 1 previous error
      1 error: could not compile `envoy-http2` (lib test) due to 1 previous error
      2 error[E0560]: struct `FilterResponse` has no field named `details`
```

Exactly one `E0560` per crate — the predicted failure, not an incidental one.

**Step 5 — the `E0063` sweep, driven to a FIXPOINT from the COMPILER's list.**
`FilterResponse` has no `Default`, so the new field is a hard error at every
exhaustive literal. `cargo` stops at the first failing crate, so round 0 is NOT
the blast radius:

| round | crate | distinct `file:line:col` sites |
|---|---|---:|
| 0 | `envoy-filter` (`types.rs` ×2, `jwt_authn.rs` ×2, `local_rate_limit.rs` ×2, `cors.rs`, `pipeline.rs`, `router.rs`) | **9** |
| 1 | `envoy-http1/src/hcm.rs` (the encode-side literal at `:1421` + four test literals) | **5** |
| 2 | `envoy-http2/src/hcm.rs` (the encode-side literal at `:1063` + three test literals) | **4** |
| 3 | — | **0** (fixpoint) |
| | **TOTAL** | **18** |

**18 is exactly the `PLAN.md` / `ADR-0201` MEASURED figure, and it refutes
`SPEC.md` §8 PV-3's 28** (a text count of `FilterResponse {` lines that includes
the struct declaration, 8 `fn` signatures, 2 `impl` headers and one
functional-update literal, and cannot see the two `Self { … }` literals inside
`impl FilterResponse`). No site was found by grep; every one came from the
compiler. The `..FilterResponse::test_200()` functional update in
`header_mutation.rs` absorbs the field and was NOT edited — the compiler never
named it.

⚠ **A census trap caught in passing:** `grep -c "details: None," crates/…` reads
**20**, not 18, because it matches the two PRE-EXISTING
`response_code_details: None,` lines as a substring. The diff-derived count
(`git diff -U0 | grep -c '^+.*details: None,'`) is **18**. Anchor the name or
count the diff.

**Steps 6–7 — the threading.** `RequestPath::SynthFromDecode` and
`H2RequestPath::SynthFromDecode` each grow a second field carrying
`filter_resp.details`; each arm assigns `response_code_details_for_log(_h2)` from
it. Both locals lose their `= None` initialiser — every arm now assigns, and
keeping the initialiser makes `rustc` warn that the assigned value is never read.
Build clean afterwards, zero warnings.

**Step 8 — GREEN.**

```
test hcm::tests::h1_decode_stop_and_send_details_reach_the_access_log ... ok
test hcm::tests::h2_decode_stop_and_send_details_reach_the_access_log ... ok
test hcm::tests::hcm_h2_sets_response_code_details_from_response_path ... ok
```

**Step 9 — MUTATION: the H1 test is not vacuous.** ⚠ The bare text
`response_code_details_for_log = details.map(str::to_owned);` occurs **2×** in
`crates/envoy-http1/src/hcm.rs` (the `BuildOutcome::Synth` arm carries the same
line), so a plain `sed` would have mutated both and faked a result. The mutation
was anchored on the preceding phase-115 comment, whose combined form was asserted
to occur exactly **1×**. `details.map(str::to_owned)` → `details.and(None)`:

```
   Compiling envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
test hcm::tests::h1_decode_stop_and_send_details_reach_the_access_log ... FAILED
  left: "d=-"
 right: "d=seam_probe"
test result: FAILED. 0 passed; 1 failed; …
```

The `Compiling envoy-http1` line is the proof the binary was actually rebuilt (a
stale test binary gives a FALSE PASS). Restored from the backup and verified
**md5-identical** (`993cf52812c8e140fb6a09cbf867608e` both sides), then the
UNMUTATED CONTROL re-run from the same tree, with its own forced rebuild:
`… h1_decode_stop_and_send_details_reach_the_access_log ... ok`.

**Step 10 — gate, all green.**

```
$ cargo build --workspace --all-targets                                  → BUILD=0
$ cargo fmt --all -- --check                                             → FMT=0
$ cargo clippy --workspace --all-targets --all-features -- -D warnings   → CLIPPY=0
                                    8 `Checking` lines, 0 warning/error rows
$ cargo test -p envoy-filter -p envoy-http1 -p envoy-http2               → TEST=0
                                    7 binaries, 7 `ok` rows, 0 `FAILED` rows,
                                    585 passed, 0 failed
```

**Size and identity, both reproducing the plan EXACTLY.**

```
$ git diff --cached --numstat | awk '{i+=$1; d+=$2} END {print i, d, i-d}'
124 24 100
```

against `PLAN.md`'s per-task table row `2 — FilterResponse::details | 124 | 24 | 100`.
And the unit-test total across `envoy-config` + `envoy-filter` + `envoy-http1` +
`envoy-http2` is **1307 passed / 0 failed over 9 binaries**, against the plan's
predicted post-Task-2 **1307**. Two independent invariants of the same slice,
both landing on the predicted value.

---

## Task 3 — `HealthCheckFilterConfig` and `HealthCheckFilter`

**Commit:** `phase 115 task 3: HealthCheckFilterConfig + HealthCheckFilter (matching, local reply, counters)`

**What it is.** The config schema
(`envoy.extensions.filters.http.health_check.v3.HealthCheck`, non-pass-through
mode only) and the runtime filter: AND-folded `HeaderMatcher` matching, the empty
200 local reply carrying `x-envoy-upstream-healthchecked-cluster`, and the eight
`http.<stat_prefix>.health_check.*` counters. **Not yet reachable from a
bootstrap** — Task 4 adds the typed-config variant.

**Steps 1–2 — the config tests, RED first.** Six schema tests were added to
`crates/envoy-config/src/bootstrap.rs` above the phase-31 `cdn_loop` banner.
⚠ Located BY TEXT, not by a line number or "append at EOF": `bootstrap.rs`
carries TEN column-0 `#[cfg(test)] mod` blocks and an EOF append lands inside the
wrong one.

```
$ cargo test -p envoy-config health_check_config_tests
      1 error[E0432]: unresolved import `crate::HealthCheckFilterConfig`
      1 error: could not compile `envoy-config` (lib test) due to 1 previous error
```

Exactly one error — the predicted one.

**Steps 3–4 — the config type, GREEN.** `HealthCheckFilterConfig` with a REQUIRED
`pass_through_mode`, a `#[serde(default)]` `headers`, the two RECOGNIZED-then-
rejected fields (`cache_time`, `cluster_min_healthy_percentages`) and the
`#[serde(skip)] local_cluster` that Task 4 stamps. Re-exported from
`crates/envoy-config/src/lib.rs`; `cargo fmt --all` re-flowed the whole `pub use`
list, which is why that file's numstat is `17 16` rather than `1 0` — expected
and called out by the plan.

```
$ cargo test -p envoy-config health_check_config_tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 722 filtered out
```

**Step 5 — the filter's tests, RED first.** `crates/envoy-filter/src/health_check.rs`
was created holding ONLY the 147-line test module, extracted VERBATIM from
`PLAN.md`'s fence by script (not retyped), plus `pub mod health_check;`:

```
$ cargo test -p envoy-filter health_check::     → 18 errors
      2 error[E0425]: cannot find type `HealthCheckFilter` in this scope
      1 error[E0425]: cannot find value `STAT_NAMES` in this scope
      3 error[E0433]: cannot find type `HealthCheckFilter` in this scope
      3 error[E0433]: cannot find type `Arc` / `Decision` / `StatsRegistry` …
```

**18** is exactly the plan's MEASURED count, and the two load-bearing names
(`HealthCheckFilter`, `STAT_NAMES`) are among them; the rest are names the
implementation's own `use` lines bring into scope.

**Step 6 — the implementation, GREEN.** 127 lines, again extracted verbatim from
the plan's fence. The design point `ADR-0201` correction 1 turns on: a `:path`
matcher is evaluated against a ONE-ENTRY view `[(":path", req.path.clone())]`,
because neither codec puts pseudo-headers into `FilterRequest::headers`; every
other matcher name goes to `req.headers` as usual.

```
$ cargo test -p envoy-filter health_check::
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 214 filtered out
```

**Steps 7–8 — TWO mutations, both landing exactly where predicted.** Each anchor
was asserted to occur exactly **1×** first, each run showed a forced
`Compiling envoy-filter`, and each restore was md5-verified
(`d87fc1d7c7f7cab3a2318e61855ccb19` on both sides, both times).

| mutation | what it restores | predicted RED | MEASURED |
|---|---|---|---|
| `m.matches(&path_view)` → `m.matches(&req.headers)` | the naive matcher reuse `SPEC.md` §4 item 3 implies | 5 named tests | **5 passed / 5 failed** — `matched_probe_is_answered_locally`, `filter_is_method_agnostic`, `header_value_is_not_an_echo_of_the_request`, `matchers_fold_as_and`, `registers_eight_counters_and_ticks_two_per_intercept` ✓ exactly those five |
| `req.path.clone()` → `req.path.split('?').next()…` | stripping the query string | 1 named test | **9 passed / 1 failed** — `path_matcher_sees_the_query_string` ✓ |

The first mutation is the one that matters: it is the filter `SPEC.md` describes,
and it would never have matched `:path` at all. The test suite sees it.
**Unmutated control re-run from the same tree after the second restore, with its
own forced rebuild: `test result: ok. 10 passed; 0 failed`.**

**Step 9 — gate. ⚠ The clippy leg is DEFERRED to Task 4 BY DESIGN and that
deferral was MEASURED, not assumed.**

```
$ cargo build --workspace --all-targets                  → BUILD=0
$ cargo fmt --all -- --check                             → FMT=0
$ cargo test -p envoy-config health_check_config_tests   → ok. 6 passed; 0 failed
$ cargo test -p envoy-filter health_check::              → ok. 10 passed; 0 failed

$ cargo clippy --workspace --all-targets --all-features -- -D warnings
CLIPPY=101
  error: constant `PATH_PSEUDO_HEADER` is never used
  error: fields `headers`, `local_cluster`, `request_total`, and `ok` are never read
  error: methods `decode_headers`, `encode_headers`, and `matches` are never used
  error: could not compile `envoy-filter` (lib) due to 3 previous errors
files named: crates/envoy-filter/src/health_check.rs
```

**EXACTLY the three `dead_code` errors `PLAN.md` and `ADR-0201` DECISION 5
predicted, all in that one file, and nothing else in the workspace.** The filter
has no production consumer until Task 4 adds the typed-config variant. **No
`#[allow]` and no `_` prefix was added** — `ADR-0194` DECISION 1, chosen so a
forgotten suppression cannot outlive the gap. Task 4 must show clippy exit 0 with
a non-zero `Checking` count. Every alternative ordering lands an intermediate
commit that parses a health-check config and silently ignores part of it, the
window `ADR-0176` DECISION 2 forbids.

**Size and identity, both reproducing the plan EXACTLY.**

```
$ git diff --cached --numstat -- crates/
83	0	crates/envoy-config/src/bootstrap.rs
17	16	crates/envoy-config/src/lib.rs
274	0	crates/envoy-filter/src/health_check.rs
2	0	crates/envoy-filter/src/lib.rs
TOTAL ins=376 del=16 net=360
```

against `PLAN.md`'s per-task row `3 — config type + filter | 376 | 16 | 360`, and
`health_check.rs` at **274** lines against the whole-slice table's 274. The
four-crate unit total is **1323 passed / 0 failed over 9 binaries** against the
plan's predicted post-Task-3 **1323**.

---

## Task 4 — Make the filter reachable: typed config, validator, `node.cluster` stamping, 13th instance

**Commit:** `phase 115 task 4: health_check reachable — typed config, validator, node.cluster stamping, 13th HttpFilterInstance`

**What it is.** The four edges that make Task 3's filter reachable from a
bootstrap: the `HttpFilterTypedConfig::HealthCheck` variant (`@type`
`…filters.http.health_check.v3.HealthCheck`, name
`envoy.filters.http.health_check`), two new `ConfigError` variants and the
`validate_health_check_config` gauntlet, the `node.cluster` stamping in
`validate_hcm`, and `HttpFilterInstance::HealthCheck` — the **THIRTEENTH**
production variant.

**Steps 1–2 — RED first, with exactly the predicted errors.** Thirteen new
`envoy-config` tests (seven appended to `mod health_check_config_tests`) and one
`envoy-filter` instance test, both fences extracted from `PLAN.md` by script:

```
$ cargo test -p envoy-config health_check_config_tests
      1 error[E0599]: no variant named `UnsupportedHealthCheckField` found for enum `ConfigError`
      1 error[E0599]: no variant named `UnsupportedHealthCheckPseudoHeader` found for enum `ConfigError`
      1 error[E0599]: no variant or associated item named `HealthCheck` found for enum `bootstrap::HttpFilterTypedConfig` in the current scope
```

Exactly **3** — the plan's MEASURED count, and each one names a thing this task
adds.

**Steps 3–7 — the implementation.** `UnsupportedHealthCheckField` (boot-fatal on
`pass_through_mode: true`, `cache_time`, `cluster_min_healthy_percentages` —
`ADR-0049` fail-loud; upstream ACCEPTS the first two, so these are RECORDED
REJECT-direction divergences, CF-115-1 / CF-115-5) and
`UnsupportedHealthCheckPseudoHeader` (any `:`-prefixed matcher name other than
`:path`; upstream MATCHES `:method` / `:authority` / `:scheme`, envoy-rust cannot
see them, CF-115-6 — rejected at load rather than silently never matching). The
validator then runs each matcher through the shared `validate_header_matcher`
gauntlet on a clone.

`validate` captures `node.cluster` (empty without a `node`) BEFORE the `&mut`
listener loop and passes it to `validate_hcm` as a new last parameter; the
stamping loop writes it into every `health_check` filter's `#[serde(skip)]`
`local_cluster`. `validate` runs at `parse_bootstrap` AND at the post-merge
re-validation in `load_dynamic_resources`, so LDS-delivered listeners are stamped
too. `validate_hcm` now takes seven parameters — under clippy's
`too_many_arguments` threshold, and clippy confirms it below.

**Step 8 — GREEN, both at the predicted counts.**

```
$ cargo test -p envoy-config health_check_config_tests
running 13 tests → test result: ok. 13 passed; 0 failed
$ cargo test -p envoy-filter health_check
running 11 tests → test result: ok. 11 passed; 0 failed     (10 filter + 1 instance)
```

**Step 9 — THE GATE THAT CLOSES TASK 3's DEFERRAL.**

```
$ cargo build --workspace --all-targets                                  → BUILD=0
$ cargo fmt --all -- --check                                             → FMT=0
$ cargo clippy --workspace --all-targets --all-features -- -D warnings   → CLIPPY=0
                                    13 `Checking` lines, 0 warning/error rows
$ cargo test -p envoy-config -p envoy-filter                             → TEST=0
                                    4 binaries, 4 `ok` rows, 0 `FAILED` rows,
                                    960 passed, 0 failed
```

**Clippy exits 0 with a NON-ZERO `Checking` count** (the count is the gate; an
exit 0 with zero `Checking` lines is a cached no-op). The three `dead_code`
errors Task 3 measured are gone because the filter now has a production consumer
— **not because anything was suppressed.** Verified on disk:
`grep -c 'allow(' crates/envoy-filter/src/health_check.rs` = **0** and
`grep -cE '^\s+_[a-z_]+:'` = **0**. `ADR-0194` DECISION 1 discharged.

### ⚠ Size drift: +1 line against `PLAN.md`'s per-task table — RECORDED, not absorbed

```
$ git diff --cached --numstat -- crates/
200	0	crates/envoy-config/src/bootstrap.rs
22	0	crates/envoy-config/src/lib.rs
43	0	crates/envoy-filter/src/instance.rs
TOTAL ins=265 del=0 net=265
```

against `PLAN.md`'s row `4 — reachable: variant, validator, stamping, instance |
264 | 0 | 264`. **Two of the three files reproduce EXACTLY** —
`crates/envoy-config/src/lib.rs` at 22 (making its cumulative `39 / 16` the
whole-slice table's figure to the line) and `crates/envoy-filter/src/instance.rs`
at 43 (the whole-slice table's 43). The drift is entirely in
`crates/envoy-config/src/bootstrap.rs`: **200 landed against the prototype's
199**, i.e. cumulative **283 against the whole-slice table's 282**.

It is located and it is one BLANK SEPARATOR, not a line of code. The nine
`bootstrap.rs` hunks sum as `5 + 1 + 8 + 1 + 1 + 6 + 3 + 39 + 136 = 200`, and
every hunk is its `PLAN.md` fence plus exactly one separating blank line
(fences: 4, 1, 7, 1, 1, 5, 3, 38, 135 = 195 content lines this task, 276 across
Tasks 3+4). The prototype therefore carried **six** separating blanks where this
tree carries **seven**; which of the seven the prototype omitted is not
recoverable from the fences, and no double blank exists in the landed tree
(checked: zero `+` blank lines adjacent to a context blank) — so nothing was
trimmed to chase the number. `ADR-0198` reconciliation precedent; `PLAN.md`'s
102-line §6.1 margin absorbs it with 101 to spare.

**The test-count identity, by contrast, is EXACT:** the four-crate unit total is
**1331 passed / 0 failed over 9 binaries**, against the plan's predicted
post-Task-4 **1331**.
