# Sub-phase 75.2 — `HeaderMatcher` absence semantics: the ACCESS-LOG-path witnesses + the contract bank — Implementation Log

> **What this document is.** The §5 **state-3 implementation** log for sub-phase
> **75.2**, appended per task as each task completed. Written for a stranger with
> zero prior context (D-3.4). Every command output below was FRESHLY CAPTURED in
> this session — none is transcribed from an earlier tree (the sub-phase-75.1
> review's finding M-5 was exactly that defect). Outputs are pasted VERBATIM with
> exactly two mechanical exceptions, applied uniformly and flagged here rather than
> left for a reviewer to discover: (1) the `diff -u` headers' `---`/`+++` lines are
> dropped, because they carry file mtimes rather than content; (2) in `cargo test`
> output the `tracing` lines' ANSI colour escapes are stripped and their
> `2026-07-26T…Z` timestamps removed, leaving the message text unaltered. Nothing
> else is elided, reordered or retyped — in particular every `test result:` line,
> every `N passed` count and every assertion message is byte-for-byte as emitted.
>
> **Session start state (verified on disk, not trusted from the handoff):**
> `git status --porcelain` clean; branch `main`; `HEAD` ==
> `1bf256aa01766f9c6e47132867c2e9e1083031f5`; `git fetch origin --prune` showing
> `origin/main` at the SAME SHA. CI on that full 40-char SHA re-confirmed GREEN —
> run `30199193179`, both jobs `success` at FULL step counts **15**
> (`build + test + lint`) and **13** (`fuzz`), so no runner-starvation signature
> (`steps: 0` + `runner_name: ""`). **State detection: `PLAN.md` EXISTS and no
> `PROGRESS.md` — §5 state 3 exactly**, so `superpowers:executing-plans` was the
> routed skill.
>
> **Censuses re-verified on disk at session start:** ROADMAP **104** rows, **102**
> `done`, exactly **2** `in-progress` (`75` and `75.2`) — measured by splitting on
> `' | '` WITH the spaces, since a naive `awk -F'|' $5` false-reports rows
> `36`/`38`/`39`/`52`/`54`/`66`/`70` (they carry unescaped `|` and must NOT be
> "fixed"). **83** fixture directories, `0084`/`0085` NEITHER existing. **83**
> differential test files. `known-failures.txt` **21** lines.
> `BEHAVIOR_CONTRACT.md` **3363** lines. `DECISIONS.md` head **ADR-0161**.
>
> **Pre-flight anchor re-derivation (before any edit).** Every `BEHAVIOR_CONTRACT.md`
> site named in `PLAN.md` was re-derived BY TEXT ANCHOR and all of ADR-0161's
> corrected numbers HELD: `## xDS wire state machine` at `:2677`; the CF-72-2
> `**§D` record at `:2423-2427`; the phase-72 block's letters running `§A…§F` so
> the next free is `§G`; the M74-31 contract site at `:2657`; and all six Task-8
> sites present. This mattered: **two of those sites had drifted again by the time
> their own task ran**, moved by this session's Tasks 5/6 (M74-31's contract site
> `2657` → `2709`; M-3's `2545` → `2597`), so each task re-derived immediately
> before editing rather than trusting `PLAN.md`'s numbers.

---

## Task 1 — Fixture `0084-headermatcher-absence-accesslog` (the D1 witness)

**Files created (4):**

| file | `wc -l` |
|---|---|
| `tests/fixtures/0084-headermatcher-absence-accesslog/envoy.yaml` | 60 |
| `tests/fixtures/0084-headermatcher-absence-accesslog/envoy-rust.yaml` | 58 |
| `tests/fixtures/0084-headermatcher-absence-accesslog/expectations.yaml` | 62 |
| `tests/fixtures/0084-headermatcher-absence-accesslog/README.md` | 139 |

Stencilled on `0078-accesslog-header-filter` (the access-log recipe MEASURED 5/5
across `0078`–`0082`), **not** on `0083` — `0083` is the most recent fixture but is
a `http1_probe_list` one with a different per-side recipe.

**Step 3 — the two configs differ ONLY by the four recipe deltas.** VERBATIM:

```
$ diff -u tests/fixtures/0084-headermatcher-absence-accesslog/envoy.yaml \
          tests/fixtures/0084-headermatcher-absence-accesslog/envoy-rust.yaml
@@ -1,9 +1,8 @@
 node: { id: envoy-rust-phase-75-fixture-0084, cluster: envoy-rust-phase-75 }
-admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
 static_resources:
   listeners:
     - name: http1_listener
-      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
+      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
       filter_chains:
         - filters:
             - name: envoy.filters.network.http_connection_manager
@@ -11,12 +10,11 @@
                 "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                 stat_prefix: ingress_http
                 codec_type: HTTP1
-                generate_request_id: false
                 access_log:
                   - name: envoy.access_loggers.file
                     typed_config:
                       "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
-                      path: /tmp/0084-envoy-mount/access.log
+                      path: /tmp/0084-envoy-rust-mount/access.log
                       log_format:
                         text_format_source:
                           inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n"
```

Exactly four changes, no fifth hunk. `codec_type: HTTP1` is on BOTH sides and is
NOT a divergence (ADR-0158 C3).

**Steps 6–7 — build the DEBUG binary the harness actually runs, then run.** VERBATIM:

```
$ cargo build -p envoy-bin
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.08s

$ cargo test -p differential --test headermatcher_absence_accesslog -- --nocapture
   Compiling differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.49s
     Running tests/headermatcher_absence_accesslog.rs (target/debug/deps/headermatcher_absence_accesslog-b4eb4ae02b28971f)

running 1 test
INFO node registered node.id=envoy-rust-phase-75-fixture-0084 node.cluster=envoy-rust-phase-75
INFO listener bound with SO_REUSEPORT (one accept queue per worker) addr=127.0.0.1:46729 sockets=32
INFO envoy-rust listening (http_connection_manager) addr=127.0.0.1:46729 stat_prefix=ingress_http codec_type=HTTP1
test headermatcher_absence_accesslog ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.22s
```

Asserted on the **`1 passed`** figure, not the exit code. The ~3 s runtime on a
backend-free fixture with a warm Envoy image is NORMAL, not a silent skip — and
the interleaved envoy-rust boot lines prove the subject proxy really ran.

**Step 8 — the mutation check (the TDD RED). See the dedicated section below**;
both fixtures' mutations were run together in one worktree after Tasks 1–4 had
landed, so the worktree could obtain them from `main` rather than by hand-copying.

**Commit:** `cf233ad` — *phase 75.2: fixture 0084 — the D1 access-log witness
(value matcher + invert + absent DROPS)*.

---

## Task 2 — The `0084` test entrypoint

**File created:** `tests/differential/tests/headermatcher_absence_accesslog.rs` — 64 lines.

Registration cost is ONE file: `tests/differential/Cargo.toml` has no `[[test]]`
stanza, so cargo autodiscovers `tests/*.rs`. No manifest edit, no registry entry,
no `ci.yml` change.

**Step 3 — fmt + clippy.** Both new entrypoints were `touch`ed first so the run
could not be a partially-cached false green. VERBATIM:

```
$ cargo fmt --all -- --check
FMT CLEAN

$ cargo clippy -p differential --all-targets --all-features -- -D warnings
    Checking differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.78s
```

The `Checking differential` line is the evidence the run was not cached
(`cargo clippy` prints `Checking`, NOT `Compiling`).

**Commit:** `f0fabea` — *phase 75.2: test entrypoint for fixture 0084*.

---

## Task 3 — Fixture `0085-headermatcher-absence-accesslog-present-polarity` (the D2 witness)

**Files created (4):**

| file | `wc -l` |
|---|---|
| `…/0085-headermatcher-absence-accesslog-present-polarity/envoy.yaml` | 59 |
| `…/0085-headermatcher-absence-accesslog-present-polarity/envoy-rust.yaml` | 57 |
| `…/0085-headermatcher-absence-accesslog-present-polarity/expectations.yaml` | 45 |
| `…/0085-headermatcher-absence-accesslog-present-polarity/README.md` | 167 |

**Step 3 — four recipe deltas only.** VERBATIM:

```
$ diff -u tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/envoy.yaml \
          tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/envoy-rust.yaml
@@ -1,9 +1,8 @@
 node: { id: envoy-rust-phase-75-fixture-0085, cluster: envoy-rust-phase-75 }
-admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
 static_resources:
   listeners:
     - name: http1_listener
-      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
+      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
       filter_chains:
         - filters:
             - name: envoy.filters.network.http_connection_manager
@@ -11,12 +10,11 @@
                 "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                 stat_prefix: ingress_http
                 codec_type: HTTP1
-                generate_request_id: false
                 access_log:
                   - name: envoy.access_loggers.file
                     typed_config:
                       "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
-                      path: /tmp/0085-envoy-mount/access.log
+                      path: /tmp/0085-envoy-rust-mount/access.log
                       log_format:
                         text_format_source:
                           inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n"
```

**Step 6 — run.** VERBATIM:

```
$ cargo test -p differential --test headermatcher_absence_accesslog_present_polarity -- --nocapture
   Compiling differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.49s
     Running tests/headermatcher_absence_accesslog_present_polarity.rs (target/debug/deps/headermatcher_absence_accesslog_present_polarity-3fa1b92e04b9a4f4)

running 1 test
INFO node registered node.id=envoy-rust-phase-75-fixture-0085 node.cluster=envoy-rust-phase-75
INFO listener bound with SO_REUSEPORT (one accept queue per worker) addr=127.0.0.1:44237 sockets=32
INFO envoy-rust listening (http_connection_manager) addr=127.0.0.1:44237 stat_prefix=ingress_http codec_type=HTTP1
test headermatcher_absence_accesslog_present_polarity ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.13s
```

**Commit:** `6fe4bf7` — *phase 75.2: fixture 0085 — the D2 access-log witness
(present_match: false means header-must-be-ABSENT)*.

---

## Task 4 — The `0085` test entrypoint

**File created:** `tests/differential/tests/headermatcher_absence_accesslog_present_polarity.rs` — 62 lines.

fmt + clippy clean (the same run quoted under Task 2 covered both files, both
`touch`ed).

**Commit:** `3b44510` — *phase 75.2: test entrypoint for fixture 0085*.

---

## The mutation checks — the TDD RED for both fixtures (Task 1 Step 8 + Task 3 Step 7)

Both fixtures pin ALREADY-CORRECT code (sub-phase 75.1 landed the engine fix), so
they pass on the first run. TDD's RED is therefore honored by a mutation check,
per the standing house pattern for characterization pins.

**This section records a REAL correction to `PLAN.md`. It is the substance of
ADR-0162.**

### Setup — a scratch worktree, reset to `main`

```
$ git worktree add /tmp/claude-1000/mut-75-2 HEAD
Preparing worktree (detached HEAD 3b44510)
HEAD is now at 3b44510 phase 75.2: test entrypoint for fixture 0085
$ cd /tmp/claude-1000/mut-75-2 && git reset --hard main
HEAD is now at 3b44510 phase 75.2: test entrypoint for fixture 0085
$ git rev-parse HEAD
3b445100149234ff4b2cd63416c06f2483166591
```

**DEVIATION (recorded, ADR-0162).** `PLAN.md` names TWO worktrees (`mut-0084`,
`mut-0085`). ONE was used, with the two mutations applied SEQUENTIALLY. The
guarantee the plan states is isolation from the MAIN tree — so that a parallel
agent's `git checkout` cannot silently revert an in-place mutation and hand back a
false green — and one worktree satisfies that fully; two would have paid a second
full cold workspace build for no additional guarantee. The worktree was restored
to pristine between mutations and a post-mutation control was re-run before it was
removed (below). Because Tasks 1–4 had already been committed, the worktree
obtained both fixtures from `main` rather than by hand-copying.

### The UNMUTATED control — both fixtures GREEN

```
##### 0084 (unmutated) #####
test headermatcher_absence_accesslog ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.15s
##### 0085 (unmutated) #####
test headermatcher_absence_accesslog_present_polarity ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.14s
```

### Mutation A1 — the mutation `PLAN.md` Task 1 Step 8 SPECIFIES. It does NOT witness `0084`.

Applied as specified: the `(_, None) => return false,` arm MOVED to the TOP of the
`match (&self.mode, value)`.

```
$ grep -n 'None) => return false' crates/envoy-config/src/matcher.rs
46:            (_, None) => return false,
$ cargo build -p envoy-bin 2>&1 | grep -c 'Compiling envoy-config'
1
```

First run went RED — but **READ THE FAILURE TEXT**:

```
    127.0.0.1:55382 not accept-ready within 10s: Connection refused (os error 111)
test headermatcher_absence_accesslog ... FAILED
test result: FAILED. 0 passed; 1 failed; ... finished in 11.27s
```

That is the documented `wait_accept_ready` port-reuse **startup-race host flake** —
it never reached an assertion, so it is **NOT mutation evidence**. Re-run from the
same worktree, build confirmed `Finished` / exit 0:

```
test headermatcher_absence_accesslog ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.25s
```

**The specified mutation leaves `0084` GREEN.** The reason is structural: `return
false` short-circuits the function BEFORE the closing `mode_result ^
self.invert_match`, so an absent header under a VALUE matcher yields `false` and
DROPS **whether the arm sits first or last**. Hoisting it cannot change D1.

What it DOES break is **P1, the mode-scoping guard** — measured in the same worktree:

```
$ cargo test -p envoy-config --lib
test matcher::tests::invert_match_inverts_present_match_result ... FAILED
test matcher::tests::present_match_false_matches_when_absent ... FAILED
test matcher::tests::absence_semantics_matrix_matches_measured_upstream ... FAILED
test matcher::tests::pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream ... FAILED
test result: FAILED. 645 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

**FOUR** assertions, not the "three" `PLAN.md`/ADR-0159 state — and every
value-mode assertion plus BOTH access-log fixtures stay green. So the arm ORDER is
guarded **only in-process**: no differential fixture can catch a regression in it.
`BEHAVIOR_CONTRACT.md` Phase 75 §D now records the corrected count with the four
test names and that in-process-only fact.

### Mutation A2 — the TRUE pre-75.1 D1 revert. `0084` goes RED on the assertion.

`(_, None) => return false,` → `(_, None) => false,` — dropping the `return` so the
absent-header `false` REACHES the XOR, where `invert_match: true` resurrects it to
a KEEP. That is divergence D1 as ADR-0156 measured it.

```
$ grep -n '(_, None) =>' crates/envoy-config/src/matcher.rs
57:            (_, None) => false,
$ cargo build -p envoy-bin 2>&1 | grep -c 'Compiling envoy-config'
1

$ cargo test -p differential --test headermatcher_absence_accesslog -- --nocapture
running 1 test

thread 'headermatcher_absence_accesslog' (3319049) panicked at tests/differential/tests/headermatcher_absence_accesslog.rs:63:10:
fixture green: CF-71-1: an access log grew beyond 1 lines under a 2s settle (envoy_rust=2, envoy=1) — a suppressed record leaked
test headermatcher_absence_accesslog ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.02s
```

`envoy_rust=2, envoy=1` — the LINE-COUNT assertion, reached and failed. **Fixture
`0084` is load-bearing.**

### Mutation B — as `PLAN.md` Task 3 Step 7 specifies. CORRECT as written.

`PresentMatch` restored to its pre-75.1 body `if *want_present { v.is_some() } else { true }`.

```
$ grep -n 'if \*want_present' crates/envoy-config/src/matcher.rs
53:                if *want_present { v.is_some() } else { true }
$ cargo build -p envoy-bin 2>&1 | grep -c 'Compiling envoy-config'
1

$ cargo test -p differential --test headermatcher_absence_accesslog_present_polarity -- --nocapture
thread 'headermatcher_absence_accesslog_present_polarity' (3321309) panicked at tests/differential/tests/headermatcher_absence_accesslog_present_polarity.rs:61:10:
fixture green: CF-71-1: an access log grew beyond 1 lines under a 2s settle (envoy_rust=2, envoy=1) — a suppressed record leaked
test headermatcher_absence_accesslog_present_polarity ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.04s
```

**Fixture `0085` is load-bearing.**

**The two mutations are DISTINCT and hit DIFFERENT cells** — hoisting the arm
breaks P1 and leaves D1 correct; dropping the `return` breaks D1 and leaves P1
correct. A future session must pick the matching mutation or it will misread a
GREEN as a vacuous fixture. Recorded in `BEHAVIOR_CONTRACT.md` Phase 75 §D and in
ADR-0162.

### Post-mutation restored control, then cleanup

```
$ git checkout -- crates/envoy-config/src/matcher.rs
$ git status --porcelain          # (empty — worktree pristine)
$ cargo build -p envoy-bin 2>&1 | grep -c 'Compiling envoy-config'
1
=== restored control: 0084 ===   test result: ok. 1 passed; 0 failed; ... finished in 3.17s
=== restored control: 0085 ===   test result: ok. 1 passed; 0 failed; ... finished in 3.13s
=== restored control: envoy-config lib === test result: ok. 649 passed; 0 failed; ... finished in 0.01s
```

`649 passed` restored from `645 passed; 4 failed` — the mutation was genuinely
present and genuinely reverted. Then **only my own** worktree was removed:

```
$ git worktree remove --force /tmp/claude-1000/mut-75-2
$ git worktree list
/home/esa/git/envoy-rust                                            3b44510 [main]
/home/esa/git/envoy-rust/.claude/worktrees/agent-a0cda5e6afdd64be2  2d6ecda [...]
/home/esa/git/envoy-rust/.claude/worktrees/agent-a22debad535db1d78  7140aba [...]
/home/esa/git/envoy-rust/.claude/worktrees/agent-a54a85accb5dc112f  2b535b5 [...]
/home/esa/git/envoy-rust/.claude/worktrees/agent-ac17c8d4a0ab78914  9e8cfe7 [...]
```

The four `.claude/worktrees/agent-*` belong to a PARALLEL workstream and were left
untouched.

---

## Task 5 — `BEHAVIOR_CONTRACT.md`: the new `### Phase 75` block

**File modified:** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — **+140 / −0**.

**Step 1 — the insertion anchor, re-derived by text.** The phase-74 block's body
ends at `:2673`, its closing `---` at `:2675`, and `## xDS wire state machine` at
`:2677` — confirming ADR-0161's correction **C1**. (The `SPEC.md`'s `~2632` would
have landed the new block INSIDE the phase-74 block's `**§H`, silently corrupting
a landed record.)

**Step 3 — verification.** VERBATIM:

```
$ grep -n '^### Phase 7[0-9]' docs/envoy-rust/BEHAVIOR_CONTRACT.md
2147:### Phase 70 (ADR-0140/0141): `status_code_filter` — …
2263:### Phase 71 (ADR-0144/0145): `response_flag_filter` — …
2341:### Phase 72 (ADR-0148/0149/0150): header_filter — …
2445:### Phase 73 (ADR-0152/0153): `and_filter` / `or_filter` — …
2493:### Phase 74 (ADR-0154/0155): `metadata_filter` — …
2677:### Phase 75 (ADR-0156/0157/0158/0159/0161/0162): `HeaderMatcher` ABSENCE semantics — …

$ awk '/^### Phase 75 /{f=1} f&&/^## xDS wire state machine/{exit} f' … | grep -c '^---$'
1
$ grep -c '^### Phase 75 ' docs/envoy-rust/BEHAVIOR_CONTRACT.md
1
$ git diff --numstat docs/envoy-rust/BEHAVIOR_CONTRACT.md
140	0	docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

Headings ascend `70…75`; exactly ONE `---` closes the new block; the heading is
unique; and **`140 0` proves a PURE INSERTION — no landed line was touched.**

Contents: **§A** the rule, **§B** the four-cell polarity matrix, **§C** the
nine-sink MEASURED access-log matrix, **§D** the mode-scoping guard, **§E** Trap A,
**§F** Trap B, **§G** the authoritative fixtures, **§H** the two-fixture driver
constraint.

**DEVIATION from `PLAN.md`, on a measured figure.** The plan's §D text says the
arm-hoist mutation "turns three in-process guards RED". Freshly measured this
session it is **FOUR** (`649 passed` → `645 passed; 4 failed`), agreeing with the
`STATE.md` standing-traps figure; the plan inherited "three" from ADR-0159's
*pre*-measurement, taken before `absence_semantics_matrix_matches_measured_upstream`
existed. §D now carries FOUR with the test names spelled out, plus the two facts
the mutation work surfaced: that the arm ORDER is guarded ONLY in-process, and that
the two mutations are DISTINCT. The landed ADR-0159 is NOT edited (append-only,
D-3.5). ADR-0162 records this.

**Commit:** `b027eb0`.

---

## Task 6 — `BEHAVIOR_CONTRACT.md`: extend CF-72-2 §D, bank CF-75-1 as §G

**File modified:** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — **+57 / −5**.

The §D record grows from one sentence to three enumerated members (name-only
`{ name }`; `treat_missing_header_as_empty` accepted **AND HONORED** upstream; the
top-level `contains_match` arm). CF-75-1 is banked as the new `**§G`.

**Step 3 — verification.** VERBATIM:

```
$ awk '/^### Phase 72 /{f=1} f&&/^### Phase 73 /{exit} f' … | grep -o '^\*\*§[A-Z]'
**§A
**§B
**§C
**§D
**§E
**§F
**§G
$ … | sort | uniq -d          # duplicate letters — NONE
$ grep -c '^### Phase 7[0-9]' docs/envoy-rust/BEHAVIOR_CONTRACT.md
6
$ git diff --numstat docs/envoy-rust/BEHAVIOR_CONTRACT.md
57	5	docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

The letters run `§A…§G` in order with no duplicate, the phase-heading count is
unchanged from Task 5, and the **only** deleted lines are the five old §D lines the
plan directs replacing (verified by reading the diff's `-` lines, not by counting).

**DEVIATION (placement).** `PLAN.md` Step 2 says to add §G "immediately after" §D,
which would order the sub-headings `§A §B §C §D §G §E §F`. It is placed after
`**§F` — the block's end — so the letters read `§A…§G` in order, which is what the
plan's own Step 3 verification checks for. ADR-0162 records this.

**Commit:** `e325d04`.

---

## Task 7 — CONSUME M74-31: the causal kept-LAST non-sequitur, at all FOUR live sites

**Files modified (4):**

| file | change |
|---|---|
| `tests/differential/tests/access_log_metadata_filter.rs` | `//!` doc comment only |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | prose (phase-74 `**§H`) |
| `tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml` | `#` comments only |
| `tests/fixtures/0081-accesslog-metadata-filter/README.md` | prose |

**Step 1 — re-derived on the live tree, and the contract site HAD MOVED.** The
M74-31 contract site was at `:2657` at session start (ADR-0161 C3) but had drifted
to **`:2709`** by the time this task ran — pushed down by this session's own Tasks
5/6. Re-deriving by text anchor is what caught it.

**It is a FOUR-site problem (ADR-0161 C4).** `0081/expectations.yaml:20` (*"Probe 2
— KEPT, and placed SECOND (phase 74 §5.2 state-3 re-entry, `REVIEW.md` I-3)"*) is
DESCRIPTIVE, not causal, and was **left alone**. The "FIVE" figure at
`74/REVIEW.md:1269` is an append-only historical artifact and was **NOT** edited
(D-3.5).

**Step 6 — verification.** VERBATIM:

```
$ grep -rn 'placed SECOND\|stay SECOND' --include=*.rs --include=*.md --include=*.yaml tests/ docs/envoy-rust/BEHAVIOR_CONTRACT.md
tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml:20:    # Probe 2 — KEPT, and placed SECOND (phase 74 §5.2 state-3 re-entry,
tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml:36:    # It must stay SECOND, not last — but NOT for settle reasons: the driver's
docs/envoy-rust/BEHAVIOR_CONTRACT.md:2709:is placed SECOND, not last; kept-LAST (ADR-0147) holds because the LAST probe is

$ git diff --stat
 docs/envoy-rust/BEHAVIOR_CONTRACT.md                           |  6 ++++--
 tests/differential/tests/access_log_metadata_filter.rs         |  6 ++++--
 tests/fixtures/0081-accesslog-metadata-filter/README.md        | 10 ++++++----
 .../fixtures/0081-accesslog-metadata-filter/expectations.yaml  |  9 ++++++---
 4 files changed, 20 insertions(+), 11 deletions(-)

$ git diff .../0081-.../expectations.yaml | grep -E '^[+-]' | grep -v '^[+-][[:space:]]*#' | grep -v '^[+-][+-]'
(NOTHING)
$ git diff .../0081-.../expectations.yaml | grep -cE '^[+-].*(expect_logged|method:|extra_headers|expected_status)'
0
```

**Adjudicated by LINE and by FILE, not by count** (a grep here legitimately returns
>0 because the fixed records QUOTE the defect). Each surviving hit is non-causal:
`:20` is the descriptive one, `:36` now says explicitly "NOT for settle reasons",
and `:2709` now says "holds because the LAST probe is KEPT". **`0081` is
behaviorally untouched** — every changed line in its `expectations.yaml` is a
comment, and no probe, `expect_logged` value or probe ORDER moved. The `.rs` edit
is `//!` only (verified by a diff filtered of `//!`, which printed nothing — the
`mechanical-fanout-scripts-corrupt-doc-comments` guard).

**Step 7 — `0081`'s entrypoint re-run to prove the edits were comment-only.** VERBATIM:

```
$ cargo test -p differential --test access_log_metadata_filter -- --nocapture
test access_log_metadata_filter ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.66s
```

Neither `0081` nor `0082` gained an `on_header_missing` block (ADR-0155 PV-6).

**Commit:** `60f1322`.

---

## Task 8 — The sub-phase-75.1 review's open findings: M-1, M-2, M-3, N-1, N-2

**Files modified (4):** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (M-1, M-2/N-2, M-3),
`crates/envoy-config/src/bootstrap.rs` (M-2 mirror, **doc comment only**),
`crates/envoy-config/src/matcher.rs` (N-1, **test-module comment only**),
`tests/fixtures/0081-accesslog-metadata-filter/README.md` (M-3).

**Step 1 — all six sites re-derived by text anchor.** M-3's contract site had
drifted `:2545` → **`:2597`** from this session's Tasks 5/6.

- **M-1** — the stale `matcher.rs:52` XOR citation made **LINE-NUMBER-FREE** ("the
  XOR that closes the function") rather than re-pointed at `:69`. That citation
  class has gone stale three times (`:51` → `:52` → `:69`), twice inside the phase
  chartered to fix it. The PAST-TENSE `matcher.rs:52` at **`matcher.rs:473`** is a
  correct historical reference and was deliberately **left untouched** (the 75.1
  review adjudicated it explicitly as not a finding).
- **M-2** (both sites) — "AGREE when PRESENT / DIFFER when ABSENT" was over-broad:
  ABSENT × `present_match: true` also AGREES. Now states the exact arithmetic —
  they differ in exactly ONE of four cells. **The load-bearing "do NOT unify them"
  instruction is KEPT**; only the reasoning is corrected.
- **M-3** (both sites) — re-tensed to "whose divergence *was* mode-scoped and is
  CLOSED by sub-phase 75.1". The `0081/README.md` copy's `> ` blockquote prefix was
  preserved. The surrounding CF-74-1 conflation warning is the point of the
  sentence and is KEPT.
- **N-1** — "flipped the two `false ×` expectations" → exactly **ONE**
  (`false × present`, true → false); `false × absent` keeps its verdict, now for
  the right reason. This is what the test's own body comment 20 lines below already
  said.
- **N-2** — folded into M-2's replacement text; the ambiguous "See §C" now points
  at the **Phase 75** block and its §E.

**Step 6 — verification.** VERBATIM:

```
$ grep -n 'See §C' docs/envoy-rust/BEHAVIOR_CONTRACT.md
(NOTHING)
$ grep -c '^\*\*§C ' docs/envoy-rust/BEHAVIOR_CONTRACT.md
9
```

**The §C count is 9, not the plan's expected 8 — adjudicated, not accepted.** It
was **8** at session start (`git show 1bf256aa:…| grep -c`) and is 9 now because
**this session's own Task 5** adds `**§C The MEASURED access-log matrix**` at
`:2777`. Task 8's own diff contains no `±**§C ` heading line at all, so the
invariant the check actually asserts — *this task removes an ambiguous reference,
not a heading* — HOLDS.

All four stale claims are gone (`matcher.rs:52` in the contract,
`DIFFER when it is ABSENT` at both sites, `divergence is mode-scoped` at both
sites, `flipped the two`) — each verified by a grep returning nothing.

**Step 7 — the two `crates/` edits are COMMENT-ONLY.** VERBATIM:

```
$ git diff crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/matcher.rs \
    | grep -E '^[+-]' | grep -v '^[+-][+-]' | grep -vE '^[+-][[:space:]]*(///|//)'
(NOTHING)
```

Then, with both files `touch`ed first so no gate could be a cached false green:

```
##### cargo build --workspace --all-targets #####
   Compiling http1-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http1-echo-server)
   Compiling envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
   Compiling http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.37s
##### cargo clippy --workspace --all-targets --all-features -- -D warnings #####
    Checking envoy-admin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-admin)
    Checking http1-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http1-echo-server)
    Checking envoy-health v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-health)
    Checking http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Checking envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.77s
##### cargo fmt --all -- --check #####
FMT CLEAN
##### cargo test -p envoy-config --lib #####
test result: ok. 649 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

`649 passed` — **the same count as the pre-edit control**, confirming no test was
added, removed or altered. `cargo fmt` does NOT reflow doc comments, so the edited
comment lines were kept within the surrounding files' width by hand.

**M-5 and N-3 need NO fix** and appear nowhere in this implementation — both are
landed historical artifacts (`PROGRESS.md` blocks and commit messages) whose
retroactive editing would be worse than the imprecision (D-3.5). **N-4** is a
coverage note, record only.

**Commit:** `939a14c`.

---

## Task 9 — `PROGRESS.md` and the census

**File created:** this document.

**ADR-0162 FIRED** — the mutation-procedure correction above, appended as a **PURE
INSERTION** at the head of the newest-first block (`git diff --numstat` reads
`17 0`, so no landed ADR was edited; ADR-0161's text is byte-identical). Ledger
head is now **ADR-0162**, next **ADR-0163**.

### Step 2 — census figures at completion

```
fixture dirs:      85    (expected 85)   ✓
diff test files:   85    (expected 85)   ✓
known-failures:    21    (expected 21 — NEVER trimmed)   ✓
corpus seeds:      63    (expected 63 — unchanged)   ✓
fuzz targets:       5    (expected 5 — unchanged)   ✓
ROADMAP:          104 rows, 102 done, 2 in-progress (75, 75.2)   ✓
```

**`#![forbid(unsafe_code)]` — the plan's recipe is unsound; re-derived properly.**
`PLAN.md` expects **17** from
`grep -rc '#!\[forbid(unsafe_code)\]' $(git ls-files 'crates/*/src/{lib,main}.rs') | grep -c ':1'`.
That recipe is wrong twice over: it globs only `crates/` (**14** roots) when the
workspace has **22** members, and it returns **13** because
`crates/envoy-listener/src/lib.rs` contains the string **twice**. Re-derived by
enumerating members from the ROOT `Cargo.toml` (per the standing "never a repo-wide
`find`" trap, which would also have swept the parallel workstream's worktrees):
**22 of 22 crate roots carry the attribute exactly once — D-3.8 HOLDS.** The
invariant was never at risk; only the counting recipe was. This is the standing
"adjudicate by LINE and by FILE, never by COUNT" trap firing on the plan's own
verification step. Recorded in ADR-0162.

### Net LoC against the §6.1 projection

```
$ git diff --shortstat 1bf256aa…  (excluding this PROGRESS.md)
 17 files changed, 1026 insertions(+), 28 deletions(-)
```

**≈ 998 net LoC against the plan's ~760 projection — an overshoot of ~31%**, in the
same direction and of the same order as sub-phase 75.1's (which projected ~1210 and
landed 1457, +20%). It remains **~33% under the ~1500 §6.1 gate**, so the
no-split verdict holds comfortably. The overshoot is concentrated in the two
fixture READMEs (139 + 167 lines against the plan's ~120 + ~110), in ADR-0162
(17 lines, unprojected because the mutation finding was unforeseen) and in the §D
expansion that finding required.

---

## What this sub-phase did NOT do

- **No `crates/` behavior change.** The only two `crates/` edits are comment-only,
  proven by a diff filtered of `///`/`//` printing EMPTY. The 75.1 engine was NOT
  re-derived, reverted or "simplified"; the mutations lived only in a scratch
  worktree that was restored and removed.
- **The §7.5 gate was NOT run as this session's state** — that is state-4. Only the
  per-task verifications above were run, plus the build/clippy/fmt/`envoy-config`
  gates Task 8 Step 7 requires. **A full `cargo test --workspace --no-fail-fast`
  sweep has NOT been run and is state-4's job**, along with `cargo deny check`.
- **Sub-phase 75.1 was NOT re-opened, re-verified or re-graded**; no `75.1/` or
  parent `75/` artifact was touched.
- **ROADMAP row `75.2` stays `in-progress`** with all 6 cells preserved, and parent
  row `75` stays `in-progress` — it flips only at 75.2's own state-6 close-out.
- **No new fuzz target, corpus seed or `ci.yml` step** (§7.4, re-confirmed).
- **`known-failures.txt` was NOT trimmed** (21 lines).

## Observation handed to the state-5 review (not fixed here)

The phase-72 `**§C` block in `BEHAVIOR_CONTRACT.md` (around `:2404-2406`) carries
the same understated *"turns three in-process guards RED"* phrasing this session
measured as **FOUR**. It is a live document, not an append-only one, so it COULD be
corrected — but it is outside `PLAN.md`'s Task 8 finding list, so it was left alone
rather than widened into. Recorded here and in ADR-0162 for the reviewer to weigh.

## Next

**Sub-phase 75.2 §5 state-4 — the §7.5 verification gate**, in a SEPARATE session
(§5.1; ADR-0127: the context that wrote an artifact must not grade it).
`PLAN.md`'s "The §7.5 phase-done gate" section lists exactly what to run and the
adjudication discipline for gate (b), including the documented CI-authoritative
host-flake set. **This session did not chain into it.**
