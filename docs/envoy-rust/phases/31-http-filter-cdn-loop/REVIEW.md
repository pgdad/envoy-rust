# Phase 31 — `31-http-filter-cdn-loop` — REVIEW

> **Lifecycle state 5 (code review).** Holistic phase-level review by a fresh
> `superpowers:code-reviewer` subagent (per `superpowers:requesting-code-review`), scoped to the
> whole phase-31 implementation (`e546c04..2a5fe48`). Scope locked by **ADR-0076**; §6.2 wire facts
> locked by **ADR-0077**. Each of the 7 tasks was ALSO two-stage-reviewed during state-3 (spec then
> code-quality, fresh subagents) → all APPROVED (0C/0I); this is the cross-cutting feature-level pass.
> Read with zero prior context (D-3.4).

## Verdict

**APPROVED — 0 Critical / 0 Important / 3 Minor.** Ready to merge (already on `main`). The §7.5 gate
is GREEN on the AUTHORITATIVE Linux CI run **`27915239054` @ `a2051b2`**. `#![forbid(unsafe_code)]`
holds; clippy/fmt/test clean.

## Scope reviewed

`e546c04..2a5fe48` — production parser+filter (`crates/envoy-filter/src/cdn_loop.rs`, `instance.rs`,
`lib.rs`), config (`crates/envoy-config/src/bootstrap.rs`, `lib.rs`), fixture `0039-http-filter-cdn-loop`
+ the differential test, the `cdn_loop_parse` fuzz target + its CI wiring, and the
BEHAVIOR_CONTRACT/PROGRESS bookkeeping. Targeted spot-checks run by the reviewer: `cargo test -p
envoy-filter cdn_loop` (55 pass), `cargo test -p envoy-config cdn_loop` (9 pass), `cargo clippy -p
envoy-filter --all-targets` (clean).

## Strengths

- **Spec/ADR fidelity is exact.** Every §6.2-LOCKED fact (ADR-0077) is reproduced verbatim: 502 body
  `The server has detected a loop between CDNs.` (44B, asserted `len()==44`), 400 body
  `Invalid CDN-Loop header in request.` (35B, asserted), comma-only append, empties preserved on raw
  bytes, case-sensitive parameter-ignoring match, multi-header coalescing, all-fatal config, `@type =
  ...cdn_loop.v3.CdnLoopConfig`, no stat. The `count > max_allowed_occurrences` boundary (strict `>`,
  not `>=`) is correct and pinned at max=0/1/2.
- **The parser is a clean single-pass, allocation-light, panic-free byte function.** All slicing goes
  through `split_first`/`split_last`/bounded loops; `consume_quoted_string` handles `quoted-pair` and
  the truncated-escape edge. No backtracking → no quadratic/DoS surface.
- **Clean pipeline integration.** `CdnLoopFilter` is the 9th `HttpFilterInstance` variant, wired
  through `build`/`decode_headers`/`encode_headers`/`apply_route_config` exactly like its siblings;
  encode-side and route-config arms are correctly inert. Reject responses carry empty header vecs and
  rely on the shared H1/H2 synth decorators (csrf/buffer precedent).
- **Append handles all multiplicities.** `retain_mut` coalesces N existing `cdn-loop` entries into
  one, preserves the first entry's key casing, drops redundant entries, operates on raw joined bytes
  so empty entries survive.
- **The no-op witness genuinely protects the 38-fixtures-green invariant** — builds a LIVE
  header_mutation+router pipeline (asserts `x-witness` was added, proving the chain ran) and asserts a
  carried `cdn-loop` value (incl. would-be-self-loop + malformed) survives byte-identical with no
  400/502. Empirically regression-proven (prepending cdn_loop makes it FAIL).
- **The fuzz target is correctly wired** — `cdn_loop_parse.rs` splits on `\n` to exercise the
  multi-header `&[&[u8]]` surface; `fuzz/Cargo.toml` has the `[workspace]` isolation + root `exclude`;
  CI runs it 30s `+nightly` alongside the existing two targets.

## Cross-cutting concerns scrutinized (all CLEARED)

1. **tchar-predicate duplication** (envoy-filter `is_tchar` vs envoy-config `is_cdn_id_tchar`) —
   ACCEPTABLE: byte-for-byte identical, both trace to RFC 7230 §3.2.6; the duplication is documented
   and avoids inverting the crate dependency direction for a 15-byte `const fn`.
2. **`String::from_utf8_lossy` append bridge** (`cdn_loop.rs`) — NO correctness risk: both H1 and H2
   populate `FilterRequest.headers` as already-valid-UTF-8 `String`s → the round-trip is
   guaranteed-lossless; the lossy variant is defensive-only.
3. **No-op witness** — genuine (see Strengths).
4. **Egress header-NAME casing not differentially pinned** — ACCEPTABLE documented limitation: the
   `http1-echo-server` lowercases reflected names, so fixture 0039 proves the appended VALUE
   byte-shape but not the name casing; recorded in BEHAVIOR_CONTRACT + the fixture README; the
   in-process `append_preserves_first_existing_key_casing` test covers it. RFC 7230 header names are
   case-insensitive on the wire → not an equivalence risk.
5. **Parser DoS/panic surface from untrusted headers** — NONE: single-pass linear scan, no recursion,
   output `Vec` bounded by input length; panic-freedom verified exhaustively to len 4 + the fuzz target.
6. **Multi-header coalescing assumption** — VERIFIED: the H1 codec maps each header line 1:1 into a
   separate `Vec` entry (no pre-coalescing), so the filter's coalesce-before-count is correct + necessary.

## Issues

### Critical
None.

### Important
None.

### Minor

- **M-1 (was T3-CQ-1) — stale module doc-comment. RESOLVED in this state-5 review.** `cdn_loop.rs:1-2`
  opened "RFC 8586 `CDN-Loop` header **parser**" but the module now also houses `CdnLoopFilter`. Fixed
  to "parser + the `envoy.filters.http.cdn_loop` runtime filter" (commit landed with this REVIEW;
  doc-comment only, clippy/fmt/test re-confirmed clean).
- **M-2 (was T1) — `count_cdn_id` empty-needle contract** — `count_cdn_id(b"", parsed)` would match
  every empty list entry. Unreachable in production (config validation guarantees a non-empty token
  `cdn_id`), but the public doc doesn't state the non-empty-needle precondition. **CARRY-FORWARD**
  (cosmetic doc note; no behavior risk).
- **M-3 (was T3-CQ-2 / T3-CQ-3 / T3-CQ-4 / T2 / T4 / T5 / T6) — the recorded cosmetic minors** —
  `retain_mut` micro-clone on the cold append path; `encode_headers` doc lacks an ADR anchor; the
  `split_on_comma` one-line wrapper; parser doc-notes; fixture/test-helper prose style.
  **CARRY-FORWARD** — all non-functional polish; none touches Envoy-equivalence; can ride a future
  cdn_loop touch.

## Recommendations / disposition

- **Fixed now (this review):** M-1 (the actively-inaccurate module doc-comment) — a 1-line doc fix, no
  behavior change, re-verified clippy/fmt/test clean.
- **Carry-forward (legitimately optional, none an Envoy-equivalence divergence):** M-2, M-3. These join
  the existing non-cdn_loop carry-forwards (the empty-`metadata_match` doc-comment; M29-1/M29-2 + M30-1
  the `Http1HashSweep` driver wording / `extract_marker`; M30-2 `lb_policy` serde-default).

## Assessment

**Ready to merge: YES (already on `main`; APPROVED).** The phase faithfully and completely implements
the RFC 8586 CDN-Loop filter against the §6.2-LOCKED facts. The parser↔filter↔config triad is correct;
the duplications and the `from_utf8_lossy` bridge are justified and safe; the differential fixture is a
genuine byte-exact append proof; the no-op witness protects the regression invariant; the fuzz target
is correctly wired. **0 Critical / 0 Important / 3 Minor** (1 fixed now, 2 carry-forward). The §7.5
gate is GREEN on authoritative Linux CI `27915239054`. The fifteenth consecutive clean state-5 (after
17–30). Per the lifecycle: **state-5 APPROVED → state-6 close-out next** (flip ROADMAP row `31` →
`done`, advance STATE, relocate the phase-31 Notes subsections per ADR-0035).
