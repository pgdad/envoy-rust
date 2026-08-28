# Phase 111 — gRPC-family PREREQUISITE: HTTP/2 response TRAILER forwarding (upstream → downstream) — CODE REVIEW

**Verdict: APPROVED-WITH-MINORS.**

Per `BOOTSTRAP_PROMPT.md` §7.5, an approved `REVIEW.md` closes gate **(f)** — the only gate still
open. Gates (a)–(e) were run and adjudicated by the §5 state-4 verification session and are
recorded, with actual command outputs, in `PROGRESS.md`. **This review did not re-run them and does
not re-adjudicate them** (§5.1; ADR-0127 — the context that ran the gate must not grade it, and the
context that grades it must not fix it). It re-confirmed CI on the exact tree under review
independently (§0.3), because that is a fact about the commits rather than a re-run of the gate.

**Zero Issues. Fifteen Minors, thirteen Notes.** Not one finding is a wire-behaviour defect on any
path a fixture, a gRPC client or a proxied response will take. The production seam is the strongest
part of the phase: the three-way end-of-stream fork falls out of three expressions, the no-trailers
path is byte-identical to the pre-phase code *by construction rather than by assertion*, the
`H2SendTrailers` widening is provably unable to reach an abort anywhere in the workspace, and the
`StopAndSend` clear sits on top of a threading design that the compiler enforces at five of its six
sites.

The findings cluster into three shapes, and each shape recurs.

**First, an invariant the code states and does not hold.** `crates/envoy-http2/src/lib.rs:46` and
`PLAN.md` D-PLAN-2 both assert "`Some(vec![])` is never produced". The landed read site produces it
on two reachable inputs. Two live production branches read `trailer_map.is_none()`, so the
consequence reaches the wire as an extra zero-field HEADERS frame. **M-1**, and the one-line fix is
named.

**Second, an assertion that is named but not made.** The empty-body-with-trailers row — the gRPC
main case, the reason this prerequisite exists — is covered by a test whose name says
`..._with_no_data_frame` and whose body asserts only that the received *bytes* are empty. Deleting
the `if !body_empty` guard at the emit seam survives the entire unit suite and both differential
sweeps. **M-2** — and **M-14** is the same gap seen from the other side: Task 1 carries the phase's
riskiest change and its recorded RED is a compile error, with no mutation ever aimed at the fork's
discriminating term.

**Third, a coverage commitment that was made and not met.** `PLAN.md` D-PLAN-6 excludes three cells
from fixture `0090` on the explicit ground that they "are covered by unit tests at the emit seam
instead". One of the three is. Non-200-with-trailers and five-trailer blocks have no test anywhere
in the tree, `tests/fixtures/0090-h2-response-trailers/README.md` repeats the claim verbatim, and
`PROGRESS.md` does not disclose the gap. **M-3.**

Seven findings arrived from subagents graded **Important**; all seven are **DOWNGRADED** here after
re-derivation on disk. Grading them honestly matters: §5.2 sends any Issue back to **state 3**, not
state 4, and none of these seven warrants three more sessions. §5 records each dissent with its
reasoning, along with one subagent claim dropped entirely, two subagent citations that do not
resolve, and one finding this reviewer raised against the state-4 record and then withdrew.

The review's single most useful *new* output is **M-7**: this phase stale-dated a citation in a
**sibling section of `BEHAVIOR_CONTRACT.md`** — `:796`'s `tests/differential/src/lib.rs:1189-1193`
is now `:1212-1216`, moved by the phase's own insertions into `lib.rs`. Three drifted citations were
banked for this review; this is a **fourth**, and it is the interesting one, because it is in a file
the phase edited but in a section the phase did not write. `110.2` M-1 named the
self-invalidating-citation hazard for a document invalidating *its own* citation. This is the next
case out: a document invalidating a *neighbour's*.

Per §6.3 and ADR-0165 **nothing was fixed by this session**. **No §5.2 re-entry to state 3 is
required** — the verdict is an approval and gate (f) is CLOSED; every Minor and Note below is
BANKED for the state-6 close-out to carry.

---

## §0 — How this review was conducted

### §0.1 — Scope

The tree under review is `main` at `231aba8de521596ba60dbf75b2cd09fcda40a316`, clean
(`git status --porcelain` = 0 lines), with `git fetch origin --prune` run at session start and its
**own** exit code checked (0) rather than a pipeline's.

The **graded range is `0ba60db 6a790ab`**, not a range ending at `HEAD`. `HEAD` has since moved by
two docs-only state commits (`d4ebda2`, the state-4 advance; `231aba8`, its CI record), and a range
ending at `HEAD` drifts as each lands. Within that range:

| | |
|---|---|
| ten task commits | `d94b3c0` → `a2c5589`, one per `PLAN.md` task |
| state-3 advance | `111b34a212675d332a506536dc090570da2f3b63` |
| state-3 CI record | `6a790abc0c59f0384ef561a6d4177faefbd50d1d` (docs-only) |
| files changed excluding `docs/` | **13** |
| net LoC excluding `docs/` | **1525** (added 1559, deleted 34) — re-derived, not inherited |
| added test attributes | **24** added, **0** removed (`#[test]` 8 + `#[tokio::test]` 7 + `#[tokio::test(flavor = "multi_thread")]` 9) |

The 13 files: `crates/envoy-http2/src/{client,error,hcm,lib,response}.rs`;
`tests/differential/src/{backend,lib}.rs`; `tests/differential/tests/h2_response_trailers.rs`; the
four files of `tests/fixtures/0090-h2-response-trailers/`;
`tests/helpers/http2-echo-server/src/main.rs`. Plus `docs/envoy-rust/BEHAVIOR_CONTRACT.md`'s new
`## Response trailers` section.

`crates/envoy-filter/` is **untouched** — `git diff --name-only 0ba60db 6a790ab --
crates/envoy-filter/` returns **0** files. SPEC D6 and non-goal 3 hold. The phase introduces **zero**
`TODO`/`FIXME`/`XXX`/`unimplemented!`/`todo!` markers (§6.3 anti-pattern check: `git diff … |
grep -cE '^\+.*(TODO|FIXME|XXX|unimplemented!|todo!)'` = **0**), and
`#![forbid(unsafe_code)]` is intact at `crates/envoy-http2/src/lib.rs:1` (D-3.8).

### §0.2 — Method

Six read-only reviewers were dispatched in parallel over an orthogonal partition of the surface —
(i) the `envoy-http2` production path, (ii) the differential harness, (iii) the test helper and
backend, (iv) fixture `0090`, (v) the `BEHAVIOR_CONTRACT.md` section, (vi) artifact consistency
(SPEC ↔ PLAN ↔ PROGRESS, the CF ledger, every `file:line`). Each received the SPEC's D1–D8 and the
PLAN's D-PLAN-1…D-PLAN-8 verbatim, the SPEC §5 non-goals, and the four standing REJECTIONS, and each
was instructed **not to run `cargo`** (the lock serialises, and re-running the gate is state 4's
work, already done) while being told that `~/.cargo/registry/src/` is readable evidence. None was
permitted to spawn further subagents.

**Every subagent finding in this document was re-verified on disk by the main session before being
recorded, and the ones that did not survive were dropped or downgraded** (§5). Where a finding
turned on the behaviour of a pinned dependency, it was re-traced through
`~/.cargo/registry/src/index.crates.io-*/h2-0.4.16/` — the version `Cargo.lock` actually pins, which
is **not** the only `h2` in this host's registry cache (`0.4.13`, `0.4.15`, `0.4.16`, `0.4.19` are
all present, and a `ls | head -1` picks the wrong one).

### §0.3 — CI re-confirmed independently on the exact tree under review

This is a fact about the commits, not a re-run of the gate. The **code** tree under review is
`111b34a2`; `git diff --name-only 111b34a2 6a790ab -- . ':(exclude)docs/'` is empty, which is what
makes CI on `111b34a2` authoritative for every non-docs byte this review grades.

- `gh run list --commit "$(git rev-parse 111b34a2…)"` (full 40-char SHA — a short or retyped SHA
  returns `[]`) → run **33006543581**, `push`, `completed`, conclusion **`success`**, **attempt 1**
  (no rerun needed).
- **Both** jobs enumerated via `gh api repos/pgdad/envoy-rust/actions/runs/33006543581/jobs`, because
  `gh run view --log` returns only one and a fuzz-only failure would be invisible:
  `build + test + lint` **15/15 steps success, 0 non-success**;
  `fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse + grpc_health_decode)`
  **13/13 steps success, 0 non-success**. Both carry a real `runner_name` — an empty one with
  `steps:0` would be runner STARVATION, not a result.
- Job log fetched from `gh api repos/pgdad/envoy-rust/actions/jobs/98301653236/logs` (the
  `…/actions/runs/<run>/jobs/<job>/logs` spelling 404s with a 131-byte body over which every
  `grep -c` returns a believable ZERO): **417 582 bytes**, asserted before any count was taken.
- **Identity: `binaries=167 passed=2252 failed=0`.** Derived by matching `(ok|FAILED)` rather than
  `ok` alone — a pattern that matches only `ok` discards the `FAILED.` lines before `awk` sees them
  and makes its `failed=0` **true by construction**. The awk field numbers were derived from a real
  matched line printed field by field rather than assumed: `$4` passed, `$6` failed. Cross-checked
  independently of the awk: **167 `test result: ok.` lines, 0 `test result: FAILED.` lines,
  0 `failures:` blocks**, and **167 libtest `running N tests` headers** — one per binary.
- **All 90 differential fixture binaries carry a `Running tests/<name>.rs (target/debug/deps/…)`
  line: 90 found, 0 missing**, driven from `ls tests/differential/tests/*.rs` rather than from a
  hand-written list. ⚠ **This census must be taken ANSI-stripped.** The `Running` token in a GitHub
  job log is colour-coded — the raw bytes are `\e[1m\e[92m     Running\e[0m tests/…` — so a naive
  `grep -c 'Running '` returns **0**, a clean and entirely believable zero. This review hit that
  zero, and it is the same shape as the `cargo deny` four-ok line the ledger already records. The
  state-4 record's `Running`-line claim is **CORRECT**; it was this reviewer's grep that was wrong.
- **All 90 fixture test functions report `test <name> ... ok`**, resolved by extracting each runner's
  `fn` name from the tree — 41 of the 90 have a test-function name that differs from their file name,
  so a census keyed on either one alone under-reads. **`h2_response_trailers` is among them**, so
  gate (a)'s witness genuinely executed in CI.
- `h2spec not found` occurs **0** times over the 417 KB log, so gate (c) genuinely ran there
  (ADR-0163; the local green is demonstrably vacuous on this host and was not relied on).
- `tests/conformance/h2spec/known-failures.txt`: **21 lines**, md5
  `19cd44d86a8b15d825f76c6e7b265e65`, last touched at `dac3f8b` (phase 05.2, 2026-05-03) —
  **UNTRIMMED**.

### §0.4 — Standing censuses re-derived at HEAD

| census | value | how |
|---|---|---|
| `ROADMAP.md` rows | **117 / 116 `done` / 0 `in-progress` / 1 `planned`** | driven from the `^\| [0-9]` prefix, status read as **field 4** on a `' | '` split. ⚠ An `NF == 6` filter returns **ZERO** rows here, not merely two fewer — every row begins `| `, so the split yields a leading empty field and a normal row has **7**. That census reports a clean, believable `0` and would make stop-condition leg (i) look vacuously TRUE. |
| row `111` | **`planned`** | unchanged by the state-3 chain, the state-4 gate and both record commits — correct; it flips only at the state-6 close-out, which is not this session |
| ADR head | **ADR-0182**, next free **ADR-0183** | `grep -o '^## ADR-[0-9]\{4\}' \| sort -t- -k2 -n \| tail -1`. Raw `grep -c '^## ADR-'` reads **179**, one high, because of a schema template at the file head; distinct numbers = **178**. `ADR-0183` does not exist. |
| ADRs added by this phase's state-3/4 | **0** | `git diff --numstat 0ba60db 6a790ab -- docs/envoy-rust/DECISIONS.md` is empty. Correct: an implementation session and a verification gate both record in `PROGRESS.md`. `ADR-0181`/`ADR-0182` landed earlier, at `be1aaf1` (state 2). |
| `STATE.md` `**[NEW AT ` blocks at HEAD | **ANCHORED 50 / NAIVE 53 / BARE 56** | whole-file and traps-line-only COINCIDE at HEAD. Traps line = **198 510 characters**. |
| the same census at `6a790ab` | **ANCHORED 49 / NAIVE 52 / BARE 55** | series, anchored form: `0ba60db` **48** → `111b34a2` **49** → `6a790ab` **49** → `d4ebda2`/HEAD **50** |
| crates | **14** | `envoy-{accesslog,admin,bin,cluster,config,filter,health,http1,http2,jwt,listener,stats,tcp,tls}` |

---

## §1 — Strengths

**The emit fork is three expressions, and all four measured rows fall out of them.**
`crates/envoy-http2/src/response.rs:143-159`:

```rust
send_response.send_response(head, /* end_of_stream = */ body_empty && trailer_map.is_none())
if !body_empty { send_stream.send_data(resp.body, /* end_of_stream = */ trailer_map.is_none()) }
if let Some(map) = trailer_map { send_stream.send_trailers(map) }
```

With `trailers = None` this reduces token-for-token to the pre-phase expressions:
`send_response(head, resp.body.is_empty())` and `send_data(body, true)`. **D4's byte-identity
requirement for the no-trailers path is met by construction, not by test** — which is the strongest
form the requirement can take, and it is why the 89 pre-existing fixtures were never at risk.

**`build_trailer_map` runs before a single byte goes out** (`response.rs:139-146`). A conversion
failure therefore cannot leave a half-emitted response on the wire. That ordering is easy to get
wrong and was got right without comment.

**The unit tests at the emit seam drive a real in-process HTTP/2 connection and read back what the
client observed** (`response.rs:362-416`, `round_trip`). They test the *frame sequence*, not the
code shape, which is the only thing that can pin a fork whose whole content is END_STREAM placement.
The helper's own comment at `:400-402` names the trap it had to avoid — trailers must be awaited
*before* the connection task is aborted, "or the trailer HEADERS frame is never pumped off the socket
and this reads an empty block — a false green on the very cell under test". The same discipline is
repeated independently at four more sites (`hcm.rs:1794-1796`, `tests/differential/src/lib.rs:2434-2438`,
`tests/helpers/http2-echo-server/src/main.rs:455-456`, `tests/differential/src/backend.rs:996-1006`).
A phase that identifies its own vacuity hazard five times and writes the guard each time is a phase
that understood what it was building.

**The `H2SendTrailers` widening is provably unable to reach an abort — verified workspace-wide,
which is wider than the claim it was checking.** `grep -rn 'unreachable!\|unimplemented!\|todo!'
--include=*.rs crates/envoy-http2/ crates/envoy-bin/` = **0**; the same grep over all of `crates/`
and `tests/` = 18 hits, none of them a `match` on `Http2Error`. A full census of `Http2Error::`
outside `error.rs` (40 hits) finds **zero** exhaustive `match` arms on the type — every pattern use
is a `matches!(…)` or an `Err(Http2Error::X { .. }) =>` in a test with an `other =>` catch-all
(`client.rs:418, 425, 586-587`). `PoolError` *is* matched exhaustively (`hcm.rs:301-323`,
`envoy-http1/src/hcm.rs:456-475`), but on `PoolError`, whose `Connect(#[from] Http2Error)` arm binds
the inner error without inspecting it. This is the documented "widening a returnable error set lands
in a caller's `unreachable!()`" hazard, and it was closed by measurement rather than by argument.

**The threading is compiler-enforced at five of its six sites, which is better than the design the
PLAN described.** `trailers` is a **field** on `H2AttemptResult` (`hcm.rs:163`), so all five
construction sites (`hcm.rs:195, 387, 409, 423, 435`) are `E0063`-enforced — a new synth path cannot
forget it. The `let (resp, resp_trailers): (Response, crate::TrailerBlock) = match request_path`
tuple at `hcm.rs:596` forces every arm of the request-path match to supply a value. Only the
`AcquireOutcome::Sent(Ok(..))` arm at `hcm.rs:400` carries real trailers; every other arm is a
locally-generated synth and passes `None`. The single genuinely unforced site is the hand-written
clear at `hcm.rs:1082`, and it carries its own test —
`h2_encode_filter_stop_and_send_drops_upstream_trailers` (`hcm.rs:5474`) — whose doc comment
correctly names it "the alongside design's INVERSE hazard".

**Retry correctness is structural, not incidental.** `attempt` is a per-iteration binding inside the
retry `loop` (`hcm.rs:746`); both `continue` arms (`:906`, `:918`) drop it whole, and only the
`break` at `:858-862` propagates `attempt.trailers`. A trailer block read on a retried-away attempt
cannot survive. The H1 upstream fork hard-codes `None` (`hcm.rs:265-273`), correctly — H1 trailers
are unbuilt.

**The read site sits exactly where D2 specifies and for the stated reason.**
`client.rs:212-233`: after the `while let Some(chunk_result) = recv_stream.data()` drain, **before**
the `100..=599` status guard, mirroring the header conversion's defensive non-ASCII skip four lines
below. The placement is not merely asserted — it is a hard requirement of the codec, and it holds:
`h2-0.4.16` `src/proto/streams/recv.rs:1216-1258` shows `poll_data` returning `Ready(None)` on a
non-`Data` event while pushing the `Event::Trailers` **back onto the front of the queue** and calling
`notify_recv()` specifically so a later `poll_trailers` finds it. `data() -> None` is precisely the
precondition `trailers()` needs.

**No latency or hang regression for the 89 pre-existing fixtures that route through `drive_http2`.**
After `data()` yields `None` there are exactly two states, both of which resolve `trailers()` without
further I/O: an `Event::Trailers` is queued and pops immediately, or the queue is empty and
`schedule_recv` returns `Ready(None)` because `ensure_recv_open()` is false. `Poll::Pending` is
unreachable. The extra `.await` costs nothing.

**`diff_headers` and `HEADER_ALLOW_LIST` are byte-identical across the graded range.** The phase
reuses the header-axis comparison verbatim rather than growing a `diff_trailers`, so the 89
pre-existing fixtures' header comparison has **zero** blast radius, and `HEADER_ALLOW_LIST` is still
exactly the three entries (`server`, `date`, `x-envoy-upstream-service-time`) that non-goal 8
requires it to stay. Neither trailer name is on it, so both are compared VALUE-EXACT.

**The expectation dispatch is genuinely opt-in-safe.** `expected_trailers: Option<Http1TrailerRule>`
carries `#[serde(default)]` (`lib.rs:194-199`), and the comparison at `lib.rs:6800-6810` is guarded
by `matches!(…, Some(…))`. Three tests pin exactly this — including
`fixture_0010_expectations_still_parse_without_expected_trailers` (`lib.rs:9396`), which asserts the
parent fixture's own expectations still deserialise. Nothing is asserted on any fixture that omits
the key.

**The `{{HTTP2_TRAILERS_BACKEND_PORT}}` plumbing closes the repo's known unsubstituted-token family.**
Five sites — scan (`lib.rs:3623`), spawn (`:3627`), port string (`:3637`), upstream kv + `BACKEND_HOST`
guard (`:3689`, `:3708`), subject kv + guard (`:3799`, `:3818`) — all keyed off one condition, so
`h2_trailers_backend_port_str` is `Some` **iff** the backend spawned. `scan_needs_marker` wraps the
marker in `{{…}}`, so scanning for `HTTP2_BACKEND_PORT` cannot false-positive on
`{{HTTP2_TRAILERS_BACKEND_PORT}}` or vice versa; and the scan's six template sources are exactly the
set `render_yaml` is applied to. **An unsubstituted token cannot reach either parser**, because any
placement that renders it is a placement the scan sees. Critically the new backend was also added to
**both** `BACKEND_HOST` gate chains — without that arm the port would render while `{{BACKEND_HOST}}`
did not, which is the exact failure shape the ledger records for `{{ADMIN_PORT}}`. The keep-alive
binding is a leading-underscore *name*, not the wildcard `_` pattern, so the child survives to the end
of `run_fixture`.

**The two YAMLs are fixture `0010` verbatim plus exactly two configuration lines per side, and the
per-side divergence comment block is byte-identical to `0010`'s.** D8's "carried over VERBATIM" is
literally true, not approximately. Self-diffing `0090`'s two sides yields exactly the five documented
divergences (`admin:` block, `0.0.0.0` vs `127.0.0.1`, `generate_request_id: false`, the six-entry
`request_headers_to_remove`, `dns_lookup_family: V4_ONLY`) and **nothing more**. D-PLAN-7's
body-stabilising suppressions carried over complete: `generate_request_id: false` plus all six
`request_headers_to_remove` entries, byte-identical to the parent.

**Fixture `0090`'s non-vacuity is not argued, it is measured.** `PROGRESS.md:711-741` records that
the PLAN's own Task-9 mutation was **misaimed** — it redded on FRAMING (`stream no longer needed`),
not on trailers — and that a re-aimed one (passing `None` for `trailers` at the emit call) produced

```
Caused by:
    header name sets differ: only-in-envoy=["x-trail-a", "x-trail-b"], only-in-envoy-rust=[]
```

That single string is a four-way proof: Envoy emits both, the harness observes both on the Envoy
side, the assertion bites on the **trailer** axis (the `diff_trailers` context string is what
distinguishes it from the header diff), and envoy-rust's forwarding is what makes it green. **A RED
is not evidence until you read which cell it names**, and this session read it, discovered the plan's
mutation was aimed at the wrong cell, and re-aimed it rather than accepting the green.

**The backend emits trailers on the wire in a shape both proxies see identically, and the echo body
is provably unchanged between the two helper modes.** `tests/helpers/http2-echo-server/src/main.rs`
does `send_response(response, false)` → `send_data(body, false)` → `send_trailers(map)`; `h2-0.4.16`
`frame/headers.rs:131-149` (`Headers::trailers`) sets END_STREAM on the trailer frame. Both handler
modes call the **identical `make_response_body(&parts, &body_bytes)`** with identical arguments — the
same function, no wrapper, no extra line. `Http2TrailersBackend` (`backend.rs:547-587`) is a faithful
structural clone of `Http2CloseBackend` including the **H2-handshake-aware** `wait_h2_accept_ready`
readiness poll and the reaping `Drop`, so it adds no member to the repo's documented startup-race
family. `Args` derives `PartialEq` and the existing tests use full struct literals, so adding the
`trailers` field forced both to be updated — a real forcing function that fired.

**The `--trailers` path is proven end to end at two levels.** In-process against the handler
(`main.rs:417-468`, asserting status, that the announce header is *exactly* `x-trail-a` and nothing
else, the body shape, and both trailers with `x-trail-b` labelled "the UNANNOUNCED trailer must be
sent too"), and through the **real spawned binary** via `Http2TrailersBackend::spawn()`
(`backend.rs:959-1007`) — the latter being the only thing that covers `run()`'s `else if args.trailers`
dispatch arm, which an in-crate handler test would not catch.

**`drive_http2_reports_no_trailers_when_none_sent` (`lib.rs:9646`) is a real negative control**, not
filler: without it the positive test would be satisfied by a driver that fabricates entries. The same
discipline appears at the emit seam (`no_trailers_*_is_unchanged`) and at the read site
(`send_request_returns_none_when_upstream_sends_no_trailers`, `client.rs:676`).

**The `BEHAVIOR_CONTRACT.md` section carries no line-number citations at all**, and every path
citation in it resolves. That is the correct lesson from `110.2` M-1, applied one phase later by a
different session. It also records **eight of the phase's nine carry-forwards**, including the live,
phase-CREATED CF-111-6, under a heading literally titled "Measured upstream behaviours that
envoy-rust does NOT match" — which cannot be mistaken for a parity claim. Its MATCHES /
does-NOT-match / Still-unmeasured split is a cleaner arrangement of the same content the gRPC
section's §F/§G/§H carries.

**The section names the limits of its own witness.** Duplicate-name multiplicity and trailer order
are both recorded as **gaps rather than guarantees**, with the reason (a set comparison collapses the
name; `HeaderMap` iteration order is not insertion order). A section that quietly enjoyed the
appearance of pinning multiplicity would have been easier to write and much worse.

**Task 4's mutation hygiene is exemplary and should be the template.** It asserts
`grep -c 'trailers = None;'` = **1** *and* `grep -c 'trailers: None,'` = **4** *before* mutating —
precisely the "a `sed` on the wrong spelling fakes a result" trap — then shows a `Compiling
envoy-http2` line (so the binary is not stale), a `test result:` line (so the RED is a test failure
and not a compile error), a FAILED verdict, and a GREEN control from the **same seeded worktree**.
Every element of the project's recorded mutation discipline is present in one place.

**All ten planned tasks landed in full, and three of the landed tests are strictly STRONGER than the
plan's sketch.** `tests/differential/src/backend.rs`'s backend test round-trips a real H2 request and
asserts both trailers where the plan asked only for `port() > 0` plus a host assertion; the
`http2-echo-server` wire test was never asked for at all; and
`fixture_0010_expectations_still_parse_without_expected_trailers` loads a **real on-disk fixture**
through `load_expectations` rather than a string literal, which is what makes the "89 fixtures keep
deserialising" claim a fact about the corpus rather than about a test fixture.

**The state-4 gate record is evidence, not assertion, and it reproduces.** Every number in it that is
recoverable from the repository was independently re-derived by this review and matched exactly: net
LoC **1525**; the `+24` test-attribute delta **and its per-file split** (response.rs 6, client.rs 2,
hcm.rs 3, main.rs 3, backend.rs 1, lib.rs 8, the new runner 1); the `**[NEW AT ` census
**49 / 52 / 55** at `6a790ab`; the `known-failures.txt` line count and md5; the 5 fuzz targets ↔ 5
`ci.yml` steps; the `ROADMAP.md` **117 / 116 / 0 / 1**; the commit numstats `25 26` / `35 0` /
`146 0` for `111b34a2` and `2 0` for `6a790ab`. **No quoted number in the record contradicts another**,
and the inter-run arithmetic is self-consistent three ways (`2228 + 24 = 2252` from CI, from local
sweep 1's `2244 + 8`, and from local sweep 2's `2244 + 8`).

**The carry-forward ledger is intact element-for-element.** Normalising `SPEC.md` §6, `PLAN.md` §3
and `PROGRESS.md`'s lists to token streams and diffing them finds **no member present in one and
missing from another**. All nine CF-111-1…9 are stated, each appears outside the phase directory,
and `PLAN.md` §2 explicitly *labels* its three corrections to the landed `SPEC.md` rather than
quietly overwriting them — which is the difference between reconciliation and revisionism.

---

## §2 — Issues (Must Fix)

**NONE.**

Stated plainly, because §5.2 makes this the load-bearing sentence of the review: any Issue here would
send the work back to §5 **state 3**, not state 4.

The production seam is correct on every input traced. The three-way fork matches all four measured
rows; the no-trailers path is byte-identical by construction; the `StopAndSend` clear is complete
over a closed enumeration of local-reply sites verified two independent ways (the two production
call sites of `decorate_filter_synth_response_h2`, and the `E0063`-enforced `H2AttemptResult.trailers`
field); retried-away trailers cannot survive; the error widening cannot reach an abort anywhere in
the workspace; `crates/envoy-filter/` is untouched; the two fixture YAMLs are `0010` verbatim plus
two configuration lines per side; the trailer comparison bites on the trailer axis, proven by a
re-aimed mutation with a named cell; and CI is green on the exact code tree with fixture `0090`
demonstrably executed and all 90 differential binaries demonstrably run.

**Five subagent findings were graded Important and are DOWNGRADED by this session; §5 records each
dissent with reasoning.** Every one is a test-strength or documentation defect. Not one changes a
byte of wire behaviour on a path a fixture, a gRPC client or a proxied response takes, and not one is
worth three more sessions under §5.2.

---

## §3 — Minor

### M-1 — `Some(vec![])` IS producible, on three reachable inputs, and two live production branches read the invariant it breaks

`crates/envoy-http2/src/lib.rs:46` states, of `TrailerBlock`:

> `Some(vec![])` is never produced.

`PLAN.md` D-PLAN-2 states the same: "`Some(vec![])` is not produced." The landed read site produces
it. `crates/envoy-http2/src/client.rs:218-232`:

```rust
let trailers: Option<Vec<(String, String)>> = recv_stream.trailers().await…
    .map(|map| {
        let mut out = Vec::with_capacity(map.len());
        for (name, value) in map.iter() {
            let Ok(value_str) = value.to_str() else { continue };
            out.push((name.as_str().to_string(), value_str.to_string()));
        }
        out                      // <-- returned unconditionally, even when empty
    });
```

The `map` closure returns `Some(out)` whatever `out` contains, and `.map` preserves `Some`. **Three
reachable inputs empty it**, found independently by three of the six reviewers:

1. **A zero-field trailer HEADERS block.** `h2-0.4.16` `proto/streams/recv.rs`'s `recv_trailers`
   calls `frame.into_fields()` and pushes `Event::Trailers(map)` with **no emptiness check**;
   `share.rs`'s `poll_trailers` maps `Some(Ok(map)) → Ok(Some(map))`. A zero-field trailer frame
   therefore resolves to `Ok(Some(HeaderMap::new()))`, not `Ok(None)`. **This is exactly PV-3 row 8**
   — see M-5.
2. **A trailer block whose values are all non-ASCII.** `http-1.4.0` `header/value.rs` permits
   obs-text (`0x80..=0xFF`) in a `HeaderValue`, and `h2`'s HPACK decoder builds values with
   `HeaderValue::from_bytes`; `to_str()` rejects exactly those bytes. A response whose only trailer
   is `x-t: <0xFF>` yields `Some(vec![])`.
3. **A trailer block containing only pseudo-headers** — the CF-111-6 shape. `into_fields()` discards
   the `Pseudo` struct and surfaces only `fields`, so a pseudo-only block arrives as
   `Some(<empty HeaderMap>)`.

**Why it matters — it reaches the wire.** `response.rs:147` and `:152` both branch on
`trailer_map.is_none()`. With `Some(vec![])` the map is `Some(empty)`, so an **empty-body** response
takes `send_response(head, false)` and then `send_trailers(<empty map>)` — h2 applies no emptiness
check on either side (`share.rs:344`, `proto/streams/send.rs:301-323`) — instead of the single
`send_response(head, true)` it would otherwise emit. That is END_STREAM moved off the response
HEADERS frame onto a **second, zero-field HEADERS frame**: a wire shape neither this phase nor its
predecessor intends, and one the equivalence matrix's own `HTTP/2 & HTTP/3 framing` row
(`BEHAVIOR_CONTRACT.md:19`, "same frame types/order on equivalent events") speaks to. The blast
**Input 1 is the sharpest of the three, and it deserves saying plainly: it is the one cell where this
phase moved a frame sequence that was previously at PARITY.** Inputs 2 and 3 land on responses that
already diverge for other, banked reasons. Input 1 does not — PV-3 row 8 measured the zero-field
block at parity ("0 trailers, clean" on both sides), and pre-phase envoy-rust emitted
`send_data(body, end_of_stream = true)`: one DATA frame, stream closed. Post-phase it emits
`send_data(body, false)` followed by an extra zero-field HEADERS frame carrying END_STREAM.

The blast radius is nonetheless small, and that is why this is M-1 rather than an Issue. The change
is invisible at the level every consumer reads: `RecvStream::trailers()` yields `Some(<empty>)`,
which `drive_http2` and every other consumer flattens to zero trailers, so status, body, headers and
trailers all still agree and **no fixture can go red**. But the equivalence matrix's own
`HTTP/2 & HTTP/3 framing` row (`BEHAVIOR_CONTRACT.md:19`, "same frame types/order on equivalent
events") speaks to exactly this, the invariant is load-bearing at two live branch sites, and a future
`%TRAILER(…)%` consumer (CF-111-4) would branch on it too. Nothing re-measured this cell after the
forwarding landed, nothing tests it, and the design document says the state cannot arise.

**Fix:** one line at `client.rs:232` — return `None` rather than `Some(out)` when `out` is empty
(`.and_then(|map| { … if out.is_empty() { None } else { Some(out) } })`). This is the only finding in
the review that is a code defect rather than a test or documentation defect, and it is a one-line
code defect whose consequence is one extra empty frame on an input that already diverges. That is why
it is M-1 and not an Issue.

### M-2 — the gRPC main case's "NO DATA frame" property is named but not asserted; deleting the `if !body_empty` guard survives the entire suite

`response.rs:449`, `trailers_follow_an_empty_body_with_no_data_frame`, asserts
`assert!(body.is_empty(), "expected no DATA frame, got {body:?}")`. That is an assertion about
**bytes**, not about **frames**. Concrete mutation the whole unit suite survives — delete the
`if !body_empty {` guard at `response.rs:150` so `send_data` always runs:

- **Row (empty, present):** a zero-length DATA frame is emitted, then trailers. `body` still
  accumulates nothing, so `body.is_empty()` holds → **the test at `:449` passes.**
- **Row (empty, none):** `send_response(head, true)` then `send_data(.., true)` →
  `UserError::UnexpectedFrameType`. But `send_envoy_response(..).await.expect(…)` runs inside a
  `tokio::spawn`ed task the harness **never joins** (`response.rs:362-380`, `round_trip`; `server.abort()` at `:414`), and the client has already received a complete `204` with END_STREAM → **the test at
  `:475` also passes.**
- Rows (non-empty, none) and (non-empty, present) are unaffected.

Fixture `0090` cannot catch it either: its probe is a non-empty body.

So the single most consequential row in the phase — the one the whole gRPC prerequisite exists for,
and the one `BEHAVIOR_CONTRACT.md:977-980` states as "with **no DATA frame at all**" — has no
assertion distinguishing "no DATA frame" from "an empty DATA frame". The contract makes a framing
claim that nothing in the tree pins.

**Fix:** in `round_trip`'s drain loop (`response.rs:395-399`) count the iterations and return the
count; assert `data_frames == 0` at `:449` and `== 1` at `:424`/`:464`. Roughly ten lines, and it
converts the currently-indirect coverage of rows 1 and 3 (see N-1) into explicit assertions at the
same time.

### M-3 — D-PLAN-6 commits three cells to unit tests at the emit seam; only ONE is covered, the fixture README repeats the claim, and `PROGRESS.md` does not disclose the gap

`PLAN.md` D-PLAN-6 excludes three cells from fixture `0090` on this explicit ground:

> empty-body-with-trailers, non-200-with-trailers, five-trailer order → real and measured, but each
> needs a *second* backend mode or a second fixture; **they are covered by unit tests at the emit seam
> instead**, which is where the logic lives

Census of every trailer test in `crates/envoy-http2/` (`response.rs:424, 449, 464, 475, 488, 505`;
`client.rs:652, 676`; `hcm.rs:1859, 1897, 5474`), cross-checked against every `synth_response(` call
site in `response.rs` (statuses used: `200` ×6, `204` ×1, `418` ×1, `99` ×1):

| cell | covered? |
|---|---|
| empty body + trailers | **yes** — `trailers_follow_an_empty_body_with_no_data_frame` (`response.rs:449`) |
| **non-200 + trailers** | **NO.** The only non-200 statuses in the file are `418` (`:220`), `99` (`:233`) and `204` (`:476`); `204` is `no_trailers_empty_body_is_unchanged`, i.e. non-200 *without* trailers. Nothing anywhere pairs a non-200 with a trailer block. |
| **five-trailer blocks / order** | **NO.** The largest block in any test is three entries (`trailer_names_envoy_forwards_are_not_stripped`, `:488`), and `sorted()` (`:418`) actively destroys order in every test that could have shown it. |

`tests/fixtures/0090-h2-response-trailers/README.md:35` repeats the claim verbatim
("Covered instead by unit tests at the emit seam (`crates/envoy-http2/src/response.rs`)"), and
`PROGRESS.md` does **not** disclose the gap — `grep -n 'non-200\|five-trailer'` over it returns
**zero** hits.

**Why it matters.** This is the closest thing in the phase to a §6.3 anti-pattern: work was excluded
from the fixture *on the stated ground that it lives elsewhere*, and for two of three cells it does
not live anywhere. The behavioural risk is genuinely low — the emit seam treats status opaquely and
does not branch on trailer count — but a future session reading either document will believe two
cells are pinned when nothing pins them, and this repository's own ledger records
inherited-and-drifted fixture-README claims as a recurring hazard.

**Fix:** add the two tests. Non-200 is trivial and was explicitly measured upstream (PLAN §1 PV-3
row 7): one more `round_trip(synth_response(500, .., b"err"), Some(vec![…]))`. Five-trailer order is
harder and arguably should not be attempted (see N-2 — `HeaderMap` iteration order makes an order
assertion partly meaningless), in which case the honest move is to narrow both documents' claim to
the one cell that is covered and bank the rest against CF-111-9.

### M-4 — the trailer comparison is vacuity-permitting by construction, and it is the first harness rule for which that is possible

`diff_headers` on two empty slices returns `Ok(())`: `names_lc` yields two empty `BTreeSet`s, they
compare equal, and the value loop never runs. So
`expected_trailers: set_equal_modulo_allow_list` (`tests/differential/src/lib.rs:6800-6810`)
**cannot distinguish "both proxies forwarded the block" from "neither side saw a single trailer"**. A
fixture that exists solely to witness trailer forwarding can go green while witnessing nothing.

This is **not** the failure the phase was asked about and it is **not** currently false: a
subject-only trailer loss is caught loudly, and the re-aimed mutation at `PROGRESS.md:711-741` proves
it. The exposed case is a **both-sides** regression — a helper `--trailers` mode that stopped
emitting, or a regression in `drive_http2`'s trailer read.

What holds the property today is convention plus two unit tests
(`drive_http2_surfaces_response_trailers`, `lib.rs:9613`; the helper's wire test, `main.rs:417`), and
**both begin with a `locate_http2_echo_server().is_err()` early-return that prints a `skipping …`
line and passes**. Contrast every other rule in this harness: `expected_headers` cannot go empty
(responses always carry headers) and `expected_body: byte_exact` requires an explicit literal.
`expected_trailers` is the first assertion whose asserted quantity can legitimately be empty on both
sides, so there is no precedent excusing the omission.

**Fix, inside the phase's own grain:** before the `diff_headers` call at `lib.rs:6805`, bail when the
rule is declared and `upstream_resp.trailers.is_empty()` — the upstream-Envoy side is the reference,
and a reference that produced no trailers means the fixture is not measuring what it claims. Three
lines, no new YAML surface, no allow-list change, no new rule variant. (A
`NonEmptySetEqualModuloAllowList` variant would also work but costs an enum member for one fixture.)

**Three of the six reviewers reached this independently**, which is the main reason it is recorded at
this length rather than as a note.

### M-5 — the contract files the empty-trailer-block cell under MATCHES on the strength of a PRE-FIX measurement

`BEHAVIOR_CONTRACT.md:982-983`, under **"Measured upstream behaviours that envoy-rust MATCHES"**:

> An **empty trailer HEADERS block** (zero fields) yields zero trailers and a clean end-of-stream on
> both sides.

The envoy-rust half was measured **before the fix**: `PLAN.md` §1 PV-3 row 8 records it in a column
headed *"envoy-rust today"*, i.e. with no trailer code in the tree. Post-fix the path is different,
and by M-1 it produces `Some(vec![])` and emits an extra zero-field HEADERS frame. Header, body and
trailer comparison still agree — which is why no test catches it — but the **frame sequence** differs.

**Why it matters:** this is the one sentence in the section filed under MATCHES that could not be
confirmed against the landed code, and per doctrine D-3.3 the contract *is* the specification a
future phase implements against.

**Fix:** move the cell out of MATCHES — either into `### Still unmeasured` with a note that the
envoy-rust column predates the change and the framing was never compared, or into the does-NOT-match
list once M-1 is decided. Closing M-1 in code makes the MATCHES claim true and this finding
disappears.

### M-6 — "forwarded verbatim" is stronger than the code, which silently DROPS non-ASCII trailer values, and the drop is recorded nowhere

`BEHAVIOR_CONTRACT.md:920` states the forward rule as "forwarded **verbatim**", and `:973-975`
generalises to "Envoy sanitises no trailer NAME at all". But `client.rs:227-229` does:

```rust
let Ok(value_str) = value.to_str() else { continue };
```

`HeaderValue` permits obs-text; `to_str()` does not. A trailer such as `x-note: caf\xE9` is accepted
by h2's decoder, **silently discarded here**, and never reaches the wire — while upstream Envoy
forwards it. PV-3 probed a trailer value with a space and a comma (row 9) but never a non-ASCII one,
so the cell is genuinely unmeasured — yet it appears neither in the does-NOT-match list nor in
`### Still unmeasured`.

The skip itself is defensible: it mirrors the pre-existing header-path skip four lines below
(`client.rs:245-249`), and consistency with the header axis is the right call. What is wrong is that
the contract says "verbatim" with no counterpart statement.

**Fix:** one bullet under `### Still unmeasured` recording that envoy-rust's trailer conversion skips
a value that is not visible-ASCII, mirroring the header path, and that Envoy's behaviour on such a
trailer is unmeasured. Two sentences; it stops a future phase reading "verbatim" as a contract it can
rely on.

### M-7 — this phase stale-dated a citation in a SIBLING section of `BEHAVIOR_CONTRACT.md`, and that is a fourth drifted citation beyond the three banked

`BEHAVIOR_CONTRACT.md:796`, inside the **gRPC** section, reads:

> `x-envoy-upstream-service-time`, at `tests/differential/src/lib.rs:1189-1193`

At `0ba60db` that was exact — `git show 0ba60db:tests/differential/src/lib.rs | grep -n
'HEADER_ALLOW_LIST: '` → **1189**. The constant is now at **`tests/differential/src/lib.rs:1212-1216`**,
moved **+23** by this phase's own insertions into `lib.rs` (the `Http1TrailerRule` block near
`:1093`). The phase edited the containing file; it did not edit the containing section; the citation
broke anyway.

**Why this is the review's most interesting finding.** `110.2` M-1 named the self-invalidating
citation: a document whose own commit invalidates its own `file:line`. This is the next case out —
**a document invalidating a NEIGHBOUR's citation, in a file it edited but a section it did not
write.** The three citations banked for this review are all of the first kind (a document's citations
into files its own phase edited). This one is not in any of the three artifacts a reviewer is pointed
at, and no per-document audit would have found it. The general rule it implies: **when a phase
inserts into a shared canonical document, or into a shared source file, it must re-resolve every
`file:line` citation *anywhere in the repository* that points below the insertion point** — not just
the ones in its own documents.

Two related, both pre-existing and neither this phase's defect, recorded so one line-count pass
closes all three: `PLAN.md`'s Global Constraints carry the same now-stale `1189-1193` (landed and
uneditable — graded, not patched); and `BEHAVIOR_CONTRACT.md:888` cites the ADR-0059 no-`content-type`
rule as `:1131-1137` when the text is at `:1523` today — that one was **already** wrong before this
phase and the new 132-line section at `:904` moved the target a further +132.

**Fix:** correct `:796` to `1212-1216`, or better, drop the range entirely as the new
`## Response trailers` section already does.

### M-8 — the contract says "the same six names" but `H2_FORBIDDEN_HOP_BY_HOP` has five

`BEHAVIOR_CONTRACT.md:1008`: "Mirroring the header block's `H2_FORBIDDEN_HOP_BY_HOP` strip onto
trailers … `h2` rejects **the same six names** on the RECEIVE side".

`crates/envoy-http2/src/lib.rs:36-42` has **five** entries: `connection`, `transfer-encoding`,
`keep-alive`, `upgrade`, `proxy-connection`. `te` is not among them. h2's set is those five **plus** a
conditional `te != "trailers"` — six *conditions*, not six shared names. The section's own CF-111-5
bullet at `:989-992` enumerates all six correctly, so this is an internal inconsistency within one
section.

**Fix:** "…rejects those five names plus `te` with a value other than `trailers` on the RECEIVE
side". Standing rejection (1) is unaffected — it is correct on its facts, and independently so:
`h2-0.4.16` applies the identical check on the **send** side too
(`proto/streams/send.rs:312-328` → `check_headers`), which makes the dead-strip argument doubly sound.

### M-9 — CF-111-4 is the one carry-forward of nine absent from `BEHAVIOR_CONTRACT.md`

The `### Scope` list records CF-111-2 (H1 trailers), CF-111-3 (request trailers) and CF-111-1 (filter
pipeline). CF-111-4 — the `%TRAILER(…)%` and `%GRPC_STATUS%` access-log operators — is not there.
`grep -c 'CF-111-4\|%TRAILER\|GRPC_STATUS' docs/envoy-rust/BEHAVIOR_CONTRACT.md` returns **0**: not
one occurrence of either operator anywhere in the 4437-line file.

Eight of nine CFs are in the section. This is the ninth, and it is the one a future access-log phase
would grep the contract for — and find nothing at all. Same shape as `110.2` M-5, one phase later.

**Fix:** one bullet under `### Scope`.

### M-10 — `PROGRESS.md:500-513`'s no-self-skip evidence is unsound as recorded

The Task-6 record claims the two new `drive_http2` trailer tests did not self-skip:

> 3 = the pre-existing `drive_http2_round_trip_against_in_process_listener` + the 2 new ones, **with
> no `skipping …` line printed**

But the command quoted at `:504` is `cargo test -p differential --lib drive_http2` with **no
`--nocapture`**. Cargo suppresses a *passing* test's captured stdout, so the absence of a `skipping …`
line is guaranteed whether or not the tests skipped — the observation cannot discriminate.

Task 7 gets this right at `PROGRESS.md:466`, which does pass `-- --nocapture` and whose "1 passed
with no `skipping …` line" is therefore sound. The conclusion is almost certainly still true (that
`--nocapture` run proves `http2-echo-server` was built in the same session, and CI runs
`cargo build --workspace --all-targets` before `cargo test --workspace`, so the guard cannot fire
there) — but the stated evidence does not establish it.

This is the repository's own recorded lesson about silently self-skipping gates — the h2spec local
green is the canonical case — applied to the phase's own new tests. It is worth recording precisely
because the phase got it right one task earlier and wrong one task later.

### M-11 — the helper's body-identity rationale is false, and the invariant it defends is unasserted

`tests/helpers/http2-echo-server/src/main.rs:218-220`:

> The echo body shape is deliberately IDENTICAL to `handle_connection`'s: fixture `0090` inherits
> fixture `0010`'s byte-exact body comparison, so any divergence here would fail the fixture for a
> reason unrelated to trailers.

That is not what `0090` does. `tests/fixtures/0090-h2-response-trailers/expectations.yaml` says
explicitly "Deliberately NO per-driver `expected_body`" and asserts only cross-proxy
`equivalence.response_body: byte_exact`. Since **both** proxies drive the **same** trailers backend, a
body change in `--trailers` mode shifts both sides equally and `0090` stays green.

The invariant is nonetheless true — both handlers call the identical `make_response_body(&parts,
&body_bytes)` — but it is held by code identity, not by any gate. The wire test at `main.rs:453-454`
checks only `starts_with("method: GET\n")` and `contains("path: /test\n")`, weaker than the
non-trailers test at `:521-524`, which additionally pins `:authority` and `:scheme`.

**Fix:** correct the rationale to name what actually protects it (the shared `make_response_body`
call), or add the real pin — drive both handlers in one test and assert the two bodies are equal.

### M-12 — `trailer_names_envoy_forwards_are_not_stripped` asserts only a count

`response.rs:488-503` ends with `assert_eq!(sorted(trailers).len(), 3);`. It never checks *which*
three survived. A mutation that renamed or re-cased a trailer, or that swapped a value, passes. Given
the test's stated purpose is pinning D-PLAN-4's no-strip decision against PV-3 rows 10-12
(`content-length`, `te: trailers`, `host` forwarded verbatim), it should assert the three
`(name, value)` pairs, as its neighbours at `:424` and `:505` already do. Three lines.

### M-13 — `Http1TrailerRule` is born misnamed, and is a byte-identical twin of `Http1HeaderRule`

`tests/differential/src/lib.rs:1093-1108` defines `Http1TrailerRule` with the same three derives, the
same `#[serde(rename_all = "snake_case", deny_unknown_fields)]`, and the same single
`SetEqualModuloAllowList` variant as `Http1HeaderRule` (`:1087-1091`). It is used at exactly one
place — the `Driver::Http2` variant — and its own doc comment opens "trailer equivalence rule for
`Driver::Http2`".

The sibling `Http1HeaderRule`/`Http1BodyRule` earned their prefix by being defined for the H1 driver
first and reused on H2 later. This one is born wrong, and it reads especially oddly against
`DriveHttp1Result.trailers`' doc at `:1223-1228`, which states H1 trailers are permanently empty.

Either reuse `Http1HeaderRule` directly (identical fixture YAML, zero new types) or name it
`TrailerRule`. Not worth a re-spin on its own; worth folding into any follow-up touch.

### M-14 — five of the ten tasks have a COMPILE ERROR as their only RED, and the one that matters is Task 1

Grading each task's recorded RED in `PROGRESS.md` against the three kinds this project accepts — a
real assertion failure, a mutation-proved characterisation pin, or (not evidence) a compile error:

| task | recorded RED | grade |
|---|---|---|
| 1 — emit fork, 6 tests | `error[E0061]: this function takes 2 arguments but 3 arguments were supplied` | **compile error only** |
| 2 — read site, 2 tests | `error[E0308]: mismatched types … expected Response, found (_, _)` | **compile error only** |
| 3 — threading, 2 tests | quoted panic + `test result: FAILED. 0 passed; 1 failed` | **real assertion RED** |
| 4 — the clear, 1 test | quoted panic + `FAILED` **and** a full mutation check with control | **real RED + mutation-proved** |
| 5 — backend, 4 tests | `E0560`/`E0609`/`E0425` | **compile error only** |
| 6 — driver, 2 tests | `error[E0609]: no field 'trailers' on type 'DriveHttp1Result'` | **compile error only** |
| 7 — compare, 6 tests | `E0026`/`E0433` | **compile error only** |
| 8 — token plumbing | none, explicitly by PLAN design | **no RED, by design** |
| 9 — fixture | GREEN first run, then a documented misaimed → re-aimed mutation with a real trailer-axis failure | **mutation-proved** |
| 10 — contract | n/a, docs-only | **n/a** |

Two real assertion REDs, two mutation-proved, five compile-error-only, two with no RED.

For Tasks 2, 5, 6 and 7 the compile error is a defensible TDD RED and `PROGRESS.md` argues so
explicitly and honestly ("A compile error is NOT a valid *mutation-check* RED, but it IS the correct
TDD RED for a signature-changing task") — those tasks change a type or a struct and the interface
genuinely does not exist yet.

**Task 1 is the one that matters.** It carries the phase's single riskiest change — D4/D-PLAN-3's
three-way end-of-stream fork, which every one of the 89 pre-existing fixtures traverses — it landed
six behavioural tests, and the only evidence any of the six *bites* is that they failed to compile.
No mutation was ever aimed at the discriminating term `body_empty && trailer_map.is_none()`. The
nearest evidence is incidental: Task 9's **misaimed** plan mutation struck `if let Some(map) =
trailer_map {` and went RED on framing, which proves the emit path is load-bearing for *something* —
not that the four-row table is pinned cell by cell.

This is **M-2 seen from the other side**, and the two share one fix. One mutation pass over the fork
in a scratch worktree — flip `body_empty && trailer_map.is_none()` to `body_empty`, force a rebuild,
and confirm `trailers_follow_an_empty_body_with_no_data_frame` (and only it) goes RED with a
`test result:` line present — closes the only unproven claim in the phase's riskiest file. Combined
with M-2's frame counting, the fork would then be pinned by assertion *and* by mutation.

### M-15 — twenty-eight further `file:line` citations across `SPEC.md` and `PLAN.md` are stale at `6a790ab`, every one moved by this phase's own insertions

Beyond the three banked in §7 and the fourth in M-7: **8 in `SPEC.md`** and **20 in `PLAN.md`**.
Every one was TRUE at its own landing commit (`SPEC.md` at its state-1 commit, `PLAN.md` at
`82e2e75`) — indeed all 21 of `PLAN.md`'s citations resolved exactly at both `82e2e75` and
`0ba60db`, which is a real credit to the state-2 session. **There are zero UNRESOLVABLE citations in
either document.** This is drift, not error, and both documents are landed and were not patched.

Spot-checked and confirmed exactly by this session:

| document | citation | correct at `6a790ab` |
|---|---|---|
| `SPEC.md` §1.2 F5 | `crates/envoy-http2/src/client.rs:193` (the drain loop) | **`:203`** |
| `SPEC.md` §1.2 F6 / §3 D4 | `crates/envoy-http2/src/response.rs:81` (`send_envoy_response`) | **`:133`** |
| `SPEC.md` §1.2 F6 / §3 D4 | `crates/envoy-http2/src/hcm.rs:1043` | **`:1096`** |
| `SPEC.md` §1.2 F10 / §3 D7 | `tests/differential/src/lib.rs:2332` (`drive_http2`) | **`:2365`** |
| `SPEC.md` §1.2 F11 / §3 D7 | `tests/helpers/http2-echo-server/src/main.rs:58` | **`:62`** |

plus `SPEC.md`'s `lib.rs:50` → `:69` and `lib.rs:2343` → `:2376` and `lib.rs:2951` → `:3004`.
`SPEC.md`'s other nine citations (`probe.rs:551`, `envoy-http1/client.rs:588`/`:595`,
`envoy-http1/response.rs:13`/`:18`, `grpc.rs:217`, `BEHAVIOR_CONTRACT.md:18`, `pipeline.rs:88`/`:105`)
still resolve verbatim — they point into files this phase did not touch, which is exactly the
pattern. On the `PLAN.md` side only `grpc.rs:195` and `grpc.rs:436-439` survive; the rest moved
(e.g. `response.rs:87`/`:91` → the fork now at `:139`/`:147`; `hcm.rs:242`/`:245` → `:246`/`:251`;
`lib.rs:1204` `diff_headers` → `:1227`).

**The generalisation is M-7's, and this census is the evidence for it**: the citations a phase
invalidates are not confined to its own artifacts, and they outnumber the ones a handoff banks by an
order of magnitude. **Locate by TEXT.**

---

## §4 — Notes

**N-1 — rows (non-empty, none) and (empty, none) are covered only indirectly.** `response.rs:464`
and `:475` assert bytes and trailer-emptiness but never the END_STREAM flag. They detect a fork
collapse only because dropping a non-closed `SendStream` cancels the stream, which makes
`chunk.unwrap()` in the drain loop panic — so the failure message would be a `Reason::CANCEL` unwrap,
not "END_STREAM was on the wrong frame". M-2's frame-counting fix resolves this too.

**N-2 — "wire order" is preserved only for DISTINCT trailer names.** `crates/envoy-http2/src/lib.rs:44`
("in wire order") and `:55-56` ("preserving duplicate names and wire order for free") overstate the
guarantee. `http::HeaderMap::iter()` walks entries in index order and chains each entry's extra
values, and its own doc says the order is arbitrary; for a fresh append-only map that is insertion
order *per distinct name*, so `x-a: 1, x-b: 2, x-a: 3` iterates as `(x-a,1), (x-a,3), (x-b,2)`. The
`Vec` faithfully preserves whatever order it is given; the loss happens upstream of it, at
`HeaderMap::iter()`. **Inherited, not a regression** — the pre-existing header conversion at
`client.rs:244-250` has exactly the same property, and CF-111-9 reaches the same conclusion by a
blunter route. Suggest softening the two doc lines and cross-referencing CF-111-9.

**N-3 — CF-111-6's envoy-rust half is position-dependent and the contract states it flatly.**
`BEHAVIOR_CONTRACT.md:1000-1005` says "envoy-rust forwards the block's surviving non-pseudo fields".
That holds only when the pseudo-header appears **first**: `h2-0.4.16` `frame/headers.rs`'s
`set_pseudo!` macro sets `malformed = true` when a regular field already preceded the pseudo, which
becomes `Error::MalformedMessage`, surfaces as `Http2Error::H2RecvBody`, and yields a `503` — i.e. the
**CF-111-5** shape, not the CF-111-6 shape. Since a future phase will read this bullet to decide how
to close the divergence, it wants one clause: "(measured with the pseudo-header at the head of the
block; a pseudo-header after a regular field is rejected by `h2` as malformed and takes the CF-111-5
path instead)".

**N-4 — the last `### Still unmeasured` bullet is answerable from the pinned source, and the answer
is "no".** `BEHAVIOR_CONTRACT.md:1031-1032` asks "whether `h2`'s send-side validation would turn some
block Envoy accepts into an error here". `h2-0.4.16` `proto/streams/send.rs:312-328` calls
`check_headers` from `send_trailers`, and `check_headers` tests **exactly** the same conditions as the
receive side. Any block that survived receive validation survives send validation. It can be moved
out of "Still unmeasured" and recorded as resolved — which independently reinforces standing
rejection (1).

**N-5 — no test pins that a retried attempt's trailer block is discarded.** The property holds
structurally (§1), but M-3's pattern — "the plan says a unit test covers it" — argues for one cheap
test: an upstream returning `503 + trailers` on attempt 1 and `200` with none on attempt 2, asserting
the client sees no trailers. The differential fixture cannot express this (single backend mode).

**N-6 — `--trailers` and `--close-before-response` are not "mutually independent".**
`main.rs:46-47`'s doc comment says they are; `run()` at `:111-117` is a precedence chain
(`if close_before_response … else if trailers …`), so passing both silently selects the former and
drops the latter with no diagnostic. Latent — no caller passes both — but a future fixture that did
would go RED with no clue why. Note that **rejecting** the combination would be the deviation: no
helper in `tests/helpers/` implements mutual exclusion (checked across `tls-echo-server`,
`http1-echo-server`, and a grep for "mutually exclusive"). A comment correction is the right size.

**N-7 — two of the three new `diff_headers` trailer tests add no coverage.**
`diff_headers_rejects_a_missing_trailer` (`lib.rs:9321`) and
`diff_headers_rejects_a_differing_trailer_value` (`:9333`) are name-substituted copies of pre-existing
tests, and `diff_headers` is byte-identical across the range and entirely name-agnostic — feeding it
`x-trail-a` instead of `x-foo` exercises no new path. The third,
`diff_headers_accepts_equal_trailer_sets` (`:9308`), **does** earn its place: it is the only test in
the file asserting order-insensitivity (envoy `a,b` vs rust `b,a`). Also,
`diff_headers_rejects_a_differing_trailer_value` omits the error-message assertion its sibling makes,
so it would pass if the diff bailed for an unrelated reason.

**N-8 — the fixture README's "exactly two lines changed per side" is true only under an unstated
span.** It is exact for the *configuration*, which is what the sentence is about and what this review
verified; the files also differ in their comment headers (17 header lines on `0090/envoy.yaml`
against 7 on `0010/envoy.yaml`). `PROGRESS.md:639` states the same claim **with** its span
(`sed -n '/^node: {/,$p'`); the README drops it. This is the span-dependent-measurement trap the
ledger already records. One clause fixes it.

**N-9 — `0090` omits the vestigial `inputs/payload.bin` that `0010` carries**, and the README's "No
`inputs/` directory — the H2 driver does not read one" reads as a contrast the parent does not
support. The claim about the driver is true (only the TCP arms read `inputs/payload.bin`;
`run_http2_arm` never touches it) and omitting a tracked zero-byte file is an improvement — but a
future session diffing the two directories will hit an unexplained delta. One clause.

**N-10 — `backend_scan_sources` does not cover the reload template.** `lib.rs:3387-3394` covers six
templates, but `render_yaml` is additionally applied to a reload template at `:5957` and `:6071` with
the same kv lists, so a marker placed **only** in a reload template would render without spawning its
backend. Identical for all ten markers, long predates phase 111, and fixture `0090` carries no reload
template. Recorded only so a future marker author does not re-derive it.

**N-11 — small textual items, batched.** `backend.rs:545-546` "would be surface no fixture uses" is
missing an article. `backend.rs:959`'s test is inserted before `http2_echo_backend_spawns_and_echoes`
rather than after, inconsistent with the declaration order of the structs they test.
`helper-common/src/lib.rs:13`'s module doc cites `--close-before-response` as *the* example of a
per-binary flag; `--trailers` is now a second. `client.rs:216-217`'s "and, more importantly, while
`recv_stream` is still alive" is misleading — `recv_stream` is a local that lives to the end of
`send_request` regardless of where the status guard sits; the accurate half of the sentence is the
first half. `response.rs:95`'s `name.to_ascii_lowercase()` before `HeaderName::from_bytes` is
redundant (that constructor already normalises) but harmless and consistent with `build_http_response`.

**N-12 — `PLAN.md` §5's file table is one file short of what the phase touched.** It lists **10**
rows and omits `crates/envoy-http2/src/lib.rs`, which the phase does modify (+19 lines — the
`TrailerBlock` alias). The alias itself is standing rejection (2) and is not re-raised; the point is
only that a reader reconstructing the change set from §5 comes up one file short of the 13 the diff
actually carries. §5 also calls `tests/differential/tests/h2_response_trailers.rs` "the 19-line
auto-discovered runner"; it landed at **43** lines, all of the excess being module doc, and at the
same length as `grpc_aware_local_replies.rs` — the established precedent for a fixture whose
rationale is load-bearing. `PLAN.md` is landed and uneditable; noted for the record, not for repair.

**N-13 — `PROGRESS.md`'s crate-root census is one low.** The state-4 constraints table reads
"`#![forbid(unsafe_code)]` at every crate root | 21 roots checked, 0 missing". Enumerating every
`Cargo.toml` under `crates/` and `tests/` and resolving its `src/lib.rs`/`src/main.rs` gives **22**
roots. **The load-bearing half of the claim — zero missing — is correct**; only the denominator is
off. Cosmetic, and recorded only because a denominator that drifts silently is how a coverage census
stops meaning anything.

---

## §5 — Severity dissent, and subagent findings DOWNGRADED on re-verification

Seven subagent findings arrived graded **Important**. All seven are real; all seven are **DOWNGRADED
to Minor** here. §5.2 is the reason this section exists: an Issue at state 5 sends the work back to
state **3**, which costs three more sessions (3 → 4 → 5), and the grading must be worth that.

| finding | arrived as | recorded as | why |
|---|---|---|---|
| `Some(vec![])` is producible | Important ×3 (slices i, v, vi) | **M-1** | Real, and the only *code* defect in the review — reached independently by three reviewers via three different inputs, which is why M-1 records all three. Two of the three land on responses that already diverge for banked reasons; the third (a zero-field trailer block) moves a frame sequence that was at parity, and M-1 says so rather than smoothing it over. It stays a Minor because the change is invisible at the level every consumer reads — `Some(<empty>)` flattens to zero trailers, so status, body, headers and trailers all still agree — the harm is one extra empty frame, and the remedy is one line. Weighed against three more sessions, banking it is the proportionate call; if this review were wrong about anything, this is the finding it would be wrong about. |
| Task 1's RED is a compile error and the fork is unmutated | Important (slice vi) | **M-14** | Correct, and `PROGRESS.md` says so itself in the same breath rather than dressing the compile error up as an assertion failure. The fork's *behaviour* is nonetheless pinned by six wire-level tests over a real in-process H2 connection; what is missing is the mutation that proves those tests discriminate. Same fix as M-2, ~10 minutes in a scratch worktree. |
| "no DATA frame" unasserted | Important (slice i) | **M-2** | The landed code is **correct** on this row; what is missing is the assertion that keeps it correct. A maintenance risk, not a defect. Ten lines to close. |
| D-PLAN-6's coverage commitment unmet | Important (slice i), Minor (slice iv) | **M-3** | The strongest candidate for an Issue, and the one this reviewer weighed longest — a landed PLAN claim is false and `PROGRESS.md` does not disclose it. But the two uncovered cells are behaviourally low-risk (the fork does not branch on status or on trailer count), the third is arguably not worth testing at all (N-2), and the remedy is ~20 lines of test plus a one-clause documentation correction. Banking it costs one line in the next phase's carry-forward list; raising it costs three sessions. |
| the trailer rule is vacuity-permitting | Important (slice ii), Minor (slice iv) | **M-4** | The property is **not currently false** — the re-aimed mutation proves fixture `0090` bites on the trailer axis today. What is missing is a standing guard against a both-sides regression. Genuinely worth fixing; not worth a re-entry. |
| contract too strong (empty block; "verbatim") | Important ×2 (slice v) | **M-5**, **M-6** | Both are the contract over-claiming rather than under-claiming, which is the safer direction to be wrong in for a specification that is read before it is implemented against. Neither describes a behaviour a fixture or a gRPC client encounters. |
| 28 further stale `SPEC`/`PLAN` citations | Important-adjacent (slice vi) | **M-15** | Both documents are landed and uneditable, every citation was true at its own commit, and none is unresolvable. Drift, recorded so the next session locates by text. |

**Two subagent `file:line` citations do not resolve and are NOT reproduced above.** Slice (vi) cited
`crates/envoy-http2/src/response.rs:1016` and `:1055` for two emit-seam tests; that file is **523**
lines. The tests it meant are at `:449` (`trailers_follow_an_empty_body_with_no_data_frame`) and
`:488` (`trailer_names_envoy_forwards_are_not_stripped`), which this session resolved by text and
used throughout. The findings survive; the citations did not. Recorded because it is the review's own
small demonstration of the rule the review spends M-7 and M-15 on: **a subagent finding is a claim,
and so is its line number.**

**One subagent claim was DROPPED entirely.** Slice (ii) offered an md5 mismatch as evidence that
`diff_headers` had changed; on re-derivation the `awk` range had over-run into `DriveHttp1Result`,
which genuinely did change. Re-extracted over the correct span, `diff_headers` is byte-identical at
`0ba60db` and `6a790ab` (md5 `e9d7bd64fe8f23f24d2d0cdf064d79a1`), and so is `HEADER_ALLOW_LIST`. The
agent caught and corrected this itself; it is recorded because **the correct span is what settles the
claim**, and an md5 over a hand-chosen span is span-dependent.

**One finding this reviewer raised against the state-4 record was WITHDRAWN.** An initial census
returned `0` for the CI job log's `Running tests/<name>.rs` lines, which would have made the record's
gate-(b) derivation unreproducible. The zero was an artifact: the `Running` token is **ANSI
colour-coded** in a GitHub job log (`\e[1m\e[92m     Running\e[0m tests/…`), so `grep -c 'Running '`
matches nothing. ANSI-stripped, the census reads **90 found / 0 missing**. **The state-4 record is
correct and this reviewer's grep was wrong** — recorded in full because it is the same shape as the
`cargo deny` four-ok line the ledger already warns about, and because a review that only reported the
findings that survived would have hidden the more useful lesson.

---

## §6 — Deliberate decisions verified, not re-litigated

- **D3 / D-PLAN-2, trailers ALONGSIDE `Response`.** Upheld. `envoy_http1::Response` gained no field;
  `git diff 0ba60db 6a790ab -- crates/envoy-http1/` touches nothing. The `E0063` argument is correct
  and is reinforced by the `PartialEq`/`Eq` derive hazard the doc comment names — a fifth field would
  have silently redefined every whole-`Response` equality assertion in the tree.
- **The `TrailerBlock` alias is a spelling change, not a design deviation.** Confirmed:
  `crates/envoy-http2/src/lib.rs:61` is `pub type TrailerBlock = Option<Vec<(String, String)>>`, a
  transparent alias introduced because the nested form trips `clippy::type_complexity` at the retry
  loop's `AcquireOutcome::Sent`. D-PLAN-2's "the trailer type is `Option<Vec<(String, String)>>` at
  every production hop" remains literally true. **Not graded as a departure.**
- **D-PLAN-4, no defensive hop-by-hop strip.** Confirmed, and independently strengthened: `h2-0.4.16`
  rejects the six conditions on the **receive** side (`frame/headers.rs`'s `HeaderBlock::load`, the
  *shared* header-block decoder that serves trailer frames too) **and** on the send side
  (`proto/streams/send.rs`'s `check_headers`, called from `send_trailers`). The strip would be dead
  twice over. The reasoning is recorded in a doc comment at `build_trailer_map`
  (`response.rs:74-91`) exactly as the PLAN required. **Not re-raised.**
- **D-PLAN-1, EMIT → READ → THREAD.** Confirmed against the commit order (`d94b3c0` emit, `d32dd04`
  read, `7b94194` thread). The stated rationale — that a threading-first order gives Tasks 1 and 2 no
  observable behaviour and therefore no honest TDD RED — is sound and is why the emit-seam tests are
  wire-level rather than shape-level.
- **`PLAN.md`'s two code defects and its misaimed Task-9 mutation.** All three confirmed as already
  recorded in `PROGRESS.md` with their fixes. `PLAN.md` is landed and was **not** patched. Not
  re-discovered here.
- **The five-member local flake set.** Not re-examined; environmental, CI-authoritative, and this
  review ran no `cargo`.
- **Non-goals 1–9.** All held. No `crates/envoy-filter/` change (0 files); no H1 trailer work; no
  request trailers; no gRPC data-path filter; no `%TRAILER`/`%GRPC_STATUS`; no trailers on local
  replies (explicitly cleared); no `ROADMAP.md` row repair; **no new config surface, no new
  dependency, no `Cargo.toml`/`Cargo.lock`/`ci.yml`/`deny.toml` change, and `HEADER_ALLOW_LIST`
  byte-identical at 3 entries**; no `uring.rs` change.

---

## §7 — Status of the three findings banked for this review

All three were handed to this session already found. Each is **CONFIRMED on disk**, and none is
re-issued as a fresh discovery.

**(1) `PROGRESS.md`'s three inherited `file:line` citations have all drifted — CONFIRMED, all three,
to the line.**

| citation | at `0ba60db` | at `6a790ab` / HEAD | delta |
|---|---:|---:|---:|
| `hcm.rs:1043` — the sole production `send_envoy_response` caller | 1043 | **1096** | +53 |
| `lib.rs:1189` — `HEADER_ALLOW_LIST` | 1189 | **1212** | +23 |
| `lib.rs:1261` — `load_expectations` | 1261 | **1291** | +30 |

Each was TRUE when the pre-flight table measured it and each was moved by the phase's own insertions.
None is load-bearing for any gate. **M-7 adds a fourth of a materially different kind** — one that no
per-document audit of these three artifacts would have found — and **M-15 adds twenty-eight more**
across `SPEC.md` and `PLAN.md`, which is the point: the three banked here are not the population,
they are a sample of it.

**(2) State 3's dated-block-header census was MISLABELLED — CONFIRMED exactly.** State 3 recorded
"anchored 52 / naive 55". Re-measured at `6a790ab` the three forms read **ANCHORED 49 / NAIVE 52 /
BARE 55**. So 52 and 55 are genuine measurements reported **one label to the left**, and the anchored
count had never been taken. This is the sharper version of the standing warning: *two numbers
agreeing with a prior record is not confirmation when the record may have mislabelled which
measurement they are.* State the PATTERN alongside every count. Series, anchored form: `0ba60db` 48 →
`111b34a2` 49 → `6a790ab` 49 → HEAD **50**.

**(3) Net LoC 1525 against the plan's ≈916 — CONFIRMED, and it is 1.66×, 25 lines over the ~1500
§6.1 threshold.** Re-derived at the carrying range `0ba60db 6a790ab` excluding `docs/`: added 1559,
deleted 34, **net 1525**. §6.1 does **not** fire retroactively — the gate reads on the state-2
*estimate*, which cleared it at ≈916, and its mid-execution trigger is per-task sub-step count,
evaluated at Task 3 and not fired. **This is not graded as a defect and no split is asked for**; ten
tasks are landed, green, and mutation-proved. It is a third datapoint for the unlanded
`.claude/drafts/DRAFT-ADR-split-thresholds.md`, and the sharpest of the three: **a phase that cleared
the gate on its estimate would have failed it on its actual**, while being by every other measure a
clean, well-scoped, single-cell phase whose §7.5 gate then passed five-for-five. Recorded; not acted
on.

---

## §8 — Carry-forwards for the state-6 close-out to bank

**Opened by this review:** M-1 … M-15 and N-1 … N-13 above.

**Opened by the phase and carried UNCONSUMED** (§6.3; ADR-0165 — a phase banks, it never clears):
**CF-111-1** (trailers bypass the filter pipeline), **CF-111-2** (H1 trailers, blocked behind chunked
response encoding), **CF-111-3** (REQUEST trailers), **CF-111-4** (`%TRAILER(…)`/`%GRPC_STATUS%` —
see M-9), **CF-111-5** (connection-specific trailer name ⇒ envoy-rust 503 vs Envoy 200+RST;
pre-existing, in the `h2` codec), **CF-111-6** (pseudo-header trailer — **LIVE, a divergence this
phase CREATED**; see N-3 for its position-dependence), **CF-111-7** (Envoy's `http2.trailers` stats
exist and stay 0), **CF-111-8** (duplicate trailer names unassertable under `diff_headers`),
**CF-111-9** (trailer wire ORDER doubly invisible).

**Carried forward from earlier phases, INTACT and unconsumed:** the `110.2` REVIEW's M-1…M-8 +
N-1…N-12; the `110.1` REVIEW's M-1…M-9 + N-1…N-10; CF-110-1…CF-110-9; CF-109-1/2/3; CF-108-1/2/3;
CF-76-1; CF-75-2/3/4/5/6; CF-72-2/CF-75-1; M71-6; CF-74-1/2/3/4/6; CF-73-1; the `109.2`, `109.1` and
`108.2` REVIEW sets; and the HTTP-filters-family (1)–(4).

**Nothing was fixed by this session.** No code file, no landed artifact (`SPEC.md` and `PLAN.md`
included), no `ROADMAP.md` row, no ADR, and no `stop` file.

**If a follow-up phase wants a natural first task**, M-1 + M-2 + M-3's non-200 test + M-4's non-empty
guard total roughly 40 lines across three files, and M-14's single mutation pass costs about ten
minutes. Together they close the five findings this review would most like closed, and M-1 closing
in code makes M-5 disappear on its own.

---

## §9 — Assessment

Phase 111 did the load-bearing thing well. Its job was to move a value the codec already had — an
`Option<http::HeaderMap>` sitting unread on `RecvStream` — across four hops to a `send_trailers` call,
and the interesting problem was never the plumbing. It was **containment**: keeping the value off the
shared `envoy_http1::Response` that four crates construct at 42 sites and that derives `PartialEq`,
and then making sure that a value riding *alongside* a struct rather than *inside* it does not leak
onto the twelve response paths that must never carry it. Both halves were solved, and the second was
solved better than the plan described: by putting `trailers` on `H2AttemptResult` as a **field**, the
implementation converted five of its six clear-sites into `E0063` errors, and by destructuring the
request-path match into a typed tuple it converted the rest. Only one clear is hand-written, and it
carries its own test whose doc comment names the hazard it exists for. That is the difference between
a phase that remembers to clear a local and a phase that cannot forget to.

The emit fork deserves specific credit for what it is not. It is not a `match` on a two-field tuple,
and it is not four branches. It is three expressions in which `trailer_map.is_none()` appears twice,
and the consequence is that the no-trailers path reduces **token-for-token** to the pre-phase code. D4
asked for the 89 pre-existing fixtures to be byte-identical on the wire; the implementation made that
true by construction rather than by test, which is the only version of that promise that cannot rot.

The phase is also unusually honest about the limits of its own witness. Fixture `0090` probes one
cell, and the four cells it declines are each declined **on a measurement**, with the measurement
named: a forbidden-name trailer would red for a reason predating the phase (CF-111-5), a pseudo-header
trailer would red on a divergence the phase itself creates and is unwilling to hide (CF-111-6),
duplicate names are unassertable under a set comparison (CF-111-8), and no stat may be asserted
because Envoy's own trailer stats stay zero (CF-111-7). The `BEHAVIOR_CONTRACT.md` section carries all
of that forward, including the one divergence the phase created, under a heading that cannot be
mistaken for parity. And when the plan's own Task-9 mutation went RED, the session read **which cell**
it named, found it was framing rather than trailers, and re-aimed it — producing the
`only-in-envoy=["x-trail-a", "x-trail-b"]` string that is the single best piece of evidence in the
whole phase.

The defect profile has one shape, and it is worth naming because it recurs five times: **a claim was
made in prose that nothing in the tree holds.** `crates/envoy-http2/src/lib.rs:46` says
`Some(vec![])` is never produced, and it is, on three separate inputs (M-1). A test name says "no
data frame" and its body says "no bytes" (M-2). `PLAN.md` says three cells are covered by unit tests
at the emit seam, and one of them is (M-3). The helper's doc comment says fixture `0090`'s body
comparison protects an invariant that fixture deliberately does not assert (M-11). The contract says
"verbatim" where the code silently drops non-ASCII values (M-6), and files under MATCHES a cell whose
envoy-rust column was measured before the fix existed (M-5). Only one of these — M-1's zero-field
input — changes anything the proxy does, and it changes one frame on a response no fixture drives.
All of them change what a future session will believe, which in a project whose entire method is
context-isolated sessions reading each other's documents is not a small category.

There is a matching gap on the evidence side, and it lands on the one file where it costs most.
Task 1 is the phase's riskiest change and its recorded RED is a compile error; the discriminating
term of the four-row fork, `body_empty && trailer_map.is_none()`, was never mutated (M-14). The
phase's mutation discipline elsewhere is exemplary — Task 4 asserts its target occurs exactly once
before `sed`ing it, shows a `Compiling` line so the binary is not stale, shows a `test result:` line
so the RED is a failure and not a compile error, and runs an unmutated control from the same seeded
worktree — which makes the omission at Task 1 conspicuous rather than characteristic. It is roughly
ten minutes of work, and it would close M-2 at the same time.

**M-7 is the one worth remembering.** Three drifted citations were banked for this review, and all
three are of the kind the repository already knows — a phase's document citing into files that phase
edited. The fourth is not in any of the three artifacts a reviewer is pointed at. It is at
`BEHAVIOR_CONTRACT.md:796`, inside the **gRPC** section, written by a previous phase, broken by this
one, because this phase inserted 23 lines above `HEADER_ALLOW_LIST` in a **shared source file**.
`110.2` M-1 named the self-invalidating citation; this is a document invalidating a **neighbour's**.
The rule that follows is a real generalisation: when a phase inserts into a shared canonical document
or a shared source file, the citations it must re-resolve are not the ones in its own artifacts — they
are every citation *anywhere in the repository* that points below the insertion point.

Nothing found changes a byte of wire behaviour on any path a proxied response, a gRPC client or a
fixture takes. Nothing found weakens a gate. Nothing found is worth three more sessions under §5.2.
The three-way fork is correct on all four measured rows and byte-identical on the two that must not
move, the error widening cannot reach an abort anywhere in the workspace, the local-reply clears are
complete over a compiler-enforced enumeration, fixture `0090` is `0010` verbatim plus two
configuration lines per side and is mutation-proved to bite on the trailer axis, and CI is green on
the exact code tree with all 90 differential binaries demonstrably executed and `h2_response_trailers`
among them.

**Gate (f) is CLOSED. All six §7.5 gates are now GREEN. Phase `111` is approved to land.**

### STOP CONDITION — re-derived from disk at this review, ALL THREE LEGS

The mission (feature-complete Envoy-in-Rust against the `ENVOY_TARGET.md` v1.33.0 pin) is complete
only when EVERY `ROADMAP.md` row is `done` **AND** no in-scope leaf remains. **ALL THREE LEGS MUST
HOLD. IT IS NOT COMPLETE.** This is the **seventy-third** consecutive evaluation by the ledger's
running count, and all three legs were measured independently and freshly at
`231aba8de521596ba60dbf75b2cd09fcda40a316`.

- **Leg (i) — FALSE.** **117 rows / 116 `done` / 0 `in-progress` / 1 `planned`.** The single
  not-`done` row is `111` itself, which stays `planned` until its own state-6 close-out. Driven from
  the `^\| [0-9]` prefix with status read as **field 4** on a `' | '` split. ⚠ An `NF == 6` filter
  returns **ZERO** rows here — every row begins `| `, so the split yields a leading empty field and a
  normal row has **7** fields, while the two rows carrying an unescaped in-cell pipe (38, 39) have
  more. That census reports a clean, believable `0` and would make this leg look **vacuously TRUE**.
- **Leg (ii) — FALSE**, by direct tree probes rather than by the ledger's assertion: **14** crates
  (`envoy-accesslog envoy-admin envoy-bin envoy-cluster envoy-config envoy-filter envoy-health
  envoy-http1 envoy-http2 envoy-jwt envoy-listener envoy-stats envoy-tcp envoy-tls`), with no
  `envoy-http3` / `envoy-grpc` / `envoy-wasm` / `envoy-protos` / `envoy-runtime`;
  `quinn` / `tonic-web` / `wasmtime` = **0** across `crates/*/Cargo.toml`; `tests/conformance/` holds
  **only** `h2spec/`. The unbuilt set still includes the whole gRPC DATA path (blocked on the SECOND
  prerequisite, the headers-only filter API), RTDS, hot restart / graceful drain, HTTP/3 + QUIC, the
  observability sinks and the WASM host.
- **Leg (iii) — FALSE.** **11** `### ` family headings, of which **TWO carry ZERO rows**
  (`### HTTP/3 + QUIC family`, `### WASM host family`); the other nine read
  10 / 5 / 3 / 14 / 4 / 6 / 29 / 6 / 13. ⚠ Driven from a single `/^### /` rule plus a `/^\| [0-9]/`
  row rule — an `awk` whose first rule matches `/^### .* family/` and then calls `next` never reaches
  a later `/^### /` rule and under-reports.

**NO `stop` FILE WAS CREATED**; `ls stop` returns `No such file or directory`. A human asking for a
conditional `stop` file is an instruction to **evaluate the condition**, not evidence that the answer
changed.

### Next state

**§5 state 6 — the CLOSE-OUT**, a SEPARATE session per §5.1 and ADR-0127 (a reviewer must not close
out what it graded). That session is the **only** one that flips `ROADMAP.md` row `111` from
`planned` to `done`, and it flips the **status cell only** — nothing else, even if a handoff asks for
more. Assert the row's starting status first. A close-out adds **no ADR** and **no Notes
subsection**; its Notes go to a **NEW EOF section**, never an existing header a handoff names. The
`### Phase-111 §5 state-5 code review` Notes subsection is retired to `STATE_HISTORY.md` at that
close-out.

The next-phase pick — a phase `112`, or the repo-health candidate drafted in `.claude/drafts/` — is
its own session **after** that. **A close-out and a pick are never chained.**

Per §5.2, the alternative disposition available to this review was a re-entry at state **3**, not
state 4. It was weighed (§5) and declined: **the verdict is an approval.**

This review **fixed nothing**, as ADR-0165 requires, and touched no `ROADMAP.md` line, no landed
artifact (`SPEC.md` and `PLAN.md` included), and no file under `crates/`, `tests/` or
`.github/`.
