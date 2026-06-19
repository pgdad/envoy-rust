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

## State-3 Task 2 — xxHash64

**Implemented.** From-scratch xxHash64 (`pub(crate) fn xxh64(data: &[u8]) -> u64`) in
`crates/envoy-cluster/src/xxhash.rs`, seed fixed to 0 (Envoy `RING_HASH` default — no
arbitrary-seed generalization). Pure safe Rust per doctrine D-3.2 (no hashing crate added;
crate root `#![forbid(unsafe_code)]`): little-endian reads via `u64::from_le_bytes` /
`u32::from_le_bytes` plus `wrapping_*` / `rotate_left`. Covers the full algorithm — 4-lane
32-byte stripe loop + lane merge for input >=32 bytes, the `seed + PRIME64_5` short path,
`+= len`, the 8-byte / 4-byte / single-byte tail mixers, and the final avalanche. `xxh64`,
`round`, `merge_round` carry a module-level `#![allow(dead_code)]` (the ring consumer lands
in a later task; until then only the tests exercise the symbol). `mod xxhash;` added to
`crates/envoy-cluster/src/lib.rs`.

**Test vectors pinned (TDD — written + run failing before implementation):**
- `xxh64(b"")            == 0xEF46DB3751D8E999`  — LOCKED canonical (ADR-0070), empty path
- `xxh64(b"abc")         == 0x44BC2CF5AD770999`  — LOCKED canonical (ADR-0070), <32-byte tail
- `xxh64(b"The quick brown fox jumps over the lazy dog") == 0x0B242D361FDA71BC` — 43 bytes, block loop + tail
- `xxh64(b"0123456789abcdef" x4 = 64 bytes)      == 0x1AF3AC4760FE2F85` — exact multiple of 32 (two stripes, no tail)
- `xxh64(b"172.22.0.2:5678_0")                   == 0xFB4D13869ECAFECD` — realistic ring-key shape

**How the >=32-byte expected values were obtained:** the python `xxhash` library (xxhash 3.7.0,
`pip install --break-system-packages xxhash`) was available in-environment as a reference
*generator* (allowed — generating a test constant from an external tool is not a code
dependency). Both LOCKED canonical vectors reproduced byte-for-byte under
`xxhash.xxh64(..., seed=0)`, confirming seed=0 + the reference's correctness; the block-loop,
multiple-of-32, and ring-key constants were then generated from the same reference. No
hand-reasoned/un-justified constant was used.

**cargo test:** `cargo test -p envoy-cluster xxhash` →
`test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 111 filtered out`

**cargo clippy:** `cargo clippy -p envoy-cluster --all-targets --all-features -- -D warnings` →
`Finished \`dev\` profile [unoptimized + debuginfo] target(s)` (clean, no warnings).

## State-3 Task 3 — RingHash config + validators

Added the `RING_HASH` config surface + validators to `crates/envoy-config` (no LB
logic / request-path changes — deferred to later tasks). Strict TDD: the 8 tests
below were written and run **failing first** (compile errors: missing `RingHash`
variant, `ring_hash_lb_config` field, `HashFunction`, and the two `ConfigError`
variants), then implementation made them green.

**Implemented (`crates/envoy-config/src/bootstrap.rs`):**
- `LbPolicy::RingHash` (wire `RING_HASH` via the enum's `SCREAMING_SNAKE_CASE` rename).
- `RingHashLbConfig { minimum_ring_size: u64 (serde default 1024),
  maximum_ring_size: u64 (serde default 8_388_608), hash_function: HashFunction
  (serde default XxHash) }` — `#[serde(deny_unknown_fields)]`; defaults via
  `default_minimum_ring_size` / `default_maximum_ring_size` fns + `#[serde(default)]`
  on `hash_function`. A present-but-empty `ring_hash_lb_config: {}` therefore yields
  min 1024 / max 8_388_608 / hash XX_HASH.
- `HashFunction { XxHash, MurmurHash2 }` — `SCREAMING_SNAKE_CASE`, `#[default] XxHash`.
  `XxHash → XX_HASH`. `MurmurHash2` carries an EXPLICIT `#[serde(rename = "MURMUR_HASH_2")]`
  because serde's SCREAMING_SNAKE_CASE emits `MURMUR_HASH2` (no underscore before the
  trailing digit) — the Envoy wire name is `MURMUR_HASH_2`. **Modeling choice:** both
  Envoy wire values are RECOGNIZED at serde-parse so the phase-28 XX_HASH-only narrowing
  surfaces as a precise `ConfigError::UnsupportedHashFunction` (a documented divergence,
  ADR-0070) instead of an opaque serde unknown-variant error. A truly bogus value (e.g.
  `BOGUS_HASH`) still fails at serde parse (test (e)).
- `Cluster.ring_hash_lb_config: Option<RingHashLbConfig>` (serde default `None`,
  `skip_serializing_if = "Option::is_none"`).

**ConfigError (`crates/envoy-config/src/lib.rs` — that is where the enum lives):**
- `UnsupportedHashFunction { cluster }` — MURMUR_HASH_2 rejected (ADR-0070).
- `RingSizeInversion { cluster, minimum, maximum }` — min > max.
  Both all-fatal (ADR-0049).

**(g) accept-and-ignore decision + where validation is gated:** matching upstream
Envoy, a `ring_hash_lb_config` present on a NON-`RING_HASH` cluster is
ACCEPTED-AND-IGNORED (no error). To achieve this, the sub-config validation in
`validate_cluster` is **gated to `lb_policy == LbPolicy::RingHash`** — a
ROUND_ROBIN cluster's `ring_hash_lb_config` (even an otherwise-invalid one, e.g. a
ring-size inversion) is never validated. The validation runs at the TOP of
`validate_cluster`, BEFORE the EDS `load_assignment: None` early-return, so a
`RING_HASH` EDS cluster's sub-config is still checked at parse time. This makes all
of (d) [UnsupportedHashFunction], (f) [RingSizeInversion], and (g) [accept-and-ignore]
pass.

**Tests added (mirror `rejects_lb_policy_least_request` style; fixtures carry an
`admin` block to satisfy the `parse_bootstrap` NoRuntime gate):**
`parses_lb_policy_ring_hash` (a), `parses_ring_hash_lb_config_minimum_ring_size` (b),
`ring_hash_lb_config_absent_is_none` + `ring_hash_lb_config_empty_applies_defaults` (c),
`rejects_hash_function_murmur_hash_2` (d), `rejects_hash_function_bogus_value` (e,
serde unknown-variant), `rejects_ring_size_inversion` (f),
`accepts_ring_hash_lb_config_on_non_ring_hash_cluster` (g).

**cargo test:** `cargo test -p envoy-config` →
`test result: ok. 439 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`

**cargo clippy:** `cargo clippy -p envoy-config --all-targets --all-features -- -D warnings` →
`Finished \`dev\` profile [unoptimized + debuginfo] target(s)` (clean, no warnings;
one `collapsible_if` was fixed via a `&&  let` chain, consistent with existing usage).
