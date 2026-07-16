# Phase 70 — `status_code_filter` — §5 state-5 CODE REVIEW

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
> ## VERDICT: **NOT ready to merge — 1 Important.**
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
