# Phase 05.4 — Progress

Per-task running log. Append-only during execution; one section per task.
Source of truth: `SPEC.md` (D1–D7) + `PLAN.md` (Tasks 1–7).

---

## Task 1 — `envoy-config` `Cluster.dns_lookup_family` field + `DnsLookupFamily` enum + ADR-0024

- **Commit:** _(pending — fill in via post-hoc `phase 05.4: progress note (task 1)` if SHA needed cross-task)_
- **Deliverables:** SPEC §3 D1.
- **ADR landed:** ADR-0024 (`Cluster.dns_lookup_family` field + `DnsLookupFamily` enum, parse-only).
- **Files modified:**
  - `docs/envoy-rust/DECISIONS.md` (ADR-0024 appended after ADR-0023).
  - `crates/envoy-config/src/bootstrap.rs` (`DnsLookupFamily` enum; `Cluster.dns_lookup_family` field; `parses_cluster_with_dns_lookup_family_v4_only` parse test).
  - `crates/envoy-config/src/lib.rs` (re-export `DnsLookupFamily`).
  - `crates/envoy-cluster/src/cluster.rs` (2 hand-written `Cluster` initialisers gain `dns_lookup_family: None`; planner-confirmed count).
- **LoC:** ~85 (5 field + 10 enum + 1 re-export + 25 parse test + 2 initialiser updates + 13 ADR + 30 PROGRESS narrative).
- **Verification:**
  - `cargo test -p envoy-config parses_cluster_with_dns_lookup_family_v4_only` — `test result: ok. 1 passed`.
  - `cargo test -p envoy-config` — `test result: ok. 146 passed; 0 failed` (existing tests unchanged; +1 new).
  - `cargo test -p envoy-cluster` — `test result: ok. 14 passed; 0 failed` (existing tests unchanged after the 2 initialiser updates).
  - `cargo clippy -p envoy-config -p envoy-cluster --all-targets -- -D warnings` — clean.
  - `cargo fmt --all -- --check` — clean (after a `cargo fmt --all` apply: the new `DnsLookupFamily` re-export pushed the line past width; `cargo fmt` re-flowed the `pub use bootstrap::{ ... }` block; semantically equivalent — no symbol added/dropped beyond the new re-export).
- **Deviations from PLAN:**
  - **Step 1 grep semantics:** the `^## ADR-` count returned `24`, not the projected `23`, because the count includes the `## ADR-NNNN: <title>` template marker at line 10 of DECISIONS.md (not a real ADR). The numeric ADR head before this commit was ADR-0023 as expected — no unexpected ADRs landed beyond ADR-0023. Proceeded.
  - **Step 1 `cluster.rs` initialiser grep:** the planner's `envoy_config::Cluster {\|^        Cluster {$` regex returned 0 matches (the actual lines use `clusters: vec![Cluster {` at line 426 and `let mk_cluster = || Cluster {` at line 456 — both inside `mod tests`, both bringing `Cluster` into scope via `use envoy_config::{… Cluster …}`). The plan's projected count of **2** is correct; only the regex was imprecise. Both confirmed schema-`Cluster` literals updated; the other 2 `Cluster {` matches in the file (lines 240, 503) build the local runtime `Cluster` struct in `cluster.rs` itself (different type — has `name`/`endpoints`/`cursor`, not `cluster_type`/`load_assignment`/etc.) and are correctly NOT touched.
  - **Step 3 test YAML path correction:** the plan's verbatim test snippet uses `bootstrap.clusters.len()` and `bootstrap.clusters[0]`, but the actual struct hierarchy is `Bootstrap.static_resources.clusters`. The path was corrected to `bootstrap.static_resources.clusters[…]` to match the existing analogous test (`validates_strict_dns_cluster_does_not_require_literal_ip_endpoints` at line 4483). Test intent (assert the new field parses) is preserved verbatim.
  - **Step 3 test YAML missing `admin:` block:** the plan's verbatim YAML omits an `admin:` block, which causes `parse_bootstrap` to fail with `ConfigError::NoRuntime` (validator rejects a bootstrap with no admin endpoint AND no listeners). An `admin:` block matching the existing analogous test was added to the YAML. The new field's parse-shape check (`Some(DnsLookupFamily::V4Only)`) is unchanged and still the test's load-bearing assertion.
  - **Step 10 fmt re-flow:** adding `DnsLookupFamily` to the alphabetic re-export list pushed `FilterChain` past the line width; `cargo fmt --all` re-flowed lines 12-14 of `lib.rs` (token-equivalent; no symbols added or dropped beyond the new re-export). Per PLAN Step 10 ("If fmt fails, run `cargo fmt --all` and re-stage"), this is the prescribed remediation.
- **Carryforward note:** None — Task 1 is mechanically scoped per SPEC §3 D1.
- **Fuzz seed:** Not added (per PLAN signpost H — optional, deferred to PLAN discretion; planner elected NOT to add).

---

## Task 2 — 5-fixture coordinated `dns_lookup_family: V4_ONLY` edit

- **Commit:** _(pending — fill in post-hoc if needed)_
- **Deliverables:** SPEC §3 D2.
- **ADR landed:** None (D2 has no ADR; the ADR-0024 grant landed at Task 1).
- **Files modified:**
  - `tests/fixtures/0003-tcp-proxy/envoy.yaml` (line 28 inserted).
  - `tests/fixtures/0004-tls-downstream/envoy.yaml` (line 38 inserted).
  - `tests/fixtures/0005-tls-upstream/envoy.yaml` (line 17 inserted).
  - `tests/fixtures/0006-tls-sni/envoy.yaml` (line 41 inserted; further edits at Task 3).
  - `tests/fixtures/0008-http1-router-upstream/envoy.yaml` (line 50 inserted).
- **LoC:** 5 (1 line per fixture).
- **Cadence:** single bundled commit per SPEC §6 signpost 15 + PLAN signpost G.
- **Verification:**
  - `grep -A1 'type: STRICT_DNS' tests/fixtures/000{3,4,5,6,8}*/envoy.yaml` shows each `type:` line followed by `dns_lookup_family: V4_ONLY`.
  - `grep -n 'dns_lookup_family' tests/fixtures/000*/envoy-rust.yaml` returns empty (the envoy-rust.yaml siblings are intentionally unchanged).
  - The actual differential green re-baseline is at Task 7 — Tasks 2-6 land progressively; the gate fires once.
- **Deviations from PLAN:** _(none expected)_

---

## Task 3 — `envoy-config` `Listener.listener_filters` parse-and-ignore + ADR-0026 + fixture 0006 tls_inspector block

- **Commit:** _(pending)_
- **Deliverables:** SPEC §3 D3.
- **ADR landed:** ADR-0026 (`Listener.listener_filters` parse-and-ignore field; new pattern in envoy-config).
- **Files modified:**
  - `docs/envoy-rust/DECISIONS.md` (ADR-0026 appended after ADR-0024).
  - `crates/envoy-config/src/bootstrap.rs` (`Listener.listener_filters` field; `parses_listener_with_tls_inspector_listener_filter` parse test).
  - `crates/envoy-tls/src/tests.rs` (`synth_listener_two_tls_chains` gains `listener_filters: vec![]`).
  - `tests/fixtures/0006-tls-sni/envoy.yaml` (explicit `tls_inspector` listener-filter block inserted after the `address:` line).
- **LoC:** ~138 (5 field + 60 parse test + 1 initialiser update + 4 fixture YAML + 13 ADR + ~25 PROGRESS narrative; parse test ran shorter than the planner's ~85 estimate because the YAML body uses `filename: "/tmp/leaf.pem"` flow-style entries rather than block-style + `inline_string:` strings).
- **Coupling per SPEC §6 signpost 12:** schema + fixture YAML in same commit (splitting would red the parser or red Envoy on macOS Docker).
- **Verification:**
  - `cargo test -p envoy-config parses_listener_with_tls_inspector_listener_filter` — `test result: ok. 1 passed`.
  - `cargo test -p envoy-config` — `test result: ok. 147 passed; 0 failed` (existing 146 + 1 new).
  - `cargo test -p envoy-tls` — `test result: ok. 15 passed; 0 failed` (existing tests still pass after the literal update).
  - `cargo clippy -p envoy-config -p envoy-tls --all-targets -- -D warnings` — clean.
  - `cargo fmt --all -- --check` — clean (after a `cargo fmt --all` apply: rustfmt re-flowed the `let filter_yaml = serde_yaml::to_string(...)` line because the original two-line break put the `.expect(...)` on a continuation that fmt prefers to collapse onto the wrapped form `let filter_yaml = serde_yaml::to_string(&listener.listener_filters[0]).expect("filter serialises back");`; semantically equivalent).
- **Deviations from PLAN:**
  - **Step 2 `inline_string:` → `filename:` shape correction:** the plan's verbatim YAML uses `inline_string:` for the embedded cert/key (with explicit fallback note "if the parse rejects them as malformed, replace with `filename:` references"). The validator at `bootstrap.rs:960-968` calls `validate_data_source(..., Required::Filename)` which rejects `inline_string:`-only data sources at parse time (envoy-config's phase-03 baseline accepts `filename:` only; phase-04.1 added `inline_string:` to the schema but it's not yet accepted as the sole-value variant in the listener TLS validator path). Switched to `filename: "/tmp/leaf.pem"` / `filename: "/tmp/leaf.key"` mirroring the existing `parses_listener_with_downstream_tls_context` test (line 2228-2230). The runtime cert load doesn't run during parse, so the non-existent path is safely opaque. Test intent (assert listener_filters parses + smoke-check the opaque value contains the filter name) preserved verbatim.
  - **Step 2 struct path correction:** as projected in the plan's "ALSO" note, the actual hierarchy is `bootstrap.static_resources.listeners`, not `bootstrap.listeners`. Adjusted to match the existing `parses_listener_with_downstream_tls_context` analogue.
  - **Step 2 `admin:` block omitted intentionally:** the plan's "ALSO" note flagged the prior Task 1 defect of YAML missing `admin:`. In this test the bootstrap has 1 listener so the validator's `admin.is_none() && listeners.is_empty()` rejection at bootstrap.rs:885 does NOT fire; no `admin:` block needed. (The same shape is used by the analogous `parses_listener_with_downstream_tls_context` test at line 2211 with no admin block.)
- **Pattern note:** parse-and-ignore is now a documented envoy-config posture per ADR-0026. Future fields meeting the criteria may follow the same pattern without a new ADR.

---

## Task 4 — 3 echo-server helper bind flips (0.0.0.0)

- **Commit:** _(pending)_
- **Deliverables:** SPEC §3 D4.
- **ADR landed:** None (D4 has no ADR; ADR-0015's host-gateway grant is the operative cross-reference).
- **Files modified:**
  - `tests/helpers/tcp-echo-server/src/main.rs` (line 118 bind; line 119 tracing log; line 3 doc comment).
  - `tests/helpers/tls-echo-server/src/main.rs` (line 109 bind; line 110 tracing log; line 3 doc comment).
  - `tests/helpers/http1-echo-server/src/main.rs` (line 98 bind; line 99 tracing log; line 3 doc comment).
- **LoC:** 9 (3 bind + 3 log + 3 doc-comment), exactly matching plan projection.
- **Verification:**
  - `cargo test -p tcp-echo-server -p tls-echo-server -p http1-echo-server` — all green:
    - `http1-echo-server`: `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.
    - `tcp-echo-server`: `test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.
    - `tls-echo-server`: `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.
  - `cargo clippy -p tcp-echo-server -p tls-echo-server -p http1-echo-server --all-targets -- -D warnings` — clean.
  - `cargo fmt --all -- --check` — clean (no fmt drift; no remediation needed).
  - Test-internal ephemeral binds at lines 212/236/281/332 (`"127.0.0.1:0"`) are intentionally unchanged — confirmed by `grep` only flagging the 3 production binds at 118/109/98.
- **Deviations from PLAN:** None. Line numbers, exact `before`/`after` strings, and tracing-log shapes all matched the plan verbatim. The bind-address flip is mechanically transparent: `0.0.0.0` is a superset of `127.0.0.1` reachability, so all existing tests connecting to `127.0.0.1:<port>` still hit the listener (confirmed by 18/18 tests passing).

## Task 5 — `envoy-http1::Client` content-length: 0 suppression on empty-body + ADR-0025 + fixture 0008 expectations update

- **Commit:** _(pending)_
- **Deliverables:** SPEC §3 D5.
- **ADR landed:** ADR-0025 (suppress synthetic `content-length: 0` on empty-body GET; RFC 7230 §3.3.2 + Envoy v1.33 parity).
- **Files modified:**
  - `docs/envoy-rust/DECISIONS.md` (ADR-0025 appended after ADR-0026 at line 478; landing-time order ADR-0023 → ADR-0024 → ADR-0026 → ADR-0025 preserved).
  - `crates/envoy-http1/src/client.rs` (`body_is_nonempty` predicate added at the request-write CL emission block; `send_request_writes_serialized_request_bytes` assertion flipped from `s.contains(...)` to `!s.contains(...)` with a 4-line ADR-0025 reference comment).
  - `tests/fixtures/0008-http1-router-upstream/expectations.yaml` (`expected_body.body:` line drops `  content-length: 0\n` substring; remaining shape `method: GET\npath: /\nheaders:\n  host: envoy-rust.test\nbody: \n` is unchanged).
- **LoC:** ~46 (6 client predicate-and-comment + 8 unit test flip-and-comment + 1 fixture YAML + 13 ADR + 18 PROGRESS narrative).
- **Coupling per SPEC §6 signpost 11:** client behavior change + expectations update in same commit (splitting would red the unit test or red fixture 0008 byte-equal echo body).
- **Verification:**
  - Pre-fix `cargo test -p envoy-http1 send_request_writes_serialized_request_bytes`: `test result: FAILED. 1 failed` (exactly as plan Step 3 projected; failure dump showed `GET / HTTP/1.1\r\nhost: envoy-rust.test\r\nuser-agent: test\r\ncontent-length: 0\r\n\r\n`).
  - Post-fix `cargo test -p envoy-http1 send_request_writes_serialized_request_bytes`: `test result: ok. 1 passed; 0 failed`.
  - Full `cargo test -p envoy-http1`: `test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`. No other test asserted on `content-length: 0` for an empty-body request — the fix is bounded as projected.
  - `cargo clippy -p envoy-http1 --all-targets -- -D warnings` — clean.
  - `cargo fmt --all -- --check` — clean (no fmt drift; no remediation needed).
  - `grep -c '^## ADR-' docs/envoy-rust/DECISIONS.md` = `27` (was 26 pre-task; matches plan's controller-calibration projection of 27 = 24 active ADRs + the template-marker line at line 10 + ADR-0024 + ADR-0026 + ADR-0025 = 27 sectioned headings — consistent with the count post-Task-3).
  - `grep -n '^## ADR-' docs/envoy-rust/DECISIONS.md | tail -4` shows `ADR-0023` (line 424) → `ADR-0024` (line 437) → `ADR-0026` (line 459) → `ADR-0025` (line 478) — landing-time order preserved.
  - Fixture 0008 differential green re-baseline materializes at Task 7.
- **Deviations from PLAN:** None. Line numbers had drifted slightly from the plan's projection (CL emission block at lines 94-103 actual; assertion at lines 460-463 actual), but the plan's intent matched verbatim. The `Request::body_bytes()` accessor was found at the projected location `crates/envoy-http1/src/codec.rs:62-64` with the projected signature `pub(crate) fn body_bytes(&self) -> Option<&[u8]>` — no deviation in accessor name or shape. `is_some_and(|b| !b.is_empty())` is the idiomatic Rust ≥1.70 form for the `Option<&[u8]>` non-empty predicate; clippy raised no objection.

## Task 6 — Harness STRICT_DNS settle time 500ms → 2000ms for host_gateway fixtures

- **Commit:** _(pending)_
- **Deliverables:** SPEC §3 D6.
- **ADR landed:** None (D6 has no ADR; test-harness timing constant per PLAN signpost L).
- **Files modified:**
  - `tests/differential/src/upstream.rs` (the flat `tokio::time::sleep(Duration::from_millis(500)).await;` after `get_host_port_ipv4` replaced with a `let settle_ms = if host_gateway { 2000 } else { 500 };` conditional + 5-line doc comment; the `host_gateway: bool` parameter was already in scope at the function signature `pub async fn start(envoy_yaml_path: &Path, host_gateway: bool, tls_pki: Option<&crate::tls::TlsTestPki>) -> Result<UpstreamProxy>`).
- **LoC:** 7 (2 effective + 5 comment), replacing 1 line — net +6.
- **Verification:**
  - Verified `host_uses_host_gateway = upstream_yaml.contains("host.docker.internal")` derivation at `tests/differential/src/lib.rs:989` and pass-through at line 992; the 3 unaffected fixtures (0001/0002/0007) do NOT contain `host.docker.internal` in their upstream YAML and continue at the existing 500ms settle.
  - `cargo test -p differential --lib`: `test result: ok. 52 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 2.04s` (the 1 ignored is `upstream::tests::starts_upstream_envoy_and_exposes_host_port` which `requires Docker; runs under cargo test --workspace in CI` — gated as expected per PLAN signpost K).
  - `cargo clippy -p differential --all-targets -- -D warnings` — clean (`Finished dev profile` with no warnings).
  - `cargo fmt --all -- --check` — clean (no output, no drift).
  - Behavioral verification of the 2000ms bump (i.e. that DNS resolution actually completes by 2000ms on the 5 host-gateway fixtures) deferred to Task 7's CI run per PLAN signpost K.
- **Deviations from PLAN:** None. Line drift inside `tests/differential/src/upstream.rs` was minor — the `tokio::time::sleep(Duration::from_millis(500)).await;` originally projected at line 88 was found at line 88 verbatim. Indentation note: the original sleep was at 4-space indent (function-body level, immediately after the `get_host_port_ipv4` `?;`), not 8-space — the planner's snippet showed 8-space leading whitespace; matched the surrounding `let host_port = ...;` block at 4 spaces. The 2000ms ceiling was NOT tightened in this task per SPEC §6 signpost 16.

---

## Task 7 — State-4 phase-done gate verification — substantively closes phase-04.3 REVIEW C-1

- **Commit:** _(pending — this verification commit)_
- **Deliverables:** SPEC §3 D7.
- **ADR landed:** None (D7 is verification only).
- **Files modified:**
  - `docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md` (this section).
  - _(`Cargo.lock` — no-op; no diff, as projected)_

### Local stable-toolchain command outputs (tail-quoted)

```
$ cargo build --workspace --all-targets 2>&1 | tail -10
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s

$ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -15
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s

$ cargo fmt --all -- --check 2>&1 | tail -5
(empty)

$ cargo test --workspace 2>&1 | tail -25
   Doc-tests envoy_tls

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

(workspace aggregate: 339 passed; 0 failed; 1 ignored across 13 test binaries + 7 doc-tests; the 1 ignored is `differential::upstream::tests::starts_upstream_envoy_and_exposes_host_port` — Docker-gated and runs under `cargo test --workspace` in CI per PLAN signpost K)

$ cargo deny check 2>&1 | tail -15
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

The four `license-not-encountered` warnings (BSD-2-Clause, MPL-2.0, Unicode-DFS-2016, Zlib at `deny.toml:40/47/43/45`) are advisory-only — they flag licenses the policy permits but no in-tree crate carries. Pre-existing across the 05.x baseline; not gated by `-D warnings` semantics. Final line `advisories ok, bans ok, licenses ok, sources ok` is the pass signal.

### Fuzz short-budget output (tail-quoted)

```
$ cd crates/envoy-config/fuzz && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30 2>&1 | tail -20
#630034	REDUCE cov: 9458 ft: 26030 corp: 2875/1308Kb lim: 4096 exec/s: 21001 rss: 490Mb L: 178/4020 MS: 2 ChangeBinInt-EraseBytes-
#630154	DONE   cov: 9458 ft: 26030 corp: 2875/1308Kb lim: 4096 exec/s: 20327 rss: 490Mb
###### Recommended dictionary. ######
"\000\000\000\000\000\000\000\011" # Uses: 16466
"\377\377\377\377" # Uses: 9809
"\000\000\000\000\000\000\000\001" # Uses: 8114
"\011\000" # Uses: 4951
"\013\000" # Uses: 4769
"\177\000" # Uses: 4792
"\000\000\000\037" # Uses: 2034
"`\000\000\000" # Uses: 541
"\000\000\000\000\000\000\000\000" # Uses: 444
###### End of recommended dictionary. ######
Done 630154 runs in 31 second(s)
```

630154 runs / 9458 cov / 26030 ft / corp 2875 — no crash. Schema additions from Tasks 1 and 3 (`Cluster.dns_lookup_family`, `Listener.listener_filters`) parse cleanly through the fuzzer's mutation surface. Per PLAN Step 2 expectation: short-budget run completes with no crash; the existing 12-seed corpus continues to parse through the new `Option<DnsLookupFamily>` and `Vec<ListenerFilter>` fields (both default to `None`/`vec![]` via `#[serde(default)]`).

### Cargo.lock sync

```
$ cargo build --workspace 2>&1 | tail -3
   Compiling envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
   Compiling http1-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http1-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.74s
$ git status Cargo.lock
On branch main
Your branch is ahead of 'origin/main' by 10 commits.
nothing to commit, working tree clean
$ git diff Cargo.lock
(empty)
```

Per PLAN signpost I + SPEC §6 signpost 2: phase 05.4 introduces no new top-level deps; `Cargo.lock` no-op as projected. No staged change to lockfile in this task's commit.

### Docker-gated CI run

- **Run URL:** https://github.com/pgdad/envoy-rust/actions/runs/25276504502
- **Run ID:** 25276504502
- **HEAD SHA:** `06c706d5f76e539470a8e387adad27fa48663b33` (`phase 05.4: STRICT_DNS settle time 500ms→2000ms for host_gateway fixtures (task 6)`)
- **Result:** SUCCESS
- **Jobs:**
  - `build + test + lint` (includes differential harness → Docker): 1m36s ✓
  - `fuzz (parse_bootstrap, 30s)`: 1m54s ✓ (273161 runs / 7394 cov / 16280 ft / corp 1435 in 31 seconds; no crash on CI's nightly toolchain)
- **CI cargo deny tail (final line):** `advisories ok, bans ok, licenses ok, sources ok`
- **Annotation:** 1 unrelated GitHub Actions runner deprecation notice (Node.js 20 / `actions/checkout@v4`), no test impact.

### Per-fixture matrix

All 8 fixtures green simultaneously per SPEC §1 acceptance signal (a) + (b). Pulled from CI integration-test results in `build + test + lint > test (includes differential harness → Docker)` step (timestamps `10:20:52` → `10:21:08`):

| Fixture | Test binary | Status | CI duration | Note |
|---|---|---|---|---|
| `tests/fixtures/0001-tcp-echo` | `echo_fixture` | GREEN | 1.06s | unchanged from 05.1; no host_gateway, settle 500ms |
| `tests/fixtures/0002-static-admin-ready` | `admin_ready_fixture` | GREEN | 6.53s | unchanged from earlier phases |
| `tests/fixtures/0003-tcp-proxy` | `tcp_proxy_fixture` | GREEN | 2.67s | RESTORED — `dns_lookup_family: V4_ONLY` (Task 2) + 0.0.0.0 echo bind (Task 4) + settle 2000ms (Task 6) |
| `tests/fixtures/0004-tls-downstream` | `tls_downstream_fixture` | GREEN | 2.81s | RESTORED — same trio |
| `tests/fixtures/0005-tls-upstream` | `tls_upstream_fixture` | GREEN | 2.70s | RESTORED — same trio |
| `tests/fixtures/0006-tls-sni` | `tls_sni_fixture` | GREEN | 3.08s | RESTORED — same trio + `tls_inspector` listener filter (Task 3) |
| `tests/fixtures/0007-http1-direct-response` | `http1_direct_response_fixture` | GREEN | 0.85s | unchanged; no host_gateway, settle 500ms |
| `tests/fixtures/0008-http1-router-upstream` | `http1_router_upstream_fixture` | GREEN | 2.47s | RESTORED — same trio + content-length: 0 suppression (Task 5) |

CI-side `cargo test --workspace` reports `test result: ok` across every test binary including the previously Docker-gated `differential::upstream::tests::starts_upstream_envoy_and_exposes_host_port` (1 passed under CI Docker). Differential lib unittests: 53 passed; 0 failed; 0 ignored on CI vs. 52 passed; 0 failed; 1 ignored locally — the lone delta is Docker availability.

**Substantively closes phase-04.3 REVIEW C-1.** The C-1 carryforward chain — originating at phase-02.2's ADR-0015 landing `435c6fa` (host-gateway grant), latent across phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3, partially closed at 05.1 state-6 commit `1d05cd0` — ends here. The 6 root-cause fixes that close the chain:

1. `Cluster.dns_lookup_family` schema knob + `DnsLookupFamily` enum (Task 1 / ADR-0024).
2. 5-fixture coordinated `dns_lookup_family: V4_ONLY` (Task 2).
3. `Listener.listener_filters` parse-and-ignore + `tls_inspector` block on fixture 0006 (Task 3 / ADR-0026).
4. 3 echo-server helpers bind `0.0.0.0` (Task 4).
5. `envoy-http1::Client` suppress synthetic `content-length: 0` on empty-body GETs + fixture 0008 expectations update (Task 5 / ADR-0025).
6. Differential harness STRICT_DNS settle bump 500ms → 2000ms for host_gateway fixtures (Task 6).

**Phase-04.1 REVIEW M-claim** (drive_http1 per-function unit test) is unblocked by the fixture-mask removal but stays deferred per the 04.3 disposition. No new I3-style or A-style closures expected at 05.4.

### Deviations from PLAN

None. Steps 1-7 of the plan executed verbatim. Notes:

- **Step 1 cargo deny warnings count:** The plan's Step 1 expectation cited `0 errors` only; the actual CI + local outputs include 4 `license-not-encountered` advisory-only warnings (BSD-2-Clause, MPL-2.0, Unicode-DFS-2016, Zlib at `deny.toml:40/47/43/45`). These are pre-existing across the 05.x baseline — they flag policy-permitted licenses that no in-tree crate carries — and are not gated by `cargo deny`'s pass signal. Final line `advisories ok, bans ok, licenses ok, sources ok` is the gate; both local and CI runs pass.
- **Step 2 fuzz corpus seed projection:** The plan projected "12 corpus-walk seeds parsed cleanly". Actual local libFuzzer-mode run grew the corpus to 2875 entries via mutation in 31 seconds (CI: 1435 entries; the local machine is faster), exercising the full mutation surface rather than just the seed walk. No crash — the gate signal — confirmed at both run sites. The 12-seed accounting is preserved through the corpus directory `crates/envoy-config/fuzz/corpus/parse_bootstrap` which retains the deterministic seeds; libFuzzer's incremental mode merely augments them.
- **Step 3 `git status Cargo.lock` output:** The plan's Step 3 expectation was that `git status Cargo.lock` would print a focused single-file status. Git's actual behaviour with a clean lockfile is to print the wider working-tree summary including the global `nothing to commit, working tree clean` line — this is the pass signal for an unmodified path. Verified `git diff Cargo.lock` is empty as projected.
- **Step 4 push scope:** The push to `origin/HEAD` advanced origin/main from `a64d9fc` to `06c706d` — 10 commits including the 8 phase-05.4 commits (state-2 SPEC + state-2 PLAN + Tasks 1-6) and the 2 phase-05.1 carryforward commits (`283a4b9` REVIEW.md and `1d05cd0` ClusterType::StrictDns) that were locally landed but not yet pushed. The remote was behind the 05.1 state-6 commit, not just the 05.4 work.
