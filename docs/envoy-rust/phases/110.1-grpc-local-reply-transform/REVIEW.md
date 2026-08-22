# Sub-phase 110.1 — gRPC-aware local replies over HTTP/1.1: the three pure functions, the transform, and the H1 local-reply SEAM at BOTH wire funnels — CODE REVIEW

**Verdict: APPROVED-WITH-MINORS.**

Per `BOOTSTRAP_PROMPT.md` §7.5, an approved `REVIEW.md` closes gate **(f)** — the only gate
still open. Gates (a)–(e) were run and adjudicated by the §5 state-4 verification session and
are recorded, with actual command outputs, at `PROGRESS.md` `# Sub-phase 110.1 — §5 state-4
VERIFICATION`. **This review did not re-run them and does not re-adjudicate them** (§5.1;
ADR-0127 — the context that ran the gate must not grade it, and the context that grades it must
not fix it). It re-confirmed CI on the exact tree under review independently (§0.3), because
that is a fact about the commits rather than a re-run of the gate.

**Zero Issues. Nine Minors, ten Nits.** Not one finding is a wire-behaviour defect: every
measured cell of the upstream contract is implemented correctly, the seam is installed at both
H1 wire funnels and at neither shared builder, HTTP/2 is isolated by the COMPILER rather than by
convention, and the `outgoing_local` locality predicate is correct at every one of the sixteen
local-reply sites. The findings are concentrated in **test ADEQUACY** — three assertions that
are weaker than they look, one that is vacuous by construction on a live path, and a family
coverage table that promises more than its own test list delivers — plus one arithmetic
citation. Per §6.3 and ADR-0165 **nothing was fixed by this session**. **No §5.2 re-entry to
state 3 is required** — the verdict is an approval and gate (f) is CLOSED; every Minor and Nit
below is BANKED for the state-6 close-out to carry and for sibling `110.2` to weigh.

The three findings the state-4 verifier banked (V-1, V-2, V-3) were **re-derived from disk, not
accepted** — §8 disposes of each. All three are CONFIRMED. **V-2's underlying soundness claim
survives at the corrected boundary, but this review finds that the claim was scoped to the wrong
FILE SET all along** (M-3): surveying `hcm.rs` cannot see the encode-side filter pipeline, which
runs 64 lines before the seam and can inject any header name with no denylist. That correction
is this review's single most useful output.

---

## §0 — How this review was conducted

### §0.1 — Scope

The tree under review is `main` at `577a56746e95700300b81cc065da1593d382a694`, clean
(`git status --porcelain` = 0 lines), with `origin/main` at the same commit after a
`git fetch origin --prune` whose exit code was checked (`FETCH_EXIT=0`). The §5 state-5
detection rule was re-verified on disk rather than taken from the handoff:
`docs/envoy-rust/phases/110.1-grpc-local-reply-transform/` holds `SPEC.md` + `PLAN.md` +
`PROGRESS.md` and **no `REVIEW.md`**; `STATE.md` `## Active phase` `**id:**` reads `110` SPLIT
with the active pointer on `110.1`; `ROADMAP.md` reads row `110` `in-progress` and rows `110.1`
/ `110.2` `planned`. **`STATE.md` and `ROADMAP.md` AGREE**, so no `superpowers:systematic-debugging`
detour was needed. `ls stop` returns `No such file or directory`.

The implementation under review is the eight-commit range `eeb45d0..29d25e5`:

```
$ git diff --numstat eeb45d0 29d25e5 -- . ':(exclude)docs/'
6	6	Cargo.lock
709	0	crates/envoy-http1/src/grpc.rs
428	0	crates/envoy-http1/src/hcm.rs
6	0	crates/envoy-http1/src/headers.rs
4	0	crates/envoy-http1/src/lib.rs
27	9	crates/envoy-http1/src/uring.rs
125	0	crates/envoy-http2/src/hcm.rs
```

Three docs-only commits sit on top (`e89d278` the state-4 advance, `a1e2cdd` and `577a567` the
CI records); they carry no executable line and are reviewed only for the accuracy of what they
assert.

### §0.2 — Method

Four independent read-only reviewers were dispatched over DISJOINT file regions — and the
disjointness claim was itself verified by listing the regions before parallelizing on it, not
assumed: `crates/envoy-http1/src/grpc.rs`'s test module (`#[cfg(test)]` at `:218`),
`crates/envoy-http1/src/hcm.rs`'s fourteen new tests (`#[cfg(test)]` at `:2579`; the new block
at `:11252-11632`), `crates/envoy-http2/src/hcm.rs`'s single new test (`:7269-7393`), and the
`PROGRESS.md` / `PLAN.md` citation audit. Three distinct files, one document set — genuinely
disjoint. Every reviewer was barred from writing and from running `cargo` (the cargo lock
serializes, and a state-5 changes no code), and every one was handed V-1 / V-2 / V-3 up front so
it would not re-derive them naively.

**Every finding below was re-verified on disk by this session before being written down** — a
subagent finding is a claim. Three subagent findings were DOWNGRADED and one was materially
reframed on re-verification; §5 records those dissents by name. The decisive measurements — the
funnel census, the `outgoing_local` correspondence, the per-request scoping, the encode-filter
injection path, the size arithmetic, and the 33-test ledger — were all made by this session
directly.

### §0.3 — CI re-confirmed independently on the exact tree under review

Not inherited from the handoff. The full 40-char SHA was interpolated from `git rev-parse HEAD`,
never retyped — a short or retyped SHA silently returns `[]`:

```
$ gh run list --commit 577a56746e95700300b81cc065da1593d382a694 --json databaseId,status,conclusion,headSha
[{"conclusion":"success","databaseId":32257501630,
  "headSha":"577a56746e95700300b81cc065da1593d382a694","status":"completed"}]

$ gh api repos/pgdad/envoy-rust/actions/runs/32257501630/jobs \
   --jq '.jobs[] | {name, id, conclusion, runner_name, steps: (.steps|length)}'
{"conclusion":"success","id":96082731532,"name":"build + test + lint",
 "runner_name":"GitHub Actions 1000005385","steps":15}
{"conclusion":"success","id":96082731921,
 "name":"fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse + grpc_health_decode,...",
 "runner_name":"GitHub Actions 1000005386","steps":13}
```

**Steps 15/13, both jobs enumerated via the jobs API and selected BY NAME, both with REAL runner
names — not the `runner_name:""` + `steps:0` starvation shape.** The `runner_name` field lives
in the jobs API; `gh run view --json jobs` returns `runner: null`, which is why the jobs API is
the one that settles it.

### §0.4 — The test-count identity closes exactly, and it was closed FROM THE SOURCE

The identity `2227 − 2194 = 33` proves the count MOVED by 33; it does not prove WHERE. Counted
directly at HEAD by this session:

```
$ grep -c '#\[test\]\|#\[tokio::test\]' crates/envoy-http1/src/grpc.rs
18
$ <python: test attribute within 3 lines above each `fn grpc_*|non_grpc_*` in hcm.rs>
hcm.rs grpc_*/non_grpc_* TESTS: 14   NON-test: ['grpc_no_healthy_upstream_config']
$ grep -c 'async fn h2_route_decision_reply_is_not_grpc_transformed' crates/envoy-http2/src/hcm.rs
1
```

**18 + 14 + 1 = 33 — accounted for by name.** The `grpc_no_healthy_upstream_config` trap (a
`grpc_`-prefixed *builder* that a name-based count over-counts) was independently re-detected
and correctly excluded, exactly as `PROGRESS.md` states.

### §0.5 — Standing censuses re-derived at HEAD

Every one of these is asserted UNCHANGED by `PROGRESS.md`, and every one was re-derived here
rather than inherited:

```
fixtures (git ls-files 'tests/fixtures/**' | cut -d/ -f3 | sort -u | wc -l) : 88
differential test files                                                     : 88
fuzz targets                                                                :  5
crates                                                                      : 14
phase directories                                                           : 120
known-failures.txt                                    : 21 lines / 1 real entry
HEADER_ALLOW_LIST                                     :  3 entries; `location` NOT present (0)
ADR head                                              : ## ADR-0179 ; ADR-0180 = 0 (UNRESERVED)
ROADMAP (split on ' | ', status = FIELD 4)  : 116 rows / 113 done / 1 in-progress / 2 planned
```

`BEHAVIOR_CONTRACT.md` carries a `## Active gRPC health check (grpc_health_check)` section and
**no gRPC local-reply section** — correct: that section is `110.2`'s, and PLAN non-goal 9 holds.

---

## §1 — Strengths

**The seam placement is the best decision in the sub-phase, and it is the one that is genuinely
witnessed.** Both SPECs and the inherited handoff located the tokio seam at the wire funnel
(`hcm.rs:1457`/`:1468`). ADR-0179 DECISION 2 overturned that on measurement, and the landed code
puts the call immediately before `let response_status_for_log` (`hcm.rs:1491` vs `:1497`) —
because those two locals drive BOTH the access-log record and the per-class counter dispatch.
A wire-write placement would have been byte-correct on the wire and silently wrong in the access
log and the stats. The state-3 placement mutation reds **exactly** the two witnesses and leaves
the other fifteen wire-shape tests green, which is the positive proof that placement is enforced
rather than merely documented. This is the seam-placement lesson in its purest form: **no
wire-shape test can see placement, so the observability witnesses are the whole gate.**

**The access-log witness is materially stronger than the PLAN asked for.** The PLAN specified an
assertion through a `log.records()` API that does not exist. Rather than invent one, the
implementer asserted the log LINE byte-exactly:

```rust
assert_eq!(logged, "{\"bytes\":0,\"rc\":200,\"rcd\":\"direct_response\"}\n", ...)
```

That single equality pins all three of measurement N-2's claims at once — `%RESPONSE_CODE%` = 200,
`%BYTES_SENT%` = 0, and `%RESPONSE_CODE_DETAILS%` UNCHANGED at `direct_response`. Whole-output
equality where the plan proposed a field lookup is the right instinct, and it is declared as D-3.

**HTTP/2 isolation is enforced by the COMPILER, not by convention — and that is stronger than
what the SPEC asked for.** `pub(crate) mod grpc;` (`lib.rs:21`) with no `pub use` re-export makes
`envoy_http1::grpc::…` from `envoy-http2` an E0603 hard error, and `apply_grpc_local_reply` is
itself `pub(crate)`. Two independent barriers, either alone sufficient. The transform occurs at
**exactly two** places workspace-wide — `hcm.rs:1491` and `uring.rs:525`, one per wire funnel —
and at NEITHER `synth_with` (definition `:2286`, four real call sites `:2309`/`:2317`/`:2457`/
`:2472`) nor `build_response`/`build_response_in`. Every `grpc` token in
`crates/envoy-http2/src/hcm.rs` sits at `:7330+`, far below that file's `#[cfg(test)]` boundary at
`:1297` — **not one line of H2 production code mentions gRPC.**

**The locality predicate is correct at every site, and it fails SAFE.** This session traced it
directly rather than accepting it. `upstream_response: true` is constructed at exactly two sites
(`hcm.rs:591` the direct-head proxied path, `:621` the owned proxied response); every
synth-bearing `AttemptResult` sets it false (`:413`, `:642`, `:652`, `:664`). The retry loop has
a single break site (`:1290-1294`) propagating the bit unchanged, and `outgoing_local =
!completing_upstream_response` (`:1383`) is the only place it is ever cleared. The
retry-limit-exceeded path — where a REAL upstream 5xx is surfaced downstream verbatim — carries
`upstream_response == true` and is correctly NOT transformed. The request-budget overflow never
reaches `:1383`, so the `:1005` default of `true` stands, which is exactly the "covered by
omission rather than skipped by omission" property the declaration comment claims. And
`let mut outgoing_local = true;` sits at 8-space indent inside the `loop {` at 4-space indent
(`:792`/`:1005`), so **there is no keep-alive leak between requests on one connection.**

**The io_uring seam is installed in the one place that cannot be forgotten.** The transform lives
INSIDE `write_owned` (`uring.rs:525`) rather than at its four call sites, and all four sites write
synthetic local replies while the proxied path uses a different writer (`write_head_body` at
`:376`, untransformed — correct per CF-110-2). Both ordering invariants hold on disk:
`cluster.record_response` stays BEFORE the write so outlier detection records the ORIGINAL 503,
and `tick_class` stays AFTER it so the per-class counter sees the TRANSFORMED 200 — matching the
tokio path and measurement N-2. In a file with no test harness at all, structural
unforgettability is the right engineering answer, and the plan says so plainly rather than
implying coverage that does not exist.

**The pure functions are pinned about as hard as pure functions can be.**
`http_to_grpc_status` is swept across the ENTIRE `u16` domain with a counted eight-entry special
set, so neither a range arm nor an extra entry can land unnoticed; all twenty measured cells,
including the eight counter-intuitive `2`s (`405`, `408`, `409`, `412`, `413`, `499`, `500`,
`501`), are asserted independently. `grpc_message_encode` gets all nine measured byte-exact pairs
PLUS a 256-value property sweep PLUS a separate uppercase-hex test, making both boundaries
(0x1F/0x20 and 0x7D/0x7E), the `%` carve-out, `0x7F`, and per-byte UTF-8 (`é` → `%C3%A9`)
triple-covered. Detection covers all fourteen measured cells with both real traps directly
witnessed — `grpcfoo`/`grpc-web` against a naive `starts_with`, `APPLICATION/GRPC` and
`; charset=utf-8` against a lenient match — and correctly declines to build trailing-space
tolerance into the matcher.

**The idempotence guard was found by running the PLAN's own code, not by reading it.** The PLAN
specified both `transform_is_idempotent` AND an implementation that fails it. The implementer ran
it, quoted the failure, diagnosed the root cause (four emitted headers, only two dropped on
input, and the mapped code recomputed from the already-rewritten `200`), and correctly rejected
the obvious-but-wrong fix of dropping the other two. **A PLAN's own code is a claim — and here
running it was what caught the defect.** Equally, the Task-3 mutation `sed` was refused by a
`count == 1` guard because the anchor string occurs twice — once in the implementation and once
in a test that recomputes the predicate — which would have moved both in lockstep and returned a
GREEN that read as "these tests are vacuous". Both catches are recorded honestly as deviations.

**The blast radius is structural, not merely observed.** `grep -rn 'application/grpc\|grpc-status\|
grpc-message\|te: trailers' tests/` returns ZERO across the entire test tree, so no existing
fixture or test CAN red from this surface. That is a stronger regression-equivalence argument
than "the suite went green", and it was re-derived independently at both state-3 and state-4.

---

## §2 — Issues (Must Fix)

**NONE.**

Stated plainly, because §5.2 makes this the load-bearing sentence of the review: any Issue here
would send the work back to §5 state 3, not state 4. There is no Issue. Every measured cell of
the §1.1 mapping matrix, the §1.2 detection rule, the §1.3 encoder rule and the §1.4 wire shape
is implemented correctly; the seam is at both H1 funnels and at neither shared builder; HTTP/2 is
compiler-isolated; the locality bit is right at all sixteen sites; and no local reply reaches the
wire untransformed. The nine Minors below are test-adequacy and citation findings, and the ten
Nits are cosmetic. **Not one changes a byte of wire behaviour.**

---

## §3 — Minor

### M-1 — `resp.reason = None` is a live contract cell whose ONLY assertion is vacuous by construction, and the path that reaches it with `Some(..)` is a shipping production filter

The transform sets `resp.reason = None` (`crates/envoy-http1/src/grpc.rs:213`) so that
`serialize_response_head` falls back to `canonical_reason(200)` and emits `HTTP/1.1 200 OK`. The
sole assertion of that cell is `grpc.rs:341`:

```rust
assert_eq!(resp.reason, None, "reason must fall back to the canonical 200 OK");
```

but the only `Response` constructor in the whole test module hard-codes `reason: None`
(`grpc.rs:306`), so the assertion is checking a field the fixture never set. **Deleting
`grpc.rs:213` leaves the entire suite green**, and `sed -n '218,709p' … | grep -c "reason: Some"`
returns **0**.

The cell is not hypothetical. This session traced the whole chain on disk:

```
$ grep -n 'reason: Some' crates/envoy-filter/src/local_rate_limit.rs
148:                reason: Some("Too Many Requests"),
$ grep -n '^#\[cfg(test)\]' crates/envoy-filter/src/local_rate_limit.rs
260:#[cfg(test)]                     <- :148 is PRODUCTION code
$ sed -n '933,940p' crates/envoy-http1/src/hcm.rs
            envoy_filter::Decision::StopAndSend(filter_resp) => {
                RequestPath::SynthFromDecode(Response {
                    status: filter_resp.status,
                    reason: filter_resp.reason,      <- carried verbatim
$ sed -n '99p' crates/envoy-http1/src/response.rs
    let reason = resp.reason.unwrap_or_else(|| canonical_reason(resp.status));
```

`local_rate_limit` (phase 09, the first production filter to emit `StopAndSend`) sets
`reason: Some("Too Many Requests")`; that reaches `outgoing` at `hcm.rs:1406` with
`outgoing_local` at its default `true`; the seam runs. **Without line 213 a gRPC-detected
rate-limited reply would put `HTTP/1.1 200 Too Many Requests` on the wire** where upstream emits
`HTTP/1.1 200 OK`. Nothing outside `grpc.rs` pins it either — both filter-driven seam tests in
`hcm.rs` hand-build `reason: None`.

**The implementation is CORRECT. The witness is vacuous.** Remedy is one line: give `resp_with`
a `Some(..)` reason in one existing transform test, or add a single case. This is the sharpest
test-adequacy finding in the review and the one a later slice should fix first.

### M-2 — the idempotence sentinel's soundness argument surveys the wrong FILE SET: the encode-side filter pipeline runs 64 lines before the seam and can inject `grpc-status` with no denylist

`grpc.rs:158-160` returns early when the response already carries `grpc-status`, and the comment
above it justifies the sentinel as follows:

```
// No local reply carries `grpc-status` before this point: the whole synth
// family in `hcm.rs` contains no `grpc` string at all outside its tests.
```

State-4 finding V-2 re-derived that at the corrected boundary and confirmed it — but **both the
comment and V-2 scope the survey to `crates/envoy-http1/`**, and by construction such a survey
cannot see an operator-supplied literal string. The encode-side filter pipeline runs at
`hcm.rs:1427`; the seam runs at `:1491`. Between them sits any configured filter's
`encode_headers`, and `header_mutation`'s applies arbitrary operator-configured mutations to the
response header vector with no header-name denylist:

```
$ sed -n '74,77p' crates/envoy-filter/src/header_mutation.rs
    pub(crate) fn encode_headers(&mut self, resp: &mut FilterResponse) -> Decision {
        apply_mutations(&mut resp.headers, &self.response_mutations);
        Decision::Continue
    }
$ grep -n 'pipeline.encode_headers\|apply_grpc_local_reply' crates/envoy-http1/src/hcm.rs | awk -F: '$1<2579'
1427:        match pipeline.encode_headers(&mut filter_resp) {
1491:            crate::grpc::apply_grpc_local_reply(&mut outgoing, &req.headers);
```

`map_entry` (`header_mutation.rs:80+`) only lowercases the key and rejects two unsupported append
actions; there is no reserved-name check anywhere in `envoy-config`'s `HeaderValueOption`
validation. **Consequence: a route configured with `header_mutation` adding `grpc-status` to
responses — or a `local_rate_limit` whose `response_headers_to_add` carries it — makes the guard
fire on every local reply on that route and SUPPRESSES the entire gRPC transform.** Upstream
Envoy has no such sentinel.

Two things keep this a Minor rather than an Issue. First, reachability requires an operator to
configure a filter to add literally `grpc-status` to responses, which is obscure. Second, the
divergence direction is UNMEASURED — this session did not probe upstream under that config, and
a state-5 does not run Docker probes for a corner the phase never claimed. What IS certain, and
is the finding, is that **the sentinel's stated soundness argument is incomplete**: it is a
survey of `hcm.rs` offered as a proof about all reachable response headers, and the encode-filter
pipeline falsifies the scope. Sibling `110.2` should either measure the cell or narrow the
comment to what it actually establishes.

### M-3 — nine of sixteen local-reply sites are undriven by any gRPC test, and the over-claim is INHERITED from the PLAN's own Task-6 family table

`PLAN.md`'s Task-6 table assigns to "**this task**" the four `run_attempt` `synth_status(503)`
sites (`:463`/`:492`/`:509`/`:638`) and the two `synth_overflow` sites (`:470`/`:477`,
`serve_connection:1078`). Its Step-1 test list then specifies **eight** tests, none of which
drives any of them:

```
$ grep -n 'async fn ' PLAN.md | awk -F: '$1>=1600 && $1<=1750'
1625: grpc_transforms_synth_400_bad_host          1676: grpc_transforms_filter_stop_and_send
1640: grpc_transforms_synth_redirect_and_keeps_location   1689: grpc_does_not_transform_a_proxied...
1653: grpc_transforms_synth_501_chunked_rejection 1706: grpc_transform_is_visible_to_the_access_log
1664: grpc_transforms_synth_no_healthy_upstream_with_message  1724: grpc_transform_ticks_the_2xx...
```

**The implementation conformed to that list exactly and exceeded it by one** (the encode-arm
split, declared as D-4). The five undriven behaviours are the connect-failure 503, the reset 503,
the pool overflow, the request-budget overflow, and the host-miss 404 (`build_response_in:2145` —
`hcm_config_single_route` sets `domains: vec!["*"]`, so the 404 test always lands on the
route-miss twin at `:2164`). The connect-failure 503 is the single most common local reply a real
proxy emits and it has zero gRPC witness; the tree already carries every idiom that would make
each a one-liner (`127.0.0.1:1`, `cluster_mgr_with_endpoint_max_requests`, a
`max_connections:1 / max_pending_requests:0` bootstrap).

**The family is nevertheless covered BY CONSTRUCTION**, which is why this is a Minor and not an
Issue: all sixteen sites converge on ONE seam call gated on ONE bit, and §1 records that this
session verified the bit correct at every site by direct trace. What is missing is behavioural
confirmation, not coverage. The finding proper is that `PROGRESS.md`'s Task-6 headline —
"family-wide seam coverage" — restates the PLAN's table rather than its test list, and **the
over-claim is not declared among D-1…D-5.**

### M-4 — the proxied negative witness runs on the zero-copy direct-head path, where two of its four assertions are structurally blind, and its `201` cannot discriminate a status-keyed regression

`grpc_does_not_transform_a_proxied_upstream_response` (`hcm.rs:11526`) builds its config with
`hcm_config_with_cluster`, which sets `access_log: vec![]`, `test_router_only_pipeline()` and
`include_attempt_count_in_response: false` (`hcm.rs:3961-3970`). That satisfies
`direct_conn_eligible` (`:772-773`) and therefore `attempt_direct` (`:1145`), so the response
takes the direct-head path where `outgoing.headers` is EMPTY and the wire head comes from
`direct_head_buf`. Its two negative header assertions —

```rust
assert!(!s.contains("grpc-status"), ...);   assert!(!s.contains("grpc-message"), ...);
```

— therefore cannot fail on that path regardless of the transform. What IS load-bearing there is
`s.ends_with("UPSTREAM")` (the transform drops the body) and the `debug_assert!(!outgoing_direct)`
at `:1488`, which panics under `cfg(debug_assertions)` — so the test is a genuine witness, for a
narrower reason than it appears. Separately, the fixture's upstream status is `201`, which no
local reply in the family can produce, so a regression that re-derived locality from the STATUS
(`outgoing_local = outgoing.status >= 500`) would pass all fourteen tests — and four of the five
undriven M-3 behaviours are `503`s, byte-indistinguishable on the wire from a proxied 503 apart
from the transform. **The non-direct proxied path (access-log on, or a non-router chain) has no
negative witness at all.**

### M-5 — the pass-through RELATIVE-ORDER half of the header rule is unpinned: no fixture carries more than one pass-through header

Contract §1.4 / ADR-0179 DECISION 4 has two independent halves — pass-throughs go FIRST, *and*
they keep their MUTUAL relative order. Every one of the five `Response` fixtures in `grpc.rs`'s
transform tests carries at most one pass-through (`location` in one, `x-envoy-overloaded` in
another, zero in the other three), so the second half is never exercised, and neither is a
pass-through originally positioned AFTER `date`/`server`/`connection`. An implementation that
reversed, sorted, or hoisted-by-name the pass-through vector — the exact `location` special case
the doc at `grpc.rs:448-449` claims to have ruled out — passes all eighteen `grpc.rs` tests.

For balance: the `date`→`server`→`connection` re-ordering IS genuinely load-bearing, because
`bodied_local_reply_takes_the_measured_wire_shape` feeds `server` BEFORE `date` and asserts `date`
BEFORE `server` on output. Only the pass-through half is unpinned.

### M-6 — `assert_grpc_shape`, the shared helper for six of the fourteen seam tests, is entirely substring-based — and it was transcribed FAITHFULLY from a weak PLAN literal

`hcm.rs:11388-11407` asserts with `s.contains(...)` throughout:

```rust
assert!(s.contains(&format!("grpc-status: {grpc_status}\r\n")), "gs: {s}");
assert!(s.contains("content-length: 0\r\n"), "cl: {s}");
```

`contains("grpc-status: 12\r\n")` is satisfied by a header actually named `x-grpc-status`;
`contains("content-length: 0\r\n")` is satisfied even if a pre-transform `content-length: 19` also
survived. Only `grpc_local_reply_header_order_matches_upstream` (`:11358`) pins the exact header
NAME multiset, and it does so on the `synth_direct_response` arm alone — the one arm with no
pass-through header. The two arms where the name-set CAN go wrong (redirect's `location`,
overflow's `x-envoy-overloaded`) are never order-checked at the seam; the redirect test asserts
only `s.contains("location: https://h/x\r\n")`, never its position.

Worth stating precisely, because it changes where the fix belongs: the helper is **byte-identical
to the literal in `PLAN.md:1607-1618`**. The transcription was faithful; the source was weak.
A YAML-key-style anchor (a leading `\r\n`) would close it in one edit.

### M-7 — `PROGRESS.md`'s size table omits two of its own range's seven files; the true docs-excluded net is **1290**, not 1165, and "~28% above centre" is **+41.4%**

`PROGRESS.md`'s "Size — measured, against the PLAN's projection" quotes the command
`git diff --numstat eeb45d0 HEAD -- . ':(exclude)docs/'` and then lists five rows. Re-derived
here:

```
$ git diff --numstat eeb45d0 29d25e5 -- . ':(exclude)docs/' | awk '{a+=$1;d+=$2} END{print a,d,a-d}'
1305 15 1290
$ git diff --numstat eeb45d0 29d25e5 -- crates/envoy-http1/ | awk '{a+=$1;d+=$2} END{print a,d,a-d}'
1174 9 1165          <- exactly PROGRESS.md's figure: it is envoy-http1 ALONE
```

The omitted rows are `crates/envoy-http2/src/hcm.rs` (+125) and `Cargo.lock` (6/6, net 0).
`PROGRESS.md` acknowledges both in a parenthetical — "plus the Task-8 `envoy-http2/src/hcm.rs`
test and the `Cargo.lock` patch bump, which land in this commit" — and then draws every
percentage conclusion from the incomplete number anyway. This matters because **the PLAN's ≈912
bottom-up INCLUDES Task 8's ≈50 for `envoy-http2`** (`PLAN.md` File Structure), so comparing 1165
against 912 is apples-to-oranges by construction.

Re-derived against the true net **1290**: `1290/912 − 1 = **+41.4%**`, not ~28%; headroom to the
worst-case ≈1332 is **42** LoC, not 167, so "well under" overstates it. **Both stated
CONCLUSIONS nevertheless SURVIVE** — 1290 is inside the honest planning range 820–1330 (at 92% of
its width) and is under ≈1332. Right measurement, wrong scope: a banked figure's conclusion is a
claim too, and here the denominator and the numerator were drawn from different file sets.

### M-8 — deviation D-5 mischaracterizes the `Cargo.lock` delta as "one patch bump": four further package versions moved, two of them minor-version DOWNGRADES, disclosed in neither half

D-5 describes the lock change as "a pure lock patch bump" produced by `cargo update -p h2`. The
actual delta over the range:

```
$ git diff eeb45d0 29d25e5 -- Cargo.lock
-version = "0.4.13"        +version = "0.4.16"      # h2 — the declared bump
- "windows-sys 0.61.2"     + "windows-sys 0.52.0"   # x3 (getrandom, rustix, tempfile)
- "socket2 0.6.3"          + "socket2 0.5.10"       # hyper-util
$ grep -c 'windows-sys\|socket2' docs/envoy-rust/phases/110.1-grpc-local-reply-transform/PROGRESS.md
0
```

`windows-sys 0.61.2 → 0.52.0` and `socket2 0.6.3 → 0.5.10` are minor-version **regressions**, not
patch bumps, and they are not what `cargo update -p h2` should produce. Neither half of
`PROGRESS.md` mentions them, so a reader of D-5 would not know to look.

Why this is a Minor and not an Issue: **no new dependency was added** (D-3.2 governs direct
dependencies, and no `Cargo.toml` moved — `git diff --numstat eeb45d0 HEAD -- '*Cargo.toml'`
returns zero rows), and `cargo deny check` at state-4 ran against the CURRENT lock — the one
containing 0.52.0 and 0.5.10 — and returned the four-ok line, so `advisories ok` already covers
them. Both packages are Windows/socket transitive plumbing under `hyper-util` and `rustix` on a
project whose CI is Linux. The defect is in the RECORD, not in the tree.

### M-9 — D-5's "`Cargo.lock` is byte-untouched" proof used a MOVING `HEAD` endpoint and went false the instant its own carrying commit landed

D-5 states: *"My commits leave `Cargo.lock` byte-untouched (`git diff --numstat eeb45d0 HEAD --
Cargo.lock` is EMPTY)."* At HEAD that reads `6 6`. Provenance established rather than guessed:

```
$ git log --oneline eeb45d0..29d25e5 -- Cargo.lock
29d25e5 phase 110.1 state-3: implementation COMPLETE — ... + PROGRESS.md
$ git diff --numstat eeb45d0 29d25e5^ -- Cargo.lock
                                        <- EMPTY: TRUE when written
```

The sentence was true at the moment of writing and false one commit later, because `29d25e5` is
simultaneously the commit that carries the lock change and the commit that adds the file
asserting there isn't one. A numstat citation must be re-derived at the CARRYING commit, or cited
as a fixed range. The CONCLUSION D-5 draws — that RUSTSEC-2026-0258 is pre-existing — rests on
the independent scratch-worktree reproduction at `eeb45d0` and **SURVIVES**; only the
parenthetical proof offered alongside it does not.

---

## §4 — Nit

### N-1 — the `lib.rs` 110.1 comment is ORPHANED: it sits above `mod error;`, not above the module it explains

```
$ sed -n '17,21p' crates/envoy-http1/src/lib.rs
// 110.1: gRPC-aware local replies. DELIBERATELY `pub(crate)` — see the module
// doc. Nothing outside this crate may reach it, because `envoy-http2` shares
// this crate's `build_response` and must stay untransformed (CF-110-1).
mod error;
pub(crate) mod grpc;
```

The comment explains `pub(crate) mod grpc;` on line 21 but visually attaches to `mod error;` on
line 20. It is a plain `//` comment, so no gate reads it — `cargo fmt` will not move it and
clippy will not flag it. Insertion-before-the-wrong-item is the recurring shape here: a
declaration list was appended to alphabetically and the comment landed one line early.

### N-2 — the single H2 negative witness covers ONE of the four shared route-decision arms

`h2_route_decision_reply_is_not_grpc_transformed` (`envoy-http2/src/hcm.rs:7346`) builds a
`direct_response` route only. `build_response_in` has four `BuildOutcome::Synth` producers
reachable from H2 — `synth_400` (`:2125`), `synth_404` (`:2145`, `:2164`), `synth_direct_response`
(`:2171`) and `synth_redirect` (`:2195`). A WHOLESALE install at `synth_with` or
`build_response`/`build_response_in` IS caught, because `synth_400`/`synth_404`/
`synth_direct_response` all route through `synth_with`. **`synth_redirect` does not** (it
deliberately avoids `synth_with`, documented at `:2425`), so a per-wrapper install there would
leave the witness green while transforming H2 redirects. The witness's assertions are otherwise
strong — status, exact `content-type: text/plain`, absence of both gRPC headers, and body
survival — so status-only, header-only and body-drop-only partial installs are all caught; only
`content-length` is unasserted. Downgraded from the reviewing agent's `Important` because
`synth_redirect` carries no transform today and the compiler barrier of §1 makes the class
unreachable from `envoy-http2` itself.

### N-3 — `decorate_filter_synth_response` is a SECOND `pub` H1 function reachable from `envoy-http2` that Global Constraint 1 never names

```
$ grep -n 'fn decorate_filter_synth_response' crates/envoy-http1/src/hcm.rs
2522:pub fn decorate_filter_synth_response(resp: &mut Response, connection: Option<&str>) {
$ grep -n 'decorate_filter_synth_response' crates/envoy-http2/src/response.rs
68:    envoy_http1::hcm::decorate_filter_synth_response(resp, None);
```

Global Constraint 1 enumerates `synth_with`, any `synth_*` wrapper, and `build_response`/
`build_response_in`. It omits this one, which is `pub` (not `pub(crate)`), is called from
`envoy-http2` via the `decorate_filter_synth_response_h2` wrapper, and decorates H2's
filter-produced local replies. A transform installed there would transform H2 filter synths and
no test on either side would notice. **Correctly, none is installed there today** — the two-site
census in §1 proves it. This is a completeness gap in the CONSTRAINT's enumeration, banked so a
future H2 slice inherits the full list.

### N-4 — ADR-0179 and `PLAN.md` assert `crates/envoy-http2/src/hcm.rs:513-518`; the correct span is `:518-522`, and `DECISIONS.md` carries no correction pointer

Adjudicated at HEAD: `:512-515` is the explanatory comment and the call spans `:518-522`.
`PROGRESS.md` deviation D-2 is CORRECT and the three in-code citations
(`grpc.rs:15`, `grpc.rs:138`, `envoy-http2/src/hcm.rs:7328`) all carry the right span — the
transcription into shipping code was checked against the source and corrected, which is the
discipline working. What remains is that `PLAN.md` still asserts the wrong span at four sites
(`:19`, `:304`, `:1147`, `:1917`) and so does landed **ADR-0179**'s W-2/W-3 paragraph. Both are
append-only and correctly UNEDITED; the note here is that `DECISIONS.md` has no pointer to the
correction, so archaeology on the ADR alone reproduces the wrong number.

### N-5 — the `BEHAVIOR_CONTRACT.md` census pathspec is bare and provably VACUOUS

`PROGRESS.md`'s structural-census block asserts
`git diff --stat eeb45d0..HEAD -- tests/ BEHAVIOR_CONTRACT.md : EMPTY`, but the file lives at
`docs/envoy-rust/BEHAVIOR_CONTRACT.md`. Proven vacuous with a positive control — a commit that
DID change the file:

```
$ C=$(git log -1 --format=%H -- docs/envoy-rust/BEHAVIOR_CONTRACT.md)     # 8644fa4
$ git diff --stat $C^ $C -- docs/envoy-rust/BEHAVIOR_CONTRACT.md
 docs/envoy-rust/BEHAVIOR_CONTRACT.md | 13 +++++++++----
$ git diff --stat $C^ $C -- BEHAVIOR_CONTRACT.md
                                        <- EMPTY: the probe cannot ever match
```

**The conclusion SURVIVES** — re-run with the correct path,
`git diff --numstat eeb45d0 29d25e5 -- tests/ docs/envoy-rust/BEHAVIOR_CONTRACT.md` returns zero
rows. A probe that fails to match returns a believable zero; assert the probe RAN.

### N-6 — the `Cargo.toml` non-goal is asserted with a WORKING-TREE probe for a RANGE property

The same census block asserts `git status --porcelain -- Cargo.toml crates/*/Cargo.toml : EMPTY`.
`git status` reads the uncommitted tree, so it reads EMPTY even if a `Cargo.toml` change had been
COMMITTED in the range — which is exactly what the census exists to rule out. Conclusion survives
under the correct probe: `git diff --numstat eeb45d0 29d25e5 -- '*Cargo.toml'` returns zero rows.

### N-7 — response-side header-name case-insensitivity is unexercised, and duplicate `date`/`server`/`connection` inverts their order

The partition loop uses `eq_ignore_ascii_case` five times (`grpc.rs:179-189`), but no response
fixture in the tests carries an upper-case header name, so mutating any of those to `==` survives.
The REQUEST-side lookup is properly covered (`header_name_lookup_is_case_insensitive`). Separately,
the slot capture is guarded by `date.is_none() &&`, so a SECOND `date` falls through into
`passthrough` and is emitted BEFORE `content-type` while the first lands in the trailing slot — an
order inversion. The measured contract is silent on duplicates, so this is a coverage gap rather
than a proven divergence; `header_mutation`'s `Append` action can produce one (see M-2).

### N-8 — the trailing-space cell's codec premise is unpinned anywhere in-tree

`trailing_space_tolerance_is_deliberately_absent` (`grpc.rs:296`) pins only the NEGATIVE half —
that an untrimmed `application/grpc ` must not match — and explicitly delegates the positive half
(upstream DOES transform it) to the codec's OWS handling. Nothing in the tree pins that premise;
it holds today via `httparse`, not via envoy-rust code:

```
$ head -1 crates/envoy-http1/src/codec.rs
//! HTTP/1.1 request codec — a thin wrapper over `httparse::Request::parse`.
$ sed -n '1228p' ~/.cargo/registry/src/index.crates.io-.../httparse-1.10.1/src/lib.rs
        // trim trailing whitespace in the header
```

No live bug, but a codec change would silently stop transforming a reply upstream DOES transform
and no gate would notice. `application/grpc-web+proto`, called out as MEASURED-negative in the
module doc, is likewise absent from the seam-level detection table (`hcm.rs:11319-11327`).

### N-9 — three assertions claim less than their names suggest

`every_other_status_in_the_whole_u16_range_is_unknown` uses `assert_ne!(…, 2)` on its specials
branch, so swapping `400→13` with `401→16` passes it (the exact values are pinned separately, so
nothing is actually unpinned). `grpc_transforms_synth_501_chunked_rejection`,
`…_synth_redirect_and_keeps_location` and `…_filter_stop_and_send_on_encode` all assert
`grpc-status: 2`, which is the mapper's fall-through — they would pass with the table emptied.
And neither placement witness has a non-gRPC twin proving the log reads `404` and the counter
ticks `downstream_rq_4xx` WITHOUT the transform; the placement mutation supplies that evidence
once, at state-3, rather than standing in the suite.

### N-10 — two structural odds and ends

**(a)** `router::write_proxied_response` (`crates/envoy-http1/src/router.rs:197`) is a THIRD
`Http1Response` write site outside both funnels. It is production-dead — its only callers are
`router.rs`'s own `#[cfg(test)] mod tests` (`:256`, `:387`, `:406`) — and it writes a PROXIED
response, so it must not be transformed and correctly is not. Noted only because it is `pub`: a
future production caller would bypass the funnel silently. **(b)** The "ADR-0049
silent-divergence class" citation, which appears twice in `SPEC.md`, six times in `PLAN.md` and
once each in `grpc.rs` and `hcm.rs`, is THEMATIC rather than literal. ADR-0049 is the phase-18
CDS-envelope reconciliation; its transferable principle is D-3.3's "never both silently", which
does support the partial-family argument, but a reader who greps ADR-0049 expecting a
partial-coverage doctrine will not find one.

---

## §5 — Severity dissent, and subagent findings DOWNGRADED on re-verification

A subagent finding is a claim. Four were re-verified by this session and moved.

**DOWNGRADED — "three of the four shared route-decision arms are unwitnessed" (proposed
`Important`, landed as N-2).** The claim is factually correct, but the reviewer priced it as if
all four arms were equally exposed. Re-derived: `synth_400`, `synth_404` and
`synth_direct_response` all route through `synth_with`, so the ONE installation the constraint
actually forbids — a wholesale install at `synth_with` / `build_response` / `build_response_in` —
is caught by the existing witness. Only `synth_redirect`, which deliberately does not reuse
`synth_with`, escapes, and only under a per-wrapper install that nothing in the tree resembles.
With HTTP/2 compiler-isolated from the transform (§1), the residual class is a hypothetical
future edit inside `envoy-http1`. That is a Nit.

**DOWNGRADED — "`decorate_filter_synth_response` is a second shared H1→H2 function" (proposed
`Important`, landed as N-3).** Verified exactly as reported, and it is a genuine completeness gap
in Global Constraint 1's enumeration. But no transform is installed there, none is proposed, and
the omission has no effect on the landed tree. It is documentation debt to be inherited by a
future H2 slice, not a defect in this one.

**REFRAMED, not downgraded — "nine of sixteen local-reply sites undriven" (M-3).** The reviewer
presented this as an implementation shortfall against the PLAN's Task-6 family table. This
session checked the PLAN's Step-1 test list and found it specifies eight tests, none of which
covers the six sites the table assigns to that task. **The implementation matched its
instructions exactly and exceeded them by one.** The over-claim originates in the PLAN and was
inherited by `PROGRESS.md`. Same evidence, materially different attribution — and the
attribution is what a state-6 needs in order to bank it usefully.

**PARTIALLY REJECTED — "`x-envoy-overloaded` moves from last-but-one to first and nothing
observes it."** The MOVE is not a defect: measurement N-3 in `PLAN.md` records upstream's gRPC
overflow order as `x-envoy-overloaded, content-type, grpc-status, grpc-message, date, server,
connection, content-length`, which is exactly what the transform produces. The relocation is the
measured rule being obeyed, and it IS observed — `arbitrary_pass_through_headers_survive_in_
original_position` (`grpc.rs`) drives precisely `x-envoy-overloaded` and asserts the full name
vector. Only the end-to-end drive through the seam is missing, which is already M-3's subject.
The claim that nothing observes it is false.

**ACCEPTED AS FOUND** — M-1 (traced end-to-end through `local_rate_limit` by this session), M-4
(the direct-head eligibility re-derived from `hcm.rs:772-773`/`:1145`/`:3961-3970`), M-5, M-7
(arithmetic re-derived independently), M-8 and M-9 (the lock diff read in full, the temporal
provenance established at `29d25e5^`).

---

## §6 — Deliberate decisions verified

Each of these looks like a defect until it is traced, so each is recorded as CORRECT rather than
left for the next reviewer to re-litigate.

1. **`outgoing_local` defaults to `true`.** A newly added writer arm is therefore TRANSFORMED by
   omission rather than skipped by omission. The opposite default would be the more conservative
   choice for proxied traffic, but proxied responses arrive through exactly one arm which
   explicitly clears the bit, and the `debug_assert!(!outgoing_direct)` backstops the
   zero-copy path. For a family whose failure mode is silent under-coverage, failing toward
   coverage is right.

2. **The seam reads POST-filter request headers.** `req.headers` is taken and written back by the
   decode pipeline (`hcm.rs:733`/`:741`) before the seam reads `&req.headers` at `:1491`. That
   matches upstream, which evaluates the gRPC content-type against the current request header map
   at local-reply time. Correct — though unasserted anywhere (N-9's neighbourhood).

3. **`write_head_body` at `uring.rs:376` is untransformed.** That is the PROXIED path in the
   io_uring worker, and CF-110-2 requires exactly this.

4. **`synth_with`'s header order was NOT touched.** CF-110-4 records envoy-rust's non-gRPC order
   as divergent from upstream's; PLAN non-goal 10 forbids fixing it here. It was not fixed.
   Confirmed: `git diff eeb45d0 29d25e5 -- crates/envoy-http1/src/hcm.rs` contains no change
   below the `#[cfg(test)]` boundary other than the seam block.

5. **`location` was NOT added to `HEADER_ALLOW_LIST`.** Re-derived: 3 entries, `location` absent.

6. **No ADR was fired, and none was required.** Head stays `ADR-0179`; `ADR-0180` unreserved.
   ADR-0163 sets the precedent that a state-5 MAY fire one, but only where the review changes a
   recorded decision. This review changes none — it banks findings.

7. **`#![forbid(unsafe_code)]` is intact** at `crates/envoy-http1/src/lib.rs:1` (D-3.8).

---

## §7 — Status of already-banked findings — read BEFORE grading, NOT re-issued

None of the following was fixed, and none should have been (§6.3; ADR-0165). They are listed so
that a later reader does not mistake this review's silence for a claim that they are resolved.

- **CF-110-1 (NARROWED)** — HTTP/2 gRPC local replies are UNBUILT; shape measured (headers-only,
  `content-length` OMITTED rather than `0`). Task 8 pins today's UNtransformed behaviour, which is
  a characterization pin, NOT upstream parity. Confirmed still open. N-2 and N-3 refine what a
  future H2 slice must cover: eight distinct local-reply sites across three families, converging
  on `finalize_h2_stream`, with no `outgoing_local` analogue on the H2 side to reuse.
- **CF-110-2** — proxied gRPC responses untransformed and unmeasured. Guarded by `outgoing_local`
  and witnessed negatively, subject to M-4.
- **CF-110-3 (REASSIGNED)** — upstream emits `location` on a `201`/`3xx` `direct_response`;
  envoy-rust does not. Pre-existing, orthogonal. **Binding on `110.2`: fixture `0089` must not use
  a `201` or `3xx` `direct_response` cell.**
- **CF-110-4** — `synth_with`'s non-gRPC header order differs from upstream's. ORDER-only,
  pre-existing, invisible to `diff_headers`, and NOT a licence to touch `synth_with`. Untouched.
- **CF-110-5** — the io_uring local-reply seam is unwitnessed by any test. Confirmed: `uring.rs`
  still has zero `#[cfg(test)]`/`#[test]`/`#[tokio::test]`. Its only gate is
  `clippy --all-features`, now proven causally in both directions by V-3.
- CF-109-1 (WIDENED)/2/3, CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6,
  CF-74-1/2/3/4/6, CF-73-1, the `109.2` REVIEW's M-1…M-8 + N-1…N-11, the `109.1` M-5 + N-1…N-6
  set, the `108.2` M-2 + N-1…N-6 set, and the HTTP-filters-family (1)-(4) — **all carried
  unchanged.**

---

## §8 — Disposition of the three state-4 banked findings

Each was re-derived from disk rather than accepted.

**V-1 (METHOD) — CONFIRMED, and its lesson is the durable output.** The stable local red core is
FIVE, not seven; a back-to-back isolation loop with no settle gap manufactured a false
`FAILS-IN-ISOLATION` verdict on `access_log_rf_no_healthy` and
`access_log_rf_overflow_request_budget` — both of which sit on local-reply paths this sub-phase
transforms, i.e. exactly the shape of a real regression, which is why running it to ground rather
than waving it off was the right call. Root cause established by `docker inspect`, not inference:
a host port collision stopped the UPSTREAM REFERENCE container from starting, a path envoy-rust's
code is not on. This review did not re-run the sweep (§5.1 — that is the state-4's product), but
it did cross-check the arithmetic that settles it: local `passed + failed = 2227` equals CI's
`passed = 2227` with `failed = 0` on run 32257501630, which independently confirms that every
locally-red name passes in CI. **Note for the record that V-1's two "extras" are among the very
sites M-3 finds have no gRPC test** — the flake tail brushed against a genuine coverage gap
without either being caused by the other.

**V-2 (CITATION) — CONFIRMED, and EXTENDED.** The `#[cfg(test)]` boundary in
`crates/envoy-http1/src/hcm.rs` is **2579**, not the 2532 that `PROGRESS.md` D-1 and two handoffs
cite; provenance is `git show 507c67b:` = 2532, true at Task 4 and stale since Task 5. Re-derived
at HEAD: the three `grpc` hits below 2579 are two comments (`:1440`, `:1441`) and the seam call
(`:1491`), and `GRPC_STATUS` is defined in `headers.rs:18` and consumed only in `grpc.rs`. **The
soundness claim survives at the correct boundary — but this review finds the claim was scoped to
the wrong FILE SET all along (M-2).** A survey of `crates/envoy-http1/` cannot see an operator
string injected by `header_mutation`'s `encode_headers`, which runs 64 lines before the seam.
V-2 fixed the line number; the scope error underneath it was never the line number's fault.

**V-3 (EVIDENCE ADDED) — CONFIRMED, CF-110-5 stays OPEN.** The causal experiment is sound in both
directions and is the right shape: a `count == 1` guard, an unmutated control, a red WITH
`--all-features` citing `uring.rs:526`, and a genuine 20-`Checking` run WITHOUT the flag that
never mentions the file. The negative arm is what makes it evidence rather than a coincidence —
a one-direction green would have proven nothing. **CF-110-5 was correctly NOT fixed**: a state-4
grades, it does not change code.

---

## §9 — Carry-forwards for the state-6 close-out to bank

Nothing here is an obligation on the close-out itself, which is a ROADMAP status flip and a
`STATE.md` relocation and nothing else. These are for sibling `110.2` and for whichever slice
next touches this surface.

**Binding on `110.2` specifically:**
1. **CF-110-3 stands: fixture `0089` must NOT use a `201` or `3xx` `direct_response` cell**, or it
   reds for a `location` reason unrelated to gRPC. A `redirect:` route probe is fine.
2. `110.2` writes the `BEHAVIOR_CONTRACT.md` `## gRPC` section. **M-2 belongs in it**: state
   whether the transform is suppressed by a pre-existing `grpc-status` response header, and
   either measure that cell upstream or say plainly that it is unmeasured.
3. `110.2` is the natural home for a `%RESPONSE_CODE%`/`%BYTES_SENT%` differential witness of the
   placement finding, which today rests entirely on two in-process tests and one state-3 mutation.

**Cheap test-adequacy debt, in the order a later slice should take it:**
4. **M-1** — one line: give a transform fixture a `Some(..)` reason. It closes the only vacuous
   assertion on a live cell in the whole sub-phase.
5. **M-6** — anchor `assert_grpc_shape`'s `contains` calls with a leading `\r\n`. One edit,
   closes the `x-grpc-status` and surviving-`content-length` classes for six tests at once.
6. **M-3** — the connect-failure 503 first (`127.0.0.1:1` is already an idiom in this file), then
   the two overflow sites and the host-miss 404.
7. **M-4** — a proxied negative on the NON-direct path (access-log on), and a local/proxied pair
   that share a status so the discrimination is actually witnessed.
8. **M-5** — one fixture with two pass-through headers.

**Record accuracy, for whoever next quotes these numbers:**
9. **The sub-phase's true docs-excluded net is 1290, not 1165** (M-7). Anyone calibrating a future
   §6.1 estimate from `110.1` must use 1290, or the calibration inherits a 125-line under-count on
   the low side and an apples-to-oranges comparison against a projection that included Task 8.
10. **The `Cargo.lock` delta is four packages plus `h2`, with two minor-version downgrades**
    (M-8), and D-5's byte-untouched proof is a moving-endpoint citation (M-9).
11. **N-4**: `ADR-0179` and `PLAN.md` assert `envoy-http2/src/hcm.rs:513-518`; the span is
    `:518-522`. Both are append-only and correctly unedited — but `DECISIONS.md` has no pointer to
    the correction, so ADR-only archaeology reproduces the wrong number.

---

## §10 — Assessment

`110.1` is a well-built slice whose hardest problem was never the code. The three pure functions
are small, total, and pinned to measurement rather than to intuition — the `~` → `%7E` boundary
and the sparse eight-entry status table are both places where the obvious rule is wrong and the
tests know it. The genuinely difficult decision was WHERE the transform goes, and the phase got it
right for a reason no wire-shape test could have supplied: it is placed before the access-log and
stats derivation because upstream logs and counts the TRANSFORMED status, and the only evidence
that this is enforced rather than merely intended is two observability witnesses plus a placement
mutation that reds exactly those two and leaves fifteen others green. **A seam's correct placement
is not the wire write, and only a placement mutation proves it** — this phase is the cleanest
demonstration of that principle in the repository.

The second thing it got right is structural rather than behavioural. HTTP/2 shares this crate's
`build_response`, so the whole design hinges on the transform being unreachable from H2. Rather
than rely on doc comments, the phase made it a compile error — `pub(crate) mod grpc;` with no
re-export, and a `pub(crate)` function inside it. Two barriers, either sufficient. Likewise the
io_uring seam lives inside `write_owned` rather than at its four call sites, in a file with no
test harness at all, so a fifth local-reply site added later cannot forget it. Both are the right
answer to "how do I make this impossible to get wrong later" rather than "how do I make this work
now."

The defect profile is narrow and consistent: **zero findings in the production code, and every
finding in a witness or a citation.** Three shapes recur. First, an assertion that is weaker than
its name — substring matching where a byte anchor was available (M-6), a fixture that cannot
exercise the field it asserts (M-1), a negative witness sitting on the one path where its
assertions cannot fire (M-4). Second, a claim scoped to the wrong set — V-2 corrected a stale line
number for a soundness argument whose real problem was that it surveyed `hcm.rs` when the
injection point is in `envoy-filter` (M-2), and a size figure drawn from `envoy-http1` alone and
compared against a projection that included `envoy-http2` (M-7). Third, a citation that was true
when written and false one commit later (M-9), or that names a file the pathspec can never match
(N-5).

Two findings are worth singling out. **M-2** is the most consequential: the idempotence sentinel's
justification has been re-derived twice — once by the implementer, once by the state-4 verifier —
and both times within `crates/envoy-http1/`, which is precisely the file set that cannot contain
the counterexample. The encode-side filter pipeline runs 64 lines before the seam and will apply
any operator-configured header mutation with no reserved-name check. Nothing is broken today, but
the argument does not establish what it claims to, and a correct-looking proof that surveys the
wrong scope is the kind of thing that stays wrong for several phases. **M-3** is the most
instructive: it looks like an implementation shortfall and is not — the PLAN's own family table
promises coverage its own test list never specifies, the implementer built exactly what was
specified, and `PROGRESS.md` then inherited the table's language. Attribution matters here,
because fixing the implementation would be the wrong lesson; the lesson is that a PLAN's coverage
table and its test list are two claims and they need to be diffed against each other.

Nothing found changes a byte of wire behaviour, and nothing found is worth three more sessions
under §5.2. The eight local-reply behaviours that ARE driven, the compiler-enforced H2 isolation,
the verified-correct locality bit at all sixteen sites, and the structurally-zero blast radius
together make this a sound foundation for the differential witness `110.2` will add.

**Gate (f) is CLOSED. All six §7.5 gates are now GREEN. Sub-phase `110.1` is approved to land.**

### STOP CONDITION — re-derived from disk at this review, ALL THREE LEGS

The mission is complete only when EVERY ROADMAP row is `done` AND no in-scope leaf remains. **ALL
THREE LEGS MUST HOLD. IT IS NOT COMPLETE.**

- **Leg (i) — FALSE.** 116 rows; `110` `in-progress`, `110.1` and `110.2` `planned`.
- **Leg (ii) — FALSE**, by direct tree probes rather than by the ledger's own assertion: **14**
  crates with no `envoy-http3`/`envoy-grpc`/`envoy-wasm`/`envoy-protos`/`envoy-runtime`;
  `quinn`/`tonic-web`/`wasmtime` each **0** in crate manifests; `tests/conformance/` holds only
  `h2spec/`; `runtime_key_is_rtds_inert` still present.
- **Leg (iii) — FALSE.** Heading-slice census over all eleven `### ` family headings:
  `10/5/3/14/0/3/6/29/6/0/13 = 89 under headings + 27 before the first heading = 116`, with
  **TWO** zero-row headings (`### HTTP/3 + QUIC family`, `### WASM host family`).

**NO `stop` FILE WAS CREATED**; `ls stop` returns `No such file or directory`.

### Next state

**§5 state 6 — the close-out**, a SEPARATE session per §5.1 and ADR-0127 (a reviewer must not
close out what it graded). At that close-out **ROADMAP row `110.1` flips `planned` → `done` and
NOTHING ELSE** — assert its own starting status first. **Parent row `110` STAYS `in-progress`**,
because sibling `110.2` is still `planned`; this is NOT the `109.2`/`76.2`/`108.2` two-row
close-out precedent, and applying that precedent here would be wrong. The
`### Sub-phase 110.1 §5 state-5 code review` Notes subsection is retired to `STATE_HISTORY.md`,
and **no ADR and no new Notes subsection is added** — both are measured precedents. Sibling
`110.2` then opens at its own §5 state 2 (PLAN-write) in a session after that, and `110.2`'s
state 3 is the next session that writes code. This review **fixed nothing**, as ADR-0165 requires.
