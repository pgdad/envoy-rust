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

---
---

# Sub-phase 110.1 — §5 state-4 VERIFICATION

> Appended by a SEPARATE session from the one that wrote everything above
> (§5.1; ADR-0127 — the context that wrote the code must not grade it). This
> section **GRADES** the landed implementation; it changes NO code, fixes NO
> banked finding, writes NO `REVIEW.md`, creates NO fixture and does NOT touch
> `ROADMAP.md`. Every command output below is **QUOTED, not summarized**, and
> every inherited number was **RE-DERIVED ON DISK** — several did not survive,
> and those are called out by name.
>
> Base commit graded: `8d22234e891d9a7c8437d5af7e70afaadffdd783`
> (the state-3 CI-confirmation record commit, sitting on top of the
> implementation head `29d25e548c8ac3938de8c02b3e1c347d868b4233`).

## Step 0 — state confirmed from disk, before anything else

```
$ git status --porcelain | wc -l
0
$ git rev-parse --abbrev-ref HEAD
main
$ git rev-parse HEAD
8d22234e891d9a7c8437d5af7e70afaadffdd783
$ git fetch origin --prune ; echo "FETCH_EXIT=$?"
FETCH_EXIT=0
$ git log --oneline -1 origin/main
8d22234 phase 110.1 state-3: record CI confirmation on 29d25e5 — binaries=165 passed=2227 failed=0, the identity moved by exactly the 33 tests added
$ ls stop
ls: cannot access 'stop': No such file or directory
```

**§5 state-4 detection rule re-verified on disk, not taken from the handoff:**

```
$ ls docs/envoy-rust/phases/110.1-grpc-local-reply-transform/
PLAN.md  PROGRESS.md  SPEC.md
```

`SPEC.md` + `PLAN.md` + `PROGRESS.md` and **NO `REVIEW.md`** → §5 **state 4**.
`STATE.md` `## Active phase` `**id:**` reads `110` SPLIT with the active pointer
on `110.1`; `ROADMAP.md` reads row `110` `in-progress`, rows `110.1`/`110.2`
`planned`. **`STATE.md` and `ROADMAP.md` AGREE** — no ambiguity, so no
`superpowers:systematic-debugging` detour.

---

## Gate (e) — the five workspace commands, each quoted

Run from a clean tree at `8d22234`, serialized against each other (the cargo
lock makes them mutually exclusive anyway).

### `cargo build --workspace --all-targets`

```
$ cargo build --workspace --all-targets ; echo "EXIT=$?"
   Compiling http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.59s
EXIT=0
$ grep -c '^ *Compiling' build.log
1
```

**The `Compiling` count is asserted, not assumed** — a bare `cargo build` on a
warm `target/` returns exit 0 with ZERO `Compiling` lines and that is a cached
no-op, not a gate. This run compiled one target.

### `cargo clippy --workspace --all-targets --all-features -- -D warnings`

A cached clippy is the same trap in a different suit (clippy prints `Checking`,
not `Compiling`), so the dirty set was FORCED with an **mtime-only `touch` of a
crate root**, which leaves the working tree byte-clean:

```
$ md5sum crates/envoy-http1/src/lib.rs > lib_md5_before.txt
$ touch crates/envoy-http1/src/lib.rs
$ git status --porcelain | wc -l
0
$ cargo clippy --workspace --all-targets --all-features -- -D warnings ; echo "EXIT=$?"
EXIT=0
$ grep -c '^ *Checking' clippy.log
14
$ grep '^ *Checking envoy-http1' clippy.log
    Checking envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
$ grep -cE '^(warning|error)' clippy.log
0
$ tail -3 clippy.log
    Checking envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Checking differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.73s
$ md5sum -c lib_md5_before.txt
crates/envoy-http1/src/lib.rs: OK
```

**14 `Checking` lines including `envoy-http1`, 0 warnings, exit 0, and the file
byte-identical afterwards.** A REAL run.

### `cargo fmt --all -- --check`

```
$ cargo fmt --all -- --check ; echo "EXIT=$?"
EXIT=0
$ wc -l < fmt.log
0
```

Exit 0 **and** zero bytes of diff output — both asserted, because `--check`
signals only through its exit code and a swallowed diff would read the same.

### `cargo test --workspace` — see the census section below

### `cargo deny check`

```
$ grep -A1 '^name = "h2"' Cargo.lock
name = "h2"
version = "0.4.16"
$ cargo deny check ; echo "EXIT=$?"
EXIT=0
$ grep -E 'advisories ok|bans ok|licenses ok|sources ok' deny.log
advisories ok, bans ok, licenses ok, sources ok
$ grep -c 'license-not-encountered' deny.log
5
```

Gated on the **four-ok line**, not on the absence of output. The five
`license-not-encountered` warnings are NORMAL on a green run (the last is
quoted in full):

```
warning[license-not-encountered]: license was not encountered
   ┌─ /home/esa/git/envoy-rust/deny.toml:50:6
   │
50 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

The state-3 session's `h2 0.4.13 → 0.4.16` lock bump for RUSTSEC-2026-0258 is
**already applied and confirmed present at `Cargo.lock`**; it was NOT
re-litigated here, and `deny` is green with it in place.

---

## Gate (e) — `cargo test --workspace`: TWO sweeps, both censused

The `ok`-only census form makes `failed=0` TAUTOLOGICAL, so the `(ok|FAILED)`
form with **awk fields 4/6** was used (`$5`/`$7` return a believable
`passed=0`), the binary count asserted separately, and every result
cross-checked against `grep -c 'test result: FAILED'`. Neither run was piped
through `tail` — both were redirected to a file, because `tail` truncates the
`failures:` block.

### Sweep A — `cargo test --workspace --all-targets --no-fail-fast`

```
$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' sweep1.log \
   | awk '{b++; p+=$4; f+=$6} END {print "binaries="b" passed="p" failed="f" sum="p+f}'
binaries=149 passed=2220 failed=7 sum=2227
$ grep -c 'test result: FAILED' sweep1.log
7
$ wc -c sweep1.log
260523 sweep1.log
```

### Sweep B — `cargo test --workspace --no-fail-fast` (the §5 gate's exact form)

```
$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' sweep2.log \
   | awk '{b++; p+=$4; f+=$6} END {print "binaries="b" passed="p" failed="f" sum="p+f}'
binaries=165 passed=2200 failed=27 sum=2227
$ grep -c 'test result: FAILED' sweep2.log
27
$ wc -c sweep2.log
268827 sweep2.log
```

### The identity — CONFIRMED, and the binary count explained

**`passed + failed = 2227` in BOTH sweeps**, and `2227 − 2194 = 33` — exactly
the tests this sub-phase added. The state-3 session's two sweeps also measured
2227. **Four independent full sweeps, four times 2227.** The identity is the
load-bearing figure and it holds.

**⚠ The BINARY count is form-dependent, and this is worth stating because a
state-4 that expects a fixed 165 from any sweep will misread its own gate.**
`--all-targets` yields **149** binaries; the plain form yields **165**. The
16-binary gap is the doc-test harnesses, which `--all-targets` excludes — and
that was MEASURED rather than assumed:

```
$ grep -c '^   Doc-tests' sweep2.log      # plain form
16
$ grep -c '^   Doc-tests' sweep1.log      # --all-targets form
0
$ grep -A4 '^   Doc-tests' sweep2.log \
   | grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' \
   | awk '{b++;p+=$4;f+=$6} END{print "docbins="b" passed="p" failed="f}'
docbins=16 passed=0 failed=0
```

`149 + 16 = 165`, and the 16 doc-test harnesses contribute **zero** tests —
which is exactly why `passed + failed` is invariant at 2227 across both forms.
**CI runs the plain form, so `binaries=165` is the figure to expect there**, and
the state-3 record's `binaries=165` is consistent.


---

## Every RED classified BY ISOLATION — and the isolation harness itself had to be fixed first

**The red SET moved hard between the two sweeps — 7 vs 27 — while `passed +
failed` stayed pinned at 2227.** That is the documented local behaviour, not a
signal: only the INTERSECTION is stable.

```
$ comm -12 reds1.txt reds2.txt
access_log_h2_rcd_upstream_reset
access_log_h2_uc_upstream_reset
access_log_rcd_upstream_reset
access_log_rf_upstream_reset
admin_config_dump_server_info
$ cat reds1.txt reds2.txt | sort -u | wc -l
29
```

**The intersection is EXACTLY the ADR-0164 stable core of five.** The union of
29 distinct names was then classified one at a time. Red names were extracted
via the `---- <name> stdout ----` markers, never by indentation — the
`failures:` block cannot be censused by indentation.

### ⚠ FINDING V-1 (METHOD): a back-to-back isolation loop MANUFACTURES a false `FAILS-IN-ISOLATION` verdict

The first classification pass ran all 29 names in one loop with **no settle gap
between cargo invocations**. It returned **SEVEN** deterministic failures — the
expected five, plus `access_log_rf_no_healthy` and
`access_log_rf_overflow_request_budget`. Two extras is exactly the shape of a
real regression, and both names sit on local-reply paths this sub-phase
transforms (`synth_no_healthy_upstream`, `synth_overflow`), so it had to be run
to ground rather than waved off.

**It is an artifact of the harness, not a property of the tests.** Re-run ONE AT
A TIME with an 8-second settle gap, three rounds each, with an unchanged member
of the stable core as the control:

```
round1 access_log_rf_no_healthy                | passed=1 failed=0 |
round1 access_log_rf_overflow_request_budget   | passed=1 failed=0 |
round1 access_log_rcd_upstream_reset           | passed=0 failed=1 |
round2 access_log_rf_no_healthy                | passed=1 failed=0 |
round2 access_log_rf_overflow_request_budget   | passed=1 failed=0 |
round2 access_log_rcd_upstream_reset           | passed=0 failed=1 |
round3 access_log_rf_no_healthy                | passed=1 failed=0 |
round3 access_log_rf_overflow_request_budget   | passed=1 failed=0 |
round3 access_log_rcd_upstream_reset           | passed=0 failed=1 |
```

**Both extras PASS ALONE 3/3; the control FAILS ALONE 3/3.** The stable core is
FIVE, exactly as inherited.

**Root cause, established by direct probe rather than inference.** Both extras
failed with the container-readiness signature, and the container that never
became ready was still on the host in state `created`:

```
$ docker inspect 8149861d6d47 --format '{{.State.Status}} | {{.State.Error}}'
created | failed to set up container networking: driver failed programming
external connectivity on endpoint infallible_booth: failed to bind host port
for 0.0.0.0::172.17.0.4:10000/tcp: address already in use
```

**A host port collision stopped the upstream Envoy REFERENCE container from
starting.** envoy-rust's code is not on that path at all — the failure is that
the *upstream* side never came up. Three FOREIGN containers were holding host
ports 21005–21006 and 22010–22019 throughout:

```
$ docker ps --format '{{.ID}} {{.Names}} {{.Image}} {{.Status}}'
e444969ef262 a90-ref  envoyproxy/envoy:contrib-v1.37.2 Up 3 minutes
53eb1f8935a1 b90-ref2 envoyproxy/envoy:contrib-v1.37.2 Up 5 minutes
a2bdae1aa19e b90-ref  envoyproxy/envoy:contrib-v1.37.2 Up 11 minutes
```

They are **not this repo's** — proven, not assumed:

```
$ grep -rl 'contrib-v1.37' --include='*.rs' --include='*.toml' --include='*.yaml' --include='*.yml' . | wc -l
0
$ grep -rl 'a90-ref\|b90-ref' --include='*.rs' --include='*.sh' --include='*.yaml' . | wc -l
0
```

This repo pins `envoyproxy/envoy:v1.33.0`; those run `contrib-v1.37.2` under
names the tree never mentions. **They were left alone** (they are not this
session's to kill), and the classification was made robust to them instead.

**Standing lesson for the next state-4: ONLY ISOLATION CLASSIFIES, but an
isolation run is itself a probe that can fail to execute honestly. Put a settle
gap between Docker-spawning runs, repeat each verdict, and pair every claimed
deterministic failure with a same-shape control from the known core.**

### The ADR-0164 stable core of FIVE — deterministic in isolation, which IS its signature

Each re-run with settle gaps, two further rounds apiece on top of the pass
above:

```
round1 access_log_rf_upstream_reset      | passed=0 failed=1
round1 access_log_h2_rcd_upstream_reset  | passed=0 failed=1
round1 access_log_h2_uc_upstream_reset   | passed=0 failed=1
round1 admin_config_dump_server_info     | passed=0 failed=1
round2 access_log_rf_upstream_reset      | passed=0 failed=1
round2 access_log_h2_rcd_upstream_reset  | passed=0 failed=1
round2 access_log_h2_uc_upstream_reset   | passed=0 failed=1
round2 admin_config_dump_server_info     | passed=0 failed=1
```

Their failure text confirms both are HOST-environment, not behavioural. The
four `*_upstream_reset` fixtures never get an upstream reference container:

```
---- access_log_rcd_upstream_reset stdout ----
thread 'access_log_rcd_upstream_reset' panicked at
tests/differential/tests/access_log_rcd_upstream_reset.rs:33:10:
fixture green: upstream Envoy never became accept-ready
Caused by:
    127.0.0.1:55212 not accept-ready within 10s: Connection refused (os error 111)
```

and `admin_config_dump_server_info` is the documented `192.168.65.2`
host-bridge divergence — upstream sees a backend host this host's Docker bridge
addresses differently:

```
---- admin_config_dump_server_info stdout ----
Caused by:
    text_lines diverged after allow-lists:
      envoy-only:      ["backend::192.168.65.2:42919::canary::false", …]
      envoy-rust-only: []
```

**Both are LOCAL-only and CI passes them.**

### The other 24 — every one PASSES ALONE

```
access_log_response_flag_filter                     | passed=1  failed=0 | PASSES-ALONE
access_log_rf_no_healthy                            | passed=1  failed=0 | PASSES-ALONE (3/3)
access_log_rf_overflow_request_budget               | passed=1  failed=0 | PASSES-ALONE (3/3)
access_log_rf_retry_exhausted                       | passed=1  failed=0 | PASSES-ALONE
access_log_route_name                               | passed=1  failed=0 | PASSES-ALONE
access_log_upstream_cluster                         | passed=1  failed=0 | PASSES-ALONE
admin_drain_listeners                               | passed=2  failed=0 | PASSES-ALONE
admin_ready_fixture                                 | passed=1  failed=0 | PASSES-ALONE
admin_ready_returns_200_post_migration              | passed=1  failed=0 | PASSES-ALONE
admin_stats_prometheus                              | passed=1  failed=0 | PASSES-ALONE
headermatcher_absence_accesslog_present_polarity    | passed=1  failed=0 | PASSES-ALONE
header_to_metadata                                  | passed=20 failed=0 | PASSES-ALONE
http1_direct_response_fixture                       | passed=1  failed=0 | PASSES-ALONE
http2_direct_response_fixture                       | passed=1  failed=0 | PASSES-ALONE
http_filter_buffer_fixture                          | passed=1  failed=0 | PASSES-ALONE
http_filter_cors_fixture                            | passed=1  failed=0 | PASSES-ALONE
http_filter_fault_fixture                           | passed=1  failed=0 | PASSES-ALONE
http_filter_jwt_authn_fixture                       | passed=1  failed=0 | PASSES-ALONE
http_filter_rbac_fixture                            | passed=1  failed=0 | PASSES-ALONE
lb_ring_hash_fixture                                | passed=1  failed=0 | PASSES-ALONE
network_filter_rbac_deny_fixture                    | passed=1  failed=0 | PASSES-ALONE
rbac_matcher_value_enrichment                       | passed=1  failed=0 | PASSES-ALONE
upstream_outlier_detection_consecutive_5xx_fixture  | passed=1  failed=0 | PASSES-ALONE
xds_eds_hot_reload_fixture                          | passed=1  failed=0 | PASSES-ALONE
```

Every isolation verdict was gated on `passed + failed ≥ 1`, so a
`0 passed; N filtered out` false green could not be counted as a pass — the
classifier emits `NO-TEST-RAN(FALSE-GREEN-GUARD)` in that case, and it fired
zero times.

**Two of the 24 are directly load-bearing for this sub-phase and both pass
alone: `http1_direct_response_fixture` and `http2_direct_response_fixture`** —
the H1 path the transform now sits on, and the H2 path Task 8 pins as
UNTRANSFORMED.

### Verdict: ZERO real regressions

And regression-equivalence here is **STRUCTURAL**, re-derived on disk at this
session rather than inherited:

```
$ grep -rn 'application/grpc\|grpc-status\|grpc-message\|te: trailers' tests/ | wc -l
0
$ grep -rl 'grpc' tests/
tests/differential/tests/upstream_grpc_health_check.rs
tests/differential/src/lib.rs
tests/fixtures/0075-upstream-grpc-health-check/envoy.yaml
tests/fixtures/0075-upstream-grpc-health-check/expectations.yaml
tests/fixtures/0075-upstream-grpc-health-check/README.md
tests/fixtures/0075-upstream-grpc-health-check/envoy-rust.yaml
```

**Zero hits across the entire test tree.** No existing fixture or test sends a
downstream gRPC `content-type` or asserts on `grpc-status`/`grpc-message`, so
nothing in the differential corpus **CAN** red from this surface. The only
`grpc` presence is fixture `0075`, the proxy as a gRPC CLIENT toward a backend —
orthogonal. W-6 holds.


---

## NEW EVIDENCE for CF-110-5 — the io_uring seam's only gate, proven CAUSALLY

`PROGRESS.md` above banks CF-110-5 honestly: `uring.rs` has **no test harness at
all**, so `clippy --workspace --all-targets --all-features -- -D warnings` is
the *only* gate that even compiles the io_uring seam. That claim was itself
taken on trust from a structural count. **A state-4 can do better than trust it,
so this session ran the causal experiment.**

Re-derived on disk first:

```
$ grep -c '#\[cfg(test)\]\|#\[test\]\|#\[tokio::test\]' crates/envoy-http1/src/uring.rs
0
$ grep -n 'write_owned' crates/envoy-http1/src/uring.rs
292:                write_owned(&mut down, &mut resp, &req.headers, &mut write_buf).await?;
313:                    write_owned(&mut down, &mut resp, &req.headers, &mut write_buf).await?;
338:                        write_owned(&mut down, &mut resp, &req.headers, &mut write_buf).await?;
389:                        write_owned(&mut down, &mut resp, &req.headers, &mut write_buf).await?;
519:async fn write_owned(
$ grep -n 'apply_grpc_local_reply' crates/envoy-http1/src/uring.rs
525:    crate::grpc::apply_grpc_local_reply(resp, req_headers);
$ grep -n 'write_head_body' crates/envoy-http1/src/uring.rs
376:                        write_head_body(&mut down, &mut head_buf, &body).await?;
479:async fn write_head_body(
506:/// synthetic local reply; the proxied path uses `write_head_body` instead.
527:    write_head_body(down, buf, &resp.body).await
```

**4 `write_owned` call sites, all passing `&req.headers`; EXACTLY 1
`apply_grpc_local_reply`, and it is INSIDE the funnel at `:525`, above the
`write_head_body` at `:527`; exactly 1 untransformed `write_head_body` call
outside the funnel, at `:376` — the PROXIED path, which is correct (CF-110-2).**
The structural claim survives re-derivation.

### The causal experiment — a scratch worktree, a count guard, and a control

```
$ git worktree add --detach <scratch> HEAD          # WORKTREE_EXIT=0
$ grep -c 'crate::grpc::apply_grpc_local_reply(resp, req_headers);' \
    crates/envoy-http1/src/uring.rs
1                                                    # count guard: EXACTLY ONCE
```

**Unmutated control from the same tree**, dirty set forced by an mtime-only
`touch`:

```
$ cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings ; echo "EXIT=$?"
EXIT=0
$ grep -c '^ *Checking' uring_control.log
61
```

Two probe lines were then inserted immediately above the seam call (one
`_`-prefixed, one not, so `-D warnings` has something to bite on) and the SAME
command re-run:

```
$ cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings ; echo "EXIT=$?"
error: unused variable: `unused_probe_variable`
   --> crates/envoy-http1/src/uring.rs:526:9
error: could not compile `envoy-http1` (lib) due to 1 previous error
EXIT=101
```

and, from the same mutated tree, the SAME command **without** `--all-features`:

```
$ cargo clippy -p envoy-http1 --all-targets -- -D warnings ; echo "EXIT=$?"
EXIT=0
$ grep -c '^ *Checking' uring_mut_nofeat.log
20
$ grep -c 'uring.rs' uring_mut_nofeat.log
0
```

**This is the whole point of Global Constraint 8, now PROVEN rather than
documented.** With `--all-features` the gate reads `uring.rs` and REDs, citing
the file and line. Without it the gate is a genuine, 20-`Checking`-line real run
that never mentions `uring.rs` at all — the seam is completely invisible to it.
**A state-4 that drops `--all-features` would return a green that says nothing
whatsoever about the io_uring seam.**

The scratch worktree was removed and the main tree re-verified clean:

```
$ git worktree remove --force <scratch> && git status --porcelain | wc -l
0
```

**CF-110-5 stays OPEN and was NOT fixed here** (a state-4 grades; it does not
change code). What this section adds is that its compile-time backstop is real.

---

## Structural censuses — every one RE-DERIVED, and one inherited number DIED

```
$ git ls-files 'tests/fixtures/**' | cut -d/ -f3 | sort -u | wc -l
88
$ git ls-files 'tests/fixtures/*/' | wc -l          # the vacuous glob, for the record
0
$ git ls-files 'tests/differential/tests/*.rs' | wc -l
88
$ <ConfigError enum block extracted by brace matching, ^    [A-Z][A-Za-z0-9]*\s*(\{|\(|,)>
FILE: crates/envoy-config/src/lib.rs
VARIANTS: 134
$ grep -cP '^    [A-Z]' crates/envoy-config/src/lib.rs   # the naive form, for the record
162
$ git ls-files '**/fuzz_targets/*.rs'
crates/envoy-accesslog/fuzz/fuzz_targets/accesslog_format_parse.rs
crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs
crates/envoy-filter/fuzz/fuzz_targets/cdn_loop_parse.rs
crates/envoy-http2/fuzz/fuzz_targets/grpc_health_decode.rs
crates/envoy-jwt/fuzz/fuzz_targets/jwt_parse.rs
                                                     -> 5 targets across FIVE crates
$ wc -l tests/conformance/h2spec/known-failures.txt
21
$ grep -vc '^\s*#\|^\s*$' tests/conformance/h2spec/known-failures.txt
1                                                    # ONE real entry
$ sed -n '1189,1193p' tests/differential/src/lib.rs
pub const HEADER_ALLOW_LIST: &[(&str, AllowMode)] = &[
    ("server", AllowMode::NameRequired),
    ("date", AllowMode::NameRequired),
    ("x-envoy-upstream-service-time", AllowMode::NameRequired), // 04.3 NEW
];
                                                     -> 3 entries; `location` NOT present (grep -c = 0)
$ ls crates | wc -l
14
$ ls docs/envoy-rust/phases | wc -l
120
$ <ROADMAP split on ' | ', status = FIELD 4>
rows: 116   Counter({'done': 113, 'planned': 2, 'in-progress': 1})
not done: [('110','in-progress'), ('110.1','planned'), ('110.2','planned')]
$ grep -o '^## ADR-[0-9]*' docs/envoy-rust/DECISIONS.md | sort -u | tail -3
## ADR-0177
## ADR-0178
## ADR-0179
$ grep -c '^## ADR-0180' docs/envoy-rust/DECISIONS.md
0                                                    # ADR-0180 UNRESERVED
$ grep -c '^## ADR-' docs/envoy-rust/DECISIONS.md
176                                                  # reads ONE high (schema template)
```

**ALL UNCHANGED.** No fixture, no config surface, no fuzz target, no ADR, no
`ROADMAP.md` touch. `ROADMAP.md` was NOT touched by this state-4 either.

File sizes, re-derived:

```
$ wc -l crates/envoy-http1/src/hcm.rs crates/envoy-http1/src/uring.rs \
        crates/envoy-http1/src/grpc.rs crates/envoy-http2/src/hcm.rs \
        docs/envoy-rust/STATE.md docs/envoy-rust/STATE_HISTORY.md
   11633 crates/envoy-http1/src/hcm.rs
     538 crates/envoy-http1/src/uring.rs
     709 crates/envoy-http1/src/grpc.rs
    7394 crates/envoy-http2/src/hcm.rs
     214 docs/envoy-rust/STATE.md
   16200 docs/envoy-rust/STATE_HISTORY.md
$ <python len() over STATE.md line 28, the `**Standing traps**` line>
TRAPS_CHARS 154711    TRAPS_PREFIX_OK True
NEW_AT_anchored 38    NEW_AT_naive 39
```

The anchored `**[NEW AT ` marker census is **38** against a naive `[NEW AT `
count of **39** — the +1 gap is the quoted `76.2` marker inside another block's
prose, as inherited. Confirmed.

### ⚠ FINDING V-2 (CITATION): the `#[cfg(test)] mod tests` line in `envoy-http1/src/hcm.rs` is **2579**, not 2532

The handoff and `PROGRESS.md`'s deviation D-1 both cite `awk 'NR<2532'` as "the
whole non-test region" of `hcm.rs`, and D-1 rests a soundness argument on
`grep -c 'grpc'` over that region returning **0**. At `HEAD` that is FALSE in
both halves:

```
$ grep -n '^#\[cfg(test)\]' crates/envoy-http1/src/hcm.rs
2579:#[cfg(test)]
$ awk 'NR<2532' crates/envoy-http1/src/hcm.rs | grep -c 'grpc'
3
```

**Provenance established rather than guessed** — the number was correct when it
was measured, at the Task-4 commit, and went stale when Task 5 landed the seam:

```
$ git show 507c67b:crates/envoy-http1/src/hcm.rs | grep -n '^#\[cfg(test)\]'
2532:#[cfg(test)]
```

**The underlying soundness claim SURVIVES, re-derived at `HEAD` against the
correct boundary.** The three hits in the real non-test region are:

```
$ grep -n 'grpc' crates/envoy-http1/src/hcm.rs | awk -F: '$1<2579'
1440:                // deny with a gRPC content-type returns 200 + `grpc-status: 7`
1441:                // + `grpc-message: RBAC: access denied`.
1491:            crate::grpc::apply_grpc_local_reply(&mut outgoing, &req.headers);
```

— two COMMENT lines and **the seam call itself**. Nothing emits a `grpc-status`
header before the transform runs:

```
$ grep -rn 'GRPC_STATUS\|GRPC_MESSAGE\|GRPC_CONTENT_TYPE' crates/envoy-http1/src/ \
    | grep -v 'src/grpc.rs'
crates/envoy-http1/src/headers.rs:18:pub const GRPC_STATUS: &str = "grpc-status";
crates/envoy-http1/src/headers.rs:19:pub const GRPC_MESSAGE: &str = "grpc-message";
crates/envoy-http1/src/headers.rs:20:pub const GRPC_CONTENT_TYPE: &str = "application/grpc";
```

The constants are DEFINED in `headers.rs` and CONSUMED only in `grpc.rs`. **The
`grpc-status` sentinel that D-1's idempotence guard turns on is therefore still
exact at `HEAD`.** This is a stale citation, not a defect — but a state-5
reviewer copying `NR<2532` forward would re-derive `3` and read it as the guard
being unsound.


---

## The seam, re-derived on disk — W-2 / W-3 / W-4

**W-2 — `synth_with` has FOUR direct callers, and the transform is in NEITHER
shared path:**

```
$ grep -rn 'synth_with' crates/
crates/envoy-http1/src/grpc.rs:136:/// This is NOT called from `synth_with`, from any `synth_*` wrapper, or from
crates/envoy-http1/src/hcm.rs:1479:        // NOT installed in `synth_with` / any `synth_*` / `build_response`:
crates/envoy-http1/src/hcm.rs:2286:fn synth_with(status: u16, body: Bytes, close: bool) -> Response {
crates/envoy-http1/src/hcm.rs:2309:    synth_with(
crates/envoy-http1/src/hcm.rs:2317:    synth_with(status, Bytes::new(), close)
crates/envoy-http1/src/hcm.rs:2425:/// `direct_response` DOES carry. It therefore must NOT reuse [`synth_with`],
crates/envoy-http1/src/hcm.rs:2457:    synth_with(503, Bytes::from_static(b"no healthy upstream"), close)
crates/envoy-http1/src/hcm.rs:2472:    let mut resp = synth_with(
crates/envoy-http1/src/hcm.rs:11150:    /// Reusing the shared `synth_with` would emit a sixth header upstream does
crates/envoy-http1/src/hcm.rs:11435:    /// NOT reuse `synth_with`. MEASURED upstream: the transform DOES fire, the
crates/envoy-http2/src/hcm.rs:7332:    /// `synth_with`, any `synth_*` wrapper, or `build_response`.
```

Definition at `:2286`; **four CALL sites** at `:2309`, `:2317`, `:2457`,
`:2472`; the rest are doc mentions. **The SPEC's §1.5 CORRECTION 1 holds.**

**The transform appears at EXACTLY TWO places in the whole workspace — one per
wire funnel, and neither is a shared path:**

```
$ grep -rn 'apply_grpc_local_reply' crates/ | grep -v 'src/grpc.rs'
crates/envoy-http1/src/uring.rs:525:    crate::grpc::apply_grpc_local_reply(resp, req_headers);
crates/envoy-http1/src/hcm.rs:1491:            crate::grpc::apply_grpc_local_reply(&mut outgoing, &req.headers);
```

**Global Constraint 1 is SATISFIED**: no occurrence inside `synth_with`, inside
any `synth_*` wrapper, or inside `build_response`/`build_response_in`.

**W-3 — the H1/H2 sharing edge, re-confirmed, and the PLAN's line citation is
the one that drifted:**

```
$ sed -n '516,522p' crates/envoy-http2/src/hcm.rs
    let request_path = match decode_decision {
        envoy_filter::Decision::Continue => {
            H2RequestPath::Match(build_response(
                &config.inner,
                &mut envoy_req,
                /* close = */ false,
            ))
```

The call spans **`:518-522`**. `PROGRESS.md` deviation **D-2 is CORRECT** and
the PLAN's `:513-518` is not; `:512-514` is the explanatory comment. Re-derived
independently here.

**The module boundary that makes W-3 structural rather than conventional:**

```
$ grep -n 'mod grpc' crates/envoy-http1/src/lib.rs
21:pub(crate) mod grpc;
```

`pub(crate)` — **`envoy-http2` cannot reach the transform even by accident.**
That is stronger than the doc comments, and it was verified rather than assumed.

**W-4 — HTTP/2 is untouched, and every `grpc` token in `envoy-http2/src/hcm.rs`
is TEST-ONLY:**

```
$ grep -n 'grpc' crates/envoy-http2/src/hcm.rs
7346:    async fn h2_route_decision_reply_is_not_grpc_transformed() {
7354:                ("content-type".to_string(), "application/grpc".to_string()),
7371:                    "H2 must keep text/plain, not application/grpc"
7377:                        .any(|(n, _)| n.eq_ignore_ascii_case("grpc-status")),
7378:                    "H2 must carry NO grpc-status: {:?}",
7385:                        .any(|(n, _)| n.eq_ignore_ascii_case("grpc-message")),
7386:                    "H2 must carry NO grpc-message: {:?}",
```

All seven hits are at `:7346+`, inside the test module. **Not one line of H2
production code mentions gRPC.**

---

## The "33 new tests" claim — re-derived from the source, not from the delta

The identity `2227 − 2194 = 33` proves the count MOVED by 33; it does not prove
WHERE. Counted directly:

```
$ grep -c '#\[test\]\|#\[tokio::test\]' crates/envoy-http1/src/grpc.rs
18
$ <python: test attribute within 3 lines above each `fn grpc_*|non_grpc_*` in hcm.rs>
TEST  grpc_local_reply_transforms_direct_response
TEST  non_grpc_request_leaves_direct_response_untouched
TEST  grpc_local_reply_transforms_route_not_found_without_grpc_message
TEST  grpc_detection_edges_hold_through_the_seam
TEST  grpc_local_reply_header_order_matches_upstream
NOT   grpc_no_healthy_upstream_config          <- a config BUILDER, not a test
TEST  grpc_transforms_synth_400_bad_host
TEST  grpc_transforms_synth_redirect_and_keeps_location
TEST  grpc_transforms_synth_501_chunked_rejection
TEST  grpc_transforms_synth_no_healthy_upstream_with_message
TEST  grpc_transforms_filter_stop_and_send_on_decode
TEST  grpc_transforms_filter_stop_and_send_on_encode
TEST  grpc_does_not_transform_a_proxied_upstream_response
TEST  grpc_transform_is_visible_to_the_access_log
TEST  grpc_transform_ticks_the_2xx_response_class
TESTS: 14
$ grep -c 'h2_route_decision_reply_is_not_grpc_transformed' crates/envoy-http2/src/hcm.rs
1
```

**18 + 14 + 1 = 33 — the delta is fully accounted for by name.** The
`grpc_no_healthy_upstream_config` trap (a `grpc_`-prefixed *builder* that a
name-based count would over-count) was checked and correctly excluded, exactly
as `PROGRESS.md` states.

`grpc.rs`'s non-test/test split, re-derived:

```
$ <python: first `#[cfg(test)]` line in grpc.rs>
cfg(test) at 218 -> non-test 217, test 492      (217 + 492 = 709)
```

Matches `PROGRESS.md`'s `217 non-test / 492 test` exactly.

---

## Gate (c) — conformance, and the h2spec self-skip PROVEN on this host

`known-failures.txt` is untouched at **21 lines / ONE real entry** (quoted
above). **The h2spec gate's local self-skip is not taken from ADR-0163 on
trust — it was reproduced:**

```
$ cargo test -p h2spec-conformance -- --nocapture
h2spec_runner: h2spec not found — skipping locally
test h2spec_pass_rate_gate ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**That green is worth NOTHING.** The gate printed `h2spec not found — skipping
locally` and still reported `ok`. Gate (c) is settled ONLY on the CI build log,
by `grep -c 'h2spec not found'` = 0 **with** `test h2spec_pass_rate_gate ... ok`
present — recorded below.

## Gate (d) — no new fuzz target, and all five are wired into CI

```
$ grep -c 'cargo fuzz run' .github/workflows/ci.yml
0
$ grep -c 'fuzz run' .github/workflows/ci.yml
5
```

⚠ The bare `cargo fuzz run` probe returns a believable **0** — the workflow
invokes `cargo +nightly fuzz run`, because the workspace `rust-toolchain.toml`
pins stable. The five steps are `parse_bootstrap`, `jwt_parse`,
`cdn_loop_parse`, `accesslog_format_parse`, `grpc_health_decode`, each
`-max_total_time=30`, and the job name enumerates all five. **No target was
added, and none is required** — §7.4's trigger does not fire (no parser, no
codec, no filter, no config surface).

## Gate (b) — the 88 differential fixtures

Blast radius MEASURED ZERO (above), and all 88 fixture directories / 88
differential test files are unchanged. Every fixture that reddened locally was
classified by isolation; **CI is authoritative** for the backend-routing ones,
which red on this host's `192.168.65.2` bridge. The pinned reference image is
present at the `ENVOY_TARGET.md` digest:

```
$ docker image inspect envoyproxy/envoy:v1.33.0 --format '{{.Id}}'
sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2
```


---

## §8 / §7.5 gate — verdict per item

| item | verdict | evidence |
|---|---|---|
| **(a)** no new differential fixture | **PASS (N/A)** | fixture dirs still **88**; no `0089` |
| **(b)** all 88 pre-existing fixtures green | **PASS locally, CI-authoritative** | blast radius **0** hits; every local RED classified by isolation; 24/29 pass alone, 5 are the ADR-0164 host-only core |
| **(c)** conformance unchanged | **PASS locally, CI-DECIDES** | `known-failures.txt` **21** lines / **1** real entry; the local h2spec green is a proven self-skip and settles nothing |
| **(d)** no new fuzz target | **PASS** | **5** targets across five crates, all five wired in `ci.yml` |
| **(e)** the five workspace commands | **PASS** | build exit 0 (1 `Compiling`); clippy exit 0 (**14** `Checking`, 0 warnings, `--all-features`); fmt exit 0 + 0 diff bytes; test `passed+failed=2227`, zero real regressions; deny exit 0 on the four-ok line |
| **(f)** `REVIEW.md` APPROVED | **NOT THIS SESSION** | state 5 writes it (§5.1; ADR-0127). Correctly absent. |

**Gate (e) is fully GREEN. Gates (a), (b) and (d) are GREEN. Gate (c) is green
only once the CI log confirms the h2spec gate genuinely EXECUTED. Gate (f) is
out of scope for a state-4 by construction.**

### What this state-4 did NOT do — by design

No code changed. No banked finding fixed (§6.3; ADR-0165) — the `109.2`
M-1…M-8 / N-1…N-11, the `109.1` and `108.2` Minor/Nit sets, **CF-110-1
(NARROWED) / CF-110-2 / CF-110-3 (REASSIGNED) / CF-110-4 / CF-110-5**,
CF-109-1 (WIDENED)/2/3, CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, CF-72-2/CF-75-1,
M71-6, CF-74-1/2/3/4/6, CF-73-1 and the HTTP-filters-family (1)-(4) are all
carried unchanged. No `REVIEW.md`. No fixture `0089`. No ADR — head stays
**ADR-0179**, `ADR-0180` UNRESERVED. **`ROADMAP.md` NOT touched** (a state-4
never touches it; row `110.1` stays `planned` until its own state-6). No landed
artifact edited.

### Findings this verification banks for the state-5 reviewer

- **V-1 (METHOD, no code impact).** A back-to-back isolation loop without a
  settle gap manufactured a false `FAILS-IN-ISOLATION` verdict on two tests, on
  the very axis the doctrine uses to separate flakes from regressions. Root
  cause proven by `docker inspect`: a host port collision (from three FOREIGN
  `contrib-v1.37.2` containers, proven not to be this repo's) stopped the
  *upstream reference* container from starting. Settle gaps + repeated rounds +
  a same-shape control resolved it. **The stable core is FIVE.**
- **V-2 (CITATION, no code impact).** `PROGRESS.md` D-1 and the handoff both
  cite `awk 'NR<2532'` for `hcm.rs`'s non-test region; the boundary is **2579**
  at `HEAD` and `NR<2532` now returns **3** `grpc` hits. The number was true at
  Task 4 (`507c67b`) and went stale when Task 5 landed. **The soundness claim
  underneath it SURVIVES** when re-derived at the correct boundary: the three
  hits are two comments plus the seam call, and the `GRPC_STATUS` constant is
  consumed only in `grpc.rs`, so the idempotence sentinel is still exact.
- **V-3 (EVIDENCE ADDED, CF-110-5 stays OPEN).** The claim that
  `clippy --all-features` is the *only* gate covering the io_uring seam is now
  proven CAUSALLY, in both directions, from a scratch worktree with an
  unmutated control: with the flag the gate REDs at `uring.rs:526`; without it
  the gate is a real 20-`Checking` run that never mentions the file.

---

## STOP CONDITION — evaluated at session close, ALL THREE LEGS RE-DERIVED FROM DISK

The mission is COMPLETE only when **every** ROADMAP row is `done` **AND** no
in-scope leaf remains. **ALL THREE LEGS MUST HOLD. IT IS NOT COMPLETE** — this
is the **FIFTY-FIRST** consecutive evaluation and the answer is FALSE again.

**Leg (i) — FALSE.** 116 rows; three are not `done`:

```
rows: 116   Counter({'done': 113, 'planned': 2, 'in-progress': 1})
not done: [('110','in-progress'), ('110.1','planned'), ('110.2','planned')]
```

**Leg (ii) — FALSE**, by DIRECT TREE PROBES rather than by the ledger's own
assertion:

```
crates: 14 -> envoy-accesslog envoy-admin envoy-bin envoy-cluster envoy-config
              envoy-filter envoy-health envoy-http1 envoy-http2 envoy-jwt
              envoy-listener envoy-stats envoy-tcp envoy-tls
  envoy-http3 NO   envoy-grpc NO   envoy-wasm NO   envoy-protos NO   envoy-runtime NO
quinn in crate manifests: 0     tonic-web: 0     wasmtime: 0
tests/conformance contents: h2spec
runtime_key_is_rtds_inert hits: 2
```

The unbuilt set is the gRPC DATA path, `RuntimeUInt32`/CSRF honoring, RTDS, hot
restart / graceful drain, network-filter payload codecs, `sni_cluster`,
non-deterministic + priority/panic/locality LB, HTTP/3 + QUIC, the
observability sinks and the WASM host.

**Leg (iii) — FALSE.** Two of the eleven `### ` family headings still carry ZERO
rows, by a heading-slice census over all eleven:

```
 10  ### HTTP filters family
  5  ### Network filters family
  3  ### Load balancing family
 14  ### Upstream robustness family
  0  ### HTTP/3 + QUIC family
  3  ### gRPC family
  6  ### xDS / dynamic config family
 29  ### Observability family
  6  ### Runtime + hot restart family
  0  ### WASM host family
 13  ### Deprecated / edge features
counts: 10/5/3/14/0/3/6/29/6/0/13 = 89 under headings + 27 before first heading = 116
zero-row headings: ['### HTTP/3 + QUIC family', '### WASM host family']
```

**NO `stop` FILE WAS CREATED**; `ls stop` returns `No such file or directory`.

---

## CI confirmation — gate (c) settled, and the identity correctly did NOT move

Recorded by the follow-up record commit `a1e2cdd` (numstat `2 0`), on the
state-4 advance commit's FULL 40-char SHA interpolated from `git rev-parse HEAD`
— never retyped, because a short or retyped SHA silently returns `[]`.

```
$ gh run list --commit e89d2786c2fce4ec5ac5de5fce8890a3566ef19a --json databaseId,status,conclusion,headSha
[{"conclusion":"success","databaseId":32256633910,
  "headSha":"e89d2786c2fce4ec5ac5de5fce8890a3566ef19a","status":"completed"}]

$ gh api repos/pgdad/envoy-rust/actions/runs/32256633910/jobs \
   --jq '.jobs[] | {name, id, conclusion, runner_name, steps: (.steps|length)}'
{"conclusion":"success","id":96079736111,
 "name":"fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse + grpc_health_decode,...",
 "runner_name":"GitHub Actions 1000005381","steps":13}
{"conclusion":"success","id":96079736277,"name":"build + test + lint",
 "runner_name":"GitHub Actions 1000005380","steps":15}
```

**Attempt 1, no rerun needed. Steps 15/13, both jobs enumerated via the jobs API
and selected BY NAME, both with REAL runner names — not the `runner_name:""` +
`steps:0` starvation shape.**

### The census — the `(ok|FAILED)` recipe, awk fields 4/6

```
$ wc -c ci_build.log
682489 ci_build.log
$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' ci_build.log \
   | awk '{b++; p+=$4; f+=$6} END {print "binaries="b" passed="p" failed="f" sum="p+f}'
binaries=165 passed=2227 failed=0 sum=2227
$ grep -c 'test result: FAILED' ci_build.log
0
```

**`binaries=165 passed=2227 failed=0`.** The identity **did NOT move** — 2227 →
2227 — which is exactly what a DOCS-ONLY commit must show (and docs-only pushes
DO build). The state-3 commit moved it 2194 → 2227 for the 33 tests added; this
commit adds none, and it did not move. **The local `passed + failed = 2227`
cross-checks against CI's `passed = 2227` with `failed = 0`, so every RED
classified above as host-environmental is confirmed to pass in CI** — including
all five ADR-0164 stable-core members and all 24 pass-alone names.

### Gate (c) — settled here, and ONLY here

```
$ grep -c 'h2spec not found' ci_build.log
0
$ grep -o 'test h2spec_pass_rate_gate \.\.\. ok' ci_build.log
test h2spec_pass_rate_gate ... ok
```

**Zero self-skip messages AND the gate reported `ok`, so h2spec genuinely
EXECUTED — the EIGHTH consecutive commit (ADR-0163).** Contrast the local run
reproduced earlier in this section, which printed `h2spec not found — skipping
locally` and *still* reported `ok`. **Gate (c) is GREEN.**

### Final gate ledger

**(a) N/A · (b) GREEN · (c) GREEN · (d) GREEN · (e) GREEN · (f) state-5's.**
Every §8 item is now green or correctly out of scope for a state-4.

---

## Next state

Sub-phase `110.1` now sits at §5 **state 4 complete** — `SPEC.md` + `PLAN.md` +
`PROGRESS.md` (this verification appended), and still **NO `REVIEW.md`**. The
next session runs §5 **state 5** (`superpowers:requesting-code-review`) and
writes `110.1/REVIEW.md` — a SEPARATE session per §5.1 and ADR-0127, because
the context that GRADED the gate must not also review the code. It should read
findings **V-1**, **V-2** and **V-3** above before starting, and it must NOT fix
anything: a state-5 produces a review, and any issue it raises sends the work
back to state 3, not to state 4 (§5.2).

Sibling `110.2` (fixture `0089` + the `BEHAVIOR_CONTRACT.md` `## gRPC` section +
the parent-110 close) remains BLOCKED until `110.1` is `done`.
