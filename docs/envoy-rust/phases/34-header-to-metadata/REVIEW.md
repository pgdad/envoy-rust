# Phase 34 — `header_to_metadata` — REVIEW (state-5 code-review)

> **Lifecycle state 5 (code-review).** Driven by `superpowers:requesting-code-review`:
> a fresh `superpowers:code-reviewer` subagent reviewed the phase-34 diff against `PLAN.md`,
> the §A empirically-locked facts (ADR-0084), and doctrine (D-3.2 / D-3.4 / D-3.8), with the
> reviewing session independently spot-verifying the load-bearing extraction logic. Read with
> zero prior context (D-3.4).

## Scope of review

- **Range:** `bf42699^..48f8086` (BASE `4256e3e`, HEAD `48f8086`) — the 7 task commits.
- **Diff size:** 15 files, +1226 / −11.
- **Commits:** T1 `bf42699` (config schema + `HttpFilterTypedConfig::HeaderToMetadata`),
  T2 `d27a268` (validator + `ConfigError::HeaderToMetadataInvalidRule`),
  T3 `204bad3` (`HeaderToMetadataFilter`), T4 `9f32579` (instance wiring, 12th variant),
  T5 `cac76b7` (H1+H2 in-process backstops), T6 `9c403b5` (fixture 0042 differential),
  T7 `48f8086` (BEHAVIOR_CONTRACT + `parse_bootstrap` fuzz seed).
- **Reviewed against:** `PLAN.md` (§A1–A6 locked facts, ADR-0084); doctrine D-3.2 (no new
  crate/dep/fuzz-target), D-3.4 (context isolation), D-3.8 (`#![forbid(unsafe_code)]`).

## Verdict

**APPROVE-WITH-MINORS.** The implementation encodes every §A-locked fact correctly — including
both deliberate ADR-0084 divergences (A2 default namespace = `envoy.filters.http.header_to_metadata`;
A3 static-`value`-wins precedence), each commented at the point of implementation and documented in
BEHAVIOR_CONTRACT.md. All doctrine constraints hold. The three-tier test strategy (unit + H1/H2
in-process backstops + byte-exact fixture-0042 with a genuine anti-echo probe pair) is substantive,
not shallow. **Zero Critical, zero Important findings.** Only nice-to-have test-coverage additions and
the pre-surfaced cosmetics remain — none blocks the phase. **Ready for state-6 close.**

## Strengths

- **Filter extraction logic is correct and faithful to §A** (`crates/envoy-filter/src/header_to_metadata.rs:21-49`,
  independently re-verified by the reviewing session). The three-way `match` on `found.as_deref()`
  encodes §A4's tri-state precisely: `Some(non-empty)` → present, `Some("")` → write nothing (line 33),
  `None` → missing. The §A3 static-value precedence is the single line
  `kv.value.clone().unwrap_or(header_value)` (line 41) with an accurate inline `// §A3: static value wins`
  comment. Case-insensitive lookup via `eq_ignore_ascii_case` (line 26) is correct HTTP semantics.
- **The two-layer value resolution is subtly correct and defensive.** For `on_header_present`,
  `header_value` is the real header value (line 32) and `kv.value` overrides it. For `on_header_missing`,
  `header_value` is pre-seeded to `kv.value.unwrap_or_default()` (line 38, documented unreachable per the
  §A5d validator guarantee) and the final `unwrap_or(header_value)` resolves to the same static value —
  robust even though the validator already guarantees `value.is_some()`.
- **Validator (`crates/envoy-config/src/bootstrap.rs`, the `validate_header_to_metadata_config` helper +
  `validate_http_filters` arm) implements all of §A5 (a)-(d)** and checks the filter-name mismatch
  *before* rule validity, so a wrong-`name` filter yields `UnsupportedHttpFilter` regardless of rule
  contents — correct precedence. Tests drive through the full `parse_bootstrap` entry-point (higher-value
  than testing the helper in isolation), and T2 added a **bonus** symmetric `empty-key on on_header_missing`
  test beyond the 5 the plan specified.
- **Config schema matches §A1/A2 exactly.** `default_h2m_namespace()` returns the filter canonical name
  (the A2 divergence); `key` is required (no `#[serde(default)]` → a missing key fails serde before the
  validator runs); `deny_unknown_fields` on all three structs rejects `cookie`/`remove`/`encode`; the
  single-variant `HeaderToMetadataType` enum rejects `NUMBER`/`PROTOBUF_VALUE` (the §2.2 non-string-Value
  deferral) — all intentionally stricter than Envoy, documented.
- **H1 and H2 backstops** (`crates/envoy-http1/src/hcm.rs`, `crates/envoy-http2/src/hcm.rs`) are genuine
  end-to-end tests: each drives a real request (H1 raw bytes; H2 a real `h2::client` handshake carrying
  `x-tier: prod`) through the full HCM, scrapes the file sink, and asserts `"prod / -\n"` — exercising
  both the written key and the absent-key sentinel in one line. This confirms the filter-agnostic
  threading carries the new filter's output on both codecs with no new plumbing (the PLAN's reuse claim).
- **Fixture 0042 present+missing pair is a true anti-echo guard.** Probe 2 (`/b`, no `x-tier`) renders
  `tier=missing` via `on_header_missing` — output a naive header-echo could not produce. The two yamls
  are equivalent modulo the established per-side conventions (bind IP, admin block, `generate_request_id`),
  and `value: "missing"` is correctly quoted to dodge the YAML-null→boot-fatal trap.
- **Doctrine holds:** no `unsafe`; no `Cargo.toml` change (D-3.2/D-3.8 confirmed by diff); the fuzz seed
  is `!`-un-ignored in `crates/envoy-config/fuzz/.gitignore` and `git ls-files`-tracked, targeting the
  pre-existing `parse_bootstrap` target which already runs in CI and auto-loads its corpus — no `ci.yml`
  change needed (correct).

## Findings

### Critical (Must Fix)
None.

### Important (Should Fix)
None.

### Minor (Nice to Have) — phase-34, non-blocking

- **M34-1 — no unit test for same-namespace/key overwrite (last-write-wins).**
  `crates/envoy-filter/src/header_to_metadata.rs`. Two `request_rules` writing the same
  `metadata_namespace`/`key` silently let the last rule win via `BTreeMap::insert`. This is the natural
  (and almost certainly Envoy-correct) behavior, but §A does not pin it and `multi_rule_composes` only
  covers *distinct* namespaces. A test locking last-rule-wins would prevent a silent future regression.
- **M34-2 — A2 default namespace is exercised only at the config layer, not the filter-execution layer.**
  The headline A2 divergence is proven by `header_to_metadata_default_namespace_is_filter_name`
  (config parse), but every *filter* test uses an explicit `envoy.lb` namespace. A filter-execution unit
  test that builds from a defaulted `metadata_namespace` and asserts the write lands under
  `envoy.filters.http.header_to_metadata` would close the loop end-to-end. Low value (the default is a
  plain string carried verbatim by the same `.entry()` path).
- **M34-3 (cosmetics, confirmed — none more severe than pre-assessed):** the T5 redundant function-scope
  `use tempfile::tempdir;` in `crates/envoy-http1/src/hcm.rs` (the H2 side inlines `tempfile::tempdir()`
  — harmless inconsistency); the `BEHAVIOR_CONTRACT.md` `A-missing`-vs-`A5` heading-numbering quirk; and
  the fixture `README.md` `generate_request_id … false (load-bearing)` wording (inert here — no
  `%REQ(X-REQUEST-ID)%` in the log_format — so "load-bearing" overstates a real per-side convention).

## Carry-forward Minors (still-live; NOT phase-34 regressions)

Phase 34 reused the phase-33 dynamic-metadata store + `%DYNAMIC_METADATA%` operator + filter-agnostic
H1/H2 threading **UNCHANGED**, so it did NOT touch the surfaces these Minors live on — they remain open
for the next phase that does:

- **M33-1** — unnecessary `.clone()` at `crates/envoy-http1/src/hcm.rs:1211` (single-use H1 record-build
  `dynamic_metadata` local → could move, as the H2 path does). NOT consumed: phase 34 added backstop
  tests to `hcm.rs` but did not edit the record-build path. **M33-2** — doc-pointer line drift in
  `command_operator.rs`/`record.rs` (doc-only).
- The empty-`metadata_match`→fallback doc-comment (`crates/envoy-cluster/src/subset.rs`); M29-1/M29-2,
  M30-1 (`Http1HashSweep` driver diagnostics / duplicated `extract_marker`); M30-2 (`Cluster.lb_policy`
  serde-default); the phase-31 M-2/M-3 (`cdn_loop`); the HTTP-filters-family (1)-(4) buffer carry-forwards.

## Assessment

**APPROVE-WITH-MINORS.** Production-ready. Correctness against §A is verified (incl. both ADR-0084
divergences); doctrine D-3.2/D-3.4/D-3.8 all hold; the validator matches §A5 with a bonus test; the
test pyramid is substantive. The two new phase-34 Minors (M34-1 overwrite-semantics test, M34-2 A2
execution-layer test) and the M34-3 cosmetics are nice-to-have polish that does not block the phase.
§7.5 (f) (REVIEW.md approved) is satisfied — the phase is ready for the state-6 close (ROADMAP row `34`
→ `done`).
