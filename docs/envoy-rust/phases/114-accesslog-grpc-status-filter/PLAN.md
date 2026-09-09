# Phase 114 — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the `grpc_status_filter` access-log FILTER arm — the seventh of upstream Envoy's twelve `envoy.config.accesslog.v3.AccessLogFilter` oneof arms — gating a sink's per-record emission on the request's UNGATED effective gRPC status, witnessed by new differential fixture `0094-accesslog-grpc-status-filter`.

**Architecture:** A seventh `Option` arm on the config `AccessLogFilter`, carrying an untagged token list resolved to integer codes by a MEASURED grammar. One new NON-`Option` `u8` field on `AccessLogRecord` holds the effective status, populated at BOTH codecs' existing record-build sites from one shared helper: the response `grpc-status` header if present and in range, else the phase-110 `http_to_grpc_status` map over the response code. The `should_log` predicate gains a fifth parameter carrying that code, and a new `LogFilter::GrpcStatus { codes, exclude }` arm evaluates plain-integer membership — no trait object, so the ADR-0150 cycle seam is not involved.

**Tech Stack:** Rust 2024, `envoy-config` (serde + `serde_yaml` 0.9.34), `envoy-accesslog` (a leaf crate with zero intra-workspace dependencies), `envoy-http1`, `envoy-http2`, the existing `Driver::Http1AccessLogByteExact` differential driver.

**Spec:** `docs/envoy-rust/phases/114-accesslog-grpc-status-filter/SPEC.md`. **Read it together with this plan — but see "SPEC corrections" below: this PLAN-write measured five SPEC claims to be wrong, and `ADR-0197` is the forward correction. Where they disagree, this plan wins.**

---

## Global Constraints

- Upstream target is `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`. Every behavioural cell in this plan was MEASURED against it at this PLAN-write; none was recalled or derived. Container ownership was proved on every probe with `docker inspect <cid> --format '{{.Image}}'`.
- `#![forbid(unsafe_code)]` in every crate root (D-3.8). No `unsafe` anywhere in this phase.
- **No new dependency, no new workspace crate, no new harness driver, no new fuzz target.** `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml` and `tests/differential/src/lib.rs` MUST be untouched — VERIFIED untouched on the prototype (PV-9), with a positive control proving the check reaches a file that IS touched. If a task appears to need one of them, STOP: the scope has drifted.
- Every task ends green on `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`. All three were clean on the prototype. ⚠ Per `ADR-0194` DECISION 2 a whole-slice prototype validates the SLICE, **never a TASK BOUNDARY** — these are the plan's gates for the executor to run, not a measured claim about each boundary.
- `AccessLogRecord` deliberately does NOT implement `Default`, so a new field is an `E0063` at every exhaustive struct literal. **There are exactly FOUR such literals** (Task 5).
- Adding a `pub` field to `AccessLogFilter` is a CROSS-CRATE change: every exhaustive `AccessLogFilter` literal in `envoy-config` and `envoy-http1` breaks with `E0063`. Literals using `..AccessLogFilter::default()` absorb it silently and must NOT be edited (Task 3).
- Nothing is fixed (§6.3; `ADR-0165`) **except the single rider Task 1 takes deliberately and labels as such** — `CF-113-7`, authorised as a rider by `SPEC.md` §5 non-goal 6 because this phase touches its file. No other carry-forward is consumed.

---

## SPEC corrections this PLAN-write measured — read before Task 1

Five claims in the landed `SPEC.md` did not survive PV-1…PV-9. `SPEC.md` is landed and is NOT edited; `ADR-0197` carries the forward correction.

1. **`SPEC.md` §1 and §4 item 6's "Production `should_log` call sites are exactly TWO … the remaining 125 of the 127 occurrences are test code" is FALSE.** The 127 total is right. The split is **5 production + 122 test**, not 2 + 125. The three missed production sites are `crates/envoy-accesslog/src/filter.rs:148` and `:151` (the `And`/`Or` recursion) and `crates/envoy-accesslog/src/file_sink.rs:114` (the `FileSink` → `LogFilter` delegation). They are NOT mechanical test edits. Task 6 covers all five.
2. **`SPEC.md` §8 PV-1's citation `tests/differential/src/lib.rs:1187` is off by one.** Line 1187 is the `#[serde(default)]` attribute; `extra_headers` is at **`:1188`**. The anchor text also occurs 3× in that file (`:155`, `:1188`, `:1232`), so it must be qualified by the enclosing struct. Every other cited anchor re-derived CORRECT at `0eb81bd`.
3. **`SPEC.md` §6's probe table gives the wrong observed status for the gRPC probes.** Probes 1, 2 and 4 send `content-type: application/grpc`, so the phase-110 local-reply transform rewrites the status to **200** and emits a `grpc-status` header. Their `expected_status` is 200, NOT the route's `direct_response.status`. MEASURED on both proxies.
4. **`SPEC.md` §6's "the explicit-header leg is not directly expressible in this fixture" is FALSE, and this is the most consequential correction.** It IS expressible, and without `response_headers_to_add`: a gRPC-content-type request against a non-200 `direct_response` makes the phase-110 transform emit the header, and probes 1 and 2 witness the header leg for real. Had the filter derived from the LOGGED response code (200) it would have computed 2 for both and dropped them. **`CF-114-4` therefore narrows to the `exclude: true` witness alone.**
5. **`SPEC.md` §7's size estimate is high.** Projected central ≈1250 net code lines in a 1030–1465 band; **MEASURED 938**, below the band's low end (0.75× the central). The §6.1 gate does NOT fire.

A sixth item is not a correction but a trap the SPEC does not carry: **the filter's code-1 spelling is `CANCELED` (one L), while phase 113's `%GRPC_STATUS(SNAKE_STRING)%` table renders code 1 as `CANCELLED` (two Ls)**. Both are MEASURED. They are different upstream enums. `CANCELLED` is REJECTED by the filter. **Do not reuse `GRPC_STATUS_NAMES` (`crates/envoy-accesslog/src/command_operator.rs`) as the accept-list** — it gets exactly that cell backwards, and the other 16 names coincide, which is what makes the mistake easy to miss.

---

## File Structure

| file | responsibility | change |
|---|---|---|
| `crates/envoy-http2/src/hcm.rs` | the CF-113-7 doc-comment re-attachment; the H2 record-build population; the H2 in-process pin | modify |
| `crates/envoy-http1/src/grpc.rs` | item-level `pub` on `http_to_grpc_status` | modify |
| `crates/envoy-http1/src/lib.rs` | the targeted `pub use` | modify |
| `crates/envoy-config/src/bootstrap.rs` | `GrpcStatusFilter`, `GrpcStatusToken`, the canonical table, `resolve_grpc_status_token`, the seventh `AccessLogFilter` arm, the validator growth, the tests | modify |
| `crates/envoy-config/src/lib.rs` | the `ConfigError::UnknownGrpcStatus` variant + three re-exports | modify |
| `crates/envoy-accesslog/src/record.rs` | the `grpc_status_code` field + `test_baseline` | modify |
| `crates/envoy-accesslog/src/filter.rs` | the `should_log` widening + the `LogFilter::GrpcStatus` arm + tests | modify |
| `crates/envoy-accesslog/src/file_sink.rs` | the `should_log` widening + one E0063 literal | modify |
| `crates/envoy-http1/src/hcm.rs` | `effective_grpc_status`, the H1 record-build population, the `should_log` call site, `compile_access_log_filter`'s seventh arm, tests | modify |
| `tests/fixtures/0094-accesslog-grpc-status-filter/{envoy,envoy-rust,expectations}.yaml`, `README.md` | the differential fixture | create |
| `tests/differential/tests/accesslog_grpc_status_filter.rs` | the fixture runner | create |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | the grammar + the runtime rule | modify |

**Task order is dependency-forced.** Task 1 is taken FIRST because it moves every line number in `crates/envoy-http2/src/hcm.rs` and must not be entangled with a feature edit. Task 2 must precede Task 5 because the H2 record build cannot reach `http_to_grpc_status` without it. Task 5 must precede Task 6 because the widened call sites pass `record.grpc_status_code`. Task 6 must precede Task 7 because the new arm consumes the fifth parameter. Task 8 needs both Task 3's config type and Task 7's runtime variant.

---

## §6.1 SPLIT GATE — MEASURED, DOES NOT FIRE

The gate is ~25 tasks OR ~1500 net LoC. **This plan is 10 tasks and MEASURED 938 net LoC excluding `docs/`.**

The measurement is not a projection. Every code block below was inserted into a scratch `git worktree` (created with `git worktree add --detach`, NEVER `cp -r` — a worktree's `.git` is a pointer file and `cp -r` makes two trees share ONE index, which is the method failure `ADR-0193` paid for) with its **own `CARGO_TARGET_DIR`**, and measured through its **own `GIT_INDEX_FILE`** seeded by `git read-tree HEAD`. The main tree was verified `git status --porcelain`-clean during the measurement.

That tree builds, passes `cargo clippy --workspace --all-targets --all-features -- -D warnings`, passes `cargo fmt --all -- --check`, passes **1224** unit tests across the four affected crates (133 / 722 / 243 / 126+1 ignored) and **2037** across the whole workspace, and turns fixture `0094` GREEN against both real proxies — with the Task 9 non-vacuity mutation verified RED from the same tree and the unmutated control verified GREEN after an md5-checked restore.

| file | insertions | deletions | net |
|---|---:|---:|---:|
| `crates/envoy-accesslog/src/file_sink.rs` | 13 | 5 | 8 |
| `crates/envoy-accesslog/src/filter.rs` | 169 | 72 | 97 |
| `crates/envoy-accesslog/src/record.rs` | 12 | 0 | 12 |
| `crates/envoy-config/src/bootstrap.rs` | 249 | 4 | 245 |
| `crates/envoy-config/src/lib.rs` | 26 | 19 | 7 |
| `crates/envoy-http1/src/grpc.rs` | 6 | 1 | 5 |
| `crates/envoy-http1/src/hcm.rs` | 210 | 60 | 150 |
| `crates/envoy-http1/src/lib.rs` | 1 | 0 | 1 |
| `crates/envoy-http2/src/hcm.rs` | 32 | 0 | 32 |
| `tests/differential/tests/accesslog_grpc_status_filter.rs` | 24 | 0 | 24 |
| `tests/fixtures/0094-…/README.md` | 73 | 0 | 73 |
| `tests/fixtures/0094-…/envoy-rust.yaml` | 91 | 0 | 91 |
| `tests/fixtures/0094-…/envoy.yaml` | 93 | 0 | 93 |
| `tests/fixtures/0094-…/expectations.yaml` | 100 | 0 | 100 |
| **TOTAL (14 files)** | **1099** | **161** | **938** |

Re-summed mechanically: 8+97+12+245+7+5+150+1+32+24+73+91+93+100 = **938** ✓. The `docs/` slice is separately **0** at the measurement (Task 10's `BEHAVIOR_CONTRACT.md` section is excluded from the gate and was not prototyped).

Against the 1500 gate that leaves a **562-line / 37% margin**. The worst measured post-plan drift on this project is 1.10× (`112.2`), which lands 1032 — still 31% clear. The SPEC's central projection was ≈1250, so this measures at **0.75×** of it; the projection was HIGH, which is the opposite of the projected-estimate band (1.33×–1.66× UNDER) and is why a measurement was required rather than a calibration factor.

Calibration for the record — MEASURED-on-prototype estimates: `112.1` 1.00×, `112.2` 1.10×, `113` 1.07×. PROJECTED estimates: `110.2` 1.33×, `110.1` 1.41×, `111` 1.66×.

⚠ **`ADR-0189`'s measured 595 landed at 652 purely because the plan was edited AFTER the measurement, so a measured estimate goes stale in its METHOD CLAIM and not only in its number.** The 938 above was taken against this plan's FINAL content, after the last `cargo fmt` pass and after the fixture README was written. **If you edit this plan, re-measure.**

**Therefore: NO SPLIT. `ADR-0196`'s reservation of `ADR-0197` for a split is RELEASED, and the number is consumed by this PLAN-write ADR instead**, so `DECISIONS.md` gains no new gap. `ADR-0198` is next free.

---

## Task 1: RIDER — re-attach `finalize_h2_stream`'s doc comment (CF-113-7)

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing. **Zero behaviour change.**

**This is a deliberate rider, not a deliverable.** `SPEC.md` §5 non-goal 6 authorises it because this phase touches this file, and requires it be taken **in its own commit, explicitly labelled, or not at all**. It is taken FIRST because it moves every line number below 989 in a 7761-line file, and entangling that with a feature edit would make both unreviewable.

**The defect, re-derived at `0eb81bd`.** `h2_grpc_status()` was spliced in above `finalize_h2_stream`'s `#[allow(clippy::too_many_arguments)]` attribute, and because there is no `///`-blank separator between the two doc blocks, rustdoc concatenates them: `finalize_h2_stream`'s original 10-line doc comment now documents a function whose entire body is `None`, and `finalize_h2_stream` carries no doc comment at all. `fmt` and `clippy` cannot see this — an anchor can be unique and still be the wrong place.

- [ ] **Step 1: Locate the region by TEXT and assert the anchor is unique**

```bash
grep -cF '/// 06.2 Task 7: factored per-stream finalization — sends the' crates/envoy-http2/src/hcm.rs
grep -cF 'fn h2_grpc_status() -> Option<String> {' crates/envoy-http2/src/hcm.rs
grep -cF '#[allow(clippy::too_many_arguments)]' crates/envoy-http2/src/hcm.rs
```
Expected: `1`, `1`, and a count ≥ 1. ⚠ If the third is > 1, do NOT use it as an anchor; use the first instead. Then read the region with `grep -n` on the first anchor and `sed -n '<n-2>,<n+30>p'`.

- [ ] **Step 2: Move the helper ABOVE `finalize_h2_stream`'s doc block**

Cut the 14-line run that begins with `/// The `%GRPC_STATUS%` backing value for the HTTP/2 access-log record.` and ends with the blank line after `}` (the helper's own doc comment, the `fn`, and its trailing blank), and paste it immediately ABOVE the line `/// 06.2 Task 7: factored per-stream finalization — sends the`.

After the move the file must read, in order:

```rust
/// The `%GRPC_STATUS%` backing value for the HTTP/2 access-log record.
///
/// ALWAYS `None` (CF-113-2). H2's gRPC status would have to come from the
/// response TRAILER block, and `finalize_h2_stream` MOVES `trailers` into
/// `send_envoy_response` before it builds the record — so the value is not
/// live at the record build. Phase 113 is HTTP/1.1-only by charter.
///
/// This is a named function rather than an inline `None` so the boundary is
/// pinned by `h2_grpc_status_is_absent` below: a phase that lifts it must
/// change a tested function, not silently edit a struct literal.
fn h2_grpc_status() -> Option<String> {
    None
}

/// 06.2 Task 7: factored per-stream finalization — sends the
/// downstream response via `send_envoy_response`, then (if the HCM
/// config carries access-log sinks) builds an `AccessLogRecord` and
/// emits it once per sink. Mirrors the H1 factored join-point at
/// `envoy_http1::serve_connection`'s tail.
///
/// Per PLAN-write SPEC correction 2 the access-log dispatch lands
/// AFTER `send_envoy_response` returns (covers both the empty-body
/// `send_response(.., end_of_stream=true)` branch and the non-empty
/// `send_data(.., end_of_stream=true)` branch uniformly).
#[allow(clippy::too_many_arguments)]
async fn finalize_h2_stream(
```

- [ ] **Step 3: Assert the move was byte-neutral**

```bash
git diff --numstat crates/envoy-http2/src/hcm.rs
```
Expected: `14	14	crates/envoy-http2/src/hcm.rs` — equal insertions and deletions, because nothing but position changed. ⚠ If the two numbers differ, text was altered; revert and redo.

- [ ] **Step 4: Verify nothing broke**

Run: `cargo test -p envoy-http2 --lib h2_grpc_status && cargo clippy -p envoy-http2 --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: `h2_grpc_status_is_absent` PASSES (it calls `super::h2_grpc_status()` and is position-independent), clippy silent, fmt silent.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 114 rider: re-attach finalize_h2_stream's stolen doc comment (CF-113-7)"
```

---

## Task 2: Widen `http_to_grpc_status` to `pub`, narrowly (PV-3)

**Files:**
- Modify: `crates/envoy-http1/src/grpc.rs`
- Modify: `crates/envoy-http1/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `envoy_http1::http_to_grpc_status(status: u16) -> u8` — the phase-110 map, reachable from `envoy-http2`.

**This DEPARTS from phase 113's `ADR-0193` DECISION 6, and the departure is deliberate.** That decision refused a visibility widening because `envoy-accesslog` is a LEAF crate and calling `envoy_http1::grpc` from it would be a dependency **cycle**. That argument is sound and untouched — but it does not transfer here, because the caller is `envoy-http2`, which **already depends on `envoy-http1`** and names `envoy_http1::` at 76 sites. The cycle does not exist on this edge.

The standing objection that DOES apply is the module doc at `crates/envoy-http1/src/grpc.rs`: *"This module is `pub(crate)` ON PURPOSE … anything reachable from the shared route-decision path would also rewrite HTTP/2 responses."* Read literally, that hazard is about `apply_grpc_local_reply` — a MUTATING transform reachable from `build_response`. `http_to_grpc_status` is a pure `u16 -> u8` lookup with no side effects and no reachability from `build_response`. **So the widening is item-level only: the MODULE stays `pub(crate)`, and exactly one function is re-exported.** `is_grpc_request` and `apply_grpc_local_reply` remain unreachable from outside the crate, which is what the module doc actually protects.

The alternative — copying the eight-entry table into `envoy-http2` — was rejected: two copies of a MEASURED table drift silently, and `SPEC.md` §4 item 4 says reusing the map rather than duplicating it "is the point".

- [ ] **Step 1: Write the failing test**

Add to `crates/envoy-http2/src/hcm.rs`, at EOF:

```rust
// ── Phase 114: the H2 arm of the ungated derivation (CF-114-3) ──────────────
#[cfg(test)]
mod h2_grpc_status_code_tests {
    #[test]
    fn the_phase_110_map_is_reachable_from_http2() {
        assert_eq!(envoy_http1::http_to_grpc_status(404), 12);
        assert_eq!(envoy_http1::http_to_grpc_status(200), 2);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-http2 --lib the_phase_110_map_is_reachable`
Expected: FAIL to COMPILE — `` function `http_to_grpc_status` is private `` / `` no function `http_to_grpc_status` in the root ``.

- [ ] **Step 3: Widen, narrowly**

In `crates/envoy-http1/src/grpc.rs`, replace the `pub(crate)` on the map (and ONLY on the map) with:

```rust
// Phase 114: item-level `pub` so `envoy-http2`'s access-log record build can
// reuse ONE map rather than copy the table. The MODULE stays `pub(crate)`; only
// this pure `u16 -> u8` lookup is re-exported (see `lib.rs`). The module doc's
// hazard is about reachability from the shared `build_response` route-decision
// path, which `apply_grpc_local_reply` is on and this function is not.
pub fn http_to_grpc_status(status: u16) -> u8 {
```

In `crates/envoy-http1/src/lib.rs`, add ONE line immediately above `pub use response::{Http1Response, Response};`:

```rust
pub use grpc::http_to_grpc_status; // 114: the ONE HTTP->gRPC map, shared with envoy-http2.
```

⚠ Leave `pub(crate) mod grpc;` and its three-line gate comment EXACTLY as they are. A `pub use` re-exports an item out of a `pub(crate)` module without widening the module.

- [ ] **Step 4: Run the test to verify it passes, and that nothing ELSE leaked**

Run:
```bash
cargo test -p envoy-http2 --lib the_phase_110_map_is_reachable
grep -c 'pub(crate) mod grpc;' crates/envoy-http1/src/lib.rs
grep -c 'pub(crate) fn is_grpc_request' crates/envoy-http1/src/grpc.rs
grep -c 'pub(crate) fn apply_grpc_local_reply' crates/envoy-http1/src/grpc.rs
```
Expected: PASS, then `1`, `1`, `1` — the module and the two mutating/gating items are untouched.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/grpc.rs crates/envoy-http1/src/lib.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 114 task 2: narrow pub on http_to_grpc_status so both codecs share ONE map (PV-3)"
```

---

## Task 3: The `GrpcStatusFilter` config surface and the MEASURED token grammar

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs`
- Modify: `crates/envoy-config/src/lib.rs` (three re-exports only)
- Test: `crates/envoy-config/src/bootstrap.rs`'s `mod tests`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub struct GrpcStatusFilter { pub statuses: Vec<GrpcStatusToken>, pub exclude: bool }`; `pub enum GrpcStatusToken { Num(i64), Name(String) }`; `pub(crate) const GRPC_STATUS_FILTER_NAMES: [&str; 17]`; `pub fn resolve_grpc_status_token(tok: &GrpcStatusToken) -> Option<u8>`; and `AccessLogFilter.grpc_status_filter: Option<GrpcStatusFilter>`.

**PV-2 is discharged here, and the answer is an untagged enum.** The `statuses` list must hold, in ONE list, bare identifiers (`NOT_FOUND`), bare integers (`5`) and quoted integers (`"5"`). `serde_yaml` 0.9.34 routes an unquoted scalar through `visit_untagged_scalar`, so a bare `5` arrives as an integer and a quoted `"5"` arrives as a string — an untagged enum can therefore distinguish and accept both. The landed precedent is `RuntimeValue`, whose doc states the rule this type inherits: **arm order is load-bearing; the integer arm must come first.**

**The YAML-version trap does NOT bite.** `serde_yaml`'s `parse_bool` accepts exactly `true|True|TRUE|false|False|FALSE`, so bare `TRUE` booleanizes on THIS side too and the upstream rejection reproduces. `y`/`n`/`on`/`off`/`yes`/`no` are YAML-1.1 booleans upstream but plain STRINGS here (the landed `CF-108-4` divergence) — yet both sides still REJECT them: upstream because the boolean never reaches the enum parser, here because the string resolves to no token. Same verdict, different error class, which is the `ADR-0049` fail-loud parity posture. MEASURED both directions.

- [ ] **Step 1: Write the failing tests**

Add to `crates/envoy-config/src/bootstrap.rs`'s `mod tests`:

```rust
    #[test]
    fn grpc_status_filter_accepts_every_measured_token() {
        // MEASURED against envoyproxy/envoy:v1.33.0 with `--mode validate`, one
        // run per token, against a negative control that REJECTS a bogus arm
        // name. All 17 canonical names, their case-insensitive spellings, the
        // integers 0-16, and the PERMISSIVE string-numeric forms.
        for tok in [
            "OK",
            "CANCELED",
            "UNKNOWN",
            "INVALID_ARGUMENT",
            "DEADLINE_EXCEEDED",
            "NOT_FOUND",
            "ALREADY_EXISTS",
            "PERMISSION_DENIED",
            "RESOURCE_EXHAUSTED",
            "FAILED_PRECONDITION",
            "ABORTED",
            "OUT_OF_RANGE",
            "UNIMPLEMENTED",
            "INTERNAL",
            "UNAVAILABLE",
            "DATA_LOSS",
            "UNAUTHENTICATED",
            "not_found",
            "ok",
            "Ok",
            "oK",
            "0",
            "5",
            "16",
            "05",
            "+5",
            " 5",
            "5 ",
        ] {
            assert!(
                resolve_grpc_status_token(&GrpcStatusToken::Name(tok.into())).is_some(),
                "MEASURED ACCEPT upstream, must resolve here: {tok:?}"
            );
        }
        for n in 0..=16i64 {
            assert!(resolve_grpc_status_token(&GrpcStatusToken::Num(n)).is_some());
        }
        // Index IS the code, and code 1 is CANCELED with ONE L.
        assert_eq!(
            resolve_grpc_status_token(&GrpcStatusToken::Name("CANCELED".into())),
            Some(1)
        );
        assert_eq!(
            resolve_grpc_status_token(&GrpcStatusToken::Name("unauthenticated".into())),
            Some(16)
        );
    }

    #[test]
    fn grpc_status_filter_rejects_every_measured_reject() {
        // MEASURED REJECT upstream. `CANCELLED` (two Ls) is the standing trap:
        // it is the code-1 SNAKE spelling of the %GRPC_STATUS% renderer and is
        // NOT a legal filter token.
        for tok in [
            "CANCELLED",
            "NotFound",
            "0x5",
            "",
            "1.0",
            "TRUE",
            "on",
            "y",
            "BOGUS_XYZ",
        ] {
            assert!(
                resolve_grpc_status_token(&GrpcStatusToken::Name(tok.into())).is_none(),
                "MEASURED REJECT upstream, must not resolve here: {tok:?}"
            );
        }
        for n in [-1i64, 17, 99] {
            assert!(resolve_grpc_status_token(&GrpcStatusToken::Num(n)).is_none());
        }
    }

    #[test]
    fn grpc_status_filter_mixed_token_list_deserializes() {
        // PV-2: ONE list holding a bare identifier, a bare integer and a QUOTED
        // integer. Bare ints bind to `Num`; quoted ints arrive as `Name` and are
        // resolved by the permissive numeric parse.
        let yaml = r#"
grpc_status_filter:
  statuses: [NOT_FOUND, 5, "7"]
  exclude: true
"#;
        let f: AccessLogFilter = serde_yaml::from_str(yaml).expect("deserializes");
        let gsf = f
            .grpc_status_filter
            .as_ref()
            .expect("grpc_status_filter present");
        assert_eq!(
            gsf.statuses,
            vec![
                GrpcStatusToken::Name("NOT_FOUND".into()),
                GrpcStatusToken::Num(5),
                GrpcStatusToken::Name("7".into()),
            ]
        );
        assert!(gsf.exclude);
        assert_eq!(
            gsf.statuses
                .iter()
                .map(|t| resolve_grpc_status_token(t).unwrap())
                .collect::<Vec<_>>(),
            vec![5u8, 5, 7]
        );
    }

    #[test]
    fn grpc_status_filter_exclude_defaults_false() {
        let f: AccessLogFilter =
            serde_yaml::from_str("grpc_status_filter:\n  statuses: [OK]\n").expect("deserializes");
        assert!(!f.grpc_status_filter.as_ref().unwrap().exclude);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config --lib grpc_status_filter`
Expected: FAIL to COMPILE — `` cannot find type `GrpcStatusToken` `` and `` no field `grpc_status_filter` on type `AccessLogFilter` ``.

- [ ] **Step 3: Write the implementation**

In `crates/envoy-config/src/bootstrap.rs`, append a seventh field to `AccessLogFilter` (immediately after `pub metadata_filter: Option<MetadataFilter>,`), then add the four new items after the struct's closing brace:

```rust
    /// Phase 114: the SEVENTH `AccessLogFilter` arm — gates emission on the
    /// request's EFFECTIVE gRPC status (response `grpc-status` header if
    /// present, else the phase-110 HTTP->gRPC map over the response code).
    /// Mutually exclusive with the other arms.
    pub grpc_status_filter: Option<GrpcStatusFilter>,
}

/// Phase 114: `envoy.config.accesslog.v3.GrpcStatusFilter`. `statuses` holds
/// canonical SCREAMING_SNAKE names (case-insensitively) or integers 0-16;
/// `exclude` inverts the membership test. An EMPTY `statuses` keeps NOTHING.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct GrpcStatusFilter {
    pub statuses: Vec<GrpcStatusToken>,
    pub exclude: bool,
}

/// One `GrpcStatusFilter.statuses` entry as it arrives from YAML. Untagged, and
/// the arm ORDER is load-bearing exactly as `RuntimeValue`'s is: a bare YAML
/// integer must bind to `Num` before `Name` is tried.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GrpcStatusToken {
    Num(i64),
    Name(String),
}

/// The `GrpcStatusFilter.Status` canonical names, MEASURED against
/// `envoyproxy/envoy:v1.33.0` via `--mode validate` over all 17 names plus a
/// reject set. Index IS the code.
///
/// Code 1 is `CANCELED` with ONE L here. That is the OPPOSITE of the
/// `%GRPC_STATUS(SNAKE_STRING)%` rendering table in
/// `crates/envoy-accesslog/src/command_operator.rs`, whose code-1 snake cell is
/// `CANCELLED` with TWO. They are two different upstream enums; `CANCELLED` is
/// REJECTED here (MEASURED). Do NOT unify the two tables.
pub(crate) const GRPC_STATUS_FILTER_NAMES: [&str; 17] = [
    "OK",
    "CANCELED",
    "UNKNOWN",
    "INVALID_ARGUMENT",
    "DEADLINE_EXCEEDED",
    "NOT_FOUND",
    "ALREADY_EXISTS",
    "PERMISSION_DENIED",
    "RESOURCE_EXHAUSTED",
    "FAILED_PRECONDITION",
    "ABORTED",
    "OUT_OF_RANGE",
    "UNIMPLEMENTED",
    "INTERNAL",
    "UNAVAILABLE",
    "DATA_LOSS",
    "UNAUTHENTICATED",
];

/// Resolve one `statuses` token to its code, or `None` if it is not a legal
/// token. MEASURED rule: an integer (YAML-native or string-spelled) must be in
/// 0..=16; the string form is parsed PERMISSIVELY with `trim().parse::<i64>()`,
/// so `" 5"`, `"5 "`, `"05"` and `"+5"` all ACCEPT upstream. A non-numeric
/// string matches a canonical name case-INSENSITIVELY, underscores required
/// (`not_found` ACCEPTS, `NotFound` REJECTS).
pub fn resolve_grpc_status_token(tok: &GrpcStatusToken) -> Option<u8> {
    let as_code = |n: i64| -> Option<u8> { (0..=16).contains(&n).then_some(n as u8) };
    match tok {
        GrpcStatusToken::Num(n) => as_code(*n),
        GrpcStatusToken::Name(s) => {
            if let Ok(n) = s.trim().parse::<i64>() {
                return as_code(n);
            }
            GRPC_STATUS_FILTER_NAMES
                .iter()
                .position(|n| n.eq_ignore_ascii_case(s))
                .map(|i| i as u8)
        }
    }
}
```

In `crates/envoy-config/src/lib.rs`, add ONE line to the `pub use bootstrap::{…}` block, immediately after the line ending `Admin, AndFilter,`:

```rust
    GrpcStatusFilter, GrpcStatusToken, resolve_grpc_status_token,
```

- [ ] **Step 4: Fix the `E0063` blast radius — and only the exhaustive literals**

Adding a `pub` field to `AccessLogFilter` is a cross-crate break. Find every site:

```bash
cargo build --workspace --all-targets 2>&1 | grep -B2 'missing field `grpc_status_filter`'
```

Add `grpc_status_filter: None,` to each **exhaustive** literal. ⚠ **Do NOT add it to a literal that already carries `..AccessLogFilter::default()` or `..Default::default()`** — those absorb the field silently, and an explicit `None` beside a functional update is noise a reviewer will rightly flag. ⚠ **Do NOT match `AccessLogFilter {` textually**: that substring also occurs in `pub struct AccessLogFilter {`, in `let AccessLogFilter { … } = filter;`, in the return type `-> AccessLogFilter {`, and inside `ConfigError::AmbiguousAccessLogFilter { detail }`. All four are false positives and all four were hit at the prototype. Work from the compiler's own error list.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-config --lib grpc_status_filter`
Expected: PASS, 4 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs crates/envoy-http1/src/hcm.rs
git commit -m "phase 114 task 3: the GrpcStatusFilter config arm and its MEASURED token grammar"
```

---

## Task 4: Validation — the seventh arm and a fail-loud bad-token error

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`validate_access_log_filter` and its contract doc)
- Modify: `crates/envoy-config/src/lib.rs` (one new `ConfigError` variant)
- Test: `crates/envoy-config/src/bootstrap.rs`'s `mod tests`

**Interfaces:**
- Consumes: `GrpcStatusFilter`, `GrpcStatusToken`, `resolve_grpc_status_token` (Task 3).
- Produces: `ConfigError::UnknownGrpcStatus { token: String }`.

The destructure in `validate_access_log_filter` has **no `..`**, so the compiler forces the seventh binding. The `set_arms` ARRAY is not length-checked and is the real hazard: an arm present in the struct but missing from the array counts as ZERO and turns a valid single-arm filter into `AmbiguousAccessLogFilter{"no filter variant is set"}`. The landed `six_arm_cardinality_counts_every_arm` test exists to catch exactly that, and it must grow to seven.

- [ ] **Step 1: Write the failing tests**

Rename `six_arm_cardinality_counts_every_arm` to `seven_arm_cardinality_counts_every_arm`, add a seventh entry to its `single_arms` vector and a seventh arm to its all-arms literal, and change its length assertion:

```rust
            AccessLogFilter {
                grpc_status_filter: Some(GrpcStatusFilter {
                    statuses: vec![GrpcStatusToken::Name("NOT_FOUND".into())],
                    exclude: false,
                }),
                ..AccessLogFilter::default()
            },
        ];
        assert_eq!(single_arms.len(), 7, "seven arms must be covered");
```

Then add two new tests:

```rust
    #[test]
    fn grpc_status_filter_bad_token_is_fail_loud() {
        let yaml = access_log_filter_yaml(
            "grpc_status_filter:\n                        statuses: [NOT_FOUND, CANCELLED]",
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("CANCELLED must be rejected");
        assert!(
            matches!(&err, crate::ConfigError::UnknownGrpcStatus { token } if token == "CANCELLED"),
            "got {err:?}"
        );
    }

    #[test]
    fn grpc_status_filter_empty_statuses_loads() {
        // MEASURED: `grpc_status_filter: {}` VALIDATES upstream (it keeps
        // nothing at runtime, which is a RUNTIME rule, not a load rule).
        let yaml = access_log_filter_yaml("grpc_status_filter: {}");
        crate::parse_bootstrap(&yaml).expect("empty grpc_status_filter must load");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config --lib grpc_status_filter_bad_token`
Expected: FAIL to COMPILE — `` no variant `UnknownGrpcStatus` ``.

- [ ] **Step 3: Add the error variant**

In `crates/envoy-config/src/lib.rs`, immediately after the `UnknownResponseFlag` variant (the exact analogue — it also validates a string token against a fixed set and carries the offender in the payload):

```rust
    /// Phase 114: a `grpc_status_filter.statuses` entry is neither a canonical
    /// gRPC status name (case-insensitive, underscores required) nor an integer
    /// in 0..=16. Upstream rejects the same class at load.
    #[error("grpc_status_filter statuses must be a known gRPC status token: {token}")]
    UnknownGrpcStatus { token: String },
```

- [ ] **Step 4: Grow the validator**

Three edits in `validate_access_log_filter`. Add `grpc_status_filter,` as the seventh binding of the destructure; add `grpc_status_filter.is_some(),` as the seventh entry of `set_arms`; and add the token loop immediately before the closing `Ok(())`:

```rust
    // Phase 114: every `statuses` token must resolve. Fail-loud, first offender
    // wins, mirroring `UnknownResponseFlag`.
    if let Some(gsf) = grpc_status_filter {
        for token in &gsf.statuses {
            if resolve_grpc_status_token(token).is_none() {
                return Err(crate::ConfigError::UnknownGrpcStatus {
                    token: match token {
                        GrpcStatusToken::Num(n) => n.to_string(),
                        GrpcStatusToken::Name(s) => s.clone(),
                    },
                });
            }
        }
    }
    Ok(())
```

Also update `validate_access_logs`' numbered contract doc, whose item 3 states the arm count as SIX, to say SEVEN and name `grpc_status_filter` (phase 114). ⚠ Locate that prose by TEXT; it sits above the function, not inside it.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-config --lib`
Expected: PASS. The prototype's figure is **722 passed; 0 failed** for this crate at the end of the phase; at this task expect the baseline 716 plus the six tests added by Tasks 3 and 4.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 114 task 4: validate the seventh AccessLogFilter arm fail-loud on a bad status token"
```

---

## Task 5: The UNGATED derivation — `effective_grpc_status` and both record builds

**Files:**
- Modify: `crates/envoy-accesslog/src/record.rs` (the field + `test_baseline`)
- Modify: `crates/envoy-http1/src/hcm.rs` (the helper + the H1 record build + one E0063 literal)
- Modify: `crates/envoy-http2/src/hcm.rs` (the H2 record build)
- Modify: `crates/envoy-accesslog/src/file_sink.rs` (one E0063 literal)
- Test: `crates/envoy-http1/src/hcm.rs`, `crates/envoy-http2/src/hcm.rs`

**Interfaces:**
- Consumes: `envoy_http1::http_to_grpc_status` (Task 2).
- Produces: `AccessLogRecord.grpc_status_code: u8`; `envoy_http1::hcm::effective_grpc_status(response_headers: &[(String, String)], status: u16) -> u8`.

**This is the phase's real derivation work, and PV-4 changed its shape.** `SPEC.md` §5 non-goal 3 left open whether the response-header map is even live at the H2 record build, on the ground that `h2_grpc_status()` returns `None`. **MEASURED: it IS live.** That helper's doc is true of **trailers** only — `finalize_h2_stream` moves `trailers` into `send_envoy_response`, but it clones the response headers into `response_headers_for_log_owned` one line BEFORE that move, and that borrow is still alive at the record build (the existing `extract_upstream_service_time(response_headers_for_log)` call proves it by construction). **So H2 gets BOTH legs, not just the response-code leg**, and the two codecs share one helper.

The field is `u8`, NOT `Option<u8>`: §2.3 measured that a status is always defined, on every request, including plain HTTP. That is the whole finding of the phase.

- [ ] **Step 1: Write the failing tests**

Add to `crates/envoy-http1/src/hcm.rs` at EOF:

```rust
// ── Phase 114: the ungated effective-gRPC-status derivation ─────────────────
#[cfg(test)]
mod grpc_status_filter_tests {
    use super::*;

    fn hdrs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn derivation_prefers_the_response_header() {
        // MEASURED: a gRPC request against a 404 direct_response is rewritten by
        // the phase-110 transform to HTTP 200 + `grpc-status: 12`, and the filter
        // reads 12 — NOT `http_to_grpc_status(200)` = 2.
        assert_eq!(
            effective_grpc_status(&hdrs(&[("grpc-status", "12")]), 200),
            12
        );
        assert_eq!(effective_grpc_status(&hdrs(&[("Grpc-Status", "0")]), 200), 0);
    }

    #[test]
    fn derivation_falls_back_to_the_phase_110_map() {
        // MEASURED on plain-HTTP probes, which carry NO grpc-status header:
        // 404 -> 12, 400 -> 13, 200 -> 2 (the `_ => 2` default; upstream's map
        // has NO `200 => 0` arm).
        for (status, want) in [
            (404u16, 12u8),
            (400, 13),
            (200, 2),
            (503, 14),
            (401, 16),
            (403, 7),
        ] {
            assert_eq!(effective_grpc_status(&[], status), want, "status {status}");
        }
    }

    #[test]
    fn derivation_is_ungated_and_differs_from_the_phase_113_field() {
        // THE governing finding. A plain 404 with no gRPC content-type has
        // `record.grpc_status == None` (phase 113's GATED field) but an
        // effective status of 12. An implementation that reuses the phase-113
        // field cannot express this row.
        let req = hdrs(&[("host", "x")]); // no content-type at all
        assert!(!crate::grpc::is_grpc_request(&req));
        assert_eq!(effective_grpc_status(&[], 404), 12);
    }

    #[test]
    fn derivation_ignores_an_unparseable_or_out_of_range_header() {
        for bad in ["", "abc", "99", "-1", "1.0"] {
            assert_eq!(
                effective_grpc_status(&hdrs(&[("grpc-status", bad)]), 404),
                12,
                "unparseable/out-of-range {bad:?} must fall back to the map"
            );
        }
    }
}
```

And REPLACE the placeholder test Task 2 added at the end of `crates/envoy-http2/src/hcm.rs` with the real pin:

```rust
// ── Phase 114: the H2 arm of the ungated derivation (CF-114-3) ──────────────
#[cfg(test)]
mod h2_grpc_status_code_tests {
    // Pins that H2 uses the SAME derivation as H1 — one function, one map. The
    // H2 arm has NO differential witness (CF-114-3), so this in-process test is
    // its sole witness and a phase that changes H2's behaviour must edit it.
    #[test]
    fn h2_uses_the_shared_effective_status() {
        assert_eq!(
            envoy_http1::hcm::effective_grpc_status(&[], 404),
            12,
            "H2 derives through the same helper H1 does"
        );
        assert_eq!(
            envoy_http1::hcm::effective_grpc_status(
                &[("grpc-status".to_string(), "3".to_string())],
                200
            ),
            3
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-http1 -p envoy-http2 --lib grpc_status`
Expected: FAIL to COMPILE — `` cannot find function `effective_grpc_status` ``.

- [ ] **Step 3: Add the field**

In `crates/envoy-accesslog/src/record.rs`, append to `AccessLogRecord` immediately after `pub grpc_status: Option<String>,`:

```rust
    /// Phase 114: the UNGATED effective gRPC status code the `grpc_status_filter`
    /// access-log filter arm evaluates. MEASURED rule: the response `grpc-status`
    /// header if present, ELSE `http_to_grpc_status(response_code)`.
    ///
    /// This is NOT `grpc_status` above. That field is GATED on the REQUEST being
    /// a gRPC request and holds a raw string; this one is defined on EVERY
    /// request, including plain HTTP, which is the measured upstream behaviour of
    /// the filter. A plain 404 with no gRPC content-type has `grpc_status: None`
    /// and `grpc_status_code: 12`.
    pub grpc_status_code: u8,
```

and add `grpc_status_code: 2,` to `AccessLogRecord::test_baseline()` immediately after its `grpc_status: None,`.

- [ ] **Step 4: Add the shared helper**

In `crates/envoy-http1/src/hcm.rs`, immediately ABOVE `fn build_access_log_record(`:

```rust
/// Phase 114: the UNGATED effective gRPC status. MEASURED on both proxies: the
/// response `grpc-status` header if present and parseable, else the phase-110
/// `http_to_grpc_status` map over the response code. Shared by both codecs.
///
/// An unparseable or out-of-range header value falls back to the response-code
/// map — upstream's own source of the value is an optional parse, and a fixture
/// cannot set an arbitrary `grpc-status` on either proxy (CF-114-5).
pub fn effective_grpc_status(response_headers: &[(String, String)], status: u16) -> u8 {
    match access_log_header_value(response_headers, crate::headers::GRPC_STATUS)
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|n| (0..=16).contains(n))
    {
        Some(n) => n as u8,
        None => crate::grpc::http_to_grpc_status(status),
    }
}
```

- [ ] **Step 5: Populate at both record builds**

In `crates/envoy-http1/src/hcm.rs`, inside `build_access_log_record`'s literal, immediately after the phase-113 `grpc_status: if … { … } else { None },` block:

```rust
        // phase 114: the UNGATED effective status the `grpc_status_filter` arm
        // reads. Deliberately NOT gated on `is_grpc_request` — MEASURED: a plain
        // 404 with no gRPC content-type is KEPT by a sink filtered on
        // UNIMPLEMENTED. The header leg wins when present; otherwise the
        // phase-110 map over the response code.
        grpc_status_code: effective_grpc_status(response.headers, response.status),
```

In `crates/envoy-http2/src/hcm.rs`, inside the `AccessLogRecord` literal, immediately after `grpc_status: h2_grpc_status(),`:

```rust
            // Phase 114: the UNGATED effective status. Unlike `grpc_status`
            // above (a TRAILER-sourced value that CF-113-2 leaves absent on H2),
            // this reads the RESPONSE HEADERS, which ARE live at this point:
            // `response_headers_for_log_owned` is cloned before the `resp` move.
            grpc_status_code: envoy_http1::hcm::effective_grpc_status(
                response_headers_for_log,
                response_status_for_log,
            ),
```

- [ ] **Step 6: Fix the `E0063` blast radius — FOUR exhaustive literals**

```bash
cargo build --workspace --all-targets 2>&1 | grep -A3 'missing field `grpc_status_code`'
```
The four are: the H1 production build (Step 5), the H2 production build (Step 5), `record.rs`'s `test_baseline` (Step 3), and one test literal each in `crates/envoy-accesslog/src/file_sink.rs` and `crates/envoy-http1/src/hcm.rs` — add `grpc_status_code: 2,` to the latter two. ⚠ Literals that use `..base` functional-update syntax absorb the field silently and must NOT be edited. ⚠ **Do NOT match `AccessLogRecord {` textually** — that substring also occurs in the return types `-> AccessLogRecord {` and `-> envoy_accesslog::AccessLogRecord {`, both of which were hit at the prototype. Work from the compiler's list.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p envoy-accesslog -p envoy-http1 -p envoy-http2 --lib`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/envoy-accesslog/src/record.rs crates/envoy-accesslog/src/file_sink.rs crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 114 task 5: the UNGATED effective-gRPC-status derivation on both codecs (PV-4)"
```

---

## Task 6: The fifth `should_log` widening — behaviour-neutral

**Files:**
- Modify: `crates/envoy-accesslog/src/filter.rs` (the definition + 2 production recursion sites + 66 test sites)
- Modify: `crates/envoy-accesslog/src/file_sink.rs` (the definition + 1 production delegation + 4 test sites)
- Modify: `crates/envoy-http1/src/hcm.rs` (1 production site + 52 test sites)
- Modify: `crates/envoy-http2/src/hcm.rs` (1 production site)

**Interfaces:**
- Consumes: `AccessLogRecord.grpc_status_code` (Task 5).
- Produces: `LogFilter::should_log(&self, status: u16, response_flags: &str, headers: &[(String, String)], dynamic_metadata: &BTreeMap<…>, grpc_status_code: u8) -> bool` and the matching `FileSink::should_log`.

**This is its own task on the phase-74 `T3` precedent** (commit `796450d`, *"widen should_log with the dynamic-metadata store (behavior-neutral)"* — 4 files, +152/−91, net +61 over `crates/`). It changes no behaviour: every existing arm ignores the new parameter, and Task 7 lands the arm that consumes it.

⚠ **`SPEC.md` is wrong about the call-site split.** There are **127** `.should_log(` occurrences and **FIVE** of them are production, not two: the two HCM dispatch sites the SPEC names, PLUS `filter.rs:148`, `filter.rs:151` (the `And`/`Or` recursion) and `file_sink.rs:114` (the `FileSink` → `LogFilter` delegation). The remaining **122** are test. Locate all five by TEXT, not by line number.

⚠ **Do NOT add `#[allow(clippy::only_used_in_recursion)]`.** The phase-74 precedent needed it because its widening task landed a parameter no arm consumed. Here Task 7 lands the consuming arm, and the prototype passed `-D warnings` at the end of the phase without it. If the executor gates Task 6 in isolation and clippy fires that lint, add the allow with a comment saying it is TRANSIENT and REMOVE it in Task 7 — do not leave it.

- [ ] **Step 1: Widen the two definitions**

In `crates/envoy-accesslog/src/filter.rs`, add a sixth parameter to `LogFilter::should_log` after `dynamic_metadata`:

```rust
        grpc_status_code: u8,
```

and thread it through the two recursion arms so they read:

```rust
            LogFilter::And(filters) => filters.iter().all(|f| {
                f.should_log(
                    status,
                    response_flags,
                    headers,
                    dynamic_metadata,
                    grpc_status_code,
                )
            }),
            LogFilter::Or(filters) => filters.iter().any(|f| {
                f.should_log(
                    status,
                    response_flags,
                    headers,
                    dynamic_metadata,
                    grpc_status_code,
                )
            }),
```

In `crates/envoy-accesslog/src/file_sink.rs`, add the same parameter to `FileSink::should_log` and forward it:

```rust
        grpc_status_code: u8,
    ) -> bool {
        match &self.filter {
            Some(f) => f.should_log(
                status,
                response_flags,
                headers,
                dynamic_metadata,
                grpc_status_code,
            ),
            None => true,
        }
    }
```

Update both doc comments to say `Phase 70/71/72/73/74/114` and to name the new parameter.

- [ ] **Step 2: Widen the two production dispatch sites**

Both already use the multiline call form, so each is a one-line insert. In `crates/envoy-http1/src/hcm.rs`, after `&record.dynamic_metadata,`:

```rust
                    record.grpc_status_code,
```

In `crates/envoy-http2/src/hcm.rs`, after `&record.dynamic_metadata,`:

```rust
                record.grpc_status_code,
```

- [ ] **Step 3: Sweep the 122 test sites**

Every test site needs exactly one more argument appended. Use the neutral literal `2` (UNKNOWN) — the existing arms ignore it, so any value works and a literal keeps the diff minimal.

⚠ **A single `sed` will not do this correctly.** The last argument has eight distinct spellings (`&Default::default()`, `&md`, `&hit`, `&miss`, `&empty`, `&other_key`, `&other_ns`, and three call expressions such as `&md("com.example", "k", "1")`), three of which contain **nested parentheses**, so a `\)`-anchored regex must balance. Ten sites are already in the multiline form and need a trailing comma added to the last argument plus a new argument line. Walk the file with a paren-matching pass instead:

```python
# scratch script — find each `.should_log(`, brace-match to its closing paren,
# and insert the new argument before it.
i = 0
while True:
    j = t.find('.should_log(', i)
    if j < 0:
        break
    k = j + len('.should_log(')
    depth, pos = 1, k
    while depth > 0:
        c = t[pos]
        if c == '(':
            depth += 1
        elif c == ')':
            depth -= 1
        pos += 1
    close = pos - 1
    inner = t[k:close]
    if 'grpc_status_code' in inner:      # already widened
        i = pos
        continue
    new = inner + ', 2' if '\n' not in inner else <multiline form>
    t = t[:k] + new + t[close:]
    i = k + len(new)
```

Then run `cargo fmt --all` — five inline sites cross rustfmt's `fn_call_width` of 60 after the append and will be reflowed into the multiline form. **Run `fmt` as part of THIS task, not as a follow-up**; the phase-72 precedent shows a later commit had to clean up what a widening left behind.

- [ ] **Step 4: Add the behaviour-neutrality pin**

In `crates/envoy-accesslog/src/filter.rs`'s `mod tests`:

```rust
    #[test]
    fn existing_arms_ignore_the_grpc_status_argument() {
        // The phase-74 T3 behaviour-neutrality pin, repeated for the fifth
        // parameter: every pre-phase-114 arm must be blind to it.
        for code in [0u8, 2, 12, 16] {
            assert!(ge(500).should_log(503, "-", &[], &Default::default(), code));
            assert!(!ge(500).should_log(499, "-", &[], &Default::default(), code));
            assert!(rf(&["NR"]).should_log(404, "NR", &[], &Default::default(), code));
            assert!(!rf(&["NR"]).should_log(404, "UH", &[], &Default::default(), code));
        }
    }
```

- [ ] **Step 5: Verify — count, build, lint, test**

```bash
grep -c '\.should_log(' crates/envoy-accesslog/src/filter.rs crates/envoy-accesslog/src/file_sink.rs crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test -p envoy-accesslog -p envoy-http1 -p envoy-http2 --lib
```
Expected counts: `68`, `5`, `53`, `1` — unchanged from before the sweep, because the sweep adds arguments and not call sites. All four commands clean.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-accesslog/src/filter.rs crates/envoy-accesslog/src/file_sink.rs crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 114 task 6: widen should_log with the effective gRPC status (behavior-neutral)"
```

---

## Task 7: `LogFilter::GrpcStatus` and its `should_log` arm

**Files:**
- Modify: `crates/envoy-accesslog/src/filter.rs`
- Test: same file, `mod tests`

**Interfaces:**
- Consumes: the fifth `should_log` parameter (Task 6).
- Produces: `LogFilter::GrpcStatus { codes: Vec<u8>, exclude: bool }`.

**The arm carries PLAIN DATA, not a trait object.** `envoy-accesslog` depends only on `tokio`, `bytes`, `tracing` and `thiserror` — deliberately, to avoid a cycle with `envoy-config` — so the `Header` and `Metadata` arms inject an `Arc<dyn …>` through the `ADR-0150` seam. This arm needs no `envoy-config` type at all: the compile step resolves every token to an integer before the runtime ever sees it. That makes this arm **simpler** than phase 72's or phase 74's, not harder, and `ADR-0150` is not involved.

The predicate is `codes.contains(&grpc_status_code) != *exclude` — one expression covering both directions. An empty `codes` makes `contains` false for every record, so `exclude: false` keeps nothing and `exclude: true` keeps everything, which is exactly what was MEASURED.

- [ ] **Step 1: Write the failing tests**

Add to `crates/envoy-accesslog/src/filter.rs`'s `mod tests`:

```rust
    // ── Phase 114: the `GrpcStatus` arm ───────────────────────────────────────
    #[test]
    fn grpc_status_arm_is_membership_over_the_effective_code() {
        let f = LogFilter::GrpcStatus {
            codes: vec![12, 13],
            exclude: false,
        };
        assert!(f.should_log(200, "-", &[], &Default::default(), 12));
        assert!(f.should_log(404, "-", &[], &Default::default(), 13));
        assert!(!f.should_log(200, "-", &[], &Default::default(), 2));
        assert!(!f.should_log(503, "-", &[], &Default::default(), 14));
    }

    #[test]
    fn grpc_status_arm_exclude_inverts_over_the_same_code() {
        // MEASURED upstream: sink DDD (`exclude: true`) kept every probe whose
        // derived status was NOT 12 and dropped both whose status WAS 12 —
        // including a plain-HTTP 404. The inversion is over the SAME value.
        let inc = LogFilter::GrpcStatus {
            codes: vec![12],
            exclude: false,
        };
        let exc = LogFilter::GrpcStatus {
            codes: vec![12],
            exclude: true,
        };
        for code in 0..=16u8 {
            assert_ne!(
                inc.should_log(200, "-", &[], &Default::default(), code),
                exc.should_log(200, "-", &[], &Default::default(), code),
                "exclude must invert at code {code}"
            );
        }
    }

    #[test]
    fn grpc_status_arm_empty_statuses_keeps_nothing() {
        // MEASURED: sink EEE (`grpc_status_filter: {}`) kept NOTHING on any of
        // the eight probes. An empty list is not "match everything".
        let f = LogFilter::GrpcStatus {
            codes: vec![],
            exclude: false,
        };
        for code in 0..=16u8 {
            assert!(!f.should_log(200, "-", &[], &Default::default(), code));
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-accesslog --lib grpc_status_arm`
Expected: FAIL to COMPILE — `` no variant named `GrpcStatus` found for enum `LogFilter` ``.

- [ ] **Step 3: Add the variant and the arm**

In `crates/envoy-accesslog/src/filter.rs`, append to `LogFilter` after the `Metadata` variant:

```rust
    /// Phase 114: emit a record iff its EFFECTIVE gRPC status is in `codes` —
    /// or, when `exclude` is set, iff it is NOT. `codes` is already resolved to
    /// integers by the compile step. An EMPTY `codes` with `exclude: false`
    /// keeps NOTHING (MEASURED); with `exclude: true` it keeps EVERYTHING.
    /// Carries plain data, not a trait object: this arm needs no `envoy-config`
    /// type, so the ADR-0150 cycle seam is not involved.
    GrpcStatus { codes: Vec<u8>, exclude: bool },
```

and append the matching arm to `should_log`'s `match self`, after the `Metadata` arm:

```rust
            // Phase 114: membership over the UNGATED effective status, inverted
            // by `exclude`. Empty `codes` => contains() is false for every
            // record, so `exclude: false` keeps nothing (MEASURED).
            LogFilter::GrpcStatus { codes, exclude } => {
                codes.contains(&grpc_status_code) != *exclude
            }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-accesslog --lib`
Expected: PASS. On the prototype this crate finishes the phase at **133 passed; 0 failed**.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-accesslog/src/filter.rs
git commit -m "phase 114 task 7: LogFilter::GrpcStatus — plain-integer membership with exclude inversion"
```

---

## Task 8: `compile_access_log_filter` — the seventh tuple arm

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs`
- Test: same file, `mod grpc_status_filter_tests`

**Interfaces:**
- Consumes: `envoy_config::GrpcStatusFilter` + `resolve_grpc_status_token` (Task 3), `LogFilter::GrpcStatus` (Task 7).
- Produces: nothing new — it widens the existing six-tuple match to seven.

The validator has already proved every token resolves, so the compile step may `expect()`. That is the same posture the `header_filter` and `metadata_filter` arms take with their pre-compiled `SafeRegex`.

- [ ] **Step 1: Write the failing test**

Add to `crates/envoy-http1/src/hcm.rs`'s `mod grpc_status_filter_tests` (created in Task 5):

```rust
    #[test]
    fn compile_produces_the_seventh_arm_with_resolved_codes() {
        let f = envoy_config::AccessLogFilter {
            grpc_status_filter: Some(envoy_config::GrpcStatusFilter {
                statuses: vec![
                    envoy_config::GrpcStatusToken::Name("UNIMPLEMENTED".into()),
                    envoy_config::GrpcStatusToken::Num(13),
                    envoy_config::GrpcStatusToken::Name("14".into()),
                ],
                exclude: true,
            }),
            ..Default::default()
        };
        match compile_access_log_filter(&f) {
            envoy_accesslog::LogFilter::GrpcStatus { codes, exclude } => {
                assert_eq!(codes, vec![12, 13, 14]);
                assert!(exclude);
            }
            other => panic!("expected the GrpcStatus arm, got {other:?}"),
        }
    }
```

Note the three tokens deliberately cover all three input shapes the grammar accepts: a canonical NAME, a bare INTEGER, and a string-spelled integer.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-http1 --lib compile_produces_the_seventh_arm`
Expected: FAIL — the `_ => unreachable!(…)` arm panics with *"validated by validate_access_logs: exactly one filter arm is set"*, because the six-tuple does not yet include `grpc_status_filter`.

- [ ] **Step 3: Widen the match**

In `compile_access_log_filter`, add `&f.grpc_status_filter,` as the seventh element of the scrutinee tuple, add a seventh `None` to each of the six existing patterns, and add the new arm immediately before the `_ =>` fallback:

```rust
        // Phase 114: the seventh arm. `statuses` tokens are resolved to codes
        // here (the validator already proved every one resolves), so the runtime
        // predicate is a plain integer membership test.
        (None, None, None, None, None, None, Some(gsf)) => envoy_accesslog::LogFilter::GrpcStatus {
            codes: gsf
                .statuses
                .iter()
                .map(|t| {
                    envoy_config::resolve_grpc_status_token(t)
                        .expect("validated by validate_access_logs: every status token resolves")
                })
                .collect(),
            exclude: gsf.exclude,
        },
```

Update the function's doc comment: it currently says *"SIX arms ship"* — make it SEVEN and name `grpc_status_filter` (phase 114).

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p envoy-http1 --lib && cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: PASS, clippy silent. On the prototype `envoy-http1` finishes the phase at **243 passed; 0 failed**.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 114 task 8: compile_access_log_filter grows to seven arms"
```

---

## Task 9: Differential fixture `0094-accesslog-grpc-status-filter`

**Files:**
- Create: `tests/fixtures/0094-accesslog-grpc-status-filter/envoy-rust.yaml`
- Create: `tests/fixtures/0094-accesslog-grpc-status-filter/envoy.yaml`
- Create: `tests/fixtures/0094-accesslog-grpc-status-filter/expectations.yaml`
- Create: `tests/fixtures/0094-accesslog-grpc-status-filter/README.md`
- Create: `tests/differential/tests/accesslog_grpc_status_filter.rs`

**Interfaces:**
- Consumes: everything above.
- Produces: nothing. **`tests/differential/src/lib.rs` is NOT touched** — the driver, `expect_logged` and `extra_headers` all already exist.

**Numbering re-derived at this PLAN-write:** `tests/fixtures/` holds 93 directories, git-tracked and on-disk lists identical, contiguous `0001`…`0093`. `0094` is free.

**Authoring constraints, all inherited and all load-bearing.** No route-level `response_headers_to_add` (envoy-rust's hand-rolled `Route` visitor rejects the key by name — boot-fatal). `direct_response.body` is MANDATORY on every route (`CF-110-7`; `DirectResponse.body` is `DataSource`, not `Option`). The format string must NOT echo `content-type` (`%REQ(NAME)%` is allow-list gated; `:path` IS on the list). Every probe gets a DISTINCT path. The LAST probe is KEPT, so the fixture pays the short suppression settle rather than the long one. `{{PORT}}` is the only token — `Http1AccessLogByteExact` is not one of the drivers that receive `{{ADMIN_PORT}}`, so the upstream `admin:` block uses a literal `port_value: 0`.

⚠ **The SPEC's probe table is corrected here (SPEC correction 3).** Probes 1, 2 and 4 send a gRPC content-type, so the phase-110 transform rewrites the status to **200**; their `expected_status` is 200, not the route's. And probes 1 and 2 witness the **header** leg, which SPEC correction 4 establishes IS expressible after all.

- [ ] **Step 1: Write `envoy-rust.yaml`**

```yaml
node: { id: envoy-rust-phase-114-fixture-0094, cluster: envoy-rust-phase-114 }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                access_log:
                  - name: envoy.access_loggers.file
                    filter:
                      grpc_status_filter:
                        statuses: [UNIMPLEMENTED, INTERNAL]
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0094-envoy-rust-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "PATH=%REQ(:PATH)% CODE=%RESPONSE_CODE% GS=%GRPC_STATUS%\n"
                    # Phase 114 — the `grpc_status_filter` arm. `statuses` is
                    # [UNIMPLEMENTED, INTERNAL] = codes 12 and 13.
                    #
                    # The line renders `%GRPC_STATUS%` (phase 113's GATED value)
                    # beside the path so the kept lines WITNESS that the
                    # formatter's gated value and the filter's UNGATED value
                    # genuinely differ: probes 5, 6 and 8 are KEPT while
                    # `%GRPC_STATUS%` renders the `-` sentinel for them.
                    #
                    # It does NOT echo `content-type`: `%REQ(NAME)%` is
                    # ALLOW-LIST gated by the seven-name `REQ_ALLOW_LIST` in
                    # `crates/envoy-accesslog/src/command_operator.rs`, so
                    # `%REQ(CONTENT-TYPE)%` would be BOOT-FATAL. `:path` IS on
                    # that list, which is why every probe is attributed by path.
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        # Probes 1-4 carry `content-type: application/grpc`, so
                        # the phase-110 local-reply transform rewrites the status
                        # to 200 and emits `grpc-status: http_to_grpc_status(N)`.
                        # The filter then reads that HEADER. Probes 5-8 carry no
                        # gRPC content-type, so no header exists and the filter
                        # DERIVES from the response code. `body:` is MANDATORY on
                        # every route (CF-110-7).
                        - match: { path: "/g-unimpl" }
                          direct_response:
                            status: 404
                            body: { inline_string: "hi\n" }
                        - match: { path: "/g-internal" }
                          direct_response:
                            status: 400
                            body: { inline_string: "hi\n" }
                        - match: { path: "/g-unknown" }
                          direct_response:
                            status: 200
                            body: { inline_string: "hi\n" }
                        - match: { path: "/g-unavail" }
                          direct_response:
                            status: 503
                            body: { inline_string: "hi\n" }
                        - match: { path: "/p-unimpl" }
                          direct_response:
                            status: 404
                            body: { inline_string: "hi\n" }
                        - match: { path: "/p-internal" }
                          direct_response:
                            status: 400
                            body: { inline_string: "hi\n" }
                        - match: { path: "/p-unknown" }
                          direct_response:
                            status: 200
                            body: { inline_string: "hi\n" }
                        - match: { path: "/g-param" }
                          direct_response:
                            status: 404
                            body: { inline_string: "hi\n" }
                        - match: { prefix: "/" }
                          direct_response:
                            status: 404
                            body: { inline_string: "hi\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 2: Write `envoy.yaml` as that file plus exactly four harness hunks**

Copy `envoy-rust.yaml` and apply these four, and only these four. They are the same four every landed access-log byte-exact fixture carries (`0076`, `0078`, `0081`, `0093`), and none of them is semantic:

1. Insert `admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }` as line 2 (upstream needs an admin block; a literal `0`, never `{{ADMIN_PORT}}`).
2. Change the listener bind address from `127.0.0.1` to `0.0.0.0`.
3. Insert `                generate_request_id: false` immediately after `codec_type: HTTP1` (envoy-rust's HCM does not model the field and `deny_unknown_fields` rejects it).
4. Change the log path from `/tmp/0094-envoy-rust-mount/access.log` to `/tmp/0094-envoy-mount/access.log` (the driver bind-mounts only the upstream side's parent directory, so the two paths must live in different parents).

Verify with:

```bash
diff tests/fixtures/0094-accesslog-grpc-status-filter/envoy.yaml \
     tests/fixtures/0094-accesslog-grpc-status-filter/envoy-rust.yaml
```
Expected: exactly four hunks, at `2d1`, `6c5`, `14d12` and `22c20`. **The `filter:` block must be byte-identical on both sides.**

- [ ] **Step 3: Write `expectations.yaml`**

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0094-envoy-mount/access.log
    envoy_rust: /tmp/0094-envoy-rust-mount/access.log
  # The sink's filter is `grpc_status_filter: { statuses: [UNIMPLEMENTED,
  # INTERNAL] }` = codes 12 and 13. Eight probes, EACH ON A DISTINCT PATH
  # because identical paths cannot attribute a kept log line. Five are KEPT.
  #
  # ⚠ Probes 1-4 send `content-type: application/grpc`, so the phase-110
  # local-reply transform REWRITES the status to 200 and emits a
  # `grpc-status` header — `expected_status` is therefore 200 on all four,
  # NOT the route's `direct_response.status`. That was MEASURED on both
  # proxies at the PLAN-write; the landed SPEC §6 table says otherwise and
  # ADR-0197 corrects it.
  probes:
    # Probe 1 — KEPT via the HEADER leg. 404 + gRPC content-type -> the
    # transform emits `grpc-status: 12` and rewrites the code to 200. The
    # filter reads 12, which is in [12, 13]. Had it derived from the LOGGED
    # response code instead, it would have computed 2 and dropped this row.
    - method: get
      path: /g-unimpl
      host: envoy-rust.test
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expect_logged: true
    # Probe 2 — KEPT via the HEADER leg. 400 -> `grpc-status: 13`.
    - method: get
      path: /g-internal
      host: envoy-rust.test
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expect_logged: true
    # Probe 3 — SUPPRESSED. 200 -> `grpc-status: 2` (upstream's map has NO
    # `200 => 0` arm; 2 is the `_ => 2` default). 2 is not in [12, 13].
    - method: get
      path: /g-unknown
      host: envoy-rust.test
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expect_logged: false
    # Probe 4 — SUPPRESSED. 503 -> `grpc-status: 14`.
    - method: get
      path: /g-unavail
      host: envoy-rust.test
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expect_logged: false
    # Probe 5 — KEPT, and THE DECISIVE CELL. A plain HTTP 404 with no gRPC
    # content-type. `%GRPC_STATUS%` renders `-` for it (phase 113's field is
    # GATED and absent), yet the filter still derives 12 and KEEPS the row.
    # An implementation that reuses `AccessLogRecord.grpc_status` goes RED
    # here. MEASURED on both proxies.
    - method: get
      path: /p-unimpl
      host: envoy-rust.test
      expected_status: 404
      expect_logged: true
    # Probe 6 — KEPT via the DERIVATION leg, plain HTTP. 400 -> 13.
    - method: get
      path: /p-internal
      host: envoy-rust.test
      expected_status: 400
      expect_logged: true
    # Probe 7 — SUPPRESSED. Plain HTTP 200 -> 2.
    - method: get
      path: /p-unknown
      host: envoy-rust.test
      expected_status: 200
      expect_logged: false
    # Probe 8 — KEPT, and LAST so the fixture pays the SHORT suppression
    # settle. `application/grpc; charset=utf-8` is NOT gRPC to the phase-110
    # detector (a parameter defeats it), so the transform does NOT fire, the
    # status stays 404 and `%GRPC_STATUS%` renders `-` — yet the filter still
    # derives 12 and KEEPS the row. The second witness that the filter's gate
    # is not the formatter's.
    - method: get
      path: /g-param
      host: envoy-rust.test
      extra_headers:
        - ["content-type", "application/grpc; charset=utf-8"]
      expected_status: 404
      expect_logged: true
  # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). NO static literal:
  # the `http1_access_log_byte_exact` driver asserts each emitted line is
  # byte-identical between upstream Envoy v1.33.0 and envoy-rust, and that each
  # side emits EXACTLY the kept count. Both proxies must agree on BOTH halves of
  # every filter decision — a one-sided suppression fails the line-count
  # assertion before the byte compare is reached. Five lines per side, MEASURED
  # at the PLAN-write against both real proxies:
  #   PATH=/g-unimpl CODE=200 GS=Unimplemented
  #   PATH=/g-internal CODE=200 GS=Internal
  #   PATH=/p-unimpl CODE=404 GS=-
  #   PATH=/p-internal CODE=400 GS=-
  #   PATH=/g-param CODE=404 GS=-
  # Every route is a direct_response -> `clusters: []`, no backend spawns.
```

- [ ] **Step 4: Write the runner**

```rust
//! Fixture 0094 — the `grpc_status_filter` access-log FILTER arm (phase 114),
//! the SEVENTH of the twelve `AccessLogFilter` oneof arms.
//!
//! Cluster-free and backend-free (`clusters: []`, every route a
//! `direct_response`), so it is verifiable on a development host rather than
//! CI-only. Eight probes on eight distinct paths; five are kept.
//!
//! ⚠ Probes 5, 6 and 8 are what make this fixture non-vacuous. They are
//! requests the phase-113 `%GRPC_STATUS%` gate renders `-` for, and the filter
//! keeps them anyway — so an implementation that reuses
//! `AccessLogRecord.grpc_status` goes RED on all three.

use std::path::PathBuf;

#[tokio::test]
async fn accesslog_grpc_status_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0094-accesslog-grpc-status-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 5: Write `README.md`**

A fixture README in the house style (`0093`'s is the template). It must carry, as its own sections: the phase and arm identity; the driver and the cluster-free/backend-free property; the four MEASURED rules (header-then-derivation source, the ungated gate, `exclude` inversion, empty-list-keeps-nothing) with a note that the last two are in-process only and banked as `CF-114-4`; **why the gRPC probes expect HTTP 200** (the phase-110 transform); **why probes 5, 6 and 8 make the fixture non-vacuous**, naming the mutation and its result; the six authoring constraints from this task's preamble; and the per-fixture claim that the two YAMLs differ in exactly the four harness hunks with a byte-identical `filter:` block.

- [ ] **Step 6: Run the fixture GREEN against both real proxies**

```bash
cargo build -p envoy-bin            # the harness uses the DEBUG binary
cargo test -p differential --test accesslog_grpc_status_filter
```
Expected: PASS. The five kept lines, MEASURED byte-identical on both sides at this PLAN-write:

```
PATH=/g-unimpl CODE=200 GS=Unimplemented
PATH=/g-internal CODE=200 GS=Internal
PATH=/p-unimpl CODE=404 GS=-
PATH=/p-internal CODE=400 GS=-
PATH=/g-param CODE=404 GS=-
```

Note what the last three lines show: the record was KEPT while `%GRPC_STATUS%` rendered the `-` sentinel. That is the formatter's gated value and the filter's ungated value disagreeing on the same record, in the log file itself.

- [ ] **Step 7: MUTATION — prove the fixture is not vacuous (PV-7)**

⚠ **A test asserting the ungated behaviour passes vacuously against an implementation that never exercises it. The mutation IS the red evidence.** Run it in a scratch worktree with its OWN `CARGO_TARGET_DIR` (never the main tree's — a shared target directory poisons the test binary), and assert the target occurs EXACTLY ONCE before editing.

```bash
grep -cF '        grpc_status_code: effective_grpc_status(response.headers, response.status),' \
  crates/envoy-http1/src/hcm.rs      # must print exactly 1
```

Replace that one line with the gated form — the shape an implementation that reused `record.grpc_status` would produce:

```rust
        grpc_status_code: if crate::grpc::is_grpc_request(&request.req.headers) {
            effective_grpc_status(response.headers, response.status)
        } else {
            2
        },
```

Then `cargo build -p envoy-bin` (assert a `Compiling envoy-http1` line appears — a stale binary gives a FALSE PASS), delete both log files, and re-run the fixture.

Expected: **RED**, with exactly this failure:

```
fixture green: envoy-rust emitted 2 access-log lines but 5 were expected to be logged;
lines: ["PATH=/g-unimpl CODE=200 GS=Unimplemented", "PATH=/g-internal CODE=200 GS=Internal"]
```

The three lost lines are probes 5, 6 and 8 — precisely the ungated cells. Then restore the file, verify the restore by `md5sum` against a copy taken before the edit, rebuild, and re-run to confirm the unmutated control is **GREEN from the same tree**. A mutation RED without its control is not evidence.

- [ ] **Step 8: Commit**

```bash
git add tests/fixtures/0094-accesslog-grpc-status-filter tests/differential/tests/accesslog_grpc_status_filter.rs
git commit -m "phase 114 task 9: differential fixture 0094 — the grpc_status_filter arm, 8 probes"
```

---

## Task 10: `BEHAVIOR_CONTRACT.md` — the grammar and the runtime rule

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the access-log filter section phases 70–74 built)

This is the only `docs/` change in the phase and is EXCLUDED from the §6.1 LoC gate.

- [ ] **Step 1: Locate the section**

```bash
grep -n 'metadata_filter' docs/envoy-rust/BEHAVIOR_CONTRACT.md
```
⚠ Find the heading that owns the access-log FILTER arms and assert it occurs exactly once before editing. Do not guess the heading text — this file is 4694 lines and a substring match can land inside prose.

- [ ] **Step 2: Append the three sub-sections**

1. **The `statuses` token grammar.** Case-insensitive match against a canonical `SCREAMING_SNAKE` name, OR an integer in 0–16. The string form is parsed permissively — `" 5"`, `"5 "`, `"05"` and `"+5"` all accept, `"0x5"` and `""` do not. Duplicates permitted, no uniqueness bound. Rejects: `CANCELLED` (two Ls), `NotFound` (camel case — the underscores are required), `17`, `-1`, `1.0`. **Record the code-1 asymmetry explicitly**: this enum spells code 1 `CANCELED` (one L), while `%GRPC_STATUS(SNAKE_STRING)%` renders it `CANCELLED` (two Ls). Both are measured; they are different upstream enums and must not be unified.
2. **The runtime rule.** The effective status is the response `grpc-status` header if present, else `http_to_grpc_status(response_code)`. **It is NOT gated on the request being a gRPC request** — the opposite of `%GRPC_STATUS%`, and the difference is observable on a plain HTTP request. `exclude: true` inverts membership over that same derived status. An empty `statuses` list keeps nothing.
3. **The envoy-rust scope.** Two sources of three: the trailer source is not implemented (`CF-114-1`, blocked behind `CF-111-2`). Both codecs implement both landed legs. The `exclude` arm and the H2 arm are pinned in-process only, not differentially (`CF-114-3`, `CF-114-4`), and an unparseable or out-of-range header value falls back to the response-code map, which is unmeasured upstream (`CF-114-5`).

- [ ] **Step 3: Verify no other section was disturbed**

Run: `git diff --numstat docs/envoy-rust/BEHAVIOR_CONTRACT.md`
Expected: additions only, deletions `0`.

- [ ] **Step 4: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 114 task 10: BEHAVIOR_CONTRACT — the grpc_status_filter grammar and runtime rule"
```

---

## Carry-forwards this phase opens or amends

- **CF-114-1** — the TRAILER source of the effective gRPC status is not implemented; the phase ships a two-source rule where upstream has three. Blocked behind `CF-111-2`. **Unchanged** by this PLAN-write.
- **CF-114-2** — five `AccessLogFilter` arms remain unbuilt. **Unchanged.**
- **CF-114-3** — **NARROWED.** `SPEC.md` left open whether the H2 response-header leg is even reachable; PV-4 measured that it IS, so the phase implements BOTH legs on H2 and the carry-forward reduces to the missing cross-proxy H2 *fixture*. The behaviour is pinned in-process by `h2_grpc_status_code_tests`.
- **CF-114-4** — **NARROWED.** `SPEC.md` scoped it to whichever of the `exclude: true` and explicit-header witnesses the fixture could not express. PV-5 measured that the header leg IS expressible (probes 1 and 2 witness it), so this reduces to the **`exclude: true` witness alone**, covered in-process by `grpc_status_arm_exclude_inverts_over_the_same_code`. A second fixture `0095` was considered and rejected: the byte-exact driver takes one log file per side, so `exclude` needs its own fixture, and one more full fixture buys one bit that an in-process test already pins over all 17 codes.
- **CF-114-5 (NEW, opened by this PLAN-write).** The filter's behaviour when the response `grpc-status` header is PRESENT but unparseable or out of 0–16 is **not measured upstream**, and it is not witnessable by a fixture on either proxy — neither side can set an arbitrary `grpc-status` response header without route-level `response_headers_to_add`, which is boot-fatal in envoy-rust. envoy-rust falls back to the response-code map; upstream's own source is an optional parse, so the fallback is the defensible reading, but it is a CHOICE and not a measurement. Pinned in-process by `derivation_ignores_an_unparseable_or_out_of_range_header`.
- **CF-113-7 is CONSUMED** by Task 1, as the rider `SPEC.md` §5 non-goal 6 authorises. Every other previously banked carry-forward carries forward INTACT: CF-113-1/2/3, CF-113-5/6 and CF-113-8…CF-113-13, CF-112-1…CF-112-19, the `112.1`/`112.2`/`111`/`110.x`/`109.x`/`108.2` REVIEW sets, CF-111-1…CF-111-9, CF-110-1…9, CF-109-1/2/3, CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6, CF-74-1/2/3/4/6, CF-73-1 and the HTTP-filters-family (1)–(4). **CF-113-4 stays CONSUMED** (by the phase-114 pick). **CF-111-4 stays consumed only in PART. CF-112-5 stays CLOSED. CF-112-8 Consequence 2 stays BANKED as structurally unwitnessable.** The phase-112 ALPN cleanup set is NOT taken and is NOT re-costed.

---

## Self-review

**Spec coverage.** `SPEC.md` §4's nine deliverables map to tasks as: 1→T3, 2→T3, 3→T4, 4→T2+T5, 5→T7, 6→T6, 7→T8, 8→T9, 9→T10. Every one is covered. Deliverable 4 is split across two tasks because the visibility widening is a design change that deserves its own reviewable boundary. Task 1 is a rider and maps to no deliverable, by design.

**Placeholder scan.** No `TBD`, no "add appropriate error handling", no "similar to Task N". Every code step carries the literal code. Task 9's fixture YAMLs and runner are quoted in full; Task 2's and Task 10's edits are quoted or specified field-by-field. Task 9 Step 5's README is specified as a section list rather than quoted verbatim — it is prose documentation, and every claim it must make is enumerated.

**Type consistency.** `GrpcStatusFilter` / `GrpcStatusToken` / `GRPC_STATUS_FILTER_NAMES` / `resolve_grpc_status_token` / `UnknownGrpcStatus` / `grpc_status_code` / `effective_grpc_status` / `LogFilter::GrpcStatus { codes, exclude }` / `http_to_grpc_status` are spelled identically in every task that names them, and each is defined before it is consumed. The `should_log` signature is fixed once in Task 6 and every later call matches it.

**Every code block in this plan was EXECUTED, not written from reasoning.** The blocks were inserted into a scratch worktree, which compiled, passed `cargo clippy --workspace --all-targets --all-features -- -D warnings`, passed `cargo fmt --all -- --check`, passed 1224 unit tests across the four affected crates and 2037 across the workspace, and turned fixture `0094` GREEN against both real proxies — with Task 9 Step 7's mutation verified RED and its unmutated control verified GREEN from the same tree after an md5-checked restore.

⚠ **What this plan does NOT claim.** Per `ADR-0194` DECISION 2, a whole-slice prototype validates the SLICE and never a TASK BOUNDARY. The gates listed under each task are instructions to the executor, not a measured assertion that each intermediate commit passes `-D warnings` in isolation. Task 6's clippy note is written accordingly.
