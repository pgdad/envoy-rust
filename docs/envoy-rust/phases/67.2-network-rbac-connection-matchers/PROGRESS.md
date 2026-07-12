# Phase 67.2 — network RBAC connection-level matcher arms — Progress Log

> Running log, updated on each task completion (§5 state-3, `superpowers:executing-plans`).
> This is the §5 state-3 IMPLEMENTATION session. It does NOT chain into state-4
> verification (§5.1) — the §7.5 gate is a separate session. Written for a stranger
> with zero prior context (D-3.4).

**Plan:** `docs/envoy-rust/phases/67.2-network-rbac-connection-matchers/PLAN.md` — 6 tasks / ~695 net LoC.
**Started from:** clean `main` at `af89137` (= `origin/main`; the §5 state-2 PLAN-write commit). No sibling ahead.

TDD per task: write failing test → confirm RED → implement → confirm GREEN → commit.

---

## Task 1 — `CidrRange` type (parse, validate, `contains`) — ✅ DONE

**Commit:** (see `phase 67.2 task 1`).

- Added `pub struct CidrRange { address_prefix: std::net::IpAddr, prefix_len: u8 }`
  (`#[serde(deny_unknown_fields)]`) just above the `Permission` enum in
  `crates/envoy-config/src/bootstrap.rs`, plus `CidrRange::validate` (family-max
  prefix width, `Err(detail)`), `CidrRange::contains` (v4/v6, IPv4-mapped-IPv6
  canonicalised to IPv4 first per ADR-0133), and the free `prefix_match` bit helper.
- Added `ConfigError::InvalidCidrRange { listener, policy_name, path, detail }`
  (scope-neutral `listener {listener:?}`, per 67.1 W-1 / ADR-0130) in
  `crates/envoy-config/src/lib.rs`, and re-exported `CidrRange` from the crate root.
- **7 unit tests** in the RBAC `mod tests`: v4 contains, /0 + /32 boundaries, v6 + /128,
  IPv4-mapped-IPv6 peer matches an IPv4 range, cross-family never matches, over-wide
  prefix rejected by `validate`, and `deny_unknown_fields` + bare-`u8` `prefix_len`
  (the ADR-0133 wrapper rejection).

**Deviation from the PLAN's verbatim test text (found on contact with reality):** the
plan's IPv6 test YAML used unquoted `address_prefix: 2001:db8::` / `::1`. serde_yaml 0.9
scans a plain scalar ending in a colon-before-newline as a mapping key
(`"mapping values are not allowed in this context"`), so those two literals must be
QUOTED (`"2001:db8::"`, `"::1"`). This is inherent YAML behavior for IPv6 literals ending
in `::`, not a `CidrRange` logic issue; IPv4 literals and Rust `.parse()` literals are
unaffected. No `CidrRange` code changed — only the test YAML was quoted.

**Result:** `cargo test -p envoy-config cidr_range` → **7 passed**.

---

## Task 2 — five enum arms + V-1 fallout across all match sites — ✅ DONE

**Commit:** (see `phase 67.2 task 2`). The load-bearing, atomic task (the reason parent 67 split).
Adding the arms broke the compile at four exhaustive match sites across three crates; all
classified in ONE green commit (D-3.6). NO `_ =>` catch-all anywhere — the compile break was
the forcing function (confirmed: `cargo build --workspace --all-targets` went RED on the missing
variants, then GREEN once every site was classified).

- **Enum arms + dispatch** (`crates/envoy-config/src/bootstrap.rs`):
  `Permission::{DestinationIp(CidrRange), DestinationPort(u16)}`,
  `Principal::{DirectRemoteIp, RemoteIp, SourceIp}(CidrRange)`, each with a `#[serde(rename)]`
  and a line in the hand-rolled `impl_single_key_oneof!` deserializer.
- **Site 1 — `define_rbac_tree_validator!`:** added a trailing `extra_leaves: [ $($leaf),* ]`
  variadic macro param emitting `crate::$node::$leaf(_) => Ok(())`; instantiations pass
  `[DestinationIp, DestinationPort]` / `[DirectRemoteIp, RemoteIp, SourceIp]`.
- **Site 2 — `validate_l4_permission` / `validate_l4_principal`:** widened the L4 allow-list to
  ADMIT the arms (the `header`/`url_path`/`metadata` rejections stay); each CidrRange's width is
  validated via `CidrRange::validate` → `ConfigError::InvalidCidrRange`.
- **Site 3 — HTTP filter `lower_permission` / `lower_principal`** (`crates/envoy-filter/src/rbac.rs`):
  the L4-only arms are rejected fail-loud (`FilterError::InvalidConfig`) — startup-fatal (this runs
  inside `collect::<Result<_,_>>()?` at filter build). Upstream ACCEPTS them in an HTTP rbac filter
  (measured, ADR-0133), so this is a deliberate divergence (ADR-0049 decision-2 (b)), NOT parity.
- **Site 4 — network engine `permission_matches` / `principal_matches`**
  (`crates/envoy-bin/src/network_rbac.rs`): `destination_ip` → `local_addr.ip()`, `destination_port`
  → `local_addr.port()`, the three source-IP arms → `peer_addr.ip()` (one shared expression).
  REMOVED both `#[allow(clippy::only_used_in_recursion)]` attrs (`conn` is now read) and rewrote the
  two stale "threaded through but not read" doc paragraphs + the stale module header.
- **Replaced** the now-stale test `network_rbac_connection_matcher_arms_do_not_exist_yet` with
  `network_rbac_accepts_connection_matcher_arms` (they now deserialize + validate) and added
  `network_rbac_rejects_invalid_cidr_prefix_len`. Added HTTP-reject witnesses
  (`http_rbac_rejects_destination_port_permission`, `http_rbac_rejects_direct_remote_ip_principal`)
  and engine witnesses (`direct_remote_ip_matches_peer`, `destination_port_matches_local_port`).

**Note for the state-4 session:** `envoy-bin` is a BINARY crate (no lib target), so the plan's
`cargo test -p envoy-bin --lib ...` fails with "no library targets"; use `--bins` (or plain
`cargo test -p envoy-bin`). Same applies to Tasks 3/5 commands in the plan.

**Result:** `cargo build --workspace --all-targets` GREEN; `cargo clippy -p envoy-bin --all-targets
-- -D warnings` clean; envoy-config network_rbac (16), envoy-filter http_rbac_rejects (2), envoy-bin
network_rbac (13) all pass.

---

## Task 3 — exhaustive engine backstops (synthetic `ConnectionInfo`) — ✅ DONE

**Commit:** (see `phase 67.2 task 3`). Tests only (`crates/envoy-bin/src/network_rbac.rs`), no impl change.

- Added a `conn2()` helper (peer `192.0.2.5:40000` NOT in 10.0.0.0/8; local port 9999) so no-match
  cases are meaningful.
- `direct_remote_ip_no_match_denies` (peer outside range ⇒ inverse of ALLOW, `denied` ticks);
  `remote_ip_and_source_ip_evaluate_peer_like_direct_remote_ip` (both aliases match/no-match on peer);
  `destination_port_no_match_denies` (local 9999 ≠ 10000); `destination_ip_matches_local_ip`
  (local 127.0.0.1 ∈ 127.0.0.0/8); `combinators_over_new_leaves` (`and_rules` of two destination
  predicates + `not_id` over a non-matching `direct_remote_ip`).

**Result:** `cargo test -p envoy-bin --bins network_rbac` → **18 passed**.

---

## Task 4 — HTTP RBAC rejects every L4-only arm (fail-loud) — ✅ DONE

**Commit:** (see `phase 67.2 task 4`). Tests only (`crates/envoy-filter/src/rbac.rs`).

- Added the remaining per-arm `lower_*` witnesses: `http_rbac_rejects_destination_ip_permission`
  and `http_rbac_rejects_remote_ip_and_source_ip_principals` (all five arms now covered, alongside
  Task 2's `destination_port` / `direct_remote_ip` witnesses).
- Added `http_rbac_build_from_config_rejects_l4_principal_startup_fatal` — mirrors the existing
  `build_from_config_allow_with_header_principal_creates_filter`, proving the rejection is
  STARTUP-FATAL (propagates through `RbacFilter::build_from_config`, not just the private `lower_*`).

**Result:** `cargo test -p envoy-filter http_rbac` → **5 passed**.

---

## Task 5 — end-to-end loopback backstops (real socket, 127.0.0.1) — ✅ DONE

**Commit:** (see `phase 67.2 task 5`). Tests only (`crates/envoy-bin/tests/network_filter_rbac.rs`).
Boots the real `target/debug/envoy-bin` and connects over loopback so `peer_addr`/`local_addr`
are EXACT.

- Added an `allow_rules(permissions, principals)` helper at `rbac_echo_cfg`'s required 16-space
  indentation, and three tests: `direct_remote_ip_loopback_allows_end_to_end` (peer 127.0.0.1 ∈
  127.0.0.0/8 ⇒ ALLOW, echo round-trips `ping`), `direct_remote_ip_non_loopback_denies_end_to_end`
  (peer ∉ 10.0.0.0/8 ⇒ DENY, zero bytes clean EOF), and `destination_port_end_to_end` (rule naming
  the listener's own port ⇒ ALLOW; rule naming a different port ⇒ DENY, zero bytes). All follow the
  ADR-0131 first-byte / 67.1 DENY-wire-shape mechanics of the two named 67.1 tests.

**Two corrections vs the PLAN's Task-5 skeleton (found on contact with reality):**
1. The harness helper is `reserve_port()` (from `mod common`), not the plan's `free_port()`.
2. The plan's skeleton `rules` strings used the engine `cfg()` indentation (`"  action: …"`); the
   integration `rbac_echo_cfg` splices a FULL `rules:` block at 16-space indent (mirroring the
   file's `ALLOW_ALL`/`DENY_ALL` consts). Used the `allow_rules` helper to get it exactly right.

**Result:** `cargo build -p envoy-bin` then `cargo test -p envoy-bin --test network_filter_rbac`
→ **21 passed** (18 pre-existing 67.1 + 3 new).

---

## Task 6 — `BEHAVIOR_CONTRACT.md` rows + fuzz corpus seed — ✅ DONE

**Commit:** (see `phase 67.2 task 6`). Docs + fuzz seed.

- `docs/envoy-rust/BEHAVIOR_CONTRACT.md`: added item **14** to the `envoy.filters.network.rbac`
  section — the five arms and what each evaluates against; the `remote_ip` ≡ `direct_remote_ip` ≡
  `source_ip` equivalence (no listener filters) and `source_ip` deprecation (warning not replicated);
  the `CidrRange` bare-`u8` `prefix_len` vs wrapper divergence + IPv4-mapped-IPv6 canonicalisation;
  `destination_port` as `u16`; the CORRECTED framing that the HTTP RBAC filter rejecting these L4
  arms is a deliberate FAIL-LOUD divergence (upstream ACCEPTS them, measured), NOT parity; and the
  no-differential-fixture rationale. Updated item **11** (arms now exist as of 67.2) and the section
  header (adds "connection-level matcher arms phase 67.2, ADR-0133").
- **Fuzz corpus seed:** `crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_rbac_cidr.yaml`
  — a `[rbac, echo]` bootstrap whose network rbac policy exercises `destination_ip`,
  `destination_port`, and all three source-IP arms with `CidrRange`s. Added its `!`-un-ignore line to
  `crates/envoy-config/fuzz/.gitignore`. **NO new fuzz target** — the pre-existing `parse_bootstrap`
  target reaches the new `CidrRange` parser (§7.5 gate (d) satisfied by the seed; state-4 records it
  explicitly). Proven tracked: `git ls-files …/corpus | grep rbac_cidr` lists it. The seed config is
  ACCEPTED by `target/debug/envoy-bin` (parses + validates + binds), so it is a valid CidrRange seed.

---

## State-3 implementation COMPLETE — handoff to state-4

All 6 PLAN tasks landed on `main` (commits `f31b21c` → task-6). Per §5.1 this session STOPS here and
does NOT run the §7.5 verification gate — that is the state-4 session's job.

**§6.1 mid-execution valve:** never fired. No single task's sub-steps blew past ~10 items. The plan's
~695 LoC / 6-task estimate held.

**State-4 session must (per PLAN "State-4 verification checklist"):** run + quote into a state-4
record `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features
-- -D warnings` (watch the two removed `#[allow(clippy::only_used_in_recursion)]`), `cargo fmt --all
-- --check`, `cargo test --workspace --no-fail-fast` (the ~5 environmental REDs are CI-authoritative;
never pipe through `tail`), `cargo deny check`; RECORD §7.5 gate (d) EXPLICITLY (no new fuzz target;
pre-existing `parse_bootstrap` + the new `network_filter_rbac_cidr.yaml` seed); confirm the
regression-only differential surface (`0001`–`0073` green) after `cargo build -p envoy-bin`; confirm
CI green on the FULL 40-char SHA. **Command note:** `envoy-bin` is a BINARY crate — use `--bins`, not
`--lib`, for its lib-style tests (`cargo test -p envoy-bin` also works).

---

# Phase 67.2 — §5 STATE-4 (verification) — the FULL §7.5 (a)-(f) gate

> `superpowers:verification-before-completion`. This is a SEPARATE session from the state-3
> implementation (§5.1). It ran the whole §7.5 gate against the current tree and quotes every
> command's output below. **NO code, no fixture, no `known-failures.txt`, no ROADMAP row, and no
> new `REVIEW.md` was created.** Cold-started clean: `git status --porcelain` empty; branch `main`;
> `HEAD` = `origin/main` = `42ce89c` (the state-3 STATE-advance commit); `git fetch origin --prune`
> showed no sibling ahead → §5 state 4.

## STEP 0.5 — CI on the state-3 push (FULL 40-char SHA)

```
$ gh run list --commit 42ce89c65cf366f8addae91d6f704db00794c9f8
completed  success  phase 67.2: §5 state-3 implementation COMPLETE — STATE advanced to st…  ci  main  push  29129557480  6m20s
```

CI run `29129557480` GREEN on the full SHA. No rerun needed.

## §7.5 gate (a) — `cargo build --workspace --all-targets`

```
$ cargo build --workspace --all-targets
EXIT=0
```

Clean. The debug `target/debug/envoy-bin` the differential harness runs is now fresh (no stale
`unknown field`/`unknown filter`).

## §7.5 gate (e) — clippy / fmt / deny

```
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
CLIPPY_EXIT=0        # Checking envoy-config / envoy-filter / envoy-bin all present in the run
```

Clean — the two REMOVED `#[allow(clippy::only_used_in_recursion)]` attrs on
`network_rbac.rs::permission_matches`/`principal_matches` produce NO warning (the new arms read
`conn`, so the recursion is no longer the only use).

```
$ cargo fmt --all -- --check
FMT_EXIT=0           # (empty output)
```

Clean (the state-3 `cargo fmt --all` cleanup commit `494043f` holds).

```
$ cargo deny check
DENY_EXIT=0
advisories ok, bans ok, licenses ok, sources ok
```

Clean (only benign `license-not-encountered` warnings — no advisory against any dep; no patch-bump
needed this session).

## §7.5 gate — `cargo test --workspace --no-fail-fast`

Full output redirected to a file (NEVER piped through `tail`). Aggregate:

```
$ cargo test --workspace --no-fail-fast     # TEST_EXIT=101
TOTAL passed=1925 failed=5
```

`passed + failed = 1925 + 5 = 1930`. **Cross-check: `local passed+failed == CI passed = 1930`**
(CI run `29129557480` GREEN — those 5 pass on CI's networking).

**All 5 REDs are the documented environmental core — NONE in the RBAC surface:**

| Failing test | Cause (memory) |
|---|---|
| `access_log_h2_rcd_upstream_reset` | envoy `UF`/`Network_is_unreachable`/`[fdc4:f303:9324::254]` (IPv6 close-backend unreachable) vs rust `UC` — `tcpclosebackend-ipv6-unreachable-host-flake` |
| `access_log_h2_uc_upstream_reset` | same — envoy `UF`, rust `UC` |
| `access_log_rcd_upstream_reset` | same — envoy `UF`/`Network_is_unreachable`, rust `UC` |
| `access_log_rf_upstream_reset` | same — envoy `UF`, rust `UC` |
| `admin_config_dump_server_info` | envoy routes backend via `192.168.65.2` (non-allow-listed host bridge IP) — `differential-host-bridge-ip-192-168-65-2` |

**The phase's own RBAC surface is GREEN:** `envoy_config` unittests **583 passed** (incl. `cidr_range` 7
+ `network_rbac` 16); `envoy_filter` unittests **211 passed** (incl. `http_rbac` 5); `envoy_bin`
unittests **37 passed** (incl. `network_rbac` 18); the `network_filter_rbac.rs` integration binary
**21 passed** (18 pre-existing 67.1 + 3 new loopback backstops). Zero failures anywhere in `rbac`/`cidr`.

## §7.5 gate (d) — fuzz — RECORDED EXPLICITLY

**NO new fuzz target** (SPEC §D7). The pre-existing `parse_bootstrap` target is the only one
(`ls crates/envoy-config/fuzz/fuzz_targets/` → `parse_bootstrap.rs`). It reaches the new `CidrRange`
parser via the NEW, tracked corpus seed:

```
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap | grep rbac_cidr
crates/envoy-config/fuzz/corpus/parse_bootstrap/network_filter_rbac_cidr.yaml
```

Exercised short-budget (toolchain present — `cargo-fuzz 0.13.2`):

```
$ cargo +nightly fuzz run parse_bootstrap -- -runs=20000 -max_total_time=100    # FUZZ_EXIT=0
#20000  DONE   cov: 15035 ft: 30435 corp: 2579/1675Kb ... exec/s: 10000
Done 20000 runs in 2 second(s)
```

20000 runs, no crash / no panic / no leak. **Gate (d) is SATISFIED by the corpus seed** (not passed
over in silence).

## §7.5 gate (b) — differential surface (regression-only)

**NO new fixture** (`0001`–`0073` unchanged). The differential crate ran as part of the workspace
test run above; the ONLY differential REDs are the 5 environmental ones tabulated above — all
pre-existing, CI-authoritative. The RBAC arms are covered in-process + at loopback (no differential
fixture — the IP/port arms are structurally host-dependent, parent V-4). Regression surface holds.

## §7.5 gate (c) — conformance

Unchanged this phase; `tests/conformance/h2spec/known-failures.txt` untouched (never trimmed —
`h2spec-3-5-2-preface-host-sensitive`). CI-authoritative.

## §7.5 gate (f)

`REVIEW.md` does not exist yet. (f) is UNMET by design — it is satisfied only by the SEPARATE state-5
code-review (`superpowers:requesting-code-review`), which this session does NOT chain into (§5.1;
ADR-0127 names 4→5 as un-chainable).

## Verdict

§7.5 (a), (b), (c), (d), (e) all satisfied (modulo the fully-adjudicated, CI-authoritative
environmental REDs); (f) is the one unmet gate and is the state-5 review's job. `67.2` is
IMPLEMENTATION-COMPLETE and VERIFIED → §5 state 5. **NO new ADR** (ledger head `ADR-0133`, next
`ADR-0134` unreserved). `#![forbid(unsafe_code)]` holds. Per §5.1 this session did NOT chain into
state-5.

---

# Phase 67.2 — §5.2 STATE-3 RE-ENTRY (C-1 repair) — the NOT-APPROVED review's blocking findings landed

> This section RECONCILES the state machine. The §5 state-5 code-review (commit `ab216b4`,
> `REVIEW.md`) returned **NOT APPROVED** on Critical **C-1** (plus Important **I-1** and advisory
> **N-1**); per §5's asymmetry the phase re-opened at **§5 state 3**. The C-1/I-1/N-1 repair was
> then landed on `main` by a **sibling test-hardening workstream** (commits `e0a15dc` +
> `8cab4af`, which cite `REVIEW.md`) as bare `fix()`/`test()` commits — WITHOUT walking the §5
> phase-state machine (no PROGRESS.md entry, no STATE advance). This recording session (per
> `BOOTSTRAP_PROMPT.md` §1 Step E — a state-machine discrepancy adjudicated with
> `superpowers:systematic-debugging`) records that landed repair as the state-3 re-entry and
> advances STATE to §5 state 4. **NO code changed this session** — docs-only reconciliation.

## What landed (the REVIEW.md C-1/I-1/N-1 obligations), and where

- **C-1 (Critical, blocking) — the config-reachable, release-mode data-plane PANIC — FIXED**
  (`e0a15dc fix(envoy-config): CidrRange v4-mapped-v6 prefix panics data plane (C-1)`). Exactly the
  fix `REVIEW.md` §1 prescribed: `CidrRange::validate` now sizes `prefix_len` against the **canonical**
  address family via a new shared `canonical_ip` helper (the single canonicalisation rule now used by
  BOTH `validate` and `contains`), so an IPv4-mapped-IPv6 `address_prefix` such as `"::ffff:127.0.0.0"`
  is bounded at 32 and an over-wide `prefix_len` is rejected fail-loud with
  `ConfigError::InvalidCidrRange` at config load — closing the `validate`/`contains` family
  disagreement (`bootstrap.rs:1646` vs `:1664`/`:1691`).
- **N-1 (advisory) — the defensive `prefix_match` bounds guard — INCLUDED** (same commit): a silent
  `if full > net.len() || full > addr.len() { return false }` bail (NOT a `debug_assert!`, so `contains`
  cannot panic even in debug), keeping the data-plane "must never panic" invariant true
  unconditionally even for a future caller that constructs a `CidrRange` without validating.
- **I-1 (Important) — the coverage blind spot — CLOSED** (`8cab4af test(envoy-config): C-1 config-load
  rejection through real validate path`, plus tests in `e0a15dc`): a mapped-prefix regression test
  `cidr_range_validate_rejects_ipv4_mapped_ipv6_over_wide_prefix` AND a `contains`-level property sweep
  `cidr_range_contains_never_panics_for_validated_prefixes` over every `validate`-passing prefix across
  a v4/v6/v4-mapped address matrix (the `parse_bootstrap` fuzzer never reaches `contains`, so gate (d)
  was structurally blind — the property test replaces it, no new fuzz target, honoring the SPEC posture).
  Both tests fail against the pre-fix code (the property test panics at the exact index).

## Verification of the landed repair (this session, read-only — NOT the §7.5 gate)

Confirmed the repair is real and complete before recording it as the re-entry:

- `cargo build --workspace --all-targets` → exit 0.
- `cargo test -p envoy-config` → **586 passed / 0 failed** (was 583 at state-4; +3 for the
  C-1/I-1 regression + property tests). `cargo test -p envoy-http1` 168, `cargo test -p envoy-tls` 16 —
  green (the sibling workstream's unrelated http1-smuggling / TLS-SNI hardening, NOT 67.2 scope).
- `cargo clippy -p envoy-config -p envoy-http1 -p envoy-tls --all-targets --all-features -- -D warnings`
  → clean.
- **The original live C-1 repro is now CLOSED:** booting `target/debug/envoy-bin` on the exact config
  that previously was accepted-then-panicked (`destination_ip: { address_prefix: "::ffff:127.0.0.0",
  prefix_len: 40 }`) now **exits 1 fail-loud at config load** — `listener "rbac_listener": network rbac
  policy "p0" has an invalid CidrRange at permissions[0]: prefix_len 40 exceeds 32 for IPv4` — no
  panic, no acceptance.
- CI GREEN on `main` at `96a1fd7` (run `29164710520`, the commit carrying the fix).

## Scope note

The C-1/I-1/N-1 repair touched ONLY `crates/envoy-config/src/bootstrap.rs`. The sibling workstream's
other commits on `main` (`076b178`/`de7b643` HTTP/1 Content-Length + Transfer-Encoding smuggling
hardening, `c0689af` TLS unknown-SNI, `000f776`/`96a1fd7` `docs/TEST_GAP_ANALYSIS.md`) are **OTHER
workstreams, NOT phase-67.2 scope** — recorded here only so a reader of `main`'s log is not confused.
`67.2` added no fixture, no fuzz target, no ADR; `#![forbid(unsafe_code)]` holds; `ADR-0131` not
reverted; the deliberate HTTP-rejects-L4 divergence is untouched (confirmed WAI, `REVIEW.md` §5).

## Handoff to state-4

The state-3 re-entry's obligations (C-1 + I-1 + N-1) are landed and verified. Per §5.1 this recording
session does NOT run the §7.5 (a)-(f) gate — that is the SEPARATE state-4 verification session's job,
which then hands to a fresh state-5 re-review that SUPERSEDES `REVIEW.md` (D-3.5) before the state-6
close-out. **NO new ADR** (ledger head `ADR-0133`, next `ADR-0134` unreserved). ROADMAP row `67.2`
stays `in-progress`.
