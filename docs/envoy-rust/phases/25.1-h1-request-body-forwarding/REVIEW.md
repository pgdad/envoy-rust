# Phase 25.1 (`25.1-h1-request-body-forwarding`) — REVIEW

> State-5 code review (`superpowers:requesting-code-review`) of the `25.1`
> state-3 implementation. Artifact of record; context-isolated (D-3.4) — a
> stranger with zero prior context can read it standalone.

- **Phase:** `25.1` — H1 request-body forwarding (Content-Length-delimited), Part A (foundation slice) of parent phase `25` (`envoy.filters.http.buffer`), per the ADR-0064 split.
- **Review subject (git range):** `7257bc619` (parent of Task-1 commit `20ce64010`) … `0c8b66512` (end of state-3 implementation). Production change isolated to ONE file — `crates/envoy-http1/src/hcm.rs` (+379/−30) — plus the `BEHAVIOR_CONTRACT.md` "Request body forwarding (HTTP/1.1)" note (+15) and the phase `PROGRESS.md` log (+24).
- **Requirements reviewed against:** `SPEC.md` (§3 D1 + §3.3 invariants + §4 non-goals + §5 architectural invariants) and `PLAN.md` (Tasks 1–3).
- **Method:** two independent read-only review subagents — (1) correctness + buffer-advance arithmetic + regression-equivalence + per-attempt replay + scope fidelity; (2) test-validity + flakiness + code quality — synthesized and adjudicated here.
- **Verdict: APPROVED.** No Critical and no blocking-Important findings. Minor-only. Per `BOOTSTRAP_PROMPT.md` §5.2 the phase advances to the state-6 deterministic close-out (NOT a state-3 re-entry).

---

## What was implemented (the change under review)

The H1 router previously forwarded an always-empty body upstream (`out_req.body = Some(Bytes::new())`) and drained-and-discarded the downstream Content-Length body AFTER the response was built — so envoy-rust could not proxy an H1 POST/PUT body at all (a gap since phase 04.3). `25.1`:

1. **Relocated the Content-Length body read to BEFORE the filter pipeline** (`hcm.rs:639-667`): one head advance `buf.advance(consumed)`, then — for `body_len > 0` — accumulate `from_buf = buf.len().min(body_len)` in-buffer bytes plus the socket tail (`remaining = body_len - from_buf`, read in capped 4 KB chunks) into a `BytesMut`, `freeze()` → `req.body = Some(...)`; `body_len == 0` → `Bytes::new()` (head-advance-only no-op).
2. **Forwarded the body per attempt** (`hcm.rs:359`): `out_req.body = req.body.clone()`, replacing `Some(Bytes::new())` — replay-safe across retries (the retry loop re-invokes `run_attempt(&req, …)`; ADR-0044 precedent).
3. **Removed the post-response discard-drain.**
4. **Added 5 tests + 3 test-only helpers** (test-module-only): `h1_forwards_request_body_upstream`, `h1_chunked_request_still_501_after_body_forwarding`, `h1_bodyless_get_unchanged_after_body_forwarding`, `h1_keep_alive_two_bodied_posts_do_not_bleed`, `h1_retried_post_replays_body`; helpers `spawn_recording_upstream`, `drive_keep_alive`, `spawn_fail_then_ok_recording_upstream`.
5. **Added the BEHAVIOR_CONTRACT note** documenting H1 Content-Length body forwarding (chunked/streaming remains a non-goal; H2 unchanged).

---

## Strengths

- **Buffer-advance arithmetic is byte-for-byte equivalent to the removed drain** (`hcm.rs:639-667` vs the deleted `:676-697`): identical accounting — one head advance, `from_buf = buf.len().min(body_len)`, `buf.advance(from_buf)`, `remaining` read in capped 4 KB chunks. The only semantic delta is copy-into-`body_buf` vs discard. No double-advance, no off-by-one, no over-read past `body_len`, no leftover bytes to corrupt the next keep-alive request's parse. `consumed == req.bytes_consumed` remains valid at the new earlier position (nothing between the new and old locations touches `buf`/`downstream`).
- **No-op invariant holds** (`hcm.rs:640,664-665`): `body_len == 0` reduces to head-advance + `Bytes::new()` — body-wise identical to before for every existing fixture (all bodyless / `content-length: 0`). This is the load-bearing regression-equivalence claim and it is sound; the §7.5 differential gate (all 32 Docker-gated fixtures `0001`–`0032` green simultaneously, local + Linux CI anchor `27476243121`) is the authoritative proof.
- **Per-attempt replay safety is correct and directly tested** (`hcm.rs:359`): `run_attempt` takes `req: &Request` and the clone is non-consuming, so attempt 2 re-clones from the still-populated `req.body`. `h1_retried_post_replays_body` asserts the body appears exactly twice (failed attempt + retry).
- **Chunked 501 preserved**: a real `Transfer-Encoding: chunked` request carries no Content-Length → `body_len == 0` → body read is a no-op, and the existing 501 disposition (`hcm.rs:694`) is unchanged.
- **Timeout/EOF/Io dispositions match the former drain verbatim** (`Ok(Ok(0))→UnexpectedEof`, `Ok(Err)→Io`, `Err(_elapsed)→return Ok(())`); the relocation did NOT cross the response-write boundary, so mid-body-timeout → graceful close was already the pre-existing semantic, not a new behavior.
- **Tests assert REAL forwarded bytes, not status codes.** `h1_forwards_request_body_upstream` reads the upstream-recorded buffer and asserts `ends_with("hello world")` — it would unambiguously FAIL on the pre-change empty-body code (heeds the phase-10 backstop lesson). The keep-alive bleed test is genuinely single-connection (one `accept` + one `connect`, both POSTs written up front), so request 2 parsing cleanly truly proves request 1's body was fully consumed from `buf`. The retry test's exactly-two invariant fails both pre-change (0) and on a hypothetical move-not-clone bug (1).
- **Capture-ordering races are provably avoided**: each recording upstream records BEFORE it writes its canned response, the proxy emits downstream only after the upstream response, and `drive`/`drive_keep_alive` await EOF — so reading the shared buffer after they return observes a completed recording. Retry attempts open strictly sequential upstream connections (no interleave).
- **Scope fidelity is exact**: change confined to `hcm.rs` + the BEHAVIOR_CONTRACT note. No Cargo/dependency change, no new fixture, no new crate, no streaming `decode_data` hook, no `BufferFilter`/`Buffer`/`BufferPerRoute` — all correctly deferred to `25.2`. No under-build (the body genuinely reaches the upstream).

---

## Issues

### Critical (Must Fix)
None.

### Important (Should Fix)
None blocking. (The correctness reviewer placed the unbounded-`with_capacity` item under "Important" but explicitly de-escalated its merge impact to non-blocking and returned "Ready to merge: Yes." It is adjudicated as **Minor M25.1-1** below, with the routing rationale.)

### Minor (Nice to Have / recommendations to carry into 25.2)

- **M25.1-1 — Eager `BytesMut::with_capacity(body_len)` reserves the untrusted, uncapped client Content-Length** (`hcm.rs:641`). `parse_content_length` returns an arbitrary `usize` and there is no `max_request_bytes` guard anywhere in `envoy-http1` today, so a client sending only headers with `Content-Length: 4000000000` triggers a ~4 GB up-front reservation before any body byte arrives. This is a *new* amplification facet vs the removed drain (which used a fixed 4 KB stack buffer and never allocated proportional to the claimed CL). **Why it is non-blocking / Minor, not a §5.2 state-3 trigger:** (a) the construction is exactly what `PLAN.md` Task 1 Step 4 specified (`BytesMut::with_capacity(body_len)`), and the implementers consciously recorded the deferral in PROGRESS Task 1 Minor (a) during state-3; (b) `max_request_bytes` enforcement is the locked deliverable of `25.2` (SPEC §4), not `25.1`; (c) practical severity on the default Linux/overcommit deployment target is low — only the bytes actually received are written via `extend_from_slice`, so an attacker who sends no body faults in ~no resident pages (a virtual reservation that overcommit grants); (d) both independent reviewers returned "Ready to merge: Yes". **Recommendation for the 25.2 planner (architectural note worth surfacing now):** `25.2`'s `BufferFilter` length-checks `body.len() > max_request_bytes` in `decode_headers`, which runs AFTER `25.1` has already buffered the whole claimed body (SPEC §5.3 — full-body-before-pipeline is intentional). So the post-read length-check does NOT by itself bound the *read/allocation*; to actually cap request-body memory, `25.2` (or a follow-up) should bound the read/allocation in `hcm.rs`, e.g. reserve `min(body_len, cap)` and grow on demand, and/or reject early once `body_len > effective_max`. A one-line `with_capacity` softening (don't pre-reserve the full claimed CL) is cheap and could land in `25.2` alongside the cap.

- **M25.1-2 — Cross-TCP-segment body reassembly is not exercised by a test** (production loop `hcm.rs:645-663`). All four body tests write head+body in a single `write_all`, so on loopback the body is already in `buf` after head-parse (`from_buf == body_len`) and the `while remaining > 0` loop runs zero times — leaving the multi-read reassembly path plus its `UnexpectedEof` / idle-timeout-graceful-close dispositions uncovered by a *forwarding* test. **Non-blocking** because that loop is a verbatim relocation of the previously-tested drain loop (the read path itself is not novel — only the copy-instead-of-discard is). Recommendation: add a test that writes the head, flushes, briefly sleeps, then writes the body, to pin the reassembly path for forwarding (a good `25.2` fixture-`0033` companion or a cheap unit add).

- **M25.1-3 — `h1_keep_alive_two_bodied_posts_do_not_bleed` uses loose `contains` assertions** (`hcm.rs` ~`:2800-2810`). `got.contains("aaa")`/`contains("bbbb")` over the concatenated two-connection capture would not catch a same-connection wrong-order bleed by itself; the accompanying `POST /one`/`POST /two` presence checks + the 2-response count make a true bleed detectable, so it is not vacuous — just not maximally tight. Optional: assert ordering via `find("aaa") < find("POST /two")`.

- **M25.1-4 — `drive_keep_alive` status-line split heuristic is a latent foot-gun** (`hcm.rs:1739`): splitting recorded bytes on `HTTP/1.1 ` breaks if a future caller's response body contains that literal. Safe here (all canned responses are `Content-Length: 0`); the existing comment documents the assumption. Optional: a `debug_assert!` that the last request carries `Connection: close` (the helper otherwise hangs on `read_to_end` if a caller forgets it).

- **M25.1-5 — ~10 lines of read-loop duplication** between `spawn_recording_upstream` (`hcm.rs:2641`) and `spawn_fail_then_ok_recording_upstream` (`hcm.rs:2814`). Small, test-only; extracting a `record_until_idle` helper would DRY it. Non-blocking.

- **M25.1-6 — 200 ms per-read idle timeout** as the "request fully arrived" heuristic in the recording helpers is the one real flakiness surface (the pre-existing `spawn_capturing_upstream` uses 500 ms). Acceptable for tiny single-segment bodies; the implementers report 20/20 multi-thread green. If CI ever flakes, bump to 500 ms.

- **M25.1-7 — Stale comment** at `hcm.rs:599` ("Compute body length (for drain)") — the value now sizes/bounds the body read, not a drain. Cosmetic.

- **Deferred Minor (b) from PROGRESS Task 1 — CONFIRMED non-concern.** A chunked request carrying a bogus Content-Length now buffers those bytes into `req.body` before the `:694` 501. Traced: in BOTH the old and new code exactly `body_len` bytes are consumed from the socket and never reach an upstream (the 501 synth path does not forward `req.body`); the 501 disposition is identical. The only delta is a transient `Bytes` immediately dropped. The sole residual is the same allocation facet as M25.1-1; no separate fix needed.

---

## Assessment

**Ready to merge? Yes (APPROVED).**

**Reasoning:** The buffer-advance arithmetic, the `body_len == 0` no-op regression-equivalence invariant, the chunked-501 preservation, per-attempt replay safety, and the verbatim error/timeout dispositions are all verified correct against the actual code; the relocation is buffer-safe and does not cross the response-write boundary; the tests assert real recorded forwarded bytes (non-vacuous, with the body-forwarding and retry-replay tests genuinely failing on the pre-change code) and the capture-ordering races are provably avoided; and the scope is exactly the locked D1. The single substantive finding (eager unbounded `with_capacity` on the untrusted Content-Length, M25.1-1) is a real but low-practical-severity hardening item that was PLAN-specified, consciously deferred in state-3, and belongs to `25.2`'s `max_request_bytes` layer — both independent reviewers returned "Yes." Per §5.2 there is no Critical/Important blocker, so the phase advances to the state-6 deterministic close-out. The recommendations (M25.1-1 read/allocation bound; M25.1-2 cross-segment forwarding test) are carried forward to the `25.2` PLAN-write.
