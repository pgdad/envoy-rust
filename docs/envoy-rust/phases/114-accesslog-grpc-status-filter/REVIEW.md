# Phase 114 — the `grpc_status_filter` access-log FILTER arm — CODE REVIEW

> **§5 state 5.** This document is the state-5 output for phase `114` and it **CLOSES §7.5 gate (f)**,
> the only gate the state-4 session left open. Written by a THIRD context: the state-3 session
> implemented, the state-4 session graded, and neither may review (§5.1; `ADR-0127`).
>
> **Verdict: APPROVED.** §2 (Issues — Must Fix) is **EMPTY**, so the state machine advances to
> **state 6**, not back to state 3 (§5.2).
>
> **This review wrote no code, no test and no fixture, and edited no landed artifact.** Every finding
> below is **BANKED** as a carry-forward, not fixed (§6.3; `ADR-0165`: a phase banks, it never clears
> — and a REVIEW banks its own findings too).
>
> **Findings: 13 Important, 36 Minor, opening `CF-114-6` … `CF-114-21`.** `ADR-0199` fires: the verdict
> and the banked set are decisions.
>
> **No landed NUMBER that carries a decision was contradicted.** Net 1038 over 14 files, the 938
> prototype table reconciling to four files, 17 added `#[test]` attributes against 0 removed, the five
> production `should_log` sites, 94 fixtures / 93 runners, the ADR head at `0198`, `ROADMAP.md` row
> `114` still `planned`, and the CI identity `binaries=170 passed=2315 failed=0` all reproduced exactly.
>
> **But eight things this review found are worse than ordinary drift.** (1) **This phase repeated
> the very regression its own Task 1 landed to repair** — a new helper was spliced between an existing
> function's doc comment and its signature, leaving `build_access_log_record` undocumented. (2)
> **`CF-114-5`'s premise is false**: the header-parse behaviour it calls "unmeasured upstream" was
> MEASURED in-tree by phase 113, and it points the other way — which makes the landed fallback a
> **predicted divergence**, now locked in by a green test. (3) **The H2 arm's declared "sole witness"
> is vacuous** — it calls the H1 helper directly and would pass against a stub at the H2 record build;
> phase 114 is the first filter-arm phase to skip the end-to-end H2 test its three predecessors all
> shipped. (4) **Twelve of the seventeen canonical name→code bindings are asserted nowhere**, and the
> accept sweep is a byte-copy of the table it tests. (5) **The header leg's producer shares the
> predicate with the fallback leg** across the entire tested surface, so a bug swapping the two legs is
> unwitnessable by fixture `0094`. (6) **A landed `docs/` numstat is wrong in BOTH of its figures**, and wrong
> self-referentially. (7) **This phase broke a citation in a LIVE document it edited itself** — not an
> append-only archive. (8) **The record's two enumerations of "nine `PLAN.md` corrections" are two different
> nines**, and one of them says the other contains it.

---

## §0 — How this review was conducted

### §0.1 — Scope

The **code** surface is the **1038 net lines** of the arc from base `c9136ae5f89ada5f1e64e93e2a1a62a19bc5fcc9`
(the state-2 CI record, parent of Task 1) to `762d3ccd141971a80943a8d79a95983f7275ff7a` (the state-4
verification gate). Re-derived at this session with
`git diff --numstat c9136ae..762d3cc -- . ':(exclude)docs/'` (⚠ in `--numstat` the PATH is field `$3`,
not `$2`):

| file | + | − | net |
|---|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | 260 | 9 | 251 |
| `crates/envoy-accesslog/src/filter.rs` | 178 | 79 | 99 |
| `crates/envoy-http1/src/hcm.rs` | 214 | 64 | 150 |
| `crates/envoy-http2/src/hcm.rs` | 42 | 10 | 32 |
| `crates/envoy-accesslog/src/record.rs` | 11 | 0 | 11 |
| `crates/envoy-accesslog/src/file_sink.rs` | 17 | 9 | 8 |
| `crates/envoy-config/src/lib.rs` | 26 | 19 | 7 |
| `crates/envoy-http1/src/grpc.rs` | 6 | 1 | 5 |
| `crates/envoy-http1/src/lib.rs` | 1 | 0 | 1 |
| `tests/differential/tests/accesslog_grpc_status_filter.rs` | 24 | 0 | 24 |
| `tests/fixtures/0094-…/README.md` | 166 | 0 | 166 |
| `tests/fixtures/0094-…/envoy-rust.yaml` | 91 | 0 | 91 |
| `tests/fixtures/0094-…/envoy.yaml` | 93 | 0 | 93 |
| `tests/fixtures/0094-…/expectations.yaml` | 100 | 0 | 100 |
| **TOTAL (14 files)** | **1229** | **191** | **1038** |

Plus, excluded from the §6.1 gate: `BEHAVIOR_CONTRACT.md` `135 0`, `DECISIONS.md`, `PROGRESS.md` `328 0`
at the gate commit, `STATE.md` `29 16`, `STATE_HISTORY.md` `28 0`.

**1038 against the PLAN's MEASURED 938 is 1.107×**, re-derived here and matching `ADR-0198` DECISION 4
exactly. The §6.1 gate is ~25 tasks OR ~1500 net LoC; this is 10 tasks and 1038, so it clears by 462
lines / 31% and does not fire on either axis.

⚠ **One commit lands after the reviewed arc and is NOT part of the review surface:** `cadf2d8`, this
session's own follow-up commit recording the state-4 CI confirmation. It is docs-only (`STATE.md`, `2 0`)
and touches no reviewed file.

**Document precedence, later wins:** `SPEC.md` < `PLAN.md` < `PROGRESS.md`/`ADR-0198`. `ADR-0197`
corrects five landed `SPEC.md` claims; `ADR-0198` corrects nine `PLAN.md` claims. This review adjudicates
against the LATEST binding statement of each claim, not against the SPEC.

### §0.2 — Method

Six read-only reviewers were fanned out over the independent dimensions of the surface — the
`envoy-config` grammar and validator, the `envoy-accesslog` predicate and record, the two codecs'
derivation and wiring, fixture `0094` and its driver, a documentation and citation audit, and a
dedicated **untested-composition** probe. Each was given full zero-context instructions (D-3.4), told
**not** to run `cargo` (the gate is discharged and the lock serializes), told not to mutate the tree,
and told that a positive control must precede any reported zero.

⚠ **A SUBAGENT FINDING IS A CLAIM.** Every finding below was re-verified on disk by this main session
before being recorded. **One was NOT CHARGED** (a claim that eight token-grammar cells carry a MEASURED
label without evidence — the measurement exists, in `ADR-0197` DECISION 6, which the reviewer's census
did not weigh), **two were DOWNGRADED**, and **one briefing claim of this session's own was corrected**.
All four are adjudicated in §5.

**Three of the Important findings were reached independently by more than one path**, which is
corroboration rather than agreement because the paths did not share a method: the H2 pin's vacuity was
found by this session's call-edge trace, by a census-with-control, and by a coverage walk; the doc-comment
theft by a diff read and a base-vs-HEAD `git show`; the name→code gap by a sequence comparison and by a
per-name occurrence count. ⚠ By contrast, where `SPEC.md`, `PLAN.md` and `BEHAVIOR_CONTRACT.md` agree on
the 17-row token table, that is **not** corroboration — all three descend from one measurement session.

### §0.3 — The §7.5 gate was NOT re-run

Legs (a)–(e) were discharged at the state-4 gate (`762d3cc`) and are quoted with real command output in
`PROGRESS.md`'s `# §5 STATE 4` section. Re-running them here would burn a Docker corpus sweep to reproduce
a result already recorded, and §5.1 makes the grading context and the reviewing context distinct precisely
so the review reads the evidence rather than regenerating it. This review **inherits legs (a)–(e) as data
and re-verifies them as claims** — by re-deriving the figures they rest on from disk, not by re-executing
them. Leg (f) is this document.

What this session DID re-derive from disk, independently:

| claim | source | re-derived here |
|---|---|---|
| net code lines | `ADR-0198` D4: 1038 | **1038** (added 1229 / deleted 191, 14 files) ✅ |
| the prototype reconciliation | `ADR-0198` D4 | `938 + 93 + 6 + 2 − 1 = 1038`, ten files at Δ0 ✅ |
| production `should_log` sites | `ADR-0197` correction 1: FIVE | column-0 `#[cfg(test)]` boundary walk over `git ls-files crates/` → **138 total, 133 test, 5 production**, at the five cited lines ✅ |
| the two gRPC tables differ on exactly one cell | `ADR-0197` D7 | extracted both, compared elementwise → **1 differing cell (index 1, `CANCELED`/`CANCELLED`), 16 agreeing** ✅ |
| PV-9 — four untouchable paths | `PROGRESS.md` | `git diff --numstat c9136ae..762d3cc --` on `Cargo.toml`, `Cargo.lock`, `ci.yml`, `tests/differential/src/lib.rs` → **empty**, against a positive control of `1 0` on `envoy-http1/src/lib.rs` ✅ |
| no suppression added | `PROGRESS.md` | added lines containing `#[allow(` / `#[expect(` / `unsafe` / `#[ignore]` = **0 each**, against controls of 1229 added lines and 24 containing `#[` ✅ |
| `#![forbid(unsafe_code)]` in all crate roots | `PROGRESS.md` | 14 roots, **0 missing** ✅ |
| +17 tests, −0 | `ADR-0198` Consequences | **17** added `#[test]`/`#[tokio::test]`, **0** removed ✅ |
| fixture / runner census | `ADR-0198` Consequences | **94** fixture directories, **93** differential runners ✅ |
| ADR head / next free | `PROGRESS.md`: `ADR-0199` free | highest = **0198**; 194 distinct ADRs (195 `^## ADR-` matches − the line-10 schema template); `ADR-0199` absent ✅ |
| ROADMAP untouched | `PROGRESS.md` | row `114` still `planned`, status at field 4 on a `' | '` split, file line 196 ✅ |
| `known-failures.txt` untouched | `PROGRESS.md` | `git diff --numstat` on it → **empty** ✅ |
| the four fixture-YAML hunks | state-4 gate | `diff` of the pair → exactly **4** hunks; the `filter:` block identical on both sides ✅ |
| `BEHAVIOR_CONTRACT.md` section | `PROGRESS.md`: 135 lines | `135 0`, one `### ` heading ✅ |
| `CF-75-5` still open | `PROGRESS.md` | tracked corpus seeds: `parse_bootstrap` 67, `accesslog_format_parse` 11, `jwt_parse` 3, `grpc_health_decode` 1, **`cdn_loop_parse` 0** ✅ |
| CI identity at the gate commit | PENDING line's PREDICTION: unchanged | **`binaries=170 passed=2315 failed=0`**, derived from the ANSI-stripped job log with `ok` and `FAILED` rows counted separately; recorded by this session in `cadf2d8` ✅ |

**Every one reproduced.** No decision-carrying figure in the record was contradicted by this review.

⚠ **One handoff claim was CORRECTED here, and it is the same correction the phase-113 review had to
make about the same warning.** The handoff states that the 40-hex SHA audit reports **three** known-false
`MISSING`es over `STATE.md` + `phases/114-…/` + `DECISIONS.md`. Run with the handoff's **own** stated
command — `grep -oE '\b[0-9a-f]{40}\b'` — over that exact file set across the arc, there is exactly
**ONE**: `70ac2294010887f48b18e2d64f5cccd48421fad1`, the h2spec 2.6.0 release build hash. The Docker-digest
truncation artifact does **not** appear, because the trailing `\b` cannot match mid-hex inside a 64-character
digest — only the unanchored `[0-9a-f]{40}` form truncates it, and the handoff does not use that form. And
`b0f43d67aa25c1b03c97186a200cc187f4c22db3` occurs **0** times in the phase's `docs/` diff; it lives in
`ENVOY_TARGET.md`, outside the audited set. **The warning is right that none is a defect; it is wrong that
three of them appear.**

⚠ **And THIS document is now a fixed point of that audit.** By naming
`b0f43d67aa25c1b03c97186a200cc187f4c22db3` above in order to explain that it is *not* a defect, `REVIEW.md`
has put it into the audited set: an audit run over this file reports **two** `MISSING`es, both known-false.
A self-referential census changes its own answer, and a warning that carries a COUNT rather than a RULE will
keep going stale for exactly this reason.

---

## §1 — Strengths

**The governing rule is implemented exactly as measured, and the hardest part of it is the part that is
right.** `effective_grpc_status` (`crates/envoy-http1/src/hcm.rs:1666`) is the response `grpc-status`
header if present and in range, else `http_to_grpc_status(response_code)` — **ungated**, on every request.
Both codecs populate the record from that one function unconditionally
(`crates/envoy-http1/src/hcm.rs:1792`, `crates/envoy-http2/src/hcm.rs:1202`): no surviving
`is_grpc_request` gate, no early return, no `Option` that defaults. The ungatedness survives all the way
to the predicate, which is a single expression at `crates/envoy-accesslog/src/filter.rs:194`.

**The trap the phase was most likely to fall into was avoided, and the avoidance is guarded.** Phase 113's
`AccessLogRecord.grpc_status: Option<String>` is gated on `is_grpc_request` and is a raw wire string; reusing
it was the obvious wrong move because it is already named `grpc_status`. It was not reused, not relaxed, not
unified and not repurposed — the H1 gate is intact and H2 still returns `None` from its hard-coded helper. The
new field is a separate plain `u8` at `crates/envoy-accesslog/src/record.rs:133`, and its doc contrasts the two
explicitly. `PROGRESS.md` records the mutation that proves it: gating the derivation makes fixture `0094` emit 2
lines where 5 are expected, and **the three lost are exactly probes 5, 6 and 8** — the specific prediction, not
merely "lines went missing".

**The predicate is one line and it satisfies three measured rules at once.**
`codes.contains(&grpc_status_code) != *exclude` (`filter.rs:194-196`). `!=` on `bool` is XOR, so membership,
`exclude` inversion and the empty-`codes`-keeps-nothing rule all fall out with no branch. It carries plain data
rather than a trait object, so the `ADR-0150` cycle seam is not involved and `envoy-accesslog` keeps its
zero-intra-workspace-dependency posture — verified: its `Cargo.toml` is byte-unchanged across the arc, and no
`use envoy_*` appears anywhere in its `src/`.

**The two gRPC status tables were kept apart, which is the phase's single most fragile cell.** The filter's
`GRPC_STATUS_FILTER_NAMES` (`crates/envoy-config/src/bootstrap.rs:790`) spells code 1 `CANCELED` with one L;
the formatter's `GRPC_STATUS_NAMES` (`crates/envoy-accesslog/src/command_operator.rs:114`) renders it
`CANCELLED` with two. Extracted and compared elementwise here: **exactly one differing cell, sixteen agreeing** —
which is precisely what makes the mistake easy to miss and the separation load-bearing. Both directions are
pinned by assertions that do **not** derive from the table: `CANCELED → Some(1)` at `bootstrap.rs:20509`, and a
full `parse_bootstrap` round trip rejecting `CANCELLED` with the offending token named at `bootstrap.rs:14976`.

**The oneof cardinality machinery grew in both places a compiler cannot cross-check, and a test covers the
gap.** The destructure at `bootstrap.rs:5818` has seven bindings and no `..`, and the `set_arms` array at
`:5827` has seven entries. Nothing in the language ties those two sevens together — but
`seven_arm_cardinality_counts_every_arm` (`:14878`) walks each arm alone and asserts `single_arms.len() == 7`.
That is the highest-risk cell of the config change and it is properly pinned.

**Validation recurses, so a bad token nested inside a composition is caught at boot.**
`validate_access_log_filter` re-enters itself for every `and_filter`/`or_filter` child (`bootstrap.rs:5875`
and `:5885`) before reaching the seventh-arm token check in the same function body (`:5901`). Traced end to
end: a `grpc_status_filter` at any nesting depth deserializes, validates, compiles and evaluates correctly.
That also makes the `.expect("validated by validate_access_logs")` at `crates/envoy-http1/src/hcm.rs:1914` a
real invariant rather than a hope — the validator and the compile step call the same
`resolve_grpc_status_token`, so they cannot disagree.

**The token grammar matches the measured accept and reject sets, including the counter-intuitive cells.**
`resolve_grpc_status_token` (`bootstrap.rs:816`) tries a permissive `trim().parse::<i64>()` first, then a
case-insensitive canonical-name match with no separator normalisation — so `not_found` accepts and `NotFound`
rejects, which is the cell a guessed rule gets wrong in both directions. The untagged `Num`-before-`Name` arm
order is load-bearing and the reason is written down at `:771`. The YAML-1.1-versus-1.2 hazard the SPEC feared
does **not** manufacture a reject-direction divergence: `serde_yaml`'s `parse_bool` accepts exactly
`true|True|TRUE|false|False|FALSE`, so bare `TRUE` booleanizes here too, and `y`/`n`/`on`/`off` reject on both
sides by different routes.

**Fixture `0094` witnesses the phase's actual finding rather than a restatement of it.** The `text_format`
renders the *formatter's* gated `%GRPC_STATUS%` beside the path, so the kept lines visibly carry `GS=-` on
probes 5, 6 and 8 while the filter still keeps them. Eight probes on eight distinct paths, each occurring
exactly once in the config, the runner and the expectations; `expected_status` set explicitly on every one
rather than relying on the driver's `200` default, which three probes would have failed. Probe 8's
`application/grpc; charset=utf-8` is a second, differently-shaped witness that the filter's gate is not the
formatter's.

**The `E0063`-on-every-literal property was used rather than worked around.** `AccessLogRecord` deliberately
does not implement `Default`, so the new field broke every exhaustive literal and the compiler produced the
work list. `ADR-0198` DECISION 1 records that the plan's *count* said four and its *enumeration* said five,
and that the enumeration was right — the correct resolution.

**The phase ends carrying zero suppressions, and the removal is checkable rather than remembered.** The one
transient `#[allow(clippy::only_used_in_recursion)]` the plan directed at Task 6 was deleted at Task 7.
Re-verified here: zero `#[allow(` anywhere in `crates/envoy-accesslog/src/` against a repo-wide control of 31,
and zero `only_used_in_recursion` anywhere in `crates/` against a control showing it 20 times in `docs/`.

**Task 1's rider is correct and genuinely byte-neutral.** `CF-113-7`'s stolen doc block is back above
`finalize_h2_stream`'s `#[allow(clippy::too_many_arguments)]`, the ten deleted and ten added lines are identical
as a sequence, and the md5 is `1b93f0ffd4a2a2daa03ed7b6d081590b` over **578 bytes** on both sides — a stated
byte count, so not the empty-file md5 trap. `ADR-0198` DECISION 1 correctly records that the step's predicted
numstat `14 14` measured `10 10` while its stated *invariant* (equal insertions and deletions) held, and that a
numstat is a rendering of a diff algorithm's choice rather than a property of an edit.

**No ordering defect across the ten task commits.** `git log -S grpc_status_code -- record.rs` over the arc
names only `537d95b`: the field was introduced together with its real derivation on both codecs in one commit.
No intermediate commit wrote a placeholder that a later commit fixed, and no test pinned one.

---

## §2 — Issues (Must Fix)

**EMPTY.**

No finding below is a correctness defect against a MEASURED upstream cell, none makes a fixture pass when it
should fail, none disturbs a §7.5 gate leg, and none is a broken build or a panic path. **§5.2 therefore does
not fire and the phase advances to state 6, not back to state 3.**

The closest call is **F-2**, and it is recorded here rather than silently placed in §3 because the reasoning
matters. F-2 predicts a real behavioural divergence from upstream on a present-but-unparseable or out-of-range
`grpc-status` response header. It is **not** a Must Fix for two reasons, both of which have to hold: the
divergence rests on an **unverified premise** about upstream internals (that upstream's filter and its
`%GRPC_STATUS%` formatter read the same status helper), which this project may not settle by reading Envoy
source (D-3.3); and the cell is **structurally unwitnessable** by the current fixture corpus, so no differential
evidence exists in either direction. What the review CAN say from disk is that the record's stated ground for
the choice — "unmeasured upstream" — is false, and that a cheap upstream-only measurement settles it. That is
banked as `CF-114-7` with the measurement named.

---

## §3 — Important

### F-1 — the new derivation helper HIJACKED `build_access_log_record`'s doc comment: this phase repeated, in the same arc, the exact regression its own Task 1 landed to repair

`crates/envoy-http1/src/hcm.rs:1656` (the orphaned docblock) / `:1666` (the thief) / `:1676` (the now-undocumented function)

`effective_grpc_status` was spliced **between** `build_access_log_record`'s doc comment and its `fn` line, with
no blank line separating them. The result on disk:

```
1656  /// Build the per-request access-log record (extracted verbatim from
1657  /// `serve_connection`'s factored access-log dispatch site, including the
1658  /// `%RESPONSE_FLAGS%` derive block).
1659  /// Phase 114: the UNGATED effective gRPC status. MEASURED on both proxies: the
…
1666  pub fn effective_grpc_status(response_headers: &[(String, String)], status: u16) -> u8 {
…
1676  fn build_access_log_record(
```

So a `u16 → u8` helper now carries, as the first paragraph of its own documentation, a provenance note
describing a different function, and `build_access_log_record` — the H1 record-build join point — has no doc
comment at all.

**Verified.** The anchor `Build the per-request access-log record` occurs exactly **once** in the file. At
`c9136ae`, `git show` puts it at lines 1655–1657 with `fn build_access_log_record(` immediately below at 1658.
The phase moved the function away from its documentation.

**Why it matters, and why this is a worse class than the drift findings around it.** It **destroyed information
that existed before the phase** — the same charge `ADR-0195` DECISION 2 laid against `CF-113-7`, whose remedy
was Task 1 of *this* phase. The mechanism is identical (a new helper spliced above an existing item's
doc/attribute block), the file class is identical (an HCM), and it is invisible to all three per-task gates:
`cargo fmt` will not reflow it, clippy does not lint it, and the build is unaffected. The general lesson
`ADR-0195` drew — *an anchor immediately above an attribute is inside the preceding doc block's scope* — needed
one more clause: **so is an anchor immediately above a `fn`.** Inserting a free function directly above an
existing item requires a blank line, and nothing in the tree enforces it.

**Banked as `CF-114-6`.** The remedy is a two-line move (insert a blank line after `:1658`, and relocate the
phase-114 paragraph below it). It is banked rather than Must-Fixed for the same proportionality reason
`ADR-0195` gave: it changes no behaviour, and sending the phase back through a full state-3/state-4 arc for a
comment relocation would be disproportionate. **The state-6 close-out, or the next rider touching
`crates/envoy-http1/src/hcm.rs`, should take it.**

### F-2 — `CF-114-5` is banked as "unmeasured upstream", but phase 113 MEASURED the same header parse in this very tree and got the opposite answer — so the landed fallback is a PREDICTED divergence, now pinned by a green test

`crates/envoy-http1/src/hcm.rs:1666-1674` (the fallback) / `:11885` (the test that pins it) / `BEHAVIOR_CONTRACT.md` §H / `crates/envoy-accesslog/src/command_operator.rs:135-144` (the contrary measurement)

The landed derivation treats a present-but-unparseable or out-of-range `grpc-status` header as **absent** and
falls through to the HTTP-code map:

```rust
access_log_header_value(response_headers, crate::headers::GRPC_STATUS)
    .and_then(|v| v.trim().parse::<i64>().ok())
    .filter(|n| (0..=16).contains(n))
{ Some(n) => n as u8, None => crate::grpc::http_to_grpc_status(status) }
```

`ADR-0197` opened `CF-114-5` on the ground that upstream's behaviour here "is unmeasured upstream and
unwitnessable by any fixture on either proxy."

**The first half of that ground is false.** Phase 113 measured the same wire value's parse against the same
pinned image and recorded it in-tree, twice:

- `crates/envoy-accesslog/src/command_operator.rs:135-138` — *"MEASURED: surrounding whitespace is tolerated …
  and anything unparseable (e.g. `5.0`, `notanumber`) renders the literal **`-1`** in EVERY format."*
- `crates/envoy-accesslog/src/command_operator.rs:142-144` — *"An out-of-enum numeric renders as the NUMBER in
  every format (MEASURED: **`99` renders `99`** under CAMEL_STRING, not `Unknown` and not an error)."*

Both are pinned by tests. Upstream, in other words, treats an unparseable value as a **value** (`-1`) and passes
an out-of-enum numeric **through unchanged** — it does not fall back to anything. If upstream's
`GrpcStatusFilter` reads the same status helper its `%GRPC_STATUS%` formatter reads, then on a sink filtering
`statuses: [UNIMPLEMENTED]` against a 404 response:

| response `grpc-status` | upstream effective | envoy-rust effective | verdict |
|---|---|---|---|
| `abc` | −1 | **12** (map) | upstream DROPS, envoy-rust **KEEPS** |
| `99` | 99 | **12** (map) | upstream DROPS, envoy-rust **KEEPS** |
| `-1` | −1 | **12** (map) | upstream DROPS, envoy-rust **KEEPS** |
| absent | 12 | 12 | agree |

**Confidence and its limit.** The in-tree measurement is certain; the shared-helper premise is not, and this
project may not settle it by reading Envoy source (D-3.3). That is exactly why this is banked rather than
Must-Fixed (§2). But the premise is cheap to bypass: **the pick session already ran the measurement technique
that settles it** — six sinks on one upstream listener, routes producing chosen `grpc-status` values, logs read
via `docker logs`. Adding two routes that set `grpc-status: abc` and `grpc-status: 99` on the **upstream side
only** answers it in one run, with no envoy-rust involvement and no fixture.

**The compounding half.** `derivation_ignores_an_unparseable_or_out_of_range_header`
(`crates/envoy-http1/src/hcm.rs:11885`) pins the fallback for exactly `["", "abc", "99", "-1", "1.0"]`. So the
choice is now a **green test**, and a future phase that discovers the divergence must delete a passing
assertion to fix it. That is the correct shape for a deliberate characterization pin — but only if the record
says it is characterizing a CHOICE. `BEHAVIOR_CONTRACT.md` §H presents it as an unmeasurable gap instead.

**A structural note that rides along.** `AccessLogRecord.grpc_status_code` is a `u8`
(`crates/envoy-accesslog/src/record.rs:133`), which cannot represent `-1` or a user-defined code above 16.
`SPEC.md` §5 non-goal 1 requires that adding the trailer source later be "an added branch, not a reshape";
combined with this, adopting upstream's `-1`/pass-through semantics would be a type change across `record.rs`,
`filter.rs`, `file_sink.rs` and both codecs.

**Banked as `CF-114-7`**, which AMENDS `CF-114-5` rather than replacing it.

### F-3 — the H2 arm's declared "sole witness" witnesses nothing about H2, and phase 114 is the first filter-arm phase to skip the end-to-end H2 test its three predecessors all shipped

`crates/envoy-http2/src/hcm.rs:7773-7793` (the claimed pin) / `:1202` (the unwitnessed production wiring) / `:1222` (the unwitnessed sink gate)

`h2_grpc_status_code_tests::h2_uses_the_shared_effective_status` documents itself as *"the H2 arm has NO
differential witness (`CF-114-3`), so this in-process test is its sole witness and a phase that changes H2's
behaviour must edit it."* Its body calls `envoy_http1::hcm::effective_grpc_status` **directly**, with
hand-built arguments. It never constructs an `HCMConfig`, never constructs an `AccessLogRecord`, never drives a
stream, never reaches `finalize_h2_stream`, and never touches a filtered sink.

**It would pass against a stub.** Replace `crates/envoy-http2/src/hcm.rs:1202-1205` with
`grpc_status_code: 2`, or replace the `record.grpc_status_code` argument at `:1222` with a literal — the test
is untouched and stays green. There is no call edge from the test to the code it claims to pin.

**It is also a duplicate.** Its two assertions are `effective_grpc_status(&[], 404) == 12` and the header leg
with `("grpc-status", "3")`. `crates/envoy-http1/src/hcm.rs:11881` already asserts the first verbatim, and
`:11847` the second in the same shape. The test is an `envoy-http1` unit filed in the `envoy-http2` crate.

**Verified, with the control that makes it a finding rather than an observation.** A grep for assertions
reading `record.grpc_status_code` anywhere in `crates/` returns **zero**; the identical grep shape for phase
113's `record.grpc_status` returns two, at `crates/envoy-accesslog/src/record.rs:265` and `:271`. So phase
113's field is asserted on a built record and phase 114's is not — on either codec.

**The precedent this breaks, and the harness that was already there.** Every prior filter-arm phase landed an
H2 end-to-end filtered-sink test that drives a real stream: `h2_filtered_sink_suppresses_below_threshold`
(`:3785`, phase 70), `h2_response_flag_filter_suppresses_no_flag` (`:3916`, phase 71),
`h2_header_filter_keeps_match_drops_mismatch_and_absent` (`:4032`, phase 72),
`h2_metadata_filter_gate_reads_the_threaded_dynamic_metadata` (`:4160`, phase 74). There is no
`h2_grpc_status_filter_*` row. The helpers those four use — `h2_hcm_config_with_access_log` (`:3282`) and
`serve_one_h2_request_with_access_log` (`:3343`) — already exist, so the remedy is one test, not a harness.

**Why it matters.** `CF-114-3`'s whole point was that H2 behaviour would be pinned in-process *"so that a later
phase lifting the boundary has to delete an explicit assertion rather than silently change behaviour"*
(`SPEC.md` §5 non-goal 3). No such assertion exists. The H2 record-build wiring and the H2 sink gate are the
only filter-arm wiring in the tree with zero coverage of any kind, and `PROGRESS.md` records the vacuous test
as discharging the obligation.

⚠ **The derivation itself is fine.** H2 clones `resp.headers` at `:1107`, one line before `resp` is moved, and
the borrow is live at the record build — PV-4's claim re-verified here, and independently corroborated by the
pre-existing `extract_upstream_service_time(response_headers_for_log)` at `:1182`. **H2 genuinely gets both
legs.** The finding is about the witness, not the behaviour.

**Banked as `CF-114-8`**, which AMENDS `CF-114-3`.

### F-4 — twelve of the seventeen canonical name→code bindings are asserted NOWHERE, and the accept sweep is a byte-copy of the table it tests

`crates/envoy-config/src/bootstrap.rs:790-808` (the positional table) / `:20470-20523` (the sweep)

`GRPC_STATUS_FILTER_NAMES` is positional — `bootstrap.rs:788` states *"Index IS the code"* and
`resolve_grpc_status_token` returns `.position(...).map(|i| i as u8)`. `grpc_status_filter_accepts_every_measured_token`
iterates 28 spellings and asserts only `.is_some()` on each. That is an **existence** assertion: it stays green
against any permutation of the table. Its seventeen canonical literals are the same sequence as the table's,
extracted and compared here — **byte-identical**. Producer and assertion share the predicate.

Only **five** name→code bindings are pinned anywhere in the tree: `CANCELED → 1` and `unauthenticated → 16`
(`:20509`, `:20513`), `NOT_FOUND → 5` (`:20577`), `UNIMPLEMENTED → 12`
(`crates/envoy-http1/src/hcm.rs:11910`), and `INTERNAL → 13` behaviourally through fixture `0094`. The other
twelve — `OK`(0), `UNKNOWN`(2), `INVALID_ARGUMENT`(3), `DEADLINE_EXCEEDED`(4), `ALREADY_EXISTS`(6),
`PERMISSION_DENIED`(7), `RESOURCE_EXHAUSTED`(8), `FAILED_PRECONDITION`(9), `ABORTED`(10), `OUT_OF_RANGE`(11),
`UNAVAILABLE`(14), `DATA_LOSS`(15) — have **no code assertion at all**. Transpose any two of them and the whole
workspace stays green while `statuses: [PERMISSION_DENIED]` silently gates on code 8.

**The control that makes this a finding rather than a general complaint.** The parallel phase-113 table IS swept
index-by-index against an **independent** seventeen-row literal:
`grpc_status_canonical_table_all_seventeen_codes` at `crates/envoy-accesslog/src/command_operator.rs:1222`. The
sibling got the strong test; this one did not — in the phase whose own doc at `bootstrap.rs:783-789` warns that
this table is *deliberately different at index 1* from that sibling, which is exactly the situation where a
transcription slip is likeliest and least visible.

This is `ADR-0195` DECISION 4 recurring one phase later, on the adjacent table. **Banked as `CF-114-9`**; the
remedy is a `[(name, code); 17]` literal written from the SPEC transcript rather than from the implementation.

### F-5 — the new arm is never composed under `and_filter`/`or_filter` at ANY of the five stages, in the phase that widened that very recursion

`crates/envoy-accesslog/src/filter.rs:160` and `:169` (the threading) / `crates/envoy-config/src/bootstrap.rs:5875`,`:5885` / `crates/envoy-http1/src/hcm.rs:1886`,`:1889`

The only reason the fifth parameter had to be threaded through the `And`/`Or` recursion is so a `GrpcStatus`
leaf can sit under a composition. That composition has **no test anywhere** — not in config deserialization, not
in validation, not in `compile_access_log_filter`, not in the predicate, and not in a fixture. Every one of the
four `LogFilter::GrpcStatus { .. }` literals in test code (`filter.rs:524`, `:539`, `:543`, `:560`) is a bare
top-level filter, and every `grpc_status_filter:` in a config test is a top-level single arm.

**Verified, with controls at each stage.** A grep for `GrpcStatus` inside an `And`/`Or` construction returns
**0**; the identical shape returns **5** for the phase-74 `Metadata` arm. Phase 74 landed both a predicate-level
composition test (`filter.rs:490`) and a config-level one
(`metadata_filter_nested_in_or_filter_surfaces_through_recursion`, `bootstrap.rs:15145`). Phase 73 shipped
fixtures that nest arms (`tests/fixtures/0080-accesslog-or-filter/` nests an `and_filter` inside an `or_filter`).

**It works.** Traced end to end here: `AndFilter.filters: Vec<AccessLogFilter>` binds a nested seventh arm at
any depth; the validator recurses **before** the seventh-arm token check sits in the same function body, so a
bad nested token IS caught at boot as `UnknownGrpcStatus`; `set_arms` is evaluated per recursion level so
cardinality still holds; the compile step recurses; the predicate threads the parameter verbatim. **The finding
is that the phase changed the composition code and left the composition untested**, breaking a pattern its two
composition-touching predecessors both set. **Banked as `CF-114-10`.**

### F-6 — `codes: []` with `exclude: true` is a config-reachable keep-everything sink with no test, and it is the one clause of its own doc not labelled MEASURED

`crates/envoy-accesslog/src/filter.rs:102` (the claim) / `:557` (the test that covers the other half)

The variant doc reads: *"An EMPTY `codes` with `exclude: false` keeps NOTHING (MEASURED); with `exclude: true`
it keeps EVERYTHING."* The `(MEASURED)` label attaches to the first clause only. The second — an unconditional
keep-all — is asserted flatly and has no test.

**It is reachable.** `GrpcStatusFilter` derives `Default` and carries `#[serde(default, deny_unknown_fields)]`
(`bootstrap.rs:764-766`), so `grpc_status_filter: { exclude: true }` deserializes to `statuses: []`,
`exclude: true`. The validator's token loop iterates zero times and returns `Ok`
(`grpc_status_filter_empty_statuses_loads`, `:14988`, pins that an empty list loads). The compile step maps
`statuses` through `.iter().map(...).collect()`, yielding `codes: vec![]`, and passes `exclude` through. A user
can configure a sink that logs unconditionally through the *filter* path, and nothing in the repo asserts that
it does.

The behaviour is right — `false != true` is `true` for every code, one expression at `filter.rs:194`. The
finding is that the phase's `exclude` coverage is `codes: vec![12]` and nothing else: the only `exclude: true`
in this crate is at `filter.rs:545`. **Banked as `CF-114-11`**, which AMENDS `CF-114-4`.

### F-7 — the Task-2 re-export is dead public API: Task 5 deleted its only consumer three commits later, and `lib.rs`'s encapsulation comment is now false

`crates/envoy-http1/src/lib.rs:34` (the dead `pub use`) / `:17-19` (the now-false claim) / `crates/envoy-http1/src/grpc.rs:69` (the widened item)

`pub use grpc::http_to_grpc_status; // 114: the ONE HTTP->gRPC map, shared with envoy-http2.` has **zero
consumers anywhere in the workspace**. `envoy-http2` reaches the shared map through
`envoy_http1::hcm::effective_grpc_status`, not through this symbol. Repo-wide grep: the five `envoy-http1`
hits are all crate-internal, the `lib.rs:34` hit is the re-export itself, and the rest are doc mentions — no
`envoy-http2` hit. Positive control: the identical grep for `effective_grpc_status` returns three
`crates/envoy-http2/src/hcm.rs` hits.

**The per-commit walk shows exactly how it happened, and it is invisible in the squashed range.** `9a74ac5`
(Task 2) added the `pub use` **and** its only cross-crate consumer, a test asserting
`envoy_http1::http_to_grpc_status(404) == 12`. `537d95b` (Task 5) rewrote that test's body to call
`envoy_http1::hcm::effective_grpc_status`, orphaning the re-export.

**The stale claim.** `lib.rs:17-19` still reads *"DELIBERATELY `pub(crate)` — see the module doc. **Nothing
outside this crate may reach it**, because `envoy-http2` shares this crate's `build_response` and must stay
untransformed (`CF-110-1`)."* One item of `grpc` now is reachable from outside the crate, at line 34, twelve
lines below. `grpc.rs`'s own module doc is **not** contradicted — it says the *module* is `pub(crate)`, which
remains literally true, and `grpc.rs:64-68` reconciles the item-level widening with it. The `lib.rs` copy of
that rationale was not reconciled.

**Nothing will catch it.** No `[lints]` section exists in any manifest (control: 27 manifests declare
`[dependencies]`), `unreachable_pub` appears nowhere in `crates/`, and CI runs plain `-D warnings`.

**Why it matters.** PV-3 asked whether the widening leaked. It did not leak *broadly* — the enclosing module
stayed `pub(crate)` and that was the right call — but it leaked *unnecessarily*, and the file documenting the
module's deliberate encapsulation now carries a false statement about it. **Banked as `CF-114-12`.**

### F-8 — on the entire tested surface the header leg's PRODUCER shares the predicate with the fallback leg, so a bug swapping the two is unwitnessable by fixture `0094`

`crates/envoy-http1/src/grpc.rs:167` (the producer) / `crates/envoy-http1/src/grpc.rs:69` (the map both use) / `crates/envoy-http1/src/hcm.rs:1666-1674` (the consumer)

The derivation has two legs: the response `grpc-status` header, else `http_to_grpc_status(response_code)`. In
fixture `0094`, the **only** thing that produces a `grpc-status` response header is the phase-110 local-reply
transform, `apply_grpc_local_reply`, which emits `http_to_grpc_status(resp.status)` — **the same function the
fallback leg calls**. So across the whole fixture the two legs are guaranteed by construction to agree, and an
implementation that read the wrong leg would produce identical output.

Probes 1 and 2 are the fixture's stated header-leg witnesses. They discriminate one specific wrong
implementation — deriving from the **logged** response code, which the transform rewrote to 200 — because the
logged code and the header disagree there. They do **not** discriminate a leg swap, because the header's value
*is* the map's value.

**The shape that would separate them is a proxied response carrying a backend-supplied `grpc-status`.**
`build_access_log_record` reads `response.headers`, bound to `&outgoing.headers`
(`crates/envoy-http1/src/hcm.rs:1558`), which on the proxy path is the backend's forwarded header set. A backend
returning `grpc-status: 5` drives the header leg with a value the map can never produce — the map's image is
`{2, 7, 12, 13, 14, 16}`, so `{0, 1, 3, 4, 5, 6, 8, 9, 10, 11, 15}` are reachable **only** through the header.
Fixture `0094` is `clusters: []` and backend-free by design (so it runs on the development host), so it cannot
express that shape. No test and no fixture does.

⚠ This is the same structural blindness fixture `0093`'s own README documents for `%GRPC_STATUS%`. The unit test
at `crates/envoy-http1/src/hcm.rs:11847` partly escapes it by asserting the header literal `"0"` on a 200
response, where the map would give 2 — so the leg separation IS pinned in-process, for one value. The finding is
that it has **no cross-proxy witness at all**, and that the fixture's README does not say so.

**Banked as `CF-114-13`.**

### F-9 — a landed `docs/` numstat is wrong in BOTH figures, and it is wrong SELF-REFERENTIALLY

`docs/envoy-rust/phases/114-accesslog-grpc-status-filter/PROGRESS.md:1350` — *"The `docs/` slice, excluded
from the gate, is **+1424 / −0** (`PROGRESS.md`, `ADR-0198`, the `BEHAVIOR_CONTRACT.md` section, `STATE.md`,
`STATE_HISTORY.md`)."*

Measured here over exactly that enumerated file set, at the state-3 advance that carries the sentence:

```
git diff --numstat c9136ae..ad19e9a -- docs/
  135    0   BEHAVIOR_CONTRACT.md
  197    0   DECISIONS.md
   28   17   STATE.md
   28    0   STATE_HISTORY.md
 1455    0   114-…/PROGRESS.md
  SUM  1843 / 17
```

**Both figures are wrong.** `+1424` is short of `+1843` by 419 — consistent with the count having been taken
mid-write, before the remaining lines of `PROGRESS.md` were appended. **That is the self-referential failure
this project's own ledger warns about**: `PROGRESS.md` is in the set it is counting, so writing the sentence
changes the answer. And `−0` is false **by the sentence's own enumeration** — it names `STATE.md`, which
deletes 17 lines at that very commit.

**Why it matters.** It is a landed, measured-looking figure in the document a close-out reads to size its own
work, and it is the one figure in the phase's record that a later session is most likely to inherit rather than
re-derive. ⚠ The **gate-relevant** number is unaffected: the `docs/` slice is excluded from §6.1, and the
non-`docs/` net of 1038 reproduces exactly. **Banked as `CF-114-17`;** the correction lands in `ADR-0199`, not
by editing `PROGRESS.md`.

### F-10 — this phase broke a citation in a LIVE document — one it edited itself — and recorded nothing

`docs/envoy-rust/BEHAVIOR_CONTRACT.md:846` cites *"the **idempotence sentinel** at
`crates/envoy-http1/src/grpc.rs:158-160`, which returns early on any pre-existing `grpc-status`"*.

- At `c9136ae`, `grpc.rs:158-160` is exactly that sentinel:
  `if headers::find_header(&resp.headers, headers::GRPC_STATUS).is_some() { return; }`.
- On disk now, `grpc.rs:158-160` is **three lines of comment prose about that same sentinel**, and the sentinel
  itself has moved to `:163`. Task 2 inserted a five-line comment block above `http_to_grpc_status` at `:64`
  and shifted everything below it.

The anchor `if headers::find_header(&resp.headers, headers::GRPC_STATUS).is_some() {` occurs **exactly once**
in the file, so the relocation is unambiguous.

**Why this is the audit's most consequential citation finding.** `SPEC.md` §8 PV-1 warned that *"any phase that
edits `hcm.rs` moves them"* and that *"a stale citation can land on a plausible wrong target"*. Both halves came
true, in `grpc.rs` rather than `hcm.rs`, and the landing is maximally plausible — the wrong lines *discuss the
right thing*. The distinguishing fact is that `BEHAVIOR_CONTRACT.md` is a **live, canonical, non-archival
document**, unlike `STATE_HISTORY.md` and `DECISIONS.md` where staleness is inherent and expected — and **phase
114 edited that very file at Task 10** without censusing what it had moved for anyone else. **Banked as
`CF-114-18`.**

### F-11 — the record's two enumerations of "nine `PLAN.md` corrections" are two DIFFERENT nines, and one of them claims the other contains it

`PROGRESS.md:1394-1409` (the §5 table) against `DECISIONS.md` `ADR-0198` DECISION 1

Three defects, compounding:

1. **`PROGRESS.md:1396` says *"all are recorded in `ADR-0198` DECISION 1."* Its item 4 is not.** That item —
   *"`E0027` forces the validator destructure at Task 3, not Task 4"* — appears **0** times in DECISION 1 and
   **1** time in DECISION 2, where it is filed as a boundary-gate deviation rather than a plan correction.
2. **ADR-0198's item 9 appears nowhere in `PROGRESS.md`'s nine.** It is *"three doc statements beyond the one
   the plan names became FALSE"*, which points at DECISION 3. So the two sets of nine overlap in eight items and
   their **union is ten**.
3. **ADR-0198 says *"In the order they were hit"* and is not in that order** — its sequence runs T1, T2, T3,
   T5, T6, **T9, T4**, T8. Task 9's fixture README was not written before Task 4's anchor was used.

⚠ A fourth, smaller point: ADR-0198's item 9 is not a *correction of a `PLAN.md` claim* at all. DECISION 3 says
the plan *"names only"* one of the three stale doc statements — an **omission**, not a wrong claim.

**Why it matters.** These two lists are exactly what a state-5 review and a state-6 close-out grade the
implementation against. A reader reconciling "nine" against "nine" finds neither list complete and no statement
that they differ. **Banked as `CF-114-19`.**

### F-12 — the `PLAN.md` prototype did NOT carry Task 1, contradicting the plan's own method claims twice

`PLAN.md:80` (the measured cell) against `PLAN.md:66` and `:1565` (the method claims)

`PLAN.md` §6.1 states *"Every code block below was inserted into a scratch `git worktree` …"* and its
Self-review states *"**Every code block in this plan was EXECUTED, not written from reasoning.**"* The
measured table's cell for the H2 codec is `| crates/envoy-http2/src/hcm.rs | 32 | 0 | 32 |`. Measured here:

| range | numstat |
|---|---|
| `c9136ae..2cf0830` — the rider ALONE | `10 10` |
| `2cf0830..762d3cc` — the arc MINUS the rider | **`32 0`** |
| `c9136ae..762d3cc` — what landed | `42 10` |

The prototype's cell is **byte-for-byte the arc excluding Task 1**. Corroborating: Task 1 Step 3 predicts a
numstat of `14 14`; the real value is `10 10`, which a prototype run would simply have printed.

**Why it matters, and what it explains.** The *net* is unaffected — the rider is net-zero, so the measured 938
stands and the §6.1 adjudication is untouched. But two method claims are over-broad, and the miss has a
knock-on: `ADR-0198` correction 1 explains the `14 14` → `10 10` gap **entirely** as *"git found a more compact
minimal edit"*, without noticing the simpler structural explanation available to it (the step was never run).
And `ADR-0198` DECISION 4's *"Same **14 files** as the prototype … both HCM files (150/32)"* is true only in
**net**: the H2 file's insertion/deletion split is `42/10` against the prototype's `32/0`, and the difference is
precisely the unprototyped rider. **Banked as `CF-114-20`.**

### F-13 — the corrected "exactly TWO" survives UNCORRECTED in two more landed places, one of them the phase's own ROADMAP row, and the correction mis-states its location

`SPEC.md:211`, `ROADMAP.md:196`; charge at `ADR-0197` DECISION 3 and `PLAN.md:31`

`ADR-0197` correction 1 charges *"`SPEC.md` **§1** and §4 item 6's 'Production `should_log` call sites are
exactly TWO'"*. Three things are off:

- **The §1 half is wrong.** `SPEC.md` §1 spans lines 13–51 and contains **zero** occurrences of `127`, `125`,
  `TWO` or `two`. The claim lives only at `SPEC.md:147` (§4 item 6).
- **`SPEC.md:211` carries the same wrong figure and no correction names it** — the §7 size-estimate row reads
  *"the behaviour-neutral fifth-parameter `should_log` sweep (**2 production sites, ~125 test sites**)"*.
- ⚠ **`ROADMAP.md:196` carries it too.** The landed row `114` says the phase lands *"a fifth `should_log`
  parameter (production call sites: **exactly TWO**)"*. **No phase-114 document mentions that the ROADMAP row
  carries the wrong figure.** ⚠ This review does **not** touch `ROADMAP.md`; the row is append-only history and
  the state-6 close-out flips its status cell only.

**The correct figure, re-derived here** by a column-0 `#[cfg(test)]` boundary walk rather than a name
heuristic: **138** total `.should_log(` occurrences in `crates/`, of which **five** are production
(`file_sink.rs:115`, `filter.rs:160`, `filter.rs:169`, `envoy-http1/src/hcm.rs:1575`,
`envoy-http2/src/hcm.rs:1217`). Pre-arc the total was **127**, so `SPEC.md`'s **127 is right and its 2 + 125
split is wrong** — which is exactly what `ADR-0197` says, in the one place it says it. **Banked as
`CF-114-21`.**

---

## §4 — Minor

### N-1 — `record.rs` says "20 fields total" twice, and THIS diff is what made it wrong

`crates/envoy-accesslog/src/record.rs:5` (module doc) and `:14` (struct doc). Both read `20 fields total`.
The struct now has **21**. Measured: `git show c9136ae:` → 20 `pub` fields; on disk → 21. Both anchor strings
occur exactly once. The enumeration *inside* those docs is still accurate — `grpc_status_code` is a filter
input, not a command-operator target — so the right correction is a new category, not a bumped count. The clean
case: correct at `c9136ae`, made wrong by this phase, in a file the phase edited.

### N-2 — the behaviour-neutrality pin covers 2 of the 6 pre-existing arms while its comment claims all of them

`crates/envoy-accesslog/src/filter.rs:510`. `existing_arms_ignore_the_grpc_status_argument` says *"the phase-74
T3 behaviour-neutrality pin, repeated for the fifth parameter: **every** pre-phase-114 arm must be blind to
it"*, then varies `code` over `[0, 2, 12, 16]` against `ge(500)` and `rf(&["NR"])` only. The precedent it names,
`existing_arms_ignore_the_dynamic_metadata_argument` (`filter.rs:362`), covers `StatusCode`, `ResponseFlag`,
`Header`, `And` **and** `Or` — five arms. Neutrality for `Header`/`Metadata` is trivially true by inspection
(they do not name the parameter), but `And`/`Or` are the two arms that DO touch it and they are the two the
narrower pin drops. Overlaps F-5: an `And`/`Or` neutrality probe over varying codes was the cheap half of the
missing composition coverage.

### N-3 — `exclude: true` has no ABSOLUTE assertion, and codes 0 and 16 are never members of `codes`

`crates/envoy-accesslog/src/filter.rs:535`. `grpc_status_arm_exclude_inverts_over_the_same_code`'s only
assertion is `assert_ne!(inc.should_log(…), exc.should_log(…))` over `0..=16`. That is a **relative** claim,
satisfied by any predicate of the form `f(codes, code) != exclude` — including the polarity-inverted
`!codes.contains(&c) != *exclude`. The suite as a whole is sound because
`grpc_status_arm_is_membership_over_the_effective_code` (`:523`) pins absolute values for `exclude: false` and
the inversion propagates. But the measured upstream fact the test's own comment cites — sink DDD kept every
probe whose status was NOT 12 and dropped both whose status WAS 12 — is a statement of absolute verdicts and is
never directly asserted, so a reader auditing `exclude` in isolation gets no answer. Related: the three `codes`
literals in this crate are `[12, 13]`, `[12]` and `[]`, so codes **0** and **16** — the two boundary values —
are only ever tested on the non-matching side.

### N-4 — `FileSink::should_log` never delegates a `GrpcStatus` filter in any test

`crates/envoy-accesslog/src/file_sink.rs:103-124`. The delegation is one of the five production `should_log`
sites and the only one in this crate that can return `true` without consulting a filter. Its test uses a
`StatusCode` filter and passes the constant `2` for `grpc_status_code` in all four calls. Low risk — the
delegation is a two-arm `match` forwarding positionally, so a mis-wire is a compile error — but combined with
F-5 it means the `GrpcStatus` arm is only ever exercised by calling `LogFilter::should_log` directly, never
through either production entry point in the crate.

### N-5 — `file_sink.rs`'s exhaustive record literal falsifies `record.rs`'s "ONE literal" promise, and this diff is the proof

`crates/envoy-accesslog/src/file_sink.rs:176` vs `crates/envoy-accesslog/src/record.rs:138`. The latter
documents `test_baseline()` as *"so every in-crate test module can build the canonical baseline record from ONE
literal — adding a record field forces exactly ONE test-side update."* This phase had to update **two** literals
in this crate. `file_sink.rs:176-200` is a full 21-field exhaustive literal differing from `test_baseline()` in
one field, where `default_format.rs:206` handles the identical delta by mutating a copy. Every other builder in
the crate spreads from the shared fixture. Pre-existing; this phase is the event that demonstrates it. The
`E0063`-on-every-literal property means it cannot silently rot, but the doc's promise is false.

### N-6 — `filter.rs`'s module doc stops at phase 72, four phases stale, and the diff updated two sibling docblocks in the same file without touching it

`crates/envoy-accesslog/src/filter.rs:1-4` — *"phase 72 **adds** `header_filter`"*, present tense, implying 72
is the head. Phases 73, 74 and 114 are absent. Pre-existing staleness, but the diff updated both `should_log`
doc blocks to the full `70/71/72/73/74/114` roll-call (`filter.rs:113`, `file_sink.rs:99`) and left the module
header inconsistent with them inside the same file.

### N-7 — a BARE `0x5` resolves to code 5; the record only ever spells the quoted `"0x5"`, and no test covers the bare form

`crates/envoy-config/src/bootstrap.rs:774-779` (the untagged enum). Traced through the `Cargo.lock`-pinned
registry source: `serde_yaml-0.9.34+deprecated/src/de.rs:1118` routes a plain scalar through `visit_int` before
falling back to a string, and `de.rs:940 parse_unsigned_int` explicitly strips `0x` (→ radix 16), `0o` (→ 8) and
`0b` (→ 2). So `statuses: [0x5]` binds `Num(5)` and resolves to code **5 (NOT_FOUND)**. Every place the record
addresses this token — `BEHAVIOR_CONTRACT.md` §A, `ADR-0197` DECISION 6, `PLAN.md`, and the reject sweep at
`bootstrap.rs:20532` — spells it **quoted**, `"0x5"`, and the quoted form does reject correctly (string →
`trim().parse::<i64>()` fails → no canonical name → `UnknownGrpcStatus`). The bare form is a class the
measurement set never addressed and no test reaches: `grep` for a `statuses:` list containing `0x` returns
**0** against a control of **6** `statuses: [` occurrences repo-wide. Whether it is also a *divergence* is
unknown — upstream's yaml-cpp path was not measured for it.

### N-8 — a bare leading-zero integer resolves to DECIMAL here and would be OCTAL upstream — an unmeasured silent wrong-code hypothesis

Same site. `serde_yaml`'s `digits_but_not_number` (`de.rs:1092`) makes `parse_unsigned_int` return `None` for
`010`, so it falls to `Name("010")`, where `trim().parse::<i64>()` yields **10 (ABORTED)**. Envoy parses YAML
**1.1**, where a leading zero denotes octal, which would give **8 (RESOURCE_EXHAUSTED)**. If upstream accepts
the token at all, that is a divergence with **no error on either side** — just a different gRPC code gating the
sink. ⚠ **Labelled a hypothesis, not a finding**: the envoy-rust half is source-traced and certain; the upstream
half is not measured anywhere in this project and was not measurable here. It is banked together with N-7
because one upstream `--mode validate` run settles both.

### N-9 — `grpc_status_filter` is the ONLY one of the seven `AccessLogFilter` arms with zero fuzz-corpus reach

`crates/envoy-config/fuzz/`. Per-arm census over the corpus, with every sibling as its own control:

| arm | corpus files mentioning it | tracked seeds |
|---|---:|---:|
| `status_code_filter` | 32 | 1 |
| `header_filter` | 22 | 2 |
| `and_filter` | 19 | 1 |
| `or_filter` | 18 | 1 |
| `metadata_filter` | 10 | 1 |
| `response_flag_filter` | 3 | 1 |
| **`grpc_status_filter`** | **0** | **0** |

`GrpcStatusToken` is an `#[serde(untagged)]` enum whose arm order is documented as load-bearing, and
`resolve_grpc_status_token` does an `as u8` narrowing after a range check. Neither is reachable from any
committed seed, so the fuzzer would have to invent the `grpc_status_filter:` key from a corpus that never
contains it. §7.5 leg (d) is correctly reported PASS — the phase added no new target, and all five pre-existing
targets ran clean — but for this arm the fuzzer has no entry point. No defect is predicted; the cast is guarded
at `bootstrap.rs:817`.

### N-10 — the `Num` arm of the `UnknownGrpcStatus` token formatter is never exercised

`crates/envoy-config/src/bootstrap.rs:5906`. `GrpcStatusToken::Num(n) => n.to_string()` is reachable only from
a config such as `statuses: [17]` or `statuses: [-1]`. No test drives an out-of-range **bare integer** through
`parse_bootstrap`; the reject sweep only checks `resolve(Num(-1|17|99)).is_none()`, which never constructs the
error. So the SPEC-measured reject rows `17` and `-1` have no end-to-end witness and the numeric error message
shape is unasserted.

### N-11 — `deny_unknown_fields` on the new leaf is untested, and so is the nested bad-token catch

`crates/envoy-config/src/bootstrap.rs:765`, `:5875`. Nothing asserts that `grpc_status_filter: { bogus: 1 }`
rejects; a grep for `grpc_status_filter` co-occurring with an unknown-field assertion returns **0** against a
control of 48 `grpc_status_filter` hits in `.rs`. Nothing asserts that a bad token inside
`and_filter.filters[i].grpc_status_filter` is caught by the recursion. Both are correct by construction — the
attribute is present and the recursion was read here — but neither is pinned.

### N-12 — the reject direction crosses the serde boundary for exactly ONE token

`crates/envoy-config/src/bootstrap.rs:20525` vs `:14976`. `grpc_status_filter_rejects_every_measured_reject`
hand-constructs `GrpcStatusToken::Name(…)` for all nine reject tokens and asserts the **resolver** returns
`None`. But `BEHAVIOR_CONTRACT.md` §A's claims about `TRUE`, `1.0`, `0x5`, `17` and `-1` are claims about
**YAML**: `TRUE` booleanizes to `Content::Bool`, `1.0` to `Content::F64`, `0x5` to `Content::U64(5)` (N-7),
and `17`/`-1` bind to `Num`. Only `CANCELLED` (`:14976`) is driven through `parse_bootstrap`. For the
bool/float classes the untagged enum matches nothing and the failure surfaces as `ConfigError::Yaml` carrying
serde's fixed *"data did not match any variant of untagged enum GrpcStatusToken"*, **not** the purpose-built
`ConfigError::UnknownGrpcStatus` that `BEHAVIOR_CONTRACT.md:3423` advertises for "an entry that is neither".
The verdict (fail-loud) is preserved either way; the error class and the contract sentence are not.

### N-13 — the fixture README's universal claim about the four harness hunks is false

`tests/fixtures/0094-accesslog-grpc-status-filter/README.md:130-132` — *"exactly **four harness hunks** … the
same four every landed access-log byte-exact fixture carries (`0076`, `0078`, `0081`, `0093`)."* The four named
exemplars do carry four. The universal does not hold. Census over all 32 `http1_access_log_byte_exact` fixtures:
**22 carry 4**, six carry 3 (`0053`, `0057`–`0061`), two carry 5 (`0079`, `0080`), two carry 6 (`0051`,
`0052`). The load-bearing claim — that *this* fixture's four divergences are non-semantic — is true and is
independently checkable from the diff quoted immediately below the sentence, which is why this is Minor.

### N-14 — the README's stated reason for the two log paths needing different parents is not what the driver does

`README.md:144-145` — *"because the driver bind-mounts only the upstream side's parent."* The driver creates
**both** parents and chmods **both** to `0o777` (`tests/differential/src/lib.rs:4204-4216`), removes leftovers
at both file paths (`:4218-4227`), then mounts only the upstream parent (`:4229-4232`). Nothing in that sequence
requires the two *parents* to differ — only the two *file paths*. The real invariant worth writing down is
`envoy != envoy_rust` as full paths. ⚠ The review could not exclude some **other** reason different parents are
required (this host's virtiofs bind-mount caching is a standing hazard); the README simply does not offer one.

### N-15 — fixture `0094`'s suppression controls can only show "∉ {12, 13}", so the phase's headline map values are not witnessed there

`expectations.yaml:36-37`, `:45-46`, `:69`; `README.md:47-50`. Probes 3, 4 and 7 are suppressed, and the only
observable the byte-exact driver has for a suppressed probe is the per-side line count. So their derived values
are observable only as non-membership. Mutating the map's `_ => 2` arm to `_ => 0` (or `5`, or `99`) leaves
probes 3 and 7 suppressed, leaves the five kept lines byte-identical and leaves `0094` **green**; the same holds
for probe 4 and the `429|502|503|504 => 14` arm. The comments name the specific values as though the fixture
demonstrates them. ⚠ **The map is not unguarded repo-wide** — fixture `0093` renders `NUM=%GRPC_STATUS_NUMBER%`
and pins `200 → 2` cross-proxy positively — which is why this is a documentation over-claim rather than a
coverage hole.

### N-16 — the `prefix: "/"` catch-all returns 404, the same status three probes use, so a broken route entry stays green

`tests/fixtures/0094-…/envoy-rust.yaml:83-86` (status 404) against the routes at `:53`, `:69` and `:81`. If any
of `/g-unimpl`, `/p-unimpl` or `/g-param` had its `match:` block deleted or its path mistyped, the probe would
fall through to the catch-all, produce the same 404, the same derived 12, the same kept line and the same
rendered `PATH=`, and the fixture would stay green. Only probes 2/3/4/6/7 use a status the catch-all cannot
supply. Two of the three masked probes are exactly the ones the README calls the fixture's non-vacuity witnesses
— their *filter* witness is unaffected, but the fixture's own route table is not being shown to be consulted for
them. Cheap remedy for a later phase: give the catch-all a status no probe route uses.

### N-17 — the README's "verified by md5 over the block" is an unreproducible method claim

`README.md:143`. The **substance is true** — `diff` of the pair shows no hunk touching the `filter:` block on
either side. The method is not reproducible: an md5 over a hand-chosen span is span-dependent and the span is
not stated, so a reader cannot re-run it. `diff` proves the property outright and would be the citable form.
This is the same class the state-4 gate recorded against itself when an md5 over a hand-chosen span came back
as the empty-file md5 on 0 bytes.

### N-18 — the fixture README's "what this fixture does not witness" list omits two banked carry-forwards

`README.md:59-65` banks `exclude: true` and empty-`statuses` as `CF-114-4` and reads as the complete uncovered
set. `CF-114-3` (no cross-proxy H2 witness, with the H2 sibling driver `Driver::Http2AccessLogByteExact` already
existing) and `CF-114-5` (a present-but-unparseable header) are both banked in the ledger and appear nowhere in
the fixture's own documentation.

### N-19 — `SPEC.md` cites `filter.rs:109` for `should_log`; the phase's own insertion moved it to `:121`

`SPEC.md:32` and `:147`. On disk, `filter.rs:109` is a bare `}`; `pub fn should_log(` is at `:121`. The twelve
`GrpcStatus` variant lines this phase inserted above it pushed it down. The companion citation `file_sink.rs:103`
is still correct because that file's insertion landed below it. The SPEC was correct when written and is landed,
so it must not be edited — recorded because this project tracks citation drift and because the drift is
**self-inflicted**: a phase invalidates its own SPEC's citations into the files it edits.

### N-20 — the `0..=16` enum bound is duplicated across two crates with nothing pinning them equal

`crates/envoy-http1/src/hcm.rs:1669` (`.filter(|n| (0..=16).contains(n))`) and
`crates/envoy-config/src/bootstrap.rs:817` (`(0..=16).contains(&n).then_some(n as u8)`). The phase's stated goal
is ONE source of truth for the HTTP→gRPC **map**, and it achieves that. The **bound** now has two independent
copies. They agree today; a future gRPC code would need both edited, and no test cross-checks them.

### N-21 — `AccessLogResponseInfo.headers`' doc names one consumer where there are now three

`crates/envoy-http1/src/hcm.rs:1640` — *"borrowed for the upstream-service-time extract."* It now feeds
`extract_upstream_service_time`, phase 113's gated `grpc_status`, and phase 114's `effective_grpc_status`.
Verified pre-existing: identical text at the identical line at `c9136ae`, so the staleness originates with phase
113; phase 114 added the third consumer without updating it.

### N-22 — header-leg edge cases the implementation deliberately handles are unpinned

`crates/envoy-http1/src/hcm.rs:1667-1670`, test list at `:11886` (`["", "abc", "99", "-1", "1.0"]`). No panic
path exists — the chain is `Option`-based and the `as u8` is guarded by the range filter. But three branches the
implementation took on purpose have no coverage: the `.trim()` (no test passes a whitespace-padded value, so
nothing distinguishes this from a plain `parse()`), the **first-wins** behaviour on a duplicated `grpc-status`
header (`access_log_header_value` at `:2013` uses `.find(...)`, not a comma-join), and the leading `+` that
`i64::from_str` accepts. The upper boundary `"16"` is also untested, though `"0"` is. ⚠ Upstream's handling of a
duplicated inline `grpc-status` header could not be settled from this checkout; if upstream comma-joins, its
parse fails to `InvalidCode` while envoy-rust reads the first value and succeeds — which folds into F-2.

### N-23 — `derivation_is_ungated_and_differs_from_the_phase_113_field` never builds a record

`crates/envoy-http1/src/hcm.rs:11874`. Billed as pinning the governing finding, it asserts `!is_grpc_request(req)`
and `effective_grpc_status(&[], 404) == 12` as two independent facts. It never constructs an `AccessLogRecord`,
so it stays green against an implementation that populates `grpc_status_code` from the gated `grpc_status` field.
The real witness for the ungatedness is fixture `0094` probes 5, 6 and 8, mutation-proved — on H1 only. Same
class as F-3, one severity lower because H1 does have the cross-proxy witness.

### N-24 — the whole fixture corpus contains exactly ONE `statuses:` value, so the grammar's integer and case-insensitive halves have no cross-proxy witness

`tests/fixtures/0094-…/envoy.yaml:19` and `envoy-rust.yaml:17`, both `statuses: [UNIMPLEMENTED, INTERNAL]`.
Repo-wide there is no other `statuses:` list in any fixture (the one other hit is
`0019-upstream-active-health-check`'s unrelated `expected_statuses:`). Since the `filter:` block is byte-identical
on both sides, a fixture written `statuses: [5, "13"]` or `[unimplemented, INTERNAL]` would be a **free** upstream
witness for the integer and case-insensitivity rules, which today are pinned only by the pick session's
`--mode validate` prose with nothing in CI able to catch a regression. It also means the `statuses` axis is never
varied: a hardcoded `[12, 13]`, or a `12..=13` range match, passes the fixture.

### N-25 — the phase's cross-proxy code coverage is `{2, 12, 13, 14}`, and membership-TRUE is witnessed for `{12, 13}` alone

Derived here from the fixture and the test census, and recorded because a precisely bounded uncovered set is
more useful than a general remark:

```
codes with a membership-TRUE witness (in `statuses` AND a record kept):  {12, 13}          — 2 of 17
codes with any cross-proxy witness (fixture 0094):                       {2, 12, 13, 14}   — 4 of 17
codes reachable ONLY through the header leg (the map's image is
  {2, 7, 12, 13, 14, 16}):                     {0,1,3,4,5,6,8,9,10,11,15} — none witnessed anywhere
names with an index→code assertion anywhere:                             5 of 17  (see F-4)
```

Response codes mapping to 7 (`403`) and 16 (`401`) are asserted through the table only; 14 (`503`) is witnessed
cross-proxy as a **drop**, never as a keep.

### N-26 — `ADR-0197` presents a PARAPHRASE as a verbatim quotation

`DECISIONS.md` `ADR-0197` DECISION 3, echoed at `PLAN.md:31`. The ADR quotes `SPEC.md` as saying
*"Production `should_log` call sites are exactly TWO … the remaining 125 of the 127 occurrences are test
code."* `SPEC.md:147` actually reads *"**Production call sites are exactly TWO** … the remaining 125 of the 127
`.should_log(` occurrences in `crates/` are test code."* The quotation inserts `` `should_log` `` and drops
`` `.should_log(` `` and `` in `crates/` ``. Measured: `grep -cF` on the quoted string against `SPEC.md`
returns **0**; on the real string, **1**. ⚠ The quoted form *does* occur verbatim in `STATE_HISTORY.md`'s
archived scope handoff, so the ADR appears to have quoted the handoff and attributed it to the SPEC. This is
the same defect class `ADR-0195` recorded against `ADR-0193` one phase earlier.

### N-27 — "six contiguous `### ` sections" where the document's own listing shows five

`PROGRESS.md:1244`. The listing beneath the sentence names phases 70, 71, 72, 73, 74 — five filter-arm
sections, phase 73 carrying two arms in one — plus phase 75 marked as the *next* section. Measured over the
`BEHAVIOR_CONTRACT.md` span: **5**.

### N-28 — "the nine sections `PLAN.md` Step 5 enumerates" — the plan enumerates seven, and the README has seven

`PROGRESS.md:1226` and `ADR-0198` DECISION 4 (*"All nine required sections are present"*). `PLAN.md:1453`'s
semicolon-separated list has **seven** entries, and the landed README has exactly **seven** `## ` sections.
Neither reading yields nine. ⚠ This does not disturb F-12 or the README's 166-vs-73 line overrun, which is a
separate and correctly recorded correction.

### N-29 — the SPEC's ADR-log absence census was invalidated by its own landing commit

`SPEC.md:157` and `:271`. The claim is that `not_health_check_filter` and `traceable_filter` are *"measured
absent from the entire ADR log (`grep -ic` = 0 each, against `duration_filter` = 10 and `runtime_filter` = 9)"*,
and that `extension_filter` is *"never named in any ADR"* / *"none of which is named in any landed ADR"*.
Measured: at the pre-pick commit the three read **0/0/0** against controls **10/9**; at the commit that WROTE
the SPEC they read **2/2/2** against controls **13/12**, because `ADR-0196` lands in the same commit and names
all three. **The substantive point stands** — no *prior* ADR discussed them — but the stated census is false at
the commit carrying it. Same class as the self-referential count in F-9.

### N-30 — "352,248 bytes" is a character count

`PROGRESS.md:96` and `ADR-0198` DECISION 1 item 1 (*"7762 lines and 352,248 bytes, both invariant"*). Measured:
**353,032 bytes** and **352,248 characters**, both invariant across the rider; the 784-byte gap is the
multi-byte em-dashes and arrows. The byte-neutrality argument is sound under **either** measure — only the label
is wrong. ⚠ Recorded because this project's own standing trap says a length series in characters reads
differently from `wc -c`, and this is that trap landing in a landed ADR.

### N-31 — a zero and its positive control taken at different scopes

`SPEC.md:157` — *"there is no downstream health-check filter (`health_check_filter` = **0** hits in `crates/`,
control `envoy.filters.http` = **363**)"*. The zero is scoped to `crates/`; the control figure is the `*.rs`
count. Over `crates/`, the control reads 422 lines / 424 occurrences. The conclusion is unaffected — the zero
holds under both scopes — but this is exactly the "state the scope with the number" discipline `ADR-0196`
insists on for its own `gauge`/`histogram` control, applied inconsistently four sections earlier in the
same document family.

### N-32 — a `diff` transcript presented as command output is not verbatim, and one copy ALTERS a quoted line

`PROGRESS.md:1081-1088` and the fixture `README.md:134-140`. `PROGRESS.md`'s own banner says the file carries
*"REAL quoted command output"*. Both blocks inline the hunk headers onto the `<` line and drop the `---`
separators that `diff(1)` prints. The README's copy additionally **changes the content of a quoted line**,
padding `address: 0.0.0.0,` with two extra spaces so it aligns under `127.0.0.1,`. Cosmetic, and the underlying
four hunks are real (this review reproduced them) — but a transcript that is edited for alignment is no longer
a transcript, and a reader diffing it against a real run gets a spurious hit.

### N-33 — `CF-114-4` was NARROWED by `ADR-0197` and silently WIDENED back by two later artifacts

`ADR-0197` DECISION 10 and `PLAN.md:1551` say the carry-forward *"reduces to the `exclude: true` witness
**alone**"*. `BEHAVIOR_CONTRACT.md:3505` then banks *"`exclude` **and the empty-list rule**"* under
`CF-114-4`, and the fixture `README.md:59` banks *"Rules **3 and 4**"* under it. Under document precedence the
later artifacts win, so the narrowing statement is stale and nothing says so. ⚠ This review's `CF-114-11`
amends `CF-114-4` again and states the scope explicitly to stop the drift.

### N-34 — correction 3 OVER-CHARGES `SPEC.md` §6's probe table, and the fixture README propagates the harder form

`PLAN.md:33` and `:1202`, fixture `README.md:75`; softened at `ADR-0197` DECISION 3(a). `PLAN.md` says
*"`SPEC.md` §6's probe table **gives the wrong observed status** for the gRPC probes."* But `SPEC.md:180`'s
column is literally headed `` `direct_response.status` ``, and 404/400/200/503 **are** the correct route
statuses; the table has no observed-status column at all. `ADR-0197` walks this back in the same sentence
(*"the SPEC's derived status column is right but its implied mechanism is not"*) — which is the accurate
statement — yet the README propagates the harder form: *"the landed `SPEC.md` §6 probe table gives the route's
status for these probes. **That is wrong**."* Giving the route's status is what that column is for. Found
independently by two reviewers. ⚠ The underlying correction is real and important: the phase-110 transform
rewrites the gRPC probes' status to 200, which is why probes 1 and 2 witness the header leg. Only the charge
against the SPEC is over-stated.

### N-35 — `ADR-0197`'s "22 of 23" SPEC anchors is not independently reproducible

`ADR-0197` DECISION 3. A census of distinct `file:line` anchors in `SPEC.md` yields **22**, of which 21
re-derive correct at the SPEC's own commit and one is wrong — `tests/differential/src/lib.rs:1187`, exactly the
one the ADR names. 23 is reachable only by counting one of the two cited line *ranges* as two anchors. The ADR
states no enumeration, so the denominator cannot be reproduced. Not load-bearing; recorded because a stated
denominator is a claim.

### N-36 — the phase's citation blast radius across `docs/` was never censused

`SPEC.md` §8 PV-1 warns that *"any phase that edits `hcm.rs` moves them"*, yet no phase-114 document records
what this phase moved **for other documents**. Censused here over all git-tracked `docs/` files, resolving
basenames against the nine touched crate files and comparing each cited line against the arc's first changed
line: roughly **2,300** resolved citations point at a line whose content changed, dominated by
`bootstrap.rs` (~1,617 of 1,916) and the two HCMs. ⚠ **This is a systemic property of the repository, not a
phase-114 defect**: the overwhelming majority sit in `STATE_HISTORY.md` and `DECISIONS.md`, both append-only
archives where staleness is inherent and expected, and this project's own doctrine is that a landed document is
never edited. It is recorded for one reason: **F-10 shows that at least one of them lands in a live document**,
and the cheap discipline that would have caught it is to census the touched files' citations in the LIVE
documents only (`BEHAVIOR_CONTRACT.md`, `ROADMAP.md`, `MISSION.md`, `SKILL_ROUTING.md`, `STATE.md`) at the end
of any phase that edits a crate file.

---

## §5 — Subagent findings adjudicated, and dissent

⚠ **A subagent finding is a CLAIM.** Every finding in §3 and §4 was re-verified on disk by this main session
before it was recorded. Four claims did not survive that pass in the form they arrived, and they are recorded
here rather than quietly dropped.

**NOT CHARGED — "eight token-grammar cells are labelled MEASURED without evidence."** A reviewer censused
`"05"`, `"+5"`, `" 5"`, `"5 "`, `"0x5"`, `""`, `"on"` and `"y"` across `SPEC.md` and `PROGRESS.md`, found zero
occurrences against four positive controls, and concluded the MEASURED label at `bootstrap.rs:813` and
`:20471` was transcribed from phase 113's *wire-header* parser rather than measured on the config surface. **The
measurement exists.** `ADR-0197` DECISION 6 states it directly: *"The grammar itself was RE-MEASURED at this
PLAN-write over 50 tokens, one `--mode validate` run each … the string-numeric form is PERMISSIVE — `"05"`,
`"+5"`, `" 5"` and `"5 "` all ACCEPT, which is exactly `trim().parse::<i64>()`, while `"0x5"` and `""`
REJECT."* The census was over the wrong document set: `SPEC.md` carries the state-0/1 transcript, and the
PLAN-write's own re-measurement lands in its ADR. **The label stands.** ⚠ The residue worth recording is that
`ADR-0197` DECISION 6 states the result in prose ("50 tokens") where `SPEC.md` §2.2 gives a per-token table, so
the second measurement is less auditable than the first — a style note, not a defect, and not banked.

**DOWNGRADED — "the `BEHAVIOR_CONTRACT.md` `0x5` row is wrong."** The contract row reads `` `"0x5"`, `""` |
REJECT ``, **quoted**, and the quoted form does reject correctly. The row above it distinguishes bare from
quoted deliberately (`` `0`, `5`, `16`, and the quoted `"5"` ``). So the contract is not wrong; it is **silent**
on a class envoy-rust accepts. Re-stated as N-7 at Minor, with the source trace through the pinned
`serde_yaml` retained because that is what makes the class real.

**DOWNGRADED — the fixture README's four-hunk claim, from Important to Minor.** The universal quantifier is
false (N-13), but the sentence's load-bearing half — that this fixture's four divergences are non-semantic —
is true, and the four hunks are quoted verbatim in the six lines immediately below it, so a reader can check the
claim that matters without trusting the generalisation.

**CORRECTED — a claim in this session's own briefing.** The handoff's SHA-audit warning names three known-false
`MISSING`es. Under the handoff's own command there is exactly one; see §0.3. ⚠ **This is the second consecutive
review to have to correct the same warning** — `ADR-0195` recorded that it named two where there was one. The
warning has now over-counted in two different directions across two phases, which suggests the figure is being
carried forward rather than re-derived. It should be re-derived at each handoff, or dropped in favour of naming
the *rule* (a truncated Docker digest and non-commit build hashes are not defects) without a count.

**RE-CLASSIFIED, not downgraded — two findings the documentation audit filed as "Issue (must fix — forward)".**
F-9 (the wrong `docs/` numstat) and F-11 (the two different nines) are real and are charged in full. But in this
project's taxonomy **§2 "Issues — Must Fix" means the phase returns to §5 state 3** (§5.2) — it is reserved for
implementation defects, not for a wrong figure in a landed document, which is corrected **forward** in a new ADR
and never by editing the landed text (D-3.5). The reviewer's own qualifier, "must fix — **forward**", is exactly
that mechanism. Both are therefore Important, and `ADR-0199` carries the corrections.

**A concurrency note the audit surfaced about this session itself.** The documentation audit began at
`762d3cc` and observed `HEAD` move to `cadf2d8` beneath it — this session's own CI-record commit. It correctly
pinned every arc measurement to the explicit range `c9136ae..762d3cc` rather than to `HEAD`, so no figure it
reports is contaminated. ⚠ Recorded because it is the failure mode a `<base> HEAD` numstat citation produces
silently: **cite the range, not the endpoint.**

**Dissent recorded, not resolved.** One reviewer put the upstream-divergence prediction in F-2 at 75%
confidence and named the unverified leg precisely (that upstream's filter and formatter share
`Grpc::Common::getGrpcStatus`). This review does **not** adopt that premise — D-3.3 forbids settling
"equivalent" by reading Envoy source, and no upstream source is present in this checkout. F-2 is therefore
charged on what IS on disk (phase 113's in-tree measurement contradicting the landed choice, and the false
"unmeasured" ground) and banked with a measurement that does not need the premise.

---

## §6 — Deliberate decisions verified, not re-litigated

These were checked and found sound. They are recorded so a later session does not spend the check again.

- **The two-source rule** (header, else map) against upstream's three (header, trailer, map). The trailer leg is
  blocked behind `CF-111-2` — H1 discards response trailers behind chunked response encoding, a ~153-site change
  — and `CF-114-1` banks it. `SPEC.md` §5 non-goal 1 required the implementation be shaped so the third source is
  an added branch; the two-arm `match` at `hcm.rs:1671` satisfies that at the control-flow level. ⚠ F-2's closing
  note qualifies it at the *type* level.
- **The item-level `pub` on `http_to_grpc_status` with `mod grpc` staying `pub(crate)`** (PV-3). This DEPARTS
  from `ADR-0193` DECISION 6, and `ADR-0197` says so and gives the reason: that decision refused a widening on a
  dependency-**cycle** argument about `envoy-accesslog`, which does not apply to a crate `envoy-http2` already
  depends on at many sites. The departure is sound and correctly declared. F-7 charges the dead re-export, not
  the widening.
- **`LogFilter::GrpcStatus` carrying plain data rather than a trait object.** The `Header` and `Metadata` arms
  need an injected matcher because they consume `envoy-config` types; this arm consumes integers. The
  `ADR-0150` seam is correctly not involved, and `envoy-accesslog`'s dependency set is unchanged.
- **The `.expect("validated by validate_access_logs")` at `crates/envoy-http1/src/hcm.rs:1914`.** Not a live
  panic path: the validator checks every token, recurses into composition children, and calls the same
  `resolve_grpc_status_token` the compile step calls, so the two cannot disagree. The dynamic LDS/CDS/RDS merge
  re-runs `bootstrap::validate`, so no unvalidated filter reaches the compile step. Consistent with the phase-70
  `unreachable!` and the phase-72 SafeRegex precedent; the phase introduces no new exposure.
- **The `_ => unreachable!` seventh-arm tuple match** (`hcm.rs:1919`). Every arm fully specifies all seven
  positions, so no arm can shadow another, and ambiguous or zero-arm configs still fall through to the
  validator-guaranteed-unreachable branch.
- **H1/H2 parity, and the deliberate asymmetry underneath it.** `apply_grpc_local_reply` is H1-only
  (`CF-110-1`), so a gRPC request against a `direct_response: 503` gets a rewritten 200 plus `grpc-status: 14`
  on H1 and an untransformed 503 on H2 — but both derive **14**, because the transform emits
  `http_to_grpc_status(status)`, the same function H2's fallback leg calls. The pre-existing phase-110 H2 gap
  does not leak into the filter verdict. ⚠ That convergence is not pinned by any test; it folds into F-3.
- **The `%GRPC_STATUS%` gate was not relaxed.** Phase 113's field stays request-gated and its fixture `0093`
  witness is untouched. The phase added a second, differently-shaped value beside it, which is exactly what
  `SPEC.md` §5 non-goal 4 required.
- **The §6.1 split gate does not fire** — 10 tasks against ~25 and 1038 net against ~1500, re-derived here. The
  state-2 no-split adjudication stands on the LANDED number, not only the predicted one.
- **The five local differential RED tests were not re-litigated.** They carry a causal control at `c9136ae`,
  before any phase-114 code existed, and the identity closes (`2310 + 5 = 2315` local against 2315 in CI at the
  identical tree, binaries `170 = 170`). Re-deriving that would cost a full debug build to reproduce a recorded
  result.
- **`known-failures.txt` was not trimmed**, and must not be on local evidence: the local h2spec gate is vacuous
  on this host and `3.5/2` passes here while CI fails it.

---

## §7 — Carry-forwards for the state-6 close-out to bank

**Opened by this review.** Nothing here was fixed (§6.3; `ADR-0165`).

| id | what | where |
|---|---|---|
| **CF-114-6** | The phase-114 derivation helper was spliced between `build_access_log_record`'s doc comment and its `fn` line, leaving a join-point function undocumented and the helper carrying someone else's provenance note. **The same class as `CF-113-7`, which this phase's own Task 1 repaired.** Two-line remedy. | `crates/envoy-http1/src/hcm.rs:1656`/`:1666`/`:1676` (F-1) |
| **CF-114-7** | **AMENDS `CF-114-5`.** Its "unmeasured upstream" premise is false — phase 113 measured the same header parse in-tree (`-1` for unparseable, `99` passed through) — and the landed map-fallback is therefore a PREDICTED divergence, now pinned by a green test. Settled by two extra routes on an **upstream-only** recon listener. | `crates/envoy-http1/src/hcm.rs:1666-1674`, `:11885`; `BEHAVIOR_CONTRACT.md` §H (F-2) |
| **CF-114-8** | **AMENDS `CF-114-3`.** The H2 arm's declared in-process pin has no call edge to the H2 record build and passes against a stub; the four prior filter-arm phases each landed an end-to-end H2 test and the helpers they use already exist. | `crates/envoy-http2/src/hcm.rs:7773`, `:1202`, `:1222` (F-3) |
| **CF-114-9** | Twelve of the seventeen canonical name→code bindings are asserted nowhere, and the accept sweep is a byte-copy of the table it tests. Remedy: a `[(name, code); 17]` literal taken from the SPEC transcript, not from the implementation. | `crates/envoy-config/src/bootstrap.rs:790`, `:20470` (F-4) |
| **CF-114-10** | The new arm is never composed under `and_filter`/`or_filter` at any of the five stages, in the phase that widened that recursion. It works; it is untested. Phase 74 landed exactly these tests for its arm. | `filter.rs:160`/`:169`; `bootstrap.rs:5875`; `hcm.rs:1886` (F-5) |
| **CF-114-11** | **AMENDS `CF-114-4`.** `codes: []` with `exclude: true` — a config-reachable keep-everything sink — has no test, and its doc clause is the one not labelled MEASURED. | `crates/envoy-accesslog/src/filter.rs:102`, `:557` (F-6) |
| **CF-114-12** | The Task-2 `pub use` re-export has zero consumers (Task 5 replaced the only one), and `lib.rs`'s "Nothing outside this crate may reach it" is now false. No lint in the tree can catch either. | `crates/envoy-http1/src/lib.rs:34`, `:17-19` (F-7) |
| **CF-114-13** | On the whole tested surface the header leg's producer emits `http_to_grpc_status(status)` — the same function the fallback calls — so a leg swap is unwitnessable by fixture `0094`. The separating shape is a **proxied** response carrying a backend-supplied `grpc-status`; eleven of the seventeen codes are reachable only that way. | `crates/envoy-http1/src/grpc.rs:167`, `:69`; `hcm.rs:1666` (F-8) |
| **CF-114-14** | The **bare** radix-prefixed (`0x5` → 5) and leading-zero (`010` → 10, octal 8 upstream) integer token forms are unmeasured on both sides and untested here. One `--mode validate` run settles both. | `crates/envoy-config/src/bootstrap.rs:774-779` (N-7, N-8) |
| **CF-114-15** | `grpc_status_filter` is the only one of the seven `AccessLogFilter` arms with zero fuzz-corpus reach and zero tracked seeds, against six sibling controls. No defect predicted; the narrowing cast is guarded. | `crates/envoy-config/fuzz/corpus/` (N-9) |
| **CF-114-17** | A landed `docs/`-slice numstat is wrong in both figures — `+1424 / −0` against a measured `1843 / 17` — and wrong self-referentially, the counting document being inside the set it counts. The gate-relevant non-`docs/` net of 1038 is unaffected. | `PROGRESS.md:1350` (F-9) |
| **CF-114-18** | This phase's Task 2 shifted `crates/envoy-http1/src/grpc.rs` by five lines and broke a citation in `BEHAVIOR_CONTRACT.md` — a **live** document the phase itself edited at Task 10 — onto three lines of comment prose *about the same sentinel*. The sentinel is now at `:163`. | `BEHAVIOR_CONTRACT.md:846` (F-10) |
| **CF-114-19** | `PROGRESS.md` §5 and `ADR-0198` DECISION 1 enumerate two DIFFERENT sets of "nine `PLAN.md` corrections" whose union is ten; `PROGRESS.md:1396`'s claim that all nine are in DECISION 1 is false for its item 4; and DECISION 1's stated "order they were hit" is not that order. | `PROGRESS.md:1394`; `ADR-0198` D1 (F-11) |
| **CF-114-20** | The `PLAN.md` prototype did not carry Task 1: its H2 cell `32 0` is byte-for-byte the arc EXCLUDING the rider, against a landed `42 10`. Two method claims are over-broad, and `ADR-0198` correction 1 explains the `14 14`→`10 10` miss without the structural explanation this makes available. Net unaffected; the §6.1 adjudication stands. | `PLAN.md:80`, `:66`, `:1565` (F-12) |
| **CF-114-21** | The corrected "exactly TWO" production `should_log` sites survives uncorrected at `SPEC.md:211` (the §7 estimate row) and at **`ROADMAP.md:196`** (the landed phase row), and `ADR-0197` mis-states the claim's location as `SPEC.md` §1, which contains no such text. True figure re-derived here: **FIVE** production of **138** total. | `SPEC.md:211`; `ROADMAP.md:196`; `ADR-0197` D3 (F-13) |
| **CF-114-16** | The Minor documentation-and-coverage set, banked as one: **N-1** to **N-6**, **N-10** to **N-36**. Stale counts (`20 fields total`, the module doc, the response-headers doc), weak or partial pins (`exclude` polarity, the 2-of-6 neutrality sweep, the `FileSink` delegation, the record-free ungatedness test), untested reachable cells (`deny_unknown_fields`, the nested bad token, the numeric error arm, the YAML reject boundary), fixture-README accuracy (the false universal, the bind-mount reason, the span-less md5, the omitted carry-forwards, the suppression-control over-claim, the 404 catch-all), the self-inflicted `SPEC.md:32`/`:147` citation drift, the duplicated `0..=16` bound, and the record-accuracy set the documentation audit added (a paraphrase quoted as verbatim, three prose counts contradicted by their own listings, a census invalidated by its own landing commit, a byte/character label, a scope-mismatched control, two edited `diff` transcripts, a silently re-widened carry-forward, an over-charged SPEC correction, an unreproducible anchor denominator, and the uncensused live-document citation blast radius). | see §4 |

**Consumed by this phase, and it stays consumed:** `CF-113-7`, by the Task-1 rider (`2cf0830`), byte-neutrally
and in its own labelled commit. ⚠ **`CF-114-6` is not a re-opening of `CF-113-7`** — it is the same *class* at a
new site, and the record should say so rather than reusing the old id.

**Carried forward INTACT and UNCONSUMED:** `CF-114-1` (no TRAILER source, blocked behind `CF-111-2`),
`CF-114-2` (five arms still unbuilt), `CF-114-3` (amended by `CF-114-8`), `CF-114-4` (amended by `CF-114-11`),
`CF-114-5` (amended by `CF-114-7`); `CF-113-1/2/3`, `CF-113-5/6`, `CF-113-8` … `CF-113-13`; `CF-112-1` …
`CF-112-19`; the `112.1`/`112.2`/`111`/`110.x`/`109.x`/`108.2` REVIEW sets; `CF-111-1` … `CF-111-9`;
`CF-110-1` … `CF-110-9`; `CF-109-1/2/3`; `CF-108-1/2/3`; `CF-76-1`; `CF-75-2/3/4/6`; `CF-72-2`/`CF-75-1`;
`M71-6`; `CF-74-1/2/3/4/6`; `CF-73-1`; HTTP-filters-family (1)–(4). **`CF-113-4` stays CONSUMED. `CF-111-4`
stays consumed only in PART. `CF-112-5` stays CLOSED. `CF-112-8` Consequence 2 stays BANKED as structurally
unwitnessable.** ⚠ **`CF-112-13`'s blast radius is FIVE internally-tagged unit variants, not the four its own
REVIEW names.** ⚠ **`CF-75-5` was re-observed here and is still open**: `cdn_loop_parse` has **0** tracked
corpus seeds against 67 / 11 / 3 / 1 for the other four targets. The phase-112 ALPN cleanup remains a **RIDER,
NOT A PHASE** (`ADR-0192` DECISION 5) and was neither taken nor re-costed.

---

## §8 — Assessment, and the §7.5 gate adjudicated

**Ready to merge: YES.** The phase implements the rule it measured, in the shape the plan specified, with the
one seam that mattered — the ungated derivation, distinct from phase 113's gated field — correct on both codecs
and mutation-proved cross-proxy on one. The thirteen Important findings fall in three families —
**verification strength**, **coverage disclosure** and **record accuracy** — and not one of them is a wrong
value against a measured upstream cell.

**What separates this phase's record from its execution is worth naming, because it is a pattern.** Every
figure the phase asserted about *itself* reproduced exactly — the net 1038, the four-file reconciliation, the
five `should_log` sites, the seventeen added tests, the fixture and runner censuses, the CI identity. The
weakness is uniformly in **what the tests ask**: an existence sweep where a value sweep was needed (F-4), a
relative assertion where an absolute one was needed (N-3), a direct helper call where a call-edge was needed
(F-3, N-23), a producer that shares its predicate with the assertion (F-8), and a recursion widened without
being composed (F-5). `ADR-0195` DECISION 7 named the mirror-image of this one phase ago — code figures
disciplined, prose counts not. Here the code figures are again disciplined and the **test questions** are not.
The two together suggest the checkable thing to add: **for each new predicate input, name the mutation that
would go RED, and confirm the test that catches it exists.**

**The second pattern is narrower and cheaper to fix.** Five of the thirteen Important findings (F-9 and F-11
through F-13, plus F-10) are one shape: **a figure or a citation that was correct when taken and was invalidated
by the very commit that carried it**. The `docs/` numstat counted a file it was being written into; the SPEC's
ADR-log census was falsified by the ADR landing beside it; the SPEC's own `filter.rs:109` citation was moved by
the code the SPEC specified; a live `BEHAVIOR_CONTRACT.md` citation was moved by Task 2 five commits before Task
10 edited that same contract file. `ADR-0195` DECISION 7 diagnosed *"re-derive every prose count from the
artifact it summarises as the last edit before staging."* That is necessary and, on this evidence, not
sufficient — several of these were re-derived correctly and then invalidated **afterwards**, by the staging
itself. The additional rule the evidence supports: **a count or citation that names a file the commit also
changes must be taken from the STAGED tree, not from the working tree before the edit** — and any phase that
edits a crate file should census that file's citations in the five LIVE documents before it commits.

### The §7.5 gate, all six adjudicated

- **(a) new/changed differential fixtures green** — PASS. Fixture `0094` is green locally and in CI at
  `762d3cc`, witnessed by name in the job log. ⚠ F-8, N-15, N-16 and N-24 bound what it witnesses; none makes it
  red.
- **(b) pre-existing fixtures still green** — PASS. `binaries=170 passed=2315 failed=0` in CI. The five local
  reds carry a causal control at the pre-phase commit and the identity closes.
- **(c) conformance at threshold** — PASS, CI-authoritative. `h2spec not found` = 0 against 10 mentions;
  `Version: 2.6.0`. The local gate is vacuous on this host and was correctly not used to trim
  `known-failures.txt`.
- **(d) new fuzz target clean** — PASS, vacuously by letter (no new target) and discharged in substance (all
  five pre-existing targets exit 0 at the CI short budget). ⚠ N-9 records that the new arm has no corpus reach,
  which is a coverage gap rather than a failed leg.
- **(e) the five `cargo` legs** — PASS, on a forced dirty set after the first `build`/`clippy` pair proved to be
  a warm-cache no-op. `cargo deny check`: advisories, bans, licenses, sources all ok.
- **(f) `REVIEW.md` approved** — **PASS. This document.** §2 is empty; §5.2 does not fire.

**All six PASS. The phase is done under §7.5 and advances to §5 state 6.**

### STOP CONDITION — re-derived from disk at this review. ALL THREE LEGS FALSE

Measured at `762d3cc`, before any edit by this session, and unchanged by it (this review touches no
`ROADMAP.md` row and adds no crate).

- **Leg (i) — FALSE.** `ROADMAP.md` holds **122** rows: **121 `done`, 0 `in-progress`, 1 `planned`**. The
  buckets sum to 122. The one `planned` row is `114`, at file line **196** — it stays `planned` until the
  state-6 close-out, so this leg is unchanged when this session ends. ⚠ Status is field **4** on a `' | '`
  split driven from the `^\| [0-9]` prefix. The field-count histogram is `{6: 120, 7: 1, 10: 1}`, so the
  forbidden `NF == 6` filter reads **120** and silently drops the two rows carrying unescaped in-cell pipes, at
  file lines **168** and **169**. They are append-only history and must not be "fixed".
- **Leg (ii) — FALSE.** **14** crates — `envoy-{accesslog,admin,bin,cluster,config,filter,health,http1,http2,jwt,listener,stats,tcp,tls}`
  — with `envoy-http3`, `envoy-grpc`, `envoy-wasm`, `envoy-protos` and `envoy-runtime` all absent by `test -d`.
  `quinn` / `wasmtime` / `tonic` / `opentelemetry` / `prost` = **0** across the **28** manifests from
  `git ls-files '*Cargo.toml'`; ⚠ the positive control was taken by the identical invocation, `tokio` = **19 of
  28**. ⚠ The denominator is method-dependent: `crates/*/Cargo.toml` plus the root gives **15**. `histogram` =
  **0** over both `crates/` and `crates/*/src/`, against a `gauge` control of **365** and **352** in those same
  two scopes. QUIC and HTTP/3, the gRPC data path, ADS/delta-xDS/SDS/RTDS, hot restart, the stats sinks, the
  histogram primitive, **five** of the twelve access-log filter arms and the WASM host are all unbuilt.
- **Leg (iii) — FALSE.** **11** `### ` family headings, one of which (`### WASM host family`) carries **ZERO**
  rows: 10/5/3/14/3/4/6/31/6/**0**/13, with **27** rows before the first heading, summing to 122. ⚠ A naive
  `awk` census reports only 10 headings because it never emits the zero-row one; the census must seed every
  heading at 0. ⚠ `ROADMAP.md` has a filing defect — **seven** rows whose titles begin `Observability family:`
  sit physically under `### Deprecated / edge features` (file lines 216–222), so the heading-SLICE census reads
  Observability **31** while the LOGICAL family is **38**. Do not repair it.

**ALL THREE LEGS ARE FALSE. The mission is NOT complete. No `stop` file was created and none exists.**
`ADR-0167` DECISION 2 governs: the operator's standing conditional instruction is an instruction to EVALUATE
the condition, not evidence that the answer changed.

### Next state

**§5 state 6 — the CLOSE-OUT — and it is a SEPARATE SESSION** (§5.1; `ADR-0127`: a reviewer must not close out
what it reviewed). That session flips `ROADMAP.md` row `114` to `done` — **the status cell only** — relocates
this phase's `STATE.md` Notes subsections to `STATE_HISTORY.md`, and banks `CF-114-6` … `CF-114-16` alongside
everything carried forward. ⚠ A close-out adds **no** ADR and **no** Notes subsection, and its `## Last commit`
section carries **no** PENDING line. ⚠ **A landed `REVIEW.md` is never edited**: if a second round is ever
needed it writes `REVIEW-2.md` and reads this one first.
