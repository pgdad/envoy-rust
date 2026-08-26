# Phase 111 — PROGRESS (§5 state-3 implementation)

> **What this file is.** The running execution log for the §5 state-3
> implementation of phase `111` (HTTP/2 response TRAILER forwarding, upstream →
> downstream). One section per `PLAN.md` task, appended as that task lands. It
> records what was actually run and what it actually printed — not what the plan
> predicted.
>
> **Session start:** HEAD `0ba60db1b69981a473188661929c4e2093ea356e`, branch
> `main`, `git status --porcelain` empty, `origin/main` in sync
> (`git fetch origin --prune` run BARE, exit 0 — a piped `$?` reads `tail`'s).
> `ls stop` → `No such file or directory`.
>
> **This session does NOT** run the §7.5 gate adjudication (state 4), write
> `REVIEW.md` (state 5), flip any `ROADMAP.md` row (state 6), fix any banked
> finding (§6.3; ADR-0165), or edit any landed artifact.

---

## Pre-flight — the plan's own claims, RE-VERIFIED on disk

`PLAN.md` resolved every `file:line` at `82e2e75`; HEAD is now `0ba60db`. Every
load-bearing citation was re-resolved by TEXT at `0ba60db` before Task 1 began,
and **every one held**:

| claim (`PLAN.md`) | measured at `0ba60db` | verdict |
|---|---|---|
| ROADMAP census 117 / 116 `done` / 0 `in-progress` / 1 `planned` | 117 / 116 / 0 / 1 | ✅ |
| ADR head `ADR-0182` | `ADR-0182` (MAX recipe) | ✅ |
| fixture dirs 89, differential test files 89, highest `0089` | 89 / 89 / `0089-grpc-aware-local-replies` | ✅ |
| PV-5: `send_envoy_response` has exactly ONE production caller | `hcm.rs:1043` (the only non-doc, non-test hit) | ✅ |
| Task 1 Step 1: no `unreachable!`/`unimplemented!` in `envoy-http2`/`envoy-bin`; no exhaustive `match` on `Http2Error` | both greps returned ZERO hits | ✅ |
| Task 3: the five declarations at `:242`/`:245`, `:141`, `:716`, `:568`, `:961` | all five present at exactly those lines | ✅ |
| Task 3 Step 4: five `H2AttemptResult {` literal sites at `:191`,`:372`,`:390`,`:403`,`:414` | exactly those five | ✅ |
| Task 6: `DriveHttp1Result` has exactly TWO struct-literal sites | `:2319` (`drive_http1`), `:2410` (`drive_http2`) | ✅ |
| Task 6 trap: `conn_handle.abort()` sits after the header loop in `drive_http2` | confirmed — `drop(send_request); conn_handle.abort();` follows the header conversion | ✅ |
| Task 8: the token needs FOUR edits (2 kv-push + 2 `.is_some()` guard chains) | `:3613`/`:3714` kv-push, two guard chains at `:3620`-ish and `:3721`-ish | ✅ |
| `HEADER_ALLOW_LIST` is 3 entries | 3 (`server`, `date`, `x-envoy-upstream-service-time`) at `lib.rs:1189` | ✅ |
| Task 5: `Http2CloseBackend` + `spawn_helper_backend` + `wait_h2_accept_ready` shapes | all present in `tests/differential/src/backend.rs` | ✅ |

**Three adaptations the plan itself flagged, each CONFIRMED necessary** (the
plan's "read the in-tree helper, the tree is authoritative" instruction earned
its keep three times):

1. **Task 2's `Request` literal.** `envoy_http1::codec::Request` carries
   `bytes_consumed: usize` and `body: Option<Bytes>` — the plan's sketch has
   neither. The in-tree `mk_request` helper (`client.rs`, in `mod tests`) is
   authoritative and is what the new tests use.
2. **Task 7's `load_expectations`.** It takes `&Path`, not `&str`
   (`lib.rs:1261`), so the plan's YAML-string round-trip tests cannot call it.
   They parse via `serde_yaml::from_str::<Expectations>` instead — same
   assertion, same `deny_unknown_fields` surface.
3. **Task 4's `StopAndSend` fixture.** `HttpFilterInstance::test_stop_and_send_on_encode`
   and `FilterPipeline::test_from_instances` DO exist (used by
   `h2_stop_and_send_at_encode_substitutes_wire_response`), so no new filter is
   written — but the only in-tree config helper that accepts a pipeline
   (`synth_h2_hcm_config_with_pipeline`) routes to a `DirectResponse`, not to an
   upstream. Task 4 therefore needs a proxying variant of it.

**Stop condition, re-evaluated from disk at `0ba60db` — ALL THREE LEGS FALSE.**
(i) row `111` is `planned`. (ii) 14 crates, ZERO of
`envoy-http3`/`envoy-grpc`/`envoy-wasm`/`envoy-protos`/`envoy-runtime`;
`quinn`/`tonic-web`/`wasmtime` = 0 across `crates/*/Cargo.toml`;
`tests/conformance/` holds only `h2spec/`. (iii) TWO `### ` family headings carry
ZERO rows (`### HTTP/3 + QUIC family`, `### WASM host family`).
**The mission is NOT complete and NO `stop` file was created.**

---

## Task 1 — Downstream emit seam: `send_envoy_response` emits a trailer block ✅

**Files:** `crates/envoy-http2/src/error.rs`, `crates/envoy-http2/src/response.rs`,
`crates/envoy-http2/src/hcm.rs` (the single production call site, passing `None`
for now).

**Step 1 — the error-variant safety check, RE-RUN rather than trusted.**

```
$ git grep -n 'unreachable!\|unimplemented!' -- 'crates/envoy-http2/**/*.rs' 'crates/envoy-bin/**/*.rs'
(no output)
$ git grep -n 'match .*Http2Error' -- '*.rs'
(no output)
```

Both expectations met, so `H2SendTrailers` cannot land in a caller's
`unreachable!()` (the standing "widening a returnable error set" trap).

**Step 3 — RED.** Six new tests plus a `round_trip` helper that drives
`send_envoy_response` over a REAL in-process H2 connection, because
`build_http_response` sees headers and never FRAMES, and the frame sequence is
the entire subject of this task.

```
$ cargo test -p envoy-http2 --lib response::tests
error[E0061]: this function takes 2 arguments but 3 arguments were supplied
error: could not compile `envoy-http2` (lib test) due to 1 previous error
EXIT=101
```

A compile error is NOT a valid *mutation-check* RED, but it IS the correct TDD
RED for a signature-changing task — and the message names the arity, not
something unrelated, which is the check the plan asks for.

**Step 5 — the three-way fork, implemented as measured (D-PLAN-3).**
`send_response(head, body_empty && trailer_map.is_none())`, then `send_data(body,
trailer_map.is_none())` only when the body is non-empty, then `send_trailers`.
The empty-body-with-trailers row sends **no DATA frame at all** — legal, and the
gRPC main case.

**D-PLAN-4 recorded at the site.** `build_trailer_map` carries a `# No hop-by-hop
strip here, deliberately` doc section explaining that `h2` rejects the same six
names on the RECEIVE side, so such a guard would be unreachable, untestable code
(§6.3), with the CF-111-5 asymmetry named. This is there so a reviewer does not
re-raise it.

**Step 7 — GREEN.**

```
$ cargo test -p envoy-http2 --lib response::tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 107 filtered out
```

13 passed — 7 pre-existing + 6 new, a NON-ZERO count (`0 passed; N filtered out`
is a false green).

**Step 8 — the whole crate.**

```
$ cargo test -p envoy-http2
test result: ok. 119 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Also run (not required by the task, run to keep the state-4 gate honest):**
`cargo fmt --all -- --check` initially FAILED on two hunks in the new test block
(rustfmt reflowed a call and dropped a trailing blank line); `cargo fmt --all`
applied it and the re-check is clean. `cargo clippy -p envoy-http2 --all-targets
--all-features -- -D warnings` finished clean.

**The six new tests, and the cell each pins:**

| test | cell |
|---|---|
| `trailers_follow_a_non_empty_body` | the subject: body then trailers |
| `trailers_follow_an_empty_body_with_no_data_frame` | the gRPC main case — no DATA frame |
| `no_trailers_non_empty_body_is_unchanged` | PV-6 regression pin |
| `no_trailers_empty_body_is_unchanged` | PV-6 regression pin, `end_of_stream=true` HEADERS |
| `trailer_names_envoy_forwards_are_not_stripped` | PV-3 rows 10-12 — `content-length`/`te`/`host` NOT stripped |
| `duplicate_trailer_names_are_both_emitted` | PV-3 row 5 — `append`, not `insert` |

---

## Task 2 — Upstream read: `send_request` returns the trailer block ✅

**Files:** `crates/envoy-http2/src/client.rs`, `crates/envoy-http2/src/hcm.rs`
(two production call sites, made to discard the trailers with a marker Task 3
consumes).

**Step 1 — the `Request` literal came from the TREE, not from the plan.**
`PLAN.md`'s sketch builds `Request { method, path, version, headers, body:
Bytes::new() }`. The real `envoy_http1::codec::Request` has **six** fields —
it also carries `bytes_consumed: usize`, and its `body` is `Option<Bytes>`, not
`Bytes`. The in-tree helper `mk_request(method, path, headers, body)` in the same
`mod tests` is authoritative and is what both new tests call. This is the first of
the three "read the in-tree helper" pointers the plan deliberately left unresolved,
and it would have been a compile error had the sketch been transcribed.

**Step 2 — RED.**

```
$ cargo test -p envoy-http2 --lib client::tests
error[E0308]: mismatched types
   --> crates/envoy-http2/src/client.rs:624:13
624 |         let (resp, trailers) = client.send_request(req).await.unwrap();
    |             ^^^^^^^^^^^^^^^^   expected `Response`, found `(_, _)`
error: could not compile `envoy-http2` (lib test) due to 2 previous errors
EXIT=101
```

The message names the tuple mismatch — the check the plan asks for — not
something unrelated.

**Step 3 — the read, placed where PV-5 says it must go.** `recv_stream.trailers()`
sits immediately after the body-drain loop's closing brace and **BEFORE** the
`(g)` status-range guard: `h2` resolves `trailers()` only once `data()` has
returned `None` (the drain loop guarantees that), and reading before `(g)` keeps
`recv_stream` alive and reads a block even on a status that guard would reject.
Non-ASCII trailer values are SKIPPED rather than failing the response — the same
defensive posture the header conversion directly below already takes.

**Step 4 — the call sites.** Two production sites in `hcm.rs` (the pooled H2
`client_stream_mut().send_request(out_req)` and the no-pool per-call
`s.send_request(out_req)`) each gained `.map(|(r, _trailers)| r)` with a marker
comment naming Task 3. Five `#[cfg(test)]` sites inside `client.rs` were
destructured. Note the H1 fork's `send_request` at the same `match` is
`envoy_http1::Client`'s and is untouched.

**Step 5 — GREEN.**

```
$ cargo test -p envoy-http2
test result: ok. 121 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

121 = 119 (post-Task-1) + the 2 new tests. `cargo build --workspace
--all-targets` finished clean — the return-type change is genuinely crate-local,
verified rather than asserted. `cargo fmt --all -- --check` clean;
`cargo clippy -p envoy-http2 --all-targets --all-features -- -D warnings` clean.

---

## Task 3 — Thread the trailers from the upstream attempt to the emit seam ✅

**Files:** `crates/envoy-http2/src/hcm.rs`, plus a new crate-level type alias in
`crates/envoy-http2/src/lib.rs` (see "the plan's code tripped clippy" below).

**§6.1's MID-EXECUTION split trigger was evaluated here and does NOT fire.**
This was the plan's named candidate ("the largest single task… if any step blows
past ~10 sub-steps on contact with reality, stop and split"). Measured on
contact: the threading resolved into **five coherent edit groups** — the
`AcquireOutcome::Sent` payload (+3 fork call sites), the `H2AttemptResult` field
(+5 literal sites), the retry-loop tuple (+its single `break`), the
`request_path` match (+4 arm tails), and `finalize_h2_stream` (param + 1 call
site + the emit call). No group required unplanned discovery, and the whole task
landed in one pass. **Phase 111 stays whole; no `111.1`/`111.2` was created.**

**Step 1/2 — RED, and it is a genuine ASSERTION, not a compile error** (the plan
insists on the distinction, and a compile error would have meant the helper
sketch was wrong):

```
$ cargo test -p envoy-http2 --lib h2_forwards_upstream_response_trailers
thread 'hcm::tests::h2_forwards_upstream_response_trailers_downstream' panicked at
  crates/envoy-http2/src/hcm.rs:1842:9:
assertion `left == right` failed: both the announced AND the unannounced trailer must be forwarded
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 123 filtered out
```

Note WHICH assertion fired: the `trailer:` announce-header assertion, which sits
ABOVE it, PASSED — so F3's "the announce header is already at parity" is
re-confirmed by this RED rather than merely quoted.

**Two helpers were written, and the plan's "read the in-tree helper" pointer
paid off again.** `spawn_upstream_h2_server_with_trailers` mirrors the existing
`spawn_upstream_h2_server` shape; `drive_one_h2_request_through_hcm` is new,
because the module's existing tests all drive the HCM INLINE and none has an
observation type. It awaits `body_stream.trailers()` BEFORE dropping the
connection task — reading after would silently report zero.

**Steps 3–7 — the five declarations, all located by TEXT.** The `H1` fork of
`AcquireOutcome::Sent` (which shares the enum and can carry no trailers) is
wrapped `(r, None)` with CF-111-2 named at the site. Exactly ONE
`H2AttemptResult` literal — the `Sent(Ok(..))` proxied arm — carries real
trailers; the other four pass `None`, as do all three non-proxy `request_path`
arm tails. That is D-PLAN-5, and it is structural rather than remembered.

**⚠ THE PLAN'S OWN CODE TRIPPED CLIPPY, EXACTLY AS THE STANDING TRAP PREDICTS.**
The plan specifies `Sent(Result<(envoy_http1::Response, Option<Vec<(String,
String)>>), String>)`. That form fails the plan's own gate (e):

```
error: very complex type used. Consider factoring parts into `type` definitions
   --> crates/envoy-http2/src/hcm.rs:251:14
    | Sent(Result<(envoy_http1::Response, Option<Vec<(String, String)>>), String>),
    = note: `-D clippy::type-complexity` implied by `-D warnings`
```

**Fix: a crate-level `pub type TrailerBlock = Option<Vec<(String, String)>>;`**
in `lib.rs`, carrying the D-PLAN-2 rationale as its doc comment, used at every
production hop (`client.rs`'s return type, `response.rs`'s parameter, the
`AcquireOutcome` payload, the `H2AttemptResult` field, the retry-loop tuple, the
`finalize_h2_stream` parameter). **A type alias is transparent, so the plan's
§8 "the trailer type is `Option<Vec<(String, String)>>` at every production hop"
remains literally true** — this is a spelling change, not a design change.

**A second plan-code defect, same class:** the plan's Task 3 Step 7 declares the
`finalize_h2_stream` parameter `mut trailers` (because Task 4 will assign to it).
At Task 3 nothing assigns to it yet, so `-D warnings` fails on `unused_mut`. The
`mut` is therefore added in **Task 4**, at the commit that introduces the
assignment, keeping every commit's tree warning-free.

**Steps 8/9 — GREEN.**

```
$ cargo test -p envoy-http2
test result: ok. 123 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
$ cargo build --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s)
```

123 = 121 (post-Task-2) + 2 new. The workspace build is what PROVES the enum
widening and the struct field stayed crate-local — a `-p envoy-http2` build alone
would not. `cargo fmt --all -- --check` clean;
`cargo clippy -p envoy-http2 --all-targets --all-features -- -D warnings` clean.

---

## Task 4 — Locally-generated responses carry NO trailers ✅

**Files:** `crates/envoy-http2/src/hcm.rs` (the encode-filter `StopAndSend` arm,
plus a new proxying test-config helper).

**Step 1 — the hazard, confirmed by reading the arm rather than by argument.**
The arm rebuilds `resp` as a fresh `Response { status, reason, headers, body }`
literal from the filter's replacement. Under D-PLAN-2 the trailers live in a
SEPARATE local, so that rebuild does **not** touch them.

**Step 2 — the in-tree `StopAndSend` fixture EXISTS** (the plan left this
unresolved on purpose). `envoy_filter::HttpFilterInstance::test_stop_and_send_on_encode`
and `FilterPipeline::test_from_instances` are already used by
`h2_stop_and_send_at_encode_substitutes_wire_response`, so **no new filter was
written and `crates/envoy-filter/` was not touched** (SPEC non-goal 3). What was
missing is a config helper: the only in-tree helper taking a pipeline
(`synth_h2_hcm_config_with_pipeline`) routes to a `DirectResponse`, and this
hazard only exists when a REAL upstream response's trailers meet the filter. A
proxying sibling, `synth_h2_hcm_config_proxy_with_pipeline`, was added.

**Step 3 — RED, and the hazard is REAL, not theoretical:**

```
$ cargo test -p envoy-http2 --lib stop_and_send_drops_upstream_trailers
thread 'hcm::tests::h2_encode_filter_stop_and_send_drops_upstream_trailers' panicked at
  crates/envoy-http2/src/hcm.rs:5496:9:
a filter-replaced response must carry no upstream trailers, got [("x-trail-a", "alpha")]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 124 filtered out
```

The upstream's `x-trail-a: alpha` genuinely leaked onto a `418 teapot` the
upstream never sent. **Invisible to all 89 existing differential fixtures**,
because not one of them has a trailer at all.

**Step 4 — the clear**, with a comment stating explicitly why it is NOT redundant
with the rebuild directly above it. The `finalize_h2_stream` parameter regains
its `mut` at this commit (deferred from Task 3 to keep that commit
warning-free).

**Step 5 — GREEN.**

```
$ cargo test -p envoy-http2
test result: ok. 124 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

`cargo fmt --all -- --check` clean; `cargo clippy -p envoy-http2 --all-targets
--all-features -- -D warnings` clean.

**Step 6 — MUTATION CHECK, in a scratch worktree, with an unmutated control.**

The clear is a one-line guard, so it gets proved non-vacuous. The worktree was
created `--detach` at `HEAD` and then **SEEDED** with the working-tree `hcm.rs`
— the clear was not yet committed, and an unseeded worktree would have tested
the PRE-clear tree, whose "RED" would have proved nothing.

```
$ grep -c 'trailers = None;'  crates/envoy-http2/src/hcm.rs     # target, EXACTLY once
1
$ grep -c 'trailers: None,'   crates/envoy-http2/src/hcm.rs     # the struct-FIELD form, must NOT be hit
4
```

That second count is the point of the check the plan demands: a `sed` on the
wrong spelling would have hit four `H2AttemptResult` literals as well and faked a
result. The `sed` is anchored to `^            trailers = None;$`.

**CONTROL (unmutated, same worktree):**

```
   Compiling envoy-http2 v0.0.0 (…/scratchpad/mut-111-t4/crates/envoy-http2)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 124 filtered out
```

**MUTATED** (`trailers = None;` → `// MUTATED: the clear removed`; target count
0, marker count 1; `touch crates/envoy-http2/src/lib.rs` to force a real
rebuild):

```
   Compiling envoy-http2 v0.0.0 (…/scratchpad/mut-111-t4/crates/envoy-http2)
thread 'hcm::tests::h2_encode_filter_stop_and_send_drops_upstream_trailers' panicked at
  crates/envoy-http2/src/hcm.rs:5506:9:
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 124 filtered out
```

All four gates the traps ledger requires are met: a `Compiling envoy-http2` line
(so no stale binary FALSE-PASSED), a `test result:` line (so it RAN — a compile
error is not a mutation RED), a FAILED verdict, and a GREEN control from the same
worktree. The scratch worktree was then removed and only that one; the four
sibling `.claude/worktrees/agent-*` worktrees belong to a parallel workstream and
were left alone. Main tree re-checked afterwards: target still exactly 1.

---

## Net-LoC checkpoint after the production chain (Tasks 1–4)

The handoff asks for this to be tracked as the phase runs and said so honestly
rather than reported at the end.

```
$ git diff --numstat 0ba60db HEAD -- . ':(exclude)docs/' | awk '{a+=$1;d+=$2} END{print a, d, a-d}'
775 33 742
```

**742 net against the plan's ≈406 for those four tasks — a 1.83× ratio on this
slice, above the worst-observed landed-phase overrun of 1.75.** The overshoot is
concentrated in test scaffolding, not production code: the `round_trip` helper
(Task 1), and `spawn_upstream_h2_server_with_trailers` +
`drive_one_h2_request_through_hcm` + `ObservedH2Response` (Task 3) together
account for most of it, because — as the plan anticipated but did not price —
NO in-tree helper could observe a trailer frame, so all three had to be written
rather than reused.

**This does NOT fire §6.1.** The gate reads on the PLAN's estimate at state 2
(≈916 < ~1500), which is where it was applied and did not fire; §6.1's
*mid-execution* trigger is about a single task's sub-steps exceeding ~10 items,
which was evaluated at Task 3 and did not fire either. Recorded here so the
state-4 and state-5 sessions see the drift rather than discover it: the
remaining six tasks are planned at ≈510, and at this slice's ratio the phase
would land near ≈1670 — over the gate that the estimate cleared. That is the
exact risk `PLAN.md` §6 stated rather than rounded away.

---

## Task 5 — A trailer-emitting H2 backend the harness can spawn ✅

**Files:** `tests/helpers/http2-echo-server/src/main.rs`,
`tests/differential/src/backend.rs`.

**Steps 1/2 — RED**, and it names exactly what the plan predicted:

```
$ cargo test -p http2-echo-server
error[E0560]: struct `Args` has no field named `trailers`     (×2 — the two existing Args literals)
error[E0609]: no field `trailers` on type `Args`              (×2)
error[E0425]: cannot find function `handle_connection_with_trailers` in this scope
EXIT=101
```

**Step 3 — the flag and the mode.** `Args` gains `trailers: bool`; `parse_argv`'s
closure gains an `else if` branch; `print_help`'s usage line gains
`[--trailers]`; the accept loop's dispatch becomes three-way. The response path
differs from `handle_connection` in exactly two places — the `trailer: x-trail-a`
announce header, and a tail of `send_data(.., false)` + `send_trailers`. **The
`make_response_body` echo shape is untouched**, which is load-bearing: fixture
`0090` inherits fixture `0010`'s byte-exact body comparison (D-PLAN-7).

Three tests, not two: the plan's two argv tests, plus a wire-level test that
drives the mode over a real in-process H2 client and asserts the announce header,
the unchanged echo body shape, AND both trailers. An argv test alone would prove
the flag parses, not that anything is emitted.

**Steps 4/5 — `Http2TrailersBackend`**, a near-copy of the sibling
`Http2CloseBackend`: same `spawn_helper_backend`, same `wait_h2_accept_ready`
readiness poll, same `Drop`/`kill_and_reap`. Its test likewise goes past the
plan's sketch (which asserts only `port() > 0` and the container host) and
round-trips an actual request, because a spawn that merely becomes accept-ready
would satisfy the sketch while emitting no trailers at all.

**Step 6 — GREEN, and the new backend test PROVEN to have run rather than
self-skipped** (the sibling helper tests return early when the binary is not
built, which is a silent pass):

```
$ cargo test -p http2-echo-server
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
$ cargo test -p differential --lib backend
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 143 filtered out
$ cargo test -p differential --lib http2_trailers_backend_spawns_and_emits_trailers -- --nocapture
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 164 filtered out
```

The third run is the one that matters: `1 passed` with no `skipping …` line on
stdout. `cargo build --workspace --all-targets`, `cargo fmt --all -- --check` and
`cargo clippy -p differential -p http2-echo-server --all-targets --all-features
-- -D warnings` all clean.

---

## Task 6 — The differential driver OBSERVES trailers ✅

**Files:** `tests/differential/src/lib.rs`.

**Steps 1/2 — RED:**

```
$ cargo test -p differential --lib drive_http2_surfaces_response_trailers
error[E0609]: no field `trailers` on type `DriveHttp1Result`   (×3)
EXIT=101
```

**Step 3 — the field, and the ORDERING trap the plan flags as most likely to
bite.** `body_stream.trailers().await` is inserted immediately after the
body-drain loop and **BEFORE** the `drop(send_request); conn_handle.abort();`
block. Reading after the abort would silently report zero trailers — a false
green on the very cell fixture `0090` exists to witness — so the comment at the
site says so explicitly rather than leaving the ordering to look incidental.

Both `DriveHttp1Result` struct-literal sites were updated, exactly two as the
plan measured: `drive_http2` supplies the real block, and `drive_http1` supplies
`Vec::new()` with CF-111-2 named (the H1 chunked decoder discards trailers and
H1 trailer forwarding is unbuilt).

**Step 4 — GREEN, and neither new test self-skipped** (both would return early
if `http2-echo-server` were unbuilt, which is a silent pass):

```
$ cargo test -p differential --lib drive_http2
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 164 filtered out
$ cargo test -p differential --lib
test result: ok. 165 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
```

3 = the pre-existing `drive_http2_round_trip_against_in_process_listener` + the
2 new ones, with no `skipping …` line printed. The trailerless control
(`drive_http2_reports_no_trailers_when_none_sent`, against the plain
`Http2EchoBackend`) is what stops the positive test from being satisfied by a
driver that fabricates entries. `cargo build --workspace --all-targets`,
`cargo fmt --all -- --check` and `cargo clippy -p differential --all-targets
--all-features -- -D warnings` all clean.

---

## Task 7 — The differential harness COMPARES trailers ✅

**Files:** `tests/differential/src/lib.rs`.

**Step 1 — the plan's YAML tests needed adapting, as it warned.**
`load_expectations` takes a **`&Path`**, not a `&str` (`lib.rs:1261`), so the
plan's two YAML-string round-trip tests cannot call it. They parse via
`serde_yaml::from_str::<Expectations>(yaml)` instead — the same type, the same
`deny_unknown_fields` surface, the same assertion. A **sixth** test was added
beyond the plan's five: `fixture_0010_expectations_still_parse_without_expected_trailers`
loads the LANDED fixture `0010`'s file through the real `load_expectations`, so
the "89 pre-existing fixtures keep deserializing" claim is witnessed against a
file on disk rather than only against a hand-written string.

**Step 2 — RED:**

```
$ cargo test -p differential --lib expected_trailers
error[E0026]: variant `Driver::Http2` does not have a field named `expected_trailers`  (×3)
error[E0433]: cannot find type `Http1TrailerRule` in this scope
EXIT=101
```

**Steps 3/4 — the rule, the field, the dispatch, the comparison.**
`Http1TrailerRule` is an externally-tagged unit variant copying
`Http1HeaderRule`'s shape; `Driver::Http2` gains `#[serde(default)]
expected_trailers`. The comparison at the end of `run_http2_arm` **reuses
`diff_headers` verbatim** (PV-4) — no `diff_trailers` — because
`BEHAVIOR_CONTRACT.md`'s matrix row for response trailers literally says
"Set-equal under the same allow-list discipline". Its `.context(…)` string says
`diff_trailers` so a failure names the trailer axis rather than the header one.

**`deny_unknown_fields` ordering honoured:** the Rust field lands HERE, before
any fixture YAML mentions the key — which is why Task 9 comes after this task.

**Steps 5/6 — GREEN, after adjudicating one RED that is NOT this phase's.**
The first full-suite run failed one test:

```
---- tests::wait_accept_ready_times_out_for_closed_socket stdout ----
panicked at tests/differential/src/lib.rs:8639:9: assertion failed: result.is_err()
test result: FAILED. 170 passed; 1 failed; 2 ignored
```

**Classified by ISOLATION, which is the only thing that classifies** — never by
the failure text:

```
$ cargo test -p differential --lib wait_accept_ready_times_out_for_closed_socket
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 172 filtered out
$ cargo test -p differential --lib --no-fail-fast          # re-run of the WHOLE suite
test result: ok. 171 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
```

It passes alone and was absent from the very next sweep of the same tree — the
ADR-0164 startup-race tail signature. It is the known `wait_accept_ready`
closed-socket **port-reuse** flake: the test drops a listener and asserts the
port then refuses, and a parallel test re-binds the freed port. Its surface
(`reserve_port`/`wait_accept_ready`) is untouched by this phase. **Not a
regression, and no test was weakened.**

171 = 165 (post-Task-6) + the 6 new tests. `cargo build --workspace
--all-targets`, `cargo fmt --all -- --check` and `cargo clippy -p differential
--all-targets --all-features -- -D warnings` all clean.

---

## Task 8 — `{{HTTP2_TRAILERS_BACKEND_PORT}}` fixture-token plumbing ✅

**Files:** `tests/differential/src/lib.rs`.

**Step 1 — scan + spawn + port binding**, placed beside its `H2_CLOSE_BACKEND_PORT`
sibling. The `_h2_trailers_backend` binding is the child process's keep-alive —
dropping it kills the backend — so it sits with its siblings rather than later,
and the comment at the site says so.

**Step 2 — FOUR substitution edits, not two.** This is the trap the handoff
names, so it was verified by COUNT rather than by inspection:

```
$ grep -c 'HTTP2_TRAILERS_BACKEND_PORT'          tests/differential/src/lib.rs
3     # = 1 scan marker + 2 kv-push sites (upstream side, subject side)
$ grep -c 'h2_trailers_backend_port_str.is_some()' tests/differential/src/lib.rs
2     # = the two BACKEND_HOST guard chains, one per proxy side
```

Both `.push((\"HTTP2_TRAILERS_BACKEND_PORT\", …))` blocks and both `.is_some()`
guard arms were applied via a replacement asserted to match **exactly twice**,
so a single-side edit could not pass silently. Missing either guard is the quiet
failure mode: the port token renders, `{{BACKEND_HOST}}` does not, and the
fixture then fails with an unsubstituted `{{…}}` reaching the config parser
instead of with a trailer mismatch — a comment at the site records that symptom.

**Step 3 — verification.** There is deliberately no unit test for the token
itself; fixture `0090` in Task 9 is its real test.

```
$ cargo build --workspace --all-targets      # clean
$ cargo test -p differential --lib --no-fail-fast
test result: ok. 171 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
```

Unchanged at 171 — this task adds no test, and nothing regressed.
`cargo fmt --all -- --check` and `cargo clippy -p differential --all-targets
--all-features -- -D warnings` clean.

---

## Task 9 — Fixture `0090-h2-response-trailers` — the differential witness ✅

**Files created:** `tests/fixtures/0090-h2-response-trailers/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`,
`tests/differential/tests/h2_response_trailers.rs`. No `inputs/` (the H2 driver
reads none). Fixture census **89 → 90**; differential test files **89 → 90**.

**Steps 1/2 — the YAMLs were DERIVED from fixture `0010`'s files on disk, not
transcribed from the plan**, then the derivation was proved by diff:

```
$ diff <(sed -n '/^node: {/,$p' …0010…/envoy.yaml) <(sed -n '/^node: {/,$p' …0090…/envoy.yaml)
1c1   node.id / cluster labels
47c47 {{HTTP2_BACKEND_PORT}} -> {{HTTP2_TRAILERS_BACKEND_PORT}}
$ diff … envoy-rust.yaml …
1c1, 34c34   (the same two lines)
```

**Exactly two lines differ per side.** That is the strongest available statement
of D-PLAN-7's requirement that `generate_request_id: false` and the six-entry
`request_headers_to_remove` list carry over intact — they are load-bearing for
the byte-exact echoed body, and a hand-transcribed YAML could have dropped one
silently.

**Step 3 — `expectations.yaml`** carries `expected_trailers:
set_equal_modulo_allow_list` and deliberately NO per-driver `expected_body`
(the cross-proxy `equivalence.response_body: byte_exact` is the real assertion;
hard-coding the echoed request shape a second time would make the fixture fail
on any unrelated request-header change).

**Step 6 — GREEN cross-proxy on the first run:**

```
$ cargo build -p envoy-bin        # the harness runs the DEBUG binary
$ cargo test -p differential --test h2_response_trailers -- --nocapture
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.81s
```

**Step 7 — the vacuity proof, and it took THREE runs to get right.** Recorded in
full, because two of the three are instructive failures.

**(a) The control failed for a BOOKKEEPING reason first** — the exact trap the
traps ledger records. The scratch worktree was built with `cargo build -p
envoy-bin`, and the fixture died in 0.00s:

```
fixture green: spawning Http2TrailersBackend
Caused by: 1: http2-echo-server not found at …/mut-111-t9/target/debug/http2-echo-server
```

An assertion NEVER REACHED. Had this been the MUTATED run it would have "confirmed"
the mutation for entirely the wrong reason. **A control worktree needs
`cargo build --workspace --all-targets`**, because the harness spawns helper
BACKEND binaries that `-p envoy-bin` does not build. Rebuilt properly, the
control is green:

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.68s
```

**(b) The ~2.7s green was AUDITED rather than trusted.** This fixture is not
backend-free, so a fast green deserves proof that an upstream Envoy container
genuinely ran. A `docker ps` poll (with a VALID `--format`, since an invalid
field prints template errors that read as "no containers") ran alongside the
control:

```
$ docker ps --format '{{.Image}} {{.Status}}'
envoyproxy/envoy:v1.33.0 Up Less than a second
```

The pinned image, up, during the run. The green is real.

**(c) The plan's mutation was MISAIMED — it went RED for the wrong reason.** The
plan mutates the emit seam's `if let Some(map) = trailer_map {` into a
never-taken branch. That does go RED, but with:

```
fixture green: envoy-rust http2 drive
Caused by: 0: H2 body data / 1: stream error received: stream no longer needed
```

— a FRAMING failure, not a trailer failure. The reason is structural: that
mutation removes the `send_trailers` call while leaving
`send_data(body, trailer_map.is_none())` computing `false`, so END_STREAM rides
no frame at all and the stream is simply never closed. It proves the fixture is
sensitive to *something* on the emit path; it does not prove the fixture asserts
the TRAILER block.

**Re-aimed at the forward itself** — reproducing exactly the pre-phase behaviour
(trailers read and threaded, then dropped at the emit call, which is literally
Task 1's placeholder):

```
$ grep -c 'let send_result = send_envoy_response(send_response, resp, trailers).await;'  # EXACTLY once
1
$ sed -i 's/…, resp, trailers).await;/…, resp, None).await;/'   # original 0, mutant 1
$ touch crates/envoy-http2/src/lib.rs && cargo build --workspace --all-targets
   Compiling envoy-http2 v0.0.0 (…/mut-111-t9/crates/envoy-http2)
$ cargo test -p differential --test h2_response_trailers
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.63s

fixture green: diff_trailers (set_equal_modulo_allow_list)
Caused by:
    header name sets differ: only-in-envoy=["x-trail-a", "x-trail-b"], only-in-envoy-rust=[]
```

**That failure text is the whole witness of this phase, stated by the harness
itself.** It proves four things at once, none of which the green run alone
proves: upstream Envoy really does emit both trailers; the harness really
observes them on the Envoy side; the assertion bites on the TRAILER axis
specifically (the `diff_trailers` context string names it, distinguishing it
from the header diff); and it is envoy-rust's forwarding — nothing else — that
makes the fixture green. **The fixture is not vacuous, and it is not passing
because both sides return zero trailers.**

All the mutation-hygiene gates hold: a `Compiling envoy-http2` line (no stale
binary), a `test result:` line (it RAN — a compile error is not a mutation RED),
FAILED, and a GREEN control from the same worktree. The scratch worktree was
removed — only that one; the four sibling `.claude/worktrees/agent-*` belong to a
parallel workstream. Main tree re-verified unmutated afterwards.

---

## Task 10 — `BEHAVIOR_CONTRACT.md` gains a `## Response trailers` section ✅

**Files:** `docs/envoy-rust/BEHAVIOR_CONTRACT.md`. **Contributes 0 to the
net-LoC budget** (the house metric excludes `docs/`), which is exactly why it is
last and could not be traded away for size.

**Step 1 — insertion point located by TEXT**, in a file that exceeds the read
limit and was paged, never read whole:

```
$ grep -n 'Response trailers'      docs/envoy-rust/BEHAVIOR_CONTRACT.md   ->   18  (the matrix row)
$ grep -n '^## Header allow-list'  docs/envoy-rust/BEHAVIOR_CONTRACT.md   -> 904
```

The new `## Response trailers` section is placed immediately BEFORE
`## Header allow-list`, since it reuses that allow-list. Post-insert the file is
4437 lines with `## Response trailers` at `:904` and `## Header allow-list`
pushed to `:1036`.

**Step 2 — the section**, eight parts, each traceable to a measurement in
`PLAN.md` §1: the forward rule (verbatim, announce header NOT consulted); the
announce header's pre-existing parity; the comparison discipline and the two
gaps it implies (CF-111-8 multiplicity, CF-111-9 order); the scope (H2 only,
CF-111-1/2/3, locally-generated replies carry none); the measured behaviours
envoy-rust MATCHES; the two it does NOT (CF-111-5, CF-111-6) stated plainly
rather than omitted, with the dead-strip reasoning; the stats (CF-111-7, no stat
asserted); and what remains unmeasured.

The row it populates — `| Response trailers | Set-equal under the same
allow-list discipline |` at `:18` — was seeded at phase 00 and has been an
aspiration ever since. It now has a rule and a witness.

**Step 3 — the contract and the code re-checked against each other**, because
they are meant to move in lockstep and a Global Constraint forbids this phase
from touching the list:

```
$ grep -n 'HEADER_ALLOW_LIST' -A 4 tests/differential/src/lib.rs
1212: pub const HEADER_ALLOW_LIST: &[(&str, AllowMode)] = &[
1213:     ("server", AllowMode::NameRequired),
1214:     ("date", AllowMode::NameRequired),
1215:     ("x-envoy-upstream-service-time", AllowMode::NameRequired), // 04.3 NEW
1216: ];
```

Exactly THREE entries, unchanged. `location` and `content-type` remain ABSENT.

---

## End-of-state-3 measurements

**These are a SMOKE CHECK, not the §7.5 gate adjudication.** State 4 is a
separate session and it is what runs `cargo deny check`, `cargo fmt --check`, the
conformance suites and the full Docker differential sweep, quoting every output.
What follows is only enough to prove this session did not hand state 4 a broken
tree.

### Net LoC — the honest number

```
$ git diff --numstat 0ba60db HEAD -- . ':(exclude)docs/' | awk '{a+=$1;d+=$2} END{print a,d,a-d}'
1559 34 1525
```

**1525 net against the plan's ≈916 — a 1.66× overrun that lands 25 lines OVER the
~1500 §6.1 threshold** (under the worst-observed landed-phase ratio of 1.75,
above the median 1.32). Thirteen files excluding `docs/`.

**§6.1 does not fire, and the reason is not a technicality.** The gate is
evaluated at state 2 against the PLAN's estimate (≈916), where it did not fire;
its *mid-execution* trigger is a single task's sub-step count exceeding ~10,
which was evaluated at Task 3 — the plan's own named candidate — and did not fire
either. Splitting a phase whose ten tasks are all landed and green would spend
six further sessions to no purpose. **But the number is recorded prominently
rather than buried: `PLAN.md` §6 predicted exactly this ("at the worst-observed
1.75× it lands ≈1603, i.e. over the gate") and it is now a THIRD datapoint for
the unlanded `.claude/drafts/DRAFT-ADR-split-thresholds.md`, alongside phase
110's twelve-session split and phase 111's own state-2 refusal to split.**

The overshoot is concentrated in test scaffolding rather than production code,
and the cause is structural: **no in-tree helper could observe a trailer frame**,
so `round_trip`, `spawn_upstream_h2_server_with_trailers`,
`drive_one_h2_request_through_hcm` + `ObservedH2Response`, and a wire-level test
for the backend mode all had to be written from scratch. When a phase introduces
an observable the test helpers cannot see, the OBSERVER has to be priced, not
just the feature.

### Workspace sweep

```
$ cargo test --workspace --no-fail-fast
binaries=167 passed=2247 failed=5 identity=2252
```

Counted with `grep -oE 'test result: (ok|FAILED)\. …'` — the `ok`-only form makes
`failed=0` true by construction — with the awk field numbers derived by printing
one matched line (`$4` passed, `$6` failed) rather than inherited.

**The binary count moved 166 → 167 exactly as predicted**, the new binary being
the auto-discovered `tests/differential/tests/h2_response_trailers.rs`, and
fixture `0090` ran and passed inside the sweep.

**The identity closes EXACTLY:**

```
pre-phase CI passed                     2228
new tests this phase                    + 24   (t1:6, t2:2, t3:2, t4:1, t5:3+1, t6:2, t7:6, t9:1)
                                        = 2252
local passed + failed                     2252
```

This is the strongest flake-vs-regression discriminator available and it closes
to the line: every one of the 24 new tests is accounted for, and nothing else
moved.

**The 5 local REDs are the recorded stable CORE flake set, unchanged and
untouched by this phase** — the four `access_log_*_upstream_reset` binaries
(`TcpCloseBackend`, IPv6-unreachable on this host) and
`admin_config_dump_server_info` (the `192.168.65.2` bridge-IP family). Extracted
from the `---- <name> stdout ----` markers, never by indentation. These fail
DETERMINISTICALLY in isolation on this host — that determinism IS the
environmental signature — and CI is authoritative for them. **No tail members
appeared, no differential fixture failed, and no test was weakened.**

### Per-crate confirmations (each run at its own task)

| crate | before | after |
|---|---:|---:|
| `envoy-http2` lib tests | 119 | **124** |
| `differential` lib tests | 165 | **171** |
| `http2-echo-server` | 7 | **10** |
| fixture dirs / differential test files | 89 / 89 | **90 / 90** |

`cargo build --workspace --all-targets`, `cargo fmt --all -- --check` and
`cargo clippy` (on every crate this phase touches) were run and clean at each
task's commit.

---

## State advanced

`STATE.md` moved to **§5 state-3-COMPLETE**; the next unit is the **§5 state-4
VERIFICATION GATE**, a separate session (§5.1; ADR-0127).

The ADR-0035 relocation was performed and VERIFIED, not asserted: **16 lines
relocated** (the four top sections' superseded blocks — 5 + 4 + 4 + 2 — plus the
`### Doctrine reminders` §5.1 bullet), archived into `STATE_HISTORY.md` under
headers resolved by EXACT whole-line equality to exactly one line each, captured
from a PRE-EDIT backup, with length-changing splices done bottom-most first.
Checks that passed: `(old STATE − new STATE)` equals the relocated 16 plus the 10
in-place-superseded Notes lines; every relocated line is 0× in the new
`STATE.md` and exactly +1× in `STATE_HISTORY.md` (a PER-FILE delta — a combined
count is invariant by construction and false-passes); the pre-edit
`STATE_HISTORY.md` is a SUBSEQUENCE of the post-edit file, proving a pure
insertion with zero lines lost.

The active phase's Notes subsection was RENAMED IN PLACE
(`### Phase-111 §5 state-2 PLAN-write` → `### Phase-111 §5 state-3
implementation`) with its bullets superseded in place, per the measured mid-arc
rule; those 10 lines are deliberately NOT archived and will be relocated at the
state-6 close-out.

**The ADR-0160 token sweep found three real drops, each adjudicated against the
PERMANENT record rather than merely the archive:** `hpack`, `http2::Framer` and
the `…/111-h2-response-trailer-forwarding/PLAN.md` path, all from the superseded
state-2 Notes bullets. All three survive in `PLAN.md` and in `ADR-0182` (and the
path's referent exists on disk), which is the stronger outcome and is what makes
in-place Notes supersession safe. The universe was built by pairing backticks
PER LINE — a whole-file regex pairs them globally and manufactures phantom
zeroes.

**The Standing-traps line was REWRITTEN, not merely preserved.** Its preamble
carried six claims this session's own commits falsified; byte-preservation
protects against loss, not against staleness. The enduring doctrine tail
(183 610 chars) was carried forward BY PYTHON SLICE and asserted with
`endswith`. A new block was spliced in front of the first existing marker.
Census re-measured AFTER the write, over both named spans: **traps line alone
anchored 52 / naive 55; whole `STATE.md` anchored 52 / naive 55** (the two spans
still coincide — every anchored marker lives inside the traps line). The new
block was written so it never quotes the marker in prose, so it contributes
exactly +1 to each — a fixed point solved before the write and verified after
it. Line length 186 197 → **191 756** characters.

⚠ **One finding worth the next session's attention: the `### Doctrine reminders`
§5.1 bullet was STALE on arrival.** It still read "PHASE `111` … SITS AT §5
STATE-0/1" after the state-2 PLAN-write had landed, and
`git show be1aaf1 -- docs/envoy-rust/STATE.md | grep -c '§5.1'` returns **0** —
that advance did not touch the bullet at all, even though the traps line's own
relocation recipe names it as part of the superseded set. It has been rewritten
here and archived verbatim (stale claim included, corrected only in the
replacement). A recipe asserting that a line is always rewritten is not evidence
that it was.

---

# §5 STATE-4 — the §7.5 VERIFICATION GATE

> **What this section is.** The gate adjudication for phase `111`, run by a
> SEPARATE session from the one that wrote the implementation (§5.1; ADR-0127 —
> the context that wrote an artifact must not grade it). It records what was
> actually run and what it actually printed.
>
> **Session start:** HEAD `6a790abc0c59f0384ef561a6d4177faefbd50d1d` (the
> state-3 CI-RECORD commit), branch `main`, `git status --porcelain` EMPTY,
> `git fetch origin --prune` run BARE with its OWN exit code read (`fetch_exit=0`
> — a piped `$?` reads `tail`'s). `ls stop` → `No such file or directory`.
>
> **This session does NOT** write `REVIEW.md` (state 5), flip any `ROADMAP.md`
> row (state 6), fix any banked finding (§6.3; ADR-0165), or edit any landed
> artifact — `SPEC.md` and `PLAN.md` were confirmed untouched and stay so.

**Everything state 3 ran is re-run here from scratch, and four things run for
the FIRST TIME:** `cargo deny check`, `cargo fmt --all -- --check` as a gate
rather than a per-task courtesy, the h2spec conformance suite, and the full
Docker differential sweep over all **90** fixtures.

---

## Gate (e) — the five cargo commands

Run as ONE lock-serialised sequence in the main session (the cargo lock
serialises them; parallelising them would only queue them).

```
$ cargo build --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.09s
BUILD_EXIT=0

$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Checking envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
    Checking differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Checking envoy-health v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-health)
    Checking http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Checking envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.66s
CLIPPY_EXIT=0

$ cargo fmt --all -- --check
FMT_EXIT=0          # no output, which for --check IS the pass

$ cargo deny check
… 5 × warning[license-not-encountered] (BSD-2-Clause, MPL-2.0, Unicode-DFS-2016, Zlib, …)
advisories ok, bans ok, licenses ok, sources ok
DENY_EXIT=0
```

**The `build` line above is a fully-cached no-op (0.09s) and was NOT accepted as
evidence.** A `Finished` line with zero `Compiling` lines proves nothing about
whether the tree compiles. The gate was re-run behind a forced rebuild — an
**mtime-only `touch` of one crate root**, so the tree stays clean:

```
$ md5sum crates/envoy-http2/src/lib.rs      # BEFORE
5273ebf04feea38b100f0f6cc4424d84  crates/envoy-http2/src/lib.rs
$ touch crates/envoy-http2/src/lib.rs
$ md5sum crates/envoy-http2/src/lib.rs      # AFTER — byte-identical
5273ebf04feea38b100f0f6cc4424d84  crates/envoy-http2/src/lib.rs
$ git status --porcelain | wc -l
0
$ cargo build --workspace --all-targets
FORCED_BUILD_EXIT=0
COMPILING_LINES=4
```

Four genuine `Compiling` lines and exit 0. Gate (e)'s build is **not vacuous**.

**`cargo deny`'s four-ok line is ANSI-COLOUR-CODED**, so it was counted both
ways rather than trusted to a naive grep:

```
$ sed -e 's/\x1b\[[0-9;]*m//g' deny.log | grep -c 'advisories ok, bans ok, licenses ok, sources ok'   -> 1
$ grep -c 'advisories ok, bans ok, licenses ok, sources ok' deny.log                                  -> 1
$ warning/error census (ANSI-stripped): 5 warning[license-not-encountered], 0 errors
```

`license-not-encountered` on a green run is NORMAL (unmatched allowances in
`deny.toml`), and no RustSec advisory fired — no dep patch-bump was needed.

**These same five commands are also what CI runs**, verbatim, confirmed by
reading the step groups out of the CI log: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo build --workspace --all-targets`, `cargo test --workspace`,
`cargo deny check`.

**GATE (e): PASS.**

---

## The IDENTITY — re-derived twice, independently, and it closes EXACTLY

This is the strongest single measurement at this gate, so it was derived from
two sources with the field numbers **read off a printed line** rather than
inherited (they are log-source-dependent), and matching `(ok|FAILED)` rather
than `ok` alone (an `ok`-only grep discards `FAILED.` lines before `awk` sees
them, making `failed=0` TRUE BY CONSTRUCTION).

**Source 1 — CI on the state-3 advance commit `111b34a212675d332a506536dc090570da2f3b63`.**
The run was located with the FULL 40-char SHA (a short or retyped SHA returns
`[]`), and BOTH jobs were enumerated via the jobs API, because
`gh run view --log` returns only ONE job and a fuzz-only failure would be
invisible:

```
$ gh run list --commit $(git rev-parse 111b34a2) --json …
[{"attempt":1,"conclusion":"success","databaseId":33006543581,"headSha":"111b34a2…","status":"completed","workflowName":"ci"}]

$ gh api repos/pgdad/envoy-rust/actions/runs/33006543581/jobs --jq …
98301652877  fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse + grpc_health_decode,…)  success  steps=13  nonsuccess=0  runner=GitHub Actions 1000005535
98301653236  build + test + lint                                                                                  success  steps=15  nonsuccess=0  runner=GitHub Actions 1000005536
```

Attempt **1** — no rerun was needed. Both jobs `success`, **15 + 13 steps, ZERO
non-success steps in either**, and both carry a real `runner_name` (an empty
`runner_name` with `steps:0` would be runner STARVATION, not a result).

⚠ **The job-log endpoint bit, exactly as the traps ledger warns.**
`…/actions/runs/<run>/jobs/<job>/logs` returns a **131-byte** `404 Not Found`
body — and every `grep -c` over it returns a believable ZERO. The correct path
is `…/actions/jobs/<job>/logs`, which returns **417 582 bytes**. The
`wc -c is in the hundreds of KB` assertion is what caught it:

```
$ gh api repos/pgdad/envoy-rust/actions/runs/33006543581/jobs/98301653236/logs  ->  131 bytes  (404)
$ gh api repos/pgdad/envoy-rust/actions/jobs/98301653236/logs                   ->  417582 bytes
```

Fields derived from one matched line, then the identity:

```
$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' ci.log | head -1 | awk '{…}'
$1=[test] $2=[result:] $3=[ok.] $4=[171] $5=[passed;] $6=[0] $7=[failed]

$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' ci.log \
    | awk '{p+=$4; f+=$6; n++} END{printf "binaries=%d passed=%d failed=%d identity=%d\n",n,p,f,p+f}'
binaries=167 passed=2252 failed=0 identity=2252
```

Cross-checked independently of the awk: **167 `test result: ok.` lines, 0
`test result: FAILED.` lines, 0 `test <x> ... FAILED` lines, 0 `failures:`
blocks** over the whole CI job log. So `failed=0` is a measurement, not an
artifact of the pattern.

**Source 2 — two full local sweeps** (`cargo test --workspace --no-fail-fast`,
redirected to a FILE, never `tail` — `tail` truncates the `failures:` block):

| run | binaries | passed | failed | **identity** |
|---|---:|---:|---:|---:|
| CI (`111b34a2`) | 167 | 2252 | 0 | **2252** |
| local sweep 1 | 167 | 2244 | 8 | **2252** |
| local sweep 2 | 167 | 2244 | 8 | **2252** |

**The `+24` numerator was DERIVED FROM THE DIFF, not inherited from the
handoff.** A non-permissive pattern under-counts it (it misses the
`#[tokio::test(flavor = "multi_thread")]` form — a first attempt read 15):

```
$ git diff 0ba60db HEAD -- . ':(exclude)docs/' | grep -E '^\+[[:space:]]*#\[(tokio::)?test'
      8  #[test]
      7  #[tokio::test]
      9  #[tokio::test(flavor = "multi_thread")]
   TOTAL added: 24      TOTAL removed: 0
```

And the per-file attribution matches the handoff's per-task claim exactly:

| file | added tests | handoff's task attribution |
|---|---:|---|
| `crates/envoy-http2/src/response.rs` | 6 | t1:6 |
| `crates/envoy-http2/src/client.rs` | 2 | t2:2 |
| `crates/envoy-http2/src/hcm.rs` | 3 | t3:2 + t4:1 |
| `tests/helpers/http2-echo-server/src/main.rs` | 3 | t5:3 |
| `tests/differential/src/backend.rs` | 1 | t5's +1 |
| `tests/differential/src/lib.rs` | 8 | t6:2 + t7:6 |
| `tests/differential/tests/h2_response_trailers.rs` | 1 | t9:1 |
| **total** | **24** | **24** |

```
pre-phase CI passed          2228
new tests this phase         + 24   (measured from the diff, not inherited)
                             = 2252
CI passed + failed             2252   ✅
local sweep 1 passed + failed  2252   ✅
local sweep 2 passed + failed  2252   ✅
```

**Binary count moved 166 → 167** exactly as `PLAN.md` §5 predicted, the new
binary being the auto-discovered `tests/differential/tests/h2_response_trailers.rs`
— no `ci.yml` edit was made and none was needed.

**And the identity MOVED, which a code push requires.** The record commit
`6a790ab` on top is DOCS-ONLY and must not move it — verified per-file:

```
$ git show --numstat --format='' 111b34a2      25 26 STATE.md | 35 0 STATE_HISTORY.md | 146 0 PROGRESS.md
$ git show --numstat --format='' 6a790ab        2  0 STATE.md
$ git diff --name-only 111b34a2 HEAD -- . ':(exclude)docs/' | wc -l      ->  0
```

Zero code files between the CI-tested commit and this session's HEAD, so **CI on
`111b34a2` is authoritative for the code at HEAD.**

---

## Gates (a) and (b) — the Docker differential sweep over all 90 fixtures

The differential fixtures run inside `cargo test --workspace`, so the two local
sweeps above ARE the local differential sweep, and the CI job log is the
authoritative one — **this host routes the backend via `192.168.65.2` rather
than the allow-listed addresses, so backend-routing fixtures go RED here by
construction.**

**Gate (a) — fixture `0090-h2-response-trailers` GREEN cross-proxy, in CI:**

```
     Running tests/h2_response_trailers.rs (target/debug/deps/h2_response_trailers-21f92e003cbe1a82)
running 1 test
… INFO node registered node.id=envoy-rust-phase-111-fixture-0090 node.cluster=envoy-rust-phase-111
… INFO listener bound with SO_REUSEPORT (one accept queue per worker) addr=127.0.0.1:33977 sockets=4
… INFO envoy-rust listening (http_connection_manager) addr=127.0.0.1:33977 stat_prefix=ingress_http2 codec_type=HTTP2
test h2_response_trailers ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.52s
```

**The ~2.5 s green is NORMAL for this fixture, not a silent skip**, and the log
lines above are why: envoy-rust genuinely bound a listener under the fixture's
own node id, and CI pulls the pinned image in a dedicated step whose tag is
greped out of `tests/differential/src/upstream.rs` so it cannot drift. The
fixture also ran GREEN inside both local sweeps.

**Gate (b) — all 89 pre-existing fixtures still green.** Adjudicated in CI,
where the whole differential surface is reachable:

```
differential test files on disk                                 90
… with a `Running tests/<name>.rs (target/debug/deps/…)` line in the CI log   90   (0 missing)

whole CI job log:
  test result: ok.       lines   167
  test result: FAILED.   lines     0
  test <x> ... FAILED    lines     0
  failures:              blocks    0
```

Every test binary that runs emits exactly one `test result:` line. 167 result
lines, all `ok`, zero `FAILED` anywhere, and all 90 fixture binaries present ⇒
**every one of the 90 differential binaries was green in CI**, which is gate (a)
and gate (b) together.

⚠ **One methodological note for the reviewer.** A first pass tried to pair each
`Running tests/<f>.rs` line with the next `test result:` line and reported
89/90, flagging `access_log_rf_connect_failure`. That was a **bug in the
adjudication script, not a red fixture** — CI runs test binaries in parallel and
`Running tests/access_log_rf_no_healthy.rs` interleaved *between* the previous
binary's `test … ok` line and its `test result:` line. The fixture is green
(`test access_log_rf_connect_failure ... ok`). Recorded because a pairing
heuristic over an interleaved log is exactly the kind of measurement that
manufactures a believable false RED.

**The fixture's own derivation was re-proved rather than inherited** (D-PLAN-7's
requirement that `generate_request_id: false` and the six-entry
`request_headers_to_remove` list carry over intact — they are load-bearing for
the byte-exact echoed body):

```
$ diff <(sed -n '/^node: {/,$p' …0010…/envoy.yaml)      <(sed -n '/^node: {/,$p' …0090…/envoy.yaml)
1c1   node.id / cluster        47c47  {{HTTP2_BACKEND_PORT}} -> {{HTTP2_TRAILERS_BACKEND_PORT}}
$ diff <(sed -n '/^node: {/,$p' …0010…/envoy-rust.yaml) <(sed -n '/^node: {/,$p' …0090…/envoy-rust.yaml)
1c1   node.id / cluster        34c34  {{HTTP2_BACKEND_PORT}} -> {{HTTP2_TRAILERS_BACKEND_PORT}}
```

**Exactly two lines differ per side.** Fixture census re-derived: **90** fixture
directories, **90** differential test files, highest `0090-h2-response-trailers`.

**GATES (a) and (b): PASS.**

---

## Gate (c) — h2spec at its declared threshold, `known-failures.txt` UNTRIMMED

**The declared threshold, read from the runner rather than remembered:**

```
$ grep -n 'PASS_RATE_GATE' tests/conformance/h2spec/tests/h2spec_runner.rs
18:  const PASS_RATE_GATE: f64 = 0.95;
120:      pass_rate >= PASS_RATE_GATE,
```

**`known-failures.txt` is UNTRIMMED — proved three ways:**

```
$ git diff --stat 0ba60db HEAD -- tests/conformance/          (empty — phase 111 touched nothing under it)
$ wc -l  tests/conformance/h2spec/known-failures.txt          21
$ md5sum tests/conformance/h2spec/known-failures.txt          19cd44d86a8b15d825f76c6e7b265e65
$ git log -1 --format='%h %ad %s' --date=short -- …/known-failures.txt
dac3f8b 2026-05-03 phase 05.2: post-CI fixup — h2spec parser + 3.5/2 known-failure (task 13/14)
```

Last touched 2026-05-03, three months and ~50 phases ago.

**⚠ The gate SELF-SKIPS SILENTLY on this host — and that was DEMONSTRATED here,
not merely cited.** A local run with `--nocapture` prints the skip line and then
reports `ok` anyway:

```
$ cargo test -p h2spec-conformance --test h2spec_runner -- --nocapture
running 3 tests
test tests::parse_summary_line_extracts_pass_fail_counts ... ok
test tests::parse_h2spec_output_extracts_section_failure_ids ... ok
h2spec_runner: h2spec not found — skipping locally
test h2spec_pass_rate_gate ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
H2SPEC_LOCAL_EXIT=0
```

**So the `h2spec_pass_rate_gate ... ok` that appeared in BOTH local workspace
sweeps is VACUOUS**, and any adjudication resting on it would be false. Without
`--nocapture` that skip line is invisible and the run is indistinguishable from
a real pass.

ADR-0163 SETTLED that it is NOT vacuous in CI, and that was re-proved here from
the CI log directly rather than cited:

```
$ grep -c 'h2spec not found' <CI job log>          ->  0        # over 417 KB of log
$ h2spec install step:  curl … summerwind/h2spec/releases/download/v2.6.0/… | sudo tar xz -C /usr/local/bin
                        h2spec --version
                        Version: 2.6.0 (70ac2294010887f48b18e2d64f5cccd48421fad1)
$ test h2spec_pass_rate_gate ... ok
```

The binary was genuinely downloaded and reported its version, and the string
`h2spec not found` occurs **ZERO** times anywhere in the log — so the gate ran
and passed its 0.95 threshold in CI, on an untrimmed known-failures list.

**GATE (c): PASS — CI-authoritative per ADR-0163, and the local run is proven
VACUOUS rather than assumed so.** The two halves of that sentence are the whole
adjudication: the skip line appears LOCALLY (so the local green means nothing)
and appears ZERO times in CI (so the CI green means everything).

---

## Gate (d) — no new fuzz target, VERIFIED rather than inherited

The claim to test is §7.4's: a phase ships a fuzz target only if it introduces
a **parser, codec or filter**. `SPEC.md` §4 and `PLAN.md` §8 both assert `h2`
owns the trailer framing so none is needed. That is a claim; here is the check.

**1 — the target/step census is 1:1 and unmoved:**

```
$ find crates -path '*/fuzz/fuzz_targets/*.rs'        5  (accesslog_format_parse, parse_bootstrap,
                                                          cdn_loop_parse, grpc_health_decode, jwt_parse)
$ grep -n 'cargo +nightly fuzz run' .github/workflows/ci.yml     5  (one step per target)
$ git diff --name-only 0ba60db HEAD | grep -ci fuzz              0
```

(The `.claude/worktrees/agent-*` copies of these directories belong to a
parallel workstream and are not this tree.)

**2 — the new code contains no hand-written parser.** Both new conversions are
delegations, in each direction:

```rust
// emit side — crates/envoy-http2/src/response.rs
let header_name  = HeaderName::from_bytes(name_lc.as_bytes()).map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
let header_value = HeaderValue::from_str(value).map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
map.append(header_name, header_value);

// read side — crates/envoy-http2/src/client.rs
let Ok(value_str) = value.to_str() else { continue };      // skip non-ASCII, same posture as the header loop
out.push((name.as_str().to_string(), value_str.to_string()));
```

Every byte-level validation is `http`'s (`HeaderName::from_bytes`,
`HeaderValue::from_str`, `HeaderValue::to_str`); every frame-level validation
including hpack decoding is `h2`'s. There is no new state machine, no new
tokenizer, no length arithmetic over attacker-controlled bytes, and no new
filter — `crates/envoy-filter/` was confirmed untouched (0 files).

**GATE (d): PASS — no new fuzz target is required, and none was added.**

---

## The local REDs — sixteen across two sweeps, classified BY ISOLATION and only by isolation

Two full sweeps were run with `--no-fail-fast`, redirected to files, with a
settle gap between them, and their RED SETS were diffed. Failing tests were
extracted from the `---- <name> stdout ----` markers, never by indentation.

```
sweep 1:  binaries=167 passed=2244 failed=8   identity=2252
sweep 2:  binaries=167 passed=2244 failed=8   identity=2252
```

| | members |
|---|---|
| **in BOTH sweeps (5)** | `access_log_h2_rcd_upstream_reset`, `access_log_h2_uc_upstream_reset`, `access_log_rcd_upstream_reset`, `access_log_rf_upstream_reset`, `admin_config_dump_server_info` |
| **sweep 1 ONLY (3)** | `http_filter_jwt_authn_fixture`, `http_filter_rbac_fixture`, `rbac_matcher_value_enrichment` |
| **sweep 2 ONLY (3)** | `admin_ready_returns_200_post_migration`, `backend::tests::tls_echo_backend_spawns_and_echoes`, `set_metadata_dynamic_metadata` |

⚠ **The two sets have the SAME SIZE and DISJOINT TAILS. That is the signal, and
the size is not** (ADR-0164: the startup-race tail's size carries no signal).
The intersection is exactly the recorded stable CORE set.

**Every one of the eleven was then run in ISOLATION**, with a **90 s settle
before the first and 45 s between each** — back-to-back Docker-spawning runs
MANUFACTURE a false `FAILS-IN-ISOLATION`, and that has produced a wrong answer
in this repo before. Targets came from cargo's OWN
`to rerun pass '-p differential --test X'` hints rather than a hand-written list
(two of the RED names are test-FUNCTION names, not file names).

| test | in sweeps | **isolation** | class |
|---|---|---|---|
| `access_log_h2_rcd_upstream_reset` | both | **FAILED** (exit 101) | CORE — environmental |
| `access_log_h2_uc_upstream_reset` | both | **FAILED** (exit 101) | CORE — environmental |
| `access_log_rcd_upstream_reset` | both | **FAILED** (exit 101) | CORE — environmental |
| `access_log_rf_upstream_reset` | both | **FAILED** (exit 101) | CORE — environmental |
| `admin_config_dump_server_info` | both | **FAILED** (exit 101) | CORE — environmental |
| `http_filter_jwt_authn` | sweep 1 only | **ok** (1 passed) | TAIL — startup race |
| `http_filter_rbac` | sweep 1 only | **ok** (1 passed) | TAIL — startup race |
| `rbac_matcher_value_enrichment` | sweep 1 only | **ok** (1 passed) | TAIL — startup race |
| `set_metadata_dynamic_metadata` | sweep 2 only | **ok** (1 passed) | TAIL — startup race |
| `backend::tests::tls_echo_backend_spawns_and_echoes` | sweep 2 only | **ok** (1 passed) | TAIL — startup race |
| `admin_ready_returns_200_post_migration` | sweep 2 only | **ok** (1 passed) | TAIL — startup race |

**The FIVE CORE fail DETERMINISTICALLY in isolation, and that determinism IS the
environmental signature** — not a regression indicator. Their causes are the
recorded ones and were read rather than assumed:

```
access_log_rf_upstream_reset:   access log byte-exact mismatch: line 0
    envoy="{\"rc\":503,\"rf\":\"UF\"}"  envoy-rust="{\"rc\":503,\"rf\":\"UC\"}"
        -> the TcpCloseBackend family; IPv6 is unreachable on this host.

admin_config_dump_server_info:  admin body rule: /clusters — text_lines diverged after allow-lists
    envoy-only: ["backend::192.168.65.2:35521::canary::false", …]  envoy-rust-only: []
        -> the 192.168.65.2 host-bridge-IP family; this host does not route the
           backend through the allow-listed addresses.
```

Both are **CI-authoritative and both are GREEN in CI**, where the routing is
normal — which is the direct proof they are host artefacts rather than defects.

**The SIX TAIL members all reached a readiness wait, never their subject
assertion** — ADR-0164 part 1 — and each is absent from one of the two sweeps —
ADR-0164 part 3. Five read `upstream Envoy never became accept-ready … not
accept-ready within 10s: Connection refused (os error 111)` and one reads
`WouldBlock: Resource temporarily unavailable` on `drive /ready`. ⚠ **None of
those texts was used to classify them** — classification is by isolation only,
because one recorded member of this same family reads `104 ConnectionReset /
"clean EOF, not RST"`, which looks semantic and is not.

**ADR-0164 part 4 — untouched by this phase's surface.** The phase's 13 changed
files are the `envoy-http2` trailer path, the differential harness's trailer
observation/comparison, the `http2-echo-server` `--trailers` mode and fixture
`0090`. None of the eleven REDs touches trailers, and none of the six tail
members' surfaces (`reserve_port` / `wait_accept_ready` / container readiness)
is in the diff at all.

**Every one of the eleven is GREEN in CI** — the authoritative run reads
`167 test result: ok.` lines with zero `FAILED`.

**No test was weakened, `known-failures.txt` was not trimmed, and no RED was
explained away by its text.**

---

## Net LoC and the §6.1 threshold — re-derived, and it is over

```
$ git diff --numstat 0ba60db 6a790ab -- . ':(exclude)docs/' | awk '{a+=$1;d+=$2;n++} END{…}'
added=1559  deleted=34  net=1525  files=13
$ git diff --numstat 0ba60db 6a790ab                          # docs INCLUDED, for contrast
added=2696  deleted=60  net=2636  files=17
```

**1525 net excluding `docs/` against the plan's ≈916 — 1.66×, and 25 lines OVER
the ~1500 §6.1 threshold.** State 3's figure is confirmed to the line.

⚠ **The range is cited as `0ba60db 6a790ab`, not `0ba60db HEAD`.** A range
ending at `HEAD` DRIFTS as this session's own state-advance commit lands; the
figure above is anchored at the carrying commit and stays true.

**§6.1 does NOT fire, and this gate does not re-open it.** The gate is evaluated
at state 2 against the PLAN's estimate (≈916), where it did not fire; its
*mid-execution* trigger is a single task's sub-step count exceeding ~10, which
was evaluated at Task 3 — the plan's own named candidate — and did not fire
either. §6.1 has no retroactive clause and splitting ten landed, green tasks
would spend six further sessions to no purpose. **Recorded, not acted on.**

**It is a THIRD datapoint for the unlanded `.claude/drafts/DRAFT-ADR-split-thresholds.md`,
and the sharpest of the three:** a phase that CLEARED the gate on its estimate
would have FAILED it on its actual, while being by every other measure a clean,
well-scoped, single-cell phase whose ten tasks each landed green in one pass.
The draft binds nothing and was not edited.

**Commit shape, audited.** Twelve commits between `0ba60db` and HEAD — ten task
commits (`d94b3c0` … `a2c5589`), the state-3 advance `111b34a2` (`25 26`
STATE.md, `35 0` STATE_HISTORY.md, `146 0` PROGRESS.md, **`ROADMAP.md`
deliberately NOT touched**), and the CI-record commit `6a790ab` (`2 0`).

---

## Global Constraints — every one re-verified at this gate

| constraint | measured at HEAD | verdict |
|---|---|---|
| no new dependency; `Cargo.lock` unchanged | `git diff --name-only 0ba60db HEAD -- Cargo.lock '**/Cargo.toml' …` → EMPTY | ✅ |
| no `ci.yml`, no `deny.toml` change | same command, EMPTY | ✅ |
| `crates/envoy-filter/` untouched | 0 files | ✅ |
| `crates/envoy-http1/` untouched | 0 files | ✅ |
| `#![forbid(unsafe_code)]` at every crate root | 21 roots checked, 0 missing | ✅ |
| no `unsafe` introduced | 0 added lines matching `\bunsafe\b` | ✅ |
| `HEADER_ALLOW_LIST` still 3 entries | `server`, `date`, `x-envoy-upstream-service-time` at `tests/differential/src/lib.rs:1212`; `location`/`content-type` ABSENT | ✅ |
| `known-failures.txt` untrimmed | md5 `19cd44d8…`, 21 lines, last touched `dac3f8b` 2026-05-03 | ✅ |
| landed artifacts unedited | `git diff --name-only 0ba60db HEAD -- <phase dir>` lists **`PROGRESS.md` only** — `SPEC.md` and `PLAN.md` untouched | ✅ |
| no ADR added by state 3 | `DECISIONS.md` 0 files changed; head still `ADR-0182`, next free `ADR-0183` | ✅ |
| no `ROADMAP.md` row flipped | 0 files changed; row `111` still `planned` | ✅ |
| nothing fixed from the CF ledger | no banked finding touched | ✅ |

**The `H2SendTrailers` widening was re-checked with my OWN greps**, not the
plan's, because widening a returnable error set can land in a caller's
`unreachable!()` and gate (e) is blind to it:

```
$ git grep -c 'unreachable!\|unimplemented!' -- 'crates/envoy-http2/**/*.rs' 'crates/envoy-bin/**/*.rs'   ->  0 files
$ git grep -c 'match .*Http2Error' -- '*.rs'                                                              ->  0 files
```

**D-PLAN-3's four-row end-of-stream table was checked against the landed code**,
because "implementation complete" is itself a claim:

```rust
let mut send_stream = send_response.send_response(head, /* end_of_stream = */ body_empty && trailer_map.is_none())…;
if !body_empty { send_stream.send_data(resp.body, /* end_of_stream = */ trailer_map.is_none())…; }
if let Some(map) = trailer_map { send_stream.send_trailers(map)…; }
```

All four rows fall out of those three lines, including the gRPC main case
(empty body + trailers ⇒ **no DATA frame at all**). Matches the plan exactly.

**Citation audit of the phase's own new artifacts:** every qualified path in
`tests/fixtures/0090-h2-response-trailers/README.md` (7) and in
`tests/differential/tests/h2_response_trailers.rs` (1) resolves on disk; neither
file carries a `file:line` citation to go stale.

⚠ **BANKED FOR STATE 5 — `PROGRESS.md`'s three inherited `file:line` citations
have ALL drifted, under this phase's OWN commits.** They were true at `0ba60db`
when the pre-flight table measured them, and the phase's insertions moved them:

| citation in `PROGRESS.md` | resolves at HEAD to | drift |
|---|---|---|
| `hcm.rs:1043` (the sole production `send_envoy_response` caller) | `crates/envoy-http2/src/hcm.rs:1096` | +53 |
| `lib.rs:1189` (`HEADER_ALLOW_LIST`) | `tests/differential/src/lib.rs:1212` | +23 |
| `lib.rs:1261` (`load_expectations`) | `tests/differential/src/lib.rs:1291` | +30 |

This is the standing "a line citation is invalidated by your own edit" trap in
its cross-file form. **Not fixed here** — a state-4 session grades, it does not
edit what it grades (§5.1; ADR-0127), and none of the three is load-bearing for
any gate. `PROGRESS.md`'s own Task-10 section already quotes the correct `:1212`.

⚠ **CONFIRMED: state 3's `**[NEW AT ` census was MISLABELLED.** Its record reads
"traps line alone anchored 52 / naive 55; whole `STATE.md` anchored 52 / naive
55". Re-measured from disk at `6a790ab`, all THREE forms, over both spans:

| form | pattern | count |
|---|---|---:|
| ANCHORED | `re.findall(r'\*\*\[NEW AT [^\]]*\]\*\*')` | **49** |
| NAIVE | `str.count('**[NEW AT ')` | **52** |
| BARE | `str.count('[NEW AT ')` | **55** |

The two spans still coincide (every anchored marker lives inside the traps
line). **52 and 55 are real numbers for the NAIVE and BARE forms — they were
reported under the ANCHORED and NAIVE labels.** The genuinely anchored count,
49, had never been measured. Traps-line length at `6a790ab`: **191 756
characters** (193 445 bytes — the series is CHARACTERS).

---

## Stop condition — re-evaluated from disk at this gate. ALL THREE LEGS FALSE.

The mission is complete only when EVERY `ROADMAP.md` row is `done` AND no
in-scope leaf remains. Each leg measured independently and freshly:

**LEG (i) — FALSE.** Status is field **4** on a `' | '` split; a `NF == 6`
census DROPS the two rows carrying an in-cell pipe and misreads the table.

```
$ awk -F' \\| ' '/^\| [0-9]/ {n++; s=$4; gsub(/^ +| +$/,"",s); c[s]++} END {…}' docs/envoy-rust/ROADMAP.md
rows=117   done=116   planned=1   in-progress=0   blocked=0
$ grep -n '^| 111 ' docs/envoy-rust/ROADMAP.md      -> row 111, status `planned`
```

Row `111` is `planned` and stays `planned` until the state-6 close-out — neither
a state-3 commit, a record commit, nor this state-4 commit ever touches
`ROADMAP.md`.

**LEG (ii) — FALSE.**

```
crates: 14  (envoy-{accesslog,admin,bin,cluster,config,filter,health,http1,http2,jwt,listener,stats,tcp,tls})
envoy-http3 ABSENT | envoy-grpc ABSENT | envoy-wasm ABSENT | envoy-protos ABSENT | envoy-runtime ABSENT
quinn / tonic-web / wasmtime across crates/*/Cargo.toml:  0
tests/conformance/ holds only: h2spec
```

**LEG (iii) — FALSE.** Driven from a SINGLE `/^### /` rule plus a
`/^\| [0-9]/` row rule — an `awk` whose first rule matches `/^### .* family/`
and calls `next` never reaches a later `/^### /` rule and UNDER-REPORTS:

```
 10  ### HTTP filters family          5  ### Network filters family      3  ### Load balancing family
 14  ### Upstream robustness family   0  ### HTTP/3 + QUIC family        4  ### gRPC family
  6  ### xDS / dynamic config family 29  ### Observability family        6  ### Runtime + hot restart family
  0  ### WASM host family            13  ### Deprecated / edge features
```

**TWO family headings carry ZERO rows** (`### HTTP/3 + QUIC family`,
`### WASM host family`).

**ALL THREE LEGS MUST HOLD; NONE DOES. The mission is NOT complete. NO `stop`
file was created** (`ls stop` → `No such file or directory`, checked at session
start and again before the commit).

**Carry-forward set: live and ENTIRELY UNCONSUMED. Nothing was fixed at this
gate** (§6.3; ADR-0165 — a phase banks, it never clears; and a state-4 session
grades, it does not fix): **CF-111-1** (trailers bypass the filter pipeline),
**CF-111-2** (H1 trailers, blocked behind chunked response encoding),
**CF-111-3** (REQUEST trailers), **CF-111-4** (`%TRAILER%`/`%GRPC_STATUS%`),
**CF-111-5** (connection-specific trailer name ⇒ envoy-rust 503 vs Envoy
200+RST; pre-existing, inside the `h2` codec), **CF-111-6** (pseudo-header
trailer — a divergence this phase CREATES, now live since the forwarding
landed), **CF-111-7** (Envoy's `http2.trailers` stats exist and stay 0),
**CF-111-8** (duplicate trailer names unassertable under `diff_headers`),
**CF-111-9** (trailer wire ORDER doubly invisible); plus the `110.2` REVIEW's
M-1…M-8 + N-1…N-12, the `110.1` REVIEW's M-1…M-9 + N-1…N-10, CF-110-1…CF-110-9,
CF-109-1/2/3, CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6,
CF-74-1/2/3/4/6, CF-73-1, the `109.2`/`109.1`/`108.2` REVIEW sets, and the
HTTP-filters-family (1)-(4).

**ADR head is `ADR-0182`; NO number is reserved; the next free is `ADR-0183`.**
This session added no ADR, which is correct — a verification gate records in
`PROGRESS.md`, not in the decision log. (`grep -c '^## ADR-'` reads **179**, ONE
HIGH, because of a schema template at the file head; the MAX recipe
`grep -o '^## ADR-[0-9]\{4\}' | sort -t- -k2 -n | tail -1` returns `ADR-0182`.)

---

## §7.5 GATE ADJUDICATION — the verdict

| gate | verdict | the decisive evidence |
|---|---|---|
| **(a)** fixture `0090` green cross-proxy | **PASS** | CI: `test h2_response_trailers ... ok` / `1 passed; 0 failed … 2.52s`, with envoy-rust's listener genuinely bound under `node.id=envoy-rust-phase-111-fixture-0090`. Green in both local sweeps too. |
| **(b)** all 89 pre-existing fixtures green | **PASS** | CI: all **90** differential binaries present with a `Running` line; **167 `test result: ok.`, 0 `FAILED`, 0 `failures:` blocks** over the whole job log. |
| **(c)** h2spec at threshold, `known-failures.txt` untrimmed | **PASS** (CI-authoritative, ADR-0163) | h2spec **2.6.0** installed and version-reported in CI, `h2spec_pass_rate_gate ... ok` against `PASS_RATE_GATE = 0.95`, **0** occurrences of `h2spec not found`; known-failures md5 `19cd44d8…`, unchanged since 2026-05-03. |
| **(d)** no new fuzz target needed, none added | **PASS** (verified, not inherited) | 5 targets ↔ 5 `ci.yml` steps, unmoved; the phase's new code delegates all byte-level validation to `http` and all framing to `h2` — no new parser, codec or filter. |
| **(e)** the five cargo commands | **PASS** | all five exit 0 locally AND in CI; the build re-run behind a forced rebuild (**4 `Compiling` lines**) so the 0.09 s cached `Finished` is not the evidence; `cargo deny` four-ok line confirmed ANSI-stripped. |
| **(f)** `REVIEW.md` approved | **NOT THIS SESSION** | State 5, a separate session (§5.1; ADR-0127). |

**Five of the six gates in this session's scope PASS. The sixth, (f), is state 5's.**

**The identity closes exactly at 2252 from three independent runs** (CI, local
sweep 1, local sweep 2), against a `+24` numerator derived from the diff rather
than inherited. Nothing moved that should not have; the one thing that had to
move — the binary count, 166 → 167 — moved by exactly one.

**No test was weakened. `known-failures.txt` was not trimmed. No banked finding
was fixed. No landed artifact was edited. No `ROADMAP.md` row was flipped. No
ADR was added. No `stop` file was created.**

---

## Next state

**§5 state 5 — the CODE REVIEW** (`superpowers:requesting-code-review`), in a
SEPARATE session (§5.1; ADR-0127 — the context that graded an artifact must not
review it, and a reviewer must not fix what it grades). Its output is
`REVIEW.md`, which does not exist yet; `SPEC.md` + `PLAN.md` + `PROGRESS.md`
present with NO `REVIEW.md` is what made this session state 4, and
`REVIEW.md`'s appearance is what will make the next one state 6.

Per §5.2, an Issue at state 5 sends the work back to state **3**, not 4.

**Three things the reviewer should start from, all banked here rather than
fixed:** (1) `PROGRESS.md`'s three drifted `file:line` citations (table above);
(2) the state-3 `**[NEW AT ` census mislabel (table above); (3) the 1525-vs-1500
§6.1 observation, which is data for the unlanded split-thresholds draft and NOT
a defect in this phase.
