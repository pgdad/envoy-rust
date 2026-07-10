# Phase 67.1 — state-5 CODE REVIEW (re-review; SUPERSEDES the prior REVIEW.md)

> Written by the §5 **state-5 code-review** session (`superpowers:requesting-code-review`), per
> `BOOTSTRAP_PROMPT.md` §5 state 5 and `SKILL_ROUTING.md`. Output of this session is this file.
>
> **This review SUPERSEDES the earlier state-5 review** (verdict **NOT APPROVED**, commit `065a857`,
> which raised the Critical **C-1**). Per doctrine **D-3.5** a review is superseded only by a LATER
> review, never edited: the prior REVIEW.md text is preserved verbatim in this repo's git history at
> `065a857` as the record of that earlier review. This file replaces it because the §5.2 re-entry has
> since landed the C-1 repair (`ADR-0132`, tasks 15–20, commits `d066f72`..`641ce42`) and the state-4
> verification (`b99ee8d`), and the phase is back at state 5 awaiting a fresh grade.
>
> **Review surface:** the whole `67.1` phase (`f40a41e..b99ee8d`), with **primary focus on the C-1
> repair** — the delta since the prior review, `cd874f6..HEAD`: `direct_response` chain-bypass,
> `[rbac, tcp_proxy]` fail-loud rejection, and the I-2 / I-4 / I-5 / M-1..M-8 fixes.
> **Method:** full read of the repair diff; one independent adversarial `general-purpose` reviewer
> subagent tasked to refute the fix; **and first-hand LIVE MEASUREMENT by this session of the three
> compositions that had zero coverage when C-1 shipped** — because the lesson of C-1 (and of
> `ADR-0131` before it) is that a green (a)-(e) gate proves the code does what its tests ask, never
> that the tests ask the right question. Every measurement is quoted inline.

---

## VERDICT

> ### **APPROVED. §7.5 gate (f) is SATISFIED.**
>
> The Critical **C-1** that blocked the prior review is **fixed, and the fix is measured-correct**
> against the very behaviors C-1 identified as divergent. The three previously-uncovered compositions
> are now covered — two by measured parity, one by a fail-loud rejection with a named owner (`67.3`) —
> and `67.1`'s composition matrix is closed. All Important findings (I-2, I-3, I-4, I-5) and all eight
> Minors (M-1..M-8) are resolved. No settled decision was weakened; no forbidden anti-pattern was
> introduced.
>
> Per `BOOTSTRAP_PROMPT.md` §5 state 6, the **SEPARATE next session** performs the close-out (flip
> ROADMAP row `67.1` → `done`, advance `STATE.md`, ADR-0035 relocation). Per §5.1 / `ADR-0127` this
> session does **not** chain into state 6.

---

## §1. What C-1 was, and why the fix is the right shape

The prior review's C-1 was a **measured, reproducible, cross-proxy divergence**: `ChainHandler`
awaited the first downstream byte *before delegating to the terminal handler*, so the first-byte gate
stalled the **terminal** filter too. `[rbac, echo]` could not detect it because `echo` has no
establishment-time work; the measurement had generalized over a population of one. Against three
untested terminal filters it produced two hangs (`[rbac, direct_response]` no-byte; `[rbac, tcp_proxy]`
server-first) and one dropped payload (`[rbac, direct_response]` + byte).

`ADR-0132` re-measured **all four** terminal filters against the pinned image (mid-flight `/stats`
scrape, per the C-1 requirement) and split the fix by terminal:

- **`echo` / `http_connection_manager`** — no establishment-time work, so the chain's first-byte gate
  is observationally identical to upstream's "run establishment, defer only the verdict" model.
  Unchanged. Correct.
- **`direct_response`** — writes its payload and closes at establishment, so upstream never evaluates
  RBAC at all (even under `action: DENY`). The fix **bypasses the chain**: `main.rs` hands the
  connection straight to `DirectResponseHandler`. This is a *simplification*, not a special-case, and
  it is exact measured parity.
- **`tcp_proxy`** — connects upstream and relays a server-first banner at establishment. Faithful
  behavior needs an establishment/data-phase split of `ConnectionHandler`, which fired §6.1's
  mid-execution valve and was carved into `67.3`. Until then, `[rbac, tcp_proxy]` is **rejected at
  config load, fail-loud**, naming `67.3` — strictly better than the shipped runtime deadlock.

This is the correct descendant of the ADR-0131 lesson applied one level up, and it does **not**
re-litigate ADR-0131 (the first-byte *verdict* timing stands, re-confirmed).

---

## §2. First-hand measurement performed this session

This is the load-bearing part of a state-5 review: I built `target/debug/envoy-bin` at `HEAD`
(`b99ee8d`; per memory `differential-harness-uses-debug-envoy-bin` a stale binary would red the new
`ConfigError` variant) and drove the three compositions with raw sockets, scraping the mapped admin
port. **Every C-1 divergence is gone, and the observed behavior matches `ADR-0132`'s upstream table.**

### (a) `[rbac, direct_response]` — the two hangs and the dropped payload, retested

```
=== [rbac(DENY), direct_response], client SENDS NOTHING  (C-1 R2: was a permanent hang) ===
  recv=b'HELLO-DR\n'  how=clean_eof
  stats: drd.rbac.{allowed,denied,shadow_allowed,shadow_denied} = 0,0,0,0

=== [rbac(DENY), direct_response], client sends a byte    (C-1 R3: was a dropped payload) ===
  recv=b'HELLO-DR\n'  how=clean_eof
  stats: drd.rbac.{allowed,denied,shadow_allowed,shadow_denied} = 0,0,0,0
```

The payload is delivered with a clean EOF whether or not the client speaks, the four counters are
**registered** (stat-tree parity) and **never tick**, and **a DENY policy does not suppress the
payload** — exactly `ADR-0132`'s measured upstream behavior. The pre-fix hang is gone.

### (b) `[rbac, tcp_proxy]` — the deadlock, now a fail-loud config rejection

```
$ target/debug/envoy-bin -c [rbac, tcp_proxy]     # exit 1, ConfigError on STDOUT (per the trap)
ERROR envoy-rust exited with error error=listener "l0" filter_chains[0]: non-terminal filter
  "envoy.filters.network.rbac" before terminal filter "envoy.filters.network.tcp_proxy" is not yet
  supported — "envoy.filters.network.tcp_proxy" does establishment-time work before the first
  downstream byte, which envoy-rust's network filter chain cannot yet express (phase 67.3 owns
  this; upstream Envoy accepts this config)
```

Exit 1, the error names **both** filters and phase **`67.3`**, and it is on STDOUT (per memory /
trap 8). The runtime deadlock is unreachable.

### (c) The untouched `echo` path still behaves (regression check)

```
=== [rbac(DENY), echo], send a byte ===
  recv=b''  how=clean_eof   stats: ed.rbac.denied = 1
```

Zero bytes, clean EOF, `denied` ticks once — unchanged from `ADR-0131`/SPEC R-2. `echo` was not
broken by the restructure.

### (d) The composition test suite, driven against the real binary

`cargo test -p envoy-bin --test network_filter_rbac` → **18 passed / 0 failed**, including the
`[rbac, hcm]` ALLOW/DENY pair (`rbac_before_hcm_evaluates_on_the_first_request`,
`deny_before_hcm_writes_nothing_and_ticks_denied_once`), the two `[rbac, direct_response]` witnesses,
and `[rbac, tcp_proxy]`'s rejection + over-rejection guard. These are live probes: they spawn
`envoy-bin` and drive real sockets. I independently re-ran the mutation-checked config/listener/unit
pins as well — `rejects_rbac_composed_with_tcp_proxy`, `lone_tcp_proxy_chain_is_still_accepted`,
`terminal_not_last_error_wins_over_unsupported_composition`,
`structurally_invalid_metadata_leaf_reports_a_scope_neutral_listener_error`,
`well_formed_metadata_leaf_is_rejected_by_the_l4_walk_instead`,
`network_rbac_depth_bound_precedes_the_l4_walk`, `pending_tasks_aggregates_across_accept_loops`,
`pending_tasks_single_slot_is_the_identity`, `sequential_connections_do_not_accumulate_joinset_tasks`,
`shadow_counters_register_at_zero_and_never_tick` — all green.

> **Scope of the measurement, stated honestly.** This session measured **envoy-rust at `HEAD`** and
> confirmed it matches the upstream behavior that `ADR-0132` (and the prior review) measured
> exhaustively against the pinned image with mid-flight scraping. It did **not** re-boot upstream
> Envoy — the upstream side is already on disk, cross-proxy, and re-confirmed; the open question a
> state-5 review owns after a C-1 repair is *"does the shipped fix reproduce the measured upstream
> behavior?"*, and it does.

---

## §3. Strengths of the repair

1. **The bypass is a deletion, not a special-case.** `main.rs`'s `direct_response` arm drops
   `chain_filters` and hands the raw handler through; the filters are still *built* (which is what
   registers the counters at 0 for stat-tree parity) and then genuinely never run. That is the
   simplest expression of the measured fact "upstream never evaluates RBAC on this chain."

2. **The `tcp_proxy` rejection is placed correctly and guarded against over-rejection.** It sits
   **after** both terminal-position checks (so `[echo, rbac, tcp_proxy]` still reports terminal-not-last
   — pinned) and behind a `chain_len >= 2` guard (so lone `tcp_proxy`, fixture `0003`, is untouched —
   pinned at both the config layer and against the real binary). The error names its owning phase and
   `67.3` deletes it; it is a loud refusal with a ROADMAP row, not a §6.3 stub.

3. **I-2 is fixed at the right layer, and the false invariant is corrected.** The comment that had the
   validation order backwards is now correct at both sites, the message is generalized to a seventh
   shared scope-neutral variant (accurate for the HTTP filter too), and — crucially — the L4 walk was
   **not** reordered ahead of `validate_rbac_rules`, preserving the depth bound that
   `network_rbac_depth_bound_precedes_the_l4_walk` pins. Both halves of the ordering claim are now
   tested.

4. **I-4's `PendingTasks` aggregator is sound.** One slot per accept loop, the total recomputed and
   broadcast under a `std::sync::Mutex` held across no `.await`, `send_replace` so the value updates
   with no subscriber, poison-recovery on a `Vec<usize>` that cannot be left inconsistent, and a
   `debug_assert` at the slot-mint seam that turns a latent nondeterministic `index out of bounds`
   into a deterministic failure. The single-socket identity is preserved, so the M66-3 reaping witness
   is unaffected. Mutation-checked with the rebuild confirmed.

5. **M-1 caught and fixed a genuinely vacuous assertion.** `register_counter` is get-or-create, so the
   registration half of `shadow_counters_register_at_zero_and_never_tick` passed regardless; the new
   non-creating `registered_stat` (via `snapshot()`) makes it a real check, mutation-verified.

6. **M-2's discipline is now uniform and it already earned its keep.** Every downstream read is
   bounded by `READ_BUDGET`, and `validate_config` is bounded by `VALIDATE_BUDGET` with `kill_on_drop`
   — which mattered, because the task-16 RED *hung* (a valid config serves forever) exactly as M-2
   predicted, one task early.

7. **The docs were corrected alongside the code, not left stale.** `ChainHandler`'s rustdoc carries
   the per-terminal wrappability table and names ADR-0130 Decision 2 as superseded;
   `direct_response.rs`'s "writes IMMEDIATELY" contract is true again; and `BEHAVIOR_CONTRACT.md`
   gains item 13 (the measured composition table) and item 9b (M-5), and corrects item 1 (first-byte is
   a property of the verdict, and the data-less-FIN semantic is per-terminal). I checked item 13's
   `direct_response` row against my own measurement — it matches.

---

## §4. Issues

### CRITICAL — none.

C-1 is fixed and measured-correct. No new Critical was found by this review or by the adversarial
subagent.

### IMPORTANT — none.

I-2, I-3, I-4, I-5 from the prior review are all resolved (I-3 on the ledger by `ADR-0132` decision 5:
`M66-3` recorded PARTIALLY consumed, the drain bound opened as `CF-67-6`). No new Important was found.

### MINOR — forward-looking / cosmetic, none blocks

An independent adversarial reviewer subagent, tasked to refute the fix, found **no Critical and no
Important** finding (it independently verified the bypass's counter registration, the `tcp_proxy`
rejection's placement + in-bounds indexing, the `PendingTasks` sizing invariant across all three build
sites, the non-vacuity of every new test, and that every doc-referenced test symbol resolves). It
surfaced two Minors, folded in below with mine.

- **N-1 (advisory, for the next NEW non-terminal network filter — not `67.2`).** The
  `direct_response` bypass in `main.rs` is **unconditional on the non-terminal prefix**: it drops
  `chain_filters` for *any* chain ending in `direct_response`. That is exact parity today because
  `rbac` is the only non-terminal network filter and `[rbac, direct_response]` was measured to bypass.
  But a future non-terminal filter with establishment-time side effects composed before
  `direct_response` would inherit the bypass **silently** — the same "untested composition" shape as
  C-1. `ADR-0132`'s methodological note already flags the general hazard; recording it here at the
  code site so the phase that lands `sni_cluster` (or any second non-terminal network filter) probes
  `[<new-filter>, direct_response]` against upstream before trusting the bypass. `67.2` widens `rbac`'s
  matcher *arms* over the same filter, so it does **not** newly reach this.

- **N-2 (diagnostic precision, reachable today).** Because the composition check
  (`bootstrap.rs`) runs **before** the per-filter allow-list loop, a chain like
  `[<unknown-filter-name>, tcp_proxy]` is reported as `UnsupportedNetworkFilterChainComposition` with
  the unknown name as `non_terminal` — rather than `UnsupportedFilter`. Both are fail-loud config
  rejections, so this is a precision loss, not a correctness bug, but it is a genuinely new ordering
  interaction that the neighboring **M-4** comment (which documents only unknown-name-*as-last* →
  `NetworkFilterChainNotTerminated`) does not mention. A one-line extension of that comment would
  close the gap. (Correctness is safe: the reviewer confirmed `filters[chain_len - 2]` is provably
  non-terminal for any *known* filter, since the terminal-not-last scan already returned for any
  terminal in a non-last slot.) `67.3` deletes this variant regardless.

- **N-3 (cosmetic, no action).** In `DECISIONS.md`, the `---` separator preceding `## ADR-0131` (left
  by `ADR-0132`'s prepend) lacks the blank-line padding the rest of the file uses. It renders
  correctly (thematic break + ATX heading) and `DECISIONS.md` is append-only (D-3.5), so this is noted
  only for awareness, not for a fix.

None of N-1/N-2/N-3 is an obligation on `67.1`; they are notes for the phases that reopen this
surface.

---

## §5. Findings explicitly considered and REJECTED by this review

Recorded so a future session does not re-raise them (and none was raised by the adversarial subagent):

- **"Re-wrap `direct_response` in the chain / make `[rbac, tcp_proxy]` work here."** No. The bypass is
  exact measured parity; the establishment/data-phase split is `67.3`'s charter (`ADR-0132`).
- **"Revert `ADR-0131`."** No. The first-byte *verdict* timing is measured and re-confirmed; C-1 was
  never about *when RBAC decides*.
- **"Bound `close_with_drain`'s steady-state drain."** No — that is `CF-67-6`, deliberately deferred
  (`ADR-0132` decision 5); `ADR-0124`'s drain and both post-EOF-write tests stay unweakened.
- **"Add a `_ =>` catch-all to `permission_matches` / `principal_matches` / `validate_l4_*`."** No.
  The compile break is the intended forcing function for `67.2`; `clippy::only_used_in_recursion` is
  deliberately allowed and `conn` keeps its name.
- **"Add `rbac` to `is_terminal_network_filter`," "reject `filters: []`," "fix `echo`/`hcm`,"
  "trim `known-failures.txt`."** No — each is settled and/or measured parity.
- **"The single-listener `.next()` limit is a new bug."** No — pre-existing (M-8), documented at the
  site.

---

## §6. Assessment

**Ready to merge?** **Yes.**

**Reasoning.** The architecture was already sound; the one thing that blocked it — a measured
divergence on three uncovered compositions — is now fixed by terminal, and I re-measured all three
against the shipped binary and confirmed parity (two) / a fail-loud rejection with a named owner
(one). The tests that gave C-1 room to ship now exist and are non-vacuous, the doc invariants are
corrected, and every Important and Minor is resolved. `67.1`'s composition matrix is closed; the
C-1 lesson is carried forward for `67.2` (matcher arms) and `67.3` (`[rbac, tcp_proxy]`).

**Next session (SEPARATE, per §5.1 / `ADR-0127`):** state 6 close-out — flip ROADMAP row `67.1` →
`done` (parent row `67` stays `in-progress`; it flips only when `67.1`+`67.2`+`67.3` are all `done`),
advance `STATE.md`, ADR-0035 relocation.

---

## §7. Carry-forward ledger (unchanged by this review)

This review **opens and closes no carry-forward**. State as of `ADR-0132`:

- **CONSUMED by `67.1`:** `CF-66-2`, **`M66-3` PARTIALLY** (reaping half only — the steady-state drain
  bound is `CF-67-6`), `M66-4`, `CF-67-4`, `M66-6`.
- **CLOSED by recon, no code change:** `M66-5`.
- **OPENED, none blocks:** `CF-67-5` (upstream's *connection* behavior on empty `filters: []`),
  `CF-67-6` (bound `close_with_drain`'s steady-state drain).
- **DEFERRED to `67.2`:** the connection-level matcher arms + `CidrRange` + the V-1 three-site
  shared-enum fallout.
- **DEFERRED to `67.3`:** the `ConnectionHandler` establishment/data-phase split, the correct
  `[rbac, tcp_proxy]` composition (which DELETES `UnsupportedNetworkFilterChainComposition`), the
  per-terminal data-less-FIN semantics, and the TLS-composition probe.
- **STILL LIVE, none blocks:** `CF-67-1`, `CF-67-2` (M-7 *pins* its boundary, does not consume),
  `CF-67-3` (scope unchanged), `M66-7`, `CF-66-1`, and the long tail recorded in `STATE.md`.
- **Advisory (this review):** N-1, N-2, N-3 above — notes for the phases that reopen the composition
  surface (and one cosmetic doc nit); none is an obligation on `67.1`.
- **Numbering:** `M66-1` was never allocated; the ledger does not backfill.
- **`DECISIONS.md` ledger head: `ADR-0132`.** Next available: **`ADR-0133`**, unreserved — this review
  needed no ADR.
