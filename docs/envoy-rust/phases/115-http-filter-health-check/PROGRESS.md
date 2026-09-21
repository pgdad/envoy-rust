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

---

## Task 5 — An intercepted probe is not counted in `downstream_rq_Nxx`; the H1/H2 end-to-end pins

**Commit:** `phase 115 task 5: an intercepted health-check probe is not counted in downstream_rq_Nxx; H1/H2 end-to-end pins`

**What it is.** `ADR-0201` correction 4's most consequential half. Upstream
MEASURED: an intercepted probe IS counted in `downstream_rq_total` and
`downstream_rq_http1_total` but is **NOT** counted in `downstream_rq_2xx` (nor
`downstream_rq_completed`). envoy-rust already emits `downstream_rq_2xx`, so
shipping the filter without the exclusion would have CREATED a divergence on a
landed, contracted stat. The discriminator is the `health_check_ok` detail
string: exactly one producer sets it, and it is already live at both tick sites,
so no new flag is threaded.

**Steps 1–2 — RED, and ONLY on the counter.** Two end-to-end tests. The H1 one
goes through real bootstrap YAML → `parse_bootstrap` (which does the `node.cluster`
stamping) → `HCMConfig::from_config` → the wire, and reads the access log as an
INDEPENDENT second witness of the same two requests. The H2 one reuses Task 2's
generalised `h2_response_code_details_line` (CF-115-4 pins the H2 cell in-process
only — there is no H2 fixture).

```
H1: panicked at crates/envoy-http1/src/hcm.rs:7385:9:
    assertion `left == right` failed: the intercept is not counted
      left: 2
     right: 1
H2: panicked at crates/envoy-http2/src/hcm.rs:4727:9:
    assertion `left == right` failed: the intercept is not counted
      left: 1
     right: 0
```

**Exactly the two predicted failures, and nothing else in either test.** That is
the load-bearing observation: every OTHER assertion — the 200, the
`x-envoy-upstream-healthchecked-cluster: hc-node-cluster` header, the absent
`content-type`, the empty body, `/healthz?x=1` falling through to `MAIN`,
`downstream_rq_total == 2`, `health_check.request_total == 1`, and the exact
access-log text `/healthz|health_check_ok|-|0\n/healthz?x=1|direct_response|-|4\n` —
**already passed before this task's change**, which is an end-to-end confirmation
that Tasks 2, 3 and 4 compose correctly through real YAML.

**Steps 3–4 — the exclusion**, guarding the existing per-class `match` on both
codecs with `response_code_details_for_log(_h2).as_deref() != Some(HEALTH_CHECK_OK)`.

**Step 5 — GREEN.** `cargo test -p envoy-http1 -p envoy-http2` → 5 binaries,
5 `ok` rows, 0 `FAILED` rows, **373 passed / 0 failed**.

**Step 6 — MUTATION.** Anchor asserted to occur exactly **1×**;
`!= Some(envoy_filter::health_check::HEALTH_CHECK_OK)` → `!= Some("__never__")`
(i.e. the guard is present but can never fire):

```
   Compiling envoy-http1 v0.0.0 (…)
assertion `left == right` failed: the intercept is not counted
  left: 2
 right: 1
test result: FAILED. 0 passed; 1 failed; …
```

Restored md5-identical (`6fb5a7d70e3daef38a2906887efe07b2`); the unmutated
control re-run GREEN from the same tree with its own forced rebuild.

**Step 7 — gate, all green.**

```
$ cargo build --workspace --all-targets                                  → BUILD=0
$ cargo fmt --all -- --check                                             → FMT=0
$ cargo clippy --workspace --all-targets --all-features -- -D warnings   → CLIPPY=0
                                    7 `Checking` lines, 0 warning/error rows
```

**Size and identity, both reproducing the plan EXACTLY.**

```
$ git diff --cached --numstat -- crates/
121	6	crates/envoy-http1/src/hcm.rs
64	6	crates/envoy-http2/src/hcm.rs
TOTAL ins=185 del=12 net=173
```

against `PLAN.md`'s row `5 — counter exclusion + end-to-end pins | 185 | 12 | 173`.
The four-crate unit total is **1333 passed / 0 failed over 9 binaries**, the
plan's exact predicted post-Task-5 figure — the FOURTH consecutive task whose
test identity lands on the prediction.

**Cumulative code size after Task 5:** `git diff --numstat 74f2e12 -- crates/` =
`ins=953 del=55 net=898`, against the plan's whole-slice `crates/` total of
**897**. The single line of divergence is Task 4's recorded `bootstrap.rs` blank
separator and nothing else; `crates/` is otherwise line-for-line the measured
prototype.

---

## Task 6 — Differential fixture `0095-http-filter-health-check`

**Commit:** `phase 115 task 6: fixture 0095-http-filter-health-check — ten probes, byte-identical configs`

**What it is.** The WIRE witness: ten sequential HTTP/1.1 probes at a
backend-free, CLUSTER-FREE HCM listener whose chain is
`[health_check A (:path exact /healthz), health_check B (:path exact /both AND
x-probe exact yes), router]` with a `prefix: "/"` `direct_response` catch-all
answering `MAIN`. Driver `Http1ProbeList`; no harness change, no new driver.

**Why it is not vacuous, and the reason is non-obvious.** **Every probe answers
200**, so status alone cannot pass this fixture. The BODY decides — an
intercepted probe is empty with no `content-type`, a fall-through is the 4-byte
`MAIN` — and `set_equal_modulo_allow_list` additionally compares
`x-envoy-upstream-healthchecked-cluster` VALUE-exact across the two proxies.

**Steps 1–5 — the files, all four extracted from `PLAN.md` by script.** Line
counts, each matching the plan's whole-slice table exactly: `envoy.yaml` **53**,
`envoy-rust.yaml` **53**, `expectations.yaml` **102**, `README.md` **67**, the
runner **29**.

**Byte-identity RE-DERIVED at this commit, not inherited:**

```
$ cmp tests/fixtures/0095-http-filter-health-check/envoy.yaml \
      tests/fixtures/0095-http-filter-health-check/envoy-rust.yaml
(silent)
$ md5sum …/envoy.yaml …/envoy-rust.yaml
85b837a66319583bf8c5a81f6b15f4e1  …/envoy-rust.yaml
85b837a66319583bf8c5a81f6b15f4e1  …/envoy.yaml
```

`node: { id: fixture-0095, cluster: hc-fixture-cluster }` is on BOTH sides, and
`hc-fixture-cluster` is a value no YAML-1.1 parser booleanizes (`envoy-rust`
parses YAML 1.2, upstream 1.1 — the `0088` README records the trap an unquoted
`y` sets). `admin` carries a LITERAL `port_value: 0`, not `{{ADMIN_PORT}}`, which
is driver-gated and `Http1ProbeList` never receives.

**Step 6 — GREEN, and the fast green was AUDITED rather than believed.**

```
$ cargo build -p envoy-bin          # the harness runs the DEBUG binary
$ cargo test -p differential --test http_filter_health_check
running 1 test
test http_filter_health_check_fixture ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.22s
```

1.22 s is fast enough to be suspicious. `docker ps --format '{{.ID}} {{.Image}}'`
was polled at 4 Hz for the duration of the run and observed exactly **one**
`envoyproxy/envoy:v1.33.0` container (`5b2aa6ce5e77`), so the reference side
really ran — the image ID `56da5afd7df3` is the `ENVOY_TARGET.md` pin's digest
prefix. Backend-free fixtures genuinely finish in ~1 s.

**Step 7 — MUTATIONS V1–V3, each landing on exactly the predicted probe.** The
probe-list driver aborts at the FIRST failing probe, so one red run names ONE
probe — which is what makes these three mutations a partition rather than a
repeat. Each: assert the anchor occurs exactly `1`, apply, **rebuild
`envoy-bin`** (a `Compiling envoy-filter`/`envoy-config` AND `Compiling
envoy-bin` line every time — `cargo test -p differential` alone never rebuilds
the proxy and would read a FALSE GREEN), run, restore, md5.

| # | mutation | predicted RED | MEASURED |
|---|---|---|---|
| V1 | `cfg.local_cluster = local_cluster.to_string()` → `String::new()` (`bootstrap.rs`) | `p1 … diff_headers` | `probe p1-healthz-intercepted: diff_headers` ✓ |
| V2 | `req.path.clone()` → query-stripped (`health_check.rs`) | `p2 … subject body != expected` | `probe p2-query-string-falls-through: subject body != expected` ✓ |
| V3 | `self.headers.iter().all(` → `.any(` (`health_check.rs`) | `p9 … subject body != expected` | `probe p9-and-path-alone-falls-through: subject body != expected` ✓ |

md5 after each restore: V1 `cfb018af8f2313b6b1ce009a1d697144`, V2 and V3 both
`d87fc1d7c7f7cab3a2318e61855ccb19` — identical to their backups. A **10-second
settle gap** separated the Docker runs (back-to-back container starts on this
host can manufacture a false red).

**Unmutated control, from the same tree after the last restore**, with
`git status --porcelain -- crates/` EMPTY and `envoy-bin` rebuilt:
`test result: ok. 1 passed; 0 failed; … finished in 1.30s`.

**Gate:** build 0, fmt 0, clippy `-D warnings` 0 with **14 `Checking`** lines and
zero diagnostics.

**Size, reproducing the plan EXACTLY.**

```
$ git diff --cached --numstat
29	0	tests/differential/tests/http_filter_health_check.rs
67	0	tests/fixtures/0095-http-filter-health-check/README.md
53	0	tests/fixtures/0095-http-filter-health-check/envoy-rust.yaml
53	0	tests/fixtures/0095-http-filter-health-check/envoy.yaml
102	0	tests/fixtures/0095-http-filter-health-check/expectations.yaml
TOTAL ins=304 del=0 net=304
```

against `PLAN.md`'s row `6 — fixture 0095 | 304 | 0 | 304`.

---

## Task 7 — Differential fixture `0096-http-filter-health-check-stats`

**Commit:** `phase 115 task 7: fixture 0096-http-filter-health-check-stats — intercepts are counted by health_check, not by downstream_rq_2xx`

**What it is.** The STAT witness, on the existing `Driver::AdminScrape`: three
HTTP/1.1 pre-requests (`GET /healthz`, `GET /other`, `GET /healthz`) against a
backend-free, cluster-free `[health_check, router]` listener, then a **bilateral
ABSOLUTE** stat assertion scraped from BOTH admin listeners.

| stat | value | rule |
|---|---:|---|
| `http.ingress_http.health_check.request_total` | 2 | ticks once per INTERCEPT, never on a fall-through |
| `http.ingress_http.health_check.ok` | 2 | the same, in non-pass-through mode |
| `http.ingress_http.downstream_rq_total` | 3 | every request is counted |
| `http.ingress_http.downstream_rq_2xx` | 1 | **an intercepted probe is NOT counted** |
| `http.ingress_http.health_check.failed` | 0 | ⚠ NOT a presence witness |

⚠ `scrape_admin_stat` returns **0** for a name a proxy never registered, so the
`value: 0` row passes even if the stat is ABSENT — **only the four non-zero rows
are witnesses.** Presence of the six zero-valued counters is pinned in-process by
`registers_eight_counters_and_ticks_two_per_intercept`, not here. And the
`scrapes:` block is fixture `0015`'s `/server_info` sub-case carried verbatim,
present ONLY because `Driver::AdminScrape` rejects an empty list.

**Steps 1–5 — the files**, all four extracted from `PLAN.md` by script:
`envoy.yaml` **40**, `envoy-rust.yaml` **40**, `expectations.yaml` **47**,
`README.md` **45**, the runner **25** — every count matching the whole-slice
table. Byte-identity re-derived: `cmp` silent, md5
`e66e8ad0cc20d41cf5d7f12ce42ecd65` on both sides. Unlike `0095`, this fixture
DOES use `{{ADMIN_PORT}}` — it is driver-gated and `AdminScrape` is one of the
four drivers that receive it — and it still carries a `{{PORT}}` listener,
because the harness's accept-ready wait is unconditional.

**Step 6 — GREEN, audited the same way as `0095`:**

```
test http_filter_health_check_stats_fixture ... ok
test result: ok. 1 passed; 0 failed; … finished in 1.32s
```

with `docker ps` polled at 4 Hz through the run observing exactly one
`envoyproxy/envoy:v1.33.0` container (`f908eadf74de`).

**MUTATIONS V4–V5**, each with the anchor asserted unique, a forced
`envoy-bin` rebuild, a 10-second settle gap and an md5-verified restore:

| # | mutation | predicted RED | MEASURED |
|---|---|---|---|
| V4 | the H1 per-class gate compares against a sentinel that never matches | `downstream_rq_2xx expected 1 got 3` | `subject stat http.ingress_http.downstream_rq_2xx expected 1 got 3` ✓ |
| V5 | delete `self.request_total.inc();` | `health_check.request_total expected 2 got 0` | `subject stat http.ingress_http.health_check.request_total expected 2 got 0` ✓ |

V4 is the one that matters: `got 3` is what upstream would have disagreed with,
and it is the exact divergence `ADR-0201` correction 4 predicted envoy-rust would
have shipped without Task 5.

md5 after restore: V4 `6fb5a7d70e3daef38a2906887efe07b2`, V5
`d87fc1d7c7f7cab3a2318e61855ccb19` — identical to their backups.

**Unmutated control from the restored tree**, `git status --porcelain -- crates/`
EMPTY, `envoy-bin` rebuilt: `test result: ok. 1 passed; 0 failed; … in 1.31s`.

**Gate:** build 0, fmt 0, clippy `-D warnings` 0 with **9 `Checking`** lines and
zero diagnostics.

**Size, reproducing the plan EXACTLY.**

```
$ git diff --cached --numstat
25	0	tests/differential/tests/http_filter_health_check_stats.rs
45	0	tests/fixtures/0096-http-filter-health-check-stats/README.md
40	0	tests/fixtures/0096-http-filter-health-check-stats/envoy-rust.yaml
40	0	tests/fixtures/0096-http-filter-health-check-stats/envoy.yaml
47	0	tests/fixtures/0096-http-filter-health-check-stats/expectations.yaml
TOTAL ins=197 del=0 net=197
```

against `PLAN.md`'s row `7 — fixture 0096 | 197 | 0 | 197`.

---

## Task 8 — `BEHAVIOR_CONTRACT.md`: the health_check section

**Commit:** `phase 115 task 8: BEHAVIOR_CONTRACT — the health_check filter section`

The only `docs/` change in the phase, and the only task EXCLUDED from the §6.1
LoC gate. The anchor was asserted unique first
(`grep -cF '**H1 upstream connection-pool …'` = **1**, at file line 1958, with the
`cdn_loop` blocks immediately above — where the HTTP-filter wire contracts live),
and the 71-line block was extracted from `PLAN.md`'s fence by script.

```
$ git diff --numstat docs/envoy-rust/BEHAVIOR_CONTRACT.md
72	0	docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

**Additions only, deletions 0**, as the plan requires. The section records the
intercept wire shape, the matching rule (including that `:path` carries the query
string and that chain order is declaration order), the eight counters and the
`downstream_rq_2xx` exclusion, the four boot-fatal config rejections with their
REJECT-direction divergences named, and the three measured upstream behaviours
envoy-rust does NOT match (CF-115-2, CF-115-7, CF-115-4).

### ⚠ Citation damage this phase causes — MEASURED, and deliberately NOT repaired

A phase invalidates `file:line` citations it does not own. The blast radius was
DERIVED rather than sampled: for each file this phase MODIFIED (creations cannot
invalidate anything), the earliest OLD line its diff touches, then every
`…<file>.rs:<n>` / `…<file>.md:<n>` citation in the seven top-level documents
classified against it.

| modified file | first changed OLD line |
|---|---:|
| `crates/envoy-config/src/bootstrap.rs` | 1527 |
| `crates/envoy-config/src/lib.rs` | 27 |
| `crates/envoy-filter/src/cors.rs` | 224 |
| `crates/envoy-filter/src/instance.rs` | 27 |
| `crates/envoy-filter/src/jwt_authn.rs` | 202 |
| `crates/envoy-filter/src/lib.rs` | 16 |
| `crates/envoy-filter/src/local_rate_limit.rs` | 150 |
| `crates/envoy-filter/src/pipeline.rs` | 222 |
| `crates/envoy-filter/src/router.rs` | 62 |
| `crates/envoy-filter/src/types.rs` | 54 |
| `crates/envoy-http1/src/hcm.rs` | 935 |
| `crates/envoy-http2/src/hcm.rs` | 129 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | 1957 |

| document | invalidated | of which unambiguous | still valid |
|---|---:|---:|---:|
| `MISSION.md` | 0 | 0 | 0 |
| `STATE.md` | 45 | 17 | 5 |
| `ROADMAP.md` | 99 | 34 | 18 |
| `BEHAVIOR_CONTRACT.md` | 15 | 1 | 4 |
| `SKILL_ROUTING.md` | 0 | 0 | 0 |
| `ENVOY_TARGET.md` | 0 | 0 | 0 |
| `DECISIONS.md` | 465 | 220 | 153 |
| **TOTAL** | **624** | **272** | **180** |

⚠ **The figure is an UPPER BOUND and the method is why.** A bare `hcm.rs:<n>`
citation cannot be attributed to `envoy-http1` or `envoy-http2` from its text, so
an ambiguous basename is classified against the EARLIER of the two thresholds.
The "unambiguous" column is the subset whose path prefix resolves to exactly one
modified file; the truth lies between 272 and 624.

**Nothing is repaired, and that is the standing discipline, not laziness.**
`DECISIONS.md` is append-only (D-3.5), so its 465 are structurally
un-repairable and always have been; `ROADMAP.md` is untouchable this phase (its
row `115` stays `planned` until the state-6 close-out, and it already carries an
off-by-one from the phase-115 row insert); `STATE.md`'s citations sit in
narrative that each session supersedes or relocates anyway. **RE-ANCHOR ON TEXT
when following any of these; do not mass-repair.** The one live document this
phase both edits and cites into is `BEHAVIOR_CONTRACT.md`, and its 15 are all
`hcm.rs`/`bootstrap.rs` citations — 1 of them unambiguous.

---

## §5 state-3 summary — the implementation is COMPLETE

All EIGHT `PLAN.md` tasks landed IN ORDER, one commit each, TDD on every one:

| task | commit | numstat (crates/ + tests/) | plan row | |
|---|---|---|---|---|
| 1 — rider `CF-114-6` | `a05be5e` | `3 / 3` net **0** | `3 / 3` net 0 | ✓ |
| 2 — `FilterResponse::details` | `4501f27` | `124 / 24` net **100** | `124 / 24` net 100 | ✓ |
| 3 — config type + filter | `73d2972` | `376 / 16` net **360** | `376 / 16` net 360 | ✓ |
| 4 — reachable | `46f0a4a` | `265 / 0` net **265** | `264 / 0` net 264 | **+1** |
| 5 — counter exclusion + pins | `e28790f` | `185 / 12` net **173** | `185 / 12` net 173 | ✓ |
| 6 — fixture `0095` | `4399734` | `304 / 0` net **304** | `304 / 0` net 304 | ✓ |
| 7 — fixture `0096` | `7e33470` | `197 / 0` net **197** | `197 / 0` net 197 | ✓ |
| 8 — `BEHAVIOR_CONTRACT.md` | `63d9445` | `72 / 0` (`docs/`, excluded) | not prototyped | — |
| **TOTAL** | | **`1454 / 55` net 1399** | **`1453 / 55` net 1398** | **1.0007×** |

**1399 against a MEASURED 1398 is the tightest landing in this project's recorded
series** — `112.1` 1.00×, `113` 1.07×, `112.2` 1.10×, `114` 1.107× — and it is
still **101 lines under** the ~1500 §6.1 gate. The mechanism `PLAN.md` installed
is why: every code fence was extracted from the plan **by script**, never
retyped, so no line of code drifted at all. The single line is Task 4's blank
separator in `crates/envoy-config/src/bootstrap.rs`, located and recorded above.

### The test identity

```
$ cargo test --workspace --no-fail-fast
binaries: 172   ok-rows: 163   FAILED-rows: 9
passed: 2336    failed: 9      passed + failed: 2345
```

**`binaries = 172` and `passed + failed = 2345` are EXACTLY `PLAN.md`'s predicted
identity** (`binaries=172 passed=2345 failed=0`) — 2 new differential runners and
30 new test functions, predicted as arithmetic on an unbuilt tree and landed
without adjustment. The four-crate unit totals also landed on every predicted
intermediate: **1307 / 1323 / 1331 / 1333** at the Task 2/3/4/5 boundaries.

### The nine local failures — TWO families, NEITHER of them this slice's

Classified by **ISOLATION against an UNMODIFIED CONTROL**, never by message text.
The control is a detached `git worktree` at `04661b7` (the pre-phase commit) with
its **own** `CARGO_TARGET_DIR`, built `--workspace --all-targets`, and its
`envoy-bin` is md5-**different** from this tree's —
`20c66e6181e83c010cd93b0c80f588c8` vs `0fe25404c4f5c6b9c809e958b3a43ccc` — which
is what proves the control is a genuinely different tree. Every fixture was
re-run there ALONE with a 20-second settle gap, and the four not already on
record were re-run alone on THIS tree as well.

| fixture | alone @ control `04661b7` | alone @ phase tree | family |
|---|---|---|---|
| `access_log_h2_rcd_upstream_reset` | FAIL | — | A: deterministic host signature |
| `access_log_h2_uc_upstream_reset` | FAIL | — | A |
| `access_log_rcd_upstream_reset` | FAIL | — | A |
| `access_log_rf_upstream_reset` | FAIL | — | A |
| `admin_config_dump_server_info` | FAIL | — | A |
| `access_log_upstream_host` | **ok** | **ok** | B: parallel-load flake |
| `lb_maglev_fixture` | **ok** | **ok** | B |
| `lb_ring_hash_fixture` | **ok** | **ok** | B |
| `lb_subset_fixture` | **ok** | **ok** | B |

Family A are the five already recorded at the PLAN-write; they fail
deterministically in isolation on BOTH trees. Family B are four backend-routing
fixtures that PASS in isolation on BOTH trees and redden only inside the full
parallel `--workspace` run. **The two families have OPPOSITE tells and the panic
text cannot tell them apart:** `access_log_upstream_host` panics with `deadline
has elapsed` on upstream's own H1 drive, which reads exactly like Family A —
**a first draft of this record sorted it there, and the control refuted it.**
CI on native Linux is authoritative for all nine.

### What this session did NOT do

- **No ADR fired, and saying so is part of the record.** Nothing measured here
  contradicted a landed figure that `ADR-0201` does not already settle; the one
  line of size drift is reconciled in place. A state advance that writes an ADR
  anyway manufactures a decision the record does not need. **`ADR-0202` is next
  free and nothing is reserved.**
- **`ROADMAP.md` was NOT touched.** Row `115` stays `planned` until the state-6
  close-out.
- **`known-failures.txt` was NOT trimmed** and no landed artifact (`SPEC.md`,
  `ADR-0200`, `ADR-0201`, the phase-114 artifacts) was edited.
- **Nothing was fixed** beyond the one scheduled rider (§6.3; `ADR-0165`).
  `CF-114-6` is CONSUMED by Task 1. `CF-115-1 … CF-115-10` — including
  **CF-115-9, the request-time PANIC in the `fault` filter's `safe_regex_match`
  header gate** — remain banked; `fault.rs` is not in this phase's file list.
  `CF-75-5` and the phase-112 ALPN rider stand, and the rider was not re-costed.
- **No subagent was dispatched.**
- **No §5 state was chained.** The unit ends here; the §5 STATE-4 VERIFICATION
  GATE is a separate session (§5.1; `ADR-0127`).

### Ledger discipline for this advance

`STATE.md` `29 17`, `STATE_HISTORY.md` `28 0` in four hunks of 8 / 8 / 7 / 5 —
the same shape as the phase-114 state-3 advance `ad19e9a`, whose `## Last commit`
section was read for the state-3 shape rather than copying the file as found
(block + ONE blank + the pending-CI line + THREE blanks + the pointer).

**Relocation proved, not asserted** (ADR-0035): the `(old STATE.md − new
STATE.md)` non-blank multiset is **16** — Active phase 5, Next expected skill
4 + the rolling `### Doctrine reminders` §5.1 bullet, Last commit 4, Last updated
2 — every one of the 16 present in `STATE_HISTORY.md`, **0 missing**, against
**20** non-blank additions there: 16 + four copies of the one newly created
`### Superseded at the phase-115 §5 state-3 implementation …` heading. The four
archive headers were resolved by **EXACT whole-line equality, asserted unique,
BEFORE any length-changing splice**, and spliced bottom-up.

⚠ The doctrine bullet was archived because the bullet ITSELF was tested — its
exact predecessor text was **not** already in `STATE_HISTORY.md` — not because
the prior advance's hunk count said so; that heuristic has been wrong twice.

**Token sweep over the PAIR**, method stated with the number: whitespace-split
tokens over the concatenated `STATE.md` + `STATE_HISTORY.md`, pair universe
**90 598 → 90 843** distinct (2 951 177 → 2 996 900 total), **candidate drops 0,
unconserved 0**. Backticks paired **per line, never whole-file**: `STATE.md` has
**0** odd-backtick lines and `STATE_HISTORY.md`'s **15** are all pre-existing
archived wrapped prose — **0** introduced here.

**Traps line**, both spans and both forms: length **280 344** characters (python
`len`, not `wc -c`); anchored count **83** and naive count **88** over the traps
line alone, and **83 / 88** over the whole of `STATE.md` — the two spans coincide,
and this block contributed **exactly +1 to each** by not quoting the marker.
`STATE.md` column-0 `### ` count is now **5** (was 4); the file is **254** lines.

---

# §5 STATE 4 — the §7.5 verification gate

**One commit: this state-advance commit.** The state-3 CI record (`b5c71f6`)
closed the previous chain, so this session entered owing no CI record —
detected STRUCTURALLY by reading `STATE.md`'s `## Last commit` block, which
already carries a CI-confirmed answer (run `35550913313`, attempt 1, `success`,
`binaries=172 passed=2345 failed=0` on `9aa367c`). `git status --porcelain` was
empty at entry and HEAD was `b5c71f6d7bcc56843417fd9adfd077682596ea25`, a
docs-only commit on top of the state-3 code tree.

**No `ADR` fired**: nothing this gate measured contradicts a landed figure. One
state-3 OBSERVATION did not reproduce (Family B passing alone, below), but that
is a host-condition reading, not a decision, and the control settles it without
one. `ROADMAP.md` was NOT touched — row `115` stays `planned` until the state-6
close-out. **Nothing was fixed** (§6.3; `ADR-0165`) and no carry-forward was
consumed.

⚠ **§7.5 leg (f) — `REVIEW.md` is approved — is OUT OF SCOPE and is adjudicated
as such, not as a pass.** It is state 5's product; `115/REVIEW.md` does not
exist, which is the correct state at the end of a state-4 gate.

## Stop condition — all three legs re-measured from disk, all three FALSE

```
LEG (i)   rows=123  done=122  planned=1     SUM 123==123
          not-done: row 115 at ROADMAP.md line 78
          field-count histogram on ' | ': {6: 121, 7: 1, 10: 1}
          (the FORBIDDEN NF==6 filter would read 121)
LEG (ii)  crates=14   tracked manifests=28 (git ls-files '*Cargo.toml')
          envoy-http3/grpc/wasm/protos/runtime/xds: all six absent by test -d
          quinn=0 wasmtime=0 tonic=0 opentelemetry=0 prost=0  of 28
          POSITIVE CONTROL, identical invocation: tokio=19 of 28
          histogram over crates/ = 0 ; gauge = 365 by `grep -rIo 'gauge' crates/ | wc -l`
                                               (352 over crates/*/src/)
LEG (iii) headings=11  slices=11/5/3/14/3/4/6/31/6/0/13  pre-heading=27
          SUM=123   zero-row family: ['### WASM host family']
          (census from a /^### / rule that seeds every heading at 0)
```

`ls stop` → `No such file or directory`. **No `stop` file was created.**

## Leg (e) — the five `cargo` commands

⚠ **An exit-0 `build`/`clippy` on a warm cache is not evidence.** Before each
of the two, all **22** tracked crate roots (`crates/*/src/{lib,main}.rs` and
`tests/**/src/{lib,main}.rs`, list driven from `git ls-files`, never a glob —
`touch` CREATES files) were given an **mtime-only `touch -m`**. `git status
--porcelain` read EMPTY after both touches and at the end of the run.

```
cargo build  --workspace --all-targets                 exit 0   22 Compiling   Finished in 11.19s
cargo clippy --workspace --all-targets --all-features
             -- -D warnings                            exit 0   22 Checking    Finished in 2.93s
             warning/error lines in build + clippy logs: 0 / 0
cargo fmt    --all -- --check                          exit 0   (0 bytes of output)
cargo deny   check                                     exit 0
             advisories ok, bans ok, licenses ok, sources ok
```

The 22 names in the `Compiling` and `Checking` lists are identical: the 14
`envoy-*` crates + `differential` + `h2spec-conformance` + the six helpers.
(`cargo deny` also prints its pre-existing `license-not-encountered` WARNING
for an unmatched allowance in `deny.toml`; it is a warning, all four checks
read `ok`, and it predates this phase.)

⚠ **A 2.93 s clippy over 22 crates looked too fast to be a real lint pass, so
it was given a NEGATIVE CONTROL** rather than believed: a four-line function
`pub fn clippy_negative_control_probe() -> bool { let v = vec![1u8]; v.len() == 0 }`
was appended to `crates/envoy-filter/src/health_check.rs` and the same clippy
command re-run:

```
exit 101
    Checking envoy-filter v0.1.0 (/home/esa/git/envoy-rust/crates/envoy-filter)
error: length comparison to zero
error: useless use of `vec!`
error: could not compile `envoy-filter` (lib) due to 2 previous errors
```

The file was restored with `git checkout --` and **md5-verified** against its
pre-mutation hash (`RESTORED-md5-ok`, `git status --porcelain` empty), and a
re-run exited 0. So the fast pass is incremental reuse of the type-check, and the
lints genuinely execute.

## Legs (a) and (b) — the differential corpus

The harness runs the DEBUG `envoy-bin`; `cargo build -p envoy-bin` exit 0 was
run first (md5 `b17766df2a50529bafa71a931bbfe434`). Then, redirected to a file
(never through `tail`), ANSI-stripped and censused by regex with `ok` and
`FAILED` rows counted separately:

```
$ cargo test --workspace --no-fail-fast        (exit 101, real 6m53.758s)
log bytes 262738   test-result rows 172   ok 163   FAILED 9
passed 2336   failed 9   passed + failed = 2345   Running lines 156   Doc-tests 16
```

**`binaries = 172` and `passed + failed = 2345` reproduce EXACTLY** — the
state-3 local run, `PLAN.md`'s prediction, and the CI identity on `9aa367c`
(`binaries=172 passed=2345 failed=0`). The flake-vs-regression identity closes:
if this phase had broken a test, the sum would not match CI's `passed`.

### Leg (a) — fixtures `0095` and `0096`, this phase's own witnesses

```
     Running tests/http_filter_health_check.rs (target/debug/deps/http_filter_health_check-3794e3931298f76c)
test http_filter_health_check_fixture ... ok
     Running tests/http_filter_health_check_stats.rs (target/debug/deps/http_filter_health_check_stats-b527d25bf1a19757)
test http_filter_health_check_stats_fixture ... ok
```

Both GREEN. Both keep their configs **byte-identical** across the two proxies
(`cmp` exit 0), with the span stated alongside the hash:

```
0095 envoy.yaml == envoy-rust.yaml   2484 bytes   md5 85b837a66319583bf8c5a81f6b15f4e1
0096 envoy.yaml == envoy-rust.yaml   1738 bytes   md5 e66e8ad0cc20d41cf5d7f12ce42ecd65
```

### Leg (b) — the other fixtures, and the nine local reds

Censused from the `---- <name> stdout ----` markers — exactly the nine that
state 3 recorded:

```
access_log_h2_rcd_upstream_reset
access_log_h2_uc_upstream_reset
access_log_rcd_upstream_reset
access_log_rf_upstream_reset
access_log_upstream_host
admin_config_dump_server_info
lb_maglev_fixture
lb_ring_hash_fixture
lb_subset_fixture
```

**Isolation on THIS tree**, each alone with a 20-second settle gap:

```
access_log_h2_rcd_upstream_reset | exit=101 | test result: FAILED. 0 passed; 1 failed
access_log_h2_uc_upstream_reset | exit=101 | test result: FAILED. 0 passed; 1 failed
access_log_rcd_upstream_reset | exit=101 | test result: FAILED. 0 passed; 1 failed
access_log_rf_upstream_reset | exit=101 | test result: FAILED. 0 passed; 1 failed
admin_config_dump_server_info | exit=101 | test result: FAILED. 0 passed; 1 failed
access_log_upstream_host | exit=101 | test result: FAILED. 0 passed; 1 failed
lb_maglev | exit=101 | test result: FAILED. 0 passed; 1 failed
lb_ring_hash | exit=101 | test result: FAILED. 0 passed; 1 failed
lb_subset | exit=101 | test result: FAILED. 0 passed; 1 failed
ISOLATION DONE 2026-09-21T04:51:17-04:00
```

⚠⚠ **The finding of this gate: Family B did NOT pass alone this time.** At
state 3 the four backend-routing fixtures passed in isolation on both trees; here
all four fail in isolation on this tree. Isolation ALONE would therefore have
classified them as deterministic — the Family-A signature — and a session
reasoning from the state-3 tell ("passes alone ⇒ parallel-load flake") would have
had nothing left to go on. `superpowers:systematic-debugging` was invoked before
any classification:

- **Where it breaks.** All four panic on the **REFERENCE side** — upstream
  Envoy's own drive, before envoy-rust's response is compared:
  `fixture green: upstream envoy http1 drive (Http1AccessLogByteExact probe 0)` /
  `fixture passes: upstream http1 drive (key \`key-0\`)` (maglev, ring_hash) /
  `fixture passes: upstream http1 drive (probe \`prod-route\` path \`/prod\`)`
  (subset), each `Caused by: deadline has elapsed`. No envoy-rust code is on
  that path.
- **What changed on the host.** `docker ps -a -q | wc -l` read **93**: a
  NEIGHBOUR workload (`cache-platform-*` builders, dataplanes and proxies, 24
  `valkey` containers) was running on the same Docker Desktop VM, with builder
  containers cycling second by second. The hypothesis: a loaded Docker VM makes
  the reference container's backend route miss its deadline even with no
  parallel test load, so the load-sensitive family now fails "alone".
- **The control.** A detached worktree at `04661b7` (the pre-phase commit;
  `0095` verified ABSENT there) with its OWN `CARGO_TARGET_DIR`, built
  `--workspace --all-targets` (exit 0; `envoy-bin` md5
  `4aad5f4f72b552a7769f3fcb1412fdf5` vs this tree's
  `b17766df2a50529bafa71a931bbfe434`). Each Family-B fixture was run alone on
  the control and on this tree **interleaved**, control first, 20 s apart, so
  both sides saw the same host conditions:

```
control | access_log_upstream_host | test result: FAILED. 0 passed; 1 failed | fixture green: upstream envoy http1 drive (Http1AccessLogByteExact probe 0)
phase   | access_log_upstream_host | test result: FAILED. 0 passed; 1 failed | fixture green: upstream envoy http1 drive (Http1AccessLogByteExact probe 0)
control | lb_maglev                | test result: FAILED. 0 passed; 1 failed | fixture passes: upstream http1 drive (key `key-0`)
phase   | lb_maglev                | test result: FAILED. 0 passed; 1 failed | fixture passes: upstream http1 drive (key `key-0`)
control | lb_ring_hash             | test result: FAILED. 0 passed; 1 failed | fixture passes: upstream http1 drive (key `key-0`)
phase   | lb_ring_hash             | test result: FAILED. 0 passed; 1 failed | fixture passes: upstream http1 drive (key `key-0`)
control | lb_subset                | test result: FAILED. 0 passed; 1 failed | fixture passes: upstream http1 drive (probe `prod-route` path `/prod`)
phase   | lb_subset                | test result: FAILED. 0 passed; 1 failed | fixture passes: upstream http1 drive (probe `prod-route` path `/prod`)
DONE 2026-09-21T04:56:07-04:00 containers=76
```

  and Family A alone on the same control:

```
control | access_log_h2_rcd_upstream_reset | test result: FAILED. 0 passed; 1 failed
control | access_log_h2_uc_upstream_reset | test result: FAILED. 0 passed; 1 failed
control | access_log_rcd_upstream_reset | test result: FAILED. 0 passed; 1 failed
control | access_log_rf_upstream_reset | test result: FAILED. 0 passed; 1 failed
control | admin_config_dump_server_info | test result: FAILED. 0 passed; 1 failed
```

**All nine fail identically at the pre-phase control, at the same panic site
and on the same upstream drive.** A test that fails the same way before the
phase existed was not broken by the phase; CI on native Linux — which reported
`failed=0` on this exact code tree at `9aa367c` — is authoritative for all nine.
**The lesson is the one the handoff stated, sharpened:** the Family-B tell
("passes alone") is a property of the host's load at the time, not of the
fixture. It held at state 3 and did not hold here. **Only the control
discriminates; isolation is an input to it, never a verdict on its own.** The
control worktree was removed afterwards (`git worktree list` shows no scratchpad
entry).

## Leg (c) — conformance

**The local h2spec gate self-skips and is worth nothing here.** `which h2spec`
is empty and `tools/` does not exist, so `h2spec_runner.rs` takes its
`h2spec not found` early-return and reports a vacuous `h2spec_pass_rate_gate ...
ok`. **Leg (c) is CI-AUTHORITATIVE**: CI installs the pinned h2spec 2.6.0, and
the positive control that it genuinely executed is `h2spec not found` = 0 in the
ANSI-stripped CI job log — recorded for `9aa367c` at state 3 and re-checked on
this advance's own push by the follow-up CI record. **`known-failures.txt` was NOT
trimmed** — its `3.5/2` entry passes on this host and fails in CI.

## Leg (d) — fuzzing

**Phase 115 added NO fuzz target** — verified, not assumed: `git diff
--name-status 04661b7..HEAD -- '*fuzz*'` is EMPTY. The letter of (d) is
therefore vacuous, so all five pre-existing targets were run at the CI contract
(`cargo +nightly fuzz run <target> -- -max_total_time=30`, from the CRATE
directory), with the seed-corpus line asserted so an empty-corpus exit 0 cannot
pass for a run:

```
parse_bootstrap        | exit 0 | INFO: seed corpus: files: 13014 | Done 200086 runs in 31 second(s)   | crash lines 0
jwt_parse              | exit 0 | INFO: seed corpus: files: 6250  | Done 6062006 runs in 112 second(s) | crash lines 0
cdn_loop_parse         | exit 0 | INFO: seed corpus: files: 1638  | Done 10300821 runs in 31 second(s) | crash lines 0
accesslog_format_parse | exit 0 | INFO: seed corpus: files: 3002  | Done 3534523 runs in 31 second(s)  | crash lines 0
grpc_health_decode     | exit 0 | INFO: seed corpus: files: 216   | Done 32081561 runs in 31 second(s) | crash lines 0
```

`parse_bootstrap` covers this phase's new config surface (the `health_check`
typed config and its validator). The corpus sizes above are LOCAL, mostly
gitignored artifacts; the TRACKED seed counts are **67 / 3 / 0 / 11 / 1** —
`cdn_loop_parse` still has **0** tracked seeds (`CF-75-5`, open).

## The four files that had to stay untouched

```
git diff --numstat 04661b7..HEAD -- <file>
Cargo.toml                       UNTOUCHED
Cargo.lock                       UNTOUCHED
.github/workflows/ci.yml         UNTOUCHED
tests/differential/src/lib.rs    UNTOUCHED
POSITIVE CONTROL (same probe, files the phase DID touch):
crates/envoy-config/src/bootstrap.rs     283  0
crates/envoy-filter/src/health_check.rs  274  0
```

## The SPEC's six deliverables, checked on disk

1. `HealthCheckFilterConfig` (`crates/envoy-config/src/bootstrap.rs`) carries
   `pass_through_mode: bool` with NO `serde(default)` (absent is boot-fatal),
   `headers: Vec<HeaderMatcher>`, the RECOGNIZED `cache_time` and
   `cluster_min_healthy_percentages`, and the `serde(skip)` `local_cluster`.
2. `ConfigError::UnsupportedHealthCheckField` and
   `ConfigError::UnsupportedHealthCheckPseudoHeader` exist
   (`crates/envoy-config/src/lib.rs`).
3. `crates/envoy-filter/src/health_check.rs` exists (274 lines; `HEALTH_CHECK_OK`
   = `"health_check_ok"`, `X_ENVOY_UPSTREAM_HEALTHCHECKED_CLUSTER`), and
   `HttpFilterInstance` has **13** production variants, `HealthCheck` the 13th
   (plus the two `cfg(feature = "test-util")` test variants).
4. `FilterResponse` carries `pub details: Option<&'static str>`
   (`crates/envoy-filter/src/types.rs`).
5. Fixture `0095` (and `0096`, added by `ADR-0201`) are GREEN above.
6. `BEHAVIOR_CONTRACT.md` grew `72 0` over the phase arc — additions only.

## Doctrine checks

```
#![forbid(unsafe_code)] in all 14 crates/*/src/{lib,main}.rs: 14 present, 0 missing
over `git diff -U0 04661b7..HEAD -- crates tests`, added lines:
  #[allow( 0   #[expect( 0   unsafe 0   #[ignore 0   todo! 0   unimplemented! 0   dbg! 0
  POSITIVE CONTROL, same probe: #[test]/#[tokio::test] = 30   (= PLAN.md's 30 new test functions)
```

## Size, re-measured at this commit

```
git diff --numstat 04661b7..HEAD -- . ':(exclude)docs/'
  files=23  insertions=1454  deletions=55  NET=1399   (1.0007x the MEASURED 1398)
```

Identical to state 3's figure; §6.1 does not fire on either axis at the LANDED
number (8 tasks against ~25; 1399 against ~1500). The single line of drift is
still `bootstrap.rs`'s blank separator (283 vs 282) and was not re-litigated.

## What this session did NOT do

- **No code, test or fixture changed.** The gate is docs-only; the one clippy
  negative-control mutation was restored and md5-verified before anything else ran.
- **`ROADMAP.md` untouched**; row `115` remains `planned`.
- **No ADR fired**, no carry-forward consumed, nothing fixed — including
  **CF-115-9** (the `fault` filter's `safe_regex_match` request-time panic).
- **`known-failures.txt` untouched.**
- **Leg (f) not attempted** — `REVIEW.md` is state 5's. No §5 state was chained.

## For the state-5 code review

- The nine local reds are adjudicated here **by a pre-phase control under
  the same host conditions**. Do not re-litigate them. Do NOT rely on the
  "Family B passes alone" tell: it did not hold at this gate.
- `SPEC.md`, `ADR-0200` and `ADR-0201` are landed and uneditable; where `SPEC.md`
  and `PLAN.md` disagree, `PLAN.md` wins on all seven counts `ADR-0201` records.
- `CF-115-1` … `CF-115-10` stand; `CF-114-6` is CONSUMED; `CF-75-5` open.
- The next free ADR number is **`ADR-0202`**.
