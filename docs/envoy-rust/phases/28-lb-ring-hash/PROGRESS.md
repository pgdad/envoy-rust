# Phase 28 — `28-lb-ring-hash` — PROGRESS

> Running log. The state-2 PLAN-write (§6.2 reconnaissance + PLAN authoring) is below;
> per-task state-3 entries append as the implementation lands.

## State-2 PLAN-write (this commit) — §6.2 VERIFIED LOCALLY at PLAN-write, ADR-0070 FIRED

`superpowers:writing-plans`. Ran the SPEC §6.2 / ADR-0069-mandated empirical reconnaissance
**at the PLAN-write** (the phase-27 verify-at-PLAN-write discipline), then authored `PLAN.md`.
**Unlike phases 26/27 this ran LOCALLY** — `RING_HASH` selection is a normal request/response
with NO file-watch/reload trigger, so it is observable on this Docker-Desktop host (and fixture
0036 will be locally authoritative, not Linux-CI-only).

### §6.2 method (Provenance)

Docker network `p28recon-net`; two distinguishable single-endpoint backends `hashicorp/http-echo
-text=BACKEND_ONE|BACKEND_TWO -listen=:5678` (IPs `172.22.0.2:5678` / `172.22.0.3:5678`); an
`envoyproxy/envoy:v1.33.0` container (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`)
on the same network — admin `:9901`, H1 listener `:10000` (`stat_prefix: ingress_http`, route `/`
→ cluster `ring_cluster`, route `hash_policy: [{ header: { header_name: "x-hash-key" } }]`,
`ring_cluster` = `type: STATIC` / `lb_policy: RING_HASH` / two equal-weight `lb_endpoints` / NO
HC / NO OD). A 27-key `x-hash-key` sweep recorded the backend per value; an independent
xxHash64-from-scratch replica (validated on canonical vectors) reproduced the selection. Host
left clean (all `p28recon-*` containers + network removed).

### §6.2 findings → ADR-0070 (FIRED)

- **Hash = xxHash64 seed 0** — CONFIRMED (vectors `xxh64("")=0xEF46DB3751D8E999`,
  `xxh64("abc")=0x44BC2CF5AD770999`).
- **The ring algorithm was CRACKED + EXACTLY VALIDATED — 36/36 keys** (27 oracle + 9 independent)
  reproduced live: per-host ring key `"<ip:port>_<i>"` (the `_` separator is load-bearing — other
  separators matched only 11–14/27), `replicas = minimum_ring_size / num_hosts` (1024/2 = 512),
  sorted `(hash, host)` ring, request hash `xxh64(header_value)`, `bisect_left` lookup with wrap.
- **STRONG differential target FIRES** — cross-proxy byte-identical selection is achievable + proven;
  the ADR-0069 same-key-stability fallback is NOT taken.
- **Determinism PASS** (5 keys × 10 = identical); **spread PASS** (27-key sweep 14 ONE / 13 TWO).
- **Config shapes** match the SPEC: cluster `lb_policy: RING_HASH`; `ring_hash_lb_config` optional
  (default `minimum_ring_size` 1024); fields `minimum_ring_size`/`maximum_ring_size`/`hash_function`
  (`XX_HASH` default | `MURMUR_HASH_2`); route `hash_policy: [{ header: { header_name } }]`.
- **Fallback:** absent `x-hash-key` → per-request RANDOM host (not stuck). **REFINEMENT:** an
  empty-but-present header value is HASHED (`xxh64("")`, deterministic), NOT the random fallback —
  only an ABSENT hash result falls back.
- **Invalid-config (ADR-0049 all-fatal, two classes):** bogus `hash_function` enum → proto
  parse-reject (exit 1); `minimum_ring_size > maximum_ring_size` → semantic init-reject (exit 1).
  `MURMUR_HASH_2` is a valid Envoy enum but is OUT of phase-28 scope → envoy-rust rejects it
  (the deliberate XX_HASH-only narrowing).

**The oracle mapping (the fixture-0036 / Task-5 regression ground truth)** — config: backend1
`172.22.0.2:5678`=ONE, backend2 `172.22.0.3:5678`=TWO, default `ring_hash_lb_config`:

```
key-0  ONE | key-1  ONE | key-2  TWO | key-3  TWO | key-4  TWO | key-5  TWO
key-6  TWO | key-7  TWO | key-8  TWO | key-9  TWO | key-10 ONE | key-11 TWO
key-12 ONE | key-13 TWO | key-14 ONE | key-15 TWO | key-16 ONE | key-17 ONE
key-18 ONE | key-19 TWO | key-20 ONE | key-21 ONE | key-22 TWO | key-23 ONE
user-alice ONE | session-abc123 ONE | 1.2.3.4 TWO
```

(The exact ip:port → host-index mapping is environment-specific; the LOAD-BEARING invariant the
implementation reproduces is the ALGORITHM — pin a subset of this table in the Task-5 unit test
using the same two address strings, and the Task-7 differential asserts cross-proxy agreement
regardless of the concrete backend addresses the harness assigns.)

### PLAN authored

10 tasks (Task 1 §6.2 DONE; Tasks 2–10 = implementation + verification). Spine: xxHash64-from-scratch
→ `LbPolicy::RingHash` + `RingHashLbConfig` config + validators → route `hash_policy` config → the
ring build/lookup + `pick()` dispatch → the request-hash threading (the 2 HCM call sites; round-robin
a no-op) → fixture 0036 + the key-sweep differential driver → in-process backstop + fuzz seed →
BEHAVIOR_CONTRACT "LB selection" → state-4 gate. **§6.1 single-phase confirmed** (~1000–1300 LoC /
~10 tasks, under the gate; **ADR-0071 UNFIRED**). The phase-27 carry-forwards M27-1 (`store_endpoints`
`pub(crate)`) + M27-2 (the `pick()` slow-path `debug_assert`) fold into Task 5 (the cluster/LB code).
PLAN plan-reviewed (see the state-2 commit).

**Outcome:** STATE advances to **state-2 PLAN-write COMPLETE / state-3-next** (next skill
`superpowers:subagent-driven-development`; Task 2 first). ADR-0070 FIRED (ledger head; count 71;
ADR-0071 reserved-but-unfired). The superseded state-1 top-section narrative is relocated to
`STATE_HISTORY.md` per ADR-0035; the `### Phase-28 state-1 brainstorm` Notes subsection STAYS (phase
28 is still in-progress — it relocates at the state-6 close-out). Per §5.1 the state-3 execution is
the NEXT session.
