# Phase 28 (`28-lb-ring-hash`) — REVIEW

> **Lifecycle state 5** (`BOOTSTRAP_PROMPT.md` §5 — verified, not reviewed →
> `superpowers:requesting-code-review` → REVIEW.md). This review covers the phase-28
> state-3 implementation arc (PLAN Tasks 2–9 = the RING_HASH LB deliverables) + the
> Task-10 state-4 verification gate. **Verdict: APPROVED.**
>
> **Review model:** each of PLAN Tasks 2–9 was ALREADY individually two-stage-reviewed
> (spec-compliance THEN code-quality) by a fresh `superpowers:code-reviewer` subagent during
> execution. This state-5 review is therefore the **holistic phase review** — a single fresh
> `superpowers:code-reviewer` subagent given crafted context (the SPEC + the ADR-0070 locked
> algorithm + the seven cross-cutting focus areas, NOT this session's history), tasked with
> the system-level seams per-task reviews cannot fully see, plus re-triage of the per-task
> Minors logged in STATE.md.
>
> **Review range:** `50a03e4` (state-2 PLAN-write base) … `4b31144` (Task-10 code-HEAD)
> — the full phase-28 production + test diff (+3022 / −58, 20 files). The state-4 STATE-advance
> commit `9c84e74` is docs-only and out of code scope.
> **Differential evidence:** the AUTHORITATIVE native-Linux CI run **`27837455306`** @ `4b31144`
> (both jobs GREEN: fixture `0036-lb-ring-hash` cross-proxy RING_HASH witness + all 35
> pre-existing Docker-gated fixtures `0001`–`0035`, h2spec ≥95%, `parse_bootstrap` [new
> RING_HASH+hash_policy seed] + `jwt_parse` fuzz; local fmt / clippy `--all-targets
> --all-features` / builds / `deny check` / `cargo test` clean). Per phase-28's local-observability
> advantage (ring_hash selection is a normal request/response, no file-watch/reload trigger) the
> fixture-0036 differential also ran GREEN locally — the projected `{{BACKEND_IP}}` CI-portability
> risk did NOT materialize.

## Verdict: **APPROVED**

**APPROVED — 0 Critical / 0 Important / 3 Minor (non-blocking).** The independent reviewer
re-derived xxHash64 from scratch in a second language and confirmed all 11 vectors/oracle keys
reproduce byte-for-byte; the ADR-0070 ring algorithm (the load-bearing `_` separator, seed-0,
`min_ring_size/num_hosts` replicas, sorted ring, `bisect_left` wrap) is exact; the empty-vs-absent
header distinction and the `ring.is_some()` discriminator footgun are both correctly handled and
guarded; round-robin regression-equivalence holds (all 35 pre-existing fixtures green). No finding
was rated above Minor. This is the **thirteenth consecutive clean state-5** (after 17, 18, 19, 20,
21, 22, 23, 24, 25.1, 25.2, 26, 27).

Per `BOOTSTRAP_PROMPT.md` §5.2 the re-enter-state-3 trigger is a Critical or Important finding;
there are none. The phase lands APPROVED with M-track follow-ups (the established pattern — REVIEW
Minors weighed at the next phase's planning). The state-6 deterministic close-out (commit
`phase 28: RING_HASH consistent-hashing load balancer [ADR-0069, ADR-0070]`; flip ROADMAP row `28`
`in-progress → done`; STATE → AWAITING NEXT PLANNING; ADR-0035 narrative relocation; push) is the
NEXT session. This approved `REVIEW.md` satisfies §7.5 gate (f); (a)–(e) are GREEN at CI
`27837455306`.

## Scope reviewed (PLAN Tasks 2–9)

The Task-2 xxHash64 seed-0-from-scratch (`crates/envoy-cluster/src/xxhash.rs`, canonical vectors
pinned); the Task-3 `LbPolicy::RingHash` + `RingHashLbConfig` + `HashFunction` + validators
(`crates/envoy-config/src/bootstrap.rs`, `lib.rs` — XX_HASH-only narrowing; `UnsupportedHashFunction`
/ `RingSizeInversion` all-fatal); the Task-4 route `hash_policy` (header source;
`UnsupportedHashPolicy` rejects non-header sources); the Task-5 `HashRing` build+lookup
(`crates/envoy-cluster/src/ring_hash.rs`) + the `pick()` RingHash dispatch arm
(`crates/envoy-cluster/src/cluster.rs`; M27-1 + M27-2 folded; the `ring.is_some()` discriminator +
Maglev-footgun guard comment); the Task-6 `Option<u64>` request-hash threading through
`pick()`/`pick_endpoint()` + the H1/H2 HCM hash-key extraction (`crates/envoy-http1/src/hcm.rs`,
`crates/envoy-http2/src/hcm.rs` — the empty-vs-absent MUST-HAVE distinction); the Task-7 fixture
`0036-lb-ring-hash` (cross-proxy STRONG differential) + the `tests/differential/src/lib.rs` key-sweep
driver + the `{{BACKEND_IP}}`/`discover_host_lan_ip` shared-address mechanism; the Task-8 in-process
backstop (`crates/envoy-bin/tests/lb_ring_hash.rs`) + the `parse_bootstrap` fuzz seed; the Task-9
BEHAVIOR_CONTRACT "LB selection" extension.

## Cross-cutting focus areas — all PASS

1. **xxHash64-from-scratch fidelity — PASS.** The reviewer re-derived the algorithm independently
   (constants, the 4-lane block loop, `merge_round`, the 8/4/1-byte tail, the final avalanche, the
   seed-0 path) and all five locked unit vectors reproduce byte-for-byte: `xxh64("") =
   0xEF46DB3751D8E999`, `xxh64("abc") = 0x44BC2CF5AD770999`, plus the ≥32 / multiple-of-32 /
   ring-key-shape cases. The empty/`abc` vectors are the genuine canonical xxHash64 spec values; the
   longer two are documented as generated by an independent xxhash 3.7.0 library — pinned-oracle, not
   self-derived from the code under test.
2. **ADR-0070 ring algorithm byte-exactness — PASS.** `ring_hash.rs` key shape
   `format!("{address}_{i}")` preserves the load-bearing `_`; `replicas = min_ring_size / num_hosts`
   integer division; `sort_by_key` ascending; lookup via `partition_point(|h| h < key_hash)` is
   genuine `bisect_left` with the `pos == len → 0` wrap. The pinned 8-key oracle test + the 6-key
   backstop oracle anchor it to the live-Envoy §6.2 ground truth (36/36).
3. **empty-vs-absent header distinction — PASS.** Both HCMs route through the shared
   `request_hash_key` helper (`crates/envoy-http1/src/hcm.rs:349`), which is
   `lookup(...).map(hash_request_key)` — `.map()` not `.filter()`. H2 reuses the H1-computed key via
   the shared `build_response`, so there is a SINGLE extraction site (the two HCMs cannot diverge).
   Tests `request_hash_key_present_empty_is_some_not_none` + the backstop
   `empty_header_value_is_hashed_deterministically` assert both arms (present-empty → hashed; only
   absent → fallback).
4. **`ring.is_some()` LB discriminator — PASS (sound for 2 policies + guarded).** Using ring
   presence to discriminate RingHash-vs-RoundRobin is sound while only those two policies exist; the
   `from_bootstrap` build site carries a prominent guard comment warning that a future MAGLEV would
   misclassify and to replace it with an explicit `lb_policy` discriminant — exactly the footgun
   guard the focus area required.
5. **round-robin no-op regression-equivalence — PASS.** A `None` key falls straight through to the
   unchanged phase-02 cursor path; `key_hash` is inert. The 126 envoy-cluster lib tests, the 6-test
   backstop, the config tests, and all 35 pre-existing fixtures (CI `27837455306`) are green; the
   pre-existing suite was mechanically updated to `pick_endpoint(None)` with no behavioral change.
6. **fixture-0036 CI-portability — PASS.** `discover_host_lan_ip()` (route-based, sends no packets)
   renders one shared `{{BACKEND_IP}}` into BOTH proxies' endpoints, so both build the ring from
   identical `ip:port` strings (the cross-proxy precondition — the ring is IP-string-sensitive). The
   per-side IP split that EDS uses is correctly avoided; the README documents the failure signature
   and remediation. Risk did not materialize (green locally + on Linux CI).
7. **HC/OD + RING_HASH composition deferral — PASS (recorded §2.2 non-goal, not a gap).** The
   health/outlier skip-and-retry slow path that IS present is correct; the differential composition
   is a recorded deferred non-goal in the SPEC + BEHAVIOR_CONTRACT, not flagged as missing.
   `#![forbid(unsafe_code)]` holds across all five touched crates.

## Findings — Minor (3; non-blocking → phase-29 carry-forwards)

- **M28-1 — `maximum_ring_size` is parse-validated + stored but never consulted in the ring build**
  (`crates/envoy-cluster/src/ring_hash.rs` build path). Only `minimum_ring_size` drives the replica
  count (`min_ring_size / num_hosts`). Upstream Envoy's ketama build can scale replicas UP toward
  `maximum_ring_size` for small host counts; envoy-rust's build is `minimum_ring_size`-governed. This
  does not affect the validated 2-host/1024 oracle (512 replicas/host, far under 8M), which is why
  36/36 holds — but a future fixture with a host count where `min/num_hosts` collides with Envoy's
  max-scaling could silently diverge. **Fix:** no code change needed for this phase's validated scope;
  add one sentence to the `ring_hash.rs` module doc + BEHAVIOR_CONTRACT making explicit that
  `maximum_ring_size` is parse-validation-only and the build is `minimum_ring_size`-governed, so a
  later reader does not assume Envoy's full max-scaling is replicated.
- **M28-2 — the hash-key-absent fallback is the round-robin CURSOR path, where Envoy's documented
  fallback is a RANDOM host** (`cluster.rs` pick, RING_HASH + `None` key). This is a real behavioral
  divergence (deterministic cursor vs random). It is acceptable + correctly handled this phase: the
  BEHAVIOR_CONTRACT states the fallback is non-deterministic in Envoy and therefore NOT differentially
  asserted, and no fixture exercises it. The backstop's `None → host0 then host1` rotation assertion is
  a characterization of envoy-rust's internal behavior, NOT an Envoy-matched fidelity contract — fine
  as characterization, just not a fidelity guarantee. Note for whenever a hash-policy-absent path is
  ever differentially scoped.
- **M28-3 — the defense-in-depth `host_index < total` guard in the ring dispatch is dead under the
  current invariant** (`cluster.rs` ring arm). The ring is built once from the bootstrap set and never
  rebuilt on reload (RING_HASH+EDS is out of scope), so `host_index` always indexes `eps` validly. The
  guard is correct, harmless, and commented; it silently falls through to the cursor path rather than
  erroring if it ever tripped. No change needed; noted for the RING_HASH+EDS-reload future phase.

## Triage of per-task Minors already logged during state-3 (all CONFIRMED non-blocking)

- **Task-6 cosmetic minors** (logged in STATE.md): the extraction is config-shape-gated not
  LB-type-gated (wasted-but-harmless work for non-ring clusters); the `lookup` lifetime doc; the
  test-(a) None-arm over-specification; the TCP `None` degradation note. CONFIRM non-blocking — the
  holistic reviewer independently surfaced the same characterization concern (re-filed as **M28-2**)
  and rated nothing above Minor. The config-shape-gating is harmless (the extracted key is inert on
  the RoundRobin path).
- **Task-8 spread/cursor test-comment nits** (folded during state-3). CONFIRM non-blocking — the
  backstop tests pass on CI and exercise the edges the differential cannot reach (empty-is-hashed,
  single-host ring, the no-hash fallback).

## Recommendations

- Land **M28-1** (one doc sentence: `maximum_ring_size` is parse-validation-only; the build is
  `minimum_ring_size`-governed) opportunistically — cheapest when the ring code is next touched.
- When **MAGLEV** or **RING_HASH+EDS-hot-reload** lands, honor the already-written guard comment:
  convert the `ring.is_some()` dispatch to an explicit `lb_policy` discriminant and rebuild the ring
  on endpoint-set swaps (both already flagged in-code; closes M28-3).
- Consider a future fixture with an odd host count (e.g. 3 hosts, non-divisible `minimum_ring_size`)
  to bound the `min/num_hosts` integer-division divergence against live Envoy before relying on
  RING_HASH for non-power-of-two memberships (the empirical complement to M28-1).

## §7.5 phase-done gate (final)

(a) fixture `0036` GREEN + (b) all of `0001`–`0035` GREEN + (c) h2spec ≥95% + (d) `parse_bootstrap`
fuzz seed clean + (e) `cargo build`/`clippy`/`fmt --check`/`test`/`deny check` clean — ALL GREEN at
the AUTHORITATIVE Linux CI `27837455306` @ `4b31144`. (f) `REVIEW.md` approved — THIS document.
`#![forbid(unsafe_code)]` holds (D-3.8). **§7.5 gate (a)–(f) COMPLETE.**

---

_Verdict APPROVED (0C / 0I / 3 Minor → M28-1..M28-3 phase-29 carry-forwards). Range `50a03e4`..`4b31144`,
CI `27837455306`. Ledger head ADR-0070 (count 71; ADR-0071 reserved + UNFIRED). The state-6
deterministic close-out is the NEXT session._
