# Phase 115 — `envoy.filters.http.health_check` — REVIEW-2 (§5 state 5, the FRESH RE-REVIEW)

> **What this file is, and why it is not `REVIEW.md`.** This is the §5 **state-5 re-review** of
> phase `115-http-filter-health-check`, conducted after the §5.2 state-3 re-entry answered round 1
> (the fix commit `e569f5b`, `PROGRESS.md` `# §5.2 STATE-3 RE-ENTRY`, design `ADR-0202`) and after
> the state-4 re-verification re-ran the full §7.5 gate (`39710cf`, `PROGRESS.md`
> `# §5 STATE 4 (re-verification after the §5.2 re-entry)`; CI run `35649049188`,
> `binaries=172 passed=2352 failed=0`). **For gate (f) it supersedes the round-1 `REVIEW.md`**,
> which is a landed artifact and was NOT edited (D-3.5); its `NOT APPROVED` verdict was answered
> by `e569f5b` and is not a live instruction.
>
> Written for a reader with **zero prior context** (D-3.4). The phase, its scope and its SPEC
> corrections are defined in `SPEC.md`, `PLAN.md`, `ADR-0200`, `ADR-0201` and `ADR-0202`. Code
> citations are to HEAD `4f5acd0`, whose code tree is identical to `e569f5b` (every later commit is
> docs-only).
>
> **This session fixed nothing it graded** (§5.1, `ADR-0127`; §6.3, `ADR-0165`). It did not re-run
> the §7.5 gate; legs (a)–(e) stand as recorded at `39710cf`. `ROADMAP.md` was not touched.

---

## 1. VERDICT

**NOT APPROVED — §5.2 fires again; the phase re-enters §5 STATE 3** (not state 4, not state 6).

**0 Critical · 1 MUST-FIX Issue · 1 required record correction · 7 Minor**, all NEW this round.
Round 1's findings are not re-issued; §6 records their disposition.

Round 1's two MUST-FIX cells are **genuinely fixed**. On fixture `0095`'s own config, H1 non-`HEAD`,
H1 `HEAD` and H2 intercepts each carry exactly upstream's framing (§3.1). The flag is
compiler-enforced at every construction site, the H1 writer emits no stray bytes, and H2 ends
the stream on the HEADERS frame. The in-process pins RED under targeted mutation (§3.2).

**But the fix decides the H1 `HEAD` framing too EARLY, and that creates a new header-set
divergence — a REGRESSION from parity — whenever a later stage writes a `content-length`.**
`decorate_filter_reply` pushes `transfer-encoding: chunked` at the decode-side `SynthFromDecode`
site. The encode-side filters and the gRPC local-reply transform run AFTER that site, and both can
add a `content-length` without removing the TE. The wire then carries BOTH framing headers. RFC
9112 §6.1 forbids a sender from doing that, and it is the classic request-smuggling ambiguity.
Upstream settles framing last, so a `content-length` wins. The issue is §2 I2-1.

---

## 2. Issues (MUST FIX) — the phase re-enters §5.2 STATE 3

### I2-1 — An H1 `HEAD` intercept carries `transfer-encoding: chunked` AND a `content-length` whenever a later stage adds one; upstream sends exactly one framing header

**Measured by this session on BOTH proxies.** The reference was `envoyproxy/envoy:v1.33.0`, digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2` by `docker inspect`, with
port-mapped listeners and an HTTP readiness poll. The subject was the landed DEBUG `envoy-bin` at
HEAD (`cargo build -p envoy-bin` exit 0, tree clean). Both were read as raw socket bytes with
`date` stripped. The base config is `0095`'s filter (`:path` `exact: /healthz`) in front of a
`direct_response` `MAIN` route, with `node.cluster: rv2`.

| cell | upstream v1.33.0 | envoy-rust at HEAD | envoy-rust at `25c5397` (pre-fix) |
|---|---|---|---|
| **3a** `HEAD /healthz`; `header_mutation` AFTER `health_check` sets response `content-length: 7` (`OVERWRITE_IF_EXISTS_OR_ADD`) | filter header, `server`, **`content-length: 7`** — no TE | filter header, **`transfer-encoding: chunked`**, `server`, `connection`, **`content-length: 7`** | filter header, `server`, `connection`, `content-length: 7` — **parity** |
| 3b same config, `GET /healthz` | `content-length: 7` | `content-length: 7` | — |
| 3a control, `HEAD /other` | `content-type`, `content-length: 7` | `content-type`, `content-length: 7` (+ body bytes, `CF-115-14`) | — |
| **G1** `HEAD /healthz`, request `content-type: application/grpc`, plain `0095` config | `content-type: application/grpc`, `grpc-status: 2`, filter header, `server`, **`transfer-encoding: chunked`** — no CL | filter header, **`transfer-encoding: chunked`**, `content-type: application/grpc`, `grpc-status: 2`, `server`, `connection`, **`content-length: 0`** | filter header, `content-type`, `grpc-status: 2`, `server`, `connection`, `content-length: 0` (divergent: CL for TE) |
| base `HEAD /healthz`, plain config | filter header, `server`, `transfer-encoding: chunked` | same set (+ `connection: keep-alive`, pre-existing ADR-0033) | — |

(`connection: keep-alive` is envoy-rust's pre-existing ADR-0033 decoration on a keep-alive
request, on every filter-synth reply. It is not charged here, as in round 1.)

**Why it is a MUST-FIX.** It is a header-set divergence on the phase's OWN reply, reached by
in-scope configs. §7.2 requires response headers to be set-equal modulo the allow-list. Cell 3a
composes the phase filter with a landed filter (`header_mutation`, phase 07.2) in the order round
1's composition probe exercised, and it was at PARITY before `e569f5b`. The fix therefore
regressed a cell. It also puts two contradictory framing headers on the wire, which RFC 9112 §6.1
says a sender MUST NOT do. G1 already diverged before the fix (`content-length: 0` for
`transfer-encoding: chunked`), but it now carries the same double framing. Round 1 raised header
divergences on this reply to MUST-FIX on the same grounds.

**Cause.** `crates/envoy-http1/src/hcm.rs`, `RequestPath::SynthFromDecode` arm (the
`decorate_filter_reply(&mut outgoing, headers_only, Some(connection_value(close)), req.method ==
"HEAD")` call). It pushes `transfer-encoding: chunked` for a `HEAD` BEFORE the encode-side
`pipeline.encode_headers` pass. It also runs before `crate::grpc::apply_grpc_local_reply` (the
`if outgoing_local` block after it). That transform drops only `content-type`/`content-length`,
keeps `transfer-encoding` as a pass-through, and appends `content-length: 0`
(`crates/envoy-http1/src/grpc.rs`, `fn apply_grpc_local_reply`, the final `out.push`).
`header_mutation` writes its `content-length` in the encode pass. Neither later stage knows that the
reply's framing was already chosen. **Nothing in the tree could see it:** the in-process HEAD pin
(`h1_health_check_head_intercept_is_chunked_and_bodyless`) and probe `p11` both use a config with no
encode-side filter and no gRPC request. The only H2 equivalents were measured at parity (below).

**H2 is NOT affected** (reviewer-measured on both proxies, frame-level): with the same
`content-length: 7` mutation, both proxies send `content-length: 7` on one HEADERS frame with
END_STREAM. The H2 decoration pushes no framing header, so no duplicate can arise.

### Required disposition for the §5.2 STATE-3 re-entry (round 2)

1. **Decide the H1 headers-only framing LAST.** Framing must be settled after the encode-side filter
   pass and after the gRPC local-reply transform, or be made equivalent to that, so that the wire
   matches every cell above: 3a → `content-length: 7` only; G1 → `transfer-encoding: chunked`,
   `content-type: application/grpc`, `grpc-status: 2` and NO `content-length`; base `HEAD` and H1
   non-`HEAD` unchanged. **Invariant: no headers-only reply may carry both framing headers.** How
   to realise it is the state-3 session's design call; record it in an ADR, which also corrects
   `ADR-0202` (append-only; see item 4). ⚠ Do NOT widen the change to other filters' local replies
   (their upstream `HEAD`/H2 framing stays unmeasured), and do not fix `CF-115-14`.
2. **Pin 3a and G1 in-process**, each RED by a mutation that restores the `e569f5b` ordering, run
   against a forced rebuild with an unmutated control.
3. **Witness differentially where the driver can express it.** G1 needs no config change:
   `Http1Probe::extra_headers` (`tests/differential/src/lib.rs`, `pub struct Http1Probe`) can carry
   `content-type: application/grpc` on a `HEAD /healthz` probe in `0095`, and the head-only read
   still compares the header set. 3a needs a `header_mutation`-after-`health_check` config, so it
   needs a new fixture or a banked carry-forward with its reason; the state-3 session chooses.
4. **Correct the record (§2 R2-1 below).**
5. Re-run the full §7.5 gate at the next state 4. A fresh state-5 session writes `REVIEW-3.md`;
   this file and `REVIEW.md` are landed and never edited.

### R2-1 (required record correction) — "upstream leaves the socket OPEN after an H1 `HEAD` intercept" is a timeout artifact

`ADR-0202` ("NEW MEASUREMENT"), `PROGRESS.md` (`# §5.2 STATE-3 RE-ENTRY`, Step 0 item 1) and the
`BEHAVIOR_CONTRACT.md` wire-shape bullet all state that upstream leaves the connection OPEN after
an H1 `HEAD` intercept, even under `Connection: close`. That reading came from a raw read with a
**1 s** timeout. **Re-measured by this session with a 10 s read, one request per connection, all
under `Connection: close`:** `HEAD /healthz` EOF after **1.001 s**, `GET /healthz` **1.002 s**,
`HEAD /other` **1.001 s**, `GET /other` **1.003 s**. Upstream closes every cell after the same
~1 s delayed close. The "open" socket was a read that stopped a millisecond before the EOF. Step 0
recorded `GET` as `<EOF>` and `HEAD` as `<TIMEOUT>`, which is the same race landing on opposite
sides. envoy-rust closes at once (reviewer-measured, 0.000 s). Connection lifetime is not compared
(§7.2 timing row), so no wire claim moves.

The head-only `HEAD` read in `drive_http1` (`ADR-0202` DECISION 3) is still correct on its OTHER
ground: RFC 9110 §9.3.2 says a `HEAD` response has no content. Only the stated reason is wrong. The
contract bullet must be corrected. `ADR-0202` is append-only and must be corrected FORWARD in the
re-entry's ADR, never edited.

---

## 3. What this session measured or re-derived itself

### 3.1 Round 1's cells are fixed

On fixture `0095`'s config, reviewer-measured on both proxies and consistent with this session's
base `HEAD` row above:

- H1 `GET /healthz` → `content-length: 0`, close and keep-alive alike;
- H1 `HEAD /healthz` → `transfer-encoding: chunked`, no CL, zero bytes after the head (no chunk
  terminator), with a pipelined `GET` answered next and no stray bytes;
- H2 `GET`/`HEAD /healthz` → filter header + `server`, one HEADERS frame with END_STREAM +
  END_HEADERS, identical on both proxies.

Other cells at parity: request `Content-Length: 0` on `HEAD`; `HEAD` with a 5-byte body and a
pipelined `GET`; `TE: trailers`; a request carrying the filter header (value is `node.cluster`,
not an echo); `header_mutation` BEFORE the filter; `local_ratelimit` before the filter, exhausted
(429 with `content-length: 18`, H1 and H2 — ADR-0033 framing unchanged, as `ADR-0202` DECISION 2
scopes it).

### 3.2 The code, read and mutated (reviewer, each mutation under a forced rebuild, target asserted to occur once, unmutated control green)

- `decorate_filter_reply` (`crates/envoy-http1/src/hcm.rs`, `pub fn decorate_filter_reply`) is
  byte-for-byte the ADR-0033 decoration when the flag is off.
- Exactly one `headers_only: true` exists in the tree (`crates/envoy-filter/src/health_check.rs`).
  `FilterResponse` has no `Default`, so every literal must state the flag. The one functional
  update (`..FilterResponse::test_200()`) takes `false`. The encode-pass `FilterResponse` built
  from `outgoing` hard-codes `false`, which is correct because the reply is already decorated by
  then. An encode-side replacement carries its own flag on both codecs.
- `Http1Response::write_to_buf` writes the head and then `resp.body` verbatim, so an empty body
  means zero bytes after `\r\n\r\n`. The H1 server loop keeps no response-framing state, so
  nothing thinks a chunked body is still owed.
- H2: `send_envoy_response` sets `end_of_stream = body_empty && trailer_map.is_none()`, so the
  headers-only reply is one HEADERS frame with END_STREAM. The H2 path never calls
  `apply_grpc_local_reply`.
- The io_uring H1 worker (`crates/envoy-http1/src/uring.rs`) is gated Router-only and never runs
  the filter pipeline, so it cannot carry the filter (checked by this session).
- Mutations, each RED as predicted: `if head_request` → `if !head_request` turned 3 tests RED
  (the dispatcher unit test, the H1 `HEAD` end-to-end test, the H1 `GET` end-to-end test). The H2
  decode arm passing `false` turned `h2_health_check_filter_intercepts_and_logs` RED. The F-3
  rider, `eq_ignore_ascii_case` → `==`, turned `path_pseudo_header_name_is_case_insensitive` RED.

### 3.3 The record re-derived (reviewer; `git diff --numstat`, `cmp`, test-attribute census)

- Re-entry net **395** excluding `docs/` (444 − 49) over `25c5397..e569f5b`.
- **7** new test functions: 8 attributes added, 1 removed; the removed one is
  `h1_health_check_filter_end_to_end`, deleted and re-added. Split: filter 2, http1 3, http2 1,
  differential 1.
- **18** `E0063` sweep sites. The tree holds 21 `headers_only:` construction literals: the 18,
  plus the 2 hand-edited `types.rs` `Self { … }` literals, plus the one `true`.
- Whole phase **1794** net over `04661b7..39710cf` (1877 − 83).
- `0095`: `envoy.yaml` ≡ `envoy-rust.yaml` by `cmp`; eleven probes; `p11` is `head`, `/healthz`,
  empty body, `set_equal_modulo_allow_list`; no `HEAD /other` probe. Re-run GREEN in a scratch
  worktree (1 test, 1.43 s — normal for a backend-free fixture).
- `p11` cannot pass vacuously: both sides must answer 200, `diff_headers` compares the name set
  and every non-allow-listed value (`content-length`, `transfer-encoding` included), and the
  recorded V6 mutation REDs exactly at `p11 … diff_headers`.
- `REVIEW.md` §2 items 1–4 each map to landed code, tests and contract text.

### 3.4 Stop condition — re-derived from disk; ALL THREE LEGS FALSE

- **(i)** `ROADMAP.md`: 123 rows, 122 `done`, 1 `planned`; not-done set exactly `{115}` at file
  line 78. Status is field 4 of a `' | '` split; field-count histogram `{6: 121, 7: 1, 10: 1}`.
- **(ii)** 14 crates; `envoy-{http3,grpc,wasm,protos,runtime,xds}` absent. `quinn`/`wasmtime`/
  `tonic`/`opentelemetry`/`prost` appear in 0 of the 28 manifests, against `tokio` in 19/28.
  `histogram` = 0 over `crates/` against `gauge` = 365 (`crates/`) / 352 (`crates/*/src/`).
- **(iii)** 11 `### ` family headings reading 11/5/3/14/3/4/6/31/6/0/13, plus 27 pre-heading rows,
  sum 123. `### WASM host family` has zero rows.

No `stop` file exists and none was created.

---

## 4. Minor (banked; the round-2 re-entry MAY take any as a rider in a file it already touches)

- **M2-1 — the strip of filter-supplied framing headers is unpinned.** Deleting the `retain`
  block in `decorate_filter_reply` leaves `envoy-http1` green (reviewer mutation, 248/0). No
  production filter supplies a `content-length` today.
- **M2-2 — the encode-side headers-only path is untested on both codecs.** Nothing covers the
  flag at the encode-side `StopAndSend` arms, and no production filter reaches them.
- **M2-3 — F-5's rider binds by name, but a swapped pair is still invisible.** Swapping
  `counter("ok")` and `counter("request_total")` leaves all 13 filter tests green (reviewer
  mutation). The index hazard is gone; the test still cannot tell the two apart.
- **M2-4 — the headers-only guard is `debug_assert!` only.** In release, a future filter that sets
  the flag AND a body would write that body after `content-length: 0` or
  `transfer-encoding: chunked`.
- **M2-5 — the contract lead says "Each bullet below names its witness", and eight do not:** the
  `:path`-verbatim bullet, the four config-validity bullets and the three "does NOT match"
  bullets (which cite CF numbers instead). The sentence "envoy-rust closes it" was also unlabelled
  and untested (and see R2-1).
- **M2-6 — `p11`'s `expected_body: ""` can never fail**, because the driver returns an empty body
  for every `HEAD`. The header set alone discriminates; the README says so, and V6 proves it.
- **M2-7 — `REVIEW.md` N-6 says a gRPC-content-type health-check reply gains `grpc-status: 0`;
  both proxies send `grpc-status: 2`** (measured in G1 and G2 above; the mapping predates this
  fix). `REVIEW.md` is landed, so the correction lives here.

---

## 5. Pre-existing divergences observed, NOT this phase's → lead list `CF-115-15`

Each was measured on both proxies by the composition reviewer. This session did NOT re-run any of
them except where marked, so they are LEADS for a future pick, not charged facts. A
`direct_response` control on `/other` reproduces each, so none is specific to the health-check
reply.

- (a) **H2 `HEAD` to a `direct_response` route sends a DATA frame `MAIN`**; upstream sends
  HEADERS+END_STREAM only. This is the H2 form of `CF-115-14`. The H1 `HEAD` body also appears on
  the `local_ratelimit` 429. Both AMEND `CF-115-14`'s scope; it is still not fixed.
- (b) Three requests in ONE write: upstream answers two and closes; envoy-rust answers all three.
- (c) Lower-case methods (`head`, `get`): upstream `400`, envoy-rust serves them. Side effect: the
  case-sensitive `req.method == "HEAD"` then frames `head /healthz` as a non-`HEAD` reply.
- (d) An unread request body on a `direct_response` reply: upstream answers `connection: close` +
  EOF; envoy-rust keeps the connection alive.
- (e) Idle H1 keep-alive: envoy-rust closes after ~5 s; upstream stays open past 10 s.
- (f) A `header_mutation`-added `transfer-encoding: chunked` is combined with `content-length` on
  EVERY envoy-rust reply (upstream: no effect on a `GET` intercept).
- (g) gRPC local replies: an H1 `HEAD /other` gRPC reply is TE-framed upstream with no
  `grpc-message`, while envoy-rust sends `content-length: 0` + `grpc-message: MAIN`. envoy-rust's
  H2 path has no gRPC local-reply conversion at all (upstream: `grpc-status: 2` on H2 too).
- (h) `codec_type: AUTO` does not accept an h2c preface (already `CF-115-13` (g) — now
  reviewer-reproduced); envoy-rust serves at most one listener per process.
- Already banked and reproduced: `Expect: 100-continue` never answered (`CF-115-13` (d)).

---

## 6. Round-1 findings — disposition

| round-1 item | disposition |
|---|---|
| I-1 (H2 `content-length: 0`) | **FIXED** at `e569f5b`; parity measured on both proxies (§3.1) |
| I-2 (H1 `HEAD` framing) | **FIXED for the plain config**; the fix's ORDER opens I2-1 |
| I-3 (contract overstated as measured) | **FIXED** per codec/method; residual label gap M2-5, and R2-1 corrects the open-socket sentence |
| F-3 (filter half), F-5, N-1 | taken as riders at `e569f5b`; F-3 proved by mutation (§3.2); F-5 see M2-3 |
| F-1/`CF-115-11`, F-2, F-3 (validator half), F-4, F-6/`CF-115-12`, F-8/`CF-115-13`, N-2 … N-12 | banked, unchanged (N-6 corrected by M2-7) |
| F-7 | not charged; still refuted for this filter (3a's GET cell and `x-mut` before the filter at parity) |

## 7. Carry-forwards

| id | status | what |
|---|---|---|
| `CF-115-1` … `-3`, `-5` … `-13` | unchanged | as recorded in `PLAN.md` / `REVIEW.md` §7 |
| `CF-115-4` | unchanged | no H2 differential witness; the H2 cells of this round are at parity |
| `CF-115-14` | **AMENDED (scope)** | also on H2 (a DATA frame after a `HEAD` reply) and on other filters' local replies (the 429); still NOT fixed |
| `CF-115-15` | **NEW (leads, unverified)** | §5 (b)–(h) — seven leads |

`CF-114-6` stays CONSUMED; `CF-75-5` and the phase-112 ALPN rider stand and are not re-costed.

---

## 8. Assessment

**Ready to close? NO — re-enter §5.2 STATE 3.** The headers-only design is right, and it fixes
every cell round 1 named. But the H1 `HEAD` framing header is chosen before the stages that can
still write a `content-length`. That turned a cell that was at parity into a double-framed reply,
and it double-frames the gRPC `HEAD` cell. The fix is narrow: settle the framing last.

| §7.5 leg | status at this review |
|---|---|
| (a) new fixtures green | PASS on the reviewed tree (`39710cf`; `0095` re-run green here) — must be re-run after the fix |
| (b) pre-existing fixtures green | PASS on the reviewed tree (CI `35649049188` `failed=0`) — must be re-run |
| (c) conformance | PASS, CI-authoritative — must be re-run |
| (d) fuzz | PASS (no new target) — must be re-run |
| (e) build / clippy / fmt / test / deny | PASS on the reviewed tree — must be re-run |
| (f) review approved | **NOT APPROVED** (this file) |

### How this review was conducted

This is a fresh context. State 3 implemented and state 4 graded, and neither may review (§5.1,
`ADR-0127`). Three read-only reviewers worked in parallel, each in its own scratch worktree at
`4f5acd0` with its own `CARGO_TARGET_DIR`, all removed afterwards; the main tree stayed clean. The
three dimensions: (1) the `e569f5b` code diff, with targeted mutations; (2) an
UNTESTED-COMPOSITION probe against BOTH proxies, H1 as raw bytes and H2 at frame level; (3) the
record, the contract and the fixture, re-derived. **Every finding charged in §2 was reproduced by
this session on disk** before it was written down: both I2-1 cells on both proxies, the pre-fix
binary at `25c5397` built in its own worktree, and the R2-1 timings. The session's containers
carried an `rv3-` prefix and were removed; no other container was touched. Findings the reviewers
measured but this session did not re-run are labelled "reviewer-measured" or banked as leads (§5).

### Next state

**§5.2 STATE 3 — the round-2 re-entry, implementing §2's required disposition**, in a SEPARATE
session (§5.1; `ADR-0127`), TDD per fix, appending to `PROGRESS.md`. Then state 4 (the full gate)
and a fresh state-5 review writing `REVIEW-3.md`. `ROADMAP.md` row `115` stays `planned`
throughout.
