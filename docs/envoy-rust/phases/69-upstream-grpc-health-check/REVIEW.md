# Phase 69 — §5 state-5 Code Review (`grpc_health_check`)

> **Skill:** `superpowers:requesting-code-review`. **State:** §5 state-5 (the
> INDEPENDENT-context re-review, ADR-0127 — the context that wrote the code must
> not be the sole grader). **Range reviewed:** `26c9559..2545a71` (the full
> phase-69 surface — the 12 state-3 implementation commits `dacf89c..2545a71` on
> base `26c9559`, the state-2 PLAN-write commit). **Method:** five review
> DIMENSIONS fanned out to four parallel reviewer subagents (correctness /
> test-coverage / reuse+behavior-contract-fidelity / fuzz-surface) per the
> PARALLELISM block; the main session read the whole surface independently,
> deduped + adjudicated the findings, ran a corroborating coverage grep, and is
> the sole writer of this file + the ledger. Subagents returned FINDINGS ONLY —
> no ledger/commit/push mutation.
>
> **The state-3 final whole-branch review returned Ready-to-merge (0C/0I, 2
> Minor → CF-69-5).** This state-5 re-review, with fresh context, reaches a
> DIFFERENT verdict on ONE axis (see the Assessment): a SPEC §2.1(8) /
> PLAN-Task-5-committed test on the core verdict path was left undelivered. That
> is exactly the self-grading hazard the independent-grader rule (ADR-0127)
> exists to catch.

---

## Scope of the reviewed surface

- **`crates/envoy-config`** — `GrpcHealthCheck` config schema (`service_name`,
  `authority`, `initial_metadata: Vec<HeaderValueOption>`) + the
  `grpc_health_check` field on `HealthCheck`; `validate_health_checks`
  restructured to "at most one of {http,tcp,grpc}" (`MultipleHealthCheckers`,
  replacing `BothHttpAndTcpHealthCheck`) + `GrpcHealthCheckRequiresHttp2` (the
  H2-upstream requirement) + the re-pointed pinning test.
- **`crates/envoy-http2/src/grpc.rs`** (NEW) — the hand-rolled gRPC health codec
  (`encode_health_check_request` / `decode_health_check_response` /
  `ServingStatus` / varint) + the trailers-aware unary `grpc_health_check_call`.
- **`crates/envoy-health/src/probe.rs`** — `GrpcProbeError` + `grpc_probe_once` +
  `grpc_probe_loop`; the M68-2 fold (`TcpProbeError::Send` → new `::Read` for the
  read-error path).
- **`crates/envoy-health/src/scheduler.rs`** — the 3-tuple checker dispatch +
  `grpc_cfg` extraction.
- **`tests/differential/src/lib.rs`** — `Driver::Http2AfterSettle` +
  `run_http2_after_settle_arm`; **fixture `0075`** + its per-fixture test.
- **`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** — the gRPC-HC section.
- **Fuzz** — the `grpc_health_decode` target (new `envoy-http2/fuzz` subcrate) +
  the `parse_bootstrap` `grpc_health_check_seed`; `ci.yml` wiring.

---

## Strengths

- **The gRPC codec is genuinely robust.** Every arithmetic/indexing site in
  `decode_health_check_response` was independently re-derived (main session + the
  correctness reviewer): `i ≤ body.len()` is invariant at every loop iteration,
  the `checked_add` guard on the attacker-controlled wire-type-2 length is
  correct, the varint `shift` maxes at 63 (byte index 10 returns `None` first) so
  the `<< shift` can never hit the ≥64 shift-overflow panic, and unknown fields
  are skipped without touching `status`. The `grpc_health_decode` fuzz target
  reaches every decode branch and ran clean over 79.4M execs (state-4). The
  overflow was found + fixed in-phase by the phase's own smoke-run — the fuzz
  investment paid for itself.
- **No false-Healthy is reachable.** An empty/absent-status body decodes to
  `Unknown` (not `Serving`); a missing/unparseable `grpc-status` trailer →
  `MissingTrailer` (never defaulted to `0`); a non-200 `:status` → `BadResponse`;
  a trailers-only response → `MissingTrailer`. `Ok(Serving)` requires
  `grpc-status == 0` **AND** a SERVING message body — the two-factor verdict is
  correct and matches the MEASURED Envoy contract.
- **The trailers-alive design is correct** — DATA is fully drained
  (`while recv_stream.data().await`) releasing flow-control capacity, *then*
  `recv_stream.trailers().await`; this is the single genuinely-new primitive over
  the existing client (which drops `recv_stream` before trailers), and it is done
  right.
- **Clean reuse.** The 3-tuple scheduler dispatch generalizes the phase-68
  2-tuple with the HTTP/TCP arms byte-identical (just a widened tuple + trailing
  `None`); `grpc_probe_*` mirrors `tcp_probe_*`; `initial_metadata` reuses the
  existing `HeaderValueOption`; no dead code (every `ServingStatus` /
  `GrpcDecodeError` / `GrpcCallError` / `GrpcProbeError` variant is constructed).
- **The validator is precise and regression-free.** "At most one of three" via
  an `is_some()` count, evaluated *before* the H2 gate; the H2 predicate matches
  the codebase's only modeled representation of upstream H2; HTTP-only / TCP-only
  configs (`n_set == 1`, not grpc) bypass the H2 gate → no phase-68 regression.
- **The behavior contract is faithful** — every load-bearing claim (H2
  requirement, `{}`⇒overall-server, the two-factor verdict, no `network_failure`,
  the oneof, the shared stat tree, no `grpc-timeout` header, whole-probe timeout,
  `initial_metadata` accepted-but-ignored) was cross-checked against the code and
  holds.
- **Fuzz wiring is complete and correct** — the `ci.yml` step (`working-directory:
  crates/envoy-http2`, matching the 4 sibling targets), the job-name update, the
  rust-cache path, the root-workspace `exclude`, and BOTH un-ignored seeds are all
  present and `git ls-files`-tracked (the ~110 untracked local fuzz artifacts vs
  the single tracked `serving_seed` prove the `*`-ignore + `!`-un-ignore pair
  works).

---

## Issues

### Critical (Must Fix)

None. Both the main-session read and the dedicated correctness reviewer cleared
every decode-arithmetic site, every false-Healthy path, the timeout scoping, the
scheduler dispatch, and the validator. There is **no live bug** in the phase-69
code.

### Important (Should Fix before merge)

**I-1 — The SPEC §2.1(8) / PLAN-Task-5-committed probe-layer verdict test
(`SERVING → healthy`, `NOT_SERVING → failure`) is undelivered; the whole-probe
timeout path is also untested — an asymmetry with the `tcp_probe_once` twin.**

- **Files:** `crates/envoy-health/src/probe.rs` — `grpc_probe_once`
  (`:302`-`328`, verdict arms `:312`-`322`; timeout wrap `:324`-`327`). The sole
  `grpc_probe_once` test is `grpc_probe_connect_refused_is_err` (`:506`), which
  exits via the `Connect` arm and never reaches the `Ok(Serving)⇒Ok(())` /
  `Ok(_other)⇒Err(NotServing)` mapping or the timeout branch (grep-confirmed: no
  `grpc_probe_serving*`, no `grpc_probe_*timeout`, no `grpc_probe_*ok` test
  exists anywhere in the tree).
- **What's wrong / why it matters.** SPEC §2.1 item **(8)** committed to
  in-process coverage of "**the SERVING → healthy path and the NOT_SERVING →
  failure path**", and PLAN Task 5 Step 1 concretely sketched a
  `grpc_probe_serving_is_ok` test. Neither landed. The `grpc.rs` loopback tests
  (`call_serving_verdict`, `call_not_serving_still_ok_grpc_status`) prove
  `grpc_health_check_call` returns the correct *`ServingStatus`* — i.e. the
  **wire→status decode** hop. They do **not** exercise the **status→verdict** hop
  (`grpc_probe_once`'s `Serving⇒Ok` / `else⇒Err` mapping), which is the arrow
  that turns a probe result into *keep-vs-eject*. So the "SERVING → healthy /
  NOT_SERVING → failure PATH" the SPEC named is nowhere pinned end-to-end. A
  mutation flipping those two arms — treating `NOT_SERVING`/`UNKNOWN` as *healthy*
  — passes the entire suite green. This is the feature's core safety property
  (an active health check exists precisely to eject non-SERVING endpoints), and
  the gRPC probe is **asymmetric with its own TCP twin**, which *does* pin both
  its Ok-path (`tcp_probe_connection_only_healthy`, `tcp_probe_receive_match_healthy`)
  and its timeout-path (`tcp_probe_receive_mismatch_times_out`).
- **The code is correct today** (verified independently, twice) — this is a
  committed-coverage gap, not a live defect; hence Important, not Critical. It is
  the escalation of the state-3-accepted CF-69-4 ("indirect coverage, future
  `test-util` feature"): an independent re-read rejects "indirectly covered" as
  an accurate reading of the SPEC's "SERVING → healthy path … end-to-end", since
  the `→ healthy` link itself has zero coverage.
- **How to fix (tightly scoped — a focused test-addition, NOT a redesign):** at
  the `grpc_probe_once` layer, add (a) a `grpc_probe_serving_is_ok` — a loopback
  `h2::server` returning `08 01` + `grpc-status:0` (reuse the `grpc.rs`
  `call_serving_verdict` server body) asserting `grpc_probe_once(...).is_ok()`;
  (b) a `grpc_probe_not_serving_is_err` — the same server returning `08 02`
  asserting `matches!(err, GrpcProbeError::NotServing)`; and (c) a
  `grpc_probe_hang_times_out` — an H2 backend that completes the handshake but
  never responds, asserting `Err(GrpcProbeError::Timeout)` under a short
  `probe_timeout` (the gRPC analogue of `tcp_probe_receive_mismatch_times_out`).
  Per §5.2 this is a SEPARATE state-3 re-entry session — do **not** fix here.

### Minor (Nice to have — carry-forwards, do NOT block merge)

- **M69-A — `BEHAVIOR_CONTRACT.md` omits the empty-`authority` default.** The
  scheduler defaults an empty `authority` to the cluster name (`scheduler.rs:99`-`106`,
  mirroring the HTTP checker's `host` default), but the contract's "Checker
  shape" bullet only says `authority` "overrides the probe's `:authority`". Add
  "empty `authority` ⇒ the cluster name."
- **M69-B — `BEHAVIOR_CONTRACT.md` `content-type` asymmetry.** The contract lists
  `content-type: application/grpc` as a response fact, but `grpc_health_check_call`
  validates only `:status 200` + the `grpc-status` trailer (the response
  `content-type` is never inspected — `grpc.rs:203`-`205`). Not a contradiction;
  add a half-sentence noting the response `content-type` is not a validated
  precondition. (This is CF-69-5's "content-type not validated" leg — the OUTCOME
  is correct: a non-grpc body → decode-error → failure.)
- **M69-C — `grpc_health_decode.rs` fuzz target omits `#![forbid(unsafe_code)]`,**
  unlike the 4 sibling targets (e.g. `parse_bootstrap.rs`). Cosmetic (the body
  has no `unsafe`); add for consistency with the crate-root forbid convention.
- **M69-D — single committed fuzz seed** (`serving_seed` only). libFuzzer
  coverage-guidance discovers the rest, but 2-3 un-ignored edge seeds
  (NotServing `…08 02`, a compressed `01 …`, a `<5`-byte truncation) would make
  the 30s CI budget deterministic rather than luck-of-the-draw.
- **M69-E — `MultipleHealthCheckers` tested only for http+tcp and http+grpc,**
  never tcp+grpc or all-three. The count logic is generic (low risk); one
  `tcp_health_check + grpc_health_check` case would fully close the oneof.
- **M69-F — the scheduler grpc test asserts only `attempt >= 1`,** not the
  `failure` tick / ejection; and the `grpc_cfg` authority-default (`scheduler.rs:100`-`104`)
  has no direct test. (Folds naturally into the I-1 re-entry.)
- **M69-G — stale doc framing.** `scheduler.rs:1`-`6` (module doc) and
  `bootstrap.rs:~2424` (the `HealthCheck` struct doc) still read "HTTP-only" /
  phase-12 framing. No behavioral impact.
- **Reuse-duplication (documented KEEPs, not new findings):** the probe-loop body
  is now triplicated (`probe_loop`/`tcp_probe_loop`/`grpc_probe_loop`) and
  `run_http2_after_settle_arm` is a ~50-line verbatim clone of its H1 twin
  (CF-69-3). Both are defensible against the repo's established per-protocol-twin
  convention; flagged only so a future "collapse into a generic loop / a shared
  `assert_after_settle_equivalence` helper" is on record if a THIRD twin ever
  lands.

---

## Carry-forward reconciliation (per the state-5 brief)

Each live CF-69-x was re-graded for correct classification:

- **CF-69-1 (fixture `0075` OMITS the response-header axis)** — **ACCEPTABLE
  documented boundary, NOT a finding.** The synth-503 header set is produced by
  the *pre-existing* H2 no-healthy-upstream path (the same observable as the
  TCP-HC fixture `0074`); this phase does not touch 503 header emission, so the
  omission hides no gRPC-introduced divergence — it only forgoes catching an
  unrelated pre-existing H2-503 header gap, which is out of scope. Stays live.
- **CF-69-2 (`health_check.network_failure` not modeled for ANY checker)** —
  **CORRECT deliberate divergence (ADR-0139), NOT a bug.** The gRPC probe ticks
  only `attempt`/`success`/`failure`, symmetric with HTTP/TCP; the
  transport-vs-app distinction is neither emitted nor differentially asserted.
  Confirmed — not filed.
- **CF-69-3 (verbatim `run_http2_after_settle_arm` clone)** — **reasonable KEEP**
  per the per-protocol-twin convention; logged as M69 reuse-note.
- **CF-69-4 (`grpc_probe_once` verdict-mapping direct-coverage)** — **ESCALATED
  → I-1 (Important).** At state-3 this was accepted as a Minor "future `test-util`
  feature"; the independent re-review finds it is a SPEC §2.1(8)-committed test
  on the core verdict path that was dropped, not optional polish. This is the
  one disposition where state-5 overturns the state-3 grade.
- **CF-69-5 (`grpc_health_check_call` cosmetic classification — trailers-only →
  `MissingTrailer`; `content-type` not validated pre-decode)** — **both OUTCOMES
  correct** (the correctness reviewer confirmed the trailers-only *verdict*
  matches Envoy: a trailers-only response carries no SERVING message, so it is
  unhealthy regardless of grpc-status; a non-grpc body → decode-error →
  failure). Kept as doc-note Minors M69-B; no verdict divergence.

---

## Untested-composition analysis (per memory `state5-must-probe-untested-compositions`)

The phase adds a THIRD health-check handler (gRPC) over the shared scheduler /
`EndpointHealth` / ejection machinery. Compositions:

- **Config compositions** (validator, "exactly one checker"): http-only,
  tcp-only, grpc-only, and the rejected {≥2, none, grpc-on-non-H2}. All directly
  tested except the tcp+grpc oneof pair (M69-E, low-risk).
- **Runtime verdict paths** (the point of the memory): the connect-refuse →
  failure → eject → synth-503 path IS proven differentially (fixture `0075`
  GREEN) and at the scheduler-spawn layer; the wire→status decode of SERVING /
  NOT_SERVING IS proven at the call layer against a real `h2::server`. The gap is
  the **status→verdict** hop at the probe layer (I-1) — I did NOT accept "it's
  probably parity": I verified the code is correct AND grep-confirmed no test
  pins it, and traced that the SPEC named it as a deliverable. The escalation is
  measured, not assumed.
- The SERVING/NOT_SERVING paths being witnessed **in-process rather than
  differentially** is an SPEC §2.2-scoped boundary (the gRPC-backend differential
  helper was explicitly deferred, ADR-0139, the phase-68 precedent) — that
  boundary itself is acceptable; what is NOT acceptable is that within the chosen
  in-process scope, the committed verdict-path test was skipped.

---

## Recommendations

- **State-3 re-entry (I-1):** add the three `grpc_probe_once` tests above; fold in
  M69-F (assert the scheduler `failure` tick) opportunistically. Optionally sweep
  the cheap Minors (M69-A/B doc lines, M69-C forbid-unsafe, M69-E tcp+grpc oneof)
  in the same session since the surface is already open — but I-1 is the only
  merge-blocker.
- Leave the reuse-duplication KEEPs (CF-69-3, triplicated loop) as-is until a
  third twin justifies the shared helper.

---

## Assessment

**Ready to merge?** **No — merge with fixes.** Per §5.2 the phase RE-ENTERS §5
state-3 (a SEPARATE session) to close I-1, then re-runs state-4 verification and
this state-5 review.

**Reasoning:** The phase-69 code is correct — two independent reads found no live
bug, the codec is fuzz-hardened, no false-Healthy is reachable, and the
connect-refuse ejection path is differentially GREEN. But SPEC §2.1(8) and PLAN
Task 5 committed to a probe-layer `SERVING → healthy` / `NOT_SERVING → failure`
test that was silently dropped and relabeled "indirect coverage" (CF-69-4),
leaving the feature's core eject-vs-keep verdict arm and its whole-probe timeout
unpinned and asymmetric with the TCP twin. That is a plan-alignment miss on a
safety-critical path with a cheap, well-scoped fix (three loopback tests) — the
kind of self-graded deferral the independent-review rule exists to reverse. All
other findings are Minor carry-forwards. **DECISIONS.md ledger head: ADR-0139
(no new ADR this session).**
