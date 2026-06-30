# Phase 53 — `53-accesslog-rf-upstream-reset` — State-5 Code Review

**Reviewer:** fresh `superpowers:code-reviewer` subagent (no session history)
**Diff range:** `204502b..11af920` (phase-53 base = last phase-52 commit → state-4 verification commit)
**Reviewed against:** `SPEC.md` + `PLAN.md` + `PROGRESS.md` + `BEHAVIOR_CONTRACT.md` §F + project coding standards
**Authoritative CI:** run `28435164596` @ code-HEAD `989791d` — both jobs `success`

## VERDICT: ✅ **APPROVED** (0 Critical / 0 blocking-Important / 3 Minor → carry-forward)

The implementation faithfully executes SPEC §A–§G and PLAN Tasks 1–9. The reset 502→503
correction is surgical, the `reset_for_log` discriminator is replay-safe, the derive
precedence is sound, the 502-sweep is complete without over-reach, and the test surface
(two in-process + the differential) adequately proves the `UC` byte-exact claim. No
findings block close-out → advance STATE to the §5 state-6 close-out.

---

## Strengths

- **Surgical reset-arm correction.** `crates/envoy-http1/src/hcm.rs:618/620` is the *only*
  `AttemptOutcome::Reset` synth arm in `envoy-http1` (grep-confirmed); the literal flip
  `synth_status(502→503, close)` plus the co-located `warn!` string (`:615`) is exactly the
  SPEC §A(i) edit, nothing more. The three connect-failure arms (`:501/:530/:547`) were
  already 503 and are untouched.
- **502-survivors correctly preserved.** Verified UNCHANGED: `crates/envoy-http1/src/response.rs:88`
  (`502 => "Bad Gateway"` generic reason-phrase table), the `envoy-filter` cdn_loop 502
  (`cdn_loop.rs:357` + tests), and the H2 `synth_h2_502` family
  (`crates/envoy-http2/src/hcm.rs:1031` etc.). The sweep touched the H1 reset path only —
  no over-reach, no missed reset-path 502.
- **Replay-safety (ADR-0044) holds.** `reset_for_log = matches!(final_outcome, Some(AttemptOutcome::Reset))`
  is set post-loop at `hcm.rs:1200`, reading `final_outcome` which is unconditionally
  overwritten every iteration (`:1092`). A reset on attempt 1 retried to success yields
  `final_outcome = Some(Response)` → not flagged. The FINAL-outcome rule is correct; the
  flag cannot be set spuriously by an earlier attempt.
- **Derive precedence correct.** `hcm.rs:1355` chain is `URX → UF → UC → rcd-match`.
  `connect_failure_for_log` and `reset_for_log` are mutually exclusive (both key on a single
  `final_outcome` value), so a reset-after-connect renders exactly `UC` and a connect-failure
  renders exactly `UF` regardless of UF/UC ordering. `reset_for_log` is set only where
  rcd = `via_upstream` → the else-match's `_ => "-"` arm, so the `NR/UH/UO` arms stay
  byte-identical (fixtures 0056–0060 unaffected).
- **`upstream_rq_5xx` non-interaction preserved.** The L5 gate (`hcm.rs:~1148`,
  `if completing_upstream_response && status/100==5`) is unchanged; a reset synth carries
  `upstream_response: false` → the gate stays false at 503 exactly as at 502.
- **Standards clean.** `#![forbid(unsafe_code)]` holds (`crates/envoy-http1/src/lib.rs:1`);
  the production edits add only literal/`matches!`/`bool` — no `unwrap/expect/panic/todo!/unreachable!`
  on the hot path. `unwrap/expect` appear only in the new `#[cfg(test)]` listeners (allowed).
- **Strong in-process tests.** `h1_upstream_reset_returns_503` drives a genuine
  accept-then-close loopback (its RED showed `HTTP/1.1 502 Bad Gateway` per PROGRESS —
  proving it hit the *reset* arm, not the already-503 connect-failure arm, so the fail-first
  was meaningful). `h1_upstream_reset_access_log_carries_uc_flag` hard-asserts the literal
  `{"rc":503,"rf":"UC"}\n` line — the authoritative proof of the byte-exact `UC` value (the
  differential proves cross-proxy *equality* with Envoy, recon-confirmed at `UC`).
- **Fixture parity is clean.** `envoy.yaml` vs `envoy-rust.yaml` differ only by the documented
  per-side deltas (admin block present/absent, `0.0.0.0`/`127.0.0.1` bind, mount path) + the
  `{{BACKEND_HOST}}` render; cluster/route/`json_format` are byte-identical. NO `circuit_breakers`,
  NO `retry_policy` → single attempt → `Some(Reset)` → flagged, as designed.
- **Harness wiring additive.** The `{{CLOSE_BACKEND_PORT}}` marker, `TcpCloseBackend`, and the
  read-then-close `tcp-echo-server` mode are well-contained; `needs_close_backend` is false for
  all of 0001–0060, so zero behavior change to existing fixtures. The `--close-on-accept`
  default path is preserved.
- **BEHAVIOR_CONTRACT §F sweep complete + H2-502 survivor intact.** The `%RESPONSE_FLAGS%`
  row extends five→six with a correct `UC` per-flag clause; the I1 502→503 sweep flipped
  `:36/:289/:296/:387`; the H2 no-healthy `:1031` 502 is correctly left as-is (H2 deferred, SPEC §4).

---

## Findings (all Minor → carry-forward)

### Minor 1 → **M53-2** (doc precision) — BEHAVIOR_CONTRACT global phrasing vs the still-502 H2 reset path
`docs/envoy-rust/BEHAVIOR_CONTRACT.md:36, :289, :296, :387` now read "send-fail/reset **503**",
but the **H2** reset path still synthesizes 502 (`crates/envoy-http2/src/hcm.rs:387/398` via
`synth_h2_502`; the H2 comment at `:768` still reads "reset synth-502", correctly, for H2).
These contract rows are H1-anchored by lineage (they previously held the H1 value "502"), so
flipping them to 503 is internally consistent — but a future reader could misread the
unqualified "reset 503" as global. **Fix (non-blocking):** add an "(H1)" qualifier or a
one-clause note that the H2 reset remains synth-502 (deferred M45-1), at least on the `:387`
per-attempt-counting paragraph. Fold into the next phase that touches BEHAVIOR_CONTRACT §F or
the H2 reset path.

### Minor 2 (informational — no action required) — expectations.yaml does not pin the literal line
`tests/fixtures/0061-accesslog-rf-upstream-reset/expectations.yaml` asserts pure cross-proxy
whole-line equality + `expected_status: 503`; it does not pin the literal `{"rc":503,"rf":"UC"}`
(the `Http1AccessLogByteExact` driver has no expected-line field — the established 0056–0060
pattern). The literal-`UC` guarantee therefore rests on (a) the in-process
`h1_upstream_reset_access_log_carries_uc_flag` hard-assert and (b) the differential proving
envoy-rust == Envoy, with Envoy recon-confirmed at `UC`. **Adequate; matches precedent — no
change.** Recorded so close-out is aware the differential alone does not pin the constant.

### Minor 3 → **M53-3** (carry-forward, pre-acknowledged) — un-reconned URX+UC combination wording
Under `retry_on: reset` with `num_retries` exhausted on a final reset attempt, both
`retry_limit_exceeded_for_log` and `reset_for_log` would be true; the derive renders `URX`
(ordering wins, deterministic). This is out-of-fixture-scope and pre-acknowledged in SPEC §4.
A latent nuance: on that path the downstream body is the reset **synth-503**, not the "last
upstream response verbatim" that the `URX` BEHAVIOR_CONTRACT `%RESPONSE_FLAGS%` clause describes
— a wording mismatch only, exercised by no fixture. Note alongside M45-2/M53-1; not a phase-53
regression.

---

## Acceptance-gate cross-check
Per PROGRESS state-4, CI run `28435164596` @ `989791d` is documented GREEN: fixture `0061` `ok`
on native Linux (both proxies emit `{"rc":503,"rf":"UC"}`), **132 `test result: ok` / 0 FAILED**
(all 0001–0060 additive-clean), `h2spec_pass_rate_gate ... ok` (≥95% unchanged, no H2 codec
change), fuzz 4-target clean (no new target). The sole LOCAL-RED (0061 `UF`-vs-`UC`) is the
documented `differential-host-bridge-ip-192-168-65-2` artifact — envoy-rust is the correct side
(`UC` via post-connect `error=UnexpectedEof` reset). CI is authoritative per project convention;
the documented evidence satisfies §5 gate (a)–(e). (f) REVIEW.md is this review.

**Disposition:** Approved for close-out. The 3 Minors are carry-forward / doc-precision /
informational; none gates the phase. NEW carry-forwards: **M53-2** (Minor 1), **M53-3** (Minor 3);
Minor 2 is informational (no action). M53-1 (the deterministic `UC` rcd witness) remains live
from state-3. → Advance STATE to the §5 state-6 close-out.
