# Phase 31 — `31-http-filter-cdn-loop` — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development` to
> implement this plan task-by-task (a fresh implementer subagent per task, SERIAL, two-stage
> reviewed). Steps use checkbox (`- [ ]`) syntax. This PLAN is the state-2 output; scope locked by
> **ADR-0076**, the §6.2 wire facts locked by **ADR-0077**.

**Goal:** Implement Envoy's `envoy.filters.http.cdn_loop` HTTP filter (RFC 8586 `CDN-Loop`: parse →
count `cdn_id` → reject-on-loop [502] / reject-malformed [400] / else append `cdn_id` and forward),
behaviorally byte-equivalent to upstream Envoy v1.33.0 under the differential contract.

**Architecture:** A new self-contained RFC 8586 `CDN-Loop` parser (`crates/envoy-filter/src/cdn_loop.rs`)
+ a `CdnLoopFilter` (the 9th `HttpFilterInstance` variant) that runs decode-side in the existing
H1/H2 filter pipeline. Config (`CdnLoopConfig`) joins the existing `@type`-tagged HTTP-filter enum.
No new pipeline machinery; inert when not configured (all 38 existing fixtures stay green).

**Tech Stack:** Rust; `crates/envoy-config` (serde config + validators), `crates/envoy-filter` (the
filter + parser), `tests/fixtures/0039-http-filter-cdn-loop` (differential), `tests/differential`
(the `http1_probe_list` driver — reused), the `parse_bootstrap` + a new `cdn_loop_parse` fuzz target.

---

## §A — §6.2-LOCKED facts (ADR-0077; empirically recorded against live `envoyproxy/envoy:v1.33.0`)

The §6.2 reconnaissance stood up Envoy v1.33.0 with an H1 listener (`cdn_loop` filter:
`cdn_id: "mycdn.example"`, `max_allowed_occurrences: 0` → router) + a header-reflecting echo backend.
**All four headline SPEC projections CONFIRMED** (loop→502, malformed→400, append bare `cdn_id`,
no stat). The exact wire truth the implementation MUST reproduce:

1. **Append (within limit) — COMMA-ONLY, NO SPACE.** No `CDN-Loop` present → the backend receives
   `CDN-Loop: mycdn.example` (the bare `cdn_id`). One foreign entry present → the backend receives
   `CDN-Loop: othercdn.example,mycdn.example` — **comma-only, NO space**. (⚠ the SPEC §1.3 example
   wrote `<foreign-id>, <cdn_id>` with a space — that example is WRONG; ADR-0077 corrects it to
   comma-only. The append is: if the header exists, `"{existing},{cdn_id}"`; else set `"{cdn_id}"`.)
2. **Loop reject — 502.** `count(cdn_id) > max_allowed_occurrences` → **502 Bad Gateway**, body =
   `The server has detected a loop between CDNs.` (**44 bytes, NO trailing newline**),
   `content-type: text/plain`, `content-length: 44`, `server: envoy`. Response-flags = `-` (none);
   `%RESPONSE_CODE_DETAILS%` = `cdn_loop_detected`. NO `x-envoy-upstream-service-time`.
3. **Malformed reject — 400.** A malformed `CDN-Loop` header → **400 Bad Request**, body =
   `Invalid CDN-Loop header in request.` (**35 bytes, NO trailing newline**), `content-type: text/plain`,
   `content-length: 35`, `connection: close`, `server: envoy`. `%RESPONSE_CODE_DETAILS%` =
   `invalid_cdn_loop_header`.
4. **Parser grammar (strict RFC 7230 + RFC 8586):** the header value is a comma-separated list of
   `cdn-info`; each `cdn-info` = a `cdn-id` optionally followed by `;`-separated `parameter`s.
   - A `cdn-id` MUST be a bare RFC-7230 `token` (tchars). A quoted-string as the id → **MALFORMED
     (400)** (even a well-formed `"mycdn.example"` → 400, NOT a loop). A non-tchar in the id (space,
     `/`, `@`, tab) → **400**.
   - A `parameter` is `name=value` (`value` = token or quoted-string). A bare parameter without
     `=value` (e.g. `a;b`) → **400**. A quoted-string parameter value (`a; b="c"`) → OK (200).
   - **Empty entries are NOT malformed:** `a,,b`, a trailing comma `a,`, a leading comma `,a`, `,,,`
     → all parse to zero/positive matches and **append (200)** (the empty entries are preserved
     verbatim on append, e.g. `a,` → `a,,mycdn.example`). Only a structurally-bad `cdn-info` (bad id
     / bad param) makes the header malformed.
   - **OWS is trimmed** around list entries: `  othercdn.example  ` → matches/appends as
     `othercdn.example` (the appended result is `othercdn.example,mycdn.example`).
   - **Matching is CASE-SENSITIVE** on the `cdn-id` token (`MYCDN.EXAMPLE` ≠ `mycdn.example` → 200
     append, no loop). Parameters are IGNORED for matching (`mycdn.example; foo=bar` still counts as
     a `mycdn.example` match → 502).
   - **Multiple `CDN-Loop` request headers** are coalesced into one comma-joined list before
     counting (`CDN-Loop: a` + `CDN-Loop: mycdn.example` → loop detected → 502).
5. **Config validity — ALL BOOT-FATAL (ADR-0049 all-fatal):**
   - empty `cdn_id: ""` → fatal (PGV `string.min_len = 1`).
   - `cdn_id` containing a comma (`"a,b"`) → fatal (`Provided cdn_id "a,b" is not a valid CDN
     identifier`).
   - `cdn_id` with an invalid token char (`"a b"`, `"a@b"`) → fatal (same "not a valid CDN
     identifier" family). → envoy-rust models these as startup-fatal `ConfigError` variants (a
     valid `cdn_id` is a non-empty bare RFC-7230 token).
6. **Stats — NONE.** The filter emits no dedicated stat (effects show only in the generic HCM
   `downstream_rq_{2xx,4xx,5xx}`). Emit NO cdn_loop stat (the phase-21/24/28/29/30 no-stat discipline).
7. **Wire shape (`/config_dump`):** `@type` =
   `type.googleapis.com/envoy.extensions.filters.http.cdn_loop.v3.CdnLoopConfig`; fields `cdn_id`
   (string) + `max_allowed_occurrences` (uint32, default 0, omitted-when-zero in the dump).

**§6.1 split:** NOT fired — ~6–8 tasks / ~600–900 LoC, comparable to csrf/buffer, well under the
gate. ADR-0078 reserved-but-UNFIRED. **ADR-0077 LOCKS the above** (the headline shape matched the
SPEC; the one SPEC example error — comma-space → comma-only — is corrected here, plus the precise
reject bodies/details, the case-sensitive parameter-ignoring match, the strict token grammar, the
empty-entry-is-not-malformed boundary, the multi-header coalescing, and the all-fatal config
validity are recorded).

---

## §B — File structure

- **Create** `crates/envoy-filter/src/cdn_loop.rs` — the RFC 8586 `CDN-Loop` parser (`parse_cdn_loop`
  → `Result<Vec<CdnInfo>, Malformed>`; `count_cdn_id`) + the `CdnLoopFilter` struct + its decode
  logic. The parser is the correctness gate (pinned unit oracle).
- **Modify** `crates/envoy-filter/src/lib.rs` — `mod cdn_loop;`.
- **Modify** `crates/envoy-filter/src/instance.rs` — the `CdnLoop` `HttpFilterInstance` variant + the
  `build()` dispatch from `HttpFilterTypedConfig::CdnLoop`.
- **Modify** `crates/envoy-config/src/bootstrap.rs` — `CdnLoopConfig { cdn_id, max_allowed_occurrences }`
  + the `CdnLoop` arm in the `@type`-tagged HTTP-filter config enum + the `cdn_id` validator.
- **Modify** `crates/envoy-config/src/lib.rs` — `ConfigError::CdnLoopEmptyCdnId` /
  `ConfigError::CdnLoopInvalidCdnId`.
- **Create** `tests/fixtures/0039-http-filter-cdn-loop/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
  — the differential (the 0013-header-mutation + 0032-csrf shape; `http1_probe_list` driver).
- **Create** `tests/differential/tests/http_filter_cdn_loop.rs` — the per-fixture entry (clone of
  `http_filter_csrf.rs`).
- **Create** `crates/envoy-filter/fuzz/fuzz_targets/cdn_loop_parse.rs` (+ register in
  `crates/envoy-filter/fuzz/Cargo.toml`) — a dedicated fuzz target over `parse_cdn_loop` (the
  untrusted-request-input parse surface; the §7.4 / jwt_parse precedent). [If `crates/envoy-filter`
  has no `fuzz/` yet, the implementer adds the minimal `cargo fuzz` scaffold mirroring
  `crates/envoy-jwt/fuzz/`.]
- **Modify** `crates/envoy-config/fuzz/corpus/parse_bootstrap/` — add a `http_filter_cdn_loop.yaml`
  seed + register in `fuzz_corpus_seeds_parse_or_reject_cleanly` + `.gitignore`.
- **Modify** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — an "HTTP filters" cdn_loop subsection.

---

## Task 1: the RFC 8586 `CDN-Loop` parser (the correctness gate)

**Files:** Create `crates/envoy-filter/src/cdn_loop.rs`; Modify `crates/envoy-filter/src/lib.rs`.

- [ ] **Step 1: Write failing tests** — a pinned oracle in `cdn_loop.rs` `#[cfg(test)]` encoding the §A.4
  grammar: `count_cdn_id("mycdn.example", b"") == 0`; bare/foreign append counts; case-sensitivity
  (`MYCDN.EXAMPLE` ≠ `mycdn.example`); parameter-ignoring match (`mycdn.example; foo=bar` counts);
  malformed cases → `Err` (quoted-string id `"abc"`, non-tchar id `a b`/`a@b`, bare param `a;b`,
  unterminated quote `"abc`); NOT-malformed (`a,,b`, `a,`, `,a`, `,,,`, OWS `  a  `); multi-value
  coalescing via a `&[&[u8]]` (multi-header) input. Cover the `max_allowed_occurrences > 0` count
  boundary at the call site (Task 3) — here just the count.
- [ ] **Step 2: Run → FAIL** (`cargo test -p envoy-filter cdn_loop`; expected: unresolved `parse_cdn_loop`).
- [ ] **Step 3: Implement** `parse_cdn_loop(values: &[&[u8]]) -> Result<Vec<CdnInfo>, MalformedCdnLoop>`
  (split each header value on `,`, OWS-trim each entry, parse `cdn-id` [tchar+] + optional
  `;`-parameters [name=value], reject non-token id / quoted-string id / bare param, KEEP empty
  entries as zero-id placeholders) + `count_cdn_id(cdn_id, parsed) -> usize` (case-sensitive token
  equality, ignoring parameters). NO `unsafe`. Keep the parser a pure function over bytes.
- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** `phase 31: Task 1 — RFC 8586 CDN-Loop parser (§A oracle) [ADR-0077]`.

## Task 2: config schema + validation (`CdnLoopConfig` + the `@type` variant)

**Files:** Modify `crates/envoy-config/src/bootstrap.rs`, `crates/envoy-config/src/lib.rs`.

- [ ] **Step 1: Write failing `parse_bootstrap` tests** — a cluster/HCM with the `cdn_loop` filter
  parses to `CdnLoopConfig { cdn_id: "mycdn.example", max_allowed_occurrences: 0 }`; the `@type`
  `...cdn_loop.v3.CdnLoopConfig` selects the `CdnLoop` variant; `max_allowed_occurrences` absent →
  default 0; `deny_unknown_fields` rejects an unknown field; an empty `cdn_id` → `CdnLoopEmptyCdnId`;
  a comma/invalid-token `cdn_id` (`"a,b"`, `"a b"`) → `CdnLoopInvalidCdnId`.
- [ ] **Step 2: Run → FAIL.**
- [ ] **Step 3: Implement** `CdnLoopConfig` (serde, `deny_unknown_fields`, `max_allowed_occurrences`
  default via `#[serde(default)]`) + the `CdnLoop(CdnLoopConfig)` arm in the `@type`-tagged
  HTTP-filter enum + a `validate` path: `cdn_id` non-empty AND every byte a valid RFC-7230 tchar
  (the §A.4 token rule — reuse/extend the parser's tchar predicate via a small shared helper or
  duplicate the tchar set with a comment) → the two `ConfigError` variants. All-fatal.
- [ ] **Step 4: Run → PASS** (`cargo test -p envoy-config`).
- [ ] **Step 5: Commit** `phase 31: Task 2 — CdnLoopConfig schema + cdn_id validation (all-fatal) [ADR-0077]`.

## Task 3: `CdnLoopFilter` + the `HttpFilterInstance` variant (decode-side count/append/reject)

**Files:** Modify `crates/envoy-filter/src/cdn_loop.rs`, `crates/envoy-filter/src/instance.rs`.

- [ ] **Step 1: Write failing tests** (in-process, in `instance.rs`/`cdn_loop.rs`): build a
  `CdnLoopFilter` from a `CdnLoopConfig`; drive a synthetic decode with the §A probes — no header →
  `Decision::Continue` AND the request now carries `cdn-loop: mycdn.example`; foreign id → Continue +
  `cdn-loop: othercdn.example,mycdn.example` (comma-only); self id → `Decision::StopAndSend` 502 +
  body `The server has detected a loop between CDNs.`; malformed → StopAndSend 400 +
  `Invalid CDN-Loop header in request.`; the `max_allowed_occurrences: 1` boundary (one self entry →
  Continue+append, two → 502).
- [ ] **Step 2: Run → FAIL.**
- [ ] **Step 3: Implement** the decode path: read all `cdn-loop` request-header values → `parse_cdn_loop`;
  `Err` → `StopAndSend(400, "Invalid CDN-Loop header in request.")`; else `count_cdn_id` >
  `max_allowed_occurrences` → `StopAndSend(502, "The server has detected a loop between CDNs.")`; else
  append `cdn_id` to the `cdn-loop` request header (comma-only join; set if absent) and `Continue`.
  Encode path inert. Use the existing H1/phase-11-H2 filter-synth local-reply decorators
  (`text/plain`, exact bodies, no trailing newline). Register the `CdnLoop` variant + `build()`
  dispatch. (The pipeline is codec-agnostic → H1 + H2 both covered, the csrf/buffer precedent.)
- [ ] **Step 4: Run → PASS** (`cargo test -p envoy-filter`).
- [ ] **Step 5: Commit** `phase 31: Task 3 — CdnLoopFilter (9th HttpFilterInstance variant) [ADR-0077]`.

## Task 4: fixture `0039-http-filter-cdn-loop` (the STRONG differential)

**Files:** Create `tests/fixtures/0039-http-filter-cdn-loop/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`,
`tests/differential/tests/http_filter_cdn_loop.rs`.

- [ ] **Step 1: Author the fixture** — clone the **0013-header-mutation** shape (it observes
  request-header mutations via the echo backend) for the append probes + the **0032-csrf** shape for
  the reject probes. An H1 listener whose HCM chain is `cdn_loop` (`cdn_id: "mycdn.example"`,
  `max_allowed_occurrences: 0`) → router; one `http1-echo-server` backend (reflects the received
  `cdn-loop` request header in its sorted echo body). `envoy.yaml` + `envoy-rust.yaml` (identical
  modulo the per-side bind address); `expectations.yaml` with `driver.kind: http1_probe_list`:
  - P1 no-header → 200, echo body shows `cdn-loop: mycdn.example`.
  - P2 `CDN-Loop: mycdn.example` → 502, body `byte_exact: The server has detected a loop between CDNs.`.
  - P3 `CDN-Loop: othercdn.example` → 200, echo body shows `cdn-loop: othercdn.example,mycdn.example`
    (comma-only).
  - P4 malformed `CDN-Loop: "abc` → 400, body `byte_exact: Invalid CDN-Loop header in request.`.
  - P5 (near-malformed boundary — the spec-reviewer's note) `CDN-Loop: othercdn.example,` (trailing
    comma) → 200, echo body shows `cdn-loop: othercdn.example,,mycdn.example` (empty entry preserved).
  Use `expected_headers: set_equal_modulo_allow_list` (the csrf/buffer pattern). The append probes
  observe the appended `cdn-loop` via the **0013-header-mutation echo-body mechanism**: the echo
  server reflects the request headers as alphabetically-SORTED `name: value` lines, so the appended
  `cdn-loop` line is deterministic and a whole-body `byte_exact` echo assertion works (0013's proven
  approach) — assert the echo body shows the exact `cdn-loop` line. `http_filter_cdn_loop.rs` clones
  `http_filter_csrf.rs`. **NOTE (the reject probes' `connection` header):** every differential
  probe driver sends `Connection: close`, and the shared `decorate_filter_synth_response` keys the
  reply's `connection` header off the per-request close flag (NOT the status) — so BOTH the 502 and
  the 400 reject probes carry `connection: close` on BOTH proxies under the close-driver (the §A.2
  "no `connection` on 502" is a keep-alive-recon artifact; `connection` is NOT in the header
  allow-list, so it is value-compared and resolves clean on both sides, exactly as the 0032 csrf-403
  reject already does). Do not try to suppress `connection` on the 502 to match the bare §A.2 list.
- [ ] **Step 2: Run the differential LOCALLY** (`cargo test -p differential cdn_loop` / the harness
  target) against `envoyproxy/envoy:v1.33.0` — expected: all 5 probes cross-proxy identical.
- [ ] **Step 3: Confirm** the run is GREEN; spot-check fixtures 0013/0032 still pass.
- [ ] **Step 4: Commit** `phase 31: Task 4 — fixture 0039-http-filter-cdn-loop differential (STRONG) [ADR-0077]`.

## Task 5: in-process backstop (the no-op witness + the parser edges)

**Files:** Modify `crates/envoy-filter/src/cdn_loop.rs` (or `instance.rs`) tests.

- [ ] **Step 1: Write tests** — the §A.4 edge matrix not covered by the fixture (multi-`CDN-Loop`-header
  coalescing → 502; case-sensitivity; param-ignoring match + param-preserving append; the
  empty-entry-not-malformed vs malformed-id boundary; OWS trim; the `max_allowed_occurrences > 0`
  boundary) + the **inert no-op witness**: a filter chain WITHOUT `cdn_loop` leaves a `CDN-Loop`
  request header untouched and never 400/502s (the 38-fixtures-stay-green proof, in-process).
- [ ] **Step 2: Run → (write-then-pass per edge).**
- [ ] **Step 3: Commit** `phase 31: Task 5 — cdn_loop backstop (parser edges + no-op witness)`.

## Task 6: `parse_bootstrap` seed + a dedicated `cdn_loop_parse` fuzz target

**Files:** Create `crates/envoy-filter/fuzz/fuzz_targets/cdn_loop_parse.rs` (+ `fuzz/Cargo.toml`);
Modify `crates/envoy-config/fuzz/corpus/parse_bootstrap/` + `fuzz_corpus_seeds_parse_or_reject_cleanly`
+ `.gitignore`.

- [ ] **Step 1:** Add `http_filter_cdn_loop.yaml` to the `parse_bootstrap` corpus (an HCM with the
  cdn_loop filter) + register it (the phase-30 Task-9 pattern). Verify via
  `fuzz_corpus_seeds_parse_or_reject_cleanly`.
- [ ] **Step 2:** Add the `cdn_loop_parse` fuzz target over `parse_cdn_loop` (mirror
  `crates/envoy-jwt/fuzz/` scaffold; the parser must never panic on arbitrary bytes — it returns
  `Ok`/`Err`). (cargo-fuzz runs at the state-4 CI gate; locally just `cargo build` the target.)
- [ ] **Step 3: Commit** `phase 31: Task 6 — parse_bootstrap seed + cdn_loop_parse fuzz target (§7.4)`.

## Task 7: BEHAVIOR_CONTRACT subset + state-3 close-out

**Files:** Modify `docs/envoy-rust/BEHAVIOR_CONTRACT.md`; finalize `PROGRESS.md`.

- [ ] **Step 1:** Add an "HTTP filters" `cdn_loop` subsection to BEHAVIOR_CONTRACT (the §A facts:
  parse → count → reject-502 [`cdn_loop_detected`] / reject-400 [`invalid_cdn_loop_header`] / append
  comma-only; case-sensitive param-ignoring match; all-fatal config validity; no stat; fixture 0039;
  deferred non-goals).
- [ ] **Step 2:** Run `cargo fmt --all -- --check` clean LOCALLY (the `envoy-rust-state4-ci-first-execution`
  discipline — pre-empt the mid-phase fmt red).
- [ ] **Step 3:** Finalize `PROGRESS.md` (per-task SHAs + review dispositions); advance STATE to
  state-3-complete/state-4-next (the next session = state-4 verification). Commit
  `phase 31: Task 7 — BEHAVIOR_CONTRACT cdn_loop row + state-3 close [ADR-0077]`.

---

## §C — Process notes

- **TDD per task; SERIAL implementer subagents** (never parallel — they race on `main`); two-stage
  review (spec-compliance THEN code-quality via fresh `superpowers:code-reviewer` subagents) per task.
- **Load-bearing invariant:** all 38 pre-existing fixtures `0001`–`0038` stay green (the filter is
  inert when not in the chain — Task 5's no-op witness + the state-4 differential suite prove it).
- **Carry-forwards (NOT consumed — cdn_loop does not touch the LB hash-sweep driver):** the
  empty-`metadata_match` doc-comment; M29-1/M29-2 + M30-1 (the `Http1HashSweep` driver wording /
  `extract_marker`); M30-2 (`lb_policy` serde-default). All carry forward.
- **§7.5 gate (previewed):** (a) fixture 0039 green + (b) 0001–0038 green + (c) h2spec ≥95% + (d)
  `parse_bootstrap` + `cdn_loop_parse` fuzz clean + (e) build/clippy/fmt/test/deny clean + (f)
  REVIEW.md approved. `#![forbid(unsafe_code)]` holds.

_Scope locked by **ADR-0076**; §6.2 wire facts locked by **ADR-0077**. ADR-0078 (§6.1 split)
reserved-but-UNFIRED. The state-3 implementation (`superpowers:subagent-driven-development`) is the
next session._
