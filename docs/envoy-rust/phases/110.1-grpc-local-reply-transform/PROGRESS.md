# Sub-phase 110.1 — §5 state-3 implementation PROGRESS

> Written for a reader with ZERO prior context (D-3.4). Every figure below was
> MEASURED in this session unless explicitly labelled as inherited. Command
> outputs are QUOTED, not summarized. Line numbers drift — everything here is
> locatable BY TEXT.

**Session scope:** execute `docs/envoy-rust/phases/110.1-grpc-local-reply-transform/PLAN.md`'s
8 TDD tasks, in order, TDD on every one (D-3.1). NO `REVIEW.md`, NO fixture,
NO chaining into state 4.

**Base commit:** `eeb45d0e87390eeb3814c3220193b4db94311abb` (the `110.1` state-2
PLAN-write's CI-confirmation record commit).

---

## What landed, per task

| task | commit | deliverable |
|---|---|---|
| 1 | `37b4106` | `crates/envoy-http1/src/grpc.rs` CREATED + `is_grpc_request`; `lib.rs` `pub(crate) mod grpc;`; `headers.rs` 3 constants |
| 2 | `4f66939` | `http_to_grpc_status` — the sparse-8 table over a default of 2 |
| 3 | `248f9b9` | `grpc_message_encode` — the CORRECTED `0x20..=0x7D` rule |
| 4 | `507c67b` | `apply_grpc_local_reply` — the transform, **plus an idempotence guard the PLAN's code lacked** |
| 5 | `15f36b7` | the tokio seam in `serve_connection` — `outgoing_local` + the single transform call |
| 6 | `0609a85` | family-wide seam coverage + the access-log and stats placement witnesses |
| 7 | `b5c3969` | the io_uring seam — transform INSIDE `write_owned` |
| 8 | (this commit) | the W-4 HTTP/2 negative witness + this file |

### Test-count ledger — the delta is the evidence

Every count below is from a `test result:` line, never from an exit code.

```
baseline  `cargo test -p envoy-http1 --lib`  : 201 passed; 0 failed
after T1  `--lib grpc::`                     :   5 passed; 0 failed  (201 filtered out)
after T2  `--lib grpc::`                     :   7 passed; 0 failed
after T3  `--lib grpc::`                     :  11 passed; 0 failed
after T4  `--lib grpc::`                     :  18 passed; 0 failed
after T4  `--lib` (whole crate)              : 219 passed; 0 failed   = 201 + 18
after T5  `--lib` (whole crate)              : 224 passed; 0 failed   = 219 +  5
after T6  `--lib` (whole crate)              : 233 passed; 0 failed   = 224 +  9
T8        `cargo test -p envoy-http2 --lib h2_route_decision_reply_is_not_grpc_transformed`
                                             :   1 passed; 0 failed
```

**33 new tests total** — **32 in `envoy-http1`** (18 unit in `grpc.rs` + 14 in
`hcm.rs`, matching the measured 201 -> 233) plus **1 in `envoy-http2`**. The 14
in `hcm.rs` are 13 `grpc_*` tests plus the
`non_grpc_request_leaves_direct_response_untouched` control; a 14th `grpc_`-
prefixed name, `grpc_no_healthy_upstream_config`, is a config BUILDER, not a
test. **Zero pre-existing tests moved** at any task boundary.

**The workspace identity moved by EXACTLY 33.** The CI-confirmed baseline at
`eeb45d0` was `passed + failed = 2194`; the post-change sweep measures
`2219 + 8 = 2227`, and `2227 - 2194 = 33`. That is the whole point of the
identity: it must move by exactly the number of tests added, and it does.

---

## Mutation evidence — every task, with an unmutated control from the same tree

All mutation runs used a **seeded scratch worktree** (`git worktree add
--detach`), because the rows under test were not yet committed at the time each
check ran. Every run forced a rebuild with an mtime-only `touch
crates/envoy-http1/src/lib.rs` and asserted `Compiling envoy-http1` ≥ 1 — a
cached no-op would be a FALSE PASS. Every verdict is gated on the `test result:`
line EXISTING, never on the exit code (a compile error is not a mutation RED).

### Task 1 — `is_grpc_request`

| mutation | result |
|---|---|
| A: `value.eq_ignore_ascii_case(GRPC_EXACT)` (case-insensitive) | **RED** — `4 passed; 1 failed`, `content-type "APPLICATION/GRPC" must detect as false` |
| B: `value.starts_with(GRPC_EXACT)` (naive prefix) | **RED** — `3 passed; 2 failed` |
| unmutated control | **GREEN** — `5 passed; 0 failed` |

Mutation B failed first on `application/grpc; charset=utf-8` rather than on the
`grpcfoo`/`grpc-web` cells the PLAN predicted — the assertion loop aborts at the
first failing cell and `; charset=utf-8` sits earlier in the table. Same test,
same mutation killed; only the PLAN's guess at WHICH cell reports first was off.

### Task 2 — `http_to_grpc_status`

| mutation | result |
|---|---|
| `429 => 14, 500..=599 => 14` (the plausible "5xx is UNAVAILABLE") | **RED** — `5 passed; 2 failed`; `HTTP 500 must map to grpc-status 2` AND `status 500 must map to the default 2 (UNKNOWN)` from the full-`u16`-range sweep |
| unmutated control | **GREEN** — `7 passed; 0 failed` |

### Task 3 — `grpc_message_encode`

**⚠ A PLAN DEFECT FOUND HERE, AND IT WOULD HAVE FAKED A "VACUOUS TESTS" VERDICT.**
The PLAN's Step-5 `sed` scripts for mutations A and C target the string

```
if (0x20..=0x7D).contains(&byte) && byte != b'%' {
```

which occurs **TWICE** in the file — once in the implementation and once inside
`encoder_rule_holds_for_every_byte_value`, which recomputes the same predicate
to build its expectation. `sed -i 's/X/Y/'` substitutes on EVERY matching line,
so both would have been mutated in lockstep: the test's expectation would have
moved with the implementation and the run would have come back **GREEN**,
reading as "these tests are vacuous" when in fact nothing had been mutated. A
`python` guard asserting `count == 1` refused the edit and surfaced it.

Re-run with an implementation-only two-line anchor (the impl line plus its
`out.push(byte as char);` successor, which the test does not have):

| mutation | result |
|---|---|
| A: upper bound `0x7D` → `0x7E` (the parent SPEC's WRONG rule) | **RED** — `9 passed; 2 failed`; `byte 0x7E encoded wrongly` + the `t~t` measured-body cell |
| B: `b"0123456789abcdef"` (lowercase hex) | **RED** — `8 passed; 3 failed` |
| C: drop the `&& byte != b'%'` carve-out | **RED** — `9 passed; 2 failed`; `byte 0x25 encoded wrongly` + the `%2525` cell |
| unmutated control | **GREEN** — `11 passed; 0 failed` |

### Task 4 — `apply_grpc_local_reply`

| mutation | result |
|---|---|
| A: always emit `grpc-message`, even for an empty body | **RED** — `16 passed; 2 failed` (`empty_body_omits_grpc_message_entirely`, `redirect_keeps_location_and_still_transforms`) |
| B: drop pass-through headers instead of preserving them | **RED** — `16 passed; 2 failed` (`arbitrary_pass_through_headers_survive_in_original_position`, `redirect_keeps_location_and_still_transforms`) |
| C: forget to drop the body | **RED** — `17 passed; 1 failed` (`bodied_local_reply_takes_the_measured_wire_shape`) |
| unmutated control | **GREEN** — `18 passed; 0 failed` |

Mutation C killed ONE test, not the two the PLAN predicted: the PLAN expected
`transform_is_idempotent` to fail too, via double-encoding. With the idempotence
guard (below) a second application returns early, so a kept body no longer
double-encodes. That is a correct consequence of the guard, not a weakened test.

### Task 5 — the tokio seam

| mutation | result |
|---|---|
| polarity flip `if outgoing_local` → `if !outgoing_local` | **RED** — `4 passed; 4 failed`, all four transform tests |
| unmutated control | **GREEN** — `224 passed; 0 failed` |

### Task 6 — the placement mutation (deferred from Task 5, and the sharpest evidence in the plan)

Moving the whole `if outgoing_local { … }` block from immediately BEFORE
`let response_status_for_log` to immediately BEFORE `if outgoing_direct {` —
i.e. installing the transform at the wire write instead:

```
test result: FAILED. 15 passed; 2 failed; 0 ignored; 0 measured; 216 filtered out
---- hcm::tests::grpc_transform_ticks_the_2xx_response_class stdout ----
---- hcm::tests::grpc_transform_is_visible_to_the_access_log stdout ----
```

**EXACTLY the two measurement-N-2 witnesses go RED, and the other 15 stay
GREEN.** That is the whole point: the wire-shape tests are structurally blind to
placement, so without those two witnesses a wire-write placement would be
byte-correct on the wire and silently wrong in BOTH the access log and the
per-class stats. This is the positive proof that placement is enforced rather
than merely documented.

Polarity flip re-run over the FULL family (to prove the nine new family tests
are not vacuous either): **`4 passed; 13 failed`** — including
`grpc_does_not_transform_a_proxied_upstream_response`, which goes RED because
inverted polarity transforms the PROXIED reply. That is the CF-110-2 guard
firing. Unmutated control: **`233 passed; 0 failed`**.

### Task 8 — the H2 negative witness

`h2_route_decision_reply_is_not_grpc_transformed` is **GREEN ON ARRIVAL**, and
that is CORRECT — it is a characterization pin, not a feature test. Its RED
comes from the mutation. Installing the transform on the SHARED path:

```rust
let mut out = build_response_in(&config.current_route_config(), req, close, &config.runtime);
if let BuildOutcome::Synth(ref mut r, _) = out {
    crate::grpc::apply_grpc_local_reply(r, &req.headers);
}
out
```

```
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 113 filtered out
---- hcm::tests::h2_route_decision_reply_is_not_grpc_transformed stdout ----
assertion `left == right` failed: H2 must keep the CONFIGURED status — 110.1's
transform must not reach the shared path
```

with `Compiling envoy-http2 = 1`. Unmutated control from the same tree:
**`1 passed; 0 failed`**. Global Constraint 1 is now ENFORCED, not merely
documented.

---

## Deviations from the PLAN — each with its reason

### D-1 (SUBSTANTIVE): `apply_grpc_local_reply` needed an idempotence guard the PLAN's code does not have

The PLAN specifies both `transform_is_idempotent` AND an implementation that
**fails it**. Verified by running the PLAN's literal code:

```
assertion `left == right` failed: applying the transform twice must change nothing
  left:  headers: [("grpc-status","14"), ("grpc-message","100%25 done"),
                   ("content-type","application/grpc"), ("grpc-status","2"),
                   ("content-length","0")]
  right: headers: [("content-type","application/grpc"), ("grpc-status","14"),
                   ("grpc-message","100%25 done"), ("content-length","0")]
```

**Root cause:** the transform EMITS four headers it owns (`content-type`,
`grpc-status`, `grpc-message`, `content-length`) but DROPS only two of them on
input. On a second application `grpc-status`/`grpc-message` fall into the
pass-through bucket, are preserved at the front, and a fresh `grpc-status` is
appended — carrying `2`, because `http_to_grpc_status` now reads the
already-rewritten `200`.

**Why simply dropping the other two is NOT sufficient:** the mapped code would
still be recomputed from the rewritten `200` and come out `2` instead of `14`.
The fix has to be an early return.

**Fix:** an explicit guard — `grpc-status` is emitted unconditionally by the
transform, so its presence is an EXACT sentinel for "already transformed".
Verified on disk that the sentinel is sound: `grep -c 'grpc'` over `hcm.rs`'s
entire non-test region (`awk 'NR<2532'`) returns **0**, so no local reply
carries `grpc-status` before the transform.

Today nothing can reach the function twice — the tokio and io_uring workers are
mutually exclusive. The guard makes idempotence a property of the FUNCTION
rather than of the current call graph, which is exactly what the PLAN's own
rationale for the test asks for.

### D-2 (CITATION): the H2 `build_response` call span is `:518-522`, not `:513-518`

The PLAN cites `crates/envoy-http2/src/hcm.rs:513-518` in several doc comments.
On disk, `:512-514` is the explanatory comment and the call itself spans
`:518-522`. Transcribed as `:518-522` so the citation is accurate.

### D-3 (BUILDER NAMES): Task 6's builder names are the PLAN's own inventions; reused what exists

The PLAN says so explicitly and instructs a grep first. Result:

| PLAN's invented name | what was actually used |
|---|---|
| `hcm_config_redirect_route` | **REUSED** `redirect_placeholder_config()` (returns `HCMConfig`, wrapped in `Arc::new`) |
| `hcm_config_route_to_empty_cluster` | **REUSED** `hcm_config_with_cluster` + `cluster_mgr_no_fallback_subset()` + a `metadata_match` naming a non-existent subset, wrapped in a new one-purpose helper `grpc_no_healthy_upstream_config` |
| `hcm_config_with_stop_and_send_filter` | **REUSED** `hcm_config_with_pipeline` + `HttpFilterInstance::test_stop_and_send_on_decode` / `..._on_encode` |
| `hcm_config_route_to_live_backend` | **REUSED** `spawn_in_process_upstream` + `cluster_mgr_with_endpoint` + `hcm_config_with_cluster` |
| `hcm_config_single_route_with_access_log` | **WROTE INLINE** using the tree's established `FileSink` + `CompiledJsonFormat` pattern (the PLAN's `log.records()` API does not exist) |
| `h2_direct_response_h1_config` | **WROTE NEW** — only `h2_redirect_h1_config` existed; modelled on it exactly |

The PLAN's access-log test asserts via a non-existent `log.records()` API; it
was rewritten to assert the log LINE byte-exactly, which is strictly stronger:

```
{"bytes":0,"rc":200,"rcd":"direct_response"}
```

This pins all three of measurement N-2's claims at once — `%RESPONSE_CODE%` =
200, `%BYTES_SENT%` = 0, and `%RESPONSE_CODE_DETAILS%` UNCHANGED at
`direct_response`.

### D-4 (EXTRA COVERAGE): the encode-side filter arm got its own test

The PLAN's family table lists "filter `StopAndSend` (decode + encode)" as one
row but supplies only a decode-side test. Both arms are covered:
`grpc_transforms_filter_stop_and_send_on_decode` and
`..._on_encode`. The encode arm is the one where the `outgoing_local = true`
re-assert is load-bearing — it can replace a PROXIED response, which would have
cleared the bit.

### D-5 (GATE, NOT PLAN SCOPE): `Cargo.lock` moved by one patch bump — `h2 0.4.13` → `0.4.16`

The PLAN's File Structure says no `Cargo.lock` change. `cargo deny check`
nevertheless FAILED on **RUSTSEC-2026-0258** (`h2` unbounded empty DATA frames,
low severity, patched in 0.4.16).

**Proven PRE-EXISTING and not caused by this sub-phase**, by direct experiment
rather than inference: a scratch worktree checked out at the session base
`eeb45d0` reproduces `advisories FAILED` with the same advisory ID. My commits
leave `Cargo.lock` byte-untouched (`git diff --numstat eeb45d0 HEAD -- Cargo.lock`
is EMPTY).

The requirement is `h2 = "0.4"` in all four consuming `Cargo.toml`s, so
`cargo update -p h2` satisfies it with **NO `Cargo.toml` change** — a pure lock
patch bump, which is the standing remedy for this class. Exit gate (e) requires
`cargo deny check` clean and D-3.6 forbids landing on a red gate, so the bump is
required to reach state 4.

---

## What is NOT covered — stated plainly rather than implied

### CF-110-5 (NEW) — the io_uring local-reply seam is unwitnessed by any test

`crates/envoy-http1/src/uring.rs` has **NO test harness at all**: measured on
disk, `grep -c '#\[cfg(test)\]\|#\[test\]\|#\[tokio::test\]'` over the file
returns **0**. The module is feature-gated
(`#[cfg(all(feature = "uring", target_os = "linux"))]` at `lib.rs`), so a plain
`cargo test --workspace` never even COMPILES it.

The seam's evidence is therefore **structural, not behavioural**:

```
write_owned CALL sites                        : 4   (all now pass &req.headers)
apply_grpc_local_reply occurrences            : 1   (inside the funnel)
write_head_body call sites outside the funnel : 1   (the PROXIED path, untransformed — correct)
cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings : exit 0,
                                                with `Checking envoy-http1` = 1
```

The transform lives INSIDE `write_owned` rather than at its four call sites
precisely so the coverage is structural — a fifth local-reply site added later
cannot forget it. Both ordering invariants were verified on disk:
`cluster.record_response` stays BEFORE `write_owned` (lines 337, 388) so outlier
detection records the ORIGINAL 503; `tick_class` stays AFTER (lines 293, 314,
339, 390) so the per-class counter sees the TRANSFORMED 200, matching
measurement N-2 and the tokio path.

**Bank CF-110-5.** This is a real coverage gap, and saying so is required rather
than implying test coverage that does not exist.

### CF-110-4 (NEW) — envoy-rust's non-gRPC `synth_with` header order differs from upstream's

From the PLAN's own measurements: upstream's NON-gRPC local-reply order is
`[pass-through,] content-length, content-type, date, server, connection`,
whereas `synth_with` emits `server, date, content-length, content-type,
connection`. ORDER-only, pre-existing, gRPC-ORTHOGONAL, and invisible to the
differential harness's `diff_headers` (which compares a `BTreeSet` of lower-cased
NAMES plus VALUES outside the 3-entry `HEADER_ALLOW_LIST`, and never reads
order). **Not fixed here, and not a licence to touch `synth_with`.**

### CF-110-3 (unchanged) — upstream emits `location` on a `201`/`3xx` `direct_response`

envoy-rust does not. Pre-existing and orthogonal to gRPC. Its only bearing on
this family is that sibling `110.2`'s fixture MUST NOT use a `201` or `3xx`
`direct_response` cell, or it will RED for a reason unrelated to gRPC.

### Out of scope by design

- **HTTP/2 gRPC local replies — CF-110-1.** Shape measured, not built. Task 8
  witnesses that H2 is UNTRANSFORMED; that is a pin on today's behaviour, NOT
  upstream parity.
- **Proxied/upstream-originated responses — CF-110-2.** Guarded by
  `outgoing_local` and witnessed negatively by
  `grpc_does_not_transform_a_proxied_upstream_response`.
- **No trailer API of any kind.** Every header in this surface rides on a
  `content-length: 0` reply, so no trailer section exists anywhere.

### Structural non-goals — ASSERTED, not assumed

```
fixture directories                     : 88     (unchanged; 0089 belongs to 110.2)
differential test files                 : 88     (unchanged)
ConfigError variants                    : 134    (unchanged — no config surface added)
fuzz targets                            : 5      (unchanged — §7.4's trigger does not fire)
known-failures.txt                      : 21 lines (untouched)
HEADER_ALLOW_LIST                       : 3 entries (server, date,
                                          x-envoy-upstream-service-time — `location` NOT added)
crates                                  : 14     (unchanged)
phase directories                       : 120    (unchanged)
git diff --stat eeb45d0..HEAD -- tests/ BEHAVIOR_CONTRACT.md : EMPTY
git status --porcelain -- Cargo.toml crates/*/Cargo.toml     : EMPTY
```

No fixture, no `BEHAVIOR_CONTRACT.md` edit, no config surface, no fuzz target,
no new dependency, no `ci.yml`/`deny.toml` change. **No banked finding was
fixed** (§6.3; ADR-0165). **No ADR was fired** — ADR head stays `ADR-0179`,
`grep -c '^## ADR-0180'` = **0**. **`ROADMAP.md` was NOT touched** — a state-3
does not touch it; row `110.1` stays `planned` until its own state-6.

---

## Size — measured, against the PLAN's projection

`git diff --numstat eeb45d0 HEAD -- . ':(exclude)docs/'`, read as
`added − deleted` (the metric the four landed calibration phases were measured
under). Cited as a RANGE so the figure survives the state-advance commit landing
on top of it:

```
709   0   crates/envoy-http1/src/grpc.rs
428   0   crates/envoy-http1/src/hcm.rs
  6   0   crates/envoy-http1/src/headers.rs
  4   0   crates/envoy-http1/src/lib.rs
 27   9   crates/envoy-http1/src/uring.rs
---------------------------------------------
added=1174  deleted=9  net=1165
```

(plus the Task-8 `envoy-http2/src/hcm.rs` test and the `Cargo.lock` patch bump,
which land in this commit.)

The PLAN's central estimate was **≈912** with an honest planning range of
**820–1330**. The measured **≈1165** sits inside that range, ~28% above centre
and well under the worst-case ≈1332. `grpc.rs` splits **217 non-test / 492
test**; the test half dominating is exactly what the calibration predicted.

---

## Exit-gate results — the §5 state-4 gate, run at state-3

```
cargo build  --workspace --all-targets                                  -> exit 0
cargo clippy --workspace --all-targets --all-features -- -D warnings    -> exit 0   (4 `Checking` lines: a REAL run, not a cached no-op)
cargo fmt    --all -- --check                                           -> exit 0
cargo deny check                                                        -> exit 0   advisories ok, bans ok, licenses ok, sources ok
cargo test   --workspace --no-fail-fast                                 -> see census
```

`--all-features` is the ONLY gate that compiles the feature-gated io_uring seam
(Global Constraint 8), and it was run at both crate and workspace scope.

`cargo deny` is gated on the FOUR-OK LINE, not on the absence of output: the
five `license-not-encountered` warnings are NORMAL on a green run.

### Workspace gate census — TWO full sweeps, diffed

The `ok`-only census form makes `failed=0` tautological, so the
`(ok|FAILED)` form with awk fields 4/6 was used, the binary count asserted
separately, and the result cross-checked against `grep -c 'test result: FAILED'`.

| sweep | binaries | passed | failed | `passed + failed` |
|---|---|---|---|---|
| 1 (pre-`h2`-bump) | 165 | 2219 | 8 | **2227** |
| 2 (post-`h2`-bump) | 165 | 2214 | 13 | **2227** |

**`passed + failed` is 2227 in BOTH sweeps**, and `2227 − 2194 = 33` — exactly
the tests this session added. The identity moved by precisely the right amount.
The cross-check holds: local `passed + failed` should equal CI's `passed` with
`failed=0`, so **CI is expected to report `binaries=165 passed=2227 failed=0`**.

### Every RED classified BY ISOLATION — never by name, never by text

**The red SET moved between the two sweeps (8 vs 13) while the identity held.**
That is the documented behaviour, not a signal: membership and size of the
local flake tail both move run to run. Only the INTERSECTION is stable.

**The ADR-0164 stable core of FIVE — present in BOTH sweeps, and each FAILS
DETERMINISTICALLY IN ISOLATION, which IS its signature (LOCAL-only; CI passes
them):**

```
access_log_rcd_upstream_reset        alone -> FAILED. 0 passed; 1 failed
access_log_rf_upstream_reset         alone -> FAILED. 0 passed; 1 failed
access_log_h2_rcd_upstream_reset     alone -> FAILED. 0 passed; 1 failed
access_log_h2_uc_upstream_reset      alone -> FAILED. 0 passed; 1 failed
admin_config_dump_server_info        alone -> FAILED. 0 passed; 1 failed
```

**The container-readiness / parallel-load tail — every one PASSES ALONE:**

```
sweep 1:  access_log_and_filter                                   alone -> ok. 1 passed
          tcp_proxy_fixture                                       alone -> ok. 1 passed
          send_request_maps_h2_handshake_failure_to_typed_error    alone -> ok. 1 passed
sweep 2:  access_log_omit_empty                                   alone -> ok. 1 passed
          access_log_rcd_host_not_found                           alone -> ok. 1 passed
          access_log_rcd_route_not_found                          alone -> ok. 1 passed
          access_log_response_code_details                        alone -> ok. 1 passed
          access_log_rf_connect_failure                           alone -> ok. 1 passed
          access_log_rf_no_route                                  alone -> ok. 1 passed
          access_log_rf_overflow_request_budget                   alone -> ok. 1 passed
          tls_sni_fixture                                         alone -> ok. 1 passed
```

**ZERO real regressions**, in either sweep.

And regression-equivalence here is STRUCTURAL, not merely observed. W-6 was
re-derived on disk at this session:

```
grep -rn 'application/grpc\|grpc-status\|grpc-message\|te: trailers' tests/   ->  0
```

**Zero hits across the ENTIRE test tree.** No existing fixture or test sends a
downstream gRPC `content-type` or asserts on `grpc-status`/`grpc-message`, so
nothing in the differential corpus CAN red from this surface. The only `grpc`
presence anywhere under `tests/` is fixture `0075-upstream-grpc-health-check`,
which is the proxy acting as a gRPC CLIENT toward a backend — orthogonal.

### Gate items (b), (c), (d) — the parts a local run cannot settle

- **(b)** All 88 pre-existing differential fixtures: blast radius MEASURED ZERO
  (above). **CI is authoritative** for the backend-routing fixtures, which red
  on this host's `192.168.65.2` bridge.
- **(c)** Conformance untouched: `known-failures.txt` still **21** lines.
  **⚠ The h2spec gate SELF-SKIPS SILENTLY on a developer host, so a local green
  proves NOTHING (ADR-0163).** It must be confirmed on the CI log with
  `grep -c 'h2spec not found'` = 0 AND `test h2spec_pass_rate_gate ... ok`
  present. That check belongs to the CI confirmation, not to this local run.
- **(d)** No new fuzz target — still **5**, across five crates. §7.4's trigger
  does not fire: no parser, no codec, no filter, no config surface.

## Next state

This sub-phase now sits at §5 **state 3 complete** (`SPEC.md` + `PLAN.md` +
`PROGRESS.md`, and NO `REVIEW.md`). The next session runs §5 **state 4**
(`superpowers:verification-before-completion`) — a SEPARATE session per §5.1 and
ADR-0127. Sibling `110.2` (fixture `0089` + the `BEHAVIOR_CONTRACT.md` `## gRPC`
section + the parent-110 close) follows only after `110.1` is `done`.
