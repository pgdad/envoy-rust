# Phase 113 — the `%GRPC_STATUS%` access-log command-operator family — CODE REVIEW

> **§5 state 5.** This document is the state-5 output for phase `113` and it **CLOSES §7.5 gate (f)**,
> the only gate the state-4 session left open. Written by a THIRD context: the state-3 session
> implemented, the state-4 session graded, and neither may review (§5.1; `ADR-0127`).
>
> **Verdict: APPROVED.** §2 (Issues — Must Fix) is **EMPTY**, so the state machine advances to
> **state 6**, not back to state 3 (§5.2).
>
> **This review wrote no code, no test and no fixture, and edited no landed artifact.** The CI
> identity must therefore stay at `binaries=169 passed=2298 failed=0`. Every finding below is
> **BANKED** as a carry-forward, not fixed (§6.3; `ADR-0165`: a phase banks, it never clears — and
> a REVIEW banks its own findings too).
>
> **Twenty-eight findings: 6 Important, 22 Minor, opening `CF-113-7` … `CF-113-13`.** `ADR-0195`
> fires: the verdict and the banked set are decisions. **No landed NUMBER that carries a decision
> was contradicted** — net 1165, the 1092 table summing exactly, 12 probes, 11 tracked seeds, 17
> table rows, 14→15 `Op` variants, 19→20 record fields, `44 0` on `DECISIONS.md` and all five PV-8
> paths all reproduced exactly. But **one landed count is off by one** (`PLAN.md:60`'s "9 tasks"
> against ten `## Task` headings, inherited into landed `ADR-0193`), **one landed cross-reference
> points at a measurement that says the opposite** (§J → `ADR-0154`), **one landed ADR presents a
> paraphrase as a verbatim quotation** (`ADR-0193` D2), **one self-referential count was invalidated
> by the very commit that wrote it** (the "misleading 4", now 8), and **this phase introduced one
> regression to a pre-existing artifact** (a doc-comment hijack in `envoy-http2/src/hcm.rs`).

---

## §0 — How this review was conducted

### §0.1 — Scope

The review surface is the **1165 net code lines** between base `1ff03ba4` (the state-2 CI record,
parent of Task 1) and HEAD `9e6580d9fac34e3b9fb82287d6e8d9252abd631e` (the state-4 CI record).
Re-derived at this session with `git diff --numstat 1ff03ba4 HEAD -- . ':(exclude)docs/'`
(⚠ in `--numstat` the PATH is field `$3`, not `$2`):

| file | + | − | net |
|---|---|---|---|
| `crates/envoy-accesslog/src/command_operator.rs` | 366 | 2 | 364 |
| `crates/envoy-accesslog/src/json_format.rs` | 85 | 0 | 85 |
| `crates/envoy-accesslog/src/record.rs` | 31 | 5 | 26 |
| `crates/envoy-accesslog/src/file_sink.rs` | 1 | 0 | 1 |
| `crates/envoy-accesslog/fuzz/.gitignore` + 3 corpus seeds | 6 | 0 | 6 |
| `crates/envoy-http1/src/hcm.rs` | 133 | 0 | 133 |
| `crates/envoy-http2/src/hcm.rs` | 27 | 0 | 27 |
| `tests/differential/tests/accesslog_grpc_status.rs` | 25 | 0 | 25 |
| `tests/fixtures/0093-accesslog-grpc-status/` (4 files) | 498 | 0 | 498 |
| **TOTAL** | **1172** | **7** | **1165** |

Plus, excluded from the §6.1 gate: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` `138 0`,
`DECISIONS.md` `44 0`, `STATE.md` `29 16`, `STATE_HISTORY.md` `72 0`, `PROGRESS.md` `1210 0`.

**1165 against the PLAN's MEASURED 1092 is 1.07×**, re-derived here and matching `ADR-0194`
DECISION 4 exactly. The §6.1 gate is ~1500, so it clears by 335 lines / 22%.

**Document precedence, later wins:** `SPEC.md` < `PLAN.md` < `PROGRESS.md`/`ADR-0194`.
`ADR-0193` corrects four landed `SPEC.md` claims; `ADR-0194` corrects the PLAN. This review
adjudicates against the LATEST binding statement of each claim, not against the SPEC.

### §0.2 — Method

Six read-only reviewers were fanned out over the independent dimensions of the surface — the
`envoy-accesslog` parser/render/JSON, the H1 population site and its gate tests, the H2 boundary
and the two declared absences, fixture `0093`, the `BEHAVIOR_CONTRACT.md` §I-§L text, and a
citation/carry-forward audit. Each was given full zero-context instructions (D-3.4), told **not**
to run `cargo` (the gate is discharged and the lock serializes), told not to mutate the tree, and
told that a positive control must precede any reported zero.

⚠ **A SUBAGENT FINDING IS A CLAIM.** Every finding below was re-verified on disk by this main
session before being recorded. **Two were DOWNGRADED** (the negative-`grpc-status` divergence, which
rested on unverifiable recall of upstream C++; and "the H2 helper is dead weight", refuted by a
call-edge grep showing a real production call site), **one was NOT CHARGED** to this phase (a
pre-existing `uring.rs` doc defect), and **one briefing claim was corrected** (the SHA audit's
"two known-false MISSINGes" is one). All four are adjudicated in §5.

The doc-comment hijack (F-1) was found **independently by this session and by two of the
reviewers** — three separate paths: a diff read, a base-vs-HEAD `git show` comparison, and a
call-edge grep. That is corroboration rather than agreement, because the three did not share a
method. ⚠ By contrast, where several artifacts agree because they descend from one ancestor — as
`SPEC.md`, `PLAN.md` and `BEHAVIOR_CONTRACT.md` §J all do on the 17-code table — that is **not**
corroboration, and F-3 says so.

⚠ **One of this session's own greps mis-fired and is recorded rather than quietly fixed.** Checking
whether any test pins the empty-`()` scope limit, a grep scoped to `crates/` returned **3** hits and
appeared to refute the reviewer's zero. All three were untracked, **gitignored fuzzer-generated
corpus artifacts** left on disk by the state-4 gate's fuzz run — not source, not tests. Re-scoped to
`crates/*/src/` + `tests/`, the count is **0** against a positive control of **120** files
containing `%REQ(`. The finding stands; the first measurement was wrong for exactly the reason this
project's own standing traps name.

### §0.3 — The §7.5 gate was NOT re-run

Legs (a)-(e) were discharged at the state-4 gate (`a4369826`) and are quoted with real command
output in `PROGRESS.md:944-1177`. Re-running them here would burn a Docker corpus sweep to
reproduce a result already recorded, and §5.1 makes the grading context and the reviewing context
distinct precisely so the review reads the evidence rather than regenerating it. This review
**inherits legs (a)-(e) as data and re-verifies them as claims** — by re-deriving the figures they
rest on from disk, not by re-executing them. Leg (f) is this document.

What this session DID re-derive from disk, independently:

| claim | source | re-derived here |
|---|---|---|
| net code lines | `ADR-0194` D4: 1165 | **1165** (added 1172 / deleted 7) ✅ |
| PV-8 — five untouchable paths | `PROGRESS.md:762`, `:1002` | `git diff --numstat 1ff03ba4 HEAD --` on `Cargo.toml`, `Cargo.lock`, `deny.toml`, `ci.yml`, `tests/differential/src/lib.rs` → **empty** ✅ |
| no `#[allow(dead_code)]` added | `ADR-0194` D1 | scoped to `crates/`+`tests/` → **0**, against controls 1187 `+` lines / 35 `fn ` / 27 `#[` ✅ (unscoped reads **8**, all ADR/STATE/PROGRESS prose — the handoff's figure of 4 was measured before the state-4 gate added more prose) |
| the 15 tree-wide `allow(dead_code)` are pre-existing | — | none appears in this phase's diff ✅ |
| fuzz seeds tracked and un-ignored | `PROGRESS.md:638` | `git ls-files` → **11**; plain `git check-ignore` → not-ignored on all three ✅ |
| gate (d) has a CI step | `PROGRESS.md:1127` | `ci.yml:78/122/127` name `accesslog_format_parse` ✅ |
| ADR head / next free | `ADR-0195` free | highest = **0194**, 190 distinct ADRs (191 `^## ADR-` matches − the line-10 schema template) ✅ |
| 12 probes, distinct paths | `SPEC.md:160` | 12 `path: /g-*`, each occurring **exactly once** ✅ |
| the 17-code table | `ADR-0193` D8 | 17 rows, both spellings ✅ |
| SHA audit | — | 3 unique 40-hex strings; **1 MISSING**, and it is the known Docker-digest truncation artifact `56da5afd…`, not a defect ✅ |
| ADR-0193's four SPEC corrections | `ADR-0193` | each verified against the landed SPEC original it charges ✅ |
| `DECISIONS.md` additions only | `44 0` | **44 added, 0 deleted** — no landed ADR was edited ✅ |

⚠ **One briefing claim was CORRECTED here.** The handoff warns that the SHA audit reports **two**
known-false `MISSING`es. Over the audited set (`STATE.md` + the phase directory) there is exactly
**one**: `b0f43d67aa25c1b03c97186a200cc187f4c22db3` does not occur in either — it lives in
`ENVOY_TARGET.md`, phases 18/19/33, `DECISIONS.md` and `STATE_HISTORY.md`, all outside the audited
set. The warning is right that it is not a defect; it is wrong that it appears here.
| Task-7 ledger correction | `PROGRESS.md:1164` | present and correct — Task 7 is `c9dfb50`; eleven-commit count unaffected ✅ |
| ROADMAP untouched | `PROGRESS.md:1185` | row `113` still `planned` at line 195 ✅ |

**Every one reproduced.** No figure in the record was contradicted by this review.

---

## §1 — Strengths

These are specific, and each was checked rather than accepted.

1. **The gate is a REUSE, not a re-implementation, and that is the single best decision in the
   phase.** `crates/envoy-http1/src/hcm.rs:1764` calls `crate::grpc::is_grpc_request(&request.req.headers)` —
   literally the same `pub(crate)` predicate the phase-110 local-reply transform gates on
   (`crates/envoy-http1/src/grpc.rs:45`). Producer and logger therefore **cannot drift apart**. I
   checked the predicate against all six of `SPEC.md` §2.2's measured upstream rows and it
   reproduces every one: exact `application/grpc` and the `application/grpc+` prefix accept;
   `; charset=utf-8`, `-web`, `APPLICATION/GRPC` and absent all reject.

2. **`ADR-0193` DECISION 2 is a negative result about the phase's own witness, measured and then
   published in three places.** The phase discovered that fixture `0093` **cannot** witness the
   request-side gate, proved it by mutation twice (PLAN-write and again at state 3), and wrote the
   limitation into `README.md:44-66`, `expectations.yaml:156-163` and `BEHAVIOR_CONTRACT.md` §I —
   each naming the sole surviving witness by full test path and telling the reader not to record
   the fixture as coverage. It then generalised it into a reusable rule. A phase that goes looking
   for the way its own fixture is blind, finds it, and then makes it impossible for a reviewer to
   miss, is doing the thing this process exists for.

3. **The record field carries the RAW wire string, not a parsed integer** (`record.rs`,
   `Option<String>`), which is what lets the renderer reproduce upstream's `99` (out-of-enum) and
   `-1` (unparseable) cells at all. Parsing is deferred to render time
   (`command_operator.rs:138`), and the `-1` sentinel is explicitly distinguished from the `-`
   absent sentinel in the contract (§K). That is the correct layering and it was chosen for a
   measured reason.

4. **The empty-`()` relaxation is one line and generalises correctly.**
   `rest.is_some_and(|r| r != "()")` replaces `rest.is_some()`. Special-casing the new keyword
   would have been more code AND would have left the engine internally inconsistent. The structural
   confinement argument is **true as stated** — I read `parse_operator` and confirmed `REQ`,
   `RESP`, `DYNAMIC_METADATA` and `GRPC_STATUS` are matched by exact keyword in arms that precede
   the `other => no_arg_op(other)` arm carrying the relaxation.

5. **`ADR-0194` DECISION 1 chose the honest option under pressure.** Faced with a per-task clippy
   gate that was structurally unmeetable at two boundaries, it deferred the gate rather than adding
   an `#[allow(dead_code)]` that would have to be remembered away. I verified the outcome rather
   than the intent: **zero** `allow(dead_code)` in the phase's `crates/`+`tests/` diff, against a
   real positive control, and clippy green from Task 3 onward.

6. **The mutation discipline is real, and the mutations were aimed.** The canonical-table mutation
   censused its anchor first and found it occurs **2×** (const + the test's own `EXPECT` table),
   then used the `];`-disambiguated single-occurrence form — the exact trap that would otherwise
   have mutated impl and expectation together and read as "vacuous tests". The resulting RED quotes
   upstream's own output on the `envoy=` side, which is the positive control proving a real
   container served the probe.

7. **`ADR-0194` DECISION 5 records three PLAN predictions that did NOT hold**, including one — the
   `5 failures` that were `3` — whose root cause is that two tests assert ABSENCE and pass
   vacuously against a `None` placeholder. It then draws the right conclusion: the gate pin's RED
   evidence must come from a wrong implementation, not a missing one. Recording a prediction miss
   and deriving a method rule from it is worth more than the prediction having been right.

8. **The fixture is cluster-free, backend-free and deterministic.** `clusters: []`, 13
   `direct_response` routes, no timing/ID/duration operator in the format string, per-fixture-unique
   `/tmp` mount dirs, and 12 distinct probe paths so every logged line is attributable. It is
   therefore verifiable on the development host, where backend-routing fixtures go RED against the
   `192.168.65.2` bridge.

9. **The assertion path is real and terminates in a genuine comparator.** The runner selects
   `Driver::Http1AccessLogByteExact`, whose arm reaches
   `assert_access_log_lines_byte_identical` (`tests/differential/src/access_log.rs:305`)
   unconditionally on the success path. This is the failure mode where an expectation block is
   declared but no driver arm consumes it; it does not occur here.

---

## §2 — Issues (Must Fix)

**EMPTY.**

No finding in this review is a correctness defect. Nothing diverges from a measured upstream cell;
nothing makes a fixture pass when it should fail; nothing breaks a gate leg; no behavioural path is
wrong. The twenty findings below are documentation accuracy, verification strength, and coverage
disclosure.

**Therefore §5.2 does NOT fire.** The phase advances to **state 6**, the close-out — it does not
re-enter at state 3.

⚠ One finding (**F-1**) deserves an explicit note on why it is not here. It is a **regression this
phase introduced to a pre-existing artifact** — it destroyed information that existed before the
phase — which is a different and worse class than the inherited-gap findings that make up the rest.
It is nevertheless not a Must Fix: it changes no behaviour, breaks no gate, and its remedy is a
two-hunk mechanical move. Sending the phase back to state 3 to re-run a full implementation and
verification arc for a comment relocation would be disproportionate. It is banked as `CF-113-7`
with the fix spelled out, and it should be taken by the next rider that touches
`crates/envoy-http2/src/hcm.rs`.

---

## §3 — Important

### F-1 — the new H2 helper HIJACKED `finalize_h2_stream`'s doc comment: a ~190-line function is now undocumented and a `{ None }` stub carries a docblock describing someone else's behaviour

`crates/envoy-http2/src/hcm.rs:989-1014`.

`PLAN.md:1057` directed "add immediately above `finalize_h2_stream`'s
`#[allow(clippy::too_many_arguments)]`", and `PROGRESS.md:573` records the anchor was asserted
unique — it is; `grep -c` returns 1. **The anchor was unique and the splice landed exactly where it
was aimed. The aim was wrong.** That attribute sits *below* a 10-line doc comment that has
documented `finalize_h2_stream` since phase 06.2, so inserting above the attribute inserted
*inside* the doc block's scope.

Measured base-vs-HEAD:

```
$ git show 1ff03ba4:crates/envoy-http2/src/hcm.rs | sed -n '989,999p'
/// 06.2 Task 7: factored per-stream finalization — sends the
...
/// `send_data(.., end_of_stream=true)` branch uniformly).
#[allow(clippy::too_many_arguments)]
async fn finalize_h2_stream(
```
```
$ sed -n '989,1014p' crates/envoy-http2/src/hcm.rs      # at HEAD
/// 06.2 Task 7: factored per-stream finalization — sends the
...
/// `send_data(.., end_of_stream=true)` branch uniformly).
/// The `%GRPC_STATUS%` backing value for the HTTP/2 access-log record.   <-- NEW, no blank ///
...
fn h2_grpc_status() -> Option<String> { None }

#[allow(clippy::too_many_arguments)]
async fn finalize_h2_stream(                                              <-- now UNDOCUMENTED
```

**Why it matters.** `h2_grpc_status()` — whose entire body is `None` — now carries a docblock whose
first four lines say it "sends the downstream response via `send_envoy_response`… builds an
`AccessLogRecord` and emits it once per sink." That is actively false, and it sits at exactly the
seam a future phase reads when lifting `CF-113-2`. Meanwhile the real dispatch-ordering contract
("the access-log dispatch lands AFTER `send_envoy_response` returns") is now attached to the wrong
item and will be read as describing the stub.

This is the documented "mechanical splice corrupts doc comments" class. `cargo fmt` will not reflow
it and clippy does not lint it, so all three of the phase's per-task gates passed
(`PROGRESS.md:592-598`) — correctly. Nothing was wrong with the gates; the defect is invisible to
them.

**Fix:** move lines 999-1011 (the helper plus its own doc comment) to *above* line 989, so 989-998
re-attaches to `finalize_h2_stream`. Two hunks, zero behaviour change. Banked as **`CF-113-7`**.

### F-2 — the empty-`()` relaxation's scope limit has NO permanent test, and its only evidence is a probe that was deliberately deleted

`crates/envoy-accesslog/src/command_operator.rs:321-343`.

The relaxation's soundness rests entirely on **dispatch ordering**: `REQ`, `RESP` and
`DYNAMIC_METADATA` are matched by exact keyword before the `other => no_arg_op(other)` arm that
carries `rest.is_some_and(|r| r != "()")`. That ordering is true today — I read it. Nothing
enforces it tomorrow.

`PROGRESS.md:293-315` records how the scope was established: a temporary test
`tmp_scope_probe_empty_parens_does_not_reach_arg_taking_operators` was appended, run green, and then
**reverted**, with `grep -c tmp_scope_probe` re-checked to 0. I confirm it is gone: `tmp_scope_probe`
occurs **0** times under `crates/`.

So the permanent tree contains **no assertion at all** that `%REQ()%`, `%RESP()%` and
`%DYNAMIC_METADATA()%` still reject:

```
$ grep -rc '%REQ()%\|%RESP()%\|%DYNAMIC_METADATA()%' crates/*/src/ tests/     # files with ≥1 hit
0
$ grep -rl '%REQ(' crates/*/src/ tests/ | wc -l                               # positive control
120
```

The permanent guard that DOES exist — inside
`grpc_status_rejects_length_suffix_and_argument_on_the_alias` — pins only `%RESPONSE_CODE(FOO)%`,
i.e. that a **non-empty** argument on a no-arg keyword is still fatal. That is a different property.

**Why it matters.** A natural future "simplification" — hoisting the `rest == "()"` tolerance above
the `match keyword` — silently widens the accept set for three argument-taking operators, and **no
test fails**. `BEHAVIOR_CONTRACT.md` §L states the scope limit as a rule; the tree does not enforce
it.

**Compounding:** the scope limit is also an **envoy-rust-side** measurement, not an upstream one.
The probe asserted what *envoy-rust* rejects. Upstream's behaviour on `%REQ()%` is unmeasured
anywhere in the phase record. The contract defines what *upstream* does, so this cell belongs in a
NOT-MEASURED list until a `--mode validate` run settles it.

**Fix:** three lines appended to the existing test, plus one upstream `--mode validate` run for the
three spellings. Banked as **`CF-113-8`**.

### F-3 — the canonical-table test asserts against a byte-copy of the table it is testing, and 11 of the 17 codes have no cross-proxy witness

`crates/envoy-accesslog/src/command_operator.rs:114-131` (the `GRPC_STATUS_NAMES` const) and
`:1222-1262` (the test's `EXPECT` array).

Measured mechanically — the extracted tuple lists are **byte-identical**:

```
$ sed -n '114,131p' … | grep -oE '\("[A-Za-z]+", "[A-Z_]+"\)' > impl.txt   # 17 rows
$ sed -n '1222,1262p' … | grep -oE '\("[A-Za-z]+", "[A-Z_]+"\)' > test.txt # 17 rows
$ diff impl.txt test.txt
(identical)
```

`grpc_status_canonical_table_all_seventeen_codes` therefore verifies the **lookup mechanism** —
index arithmetic, camel/snake selection, the `Number` path — but **not the values**. The producer
and the assertion share the predicate, so a mis-measured spelling would be asserted against itself
and pass. This is the "fixture is vacuous when the producer shares the predicate" class, in its
in-process form.

The external oracle is thin. Fixture `0093` witnesses **6** distinct in-enum codes cross-proxy
(2, 7, 12, 13, 14, 16), and the phase's mutation proved exactly one of them (16,
`Unauthenticated`). Codes **0, 1, 3, 4, 5, 6, 8, 9, 10, 11, 15** — eleven of seventeen — have no
differential witness in the landed tree. That includes **both cells `ADR-0193` DECISION 8 calls
unguessable**: code 0 rendering `OK` in both spellings, and code 1's `Canceled`/`CANCELLED` one-L /
two-L asymmetry.

**This is a verification-strength gap, not a known-wrong value.** The 0-16 sweep is recorded as
having been run (`ADR-0193` DECISION 8) and I have no reason to doubt the values. But no raw
transcript is banked anywhere — the only artifacts carrying the eleven unwitnessed spellings are
`command_operator.rs`, this phase's `PLAN.md`, and `BEHAVIOR_CONTRACT.md` §J, all written from the
same table. The measurement is asserted rather than auditable.

**Fix (cheap):** bank the 0-16 upstream transcript verbatim in the close-out or a rider, so the
measurement is re-checkable without re-running Docker. Banked as **`CF-113-9`**.

### F-4 — `BEHAVIOR_CONTRACT.md` §J cites `ADR-0154` as corroborating a spelling, where `ADR-0154` measured the OPPOSITE spelling on an adjacent surface

`docs/envoy-rust/BEHAVIOR_CONTRACT.md:967-970`:

> 2. **Code 1 is `Canceled` with ONE `l` in camel but `CANCELLED` with TWO in snake.** The two
>    columns genuinely disagree on spelling. **ADR-0154 already recorded the one-L form as a live
>    trap.**

`ADR-0154` finding 7 (`DECISIONS.md:3632`) actually records, for the `grpc_status_filter` **config
enum**:

> `grpc_status_filter`'s shape was MEASURED (no `min_items`, no uniqueness bound, enum spelling
> **`CANCELED`** one-L — **`CANCELLED` is rejected**; numeric tokens accepted)

Both measurements are correct and they are about **different surfaces**: the access-log SNAKE_STRING
*render* is `CANCELLED` (two Ls); the `grpc_status_filter` config *input* enum is `CANCELED` (one L)
and **rejects** two Ls. §J presents `ADR-0154` as corroboration when, on the all-caps axis, it
measured the opposite.

**Why it matters, concretely.** `CF-113-4` reserves `grpc_status_filter` for a future phase and
`ADR-0183` already flagged `ADR-0154` DECISION 7's premise as stale — so a future session **will**
come back here. A reader who takes §J at face value writes `CANCELLED` into that filter's enum and
upstream rejects it. This is a cross-reference that will actively cost a future phase time.

**Fix:** replace the citation with the explicit conflict — "upstream spells this code differently on
two surfaces: the access-log SNAKE_STRING render is `CANCELLED` (two Ls, measured here), while the
`grpc_status_filter` config enum is `CANCELED` (one L) and rejects `CANCELLED` (`ADR-0154` finding
7). Do not unify them." Banked as **`CF-113-10`**.

### F-5 — the H2 boundary's stated reason is the weaker one AND does not hold for a proxied trailers-only gRPC response — in the code comment and in the contract

`crates/envoy-http2/src/hcm.rs:1001-1004` and `docs/envoy-rust/BEHAVIOR_CONTRACT.md:1035-1037`:

> **HTTP/2 renders the `-` sentinel unconditionally** — the operator has no H2 data path, **because
> the trailer block is not live at the H2 record build.**

The stated obstacle is inadequate in **two independent directions**, found by two reviewers
approaching it separately and both confirmed here.

**(a) It omits the binding reason.** §F of this very contract section already records **`CF-110-1`**:
H2 performs no gRPC local-reply transform at all — H2 owns a separate `synth_h2_*` generator family
(`crates/envoy-http2/src/hcm.rs:1278/:1302/:1325/:1348`), and `apply_grpc_local_reply` has exactly
two production call sites, **both in `envoy-http1`**. So on the H2 *local-reply* surface there is no
`grpc-status` header **or** trailer to read, whatever the trailer ownership does.
`SPEC.md:144` states both reasons; the contract kept only the weaker one. A future phase that fixes
the trailer ownership will find `%GRPC_STATUS%` still renders `-`, because nothing produces it.

**(b) The reason it does state is false for the shape most likely to be logged.** The obstacle
holds for an ok-path gRPC response (HEADERS, DATA, TRAILERS). It does **not** hold for a **proxied
trailers-only** response — HEADERS+END_STREAM carrying `grpc-status` in the initial header block,
which is the standard gRPC *error* carrier. Verified on disk: H2 genuinely proxies upstream
responses (`crates/envoy-http2/src/hcm.rs:354`, and `:397-400` is "the ONLY arm that carries real
trailers"); `resp.headers` is cloned into `response_headers_for_log_owned` at `:1107` — **before**
the `trailers` move at `:1110` — and that borrow is still live at the record build, used at `:1182`.
So `grpc-status` from a trailers-only response *is* available exactly where the code says it is not.

**Why it matters.** The next phase inherits a false difficulty estimate: it is told it must fix an
ownership problem, when for the local-reply surface the real blocker is a missing transform and for
the proxied surface there is no blocker at all beyond a `pub(crate)` re-export. And the false
version is in `BEHAVIOR_CONTRACT.md`, which invariant 4.1.5 makes canonical and D-3.3 makes the only
permitted authority on equivalence.

**This does not change the verdict on `CF-113-2` itself.** Deferring the H2 arm is correct: the
phase is chartered HTTP/1.1-only, the fixture surface is local-reply, and the H1 arm is itself
local-reply-only (`CF-113-3`). The scoping decision is sound; the recorded *rationale* is not.
Banked as **`CF-113-11`**.

### F-6 — `%GRPC_STATUS%` under `json_format` has ZERO cross-proxy witness anywhere in the tree, and the gap is unbanked

Phase 113 landed a full JSON typing contract — `%GRPC_STATUS(NUMBER)%`/`%GRPC_STATUS_NUMBER%` emit
**unquoted numbers** (`5`, `99`, `-1`), the two string formats emit **quoted strings** (including
`"99"` and `"-1"` in the fallback cells), all four are **`null`** when absent, and a multi-segment
leaf leaves the carve-out and renders `"x-"`. Those rules are in `BEHAVIOR_CONTRACT.md` §K and are
pinned in-process by four tests. **Nothing on the wire checks any of them.**

```
$ grep -rl 'GRPC_STATUS' tests/fixtures/ | wc -l
3                     # all three are 0093's README + its two YAMLs
$ grep -rl 'json_format' tests/fixtures/ | wc -l
79                    # positive control — JSON-sink fixtures exist in quantity
$ comm -12 <(GRPC_STATUS dirs) <(json_format dirs) | wc -l
0
```

Quoting and typing is precisely the class that has diverged before, and `ADR-0193` DECISION 7
records that this is **the engine's first operator that is both numeric-typed and `Option`-backed**
— a genuinely new shape requiring a new `number_opt` helper. It is the least-precedented part of the
phase and the only substantial part with no differential witness.

The gap is **not** covered by `CF-113-5` (io_uring) or `CF-113-6` (the gate), and neither the README
nor `expectations.yaml` discloses it — where both are exemplary about the gate limitation. A second
fixture is required: `assert_access_log_lines_byte_identical` takes one log file per side, so a JSON
sink cannot be bolted onto `0093`.

**Fix:** bank the carry-forward and add one sentence to `README.md`'s "what it does NOT witness"
section. Banked as **`CF-113-12`**.

---

## §4 — Minor

### N-1 — the engine-wide `%OP()%` grammar change is filed only under `## gRPC`, and the canonical grammar table is now incomplete

`BEHAVIOR_CONTRACT.md:2157-2162` is the contract's canonical statement of operator forms and still
lists exactly four: `%OP%`, `%OP(ARG)%`, `%OP(ARG):N%`, `%%`. Phase 113 shipped **two new forms** and
added neither: (a) `%OP()%` on a no-arg operator, and (b) the **optional-argument** operator, which
the code's own doc calls "the engine's FIRST operator with an OPTIONAL parenthesized argument".
`grep -n 'GRPC_STATUS' BEHAVIOR_CONTRACT.md` returns hits **only** in `:902-1032` (positive control:
`RESPONSE_CODE_DETAILS` = 8, spread across the file). `PROGRESS.md:795-797` reasons that §L earned
its own heading because "a reader looking for `%RESPONSE_CODE()%` will not find it under a
`%GRPC_STATUS%` typing heading" — correct, and a heading *inside* `## gRPC` does not solve it.
**Two table rows fix this.**

### N-2 — the "Standalone operators (no argument)" list is stale at 8 against a 12-entry table

`BEHAVIOR_CONTRACT.md:2204-2206` names 8 operators; `no_arg_op` now holds **12**. ⚠ **This is
PRE-EXISTING** — it went stale at phases 41/42/43 and phase 113 merely adds a fourth omission
(`GRPC_STATUS_NUMBER`). Recorded so the close-out does not charge it to this phase.

### N-3 — §L generalises from 2 measured samples to 11 operators while presenting the result as MEASURED

`BEHAVIOR_CONTRACT.md:1009-1022`. The measurement listed covers `%RESPONSE_CODE()%` and
`%PROTOCOL()%` — 2 of the 11. `ADR-0193` DECISION 3 lists the same four probes (the two above plus
the two new keywords) and no more. The claim is almost certainly right — it is one shared branch —
but the contract's entire value is that a MEASURED cell can be trusted without re-measuring, and an
inductive step wearing a MEASURED label is the one thing it must not do. One clause fixes it.
The in-process test `empty_parens_are_accepted_on_no_arg_operators` has the same shape: its comment
quantifies over "every pre-existing no-arg operator" (11) while its body exercises 2.

### N-4 — §I says "All four of §B's negative spellings" where §B has NINE

`BEHAVIOR_CONTRACT.md:929-930`. §B (`:685-707`) lists **9** `NO` rows; §I's own table has 4. The
"four" comes from the `ADR-0177` trap set, not from §B. A future session checking gate parity
against §B will believe it covered §B when it covered 4/9 — and the five uncovered spellings include
**both** `starts_with` traps §B itself calls "the point of the rule" (`application/grpcfoo`,
`application/grpc-web+proto`). Reword to "four of §B's nine".

### N-5 — the gate's negative test set never varies the RESPONSE content-type

`crates/envoy-http1/src/hcm.rs:11700-11737`. `gs_header()` builds only a `grpc-status` row, so on
every negative row the response carries no `content-type`. Consequence: the mutation
`is_grpc_request(&request.req.headers) || is_grpc_request(response.headers)` survives the **entire
module** green. Nothing in the tree distinguishes "gate on the request" from "gate on
request-or-response". The mutation is reachable-shaped — a `direct_response` route can set
`content-type: application/grpc` on a reply to a plain request, and upstream logs `-` there
(`SPEC.md` §2.2 row 1). **One row fixes it.**

### N-6 — fixture `README.md:32` claims the fixture "reproduces the sparse map exactly"; it drives 5 of the map's 8 explicit keys

`http_to_grpc_status` has eight explicit keys — `400, 401, 403, 404, 429, 502, 503, 504`. The
fixture drives `200, 400, 401, 403, 404, 501, 503`, i.e. **five** of the eight; `429`, `502` and
`504` are never driven, so deleting them from the `429 | 502 | 503 | 504 => 14` arm keeps `0093`
green. **Mitigating:** all six *distinct output values* (2, 7, 12, 13, 14, 16) are covered, and the
map is exhaustively pinned in-process by a full-`u16` sweep in `grpc.rs`. Documentation over-claim,
not a coverage hole with teeth.

### N-7 — `expectations.yaml:22-24` describes a settle the driver provably never runs

The comment says "The LAST probe being kept means the driver's ordering-aware `suppression_settle`
charges the cheap 2 s CF70_3_SETTLE." The arm computes
`let has_suppression = expected_lines < probes.len();` and enters the settle block only
`if has_suppression`. `0093` has 12 probes and **12** `expect_logged: true` / **0** `false`, so
`has_suppression` is **false** and the block is skipped entirely — the driver's own comment two
lines up says "the all-kept fixtures see ZERO change". Inherited text from the suppression lineage;
`0093` is the first all-kept fixture to copy it. Harmless (the fixture is 2 s faster than advertised)
but it teaches a false rule.

### N-8 — two `envoy-http1` comments were made stale by this phase and not updated

`crates/envoy-http1/src/hcm.rs:1496-1499` says the borrow serves "the **single consumer**
(`extract_upstream_service_time` at the record build)… (it was only ever read **once**,
logging-on)", and `:1640` documents the field as "borrowed **for the upstream-service-time
extract**." There are now **two** consumers — `:1734` and the new `:1765`. The borrow-vs-clone
rationale is unchanged and still correct; only the counts are wrong. A future reader auditing the
borrow will mis-derive the consumer set.

### N-9 — `PLAN.md:60` states its own task count as 9 against ten `## Task` headings, and `ADR-0193` inherited it

`PLAN.md:60`: "**This plan is 9 tasks and MEASURED 1092 net LoC excluding `docs/`.**" The plan
carries **ten** `## Task N` headings. `ADR-0193`'s title ("written as 9 TDD tasks") and DECISION 1
("1092 net over 9 tasks") inherit the figure. ⚠ **The over-claim originates in `PLAN.md`, not in the
ADR** — charge it to the source, not the transcription. It is defensible for the *LoC* measurement
(Task 10 is docs-only — verified: `0b0c7ea` touches only `BEHAVIOR_CONTRACT.md` and `PROGRESS.md`),
but the sentence adjudicates the §6.1 gate, whose task limb counts the plan's tasks. **The
conclusion is unaffected**: 10 against a ~25 gate clears just as comfortably as 9.

### N-10 — a bare `:N` length suffix is rejected with the wrong error variant, and is untested

`%GRPC_STATUS:5%` has no `(`, so the keyword becomes `"GRPC_STATUS:5"`, misses the exact-match arm,
falls to `no_arg_op` → `None` → `UnknownKeyword`, where upstream's condition is
`MalformedArgument`-shaped. **Observably harmless** — `bootstrap.rs` maps every `FormatParseError`
to the same `ConfigError::InvalidAccessLogFormat`, so both spellings are boot-fatal exactly as
upstream. But the test comment claims "a trailing `:N` length is separately fatal upstream" while
the body exercises only the *parenthesized* form. Add `%GRPC_STATUS:5%` and
`%GRPC_STATUS_NUMBER:5%` as `is_err()` assertions.

### N-11 — `json_format.rs:290` uses a `_ =>` wildcard where the two string variants should be named

`match format { GrpcStatusFormat::Number => …, _ => quote_opt(…) }`. A future fourth
`GrpcStatusFormat` would silently be typed as a JSON string instead of failing to compile. Write
`CamelString | SnakeString =>`. The phase relies on exhaustive matching elsewhere as a design
feature (it is what forced both render arms); this one place opts out of it.

### N-12 — `record.rs:252` is a tautological test

`record_grpc_status_defaults_absent_and_carries_the_raw_value` asserts `is_none()` on
`test_baseline()` and then asserts back the literal `"13"` it just wrote into the struct. Zero
behavioural content; it would pass against any struct carrying an `Option<String>` of that name.
Harmless, but a reader could mistake it for coverage of the population gate — which lives in
`envoy-http1`.

### N-13 — the `grpc_status_malformed.txt` fuzz seed exercises only its first construct

The seed is `%GRPC_STATUS(% %GRPC_STATUS(FOO)% %GRPC_STATUS()% %GRPC_STATUS(CAMEL_STRING):5%`, but
`parse_format` returns `Err` at the first operator (unclosed paren) and never parses the remaining
three. It retains mutation value as a seed, but the `(FOO)`, `()` and `:5` branches it names are not
reached *on the seed itself*. Split into three files, or put the terminal error last.

### N-14 — two fixture-README accuracy nits

`README.md:29` says each probe "drives a *different* `direct_response.status`" — probes 5 and 8 are
both `404` (probe 8 varies the *content-type* instead, which the next-but-one sentence does say).
And the paired-config divergence is explained in prose but cites no ADR, where the four deltas are
exactly the house recipe recorded in `ADR-0158` CORRECTION C3 and re-confirmed in `ADR-0161`.
⚠ Several sibling fixture READMEs (`0092`, `0086`, `0012`) also omit the citation, so this is not a
house-convention breach.

### N-15 — a self-referential count that its OWN commit invalidated: the "misleading 4" is 8

`PROGRESS.md:1209-1210` and `STATE.md` both instruct the state-5 reviewer that "an unscoped
`git diff | grep` returns a misleading **4** where the scoped one returns **0**." Measured at three
commits:

```
$ git diff 1ff03ba4 8841ae3 | grep -c '^+.*allow(dead_code)'   → 4    (state-3)
$ git diff 1ff03ba4 a436982 | grep -c '^+.*allow(dead_code)'   → 8    (THE COMMIT THAT WROTE THE SENTENCE)
$ git diff 1ff03ba4 9e6580d | grep -c '^+.*allow(dead_code)'   → 8    (HEAD)
```

`a436982`'s own additions to `DECISIONS.md`/`STATE.md`/`PROGRESS.md` quote the phrase four more
times, so the sentence was false the moment it landed. **The load-bearing half is correct and
unaffected** — the scoped count is **0**, verified in §0.3 against a real positive control. This is
the same class the phase itself documents twice over (a count is only as current as the last edit to
the document carrying it), recurring in the one place that was warning about it. Restate as "scoped
= 0; the unscoped number drifts with every commit that discusses the phrase."

### N-16 — `ADR-0193` DECISION 2 presents a paraphrase as a verbatim quotation

`DECISIONS.md` renders the SPEC's sentence inside quotation marks as *"an implementation that
ignores the gate and always reads the header would go RED on **four of the fixture's twelve
probes**."* The landed `SPEC.md:177` actually reads *"…would go RED on **four probes**."*

```
$ grep -cF 'four of the fixture' SPEC.md      → 0
$ grep -cF 'would go RED on four probes' SPEC.md → 1   (positive control)
$ grep -cF 'four of the fixture' DECISIONS.md → 2
$ grep -cF 'would go RED on four probes' PLAN.md → 1   (the PLAN quotes it VERBATIM)
```

The substance is preserved — four of twelve is arithmetically right — but a landed, uneditable ADR
carries an expanded paraphrase inside quotation marks, and the PLAN proves the verbatim form was
available. ⚠ **This one is NOT inherited**: it originates in the ADR, not in the SPEC.

### N-17 — `PROGRESS.md`'s E0063 table is labelled "re-derived at THIS commit" but carries PRE-edit line numbers

`PROGRESS.md:174-186`. The five sites resolve at `abbe107` (the tree *before* Task 2), not at
`2201c36` (the commit the section documents). Symptom: `record.rs:128` is cited as `test_baseline()`;
at `2201c36` line 128 is a doc-comment line and `fn test_baseline` is at **138**, while at `abbe107`
it is at **127** with its literal at 128. The four `..base` literals cited at `record.rs:166/178/190/205`
are likewise pre-edit (post-edit: 178/190/202/217). **Every number is correct under the pre-edit
anchor — only the label is wrong.** Relabel as "re-derived at the pre-edit tree (`abbe107`)".

### N-18 — two `SPEC.md` self-contradictions, neither corrected forward

(a) `SPEC.md:83` introduces its table as **"Three further cells"**; the table has **four** rows
(`5`, `13`, `99`, `notanumber`). (b) `SPEC.md:228` PV-5's header says "beyond the **six** measured
codes" while its own next sentence lists **seven** (2, 5, 7, 12, 13, 14, 16). ⚠ `ADR-0193` D8 gets
(b) right ("only codes 2, 5, 7, 12, 13, 14 and 16") but **silently repairs the SPEC without flagging
it** — a correction that is invisible to anyone reading the SPEC alone. Both are prose counts
contradicted by their own adjacent tables.

### N-19 — `PLAN.md` contradicts itself on the `envoy-accesslog` test count, and `ADR-0194` missed that the right figure was already there

`PLAN.md:62` states the crate finishes at **129**; `PLAN.md:366` (Task 2 Step 4) predicts **127**.
`ADR-0194` D5(a) correctly calls the 127 "a stale prototype figure" and re-derives 129 by
arithmetic — but does not note that **129 was already stated in the same document 300 lines
earlier**. The correction is right; its provenance claim understates what the plan already knew.

### N-20 — `PROGRESS.md`'s `h2spec_runner.rs:26` citation is off by one

The prose says ":26 returns early with `eprintln!("h2spec_runner: {} — skipping locally")`". Line
**26** is the guard `if e.to_string().contains("h2spec not found") {`; the `eprintln!` is line **27**
and the `return` line **28**. The prose describes the block accurately; only the line lands on the
guard rather than the statement it quotes. Leg (c)'s conclusion is unaffected.

### N-21 — the PV-8 path set is stated as four in four places and five in one

`SPEC.md:147` non-goal 5, `SPEC.md:235` PV-8, `PLAN.md:19` Global Constraints and `ADR-0194`'s
Consequences all name **four** paths; only `PROGRESS.md:999-1005` (the state-4 gate) names **five**,
adding `deny.toml`. **All five are genuinely untouched** — re-measured empty in §0.3 — so this is a
wording inconsistency, not a false claim. The five-path form is the stronger one and should become
the standard.

### N-22 — `ADR-0193`'s headline attributes its own split reservation to itself

The headline reads "`ADR-0193`'s own split reservation is RELEASED"; the reservation was placed **by
`ADR-0192`, on the number 0193**. DECISION 1 states it correctly. Cosmetic.

---

## §5 — Subagent findings adjudicated, and dissent

⚠ **A subagent finding is a claim.** Sixteen reviewer findings were re-verified on disk and
recorded above. The following did **not** survive as stated, or were re-graded.

**DOWNGRADED to an unmeasured cell, not a defect — the negative / `>i64::MAX` `grpc-status`
divergence.** One reviewer proposed that `raw.trim().parse::<i64>().unwrap_or(-1)` diverges from
upstream, which it believed parses into a `uint64_t` — so `grpc-status: -5` would be a parse
failure upstream (`-1`) but parses to `-5` here. **The reviewer explicitly labelled this as recall of
upstream C++ source, which is not on disk in this repository, and it was not measured.** D-3.3
forbids deciding what "equivalent" means by reading Envoy source; the contract is the contract. I
therefore record it **not** as a defect but as an **unmeasured cell** to be settled by a
`--mode validate` / access-log run before it becomes a standing assumption. Reachability is low but
non-zero (a `direct_response` config can set an arbitrary value, which is exactly how `SPEC.md` §2.3
measured). Banked inside **`CF-113-13`**.

**NOT CHARGED to this phase — the `uring.rs` engagement-gate doc.** One reviewer found that
`crates/envoy-http1/src/uring.rs:7-8` claims engagement requires "no access log", while the actual
gate in `envoy-bin/src/main.rs` enforces no such condition — so a listener with an `access_log:`
block plus both env vars silently drops all access logs rather than falling back. I re-verified the
substance and it appears correct. **It is pre-existing, outside phase 113's diff, and outside its
charter** (§6.3; `ADR-0165`). It means `CF-113-5` is a *live* gap rather than one the config gate
forecloses — recorded as context for whoever takes `CF-113-5`, not as a finding against this phase.

**DOWNGRADED — "the H2 named helper is dead weight / over-engineered."** A call-edge grep shows
`h2_grpc_status` has a genuine **production** call site at `crates/envoy-http2/src/hcm.rs:1197`
(`grpc_status: h2_grpc_status(),`), not only test callers — 6 occurrences total, positive control
`finalize_h2_stream` = 10. It is not decoration. I accept the reviewer's narrower point that
`h2_grpc_status_is_absent` is a **tautology** (a test asserting that a function whose body is `None`
returns `None`) and that the real tripwire is structural rather than assertional: a signature change
breaks the test at compile time, and a call-site replacement orphans the function into a
`dead_code` hard error under CI's `-D warnings`. That is a lint, not a test — worth knowing, but the
design choice was still better than an inline `None`.

**REJECTED as a finding, recorded as a method note — this session's own mis-scoped grep.** See
§0.2. My first attempt to check F-2 returned 3 hits and appeared to refute the reviewer; all three
were gitignored fuzzer artifacts. The reviewer was right and my measurement was wrong.

**NOT RE-OPENED — the Task-7 citation.** `PROGRESS.md`'s state-3 ledger row 7 reads
*"(in `f151731`'s successor)"*; the real commit is `c9dfb50`. The state-4 gate already corrected
this forward at `PROGRESS.md:1164-1177` and re-verified the eleven-commit count unaffected. I
confirmed the correction is present and correct and that `c9dfb50` is indeed Task 7. Closed.

**CONSOLIDATED, not double-counted.** Two reviewers reached the H2 rationale defect from different
directions — one that the stated trailer obstacle is *incomplete* (it omits `CF-110-1`), one that it
is *inaccurate* for trailers-only responses. Both verify. They are one finding, **F-5**, with two
limbs, rather than two findings.

**ACCEPTED with a counting nuance — `ADR-0193`'s "FOUR landed `SPEC.md` claims".** The audit found
the headline defensible but incomplete: the four *substantive* corrections are D2 (fixture
non-vacuity FALSE), D3 (the reject set — empty `()`), D4 (one `Op` variant not two) and D5 (E0063
blast radius 5 not 10) — exactly the four `PLAN.md` enumerates. **D9 additionally corrects three
`SPEC.md` citations, which the headline does not count**, and D8 is an *extension* rather than a
correction (verified: `SPEC.md` §2.3 never asserts a spelling for codes 0 or 1). So the count is
**four** if "claims" excludes citations and **seven** otherwise. I record this as a definitional
ambiguity, **not** a finding: the ADR's own body makes the distinction visible.

**Symmetrically for `ADR-0194`'s "three PLAN predictions".** Exactly three predictions did not hold
(D5 a/b/c) ✅. Counting *all* its PLAN corrections gives five (D1's clippy gate and D4's size figure
as well). Both counts are true of different sets; the ADR names the narrower one accurately.

**A REVIEWER'S OWN VERDICT IS ALSO A CLAIM.** Three of the six reviewers returned "With fixes" and
one returned "Yes". Those verdicts are scoped to a single dimension and none of them adjudicates
§7.5; the phase-level verdict is this document's, reached after re-verification, and it is
**APPROVED** because §2 is empty. A dimension reviewer saying "with fixes" about documentation
accuracy is not a Must Fix at phase level.

---

## §6 — Deliberate decisions verified, not re-litigated

These were checked and are **correct as decided**. They are listed so a future session does not
re-open them.

1. **Fixture `0093` does NOT cover the operator's request-side gate**, and this review does not
   record it as doing so. Re-confirmed from the artifacts rather than inherited: `README.md:44-66`
   and `expectations.yaml:156-163` both state it in writing, name the sole witness by full test
   path, and bank `CF-113-6`. The structural argument is coherent given the configs — `clusters: []`
   plus 13 `direct_response` routes means every response is a local reply, and the only writer of a
   response `grpc-status` on that path shares the predicate under test.

2. **ONE `Op` variant, not the SPEC's two** (`ADR-0193` DECISION 4). `%GRPC_STATUS_NUMBER%` is a
   spelling of `%GRPC_STATUS(NUMBER)%`, measured byte-identical upstream in both formats; the
   `no_arg_op` entry constructs the same variant. Verified: the enum has **15** variants and the
   `no_arg_op` table has 12 entries.

3. **No visibility widening** (PV-3, `ADR-0193` DECISION 6). `envoy-accesslog` is a leaf crate and
   cannot call `envoy_http1::grpc` — that is a dependency cycle, not a style preference. The gate
   correctly stays HCM-side. Nothing became `pub`.

4. **The E0063 blast radius is five literals, not the SPEC's ten** (`ADR-0193` DECISION 5), and the
   five were re-derived from disk at Task 2 rather than inherited.

5. **No new fuzz target** — §7.4's trigger is satisfied by the pre-existing
   `accesslog_format_parse`, which covers the parser this phase extends. Adding one would have
   required a `ci.yml` step, and `ci.yml` is on the untouchable list. Correct call; gate (d) has a
   real CI step and three tracked, un-ignored seeds.

6. **PV-8 holds.** `Cargo.toml`, `Cargo.lock`, `deny.toml`, `ci.yml` and
   `tests/differential/src/lib.rs` are untouched across the whole phase — re-measured empty here.
   No new dependency, crate, config surface, driver or fuzz target.

7. **Nothing outside phase 113 was fixed** (§6.3; `ADR-0165`). Every carry-forward stands INTACT:
   `CF-113-1`…`CF-113-6`, `CF-112-1`…`CF-112-19`, the `112.1`/`112.2`/`111`/`110.x`/`109.x`/`108.2`
   REVIEW sets, `CF-111-1`…`CF-111-9`, `CF-110-1`…`9`, `CF-109-1/2/3`, `CF-108-1/2/3`, `CF-76-1`,
   `CF-75-2/3/4/5/6`, `CF-72-2`/`CF-75-1`, `M71-6`, `CF-74-1/2/3/4/6`, `CF-73-1` and the
   HTTP-filters-family (1)-(4). **`CF-111-4` remains consumed only in PART** (the `%TRAILER(name)%`
   half is untouched). **`CF-112-5` stays CLOSED. `CF-112-8` Consequence 2 stays BANKED as
   structurally unwitnessable.** ⚠ `CF-112-13`'s blast radius is **five** internally-tagged unit
   variants, not the four its own REVIEW names. The phase-112 ALPN cleanup remains a **RIDER, NOT a
   phase** at ≈134 net lines (`ADR-0192` DECISION 5) — not re-costed here and not taken.

---

## §7 — Carry-forwards for the state-6 close-out to bank

| id | what | severity |
|---|---|---|
| **CF-113-7** | The H2 doc-comment hijack: `finalize_h2_stream` lost its 10-line doc block and `h2_grpc_status()` inherited it. Two-hunk move, `crates/envoy-http2/src/hcm.rs:989-1014`. **A regression this phase introduced**, not an inherited gap. | Important |
| **CF-113-8** | The empty-`()` scope limit has no permanent test — the only evidence was a probe deliberately reverted — and upstream's behaviour on `%REQ()%`/`%RESP()%`/`%DYNAMIC_METADATA()%` is unmeasured. Three test lines + one `--mode validate` run. | Important |
| **CF-113-9** | The canonical-table test's `EXPECT` array is a byte-copy of the const it asserts against; 11 of 17 codes have no cross-proxy witness, including both cells `ADR-0193` D8 calls unguessable. Bank the 0-16 transcript. | Important |
| **CF-113-10** | `BEHAVIOR_CONTRACT.md` §J cites `ADR-0154` as corroborating `CANCELLED`, where `ADR-0154` measured `CANCELED` (one L) and `CANCELLED` **rejected** on the `grpc_status_filter` config surface. Will mislead `CF-113-4`. | Important |
| **CF-113-11** | The H2 boundary rationale — in the code comment and in `BEHAVIOR_CONTRACT.md:1035-1037` — omits the binding reason (`CF-110-1`) and states an obstacle that does not hold for a proxied trailers-only response. `CF-113-2`'s scoping is unaffected and remains correct. | Important |
| **CF-113-12** | `%GRPC_STATUS%` under `json_format` has zero cross-proxy witness; the typing rules landed in the contract with no wire check. Needs a second fixture (the byte-exact driver takes one log file per side). | Important |
| **CF-113-13** | The Minor set, N-1…N-22, in three groups. **(i) Code/test:** the gate's unvaried response content-type (N-5), the bare-`:N` error variant (N-10), the `_ =>` wildcard (N-11), the tautological record test (N-12), the malformed fuzz seed (N-13). **(ii) Contract:** the canonical grammar table's two missing forms (N-1), the stale standalone-operator list (**pre-existing**, N-2), §L's 2→11 generalisation (N-3), §I's "four of §B's nine" (N-4). **(iii) Record accuracy:** the fixture README/`expectations.yaml` nits (N-6, N-7, N-14), the two stale `envoy-http1` comments (N-8), `PLAN.md:60`'s task count inherited into `ADR-0193` (N-9), the self-invalidating "misleading 4" (N-15), `ADR-0193` D2's non-verbatim quotation (N-16), the mislabelled E0063 anchor (N-17), the two `SPEC.md` self-contradictions (N-18), `PLAN.md`'s 129-vs-127 (N-19), the `h2spec_runner.rs:26` off-by-one (N-20), the PV-8 four-vs-five drift (N-21), `ADR-0193`'s headline self-reference (N-22). **Plus the unmeasured cells**: negative and `>i64::MAX` `grpc-status` values, and the gate-open/header-absent rendering. | Minor |

**Context for `CF-113-5`, not a new finding:** `crates/envoy-http1/src/uring.rs:7-8` claims the
engagement gate requires "no access log"; the actual gate in `envoy-bin/src/main.rs` enforces no
such condition. `CF-113-5` is therefore a **live** gap, not one the config gate forecloses.
Pre-existing and outside this phase.

---

## §8 — Assessment, and the §7.5 gate adjudicated

**Ready to merge? YES — APPROVED.**

**Reasoning.** The implementation is correct on every cell I could adjudicate. The gate predicate
reproduces all six measured upstream rows and is the *same function* the value's producer uses, so
the two cannot drift. The 17-code table matches `PLAN.md`, `BEHAVIOR_CONTRACT.md` §J and the real
gRPC/absl canonical spellings including both traps. The empty-`()` relaxation's structural
confinement argument is verifiably true and its "eleven pre-existing operators" count is exactly
right — I counted the `no_arg_op` table myself. The JSON typed carve-out gets the
number/string/null trichotomy right including the fallback cells. The fixture's assertion path
terminates in a real byte-exact comparator, its twelve probes have distinct paths, and it is
deterministic and backend-free. Nothing here would make a fixture pass when it should fail.

The twenty-eight findings divide into four kinds, none of them correctness: **one regression this
phase introduced** (F-1, a doc-comment hijack with a two-hunk fix), **documentation defects that
will actively mislead a future phase** (F-4's ADR cross-reference, F-5's H2 rationale, N-1's stale
grammar table, N-4's miscount), **two verification-strength gaps** (F-2's untested scope limit,
F-3's self-referential table oracle) joined by one coverage-disclosure gap (F-6's unwitnessed JSON
typing), and **a set of record-accuracy slips** (N-9, N-15…N-22) of which the sharpest are a count
inherited into a landed ADR, a paraphrase presented as a quotation, and a self-referential figure
that its own commit falsified. All are recorded and banked; none is fixed here (§6.3; `ADR-0165`).

⚠ **A pattern worth naming for the next phase, because it recurs across six of these findings.**
N-9, N-15, N-18(a), N-18(b), N-19 and the `PLAN.md` 129/127 split are all the same shape: **a prose
count contradicted by a table, a list, or a later measurement inside the very same document.** The
phase's own record warns about this class twice and then commits it six times. The cheap mechanical
guard is to re-derive every prose count from the artifact it summarises as the last edit before
staging — the discipline this phase applied rigorously to its *code* figures, where every one
reproduced.

### The §7.5 gate, all six adjudicated

| leg | verdict | basis |
|---|---|---|
| **(a)** all new/changed differential fixtures green | **PASS** | Fixture `0093` GREEN against both real proxies on an `envoy-bin` proved to carry the phase's code (12 `Compiling` lines), `PROGRESS.md:1082-1085`. ⚠ **Leg (a) does NOT extend to the request-side gate** — `0093` provably cannot witness it (`ADR-0193` D2, `ADR-0194` D3, `CF-113-6`), and this review does not record it as doing so. |
| **(b)** all pre-existing fixtures still green | **PASS** | Local `--no-fail-fast` sweep read 169 binaries / 2292 passed / 6 failed; five are the documented stable core and the sixth, `set_metadata_dynamic_metadata`, was treated as a genuine suspect (it names the `%DYNAMIC_METADATA%` surface this phase edited) and cleared by ISOLATION 3/3 with settle gaps, corroborated by a panic blaming the upstream container and by CI running all six green at `failed=0`. |
| **(c)** conformance suites at threshold | **PASS (CI-only)** | `which h2spec` is empty on the dev host and `h2spec_runner.rs:26` self-skips, so a local green is worthless. CI's ANSI-stripped job log has `h2spec not found` = **0**, the positive control that the gate genuinely executed. |
| **(d)** new fuzzer clean on its short-budget CI run | **PASS** | No new target — §7.4's trigger is satisfied by the pre-existing `accesslog_format_parse`, whose `ci.yml` step I re-confirmed at `:78/:122/:127`. 4,117,175 runs, 0 crash markers, corpus proved READ at 1774 seed files. Three new seeds, tracked and un-ignored (re-verified here with `git ls-files` and the plain `git check-ignore`). |
| **(e)** the five cargo commands clean | **PASS** | `fmt` 0 bytes; `build` exit 0; `clippy` **14 `Checking` / 0 errors / 0 warnings** after a forced dirty set — the first run was a 0.10s cached no-op with zero `Checking` lines and was correctly rejected; `deny` `advisories ok, bans ok, licenses ok, sources ok`. The one `license-not-encountered` warning is pre-existing on an untouched `deny.toml`. |
| **(f)** `REVIEW.md` approved | **PASS — THIS DOCUMENT** | §2 is EMPTY. Twenty findings, all BANKED as `CF-113-7`…`CF-113-13`. |

**All six legs PASS. §7.5 is fully discharged and phase 113 is DONE pending the state-6 close-out.**

### STOP CONDITION — re-derived from disk at this review. ALL THREE LEGS FALSE

Measured freshly at this session, not inherited:

- **Leg (i) — FALSE.** `ROADMAP.md` census: **121 rows / 120 `done` / 0 `in-progress` / 1
  `planned`**; buckets sum to the row count (121 = 121). The not-done set is exactly
  `[('113','planned')]`. Status is field **4** on a `' | '` split driven from the `^\| [0-9]`
  prefix. ⚠ The forbidden `NF == 6` filter reads **119**, dropping the two rows at file lines 168
  and 169 that carry unescaped in-cell pipes (NF=7 and NF=10). They are append-only history and
  were **not** "fixed". ⚠ **Neither a state-4 gate nor a state-5 review touches `ROADMAP.md`** —
  row `113` stays `planned` until the state-6 close-out.
- **Leg (ii) — FALSE.** **14** crates, with `envoy-http3`/`envoy-grpc`/`envoy-wasm`/`envoy-protos`/
  `envoy-runtime` all absent by `test -d`. `quinn`/`wasmtime`/`tonic`/`opentelemetry`/`prost` = **0**
  across all **28** manifests counted by `git ls-files '*Cargo.toml'` (⚠ the denominator is
  method-dependent; this is the `git ls-files` set). **Positive control, identical invocation:
  `tokio` = 19 of 28.** Still unbuilt: QUIC and HTTP/3, the whole gRPC data path, ADS/delta-xDS/SDS/
  RTDS, hot restart, the observability sinks (exactly ONE access-log sink and **no stats sinks at
  all**), **no histogram primitive** (`grep -rni histogram crates/` = **0** against a **462**-hit
  `gauge` control), 6 of the 12 access-log filter arms, and the WASM host.
- **Leg (iii) — FALSE.** **11** `### ` family headings, of which **one** carries ZERO rows
  (`### WASM host family`). The eleven read 10/5/3/14/3/4/6/**30**/6/**0**/13 with **27** rows before
  the first heading, summing to 121. ⚠ A naive `awk` census under-reports at 10 headings because it
  never emits the zero-row one; this was driven from a single `/^### /` rule seeding every heading
  at 0. ⚠ `ROADMAP.md`'s known filing defect stands and was **not** repaired: seven
  `Observability family:` rows sit physically under `### Deprecated / edge features`, so the
  heading-slice census reads Observability 30 while the LOGICAL family is 37.

**ALL THREE LEGS MUST HOLD; NONE DOES. The mission is NOT complete and NO `stop` file was created**
(`ls stop` → `No such file or directory`). `ROADMAP.md:58` governs: a family becomes concrete rows
only when it enters `in-progress`, so an all-`done` census measures the rows that EXIST, not the
surface that remains. `ADR-0167` DECISION 2 is the standing adjudication.

### Next state

**§5 state 6 — the close-out — in a SEPARATE session** (§5.1; `ADR-0127`: a reviewer must not close
out what it graded). The close-out flips `ROADMAP.md` row `113` `planned` → `done` (**status cell
only**, by splitting on `' | '`, asserting 6 cells and the expected current status, replacing that
one cell and re-joining), relocates this phase's `STATE.md` `## Notes` subsections to
`STATE_HISTORY.md`, and adds **no ADR and no Notes subsection of its own**. ⚠ **Derive the not-done
set from `ROADMAP.md` at the close-out** — do not trust any row list a document names, including
this one.
