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
