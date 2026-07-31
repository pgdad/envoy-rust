# Sub-phase 76.2 — `Route.redirect` runtime + fixture `0086` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` (or
> `superpowers:subagent-driven-development` for the independent tasks) to implement this plan
> task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Every task is TDD: the
> failing test is written and RUN RED before any implementation line** (doctrine D-3.1).

**Goal:** Make a configured `redirect:` route actually serve a real 3xx response carrying a
byte-correct `location:` header — replacing sub-phase `76.1`'s honest `synth_501` placeholder —
and prove it against upstream Envoy `v1.33.0` with a new backend-free differential fixture `0086`.

**Architecture:** A single **pure, total** function `plan_redirect()` encodes the whole MEASURED
upstream rule set (scheme / authority / path / query / status) and is exhaustively unit-testable
without a socket. A **dedicated** `synth_redirect()` response builder emits the exact five-header
redirect shape — deliberately **not** reusing the shared `synth_with()`, which always emits a
`content-type` that upstream's redirect does not carry. Both are consumed by **one** new arm at
the single `match &route.action` dispatch seam in `crates/envoy-http1/src/hcm.rs`, which serves
**both codecs** because HTTP/2 has no route-action dispatch of its own and calls H1's
`build_response`. `build_response`/`build_response_in` widen from `&Request` to `&mut Request` so
`prefix_rewrite` can rewrite the request's `:path` in place, which is what makes the rewrite
observable in the access log.

**Tech Stack:** Rust (workspace toolchain pin, `rust-toolchain.toml`); `serde`/`serde_yaml`
(config); `bytes`; the in-repo `differential` test crate (`testcontainers` + upstream Envoy
`envoyproxy/envoy:v1.33.0`).

---

## 0. Read this first — the state of the world (doctrine D-3.4: zero prior context assumed)

This plan is the §5 **state-2** output for sub-phase `76.2`. Its input is
`docs/envoy-rust/phases/76.2-redirect-runtime-fixture/SPEC.md` (556 lines), which banks every
MEASURED upstream behaviour as tables R1-R16 / Q1-Q4 / E1-E2. **Read the SPEC's §2 before
starting** — this plan does not restate every measured cell, it encodes them.

**Sibling `76.1` is `done`.** It landed the CONFIG surface: the `RedirectAction` struct (with
presence-preserving `Option`s), the five-value `RedirectResponseCode` enum with its `status()`
mapping, the `RouteAction::Redirect` variant, the widened `Route` visitor, the two oneof
validators, both `Serialize` arms, and an **honest `synth_501` placeholder** at the runtime
dispatch seam pinned by a test named `build_response_redirect_is_not_implemented_placeholder`.
**76.2 deliberately flips that test** (Task 5) — it is a named test precisely so the replacement
is visible rather than silent.

**Do not** edit `76.1/SPEC.md`, `76.1/PLAN.md`, `76.1/PROGRESS.md`, `76.1/REVIEW.md`,
`76/SPEC.md` or `76.2/SPEC.md` — all landed historical artifacts (D-3.5).
**Do not** flip any `ROADMAP.md` status cell during implementation — that happens at the §5
state-6 close-out only.

---

## 1. Citation re-verification — MEASURED ON DISK at commit `537e2a1`

`76.2/SPEC.md` was authored at commit `f438cb9`, **before `76.1` landed**. `76.1` added ~650 lines
to `crates/envoy-config/src/bootstrap.rs` and ~71 lines to `crates/envoy-http1/src/hcm.rs`, so a
subset of the SPEC's `file:line` citations has drifted. **Every citation below was re-verified by
anchoring on TEXT, never on a number.** Use THIS table, not the SPEC's numbers.

| SPEC citation | verdict | measured on disk at `537e2a1` |
|---|---|---|
| `hcm.rs:2110` `match &route.action` | **VERIFIED** | `:2110` exactly |
| `hcm.rs:2112` bare `"direct_response"` literal | **VERIFIED** | `:2112` exactly |
| `hcm.rs:2086` / `:2105` `"route_not_found"` | **VERIFIED** | both exactly |
| `hcm.rs:2051-2055` `build_response_in` decl | **VERIFIED** | `:2051-2055` exactly |
| `hcm.rs:1601-1608` `build_access_log_record` | **VERIFIED** | fn at `:1601` |
| `hcm.rs:859` `let mut req = req;` | **VERIFIED** | `:859` exactly |
| `hcm.rs:2185-2204` `synth_with` | **DRIFTED → `:2193-2212`** | +8 (the 76.1 placeholder arm) |
| `hcm.rs:2183-2184` `synth_with` doc | **DRIFTED → `:2184-2192`** | +8 |
| `hcm.rs:2242-2251` `synth_overflow` | **DRIFTED → `:2250-2262`** | +8 |
| `hcm.rs:2172-2174` `connection_value` | **DRIFTED → `:2180-2182`** | +8 |
| `hcm.rs:2155-2166` `route_matches` | **DRIFTED → `:2163`** | +8 |
| `hcm.rs:2138-2143` `strip_port` | **DRIFTED → `:2146`** | +8 |
| `hcm.rs:2019` `resolve_route_in` Host-miss | **DRIFTED → `:2015` (fn) / `:2019` (the `?`)** | fn decl at `:2015` |
| `hcm.rs:9690` / `:9707` — the "two" in-file test call sites | **REFUTED** | there are **THREE**: `:9734`, `:9761`, `:9778`. `76.1`'s T-C9 added the third. |
| "**8** `build_response`/`build_response_in` call sites" | **REFUTED** | **7 call sites** + 2 definitions. Full list in Task 5. |
| "**9** `BuildOutcome::Synth(` construction sites" | **DRIFTED → 12** | `hcm.rs` 9, `uring.rs` 2, `envoy-http2/src/hcm.rs` 1. The rejected alternative is *more* churn than the SPEC said, so its rejection stands stronger. |
| `response.rs:188-215` `canonical_reason` | **VERIFIED** | `:188` fn, `:195` 301, `:196` 302, `:213` `_ => "OK"` — all exact |
| `response.rs:184-187` its doc comment | **DRIFTED → `:183-187`** | off by one at the start |
| `bootstrap.rs:2572-2581` `RouteMatch` | **DRIFTED → `:2657-2666`** (`prefix` at `:2661`) | +85 |
| `bootstrap.rs:2591` visitor `_` catch-all | **VERIFIED** | `:2591` exactly (`"…more than one is present"` at `:2594`) |
| `envoy-http2/src/hcm.rs:18` H1 import | **VERIFIED** | `:18` exactly |
| `envoy-http2/src/hcm.rs:459` `let mut envoy_req` | **VERIFIED** | `:459` exactly |
| `envoy-http2/src/hcm.rs:475` `resolve_route` | **VERIFIED** | `:475` exactly |
| `envoy-http2/src/hcm.rs:518` `build_response` | **VERIFIED** | `:518` exactly |
| `envoy-http2/src/hcm.rs:489` `mem::take` | **DRIFTED → `:490-492`** | three takes |
| `uring.rs:280` `req.body = …` / `:287` the call | **DRIFTED → `:278`** / **VERIFIED `:287`** | |
| `differential/src/lib.rs:1177-1181` `HEADER_ALLOW_LIST` | **VERIFIED** | exactly 3 entries, all `AllowMode::NameRequired` |
| `lib.rs:1192-1247` `diff_headers` | **VERIFIED** | `:1192`-`:1247` exactly |
| `lib.rs:1199-1215` name-set check | **VERIFIED** | `BTreeSet` compare at `:1206-1215` |
| `lib.rs:1237` value-exact compare | **VERIFIED** | `:1237` exactly |
| `lib.rs:1144-1165` `Http1Probe` (`host` at `:1149`) | **VERIFIED** | exactly |
| `lib.rs:1069-1073` `Http1HeaderRule` | **DRIFTED → enum at `:1071`** | attrs above |
| `lib.rs:1081-1085` `HeaderRule` | **DRIFTED → enum at `:1082`** | |
| `lib.rs:5421-5536` `run_http1_probe_list_arm` | **DRIFTED → fn at `:5423`** | |
| `lib.rs:2194-2206` `drive_http1` request write | **VERIFIED (fn at `:2182`)** | |
| `tests/differential/Cargo.toml` zero `[[test]]` | **VERIFIED** | 0 |
| 85 fixture dirs / 85 differential `.rs` / `0086` free | **VERIFIED** | 85 / 85 / `git ls-files 'tests/fixtures/0086*'` → 0 |
| `BEHAVIOR_CONTRACT.md` has no Phase 76 heading | **VERIFIED** | `grep -c 'Phase 76'` → **0**. Phase 75 spans `:2751`–`:2958`; `## xDS wire state machine` is at `:2959`. |
| `rds.rs:135` the CF-76-2 `if let` | **VERIFIED** | `:135` exactly |
| `bootstrap.rs:4076` / `:4082` oneof validators | **VERIFIED** | exactly |

**Newly measured, not in the SPEC at all — and Task 3 depends on it:**
`crates/envoy-http1/src/hcm.rs` imports `envoy_config` types at `:11-14` and that import list does
**NOT** contain `RedirectAction` — `76.1` imported it only inside the `#[cfg(test)]` module at
`:2362`. Adding `plan_redirect` to the non-test body therefore fails to compile until the
top-level import is widened. **This was caught by the pre-flight, not by reading** (§2).
Likewise the test module's `use super::*;` is at `:2360` (the Standing-traps line's `:2353` has
drifted).

Also newly measured: `crates/envoy-http1/src/headers.rs` has **no** `LOCATION` constant (7 name
constants: `HOST`, `CONTENT_LENGTH`, `CONNECTION`, `SERVER`, `DATE`, `TRANSFER_ENCODING`,
`CONTENT_TYPE`). Task 1 adds it.

---

## 2. Pre-flight record — the literal Rust in this plan was COMPILED, LINTED and RUN

Per the recurring project failure where a PLAN's example code trips the plan's own gate, **every
literal Rust block in Tasks 1-5, 8 and 9 was applied to a scratch `git worktree` detached at
`537e2a1` and gated before this plan was written.** MEASURED results:

| gate | result |
|---|---|
| `cargo fmt --all -- --check` | **exit 0, ZERO output** — rustfmt would change nothing; the literals below are already canonical and transcribe verbatim |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **exit 0**, with **8** `Checking` lines (`envoy-http1`, `envoy-http2`, `envoy-admin`, `envoy-health`, `envoy-bin`, `differential`, `http1-echo-server`, `http2-echo-server`) — a real re-check, **not** a fully-cached no-op |
| `cargo clippy -p envoy-config --all-targets --all-features -- -D warnings` (forced re-check) | **exit 0**, **17** `Checking` lines |
| `cargo test -p envoy-config --lib -- rds::` | **`ok. 12 passed; 0 failed`** — the CF-76-2 rewrite of `rds.rs` breaks no existing RDS test |
| representative new tests | `plan_redirect_matches_every_measured_location_cell ... ok`, `synth_redirect_emits_five_names_and_no_content_type ... ok` |
| the 76.1 placeholder pin | `build_response_redirect_is_not_implemented_placeholder ... **FAILED**` — `left: 301, right: 501`. **This is the designed flip**, and it is direct evidence the new dispatch arm serves a real 301. |

**One caveat, stated honestly:** the pre-flight ran **4** of the 22 measured location rows (R1,
R5, Q2, E2) — enough to prove the code compiles, lints and computes correctly, not enough to
prove all 22 cells. **Task 3 must land all 22 rows and run them.** The scratch worktree was
removed; the main tree is clean.

---

## Global Constraints

- **Doctrine D-3.1 — TDD, no exceptions.** Test first, run it RED, then implement, then GREEN,
  then commit. Every task below is written in that order.
- **Doctrine D-3.8 — `unsafe` is forbidden.** No crate root's `#![forbid(unsafe_code)]` is
  touched.
- **Doctrine D-3.6 — every commit is a green build.** `cargo build --workspace --all-targets`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo fmt --all -- --check` must be clean at every task boundary.
- **No new crate, no new dependency, no new fuzz target, no `ci.yml` edit.** 76.2 adds none;
  §7.5 gate (d) is therefore satisfied by the existing 5 fuzz targets.
- **Never add `location` to `HEADER_ALLOW_LIST`** (`tests/differential/src/lib.rs:1177-1181`).
  It is compared value-exact by `diff_headers`, and that comparison **is** this sub-phase's entire
  differential witness. Adding it silently vacates the fixture.
- **Never trim `tests/conformance/h2spec/known-failures.txt`** (21 lines).
- **Never weaken a fixture** to make it pass.
- **`cargo build -p envoy-bin` before ANY local differential run** — the harness executes
  `target/debug/envoy-bin`, and a stale binary REDs every fixture with a bogus `unknown field`.
- **Always pass `-p differential`**, never a bare `--test <name>`: 33 test-binary names are
  duplicated between `tests/differential/tests/` and `crates/envoy-bin/tests/`.
- **`--no-fail-fast` is a `cargo test` flag, not a test-harness flag** — it goes *before* the
  `--`, e.g. `cargo test -p envoy-http1 --no-fail-fast -- <filter>`. Putting it after `--` fails
  with `error: Unrecognized option: 'no-fail-fast'`.
- **Redirect fixtures are backend-free and therefore FULLY verifiable locally** — a deliberate
  property of this phase's pick. Do not defer fixture `0086` to CI.
- **Out of scope, do NOT fix opportunistically** (§6.3): **CF-76-1** (upstream strips the query
  before route path matching), **CF-75-2**, **CF-75-3**, **CF-75-4**, **CF-75-5**, **CF-75-6**,
  and ADR-0028's deferred RDS re-validation beyond the one arm Task 8 names.

---

## 3. File structure

| file | disposition | responsibility after 76.2 |
|---|---|---|
| `crates/envoy-http1/src/headers.rs` | modify | +1 name constant `LOCATION` |
| `crates/envoy-http1/src/response.rs` | modify | `canonical_reason` gains 303/307/308 |
| `crates/envoy-http1/src/hcm.rs` | modify | `RedirectPlan`, `plan_redirect`, `synth_redirect`, the real dispatch arm, `&mut Request` on both `build_response*` signatures, and the in-process test bank |
| `crates/envoy-http1/src/uring.rs` | modify | one call site `&req` → `&mut req` |
| `crates/envoy-http2/src/hcm.rs` | modify | one call site `&envoy_req` → `&mut envoy_req`; + the H2 shared-seam test |
| `crates/envoy-config/src/bootstrap.rs` | modify | lift the two redirect oneof checks into `validate_redirect_oneofs`; fix the orphaned `RouteAction` doc comment |
| `crates/envoy-config/src/rds.rs` | modify | `if let` → exhaustive `match`; call the shared validator on the warm path (CF-76-2) |
| `tests/fixtures/0086-route-redirect-action/` | **create** | `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md` |
| `tests/differential/tests/route_redirect_action.rs` | **create** | the fixture entrypoint (cargo auto-discovers it; no registry edit) |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | modify | new `### Phase 76 …` section banking the measured rules |
| `docs/envoy-rust/phases/76.2-redirect-runtime-fixture/PROGRESS.md` | **create** | appended per task (state 3) |

---

## 4. §6.1 SPLIT DECISION — RE-DERIVED AND OWNED BY THIS SESSION

The §6.1 gate fires at **~25 numbered tasks** OR **~1500 net LoC estimated**.

**The SPEC's own projection (≈1083 / ~10-12 tasks) is NOT the input to this decision.** Per the
measured record, `76.1` came in at **774** actual net code LoC against its `PLAN.md`'s **≈515** —
a **+50% overshoot living almost entirely in the test half**. The measured actual-net-LoC band for
recent phases is `68→950, 69→1540, 70→1372, 73→873, 74→1981, 75.1→1413, 75.2→897, 76.1→774`.
(`76.1`'s 774 was re-derived here, not inherited: `git diff --numstat cf5cf85 9556b2c` excluding
`docs/`, = added 793 / deleted 19.)

So the estimate is re-derived **bottom-up**, and the non-test half plus two test groups are
**MEASURED from the pre-flight diff (286 net LoC)** rather than guessed:

| component | est. net LoC | basis |
|---|---|---|
| `LOCATION` const + 3 reason phrases + their tests | 40 | 13 MEASURED code + ~27 test |
| `RedirectPlan` + `plan_redirect` (+ doc) | 105 | **MEASURED** |
| the 22-row measured `location` table test | 255 | pre-flight's 4 rows MEASURED at 75; +18 rows × ~10 |
| `synth_redirect` + the five-name/no-`content-type` test | 60 | **MEASURED** (30 + 30) |
| `&mut Request`: 2 signatures + 7 call sites | 10 | **MEASURED** |
| real dispatch arm + T-C9 flip + detail-string assert | 60 | 27 MEASURED code + ~33 test |
| `prefix_rewrite` `:path` mutation + non-mutation pins | 110 | two async tests + an `HCMConfig` fixture |
| H2 shared-seam in-process test | 100 | mirrors the existing `h2_resolve_route_reachable_…` |
| CF-76-2 (validator lift + `rds.rs` match + 2 tests) | 120 | 44 MEASURED code + ~76 test |
| M-1/M-2 the orphaned `RouteAction` doc comment | 12 | **MEASURED** |
| fixture `0086` (2 configs + expectations + README + entrypoint) | 330 | 18 flow-style routes × 2 configs + 18 × 7-line probes |
| `BEHAVIOR_CONTRACT.md` Phase 76 section | 110 | modelled on the 207-line Phase 75 section |
| **TOTAL** | **≈ 1312** | |
| **TASKS** | **11** | |

**DECISION: NO SPLIT.** ≈1312 is under the ~1500 gate with ~13% headroom, and 11 tasks is well
under ~25. This is a **+21% upward correction** of the SPEC's ≈1083, which is exactly what
calibrating against measured phases (rather than against the projection) produces.

**The risk is named, not hidden.** If the executor writes the 22 location cells as 22 separate
`#[test]` fns in the house style (~18 lines each with doc block and assert messages) instead of
the table specified in Task 3, that one group alone grows from 255 to ~400 and the total lands
near ~1460 — still under, but with the headroom gone. **Two mitigations are binding on state 3:**

1. **Task 3's test is table-driven, by design, and that design is load-bearing** — a pure total
   function's cells belong in a table where a newly measured cell costs ONE line. Each row carries
   its own `label`, so a failure names the exact cell; attribution is not lost.
2. **§6.1's mid-execution trigger stays armed.** If any single task's sub-steps blow past ~10
   items, or the running net LoC crosses ~1500 before Task 10, **STOP and split per §6.2** with a
   new ADR (next free is **ADR-0171**, re-derived on disk: `grep -o '^## ADR-[0-9]\{4\}' … | sort
   -t- -k2 -n | tail -1` → `ADR-0170`). Do not quietly write the oversize plan's worth of code.

---

## 5. CF-76-2 DISPOSITION — **ADDRESSED**, not deferred (Task 8)

**The carry-forward.** `crates/envoy-config/src/rds.rs:135` re-validates a hot-reloaded route
table with `if let crate::RouteAction::Route(ar) = &route.action` — an **`if let`, not an
exhaustive `match`** — so `76.1` adding the `Redirect` variant tripped no compile error there.
Consequence: an **RDS hot reload** delivering `redirect: { path_redirect: "/p", prefix_rewrite:
"/q" }` is accepted warm and installed live, while the **byte-identical config at boot is
boot-fatal**.

**Why it was only a Minor at `76.1`, and why that changes now.** `76.1` joined an
ADR-0028-sanctioned hole rather than creating one (`rds.rs` already skips the pre-existing
`direct_response` validators), and its blast radius was **NIL** because the runtime arm was the
inert 501 either way. **76.2 removes that inertness.** After Task 5, those routes serve a real 3xx
built from fields never checked for mutual exclusivity. The condition that made it tolerable is
gone.

**Decision: ADDRESS it, minimally and precisely.** Task 8:

1. Lifts `76.1`'s two inline oneof checks (`bootstrap.rs:4076`/`:4082`) into a shared
   `pub(crate) fn validate_redirect_oneofs(rd, context, route)`, so boot and warm paths are the
   same code by construction rather than by discipline.
2. Converts `rds.rs`'s `if let` into an **exhaustive `match`**, restoring the compile-time forcing
   function for any future fourth `RouteAction` variant, and calls the shared validator on the
   `Redirect` arm.
3. Leaves `DirectResponse` as an explicit `=> {}` arm **with a comment naming ADR-0028**, so the
   pre-existing sanctioned deferral is documented rather than silently joined.

**Scope boundary — do NOT widen.** **ADR-0028 is NOT lifted.** Task 8 does not add `validate_hcm`,
`InvalidStatusCode` or `validate_data_source` to the RDS path. It closes exactly the hole `76.1`
opened, in the sub-phase where that hole goes live.

**The generalised lesson, carried forward:** `76.1/SPEC.md` §2.3's claim that *"the compiler
enforces the seam"* is weaker than it sounds. It holds only at genuine exhaustive `match` sites.
It did **not** hold at `rds.rs:135`'s `if let` (which is exactly how CF-76-2 happened) and it does
**not** hold at the visitor's own `_` catch-all (`bootstrap.rs:2591`), where a future fourth
`RouteAction` variant would silently fall into *"more than one is present"* rather than failing to
build. **Add any future `RouteAction` variant by AUDITING EVERY SITE BY GREP, never by trusting
the build.** Task 9's doc comment records this in the code itself.

### Banked `76.1` review findings — what this plan schedules, and what it does not

| finding | disposition in this plan |
|---|---|
| **M-1** `pub enum RouteAction` is undocumented (its 04.3 doc block orphaned onto `RedirectResponseCode`) | **SCHEDULED — Task 9.** `76.2` edits this exact region. |
| **M-2** the orphaned text is also stale (describes a TWO-way oneof) | **SCHEDULED — Task 9**, together with M-1. Fixing M-1 alone would re-attach stale text. |
| **M-5** = CF-76-2 | **SCHEDULED — Task 8** (see above). |
| **N-3** the T-C9 doc block (incl. "76.2 MUST flip this test") sits on the HELPER, not the test | **SCHEDULED — Task 5**, for free, as part of flipping T-C9. |
| M-3, M-4, M-6, N-1, N-2, N-4…N-9 | **NOT scheduled.** Banked, still open. They are polish on `76.1`'s config surface, not on `76.2`'s runtime surface; scheduling them would widen scope without a differential witness. |
| **N-10, N-11** (defects in the landed `76.1/PROGRESS.md`) | **NOT EDITABLE by any session** (D-3.5). Recorded for accuracy, not repair. |

---

## Task 1: `location` header constant + the three missing reason phrases

**Files:**
- Modify: `crates/envoy-http1/src/headers.rs` (after the `CONTENT_TYPE` constant)
- Modify: `crates/envoy-http1/src/response.rs` (`canonical_reason`, fn at `:188`)
- Test: `crates/envoy-http1/src/response.rs` (`#[cfg(test)] mod tests`, at `:218`)

**Interfaces:**
- Produces: `envoy_http1::headers::LOCATION` (`&str = "location"`); `canonical_reason(303|307|308)`
  returning the correct RFC 7231 phrases. Task 3 consumes `LOCATION`.

**Why this is in-process and not differential.** MEASURED: all five wire status lines are
`301 Moved Permanently`, `302 Found`, `303 See Other`, `307 Temporary Redirect`,
`308 Permanent Redirect`. envoy-rust's table has 301 and 302 but **303, 307 and 308 fall through
to `_ => "OK"`** — so a `SEE_OTHER` redirect emits `HTTP/1.1 303 OK` today. **The differential
fixture CANNOT catch this**: the harness's `drive_http1` parses the status *code* only and the
equivalence matrix compares `response_status: exact`; the reason phrase is not part of it. This is
a silent-wrong-answer hazard that only an in-process test closes.

- [ ] **Step 1: Write the failing test**

Append inside `crates/envoy-http1/src/response.rs`'s `mod tests`:

```rust
    /// 76.2: the three redirect reason phrases MEASURED on the wire against
    /// `envoyproxy/envoy:v1.33.0`. Before 76.2 all three fell through to
    /// `_ => "OK"`, so a `SEE_OTHER` redirect emitted `HTTP/1.1 303 OK`.
    /// The differential fixture CANNOT catch this — the harness parses the
    /// status CODE only — so this in-process pin is the ONLY guard.
    #[test]
    fn canonical_reason_covers_the_three_redirect_codes() {
        assert_eq!(canonical_reason(303), "See Other");
        assert_eq!(canonical_reason(307), "Temporary Redirect");
        assert_eq!(canonical_reason(308), "Permanent Redirect");
        // Guard the two that already worked, so a careless table edit is caught.
        assert_eq!(canonical_reason(301), "Moved Permanently");
        assert_eq!(canonical_reason(302), "Found");
    }
```

- [ ] **Step 2: Run it RED**

Run: `cargo test -p envoy-http1 --lib -- canonical_reason_covers_the_three_redirect_codes`
Expected: **FAIL**, `assertion `left == right` failed  left: "OK"  right: "See Other"`.

- [ ] **Step 3: Implement — the three phrases**

In `crates/envoy-http1/src/response.rs`, replace the two lines
`        302 => "Found",` / `        304 => "Not Modified",` with:

```rust
        302 => "Found",
        // 76.2: MEASURED on the wire against envoyproxy/envoy:v1.33.0 —
        // `HTTP/1.1 303 See Other` / `307 Temporary Redirect` /
        // `308 Permanent Redirect`. Before 76.2 all three fell through to
        // `_ => "OK"`, so a `SEE_OTHER` redirect emitted `HTTP/1.1 303 OK`.
        // The differential fixture CANNOT catch this — the harness parses the
        // status CODE only — so these three are pinned in-process.
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
```

- [ ] **Step 4: Implement — the `location` constant**

In `crates/envoy-http1/src/headers.rs`, immediately after
`pub const CONTENT_TYPE: &str = "content-type";`:

```rust
/// 76.2 NEW: the redirect response's `location:` header. Deliberately NOT on
/// the differential harness's 3-entry `HEADER_ALLOW_LIST`, so it is compared
/// value-exact by `diff_headers` — never add it there.
pub const LOCATION: &str = "location";
```

- [ ] **Step 5: Run GREEN**

Run: `cargo test -p envoy-http1 --lib -- canonical_reason`
Expected: **PASS**.
Then: `cargo fmt --all -- --check` (expect exit 0, zero output) and
`cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings` (expect exit 0 with a
non-zero `Checking` line count — a clippy green with ZERO `Checking` lines is a fully-cached
no-op, not evidence).

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-http1/src/response.rs crates/envoy-http1/src/headers.rs
git commit -m "phase 76.2 task 1: canonical_reason 303/307/308 + the location header constant"
```

---

## Task 2: widen the `envoy_config` import so `RedirectAction` is nameable outside tests

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs:11-14` (the top-level `use envoy_config::{…}`)
- Modify: `crates/envoy-http1/src/hcm.rs:2361-2363` (the test module's `use envoy_config::{…}`)

**Interfaces:**
- Produces: `RedirectAction` nameable in `hcm.rs`'s non-test body. Task 3 depends on this.

**Why this is its own step.** MEASURED: `76.1` imported `RedirectAction` **only** inside
`#[cfg(test)] mod tests`. The pre-flight (§2) hit exactly this and failed with
`error[E0425]: cannot find type `RedirectAction` in this scope --> hcm.rs:2276:10`. Folding it
into Task 3 would make Task 3's RED ambiguous — a compile error from a missing import looks
identical to a compile error from a missing function.

- [ ] **Step 1: Widen the top-level import**

Replace `crates/envoy-http1/src/hcm.rs:11-14` with:

```rust
use envoy_config::{
    AttemptOutcome, DirectResponse, HashPolicy, HttpConnectionManagerConfig, RedirectAction,
    RetryConfig, Route, RouteAction, RouteConfiguration, VirtualHost,
};
```

- [ ] **Step 2: Drop the now-redundant test-module import**

The test module opens with `use super::*;`, so its own `RedirectAction` import becomes redundant.
Replace the three-line test import at `:2361-2363` with the single line:

```rust
    use envoy_config::{DataSource, HashPolicyHeader, LbMetadata, RouteAction_Route, RouteMatch};
```

- [ ] **Step 3: Verify nothing broke**

Run: `cargo build -p envoy-http1 --all-targets`
Expected: **success**, no `unused_imports` warning.
Then: `cargo fmt --all -- --check` (exit 0, zero output).

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 76.2 task 2: import RedirectAction into hcm.rs's non-test body"
```

---

## Task 3: the pure `location`-builder — `RedirectPlan` + `plan_redirect`

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` — insert immediately **after** the `synth_status`
  function (which ends `    synth_with(status, Bytes::new(), close)\n}`) and **before**
  `synth_no_healthy_upstream`'s doc block
- Test: `crates/envoy-http1/src/hcm.rs` `#[cfg(test)] mod tests`

> **Anchor by TEXT, not by number** — insert after the literal
> `pub(crate) fn synth_status(status: u16, close: bool) -> Response {` block.
> **And check for a doc comment above whatever you insert before.** `76.1`'s M-1 was caused by a
> PLAN that said "insert immediately before the `#[derive]`" without mentioning the doc comment
> above it, orphaning it onto the new type. `synth_no_healthy_upstream` **has** a doc block
> (`/// 12.2 (parent-12 D6.2 per ADR-0037): …`); insert **above** that doc block, never between
> it and its function.

**Interfaces:**
- Consumes: `RedirectAction` (Task 2), `strip_port` (already in `hcm.rs`),
  `RedirectResponseCode::status()` (landed at `76.1`).
- Produces: `pub(crate) struct RedirectPlan { location: String, status: u16, rewritten_path:
  Option<String> }` and `fn plan_redirect(authority: &str, target: &str, matched_prefix:
  Option<&str>, rd: &RedirectAction) -> RedirectPlan`. Task 5 consumes both.

**The design constraint.** `plan_redirect` is **pure and total**: no I/O, no panics, no clock. That
is what makes all 22 measured cells unit-testable without a socket, and it is why the `path.get(..)`
slice is used instead of `&path[..]`.

- [ ] **Step 1: Write the failing test — all 22 measured cells**

Insert into `hcm.rs`'s `mod tests`. **This is the complete list; do not sample it.** Rows are
lifted from `76.2/SPEC.md` §2.3 (R1-R16, Q1-Q4, E1-E2).

```rust
    /// 76.2 T3-1: the MEASURED `location` table. One row per upstream cell
    /// measured against `envoyproxy/envoy:v1.33.0` (SPEC 2.3 — R1-R16, Q1-Q4,
    /// E1-E2). Table-driven ON PURPOSE: `plan_redirect` is a pure total
    /// function, so a newly measured cell must cost ONE line. Each row carries
    /// its own `label`, so a failure names the exact cell.
    #[test]
    fn plan_redirect_matches_every_measured_location_cell() {
        struct Cell {
            label: &'static str,
            host: &'static str,
            prefix: Option<&'static str>,
            target: &'static str,
            rd: RedirectAction,
            status: u16,
            location: &'static str,
        }
        fn rd(f: impl FnOnce(&mut RedirectAction)) -> RedirectAction {
            let mut r = RedirectAction::default();
            f(&mut r);
            r
        }
        fn cell(
            label: &'static str,
            host: &'static str,
            prefix: &'static str,
            target: &'static str,
            rd: RedirectAction,
            status: u16,
            location: &'static str,
        ) -> Cell {
            Cell {
                label,
                host,
                prefix: Some(prefix),
                target,
                rd,
                status,
                location,
            }
        }
        use envoy_config::RedirectResponseCode as RC;
        let cells = vec![
            cell(
                "R1 host_redirect replaces the authority",
                "envoy-rust.test",
                "/a-host",
                "/a-host",
                rd(|r| r.host_redirect = Some("example.com".into())),
                301,
                "http://example.com/a-host",
            ),
            cell(
                "R2 the query is preserved by default",
                "envoy-rust.test",
                "/b-query",
                "/b-query/deep?a=b",
                rd(|r| r.host_redirect = Some("example.com".into())),
                301,
                "http://example.com/b-query/deep?a=b",
            ),
            cell(
                "R3 path_redirect replaces the path wholesale",
                "envoy-rust.test",
                "/c-pathr",
                "/c-pathr/sub",
                rd(|r| r.path_redirect = Some("/newpath".into())),
                301,
                "http://envoy-rust.test/newpath",
            ),
            cell(
                "R4 path_redirect STILL keeps the query",
                "envoy-rust.test",
                "/d-pathq",
                "/d-pathq/x?k=v",
                rd(|r| r.path_redirect = Some("/newpath".into())),
                301,
                "http://envoy-rust.test/newpath?k=v",
            ),
            cell(
                "R5 prefix_rewrite replaces only the matched span",
                "envoy-rust.test",
                "/e-pfx",
                "/e-pfx/sub",
                rd(|r| r.prefix_rewrite = Some("/replaced".into())),
                301,
                "http://envoy-rust.test/replaced/sub",
            ),
            cell(
                "R6 https_redirect forces the scheme",
                "envoy-rust.test",
                "/f-https",
                "/f-https/x",
                rd(|r| r.https_redirect = Some(true)),
                301,
                "https://envoy-rust.test/f-https/x",
            ),
            cell(
                "R7 response_code TEMPORARY_REDIRECT",
                "envoy-rust.test",
                "/g-c307",
                "/g-c307",
                rd(|r| {
                    r.host_redirect = Some("example.com".into());
                    r.response_code = RC::TemporaryRedirect;
                }),
                307,
                "http://example.com/g-c307",
            ),
            cell(
                "R8 strip_query drops the query",
                "envoy-rust.test",
                "/h-strip",
                "/h-strip/a?q=1&z=2",
                rd(|r| {
                    r.host_redirect = Some("example.com".into());
                    r.strip_query = true;
                }),
                301,
                "http://example.com/h-strip/a",
            ),
            cell(
                "R9 port_redirect alongside host_redirect",
                "envoy-rust.test",
                "/i-port",
                "/i-port",
                rd(|r| {
                    r.host_redirect = Some("example.com".into());
                    r.port_redirect = Some(8443);
                }),
                301,
                "http://example.com:8443/i-port",
            ),
            cell(
                "R10 a bare redirect{} echoes the request",
                "envoy-rust.test",
                "/j-bare",
                "/j-bare/deep",
                RedirectAction::default(),
                301,
                "http://envoy-rust.test/j-bare/deep",
            ),
            cell(
                "R11 scheme_redirect is NOT allow-listed — `ftp` is emitted verbatim",
                "envoy-rust.test",
                "/k-scheme",
                "/k-scheme/x",
                rd(|r| r.scheme_redirect = Some("ftp".into())),
                301,
                "ftp://envoy-rust.test/k-scheme/x",
            ),
            cell(
                "R12 scheme_redirect + host_redirect together",
                "envoy-rust.test",
                "/l-both",
                "/l-both/y",
                rd(|r| {
                    r.scheme_redirect = Some("https".into());
                    r.host_redirect = Some("e.com".into());
                }),
                301,
                "https://e.com/l-both/y",
            ),
            cell(
                "R13 response_code SEE_OTHER + strip_query",
                "envoy-rust.test",
                "/m-see",
                "/m-see/y?q=1",
                rd(|r| {
                    r.host_redirect = Some("e.com".into());
                    r.strip_query = true;
                    r.response_code = RC::SeeOther;
                }),
                303,
                "http://e.com/m-see/y",
            ),
            cell(
                "R14 a scheme change does NOT normalise a redundant :443",
                "envoy-rust.test",
                "/n-hport",
                "/n-hport/y",
                rd(|r| {
                    r.https_redirect = Some(true);
                    r.port_redirect = Some(443);
                }),
                301,
                "https://envoy-rust.test:443/n-hport/y",
            ),
            cell(
                "R15 response_code FOUND",
                "envoy-rust.test",
                "/o-found",
                "/o-found",
                rd(|r| {
                    r.host_redirect = Some("example.com".into());
                    r.response_code = RC::Found;
                }),
                302,
                "http://example.com/o-found",
            ),
            cell(
                "R16 response_code PERMANENT_REDIRECT",
                "envoy-rust.test",
                "/p-perm",
                "/p-perm",
                rd(|r| {
                    r.host_redirect = Some("example.com".into());
                    r.response_code = RC::PermanentRedirect;
                }),
                308,
                "http://example.com/p-perm",
            ),
            cell(
                "Q1 host_redirect UNSET preserves the request's port",
                "envoy-rust.test:1234",
                "/q1-hostport",
                "/q1-hostport/x",
                rd(|r| r.https_redirect = Some(true)),
                301,
                "https://envoy-rust.test:1234/q1-hostport/x",
            ),
            cell(
                "Q2 host_redirect SET DROPS the request's port — the asymmetry",
                "envoy-rust.test:1234",
                "/a-host",
                "/a-host",
                rd(|r| r.host_redirect = Some("example.com".into())),
                301,
                "http://example.com/a-host",
            ),
            cell(
                "Q3 a bare redirect{} preserves the request's port",
                "envoy-rust.test:1234",
                "/q3-hostport",
                "/q3-hostport/d",
                RedirectAction::default(),
                301,
                "http://envoy-rust.test:1234/q3-hostport/d",
            ),
            cell(
                "Q4 port_redirect OVERRIDES the request's port",
                "envoy-rust.test:1234",
                "/n-hport",
                "/n-hport/y",
                rd(|r| {
                    r.https_redirect = Some(true);
                    r.port_redirect = Some(443);
                }),
                301,
                "https://envoy-rust.test:443/n-hport/y",
            ),
            cell(
                "E1 an explicit https_redirect:false is the DEFAULT scheme",
                "envoy-rust.test",
                "/y-hfalse",
                "/y-hfalse/z",
                rd(|r| r.https_redirect = Some(false)),
                301,
                "http://envoy-rust.test/y-hfalse/z",
            ),
            cell(
                "E2 an EMPTY path_redirect performs NO rewrite",
                "envoy-rust.test",
                "/x-emptypath",
                "/x-emptypath/z",
                rd(|r| r.path_redirect = Some(String::new())),
                301,
                "http://envoy-rust.test/x-emptypath/z",
            ),
        ];
        assert_eq!(cells.len(), 22, "all 22 MEASURED cells must be present");
        for c in &cells {
            let plan = plan_redirect(c.host, c.target, c.prefix, &c.rd);
            assert_eq!(plan.location, c.location, "cell {}: location", c.label);
            assert_eq!(plan.status, c.status, "cell {}: status", c.label);
        }
    }

    /// 76.2 T3-2: `prefix_rewrite` is the ONLY arm that reports a rewritten
    /// `:path`. MEASURED: `prefix_rewrite` MUTATES the logged `:path` while
    /// `path_redirect` does NOT.
    #[test]
    fn plan_redirect_reports_a_rewritten_path_only_for_prefix_rewrite() {
        let mut pfx = RedirectAction::default();
        pfx.prefix_rewrite = Some("/replaced".into());
        assert_eq!(
            plan_redirect("h.test", "/e-pfx/sub", Some("/e-pfx"), &pfx).rewritten_path,
            Some("/replaced/sub".to_string()),
        );

        let mut pathr = RedirectAction::default();
        pathr.path_redirect = Some("/newpath".into());
        assert_eq!(
            plan_redirect("h.test", "/c-pathr/sub", Some("/c-pathr"), &pathr).rewritten_path,
            None,
            "path_redirect must NOT rewrite the request's own :path"
        );

        assert_eq!(
            plan_redirect("h.test", "/j-bare/x", Some("/j-bare"), &RedirectAction::default())
                .rewritten_path,
            None,
            "a bare redirect{} rewrites nothing"
        );
    }

    /// 76.2 T3-3: `plan_redirect` is TOTAL — it must not panic on a matched
    /// span longer than the path, nor on one landing off a UTF-8 boundary.
    #[test]
    fn plan_redirect_is_total_on_degenerate_spans() {
        let mut rd = RedirectAction::default();
        rd.prefix_rewrite = Some("/r".into());
        // Matched span longer than the path.
        assert_eq!(
            plan_redirect("h.test", "/ab", Some("/abcdefgh"), &rd).location,
            "http://h.test/r"
        );
        // Matched span landing mid-codepoint.
        assert_eq!(
            plan_redirect("h.test", "/é", Some("/é"[..2].into()), &rd).location,
            "http://h.test/r"
        );
    }
```

- [ ] **Step 2: Run it RED**

Run: `cargo test -p envoy-http1 --lib -- plan_redirect`
Expected: **FAIL to compile**, `cannot find function `plan_redirect` in this scope`.
(This is a genuine RED — the function does not exist. Task 2 already removed the *import*
ambiguity, so this error can only mean the function is missing.)

- [ ] **Step 3: Implement — verbatim from the pre-flight**

Insert into `crates/envoy-http1/src/hcm.rs` after the `synth_status` block. This is the exact text
that passed `cargo fmt --check` and `clippy -D warnings` in §2:

```rust
/// 76.2 (SPEC 2.4): the pure, total outcome of applying a `RedirectAction` to
/// one request. Produced by [`plan_redirect`] with no I/O, so every measured
/// upstream cell is unit-testable without a socket.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RedirectPlan {
    /// The `location:` header value.
    pub location: String,
    /// The wire status, from `RedirectResponseCode::status()`.
    pub status: u16,
    /// `Some(new_path)` exactly when `prefix_rewrite` applied. The dispatch arm
    /// writes it back into `req.path` so the access log observes the rewrite —
    /// MEASURED: `prefix_rewrite` MUTATES the logged `:path` while
    /// `path_redirect` does NOT.
    pub rewritten_path: Option<String>,
}

/// 76.2 (SPEC 2.4): build the redirect plan from the MEASURED upstream rules
/// (a)-(e). Pure and total — it never panics and never touches the network.
///
/// * `authority` — the request's `Host:` header VERBATIM, port included. The
///   authority in `location` comes from that header, NOT from the socket
///   (MEASURED: a `Host:` port differing from the listen port is echoed).
/// * `target` — the raw request target, query included.
/// * `matched_prefix` — the matched route's `match.prefix`, the span that
///   `prefix_rewrite` replaces. `None` (a `path:`-matched route) means the
///   whole path is the matched span.
fn plan_redirect(
    authority: &str,
    target: &str,
    matched_prefix: Option<&str>,
    rd: &RedirectAction,
) -> RedirectPlan {
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (target, None),
    };

    // (a) Scheme. `scheme_redirect` wins and is NOT validated against any
    // allow-list (MEASURED: the literal `ftp` is accepted and emitted
    // verbatim); else `https_redirect: true` forces https; else the scheme the
    // request arrived on. An explicit `https_redirect: false` is the default.
    let scheme = match (rd.scheme_redirect.as_deref(), rd.https_redirect) {
        (Some(s), _) => s,
        (None, Some(true)) => "https",
        (None, _) => "http",
    };

    // (b) Authority — the asymmetry, and the trap. `host_redirect` SET makes
    // the authority that host and DROPS the request's original port;
    // `host_redirect` UNSET preserves the request's authority INCLUDING its
    // port. `port_redirect` overrides the port in BOTH cases and is rendered
    // verbatim with no range clamp (MEASURED: upstream accepts `70000` and
    // emits `:70000`), and a scheme-only change does NOT normalise a now
    // redundant `:443`.
    let host_part = rd.host_redirect.as_deref().unwrap_or(authority);
    let authority_out = match rd.port_redirect {
        Some(port) => format!("{}:{}", strip_port(host_part), port),
        None => host_part.to_string(),
    };

    // (c) Path. The two rewrites are mutually exclusive (rejected at load by
    // the 76.1 oneof validator), and an EMPTY `path_redirect` performs NO
    // rewrite — MEASURED: `path_redirect: ""` leaves the original path.
    let mut rewritten_path = None;
    let new_path = match (
        rd.path_redirect.as_deref().filter(|p| !p.is_empty()),
        rd.prefix_rewrite.as_deref(),
    ) {
        (Some(p), _) => p.to_string(),
        (None, Some(pr)) => {
            // `get(..).unwrap_or("")` keeps the function TOTAL: a matched span
            // longer than the path, or one landing off a UTF-8 boundary, yields
            // an empty tail instead of panicking.
            let matched_len = matched_prefix.map_or(path.len(), str::len);
            let rewritten = format!("{}{}", pr, path.get(matched_len..).unwrap_or(""));
            // The request's own query rides along on the rewritten `:path`;
            // `strip_query` is a location-side rule only.
            rewritten_path = Some(match query {
                Some(q) => format!("{rewritten}?{q}"),
                None => rewritten.clone(),
            });
            rewritten
        }
        (None, None) => path.to_string(),
    };

    // (d) Query. Preserved by default even when `path_redirect` replaced the
    // path wholesale; `strip_query: true` drops it.
    let query_suffix = match (rd.strip_query, query) {
        (false, Some(q)) => format!("?{q}"),
        _ => String::new(),
    };

    RedirectPlan {
        location: format!("{scheme}://{authority_out}{new_path}{query_suffix}"),
        // (e) Status. Default 301; the five `response_code` values map through
        // the 76.1 `RedirectResponseCode::status()` table.
        status: rd.response_code.status(),
        rewritten_path,
    }
}
```

- [ ] **Step 4: Run GREEN**

Run: `cargo test -p envoy-http1 --lib -- plan_redirect`
Expected: **`3 passed; 0 failed`**. Assert on the **count**, never on the exit code —
`0 passed; N filtered out` is a false green.

- [ ] **Step 5: Mutation check — prove the authority asymmetry is really pinned**

The single most likely from-scratch mistake is treating `host_redirect` symmetrically with the
scheme change. In a **scratch worktree detached at the current commit** (never in the main tree —
a parallel agent's `git checkout` can silently revert an in-place mutation), change

```rust
    let host_part = rd.host_redirect.as_deref().unwrap_or(authority);
```

to `let host_part = authority;`. Re-run Step 4 forcing a rebuild and grep for
`Compiling envoy-http1` to prove the binary is not stale. Expected: **RED**, naming
`cell R1 host_redirect replaces the authority: location`. Then run the **unmutated control** from
the same worktree and confirm GREEN — a RED that never reached an assertion is not evidence.
Re-grep the mutation as still present afterwards, then remove the worktree.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 76.2 task 3: the pure location-builder — all 22 MEASURED cells pinned"
```

---

## Task 4: `synth_redirect` — the dedicated response builder

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` — insert immediately after `plan_redirect`
- Test: `crates/envoy-http1/src/hcm.rs` `mod tests`

**Interfaces:**
- Consumes: `headers::LOCATION` (Task 1), `now_imf_fixdate()`, `connection_value()`,
  `DEFAULT_SERVER_NAME` — all already in `hcm.rs`.
- Produces: `fn synth_redirect(status: u16, location: String, close: bool) -> Response`.
  Task 5 consumes it.

**Why a dedicated builder and not `synth_with`.** MEASURED under the harness's exact request shape
(a raw `GET <target> HTTP/1.1` with `Host:` and `Connection: close`):

| response | headers, in wire order |
|---|---|
| **redirect** | `location`, `date`, `server`, `connection`, `content-length` |
| `direct_response` (control) | `content-length`, `content-type`, `date`, `server`, `connection` |

**A redirect carries NO `content-type`. A `direct_response` does.** The shared `synth_with`
**always** emits five headers including `content-type`. If the redirect arm reused it, `diff_headers`
would fail on its **first** check — the lowercased name-set equality — with
`only-in-envoy-rust=["content-type"]`, and the whole fixture would be red for a reason unrelated
to `location`. `synth_overflow` is the established precedent for a synth path owning its own
header list.

- [ ] **Step 1: Write the failing test**

```rust
    /// 76.2 T4-1: a redirect carries EXACTLY five header names and NO
    /// `content-type` — the MEASURED finding that forces a dedicated builder.
    /// Reusing the shared `synth_with` would emit a sixth header upstream does
    /// not, and `diff_headers` would bail on its name-set check with
    /// `only-in-envoy-rust=["content-type"]`.
    #[test]
    fn synth_redirect_emits_five_names_and_no_content_type() {
        let resp = synth_redirect(301, "http://example.com/a".to_string(), true);
        let names: Vec<&str> = resp.headers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec!["location", "date", "server", "connection", "content-length"],
            "measured upstream wire order for a redirect response"
        );
        assert!(
            !names.contains(&"content-type"),
            "a redirect MUST NOT carry content-type"
        );
        assert!(resp.body.is_empty(), "redirect body is empty");
        assert_eq!(
            resp.headers
                .iter()
                .find(|(n, _)| n == "content-length")
                .map(|(_, v)| v.as_str()),
            Some("0"),
            "content-length is compared value-exact by diff_headers"
        );
        assert_eq!(
            resp.headers
                .iter()
                .find(|(n, _)| n == "location")
                .map(|(_, v)| v.as_str()),
            Some("http://example.com/a"),
        );
        assert_eq!(resp.status, 301);
    }
```

- [ ] **Step 2: Run it RED**

Run: `cargo test -p envoy-http1 --lib -- synth_redirect_emits_five_names`
Expected: **FAIL to compile**, `cannot find function `synth_redirect``.

- [ ] **Step 3: Implement — verbatim from the pre-flight**

```rust
/// 76.2: the redirect response builder. MEASURED against
/// `envoyproxy/envoy:v1.33.0`: a redirect carries EXACTLY `location`, `date`,
/// `server`, `connection`, `content-length` — and NO `content-type`, which a
/// `direct_response` DOES carry. It therefore must NOT reuse [`synth_with`],
/// whose fixed 5-header list always emits `content-type`; doing so fails the
/// harness's `diff_headers` name-set check with
/// `only-in-envoy-rust=["content-type"]`. Header ORDER matches the measured
/// upstream wire order. Body is empty, `content-length: 0`.
fn synth_redirect(status: u16, location: String, close: bool) -> Response {
    Response {
        status,
        reason: None,
        headers: vec![
            (headers::LOCATION.to_string(), location),
            (headers::DATE.to_string(), now_imf_fixdate()),
            (headers::SERVER.to_string(), DEFAULT_SERVER_NAME.to_string()),
            (
                headers::CONNECTION.to_string(),
                connection_value(close).to_string(),
            ),
            (headers::CONTENT_LENGTH.to_string(), "0".to_string()),
        ],
        body: Bytes::new(),
    }
}
```

- [ ] **Step 4: Run GREEN**

Run: `cargo test -p envoy-http1 --lib -- synth_redirect`
Expected: **`1 passed`**.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 76.2 task 4: synth_redirect — five headers, no content-type"
```

---

## Task 5: the real dispatch arm + `&mut Request` + flipping T-C9

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` — `build_response` signature (`:2039`),
  `build_response_in` signature (`:2051-2055`), the call site at `:919`, the `Redirect` arm at
  `:2114-2121`, three in-file test call sites (`:9734`, `:9761`, `:9778`)
- Modify: `crates/envoy-http1/src/uring.rs:287`
- Modify: `crates/envoy-http2/src/hcm.rs:518`
- Test: `crates/envoy-http1/src/hcm.rs` `mod tests` — rewrite `76.1`'s T-C9

**Interfaces:**
- Consumes: `plan_redirect` (Task 3), `synth_redirect` (Task 4).
- Produces: `build_response(config: &HCMConfig, req: &mut Request, close: bool) -> BuildOutcome`
  and `build_response_in(route_config: &Arc<RouteConfiguration>, req: &mut Request, close: bool)`.
  Task 6 and Task 7 consume the new signature.

**Why `&mut Request`.** The access-log record takes its path from
`x_envoy_original_path_or_path(request.req)` inside `build_access_log_record`, which runs **after**
`build_response_in` returns. So for `prefix_rewrite`'s MEASURED `:path` mutation to be observable,
the rewrite must land in `req.path` itself.

**The complete call-site list — MEASURED, 7 sites** (the SPEC's "8" is REFUTED; it counted the two
definitions):

| site | current | becomes |
|---|---|---|
| `hcm.rs:919` | `build_response_in(&route_snapshot, &req, close)` | `&mut req` (`req` is already a `mut` binding at `:859`) |
| `hcm.rs:2045` | `build_response_in(&config.current_route_config(), req, close)` | unchanged text — `req` is now `&mut Request` and reborrows |
| `hcm.rs:9734` | `match build_response(&config, &req, true)` | `&mut req`; change `let req =` → `let mut req =` |
| `hcm.rs:9761` | same | same |
| `hcm.rs:9778` | same | same |
| `uring.rs:287` | `crate::hcm::build_response(&config, &req, close)` | `&mut req` (`req` is already `mut` — it is assigned at `:278`) |
| `envoy-http2/src/hcm.rs:518` | `build_response(&config.inner, &envoy_req, false)` | `&mut envoy_req` |

**Borrow-checker note (MEASURED — the pre-flight compiled clean).** In H2, `envoy_req` is
`mem::take`-emptied into a `FilterRequest` at `:490-492` and written back before `:518`, and
`matched_route` borrows `config.inner` at `:475`. Taking `&mut envoy_req` only at `:518`, after
those borrows end, compiles. In the H1 arm, re-reading `Host` into an **owned** `String` inside the
arm ends the immutable borrow of `req.headers` before the `req.path` write-back — do it that way
rather than reusing the outer `host_raw` binding.

- [ ] **Step 1: Rewrite `76.1`'s T-C9 as the failing test**

`76.1` landed `build_response_redirect_is_not_implemented_placeholder`, whose doc block says
verbatim **"76.2 MUST flip this test."** Flip it **deliberately, by rewriting it**, not by deleting
it. This also closes **N-3**: `76.1` attached that doc block to the `redirect_placeholder_config`
*helper* (`hcm.rs:9689-9695`) rather than to the test (`:9731`). Move the (rewritten) doc block
onto the test and leave the helper a plain one-line doc.

Replace the helper's doc block at `hcm.rs:9689-9695` with:

```rust
    /// Config fixture for the redirect dispatch tests: one `prefix: "/"` route
    /// whose action is `RouteAction::Redirect` with `https_redirect: true`.
```

Replace the whole test `build_response_redirect_is_not_implemented_placeholder` (`:9730-9748`)
with:

```rust
    /// 76.2 T5-1: THE DELIBERATE FLIP of 76.1's T-C9.
    ///
    /// 76.1 shipped the `redirect:` CONFIG surface with an honest `synth_501`
    /// not-implemented placeholder at the dispatch arm, pinned by a test named
    /// `build_response_redirect_is_not_implemented_placeholder` whose doc block
    /// said "76.2 MUST flip this test". This is that flip: the arm now serves a
    /// real 301 carrying a `location:` header. The rename is the point — the
    /// replacement is a visible, named change rather than an unobserved
    /// behaviour shift.
    ///
    /// Also pins the access-log observable: `%RESPONSE_CODE_DETAILS%` for a
    /// redirect is `direct_response` (MEASURED — the SAME string upstream uses
    /// for a `direct_response:` route, and the same bare literal envoy-rust
    /// already emits), so 76.2 adds NO new detail string, `Op` or
    /// `AccessLogRecord` field.
    #[tokio::test]
    async fn build_response_redirect_emits_301_and_location() {
        let config = redirect_placeholder_config().await;
        let mut req = make_req("/foo", "localhost");
        match build_response(&config, &mut req, true) {
            BuildOutcome::Synth(resp, detail) => {
                assert_eq!(resp.status, 301, "the default response_code is 301");
                assert_eq!(
                    resp.headers
                        .iter()
                        .find(|(n, _)| n == "location")
                        .map(|(_, v)| v.as_str()),
                    Some("https://localhost/foo"),
                    "https_redirect:true forces the scheme; the authority comes \
                     from the Host header"
                );
                assert!(
                    !resp.headers.iter().any(|(n, _)| n == "content-type"),
                    "a redirect MUST NOT carry content-type"
                );
                assert_eq!(
                    detail,
                    Some("direct_response"),
                    "MEASURED: %RESPONSE_CODE_DETAILS% for a redirect is \
                     `direct_response`"
                );
            }
            _other => panic!("expected BuildOutcome::Synth(301)"),
        }
    }
```

- [ ] **Step 2: Run it RED**

Run: `cargo test -p envoy-http1 --lib -- build_response_redirect_emits_301_and_location`
Expected: **FAIL**. The compile fails first on `&mut req` (the signature is still `&Request`); after
Step 3's signature change it fails on the assertion with `left: 501, right: 301`. **MEASURED in the
pre-flight: `left: 301, right: 501` on the old test — the same evidence from the other side.**

- [ ] **Step 3: Widen both signatures and all 7 call sites**

```rust
pub fn build_response(config: &HCMConfig, req: &mut Request, close: bool) -> BuildOutcome {
```

```rust
pub(crate) fn build_response_in(
    route_config: &Arc<RouteConfiguration>,
    req: &mut Request,
    close: bool,
) -> BuildOutcome {
```

`hcm.rs:919`: `build_response_in(&route_snapshot, &mut req, close)`
`uring.rs:287`: `crate::hcm::build_response(&config, &mut req, close)`
`envoy-http2/src/hcm.rs:518-522`:

```rust
            H2RequestPath::Match(build_response(
                &config.inner,
                &mut envoy_req,
                /* close = */ false,
            ))
```

And at each of `hcm.rs:9734`, `:9761`, `:9778`, change `let req = make_req(…)` to
`let mut req = make_req(…)` and `&req` to `&mut req`.

- [ ] **Step 4: Implement the real arm**

Replace the `76.1` placeholder arm (`hcm.rs:2114-2121`, the seven comment lines plus
`RouteAction::Redirect(_) => BuildOutcome::Synth(synth_501(close), None),`) with — verbatim from
the pre-flight:

```rust
        // 76.2: the REAL redirect arm, replacing 76.1's honest `synth_501`
        // placeholder (ADR-0169 DECISION 4). ONE arm serves BOTH codecs — H2 has
        // no route-action dispatch of its own and calls this function.
        RouteAction::Redirect(rd) => {
            // The authority comes from the `Host:` header VERBATIM (port
            // included), NOT from the socket. Re-read it as an OWNED string so
            // the immutable borrow of `req.headers` ends before the `req.path`
            // write-back below.
            let authority = find_header(&req.headers, headers::HOST)
                .unwrap_or_default()
                .to_string();
            let plan = plan_redirect(&authority, &req.path, route.r#match.prefix.as_deref(), rd);
            // MEASURED: `prefix_rewrite` MUTATES the logged `:path` while
            // `path_redirect` does NOT. `build_access_log_record` reads
            // `req.path` AFTER this function returns, so the rewrite must land
            // in the request itself.
            if let Some(new_path) = plan.rewritten_path {
                req.path = new_path;
            }
            // MEASURED: `%RESPONSE_CODE_DETAILS%` for a redirect is
            // `direct_response` — the SAME bare literal the arm above emits. No
            // new detail string, `Op` or `AccessLogRecord` field is needed.
            BuildOutcome::Synth(
                synth_redirect(plan.status, plan.location, close),
                Some("direct_response"),
            )
        }
```

> `synth_501` remains in use by the chunked-`Transfer-Encoding` path at `hcm.rs:915`, so removing
> this arm does **not** make it dead code. Do not delete it.

- [ ] **Step 5: Run GREEN — including the whole H1/H2/bin regression surface**

```bash
cargo test -p envoy-http1 --lib --no-fail-fast > /tmp/t-h1.txt 2>&1; echo "exit=$?"
cargo test -p envoy-http2 --lib --no-fail-fast > /tmp/t-h2.txt 2>&1; echo "exit=$?"
grep -E 'test result:' /tmp/t-h1.txt /tmp/t-h2.txt
```

Expected: `test result: ok.` on both, with the H1 count up by the tests added in Tasks 1, 3, 4, 5.
**Redirect output to a file and read it — never pipe a verification run through `tail`**, which
truncates the `failures:` block. If a RED appears, census the failing tests from the
`---- <name> stdout ----` markers, never by indentation (that invents phantom test names).

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs crates/envoy-http1/src/uring.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 76.2 task 5: the real redirect dispatch arm + &mut Request; T-C9 deliberately flipped"
```

---

## Task 6: the `prefix_rewrite` in-place `:path` mutation pins

**Files:**
- Test: `crates/envoy-http1/src/hcm.rs` `mod tests` (no implementation — Task 5 already wrote the
  mutation; this task proves it and pins the asymmetry)

**Interfaces:**
- Consumes: `build_response` with the Task 5 signature; the `redirect_placeholder_config` helper
  pattern.

**The observable being pinned.** MEASURED via
`text_format: "PROBE path=%REQ(:PATH)% …"` against upstream: request `/e-pfx/sub` on a
`prefix_rewrite: "/replaced"` route is logged as `path=/replaced/sub`, while `/c-pathr/sub` on a
`path_redirect: "/newpath"` route is logged **unchanged**. That asymmetry is a real discriminating
observable and a parity trap — and it is invisible to fixture `0086`, which compares responses, not
logs.

- [ ] **Step 1: Write the failing tests**

```rust
    /// Config fixture for the `:path`-mutation tests: ONE route, `prefix`- or
    /// `path`-matched as the caller chooses, whose action is a redirect built
    /// from `rd`.
    async fn redirect_route_config(prefix: &str, rd: RedirectAction) -> HCMConfig {
        HCMConfig {
            stat_prefix: "test".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("test"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some(prefix.to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::Redirect(rd),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
        }
    }

    /// 76.2 T6-1: `prefix_rewrite` MUTATES the request's `:path` in place, so
    /// the access-log record — built from `req.path` AFTER `build_response`
    /// returns — observes the rewrite. MEASURED upstream: request
    /// `/e-pfx/sub` on a `prefix_rewrite: "/replaced"` route logs as
    /// `path=/replaced/sub`.
    #[tokio::test]
    async fn build_response_prefix_rewrite_mutates_the_request_path() {
        let mut rd = RedirectAction::default();
        rd.prefix_rewrite = Some("/replaced".into());
        let config = redirect_route_config("/e-pfx", rd).await;
        let mut req = make_req("/e-pfx/sub", "envoy-rust.test");
        let outcome = build_response(&config, &mut req, true);
        assert!(matches!(outcome, BuildOutcome::Synth(ref r, _) if r.status == 301));
        assert_eq!(
            req.path, "/replaced/sub",
            "prefix_rewrite must rewrite the request's own :path in place"
        );
    }

    /// 76.2 T6-2: the OTHER HALF of the asymmetry — `path_redirect` changes the
    /// `location` only and MUST NOT touch the request's `:path`. MEASURED
    /// upstream: `/c-pathr/sub` is logged unchanged.
    #[tokio::test]
    async fn build_response_path_redirect_leaves_the_request_path_alone() {
        let mut rd = RedirectAction::default();
        rd.path_redirect = Some("/newpath".into());
        let config = redirect_route_config("/c-pathr", rd).await;
        let mut req = make_req("/c-pathr/sub", "envoy-rust.test");
        match build_response(&config, &mut req, true) {
            BuildOutcome::Synth(resp, _) => {
                assert_eq!(
                    resp.headers
                        .iter()
                        .find(|(n, _)| n == "location")
                        .map(|(_, v)| v.as_str()),
                    Some("http://envoy-rust.test/newpath"),
                );
            }
            _other => panic!("expected BuildOutcome::Synth"),
        }
        assert_eq!(
            req.path, "/c-pathr/sub",
            "path_redirect must NOT touch the request's :path"
        );
    }
```

> The four helpers `cluster_mgr_empty`, `mk_stats`, `test_router_only_pipeline` and `make_req`
> already exist in this test module — `redirect_placeholder_config` (`hcm.rs:9696`) uses all four.
> Copy its field list rather than inventing one.

- [ ] **Step 2: Run it RED**

Before running, **temporarily** comment out the two lines in Task 5's arm:

```rust
            if let Some(new_path) = plan.rewritten_path {
                req.path = new_path;
            }
```

Run: `cargo test -p envoy-http1 --lib -- build_response_prefix_rewrite_mutates`
Expected: **FAIL**, `left: "/e-pfx/sub"  right: "/replaced/sub"`.
**This is the RED evidence.** The mutation must be done in a scratch worktree, not the main tree,
and re-grepped as still present after the run. Restore the lines afterwards.

- [ ] **Step 3: Run GREEN**

Restore the two lines. Run:
`cargo test -p envoy-http1 --lib -- build_response_prefix_rewrite build_response_path_redirect`
Expected: **`2 passed`**.

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 76.2 task 6: pin the prefix_rewrite :path mutation and the path_redirect non-mutation"
```

---

## Task 7: the HTTP/2 shared-seam in-process test

**Files:**
- Test: `crates/envoy-http2/src/hcm.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `envoy_http1::build_response` (already imported at `envoy-http2/src/hcm.rs:18`),
  `envoy_config::{RedirectAction, RouteAction, …}`.

**What this proves, and what it does not.** HTTP/2 has **no** route-action dispatch of its own —
CONFIRMED, not assumed: a grep over `crates/envoy-http2/` for `RouteAction` / `route.action`
returns 38 hits of which **zero** are a dispatch (35 are `RouteAction::…` route-table literals
inside `#[cfg(test)]` fixtures, 3 are comments/imports), and there is no `match … action` anywhere
in the crate. It calls H1's resolver at `:475` and H1's `build_response` at `:518`. So the one arm
Task 5 added serves both codecs — and a bug there hits both. This test pins that the seam is
reachable from the H2 crate and returns a real redirect. It is the disposition phases 68 and 69
took; **an HTTP/2 differential fixture is an explicit non-goal** (SPEC §7 item 4). It does **not**
prove upstream H2 parity — upstream's H2 `:scheme`/`:authority` handling was never probed
(SPEC §8 item 2).

Model it on the existing `h2_resolve_route_reachable_and_returns_cors_route`
(`envoy-http2/src/hcm.rs:6501`), which already builds an `Http1HCMConfig` inside this crate's test
module and calls across the crate boundary.

- [ ] **Step 1: Write the failing test**

```rust
    /// 76.2 T7-1: the SHARED SEAM. HTTP/2 has no route-action dispatch of its
    /// own — it calls `envoy_http1::hcm::build_response` at `:518` — so the
    /// single redirect arm added at 76.2 serves BOTH codecs, and a bug there
    /// hits both. This pins that the seam is reachable from the H2 crate and
    /// returns a real 301 with a `location:` header.
    ///
    /// This is envoy-rust's OWN seam, not upstream parity: upstream's H2
    /// `:scheme`/`:authority` handling was never probed (SPEC 8 item 2), and an
    /// H2 differential fixture is an explicit non-goal (SPEC 7 item 4).
    #[tokio::test]
    async fn h2_shared_seam_serves_the_redirect_arm() {
        use envoy_config::{RedirectAction, RouteAction, RouteMatch};

        let mut rd = RedirectAction::default();
        rd.host_redirect = Some("example.com".to_string());

        let h1cfg = h2_redirect_h1_config(rd).await;
        let mut req = envoy_http1::Request {
            method: "GET".to_string(),
            path: "/a-host".to_string(),
            version: envoy_http1::codec::HttpVersion::Http2,
            headers: vec![("host".to_string(), "envoy-rust.test".to_string())],
            bytes_consumed: 0,
            body: None,
        };
        match build_response(&h1cfg, &mut req, false) {
            BuildOutcome::Synth(resp, detail) => {
                assert_eq!(resp.status, 301);
                assert_eq!(
                    resp.headers
                        .iter()
                        .find(|(n, _)| n == "location")
                        .map(|(_, v)| v.as_str()),
                    Some("http://example.com/a-host"),
                    "H2 must get the identical location H1 gets — one arm, both codecs"
                );
                assert_eq!(detail, Some("direct_response"));
            }
            _other => panic!("expected BuildOutcome::Synth(301) from the shared seam"),
        }
        let _ = (RouteAction::Redirect(RedirectAction::default()), RouteMatch {
            prefix: Some("/".to_string()),
            path: None,
            headers: vec![],
        });
    }
```

> **Transcription note for the executor.** The exact `Request` construction and the
> `h2_redirect_h1_config` helper must be modelled on whatever
> `h2_resolve_route_reachable_and_returns_cors_route` (`:6501`, helper config built around `:6525`)
> already does in this module — **read that test first and copy its shape**, including how it
> builds `Http1HCMConfig` (stats, cluster manager, filter pipeline) and how it names the
> `HttpVersion`. The trailing `let _ = (…)` line above is scaffolding to keep the imports used;
> **delete it** once the helper is written and the imports are genuinely consumed. If the crate's
> `Request` re-export path differs, follow the existing test, not this literal.
>
> This is the one task in this plan whose literal Rust was **NOT** pre-flighted end-to-end (it
> depends on a helper that does not exist yet). Budget an extra compile-fix cycle, and run
> `cargo fmt --all -- --check` + `clippy -D warnings` before committing.

- [ ] **Step 2: Run it RED**

Run: `cargo test -p envoy-http2 --lib -- h2_shared_seam_serves_the_redirect_arm`
Expected: **FAIL to compile** (helper missing), then **FAIL** on the assertion until Task 5 is in
place. If Task 5 is already committed, the test should go green as soon as it compiles — in that
case honour TDD's RED by the Task 3 mutation (`host_part = authority`) in a scratch worktree and
record THAT as the RED evidence, per the standing §5.2 re-entry discipline.

- [ ] **Step 3: Write the helper, run GREEN**

Run: `cargo test -p envoy-http2 --lib -- h2_shared_seam`
Expected: **`1 passed`**.

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 76.2 task 7: pin that the shared dispatch seam serves HTTP/2"
```

---

## Task 8: CF-76-2 — the RDS warm path re-validates the redirect oneofs

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` — the `RouteAction::Redirect(rd)` arm at
  `:4073-4088`; new `validate_redirect_oneofs` inserted immediately **above** the
  `#[derive(…)] #[serde(deny_unknown_fields)] pub struct RouteMatch {` block (`:2657-2659`)
- Modify: `crates/envoy-config/src/rds.rs:133-141`
- Test: `crates/envoy-config/src/rds.rs` `#[cfg(test)] mod tests` (`:146`)

**Interfaces:**
- Produces: `pub(crate) fn validate_redirect_oneofs(rd: &RedirectAction, context: &str, route:
  &str) -> Result<(), crate::ConfigError>`, reachable from `rds.rs` as
  `crate::bootstrap::validate_redirect_oneofs`.

**Rationale and scope boundary: see §5 above.** In one line: `76.1` joined an
ADR-0028-sanctioned hole with NIL blast radius because the runtime arm was inert; **Task 5 removes
that inertness**, so the hole must close for the arm that is no longer inert — and only that arm.

**On the `ConfigError` field name.** Both variants carry `{ listener: String, route: String }`.
On the RDS path there is no listener, so the `context` argument is passed as `rds:<path>`. The
resulting message reads ``redirect action on listener `rds:/etc/envoy/rds.yaml` route `` — mildly
loose wording, accepted deliberately rather than minting a 126th `ConfigError` variant for a
context string. **Error TEXT is not part of the equivalence contract** (only the verdict is), so
this costs nothing differentially. Record it in `PROGRESS.md`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/envoy-config/src/rds.rs`'s `mod tests`. Model the RDS-file fixture on the
existing `// A minimal working RDS file:` helper already in that module (`:149`).

```rust
    /// 76.2 (CF-76-2) T8-1: an RDS HOT RELOAD carrying a mutually-exclusive
    /// `redirect:` oneof pair must be WARM-REJECTED, exactly as the
    /// byte-identical config is BOOT-fatal.
    ///
    /// 76.1 landed the `Redirect` variant while this path still used an
    /// `if let RouteAction::Route(..)`, so the new variant tripped no compile
    /// error and the pair was accepted warm and installed LIVE. That was
    /// adjudicated MINOR at 76.1 only because the runtime arm was an inert 501;
    /// 76.2 makes it serve a real 3xx, so the hole closes here.
    #[test]
    fn rds_reload_rejects_a_conflicting_redirect_oneof() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rds.yaml");
        std::fs::write(
            &path,
            r#"resources:
  - "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
    name: local_route
    virtual_hosts:
      - name: default
        domains: ["*"]
        routes:
          - match: { prefix: "/r" }
            redirect: { path_redirect: "/p", prefix_rewrite: "/q" }
"#,
        )
        .expect("write");
        let err = reparse_and_select_route_config(&path, "local_route", &|_| true)
            .expect_err("a conflicting redirect oneof must be warm-rejected");
        assert!(
            matches!(err, crate::ConfigError::RedirectPathRewriteConflict { .. }),
            "expected RedirectPathRewriteConflict, got {err:?}"
        );
    }

    /// 76.2 (CF-76-2) T8-2: the ACCEPT direction — a VALID `redirect:` route
    /// still reloads warm. Without this, T8-1 would pass just as well if the
    /// path rejected every redirect.
    #[test]
    fn rds_reload_accepts_a_valid_redirect_route() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rds.yaml");
        std::fs::write(
            &path,
            r#"resources:
  - "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
    name: local_route
    virtual_hosts:
      - name: default
        domains: ["*"]
        routes:
          - match: { prefix: "/r" }
            redirect: { host_redirect: "example.com" }
"#,
        )
        .expect("write");
        let rc = reparse_and_select_route_config(&path, "local_route", &|_| true)
            .expect("a valid redirect route must reload warm");
        assert_eq!(rc.virtual_hosts.len(), 1);
        assert!(matches!(
            rc.virtual_hosts[0].routes[0].action,
            crate::RouteAction::Redirect(_)
        ));
    }
```

> Check how the existing RDS tests obtain a temp path before transcribing `tempfile::tempdir()`;
> if the module already has its own helper, use that instead. **Do not add a new dev-dependency.**

- [ ] **Step 2: Run it RED**

Run: `cargo test -p envoy-config --lib -- rds_reload_rejects_a_conflicting_redirect_oneof`
Expected: **FAIL** — `a conflicting redirect oneof must be warm-rejected`, because the current
`if let` skips the `Redirect` arm entirely. **This RED is CF-76-2 reproduced.**

- [ ] **Step 3: Lift the shared validator**

In `crates/envoy-config/src/bootstrap.rs`, replace the `Redirect` arm body (`:4073-4088`) with:

```rust
                RouteAction::Redirect(rd) => {
                    validate_redirect_oneofs(rd, listener_name, &r.name)?;
                }
```

and insert immediately **above** the `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]`
that precedes `pub struct RouteMatch`:

```rust
/// 76.1 (§4.3), lifted to a shared helper at 76.2 (CF-76-2): the two
/// intra-`RedirectAction` oneofs are exclusive on FIELD PRESENCE, not on value
/// (MEASURED: `https_redirect: false` PLUS `scheme_redirect: "ftp"` REJECTS
/// while `https_redirect: false` ALONE ACCEPTS).
///
/// 76.1 inlined these two checks at the bootstrap route walk only, so the RDS
/// warm-reload path accepted a config the byte-identical BOOT config rejects
/// (CF-76-2). 76.2 makes the redirect arm SERVE a real 3xx, so both callers
/// now share this one function. `context` names the offending HCM listener at
/// boot, or `rds:<path>` on a reload.
pub(crate) fn validate_redirect_oneofs(
    rd: &RedirectAction,
    context: &str,
    route: &str,
) -> Result<(), crate::ConfigError> {
    if rd.path_redirect.is_some() && rd.prefix_rewrite.is_some() {
        return Err(crate::ConfigError::RedirectPathRewriteConflict {
            listener: context.to_string(),
            route: route.to_string(),
        });
    }
    if rd.https_redirect.is_some() && rd.scheme_redirect.is_some() {
        return Err(crate::ConfigError::RedirectSchemeRewriteConflict {
            listener: context.to_string(),
            route: route.to_string(),
        });
    }
    Ok(())
}
```

> **Doc-comment hazard — this is exactly how M-1 happened.** You are inserting immediately above a
> `#[derive]`. **Check first**: `bootstrap.rs:2657` is the bare `#[derive(…)]` for `RouteMatch`
> with **no** doc comment above it, so inserting there orphans nothing. MEASURED at `537e2a1`.
> If that has changed, insert above the doc comment, never between it and its type.

- [ ] **Step 4: Convert the `rds.rs` `if let` to an exhaustive `match`**

Replace `crates/envoy-config/src/rds.rs:133-141` with — verbatim from the pre-flight:

```rust
    // 76.2 (CF-76-2): an EXHAUSTIVE `match`, deliberately — 76.1's `if let`
    // meant adding the third `RouteAction` variant tripped NO compile error
    // here, so an RDS reload carrying a mutually-exclusive `redirect:` oneof
    // pair was accepted WARM while the byte-identical config was boot-fatal.
    // A fourth variant must fail to build until it is handled here.
    for vh in &selected.virtual_hosts {
        for route in &vh.routes {
            match &route.action {
                crate::RouteAction::Route(ar) => {
                    if !known_cluster(&ar.cluster) {
                        return Err(crate::ConfigError::UnknownCluster(ar.cluster.clone()));
                    }
                }
                // 76.2 closes CF-76-2: the redirect arm now SERVES a real 3xx,
                // so its oneof exclusivity must hold on the warm path too.
                crate::RouteAction::Redirect(rd) => {
                    crate::bootstrap::validate_redirect_oneofs(
                        rd,
                        &format!("rds:{path_str}"),
                        &route.name,
                    )?;
                }
                // `direct_response` re-validation (status range, body shape)
                // stays deferred under the OPEN ADR-0028 deferral — unchanged
                // by 76.2 and NOT widened into here.
                crate::RouteAction::DirectResponse(_) => {}
            }
        }
    }
```

Also update the function's doc block (`rds.rs:80-85`, step 4's description) to say the walk now
re-validates the redirect oneofs as well as the cluster reference.

- [ ] **Step 5: Run GREEN**

```bash
cargo test -p envoy-config --lib --no-fail-fast -- rds:: > /tmp/t-rds.txt 2>&1; echo "exit=$?"
grep -E 'test result:' /tmp/t-rds.txt
```

Expected: **`ok. 14 passed; 0 failed`** — the 12 pre-existing RDS tests (MEASURED green against
this exact rewrite in the §2 pre-flight) plus the two new ones.
Then the whole config crate: `cargo test -p envoy-config --lib --no-fail-fast` — expect `ok.`.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/rds.rs
git commit -m "phase 76.2 task 8: close CF-76-2 — RDS warm path re-validates the redirect oneofs [CF-76-2]"
```

---

## Task 9: M-1 + M-2 — restore and correct `RouteAction`'s doc comment

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs:2170-2176` (delete) and `:2245` (insert above)

**Interfaces:** none — documentation only. No behaviour change, no test change.

**The defect, MEASURED on disk at `537e2a1`.** `76.1` inserted `RedirectResponseCode` **between**
`RouteAction`'s 04.3 doc block and `RouteAction`'s `#[derive]`. Consequence: the 04.3 doc block
(`:2170-2176`) now attaches to `RedirectResponseCode` (`:2185`), and **`pub enum RouteAction`
(`:2246`) is UNDOCUMENTED** — `:2245` is a bare `#[derive(Debug, Clone, PartialEq)]`.
**M-2:** the orphaned text is also **stale** — it describes a TWO-way action oneof
(*"the route's peer keys are `direct_response: { … }` OR `route: { … }`"*), which `76.1` widened to
three. **Fix M-1 and M-2 together**; a verbatim restore would re-attach stale text.

**Why nothing caught it, and why it belongs here.** `envoy-config` enables no `missing_docs` lint
and `cargo fmt` does not reflow doc comments, so **nothing in §7.5 gate (e) reads prose**. Two of
three reviewers argued this was an Issue; `76.1`'s review graded it Minor and put the dissent on
the record. `76.2` edits this exact region (Task 8), which makes it the cheapest place to fix.

- [ ] **Step 1: Delete the orphaned block**

Delete `bootstrap.rs:2170-2176` — the seven lines from
`/// 04.3 NEW (under SPEC §3 D2): the action variant a route's HCM router` through
`/// neither-present are errors.` — leaving the `76.1` `RedirectResponseCode` doc
(`/// 76.1 (§4.1): …`) as that enum's own, correct, doc block.

- [ ] **Step 2: Insert the corrected block above `RouteAction`**

Immediately above `#[derive(Debug, Clone, PartialEq)]` / `pub enum RouteAction {`:

```rust
/// 04.3 NEW (under SPEC §3 D2), widened at 76.1: the action variant a route's
/// HCM router invocation dispatches into. Discrimination is by field-name
/// oneof at the route map level — the route's peer keys are
/// `direct_response: { ... }` OR `route: { ... }` OR `redirect: { ... }`, not
/// nested under a single `action:` key. The hand-rolled
/// `impl<'de> Deserialize` for `Route` (below) detects which peer key is
/// present and constructs the matching variant; neither-present and
/// more-than-one-present are both errors.
///
/// **Adding a FOURTH variant does not fail the build everywhere it must.**
/// The `Route` visitor's cardinality check ends in a `_ =>` catch-all and
/// `envoy-config`'s RDS re-validation historically used an `if let`, so a new
/// variant can slip through silently. Audit every site BY GREP, never by
/// trusting the compiler.
#[derive(Debug, Clone, PartialEq)]
pub enum RouteAction {
```

- [ ] **Step 3: Verify — mechanically, not by eye**

```bash
# the 04.3 block must now sit directly above `pub enum RouteAction`
grep -n -B2 '^pub enum RouteAction {' crates/envoy-config/src/bootstrap.rs
# and must appear exactly once in the file
grep -c '04.3 NEW (under SPEC §3 D2), widened at 76.1' crates/envoy-config/src/bootstrap.rs   # → 1
grep -c '04.3 NEW (under SPEC §3 D2): the action variant' crates/envoy-config/src/bootstrap.rs # → 0
```

Then `cargo build -p envoy-config --all-targets`, `cargo fmt --all -- --check`,
`cargo clippy -p envoy-config --all-targets --all-features -- -D warnings`. All clean.

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 76.2 task 9: restore and correct RouteAction's doc comment [M-1, M-2]"
```

---

## Task 10: differential fixture `0086-route-redirect-action`

**Files:**
- Create: `tests/fixtures/0086-route-redirect-action/envoy.yaml`
- Create: `tests/fixtures/0086-route-redirect-action/envoy-rust.yaml`
- Create: `tests/fixtures/0086-route-redirect-action/expectations.yaml`
- Create: `tests/fixtures/0086-route-redirect-action/README.md`

**Interfaces:**
- Consumes: the existing `Driver::Http1ProbeList` — **no new harness machinery**.
- Produces: the fixture directory Task 11's entrypoint points at.

**`0086` is the next free id — MEASURED**, not assumed: `git ls-files` shows exactly **85**
directories under `tests/fixtures/` (highest `0085-headermatcher-absence-accesslog-present-polarity`)
and exactly **85** `.rs` files under `tests/differential/tests/`;
`git ls-files 'tests/fixtures/0086*'` returns **0**.

### Why this fixture needs zero new harness code

1. **`location` is NOT allow-listed.** `HEADER_ALLOW_LIST` (`differential/src/lib.rs:1177-1181`)
   has exactly three entries — `server`, `date`, `x-envoy-upstream-service-time`, all
   `AllowMode::NameRequired`. `diff_headers` skips value comparison **only** for allow-listed
   names and compares every other name byte-exact at `:1237`. So `location` **and**
   `content-length` are compared value-exact, for free.
   **NEVER add `location` to that list — it would silently vacate the entire witness.**
2. **The name-set check catches the `content-type` hazard.** `diff_headers` compares lowercased
   name sets first (`:1206-1215`) and bails with `only-in-envoy-rust=[…]`, so a redirect
   accidentally built on `synth_with` fails loudly.
3. **Both proxies receive an IDENTICAL `Host:`.** `Http1Probe::host` (`:1149`) is required and
   `drive_http1` writes it verbatim. The two proxies listen on **different** ports — upstream on a
   testcontainers-mapped port, the subject on a reserved ephemeral port — but the authority in
   `location` comes from the `Host` header, **not** the socket. That is what makes `location`
   byte-comparable across two differently-ported proxies, and it is why probes `q01`/`q03` send an
   explicit `:1234` that deliberately matches neither listen port.

### The four binding authoring constraints

- **Every probe carries a DISTINCT `path:` AND selects a DIFFERENT route.** The distinct-`path:`
  rule is standing (`BEHAVIOR_CONTRACT.md` Phase 75 §G). Here it is load-bearing for
  **correctness**, not just attribution. **This is why `q01`/`q03` get their own routes
  (`/q1-hostport`, `/q3-hostport`) rather than re-probing `/f-https` and `/j-bare` with a different
  `Host:`** — that would violate the distinct-path rule.
- **PREFIX OVERLAP SILENTLY SHADOWS A PROBE.** A parent-recon cell was lost when `prefix: "/scheme"`
  preceded `prefix: "/schemehost"`. **Verify no prefix below is a prefix of another** before
  running; they were chosen so that no two share a first character except `/q1-`/`/q3-`, which
  differ at position 2.
- **Query-bearing probes MUST use `prefix:`-matched routes, never `path:`.** This keeps the fixture
  clean of **CF-76-1** (upstream strips the query before route matching; envoy-rust matches the raw
  target). Every route below is `prefix:`-matched, so the divergence is never exercised.
- **`Http1ProbeList` ABORTS AT THE FIRST FAILING PROBE** — every check in
  `run_http1_probe_list_arm` (`lib.rs:5423`) uses `bail!`/`?` inside the `for probe in probes`
  loop. **One red run names ONE probe.** A regression breaking several cells reports as a single
  failure; that is expected, and a review must not read it as "only one cell broke".

- [ ] **Step 1: Create `envoy.yaml`**

`{{PORT}}` is the **only** token this driver substitutes (`port_key_for` returns `"PORT"`, and
`Http1ProbeList` is **not** in `driver_needs_admin_port`), so **`{{ADMIN_PORT}}` must not appear**.

```yaml
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 0.0.0.0, port_value: {{PORT}} }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/a-host" }
                          redirect: { host_redirect: "example.com" }
                        - match: { prefix: "/b-query" }
                          redirect: { host_redirect: "example.com" }
                        - match: { prefix: "/c-pathr" }
                          redirect: { path_redirect: "/newpath" }
                        - match: { prefix: "/d-pathq" }
                          redirect: { path_redirect: "/newpath" }
                        - match: { prefix: "/e-pfx" }
                          redirect: { prefix_rewrite: "/replaced" }
                        - match: { prefix: "/f-https" }
                          redirect: { https_redirect: true }
                        - match: { prefix: "/g-c307" }
                          redirect:
                            host_redirect: "example.com"
                            response_code: TEMPORARY_REDIRECT
                        - match: { prefix: "/h-strip" }
                          redirect: { host_redirect: "example.com", strip_query: true }
                        - match: { prefix: "/i-port" }
                          redirect: { host_redirect: "example.com", port_redirect: 8443 }
                        - match: { prefix: "/j-bare" }
                          redirect: {}
                        - match: { prefix: "/k-scheme" }
                          redirect: { scheme_redirect: "ftp" }
                        - match: { prefix: "/l-both" }
                          redirect: { scheme_redirect: "https", host_redirect: "e.com" }
                        - match: { prefix: "/m-see" }
                          redirect:
                            host_redirect: "e.com"
                            strip_query: true
                            response_code: SEE_OTHER
                        - match: { prefix: "/n-hport" }
                          redirect: { https_redirect: true, port_redirect: 443 }
                        - match: { prefix: "/o-found" }
                          redirect: { host_redirect: "example.com", response_code: FOUND }
                        - match: { prefix: "/p-perm" }
                          redirect:
                            host_redirect: "example.com"
                            response_code: PERMANENT_REDIRECT
                        - match: { prefix: "/q1-hostport" }
                          redirect: { https_redirect: true }
                        - match: { prefix: "/q3-hostport" }
                          redirect: {}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
```

- [ ] **Step 2: Create `envoy-rust.yaml` — identical except THREE hunks**

Copy `envoy.yaml` and apply exactly three changes:
1. prepend a `node:` block,
2. `address: 0.0.0.0` → `address: 127.0.0.1`,
3. delete the trailing `admin:` block.

```yaml
node:
  id: x
  cluster: y
```

> **YAML 1.1 trap:** an unquoted `cluster: y` under `node:` parses as boolean `true`. Every
> existing fixture writes it exactly as `y` and it is fine there — **do not "improve" it.**

- [ ] **Step 3: Create `expectations.yaml` — 18 probes**

`expected_headers` is a **BARE SCALAR**, not a map: `Http1HeaderRule` (`lib.rs:1071`) is an
externally-tagged unit-variant enum with `rename_all = "snake_case"`. Do **not** confuse it with
the sibling `HeaderRule` (`lib.rs:1082`), which is `#[serde(tag = "rule")]` and *is* spelled as a
map — that one belongs to `Driver::Http1WithAccessLog`. `Http1Probe` is `deny_unknown_fields`, so
a typo'd key fails to deserialize.

```yaml
driver:
  kind: http1_probe_list
  probes:
    - name: r01-host-redirect-replaces-authority
      method: get
      path: "/a-host"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r02-query-preserved-by-default
      method: get
      path: "/b-query/deep?a=b"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r03-path-redirect-replaces-path
      method: get
      path: "/c-pathr/sub"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r04-path-redirect-still-keeps-query
      method: get
      path: "/d-pathq/x?k=v"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r05-prefix-rewrite-replaces-matched-span
      method: get
      path: "/e-pfx/sub"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r06-https-redirect-forces-scheme
      method: get
      path: "/f-https/x"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r07-response-code-307
      method: get
      path: "/g-c307"
      host: "envoy-rust.test"
      expected_status: 307
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r08-strip-query-drops-query
      method: get
      path: "/h-strip/a?q=1&z=2"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r09-port-redirect-with-host-redirect
      method: get
      path: "/i-port"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r10-bare-redirect-echoes-request
      method: get
      path: "/j-bare/deep"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r11-scheme-redirect-ftp-verbatim
      method: get
      path: "/k-scheme/x"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r12-scheme-and-host-together
      method: get
      path: "/l-both/y"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r13-response-code-303-with-strip-query
      method: get
      path: "/m-see/y?q=1"
      host: "envoy-rust.test"
      expected_status: 303
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r14-scheme-change-keeps-redundant-443
      method: get
      path: "/n-hport/y"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r15-response-code-302
      method: get
      path: "/o-found"
      host: "envoy-rust.test"
      expected_status: 302
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r16-response-code-308
      method: get
      path: "/p-perm"
      host: "envoy-rust.test"
      expected_status: 308
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: q01-host-unset-preserves-request-port
      method: get
      path: "/q1-hostport/x"
      host: "envoy-rust.test:1234"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: q03-bare-redirect-preserves-request-port
      method: get
      path: "/q3-hostport/d"
      host: "envoy-rust.test:1234"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body:
    kind: byte_exact
```

**The expected `location` for each probe** (asserted implicitly, byte-exact, by
`set_equal_modulo_allow_list`, because `location` is not allow-listed):

| probe | expected `location` |
|---|---|
| r01 | `http://example.com/a-host` |
| r02 | `http://example.com/b-query/deep?a=b` |
| r03 | `http://envoy-rust.test/newpath` |
| r04 | `http://envoy-rust.test/newpath?k=v` |
| r05 | `http://envoy-rust.test/replaced/sub` |
| r06 | `https://envoy-rust.test/f-https/x` |
| r07 | `http://example.com/g-c307` |
| r08 | `http://example.com/h-strip/a` |
| r09 | `http://example.com:8443/i-port` |
| r10 | `http://envoy-rust.test/j-bare/deep` |
| r11 | `ftp://envoy-rust.test/k-scheme/x` |
| r12 | `https://e.com/l-both/y` |
| r13 | `http://e.com/m-see/y` |
| r14 | `https://envoy-rust.test:443/n-hport/y` |
| r15 | `http://example.com/o-found` |
| r16 | `http://example.com/p-perm` |
| q01 | `https://envoy-rust.test:1234/q1-hostport/x` |
| q03 | `http://envoy-rust.test:1234/q3-hostport/d` |

- [ ] **Step 4: Create `README.md`**

82 of 85 fixtures have one; `run_fixture` never reads it. Cover: what the fixture witnesses (the
`location` construction rules (a)-(e) and all five status codes); that it is **backend-free**
(`clusters: []`, no `{{BACKEND_PORT}}` marker → no backend container spawns) and therefore fully
verifiable locally; **why `location` must never be added to `HEADER_ALLOW_LIST`**; why `q01`/`q03`
send a `Host:` port that matches neither listen port; and that every route is `prefix:`-matched to
keep the fixture clean of CF-76-1.

- [ ] **Step 5: Verify the fixture parses before wiring the entrypoint**

```bash
cargo build -p envoy-bin        # MANDATORY — a stale debug binary REDs with `unknown field`
./target/debug/envoy-bin --mode validate -c tests/fixtures/0086-route-redirect-action/envoy-rust.yaml
```
Expected: exit 0. (envoy-bin writes `ConfigError` to **stdout**.) Also assert no prefix shadows
another:
```bash
grep -o 'prefix: "[^"]*"' tests/fixtures/0086-route-redirect-action/envoy.yaml | sort
```
and check by eye that no entry is a prefix of a later one.

- [ ] **Step 6: Commit**

```bash
git add tests/fixtures/0086-route-redirect-action/
git commit -m "phase 76.2 task 10: differential fixture 0086 — 18 probes over the redirect location rules"
```

---

## Task 11: the fixture entrypoint + the local differential run

**Files:**
- Create: `tests/differential/tests/route_redirect_action.rs`

**Interfaces:**
- Consumes: `differential::run_fixture`, and `tests/fixtures/0086-route-redirect-action/`
  from Task 10.

**Registration.** **No registry file exists.** `tests/differential/Cargo.toml` has **zero**
`[[test]]` sections (MEASURED); cargo auto-discovers every `tests/differential/tests/*.rs`. Adding
the file is sufficient. Naming convention across all 85: the fixture directory name minus the
`NNNN-` prefix, `-` → `_`, and the test fn is `<same>_fixture`.

- [ ] **Step 1: Create the entrypoint**

```rust
//! Sub-phase 76.2 differential acceptance test: the `Route.redirect` action.
//! Drives 18 HTTP/1.1 probes at a backend-free HCM listener (`clusters: []`,
//! `redirect:` routes only) and requires identical (status, body,
//! header-set-modulo-allow-list) between upstream Envoy v1.33.0 and envoy-rust.
//!
//! This is the FIRST differential witness of `Route.redirect` in the corpus. It
//! pins the whole `location` construction rule set measured against
//! `envoyproxy/envoy:v1.33.0`:
//!   * the AUTHORITY ASYMMETRY — `host_redirect` DROPS the request's original
//!     port (probe `q01` vs `r01`) while a scheme-only change PRESERVES it;
//!     `port_redirect` overrides both and does NOT normalise a redundant `:443`.
//!   * the QUERY rule — preserved by default even when `path_redirect` replaces
//!     the path wholesale (`r04`), dropped by `strip_query` (`r08`, `r13`).
//!   * all five `response_code` values on the wire (301/302/303/307/308).
//!
//! `location` is deliberately NOT on the harness's 3-entry `HEADER_ALLOW_LIST`,
//! so `diff_headers` compares it VALUE-EXACT. That comparison IS this fixture's
//! entire witness — adding `location` to the allow-list would silently vacate
//! it. Both proxies listen on different ports, so `location` is only
//! byte-comparable because its authority comes from the `Host:` header, not the
//! socket; probes `q01`/`q03` send `:1234`, matching neither listen port, to
//! prove exactly that.
//!
//! Docker-gated, backend-free (no `{{BACKEND_PORT}}` marker → no backend
//! container spawns), and therefore FULLY verifiable on a developer host.

use std::path::PathBuf;

#[tokio::test]
async fn route_redirect_action_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0086-route-redirect-action");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
```

- [ ] **Step 2: Run it**

```bash
cargo build -p envoy-bin        # MANDATORY, again — the redirect runtime is new
cargo test -p differential --test route_redirect_action -- --nocapture > /tmp/t-0086.txt 2>&1
echo "exit=$?"; grep -E 'test result:|panicked|probe' /tmp/t-0086.txt
```

Expected: **`test result: ok. 1 passed`**.

**Adjudication guidance if it REDs:**
- A **`~1-3 s` green is NORMAL** for a backend-free fixture, not a silent skip. If you doubt it,
  poll `docker ps` during the run and add a deliberate negative control.
- A **mass** `client error (Connect)` across many fixtures means the Docker daemon is down:
  `sudo setfacl -m u:esa:rw /dev/kvm && systemctl --user restart docker-desktop`, then re-run.
  A mass wave of `never became accept-ready` with the daemon **up** is contention, not a defect.
- **One red run names ONE probe** (`Http1ProbeList` aborts at the first failure). Fix that cell,
  re-run, and expect the next one to surface. Do not read a single named probe as "only one cell
  broke".
- **Never weaken a probe or the allow-list to make it pass.**

- [ ] **Step 3: Run the FULL differential suite — gate (b)**

```bash
cargo test -p differential --no-fail-fast > /tmp/t-diff.txt 2>&1; echo "exit=$?"
grep -E 'test result:' /tmp/t-diff.txt
```

Expected: the 85 pre-existing fixtures still green plus the new one = **86**. Backend-routing
fixtures go RED locally on this host (it routes the backend via `192.168.65.2`, not the
allow-listed `192.168.65.254`/`172.17.0.1`) — **CI is authoritative for those**; adjudicate each
RED by ADR-0164's four-part test, never by membership in a list.

- [ ] **Step 4: Commit**

```bash
git add tests/differential/tests/route_redirect_action.rs
git commit -m "phase 76.2 task 11: fixture 0086 entrypoint — the first differential witness of Route.redirect"
```

---

## Task 12: `BEHAVIOR_CONTRACT.md` Phase 76 section

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — append a new `### Phase 76 …` section at the
  **end of the access-log/phase block**, i.e. after the Phase 75 section (which spans `:2751`
  through `:2958`) and **before** `## xDS wire state machine` (`:2959`)

**Interfaces:** none — documentation. But it is a **§7.5 obligation**: the contract is the
canonical definition of equivalence (doctrine D-3.3), and MEASURED behaviour that lives only in a
phase SPEC is not durable.

**MEASURED absent today:** `grep -c 'Phase 76' docs/envoy-rust/BEHAVIOR_CONTRACT.md` → **0**.

- [ ] **Step 1: Write the section**

Model the structure on the Phase 75 section at `:2751`. It must bank, with the same
"MEASURED against `envoyproxy/envoy:v1.33.0`" framing:

- **§A — the `location` construction rules (a)-(e)** verbatim from `76.2/SPEC.md` §2.4, with the
  full R1-R16 / Q1-Q4 / E1-E2 tables. Call out the **authority asymmetry** as the headline rule:
  `host_redirect` set → the request's original port is DROPPED; unset → preserved including port;
  `port_redirect` overrides both and is rendered verbatim with **no range clamp**; a scheme-only
  change does **not** normalise a redundant `:443`.
- **§B — the redirect response header set.** `location`, `date`, `server`, `connection`,
  `content-length`, in that wire order, and **NO `content-type`** — with the explicit contrast
  against `direct_response`, which does carry one.
- **§C — the reason phrases.** All five status lines, and the note that the reason phrase is
  **not** part of the equivalence matrix (`response_status: exact` compares the code only), so
  303/307/308 are pinned in-process, not differentially.
- **§D — the access-log observables.** `%RESPONSE_CODE_DETAILS%` is `direct_response` — the same
  string used for a `direct_response:` route, so **no new detail string, `Op` or
  `AccessLogRecord` field exists**; `%RESPONSE_FLAGS%` is `-`; and `prefix_rewrite` **mutates** the
  logged `:path` while `path_redirect` does **not**.
- **§E — the harness rule, stated as a standing prohibition.** `location` is **NOT** on the
  3-entry `HEADER_ALLOW_LIST` and is therefore compared **value-exact** by `diff_headers`. That
  comparison is fixture `0086`'s entire witness. **`location` must never be added to the
  allow-list.**
- **§F — NOT MEASURED, carried from `76.2/SPEC.md` §8**, so a later session does not mistake these
  for settled: redirect on a **TLS** listener (does the default scheme become `https`?); upstream's
  **H2** `:scheme`/`:authority` handling; a request with **no `Host`** header; `port_redirect`
  boundaries beyond the single `70000` probe; `redirect` × `typed_per_filter_config` on one route;
  whether `strip_port`'s `rfind(':')` handles a **bracketed IPv6 literal** authority. **Add two
  more, introduced by this plan and not by the SPEC:** (i) whether `prefix_rewrite` on a
  **`path:`-matched** route replaces the whole path — this plan implements "the matched span is the
  whole path when `match.prefix` is `None`", which is unmeasured and is never exercised by `0086`
  (every route there is `prefix:`-matched); and (ii) whether the query rides along on the
  **rewritten `:path`** when `prefix_rewrite` and a query combine — this plan preserves it, and
  `0086`'s `r05` probe is deliberately query-free, so the cell is unwitnessed.

- [ ] **Step 2: Verify**

```bash
grep -c '^### Phase 76' docs/envoy-rust/BEHAVIOR_CONTRACT.md          # → 1
grep -n '^## xDS wire state machine' docs/envoy-rust/BEHAVIOR_CONTRACT.md
```
The new heading must sit **before** the `## xDS wire state machine` line. Confirm no existing line
was duplicated:
```bash
sort docs/envoy-rust/BEHAVIOR_CONTRACT.md | grep -v '^$' | uniq -d | head
```

- [ ] **Step 3: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 76.2 task 12: BEHAVIOR_CONTRACT Phase 76 — the redirect location + header-set rules"
```

---

## 6. Exit criteria for §5 state 3 (hand-off to state 4)

State 3 is complete — and **state 4 is a SEPARATE session** (§5.1; ADR-0127: the context that
wrote an artifact must not grade it) — when:

- [ ] All 12 tasks are committed, each with its own commit.
- [ ] `docs/envoy-rust/phases/76.2-redirect-runtime-fixture/PROGRESS.md` exists and was appended
      **on each task completion**, quoting real command output — not reconstructed at the end.
- [ ] `STATE.md` is advanced to `76.2` state 4 with
      `## Next expected skill` = `superpowers:verification-before-completion`, including the
      ADR-0035 relocation (the **14**-line set, checked **PER FILE**: each relocated line must go
      to **0** occurrences in `STATE.md` and **+1** in `STATE_HISTORY.md`; a combined count is
      invariant by construction and false-passes) plus the structural check (heading list intact,
      zero duplicated non-blank lines).
- [ ] **No `ROADMAP.md` status cell was flipped.** A state-3 commit flips none. Row `76.2` stays
      `planned`, `76` stays `in-progress`, `76.1` stays `done`.

**What state 4 will have to run (the full §7.5 gate) — budget for it:**
`cargo build --workspace --all-targets`;
`cargo clippy --workspace --all-targets --all-features -- -D warnings`;
`cargo fmt --all -- --check`; `cargo test --workspace --no-fail-fast` (redirect to a file, run it
**2-3×** and **diff the failing SET** — a single sweep cannot satisfy ADR-0164 leg (iii));
`cargo deny check`; the differential suite; the conformance suites.

**Two arithmetic identities to close at state 4:**
1. `local passed + local failed == CI passed` — the single strongest flake-vs-regression
   discriminator.
2. **CI totals must GROW.** The last CI run was **`162 binaries passed=2137 failed=0`** (commit
   `ff2871c`, run `30585270124`), and `76.1`'s 32 new tests closed `2105 + 32 = 2137`. `76.2` adds
   roughly **30** in-process tests plus **1** differential binary, so expect approximately
   `163 binaries` / `passed≈2168`. Derive it with
   `grep -oE 'test result: (ok|FAILED)\. …'` and **awk fields `$4`/`$6`** — `grep -o 'test result:
   ok\. …'` discards `FAILED` lines so its `failed=0` is true *by construction*, and using
   `$5`/`$7` returns a clean-looking, vacuous `passed=0`.

---

## 7. Self-review against the SPEC

| `76.2/SPEC.md` §5 scope item | task |
|---|---|
| 1. Replace the `76.1` placeholder arm, flipping its test deliberately | **Task 5** |
| 2. A pure `location`-builder encoding (a)-(e) | **Task 3** |
| 3. `synth_redirect` — not `synth_with` | **Task 4** |
| 4. Three reason phrases 303/307/308, pinned in-process | **Task 1** |
| 5. The `prefix_rewrite` in-place `:path` mutation via `&mut Request` | **Task 5** (mechanism) + **Task 6** (pins) |
| 6. Fixture `0086` and its entrypoint | **Tasks 10, 11** |
| 7. `BEHAVIOR_CONTRACT.md` Phase 76 section | **Task 12** |
| 8. Close the parent phase `76` | **state-6 close-out** — flips rows `76.2` **and** `76` to `done`. NOT this state. |
| §6 in-process: R/Q/E rows | **Task 3** (all 22) |
| §6 in-process: header set, exactly five names, no `content-type` | **Task 4** |
| §6 in-process: the three reason phrases | **Task 1** |
| §6 in-process: `prefix_rewrite` mutation / `path_redirect` non-mutation | **Task 6** |
| §6 in-process: `%RESPONSE_CODE_DETAILS%` = `direct_response` reuse | **Task 5** |
| §6 in-process: an HTTP/2 redirect test proving the shared seam | **Task 7** |
| §6 differential: `0086` green; all 85 pre-existing still green | **Task 11** steps 2-3 |
| (not in the SPEC) CF-76-2 | **Task 8** — see §5 |
| (not in the SPEC) M-1 / M-2 / N-3 | **Task 9** / **Task 9** / **Task 5** |
| §7 non-goals: `regex_rewrite`, the `route:`-arm rewrites, `internal_redirect_policy`, an H2 fixture, CF-76-1, the other carry-forwards | **no task** — deliberately unbuilt |

**Type consistency check.** `plan_redirect` is introduced in Task 3 with the signature
`(authority: &str, target: &str, matched_prefix: Option<&str>, rd: &RedirectAction) ->
RedirectPlan` and is called with exactly that shape in Tasks 3, 5 and 6. `synth_redirect` is
introduced in Task 4 as `(status: u16, location: String, close: bool) -> Response` and is called
with exactly that shape in Task 5. `validate_redirect_oneofs` is introduced in Task 8 as
`(rd: &RedirectAction, context: &str, route: &str) -> Result<(), crate::ConfigError>` and is called
with exactly that shape from both `bootstrap.rs` and `rds.rs`. `RedirectPlan`'s three fields
(`location`, `status`, `rewritten_path`) are consumed under those names in Tasks 3, 5 and 6.
