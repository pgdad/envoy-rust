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

## State-3 Task 7 — fixture 0036 differential

**Fixture shape.** `tests/fixtures/0036-lb-ring-hash/` (`envoy.yaml`,
`envoy-rust.yaml`, `expectations.yaml`, `README.md`). ONE STATIC H1 HCM listener
(`stat_prefix: ingress_http`) routes `/` → a STATIC `lb_policy: RING_HASH`
cluster `ring_cluster` with TWO distinguishable echo backends
(`--body-marker backend_1`/`backend_2`); the route action carries
`hash_policy: [{ header: { header_name: "x-hash-key" } }]`. A PLAIN cluster —
NO health check, NO outlier detection. `ring_hash_lb_config.minimum_ring_size:
1024` set explicitly (= default; 1024 / 2 hosts = 512 replicas/host).
`envoy.yaml` and `envoy-rust.yaml` are IDENTICAL config except the listener bind
address (`0.0.0.0` upstream vs `127.0.0.1` subject — standard harness per-side
convention). NO behavioral divergence.

**Load-bearing addressing fix.** The ring key is `xxh64("{ip:port}_{i}")`, so
cross-proxy identical selection holds ONLY if both proxies build the ring from
IDENTICAL endpoint `ip:port` strings. The EDS per-side IP split (host-gateway IP
upstream / `127.0.0.1` subject) would defeat this. A new `{{BACKEND_IP}}` marker
(`discover_host_lan_ip` — route-based, no packets) renders to ONE SHARED host
LAN IPv4 on BOTH sides; the subject reaches the `0.0.0.0`-bound backends
directly and the upstream container reaches the same host backends via the
Docker bridge / Desktop-VM NAT (verified reachable from both). A STATIC cluster
rejects hostnames, hence a numeric IP.

**Driver.** New `Driver::Http1HashSweep` in `tests/differential/src/lib.rs`
sweeps 16 distinct `x-hash-key` values — `key-0..key-5`, `user-alice`,
`user-bob`, `user-carol`, `1.2.3.4`, `10.0.0.1`, `session-abcdef`,
`session-123456`, `tenant-acme`, `tenant-globex`, `cart-99` (includes the §6.2
oracle keys `key-0`, `key-2`, `user-alice`, `1.2.3.4`). For each key it sends
`GET /` with the header to BOTH proxies (twice) and extracts each response
body's leading `backend: <marker>\n` line. Assertions: **STRONG** — per-key the
envoy-rust marker is IDENTICAL to upstream Envoy's (cross-proxy identical
RING_HASH selection, ADR-0070); **SPREAD** — over the sweep BOTH `backend_1` and
`backend_2` are selected on EACH side; **STABILITY** — each key probed twice
hits the SAME backend per proxy. `tests/differential/tests/lb_ring_hash.rs` is
the Docker-gated wrapper (`run_fixture`, no per-test cfg gate — the harness
skips when `DOCKER_HOST` is unavailable).

**Local Docker differential witness (GREEN).** `cargo test -p differential
--test lb_ring_hash` against live `envoyproxy/envoy:v1.33.0` (image present
locally, digest pinned in `ENVOY_TARGET.md`):

```
running 1 test
test lb_ring_hash_fixture ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.32s
```

Stable across 3 consecutive runs (no ephemeral-port / ring-rebuild flake). This
phase's differential is LOCALLY observable (a plain request/response with NO
file-watch/reload trigger), so this Docker run is the authoritative differential
witness on this dev host — NOT native-Linux-CI-only (unlike phases 26/27).

**Schema lock-in.** New unit test
`driver_http1_hash_sweep_round_trips_through_serde` round-trips the
`http1_hash_sweep` expectations YAML (mirrors the RDS/EDS round-trip tests).

**clippy:** `cargo clippy -p differential --all-targets --all-features --
-D warnings` → clean (exit 0). **fmt:** `cargo fmt --all -- --check` → clean.
**lib unit tests:** `cargo test -p differential --lib` → 147 passed, 0 failed,
2 ignored.

**Task 7 review-fold: CI-portability documentation.** Folded the code-review
Important item (resolves to DOCUMENTATION, not a mechanism change — the shared
`{{BACKEND_IP}}` host-LAN-IP approach is CORRECT and stays): added a "CI
portability" section to the fixture README (rationale for bridge-routing to the
host LAN IP vs the other fixtures' `host-gateway` mapping, the local-Docker-vs-
Linux-CI path difference, and the all-keys-non-200 failure signature + first
diagnostic + remediation), a one-line egress-interface caveat to the
`discover_host_lan_ip` doc comment, and a cosmetic `host: ring_cluster` →
`host: localhost` in `expectations.yaml` (trivially safe — both vhosts are
`domains: ["*"]`).

## State-3 Task 8 — backstop + fuzz seed

**In-process RING_HASH backstop** (`crates/envoy-bin/tests/lb_ring_hash.rs`) —
the deterministic complement to the `0036-lb-ring-hash` cross-proxy differential.
The differential proves consistent-hashing selection BILATERALLY, but only for a
single shape: a PLAIN STATIC cluster where the `x-hash-key` header is ALWAYS
present. The backstop drives the cluster through the PUBLIC production path
(`parse_bootstrap` → `load_dynamic_resources` → `envoy_cluster::from_bootstrap`
→ `ClusterHandle::pick_endpoint`, with the per-request key computed via the
PUBLIC `envoy_cluster::hash_request_key`), construction via an admin-only
bootstrap (the kernel-ephemeral `port_value: 0` admin satisfies the config
NoRuntime gate — no listener/data-plane needed since the cluster is driven
directly). Six characterization/regression cases (all PASS, would FAIL on a
ring/fallback regression):

1. **ring determinism + the §6.2 oracle subset** — the SAME six key→host pins
   Task 5 locked: `key-0`→host 0, `key-2`→host 1, `key-10`→host 0,
   `key-11`→host 1, `user-alice`→host 0, `1.2.3.4`→host 1 (host 0 =
   `172.22.0.2:5678`, host 1 = `172.22.0.3:5678`, `minimum_ring_size: 1024`);
   each key also re-picks deterministically. The §6.2 oracle is ground truth —
   the test asserts against it, not against observed output.
2. **spread** — over ~16 keys both hosts are selected at least once.
3. **the no-hash-key fallback** — `pick_endpoint(None)` on a RING_HASH cluster
   returns a VALID host (not a panic, not `None`). This is the ABSENT-header path
   the differential never exercises (fixture 0036 always sends the header).
4. **empty-header-value-is-HASHED** — `Some(hash_request_key(b""))` is a
   DETERMINISTIC hashed selection (`xxh64("")`, stable across 8 repeats), NOT the
   random/None fallback (the integration companion to Task 6's helper test (c)).
5. **single-host ring** — a one-endpoint RING_HASH cluster: every key AND `None`
   route to the one host.
6. **ROUND_ROBIN-ignores-key regression** — `pick_endpoint(Some(123))` behaves
   exactly like the cursor path / `pick_endpoint(None)` (the key is inert; no
   ring is built for a non-RING_HASH cluster).

(No HC/OD + RING_HASH skip-retry test — that composition is a §2.2 deferred
non-goal; the MVP cluster is plain.)

**Fuzz corpus seed** (`crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_ring_hash_lb.yaml`,
force-added past the fuzz `.gitignore` like the other 36 tracked seeds) — a
parse-valid bootstrap exercising the new config surface for the EXISTING
`parse_bootstrap` fuzz target (no new target): a `RING_HASH` cluster with a full
`ring_hash_lb_config` (`minimum_ring_size`/`maximum_ring_size`/
`hash_function: XX_HASH`) AND a route `hash_policy: [{ header: { header_name:
"x-hash-key" } }]`. Confirmed parse-valid via a throwaway in-test parse round-trip
(removed before commit).

**test:** `cargo test -p envoy-bin --test lb_ring_hash` → `test result: ok. 6
passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`. **clippy:** `cargo
clippy -p envoy-bin --all-targets --all-features -- -D warnings` → clean (exit 0).
**fmt:** `cargo fmt --all -- --check` → clean.

## State-3 Task 9 — BEHAVIOR_CONTRACT LB selection

Added a per-feature **"LB selection"** subsection to
`docs/envoy-rust/BEHAVIOR_CONTRACT.md` (inserted after "Request body
forwarding (HTTP/1.1)", before "Header allow-list" — alongside the other
data-path equivalence notes), matching the file's heading/density/tone. Docs-only;
`git diff --stat` confirmed BEHAVIOR_CONTRACT.md as the sole change (+84 lines).

Documented:

- **`ROUND_ROBIN`** (default since phase 02, unchanged this phase) — cursor-based
  rotation over eligible endpoints; the per-request hash key is **inert** for
  round-robin (the regression-equivalence proof: all 35 pre-phase-28 fixtures stay
  green; the `pick()`/`pick_endpoint()` hash-key signature change is
  behavior-preserving).
- **`RING_HASH`** (NEW, phase 28) — deterministic + byte-identical to upstream
  Envoy v1.33.0 via the ADR-0070 algorithm: **xxHash64 seed 0** (from scratch);
  per-host ring key `"{ip:port}_{i}"` (load-bearing `_` separator; IPv4
  `SocketAddr` Display; IPv6 ring hosts an untested non-goal); `replicas =
  minimum_ring_size / num_hosts`; sorted `(hash, host)` ring; request hash =
  `xxh64(header value)`; lookup = first entry `>= request_hash` wrapping
  (`bisect_left`).
- **Keying** — route-level header `hash_policy` (`{ header: { header_name } }`);
  single-header-source MVP.
- **Empty-vs-absent** — empty-but-present header value is HASHED (`xxh64("")`,
  deterministic), NOT the fallback; only an ABSENT key falls back. The
  no-`hash_policy`-match / absent-header → **random-host fallback is NOT
  differentially asserted** (non-deterministic; backstop-only).
- **XX_HASH-only narrowing** — `MURMUR_HASH_2` rejected (all-fatal config error,
  documented intentional divergence); bogus enum → parse-reject;
  `minimum_ring_size > maximum_ring_size` → validation-reject.
- **Differential witness** — fixture `0036-lb-ring-hash` (cross-proxy identical
  RING_HASH selection per key; locally observable, no reload trigger).

**Deferred HC/OD + RING_HASH non-goal — RECORDED.** The subsection explicitly
records (per doctrine D-3.3) that the ring skip-and-retry over ineligible
(unhealthy / ejected) hosts is a **SPEC §2.2 deferred non-goal** (the Task 5
decision): the phase-28 fixture cluster is PLAIN, so the differential does NOT
exercise the eligibility-skip path — `RING_HASH` over an HC/OD cluster is **not yet
differentially validated** (backstop-only). The weighted ring, non-header hash
sources, `maglev`, `least_request`/`random`, and `RING_HASH` + EDS-hot-reload
composition are also noted as deferred (brief; SPEC §2.2).

Docs-only task — no build/test. The two commits touch only BEHAVIOR_CONTRACT.md
(commit 1) and this PROGRESS.md (commit 2).

## State-4 verification gate (§7.5)

Task 10 (state-4) — verification gate RUN locally at HEAD `0cec703` (clean tree,
branch `main`). Each gate below quotes the ACTUAL observed result. Per the
project's state-4 discipline, this dev host is Docker-Desktop/virtiofs: the
NON-Docker workspace suite + clippy + fmt + build + deny are **locally
authoritative**; the FULL differential matrix (fixtures 0001–0035) + h2spec ≥95%
+ the fuzz short-budget are **authoritative on the Linux CI run**. Fixture
`0036-lb-ring-hash` (the NEW phase-28 differential) is LOCALLY observable this
phase (plain request/response, no reload trigger) — so green locally is REAL
cross-proxy evidence.

| # | Gate | Command | Result | Marker |
|---|------|---------|--------|--------|
| 1 | fmt | `cargo fmt --all -- --check` | exit 0, no diff | **PASS** (local-authoritative) |
| 2 | build | `cargo build --workspace --all-targets` | `Finished dev profile ... in 8.98s`, exit 0 | **PASS** (local-authoritative) |
| 3 | clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | `Finished ... in 2.00s`, exit 0 | **PASS** (local-authoritative) |
| 4 | workspace test (`--exclude differential`) | `cargo test --workspace --exclude differential` | 69 `test result:` lines, ALL `0 failed`; 1 pre-existing `ignored`; 0 errors/panics; exit 0 | **PASS** (local-authoritative) |
| 5 | differential fixture 0036 | `cargo test -p differential --test lb_ring_hash` | `test result: ok. 1 passed; 0 failed ... in 1.35s`, exit 0 (real Docker 28.1.1, envoy v1.33.0) | **PASS / GREEN** (locally observable — REAL cross-proxy witness) |
| 6 | deny | `cargo deny check` | `advisories ok, bans ok, licenses ok, sources ok`, exit 0; 4 `license-not-encountered` WARNINGS (unmatched allowances in deny.toml — warnings, NOT errors) | **PASS** (local-authoritative) |
| 7 | fuzz short-budget (`parse_bootstrap`) | `cargo +nightly fuzz --version` | `error: no such command: fuzz` (exit 101) — nightly IS installed, but `cargo-fuzz` is NOT installed locally; target `parse_bootstrap.rs` present | **CI-AUTHORITATIVE / DEFERRED** (could not run locally; covered by Linux CI fuzz gate) |

### Gate 4 detail (per the host flakiness note)
Full suite was clean on the FIRST run — every one of the 69 `test result:`
lines reported `0 failed`, with no `FAILED`/`failures:`/`panicked`/`error[`
lines anywhere in the log. The single `ignored` is pre-existing (not phase-28).
**No test failed, so no isolation re-run was required** (no load-induced
boot-timing flake observed this run).

### Overall summary
- **Local gates: GREEN.** fmt, build, clippy, the full non-Docker workspace
  suite, and deny all pass with quoted exit-0 / zero-failure evidence.
- **Fixture 0036: GREEN locally** — the phase-28 RING_HASH cross-proxy payoff
  ran end-to-end against upstream Envoy v1.33.0 under real Docker (1 passed, 0
  failed). This is REAL evidence, not CI-deferred.
- **Deferred to Linux CI (authoritative there):** the full pre-existing
  differential matrix (fixtures 0001–0035), h2spec conformance (≥95%), and the
  `parse_bootstrap` fuzz short-budget (cargo-fuzz not installed on this dev
  host). NOT run locally; NOT claimed green here.
- **Known unrelated local Docker divergence:** the `admin_config_dump`
  /`server_info` virtiofs issue is a pre-existing host artifact, NOT a phase-28
  regression (not exercised by this gate).
- **No regressions detected.** Nothing failed locally; nothing was faked.
