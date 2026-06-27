# Phase 43 — `43-accesslog-upstream-cluster` — REVIEW

> **Lifecycle state 5 (code-review output).** Routed via `superpowers:requesting-code-review`;
> performed by a fresh `superpowers:code-reviewer` subagent with precisely-crafted context (the
> implementation diff + SPEC + PLAN + ADR-0100 — NOT session history). Reviews the phase-43
> `%UPSTREAM_CLUSTER%` operator + the FIRST proxy access-log fixture (diff `0567220..1221471`; the 5 TDD task
> commits `e36e709`..`3d4a3ad` + the `1221471` state-4 fmt fix).

## Verdict: **APPROVE** — 0 Critical / 0 Important / 2 Minor (non-blocking observations; no new carry-forwards)

The implementation is a faithful, exact mirror of the `%UPSTREAM_HOST%` precedent; the key HCM design call
("set at the proxy-arm entry, NOT on upstream success") is implemented correctly + deliberately + purely
additively; the "ZERO new harness code" claim is verified true; and all doctrine constraints hold. All 11 new
tests pass locally; the full differential + the first proxy fixture `0051` already passed on the AUTHORITATIVE
CI run `28302749216` @ `1221471` (`completed/success`).

## Verification (all UPHELD)
- **Operator is an exact `%UPSTREAM_HOST%` mirror** (line-for-line across all four sites): `Op::UpstreamCluster`
  (`command_operator.rs:65`); the `"UPSTREAM_CLUSTER"` no-arg keyword (`:255` list + `:273` dispatch — rejects
  both `(...)` and `:N`, the §6.2-locked strict no-arg grammar); the `render_op` arm `…unwrap_or(empty_or_dash)`
  (`:538`); the `encode_single_op` arm `quote_opt(out, r.upstream_cluster.as_deref())` (`json_format.rs:246`).
  Text present→string / absent→`-` / mixed→`-` sentinel; json present→quoted / absent→`null`.
- **The HCM "set-at-arm-entry-not-on-success" call — correct + deliberate + purely additive (the load-bearing
  design point):** H1 sets `upstream_cluster_for_log = Some(cluster_name.clone())` as the FIRST statement of the
  `BuildOutcome::Proxy { cluster: cluster_name, … }` arm (`hcm.rs:880`), BEFORE the endpoint pick (`:994`) + the
  attempt loop; the record build at `:1232` is unconditional, so a connect/reset failure still renders the
  cluster name — the deliberate improvement over phase-42's success-gated `via_upstream` (the M42-1 gap). H2
  mirrors it at the H2 proxy arm entry (`envoy-http2/src/hcm.rs:567`) threaded through `finalize_h2_stream` (a
  new `upstream_cluster_for_log_h2` param after `response_code_details_for_log_h2`; record build `:958`). The
  value is READ only by the new operator — NO effect on response/routing, so `0001`-`0050` stay byte-identical
  (CI-proven).
- **Fixture `0051` — the FIRST proxy access-log fixture, ZERO new harness code:** `git diff … --
  tests/differential/src/` is EMPTY — the existing `Driver::Http1WithAccessLog` + the marker-driven
  `Http1EchoBackend` auto-spawn (gated on the `0008`-style `{{BACKEND_HOST}}`/`{{HTTP1_BACKEND_PORT}}` STRICT_DNS
  markers) are reused as-is (the cheap seam from the PLAN's T5 plan-review finding). The byte-exact line
  `{"method":"GET","mixed":"c=backend","proto":"HTTP/1.1","rcd":"via_upstream","uc":"backend"}` asserts only the
  deterministic tokens; `%UPSTREAM_HOST%` is correctly EXCLUDED (the per-side ip:port structural mismatch). The
  per-side `envoy.yaml`/`envoy-rust.yaml` deltas are intentional + minimal, modeled on `0008`/`0050`.
- **Doctrine clean:** `#![forbid(unsafe_code)]` intact (3 crates); no new crate/dependency; no `Cargo.toml`/
  `Cargo.lock` change; no new `ConfigError` variant; ONE new `AccessLogRecord` field; ONE new `Op` variant; the
  fuzz seed `upstream_cluster.yaml` git-tracked (`!`-un-ignore) + a parseable standalone bootstrap; the
  BEHAVIOR_CONTRACT row accurate (documents the on-failure-still-renders behavior + the M42-1 framing).
- **Tests comprehensive:** operator parse/text/json (present/absent/mixed); HCM REAL routed tests (H1 + H2 via
  live in-process upstreams) for BOTH `Some(cluster)` AND `None` (direct_response).
- **M42-1 honestly ADVANCED, not closed:** `0051` witnesses `rcd:"via_upstream"` on a real upstream-success
  path; the failure-path detail vocabulary still needs failure-injection fixtures (M42-1 stays open).

## Findings

**Critical:** none.  **Important:** none.

**Minor (observations — NOT new carry-forwards, no change required):**
- **The byte-exact assertion is cross-proxy-equality-based by design.** `expectations.yaml` carries no literal
  `expected_line` field — the `http1_access_log_byte_exact` driver byte-compares the two sides' log files
  directly (the canonical expected JSON lives in comments). This matches the prior access-log fixtures' driver
  contract; cross-proxy equality + the deterministic-probe construction enforce correctness. Intended design.
- **Style artifact:** the H1 `Op::UpstreamCluster` `render_op` arm uses a block body (`{ out.push_str(…) }`)
  while the sibling `Op::UpstreamHost`/`Op::RouteName` arms are single-expression — a pure rustfmt
  line-length artifact (the `1221471` fmt fix), behavior identical. No action.

## Strengths
- Textbook precedent-mirroring — the operator is a line-for-line clone of `Op::UpstreamHost`, minimizing
  review surface + regression risk.
- The "set-at-arm-entry-not-on-success" call is documented at every layer (code comment, test doc-comment,
  BEHAVIOR_CONTRACT, ADR/PLAN) with the explicit M42-1-gap rationale — a decision that would silently rot is
  well-anchored here.
- Real routed tests with live in-process upstreams (not mocked record-builds) for both H1 + H2, both branches.
- The "cheapest seam" restraint (reusing the marker-driven backend auto-spawn → ZERO harness code) was
  honored, exactly as the PLAN's T5 plan-review finding called for.

---

_Reviewed at state-5. **APPROVE** (0 Critical / 0 Important / 2 Minor observations, non-blocking; NO new
carry-forwards). The §7.5 (a)-(e) gate was GREEN at state-4 (AUTHORITATIVE CI `28302749216` @ `1221471`
`completed/success`, incl. the first proxy access-log fixture `0051`). With (f) `REVIEW.md` APPROVE, the full
§7.5 (a)-(f) gate is COMPLETE → the next session is the state-6 phase-close (flip ROADMAP row `43` → `done`,
advance STATE to awaiting-next-planning)._
