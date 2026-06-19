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

## State-3 Task 4 — route hash_policy config

Added the route-level `hash_policy` config surface (parse + validate ONLY; no
HCM / request-path wiring — that is Task 6). `RouteAction_Route` (the
route-to-cluster action) gains `hash_policy: Vec<HashPolicy>` with
`#[serde(default, skip_serializing_if = "Vec::is_empty")]`, so an absent
`hash_policy` parses to an empty Vec — the regression-equivalence default
(every pre-existing route parses unchanged).

**HashPolicy serde model (recognize-then-reject, mirroring the Task-3 cluster
`HashFunction` style):** `HashPolicy` is a `#[serde(deny_unknown_fields)]` struct
with one `Option` per known Envoy `policy_specifier` oneof key —
`header: Option<HashPolicyHeader>` (the only MVP-supported source) plus
`cookie` / `connection_properties` / `query_parameter` / `filter_state`, each an
`Option<serde_yaml::Value>`. `HashPolicyHeader` is `{ header_name: String }`
(also `deny_unknown_fields`). Recognizing the unsupported specifier keys at parse
means the phase-28 narrowing surfaces as a precise fatal rather than an opaque
serde unknown-field error; a truly unknown key still fails at serde parse.

**Unsupported-source rejection:** new `validate_hash_policy(&HashPolicy)` returns
`ConfigError::UnsupportedHashPolicy { specifier }` (new variant in `lib.rs`) for
any non-`header` specifier (and for an empty oneof, `<none>`). It is called from
the inline-route validation loop in `validate()` for every `RouteAction::Route`
hash policy, so an unsupported source is startup-fatal (ADR-0049 all-fatal
posture) — the MVP never silently mis-routes by ignoring a hash policy.

**Tests added (`bootstrap::tests`):** `route_hash_policy_parses_header_source`
(a — one header policy, `header_name == "x-hash-key"`),
`route_hash_policy_absent_yields_empty_vec` (b — the empty-default
regression-equivalence proof), `route_hash_policy_collects_multiple_headers`
(a' — repeated field collects to len 2),
`route_hash_policy_rejects_unsupported_cookie_source` and
`route_hash_policy_rejects_unsupported_connection_properties_source` (c —
assert the specific `UnsupportedHashPolicy` variant + specifier string).

**cargo test:** `cargo test -p envoy-config` →
`test result: ok. 444 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
(prior count 439 + 5 new = 444; 0 failures — the empty-default regression proof).

**cargo clippy:** `cargo clippy -p envoy-config --all-targets --all-features -- -D warnings` →
`Finished \`dev\` profile [unoptimized + debuginfo] target(s)` (clean, no warnings).

**cargo fmt:** `cargo fmt -p envoy-config -- --check` → clean (one test line
reflowed by `cargo fmt` before the check passed).

## State-3 Task 5 — ring build + lookup + dispatch

**What landed.** The §6.2-LOCKED consistent-hashing ring core (ADR-0070):
- `crates/envoy-cluster/src/ring_hash.rs` — `pub(crate) struct HashRing { entries:
  Vec<(u64, usize)> }` (stores host INDEX, not address, to stay cheap). `build(
  addresses: &[String], min_ring_size: u64)` adds `replicas = min_ring_size /
  num_hosts` entries per host (integer division — 1024/2 = 512), each keyed
  `xxh64(format!("{address}_{i}").as_bytes())` (the `_` separator is LOAD-BEARING;
  `{i}` is the plain decimal index), then `sort_by_key` ascending by hash.
  `lookup(key_hash) -> Option<usize>` uses `partition_point(|h| h < key_hash)`
  (bisect_left → first `entry.hash >= key_hash`), wrapping to index 0 when the
  request hash exceeds every entry. Empty ring → `lookup` returns `None`
  (defensive — a RING_HASH cluster has ≥1 endpoint by construction; only a
  hot-reloaded empty set could reach it, and the caller's no-host path handles it).
- `crates/envoy-cluster/src/cluster.rs` — `Cluster` gains `ring:
  Option<crate::ring_hash::HashRing>`. **No separate `lb_policy` field**:
  `ring.is_some()` is the single source of truth for "this cluster is RING_HASH"
  (a stored `lb_policy` would be a dead field — clippy `dead-code` confirmed —
  so it was dropped). `from_bootstrap` builds the ring for `LbPolicy::RingHash`
  clusters from the endpoint `ip:port` Display strings, `min_ring_size` from the
  cluster's `ring_hash_lb_config.minimum_ring_size` (Envoy proto default 1024 when
  absent). Private `pick()` now takes `key_hash: Option<u64>` and dispatches:
  `Some(ring), Some(kh)` → `ring.lookup` → `eps[host_index]`; otherwise (None key,
  or RoundRobin with `ring == None`) falls through to the unchanged phase-02
  cursor/eligibility path. `ClusterHandle::pick_endpoint` passes `None` FOR NOW
  (behavior-preserving — Task 6 changes the delegate to thread the real key + wire
  the HCM call sites).
- `crates/envoy-cluster/src/lib.rs` — `mod ring_hash;`.

**Host-index ↔ endpoint alignment.** The ring is built by iterating the
`endpoints: Vec<SocketAddr>` in order, so ring `host_index i` == `endpoints[i]`.
`pick()` indexes the live `eps` snapshot directly by `host_index` (guarded by
`host_index < total` defense-in-depth). The ring is built ONCE from the bootstrap
endpoint set; an EDS hot-reload that swaps `endpoints` does NOT rebuild it
(RING_HASH + reloadable membership is out of phase-28 scope — STATIC fixtures
only).

**THE PINNED ORACLE — PASS.** `ring_hash::tests::pinned_oracle_matches_live_envoy`
builds the ring over host 0 = `172.22.0.2:5678` (ONE) / host 1 = `172.22.0.3:5678`
(TWO), `minimum_ring_size = 1024`, and asserts `lookup(xxh64(key))` for all 8
recorded keys — **all 8 PASS**:
`key-0`→0, `key-2`→1, `key-10`→0, `key-11`→1, `key-14`→0, `key-19`→1,
`user-alice`→0, `1.2.3.4`→1. The ring reproduces live Envoy v1.33.0 byte-for-byte.
The cluster-level selection test `cluster::tests::
pick_ring_hash_dispatch_and_round_robin_key_inert` confirms `pick(Some(xxh64(
b"key-0")))` → the host-0 endpoint, `pick(Some(xxh64(b"key-2")))` → the host-1
endpoint, and that a RoundRobin cluster's `pick(Some(123))` is identical to the
cursor path (key inert; matches `pick(None)`).

**HC/OD + RING_HASH composition — DEFERRED (non-goal).** Decided based on the
actual code shape: the MVP differential fixture is a PLAIN cluster, so the ring
returns a host directly. The skip-and-retry over ineligible ring hosts would
require threading the cluster's eligibility predicate into the ring walk (a
`lookup_eligible(pred)` that walks entries forward-with-wrap), coupling `HashRing`
to `Cluster`'s health/ejection state for a path no phase-28 fixture exercises.
Per SPEC §2.2 this composition is already a listed non-goal; gating the ring path
to plain clusters keeps `HashRing` decoupled. RING_HASH clusters are plain in
phase 28 — Task 9 records this in BEHAVIOR_CONTRACT.

**M27-2 (FOLDED).** Added `pick()` slow-path length-coupling assertions:
`debug_assert_eq!(eps.len(), h.len(), ...)` for `endpoint_health` and
`debug_assert_eq!(eps.len(), e.len(), ...)` for the outlier `ejection` array,
placed before the `is_eligible` closure that indexes them `[i]` for
`i in 0..eps.len()`. Guards a future HC/OD-wiring regression that desyncs them.

**M27-1 (FOLDED).** Tightened `Cluster::store_endpoints` `pub` → `pub(crate)`. The
only callers are the in-crate `eds_reload` pipeline and the `#[doc(hidden)] pub`
`ClusterHandle::store_endpoints` delegate (LEFT AS-IS — referenced cross-crate by
an `envoy-admin` test). No external crate reaches `&Cluster`/`Arc<Cluster>`
directly (`ClusterHandle::inner` is `pub(crate)`), so the tightening did not break
any crate's compilation. `cargo build --workspace` confirms (the only workspace
build error is the PRE-EXISTING `envoy-http1::hcm.rs` `hash_policy` gap — Task 6
wiring, present on the clean tree before this task; unrelated to M27-1).

**cargo test:** `cargo test -p envoy-cluster` →
`test result: ok. 123 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
(prior 116 + 7 new = 123: 6 `ring_hash` + 1 `cluster` selection test; 0 failures).

**cargo clippy:** `cargo clippy -p envoy-cluster --all-targets --all-features --
-D warnings` → `Finished \`dev\` profile [unoptimized + debuginfo] target(s)`
(clean, no warnings).

**cargo fmt:** `cargo fmt -p envoy-cluster -- --check` — the two TASK-5 files
(`ring_hash.rs`, `cluster.rs`) are fmt-clean. The package-level check still reports
3 PRE-EXISTING diffs in `xxhash.rs` (Task 2 debt: lines 42/78/86 — long
method-chain reflows), which is OUTSIDE this task's touch scope (only `ring_hash.rs`
/ `cluster.rs` / `lib.rs` / `PROGRESS.md` were touched). Per the recorded host note,
fmt-check is native-CI-authoritative and resolved at the state-4 gate — the
`xxhash.rs` reflow lands then (it is not Task 5's to edit).

## State-3 Task 6 — request-hash plumbing

**What was implemented.** Threaded the REAL per-request hash key from the HTTP
request through to LB selection.

- **envoy-cluster:** `ClusterHandle::pick_endpoint` now takes its own
  `Option<u64>` request-hash-key and passes it to the private `Cluster::pick`
  (Task 5). All inert (RoundRobin) call sites pass `None`
  (behavior-preserving): the crate's own tests, `eds_reload.rs`, and the
  `envoy-tcp` production caller (TCP proxying has no HTTP `hash_policy`). New
  `pub fn hash_request_key(&[u8]) -> u64` is the ONLY new public hashing
  surface — a thin wrapper over the STILL-`pub(crate)` `xxh64` (re-exported from
  `lib.rs`).
- **envoy-config:** re-exported `HashPolicy` / `HashPolicyHeader` from `lib.rs`
  (Task 4 added them to `bootstrap.rs` but never re-exported) so the HCMs can
  name them.
- **HCMs (H1 + H2):** `BuildOutcome::Proxy` gained `request_hash_key:
  Option<u64>`, computed ONCE in the shared `build_response` (H1) from the
  matched route's `hash_policy` against the request headers, then threaded to
  `run_attempt` / `run_h2_attempt` → `pick_endpoint`. H2 reuses H1's
  `build_response`, so the single compute site covers both protocols.

**The empty-vs-absent distinction (ADR-0070) + its test.** Lives in the new H1
helper `fn request_hash_key(policies, lookup) -> Option<u64>` in
`crates/envoy-http1/src/hcm.rs`. It is exactly
`lookup(name).map(envoy_cluster::hash_request_key)` — NEVER
`.filter(|v| !v.is_empty())`. The header lookup uses `find_header`, which
returns `Some(value)` whenever the header is PRESENT (even empty) and `None`
when ABSENT. So a present-empty `x-hash-key:` header → `Some(xxh64(b""))` (a
deterministic point on the ring, NOT the random-host fallback), and an absent
header → `None` (the fallback). The MUST-HAVE test (c)
`request_hash_key_present_empty_is_some_not_none` asserts BOTH cases and
**passes**; companions `request_hash_key_present_nonempty_is_hashed` (d) and
`request_hash_key_empty_policy_is_none` also pass.

**MVP single-header-policy choice.** When the matched route has one or more
`HashPolicy` entries, the FIRST entry with a `header` source wins
(`policies.iter().find_map(|p| p.header.as_ref())`). Empty `hash_policy` →
`None` without consulting the header lookup (the common, allocation-free
non-RING_HASH path). Multi-policy combination + non-header sources remain
deferred non-goals (config parse already rejects non-header sources).

**Struct-literal build-break sites fixed (the Task-4 `hash_policy` gap).**
`clone_route_action` in `crates/envoy-http1/src/hcm.rs` (production, the
documented `:320` break) now clones `hash_policy`; the 15 H1 test-fixture
literals and the 2 H2 test-fixture literals (`crates/envoy-http2/src/hcm.rs`)
now set `hash_policy: vec![]`. The whole workspace compiles.

**Workspace regression (the round-robin no-op proof).**
`cargo test --workspace --exclude differential` →
`TOTAL passed: 1216  failed: 0` (every existing round-robin fixture unchanged;
the signature change + new extraction are behavior-preserving). Per-crate:
`cargo test -p envoy-cluster` → `test result: ok. 126 passed; 0 failed`;
`-p envoy-http1` → `123 passed; 0 failed`; `-p envoy-http2` →
`72 passed; 0 failed; 1 ignored`.

**clippy:** `cargo clippy --workspace --all-targets --all-features -- -D
warnings` → clean (exit 0). **fmt:** `cargo fmt --all -- --check` → clean
(whole tree fmt-clean, including the prior `xxhash.rs` Task-2 debt resolved by
the tree-wide `cargo fmt --all`).
