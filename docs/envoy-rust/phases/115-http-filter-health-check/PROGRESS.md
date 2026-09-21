# Phase 115 — PROGRESS

> §5 **state 3** — the implementation of
> `docs/envoy-rust/phases/115-http-filter-health-check/PLAN.md`.
> Appended to on each task completion, with REAL quoted command output. Written
> for a stranger with zero prior context (D-3.4).
>
> **The unit:** `envoy.filters.http.health_check` in **non-pass-through mode** —
> an HTTP filter that answers a downstream liveness probe AT THE PROXY with an
> empty 200 carrying one `x-envoy-upstream-healthchecked-cluster` header, instead
> of forwarding the request. It is the THIRTEENTH production
> `HttpFilterInstance` variant. Witnessed by TWO new differential fixtures:
> `0095-http-filter-health-check` (the wire) and
> `0096-http-filter-health-check-stats` (the counters).
>
> **Where `PLAN.md` and `SPEC.md` disagree, `PLAN.md` wins** (`ADR-0201`, which
> corrects seven landed `SPEC.md` claims and adds the stat surface the SPEC
> omits).

---

## Session preconditions, re-derived from disk before Task 1

`git rev-parse HEAD` = `04661b7b862faac52e55633871f47d0fea975ec3`, the phase-115
§5 state-2 **CI-record** commit; `git status --porcelain` empty at entry; branch
`main`; `git fetch origin --prune` exit **0** (run BARE, not through a pipe) with
`origin/main` at the SAME SHA.

**No outstanding CI record is inherited, detected STRUCTURALLY** — the
`## Last commit` block of `STATE.md` was read and it already carries a
CI-confirmed answer: run `35153241604`, attempt 1, `success`, identity
`binaries=170 passed=2315 failed=0` on `0cb1f677745705bb254a0a4eb91b0dc372f13c0e`.
The block was never grepped for the outstanding-record marker text, which the
Standing-traps series quotes.

**The §5 state-3 detection rule was re-derived, not inherited:** the phase
directory holds `SPEC.md` (414 lines) and `PLAN.md` (2264 lines) and NO
`PROGRESS.md` and NO `REVIEW.md` before this file was created.

`git check-ignore next-prompt.txt` exit **0**, run STANDALONE — it IS ignored and
is never staged.

**Baseline `cargo build --workspace --all-targets` exit 0** before any edit
(`Finished \`dev\` profile … in 26.19s`).

### The three stop-condition legs, re-measured from disk — ALL THREE FALSE

- **Leg (i) — FALSE.** `ROADMAP.md`: **123 rows / 122 `done` / 1 `planned`**, the
  `planned` row being `115` at file line **78**. Driven from the `^\| [0-9]`
  prefix with status at field **4** of a `' | '` (WITH the spaces) split; the
  status buckets SUM to the row count (122 + 1 = 123 ✓). The `' | '` field-count
  histogram is **{6: 121, 7: 1, 10: 1}**, so the forbidden `NF == 6` filter reads
  **121** — it drops the two rows carrying unescaped in-cell pipes. Those are
  append-only history and were NOT "fixed". This session does not touch
  `ROADMAP.md`, so leg (i) is unchanged at its end.
- **Leg (ii) — FALSE.** **14** crates
  (`envoy-{accesslog,admin,bin,cluster,config,filter,health,http1,http2,jwt,listener,stats,tcp,tls}`).
  `envoy-http3` / `envoy-grpc` / `envoy-wasm` / `envoy-protos` / `envoy-runtime` /
  `envoy-xds` all re-confirmed absent by `test -d`. Over the **28** manifests of
  `git ls-files '*Cargo.toml'`: `quinn` / `wasmtime` / `tonic` / `opentelemetry` /
  `prost` = **0** each. **Positive control, IDENTICAL invocation shape: `tokio` =
  19 of 28.** `histogram` = **0** over `crates/` against a `gauge` control of
  **365** occurrences by `grep -rIo 'gauge' crates/` (**352** scoped to
  `crates/*/src/`) — **the method and the scope are stated WITH the number**
  because `ADR-0200` recorded that same control as 462/449 and it does not
  reproduce.
- **Leg (iii) — FALSE.** **11** `### ` family headings reading
  **11 / 5 / 3 / 14 / 3 / 4 / 6 / 31 / 6 / 0 / 13** with **27** pre-heading rows,
  summing to 123 ✓. Censused from a single `/^### /` rule that SEEDS EVERY
  HEADING AT 0, because a naive per-row `awk` never emits the zero-row one. The
  one zero-row family is **`### WASM host family`**. The filing defect is intact
  and was NOT repaired: rows titled `Observability family:` sit physically under
  `### Deprecated / edge features`.

**No `stop` file exists and none was created** (`ADR-0167` DECISION 2).

---

## Task 1 — RIDER: re-attach `build_access_log_record`'s doc comment (CF-114-6)

**Commit:** `phase 115 task 1: RIDER — re-attach build_access_log_record's doc comment (CF-114-6)`

**What it is.** Not a deliverable. Phase 114 spliced `effective_grpc_status`
BETWEEN `build_access_log_record`'s three-line doc comment and its `fn` line
(`crates/envoy-http1/src/hcm.rs`), so the gRPC-status helper opened with a
paragraph describing a different function and the H1 record-build join point had
no doc at all. `ADR-0200` (l) names it a legitimate rider because this phase
touches that file; `ADR-0192` DECISION 5 says riders are not costed as phases.
**Zero behaviour change, net 0 lines.**

**Step 1 — both anchors asserted unique** (the phase-113 `fn check_alpn`
precedent: a bare name can match a test function whose name merely begins with
it):

```
$ grep -cF "/// Build the per-request access-log record (extracted verbatim from" crates/envoy-http1/src/hcm.rs
1
$ grep -cF "fn build_access_log_record(" crates/envoy-http1/src/hcm.rs
1
```

**Step 2 — the move.** The three `///` lines were deleted from directly above
`/// Phase 114: the UNGATED effective gRPC status.` (file line 1656) and inserted
directly above `fn build_access_log_record(`, after the blank line that follows
`effective_grpc_status`'s closing `}`.

**Step 3 — proved a pure move.** `PLAN.md` predicts numstat `3 3`, and it is not
taken on faith that a numstat is a property of the edit (the phase-114 state-3
trap: a pure block move rendered `10 14`). Here the prediction held exactly:

```
$ git diff --numstat crates/envoy-http1/src/hcm.rs
3	3	crates/envoy-http1/src/hcm.rs
```

and the diff is literally the same three lines out and the same three lines in —
one hunk at `@@ -1653,9 +1653,6 @@` removing them and one at `@@ -1673,6 +1670,9 @@`
adding them, with no other line touched.

**Step 4 — gate.** All three legs green on the landed tree:

```
$ cargo build --workspace --all-targets      → Finished `dev` profile … in 26.19s   (exit 0)
$ cargo fmt --all -- --check                 → FMT_EXIT=0   (no output)
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
                                             → Finished … in 9.68s   CLIPPY_EXIT=0
```

⚠ The clippy leg was gated on the **`Checking` COUNT, not the exit code** — a
cached clippy exits 0 having done nothing. This run did real work: **22
`Checking` lines** (plus 3 `Compiling` for the `rustls 0.23.45` /
`aws-lc-sys 0.45.0` bump that `0cb1f67` landed) and **zero diagnostics**. The
count differs from the `PLAN.md` per-task table's `160 Checking` because that
figure was measured on a cold scratch worktree with its own `CARGO_TARGET_DIR`;
the invariant is non-zero, not the absolute.

**Deviation from `PLAN.md`, stated rather than absorbed:** Task 1's `git add`
list names only `crates/envoy-http1/src/hcm.rs`. This file — `PROGRESS.md` — is
staged with it, because §5 state 3 requires appending to `PROGRESS.md` on each
task completion and the phase-114 rider commit `2cf0830` set exactly that
precedent (`10 10` on the code file + the new `PROGRESS.md`). `PROGRESS.md` is
`docs/` and is excluded from the §6.1 LoC accounting.
