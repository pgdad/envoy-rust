# Sub-phase 76.1 — `Route.redirect` config surface — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement
> this plan task-by-task. (`superpowers:subagent-driven-development` is NOT appropriate here —
> see "Execution stance" below: seven of the eight tasks edit the same two files.) Steps use
> checkbox (`- [ ]`) syntax for tracking. TDD per `superpowers:test-driven-development` on
> EVERY task — tests first, no exceptions (doctrine D-3.1).

**Goal:** Land the `envoy.config.route.v3.RedirectAction` **config surface** in
`crates/envoy-config` — the schema, the five-value `RedirectResponseCode` enum, the third
`RouteAction` variant, the widened three-way `Route` action-cardinality check, the two
intra-`RedirectAction` oneof validators, both `Serialize` arms, and a `parse_bootstrap` fuzz
corpus seed — so that envoy-rust **accepts and rejects exactly the `redirect:` configs upstream
Envoy v1.33.0 accepts and rejects**. NO runtime behaviour and NO new differential fixture.

**Architecture:** `RedirectAction` is a plain derived-serde struct whose four oneof-member fields
are `Option<_>` because upstream's oneofs are exclusive on **field presence, not on value**. The
third `RouteAction` variant is threaded through the existing hand-written `Route` visitor (a new
accumulator, a new key arm, a widened unknown-field list, and a three-way cardinality check) and
through the two separate `Serialize` impls. The two oneof exclusivity rules are enforced in the
bootstrap validator as boot-fatal `ConfigError`s. The runtime dispatch seam gets an **honest
`synth_501` not-implemented placeholder**, pinned by its own test, which sub-phase `76.2`
deliberately flips.

**Tech Stack:** Rust (edition 2024, pinned via `rust-toolchain.toml`), `serde` + `serde_yaml`
derive, `thiserror` for `ConfigError`, `cargo fuzz` (existing `parse_bootstrap` target only).

**Source of truth:** `docs/envoy-rust/phases/76.1-redirect-config-surface/SPEC.md`. Read it
before starting. Every upstream behavioural claim in this plan was MEASURED against
`envoyproxy/envoy:v1.33.0` (`docs/envoy-rust/ENVOY_TARGET.md`) — see SPEC §3.

---

## Global Constraints

Every task's requirements implicitly include this section.

- **Toolchain / doctrine:** `#![forbid(unsafe_code)]` stays at every crate root (D-3.8). Do not
  bump `rust-toolchain.toml` (D-3.9) or `ENVOY_TARGET.md` (D-3.7). No new crate, no new
  dependency, no new fuzz target, **no `.github/workflows/ci.yml` edit**.
- **Files you may touch — exactly these five, and nothing else:**
  `crates/envoy-config/src/bootstrap.rs`, `crates/envoy-config/src/lib.rs`,
  `crates/envoy-http1/src/hcm.rs`, `crates/envoy-config/fuzz/.gitignore`,
  `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml` (new).
  **No `tests/` file. No fixture. No `BEHAVIOR_CONTRACT.md` line** (that is `76.2`).
- **`port_redirect` gets NO range bound.** MEASURED: upstream accepts `0` **and** `70000`; there
  is no PGV bound at all. Adding `1..=65535` would manufacture a reject-direction divergence.
- **The `Option`s are load-bearing.** `https_redirect` MUST be `Option<bool>`;
  `path_redirect` / `prefix_rewrite` / `scheme_redirect` MUST be `Option<String>`. A
  `#[serde(default)] pub https_redirect: bool` loses the presence bit and would silently ACCEPT a
  config upstream REJECTS. This is the single thing a from-scratch implementation gets wrong.
- **Error TEXT is not part of the equivalence contract.** Match the accept/reject VERDICT only.
  New messages follow house `ConfigError` style.
- **Never weaken an existing test or fixture. Never trim `tests/conformance/h2spec/known-failures.txt`** (21 lines —
  this development host scores h2spec 3.5/2 as PASS where CI does not, so trimming on local
  evidence breaks CI).
- **Do not fix any open carry-forward** (§6.3): `CF-76-1` (upstream strips the query before route
  path matching), `CF-75-2`, `CF-75-3`, `CF-75-4`, `CF-75-5`, `CF-75-6`. This sub-phase consumes
  none of them.
- **Do not touch the 75.1/75.2 `HeaderMatcher` engine** (`crates/envoy-config/src/matcher.rs`).
- **Commit after every task.** A task is not complete until its tests pass and it is committed.
- **`cargo fmt --all` after every code edit** — see the FMT REFLOW warning in Task 3.

---

## What this session (the state-2 PLAN-write) verified before writing this plan

Recorded so the executing session does not have to re-do it, and so a reviewer can audit it.

### Every SPEC citation was re-verified on disk by TEXT, not by number

| SPEC claim | Verdict | Actual |
|---|---|---|
| `bootstrap.rs` is "~14 400 lines" | **FALSE** | **20 397 lines.** The SPEC §0 and the session handoff both understate it by ~6 000. Harmless (it only motivates re-anchoring) but do not propagate the figure. |
| `bootstrap.rs:2178` — `pub enum RouteAction`, 2 variants, `#[derive(Debug, Clone, PartialEq)]` at `:2177` | TRUE | `:2177-2184`. No `#[serde]` attr; no `Serialize`/`Deserialize` in the derive (both are hand-written). |
| `:2416-2527` — hand-written `impl<'de> Deserialize<'de> for Route` | TRUE | exact. Visitor `V` at `:2424`; `deserialize_map(V)` at `:2525`. |
| `:2428-2432` — `expecting` string | TRUE | string is on ONE line, `:2430`. |
| `:2436-2444` — visitor accumulators | **PARTIAL** | actually `:2438-2444`; `:2434-2437` are the `visit_map` signature/`where`/brace. |
| `:2447-2482` — key match arms | TRUE | `while` at `:2446`, `match key.as_str()` at `:2447`, five arms `:2448-2482`. |
| `:2483-2494` — `other =>` arm; key list at `:2486-2492` | TRUE | list is rustfmt-vertical (one name per line), not a single-line `&[...]`. |
| `:2499-2514` — cardinality uses `M::Error::custom`, NOT `ConfigError`; two verbatim messages | **TRUE — the SPEC's correction of the parent SPEC is itself correct** | `:2498-2514`. Literals use `\`-newline continuation split across `:2502/:2503` and `:2508/:2509` with 29 leading spaces. |
| `:2529-2552` — `impl Serialize for Route`, arms `:2544/:2545`, `len` base 2 at `:2535` | TRUE | `len` = `2 + usize::from(!name.is_empty()) + usize::from(!typed_per_filter_config.is_empty())`. A third action variant does NOT change it. |
| `:2554-2570` — a SEPARATE `impl Serialize for RouteAction`, arms `:2565/:2566` | **TRUE — genuinely two impls** | `Route::serialize` does NOT delegate. Both need a new arm. |
| `:2192-2199` — `RouteAction_Route` `Option`+`skip_serializing_if` idiom | **PARTIAL** | struct is `:2190-2211` with FOUR fields; the cited idiom + `retry_policy` is at `:2196-2197`. |
| `:2583-2588` — `DirectResponse` minimal template | TRUE | exact. |
| `:3975-3996` — per-route loop, non-exhaustive `match &r.action` at `:3981` | **PARTIAL** | loop is `:3975-4058`; the `match` is `:3981-4019`. Enclosing fn is `validate_hcm(...) -> Result<(), crate::ConfigError>` at `:3892-3898`. **There is a SECOND action dispatch at `:4053` (`if let RouteAction::Route(..)`) which does NOT break on a new variant** — leave it alone. |
| `lib.rs:73/74/991` — `ConfigError` derive/enum/close | TRUE | exact. |
| `ConfigError` has EXACTLY 123 variants | TRUE | 123 by `#[error(` count AND 123 by variant-identifier count, independently. This plan adds 2 → **125**. |
| `hcm.rs:2110` — non-exhaustive `match &route.action` | TRUE | `:2109-2136`, inside `pub(crate) fn build_response_in(...) -> BuildOutcome` at `:2051-2055`. Local param is named `close`. |
| `hcm.rs:2112` — bare `"direct_response"` detail literal | TRUE | exact; a bare `&'static str`, no named const. |
| `synth_501` + the idiom `BuildOutcome::Synth(synth_501(close), None)` | TRUE | `pub(crate) fn synth_501(close: bool) -> Response` at `hcm.rs:2346-2348`. Idiom verbatim at `hcm.rs:915` and `uring.rs:285`. |
| `hcm.rs:2185-2204` — `synth_with` always emits `content-type` | TRUE | private `fn`; `content-type` unconditional at `:2193-2196`. (Relevant to `76.2`, not here.) |
| `response.rs:188-215` — `canonical_reason` missing 303/307/308 | TRUE | fn `:188-215`; 301 at `:195`, 302 at `:196`; 303/307/308 absent → `_ => "OK"` at `:213`. (`76.2`'s problem.) |
| `fuzz/.gitignore` = 66 lines, 63 `!` lines at `:2-64` | TRUE | exact; insertion point is after `:64`, before `artifacts/`. Tracked seeds = **63**. |
| `parse_bootstrap` fuzz target input shape | TRUE | `crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs` filters bytes to UTF-8 then calls `envoy_config::parse_bootstrap(&str)` (`lib.rs:993`) — a seed is a plain UTF-8 YAML bootstrap document. |
| `HEADER_ALLOW_LIST` = 3 entries, `location` NOT in it | TRUE | `tests/differential/src/lib.rs:1173-1181`. (Relevant to `76.2`.) |
| 85 fixture dirs / 85 differential test files; no fixture uses `redirect:` | TRUE | 85/85; highest is `0085-…`. |
| the redirect vocabulary is greenfield in `crates/` | TRUE | `git ls-files 'crates/**/*.rs' | xargs grep -lni …` over all ten identifiers returns **ZERO** files. |

### TWO EDIT SITES THE SPEC's §4.4 ENUMERATION **MISSES** — both found by compiling the plan's own code

1. **`crates/envoy-config/src/lib.rs:14` is an EXPLICIT, alphabetically-sorted
   `pub use bootstrap::{…}` re-export list — NOT a glob.** `RedirectAction` and
   `RedirectResponseCode` must be added to it or `crates/envoy-http1` cannot name the type and
   T-C9 fails with `error[E0432]: unresolved import`. The SPEC's §4.4 "five edit sites, all in
   `bootstrap.rs`, plus one inert arm outside the crate" does not mention this. It is Task 3.
2. **`crates/envoy-http1/src/hcm.rs:2361`'s test-module import must be widened.** It currently
   reads `use envoy_config::{DataSource, HashPolicyHeader, LbMetadata, RouteAction_Route, RouteMatch};`
   and needs `RedirectAction` added, or T-C9 fails with `error[E0422]: cannot find struct … RedirectAction`.

### The plan's literal Rust was PRE-FLIGHTED, not merely written

Every code block below was applied to a scratch `git worktree` detached at `533dceb`, then gated.
Results (full output in this session's scratch, commands reproduced in Task 8):

- `cargo fmt --all -- --check` → **exit 0, zero diff.**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → **exit 0, zero
  warnings**, 155 `Checking` lines on a forced re-run (real work, not cached).
- `cargo build -p envoy-config -p envoy-http1` → exit 0, 71 `Compiling` lines.
- `cargo test -p envoy-config --lib` → **653 passed, 0 failed.**
- `cargo test -p envoy-http1 --lib` → **187 passed, 0 failed.**
- **All 21 MEASURED cells produce the CORRECT VERDICT** — J1-J7 + T-R8/T-R9/T-R10 all REJECT;
  A1-A4/A6 + `https_redirect: false` alone + all five `response_code` names all ACCEPT;
  `port_redirect: 70000` round-trips verbatim as `70000`.
- The three pre-existing tests most at risk from the widened messages
  (`rejects_route_with_both_direct_response_and_route`,
  `rejects_route_with_neither_direct_response_nor_route`,
  `rejects_route_with_unknown_top_level_key`) all **still pass** — they assert substring
  containment of `direct_response`, `route` and `exactly one`, all of which the three-way
  messages preserve.
- The fuzz seed parses cleanly and `git status` reports it `A` (tracked) once the `!` line lands.
- Clippy emitted **13** `Checking` lines when only `envoy-config` was touched — the ADR-0150
  `envoy-accesslog` seam witness figure. No 14th line: the seam holds.

**One genuine `fmt` finding, and it WILL bite an implementer who hand-wraps** — see Task 3 step 4.

### Pre-existing test helpers this plan reuses — ALL VERIFIED TO EXIST

Do not invent helpers; these are real, at these anchors:

| helper | location | signature |
|---|---|---|
| `route_action_yaml` | `bootstrap.rs:10227` | `fn route_action_yaml(routes: &str, clusters: &str) -> String` |
| `BACKEND_CLUSTER` | `bootstrap.rs:10258` | `const BACKEND_CLUSTER: &str` |
| `NO_CLUSTERS` | `bootstrap.rs:10268` | `const NO_CLUSTERS: &str = " []";` |
| `first_route_action` | `bootstrap.rs:10273` | `fn first_route_action(b: &Bootstrap) -> &RouteAction` |
| `make_req` | `hcm.rs:9624` | `fn make_req(path: &str, host: &str) -> Request` |
| `cluster_mgr_empty` / `mk_stats` / `test_router_only_pipeline` | `hcm.rs` test module | used verbatim by the Task 3 config helper |

---

## Size re-derivation and the §6.1 split decision — RE-OWNED

`SPEC.md` §8 projected **≈579** net LoC / ≈10-12 tasks and required this session to re-derive it.

**The implementation half is not projected — it is MEASURED**, by applying it and counting:

| component | SPEC §8 est. | **MEASURED** |
|---|---|---|
| `RedirectResponseCode` + `status()` | 28 | included below |
| `RedirectAction` struct + serde + docs | 50 | included below |
| `RouteAction::Redirect`, accumulator, key arm, key list | 16 | included below |
| three-way cardinality + `expecting` | 30 | included below |
| two `Serialize` arms | 4 | included below |
| validator arm (both oneof checks) | 30 | included below |
| — **all `bootstrap.rs` implementation, measured** | (158) | **net 101** (`+111 −10`) |
| two `ConfigError` variants + the re-export (incl. rustfmt reflow) | 18 | **net 20** |
| inert `synth_501` placeholder arm at the H1 dispatch | 12 | **net 8** |
| **implementation subtotal** | **188** | **net 129** |
| in-process tests T-R1..T-R10 / T-A1..T-A7 / T-C1..T-C9 (26 tests, house style: individually named, one cell each) | 360 | **≈348 projected** (the compressed loop-driven pre-flight equivalent measured 191; house-style individual tests are more verbose, and T-C9 + its config helper measured 71 on its own) |
| fuzz seed YAML + `!`-un-ignore line | 31 | **38 measured** (37 + 1) |
| **total** | **≈579** | **≈515** |

**Calibration.** The §6.1 decision is anchored to measured history, not to feel. This session
re-derived three of the seven inherited phase figures independently, from
`git diff --numstat <state-2-PLAN-write>..<close-out> -- crates/ tests/`:

| phase | inherited | **re-measured this session** |
|---|---|---|
| 74 | 1981 | **1981** ✓ |
| 75.1 | 1413 | **1413** ✓ |
| 75.2 | 897 | **897** ✓ |

Three exact reproductions validate both the method and the inherited band (68→950, 69→1540,
70→1372, 73→873). **≈515 sits below every one of them** — it is the smallest phase in the band.

**DECISION: the §6.1 gate does NOT fire. No further split.** ≈515 net LoC against the ~1500
threshold (34% of it) and **8 numbered tasks** against the ~25 threshold. Neither axis is close.
`ADR-0170` is therefore NOT consumed and remains the next free ADR number. **No new ADR is
required by this plan** — the split is already settled by `ADR-0169`, and every design choice
here is either measured upstream behaviour or an explicit `ADR-0169` DECISION (notably DECISION 4,
the honest placeholder).

### Execution stance

**Run the tasks SERIALLY in the main session** (`superpowers:executing-plans`), NOT via
`superpowers:subagent-driven-development`. Seven of the eight tasks edit
`crates/envoy-config/src/bootstrap.rs`, and Tasks 3-6 form a strict compile-order chain — the
enum variant does not compile until all four `match` sites are handled. Parallel worktrees would
collide on the same file and the `cargo` lock serialises the test runs anyway, so a fan-out costs
coordination and buys nothing.

---

## File Structure

| file | change | responsibility |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` (20 397 lines) | modify | `RedirectResponseCode`, `RedirectAction`, `RouteAction::Redirect`, the `Route` visitor, the three-way cardinality check, both `Serialize` arms, the validator arm, and all `envoy-config` tests |
| `crates/envoy-config/src/lib.rs` (1 263 lines) | modify | two new `ConfigError` variants (123 → 125); two names added to the `pub use bootstrap::{…}` re-export list |
| `crates/envoy-http1/src/hcm.rs` (10 129 lines) | modify | the honest `synth_501` placeholder dispatch arm + its T-C9 pin + the test-module import widening |
| `crates/envoy-config/fuzz/.gitignore` (66 lines) | modify | one `!`-un-ignore line so the new seed is tracked (→ 67 lines) |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml` | **create** | a `parse_bootstrap` corpus seed exercising five distinct `redirect:` shapes |

This follows the established house layout: `envoy-config` keeps its schema, its validator and its
tests in one large `bootstrap.rs` with a single `#[cfg(test)] mod tests`. **Do not split
`bootstrap.rs`** — it is 20 397 lines and restructuring it is out of scope (and would swamp the
diff this sub-phase needs to keep reviewable).

---

## Task 1: `RedirectResponseCode` — the five-value enum

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (insert immediately BEFORE the
  `#[derive(Debug, Clone, PartialEq)]` / `pub enum RouteAction {` pair at `:2177-2178`)
- Test: `crates/envoy-config/src/bootstrap.rs` (`mod tests`, next to the existing 04.3
  `RouteAction` parse-shape block near `:10270`)

**Interfaces:**
- Consumes: nothing.
- Produces: `pub enum RedirectResponseCode` with variants `MovedPermanently` (the `#[default]`),
  `Found`, `SeeOther`, `TemporaryRedirect`, `PermanentRedirect`; and
  `pub fn status(self) -> u16` on it. Wire names are the SCREAMING_SNAKE forms
  (`MOVED_PERMANENTLY`, `FOUND`, `SEE_OTHER`, `TEMPORARY_REDIRECT`, `PERMANENT_REDIRECT`) via
  `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]`. Task 2 embeds this as
  `RedirectAction::response_code`.

**Why CamelCase variants + `rename_all`, and not `#[allow(non_camel_case_types)]`:** this is the
landed house idiom for a SCREAMING_SNAKE wire enum with a proto default — see
`LbSubsetFallbackPolicy` (`bootstrap.rs:366-373`) and `HashFunction` (`:439-450`). It is
clippy-clean without an allow. Verified: serde's `SCREAMING_SNAKE_CASE` renders all five names
correctly, so — unlike `HashFunction::MurmurHash2` — **no explicit `#[serde(rename)]` is needed**.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/envoy-config/src/bootstrap.rs`:

```rust
    // --- 76.1 Task 1: RedirectResponseCode (SPEC §4.1) ---

    /// T-A7: each of the five upstream wire names parses to its variant.
    #[test]
    fn redirect_response_code_parses_all_five_wire_names() {
        let cases = [
            ("MOVED_PERMANENTLY", RedirectResponseCode::MovedPermanently),
            ("FOUND", RedirectResponseCode::Found),
            ("SEE_OTHER", RedirectResponseCode::SeeOther),
            ("TEMPORARY_REDIRECT", RedirectResponseCode::TemporaryRedirect),
            ("PERMANENT_REDIRECT", RedirectResponseCode::PermanentRedirect),
        ];
        for (wire, want) in cases {
            let got: RedirectResponseCode =
                serde_yaml::from_str(wire).unwrap_or_else(|e| panic!("{wire} must parse: {e}"));
            assert_eq!(got, want, "{wire} must map to {want:?}");
        }
    }

    /// The Envoy proto default is MOVED_PERMANENTLY (301).
    #[test]
    fn redirect_response_code_defaults_to_moved_permanently() {
        assert_eq!(
            RedirectResponseCode::default(),
            RedirectResponseCode::MovedPermanently
        );
    }

    /// The `-> u16` mapping. MEASURED against envoyproxy/envoy:v1.33.0.
    /// 76.1 only round-trips the enum; 76.2 wires this to the response.
    #[test]
    fn redirect_response_code_maps_to_status() {
        assert_eq!(RedirectResponseCode::MovedPermanently.status(), 301);
        assert_eq!(RedirectResponseCode::Found.status(), 302);
        assert_eq!(RedirectResponseCode::SeeOther.status(), 303);
        assert_eq!(RedirectResponseCode::TemporaryRedirect.status(), 307);
        assert_eq!(RedirectResponseCode::PermanentRedirect.status(), 308);
    }

    /// T-R6 (J6) at the enum level: an unknown NAME must not deserialize.
    #[test]
    fn redirect_response_code_rejects_unknown_name() {
        let err = serde_yaml::from_str::<RedirectResponseCode>("BOGUS")
            .expect_err("unknown enum name must reject");
        assert!(
            err.to_string().contains("BOGUS"),
            "error should name the offending value; got: {err}"
        );
    }

    /// T-R7 (J7) at the enum level: a NUMERIC literal must not deserialize.
    /// Upstream rejects `response_code: 302` via PGV `defined_only`; envoy-rust
    /// rejects it because a unit enum will not accept an integer.
    #[test]
    fn redirect_response_code_rejects_numeric_literal() {
        serde_yaml::from_str::<RedirectResponseCode>("302")
            .expect_err("a numeric response_code must reject");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config --lib redirect_response_code`
Expected: **FAIL to compile** — `error[E0433]: failed to resolve: use of undeclared type
'RedirectResponseCode'` (5 occurrences). This is the honest RED for greenfield code.

- [ ] **Step 3: Write the implementation**

Insert into `crates/envoy-config/src/bootstrap.rs` immediately BEFORE the
`#[derive(Debug, Clone, PartialEq)]` line that precedes `pub enum RouteAction {`:

```rust
/// 76.1 (§4.1): `RedirectAction.RedirectResponseCode` — the five wire values of
/// Envoy v1.33's `envoy.config.route.v3.RedirectAction.RedirectResponseCode`.
/// Deserialized as a plain unit enum so that an unknown NAME (`BOGUS`) and a
/// NUMERIC literal (`response_code: 302`) both fail at serde parse, matching the
/// two MEASURED upstream rejections J6 and J7. Default = `MovedPermanently`
/// (301), the Envoy proto default.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RedirectResponseCode {
    #[default]
    MovedPermanently,
    Found,
    SeeOther,
    TemporaryRedirect,
    PermanentRedirect,
}

impl RedirectResponseCode {
    /// The HTTP status code this redirect response carries on the wire.
    /// MEASURED against `envoyproxy/envoy:v1.33.0`. Consumed by 76.2 (the
    /// runtime slice); 76.1 only round-trips the enum.
    pub fn status(self) -> u16 {
        match self {
            RedirectResponseCode::MovedPermanently => 301,
            RedirectResponseCode::Found => 302,
            RedirectResponseCode::SeeOther => 303,
            RedirectResponseCode::TemporaryRedirect => 307,
            RedirectResponseCode::PermanentRedirect => 308,
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-config --lib redirect_response_code`
Expected: **PASS — 5 passed.**

- [ ] **Step 5: fmt + clippy, then commit**

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 76.1 task 1: RedirectResponseCode — the five-value wire enum + status() mapping"
```

---

## Task 2: `RedirectAction` — the struct, with presence-preserving `Option`s

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (insert directly after the Task 1 `impl` block,
  still before `pub enum RouteAction`)
- Test: `crates/envoy-config/src/bootstrap.rs` (`mod tests`)

**Interfaces:**
- Consumes: `RedirectResponseCode` (Task 1).
- Produces: `pub struct RedirectAction` with fields `host_redirect: Option<String>`,
  `port_redirect: Option<u32>`, `path_redirect: Option<String>`,
  `prefix_rewrite: Option<String>`, `https_redirect: Option<bool>`,
  `scheme_redirect: Option<String>`, `strip_query: bool`,
  `response_code: RedirectResponseCode`. Derives
  `Debug, Clone, Serialize, Deserialize, PartialEq, Default` with
  `#[serde(deny_unknown_fields)]`. Task 3 wraps it as `RouteAction::Redirect(RedirectAction)`.

These tests deserialize `RedirectAction` **directly** (`serde_yaml::from_str::<RedirectAction>`),
not through a whole bootstrap — that keeps the RED tight and the failure message local. The
end-to-end `parse_bootstrap` versions of the same cells land in Task 6.

- [ ] **Step 1: Write the failing tests**

```rust
    // --- 76.1 Task 2: RedirectAction schema (SPEC §4.2) ---

    /// T-A6: a bare `{}` parses, with the two MEASURED proto defaults.
    #[test]
    fn redirect_action_bare_map_uses_proto_defaults() {
        let rd: RedirectAction = serde_yaml::from_str("{}").expect("bare redirect parses");
        assert!(!rd.strip_query, "strip_query defaults to false");
        assert_eq!(
            rd.response_code,
            RedirectResponseCode::MovedPermanently,
            "response_code defaults to MOVED_PERMANENTLY (301)"
        );
        assert_eq!(rd.host_redirect, None);
        assert_eq!(rd.port_redirect, None);
        assert_eq!(rd.path_redirect, None);
        assert_eq!(rd.prefix_rewrite, None);
        assert_eq!(rd.https_redirect, None);
        assert_eq!(rd.scheme_redirect, None);
    }

    /// T-A1 + T-A2: `port_redirect` has NO PGV bound upstream — `0` and `70000`
    /// both ACCEPT (MEASURED), and 70000 must round-trip VERBATIM. Adding a
    /// `1..=65535` check here would manufacture a reject-direction divergence.
    #[test]
    fn redirect_action_port_redirect_has_no_range_bound() {
        let zero: RedirectAction =
            serde_yaml::from_str("port_redirect: 0").expect("port_redirect: 0 must parse");
        assert_eq!(zero.port_redirect, Some(0));
        let big: RedirectAction =
            serde_yaml::from_str("port_redirect: 70000").expect("port_redirect: 70000 must parse");
        assert_eq!(
            big.port_redirect,
            Some(70000),
            "70000 must survive verbatim — upstream renders it verbatim in `location`"
        );
    }

    /// T-A3 + T-A4: the empty string is a LEGAL value for both string members.
    #[test]
    fn redirect_action_accepts_empty_string_members() {
        let h: RedirectAction =
            serde_yaml::from_str(r#"host_redirect: """#).expect("empty host_redirect parses");
        assert_eq!(h.host_redirect.as_deref(), Some(""));
        let s: RedirectAction =
            serde_yaml::from_str(r#"scheme_redirect: """#).expect("empty scheme_redirect parses");
        assert_eq!(s.scheme_redirect.as_deref(), Some(""));
    }

    /// T-A5 — HALF ONE OF THE PRESENCE PIN. `https_redirect: false` ALONE is
    /// ACCEPTED upstream (MEASURED), and it must land as `Some(false)`, NOT as
    /// `None` and NOT as a bare `false`. If `https_redirect` were modelled
    /// `#[serde(default)] pub https_redirect: bool` this assertion could not even
    /// be written — which is exactly why the field is `Option<bool>`.
    #[test]
    fn redirect_action_https_redirect_false_is_present_not_absent() {
        let rd: RedirectAction =
            serde_yaml::from_str("https_redirect: false").expect("https_redirect: false parses");
        assert_eq!(
            rd.https_redirect,
            Some(false),
            "writing the key at all sets the oneof — `false` is PRESENT, not absent"
        );
    }

    /// The other presence case: `path_redirect: ""` is PRESENT, not absent.
    /// This is what makes T-R9 (A7) reject in Task 5.
    #[test]
    fn redirect_action_empty_path_redirect_is_present_not_absent() {
        let rd: RedirectAction =
            serde_yaml::from_str(r#"path_redirect: """#).expect("empty path_redirect parses");
        assert_eq!(rd.path_redirect.as_deref(), Some(""));
    }

    /// T-R2 (J2) mechanism: `regex_rewrite` inside `redirect` is an explicit
    /// NON-GOAL and is boot-fatal here via `deny_unknown_fields`. Upstream
    /// rejects the J2 config through its oneof instead — a DIFFERENT mechanism,
    /// the SAME verdict, which is all the equivalence contract requires.
    #[test]
    fn redirect_action_rejects_unknown_field() {
        let err = serde_yaml::from_str::<RedirectAction>("regex_rewrite: { pattern: x }")
            .expect_err("regex_rewrite must reject (deny_unknown_fields)");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field") && msg.contains("regex_rewrite"),
            "error must name the unknown field; got: {msg}"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config --lib redirect_action`
Expected: **FAIL to compile** — `error[E0412]: cannot find type 'RedirectAction' in this scope`.

- [ ] **Step 3: Write the implementation**

Insert into `crates/envoy-config/src/bootstrap.rs` directly after Task 1's `impl` block:

```rust
/// 76.1 (§4.2): `envoy.config.route.v3.RedirectAction` — the THIRD `Route.action`
/// oneof arm. 76.1 lands the CONFIG SURFACE only; no runtime behaviour (76.2).
///
/// **The `Option`s are load-bearing — do NOT "simplify" them to bare scalars.**
/// `path_redirect`/`prefix_rewrite` and `https_redirect`/`scheme_redirect` are
/// protobuf `oneof` members, and MEASURED upstream behaviour is that they are
/// exclusive on FIELD PRESENCE, not on value: `https_redirect: false` PLUS
/// `scheme_redirect: "ftp"` REJECTS, while `https_redirect: false` ALONE
/// ACCEPTS, and `path_redirect: ""` plus `prefix_rewrite: "/q"` REJECTS. A
/// `#[serde(default)] pub https_redirect: bool` loses that presence bit and
/// would silently ACCEPT a config upstream rejects.
///
/// `port_redirect` deliberately carries NO range bound: MEASURED, upstream
/// accepts both `0` and `70000` (no PGV bound at all), so adding a `1..=65535`
/// check would manufacture a reject-direction divergence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct RedirectAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_redirect: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_redirect: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_redirect: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_rewrite: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub https_redirect: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme_redirect: Option<String>,
    #[serde(default)]
    pub strip_query: bool,
    #[serde(default)]
    pub response_code: RedirectResponseCode,
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-config --lib redirect_action`
Expected: **PASS — 6 passed.**

- [ ] **Step 5: fmt + clippy, then commit**

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 76.1 task 2: RedirectAction schema — presence-preserving Options, no port bound"
```

---

## Task 3: `RouteAction::Redirect` + all four `match` sites + the re-export (the "compiles again" task)

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` — the enum at `:2178`; both `Serialize` arms
  (`:2544/:2545` and `:2565/:2566`); an inert validator arm in the `match &r.action` at `:3981`
- Modify: `crates/envoy-config/src/lib.rs` — the `pub use bootstrap::{…}` list at `:14`
- Modify: `crates/envoy-http1/src/hcm.rs` — the dispatch arm at `:2110`; the test-module import at `:2361`
- Test: `crates/envoy-http1/src/hcm.rs` (`mod tests`) — T-C9

**Interfaces:**
- Consumes: `RedirectAction` (Task 2).
- Produces: `RouteAction::Redirect(RedirectAction)`; `envoy_config::RedirectAction` and
  `envoy_config::RedirectResponseCode` importable from other crates; a
  `BuildOutcome::Synth(synth_501(close), None)` outcome for any `redirect:` route.

**This is ONE task and cannot be split.** Adding a third variant to `RouteAction` breaks
compilation at **four** non-exhaustive `match` sites simultaneously (`Serialize for Route`,
`Serialize for RouteAction`, `validate_hcm`, `build_response_in`). The workspace does not build
again until all four are handled, so a reviewer cannot meaningfully accept a subset.

**The validator arm is deliberately INERT here** (`RouteAction::Redirect(_) => {}` — no oneof
checks). Task 5 fills it in, which is what makes Task 5's tests genuinely RED.

**Leave `bootstrap.rs:4053` alone.** There is a second action dispatch,
`if let RouteAction::Route(route_action) = &r.action`, which does NOT break on a new variant and
needs no change. Hash policies belong to the `route:` arm only.

- [ ] **Step 1: Write the failing test (T-C9 — the honest-placeholder pin)**

Add to `mod tests` in `crates/envoy-http1/src/hcm.rs`, immediately before
`async fn build_response_subset_match_populated_from_metadata_match()`:

```rust
    /// 76.1 T-C9: the HONEST PLACEHOLDER pin (ADR-0169 DECISION 4).
    /// 76.1 lands the `redirect:` CONFIG SURFACE but no runtime behaviour, so the
    /// dispatch arm returns the EXISTING `synth_501` not-implemented outcome. This
    /// test exists so the placeholder is EXERCISED rather than silent, and so that
    /// 76.2 replacing it with a real 3xx + `location` is a visible, deliberate
    /// change to a named test rather than an unobserved behaviour shift.
    /// **76.2 MUST flip this test.**
    async fn redirect_placeholder_config() -> HCMConfig {
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
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::Redirect(RedirectAction {
                            https_redirect: Some(true),
                            ..Default::default()
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
        }
    }

    #[tokio::test]
    async fn build_response_redirect_is_not_implemented_placeholder() {
        let config = redirect_placeholder_config().await;
        let req = make_req("/foo", "localhost");
        match build_response(&config, &req, true) {
            BuildOutcome::Synth(resp, detail) => {
                assert_eq!(
                    resp.status, 501,
                    "76.1 ships the config surface only; the redirect runtime is 76.2, \
                     so the placeholder must be the honest 501 not-implemented synth"
                );
                assert_eq!(
                    detail, None,
                    "the placeholder must NOT claim a %RESPONSE_CODE_DETAILS% string"
                );
            }
            _other => panic!("expected BuildOutcome::Synth(501)"),
        }
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-http1 --lib redirect`
Expected: **FAIL to compile** — `error[E0422]: cannot find struct, variant or union type
'RedirectAction' in this scope` (the import is widened in Step 3c) and, once imported,
`error[E0432]: unresolved import 'envoy_config::RedirectAction'` (the re-export is added in
Step 3b). Both failures are expected and are the point of this ordering.

- [ ] **Step 3a: Add the enum variant and the two `Serialize` arms + the inert validator arm**

In `crates/envoy-config/src/bootstrap.rs`, extend the enum (`:2178`) — append after the
`Route(RouteAction_Route),` variant:

```rust
    /// Route-to-cluster action — proxy through to the named cluster. Phase 04.3 NEW.
    Route(RouteAction_Route),

    /// Redirect action — synthesize a 3xx reply carrying a `location:` header.
    /// 76.1 NEW: the config surface only. The runtime dispatch arm is an honest
    /// `synth_501` not-implemented placeholder until 76.2 lands the real
    /// behaviour (ADR-0169 DECISION 4).
    Redirect(RedirectAction),
}
```

In `impl serde::Serialize for Route` (`:2543-2546`) add the third arm:

```rust
        match &self.action {
            RouteAction::DirectResponse(dr) => map.serialize_entry("direct_response", dr)?,
            RouteAction::Route(ar) => map.serialize_entry("route", ar)?,
            RouteAction::Redirect(rd) => map.serialize_entry("redirect", rd)?,
        }
```

In the SEPARATE `impl serde::Serialize for RouteAction` (`:2564-2567`) add the same arm:

```rust
        match self {
            RouteAction::DirectResponse(dr) => map.serialize_entry("direct_response", dr)?,
            RouteAction::Route(ar) => map.serialize_entry("route", ar)?,
            RouteAction::Redirect(rd) => map.serialize_entry("redirect", rd)?,
        }
```

**Do NOT change `Route::serialize`'s `len` at `:2535`.** It is `2 + …` where the `2` covers
`match` plus exactly one action key; a third variant still emits exactly one action key.

In `validate_hcm`'s `match &r.action` (`:3981`), add an INERT arm immediately before the
`RouteAction::Route(ar) => {` arm:

```rust
                RouteAction::Redirect(_) => {
                    // 76.1 Task 3: the variant must be handled for the workspace to
                    // compile. The two intra-RedirectAction oneof checks land in
                    // Task 5 — keeping this arm inert here is what makes Task 5's
                    // reject-direction tests genuinely RED.
                }
```

- [ ] **Step 3b: Add both types to the `pub use` re-export list**

In `crates/envoy-config/src/lib.rs`, the `pub use bootstrap::{…}` block starting at `:14` is an
**explicit, alphabetically-sorted** list. Insert the two names in alphabetical position — after
`Rds`, before `RequirementRule` (`Rds` < `RedirectAction` < `RedirectResponseCode` <
`RequirementRule`). Find this line:

```rust
    PermissionSet, Policy, Principal, PrincipalSet, RbacConfig, Rds, RequirementRule,
```

and make it:

```rust
    PermissionSet, Policy, Principal, PrincipalSet, RbacConfig, Rds, RedirectAction,
    RedirectResponseCode, RequirementRule,
```

- [ ] **Step 3c: Widen the `hcm.rs` test-module import**

In `crates/envoy-http1/src/hcm.rs`, replace the import at `:2361`:

```rust
    use envoy_config::{DataSource, HashPolicyHeader, LbMetadata, RouteAction_Route, RouteMatch};
```

with:

```rust
    use envoy_config::{
        DataSource, HashPolicyHeader, LbMetadata, RedirectAction, RouteAction_Route, RouteMatch,
    };
```

- [ ] **Step 3d: Add the honest placeholder dispatch arm**

In `crates/envoy-http1/src/hcm.rs`, in `build_response_in`'s `match &route.action` (`:2110`),
insert immediately before the `RouteAction::Route(ar) => BuildOutcome::Proxy {` arm:

```rust
        // 76.1 (ADR-0169 DECISION 4): HONEST placeholder for the redirect action.
        // The config surface landed in 76.1; the runtime lands in 76.2. Returning
        // the EXISTING synth_501 not-implemented outcome makes a configured-but-
        // unserved redirect loudly wrong rather than silently wrong, which is what
        // BOOTSTRAP_PROMPT.md §6.3 requires of a placeholder. 76.2 replaces this
        // arm with the real 3xx + `location` response; the test pinning this
        // behaviour is deliberately flipped there.
        RouteAction::Redirect(_) => BuildOutcome::Synth(synth_501(close), None),
```

`synth_501` is already in scope in this module (`pub(crate) fn synth_501` at `hcm.rs:2346`) and
the local parameter is already named `close` — **no new import is needed here.**

- [ ] **Step 4: `cargo fmt --all` — and EXPECT A REFLOW YOU DID NOT WRITE**

```bash
cargo fmt --all
git diff --stat crates/envoy-config/src/lib.rs
```

Expected: **`crates/envoy-config/src/lib.rs` shows roughly `+29 −9`, not `+2 −1`.** Inserting two
names into that `use` block pushes every following name across line boundaries, so rustfmt
**reflows the whole remaining tail of the block** (about 7 lines). This is correct and expected.

**Do NOT hand-wrap the list to avoid it, and do NOT revert the reflow** — if you commit a
hand-wrapped version, `cargo fmt --all -- --check` fails in CI at the state-4 gate. This session
hit exactly this: the first pre-flight pass failed `fmt --check` with a 66-line diff, of which
this reflow was the real half. Always let `cargo fmt` own the formatting.

- [ ] **Step 5: Verify the workspace compiles and T-C9 passes**

```bash
cargo fmt --all -- --check
cargo build --workspace --all-targets
cargo test -p envoy-http1 --lib redirect
cargo test -p envoy-config --lib
```

Expected: `fmt --check` exit 0; build clean; T-C9 **1 passed**; `envoy-config` **all pre-existing
tests still pass** (they must — nothing observable changed for a config without `redirect:`).

- [ ] **Step 6: Commit**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs crates/envoy-http1/src/hcm.rs
git commit -m "phase 76.1 task 3: RouteAction::Redirect + both Serialize arms + re-export + honest synth_501 placeholder (T-C9)"
```

---

## Task 4: the `Route` visitor — `redirect:` key, six-name unknown-field list, three-way cardinality

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` — `expecting` (`:2430`), the accumulator block
  (`:2438-2444`), the key match (`:2447-2482`), the `other =>` arm (`:2483-2494`), the
  cardinality check (`:2499-2514`)
- Test: `crates/envoy-config/src/bootstrap.rs` (`mod tests`)

**Interfaces:**
- Consumes: `RouteAction::Redirect` (Task 3).
- Produces: end-to-end `parse_bootstrap` support for a `redirect:` route; the three-way
  cardinality errors that make J3/J4/T-R10 reject.

**Regression safety, already verified by this session:** the three pre-existing tests over these
strings (`rejects_route_with_both_direct_response_and_route`,
`rejects_route_with_neither_direct_response_nor_route`,
`rejects_route_with_unknown_top_level_key`) assert only substring containment of
`direct_response`, `route`, `exactly one`, `unknown` and the offending key name. The three-way
messages below preserve every one of those substrings, so all three stay green. **Do not edit
those tests.**

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/envoy-config/src/bootstrap.rs`:

```rust
    // --- 76.1 Task 4: the Route visitor's `redirect` key + three-way cardinality ---

    /// Build a bootstrap whose single route carries the given action block.
    /// Reuses the landed 04.3 helpers `route_action_yaml` / `NO_CLUSTERS`.
    fn redirect_route_yaml(action_block: &str) -> String {
        route_action_yaml(
            &format!(
                "- match: {{ prefix: \"/t\" }}\n                          {action_block}"
            ),
            NO_CLUSTERS,
        )
    }

    /// A `redirect:` route now parses end-to-end and lands as the third variant.
    #[test]
    fn parses_route_with_redirect_action() {
        let yaml = redirect_route_yaml("redirect: { host_redirect: example.com }");
        let b = crate::parse_bootstrap(&yaml).expect("parses + validates");
        match first_route_action(&b) {
            RouteAction::Redirect(rd) => {
                assert_eq!(rd.host_redirect.as_deref(), Some("example.com"));
            }
            other => panic!("expected Redirect(_), got {other:?}"),
        }
    }

    /// T-R3 (J3): `redirect` + `route` on one Route → reject.
    #[test]
    fn rejects_route_with_both_redirect_and_route() {
        let yaml = redirect_route_yaml(
            "redirect: { host_redirect: example.com }\n                          route: { cluster: backend }",
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("redirect + route must reject");
        let msg = err.to_string();
        assert!(msg.contains("exactly one"), "got: {msg}");
        assert!(msg.contains("redirect"), "got: {msg}");
    }

    /// T-R4 (J4): `redirect` + `direct_response` on one Route → reject.
    #[test]
    fn rejects_route_with_both_redirect_and_direct_response() {
        let yaml = redirect_route_yaml(
            "redirect: { host_redirect: example.com }\n                          direct_response: { status: 200, body: { inline_string: \"ok\" } }",
        );
        let err =
            crate::parse_bootstrap(&yaml).expect_err("redirect + direct_response must reject");
        let msg = err.to_string();
        assert!(msg.contains("exactly one"), "got: {msg}");
        assert!(msg.contains("redirect"), "got: {msg}");
    }

    /// T-R10: a Route with NO action at all → reject, three-way message.
    #[test]
    fn rejects_route_with_no_action_names_all_three_arms() {
        let yaml = route_action_yaml(r#"- match: { prefix: "/t" }"#, NO_CLUSTERS);
        let err = crate::parse_bootstrap(&yaml).expect_err("no action must reject");
        let msg = err.to_string();
        assert!(msg.contains("direct_response"), "got: {msg}");
        assert!(msg.contains("route"), "got: {msg}");
        assert!(
            msg.contains("redirect"),
            "the three-way message must name `redirect` too; got: {msg}"
        );
    }

    /// T-C6: an unknown Route key still rejects, and the error now names all SIX
    /// accepted keys (the five 04.x keys plus `redirect`).
    #[test]
    fn unknown_route_key_error_names_all_six_accepted_keys() {
        let yaml = redirect_route_yaml(
            "redirect: { host_redirect: example.com }\n                          bogus_key: surprise",
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("unknown Route key must reject");
        let msg = err.to_string();
        for expected in [
            "name",
            "match",
            "direct_response",
            "route",
            "redirect",
            "typed_per_filter_config",
        ] {
            assert!(
                msg.contains(expected),
                "unknown-field error must list `{expected}`; got: {msg}"
            );
        }
    }

    /// A duplicate `redirect:` key rejects, same as its four peers.
    #[test]
    fn rejects_route_with_duplicate_redirect_key() {
        let yaml = redirect_route_yaml(
            "redirect: { host_redirect: a.example }\n                          redirect: { host_redirect: b.example }",
        );
        crate::parse_bootstrap(&yaml).expect_err("duplicate redirect key must reject");
    }

    /// T-C1..T-C5: the five pre-existing Route keys still parse after the visitor
    /// was widened. `name` / `match` / `direct_response` / `typed_per_filter_config`
    /// in one route, and `route` in another (they are mutually exclusive).
    #[test]
    fn all_five_preexisting_route_keys_still_parse() {
        let dr = route_action_yaml(
            r#"- name: r1
                          match: { prefix: "/a" }
                          direct_response: { status: 204, body: { inline_string: "" } }"#,
            NO_CLUSTERS,
        );
        let b = crate::parse_bootstrap(&dr).expect("name+match+direct_response parses");
        assert!(matches!(
            first_route_action(&b),
            RouteAction::DirectResponse(_)
        ));

        let rt = route_action_yaml(
            r#"- name: r2
                          match: { path: "/b" }
                          route: { cluster: backend }"#,
            BACKEND_CLUSTER,
        );
        let b = crate::parse_bootstrap(&rt).expect("name+match+route parses");
        assert!(matches!(first_route_action(&b), RouteAction::Route(_)));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config --lib redirect_action_yaml redirect route_with_no_action six_accepted preexisting_route_keys`

Better, run the whole new set: `cargo test -p envoy-config --lib redirect`

Expected: **FAIL.** `parses_route_with_redirect_action` and the T-C6 / duplicate / cardinality
tests fail with an `unknown field 'redirect', expected one of 'name', 'match',
'direct_response', 'route', 'typed_per_filter_config'` error, because the visitor does not yet
accept the key. `rejects_route_with_no_action_names_all_three_arms` fails on the missing
`redirect` substring. `all_five_preexisting_route_keys_still_parse` **passes immediately** — it is
a characterization pin (see Step 5).

- [ ] **Step 3: Write the implementation**

**3a — widen `expecting` (`:2430`)** so it does not become a lie:

```rust
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a Route map with `match` and exactly one of `direct_response`, `route` or `redirect`",
                )
            }
```

**3b — add the accumulator** immediately after the `typed_per_filter_config` accumulator
(`:2442-2444`):

```rust
                let mut redirect: Option<RedirectAction> = None;
```

**3c — add the key arm** immediately before the `other =>` arm, in the same duplicate-checking
shape as its four peers:

```rust
                        "redirect" => {
                            if redirect.is_some() {
                                return Err(M::Error::duplicate_field("redirect"));
                            }
                            redirect = Some(map.next_value::<RedirectAction>()?);
                        }
```

**3d — widen the unknown-field list** inside the `other =>` arm (`:2486-2492`) to six names:

```rust
                        other => {
                            return Err(M::Error::unknown_field(
                                other,
                                &[
                                    "name",
                                    "match",
                                    "direct_response",
                                    "route",
                                    "redirect",
                                    "typed_per_filter_config",
                                ],
                            ));
                        }
```

**3e — replace the two-way cardinality check** (`:2499-2514`) with the three-way form. Note this
uses a catch-all `_` for "more than one is present", which keeps the arm count at five instead of
enumerating all eight tuple combinations:

```rust
                let action = match (direct_response, route_action, redirect) {
                    (Some(dr), None, None) => RouteAction::DirectResponse(dr),
                    (None, Some(ar), None) => RouteAction::Route(ar),
                    (None, None, Some(rd)) => RouteAction::Redirect(rd),
                    (None, None, None) => {
                        return Err(M::Error::custom(
                            "Route must carry exactly one of `direct_response`, `route` or \
                             `redirect`; neither is present",
                        ));
                    }
                    _ => {
                        return Err(M::Error::custom(
                            "Route must carry exactly one of `direct_response`, `route` or \
                             `redirect`; more than one is present",
                        ));
                    }
                };
```

The wording keeps `neither is present` verbatim from the landed message (so the pre-existing test
and any archived narrative still match) and replaces `both are present` with
`more than one is present`, which is now the accurate statement.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test -p envoy-config --lib redirect
cargo test -p envoy-config --lib
```

Expected: the new tests **PASS**; the full `envoy-config` lib suite **still passes**, including
the three pre-existing at-risk tests. Confirm those three by name:

```bash
cargo test -p envoy-config --lib -- rejects_route_with_both_direct_response_and_route rejects_route_with_neither_direct_response_nor_route rejects_route_with_unknown_top_level_key
```

Expected: **3 passed.**

- [ ] **Step 5: Honour TDD's RED for the characterization pin**

`all_five_preexisting_route_keys_still_parse` passed before the implementation — it pins
ALREADY-correct behaviour, so it has no natural RED. Prove it is not vacuous with a mutation:

```bash
# Mutate: drop "route" from the visitor's accepted key list, then rebuild and re-run.
# (Edit :2490 to remove the "route", line — do NOT commit this.)
cargo test -p envoy-config --lib all_five_preexisting_route_keys_still_parse 2>&1 | grep -E 'Compiling|test result'
```

Expected: the run shows `Compiling envoy-config` (proving a forced rebuild, not a stale binary)
and the test goes **RED**. Then revert the mutation and re-run to confirm GREEN. Record both
outcomes in `PROGRESS.md`. **Revert the mutation before committing** and re-grep that it is gone.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 76.1 task 4: Route visitor accepts redirect: — six-name key list + three-way cardinality (T-R3/T-R4/T-R10/T-C1..T-C6)"
```

---

## Task 5: the two oneof validators + two `ConfigError` variants

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` — append two variants at the END of `ConfigError`
  (currently closes at `:991`; 123 variants → 125)
- Modify: `crates/envoy-config/src/bootstrap.rs` — fill in the Task 3 inert arm in `validate_hcm`
- Test: `crates/envoy-config/src/bootstrap.rs` (`mod tests`)

**Interfaces:**
- Consumes: `RouteAction::Redirect` (Task 3), the visitor (Task 4).
- Produces: `ConfigError::RedirectPathRewriteConflict { listener: String, route: String }` and
  `ConfigError::RedirectSchemeRewriteConflict { listener: String, route: String }`; boot-fatal
  rejection of J1/J5 and of the two presence cells T-R8/T-R9.

**This is the presence-not-truthiness task — the whole point of the `Option`s.** T-R8 sets
`https_redirect: false` (i.e. `Some(false)`) and T-R9 sets `path_redirect: ""` (i.e. `Some("")`).
Both must REJECT. Checking `.is_some()` — **never** truthiness, **never** `!s.is_empty()` — is
what makes them reject.

- [ ] **Step 1: Write the failing tests**

```rust
    // --- 76.1 Task 5: the two intra-RedirectAction oneof validators (SPEC §4.3) ---

    /// T-R1 (J1): `path_redirect` + `prefix_rewrite` → boot-fatal.
    #[test]
    fn rejects_redirect_with_both_path_redirect_and_prefix_rewrite() {
        let yaml =
            redirect_route_yaml(r#"redirect: { path_redirect: "/p", prefix_rewrite: "/q" }"#);
        let err = crate::parse_bootstrap(&yaml).expect_err("path_rewrite_specifier conflict");
        assert!(
            matches!(err, crate::ConfigError::RedirectPathRewriteConflict { .. }),
            "expected RedirectPathRewriteConflict, got: {err:?}"
        );
    }

    /// T-R9 (A7) — THE PRESENCE PIN, path arm. `path_redirect: ""` is EMPTY but
    /// PRESENT, so upstream still rejects (MEASURED). A validator that tested
    /// `!s.is_empty()` instead of `.is_some()` would wrongly ACCEPT this.
    #[test]
    fn rejects_redirect_with_empty_path_redirect_plus_prefix_rewrite() {
        let yaml = redirect_route_yaml(r#"redirect: { path_redirect: "", prefix_rewrite: "/q" }"#);
        let err = crate::parse_bootstrap(&yaml)
            .expect_err("an EMPTY path_redirect still sets the oneof and must reject");
        assert!(
            matches!(err, crate::ConfigError::RedirectPathRewriteConflict { .. }),
            "expected RedirectPathRewriteConflict, got: {err:?}"
        );
    }

    /// T-R5 (J5): `scheme_redirect` + `https_redirect: true` → boot-fatal.
    #[test]
    fn rejects_redirect_with_both_scheme_redirect_and_https_redirect() {
        let yaml = redirect_route_yaml(
            r#"redirect: { scheme_redirect: "https", https_redirect: true }"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("scheme_rewrite_specifier conflict");
        assert!(
            matches!(err, crate::ConfigError::RedirectSchemeRewriteConflict { .. }),
            "expected RedirectSchemeRewriteConflict, got: {err:?}"
        );
    }

    /// T-R8 (A5) — THE PRESENCE PIN, scheme arm, and the single most important
    /// test in this sub-phase. `https_redirect: false` is FALSE but PRESENT, so
    /// upstream REJECTS (MEASURED), even though `https_redirect: false` ALONE
    /// ACCEPTS (see the accept-direction suite). **This test is what fails if
    /// `https_redirect` is ever "simplified" to a bare `bool`** — a bare bool
    /// cannot distinguish absent from false, so the config would be accepted and
    /// a brand-new reject-direction divergence would be minted silently.
    #[test]
    fn rejects_redirect_with_https_redirect_false_plus_scheme_redirect() {
        let yaml =
            redirect_route_yaml(r#"redirect: { https_redirect: false, scheme_redirect: "ftp" }"#);
        let err = crate::parse_bootstrap(&yaml)
            .expect_err("https_redirect: false is PRESENT and must reject alongside scheme_redirect");
        assert!(
            matches!(err, crate::ConfigError::RedirectSchemeRewriteConflict { .. }),
            "expected RedirectSchemeRewriteConflict, got: {err:?}"
        );
    }

    /// The conflict error carries the offending listener so an operator can find it.
    #[test]
    fn redirect_oneof_conflict_names_the_listener() {
        let yaml =
            redirect_route_yaml(r#"redirect: { path_redirect: "/p", prefix_rewrite: "/q" }"#);
        let err = crate::parse_bootstrap(&yaml).expect_err("conflict");
        assert!(
            err.to_string().contains("hcm_listener"),
            "error should name the listener; got: {err}"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config --lib redirect_with_both redirect_with_empty redirect_with_https redirect_oneof`
Expected: **FAIL to compile** — `error[E0599]: no variant or associated item named
'RedirectPathRewriteConflict' found for enum 'ConfigError'`. After Step 3a the four
reject tests then fail on the assertion, because Task 3's validator arm is still inert.

- [ ] **Step 3a: Append the two `ConfigError` variants**

In `crates/envoy-config/src/lib.rs`, append at the END of the enum — immediately after
`HeaderToMetadataInvalidRule { listener: String, detail: String },` (`:990`) and before the
closing `}` (`:991`). This matches the landed house style: a `/// Phase NN (§ref): …` doc comment
plus a wrapped `#[error(…)]` when the string is long.

```rust
    /// Phase 76.1 (§4.3): a `redirect` route action sets BOTH members of the
    /// `path_rewrite_specifier` oneof (`path_redirect` and `prefix_rewrite`). Envoy rejects this
    /// boot-fatally, and it does so on FIELD PRESENCE, not on value — `path_redirect: ""` still
    /// sets the oneof (MEASURED) — so envoy-rust matches on presence (ADR-0049 all-fatal).
    /// `listener` names the offending HCM; `route` the offending route (empty when unnamed).
    #[error(
        "redirect action on listener `{listener}` route `{route}` sets both `path_redirect` and `prefix_rewrite`; they are members of one oneof and are mutually exclusive"
    )]
    RedirectPathRewriteConflict { listener: String, route: String },

    /// Phase 76.1 (§4.3): a `redirect` route action sets BOTH members of the
    /// `scheme_rewrite_specifier` oneof (`https_redirect` and `scheme_redirect`). Presence-based,
    /// not value-based: `https_redirect: false` plus `scheme_redirect: "ftp"` REJECTS upstream
    /// while `https_redirect: false` alone ACCEPTS (MEASURED). `listener` names the offending
    /// HCM; `route` the offending route (empty when unnamed).
    #[error(
        "redirect action on listener `{listener}` route `{route}` sets both `https_redirect` and `scheme_redirect`; they are members of one oneof and are mutually exclusive"
    )]
    RedirectSchemeRewriteConflict { listener: String, route: String },
```

- [ ] **Step 3b: Fill in the validator arm**

In `crates/envoy-config/src/bootstrap.rs`, replace Task 3's inert
`RouteAction::Redirect(_) => { … }` arm in `validate_hcm`'s `match &r.action` with:

```rust
                RouteAction::Redirect(rd) => {
                    // 76.1 (§4.3): the two intra-RedirectAction oneofs are
                    // exclusive on FIELD PRESENCE, not on value (MEASURED).
                    if rd.path_redirect.is_some() && rd.prefix_rewrite.is_some() {
                        return Err(crate::ConfigError::RedirectPathRewriteConflict {
                            listener: listener_name.to_string(),
                            route: r.name.clone(),
                        });
                    }
                    if rd.https_redirect.is_some() && rd.scheme_redirect.is_some() {
                        return Err(crate::ConfigError::RedirectSchemeRewriteConflict {
                            listener: listener_name.to_string(),
                            route: r.name.clone(),
                        });
                    }
                }
```

`listener_name: &str` is a parameter of `validate_hcm` (`:3896`) and `r` is the loop binding, so
both are in scope with no borrow gymnastics. **Use `.is_some()`, never `.unwrap_or(false)` and
never an emptiness test** — that is the whole rule.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test -p envoy-config --lib redirect
cargo test -p envoy-config --lib
```

Expected: the five new tests **PASS**; the full suite still passes.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 76.1 task 5: the two RedirectAction oneof validators — presence-not-truthiness (T-R1/T-R5/T-R8/T-R9)"
```

---

## Task 6: the end-to-end accept-direction suite + `Serialize` round-trips

**Files:**
- Test only: `crates/envoy-config/src/bootstrap.rs` (`mod tests`)

**Interfaces:**
- Consumes: everything from Tasks 1-5.
- Produces: no new production code. This task proves the four MEASURED acceptances survive the
  *whole* pipeline (parse **and** validate), and that both `Serialize` arms round-trip.

Tasks 2's accept tests deserialized `RedirectAction` in isolation; these drive the full
`parse_bootstrap` — which is what actually proves envoy-rust does not reject what upstream
accepts, because the validator (Task 5) also runs.

- [ ] **Step 1: Write the failing tests**

```rust
    // --- 76.1 Task 6: end-to-end accept direction + Serialize round-trips ---

    /// T-A1..T-A6 through the FULL pipeline (parse AND validate). Each of these
    /// is a MEASURED upstream ACCEPT; envoy-rust must not reject any of them.
    #[test]
    fn accepts_every_measured_redirect_acceptance_end_to_end() {
        let cases: &[(&str, &str)] = &[
            ("T-A1 port_redirect: 0", "redirect: { port_redirect: 0 }"),
            (
                "T-A2 port_redirect: 70000 (no PGV upper bound)",
                "redirect: { port_redirect: 70000 }",
            ),
            ("T-A3 empty host_redirect", r#"redirect: { host_redirect: "" }"#),
            (
                "T-A4 empty scheme_redirect",
                r#"redirect: { scheme_redirect: "" }"#,
            ),
            (
                "T-A5 https_redirect: false ALONE",
                "redirect: { https_redirect: false }",
            ),
            ("T-A6 bare redirect: {}", "redirect: {}"),
        ];
        for (label, action_block) in cases {
            let yaml = redirect_route_yaml(action_block);
            crate::parse_bootstrap(&yaml)
                .unwrap_or_else(|e| panic!("{label} must ACCEPT but was rejected: {e}"));
        }
    }

    /// T-A7 end-to-end: all five response_code names accept, and each lands as
    /// its variant with the right wire status.
    #[test]
    fn accepts_all_five_response_code_names_end_to_end() {
        let cases = [
            ("MOVED_PERMANENTLY", RedirectResponseCode::MovedPermanently, 301),
            ("FOUND", RedirectResponseCode::Found, 302),
            ("SEE_OTHER", RedirectResponseCode::SeeOther, 303),
            ("TEMPORARY_REDIRECT", RedirectResponseCode::TemporaryRedirect, 307),
            ("PERMANENT_REDIRECT", RedirectResponseCode::PermanentRedirect, 308),
        ];
        for (wire, want, status) in cases {
            let yaml = redirect_route_yaml(&format!("redirect: {{ response_code: {wire} }}"));
            let b = crate::parse_bootstrap(&yaml)
                .unwrap_or_else(|e| panic!("{wire} must accept: {e}"));
            match first_route_action(&b) {
                RouteAction::Redirect(rd) => {
                    assert_eq!(rd.response_code, want, "{wire}");
                    assert_eq!(rd.response_code.status(), status, "{wire}");
                }
                other => panic!("expected Redirect, got {other:?}"),
            }
        }
    }

    /// T-R6 (J6) end-to-end: an unknown response_code NAME is boot-fatal.
    #[test]
    fn rejects_unknown_response_code_name_end_to_end() {
        let yaml = redirect_route_yaml("redirect: { response_code: BOGUS }");
        let err = crate::parse_bootstrap(&yaml).expect_err("BOGUS must reject");
        assert!(err.to_string().contains("BOGUS"), "got: {err}");
    }

    /// T-R7 (J7) end-to-end: a NUMERIC response_code is boot-fatal.
    #[test]
    fn rejects_numeric_response_code_end_to_end() {
        let yaml = redirect_route_yaml("redirect: { response_code: 302 }");
        crate::parse_bootstrap(&yaml).expect_err("numeric response_code must reject");
    }

    /// T-R2 (J2) end-to-end: `regex_rewrite` inside `redirect` is boot-fatal here
    /// (via deny_unknown_fields — a different mechanism from upstream's oneof
    /// error, the same VERDICT, which is all the contract requires).
    #[test]
    fn rejects_regex_rewrite_inside_redirect_end_to_end() {
        let yaml = redirect_route_yaml(
            r#"redirect: { path_redirect: "/p", regex_rewrite: { pattern: x } }"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("regex_rewrite must reject");
        assert!(err.to_string().contains("unknown field"), "got: {err}");
    }

    /// T-C7: round-trip through `impl Serialize for Route` — the `redirect` key is
    /// emitted and the result re-parses to an equal Route.
    #[test]
    fn route_serialize_round_trips_the_redirect_key() {
        let yaml = redirect_route_yaml(
            r#"redirect: { host_redirect: example.com, port_redirect: 8443, strip_query: true, response_code: SEE_OTHER }"#,
        );
        let b = crate::parse_bootstrap(&yaml).expect("parses");
        let listener = &b.static_resources.listeners[0];
        let filter = &listener.filter_chains[0].filters[0];
        let hcm = match filter.typed_config.as_ref().expect("typed_config") {
            TypedConfig::HttpConnectionManager(hcm) => hcm,
            other => panic!("expected HCM, got {other:?}"),
        };
        let route = &hcm.route_config.as_ref().unwrap().virtual_hosts[0].routes[0];

        let ser = serde_yaml::to_string(route).expect("Route serializes");
        assert!(
            ser.contains("redirect:"),
            "Route::serialize must emit the `redirect` key; got:\n{ser}"
        );
        let back: Route = serde_yaml::from_str(&ser).expect("re-parses");
        assert_eq!(&back, route, "Route round-trip must be lossless");
    }

    /// T-C8: round-trip through the SEPARATE `impl Serialize for RouteAction`.
    /// These are two distinct impls — `Route::serialize` does not delegate — so
    /// both need their own coverage.
    #[test]
    fn route_action_serialize_round_trips_the_redirect_key() {
        let yaml = redirect_route_yaml("redirect: { port_redirect: 70000 }");
        let b = crate::parse_bootstrap(&yaml).expect("parses");
        let action = first_route_action(&b);
        let ser = serde_yaml::to_string(action).expect("RouteAction serializes");
        assert!(
            ser.contains("redirect:"),
            "RouteAction::serialize must emit the `redirect` key; got:\n{ser}"
        );
        assert!(
            ser.contains("70000"),
            "port_redirect must survive serialization verbatim; got:\n{ser}"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they pass or fail as expected**

Run: `cargo test -p envoy-config --lib end_to_end serialize_round_trips`

Expected: **most of these PASS immediately** — Tasks 1-5 already implemented the behaviour, so
this task is a characterization/coverage task, not a behaviour change. That is fine and expected.
Honour TDD's RED with the mutation check in Step 3 rather than pretending to a natural RED.

- [ ] **Step 3: Honour TDD's RED with two targeted mutations**

Because these tests pin already-correct code, prove each is non-vacuous. Do each mutation, rebuild
(confirm `Compiling envoy-config` appears — a stale binary would give a FALSE PASS), observe RED,
then revert:

```bash
# Mutation A — the anti-bound pin. Add a bogus upper bound to the validator arm:
#   if rd.port_redirect.is_some_and(|p| p > 65535) { return Err(...); }
# EXPECT: accepts_every_measured_redirect_acceptance_end_to_end goes RED on T-A2.
cargo test -p envoy-config --lib accepts_every_measured 2>&1 | grep -E 'Compiling|test result|panicked'

# Mutation B — the Serialize arm. Change the RouteAction::Redirect arm in
#   `impl Serialize for RouteAction` to serialize_entry("route", ...) instead.
# EXPECT: route_action_serialize_round_trips_the_redirect_key goes RED,
#         while route_serialize_round_trips_the_redirect_key stays GREEN —
#         which is precisely the evidence that the two impls are separate.
cargo test -p envoy-config --lib serialize_round_trips 2>&1 | grep -E 'Compiling|test result|panicked'
```

Mutation B is the one worth doing carefully: it demonstrates the two-impl finding empirically. If
BOTH tests go red on Mutation B, you mutated the wrong impl — re-read `bootstrap.rs:2529-2570`.

**Revert both mutations and re-grep that they are gone before committing.** Record the RED
evidence and the restored GREEN in `PROGRESS.md`.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p envoy-config --lib
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 76.1 task 6: end-to-end accept direction (T-A1..T-A7) + both Serialize round-trips (T-C7/T-C8)"
```

---

## Task 7: the `parse_bootstrap` fuzz corpus seed

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (66 → 67 lines)
- Test: `crates/envoy-config/src/bootstrap.rs` (`mod tests`)

**Interfaces:**
- Consumes: the whole config surface (Tasks 1-6).
- Produces: one tracked corpus seed. **NO new fuzz target**, so §7.5 gate (d) is satisfied by the
  existing `parse_bootstrap` short-budget CI run and **no `ci.yml` edit is needed.**

**The `.gitignore` line is not optional.** `crates/envoy-config/fuzz/.gitignore:1` is
`corpus/parse_bootstrap/*`, so without an explicit `!`-un-ignore line the seed is **silently
untracked and invisible to CI**. There are 63 such lines today at `:2-64`, and the list is
append-ordered by phase, NOT alphabetical.

- [ ] **Step 1: Write the failing test**

```rust
    /// 76.1 Task 7: the fuzz corpus seed must be a VALID bootstrap, so the
    /// `parse_bootstrap` fuzz target starts from a reachable state rather than
    /// from a document that dies in the YAML scanner.
    #[test]
    fn redirect_fuzz_corpus_seed_parses() {
        let seed = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml"
        ))
        .expect("the 76.1 corpus seed must exist and be readable");
        crate::parse_bootstrap(&seed).expect("the corpus seed must parse and validate");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-config --lib redirect_fuzz_corpus_seed`
Expected: **FAIL** — panics on `the 76.1 corpus seed must exist and be readable: No such file or
directory`.

- [ ] **Step 3: Create the seed**

Create `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml` with exactly
this content. It exercises five distinct `redirect:` shapes — scheme-only, host+port, path+code,
prefix+strip_query, and scheme_redirect+code — over non-overlapping route prefixes:

```yaml
node:
  id: envoy-rust-fuzz-seed
  cluster: envoy-rust-fuzz-seed

static_resources:
  listeners:
    - name: redirect_listener
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/scheme" }
                          redirect: { https_redirect: true }
                        - match: { prefix: "/host" }
                          redirect: { host_redirect: example.com, port_redirect: 8443 }
                        - match: { prefix: "/path" }
                          redirect: { path_redirect: "/new", response_code: FOUND }
                        - match: { prefix: "/prefix" }
                          redirect: { prefix_rewrite: "/v2", strip_query: true }
                        - match: { prefix: "/perm" }
                          redirect: { scheme_redirect: ftp, response_code: PERMANENT_REDIRECT }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 4: Add the `!`-un-ignore line**

In `crates/envoy-config/fuzz/.gitignore`, insert the new line **after** the last existing `!`
line (`!corpus/parse_bootstrap/metadata_filter.yaml`, `:64`) and **before** `artifacts/` (`:65`):

```
!corpus/parse_bootstrap/route_redirect_action.yaml
```

The file must end up **67 lines** with **64** `!` lines.

- [ ] **Step 5: Verify the test passes AND the seed is genuinely tracked**

```bash
cargo test -p envoy-config --lib redirect_fuzz_corpus_seed
wc -l crates/envoy-config/fuzz/.gitignore                      # expect 67
grep -c '^!' crates/envoy-config/fuzz/.gitignore               # expect 64
git add crates/envoy-config/fuzz/
git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml
git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/ | wc -l   # expect 64
```

Expected: test **PASS**; `git ls-files` **prints the seed path** (an empty result means the `!`
line is wrong or misplaced — fix it, do not proceed); tracked-seed count 63 → **64**.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/fuzz/
git commit -m "phase 76.1 task 7: parse_bootstrap corpus seed for redirect: routes + its !-un-ignore line"
```

---

## Task 8: full local gate rehearsal + `PROGRESS.md`

**Files:**
- Create: `docs/envoy-rust/phases/76.1-redirect-config-surface/PROGRESS.md` (append per task as
  you go; this task finalises it)

**Interfaces:**
- Consumes: Tasks 1-7.
- Produces: a `PROGRESS.md` that quotes real command output, and a tree ready for the §5 state-4
  verification session.

**This task does NOT run the §7.5 gate.** State 4 is a **separate session** (§5.1; ADR-0127 — the
context that wrote the code must not be the one that grades it). What this task does is *rehearse*
the cheap local half so state 4 does not open on an avoidable RED.

- [ ] **Step 1: Rehearse the (e)-gate commands locally**

```bash
S=$(mktemp -d)
cargo fmt --all -- --check                      > "$S/fmt.txt" 2>&1; echo "fmt exit=$?"
cargo build --workspace --all-targets           > "$S/build.txt" 2>&1; echo "build exit=$?"
cargo clippy --workspace --all-targets --all-features -- -D warnings \
                                                > "$S/clippy.txt" 2>&1; echo "clippy exit=$?"
grep -c 'Checking' "$S/clippy.txt"
cargo test -p envoy-config --lib                > "$S/t-config.txt" 2>&1; grep 'test result' "$S/t-config.txt"
cargo test -p envoy-http1 --lib                 > "$S/t-http1.txt" 2>&1; grep 'test result' "$S/t-http1.txt"
```

Expected, from this session's pre-flight of the same code: `fmt` exit **0** with an empty file;
build exit 0; clippy exit **0** with **zero** `warning`/`error` lines; `envoy-config`
**653 passed / 0 failed**; `envoy-http1` **187 passed / 0 failed**. (The absolute test counts
will differ from 653/187 because this plan's house-style tests are more numerous than the
pre-flight's compressed ones — what must hold is **0 failed** and a count that GREW.)

**Redirect to files; never pipe a verification run through `tail`** — it truncates the
`failures:` block and drops the `Compiling` line that proves a non-stale binary.

- [ ] **Step 2: Rebuild `envoy-bin` (a local differential would otherwise use a stale binary)**

```bash
cargo build -p envoy-bin 2>&1 | grep -E 'Compiling envoy-bin|Finished'
```

The differential harness runs `target/debug/envoy-bin`, and this sub-phase **adds a config key**.
A stale binary REDs every fixture with a bogus `unknown field 'redirect'`. This is a known trap;
do the rebuild even though this sub-phase adds no fixture, because gate (b) re-runs all 85.

- [ ] **Step 3: Finalise `PROGRESS.md`**

It must contain, per task: what changed, the RED evidence (including the Task 4 / Task 6 mutation
checks and their reverts), the GREEN evidence, and **verbatim** command output — not paraphrase.
Also record explicitly:

- Gate (a) is **vacuously met**: this sub-phase adds no differential fixture. State it, so a
  reviewer does not read the absence as an oversight.
- Gate (d) is met by the **existing** `parse_bootstrap` target; no `ci.yml` edit was needed; the
  new seed is tracked (quote the `git ls-files` output).
- `ConfigError` went **123 → 125** variants.
- `crates/envoy-config/fuzz/.gitignore` went **66 → 67** lines; tracked seeds **63 → 64**.
- The `cargo fmt` reflow of the `lib.rs` `pub use` block was **rustfmt's**, not hand-written.

- [ ] **Step 4: Commit and hand off to state 4**

```bash
git add docs/envoy-rust/phases/76.1-redirect-config-surface/PROGRESS.md
git commit -m "phase 76.1 state-3: implementation complete — 8 TDD tasks landed; STATE advanced to state-4"
```

Then update `docs/envoy-rust/STATE.md` (`## Next expected skill` →
`superpowers:verification-before-completion` at state 4, a **SEPARATE** session), perform the
ADR-0035 relocation, and stop. **Do not run the §7.5 gate in this session. Do not chain into
state 4.**

---

## Self-review

**1. Spec coverage.** Every §6 obligation maps to a task:

| SPEC §6 item | Task | SPEC §6 item | Task |
|---|---|---|---|
| T-R1 | 5 | T-A1 | 2, 6 |
| T-R2 | 2, 6 | T-A2 | 2, 6 |
| T-R3 | 4 | T-A3 | 2, 6 |
| T-R4 | 4 | T-A4 | 2, 6 |
| T-R5 | 5 | T-A5 | 2, 6 |
| T-R6 | 1, 6 | T-A6 | 2, 6 |
| T-R7 | 1, 6 | T-A7 | 1, 6 |
| T-R8 | 5 | T-C1..T-C5 | 4 |
| T-R9 | 5 | T-C6 | 4 |
| T-R10 | 4 | T-C7, T-C8 | 6 |
| | | T-C9 | 3 |

All 26 covered; 10 reject + 7 accept + 9 mechanics. SPEC §4 scope items: §4.1 → Task 1; §4.2 →
Task 2; §4.3 → Task 5; §4.4 items 1/5/6 → Task 3, items 2/3/4 → Task 4; §4.5 → Task 7.
§7 gate: (a) vacuous + (b)/(c)/(e) rehearsed in Task 8, (d) in Task 7, (f) is state 5.
**Two edit sites the SPEC omits** (the `pub use` re-export and the `hcm.rs` test import) are
covered in Task 3 steps 3b/3c.

**2. Placeholder scan.** No "TBD", no "add error handling", no "similar to Task N", no
"write tests for the above". Every code step carries literal, fmt-clean, clippy-clean Rust that
was compiled and run. Every helper named (`route_action_yaml`, `NO_CLUSTERS`, `BACKEND_CLUSTER`,
`first_route_action`, `make_req`, `cluster_mgr_empty`, `mk_stats`, `test_router_only_pipeline`,
`synth_501`) was verified to exist at a named anchor. The one helper this plan *introduces* —
`redirect_route_yaml` — is defined in full in Task 4 step 1 before Tasks 5-6 use it.

**3. Type consistency.** `RedirectResponseCode` variants are CamelCase in Rust
(`MovedPermanently`) and SCREAMING_SNAKE on the wire (`MOVED_PERMANENTLY`) — used consistently in
both forms throughout. The `-> u16` accessor is named `status()` in Task 1 and referenced as
`.status()` in Task 6, not `as_u16()`/`to_u16()`. Both `ConfigError` variants are spelled
`RedirectPathRewriteConflict` / `RedirectSchemeRewriteConflict` with fields
`{ listener: String, route: String }` in the declaration (Task 5 step 3a), the construction
(step 3b) and the assertions (step 1). `RouteAction::Redirect(RedirectAction)` is the same
spelling in the enum, all four match arms, and the T-C9 test.

**4. Ordering.** Tasks 1→2→3→4→5 are a strict compile-order chain and cannot be reordered:
Task 3 needs Task 2's type; Task 4 needs Task 3's variant; Task 5 needs Task 4's parse path to
reach the validator. Tasks 6 and 7 depend on 1-5 but not on each other. Task 8 is last.
