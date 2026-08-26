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
