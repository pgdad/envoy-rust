# Phase 116 — PROGRESS

> §5 **state 3** — the implementation of
> `docs/envoy-rust/phases/116-accesslog-not-health-check-filter/PLAN.md`.
> Appended to on each task completion, with REAL quoted command output. Written
> for a stranger with zero prior context (D-3.4).
>
> **The unit:** the `not_health_check_filter` access-log FILTER arm — the EIGHTH
> of upstream Envoy's twelve `envoy.config.accesslog.v3.AccessLogFilter` oneof
> arms. A sink carrying `filter: { not_health_check_filter: {} }` drops a record
> iff the downstream `envoy.filters.http.health_check` filter (phase 115)
> ANSWERED the request. Witnessed cross-proxy by the new differential fixture
> `0099-accesslog-not-health-check-filter`, and in-process on BOTH codecs.
>
> **Where `PLAN.md` and `SPEC.md` disagree, `PLAN.md` wins** (`ADR-0206`, which
> corrects three landed `SPEC.md` claims — among them a derived `Deserialize`
> that would load `not_health_check_filter: []`).

---

## Session preconditions, re-derived from disk before Task 1

`git rev-parse HEAD` = `6cd69df6aa88a14b88a794ce0f54d33dcd727219` (the
out-of-loop `chore: enable superpowers plugin at project scope` commit, which
touches no `docs/envoy-rust/` ledger, `crates/`, `tests/`, `Cargo.lock` or
`.github`) on top of `25b674476223b002eeb433b5f38dd82e56361b77`, the phase-116
§5 state-2 **CI-record** commit. `git status -sb` read `## main...origin/main
[ahead 1]` — the chore commit was UNPUSHED at entry; it rides this session's
push. `git fetch` exit 0; `origin/main` = `25b6744…`.

**No outstanding CI record is inherited, detected STRUCTURALLY** — `STATE.md`'s
`## Last commit` block was read and already carries a CI-confirmed line below
its PENDING line: run `37655678468`, attempt 1, `success`, identity
`binaries=174 passed=2360 failed=0` on `2a9c1d2534ed6196b78a9070b44f9c3d8f6d1e12`.

**The §5 state-3 detection rule was re-derived, not inherited:** the phase
directory held `SPEC.md` (286 lines) and `PLAN.md` (1951 lines) and NO
`PROGRESS.md` and NO `REVIEW.md` before this file was created.

`git check-ignore next-prompt.txt` exit **0**, run standalone — it IS ignored
and was never staged. The `.claude/worktrees/agent-*` worktrees (four, a
parallel workstream) were listed and not touched. `CARGO_TARGET_DIR` was unset
(the default absolute `target/` was used throughout).

### The three stop-condition legs, re-measured from disk — ALL THREE FALSE

- **Leg (i) — FALSE.** `ROADMAP.md`: **124 rows / 123 `done` / 1 `planned`**
  (rows by `^\| [0-9]`, status at field 4 of a `' | '` split; buckets sum to
  124 ✓). The `planned` row is `116`, file line **198**. Not touched.
- **Leg (ii) — FALSE.** **14** crates under `crates/`
  (`envoy-{accesslog,admin,bin,cluster,config,filter,health,http1,http2,jwt,listener,stats,tcp,tls}`).
  `quinn` / `wasmtime` / `tonic` / `opentelemetry` / `prost` as a dependency
  line in `git ls-files '*Cargo.toml'` = **0** each. `grep -ro 'histogram'
  crates/ | wc -l` = **0** against the `grep -ro 'gauge' crates/ | wc -l`
  control of **365** (method stated with the number).
- **Leg (iii) — FALSE.** **11** `### ` family headings, seeded at 0, reading
  **11 / 5 / 3 / 14 / 3 / 4 / 6 / 32 / 6 / 0 / 13** with **27** pre-heading
  rows, summing to 124 ✓. The zero-row family is `### WASM host family`.

**No `stop` file exists and none was created** (`ADR-0167` DECISION 2).

### Method

Every code block was EXTRACTED BY SCRIPT from `PLAN.md`'s fences (a fence
indexer over the plan; 49 fences), never retyped, and inserted at anchors each
asserted to occur exactly once. The three helper scripts the plan specifies
(`sweep116.py`, `e0063_fix.py`, `mutate_bit.py`) were likewise extracted from
their fences and saved OUTSIDE the repo. Every task's commit carries exactly the
plan's `git add` list.

---

## Task 1 — The shared health-check predicate + the sixth `should_log` parameter

**Commit:** `41b3ceb` `phase 116 task 1: one health-check predicate; widen should_log with it (behavior-neutral)`

**RED** (Step 2), the predicate's test inserted first:

```
$ cargo test -p envoy-filter --lib answered_by_health_check
error[E0425]: cannot find function `answered_by_health_check` in this scope   (×3)
error: could not compile `envoy-filter` (lib test) due to 3 previous errors
```

A compile error, as the plan predicts; the behavioural RED evidence is Task 4's
mutations. **GREEN** after Step 3:

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 227 filtered out
```

Steps 4–6: both HCM counter exclusions rewired to the predicate (pure
refactor); `LogFilter::should_log` and `FileSink::should_log` widened with the
TRANSIENT `#[allow(clippy::only_used_in_recursion)]`; the two HCM dispatch
sites pass `answered_by_health_check(record.response_code_details.as_deref())`.
All FIVE production sites (two dispatch, the `And`/`Or` recursion, the
`FileSink` delegation) were threaded by hand.

Step 7, the sweep — **exactly the plan's figures**:

```
$ python3 sweep116.py crates/envoy-accesslog/src/filter.rs crates/envoy-accesslog/src/file_sink.rs crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
crates/envoy-accesslog/src/filter.rs 77
crates/envoy-accesslog/src/file_sink.rs 4
crates/envoy-http1/src/hcm.rs 52
crates/envoy-http2/src/hcm.rs 0
```

Step 9:

```
$ grep -c 'should_log(' <four files>
filter.rs:89   file_sink.rs:6   http2 hcm.rs:4   http1 hcm.rs:56       (plan: 89 / 6 / 56 / 4)
$ cargo build --workspace --all-targets                         → Finished (exit 0)
$ cargo clippy --workspace --all-targets --all-features -- -D warnings → Finished (exit 0)
$ cargo fmt --all -- --check                                    → exit 0
$ cargo test -p envoy-filter -p envoy-accesslog -p envoy-http1 -p envoy-http2 --lib
test result: ok. 134 passed; 0 failed …    (envoy-accesslog)
test result: ok. 228 passed; 0 failed …    (envoy-filter)
test result: ok. 254 passed; 0 failed …    (envoy-http1)
test result: ok. 129 passed; 0 failed; 1 ignored …   (envoy-http2)
```

Numstat (crates/): `224 / 135`, net **89** — the plan's Task-1 row exactly.

---

## Task 2 — `LogFilter::NotHealthCheck` and its arm

**Commit:** `59e693a` `phase 116 task 2: LogFilter::NotHealthCheck drops exactly the health-check intercepts`

**RED** (the three tests appended first):

```
$ cargo test -p envoy-accesslog --lib not_health_check
error[E0599]: no variant or associated item named `NotHealthCheck` found for enum `filter::LogFilter` in the current scope   (×3)
```

Steps 3–4: the unit variant, the arm `LogFilter::NotHealthCheck =>
!is_health_check` (last arm), the doc line extended, and **Task 1's transient
allow DELETED**. **GREEN:**

```
$ cargo test -p envoy-accesslog --lib
test result: ok. 137 passed; 0 failed; …          (plan: 137)
```

Step 6, the composition mutation (`And` recursion's `is_health_check,` →
`false,`), the restored file byte-copied back from a pre-mutation copy:

```
mutated:   test filter::tests::not_health_check_arm_composes_as_a_leaf ... FAILED
           test result: FAILED. 136 passed; 1 failed      (plan: 136 passed; 1 failed)
restored:  test result: ok. 137 passed; 0 failed
```

Step 7: clippy `-D warnings` clean WITHOUT the allow — and since a fast clippy
can be a cache no-op, it was re-run after `touch crates/envoy-accesslog/src/lib.rs`:
**14** `Checking` lines, exit 0. fmt exit 0. Numstat `54 / 4`, net **50** (plan 50).

---

## Task 3 — The config arm, its map-only `Deserialize`, validation and the eighth compile arm

**Commit:** `ae5015c` `phase 116 task 3: the not_health_check_filter config arm, its map-only Deserialize, validation and compile`

**RED** (Steps 1–4; five config tests, the cardinality test grown to eight, the
H1 compile test):

```
$ cargo test -p envoy-config --lib not_health_check
error[E0422]: cannot find struct, variant or union type `NotHealthCheckFilter` in this scope   (×3)
error[E0425]: cannot find type `NotHealthCheckFilter` in this scope
error[E0560]: struct `bootstrap::AccessLogFilter` has no field named `not_health_check_filter`
```

Step 5: the field, the struct, the hand-rolled map-only visitor, the doc-comment
arm list, and the `lib.rs` re-export (`3 / 3` after `cargo fmt`, as planned).

**Step 6 — the `E0063` WORKSPACE fixpoint:**

```
round 1: crates/envoy-config/src/bootstrap.rs 4
         (loop then stopped on a NON-E0063 error)
         crates/envoy-config/src/bootstrap.rs:5954:9: error[E0027]: pattern does not mention field `not_health_check_filter`
round 2: crates/envoy-http1/src/hcm.rs 11
         Finished `dev` profile … (workspace builds)
```

**4 then 11 = 15 literals, exactly as planned.**

> **Ruling (Task 3):** the plan orders Step 6 (the `E0063` loop) before Step 7
> (the validator), but `validate_access_log_filter`'s destructure has no `..`,
> so after round 1 the build fails with `E0027` and the loop exits — the plan's
> own "the loop also stops on any OTHER compile error" note. Step 7 was applied
> between the rounds and the loop resumed. The final tree is the same either
> way. Cost if wrong: none.

Then `all_eight`'s inserted `not_health_check_filter: None,` was set to
`Some(NotHealthCheckFilter {})` (Step 2's tail), and `compile_access_log_filter`
became the eight-tuple (Step 8).

**GREEN** (Step 9):

```
$ cargo test -p envoy-config --lib   → test result: ok. 740 passed; 0 failed   (plan 740)
$ cargo test -p envoy-http1 --lib    → test result: ok. 255 passed; 0 failed   (plan 255)
```

**The `[]` trap re-proved on this tree, not inherited:** swapping the
hand-rolled `Deserialize` for `#[derive(Deserialize)] #[serde(default,
deny_unknown_fields)]` (the form `SPEC.md` §4 item 1 prescribes):

```
test bootstrap::tests::not_health_check_filter_rejects_every_measured_reject ... FAILED
test result: FAILED. 4 passed; 1 failed; … 735 filtered out      (plan: 4 passed; 1 failed)
```

restored from a byte copy → `5 passed; 0 failed`. Step 10: build, clippy
`-D warnings`, fmt all exit 0. Numstat `245 / 39`, net **206** (plan 206).

---

## Task 4 — In-process pins on BOTH codecs

**Commit:** `71b186b` `phase 116 task 4: pin not_health_check_filter in-process on H1 and H2`

Step 1 extracted the phase-115 H1 tail into `h1_config_from_bootstrap_yaml`;
Steps 2–3 added `not_health_check_h1_config` + the H1 pin and the `H2Probe`
alias + `h2_not_health_check_run` + the H2 pin.

These are **characterization pins** — they PASS when written:

```
$ cargo test -p envoy-http1 --lib not_health_check → ok. 2 passed   (the pin + Task 3's compile test)
$ cargo test -p envoy-http2 --lib not_health_check → ok. 1 passed
$ cargo test -p envoy-http1 --lib health_check     → ok. 8 passed   (Review Focus 5 kept green)
```

**Their RED evidence is Step 6 — the six production-site mutations**
(`mutate_bit.py`), each followed by `CONTROL`, both `hcm.rs` md5-verified
restored at the end:

| codec | the bit is … | result |
|---|---|---|
| H1 | `true` | `h1_not_health_check_filter_drops_exactly_the_intercepts ... FAILED` (1 passed; 1 failed) |
| H1 | `false` | same test FAILED (1; 1) |
| H1 | `req.path.starts_with("/healthz")` | same test FAILED (1; 1) |
| H1 | CONTROL | ok. 2 passed |
| H2 | `true` | `h2_not_health_check_filter_drops_exactly_the_intercepts ... FAILED` (0; 1) |
| H2 | `false` | same test FAILED (0; 1) |
| H2 | `envoy_req.path.starts_with("/healthz")` | same test FAILED (0; 1) |
| H2 | CONTROL | ok. 1 passed |

```
$ md5sum -c t4.md5
crates/envoy-http1/src/hcm.rs: OK
crates/envoy-http2/src/hcm.rs: OK
```

Only the codec's own pin fails under each mutation; Task 3's compile test is
unaffected. Step 7: clippy `-D warnings` and fmt exit 0 — the `H2Probe` alias
was carried from the plan, so `clippy::type_complexity` never fired. Numstat
`288 / 1`, net **287** (plan 287).

---

## Task 5 — Differential fixture `0099-accesslog-not-health-check-filter`

**Commit:** `3777c05` `phase 116 task 5: differential fixture 0099 for not_health_check_filter`

The four fixture files and the runner were written from the plan's fences:
`envoy-rust.yaml` 68, `envoy.yaml` 70, `expectations.yaml` 64, `README.md` 119,
runner 25 lines — the plan's per-file figures. `diff envoy-rust.yaml
envoy.yaml` shows exactly the four harness hunks (`1a2` admin, `5c6` bind,
`12a14` `generate_request_id`, `19c21` log path). The `filter:` block, by md5
over `grep -A1 '^ *filter:$'` on each side: `22ce799df9272db0bc8317652ad2e56b`
on BOTH.

The pinned image is present locally: `envoyproxy/envoy:v1.33.0
sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`.

**GREEN** (Step 5):

```
$ cargo build -p envoy-bin && cargo test -p differential --test accesslog_not_health_check_filter
test result: ok. 1 passed; 0 failed; … finished in 12.63s
```

**Step 6 — the fixture is non-vacuous**, `envoy-bin` rebuilt before EVERY run,
a settle gap between runs, `hcm.rs` md5-verified restored:

| the H1 bit is … | envoy-rust emitted | verdict |
|---|---|---|
| forced `true` | `envoy-rust emitted 0 access-log lines but 4 were expected to be logged; lines: []` | RED |
| forced `false` | `CF-71-1: an access log grew beyond 4 lines under a 2s settle (envoy_rust=6, envoy=4) — a suppressed record leaked` | RED |
| `req.path.starts_with("/healthz")` | `envoy-rust emitted 2 access-log lines but 4 were expected to be logged; lines: ["NHC GET /other 200 UA=-", "NHC GET /ua 200 UA=Envoy/HC"]` | RED |
| unmutated (`crates/envoy-http1/src/hcm.rs: OK`) | `test result: ok. 1 passed` | GREEN |

Each matches the plan's table: 0 / 6 / 2 lines, the path mutation losing
exactly probes 3 (`/healthz?x=1`) and 5 (the RBAC 403).

**PV-8** (Step 8): `git diff --stat HEAD -- Cargo.toml Cargo.lock .github
tests/differential/src/lib.rs` printed NOTHING. Numstat `346 / 0`, net **346**
(plan 346).

---

## Task 6 — `BEHAVIOR_CONTRACT.md`

**Commit:** `d7aeabe` `phase 116 task 6: BEHAVIOR_CONTRACT — the not_health_check_filter arm`

The 89-line fence inserted directly above `### Phase 75
(ADR-0156/0157/0158/0159/0161/0162): \`HeaderMatcher\` ABSENCE semantics …`
(anchor asserted unique; file line 3672 before the edit), i.e. right after the
phase-114 `grpc_status_filter` section, plus one separating blank.

```
$ grep -c '^### Phase 116 (ADR-0205/0206)' docs/envoy-rust/BEHAVIOR_CONTRACT.md
1
$ git diff --numstat
90	0	docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

---

## §5 state-3 summary — the implementation is COMPLETE

All SIX `PLAN.md` tasks landed IN ORDER, one commit each, TDD on every one:

| task | commit | numstat (crates/ + tests/) | plan row | |
|---|---|---|---|---|
| 1 — predicate + sixth parameter | `41b3ceb` | `224 / 135` net **89** | 89 | ✓ |
| 2 — `LogFilter::NotHealthCheck` | `59e693a` | `54 / 4` net **50** | 50 | ✓ |
| 3 — config arm + validator + compile | `ae5015c` | `245 / 39` net **206** | 206 | ✓ |
| 4 — H1 + H2 pins | `71b186b` | `288 / 1` net **287** | 287 | ✓ |
| 5 — fixture `0099` | `3777c05` | `346 / 0` net **346** | 346 | ✓ |
| 6 — `BEHAVIOR_CONTRACT.md` | `d7aeabe` | `90 / 0` (`docs/`, excluded) | 90 | ✓ |
| **TOTAL** | | **`1154 / 176` net 978** | **`1154 / 176` net 978** | **1.000×** |

```
$ git diff --shortstat 6cd69df HEAD -- crates tests
 12 files changed, 1154 insertions(+), 176 deletions(-)
```

**Every per-file figure of `PLAN.md`'s §6.1 table reproduced EXACTLY** (11/8,
162/78, 145/9, 3/3, 33/0, 300/75, 154/3, 25/0, 119/0, 68/0, 70/0, 64/0) — not
one line of code or whitespace drift. 522 lines under the ~1500 §6.1 gate.

### The test identity

```
$ cargo test --workspace --no-fail-fast        (exit 101; ANSI stripped; regex `test result: (ok|FAILED)\. (\d+) passed; (\d+) failed`)
binaries: 175   ok-rows: 168   FAILED-rows: 7
passed: 2367    failed: 7      passed + failed: 2374
```

**`binaries = 175` and `passed + failed = 2374` are EXACTLY `PLAN.md`'s
predicted identity** (2360 + 14: envoy-filter +1, envoy-accesslog +4,
envoy-config +5, envoy-http1 +2, envoy-http2 +1, the new fixture runner +1).
CI on native Linux is predicted to read `binaries=175 passed=2374 failed=0`.

### The seven local failures — TWO families, NEITHER of them this slice's

Classified by ISOLATION (each re-run ALONE with a settle gap), never by text:

| test | alone on this tree | family |
|---|---|---|
| `access_log_h2_rcd_upstream_reset` | FAIL — upstream renders `immediate_connect_error:_Network_is_unreachable` (IPv6 upstream address) | A: deterministic host signature |
| `access_log_h2_uc_upstream_reset` | FAIL | A |
| `access_log_rcd_upstream_reset` | FAIL | A |
| `access_log_rf_upstream_reset` | FAIL — `envoy rf=UF` vs `envoy-rust rf=UC` | A |
| `admin_config_dump_server_info` | FAIL — `admin body rule: /clusters` (Docker-bridge backend) | A |
| `admin_drain_listeners` | **ok, 2 of 2** | B: parallel-load startup race (`upstream Envoy never became accept-ready … Connection refused`) |
| `v4e_empty_envelope_reload_ticks_update_empty_and_keeps_last_good` | **ok, 2 of 2** | B: parallel-load startup race (`envoy-bin admin ready: … ConnectionRefused`) |

Family A is exactly the five the handoff and the PLAN-write recorded as failing
identically on an unmodified tree. Family B passes alone and reddens only under
the full parallel `--workspace` run — the readiness-race family. None of the
seven touches access-log filtering or the health_check filter. CI is
authoritative.

### What this session did NOT do

- **No ADR fired.** Nothing measured here contradicted a landed figure; the one
  step-order deviation is a ruling recorded under Task 3, not a decision the
  record needs. **`ADR-0207` is next free and nothing is reserved.**
- **`ROADMAP.md` was NOT touched** — row `116` stays `planned` until state 6.
- **Nothing was fixed** (§6.3; `ADR-0165`). `CF-115-3` is CONSUMED by the
  phase as planned; `CF-116-1` … `-3`, `CF-115-1`, `-2`, `-4` … `-16`, the
  phase-115 Minors, `CF-75-5` and the phase-112 ALPN rider stay banked.
- **`Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml` and
  `tests/differential/src/lib.rs` are untouched** (PV-8).
- **No subagent was dispatched**, and no whole-branch review was run here: the
  formal review is §5 state 5's, a separate session. Citation damage was not
  censused at this state.
- **No §5 state was chained.** The next unit is the §5 STATE-4 VERIFICATION
  GATE, a separate session (§5.1; `ADR-0127`).
