# Phase 116 — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the `not_health_check_filter` access-log FILTER arm — the eighth of upstream Envoy's twelve `envoy.config.accesslog.v3.AccessLogFilter` oneof arms — which drops a record iff the downstream `envoy.filters.http.health_check` filter ANSWERED the request, witnessed by new differential fixture `0099-accesslog-not-health-check-filter`.

**Architecture:** One shared predicate, `envoy_filter::health_check::answered_by_health_check(details)`, reads the request's FINAL `%RESPONSE_CODE_DETAILS%` after the decode-side filter pass. It already decides (phase 115) whether an intercepted request is excluded from `downstream_rq_Nxx`; this phase makes both HCMs' counter exclusion AND their access-log gate call it, so the two consumers can never disagree. The bit reaches the predicate as a sixth `should_log` argument, `is_health_check: bool`. A new unit variant `LogFilter::NotHealthCheck` evaluates `!is_health_check`. The config side adds an eighth `Option` arm carrying an EMPTY message whose `Deserialize` is hand-rolled to accept maps only.

**Tech Stack:** Rust 2024, `envoy-config` (serde + `serde_yaml` 0.9.34), `envoy-accesslog` (a leaf crate with zero intra-workspace dependencies), `envoy-filter`, `envoy-http1`, `envoy-http2`, the existing `Driver::Http1AccessLogByteExact` differential driver.

**Spec:** `docs/envoy-rust/phases/116-accesslog-not-health-check-filter/SPEC.md`. Read it together with this plan, but see **SPEC corrections** below: this PLAN-write measured three SPEC claims to be wrong, and `ADR-0206` is the forward correction. Where they disagree, this plan wins.

---

## Global Constraints

- Upstream target is `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`. Every behavioural cell in this plan was MEASURED against it; none was recalled. Container ownership was proved on every run with `docker inspect <cid> --format '{{.Image}}'`.
- `#![forbid(unsafe_code)]` in every crate root (D-3.8). No `unsafe` anywhere in this phase.
- **No new dependency, no new workspace crate, no new harness driver, no new fuzz target.** `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml` and `tests/differential/src/lib.rs` MUST stay untouched. They were VERIFIED untouched on the prototype (PV-8). If a task appears to need one of them, STOP: the scope has drifted.
- Every task ends green on `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`. Each task boundary was committed and gated SEPARATELY on the prototype (see the §6.1 table), not inferred from the whole slice (`ADR-0194` DECISION 2).
- Adding a `pub` field to `AccessLogFilter` is a CROSS-CRATE change: every exhaustive `AccessLogFilter` literal in `envoy-config` AND `envoy-http1` breaks with `E0063`. Cargo stops at the first failing crate, so a `-p <crate>` build can look green while the workspace is broken. Sweep in a WORKSPACE fixpoint loop (Task 3). Literals using `..AccessLogFilter::default()` / `..Default::default()` absorb the field and must NOT be edited.
- **The `is_health_check` bit is computed from the filter's DECISION, never from the request.** Its only source is `answered_by_health_check(record.response_code_details.as_deref())`, evaluated after the decode-side pass. Never key it on the path, a header, the user-agent or the health-check matcher config — `SPEC.md` §2.2 rules 2 and 3 and fixture probes 5 and 6 exist to catch exactly that.
- Nothing is fixed (§6.3; `ADR-0165`). No carry-forward is consumed except `CF-115-3`, which this phase IS.
- Run every `cargo` command from the repo root. Never pipe a verification run through `tail` — it truncates the `failures:` block.

---

## Review Focus

The five input classes most likely to bite a user that the SPEC's tables do not cover directly. Each one is pinned by a test in the task that owns the code; all five were MEASURED upstream at this PLAN-write.

1. **`not_health_check_filter: []` must be REJECTED at load**, as upstream rejects it. A derived serde `Deserialize` on an empty braced struct ACCEPTS a zero-length sequence — the obvious one-line struct is wrong. Pinned by `not_health_check_filter_rejects_every_measured_reject` (Task 3), whose RED against the derived form was measured.
2. **A YAML null (`not_health_check_filter: ~`, or the bare key) is an UNSET arm**, rejected as "no filter variant is set" — not an empty message that loads. Pinned by `not_health_check_filter_null_is_an_unset_arm` (Task 3).
3. **An H1 `HEAD` intercept is dropped too.** Since `ADR-0202` it takes a different framing path (`transfer-encoding: chunked`, no body), but it is still a health check. Pinned in `h1_not_health_check_filter_drops_exactly_the_intercepts` (Task 4).
4. **The bit is per REQUEST, never sticky on a connection.** Two requests pipelined on one H1 keep-alive connection — intercept then route, and route then intercept — keep exactly the routed one. Pinned in the same H1 test, and on H2 by sending every request over ONE connection in `h2_not_health_check_filter_drops_exactly_the_intercepts` (Task 4).
5. **A sink WITHOUT the filter on the same HCM still logs the intercept** with `health_check_ok`. Already pinned by the landed phase-115 test `h1_health_check_filter_end_to_end`, which this phase must keep green — the arm is per-sink, never a global suppression.

---

## SPEC corrections this PLAN-write measured — read before Task 1

`SPEC.md` is landed and is NOT edited; `ADR-0206` carries the forward correction.

1. **`SPEC.md` §1 and §4 item 4's "only TWO of them are production sites" is FALSE.** The 146 total re-derives exactly (`filter.rs` 80, `hcm.rs` (H1) 56, `file_sink.rs` 6, `hcm.rs` (H2) 4). The split is **5 production + 133 test + 8 non-call text matches**, not 2 + 144. The three missed production sites are the `And`/`Or` recursion in `LogFilter::should_log` and the `FileSink` → `LogFilter` delegation — the same three phase 114's SPEC missed (`ADR-0197` correction 1). Task 1 covers all five by hand and the 133 test sites by script.
2. **`SPEC.md` §4 item 1's "carries `#[serde(default, deny_unknown_fields)]` like every sibling leaf" would ship a divergence.** That derive ACCEPTS `not_health_check_filter: []`; upstream REJECTS it (`invalid JSON`, MEASURED with `--mode validate`). Every sibling leaf has at least one field, which is why none of them hit this. Task 3 hand-rolls a map-only visitor, on the `SafeRegex` precedent in the same file.
3. **`SPEC.md` §7's size estimate is low, as its own calibration paragraph predicted.** Projected central ≈730 net code lines in a 565–890 band; **MEASURED 978**, 1.34× the central and above the band's top, but inside the landed filter-arm comparators (873–1064). The §6.1 gate does NOT fire.

---

## PLAN-VERIFY discharge (`SPEC.md` §8)

| item | result |
|---|---|
| **PV-1** | Every anchor re-derived by TEXT at `fd9d65e`, each asserted to occur exactly once: `bootstrap.rs:730/731/732/5908`, `lib.rs:475`, `filter.rs:68/121`, `file_sink.rs:103`, `health_check.rs:31`, H1 `hcm.rs:1554/1605/1880`, H2 `hcm.rs:971/1133/1236`, `tests/differential/src/lib.rs:170/1178/1192/1207` — ALL CORRECT. **Every line number in this plan is a pre-edit anchor — locate by TEXT; Task 1 moves them all.** |
| **PV-2** | **A sixth `should_log` parameter, `is_health_check: bool`, derived by ONE shared predicate.** `answered_by_health_check(details: Option<&str>) -> bool` lives beside `HEALTH_CHECK_OK` in `crates/envoy-filter/src/health_check.rs`; both HCMs' counter exclusions are rewired to it and both production `should_log` sites pass it. Rejected: an `AccessLogRecord` field — the record ALREADY carries `response_code_details`, so a second field would be a derived copy that could drift, and it costs 21 `E0063` sites for nothing. The doc comment carries the SPEC's warning that a future filter reusing `health_check_ok` is a health check to BOTH consumers. |
| **PV-3** | **Proved by MUTATION on both codecs**, three mutations each, every one RED from the same tree with the unmutated control GREEN: forcing the bit `true`, forcing it `false`, and deriving it from the request path (`starts_with("/healthz")`). On fixture `0099` the path mutation emits exactly 2 lines (probes 3 and 5 lost) where 4 are expected. Task 4 and Task 5 carry the procedure. |
| **PV-4** | **Dry-run DONE on both proxies before any code**, with the exact fixture YAML. Upstream: probes 1–2 suppressed, 3–6 kept, probe 5 is `403 RBAC: access denied`. envoy-rust at `fd9d65e`: the filtered config is BOOT-REJECTED (`unknown field \`not_health_check_filter\``) — the RED-before is real — and the same config with the `filter:` lines removed accepts the RBAC `DENY` + `present_match` shape, answers probe 5 with 403, and renders every line byte-identically to upstream. The `%RESPONSE_CODE_DETAILS%` divergence (`CF-116-3`) was re-confirmed, so the format omits it. |
| **PV-5** | **`SPEC.md` §2.2 row 8 (no health_check filter on the listener) is pinned IN-PROCESS on both codecs, not by a second fixture.** The H1 and H2 pins (Task 4) each build a second HCM without the filter and assert `GET /healthz` is KEPT. A second fixture would add ~250 lines for one cell the in-process pins already assert on BOTH codecs; the fixture cannot also hold it, because one fixture holds one listener's chain. No carry-forward is needed — the cell is witnessed, only not cross-proxy, and it is a negative cell (nothing to compare beyond "kept"). |
| **PV-6** | **MEASURED, not assumed.** `not_health_check_filter: ~` and the bare key deserialize to `None` and are rejected as `AmbiguousAccessLogFilter { "no filter variant is set" }` — the same class as upstream's `filter_specifier … is required`. Pinned in Task 3. |
| **PV-7** | **Measured 978 net LoC** on the prototype, and re-measured after the final plan edit (this table is computed by the generator from the prototype's `git diff --numstat`, so it cannot go stale relative to the fences). |
| **PV-8** | `git diff --stat fd9d65e <prototype end> -- Cargo.toml Cargo.lock .github tests/differential/src/lib.rs` is EMPTY; the positive control `-- tests/differential/tests` lists the new runner. |

---

## File Structure

| file | responsibility | change |
|---|---|---|
| `crates/envoy-filter/src/health_check.rs` | `answered_by_health_check`, the ONE health-check predicate + its test | modify |
| `crates/envoy-accesslog/src/filter.rs` | the sixth `should_log` parameter, `LogFilter::NotHealthCheck`, its arm, tests | modify |
| `crates/envoy-accesslog/src/file_sink.rs` | the sixth `should_log` parameter (delegation) | modify |
| `crates/envoy-http1/src/hcm.rs` | counter exclusion + production `should_log` site rewired to the predicate; `compile_access_log_filter`'s eighth arm; the `E0063` sweep; the H1 in-process pin | modify |
| `crates/envoy-http2/src/hcm.rs` | counter exclusion + production `should_log` site rewired; the H2 in-process pin | modify |
| `crates/envoy-config/src/bootstrap.rs` | `NotHealthCheckFilter` + its map-only `Deserialize`, the eighth `AccessLogFilter` arm, validator growth, `E0063` sweep, tests | modify |
| `crates/envoy-config/src/lib.rs` | re-export `NotHealthCheckFilter` | modify |
| `tests/fixtures/0099-accesslog-not-health-check-filter/{envoy,envoy-rust,expectations}.yaml`, `README.md` | the differential fixture | create |
| `tests/differential/tests/accesslog_not_health_check_filter.rs` | the fixture runner | create |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | the config surface + the runtime rule | modify |

**Task order is dependency-forced, and chosen so that NO intermediate commit parses a config it then ignores or panics on.** The config arm and the compile arm land TOGETHER in Task 3, after the runtime arm (Task 2) exists — the phase-114 order (config first, compile last) left a window in which `not_health_check_filter: {}` parsed and then hit `compile_access_log_filter`'s `unreachable!`. The production sites pass the REAL bit from Task 1 onward, so no commit carries a placeholder `false` that a later task must remember to replace.

---

## §6.1 SPLIT GATE — MEASURED, DOES NOT FIRE

The gate is ~25 tasks OR ~1500 net LoC. **This plan is 6 tasks and MEASURED 978 net LoC excluding `docs/`** (1154 insertions, 176 deletions), a margin of 522.

The measurement is not a projection. Every code block below was cut by script from a scratch `git worktree` (created with `git worktree add --detach`, never `cp -r`) with its **own `CARGO_TARGET_DIR`**; the main tree stayed `git status --porcelain`-clean throughout. That tree passes `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`, turns fixture `0099` GREEN against both real proxies with three fixture mutations RED and the unmutated control GREEN, and ran the whole workspace suite (see the state-2 record in `ADR-0206`).

| file | insertions | deletions | net |
|---|---:|---:|---:|
| `crates/envoy-accesslog/src/file_sink.rs` | 11 | 8 | 3 |
| `crates/envoy-accesslog/src/filter.rs` | 162 | 78 | 84 |
| `crates/envoy-config/src/bootstrap.rs` | 145 | 9 | 136 |
| `crates/envoy-config/src/lib.rs` | 3 | 3 | 0 |
| `crates/envoy-filter/src/health_check.rs` | 33 | 0 | 33 |
| `crates/envoy-http1/src/hcm.rs` | 300 | 75 | 225 |
| `crates/envoy-http2/src/hcm.rs` | 154 | 3 | 151 |
| `tests/differential/tests/accesslog_not_health_check_filter.rs` | 25 | 0 | 25 |
| `tests/fixtures/0099-accesslog-not-health-check-filter/README.md` | 119 | 0 | 119 |
| `tests/fixtures/0099-accesslog-not-health-check-filter/envoy-rust.yaml` | 68 | 0 | 68 |
| `tests/fixtures/0099-accesslog-not-health-check-filter/envoy.yaml` | 70 | 0 | 70 |
| `tests/fixtures/0099-accesslog-not-health-check-filter/expectations.yaml` | 64 | 0 | 64 |
| **total (`crates/` + `tests/`)** | **1154** | **176** | **978** |

`docs/envoy-rust/BEHAVIOR_CONTRACT.md` adds 90 lines (excluded from the gate).

**Every task boundary was committed and gated separately** on the prototype — build, clippy `-D warnings`, fmt `--check` and the touched crates' unit tests at each commit:

| task | net LoC (`crates/` + `tests/`) | boundary gate |
|---|---:|---|
| 1 | 89 | clean — WITH the transient `#[allow(clippy::only_used_in_recursion)]`; without it clippy fails (MEASURED) |
| 2 | 50 | clean; the allow is deleted |
| 3 | 206 | clean |
| 4 | 287 | clean after one `clippy::type_complexity` fix (the `H2Probe` alias below is that fix) |
| 5 | 346 | fixture `0099` GREEN against both proxies |
| 6 | 0 | docs only |

---

## Task 1: The shared health-check predicate + the sixth `should_log` parameter (behaviour-neutral)

**Files:**
- Modify: `crates/envoy-filter/src/health_check.rs` (the predicate + its test)
- Modify: `crates/envoy-accesslog/src/filter.rs` (the definition + 2 production recursion sites + 77 test sites + one test)
- Modify: `crates/envoy-accesslog/src/file_sink.rs` (the definition + 1 production delegation + 4 test sites)
- Modify: `crates/envoy-http1/src/hcm.rs` (the counter exclusion + 1 production site + 52 test sites)
- Modify: `crates/envoy-http2/src/hcm.rs` (the counter exclusion + 1 production site)

**Interfaces:**
- Consumes: `envoy_filter::health_check::HEALTH_CHECK_OK` (phase 115); `AccessLogRecord.response_code_details: Option<String>` (phase 42).
- Produces: `pub fn envoy_filter::health_check::answered_by_health_check(details: Option<&str>) -> bool`; `LogFilter::should_log(&self, status: u16, response_flags: &str, headers: &[(String, String)], dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>, grpc_status_code: u8, is_health_check: bool) -> bool` and the matching `FileSink::should_log`.

This is its own task on the phase-74 `T3` / phase-114 Task 6 precedent: a mechanical widening that changes no behaviour, because no arm reads the new argument until Task 2. **The production sites pass the REAL bit already** — it is computed from the record, which every site has in scope, so there is no placeholder to forget.

⚠ **Five production sites, not two** (SPEC correction 1): the two HCM dispatch sites, the `And` and `Or` recursion inside `LogFilter::should_log`, and the `FileSink::should_log` delegation. Locate each by TEXT.

- [ ] **Step 1: Write the failing test for the predicate**

At the top of `mod tests` in `crates/envoy-filter/src/health_check.rs` (right after `use super::*;`):

```rust
    #[test]
    fn answered_by_health_check_reads_only_the_intercept_details() {
        assert!(answered_by_health_check(Some(HEALTH_CHECK_OK)));
        // Every other details value, and none at all, is NOT a health check:
        // a fall-through route, an earlier filter's local reply, a route miss.
        for other in [
            "direct_response",
            "rbac_access_denied_matched_policy[deny-flagged]",
            "route_not_found",
            "via_upstream",
            "health_check_ok ",
            "HEALTH_CHECK_OK",
        ] {
            assert!(!answered_by_health_check(Some(other)), "{other}");
        }
        assert!(!answered_by_health_check(None));
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p envoy-filter --lib answered_by_health_check`
Expected: a compile error, `cannot find function \`answered_by_health_check\``. (A compile error is not a behavioural RED; the behavioural RED evidence for this predicate is Task 4's mutations.)

- [ ] **Step 3: Add the predicate**

Directly below `pub const HEALTH_CHECK_OK` in `crates/envoy-filter/src/health_check.rs`:

```rust
/// Phase 116: `true` iff a request's final `%RESPONSE_CODE_DETAILS%` says the
/// health_check filter ANSWERED it. The ONE definition behind both HCMs'
/// `downstream_rq_Nxx` exclusion (phase 115) and the `not_health_check_filter`
/// access-log arm (phase 116), so the two can never disagree.
///
/// It reads the filter's DECISION, never the request: a `/healthz` that falls
/// through, that an earlier filter answers, or that reaches a listener with no
/// health_check filter carries other details and is NOT a health check
/// (MEASURED, phase-116 `SPEC.md` §2.2 rules 2 and 3). ⚠ Any future filter
/// that reuses the `health_check_ok` details string is treated as a health
/// check by BOTH consumers.
pub fn answered_by_health_check(details: Option<&str>) -> bool {
    details == Some(HEALTH_CHECK_OK)
}
```

- [ ] **Step 4: Rewire both counter exclusions to it**

In `crates/envoy-http1/src/hcm.rs`, the phase-115 block that reads `if response_code_details_for_log.as_deref() != Some(envoy_filter::health_check::HEALTH_CHECK_OK) {` becomes:

```rust
        if !envoy_filter::health_check::answered_by_health_check(
            response_code_details_for_log.as_deref(),
        ) {
```

In `crates/envoy-http2/src/hcm.rs`, the same over `response_code_details_for_log_h2`:

```rust
    if !envoy_filter::health_check::answered_by_health_check(
        response_code_details_for_log_h2.as_deref(),
    ) {
```

These are pure refactors: the predicate is the same comparison.

- [ ] **Step 5: Widen the two definitions**

In `crates/envoy-accesslog/src/filter.rs`, `LogFilter::should_log` gains the parameter, a TRANSIENT allow, and the two recursion arms thread it (Task 2 deletes the allow and rewrites the doc line):

```rust
    /// Phase 70/71/72/73/74/114/116: returns `true` iff a record with the given
    /// final response `status`, `response_flags` token, request `headers`,
    /// per-request `dynamic_metadata`, UNGATED effective `grpc_status_code` and
    /// `is_health_check` bit (the health_check filter ANSWERED the request)
    /// should be emitted. The `StatusCode` arm reads only `status`; the
    /// `ResponseFlag` arm only `response_flags`; the `Header` arm only
    /// `headers`; the phase-74 `Metadata` arm only `dynamic_metadata`; the
    /// phase-114 `GrpcStatus` arm only `grpc_status_code`. The status
    /// comparison is widened to `u32` (lossless; status is always in `u16`
    /// range).
    // TRANSIENT (phase 116 Task 1): no arm reads `is_health_check` until Task 2
    // lands `LogFilter::NotHealthCheck`. Task 2 DELETES this allow.
    #[allow(clippy::only_used_in_recursion)]
    pub fn should_log(
        &self,
        status: u16,
        response_flags: &str,
        headers: &[(String, String)],
        dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>,
        grpc_status_code: u8,
        is_health_check: bool,
    ) -> bool {
```

```rust
            LogFilter::And(filters) => filters.iter().all(|f| {
                f.should_log(
                    status,
                    response_flags,
                    headers,
                    dynamic_metadata,
                    grpc_status_code,
                    is_health_check,
                )
            }),
            LogFilter::Or(filters) => filters.iter().any(|f| {
                f.should_log(
                    status,
                    response_flags,
                    headers,
                    dynamic_metadata,
                    grpc_status_code,
                    is_health_check,
                )
            }),
```

⚠ **The allow is REQUIRED at this boundary.** Without it `clippy -D warnings` fails with `parameter is only used in recursion` (MEASURED on the prototype at this exact state). Do not leave it: Task 2 deletes it.

In `crates/envoy-accesslog/src/file_sink.rs`, `FileSink::should_log` becomes:

```rust
    /// Phase 70/71/72/73/74/114/116: returns `true` iff a record with final
    /// response `status`, `response_flags` token, request `headers`, per-request
    /// `dynamic_metadata`, UNGATED effective `grpc_status_code` and
    /// `is_health_check` bit should be emitted to this sink. A sink with no
    /// filter always logs.
    pub fn should_log(
        &self,
        status: u16,
        response_flags: &str,
        headers: &[(String, String)],
        dynamic_metadata: &std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, String>,
        >,
        grpc_status_code: u8,
        is_health_check: bool,
    ) -> bool {
        match &self.filter {
            Some(f) => f.should_log(
                status,
                response_flags,
                headers,
                dynamic_metadata,
                grpc_status_code,
                is_health_check,
            ),
            None => true,
        }
    }
```

- [ ] **Step 6: Pass the real bit at the two HCM dispatch sites**

In `crates/envoy-http1/src/hcm.rs`, the per-sink `should_log` call gains a sixth argument after `record.grpc_status_code,`:

```rust
                    // Phase 116: the health_check filter's DECISION, read from
                    // the final details AFTER the decode-side pass decided the
                    // response — never from the request.
                    envoy_filter::health_check::answered_by_health_check(
                        record.response_code_details.as_deref(),
                    ),
                ) {
```

In `crates/envoy-http2/src/hcm.rs`, after `record.grpc_status_code,`:

```rust
                // Phase 116: the health_check filter's DECISION (see the H1
                // site) — the same predicate as the counter exclusion above.
                envoy_filter::health_check::answered_by_health_check(
                    record.response_code_details.as_deref(),
                ),
            ) {
```

The record is built BEFORE the sink loop on both codecs, so `record.response_code_details` is the FINAL details — set by the decode-side filter pass on the intercept path, and by the route/proxy path otherwise.

- [ ] **Step 7: Sweep the 133 test sites**

Every remaining `.should_log(` call needs one more argument. Use the neutral literal `false`. The final arguments have several spellings, some with nested parentheses (`&md("com.example", "k", "1")`), and some calls are already multiline, so a regex will not do; walk the files with this paren-matching script (save it anywhere outside the repo):

```python
# Append `, false` (the sixth `should_log` argument, `is_health_check`) to every
# `.should_log(` call that does not already carry it. Paren-matched, so the
# nested-paren final arguments (`&md("com.example", "k", "1")`) are handled.
import sys
for path in sys.argv[1:]:
    t = open(path).read()
    i = 0
    n = 0
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
        if 'is_health_check' in inner or 'answered_by_health_check' in inner:
            i = pos
            continue
        if '\n' in inner:
            body = inner.rstrip()
            indent = inner[inner.rfind('\n', 0, len(body)) + 1:]
            indent = indent[:len(indent) - len(indent.lstrip())]
            tail = inner[len(body):]
            new = body + ('' if body.endswith(',') else ',') + '\n' + indent + 'false,' + tail
        else:
            new = inner + ', false'
        t = t[:k] + new + t[close:]
        i = k + len(new)
        n += 1
    open(path, 'w').write(t)
    print(path, n)
```

Run it, then format:

```bash
python3 /path/to/sweep116.py crates/envoy-accesslog/src/filter.rs crates/envoy-accesslog/src/file_sink.rs crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
cargo fmt --all
```

Expected script output: `filter.rs 77`, `file_sink.rs 4`, `hcm.rs 52` (H1), `hcm.rs 0` (H2). The five production sites already name `is_health_check` / `answered_by_health_check` and are skipped. **Run `fmt` in THIS task**, not as a follow-up.

- [ ] **Step 8: Add the behaviour-neutrality pin**

In `crates/envoy-accesslog/src/filter.rs`'s `mod tests`, after `existing_arms_ignore_the_grpc_status_argument`:

```rust
    #[test]
    fn existing_arms_ignore_the_health_check_argument() {
        // The phase-74 T3 behaviour-neutrality pin, repeated for the sixth
        // parameter: every pre-phase-116 arm, the composition arms included,
        // must be blind to it.
        let gs = LogFilter::GrpcStatus {
            codes: vec![2],
            exclude: false,
        };
        let and = LogFilter::And(vec![ge(200), ge(300)]);
        let or = LogFilter::Or(vec![ge(500), rf(&["NR"])]);
        for hc in [false, true] {
            assert!(ge(500).should_log(503, "-", &[], &Default::default(), 2, hc));
            assert!(!ge(500).should_log(499, "-", &[], &Default::default(), 2, hc));
            assert!(rf(&["NR"]).should_log(404, "NR", &[], &Default::default(), 2, hc));
            assert!(gs.should_log(200, "-", &[], &Default::default(), 2, hc));
            assert!(!gs.should_log(200, "-", &[], &Default::default(), 12, hc));
            assert!(and.should_log(300, "-", &[], &Default::default(), 2, hc));
            assert!(!and.should_log(200, "-", &[], &Default::default(), 2, hc));
            assert!(or.should_log(404, "NR", &[], &Default::default(), 2, hc));
            assert!(!or.should_log(404, "-", &[], &Default::default(), 2, hc));
        }
    }
```

- [ ] **Step 9: Verify — count, build, lint, test**

```bash
grep -c 'should_log(' crates/envoy-accesslog/src/filter.rs crates/envoy-accesslog/src/file_sink.rs crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test -p envoy-filter -p envoy-accesslog -p envoy-http1 -p envoy-http2 --lib
```

Expected: `89`, `6`, `56`, `4` (MEASURED at this boundary). The sweep adds arguments, not calls, so the four files read exactly their pre-task `80`/`6`/`56`/`4` plus the nine `should_log(` lines Step 8's pin adds to `filter.rs`. The other four commands are clean.

- [ ] **Step 10: Commit**

```bash
git add crates/envoy-filter/src/health_check.rs crates/envoy-accesslog/src/filter.rs crates/envoy-accesslog/src/file_sink.rs crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 116 task 1: one health-check predicate; widen should_log with it (behavior-neutral)"
```

---

## Task 2: `LogFilter::NotHealthCheck` and its `should_log` arm

**Files:**
- Modify: `crates/envoy-accesslog/src/filter.rs`

**Interfaces:**
- Consumes: the sixth `should_log` parameter `is_health_check: bool` (Task 1).
- Produces: `envoy_accesslog::LogFilter::NotHealthCheck` — a UNIT variant, no fields.

- [ ] **Step 1: Write the failing tests**

At the end of `mod tests` in `crates/envoy-accesslog/src/filter.rs`:

```rust
    // ── Phase 116: the `NotHealthCheck` arm ───────────────────────────────────
    #[test]
    fn not_health_check_arm_drops_exactly_the_health_checks() {
        let f = LogFilter::NotHealthCheck;
        assert!(!f.should_log(200, "-", &[], &Default::default(), 2, true));
        assert!(f.should_log(200, "-", &[], &Default::default(), 2, false));
    }

    #[test]
    fn not_health_check_arm_reads_only_the_bit() {
        // MEASURED (`SPEC.md` §2.2 rule 2): the arm keys on the filter's
        // decision, never on the request — so no status, flag, header,
        // metadata or gRPC status can make it drop a non-health-check.
        let f = LogFilter::NotHealthCheck;
        let ua = [("user-agent".to_string(), "Envoy/HC".to_string())];
        let internal = [("x-envoy-internal".to_string(), "true".to_string())];
        for status in [200u16, 403, 404, 503] {
            for headers in [&[][..], &ua[..], &internal[..]] {
                assert!(f.should_log(status, "-", headers, &Default::default(), 2, false));
                assert!(!f.should_log(status, "NR", headers, &Default::default(), 14, true));
            }
        }
    }

    #[test]
    fn not_health_check_arm_composes_as_a_leaf() {
        // MEASURED (`SPEC.md` §2.2 rule 4): sinks CCC and DDD. The composition
        // arms must THREAD the bit to the nested leaf — a recursion that drops
        // it goes red here.
        let keep = || LogFilter::Header {
            matcher: std::sync::Arc::new(HasHeaderValue("x-keep", "1")),
        };
        let or = LogFilter::Or(vec![LogFilter::NotHealthCheck, keep()]);
        let x_keep = vec![("x-keep".to_string(), "1".to_string())];
        assert!(!or.should_log(200, "-", &[], &Default::default(), 2, true));
        assert!(or.should_log(200, "-", &x_keep, &Default::default(), 2, true));
        assert!(or.should_log(200, "-", &[], &Default::default(), 2, false));

        let and = LogFilter::And(vec![LogFilter::NotHealthCheck, ge(300)]);
        assert!(and.should_log(403, "-", &[], &Default::default(), 2, false));
        assert!(!and.should_log(200, "-", &[], &Default::default(), 2, false));
        assert!(!and.should_log(403, "-", &[], &Default::default(), 2, true));
    }
```

`HasHeaderValue` and `ge` are the existing test helpers in the same module.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p envoy-accesslog --lib not_health_check`
Expected: compile error `no variant … named \`NotHealthCheck\``.

- [ ] **Step 3: Add the variant**

At the end of `pub enum LogFilter`, after `GrpcStatus { … },`:

```rust
    /// Phase 116: emit a record iff the health_check filter did NOT answer the
    /// request (`not_health_check_filter`). The config message is EMPTY, so
    /// the variant carries nothing; the bit arrives as the `is_health_check`
    /// argument, which the HCMs derive from the filter's DECISION.
    NotHealthCheck,
```

- [ ] **Step 4: Add the arm, and DELETE Task 1's transient allow**

Delete the three lines Task 1 added above `pub fn should_log(` (the two `// TRANSIENT …` comment lines and `#[allow(clippy::only_used_in_recursion)]`). Extend the doc comment's per-arm list so it ends:

```rust
    /// phase-114 `GrpcStatus` arm only `grpc_status_code`; the phase-116
    /// `NotHealthCheck` arm only `is_health_check`. The status
```

and add the arm as the LAST arm of the `match self`, after `LogFilter::GrpcStatus { … }`:

```rust
            // Phase 116: drop exactly the requests the health_check filter
            // answered (MEASURED, `SPEC.md` §2.2 rule 1).
            LogFilter::NotHealthCheck => !is_health_check,
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-accesslog --lib`
Expected: PASS, every test (137 at the prototype).

- [ ] **Step 6: Mutation — prove the composition test can fail**

`not_health_check_arm_composes_as_a_leaf` is the only test that sees the recursion THREAD the bit. In the `LogFilter::And(filters)` arm, change the recursive call's last argument from `is_health_check,` to `false,` and run `cargo test -p envoy-accesslog --lib`. Expected: exactly `not_health_check_arm_composes_as_a_leaf` FAILS (MEASURED: `136 passed; 1 failed`). Restore the line, confirm `git diff --stat` shows only this task's intended changes, and re-run: all pass.

- [ ] **Step 7: Verify and commit**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git add crates/envoy-accesslog/src/filter.rs
git commit -m "phase 116 task 2: LogFilter::NotHealthCheck drops exactly the health-check intercepts"
```

Expected: clippy clean WITHOUT the allow — the parameter is now read by a real arm.

---

## Task 3: The `NotHealthCheckFilter` config surface, validation and the eighth compile arm

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (the struct, its `Deserialize`, the eighth arm, the validator, the `E0063` literals, the tests)
- Modify: `crates/envoy-config/src/lib.rs` (one re-export)
- Modify: `crates/envoy-http1/src/hcm.rs` (`compile_access_log_filter`, the `E0063` literals, one test)

**Interfaces:**
- Consumes: `envoy_accesslog::LogFilter::NotHealthCheck` (Task 2).
- Produces: `pub struct envoy_config::NotHealthCheckFilter {}` (re-exported from the crate root); `AccessLogFilter::not_health_check_filter: Option<NotHealthCheckFilter>`.

**The config arm and the compile arm land in ONE task, deliberately.** If the config arm landed first, a config carrying `not_health_check_filter: {}` would parse and validate, and then panic in `compile_access_log_filter`'s `unreachable!` at HCM build.

- [ ] **Step 1: Write the failing config tests**

In `crates/envoy-config/src/bootstrap.rs`'s `mod tests`, immediately BEFORE `fn grpc_status_filter_empty_statuses_loads` (keep that function's `#[test]` attribute attached to it):

```rust
    // --- phase 116: the `not_health_check_filter` arm ---

    #[test]
    fn not_health_check_filter_loads_alone_and_nested() {
        // MEASURED: `{}` validates upstream, alone and nested inside both
        // composition arms on one bootstrap.
        for arm in [
            "not_health_check_filter: {}",
            "or_filter: { filters: [ { not_health_check_filter: {} }, { header_filter: { header: { name: x-keep, present_match: true } } } ] }",
            "and_filter: { filters: [ { not_health_check_filter: {} }, { status_code_filter: { comparison: { op: GE, value: { default_value: 300, runtime_key: k } } } } ] }",
        ] {
            crate::parse_bootstrap(&access_log_filter_yaml(arm))
                .unwrap_or_else(|e| panic!("MEASURED ACCEPT upstream: {arm} -> {e:?}"));
        }
    }

    #[test]
    fn not_health_check_filter_rejects_every_measured_reject() {
        // MEASURED REJECT upstream (`--mode validate`): the message is EMPTY
        // and CLOSED. `[]` is the trap — a DERIVED `Deserialize` accepts it.
        for arm in [
            "not_health_check_filter: { foo: 1 }",
            "not_health_check_filter: []",
            "not_health_check_filter: [1]",
            "not_health_check_filter: true",
            "not_health_check_filter: 0",
            "not_health_check_filter: \"\"",
        ] {
            let err = crate::parse_bootstrap(&access_log_filter_yaml(arm))
                .expect_err("MEASURED REJECT upstream");
            assert!(
                matches!(err, crate::ConfigError::Yaml(_)),
                "{arm}: must be a serde rejection, got {err:?}"
            );
        }
    }

    #[test]
    fn not_health_check_filter_null_is_an_unset_arm() {
        // MEASURED: upstream reads a YAML null as an UNSET oneof (`filter_specifier
        // … is required`), not as an empty message. Here it is `None`, so the
        // validator's zero-arm branch rejects it — the same class.
        for arm in ["not_health_check_filter: ~", "not_health_check_filter:"] {
            let err = crate::parse_bootstrap(&access_log_filter_yaml(arm))
                .expect_err("a null arm is unset");
            assert!(
                matches!(
                    &err,
                    crate::ConfigError::AmbiguousAccessLogFilter { detail }
                        if detail == "no filter variant is set"
                ),
                "{arm}: got {err:?}"
            );
        }
    }

    #[test]
    fn not_health_check_filter_with_a_second_arm_is_ambiguous() {
        let arm = "{ not_health_check_filter: {}, status_code_filter: { comparison: { op: GE, value: { default_value: 300, runtime_key: k } } } }";
        let err = crate::parse_bootstrap(&access_log_filter_yaml(arm)).expect_err("two arms");
        assert!(
            matches!(
                &err,
                crate::ConfigError::AmbiguousAccessLogFilter { detail }
                    if detail == "more than one filter variant is set"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn not_health_check_filter_round_trips_as_an_empty_map() {
        // The admin config_dump path serializes the config: the arm must come
        // back as `{}`, which the map-only visitor accepts.
        let json = serde_json::to_string(&NotHealthCheckFilter {}).expect("serializes");
        assert_eq!(json, "{}");
        let back: NotHealthCheckFilter = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, NotHealthCheckFilter {});
    }

```

`access_log_filter_yaml` is the existing helper that splices one `filter:` body into a full bootstrap.

- [ ] **Step 2: Grow the cardinality test from seven arms to eight**

Rename `seven_arm_cardinality_counts_every_arm` to `eight_arm_cardinality_counts_every_arm`, change its comment's "SEVEN"/"seven" to "EIGHT"/"eight", append an eighth single-arm entry and bump the length assertion:

```rust
            AccessLogFilter {
                not_health_check_filter: Some(NotHealthCheckFilter {}),
                ..AccessLogFilter::default()
            },
        ];
        assert_eq!(single_arms.len(), 8, "eight arms must be covered");
```

and in the same test rename `all_seven` to `all_eight` (both occurrences) and give it the eighth arm SET — after the `E0063` sweep in Step 6 inserts `not_health_check_filter: None,` into that literal, change it to:

```rust
            not_health_check_filter: Some(NotHealthCheckFilter {}),
```

- [ ] **Step 3: Write the failing compile test**

In `crates/envoy-http1/src/hcm.rs`'s `mod tests`, after `compile_access_log_filter_builds_metadata_arm_with_wrapper_default`:

```rust
    /// Phase 116: the eighth tuple arm compiles `not_health_check_filter: {}`
    /// to the unit `NotHealthCheck` predicate, and it also compiles NESTED —
    /// the composition arms recurse through the same function.
    #[test]
    fn compile_access_log_filter_builds_not_health_check_arm() {
        let leaf = envoy_config::AccessLogFilter {
            not_health_check_filter: Some(envoy_config::NotHealthCheckFilter {}),
            ..Default::default()
        };
        let compiled = compile_access_log_filter(&leaf);
        assert!(matches!(
            compiled,
            envoy_accesslog::LogFilter::NotHealthCheck
        ));
        assert!(!compiled.should_log(200, "-", &[], &Default::default(), 2, true));
        assert!(compiled.should_log(200, "-", &[], &Default::default(), 2, false));

        let nested = envoy_config::AccessLogFilter {
            and_filter: Some(envoy_config::AndFilter {
                filters: vec![
                    envoy_config::AccessLogFilter {
                        not_health_check_filter: Some(envoy_config::NotHealthCheckFilter {}),
                        ..Default::default()
                    },
                    envoy_config::AccessLogFilter {
                        not_health_check_filter: Some(envoy_config::NotHealthCheckFilter {}),
                        ..Default::default()
                    },
                ],
            }),
            ..Default::default()
        };
        let compiled = compile_access_log_filter(&nested);
        assert!(matches!(
            &compiled,
            envoy_accesslog::LogFilter::And(children)
                if children.len() == 2
                    && children
                        .iter()
                        .all(|c| matches!(c, envoy_accesslog::LogFilter::NotHealthCheck))
        ));
        assert!(!compiled.should_log(200, "-", &[], &Default::default(), 2, true));
    }
```

- [ ] **Step 4: Run to verify the failure**

Run: `cargo test -p envoy-config --lib not_health_check`
Expected: compile errors — `NotHealthCheckFilter` and the field do not exist.

- [ ] **Step 5: Add the struct, its `Deserialize`, and the eighth arm**

In `crates/envoy-config/src/bootstrap.rs`, the `AccessLogFilter` doc comment's arm list becomes:

```rust
/// predicate carried by an `AccessLog` entry. This type models EIGHT oneof arms —
/// `status_code_filter` (phase 70), `response_flag_filter` (phase 71),
/// `header_filter` (phase 72), the recursive `and_filter` / `or_filter`
/// composition (phase 73), `metadata_filter` (phase 74),
/// `grpc_status_filter` (phase 114) and `not_health_check_filter` (phase 116);
/// future
```

and the struct gains its last field, followed by the new type:

```rust
    /// Phase 116: the EIGHTH `AccessLogFilter` arm — drops a record iff the
    /// health_check filter ANSWERED the request. Mutually exclusive with the
    /// other arms. ⚠ A YAML null (`not_health_check_filter: ~`) is `None` here,
    /// an UNSET arm — exactly as upstream reads it (MEASURED).
    pub not_health_check_filter: Option<NotHealthCheckFilter>,
}

/// Phase 116: `envoy.config.accesslog.v3.NotHealthCheckFilter` — an EMPTY,
/// closed message. A MAP with no keys (`{}`) is its only spelling; any key,
/// and any non-map value (`[]`, `[1]`, `true`, `0`, `""`), is REJECTED
/// (MEASURED upstream with `--mode validate`).
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct NotHealthCheckFilter {}

/// Hand-rolled because a DERIVED `Deserialize` on an empty braced struct also
/// accepts a zero-length SEQUENCE: `not_health_check_filter: []` would load,
/// where upstream rejects it (MEASURED). The visitor accepts maps only, and
/// rejects the first key it sees.
impl<'de> serde::Deserialize<'de> for NotHealthCheckFilter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = NotHealthCheckFilter;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an empty NotHealthCheckFilter map (`{}`)")
            }
            fn visit_map<M>(self, mut map: M) -> Result<NotHealthCheckFilter, M::Error>
            where
                M: MapAccess<'de>,
            {
                match map.next_key::<String>()? {
                    Some(key) => Err(M::Error::unknown_field(&key, &[])),
                    None => Ok(NotHealthCheckFilter {}),
                }
            }
        }
        deserializer.deserialize_map(V)
    }
}
```

⚠ **Do NOT replace the hand-rolled `Deserialize` with `#[derive(Deserialize)]` + `#[serde(default, deny_unknown_fields)]`.** That is what every sibling leaf does and what `SPEC.md` §4 item 1 prescribes, and it ACCEPTS `not_health_check_filter: []` (MEASURED: `not_health_check_filter_rejects_every_measured_reject` goes RED, `4 passed; 1 failed`, against exactly that derive). Upstream rejects it. `Serialize` stays derived: an empty braced struct serializes as `{}`, which the visitor accepts (the round-trip test pins it).

In `crates/envoy-config/src/lib.rs`, add `NotHealthCheckFilter` to the `pub use bootstrap::{…}` list after `Node,` (`cargo fmt` reflows the block):

```rust
    Mutations, NetworkFilter, NetworkRbacConfig, Node, NotHealthCheckFilter, OrFilter,
```

- [ ] **Step 6: Sweep the `E0063` literals — a WORKSPACE fixpoint loop**

Every exhaustive `AccessLogFilter { … }` literal now misses a field. Cargo stops at the first failing crate, so loop until the WHOLE workspace builds. This helper (save it outside the repo) inserts `not_health_check_filter: None,` into each reported literal:

```python
# Insert `not_health_check_filter: None,` into each `AccessLogFilter { … }`
# literal that cargo reports as E0063 (missing field). Input: `file:line:col`
# triples on stdin (from `--message-format=short`). Brace-matches from the
# reported position to the literal's closing `}`.
import sys, collections
sites = collections.defaultdict(set)
for line in sys.stdin:
    f, l, c = line.strip().split(':')[:3]
    sites[f].add((int(l), int(c)))
for f, locs in sites.items():
    lines = open(f).read().split('\n')
    # process bottom-up so earlier offsets stay valid
    for l, c in sorted(locs, reverse=True):
        text = '\n'.join(lines)
        off = sum(len(x) + 1 for x in lines[:l - 1]) + (c - 1)
        o = text.index('{', off)
        depth, p = 1, o + 1
        while depth:
            if text[p] == '{': depth += 1
            elif text[p] == '}': depth -= 1
            p += 1
        close = p - 1
        before = text[:close].rstrip()
        sep = '' if before.endswith(',') or before.endswith('{') else ','
        text = before + sep + ' not_health_check_filter: None, ' + text[close:]
        lines = text.split('\n')
    open(f, 'w').write('\n'.join(lines))
    print(f, len(locs))
```

```bash
while :; do
  out=$(cargo build --workspace --all-targets --message-format=short 2>&1)
  locs=$(echo "$out" | grep 'error\[E0063\]: missing field `not_health_check_filter`' | cut -d: -f1-3 | sort -u)
  [ -z "$locs" ] && break
  echo "$locs" | python3 /path/to/e0063_fix.py
done
cargo fmt --all
```

Expected (MEASURED): two rounds — `crates/envoy-config/src/bootstrap.rs 4`, then `crates/envoy-http1/src/hcm.rs 11`. **15 literals**, matching the 14 `grpc_status_filter: None` initialisers plus the one `grpc_status_filter: Some(…)` in the cardinality test. The loop also stops on any OTHER compile error; read `$out` if it exits early. Then make Step 2's `all_eight` edit.

- [ ] **Step 7: Grow the validator to eight arms**

In `validate_access_log_filter`, change its doc comment's "all SEVEN arms" to "all EIGHT arms" and grow the destructure and the `set_arms` array:

```rust
    let AccessLogFilter {
        status_code_filter,
        response_flag_filter,
        header_filter,
        and_filter,
        or_filter,
        metadata_filter,
        grpc_status_filter,
        not_health_check_filter,
    } = filter;
    let set_arms = [
        status_code_filter.is_some(),
        response_flag_filter.is_some(),
        header_filter.is_some(),
        and_filter.is_some(),
        or_filter.is_some(),
        metadata_filter.is_some(),
        grpc_status_filter.is_some(),
        // Phase 116: the empty message has nothing else to validate.
        not_health_check_filter.is_some(),
    ]
    .iter()
    .filter(|set| **set)
    .count();
```

No new error variant: the empty message has nothing else to validate, and a field inside it is a serde rejection.

- [ ] **Step 8: Grow `compile_access_log_filter` to an eight-tuple**

Every existing arm's pattern gains a trailing `, None`, the tuple gains `&f.not_health_check_filter`, and the new arm precedes the `unreachable!`. The whole function becomes:

```rust
/// Phase 70/71/72/73/74/114/116 — translate a config-side `AccessLogFilter` into the
/// runtime `LogFilter` predicate the sink evaluates per record. The envoy-config
/// validator (`validate_access_logs`) already enforced that exactly one oneof
/// arm is set, so the 0/multi-arm cases are `unreachable!` (CF-70-1: the
/// zero-arm `expect()` is gone). EIGHT arms ship: `status_code_filter` (phase
/// 70), `response_flag_filter` (phase 71), `header_filter` (phase 72), the
/// recursive `and_filter`/`or_filter` composition arms (phase 73), which map
/// each nested child via `.iter().map(compile_access_log_filter)`,
/// `metadata_filter` (phase 74), `grpc_status_filter` (phase 114) and
/// `not_health_check_filter` (phase 116).
fn compile_access_log_filter(f: &envoy_config::AccessLogFilter) -> envoy_accesslog::LogFilter {
    match (
        &f.status_code_filter,
        &f.response_flag_filter,
        &f.header_filter,
        &f.and_filter,
        &f.or_filter,
        &f.metadata_filter,
        &f.grpc_status_filter,
        &f.not_health_check_filter,
    ) {
        (Some(scf), None, None, None, None, None, None, None) => {
            let op = match scf.comparison.op {
                envoy_config::ComparisonOp::Eq => envoy_accesslog::FilterOp::Eq,
                envoy_config::ComparisonOp::Ge => envoy_accesslog::FilterOp::Ge,
                envoy_config::ComparisonOp::Le => envoy_accesslog::FilterOp::Le,
            };
            envoy_accesslog::LogFilter::StatusCode(envoy_accesslog::StatusCodeComparison {
                op,
                // `runtime_key` is RTDS-inert here — the comparison always uses
                // `default_value` (see `RuntimeUInt32`'s envoy-config doc comment).
                threshold: scf.comparison.value.default_value,
            })
        }
        (None, Some(rff), None, None, None, None, None, None) => {
            envoy_accesslog::LogFilter::ResponseFlag {
                flags: rff.flags.clone(),
            }
        }
        // Phase 72 (ADR-0150): box the config `HeaderMatcher` into the injected
        // `HeaderMatch` seam. The validator already compiled its SafeRegex, so
        // the runtime `matches` never hits its `.expect()`.
        (None, None, Some(hf), None, None, None, None, None) => {
            envoy_accesslog::LogFilter::Header {
                matcher: std::sync::Arc::new(hf.header.clone()),
            }
        }
        // Phase 73: the two composition arms map each child recursively.
        (None, None, None, Some(af), None, None, None, None) => envoy_accesslog::LogFilter::And(
            af.filters.iter().map(compile_access_log_filter).collect(),
        ),
        (None, None, None, None, Some(of), None, None, None) => envoy_accesslog::LogFilter::Or(
            of.filters.iter().map(compile_access_log_filter).collect(),
        ),
        // Phase 74 (ADR-0150/ADR-0155): box the config `MetadataMatcher` into
        // the injected `MetadataMatch` seam (the validator already compiled its
        // SafeRegex, so the runtime `matches` never hits its `.expect()`), and
        // resolve the `google.protobuf.BoolValue` wrapper default — absent means
        // `true` (MEASURED, SPEC §0 R-0.4; `--mode validate` provably cannot
        // reach this). A matcher-less `metadata_filter` (accepted upstream,
        // R-0.2) compiles to `matcher: None`, so every record takes the
        // not-found policy.
        (None, None, None, None, None, Some(mf), None, None) => {
            envoy_accesslog::LogFilter::Metadata {
                matcher: mf.matcher.as_ref().map(|m| {
                    std::sync::Arc::new(m.clone())
                        as std::sync::Arc<dyn envoy_accesslog::MetadataMatch>
                }),
                match_if_key_not_found: mf.match_if_key_not_found.unwrap_or(true),
            }
        }
        // Phase 114: the seventh arm. `statuses` tokens are resolved to codes
        // here (the validator already proved every one resolves), so the runtime
        // predicate is a plain integer membership test.
        (None, None, None, None, None, None, Some(gsf), None) => {
            envoy_accesslog::LogFilter::GrpcStatus {
                codes: gsf
                    .statuses
                    .iter()
                    .map(|t| {
                        envoy_config::resolve_grpc_status_token(t).expect(
                            "validated by validate_access_logs: every status token resolves",
                        )
                    })
                    .collect(),
                exclude: gsf.exclude,
            }
        }
        // Phase 116: the eighth arm. The message is empty, so there is nothing
        // to carry; the bit is supplied per record by the HCM.
        (None, None, None, None, None, None, None, Some(_)) => {
            envoy_accesslog::LogFilter::NotHealthCheck
        }
        _ => unreachable!("validated by validate_access_logs: exactly one filter arm is set"),
    }
}
```

- [ ] **Step 9: Run the tests to verify they pass**

```bash
cargo test -p envoy-config --lib
cargo test -p envoy-http1 --lib
```

Expected: PASS (740 and 255 at this boundary on the prototype).

- [ ] **Step 10: Verify and commit**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs crates/envoy-http1/src/hcm.rs
git commit -m "phase 116 task 3: the not_health_check_filter config arm, its map-only Deserialize, validation and compile"
```

---

## Task 4: In-process pins on BOTH codecs (`SPEC.md` §2.2, PV-3, PV-5, `CF-116-2`)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (a shared YAML builder extracted from the phase-115 helper; the H1 pin)
- Modify: `crates/envoy-http2/src/hcm.rs` (the H2 pin)

**Interfaces:**
- Consumes: everything from Tasks 1–3, end to end, through `envoy_config::parse_bootstrap` and `HCMConfig::from_config`.
- Produces: test-only helpers `h1_config_from_bootstrap_yaml`, `not_health_check_h1_config`, `h2_not_health_check_run` and the type alias `H2Probe`.

⚠ **These are CHARACTERIZATION pins: they PASS as soon as they are written**, because Tasks 1–3 already implemented the behaviour. **Their RED evidence is Step 6's mutations, not Step 2** — a test asserting that a record is KEPT passes vacuously against a placeholder, so the mutation is the proof. Do not skip it.

The pins cover the cells fixture `0099` cannot: listener B (no health_check filter, row 8), `x-envoy-internal` (row 10), the H2 codec (`CF-116-2`), an H1 `HEAD` intercept and two keep-alive orderings (Review Focus 3 and 4).

- [ ] **Step 1: Extract the shared H1 builder**

In `crates/envoy-http1/src/hcm.rs`'s `mod tests`, `health_check_h1_config_around` ends by parsing its YAML and building the config. Move that tail into its own function so the new pin can reuse it — the end of `health_check_h1_config_around` becomes:

```rust
            log = log.display()
        );
        h1_config_from_bootstrap_yaml(&yaml).await
    }

    /// Build the H1 `HCMConfig` of the FIRST listener's HCM in `yaml`, through
    /// `parse_bootstrap` (which stamps `node.cluster`) and
    /// `HCMConfig::from_config`.
    async fn h1_config_from_bootstrap_yaml(
        yaml: &str,
    ) -> (Arc<HCMConfig>, Arc<envoy_stats::StatsRegistry>) {
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("bootstrap loads");
```

The rest of the moved body (from `let Some(envoy_config::TypedConfig::HttpConnectionManager(hcm))` to `(config, registry)`) is unchanged.

- [ ] **Step 2: Write the H1 pin**

Immediately before the phase-115 test `h1_health_check_filter_end_to_end` (above its doc comment):

```rust
    /// Phase 116: an H1 HCM whose ONE sink carries
    /// `filter: { not_health_check_filter: {} }`, over the fixture-`0099`
    /// chain `[rbac DENY on x-deny present, health_check :path exact /healthz,
    /// router]` — or, with `with_health_check: false`, the same chain without
    /// the health_check filter (`SPEC.md` §2.2 listener B).
    async fn not_health_check_h1_config(
        log: &std::path::Path,
        with_health_check: bool,
    ) -> Arc<HCMConfig> {
        let health_check = if with_health_check {
            r#"                  - name: envoy.filters.http.health_check
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.health_check.v3.HealthCheck
                      pass_through_mode: false
                      headers:
                        - { name: ":path", string_match: { exact: /healthz } }
"#
        } else {
            ""
        };
        let yaml = format!(
            r#"
node: {{ id: n, cluster: hc-node-cluster }}
static_resources:
  listeners:
    - name: l
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 0 }} }}
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
                      not_health_check_filter: {{}}
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: {log}
                      log_format:
                        text_format_source:
                          inline_string: "%REQ(:METHOD)% %REQ(:PATH)% %RESPONSE_CODE%\n"
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: MAIN }} }}
                http_filters:
                  - name: envoy.filters.http.rbac
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC
                      rules:
                        action: DENY
                        policies:
                          deny-flagged:
                            permissions: [ {{ any: true }} ]
                            principals: [ {{ header: {{ name: x-deny, present_match: true }} }} ]
{health_check}                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#,
            log = log.display()
        );
        h1_config_from_bootstrap_yaml(&yaml).await.0
    }

    /// Phase 116: `not_health_check_filter` end to end on H1 — every cell of
    /// `SPEC.md` §2.2 that one H1 listener can express, plus listener B (no
    /// health_check filter, row 8), which fixture `0099` cannot.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_not_health_check_filter_drops_exactly_the_intercepts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let send = |method: &str, path: &str, header: &str| {
            format!("{method} {path} HTTP/1.1\r\nHost: x\r\n{header}Connection: close\r\n\r\n")
        };

        let log_a = dir.path().join("a.log");
        let config = not_health_check_h1_config(&log_a, true).await;
        for (req, status) in [
            (send("GET", "/healthz", ""), "200"),     // row 1: dropped
            (send("POST", "/healthz", ""), "200"),    // row 2: dropped
            (send("GET", "/healthz?x=1", ""), "200"), // row 3: falls through
            (send("GET", "/other", ""), "200"),       // row 4
            (send("GET", "/healthz", "x-deny: 1\r\n"), "403"), // row 6: RBAC answers first
            (send("GET", "/ua", "user-agent: Envoy/HC\r\n"), "200"), // row 7
            (send("HEAD", "/healthz", ""), "200"),    // a HEAD intercept: dropped
        ] {
            let resp = String::from_utf8(drive(Arc::clone(&config), req.as_bytes()).await).unwrap();
            assert!(
                resp.starts_with(&format!("HTTP/1.1 {status} ")),
                "{req:?} -> {resp}"
            );
        }
        // The bit is per REQUEST: on one keep-alive connection the intercept is
        // dropped and its neighbour kept, in either order (MEASURED upstream).
        for pipelined in [
            "GET /healthz HTTP/1.1\r\nHost: x\r\n\r\nGET /after HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
            "GET /before HTTP/1.1\r\nHost: x\r\n\r\nGET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        ] {
            let resp =
                String::from_utf8(drive(Arc::clone(&config), pipelined.as_bytes()).await).unwrap();
            assert_eq!(resp.matches("HTTP/1.1 200 ").count(), 2, "{resp}");
        }

        let log_b = dir.path().join("b.log");
        let config = not_health_check_h1_config(&log_b, false).await;
        for req in [
            send("GET", "/healthz", ""), // row 8: no filter to answer
            send("GET", "/int", "x-envoy-internal: true\r\n"), // row 10
        ] {
            let resp = String::from_utf8(drive(Arc::clone(&config), req.as_bytes()).await).unwrap();
            assert!(resp.ends_with("MAIN"), "{req:?} -> {resp}");
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            tokio::fs::read_to_string(&log_a).await.expect("log a"),
            "GET /healthz?x=1 200\nGET /other 200\nGET /healthz 403\nGET /ua 200\n\
             GET /after 200\nGET /before 200\n"
        );
        assert_eq!(
            tokio::fs::read_to_string(&log_b).await.expect("log b"),
            "GET /healthz 200\nGET /int 200\n"
        );
    }
```

- [ ] **Step 3: Write the H2 pin**

In `crates/envoy-http2/src/hcm.rs`'s `mod tests`, immediately before the phase-115 test `h2_health_check_filter_intercepts_and_logs` (above its doc comment):

```rust
    /// One `h2_not_health_check_run` request: method, URI, one optional header.
    type H2Probe<'a> = (&'a str, &'a str, Option<(&'a str, &'a str)>);

    /// Phase 116 (`CF-116-2`: the H2 arm has no differential witness, so it is
    /// pinned HERE): drive `requests` over ONE H2 connection to an HCM whose
    /// one sink carries `filter: { not_health_check_filter: {} }`, over the
    /// chain `[rbac DENY on x-deny present, health_check :path exact /healthz,
    /// router]` — without the health_check filter when `with_health_check` is
    /// false. Returns each response status and the access log.
    async fn h2_not_health_check_run(
        with_health_check: bool,
        requests: &[H2Probe<'_>],
    ) -> (Vec<u16>, String) {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = dir.path().join("access.log");
        let health_check = if with_health_check {
            r#"                  - name: envoy.filters.http.health_check
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.health_check.v3.HealthCheck
                      pass_through_mode: false
                      headers:
                        - { name: ":path", string_match: { exact: /healthz } }
"#
        } else {
            ""
        };
        let yaml = format!(
            r#"
node: {{ id: n, cluster: hc-node-cluster }}
static_resources:
  listeners:
    - name: l
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 0 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http_h2
                codec_type: HTTP2
                access_log:
                  - name: envoy.access_loggers.file
                    filter:
                      not_health_check_filter: {{}}
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: {log}
                      log_format:
                        text_format_source:
                          inline_string: "%REQ(:METHOD)% %REQ(:PATH)% %RESPONSE_CODE%\n"
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: MAIN }} }}
                http_filters:
                  - name: envoy.filters.http.rbac
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC
                      rules:
                        action: DENY
                        policies:
                          deny-flagged:
                            permissions: [ {{ any: true }} ]
                            principals: [ {{ header: {{ name: x-deny, present_match: true }} }} ]
{health_check}                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#,
            log = log.display()
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("bootstrap loads");
        let Some(envoy_config::TypedConfig::HttpConnectionManager(hcm)) =
            &bootstrap.static_resources.listeners[0].filter_chains[0].filters[0].typed_config
        else {
            panic!("HCM expected");
        };
        let config = Arc::new(
            Http1HCMConfig::from_config(
                hcm,
                Arc::new(envoy_cluster::ClusterManager::empty()),
                Arc::new(envoy_stats::StatsRegistry::new()),
                None,
                Arc::new(RuntimeSnapshot::default()),
            )
            .await
            .expect("HCM config builds"),
        );
        let (addr, _server) = spawn_h2_hcm(config).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let mut statuses = Vec::new();
        for (method, uri, header) in requests {
            let mut req = http::Request::builder().method(*method).uri(*uri);
            if let Some((name, value)) = header {
                req = req.header(*name, *value);
            }
            let (response_fut, _) = send_request
                .send_request(req.body(()).unwrap(), true)
                .unwrap();
            let (parts, mut body) = response_fut.await.expect("response").into_parts();
            while let Some(chunk) = body.data().await {
                let chunk = chunk.unwrap();
                let _ = body.flow_control().release_capacity(chunk.len());
            }
            statuses.push(parts.status.as_u16());
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let lines = tokio::fs::read_to_string(&log).await.unwrap_or_default();
        (statuses, lines)
    }

    /// Phase 116: `not_health_check_filter` on H2 — the `SPEC.md` §2.2 cells,
    /// listener B (row 8) included.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_not_health_check_filter_drops_exactly_the_intercepts() {
        let (statuses, lines) = h2_not_health_check_run(
            true,
            &[
                ("GET", "http://x/healthz", None),
                ("POST", "http://x/healthz", None),
                ("GET", "http://x/healthz?x=1", None),
                ("GET", "http://x/healthz", Some(("x-deny", "1"))),
                ("GET", "http://x/ua", Some(("user-agent", "Envoy/HC"))),
            ],
        )
        .await;
        assert_eq!(statuses, [200, 200, 200, 403, 200]);
        assert_eq!(
            lines,
            "GET /healthz?x=1 200\nGET /healthz 403\nGET /ua 200\n"
        );

        let (statuses, lines) =
            h2_not_health_check_run(false, &[("GET", "http://x/healthz", None)]).await;
        assert_eq!(statuses, [200]);
        assert_eq!(lines, "GET /healthz 200\n");
    }
```

The `H2Probe` alias is not decoration: without it `clippy -D warnings` fails with `very complex type used` on the parameter (MEASURED).

- [ ] **Step 4: Run both pins**

```bash
cargo test -p envoy-http1 --lib not_health_check
cargo test -p envoy-http2 --lib not_health_check
```

Expected: PASS — 2 tests on H1 (the pin and Task 3's compile test), 1 on H2.

- [ ] **Step 5: Run the phase-115 tests the extraction touched**

Run: `cargo test -p envoy-http1 --lib health_check`
Expected: PASS — every phase-115 test that goes through `health_check_h1_config_around` still passes (this is Review Focus 5's pin).

- [ ] **Step 6: Mutation — the RED evidence (PV-3)**

Save this script outside the repo. It swaps ONE codec's production `is_health_check` argument for an expression, and restores it:

```python
# Replace ONE codec's production `is_health_check` argument with an
# expression, or restore it. Usage (from the repo root):
#   python3 mutate_bit.py h1|h2 '<expr>'   # mutate
#   python3 mutate_bit.py h1|h2 CONTROL    # restore the real predicate
import sys
codec, expr = sys.argv[1], sys.argv[2]
path = {'h1': 'crates/envoy-http1/src/hcm.rs', 'h2': 'crates/envoy-http2/src/hcm.rs'}[codec]
pad = {'h1': ' ' * 20, 'h2': ' ' * 16}[codec]
real = (pad + 'envoy_filter::health_check::answered_by_health_check(\n'
        + pad + '    record.response_code_details.as_deref(),\n'
        + pad + '),\n')
t = open(path).read()
if expr == 'CONTROL':
    assert t.count('// MUTATED\n') == 1, 'nothing to restore'
    start = t.index(pad + '/* MUTATED */')
    end = t.index('// MUTATED\n', start) + len('// MUTATED\n')
    t = t[:start] + real + t[end:]
else:
    assert t.count(real) == 1, 'the real predicate must occur exactly once'
    t = t.replace(real, pad + '/* MUTATED */ ' + expr + ', // MUTATED\n')
open(path, 'w').write(t)
```

For EACH codec (`h1` with `cargo test -p envoy-http1 --lib not_health_check`, `h2` with `cargo test -p envoy-http2 --lib not_health_check`), run the three mutations and the control:

```bash
python3 /path/to/mutate_bit.py h1 true                                  # expect the pin RED
python3 /path/to/mutate_bit.py h1 CONTROL
python3 /path/to/mutate_bit.py h1 false                                 # expect the pin RED
python3 /path/to/mutate_bit.py h1 CONTROL
python3 /path/to/mutate_bit.py h1 'req.path.starts_with("/healthz")'    # expect the pin RED
python3 /path/to/mutate_bit.py h1 CONTROL                               # expect GREEN; git diff empty
```

On H2 the request-path expression is `envoy_req.path.starts_with("/healthz")`. MEASURED on the prototype: all six mutations RED (`h1_not_health_check_filter_drops_exactly_the_intercepts` / `h2_not_health_check_filter_drops_exactly_the_intercepts` FAILED, every other test unaffected), both controls GREEN. After the last restore, `git diff --stat` must show only this task's two files with their intended edits.

- [ ] **Step 7: Verify and commit**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git add crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 116 task 4: pin not_health_check_filter in-process on H1 and H2"
```

---

## Task 5: Differential fixture `0099-accesslog-not-health-check-filter`

**Files:**
- Create: `tests/fixtures/0099-accesslog-not-health-check-filter/envoy-rust.yaml`, `envoy.yaml`, `expectations.yaml`, `README.md`
- Create: `tests/differential/tests/accesslog_not_health_check_filter.rs`

**Interfaces:**
- Consumes: the whole feature (Tasks 1–3) through the release `envoy-bin`, and the EXISTING `Driver::Http1AccessLogByteExact`. `tests/differential/src/lib.rs` is NOT modified.
- Produces: nothing later tasks consume.

The YAMLs below are the exact files that were dry-run on both proxies at this PLAN-write (PV-4) and turned GREEN on the prototype. `{{PORT}}` is the only token.

- [ ] **Step 1: Create `envoy-rust.yaml`**

```yaml
node: { id: envoy-rust-phase-116-fixture-0099, cluster: envoy-rust-phase-116 }
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
                      not_health_check_filter: {}
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0099-envoy-rust-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "NHC %REQ(:METHOD)% %REQ(:PATH)% %RESPONSE_CODE% UA=%REQ(USER-AGENT)%\n"
                    # Phase 116 — the `not_health_check_filter` arm: drop a
                    # record iff the health_check filter ANSWERED the request.
                    #
                    # ⚠ The format must NOT render `%RESPONSE_CODE_DETAILS%`:
                    # the RBAC 403 (probe 5) is a KEPT row, and envoy-rust's
                    # RBAC sets no details (`CF-116-3`) where upstream renders
                    # `rbac_access_denied_matched_policy[deny-flagged]`.
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        # One catch-all. A request the health_check filter does
                        # not answer falls through to it and reads "MAIN".
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "MAIN" }
                http_filters:
                  # RBAC runs FIRST, so an `x-deny` request is answered 403
                  # before the health_check filter sees it (probe 5).
                  - name: envoy.filters.http.rbac
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC
                      rules:
                        action: DENY
                        policies:
                          "deny-flagged":
                            permissions:
                              - any: true
                            principals:
                              - header:
                                  name: x-deny
                                  present_match: true
                  - name: envoy.filters.http.health_check
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.health_check.v3.HealthCheck
                      pass_through_mode: false
                      headers:
                        - name: ":path"
                          string_match: { exact: "/healthz" }
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 2: Create `envoy.yaml`**

It is `envoy-rust.yaml` plus exactly four harness hunks (the upstream `admin:` block, the `0.0.0.0` bind, `generate_request_id: false`, and the `0099-envoy-mount` log path). The `filter:` block is byte-identical on both sides — check it with `md5sum` over the block, not by eye.

```yaml
node: { id: envoy-rust-phase-116-fixture-0099, cluster: envoy-rust-phase-116 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                generate_request_id: false
                access_log:
                  - name: envoy.access_loggers.file
                    filter:
                      not_health_check_filter: {}
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0099-envoy-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "NHC %REQ(:METHOD)% %REQ(:PATH)% %RESPONSE_CODE% UA=%REQ(USER-AGENT)%\n"
                    # Phase 116 — the `not_health_check_filter` arm: drop a
                    # record iff the health_check filter ANSWERED the request.
                    #
                    # ⚠ The format must NOT render `%RESPONSE_CODE_DETAILS%`:
                    # the RBAC 403 (probe 5) is a KEPT row, and envoy-rust's
                    # RBAC sets no details (`CF-116-3`) where upstream renders
                    # `rbac_access_denied_matched_policy[deny-flagged]`.
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        # One catch-all. A request the health_check filter does
                        # not answer falls through to it and reads "MAIN".
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "MAIN" }
                http_filters:
                  # RBAC runs FIRST, so an `x-deny` request is answered 403
                  # before the health_check filter sees it (probe 5).
                  - name: envoy.filters.http.rbac
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC
                      rules:
                        action: DENY
                        policies:
                          "deny-flagged":
                            permissions:
                              - any: true
                            principals:
                              - header:
                                  name: x-deny
                                  present_match: true
                  - name: envoy.filters.http.health_check
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.health_check.v3.HealthCheck
                      pass_through_mode: false
                      headers:
                        - name: ":path"
                          string_match: { exact: "/healthz" }
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 3: Create `expectations.yaml`**

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0099-envoy-mount/access.log
    envoy_rust: /tmp/0099-envoy-rust-mount/access.log
  # The sink's filter is `not_health_check_filter: {}`: drop a record iff the
  # health_check filter ANSWERED the request. Six probes on six distinct
  # (method, path, header) combinations; two suppressed, four KEPT.
  probes:
    # Probe 1 — SUPPRESSED, the decisive drop. The health_check filter
    # answers `GET /healthz` (200, empty body).
    - method: get
      path: /healthz
      host: envoy-rust.test
      expected_status: 200
      expect_logged: false
    # Probe 2 — SUPPRESSED. The filter is method-agnostic, so the drop is too.
    - method: post
      path: /healthz
      host: envoy-rust.test
      expected_status: 200
      expect_logged: false
    # Probe 3 — KEPT. `:path` is matched WITH its query string, so this
    # near-miss falls through to the route ("MAIN") and is not a health check.
    - method: get
      path: /healthz?x=1
      host: envoy-rust.test
      expected_status: 200
      expect_logged: true
    # Probe 4 — KEPT. The plain control.
    - method: get
      path: /other
      host: envoy-rust.test
      expected_status: 200
      expect_logged: true
    # Probe 5 — KEPT, and THE CELL THAT NEEDS THE FILTER TO RUN. The path
    # matches the health_check matcher, but RBAC answers 403 FIRST, so the
    # health_check filter never decides it. An arm that re-evaluates the
    # matcher against the request drops this row; the correct arm keeps it.
    - method: get
      path: /healthz
      host: envoy-rust.test
      extra_headers:
        - ["x-deny", "1"]
      expected_status: 403
      expect_logged: true
    # Probe 6 — KEPT, and LAST so the driver pays the SHORT suppression
    # settle. Upstream's own active-health-checker user-agent does NOT make a
    # request a health check: the arm keys on the filter's decision only.
    - method: get
      path: /ua
      host: envoy-rust.test
      extra_headers:
        - ["user-agent", "Envoy/HC"]
      expected_status: 200
      expect_logged: true
  # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`), plus an exact
  # per-side line count. Four lines per side, MEASURED at the PLAN-write
  # against both real proxies:
  #   NHC GET /healthz?x=1 200 UA=-
  #   NHC GET /other 200 UA=-
  #   NHC GET /healthz 403 UA=-
  #   NHC GET /ua 200 UA=Envoy/HC
  # Every route is a direct_response -> `clusters: []`, no backend spawns.
```

- [ ] **Step 4: Create the runner**

```rust
//! Fixture 0099 — the `not_health_check_filter` access-log FILTER arm (phase
//! 116), the EIGHTH of the twelve `AccessLogFilter` oneof arms.
//!
//! Cluster-free and backend-free (`clusters: []`, one `direct_response`
//! route), so it is verifiable on a development host rather than CI-only. Six
//! probes; two suppressed, four kept.
//!
//! ⚠ Probes 5 and 6 are what make this fixture non-vacuous against the
//! plausible wrong implementations. Probe 5 is a `/healthz` that RBAC answers
//! before the health_check filter runs; probe 6 carries upstream's
//! health-checker user-agent. Both are KEPT, so an arm keyed on the request
//! instead of the filter's decision goes RED.

use std::path::PathBuf;

#[tokio::test]
async fn accesslog_not_health_check_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0099-accesslog-not-health-check-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 5: Run the fixture**

```bash
cargo build -p envoy-bin
cargo test -p differential --test accesslog_not_health_check_filter
```

Expected: PASS against both real proxies (needs Docker and the pinned image). The fixture is cluster-free and backend-free, so it is verifiable on a development host. ⚠ If `CARGO_TARGET_DIR` is set it must be an ABSOLUTE path: the harness resolves it from the test's working directory, so a relative one reports `envoy-bin not found`.

- [ ] **Step 6: Mutation — the fixture is non-vacuous (PV-3)**

With Task 4's `mutate_bit.py`, on the H1 site only (the fixture is H1), and **rebuilding `envoy-bin` before every run** — a stale binary reports the previous tree:

```bash
for m in true false 'req.path.starts_with("/healthz")'; do
  python3 /path/to/mutate_bit.py h1 "$m"
  cargo build -p envoy-bin
  cargo test -p differential --test accesslog_not_health_check_filter
  python3 /path/to/mutate_bit.py h1 CONTROL
done
cargo build -p envoy-bin
cargo test -p differential --test accesslog_not_health_check_filter     # the control
```

MEASURED on the prototype, each from the same tree:

| the H1 bit is … | envoy-rust emits | verdict |
|---|---|---|
| forced `true` | `0 access-log lines but 4 were expected` | RED |
| forced `false` | `envoy_rust=6, envoy=4` — the two intercepts leak (`CF-71-1` settle check) | RED |
| `req.path.starts_with("/healthz")` | 2 lines — `/healthz?x=1` (probe 3) and the RBAC 403 (probe 5) lost | RED |
| unmutated (md5 of `hcm.rs` equal to the committed file) | 4 lines, byte-identical | GREEN |

- [ ] **Step 7: Create the fixture README**

````markdown
# Fixture 0099 — the `not_health_check_filter` access-log FILTER arm

Phase 116. The cross-proxy witness for `not_health_check_filter`, the **EIGHTH**
of upstream Envoy's twelve `envoy.config.accesslog.v3.AccessLogFilter` `oneof`
arms, after `status_code_filter` (70), `response_flag_filter` (71),
`header_filter` (72), `and_filter`/`or_filter` (73), `metadata_filter` (74) and
`grpc_status_filter` (114).

This is a **log-emission predicate**, not a formatter. It drops a record iff
the request was **answered by the downstream `envoy.filters.http.health_check`
filter** (phase 115).

- **Driver:** the EXISTING `Driver::Http1AccessLogByteExact` (`kind:
  http1_access_log_byte_exact`). No new driver, no harness change —
  `tests/differential/src/lib.rs` is untouched by this phase.
- **Shape:** one H1 HCM listener with the chain `[rbac, health_check, router]`;
  ONE `FileAccessLog` sink carrying `filter: { not_health_check_filter: {} }`;
  one `prefix: "/"` `direct_response` route answering `200 MAIN`;
  `clusters: []`, **no backend spawns**.
  - `rbac` is `action: DENY` with one policy whose principal is
    `header: { name: x-deny, present_match: true }`.
  - `health_check` is `pass_through_mode: false` with one matcher,
    `":path"` `string_match: { exact: "/healthz" }`.
- **Cluster-free and backend-free**, therefore verifiable on a development host.
- Six probes; **four are kept**.

## The format string (identical on both sides)

```
NHC %REQ(:METHOD)% %REQ(:PATH)% %RESPONSE_CODE% UA=%REQ(USER-AGENT)%\n
```

⚠ It does **not** render `%RESPONSE_CODE_DETAILS%`. Probe 5's RBAC 403 is a KEPT
row, and envoy-rust's RBAC filter sets no details where upstream renders
`rbac_access_denied_matched_policy[deny-flagged]`. That gap is pre-existing,
unrelated to this arm, and banked as `CF-116-3`. `%REQ(USER-AGENT)%` is rendered
so probe 6's kept line shows the health-checker user-agent really was sent.

## The MEASURED rule

Measured against `envoyproxy/envoy:v1.33.0`
(`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) on
six sinks over two listeners at the phase-116 pick, and this exact fixture was
dry-run on both proxies at the PLAN-write.

1. **The arm drops EXACTLY the requests the health_check filter answered.**
   The drop is method-agnostic (probe 2), and a `/healthz?x=1` that falls
   through is kept (probe 3).
2. **The marking comes from the filter's DECISION, not the request.** A request
   carrying upstream's active-health-checker user-agent `Envoy/HC` is kept
   (probe 6).
3. **The filter must actually RUN.** A `/healthz` that RBAC answers first is kept
   (probe 5), although its path matches the health_check matcher.

The pick also measured that the same `GET /healthz` is **kept** on a listener
with no health_check filter. One fixture holds one listener's chain, so that
cell is pinned in-process instead, on both codecs:
`h1_not_health_check_filter_drops_exactly_the_intercepts`
(`crates/envoy-http1/src/hcm.rs`) and
`h2_not_health_check_filter_drops_exactly_the_intercepts`
(`crates/envoy-http2/src/hcm.rs`).

## Why probes 5 and 6 make this fixture non-vacuous

Probes 1 and 2 defeat an always-true arm; probes 3, 4 and 6 defeat an
always-false arm. Probes 5 and 6 defeat the plausible WRONG implementations —
an arm keyed on the request instead of on the filter's decision. Proved by
MUTATION at the PLAN-write, each from the same tree with `envoy-bin` rebuilt:

| the H1 bit is … | envoy-rust emits | verdict |
|---|---|---|
| forced `true` | 0 lines (4 expected) | RED |
| forced `false` | 6 lines (4 expected) — the two intercepts leak | RED |
| `req.path.starts_with("/healthz")` | 2 lines — probes 3 and 5 lost | RED |
| unmutated | 4 lines, byte-identical to upstream | GREEN |

## Authoring constraints (all load-bearing)

1. **The format must not render `%RESPONSE_CODE_DETAILS%`** (`CF-116-3`).
2. **RBAC must precede health_check** in `http_filters`, or probe 5 stops
   testing rule 3.
3. **Every probe is a distinct (method, path, header) combination**, so each
   kept line is attributable.
4. **The LAST probe is KEPT**, so the driver's ordering-aware suppression settle
   charges the cheap short wait rather than the long one.
5. **`{{PORT}}` is the only token.** `Http1AccessLogByteExact` does not receive
   `{{ADMIN_PORT}}`, so the upstream `admin:` block uses a literal
   `port_value: 0`.

## The two YAMLs

`envoy.yaml` is `envoy-rust.yaml` plus exactly **four harness hunks**, none of
them semantic, the same four every landed access-log byte-exact fixture carries:

```
1a2    > admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
5c6    <       address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
       >       address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
12a14  >                 generate_request_id: false
19c21  <                       path: /tmp/0099-envoy-rust-mount/access.log
       >                       path: /tmp/0099-envoy-mount/access.log
```

**The `filter:` block is byte-identical on both sides** (verified by md5 over the
block). The two log paths live in different parent directories because the
driver bind-mounts only the upstream side's parent.

## The assertion

Pure **cross-proxy equality**: whole-line `==` between upstream Envoy v1.33.0 and
envoy-rust, plus an exact per-side line count. Four lines per side,
byte-identical:

```
NHC GET /healthz?x=1 200 UA=-
NHC GET /other 200 UA=-
NHC GET /healthz 403 UA=-
NHC GET /ua 200 UA=Envoy/HC
```
````

- [ ] **Step 8: Verify PV-8 and commit**

```bash
git diff --stat HEAD -- Cargo.toml Cargo.lock .github tests/differential/src/lib.rs   # must print NOTHING
git add tests/fixtures/0099-accesslog-not-health-check-filter tests/differential/tests/accesslog_not_health_check_filter.rs
git commit -m "phase 116 task 5: differential fixture 0099 for not_health_check_filter"
```

---

## Task 6: `BEHAVIOR_CONTRACT.md` — the config surface and the runtime rule

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md`

- [ ] **Step 1: Insert the section**

Immediately AFTER the phase-114 `grpc_status_filter` section (i.e. directly above the heading `### Phase 75 (ADR-0156/0157/0158/0159/0161/0162): \`HeaderMatcher\` ABSENCE semantics …`), insert:

```markdown
### Phase 116 (ADR-0205/0206): `not_health_check_filter` — the EIGHTH emission-gate arm (the HEALTH-CHECK-DECISION gate)

> `filter: { not_health_check_filter: {} }`

Upstream `envoy.config.accesslog.v3.NotHealthCheckFilter`. Drops a record iff
the request was **answered by the downstream `envoy.filters.http.health_check`
filter** (phase 115). Every rule below was MEASURED against
`envoyproxy/envoy:v1.33.0`
(`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`), on
the config surface via `--mode validate` with a negative control, and at runtime
via six sinks over two listeners.

**§A The config surface — an EMPTY, CLOSED message.**

| `filter:` | verdict | envoy-rust |
|---|---|---|
| `not_health_check_filter: {}`, alone, or nested in `and_filter` / `or_filter` | **ACCEPT** | loads |
| `not_health_check_filter: { foo: 1 }` | **REJECT** | serde: unknown field |
| `not_health_check_filter: []`, `[1]`, `true`, `0`, `""` | **REJECT** | serde: invalid type |
| `not_health_check_filter: ~`, or the bare key with no value | **REJECT** — read as an UNSET oneof (`filter_specifier … is required`), not as an empty message | `None`, then `ConfigError::AmbiguousAccessLogFilter { "no filter variant is set" }` |
| `not_health_check_filter: {}` plus a second arm | **REJECT** (oneof) | `AmbiguousAccessLogFilter { "more than one filter variant is set" }` |

⚠ **`[]` is the trap.** A DERIVED serde `Deserialize` on an empty braced struct
accepts a zero-length SEQUENCE, so the obvious one-line struct would LOAD a
config upstream rejects. `NotHealthCheckFilter` therefore carries a hand-rolled
map-only visitor (`crates/envoy-config/src/bootstrap.rs`).

**§B The runtime rule — the arm reads the filter's DECISION, never the request.**

1. It drops EXACTLY the requests the health_check filter answered, whatever the
   method; a `/healthz?x=1` that falls through to the route is KEPT.
2. The same `GET /healthz` is KEPT on a listener with no health_check filter. A
   request carrying upstream's active-health-checker user-agent `Envoy/HC`, or
   `x-envoy-internal: true`, is KEPT.
3. A `/healthz` that an earlier filter answers first — an RBAC deny, 403 — is
   KEPT, although its path matches the health_check matcher. The filter must
   actually RUN and answer.
4. It composes like any other leaf inside `and_filter` / `or_filter`.

envoy-rust: `LogFilter::NotHealthCheck` evaluates `!is_health_check`, the sixth
`should_log` argument. Both HCMs compute it AFTER the decode-side filter pass,
from the final `%RESPONSE_CODE_DETAILS%`, by
`envoy_filter::health_check::answered_by_health_check` — the SAME predicate that
excludes an intercepted request from `downstream_rq_Nxx` (phase 115), so the
two consumers cannot disagree. ⚠ A future filter that reuses the
`health_check_ok` details string is a health check to BOTH.

**§C Mutual exclusion.** `not_health_check_filter` joins the `AccessLogFilter`
oneof as the **EIGHTH** arm — exactly one may be set at each level, enforced by
`validate_access_logs`, NOT by serde.

**§D envoy-rust scope — what is and is not implemented.**

- **Both codecs implement the arm.** The H2 arm has no cross-proxy fixture and is
  pinned in-process only — **`CF-116-2`**.
- **Pass-through mode is not reopened.** Upstream also marks a request as a
  health check in `pass_through_mode: true`, while forwarding it. envoy-rust
  rejects `pass_through_mode: true` at load (`CF-115-5`), so the cell cannot be
  expressed. When it lands, the bit must follow the filter's MATCH, not its local
  reply.
- **Four arms remain unbuilt**: `duration_filter`, `runtime_filter`,
  `traceable_filter`, `extension_filter` — **`CF-116-1`**.

**§E Authoritative fixture.** `0099-accesslog-not-health-check-filter`: chain
`[rbac DENY on x-deny present, health_check :path exact /healthz, router]`, one
sink, format `NHC %REQ(:METHOD)% %REQ(:PATH)% %RESPONSE_CODE% UA=%REQ(USER-AGENT)%`,
`clusters: []`, no backend. **SIX probes, FOUR kept.**

| probe | request | observed | kept | the rule it pins |
|---|---|---|---|---|
| 1 | `GET /healthz` | 200 (intercepted) | no | §B 1 |
| 2 | `POST /healthz` | 200 (intercepted) | no | §B 1, method-agnostic |
| 3 | `GET /healthz?x=1` | 200 `MAIN` | **yes** | §B 1, a near-miss falls through |
| 4 | `GET /other` | 200 `MAIN` | **yes** | the control |
| 5 | `GET /healthz` + `x-deny: 1` | **403** | **yes** ⚠ | §B 3, the filter must RUN |
| 6 | `GET /ua` + `user-agent: Envoy/HC` | 200 `MAIN` | **yes** ⚠ | §B 2, not keyed on the request |

The format does NOT render `%RESPONSE_CODE_DETAILS%`: probe 5 is a kept RBAC
403, and envoy-rust's RBAC sets no details where upstream renders
`rbac_access_denied_matched_policy[deny-flagged]` — pre-existing, **`CF-116-3`**.
§B 2's no-health-check-listener cell cannot share a fixture with this chain; it
is pinned in-process on both codecs.

**Probes 5 and 6 (⚠) are the non-vacuity witnesses.** Proved by MUTATION, not
asserted: deriving the bit from the request path makes envoy-rust emit **2**
lines where **4** are expected (probes 3 and 5 lost); forcing it `true` emits 0
and forcing it `false` leaks both intercepts; the unmutated control is GREEN
from the same tree. Assertion is pure cross-proxy equality plus an exact
per-side count.
```

- [ ] **Step 2: Verify and commit**

```bash
grep -c '^### Phase 116 (ADR-0205/0206)' docs/envoy-rust/BEHAVIOR_CONTRACT.md   # expect 1
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 116 task 6: BEHAVIOR_CONTRACT — the not_health_check_filter arm"
```

---

## After the last task

State 3 ends here. The §5 state-4 verification gate (a SEPARATE session) runs the full `BOOTSTRAP_PROMPT.md` §7.5 gate: `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`, the differential suite and the conformance suites. No fuzz target is added: this arm adds no parser — its config is an empty message (`SPEC.md` §5 non-goal 5). The CI identity is predicted to move from `binaries=174 passed=2360 failed=0` to **`binaries=175`** (one new differential test binary) with **+14** tests: envoy-filter +1, envoy-accesslog +4, envoy-config +5, envoy-http1 +2, envoy-http2 +1, and the new fixture runner +1 (MEASURED on the prototype: 175 binaries, 2369 passed + 5 failed = 2374 = 2360 + 14).

---

## Carry-forwards this phase opens or amends

Unchanged from `SPEC.md` §10 — this PLAN-write opens NONE.

- **CF-116-1**: four `AccessLogFilter` arms remain unbuilt (`duration_filter`, `runtime_filter`, `traceable_filter`, `extension_filter`).
- **CF-116-2**: the H2 arm has no differential witness; it is pinned in-process by `h2_not_health_check_filter_drops_exactly_the_intercepts` (Task 4).
- **CF-116-3**: envoy-rust's RBAC filter sets no `%RESPONSE_CODE_DETAILS%`; upstream renders `rbac_access_denied_matched_policy[<policy>]`.
- **CF-115-3** is CONSUMED by this phase. **CF-114-2** is AMENDED (five unbuilt arms → four, restated as CF-116-1). Every other banked carry-forward is untouched.

---

## Self-review

1. **Spec coverage.** `SPEC.md` §4 items 1–8 map to: item 1 → Task 3 Step 5; item 2 → Task 3 Step 7; item 3 → Task 2; item 4 → Task 1 (+ PV-2 above); item 5 → Task 3 Step 8; item 6 → Task 3 Step 6; item 7 → Task 5; item 8 → Task 6. §2.1's every acceptance cell → Task 3 Step 1. §2.2 rules 1–5 → Task 2 (rules 1, 2, 4), Task 4 (rules 1–3 on both codecs, row 8), Task 5 (rules 1–3 cross-proxy), Task 3 (rule 5). §5 non-goals: no task touches the four other arms, pass-through mode, RBAC details, or the four PV-8 files.
2. **Placeholder scan.** Every code step carries the exact code, cut by script from the measured tree; the generator asserted every extraction anchor occurs exactly once.
3. **Type consistency.** `answered_by_health_check(Option<&str>) -> bool`, `is_health_check: bool` (sixth, last), `LogFilter::NotHealthCheck` (unit), `NotHealthCheckFilter {}`, `not_health_check_filter: Option<NotHealthCheckFilter>` are spelled identically in every task.
4. **Review Focus.** All five lines have a named owning test; lines 1–4 are added by this plan, line 5 is a landed test this plan must keep green.
