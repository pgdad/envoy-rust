# Phase 115 — `envoy.filters.http.health_check` — REVIEW-4 (§5 state 5, the THIRD FRESH RE-REVIEW)

> **What this file is, and why it is not `REVIEW.md`, `REVIEW-2.md` or `REVIEW-3.md`.** This is
> the §5 **state-5 re-review, round 4**, of phase `115-http-filter-health-check`. It was conducted
> after the THIRD §5.2 state-3 re-entry answered `REVIEW-3.md` (fix commit `3c58d1e`,
> `PROGRESS.md` `# §5.2 STATE-3 RE-ENTRY (round 3)`, design `ADR-0204`). It also follows the
> state-4 re-verification that re-ran the full §7.5 gate (`ef1848d`, `PROGRESS.md`
> `# §5 STATE 4 (re-verification after the round-3 §5.2 re-entry)`; CI run `35846668145`,
> `binaries=174 passed=2360 failed=0`). **For gate (f) it supersedes `REVIEW.md`, `REVIEW-2.md`
> and `REVIEW-3.md`.** All three are landed artifacts and were NOT edited (D-3.5). Their
> `NOT APPROVED` verdicts were answered by `e569f5b`, `a1c1623` and `3c58d1e` and are not live
> instructions.
>
> Written for a reader with **zero prior context** (D-3.4). The phase, its scope and its
> corrections are defined in `SPEC.md`, `PLAN.md` and `ADR-0200` … `ADR-0204`. Code citations are to
> HEAD `1856d2d`. Its code tree is identical to `3c58d1e`: `git diff --stat 3c58d1e HEAD -- crates
> tests` is empty, because every later commit is docs-only.
>
> **This session fixed nothing it graded** (§5.1, `ADR-0127`; §6.3, `ADR-0165`). It did not re-run
> the whole §7.5 gate: legs (b)–(e) stand as recorded at `ef1848d`. It did re-run leg (a)'s four
> phase fixtures on the reviewed tree (§3.3). `ROADMAP.md` was not touched.

---

## 1. VERDICT

**APPROVED. §7.5 leg (f) passes, and the phase goes to §5 STATE 6 (the close-out)** in a separate
session. It does not re-enter state 3.

**0 Critical · 0 MUST-FIX Issues · 0 required record corrections · 5 Minor**, all NEW this round
and all banked. Findings from earlier rounds are not re-issued; §6 records how each was disposed of.

**Round 3's MUST-FIX is genuinely fixed.** Every cell `REVIEW-3.md` I3-1 named is now at parity on
both proxies, on the raw wire (§3.1). So are `REVIEW-3.md`'s unchanged-cells list (A2, `REVIEW-2.md`
3a, the base `GET` and `HEAD`) and the H2 twins. `REVIEW-3.md` R3-1's three harness statements are
corrected, and no live statement of the refuted premise remains in code, fixtures or the contract
(§3.4).

**The design now matches upstream's model, not a list of cells.** No framing header rides through
the encode pass on a headers-only reply. The codec frames it once, at the wire: a `content-length`
some stage wrote, else `transfer-encoding: chunked` for a `HEAD` and `content-length: 0` for
anything else. A stage-written `transfer-encoding` never survives. This session varied the LATER
stage's ACTION as well as its header, and its ORDER, key case and value (§3.2). No new composition
diverges on the phase's own reply except by a mechanism that reproduces identically on a
non-intercept reply and is already banked (`CF-115-16` (a) and (c), §5).

---

## 2. Issues (MUST FIX)

**None.**

---

## 3. What this session measured or re-derived itself

### 3.1 Round 3's cells are fixed, measured on both proxies

**Probe.** The reference was `envoyproxy/envoy:v1.33.0`, pinned by digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2` (`docker image inspect`).
Each config ran in its own container under `docker run -p`, loaded by `docker cp`. The subject was
the landed DEBUG `envoy-bin` at HEAD, built by `cargo build -p envoy-bin` (exit 0, 0 `Compiling`, md5
`4c8fefd49941b365c9c6f486d2e6d4cc`, the md5 the state-4 gate recorded for that command). Readiness
was an HTTP poll (`HEAD /healthz`, raw socket), never TCP. Every H1 cell is one request per
connection under `Connection: close`, read as raw socket bytes with a 10 s read and `date` stripped.
Both proxies reached EOF on every cell (upstream after its ~1 s delayed close). The base config is a
`health_check` filter (`:path` `exact: /healthz`) in front of a `direct_response` `MAIN` catch-all,
with `node.cluster: r4-cluster`. Only the `header_mutation` entries change from cell to cell. `hc`
abbreviates `x-envoy-upstream-healthchecked-cluster`. "Parity" means equal header multisets, with
`server` values ignored (on the allow-list); wire ORDER differs and is not compared.

| cell | `GET /healthz` (both proxies) | `HEAD /healthz` (both proxies) |
|---|---|---|
| base (no mutation) | `content-length: 0` — parity | `transfer-encoding: chunked` — parity |
| **A1** `content-length: 7`, APPEND, after | ONE `content-length: 7` — **parity** | ONE `content-length: 7` — parity |
| **B4** A1 placed BEFORE `health_check` | ONE `content-length: 7` — **parity** | ONE `content-length: 7` — parity |
| **B6** A1 with value `0` | ONE `content-length: 0` — **parity** | ONE `content-length: 0` — parity |
| **A3** `transfer-encoding: gzip`, OVERWRITE, after | `content-length: 0`, no TE — parity | **`transfer-encoding: chunked`** — **parity** |
| **B7** `transfer-encoding: gzip`, APPEND, after | `content-length: 0`, no TE — parity | **`transfer-encoding: chunked`** — **parity** |
| A2 `transfer-encoding: chunked`, APPEND, after | `content-length: 0` — parity | `transfer-encoding: chunked` — parity |
| `REVIEW-2.md` 3a `content-length: 7`, OVERWRITE, after | `content-length: 7` — parity | `content-length: 7` — parity |

**No envoy-rust intercept reply this session captured carried both `content-length` and
`transfer-encoding`, or two framing rows**, except the double-append cells of §3.2, where upstream
emits a single joined `content-length: 7,9` row, so it too frames with a `content-length` only.

### 3.2 Untested compositions, probed on both proxies

The handoff's instruction was to vary the later stage's ACTION as well as its header.
`APPEND_IF_EXISTS_OR_ADD` is upstream's proto default. envoy-rust rejects `ADD_IF_ABSENT` and
`OVERWRITE_IF_EXISTS` at load, so APPEND and OVERWRITE are the two actions any accepted config can
use. envoy-rust's `header_mutation` has only `append` entries (no `remove`), and envoy-rust has no
route-level `response_headers_to_add`, so those shapes cannot be expressed.

| cell | config (all AFTER `health_check` unless stated) | result, `GET` and `HEAD` |
|---|---|---|
| N3 | APPEND `content-length: 7`, then OVERWRITE `content-length: 9` | ONE `content-length: 9` — **parity** |
| N4 | APPEND `content-length: 7` + APPEND `transfer-encoding: gzip` | ONE `content-length: 7`, no TE — **parity** |
| N5 | OVERWRITE `transfer-encoding: gzip`, then OVERWRITE `content-length: 5` | ONE `content-length: 5`, no TE — **parity** |
| N6 | OVERWRITE `content-length: abc` (non-numeric) | `content-length: abc` on both — **parity** (neither proxy validates it) |
| N7 | OVERWRITE `transfer-encoding: identity` | `GET` `content-length: 0`, `HEAD` `transfer-encoding: chunked` — **parity** |
| N12 | APPEND key `Content-Length` (mixed case) `7` | ONE `content-length: 7` — **parity** |
| N13 | APPEND key `Transfer-Encoding` (mixed case) `gzip` | `GET` CL 0, `HEAD` TE chunked — **parity** |
| N8 | a `cors` stage after `health_check` (per-route `CorsPolicy`), requests carrying `Origin`; plus an `OPTIONS` preflight | base framing, no CORS headers on either proxy — **parity** (the intercept answers the preflight too, on both) |
| N9 | A1's config, keep-alive `HEAD /healthz` or `GET /healthz`, pipelined with `GET /other` | the intercept heads match apart from envoy-rust's pre-existing ADR-0033 `connection: keep-alive` (not charged, as in `REVIEW-3.md` §3.1); the `/other` head differs by `CF-115-16` (c) |
| **N1** | APPEND `content-length: 7`, then APPEND `content-length: 9` (one filter) | upstream ONE row `content-length: 7,9`; envoy-rust TWO rows `7`, `9` — **differs, `CF-115-16` (c)** |
| **N2** | APPEND `content-length: 7` BEFORE and APPEND `9` AFTER `health_check` | upstream `content-length: 9,7`; envoy-rust rows `9`, `7` — **differs, `CF-115-16` (c)** |
| **N10** | a gRPC request (`content-type: application/grpc`) + APPEND `content-length: 0` | `GET` parity (`grpc-status: 2`, `content-length: 0`); `HEAD` upstream `content-length: 0`, envoy-rust `transfer-encoding: chunked` — **differs, `CF-115-16` (a) / M3-4** |
| N11 (control) | N1's single-APPEND `content-length: 7` config, `GET`/`HEAD /other` (the `direct_response` reply, NOT the intercept) | upstream `content-length: 4,7`; envoy-rust `content-length: 4` + `content-length: 7` — the same join divergence on a reply this phase does not own |

**H2** (`codec_type: HTTP2`, `curl --http2-prior-knowledge`): the base cell, A1 and B7 are at
parity on both methods (A1 → `content-length: 7`; B7 → no `transfer-encoding` on either proxy). The
N1 double-append ends in a client-side `PROTOCOL_ERROR` on BOTH proxies: upstream sends the joined
`content-length: 7,9`, envoy-rust sends two rows. The same happens on the `/other` reply, so it is
`CF-115-16` (c) again.

**Why N1, N2 and N10 are not charged to this phase.**
- N1/N2 need two `content-length` writes. upstream joins an appended value into an existing inline
  header (`content-length` is inline). envoy-rust's `header_mutation` APPEND pushes a second row.
  That is the phase-07.2 append semantics, and N11 shows it identically on the `direct_response`
  reply. `REVIEW-3.md` §2 item 1 explicitly kept that join out of this phase's scope. ADR-0204
  also rejected option (b) (teaching APPEND to overwrite a `content-length`) on those grounds.
  The composition is also pathological on both proxies: upstream's `7,9` is itself an invalid
  `content-length`.
- N10 is `REVIEW-3.md` §5 (a) with the value `0` instead of `7`. The gRPC transform runs after the
  encode pass. The settle step's gRPC-`HEAD` rule (1) then discards the stage-written
  `content-length` (M3-4). Both are banked under `CF-115-16` (a).

### 3.3 The code, read and mutated

**By this session.** In its own worktree at HEAD with its own `CARGO_TARGET_DIR`:
- The unmutated control was `test result: ok. 254 passed; 0 failed` (`Compiling envoy-http1` = 1).
- **M1**, exactly as `PROGRESS.md` round 3 defines it: `if headers_only && req.method != "HEAD"`
  push `content-length: 0` after the `SynthFromDecode` decoration. The anchor was asserted unique
  and `Compiling envoy-http1` = 1. The result was `FAILED. 253 passed; 1 failed`. The one red was
  `h1_health_check_intercept_keeps_an_appended_content_length_alone`, exactly the recorded pin. The
  file was restored md5-identical (`0e4f5787…`).

**By a read-only code reviewer** in its own worktree and target dir. For every mutation it
asserted the target unique and `Compiling envoy-http1` = 1, then restored the file md5-identical.

| mutation | `test result` | RED tests | vs. record |
|---|---|---|---|
| control | 254/0 | — | = |
| M2 restore the old rule (3) | 252/2 | the A3/B7 test; the settle unit test | = recorded |
| M3 delete the encode-side `headers_only_reply = headers_only` | 253/1 | `h1_encode_side_headers_only_replacement_is_framed_last` | = recorded |
| M1′ push `content-length: 0` for EVERY H1 headers-only reply inside `decorate_filter_reply` | 249/5 | A1 test, A3/B7 test, encode-side test, `h1_health_check_head_intercept_is_chunked_and_bodyless`, the dispatcher unit test | not the recorded M1: `decorate_filter_reply` no longer takes the method, so this variant also hits `HEAD`. This session's M1 above is the faithful one |
| M4 disable the gRPC-`HEAD` `content-length` drop | 252/2 | the gRPC `HEAD` end-to-end test; settle unit | pinned |
| M5 case-SENSITIVE `content-length` check in settle | **254/0** | — | **unpinned** — Minor M4-2 |
| M6 settle only under `Connection: close` | 253/1 | `h1_health_check_head_intercept_is_chunked_and_bodyless` | pinned (for `HEAD` only) |
| M6b skip settle for a keep-alive non-`HEAD` | **254/0** | — | **unpinned** — Minor M4-1 |
| M7 drop `transfer-encoding` only on `HEAD` | 252/2 | the A3/B7 test; settle unit | pinned |
| M8 settle at the decode site, before the encode pass | 249/5 | encode-side, A1, A3/B7, `…head_intercept_keeps_a_later_content_length_alone`, gRPC `HEAD` | pinned |

Also verified by reading the code:
- `settle_headers_only_framing` is the single framing point for an H1 headers-only reply, and is
  called once (`crates/envoy-http1/src/hcm.rs` ~1513). That is after `pipeline.encode_headers` and
  `apply_grpc_local_reply`, and before the wire write. Nothing between the decode-side assignment
  and the writer returns early or writes a header.
- The encode-side replacement sets `headers_only_reply` both ways.
- The io_uring worker never runs the filter pipeline.
- `decorate_filter_reply` has exactly three callers: the two H1 sites and the H2 wrapper
  (`crates/envoy-http2/src/response.rs`, `connection: None`). Only `health_check` sets
  `headers_only: true`.
- **H2 behaviour is unchanged**: the removed push was gated on `connection.is_some()`, which H2
  never passes.
- Every framing-name comparison is case-insensitive (`eq_ignore_ascii_case`).
- `cargo clippy -p envoy-http1 -p envoy-http2 --all-targets -- -D warnings` exited 0 with 68
  `Checking` lines, so it was not a cached no-op. `cargo fmt --all -- --check` was clean.

**Leg (a) re-run on the reviewed tree** (debug `envoy-bin` md5 `4c8fefd4…`): `cargo test -p
differential` passed `--test http_filter_health_check` (`0095`, 1.22 s),
`--test http_filter_health_check_stats` (`0096`, 1.41 s), `--test http_filter_health_check_framing`
(`0097`, 1.28 s) and `--test http_filter_health_check_transfer_encoding` (`0098`, 1.21 s), each
`1 passed; 0 failed`. Those durations are normal for backend-free fixtures.

### 3.4 The record, re-derived by a read-only auditor

Every claim checked HOLDS:
- **Size:** round 3 is net **351** excluding `docs/` (`465 114`, 10 files) over `4fa7e71..3c58d1e`.
  The whole phase is **2572** (`2659 87`, 36 files) over `04661b7..HEAD`. The commits after
  `3c58d1e` (`0ca1710`, `fe45755`, `ef1848d`, `1856d2d`) touch only `docs/`.
- **Tests:** +4 test attributes, −0, and no `#[ignore]`. The `envoy-http1` source goes 251 → 254,
  and `tests/differential/tests` goes 96 → 97 files. This is consistent with the identity
  `2356 + 4 = 2360` over `173 + 1 = 174` binaries.
- **`0098`:** the configs are byte-identical (`cmp`; md5 `a7845079…`). There are two probes (`p1`
  `HEAD`, `p2` `GET`), and the README, `expectations.yaml`, the test doc and `ADR-0204` agree.
  `diff_headers` compares the lower-cased NAME set and then the FIRST value per name. The only
  names exempt are the allow-listed `server`, `date` and `x-envoy-upstream-service-time`, so
  `gzip` ≠ `chunked` discriminates `p1`.
- **R3-1:** all three statements in `tests/differential/src/lib.rs` are corrected. A tree-wide grep
  finds no LIVE statement of the "upstream leaves the `HEAD` socket open" premise. Every remaining
  hit quotes it as corrected. `ADR-0202` is historical, append-only, and corrected forward.
- **The contract's settled-last paragraph** is true against the code for both methods and both
  actions, and every witness it names exists at HEAD.
- **`REVIEW-3.md` disposition** items 1–4, M3-1 and M3-3 are each addressed at `3c58d1e`.
- **`ADR-0204`'s checkable claims hold:** the load-time rejection of `ADD_IF_ABSENT` and
  `OVERWRITE_IF_EXISTS`; `ADR-0203`'s recorded `GET` order; the dispatcher tests losing their method
  axis; H2 unchanged in behaviour.
- **Static checks:** `#![forbid(unsafe_code)]` is present in all 22 workspace member roots. The
  round-3 `+` lines add no `allow(`, `#[expect`, `unsafe`, `#[ignore]`, `todo!` or `dbg!`.

### 3.5 Stop condition — re-derived from disk; ALL THREE LEGS FALSE

- **(i)** `ROADMAP.md`: 123 rows, 122 `done`, 1 `planned`; the not-done set is exactly `{115}` at
  file line 78. Status is field 4 of a `' | '` split. The field-count histogram is
  `{6: 121, 7: 1, 10: 1}`.
- **(ii)** 14 crates; `envoy-{http3,grpc,wasm,protos,runtime,xds}` are absent.
  `quinn`/`wasmtime`/`tonic`/`opentelemetry`/`prost` appear in 0 of the 28 manifests, against
  `tokio` in 19. `histogram` = 0 against `gauge` = 365 over `crates/` and 352 over
  `crates/*/src/`, both counted with `grep -ro … | wc -l`.
- **(iii)** 11 `### ` family headings reading 11/5/3/14/3/4/6/31/6/0/13, plus 27 pre-heading rows,
  sum to 123. `### WASM host family` has zero rows.

No `stop` file exists and none was created. The state-6 close-out will make leg (i) TRUE (123/123
`done`). That does NOT complete the mission: legs (ii) and (iii) stay FALSE.

---

## 4. Minor (banked; none blocks the close-out)

- **M4-1 — the keep-alive non-`HEAD` intercept's framing is unpinned.** Skipping the settle step
  for a keep-alive `GET` leaves all 254 `envoy-http1` tests green (M6b). The only keep-alive intercept
  test is the `HEAD` one. The differential driver always sends `Connection: close`, so no fixture
  can see it either. The shipped code is correct: the settle call is not gated on keep-alive, and
  §3.2 N9 measured a keep-alive `GET /healthz` at parity. A pipelined `GET /healthz` + `GET /other`
  unit test would pin it. Load-balancer health checkers usually hold keep-alive connections.
- **M4-2 — the settle step's case-insensitive `content-length` match is unpinned** (M5 green). No
  accepted config reaches it today, because `header_mutation` lowercases keys at build time (§3.2
  N12 measured a mixed-case key at parity). A unit cell with `("Content-Length", "7")` would pin
  it.
- **M4-3 — a stale test name.** `decorate_filter_reply_frames_a_headers_only_reply_per_codec_and_method`
  no longer varies the method, and the dispatcher no longer frames a headers-only reply.
- **M4-4 — `tests/fixtures/0095-http-filter-health-check/expectations.yaml` (the `p12` comment)**
  says `transfer-encoding: chunked` "stays". Since `ADR-0204` nothing writes it before the settle
  step, which ADDS it. The observable outcome it describes is still true.
- **M4-5 — `0098` `p2` cannot discriminate the pre-fix tree.** The pre-`ADR-0204` settle step
  already dropped a stage-written `transfer-encoding` beside the `content-length: 0` it pushed, so
  `p2` passes either way. `p1` is the discriminating probe, and V2 proves it does. No document
  claims otherwise; this is recorded so no later session counts `p2` as a witness of the fix.

Not charged: `STATE.md`'s round-1 `### Phase-115 §5.2 state-3 re-entry` Notes still carry the
refuted "socket OPEN / never-closed" wording. That block is a historical ledger entry, and the later
`### Phase-115 §5 state-5 re-review` block corrects it; the close-out relocates both verbatim.

---

## 5. Pre-existing divergences re-measured by THIS session → `CF-115-16`

Nothing NEW is opened. Two leads gain measured cells:
- **`CF-115-16` (a)** (gRPC transform after the encode pass, with M3-4): N10, a gRPC `HEAD`
  intercept + APPEND `content-length: 0` → upstream `content-length: 0`, envoy-rust
  `transfer-encoding: chunked`.
- **`CF-115-16` (c)** (`header_mutation` APPEND does not join into an inline header): N1/N2, two
  `content-length` appends on the intercept → upstream one joined row (`7,9` / `9,7`), envoy-rust
  two rows. On H2, both proxies end the stream in a client-side `PROTOCOL_ERROR`. N11 reproduces
  the same mechanism on the `direct_response` reply.

---

## 6. Earlier findings — disposition

| item | disposition |
|---|---|
| `REVIEW-3.md` I3-1 (A1, B4, B6, A3, B7) | **FIXED** at `3c58d1e`; parity measured on both proxies and both methods (§3.1); pins RED under M1 (this session, faithful) and M2 (the reviewer) |
| `REVIEW-3.md` R3-1 | **FIXED** — all three statements corrected; no live statement of the premise remains (§3.4) |
| `REVIEW-3.md` M3-1 | **FIXED** — `h1_encode_side_headers_only_replacement_is_framed_last`, RED under M3 |
| `REVIEW-3.md` M3-3 | **FIXED** — both doc comments name the settled-last rule |
| `REVIEW-3.md` M3-5 | stands (banked) |
| `REVIEW-3.md` M3-2, M3-4 | banked, unchanged (M3-4 re-measured as N10, §5) |
| `REVIEW-2.md` I2-1, `REVIEW.md` I-1 … I-3 | stay FIXED (3a, base cells and H2 re-measured at parity) |
| `REVIEW-2.md` M2-1 … M2-4, M2-6 | banked, unchanged |

## 7. Carry-forwards

| id | status | what |
|---|---|---|
| `CF-115-1` … `-3`, `-5` … `-13` | unchanged | as recorded in `PLAN.md` / `REVIEW.md` §7 |
| `CF-115-4` | unchanged | no H2 differential witness; the H2 cells of this round are at parity |
| `CF-115-14` | unchanged | envoy-rust writes a body after a non-intercepted H1 `HEAD` reply (N11 `HEAD /other` shows it again) |
| `CF-115-15` | unchanged | (f)'s intercept half stays fixed |
| `CF-115-16` | **(a) and (c) re-measured** | §5; nothing new opened |

`CF-114-6` stays CONSUMED. `CF-75-5` and the phase-112 ALPN rider stand and are not re-costed.

---

## 8. Assessment

**Ready to close? YES — proceed to §5 STATE 6.** Round 3 settled all of the H1 headers-only
framing last, which is the model upstream's codec follows. Every cell the three earlier reviews
charged is at parity on both proxies. A fresh sweep of compositions that varied the later stage's
action, order, key case and value found no divergence on the phase's own reply that is not already
banked and reproduced on another reply. The remaining gaps are test coverage, not wire behaviour
(§4).

| §7.5 leg | status at this review |
|---|---|
| (a) new fixtures green | PASS — `0095`–`0098` re-run green here on the reviewed tree (§3.3); also recorded at `ef1848d` |
| (b) pre-existing fixtures green | PASS (CI `35846668145`, 174 `ok` rows, `failed=0`; local reds classified by an interleaved control at `ef1848d`) |
| (c) conformance | PASS, CI-authoritative (`h2spec not found` = 0) |
| (d) fuzz | PASS (no new target; five targets clean at 30 s, `ef1848d`) |
| (e) build / clippy / fmt / test / deny | PASS (`ef1848d`; clippy and fmt re-run clean on `envoy-http1`/`envoy-http2` here) |
| (f) review approved | **APPROVED** (this file) |

### How this review was conducted

This is a fresh context. State 3 implemented the fix and state 4 graded it, and neither may review
it (§5.1, `ADR-0127`).

**Who did what.**
- This session ran the two-proxy composition probe itself. That was 43 H1 requests over 21
  configs, plus 8 H2 requests over 4 configs and 4 on a fifth H2 config (`/healthz` and `/other`).
  Each ran against the pinned upstream and the HEAD `envoy-bin`.
- This session re-ran the four phase fixtures and the faithful M1 mutation itself.
- Two read-only reviewers ran in parallel, each in its OWN scratch subdirectory. The code reviewer
  also had its own worktree and `CARGO_TARGET_DIR`. They covered (1) the `3c58d1e` code diff with
  ten mutations, and (2) the record, contract and fixtures, re-derived.
- **Every cell in §3.1 and §3.2 was measured by this session on both proxies.**

**Hygiene.** The session's containers carried an `r4p-` prefix and were all removed
(`docker ps -a` shows 0 `r4p-` containers). No other container was touched. Every worktree this
session created was removed, and the main tree stayed clean. The separate scratch subdirectories
prevented the file-name collision `REVIEW-3.md` recorded.

### Next state

**§5 STATE 6 — the phase-115 close-out**, in a SEPARATE session (§5.1; `ADR-0127`). The commit
message is `phase 115: … [ADR-0200, ADR-0201, ADR-0202, ADR-0203, ADR-0204]`. `ROADMAP.md` row `115`'s
status cell alone flips to `done`, and the phase-115 Notes subsections are relocated to a NEW EOF
section of `STATE_HISTORY.md`. No ADR and no new Notes subsection. Then a separate state-0/1
next-phase pick. `ADR-0205` is next free.
