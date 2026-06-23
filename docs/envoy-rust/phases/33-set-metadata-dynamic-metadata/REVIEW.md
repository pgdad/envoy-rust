# Phase 33 — `33-set-metadata-dynamic-metadata` — REVIEW (state-5)

> **Lifecycle state 5 (code-review).** Produced by `superpowers:requesting-code-review`
> (a fresh `superpowers:code-reviewer` subagent given crafted context, NOT this session's
> history). Reviews the phase-33 diff `0fde584..ca88f83` (9 task commits `433ab4f`…`4a59579`
> + the state-4 clippy fix `883cf8a`; docs commits `dd9d80d`/`ca88f83` skimmed) against the
> **PLAN.md `## §A — Empirically-locked facts` (A1–A6, ADR-0081)** — the authoritative
> §6.2 live-Envoy contract — and SPEC.md.

## Verdict

**APPROVE-WITH-MINORS** — Ready to merge: **Yes**.

The implementation is a faithful, well-tested realization of the §A-locked contract — every
boot-fatal rejection, the case-sensitive raw-unquoted render, the absent-`-` sentinel, the
dual H1+H2 capture-before-drop, and the present/absent anti-echo fixture are correct and
exercised by real (non-mock) tests. **No Critical and no Important issues.** The only findings
are three Minors (one cosmetic `.clone()`, doc-pointer line drift, and a noted-only
first-`)` parse that is correct for the contract), none blocking — all carried forward.

## Strengths

- **§A2 parser strictness is exactly right.** `parse_dynamic_metadata_op`
  (`crates/envoy-accesslog/src/command_operator.rs`) rejects every config-fatal form the
  contract demands: no-arg, trailing `:N` suffix (`!after.is_empty()`), and 1-segment /
  3+-segment args via the exact-two-non-empty match
  `(Some(ns), Some(k), None) if !ns.is_empty() && !k.is_empty()`. Each returns
  `MalformedArgument { keyword: "DYNAMIC_METADATA", .. }`, matching the boot-fatal disposition.
- **Case-sensitivity correctly preserved.** Unlike `parse_header_op` (which lowercases),
  `parse_dynamic_metadata_op` stores namespace/key verbatim; `dynamic_metadata_is_case_sensitive`
  proves `tier` ≠ `Tier`, per §A2.
- **§A3/§A4 render byte-correct.** `render_op` for `DynamicMetadata` does
  `get(ns).and_then(|m| m.get(key)).map(String::as_str).unwrap_or("-")` — raw unquoted string
  push (no quoting), absent → single `-`. Matches §A3 (raw `prod`) + §A4 (absent → `-`).
- **The merge honors `allow_overwrite` per-key.** `SetMetadataFilter::decode_headers`
  (`crates/envoy-filter/src/set_metadata.rs`) writes `if entry.allow_overwrite || !ns.contains_key(k)`
  — keep-existing when false, overwrite when true, per-namespace/per-key. Continue-only;
  `encode_headers` genuinely inert. The four filter tests exercise real logic.
- **The capture-before-drop is sound on both paths.** H1 (`hcm.rs:792`) and H2 (`hcm.rs:494`)
  both move the last remaining field AFTER the four `method/path/headers/body` write-backs —
  partial-move-correct, no field lost. H2 threads it as a NEW parameter through
  `finalize_h2_stream` rather than inheriting from H1, exactly as the C-1 correction requires.
- **The backstops are real, not mocks.** Both `h1_dynamic_metadata_threads_into_access_log`
  and `h2_dynamic_metadata_threads_into_access_log` drive an actual request through the full
  HCM, open a real `FileSink`, and `read_to_string` the scraped line, asserting `"prod / -\n"`.
  The H2 backstop spins up a real `h2::client::handshake` — the genuine sole proof of the H2
  path (fixture 0041 is H1-only).
- **Fixture 0041 carries the anti-echo guard.** `expectations.yaml` renders
  `tier=prod missk=- missns=-` with both an absent-KEY (`envoy.test:missing`) and
  absent-NAMESPACE (`envoy.absent:k`) probe in the SAME line — a hardcoded-echo implementation
  would render the config literal on the absent probes and fail. Two probes (GET / POST+body)
  prove request-independence.
- **Regression safety demonstrated, not asserted.** The store is additive default-empty on both
  structs; the M32-4 oracle (`default_format.rs`) loops the engine≡legacy equivalence over 3
  records (baseline, 5xx-proxy, UTF-8) — proving the new operator did not perturb the default
  format (the `0012` witness). `#![forbid(unsafe_code)]` holds in all three touched leaf crates.
- **The two SPEC→PLAN overrides correctly implemented:** `@type …v3.Config` (`bootstrap.rs`),
  and NO `truncate` field on `Op::DynamicMetadata`, with documentation explaining why.

## Issues

### Critical (Must Fix)
None. No §A-contract violation, data-loss, or broken-functionality defect found.

### Important (Should Fix)
None. Error handling (config-fatal via typed `ConfigError`/`FormatParseError`), separation of
concerns (no leaf-crate cycle; HCMs are the sole copy site), and test coverage
(absent / overwrite / multi-namespace / case-sensitivity / `:N`-rejection / no-arg /
segment-count all covered) are complete.

### Minor (Nice to Have) — carried forward as phase-33 Minors

- **M33-1 — unnecessary `.clone()` at `crates/envoy-http1/src/hcm.rs:1211`.** The
  `dynamic_metadata` local is captured fresh per keep-alive iteration (line 792) and used
  exactly once, at the record build (line 1211, its last use in the iteration). The PLAN says
  "prefer move if single-use," and the H2 path correctly moves (no clone). Per-request this
  clones an almost-always-empty `BTreeMap` (cheap) → purely cosmetic. Fix: drop `.clone()`,
  move (`dynamic_metadata,`). **Fold WHEN the H1 HCM record-build site is next touched.**
- **M33-2 — doc-pointer line drift.** Doc comments in `command_operator.rs` / `record.rs`
  hardcode record-build line numbers ("H1 hcm.rs ~1189, H2 hcm.rs ~888"); the actual sites are
  now ~1211 / `finalize_h2_stream`. These will rot. Prefer referencing the function name
  (`finalize_h2_stream`) over a line number. **Doc-only; fold on a future hcm.rs touch.**
- **M33-3 (noted-only, not a defect) — `parse_dynamic_metadata_op` matches the FIRST `)`.**
  A namespace/key containing a literal `)` would be mis-parsed. NOT reachable for the
  string-only single-level MVP (Envoy's own grammar has the same first-`)` behavior), so it is
  CORRECT for the contract — recorded only as a watch-item for the future nested-path deferral
  (§2.2). No action this phase.

## Recommendations

- Land as-is. M33-1 and M33-2 join the project's M-carry-forward bucket (the same mechanism
  this phase used to consume M32-1..6), to be folded by the future phase that next touches the
  H1 HCM record-build / `hcm.rs` surface.
- No new fuzz target was added (correct per §A §3.8); the two new seeds for the existing
  targets are present and require no `ci.yml` change (consistent with memory
  `new-fuzz-target-needs-a-ci-yml-step`).

## Assessment

**Ready to merge?** Yes — **APPROVE-WITH-MINORS**.

The §7.5 gate is GREEN (state-4, quoted in PROGRESS.md `## State-4 verification`): build /
clippy / fmt / `test --workspace --exclude differential` (1396 passed / 0 failed) / deny all
exit 0; fixture `0041` byte-identical; both fuzz targets 0 crashes; the host-sensitive
differential fixtures + the h2spec `3.5/2` known-failure are GREEN on CI (run `27985371447`).
The three Minors are non-blocking carry-forwards. Phase 33 is ready for the state-6 close-out.
