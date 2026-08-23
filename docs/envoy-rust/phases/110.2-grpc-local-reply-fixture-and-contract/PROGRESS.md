# Sub-phase 110.2 — §5 state-3 IMPLEMENTATION — PROGRESS

> Written for a reader with ZERO prior context (D-3.4). This file records what
> was **BUILT and RUN**, not what `PLAN.md`'s tables promised — the `110.1`
> REVIEW finding M-3 lesson: a PLAN's coverage table and its test list are TWO
> separate claims, and `110.1/PROGRESS.md` inherited the table's language.
> Every command output quoted here was produced by this session.

**Entry state, re-verified on disk rather than taken from the handoff.**
`git status --porcelain` clean, branch `main` at
`0b6f2f63824cc109b5c4ce40db335c8e36363280`, `git fetch origin --prune` exit `0`,
`origin/main` identical to `HEAD`. The phase directory held **`SPEC.md` +
`PLAN.md` ONLY** — no `PROGRESS.md`, no `REVIEW.md` — which IS the §5 state-3
detection rule. `ROADMAP.md` census re-derived **116 rows / 114 `done` /
1 `in-progress` / 1 `planned`**, the not-done set exactly
`[('110','in-progress'), ('110.2','planned')]`. `STATE.md` and `ROADMAP.md`
AGREE, so no `superpowers:systematic-debugging` detour was needed.
**Skill: `superpowers:executing-plans`**, TDD per
`superpowers:test-driven-development` on every task.

**X-item preconditions re-confirmed FRESH before any fixture ran** (each was a
CLAIM inherited from the state-2, not a fact):

| item | re-derived this session |
|---|---|
| **X-8** DEBUG `envoy-bin` rebuilt FIRST | `cargo build -p envoy-bin` exit **0** before any probe. A stale binary fails with `unknown field` errors that look like real divergences. |
| **X-1** pinned image digest verified BEFORE probing | `docker image inspect envoyproxy/envoy:v1.33.0 --format '{{index .RepoDigests 0}}'` → `envoyproxy/envoy@sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, **matching `ENVOY_TARGET.md` exactly**. |
| **X-4** fixture census | `git ls-files 'tests/fixtures/**' \| cut -d/ -f3 \| sort -u \| wc -l` = **88**, highest `0088-runtime-fraction-route-gating`, `ls -d tests/fixtures/0089*` → `No such file or directory`; **88** differential test files. |
| **X-5** the four harness facts | `HEADER_ALLOW_LIST` is **3 entries** (`server`, `date`, `x-envoy-upstream-service-time`) with `location` count **0**; `Http1BodyRule::ByteExact { body: String }` the ONLY variant; `Http1Method` exactly `Get`/`Options`/`Post`; `drive_http1` interpolates `extra_headers` **RAW** (`req.push_str(&format!("{n}: {v}\r\n"))`, no lower-casing, no validation) and emits `Host:`/`Connection: close` itself. |
| driver gating | `driver_needs_admin_port` matches only `AdminScrape`/`Http1KeepAlive`/`Http2KeepAlive`/`TcpWithStats` — `Http1ProbeList` is NOT among them, so `admin.port_value` is a LITERAL `0`. `{{PORT}}` IS substituted for `Http1ProbeList`. |

---

## Task 1 — the fixture skeleton, the 11 mapping cells and the 4 controls — **COMPLETE**

**Built:** `tests/fixtures/0089-grpc-aware-local-replies/envoy.yaml`,
`…/envoy-rust.yaml`, `…/expectations.yaml` (probes **1–15**), and
`tests/differential/tests/grpc_aware_local_replies.rs` (43 lines).
Registration is **cargo auto-discovery** — no `Cargo.toml` edit, no registry
list, no macro, exactly as `PLAN.md` Global Constraint 14 specifies.

**Both configs are BYTE-IDENTICAL**, asserted with the byte count as well as
the hash because a uniform md5 can be the empty-file md5
`d41d8cd98f00b204e9800998ecf8427e`:

```
216e712c14b1ca1dd8fcd0a4c277f8ab  envoy.yaml
216e712c14b1ca1dd8fcd0a4c277f8ab  envoy-rust.yaml
 6561 envoy.yaml
 6561 envoy-rust.yaml
```

**24 routes** landed in Task 1 so that no later task edits the yamls. No `node:`
block and no unquoted `y`/`n`/`on`/`off` scalar (`grep -nE ': *(y|n|on|off|yes|no)
*$'` returns nothing). All four binding constraints hold in the config as
written: no `201`/`3xx` `direct_response` (CF-110-3), every `direct_response`
carries an explicit `body:` (CF-110-7), the empty cell is
`body: { inline_string: "" }`, and there is no `header_mutation` anywhere
(CF-110-8).

**First run — GREEN, and that green proves only that the fixture EXECUTES:**

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.25s
```

`110.1` already landed the behaviour, so `0089` is a **CHARACTERIZATION PIN**
that passes on its first run. The mutation is the RED evidence.

### DEVIATION D-1 (SUBSTANTIVE) — `PLAN.md`'s mutation **V1 is MISAIMED and returns a FALSE GREEN**; the corrected one-sided form is what this session ran

`PLAN.md` Task 1 Step 5 specifies changing `/m-403`'s status from `403` to `500`
in **BOTH** yamls, and predicts: *"the RED must come from `diff_headers`,
proving the `grpc-status` VALUE is genuinely compared."* **Run as written, it
returns GREEN:**

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.17s
```

**Root cause, established on disk rather than inferred.** `diff_headers`
(`tests/differential/src/lib.rs:1204-1208`) has the signature

```rust
pub fn diff_headers(
    envoy: &[(String, String)],
    envoy_rust: &[(String, String)],
    allow_list: &[(&str, AllowMode)],
) -> anyhow::Result<()>
```

— it takes **only the two proxies' headers**. There is no fixture-declared
expected header VALUE anywhere in the harness: `Http1HeaderRule` is a unit
variant (`SetEqualModuloAllowList`) carrying no data. So the comparison is
**purely CROSS-PROXY**, and a mutation applied to BOTH configs moves both
proxies in lockstep — upstream maps `500`→`2`, envoy-rust maps `500`→`2`, the
two agree, and `expected_status: 200` / `expected_body: ""` still pass because
the transform fires either way. **This is the "mutation that moves an
implementation and its own witness together and returns a GREEN reading as
'these cells are vacuous'" failure mode, in its cross-proxy form.**

**Correction — break the SYMMETRY: mutate the UPSTREAM side ONLY.** This is
precisely the shape `PLAN.md`'s own V3 (Task 3 Step 3) already prescribes for
the same reason; V1 and V3 were simply inconsistent with each other, and V1 is
the one that is wrong.

Guard first — the anchor must occur EXACTLY ONCE in the one file mutated:

```
$ grep -c 'status: 403, body: { inline_string: "B403" }' envoy.yaml
1
```

Mutation (`envoy.yaml` only, so upstream serves `500` while envoy-rust still
serves `403`) → **RED, naming exactly the intended probe**:

```
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.16s

---- grpc_aware_local_replies stdout ----
thread 'grpc_aware_local_replies' panicked at tests/differential/tests/grpc_aware_local_replies.rs:42:10:
fixture green: probe g-403-maps-to-7: diff_headers

Caused by:
    header `grpc-status`: envoy=`2` envoy-rust=`7`
```

That is the **direct** proof `PLAN.md` intended and did not obtain: the
`grpc-status` VALUE is genuinely compared cross-proxy, and the `403`→`7` cell is
live. The `test result` line EXISTS, so this is a real mutation RED and not a
compile error.

**Revert adjudicated by md5, never by eye** — `git checkout --` would have been
a NO-OP here because the file was still UNTRACKED at this point in Task 1:

```
$ md5sum -c /tmp/v1.md5
envoy.yaml: OK
envoy-rust.yaml: OK
 6561 envoy.yaml
 6561 envoy-rust.yaml
```

**Unmutated CONTROL re-run from the same tree — GREEN:**

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.13s
```

Both directions of the causal experiment therefore hold: RED with the
asymmetry, GREEN without it. A one-direction result would have proven nothing.

---

## Task 2 — the eight §1.2 detection cells — **COMPLETE**

**Built:** probes **16–23** appended to `expectations.yaml` (probe count
**15 → 23**). The two config yamls were NOT touched — their md5 stayed
`216e712c14b1ca1dd8fcd0a4c277f8ab` at 6561 bytes across the whole task, because
Task 1 landed all 24 routes precisely so later tasks add probes only.

Three POSITIVE cells (`application/grpc`, `application/grpc+proto`,
`application/grpc+` bare) and five NEGATIVE (`application/grpc; charset=utf-8`,
`APPLICATION/GRPC`, `application/grpc-web`, `application/grpcfoo`, header
absent). The two `-web`/`foo` cells are the traps a naive
`starts_with("application/grpc")` falls into; the `APPLICATION/GRPC` cell is a
real witness only because `drive_http1` interpolates `extra_headers` RAW,
re-verified on disk this session (X-5).

**Run — GREEN:**

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.16s
```

**Mutation V2 — correctly aimed, unlike V1.** V2 mutates what is SENT
(lower-casing the `APPLICATION/GRPC` value), and its witness is
`expected_status`, which is an ABSOLUTE fixture-declared assertion checked
against BOTH proxies — not the cross-proxy `diff_headers`. So it does not have
V1's symmetry defect. Guard first: `grep -c '"APPLICATION/GRPC"'` = **1**.

```
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.12s

---- grpc_aware_local_replies stdout ----
thread 'grpc_aware_local_replies' panicked at tests/differential/tests/grpc_aware_local_replies.rs:42:10:
fixture green: probe d-upper-negative: upstream status 200 != expected 404
```

The negative cell is live: lower-case the value and upstream transforms.

### DEVIATION D-2 (METHOD) — `PLAN.md`'s V2 revert step is UNSAFE, and the `md5sum -c` is what caught it

`PLAN.md` Task 2 Step 3 reverts with
`git checkout -- expectations.yaml   # SAFE: the file is TRACKED as of Task 1`.
**Tracked is not sufficient.** `git checkout --` restores the file to its
**COMMITTED** state, and at that moment the committed state was Task 1's
**15-probe** version — so it silently destroyed the eight detection probes Task
2 had just added and had not yet committed:

```
$ md5sum -c /tmp/v2.md5
expectations.yaml: FAILED
md5sum: WARNING: 1 computed checksum did NOT match
$ grep -c '^    - name:' expectations.yaml
15                      <- Task 1's state, not Task 2's 23
$ grep -c 'd-exact-positive\|d-upper-negative\|d-foo-negative' expectations.yaml
0                       <- the whole task's work, gone
```

**The control run immediately after that revert returned GREEN — a FALSE GREEN
on the WRONG FILE**, and by eye it was indistinguishable from a correct one.
Only the md5 comparison exposed it, which is exactly the standing rule that a
mutation revert is adjudicated by md5 and never by eye. The recorded 23-probe
md5 was `c7bb67c1380ee0b6527b00356e9ab528`; the post-checkout file computed
`1581969acb4ad8d13f74add97e141119`.

**The generalised trap, worth carrying forward:** `git checkout --` is a no-op
on an UNTRACKED file (the already-known hazard) *and* a DESTRUCTIVE no-op-looking
revert on a TRACKED file that carries uncommitted work. It is safe only when the
mutated file's non-mutation content is identical to `HEAD`. Task 3's V3 mutates
`envoy.yaml`, which Task 3 does not otherwise modify, so `git checkout --` is
genuinely safe there — but this session used an explicit backup and md5
adjudication regardless.

**Recovery:** Task 2's block was re-applied and adjudicated byte-exactly against
the pre-mutation md5, then the control was re-run on the CORRECT file:

```
$ md5sum -c /tmp/v2.md5
expectations.yaml: OK
$ grep -c '^    - name:' expectations.yaml
23
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.10s
```

---

## Task 3 — method-insensitivity and the two empty-body cells — **COMPLETE**

**Built:** probes **24–26** (probe count **23 → 26**). The yamls were again
untouched (`216e712c…`, 6561 bytes).

- `x-post-method-insensitive` — a `post` probe at `/x-post` (a `403` route, so
  it also re-witnesses the `403`→`7` cell under a second method). `post` is the
  available second method because `Http1Method` has exactly three variants
  `get`/`options`/`post` — **there is no `put` and no `delete`** (re-verified on
  disk, X-5).
- `e-empty-no-grpc-message` — the `inline_string: ""` route.
- `nomatch-404-no-grpc-message` — `/no-such-route`, which matches NO route
  because `0089` deliberately has no catch-all. This drives a DIFFERENT
  local-reply site: the HCM's own route-not-found 404 (`synth_404` via
  `build_response_in`) rather than a `direct_response`. Per ADR-0180 DECISION 6
  this narrows `110.1/REVIEW.md` M-3 by one site **as a free side effect, not as
  scheduled work** — no banked finding is being fixed here (§6.3; ADR-0165).

**Neither empty-body cell has a non-gRPC twin**, and that is deliberate:
CF-110-6 records that envoy-rust's `synth_with` emits `content-type` on an
empty-body local reply where upstream emits none. In the gRPC direction both
proxies emit `content-type: application/grpc` and AGREE, so SPEC §3 F3's
requirement — `grpc-message` ABSENT ENTIRELY — is met twice over.

**Run — GREEN:**

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.15s
```

**Mutation V3 — as specified in `PLAN.md`, which gets this one right.** V3
mutates only the UPSTREAM side, the same symmetry-breaking shape D-1 had to
introduce into V1. Guard: `grep -c 'inline_string: ""' envoy.yaml` = **1**.
Giving upstream's `/e-empty` route the body `EMPTYNOW` makes upstream emit a
`grpc-message` header that envoy-rust does not:

```
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.13s

---- grpc_aware_local_replies stdout ----
thread 'grpc_aware_local_replies' panicked at tests/differential/tests/grpc_aware_local_replies.rs:42:10:
fixture green: probe e-empty-no-grpc-message: diff_headers

Caused by:
    header name sets differ: only-in-envoy=["grpc-message"], only-in-envoy-rust=[]
```

That is the **name-set** half of `diff_headers` firing, which is the direct
proof that `grpc-message` **ABSENCE** — not merely its value — is pinned. A
value-only comparison could not have produced this failure.

Reverted from an explicit backup copy rather than by `git checkout` (per D-2),
and adjudicated by md5:

```
$ md5sum -c /tmp/v3.md5
envoy.yaml: OK
envoy-rust.yaml: OK
 6561 envoy.yaml
 6561 envoy-rust.yaml
$ grep -c '^    - name:' expectations.yaml
26                      <- Task 3's own work confirmed intact, the D-2 lesson applied
```

Unmutated control from the same tree — **GREEN**:

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.14s
```

---

## Task 4 — the four §1.3 percent-encoding cells — **COMPLETE**

**Built:** probes **27–30** (probe count **26 → 30**). Yamls untouched
(`216e712c…`, 6561 bytes).

Two encoded cells and their two byte-exact untransformed controls. The controls
are load-bearing rather than decorative: without them a wrong ENCODING and a
wrong SOURCE BODY are indistinguishable. Both controls assert the original body
byte-exactly and both PASSED, which also establishes that upstream's YAML 1.1
parser and `serde_yaml`'s YAML 1.2 parser resolved the `\n`, `\t`, `é`, `%25`,
`\"` and `\\` escapes IDENTICALLY — a real risk given the two parsers differ
elsewhere in the tree.

**Run — GREEN:**

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.01s
```

### DEVIATION D-3 (ADDITION) — mutation **V4**, not in `PLAN.md`, because nothing else in the plan proves the `grpc-message` VALUE is compared at all

`PLAN.md` specifies no mutation for Task 4. V1 proves the cross-proxy VALUE
comparison is live for **`grpc-status`**, and V3 proves the **name-set** half
catches `grpc-message`'s ABSENCE — but no mutation in the plan establishes that
`grpc-message`'s VALUE is compared, which is the entire §1.3 encoding witness.
Left as planned, a broken encoder that produced the same header NAME would be
invisible to this fixture's own vacuity proofs.

One-sided upstream-only mutation (the D-1 shape), changing `/enc-main`'s trailing
`end` to `END` in `envoy.yaml` only. Guard: anchor count **1**.

```
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.05s

---- grpc_aware_local_replies stdout ----
thread 'grpc_aware_local_replies' panicked at tests/differential/tests/grpc_aware_local_replies.rs:42:10:
fixture green: probe enc-main-percent-encoded: diff_headers

Caused by:
    header `grpc-message`: envoy=`a b%0Acontrol%09tab %C3%A9 %2525 END` envoy-rust=`a b%0Acontrol%09tab %C3%A9 %2525 end`
```

Two things fall out of that one line. First, the `grpc-message` VALUE is
genuinely compared byte-exact cross-proxy — the encoding cells are not vacuous.
Second, the failure text DISPLAYS both proxies' full encodings, which
independently re-confirms the §1.3 rule cell by cell at this state rather than
on the state-2's authority: `\n`→**`%0A`**, `\t`→**`%09`**, `é`→**`%C3%A9`**
(UTF-8 encoded PER BYTE), `%25`→**`%2525`** (the discriminating cell for an
encoder that forgets to escape `%` itself), and the space PRESERVED. Both
proxies agree on every one.

Reverted from an explicit backup and adjudicated by md5 (`envoy.yaml: OK`,
`envoy-rust.yaml: OK`, 6561 bytes each, probe count still 30); unmutated control
**GREEN**:

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.04s
```

This makes **four** in-place mutations against SPEC §3 F6's "at least two", each
reverted byte-exactly and md5-verified, each with an unmutated control from the
same tree.

---

## Task 5 — the redirect cells; the 32-probe set is COMPLETE — **COMPLETE**

**Built:** probes **31–32** (probe count **30 → 32**, matching `PLAN.md`'s
frozen probe table exactly). Yamls untouched.

A `redirect:` route is the ONLY safe way to get a `location` header into this
fixture. Upstream also emits `location` on a `201`/`3xx` `direct_response` and
envoy-rust does not (CF-110-3, re-measured and WIDENED to `302` at the
state-2), so no such cell may appear — but `synth_redirect` already emits
`location` on both proxies. `location` is NOT on the `HEADER_ALLOW_LIST`
(re-derived this session: 3 entries, `location` count **0**), so its VALUE is
compared byte-exact, and that comparison IS the cell's witness: the gRPC probe
proves `location` SURVIVES the transform alongside `grpc-status: 2`.

**Full 32-probe run — GREEN:**

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.04s
$ grep -c '^    - name:' expectations.yaml
32
```

**X-3 re-asserted at the finished fixture** — both the md5 AND the byte count,
because a uniform md5 can be the empty-file md5:

```
216e712c14b1ca1dd8fcd0a4c277f8ab  envoy-rust.yaml
216e712c14b1ca1dd8fcd0a4c277f8ab  envoy.yaml
 6561 envoy-rust.yaml
 6561 envoy.yaml
```

### X-7 — the fast green AUDITED, in both directions

A backend-free fixture finishing in ~1 s is NORMAL, but "normal" is not
evidence. Proven with a **VALID** `docker ps` format field
(`{{.ID}} {{.Image}} {{.Names}}` — `{{.ImageID}}` is INVALID and turns every
poll line into a template error that reads as "no containers ran"):

```
NEGATIVE CONTROL (poll with no test running):
  0 lines matching envoyproxy/envoy

POSITIVE (poll while the fixture runs):
  ab8b9cbd2a1f envoyproxy/envoy:v1.33.0 keen_poincare

  4 poll lines captured; 0 template errors
  test result: ok. 1 passed; 0 failed; ... finished in 1.08s
```

The reference container genuinely ran, the poll genuinely executed, and the
negative control makes the positive non-vacuous.

---

## Task 6 — the fixture README — **COMPLETE**

**Built:** `tests/fixtures/0089-grpc-aware-local-replies/README.md`, **209
lines**. No code reads it; it is convention (85 of the 88 pre-existing fixtures
carry one). It contains all seven items `PLAN.md` Task 6 Step 1 requires:
title and provenance with the pinned digest; the full 32-row witness table;
"what actually pins what"; the three deliberately-absent cells each with its
measurement; the two shape decisions; how to run it (including the
`cargo build -p envoy-bin` precondition and the X-7 audit recipe with the VALID
`docker ps` format field); and the mutation table.

**Step 2 — no stray byte-identity claim.** The file makes exactly ONE
byte-identity claim (line 144) and it is immediately backed by both the md5 and
the byte count:

```
216e712c14b1ca1dd8fcd0a4c277f8ab  envoy.yaml
216e712c14b1ca1dd8fcd0a4c277f8ab  envoy-rust.yaml
 6561 envoy.yaml
 6561 envoy-rust.yaml
```

The README states explicitly that byte-identity is a **PER-FIXTURE** claim to be
re-derived and never a tree property.

The mutation table records all **four** mutations, and states the reason three
of them must be ONE-SIDED: `diff_headers` is purely cross-proxy, so a two-sided
mutation moves both proxies in lockstep and returns a false green. That is the
D-1 lesson, recorded where the next reader of the fixture will find it rather
than only in this PROGRESS file.

---

## Task 7 — the `BEHAVIOR_CONTRACT.md` `## gRPC` section — **COMPLETE**

**Built:** a new `## gRPC` section, **260 lines**, with `###` subsections
**§A–§H** plus the fixture pointer and the two adjacent-divergence notes.

**Step 1 — the insertion point re-derived BY TEXT, not by `PLAN.md`'s line
numbers.** `## Active gRPC health check` at **574** and `## Header allow-list`
at **644** (matching the PLAN's figures, but re-derived rather than trusted),
with `grep -c '^## gRPC'` = **0** beforehand. The section went immediately
BEFORE `## Header allow-list`, keeping all gRPC content contiguous. Sections in
this file are INSERTED TOPICALLY, never appended at EOF.

**Step 3 — verified structurally, with the FULL pathspec** (a bare
`-- BEHAVIOR_CONTRACT.md` matches nothing and returns a believable EMPTY forever
— `110.1/REVIEW.md` N-5):

```
$ grep -c '^## gRPC' docs/envoy-rust/BEHAVIOR_CONTRACT.md
1
$ grep -c '^## ' docs/envoy-rust/BEHAVIOR_CONTRACT.md
16                                  <- 15 -> 16
$ grep -n '^## ' … | sed -n '5,9p'
522:## Active TCP health check (`tcp_health_check`)
574:## Active gRPC health check (`grpc_health_check`)
644:## gRPC
904:## Header allow-list
$ git diff --numstat -- docs/envoy-rust/BEHAVIOR_CONTRACT.md
260	0                               <- PURE INSERTION
```

Pure insertion proven the stronger way as well — the pre-edit file is a
SUBSEQUENCE of the post-edit file (`SUBSEQUENCE: True`, 4046 → 4306 lines,
delta **260**), so no existing line was altered or reordered.

**Step 4 — the fixture is still GREEN** after the docs edit:

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.85s
```

### DEVIATION D-4 (METHOD) — the section was DRAFTED by a subagent, and THREE of its claims were WRONG and had to be corrected before the main session wrote the file

Per the handoff, Task 7 is the one task genuinely independent of Tasks 1–6, so
it was drafted by a read-only subagent (told not to write and not to run
`cargo`) while the main session wrote Task 6. **The main session wrote the file,
and graded the draft rather than pasting it.** Three claims did not survive:

1. **A WRONG cross-reference.** The draft linked the ADR-0059 empty-body rule as
   `[recorded above](#header-allow-list)`. Lines 1131-1137 are NOT in
   `## Header allow-list` — an `awk` scan for the enclosing `^## ` heading puts
   them in **`## Stat-name mapping`** (line 674). Corrected to name that section
   explicitly, with no anchor.
2. **UNVERIFIABLE provenance.** The draft attributed the `~`→`%7E` correction to
   "ADR-0178 V-8" and claimed it was "re-confirmed on nine bodies supplied as
   `inline_bytes`". `grep -c 'V-8' docs/envoy-rust/DECISIONS.md` = 22, none
   confirmable as that specific finding, and the nine-body figure is nowhere on
   disk. Replaced with the citation this session actually read: `110.1/SPEC.md`
   §1.3.
3. **AN OVERSTATED MEASUREMENT.** The draft said detection was measured
   "over four methods". `110.1/SPEC.md` §1.2 records `GET`, `POST` and `PUT` —
   **three**. Corrected to name them.

Two of the draft's riskier-looking claims DID survive verification and were
kept: `decorate_filter_synth_response_h2` genuinely exists (referenced at
`crates/envoy-http1/src/hcm.rs:2491` and `crates/envoy-filter/src/jwt_authn.rs:13`),
and the filter local-reply list (rbac 403 / fault / jwt_authn 401 /
local_ratelimit 429 / overflow 503) is quoted VERBATIM from
`BEHAVIOR_CONTRACT.md:1136-1137`. The lesson is the standing one, and it cut
both ways here: **a subagent finding is a CLAIM — re-verify on disk**, including
the ones that look safe and excluding none that look authoritative.

**One addition beyond `PLAN.md`'s content list.** §E records a THIRD property of
`diff_headers` alongside the two the PLAN names: it takes only the two proxies'
header vectors and no fixture-declared expected value, so it is purely
cross-proxy, and **a mutation intended to witness a header cell must be
ONE-SIDED**. That is the D-1 finding, promoted into the canonical contract so
the next author of a header fixture meets it before writing a two-sided
mutation.

---

# Sub-phase 110.2 — §5 state-3 IMPLEMENTATION — SUMMARY

**All SEVEN `PLAN.md` tasks are COMPLETE.** Seven task commits, each with its own
fixture run.

## What was BUILT (not what a table promised)

| deliverable | disposition |
|---|---|
| `tests/fixtures/0089-grpc-aware-local-replies/envoy.yaml` | NEW, 103 lines, 24 routes |
| `tests/fixtures/0089-grpc-aware-local-replies/envoy-rust.yaml` | NEW, BYTE-IDENTICAL to the above |
| `tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml` | NEW, **32** probes |
| `tests/fixtures/0089-grpc-aware-local-replies/README.md` | NEW, 209 lines |
| `tests/differential/tests/grpc_aware_local_replies.rs` | NEW, 43 lines, cargo AUTO-DISCOVERED |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | `## gRPC` section, PURE INSERTION `260 0` |

**Exactly the 32 probes of `PLAN.md`'s frozen table — no more, no fewer.** The
coverage claim and the probe list are the SAME artifact here: `grep -c
'^    - name:' expectations.yaml` = **32**, and every one appears in the README's
32-row table. This is deliberate, per the `110.1` M-3 lesson.

## Standing censuses re-derived at this state

| census | before | after |
|---|---:|---:|
| fixture directories | 88 | **89** |
| differential test files | 88 | **89** |
| `^## ` in `BEHAVIOR_CONTRACT.md` | 15 | **16** |
| `^## gRPC` in `BEHAVIOR_CONTRACT.md` | 0 | **1** |
| `HEADER_ALLOW_LIST` entries | 3 | **3** (unchanged; `location` count still **0**) |
| `known-failures.txt` lines | 21 | **21** (untouched) |
| ADR head | ADR-0180 | **ADR-0180** (no ADR fired; ADR-0181 still UNRESERVED) |
| ROADMAP rows / `done` / `in-progress` / `planned` | 116/114/1/1 | **116/114/1/1** (NOT touched — rows flip at state 6) |

## Scope discipline — verified, not asserted

```
$ git diff --name-only 0b6f2f6 HEAD -- crates/ '*Cargo.toml' Cargo.lock .github/ \
    deny.toml docs/envoy-rust/ROADMAP.md 'docs/envoy-rust/phases/110-*' \
    'docs/envoy-rust/phases/110.1-*' .../110.2/SPEC.md .../110.2/PLAN.md | wc -l
0
```

**NO crate source change** (Global Constraint 1 / SPEC §5), no `Cargo.toml` or
`Cargo.lock`, no `ci.yml`, no `deny.toml`, no `ROADMAP.md`, and **no landed
artifact edited**. **NOT ONE banked finding was fixed** (§6.3; ADR-0165) —
CF-110-6, CF-110-7, CF-110-8 and CF-110-9 are all still open, and the
`110.1` REVIEW's M-1…M-9 + N-1…N-10 are untouched. `0089`'s `/no-such-route`
probe drives one of M-3's undriven sites, but as a free structural side effect
of having no catch-all route, not as scheduled work.

## Size — measured, at the CARRYING commit rather than a moving `HEAD`

```
$ git diff --numstat 0b6f2f6 HEAD -- . ':(exclude)docs/' | awk '{a+=$1;d+=$2} END{print a,d,a-d}'
817 0 817
```

**Net 817**, docs-excluded, `added − deleted` — the metric every landed
calibration phase was measured under. `BEHAVIOR_CONTRACT.md` is under `docs/`
and is correctly excluded. Against `PLAN.md`'s ≈615 that is a ratio of **1.33**,
inside the eight-phase distribution the state-2 measured (median **1.19**, worst
**1.50**) and well under the ~1500 §6.1 gate. The range is cited explicitly as
`0b6f2f6 HEAD` rather than as a bare `HEAD` claim, because a numstat citation
goes false the instant its own carrying commit lands (`110.1/REVIEW.md` M-9).

## Vacuity proofs — FOUR mutations, all with unmutated controls

| mutation | sides mutated | probe RED'd | evidence |
|---|---|---|---|
| **V1** (corrected, D-1) | upstream only | `g-403-maps-to-7` | `header 'grpc-status': envoy='2' envoy-rust='7'` |
| **V2** | `expectations.yaml` | `d-upper-negative` | `upstream status 200 != expected 404` |
| **V3** | upstream only | `e-empty-no-grpc-message` | `header name sets differ: only-in-envoy=["grpc-message"]` |
| **V4** (added, D-3) | upstream only | `enc-main-percent-encoded` | `header 'grpc-message': envoy='…%2525 END' envoy-rust='…%2525 end'` |

Each was guarded by an exactly-once anchor count before mutating, reverted
byte-exactly, adjudicated by **md5** rather than by eye, and paired with an
unmutated control run from the same tree. Every RED carried a real
`test result: FAILED` line — a compile error is not a mutation RED.

## The four DEVIATIONS, in order of consequence

- **D-1 (SUBSTANTIVE)** — `PLAN.md`'s mutation **V1 is misaimed and returns a
  FALSE GREEN**: it mutates both yamls, and `diff_headers` is purely
  cross-proxy, so both proxies move in lockstep. Corrected to a one-sided
  mutation. **A PLAN's own code is a claim — this one was run, and it failed.**
- **D-2 (METHOD)** — `PLAN.md`'s V2 revert uses `git checkout --` on the grounds
  the file is TRACKED. Tracked is not sufficient: it restored the file to its
  COMMITTED state and destroyed Task 2's eight uncommitted probes, producing a
  false-green control on the wrong file. Caught by `md5sum -c`.
- **D-3 (ADDITION)** — mutation **V4**, because no mutation in the plan proved
  the `grpc-message` VALUE — the whole §1.3 encoding witness — is compared.
- **D-4 (METHOD)** — the Task-7 contract section was subagent-DRAFTED and then
  GRADED, not pasted; three of its claims were wrong (a wrong section
  cross-reference, an unverifiable ADR provenance, and an overstated
  "four methods").

## X-item ledger — all re-confirmed FRESH at this state

X-8 (debug `envoy-bin` rebuilt before every run), X-1 (digest verified by
`docker image inspect` BEFORE probing, matching `ENVOY_TARGET.md` exactly), X-2
(the fixture run IS the dry-run at this state — green probe by probe, task by
task), X-3 (md5 **AND** byte count: `216e712c14b1ca1dd8fcd0a4c277f8ab`, **6561**
bytes each), X-4 (census 88 → 89, no `0089` beforehand), X-5 (all four harness
facts on disk), X-7 (the ~1 s green audited with a VALID `docker ps` format
field plus a negative control).

## PREDICTION for state 4 — NOT a measurement

**This session did NOT run the state-4 gate** (§5.1; ADR-0127 — that is the next
session's product, and it is CI's first real execution of this code). The
workspace identity is `passed + failed = **2227**` today at `binaries=165` (the
plain `cargo test --workspace` form CI runs; `--all-targets` yields 149, the
16-binary gap being the doc-test harnesses).

`0089` adds **exactly ONE test binary containing exactly ONE test**, by cargo
auto-discovery. **PREDICTED: `binaries=166 passed=2228 failed=0`. Any other
movement is a signal.** This figure is a PREDICTION and is labelled as one; the
state-4 session must derive it, not inherit it.

## One RED observed at session close — CLASSIFIED AS A KNOWN FLAKE BY ISOLATION, not by text

Recorded because a session that hides a red teaches the next reader nothing. The
final confirmation run — on a tree whose only change since the previous GREEN
was `docs/`-only — came back RED:

```
thread 'grpc_aware_local_replies' panicked at tests/differential/tests/grpc_aware_local_replies.rs:42:10:
fixture green: upstream Envoy never became accept-ready

Caused by:
    127.0.0.1:55000 not accept-ready within 10s: Connection refused (os error 111)

test result: FAILED. 0 passed; 1 failed; ... finished in 18.16s
```

**It is the documented upstream-container readiness family** — `upstream Envoy
never became accept-ready … Connection refused`, whose root cause is the
ephemeral-port startup race in `reserve_port()` (CF-75-6). Two facts settle it,
and neither is the error text:

1. **The tree was byte-identical to the green run.** `git status --porcelain
   tests/` was empty, `md5sum` on both yamls still `216e712c14b1ca1dd8fcd0a4c277f8ab`
   at 6561 bytes, and the probe count still 32. The only files modified were
   `STATE.md`, `STATE_HISTORY.md` and this `PROGRESS.md`.
2. **It PASSES IN ISOLATION, three times, with a settle gap.** The 18.16 s
   duration is itself the signature: 10 s of accept-ready wait plus teardown,
   rather than the ~1 s a real probe comparison takes.

```
run 1: exit=0 | test result: ok. 1 passed; 0 failed; ... finished in 1.19s
run 2: exit=0 | test result: ok. 1 passed; 0 failed; ... finished in 1.19s
run 3: exit=0 | test result: ok. 1 passed; 0 failed; ... finished in 1.16s
```

**A 20-25 s SETTLE GAP separated each run**, because back-to-back Docker-spawning
runs manufacture a false `FAILS-IN-ISOLATION` verdict — the trap that produced a
wrong classification at the `110.1` state-4. **ONLY ISOLATION CLASSIFIES, never
the error text.** This family is CI-authoritative and is never a regression; the
state-4 session should expect it under a full parallel `cargo test --workspace`
and must not read it as one.

## Next state

**§5 state 4 — verification**, a SEPARATE session (§5.1; ADR-0127).
`superpowers:verification-before-completion` runs the full §7.5 gate at
WORKSPACE scope. `ROADMAP.md` is NOT touched until state 6, where rows `110.2`
**and parent `110`** flip `done` TOGETHER (the `76.2`/`108.2`/`109.2` two-row
precedent, unlike the `110.1` close-out which flipped one row only).

---

# Sub-phase 110.2 — §5 state-4 VERIFICATION — PROGRESS

> A SEPARATE session from the state-3 implementation (§5.1; ADR-0127 — the
> context that wrote the code must not be the one that grades its gate).
> Skill: `superpowers:verification-before-completion`. This session ran the
> full §7.5 gate at WORKSPACE scope and quotes every command's ACTUAL output
> below. It wrote NO code, fixed NO banked finding, touched NO `ROADMAP.md`
> line, edited NO landed artifact, and did NOT write `REVIEW.md`.

**Entry state, re-verified on disk rather than taken from the handoff.**
`git status --porcelain` clean, branch `main` at
`84e930173a0e21ac48f216c25c53ec6ff90ac877` (the state-3 CI-record commit,
numstat **`1 0`** touching `docs/envoy-rust/STATE.md` ONLY — re-derived, not
inherited). The phase directory held **`SPEC.md` + `PLAN.md` + `PROGRESS.md`
and NO `REVIEW.md`**, which IS the §5 state-4 detection rule. `STATE.md` and
`ROADMAP.md` AGREE (row `110` `in-progress`, `110.1` `done`, `110.2`
`planned`), so no `superpowers:systematic-debugging` detour was needed.
Toolchain `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` / `rustc 1.95.0 (59807616e 2026-04-14)`.

## Gate (e) — the five workspace-scope commands

**`cargo build --workspace --all-targets` — exit 0.** Gated on the `Compiling`
count, not the exit code, because a warm `target/` returns exit 0 with ZERO
`Compiling` lines:

```
$ grep -c '^ *Compiling' build.log
8
   Compiling envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.00s
EXIT=0
```

Zero `warning` lines. This also satisfies **X-8** — the harness runs the DEBUG
`envoy-bin`, and a stale one fails with `unknown field` errors that read as real
divergences. X-8 is a precondition, not a gate.

**`cargo clippy --workspace --all-targets --all-features -- -D warnings` — exit 0.**
⚠ **clippy prints `Checking`, not `Compiling`.** The first run returned exit 0
with only **1** `Checking` line — a warm-cache near-no-op, and therefore weak
evidence. The causal experiment was run: an **mtime-only `touch` of every
tracked crate root**, driven from `git ls-files` so that no file could be
CREATED (a hand-written path list once made 22 empty files), leaving the tree
clean:

```
$ git ls-files 'crates/*/src/lib.rs' 'crates/*/src/main.rs' \
      'tests/*/src/lib.rs' 'tests/*/*/src/main.rs' | wc -l
22
$ xargs -a roots.txt touch -m
$ git status --porcelain          # empty — tree still clean
```

Re-run against that genuinely dirty set:

```
$ grep -c '^ *Checking' clippy2.log
22
$ grep -cE '^(warning|error)' clippy2.log
0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.13s
EXIT=0
```

**22 `Checking` lines for 22 touched roots** — a genuine workspace-wide pass,
not a cached one.

**`cargo fmt --all -- --check` — exit 0**, and the log is **7 bytes** (the
`EXIT=0` line alone), i.e. ZERO diff output:

```
EXIT=0
```

**`cargo deny check` — exit 0.** ⚠ The four-ok line is **ANSI-COLOUR-CODED**;
a naive `grep -E 'advisories ok, bans ok'` returns a FALSE **0**. Matched
ANSI-STRIPPED:

```
$ sed -e 's/\x1b\[[0-9;]*m//g' deny.log | grep -E 'advisories ok|bans ok|licenses ok|sources ok'
advisories ok, bans ok, licenses ok, sources ok
$ sed -e 's/\x1b\[[0-9;]*m//g' deny.log | grep -cE '^error'
0
```

Five `license-not-encountered` warnings (`"0BSD"`, `"BSD-2-Clause"`, … —
*unmatched license allowance*) are present and are the documented NORMAL class
on a green run; the four-ok line is what gates.

**`cargo test --workspace` — run TWICE with `--no-fail-fast`, diffing the RED SET.**
The CI-total recipe is used (the `ok`-only form makes `failed=0` TAUTOLOGICAL,
and fields `$5`/`$7` return a believable `passed=0`):

```
$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' test1.log \
    | awk '{p+=$4; f+=$6; n++} END {print "binaries="n" passed="p" failed="f" identity="p+f}'
binaries=166 passed=2223 failed=5 identity=2228

$ ... test2.log
binaries=166 passed=2221 failed=7 identity=2228
```

**THE IDENTITY IS 2228 IN BOTH RUNS, AND IT EQUALS CI's `passed`.** That is the
cross-check the protocol demands (`local passed + failed == CI passed`), and it
confirms the movement `2227 → 2228` is **EXACTLY the one test fixture `0089`
adds** — the state-3 PREDICTION, now MEASURED rather than inherited. Binary
count **166** matches CI, which runs the plain form (`--all-targets` yields 149;
the 16-binary gap is the doc-test harnesses, so only the identity is invariant).

## The local RED set — CLASSIFIED BY ISOLATION, never by error text

The RED set VARIES run-to-run, which is why it was measured twice and diffed:

```
$ diff red1.txt red2.txt
5a6,7
> client::tests::send_request_maps_h2_handshake_failure_to_typed_error
> runtime_static_layer
```

**The stable core FIVE — all fail DETERMINISTICALLY IN ISOLATION**, each run
alone behind a **22 s SETTLE GAP** (back-to-back Docker-spawning runs manufacture
a false `FAILS-IN-ISOLATION`, the trap that produced a wrong classification at
the `110.1` state-4), with the count asserted rather than the exit code:

```
access_log_h2_rcd_upstream_reset | exit=101 | test result: FAILED. 0 passed; 1 failed
access_log_h2_uc_upstream_reset  | exit=101 | test result: FAILED. 0 passed; 1 failed
access_log_rcd_upstream_reset    | exit=101 | test result: FAILED. 0 passed; 1 failed
access_log_rf_upstream_reset     | exit=101 | test result: FAILED. 0 passed; 1 failed
admin_config_dump_server_info    | exit=101 | test result: FAILED. 0 passed; 1 failed
```

Deterministic-in-isolation IS the documented signature of the HOST-ENVIRONMENT
families, and the isolation fingerprints are byte-for-byte the under-load ones:

- the four `*_upstream_reset` access-log fixtures diverge `rf` **`UF`** (upstream)
  vs **`UC`** (envoy-rust) because upstream reaches an **unreachable IPv6**
  backend — `remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:…`;
- `admin_config_dump_server_info` diverges only on `backend::**192.168.65.2**:…`
  cluster rows — the documented Docker-Desktop host-bridge address, whose
  backend-routing fixtures are CI-AUTHORITATIVE and RED locally by construction.

**The two run-2-only REDs PASS in isolation** behind the same settle gap, with
the count asserted (so neither is a `0 passed; N filtered out` false green):

```
h2_handshake         | exit=0 | test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 113 filtered out
runtime_static_layer | exit=0 | test result: ok. 1 passed; 0 failed
```

— the documented `envoy-http2` h2-handshake host-flake and the ephemeral-port /
admin-ready startup-race family (`reserve_port()`, CF-75-6). Both are
parallel-load artifacts, never regressions.

**ATTRIBUTION — measured, not argued.** No RED can be attributed to `110.2`,
because the sub-phase touched none of the relevant surface. The complete
`0b6f2f6..HEAD` change set is NINE files — four under `docs/` and five that ARE
fixture `0089` plus its entrypoint:

```
$ git diff --name-only 0b6f2f6 HEAD -- crates/ '*Cargo.toml' Cargo.lock .github/ deny.toml rust-toolchain.toml | wc -l
0
$ git diff --name-only 0b6f2f6 HEAD -- 'tests/differential/tests/access_log_*upstream_reset.rs' \
    'tests/differential/tests/admin_config_dump_server_info.rs' 'tests/differential/src/' | wc -l
0
```

ZERO crate source, ZERO build inputs, ZERO shared-harness files, and ZERO of the
five RED fixtures' own files. **CI on this exact tree reports `failed=0`**, which
is the authority for all seven.

## Gate (a) — fixture `0089` green cross-proxy on EVERY probe

`test grpc_aware_local_replies ... ok` in BOTH full sweeps, and alone:

```
test grpc_aware_local_replies ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.99s
```

`0 filtered out` is asserted because `cargo test -p <pkg> <name>` lies both ways.
**A green run means ALL 32 probes passed**, verified on disk rather than assumed:
the probe helper propagates with `?`/`bail!` carrying `probe.name`, so the
probe-list driver ABORTS at the first failing probe and one red run names ONE
probe. Probe count re-derived **32** by three agreeing probes.

**X-7 — the ~1 s green was AUDITED, not trusted.** A backend-free fixture
finishing in ~1 s is normal, but that is exactly the shape a silently-skipped run
also has:

```
$ docker ps --format '{{.ID}} {{.Image}} {{.Names}}'    # NEGATIVE CONTROL, before
0 envoy containers
# during the run, polled every 0.3 s with a VALID format field:
d92529b66336 envoyproxy/envoy:v1.33.0 sleepy_mccarthy
```

The poll captured real lines rather than the template errors `{{.ImageID}}`
produces — an INVALID field turns every poll line into an error that reads as
"no containers ran". The pin was verified BEFORE probing and matches
`ENVOY_TARGET.md` exactly:

```
$ docker image inspect envoyproxy/envoy:v1.33.0 --format '{{index .RepoDigests 0}}'
envoyproxy/envoy@sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2
```

**X-3 re-derived** — both configs byte-identical, asserted with the byte COUNT as
well as the hash because a uniform md5 can be the empty-file md5
`d41d8cd98f00b204e9800998ecf8427e`:

```
216e712c14b1ca1dd8fcd0a4c277f8ab  envoy.yaml
216e712c14b1ca1dd8fcd0a4c277f8ab  envoy-rust.yaml
6561 / 6561 bytes
```

## Gate (b) — all 88 pre-existing fixtures still green

**89** fixture directories and **89** differential test files (both moved 88 → 89).
The 88 pre-existing fixtures are green in CI (`failed=0` across all 166 binaries);
locally, the five host-environment REDs above are pre-existing and
CI-AUTHORITATIVE, per the standing rule for the `192.168.65.2` bridge and the
unreachable-IPv6 backend families.

## Gate (c) — conformance unchanged, and the local green is VACUOUS

`known-failures.txt` **UNTOUCHED at 21 lines / ONE real entry**, and it does not
appear in the nine-file change set. It was never trimmed on local evidence.

⚠ **The local h2spec gate SELF-SKIPS SILENTLY and STILL reports `ok`** — proven
here rather than assumed, with `--nocapture`:

```
h2spec_runner: h2spec not found — skipping locally
test h2spec_pass_rate_gate ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
$ grep -c 'h2spec not found' h2spec.log
1
```

**A local `ok` therefore proves NOTHING about h2spec.** Gate (c) is
CI-AUTHORITATIVE, and CI is where it genuinely executed — on the state-3 run,
independently re-censused by this session:

```
$ grep -c 'h2spec not found' ci_state3.log
0
$ grep -c 'test h2spec_pass_rate_gate ... ok' ci_state3.log
1
```

## Gate (d) — N/A, and the census re-derived

No new fuzz target: this sub-phase adds no parser, codec, filter or config
surface. The census is **5 targets across 5 crates**, driven from `git ls-files`
because a `find`-based probe DOUBLE-COUNTS on this tree:

```
$ git ls-files '*fuzz_targets/*.rs'        -> 5   (envoy-accesslog, envoy-config,
                                                   envoy-filter, envoy-http2, envoy-jwt)
$ find . -path '*fuzz_targets/*.rs' | wc -l -> 25  (the double-count, reproduced)
```

## Gate (f) — NOT this session's

`REVIEW.md` is state 5, a SEPARATE session (§5.1; ADR-0127). It was not written.

## Standing censuses re-derived from disk

| census | expected | measured |
|---|---:|---:|
| fixture directories | 89 | **89** |
| differential test files | 89 | **89** |
| crates | 14 | **14** |
| phase directories | 120 | **120** |
| `known-failures.txt` | 21 lines / 1 entry | **21 / 1** |
| fuzz targets / crates | 5 / 5 | **5 / 5** |
| `HEADER_ALLOW_LIST` | 3, `location` ABSENT | **3**, `location` and `content-type` both ABSENT |
| `ConfigError` variants | 134 | **134** (two agreeing probes; naive whole-file `grep -cP '^    [A-Z]'` reads **162**) |
| ADR head / next free | 0180 / 0181 | **ADR-0180** / **ADR-0181** UNRESERVED |
| ROADMAP rows / done / in-progress / planned | 116/114/1/1 | **116/114/1/1**, not-done exactly `[('110','in-progress'), ('110.2','planned')]` |
| `BEHAVIOR_CONTRACT.md` lines / `^## ` / `^## gRPC` / `^### ` | 4305 / 16 / 1 / 32 | **4305 / 16 / 1 / 32** |
| `0089` probes | 32 | **32** |
| `envoy.yaml` / `README.md` / entrypoint lines | 103 / 209 / 43 | **103 / 209 / 43** |
| `grpc.rs` / `http1 hcm.rs` / `uring.rs` / `http2 hcm.rs` | 709 / 11633 / 538 / 7394 | **709 / 11633 / 538 / 7394** |
| net docs-excluded LoC, range `0b6f2f6 6af7649` | 817 | **817** (`added=817 deleted=0`) |

⚠ The `BEHAVIOR_CONTRACT.md` numstat was taken with the **FULL path**; the bare
form is the documented empty-forever trap and was run as its own control:

```
$ git diff --numstat 0b6f2f6 6af7649 -- BEHAVIOR_CONTRACT.md
                                                        (EMPTY — matches nothing)
$ git diff --numstat 0b6f2f6 6af7649 -- docs/envoy-rust/BEHAVIOR_CONTRACT.md
260     0       docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

## The four state-3 DEVIATIONS — re-verified, not inherited

- **D-1 CONFIRMED on disk.** `diff_headers` (`tests/differential/src/lib.rs:1204-1208`)
  takes `envoy`, `envoy_rust` and `allow_list` and **no fixture-declared expected
  header value**; `Http1HeaderRule` is a unit variant. Header assertions are
  therefore purely CROSS-PROXY, and a symmetric mutation moves both proxies in
  lockstep. The state-3's correction to a ONE-SIDED mutation was right.
- **D-2, D-3, D-4** are method/addition records internal to the state-3 and are
  consistent with the artifacts on disk; none of them alters a gate outcome here.

## DEVIATION V-1 (this session) — `STATE.md` mis-states `PROGRESS.md`'s length

`STATE.md` asserts `110.2/PROGRESS.md` is **671 lines** in two places (the
`## Active phase` `**directory:**` block and the `## Last commit` block). The
measured value is **713**, at every commit that could carry it:

```
on disk:    713      at 6af7649: 713      at 84e9301: 713
$ git diff --numstat 6af7649 HEAD -- .../110.2/PROGRESS.md      (EMPTY)
```

The file has never been 671 lines. **This is a stale COUNT in the live ledger,
not in a landed artifact, and it is NOT corrected in place**: the blocks that
carry it are superseded by this state-4 transition and are relocated to
`STATE_HISTORY.md` **verbatim, byte-for-byte** per ADR-0035, which forbids
editing a narrative on its way to the archive. The correct figure is stated in
this session's replacement blocks. Recorded as evidence for the state-5 reviewer
that a handed COUNT is a claim.

## What this session did NOT do

No code written; no banked finding fixed (§6.3; ADR-0165 — **CF-110-1…CF-110-9**,
the `110.1` REVIEW's **M-1…M-9 + N-1…N-10**, the `109.2` M-1…M-8 + N-1…N-11, the
`109.1` M-5 + N-1…N-6, the `108.2` M-2 + N-1…N-6 and the HTTP-filters-family
(1)-(4) all stay OPEN); no `ROADMAP.md` line touched (rows flip at state 6, where
`110.2` **and parent `110`** flip `done` TOGETHER); no landed artifact edited; no
ADR fired (head stays **ADR-0180**); no `REVIEW.md`; no `stop` file; and no code
was changed to make a gate pass.

## Next state

§5 **state 5 — code review**, a SEPARATE session (§5.1; ADR-0127: the context
that ran the gate must not grade it). `superpowers:requesting-code-review`
writes `REVIEW.md`. It must weigh the four state-3 deviations, V-1 above, the
four new carry-forwards CF-110-6/7/8/9, and whether the 32-probe set actually
witnesses what `110.2/SPEC.md` §3 F3 asks for — **a green gate proves the tests
pass, not that they ask the right question.**
