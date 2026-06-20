# Phase 30 (`30-lb-subset`) — REVIEW

> **Lifecycle state 5** (`BOOTSTRAP_PROMPT.md` §5 — verified, not reviewed →
> `superpowers:requesting-code-review` → REVIEW.md). This review covers the phase-30
> state-3 implementation arc (PLAN Tasks 1–9 = the subset-LB deliverables) verified at
> the state-4 §7.5 gate. **Verdict: APPROVED.**
>
> **Review model:** each of PLAN Tasks 1–9 was ALREADY individually two-stage-reviewed
> (spec-compliance THEN code-quality) by a fresh `superpowers:code-reviewer` subagent during
> execution. This state-5 review is therefore the **holistic phase review** — a single fresh
> `superpowers:code-reviewer` subagent given crafted context (the SPEC + the ADR-0074 §6.2-locked
> subset algorithm + ADR-0075 + the six cross-cutting focus areas, NOT this session's history),
> tasked with the system-level seams per-task reviews cannot fully see, plus a re-triage of the
> open carry-forwards (M29-1/M29-2, M30-1, M30-2).
>
> **Review range:** `750f362` (`9e6eb6e^` — the state-2 PLAN-write base) … `ebdab5d` (the state-4
> HEAD) — the full phase-30 production + test diff (+2111 / −106, 19 files under `crates/` + `tests/`;
> the interleaved `PROGRESS.md`/`SPEC.md`/`PLAN.md`/`STATE.md`/`STATE_HISTORY.md`/`BEHAVIOR_CONTRACT.md`/
> `DECISIONS.md` commits in-range are docs, out of code scope). The state-4 STATE-advance commit
> `ebdab5d` is docs-only and out of code scope.
> **Differential evidence:** the AUTHORITATIVE native-Linux CI run **`27881837635`** @ `1acf78c`
> (both jobs GREEN: fixture `0038-lb-subset` cross-proxy route-selection STRONG witness `test
> lb_subset_fixture ... ok` [`/prod`→prod, `/canary`→canary, `/nope`→503 NO_FALLBACK] + all 37
> pre-existing Docker-gated fixtures `0001`–`0037` [0 failed workspace-wide], h2spec ≥95%
> [`h2spec_pass_rate_gate ... ok`, unchanged — no H2 codec change], `parse_bootstrap` [new
> `cluster_lb_subset.yaml` seed] + `jwt_parse` fuzz clean; `cargo fmt --all -- --check` / `cargo
> clippy --workspace --all-targets --all-features -- -D warnings` / `cargo build --workspace
> --all-targets` / `cargo test --workspace` / `cargo deny check` all clean). Per phase-30's
> local-observability advantage (subset selection is a normal request/response, no
> file-watch/reload trigger) the fixture-0038 differential also ran GREEN locally during state-3.

## Verdict: **APPROVED**

**APPROVED — 0 Critical / 0 Important / 3 Minor (non-blocking).** The independent reviewer
re-derived the §6.2-locked subset algorithm (`subset.rs`) and confirmed it is faithful to the
ADR-0074 contract: per-selector subsets are keyed on the selector's key-set alone (so superset
matching falls out naturally), `resolve()` does selector-key-set matching → value-tuple lookup →
fallback dispatch, and the §A oracle rows (incl. ANY_ENDPOINT / DEFAULT_SUBSET / empty-selector /
NO_FALLBACK no-match→`Eligible::None`) are pinned and pass. The **multi-key tuple-order bug
(90f82de) is genuinely fixed AND regression-guarded** — both `build` (`subset.rs:48-49`) and
`lookup` (`subset.rs:79`) iterate the sorted `BTreeSet` keyset, and the non-vacuous
`multi_key_selector_tuple_order_independent` test (`subset.rs:273`, declaration order ≠ sorted)
would catch a reintroduction. The **no-op invariant is provably byte-identical** to the pre-phase
`pick()` when `lb_subset_config` is absent (the `if let Some(idxs)` block is entirely skipped →
the same `hash_lb`/fast-path/slow-path + `cursor.fetch_add` falls through) — the load-bearing
proof for the 37 green fixtures. The **`endpoint_eligible()` factoring (d405f74) is semantically
identical** across the subset path and the pre-existing slow HC/OD path. **HCM threading is
H1/H2-identical by construction** — H2 reuses H1's `build_response`, so there is exactly one
`subset_match` extraction site (`envoy-http1/src/hcm.rs:1457`). The config wire shapes match
ADR-0075 (endpoint `metadata` + route `metadata_match` nested `core.v3.Metadata`; `default_subset`
flat `google.protobuf.Struct`). No finding was rated above Minor. This is the **fifteenth
consecutive clean state-5** (after 17, 18, 19, 20, 21, 22, 23, 24, 25.1, 25.2, 26, 27, 28, 29).

Per `BOOTSTRAP_PROMPT.md` §5.2 the re-enter-state-3 trigger is a Critical or Important finding;
there are none. The phase lands APPROVED with the M29-1/M29-2 + M30-1 + M30-2 follow-ups carried
forward (the established pattern — REVIEW Minors weighed at the next phase's planning / the next
touch of the shared differential driver). The state-6 deterministic close-out (squash/landing
commit; flip ROADMAP row `30` `in-progress → done`; STATE → AWAITING NEXT PLANNING; ADR-0035
narrative relocation; push) is the NEXT session. This approved `REVIEW.md` satisfies §7.5 gate (f);
(a)–(e) are GREEN at CI `27881837635`.

## Scope reviewed (PLAN Tasks 1–9)

The Task-1 `LbMetadata` + endpoint `metadata` config (`crates/envoy-config/src/bootstrap.rs`, the
`MetadataWire` `envoy.lb`-only shim); the Task-2 `Cluster.lb_subset_config` (accept-all, NO fatal
validator — ADR-0074) + the Task-2 correction (`default_subset` is a FLAT `google.protobuf.Struct`
→ `Option<BTreeMap<String,String>>` via `deserialize_opt_flat_struct` — ADR-0075); the Task-3 route
`metadata_match` config; the Task-4 `crates/envoy-cluster/src/subset.rs` (`SubsetIndex::build` +
`resolve` — the §A §6.2-locked engine + the pinned oracle) + the Task-4 multi-key tuple-order fix
(90f82de); the Task-5 `pick()` subset narrowing (eligible-set, no-op-when-absent) + the Task-5
shared `endpoint_eligible()` predicate (d405f74); the Task-6 HCM route `metadata_match` threading
(H1 `crates/envoy-http1/src/hcm.rs` + H2 `crates/envoy-http2/src/hcm.rs`); the Task-7 fixture
`0038-lb-subset` (cross-proxy route-selection STRONG differential) + `tests/differential/tests/lb_subset.rs`
+ the NEW `Driver::Http1RouteSelect` in `tests/differential/src/lib.rs`; the Task-8 in-process
backstop tests (subset + no-op) in `cluster.rs`; the Task-9 `parse_bootstrap` fuzz seed
`cluster_lb_subset.yaml` + the BEHAVIOR_CONTRACT "LB selection" subset subsection.

## Cross-cutting focus areas — all PASS

1. **§6.2 subset algorithm correctness — PASS.** `subset.rs` builds per-selector subsets keyed on
   the selector key-set alone (superset matching falls out naturally) and `resolve()` implements
   selector-key-set match → value-tuple lookup → fallback dispatch, byte-faithful to PLAN §A. The
   pinned oracle encodes the §A rows + ANY_ENDPOINT / DEFAULT_SUBSET (empty default → `All`) /
   empty-selector / NO_FALLBACK no-match → `Eligible::None` and passes. Independently confirmed
   `resolve(None)→fallback`.
2. **Multi-key tuple-order fix (90f82de) — PASS.** Both `build` (`subset.rs:48-49`) and `lookup`
   (`subset.rs:79`) iterate the sorted `BTreeSet` keyset, so build/lookup tuples agree regardless
   of config declaration order. The regression test `multi_key_selector_tuple_order_independent`
   (`subset.rs:273`) uses `keys:[version,stage]` (declaration ≠ sorted) and the un-sorted value
   order — non-vacuous; would catch a reintroduction (old code → `None`→fallback→`Eligible::None`,
   the test asserts `Eligible::Some(vec![0])`).
3. **The no-op invariant — PASS (byte-identical).** Diffed the new `pick()` against
   `750f362:cluster.rs`. `subset: None` ⇒ `subset_idxs = None` ⇒ the `if let Some(idxs)` block is
   skipped ⇒ control falls through to the **identical** `hash_lb` → fast-path → slow-path code,
   same `cursor.fetch_add(1, Relaxed)` and `% total`. All non-subset call sites (TCP, ring_hash
   tests, eds_reload, HCM) pass `subset_match: None` mechanically. This is what keeps 0001–0037
   green.
4. **`endpoint_eligible()` factoring (d405f74) — PASS (no drift).** The extracted predicate
   reproduces the pre-existing slow-path `match None=>true / Some=>…` logic exactly; the subset
   path's prior `is_none_or` form is logically equivalent. Identical on both paths.
5. **HCM threading (H1 + H2) — PASS (identical by construction).** H2 reuses H1's `build_response`
   (`envoy-http2/src/hcm.rs:18,498`), so there is exactly ONE `subset_match` extraction site
   (`envoy-http1/src/hcm.rs:1457`: `ar.metadata_match.as_ref().map(|m| m.envoy_lb.clone())`). Both
   codecs thread the identical value to `pick_endpoint`. The H1 production deep-clone
   (`clone_route_action`, `:327`) correctly preserves `metadata_match`.
6. **Config wire shapes + fixture quality — PASS.** Endpoint `metadata` + route `metadata_match` are
   nested `core.v3.Metadata` (`MetadataWire`, `envoy.lb`-only, no `deny_unknown_fields` so other
   filter-metadata namespaces parse-and-ignore — SPEC §2.1.1); `default_subset` is flat
   `google.protobuf.Struct` via `deserialize_opt_flat_struct`; both reuse `stringify_scalar` for
   permissive scalar coercion — matches ADR-0075. Fixture 0038 is a genuine STRONG differential
   (two real backends, shared IP / distinct ports / distinct body markers, distinct `envoy.lb`
   metadata; the `Http1RouteSelect` driver asserts cross-proxy marker identity + §A-oracle marker
   agreement + byte-exact `no healthy upstream` for the 503 probe; config bodies byte-identical
   modulo bind address). `#![forbid(unsafe_code)]` holds; clippy clean; the 22 subset unit tests
   pass.

## Findings — Minor (3; non-blocking)

- **NEW — empty `metadata_match` map → fallback is an inferred disposition, not §6.2-observed**
  (`subset.rs:106-107`). `resolve()` treats `Some(m) if !m.is_empty()` as a real match and falls
  *any empty map* through to `_ => self.fallback()`. The §A oracle only ever observed *absent* or
  *non-empty* `metadata_match` against live Envoy — the empty-but-present case is an internally
  consistent inference, not an oracle-backed fact. Failure-output / future-proofing only; blocks
  nothing (no current route emits an empty `metadata_match`). **Fix (optional, defer):** a one-line
  comment at `subset.rs:106-107` flagging the empty-map→fallback disposition as inferred (not
  §6.2-locked), so a future maintainer enabling an empty-map route knows it is unverified.
- **M30-1 (carry-forward, confirmed) — `extract_marker` duplicated ~13 lines** between the new
  route-select driver and the hash-sweep driver (`tests/differential/src/lib.rs` ~:4466). Cosmetic;
  fold a shared module-scope helper WHEN the hash-sweep driver is next touched.
- **M29-1 / M29-2 (carry-forward, confirmed) — RING_HASH-worded `bail!` messages + comments in the
  shared `Http1HashSweep` driver** (untouched this phase; fixture 0038 correctly uses the NEW
  `Http1RouteSelect` driver, so the mistake was NOT repeated). Failure-output-only; fold with M30-1.
  **M30-2 (carry-forward, confirmed) — `Cluster.lb_policy` has no serde default** → a
  `lb_policy`-omitting cluster boots on Envoy but is rejected by envoy-rust; the fixture works
  around it with explicit `lb_policy: ROUND_ROBIN` in both YAMLs. Pre-existing parser-strictness
  divergence; weigh `#[serde(default)]` ROUND_ROBIN in a future config-hardening phase.

## Triage of the per-task minors already logged during state-3 (all CONFIRMED non-blocking)

The per-task Minors recorded in `PROGRESS.md` (the Task-1 Serialize-asymmetry doc note; the
Task-2/3/4/5/6/7/8/9 optional-no-action style/doc Minors) were each dispositioned at their own task
review. The Task-2 `default_subset` wire-shape error and the Task-4 multi-key tuple-order bug were
the two Important findings caught during state-3 — both folded inline (ADR-0075 correction `dd3c2c0`;
the alignment fix `90f82de` + its regression test), so neither survived to this holistic review. The
holistic reviewer surfaced nothing above Minor and independently re-confirmed the four open
carry-forwards (M29-1/M29-2 untouched, M30-1 produced, M30-2 produced) + one NEW Minor (the
empty-map disposition comment).

## Recommendations

- Land **M29-1 + M29-2 + M30-1** together opportunistically — cheapest when
  `tests/differential/src/lib.rs`'s `Http1HashSweep` driver is next touched: hoist a single
  module-scope `extract_backend_marker` (neutral wording, the route-select driver already
  demonstrates it) shared by both drivers, and genericize the hash-sweep `bail!` strings.
- Address **M30-2** in a config-hardening phase with `#[serde(default)]` ROUND_ROBIN on
  `Cluster.lb_policy` — removes the only place the two fixture YAMLs needed a non-Envoy-default
  annotation and closes a real parser-strictness divergence.
- Optionally add the one-line "empty `metadata_match` → fallback is inferred, not §6.2-locked"
  comment (NEW Minor) so the unverified edge is self-documenting.
- StrictDns + subset metadata fan-out is correct (`from_bootstrap` pushes the `envoy.lb` map N times
  per N-address DNS name; the `debug_assert_eq!` alignment guard at `cluster.rs:1167` is good) but
  is never differentially exercised (the fixture is STATIC). Optionally note in SPEC §2.2 that
  StrictDns-subset is built-but-not-differentially-exercised.

## §7.5 phase-done gate (final)

(a) fixture `0038` GREEN + (b) all of `0001`–`0037` GREEN + (c) h2spec ≥95% (unchanged) + (d) the
`parse_bootstrap` subset fuzz seed clean + (e) `cargo build`/`clippy --all-targets
--all-features`/`fmt --check`/`test --workspace`/`deny check` clean — ALL GREEN at the AUTHORITATIVE
Linux CI `27881837635` @ `1acf78c`. (f) `REVIEW.md` approved — THIS document.
`#![forbid(unsafe_code)]` holds (D-3.8). **§7.5 gate (a)–(f) COMPLETE.**

---

_Verdict APPROVED (0C / 0I / 3 Minor → the empty-map disposition comment [NEW] + M29-1/M29-2 + M30-1
+ M30-2 carry-forwards). Range `750f362`..`ebdab5d`, CI `27881837635`. Ledger head ADR-0075 (count
76; next ADR-0076). The state-6 deterministic close-out is the NEXT session._
