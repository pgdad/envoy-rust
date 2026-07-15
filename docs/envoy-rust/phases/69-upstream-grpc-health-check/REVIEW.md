# Phase 69 — §5 state-5 RE-review (`grpc_health_check`)

> **Skill:** `superpowers:requesting-code-review`. **State:** §5 state-5
> RE-review (the INDEPENDENT-context re-review, ADR-0127 — the context that wrote
> the code, and the context that graded it before, must not be the sole grader).
> **This file SUPERSEDES the prior state-5 `REVIEW.md` per D-3.5** (a review is
> superseded only by a later review). **Range re-reviewed:** the full phase-69
> surface `26c9559..2545a71` (the state-2 PLAN-write base `26c9559` + the 12
> state-3 implementation commits `dacf89c..2545a71`) **PLUS the §5.2 state-3
> re-entry `e0c6885`** (the I-1 fix). **Method:** two independent reviewer
> subagents fanned out with zero session context per the PARALLELISM block — one
> grading the **I-1 fix adequacy / non-vacuity**, one giving a **fresh
> adversarial correctness read** of the codec + probe + scheduler + validator;
> the main session independently re-read the whole surface (`grpc.rs`,
> `probe.rs`, the I-1 commit diff, the validator), reconciled + adjudicated the
> findings, and is the sole writer of this file + the ledger. Subagents returned
> FINDINGS ONLY — no ledger/commit/push mutation.
>
> **Prior state-5 verdict:** MERGE WITH FIXES (0 Critical / 1 Important **I-1** /
> 7 Minor). The §5.2 state-3 re-entry (`e0c6885`) discharged I-1 under TDD; the
> §5 state-4 re-verification re-ran the full §7.5 gate GREEN over the whole tree.
> **This re-review's verdict: APPROVE / MERGE** — the merge-blocker is closed and
> re-verified; the surviving Minors are non-blocking carry-forwards.

---

## What changed since the prior state-5 review

The phase-69 **product tree is byte-identical to the prior state-5 head
`2545a71`.** `git diff --stat 2545a71 37483a7 -- 'crates/**/*.rs'` reports a
single file changed — `crates/envoy-health/src/probe.rs`, **+82 lines, every one
inside `#[cfg(test)] mod tests`** (hunk `@@ -518,6 +518,88 @@ mod tests`). The
I-1 fix commit `e0c6885` touches only that test module, the `envoy-health`
`[dev-dependencies]` (`h2`/`http`/`bytes`, test-only), and `Cargo.lock`. **No
product logic was changed** — `grpc_probe_once`, the `grpc.rs` codec, the
scheduler dispatch, and the validator are the exact bytes the prior two
independent reads cleared. This re-review therefore confirms the FIX and
re-confirms the surface with fresh eyes (ADR-0127); it does not re-litigate a
changed feature.

---

## I-1 — CONFIRMED discharged (REAL, ADEQUATE, NON-VACUOUS)

**I-1 was:** SPEC §2.1(8) / PLAN Task 5 committed a probe-layer verdict test
(`SERVING → healthy` / `NOT_SERVING → failure`); it was dropped (relabeled
"indirect coverage", CF-69-4). The `grpc_probe_once` status→verdict arms
(`probe.rs:313`-`314`) and the whole-probe `timeout(...)` wrap (`:324`-`327`)
were exercised only by the `Connect` arm — a mutation flipping the two verdict
arms passed the entire suite. This is the feature's core eject-vs-keep safety
property.

**The fix (`e0c6885`) adds three `grpc_probe_once` tests** driving the real
function end-to-end against a loopback `h2::server` (helper
`spawn_grpc_verdict_server`, `probe.rs:527`, mirroring the `grpc.rs`
`call_serving_verdict` body):

- **`grpc_probe_serving_is_ok`** (`:558`) — server DATA `08 01` +
  `grpc-status:0` ⇒ `ServingStatus::Serving` ⇒ arm `:313 Ok(Serving)⇒Ok(())` ⇒
  asserts `.is_ok()`. Pins the healthy arm.
- **`grpc_probe_not_serving_is_err`** (`:568`) — server DATA `08 02` +
  `grpc-status:0` ⇒ `ServingStatus::NotServing` ⇒ arm `:314 Ok(_other)⇒Err(NotServing)`
  ⇒ asserts `matches!(Err(NotServing))`. Pins the **eject-vs-keep** arm.
- **`grpc_probe_hang_times_out`** (`:579`) — H2 backend that handshakes +
  accepts the request stream but never responds ⇒ the 300ms `probe_timeout`
  elapses ⇒ asserts `Err(Timeout)`. Pins the whole-probe `timeout` wrap
  (`:324`-`327`), the gRPC analogue of `tcp_probe_receive_mismatch_times_out`.

**Non-vacuity — verified three ways** (main-session trace + both reviewer
subagents, and the re-entry's own recorded RED-first): each test drives a
genuine gRPC-framed response (`00 00 00 00 02 08 <status>` — a valid
length-prefixed `HealthCheckResponse`, so the decode path is exercised, not
stubbed) and reaches the exact arm it claims. A mutation that **swaps** the two
verdict arms makes `grpc_probe_serving_is_ok` and `grpc_probe_not_serving_is_err`
FAIL (Ok↔Err inverts); **removing/widening** the timeout wrap makes
`grpc_probe_hang_times_out` FAIL (at 300ms nothing fires; the held stream later
drops → `Err(Rpc)`, not `Err(Timeout)`). PROGRESS.md `## §5.2 state-3 re-entry`
records the RED-first demonstration verbatim: with the two arms swapped + the
wrapper widened to `Duration::from_secs(30)`, all three failed for the exact
right reasons (`expected Ok, got Err(NotServing)` / `got Ok(())` /
`got Err(Rpc(...broken pipe))`), then the mutation was reverted → all GREEN.
Confirmed passing now in isolation (`cargo test -p envoy-health --lib grpc_probe_`
→ 5 passed) and in the full workspace (state-4 re-verification).

**Adequacy:** the three tests close exactly the I-1 gap — the two verdict arms
`:313`/`:314` and the timeout wrap. The gRPC probe is now **symmetric with its
TCP twin** (`tcp_probe_*` pins its Ok-path, its receive-match path, and its
timeout path). I-1 is discharged; CF-69-4 is CONSUMED.

---

## Fresh correctness re-read of the surface (ADR-0127) — no new defect

Both reviewer subagents and the main-session read independently re-cleared the
load-bearing correctness properties on the byte-identical product tree:

- **Decode safety (`decode_health_check_response`, `grpc.rs:53`-`110`;
  `read_varint`, `:127`-`141`) — no panic-reachable site.** All frame indexing is
  guarded by `len < 5`; body slicing `&body[i..]` is safe under the
  `while i < body.len()` invariant (a read at `i == body.len()` yields an empty
  slice → `read_varint` returns `None` → `BadVarint`, no panic). The one real
  overflow site — the attacker-controlled wire-type-2 length `i + l` — uses
  `checked_add` + `next <= body.len()` (`:89`-`92`) and is pinned by the
  fuzz-regression test `decode_rejects_huge_length_delimited_field_without_overflow_panic`.
  `read_varint`'s `i >= 10` guard returns `None` before `shift` (max 63 at i=9)
  is ever used past 63, so `<< shift` never hits the ≥64 shift-overflow panic.
  The `grpc_health_decode` fuzz target ran clean over 86M execs (state-4
  re-verification).
- **No false-Healthy is reachable.** `grpc_health_check_call` returns `Ok(status)`
  only after ALL of: `:status == 200` (else `BadResponse`), a present trailer
  block (else `MissingTrailer`), a parseable `grpc-status` trailer (else
  `MissingTrailer`), and `grpc-status == 0` (else `GrpcStatus`); then
  `grpc_probe_once` maps ONLY `Ok(Serving) ⇒ Ok(())`. Every risky path is
  conservative-failure: empty/trailers-only body → `ShortFrame`/`Decode`; absent
  status field → `from_u64(0)=Unknown` → `NotServing` error; a `grpc-status:0`
  placed in the initial HEADERS (no trailer frame) → `recv_stream.trailers()`
  is `None` → `MissingTrailer`. The verdict correctly requires BOTH
  `grpc-status == 0` AND a SERVING message body.
- **Scheduler dispatch (`scheduler.rs`) — correct, regression-free.** The 3-tuple
  `(http_cfg, tcp_cfg, grpc_cfg)` match leaves the HTTP `(Some,None,None)` and
  TCP `(None,Some,None)` arms behaviorally unchanged; the new gRPC
  `(None,None,Some)` arm is reachable; the `unreachable!()` catch-all is sound
  because `validate_health_checks` rejects >1 checker upstream. The empty-`authority`
  → cluster-name default is correct.
- **Validator (`validate_health_checks`, `bootstrap.rs:4783`) — correct,
  regression-free.** `n_set > 1 → MultipleHealthCheckers`, `n_set == 0 →
  UnsupportedHealthCheckType`; HTTP-only / TCP-only (`n_set == 1`) skip the H2
  gate and are NOT newly rejected. The `GrpcHealthCheckRequiresHttp2` gate
  inspects `explicit_http_config.http2_protocol_options`; the reimplementation
  exposes no other H2-config path (`ExplicitHttpConfig` is a strict http1/http2
  oneof, no ALPN/`auto_http_config`), so no valid H2 cluster is falsely rejected
  and no non-H2 cluster slips through. `BothHttpAndTcpHealthCheck` survives only
  as a doc-comment reference (grep-confirmed no live variant).

**No Critical, no Important.** Three independent reads (this session's two
subagents + main read) plus the two prior reads (state-3 whole-branch + state-5)
found no live bug.

---

## Minor findings — carry-forwards (do NOT block merge)

The prior state-5 Minors **M69-A..G were NOT swept** (the §5.2 re-entry was
deliberately scoped to the merge-blocker only, one indivisible unit); they remain
live. Re-confirmed still-accurate:

- **M69-A** — `BEHAVIOR_CONTRACT.md` omits the empty-`authority` ⇒ cluster-name
  default (`scheduler.rs:100`-`104`).
- **M69-B** — `BEHAVIOR_CONTRACT.md` `content-type` asymmetry: the contract lists
  `content-type: application/grpc` as a response fact, but `grpc_health_check_call`
  validates only `:status 200` + `grpc-status` (the response `content-type` is
  never inspected — `grpc.rs:203`-`205`). Not a contradiction (CF-69-5 leg; the
  OUTCOME is correct — a non-grpc body → decode-error → failure).
- **M69-C** — `grpc_health_decode.rs` fuzz target omits `#![forbid(unsafe_code)]`
  (cosmetic; body has no `unsafe`).
- **M69-D** — single committed fuzz seed (`serving_seed`); 2-3 edge seeds would
  make the 30s CI budget deterministic.
- **M69-E** — `MultipleHealthCheckers` tested for http+tcp and http+grpc, never
  tcp+grpc or all-three (count logic is generic; low risk).
- **M69-F** — the scheduler grpc test asserts only `attempt >= 1`, not the
  `failure` tick / ejection; the `grpc_cfg` authority-default has no direct test.
- **M69-G** — stale "HTTP-only" doc framing (`scheduler.rs:1`-`6`,
  `bootstrap.rs` `HealthCheck` struct doc). No behavioral impact.

**New Minor observations from this re-review** (non-blocking carry-forwards):

- **M69-H** — the probe-layer error arms `GrpcStatus(n)` / `Decode` / `Rpc`
  (`probe.rs:315`-`321`) remain untested at the `grpc_probe_once` layer (the
  underlying `GrpcCallError::GrpcStatus`/`Decode` behaviors ARE tested at the
  `grpc.rs` call layer — `call_nonzero_grpc_status_is_err`, the decode tests —
  so this is a mapping-arm coverage gap, not a behavior gap; strictly outside
  I-1's scope, which was the SERVING/NOT_SERVING verdict + timeout). Cheap to
  close alongside a future M69-F sweep.
- **M69-I** — the `is_h2` field-walk in `validate_health_checks`
  (`bootstrap.rs:~4815`-`4823`) duplicates the identical
  `explicit_http_config.http2_protocol_options` walk at `~3815`-`3819`; could
  share a helper. Cosmetic; both walks are correct.

**Reuse-duplication KEEPs (documented, not new findings):** the probe-loop body
is triplicated (`probe_loop`/`tcp_probe_loop`/`grpc_probe_loop`) and
`run_http2_after_settle_arm` is a ~50-line verbatim clone of its H1 twin
(CF-69-3). Both are defensible under the repo's per-protocol-twin convention;
flagged only so a future "collapse into a generic loop / shared
`assert_after_settle_equivalence` helper" is on record if a THIRD twin lands.

---

## Carry-forward reconciliation

- **CF-69-1** (fixture `0075` omits the response-header axis) — **ACCEPTABLE
  documented boundary.** The synth-503 header set is the pre-existing H2
  no-healthy-upstream observable; the phase does not touch 503 header emission.
  Stays live.
- **CF-69-2** (`health_check.network_failure` not modeled for ANY checker) —
  **CORRECT deliberate divergence (ADR-0139).** The gRPC probe ticks only
  `attempt`/`success`/`failure`, symmetric with HTTP/TCP. Stays live.
- **CF-69-3** (verbatim `run_http2_after_settle_arm` clone) — **reasonable KEEP**
  per the per-protocol-twin convention; logged as the M69 reuse-note.
- **CF-69-4** (`grpc_probe_once` verdict-mapping coverage) — **CONSUMED.** This is
  the I-1 that the §5.2 re-entry closed and this re-review confirmed.
- **CF-69-5** (`grpc_health_check_call` classification — trailers-only →
  `MissingTrailer`; `content-type` not validated pre-decode) — **both OUTCOMES
  correct** (a trailers-only response carries no SERVING body → unhealthy; a
  non-grpc body → decode-error → failure). Kept as doc-note M69-B. Stays live.

---

## §7.5 phase-done gate status (from the §5 state-4 re-verification)

The §5 state-4 re-verification (`superpowers:verification-before-completion`,
commit `37483a7`, CI run `29386606343` GREEN) re-ran the full six-gate §7.5
phase-done gate over the whole tree with the I-1 fix landed: **(a)** fixture
`0075` differential GREEN (synth-503 byte-exact over the H2 listener); **(b)**
`cargo test --workspace --no-fail-fast` passed=1998/failed=7 — the 3 new I-1
tests PASS in the full run and in isolation; the 7 REDs are all pre-existing
documented host-flakes, NONE in the phase-69 surface; **(c)** conformance
unchanged, `known-failures.txt` untouched; **(d)** `grpc_health_decode` fuzz 86M
execs clean, both seeds tracked, `ci.yml` step present; **(e)** fmt / clippy /
build-all-targets / deny all clean; **(f)** this file. `#![forbid(unsafe_code)]`
holds at every crate root (14/14). CI on `37483a7` is authoritative-GREEN.

---

## Assessment

**Ready to merge? YES — APPROVE / MERGE.** The blocking Important **I-1** is
discharged: the §5.2 state-3 re-entry (`e0c6885`) added three non-vacuous
`grpc_probe_once` tests pinning the `SERVING⇒healthy` / `NOT_SERVING⇒failure`
eject-vs-keep arms and the whole-probe timeout wrap (RED-first per D-3.1, product
logic UNCHANGED), the §5 state-4 re-verification re-ran the full §7.5 gate GREEN,
and this independent re-review (two zero-context reviewer subagents + a
main-session read) re-confirmed the codec's decode-safety, the
no-false-Healthy verdict, the scheduler dispatch, and the validator on the
byte-identical product tree — no Critical, no Important, no live bug. The
surviving findings (M69-A..I + the CF-69-1/2/3/5 boundaries + the reuse KEEPs)
are all non-blocking carry-forwards for whatever future phase re-enters this
surface. Per §5.2 the phase now advances to the §5 state-6 close-out (a SEPARATE
session): flip ROADMAP row `69` `done`. **DECISIONS.md ledger head: ADR-0139 (no
new ADR this session; ADR-0140 stays reserved-unfired).**
