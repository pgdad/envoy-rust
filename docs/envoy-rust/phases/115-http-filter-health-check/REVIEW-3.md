# Phase 115 — `envoy.filters.http.health_check` — REVIEW-3 (§5 state 5, the SECOND FRESH RE-REVIEW)

> **What this file is, and why it is not `REVIEW.md` or `REVIEW-2.md`.** This is the §5
> **state-5 re-review, round 3**, of phase `115-http-filter-health-check`. It was conducted after
> the SECOND §5.2 state-3 re-entry answered `REVIEW-2.md` (fix commit `a1c1623`, `PROGRESS.md`
> `# §5.2 STATE-3 RE-ENTRY (round 2)`, design `ADR-0203`). It also follows the state-4
> re-verification that re-ran the full §7.5 gate (`c352b57`, `PROGRESS.md`
> `# §5 STATE 4 (re-verification after the round-2 §5.2 re-entry)`; CI run `35755023684`,
> `binaries=173 passed=2356 failed=0`). **For gate (f) it supersedes `REVIEW.md` and
> `REVIEW-2.md`.** Both are landed artifacts and were NOT edited (D-3.5). Their `NOT APPROVED`
> verdicts were answered by `e569f5b` and `a1c1623` and are not live instructions.
>
> Written for a reader with **zero prior context** (D-3.4). The phase, its scope and its
> corrections are defined in `SPEC.md`, `PLAN.md` and `ADR-0200` … `ADR-0203`. Code citations are to
> HEAD `ccb7fb7`. Its code tree is identical to `a1c1623`, because every later commit is docs-only.
>
> **This session fixed nothing it graded** (§5.1, `ADR-0127`; §6.3, `ADR-0165`). It did not re-run
> the §7.5 gate: legs (a)–(e) stand as recorded at `c352b57`. `ROADMAP.md` was not touched.

---

## 1. VERDICT

**NOT APPROVED. §5.2 fires a third time, and the phase re-enters §5 STATE 3 (round 3).** It does
not go to state 4 or state 6.

**0 Critical · 1 MUST-FIX Issue · 1 required record correction · 5 Minor**, all NEW this round.
Findings from earlier rounds are not re-issued; §6 records how each was disposed of.

**Round 2's MUST-FIX is genuinely fixed.** Every cell `REVIEW-2.md` I2-1 named is now at parity on
both proxies. The invariant *no headers-only reply carries both `content-length` and
`transfer-encoding`* held on every cell this session measured (§3.1). The new function and the
flag that drives it are pinned: six of seven targeted mutations turn a test RED (§3.2).

**But round 2 settled only HALF of the framing last.** `settle_headers_only_framing` decides the
H1 `HEAD` framing after every stage, but the H1 non-`HEAD` `content-length: 0` is still pushed at
the decode-side intercept site. That is before the encode-side filter pass. Upstream's headers-only
reply carries **no** `content-length` through its encode pass; its codec writes `content-length: 0`
at the wire only when nothing else framed the reply. So a later stage that APPENDS a
`content-length` gives envoy-rust TWO `content-length` rows. The appending stage is
`header_mutation` with `append_action: APPEND_IF_EXISTS_OR_ADD`, upstream's proto default. With
a value of `7` the rows are `0` and `7`, which RFC 9112 §6.3 treats as an unrecoverable framing
error.
Upstream sends `content-length: 7` alone. `ADR-0203`'s own headline — *"a `content-length` a later
stage writes wins alone"* — is false for every non-`HEAD` intercept. The issue is §2 I3-1.

---

## 2. Issues (MUST FIX) — the phase re-enters §5.2 STATE 3

### I3-1 — An H1 non-`HEAD` intercept's `content-length: 0` is chosen before the encode pass, so an appended `content-length` gives two rows; and a stage-written `transfer-encoding` value survives on a `HEAD` intercept

**Measured by this session on BOTH proxies.** The reference was `envoyproxy/envoy:v1.33.0`, with
digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2` confirmed by
`docker image inspect`. It ran under `docker run -p` and was loaded by `docker cp`, with an HTTP
readiness poll (`HEAD /other`). The subject was the landed DEBUG `envoy-bin` at HEAD, built with
`cargo build -p envoy-bin` (exit 0, md5 `8f0daf79d7cecf7af4ab6f10f31ec61e`, the md5 the state-4
gate recorded). A third column is the pre-round-2 binary, `7aaf296` (the `e569f5b` code), built in
its own worktree with its own `CARGO_TARGET_DIR`. Every cell is one request per connection under
`Connection: close`, read as raw socket bytes with a 10 s read and `date` stripped. The base
config is a `health_check` filter (`:path` `exact: /healthz`) in front of a `direct_response`
`MAIN` catch-all, with `node.cluster: r3-cluster`. Only the `header_mutation` entry changes from
cell to cell. Headers are listed in wire order; the filter header
`x-envoy-upstream-healthchecked-cluster` is abbreviated `hc`.

| cell | upstream v1.33.0 | envoy-rust at HEAD | envoy-rust at `7aaf296` |
|---|---|---|---|
| **A1** `GET /healthz`; `header_mutation` AFTER `health_check`, response `content-length: 7`, **`APPEND_IF_EXISTS_OR_ADD`** | **`content-length: 7`**, `hc`, `server`, `connection` | `hc`, **`content-length: 0`**, `server`, `connection`, **`content-length: 7`** | same as HEAD |
| **B4** A1 with the `header_mutation` placed BEFORE `health_check` | `hc`, **`content-length: 7`**, `server`, `connection` | `hc`, **`content-length: 0`**, `server`, `connection`, **`content-length: 7`** | same as HEAD |
| **B6** A1 with value `0` | **one** `content-length: 0` | **two** `content-length: 0` rows | same as HEAD |
| A1 `HEAD /healthz` | `content-length: 7` | `content-length: 7` — **parity** | `transfer-encoding: chunked` + `content-length: 7` (I2-1, fixed) |
| **A3** `HEAD /healthz`; `transfer-encoding: gzip`, `OVERWRITE_IF_EXISTS_OR_ADD`, after | **`transfer-encoding: chunked`** | **`transfer-encoding: gzip`** | `transfer-encoding: gzip` |
| **B7** `HEAD /healthz`; `transfer-encoding: gzip`, `APPEND_IF_EXISTS_OR_ADD`, after | **`transfer-encoding: chunked`** | **`transfer-encoding: gzip`** | `transfer-encoding: chunked` + `transfer-encoding: gzip` |
| A3 `GET /healthz` | `content-length: 0`, no TE | `content-length: 0`, no TE — parity | `content-length: 0` + `transfer-encoding: gzip` |
| A2 `GET` / `HEAD /healthz`; `transfer-encoding: chunked` appended, after | `content-length: 0` / `transfer-encoding: chunked` | the same — parity | TE beside CL / two TE rows |

**Controls, which show what upstream does with a framing header a filter stage wrote.**
- On a `direct_response` reply, whose `content-length` IS in upstream's header map, A1's append
  gives upstream ONE row, `content-length: 4,7`. That is upstream's inline-header join, and a
  different shape from the health-check cell.
- The same reply with B7's `transfer-encoding: gzip` gives upstream `content-length: 4` and NO
  `transfer-encoding`.

Upstream never lets a stage-written `transfer-encoding` onto the H1 wire. Its codec frames the
reply: it uses the `content-length` in the header map if there is one, and otherwise
`transfer-encoding: chunked` for a `HEAD` and `content-length: 0` for anything else. The same
cells on H2, driven with `curl --http2-prior-knowledge`, are at parity: A1 gives `content-length:
7` on both proxies, and A3 gives no `transfer-encoding` on either.

**Why it is a MUST-FIX.** It is a header-set divergence on the phase's OWN reply, and in-scope
configs reach it. §7.2 requires response headers to be set-equal modulo the allow-list. A1 is
`REVIEW-2.md` cell 3a with the ONE other `append_action` envoy-rust supports, and that action is
upstream's proto default (envoy-rust requires the field to be written out, but accepts it). The
reply carries two `content-length` values that disagree, which RFC 9112 §6.3 makes an
unrecoverable framing error for the recipient. It is the same request-smuggling ambiguity round 2
charged for `transfer-encoding` beside `content-length`. Round 1 and round 2 both raised header
divergences on this reply to MUST-FIX on these grounds.

It is **not a regression**: the `7aaf296` binary sends the same A1, B4 and B6 bytes. The
decode-site push dates from the phase's first landing, which reused the ADR-0033 decoration. It was
kept deliberately by `ADR-0202` DECISION 2 and by `ADR-0203` DECISION 1.

A3 and B7 are the `HEAD`-side twin. `settle_headers_only_framing` rule (3) keeps any
`transfer-encoding` a stage wrote, and its unit test pins that rule with the comment "A
transfer-encoding some stage wrote, with no content-length, stays". Upstream replaces it.

**Cause.**
- `crates/envoy-http1/src/hcm.rs`, `pub fn decorate_filter_reply`, the `if connection.is_some() &&
  !head_request` push of `content-length: 0`. It runs at the decode-side
  `RequestPath::SynthFromDecode` arm, before `pipeline.encode_headers`.
- `header_mutation`'s `Append` branch (`crates/envoy-filter/src/header_mutation.rs`,
  `fn apply_mutations`) pushes a second row.
- `settle_headers_only_framing` only asks whether ANY `content-length` exists, so it keeps both
  rows. Its `else if !resp.headers.iter().any(is_te)` branch keeps a stage-written
  `transfer-encoding` value.

**Why the round-2 design did not see it.** `ADR-0203` weighed exactly this change as option (c),
"defer the non-`HEAD` `content-length: 0` too", and rejected it for two reasons. Neither holds on
this tree:
- It "would change what an `ADD_IF_ABSENT` response mutation observes". But envoy-rust rejects
  `ADD_IF_ABSENT` and `OVERWRITE_IF_EXISTS` at load
  (`crates/envoy-config/src/bootstrap.rs`, `UnsupportedHeaderMutationAppendAction`). The
  observable difference falls on `APPEND_IF_EXISTS_OR_ADD`, which is where envoy-rust is wrong
  today.
- It "would move the non-`HEAD` header's wire position". But `ADR-0203`'s own table records
  upstream's `GET` order as `…, connection: close, content-length: 0`, with `content-length`
  last. Envoy-rust sends it second today, so deferring moves it TOWARD upstream. (Order is not
  compared in any case.)

**Nothing in the tree could see it.** Every `header_mutation` composition pinned by the phase uses
`OVERWRITE_IF_EXISTS_OR_ADD`, which REMOVES the early `content-length: 0` before adding its own:
`0097`, `h1_health_check_head_intercept_keeps_a_later_content_length_alone`, and `REVIEW-2.md`'s
3a/3b. No test writes a `transfer-encoding` with a value other than `chunked`. The contract
paragraph "a `content-length` a LATER stage writes is the reply's only framing header"
(`BEHAVIOR_CONTRACT.md`, the health-check wire-shape bullet) generalises from the OVERWRITE example
and is false under APPEND.

A code reviewer reached A1 independently. With no Docker, it captured envoy-rust's in-process bytes
for A1, the same two rows, and predicted upstream's single row from the G1 measurement. This
session then measured upstream directly.

### Required disposition for the §5.2 STATE-3 re-entry (round 3)

1. **Settle ALL of the H1 headers-only framing last, not only the `HEAD` half.** The wire must
   match every cell above:
   - A1 and B4 → `content-length: 7` alone;
   - B6 → ONE `content-length: 0`;
   - A3 and B7 → `transfer-encoding: chunked` alone;
   - A2 and every `REVIEW-2.md` / `ADR-0203` cell (3a, 3b, G1, base `HEAD`, base `GET`) unchanged.

   One realisation that fits the measurements: `decorate_filter_reply` pushes NO framing header
   for a headers-only reply on either method. `settle_headers_only_framing` then (i) drops every
   `transfer-encoding` a stage wrote; (ii) keeps rule (1), the gRPC-`HEAD` drop; (iii) keeps any
   `content-length` that remains; and (iv) otherwise pushes `transfer-encoding: chunked` for a
   `HEAD` and `content-length: 0` for anything else. The state-3 session makes the design call and
   records it in an ADR that corrects `ADR-0203` FORWARD (append-only). ⚠ Keep the scope where
   `ADR-0203` DECISION 2 put it: do NOT widen the change to other filters' local replies (their
   inline-header join, `content-length: 4,7`, is a different, pre-existing divergence — §5), do not
   fix `CF-115-14`, and do not move the gRPC transform (§5 (a)).
2. **Pin A1 and A3 in-process** (B6 and B7 are welcome riders). Each pin must go RED under a
   mutation that restores the current decode-site `content-length: 0` push or rule (3), run against
   a forced rebuild with an unmutated control. Update the
   `settle_headers_only_framing_never_leaves_both_framing_headers` cell that asserts the
   stage-written TE "stays".
3. **Witness differentially where the driver can express it.** A1's `GET` reply carries
   `content-length: 7` over an empty body. That is the reason `0097` has no `GET` probe, and the
   same reason applies here, so A1's `GET` cell stays in-process unless the driver learns to read
   such a head. B6 is readable (one `content-length: 0`, empty body), but `diff_headers` compares a
   header-NAME set and may not tell one row from two — check the comparator before relying on it.
   A3/B7 are `HEAD` cells and fit the head-only read. The state-3 session chooses; a banked
   carry-forward needs its reason written down.
4. **Correct the record** (§2 R3-1 below, and the contract's settled-last paragraph above).
5. Re-run the full §7.5 gate at the next state 4. A fresh state-5 session writes `REVIEW-4.md`.
   This file, `REVIEW.md` and `REVIEW-2.md` are landed and never edited.

### R3-1 (required record correction) — `REVIEW-2.md` R2-1 was corrected in one place and left LIVE in three

`REVIEW-2.md` R2-1 showed that "upstream leaves the socket OPEN after an H1 `HEAD` intercept" is a
1 s read-timeout artifact. `ADR-0203` says "the driver's comment is corrected in this commit", and
`PROGRESS.md` round 2 says the claim was "corrected in place where the claim is live". Only the
`drive_http1` comment was. Three live statements in `tests/differential/src/lib.rs` still assert
it:

- `pub enum Http1Method`, the `Head` variant doc: "upstream's chunked-framed, **never-closed** HEAD
  reply";
- `fn drive_http1_head_reads_the_head_only`, its doc: "leaves the socket OPEN even under
  `Connection: close` (MEASURED) … an EOF that never comes";
- the same test's mock: "Hold the socket open, as upstream does."

The test itself stays valid on RFC 9110 §9.3.2: a `HEAD` reply has no content, so the driver
must return at the end of the head. Only the stated premise is false. Correct all three, and
record the correction in the round-3 ADR. `ADR-0203` is append-only.

---

## 3. What this session measured or re-derived itself

### 3.1 Round 2's cells are fixed, and the invariant holds

Measured on both proxies with the probe of §2:
- `REVIEW-2.md` 3a `HEAD /healthz` + OVERWRITE `content-length: 7` → `content-length: 7` alone.
- The same cell with the mutation placed BEFORE `health_check` → `content-length: 7` alone. The
  `7aaf296` binary sends TE + CL here, so this cell is fixed as well.
- B6 `HEAD` → `content-length: 0` alone.
- A2 → parity. This also fixes the intercept half of `CF-115-15` (f): a `header_mutation`-added
  `transfer-encoding: chunked` beside `content-length`.
- A pipelined `HEAD /healthz` + `GET /other` on one keep-alive connection → the same two heads on
  both proxies, apart from envoy-rust's pre-existing ADR-0033 `connection: keep-alive`, which is
  not charged.

**No measured envoy-rust reply at HEAD carried both `content-length` and `transfer-encoding`**,
across all 19 intercept replies this session captured.

Fixtures `0095` (twelve probes) and `0097` re-ran GREEN on the reviewed tree
(`cargo test -p differential --test http_filter_health_check` → `1 passed`, 1.27 s;
`--test http_filter_health_check_framing` → `1 passed`, 1.14 s). Those durations are normal for
backend-free fixtures.

### 3.2 The code, read and mutated

A reviewer worked in its own worktree at `a1c1623` with its own target dir. For every mutation it
asserted the target occurred exactly once, forced a rebuild and saw `Compiling envoy-http1`,
redirected the output to a file, and restored the file and confirmed it clean. The unmutated
control was `251 passed; 0 failed`.

| mutation | `test result` | RED tests |
|---|---|---|
| delete the gRPC-`HEAD` `content-length` drop | 249/2 | the G1 end-to-end test; the settle unit test |
| `else if !…any(is_te)` → `else` | 250/1 | the settle unit test |
| settle moved before `pipeline.encode_headers` (the `e569f5b` ordering) | 249/2 | the 3a and G1 end-to-end tests |
| `apply_grpc_local_reply` always returns `false` | 250/1 | the G1 end-to-end test |
| remove the TE drop in the `content-length` branch | 250/1 | the settle unit test |
| remove the decode-side `headers_only_reply = headers_only` | 249/2 | the G1 test; `h1_health_check_head_intercept_is_chunked_and_bodyless` |
| remove the encode-side `headers_only_reply = headers_only` | **251/0** | none — see M3-1 |

Also verified by reading the code:
- Between the decode-side assignment and the settle call, `serve_connection` has no `return`,
  `continue` or `?`.
- The settle call runs after the gRPC transform and before the log and counter derivations and the
  wire write. None of those write a header: `Http1Response::write_to_buf` serialises
  `outgoing.headers` verbatim.
- `%RESP(…)%` is restricted to `x-envoy-upstream-service-time` at load (`RESP_ALLOW_LIST`), so no
  access-log operator can observe the settled framing. `%BYTES_SENT%` is the body length, 0 on
  both proxies.
- `apply_grpc_local_reply` returns `false` at both early exits and `true` only after a transform.
  The io_uring caller ignores the value, and that worker never runs the filter pipeline.
- `req.method == "HEAD"` compares the method after the decode filters. It is case-sensitive, as
  RFC 9110 §9.1 specifies (upstream's `400` on a lower-case method is `CF-115-15` (c)).
- `cargo clippy -p envoy-http1 -p envoy-http2 --all-targets -- -D warnings` and `cargo fmt --check`
  were clean in the reviewer's worktree.

### 3.3 The record, re-derived

A read-only auditor ran `git diff --numstat`, `cmp`, `md5sum` and greps; the results follow.

- Round 2 is net **427** excluding `docs/` (`458 31`, 11 files) over `7aaf296..a1c1623`. The
  whole phase is **2221** (`2308 87`, 31 files) over `04661b7..HEAD`. Commits after `a1c1623`
  touch only `docs/`.
- **+4** test attributes and −0. The `envoy-http1` lib goes from 248 to 251 tests; the
  `tests/differential/tests` directory goes from 95 to 96 files, the +1 binary. So the identity
  2352 + 4 = 2356 over 173 binaries is consistent.
- `0095` has twelve probes (the README table, the expectations file and the test doc agree).
  `0097` has one probe. Both config pairs are `cmp`-identical.
- The `hcm.rs` md5 at `a1c1623` is `1e87dbc3a82b2edf249b4fcbb1ed90a7`, as `ADR-0203` records.
- Every bullet of the contract's health-check section now names a witness, which closes
  `REVIEW-2.md` M2-5.
- `forbid(unsafe_code)` is present in 14/14 crates. The diff adds 0 `allow(`, `expect(`,
  `unsafe`, `ignore`, `todo!` or `dbg!`.

The one claim that does NOT re-derive is the R2-1 correction's completeness (§2 R3-1).

### 3.4 Stop condition — re-derived from disk; ALL THREE LEGS FALSE

- **(i)** `ROADMAP.md`: 123 rows, 122 `done`, 1 `planned`; the not-done set is exactly `{115}` at
  file line 78. Status is field 4 of a `' | '` split. The field-count histogram is
  `{6: 121, 7: 1, 10: 1}`.
- **(ii)** 14 crates; `envoy-{http3,grpc,wasm,protos,runtime,xds}` are absent.
  `quinn`/`wasmtime`/`tonic`/`opentelemetry`/`prost` appear in 0 of the 28 manifests, against
  `tokio` in 19. `histogram` = 0 against `gauge` = 365 over `crates/` and 352 over
  `crates/*/src/`, both counted with `grep -ro … | wc -l`.
- **(iii)** 11 `### ` family headings reading 11/5/3/14/3/4/6/31/6/0/13, plus 27 pre-heading rows,
  sum to 123. `### WASM host family` has zero rows.

No `stop` file exists and none was created.

---

## 4. Minor (banked; the round-3 re-entry MAY take any as a rider in a file it already touches)

- **M3-1 — the encode-side `headers_only_reply = headers_only` assignment is unpinned.** Deleting
  it leaves all 251 `envoy-http1` tests green (§3.2). No production filter replaces a reply on the
  encode side with a headers-only one. A stub-filter unit test would pin it. (This refines
  `REVIEW-2.md` M2-2.)
- **M3-2 — `decorate_filter_reply` is `pub`, and for an H1 headers-only reply its output is now
  incomplete** unless the caller also runs `settle_headers_only_framing`. The doc says so. The H2
  wrapper is unaffected: it passes no connection value and pushes no framing header. Consider
  `pub(crate)`, or a single entry point.
- **M3-3 — two doc comments describe the H1 framing without the settled-last rule.** They are
  `FilterResponse::headers_only` in `crates/envoy-filter/src/types.rs` and the doc of
  `fn intercept_is_a_headers_only_reply` in `crates/envoy-filter/src/health_check.rs`. Both say
  "H1 non-HEAD `content-length: 0`, H1 HEAD `transfer-encoding: chunked`" and do not mention that
  a later stage's `content-length` wins. Incomplete, not false for the default config; I3-1's fix
  will touch the same sentences.
- **M3-4 — the gRPC-`HEAD` drop (settle rule (1)) also discards a `content-length` an encode
  filter wrote.** The value is already lost inside the gRPC transform, which runs after the encode
  pass, so this is a symptom of §5 (a) and not a separate defect. Record it beside that lead.
- **M3-5 — `REVIEW-2.md` §4 M2-6 generalises:** `0097` `p1`'s `expected_body: ""` can never fail
  either, because the driver returns an empty body for every `HEAD`. The README says the header
  set is what discriminates, and V1 proves it does.

---

## 5. Pre-existing divergences measured by THIS session, NOT this phase's → lead list `CF-115-16`

Each of these was measured on both proxies. Each reproduces on a reply that is NOT the
health-check intercept, or identically on the pre-round-2 binary, so none is charged to this
phase.

- **(a) The gRPC local-reply transform runs AFTER the encode-side filter pass; upstream converts
  the local reply first.**
  - Control: a gRPC request to a route-less path (404 NR), with a later OVERWRITE of
    `content-length: 7`. Upstream sends `grpc-status: 12` + `content-length: 7`. Envoy-rust sends
    `grpc-status: 12` + `content-length: 0`, for `GET` and `HEAD` requests, from the HEAD build and
    the `7aaf296` build alike.
  - The health-check twins (A4): a gRPC `GET` intercept → upstream `content-length: 7`,
    envoy-rust `content-length: 0`. A gRPC `HEAD` intercept → upstream `content-length: 7`,
    envoy-rust `transfer-encoding: chunked`. The `HEAD` cell was TE + CL at `7aaf296`.
  - `ADR-0203` DECISION 2 named this ordering and left it unaddressed; it is now MEASURED. The
    transform dates from phase 110.
- **(b) HTTP/1.0 requests.** Upstream answers every cell `426 Upgrade Required` (body
  `Upgrade Required`), for `/healthz` and `/other`, `GET` and `HEAD`. Envoy-rust serves them, and
  frames an HTTP/1.0 `HEAD` intercept `transfer-encoding: chunked`, which HTTP/1.0 does not define.
- **(c) `header_mutation` APPEND onto a reply that already carries the header.** On
  `direct_response` + APPEND `content-length: 7`, upstream sends ONE row `content-length: 4,7`;
  envoy-rust sends two rows, `4` and `7`. This is the phase-07.2 append semantics, not the
  health-check reply's.
- **(d) A stage-written `transfer-encoding` on a non-headers-only reply.** On `HEAD /other` +
  APPEND `transfer-encoding: gzip`, upstream sends `content-length: 4` and no TE. Envoy-rust sends
  `content-length: 4`, `transfer-encoding: gzip`, and the `MAIN` body (`CF-115-14`). This
  RE-MEASURES `CF-115-15` (f) for other replies. Its intercept half is fixed (§3.1).

---

## 6. Earlier findings — disposition

| item | disposition |
|---|---|
| `REVIEW-2.md` I2-1 (3a, G1) | **FIXED** at `a1c1623`; parity measured on both proxies (§3.1); the pins RED under the `e569f5b` ordering (§3.2). The same mechanism on the non-`HEAD` arm opens I3-1 |
| `REVIEW-2.md` R2-1 | **PARTLY** — the contract, `0095`'s README, `ADR-0203` and the `drive_http1` comment are corrected; three live test-code statements are not (R3-1) |
| `REVIEW-2.md` M2-5 | **FIXED** — every bullet names its witness (§3.3) |
| `REVIEW-2.md` M2-1 … M2-4, M2-6 | banked, unchanged (M2-2 refined by M3-1; M2-6 extended by M3-5) |
| `REVIEW-2.md` M2-7 | stands; `REVIEW.md` is not edited |
| `REVIEW.md` I-1 … I-3 | stay FIXED (H2 untouched this round; H2 A1/A3 at parity) |
| `CF-115-15` (f) | **intercept half FIXED** by `a1c1623` (A2); the other-reply half stands and is re-measured as `CF-115-16` (d) |

## 7. Carry-forwards

| id | status | what |
|---|---|---|
| `CF-115-1` … `-3`, `-5` … `-13` | unchanged | as recorded in `PLAN.md` / `REVIEW.md` §7 |
| `CF-115-4` | unchanged | no H2 differential witness; the H2 cells of this round are at parity |
| `CF-115-14` | unchanged | envoy-rust writes a body after a non-intercepted H1 `HEAD` reply; still NOT fixed |
| `CF-115-15` | **(f) narrowed** | the intercept half of (f) is fixed; (a)–(e), (g), (h) unchanged |
| `CF-115-16` | **NEW (measured, pre-existing)** | §5 (a)–(d); (a) carries M3-4 |

`CF-114-6` stays CONSUMED. `CF-75-5` and the phase-112 ALPN rider stand and are not re-costed.

---

## 8. Assessment

**Ready to close? NO — re-enter §5.2 STATE 3.** The settled-last design is right, and it fixes
every cell round 2 named. But it settles only the `HEAD` half of the framing. The non-`HEAD`
`content-length: 0` is still chosen before the encode pass. Under `APPEND_IF_EXISTS_OR_ADD`,
upstream's default `header_mutation` action, that yields two conflicting `content-length` rows
where upstream sends one. A stage-written `transfer-encoding` value also survives on a `HEAD`,
where upstream's codec writes `chunked`. The fix is narrow: settle the non-`HEAD` half last too,
and let the codec own `transfer-encoding`.

| §7.5 leg | status at this review |
|---|---|
| (a) new fixtures green | PASS on the reviewed tree (`c352b57`; `0095` and `0097` re-run green here) — must be re-run after the fix |
| (b) pre-existing fixtures green | PASS on the reviewed tree (CI `35755023684` `failed=0`) — must be re-run |
| (c) conformance | PASS, CI-authoritative — must be re-run |
| (d) fuzz | PASS (no new target) — must be re-run |
| (e) build / clippy / fmt / test / deny | PASS on the reviewed tree — must be re-run |
| (f) review approved | **NOT APPROVED** (this file) |

### How this review was conducted

This is a fresh context. State 3 implemented the fix and state 4 graded it, and neither may review
it (§5.1, `ADR-0127`).

**Who did what.**
- This session ran the two-proxy composition probe itself: 26 H1 requests over 11 configs and 4
  H2 requests over 2 configs, each against the pinned upstream, the HEAD `envoy-bin` and the
  `7aaf296` `envoy-bin` (built in its own worktree).
- Two read-only reviewers ran in parallel, each in its own scratch worktree with its own
  `CARGO_TARGET_DIR`: (1) the `a1c1623` code diff with targeted mutations, and (2) the record,
  contract and fixtures, re-derived.
- **Every finding charged in §2 was reproduced by this session on both proxies** before it was
  written down. R3-1's three sites were read by this session in the tree.

**Hygiene.** The session's containers carried an `r3p-` prefix and were all removed; no other
container was touched. Every worktree was removed, and the main tree stayed clean.

**One collision, with its cause established.** This session's probe script and the code
reviewer's throwaway-test inserter had the same file name in the shared scratchpad. One run of the
probe therefore executed the reviewer's inserter and re-inserted a probe test into the reviewer's
worktree. The reviewer saw the test reappear, restored the file (md5 `1e87dbc3…`, equal to
`a1c1623`), and re-ran clippy and fmt clean. Its mutation runs all reported `running 251 tests`,
which excludes the inserted test, so no result above is affected.

### Next state

**§5.2 STATE 3 — the round-3 re-entry, implementing §2's required disposition**, in a SEPARATE
session (§5.1; `ADR-0127`), with TDD per fix and entries appended to `PROGRESS.md`. Then state 4
(the full gate), then a fresh state-5 review writing `REVIEW-4.md`. `ROADMAP.md` row `115` stays
`planned` throughout.
