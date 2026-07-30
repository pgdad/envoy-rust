# Sub-phase 76.1 — PROGRESS

**Phase:** `76.1-redirect-config-surface` — the CONFIG SURFACE slice of the
`Route.redirect` action. Parent: `76-route-redirect-action` (`in-progress`).
Sibling: `76.2-redirect-runtime-fixture` (`planned`, must not start before `76.1`
is `done`).

**§5 state:** this document records the **state-3 IMPLEMENTATION** session, run
via `superpowers:executing-plans` per `PLAN.md`'s Execution stance (serial, main
session — seven of the eight tasks edit `crates/envoy-config/src/bootstrap.rs`,
and Tasks 3-5 form a strict compile-order chain).

**Predecessor `HEAD`:** `cf5cf85d0a2c477b90636b74fd93f6d36038f890` (the state-2
PLAN-write). Tree clean, branch `main`, in sync with `origin/main` at session
start.

**THIS SESSION DID NOT RUN THE §7.5 GATE.** State 4 is a SEPARATE session
(§5.1; ADR-0127 — the context that wrote the code must not be the one that grades
it). What Task 8 did is *rehearse* the cheap local half so state 4 does not open
on an avoidable RED. No `REVIEW.md` was written; no `stop` file was created.

---

## Contents

1. Summary and the eight commits
2. Per-task record (what changed, RED evidence, GREEN evidence, verbatim output)
3. The three mutation checks — including two findings that corrected the plan
4. Gate-by-gate status, stated explicitly (including the VACUOUS ones)
5. Measured census deltas
6. Deviations from `PLAN.md`, and why
7. What state 4 must do

---

## 1. Summary and the eight commits

All 8 `PLAN.md` tasks are landed, TDD-first, each committed separately. One extra
commit (`68e39b1`) records a test-strengthening that a mutation check forced — see
§3.

| commit | task |
|---|---|
| `3e8dd80` | Task 1: `RedirectResponseCode` — the five-value wire enum + `status()` |
| `20dd682` | Task 2: `RedirectAction` schema — presence-preserving `Option`s, no port bound |
| `fea479b` | Task 3: `RouteAction::Redirect` + both `Serialize` arms + re-export + honest `synth_501` placeholder (T-C9) |
| `27c8a05` | Task 4: `Route` visitor accepts `redirect:` — six-name key list + three-way cardinality |
| `68dd907` | Task 5: the two `RedirectAction` oneof validators — presence-not-truthiness |
| `c8002da` | Task 6: end-to-end accept direction + both `Serialize` round-trips |
| `68e39b1` | Task 6 (cont.): anchor both `Serialize` key assertions to column 0 — Mutation B exposed a VACUOUS substring check |
| `c5e1024` | Task 7: `parse_bootstrap` corpus seed + its `!`-un-ignore line |

Task 8 (this document + the `STATE.md` advance) is the final commit.

**Files touched — exactly the five `PLAN.md` permits, and nothing else:**

```
 crates/envoy-config/fuzz/.gitignore                                     |  1 +
 crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml | 37 +
 crates/envoy-config/src/bootstrap.rs                                    | 655 +, 10 -
 crates/envoy-config/src/lib.rs                                          |  28 +,  8 -
 crates/envoy-http1/src/hcm.rs                                           |  72 +,  1 -
```

No `tests/` file. No fixture. No `BEHAVIOR_CONTRACT.md` line. No `ci.yml` edit.
No new crate, no new dependency, no new fuzz target. No landed ADR edited. Both
`SPEC.md` files left byte-unchanged.

**Anchors re-verified on disk by TEXT before editing** (the plan's own instruction,
because line numbers drift). All confirmed exactly as `PLAN.md` states, with **two
citations that had drifted**:

| citation | plan says | actual on disk |
|---|---|---|
| the `hcm.rs` test-module `use envoy_config::{…}` | `:2361` | **`:2353`** — an 8-line drift |
| `build_response_subset_match_populated_from_metadata_match` (T-C9 insertion anchor) | "immediately before" | `:9680` |
| everything else (`bootstrap.rs` 20 397 lines; enum `:2178`; `Serialize for Route` `:2529`; `Serialize for RouteAction` `:2554`; `validate_hcm` match `:3981`; the leave-alone dispatch `:4053`; `hcm.rs` dispatch `:2110`; `synth_501` `:2346`; `ConfigError` 123 variants; `.gitignore` 66/63/63) | — | **exact** |

The `bootstrap.rs` = **20 397 lines** figure (not the "~14 400" that `76.1/SPEC.md`
§0 asserts) is re-confirmed. Do not propagate the wrong figure.

---

## 2. Per-task record

### Task 1 — `RedirectResponseCode`

**Changed:** inserted the five-value enum + `impl … status() -> u16` into
`bootstrap.rs` immediately before `pub enum RouteAction`. CamelCase Rust variants
+ `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` + `#[default]`, the landed house
idiom (`LbSubsetFallbackPolicy` `:366-373`, `HashFunction` `:439-450`).
Clippy-clean with NO `#[allow(non_camel_case_types)]` and no explicit
`#[serde(rename)]`.

**RED** — `cargo test -p envoy-config --lib redirect_response_code`, exit 101:

```
error[E0433]: cannot find type `RedirectResponseCode` in this scope
```

counted: 12 × `error[E0433]` + 3 × `error[E0425]`, and
`error: could not compile 'envoy-config' (lib test) due to 15 previous errors`.
(The plan predicted 5 occurrences; the house-style individually-named tests
produce more references than the pre-flight's compressed loop-driven equivalents.
Same class of error, same honest RED for greenfield code.)

**GREEN** — same command, exit 0:

```
   Compiling envoy-config v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-config)
test bootstrap::tests::redirect_response_code_defaults_to_moved_permanently ... ok
test bootstrap::tests::redirect_response_code_maps_to_status ... ok
test bootstrap::tests::redirect_response_code_rejects_unknown_name ... ok
test bootstrap::tests::redirect_response_code_parses_all_five_wire_names ... ok
test bootstrap::tests::redirect_response_code_rejects_numeric_literal ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 649 filtered out; finished in 0.00s
```

`Compiling envoy-config` is present — a forced rebuild, not a stale-binary FALSE
PASS. `cargo fmt --all -- --check` exit 0; `clippy -p envoy-config --all-targets
--all-features -- -D warnings` exit 0 with **0** `^(warning|error)` lines.

### Task 2 — `RedirectAction`

**Changed:** the eight-field struct, derived de/serialize with
`#[serde(deny_unknown_fields)]`, inserted after Task 1's `impl` block.
`https_redirect: Option<bool>`; `path_redirect`/`prefix_rewrite`/`scheme_redirect`
`Option<String>`; `port_redirect: Option<u32>` with **no range bound**.

**RED** — `cargo test -p envoy-config --lib redirect_action`, exit 101:

```
error[E0425]: cannot find type `RedirectAction` in this scope
```

counted: 8 occurrences, `due to 8 previous errors`.

**GREEN** — exit 0:

```
   Compiling envoy-config v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-config)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 654 filtered out; finished in 0.00s
```

fmt exit 0; clippy exit 0, 0 warning/error lines.

### Task 3 — `RouteAction::Redirect` + all four `match` sites + the re-export

**Changed** — the indivisible "compiles again" task. Four non-exhaustive `match`
sites break simultaneously and all four are handled here:

1. `impl Serialize for Route` — third arm `serialize_entry("redirect", rd)`.
   **`len` at `:2535` deliberately UNCHANGED** — it is `2 + …` where the `2`
   covers `match` plus exactly one action key.
2. `impl Serialize for RouteAction` — a SEPARATE impl; `Route::serialize` does not
   delegate, so it needs its own arm.
3. `validate_hcm`'s `match &r.action` — **INERT here on purpose**, so Task 5's
   tests are genuinely RED.
4. `build_response_in`'s `match &route.action` in `hcm.rs` — the honest
   `synth_501` placeholder.

Plus the two edit sites `SPEC.md` §4.4 misses: the `lib.rs` `pub use bootstrap::{…}`
re-export list (an explicit, alphabetically-sorted list, NOT a glob) and the
`hcm.rs` test-module import.

**`bootstrap.rs:4053`'s second dispatch (`if let RouteAction::Route(..)`, the
hash-policy walk) was LEFT ALONE** — it does not break on a new variant.

**RED** — `cargo test -p envoy-http1 --lib redirect`, exit 101:

```
error[E0422]: cannot find struct, variant or union type `RedirectAction` in this scope
error[E0599]: no variant or associated item named `Redirect` found for enum `envoy_config::RouteAction` in the current scope
```

**THE RUSTFMT REFLOW HAPPENED, exactly as the plan warned.** `cargo fmt --all`
then `git diff --stat crates/envoy-config/src/lib.rs`:

```
 crates/envoy-config/src/lib.rs | 16 ++++++++--------
 1 file changed, 8 insertions(+), 8 deletions(-)
```

Inserting two names pushed every following name across line boundaries and
rustfmt reflowed the whole remaining tail of the block. **That reflow is
rustfmt's, not hand-written** — the list was never hand-wrapped, and
`cargo fmt --all -- --check` exits 0. (The plan framed the same phenomenon as
`+29 −9`; the shape of the diff depends on where the inserted line breaks, but the
tail reflow is the same event. What matters is that `fmt --check` is clean.)

**GREEN:**

```
build exit=0                       (cargo build --workspace --all-targets, 0 error lines)
   Compiling envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
test hcm::tests::build_response_redirect_is_not_implemented_placeholder ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 186 filtered out; finished in 0.00s
```

Regression, `cargo test -p envoy-config --lib`:

```
test result: ok. 660 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

`clippy --workspace --all-targets --all-features -- -D warnings` exit 0, 0
warning/error lines, **13 `Checking` lines** — the ADR-0150 `envoy-accesslog` seam
witness figure. **No 14th line: the seam holds.**

### Task 4 — the `Route` visitor

**Changed:** five edits — `expecting` widened; a
`let mut redirect: Option<RedirectAction> = None;` accumulator; a `"redirect" =>`
key arm with the same duplicate-check shape as its four peers; the unknown-field
list widened from five names to **six**; and the cardinality check widened from a
2-tuple to a three-way exactly-one check with a catch-all `_` (five arms, not
eight tuple combinations; the three positive arms come first because a catch-all
must be last).

**The wording is deliberate:** `neither is present` kept VERBATIM from the landed
message; `both are present` → `more than one is present`.

**RED** — 4 of the 7 new tests failed. Three on the missing key:

```
parses + validates: Yaml(Error("static_resources.listeners[0].filter_chains[0].filters[0]: unknown field `redirect`, expected one of `name`, `match`, `direct_response`, `route`, `typed_per_filter_config`", line: 9, column: 15))
```

and one on the missing substring:

```
thread 'bootstrap::tests::rejects_route_with_no_action_names_all_three_arms' panicked at crates/envoy-config/src/bootstrap.rs:10576:9:
the three-way message must name `redirect` too; got: parsing bootstrap YAML: static_resources.listeners[0].filter_chains[0].filters[0]: Route must carry exactly one of `direct_response` or `route`; neither is present at line 9 column 15
```

Two of the seven passed pre-implementation, and **both are recorded honestly
rather than counted as RED**:

- `all_five_preexisting_route_keys_still_parse` — a characterization pin of
  ALREADY-correct code. Mutation-checked, §3.
- `unknown_route_key_error_names_all_six_accepted_keys` — passed for the **WRONG
  REASON**: `redirect` appeared in the message only as the *offending* key name,
  not as an accepted one. After the fix it passes for the right reason.
- (`rejects_route_with_duplicate_redirect_key` likewise rejected pre-fix for the
  wrong reason — unknown field rather than duplicate field.)

**GREEN** — `cargo test -p envoy-config --lib`:

```
   Compiling envoy-config v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-config)
test result: ok. 667 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

**The three at-risk pre-existing tests, confirmed BY NAME and NOT edited:**

```
test bootstrap::tests::rejects_route_with_unknown_top_level_key ... ok
test bootstrap::tests::rejects_route_with_both_direct_response_and_route ... ok
test bootstrap::tests::rejects_route_with_neither_direct_response_nor_route ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 664 filtered out; finished in 0.00s
```

### Task 5 — the two oneof validators + two `ConfigError` variants

**Changed:** appended `RedirectPathRewriteConflict { listener, route }` and
`RedirectSchemeRewriteConflict { listener, route }` at the END of `ConfigError`
(house style: `/// Phase NN (§ref): …` doc comment + wrapped `#[error(…)]`), and
filled in the Task-3 inert arm.

**The check is `.is_some()` and nothing else** — never truthiness, never
`!s.is_empty()`, never `.unwrap_or(false)`. That is the whole rule.

**RED** — `cargo test -p envoy-config --lib redirect_with`, exit 101:

```
error[E0599]: no variant named `RedirectPathRewriteConflict` found for enum `ConfigError`
error[E0599]: no variant named `RedirectSchemeRewriteConflict` found for enum `ConfigError`
```

(2 occurrences each, `due to 4 previous errors`.)

**GREEN:**

```
   Compiling envoy-config v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-config)
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 652 filtered out; finished in 0.00s   [redirect filter]
test result: ok. 672 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s    [full lib]
```

`ConfigError` variant count re-measured after the edit: **125** (123 → 125, as
planned).

### Task 6 — end-to-end accept direction + both `Serialize` round-trips

**Changed:** test-only. All six MEASURED acceptances driven through the FULL
`parse_bootstrap` pipeline (parse AND validate, so the Task-5 validator also
runs), all five `response_code` names end-to-end with their wire statuses, the
J6/J7/J2 rejections end-to-end, and both `Serialize` impls round-tripped.

**All 7 passed on arrival**, exactly as the plan predicted — Tasks 1-5 already
implemented the behaviour, so this is a characterization/coverage task:

```
   Compiling envoy-config v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-config)
test bootstrap::tests::rejects_numeric_response_code_end_to_end ... ok
test bootstrap::tests::rejects_unknown_response_code_name_end_to_end ... ok
test bootstrap::tests::rejects_regex_rewrite_inside_redirect_end_to_end ... ok
test bootstrap::tests::route_action_serialize_round_trips_the_redirect_key ... ok
test bootstrap::tests::route_serialize_round_trips_the_redirect_key ... ok
test bootstrap::tests::accepts_all_five_response_code_names_end_to_end ... ok
test bootstrap::tests::accepts_every_measured_redirect_acceptance_end_to_end ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 672 filtered out; finished in 0.00s
```

TDD's RED is honoured by the two mutations in §3, **not** by pretending to a
natural RED. Full suite after the task: **679 passed / 0 failed**.

### Task 7 — the `parse_bootstrap` corpus seed

**Changed:** created
`crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml` (five
distinct `redirect:` shapes over non-overlapping route prefixes: scheme-only,
host+port, path+code, prefix_rewrite+strip_query, scheme_redirect+code), added its
`!`-un-ignore line, and listed it in the existing
`fuzz_corpus_seeds_parse_or_reject_cleanly` cohort walk.

**RED** — `cargo test -p envoy-config --lib redirect_fuzz_corpus_seed`, exit 101:

```
thread 'bootstrap::tests::redirect_fuzz_corpus_seed_parses' panicked at crates/envoy-config/src/bootstrap.rs:10924:10:
the 76.1 corpus seed must exist and be readable: Os { code: 2, kind: NotFound, message: "No such file or directory" }
```

**GREEN, and the tracking verified by measurement rather than assumption:**

```
test bootstrap::tests::redirect_fuzz_corpus_seed_parses ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 679 filtered out; finished in 0.00s

=== .gitignore lines (expect 67) ===
67
=== ! lines (expect 64) ===
64
=== seed tracked? (expect the path) ===
crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml
=== tracked seed count (expect 64) ===
64
```

`git ls-files` prints the path, so the `!` line is correct and the seed is
genuinely visible to CI. Both the dedicated test and the cohort walk pass:

```
test bootstrap::tests::redirect_fuzz_corpus_seed_parses ... ok
test bootstrap::tests::fuzz_corpus_seeds_parse_or_reject_cleanly ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 678 filtered out; finished in 0.01s
```

### Task 8 — local gate rehearsal

Commands run with output **redirected to files, never piped through `tail`**:

```
fmt exit=0 size=0
build exit=0
clippy exit=0
clippy Checking lines: 0            <-- FULLY CACHED, so NOT yet evidence
clippy warning/error lines: 0
test result: ok. 680 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s   [envoy-config --lib]
test result: ok. 187 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.47s   [envoy-http1 --lib]
envoy-bin build exit=0
   Compiling envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.93s
```

**The first clippy green was fully cached (0 `Checking` lines) and was therefore
NOT accepted as evidence.** Re-run forced by `touch`ing the three changed files:

```
clippy exit=0
Checking lines: 13
warning/error lines: 0
    Checking envoy-config v0.0.0 (…)   Checking envoy-listener …   Checking envoy-cluster …
    Checking envoy-filter …            Checking envoy-tls …        Checking envoy-tcp …
    Checking envoy-http1 …             Checking envoy-http2 …      Checking envoy-admin …
    Checking http1-echo-server …       Checking envoy-health …     Checking http2-echo-server …
    Checking envoy-bin …
```

**13 `Checking` lines and `envoy-accesslog` is absent from the list** — the
ADR-0150 seam witness. No 14th line: the seam holds.

`envoy-bin` was rebuilt (`Compiling envoy-bin` present) because the differential
harness runs `target/debug/envoy-bin` and this sub-phase **adds a config key**; a
stale binary would RED every fixture with a bogus `unknown field 'redirect'`.

---

## 3. The three mutation checks — two of which corrected the plan

Every mutation was applied in its **own scratch `git worktree` created `--detach`
at `HEAD`**, never in the main tree (memory `mutation-checks-collide-with-parallel-subagents`
— a parallel agent's `git checkout` can silently revert an in-place mutation; four
sibling `.claude/worktrees/agent-*` worktrees were active this session and were
left untouched). Each run was preceded by an **UNMUTATED CONTROL from the same
worktree**, so a RED could not be a container/build artefact. Each mutation was
reverted and re-grepped as gone, and each worktree was removed. The main tree's
`git status --porcelain` was empty before and after.

Because the mutation must exercise the task's *finished* code, each task was
committed clean FIRST and the mutation then applied to a worktree at that commit.
This is the same evidence the plan asks for, and strictly safer than mutating a
main tree holding uncommitted work.

### Mutation 1 (Task 4, Step 5) — the PLAN'S LITERAL MUTATION IS MISAIMED

`PLAN.md` Task 4 Step 5 says: *"drop `"route"` from the visitor's accepted key
list … Expected: the test goes RED"*, citing the name list at `:2490`.

**MEASURED: it does NOT go RED.** Control GREEN, then with `"route"` removed from
the `M::Error::unknown_field(other, &[…])` name list:

```
   Compiling envoy-config v0.0.0 (…/scratchpad/mut-t4/crates/envoy-config)
test bootstrap::tests::all_five_preexisting_route_keys_still_parse ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 666 filtered out; finished in 0.00s
```

**Root cause:** that `&[…]` list is only the *error-message text*. Removing a name
from it does not stop the `"route" =>` key arm from matching, so `route:` still
parses and the test still passes. This is the standing trap "a PLAN-specified
mutation can target the wrong cell" — the mutation names a different cell from the
one the test pins.

**Correctly-aimed replacement:** remove the `"route" =>` KEY ARM itself, so
`route:` falls through to `other =>`. Same worktree, forced rebuild:

```
   Compiling envoy-config v0.0.0 (…/scratchpad/mut-t4/crates/envoy-config)
test bootstrap::tests::all_five_preexisting_route_keys_still_parse ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 666 filtered out; finished in 0.00s

thread '…all_five_preexisting_route_keys_still_parse' panicked at crates/envoy-config/src/bootstrap.rs:10641:45:
name+match+route parses: Yaml(Error("static_resources.listeners[0].filter_chains[0].filters[0]: unknown field `route`, expected one of `name`, `match`, `direct_response`, `route`, `redirect`, `typed_per_filter_config`", line: 9, column: 15))
```

The assertion was genuinely REACHED (it is an assertion failure, not a startup
failure), so the characterization pin is **non-vacuous**. Reverted; restored
GREEN confirmed:

```
test bootstrap::tests::all_five_preexisting_route_keys_still_parse ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 666 filtered out; finished in 0.00s
```

### Mutation A (Task 6, Step 3) — the anti-bound pin

Control GREEN (3 passed). Then a bogus `port_redirect > 65535` bound added to the
validator arm:

```
   Compiling envoy-config v0.0.0 (…/scratchpad/mut-t6/crates/envoy-config)
test bootstrap::tests::accepts_every_measured_redirect_acceptance_end_to_end ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 678 filtered out; finished in 0.00s

thread '…accepts_every_measured_redirect_acceptance_end_to_end' panicked at crates/envoy-config/src/bootstrap.rs:10800:37:
T-A2 port_redirect: 70000 (no PGV upper bound) must ACCEPT but was rejected: redirect action on listener `hcm_listener` route `` sets both `path_redirect` and `prefix_rewrite`; they are members of one oneof and are mutually exclusive
```

RED on exactly the intended cell (T-A2). Reverted.

### Mutation B (Task 6, Step 3) — THE LOAD-BEARING ONE, and it found a VACUOUS ASSERTION

`PLAN.md` predicts: `route_action_serialize_round_trips_the_redirect_key` RED while
`route_serialize_round_trips_the_redirect_key` stays GREEN, and warns "if BOTH go
red, you mutated the wrong impl."

**MEASURED: a THIRD outcome the plan did not anticipate — BOTH stayed GREEN.**

```
   Compiling envoy-config v0.0.0 (…/scratchpad/mut-t6/crates/envoy-config)
test bootstrap::tests::route_action_serialize_round_trips_the_redirect_key ... ok
test bootstrap::tests::route_serialize_round_trips_the_redirect_key ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 677 filtered out; finished in 0.00s
```

The mutation was verified correctly aimed — it hit only the `match self` impl
(`Serialize for RouteAction`), not Route's `match &self.action`. The **ASSERTION**
was the problem. Dumping the serialized output under the mutation:

```
SERIALIZED-UNDER-MUTATION-B:
route:
  port_redirect: 70000
  strip_query: false
  response_code: MOVED_PERMANENTLY
<<END
```

The assertion was `ser.contains("redirect:")` — and the FIELD name
**`port_redirect:` ends with the substring `redirect:`**. So the key assertion
could not distinguish the `redirect` MAP KEY from the `port_redirect` FIELD name,
and a mis-keyed arm satisfied it. T-C8 had no second line of defence (T-C7 has an
independent lossless `assert_eq!(&back, route)` round-trip, which would have
caught a Route-impl mis-key on its own).

**FIX (commit `68e39b1`, nothing weakened):** anchor the key to column 0 —
`ser.lines().any(|l| l.starts_with("redirect:"))` — in BOTH round-trip tests, each
carrying a comment recording why a bare `contains` is vacuous so a future reader
does not "simplify" it back.

**Re-run against the strengthened assertions — now the plan's predicted outcome:**

```
   Compiling envoy-config v0.0.0 (…/scratchpad/mut-t6b/crates/envoy-config)
test bootstrap::tests::route_serialize_round_trips_the_redirect_key ... ok
test bootstrap::tests::route_action_serialize_round_trips_the_redirect_key ... FAILED
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 677 filtered out; finished in 0.00s

thread '…route_action_serialize_round_trips_the_redirect_key' panicked at crates/envoy-config/src/bootstrap.rs:10905:9:
RouteAction::serialize must emit the `redirect` key at the top level; got:
route:
```

**One RED, one GREEN — that is the empirical proof the two `Serialize` impls are
separate**, which is what this mutation exists to demonstrate. Reverted; restored
GREEN confirmed (2 passed); worktree removed; main tree clean with both
`serialize_entry("redirect", rd)` arms present (grep count 2).

---

## 4. Gate-by-gate status — stated explicitly

These are **rehearsals**, not the §7.5 gate. State 4 adjudicates.

- **(a) New/changed differential fixtures green — VACUOUSLY MET.** This sub-phase
  adds **NO** differential fixture, by design: the runtime and fixture `0086` are
  `76.2`'s scope. **Stated explicitly so a reviewer does not read the absence as an
  oversight.** The differential surface of `76.1` is the REJECT direction (seven
  MEASURED upstream rejections, now boot-fatal here) proved in-process, plus (b).
- **(b) Pre-existing fixtures still green — NOT RUN THIS SESSION; state 4 owns it.**
  The regression argument: `RouteAction` gains a variant that no existing config
  reaches. MEASURED at the state-2 PLAN-write and unchanged here — a
  `grep -rn "redirect" tests/fixtures/ --include=*.yaml` returns 7 hits, ALL
  Prometheus metric NAMES in an allow-list in
  `0011-admin-stats-prometheus/expectations.yaml`. **Zero** fixture configures a
  `redirect:` route action, so the new arm is inert for all 85. `envoy-bin` has
  been rebuilt so state 4's differential does not run a stale binary.
- **(c) Conformance — NOT RUN; unchanged at its existing threshold.**
  `tests/conformance/h2spec/known-failures.txt` was **NOT touched** (21 lines).
  This host scores h2spec 3.5/2 as PASS where CI does not, so trimming on local
  evidence would break CI.
- **(d) Fuzz — MET, and NO `ci.yml` EDIT WAS NEEDED.** No new fuzz target, so the
  existing `parse_bootstrap` short-budget CI run satisfies the gate. The new seed
  is confirmed **tracked** by `git ls-files` (output quoted in §2, Task 7);
  `.gitignore` 66 → 67 lines, 63 → 64 `!` lines, 63 → 64 tracked seeds.
- **(e) Build / clippy / fmt / test / deny — REHEARSED CLEAN, except `cargo deny
  check`, which was NOT run this session** (state 4 owns it; note it can RED on a
  fresh unrelated RustSec advisory, which is a patch-bump, not a phase
  regression). `fmt --check` exit 0 with an EMPTY output file;
  `build --workspace --all-targets` exit 0; `clippy --workspace --all-targets
  --all-features -- -D warnings` exit 0 with 0 warning/error lines on a **forced**
  13-`Checking`-line re-run; `envoy-config --lib` 680 passed / 0 failed;
  `envoy-http1 --lib` 187 passed / 0 failed. **The full `cargo test --workspace`
  was NOT run this session** — that is state 4's job, and it must use
  `--no-fail-fast` with full output redirected to a file.
- **(f) `REVIEW.md` approved — NOT APPLICABLE.** State 5, a separate session. No
  `REVIEW.md` was written here.

---

## 5. Measured census deltas

| quantity | before | after |
|---|---|---|
| `ConfigError` variants | 123 | **125** |
| `crates/envoy-config/fuzz/.gitignore` lines | 66 | **67** |
| `^!` lines in that file | 63 | **64** |
| tracked `parse_bootstrap` corpus seeds | 63 | **64** |
| `envoy-config --lib` tests | 649 | **680** (+31) |
| `envoy-http1 --lib` tests | 186 | **187** (+1) |
| `RouteAction` variants | 2 | **3** |
| accepted `Route` keys | 5 | **6** |
| differential fixtures | 85 | **85** (unchanged, by design) |
| fuzz targets | 5 | **5** (unchanged — no new target) |
| clippy `Checking` lines on a forced envoy-config-touching re-run | 13 | **13** (ADR-0150 seam holds) |

**Net LoC, `crates/` + `tests/`, `cf5cf85..HEAD`: `added=793 deleted=19 net=774`.**

`PLAN.md` projected **≈515**. The overshoot is **+50%** and lives almost entirely
in the test half, exactly where the plan flagged the risk ("the test half (≈348) is
projected … house-style individual tests are more verbose"). It is recorded here
rather than smoothed over. **It does not re-open the §6.1 gate:** 774 is 52% of the
~1500 LoC threshold and the task count was 8 against ~25, so neither axis is close;
and §6.1's mid-execution trigger (a single task's sub-steps blowing past ~10 items)
never fired. No further split.

---

## 6. Deviations from `PLAN.md`, and why

Four, all recorded for the state-5 reviewer. Three ADD coverage; none weakens
anything; none changes the design.

1. **Task 4 Step 5's mutation was MISAIMED and was replaced with a
   correctly-aimed one.** Full evidence in §3, Mutation 1. The plan's mutation
   leaves the test GREEN because it edits the error-message name list rather than
   the key arm the test pins.
2. **Task 6's T-C7/T-C8 key assertions were STRENGTHENED** after Mutation B showed
   `ser.contains("redirect:")` is satisfied by `port_redirect:`. Commit `68e39b1`;
   full evidence in §3, Mutation B. Without this, T-C8 could not detect a mis-keyed
   `Serialize` arm at all.
3. **Task 4's `all_five_preexisting_route_keys_still_parse` gained a fifth-key
   block.** As written the plan's test body exercises only FOUR keys (`name`,
   `match`, `direct_response`, `route`), so it did not actually deliver the T-C5
   obligation. `typed_per_filter_config` cannot ride the `route_action_yaml`
   scaffold through `parse_bootstrap`, because the 23 D3 validator rejects any
   per-filter key absent from the HCM's `http_filters` set and that scaffold
   declares only the router; so the fifth key is asserted at `Route` level via a
   direct `serde_yaml::from_str::<Route>`, which is what exercises the widened
   visitor's key arm. This is the same technique the pre-existing
   `typed_per_filter_config_tests` module uses.
4. **Task 7's seed was also added to the `fuzz_corpus_seeds_parse_or_reject_cleanly`
   cohort walk**, not only to its own dedicated test. House precedent carries
   seeds in BOTH places (`hcm_codec_http2.yaml` is in the cohort list AND has its
   own test), so listing it only in the dedicated test would have left the newest
   seed as the single one absent from the cohort walk.

Two `PLAN.md` citations had drifted and were re-anchored on TEXT (§1): the
`hcm.rs` test import is at `:2353`, not `:2361`. The rustfmt reflow of the `lib.rs`
`pub use` block materialised as `+8 −8` rather than the plan's `+29 −9` framing —
the same event, shape depending on where the inserted line breaks; `fmt --check`
is clean and the list was never hand-wrapped.

**Nothing out of scope was touched.** No carry-forward was fixed (`CF-76-1`,
`CF-75-2`..`CF-75-6` all remain OPEN and untouched); the 75.1/75.2 `HeaderMatcher`
engine (`crates/envoy-config/src/matcher.rs`) was not touched; `location` was NOT
added to the 3-entry `HEADER_ALLOW_LIST`; `bootstrap.rs:4053` was left alone; no
landed ADR was edited and **no new ADR was required** (the split is settled by
ADR-0169 and every design choice here is either MEASURED upstream behaviour or an
explicit ADR-0169 DECISION).

---

## 7. What state 4 must do

State 4 is `superpowers:verification-before-completion`, a **SEPARATE session**
(§5.1; ADR-0127). It runs the full §7.5 gate:

1. `cargo build --workspace --all-targets`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — and
   **check the `Checking` count is non-zero**, or the green is cached and vacuous.
3. `cargo fmt --all -- --check`
4. `cargo test --workspace` **with `--no-fail-fast` and full output redirected to
   a file — never `tail`.** Run it 2-3× and DIFF the failing SET.
5. `cargo deny check`
6. the differential suite (all 85 fixtures) — `envoy-bin` is already rebuilt.
7. the conformance suites.

**Expect a local host-flake tail and adjudicate it by ADR-0164's four-part test
(assertion never reached + passes in isolation + absent from some sweep +
untouched by the phase's surface), never by membership in a list.** The stable
core of five is the four `access_log_*_upstream_reset` binaries and
`admin_config_dump_server_info`. **CF-75-6** (the `envoy-bin` fatal-startup
ephemeral-port-reuse family) is OPEN and actively flaky in CI: if it REDs, read
the failure TEXT and **rerun the SAME SHA** — do not weaken a test.

Arithmetic identity to re-check: `local passed + failed == CI passed`. The last
derived CI total was `binaries: 162 passed=2105 failed=0`; **this phase adds 32
tests, so both figures must GROW** — that is the point. Use
`grep -oE 'test result: (ok|FAILED)\. …'` with awk fields **4 and 6**, and assert
the binary count separately; the standing `ok`-only recipe makes `failed=0` true
by construction.

---

# §5 state-4 VERIFICATION — the full §7.5 gate

**Session:** `superpowers:verification-before-completion`, scoped to sub-phase
`76.1` only (NOT the parent `76`, NOT `76.2`). A SEPARATE session from the
state-3 implementation per §5.1 / ADR-0127 — the context that wrote the code is
not the one that grades it.

**Predecessor `HEAD` at session start:** `9556b2cdc43f2d1b505d1ed3d3ee6d1a1f42a783`
(the state-3 implementation). Tree clean, branch `main`, `origin/main` in sync at
`0 0`:

```
$ git status --porcelain
$ git rev-parse --abbrev-ref HEAD
main
$ git rev-parse HEAD
9556b2cdc43f2d1b505d1ed3d3ee6d1a1f42a783
$ git fetch origin --prune && git rev-list --left-right --count HEAD...origin/main
0	0
```

**State detection re-confirmed on disk, not inherited:**
`docs/envoy-rust/phases/76.1-redirect-config-surface/` holds `PLAN.md`,
`PROGRESS.md`, `SPEC.md` and **no `REVIEW.md`** — §5 state 4 exactly. ROADMAP row
`76.1` is `in-progress`, parent `76` is `in-progress`, `76.2` is `planned`.

**No `REVIEW.md` was written here** (state 5, a separate session). **No `stop`
file was created.** **No code was fixed** — the gate produced no RED attributable
to this phase's surface.

---

## S4.0 Inherited censuses — RE-DERIVED, not trusted

Every figure below was measured this session from the tree, per the standing
"re-derive every inherited census" trap.

```
$ ls -d tests/fixtures/*/ | wc -l
85
$ ls tests/differential/tests/*.rs | wc -l
85
$ awk -F' \| ' '/^\| [0-9]/{c[$4]++; n++} END{printf "rows=%d\n", n; for(k in c) printf "  %s = %d\n", k, c[k]}' docs/envoy-rust/ROADMAP.md
rows=107
  planned = 1
  in-progress = 2
  done = 104
$ wc -l tests/conformance/h2spec/known-failures.txt
21 tests/conformance/h2spec/known-failures.txt
$ wc -l crates/envoy-config/fuzz/.gitignore ; grep -c '^!' crates/envoy-config/fuzz/.gitignore
67 crates/envoy-config/fuzz/.gitignore
64
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/ | wc -l
64
$ grep -c '#\[error' crates/envoy-config/src/lib.rs
125
$ wc -l crates/envoy-config/src/bootstrap.rs
21042 crates/envoy-config/src/bootstrap.rs
$ git ls-files '*/fuzz/fuzz_targets/*.rs'
crates/envoy-accesslog/fuzz/fuzz_targets/accesslog_format_parse.rs
crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs
crates/envoy-filter/fuzz/fuzz_targets/cdn_loop_parse.rs
crates/envoy-http2/fuzz/fuzz_targets/grpc_health_decode.rs
crates/envoy-jwt/fuzz/fuzz_targets/jwt_parse.rs
```

**All nine inherited figures reproduce exactly.** Note `ConfigError` lives in
`crates/envoy-config/src/lib.rs`, not in an `error.rs` — a first census attempt
against `crates/envoy-config/src/error.rs` returned a tool warning
(`No such file or directory`), which is precisely the "disbelieve a clean-looking
zero" trap; the count was re-taken against the real file.

`git ls-files '*/fuzz/fuzz_targets/*.rs'` was used deliberately in place of a
`find`, so the four concurrent `.claude/worktrees/agent-*` worktrees cannot
inflate the target count.

---

## S4.1 Gate (e) — `cargo build --workspace --all-targets`

```
$ cargo build --workspace --all-targets
   Compiling envoy-config v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-config)
   Compiling envoy-listener v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-listener)
   Compiling envoy-cluster v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-cluster)
   Compiling envoy-filter v0.1.0 (/home/esa/git/envoy-rust/crates/envoy-filter)
   Compiling envoy-tls v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-tls)
   Compiling envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
   Compiling envoy-tcp v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-tcp)
   Compiling envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
   Compiling envoy-admin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-admin)
   Compiling http1-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http1-echo-server)
   Compiling envoy-health v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-health)
   Compiling http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
   Compiling envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.07s

real	0m3.108s
EXIT=0
```

**Not a cached no-op:** **13** `Compiling` lines, i.e. real work on the
`envoy-config` dependency cone. The cached-green trap (a build/clippy exit 0 with
ZERO `Compiling`/`Checking` lines) does not apply here. The 3.07 s wall against
34.4 s of user time is this host's core count, not a skip.

---

## S4.2 Gate (e) — `cargo fmt --all -- --check`

```
$ cargo fmt --all -- --check
EXIT=0
$ wc -c -l fmt.txt
0 0 fmt.txt
```

Exit 0 with a **byte-empty** output file — zero formatting diff.

---

## S4.3 Gate (e) — `cargo clippy --workspace --all-targets --all-features -- -D warnings`, FORCED

The three files this phase changed were `touch`ed first so the run could not be
a cached green (the state-3 session hit exactly that trap, and `PROGRESS.md` §4
warns about it):

```
$ touch crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs crates/envoy-http1/src/hcm.rs
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Checking envoy-config v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-config)
    Checking envoy-listener v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-listener)
    Checking envoy-cluster v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-cluster)
    Checking envoy-filter v0.1.0 (/home/esa/git/envoy-rust/crates/envoy-filter)
    Checking envoy-tls v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-tls)
    Checking envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
    Checking envoy-tcp v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-tcp)
    Checking envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
    Checking envoy-admin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-admin)
    Checking http1-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http1-echo-server)
    Checking envoy-health v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-health)
    Checking http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Checking envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.94s

real	0m1.995s
EXIT=0
$ grep -c 'Checking' clippy.txt
13
$ grep -cE '^(warning|error)' clippy.txt
0
```

**13 `Checking` lines with `envoy-accesslog` ABSENT — the ADR-0150 seam witness
figure, re-measured independently at state 4. There is no 14th line: the seam
holds.** Zero warning/error lines; exit 0.

---

## S4.4 Gate (e) — `cargo deny check`

```
$ cargo deny check
warning[license-not-encountered]: license was not encountered
   ┌─ /home/esa/git/envoy-rust/deny.toml:54:6
   │
54 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /home/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /home/esa/git/envoy-rust/deny.toml:52:6
   │
52 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /home/esa/git/envoy-rust/deny.toml:48:6
   │
48 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /home/esa/git/envoy-rust/deny.toml:50:6
   │
50 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
EXIT=0
```

**Exit 0 — `advisories ok, bans ok, licenses ok, sources ok`.** The five
`license-not-encountered` lines are unmatched ALLOWANCES in `deny.toml`, i.e.
allow-list entries matching no current dependency — advisory, not violations, and
the documented benign observation. **No fresh RustSec advisory fired**, so no
dependency patch-bump was needed.

---

## S4.5 Gate (e) — `cargo test --workspace`, sweep 1 of 2

Run with `--no-fail-fast` and the FULL output redirected to a file. **Never
`tail`** — `tail` truncates the `failures:` block.

```
$ cargo test --workspace --no-fail-fast   # full output -> test1.txt

real	5m56.540s
EXIT=101
```

Census, using the self-validating recipe (`grep -oE 'test result: (ok|FAILED)'`,
awk fields **4** and **6**, with the binary count asserted separately — the
standing `ok`-only recipe makes `failed=0` true by construction):

```
$ grep -c 'test result:' test1.txt
162
$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' test1.txt \
    | awk '{p+=$4; f+=$6; n++} END{printf "binaries: %d passed=%d failed=%d\n", n, p, f}'
binaries: 162 passed=2129 failed=8
$ grep -oE 'test result: (ok|FAILED)\.' test1.txt | sort | uniq -c
      8 test result: FAILED.
    154 test result: ok.
```

**THE ARITHMETIC IDENTITY CLOSES EXACTLY.** The last derived CI total was
`binaries: 162 passed=2105 failed=0`. This phase adds **32** tests
(`envoy-config --lib` 649 → 680 = +31, `envoy-http1 --lib` 186 → 187 = +1). So:

```
local passed + local failed  =  2129 + 8  =  2137
CI passed + phase delta      =  2105 + 32 =  2137     ✅ identical
```

Both figures GREW by exactly the expected amount, and the binary count is
unchanged at **162** (no new test binary — consistent with "no `tests/` file was
touched"). This is the strongest single piece of evidence that all 8 REDs are
environmental: every test that exists locally is accounted for, and the 8 are
precisely the ones CI passes.

### The 8 RED binaries

```
$ awk '/^     Running/{bin=$2} /^test result: FAILED/{print bin}' test1.txt
tests/access_log_h2_rcd_upstream_reset.rs
tests/access_log_h2_uc_upstream_reset.rs
tests/access_log_rcd_upstream_reset.rs
tests/access_log_rf_retry_exhausted.rs
tests/access_log_rf_upstream_reset.rs
tests/access_log_route_name.rs
tests/access_log_upstream_cluster.rs
tests/admin_config_dump_server_info.rs
```

All eight live in `tests/differential/tests/`, so every isolation re-run below
passes `-p differential` explicitly — **33 test-binary NAMES are duplicated
between `tests/differential/tests/` and `crates/envoy-bin/tests/`**, and a bare
`--test <name>` would silently address the wrong crate.

---

## S4.6 ADR-0164 adjudication of the RED tail — by the four-part test, never by list membership

The failure TEXT splits the eight into two classes, and **the classification is
made by ISOLATION, not by text** (a whole-sweep startup wave can preempt a core
member's normal assertion — `admin_config_dump_server_info` did exactly that
here, failing in the sweep with the startup-wave text and in isolation with its
own bridge-IP text).

### Isolation re-runs — all eight, one at a time

```
$ for t in <the eight>; do cargo test -p differential --test $t; done
access_log_h2_rcd_upstream_reset | exit=101 | test result: FAILED. 0 passed; 1 failed
access_log_h2_uc_upstream_reset  | exit=101 | test result: FAILED. 0 passed; 1 failed
access_log_rcd_upstream_reset    | exit=101 | test result: FAILED. 0 passed; 1 failed
access_log_rf_upstream_reset     | exit=101 | test result: FAILED. 0 passed; 1 failed
access_log_rf_retry_exhausted    | exit=0   | test result: ok. 1 passed; 0 failed
access_log_route_name            | exit=0   | test result: ok. 1 passed; 0 failed
access_log_upstream_cluster      | exit=0   | test result: ok. 1 passed; 0 failed
admin_config_dump_server_info    | exit=101 | test result: FAILED. 0 passed; 1 failed
```

The `passed`/`failed` COUNTS are asserted above, not the exit code:
`cargo test -p <pkg> <name>` lies both ways (`0 passed; N filtered out` exits 0
and is a FALSE GREEN; `error: no test target named …` exits 101 exactly like a
real RED).

### Class A — the stable core of five. Fails DETERMINISTICALLY in isolation, and that determinism IS the environmental signature.

Five binaries: the four `access_log_*_upstream_reset` and
`admin_config_dump_server_info`.

**The four `*_upstream_reset` binaries — `TcpCloseBackend` IPv6-unreachable.**
Verbatim from the sweep:

```
---- access_log_rcd_upstream_reset stdout ----
fixture green: access log byte-exact mismatch: line 0 not byte-identical:
  envoy="{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:43671}\",\"rf\":\"UF\"}"
  envoy-rust="{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}"
```

`immediate_connect_error:_Network_is_unreachable` at
`remote_address:[fdc4:f303:9324::254]` is the host's IPv6 stack, not a proxy
divergence: upstream Envoy resolves the closed-backend target to an IPv6 address
this host cannot route, yielding `UF`/`remote_connection_failure`, where
envoy-rust reaches the same closed port over IPv4 and correctly reports
`UC`/`connection_termination`. Fixtures `0061`/`0062`/`0069`/`0070`.

**`admin_config_dump_server_info` — the `192.168.65.2` bridge-IP family.** In the
sweep it failed with the startup-wave text; in ISOLATION it shows its own
signature:

```
---- admin_config_dump_server_info stdout ----
fixture green: admin body rule: /clusters
Caused by:
    text_lines diverged after allow-lists:
      envoy-only:      ["backend::192.168.65.2:45187::canary::false", …, "backend::192.168.65.2:45187::hostname::host.docker.internal", …]
      envoy-rust-only: []
```

Upstream Envoy inside Docker resolves `host.docker.internal` to the Docker
Desktop bridge IP `192.168.65.2` and emits a whole host block envoy-rust (running
natively, resolving to loopback) has no counterpart for. Fixture `0014`.

### Class B — the open-ended startup-race tail. Never reached an assertion; passes in isolation.

Three binaries: `access_log_rf_retry_exhausted`, `access_log_route_name`,
`access_log_upstream_cluster`. All three fail with:

```
fixture green: upstream Envoy never became accept-ready
Caused by:
    127.0.0.1:56724 not accept-ready within 10s: Connection refused (os error 111)
```

and the four ports involved in the wave — `56724`, `56726`, `56728`, `56730` —
are **consecutive even numbers allocated in one burst**, the signature of the
reserve-then-drop `reserve_port()` contention (CF-75-6) under whole-workspace
parallel load. **`Connection refused` at container start means the assertion was
NEVER REACHED**, so the run carries no information about the proxies' behaviour
at all. All three PASS in isolation (above).

### The four-part test, leg by leg

| leg | Class A (5) | Class B (3) |
|---|---|---|
| (i) assertion never reached | NO — reached, and shows a host-environment signature (IPv6-unreachable / bridge-IP), not a proxy divergence | **YES** — `Connection refused` before any comparison |
| (ii) passes in isolation | NO — fails deterministically, **and that determinism IS the documented signature** for this family | **YES** — all three green in isolation |
| (iii) absent from some sweep | — see S4.7 (sweep 2) | — see S4.7 (sweep 2) |
| (iv) untouched by the phase's surface | **YES** | **YES** |

**Leg (iv), measured — not asserted.** The phase touched exactly five files and
**no `tests/` file at all**:

```
$ git diff --name-only cf5cf85d0a2c477b90636b74fd93f6d36038f890..HEAD
crates/envoy-config/fuzz/.gitignore
crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml
crates/envoy-config/src/bootstrap.rs
crates/envoy-config/src/lib.rs
crates/envoy-http1/src/hcm.rs
docs/envoy-rust/STATE.md
docs/envoy-rust/STATE_HISTORY.md
docs/envoy-rust/phases/76.1-redirect-config-surface/PROGRESS.md
$ git diff --numstat cf5cf85..HEAD -- crates/ tests/ | awk '{a+=$1;d+=$2} END{printf "added=%d deleted=%d net=%d\n",a,d,a-d}'
added=793 deleted=19 net=774
```

and **none of the eight fixtures configures a `redirect:` route action** — the
only fixture in the tree containing the string `redirect` at all is `0011`, and
there it is seven Prometheus metric NAMES in an allow-list:

```
$ grep -rln "redirect" tests/fixtures/ --include=*.yaml
tests/fixtures/0011-admin-stats-prometheus/expectations.yaml
$ grep -n "redirect" tests/fixtures/0011-admin-stats-prometheus/expectations.yaml
127:          - envoy_http_downstream_rq_redirected_with_normalized_path
150:          - envoy_http_passthrough_internal_redirect_bad_location
151:          - envoy_http_passthrough_internal_redirect_no_route
152:          - envoy_http_passthrough_internal_redirect_predicate
153:          - envoy_http_passthrough_internal_redirect_too_many_redirects
154:          - envoy_http_passthrough_internal_redirect_unsafe_scheme
157:          - envoy_http_rq_redirect
```

The failing binaries map to fixtures `0070`, `0069`, `0062`, `0061`, `0059`,
`0049`, `0051`, `0014` (derived FROM THE TREE via
`grep -oE 'tests/fixtures/[0-9]{4}-[a-z0-9-]+' tests/differential/tests/<name>.rs`,
never from the binary name). **Not one of them reaches the new
`RouteAction::Redirect` arm** — the arm is unreachable unless a route configures
`redirect:`, and none does.

---

## S4.7 `cargo test --workspace`, sweep 2 of 2 — leg (iii), the set DIFF

```
$ cargo test --workspace --no-fail-fast   # full output -> test2.txt

real	5m22.333s
EXIT=101
$ grep -c 'test result:' test2.txt
162
$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' test2.txt \
    | awk '{p+=$4; f+=$6; n++} END{printf "binaries: %d passed=%d failed=%d\n", n, p, f}'
binaries: 162 passed=2130 failed=7
$ grep -oE 'test result: (ok|FAILED)\.' test2.txt | sort | uniq -c
      7 test result: FAILED.
    155 test result: ok.
```

`2130 + 7 = 2137` — **the identity closes again**, on a different partition.

### The failing SET changed — which is the point of running it twice

Failing test names extracted from the `---- <name> stdout ----` markers (the
`failures:` block is NOT safe to `awk` on indentation — the failure BODY is also
indented and, measured here, injects the stray line
`text_lines diverged after allow-lists:` into any such census):

| test | sweep 1 | sweep 2 | in isolation |
|---|---|---|---|
| `access_log_h2_rcd_upstream_reset` | RED | RED | **RED (deterministic)** |
| `access_log_h2_uc_upstream_reset` | RED | RED | **RED (deterministic)** |
| `access_log_rcd_upstream_reset` | RED | RED | **RED (deterministic)** |
| `access_log_rf_upstream_reset` | RED | RED | **RED (deterministic)** |
| `admin_config_dump_server_info` | RED | RED | **RED (deterministic)** |
| `access_log_rf_retry_exhausted` | RED | — | GREEN |
| `access_log_route_name` | RED | — | GREEN |
| `access_log_upstream_cluster` | RED | — | GREEN |
| `lb_subset_fixture` | — | RED | GREEN |
| `client::tests::send_request_maps_h2_handshake_failure_to_typed_error` | — | RED | GREEN |

**Union across the two sweeps: 10 distinct tests. Intersection: exactly the
stable core of five.** Every one of the five tail members is ABSENT from at least
one sweep — **leg (iii) is satisfied for the whole tail**, and the tail's
membership varying (3 in sweep 1, 2 in sweep 2, disjoint) is the documented
open-ended-tail signature whose SIZE carries no signal.

The two sweep-2-only members, verbatim:

```
---- lb_subset_fixture stdout ----
fixture passes: upstream Envoy never became accept-ready
Caused by:
    127.0.0.1:56866 not accept-ready within 10s: Connection refused (os error 111)

---- client::tests::send_request_maps_h2_handshake_failure_to_typed_error stdout ----
thread '…' panicked at crates/envoy-http2/src/client.rs:551:22:
expected H2ClientHandshake, got Ok(ClientStream { host: "test.example", .. })
```

`lb_subset_fixture` is the same never-reached-an-assertion startup race.
The `envoy-http2` handshake test is the documented pre-existing host flake — this
host's networking lets the handshake unexpectedly SUCCEED. Both GREEN in
isolation:

```
$ cargo test -p differential --test lb_subset
test result: ok. 1 passed; 0 failed
$ cargo test -p envoy-http2 --lib client::tests::send_request_maps_h2_handshake_failure_to_typed_error
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 110 filtered out
```

(The `1 passed` is asserted, not the exit code — `0 passed; N filtered out` also
exits 0 and would be a false green.)

---

## S4.8 The stable core of five — PROVED PRE-EXISTING by a control run at the pre-`76.1` commit

The core five satisfy legs (i)-(iii) NEGATIVELY: they reach their assertion, they
fail deterministically in isolation, and they appear in both sweeps. Per the
standing rule that determinism-in-isolation IS this family's environmental
signature, that is expected — **but "it is the documented signature" is a quote,
not a measurement.** So the control was MEASURED directly.

A scratch worktree was created `--detach` at
`cf5cf85d0a2c477b90636b74fd93f6d36038f890` — the state-2 PLAN-write commit,
**before a single line of `76.1`'s code existed** — and the five were re-run
there:

```
$ git worktree add --detach <scratch> cf5cf85d0a2c477b90636b74fd93f6d36038f890
HEAD is now at cf5cf85 phase 76.1 state-2 PLAN-write: 8 TDD-ordered tasks on a re-derived ~515 LoC — no split [ADR-0170]
$ cargo build --workspace --all-targets      # exit 0
$ ls target/debug/ | grep -E '^(tcp-echo-server|http1-echo-server|http2-echo-server|envoy-bin)$'
envoy-bin
http1-echo-server
http2-echo-server
tcp-echo-server
$ for t in <the five>; do cargo test -p differential --test $t; done
PRE-76.1 access_log_rf_upstream_reset     | exit=101 | test result: FAILED. 0 passed; 1 failed
PRE-76.1 access_log_rcd_upstream_reset    | exit=101 | test result: FAILED. 0 passed; 1 failed
PRE-76.1 access_log_h2_uc_upstream_reset  | exit=101 | test result: FAILED. 0 passed; 1 failed
PRE-76.1 access_log_h2_rcd_upstream_reset | exit=101 | test result: FAILED. 0 passed; 1 failed
PRE-76.1 admin_config_dump_server_info    | exit=101 | test result: FAILED. 0 passed; 1 failed
```

and the SIGNATURES are byte-identical to the ones at `HEAD`:

```
PRE-76.1 access_log_rf_upstream_reset:
  envoy="{\"rc\":503,\"rf\":\"UF\"}" envoy-rust="{\"rc\":503,\"rf\":\"UC\"}"
PRE-76.1 access_log_rcd_upstream_reset:
  …upstream_reset_before_response_started{remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:44849}…
PRE-76.1 admin_config_dump_server_info:
  text_lines diverged after allow-lists:
    envoy-only: ["backend::192.168.65.2:43651::canary::false", …]
```

**All five fail identically at a commit that predates every line of `76.1`.
They are pre-existing host-environment divergences, not regressions.**

### A false RED was caught in the middle of this control, and it is recorded

The FIRST control attempt built only `cargo build -p envoy-bin` in the worktree.
All five "failed" — but with:

```
fixture green: spawning TcpCloseBackend
Caused by:
    0: locating tcp-echo-server binary
    1: tcp-echo-server not found at <worktree>/target/debug/tcp-echo-server; run `cargo build -p tcp-echo-server` or `cargo test --workspace`
```

**That RED never reached an assertion and is NOT evidence** — it would have
"confirmed" the desired conclusion for entirely the wrong reason. The control was
re-run after `cargo build --workspace --all-targets`, and only that second run is
quoted above. Recorded here because a control that fails for a bookkeeping reason
is the easiest way to fake a green adjudication.

### Adjudication — final

**ZERO of the ten RED tests is attributable to sub-phase `76.1`.** The verdict
rests on four independent measurements, not on list membership:

1. **The phase touched no test code at all** — five `crates/` files, `net=774`,
   and **no `tests/` file** (`git diff --name-only cf5cf85..HEAD`).
2. **The new `RouteAction::Redirect` arm is unreachable for all 85 fixtures** —
   no fixture configures a `redirect:` route action.
3. **The five deterministic members fail identically at `cf5cf85`** (§S4.8).
4. **The five non-deterministic members pass in isolation and each is absent
   from at least one of the two sweeps** (§S4.7).

And the arithmetic closes on both sweeps: `2129+8 = 2130+7 = 2137 = 2105+32`.

**No code was changed in response to any RED.** There was nothing to repair — no
§5.2-style fix was needed or made.

---

## S4.9 Gate (c) — conformance. The local green IS vacuous; CI is authoritative.

`h2spec_pass_rate_gate` reports `ok` inside the workspace sweep, but that green
carries no information locally. Re-run with `--nocapture`, which is the only way
to see it:

```
$ cargo test -p h2spec-conformance --test h2spec_runner -- --nocapture
running 3 tests
test tests::parse_summary_line_extracts_pass_fail_counts ... ok
test tests::parse_h2spec_output_extracts_section_failure_ids ... ok
h2spec_runner: h2spec not found — skipping locally
test h2spec_pass_rate_gate ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
EXIT=0
```

**`h2spec not found — skipping locally` — the gate SELF-SKIPPED.** This host has
no `h2spec` binary; CI provisions it (`.github/workflows/ci.yml:43-49`,
`install h2spec`). Per **ADR-0163** the gate is **NOT** vacuous in CI — that scare
is settled and is not re-raised here. So **gate (c) is CI-AUTHORITATIVE** and is
confirmed by this session's own CI run (§S4.11), not by the local sweep.

`tests/conformance/h2spec/known-failures.txt` was **NOT touched** — still **21**
lines. This host scores h2spec 3.5/2 as PASS where CI does not, so trimming it on
local evidence would break CI. **Threshold unchanged** (`PASS_RATE_GATE = 0.95`,
`tests/conformance/h2spec/tests/h2spec_runner.rs:18`); `76.1` adds no HTTP/2
surface at all.

---

## S4.10 Gate (a), (b) and (d)

**(a) New/changed differential fixtures green — VACUOUSLY MET, and deliberately
so.** `76.1` adds **NO** differential fixture. Fixture `0086-route-redirect-action`
belongs to `76.2`. **This is stated explicitly so a reviewer does not read the
absence as an oversight** — it is the same disposition `PROGRESS.md` §4 already
records, re-confirmed here rather than inherited. Re-derived this session:
**85** fixture dirs and **85** differential test files, unchanged. `76.1`'s
differential surface is the REJECT direction (the seven MEASURED upstream
rejections J1-J7, now boot-fatal here), proved in-process by the +32 tests, plus
gate (b).

**(b) Pre-existing differential fixtures still green — MET.** All 85 fixtures ran
in both sweeps. Every RED is adjudicated environmental in §S4.6-§S4.8, with the
five deterministic members proved to fail identically at a pre-`76.1` commit. The
regression argument holds structurally too: `RouteAction` gained a variant that
**no existing config can reach**, since no fixture configures a `redirect:` route
action (measured in §S4.6). `envoy-bin` was rebuilt from this tree before the
sweeps — 13 `Compiling` lines in §S4.1 — so no fixture ran against a stale binary
that would have rejected the new `redirect` config key.

**(d) Fuzz — MET, and NO `ci.yml` edit was needed.** `76.1` adds **no new fuzz
target**, so no `ci.yml` step is required; the existing short-budget
`parse_bootstrap` run covers the new seed. Re-verified from the tree:

```
$ git ls-files '*/fuzz/fuzz_targets/*.rs'
crates/envoy-accesslog/fuzz/fuzz_targets/accesslog_format_parse.rs
crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs
crates/envoy-filter/fuzz/fuzz_targets/cdn_loop_parse.rs
crates/envoy-http2/fuzz/fuzz_targets/grpc_health_decode.rs
crates/envoy-jwt/fuzz/fuzz_targets/jwt_parse.rs
```

**5 targets across 5 crates — unchanged**, and all five already have a `ci.yml`
step (`.github/workflows/ci.yml:102-134`). The new seed is TRACKED, not merely
present:

```
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml
crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml
$ grep -n 'route_redirect_action' crates/envoy-config/fuzz/.gitignore
65:!corpus/parse_bootstrap/route_redirect_action.yaml
$ git check-ignore -v crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml ; echo "exit=$?"
exit=1
```

`check-ignore` exit **1** = the file is NOT ignored. Without the explicit `!`
line the seed would be untracked and invisible to CI.

Although not required, the short-budget `parse_bootstrap` run was executed
locally so the new seed is covered by a real libFuzzer run and not only by the
in-process `fuzz_corpus_seeds_parse_or_reject_cleanly` cohort walk. It was run
from the CRATE directory (`cargo fuzz` does not run from the repo root) with the
same budget CI uses:

```
$ cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30
INFO:    12432 files found in /home/esa/git/envoy-rust/crates/envoy-config/fuzz/corpus/parse_bootstrap
INFO: seed corpus: files: 12432 min: 1b max: 3642b total: 6927989b rss: 55Mb
#12433	INITED cov: 17088 ft: 36158 corp: 3484/2368Kb exec/s: 6216 rss: 382Mb
…
Done 76013 runs in 96 second(s)
EXIT=0
$ grep -cE 'ERROR: libFuzzer|deadly signal|panicked at|SUMMARY: ' fuzz.txt
0
$ ls crates/envoy-config/fuzz/artifacts/parse_bootstrap/
(empty)
```

**Exit 0, zero crash markers, empty artifacts directory.** (The 12 432 files are
this host's accumulated local corpus, which is gitignored; CI starts from the
**64** TRACKED seeds. The run left the working tree clean and the tracked seed
count unchanged at 64.)

---

## S4.11 Gate-by-gate verdict

| §7.5 gate | verdict | evidence |
|---|---|---|
| **(a)** new/changed fixtures green | **VACUOUSLY MET** — no new fixture by design; `0086` is `76.2`'s | §S4.10 |
| **(b)** pre-existing fixtures still green | **MET** — all 85 ran; every RED adjudicated environmental, 5 of them proved pre-existing at `cf5cf85` | §S4.5-§S4.8, §S4.10 |
| **(c)** conformance at declared threshold | **CI-AUTHORITATIVE** — the local gate self-skips (`h2spec not found`); `known-failures.txt` untouched at 21 lines; threshold unchanged | §S4.9 |
| **(d)** new fuzzer clean short-budget | **MET, no `ci.yml` edit needed** — no new target; seed tracked; local 30 s run clean | §S4.10 |
| **(e)** build / clippy / fmt / test / deny | **MET** — build exit 0 (13 `Compiling`), clippy exit 0 (13 forced `Checking`, 0 warnings), fmt exit 0 (empty), deny exit 0 (`advisories ok, bans ok, licenses ok, sources ok`), test `2129+8 = 2130+7 = 2137` with zero phase-attributable RED | §S4.1-§S4.5, §S4.7 |
| **(f)** `REVIEW.md` approved | **NOT APPLICABLE HERE — state 5, a SEPARATE session** (§5.1; ADR-0127: the context that verified must not grade). No `REVIEW.md` was written. | — |

**Gates (a), (b), (d) and (e) are MET. Gate (c) is met at its unchanged threshold
in CI. Gate (f) is the only one still open, and it belongs to state 5.**

### Census deltas re-confirmed at state 4

| quantity | state-3 claim | state-4 measurement |
|---|---|---|
| `ConfigError` variants | 125 | **125** ✅ |
| `crates/envoy-config/fuzz/.gitignore` lines | 67 | **67** ✅ |
| `^!` lines in that file | 64 | **64** ✅ |
| tracked `parse_bootstrap` seeds | 64 | **64** ✅ |
| `bootstrap.rs` lines | 21 042 | **21 042** ✅ |
| differential fixture dirs | 85 | **85** ✅ |
| differential test files | 85 | **85** ✅ |
| fuzz targets | 5 (across 5 crates) | **5** ✅ |
| `known-failures.txt` lines | 21 | **21** ✅ |
| ROADMAP rows / done / in-progress / planned | 107 / 104 / 2 / 1 | **107 / 104 / 2 / 1** ✅ |
| clippy `Checking` lines, forced | 13 (ADR-0150 seam) | **13, `envoy-accesslog` absent** ✅ |
| net `crates/`+`tests/` LoC `cf5cf85..HEAD` | 774 | **added=793 deleted=19 net=774** ✅ |
| test binaries | — | **162** (unchanged; no new test binary) |
| workspace tests | 2105 + 32 | **2137** ✅ |

**Every inherited figure reproduced. None was refuted.**

---

## S4.12 What state 4 did NOT do

- **No `REVIEW.md`** — state 5, a separate session (§5.1; ADR-0127).
- **No code fix.** The gate produced no RED attributable to `76.1`, so there was
  nothing to repair and no §5.2-style re-entry was triggered.
- **No `SPEC.md` edit** — `76/SPEC.md`, `76.1/SPEC.md` and `76.2/SPEC.md` all stay
  BYTE-UNCHANGED (ADR-0169 DECISION 5, ADR-0170).
- **No `ROADMAP.md` edit** — a state-4 commit flips no status cell; row `76.1`
  stays `in-progress`, parent `76` stays `in-progress`, `76.2` stays `planned`.
- **No new ADR.** Nothing here is a new decision: every call is an application of
  ADR-0164's adjudication rule, ADR-0163 (h2spec in CI), ADR-0150 (the seam) and
  ADR-0127 (state isolation). Ledger head stays **ADR-0170**; **ADR-0171** is the
  next free number.
- **No landed ADR edited**; **`known-failures.txt` NOT trimmed**; **no fixture
  weakened**; **`location` NOT added to the 3-entry `HEADER_ALLOW_LIST`**.
- **No carry-forward fixed opportunistically** — **CF-76-1**, **CF-75-2**..
  **CF-75-6** all remain OPEN and untouched (§6.3). In particular the four
  `Connection refused` startup races observed here are **CF-75-6**, whose owner is
  its own phase; no test was weakened to make them go away.
- **No `stop` file.** The mission is not complete: 107 rows with **three**
  non-`done` (`76`, `76.1`, `76.2`), `76.1` implemented but unreviewed, `76.2`
  wholly unimplemented, and **four** of the 11 `ROADMAP.md` family headings still
  carrying zero rows.
- **No sub-phase `76.2` work started.** No parent-`76` `PLAN.md` was written —
  §6.2 step 1, it will never have one.

---

## S4.13 What state 5 inherits

The next session is **§5 state 5, `superpowers:requesting-code-review` → `REVIEW.md`**,
scoped to `76.1`, and it is a **SEPARATE session**. Gate (f) is the only unmet
gate. Things a reviewer should be handed rather than have to rediscover:

- **The `Option`s are load-bearing.** Upstream's oneofs are exclusive on FIELD
  PRESENCE, not value, so `https_redirect: Option<bool>` and
  `path_redirect`/`prefix_rewrite`/`scheme_redirect: Option<String>` are correct
  and the validators must test **`.is_some()`** — never truthiness, never
  `!s.is_empty()`, never `.unwrap_or(false)`. T-R8/T-R9 paired with T-A5 pin it in
  both directions. **Do not accept a "simplification" here.**
- **`port_redirect` has NO PGV bound** — `0` and `70000` both ACCEPT upstream and
  `70000` round-trips verbatim. Do not add `1..=65535`.
- **The `synth_501` placeholder is intentional** (ADR-0169 DECISION 4), pinned by
  T-C9, and `76.2` deliberately flips it. A 501 for a configured redirect is the
  CORRECT behaviour at this sub-phase — it is not an unfinished stub.
- **Error TEXT is not part of the equivalence contract** for J1-J7 — the VERDICT
  is. J2 rejects via `deny_unknown_fields` rather than upstream's oneof error: a
  different mechanism, the same verdict.
- `PROGRESS.md` §6 records **four deliberate deviations from `PLAN.md`**, all of
  which ADD coverage, and §3 records the **two mutation findings that corrected
  the plan** — a misaimed mutation and a vacuous assertion. Those are the two most
  review-relevant items in the state-3 record.
