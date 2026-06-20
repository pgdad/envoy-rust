# Phase 29 (`29-lb-maglev`) — REVIEW

> **Lifecycle state 5** (`BOOTSTRAP_PROMPT.md` §5 — verified, not reviewed →
> `superpowers:requesting-code-review` → REVIEW.md). This review covers the phase-29
> state-3 implementation arc (PLAN Tasks 1–8 = the MAGLEV LB deliverables) verified at
> the Task-8 state-4 gate. **Verdict: APPROVED.**
>
> **Review model:** each of PLAN Tasks 1–8 was ALREADY individually two-stage-reviewed
> (spec-compliance THEN code-quality) by a fresh `superpowers:code-reviewer` subagent during
> execution. This state-5 review is therefore the **holistic phase review** — a single fresh
> `superpowers:code-reviewer` subagent given crafted context (the SPEC + the ADR-0072 §6.2-locked
> Maglev algorithm + the seven cross-cutting focus areas, NOT this session's history), tasked with
> the system-level seams per-task reviews cannot fully see, plus a re-triage of the one open
> minor (M29-1).
>
> **Review range:** `40f4e39^` (the state-2 PLAN-write base) … `d4e31f5` (Task-8 code-HEAD)
> — the full phase-29 production + test diff (+1433 / −82, 16 files; the interleaved
> `PROGRESS.md` commits in-range are docs, out of code scope). The state-4 STATE-advance commit
> `154bb9a` is docs-only and out of code scope.
> **Differential evidence:** the AUTHORITATIVE native-Linux CI run **`27851283501`** @ `1f2ad7b`
> (both jobs GREEN: fixture `0037-lb-maglev` cross-proxy STRONG MAGLEV witness `test
> lb_maglev_fixture ... ok` + all 36 pre-existing Docker-gated fixtures `0001`–`0036` [0 failed
> workspace-wide], h2spec ≥95% [unchanged — no H2 codec change], `parse_bootstrap` [new MAGLEV +
> `maglev_lb_config` seed] + `jwt_parse` fuzz clean; `cargo fmt --all -- --check` / `cargo clippy
> --workspace --all-targets --all-features -- -D warnings` / `cargo build --workspace
> --all-targets` / `cargo test --workspace` / `cargo deny check` all clean). Per phase-29's
> local-observability advantage (maglev selection is a normal request/response, no
> file-watch/reload trigger) the fixture-0037 differential also ran GREEN locally during state-3.

## Verdict: **APPROVED**

**APPROVED — 0 Critical / 0 Important / 2 Minor (non-blocking).** The independent reviewer
confirmed the §6.2-locked Maglev algorithm (`maglev.rs`) is byte-exact to the ADR-0072 contract:
host key = the bare `ip:port` string (NO `_i` suffix); `offset = xxh64_seed(key,0) % M`; `skip =
xxh64_seed(key,1) % (M-1) + 1`; `permutation[j] = (offset + j·skip) % M`; round-robin claim loop
in config order (earlier host wins); lookup `table[xxh64_seed(value,0) % M]`. The pinned oracle
encodes PLAN §A's full key→host table + the 32769/32768 distribution + the empty-but-present
`""→host 0` case and passes. The termination argument holds (prime M + skip coprime-to-M ⇒ each
per-host permutation is a full cyclic permutation ⇒ the claim loop always fills) and the
`next[host] * skip[host]` product is overflow-safe in `u64` (`≤ M² ≈ 2.5e13 ≪ u64::MAX`). The
seeded-xxHash64 generalization keeps seed-0 output byte-identical (no phase-28/RING_HASH
regression). The M28-3 `hash_lb: Option<HashLb>` refactor preserves RING_HASH + ROUND_ROBIN
behavior byte-for-byte and converts the phase-28 footgun into a compile error via an exhaustive
wildcard-free `match cfg.lb_policy`. No finding was rated above Minor. This is the **fourteenth
consecutive clean state-5** (after 17, 18, 19, 20, 21, 22, 23, 24, 25.1, 25.2, 26, 27, 28).

Per `BOOTSTRAP_PROMPT.md` §5.2 the re-enter-state-3 trigger is a Critical or Important finding;
there are none. The phase lands APPROVED with the M29-1 follow-up carried forward (the established
pattern — REVIEW Minors weighed at the next phase's planning). The state-6 deterministic close-out
(commit `phase 29: MAGLEV consistent-hashing load balancer [ADR-0071, ADR-0072]`; flip ROADMAP row
`29` `in-progress → done`; STATE → AWAITING NEXT PLANNING; ADR-0035 narrative relocation; push) is
the NEXT session. This approved `REVIEW.md` satisfies §7.5 gate (f); (a)–(e) are GREEN at CI
`27851283501`.

## Scope reviewed (PLAN Tasks 1–8)

The Task-1 seeded xxHash64 (`crates/envoy-cluster/src/xxhash.rs` — `xxh64 → xxh64_seed(data,
seed)`, seed-0 byte-identical wrapper); the Task-2 `LbPolicy::Maglev` + `MaglevLbConfig {
table_size }` + the `maglev_lb_config` cluster field (`crates/envoy-config/src/bootstrap.rs`); the
Task-3 MAGLEV-gated `table_size` validation (`MaglevTableSizeNotPrime` / `MaglevTableSizeTooLarge`
in `lib.rs`; over-max-before-primality ordering + the `is_prime` helper in `bootstrap.rs`); the
Task-4 `crates/envoy-cluster/src/maglev.rs` (`MaglevTable::build` + `lookup` — the §A algorithm +
the pinned live-Envoy oracle); the Task-5 M28-3 discriminator refactor (`ring: Option<HashRing>` →
`hash_lb: Option<HashLb>` with `HashLb { Ring, Maglev }`, the `pick()` dispatch, the exhaustive
`from_bootstrap` build — `crates/envoy-cluster/src/cluster.rs`); the Task-6 fixture
`0037-lb-maglev` (cross-proxy STRONG differential) + `tests/differential/tests/lb_maglev.rs` +
the reused `{{BACKEND_IP}}`/`discover_host_lan_ip` shared-IP mechanism; the Task-7 in-process
backstop (determinism / spread / fallback / single-host / the M28-3 three-policy dispatch witness,
in `cluster.rs` + `maglev.rs`); the Task-8 `parse_bootstrap` fuzz seed + the BEHAVIOR_CONTRACT
"LB selection" MAGLEV row (M28-1 folded).

## Cross-cutting focus areas — all PASS

1. **§6.2 Maglev algorithm byte-exactness — PASS.** `maglev.rs` implements `offset =
   xxh64_seed(key,0)%M`, `skip = xxh64_seed(key,1)%(M-1)+1`, `permutation[j]=(offset+j·skip)%M`,
   and the round-robin claim loop in config order (earlier host wins via the `while table[c] !=
   usize::MAX` cursor-advance). Host key is the bare `ip:port` (no `_i` suffix). The pinned oracle
   encodes PLAN §A verbatim and passes (incl. distribution 32769/32768 and `""→host 0`).
   Termination holds (prime M + coprime skip ⇒ full cyclic per-host permutation ⇒ loop always
   fills); `next·skip ≤ M² ≪ u64::MAX` is overflow-safe.
2. **Seeded xxHash64 seed-0 byte-identity — PASS.** `xxh64(d) = xxh64_seed(d, 0)`; the `seed`
   parameter flows into all four lane initializers AND the `<32`-byte branch. `seed0_equiv` (incl.
   a ≥32-byte input exercising the lane path) + the 5 locked phase-28 vectors confirm no
   regression; `seed1_host_key` pins the new seed-1 `skip` hash (independently generated via
   xxhash 3.7.0).
3. **M28-3 `hash_lb` dispatch refactor — PASS.** Both `HashLb` arms return `Option<usize>`; the
   `hi < total` bounds guard is preserved verbatim. `from_bootstrap` is an **exhaustive `match
   cfg.lb_policy`** (RingHash / Maglev / RoundRobin) with **no wildcard arm** — a future LB variant
   is a compile error, structurally retiring the phase-28 footgun. RING_HASH selection +
   ROUND_ROBIN no-op proven byte-identical by the 139 passing cluster tests incl. the
   three-policy dispatch witness.
4. **MAGLEV-gated validation — PASS.** Gated on `lb_policy == Maglev && maglev_lb_config.is_some()`;
   over-max (>5_000_011) is checked BEFORE `is_prime`, so the bounded trial loop never runs on a
   huge value. `is_prime` is correct for 0 / 1 / 2 / even / 9 / 100 / 65537 / 5000011.
   `accepts_maglev_lb_config_on_non_maglev_cluster` proves the accept-and-ignore parity even for an
   otherwise-fatal `table_size: 100` on a ROUND_ROBIN cluster (Envoy parity + the phase-28 ring
   precedent).
5. **Fixture 0037 STRONG differential — PASS.** A faithful clone of 0036 (the diff is the policy
   flip + naming only). `{{BACKEND_IP}}` appears 3× on BOTH `envoy.yaml` and `envoy-rust.yaml`
   (the shared-IP discipline preserved — the Maglev table is `ip:port`-string-sensitive, so a
   per-side IP split would break the cross-proxy STRONG identity, per memory
   `consistent-hash-lb-differential-needs-identical-endpoint-strings`). STRONG cross-proxy +
   STABILITY + SPREAD assertions are wired through the shared `Http1HashSweep` driver.
6. **`#![forbid(unsafe_code)]` / YAGNI — PASS.** Both touched crate roots retain the forbid; no
   `unsafe` introduced (the only "unsafe" matches in the diff are doc-comment prose). No weighted
   Maglev, no MAGLEV+HC/OD differential — nothing half-built smuggled in (SPEC §2.2 deferrals
   honored).
7. **M29-1 (RING_HASH-worded `bail!` messages) — PASS as classified (Minor).** The reviewer
   independently concurs it is failure-output-only, blocking nothing — see finding M29-1 below.

## Findings — Minor (2; non-blocking → phase-30 carry-forward)

- **M29-1 — the shared `Http1HashSweep` driver's `bail!` failure messages hard-code RING_HASH
  vocabulary** (`tests/differential/src/lib.rs` ~:4344–4392). The driver now serves both fixture
  0036 (RING_HASH) and 0037 (MAGLEV), but all five `bail!` strings read `"RING_HASH cross-proxy
  selection mismatch…"` / `"…the locked xxHash64 ring — ADR-0070…"` / `"RING_HASH instability…"` /
  `"RING_HASH spread failure…"`. A MAGLEV (fixture 0037) mismatch would therefore print
  RING_HASH-worded diagnostics — misleading an operator debugging 0037. **Independent judgment:
  correctly classified as Minor** — it is failure-output-only (it cannot affect a passing test or
  any production path; the `up1`/`su1` markers + the offending key still identify the real
  problem) and there is no correctness, coverage, or contract gap, so it does NOT rise to
  Important. **Fix (defer):** thread a `policy_label: &str` (or the driver/fixture name) into the
  driver and interpolate it into the messages, or genericize to "consistent-hash LB" + cite the
  active ADR. A standalone cleanup that benefits BOTH fixtures — cheapest when the differential
  driver is next touched.
- **M29-2 — the same RING_HASH wording appears in the driver's COMMENTS** (`tests/differential/src/lib.rs`
  ~:4341–4377: "ring distribution", "RING_HASH selection for this key"). Same root cause as M29-1;
  cosmetic; fold into the M29-1 cleanup.

## Triage of the per-task minors already logged during state-3 (all CONFIRMED non-blocking)

- The per-task Minors recorded in `PROGRESS.md` (Task-2 strengthen-the-gating-test [CONSUMED by
  Task 3]; the Task-4/5/6/7/8 optional-no-action style/doc Minors) were each dispositioned at
  their own task review. The holistic reviewer surfaced nothing above Minor and independently
  re-confirmed the one open carry-forward (M29-1 = M-1/M-2 in the subagent report). The Task-2
  carry-forward (feed a non-prime `table_size` on a non-MAGLEV cluster) was CONSUMED by Task 3, and
  the M28-1 / M28-2 / M28-3 phase-28 carry-forwards were all CONSUMED or folded this phase (M28-3 →
  the Task-5 refactor; M28-1 → the Task-8 BEHAVIOR_CONTRACT fold; M28-2 → reconfirmed by §6.2,
  backstop-covered).

## Recommendations

- Land **M29-1 + M29-2** (policy-agnostic driver diagnostics) opportunistically — cheapest when the
  `tests/differential/src/lib.rs` `Http1HashSweep` driver is next touched; benefits both the 0036
  and 0037 fixtures.
- The exhaustive wildcard-free `match cfg.lb_policy` in `from_bootstrap` is now the canonical LB
  dispatch — a future LB policy (subset / locality / priority / least_request / random) will be a
  compile error until it adds its arm. Preserve this property (do NOT add a `_ =>` catch-all).

## §7.5 phase-done gate (final)

(a) fixture `0037` GREEN + (b) all of `0001`–`0036` GREEN + (c) h2spec ≥95% (unchanged) + (d) the
`parse_bootstrap` MAGLEV fuzz seed clean + (e) `cargo build`/`clippy --all-targets
--all-features`/`fmt --check`/`test --workspace`/`deny check` clean — ALL GREEN at the
AUTHORITATIVE Linux CI `27851283501` @ `1f2ad7b`. (f) `REVIEW.md` approved — THIS document.
`#![forbid(unsafe_code)]` holds (D-3.8). **§7.5 gate (a)–(f) COMPLETE.**

---

_Verdict APPROVED (0C / 0I / 2 Minor → M29-1 / M29-2 phase-30 carry-forwards). Range
`40f4e39^`..`d4e31f5`, CI `27851283501`. Ledger head ADR-0072 (count 73; ADR-0073 reserved +
UNFIRED). The state-6 deterministic close-out is the NEXT session._
