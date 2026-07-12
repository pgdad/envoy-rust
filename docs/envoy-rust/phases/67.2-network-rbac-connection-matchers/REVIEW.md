# Phase 67.2 — §5 state-5 RE-REVIEW (SUPERSEDES the NOT-APPROVED `ab216b4` review, per D-3.5)

> Written by the §5 **state-5 RE-REVIEW** session (`superpowers:requesting-code-review`), per
> `BOOTSTRAP_PROMPT.md` §5 state 5 and `SKILL_ROUTING.md`. The output of this session IS this file.
> **This review SUPERSEDES the first `67.2` review** (verdict NOT APPROVED on Critical C-1), whose
> full text is preserved in git at commit `ab216b4` — a review is never edited, only superseded by a
> LATER review (D-3.5). Cold-started clean: `git status --porcelain` empty; branch `main`;
> `HEAD` = `origin/main` = `cb73bd8` (the POST-C-1-repair state-4 verification commit);
> `git fetch origin --prune` showed no sibling ahead; CI run `29173588258` GREEN on the full 40-char
> SHA `cb73bd80ff524a96b49cac0505a025caec6c6db4`.
>
> **Review surface:** the whole `67.2` arc (`08f820d..cb73bd8`) — the six task commits
> (`f31b21c`..`bbcbe7c` + the `494043f` fmt cleanup) PLUS the C-1/I-1/N-1 repair
> (`e0a15dc` + `8cab4af`, landed out-of-band by a sibling workstream and reconciled as the §5.2
> state-3 re-entry). The five connection-level matcher arms, the `CidrRange` type, the V-1
> shared-enum fallout, the docs/fuzz seed, and the repair.
>
> **Method (memory `state5-must-probe-untested-compositions`).** This re-review did not merely
> re-read the repair diff. It (1) **re-drove the original C-1 live repro** end-to-end against a
> freshly-built `target/debug/envoy-bin`; (2) **live-probed the untested compositions** (boundary
> widths, nested combinators, the validate-passing mapped-prefix equivalence, the LDS path);
> (3) **measured upstream** on the acceptance question the repair left open (`--mode validate`
> against the pinned `envoyproxy/envoy:v1.33.0`); and (4) dispatched an **independent adversarial
> `general-purpose` subagent** tasked to reproduce-or-refute every primary finding and to hunt for
> new Criticals with exhaustive old-vs-new differential probes. Every measurement is quoted inline.

---

## VERDICT

> ### **APPROVED. §7.5 gate (f) is satisfied.**
>
> The C-1 repair is **measured-correct**: the config-reachable, release-mode data-plane panic the
> `ab216b4` review found is CLOSED at its root. `CidrRange::validate` now sizes `prefix_len`
> against the **canonical** address family via the shared `canonical_ip` helper — the single
> canonicalisation rule used by both `validate` and `contains` — so an IPv4-mapped-IPv6
> `address_prefix` is bounded at 32 and every over-wide width is rejected fail-loud with
> `ConfigError::InvalidCidrRange` at config load. Reproduced end-to-end (§2), confirmed through
> nested combinators and the LDS path (§2, §4), and independently verified by an adversarial
> subagent whose exhaustive old-vs-new sweep found **no new Critical and no behavior change other
> than panic→load-time-rejection** (§4). The I-1 coverage blind spot is closed by a regression test
> + a `contains`-level property sweep, both proven to fail against the pre-fix code (§4 V-4).
>
> **No new Critical. No new Important.** Two Minors are opened (§5), neither blocking: **M-1** (the
> N-1 defensive guard has a measured off-by-one band that is NOT config-reachable, plus an
> overclaiming comment) and **M-2** (the repair is a measured config-acceptance divergence vs
> upstream that was unrecorded — **closed THIS session** by ADR-0134 + the `BEHAVIOR_CONTRACT.md`
> item-14 bullet landed with this review, per D-3.5 / invariant 4.1.5's "never silently").
>
> With (f) met, all six §7.5 gates are satisfied. The SEPARATE next session runs the §5 state-6
> close-out — which flips ONLY ROADMAP row `67.2` to `done` (parent `67` waits for `67.3`; the
> `67.2/SPEC.md` header + D8 are stale on this point). Per §5.1 / ADR-0127 this session does NOT
> chain into the close-out.

---

## §1. What the repair is (code-read, verified at `HEAD`)

- **`canonical_ip`** (`crates/envoy-config/src/bootstrap.rs:1649`): collapses an IPv4-mapped-IPv6
  address to its 4-byte IPv4 form; everything else unchanged. Documented as THE single
  canonicalisation rule shared by `validate` and `contains`, with the C-1 mechanism spelled out.
- **`CidrRange::validate`** (`:1673`): takes the family cap from `canonical_ip(self.address_prefix)`
  — a mapped prefix is now IPv4 (≤ 32). This is exactly the root-cause fix the `ab216b4` review §1
  prescribed ("make `validate` size the prefix against the canonical family, matching `contains`").
- **`prefix_match`** (`:1706`): gains the N-1 defensive bounds bail
  (`if full > net.len() || full > addr.len() { return false }`) — a silent bail, not a
  `debug_assert!`. See M-1 for its measured limitation.
- **Tests** (`e0a15dc` + `8cab4af`): `cidr_range_validate_rejects_ipv4_mapped_ipv6_over_wide_prefix`
  (widths 33/39/40/64/128 rejected + the /8 equivalence preserved),
  `cidr_range_contains_never_panics_for_validated_prefixes` (property sweep: every
  validate-passing prefix × a v4/v6/mapped address matrix, no panic), and
  `network_rbac_rejects_ipv4_mapped_ipv6_over_wide_cidr_at_config_load` (through the real
  `validate(&mut bootstrap)` path). The repair touched ONLY `crates/envoy-config/src/bootstrap.rs`.

## §2. First-hand measurement performed this session

### (a) The original C-1 live repro is CLOSED (end-to-end, the exact `ab216b4` config)

`cargo build -p envoy-bin` first (memory `differential-harness-uses-debug-envoy-bin`), then
`target/debug/envoy-bin -c <cfg>` with the exact previously-accepted-then-panicking config
(`[rbac, echo]`, `destination_ip: { address_prefix: "::ffff:127.0.0.0", prefix_len: 40 }`):

```
EXIT=1
ERROR envoy-rust exited with error error=listener "rbac_listener": network rbac policy "p0"
      has an invalid CidrRange at permissions[0]: prefix_len 40 exceeds 32 for IPv4
```

Exit 1 fail-loud at config load, `ConfigError` on STDOUT, **no panic, no acceptance, no listener
bound**. The boundary width `/33` is likewise rejected (`prefix_len 33 exceeds 32 for IPv4`).

### (b) Untested compositions LIVE-PROBED (the `state5-must-probe-untested-compositions` duty)

- **Nested combinators:** a mapped `/40` under `and_ids → not_id` is rejected at load with the
  exact nested path — `invalid CidrRange at principals[0].ids[0].not_id: prefix_len 40 exceeds 32
  for IPv4`. (The adversarial subagent additionally probed `not_id` directly, `and_ids` positional,
  `or_ids → not_id → remote_ip`, `not_rule → destination_ip`, AND the **LDS dynamic-listener**
  path — all rejected fail-loud with correct paths; the `validate_l4_*` walkers recurse into every
  combinator and LDS listeners re-run the same validation gauntlet.)
- **The validate-passing mapped prefix works end-to-end:** `direct_remote_ip:
  { address_prefix: "::ffff:127.0.0.0", prefix_len: 8 }` boots, a loopback client is ALLOWed, and
  the echo terminal round-trips the payload (`recv: b'ping'`) — the mapped spelling is equivalent
  to plain `127.0.0.0/8`, as item 14 claims, live.
- **Repair tests green from the run:** `cargo test -p envoy-config cidr_range` → **9 passed / 0
  failed**, including both repair tests quoted `ok`.

### (c) Upstream measured on the question the repair left open

```
$ docker run --rm -v <cfg>:/cfg.yaml:ro envoyproxy/envoy:v1.33.0 --mode validate -c /cfg.yaml
configuration '/cfg.yaml' OK        # the exact C-1 config: mapped /40 destination_ip
```

**Upstream Envoy v1.33.0 ACCEPTS the config envoy-rust now rejects.** The C-1 repair is therefore
a **measured config-acceptance divergence**, not parity — falsifying the state-3 re-entry's "no
new ADR — the fix corrects an internal family classification, no measured wire shape changes"
rationale. Recorded NOW: **ADR-0134** + a `BEHAVIOR_CONTRACT.md` item-14 bullet, both landed with
this review (see M-2). Upstream's *runtime matching* semantics for a mapped prefix remain
UNMEASURED (the IP arms are host-dependent under the Docker harness — parent V-4 — so no fixture
can witness them); only config ACCEPTANCE was measured, and only acceptance is asserted.

### (d) The N-1 guard probed adversarially (found: M-1)

Standalone `rustc -O` probe of the HEAD code, verbatim:

```
validate() = Err("prefix_len 33 exceeds 32 for IPv4")     # config path rejects this
contains(127.0.0.1)  [octets differ at byte 3] -> Ok(false)
contains(127.0.0.0)  [first 4 octets EQUAL]    -> PANIC: index out of bounds: len 4, index 4
v6 /129 contains(2001:db8::) [16 octets equal] -> PANIC: index out of bounds: len 16, index 16
```

For an **unvalidated** `CidrRange`, the guard's `full > net.len()` check misses the band
`full == net.len() && rem > 0` (v4 `prefix_len` 33..=39, v6 129..=135): when the first `full`
octets compare equal, `net[full]` still indexes out of bounds. **Not config-reachable** — see M-1.

## §3. Independent adversarial subagent — CONFIRMED on all counts

An independent `general-purpose` reviewer, given the repair commits and tasked to
reproduce-or-refute, returned:

- **V-1 — config-reachable surface CLOSED.** Traced every path to the only two
  `CidrRange::contains` production call sites (`network_rbac.rs:123`/`:149`): static listeners,
  LDS dynamic listeners (same validation gauntlet re-run over the merged set; no LDS hot-reload
  exists), and the HTTP RBAC filter (all five L4 arms rejected at `lower_*`; `RuntimeMatcher` has
  no CIDR variant, so HTTP can never reach `contains`). Six live nested/LDS probes all rejected
  fail-loud with correct path strings; a nested `/8` control config boots and serves.
- **V-2a — the M-1 guard band CONFIRMED** with its own probe (all 14 band members panic on
  equal-octet inputs), and **CONFIRMED not config-reachable**: an exhaustive sweep (14 prefixes ×
  256 prefix_lens × 13 addresses) found **0 validate-passing configs that panic**. The guard
  comment's "unconditionally" claim adjudicated FALSE as written.
- **V-2b — the divergence-unrecorded finding CONFIRMED**, independently reproducing the upstream
  `configuration OK` and grepping `BEHAVIOR_CONTRACT.md` + `DECISIONS.md` (item 14 records only the
  peer-side canonicalisation; ADR-0133 predates the fix; no record of the mapped-prefix-width
  rejection existed).
- **V-3 — NO new Critical.** Old (`f2fb252`) vs new differential probe over the full matrix: **288
  behavior diffs, all of them** validate-verdict flips on the three mapped prefixes × widths
  33..=128, **every one** a previously-panic-capable config (`old_contains_panics_on_some_addr=true`
  for all 288; zero `contains` diffs on any both-validated config). No previously-correct config
  changed behavior — the only change is accept-then-panic → reject-at-load.
- **V-4 — the repair tests PIN the fix.** Replayed against the extracted pre-fix code: the property
  sweep panics at exactly `("::ffff:0.0.0.0", 33, 0.0.0.0)`, the regression test's first `is_err()`
  fails (old validate accepts all five sampled widths), and the `8cab4af` end-to-end test's
  `expect_err` fails. All 10 relevant tests pass at `HEAD`.

## §4. Strengths (carried and new)

1. The `ab216b4` review's §3 strengths all still hold: exhaustive catch-all-free classification at
   every match site (re-verified: the only `_ =>` string in `network_rbac.rs` is inside the doc
   comment forbidding it); the deliberate HTTP-rejects-L4 divergence complete and pinned; the
   faithful `destination_port: u16`; the shared source-IP evaluation; the `extra_leaves` macro.
2. **The repair is the prescribed minimal fix, executed exactly**: one shared helper, the family
   decision moved to the canonical side, a defensive bail, and tests that demonstrably fail
   pre-fix. No scope creep — the diff touches only `bootstrap.rs`.
3. **The `canonical_ip` doc comment encodes the C-1 mechanism** (why the two functions MUST share
   one rule), so the invariant survives the next editor.
4. **The property sweep mirrors the data-plane gate** (`if cidr.validate().is_err() { continue }`)
   — it tests exactly the reachable surface, not a fantasy one.

## §5. Issues

### CRITICAL — none.

### IMPORTANT — none.

### MINOR — neither blocks

- **M-1 (defensive-guard band + overclaiming comment; carry-forward).** The N-1 bail in
  `prefix_match` (`bootstrap.rs:1717`) misses `full == net.len() && rem > 0`: an **unvalidated**
  `CidrRange` with v4-canonical `prefix_len` 33..=39 (or v6 129..=135) still panics on `net[full]`
  when the first `full` octets are equal — measured (§2d), independently confirmed with an
  exhaustive sweep proving it is **NOT config-reachable** (every data-plane `CidrRange` passes
  `validate` first; both `contains` call sites evaluate config-validated ranges only). The code
  comment at `:1709-1716` ("keeps the data-plane invariant … true **unconditionally**, even for a
  future caller that constructs a `CidrRange` without validating") is FALSE as written, and the
  regression test's "must not panic, regardless of validation" line holds only for its sampled
  addresses. **Fix (one line + comment)**: bail on whole-byte-rounded width — e.g.
  `let needed = bits.div_ceil(8); if needed > net.len() || needed > addr.len() { return false; }`
  — plus a no-panic test for the band, and correct the comment. Severity: the original N-1 was
  itself advisory/optional; its incomplete implementation cannot outrank the absence it improved
  on. Carried forward to the next phase that touches `bootstrap.rs`'s CidrRange surface (`67.3` or
  later); do NOT fix in the close-out session.
- **M-2 (unrecorded acceptance divergence — CLOSED THIS SESSION).** The C-1 repair rejects a config
  upstream Envoy v1.33.0 accepts (measured, §2c) — a divergence that every sibling in item 14
  records explicitly, but which had no record (the re-entry's "no new ADR" rationale assumed no wire
  shape changed; acceptance IS a wire-visible surface). Per D-3.5 ("decisions are written, not
  remembered") and invariant 4.1.5 ("never both silently"), this review lands **ADR-0134** (the
  measurement, the options, the kept rejection posture per ADR-0049 decision-2 (b)) and the
  matching **`BEHAVIOR_CONTRACT.md` item-14 bullet** in this commit. Docs-only; no production code,
  no fixture, no fuzz target — the review's probe produced a new measurement, and recording
  measurements is exactly what the contract/ledger exist for.
- **N-2 (cosmetic, no action required).** No in-repo test pins the nested-combinator
  `InvalidCidrRange` paths (`principals[0].ids[0].not_id` etc.) — live-probed green here and
  structurally shared with the depth/empty-set walk the 67.1 tests cover. Note only.

## §6. Findings explicitly considered and REJECTED (carried verbatim from `ab216b4` §5 — do NOT re-litigate)

- **HTTP-accepts-L4-arms "parity fix"** — NO. Deliberate FAIL-LOUD DIVERGENCE, confirmed WAI
  (`BEHAVIOR_CONTRACT.md` item 14, ADR-0133, ADR-0049 decision-2 (b)). Do not edit
  `crates/envoy-filter/src/rbac.rs` toward parity.
- **The bare-`u8` `prefix_len` wrapper rejection** — NO. Deliberate (ADR-0063 precedent, ADR-0049).
- **A differential fixture for the IP/port arms** — NO. Structurally host-dependent (parent V-4);
  in-process + loopback coverage is the recorded posture.
- **A `_ =>` catch-all at the exhaustive RBAC match sites** — NEVER. The compile break is the
  forcing function.
- *(New this review)* **"Reject-at-load is wrong; match the mapped prefix as raw 16-byte IPv6 for
  upstream parity"** — NO. That would contradict the measured mapped-PEER canonicalisation
  (ADR-0133) and make mapped-prefix ranges silently unmatchable against the v4 peers they textually
  name; the pre-repair accept-then-panic was strictly worse. ADR-0134 records the options and keeps
  the fail-loud rejection.

## §7. Assessment

**Ready to merge?** **Yes.**

**Reasoning.** The single Critical that blocked the `ab216b4` review is fixed at its root, proven
closed end-to-end (original repro, boundary widths, nested combinators, LDS), pinned by tests that
fail pre-fix, and independently confirmed with an exhaustive old-vs-new sweep showing no other
behavior change and no new Critical. The two residual findings are a non-config-reachable
defensive-guard band with an overclaiming comment (M-1, carried forward with a precise one-line
fix) and a documentation gap (M-2, closed in this very commit by ADR-0134 + the contract bullet).
§7.5 (a)-(e) were re-verified GREEN at the post-repair state-4 (evidence in `PROGRESS.md`); with
this APPROVED review, **(f) is met and all six gates are satisfied**.

**Next session (SEPARATE, per §5.1 / ADR-0127): the §5 state-6 close-out.** Flip ROADMAP row
`67.2` → `done` (ONLY row `67.2` — parent `67` flips `done` only when `67.3` is also `done`; the
`67.2/SPEC.md` header + D8 are STALE on this), relocate the closed-phase STATE narrative per
ADR-0035, and advance `STATE.md` (sibling `67.3` has `SPEC.md` → its §5 state-2 PLAN-write is the
next phase work). `67.2` must NOT touch `67.3`'s scope (ADR-0132).

## §8. Carry-forward ledger

- **CONSUMED by the repair + this re-review:** `ab216b4`'s **C-1** (Critical — fixed `e0a15dc`,
  verified §2/§3), **I-1** (Important — regression + property tests, proven to pin the fix), and
  **N-1** (advisory — implemented as the `prefix_match` bail, with its M-1 residual).
- **OPENED by this review:** **M-1** (the guard band + comment fix + band test; carried to the next
  phase touching the CidrRange surface — `67.3` or later); **N-2** (cosmetic note, no obligation).
  **M-2 was opened and CLOSED within this session** (ADR-0134 + the item-14 bullet).
- **Unchanged:** `CF-67-1` (`shadow_rules`), `CF-67-2` (`Action::LOG`), `CF-67-3` (`on_data`
  iteration) stay live, none blocks; parent V-1 remains CONSUMED by `67.2`.
- **DEFERRED to `67.3` (unchanged):** the `ConnectionHandler` establishment/data-phase split, the
  `[rbac, tcp_proxy]` composition, the `UnsupportedNetworkFilterChainComposition` deletion
  (ADR-0132).
- **This review fired ADR-0134** (the mapped-prefix-width acceptance divergence). **`DECISIONS.md`
  ledger head: `ADR-0134`; next available: `ADR-0135`, unreserved.**
- **Numbering:** `M66-1` was never allocated; the ledger does not backfill.
