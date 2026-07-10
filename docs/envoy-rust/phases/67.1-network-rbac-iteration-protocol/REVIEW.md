# Phase 67.1 — state-5 CODE REVIEW

> Written by the §5 **state-5 code-review** session (`superpowers:requesting-code-review`), per
> `BOOTSTRAP_PROMPT.md` §5 state 5 and `SKILL_ROUTING.md`. Output of this session is this file.
>
> **Review surface:** 15 commits, `f40a41e..cd874f6` (plus the docs-only `1a5afd2`).
> **Method:** three independent `general-purpose` reviewer subagents (runtime / config / harness+tests),
> each given the SPEC, the governing ADRs, and an explicit list of settled decisions they were
> forbidden to re-litigate — plus **first-hand adjudication and live measurement by this session**.
> Every finding below that this session could verify, it verified; the verification is quoted inline.

---

## VERDICT

> ### **NOT APPROVED. §7.5 gate (f) is NOT satisfied.**
>
> Per `BOOTSTRAP_PROMPT.md` **§5.2**, this phase **re-enters the lifecycle at state 3
> (implementation + TDD)**, *not* at state 4. §6.1's mid-execution split valve stays armed for that
> re-entry.

**One Critical finding blocks approval.** It is a **measured, reproducible, cross-proxy behavioral
divergence** on configs that *both* proxies accept, in which envoy-rust withholds a response payload
that upstream Envoy delivers — up to and including a **permanent hang**. It is invisible to the
entire §7.5 (a)-(e) gate because **no fixture and no backstop exercises the affected code path.**

This is not a regression against the tests that ran; those tests are green and honest. It is a gap
between what was *measured* (`[rbac, echo]`) and what was *shipped* (a first-byte gate in front of
**all four** terminal filters).

---

## §1. Strengths (accurate, and load-bearing to trust the rest)

These are real, and several are unusually good:

1. **`ChainHandler` is the right abstraction.** Wrapping an arbitrary `Arc<dyn ConnectionHandler>`
   (`crates/envoy-listener/src/lib.rs`) means one implementation covers every terminal filter with no
   per-arm special-casing, and `wrap_in_chain` (`crates/envoy-bin/src/main.rs:986`) returns `inner`
   untouched for an empty prefix, so a lone-terminal chain pays zero per-connection cost. Pinned by
   `wrap_in_chain_with_no_filters_returns_inner_unchanged`.

2. **The accept-loop hoist is a genuine net simplification.** Two standalone, non-reaping accept
   loops were **deleted**, not patched; `echo` and `direct_response` became plain
   `ConnectionHandler`s, and all four terminal filters now flow through the one loop that already
   reaps. `echo` remained "the structural model" — the joint-repair unit the phase-66 review demanded.

3. **The M66-3 reaping half is witnessed by a test that could not otherwise pass.**
   `sequential_connections_do_not_accumulate_joinset_tasks` reads `pending_tasks_watch()`, and the
   session correctly identified that `cx_active` **cannot** witness reaping (it is decremented
   *inside* the spawned task, so it reads 0 while a completed `JoinSet` entry lingers). Deleting the
   `join_next()` select arm makes it fail with `JoinSet leaked 50 completed tasks`. That is a real
   mutation check on a real invariant.

4. **The vacuity defense on fixture `0072` is correctly built AND correctly documented.** SPEC R-8
   identified that `ByteExact` is a bare `envoy_body != rust_body` check, so a DENY fixture asserting
   "both returned zero bytes" would pass against an envoy-rust that never implemented RBAC. The
   `expected_stats` extension closes it, and `expectations.yaml` **names which single assertion is
   the witness** (`rbac_deny.rbac.denied == 1`) and which three are mere consistency checks
   (`value: 0` passes vacuously for an unregistered name). Reviewers confirmed the delta refactor
   (ADR-0131 decision 4) **preserves** the witness rather than weakening it.

5. **The ADR-0124 drain survived the restructure intact,** factored into
   `envoy_listener::close_with_drain` and pinned at **two** layers
   (`post_eof_client_write_is_accepted_not_reset` and `deny_post_eof_client_write_is_accepted_not_reset`).

6. **`rules: None ⇒ INERT` is modelled exactly as measured** (no default `Rules` materialised), and
   the in-process backstop `rules_omitted_is_inert_neither_counter_ticks` requires the counters to be
   **registered** while asserting they never tick — a strictly stronger check than the differential
   path, which cannot distinguish absent from zero.

7. **Exhaustive matching with no `_ =>` catch-all** in `permission_matches` / `principal_matches` /
   `validate_l4_permission` / `validate_l4_principal`, verified by an `E0004` probe. The compile break
   is the intended forcing function for `67.2`.

8. **Error precedence (R-5) is correctly implemented and non-vacuously pinned.** The terminal-not-last
   scan precedes the chain-termination check, and
   `terminal_not_last_error_wins_over_chain_not_terminated` asserts the *specific variant*, so it
   would fail if the two checks were reordered.

9. **The intellectual honesty of the state-3 and state-4 records is exemplary** — in particular the
   "a mutation check can lie" finding (a stale test binary produced a false PASS) and the state-4
   refusal to accept a cached `Finished` as evidence. That discipline is what makes the rest of the
   evidence credible, and it is what this review extended.

---

## §2. Issues

### CRITICAL — must fix before this phase can land

#### **C-1. `ChainHandler` gates the TERMINAL filter behind the first downstream byte. Upstream Envoy does not. Measured divergence on four accepted configs, including two permanent hangs.**

**Files:** `crates/envoy-listener/src/lib.rs` (`ChainHandler::handle`, the `peek` block);
`crates/envoy-bin/src/main.rs:263`, `:294-297`, `:357`, `:717` (`wrap_in_chain` applied to all four
terminal arms).

**What ADR-0131 measured.** Chain `[rbac(DENY, any), echo]`, four client behaviors. It concluded:
upstream evaluates RBAC on the **first downstream byte** (`ONE_TIME_ON_FIRST_BYTE`); a byte-less
connection is never evaluated and ticks no counter. **That conclusion is correct.**

**What was implemented.** `ChainHandler::handle` awaits `TcpStream::peek()` **before running any
filter *and* before delegating to `inner`** — the terminal `ConnectionHandler`. So the first-byte
wait gates *the whole chain*, terminal filter included.

**Why `[rbac, echo]` cannot detect the error.** `echo`'s establishment-time behavior is *nil* — it
only ever reacts to data. Under both models it looks identical. The measurement generalized to a
population of one.

**The measurement this session performed.** Pinned image
`envoyproxy/envoy:v1.33.0@sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`
(D-3.7), booted under `docker run -p` (per memory `state0-recon-docker-needs-port-mapping`); backend
for the `tcp_proxy` case run as a **sibling container** on a shared docker network (per memory
`state0-recon-backend-sibling-container`). envoy-rust is `target/debug/envoy-bin` built at `cd874f6`.
Raw sockets driven from Python; `/stats` scraped over the mapped admin port. All containers removed
afterwards.

**Upstream Envoy — measured:**

| # | Chain | Client behavior | Result | Counters |
|---|---|---|---|---|
| U1 | `[rbac(ALLOW, any), direct_response]` | connect; **send nothing**; read | **`HELLO-DR\n` delivered** | `allowed 0`, `denied 0` |
| U2 | `[rbac(DENY, any), direct_response]` | connect; **send nothing**; read | **`HELLO-DR\n` + clean EOF** | all four `0` |
| U3 | `[rbac(DENY, any), direct_response]` | connect; send `X`; read | **`HELLO-DR\n` + clean EOF** | all four `0` |
| U4 | `[rbac(ALLOW, any), tcp_proxy]` → server-speaks-first backend | connect; **send nothing**; read | **`220 BANNER-FIRST\n` delivered** | `allowed 1`; `cluster.be.upstream_cx_total 1` |

**envoy-rust @ `cd874f6` — measured on the same chains:**

| # | Chain | Client behavior | Result |
|---|---|---|---|
| R0 | `[direct_response]` alone (**control**) | connect; send nothing; read | `HELLO-DR\n` ✅ (matches upstream; fixture `0071` unaffected) |
| R1 | `[rbac(ALLOW, any), direct_response]` | connect; **send nothing**; read | ❌ **no bytes; connection stays open (3 s timeout)** |
| R2 | `[rbac(DENY, any), direct_response]` | connect; **send nothing**; read | ❌ **no bytes; connection stays open** |
| R3 | `[rbac(DENY, any), direct_response]` | connect; send `X`; read | ❌ **zero bytes + clean EOF** (upstream sends the payload) |
| R4 | `[rbac(ALLOW, any), tcp_proxy]` → server-first backend | connect; **send nothing**; read | ❌ **banner never delivered (3 s timeout)** |
| R5 | `[rbac(ALLOW, any), tcp_proxy]` → server-first backend | connect; send `X`; read | `220 BANNER-FIRST\n` (control: the peeked byte is **not** consumed — `peek` is correct) |

**The mechanism, stated plainly.** Upstream Envoy runs **every** filter's `onNewConnection` at
connection establishment — including the *terminal* filter's, which is where `direct_response` writes
its payload and where `tcp_proxy` initiates the upstream connection. RBAC's **decision** is deferred
to the first downstream byte, but **RBAC does not gate the terminal filter's establishment-time
work.** envoy-rust's `ChainHandler` conflates *"when RBAC decides"* with *"when the connection is
handed to the terminal filter"*, and therefore stalls the terminal filter too.

Note U2/U3: with `direct_response` terminal, upstream RBAC **never enforces at all** — the terminal
filter writes and closes before any downstream byte can arrive, so `onData` never fires and no
counter ticks. A DENY policy does **not** suppress the payload. envoy-rust suppresses it.

> **Caveat, recorded rather than smoothed over.** In U4 the `allowed: 1` tick was scraped *after* the
> client closed its socket, so the event that triggered the evaluation (a first byte vs. the client's
> FIN) is **not disambiguated by this measurement**. The load-bearing, unambiguous part of U4 is that
> **the banner was delivered and `upstream_cx_total` reached 1 before the client ever sent a byte** —
> envoy-rust delivers neither. The fix session should disambiguate the tick trigger before asserting
> anything about *when* `allowed` increments on a `tcp_proxy` chain.

**Why it matters.**
- **D-3.3** makes differential correctness the ship criterion. These are four divergences on configs
  both proxies accept and neither rejects.
- Two of them (R1, R2, R4) are **hangs**: the client waits for a payload/banner forever while
  envoy-rust waits for a byte the protocol says the *server* speaks first. For `tcp_proxy` that means
  every server-speaks-first upstream protocol (SMTP, MySQL, FTP, Redis push, PostgreSQL) deadlocks
  behind a network `rbac` filter.
- R3 is a **silent wrong answer**: the payload is dropped rather than delivered.
- **`ADR-0130` Decision 2 explicitly claimed** this shape *"works **uniformly for all four** terminal
  filters — `[rbac, echo]`, `[rbac, direct_response]`, `[rbac, tcp_proxy]`, `[rbac, http_connection_manager]`
  — with no per-arm special-casing."* That claim was true when written; the ADR-0131 peek, added
  later, **falsified it**, and no session re-checked the other three arms.
- It is **structurally the same error as the one ADR-0131 was created to correct** — a first-byte
  inference generalized from probes that all happened to share one incidental property. ADR-0131
  fixed it one level down (every probe sent a payload); this fixes it one level up (every probe used
  `echo`).
- `direct_response`'s own module doc (`crates/envoy-bin/src/direct_response.rs:4-6`) states the filter
  writes its payload *"IMMEDIATELY — without reading or waiting for any client bytes."* Under
  `ChainHandler` that documented contract is now false. envoy-rust is **self-inconsistent**, before
  any comparison with upstream.

**Why the §7.5 (a)-(e) gate did not catch it.** Verified this session:

```
$ grep -h "^ *- name: envoy.filters.network" tests/fixtures/0072-*/*.yaml tests/fixtures/0073-*/*.yaml | sort | uniq -c
      4             - name: envoy.filters.network.echo
      4             - name: envoy.filters.network.rbac
$ grep -h "name: envoy.filters.network" crates/envoy-bin/tests/network_filter_rbac.rs | sort | uniq -c
      2 - name: envoy.filters.network.echo
      3 - name: envoy.filters.network.rbac
```

**`rbac` is paired with `echo` in every fixture and every backstop, and with nothing else, anywhere.**
The three untested combinations are exactly the three broken ones. The gate is green and truthful; it
simply never asked the question.

**How to fix (design is the state-3 session's to make, not this review's).** The shape the measurement
implies: the terminal handler must be started at connection establishment, and the RBAC verdict must
be applied at first-byte *to the already-running connection* — i.e. the first-byte wait belongs to the
**filter's decision point**, not to the **chain's hand-off point**. Options the fix session should
weigh and record in an ADR:
- Run `inner.handle()` immediately and evaluate the chain concurrently on first byte, closing the
  connection (via `close_with_drain`) if a filter returns `StopIteration`. Note U2/U3 show upstream's
  `direct_response` closes first and RBAC never runs — so a race here must be resolved to match, not
  merely to avoid a hang.
- Or make the first-byte gate a per-filter property rather than a chain property, so a chain whose
  terminal filter speaks first is not stalled.
- Either way, **probe upstream before choosing**, and probe **all four** terminal filters. That is the
  lesson of both ADR-0131 and this finding.

**Required alongside the fix:**
- A **differential fixture or in-process backstop for `[rbac, direct_response]`** (the cheapest
  witness — it needs no backend) and one for `[rbac, tcp_proxy]` against a server-speaks-first
  backend. Without them the same class of bug reappears at `67.2`.
- **`ADR-0132`** (unreserved; ledger head is `ADR-0131`) recording the measurement above, the chosen
  model, and explicitly **superseding `ADR-0130` Decision 2's "uniformly for all four" claim**.
  Per D-3.5 the ADR is appended, never edited into ADR-0130.
- A correction to `BEHAVIOR_CONTRACT.md`'s network-filter rows, which currently describe the
  first-byte rule as a property of the *chain*.

---

### IMPORTANT — should fix

#### **I-2. The W-1 "no `HCM listener` for a network filter" fix is incomplete on a reachable path, and the comment asserting otherwise is factually inverted.**

**File:** `crates/envoy-config/src/lib.rs:505-514` (the comment) and `:571`
(`RbacMetadataMatcherInvalid`, still rendering `"HCM listener {listener:?}: …"`).

The comment claims `RbacMetadataMatcherInvalid` is unreachable from a network `rbac` filter *"because
a network rbac filter's `metadata` leaf is rejected outright by `validate_l4_permission` (67.1 D3)
before that error can be reached."* **The ordering is the reverse.** In `validate_network_rbac_config`
(`bootstrap.rs:3927`), `validate_rbac_rules` runs **first**, and it validates `Metadata` leaves via
`validate_metadata_matcher`; only *then* does the L4 allow-list walk run (`:3931`). So a
**structurally invalid** metadata leaf (empty `filter`, or a multi-segment `path`) raises
`RbacMetadataMatcherInvalid` before the L4 walk ever sees it.

**Verified this session** against `target/debug/envoy-bin` at `cd874f6`, with a network `rbac` filter:

```
# metadata leaf with an EMPTY `filter`  -> the shared tree validator fires first
error=HCM listener "l0": RBAC policy "p0" metadata matcher at permissions[0] is invalid:
      metadata matcher `filter` must not be empty

# metadata leaf that is structurally VALID -> the L4 allow-list fires, correctly scope-neutral
error=listener "l0": network rbac policy "p0" uses matcher "metadata" at permissions[0],
      which cannot be evaluated at L4
```

The first line is exactly the misleading `"HCM listener"` string for a filter with no HCM that W-1
was chartered to eliminate.

**Why it matters.** Low runtime impact — the config is still rejected, fail-loud is preserved, and
§7.4 permits differing error text. But a **load-bearing code comment states a false invariant**, and
future maintainers (notably `67.2`, which widens this exact allow-list) will trust it.

**Fix.** Generalize `RbacMetadataMatcherInvalid`'s message to `"listener {listener:?}: …"` (it stays
accurate for the HTTP filter, whose listener *is* an HCM listener), and correct the comment to list it
as a **seventh** shared scope-neutral variant. **Do not** instead move the L4 walk ahead of
`validate_rbac_rules` — the current order is what bounds tree depth before the L4 recursion runs, and
that stack-safety guarantee is documented and pinned by
`network_rbac_depth_bound_precedes_the_l4_walk`.

#### **I-3. `M66-3` is recorded as CONSUMED, but only one of its two halves was fixed.**

`SPEC.md` §10 defines M66-3 as *"the `JoinSet` non-reaping **+ unbounded per-connection drain**
shared verbatim by `echo.rs:21-59` and `direct_response.rs:36-74`."*

- **Reaping half: genuinely fixed and witnessed.** No dispute.
- **Unbounded-drain half: relocated, not bounded.** `close_with_drain`
  (`crates/envoy-listener/src/lib.rs`) reads until client EOF with **no idle or total timeout**. It is
  bounded *only* at listener shutdown, by `DRAIN_BUDGET` via `accept_loop`'s `abort_all()`. In steady
  state a client that holds a denied connection open — never sending FIN — pins a task and a
  `cx_active` slot for the listener's entire lifetime.

The doc comment *"Bounded by the caller: `Listener::serve`'s `DRAIN_BUDGET` aborts stragglers"* is
true **only on the shutdown path**, which is a narrower claim than it reads as. Upstream Envoy bounds
this with `delayed_close_timeout` (default 1 s), so a bound is also the parity-shaped choice.

**Why it matters.** The carry-forward ledger is the project's memory (D-3.5). Recording M66-3 as fully
consumed retires a defect that is still live, and it is a slowloris-shaped resource-exhaustion surface
on the DENY path this very phase introduces.

**Fix.** Either (a) narrow the ledger entry to "reaping consumed; steady-state per-connection drain
bound deferred as **CF-67-6**", or (b) add an idle timeout to `close_with_drain` — but **do not**
weaken or delete either post-EOF-write test while doing so; the drain itself is ADR-0124 and must
survive.

#### **I-4. `Listener::pending_tasks()` publishes a meaningless value under the SO_REUSEPORT fan-out path.**

In `Listener::serve`'s fan-out branch the single `pending_tasks` `watch::Sender` is `.clone()`d once
per accept loop, and each loop calls `send_replace(join_set.len())` with **its own socket's** count.
`watch::Sender` clones share one channel, so the published value is neither a total across sockets nor
stable — it flaps to whichever loop wrote last. The public doc promises *"in-flight connection tasks
currently held by the accept loop's `JoinSet`"*, a meaning it cannot deliver there. This is also
inconsistent with `bind_shards`, which gives each shard its **own** channel.

The M66-3 witness is unaffected (it uses the single-socket `Listener::bind` path), so this is latent —
but `pending_tasks()` is `pub` and documented as usable for introspection.

**Fix.** Either give each fan-out loop its own watch and expose an aggregated receiver, or document
explicitly that under fan-out the value is per-socket last-writer-wins and **not** a total.

#### **I-5. Zero test coverage of `rbac` composed with any terminal filter other than `echo`.**

This is the process defect that allowed **C-1** to ship. `main.rs` wraps all four terminal arms in
`ChainHandler`; the test suite exercises exactly one. At minimum, `[rbac, direct_response]` deserves an
in-process backstop (no Docker, no backend needed) and `[rbac, tcp_proxy]` a server-speaks-first one.

---

### MINOR — nice to have

- **M-1. The `network_rbac.rs` unit test `shadow_counters_register_at_zero_and_never_tick` is vacuous
  at the registration half.** Its `stat()` helper calls `registry.register_counter(name)`, which is
  **get-or-create** (`crates/envoy-stats/src/registry.rs:45-65`) — so it would *create* the counter and
  read 0 even if `NetworkRbacFilter::new` had never registered it. The *behavioral* half (shadow
  counters never tick) is sound, and the in-process backstop
  `rules_omitted_is_inert_neither_counter_ticks` **does** genuinely pin registration (it scrapes admin
  `/stats` and `panic!`s on an absent name). Consider asserting via a non-creating lookup at the unit
  layer, or note the reliance on the backstop.

- **M-2. Three backstops `read_to_end` without a timeout** (`crates/envoy-bin/tests/network_filter_rbac.rs:167`,
  `:194`, `:266`). The first test wraps its read in a 5 s `tokio::time::timeout`; these three do not, so
  a "never closes" regression hangs CI instead of failing with a useful message. Given C-1 produces
  exactly that failure mode in neighboring configs, this is worth fixing.

- **M-3. No in-process witness that `denied` reaches exactly 1.** In-process, `denied` is asserted
  `== 0` twice but never `== 1`; the positive tick rides entirely on the Docker-gated fixture `0072`. A
  cheap admin scrape in `deny_writes_zero_bytes_and_closes_cleanly_discarding_client_bytes` would give
  the DENY counter a Docker-independent witness.

- **M-4. An unknown filter name in last position now reports `NetworkFilterChainNotTerminated` rather
  than `UnsupportedFilter`,** because the chain-termination check precedes the per-filter allow-list
  loop. Both are fail-loud rejections, and the one affected test (`rejects_unknown_filter_name`) was
  correctly updated rather than weakened — but the diagnostic is now less precise for a bare unknown
  filter. Worth a line in the D2 comment.

- **M-5. `rules: { policies: {} }` is rejected (`EmptyRbacPolicies`), and upstream's behavior for the
  *network* filter was never measured.** SPEC R-3 measured only `rules` **omitted**. Reusing the
  phase-10 check is sanctioned by D1 and consistent with the ADR-0049 fail-loud posture, but the
  network-filter divergence is unrecorded. Either measure it or add a `BEHAVIOR_CONTRACT.md` line.

- **M-6. `ChainHandler::handle` propagates a `peek` error as a task failure.** A client that resets
  before sending its first byte turns `downstream.peek(..).await?` into an `Err`, which `accept_loop`
  logs as a connection-task failure. Harmless but noisy; a reset-before-data is not really a failure.
  Consider treating it like the `Ok(0)` case.

- **M-7. No dedicated test pins the rejection of `action: LOG`, `enforcement_type`, or `delay_deny`.**
  All three are correctly rejected today (by the 2-variant `Action` enum and `deny_unknown_fields`
  respectively), and `shadow_rules` has its own test. A one-line `action: LOG` test would lock CF-67-2's
  boundary.

- **M-8. Only the first listener is served** (`bootstrap.all_listeners().next()`, `main.rs:229`).
  Pre-existing and **not** introduced by 67.1, but the chain refactor now lives inside that
  single-listener block. Noted so a future session does not read it as new.

---

## §3. Findings explicitly considered and REJECTED by this review

Recorded so a future session does not re-raise them, and so the reviewers' constraint list is auditable:

- **"Revert to establishment-time evaluation."** No. ADR-0131 is a *measured* correction and its
  first-byte conclusion for the RBAC *decision* is confirmed by this session's U-series measurements.
  C-1 does not question *when RBAC decides*; it questions *what else was made to wait for it*.
- **"Add a `_ =>` catch-all to `permission_matches` / `principal_matches` / `validate_l4_*`."** No.
  The compile break is the intended forcing function for `67.2`. The `#[allow(clippy::only_used_in_recursion)]`
  on the two `*_matches` fns is deliberate; `conn` must keep its name.
- **"Add `rbac` to `is_terminal_network_filter`."** No. Its absence *is* its non-terminality.
- **"Reject `filters: []`."** No. Upstream accepts it (R-7); that measurement closed M66-5.
- **"Weaken or delete `post_eof_client_write_is_accepted_not_reset` / its DENY twin."** No.
- **"Fix the `echo` `typed_config` asymmetry."** No — ADR-0014 shim, pinned by fixture `0001`.
- **"Trim `known-failures.txt`."** No. h2spec is a local SKIP, not a local pass.
- **A reviewer's hypothesis that upstream Envoy's RBAC returns `StopIteration` from
  `onNewConnection`,** which would have made envoy-rust's behavior *parity* rather than a bug. This
  session **measured it and it is false** (U1-U4). Recorded because it is the natural defense of the
  current code, and it does not survive contact with the pinned image.

---

## §4. What this review did NOT re-run

Per the state-4 record, §7.5 **(a)-(e) are satisfied** and were **not** re-executed here
(`cargo test --workspace` costs ~15 min and re-derives a conclusion already on disk). This review
examined the **code**, not the build. The one thing it *did* execute was targeted, novel measurement
that no prior session performed: live probes of the three untested terminal-filter combinations
against both proxies.

**That distinction is the point of state 5.** A green gate proved the code does what its tests ask.
It could not prove the tests ask the right questions.

---

## §5. Assessment

**Ready to merge?** **No.**

**Reasoning.** The architecture is sound, the abstraction is correct, the tests that exist are
non-vacuous, and the documentation is unusually honest. But `ChainHandler` applies a measured
`[rbac, echo]` behavior to three terminal filters whose behavior was never measured, and on all three
it is wrong — producing two hangs and one dropped payload against upstream Envoy on configs envoy-rust
accepts. The §7.5 (a)-(e) gate cannot see it because `rbac` is never composed with anything but `echo`
anywhere in the suite. That is a Critical, and it is the direct descendant of the same
over-generalization ADR-0131 exists to correct.

**Required before re-entering state 4:**

1. Fix **C-1**; author **ADR-0132** recording the U/R measurement tables and explicitly superseding
   ADR-0130 Decision 2's "uniformly for all four" claim.
2. Add the missing composition tests (**I-5**) — at minimum `[rbac, direct_response]`.
3. Fix **I-2** (the `RbacMetadataMatcherInvalid` message + its inverted comment).
4. Resolve **I-3** — either bound the drain or narrow the M66-3 ledger claim (a carry-forward is an
   acceptable resolution; a silent over-claim is not).
5. Address **I-4**, and the Minors as budget allows.

Per **§5.2** the phase re-enters at **state 3** (implementation + TDD), not state 4. Per **§6.1** the
mid-execution split valve remains armed: if the C-1 redesign blows any single task past ~10 sub-steps,
split rather than push through.

---

## §6. Carry-forward ledger (unchanged by this review, except as noted)

- **This review OPENS no carry-forward.** It proposes **CF-67-6** (the steady-state `close_with_drain`
  bound) *only* as one acceptable resolution of **I-3**; the state-3 session decides and records.
- **CONSUMED by `67.1`:** CF-66-2, M66-3 (**but see I-3 — the drain half is contested**), M66-4,
  CF-67-4, M66-6.
- **CLOSED by recon, no code change:** M66-5.
- **OPENED by ADR-0130:** CF-67-5 (probe upstream's *connection* behavior on an empty `filters: []`
  chain). Still open; blocks nothing. **Note:** C-1's fix will require probing terminal-filter
  establishment-time behavior anyway, which is adjacent — the state-3 session may be able to close
  CF-67-5 for free.
- **STILL LIVE, none blocks:** CF-67-1 (`shadow_rules`), CF-67-2 (`Action::LOG`), CF-67-3
  (payload-visible `on_data`-time iteration + buffering — **scope unchanged**), M66-7, CF-66-1, and the
  long tail recorded in `STATE.md`.
- **DEFERRED to `67.2`:** the connection-level matcher arms + `CidrRange` + the three-site V-1
  shared-enum fallout.
- **Numbering:** M66-1 was never allocated. The ledger advances monotonically and does not backfill.
- **`DECISIONS.md` ledger head: ADR-0131.** Next available: **ADR-0132**, unreserved — **claimed by the
  state-3 re-entry session for the C-1 decision.**
