# Phase 35 — `35-rbac-dynamic-metadata` — PROGRESS (state-3 implementation)

> **Lifecycle state 3 (implementation).** Driven by `superpowers:subagent-driven-development`:
> a fresh implementer subagent per task, followed by a two-stage review (spec-compliance, then
> code-quality) per task. Every task was TDD (failing test → run-fail → minimal impl → run-pass →
> commit). Read with zero prior context (D-3.4). The §A locked facts (ADR-0086) were honored
> exactly — in particular the THREE MATERIAL divergences: **(A5)** a multi-segment `path` is
> ACCEPTED by Envoy but envoy-rust rejects `path.len() != 1` boot-fatal (stricter — the flat
> string-only store cannot resolve a nested path); **(A6)** a non-`string_match` `value` is ACCEPTED
> by Envoy but the string-only `ValueMatcher` rejects it boot-fatal; **(A7)** the
> `rbac.v3.Permission.metadata`/`Principal.metadata` fields are DEPRECATED-but-functional at
> v1.33.0 (a stderr boot warning → NON-DIFFERENTIAL; envoy-rust does not emit it).

## Summary

All 6 PLAN.md tasks landed, each spec-reviewed ✅ and code-quality-reviewed ✅ (review findings
folded back in before moving on), plus a final whole-implementation review (✅ APPROVE) and two
follow-up cleanup commits. Phase 35 is the FIRST dynamic-metadata CONSUMER: it extends the EXISTING
phase-10 `envoy.filters.http.rbac` filter with a string-only `metadata` Permission/Principal
condition that reads `req.dynamic_metadata` written mid-chain by an upstream `header_to_metadata`
producer — closing the produce→consume loop the phase-33/34 arc teed up. It ADDS NO new
`HttpFilterInstance` variant and NO new infrastructure: the phase-33 store, the phase-34 producer,
the 04.x `StringMatcher`, the RBAC engine, the decision matrix + `403`/`RBAC: access denied` local
reply, the stats, and the decode-pipeline shared-`&mut FilterRequest` threading are all REUSED
UNCHANGED. The change is purely additive — a `Metadata` arm on each of the `Permission`/`Principal`
config visitors + runtime enums + lowering + eval. The cross-proxy byte-exact differential (fixture
`0043`) **passed green LIVE** against `envoyproxy/envoy:v1.33.0` on this host (prod→200+`ok\n`,
dev/absent→403+`RBAC: access denied`).

**Sequencing note (executed as planned, PLAN §File-structure):** Tasks T1→T2→T3 ran contiguously.
T1's new `Permission::Metadata`/`Principal::Metadata` config variants made the `lower_*`/`eval_*`
matches in `crates/envoy-filter/src/rbac.rs` non-exhaustive → the `envoy-filter` crate did not
compile until T3 closed those arms. T1/T2 were gated on `cargo test -p envoy-config` (T1 also added
minimal `Ok(())` accept arms to the in-crate `validate_permission_tree`/`validate_principal_tree`
so envoy-config itself compiled; T2 replaced them with the real validation). T3 closed the
cross-crate red window (`cargo build -p envoy-filter` + `cargo test -p envoy-filter rbac` green).
Each task is its own commit (the per-task commits are not individually workspace-green between T1
and T3, by design; the whole stack is pushed together).

## Per-task log

### T1 — config schema: the `MetadataMatcher` trio + `Permission`/`Principal` `"metadata"` visitor arms
**Commit `c05426f`.** Files: `crates/envoy-config/src/{bootstrap.rs,lib.rs,matcher.rs}`.
Added `MetadataMatcher { filter, path: Vec<MetadataPathSegment>, value: ValueMatcher }` +
`MetadataPathSegment { key }` + a string-only `ValueMatcher` enum (only variant
`StringMatch(StringMatcher)`, renamed `string_match`) with a hand-rolled "exactly one key"
`Deserialize` (rejects any non-`string_match` oneof key via `unknown_field` → boot-fatal, A6); all
`#[serde(deny_unknown_fields)]`. Added the `Metadata(MetadataMatcher)` arm to BOTH the `Permission`
and `Principal` hand-rolled visitors (`KEYS` += `"metadata"`) + `ValueMatcher::matches` (delegates
to the inner `StringMatcher::matches`) in `matcher.rs` + lib.rs re-exports. 4 TDD tests (parse as
Permission; parse as Principal; reject `present_match` value; reject unknown `invert` field).
**Gate:** `cargo test -p envoy-config` → 499 passed. Spec ✅ / quality ✅ (a `cargo fmt` regression
was caught at code-quality review and fixed in-amend → fmt clean).

### T2 — validator arm + `ConfigError::RbacMetadataMatcherInvalid`
**Commit `e620225`.** Files: `crates/envoy-config/src/{lib.rs,bootstrap.rs}`.
Replaced T1's placeholder `Metadata` arms in `validate_permission_tree`/`validate_principal_tree`
with a shared `validate_metadata_matcher` (empty `filter` → boot-fatal; `path.len() != 1` →
boot-fatal, the A5 stricter-than-Envoy reject, also catching empty `path: []`) + the new
`ConfigError::RbacMetadataMatcherInvalid { listener, policy_name, path, detail }` variant. 6 TDD
tests driven through `parse_bootstrap` (empty filter; multi-segment path; empty path; empty filter
under principals; valid single-segment OK; safe_regex value parse-accepted). **Note (pre-existing
limitation, documented):** the validator does NOT compile a SafeRegex inside the metadata `value`'s
StringMatcher — matching the pre-existing RBAC `Permission::Header` path (only the route-config walk
compiles header SafeRegex; the RBAC validation path is an immutable borrow). A SafeRegex metadata
value would panic at runtime; not exercised by fixture 0043 (which uses `exact`). **Gate:**
`cargo test -p envoy-config rbac_metadata` → green (10 incl. T1). Spec ✅ / quality ✅ (3 minor
polish items folded: misleading test comment, `{path:?}`→`{path}` Display, tests moved into the
`rbac_tests` submodule).

### T3 — runtime variants + eval + lowering (`rbac.rs`)
**Commit `5b2c079`.** File: `crates/envoy-filter/src/rbac.rs`.
Added `RuntimePermission::Metadata(envoy_config::MetadataMatcher)` /
`RuntimePrincipal::Metadata(...)` (holding the config matcher directly, the `Header` precedent) +
the shared `eval_metadata` helper
(`req.dynamic_metadata.get(&m.filter).and_then(|ns| ns.get(&m.path[0].key)).is_some_and(|v| m.value.matches(v))`
— absent namespace OR absent key → no match; `path[0]` safe per the T2 `path.len()==1` invariant) +
the two eval arms + the two lowering arms (clone). 5 TDD tests (present match; value-mismatch;
absent-namespace; absent-key; principal mirror). **Gate:** `cargo build -p envoy-filter` +
`cargo test -p envoy-filter rbac` → 24 passed; closed the cross-crate red window; `cargo build
--workspace` green. Spec ✅ / quality ✅ (Minor: the lowering arms + Principal-negative path are
covered by T4 through `build_from_config`).

### T4 — in-process producer→consumer backstop (test-only)
**Commit `e05fea6`.** File: `crates/envoy-filter/src/rbac.rs` (tests).
5 tests proving the load-bearing mid-chain thread + the rich composition cases the cross-proxy
fixture cannot show deterministically: `mid_chain_producer_then_consumer_allows_prod` /
`..._denies_dev` / `mid_chain_absent_header_denies` (a REAL `[header_to_metadata, rbac]`
`FilterPipeline` via the NON-gated `FilterPipeline::build_from_config`, driving
`pipeline.decode_headers` — the producer writes `dynamic_metadata` the consumer reads in the SAME
pass), plus `metadata_composes_in_and_rules` and `metadata_principal_and_deny_inversion` (via
`RbacFilter::build_from_config` + injected metadata — exercising the `AndRules`/Principal/DENY
lowering+eval arms). **Design note:** deliberately NOT gated on `#[cfg(feature = "test-util")]` and
NOT using `test_from_instances` — CI's `cargo test --workspace` does not enable `test-util`, so
gating would silently drop the backstop from CI. All 5 run under plain `cargo test`. TDD
meaningfulness was confirmed by temporarily breaking the producer (the allow-prod test failed) then
reverting. **Gate:** `cargo test -p envoy-filter` → 195 passed. Spec ✅ / quality ✅ (a uniformity
polish — status+body assertion on the two standalone denials — was folded).

### T5 — fixture `0043` differential (header-driven metadata → RBAC verdict)
**Commit `1be9899`.** Files: `tests/fixtures/0043-http-rbac-dynamic-metadata/{envoy.yaml,
envoy-rust.yaml,expectations.yaml,README.md}` + `tests/differential/tests/rbac_dynamic_metadata.rs`.
The STRONG cross-proxy byte-exact target: chain `[header_to_metadata, rbac, router]` (producer
BEFORE consumer — A2 REQUIRED), `direct_response` 200 `ok\n`, `action: ALLOW` single-policy
`metadata` Permission requiring `tier == prod`; 3 probes via `extra_headers` (prod→200+`ok\n`,
dev→403+`RBAC: access denied`, absent→403). Reuses the fixture-0017 `http1_probe_list` driver. The
two YAML sides are HCM-identical modulo the documented per-side diffs (admin/bind/
`generate_request_id`/`node.id`). NO `on_header_missing` (the absent probe must leave the key
unset). **Ran LIVE against `envoyproxy/envoy:v1.33.0` (Docker up on this host) and PASSED byte-exact
on all 3 probes** (rebuilt `envoy-bin` first, per the phase-33/34 stale-binary lesson). Spec ✅ /
quality ✅ (a README clarification — probe-3 covers absent-header, distinct from the doc-only
reorder/producer-omitted case covered by the T4 backstop — was folded).

### T6 — BEHAVIOR_CONTRACT extension + `parse_bootstrap` fuzz seed
**Commit `80c3256`.** Files: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` +
`crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_metadata.yaml` +
`crates/envoy-config/fuzz/.gitignore`.
Added the `### Phase 35 (ADR-0086): the RBAC metadata Permission/Principal condition` subsection
(facts A1–A7 + §2.2 deferrals + the honest SafeRegex caveat) and a `parse_bootstrap` corpus seed (a
`[header_to_metadata, rbac, router]` chain with a `metadata` Permission). **NO new fuzz target, NO
ci.yml change** (reuses the existing `parse_bootstrap` target). The seed required an explicit
`!corpus/parse_bootstrap/hcm_rbac_metadata.yaml` un-ignore line in the fuzz `.gitignore` (the corpus
is `*`-ignored) — verified git-tracked via `git ls-files`. Spec ✅ / quality ✅ (an editorial
clause clarifying the seed mirrors fixture 0043's chain but is a separate corpus artifact was
folded).

## Follow-up commits (post-task cleanup at the workspace-verification boundary)

### Clippy doc-lint fix
**Commit `c9822ce`.** The workspace-level `cargo clippy --all-targets --all-features -- -D warnings`
(which the per-task per-crate gates do not run — the state-4 "CI's first real execution" precedent)
flagged `doc_lazy_continuation` on the fixture-0043 differential test doc-comment (a probe list
followed by unindented continuation prose). Added a blank doc line to separate the list from the
following paragraph. Doc-only.

### Final-review minors fold
**Commit `177391c`.** The final whole-implementation review (✅ APPROVE) raised two Minor items, both
folded into one commit (`crates/envoy-config/src/bootstrap.rs` only): **(1)** restored the
`Permission` enum's doc-comment placement — T1 had inserted the new matcher types between the
`Permission` doc-comment and its enum, so the prose mis-described `MetadataMatcher` and `Permission`
lost its doc; the matcher-types block was relocated above the `Permission` doc-comment. **(2)** added
`rbac_metadata_permission_json_round_trips` — a `serde_json` round-trip test backing
BEHAVIOR_CONTRACT A1's `/config_dump` (JSON surface) claim (the YAML serializer emits `!Tag` syntax
per the hand-rolled-Deserialize rationale, so JSON is the correct round-trip; verified verbatim
snake_case keys).

## Workspace-gate sanity (controller-run at state-3 close — NOT the full §7.5 gate, which is state-4)

To avoid pushing a red tree (the full differential suite + conformance + `cargo deny` + fuzz +
`REVIEW.md` are the SEPARATE state-4/state-5 sessions):

- `cargo fmt --all -- --check` → **exit 0**.
- `cargo build --workspace --all-targets` → **green**.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → **exit 0** (after the
  `c9822ce` doc-lint fix).
- `cargo test -p envoy-config` → **505+ passed**; `cargo test -p envoy-filter` → **195 passed**;
  the `rbac_dynamic_metadata` differential → **passed live** (byte-exact, both proxies).
- `cargo test --workspace --no-fail-fast` failures are EXCLUSIVELY documented host-only false-REDs
  in code UNTOUCHED by phase 35 (CI-authoritative): `admin_config_dump_server_info` /
  `lb_maglev_fixture` / `lb_subset_fixture` / `tls_upstream_fixture` (the Docker-bridge-IP
  `192.168.65.2` backend-routing/upstream/admin-dump false-RED, memories
  `differential-host-bridge-ip-192-168-65-2` + `host-docker-desktop-virtiofs-no-inotify`) and
  `envoy-http2 client::tests::send_request_maps_h2_handshake_failure_to_typed_error` (a pre-existing
  host-environmental h2-handshake flake — the cumulative phase-35 diff touches ZERO `envoy-http2`
  files, so it cannot be a phase-35 regression). No phase-35 surface test fails.

`#![forbid(unsafe_code)]` holds (D-3.8). No new crate, no new dependency, no new `HttpFilterInstance`
variant (D-3.2). The full §7.5 phase-done gate (a)–(f) — the complete differential + conformance +
deny + fuzz short-budget CI run + `REVIEW.md` approval — is the state-4 verification session, which
is CI-authoritative.

---

## State-4 verification — §7.5 phase-done gate (`superpowers:verification-before-completion`)

> This is the §5 state-4 session. The §7.5 gate (a)–(e) was RUN and is QUOTED below ((f) `REVIEW.md`
> is the SEPARATE state-5 session). **CI run `28125206968` (push of the state-3 stack `d1cf8a3`) went
> GREEN** — it is the authoritative first real execution of the full Docker differential suite +
> conformance + `cargo deny` + the fuzz short-budget run (memory `envoy-rust-state4-ci-first-execution`):
>
> ```
> ✓ main ci · 28125206968   (triggered via push)
> ✓ build + test + lint   in 4m35s   (job 83287136008)
> ✓ fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse, 30s each)   in 4m3s   (job 83287136031)
> ```
> The build+test+lint job emitted **114 `test result: ok` lines and ZERO `FAILED`/`error[` lines**.

### (a) fixture `0043` green — cross-proxy byte-identical

CI (job 83287136008), authoritative live Docker differential vs `envoyproxy/envoy:v1.33.0`:
```
Running tests/rbac_dynamic_metadata.rs
test rbac_dynamic_metadata ... ok
```
Local re-confirmation (isolation, Docker up, `envoyproxy/envoy:v1.33.0` present):
```
$ cargo test -p differential --test rbac_dynamic_metadata
test rbac_dynamic_metadata ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
The fixture probes `X-Tier: prod`→`200`+`ok\n` / `dev`,absent→`403`+`RBAC: access denied` (19B),
byte-identical both proxies. LOCALLY observable (no reload trigger — NOT Linux-CI-only). **GREEN.**

### (b) all `0001`–`0042` still green (43 fixtures present `0001`–`0043`)

CI (job 83287136008) — the `0017` rbac header-only regression-equivalence witness + the in-process
backstop, both green; 114 `test result: ok`, 0 FAILED on the whole job:
```
Running tests/http_filter_rbac.rs
test http_filter_rbac_fixture ... ok
test http_filter_rbac_in_process_backstop ... ok
```
The `metadata` matcher is an ADDITIVE `Permission`/`Principal` enum variant that no existing config
uses (the store/producers/operator/decode-threading are byte-preserved), so `0012`/`0041`/`0042` +
the rest carry unchanged — confirmed by the 0-FAILED CI job.

**Local Docker-differential host-flakiness (CI-authoritative; NOT regressions):** under the full
parallel `cargo test --workspace --no-fail-fast` two fixtures false-RED — `admin_config_dump_server_info`
(the documented Docker-bridge-IP `192.168.65.2` divergence, memory
`differential-host-bridge-ip-192-168-65-2`) and `http_filter_fault_fixture` (HTTP/2 "connection closed
before reading preface" parallel-load timing). **Both pass when run in isolation** —
`cargo test -p differential --test http_filter_fault` → `ok. 1 passed`;
`cargo test -p differential --test http_filter_rbac` → `ok. 1 passed` on retry. Phase-35's cumulative
diff touches ZERO fault/admin/http2 files (only `envoy-config/src/{bootstrap,lib,matcher}.rs` +
`envoy-filter/src/rbac.rs` + docs + fixture `0043` + the fuzz seed), and CI ran all of them green.
**GREEN (CI).**

### (c) h2spec ≥95% (unchanged — no HTTP/2 codec change)

CI (job 83287136008), h2spec `2.6.0` pinned:
```
Running tests/h2spec_runner.rs
test tests::parse_h2spec_output_extracts_section_failure_ids ... ok
test h2spec_pass_rate_gate ... ok
```
Local: `h2spec_runner` → `test result: ok. 3 passed`. Phase 35 touches ZERO `envoy-http2`/HTTP/2-codec
files (the `§3.5/2` local false-RED of memory `h2spec-3-5-2-preface-host-sensitive` does not apply —
the CI gate is authoritative and NEVER trim `known-failures.txt` from local evidence). **GREEN.**

### (d) fuzz targets clean for the short-budget CI run — NO new fuzz target

CI fuzz job 83287136031 GREEN (4m3s): `parse_bootstrap + jwt_parse + cdn_loop_parse +
accesslog_format_parse`, 30s each. The new `parse_bootstrap` corpus seed `hcm_rbac_metadata.yaml`
exercises the existing target (NO new target). The phase-35 diff confirms NO `ci.yml` change — only
`crates/envoy-config/fuzz/.gitignore` (+1 un-ignore line) + the corpus seed (memory
`new-fuzz-target-needs-a-ci-yml-step` does not apply; memory `fuzz-corpus-seed-gitignored-by-default`
honored — the seed is git-tracked via the explicit `!`-un-ignore line). **GREEN.**

### (e) workspace gate — build / clippy / fmt / test / deny

Run locally this session (all on `d1cf8a3`, clean tree):
```
$ cargo fmt --all -- --check                                                  → exit 0
$ cargo build --workspace --all-targets                                       → Finished, exit 0
$ cargo clippy --workspace --all-targets --all-features -- -D warnings        → Finished, exit 0
$ cargo deny check                                                            → advisories ok, bans ok, licenses ok, sources ok (exit 0)
$ cargo test --workspace                                                      → green modulo the (b) documented host-only Docker-differential false-REDs; CI job 83287136008 ran the full suite GREEN (114 ok / 0 FAILED)
```
Per-crate phase-35 surfaces: `cargo test -p envoy-config` → **506 passed**; `cargo test -p envoy-filter`
→ **195 passed** (both 0 failed). `cargo deny check` warnings are benign `license-not-encountered`
allowances (no advisory/ban/source failure). **GREEN.**

### Gate verdict

(a) ✅ / (b) ✅ / (c) ✅ / (d) ✅ / (e) ✅ — all five state-4 gates PASS (CI run `28125206968` green +
local re-confirmation). The only local REDs are the documented host-only Docker-differential false-REDs
in code UNTOUCHED by phase 35 (CI-authoritative). (f) `REVIEW.md` is the SEPARATE state-5 session.

**Carry-forward Minors (NONE blocks; for state-5 `REVIEW.md`):** M34-1/M34-2/M34-3 + M33-1/M33-2 +
the empty-`metadata_match`→fallback doc-comment + M29-1/M29-2 + M30-1/M30-2 + the phase-31 M-2/M-3 +
the HTTP-filters-family (1)-(4) buffer carry-forwards. **NEW (phase-35):** the
pre-existing-SafeRegex-in-RBAC-values limitation — a `safe_regex` in an RBAC `metadata`/`header` matcher
value is accepted at config-load but not compiled → would panic at runtime; clean fix home = compile
RBAC SafeRegex at `rbac.rs` lowering time (covering both `header` and `metadata`) in a future phase.
None applies to the verification surface this session.
