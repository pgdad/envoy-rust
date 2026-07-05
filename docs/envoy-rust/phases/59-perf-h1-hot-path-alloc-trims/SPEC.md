# Phase 59 — `59-perf-h1-hot-path-alloc-trims` — SPEC

**Pick (ADR-0116):** Trim four per-request costs on the H1 downstream hot path — a global `Date`-header mutex, two access-log-only heap allocations, and a per-response wire-buffer allocation — **without changing any emitted byte**. This is a **regression-equivalence phase**, not a differential-witness leaf: it adds NO fixture and NO `Op`/`AccessLogRecord` field; its acceptance is (a) all fixtures `0001`-`0065` stay byte-identical and (b) a micro-benchmark showing same-or-better throughput on each changed path (the BEHAVIOR_CONTRACT "a phase may opt in to latency bounds" clause). Motivated by a CPU flamegraph of `envoy-bin` under load (~14k rps on a k3s node) in which kernel network I/O was ~84% of self-time and envoy-rust app code ~11.5% — these are the cheap, safe fraction of that 11.5%, taken because they are byte-neutral and independently verifiable, not because the proxy is compute-bound (it is not).

> **State-0 recon (this session) — the four costs, located live in the tree at rev `89228d0`.**
> - `crates/envoy-http1/src/date.rs:14` — `static DATE_CACHE: Mutex<Option<(u64, String)>>`. The IMF-fixdate is already cached per-second (format runs ~1 Hz), but **every response** locks this one global mutex + clones the ~29-byte `String`. Under N worker threads the lock serializes them; a micro-bench of `now_imf_fixdate()` shows aggregate throughput *falling* as workers rise (1t 36.8 → 2t 15.4 → 8t 10.3 Mops/s) — the textbook contention signature.
> - `crates/envoy-http1/src/hcm.rs:1030` — `upstream_host_for_log = Some(endpoint.to_string())`. The resolved upstream `SocketAddr` is Display-formatted into a `String` on **every proxied request**, unconditionally, though it is read **only** under the `if !config.access_log.is_empty()` guard (record build, `hcm.rs:1401`).
> - `crates/envoy-http1/src/hcm.rs:1293` — `let response_headers_for_log = outgoing.headers.clone()`. The **entire** response-header vec is cloned per request; the single consumer (`extract_upstream_service_time`, `hcm.rs:1396`) only borrows it, and `outgoing` is still alive at that point.
> - `crates/envoy-http1/src/response.rs:31` — `Http1Response::write_to` allocates a fresh `Vec<u8>` per response. On a kept-alive connection that is one allocation per request; the read buffer beside it (`hcm.rs:651`) is already reused, the write buffer is not.

## §1 — Goal & differential surface

**Goal.** Remove the four costs above with **zero change to any wire byte or access-log byte**, on both the proxied and synth/error H1 paths.

**Differential surface at phase end: UNCHANGED.** No new fixture; no fixture output changes. `date:` stays on the BEHAVIOR_CONTRACT allow-list and is emitted byte-for-byte as before (every thread produces the identical per-second string). `%UPSTREAM_HOST%` and `%UPSTREAM_SERVICE_TIME%` render identically **when access logging is configured** (the only case in which they are observable); the trims change only the un-observable no-logging path. h2spec unchanged (no H2 or codec change).

**Perf surface (opt-in latency/throughput bound, this phase only).** A self-contained `--release` micro-bench (`crates/envoy-http1/tests/perf_bench.rs`, `#[ignore]`d so normal `cargo test` never pays for it) measuring: (i) `now_imf_fixdate()` single-thread ns/call + aggregate Mops/s at 1/2/4/8/16 threads; (ii) `write_to` (fresh alloc) vs `write_to_buf` (reused buffer) ns/response. **Acceptance: same-or-better on every metric vs the pre-change baseline.** Observed this session: Date contended 4.9×–14× at 2–8 threads and same-or-faster single-thread; response write 1.30×.

## §2 — Scope (minimum-viable, ADR-0116)

**A cache-mechanism swap + two logging-alloc elisions + a buffer-reuse method + a self-contained micro-bench + a byte-equivalence unit test — NO new fixture / `Op` / `AccessLogRecord` field / runtime crate / dependency / `ConfigError` variant / fuzz target:**

- **§A — `date.rs`: global `Mutex` → per-worker `thread_local!` cache.** Replace `static DATE_CACHE: Mutex<Option<(u64,String)>>` with `thread_local! { static DATE_CACHE: RefCell<Option<(u64,String)>> }` (`const`-initialized). `now_imf_fixdate()` keeps its `-> String` signature (zero caller changes) and its per-second refresh semantics; it now reads/writes the thread-local via `with_borrow_mut`, so concurrent workers never contend. The hand-rolled `format_imf_fixdate` is **untouched** — no `httpdate` dependency (parent-04 SPEC §3 D1's hand-rolled lock is preserved). Every thread yields the identical string for a given second → the emitted `date:` header is byte-identical.
- **§B — `hcm.rs:1030`: gate the upstream-host format on logging.** `if !config.access_log.is_empty() { upstream_host_for_log = Some(endpoint.to_string()); }`. `endpoint` is `SocketAddr` (Copy), used only here; `upstream_host_for_log` is read only under the same guard (`:1401`) — so this is byte-identical when logging is on and skips the alloc when off.
- **§C — `hcm.rs:1293`: drop the response-header clone; borrow instead.** Delete the per-request `outgoing.headers.clone()`; change the one consumer to `extract_upstream_service_time(&outgoing.headers)` (`:1396`). `outgoing` is borrowed by the wire-write at `:1298` and stays owned through the access-log block, so the borrow is valid; the whole-vec clone is removed on **every** request, logging on or off.
- **§D — `response.rs`: add `Http1Response::write_to_buf(resp, w, buf: &mut Vec<u8>)`.** It `clear()`s the caller buffer (retaining capacity), fills it exactly as `write_to` did, and writes. `write_to` becomes a thin wrapper (`let mut buf = Vec::new(); write_to_buf(...)`) so all existing callers/tests are unchanged. `serve_connection` (`hcm.rs:651`) declares one `write_buf: Vec<u8>` beside the read buffer and passes it at the sole wire-write site (`:1298`) → one wire-buffer allocation per connection, not per response.
- **§E — micro-bench + byte-equivalence test.** `tests/perf_bench.rs` (the `#[ignore]`d perf harness, run with `--test-threads=1`) + a `response.rs` unit test asserting `write_to_buf` is byte-identical to `write_to`, **including across reuse** (a large response then a smaller one on the same buffer, proving no bleed-through). `bytes` added to **dev-**dependencies only (already in the tree; runtime deps unchanged).

**Load-bearing additivity invariant:** all `0001`-`0065` stay byte-identical. §A/§C/§D change the *mechanism* by which identical bytes are produced (proven by the §E byte-equivalence test + the existing `date.rs`/`response.rs` wire-format tests). §B changes behavior **only** on the `config.access_log.is_empty()` path, which no differential fixture exercises for `%UPSTREAM_HOST%` (that operator is only asserted by logging-on fixtures, e.g. `0052`). No fixture drives a no-logging proxied request that also inspects upstream-host output, so §B is unobservable to the suite.

## §3 — PLAN-VERIFY items (state-2 §6.2)

1. **Re-grep `hcm.rs:1030`/`:1293`/`:1298`/`:1396`/`:651` line numbers** against the tree at PLAN-write time (drift check, per every recent phase's §3 discipline).
2. **Confirm `upstream_host_for_log` (`:835`→`:1401`) and `response_headers_for_log` (`:1293`→`:1396`) have no consumer outside the `!access_log.is_empty()` block** — re-verify §B/§C are unobservable when logging is off.
3. **Confirm `outgoing` is not moved between `:1298` and `:1396`** so the §C borrow compiles (this session compiled it clean; PLAN-write re-confirms against any drift).
4. **Confirm no existing fixture drives a no-logging proxied request whose `%UPSTREAM_HOST%` (or `%UPSTREAM_SERVICE_TIME%`) would newly differ** — re-verify the §2 additivity invariant before scoping PLAN's task list as safe.
5. **Confirm `thread_local!` + `with_borrow_mut` compile on the pinned toolchain** (1.95.0; `with_borrow_mut` stable since 1.73 — verified this session).
6. **Decide the §6.1 split** — projected NOT to fire (~5 files, ~250 LoC); PLAN-write makes the final call.

## §4 — Reuse map (what exists; do not rebuild)

- The per-second Date caching semantics, the `format_imf_fixdate` civil-date algorithm, and the `date.rs` byte-exact tests — **reused verbatim**; §A swaps only the cache container.
- The `!config.access_log.is_empty()` guard (`hcm.rs:1323`) and the `AccessLogRecord` shape — **reused**; §B/§C only move allocations relative to that existing guard.
- `Http1Response::write_to`'s serialization body — **reused**; §D factors it into `write_to_buf` and leaves `write_to` as a wrapper.
- The read-buffer-reuse pattern already present in `serve_connection` (`hcm.rs:651`, the `BytesMut` reused across keep-alive requests) — §D mirrors it for the write buffer.

## §5 — Behavioral contract notes

- **`date:`** — allow-listed (value not diffed), but emitted byte-identical regardless. No BEHAVIOR_CONTRACT edit.
- **`%UPSTREAM_HOST%` / `%UPSTREAM_SERVICE_TIME%`** — identical whenever observable (logging on). No contract edit.
- **Timing** — the contract's default is "not compared"; this phase **opts into** a throughput/latency bound for its own micro-bench only (§1 perf surface), asserting same-or-better. This does not add a standing timing gate to the differential suite.
- `#![forbid(unsafe_code)]` holds (thread-local + `RefCell` are safe; no raw fd, no `unsafe`).

## §6 — Process

- **§6.1 split — projected NOT to fire.** ~5 files (`date.rs`, `hcm.rs`, `response.rs`, `Cargo.toml` dev-dep, new `tests/perf_bench.rs`), ~250 LoC, no new harness/fixture/struct. Well under the ~25-task/~1500-LoC gate.
- **§6.2 reconciliation** — reserved (shared slot with the split per the lapsed-reservation convention) in case §3's PLAN-write re-verification overturns a §A–§E fact. Not expected to fire (all four sites + the byte-equivalence are code-verified this session).
- **Carry-forwards:** OPENS none; CONSUMES none. Optionally NOTE (non-blocking) the un-taken larger trims from the same flamegraph — the per-request `filter_pipeline.clone()` (`hcm.rs:722`) and per-attempt `req.headers.clone()` (`hcm.rs:447`) — as a future `perf` carry-forward; deliberately OUT of scope here (bigger blast radius, more tests in play). No existing carry-forward blocks this phase.
- Pick + §A–§E ground-truth locked by **ADR-0116** (the next-available number; upstream fired ADR-0115 at phase 58).

## §7 — Acceptance (§7.5, re-run at state-4)

(a) all `0001`-`0065` fixtures green **simultaneously and byte-identical** (the §2 additivity invariant — the whole point; no fixture output moves) + (b) the §E byte-equivalence unit test green (`write_to_buf` == `write_to`, incl. across reuse) + (c) the §1 micro-bench shows **same-or-better** on every metric vs the pre-change baseline (Date single + 1/2/4/8/16-thread; response write) + (d) h2spec unchanged (no H2/codec change) + (e) no new fuzz target (no new parser/format — §H-equivalent) + (f) build/clippy/fmt/test/deny clean; `#![forbid(unsafe_code)]` holds; NO new runtime crate/dependency/`Op`/`AccessLogRecord` field/`ConfigError` variant (dev-only `bytes`) + (g) `REVIEW.md` approved. **If §3's PLAN-write re-grep finds a fixture would newly diverge, or the §C borrow does not compile against drift**, a §6.2 reconciliation ADR fires and §B/§C are re-scoped.

_Scope locked by ADR-0116. The state-2 PLAN-write (`superpowers:writing-plans`) re-confirms §3's PLAN-VERIFY items (line-number drift, the two access-log-guard consumer checks, the `outgoing` liveness for §C, the toolchain check, the §6.1 split decision) and authors `PLAN.md`. The state-3 implementation is the session after._
