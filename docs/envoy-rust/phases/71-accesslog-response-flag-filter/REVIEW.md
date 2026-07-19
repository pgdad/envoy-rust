# Phase 71 — `response_flag_filter` — REVIEW (§5 state-5 code-review)

> **Verdict: APPROVED** — 0 Blocker / 0 Major / 8 Minor (2 of which were
> DISCHARGED within this review by probe-derived test additions; 1 resolved by
> **ADR-0147**). Gate §7.5(f) is MET; the phase advances to the state-6
> close-out. No §5.2 state-3 re-entry.
>
> Written by the §5 state-5 code-review session (`superpowers:requesting-code-review`),
> a SEPARATE context from the implementing sessions (ADR-0127). Self-contained
> per D-3.4.

## §1. What was reviewed

- **Range:** `9b27d44` (phase-70 close) → `7559fff` (the phase-71 code head:
  T1–T11 + fmt + the ADR-0146 CF-70-3 driver correction). The state-4
  verification commit `a68fff0` above it is docs-only.
- **Against:** `SPEC.md` (state-1 scope), `PLAN.md` (the 12 TDD tasks),
  `BEHAVIOR_CONTRACT.md` (the phase-71 `response_flag_filter` subsection),
  ADR-0144/0145/0146, and the standing invariants.
- **Method:** three parallel READ-ONLY review dimensions (config
  schema/validation; runtime predicate + HCM emit gates; differential
  driver/fixture/fuzz/docs) fanned out to subagents, while THIS session ran the
  decisive live probes and mutation measurements itself (memory
  `state5-must-probe-untested-compositions`: MEASURE, don't hypothesize). All
  subagent HYPOTHESES were subsequently measured — none was left adjudicated on
  plausibility.

## §2. Measurements (all against live `envoyproxy/envoy:v1.33.0` and/or the built `target/debug/envoy-bin`)

**P1 — H1 multi-sink composition (UNTESTED in-tree; live cross-proxy probe).**
One H1 HCM, FIVE sinks on the same `access_log` list — `flags:["NR"]`,
`flags:["UH"]`, `status_code_filter GE 500`, `response_flag_filter: {}` (empty),
no-filter — driven with the same requests (`/direct` → 503 `-`,
`/nowhere` → 404 `NR`) on BOTH proxies (upstream flushed via graceful stop).
Result: **byte-identical per-sink files across proxies** — `["NR"]` kept only
the 404 `NR`; `["UH"]` empty; `GE 500` kept only the 503s; the EMPTY filter kept
only the flagged 404 (PV-6 semantics now live-measured on envoy-rust, not just
upstream + in-process); no-filter kept everything in the same order. This also
live-witnesses `response_flag_filter` and `status_code_filter` coexisting on
DIFFERENT sinks of one list (legal, unlike both arms in one `filter`), and
per-sink independence (§A of the phase-70 contract) across the new arm.

**P2 — H2 emit gate under `response_flag_filter` (UNTESTED differentially;
live cross-proxy probe).** The same five-sink config with `codec_type: HTTP2`
(+ `http2_protocol_options: {}`), driven with `curl --http2-prior-knowledge` on
both proxies. Result: **per-record dispositions identical on both proxies** for
every sink (every 404 `NR` kept by `["NR"]`/empty, dropped by `["UH"]`; every
503 `-` kept only by `GE 500`; no-filter logs all). The H2 widened gate (T6) is
live-confirmed parity-correct, not just in-process-correct.

**P3 — config-acceptance parity probes (`--mode validate`, networking-free).**
Upstream REJECTS `flags: [""]` and `flags: [" NR"]` (both via the PGV `in`-list
message) — envoy-rust's exact-match `UnknownResponseFlag` rejection is the SAME
class: parity CONFIRMED (two reviewer hypotheses resolved). Upstream ACCEPTS
`flags: ["NR", "NR", "UF"]` (duplicates + multiple; re-confirms R-0.2), and
envoy-rust BOOTS the same config (live) — parity confirmed, then PINNED by a
new test (§4, D-1).

**P4 — upstream FileAccessLog flush timing (the ADR-0146 soundness premise).**
Two back-to-back requests against an unfiltered upstream file sink, polling the
file per second: line 1 at **t≈0.012s**, line 2 at **t≈10.05s**. Upstream does
NOT put back-to-back records in one flush — ADR-0146's "same flush" rationale is
measured-false. Adjudication: **ADR-0147** (fired this session) re-scopes the
CF-70-3 closure — sound for the SUBJECT side on all orderings and for the
REFERENCE side on kept-last orderings (0077); the residual reference-side window
on dropped-last shapes (0076) bites only at fixture-authoring / pin-refresh
time. No code change owed; CF-71-1 carries the optional ordering-aware settle
hardening.

**M1/M2/M3 — mutation measurements (scratch worktree, forced rebuilds
verified via `Compiling` lines, mutations re-grepped in place).**
- **M1** (H1 emit gate passes `"-"` instead of `&record.response_flags`,
  `envoy-http1/src/hcm.rs:1512`): the ENTIRE pre-existing H1 in-process suite
  stayed GREEN (178 passed) — the H1 threading had NO in-process witness; the
  Docker differential `0077` DID catch it (`envoy-rust emitted 0 access-log
  lines but 1 were expected`). Both halves measured. → finding D-2, closed by a
  new in-process test whose non-vacuity was itself mutation-verified (the new
  test FAILS under M1: `got []`).
- **M2** (same mutation at the H2 gate `envoy-http2/src/hcm.rs:1135`): caught by
  `h2_response_flag_filter_suppresses_no_flag` (1 failed) — the T6 end-to-end
  test is non-vacuous.
- **M3** (empty-`flags` branch flipped to `true` in
  `envoy-accesslog/src/filter.rs`): caught by
  `response_flag_empty_matches_any_flag_set` (1 failed) — the PV-6 pin is
  non-vacuous.

## §3. Dimension results (subagent fan-out, all claims re-verified or measured)

- **Config schema/validation:** `RESPONSE_FLAG_TOKENS` is exactly the measured
  29 tokens, order included; cardinality checked before token membership (a
  both-arms+bogus config yields `AmbiguousAccessLogFilter`); the M70-R1
  destructuring genuinely omits `..`; serde attrs, re-export, error convention,
  grow-only `ConfigError` all conform; NO access-log config surface bypasses
  `validate_access_logs` (HCM is the sole `AccessLog` carrier; `TcpProxyConfig`
  has none; non-file sink names are rejected before the filter check).
- **Runtime/HCM:** both emit gates thread the record's REAL token, derived
  before the gate on both codecs; the StatusCode arm and the `(Some, None)`
  compile arm are byte-unchanged from phase 70; the `unreachable!` in
  `compile_access_log_filter` is genuinely validator-backed; `access_logs_total`
  stays per-sink intent-to-emit inside the gate on both loops; NO pre-existing
  assertion was weakened by the mechanical 2nd-arg updates (every `"-"`-passing
  site exercises the StatusCode arm or a filterless sink, both of which must
  ignore the argument — and that ignoring is itself pinned).
- **Driver/fixture/fuzz/docs:** no residual ordering-witness assert; the settle
  is correctly gated, path-correct, and pre-shutdown in BOTH arms; fixture 0077
  pairs byte-for-byte on the 0076 pairing conventions; `expectations.yaml`
  matches the driver serde; the README correctly documents the ADR-0146
  retirement; NO pre-existing fixture changed; `known-failures.txt` untouched;
  the fuzz seed is tracked and valid; ROADMAP row 71 is well-formed (6 cells, no
  unescaped `|`); the BEHAVIOR_CONTRACT subsection matches the code
  token-for-token; `#![forbid(unsafe_code)]` holds at every touched crate root.

## §4. Findings

**Discharged WITHIN this review** (probe-derived test additions, permitted in
the state-5 commit; both landed fmt/clippy-clean, crates re-run green —
`envoy-config` 617 passed, `envoy-http1` 179 passed):

- **D-1 (was Major-candidate, config dimension):** the MEASURED load-parity
  facts "duplicate tokens accepted" and "multiple tokens accepted" were pinned
  by NO test — a future dedup/uniqueness "improvement" would regress load
  parity with no test failing (loudly at boot, hence not Major once measured).
  → Measured on both proxies (P3), then pinned by
  `accepts_response_flag_filter_duplicate_and_multi_tokens`
  (`crates/envoy-config/src/bootstrap.rs`).
- **D-2 (was Major-candidate, runtime dimension + M1):** the H1 emit-loop
  threading of `&record.response_flags` had NO in-process witness (H2 had one;
  H1 was pinned only by the host-flake-prone Docker differential, and the
  `--lib --bins` dry-run class that let the T7 regression escape locally would
  never see it). → Closed by `h1_response_flag_sink_gates_emit_loop_end_to_end`
  (`crates/envoy-http1/src/hcm.rs`; the RF builder's route narrowed to a fixed
  `path: /routed` so a no-route `NR` is drivable), non-vacuity proven under M1.

**Resolved by ADR (no code owed):**

- **F-1 → ADR-0147:** ADR-0146's "settle is sound for BOTH probe orderings"
  claim is measured-false for the reference side of dropped-last fixtures (P4).
  Re-scoped by ADR-0147; binding authoring convention recorded (kept-last
  ordering + graceful-stop verification of the reference's dropped half);
  hardening carried as **CF-71-1**.

**Minor (carry-forwards, owner = the next phase touching each surface):**

- **M71-1:** `rejects_access_log_filter_with_both_arms` matches
  `AmbiguousAccessLogFilter { .. }` without asserting `detail`, so it cannot
  distinguish the zero-arm from the both-arm branch; and no test pins the
  cardinality-before-token-check precedence (both-arms + bogus token).
- **M71-2:** stale "ordering witness" doc phrasing survives the ADR-0146
  retirement in three places — the `CF70_3_SETTLE` doc comment
  (`tests/differential/src/lib.rs:1677-1680`, which still calls the retired
  assert "the primary soundness guarantee"), the 0077 integration-test doc
  comment, and the `BEHAVIOR_CONTRACT.md` §F "as the CF-70-3 ordering witness"
  phrase. Fold into CF-71-1's owner.
- **M71-3:** the all-suppressed shape (`expected_logged_count == 0`) is legal
  per the driver types but untested; `wait_file_lines(path, 0)` returns
  immediately and the reference-side settle is near-vacuous there (same
  mechanism as F-1). No such fixture exists today.
- **M71-4:** the validator docstring's rejection inventory
  (`bootstrap.rs:5109` region) predates phase 71 — it omits
  `UnknownResponseFlag` and still calls the `> 1` branch unreachable.
- **M71-5:** no in-tree TEST pins the multi-sink mixed-filter composition
  (P1/P2 measured it live-green on both codecs, but nothing prevents
  regression); the in-process helpers all build single-sink configs.
- **M71-6:** the H2 differential for `response_flag_filter` remains deferred
  (SPEC §2.2's explicit disposition) — consequently the H2 arm's
  `has_suppression` settle path has never executed against real proxies (P2
  covered the H2 gate live, but outside the harness).
- **M71-7 (latent):** the membership test is exact-string over the SINGLE
  rendered token and the empty-`flags` branch is `!= "-"`. Both are correct
  while records are single-token by construction (they are, today); the first
  future phase that renders multi-flag records (upstream comma-joins) or an
  empty-string flag breaks parity silently. The same future phase must also
  resolve the pre-existing both-booleans question (envoy-rust renders `URX`
  where upstream may render `UF,URX` — surfaced by, not created by, phase 71).
- **M71-8:** upstream rejection parity for `""` / whitespace-wrapped tokens is
  measured (P3) but unpinned by tests — cheap to fold into any future
  validator-touching task.

## §5. Obligations check

- **CF-70-1** (zero-arm `expect()` → 2-arm match): DISCHARGED — verified at
  `envoy-http1/src/hcm.rs:1741` region, `(Some,None)` arm byte-equivalent.
- **M70-R1** (`set_arms` destructuring, `> 1` reachable): DISCHARGED — no `..`,
  both-arm rejection test present and mutation-verified at state-3.
- **CF-70-3**: CLOSED for the subject-under-test on all orderings and the
  reference on kept-last orderings; re-scoped by ADR-0147 (residual → CF-71-1).
- **M70-R2** (`expected_logged_count` witness): FOLDED —
  `expected_logged_count_counts_only_kept` present.
- NOT consumed (still live, correctly untouched): M70-R4/R9, M69-A..I,
  CF-69-1/2/3/5, M68-1, M-1, CF-67-3/5/6/7, the older Minors + the
  HTTP-filters-family (1)–(4).

## §6. Verdict

**APPROVED.** The implementation is faithful to SPEC/PLAN/CONTRACT on every
measured axis; the untested compositions (multi-sink, mixed arms across sinks,
empty-`flags` on the subject, the H2 gate, duplicate tokens) were LIVE-MEASURED
cross-proxy this session and all showed exact parity; the two decisive coverage
holes were closed in-review with mutation-verified tests; the one soundness
overclaim is corrected by ADR-0147 with no code owed. §7.5(f) is MET — with
(a)–(e) already PASSED at state-4, the phase-done gate is complete. The next
session is the **§5 state-6 close-out** (flip ROADMAP row `71` → `done`,
relocate the Notes subsection, STATE → awaiting next planning).

**New carry-forwards from this review:** CF-71-1 (ordering-aware settle
hardening + M71-2's doc corrections; owner = the next differential-driver /
access-log-filter phase) and M71-1/3/4/5/6/7/8 above.
