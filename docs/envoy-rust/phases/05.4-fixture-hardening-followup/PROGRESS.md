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
