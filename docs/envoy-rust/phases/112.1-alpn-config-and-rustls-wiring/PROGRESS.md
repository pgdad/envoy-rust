# Sub-phase 112.1 — PROGRESS (§5 state-3 implementation)

> **What this file is.** The running execution log for the §5 state-3
> implementation of sub-phase `112.1` (the ALPN config surface plus the `rustls`
> wiring on BOTH sides). One section per `PLAN.md` task, appended as that task
> landed. It records what was actually run and what it actually printed — not
> what the plan predicted. Where the two differ, the difference is stated.
>
> **Session start:** HEAD `3a2cf93e40b653d33bacbf5504206a1d5a5c0142`, branch
> `main`, `git status --porcelain` empty, `ls stop` → `No such file or
> directory`. The four `.claude/worktrees/agent-*` worktrees belong to a
> PARALLEL WORKSTREAM and were left untouched throughout.
>
> **This session does NOT** run the §7.5 gate adjudication (state 4), write
> `REVIEW.md` (state 5), flip any `ROADMAP.md` row (state 6), create any
> differential fixture or touch `tests/` (sibling `112.2`), fix any banked
> finding (§6.3; `ADR-0165`), or edit any landed artifact.

---

## Pre-flight — the plan's own claims, RE-VERIFIED on this tree

`PLAN.md` resolved its citations at `9f2010a`; the session started at `3a2cf93`.
Every load-bearing claim was re-tested before or during the task that depends on
it. **The plan's substantive engineering claims all held; five presentational /
task-boundary defects did not.** The defects are §"Where the PLAN was wrong"
below and are the subject of `ADR-0186`.

| claim (`PLAN.md`) | measured this session | verdict |
|---|---|---|
| M-R2: the `E0063` blast is FOUR, at `envoy-tls/src/tests.rs` ×3 + `envoy-tcp/src/lib.rs` ×1 | exactly four, at `tests.rs:135/240/454` and `envoy-tcp/src/lib.rs:1189` | ✅ |
| **M-R3: `cargo build --workspace` is a FALSE GREEN** — all four sites are test-only | run with all four sites unfixed: `Finished`, **exit 0** | ✅ **re-confirmed** |
| M-R5/M-R7: no new `TlsError` variant, no manifest edit, eager `Clone` twin | `TlsError` untouched; `Cargo.toml`/`Cargo.lock` untouched; `config.clone()` compiles | ✅ |
| M-R9: `ClientHello<'a>` borrowck hazard needs an owned `bool` in a block | the plan's spelling compiled first time; no `E0505` | ✅ |
| M-R11: `crates/envoy-config/fuzz` tracks **66** seeds; `tls_downstream_single_cert.yaml` is 40 lines | 66 and 40 | ✅ |
| M-R12: a new seed is gitignored by default (PLAIN `git check-ignore`) | exit **0** (ignored) before the `!` line; exit **1** after | ✅ |
| M-R13: `envoy-config` 716 / `envoy-tls` 22 / `envoy-tcp` 16 at the end | **716 / 22 / 16** | ✅ exact |
| M-R13: four mutations, four exact RED sets, green controls | reproduced exactly — see Task 8 | ✅ |
| M-R14 defect 1: `clippy::manual_contains` | the plan's fixed spelling passed `-D warnings` first time | ✅ |
| M-R14 defect 2: inserting above `fn validate_data_source(` steals its doc comment | avoided by inserting above the doc comment; verified after insertion | ✅ |
| §3: net **551** LoC | net **549** | ✅ within 0.4% |

**Stop condition, re-evaluated from disk — ALL THREE LEGS FALSE; no `stop`
file was created.** Leg (i): 120 rows / 117 `done` / 1 `in-progress` / 2
`planned`, buckets summing to the row count. Leg (ii): 14 crates, none of
`quinn`/`wasmtime`/`tonic`/`opentelemetry`/`prost` present. Leg (iii): 11 family
headings, one (`### WASM host family`) carrying zero rows. `ROADMAP.md` is
**deliberately untouched** by this state — row `112.1` flips at its state-6
close-out.

---

## Task 1 — the `alpn_protocols` field, the four `E0063` fixups, the inverted test

**RED (Step 2).** Per D-PLAN-6 the RED is produced by *inverting* the phase-03
test `rejects_unknown_field_in_common_tls_context` (renamed
`accepts_alpn_protocols_in_common_tls_context`, YAML literal left byte-identical):

```
error[E0609]: no field `alpn_protocols` on type `CommonTlsContext`
    --> crates/envoy-config/src/bootstrap.rs:8891:43
     = note: available fields are: `tls_certificates`, `validation_context`
```

**M-R3 RE-CONFIRMED — and this is the trap the plan was right to shout about.**
With all four `E0063` sites still unfixed:

```
$ cargo build --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.30s
EXIT=0
```

A plain workspace build is **green** while the tree does not compile under
`--all-targets`. Gating this task on `cargo build --workspace` would have been a
false green.

**The blast, under `--all-targets`, is exactly four** — the plan's M-R2 table
reproduced line-for-line:

```
error[E0063]: missing field `alpn_protocols` in initializer of `CommonTlsContext`
   --> crates/envoy-tls/src/tests.rs:135:33
   --> crates/envoy-tls/src/tests.rs:240:29
   --> crates/envoy-tls/src/tests.rs:454:33
    --> crates/envoy-tcp/src/lib.rs:1189:33
```

**GREEN (Step 6).**

```
cargo build --workspace --all-targets   ->  Finished
envoy-config  test result: ok. 709 passed; 0 failed
envoy-tcp     test result: ok.  16 passed; 0 failed
envoy-tls     test result: ok.  16 passed; 0 failed
```

709 = the 708 pre-existing plus the inverted test. **No pre-existing test
regressed**, which is the first in-process evidence that `#[serde(default)]`
keeps the config surface inert.

**DEVIATION — the plan's Step 1 code is not `rustfmt`-clean.** See defect C.

Commit `7173a91`, numstat `24 9` / `1 0` / `3 0`.

---

## Task 2 — `ConfigError::InvalidAlpnProtocol` and the >255-byte validator (D4′)

**RED (Step 2).**

```
error[E0599]: no variant named `InvalidAlpnProtocol` found for enum `ConfigError`
    --> crates/envoy-config/src/bootstrap.rs:8960:37
    --> crates/envoy-config/src/bootstrap.rs:8978:37
```

**Placement check (Step 4).** The validator was inserted **above
`validate_data_source`'s first doc-comment line**, never between the doc comment
and the `fn`. Verified after insertion:

```
5969-/// Validate a `DataSource` against a per-callsite restriction.
5970-///
5971-/// Cardinality: exactly one of `{filename, inline_string}` is `Some`.
5972-/// `requires` selects which side must be set; the other side must not be.
5973:fn validate_data_source(
```

`validate_data_source` still owns its own prose. **M-R14 defect 2 did not
recur.**

**GREEN (Step 6).** `test result: ok. 4 passed` — the inverted test, the
255-byte accept, and both 256-byte rejects (listener side and cluster side).
The cluster-side reject passing is itself the evidence that
`us_bootstrap_with_alpn`'s plaintext listener does its job: without it the
bootstrap is rejected `ConfigError::NoRuntime` before the cluster walk and the
assertion is vacuous.

**DEVIATION — `ds_alpn_of` had to move to Task 3.** See defect A.

Commit `43bfe7f`, numstat `142 0` (`bootstrap.rs`) / `13 0` (`lib.rs`).
**`envoy-config/src/lib.rs` measures 13, exactly the plan's row 2.**

---

## Task 3 — the remaining config-layer parse tests (characterization pins)

**GREEN on arrival (Step 2), as designed** — these are characterization pins, so
the RED comes from mutation, not from the test:

```
test bootstrap::tests::alpn_protocols_defaults_to_empty_when_absent ... ok
test bootstrap::tests::accepts_empty_alpn_protocols_list ... ok
test bootstrap::tests::accepts_empty_and_duplicate_alpn_elements ... ok
test bootstrap::tests::accepts_alpn_protocols_on_upstream_tls_context ... ok
test bootstrap::tests::accepts_alpn_element_of_exactly_255_bytes ... ok
test bootstrap::tests::accepts_alpn_protocols_in_common_tls_context ... ok
test bootstrap::tests::rejects_alpn_element_longer_than_255_bytes_on_listener ... ok
test bootstrap::tests::rejects_alpn_element_longer_than_255_bytes_on_cluster ... ok
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 708 filtered out
```

**NON-VACUITY PROOF (Step 2, second half).** In a scratch `git worktree` at
`2949ccf`, with the target asserted to occur **exactly once**, a forced rebuild
confirmed by a `Compiling` line, and gating on the `test result` line's
existence rather than the exit code:

```
=== CONTROL (unmutated, same tree) ===
   Compiling envoy-config v0.0.0 (/tmp/wt-112-1-t3/crates/envoy-config)
test result: ok. 8 passed; 0 failed

=== assert target occurs EXACTLY ONCE ===
occurrences: 1        # "    #[serde(default)]\n    pub alpn_protocols: Vec<String>,"

=== MUTATED (delete `#[serde(default)]`) ===
   Compiling envoy-config v0.0.0 (/tmp/wt-112-1-t3/crates/envoy-config)
test bootstrap::tests::alpn_protocols_defaults_to_empty_when_absent ... FAILED
   (the other seven: ok)
test result: FAILED. 7 passed; 1 failed
```

Exactly one test reddens, and it is the one that asserts the deleted behaviour.
Worktree removed; `git status --porcelain` empty afterwards; the four
`agent-*` worktrees untouched.

Commit `2949ccf`, numstat `62 0`. **`bootstrap.rs` cumulative: `228 9` = net
219, exactly the plan's row 1 including the raw additions/deletions pair.**

---

## Task 4 — thread the list into BOTH `rustls::ServerConfig` sites (D2a, D3, D5)

**RED (Step 2)** — precisely the plan's predicted shape:

```
test tests::alpn_negotiates_h2_when_both_offer_it ... FAILED
  left: None
 right: Some([104, 50])                                    # b"h2"
test tests::alpn_selection_follows_server_preference ... FAILED
  left: None
 right: Some([104, 116, 116, 112, 47, 49, 46, 49])         # b"http/1.1"
test tests::alpn_empty_server_list_does_not_advertise ... ok
test result: FAILED. 1 passed; 2 failed
```

The server never advertised. `alpn_empty_server_list_does_not_advertise` passes
already — it is the D6′.1 characterization pin (D-PLAN-3) and its RED is
mutation M2.

**GREEN (Step 5).** `test result: ok. 19 passed; 0 failed` (16 pre-existing + 3
new). **None of the 16 pre-existing tests regressed**, so `finish_server_config`
is inert for a config that declares no ALPN.

**DEVIATION — the `alpn` struct field had to move to Task 5.** See defect B.

Commit `39931ed`, numstat `42 6` / `88 0`.

---

## Task 5 — the D6′ accept-path rewrite. **The one that matters.**

**RED (Step 2) — this is the divergence the whole sub-phase exists to remove:**

```
thread 'tests::alpn_mismatch_completes_handshake_with_no_protocol' panicked at
crates/envoy-tls/src/tests.rs:1162:6:
client handshake must SUCCEED: Custom { kind: InvalidData,
                                        error: AlertReceived(NoApplicationProtocol) }
test result: FAILED. 4 passed; 1 failed
```

The pinned `rustls 0.23.39` sends a fatal `no_application_protocol` alert where
upstream Envoy v1.33.0 completes the handshake with nothing selected
(`112.1/SPEC.md` §2.2 cell 3, 45/45 runs). `alpn_client_offers_nothing_negotiates_none`
passes already, exactly as M-R4 predicts: rustls skips the whole selection block
when the client sends no ALPN extension.

> The plan predicted the error text as `Custom { kind: InvalidData, error:
> NoApplicationProtocol }`. The observed text is `AlertReceived(NoApplicationProtocol)`
> because the failure is observed from the **client** side — the client received
> the server's alert. Same event, same cause; only the spelling differs.

**GREEN (Step 5).**

```
test result: ok. 21 passed; 0 failed        # 16 pre-existing + 5 ALPN
test tests::accept_returns_handshake_error_on_garbage_input ... ok
```

`accept_returns_handshake_error_on_garbage_input` is green — the pin
`112.1/SPEC.md` §8 names on the malformed-ClientHello path. Under D6′.1 it takes
the unchanged `TlsAcceptor` route.

Workspace clippy at this point: **zero findings, 13 `Checking` lines** (a
genuine run, not a cached no-op). `cargo fmt --all -- --check`: zero diff.

Commit `82332c1`, numstat `85 13` / `18 0`.

---

## Task 6 — offer the list on the UPSTREAM side (D2b, D7)

**RED (Step 2).**

```
thread 'tests::upstream_offers_configured_alpn_to_the_server' panicked at
crates/envoy-tls/src/tests.rs:1255:5:
assertion `left == right` failed: server must have seen h2 in the client offer
  left: None
test result: FAILED. 0 passed; 1 failed
```

**The RED is real but its MECHANISM is not the one the plan predicted, and the
plan's own M-R4 says why.** Step 2 expected the rustls *server* to send
`NoApplicationProtocol` because its list is non-empty and the client offers
nothing. It cannot: M-R4 established that the trigger is
`hello.protocols.is_some()`, so a client that sends **no ALPN extension** skips
rustls' selection block entirely — no alert is possible. The handshake succeeds
and the server simply selects nothing, which the assertion catches. See defect D.

**GREEN (Step 4).** `test result: ok. 22 passed; 0 failed`.

The test is a genuine wire-level witness: the test server lists **only** `h2`
while `UpstreamTls` is configured `["http/1.1", "h2"]`, so `h2` can only be
selected if **both** names actually went out — not just the first.

**Full sweep (Step 5).**

```
cargo build --workspace --all-targets                                  -> Finished
cargo clippy --workspace --all-targets --all-features -- -D warnings   -> 0 findings, 3 Checking lines
cargo fmt --all -- --check                                             -> zero diff
envoy-config  test result: ok. 716 passed; 0 failed
envoy-tcp     test result: ok.  16 passed; 0 failed
envoy-tls     test result: ok.  22 passed; 0 failed
```

**716 / 16 / 22 — byte-for-byte the prototype's figures (M-R13).**

Commit `e127f9a`, numstat `9 1` / `47 0`. **`envoy-tls/src/lib.rs` cumulative:
`122 6` = net 116, exactly the plan's row 3 including the raw pair.**

---

## Task 7 — the `parse_bootstrap` fuzz corpus seed

**Gitignore discipline, both directions measured with the PLAIN
`git check-ignore` form** (the `-v` form reports negation rules and does not
answer the question):

```
before the `!` line:  exit 0   (IGNORED)
after  the `!` line:  exit 1   (NOT ignored)
git ls-files …/tls_downstream_alpn.yaml  ->  crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_downstream_alpn.yaml
seed census: 66 -> 67
```

Seed length **43 lines**, exactly the plan's figure.

**Step 4 — the seed genuinely parses AND reaches the new field.** `--mode
validate` does not exist in `envoy-bin` (see defect E), so the plan's documented
fallback was used: a throwaway test read the seed off disk, ran
`crate::parse_bootstrap` over it and asserted the parsed value.

```
test bootstrap::tests::zz_throwaway_fuzz_seed_parses ... ok
```

The throwaway was **deleted before committing**; `git diff --numstat --
crates/envoy-config/src/bootstrap.rs` was empty against `HEAD` afterwards,
proving the file went back byte-identical.

**Step 5 — short-budget fuzz run over the seeded corpus.**

```
$ cd crates/envoy-config/fuzz && cargo fuzz run parse_bootstrap corpus/parse_bootstrap -- -max_total_time=60
#117961  DONE   cov: 17722 ft: 37427 corp: 3561/2439Kb lim: 3807 exec/s: 1191 rss: 403Mb
Done 117961 runs in 99 second(s)
EXIT=0
```

**No crash; `find crates/envoy-config/fuzz/artifacts -type f | wc -l` = 0.** The
run wrote new inputs into the corpus directory, but those are caught by the
`corpus/parse_bootstrap/*` ignore rule, so `git status --porcelain` still showed
only the two intended paths.

Commit `d94922b`, numstat `1 0` / `43 0` = **44, exactly the plan's one
projected row.**

---

## Task 8 — the mutation proof

A fixture-free sub-phase carries its entire non-vacuity obligation on its unit
tests (`112.1/SPEC.md` §9). Run in a scratch `git worktree` at
`d94922be5c424f79e08bce4b136b6892736be0f4` — asserted equal to `main`'s HEAD
before starting, because a worktree branches from the session's START commit.

**All five method rules were applied to every mutation:** the target was
asserted to occur **exactly once** before mutating; a rebuild was forced and
confirmed by a `Compiling` line; the verdict gates on the **`test result` line's
existence**, not the exit code; an **unmutated control** was taken from the same
tree before and after; and everything ran in a scratch worktree.

```
=== TARGET OCCURRENCE ASSERTIONS (each MUST be 1) ===
M1 alpn_free.clone()                       : 1
M2 config.alpn_protocols = wire.clone();   : 1
M3 config.alpn_protocols = cfg             : 1
M4 if proto.len() > 255 {                  : 1
```

| # | mutation | result | RED set |
|---|---|---|---|
| — | **control, unmutated** | `envoy-tls ok. 6 passed`; `envoy-config ok. 8 passed` | — |
| **M1** | delete the D6′ config swap (`alpn_free.clone()` → `self.config.clone()`) | `FAILED. 5 passed; 1 failed` | **only** `alpn_mismatch_completes_handshake_with_no_protocol` |
| **M2** | delete the `ServerConfig` ALPN threading (`config.alpn_protocols = wire.clone();` → `let _ = &wire;`) | `FAILED. 4 passed; 2 failed` | **only** `alpn_negotiates_h2_when_both_offer_it` + `alpn_selection_follows_server_preference` |
| **M3** | delete the `ClientConfig` ALPN threading | `FAILED. 5 passed; 1 failed` | **only** `upstream_offers_configured_alpn_to_the_server` |
| **M4** | neuter the >255 validator (`> 255` → `> usize::MAX`) | `FAILED. 6 passed; 2 failed` | **only** `rejects_alpn_element_longer_than_255_bytes_on_listener` + `…_on_cluster` |
| — | **control re-taken after all four** | `envoy-tls ok. 6 passed`; `envoy-config ok. 8 passed` | — |

**Every mutation reddened exactly the tests that assert the deleted behaviour
and no others.** None is mis-aimed; none of the six `envoy-tls` ALPN tests and
none of the eight `envoy-config` ALPN tests is vacuous. This reproduces M-R13's
prototype result on the real tree — which was the point, since a mutation proof
is not transferable between trees.

Worktree removed. `git worktree list` afterwards showed only `main` plus the
four foreign `agent-*` worktrees, all untouched.

---

## Where the PLAN was wrong — five defects, and the one root cause behind two of them

`PLAN.md` is LANDED AND UNEDITABLE, so these are recorded here and in
**`ADR-0186`**. **None changes the sub-phase's scope, its deliverables, its
design, or the §6.1 verdict.** The plan's substantive engineering — D6′, the
borrowck shape, the dependency analysis, the sizing — was correct throughout.

**Defect A — `ds_alpn_of` is introduced in Task 2 but has no consumer until
Task 3.** Task 2 therefore fails its own stated gate:

```
error: function `ds_alpn_of` is never used
error: could not compile `envoy-config` (lib test) due to 1 previous error
```

*Remedy applied:* the helper moved to Task 3, where its first reader lives. The
other two builders (`ds_bootstrap_with_alpn`, `us_bootstrap_with_alpn`) ARE used
by Task 2's own tests and correctly stay there.

**Defect B — the `alpn` struct field is introduced in Task 4 but is not read
until Task 5's `accept()`.** Identical failure:

```
error: field `alpn` is never read
  --> crates/envoy-tls/src/lib.rs:73:5
```

The plan's Task 4 doc comment even asserts the field is "read only by the tests"
— it is not; nothing outside `accept()` ever touches it, and it is private.
*Remedy applied:* Task 4's `finish_server_config` returns a bare
`Arc<ServerConfig>`, and Task 5 widens it to the 3-tuple **and** adds both
fields. This is exactly the remedy the plan itself prescribes for
`alpn_free_config` ("Landing Task 5's `alpn_free_config` field here would leave
it unread, and `-D warnings` rejects a dead field. The churn is two lines and it
keeps every task green on its own") — the plan simply did not notice that the
same argument applies to `alpn`.

> **ROOT CAUSE of A and B, and the generalizable lesson: the prototype built
> Tasks 1–6 as ONE tree.** A whole-slice prototype can validate a plan's *code*
> — and this one did, superbly — but it is structurally incapable of validating
> the plan's *task boundaries*, because dead-code and unused-function lints only
> fire in the intermediate states a whole-slice build never occupies.
> `ADR-0185`'s method is confirmed and should be kept; this is the one thing it
> cannot see, and a plan that prototypes should either build task-by-task or
> explicitly audit each task boundary for first-reader/first-writer ordering.

**Defect C — Task 1 Step 1's assertion block is not `rustfmt`-clean.** The plan
states every block was copied from a tree that passed `cargo fmt --all --
--check` with zero diff. This one does not:

```
-        assert_eq!(ctx.common_tls_context.alpn_protocols, vec!["h2".to_string()]);
+        assert_eq!(
+            ctx.common_tls_context.alpn_protocols,
+            vec!["h2".to_string()]
+        );
```

The line is 82 characters, comfortably under `max_width = 100`, but rustfmt's
default `use_small_heuristics` sets `fn_call_width = 60` and the argument list
is 61. *Remedy applied:* `cargo fmt --all`.

**Defect D — Task 6 Step 2's predicted RED contradicts the plan's own M-R4.**
It predicts the rustls server sends `NoApplicationProtocol` when its list is
non-empty and the client offers nothing. M-R4 establishes the opposite: the
trigger is `hello.protocols.is_some()`, so that case skips the block entirely.
The real RED is an assertion failure (`left: None`). *No remedy needed* — the
test is correctly aimed and the RED is genuine; only the plan's prose is wrong.

**Defect E — `--mode validate` is an upstream-Envoy flag, not an `envoy-bin`
one.** Task 7 Step 4 offers it as the primary check, hedged with "if that mode
is unavailable *for this shape*". It is not unavailable for the shape; the
argument does not exist:

```
$ cargo run -q -p envoy-bin -- --mode validate -c …/tls_downstream_alpn.yaml
envoy-bin: unknown argument: --mode
EXIT=2
```

*Remedy applied:* the plan's own documented fallback (the throwaway test).

**One further arithmetic slip, in the `STATE.md` handoff rather than the plan.**
It predicts the CI identity moves to "~2264 from 8 new `envoy-config` tests and
6 new `envoy-tls` tests". 2252 + 8 + 6 = **2266**, not 2264.

---

## Size — measured against the plan's estimate

`git diff --numstat 3a2cf93 HEAD -- . ':(exclude)docs/**'`:

| file | PLAN §3 | ACTUAL | Δ |
|---|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | 219 (`228 9`) | **219** (`228 9`) | **0** |
| `crates/envoy-config/src/lib.rs` | 13 | **13** | **0** |
| `crates/envoy-tls/src/lib.rs` | 116 (`122 6`) | **116** (`122 6`) | **0** |
| `crates/envoy-tls/src/tests.rs` | 158 | **156** | −2 |
| `crates/envoy-tcp/src/lib.rs` | 1 | **1** | **0** |
| `crates/envoy-config/fuzz/` | 44 (projected) | **44** | **0** |
| **total** | **551** | **549** | **−2** |

**Five of six rows landed exactly, two of them matching even their raw
`additions deletions` pairs, and the single projected row landed exactly.** The
one deviation is 2 lines of formatting in the test file. Against the calibration
band (§3: 733 / 810 / 915 at 1.33× / 1.47× / 1.66×), the actual **549 is below
even the raw estimate** — the calibration factors exist to cover the gap between
a *projected* estimate and reality, and there was almost no gap to cover here
because 92% of the estimate was a line count of code that already existed.

**§6.1 was not re-adjudicated and did not need to be** (it belongs to state 2;
`ADR-0185` recorded a NOT-FIRE). **The §6.1 mid-execution trigger never fired:**
no task's sub-steps blew past ~10 items.

---

## Local verification standing at the end of state 3

**This is NOT the §7.5 gate — that is state 4**, where CI, the differential
suite, `cargo deny` and the conformance suites first really execute. What was
run here is the plan's per-task gate:

```
cargo build --workspace --all-targets                                 -> Finished
cargo clippy --workspace --all-targets --all-features -- -D warnings  -> 0 findings (13 Checking lines)
cargo fmt --all -- --check                                            -> zero diff
cargo test --workspace --lib --no-fail-fast                           -> 16 binaries, 1991 passed, 0 failed, EXIT=0
```

⚠ **The differential suite was NOT run locally and its result is not predicted
here.** `112.1/SPEC.md` §4 makes §7.5 gate **(b)** — all 90 pre-existing
fixtures still green — the load-bearing gate for this sub-phase, and
specifically `0004-tls-downstream`, `0005-tls-upstream` and `0006-tls-sni`,
which exercise the rewritten accept path. The in-process leading indicator is
strong (all 16 pre-existing `envoy-tls` tests and all 708 pre-existing
`envoy-config` tests green, and D6′.1 routes every ALPN-free config down the
byte-for-byte unchanged `TlsAcceptor` path) but it is an indicator, not the
gate. Differential fixtures also flake under full-parallel `cargo test` on this
host; **only isolation classifies a RED, never the failure text**, and CI is
authoritative.

**The CI identity must MOVE on this commit** — it is a code commit. From
`binaries=167 passed=2252 failed=0`, expect **passed=2266** (+8 `envoy-config`,
+6 `envoy-tls`) with `binaries` unchanged at **167**, since no new test binary is
added. A code commit that does NOT move the identity is the alarm.

---

## Carry-forwards — banked, not consumed (§6.3; `ADR-0165`)

**Nothing was fixed.** Phase 111's M-1…M-15 / N-1…N-13 and CF-111-1…CF-111-9
(CF-111-1 explicitly NOT this phase's to consume), the `110.2` / `110.1` /
`109.2` / `109.1` / `108.2` REVIEW sets, CF-110-1…9, CF-109-1/2/3, CF-108-1/2/3,
CF-76-1, CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6, CF-74-1/2/3/4/6, CF-73-1 and
the HTTP-filters-family (1)-(4) all carry forward INTACT.

**CF-112-1/2/3/4/6/7 carry forward; CF-112-5 stays CLOSED.** This session opened
**no new carry-forward** — the five PLAN defects are documentation corrections
recorded in `ADR-0186`, not deferred work.

---

## Next state

**§5 state 4 — the verification gate — is a SEPARATE session** (§5.1;
`ADR-0127`: an implementation and its verification are never the same context).
It runs `superpowers:verification-before-completion` over the full §7.5 six-gate
definition and quotes every command's output into this file.

---

# §5 state-4 — THE §7.5 VERIFICATION GATE

> **A separate session from the implementation**, per §5.1 and `ADR-0127`: the
> context that wrote an artifact must not grade it. This section records what
> the gate actually printed. Where a landed artifact predicted a different
> number, the difference is stated and adjudicated — see `ADR-0187`.
>
> **Session start:** `git status --porcelain` empty; branch `main`; HEAD
> `c86afd543befabd23f05739f791b100df0d7d48e` (the state-3 implementation);
> `ls stop` → `No such file or directory`. The four `.claude/worktrees/agent-*`
> worktrees belong to a PARALLEL WORKSTREAM and were left untouched throughout.
>
> ⚠ **The `next-prompt.txt` handoff this session was launched with was STALE.**
> It instructed the session to perform the state-3 implementation and asserted
> HEAD was `3a2cf93`. On disk, HEAD was already `c86afd5`, `STATE.md` read
> **state-3-COMPLETE**, and its `## Next expected skill` named the state-4
> gate. **Disk was treated as authoritative** — a handoff's claims are claims.
> The state-3 CI record the previous session left outstanding was landed first
> as `1dca192` (numstat exactly `2 0`) before any state-4 work began.

---

## Gate (e) — the five workspace commands, run locally from a clean tree

```
$ cargo build --workspace --all-targets
   Compiling envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
   Compiling http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
   Compiling envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.49s
BUILD_EXIT=0
```
`Compiling` lines = **5**, `^warning` = **0**, `^error` = **0**. A non-zero
`Compiling` count matters: exit 0 with zero of them would be a cached no-op.

```
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Checking envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.97s
CLIPPY_EXIT=0
```
**Zero findings over 13 `Checking` lines** — clippy prints `Checking`, not
`Compiling`, and exit 0 with zero of them would likewise be a cached no-op.

```
$ cargo fmt --all -- --check
FMT_EXIT=0
```
The whole log is **11 bytes** — the exit marker and nothing else, i.e. a zero
diff.

```
$ cargo deny check
advisories ok, bans ok, licenses ok, sources ok
DENY_EXIT=0
```
Gated on the four-ok line, ANSI-stripped first because that line is
colour-coded and a naive grep false-zeroes it. The accompanying
`unmatched license allowance` notes (e.g. `"Zlib"`) are the normal
`license-not-encountered` warnings on a green run, not findings.

```
$ cargo test --workspace --no-fail-fast          # sweep 1
binaries=167 passed=2259 failed=6   identity(passed+failed)=2265
$ cargo test --workspace --no-fail-fast          # sweep 2
binaries=167 passed=2258 failed=7   identity(passed+failed)=2265
```

**The identity is 2265 in both sweeps and equals CI's `passed` exactly.**
Only `passed + failed` is invariant; `passed` alone is not, because the local
RED set varies run to run. Counts come from matching
`test result: (ok|FAILED)` — never bare `ok`, which makes `failed=0` true by
construction — with awk fields **derived from a real matched line** ($4, $6).

---

## Gate (e), continued — the local RED set, classified by ISOLATION ONLY

Failing names were extracted from the `---- <name> stdout ----` markers, never
by indentation, with output redirected to a file rather than piped through
`tail` (which truncates the `failures:` block).

| sweep | RED set |
|---|---|
| 1 (6) | the five core + `access_log_h2_rf_overflow` |
| 2 (7) | the five core + `access_log_status_code_filter` + `network_filter_rbac_allow_fixture` |

Every member was then re-run **ALONE with a 30-second settle gap before it**,
because back-to-back Docker-spawning runs manufacture a false
`FAILS-IN-ISOLATION`.

```
access_log_h2_rcd_upstream_reset    FAILED. 0 passed; 1 failed   (2.73s)   -> CORE
access_log_h2_uc_upstream_reset     FAILED. 0 passed; 1 failed   (2.70s)   -> CORE
access_log_rcd_upstream_reset       FAILED. 0 passed; 1 failed   (2.75s)   -> CORE
access_log_rf_upstream_reset        FAILED. 0 passed; 1 failed   (2.77s)   -> CORE
admin_config_dump_server_info       FAILED. 0 passed; 1 failed   (2.76s)   -> CORE
access_log_h2_rf_overflow           ok. 1 passed; 0 failed       (1.25s)   -> parallel-load flake
access_log_status_code_filter       ok. 1 passed; 0 failed      (13.26s)   -> parallel-load flake
network_filter_rbac_allow           ok. 1 passed; 0 failed       (2.40s)   -> parallel-load flake
```

**The stable core is FIVE — unchanged from the record — and it is the exact
intersection of the two sweeps.** Its determinism in isolation IS this host's
signature, not a regression; all five are green in CI. The three varying
members each pass alone, so none is a new core member. **No test was
weakened and nothing was fixed.**

⚠ **A recorded trap fired and is worth re-recording.** The first isolation
attempt at `network_filter_rbac_allow_fixture` returned **exit 101 with NO
`test result` line at all** — cargo rejected the target and listed the
available ones. That name is the **test FUNCTION**; the **runner FILE** is
`network_filter_rbac_allow.rs`. Gating on the existence of the `test result`
line rather than on the exit code is what caught it; gating on the exit code
would have recorded a false `FAILS-IN-ISOLATION`.

---

## Gate (a) — new/changed differential fixtures: NONE, vacuous BY DESIGN

`112.1` ships **no new differential fixture**. `112.1/SPEC.md` §4 assigns the
witness to sibling `112.2` (`0091-tls-alpn`, `0092-tls-alpn-server-preference`,
and the cell-6 control on `0004-tls-downstream`), mirroring `ADR-0178`'s
`110.1` and `ADR-0176`'s `109.1`. `tests/` was not touched this sub-phase.
**Gate (a) passes vacuously and is recorded as vacuous rather than as a pass.**

## Gate (b) — all pre-existing differential fixtures still green. **THE LOAD-BEARING GATE**

`112.1/SPEC.md` §4 makes this the gate that carries the weight, because the
sub-phase rewrites `DownstreamTls::accept`, the accept path shared by every TLS
listener in the tree.

```
differential runner files on disk : 90
fixture dirs on disk              : 90
runners MISSING from the CI log   : 0
CI whole-log `test result: FAILED`: 0
```

**90 of 90, zero missing.** The census is driven from the **runner FILE names**
(`ls tests/differential/tests/*.rs`), not the test-function names, which differ
for many fixtures. The three fixtures that exercise the rewritten accept path
are among them and each shows a `Running` line:

```
tls_downstream : 1     tls_upstream : 1     tls_sni : 1
```

This gate cannot pass vacuously: the three already passed before the rewrite,
so any change in the accept path is directly observable in them.

## Gate (c) — conformance suites

```
CI:    Running tests/h2spec_runner.rs
       test h2spec_pass_rate_gate ... ok
       test result: ok. 3 passed; 0 failed; ... finished in 0.16s
CI control: `h2spec not found` = 0  -> the suite GENUINELY RAN (ADR-0163)
```
`known-failures.txt` unchanged: **21** lines, md5
`19cd44d86a8b15d825f76c6e7b265e65`. It was not trimmed.

**The local run is DEMONSTRABLY VACUOUS on this host and was re-measured to
prove it**, so that the CI evidence above is not mistaken for local evidence:

```
$ cargo test -p h2spec-conformance --test h2spec_runner -- --nocapture
h2spec_runner: h2spec not found — skipping locally
test result: ok. 3 passed; 0 failed; ... finished in 0.00s
H2SPEC_LOCAL_EXIT=0
```
Exit 0, three "passes", and the skip line is invisible without `--nocapture`.
The `0.00s` runtime on a conformance suite is the tell. **CI is authoritative.**

## Gate (d) — fuzzing

**`112.1` adds NO new fuzz target** — only a corpus seed for the existing
`parse_bootstrap` target — so no new `ci.yml` step was owed, and the existing
step covers it. The seed is genuinely in the tree and genuinely not ignored:

```
git ls-files …/corpus/parse_bootstrap/tls_downstream_alpn.yaml -> tracked
git check-ignore (PLAIN form) -> EXIT=1  (NOT ignored)
seed length: 43 lines        tracked seed census: 67
```

The CI fuzz job (`success`, 13 steps, real runner) ran **5** targets:

```
Done   180288 runs in 31 second
Done  4575148 runs in 31 second
Done  5293078 runs in 31 second
Done  2720132 runs in 31 second
Done 19514105 runs in 31 second
ERROR: libFuzzer      = 0
Test unit written to  = 0      (no crash artifacts)
```

## Gate (f) — REVIEW.md

**Not this session's.** Gate (f) is closed by the §5 state-5 code review.

---

## CI on the state-3 implementation — CONFIRMED, and the identity corrected

```
run 33522915551   conclusion success   attempt 1   total_count: 1
  build + test + lint : 15 steps, runner GitHub Actions 1000005661, success
  fuzz                : 13 steps, runner GitHub Actions 1000005660, success
identity: binaries=167 passed=2265 failed=0
```
Not the `runner_name:""` + `steps:0` starvation shape. Counted from the
ANSI-stripped job log (raw **418800** bytes, so not the ~120-byte out-of-repo
`gh` error trap). Controls: `test result: FAILED` = **0**; `Running ` = **151**
after stripping; `h2spec not found` = **0**; `cargo deny`'s four-ok line
present.

**The identity MOVED, as a code commit requires — but by +13, to 2265, not the
+14 to 2266 that `PLAN.md` M-R13, `ADR-0185` and `ADR-0186` all predicted.
The prediction was wrong; the run was not.** `ADR-0187` records it.

| | at `3a2cf93` | at `c86afd5` | Δ |
|---|---|---|---|
| `envoy-config` lib | **709** | **716** | **+7** |
| `envoy-tls` lib | **16** | **22** | **+6** |
| workspace identity | 2252 | **2265** | **+13** |

**Two independent methods agree**, which is why the run is trusted over the
prediction:

1. the per-binary passed-count **multiset** of this CI log diffed against the
   baseline CI log for `28e7f4e` (run 33388511508, re-derived here at
   `binaries=167 passed=2252 failed=0`) — exactly two binaries move,
   `709 → 716` and `16 → 22`, both with `0 ignored`;
2. a **source census** of test attributes at each commit — `envoy-config`
   709 → 716, `envoy-tls` 16 → 22. (The `envoy-tls` census needs the pattern
   `#\[(tokio::)?test` without a closing bracket: 21 of its 22 tests are
   spelled `#[tokio::test(flavor = "multi_thread")]`, and a pattern anchored on
   `]` reads **1**.)

**The root cause is a DOUBLE COUNT, not a lost test.** Task 1 RENAMED
`rejects_unknown_field_in_common_tls_context` to
`accepts_alpn_protocols_in_common_tls_context` — present at `3a2cf93`, absent
at `c86afd5` — and a rename adds zero. Both halves of "708 + 8" are individually
real: `8` is the count of tests bearing ALPN names (Task 3's filtered run reads
`8 passed … 708 filtered out`, which sums to 716), and `708` is the count the
filter did NOT match. **The error is that they were ADDED, which counts the
renamed test on both sides.** The correct decomposition is **709 + 7**.
`binaries` correctly stayed at **167**: no new test binary was added.

---

## Stop condition — re-derived from disk this session. ALL THREE LEGS FALSE

No `stop` file was created; `ls stop` → `No such file or directory`.

- **Leg (i) FALSE** — **120** rows / **117** `done` / **1** `in-progress` /
  **2** `planned`; buckets sum to the row count (117+1+2 = 120). Status is
  field **4** on a `' | '` split driven from the `^\| [0-9]` prefix. Control:
  the forbidden `NF == 6` form reads **118**, dropping exactly the two rows
  (38, 39) that carry unescaped in-cell pipes — a believable near-miss, which
  is why it is not used. Those rows were NOT "fixed".
- **Leg (ii) FALSE** — **14** crates, none of `envoy-http3`/`envoy-grpc`/
  `envoy-wasm`/`envoy-protos`/`envoy-runtime`; `quinn`/`wasmtime`/`tonic`/
  `opentelemetry`/`prost` = **0** across all **15** manifests, against a
  **positive control run with the identical invocation**: `tokio` = **12** of
  15, so the zeros are real. `tests/conformance/` holds only `h2spec/`.
- **Leg (iii) FALSE** — **11** `### ` family headings, of which one
  (`### WASM host family`) carries **ZERO** rows. Driven from a single
  `/^### /` rule; the eleven read 10/5/3/14/3/4/6/29/6/0/13 with **27** rows
  before the first heading, summing to 120.

---

## Carry-forwards — banked, not consumed (§6.3; `ADR-0165`)

**Nothing was fixed, and a verification state is bound by §6.3 as much as any
other.** The entire banked set carries forward INTACT: phase 111's
M-1…M-15 / N-1…N-13 and CF-111-1…CF-111-9, the `110.2` / `110.1` / `109.2` /
`109.1` / `108.2` REVIEW sets, CF-110-1…9, CF-109-1/2/3, CF-108-1/2/3, CF-76-1,
CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6, CF-74-1/2/3/4/6, CF-73-1 and the
HTTP-filters-family (1)-(4). **CF-112-1/2/3/4/6/7 carry forward; CF-112-5 stays
CLOSED.** This session opened **no new carry-forward**: the identity correction
is a documentation figure, recorded in `ADR-0187`, not deferred work.

`ROADMAP.md` was **deliberately untouched** — row `112.1` flips at its state-6
close-out, not here.

---

## Verdict

**§7.5 gate: (a) vacuous by design, (b) PASS, (c) PASS, (d) PASS, (e) PASS.
(f) is state 5's.** `112.1` is code-complete and verified.

## Next state

**§5 state 5 — the code review — is a SEPARATE session** (§5.1; `ADR-0127`:
the context that graded an artifact must not review it). It runs
`superpowers:requesting-code-review` and outputs `REVIEW.md`, closing gate (f).
A state-5 review writes no code, so **the CI identity must NOT move from
`binaries=167 passed=2265 failed=0`.**
