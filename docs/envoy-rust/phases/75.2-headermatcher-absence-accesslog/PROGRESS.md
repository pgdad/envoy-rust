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

---

# §5 state-4 — the §7.5 phase-done gate

> **What this section is.** The §5 **state-4 verification** for sub-phase 75.2,
> run in a SEPARATE session from the state-3 implementation above (§5.1;
> ADR-0127 — the context that wrote an artifact must not grade it). It runs the
> gate that `PLAN.md`'s "The §7.5 phase-done gate" section specifies and quotes
> every command output VERBATIM. **State-4 is SOLO-SERIAL** — no subagents were
> dispatched; the cargo lock makes concurrency pointless and the gate-(b)
> adjudication is one indivisible judgment. **This session VERIFIES only:** it
> wrote no `REVIEW.md`, re-implemented nothing, changed no `crates/` code, did
> not re-open or re-grade sub-phase 75.1, and flipped no ROADMAP row.
>
> Outputs are pasted VERBATIM under the same two mechanical conventions declared
> at the head of this document: `tracing` lines have their ANSI colour escapes
> stripped and their `2026-07-26T…Z` timestamps removed, message text unaltered.
> Nothing else is elided or retyped — every `test result:` line, every `N passed`
> count and every assertion message is byte-for-byte as emitted.
>
> **Session start state (verified on disk, not trusted from the handoff):**
> `git status --porcelain` clean; branch `main`; `HEAD` ==
> `2ae5f464e67e63d4cf7525d870ee9858fc228706`; `git fetch origin --prune` showing
> `origin/main` at the SAME SHA, re-checked at the end of the gate. CI on that
> full 40-char SHA re-confirmed GREEN — run `30202383015`, `completed`/`success`,
> both jobs at FULL step counts **15** (`build + test + lint`) and **13**
> (`fuzz`), so no runner-starvation signature (`steps: 0` + `runner_name: ""`).
>
> **State detection: `PLAN.md` AND `PROGRESS.md` exist, all 9 tasks are
> implemented and committed, and no `REVIEW.md` exists — §5 state 4 exactly.**

## Gate (e) — build / clippy / fmt / test / deny

### `cargo build --workspace --all-targets`

VERBATIM:

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
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.14s
EXIT=0
```

**Zero warnings** (`grep -c warning` over the captured output → `0`). This also
satisfies the standing "`cargo build -p envoy-bin` before ANY local differential"
rule — `envoy-bin` is compiled in the list above, so every differential run below
executed against a FRESH `target/debug/envoy-bin`, not a stale one.

**The green was audited rather than assumed.** Re-running immediately afterwards
is fully cached, which proves the first invocation really did the compilation work
rather than short-circuiting:

```
$ cargo build --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.08s
EXIT=0
```

### `cargo fmt --all -- --check`

VERBATIM (the command prints NOTHING when clean; the exit code is the signal):

```
$ cargo fmt --all -- --check
EXIT=0
```

### `cargo clippy --workspace --all-targets --all-features -- -D warnings`

**Forced, not inherited.** Per the standing partial-cache trap, the five files
this sub-phase edited were `touch`ed first so clippy could not report a cached
green — and `cargo clippy` prints `Checking`, NOT `Compiling`, so the re-check is
confirmed by the `Checking` lines for `envoy-config` and `differential`:

```
$ touch tests/differential/tests/headermatcher_absence_accesslog.rs \
        tests/differential/tests/headermatcher_absence_accesslog_present_polarity.rs \
        tests/differential/tests/access_log_metadata_filter.rs \
        crates/envoy-config/src/bootstrap.rs \
        crates/envoy-config/src/matcher.rs

$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Checking envoy-config v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-config)
    Checking differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
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
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.91s
EXIT=0
```

**14 `Checking` lines; zero `warning`/`error` lines.** Both edited crates
(`envoy-config` and `differential`) appear, so the run was genuinely forced.

### `cargo deny check`

VERBATIM:

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

**GREEN — `advisories ok, bans ok, licenses ok, sources ok`.** This was one of the
two things the state-3 session had NOT run at all. The five warnings are
`license-not-encountered`: they name licences ALLOWED in `deny.toml` that no
dependency in the current graph actually uses. They are unmatched *allowances*,
not violations — nothing is denied, and no dependency was added or bumped. No
freshly-published RustSec advisory fired, so the `cargo update --precise`
contingency was not needed.

## Gate (b) — the full workspace sweep (the dominant risk)

The second of the two things never run at state-3, and the substance of this
session. Run per `PLAN.md`'s adjudication discipline: `--no-fail-fast` (a bare
`cargo test --workspace` aborts at the first failing BINARY and would have hidden
four of the five REDs below), with the FULL output redirected to a FILE — **never
piped through `tail`**, which truncates the `failures:` block and destroys the
very names the gate must adjudicate.

**Run TWICE and the failing SET diffed**, because the startup-race flake family's
membership changes run to run.

### Totals

```
$ cargo test --workspace --no-fail-fast > sweep1.txt 2>&1 ; echo "EXIT=$?"
EXIT=101
$ cargo test --workspace --no-fail-fast > sweep2.txt 2>&1 ; echo "EXIT=$?"
EXIT=101

sweep1: passed=2100 failed=5 sum=2105
sweep2: passed=2100 failed=5 sum=2105
```

(Summed over every `test result:` line in each run — 162 test binaries per run.)

### The failing SET, diffed across the two runs

```
=== failing SET run 1 ===
access_log_h2_rcd_upstream_reset
access_log_h2_uc_upstream_reset
access_log_rcd_upstream_reset
access_log_rf_upstream_reset
admin_config_dump_server_info
=== failing SET run 2 ===
access_log_h2_rcd_upstream_reset
access_log_h2_uc_upstream_reset
access_log_rcd_upstream_reset
access_log_rf_upstream_reset
admin_config_dump_server_info
=== diff of the two SETs (empty == identical) ===
IDENTICAL
```

**The set is STABLE across both runs** — no startup-race member appeared in either
run, so nothing had to be adjudicated on changing membership.

### The `local passed + failed == CI passed` cross-check

CI totals extracted from the run on this exact HEAD SHA (run `30202383015`,
162 `test result:` lines):

```
CI total passed=2105  failed=0
local passed=2100  failed=5   ->  2100 + 5 = 2105
```

**The identity holds exactly.** Every test that passes in CI is accounted for
locally as either a pass or one of the five environmentally-failing members —
there is no test that CI runs and this host silently skipped, and no test that
went RED locally beyond the five.

### Adjudication of all five REDs, BY NAME and by failure TEXT

All five are members of the documented CI-AUTHORITATIVE host-flake set enumerated
VERBATIM in `PLAN.md`'s §7.5 gate section. Each was re-run **in ISOLATION naming
its target binary**, and each reported `running 1 test` / `1 failed` — **not**
`0 passed; N filtered out` (which would be a false green) and **not**
`error: no test target named …` (which exits 101 exactly like a real RED).

```
$ cargo test -p differential --test access_log_h2_rcd_upstream_reset
running 1 test
test access_log_h2_rcd_upstream_reset ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.78s
EXIT=101

$ cargo test -p differential --test access_log_h2_uc_upstream_reset
running 1 test
test access_log_h2_uc_upstream_reset ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.71s
EXIT=101

$ cargo test -p differential --test access_log_rcd_upstream_reset
running 1 test
test access_log_rcd_upstream_reset ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.74s
EXIT=101

$ cargo test -p differential --test access_log_rf_upstream_reset
running 1 test
test access_log_rf_upstream_reset ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.72s
EXIT=101

$ cargo test -p differential --test admin_config_dump_server_info
running 1 test
test admin_config_dump_server_info ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.68s
EXIT=101
```

**All five fail DETERMINISTICALLY in isolation. That determinism IS the
environmental signature** for these two families — it is the startup-race family
whose members pass in isolation and vary run to run, and no startup-race member
appeared at all.

**Members 1–4 — the `TcpCloseBackend` IPv6-unreachable set** (the four
`access_log_*_upstream_reset` binaries, named verbatim in `PLAN.md`'s flake list).
Root cause, read from the failure TEXT rather than inferred: upstream Envoy in its
container cannot REACH the host-spawned close backend, so it reports a connect
FAILURE (`rf: UF`) where envoy-rust correctly reports the reset (`rf: UC`). Two of
the four log `%RESPONSE_CODE_DETAILS%` and therefore spell the cause out in full:

```
---- access_log_h2_rcd_upstream_reset stdout ----

thread 'access_log_h2_rcd_upstream_reset' (3860681) panicked at tests/differential/tests/access_log_h2_rcd_upstream_reset.rs:28:10:
fixture green: access log byte-exact mismatch: line 0 not byte-identical: envoy="{\"method\":\"GET\",\"proto\":\"HTTP/2\",\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:42349}\",\"rf\":\"UF\"}" envoy-rust="{\"method\":\"GET\",\"proto\":\"HTTP/2\",\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}"
envoy lines: ["{\"method\":\"GET\",\"proto\":\"HTTP/2\",\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:42349}\",\"rf\":\"UF\"}"]
envoy-rust lines: ["{\"method\":\"GET\",\"proto\":\"HTTP/2\",\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}"]
```

`immediate_connect_error:_Network_is_unreachable` at an **IPv6** literal
(`[fdc4:f303:9324::254]`) is the signature exactly. The other two carry no `rcd`
field, so their log format shows only the same root cause's downstream effect:

```
---- access_log_h2_uc_upstream_reset stdout ----
fixture green: access log byte-exact mismatch: line 0 not byte-identical: envoy="{\"method\":\"GET\",\"proto\":\"HTTP/2\",\"rc\":503,\"rf\":\"UF\"}" envoy-rust="{\"method\":\"GET\",\"proto\":\"HTTP/2\",\"rc\":503,\"rf\":\"UC\"}"

---- access_log_rf_upstream_reset stdout ----
fixture green: access log byte-exact mismatch: line 0 not byte-identical: envoy="{\"rc\":503,\"rf\":\"UF\"}" envoy-rust="{\"rc\":503,\"rf\":\"UC\"}"
```

**Member 5 — `admin_config_dump_server_info`, the `192.168.65.2` bridge-IP
family.** Upstream Envoy resolves `host.docker.internal` to this host's Docker
bridge address `192.168.65.2` and lists that endpoint in `/clusters`; envoy-rust,
running natively, does not:

```
---- admin_config_dump_server_info stdout ----

thread 'admin_config_dump_server_info' (3866548) panicked at tests/differential/tests/admin_config_dump_server_info.rs:18:10:
fixture green: admin body rule: /clusters

Caused by:
    text_lines diverged after allow-lists:
      envoy-only:      ["backend::192.168.65.2:32869::canary::false", "backend::192.168.65.2:32869::cx_active::0", "backend::192.168.65.2:32869::cx_connect_fail::0", "backend::192.168.65.2:32869::cx_total::0", "backend::192.168.65.2:32869::health_flags::healthy", "backend::192.168.65.2:32869::hostname::host.docker.internal", "backend::192.168.65.2:32869::local_origin_success_rate::-1", "backend::192.168.65.2:32869::priority::0", "backend::192.168.65.2:32869::region::", "backend::192.168.65.2:32869::rq_active::0", "backend::192.168.65.2:32869::rq_error::0", "backend::192.168.65.2:32869::rq_success::0", "backend::192.168.65.2:32869::rq_timeout::0", "backend::192.168.65.2:32869::rq_total::0", "backend::192.168.65.2:32869::sub_zone::", "backend::192.168.65.2:32869::success_rate::-1", "backend::192.168.65.2:32869::weight::1", "backend::192.168.65.2:32869::zone::"]
      envoy-rust-only: []
```

`envoy-rust-only: []` is decisive: envoy-rust emitted nothing WRONG — the
divergence is entirely extra lines on the upstream side, produced by a host
networking fact. **None of the five touches the `HeaderMatcher`, the access-log
FILTER path, or anything sub-phase 75.2 changed**, and all five are green in CI on
this exact SHA.

**A mass-RED cross-check was also considered and ruled out:** a Docker-daemon
outage presents as dozens of simultaneous `client error (Connect)` failures. Only
five tests failed, `docker info` reported the daemon healthy, and the pinned image
`envoyproxy/envoy:v1.33.0` (`56da5afd7df3`, matching the `ENVOY_TARGET.md` digest)
was present locally, so that family is excluded.

### The specifically-watched fixtures — `0078`–`0083`

`PLAN.md` singles these out because `0081` and `access_log_metadata_filter` were
EDITED (comments only) at state-3. Result lines paired to their `Running` lines in
BOTH sweeps:

```
sweep1  access_log_header_filter (0078):                    ok. 1 passed; 0 failed; ... finished in 3.34s
sweep2  access_log_header_filter (0078):                    ok. 1 passed; 0 failed; ... finished in 3.29s
sweep1  access_log_and_filter (0079):                       ok. 1 passed; 0 failed; ... finished in 3.23s
sweep2  access_log_and_filter (0079):                       ok. 1 passed; 0 failed; ... finished in 3.12s
sweep1  access_log_or_filter (0080):                        ok. 1 passed; 0 failed; ... finished in 12.74s
sweep2  access_log_or_filter (0080):                        ok. 1 passed; 0 failed; ... finished in 12.64s
sweep1  access_log_metadata_filter (0081):                  ok. 1 passed; 0 failed; ... finished in 12.72s
sweep2  access_log_metadata_filter (0081):                  ok. 1 passed; 0 failed; ... finished in 12.62s
sweep1  access_log_metadata_filter_key_not_found (0082):    ok. 1 passed; 0 failed; ... finished in 3.24s
sweep2  access_log_metadata_filter_key_not_found (0082):    ok. 1 passed; 0 failed; ... finished in 3.11s
sweep1  headermatcher_absence_parity (0083):                ok. 1 passed; 0 failed; ... finished in 1.14s
sweep2  headermatcher_absence_parity (0083):                ok. 1 passed; 0 failed; ... finished in 1.04s
```

The fixture→binary mapping was DERIVED from the tree
(`grep -rl "tests/fixtures/00NN" tests/differential/tests/*.rs`), not assumed from
the fixture name — `0079`/`0080` are `access_log_and_filter`/`access_log_or_filter`,
which a name-guessed grep would have missed and reported as a vacuous `0 hits`.
Each lookup asserts the binary was FOUND before reading its verdict.

**Gate (b) VERDICT: GREEN.** All 83 pre-existing fixtures and every in-process
test are accounted for; the only five REDs are documented, environmental,
deterministic-in-isolation, unrelated to this sub-phase's surface, and green in CI.

## Gate (a) — the two NEW fixtures

Re-run in isolation, **asserting on the `1 passed` count and never on the exit
code**. VERBATIM:

```
$ cargo test -p differential --test headermatcher_absence_accesslog
   Compiling differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.35s
     Running tests/headermatcher_absence_accesslog.rs (target/debug/deps/headermatcher_absence_accesslog-b4eb4ae02b28971f)

running 1 test
INFO node registered node.id=envoy-rust-phase-75-fixture-0084 node.cluster=envoy-rust-phase-75
INFO listener bound with SO_REUSEPORT (one accept queue per worker) addr=127.0.0.1:45687 sockets=32
INFO envoy-rust listening (http_connection_manager) addr=127.0.0.1:45687 stat_prefix=ingress_http codec_type=HTTP1
test headermatcher_absence_accesslog ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.26s
EXIT=0
```

```
$ cargo test -p differential --test headermatcher_absence_accesslog_present_polarity
   Compiling differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.36s
     Running tests/headermatcher_absence_accesslog_present_polarity.rs (target/debug/deps/headermatcher_absence_accesslog_present_polarity-3fa1b92e04b9a4f4)

running 1 test
INFO node registered node.id=envoy-rust-phase-75-fixture-0085 node.cluster=envoy-rust-phase-75
INFO listener bound with SO_REUSEPORT (one accept queue per worker) addr=127.0.0.1:45609 sockets=32
INFO envoy-rust listening (http_connection_manager) addr=127.0.0.1:45609 stat_prefix=ingress_http codec_type=HTTP1
test headermatcher_absence_accesslog_present_polarity ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.15s
EXIT=0
```

**`1 passed; … 0 filtered out` on both — a real pass, not the `0 passed; N
filtered out` false green.** Both also passed inside BOTH full sweeps (see the
watched-fixture table above), so gate (a) has three independent green runs each.
The interleaved envoy-rust boot lines prove the subject proxy really started, and
the ~3 s runtime on a backend-free fixture with a warm Envoy image is NORMAL, not
a silent skip.

**No mutation check was re-run at state-4, deliberately.** The load-bearingness of
both fixtures was established at state-3 and is recorded in ADR-0162; re-running a
tree-mutating probe is not a state-4 action, and ADR-0162's central finding — that
the two mutations are DISTINCT and hit DIFFERENT cells — is a state-5 reading, not
a gate item.

## Gate (c) — conformance

`known-failures.txt` is **21 lines** and byte-unchanged against `HEAD`
(`git diff --stat HEAD -- tests/conformance/h2spec/known-failures.txt` prints
nothing). **It was NOT trimmed.** The conformance crate is green in both sweeps:

```
sweep1  h2spec_runner: test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
sweep2  h2spec_runner: test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**That local green must NOT be read as a conformance pass, and this session
declines to report it as one.** `h2spec_pass_rate_gate` self-skips when the
h2spec binary is absent, and this host has no such binary. Proven with
`--nocapture`, which reveals the message `cargo test` otherwise swallows:

```
$ cargo test -p h2spec-conformance --test h2spec_runner -- --nocapture
running 3 tests
test tests::parse_summary_line_extracts_pass_fail_counts ... ok
test tests::parse_h2spec_output_extracts_section_failure_ids ... ok
h2spec_runner: h2spec not found — skipping locally
test h2spec_pass_rate_gate ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

So gate (c) rests on the criterion `PLAN.md` actually states — **"unchanged"** —
and that is satisfied on three independent grounds: `known-failures.txt` is
byte-identical at 21 lines; sub-phase 75.2 changed **no** H2, codec or conformance
code at all (its only two `crates/` edits are comment-only); and CI on this exact
SHA reports `test h2spec_pass_rate_gate ... ok`. Trimming `known-failures.txt` on
local evidence would in any case have been wrong — this host scores h2spec
3.5/2 as PASS where CI does not.

**An observation for the state-5 review, NOT fixed here (see the list at the
end).** In the CI log for run `30202383015` the `h2spec_runner` binary reports
`finished in 0.15s`, even though CI provisions a real h2spec 2.6.0 on `PATH`
(`.github/workflows/ci.yml:43-49`, under `set -euo pipefail` with a `h2spec
--version` assertion) so `locate_h2spec()` should find it. 0.15 s is not a
plausible duration for spawning `envoy-bin` and running the h2spec suite against
it. This is a **pre-existing** property of the conformance harness, entirely
untouched by sub-phase 75.2 and identical on every recent SHA, so it is recorded
rather than acted on: chasing it would be a state-3 re-entry on an out-of-scope
surface, which a state-4 session must not do.

## Gate (d) — fuzz

**Nothing new to run**, re-confirmed on disk rather than inherited. No new fuzz
target, no new corpus seed, no `ci.yml` step was added by this sub-phase:

```
$ git ls-files | grep 'fuzz_targets/.*\.rs$'
crates/envoy-accesslog/fuzz/fuzz_targets/accesslog_format_parse.rs
crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs
crates/envoy-filter/fuzz/fuzz_targets/cdn_loop_parse.rs
crates/envoy-http2/fuzz/fuzz_targets/grpc_health_decode.rs
crates/envoy-jwt/fuzz/fuzz_targets/jwt_parse.rs
-- count --
5

$ git ls-files | grep -c 'fuzz/corpus/parse_bootstrap/'
63
```

**5 targets and 63 tracked seeds — both unchanged.** The census uses `git
ls-files` rather than a repo-wide `find`, which would have swept the parallel
workstream's `.claude/worktrees/agent-*` copies and inflated every count.
The existing `parse_bootstrap` target already covers the unchanged `HeaderMatcher`
deserializer and is parse-only — it never calls `HeaderMatcher::matches` — so no
seed could encode the runtime semantics this sub-phase witnesses. CI's `fuzz` job
ran all five targets green on this SHA (13 steps).

## Gate (f) — review

**`REVIEW.md` does not exist. That is state-5's output and is deliberately NOT
produced by this session** (§5.1; ADR-0127). Gate (f) is therefore OPEN by design,
and is the only one of the six that is.

## Censuses re-verified on disk at the close of the gate

```
fixture dirs:      85    (expected 85)   ✓
diff test files:   85    (expected 85)   ✓
known-failures:    21    (expected 21 — NEVER trimmed)   ✓
corpus seeds:      63    (expected 63 — unchanged)   ✓
fuzz targets:       5    (expected 5 — unchanged)   ✓
ROADMAP:          104 rows, 102 done, 2 in-progress (75, 75.2)   ✓
```

The ROADMAP census splits on `' | '` WITH the spaces and reads status from field
**4**; a naive `awk -F'|' $5` false-reports rows `36`/`38`/`39`/`52`/`54`/`66`/`70`
as not-done, because those carry unescaped `|` and must NOT be "fixed"
(append-only). The two non-`done` rows are exactly `75` and `75.2`, both
`in-progress` — **neither was flipped by this session**; both flips belong to
75.2's state-6 close-out.

**D-3.8 re-derived independently** (the state-3 session showed `PLAN.md`'s recipe
to be unsound in two ways; this is a fresh derivation, not a re-quote). Enumerating
workspace members from the ROOT `Cargo.toml` — **22** members — and reading each
member's `src/lib.rs` or `src/main.rs`:

```
roots scanned=22   carrying the attribute=22
note: crates/envoy-listener/src/lib.rs contains the string 2 times (comment/test mention)
```

**22 of 22 crate roots carry `#![forbid(unsafe_code)]` — D-3.8 HOLDS.** The
`envoy-listener` double occurrence is the incidental second mention state-3
identified, and is why a naive `grep -c … | grep -c ':1'` undercounts.

## Gate verdict

| gate | verdict | evidence |
|---|---|---|
| (a) new fixtures green | **GREEN** | `0084` and `0085` each `1 passed; 0 filtered out`, in isolation AND inside both sweeps |
| (b) pre-existing fixtures green | **GREEN** | 2 × `--no-fail-fast` sweeps, identical stable failing set of 5, all documented/environmental/deterministic-in-isolation; `2100 + 5 = 2105 = CI passed` |
| (c) conformance | **GREEN (unchanged)** | `known-failures.txt` 21 lines byte-identical, NOT trimmed; no H2/codec/conformance code touched; CI green on this SHA. Local gate self-skips — recorded, not claimed as a pass |
| (d) fuzz | **GREEN (nothing new)** | 5 targets / 63 seeds unchanged; no `ci.yml` step added |
| (e) build / clippy / fmt / test / deny | **GREEN** | build exit 0 zero warnings (audited via a cached re-run); fmt exit 0; clippy exit 0 forced over both edited crates; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` |
| (f) review | **OPEN BY DESIGN** | state-5's output; not written here |

**Gates (a)–(e) are GREEN. The §7.5 gate did NOT red on any genuine defect, so
there is no §5.2 re-entry to state 3.** Sub-phase 75.2 is VERIFIED and advances to
§5 state-5, the code review.

## What this state-4 session did NOT do

- **Wrote no `REVIEW.md`** and did not chain into state-5 (§5.1; ADR-0127).
- **Changed no `crates/` code** — nothing beyond `touch` (mtime only) was applied
  to the tree; `git status --porcelain` was clean at session start AND after the
  entire gate, and the only content change in the phase commit below is this
  `PROGRESS.md` section plus `STATE.md`/`STATE_HISTORY.md`.
- **Ran no mutation and created no `git worktree`** — load-bearingness is already
  established (ADR-0162), and a tree-mutating probe is not a state-4 action.
- **Did not re-open, re-verify or re-grade sub-phase 75.1**, nor touch any `75.1/`
  artifact or the FROZEN parent `75/SPEC.md`.
- **Flipped no ROADMAP row.** `75.2` stays `in-progress` with all 6 cells
  preserved; parent `75` stays `in-progress`. Both flips belong to state-6.
- **Did not trim `known-failures.txt`**, weaken a fixture, touch a probe or an
  `expect_logged` value, or add `on_header_missing` to `0081`/`0082`.
- **Left the parallel workstream alone** — the four `.claude/worktrees/agent-*`
  trees and the long-running `envoyproxy/envoy:v1.33.0` container (Up 2 days) were
  neither removed nor swept into any census.

## Observations handed to the state-5 review (neither fixed here)

1. **Carried forward from state-3, still open.** The phase-72 `**§C` block in
   `BEHAVIOR_CONTRACT.md` still says the arm-hoist mutation *"turns three
   in-process guards RED"* where state-3 MEASURED **four**. Outside `PLAN.md`'s
   Task 8 finding list; a live document, so it COULD be corrected.
2. **NEW at this state-4 gate.** The h2spec conformance gate is not demonstrably
   executing its suite: it self-skips locally (no h2spec binary on this host,
   proven with `--nocapture`), and in CI — where h2spec 2.6.0 IS provisioned on
   `PATH` — the `h2spec_runner` binary nevertheless reports `finished in 0.15s`,
   which is not a plausible duration for a real run. **Pre-existing and wholly
   unrelated to sub-phase 75.2** (which touched no H2, codec or conformance code),
   so it was recorded rather than chased. If the reviewer agrees it is real, it
   wants its own phase, not a widening of 75.2.

## Next

**Sub-phase 75.2 §5 state-5 — the code review** (`superpowers:requesting-code-review`
→ `REVIEW.md`), in a SEPARATE session (§5.1; ADR-0127: the context that ran a gate
must not grade the work). If the review raises issues, re-entry is at **state 3**,
not state 4 (§5.2). **This session did not chain into it.**

---

# §5.2 state-3 RE-ENTRY — closing the `REVIEW.md` findings

> **What this section is.** The **§5.2 re-entry** log for sub-phase 75.2. Per
> `BOOTSTRAP_PROMPT.md` §5.2, a `REVIEW.md` carrying issues re-enters the lifecycle
> at **step 3, NOT step 4** — this session resumed IMPLEMENTATION under TDD; it did
> **not** re-run the §7.5 gate (that is the state-4 RE-VERIFICATION, a separate
> session after this one). Written for a stranger with zero prior context (D-3.4).
>
> **Elision policy for THIS section — three declared classes, correcting review
> finding N-7.** The earlier sections of this document declare exactly two
> mechanical exceptions, both concerning *output*, and close with "Nothing else is
> elided"; N-7 correctly observed that several quoted *command lines* were also
> abbreviated with `…`, an undeclared third class. This section declares all three
> up front: (1) `diff -u` header `---`/`+++` lines are dropped (they carry mtimes,
> not content); (2) ANSI colour escapes and `2026-07-27T…Z` timestamps are stripped
> from `tracing` lines, message text unaltered; (3) where a quoted COMMAND line is
> shortened, the shortening is marked `[…]` and what was removed is stated in the
> surrounding prose. Every `test result:` line, every `N passed` count and every
> assertion message below is byte-for-byte as emitted.
>
> **Session start state (verified on disk, not trusted from the handoff):**
> `git status --porcelain` clean; branch `main`; `HEAD` ==
> `29055a5086c1fac2eb90e27f4bb523f0cd19d183`; `git fetch origin --prune` showing
> `origin/main` at the SAME SHA, re-checked immediately before committing. CI on
> that FULL 40-char SHA confirmed GREEN — run `30256369763`, `completed`/`success`,
> both jobs at FULL step counts **15** (`build + test + lint`) and **13** (`fuzz`),
> so no runner-starvation signature (a starved job reports `steps: 0`).
>
> **State detection.** `SPEC.md`, `PLAN.md`, `PROGRESS.md` AND `REVIEW.md` all
> exist, and `REVIEW.md` carries a **CHANGES-REQUESTED** verdict — §7.5 gate (f) is
> NOT met, so §5.2 routes to step 3. Skills invoked in order:
> `superpowers:receiving-code-review`, then `superpowers:executing-plans` with
> `superpowers:test-driven-development` on the one finding that touches a fixture.

## How each finding was weighed

`superpowers:receiving-code-review` requires the findings be evaluated on their
technical merits and verified against the codebase before acting — not agreed with
performatively. **Every factual claim below was re-checked on disk in this session
before the corresponding edit was made.** All four Importants and all four
actionable Minors verified as stated; none needed pushing back on. Two verification
notes worth recording:

- **I-1 was re-MEASURED from scratch, not taken on the review's word.** Both
  pre-fix mutation results were independently reproduced in this session's own
  scratch worktree before the fix was written (§ "The TDD RED" below). They
  reproduced exactly: `642 passed; 7 failed` and `645 passed; 4 failed`.
- **M-5 was investigated further and one NEW fact was found** that the review
  missed and that partially explains the anomaly (below). The review's own
  conclusion still stands.

## I-1 — the fixture strengthening (the only finding that touches a fixture)

**The defect.** `Driver::Http1AccessLogByteExact` asserts exactly two things: each
side's log file holds `expected_logged_count(probes)` lines, and those lines are
byte-identical cross-proxy. There is no per-probe assertion and no expected-line
field. Both fixtures put `path: /x` on EVERY probe while the format renders
`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%`, so every probe produced the identical
line `STATUS=200 PATH=/x` — the fixture could not attribute the surviving line to a
probe, and any regression that MOVED the keep between probes passed GREEN.

**The change (10 files, +156/−54).** A distinct `path:` per probe, and both route
tables widened from `match: { path: "/x" }` to `match: { prefix: "/" }` so the new
paths still route:

| fixture | probe | path | verdict |
|---|---|---|---|
| `0084` | 1 — no `x-a` (the D1 cell) | `/absent` | DROPPED |
| `0084` | 2 — `x-a: v` | `/valmatch` | DROPPED |
| `0084` | 3 — `x-a: zzz` | `/valmiss` | **KEPT** |
| `0085` | 1 — `x-a: v` (the D2 cell) | `/present` | DROPPED |
| `0085` | 2 — no `x-a` | `/absent` | **KEPT** |

`%REQ(:PATH)%` was ALREADY in the format string and `:path` IS one of the seven
names on `REQ_ALLOW_LIST`, so the kept line becomes self-identifying at **zero**
runtime cost — unlike the gating header `x-a`, which is BOOT-FATAL to echo. The
KEPT probe is still LAST in both fixtures, so the driver's ordering-aware
`suppression_settle` still charges the cheap 2 s `CF70_3_SETTLE` rather than the
12 s `CF71_1_SETTLE` (it inspects only `probes.last()`). `expected_logged_count`
is unchanged at **1** on both. **No fixture was weakened**: every probe, its
headers, its `expect_logged` value and the probe ORDER are all preserved.

Also corrected at the same sites, per I-1's secondary point: `0084`'s claim that
probe 2 is *"the control that proves the matcher is live at all, so probe 1's
silence is attributable to the ABSENCE rule and not to a dead filter."* That is
backwards for the XOR-drop class — with probe 2 REMOVED that regression would give
envoy-rust 0 lines against an expected 1 and go RED; probe 2 is what converts the
RED into a GREEN. Its stated purpose is in any case already discharged by the line
COUNT alone (an always-log filter yields 3 lines, an always-drop 0). The text now
says probe 2 covers the value-MATCH half of the XOR.

### The TDD RED — MEASURED, in this session's own scratch worktree

Per memory `state3-reentry-fixes-are-characterization-pins-red-via-mutation`, a
re-entry fix that strengthens a characterization pin takes its RED from a mutation.
`REVIEW.md` §10 specifies exactly which: the two mutations of its §2.2/§2.3, which
must now go RED. The worktree was created **`--detach` at HEAD** so no parallel
agent's `git checkout` could silently revert a mutation (memories
`mutation-checks-collide-with-parallel-subagents`, `worktree-subagents-get-stale-base`):

```
$ git worktree add --detach <scratch>/re75-mut 29055a5086c1fac2eb90e27f4bb523f0cd19d183
Preparing worktree (detached HEAD 29055a5)
$ git rev-parse HEAD
29055a5086c1fac2eb90e27f4bb523f0cd19d183
$ git status --porcelain
(clean)
```

**Mutation P — polarity inversion.** `crates/envoy-config/src/matcher.rs:50`,
`v.is_some() == *want_present` → `v.is_some() != *want_present`. This inverts
exactly the rule fixture `0085` exists to witness.

```
$ grep -c 'Compiling envoy-config' build-mutP.txt
1
$ cargo test -p envoy-config --lib
test result: FAILED. 642 passed; 7 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

##### PRE-FIX (fixture as shipped at 29055a5, path: /x on every probe) #####
$ cargo test -p differential --test headermatcher_absence_accesslog_present_polarity
test headermatcher_absence_accesslog_present_polarity ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.16s

##### POST-FIX (distinct paths copied in; mutation UNCHANGED and re-verified present) #####
$ sed -n '50p' crates/envoy-config/src/matcher.rs
            (HeaderMatcherMode::PresentMatch(want_present), v) => v.is_some() != *want_present,
$ cargo test -p differential --test headermatcher_absence_accesslog_present_polarity
thread 'headermatcher_absence_accesslog_present_polarity' (504612) panicked at tests/differential/tests/headermatcher_absence_accesslog_present_polarity.rs:61:10:
fixture green: access log byte-exact mismatch: line 0 not byte-identical: envoy="STATUS=200 PATH=/absent" envoy-rust="STATUS=200 PATH=/present"
envoy lines: ["STATUS=200 PATH=/absent"]
envoy-rust lines: ["STATUS=200 PATH=/present"]
test headermatcher_absence_accesslog_present_polarity ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.14s
```

**GREEN → RED, and the failure text is exactly the predicted mechanism:** one line
on each side (so the COUNT assertion still passes — this is precisely the class the
count could never catch), and the byte compare fails because the surviving line now
NAMES a different probe on each proxy.

**Mutation X — drop the `invert_match` XOR.** `matcher.rs:69`,
`mode_result ^ self.invert_match` → `let _ = self.invert_match; mode_result`.

```
$ grep -c 'Compiling envoy-config' build-mutX.txt
1
$ cargo test -p envoy-config --lib
test result: FAILED. 645 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

##### PRE-FIX #####
$ cargo test -p differential --test headermatcher_absence_accesslog
test headermatcher_absence_accesslog ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.13s

##### POST-FIX #####
thread 'headermatcher_absence_accesslog' (511283) panicked at tests/differential/tests/headermatcher_absence_accesslog.rs:63:10:
fixture green: access log byte-exact mismatch: line 0 not byte-identical: envoy="STATUS=200 PATH=/valmiss" envoy-rust="STATUS=200 PATH=/valmatch"
envoy lines: ["STATUS=200 PATH=/valmiss"]
envoy-rust lines: ["STATUS=200 PATH=/valmatch"]
test headermatcher_absence_accesslog ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.13s
```

**The RED matrix, all four cells measured in one worktree:**

| mutation | fixture | PRE-fix | POST-fix |
|---|---|---|---|
| P — `v.is_some() != *want_present` | `0085` | **GREEN** (7 in-process RED) | **RED** — `/absent` vs `/present` |
| X — XOR dropped | `0084` | **GREEN** (4 in-process RED) | **RED** — `/valmiss` vs `/valmatch` |

**The UNMUTATED control, from the SAME worktree** — required, because a RED that
never reached an assertion is not evidence (memory `mutation-red-needs-unmutated-control`;
both REDs above quote a real assertion message, not a startup failure):

```
$ git checkout -- crates/envoy-config/src/matcher.rs
$ git status --porcelain
 M tests/fixtures/0084-headermatcher-absence-accesslog/envoy-rust.yaml
 M tests/fixtures/0084-headermatcher-absence-accesslog/envoy.yaml
 M tests/fixtures/0084-headermatcher-absence-accesslog/expectations.yaml
 M tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/envoy-rust.yaml
 M tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/envoy.yaml
 M tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/expectations.yaml
$ grep -c 'Compiling envoy-config' build-restored2.txt
1
$ cargo test -p envoy-config --lib
test result: ok. 649 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
$ cargo test -p differential --test headermatcher_absence_accesslog --test headermatcher_absence_accesslog_present_polarity
test headermatcher_absence_accesslog ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.13s
test headermatcher_absence_accesslog_present_polarity ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.25s
```

`649 passed` restored from `642`/`645` — the mutations were genuinely present and
genuinely reverted, and the strengthened fixtures are GREEN on an unmutated engine.
The `Compiling envoy-config` count of 1 on EVERY build proves no run was served
from a stale binary (memory `mutation-check-needs-forced-rebuild`).

**The same two fixtures, re-run in the MAIN tree after the fix landed there:**

```
$ cargo build -p envoy-bin        # required: the harness runs target/debug/envoy-bin
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.48s
$ cargo test -p differential --test headermatcher_absence_accesslog --test headermatcher_absence_accesslog_present_polarity
test headermatcher_absence_accesslog ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.21s
test headermatcher_absence_accesslog_present_polarity ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.12s
```

The scratch worktree was then removed with `git worktree remove --force`. **Only my
own worktree was removed** — the four `.claude/worktrees/agent-*` belonging to the
parallel workstream were left untouched, verified by `git worktree list` afterwards.

## I-2, I-3, I-4, M-1..M-4 — the documentation fixes

**Every `BEHAVIOR_CONTRACT.md` site was re-derived by TEXT ANCHOR, never by the
line number `REVIEW.md` quotes.** Those numbers are valid at `1f05c2d` only, and
each fix shifts the ones below it — the trap that bit twice mid-session at state-3.
Each edit was located with a `grep -n` for its own unique text immediately before
being applied.

- **I-2** — the §C caption claimed *"every cell now matches the upstream column"*
  while rows **s5** and **s6** of that same nine-row table carry `*(boot-fatal)*`
  and are the OPEN CF-72-2 reject-direction gaps. Scoped to *"Every cell that RUNS
  on both proxies now matches the upstream column; rows s5 and s6 stay
  `*(boot-fatal)*` here"*, with the CF-72-2 pointer.
- **I-3** — the new `contains_match` bullet cited the WRONG source site and
  endorsed a rationale its own measurement refutes. Both halves fixed. The bullet
  now points at `StringMatcherMode::Contains` (line-number-free) and states that
  the in-source comment is SUPERSEDED on one point. **The in-source comment itself
  was corrected**, in `crates/envoy-config/src/bootstrap.rs`: it claimed *"Envoy
  v1.33.0 only supports Contains via the modern string_match field"*, which phase
  75 MEASURED to be false — upstream v1.33.0 DOES accept a top-level
  `contains_match`, with a deprecation warning. The in-tree restriction is a
  deliberate ADR-0049 fail-loud choice, not an upstream limitation. The sibling
  `StringMatch` variant's "(the only path to Contains)" was narrowed to "the only
  IN-TREE path" for the same reason. **This is the ONLY `crates/` edit in this
  re-entry and it is COMMENT-ONLY** — proved by filtering the diff:

  ```
  $ git diff -U0 crates/envoy-config/src/bootstrap.rs | grep -E '^[+-]' \
      | grep -vE '^(\+\+\+|---)' | grep -vE '^[+-]\s*///'
  (no output)
  $ git diff --stat crates/
   crates/envoy-config/src/bootstrap.rs | 17 ++++++++++++++---
   1 file changed, 14 insertions(+), 3 deletions(-)
  ```

- **I-4** — `three` → `four` at the phase-72 `**§C` site, with the two independent
  measurements named (`75.1/PROGRESS.md` `56 passed; 4 failed`; `75.2/PROGRESS.md`
  `645 passed; 4 failed` against a `649 passed` control) and the reason the stale
  `three` existed (a PLAN-write pre-flight taken BEFORE the fourth guard was added;
  ADR-0162 records the correction). **The secondary hazard was also closed**: a new
  paragraph states explicitly that the RED set is NOT the adjacent four-name
  PINNING list — the pinning list includes
  `pv4_value_matcher_absent_plus_invert_dropped_is_parity_with_upstream`, which the
  hoist does NOT break, and omits `present_match_false_matches_when_absent`, which
  it does — and routes the reader to Phase 75 §D, which names all four RED tests.
- **M-1** — both new READMEs said *"All three are BANKED in
  `BEHAVIOR_CONTRACT.md`"*. Re-verified on disk this session:
  `grep -c 'CF-75-2' docs/envoy-rust/BEHAVIOR_CONTRACT.md` → **0**;
  `grep -c 'CF-75-2' docs/envoy-rust/STATE.md` → **5**. Both READMEs now say
  CF-72-2 and CF-75-1 are banked in the contract while **CF-75-2 is not in that
  file at all** — it is an open carry-forward recorded in `STATE.md` that needs its
  own measured phase, and it is not a regression because the PRESENCE axis these
  fixtures pin is parity.
- **M-2** — `0085`'s README and its test entrypoint claimed the D2 cell had *"NO
  behavioral test anywhere in the tree"* before phase 75. It had two, and they
  ASSERTED the divergence — `present_match_false_returns_true_when_present` and
  `present_match_false_returns_true_when_absent`, still readable at
  `git show f68b160^:crates/envoy-config/src/matcher.rs`. Both sites now say the
  only in-tree tests of the cell ASSERTED the divergence and that there was no
  CROSS-PROXY witness anywhere, which is the materially more interesting fact and
  the one this fixture actually supplies.
- **M-3** — `~24 probes` → **22** for fixture `0083` at the Phase-75 §G site, where
  the same file already said 22 four hundred lines earlier. Re-verified:
  `grep -c '^    - name: p' tests/fixtures/0083-headermatcher-absence-parity/expectations.yaml`
  → **22**.
- **M-4** — the one M74-31 rewrite that still over-attributed byte-DISTINCTNESS to
  probe PLACEMENT. Byte-distinctness (`M=-` vs `M=1`) is a property of the rendered
  lines and holds at any probe order; placement SECOND buys only the ORDER. Now
  reads *"The two kept lines are byte-DISTINCT (`M=-` vs `M=1`) whatever the probe
  order; what placing it SECOND buys is the specific ORDER…"*.

**§G was additionally given a standing rule**, because the defect I-1 found is a
property of the DRIVER and will recur: a new paragraph records why every probe in an
`http1_access_log_byte_exact` fixture must carry a distinct `path:`, quotes both
measured pre-fix results, and instructs that the same discipline apply to any future
fixture on that driver.

## M-5 — DISCLOSURE, plus one NEW fact the review did not have

`REVIEW.md` §10 asks this to be closed by disclosure rather than by re-running
anything. The anomaly: at the state-3 mutation worktree's base commit `3b44510` the
two mutated lines sit at **50** and **54**, yet mutation A2 — described as deleting
the keyword `return`, a pure in-place edit that cannot move its own line — is quoted
at `57:` (+3), and mutation B's replacement is quoted at `53:` where the arm it
replaces is at 50 (+3).

**The NEW fact, verifiable from the record itself.** Mutation B's quoted line

```
53:                if *want_present { v.is_some() } else { true }
```

carries **16 spaces of indentation**, whereas the arm it replaces —

```
50:            (HeaderMatcherMode::PresentMatch(want_present), v) => v.is_some() == *want_present,
```

— carries **12**. A deeper indent means B's replacement was a MULTI-LINE arm, not
the one-liner the narrative implies. A four-line arm replacing a one-line arm is
exactly `+3` lines, which reconciles **both** quoted numbers — B's own `if` landing
at 53 and everything below it, including `(_, None)`, shifting from 54 to 57 —
**without** requiring the two mutations to have coexisted.

**This is an explanation, not a proof.** It is consistent with the record but the
worktree was removed, so the mutated file cannot be recovered and the alternative
reading (A2 and B present simultaneously, contradicting *"The worktree was restored
to pristine between mutations"*) cannot be formally excluded. **The load-bearing
conclusion survives either reading and was re-verified from source this session:**
mutation B touches only the `PresentMatch` arm, which `0084` never exercises (it
uses `exact_match`), and mutation A2 touches only the `(_, None)` arm, which `0085`
never reaches (the `(PresentMatch(want), v)` arm matches ANY `v`, `None` included,
and sits first). Each fixture's RED is therefore attributable to exactly one
mutation under either reading. **Recorded here so a future reader meets the
discrepancy in the record rather than discovering it.**

## M-6 — CLOSED by fresh evidence, not by family membership

`REVIEW.md` §10 offers two ways to close this: quote the missing failure text from a
fresh isolation run, or state plainly that the test was adjudicated by family
membership. The first was chosen — it is one cheap isolation run and it converts a
pattern-match into evidence. `access_log_rcd_upstream_reset` was the one of five
gate-(b) REDs whose panic text the state-4 record never quoted:

```
$ cargo test -p differential --test access_log_rcd_upstream_reset
thread 'access_log_rcd_upstream_reset' (533946) panicked at tests/differential/tests/access_log_rcd_upstream_reset.rs:33:10:
fixture green: access log byte-exact mismatch: line 0 not byte-identical: envoy="{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:39421}\",\"rf\":\"UF\"}" envoy-rust="{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}"
envoy lines: ["{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:39421}\",\"rf\":\"UF\"}"]
envoy-rust lines: ["{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}"]
test access_log_rcd_upstream_reset ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.71s
```

This is the documented `TcpCloseBackend` IPv6-unreachable **host** flake, verbatim:
upstream Envoy cannot reach the host-spawned close backend at
`[fdc4:f303:9324::254]`, reports `immediate_connect_error:_Network_is_unreachable`
and `rf: UF`, where envoy-rust sees a plain `connection_termination` / `rf: UC`.
It is an environment divergence on the reference side, not an envoy-rust defect —
and it fails DETERMINISTICALLY in isolation, which IS the signature for this family
(the startup-race family behaves the opposite way). The same binary is GREEN in CI.
**All five gate-(b) REDs now have their failure text on the record.**

## N-1, N-2 — fixed. N-3, N-6, N-7 — disposition stated

- **N-1 (fixed, both READMEs).** The `generate_request_id` per-side divergence row
  explained the difference by consequence (*"envoy-rust does not emit request-ids
  here"*) rather than by cause. Verified on disk: `HttpConnectionManagerConfig` is
  `#[serde(deny_unknown_fields)]` (`bootstrap.rs:1100`) and
  `grep -c 'generate_request_id' crates/envoy-config/src/bootstrap.rs` → **0**, so
  writing the field on the rust side would be BOOT-FATAL, not inert. Both rows now
  say so. This matters: the old wording invited a future fixture author to add the
  field to the rust side.
- **N-2 (fixed).** The Phase-75 block's *"13-probe … ROUTE matrix (7 matcher modes ×
  invert polarity × {…})"* is a loose factorization (7 × 2 = 14) sitting directly
  under a sentence naming fixture `0083`, inviting the reader to think it describes
  `0083` (22 probes). It now says the 13 probes were a hand-picked slice of the
  state-0/state-2 RECON, "not the full cross product", and states explicitly that
  the recon matrix is NOT fixture `0083`, pointing at §G for the shipped fixtures.
- **N-3 (no action).** CF-75-1's scope note says the residual divergence is
  "confined to the PRESENT-value cells, both polarities"; the present-but-EMPTY-value
  cell is parity. The parenthetical "(the middle row above)" already disambiguates,
  so this is presentation only and the text is not wrong.
- **N-6 (no action, stated).** `PLAN.md` says "8 tasks" against a plan defining
  `### Task 1` … `### Task 9`. Excluding Task 9 (`PROGRESS.md` itself) from the LoC
  table is defensible; carrying that exclusion into the task-count gate is not.
  **Not corrected**: `PLAN.md` is this sub-phase's landed planning record, 8 and 9
  both clear the ~25 §6.1 gate comfortably, and there is no reader decision that
  turns on it. Recorded here so the discrepancy is not later read as an oversight.
- **N-7 (closed by declaration).** The third elision class — abbreviated COMMAND
  lines — is now declared in this section's own header, together with the other
  two. The earlier sections are left as written (they were accurate about output
  and are a landed record).

## M-7, M-8, N-4, N-5 — NO ACTION, as `REVIEW.md` §10 directs

- **M-7.** `ROADMAP.md` row `75.2` still carries the "five-site" M74-31 figure that
  ADR-0161 correction C4 refuted to FOUR. **Deliberately left.** `ROADMAP.md` is
  append-only under `BOOTSTRAP_PROMPT.md` §4.1 invariant 2 ("only update status and
  sub-phases columns"), and the refutation is durably recorded in ADR-0161,
  `PLAN.md` and this document. The review recorded the decision explicitly so the
  state-6 close-out does not re-litigate it; this entry carries it forward.
- **M-8.** The ADR-0035 orphan — the state-4 commit rewrote the `### Doctrine
  reminders` §5.1 bullet without first relocating its prior text — was ALREADY
  REPAIRED by the state-5 review itself, losslessly, by appending two archive
  sections to `STATE_HISTORY.md` (39 insertions, 0 deletions). Nothing further to
  do. **The forward obligation stands and was honoured this session: the ADR-0035
  delta check below was run against the FULL superseded set INCLUDING that bullet.**
- **N-4 and N-5.** Both are stale-but-true-when-written historical statements.
  Retroactively editing them would violate D-3.5; the CURRENT measured figures are
  carried forward in `STATE.md` instead.

## What this session did NOT do

- **Did NOT re-run the §7.5 gate.** That is the state-4 RE-VERIFICATION, a separate
  session. The only test runs here were the two touched fixtures, the
  `envoy-config` unit suite (as the mutation instrument), one isolation run for
  M-6, plus `cargo fmt --all -- --check` (exit 0) and `cargo build -p envoy-bin`
  (exit 0) as a build sanity check on the comment-only `crates/` edit.
- **Did NOT change any `crates/` behavior.** The single `crates/` edit is
  comment-only, proved above by a filtered diff that prints empty.
- **The commit is FIFTEEN files.** TWELVE carry the fixes (six fixture config
  files, two fixture `README.md`s, two test-entrypoint doc-comments,
  `BEHAVIOR_CONTRACT.md`, `bootstrap.rs`); the other three are this `PROGRESS.md`
  and the `STATE.md` / `STATE_HISTORY.md` ledger pair.
- **Did NOT touch** `ROADMAP.md`, the frozen `75/SPEC.md`, any `75.1/` artifact,
  `ci.yml`, any fuzz target or corpus seed, `known-failures.txt`, or any of the
  other 83 fixtures.
- **Did NOT widen into CF-75-3 or CF-75-2.** Both remain open carry-forwards owned
  by their own future phases.
- **Did NOT flip** ROADMAP row `75.2` or parent row `75`; both stay `in-progress`.
- **Did NOT fire an ADR.** No genuinely new decision arose: every fix was already
  licensed — I-4 by ADR-0162's own title, the rest by `REVIEW.md` §10. Ledger head
  remains **ADR-0163**, next available **ADR-0164**.

## Next

**Sub-phase 75.2 §5 state-4 — the RE-VERIFICATION** (`superpowers:verification-before-completion`),
in a SEPARATE session (§5.1; ADR-0127). It re-runs the full §7.5 gate (a)–(e) over
the strengthened fixtures. After it lands: **state-5 RE-REVIEW** → **state-6
CLOSE-OUT**, at which ROADMAP row `75.2` AND parent row `75` both flip `done`. Each
is its own session. **This session did not chain into any of them.**
