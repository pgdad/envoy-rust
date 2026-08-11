# Sub-phase 109.1 Implementation Plan — `RouteMatch.runtime_fraction` config surface, three boot-fatal validators, the store's first typed lookup, and the LIVE gate at both `route_matches` call sites

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every implementation subagent still does TDD (D-3.1) and gets FULL zero-context instructions (D-3.4); any tree-MUTATING subagent gets its OWN worktree reset to current `main`.

**Goal:** Land `RouteMatch.runtime_fraction: Option<RuntimeFractionalPercent>` end-to-end — parse, validate boot-fatally at ALL THREE validation paths, resolve through the runtime snapshot store's FIRST typed lookup, and gate route matching LIVE at BOTH `route_matches` call sites (H2 inheriting via the shared resolver) — witnessed entirely in-process, NO new differential fixture (the 108.1 foundation-slice precedent; fixture `0088` is sibling 109.2's).

**Architecture:** The typed lookup (`RuntimeSnapshot::route_fraction_gate`) lives in `crates/envoy-config/src/runtime.rs` and encodes the `109.1/SPEC.md` §1.3 f64 cascade as a `Result<FractionGate, FractionGateError>`; validators at boot (`validate_hcm`), post-merge (`load_dynamic_resources` → `validate()`) and RDS reload (`reparse_and_select_route_config` + the rds_watcher classifier extension) map its errors to four new `ConfigError` variants. The threading seam is `HCMConfig.runtime: Arc<RuntimeSnapshot>` (SPEC §3 D4, FIXED): public `resolve_route`/`build_response` signatures UNCHANGED (H2 zero edits), the `_in` fns and `route_matches` gain a `&RuntimeSnapshot` parameter.

**Tech Stack:** Rust (pinned toolchain, D-3.9); no new dependencies (D-3.2); `serde`/`serde_yaml` for the wire field; existing `thiserror` `ConfigError`.

**Authority documents (read before executing):** `docs/envoy-rust/phases/109.1-runtime-fraction-config-and-gate/SPEC.md` (the design authority — §1.3 cascade, §3 D1-D8), `BOOTSTRAP_PROMPT.md` §5/§7.5, `docs/envoy-rust/BEHAVIOR_CONTRACT.md` `## Runtime`. ADR-0176 fixed the cut; ADR-0175 the pick.

## Global Constraints

- **Gate every task on `--workspace --all-targets`.** Adding a field to a public struct is a workspace-wide `E0063` blast; `cargo test -p <crate>` stays green while the workspace breaks. Task boundaries run: `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` (gate on a NON-ZERO `Checking` count — exit 0 with zero `Checking` lines is a cached no-op, not evidence), `cargo fmt --all -- --check`, and the task's named test commands with `--no-fail-fast`, full output redirected to a file (never piped through `tail`).
- **TDD per task, no exceptions** (D-3.1). Where a new test would pass immediately (a characterization pin), honour RED with a mutation check in a scratch `git worktree` created `--detach` at HEAD (never mutate the main tree; run the unmutated control from the same worktree; a run with no `test result` line is NOT evidence).
- **`cargo fmt --all` after EVERY code transcription, then `--check`** — transcription does not preserve canonicality (measured, 108.2 state-3 finding 1).
- **Assert test COUNTS, never exit codes.** `ok. 0 passed; N filtered out` is a false green; gate each named test run on the exact expected `N passed`.
- **Every new `ConfigError` variant returnable through `reparse_and_select_route_config` MUST be added to the `rds_watcher.rs` reload classifier's `update_rejected` arm WITH a test** — the `other => unreachable!()` arm is an ABORT in release (`panic = "abort"`) and the compiler will NOT flag the omission. The classifier test lands BEFORE the widening (Task 5 step order).
- **Do NOT touch:** fixtures `0011`/`0087` (or any fixture), `HEADER_ALLOW_LIST` (3 entries), `known-failures.txt` (21 lines), the CSRF `runtime_key` reject (`validate_csrf_config`), the test `runtime_key_is_rtds_inert` (`crates/envoy-http1/src/hcm.rs` — pins the STATUS-CODE-FILTER consumer, inert by design), `Route`'s hand-written `Serialize`/`Deserialize` impls, the jwt matcher `route_match_matches` (`crates/envoy-filter/src/jwt_authn.rs:173-186`), any landed phase artifact (D-3.5), `ENVOY_TARGET.md`, `rust-toolchain.toml`, `ci.yml` (no new fuzz target ⇒ no `ci.yml` edit).
- **No fixture `0088`, no `expectations.yaml`, no `BEHAVIOR_CONTRACT.md` `## Runtime` consumer SUBSECTION, no M-1 correction** — all sibling 109.2 (SPEC §4). The ONLY contract edit in 109.1 is the one-sentence D7 narrowing (Task 7).
- **Line numbers in this plan were verified at commit `b6e38f7` and WILL drift as tasks land — locate every site by the quoted TEXT, never by the inherited number.**
- All new `ConfigError` variants follow the house `thiserror` style (`#[error("…")]` struct variants, fields named `listener`/`route` per the `RedirectPathRewriteConflict` precedent).
- Commits per task, message prefix `phase 109.1 task N:`. `next-prompt.txt` is gitignored — never `git add` it.

## Plan-verify results (SPEC §6 W-1…W-5, re-derived FRESH at this plan-write, commit `b6e38f7`)

- **W-1 (censuses, re-derived):** `git grep -c 'RouteMatch {'` over `crates/ tests/` = **101 raw hits = 100 struct literals + 1 struct def** (`bootstrap.rs:2897`); per-file literals: 57 `envoy-http1/src/hcm.rs`, 36 `envoy-http2/src/hcm.rs`, 3 `envoy-filter/src/jwt_authn.rs`, 2 `envoy-config/src/bootstrap.rs` (`:20238`, `:20297` — jwt tests), 1 `envoy-filter/src/instance.rs`, 1 `envoy-filter/src/types.rs`. ZERO use `..Default::default()`; no `Default` impl. `HCMConfig {` = 51 raw hits; **the 4 `envoy-http2` hits are NOT H1 constructions**: struct def `:37`, `impl` `:42`, ONE H2-wrapper literal `:4871` (wraps `inner: Arc<Http1HCMConfig>` — gains NO field), one fn-signature brace `:6862`. True H1 literal count = **41** (39 `envoy-http1/src/hcm.rs` after subtracting its own def `:124` + `impl` `:179`, plus 2 `rds_watcher.rs` test literals `:345`/`:710`). **NEW census the SPEC did not price:** `HCMConfig::from_config` has **44 call sites** that gain the new argument — 3 production (`envoy-bin/src/main.rs:480/:541/:618`), 1 `envoy-admin/src/endpoint.rs:2029` (**test-only** — inside the `#[cfg(test)]` module at `:1672`; this is W-5's "fourth caller", found and classified), 8 `envoy-http1/src/hcm.rs` tests, 32 `envoy-http2/src/hcm.rs` tests.
- **W-2 (reload classifier, re-read):** the classifier now spans `rds_watcher.rs:189-245` (drifted from the cited 205-240; located by text). It matches exactly SIX variants — `RdsFileError`/`RdsParseError` → `update_failure`; `RdsRouteConfigNotFound`/`UnknownCluster`/`RedirectPathRewriteConflict`/`RedirectSchemeRewriteConflict` → `update_rejected` — then `other => unreachable!(…)`. The three snapshot-dependent variants Task 5 makes returnable through `reparse` go in the `update_rejected` arm; the jwt variant is NOT returnable through `reparse` (RDS route configs carry no jwt rules) and MUST NOT be added to the classifier.
- **W-3 (snapshot infallibility + cost):** `RuntimeSnapshot::from_bootstrap` (`runtime.rs:131-143`) returns `RuntimeSnapshot` directly — no `Result`, total by construction (absent block → `default()`; empty block → one synthetic `""` layer). Cost: one flatten pass into a `BTreeMap` — build ONCE per proxy boot in `main.rs` and `Arc`-clone. (It remains rebuilt per-request by admin `/runtime` at `endpoint.rs:982` — deliberately untouched, out of scope.)
- **W-4 (lookup API, decided — see Task 1):** `RuntimeSnapshot::route_fraction_gate(&self, rf: &RuntimeFractionalPercent) -> Result<FractionGate, FractionGateError>` implementing the §1.3 cascade exactly, plus the infallible request-path wrapper `route_fraction_passes(&self, rf) -> bool` whose `Err` fallback (validated-unreachable) is `default_value.numerator != 0` — total, panic-free, and directly unit-testable.
- **W-5 (production call sites):** exactly THREE `HCMConfig::from_config` call sites in `envoy-bin/src/main.rs` (`:480` uring, `:541` per-worker, `:618` shared); `bootstrap` is already `Arc<Bootstrap>` (`main.rs:58`) and is in scope at all three. The grep-for-a-fourth found only the `envoy-admin` TEST caller (above).
- **Validation-path wiring fact (re-derived):** BOTH the boot path (`parse_bootstrap` → `bootstrap::validate` at `lib.rs:1073`) and the post-merge path (`load_dynamic_resources` → `bootstrap::validate` at `lib.rs:1328`) flow through the SAME `validate()` (`bootstrap.rs:3644`) → `validate_hcm` (`:3912` call, sole caller). Wiring the validator into `validate_hcm`'s route walk (`:4306-4308`) covers paths 1 AND 2 with one edit; only the RDS reload path (`reparse_and_select_route_config`, `rds.rs:101`) needs separate wiring (Task 5).
- **jwt reach (re-derived):** `RequirementRule.r#match: RouteMatch` (`bootstrap.rs:1386`) is the ONLY other `RouteMatch` consumer; `validate_jwt_authn_config` (`:4765`, sole production caller `:4508` inside `validate_hcm`'s filter walk) already walks `for rule in &cfg.rules` (`:4788-4800`) — the CF-109-3 presence check hooks there, needs NO snapshot.
- **Serialization exposure (checked):** `RouteMatch` derives `Serialize` with NO `skip_serializing_if` — `prefix: null`/`path: null` are ALREADY emitted for absent fields today, and no differential fixture asserts a full route-match subtree bilaterally (`0028` compares only `route_config.name`). The new always-`null`-when-absent `runtime_fraction` is consistent with existing behavior; in-process tests that pin exact route JSON (if any surface during Task 2) are updated to include the field, mechanically.
- **§6.1 re-check (re-derived bottom-up, this session):** T1 ≈290 (90 impl + 200 tests) + T2 ≈180 (8 impl + 100 mechanical + 60 tests + 10 seed) + T3 ≈250 (90 impl + 160 tests) + T4 ≈115 (15 impl + ~96 mechanical + 4 main.rs) + T5 ≈125 (15 impl + 110 tests) + T6 ≈205 (25 impl + 180 tests) + T7 ≈15 docs = **≈1180 net LoC, 7 tasks**. Test halves are table-driven (the 76.2 mitigation: one measured cell = one table row), which is what held 76.2 to +5%. Under the ~1500 gate with headroom and far under ~25 tasks: **the §6.1 split does NOT re-fire. No ADR is reserved; if the mid-execution trigger fires anyway, §6.2 applies IN FULL and ADR-0177 records it.**

---

### Task 1: The typed lookup — `route_fraction_gate` cascade in `runtime.rs`

**Files:**
- Modify: `crates/envoy-config/src/runtime.rs` (append after `impl RuntimeSnapshot`, before `flatten_layer`)
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `RuntimeSnapshot`/`RuntimeEntry` (landed 108.1), `crate::RuntimeFractionalPercent`, `crate::FractionalPercent`, `crate::DenominatorType`.
- Produces (later tasks rely on these EXACT names): `pub enum FractionGate { Always, Never }`, `pub enum FractionGateError { NondeterministicValue { key: String, value: String }, MapShapedKey { key: String }, NondeterministicDefault { numerator: u32, denominator: u32 } }`, `RuntimeSnapshot::route_fraction_gate(&self, rf: &crate::RuntimeFractionalPercent) -> Result<FractionGate, FractionGateError>`, `RuntimeSnapshot::route_fraction_passes(&self, rf: &crate::RuntimeFractionalPercent) -> bool`.

- [ ] **Step 1: Write the failing tests** — a table-driven test pinning EVERY §1.1 + §1.2 cell plus the §1.3/§7 edges, and a wrapper test. Add to the existing `mod tests` (reuse its `layer()` helper):

```rust
    /// 109.1 Task 1 helpers: build a snapshot from yaml layer fragments, and a
    /// RuntimeFractionalPercent literal.
    fn snap(layer_yamls: &[&str]) -> RuntimeSnapshot {
        let layers: Vec<crate::RuntimeLayer> = layer_yamls.iter().map(|y| layer(y)).collect();
        let names = layers.iter().map(|l| l.name.clone()).collect();
        RuntimeSnapshot::from_layers(names, &layers)
    }

    fn rf(numerator: u32, denominator: crate::DenominatorType, key: Option<&str>) -> crate::RuntimeFractionalPercent {
        crate::RuntimeFractionalPercent {
            default_value: crate::FractionalPercent {
                numerator,
                denominator,
            },
            runtime_key: key.map(str::to_string),
        }
    }

    /// 109.1 SPEC §1.3: the evaluation cascade, pinned against EVERY measured
    /// cell of §1.1 (13, re-run at the split) and §1.2 (10 V-8 closure cells),
    /// plus the §1.3/§7 derived edges. One measured cell = one table row.
    #[test]
    fn route_fraction_gate_pins_every_measured_cell() {
        use crate::DenominatorType::{Hundred, Million};
        use FractionGate::{Always, Never};
        let empty = RuntimeSnapshot::default();
        let one = |v: &str| snap(&[&format!("name: l\nstatic_layer:\n  gate.k: {v}\n")]);

        // (label, snapshot, rf, expected)
        let ok_cells: Vec<(&str, RuntimeSnapshot, crate::RuntimeFractionalPercent, FractionGate)> = vec![
            ("cell 1: absent key, default 100 -> Always", empty.clone(), rf(100, Hundred, Some("gate.k")), Always),
            ("cell 2: absent key, default 0 -> Never", empty.clone(), rf(0, Hundred, Some("gate.k")), Never),
            ("cell 3: key 0 overrides default 100 -> Never", one("0"), rf(100, Hundred, Some("gate.k")), Never),
            ("cell 4: key 100, default 0 -> Always", one("100"), rf(0, Hundred, Some("gate.k")), Always),
            ("cell 6: quoted \"0\" parses like the integer -> Never", one("\"0\""), rf(100, Hundred, Some("gate.k")), Never),
            ("cell 9: integer value is numerator over HUNDRED, not the default's MILLION -> Always", one("100"), rf(0, Million, Some("gate.k")), Always),
            ("cell 10: unparseable -> default 100 -> Always", one("abc"), rf(100, Hundred, Some("gate.k")), Always),
            ("cell 11: unparseable -> default 0 -> Never (both directions)", one("abc"), rf(0, Hundred, Some("gate.k")), Never),
            ("cell 12: 200 >= 100 -> Always", one("200"), rf(0, Hundred, Some("gate.k")), Always),
            ("cell 13: two layers, base 100 override 0, last-wins final \"0\" -> Never",
                snap(&["name: base\nstatic_layer:\n  gate.k: 100\n", "name: over\nstatic_layer:\n  gate.k: 0\n"]),
                rf(100, Hundred, Some("gate.k")), Never),
            ("cell B1: bool true -> default 100 -> Always", one("true"), rf(100, Hundred, Some("gate.k")), Always),
            ("cell B2: bool true -> default 0 -> Never", one("true"), rf(0, Hundred, Some("gate.k")), Never),
            ("cell B3: bool false is NOT 0 -> default 100 -> Always", one("false"), rf(100, Hundred, Some("gate.k")), Always),
            ("cell F1: yaml 0.0 self-heals to \"0\" via Display -> parses as 0 -> Never", one("0.0"), rf(100, Hundred, Some("gate.k")), Never),
            ("cell F2: yaml 100.0 self-heals to \"100\" -> Always", one("100.0"), rf(0, Hundred, Some("gate.k")), Always),
            ("cell N1: -7 -> default 100 -> Always", one("-7"), rf(100, Hundred, Some("gate.k")), Always),
            ("cell N2: -7 -> default 0 -> Never (both directions)", one("-7"), rf(0, Hundred, Some("gate.k")), Never),
            // §1.3/§7 derived edges (recorded, upstream-unmeasured where noted):
            ("edge: empty-string value -> default (final_value last-NON-EMPTY rule)", one("\"\""), rf(100, Hundred, Some("gate.k")), Always),
            ("edge: NaN spelling -> non-finite -> default", one("NaN"), rf(0, Hundred, Some("gate.k")), Never),
            ("edge: inf spelling -> non-finite -> default", one("inf"), rf(100, Hundred, Some("gate.k")), Always),
            ("edge: negative float -0.5 -> v < 0 -> default", one("-0.5"), rf(100, Hundred, Some("gate.k")), Always),
            ("edge: exponent 1e6 parses >= 100 -> Always (recorded; excluded from fixtures)", one("1e6"), rf(0, Hundred, Some("gate.k")), Always),
            ("edge: -0.0 == 0.0 in IEEE -> Never", one("-0.0"), rf(100, Hundred, Some("gate.k")), Never),
            ("edge: no runtime_key at all -> pure default 100 -> Always", empty.clone(), rf(100, Hundred, None), Always),
            ("edge: empty runtime_key is not consulted -> default 0 -> Never", one("0"), rf(0, Hundred, Some("")), Never),
        ];
        for (label, s, r, expected) in ok_cells {
            assert_eq!(s.route_fraction_gate(&r), Ok(expected), "{label}");
        }

        // Boot-fatal cells (CF-109-1: 0 < v < 100; the WIDENED class includes
        // non-integral floats and float-shaped strings — MEASURED, §1.2).
        for (label, s, r) in [
            ("cell 5: integer 50 is per-request nondeterministic upstream", one("50"), rf(100, Hundred, Some("gate.k"))),
            ("cell F3: 0.5 parses upstream (NOT default) -> boot-fatal here", one("0.5"), rf(100, Hundred, Some("gate.k"))),
            ("cell F4: 1.5 parsed AND per-request sampled upstream (GATED 1/40)", one("1.5"), rf(0, Hundred, Some("gate.k"))),
            ("cell S1: quoted \"0.5\" parses like the float", one("\"0.5\""), rf(100, Hundred, Some("gate.k"))),
        ] {
            assert!(
                matches!(
                    s.route_fraction_gate(&r),
                    Err(FractionGateError::NondeterministicValue { ref key, .. }) if key == "gate.k"
                ),
                "{label}"
            );
        }

        // CF-109-2, the SNAPSHOT-PREFIX rule (cells 7/8 + the two conservative
        // edges analysed in SPEC §3 D3): consulted key K is fatal iff ANY entry
        // starts with "K.".
        let map_snap = snap(&["name: l\nstatic_layer:\n  gate.k:\n    numerator: 0\n    denominator: HUNDRED\n"]);
        for (label, s, r) in [
            ("cell 7: map value at consulted key, default 100", map_snap.clone(), rf(100, Hundred, Some("gate.k"))),
            ("cell 8: map value at consulted key, default 0", map_snap.clone(), rf(0, Hundred, Some("gate.k"))),
            ("edge: scalar K beside literal dotted sibling K.foo -> conservatively fatal (recorded)",
                snap(&["name: l\nstatic_layer:\n  gate.k: 100\n  gate.k.foo: 1\n"]),
                rf(0, Hundred, Some("gate.k"))),
        ] {
            assert!(
                matches!(
                    s.route_fraction_gate(&r),
                    Err(FractionGateError::MapShapedKey { ref key }) if key == "gate.k"
                ),
                "{label}"
            );
        }
        // ...but a DIFFERENT key's dotted entries do NOT trip the prefix rule,
        // and a PREFIX-SHARING SIBLING (gate.k2) does not either ("gate.k" is
        // not a string-prefix of "gate.k2" WITH the dot).
        let sibling = snap(&["name: l\nstatic_layer:\n  gate.k2: 100\n  other.map.leaf: 1\n"]);
        assert_eq!(
            sibling.route_fraction_gate(&rf(100, Hundred, Some("gate.k"))),
            Ok(Always),
            "prefix rule must use \"K.\" — a sibling gate.k2 entry is NOT a gate.k map"
        );

        // Non-deterministic default_value (numerator neither 0 nor the
        // denominator value) is fatal whenever the default is REACHED —
        // directly (no key) or via the unparseable fallback.
        for (label, s, r) in [
            ("edge: default 50/HUNDRED, no key", empty.clone(), rf(50, Hundred, None)),
            ("edge: default 150/HUNDRED reached via unparseable value", one("abc"), rf(150, Hundred, Some("gate.k"))),
        ] {
            assert!(
                matches!(s.route_fraction_gate(&r), Err(FractionGateError::NondeterministicDefault { .. })),
                "{label}"
            );
        }
    }

    /// 109.1 Task 1: the infallible request-path wrapper. Ok maps directly;
    /// the Err arm (validated-unreachable in production — all three error
    /// classes are boot-fatal at every validation path) falls back to the
    /// default_value's sign, total and panic-free.
    #[test]
    fn route_fraction_passes_is_total_and_maps_the_gate() {
        use crate::DenominatorType::Hundred;
        let empty = RuntimeSnapshot::default();
        let fifty = snap(&["name: l\nstatic_layer:\n  gate.k: 50\n"]);
        assert!(empty.route_fraction_passes(&rf(100, Hundred, Some("gate.k"))), "Always -> true");
        assert!(!empty.route_fraction_passes(&rf(0, Hundred, Some("gate.k"))), "Never -> false");
        // Err fallback: nondeterministic value, default 100 -> true; default 0 -> false.
        assert!(fifty.route_fraction_passes(&rf(100, Hundred, Some("gate.k"))), "Err fallback follows default sign (non-zero)");
        assert!(!fifty.route_fraction_passes(&rf(0, Hundred, Some("gate.k"))), "Err fallback follows default sign (zero)");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config --lib -- runtime::tests::route_fraction 2>&1 | tee /tmp/t1-red.log`
Expected: compile error `cannot find type FractionGate` / no method `route_fraction_gate` — the RED.

- [ ] **Step 3: Write the implementation** — append to `runtime.rs` after the existing `impl RuntimeSnapshot` block (extend that same `impl`), types above it:

```rust
/// 109.1: the deterministic route-gate verdict of a validated
/// `RuntimeFractionalPercent`. `Always` = the runtime_fraction gate passes and
/// prefix/path/headers matching applies unchanged; `Never` = the route never
/// matches. There is no sampling arm by design — every nondeterministic input
/// is boot-fatal (CF-109-1/2, ADR-0176 DECISION 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FractionGate {
    Always,
    Never,
}

/// 109.1: the boot-fatal classes of the SPEC §1.3 evaluation cascade. The
/// three validation paths map these onto `ConfigError` variants with listener/
/// route context; the request path never sees them (`route_fraction_passes`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FractionGateError {
    /// CF-109-1 (WIDENED at ADR-0176): the consulted key's `final_value`
    /// parses as a finite f64 strictly between 0 and 100 — upstream samples
    /// per request (integer `50` GATED 27/33 over 60; float `1.5` GATED 1/40).
    NondeterministicValue { key: String, value: String },
    /// CF-109-2, the SNAPSHOT-PREFIX rule (SPEC §3 D3): some entry name starts
    /// with `"<key>."`, i.e. a map-shaped value was flattened at (or beside)
    /// the consulted key — a plain lookup would silently use `default_value`
    /// where upstream honors the map (pick cells 7-8).
    MapShapedKey { key: String },
    /// The `default_value` itself is non-deterministic: numerator neither `0`
    /// nor `== denominator.value()` (the house `selects_deterministic`
    /// discipline; upstream also accepts `>` — the recorded slightly-narrower
    /// divergence, parent SPEC §3 D2(a)).
    NondeterministicDefault { numerator: u32, denominator: u32 },
}
```

and inside `impl RuntimeSnapshot`:

```rust
    /// 109.1: the store's FIRST typed lookup — resolve a route
    /// `runtime_fraction` to its deterministic gate per the SPEC §1.3 cascade,
    /// MEASURED against envoyproxy/envoy:v1.33.0 (23 cells: parent §1.1 + the
    /// V-8 closure §1.2):
    ///
    /// 1. key consulted and any entry starts with `"<key>."` → `MapShapedKey`;
    /// 2. key consulted, present, `final_value` parses as finite f64 `v`:
    ///    `v == 0` → Never; `v >= 100` → Always; `0 < v < 100` →
    ///    `NondeterministicValue`; `v < 0` → fall through to the default;
    /// 3. key absent / unparseable / non-finite / not consulted → the
    ///    `default_value`: numerator `0` → Never, `== denominator.value()` →
    ///    Always, else `NondeterministicDefault`.
    ///
    /// An empty `runtime_key` string is treated as not-consulted (upstream
    /// unmeasured; the absent-like reading, recorded in the PLAN).
    pub fn route_fraction_gate(
        &self,
        rf: &crate::RuntimeFractionalPercent,
    ) -> Result<FractionGate, FractionGateError> {
        if let Some(key) = rf.runtime_key.as_deref().filter(|k| !k.is_empty()) {
            let prefix = format!("{key}.");
            if self
                .entries
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(name, _)| name.starts_with(&prefix))
            {
                return Err(FractionGateError::MapShapedKey {
                    key: key.to_string(),
                });
            }
            if let Some(entry) = self.entries.get(key)
                && let Ok(v) = entry.final_value.parse::<f64>()
                && v.is_finite()
            {
                if v == 0.0 {
                    return Ok(FractionGate::Never);
                }
                if v >= 100.0 {
                    return Ok(FractionGate::Always);
                }
                if v > 0.0 {
                    return Err(FractionGateError::NondeterministicValue {
                        key: key.to_string(),
                        value: entry.final_value.clone(),
                    });
                }
                // v < 0: MEASURED → default_value (cells N1/N2); fall through.
            }
            // Absent key, unparseable or non-finite value: default (cells
            // 1, 2, 10, 11, B1-B3); fall through.
        }
        let p = &rf.default_value;
        if p.numerator == 0 {
            Ok(FractionGate::Never)
        } else if p.numerator == p.denominator.value() {
            Ok(FractionGate::Always)
        } else {
            Err(FractionGateError::NondeterministicDefault {
                numerator: p.numerator,
                denominator: p.denominator.value(),
            })
        }
    }

    /// 109.1: infallible request-path wrapper over [`Self::route_fraction_gate`].
    /// The `Err` arm is VALIDATED-UNREACHABLE in production — all three error
    /// classes are boot-fatal at every validation path (boot, post-merge, RDS
    /// reload) — and deliberately does NOT panic (the rds_watcher
    /// `unreachable!()` lesson, 76.2 I-1): it falls back to the
    /// `default_value`'s sign, which is total and deterministic.
    pub fn route_fraction_passes(&self, rf: &crate::RuntimeFractionalPercent) -> bool {
        match self.route_fraction_gate(rf) {
            Ok(FractionGate::Always) => true,
            Ok(FractionGate::Never) => false,
            Err(_) => rf.default_value.numerator != 0,
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-config --lib -- runtime::tests 2>&1 | tee /tmp/t1-green.log`
Expected: PASS with the pre-task runtime test count + 2 (assert the exact `N passed`; derive the pre-task count first with the same filter).

- [ ] **Step 5: Workspace gates + commit**

Run: `cargo build --workspace --all-targets && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Then: `git add crates/envoy-config/src/runtime.rs && git commit -m "phase 109.1 task 1: RuntimeSnapshot::route_fraction_gate — the store's first typed lookup, pinned against all 23 measured cells"`

### Task 2: The wire field + the 100-site literal fan-out + the fuzz seed

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (the `RouteMatch` struct, located by the text `pub struct RouteMatch {`; plus its 2 test literals)
- Modify (mechanical, one line each): the 98 other `RouteMatch { … }` literals — 57 `crates/envoy-http1/src/hcm.rs`, 36 `crates/envoy-http2/src/hcm.rs`, 3 `crates/envoy-filter/src/jwt_authn.rs`, 1 `crates/envoy-filter/src/instance.rs`, 1 `crates/envoy-filter/src/types.rs`
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/runtime_fraction_route.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (one `!` line)
- Test: `crates/envoy-config/src/bootstrap.rs` tests

**Interfaces:**
- Produces: `RouteMatch.runtime_fraction: Option<RuntimeFractionalPercent>` — the field every later task reads. `deny_unknown_fields` retained; `Route`'s hand-written impls untouched (its `Deserialize` delegates via `map.next_value::<RouteMatch>()` and its `Serialize` via `serialize_entry("match", &self.r#match)`, so the derived `RouteMatch` impls carry the field automatically).

- [ ] **Step 1: Write the failing serde tests** (bootstrap.rs test module, near the existing jwt `RouteMatch` literal tests at the text `rules: vec![crate::RequirementRule {`):

```rust
    /// 109.1 Task 2: `match.runtime_fraction` parses (accept direction), stays
    /// optional, and deny_unknown_fields still rejects a misspelling.
    #[test]
    fn route_match_runtime_fraction_parses_and_stays_optional() {
        let m: crate::RouteMatch = serde_yaml::from_str(
            r#"
prefix: "/gated"
runtime_fraction:
  default_value: { numerator: 100, denominator: HUNDRED }
  runtime_key: gate.k
"#,
        )
        .expect("runtime_fraction must parse");
        let rf = m.runtime_fraction.expect("field present");
        assert_eq!(rf.default_value.numerator, 100);
        assert_eq!(rf.runtime_key.as_deref(), Some("gate.k"));

        // runtime_key stays optional inside the block.
        let m: crate::RouteMatch =
            serde_yaml::from_str("path: \"/x\"\nruntime_fraction:\n  default_value: { numerator: 0 }\n")
                .expect("keyless runtime_fraction must parse");
        assert!(m.runtime_fraction.unwrap().runtime_key.is_none());

        // Absent field → None (100 existing literals rely on this default).
        let m: crate::RouteMatch = serde_yaml::from_str("prefix: \"/\"\n").expect("bare match");
        assert!(m.runtime_fraction.is_none());

        // deny_unknown_fields is retained: a misspelling stays boot-fatal.
        assert!(
            serde_yaml::from_str::<crate::RouteMatch>("prefix: \"/\"\nruntime_fractoin: {}\n").is_err(),
            "unknown fields must still reject"
        );
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p envoy-config --lib -- route_match_runtime_fraction 2>&1 | tee /tmp/t2-red.log`
Expected: FAIL — `unknown field 'runtime_fraction'` (the parse) / `no field runtime_fraction` (compile) — the RED.

- [ ] **Step 3: Add the field** — in `pub struct RouteMatch` (after `headers`):

```rust
    /// 109.1 (ADR-0176): optional runtime-keyed fractional gate. Reuses the
    /// CSRF wire type `RuntimeFractionalPercent`. The gate is evaluated by
    /// `RuntimeSnapshot::route_fraction_gate` (the SPEC §1.3 cascade) and is
    /// deterministic-only: every nondeterministic input is boot-fatal
    /// (CF-109-1/2). Present inside `jwt_authn.rules[].match` it is boot-fatal
    /// (CF-109-3) — the hand-copied jwt matcher never reads it.
    #[serde(default)]
    pub runtime_fraction: Option<RuntimeFractionalPercent>,
```

Insert AFTER the existing `headers` field INSIDE the struct — do NOT insert between any doc comment and a `#[derive]` (the M-1 orphaning trap).

- [ ] **Step 4: The mechanical fan-out.** `cargo build --workspace --all-targets 2>&1 | grep -c E0063` — expect ~100 errors. Add `runtime_fraction: None,` as the LAST field line of every `RouteMatch { … }` literal (100 sites; per-file counts in the W-1 census above). Drive the site list from `git grep -n 'RouteMatch {' -- crates/ tests/` minus the struct def; do NOT hand-type paths. After the sweep: `cargo build --workspace --all-targets` must be CLEAN, and `git grep -c 'runtime_fraction: None' -- crates/` must return the per-file counts above (sum 100).

- [ ] **Step 5: The fuzz seed** (SPEC §3 D8). Create `crates/envoy-config/fuzz/corpus/parse_bootstrap/runtime_fraction_route.yaml`:

```yaml
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
layered_runtime:
  layers:
  - name: base
    static_layer:
      gate.k: 100
static_resources:
  listeners:
  - name: l
    address:
      socket_address: { address: 127.0.0.1, port_value: 10000 }
    filter_chains:
    - filters:
      - name: envoy.filters.network.http_connection_manager
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
          stat_prefix: ingress_http
          route_config:
            name: local_route
            virtual_hosts:
            - name: vh
              domains: ["*"]
              routes:
              - match:
                  prefix: "/gated"
                  runtime_fraction:
                    default_value: { numerator: 100, denominator: HUNDRED }
                    runtime_key: gate.k
                direct_response: { status: 200, body: { inline_string: ok } }
          http_filters:
          - name: envoy.filters.http.router
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
```

Append to `crates/envoy-config/fuzz/.gitignore`, directly after the existing `!corpus/parse_bootstrap/layered_runtime.yaml` line (BEFORE `artifacts/`): `!corpus/parse_bootstrap/runtime_fraction_route.yaml`. Verify tracking with `git add` + `git ls-files crates/envoy-config/fuzz/corpus/ | wc -l` = **66** (was 65); `.gitignore` = **69** lines / **66** `!` lines. (`git check-ignore -v` prints negation rules and exits 0 — only the PLAIN form's exit code answers; `git ls-files` is THE proof.)

- [ ] **Step 6: Run the tests to verify green**

Run: `cargo test -p envoy-config --lib -- route_match_runtime_fraction 2>&1` — expect `1 passed`.
Run: `cargo test --workspace --no-fail-fast 2>&1 > /tmp/t2-sweep.log` — the full suite must be green modulo the documented ADR-0164 host-flake core (five members, deterministic in isolation) + startup-race tail (passes in isolation); adjudicate any RED by ISOLATION, never by name.

- [ ] **Step 7: Gates + commit**

`cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all && cargo fmt --all -- --check`
`git add -A crates/ && git commit -m "phase 109.1 task 2: RouteMatch.runtime_fraction wire field + 100-site literal fan-out + parse_bootstrap corpus seed"`

### Task 3: Four `ConfigError` variants + the validator at boot & post-merge + the jwt reject

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (the `ConfigError` enum — append the four variants near the other `Unsupported*` runtime/csrf variants, located by the text `UnsupportedNonDeterministicCsrfFilterEnabled`)
- Modify: `crates/envoy-config/src/bootstrap.rs` — new `validate_route_runtime_fraction` fn; `validate()` builds the snapshot; `validate_hcm` gains a param and calls the validator in its route walk; `validate_jwt_authn_config` gains the presence check
- Test: `crates/envoy-config/src/bootstrap.rs` + `crates/envoy-config/src/lib.rs` tests

**Interfaces:**
- Consumes: Task 1's `route_fraction_gate`/`FractionGateError`; Task 2's field.
- Produces (Task 5 relies on the EXACT variant names):

```rust
    /// CF-109-1 (WIDENED, ADR-0176 D3): consulted runtime value strictly
    /// between 0 and 100 — upstream samples per request; envoy-rust is
    /// deterministic-only.
    #[error(
        "route runtime_fraction on `{listener}` route `{route}`: runtime key `{key}` resolves to `{value}`, strictly between 0 and 100 — upstream samples per request; envoy-rust supports only deterministic 0/>=100 values (CF-109-1)"
    )]
    UnsupportedNonDeterministicRuntimeFraction {
        listener: String,
        route: String,
        key: String,
        value: String,
    },
    /// CF-109-2 (ADR-0176 D3): map-shaped value at (or beside) a CONSULTED
    /// key — the store flattens maps to dotted keys, so a plain lookup would
    /// silently fall back to the default where upstream honors the map.
    #[error(
        "route runtime_fraction on `{listener}` route `{route}`: runtime key `{key}` carries a map-shaped (or dotted-sibling) value in the runtime snapshot — unsupported (CF-109-2)"
    )]
    UnsupportedMapShapedRuntimeKey {
        listener: String,
        route: String,
        key: String,
    },
    /// The runtime_fraction's own default_value is non-deterministic
    /// (numerator neither 0 nor the denominator value) — the house
    /// `selects_deterministic` discipline (CSRF/fault precedent).
    #[error(
        "route runtime_fraction on `{listener}` route `{route}`: default_value {numerator}/{denominator} is non-deterministic (numerator must be 0 or the denominator value)"
    )]
    UnsupportedNonDeterministicRuntimeFractionDefault {
        listener: String,
        route: String,
        numerator: u32,
        denominator: u32,
    },
    /// CF-109-3 (ADR-0176): `runtime_fraction` inside `jwt_authn.rules[].match`
    /// — the hand-copied jwt matcher would silently ignore it (upstream honors
    /// it there).
    #[error(
        "jwt_authn on listener `{listener}`: rules[].match.runtime_fraction is unsupported (CF-109-3) — the jwt requirement matcher does not evaluate runtime gates"
    )]
    UnsupportedRuntimeFractionInJwtRule { listener: String },
```

and `pub(crate) fn validate_route_runtime_fraction(m: &crate::RouteMatch, runtime: &crate::runtime::RuntimeSnapshot, listener: &str, route: &str) -> Result<(), crate::ConfigError>`.

- [ ] **Step 1: Write the failing tests** (bootstrap.rs test module; the yaml helper embeds a full bootstrap — model the admin+listener+HCM shape on the Task 2 fuzz seed yaml, with `layered_runtime` varying per case):

```rust
    /// 109.1 Task 3 helper: a full bootstrap whose single gated route consults
    /// `gate.k`; `layer_body` is the static_layer yaml fragment for the key
    /// (empty string = no layered_runtime block), `rf_yaml` the
    /// runtime_fraction block.
    fn runtime_fraction_bootstrap(layer_body: &str, rf_yaml: &str) -> String {
        let layered = if layer_body.is_empty() {
            String::new()
        } else {
            format!("layered_runtime:\n  layers:\n  - name: base\n    static_layer:\n{layer_body}")
        };
        format!(
            r#"admin:
  address:
    socket_address: {{ address: 127.0.0.1, port_value: 9901 }}
{layered}static_resources:
  listeners:
  - name: l0
    address:
      socket_address: {{ address: 127.0.0.1, port_value: 10000 }}
    filter_chains:
    - filters:
      - name: envoy.filters.network.http_connection_manager
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
          stat_prefix: ingress_http
          route_config:
            name: local_route
            virtual_hosts:
            - name: vh
              domains: ["*"]
              routes:
              - match:
                  prefix: "/gated"
{rf_yaml}                direct_response: {{ status: 200, body: {{ inline_string: ok }} }}
          http_filters:
          - name: envoy.filters.http.router
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#
        )
    }

    const RF_KEYED_100: &str = "                  runtime_fraction:\n                    default_value: { numerator: 100, denominator: HUNDRED }\n                    runtime_key: gate.k\n";

    /// 109.1 Task 3: the three boot-fatal classes at the BOOT path (path 1 of
    /// three), one accept-direction control per reject.
    #[test]
    fn boot_rejects_nondeterministic_map_shaped_and_bad_default_runtime_fractions() {
        // CF-109-1: consulted value strictly between 0 and 100 (int + widened float/string forms).
        for value in ["50", "0.5", "1.5", "\"0.5\""] {
            let yaml = runtime_fraction_bootstrap(&format!("      gate.k: {value}\n"), RF_KEYED_100);
            let err = crate::parse_bootstrap(&yaml).expect_err("nondeterministic value must be boot-fatal");
            assert!(
                matches!(err, crate::ConfigError::UnsupportedNonDeterministicRuntimeFraction { ref key, .. } if key == "gate.k"),
                "value {value}: got {err:?}"
            );
        }
        // CF-109-2: map-shaped consulted key (the snapshot-prefix rule).
        let yaml = runtime_fraction_bootstrap(
            "      gate.k:\n        numerator: 0\n        denominator: HUNDRED\n",
            RF_KEYED_100,
        );
        assert!(matches!(
            crate::parse_bootstrap(&yaml).expect_err("map-shaped consulted key must be boot-fatal"),
            crate::ConfigError::UnsupportedMapShapedRuntimeKey { ref key, .. } if key == "gate.k"
        ));
        // Non-deterministic default_value.
        let yaml = runtime_fraction_bootstrap(
            "",
            "                  runtime_fraction:\n                    default_value: { numerator: 50, denominator: HUNDRED }\n",
        );
        assert!(matches!(
            crate::parse_bootstrap(&yaml).expect_err("nondeterministic default must be boot-fatal"),
            crate::ConfigError::UnsupportedNonDeterministicRuntimeFractionDefault { numerator: 50, .. }
        ));
        // Accept-direction controls: deterministic values and defaults BOOT.
        for (layer, rf) in [
            ("      gate.k: 0\n", RF_KEYED_100),
            ("      gate.k: 100\n", RF_KEYED_100),
            ("      gate.k: abc\n", RF_KEYED_100), // unparseable -> default, deterministic
            ("", RF_KEYED_100),                    // absent key -> default
        ] {
            let yaml = runtime_fraction_bootstrap(layer, rf);
            crate::parse_bootstrap(&yaml).expect("deterministic runtime_fraction must be accepted");
        }
    }

    /// 109.1 Task 3 (CF-109-3): runtime_fraction inside jwt rules is boot-fatal.
    #[test]
    fn jwt_rule_with_runtime_fraction_is_rejected() {
        let mut cfg = jwt_cfg_with_rule(); // reuse the existing test helper that
        // builds a valid JwtAuthnConfig with one RequirementRule (located by the
        // text `rules: vec![crate::RequirementRule {` — if the existing tests
        // build inline, construct the same shape inline here).
        cfg.rules[0].r#match.runtime_fraction = Some(crate::RuntimeFractionalPercent {
            default_value: crate::FractionalPercent {
                numerator: 0,
                denominator: crate::DenominatorType::Hundred,
            },
            runtime_key: None,
        });
        assert!(matches!(
            validate_jwt_authn_config(&cfg, "l0").unwrap_err(),
            crate::ConfigError::UnsupportedRuntimeFractionInJwtRule { .. }
        ));
    }
```

For the jwt test: mirror the construction of the EXISTING passing test at the text `assert!(validate_jwt_authn_config(&cfg, "l0").is_ok());` (bootstrap.rs `:20307` at plan time) — clone its `cfg` construction, then set `runtime_fraction` on the rule. Do not invent a helper if none exists.

Add the POST-MERGE path witness in `lib.rs` tests (path 2 of three), modeled on the existing LDS `load_dynamic_resources` tests (tempfile is a dev-dependency; mirror the existing test file-writing shape in `lib.rs`/`lds.rs` tests — a bootstrap with `dynamic_resources.lds_config` pointing at a temp LDS file whose listener carries the SAME gated-route HCM with `gate.k: 50` in the STATIC bootstrap's `layered_runtime`):

```rust
    /// 109.1 Task 3, path 2 of 3: an LDS-delivered listener carrying a
    /// nondeterministic runtime_fraction is rejected by the post-merge
    /// validate() — same validator, same snapshot, defer-then-revalidate.
    #[test]
    fn load_dynamic_resources_rejects_lds_delivered_nondeterministic_runtime_fraction() {
        // (construct per the existing LDS test pattern; assert:)
        let err = load_dynamic_resources(&mut bootstrap).expect_err("post-merge must reject");
        assert!(matches!(
            err,
            ConfigError::UnsupportedNonDeterministicRuntimeFraction { .. }
        ));
    }
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p envoy-config --lib -- runtime_fraction 2>&1 | tee /tmp/t3-red.log`. Expected: compile errors on the new variant names — the RED.

- [ ] **Step 3: Implement.**
  1. Append the four variants to `ConfigError` (code block above, verbatim).
  2. In `bootstrap.rs`, add next to `validate_route_match_cardinality` (located by text):

```rust
/// 109.1: validate a route's optional `runtime_fraction` against the boot
/// runtime snapshot, mapping the lookup's error classes onto boot-fatal
/// `ConfigError`s. Shared by all THREE validation paths: boot `validate_hcm`,
/// post-merge `load_dynamic_resources` → `validate()`, and RDS reload
/// `reparse_and_select_route_config` (which passes the rds file path as
/// `listener` context, the `validate_redirect_oneofs` precedent).
pub(crate) fn validate_route_runtime_fraction(
    m: &crate::RouteMatch,
    runtime: &crate::runtime::RuntimeSnapshot,
    listener: &str,
    route: &str,
) -> Result<(), crate::ConfigError> {
    use crate::runtime::FractionGateError;
    let Some(rf) = &m.runtime_fraction else {
        return Ok(());
    };
    match runtime.route_fraction_gate(rf) {
        Ok(_) => Ok(()),
        Err(FractionGateError::NondeterministicValue { key, value }) => {
            Err(crate::ConfigError::UnsupportedNonDeterministicRuntimeFraction {
                listener: listener.to_string(),
                route: route.to_string(),
                key,
                value,
            })
        }
        Err(FractionGateError::MapShapedKey { key }) => {
            Err(crate::ConfigError::UnsupportedMapShapedRuntimeKey {
                listener: listener.to_string(),
                route: route.to_string(),
                key,
            })
        }
        Err(FractionGateError::NondeterministicDefault {
            numerator,
            denominator,
        }) => Err(
            crate::ConfigError::UnsupportedNonDeterministicRuntimeFractionDefault {
                listener: listener.to_string(),
                route: route.to_string(),
                numerator,
                denominator,
            },
        ),
    }
}
```

  3. `validate_hcm` gains a parameter `runtime: &crate::runtime::RuntimeSnapshot` (append after `defer_cluster_refs`). In its route walk — the text `validate_route_match_cardinality(r.r#match.prefix.is_some(), r.r#match.path.is_some())?;` (`:4308` at plan time) — add directly after that line:

```rust
            validate_route_runtime_fraction(&r.r#match, runtime, listener_name, &r.name)?;
```

  4. In `validate()` (the fn at the text `pub(crate) fn validate(bootstrap: &mut Bootstrap)`), build the snapshot ONCE after the early listener-count/no-runtime gates and BEFORE the listener walk:

```rust
    // 109.1: the boot runtime snapshot, built once for the whole validation
    // walk. `from_bootstrap` is total; validators never mutate
    // `layered_runtime`, so the snapshot stays accurate across the walk.
    let runtime_snapshot = crate::runtime::RuntimeSnapshot::from_bootstrap(bootstrap);
```

     and pass `&runtime_snapshot` at the single `validate_hcm(` call site (the text `validate_hcm(` at `:3912`). NOTE: `from_bootstrap(&*bootstrap)` takes an immutable borrow that ENDS at the statement, before the `&mut` walk — no borrow conflict. `validate()`'s many direct test callers (`validate(&mut b)`) need NO change (the snapshot is built internally).
  5. `validate_jwt_authn_config`: in the `for rule in &cfg.rules` walk, directly after the `validate_route_match_cardinality(` call (the text at `:4796`):

```rust
        // 109.1 (CF-109-3): the hand-copied jwt matcher never evaluates
        // runtime gates — a present runtime_fraction here would be silently
        // inert, the exact ADR-0049 divergence class. Boot-fatal instead.
        if rule.r#match.runtime_fraction.is_some() {
            return Err(crate::ConfigError::UnsupportedRuntimeFractionInJwtRule {
                listener: listener_name.to_string(),
            });
        }
```

- [ ] **Step 4: Run the tests** — `cargo test -p envoy-config --lib -- runtime_fraction 2>&1` and the jwt/lds filters; assert the exact new-test pass counts. Then `cargo test -p envoy-config --lib` (whole crate — the existing validator tests must be untouched).

- [ ] **Step 5: Workspace gates + commit** — build/clippy/fmt as in Global Constraints (validate_hcm is `fn`-private to envoy-config; the workspace build catches nothing new here, run it anyway). Commit: `git add crates/envoy-config/ && git commit -m "phase 109.1 task 3: three boot-fatal runtime_fraction validators at boot + post-merge, jwt CF-109-3 reject [4 new ConfigError variants]"`

### Task 4: The threading seam — `HCMConfig.runtime` + `from_config` parameter (behavior-neutral)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (`HCMConfig` struct + `from_config`; 39 test literals; 8 test call sites)
- Modify: `crates/envoy-http1/src/rds_watcher.rs` (2 test literals)
- Modify: `crates/envoy-http2/src/hcm.rs` (32 test call sites; the H2 wrapper literal at `:4871` gains NOTHING)
- Modify: `crates/envoy-admin/src/endpoint.rs` (1 test call site)
- Modify: `crates/envoy-bin/src/main.rs` (build the snapshot once; 3 production call sites)

**Interfaces:**
- Consumes: `envoy_config::runtime::RuntimeSnapshot` (Task 1 file, landed 108.1 type).
- Produces: `HCMConfig.runtime: Arc<RuntimeSnapshot>` (public field) and `from_config(cfg, cluster_mgr, registry, pool_mgr, runtime: Arc<envoy_config::runtime::RuntimeSnapshot>)` — the 5th parameter, appended LAST. Task 6 reads `config.runtime`. Behavior-neutral: nothing reads the field yet.

- [ ] **Step 1: Add the field and parameter (the compiler is the RED).** In `HCMConfig` (after `pool_mgr`):

```rust
    /// 109.1 (ADR-0176 D4): the boot runtime snapshot, built ONCE per proxy
    /// boot (`RuntimeSnapshot::from_bootstrap` in envoy-bin) and shared by
    /// Arc-clone. Read by `route_matches` to evaluate
    /// `RouteMatch.runtime_fraction` gates; `RuntimeSnapshot::default()` (the
    /// empty snapshot) makes every lookup fall back to `default_value`, which
    /// is exactly the no-`layered_runtime` semantics — the right value for
    /// test literals.
    pub runtime: Arc<envoy_config::runtime::RuntimeSnapshot>,
```

`from_config` gains the 5th parameter `runtime: Arc<envoy_config::runtime::RuntimeSnapshot>` and sets `runtime` in the `Ok(HCMConfig { … })` it builds (locate the constructor's struct literal inside `from_config` — it is one of the 39).

- [ ] **Step 2: Run `cargo build --workspace --all-targets 2>&1 | tee /tmp/t4-red.log`** — expect E0063 at every `HCMConfig` literal + E0061 at every `from_config` call site. This IS the census check: the error sites must match the W-1 counts (41 literals incl. the one inside `from_config`, 44 call sites).

- [ ] **Step 3: The mechanical fan-out.**
  - **Literals (40 remaining):** add `runtime: Arc::new(RuntimeSnapshot::default()),` (with `use envoy_config::runtime::RuntimeSnapshot;` added ONCE to `crates/envoy-http1/src/hcm.rs`'s non-test imports — production code needs it — and to `rds_watcher.rs`'s test-module imports). ONE spelling everywhere; no per-site variation.
  - **Test call sites (41):** append `Arc::new(RuntimeSnapshot::default())` as the 5th argument — 8 in `envoy-http1/src/hcm.rs`, 32 in `envoy-http2/src/hcm.rs` (add `use envoy_config::runtime::RuntimeSnapshot;` to its test-module imports), 1 in `envoy-admin/src/endpoint.rs` (spell it `envoy_config::runtime::RuntimeSnapshot::default()` there if no import exists).
  - **Production call sites (3):** in `crates/envoy-bin/src/main.rs`, immediately after `let bootstrap = std::sync::Arc::new(bootstrap);` (the text at `:58`):

```rust
    // 109.1: the boot runtime snapshot, built ONCE and Arc-shared into every
    // HCMConfig (route runtime_fraction gates read it; admin /runtime keeps
    // its own per-request rebuild, deliberately untouched).
    let runtime_snapshot = std::sync::Arc::new(
        envoy_config::runtime::RuntimeSnapshot::from_bootstrap(&bootstrap),
    );
```

    and pass `std::sync::Arc::clone(&runtime_snapshot)` as the 5th argument at the three `envoy_http1::HCMConfig::from_config(` sites (`:480`, `:541`, `:618` at plan time — locate by the call text).

- [ ] **Step 4: Verify green + behavior-neutrality.** `cargo build --workspace --all-targets` clean; `cargo test --workspace --no-fail-fast 2>&1 > /tmp/t4-sweep.log` — the ENTIRE existing suite is the witness that the seam is behavior-neutral (adjudicate REDs by isolation per ADR-0164). `git grep -c 'runtime: Arc::new(RuntimeSnapshot::default())'` returns 40-41 per the census.

- [ ] **Step 5: Gates + commit** — clippy/fmt per Global Constraints. `git add -A crates/ && git commit -m "phase 109.1 task 4: HCMConfig.runtime seam — snapshot built once in envoy-bin, threaded through from_config (behavior-neutral)"`

### Task 5: The RDS reload path — `reparse` widening + the classifier extension (classifier test FIRST)

**Files:**
- Test FIRST: `crates/envoy-http1/src/rds_watcher.rs` (reload-level classifier test)
- Modify: `crates/envoy-config/src/rds.rs` (`reparse_and_select_route_config` signature + walk; its 8 test call sites)
- Modify: `crates/envoy-http1/src/rds_watcher.rs` (the production call passes the store's snapshot; the classifier's `update_rejected` arm + comment)

**Interfaces:**
- Consumes: Task 3's `validate_route_runtime_fraction` + variants; Task 4's `HCMConfig.runtime`.
- Produces: `reparse_and_select_route_config(path, route_config_name, known_cluster, runtime: &crate::runtime::RuntimeSnapshot)` — the 4th parameter; the classifier handles NINE variants.

- [ ] **Step 1 (BEFORE any widening — the W-2 discipline): write the failing reload test** in `rds_watcher.rs`'s test module, modeled on the existing reload tests (the harness at the text `let store = Arc::new(HCMConfig {`). The store's `runtime` field carries a snapshot with `gate.k` = `"50"` (build via `envoy_config::runtime::RuntimeSnapshot::from_layers` with one yaml layer, mirroring `runtime.rs` test helpers); the RDS file body is the existing `rds_body`-style yaml with the route match widened:

```rust
    /// 109.1 Task 5 (the 76.2 I-1 regression class, closed BEFORE it can
    /// open): an RDS reload delivering a route whose runtime_fraction
    /// resolves nondeterministically against the boot snapshot must be
    /// warm-REJECTED — Err + update_rejected ticked + live table untouched —
    /// and must NOT hit the classifier's `unreachable!()` abort arm.
    #[tokio::test]
    async fn reload_warm_rejects_nondeterministic_runtime_fraction() {
        // Build the standard reload harness (mirror the existing
        // reload_success test's store/counters/watch-target setup) with ONE
        // change: `runtime` carries gate.k = "50".
        // RDS body: a route matching `{ prefix: "/", runtime_fraction:
        //   { default_value: { numerator: 100, denominator: HUNDRED },
        //     runtime_key: gate.k } }` to a known cluster.
        let result = reload(&target); // the existing reload entry point
        assert!(
            matches!(
                result,
                Err(envoy_config::ConfigError::UnsupportedNonDeterministicRuntimeFraction { .. })
            ),
            "got {result:?}"
        );
        // update_rejected ticked, update_failure NOT, live table untouched:
        // assert per the existing reload-reject tests' counter/table pattern.
    }
```

  (Transcribe the harness lines from the adjacent existing tests verbatim — this plan deliberately does not duplicate the ~40-line harness; the test's ASSERTIONS above are the contract.)

- [ ] **Step 2: Run to verify the RIGHT failure** — `cargo test -p envoy-http1 --lib -- reload_warm_rejects_nondeterministic 2>&1 | tee /tmp/t5-red.log`. Expected RED: the reload SUCCEEDS (`Ok`) — after Task 2 the field parses and NOTHING on the reparse path validates it. (A compile error is NOT the RED sought here; fix compile errors first, then confirm the assertion-level failure.)

- [ ] **Step 3: Widen `reparse_and_select_route_config`** — 4th parameter `runtime: &crate::runtime::RuntimeSnapshot`; inside the vh/route walk (the exhaustive `match &route.action` block), add BEFORE the action match, mirroring the `validate_redirect_oneofs` context convention (path string as `listener` context):

```rust
            // 109.1: the SAME runtime_fraction validators as boot, applied
            // against the BOOT snapshot (runtime state never mutates
            // post-boot in this tree). A warm config must not install a gate
            // the byte-identical boot config would reject.
            crate::bootstrap::validate_route_runtime_fraction(
                &route.r#match,
                runtime,
                &path_str,
                &route.name,
            )?;
```

  Update the 8 `rds.rs` test call sites: append `&crate::runtime::RuntimeSnapshot::default()` (and for the happy-path test keep it green — the default snapshot + deterministic-default routes accept). Add 3 unit tests in `rds.rs` pinning each new variant through `reparse` (nondeterministic value / map-shaped key via a snapshot built `from_layers`; nondeterministic default with the default snapshot).

- [ ] **Step 4: Extend the classifier.** In `rds_watcher.rs`: production call passes `&target.store.runtime` as the 4th argument; the `update_rejected` arm gains the THREE reparse-returnable variants:

```rust
                | envoy_config::ConfigError::UnsupportedNonDeterministicRuntimeFraction { .. }
                | envoy_config::ConfigError::UnsupportedMapShapedRuntimeKey { .. }
                | envoy_config::ConfigError::UnsupportedNonDeterministicRuntimeFractionDefault { .. }
```

  and the classifier's "ONLY the six variants" comment is updated to NINE (name them). `UnsupportedRuntimeFractionInJwtRule` is NOT added — `reparse` cannot return it (RDS route configs carry no jwt rules); the comment records that exclusion.

- [ ] **Step 5: Run the tests** — the Step-1 test + the 3 rds.rs tests now PASS (assert exact counts); the existing reload tests stay green: `cargo test -p envoy-http1 --lib -- rds_watcher 2>&1` and `cargo test -p envoy-config --lib -- rds:: 2>&1`.

- [ ] **Step 6: Gates + commit** — workspace build/clippy/fmt. `git add crates/envoy-config/src/rds.rs crates/envoy-http1/src/rds_watcher.rs && git commit -m "phase 109.1 task 5: RDS reload path validates runtime_fraction against the boot snapshot; reload classifier extended to nine variants (abort trap closed)"`

### Task 6: The LIVE gate at both `route_matches` call sites + the H2 inheritance witness

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (`route_matches`, `resolve_route_in`, `build_response_in`, the two public wrappers, the keep-alive loop's two `_in` call sites, the 6 `route_matches` test call sites)
- Test: `crates/envoy-http1/src/hcm.rs` + `crates/envoy-http2/src/hcm.rs`

**Interfaces:**
- Consumes: Task 1's `route_fraction_passes`; Task 4's `config.runtime`.
- Produces: the live gate. Public `resolve_route(config, req)` / `build_response(config, req, close)` signatures UNCHANGED — H2 (`envoy-http2/src/hcm.rs:475` `resolve_route(&config.inner, &envoy_req)`) needs ZERO edits.

- [ ] **Step 1: Write the failing tests.** In `hcm.rs`'s test module (mirror `resolve_route_test_config`/`make_req`):

```rust
    /// 109.1 Task 6 helper: an HCMConfig whose table carries a GATED
    /// direct_response route (`/`-prefix, runtime_fraction default 100/HUNDRED
    /// consulting gate.k, body "gated") ABOVE a bare catch-all
    /// (`/`-prefix, body "fallback"), with `runtime` built from one yaml layer
    /// mapping gate.k to `value` (None = empty snapshot).
    async fn gated_route_test_config(value: Option<&str>) -> HCMConfig {
        let runtime = match value {
            None => Arc::new(RuntimeSnapshot::default()),
            Some(v) => {
                let layer: envoy_config::RuntimeLayer = serde_yaml::from_str(&format!(
                    "name: l\nstatic_layer:\n  gate.k: {v}\n"
                ))
                .expect("layer");
                Arc::new(RuntimeSnapshot::from_layers(vec!["l".to_string()], &[layer]))
            }
        };
        // ...struct literal exactly as `resolve_route_test_config`, except:
        // `runtime`, and TWO routes — the gated one (runtime_fraction
        // default_value numerator 100 HUNDRED, runtime_key "gate.k",
        // direct_response body "gated") above the bare catch-all
        // (direct_response body "fallback").
    }

    /// The gate at call site 1 of 2 (`resolve_route_in`, hcm.rs `.position(`):
    /// key "0" -> the gated route NEVER matches; first-match-wins falls to the
    /// catch-all. Key "100" -> the gated route matches.
    #[tokio::test]
    async fn resolve_route_honors_runtime_fraction_gate() {
        let config = gated_route_test_config(Some("0")).await;
        let req = make_req("/x", "localhost");
        let r = resolve_route(&config, &req).expect("catch-all resolves");
        assert!(
            matches!(&r.route().action, RouteAction::DirectResponse(dr) if dr.body.inline_string.as_deref() == Some("fallback")),
            "key 0 must skip the gated route"
        );
        let config = gated_route_test_config(Some("100")).await;
        let r = resolve_route(&config, &req).expect("gated resolves");
        assert!(
            matches!(&r.route().action, RouteAction::DirectResponse(dr) if dr.body.inline_string.as_deref() == Some("gated")),
            "key 100 must match the gated route"
        );
        // Absent key -> default_value (numerator 100) -> gated.
        let config = gated_route_test_config(None).await;
        let r = resolve_route(&config, &req).expect("resolves");
        assert!(
            matches!(&r.route().action, RouteAction::DirectResponse(dr) if dr.body.inline_string.as_deref() == Some("gated")),
            "absent key must honor default_value 100"
        );
    }

    /// The gate at call site 2 of 2 (`build_response_in`, hcm.rs `.find(`):
    /// the SAME table through build_response — the documented resolve/build
    /// equivalence (hcm.rs "the 30-fixture regression-equivalence guarantee")
    /// now includes the gate.
    #[tokio::test]
    async fn build_response_honors_runtime_fraction_gate() {
        let config = gated_route_test_config(Some("0")).await;
        let mut req = make_req("/x", "localhost");
        match build_response(&config, &mut req, true) {
            BuildOutcome::Synth(resp, _) => assert!(
                String::from_utf8_lossy(&resp).contains("fallback"),
                "key 0 must serve the catch-all body"
            ),
            other => panic!("expected Synth, got {other:?}"),
        }
        let config = gated_route_test_config(Some("100")).await;
        let mut req = make_req("/x", "localhost");
        match build_response(&config, &mut req, true) {
            BuildOutcome::Synth(resp, _) => assert!(
                String::from_utf8_lossy(&resp).contains("gated"),
                "key 100 must serve the gated body"
            ),
            other => panic!("expected Synth, got {other:?}"),
        }
    }
```

  (Adjust the `BuildOutcome::Synth` destructuring to the actual variant shape at the existing direct_response tests — locate by the text `BuildOutcome::Synth` and mirror; the assertion contract is the two bodies.)

  In `envoy-http2/src/hcm.rs`'s test module, the inheritance witness — H2's OWN call path (`resolve_route(&config.inner, …)`) with a from_config-built inner carrying a real snapshot:

```rust
    /// 109.1: H2 inherits the runtime_fraction gate through the SHARED
    /// resolver with ZERO H2 edits — `resolve_route`'s public signature is
    /// unchanged and the gate lives inside it. Key "0" in the snapshot skips
    /// the gated route for an H2-resolved request exactly as for H1.
    #[tokio::test]
    async fn h2_inherits_runtime_fraction_gate_via_shared_resolver() {
        // Build cfg: the standard HttpConnectionManagerConfig used by the
        // existing H2 tests (route_config with TWO direct_response routes —
        // gated "/"-prefix consulting gate.k above a bare catch-all), then:
        let layer: envoy_config::RuntimeLayer =
            serde_yaml::from_str("name: l\nstatic_layer:\n  gate.k: 0\n").expect("layer");
        let runtime = Arc::new(RuntimeSnapshot::from_layers(vec!["l".to_string()], &[layer]));
        let built = Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None, runtime)
            .await
            .expect("build");
        let inner = Arc::new(built);
        // The EXACT call H2 production makes (envoy-http2/src/hcm.rs, the text
        // `envoy_http1::hcm::resolve_route(&config.inner, &envoy_req)`):
        let req = /* an envoy_http1::Request for GET /x with Host — mirror the
                     H1 make_req shape */;
        let r = envoy_http1::hcm::resolve_route(&inner, &req).expect("resolves");
        assert!(
            matches!(&r.route().action,
                envoy_config::RouteAction::DirectResponse(dr)
                    if dr.body.inline_string.as_deref() == Some("fallback")),
            "H2's resolver call must honor the gate (key 0 -> catch-all)"
        );
    }
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p envoy-http1 --lib -- runtime_fraction_gate 2>&1 | tee /tmp/t6-red.log`. Expected: with the helper built but the gate NOT yet wired, the key-"0" assertions FAIL (the gated route still matches) — a true behavioral RED. (Compile errors from the helper come first; resolve them, then confirm the assertion RED.)

- [ ] **Step 3: Wire the gate.**
  1. `route_matches` (the text `fn route_matches(r: &Route, path: &str, headers: &[(String, String)]) -> bool`) gains `runtime: &envoy_config::runtime::RuntimeSnapshot` and opens with:

```rust
    // 109.1: the runtime_fraction gate, evaluated FIRST (upstream AND-combines
    // it with the path/header criteria; order is behavior-neutral for an AND).
    // `route_fraction_passes` is infallible — every nondeterministic input is
    // boot-fatal at all three validation paths, so the request path never
    // sees an error.
    if let Some(rf) = &r.r#match.runtime_fraction
        && !runtime.route_fraction_passes(rf)
    {
        return false;
    }
```

  2. `resolve_route_in` and `build_response_in` gain `runtime: &envoy_config::runtime::RuntimeSnapshot` (appended last) and pass it at their `route_matches(` calls (the `.position(` at `:2028` and `.find(` at `:2094`).
  3. The public wrappers pass `&config.runtime`: `resolve_route_in(&config.current_route_config(), req, &config.runtime)` and the same for `build_response`.
  4. The keep-alive loop's two direct `_in` call sites (the texts `resolve_route_in(&route_snapshot, &req)` at `:875` and `build_response_in(&route_snapshot, &mut req, close)` at `:919`) pass `&config.runtime` — `config` is in scope at both.
  5. The 6 `route_matches` test call sites (`:10342-10368`) gain `&RuntimeSnapshot::default()` as the 4th argument.

- [ ] **Step 4: Run the tests** — the three new tests PASS; the whole crate stays green: `cargo test -p envoy-http1 --lib 2>&1 > /tmp/t6-h1.log`, `cargo test -p envoy-http2 --lib 2>&1 > /tmp/t6-h2.log` (assert counts), then the full `cargo test --workspace --no-fail-fast 2>&1 > /tmp/t6-sweep.log` (`-p` green is meaningless alone — Global Constraints).

- [ ] **Step 5: Gates + commit** — build/clippy/fmt. `git add crates/envoy-http1/ crates/envoy-http2/ && git commit -m "phase 109.1 task 6: runtime_fraction gate LIVE at both route_matches call sites; H2 inherits via the shared resolver (zero H2 production edits)"`

### Task 7: The D7 absence-assertion narrowing + final gates

**Files:**
- Modify: `crates/envoy-config/src/runtime.rs` (module doc, the text `**Nothing reads this store yet.**`)
- Modify: `crates/envoy-bin/src/runtime_stats.rs` (the consumer-absence wording near `:15-17`, located by text)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (ONE sentence — the text `Nothing READS the runtime store for behavior yet` at `:3168-3171` at plan time)

- [ ] **Step 1: Narrow all three in place** (D7 — this slice falsifies them, so this slice fixes them; the full `## Runtime` consumer subsection stays 109.2's). Each narrows to the same fact, adapted to its house wording: *the ROUTE `runtime_fraction` consumer is live as of 109.1 (`RuntimeSnapshot::route_fraction_gate`, evaluated inside `route_matches`); the `RuntimeUInt32` (`status_code_filter`) and CSRF consumers and RTDS remain unbuilt, so every remaining "no runtime CONSUMER for this key" assertion (incl. the test `runtime_key_is_rtds_inert`) stays true.* Do NOT edit `runtime_key_is_rtds_inert`, the CSRF rejects, any fixture, or any landed artifact.

- [ ] **Step 2: Verify nothing structural broke** — `git grep -n 'Nothing reads this store yet\|Nothing READS the runtime store' crates/ docs/envoy-rust/BEHAVIOR_CONTRACT.md` returns ZERO hits of the OLD absolute wording.

- [ ] **Step 3: The full task-level exit gate** (state 4 owns the formal §7.5 sweep; this is the state-3 exit bar): `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` (non-zero `Checking` count), `cargo fmt --all -- --check`, `cargo test --workspace --no-fail-fast 2>&1 > /tmp/t7-sweep.log` run 2×, diffing the failing SET (expect: the ADR-0164 five-member core + a startup-race tail, all isolation-classified; the identity `local passed + failed == 2180 + <new test count>` must close), `cargo deny check` (gate on exit code + the `advisories ok, bans ok, licenses ok, sources ok` line).

- [ ] **Step 4: Commit** — `git add -A crates/ docs/envoy-rust/BEHAVIOR_CONTRACT.md && git commit -m "phase 109.1 task 7: narrow the three consumer-absence assertions (D7) — route runtime_fraction consumer is live"`

---

## Self-review (run at plan-write, recorded)

1. **Spec coverage:** D1 → Task 2; D2 → Task 1; D3 (snapshot-prefix) → Task 1 (+Task 3 mapping); D4 → Tasks 4+6; D5 (jwt) → Task 3; D6 (three paths) → Tasks 3 (boot + post-merge) + 5 (RDS + classifier); D7 → Task 7; D8 (seed) → Task 2. §3 "Also in scope" (unit+mutation tests, gate at both call sites, H2 witness, classifier test) → Tasks 1/5/6. §5 (no fixture) honored. No gap found.
2. **Placeholder scan:** Task 5 step 1 and Task 6's H2 witness deliberately reference the ADJACENT existing test harnesses ("mirror the existing reload test" / "the standard H2 cfg") rather than duplicating ~40-line setup blocks — the assertions, types and call shapes are given in full; the harness lines exist on disk at the named texts. No TBD/TODO remains.
3. **Type consistency:** `FractionGate`/`FractionGateError`/`route_fraction_gate`/`route_fraction_passes` (Task 1) are consumed by those exact names in Tasks 3/6; the four `ConfigError` variant names in Task 3 match Task 5's classifier arms verbatim; `from_config`'s 5-arg shape (Task 4) matches Task 6's H2 witness call.
4. **Known risks, priced:** (a) exact `BuildOutcome::Synth` payload shape — Task 6 instructs mirroring the on-disk variant; (b) the `validate()` borrow sequencing (immutable `from_bootstrap` before the `&mut` walk) — compiles because the borrow ends at the statement; (c) in-process tests pinning exact route JSON may surface at Task 2's sweep — update mechanically to include the field (recorded in the plan-verify serialization note).
