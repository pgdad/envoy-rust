# Phase 67.3 — §5 state-5 CODE-REVIEW

> Written by the §5 **state-5 code-review** session (`superpowers:requesting-code-review`), per
> `BOOTSTRAP_PROMPT.md` §5 state 5 and `SKILL_ROUTING.md`. The output of this session IS this file.
> Cold-started clean: `git status --porcelain` empty; branch `main`; `HEAD` = `origin/main` =
> `d01e218` (the state-4 verification commit); `git fetch origin --prune` showed no sibling ahead.
> **STEP 0.5 CI (FULL 40-char SHA):** the code-complete commit `9b09bdc`'s CI run `29194938132` is
> GREEN on `9b09bdcc93f8b8ba77eeacdaef86110867e8a143` (the authoritative §7.5 gate-GREEN signal); the
> state-4 verify commit `d01e218`'s CI run `29204641108` is GREEN on
> `d01e2187e6f09c889dcb4e5a631871a2c148dbc5`.
>
> **Review surface:** the phase's landed diff `b5fc211..HEAD` (`git diff b5fc211..HEAD -- crates docs`)
> — the five task commits `412e133`/`7be5bf2`/`23ff455`/`7946aec`/`0872f6b` + the `9b09bdc` fmt
> fixup. The establishment/data split (`envoy_listener::{GateOutcome, FirstByteGate,
> ConnectionHandler::handle_gated}` + the `ChainHandler` delegation), `envoy_tcp::TcpProxy::{
> connect_upstream, relay, relay_gated}` + the `handle_gated` override, the narrowed
> `UnsupportedNetworkFilterChainComposition` (TLS-only), the envoy-bin backstops + FIN matrix, and
> `BEHAVIOR_CONTRACT.md` item 13.
>
> **Method (memory `state5-must-probe-untested-compositions`).** A green §7.5 gate proves the code
> does what its tests ask, not that the tests ask the right question. The phase adds `handle_gated`
> over N terminal handlers (`echo`/`hcm` inherit the default; `tcp_proxy` overrides; `direct_response`
> bypasses). This review (1) code-read every branch of `relay_gated` and the gate primitive; (2)
> grepped which compositions the fixtures/backstops actually exercise; (3) **LIVE-PROBED an untested
> composition** — the "upstream EOFs before the client's first byte" case the PLAN itself flagged as
> the risky part (`PLAN.md:403`) — building a throwaway integration test against a freshly-built
> `target/debug` and a **control** on the non-gated path; and (4) dispatched an independent
> adversarial `general-purpose` subagent for breadth. The live probe surfaced a **Critical**.

---

## VERDICT

> ### **NOT APPROVED — one Critical (C-1) + two Important (I-1, I-2). §5.2 re-entry at state-3 required.**
>
> The phase's structure is sound and its stated witnesses pass, but **two untested branches of
> `relay_gated`** — both flagged by the PLAN's own §6.1 mid-execution-valve note (`PLAN.md:403`) as
> the risky part — carry defects, and neither is covered by any witness:
>
> - **C-1 (Critical) — hang on upstream-EOF-before-first-byte.** When a server-first `tcp_proxy`
>   backend reaches EOF *before* the client's first downstream byte, the banner branch awaits the
>   first-byte gate *after* the upstream has already closed, holding the downstream write half open
>   indefinitely for a client that then stays passive. The client never sees the FIN, the connection
>   task blocks forever, and the `upstream_cx_active` guard stays held. The **non-gated** `relay` path
>   tears the same connection down promptly (ADR-0016 `select!`) — proven by a control probe — so
>   this is an internal liveness regression, a divergence from `enable_half_close: false`, and it
>   **contradicts the BEHAVIOR_CONTRACT item-13 "PLAINTEXT = FULL PARITY" claim**. *Deterministically
>   reproduced this session.*
> - **I-1 (Important) — silent upstream-byte loss at the `Admitted(Some(b))` transition.** The
>   phase-1 banner `tokio::io::copy(ur, dw)` future is *dropped* when the gate wins the biased select,
>   then phase-2 starts a *fresh* copy on the same `ur`. If the dropped future was parked mid-write,
>   its internal `CopyBuffer` (read-but-unwritten upstream bytes, ≤ tokio's 8 KiB) is discarded — a
>   silent data-loss window for full-duplex traffic. *The mechanism is deterministically proven
>   (probe #3: exactly the buffered bytes vanish); end-to-end reproduction over real sockets did NOT
>   trigger it in 18 trials up to 64 MiB (the parked-mid-write instant has a narrow window), so the
>   trigger is narrow but the corruption is real and silent.*
> - **I-2 (Important) — the `Admitted(Some(b))` re-inject + duplex-payload branch is behaviourally
>   unverified.** Every ALLOW witness is byte-less (`None`); every first-byte witness is DENY. No test
>   sends an *allowed* first byte through `[rbac(ALLOW), tcp_proxy]` and asserts the re-injected byte
>   + subsequent bidirectional payload arrive intact. The single most intricate branch is untested —
>   which is exactly why C-1 and I-1 shipped green.
>
> Everything else is in good shape: the `FirstByteGate` extraction is clean and preserves echo/hcm
> byte-for-byte (verified — they inherit the non-consuming `evaluate_peek` default); the
> `connect_upstream`/`relay` split preserves ADR-0016 posture and the `cx_active`/`cx_total`
> placement; the config narrowing to `transport_socket.is_some()` is a sound TLS proxy (envoy-rust
> rejects every non-`tls` transport-socket name, so a validated chain with a transport socket is
> necessarily TLS); the DENY-withholds-the-byte and FIN-matrix witnesses test what they claim and
> incorporate the `connect_when_ready` counter-pollution fix (memory
> `wait-ready-probe-pollutes-tcp-proxy-counters`). No security or data-durability issue; no other
> Critical.
>
> Per §5.2, the next session **re-enters at state-3** (fix C-1 + I-1 + close I-2's gap under TDD —
> add the failing witnesses first), **not state-4**. Per §5.1 / ADR-0127 this session does NOT chain
> into the fix.

---

## §1. What the phase is (code-read, verified at `HEAD`)

- **`envoy_listener::GateOutcome`** (`crates/envoy-listener/src/lib.rs:138`): `ClientGoneEarly` /
  `SkippedCleanly` / `Admitted` / `Denied`. `Copy`, `Eq`. Clean.
- **`envoy_listener::FirstByteGate`** (`:158`): owns `Arc<[Arc<dyn NetworkFilter>]>`; `run` (no I/O,
  first `StopIteration` denies), `run_for_test`, `evaluate_peek` (non-consuming `peek`; `Ok(0)` ⇒
  `SkippedCleanly` — no eval), `evaluate_read_half` (consuming one-byte `read`; `Ok(0)` ⇒ STILL
  evaluates, byte `None`; a real byte returned `Some(b)` for re-injection). The `NetworkFilter` shape
  is unchanged and no payload is exposed — CF-67-3 stays deferred. **Verified** the two front-ends
  encode the D3 FIN asymmetry as a handler property (which front-end the terminal uses), not a
  name-check.
- **`ConnectionHandler::handle_gated`** default (`:58`): `self: Arc<Self>`; peek-gate → `handle`;
  `SkippedCleanly | Denied` → `close_with_drain`; `ClientGoneEarly` → drop. Dyn-safe. echo/hcm
  inherit it ⇒ **byte-for-byte unchanged** (fixtures `0072`/`0073` need no edit — confirmed unedited).
- **`ChainHandler::handle`** (`:313`): now builds a `FirstByteGate` and delegates to
  `inner.handle_gated`. The old inline peek/loop is gone (moved verbatim into the gate). Observationally
  identical for the default path.
- **`TcpProxy::connect_upstream` + `relay`** (`crates/envoy-tcp/src/lib.rs:81`, `:141`): the
  establishment/data split. `UpstreamConn` carries the RAII `_cx_guard`. ADR-0016 `select!` posture
  and the `cx_active`/`cx_total` placement preserved exactly; `handle::<S>` now composes the two. The
  11 pre-existing regression tests pass unchanged.
- **`TcpProxy::handle_gated` override + `relay_gated`** (`:265`, `:288`): connect upstream at
  establishment, then race the banner (upstream→downstream copy) against the first-byte gate on the
  split downstream read half; branch on the outcome. **This is where C-1 lives (§3).**
- **Config narrowing** (`crates/envoy-config/src/bootstrap.rs:3233`, `lib.rs:130`): the
  `UnsupportedNetworkFilterChainComposition` rejection gains `&& chain.transport_socket.is_some()`,
  so plaintext `[rbac, tcp_proxy]` validates and only TLS-downstream stays fail-loud (re-messaged to
  name CF-67-7). Sound (§4).

---

## §2. Strengths

- **The gate extraction is the right abstraction.** Pulling the peek+filter loop into a
  filter-owned `FirstByteGate` with two explicit front-ends (`evaluate_peek` vs `evaluate_read_half`)
  makes the D3 FIN asymmetry fall out of *which* front-end a terminal uses, exactly as ADR-0135 W-3
  prescribes — no filter-name special-casing, no `NetworkFilter` shape change.
- **echo/hcm parity is structurally guaranteed, not just tested.** They inherit the default
  `handle_gated`, whose body is the pre-67.3 `ChainHandler` peek verbatim. Fixtures `0072`/`0073`
  and `known-failures.txt` are unedited (verified by `git diff --name-only`).
- **DENY-withholds-the-byte is correctly implemented and witnessed.** `evaluate_read_half` returns
  the byte as `Some(b)`; `relay_gated`'s `Denied` arm drops the upstream and `close_with_drain`s the
  downstream without ever writing `b` to `uw`. `deny_delivers_banner_then_closes_without_forwarding_the_byte`
  (in-process) + `deny_before_tcp_proxy_delivers_banner_then_withholds_the_byte` (envoy-bin, asserts
  the recording backend never saw the byte) pin it. W-4/R-2 honored.
- **The counter-pollution trap was found and fixed during implementation.** The envoy-bin backstops
  use a new `connect_when_ready` (retry-connect that KEEPS the stream) instead of `wait_ready` on the
  data listener, so the test client is the sole data connection — matching memory
  `wait-ready-probe-pollutes-tcp-proxy-counters`. The FIN-matrix backstop correctly contrasts
  `tpf.rbac.allowed == 1` (tcp_proxy) against `ef.rbac.allowed == 0` (echo).
- **The config narrowing is a faithful TLS proxy.** `transport_socket.is_some()` is sound because
  `rejects_unknown_transport_socket_name` (`bootstrap.rs:7965`) proves envoy-rust accepts only the
  `envoy.transport_sockets.tls` name; any other transport socket is already rejected, so a validated
  chain carrying a transport socket is necessarily TLS-downstream. No false-accept, no false-reject
  of a genuinely-plaintext chain. The precedence guard (`terminal_not_last_wins_for_echo_rbac_tcp_proxy`)
  and the over-rejection guards survive.

---

## §3. Issues

### Critical (Must Fix)

**C-1. `relay_gated` hangs the connection when the `tcp_proxy` upstream reaches EOF before the
client's first downstream byte.**

- **File:** `crates/envoy-tcp/src/lib.rs:314-321` (the `r = &mut banner =>` branch of the phase-1
  `select!` inside `relay_gated`; the offending `gate_fut.await?` is at `:320`).
- **What's wrong.** The phase-1 `select!` races the banner copy (`copy(ur, dw)`) against the gate
  (`evaluate_read_half(dr)`). When the upstream closes *before* the client sends its first byte, the
  banner copy completes (upstream EOF) and the **banner branch** is taken. That branch does
  `gate_fut.await?` — it *blocks on a downstream read that never resolves* for a client that received
  the banner and then stays passive (sends neither a byte nor a FIN). The downstream write half `dw`
  is a live local, dropped only at function end, so **the function never returns, the client never
  sees the FIN, the connection task blocks indefinitely, and the `_cx_guard` (`upstream_cx_active`)
  stays held.** The branch's own comment claims "`dw` is dropped at scope end → client sees FIN," but
  the `gate_fut.await` *is* the scope, so scope-end is never reached.
- **Why it matters.** (a) **Liveness / resource leak.** A connection that should close stays open
  forever (until the client independently closes), pinning a task and the active-connection gauge.
  (b) **Divergence from ADR-0016.** With `enable_half_close: false`, either side's FIN tears the
  whole connection down. The **non-gated** `relay` path implements this via its `select!`. The gated
  path breaks it. (c) **Divergence from upstream Envoy**, which closes the downstream when the
  upstream closes. (d) **Contradicts the BEHAVIOR_CONTRACT item-13 "PLAINTEXT = FULL PARITY" claim**
  landed this phase. (e) It is a **narrower re-appearance of exactly the hang** that 67.1's fail-loud
  rejection existed to prevent, on the composition this phase re-enables. The PLAN flagged this exact
  race as the risky part (`PLAN.md:403`: *"the 'upstream EOFs before the client's first byte' race"*);
  the implemented handling is incorrect.
- **Repro (live-probed this session; throwaway test deleted, tree clean).** A server-first backend
  that writes `220 BANNER\r\n` then closes immediately, a `[rbac(ALLOW), tcp_proxy]` chain, a client
  that reads the banner then sends nothing:
  - **gated path** (`ChainHandler` → `relay_gated`): client does **not** observe EOF within 2s →
    **hang confirmed** (the read times out).
  - **control, non-gated path** (lone `Arc<TcpProxy>` → `relay`): client observes EOF promptly →
    "correctly tore down on upstream EOF".
  The two paths, same backend, opposite outcomes — isolating the defect to `relay_gated`'s banner
  branch, not the backend or the harness.
- **Why the gate is green anyway.** No witness exercises upstream-EOF-*before*-first-byte. The banner
  witnesses keep the backend's read loop alive (it never closes first);
  `dataless_fin_through_rbac_allow_reaches_backend_as_eof` has the *client* close first, not the
  upstream. The anticipated race (`PLAN.md:403`) shipped without a witness.
- **How to fix (for the state-3 re-entry — do NOT fix in this review session).** In the banner
  (upstream-EOF) branch, do **not** `await` the gate. Upstream is already gone; under
  `enable_half_close: false` the connection must be torn down, and per ADR-0131 case C a client that
  never sent a byte is never evaluated — so RBAC-on-a-later-byte is moot (there is no upstream left to
  forward to). Tear down immediately: drop the halves (client sees FIN) and return `Ok(())`. Add the
  failing witness first (the probe above), confirm it hangs against current code, then repair.

### Important (Should Fix)

**I-1. `relay_gated` can silently drop buffered upstream bytes at the `Admitted(Some(b))`
phase-1→phase-2 transition.**

- **File:** `crates/envoy-tcp/src/lib.rs:309` (phase-1 `banner = tokio::io::copy(&mut ur, &mut dw)`,
  dropped at the select block end) → `:348` (phase-2 `tokio::io::copy(&mut ur, &mut dw)`, a fresh
  copy on the same `ur`).
- **What's wrong.** When the gate wins the biased select (the client's first ALLOW byte), the phase-1
  banner copy future is dropped. `tokio::io::copy` owns an internal `CopyBuffer` (default 8 KiB); if
  the dropped future was parked **mid-write** (it had read a chunk from `ur` but the write to `dw` was
  `Pending` under client backpressure), those read-but-unwritten bytes are discarded with the buffer.
  Phase-2 then starts a *fresh* copy that reads the *next* bytes from `ur` — so the buffered bytes are
  **silently lost**. `[rbac, tcp_proxy]` is a generic byte proxy, so any full-duplex protocol where
  the upstream is streaming when the client sends its first byte is exposed.
- **Why it matters.** Silent data corruption on a proxy data path is worse than a visible failure —
  no error, no counter, no log. Bounded per occurrence (≤ one `CopyBuffer`, ~8 KiB) but unbounded in
  aggregate across connections.
- **Evidence (measured this session — reviewer hypothesis, verified, tree clean).** The **mechanism
  is deterministic** (probe #3: drive a `copy` to a parked mid-write state — writer accepted 4096 of
  an 8192-byte chunk, then `Pending` — drop it, restart a fresh `copy` on the same reader; result:
  **exactly 4096 bytes lost**, contiguity gap confirmed). The **end-to-end trigger is narrow**: an
  8 MiB and a 64 MiB real-socket flood with a non-reading client (probe #2, 18 runs) delivered every
  byte — on real loopback the parked-mid-write-with-buffered-data instant did not coincide with the
  drop. So: real mechanism, narrow real-world window. Honest severity is **Important** (elevate to
  Critical if the deployment expects large server preambles / slow-reading full-duplex clients).
- **How to fix (state-3 re-entry).** Do not drop-and-restart the upstream→downstream copy. Keep the
  **same** `copy(ur, dw)` future alive across phase 1 and phase 2 — e.g. in the `Admitted(Some(b))`
  branch, `select!` the already-running banner copy against `copy(dr → uw)` instead of starting a new
  `copy(ur, dw)`. Add a gated-ALLOW-with-payload witness (I-2) that would have caught it.

**I-2. The `Admitted(Some(b))` re-inject + bidirectional-payload branch has no behavioural test.**

- **File (test gap):** `crates/envoy-tcp/src/lib.rs` `#[cfg(test)]` + `crates/envoy-bin/tests/network_filter_rbac.rs`.
- **What's missing.** Every ALLOW witness is byte-less (exercises the `None` / data-less-FIN branch);
  every first-byte witness is DENY (the byte is withheld, so the re-inject + duplex-copy path at
  `:340-354` never runs). No test sends an *allowed* first byte through `[rbac(ALLOW), tcp_proxy]` and
  asserts (a) the re-injected first byte reaches the backend in order, and (b) subsequent client
  payload and a return payload both flow intact. `proxies_payload_end_to_end` covers only the
  non-gated `handle`.
- **Why it matters.** The single most intricate branch — the one the whole banner/gate/re-inject
  dance exists to serve — is unverified at the behavioural level. Both C-1 and I-1 shipped green
  precisely because this path is untested. Add the witness at the state-3 re-entry (it is also the
  natural regression guard for the I-1 fix).

### Minor (Nice to Have)

**M67.3-1. `SkippedCleanly` is unreachable in `relay_gated` (dead arm, harmless).**
`evaluate_read_half` returns only `ClientGoneEarly` / `Denied` / `Admitted` — never `SkippedCleanly`
(that outcome is produced only by `evaluate_peek`). The `SkippedCleanly | Denied` arm at
`crates/envoy-tcp/src/lib.rs:327` therefore never matches `SkippedCleanly` on this path. Defensively
grouped with `Denied` and harmless, but a one-line comment noting the arm is `Denied`-only-here (or a
split with `unreachable!`) would prevent a future reader assuming a data-less FIN can reach it.

**M67.3-2. The item-13 "FULL PARITY" phrasing over-claims (resolved by the C-1 fix).**
`BEHAVIOR_CONTRACT.md:425` asserts plaintext `[rbac, tcp_proxy]` is "FULL PARITY." C-1 (and, more
narrowly, I-1) are live counter-examples. Once fixed the phrasing becomes accurate; if any fix is
deferred, narrow the row to record the divergence (invariant 4.1.5 "never silently"). Tracked by C-1.

**M67.3-3. Config-narrowing legibility: prefer the precise TLS predicate over `is_some()`.**
`crates/envoy-config/src/bootstrap.rs:3233` uses `chain.transport_socket.is_some()`. This is
*correct* today only because the earlier block (`:3136`) already `return`s `UnknownTransportSocketName`
for any non-`tls` transport-socket name and `MismatchedTransportSocketDirection` for a non-Downstream
context, so control reaches `:3233` only for a valid TLS-downstream socket. The chain already
computes `chain_has_tls` (`:3132`) with the exact `name == TLS_TRANSPORT_SOCKET` predicate; using
that here would be self-documenting and immune to a future reordering of the two blocks. Low-risk
robustness cleanup.

**M67.3-4. `ClientGoneEarly` drops the underlying I/O error (minor diagnostic regression).**
`crates/envoy-listener/src/lib.rs:199,220` (`evaluate_peek`/`evaluate_read_half`) map `Err(_) =>
ClientGoneEarly`, discarding the error; the default `handle_gated` then logs a generic message with
no `error = %err` field. The pre-67.3 `ChainHandler` logged the error. Consider threading it through
for parity of diagnostics.

**M67.3-5. Swallowed results on the data-less-FIN ALLOW drain; broken-pipe-on-re-inject log noise.**
`crates/envoy-tcp/src/lib.rs:359-360` discards `uw.shutdown()` and the drain `copy` results (the
non-gated `relay` propagates `CopyFailed`) — acceptable on a teardown drain but inconsistent, and a
genuine upstream error there is invisible. Separately, an upstream reset between establishment and
the `uw.write_all(&[b])` re-inject (`:342`) propagates `CopyFailed`, which `accept_loop` surfaces at
`warn!` as "connection task failed"; real Envoy treats an upstream reset as a normal close (UF), so
this is log noise, not a correctness issue. Both cosmetic.

---

## §4. Contract & invariant conformance (spot-checks)

- **§7.5 acceptance frame** — all six (a)-(f) gates are GREEN per PROGRESS.md's state-4 section
  (build/clippy `-D warnings`/fmt/deny EXIT 0; `cargo test --workspace --no-fail-fast` 1947 passed +
  6 documented CI-authoritative host-flake REDs; local `1947+6 == CI 1953 passed`). This review does
  not re-run the gate (it is the acceptance frame, already met); it reviews code quality + contract
  conformance, and C-1 is a gap the gate cannot see.
- **Standing traps — all honored.** `is_terminal_network_filter` untouched; `filters: []` still
  accepted; `direct_response` bypass intact (not re-wrapped); ADR-0131 first-byte verdict preserved;
  ADR-0016 `select!` + `cx_active`/`cx_total` placement preserved in `connect_upstream`/`relay`;
  ADR-0124 `close_with_drain` + both `post_eof_*` tests unweakened; `rbac.rs` untouched (item 14 /
  ADR-0133 not re-litigated); `tls_handler.rs` untouched (D6 keeps TLS rejected); differential surface
  `0001`/`0071`/`0072`/`0073` + `known-failures.txt` unedited; `#![forbid(unsafe_code)]` holds.
- **Carry-forwards.** M-1 not consumed (67.3 doesn't touch CidrRange); CF-67-6 not folded (D8
  opportunistic); CF-67-7 correctly opened for the TLS composition. Unchanged.

---

## §5. Independent adversarial subagent

An independent `general-purpose` subagent reviewed the same range for `relay_gated`'s branch logic,
error handling, guard lifetimes, and the config narrowing. It **independently reproduced C-1**
(upstream-EOF-first delays FIN indefinitely) and **raised I-1** (the `Admitted(Some(b))` copy
drop-and-restart data-loss window) and **I-2** (the untested re-inject/duplex branch), all folded
into §3 above. It confirmed the config narrowing is *correct* but fragile (M67.3-3), and flagged the
Minors M67.3-4/M67.3-5. It found **no security, data-durability, or additional Critical** issue.
This session did not merely accept its I-1 hypothesis: I-1 was **measured** — the mechanism confirmed
deterministically (probe #3) and the end-to-end trigger characterised as narrow (18 real-socket runs,
no loss). That measurement is *why* I-1 is graded Important rather than Critical (memory
`state5-must-probe-untested-compositions`: measure reviewer hypotheses; here the mechanism was real
but its real-world window narrow).

---

## §6. Assessment

**Ready to merge? No — with fixes.** One Critical (C-1: the gated path hangs on
upstream-EOF-before-first-byte) and two Important (I-1: a proven silent-data-loss mechanism at the
`Admitted(Some(b))` copy transition, narrow real-world trigger; I-2: the re-inject/duplex branch is
behaviourally untested — the gap that let C-1 and I-1 ship green). The rest of the phase — the gate
abstraction, the establishment/data split, echo/hcm parity, DENY-withholds, the FIN matrix, the
config narrowing — is well-built and contract-conformant. Fix C-1 + I-1 and close I-2's gap under TDD
at a §5.2 state-3 re-entry (add the failing/duplex witnesses first), then re-verify (state-4) and
re-review (state-5). Per §5.1 / ADR-0127 this session does not chain into the fix.
