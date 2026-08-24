# Sub-phase 110.2 — the differential witness: fixture `0089` (32 probes) + the `BEHAVIOR_CONTRACT.md` `## gRPC` section — CODE REVIEW

**Verdict: APPROVED-WITH-MINORS.**

Per `BOOTSTRAP_PROMPT.md` §7.5, an approved `REVIEW.md` closes gate **(f)** — the only gate still
open. Gates (a)–(e) were run and adjudicated by the §5 state-4 verification session and are
recorded, with actual command outputs, in `PROGRESS.md`. **This review did not re-run them and does
not re-adjudicate them** (§5.1; ADR-0127 — the context that ran the gate must not grade it, and the
context that grades it must not fix it). It re-confirmed CI on the exact tree under review
independently (§0.3), because that is a fact about the commits rather than a re-run of the gate.

**Zero Issues. Eight Minors, twelve Notes.** Not one finding is a wire-behaviour defect, and not one
weakens the gate. Every cell `110.2/SPEC.md` §3 **F3** requires is witnessed by a named probe, and
on four clauses — default-arm coverage, empty-body sites, encoding, detection — the fixture
**exceeds** what F3 asks. The findings are concentrated in **citation accuracy** and in **the stated
SCOPE of three carry-forwards**, plus a family of coverage observations that F3 deliberately scoped
out and that `110.1`'s in-process suite pins absolutely.

Two subagent findings arrived graded **Must-Fix** and are **DOWNGRADED** here after re-derivation
(§5). Grading them honestly matters: §5.2 sends a Must-Fix back to **state 3**, and neither
warrants three more sessions.

The review's single most useful output is **M-1**: the `## gRPC` section carries a line-number
citation into its own file that was **correct when ADR-0180 wrote it and was invalidated by the very
commit that transcribed it** — the section is 260 lines long, and the cited block moved down by
exactly 260. Three sibling citations into *other* files all survive intact. That is a
self-referential-citation hazard the repository has not previously named.

Per §6.3 and ADR-0165 **nothing was fixed by this session**. **No §5.2 re-entry to state 3 is
required** — the verdict is an approval and gate (f) is CLOSED; every Minor and Note below is
BANKED for the state-6 close-out to carry.

---

## §0 — How this review was conducted

### §0.1 — Scope

The tree under review is `main` at `d5e7f037de7128b5e951abd976964bcd127992e7`, clean
(`git status --porcelain` = 0 lines), with `origin/main` at the same commit after a
`git fetch origin --prune` whose exit code was checked (`FETCH_EXIT=0`).

The §5 state-5 detection rule was re-verified **on disk** rather than taken from the handoff:
`docs/envoy-rust/phases/110.2-grpc-local-reply-fixture-and-contract/` holds `SPEC.md` (354 lines)
+ `PLAN.md` (1185) + `PROGRESS.md` (1062) and **no `REVIEW.md`**; `STATE.md` `## Active phase`
`**id:**` reads `110` SPLIT with the active pointer on `110.2`; `ROADMAP.md` reads row `110`
`in-progress`, row `110.1` **`done`**, row `110.2` `planned`. **`STATE.md` and `ROADMAP.md` AGREE**,
so no `superpowers:systematic-debugging` detour was needed. `ls stop` returns
`No such file or directory`.

The implementation under review is the range `0b6f2f6..HEAD` — **nine files**, re-derived here:

```
$ git diff --name-status 0b6f2f6 HEAD
M	docs/envoy-rust/BEHAVIOR_CONTRACT.md
M	docs/envoy-rust/STATE.md
M	docs/envoy-rust/STATE_HISTORY.md
A	docs/envoy-rust/phases/110.2-.../PROGRESS.md
A	tests/differential/tests/grpc_aware_local_replies.rs
A	tests/fixtures/0089-grpc-aware-local-replies/README.md
A	tests/fixtures/0089-grpc-aware-local-replies/envoy-rust.yaml
A	tests/fixtures/0089-grpc-aware-local-replies/envoy.yaml
A	tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml

$ git diff --numstat 0b6f2f6 6af7649 -- . ':(exclude)docs/'
43	0	tests/differential/tests/grpc_aware_local_replies.rs
209	0	tests/fixtures/0089-grpc-aware-local-replies/README.md
103	0	tests/fixtures/0089-grpc-aware-local-replies/envoy-rust.yaml
103	0	tests/fixtures/0089-grpc-aware-local-replies/envoy.yaml
359	0	tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml
                                    → added=817 deleted=0 net=817
```

**Zero files under `crates/`.** This sub-phase ships no crate source change, exactly as
`110.2/SPEC.md` §5 states. The docs-excluded net is **817**, confirming the figure `PROGRESS.md`
cites — and note it is cited at the **RANGE** `0b6f2f6 6af7649`, not against a moving `HEAD`
(the `110.1` M-9 trap, correctly avoided here).

### §0.2 — Method

Five independent read-only reviewers were dispatched over disjoint QUESTIONS: (a) the probe set
versus SPEC §3 F1–F6 cell by cell; (b) whether the three deliberate absences are genuinely forced;
(c) the `BEHAVIOR_CONTRACT.md` `## gRPC` section against the code and against its own citations;
(d) whether mutations V1–V4 each pin a distinct assertion; (e) an audit of `PROGRESS.md`'s state-3
and state-4 claims against the tree. Every reviewer was barred from writing and from running
`cargo` and `docker` (the cargo lock serializes, and a state-5 changes no code).

**Every finding below was re-verified on disk by this session before being written down** — a
subagent finding is a claim. The decisive measurements — the 260-line citation drift, the
`synth_with`/`synth_redirect` split, the 25-distinct-paths count, the corpus-wide 404 census, and
the STOP-condition legs — were all made by this session directly. §5 records the dissents.

### §0.3 — CI re-confirmed independently on the exact tree under review

Not inherited from the handoff. The full 40-char SHA was interpolated from `git rev-parse`, never
retyped — a short or retyped SHA silently returns `[]`:

```
$ gh run list --commit d6796952050925ee2914a7a5dc20b9bad848399e --json ...
[{"attempt":1,"conclusion":"success","databaseId":32637204868,"event":"push",
  "headSha":"d6796952050925ee2914a7a5dc20b9bad848399e","status":"completed"}]

$ gh api repos/{owner}/{repo}/actions/runs/32637204868/jobs --jq ...
97188651249 build + test + lint  conclusion=success steps=15 runner=GitHub Actions 1000005434
97188651357 fuzz (...)           conclusion=success steps=13 runner=GitHub Actions 1000005435
```

**Steps 15/13, both jobs with REAL runner names** — not the `runner_name:""` + `steps:0` starvation
shape. Build-job log **685271** bytes. The census was taken **non-tautologically** (matching
`(ok|FAILED)` and summing awk fields 4/6, so `failed=0` is a measurement rather than an artifact of
the grep):

```
binaries=166   passed=2228   failed=0
'test result: FAILED' = 0      Doc-tests = 16
'h2spec not found'    = 0   WITH  'test h2spec_pass_rate_gate ... ok' = 1
'test grpc_aware_local_replies ... ok' = 1
cargo deny four-ok line (ANSI-STRIPPED) = 1
```

`grpc_aware_local_replies ... ok` confirms fixture `0089` genuinely EXECUTED rather than being
silently skipped, and `local passed + failed = 2228 == CI passed` closes the cross-check.

### §0.4 — Standing censuses re-derived at HEAD

Every one of these is asserted by `PROGRESS.md`/`STATE.md`; every one was re-derived here:

```
fixture dirs (git ls-files 'tests/fixtures/**' | cut -d/ -f3 | sort -u | wc -l) : 89
differential test files                                                         : 89
crates : 14      phase directories : 120      fuzz targets : 5 across 5 crates
known-failures.txt : 21 lines / 1 real entry
HEADER_ALLOW_LIST  : 3 entries; `location` ABSENT, `content-type` ABSENT
BEHAVIOR_CONTRACT.md : 4305 lines; '^## ' = 16 ; '^## gRPC' = 1 (line 644) ; '^### ' = 32
fixture 0089 : 32 probes / 24 routes / 25 distinct probe paths
               envoy.yaml ≡ envoy-rust.yaml  md5 216e712c14b1ca1dd8fcd0a4c277f8ab  6561 bytes EACH
ADR head : ADR-0180 ; grep -c '^## ADR-0181' = 0 (UNRESERVED)
ROADMAP  : 116 rows / 114 done / 1 in-progress / 1 planned
```

Both the md5 **and** the byte count are asserted for the YAML pair, because a uniform md5 can be
the empty-file md5.

**V-1 independently CONFIRMED.** The state-4 recorded that `STATE.md` mis-stated `PROGRESS.md` as
671 lines. Re-derived here: `git show 6af7649:.../PROGRESS.md | wc -l` = **713**, and the state-4
commit's numstat on that path is `349 0`, giving 713 + 349 = **1062** on disk. The file was never
671 lines. Archiving it verbatim rather than correcting it in place was the right call under
ADR-0035.

---

## §1 — Strengths

**The deliverable's best decision is that it tells you what it does not prove.** A differential
fixture cannot assert a header VALUE — `Http1HeaderRule` is a **unit variant carrying no data**
(`tests/differential/src/lib.rs:1083-1085`), and `diff_headers` (`:1204-1263`) takes only the two
proxies' header vectors and no fixture-declared expectation. So every `grpc-status`,
`grpc-message`, `content-type`, `content-length` and `location` assertion in `0089` is **cross-proxy
agreement, not an absolute**. The literals `13`/`16`/`7`/`12`/`14`/`2` and the encoded strings
appear nowhere in `expectations.yaml` outside probe names and prose.

That could have been a silent trap. Instead it is disclosed **four times over** — `README.md:66-91`
("there is no fixture-declared expected header VALUE anywhere in the harness"), `expectations.yaml:13-20`,
the entrypoint doc `grpc_aware_local_replies.rs:23-26`, and `BEHAVIOR_CONTRACT.md` §E — each
stating plainly that the cross-proxy comparison IS the entire witness. §D goes further and warns
that the measured header ORDER "fails an **in-process unit test**, never fixture `0089`. Do not
assume `0089` is protecting it." **Naming the limit of your own witness is rarer and more valuable
than the witness.**

**The absolute half of the pin genuinely exists, in the right place.** `110.1` carries it, and it is
exhaustive rather than exemplary: `crates/envoy-http1/src/grpc.rs:599-609` sweeps **all 256 byte
values** against the encoder rule, and `:672-694` sweeps **the entire `u16` range** asserting that
exactly eight statuses are special and all 65 528 others are `2`. A "helpful" extra arm like
`500 => 13` or `4xx => 13` is unlandable. The division of labour is correct: `110.1` pins the table
absolutely in-process, `110.2` pins agreement with upstream cross-proxy. Neither alone would be
enough; together they are.

**Detection is the one matrix that IS pinned absolutely by the fixture**, and by construction rather
than by intent: the transform flips status `404`→`200` and body `"DEXACT"`→`""`, and both axes are
fixture-declared absolutes asserted against both proxies. So the eight detection probes survive even
the harness's header-comparison limits. The five negatives double as self-controls, each pinning its
own distinct body.

**Two probes witness the same contract cell at two structurally different local-reply sites.**
`e-empty-no-grpc-message` drives `synth_direct_response`; `nomatch-404-no-grpc-message` drives the
HCM's own route-not-found 404 via `synth_404`/`build_response_in` — a site `110.1` REVIEW M-3 had
listed as undriven, covered here as a free side effect of having no catch-all route. F3 asked for
one empty-body probe; the fixture ships two, at different builders.

**The 3xx default arm is witnessed despite the config being forbidden from expressing it.**
CF-110-3 bars any `201`/`3xx` `direct_response`, which looks like it should cost the fixture the
whole 3xx mapping arm. `r-redirect-grpc-keeps-location` recovers it through the one safe path — a
`redirect:` route — and simultaneously witnesses `location` survival. One probe, two cells, around
a hard constraint.

**The `%25` → `%2525` cell is the sharpest single choice in the probe set.** It is the one cell that
discriminates a hand-rolled encoder that forgets to escape `%` itself, and the README says so
explicitly. Paired with `~` → `%7E` — the boundary the parent SPEC got measurably wrong — the two
encoding probes target exactly the two places the obvious rule fails.

**Every constraint the fixture obeys is documented with its measurement and its consequence**, not
asserted as taste: the missing `node:` block (YAML 1.1 vs 1.2 booleanization), the literal admin
`port_value: 0` (`{{ADMIN_PORT}}` is driver-gated and `Http1ProbeList` is not among the four), the
mandatory explicit `body:` (CF-110-7), and the standing prohibition against allow-listing
`location` or `content-type` to make a probe pass. That last one is stated in both `README.md` and
`BEHAVIOR_CONTRACT.md` §E as a corpus-wide prohibition rather than a local note — the correct
altitude, because the failure mode it prevents looks like success.

---

## §2 — Issues (Must Fix)

**NONE.**

Stated plainly, because §5.2 makes this the load-bearing sentence of the review: any Issue here
would send the work back to §5 **state 3**, not state 4. There is no Issue.

Every cell `110.2/SPEC.md` §3 F3 requires is witnessed by a named probe (§1, and the cell-by-cell
diff behind it). The fixture is byte-identical across its two YAMLs, ships no crate source change,
trips none of the three measured non-gRPC divergences it was required to avoid, and is green
cross-proxy in CI on the exact tree under review with all 32 probes executed. The four mutations
V1–V4 pin four **distinct** assertions and their reverts are byte-exact — confirmed here from
committed history rather than from the record's own `md5sum -c` output.

**Two subagent findings were graded Must-Fix and are DOWNGRADED by this session; §5 records both
dissents with reasoning.** Neither survives contact with the digest-pinned-oracle premise or with
§5.2's actual consequence.

---

## §3 — Minor

### M-1 — the `## gRPC` section's one citation into its OWN file was correct when ADR-0180 wrote it and was invalidated by the very commit that transcribed it; the three citations into other files all survive

`BEHAVIOR_CONTRACT.md:887-888`, inside the new section, reads:

> The upstream rule is recorded in the `## Stat-name mapping` section's ADR-0059 entry
> (`BEHAVIOR_CONTRACT.md:1131-1137`)

At HEAD, `:1131-1137` is the **`listener_manager.lds.*` stat-name table** — LDS update counters,
nothing to do with `content-type`. The ADR-0059 empty-body rule is at **`:1391-1397`**.

The mechanism, established by measurement rather than inferred:

```
$ git show 0b6f2f6:docs/envoy-rust/BEHAVIOR_CONTRACT.md | sed -n '1131,1137p'
- **No `content-type` header** (ADR-0059). Upstream Envoy v1.33 does NOT
  emit `content-type` on an **empty-body** local reply. ...

$ git diff --numstat 25c21b3^ 25c21b3 -- docs/envoy-rust/BEHAVIOR_CONTRACT.md
260	0	docs/envoy-rust/BEHAVIOR_CONTRACT.md

  ## gRPC section span : 644..903  =  260 lines
  drift               : 1391 − 1131 =  260
```

**The citation was TRUE at `0b6f2f6`.** Commit `25c21b3` — 110.2 Task 7, the commit that *wrote the
sentence carrying the citation* — inserted 260 lines at `:644` and pushed the cited block down by
exactly 260. The arithmetic closes to the line.

What makes this worth naming rather than filing as a typo: the same section's **three other**
citations are all exact —

```
tests/differential/src/lib.rs:1189-1193   → HEADER_ALLOW_LIST          ✓ exact
crates/envoy-http1/src/grpc.rs:158-160    → the idempotence sentinel   ✓ exact
crates/envoy-config/src/bootstrap.rs:2923-2926 → struct DirectResponse ✓ exact
```

**Only the self-referential one broke, and it broke *because* it was self-referential.** A citation
into another file is immune to your own insertion; a citation into the file you are inserting into
is not. The repository's standing rule is "locate by TEXT, line numbers drift" — this is the sharper
special case: **a line citation into the file you are editing is invalidated by your own edit, in
the same commit, with no external event required.** The section attribution ("`## Stat-name
mapping`") remains correct, since that section spans `:934-1616` and still contains `:1391`.

Independently corroborated from a second direction: `PROGRESS.md`'s own D-4 record carries the same
pre-insertion coordinates (`1131-1137`, `1136-1137`, and `674` for the `## Stat-name mapping`
heading, which is now `:934`). All three are exactly 260 low — internally consistent with each
other and stale against the tree. The *substance* of D-4 #1 is correct; only the coordinates are.

### M-2 — CF-110-6's scope is over-stated in three places, and the fixture's own probe 32 is the counterexample

The claim, in three landed artifacts:

- `expectations.yaml:26-29` — "no empty-body CONTROL probe — envoy-rust's `synth_with` emits
  `content-type` on an empty-body local reply and upstream does not"
- `README.md:106-107` — same wording
- `BEHAVIOR_CONTRACT.md:885` — "**CF-110-6** — envoy-rust emits `content-type` on an **EMPTY-body
  local reply** where upstream emits none", and `:888-892` "**the rest of the local-reply family
  does not obey it**"

**Probe 32, `r-redirect-control`, IS an empty-body non-gRPC control probe** — `expected_status: 301`,
`expected_body: { kind: byte_exact, body: "" }`, `expected_headers: set_equal_modulo_allow_list` —
and it is GREEN. It sits roughly ten lines below the comment asserting no such probe exists.

It is green because a `redirect:` route never touches `synth_with`. The two builders are disjoint:

```rust
// crates/envoy-http1/src/hcm.rs:2286  — synth_with, content-type UNCONDITIONAL
headers: vec![ SERVER, DATE, CONTENT_LENGTH, CONTENT_TYPE, CONNECTION ]

// crates/envoy-http1/src/hcm.rs:2430  — synth_redirect, NO content-type
headers: vec![ LOCATION, DATE, SERVER, CONNECTION, CONTENT_LENGTH ]
```

And `synth_redirect`'s **own doc block** (`:2423-2429`) has recorded the rule and the exact failure
string since phase 76.2:

> a redirect carries EXACTLY `location`, `date`, `server`, `connection`, `content-length` — and NO
> `content-type`, which a `direct_response` DOES carry. It therefore must NOT reuse [`synth_with`],
> whose fixed 5-header list always emits `content-type`; doing so fails the harness's `diff_headers`
> name-set check with `only-in-envoy-rust=["content-type"]`.

So one member of the local-reply family already obeys the ADR-0059 rule, and the codebase knew it
two phases before CF-110-6 was opened. **The accurate scope is "the `synth_with` family"** —
`synth_direct_response`, `synth_status`, and through it `synth_400`, `synth_404`, `synth_501` — not
"an empty-body local reply" and not "the rest of the local-reply family".

**ADR-0180's stated reason for the divergence going uncaught is also incomplete.** It says
(`DECISIONS.md:2422`) "no fixture exercises an empty-body `direct_response`: `grep -rn
'inline_string: ""' tests/fixtures/` returns 0". That explains one of the two dry-run REDs; the
other was `/no-such-route` → `synth_404`, which involves no `direct_response` at all. Re-measured
here, the corpus-level reason is stronger and simpler:

```
17 probes corpus-wide carry `expected_status: 404`
 1 fixture asserts response headers on any of them — 0089 itself
```

**No fixture in the corpus has ever asserted headers on a 404.** That, not the `inline_string: ""`
census, is why every `synth_with` empty-body reply — the tree's most frequently produced local
replies — went unwitnessed. CF-110-6's real blast radius is wider than the carry-forward states, and
that matters for whichever slice eventually fixes it.

Nothing here is a fixture defect: SPEC F3's empty-body requirement is met twice over in the gRPC
direction, and dropping the two `synth_with` control twins was correct.

### M-3 — SPEC §3 F3 specifies an assertion the harness structurally cannot make, and F6's mutation example is impossible for the same reason

`110.2/SPEC.md:225-226` (F3): "one probe whose route body is the §1.3 string, **asserting the exact
`grpc-message` value**." `:228-229`: the redirect probe "**proving `location` survives**".
`:243-244` (F6): "for example **flipping one mapped code in the expectations**".

None of the three is expressible. `Http1HeaderRule` is a **unit variant carrying no data**
(`tests/differential/src/lib.rs:1083-1085`); `diff_headers` (`:1204-1208`) takes only the two
proxies' header vectors and no expectation parameter. There is no mapped code *in* the expectations
to flip, and no exact header value to assert. The SPEC's own §2.3 (`:139-141`) describes the
mechanism correctly as cross-proxy comparison — so **the SPEC contradicts itself**, and F3 is the
acceptance criterion a future reader will grade the fixture against.

The implementation did the only possible thing (cross-proxy comparison plus a one-sided mutation),
and did it well. But F6's impossible example is not harmless: it is the most likely origin of the
D-1 defect. Told to "flip a mapped code in the expectations" where no such code exists, `PLAN.md`
reached for the YAMLs instead and reached for **both** of them — producing a symmetric mutation that
could only ever return green. **An unsatisfiable SPEC clause propagated into a false-green
procedure two states later.** `PLAN.md`'s own V3 already had the one-sided shape, so the plan
contradicted itself as well; state 3 caught it, correctly diagnosed it, and recorded it as D-1.

### M-4 — the `§G one-path-per-probe attribution rule` citation is false three ways, and 110.2's own new §G now collides with it

`README.md:25` and `envoy.yaml:28` cite "the `BEHAVIOR_CONTRACT.md` §G one-path-per-probe
attribution rule", inherited verbatim from `110.2/SPEC.md:203` and originally from
`110/SPEC.md:387`.

1. **The phrase does not exist in the file.** `grep -c 'one-path-per-probe'
   docs/envoy-rust/BEHAVIOR_CONTRACT.md` = **0**.
2. **The rule it gestures at does not bind this driver.** The real text is at `:3186-3202`, under a
   *different* section's "§G Authoritative fixtures", and states its own scope at `:3196`: "**The
   rule binds EVERY `http1_access_log_byte_exact` fixture**". Its rationale (`:3186-3190`) is that
   the access-log driver "asserts only (a) a per-side line COUNT and (b) whole-line cross-proxy
   equality; there is no per-probe assertion". The same paragraph says why `http1_probe_list` is
   exempt (`:3169-3171`): "**because `http1_probe_list` names and asserts every probe individually —
   the strongest of the three**." Fixture `0089` is `http1_probe_list`.
3. **110.2 created a colliding §G.** The new section's own `### §G` (`:834`) is "a pre-existing
   `grpc-status` response header: MEASURED, and a DIVERGENCE (CF-110-8)". A reader following the
   README's cross-reference into the `## gRPC` section — the natural destination — lands on
   CF-110-8.

And the claim is **factually false at the probe level** regardless: re-derived here, `0089` has
**25 distinct probe paths for 32 probes** across 24 routes; seven paths (`/m-200`, `/m-400`,
`/m-404`, `/m-503`, `/enc-main`, `/enc-edge`, `/r-redir`) are each shared by a gRPC probe and its
control twin. "Each at its own distinct path" is true of the **routes**, false of the **probes**, and
the cited rule is about probes.

**Harmless to the gate** — path sharing is fine precisely for the reason the contract gives, since
every probe is individually named and asserted, and a red run names one probe by name. This is a
citation defect, and the over-claim is **inherited**: the copy in `README.md` should be charged
alongside its origin in `110/SPEC.md:387`, which is landed and uneditable.

### M-5 — CF-110-3 is the one carry-forward absent from `BEHAVIOR_CONTRACT.md`, and it is the one that qualifies §D's own `location` row

Census of the file, taken here:

```
CF-110-1 : 1     CF-110-4 : 0     CF-110-7 : 1
CF-110-2 : 1     CF-110-5 : 0     CF-110-8 : 2
CF-110-3 : 0     CF-110-6 : 1     CF-110-9 : 0
```

The section records the divergences the fixture avoided **by shape** (CF-110-6, -7, -8) and the two
**scope** carry-forwards (CF-110-1 H2, CF-110-2 proxied). **CF-110-3 alone is missing** — and it is
the only one that lands directly on a §D row. §D (`:763`) states:

> `| `location` | **SURVIVES** — a pass-through header, value unchanged |`

unqualified. That is true **only where a `location` exists to survive** — i.e. the `synth_redirect`
path. On a `201`/`3xx` `direct_response`, envoy-rust emits no `location` at all, in the gRPC
direction *and* the control direction (CF-110-3, re-measured at the 110.2 PLAN-write on `201`,
`301` and `302`; `204` gets none on either side). §H's adjacent-divergence preamble (`:879-883`)
says "**Two** ADJACENT recorded divergences", while the fixture's own `expectations.yaml:22-34`
correctly lists **three** and puts CF-110-3 first.

A reader implementing to §D takes "`location` SURVIVES" as the whole `location` story. It is the
one place in the section where the contract is less complete than the fixture that witnesses it.

### M-6 — "three independent PASS-THROUGH cases" adds a word its source does not have, and that word makes it false for one of the three

`BEHAVIOR_CONTRACT.md:781-784`:

> The rule is GENERAL rather than a `location` special case — it was measured on three independent
> **pass-through** cases: a bodied `direct_response`, a `redirect:` route, and a circuit-breaker
> overflow …

The source is `110.1/PLAN.md:200-209`, finding N-4, which says "**THREE independent cases**" — no
"pass-through" — and whose own table shows why the added word is wrong:

| case | measured gRPC order |
|---|---|
| bodied `direct_response` 503 | `content-type, grpc-status, grpc-message, date, server, connection, content-length` |
| `redirect:` route | `location, content-type, …` |
| circuit-breaker overflow | `x-envoy-overloaded, content-type, …` |

The bodied `direct_response` carries **zero** pass-through headers. Only two of the three exercise
the pass-through half of the rule at all.

This is not pedantry: `110.1/REVIEW.md` **M-5** is the banked finding that "the pass-through
RELATIVE-ORDER half of the header rule is unpinned: no fixture carries more than one pass-through
header." The contract's transcription strengthens its source's claim in exactly the direction that
makes a live open finding look closed. **A transcription that is stronger than its source is the
same defect class as a wrong number, and harder to see.**

### M-7 — the `--all-targets = 149` / "16-binary gap" pair is stale and arithmetically inconsistent at `binaries=166`, and it was re-asserted under a "MEASUREMENTS, NOT PREDICTIONS" banner

`PROGRESS.md:823`:

> Binary count **166** matches CI, which runs the plain form (`--all-targets` yields 149; the
> 16-binary gap is the doc-test harnesses, so only the identity is invariant).

CI on this tree reports `binaries=166` with **16** `Doc-tests`. So the non-doc target count is
**150**, and `166 − 149 = 17`, not 16. The triple (149, 166, 16) cannot all be true.

The `149` is correct **at the baseline**: at `0b6f2f6`, CI reported `binaries=165`, and
`165 − 16 = 149`. It was carried verbatim from the state-3 PREDICTION block (`:657`) into the
state-4 record without re-derivation, while the plain-form figure beside it was updated to 166.
Fixture `0089` adds one integration-test target, which `--all-targets` also builds — so the
correct post-change value is **150**.

Two things make this worth a Minor rather than a Note. First, the state-4 section is explicitly
framed as quoting actual output, and `STATE.md` re-asserts "**⚠ THE GATE NUMBERS BELOW ARE
MEASUREMENTS, NOT PREDICTIONS — this session ran them**" while carrying the same false pair — so a
stale prediction is presented to this reviewer as a measurement. Second, the traps line still
carries the *original, correct* statement from an earlier phase ("`--all-targets` = 149 binaries,
plain = **165**, gap = 16"), which was true then; the state-4 updated one member of the triple and
left the other two. **The identity `passed + failed = 2228` — the figure that actually matters — is
measured, correct, and equal to CI's `passed`.** Only the form-dependent side figure is stale, which
is precisely the hazard the repository's own standing rule names.

### M-8 — no mutation pins the `content-type`/`content-length` rewrite or `location` survival; a one-sided V5 would close both in one line

The four mutations map onto the assertion classes as follows (re-derived here):

| class | pinned by |
|---|---|
| `grpc-status` mapping VALUE | **V1** (one-sided, `/m-403` in `envoy.yaml` only) |
| detection NEGATIVE direction | **V2** (symmetric, but witnessed by absolute `expected_status`) |
| `grpc-message` ABSENCE on empty body | **V3** (one-sided; reds the name-set branch) |
| `grpc-message` encoding VALUE | **V4** (one-sided; the D-3 addition) |
| `content-type` / `content-length` rewrite | **nothing** |
| `location` survival | **nothing** |
| the body axis / untransformed controls | **nothing** |

The four are genuinely distinct — V4 is **not** redundant with V1, because presence in the compared
vector is name-specific while V1 only proves the value branch fires. D-3's justification for adding
it is sound, and its reverts are byte-exact (confirmed here from committed history: both YAMLs carry
md5 `216e712c…` at every one of the eight fixture commits).

The gap is that `PLAN.md` Task 5 — the `location` task — specifies **no mutation at all**, the only
fixture-building task without one. The missing V5 is the same shape as V1/V3/V4 and is one `sed`:
change `path_redirect: "/x"` in `envoy.yaml` **only** (`envoy.yaml:93`, anchor count 1). The
resulting RED would prove in one run that `location` is in both header vectors, that its value is
compared, and that it survives the transform.

**Severity note, and this is a deliberate downgrade** (§5): a reviewer graded this Must-Fix on the
argument that a green on probes 31–32 cannot distinguish `location` surviving from `location` being
dropped **on both sides**, so the pair "asserts nothing whatsoever about `location`." The
structural half is correct — `diff_headers` compares the union of what the two proxies actually
sent, so a name absent from both is not a mismatch. The conclusion does not follow. Upstream is a
**digest-pinned** image (`sha256:56da5afd…70c2`) that demonstrably emits `location` on a redirect;
it cannot regress without a pin-bump phase and its own re-baselining. So the only regression the
fixture exists to catch — envoy-rust dropping `location` unilaterally — **does** red the name-set
check. The residual is a coverage gap in the mutation set, not a blind assertion. Minor.

---

## §4 — Notes

**N-1 — where the absolute pin actually lives, stated so it is not mistaken for a gap.** `0089`
proves envoy-rust **agrees with** upstream v1.33.0; it does not prove either implements the SPEC's
transcribed table. That second leg is carried entirely by `110.1`'s in-process suite, and carried
exhaustively: `grpc.rs:599-609` sweeps all 256 byte values against the encoder rule, `:672-694`
sweeps the whole `u16` range asserting exactly eight specials. The literals `13`/`16`/`7`/`12`/`14`
appear nowhere in `expectations.yaml` outside probe names — so `g-400-maps-to-13` is a **label, not
an assertion**. Disclosed in four places by the deliverable itself; recorded here only so a future
reader does not rediscover it as a defect.

**N-2 — five §1.2 detection cells are unwitnessed cross-proxy; none is F3-required.**
`application/grpc+json`, `application/grpc;charset=utf-8` (no space), `Application/Grpc`,
`application/grpc-web+proto`, `application/json`. All are pinned in-process at `grpc.rs:255-263`.
Only **`application/grpc-web+proto`** tests an equivalence class the fixture does not otherwise
cover: an implementation shaped `starts_with("application/grpc") && (len == 16 || contains('+'))`
passes all five shipped negatives and wrongly accepts it. If one cell is ever added, that is the one.

**N-3 — `te: trailers` independence has no regression guard on either layer.** `BEHAVIOR_CONTRACT.md`
§B asserts detection is "INDEPENDENT of `te: trailers`, measured in both directions", and
`110.2/SPEC.md:50` repeats it. No probe sends `te: trailers` and no in-process test does either. A
measured-once claim with nothing defending it.

**N-4 — `0x7F` → `%7F` is unwitnessed cross-proxy but is EXPRESSIBLE.** Recorded because "not
witnessed" and "not witnessable" are different findings. `inline_bytes` is closed (boot-fatal here,
`bootstrap.rs:1207-1220`), but a YAML double-quoted `"\x7F"` escape in `inline_string` is accepted
by both loaders and `0x7F` is valid single-byte UTF-8. F3 made the second encoding probe optional
and the fixture shipped it anyway, so this is an omission, not a shortfall.

**N-5 — the filter-generated local-reply family has zero differential witness, and CF-110-5 is
untouched.** ADR-0179 DECISION 2 deliberately **WIDENED** the transform's family to filter-generated
local replies, and the seam sits at the funnel (`hcm.rs:1491`) where it covers them. But `0089`'s
`http_filters` chain is **router-only**, so no JWT 401, rate-limit 429, fault or CORS local reply is
exercised cross-proxy. Separately, `tests/differential/src/lib.rs` contains no `io_uring` reference,
so the second production call site (`uring.rs:525`) remains unwitnessed — **CF-110-5 is not closed
by this fixture**, and a reader should not infer otherwise from "the differential witness landed."

**N-6 — the contract's "whole 2xx / whole 3xx range" is a derived rule presented under a blanket
MEASURED banner.** `:648` says "Every cell below was MEASURED against the pinned reference image";
`:678-680` then generalizes to entire ranges. `110.1/SPEC.md:41-44` measured four points in those
ranges (`200`, `201`, `204`, `301`). envoy-rust's exhaustive `u16` sweep pins **envoy-rust**, not
upstream. The rule is almost certainly right; it is a derivation, and the banner does not say so.

**N-7 — §C's second encoding row is a lossy transcription of its own source.** The contract row
reads `` q"b s\l t~t dd `` → `` q"b s\l t%7Et dd ``; the measured probe at `110.1/SPEC.md:113` is
`q"b s\l t~t d<0x7F>d` → `q"b s\l t%7Et d%7Fd`. The raw `0x7F` byte was dropped in transcription,
leaving `dd`. Nothing is factually wrong — the literal string as written does encode as shown, and
`:735` restores the `0x7F` cell on its own row — but the row no longer corresponds to the
measurement it is drawn from.

**N-8 — "survives in its original leading position" is true of upstream and false of envoy-rust.**
`:783-784`. `synth_overflow` (`hcm.rs:2471-2483`) appends `x-envoy-overloaded` **last**, with the
comment "`x-envoy-overloaded` goes AFTER the 5 standard headers (wire order)". The outputs coincide
only because it is the sole pass-through once `content-type`/`content-length` are dropped and
`date`/`server`/`connection` are relocated. The phrase reads as an invariant of the transform and
is not one — and it is the same surface `110.1` M-5 keeps open.

**N-9 — the section omits the access-log and stats consequence, which ADR-0179 calls the seam's
deciding fact.** ADR-0179 DECISION 2 placed the seam **before** the access-log / per-class-stats
derivation because upstream logs `%RESPONSE_CODE%` = 200 and ticks `downstream_rq_2xx` for a
transformed reply. `110.1/PLAN.md:181-190` carries the measured lines and counters. None of it
appears in `## gRPC`, and the `## Access log field mapping` and `## Stat-name mapping` sections are
silent about gRPC — so the measured observable that justifies the whole seam placement lives only in
phase docs, not in the canonical contract. Related: three sibling sections (`:36` no-healthy-upstream,
`:3325`/`:3328` redirect shape, `:964` `downstream_rq_5xx`) state wire shapes unconditionally that
the gRPC transform overrides, with no cross-reference in either direction.

**N-10 — record-accuracy items in `PROGRESS.md`, none load-bearing.** (a) It **never names the CI
run ID** — `grep '3263'` returns 0; the identifier lives only in `STATE.md`, so a reader of
`PROGRESS.md` alone cannot locate the evidence it relies on. (b) `:495` states the contract file went
`4046 → 4306` lines; `wc -l` gives `4045 → 4305` (a `.split('\n')` trailing-element artifact), and
the same file's own state-4 census row says 4305 — so it self-contradicts. The delta (260) and the
numstat (`260 0`) are both correct. (c) `:935`'s heading "all 88 pre-existing fixtures still green"
over-states what was observed locally (five pre-existing tests were RED); the body immediately says
so and routes the claim to CI, correctly. (d) "CI on this exact tree" (`:886`) cites a run on
`6af7649` while the session's tree was `84e9301`; the two differ by `1 0` on `STATE.md` only, so the
code is byte-identical and the conclusion holds. (e) the census row label `grpc.rs 709` is ambiguous
— `crates/envoy-http2/src/grpc.rs` also exists, at 453 lines.

**N-11 — two small over-scopes.** (a) `:659-660` "the first fixture in the corpus that sends
`content-type: application/grpc` at all" — true of fixture *file contents* (`grep -rl` returns only
`0089`) and of downstream probe requests, but fixture `0075-upstream-grpc-health-check` sends that
header on the **upstream health-check** path (`:600-601` of this same file describes it). Sharper as
"the first fixture whose downstream probe requests carry it." (b) `:653-654` says a header-dict
client "destroys the response header ORDER and CASE that §D turns on" — §D states an order rule and
a name/value table but **no case rule**; the only case rule in the section is §B's request-side one.

**N-12 — `g-200-maps-to-unknown`'s status axis is vacuous by construction.** The route is
`direct_response: 200` and the transform's output status is also `200`, so `expected_status: 200`
cannot distinguish transformed from untransformed. The probe is carried entirely by
`expected_body: ""` against the route's `"B200"` — which is a fixture-declared absolute, so the cell
is sound today. Recorded because a future edit that relaxed `expected_body` there would silently
vacate the probe.

---

## §5 — Severity dissent, and subagent findings DOWNGRADED on re-verification

Five read-only reviewers ran; every finding they returned was re-derived here before entering this
document. Four were **downgraded** and one materially **reframed**. Recorded by name, because a
review that silently launders its subagents' severities is not a review.

**DOWNGRADED: "`location` survival is unwitnessable" — Must-Fix → Minor (M-8).** The structural
half is correct and I confirmed it: `diff_headers` compares the union of what the two proxies
actually sent, so a header absent from **both** sides is not a mismatch, and `Http1HeaderRule`
carries no expectation to fall back on. The conclusion — "the pair asserts nothing whatsoever about
`location`" — does not follow. The oracle is a **digest-pinned** image that demonstrably emits
`location` on a redirect and cannot regress without a pin-bump phase. The failure mode the fixture
exists to catch, envoy-rust dropping `location` unilaterally, **does** red the name-set check. What
remains is a real gap in the *mutation* set, which is what M-8 says. Grading this Must-Fix would
have sent a green, correct fixture back to state 3 over a theoretical mutual-failure mode that no
differential fixture in the corpus can see, for `location` or for anything else.

**DOWNGRADED: the `--all-targets = 149` arithmetic — Must-Fix → Minor (M-7).** The measurement is
right and I confirmed it independently (`166 − 16 = 150`). But §5.2's consequence is re-entry to
**implementation**, and a stale form-dependent side figure in a record file is not an implementation
defect: it changes no code, weakens no gate, and the identity it sits beside (`passed + failed =
2228`) is measured and correct. Minor.

**DOWNGRADED: "the first fixture that sends `content-type: application/grpc` is contradicted by
0075" — Minor → Note (N-11a).** `grep -rl 'application/grpc' tests/fixtures/` returns **only
`0089`**, so the claim is literally true of fixture file contents, and true of downstream probe
requests. Fixture `0075` sends that header on the **upstream health-check** path, generated by
code rather than by the fixture. "Contradicted" overstates it; the sentence wants a scope, not a
correction.

**DOWNGRADED: "`application/grpc-web+proto` is a coverage gap" — Minor → Note (N-2).** The
constructed counterexample implementation is sound and I accept it as the reason this is the one
cell worth adding if any ever is. But F3 does not require it, and it is pinned in-process at
`grpc.rs:262`. Note.

**REFRAMED: CF-110-6's absence bullet.** Two reviewers reached the same place from opposite
directions — one asked whether the absence was forced (it is), the other whether the probe set
covered F3 (it does). The finding is neither: the absence is forced and F3 is met, but the *stated
scope* is wrong and the fixture's own probe 32 is the counterexample. That is M-2, and the decisive
evidence — `synth_redirect`'s doc block having recorded the rule since phase 76.2, and the
corpus-wide "no fixture has ever asserted headers on a 404" census — was measured by this session.

**One reviewer disclosed writing two scratch files outside the repo** while reporting the tree
untouched. Verified: `git status --porcelain` is empty and no tracked file changed. The disclosure
was correct and the constraint held.

---

## §6 — Deliberate decisions verified, not re-litigated

Each of these looks like an omission until measured. Each is right.

- **No access-log arm in `0089`** (ADR-0180 DECISION 5, CF-110-9). `110.1/REVIEW.md` §9 item 3 named
  `110.2` as the natural home for a `%RESPONSE_CODE%`/`%BYTES_SENT%` witness. It cannot ride in this
  fixture: an `expectations.yaml` has exactly one `driver`, `Http1ProbeList` never reads an access
  log, and `AccessLogByteExactProbe` carries no `expected_headers` — so it cannot pin `grpc-status`,
  which is the entire witness `0089` exists to provide. Structural, not preferential. Correctly
  banked rather than silently dropped. **N-9 stands alongside this**: the fixture could not carry the
  witness, but the *contract section* could still have recorded the measured observable, and did not.
- **No `201`/`3xx` `direct_response` cell** (CF-110-3). `location` is emitted at exactly one site in
  the H1 HCM — `hcm.rs:2435`, inside `synth_redirect`. `synth_direct_response` delegates to
  `synth_with`, whose five-header vector has no status-conditional arm. envoy-rust **cannot** emit
  `location` on a `direct_response` at any status. Forced. And it costs the fixture nothing: the 3xx
  default arm is recovered through the `redirect:` probe.
- **No `header_mutation` `grpc-status` cell** (CF-110-8). The sentinel (`grpc.rs:158-160`) returns
  **before** every rewrite, and the filter encode hook (`hcm.rs:1427`) runs 64 lines before the seam
  (`:1491`), so an operator-injected `grpc-status` genuinely reaches it. Including the cell would red
  on a real divergence. Also unreachable route-scoped: `PerFilterConfig` has three arms and
  `HeaderMutationPerRoute` is not among them.
- **Two-sided V2 with a symmetric mutation.** Correct, and for the stated reason: V2 mutates what is
  **sent**, and its witness is `expected_status`, a fixture-declared absolute checked against both
  proxies — not the cross-proxy `diff_headers`. It does not have V1's symmetry defect.
- **`body: { inline_string: "" }` rather than a bodiless `direct_response`** (CF-110-7). A bodiless
  `direct_response` is boot-fatal here and accepted upstream, so the byte-identical pair could not
  express it. The chosen spelling covers the same contract cell.
- **Nothing fixed.** CF-110-1…CF-110-9 and the `110.1` M-1…M-9 + N-1…N-10 are all still open, and
  `git diff --name-only 0b6f2f6 HEAD` returns **zero** files under `crates/`. §6.3 and ADR-0165 hold.

---

## §7 — Status of already-banked findings — read BEFORE grading, NOT re-issued

None was fixed, and none should have been. Listed so a later reader does not mistake this review's
silence for resolution.

- **CF-110-1 (NARROWED)** — H2 gRPC local replies UNBUILT; upstream shape measured (headers-only,
  `content-length` OMITTED). Open. `0089` is H1-only and does not touch it.
- **CF-110-2** — proxied responses untransformed. Open; `0089` is cluster-free, so every reply is
  local and the proxied direction is not exercised here at all.
- **CF-110-3 (REASSIGNED, WIDENED)** — `location` on a `201`/`3xx` `direct_response`. Open, **binding
  and correctly obeyed** by `0089`'s config. See **M-5**: it is the one carry-forward the contract
  section never records.
- **CF-110-4** — `synth_with`'s non-gRPC header ORDER differs from upstream. Order-only, invisible to
  `diff_headers`. Untouched.
- **CF-110-5** — the io_uring local-reply seam is unwitnessed by any test. **Still open after this
  sub-phase** — re-derived here: the differential harness has no `io_uring` reference, so `0089`
  exercises the tokio funnel only (N-5).
- **CF-110-6** — `content-type` on an empty-body local reply. Open; scope mis-stated (M-2), and its
  true reach is every `synth_with`-derived reply, not just `direct_response`.
- **CF-110-7** — `direct_response.body` mandatory here, optional upstream. Open; shapes the config.
- **CF-110-8** — the idempotence sentinel suppresses the whole transform where upstream transforms
  and lets the operator's value win. Open, and correctly **promoted from UNMEASURED to MEASURED** —
  this closes `110.1/REVIEW.md` §9 item 2 in its stronger form.
- **CF-110-9** — no access-log witness of the seam placement. Opened by ADR-0180, still open.
- **`110.1` REVIEW M-1…M-9 + N-1…N-10** — all carried unchanged. M-1 in particular is
  **unwitnessable by this deliverable**: its cell is the HTTP reason phrase, and the H1 driver reads
  `httparse`'s `resp.code` only, exposing status as a bare `u16`. Deciding it in was correctly
  rejected. **M-5 is the one this sub-phase brushes against** — see M-6.
- CF-109-1 (WIDENED)/2/3, CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6,
  CF-74-1/2/3/4/6, CF-73-1, the `109.2` REVIEW's M-1…M-8 + N-1…N-11, the `109.1` M-5 + N-1…N-6 set,
  the `108.2` M-2 + N-1…N-6 set, and the HTTP-filters-family (1)-(4) — **all carried unchanged.**

---

## §8 — Disposition of the five deviations on the record

Each re-derived from disk rather than accepted.

**D-1 (SUBSTANTIVE) — CONFIRMED, and its origin traces further back than the PLAN.** `PLAN.md:621`
specified V1 as a mutation of `/m-403`'s status in **BOTH** yamls; run as written it returned GREEN.
The mechanism reproduces under independent reasoning: header assertions are purely cross-proxy, so a
symmetric mutation moves both proxies in lockstep and every axis of the cascade still passes. The
correction to one-sided is right, and the RED then named `g-403-maps-to-7`. **This review adds the
upstream cause** (M-3): `110.2/SPEC.md` F6's own example — "flipping one mapped code in the
expectations" — is impossible, because no mapped code exists in the expectations. Told to do
something unsatisfiable, the PLAN reached for the YAMLs and reached for both. The PLAN also
contradicted itself, since its own V3 already carried the one-sided shape.

**D-2 (METHOD) — CONFIRMED, and independently corroborated from committed history.** `git checkout
--` destroyed Task 2's eight uncommitted probes and produced a false green on the wrong file. Both
md5s the record quotes reproduce exactly from the tree: `a4138c1`'s 15-probe file hashes
`1581969a…`, and `d006d25`'s 23-probe file hashes `c7bb67c1…` — the latter being the decisive one,
because it shows the re-applied block was byte-identical to what Task 2 finally committed. The
generalization the record draws is a genuine strengthening of the known hazard and belongs in the
standing rules: `git checkout --` is a **destructive** no-op-looking revert on a TRACKED file
carrying uncommitted work, and is safe only when the file's non-mutation content matches `HEAD`.

**D-3 (ADDITION) — CONFIRMED and endorsed.** V4 is not redundant with V1. `diff_headers`'s value
loop is generic over names, so V1 proves the *branch* fires but says nothing about whether
`grpc-message` is in the vectors at all; presence is name-specific. V3 proves the name-set branch
fires for `grpc-message` but says nothing about its value. The three header mutations occupy three
distinct squares of a {branch} × {header} grid. Adding V4 was correct.

**D-4 (METHOD) — CONFIRMED, and its residue is this review's M-1.** Drafting the contract section by
subagent and then grading it caught three wrong claims. It did not catch a fourth, and could not
have by inspection: the ADR-0059 citation was **true at grading time** and was falsified by the
insertion itself. The grading step was right to exist and right to be distrustful; the failure is
that a self-referential line number cannot be validated against the pre-insertion file.

**V-1 (state-4) — CONFIRMED independently.** `git show 6af7649:.../PROGRESS.md | wc -l` = **713**,
the state-4 numstat on that path is `349 0`, and 713 + 349 = **1062** on disk. It was never 671.
Archiving it verbatim rather than correcting it in place was correct under ADR-0035, and stating the
correction in the replacement block was the right disposal. **M-7 is the same failure mode caught one
layer out** — a handed count is a claim even when the ledger hands it, and `--all-targets = 149` is
the count the ledger is still handing.

---

## §9 — Carry-forwards for the state-6 close-out to bank

Nothing here is an obligation on the close-out itself, which flips two ROADMAP status cells and
relocates a `STATE.md` Notes subsection and nothing else.

**Citation and record accuracy — the cluster this review is really about:**
1. **M-1** — `BEHAVIOR_CONTRACT.md:887-888` should cite `:1391-1397`, or better, cite the ADR-0059
   bullet **by text**. `PROGRESS.md`'s D-4 record carries the same three pre-insertion coordinates.
2. **M-5** — add CF-110-3 to the `## gRPC` section and qualify §D's `location` row; §H's "Two
   ADJACENT recorded divergences" should read three.
3. **M-6** — "three independent pass-through cases" → "three independent cases (two carrying a
   pass-through header)"; as written it makes `110.1` M-5 look closed.
4. **M-7** — `--all-targets` is **150** at HEAD, not 149; the gap is 16 only against a plain count of
   165. Anyone re-quoting the pair must re-derive it.
5. **M-2** — CF-110-6's scope is the `synth_with` family, and its real reach is `synth_400`,
   `synth_404` (both arms) and `synth_501`. The reason it went uncaught is that **no fixture in the
   corpus asserts response headers on a 404**, not the `inline_string: ""` census.
6. **M-4** — the `§G one-path-per-probe` citation is false and now collides with the new §G. The
   origin is `110/SPEC.md:387`, landed and uneditable; fixing only the copy leaves the origin
   asserting it.

**Cheap test-adequacy debt, in the order a later slice should take it:**
7. **M-8** — the one-sided V5 on `envoy.yaml`'s `path_redirect` (anchor count 1). One `sed`, and it
   closes `location` presence, `location` value, and survival in a single RED.
8. **N-2** — if one detection cell is ever added, `application/grpc-web+proto` is the one; it is the
   only unwitnessed cell testing an equivalence class the shipped five do not cover.
9. **N-3** — `te: trailers` independence has no guard on either layer. One probe or one unit test.
10. **N-5** — the filter-generated local-reply family, which ADR-0179 deliberately widened the
    transform to cover, has **zero** differential witness. A `0089`-shaped fixture with one filter in
    the chain would close it, and would also be the natural home for CF-110-9's access-log arm.
11. **N-9** — the access-log/stats consequence belongs in the canonical contract, not only in
    `110.1/PLAN.md`. It is the fact that decided the seam placement.

**Structural, for whoever writes the next contract section:**
12. A line citation **into the file you are editing** is invalidated by your own edit, in the same
    commit. Cite by text, or verify after the insertion. M-1 is the first recorded instance.

---

## §10 — Assessment

`110.2` is a small slice that did the hard part well. Its job was to take a transform proved
in-process and make it visible to the differential harness, and the interesting problem was never
"write 32 probes" — it was working out what a differential fixture can and cannot say, and then
saying so out loud. The deliverable's best property is that it **names the limit of its own
witness**: `Http1HeaderRule` is a unit variant, so no fixture can assert a header value, so every
`grpc-status` and `grpc-message` assertion in `0089` is cross-proxy agreement rather than an
absolute. That is disclosed four times over, including a standing corpus-wide prohibition against
allow-listing `location` or `content-type` to make a probe pass. A fixture that quietly enjoyed the
appearance of pinning the mapping table would have been easier to write and much worse.

The division of labour with `110.1` is the right one and worth stating, because neither half is
sufficient alone. `110.1` pins the table **absolutely and exhaustively** — all 256 byte values
against the encoder rule, the entire `u16` range asserting exactly eight specials — which makes a
"helpful" extra arm unlandable. `110.2` pins **agreement with a digest-pinned upstream**, which is
the only thing that can catch the table being right about the wrong contract. Together they cover
both failure directions.

Three design choices deserve credit. The 3xx default arm is witnessed *through* the constraint that
forbids expressing it, by routing it through a `redirect:` route that also carries the `location`
cell. The empty-body rule is witnessed at two structurally different local-reply builders rather
than twice at the same one, picking up a site `110.1` M-3 had flagged as undriven. And the two
encoding probes target exactly the two places the obvious rule fails — `%25` → `%2525` and
`~` → `%7E` — rather than sampling the rule's easy middle.

The defect profile is narrow and has one shape: **every finding is in a citation or in a stated
scope, and none is in a probe or in the code.** Three recur. First, a scope that is broader than the
thing it describes — CF-110-6 stated for "an empty-body local reply" when `synth_redirect` has
complied since phase 76.2 and the fixture's own probe 32 proves it (M-2); "the rest of the
local-reply family" when one member already obeys. Second, a transcription stronger than its source
— "three independent **pass-through** cases" where the source said "three independent cases", which
makes a live banked finding look closed (M-6). Third, a number that was true when written and false
later: `--all-targets = 149`, correct at a plain count of 165 and carried past the commit that made
it 166 (M-7).

**M-1 is the one worth remembering.** The `## gRPC` section's three citations into other files are
all exact; its single citation into its own file is wrong by exactly the section's own length. It
was true when ADR-0180 wrote it, and the commit that transcribed it is the commit that broke it. The
repository already knows "locate by text, line numbers drift" — this is the sharper case, where no
external event is required and no amount of care at drafting time would have caught it, because the
citation was valid until the moment the surrounding insertion landed. That is a genuinely new hazard
for a project whose canonical document is edited by topical insertion rather than by appending.

Nothing found changes a byte of wire behaviour. Nothing found weakens a gate. Nothing found is worth
three more sessions under §5.2. The 32 probes witness what F3 asks and more, the four mutations
prove four distinct assertions with byte-exact reverts, the two YAMLs are byte-identical, and CI is
green on the exact tree with fixture `0089` demonstrably executed.

**Gate (f) is CLOSED. All six §7.5 gates are now GREEN. Sub-phase `110.2` is approved to land.**

### STOP CONDITION — re-derived from disk at this review, ALL THREE LEGS

The mission is complete only when EVERY ROADMAP row is `done` AND no in-scope leaf remains. **ALL
THREE LEGS MUST HOLD. IT IS NOT COMPLETE.** This is the **sixty-second** consecutive evaluation.

- **Leg (i) — FALSE.** 116 rows / **114 `done`** / 1 `in-progress` / 1 `planned` (split on `' | '`,
  status = FIELD 4). The two not `done` are `110` (`in-progress`, the gRPC-family opener) and
  `110.2` (`planned` — it does not flip until its own state-6).
- **Leg (ii) — FALSE**, by direct tree probes rather than by the ledger's assertion: **14** crates
  (`envoy-accesslog envoy-admin envoy-bin envoy-cluster envoy-config envoy-filter envoy-health
  envoy-http1 envoy-http2 envoy-jwt envoy-listener envoy-stats envoy-tcp envoy-tls`), with no
  `envoy-http3`/`envoy-grpc`/`envoy-wasm`/`envoy-protos`/`envoy-runtime`;
  `grep -rl '^quinn' crates/*/Cargo.toml` = **0**, `tonic-web` = **0**, `wasmtime` = **0**;
  `tests/conformance/` holds only `h2spec/`; `runtime_key_is_rtds_inert` still on disk. The unbuilt
  set remains the gRPC DATA path, `RuntimeUInt32`/CSRF honoring, RTDS, hot restart/graceful-drain,
  network-filter payload codecs, `sni_cluster`, non-deterministic + priority/panic/locality LB,
  HTTP/3 + QUIC, the observability sinks and the WASM host.
- **Leg (iii) — FALSE.** Heading-slice census over all eleven `### ` family headings:
  `10/5/3/14/0/3/6/29/6/0/13 = 89 under headings + 27 before the first heading = 116`, with **TWO**
  zero-row headings (`### HTTP/3 + QUIC family`, `### WASM host family`).

**NO `stop` FILE WAS CREATED**; `ls stop` returns `No such file or directory`.

### Next state

**§5 state 6 — the close-out**, a SEPARATE session per §5.1 and ADR-0127 (a reviewer must not close
out what it graded). At that close-out **ROADMAP rows `110.2` AND parent `110` flip `done`
TOGETHER** — the `76.2`/`108.2`/`109.2` two-row precedent, **unlike** the `110.1` close-out which
flipped one row only, because `110.2` is the last sibling and its close therefore **closes the whole
gRPC-opener phase `110`**. Assert both rows' starting status first. The
`### Sub-phase 110.2 §5 state-5 code review` Notes subsection is retired to `STATE_HISTORY.md`, and
**no ADR and no new Notes subsection is added** — both are measured precedents. The next-phase
state-0/1 pick is its own session after that; a close-out and a pick are never chained.

This review **fixed nothing**, as ADR-0165 requires, and touched no `ROADMAP.md` line, no landed
artifact, and no file under `crates/`.
