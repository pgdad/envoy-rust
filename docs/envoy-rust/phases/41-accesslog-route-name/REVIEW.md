# Phase 41 — `41-accesslog-route-name` — REVIEW

> **Lifecycle state 5 (code-review output).** Routed via `superpowers:requesting-code-review`;
> performed by a fresh `superpowers:code-reviewer` subagent with precisely-crafted context (the
> implementation diff + SPEC + PLAN + ADR-0098 §A–§C — NOT session history). Reviews the phase-41
> `%ROUTE_NAME%` implementation (commit range `1f344b5`..`717edd3`, diff `ec509ef..717edd3`).

## Verdict: **APPROVE** — 0 Critical / 0 Important / 2 Minor (confirmations only; no new blockers)

The implementation faithfully realizes ADR-0098 §C and the PLAN: `%ROUTE_NAME%` is an exact mirror of the
existing `%UPSTREAM_HOST%` pattern — a new `Option<String>` record field, a no-arg `Op` variant with
`unwrap_or("-")` text render + `quote_opt` JSON render, and HCM plumbing that sets `Some(name)` for a named
route and `None` for an unnamed/unmatched one. The reviewer ran `cargo test -p envoy-accesslog -p
envoy-config -p envoy-http1 -p envoy-http2` (all pass) and verified every flagged risk area; the full suite
passed on CI (run `28299106385` @ `2c8b04a`).

## Verification (all UPHELD)
- **The hand-rolled `Route` serde — wired correctly + completely** (`bootstrap.rs`): `name` added to the
  struct (`:1755`), the visitor `match key` arm with a `duplicate_field` guard (`:2049-2054`), the
  `unknown_field` allow-list (`:2088` — so a route `name` key parses instead of boot-fataling), the
  constructor `name.unwrap_or_default()` (`:2118`), and the `Serialize` (`:2136-2142`). Round-trip test
  `route_parses_name_when_present_and_defaults_empty_when_absent` covers named + name-less.
- **Byte-preservation airtight:** `name` is emitted in `Serialize` ONLY when non-empty (ordered first, the
  `len` count conditionally incremented), so every empty-name route serializes byte-identically. No fixture
  `0001`-`0048` uses `%ROUTE_NAME%` (only `0049`); the one config_dump fixture with routes (`0014`) has an
  unnamed route + a `json_shape` (not byte-exact) probe. `record.route_name` defaults `None`; `Op::RouteName`
  is unused by existing fixtures.
- **§C correctness exact:** `render_op` `record.route_name.as_deref().unwrap_or(empty_or_dash)`
  (`command_operator.rs:517`) + `encode_single_op` `quote_opt(out, r.route_name.as_deref())`
  (`json_format.rs:246`) — identical to `Op::UpstreamHost`. The no-arg keyword rejects `(...)` via the shared
  no-arg arm (`:240`), tested by `route_name_rejects_paren_argument`.
- **H2 plumbing per the PLAN:** `route_name_for_log_h2` computed at the `:470` matched-route site, threaded as
  a new `finalize_h2_stream` parameter (`:838`), assigned at the record build (`:930`) — mirroring
  `upstream_host_for_log_h2`. H1 sets it directly (`hcm.rs:1212-1216`). Both `.filter(|n| !n.is_empty())
  .map(str::to_owned)` → named `Some(name)`, unnamed `None`. New integration tests
  `hcm_h1_sets_route_name_from_matched_route` / `hcm_h2_…` cover both paths.
- **Doctrine clean:** `#![forbid(unsafe_code)]` in all four touched crates; no Cargo.toml/Cargo.lock change;
  no new fuzz target (the `route_name.yaml` seed is git-tracked with its `!`-un-ignore line); no new
  `ConfigError` variant; exactly ONE new record field.
- **Fixture 0049** asserts the byte-exact line `{"method":"GET","proto":"HTTP/1.1","rn":"r=myroute",
  "single_rn":"myroute"}` (both the mixed `rn` and single-op `single_rn` renderings), with documented
  live-capture provenance from v1.33.0.

## Findings

**Critical:** none.  **Important:** none.

**Minor (confirmations — NOT new carry-forwards):**
- The pre-existing **M39-1** (mirror-enum sync) Minor remains live (as ADR-0098 flagged); nothing in this diff
  regresses it — `render_op`/`encode_single_op` both gained explicit `Op::RouteName` arms (no wildcard
  fallthrough; compiler-enforced completeness).
- The access-log integration tests use fixed `sleep` for log-flush timing (H1 50ms / H2 200ms), matching the
  existing test idiom in these files; CI is green. A latent flake source under heavy parallel load (consistent
  with the known "differential fixtures flake under parallel load" note). No change required.

## Strengths
- The highest-risk part (the hand-rolled `Route` (de)serializer + the byte-stable config-dump serialize) was
  handled exactly right — `name` emitted only when non-empty keeps `/config_dump` byte-identical for every
  existing route.
- `%ROUTE_NAME%` is a clean, minimal mirror of `%UPSTREAM_HOST%`; every ADR-0098 §C behavior has a dedicated
  test (parse-rejects-paren, text named/unnamed, json single-op quoted/`null`, mixed `-` sentinel, H1/H2 set).
- The §6.2 pivot (from the VOID `%REQ_WITHOUT_QUERY%`) is fully realized; ADR-0098 §A–§C faithfully reflected
  in code + BEHAVIOR_CONTRACT.

---

_Reviewed at state-5. **APPROVE** (0 Critical / 0 Important / 2 Minor confirmations, non-blocking). The §7.5
(a)-(e) gate was GREEN at state-4 (authoritative CI `28299106385` @ `2c8b04a` `completed/success`). With (f)
`REVIEW.md` APPROVE, the full §7.5 (a)-(f) gate is COMPLETE → the next session is the state-6 phase-close
(flip ROADMAP row `41` → `done`, advance STATE to awaiting-next-planning)._
