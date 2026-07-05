# Phase 60 — `60-perf-h1-vectored-response-write` — SPEC

**Pick (ADR-0117):** Eliminate the per-response **body memcpy** on the HTTP/1.1
downstream write path by emitting the response head and body with a single
**vectored** write (`writev`) instead of copying the body into the wire buffer —
**without changing any emitted byte**, and **without regressing any body size**
(a size threshold keeps small bodies on the proven coalesced single-write path).
This is a **regression-equivalence phase**, not a differential-witness leaf: it
adds NO fixture and NO `Op`/`AccessLogRecord` field; its acceptance is (a) all
fixtures `0001`-`0065` stay byte-identical and (b) a micro-benchmark showing
same-or-better throughput on **every** body size (the BEHAVIOR_CONTRACT "a phase
may opt in to latency bounds" clause). Motivated by the same CPU flamegraph of
`envoy-bin` under load (~14k rps on a k3s node) as phase 59 — kernel network I/O
~84% of self-time, app code ~11.5%, of which a fraction is the libc memcpy that
copies each response body into the write buffer.

> **Relationship to phase 59.** This phase **builds on** phase 59
> (`59-perf-h1-hot-path-alloc-trims`, ADR-0116), which introduced
> `Http1Response::write_to_buf` (the reused per-connection wire buffer). Phase 60
> changes *how that buffer is flushed*: below the threshold it coalesces exactly
> as 59 shipped; at/above it, the body is no longer copied into the buffer at
> all. If 59 is not yet merged, 60 stacks on it; if 59 merges first, 60 rebases
> onto it cleanly (it only touches `response.rs` + the perf bench 59 added).

> **State-0 recon (this session) — the cost, located live in the tree.**
> - `crates/envoy-http1/src/response.rs` — `write_to_buf` builds the status line,
>   headers, blank line **and body** into one `Vec<u8>` (`buf.extend_from_slice(&resp.body)`)
>   and issues one `write_all`. The body copy is a per-response memcpy that scales
>   with body size. The sole production caller is `hcm.rs:1312`
>   (`Http1Response::write_to_buf(&outgoing, &mut downstream, &mut write_buf)`),
>   where `downstream` is a concrete `tokio::net::TcpStream` — which advertises
>   `is_write_vectored() == true` (real `writev`).
> - No vectored I/O exists anywhere in the crate (`write_vectored`/`IoSlice` = 0
>   hits before this phase). `TCP_NODELAY` is already set on accepted sockets
>   (`envoy-listener`), so a 2-slice `writev` does not stall on delayed-ACK.

## §1 — Goal & differential surface

**Goal.** Remove the response-body memcpy on the H1 downstream write path with
**zero change to any wire byte**, on both the proxied and synth/error paths, and
with **no throughput regression at any body size**.

**Differential surface at phase end: UNCHANGED.** No new fixture; no fixture
output changes. The bytes a client receives are identical — `writev` of
`[head, body]` puts exactly the same octets on the wire, in the same order, as
the previous coalesced `write(head ++ body)`. h2spec unchanged (no H2 or codec
change).

**Perf surface (opt-in throughput bound, this phase only).** The phase-59
`--release` micro-bench (`crates/envoy-http1/tests/perf_bench.rs`, `#[ignore]`d)
gains a body-size sweep comparing the pre-change coalesced writer against the
shipped adaptive writer across 13 B … 1 MiB, using a discarding sink that
advertises vectored support (isolating the userspace serialization cost — the
memcpy — which is exactly what `writev` removes; the syscall count is unchanged,
one write per response either way). **Acceptance: same-or-better on every body
size vs. the coalesced baseline.** Observed this session: parity within
run-to-run noise below 1 KiB; 1.13–1.9× at 1–4 KiB; ≈13× at 64 KiB; ≈250×+ at
1 MiB (userspace serialization time).

## §2 — Scope (minimum-viable, ADR-0117)

**A single-method change in `response.rs` + a threshold constant + a vectored
drain helper + a body-size micro-bench + three byte-equivalence unit tests — NO
new fixture / `Op` / `AccessLogRecord` field / runtime crate / dependency /
`ConfigError` variant / fuzz target:**

- **§A — `write_to_buf`: build the head only; emit head+body vectored above a
  threshold.** The status line + headers + blank separator line go into `buf` as
  before, but the body is **not** appended. If `resp.body.len() >=
  VECTORED_BODY_THRESHOLD`, the head (`buf`) and the body (`&resp.body`) are
  written with `write_all_vectored` as two `IoSlice`s. Below the threshold (and
  for an empty body) the body is coalesced into `buf` and written once —
  byte-for-byte and cost-for-cost identical to the phase-59 writer.
- **§B — `VECTORED_BODY_THRESHOLD = 1024`.** `writev` has a small fixed cost
  (building the iovec array + the vectored dispatch) that, for a small body,
  exceeds the memcpy it saves; the micro-bench locates the crossover a few
  hundred bytes below 1 KiB. 1 KiB is a conservative round threshold at which the
  vectored win is unambiguous (≈1.15× and rising) and clear of measurement noise,
  so the change **never regresses a small response**. This is the load-bearing
  decision that turns "neutral-to-positive" into "strictly same-or-better".
- **§C — `write_all_vectored(w, head, body)` helper.** Writes both slices,
  draining across partial writes via `std::io::IoSlice::advance_slices` (stable
  since 1.60; toolchain 1.95) until both are consumed; a `WriteZero` guard
  prevents an infinite loop on a stuck sink. Called only with two non-empty
  slices (the empty-body case takes the coalesced path), so no zero-length slice
  reaches the syscall. On a `TcpStream` this is one `writev`; on a sink that does
  not support vectored I/O the loop falls back to sequential writes — identical
  bytes either way.
- **§D — micro-bench sweep.** `bench_response_write_coalesced_vs_vectored` (added
  to the phase-59 `perf_bench.rs`) sweeps body sizes with a `NullVecSink`
  (`is_write_vectored() == true`, discards), printing old-coalesced vs.
  new-adaptive ns/response and the speedup per size.
- **§E — byte-equivalence unit tests.** Three new `response.rs` tests over a
  `MockSink` that records plain vs. vectored calls and reassembles the bytes:
  (i) a ≥1 KiB body takes exactly one `writev`, the body is absent from `buf`,
  and the reassembled bytes equal the coalesced reference; (ii) with a small
  per-call cap the drain loop crosses the head/body boundary and still
  reassembles exactly; (iii) a sub-threshold body takes one plain write (no
  `writev`) and is byte-identical. The empty-body test and the existing
  `write_to`/`write_to_buf` wire-format tests are retained.

**Load-bearing byte invariant:** all `0001`-`0065` stay byte-identical. §A/§C
change only the *mechanism* by which the identical head+body octets reach the
wire (proven by the §E reassembly tests + the existing `response.rs` wire-format
tests). The threshold (§B) is a pure send-shape choice — it changes neither the
bytes nor, below 1 KiB, the code path.

## §3 — PLAN-VERIFY items (state-2 §6.2)

1. **Re-grep the production caller** (`hcm.rs` `write_to_buf(&outgoing, &mut
   downstream, …)`) against the tree at PLAN-write time and confirm `downstream`
   is still a concrete `TcpStream` (so `is_write_vectored()` is true in prod).
2. **Confirm `tokio::io::AsyncWriteExt::write_vectored` and
   `std::io::IoSlice::advance_slices` are available** on the pinned toolchain
   (1.95; both verified this session — `advance_slices` stable since 1.60).
3. **Confirm the empty-body and sub-threshold paths remain the coalesced
   single-write** (no zero-length `IoSlice` can reach a `writev`).
4. **Re-confirm the crossover** on the PLAN-write host (the threshold must sit at
   or above the measured crossover so no size regresses); adjust
   `VECTORED_BODY_THRESHOLD` only upward if a slower host moves the crossover.
5. **Decide the §6.1 split** — projected NOT to fire (~2 files, ~250 LoC).

## §4 — Reuse map (what exists; do not rebuild)

- `Http1Response::write_to` / `write_to_buf` structure and the reused
  per-connection buffer (phase 59) — **reused**; §A changes only the flush.
- The `perf_bench.rs` harness, `NullVecSink` shape, and `response_with_body`
  helper (phase 59) — **reused/extended**; §D adds one bench fn.
- `Http1Error: From<std::io::Error>` (`error.rs`) — **reused** for the `WriteZero`
  guard and the `write_vectored` error path.

## §5 — Behavioral contract notes

- **Wire bytes** — identical; `writev` is a send-shape change only. No
  BEHAVIOR_CONTRACT edit.
- **Timing** — the contract's default is "not compared"; this phase **opts into**
  a throughput bound for its own micro-bench only (§1), asserting same-or-better
  on every body size. No standing timing gate is added to the differential suite.
- `#![forbid(unsafe_code)]` holds (`IoSlice` + `advance_slices` are safe; no raw
  fd, no `unsafe`).

## §6 — Process

- **§6.1 split — projected NOT to fire.** ~2 files (`response.rs`, `perf_bench.rs`),
  ~250 LoC, no new harness/fixture/struct. Well under the gate.
- **§6.2 reconciliation** — reserved in case §3's PLAN-write re-verification
  overturns a §A–§E fact (e.g. the crossover moves on the PLAN host). Not expected
  to fire.
- **Carry-forwards:** OPENS none; CONSUMES none. Optionally NOTE (non-blocking)
  the remaining un-taken trims from the same flamegraph — the per-request
  `filter_pipeline.clone()` and per-attempt `req.headers.clone()` (also noted by
  phase 59) — as a future `perf` carry-forward; deliberately OUT of scope here.
- Pick + §A–§E ground-truth locked by **ADR-0117** (the next-available number;
  phase 59 drafted ADR-0116).

## §7 — Acceptance (§7.5, re-run at state-4)

(a) all `0001`-`0065` fixtures green **simultaneously and byte-identical** (the §2
byte invariant) + (b) the §E byte-equivalence tests green (vectored, partial-write
drain, sub-threshold coalesce, empty body) + (c) the §1 micro-bench shows
**same-or-better** on every body size vs. the coalesced baseline + (d) h2spec
unchanged (no H2/codec change) + (e) no new fuzz target (no new parser/format) +
(f) build/clippy/fmt/test/deny clean; `#![forbid(unsafe_code)]` holds; NO new
runtime crate/dependency/`Op`/`AccessLogRecord` field/`ConfigError` variant +
(g) `REVIEW.md` approved. **If §3's PLAN-write re-verification finds the crossover
above the threshold on the PLAN host, or a body size regresses**, a §6.2
reconciliation ADR fires and `VECTORED_BODY_THRESHOLD` is raised.

_Scope locked by ADR-0117. The state-2 PLAN-write re-confirms §3's PLAN-VERIFY
items (caller/stream type, toolchain API availability, empty/sub-threshold path,
the crossover, the §6.1 split) and authors `PLAN.md`. The state-3 implementation
is the session after._
