# Phase 67.3 — §5 state-5 RE-REVIEW (SUPERSEDES the NOT-APPROVED state-5 review)

> Written by the §5 **state-5 RE-review** session (`superpowers:requesting-code-review`), per
> `BOOTSTRAP_PROMPT.md` §5 state 5 and `SKILL_ROUTING.md`. A review is NEVER edited, only
> superseded (D-3.5): this file **supersedes** the earlier NOT-APPROVED state-5 review (preserved
> in git history at commit `7fd7e4e`, and as this file's prior revision). The output of this session
> IS this file.
>
> Cold-started clean: `git status --porcelain` empty; branch `main`; `HEAD` = `origin/main` =
> `2bd6d5e` (the §5 state-4 RE-verification commit); `git fetch origin --prune` showed no sibling
> ahead. **STEP 0.5 CI (FULL 40-char SHA):** the §5.2 state-3 re-entry commit `e551e15` — the
> commit that actually carries the C-1/I-1/I-2 fix — CI run `29208575008` is GREEN
> (`completed`/`success`) on `e551e15e90e93db8454a7b97b5a2b25d263734d1` (the authoritative §7.5
> gate-GREEN signal for the post-fix tree); the docs-only state-4 RE-verification commit `2bd6d5e`
> changed no code (`67.3/PROGRESS.md` + `STATE.md` + `STATE_HISTORY.md` only).
>
> **Review surface:** the §5.2 state-3 re-entry's fix diff `e551e15 -- crates/envoy-tcp/src/lib.rs`
> (the `relay_gated` body + two new in-process witnesses), read against the full landed phase diff
> `b5fc211..HEAD` and the NOT-APPROVED review's C-1/I-1/I-2. **Only `crates/envoy-tcp/src/lib.rs`
> changed in the re-entry** — no other production code, no fixture, no `known-failures.txt`.
>
> **Method (memory `state5-must-probe-untested-compositions`).** A green §7.5 gate proves the code
> does what its tests ask, not that the tests ask the right question. This RE-review (1) code-read
> every branch of the repaired `relay_gated` and re-read the gate primitive + `close_with_drain`;
> (2) LIVE-PROBED the compositions the PLAN flagged (`PLAN.md:403`) and that the prior review found
> defective — running the in-process witnesses that now encode them (the C-1 regression witness
> re-run **5× for stability**, the full `envoy-tcp` lib, and the **real-binary** `envoy-bin`
> establishment backstops); (3) dispatched an independent adversarial `general-purpose` subagent and
> **MEASURED** (not merely accepted) each of its "proven-safe" hypotheses against the code and the
> non-gated `relay` control. The prior arc's Critical hid behind an untested branch; this pass
> confirms those branches are now both correct AND witnessed.

---

## VERDICT

> ### **APPROVED.** The §5.2 state-3 re-entry's C-1 fix, I-1 fix, and I-2 witness resolve every
> ### finding of the NOT-APPROVED state-5 review; no new defect is introduced; all standing traps
> ### are honored; the §7.5 acceptance frame is GREEN (state-4 RE-verification). Advance to §5
> ### state 6 (close-out) — a SEPARATE session per §5.1 / ADR-0127.
>
> The three findings are resolved as follows, each confirmed by code-read **and** a green, stable
> witness, and cross-checked by an independent adversarial subagent that reached the same conclusions
> and found no regression:
>
> - **C-1 (was Critical) — RESOLVED.** The phase-1 `u2d`-wins (upstream-EOF-before-first-byte) branch
>   (`crates/envoy-tcp/src/lib.rs:321-334`) no longer `gate_fut.await?`s; on upstream EOF it yields
>   `(GateOutcome::SkippedCleanly, None)`, routing to the `SkippedCleanly | Denied` close arm
>   (`:340-352`): drop `u2d`, drop the upstream (guard fires), `reunite` the downstream halves, and
>   `close_with_drain`. `close_with_drain` **shuts the write half FIRST** (`envoy-listener/src/lib.rs:240`),
>   so a *passive* client receives a prompt clean EOF — the exact hang the prior review reproduced is
>   gone. Correct per ADR-0016 (`enable_half_close:false` → an upstream FIN tears the connection down)
>   and ADR-0131 case C (a byte-less client is never evaluated, so there is no RBAC verdict to defer).
>   The regression witness `upstream_eof_before_first_byte_closes_downstream_promptly` reproduced the
>   pre-fix 3-second hang (documented RED) and now passes **5/5** reruns (0.00s each).
> - **I-1 (was Important) — RESOLVED (structurally).** The upstream→downstream copy is now ONE
>   continuous `Box::pin`'d future `u2d = tokio::io::copy(&mut ur, &mut dw)` (`:312`), created once and
>   carried across both phases — `&mut u2d` in the `Admitted(Some(b))` select (`:366`), `u2d.await` in
>   the `Admitted(None)` drain (`:379`). It is NEVER dropped-and-restarted on any admitted path, so its
>   internal `CopyBuffer` (read-but-unwritten banner bytes under client backpressure) can no longer be
>   silently discarded. The prior review MEASURED the loss *mechanism* as deterministic but the
>   *end-to-end* trigger as narrow (18 real-socket runs, no loss); a reliably-red e2e test is therefore
>   not achievable, so the fix is correctly framed as a **structural elimination** of the
>   drop-and-restart, guarded by the I-2 behavioural witness — an honest and defensible call.
> - **I-2 (was Important) — RESOLVED.** `allowed_first_byte_and_payload_round_trip_both_directions`
>   drives an ALLOWED first byte + a 59-byte payload through `[rbac(ALLOW), tcp_proxy]` over a
>   server-first backend and asserts (a) the re-injected first byte + subsequent payload reach the
>   backend byte-exact and IN ORDER, and (b) the banner and a `250 OK` response both flow downstream.
>   It genuinely exercises the single most intricate branch — the gap that let C-1 and I-1 ship green.
>   Passes; stable.
>
> Everything the prior review praised still holds and was re-confirmed: the `FirstByteGate` extraction,
> the `connect_upstream`/`relay` establishment/data split (ADR-0016 posture + `cx_active`/`cx_total`
> placement intact), echo/hcm byte-for-byte parity (they inherit the non-consuming `evaluate_peek`
> default), the DENY-withholds-the-byte + FIN-matrix witnesses, and the `transport_socket.is_some()`
> TLS narrowing. No new Critical, no new Important, no security or data-durability issue.

---

## §1. What changed since the NOT-APPROVED review (code-read, verified at `HEAD`)

The re-entry touched **only** `crates/envoy-tcp/src/lib.rs` (`git show e551e15 --stat`): the
`relay_gated` body and two new `#[cfg(test)]` witnesses (+ one backend helper). The rest of the phase
(the `FirstByteGate`/`handle_gated` primitive, the config narrowing, the envoy-bin backstops, the
BEHAVIOR_CONTRACT item-13 split) is byte-identical to the state-5-reviewed tree and is not re-graded
here beyond the standing-trap spot-checks in §3.

The repaired `relay_gated` (`crates/envoy-tcp/src/lib.rs:288-388`):

1. **`let mut u2d = Box::pin(tokio::io::copy(&mut ur, &mut dw));`** (`:312`) — the continuous
   upstream→downstream copy (I-1). `Box::pin` (not `pin!`) so the close paths can `drop` it early to
   reclaim the `dw`/`ur` borrows for `reunite`.
2. **Phase-1 `biased` `select!`** (`:318-335`): the gate (`evaluate_read_half(&mut dr)`) is polled
   first; the `u2d`-wins arm (upstream EOF before the first byte) now returns
   `(GateOutcome::SkippedCleanly, None)` — **no gate await** (C-1).
3. **`ClientGoneEarly`** → drop (`:339`).
4. **`SkippedCleanly | Denied`** (`:340-352`): `drop(u2d)`; drop upstream; `reunite`; `close_with_drain`.
   This arm is now reachable via BOTH the gate's `Denied` and the phase-1 banner branch's
   `SkippedCleanly` — which **consumes M67.3-1** (the arm is no longer a dead `SkippedCleanly` match).
5. **`Admitted(Some(b))`** (`:355-374`): `uw.write_all(&[b])` (re-inject), then `select!` `copy(dr→uw)`
   against `&mut u2d` (the SAME copy — no restart, I-1), then `drop(u2d)` + drop halves.
6. **`Admitted(None)`** (data-less FIN) (`:375-381`): `uw.shutdown()`, then `u2d.await` (drain the SAME
   copy to upstream EOF).

---

## §2. Grading — do the fixes actually resolve the findings?

### C-1 — RESOLVED (confirmed by code-read + LIVE re-probe)

- **The hang is gone.** The banner branch no longer awaits a byte a passive client will never send.
  It routes to the clean-close path; `close_with_drain`'s `writer.shutdown().await` sends the FIN
  before the drain loop, so the client observes `Ok(0)` promptly.
- **The banner is not truncated on this path.** `tokio::io::copy` resolves only after flushing on
  source EOF, so `u2d` completing `Ok` means the full banner was written to `dw` before the
  SkippedCleanly routing — the passive client reads the full banner, then the clean EOF (asserted by
  the witness's `read_exact(&banner)` then `read == 0`).
- **No guard leak.** `_cx_guard` is bound in the function-scope `UpstreamConn` destructure (`:296-301`)
  and drops when `relay_gated` returns down every arm — including the close arm. (It is held *for the
  duration of* `close_with_drain`; see the non-blocking note in §4 — pre-existing, matches `relay`.)
- **Live re-probe:** `upstream_eof_before_first_byte_closes_downstream_promptly` green **5/5**; the full
  `envoy-tcp` lib green **16/16**; the real-binary `envoy-bin` backstops green **24/24**.
- **M67.3-1 consumed, M67.3-2 resolved:** `SkippedCleanly` is now reachable + meaningful on the gated
  path; BEHAVIOR_CONTRACT item-13's "PLAINTEXT = FULL PARITY" claim (`:429`) is now accurate (the C-1
  and I-1 counter-examples are fixed), so no contract row needs a divergence note.

### I-1 — RESOLVED structurally (mechanism eliminated, not merely papered over)

- The copy is provably continuous across the phase-1→phase-2 transition on every admitted path. There
  is no `tokio::io::copy(&mut ur, &mut dw)` reconstruction anywhere after `:312` (verified by reading
  the whole function). The CopyBuffer survives because the future survives.
- **No double-poll / poll-after-Ready.** `tokio::select!` returns a branch only when it is `Ready`; if
  control is still inside the phase-1 select, `u2d` has not returned `Ready` (or the select would have
  returned via the `u2d` arm → SkippedCleanly, not Admitted). So re-polling `&mut u2d` / `u2d.await` in
  phase 2 is always on a still-pending future. The `biased` order means a gate-`Ready` tick does not
  poll `u2d`, leaving its state untouched. (Measured against the subagent's hypothesis — confirmed.)
- **Borrows are sound.** `drop(u2d)` releases the `&mut ur`/`&mut dw` borrows before `drop((ur, uw))`
  and `dr.reunite(dw)`; the halves are owned split-halves of one stream, so `reunite` succeeds. Compiles
  under `cargo clippy --all-targets --all-features -- -D warnings` clean.
- **Guarded by I-2** rather than a flaky red-first e2e — the correct engineering choice given the
  MEASURED-narrow trigger.

### I-2 — RESOLVED

- The witness exercises the `Admitted(Some(b))` re-inject + bidirectional copy and asserts byte-exact,
  in-order delivery in both directions. Re-inject ordering is guaranteed: `write_all(&[b])` completes
  before `copy(dr→uw)` reads the remainder of the client stream, so the first byte precedes the payload
  at the backend (asserted). This is the natural regression guard for the I-1 fix.

---

## §3. Contract & invariant conformance (spot-checks, re-verified at `HEAD`)

- **§7.5 acceptance frame — GREEN at state-4 RE-verification.** `67.3/PROGRESS.md`'s
  `## Session: §5 state-4 RE-verification` records build/clippy(`-D warnings`)/fmt/deny EXIT 0;
  `cargo test --workspace --no-fail-fast` **1949 passed / 6 failed** — all 6 the documented
  CI-authoritative host-flakes (the four `access_log_*_upstream_reset` witnesses `0061`/`0062`/`0069`/`0070`
  fail deterministically = the reference Envoy can't reach the host-spawned close backend and logs
  `rf:"UF"` where envoy-rust correctly logs `rf:"UC"`; `admin_config_dump_server_info` +
  `upstream_circuit_breaker_max_pending_requests_fixture` PASS in isolation = parallel-load flakes);
  `local 1949+6 == CI passed`. The two new re-entry witnesses are GREEN. This RE-review does not
  re-run the gate (it is the acceptance frame, already met); it grades whether the fixes hold and
  probes what the gate cannot see.
- **Standing traps — all honored** (`git diff --name-only b5fc211..HEAD`): `tests/fixtures/`,
  `tests/conformance/h2spec/known-failures.txt`, `crates/envoy-filter/src/rbac.rs`, and
  `crates/envoy-bin/src/tls_handler.rs` are all **UNTOUCHED**. `is_terminal_network_filter` untouched;
  `filters: []` still accepted; the `direct_response` chain bypass intact (not re-wrapped); ADR-0131
  first-byte verdict preserved; ADR-0016 `select!` + `cx_active`/`cx_total` placement preserved in
  `connect_upstream`/`relay`; ADR-0124 `close_with_drain` + both `post_eof_*` tests unweakened; item-14
  / ADR-0133 (`rbac.rs` HTTP-vs-L4 divergence) not re-litigated; D6 keeps TLS `[rbac, tcp_proxy]`
  rejected (CF-67-7); differential surface `0001`/`0071`/`0072`/`0073` unedited; `#![forbid(unsafe_code)]`
  holds. The C-1 `SkippedCleanly`-on-upstream-EOF route and the I-1 continuous-`u2d` copy are present
  and intact.
- **No new ADR.** The fixes align with existing decisions (ADR-0016 teardown, ADR-0131 first-byte
  verdict, ADR-0124 drain) — they close a divergence and an internal correctness gap without a new
  measured wire-shape. Ledger head stays **ADR-0135** (next `ADR-0136`, unreserved).
- **Carry-forwards.** **M-1** not consumed (67.3 doesn't touch the CidrRange surface); **CF-67-6** not
  folded (D8 opportunistic); **CF-67-7** correctly opened for the TLS composition. Unchanged.

---

## §4. Independent adversarial subagent

An independent `general-purpose` subagent re-reviewed `e551e15`'s diff for the `relay_gated` state
machine — C-1 fix correctness, I-1 buffer continuity, poll-after-Ready hazards, re-inject backpressure
deadlock, borrow/reunite soundness, and error handling. It **independently graded C-1/I-1/I-2 all
RESOLVED** and **found no new defect**, proving each probed hazard safe:

- poll-after-Ready is impossible (a still-in-phase-1 `u2d` has never returned `Ready`);
- the single re-injected byte is the first write to a fresh upstream socket → cannot block;
- the gate reads `dr` and `u2d` reads `ur` → no contention, no dropped/duplicated downstream byte;
- `drop(u2d)` before `reunite` makes the borrows sound.

This session did not merely accept those hypotheses (memory `state5-must-probe-untested-compositions`):
each was re-derived from the code here and cross-checked against the green + stable witnesses and the
non-gated `relay` control. The subagent's one **hypothesised, pre-existing** note is recorded below as
non-blocking.

**Non-blocking observations (pre-existing, NOT introduced by the fix — do not block APPROVE):**

- **`_cx_guard` held across `close_with_drain` on the SkippedCleanly/Denied path.** On the C-1 path the
  upstream is already fully closed, yet `cluster.<name>.upstream_cx_active` stays incremented until the
  downstream client closes (or the listener's `DRAIN_BUDGET` aborts). This connection-lifetime guard
  timing is the existing design — identical on the `Denied` close path and to `relay`, and a strict
  improvement over the pre-fix behaviour, which held the guard *forever* by hanging. Cosmetic; folds
  naturally with the M67.3-5 cosmetic family for a future phase that touches these close paths.
- **`enable_half_close:false` teardown drops the other direction's in-flight copy** on a downstream FIN
  in the `Admitted(Some(b))` select — ADR-0016 semantics, present in the pre-fix `select` structure,
  unchanged, and matching the non-gated `relay`.

---

## §5. Minors — disposition

- **M67.3-1 — CONSUMED.** `SkippedCleanly` is now produced by the phase-1 banner branch and handled
  meaningfully; no longer a dead arm.
- **M67.3-2 — RESOLVED.** BEHAVIOR_CONTRACT item-13's plaintext "FULL PARITY" is accurate now that
  C-1/I-1 are fixed; no contract edit required.
- **M67.3-3** (config narrowing uses `transport_socket.is_some()` rather than the precise
  `chain_has_tls` predicate — correct today, fragile to a future reorder of the two `bootstrap.rs`
  blocks), **M67.3-4** (`ClientGoneEarly` discards the underlying I/O error — a minor diagnostic
  regression), **M67.3-5** (swallowed `uw.shutdown()`/drain results on the data-less-FIN ALLOW drain;
  upstream-reset-on-re-inject surfaced at `warn!` as log noise) — all **stay non-blocking carry-forward
  Minors**. `crates/envoy-config/src/bootstrap.rs` and `crates/envoy-listener/src/lib.rs` were
  deliberately not touched by the re-entry (minimal change surface). **Confirmed: none blocks the
  phase.** They surface for the next phase that touches their respective surfaces.

---

## §6. Assessment

**Ready to merge? Yes.** The §5.2 state-3 re-entry resolves the Critical and both Important findings of
the NOT-APPROVED state-5 review: C-1's hang is structurally gone (banner-EOF → `SkippedCleanly` clean
close, prompt FIN to a passive client), I-1's silent-loss mechanism is eliminated (one continuous
`u2d` copy), and I-2's untested branch now has a byte-exact duplex witness. No new defect is introduced
(confirmed by an independent adversarial subagent whose hypotheses were measured, not assumed), every
standing trap is honored, the surviving Minors are all non-blocking, and the §7.5 acceptance frame is
GREEN over the post-fix tree. Advance to §5 state 6 (close-out): flip ROADMAP row `67.3` → `done`
(which also flips parent row `67` → `done`, since `67.1`/`67.2`/`67.3` are then all done), relocate the
active-phase Notes subsection, and set STATE → awaiting next planning. Per §5.1 / ADR-0127 this session
does NOT chain into the close-out — that is a SEPARATE session.
