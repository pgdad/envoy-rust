# Phase 28 — `28-lb-ring-hash` — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL — use `superpowers:subagent-driven-development` to execute this plan task-by-task (fresh subagent per task, SERIAL — never parallel, they race on `main`; TDD per `superpowers:test-driven-development`; per-task `cargo clippy --workspace --all-targets --all-features -- -D warnings`; one code commit + one PROGRESS commit per task; two-stage review [spec-compliance THEN code-quality] per task). Steps use `- [ ]` checkboxes.
>
> **Goal:** Open the Load-balancing family with Envoy's `RING_HASH` consistent-hashing load balancer — a cluster `lb_policy: RING_HASH` routes a request (keyed by a route header `hash_policy`) to the endpoint its hashed key maps to on a ketama ring, byte-identical to upstream Envoy v1.33.0.
>
> **Architecture:** A from-scratch xxHash64 (seed 0) feeds a sorted hash ring built over the cluster's endpoints; `Cluster::pick()` gains a `RingHash` dispatch arm that takes a request hash key and does a `bisect_left` ring lookup. The route `hash_policy` (header source) extracts the key in the HCM request path and threads it through `pick_endpoint()`. The `ROUND_ROBIN` path is unchanged (key ignored) — all 35 existing fixtures stay green.
>
> **Tech stack:** Rust (`envoy-cluster`, `envoy-config`, `envoy-http1`, `envoy-http2`), the `testcontainers` differential harness, the phase-27 two-distinguishable-backend harness seed. NO new dependency (xxHash64 written from scratch per D-3.2).
>
> **Scope lock:** SPEC `docs/envoy-rust/phases/28-lb-ring-hash/SPEC.md`; **ADR-0069** (pick + scope); **ADR-0070** (the §6.2-VERIFIED ring algorithm — STRONG differential target). **§6.1 single-phase confirmed (ADR-0071 UNFIRED).** Differential is LOCALLY observable (no reload trigger).

---

## The §6.2-LOCKED ring algorithm (ADR-0070 — the EXACT spec every task below depends on)

Validated 36/36 against live `envoyproxy/envoy:v1.33.0`. The implementation MUST reproduce this exactly (a one-character change to the separator breaks the differential):

1. **Hash:** `xxHash64` with **seed 0**. Canonical vectors: `xxh64("") == 0xEF46DB3751D8E999`; `xxh64("abc") == 0x44BC2CF5AD770999`.
2. **Ring build:** for each host, add `replicas = minimum_ring_size / num_hosts` entries (equal weight; e.g. 1024/2 = 512). Entry `i` (decimal `0..replicas-1`) has ring hash `xxh64( format!("{address}_{i}"), seed=0 )` where `{address}` is the host's `ip:port` string (e.g. `172.22.0.2:5678`). **The `_` separator is load-bearing.**
3. **Ring:** collect all `(hash: u64, host_index)` entries; **sort ascending by `hash`**.
4. **Request hash:** `xxh64( header_value_bytes, seed=0 )` — the raw `hash_policy` header value bytes.
5. **Lookup:** the first ring entry with `entry.hash >= request_hash`; if none, **wrap to index 0** (`bisect_left` / first-clockwise).
6. **Fallback:** an ABSENT hash key (no `hash_policy` match / header missing) → the no-hash path (Envoy: random host). An **empty-but-present** header value → HASHED normally (`xxh64("")`), NOT fallback.
7. **Config:** `lb_policy: RING_HASH` at the cluster; `ring_hash_lb_config` optional (`minimum_ring_size` default 1024, `maximum_ring_size` default ~8M, `hash_function` accept **XX_HASH only** → `MURMUR_HASH_2` is an all-fatal config error this phase). Invalid: bogus enum → parse-reject; `minimum_ring_size > maximum_ring_size` → validation-reject (both ADR-0049 all-fatal).

---

## Task 1 — §6.2 empirical reconnaissance — **DONE** (this state-2 PLAN-write commit)

Ran the §6.2 recon LOCALLY against `envoyproxy/envoy:v1.33.0`; cracked + validated the ring algorithm 36/36; STRONG differential target confirmed; **ADR-0070 FIRED**. The locked algorithm is above; the full transcript is in `PROGRESS.md` (the state-2 §6.2 entry). No code. ✅

---

## Task 2 — xxHash64 (seed 0) from scratch [D-of-the-hash]

**Files:** Create `crates/envoy-cluster/src/xxhash.rs`; Modify `crates/envoy-cluster/src/lib.rs` (add `mod xxhash;`).

- [ ] **Write failing tests first** (`#[cfg(test)]` in `xxhash.rs`): the canonical vectors `xxh64(b"") == 0xEF46DB3751D8E999`, `xxh64(b"abc") == 0x44BC2CF5AD770999`, plus a few multi-block inputs cross-checked against a reference (e.g. `xxh64(b"123456789012345")`, a >32-byte input to exercise the 4-lane block path, and a string with non-ASCII bytes). Include `xxh64` of a realistic `"172.22.0.2:5678_0"` ring key (value computed once and pinned).
- [ ] **Implement** `pub fn xxh64(data: &[u8]) -> u64` (seed fixed to 0 — Envoy's `RING_HASH` default; do not over-generalize to arbitrary seeds unless a second caller needs it). Standard xxHash64: the 4×u64 accumulator lanes (PRIME64_1..5), the >=32-byte block loop, the tail merge, the avalanche finalization. Pure safe Rust (`#![forbid(unsafe_code)]` holds — use `u64::from_le_bytes` on slices + `wrapping_*`/`rotate_left`).
- [ ] Run `cargo test -p envoy-cluster xxhash` → all vectors PASS. `cargo clippy -p envoy-cluster --all-targets --all-features -- -D warnings`.
- [ ] Commit (code) + PROGRESS commit.

**Why a unit, not a crate:** xxHash64 is internal to LB; keep it `pub(crate)` in `envoy-cluster`. NO new dependency (D-3.2).

## Task 3 — `LbPolicy::RingHash` + `RingHashLbConfig` config + validators [config]

**Files:** Modify `crates/envoy-config/src/bootstrap.rs` (the `LbPolicy` enum at `:290` — add `RingHash`; the cluster struct — add `ring_hash_lb_config: Option<RingHashLbConfig>`; a new `RingHashLbConfig { minimum_ring_size: u64 (default 1024), maximum_ring_size: u64 (default 8388608), hash_function: HashFunction }` + `HashFunction { XxHash }` enum); the cluster parse/validate path; the `ConfigError` enum (new variants).

- [ ] **Failing tests first** (config parse tests, mirroring the existing `rejects_lb_policy_least_request` at `bootstrap.rs:4319`): (a) `lb_policy: RING_HASH` parses + sets the variant; (b) `ring_hash_lb_config: { minimum_ring_size: 1024 }` parses; (c) omitted `ring_hash_lb_config` → defaults (min 1024 / max 8388608 / XX_HASH); (d) `hash_function: MURMUR_HASH_2` → a NEW fatal `ConfigError` (the phase-28 XX_HASH-only narrowing — Envoy accepts it, envoy-rust rejects, a documented divergence per ADR-0070); (e) a bogus `hash_function` enum → parse error; (f) `minimum_ring_size > maximum_ring_size` → a NEW fatal validation `ConfigError`; (g) `ring_hash_lb_config` present on a non-`RING_HASH` cluster → accepted-and-ignored OR a warn (match Envoy — §6.2 noted Envoy accepts it; default to accept-ignore unless a test shows otherwise).
- [ ] **Implement** the enum variants, the `RingHashLbConfig` struct (serde defaults), and the two new `ConfigError` variants (`UnsupportedHashFunction` for MURMUR_HASH_2; `RingSizeInversion` for min>max). The XX_HASH-only rejection + the min>max validation are ADR-0049 all-fatal.
- [ ] Run `cargo test -p envoy-config` (+ the new tests). Clippy.
- [ ] Commit (code) + PROGRESS commit.

## Task 4 — route header `hash_policy` config [config]

**Files:** Modify `crates/envoy-config/src/bootstrap.rs` (`RouteAction_Route` at `:1303`, today `{ cluster, retry_policy }` — add `hash_policy: Vec<HashPolicy>` (default empty); a new `HashPolicy` enum/struct with a `Header { header_name: String }` source).

- [ ] **Failing tests first:** (a) a route with `hash_policy: [{ header: { header_name: "x-hash-key" } }]` parses → one `HashPolicy::Header`; (b) a route with NO `hash_policy` → empty Vec (default; the regression-equivalence default — every existing fixture's routes parse unchanged); (c) a `hash_policy` with an unsupported source (e.g. `cookie`/`connection_properties`) → either a fatal `ConfigError` (preferred — keep MVP tight) or accept-and-ignore-non-header (decide per the §6.3 boundary; default to a fatal `UnsupportedHashPolicy` so the MVP can't silently mis-route).
- [ ] **Implement** the `HashPolicy` schema (header source only) + the parse + the validator. NO HCM change yet (Task 6 threads it).
- [ ] Run `cargo test -p envoy-config`. Clippy.
- [ ] **Regression check:** `cargo test -p envoy-config` — all existing config/route parse tests still green (the empty-default proof).
- [ ] Commit (code) + PROGRESS commit.

## Task 5 — the ring: build + lookup + `RingHash` dispatch in `pick()` [LB core]

**Files:** Create `crates/envoy-cluster/src/ring_hash.rs` (the ring type + build + lookup); Modify `crates/envoy-cluster/src/cluster.rs` (the `Cluster` gains an `Option<HashRing>` built at construction for `RING_HASH` clusters; `pick()` at `:322` gains a `RingHash` arm; `from_bootstrap` at `:893` builds the ring). Modify `lib.rs` (`mod ring_hash;`).

- [ ] **Failing tests first** (`ring_hash.rs` unit tests + a `cluster.rs` selection test): (a) a `HashRing::build(&[addr1, addr2], min_ring_size=1024)` produces `1024` entries (512/host) sorted ascending; (b) `ring.lookup(request_hash)` returns the first entry `>= request_hash`, wrapping past the max; (c) **the pinned oracle**: build the ring over `["172.22.0.2:5678", "172.22.0.3:5678"]` and assert the §6.2 recon's recorded mapping — e.g. `lookup(xxh64(b"key-0")) → host 0` (ONE), `lookup(xxh64(b"key-2")) → host 1` (TWO), … (pin ~8 of the 27 oracle keys from the PROGRESS §6.2 table as the regression oracle); (d) single-host ring → every key → that host; (e) determinism (same key → same host).
- [ ] **Implement** `HashRing { entries: Vec<(u64, usize)> }` (host index, not address, to stay cheap), `build(addresses: &[String], min_ring_size: u64)` using the Task-2 `xxh64` over `format!("{addr}_{i}")`, sorted; `lookup(&self, key_hash: u64) -> usize` via `partition_point`/binary search with wrap. In `cluster.rs`: build the ring in `from_bootstrap` from the endpoint address strings (the `ip:port` form — `SocketAddr`'s Display, **verified for IPv4 = `172.22.0.2:5678`, matching Envoy's `address()->asString()`**; NOTE IPv6 `SocketAddr` Display is bracketed `[::1]:5678` — the fixture is IPv4 so the differential is safe; scope the guarantee to IPv4 and treat IPv6 ring hosts as an untested §2.2-adjacent non-goal); add a `RingHash` arm to `pick()` that, given `Some(key_hash)`, does `ring.lookup` (composing with the health/outlier eligibility — see the compose note); given `None`, falls back to the no-hash path.
- [ ] **Health/outlier compose:** the MVP fixture is a PLAIN cluster, so the ring lookup returns a host directly. For an HC/OD cluster (backstop-only), Envoy skips an ineligible host to the next ring entry — implement the skip-and-retry over the ring for the slow path, OR (if heavy) gate `RING_HASH` ring-build to plain clusters + document HC/OD+RING_HASH as a deferred non-goal (SPEC §2.2). **Decide at implementation; prefer the skip-retry if it's <~30 LoC, else defer with a recorded non-goal.**
- [ ] **Fold M27-2** (the phase-27 carry-forward): add the `pick()` slow-path `debug_assert_eq!(eps.len(), health.len())` length-coupling while in this code. **Fold M27-1**: tighten the INNER `Cluster::store_endpoints` `pub` → `pub(crate)` if it's in-crate-only — but LEAVE the `ClusterHandle::store_endpoints` `#[doc(hidden)] pub` delegate as-is (it is referenced cross-crate by an `envoy-admin` test, so it cannot drop to `pub(crate)`).
- [ ] Run `cargo test -p envoy-cluster`. Clippy.
- [ ] Commit (code) + PROGRESS commit.

## Task 6 — request-hash plumbing: thread the key through `pick_endpoint()`/`pick()` + the HCM call sites [request path]

**Files:** Modify `crates/envoy-cluster/src/cluster.rs` — the request-hash-key param `Option<u64>` is threaded through **BOTH** impl sites (they are on DIFFERENT types): the PRIVATE core `Cluster::pick` at `:322` AND the PUBLIC delegate `ClusterHandle::pick_endpoint` at `:549` (which calls `self.inner.pick(...)`). Updating only one will not compile. Modify `crates/envoy-http1/src/hcm.rs` (`:392` call site) and `crates/envoy-http2/src/hcm.rs` (`:184` call site) — before the pick, resolve the matched route's `hash_policy`, extract the named header's value from the request, compute `xxh64(value)` if the header is PRESENT (incl. empty value), and pass `Some(hash)` / `None`.

- [ ] **Failing tests first:** (a) a `cluster.rs` test: `RING_HASH` cluster, `pick_endpoint(Some(xxh64(b"key-0")))` → the oracle host; `pick_endpoint(None)` → a valid host (the no-hash path); (b) a `RoundRobin` cluster ignores the key: `pick_endpoint(Some(123))` behaves identically to the cursor path (regression-equivalence — the key is inert for round-robin); (c) **MUST-HAVE — the empty-vs-absent header distinction at the EXTRACTION site (the most error-prone line in the phase; ADR-0070):** an HCM-level (or extraction-helper) test asserting a request whose `hash_policy` header is **present but empty** (`x-hash-key:` empty) yields `Some(xxh64(b""))` (NOT fallback), while a request with the header **ABSENT** yields `None` (fallback). Guard against the classic bug where `.filter(|v| !v.is_empty())` collapses empty into absent — write this test FIRST and make it fail. (d) a request with the `hash_policy` header present → the key is computed + threaded to the pick.
- [ ] **Implement** the signature change (both call sites updated — only 2 per the recon) + the HCM header-extraction-and-hash. Keep the `RoundRobin` path allocation-free (the key is `Option<u64>`, ignored on that arm). The header lookup uses the existing request-header access already in scope before the pick.
- [ ] Run `cargo test -p envoy-cluster -p envoy-http1 -p envoy-http2`. Clippy.
- [ ] **Regression check:** `cargo test --workspace --exclude differential` — all non-Docker tests green (the round-robin no-op proof across the whole tree).
- [ ] Commit (code) + PROGRESS commit.

## Task 7 — fixture 0036 + the differential harness driver [D-differential]

**Files:** Create `tests/fixtures/0036-lb-ring-hash/` (`envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`); Modify `tests/differential/src/lib.rs` (a new driver/probe that sweeps `x-hash-key` values and asserts the SAME backend marker on both proxies — reuse the phase-27 two-backend `{{HTTP1_BACKEND_1_PORT}}`/`{{_2_PORT}}` + `spawn_with_marker` machinery at `:3019-3041`); Create `tests/differential/tests/lb_ring_hash.rs` (the Docker-gated wrapper).

- [ ] **Fixture:** one H1 listener; one `RING_HASH` `STATIC` cluster with two `lb_endpoints` → backend_1 / backend_2 (distinguishable by `--body-marker`); the route carries `hash_policy: [{ header: { header_name: "x-hash-key" } }]`. `envoy.yaml` and `envoy-rust.yaml` identical (no divergence; if any, README + ADR per §7.1).
- [ ] **Driver:** for a sweep of ~16 `x-hash-key` values, send the request to BOTH proxies; assert (STRONG target, ADR-0070) the response body marker is **identical** between upstream Envoy and envoy-rust per key (cross-proxy identical selection); assert both backends are hit over the sweep (spread); assert same-key→same-backend on repeat (stability). This is a NORMAL request/response — runs + is authoritative LOCALLY (no reload trigger; this dev host observes it).
- [ ] Run `cargo test -p differential --test lb_ring_hash` LOCALLY (Docker) → green (both proxies agree per key). Clippy.
- [ ] Commit (code) + PROGRESS commit.

## Task 8 — in-process backstop + fuzz seed [D-backstop]

**Files:** Create `crates/envoy-bin/tests/lb_ring_hash.rs` (the backstop); add a `parse_bootstrap` fuzz seed (a `RING_HASH` + `hash_policy` bootstrap) under the existing fuzz corpus (no new fuzz target — the new surface is config-parse, covered by `parse_bootstrap`).

- [ ] **Backstop tests** (boot a real `envoy-bin` or drive the cluster/ring in-process): ring determinism + the oracle mapping; spread; **the no-hash-key fallback** (absent header → a valid host, the no-hash path); **the empty-header-value-is-hashed** distinction (`x-hash-key:` empty → deterministic `xxh64("")` host, NOT random); single-host ring; the `RoundRobin`-ignores-key regression; (if Task 5 implemented skip-retry) an HC/OD + RING_HASH eligibility skip. These cover what the single differential fixture can't (the fallback paths + edge cases are backstop-only, the phase-26/27 precedent).
- [ ] **Fuzz seed:** add the `RING_HASH`/`hash_policy` bootstrap YAML to the corpus at `crates/envoy-config/fuzz/corpus/parse_bootstrap/` (target `crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`); confirm `cargo +nightly fuzz run parse_bootstrap` (or the CI short-budget run) is clean on it (CI covers the run; locally just add the seed).
- [ ] Run `cargo test -p envoy-bin --test lb_ring_hash`. Clippy.
- [ ] Commit (code) + PROGRESS commit.

## Task 9 — BEHAVIOR_CONTRACT "LB selection" extension [docs]

**Files:** Modify `docs/envoy-rust/BEHAVIOR_CONTRACT.md`.

- [ ] Add an **"LB selection"** subsection: `ROUND_ROBIN` (existing) + `RING_HASH` — the selection is deterministic + byte-identical to Envoy via the ADR-0070 algorithm (xxHash64 seed 0; `"<ip:port>_<i>"` ring keys; `min_ring_size/num_hosts` replicas; sorted ring; `bisect_left` wrap); the route header `hash_policy` keys it; the no-`hash_policy`-match → random-host fallback is NOT differentially asserted (non-deterministic); the empty-header-value-is-hashed refinement; the XX_HASH-only narrowing (MURMUR_HASH_2 rejected). Cite fixture 0036 as the differential witness. **If Task 5 deferred the HC/OD + RING_HASH skip-retry compose**, record it here as a SPEC §2.2 deferred non-goal (the differential does not exercise it).
- [ ] Commit (docs) + PROGRESS commit.

## Task 10 — state-4 verification gate (§7.5) [verification]

**Skill:** `superpowers:verification-before-completion`. Run + quote into PROGRESS: `cargo fmt --all -- --check`; the standalone-crate builds; `cargo build --workspace --all-targets`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace` (incl. the Docker-gated fixture 0036 — **observable LOCALLY this phase**, plus all 35 pre-existing fixtures 0001–0035); `cargo deny check`; the `parse_bootstrap` fuzz short-budget run. **Differential anchor:** unlike phases 26/27, fixture 0036 is locally observable — but the §7.5 authoritative gate is still the Linux CI run (h2spec ≥95%, the full workspace test, fuzz). Confirm all 36 fixtures (0001–0036) green simultaneously. Then advance STATE to state-5-next.

- [ ] Quote all gate outputs into PROGRESS. Advance STATE (state-4 → state-5-next). Commit + push.

---

## Notes for the executor
- **SERIAL subagent dispatch** (`feedback_serial_subagent_dispatch`) — never parallel; they race on `main`.
- **Per-task discipline** (`project_state3_arc_skips_clippy` lesson): run `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK; `cargo fmt --all -- --check` is the state-4 gate (CI is red-at-fmt mid-phase is normal — budget the state-4 CI iteration, memory `envoy-rust-state4-ci-first-execution`).
- **The differential is LOCALLY observable** this phase (ring_hash needs no reload trigger) — fixture 0036 runs + is authoritative on this Docker-Desktop host; the only known local failure is the unrelated `admin_config_dump_server_info` virtiofs divergence (NOT a regression).
- **xxHash64 from scratch** — do NOT add a hashing crate (D-3.2). Validate against the canonical vectors BEFORE building the ring on it (a wrong hash silently breaks the differential).
- **The `_` separator + the `ip:port` address form are load-bearing** (ADR-0070) — pin the oracle mapping (Task 5(c)) as the regression guard.
- **§6.1:** single phase (ADR-0071 unfired). If a task's sub-steps balloon past ~10 (e.g. the HC/OD+RING_HASH compose proves heavy), prefer deferring that sub-scope to a recorded non-goal over splitting.

_Scope locked by ADR-0069 (pick) + ADR-0070 (the §6.2-verified ring algorithm; STRONG target). State-3 = `superpowers:subagent-driven-development` (Task 2 first; Task 1 DONE). The state-3 execution is the NEXT session per §5.1._
