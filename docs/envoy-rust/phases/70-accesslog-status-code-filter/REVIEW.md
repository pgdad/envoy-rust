# Phase 70 — `status_code_filter` — §5 state-5 CODE REVIEW

> ## ⚠️ CURRENT VERDICT LIVES IN §8 (the state-5 RE-REVIEW). READ §8 FIRST.
>
> **This file now carries TWO reviews.** The header block and §1–§7 below are the
> **FIRST** state-5 review (of range `b362bae..899ca5c`); its verdict is **SUPERSEDED** by
> **§8**, appended by the §5 state-5 **RE-REVIEW** session after the §5.2 re-entry fixed its
> blocking finding. Per D-3.5 nothing below is rewritten — the first review's reasoning is
> preserved verbatim because most of it still stands, and §8 states precisely which parts
> are discharged, which are superseded, and which are corrected.
>
> **Current verdict (§8): NOT approved — 0 Critical / 1 Important (I-2, NEW) / 7 Minor.**
> I-1 below is **DISCHARGED** (measured). Per §5.2 the phase re-enters at **state 3**.

---

## THE FIRST REVIEW (superseded verdict — reasoning preserved per D-3.5)

> **Written by the §5 state-5 code-review session** (`superpowers:requesting-code-review`).
> This is the INDEPENDENT review: per **ADR-0127** the context that wrote an artifact must
> not grade it, so neither the state-3 session's own final whole-branch review
> (`PROGRESS.md` §5, "READY TO MERGE — 0C/0I/3 Minor") nor the state-4 gate evidence
> (`PROGRESS.md` §V1–§V8) carries any authority here. Written for a stranger with zero
> prior context (D-3.4).
>
> **Reviewed range:** `b362bae..899ca5c` — 18 commits (14 TDD task commits + 2 review-fix
> commits + the state-3 docs commit + the state-4 docs commit); +2113 / −103 across 19 files.
>
> ## VERDICT (SUPERSEDED BY §8): **NOT ready to merge — 1 Important.**
>
> **0 Critical / 1 Important / 5 Minor.** Per `BOOTSTRAP_PROMPT.md` §5.2 an Important
> finding sends the phase back to **§5 state-3 (implementation), NOT state-4** — that is a
> SEPARATE session. The Important is a **test-coverage gap, not a behavioral bug**: the code
> as landed is correct, but two of its three shipped operators are pinned by nothing, and a
> silent inversion of them would leave the entire test suite green. The fix is small and
> localized (one test, using a helper that already takes the operator as a parameter).
>
> **§7.5 gate (f) is now MET** in the sense that `REVIEW.md` exists and is reasoned; it is
> **not APPROVED**, so the phase does not advance to state-6.
>
> **[§8 UPDATE — I-1 is now DISCHARGED.](#8-the-re-review-5-state-5-re-review--the-re-issued-verdict)**
> The §5.2 re-entry's fix was measured to bite. But the re-review found the **same defect
> class one layer up** (I-2, §8.3) and the verdict remains NOT approved.

---

## §1. What was reviewed (one paragraph for a stranger)

Before phase 70, an envoy-rust access-log sink logged **every** request record. Upstream
Envoy lets an `AccessLog` entry carry a **filter** — a predicate deciding whether a record
is emitted at all. Phase 70 opens that subsystem with its single canonical variant,
`status_code_filter`: a comparison of the **final response code** against a threshold using
`op ∈ {EQ, GE, LE}` (so `GE 500` keeps a 503 and drops a 200). A new config field
(`AccessLog.filter`) is compiled at HCM config-load time into a runtime predicate
(`envoy-accesslog::LogFilter`) carried on each sink, then consulted in both the HTTP/1.1 and
HTTP/2 emit loops. A sink with no filter behaves exactly as before — load-bearing, because
29 pre-existing access-log fixtures depend on it. The cross-proxy witness is the new
differential fixture `0076`.

---

## §2. Strengths (specific, verified — not courtesy)

These were checked against the code, not taken from the commit messages.

1. **The `BEHAVIOR_CONTRACT.md` §A–§G section is accurate claim-for-claim.** Every claim was
   re-verified against the landed implementation, including the two most likely to drift:
   the ordering-sensitive `access_logs_total` semantics (`.inc()` fires after the gate and
   **before** the `emit` await, so it counts intent-to-emit and a failed write does not
   deflate it) and the comparison direction (`status <op> default_value`, config value on
   the RIGHT, inclusive on GE and LE). This is unusual — contract prose commonly drifts from
   code, and here it does not.
2. **The differential fixture `0076` is a genuinely strong witness.** Its oracle is **pure
   cross-proxy equality**, not a hardcoded expected line: the documented
   `STATUS=503 PATH=/log FLAGS=-` appears only in comments, never in an assertion. Both
   halves of the filter decision are enforced — the KEPT 503 by the byte compare, the
   DROPPED 200 by the line-count assert (a stray `/nolog` line on either side makes the
   count 2 ≠ 1 and bails before the byte compare).
3. **The `envoy.yaml`/`envoy-rust.yaml` divergence is conventional, not invented.** The
   4-hunk delta (admin block, bind address, `generate_request_id`, per-proxy log path) is
   hunk-for-hunk identical to the established `0040` precedent, and the `filter` block
   itself is byte-identical across both files. No ADR is owed for it.
4. **The "29 pre-existing access-log fixtures" figure is correct.** Independently recounted
   by driver `kind`: 22 `http1_access_log_byte_exact` + 7 `http2_access_log_byte_exact` +
   1 `http1_with_access_log` = 30 including `0076` → **29** pre-phase-70, of which **28** are
   byte-exact. This corrects an earlier "27" and the correction itself checks out.
5. **The two Important findings the state-3 session claimed to fix are real fixes.** Both
   were re-verified by mutation with a confirmed recompile: reverting the H1 counter to the
   pre-loop bulk `add(len)` goes RED, and forcing the `expect_logged` serde default to
   `false` goes RED. Neither is a paper fix.
6. **The `ComparisonOp` → `FilterOp` translation carries no `_` wildcard**, so a future oneof
   arm is a compile error rather than a silent fallthrough. (Its *test* coverage is the
   Important below — the construct itself is right.)
7. **The fuzz corpus seed is genuinely tracked and genuinely valid** — `git ls-files` prints
   it and `git check-ignore` exits 1 (the corpus dir is `*`-ignored, so this is the only
   proof that counts), and the seed parses as a real bootstrap that reaches the new
   validator rather than dying at the YAML layer.

---

## §3. Findings

### §3.1 Critical — **none.**

The one hypothesis with Critical potential was that the H2 gate might be **dead code** —
that `envoy-http2` builds its own sinks without compiling the filter, so an H2 filter would
silently never apply. It is **disproven twice over**: statically, `envoy_http2::HCMConfig` is
a pure wrapper (`pub inner: Arc<Http1HCMConfig>`) over the H1-built config
(`crates/envoy-bin/src/main.rs:685` wraps the product of
`envoy_http1::HCMConfig::from_config` at `main.rs:613`), so H2 owns no sink construction at
all; and empirically, by the live H2 probe in §4.1 below.

### §3.2 Important (must fix — triggers the §5.2 state-3 re-entry)

**I-1 — the config→runtime `ComparisonOp`→`FilterOp` mapping is unpinned for `Eq` and `Le`.**

- **File:** `crates/envoy-http1/src/hcm.rs:1746-1750` (inside `compile_access_log_filter`).
- **What is wrong.** Only the `Ge` arm is ever exercised. `ComparisonOp::Eq` and
  `ComparisonOp::Le` appear **nowhere in the tree except the two match arms that define
  them** — no test, no fixture, and no fuzz-corpus entry uses `op: EQ` or `op: LE`
  (`grep -rn "op: EQ\|op: LE" crates/ tests/` → zero hits).
- **MEASURED, not hypothesized.** The two arms were swapped
  (`ComparisonOp::Eq => FilterOp::Le`, `ComparisonOp::Le => FilterOp::Eq`; `Ge` untouched)
  in an **isolated git worktree** at `899ca5c`, and the affected suites re-run:

  ```
  Compiling envoy-http1              (forced rebuild confirmed — 1 hit)
  mutation present at run time       (confirmed — 1 hit)
  test result: ok. 102 passed; 0 failed   (envoy-accesslog)
  test result: ok. 611 passed; 0 failed   (envoy-config)
  test result: ok. 173 passed; 0 failed   (envoy-http1)
  → 886 passed / 0 failed — GREEN UNDER THE SWAP
  ```

  The worktree was used deliberately: an in-place mutation was clobbered mid-run by a
  concurrent reviewer's `git checkout --`, which would have produced a false conclusion.
  The result was independently reproduced a second time by a separate reviewer.
- **Why it matters.** This is precisely the recurring defect class of this phase — the
  state-3 review already found and fixed **two** "a test that could not fail" defects, and
  this is a third of the same family. The `filter.rs` boundary tests prove `FilterOp::{Eq,Le}`
  **evaluate** correctly, and the bootstrap test proves `op: EQ` **parses** correctly, but
  **nothing connects the two**. A user config saying `op: EQ` could compile to an `Le`
  predicate — silently logging every record at or below 404 instead of exactly 404 — with
  every test in the repository still green. Two of the three shipped operators have a wholly
  untested config→runtime translation. The mapping is correct **as written**; the defect is
  that nothing holds it correct.
- **How to fix (cheap — the seam already exists).** The existing test helper
  `hcm_config_with_filtered_access_log` (`crates/envoy-http1/src/hcm.rs:4487`) **already takes
  the operator as a parameter**: `filter: Option<(envoy_config::ComparisonOp, u32)>`. Extend
  `from_config_compiles_status_code_filter_into_sink` (`hcm.rs:4562`) to table-drive all three
  ops through it — e.g. `(Eq, 404)` → `!should_log(403) && should_log(404) && !should_log(405)`
  and `(Le, 200)` → `should_log(200) && !should_log(201)`. That kills the mutation above. Per
  D-3.1 the re-entry does this under TDD: prove the new assertions RED against the swapped
  mapping first, then restore and confirm GREEN.

### §3.3 Minor (accept as carry-forwards, or fold into the state-3 re-entry if cheap)

**M70-R1 — the oneof arm-counting construct is not future-proof, and its doc comment
overclaims.** `crates/envoy-config/src/bootstrap.rs:5117-5130`; doc at `5097-5098`.
`let set_arms = [filter.status_code_filter.is_some()].iter().filter(|set| **set).count();`
is a hand-maintained one-element array. A future phase adding a second arm to
`AccessLogFilter` (`bootstrap.rs:721`) compiles cleanly **without touching this array** — and
a config setting only the new arm then counts `set_arms == 0` and is rejected with the
actively-wrong message "no filter variant is set". The doc comment claims it is "written to
stay correct as future phases add arms"; it is not — it silently does the wrong thing. The
`SubstitutionFormatString`/`AmbiguousLogFormat` precedent it cites is strictly better here:
it matches on a tuple of `Option`s, so a third arm is a non-exhaustive-match **compile
error** that forces the update. Failure is loud (a boot reject), not silent-wrong at runtime,
hence Minor. **Fix:** destructure so the compiler forces the update
(`let AccessLogFilter { status_code_filter } = filter;` then count from named bindings), or at
minimum correct the doc comment to say the array MUST be extended by hand. **This is the same
surface as CF-70-1 and should be discharged by the same future phase — the one that lands
arm #2.**

**M70-R2 — `expected_logged_count` is pinned in isolation, but its *wiring into the two
byte-exact arms* is not.** `tests/differential/src/lib.rs:6255` and `:6398`. Reverting both
arms to `expected_lines = probes.len()` leaves all 159 `differential` lib tests green
(mutation confirmed with a recompile). The behavior **is** covered — Docker fixture `0076`
would fail — but only after burning the full 15s `ACCESS_LOG_FLUSH_WAIT`, giving a slow
failure signal far from the cause. Acceptable for harness code; a unit test over the arms'
count computation would localize it.

**M70-R3 — `rejects_status_code_filter_unknown_op` asserts only `ConfigError::Yaml(_)`.**
`crates/envoy-config/src/bootstrap.rs:12958`. Any YAML-level error satisfies it, so an
unrelated typo in the fixture YAML would keep it green for the wrong reason. It **does** bite
for the behavior it targets (adding an `Ne` variant makes the parse succeed → RED), so this
is a robustness nit, not a coverage gap. **Fix:** also assert the message names the offending
token (`err.to_string().contains("NE")`).

**M70-R4 — `AccessLog.filter` serializes as `"filter": null` when unset.**
`crates/envoy-config/src/bootstrap.rs:706-711` — `#[serde(default)]` without
`skip_serializing_if = "Option::is_none"`. `Listener` is serialized into `/config_dump`, where
upstream Envoy omits unset message fields rather than emitting `null`. Genuinely low severity:
the same struct family already does this (`FileAccessLog.log_format`, `bootstrap.rs:796-797`,
shipped since phase 38), so phase 70 **extends an existing pattern rather than introducing
one** — but the crate does use the skip idiom elsewhere, so it is a choice. **Fix:** add
`skip_serializing_if = "Option::is_none"` (ideally to `log_format` too, as a separate cleanup).

**M70-R5 — CF-70-2 rests on a FALSE premise and should be corrected, not carried.** See §4.2:
the failure mode it describes cannot occur. It should be **closed** at the state-3 re-entry
rather than propagated to the next filter phase as a live hazard.

---

## §4. Measured probes (this session's own measurements)

Doctrine and hard experience say a green §7.5 gate proves the code does what its tests ask,
**not that the tests ask the right question** — and that reviewer parity-hypotheses must be
measured, because one has previously been false and hid a Critical. Two were measured here.

### §4.1 The untested composition: the H2 filtered path — **MEASURED PARITY, hypothesis TRUE**

Phase 70 gates **two** consumers (H1 + H2), but the differential covers **H1 only**; the H2
filtered path is covered in-process only, and **no fixture anywhere sets a filter on H2**.
The H2 gate is a hand-mirrored copy of the H1 gate — exactly the shape that hides a defect.
So it was probed live against **both** proxies rather than trusted.

**Method.** Fixture `0076`'s exact config — same `GE 500` filter block, same
`text_format_source`, same two `direct_response` routes, no backend — but with
`codec_type: HTTP2` + `http2_protocol_options: {}`. Real `envoyproxy/envoy:v1.33.0` in Docker
with `-p` port-mapping (host-net namespace is not shared on this host); envoy-rust from
`target/debug/envoy-bin`. Both probes driven over real HTTP/2 (`curl --http2-prior-knowledge`,
`proto=2` confirmed on every response).

| Proxy | `GET /log` | `GET /nolog` | access-log file |
|---|---|---|---|
| upstream Envoy v1.33.0 | 503 (h2) | 200 (h2) | exactly 1 line |
| envoy-rust | 503 (h2) | 200 (h2) | exactly 1 line |

```
$ md5sum envoy-mount/access.log rust-mount/access.log
195e6cc6ef72be59eac162d1d89471a1  envoy-mount/access.log
195e6cc6ef72be59eac162d1d89471a1  rust-mount/access.log
$ diff envoy-mount/access.log rust-mount/access.log      # empty
00000000: 5354 4154 5553 3d35 3033 2050 4154 483d  STATUS=503 PATH=
00000010: 2f6c 6f67 2046 4c41 4753 3d2d 0a         /log FLAGS=-.
```

**Result: byte-identical on the H2 filtered path** — md5-equal and hexdump-equal, including
trailing bytes. The 200 is dropped on both sides; the 503 is kept on both sides.

**Two conclusions.** (1) The "it's probably parity" hypothesis is **measured TRUE** for the H2
filtered composition — this is evidence, not assumption. (2) It independently proves the H2
gate is **live code, not a dead mirror**: had it been unwired, envoy-rust would have logged
the suppressed 200 and the files would differ. This empirically corroborates the static
call-chain finding in §3.1.

**Disposition — no new fixture demanded by this review.** The composition is now measured
equivalent, and `0076`'s H1 witness plus the in-process H2 gate test (which pins **both** the
line count and the counter) cover the code. An H2 filtered *differential fixture* remains a
reasonable future addition (it is already an ADR-0140 §2.2 deferral), but this review does not
make it a merge blocker: the gap it would close has now been directly measured shut.

### §4.2 CF-70-2's premise is FALSE — both proxies create the log file eagerly

CF-70-2 (`PROGRESS.md` §6) warns that if a future fixture suppressed **every** probe,
`wait_file_lines(path, 0)` returns instantly and `read_to_string` "would error on a
never-created file, yielding a misleading I/O failure rather than a clean pass."

**Measured: the file is never "never-created".** Both proxies open the sink file eagerly at
config-load, before any request:

- **envoy-rust:** booted with the filtered config and **no** request driven →
  `rust-mount3/access.log` exists, size **0**.
- **upstream Envoy v1.33.0:** container booted, file exists before any request; then a single
  suppressed 200 (`/nolog`) driven → file still exists, size **0**.

So an all-suppressed fixture reads `""` → 0 lines → compares 0 against 0 → **a correct clean
pass**, which is the desired behavior, not a misleading I/O error. **CF-70-2 as written
describes a failure mode that cannot occur**, and a future filter phase acting on it would
chase a phantom. Recorded as **M70-R5**: correct or close it rather than carry it.

### §4.3 The other two carry-forwards — confirmed as characterized

- **CF-70-1** (`compile_access_log_filter`'s `expect()` on a zero-arm filter) — **genuinely
  unreachable via config today**, confirmed by two independent traces of the full guard chain
  through **both** entry points: static (`parse_bootstrap` → `bootstrap::validate` →
  `validate_hcm:3795` → `validate_access_logs:5118`) and dynamic/LDS (`load_dynamic_resources`
  → the post-merge re-validation gate at `lib.rs:1228-1234` fires because `dynamic_listeners`
  is `Some` → the same `validate()` whose listener loop explicitly chains
  `static_listeners.iter_mut().chain(dynamic_listeners.iter_mut().flatten())`). It is a live
  footgun the moment arm #2 lands, and that phase must convert it to a full match. **Accepted
  as a carry-forward.** (One reviewer noted the `expect()` is reachable via the *public* API —
  `HCMConfig::from_config` is `pub` and an in-workspace caller could hand it an unvalidated
  struct literal. That is a boot-time panic on a caller error, consistent with the tree's
  validator-enforced-invariant convention; it does not change the disposition.)
- **CF-70-3** (`wait_file_lines(have >= want)` false-GREEN window on the DROPPED half) —
  characterization confirmed: false-pass-only, never a false fail. **Accepted as a
  carry-forward**, owned by the next access-log-filter phase.

### §4.4 The T7 / phase-06.1 R-8 reversal — confirmed correct

T7 reverses the phase-06.1 REVIEW §7 R-8 directive (bulk `add(N)` chosen over N×`inc()`).
The reversal is **ADR-0141-mandated** and R-8's rationale is now dead: with a per-sink gate,
a bulk `add(len)` would count records for sinks that suppressed them. Both are
`fetch_add(_, Relaxed)` on the same atomic, so for unfiltered sinks `N×inc()` reaches an
identical value to `add(N)` — the change is a no-op for every pre-existing fixture and a
correctness fix for filtered ones. **Confirmed; no action.**

---

## §5. The judgment call — **ADR-0142 is FIRED** (the §E.1 acceptance-class boundary)

The state-3 session recorded a MEASURED stricter-than-upstream acceptance boundary in
`BEHAVIOR_CONTRACT.md` §E.1 rather than firing an ADR: upstream **ACCEPTS** `op` omitted
(proto3 implicit default → `EQ`), `default_value` omitted (→ `0`), and numeric enum tokens
(`op: 1` → `GE`); **envoy-rust REJECTS all three**. The question left to this review was
whether that narrows ADR-0049's "the same class of configs is rejected/accepted" claim enough
to deserve its own ADR.

**Decision: fire ADR-0142** (see `DECISIONS.md`), accepting the strictness as a deliberate,
recorded, tree-wide posture. **Reasoning:**

1. **ADR-0049 does not settle this direction.** ADR-0049 governs the **reject** direction —
   when envoy-rust rejects, it does so fatally with a native message — and its findings are
   scoped to the phase-18 CDS surface. §E.1 is the **converse**: a config upstream **accepts**
   that envoy-rust **refuses to boot**. That is genuinely outside ADR-0049's decided ground,
   so leaning on it would be reading it for more than it says.
2. **It is cross-cutting, not phase-70 trivia.** The same strictness already exists at
   `FractionalPercent.numerator` and `TokenBucket.max_tokens`, and every future phase modeling
   a proto3 scalar will meet it again. As an *emergent convention* it gets rediscovered and
   re-litigated per phase; as a written decision it gets cited.
3. **Doctrine points this way.** §4.1 invariant 5 says a divergence from the contract is
   resolved by updating the contract **via ADR** or fixing the implementation — never both
   silently. The contract *was* updated (§E.1); the ADR is the half that was missing. D-3.5
   requires the decision to be written rather than remembered, and §E.1 records the measured
   **fact** without recording the **decision** (options considered, rationale).

This is a documentation-completeness action, **not** a finding against the implementation: the
divergence is fail-loud (envoy-rust refuses to boot; runtime behavior never silently differs),
was measured rather than assumed, and the alternative — sprinkling `#[serde(default)]` across
the config surface to mirror proto3 implicit defaults — would silently accept configs whose
semantics differ from the operator's intent and would contradict the tree-wide fail-loud
posture. **No code change is owed by this decision.**

---

## §6. Doctrine compliance

| Rule | Status |
|---|---|
| D-3.1 TDD (RED→GREEN→commit per task) | ✅ every task records a proven RED |
| D-3.2 dependency stance | ✅ no new crate, no new external dependency |
| D-3.3 differential-over-fidelity | ✅ `0076` is cross-proxy equality; contract-driven |
| D-3.4 stranger-readability | ✅ SPEC/PLAN/PROGRESS/contract all readable cold |
| D-3.5 decisions written | ✅ — **completed by this session** via ADR-0142 (§5) |
| D-3.6 green build | ✅ state-4 gate (a)–(e) PASS; CI GREEN on `899ca5c` (run `29497067444`) |
| D-3.7 version pin | ✅ `envoyproxy/envoy:v1.33.0` untouched |
| D-3.8 `#![forbid(unsafe_code)]` | ✅ holds at every crate root; no `unsafe` added |
| §6.1 split gate | ✅ does not fire (14 tasks; the +LoC overage is test/doc mass) |
| §7.4 fuzz | ✅ corpus seed on the existing target; no new target → no `ci.yml` step owed |
| Scope discipline (ADR-0140) | ✅ only `status_code_filter` shipped; 11 variants deferred |
| `known-failures.txt` | ✅ untouched, not trimmed |

---

## §7. Next session — the §5.2 state-3 re-entry (NOT state-4, NOT state-6)

Per `BOOTSTRAP_PROMPT.md` §5.2, an Important finding re-enters at **state 3**: you are
resuming implementation **under TDD**, not merely re-verifying.

**The single required fix — I-1** (`crates/envoy-http1/src/hcm.rs:1746-1750`): pin the `Eq`
and `Le` config→runtime mappings. Extend
`from_config_compiles_status_code_filter_into_sink` (`hcm.rs:4562`) to table-drive all three
operators through `hcm_config_with_filtered_access_log` (`hcm.rs:4487`), which **already takes
`(ComparisonOp, u32)` as a parameter**. **Prove the RED first** by swapping the `Eq`/`Le` arms
(the mutation in §3.2), confirm the new assertions fail, then restore the mapping and confirm
GREEN. Grep each run for `Compiling envoy-http1` — a stale test binary yields a FALSE PASS and
would make the new test look vacuous. **Do the mutation in a scratch git worktree**, not
in-place: a concurrent session's `git checkout --` clobbered an in-place mutation during this
review and nearly produced a false conclusion.

**Optional, cheap, same-file folds** (safe to take in the same re-entry, or defer): **M70-R3**
(tighten the unknown-`op` assertion), **M70-R5** (correct or close CF-70-2 — its premise is
measured false). **M70-R1** is best discharged by the phase that lands oneof arm #2, alongside
CF-70-1. **M70-R2** and **M70-R4** are reasonable carry-forwards.

After the fix lands: state-4 re-verification (a fresh session re-runs the full §7.5 gate),
then a state-5 re-review, then the state-6 close-out. **This session did NOT chain into any of
them** (§5.1 — one state per session).

**Carry-forwards after this review.** Opened/updated: **CF-70-1** (unchanged — the zero-arm
`expect()`; arm #2's phase must convert it to a full match), **CF-70-2** (**premise measured
FALSE — correct or close it**, M70-R5), **CF-70-3** (unchanged — false-pass-only), plus the
new **M70-R1..R5**. Not consumed by this phase and still live: **M69-A..I**, **CF-69-1/2/3/5**,
**M68-1**, **M-1**, **CF-67-3/5/6/7**, the older Minors, and the HTTP-filters-family (1)–(4).

---
---

# §8. THE RE-REVIEW (§5 state-5 RE-REVIEW) — the re-issued verdict

> **Written by the §5 state-5 RE-REVIEW session** (`superpowers:requesting-code-review`),
> **appended to — never rewriting — the first review above (§1–§7)**, per D-3.5. Written for
> a stranger with zero prior context (D-3.4).
>
> **Why a RE-review exists.** The first review (above) returned **NOT approved** on one
> blocking Important (§3.2 **I-1**). Per `BOOTSTRAP_PROMPT.md` §5.2 the phase re-entered at
> **state 3**, where a re-entry session landed the I-1 fix and folded two Minors; a separate
> state-4 session then re-ran the full §7.5 gate over the re-entry head and measured
> **(a)–(e) PASS** (`PROGRESS.md` §V(2)). Gate **(f)** — an APPROVED `REVIEW.md` — is the
> only sub-gate outstanding, and it is this session's sole deliverable. Per ADR-0127 the
> re-entry's own scoped `886/0` run carries **zero authority** here; every claim below was
> re-measured by this session.
>
> **Reviewed range:** `b860e4e..80978e8` (the §5.2 re-entry `2763c73` + the state-4
> re-verification `80978e8`). The re-entry's code delta is **two test bodies**
> (`crates/envoy-config/src/bootstrap.rs`, `crates/envoy-http1/src/hcm.rs`) + docs;
> **no production code changed** (independently re-verified — §8.2).
>
> ## VERDICT: **NOT approved — 1 Important (I-2, NEW).**
>
> **0 Critical / 1 Important / 7 Minor.**
>
> - **I-1 (the first review's blocker) — DISCHARGED.** Measured, not read: the fix bites on
>   all three arms independently (§8.1).
> - **M70-R3, M70-R5 — CONSUMED.** Both measured genuinely discharged (§8.4).
> - **I-2 — NEW Important.** The re-entry closed the *lower* half of the config→runtime
>   seam (`ComparisonOp` → `FilterOp`) and its own doc comment asserts the *upper* half
>   (YAML token → `ComparisonOp`) is already covered. **It is not.** That claim is false, and
>   the gap it conceals is the **same defect class as I-1, displaced one layer up** — the
>   fourth instance in this phase (§8.3).
>
> Per §5.2 an Important sends the phase back to **§5 state-3 (implementation), NOT state-4**
> — a SEPARATE session. **§7.5 gate (f) remains UNMET**; the phase does **not** advance to
> state-6.
>
> **This is not a reversal of the re-entry's work.** The I-1 fix is correct, well-shaped, and
> proven. I-2 is an adjacent gap that the fix's own justification wrongly claimed was closed.

---

## §8.1 I-1 is DISCHARGED — measured on all three arms, not read

The I-1 fix is a **test** change, and a test fix is exactly the kind that can be cosmetic. It
was therefore measured by mutation, never by reading the diff and agreeing.

**Method (every run).** Mutations applied in an **isolated `git worktree --detach`** at
`80978e8`, never in-place (during the first review a concurrent subagent's
`git checkout -- <file>` silently clobbered an in-place mutation and nearly produced a false
conclusion). Every run grepped for **`Compiling envoy-http1`** (a stale test binary yields a
FALSE PASS that makes a new test look vacuous) and the mutation was **re-grepped as still
present AFTER each run**, not merely before. The target was **named** (`--lib`) and the
verdict taken from the **`N passed` count, never the exit code** — `cargo test -p <pkg>
<name>` can exit 0 reporting `0 passed; N filtered out`, meaning the test never ran.

| # | Mutation of `compile_access_log_filter` (`hcm.rs:1746-1750`) | `Compiling` | Result | The assertion that caught it |
|---|---|---|---|---|
| — | *(baseline, unmutated)* | 1 hit | **1 passed** | — (proves the test genuinely RUNS) |
| A | `Eq => FilterOp::Le` **and** `Le => FilterOp::Eq` (the first review's swap) | 1 hit | **RED** | `Eq 404 filter on status 403: expected should_log=false` |
| B | `Le => FilterOp::Eq` **only** | 1 hit | **RED** | `Le 200 filter on status 100: expected should_log=true` |
| C | `Ge => FilterOp::Le` **only** | 1 hit | **RED** | `Ge 500 filter on status 499: expected should_log=false` |
| F | `threshold: …default_value` → `threshold: 0` | 1 hit | **RED** | `Ge 500 filter on status 499: expected should_log=false` |

**Each arm fails for its own distinct reason**, which is the property that matters: `assert_eq!`
bails at the first failing assertion, so a single mutation would leave the later legs' bite
unproven. Mutation **F** additionally pins the `threshold`'s **provenance** (that it comes from
`default_value` and not a constant) — a site the first review never asked about.

### The re-entry's sufficiency argument — MEASURED TRUE, not accepted

The re-entry argued the `Le` leg's `(100, true)` probe is **load-bearing**: a naive `Le 200`
table of only `(200,true),(201,false)` is **also satisfied by `Eq 200`** and would stay green
under the very mutation the fix exists to catch. That is a hypothesis, so it was measured
rather than believed:

```
# Le leg weakened to &[(200, true), (201, false)]  AND  mutation B (Le => Eq) applied
test result: ok. 1 passed; 0 failed          <-- GREEN: the arm's coverage is VACUOUS
(both mutations confirmed present after the run; Compiling envoy-http1 = 1 hit)
```

**The claim is TRUE.** Without `(100, true)` the `Le` arm is pinned by nothing. The table's
shape is deliberate and correct, and each of the three rows is uniquely satisfied by its own
operator (all six op×row combinations were checked).

---

## §8.2 Claims re-verified rather than inherited

Facts handed to this session by the ledger were re-measured where they were load-bearing.

1. **"No production code changed" — CONFIRMED.** `git diff b860e4e..2763c73 -- crates/`
   contains exactly two test bodies. `compile_access_log_filter` reads `Eq => Eq`, `Ge => Ge`,
   `Le => Le` with **no `_` wildcard** (so a future oneof arm is a compile error, not a silent
   fallthrough).
2. **The H1 mapping test covers H2 as well — CONFIRMED INDEPENDENTLY.** The first review's
   §3.1 concluded H2 owns no sink construction. Re-derived here from scratch:
   `compile_access_log_filter` exists **only** in `crates/envoy-http1/src/hcm.rs:1741`; the
   **only production** `FileSink::new` is `crates/envoy-http1/src/hcm.rs:212` (fed by
   `entry.filter.as_ref().map(compile_access_log_filter)` at `:208`); **every**
   `FileSink::new` in `crates/envoy-http2/src/hcm.rs` sits at line ≥2214 while that file's
   `#[cfg(test)]` begins at line **1286** → all test-only; and `envoy_http2::HCMConfig` is
   `{ pub inner: Arc<Http1HCMConfig>, h2_pool_mgr }`. **The single table-driven test pins the
   sole production config→runtime path for BOTH codecs.**
3. **CF-70-2 is CLOSED and the closure is honest — CONFIRMED** (§8.4, M70-R5).
4. **M70-R1 is still accurately characterized — CONFIRMED.** The doc comment at
   `bootstrap.rs:5097-5098` does still claim the one-element `set_arms` array is "written to
   stay correct as future phases add arms"; it is hand-maintained and is not. Deferring it to
   the phase that lands oneof arm #2 (alongside CF-70-1, the same surface) remains correct:
   failure is loud (a boot reject), not silent-wrong.

---

## §8.3 Important (must fix — triggers a second §5.2 state-3 re-entry)

**I-2 — the serde token→variant mapping is unpinned for `EQ` and `LE`, and the I-1 fix's own
justification wrongly claims otherwise.**

- **Files:** `crates/envoy-config/src/bootstrap.rs:747-754` (the `#[serde(rename)]`
  attributes); the false claim at `crates/envoy-http1/src/hcm.rs:4566-4568` and again in
  `PROGRESS.md` §R1.
- **What is wrong.** I-1 was "config `ComparisonOp` → runtime `FilterOp` is unpinned for `Eq`
  and `Le`". The fix pins that `match` — but it drives it with **Rust struct literals**
  (`hcm_config_with_filtered_access_log` takes `ComparisonOp` as a parameter), so it **never
  crosses the serde boundary**. The translation `op: EQ` (YAML) → `ComparisonOp::Eq` (Rust) is
  a *separate* mapping, expressed in `#[serde(rename)]` attributes, and it is pinned by
  nothing for `EQ` and `LE`. `GE` is the only token any test parses.
- **The fix's own doc comment asserts the opposite.** `hcm.rs:4568` reads "*the envoy-config
  tests pin that `op: EQ` parses; this is what connects the two*". That is **false**, and the
  re-entry's own record disproves it: `PROGRESS.md` §R1 quotes
  `grep -rn "op: EQ\|op: LE" crates/ tests/` → **"(zero hits)"**, then six lines later asserts
  the `envoy-config` tests prove `op: EQ` parses. Both cannot be true. Re-run at `80978e8`,
  the only `op: EQ` in the tree is **that doc comment itself**; `op: LE` has zero hits.
- **MEASURED, not hypothesized.** In an isolated worktree the `EQ`/`LE` **renames** were
  swapped (`#[serde(rename="LE")] Eq`, `#[serde(rename="EQ")] Le`; the Rust variant names —
  and therefore every literal construction site — untouched):

  ```
  Compiling envoy-config                        (forced rebuild confirmed — 1 hit)
  mutation present at run time                  (confirmed after the run)
  test result: ok. 102 passed; 0 failed         (envoy-accesslog)
  test result: ok. 611 passed; 0 failed         (envoy-config)
  test result: ok. 173 passed; 0 failed         (envoy-http1)
  → 886 passed / 0 failed — GREEN UNDER THE SWAP
  ```

  **886/0 is the exact total the re-entry reported as its GREEN** (`PROGRESS.md` §R6). The
  suite cannot tell the two trees apart.
- **The wrong behavior is real, and was demonstrated end-to-end through PRODUCTION YAML.** A
  temporary probe driven through the **existing** seam `compiled_filter_from_bootstrap_yaml`
  (`hcm.rs:4716` — it calls `envoy_config::parse_bootstrap` and the real
  `compile_access_log_filter`):

  ```
  # against the swapped-rename tree:
  test result: FAILED. 0 passed; 1 failed
  op: EQ 404 must DROP a 403 (got logged)
  # with the renames restored (Compiling envoy-config = 1 hit):
  test result: ok. 1 passed; 0 failed
  ```

  So a user config `op: EQ, default_value: 404` compiles to an `Le` predicate and **logs every
  record at or below 404 instead of exactly 404** — silently, with the whole repository green.
  That is I-1's failure scenario verbatim, one layer up.
- **Why Important and not Minor.** This is the **fourth** instance of this phase's recurring
  defect class ("a test that could not fail"): the state-3 review found two, the first state-5
  review found I-1, and this is the next link in the same chain. It is a **test-coverage gap,
  not a behavioral bug** — the renames are correct **as written**; the defect is that nothing
  holds them correct. Severity matches I-1's for the same reason: the failure is **silent
  wrong behavior**, not a fail-loud boot reject.
- **How to fix (cheap — the seam already exists, and the RED is already proven).**
  `compiled_filter_from_bootstrap_yaml` (`hcm.rs:4716`) already drives production YAML through
  the full serde→validator→compiler path. Its caller `bootstrap_yaml_with_runtime_key`
  (`hcm.rs:4732`) hard-codes `op: GE` at `hcm.rs:4755`. **Parameterize the op token** and
  table-drive all three through the seam — e.g. `op: EQ`/404 → drops 403, keeps 404, drops
  405; `op: LE`/200 → keeps 100, keeps 200, drops 201. That kills the swap mutation above.
  Per D-3.1 do it under TDD: prove RED against the swapped renames first (the probe above is a
  working RED), then restore and confirm GREEN. **Do the mutation in a scratch git worktree**,
  never in-place. **Also correct the false claim** at `hcm.rs:4566-4568` and `PROGRESS.md` §R1.
- **Scope note.** Fixing I-2 requires **no production change** — it is one test + two comment
  corrections. It does **not** reopen the phase-70 config surface (ADR-0142 stays settled).

---

## §8.4 The two folded Minors — both genuinely CONSUMED (measured)

**M70-R3 — CONSUMED, and the tightening BITES.** The test
`rejects_status_code_filter_unknown_op` (`crates/envoy-config/src/bootstrap.rs:12958`) now
also asserts the rejection **names the offending token**. Measured in an isolated worktree by
making `op` **valid** (`GE`) while typo-ing an **unrelated** field (`path` → `pathx`):

```
Compiling envoy-config                       (1 hit; mutation present after the run)
# the OLD assertion `matches!(err, ConfigError::Yaml(_))` — STILL SATISFIED (execution
# reached the new assertion), i.e. the wrong-reason-green it describes is REAL
# the NEW assertion — RED:
rejection must name the offending op token, got "parsing bootstrap YAML: static_resources
.listeners[0].filter_chains[0].filters[0]: unknown field `pathx`, expected `path` or
`log_format` at line 9 column 15"
```

The real message for the unmutated test is ``unknown variant `NE`, expected one of `EQ`,
`GE`, `LE` at line 9 column 15`` — "NE" occurs **only** in the offending token (not in
`EQ`/`GE`/`LE`, "unknown variant", the lowercase serde path, or the fixture's `/tmp/al.log`),
so `contains("NE")` cannot pass for a wrong reason here. See **M70-R6** for the residual nit.

**M70-R5 — CONSUMED: CF-70-2 is CLOSED.** `PROGRESS.md` §6 now strikes the entry through
(`~~latent expected_lines == 0 in the differential arms~~ — CLOSED`), **preserves the original
warning inline** rather than deleting it (D-3.5), and records the falsifying measurement so a
stranger sees *why* it is closed without opening this file. The disposition is drawn
deliberately against its live siblings: CF-70-1 and CF-70-3 both retain named owners, while
CF-70-2 alone says "**No owner, no action: this is CLOSED, not carried**". **No file lists
CF-70-2 as live** — `STATE.md:15` and `:126` both say CLOSED. The remaining mentions are in
append-only historical records (`PROGRESS.md` §V7, and §7 of the first review above), which is
correct: those are frozen text, not live claims. See **M70-R8** for the residual nit.

---

## §8.5 Minor (carry-forwards — none blocks; fold the cheap same-file ones with I-2 if convenient)

**Still live from the first review, unchanged:** **M70-R1** (the hand-maintained `set_arms`
array + its overclaiming doc comment — discharge with oneof arm #2, alongside CF-70-1, the
same surface), **M70-R2** (`expected_logged_count`'s wiring into the two byte-exact arms has
no in-process witness; re-confirmed accurate — the helper is at
`tests/differential/src/lib.rs:1134`, wired at `:6258` and `:6401`, pinned in isolation only
by `expected_logged_count_excludes_suppressed` at `:7314`), **M70-R4** (`AccessLog.filter`
serializes as `"filter": null` — no `skip_serializing_if`; re-confirmed that the sibling
`FileAccessLog.log_format` does the identical thing, so phase 70 extends an existing pattern
rather than introducing one).

**NEW, opened by this re-review:**

**M70-R6 — the `contains("NE")` assertion is unanchored.** `bootstrap.rs:12967`. Measured
robust *today* (§8.4), but it is a 2-character substring that asserts neither "this is an
unknown-**variant** error" nor "it is about the **`op`** field". `grep -rn 'rename = "[^"]*NE[^"]*"' crates/envoy-config/src/`
is currently zero-hit, so no other token contains uppercase `NE` — but a future enum gaining a
`NONE`-like variant would silently weaken it. `msg.contains("unknown variant `NE`")` is
strictly stronger at zero cost. **Preference, not a defect.**

**M70-R7 — the I-1 fix's doc comment misdescribes the table's probe shape.**
`crates/envoy-http1/src/hcm.rs:4563-4565` claims "*Each leg probes a status the operator must
KEEP and statuses on **BOTH sides** that it must DROP*". Only the `Eq` leg does (403 **and**
405). `Ge 500` drops on one side only (499; 503 is kept) and `Le 200` drops on one side only
(201; 100 is kept) — a `Ge`/`Le` predicate **cannot** drop on both sides, so the sentence is
unsatisfiable for two of three legs. The **conclusion** it draws ("no other operator satisfies
the same row") is nonetheless **correct** (verified over all six op×row combinations). Wrong
rationale, right table. `PROGRESS.md` §R2's own wording is accurate; only the code comment is
wrong. **Fix with I-2** (same file, same comment block).

**M70-R8 — the CF-70-2 closure elides a sentence instead of striking it (D-3.5 hygiene).**
`PROGRESS.md:310-323`. The original entry (`git show 2d272aa:…PROGRESS.md`) ended
"*Unreachable from `0076`. Owner: the next filter fixture.*" The load-bearing part (the
falsified warning) **is** preserved essentially verbatim, and "Owner:" is explicitly superseded
by "No owner, no action" — so D-3.5's intent is met — but "Unreachable from `0076`." was
dropped with no strikethrough. **Fix:** strike the full original sentence set rather than
eliding the tail.

**M70-R9 — a provenance error in §3.3 of the first review (above).** `REVIEW.md:174` dates the
`FileAccessLog.log_format` precedent "shipped since phase 38". It actually shipped in **phase
32** (`c869f91`, "phase 32 t4: FileAccessLog.log_format config field", already
`#[serde(default)]` with no `skip_serializing_if`); the phase-38/ADR-0092 attribution belongs
to the `SubstitutionFormatString` oneof **type**, not to the field's serde shape. Recorded
here rather than edited above, per D-3.5. **This does not weaken M70-R4** — if anything the
pattern predates phase 70 by *more* than the first review claimed. (Cosmetic sibling: the first
review's M70-R2 cites `:6255`/`:6398`; the actual call sites are `:6258`/`:6401` — the cited
lines are the first line of the explaining comment block, so it points at the right code.)

---

## §8.6 Doctrine compliance (re-checked over the re-entry head)

| Rule | Status |
|---|---|
| D-3.1 TDD (RED→GREEN per task) | ✅ the re-entry proved its RED 4×, arm by arm, in isolated worktrees |
| D-3.2 dependency stance | ✅ no new crate, no new dependency |
| D-3.3 differential-over-fidelity | ✅ `0076` is pure cross-proxy equality |
| D-3.4 stranger-readability | ✅ — but see **I-2**: `PROGRESS.md` §R1 contains a self-contradiction a stranger would have to catch |
| D-3.5 decisions written | ⚠️ met in substance; **M70-R8** is a hygiene nit on the CF-70-2 elision |
| D-3.6 green build | ✅ state-4 re-verification (a)–(e) PASS over `2763c73`; CI GREEN on `80978e8` (run `29524535731`) |
| D-3.7 version pin | ✅ `envoyproxy/envoy:v1.33.0` untouched |
| D-3.8 `#![forbid(unsafe_code)]` | ✅ holds; no `unsafe` added |
| §6.1 split gate | ✅ does not fire |
| §7.4 fuzz | ✅ corpus seed on the existing target; no new target → no `ci.yml` step owed |
| Scope discipline (ADR-0140) | ✅ only `status_code_filter` shipped |
| ADR-0142 (§E.1 boundary) | ✅ **NOT re-litigated** — the phase-70 config surface stays CLOSED |
| `known-failures.txt` | ✅ untouched, not trimmed |

**No ADR fired by this re-review** — I-2 is a coverage gap, not an ambiguity; it settles no
decision. Next-available **ADR-0143** remains unreserved.

---

## §8.7 Next session — a SECOND §5.2 state-3 re-entry (NOT state-4, NOT state-6)

Per §5.2 an Important re-enters at **state 3**: you are resuming implementation **under TDD**,
not re-verifying. **Do not re-run the §7.5 gate** — state-4 measured (a)–(e) green over this
head; a state-4 **re**-verification is owed only *after* the I-2 fix lands.

**The single required fix — I-2** (§8.3): pin the YAML-token → `ComparisonOp` mapping for `EQ`
and `LE` by table-driving `bootstrap_yaml_with_runtime_key`'s hard-coded `op: GE`
(`hcm.rs:4755`) through the existing `compiled_filter_from_bootstrap_yaml` seam
(`hcm.rs:4716`). **Prove the RED first** by swapping the `EQ`/`LE` renames
(`bootstrap.rs:748-753`) — a working RED is quoted in §8.3. Grep every run for
`Compiling envoy-config`; do the mutation in a **scratch git worktree**, never in-place. Then
**correct the false claim** at `hcm.rs:4566-4568` and in `PROGRESS.md` §R1 (a strikethrough
correction, D-3.5).

**Cheap same-file folds** (safe to take in the same re-entry): **M70-R7** (the "BOTH sides"
comment — same comment block as I-2's correction), **M70-R6** (anchor the `contains("NE")`
assertion), **M70-R8** (strike CF-70-2's elided sentence). **M70-R1** stays with oneof arm #2
alongside CF-70-1; **M70-R2**/**M70-R4**/**M70-R9** remain reasonable carry-forwards.

After the fix lands: a state-4 **re**-verification (fresh session, full §7.5 gate over the new
head), then a state-5 **re**-review, then the state-6 close-out. **This session did NOT chain
into any of them** (§5.1 — one state per session).

**Carry-forwards after this re-review.** Live: **CF-70-1** (the zero-arm `expect()`; arm #2's
phase must convert it to a full match), **CF-70-3** (false-pass-only), **M70-R1**, **M70-R2**,
**M70-R4**, and the new **M70-R6/R7/R8/R9**. **CONSUMED:** M70-R3, M70-R5 (and **I-1**).
**CLOSED:** ~~CF-70-2~~ (premise measured FALSE — do not re-open). Not consumed by this phase
and still live: **M69-A..I**, **CF-69-1/2/3/5**, **M68-1**, **M-1**, **CF-67-3/5/6/7**, the
older Minors, and the HTTP-filters-family (1)–(4).

---
---

# §9. THE SECOND RE-REVIEW (§5 state-5 RE-REVIEW, 2nd) — the re-issued verdict

> **Written by the §5 state-5 RE-REVIEW (2nd) session** (`superpowers:requesting-code-review`),
> **appended to — never rewriting — §1–§8**, per D-3.5. Written for a stranger with zero prior
> context (D-3.4).
>
> **Session provenance (recorded per the ADR-0127 precedent).** This re-review ran in the SAME
> operator-supervised session as the state-4 RE-VERIFICATION (2nd), at **explicit human
> instruction** ("continue with the state-5 re-review") — the phase-66 human-authorized-chain
> precedent ADR-0127 records. §5.1 remains binding on every autonomous session. The self-review
> hazard is bounded here because the decisive measurements grade the **second re-entry
> session's fix** (a different context wrote it), and the state-4 evidence this session might
> otherwise have had to trust was instead **numerically corroborated** — the GitHub credential
> was restored mid-session, and ADR-0143's backstop was executed (below) rather than waived.
>
> **Reviewed subject:** the second §5.2 re-entry (`1c6a5c2..60a5272` — the I-2 fix + the
> M70-R6/R7/R8 folds), measured over the CURRENT head `8844445` (a parallel workstream landed
> 4 commits after the state-4 commit `64218fa` — an `envoy-http2` header-list bound + tests +
> bench/docs — **outside phase-70 scope**; verified: `bootstrap.rs` is untouched since
> `60a5272` and the parallel `hcm.rs` delta touches none of the phase-70 sites, so measuring at
> `8844445` covers the fix on both heads, strictly stronger).
>
> ## VERDICT: **APPROVED — 0 Critical / 0 Important / remaining Minors are carry-forwards.**
>
> **I-2 is DISCHARGED** (measured, §9.1). **M70-R6, M70-R7, M70-R8 are CONSUMED** (measured +
> audited, §9.2). **§7.5 gate (f) is now MET.** The phase advances to the **state-6 close-out**
> (its own session).

## §9.1 I-2 is DISCHARGED — measured arm-by-arm, not read

Method identical to §8.1's discipline: every mutation in an **isolated `git worktree --detach`**
at `8844445` (never in-place), every run grepped for **`Compiling envoy-config`** (forced
rebuild), the mutation **re-grepped as present AFTER each run**, the target **named** (`--lib`),
every verdict from the **`N passed`/`N failed` counts** (the baseline shows `174 filtered out` —
the count, not the exit code, is the evidence the test ran).

| # | Mutation of the `#[serde(rename)]` attributes (`bootstrap.rs:747-754`; variant names untouched) | `Compiling` | Result | The assertion that caught it |
|---|---|---|---|---|
| — | *(baseline, unmutated)* | 1 hit | **1 passed** | — (proves the test genuinely RUNS) |
| A | `EQ`⇄`LE` renames swapped (§8.3's kill) | 1 hit | **RED — 0 passed / 1 failed** | `op: EQ 404 on status 403: expected should_log=false (the YAML token compiled to the wrong FilterOp)` |
| B | renames restored, then `GE`⇄`LE` swapped | 1 hit | **RED — 0 passed / 1 failed** | `op: GE 500 on status 499: expected should_log=false` |
| — | *(all restored; worktree `git status` 0 dirty, `git diff HEAD` 0 lines)* | 2 hits | **GREEN — 1 passed** | — |

The two swaps fail for **distinct, arm-specific reasons**, so the tokens are pinned
independently (the re-entry's third leg — the LE-first reorder proving the `(100,true)` probe
bites — is quoted with its distinct RED in `PROGRESS.md` §R(2)3 and was not re-run here; the
`(100,true)` probe's load-bearing property was already measured by §8.1's vacuity experiment).
The landed test drives all three tokens through the REAL path
(`bootstrap_yaml_with_filter` → `compiled_filter_from_bootstrap_yaml` → `parse_bootstrap` →
validators → `compile_access_log_filter`) — the seam I-1's literal-driven table cannot reach.
**The defect I-2 named cannot recur silently.**

## §9.2 The three folded Minors — CONSUMED (measured + independently audited)

**M70-R6 — CONSUMED and the anchor BITES.** `rejects_status_code_filter_unknown_op` now asserts
``msg.contains("unknown variant `NE`")`` (`bootstrap.rs:12970`) with the explaining comment.
Measured in the worktree by §8.4's wrong-reason probe: `op` made VALID (`GE`) while an unrelated
field is typo'd (`path` → `pathx`) → the old `matches!(Yaml(_))` is still satisfied, and the NEW
assertion goes **RED**: `rejection must name the offending op token, got "… unknown field
\`pathx\`, expected \`path\` or \`log_format\` …"` (`0 passed / 1 failed`, compile confirmed,
probe re-grepped present). Restored → **GREEN 1 passed**.

**M70-R7 + M70-R8 + the I-2 comment corrections — CONFIRMED by an independent READ-ONLY
auditor** (a fresh-context subagent, no cargo, no writes): the Task-6 doc block strikes both
original claims with corrections alongside (`hcm.rs:4570-4584`); `PROGRESS.md` §R1 strikes the
false claim and names its self-contradiction with the zero-hit grep quoted six lines earlier;
CF-70-2's elided tail is restored struck-through (`PROGRESS.md:315-316`); and a tree-wide sweep
found **no live, unstruck copy** of the false claim (every remaining hit is inside a `~~…~~`
strike or an append-only historical record). One cosmetic note, correctly frozen: §R(2)1 cites
the struck block's pre-fix location `hcm.rs:4566-4568`; it now sits at `4575-4584` (append-only
record — not edited, noted here per D-3.5).

## §9.3 ADR-0143's numeric backstop — EXECUTED, and it corroborates exactly

The GitHub credential was restored during this session (`gh auth status`: logged in via
keyring), so the backstop ADR-0143 directed at this re-review was run rather than waived:

```
$ gh run view 29596323921 --log | grep -oE "test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed" \
    | awk '{p+=$4; f+=$6} END {print "CI passed="p" CI failed="f}'
CI passed=2023 CI failed=0          # run on 60a5272 — the state-4 gate subject
$ …same for run 29622523725…
CI passed=2028 CI failed=0          # run on 8844445 — the current merged head
```

**`2023` is EXACTLY the total ADR-0143's substitute chain predicted** (`2022 + 1`), and `2028`
equals the merged head's local enumeration (`2017 + 11`, measured when the parallel push was
pulled and verified). The substitute evidence is therefore **numerically corroborated on both
identities** — no contradiction, no unexpected state. ADR-0143's protocol note stands for any
future credential outage; the standard numeric check is back in force.

## §9.4 Doctrine compliance (re-checked)

| Rule | Status |
|---|---|
| D-3.1 TDD | ✅ the re-entry's RED proven token-by-token (§R(2)3), reproduced here (§9.1) |
| D-3.4 stranger-readability | ✅ the §R(2)/§V(3) records read cold; the audited corrections close I-2's doc debt |
| D-3.5 decisions written | ✅ strikethrough corrections everywhere; ADR-0143 records the one genuine ambiguity |
| D-3.6 green build | ✅ §V(3) gate (a)-(e) PASS over `60a5272`; CI `success` on `60a5272`, `64218fa`, and `8844445` |
| D-3.7/D-3.8 | ✅ pin untouched; `#![forbid(unsafe_code)]` holds |
| Scope (ADR-0140/0142) | ✅ no production change since `1c6a5c2` on the phase-70 surface; config surface CLOSED |
| `known-failures.txt` | ✅ untouched |

**No new ADR fired by this re-review** (next-available **ADR-0144**, unreserved).

## §9.5 Carry-forwards after this re-review (the state-6 close-out inherits this list)

**Live:** **CF-70-1** (the zero-arm `expect()` — arm #2's phase MUST convert it to a full
match, alongside **M70-R1**, same surface), **CF-70-3** (false-pass-only `wait_file_lines`
window — next access-log-filter phase), **M70-R2**, **M70-R4**, **M70-R9**.
**CONSUMED:** I-1, I-2, M70-R3, M70-R5, M70-R6, M70-R7, M70-R8.
**CLOSED:** ~~CF-70-2~~ (do not re-open).
Unconsumed and still live: **M69-A..I**, **CF-69-1/2/3/5**, **M68-1**, **M-1**,
**CF-67-3/5/6/7**, the older Minors, and the HTTP-filters-family (1)–(4).

## §9.6 Next session — the §5 state-6 CLOSE-OUT (its own session)

Gate (f) is MET; all six §7.5 sub-gates now hold. The close-out (memory
`closeout-and-pick-are-separate-sessions`): flip ROADMAP row `70` → `done` (6 cells preserved;
the `op: EQ\|GE\|LE` escapes are CORRECT), relocate the closed-phase Notes + the four
top-section blocks to `STATE_HISTORY.md` (DELTA-based byte-preservation check, memory
`state6-relocation-check-must-be-delta-based`), set STATE → awaiting next planning. The
next-phase state-0/1 pick is a SEPARATE session again.
