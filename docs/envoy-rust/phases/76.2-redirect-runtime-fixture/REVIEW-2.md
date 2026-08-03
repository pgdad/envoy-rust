# Sub-phase 76.2 — REVIEW-2 (§5 state 5, the FRESH RE-REVIEW)

> **What this file is, and why it is not `REVIEW.md`.** This is the §5 **state-5 re-review** of
> sub-phase `76.2-redirect-runtime-fixture`, conducted after the §5.2 re-entry answered round 1 and
> after the state-4 re-verification re-ran the §7.5 gate. **It SUPERSEDES the round-1
> `REVIEW.md`** for the purpose of gate (f).
>
> `REVIEW.md` is a **landed artifact and was NOT edited** (D-3.5). Its verdict still reads
> `CHANGES-REQUESTED` and always will — a landed review is never rewritten. That verdict was
> answered: Issue **I-1** and Minors **M-1..M-4** were fixed at the §5.2 re-entry
> (`PROGRESS.md` §R3.0-§R3.7), and the full gate was re-run at the state-4 re-verification
> (`PROGRESS.md` §V4.0-§V4.11). **Do not read `REVIEW.md`'s verdict as a live instruction to
> re-enter at state 3.**
>
> Written for a reader with **zero prior context** (doctrine D-3.4).
>
> **This session did NOT re-run the §7.5 gate.** Gates (a)-(e) were run and adjudicated at the
> state-4 re-verification and their real output is quoted in `PROGRESS.md` §V4.1-§V4.6. Gate **(f)
> is the one gate this review owns and the one it closes.** **This session fixed nothing it
> graded** (ADR-0127; ADR-0165). No ROADMAP status cell was flipped; no `SPEC.md`, `PLAN.md`,
> `PROGRESS.md`, `REVIEW.md` or `76.1` artifact was edited. No `cargo` command was run.

---

## 1. VERDICT

**APPROVED.**

**0 Critical · 0 Issue · 7 Minor · 15 Nit** (all NEW this round; round 1's banked findings are
listed separately in §7 and are **not** re-issued here).

Round 1's single Issue is **genuinely and well fixed**, and the fix is *better* than the shape the
review suggested. The RDS reload classifier now buckets both widened `ConfigError` variants into
`update_rejected`; the `other => unreachable!(…)` forcing function was **kept** rather than deleted;
and three `reload()`-level tests — two reject-direction, one accept-direction control — pin the warm
reject semantics rather than merely "no panic". **I independently enumerated the producer's
returnable set and the classifier's new comment claiming "the six variants matched above" is
numerically and factually correct** (§3.1).

Nothing on the request/response wire path is wrong. All 22 measured `location` cells are pinned and
green; fixture `0086` is green differentially against the pinned upstream image at 19 probes; the
test census reconciles exactly (§3.2).

The seven Minors are **coverage gaps and claim-vs-reality doc mismatches, not defects in shipped
behaviour**. None changes a wire answer. Two of them (M2-2, M2-3) are the *same class* as round 1's
M-2 — a measured or invented cell with no witness — which is precisely the class this phase has
already proven it takes seriously. They are **banked for a future phase** (§6.3), not required for
this sub-phase to land: a reviewer must not fix what it grades, a close-out flips only the status
cell, and there is no remaining implementation state in `76.2` to schedule them into.

**Per §5 this routes to state 6, the close-out.**

---

## 2. How this review was conducted

Five **read-only** review dimensions were fanned out in parallel (the state-5 parallelism rule:
subagents return CITATIONS; the main session makes every decisive measurement itself). The
dimensions: the I-1 fix in `rds_watcher.rs`; fixture `0086` at 19 probes; the `plan_redirect` /
`synth_redirect` wire path; the `BEHAVIOR_CONTRACT.md` Phase 76 bank; and test quality + census
integrity. Each was told it must not write, must not run `cargo`, and must not mutate the tree.

**Every finding below was RE-VERIFIED ON DISK by the main session before being written down.** A
subagent finding is a claim, not a result. Findings that did not survive re-verification, or that
turned out to be round-1 banked items, were dropped — see §7.

Three method notes from this round:

- **Two dimensions independently reached M2-2** (the `port_redirect` witness gap) from different
  directions — one by censusing probes against routes, one by hand-executing a `strip_port`
  deletion against all 19 probes. Independent convergence on an unprompted finding is the strongest
  signal a fan-out produces, and it is worth more than either report alone.
- **Three dimensions surfaced findings that were already banked** (round 1's M-7, M-9, N-4, N-5,
  N-8, N-9), two of them graded ISSUE. A fresh reviewer with no memory of round 1 will re-derive
  round 1's findings — **the guard is reading the prior review, not trusting the dimension's
  severity.** They are recorded in §7 as still-open, not re-graded.
- **The census cross-check closed exactly**, which is the single strongest evidence that no test was
  lost across the re-entry (§3.2).

**Scope reviewed:** the whole sub-phase, `git diff 0ea2de1..HEAD` — 13 non-`docs/` files,
**+1643 / −75** — with particular weight on the re-entry commit `32a4c52` (`5782a06..32a4c52`),
which is the code round 1 never saw.

---

## 3. What I measured myself

### 3.1 The I-1 fix is complete, and "six" is correct

Round 1's I-1: Task 8 added a `validate_redirect_oneofs` call inside
`reparse_and_select_route_config`, widening its returnable `ConfigError` set; the sole production
caller's classifier matched four variants and ended `other => unreachable!(…)`; `Cargo.toml:42` is
`panic = "abort"`. An RDS hot reload of exactly the config CF-76-2 was filed about would abort the
proxy.

I walked every fallible step of the producer myself rather than inheriting the claim:

| step | `crates/envoy-config/src/rds.rs` | variant |
|---|---|---|
| 1 read | `:109` `read_to_string(..).map_err(..)` | `RdsFileError` |
| 2 parse | `:114` `parse_rds_file(..)?` → its single `map_err` at `:56` | `RdsParseError` |
| 3 select | `:120` `ok_or_else(..)` | `RdsRouteConfigNotFound` |
| 4 route arm | `:153` `return Err(..)` | `UnknownCluster` |
| 4 redirect arm | `:159` `validate_redirect_oneofs(..)?` → `bootstrap.rs:2682`, `:2688` | `RedirectPathRewriteConflict`, `RedirectSchemeRewriteConflict` |
| 4 direct-response arm | `:168` `=> {}` | — |

`parse_rds_file` has exactly one `map_err` and no other `Err` exit. `validate_redirect_oneofs` has
exactly two `return Err` sites and no `?` inside it. The `match &route.action` at `:150-169` is
**exhaustive with zero `_ =>` catch-all** (measured: 0). So the returnable set is exactly **six**,
and all six are matched at `rds_watcher.rs:202-222`. **The comment at `rds_watcher.rs:225` is
correct.**

The fix's shape is right on two counts worth naming. It put the two variants in `update_rejected`
alongside `UnknownCluster` — the correct bucket, since the file was read and parsed fine and it is
the *content* that is refused. And it **kept** the `unreachable!()` arm: the tempting reading of I-1
is "the panic was the bug, delete it", but the bug was a widened producer with an unextended
consumer, and the arm is still the only forcing function for a seventh variant.

### 3.2 The test census reconciles exactly

Re-derived independently by the main session from `git diff -U0 0ea2de1..HEAD -- '*.rs'`, counting
`#[test]` / `#[tokio::test]` attributes on both sides:

| file | added | removed |
|---|---|---|
| `crates/envoy-config/src/rds.rs` | 2 | 0 |
| `crates/envoy-http1/src/hcm.rs` | 7 | 0 |
| `crates/envoy-http1/src/rds_watcher.rs` | 3 | 0 |
| `crates/envoy-http1/src/response.rs` | 1 | 0 |
| `crates/envoy-http2/src/hcm.rs` | 1 | 0 |
| `tests/differential/tests/route_redirect_action.rs` (NEW binary) | 1 | 0 |
| **total** | **+15** | **−0** |

Zero `#[ignore]` added. Exactly one new test binary. Against CI:

```
tests   2137 (before 76.2)  + 15 = 2152 (CI on HEAD)   ✓
binaries 162 (before 76.2)  +  1 =  163 (CI on HEAD)   ✓
```

**Both identities close exactly.** No test was lost, double-counted or silently disabled across the
re-entry. (Note the +15 does not mean coverage grew by 15 units: two of the fifteen fns — the 22-row
table and the 19-probe fixture — carry most of the discriminating power.)

### 3.3 Nothing the handoff flagged as "must not regress" was regressed

All eleven re-verified on disk: probe `q02` and its `/q2-hostport` route present; the M-1 pin test
present; both new variants in the `update_rejected` arm; the `unreachable!()` arm kept;
`assert_eq!(cells.len(), 22)` at `hcm.rs:10720` intact; `synth_redirect`'s header assertion still
exact `Vec` equality (`hcm.rs:10805`), not a `contains`; `synth_501` still defined and consumed
(`hcm.rs`, `uring.rs`); no live `if let` over `RouteAction` in `rds.rs` (the single grep hit,
`:408`, is a test doc comment quoting the *superseded* code — adjudicated **by line**, not by
count); `HEADER_ALLOW_LIST` still exactly three entries with `location` absent (0 hits for
`"location"` in `tests/differential/src/lib.rs`); `known-failures.txt` still 21 lines; no `stop`
file. `ci.yml`, `deny.toml` and `known-failures.txt` are **byte-unchanged across the whole
sub-phase**.

### 3.4 CI is green on the reviewed commit

Run `30758812544` on the full 40-char SHA `5dedb278fb2700871e727e04178535090f1d5f46`: both jobs
`success`, `build + test + lint` at **15** steps, `fuzz` at **13**, both on real runners
(`runner_name` non-empty — not the starvation signature).

---

## 4. MINOR (real, recorded, NOT required for this sub-phase to land)

### M2-1 — the I-1 fix's cross-crate warning is on the wrong file, and the next widening is already scheduled

**`crates/envoy-http1/src/rds_watcher.rs:230-234`**; the gap is at
**`crates/envoy-config/src/bootstrap.rs:2666-2676`**.

The fix added exactly the right lesson to the classifier:

> `// NOTE (I-1): "fail loud" here means ABORT in release. Widening the producer's returnable set is`
> `// a CROSS-CRATE change — when you add an Err arm to reparse_and_select_route_config, grep its`
> `// callers, because the compiler will NOT tell you: unreachable!() compiles clean.`

But that is **not how I-1 was created**, and it is not how it will recur. I-1 was created by adding
a *call* to a validator whose error set lives in a different file in a different crate. The
realistic next widening is a third `return Err(..)` **inside `validate_redirect_oneofs`
(`bootstrap.rs:2677`)** — which touches neither `rds.rs` nor `rds_watcher.rs`, produces no compile
error, and lands the seventh variant in `other => unreachable!(…)`. **Reproducing I-1 exactly.**

**This is scheduled, not hypothetical.** `RedirectAction` (`bootstrap.rs`, 8 fields) has **no**
`regex_rewrite`, and `SPEC.md:477` lists *"`regex_rewrite` inside `redirect`. Measured working
upstream; excluded to hold the LoC gate"* as an explicit deferred non-goal. `regex_rewrite` is the
third member of upstream's `path_rewrite_specifier` oneof, and `path_redirect`+`regex_rewrite` is
already a MEASURED upstream rejection. So the phase that lands it will add a third check to
`validate_redirect_oneofs` — and `validate_redirect_oneofs`'s own doc block carries **no** reciprocal
"widening my error set is a cross-crate change" warning.

**Fix shape.** One sentence on `validate_redirect_oneofs`'s doc naming `rds_watcher.rs`'s classifier
as a consumer that must be extended in lockstep. The warning belongs on the file the next editor
will actually open.

### M2-2 — no differential witness for `port_redirect` overriding a request-carried port (contract cell Q4)

**`crates/envoy-http1/src/hcm.rs:2302`**; fixture
**`tests/fixtures/0086-route-redirect-action/expectations.yaml:60,95`**.

The same class as round 1's M-2, one cell over — and on a cell the contract records as **measured**
(`BEHAVIOR_CONTRACT.md:2998`, row **Q4**: `Host: envoy-rust.test:1234` + `https_redirect` +
`port_redirect: 443` ⇒ `https://envoy-rust.test:443/n-hport/y`).

Measured on disk:

- Routes setting `port_redirect`: **2** — `/i-port` (`envoy.yaml:38`) and `/n-hport` (`:51`).
- Their probes are `r09` and `r14`, **both with the unported `host: "envoy-rust.test"`**.
- The three probes sending a ported `Host:` (`q01`, `q02`, `q03`) set **no** `port_redirect`.
- **Probes combining `port_redirect` with a ported `Host:` — 0.**

Consequence, hand-executed against all 19 probes: deleting the `strip_port` call at `hcm.rs:2302`
(`format!("{}:{}", host_part, port)`) leaves `r09` at `example.com:8443` and `r14` at
`envoy-rust.test:443` — **byte-identical**. The whole fixture stays green while a real
`Host: h:1234` request would emit `http://h:1234:443/…`. **`strip_port` is differentially vacuous.**

**Not unpinned, only uncompared** — the cell is pinned in-process at `hcm.rs:10689-10700` (`"Q4
port_redirect OVERRIDES the request's port"`). *Independently reached by two review dimensions.*

**Fix shape.** A 20th probe: a new route `/q4-hostport` with
`redirect: { https_redirect: true, port_redirect: 443 }`, probed with `host: "envoy-rust.test:1234"`.
No shadowing risk. Exactly the `q02` move that closed round 1's M-2.

### M2-3 — a THIRD invented cell exists, and unlike items 7 and 8 it is neither pinned nor banked

**`crates/envoy-http1/src/hcm.rs:2321-2326`**.

`rewritten_path` re-appends the request query **unconditionally** — `rd.strip_query` is never
consulted on that branch. The comment states it as a choice: *"`strip_query` is a location-side rule
only."*

Hand-executed: route `prefix: "/a"`, `redirect: { prefix_rewrite: "/r", strip_query: true }`,
`GET /a/b?k=v` ⇒ **`location: http://h.test/r/b`** (query dropped) while the access log records
**`path=/r/b?k=v`** (query kept). The two disagree about the query.

This is exactly the class §F items 7 and 8 exist for — a behaviour the implementation *chose* rather
than measured. But measured on disk:

- **Not measured.** No R/Q/E row combines `prefix_rewrite` with `strip_query`; the only two
  `strip_query` cells (R8, R13) use `host_redirect`.
- **Not pinned.** Zero tests set both (`hcm.rs` cells at `:10566` and `:10620` are the only
  `strip_query` sites, both `host_redirect`); the §F items-7-and-8 pin at `:10866` uses the default
  `strip_query: false` and says so at `:10895`.
- **Not banked.** §F (`BEHAVIOR_CONTRACT.md:3109-3129`) lists eight items; this is not among them.

**Fix shape.** Either one assertion in the existing §F pin test, or a ninth §F item. The §F closing
note's own lesson — *"a contract that disclaims a cell and then claims a safety net that does not
exist is worse than silence"* — applies to a cell it does not disclaim at all.

### M2-4 — "the two agree on the expected `location`" is false, in three shipped documents

**`docs/envoy-rust/BEHAVIOR_CONTRACT.md:3024-3025`**,
**`tests/fixtures/0086-route-redirect-action/README.md:41-42`**,
**`tests/differential/tests/route_redirect_action.rs:16-17`**.

All three justify `q02` not being a duplicate of `r01` with some form of *"the two agree on the
expected `location` / on the output but differ on the input `Host:`."*

**Measured false.** `r01` ⇒ `http://example.com/a-host`; `q02` ⇒ `http://example.com/q2-hostport/x`.
Different paths, different `location`. They agree on the **authority** (`example.com`, no port),
which is the component that actually carries the argument.

The contract **refutes itself nine lines apart**: `:3018` gives Q2's location as
`http://example.com/q2-hostport/x` while `:3025` says it agrees with R1's output.

**Root cause is a carried-forward justification.** It was true of the *original* Q2, which used
target `/a-host` (still visible in the in-process cell at `hcm.rs:10674`). The M-2 fix re-anchored
q02 onto its own `/q2-hostport` route — correctly, for no-shadowing reasons — and carried the old
reason along. The conclusion is sound; only the stated reason is wrong. **Fix:** "agree on the
expected *authority*".

### M2-5 — §F item 6 names the wrong trigger, so it does not bank the case it appears to

**`docs/envoy-rust/BEHAVIOR_CONTRACT.md:3121-3122`**:

> `6. Whether strip_port's rfind(':') handles a bracketed IPv6 literal authority correctly.`
> `   Pre-existing and used for vhost matching; a redirect echoing the authority may surface it.`

**Backwards.** In `plan_redirect`, `strip_port` is called at **exactly one** site — `hcm.rs:2302`,
inside the `Some(port) =>` arm. The authority-**echo** path is `:2303` `host_part.to_string()`, which
never touches `strip_port` and is therefore the one path that **cannot** surface it. The trigger is
`port_redirect` being set.

A future session following this note will probe the wrong input and conclude the concern is
unfounded. It also means the concrete reachable case — `host_redirect: "[::1]"` + `port_redirect:
443` ⇒ `location: http://[::443/…`, an unparseable URI — is **not** in fact banked by item 6
(round 1's M-6 records it; §F does not).

### M2-6 — `BEHAVIOR_CONTRACT.md:2963` still advertises the pre-fix probe count

**`docs/envoy-rust/BEHAVIOR_CONTRACT.md:2963`**, verbatim:

> `> Differential witness: fixture 0086-route-redirect-action (18 HTTP/1.1 probes, backend-free).`

Measured: **19** (`grep -c '^    - name: ' expectations.yaml`). `README.md:5`, `README.md:74` and
`route_redirect_action.rs:2` were all correctly updated to 19 by `32a4c52`; the contract preamble was
missed.

This is the observation the state-4 re-verification **deliberately recorded and did not repair**
(`PROGRESS.md` §V4.9) — correctly, since a verifier does not fix what it notices. **Graded here:
MINOR, not waved through.** It is doc-only and nothing depends on it, but it is the *witness count*
in the canonical contract, and it undercounts precisely the probe the M-2 fix added to close a
witness gap. I confirmed it is the **only** stale count in the range 2950-3200.

### M2-7 — two rustdoc comments still carry the pre-fix classifier taxonomy

**`crates/envoy-http1/src/rds_watcher.rs:48-49`** and **`:175`**.

The I-1 fix corrected the **inline `//`** comment at `:206-207` to the four-member set. Two `///`
**rustdoc** comments enumerating the same taxonomy were not updated:

- `:48-49` (`RdsCounters` doc) — *"a {name-not-found, unknown-cluster} rejection ticks
  `update_rejected`"*
- `:175` (`reload`'s own doc, three lines above the function the fix edited) — *"`update_rejected`
  for {route_config_name-not-found, unknown-cluster}"*

The arm at `:219-222` now lists four members. These are the higher-visibility docs (they render in
rustdoc) and `:175` sits directly above the edited function.

---

## 5. UNMEASURED EDGES — recorded as behaviour, NOT graded as divergence

This project does not read upstream source to decide what correct means (D-3.3). The following
inputs produce outputs no measurement covers. Each was hand-executed against the code. **None is a
finding**; they are recorded so a future measuring session has the list. Round 1 already banked the
first three as M-6 / N-8 / N-9.

| input | output | banked? |
|---|---|---|
| `host_redirect: "[::1]"` + `port_redirect: 443` | `location: http://[::443/…` (unparseable) | round-1 M-6; **not** in §F (see M2-5) |
| `path_redirect: "newpath"` (no leading `/`) | `location: http://envoy-rust.testnewpath` | round-1 N-8; not in §F |
| `path_redirect: "/new?a=1"` against `/a?k=v` | `location: http://h/new?a=1?k=v` (double `?`) | round-1 N-9; not in §F |
| `strip_query: true` + `prefix_rewrite` | location drops the query, logged `:path` keeps it | **nowhere — this is M2-3** |
| route `prefix: "/a?k"`, `GET /a?k=v`, `prefix_rewrite: "/r"` | `matched_len` (4) exceeds the query-stripped `path` (2); `str::get` ⇒ `None` ⇒ `""`; `location: http://h/r?k=v` | **nowhere — NEW, see N2-15** |

The last row is worth a sentence: `route_matches` compares the **raw target** (the open CF-76-1
asymmetry) while `plan_redirect` computes `matched_len` against the **query-stripped** path. `SPEC.md`
§4.2 keeps the *fixture* clean of CF-76-1 by using only `prefix:` routes — but `prefix:` routes have
their own CF-76-1 interaction *inside* `plan_redirect`. It is **total** (no panic) purely because of
`path.get(matched_len..).unwrap_or("")` at `:2320`, which is exactly the defensive choice the code's
own comment claims it for.

---

## 6. NITS

- **N2-1** `tests/fixtures/0086-route-redirect-action/README.md:35` — *"Nine routes set
  `host_redirect`"*. **Ten** do (`grep -c` → 10); the enumerated nine are the *other* nine.
  `route_redirect_action.rs:13` phrases it correctly as *"The other nine"*. Stale-dated by the M-2
  fix, which added the tenth.
- **N2-2** `tests/fixtures/0086-route-redirect-action/README.md:67` and
  `tests/differential/tests/route_redirect_action.rs:30` — *"probes `q01`/`q03` deliberately send
  `Host: envoy-rust.test:1234`"*. **Three** do now (`expectations.yaml:119`, `:126`, `:133`). Not
  false, but stale in the exact spot the M-2 fix touched.
- **N2-3** `docs/envoy-rust/BEHAVIOR_CONTRACT.md:2996` vs `crates/envoy-http1/src/hcm.rs:10671-10679`
  — the contract's row **Q2** was re-anchored onto `/q2-hostport/x` but the in-process 22-cell table's
  Q2 still uses `/a-host`. Both cells are individually true and individually measured, but the
  contract's Q2 is now the one row of 22 that the "22-row measured table" does **not** pin (it is
  pinned by fixture probe `q02` instead), and the recon's original `/a-host` cell no longer appears
  in the contract at all.
- **N2-4** `crates/envoy-config/src/rds.rs:88-93` — `reparse_and_select_route_config`'s doc
  enumerates each step *"with the failure class each error maps to"* and tags every other bullet
  (`:75` `(update_failure)`, `:79` `(update_rejected)`, `:83` `(update_rejected)`). The
  `RouteAction::Redirect` bullet added by 76.2 carries **no** counter-class tag — and it is the one
  step whose omission produced I-1.
- **N2-5** `crates/envoy-config/src/lib.rs:993` and `:1004` — both `Redirect*Conflict` variant docs
  say *"`listener` names the offending HCM"*, but the warm path passes `&format!("rds:{path_str}")`
  (`rds.rs:161`) — an RDS file path. `validate_redirect_oneofs`'s doc documents the dual meaning; the
  variant docs were not updated. Self-describing thanks to the `rds:` prefix.
- **N2-6** `crates/envoy-http1/src/rds_watcher.rs:503` — the helper comment says *"`body` is spliced
  verbatim"*; the parameter is `redirect_body` (`:505`). No `body` identifier is in scope.
- **N2-7** `crates/envoy-http1/src/rds_watcher.rs:134-135` — *"The closure never propagates a reload
  error — a bad RDS file must NOT take the proxy down."* True for all **six** currently reachable
  variants. False for a seventh: `unreachable!()` panics past the closure's `if let Err`, and
  `Cargo.toml:42` is `panic = "abort"`. Either the invariant claim or the `unreachable!()` should
  acknowledge the other. (The `unreachable!()` itself is the right call — see §3.1.)
- **N2-8** `crates/envoy-http1/src/rds_watcher.rs:662-665` and `crates/envoy-config/src/rds.rs:444-447`
  — both accept-direction controls stop at `matches!(…, RouteAction::Redirect(_))` and never assert
  the redirect's **content**. A warm path that installed `Redirect(RedirectAction::default())` —
  dropping every field — passes both. (The controls are still a genuine strength; this is a
  refinement.)
- **N2-9** `crates/envoy-http1/src/hcm.rs:10627` — cell labelled *"R14 a scheme change does NOT
  normalise a redundant `:443`"* sets `port_redirect: Some(443)` explicitly, so the `:443` comes from
  config, not from a request-carried redundant port. The stated property is genuinely covered by the
  **Q1** cell (`:10662`, `:1234` surviving an `https` switch). Only the label is wrong.
- **N2-10** `crates/envoy-http1/src/hcm.rs:9882` — the T6-1 dispatch test asserts
  `matches!(outcome, BuildOutcome::Synth(ref r, _) if r.status == 301)` and never checks the
  `location`, so it would pass if `prefix_rewrite` wrote the correct `req.path` but built a wrong
  `location`. Covered elsewhere by table row R5 (`:10529`); a locality nit.
- **N2-11** `crates/envoy-http1/src/hcm.rs:2287-2291` — the comment says *"`scheme_redirect` wins"*,
  but `validate_redirect_oneofs` (`bootstrap.rs:2688`) rejects both-present at load, so the
  precedence is **unreachable in any loadable config** and no cell sets both (measured: 0). Swapping
  the two match arms survives the whole suite. Same family as round-1 N-1 (a comment describing an
  unreachable rule) on a different line — do not double-count them.
- **N2-12** `crates/envoy-http1/src/hcm.rs:2122-2124` — `find_header(…, HOST).unwrap_or_default()` is
  dead: `hcm.rs:2058-2068` already returned 400 unless `Host` is present and non-empty. It silently
  converts a would-be invariant violation into `location: "http:///path"`.
- **N2-13** `crates/envoy-http1/src/uring.rs:287` — the third `build_response` call site has no access
  logging at all (0 `access_log` hits in the file), so the `&mut Request` widening — whose entire
  purpose is making the `prefix_rewrite` `:path` rewrite observable in the log — is **inert** there.
  Harmless today (feature-gated, Router-only); it would silently under-report if logging is ever
  added to the uring path.
- **N2-14** `docs/envoy-rust/phases/76.2-redirect-runtime-fixture/PROGRESS.md:2535` — states the
  fixture *"deliberately carries five distinct status codes (**14×301, 2×302**, 1×303, 1×307, 1×308 =
  19)"*. Shipped is **15×301, 1×302** (`grep -o 'expected_status: [0-9]*' | sort | uniq -c`). The
  14/2 split is the **mutated** tree's census, captured during the `q02` negative control where
  `q02`'s `expected_status` was flipped 301→302. Notable because the paragraph's own lesson is
  *"re-derive it from the artifact before treating a mismatch as a signal."* **`PROGRESS.md` is a
  landed artifact and is NOT editable** (D-3.5) — recorded for accuracy, not repair.
- **N2-15** `docs/envoy-rust/BEHAVIOR_CONTRACT.md:3059` and `:3084` — two un-propagated additions:
  §B's flat *"A redirect carries NO `content-type`. A `direct_response` DOES"* does not cross-reference
  `:1131`, which already banks *"upstream does NOT emit `content-type` on an **empty-body** local
  reply"* (ADR-0059), so the two read as in tension without the body qualifier; and the file's
  canonical `%RESPONSE_CODE_DETAILS%` mapping row at `:1455` still enumerates only the
  `direct_response` **route** set-site and was not extended with the redirect one that §D correctly
  banks.

---

## 7. Round-1 findings — status, NOT re-graded

Round 1 (`REVIEW.md`) graded **0 Critical, 1 Issue, 9 Minor, 9 Nit**. Recording their status here is
necessary because three of this round's dimensions independently re-derived several of them — two at
ISSUE severity. **A fresh reviewer with no memory of round 1 will rediscover round 1's findings; the
guard is reading the prior review, not trusting the dimension.**

**FIXED at the §5.2 re-entry (verified on disk by this review):**

- **I-1** — classifier extended, forcing function kept, three `reload()` tests added. §3.1.
- **M-1** — `plan_redirect_pins_the_two_invented_cells_contract_f_items_7_and_8` exists
  (`hcm.rs:10866`) and genuinely pins **both** cells; §F's note now **names** it. I read the body:
  item 7 is asserted at `:10877-10881` (the only `matched_prefix: None` call in the tree), item 8 at
  `:10888-10892`. The fix correctly declined the review's literal suggestion (a 23rd table row) and
  recorded the deviation.
- **M-2** — probe `q02` added on its own `/q2-hostport` route; it is the unique differential
  discriminator for the port-DROP (re-derived: 10 routes set `host_redirect`, 3 probes send a ported
  `Host:`, `q02` is the only intersection).
- **M-3** — `assert_eq!(resp.reason, None)` present at `hcm.rs:10842`.
- **M-4** — the `RouteAction::Redirect` doc no longer describes a `synth_501` placeholder.

**STILL OPEN, banked by design (§6.3) — do NOT re-raise as new:** **M-5** (the two ADR-0028
misattributions in `rds.rs`; needs **ADR-0171** if taken), **M-6** (`strip_port` applied to the
configured `host_redirect`), **M-7** (the H2 seam test does not drive an H2 stream), **M-8** (one
oneof on the `envoy-config` RDS path — **substantively closed** by the re-entry, since
`rds_watcher.rs:596` now drives the scheme half through `reload()`; the residual is that the two
halves are pinned in different crates), **M-9** (the `connection` header value has no witness in
either direction — re-derived this round and confirmed: the unit test uses `close=true` only, the
driver appends `Connection: close` to every probe, and the sole `close=false` redirect call
(`envoy-http2/src/hcm.rs:6929`) never inspects the header), and **N-1** through **N-9**.

**Two round-1 nits I can now sharpen with a measurement**, offered as refinements rather than new
findings: **N-5** (§F item 8's stated reason names the wrong mechanism) is still present verbatim at
`:3129` — and §D at `:3093` states the correct reason 36 lines earlier, so the file contains its own
correction. **M-7**'s "a real end-to-end harness exists in the same file" is measured: `spawn_h2_hcm`
appears 41 times in `crates/envoy-http2/src/hcm.rs` and takes exactly the `Http1HCMConfig` the new
helper already produces.

---

## 8. STRENGTHS — measured properties a close-out must not disturb

- **The I-1 fix is better than the fix that was requested.** It kept the forcing function instead of
  deleting it, corrected the arm's comment from "four" to "six" (**verified correct**, §3.1), and
  added the cross-crate lesson to the code. Its three `reload()` tests assert the exact error
  variant via `matches!`, `Arc::ptr_eq` on the last-good table, **and all five counters — including
  the ones that must NOT move**. A fix that added only one of the two variants would be caught,
  because both halves are separately exercised (`rds_watcher.rs:547`, `:596`).
- **The RED for I-1 was a real panic at the exact `unreachable!()` line**, not a proxy for it
  (`PROGRESS.md` §R3.1). That is the strongest form TDD evidence takes.
- **The accept-direction control is genuine** (`rds_watcher.rs:641`): it pre-asserts the seeded table
  is empty, then asserts the swap happened and the mirror-image counter set. The two reject tests
  cannot pass by rejecting everything.
- **"Make the claim true" beat "delete the claim" on both doc findings.** M-1 and M-2 each offered a
  soften-the-doc option; the re-entry closed the gap instead, for ~4 lines and one probe, converting
  two false documents into two working safety nets.
- **The §F closing note is now the best paragraph in the bank** (`:3131-3149`). It names the test,
  states that it is the *only* thing standing behind either cell, and then explains **how the note
  got it wrong the first time** — including the generalisable lesson (*"an `Option` field on a
  test-table struct proves nothing if the constructor can only produce `Some`: check the constructor,
  not the type"*). A contract that documents its own past error is rarer and more useful than one
  that is merely correct.
- **The M-1 fix correctly refused the review's literal shape.** Folding an invented cell into the
  22-row measured table would have forced the `cells.len() == 22` guard to 23 and mixed an invented
  cell into a table whose entire meaning is "MEASURED against v1.33.0". Pinned separately, guard
  intact, deviation recorded (`PROGRESS.md` §R3.7). **A review recommends a fix's intent; the
  implementer owns its shape.**
- **`plan_redirect` is genuinely pure and total.** No `unwrap`/`expect`/`panic!`/indexing/arithmetic
  in `:2272-2346`. Every byte-index traced: `split_once('?')` is boundary-safe; `path.get(..)` returns
  `None` rather than panicking; `strip_port`'s one raw slice takes its index from `rfind(':')`, always
  a char boundary. The totality test at `:10776` genuinely reaches the `str::get → None` path.
- **The fixture is mechanically clean at 19**, re-derived by this review rather than taken on trust:
  19 probes / 19 routes / 19 distinct paths / 19 distinct prefixes / **0 shadowing pairs** over all
  342 ordered pairs / perfect probe↔route bijection / no unprobed route / no unmatched probe /
  assertion density 19/19/19 / **0** `path:` matchers (keeping it clear of open CF-76-1) / **0**
  `{{ADMIN_PORT}}`.
- **The paired configs differ by exactly the three intended logical edits with the route table
  byte-identical** — `routes:`→`http_filters:` span, `envoy.yaml:18-63` and `envoy-rust.yaml:21-66`,
  both md5 `fbd8bebe2a34a7685b86d51f5fedce17`; cross-checked by **no route-table line appearing in
  `diff -u` at all**. (Span stated deliberately: a span run to EOF swallows the `admin:` block the two
  files are *supposed* to differ on and fakes a drift alarm.)
- **`location` is still not allow-listed** and `diff_headers` still compares it value-exact. That
  comparison **is** the fixture's entire witness, and the prohibition is stated independently in the
  contract, the fixture README **and** the entrypoint, so it survives losing any one document.
- **The census reconciles exactly** (§3.2) — the strongest available evidence that the re-entry lost
  no test.
- **Both directions of the `:path` asymmetry remain pinned at two levels**, and the non-mutation test
  asserts the location *did* change first, so it cannot pass vacuously.
- **The state-4 re-verification's honesty is itself a strength.** It quoted the local h2spec
  self-skip rather than reading a bare `ok` as conformance; it recorded a doc defect it declined to
  fix (M2-6) rather than quietly repairing it; and it recorded three method findings — including that
  its own `touch` probe created 22 files and manufactured four phantom clippy errors. A verifier that
  publishes its own near-misses is doing the job.

---

## 9. What this review did NOT do

- **Did not re-run the §7.5 gate.** Gates (a)-(e) were adjudicated at the state-4 re-verification and
  their real output is in `PROGRESS.md` §V4.1-§V4.6. **This session ran no `cargo` command.** I did
  re-derive the test census from the diff and re-confirm CI on the full 40-char SHA — both are
  reading, not running.
- **Did not fix anything it graded** (ADR-0127; ADR-0165). Every Minor and Nit above is left open.
  M2-6 in particular was handed to me by the verifier specifically to grade, not to repair.
- **Did not flip any ROADMAP status cell.** Re-measured at this commit: **107 rows / 105 `done` / 1
  `in-progress` (`76`) / 1 `planned` (`76.2`)**, status read as field 4 splitting on `' | '`.
  Unchanged. The close-out owns the flip.
- **Did not edit** `76.2/SPEC.md`, `76.2/PLAN.md`, `76.2/PROGRESS.md`, the round-1 `76.2/REVIEW.md`,
  `76/SPEC.md`, or any `76.1` artifact (D-3.5). Several findings name defects *in* those files
  (M2-4's root cause, N2-14) — recorded, not repaired.
- **Did not add an ADR.** Head **ADR-0170**, next free **ADR-0171**, re-derived on disk
  (`grep -o '^## ADR-[0-9]\{4\}' | sort -t- -k2 -n | tail -1`). Nothing here settles a new decision.
  Round 1's M-5 still needs ADR-0171 if a later session takes it.
- **Did not re-raise ADR-0163** (the h2spec CI gate is not vacuous — settled), **CF-76-1**,
  **CF-75-2..6**, or any other banked carry-forward.
- **Did not add `location` to `HEADER_ALLOW_LIST`, trim `known-failures.txt`, weaken a fixture, or
  create a `stop` file.** The stop condition remains FALSE — rows `76` and `76.2` are non-`done`.

---

## 10. Disposition

**Gate (f) is CLOSED. All six §7.5 gates are now GREEN. Sub-phase `76.2` is approved to land.**

**Next state: §5 state 6 — the close-out**, a separate session (§5.1; ADR-0127). It flips ROADMAP row
`76.2` `planned` → `done` **and** parent row `76` `in-progress` → `done` (its only sub-phases `76.1`
and `76.2` are then both `done`) — **the status cells and nothing else**.

**Carried forward, not scheduled** (§6.3 — do not fix opportunistically):

- **This round:** M2-1 through M2-7, N2-1 through N2-15, and the five unmeasured edges of §5.
- **Round 1, still open:** M-5..M-9 and N-1..N-9. M-5 needs **ADR-0171** if taken.
- **Natural homes.** M2-2 and M2-3 belong with any future phase that touches fixture `0086` or the
  redirect surface. M2-1 belongs with the phase that lands `regex_rewrite` inside `redirect` — it
  should extend `validate_redirect_oneofs`'s doc **before** adding the third check. M2-4, M2-5, M2-6
  and most Nits are doc-only and can ride along with any phase editing those files. M-6, N-8, N-9 and
  §5's edges are best served by **one measured probe session** against `envoyproxy/envoy:v1.33.0`
  rather than by guessing — the same discipline that produced the 22 cells in the first place.

**A close-out must not:** weaken any of the 22 table cells, collapse the per-row `label` or the
`assert_eq!(cells.len(), 22)` guard, delete probe `q02` or its `/q2-hostport` route, delete
`plan_redirect_pins_the_two_invented_cells_contract_f_items_7_and_8`, remove either new variant from
the `update_rejected` arm, remove the `other => unreachable!(…)` arm, reduce `synth_redirect`'s exact
`Vec` header assertion to a `contains`, delete `synth_501`, re-introduce an `if let` over
`RouteAction` in `rds.rs`, add `location` to `HEADER_ALLOW_LIST`, trim `known-failures.txt`, edit any
landed artifact, or create a `stop` file.
