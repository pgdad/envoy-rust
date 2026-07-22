# Phase 73 — access-log `and_filter` / `or_filter` (recursive composition) — §5 state-5 CODE-REVIEW

> `superpowers:requesting-code-review`, run in its OWN session per §5.1. Reviews
> the phase-73 diff (`git diff 23ec5aa..HEAD -- crates/ tests/`; code commits T1
> `78269e2` → T8 `e8a17c2` + fmt `41c9ee7`). Base for the review diff = the
> state-2 PLAN-write commit `23ec5aa67b283cf02e4649984991d5eeced485c3` (last
> pre-implementation commit). HEAD = the state-4 verification commit
> `3138f90acfcd7ecbd4c6079152968fb8e90f9e38`, CI `completed` / `success` on the
> FULL 40-char SHA (run `29917117679`, 10m24s).
>
> Method (memory `state5-must-probe-untested-compositions`): four FRESH
> zero-context read-only reviewers fanned out across independent dimensions
> (runtime correctness/parity, ADR-0150 seam integrity, fixture/fuzz coverage,
> config-validation fail-loud). The MAIN session then did the decisive LIVE-PROBE
> measurements ITSELF — a GREEN §7.5 gate proves the code does what its tests ASK,
> not that the tests ask the right question. A phase adding a RECURSIVE composition
> handler over the existing leaf predicates has untested combinations; I grepped
> which the fixtures exercise (header-leaf children only, depth ≤2) and LIVE-PROBED
> the rest against BOTH proxies.

## Verdict: **APPROVED — no MUST-FIX. Next = §5 state-6 close-out.**

The implementation is clean, correct-as-scoped, and MEASURED at parity with
`envoyproxy/envoy:v1.33.0` across every untested composition I probed. The four
load-bearing invariants (ADR-0150 seam, no-`Box`/no-`Clone`/no-`Eq`, recursion
finiteness, fail-loud completeness) are all HELD. No Critical and no Important
MUST-FIX survived verification. The Reviewer-C "Important" coverage gaps
(response-flag / mixed-leaf / depth-3 composition children untested) were the
right questions to raise — and my LIVE-PROBES answer them: all three are
byte-identical cross-proxy, so they are COVERAGE notes (a future fixture could
pin them), NOT divergences. Two Minors + two Nits are recorded for a future touch.
ROADMAP row `73` stays `in-progress` (no flip until the state-6 close-out).

---

## LIVE-PROBE evidence (MEASURED this session — envoy-rust DEBUG `envoy-bin` vs. `envoyproxy/envoy:v1.33.0`)

The fixtures `0079`/`0080` witness only **`header_filter` children at depth ≤2**.
Everything below is UNTESTED by any fixture; each was measured live.

### Probe group 1 — config-acceptance parity across the whole structural space (`--mode validate` / boot-validate; networking-free)

Six configs, each an H1 HCM file-sink `filter`, run through envoy-rust
(`envoy-bin -c`, `ConfigError`→stdout) AND upstream (`--mode validate`):

| variant | envoy-rust | upstream v1.33.0 | verdict |
|---|---|---|---|
| A — `and_filter{ filters:[one child] }` | REJECT `InsufficientCompositeFilters{count:1}` | REJECT `AndFilterValidationError.Filters: value must contain at least 2 item(s)` | **PARITY** (both reject) |
| B — `and_filter: {}` (empty → `filters:[]`) | REJECT `InsufficientCompositeFilters{count:0}` | REJECT `AndFilterValidationError.Filters: … at least 2 item(s)` | **PARITY** |
| C — depth-3 `and[ or[ and[a,b], c ], d ]` | ACCEPT | ACCEPT (`configuration OK`) | **PARITY** (both accept) |
| D — mixed `and_filter{[ status_code_filter, header_filter ]}` | ACCEPT | ACCEPT | **PARITY** |
| E — two arms set (`header_filter` + `and_filter`) | REJECT `AmbiguousAccessLogFilter … more than one` | REJECT `'and_filter' has already been set … oneof` | **PARITY** (both reject) |
| F — nested under-2 `or[ a, and[ single ] ]` | REJECT `InsufficientCompositeFilters{count:1}` | REJECT `OrFilterValidationError.Filters[1] … AndFilter … at least 2 item(s)` | **PARITY** |

Closes the coverage gap that the reject paths were pinned **in-process only**
(Reviewer C Minor #3/#4): both proxies boot-reject the under-2 / empty / multi-arm
shapes, and **F proves the recursion descends into a nested composition and rejects
its under-2 child at its own level** on BOTH proxies (upstream even reports the
`Filters[1]` index — same descent envoy-rust does via `iter_mut()`). Fail-loud
CLASS parity confirmed (text differs per ADR-0049, D-3.3 — the REJECTION matches).

### Probe group 2 — runtime keep/drop, MIXED status+header composition (the `status` slot through recursion)

`or_filter{[ status_code_filter{ op:GE, default_value:500 }, header_filter{ x-a=1 } ]}`,
two `direct_response` routes `/a→200` `/b→503`, file format
`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%`, no backend, graceful-stop flush. Drove
`GET /a`, `GET /b`, `GET /a` w/ `x-a:1`:

| request | evaluation | envoy-rust | real Envoy v1.33.0 |
|---|---|---|---|
| `/a` (200, no hdr) | GE500 false, hdr false → OR false | DROP | DROP |
| `/b` (503) | GE500 **true** → OR true | `STATUS=503 PATH=/b` | `STATUS=503 PATH=/b` |
| `/a` + `x-a:1` (200) | hdr **true** → OR true | `STATUS=200 PATH=/a` | `STATUS=200 PATH=/a` |

**Byte-identical** (both files: `STATUS=503 PATH=/b` then `STATUS=200 PATH=/a`).
Discriminates on the `status` slot (via `/b`) AND the `headers` slot (via `x-a`)
threaded through the SAME `or_filter` recursion. The fixtures test only
header-children; this is the first cross-proxy witness that a `status_code_filter`
child is threaded correctly. Closes Reviewer C Important #2 (heterogeneous
composition) → PARITY.

### Probe group 3 — runtime keep/drop, `response_flag_filter` composition child (the `response_flags` slot through recursion)

`or_filter{[ response_flag_filter{ flags:[NR] }, header_filter{ x-a=1 } ]}`, one
`direct_response` route `/a→200` (no catch-all path → an unmatched path yields the
`NR` No-Route flag with no backend). Drove `GET /a`, `GET /zzz` (no route), `GET /a`
w/ `x-a:1`:

| request | evaluation | envoy-rust | real Envoy v1.33.0 |
|---|---|---|---|
| `/a` (200, matched) | no NR flag, hdr false → OR false | DROP | DROP |
| `/zzz` (404, NR) | `response_flag_filter{NR}` **true** → OR true | `STATUS=404 PATH=/zzz` | `STATUS=404 PATH=/zzz` |
| `/a` + `x-a:1` | hdr **true** → OR true | `STATUS=200 PATH=/a` | `STATUS=200 PATH=/a` |

**Byte-identical.** This is the ONE `should_log(status, response_flags, headers)`
argument slot with zero prior coverage inside a composition (Reviewer C Important
#1). MEASURED PARITY — the `response_flags` slot is threaded verbatim to the child.
All THREE `should_log` slots are now witnessed cross-proxy through the recursion.

### Probe group 4 — runtime keep/drop, DEPTH-3 nesting (CF-73-1 parity, beyond config-acceptance)

`and[ or[ and[x-a,x-b], x-c ], x-d ]` (depth-3), predicate
`(((x-a & x-b) | x-c) & x-d)`, one `direct_response /x→200`. Drove five requests:

| request headers | predicate | expected | rust | envoy |
|---|---|---|---|---|
| `x-d` | inner-or false → outer false | DROP | DROP | DROP |
| `x-a,x-b,x-d` | inner-and true → outer true | KEEP | KEEP | KEEP |
| `x-c,x-d` | inner-or true (x-c) → outer true | KEEP | KEEP | KEEP |
| `x-a,x-b` (no d) | outer AND x-d false | DROP | DROP | DROP |
| `x-c` | outer AND x-d false | DROP | DROP | DROP |

**Byte-identical** — each file holds exactly TWO `STATUS=200 PATH=/x` lines (the
two KEEP rows). CF-73-1's "arbitrary depth, no stack guard, matching upstream"
claim is now MEASURED parity at runtime (depth-3, mixed AND/OR), not just at
config-acceptance. The parity hypothesis the handoff flagged for confirmation is
CONFIRMED.

---

## Dimension synthesis (four fresh zero-context read-only reviewers)

- **Runtime correctness / parity:** No Critical/Important. `should_log` `.all()`/`.any()`
  semantics correct; `(status, response_flags, headers)` threaded verbatim to every
  child. The **empty-vec edge (`all([])==true`, `any([])==false`) is UNREACHABLE** —
  traced BOTH config paths (`parse_bootstrap` AND `load_dynamic_resources`/LDS-merged
  listeners): each runs `validate` → `validate_access_logs` → the recursive
  `validate_access_log_filter` (which rejects `filters.len()<2` at every level) BEFORE
  any sink is compiled; serde can only PRODUCE an empty vec, never mutate a validated
  one. The `_ => unreachable!()` in `compile_access_log_filter` is genuinely dead (the
  same 5-field no-`..` cardinality check rejects any multi-arm state before compile).
  Compile recursion preserves child order.
- **ADR-0150 seam integrity:** All 6 invariants HELD with evidence — no Cargo.toml
  changed; `envoy-accesslog` gains no `envoy-config` dep; `LogFilter` derive is exactly
  `#[derive(Debug, Clone)]` (no `Eq`/`PartialEq`); no `Box` at either layer (both
  recurse through `Vec`); `AndFilter`/`OrFilter`/`AccessLogFilter` all lack `Clone`;
  `#![forbid(unsafe_code)]` intact, no `unsafe`; re-exports alphabetically sorted.
- **Fixture / fuzz coverage:** No Critical. Flagged (correctly) that fixtures compose
  only header-leaf children at depth ≤2 — the response-flag/mixed/depth-3 combinations
  are the "N-consumers, N−1 untested combinations" pattern. **Every ranked live-probe
  the reviewer requested was executed above and returned PARITY.** Fuzz seed
  `and_or_filter.yaml` is git-tracked + `!`-un-ignored, rides the existing
  `parse_bootstrap` target (ADR-0137, no new ci.yml step) — adequate.
- **Config-validation fail-loud:** No Critical/Important. Recursion terminates (finite
  acyclic owned `Vec` tree; no new stack surface beyond what serde already survived at
  parse); fail-loud complete at every level (zero-arm / multi-arm / under-2 / nested-bad
  all rejected); check ordering correct (empty `and_filter:{}` → `set_arms==1` → reaches
  the len<2 check → `InsufficientCompositeFilters{count:0}`, matching upstream's PGV
  treatment of a set-but-empty oneof); the `&mut` threads all the way down so a nested
  `header_filter`'s SafeRegex is compiled IN the stored config the runtime later clones
  (`.expect()` cannot panic); `count` correct for both arms.

---

## Findings

### Critical
None.

### Important (MUST-FIX)
None. (The two Reviewer-C "Important" coverage gaps — response-flag and mixed-leaf
composition children untested — were resolved by LIVE-PROBE groups 2 & 3 to
byte-identical parity, so they do not gate the phase.)

### Minor
- **M73-R1 — the composition surface is NARROWER than upstream (by design, not a
  regression).** Upstream's AND/OR filters compose ANY `AccessLogFilter`, including
  `duration_filter` / `runtime_filter` / `grpc_status_filter` / `metadata_filter`.
  envoy-rust models only the three built leaf arms + the two compositions, and
  `AccessLogFilter` is `deny_unknown_fields`, so `and_filter{[status_code_filter,
  duration_filter]}` is ACCEPTED by upstream but REJECTED fail-loud by envoy-rust.
  This is consistent with the project's ADR-0049 all-fatal posture and is PRE-EXISTING
  to the leaf-type coverage (each unbuilt leaf is its own future pick, SPEC §4/§2.2).
  The composition faithfully recurses over the SUPPORTED subset. Recorded so the
  next reviewer knows the AND/OR surface tracks the leaf-arm surface, not upstream's
  full oneof.
- **M73-R2 — no dedicated FIXTURE for the response-flag / mixed-leaf / depth-3
  compositions.** They are pinned by in-process tests + this session's LIVE-PROBES
  (all PARITY), but not by a committed differential fixture. Not a MUST-FIX (the
  behavior is MEASURED parity and the recursion is uniform), but a future access-log
  phase could cheaply add a mixed-leaf or depth-3 fixture to pin the parity in CI.
  Carry-forward candidate.

### Nit
- **N73-R1 — stale struct-level doc.** `crates/envoy-config/src/bootstrap.rs:714` still
  reads "This type now models **THREE** oneof arms — `status_code_filter` …
  `response_flag_filter` … and `header_filter`". As of this phase it models FIVE
  (the two composition arms were added directly below). The per-field docs and the
  `validate_access_logs` item-3 doc WERE updated; only this summary lags. Cosmetic,
  no behavioral impact — fold "THREE" → "FIVE" + the two arms at the next touch of
  the file.
- **N73-R2 — no recursion-depth guard.** `validate_access_log_filter` /
  `compile_access_log_filter` / `should_log` recurse on the native stack with no cap.
  This is the DOCUMENTED accepted non-goal CF-73-1 (parity with upstream, whose
  fixtures stay shallow); my depth-3 probe (group 4) confirms parity at practical
  depths. A pathologically deep (tens-of-thousands) finite config could stack-abort
  rather than return a clean `ConfigError`, but serde_yaml would likely hit its own
  limit first, and the config is operator-supplied/trusted. Not worth a guard;
  recorded for completeness. Owner = a future stack-safety/DoS-hardening phase.

---

## Strengths

- The single expensive item — extracting the inline per-filter validation into the
  recursive `&mut validate_access_log_filter` helper — was done cleanly and is paid
  ONCE for both arms. The no-`..` 5-field destructure preserves the M70-R1 forced-count
  discipline (a future 6th arm cannot be added without updating the helper).
- ADR-0150 held under a recursive addition that could easily have tempted an
  `envoy-accesslog`→`envoy-config` dep or an equality derive; neither happened. Both
  layers recurse through `Vec` (no `Box`), so the types stay finite-size with no
  indirection cost beyond the heap pointer already present.
- The `should_log` composition threads all three request-context slots verbatim to
  every child — no per-child argument transformation — which is exactly why the
  status / response-flag / header children all measured byte-identical parity: the
  recursion is structurally incapable of dropping a slot for a nested child while
  keeping it for a top-level one.
- Test discipline is honest: each PLAN task was RED-first, and the in-process pins
  (empty-vec boundary, nested-bad-leaf, nested-under-2 cardinality) target exactly the
  recursion seams. Fixtures follow the kept-LAST convention (ADR-0147) and the
  format-allow-list constraint (`%REQ(:PATH)%`, never `%REQ(X-A)%`, ADR-0153 PV-6).

---

## Carry-forward disposition (unchanged from ADR-0153 + this review)

- **CF-73-1** (arbitrary nesting depth, no stack guard) — OPEN; parity CONFIRMED at
  depth-3 by this review (Probe group 4). Owner = a future stack-safety phase.
- **M73-R2** (no dedicated mixed-leaf / depth-3 / response-flag composition fixture) —
  NEW; carry forward as a cheap CI-pin candidate for the next access-log phase.
- **M71-3** (all-suppressed `expected_logged_count==0` driver shape) — NOT folded
  (both `0079`/`0080` keep ≥1 line); carry forward.
- **M73-R1** (composition surface narrower than upstream) — tracks the leaf-arm
  surface; each unbuilt leaf (`duration`/`runtime`/`grpc_status`/`metadata`) is its own
  future pick.
- **CF-72-1 / CF-72-2** (`HeaderMatcher`-parity, mode-scoped) — NOT touched (a
  composition arm does not alter the shared header-match engine). Still live.
- **M71-6/7/8, M70-R4/R9, M69-A..I, CF-69-1/2/3/5, M68-1, M-1, CF-67-3/5/6/7**, the
  older Minors, and the HTTP-filters-family (1)–(4) — all untouched; carry forward.

---

## Next state

REVIEW APPROVED, no MUST-FIX → the next session is the **§5 state-6 close-out** (its
OWN session per memory `closeout-and-pick-are-separate-sessions`): flip ROADMAP row
`73` → `done`, relocate the phase-73 Notes per ADR-0035, advance STATE to
awaiting-next-planning. Do NOT chain into it. The state-6 close-out does NOT re-open
the code (the Minors/Nits above are recorded for a future touch, not this phase).
