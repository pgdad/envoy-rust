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
