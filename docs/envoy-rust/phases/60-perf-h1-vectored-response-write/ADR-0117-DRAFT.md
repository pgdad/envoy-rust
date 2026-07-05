# DRAFT ADR for phase 60 — to be slotted into `docs/envoy-rust/DECISIONS.md`

> This is a standalone draft. `DECISIONS.md` is append-only and ordered
> newest-first; the maintainer places this block at the canonical position and
> confirms the number. **ADR-0117** is the next-available number: the ledger head
> on `main` is ADR-0115 (phase 58); phase 59 (`59-perf-h1-hot-path-alloc-trims`)
> drafts ADR-0116, and this phase stacks on it, so 0117 is the successor. If a
> sibling session fires 0116/0117 first, renumber to the then-next-available.

---

## ADR-0117: Phase-60 pick + scope — **eliminate the per-response body memcpy on the H1 downstream write path with a vectored (`writev`) write of head+body, threshold-gated so no body size regresses, with ZERO emitted-byte change — a regression-equivalence phase, NO new fixture**

- Date: 2026-07-05
- Status: accepted
- Context: The same CPU flamegraph that motivated phase 59 (`envoy-bin` under
  ~14k rps on a k3s node; kernel network I/O ~84% of self-time, app code ~11.5%,
  libc ~4.4%) shows a per-response body copy on the H1 downstream write path.
  `Http1Response::write_to_buf` (as phase 59 shipped it) serializes the status
  line, headers, blank line **and body** into one reused `Vec<u8>` and issues one
  `write_all`; `buf.extend_from_slice(&resp.body)` is a memcpy that scales with
  body size. The production caller (`hcm.rs:1312`) writes to a concrete
  `TcpStream`, which advertises `is_write_vectored() == true` — so the body copy
  can be removed by writing `[head, body]` as two `IoSlice`s in a single `writev`
  (the same one syscall; the body is no longer staged through the userspace
  buffer). This is byte-neutral (identical octets, identical order) and, like
  phase 59, NOT a differential-parity gap — it is a regression-equivalence
  maintenance phase, explicitly NOT a claim that compute is the bottleneck (it is
  not; the proxy is I/O-bound). The one wrinkle: `writev` has a small fixed cost
  that, for a *small* body, exceeds the memcpy it saves — so a naive
  "always vectored" change would regress tiny responses (measured ≈3–5% at
  ≤256 B). The pick therefore gates the vectored path on a body-size threshold.
- Options considered:
  - **Do nothing** — the app is only 11.5% of self-time; the body copy only
    matters for large payloads. Rejected: the copy is trivially removable on the
    hot path, byte-neutral, and unbounded in the body size (a 1 MiB response
    copies 1 MiB per request); the threshold makes it free for small bodies too.
  - **Always vectored (no threshold)** — simplest. Rejected: measured ≈3–5%
    *regression* on sub-256 B responses (the `writev` fixed cost > the tiny
    memcpy), which violates the same-or-better bar. The threshold is the
    load-bearing choice that makes the change strictly dominant.
  - **`sendfile`/`splice` (zero-copy to socket)** — kernel-side zero copy.
    Rejected: the body is an in-memory `Bytes` (a proxied/synth response), not a
    file fd; `sendfile` does not apply, and `writev` already removes the only
    userspace copy under our control (the kernel user→socket copy is unchanged and
    identical for `write` and `writev`).
  - **Threshold value.** Chose **1024 B**: the micro-bench crossover sits a few
    hundred bytes below it, so 1 KiB is a conservative round cutoff where the win
    is unambiguous (≈1.15× and rising) and clear of run-to-run noise. Below it the
    code path is byte- and cost-identical to phase 59. (A slower PLAN-write host
    may raise it; never lower.)
  - **Reuse a body-region buffer instead** — no. That reintroduces the copy this
    phase removes; the whole point is to *not* stage the body.
- Decision: Land §A–§E of the phase-60 SPEC in `crates/envoy-http1/` only: (§A)
  `write_to_buf` builds the head into `buf` and, when `resp.body.len() >=
  VECTORED_BODY_THRESHOLD`, emits `[head, body]` via a vectored `write_all_vectored`
  helper; below the threshold (and empty body) it coalesces into `buf` and writes
  once, identical to phase 59; (§B) `const VECTORED_BODY_THRESHOLD: usize = 1024`;
  (§C) `write_all_vectored(w, head, body)` draining across partial writes via
  `std::io::IoSlice::advance_slices` with a `WriteZero` guard, called only with two
  non-empty slices; (§D) a `bench_response_write_coalesced_vs_vectored` body-size
  sweep added to phase 59's `#[ignore]`d `perf_bench.rs`; (§E) three `response.rs`
  byte-equivalence unit tests (vectored one-writev + body-absent-from-buf,
  partial-write drain across the head/body boundary, sub-threshold coalesced
  single write) plus the retained empty-body and wire-format tests. No new fixture,
  `Op`, `AccessLogRecord` field, runtime crate, dependency, `ConfigError` variant,
  or fuzz target.
- Rationale: The change is byte-neutral, so acceptance is regression-equivalence
  (all `0001`-`0065` byte-identical) plus a same-or-better micro-bench, NOT a new
  differential witness. `writev` removes the only userspace copy on the write path
  under our control while keeping the syscall count at one per response; the
  threshold guarantees the small-body path is untouched, so the change is strictly
  same-or-better at every size. `advance_slices` + `IoSlice` are safe, so
  `#![forbid(unsafe_code)]` holds with no `unsafe`.
- Consequences: `crates/envoy-http1/` only (`response.rs`, `tests/perf_bench.rs`);
  no H2/codec/`envoy-config` change → h2spec + `parse_bootstrap` fuzz unaffected.
  Local evidence this session: all workspace lib/bin tests green (1635, zero
  failures; envoy-http1 lib 155 incl. the three new vectored tests); fmt/clippy
  clean; micro-bench parity within noise below 1 KiB, 1.13–1.9× at 1–4 KiB, ≈13×
  at 64 KiB, ≈250×+ at 1 MiB (userspace serialization). The Docker-gated
  differential suite (`0001`-`0065`) and h2spec are expected green **by
  construction** (no emitted byte changes) and must be confirmed by CI (state-4
  §7.5 gate). OPENS an optional non-blocking `perf` carry-forward (the
  `filter_pipeline.clone()` + per-attempt `req.headers.clone()` larger trims, also
  noted by phase 59); CONSUMES none. This phase **stacks on phase 59 / ADR-0116**
  (it modifies the `write_to_buf` that 59 introduced); if 59 merges first, 60
  rebases onto it cleanly. DECISIONS.md ledger head after this ADR: **ADR-0117**
  (assuming 0116 lands first; next-available **ADR-0118**). ADR-0014 in force;
  parent-04 §3 D1 (hand-rolled date) PRESERVED. The state-2 PLAN-write is the next
  session.
