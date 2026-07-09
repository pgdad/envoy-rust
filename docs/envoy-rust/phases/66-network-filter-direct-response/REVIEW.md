# Phase 66 — `66-network-filter-direct-response` — REVIEW

**Status: APPROVED** — 0 Critical / 0 Important / 5 Minor. The phase does **NOT** re-enter
§5 state 3. The next session is the §5 **state-6 close-out**.

> Produced by the §5 **state-5 code-review** session (`superpowers:requesting-code-review`),
> 2026-07-09. Pick + scope locked by **ADR-0123**; the §6.2 V-3 drain reconciliation by
> **ADR-0124**; the out-of-PLAN test change by **ADR-0125** + **ADR-0126**.
>
> **STEP 0 (disk is authoritative):** `git status --porcelain` clean; branch `main`;
> `HEAD` = `origin/main` = `ba2cb9e9287df482c692419602b038c22683d872` (the phase-66 state-4
> verification commit). `git fetch origin --prune` → `0 0` ahead/behind; no sibling
> autonomous-loop session had written `REVIEW.md`, and ROADMAP row `66` is still
> `in-progress`.
>
> Per `BOOTSTRAP_PROMPT.md` §5.1 (one state per session) the state-6 close-out was
> deliberately **NOT** run here.
>
> **The §7.5 gate was NOT re-run.** It ran at state-4 and it PASSES; its evidence is quoted
> verbatim in `PROGRESS.md` §"§5 state-4 VERIFICATION". This session changed **no code** —
> the review is read-only over the tree (re-confirmed after every subagent returned:
> `git status --porcelain` empty, `HEAD` unmoved).

---

## Verdict

**0 Critical / 0 Important / 5 Minor.** All four reviewers answered the literal
"Ready to merge?" question **"Yes"**, independently and without qualification.

One reviewer filed its `JoinSet`-non-reaping finding as **Important**. That classification is
recorded verbatim below and was **not silently softened**. This session **downgrades it to
Minor (M66-3)** on evidence it gathered itself, and states the reasoning rather than assuming
it:

- `direct_response::serve` (`crates/envoy-bin/src/direct_response.rs:36-74`) is
  **structurally identical** to `echo::serve` (`crates/envoy-bin/src/echo.rs:21-59`) —
  same `JoinSet<()>`, same `tokio::pin!(shutdown)`, same `select!{shutdown | accept}`, same
  `drop(listener); break`, same `timeout(DRAIN_TIMEOUT, join_next-loop)` → `set.shutdown()`
  tail. Neither reaps completed tasks during normal operation. **Both files were read in full
  by this session to confirm it**; the finding is a property of the *pinned structural model*,
  not of code phase 66 authored.
- It is therefore **not a regression this phase introduces**, and it has **no differential
  observable** — no fixture, no upstream-Envoy divergence, no logged value changes.
- Fixing it in `direct_response.rs` alone would make the two network filters **drift apart**,
  destroying the "echo is the model" invariant that ADR-0123 §2.1(B)(6) and the file's own
  module doc both rest on. The correct unit of repair is *both files together*, which is a
  different phase's scope.

**§5.2 disposition: the phase does NOT re-enter state 3.** Stated explicitly rather than
assumed:

- `superpowers:requesting-code-review` prescribes "Fix Critical issues immediately / Fix
  Important issues before proceeding / **Note Minor issues for later**". After adjudication
  there are **zero Critical and zero Important** issues.
- The standing project precedent is the phase-64 `REVIEW.md` (APPROVED at 0 Critical /
  0 Important / 2 Minor → M64-2, M64-3) and the phase-65 `REVIEW.md` (APPROVED at
  0/0/1 Minor → M65-1). In both, the Minors became carry-forwards and the phase closed at
  state-6 without re-entry. §5's *"if issues → back to step 3 … until `REVIEW.md` approved"*
  has never been read as "zero Minors".
- **No Minor below touches a fixture, a logged value, an upstream-Envoy observable, or the
  §7.5 gate.** Three are pre-existing, two are introduced by this phase and are doc-prose or
  test-coverage only.

---

## Method

**Four fresh `general-purpose` subagents with NO prior session context** were dispatched
concurrently over **disjoint slices** of the review range `5e3afb9..ba2cb9e` (15 commits,
20 files, +1938 / −44):

| Reviewer | Slice |
|---|---|
| 1 | `crates/envoy-bin/src/direct_response.rs`, `crates/envoy-bin/src/main.rs` (data plane + dispatch arm) |
| 2 | `crates/envoy-config/src/{lib,bootstrap}.rs` (schema, validate arm, terminal pre-pass) |
| 3 | `tests/differential/src/lib.rs`, fixture `0071`, the differential runner |
| 4 | `crates/envoy-bin/tests/upstream_h2_connection_pooling.rs` (out-of-PLAN, ADR-0125/0126), the fuzz seed, `BEHAVIOR_CONTRACT.md`, `ROADMAP.md` |

Each was handed `SPEC.md` / `PLAN.md` / the governing ADRs / the `## Network filters` contract
section, and each was **explicitly forbidden from re-litigating the six settled decisions**
(the ADR-0124 drain and its mutation check; the `echo` `typed_config` asymmetry; the
`DirectResponseConfig`-vs-route-level-`DirectResponse` naming; the `is_terminal` predicate over
a `len <= 1` check; the immutable pre-pass ordering; BLOCK-66-1). Each was told it **may**
critique *how* a settled decision is implemented. All four were read-only and forbidden from
running `cargo test --workspace`.

Each was also given at least one **high-value adversarial question** designed to fail if the
implementation were merely plausible — e.g. reviewer 3 was asked whether the fixture's
byte-exact comparison would **pass vacuously if both proxies returned zero bytes**, and
reviewer 4 was asked to **independently verify the M66-2 latent-sibling claim** rather than
accept it from ADR-0126.

### Dispatcher-side re-verification (agent reports are not evidence)

Per `superpowers:verification-before-completion`, every load-bearing reviewer claim this
review rests on was **independently re-run by this session**:

| Claim | How re-verified | Result |
|---|---|---|
| `direct_response::serve` ≡ `echo::serve` (so the `JoinSet` finding is pre-existing) | Read both files in full | **CONFIRMED** — identical accept-loop + drain-tail shape |
| ADR-0126's fix is intact: `--quiet` gone, budget unchanged | `grep -n -- '--quiet\|PREBUILD_TIMEOUT\|from_secs(30)'` on the test | **CONFIRMED** — `--quiet` survives only in a comment at `:98` ("deliberately passed nowhere"); `PREBUILD_TIMEOUT = 240s` at `:80`; the 30s readiness budget intact at `:247` |
| M66-2's four latent siblings really share the hazard | `grep -c '"--quiet"'` + `grep -c 'manifest-path'` across all four | **CONFIRMED, and SHARPENED** — all four still pass `--quiet` *and* use a nested `cargo run --manifest-path`. See the M66-2 amendment below. |
| An empty network `filters: []` chain is accepted | Located the *only* `filters.is_empty()` guard (`bootstrap.rs:3618`) and read its enclosing fn | **CONFIRMED** — that guard is `validate_http_filters` (HTTP filters *inside* HCM), reached from `validate_hcm`. No network-chain cardinality guard exists. |
| Tree unmutated by the reviewers | `git status --porcelain`; `git rev-parse HEAD` | **CONFIRMED** — empty; `HEAD` still `ba2cb9e` |

---

## Reviewer verdicts (verbatim "Ready to merge?" answers)

| Reviewer | Answer | Critical | Important | Minor |
|---|---|---|---|---|
| 1 — data plane | **Yes** | 0 | 1 (`JoinSet` non-reaping — downgraded, see Verdict) | 3 |
| 2 — config / terminal validation | **Yes** | 0 | 0 | 4 |
| 3 — driver / fixture `0071` | **Yes** | 0 | 0 | 3 |
| 4 — out-of-PLAN test, fuzz seed, docs | **Yes** | 0 | 0 | 2 |

### Strengths (as found, independently, by the reviewers)

- **The ADR-0124 drain is implemented exactly as mandated and is genuinely pinned.**
  `direct_response_once` (`direct_response.rs:77-104`) does `write_all` → `flush` →
  `shutdown()` (FIN) → read-and-discard to EOF → drop. Reviewer 1 re-ran the suite (5/5 green)
  and clippy (clean).
- **The terminal pre-pass is correct *and complete*, including the path nobody had checked.**
  Reviewer 2 traced `bootstrap.rs:3014-3029` up through `for chain in &mut listener.filter_chains`
  (`:2971`) to the listener loop at `:2967-2969`, which chains **static AND dynamic** listeners
  (`static_listeners.iter_mut().chain(dynamic_listeners.iter_mut().flatten())`), then confirmed
  `load_dynamic_resources` (`lib.rs:844`) re-invokes `bootstrap::validate` at `lib.rs:1053`.
  **LDS/xDS-loaded listeners get the terminal check too — a chain cannot escape it.** This is
  the single most important correctness property of the phase and it holds.
- **Error precedence matches upstream.** The pre-pass returns before the per-filter loop runs,
  so `[direct_response, echo]` reports `NetworkFilterNotTerminal` even when the trailing filter
  is itself malformed — reproducing the SPEC §0 R-0.6 live-Envoy evidence.
- **The fixture is a sound, non-vacuous witness.** Reviewer 3 confirmed `assert_body_rule`'s
  `ByteExact` is a bare `if envoy_body != rust_body { bail! }` (`lib.rs:6461-6467`), so an
  empty-vs-empty comparison *would* pass vacuously — but fixture `0071` configures a **non-empty
  39-byte payload** that the authoritative upstream reference deterministically emits, so any
  regression that drops, truncates, or mutates the payload is caught.
- **The `main.rs` `anyhow::bail!("validator guarantees…")` invariant is actually guaranteed.**
  `run()` calls `parse_bootstrap`, which validates; the Task-2 arm rejects any
  `direct_response` whose `typed_config` is not `TypedConfig::DirectResponse`. And the
  `filters.first()`-only read is safe **because** terminal validation makes any ≥2-filter chain
  invalid — the two halves of this phase interlock.
- **The out-of-PLAN test change did not weaken the test.** Reviewer 4 verified hunk-by-hunk
  against ADR-0125/0126: the 30s budget is unchanged, the timeout is still fatal
  (`.expect(...)` → `if …is_err() { diagnostics; panic! }` is fatality-preserving), the stderr
  read is genuinely bounded (an unbounded read would *hang* rather than fail), the child is
  reaped via `kill_on_drop(true)`, and the pre-build is bounded and loud on all four match arms.
- **All four `BEHAVIOR_CONTRACT.md` clauses match the code.** Reviewer 4 cross-checked clause 2
  (drain) against `direct_response.rs:82-102`, clause 3 (terminal rule) against
  `bootstrap.rs:825` + `:3021-3029`, and clause 4 (`inline_string`-only) against
  `DataSourceInline`'s `deny_unknown_fields` (`bootstrap.rs:794-797`). **No clause overstates
  the implementation.** The do-not-conflate banner is accurate and no pre-existing
  `direct_response` row (all of which describe the phase-04 route-level action) was altered.
- **`ROADMAP.md` row 66 is well-formed:** its two internal pipes (`.and_then(\|c\| c.filters.first())`)
  are escaped `\|`, its 6 columns are intact, and its status is still `in-progress` (correctly
  deferred to state-6).
- **The fuzz seed is tracked**, not silently `*`-ignored: `git ls-files` returns it,
  `git check-ignore -v` exits 1, and the `!` un-ignore line sits *after* the
  `corpus/parse_bootstrap/*` ignore, which is the required order.

---

## Amendment to existing carry-forward M66-2 (sharpened, not superseded)

ADR-0126 opened **M66-2** naming four latent siblings of the compile-inside-a-readiness-budget
hazard. This session **independently verified that claim and found it accurate — and
understated.** All four not only nest a `cargo run --manifest-path`, they *also still pass
`--quiet`* under the *same* 30s readiness budget as the bug that was just fixed:

| File | `--quiet` | nested `cargo run --manifest-path` | budget |
|---|---|---|---|
| `crates/envoy-bin/tests/upstream_connection_pooling.rs` | yes (`:137-140`) | yes | 30s (`:242`) |
| `crates/envoy-bin/tests/upstream_active_health_check.rs` | yes (`:147-150`) | yes | 30s (`:182`/`:226`) |
| `crates/envoy-bin/tests/upstream_outlier_detection.rs` | yes (`:179-182`) | yes | 30s (`:291`) |
| `tests/differential/src/backend.rs` | yes (`:363-367`) | yes | 30s (`:399`) |

They survive only because their helper chains (`http1-echo-server`,
`health-aware-http1-backend`) are warmed by earlier tests in the run — exactly as ADR-0126
predicts. **`--quiet`, not the stdio posture, is the silencing factor in every case**: even
`backend.rs`, which deliberately uses `stderr(inherit)` rather than `piped` (documented at
`:379-388`), would still suppress a `Blocking waiting for file lock` stall because of `--quiet`.

**Consequence for whoever picks up M66-2:** the fix is a single mechanical sweep applying the
`spawn_backend` shape now landed in `upstream_h2_connection_pooling.rs` — hoist each build out
of the readiness budget, and drop `--quiet` — to all four sites. **This is a sharpening of
M66-2's description, not a new Minor and not an ADR amendment** (ADR-0126 is landed and
append-only; nothing in it is contradicted).

---

## New carry-forwards

> Numbering note: **M66-1 was never allocated.** ADR-0126 opened `M66-2` directly. Per the
> standing convention that the ledger advances monotonically and does not backfill lapsed
> numbers, the new Minors below start at **M66-3**. Do not "fix" the gap.

- **M66-3 (Minor, resource-lifecycle, PRE-EXISTING — shared verbatim with `echo.rs`; filed by
  reviewer 1 as Important, downgraded here with reasons).** `serve()` never calls
  `set.join_next()` during normal operation — only after the accept loop breaks on shutdown
  (`direct_response.rs:64-67`; identically `echo.rs:49-52`). Backed by tokio's `IdleNotifiedSet`,
  a completed task's entry is retained (and counted by `len()`) until `join_next` pops it, so
  **one un-reaped entry accumulates per connection served**, unbounded, for the process
  lifetime. Compounding it: the drain loop's `reader.read().await`
  (`direct_response.rs:96-102`) has **no per-connection bound** — a client that reads the
  payload and then holds the connection open without sending FIN pins its task indefinitely
  (a slowloris-style hold); `DRAIN_TIMEOUT` + `set.shutdown()` tear these down **only once a
  shutdown signal arrives**. `echo_once` has the same unbounded-read shape. **Zero differential
  observable; not a regression** (verified by reading both files: the `serve()` bodies are
  structurally identical). **Fix:** add a reaping branch to the `select!` — e.g.
  `Some(_) = set.join_next(), if !set.is_empty() => {}` — and consider a per-connection idle
  bound. **Disposition:** repair `echo.rs` and `direct_response.rs` **together**, in the phase
  that next touches the network-filter data plane (plausibly the first non-terminal network
  filter, alongside CF-66-2). Fixing only one would break the "echo is the structural model"
  invariant. *`direct_response` is a far more plausible real traffic-serving endpoint than the
  test-only `echo`, so the latent cost is more likely to be paid here — this should not age
  indefinitely.*

- **M66-4 (Minor, doc-precision, INTRODUCED by phase 66).** `direct_response.rs:93-94` says the
  drain is *"Bounded by the caller's shutdown drain (DRAIN_TIMEOUT), exactly as `echo.rs` is."*
  Literally true, but it **reads as a per-connection bound and is not one** — the bound applies
  only once shutdown fires (see M66-3). Zero behavioral impact. **Fix:** one-line re-wording,
  e.g. "Bounded **on shutdown** by `DRAIN_TIMEOUT`…". **Disposition:** fold in with M66-3, or
  at any convenient session touching this file.

- **M66-5 (Minor, two unwitnessed network-chain config edges, PRE-EXISTING / adjacent).**
  (a) An **empty `filters: []` network chain is silently accepted** — verified this session:
  the only `filters.is_empty()` guard (`bootstrap.rs:3618`) is `validate_http_filters`, i.e.
  HTTP filters *inside* HCM, reached from `validate_hcm`; no network-chain cardinality guard
  exists. Phase 66 closed the *too-many-filters* direction; the *zero-filters* direction is
  untouched. (b) **`response: {}`** (an empty `DataSource` map) is rejected with
  `missing field inline_string`, because `DataSourceInline.inline_string` is required.
  **Both edges' upstream-Envoy behavior is UNWITNESSED** — SPEC §0 R-0.7 measured only
  `inline_string: ""` and full omission of `response`, and R-0.6 never probed an empty chain.
  Per D-3.3 this review therefore records the envoy-rust behavior and **declines to assert what
  Envoy does**; a reviewer's intuition that Envoy rejects an empty chain is *not* evidence.
  **Fix:** a live `--mode validate` probe of the pinned image against both shapes, then either
  match Envoy or record the divergence in `BEHAVIOR_CONTRACT.md`. **Disposition:** fold into
  the next phase touching network-filter-chain validation (natural home: CF-66-2, the generic
  chain-iteration protocol, which must decide what an empty chain means anyway).

- **M66-6 (Minor, test coverage, INTRODUCED by phase 66).** Four gaps, none of which makes a
  landed claim false — every one is safe *by construction* from the loop shapes — but each of
  which currently rests on **manual tracing rather than a test**:
  (a) **No dynamic/LDS-listener terminal test.** The completeness of the terminal rule across
  the `load_dynamic_resources` re-validation path is the phase's most load-bearing property and
  had to be established by hand-tracing this session. One test driving `load_dynamic_resources`
  with a 2-filter LDS chain would pin it.
  (b) No 3-filter chain (terminal at position 2 of 3), no multi-`filter_chains` listener, no
  multi-listener case.
  (c) No concurrent-connection test for `direct_response` — `echo.rs` has
  `handles_two_concurrent_connections` (`echo.rs:115`); the `Arc::clone`/spawn path is never
  exercised under concurrency.
  (d) `shutdown_signal_stops_the_accept_loop` (`direct_response.rs:182`) asserts only that the
  *listener* closed; unlike `echo.rs:87`'s sibling it never asserts `serve()` itself returns and
  drains in-flight work. Also: no large-payload test.
  **Disposition:** (a) is the valuable one and should be folded into the next
  `envoy-config`-validation phase; (b)-(d) are cheap polish.

- **M66-7 (Minor, cosmetics, INTRODUCED by phase 66).** (a)
  `tests/differential/tests/network_filter_direct_response.rs` lacks the `//!` module-doc header
  its siblings carry (`tcp_proxy.rs`, `tls_sni.rs` each open with a 4-6 line block naming the
  phase and noting the test is Docker-gated). (b) `drive_tcp_direct_response`'s timeout arm
  (`tests/differential/src/lib.rs:1713-1716`) discards the partially-read `out` and omits the
  byte count from its `bail!`; including `out.len()` would sharpen diagnostics on a real
  lingering-peer failure. Neither is misleading today. **Disposition:** ride a future cosmetic
  pass.

---

## Carry-forward disposition

- **Phase 66 CONSUMES none.**
- **M66-3, M66-4, M66-5, M66-6, M66-7 → NEW** (above).
- **M66-2 → stays OPEN, description SHARPENED** (above): all four latent siblings also pass
  `--quiet` under a 30s budget, so a single mechanical sweep neutralizes the class.
- **CF-66-1** (`inline_bytes`/`filename` `DataSource` arms unsupported, loudly rejected) and
  **CF-66-2** (no generic network-filter chain iteration protocol) **stay OPEN**, as opened by
  ADR-0123 §2.2. M66-5 is a natural companion to CF-66-2.
- **M64-2, M64-3, M57-1, M55-1, M53-2, M53-3, M48-2, M42-1,** the `DC`/retry-budget-overflow
  slices of **M45-2**, the phase-58 candidate carry-forward, **M40-1, M39-1/M39-2,
  M38-1/M38-2, CF-39-1**, M37-\*, M36-\*, M34-\*, M33-\*, the empty-`metadata_match`
  doc-comment, M29-\*/M30-\*, the phase-31 cosmetics, **M65-1**, and the HTTP-filters-family
  (1)-(4) all stay LIVE. **NONE blocks.**

---

## Process facts

- **NO new ADR fired this session.** No §6.2 reconciliation; no SPEC §0 recon finding
  (R-0.1 … R-0.11) overturned; no ambiguity arose that the landed ADRs do not already settle.
  The M66-2 sharpening is a carry-forward description update, **not** an amendment to the
  append-only ADR-0126 (nothing in it is contradicted — it is confirmed and made more precise).
- **DECISIONS.md ledger head: ADR-0126.** Next-available **ADR-0127**, unreserved.
- **ADR-0123 is REFINED by ADR-0124, not superseded.** ADR-0014 in force; ADR-0028 open;
  ADR-0049 governs config-validity. ADR-0006/ADR-0007 remain the reason a NEW driver — rather
  than a widened `TcpEcho` — is correct.
- **The §7.5 gate was NOT re-run** (it passed at state-4; `PROGRESS.md` carries the verbatim
  evidence). **No production code changed this session.**
- **Nothing was weakened.** No fixture touched; `tests/conformance/h2spec/known-failures.txt`
  untouched; the ADR-0124 drain and its mutation check
  (`post_eof_client_write_is_accepted_not_reset`) not re-litigated and not deleted; the `echo`
  `typed_config` asymmetry not "fixed"; BLOCK-66-1 not re-opened.
- `#![forbid(unsafe_code)]` holds (D-3.8); no `unsafe` anywhere in the diff.
- No new crate, no new dependency, no new fuzz target, no new timeout knob in the data plane,
  no `ci.yml` change.
- The **§6.1 split gate does not apply** at state-5.

---

**Next session = §5 state-6 close-out** (flip ROADMAP row `66` → `done`; relocate the
phase-66 `## Notes` subsection to `STATE_HISTORY.md` using the **delta-based** byte-preservation
check, allowlisting the `### Doctrine reminders` bullet and the `### Phase-66` heading;
`STATE.md` → "awaiting next planning"). Per §5.1, one state per session — the close-out was
deliberately **NOT** run here, and the next-phase state-0/1 pick is a **separate** session again
after that.
