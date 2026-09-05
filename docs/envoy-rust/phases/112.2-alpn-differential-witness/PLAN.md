# Phase 112.2 — the ALPN differential witness + the contract section: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`
> (recommended) or `superpowers:executing-plans` to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD per
> `superpowers:test-driven-development` (D-3.1).

**Goal:** Witness TLS ALPN negotiation differentially — cross-proxy, on all six
measured cells — by teaching the existing TLS drivers to OFFER an ALPN list and
ASSERT what the handshake selected, then landing two new fixtures, one changed
fixture, one runner and the `BEHAVIOR_CONTRACT.md` ALPN section.

**Architecture:** No new harness driver. `Driver::TlsTcpProbeList` already drives
a SEQUENCE of independent TLS handshakes against one listener, which is exactly
what four client offers against one server list need; `Driver::TlsTcp` gains the
same two fields so the no-ALPN control can ride on the pre-existing
`0004-tls-downstream` without changing driver kind. The negotiated protocol is
read from `tls.get_ref().1.alpn_protocol()` — the same completed-handshake value
`drive_tls` already reaches into for `expected_cn`. Every new field is
`#[serde(default)]`, so all 90 pre-existing fixtures parse unchanged.

**Tech Stack:** Rust 2024, `rustls 0.23.39` + `tokio-rustls 0.26.4` (both already
direct dependencies of the `differential` crate; **no manifest change**),
`serde_yaml 0.9`, `testcontainers 0.23`, upstream Envoy `v1.33.0` via Docker.

**Spec:** `docs/envoy-rust/phases/112.2-alpn-differential-witness/SPEC.md`
(371 lines, LANDED AND UNEDITABLE). Parent:
`docs/envoy-rust/phases/112-tls-alpn-negotiation/SPEC.md` (548). Sibling
foundation: `docs/envoy-rust/phases/112.1-alpn-config-and-rustls-wiring/`
(all four artifacts landed and uneditable).

---

## Global Constraints

Every task's requirements implicitly include this section.

- **No change to `crates/`.** SPEC §5 non-goal 1: the config surface and the
  `rustls` wiring are sibling `112.1`'s entire scope. **If a task appears to need
  a crate change, that is a signal `112.1` landed incomplete — raise it, do not
  absorb it.** This plan was prototyped end-to-end and needs none.
- **No new dependency and no `Cargo.toml` edit.** `rustls` and `tokio-rustls` are
  already `[dependencies]` of `tests/differential/Cargo.toml`.
- **`#![forbid(unsafe_code)]`** stays (invariant 4.1.8). No `unsafe`.
- **Every new serde field is `#[serde(default)]`.** `Driver` carries
  `#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]`
  (`tests/differential/src/lib.rs:38`) and `TlsTcpProbe` carries
  `#[serde(deny_unknown_fields)]` (`:732`), so **the fields must exist in Rust
  BEFORE any fixture YAML may name them.** Task 1 therefore precedes every
  fixture task, and no task may reorder around it.
- **Both fixtures stay `tcp_proxy`, zero HCM** (SPEC §3 E4). An HCM listener
  would collide with `ConfigError::Http2OverTlsNotSupported` the moment `h2` is
  negotiated — the interaction the phase explicitly declines (**CF-112-1**).
- **Both fixtures MUST point `tcp_proxy` at a reachable backend** (SPEC §3 E6).
  With an unreachable upstream, upstream Envoy's ALPN cells go
  **non-deterministic** — teardown races the handshake and a random cell returns
  `No ALPN negotiated` on roughly 1 in 4 sequences. Copying `0004`'s
  `{{BACKEND_HOST}}`/`{{BACKEND_PORT}}` cluster satisfies this. **Do not
  "simplify" either fixture into a backend-free one.**
- **No `""` element in any CLUSTER `alpn_protocols` list.** **CF-112-9**: an
  empty element, which `112.1`'s D4′ deliberately accepts, reaches
  `ProtocolName::from` → `PayloadU8::<NonEmpty>::new`'s
  `debug_assert!(bytes.len() >= 1)` in `rustls 0.23.39`
  (`src/msgs/base.rs:169-172`) on the UPSTREAM connect. `Cargo.toml` has no
  `[profile.dev]` override, so debug-assertions are ON for `cargo test` — which
  is what the differential harness runs. No fixture in this plan configures one;
  keep it that way. (The downstream side is inert: a zero-length name is
  rejected at wire decode, so an empty server element can never be selected.)
- **Fix no carry-forward** (SPEC §5 non-goal 7; §6.3; `ADR-0165` — a phase banks,
  it never clears). See "Carry-forward decisions" below for what that means for
  the three that land on this surface.
- **Landed artifacts are UNEDITABLE**, including this sub-phase's own `SPEC.md`
  and all four `112.1` artifacts. Fixtures are NOT landed artifacts — editing
  `0004-tls-downstream` is in scope and deliberate (SPEC §2.3; precedent
  `4e8956f` modified that exact file long after phase 03.1 created it).
- **Commit per task**, message `phase 112.2 task N: <what>`.

---

## §6.1 SPLIT GATE — ADJUDICATED ON A RE-DERIVED, MEASURED ESTIMATE

**Verdict: the gate does NOT fire. `112.2` is NOT split further.**

`SKILL_ROUTING.md` state 2 / `BOOTSTRAP_PROMPT.md` §6.1: split if `PLAN.md`
exceeds **~25 numbered tasks** OR **~1500 lines of net code change**. This plan
has **7 tasks** and a **measured 596** net code lines. Both legs are clear by a
wide margin, on both the task axis and the LoC axis.

### The estimate is MEASURED, not projected

`ADR-0185` built a complete prototype in a scratch worktree before sizing
`112.1`; that estimate landed **within 0.4%** (551 predicted, 549 landed) against
three prior slices whose PROJECTED estimates ran 1.33×, 1.41× and 1.66× under.
This session did the same: **every row below was built, compiled,
`clippy -D warnings`-cleaned, `rustfmt`-checked and exercised** in a scratch
worktree branched from `main` at `2fcc651`, then measured with
`git diff --cached --numstat`. The worktree was removed and the repository tree
was `git status --porcelain`-clean before and after.

| file | work | net LoC | how obtained |
|---|---|---|---|
| `tests/differential/src/lib.rs` | `AlpnRule`; `client_alpn` + `expected_alpn` on `TlsTcpProbe` and `Driver::TlsTcp`; `check_alpn`; `drive_tls` + `drive_tls_probes` offer plumbing and assertion; `run_tls_tcp_arm` threading; the dispatch arm; 7 tests | **263** (+274 −11) | **MEASURED** |
| `tests/differential/tests/tls_alpn.rs` | runner, two `#[tokio::test]`s | **32** | **MEASURED** |
| `tests/fixtures/0091-tls-alpn/` | `envoy.yaml` 49, `envoy-rust.yaml` 42, `expectations.yaml` 31, `README.md` 43, `inputs/payload.bin` 1 | **166** | **MEASURED** |
| `tests/fixtures/0092-tls-alpn-server-preference/` | `envoy.yaml` 49, `envoy-rust.yaml` 42, `expectations.yaml` 14, `README.md` 23, `inputs/payload.bin` 1 | **129** | **MEASURED** |
| `tests/fixtures/0004-tls-downstream/expectations.yaml` | cell 6 | **5** | **MEASURED** |
| **code subtotal** | | **595** | summed mechanically: 263+32+166+129+5 = 595 ✓ |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | new `## ALPN` section | **116** | **MEASURED** (drafted in full; the text is Task 7) |
| **total incl. docs** | | **711** | 595 + 116 = 711 ✓ |

**The subtotal was re-summed mechanically rather than asserted**, because
`ADR-0184` fired over a parent §9 table that summed to 795 against a stated ≈573.
Both totals above check.

### Calibration, re-derived at this session

`git diff --numstat <state-2> <state-3> -- . ':(exclude)docs/**'`, run here rather
than inherited:

| slice | estimate | landed net | factor | pricing method |
|---|---|---|---|---|
| `110.2` (`0cd3f12`→`6af7649`) | 615 | 817 | **1.33×** | projected |
| `110.1` (`7747d69`→`29d25e5`) | 912 | 1290 | **1.41×** | projected |
| `111` (`be1aaf1`→`111b34a`) | 916 | 1525 | **1.66×** | projected |
| `112.1` (`28e7f4e`→`c86afd5`) | 551 | **549** | **1.00×** | **prototype-MEASURED** |

The three projected factors reproduce the ledger's figures exactly. The fourth
row is the one the landed `SPEC.md` §7 table does not carry, and it is the
relevant one: **this estimate is measured, so the applicable factor is 1.00×.**
Even applying the worst PROJECTED factor as a stress test — 595 × 1.66 = **988** —
the gate still does not fire. **The verdict is robust under every factor in the
ledger**, which is why it can be stated without hedging.

### This corrects the landed `SPEC.md` §7 in four rows, in BOTH directions

`SPEC.md` §7 is UNEDITABLE; the corrections are recorded here and in **ADR-0189**.
Its subtotal (649) and its stated total (779) are each internally consistent — the
table sums correctly — but four of its six rows are wrong against measurement:

| row | SPEC §7 | measured | direction |
|---|---|---|---|
| `tests/differential/src/lib.rs` | 250 | **263** | under by 13 |
| `tests/differential/tests/tls_alpn.rs` | 45 | **32** | over by 13 |
| `0091-tls-alpn/` | 190 | **166** | over by 24 |
| `0092-tls-alpn-server-preference/` | 160 | **129** | over by 31 |
| `0004` cell 6 | 4 | **5** | under by 1 |
| `BEHAVIOR_CONTRACT.md` | 130 | **116** | over by 14 |

Net effect: **595 measured against 649 projected — the SPEC over-priced the code
by 54 lines (8%).** The two fixture rows carry most of it: §7 anchored them on
phase 111's `0090` (198 lines, which this session re-measured and confirms) but
`0091`/`0092` copy `0004-tls-downstream`'s much leaner shape (135 lines total)
rather than `0090`'s. §7's own parenthetical *"the three TLS fixtures are
106–158"* is also slightly off — re-measured they are **135 / 107 / 159**.

### Task-boundary validation — the `ADR-0186` correction, applied

`ADR-0186` recorded that **a whole-slice prototype cannot validate a plan's task
BOUNDARIES**, because dead-code and unused-function lints fire only in the
intermediate states such a build never occupies — two of `112.1`'s tasks failed
their OWN `-D warnings` gate for exactly that reason. This session therefore
built and gated the intermediate states separately:

| boundary | tree | `clippy --all-targets --all-features -- -D warnings` | `fmt --check` |
|---|---|---|---|
| after Task 1 | grammar only, dispatch site on `..` | **exit 0, clean** | clean |
| after Task 2 | + `check_alpn` + `drive_tls` + arm + widened dispatch | **exit 0, clean** | clean |
| after Task 3 | + `drive_tls_probes` (= full tree) | **exit 0, clean** | clean |

**And the hazard was reproduced deliberately, to prove the cut is not merely
lucky.** A tempting alternative cut — "Task 2 = add `check_alpn` and its unit
tests; Task 3 = call it" — was built and gated:

```
error: function `check_alpn` is never used
error: could not compile `differential` (lib) due to 1 previous error
exit 101
```

`check_alpn` is a private fn, so a task that adds it without a production caller
fails its own gate. **Task 2 below therefore lands the helper together with its
first caller. Do not split them.** Symmetrically, **Task 1 must leave the
dispatch site matching on `..`**: binding `client_alpn`/`expected_alpn` before
`run_tls_tcp_arm` accepts them is an `unused_variables` error under `-D warnings`,
and adding fields to a matched struct variant without touching the pattern is
`E0027`. Both traps are handled in Task 1's steps.

---

## Re-anchored citations — CF-112-12 damage control

**CF-112-12**: `112.1`'s insertions broke 18 `file:line` citations across landed
artifacts, and **two point forward into this sub-phase**. Every citation this
plan relies on was re-read at `2fcc651` and is given below at its CURRENT line.
The landed artifacts carrying the stale ones are uneditable; this table is the
remedy.

| cited by | claim | SPEC/ADR says | **TRUE at `2fcc651`** |
|---|---|---|---|
| `112.2/SPEC.md:199` | `ConfigError::Http2OverTlsNotSupported` | `bootstrap.rs:4267` | **`:4275`** — and `:4267` today holds `ConfigError::UnsupportedCodecType {`, a DIFFERENT variant that still looks plausible |
| `112/SPEC.md:438`, `112.2/SPEC.md:96` | merged-listener cap | `bootstrap.rs:3663-3666` | **`:3669-3672`** (`let total_listeners = bootstrap.all_listeners().count();` at `:3669`, `TooManyListeners` at `:3671`) |
| `112.1/REVIEW.md` M-1 | the `>255` validator | `bootstrap.rs:5958` | **`:5958` — still exact** (`if proto.len() > 255 {`), inside `validate_alpn_protocols` at `:5953-5967` |
| `112.1/REVIEW.md` M-5 | CF-112-1's own definition | `DECISIONS.md:2576` | **`:2599`** — the review's own commit `1b6a81d` inserted `ADR-0188` (`23 0`) above it. Locate by TEXT: `grep -n 'OPENS CF-112-1' docs/envoy-rust/DECISIONS.md` |

**The nine `tests/differential/src/lib.rs` citations in `SPEC.md` §2.4 are ALL
VALID** — re-read individually at `2fcc651`. `112.1` touched only `crates/`:
`git diff --numstat 2a9712b HEAD -- tests/` is **EMPTY**. Do not treat them as
suspect; CF-112-12's damage is confined to `bootstrap.rs`, `envoy-tls/src/tests.rs`
and `DECISIONS.md`.

| citation | content at `2fcc651` |
|---|---|
| `:38` | `#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]` |
| `:84` | `    TlsTcp {` |
| `:98` | `    TlsTcpProbeList {` |
| `:732` | `#[serde(deny_unknown_fields)]` (on `TlsTcpProbe`) |
| `:733` | `pub struct TlsTcpProbe {` |
| `:1910` | `pub async fn drive_tls(` |
| `:1985` | `pub async fn drive_tls_probes(` |
| `:4945` | `    let upstream_out = drive_tls(` |
| `:4954` | `    let subject_out = drive_tls(subject_addr, &payload, sni, roots, expected_cn.as_deref())` |

⚠ **Your OWN insertions will invalidate all nine mid-implementation.** Task 1
inserts ~174 lines above `:1910`. Re-derive by TEXT (`grep -n 'pub async fn
drive_tls'`), never by the number, after every task.

**One correction to the handoff, measured:** it refers to "the two
`run_tls_tcp_arm` call sites". `run_tls_tcp_arm` has exactly **one** call site
(the dispatch arm); it is `drive_tls` that has two call sites, **both inside**
`run_tls_tcp_arm`. The sibling `run_tls_tcp_probe_list_arm` (`:4965`) likewise
has one call site and calls `drive_tls_probes` twice.

---

## Carry-forward decisions — witnessed or banked, decided deliberately

`SPEC.md` §5 non-goal 7 forbids FIXING a carry-forward; it does not forbid
witnessing one cheaply. Three land on this surface. Decision for each:

- **CF-112-8 Consequence 2** (a comma inside an element may offer TWO protocols
  upstream and ONE 11-byte protocol here — INFERRED, not measured). **BANKED, and
  it is structurally unwitnessable by this harness.** `expected_alpn` is a
  SINGLE rule evaluated against BOTH proxies (SPEC §3 E3), so a fixture can only
  express agreement; a cell where the two proxies legitimately DIFFER cannot be
  written down at all. Confirming the inference would additionally need a third
  server list, and a fixture may carry exactly one listener with one list, so it
  would need a third fixture — and that fixture could never be green, which gate
  §7.5(a) requires. **The right home is a phase that fixes CF-112-8 and lands the
  wire measurement with the fix**, exactly as `112.1/REVIEW.md` §7 recommends
  ("M-1's comma split plus M-2's empty-element skip … should land together").
  Recorded in the contract section as INFERRED so no later session reads it as
  established.
- **CF-112-9** (empty element panics a debug build on the upstream connect).
  **BANKED, and actively avoided:** see the Global Constraint above. No fixture
  here configures an empty element on either side.
- **CF-112-6** (one empty element makes upstream negotiate nothing at all).
  **BANKED**, and recorded in the contract section as a known unmatched cell.

Everything else stays banked untouched: **CF-112-1/2/3/4/7**, **CF-112-10/11/12**,
the `112.1` REVIEW's M-1…M-5 and N-1…N-12, and every earlier phase's set.
**CF-112-5 stays CLOSED.**

---

## File Structure

| file | disposition | responsibility |
|---|---|---|
| `tests/differential/src/lib.rs` | **modify** | the grammar (`AlpnRule`, two fields × two carriers), the adjudicator (`check_alpn`), and the offer/assert plumbing in both TLS drivers and both arms |
| `tests/differential/tests/tls_alpn.rs` | **create** | runner: one `#[tokio::test]` per new fixture |
| `tests/fixtures/0091-tls-alpn/` | **create** | cells 1–4: four probes, server list `["h2", "http/1.1"]` |
| `tests/fixtures/0092-tls-alpn-server-preference/` | **create** | cell 5: one probe, server list REVERSED |
| `tests/fixtures/0004-tls-downstream/expectations.yaml` | **modify** | cell 6, the no-ALPN control |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | **modify** | the `## ALPN` section |
| `docs/envoy-rust/phases/112.2-alpn-differential-witness/PROGRESS.md` | **create** | appended per task (§5 state 3) |

`lib.rs` is 11229 lines and is the repository's established home for driver
grammar, driver bodies and dispatch arms; the phase-111 `expected_trailers` work
extended it the same way (`277 0` on this exact file). Splitting it is out of
scope and would be a far larger change than the feature.

---

## Task 1: The fixture grammar — `AlpnRule` and the two new fields

**Files:**
- Modify: `tests/differential/src/lib.rs` — the `Driver::TlsTcp` variant
  (locate by text `TlsTcp {`, ~`:84`); `TlsTcpProbe` (locate by
  `pub struct TlsTcpProbe {`, ~`:733`); the `Driver::TlsTcp` dispatch arm
  (locate by `Driver::TlsTcp { sni, expected_cn } => {`, ~`:4216`)
- Test: `tests/differential/src/lib.rs` `mod tests` — beside the existing
  `expectations_parse_tls_tcp_probe_list_driver` (locate by text)

**Interfaces:**
- Consumes: nothing.
- Produces: `pub enum AlpnRule { Selected { protocol: String }, NoneSelected }`;
  `TlsTcpProbe.client_alpn: Vec<String>`; `TlsTcpProbe.expected_alpn:
  Option<AlpnRule>`; the same two fields on `Driver::TlsTcp`. Tasks 2 and 3
  consume all four; Tasks 4–6 name them in YAML.

⚠ **Two `-D warnings` traps, both reproduced in the prototype:**
1. Adding fields to `Driver::TlsTcp` breaks the existing dispatch pattern with
   **`E0027` (pattern does not mention field)**. The pattern MUST be updated in
   this task.
2. It must be updated to **`..`**, not to binding the new names —
   `run_tls_tcp_arm` does not accept them until Task 2, so binding them here is
   **`unused_variables`**, which is an error under `-D warnings`.

- [ ] **Step 1: Write the failing tests**

Insert immediately BEFORE `fn expectations_reject_unknown_driver_kind()`:

```rust
    // 112.2 Task 1 RED: the four cells of `0091-tls-alpn` as a probe list —
    // a per-probe client offer plus a positive and a negative expectation.
    #[test]
    fn expectations_parse_tls_alpn_probe_list() {
        let yaml = r#"
driver:
  kind: tls_tcp_probe_list
  probes:
    - sni: a.example.com
      client_alpn: ["h2", "http/1.1"]
      expected_alpn: { kind: selected, protocol: h2 }
    - sni: a.example.com
      client_alpn: ["h3"]
      expected_alpn: { kind: none_selected }
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        match e.driver {
            Driver::TlsTcpProbeList { ref probes } => {
                assert_eq!(probes.len(), 2);
                assert_eq!(probes[0].client_alpn, vec!["h2", "http/1.1"]);
                assert_eq!(
                    probes[0].expected_alpn,
                    Some(AlpnRule::Selected {
                        protocol: "h2".to_string()
                    })
                );
                assert_eq!(probes[1].client_alpn, vec!["h3"]);
                assert_eq!(probes[1].expected_alpn, Some(AlpnRule::NoneSelected));
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
    }

    // 112.2 Task 1 RED: cell 4 — the client offers NOTHING. `client_alpn`
    // defaults to empty, which is "send no ALPN extension".
    #[test]
    fn expectations_parse_tls_alpn_probe_without_client_offer() {
        let yaml = r#"
driver:
  kind: tls_tcp_probe_list
  probes:
    - sni: a.example.com
      expected_alpn: { kind: none_selected }
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        match e.driver {
            Driver::TlsTcpProbeList { ref probes } => {
                assert!(probes[0].client_alpn.is_empty());
                assert_eq!(probes[0].expected_alpn, Some(AlpnRule::NoneSelected));
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
    }

    // 112.2 Task 1 RED: cell 6 rides on `Driver::TlsTcp`, so the single-probe
    // driver carries the same two fields.
    #[test]
    fn expectations_parse_tls_tcp_with_alpn_fields() {
        let yaml = r#"
driver:
  kind: tls_tcp
  sni: a.example.com
  client_alpn: ["h2", "http/1.1"]
  expected_alpn: { kind: none_selected }
equivalence:
  response_body:
    kind: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        match e.driver {
            Driver::TlsTcp {
                ref sni,
                ref client_alpn,
                ref expected_alpn,
                ..
            } => {
                assert_eq!(sni, "a.example.com");
                assert_eq!(client_alpn, &vec!["h2", "http/1.1"]);
                assert_eq!(expected_alpn.as_ref(), Some(&AlpnRule::NoneSelected));
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
    }

    // 112.2 PV: every pre-112 TLS fixture must still parse with the new
    // fields absent — this is the `#[serde(default)]` claim, pinned.
    #[test]
    fn expectations_parse_pre_112_tls_fixtures_unchanged() {
        let yaml = r#"
driver:
  kind: tls_tcp
  sni: a.example.com
equivalence:
  response_body:
    kind: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        match e.driver {
            Driver::TlsTcp {
                ref client_alpn,
                ref expected_alpn,
                ..
            } => {
                assert!(client_alpn.is_empty());
                assert!(expected_alpn.is_none());
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
    }

    // 112.2 Task 1 RED: `AlpnRule` carries `deny_unknown_fields`, so a typo
    // in the rule is a hard parse error rather than a silently ignored key.
    #[test]
    fn expectations_reject_unknown_alpn_rule_field() {
        let yaml = r#"
driver:
  kind: tls_tcp
  sni: a.example.com
  expected_alpn: { kind: selected, protocol: h2, protocul: h3 }
"#;
        let err = serde_yaml::from_str::<Expectations>(yaml).unwrap_err();
        assert!(
            err.to_string().contains("protocul"),
            "unexpected error: {err}"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p differential --lib alpn 2>&1 | tail -20
```

Expected: **compile failure**, `cannot find type AlpnRule in this scope` /
`no field client_alpn`. A compile error is the correct RED here (the grammar
does not exist yet); it is NOT an acceptable RED for the mutation checks in
Task 4, where the `test result` line must exist.

- [ ] **Step 3: Add the two fields to `Driver::TlsTcp`**

Replace the variant (keep the existing `/// 03.1 NEW:` line as the first line):

```rust
    /// 03.1 NEW: TLS round-trip with explicit SNI + optional CN/SAN check.
    ///
    /// 112.2 E1: `client_alpn` is the ALPN protocol list the harness's client
    /// OFFERS, and `expected_alpn` is what the completed handshake must have
    /// selected. Both are `#[serde(default)]`, so every pre-112 fixture
    /// (`0004-tls-downstream` included) parses unchanged; an empty
    /// `client_alpn` means "offer no ALPN extension at all", which is the
    /// pre-112 behaviour.
    TlsTcp {
        sni: String,
        #[serde(default)]
        expected_cn: Option<String>,
        #[serde(default)]
        client_alpn: Vec<String>,
        #[serde(default)]
        expected_alpn: Option<AlpnRule>,
    },
```

- [ ] **Step 4: Add the two fields to `TlsTcpProbe` and declare `AlpnRule`**

Replace the `TlsTcpProbe` struct with:

```rust
/// One TLS-SNI probe entry inside `Driver::TlsTcpProbeList`. SPEC §D6.
///
/// 112.2 E1: `client_alpn` / `expected_alpn` are per-probe, because
/// `0091-tls-alpn` varies the CLIENT's offer across four probes against one
/// server list. Both are `#[serde(default)]` so `0006-tls-sni`'s probes parse
/// unchanged.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TlsTcpProbe {
    pub sni: String,
    #[serde(default)]
    pub expected_cn: Option<String>,
    #[serde(default)]
    pub client_alpn: Vec<String>,
    #[serde(default)]
    pub expected_alpn: Option<AlpnRule>,
}

/// 112.2 E2: what the completed TLS handshake must have negotiated.
///
/// Cells 3, 4 and 6 of the phase-112 cell table assert the ABSENCE of a
/// negotiated protocol, which is a different claim from "any protocol". A
/// rule carrying only a positive value would make those three cells
/// unwriteable, and letting `expected_alpn: None` mean "negotiated nothing"
/// would make them silently vacuous — `None` already means "do not check".
/// Hence the explicit negative arm.
///
/// Internally tagged, copying `BodyRule`'s shape so a unit-form and a
/// struct-form variant can coexist:
/// `expected_alpn: { kind: selected, protocol: h2 }` /
/// `expected_alpn: { kind: none_selected }`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AlpnRule {
    /// The handshake must have selected exactly this protocol.
    Selected { protocol: String },
    /// The handshake must have completed with NO protocol selected. This is
    /// the mismatch disposition upstream Envoy was MEASURED to have (parent
    /// SPEC §1.1 F2) and that `112.1`'s D6' reproduces in envoy-rust.
    NoneSelected,
}
```

- [ ] **Step 5: Keep the dispatch site compiling — `..`, NOT the new bindings**

Replace the dispatch arm:

```rust
        Driver::TlsTcp {
            sni, expected_cn, ..
        } => {
            run_tls_tcp_arm(&ctx, upstream, subject, sni, expected_cn).await?;
        }
```

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cargo test -p differential --lib alpn 2>&1 | tail -12
cargo test -p differential --lib expectations_parse_pre_112 2>&1 | grep 'test result'
```

Expected: `test result: ok. 4 passed` and `1 passed`. ⚠ **Assert the counts are
NON-ZERO** — `0 passed; N filtered out` is a false green.

- [ ] **Step 7: Gate this task's own boundary**

```bash
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: both exit 0. **Prototype-verified at this exact boundary.** If clippy
reports `unused_variables` on `client_alpn`, Step 5 was written with bindings
instead of `..`.

- [ ] **Step 8: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 112.2 task 1: AlpnRule + client_alpn/expected_alpn on the two TLS drivers"
```

---

## Task 2: `check_alpn` and the `drive_tls` single-handshake path

**Files:**
- Modify: `tests/differential/src/lib.rs` — new `fn check_alpn` immediately
  before `fn check_cn_or_san` (locate by the doc line
  `/// Walk a leaf cert's SAN DNS entries`); `pub async fn drive_tls` (locate by
  `pub async fn drive_tls(`); `async fn run_tls_tcp_arm` (locate by
  `async fn run_tls_tcp_arm(`); the `Driver::TlsTcp` dispatch arm

**Interfaces:**
- Consumes: `AlpnRule` (Task 1).
- Produces: `fn check_alpn(negotiated: Option<&[u8]>, rule: &AlpnRule) -> Result<()>`;
  `drive_tls(addr, payload, sni, root_store, expected_cn, client_alpn: &[String],
  expected_alpn: Option<&AlpnRule>) -> Result<Vec<u8>>`. Task 3 mirrors the
  pattern; Task 6's fixture exercises this path.

⚠ **`check_alpn` and its first production caller MUST land in the SAME task.**
The prototype built the alternative and it fails its own gate with
`error: function check_alpn is never used` (exit 101). Do not split.

- [ ] **Step 1: Write the failing test**

Insert into `mod tests`:

```rust
    // 112.2 Task 2 RED: the adjudicator distinguishes "negotiated X" from
    // "negotiated nothing" in both directions.
    #[test]
    fn check_alpn_adjudicates_all_four_outcomes() {
        let h2 = AlpnRule::Selected {
            protocol: "h2".to_string(),
        };
        // positive rule, matching protocol
        check_alpn(Some(b"h2"), &h2).expect("h2 matches h2");
        // positive rule, wrong protocol
        let e = check_alpn(Some(b"http/1.1"), &h2).unwrap_err().to_string();
        assert!(e.contains("expected ALPN \"h2\""), "unexpected: {e}");
        // positive rule, nothing negotiated
        let e = check_alpn(None, &h2).unwrap_err().to_string();
        assert!(e.contains("no protocol selected"), "unexpected: {e}");
        // negative rule, nothing negotiated
        check_alpn(None, &AlpnRule::NoneSelected).expect("None satisfies NoneSelected");
        // negative rule, something negotiated
        let e = check_alpn(Some(b"h2"), &AlpnRule::NoneSelected)
            .unwrap_err()
            .to_string();
        assert!(e.contains("expected NO ALPN"), "unexpected: {e}");
    }
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p differential --lib check_alpn 2>&1 | tail -10
```

Expected: `cannot find function check_alpn in this scope`.

- [ ] **Step 3: Add `check_alpn`**

Insert immediately before the `/// Walk a leaf cert's SAN DNS entries` doc block:

```rust
/// 112.2 E2: adjudicate a completed handshake's negotiated ALPN protocol
/// against a fixture's `expected_alpn` rule.
///
/// `negotiated` is `rustls`' `alpn_protocol()` on the completed connection:
/// `None` means the handshake finished with no protocol selected, which is
/// upstream Envoy's MEASURED disposition both on a mismatch (parent SPEC
/// §1.1 F2) and when the server advertises no list at all (F4). It is NOT an
/// error condition, so it must be assertable — hence `AlpnRule::NoneSelected`.
fn check_alpn(negotiated: Option<&[u8]>, rule: &AlpnRule) -> Result<()> {
    match rule {
        AlpnRule::Selected { protocol } => {
            let got = negotiated.ok_or_else(|| {
                anyhow::anyhow!(
                    "expected ALPN {protocol:?} to be negotiated, but the handshake \
                     completed with no protocol selected"
                )
            })?;
            let got = std::str::from_utf8(got)
                .map_err(|e| anyhow::anyhow!("negotiated ALPN is not valid UTF-8: {e}"))?;
            if got != protocol {
                bail!("expected ALPN {protocol:?}, got {got:?}");
            }
        }
        AlpnRule::NoneSelected => {
            if let Some(got) = negotiated {
                bail!(
                    "expected NO ALPN protocol to be negotiated, got {:?}",
                    String::from_utf8_lossy(got)
                );
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Widen `drive_tls` — offer, then assert**

Two edits inside `pub async fn drive_tls`. First the signature and the offer
(note `let client_cfg` becomes `let mut client_cfg`):

```rust
pub async fn drive_tls(
    addr: SocketAddr,
    payload: &[u8],
    sni: &str,
    root_store: rustls::RootCertStore,
    expected_cn: Option<&str>,
    client_alpn: &[String],
    expected_alpn: Option<&AlpnRule>,
) -> Result<Vec<u8>> {
    use rustls::pki_types::ServerName;
    use std::convert::TryFrom;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let mut client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    // 112.2 E1: an empty `client_alpn` leaves `alpn_protocols` empty, which is
    // rustls' "do not send the ALPN extension at all" — the pre-112 behaviour
    // every existing TLS fixture relies on.
    client_cfg.alpn_protocols = client_alpn.iter().map(|p| p.as_bytes().to_vec()).collect();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_cfg));
```

Then the assertion, inserted after the existing `expected_cn` block's closing
brace and before `tls.write_all(payload).await?;`:

```rust
    // 112.2 E1: the negotiated protocol is read off the SAME completed
    // handshake value `expected_cn` already uses — `.alpn_protocol()` beside
    // `.peer_certificates()`. No new driver, no second connection.
    if let Some(rule) = expected_alpn {
        check_alpn(tls.get_ref().1.alpn_protocol(), rule)
            .with_context(|| format!("expected_alpn match against {addr}"))?;
    }
```

- [ ] **Step 5: Thread the arm and widen the dispatch site**

`run_tls_tcp_arm` gains two parameters, taking it to seven. **No
`#[allow(clippy::too_many_arguments)]` is needed and none may be added** —
clippy's threshold is *more than* seven, and this was MEASURED: the tree was
gated with the attribute removed and `clippy --all-targets --all-features --
-D warnings` exited **0** with no `too_many_arguments` diagnostic. (An earlier
draft of this plan asserted the attribute was required; it is not, and the
measured LoC row above is net of removing it.)

```rust
async fn run_tls_tcp_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    sni: &str,
    expected_cn: &Option<String>,
    client_alpn: &[String],
    expected_alpn: Option<&AlpnRule>,
) -> Result<()> {
```

Both `drive_tls` call sites inside it:

```rust
    let upstream_out = drive_tls(
        upstream_addr,
        &payload,
        sni,
        roots.clone(),
        expected_cn.as_deref(),
        client_alpn,
        expected_alpn,
    )
    .await
    .context("upstream envoy tls drive")?;
    let subject_out = drive_tls(
        subject_addr,
        &payload,
        sni,
        roots,
        expected_cn.as_deref(),
        client_alpn,
        expected_alpn,
    )
    .await
    .context("envoy-rust tls drive")?;
```

And the dispatch arm, which now binds what Task 1 left on `..`:

```rust
        Driver::TlsTcp {
            sni,
            expected_cn,
            client_alpn,
            expected_alpn,
        } => {
            run_tls_tcp_arm(
                &ctx,
                upstream,
                subject,
                sni,
                expected_cn,
                client_alpn,
                expected_alpn.as_ref(),
            )
            .await?;
        }
```

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cargo test -p differential --lib check_alpn 2>&1 | grep 'test result'
cargo test -p differential --lib 2>&1 | grep 'test result'
```

Expected: `1 passed` for the first; the full lib suite green for the second.

- [ ] **Step 7: Gate this task's own boundary**

```bash
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: both exit 0. **Prototype-verified at this exact boundary.**

- [ ] **Step 8: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 112.2 task 2: check_alpn + the drive_tls offer/assert path"
```

---

## Task 3: The per-probe path — `drive_tls_probes`

**Files:**
- Modify: `tests/differential/src/lib.rs` — `pub async fn drive_tls_probes`
  (locate by `pub async fn drive_tls_probes(`)

**Interfaces:**
- Consumes: `TlsTcpProbe.client_alpn`, `TlsTcpProbe.expected_alpn` (Task 1),
  `check_alpn` (Task 2).
- Produces: no signature change — `drive_tls_probes(addr, payload, probes,
  root_store)` is unchanged, because the offer now travels INSIDE each
  `TlsTcpProbe`. `run_tls_tcp_probe_list_arm` and its dispatch arm are therefore
  **not touched.** Tasks 4 and 5 exercise this path.

**Why the config moves inside the loop:** `alpn_protocols` lives on the
`ClientConfig`, and the offer is per-probe, so one config for the whole sequence
can no longer serve. `root_store` is consumed by `with_root_certificates`, hence
the `.clone()` per iteration. This is the only structural change; every other
per-probe discipline (fresh TCP connection, `expected_cn`, `read_exact`, the
ADR-0007 trailing-byte poll, `shutdown`) is untouched.

- [ ] **Step 1: Write the failing test**

There is no non-Docker unit test that can exercise a real handshake here, so the
RED for this task is the fixture in Task 4. **What CAN be pinned now is that the
probe list carries the offer through parsing into the exact shape
`drive_tls_probes` reads.** Insert into `mod tests`:

```rust
    // 112.2 Task 3 RED: `drive_tls_probes` reads the offer off each probe, so
    // pin that a probe list parses into per-probe offers that DIFFER — the
    // property that makes one fixture cover four cells.
    #[test]
    fn tls_probe_list_carries_distinct_per_probe_offers() {
        let yaml = r#"
driver:
  kind: tls_tcp_probe_list
  probes:
    - sni: a.example.com
      client_alpn: ["h2", "http/1.1"]
      expected_alpn: { kind: selected, protocol: h2 }
    - sni: a.example.com
      client_alpn: ["http/1.1"]
      expected_alpn: { kind: selected, protocol: http/1.1 }
    - sni: a.example.com
      client_alpn: ["h3"]
      expected_alpn: { kind: none_selected }
    - sni: a.example.com
      expected_alpn: { kind: none_selected }
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        let Driver::TlsTcpProbeList { probes } = e.driver else {
            panic!("unexpected driver")
        };
        let offers: Vec<&[String]> = probes.iter().map(|p| p.client_alpn.as_slice()).collect();
        assert_eq!(offers.len(), 4);
        assert_eq!(offers[0], ["h2".to_string(), "http/1.1".to_string()]);
        assert_eq!(offers[1], ["http/1.1".to_string()]);
        assert_eq!(offers[2], ["h3".to_string()]);
        assert!(offers[3].is_empty());
    }
```

- [ ] **Step 2: Run it to verify it passes already, then read this**

```bash
cargo test -p differential --lib tls_probe_list_carries 2>&1 | grep 'test result'
```

This test goes **GREEN on Task 1's grammar alone** — it is a characterization
pin, not a RED for Task 3's plumbing. **That is stated openly rather than
disguised:** the genuine RED for the plumbing is fixture `0091`'s probe 1 in
Task 4, which cannot pass until `drive_tls_probes` actually sends the offer.
Task 4's mutation check is what proves it non-vacuous.

- [ ] **Step 3: Move the config inside the loop and add the assertion**

Replace the block from `let client_cfg = ...` through `for probe in probes {`:

```rust
    // 112.2 E1: the client's ALPN offer is PER PROBE, and `alpn_protocols`
    // lives on the `ClientConfig`, so the config and its connector are built
    // inside the loop rather than once outside it. Every other probe-level
    // discipline below is unchanged.
    let mut outputs = Vec::with_capacity(probes.len());
    for probe in probes {
        let mut client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(root_store.clone())
            .with_no_client_auth();
        client_cfg.alpn_protocols = probe
            .client_alpn
            .iter()
            .map(|p| p.as_bytes().to_vec())
            .collect();
        let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_cfg));
```

Then insert the assertion after the existing `expected_cn` block and before
`tls.write_all(payload)`:

```rust
        if let Some(rule) = &probe.expected_alpn {
            check_alpn(tls.get_ref().1.alpn_protocol(), rule).with_context(|| {
                format!(
                    "expected_alpn match against {addr} for probe sni={:?} client_alpn={:?}",
                    probe.sni, probe.client_alpn
                )
            })?;
        }
```

- [ ] **Step 4: Run the full lib suite**

```bash
cargo test -p differential --lib 2>&1 | grep 'test result'
```

Expected: green, and the count is **Task 1's 5 + Task 2's 1 + this task's 1 = 7
above the pre-phase baseline**. Record the baseline before Task 1 so the delta
can be asserted rather than eyeballed.

- [ ] **Step 5: Gate this task's own boundary**

```bash
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: both exit 0. **Prototype-verified at this exact boundary.**

- [ ] **Step 6: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 112.2 task 3: per-probe ALPN offer and assertion in drive_tls_probes"
```

---

## Task 4: Fixture `0091-tls-alpn` (cells 1–4) + the runner

**Files:**
- Create: `tests/fixtures/0091-tls-alpn/envoy.yaml` (49)
- Create: `tests/fixtures/0091-tls-alpn/envoy-rust.yaml` (42)
- Create: `tests/fixtures/0091-tls-alpn/expectations.yaml` (31)
- Create: `tests/fixtures/0091-tls-alpn/README.md` (43)
- Create: `tests/fixtures/0091-tls-alpn/inputs/payload.bin` (1)
- Create: `tests/differential/tests/tls_alpn.rs` (32)

**Interfaces:**
- Consumes: `Driver::TlsTcpProbeList` + `TlsTcpProbe.{client_alpn, expected_alpn}`
  (Task 1), the per-probe plumbing (Task 3), `differential::run_fixture`.
- Produces: `tls_alpn_fixture` and (in Task 5) `tls_alpn_server_preference_fixture`.

Both YAML files are `0004-tls-downstream`'s, with the node ids changed and
**one line added** — `alpn_protocols: ["h2", "http/1.1"]` as the first key under
`common_tls_context`. That keeps the reachable-backend cluster (E6), the
`{{PORT}}` / `{{LEAF_A_CERT_PATH}}` / `{{LEAF_A_KEY_PATH}}` / `{{BACKEND_HOST}}` /
`{{BACKEND_PORT}}` templating, and the `tcp_proxy`-not-HCM shape (E4) intact by
construction. `inputs/payload.bin` is required — `run_tls_tcp_probe_list_arm`
reads `inputs/payload.bin` unconditionally.

- [ ] **Step 1: Create the fixture directory and the payload**

```bash
mkdir -p tests/fixtures/0091-tls-alpn/inputs
printf 'hello, envoy-rust\n' > tests/fixtures/0091-tls-alpn/inputs/payload.bin
```

(18 bytes, byte-identical to `0004`'s. Do NOT use `touch`: it creates files and
mutates the tree.)

- [ ] **Step 2: Create both config files from `0004`'s, mechanically**

```bash
for side in envoy.yaml envoy-rust.yaml; do
  sed -e 's/envoy-rust-phase-03-1-subject/envoy-rust-phase-112-2-subject/' \
      -e 's/envoy-rust-phase-03-1$/envoy-rust-phase-112-2/' \
      -e 's/^\( *\)common_tls_context:$/\1common_tls_context:\n\1  alpn_protocols: ["h2", "http\/1.1"]/' \
      tests/fixtures/0004-tls-downstream/$side > tests/fixtures/0091-tls-alpn/$side
done
diff tests/fixtures/0004-tls-downstream/envoy.yaml tests/fixtures/0091-tls-alpn/envoy.yaml
```

Expected diff: exactly the two `node:` lines and one added
`                alpn_protocols: ["h2", "http/1.1"]` line. **Verify the added
line's indentation matches `tls_certificates:`'s** — it is a sibling key of
`tls_certificates` under `common_tls_context`, and a mis-indented key is
`deny_unknown_fields`-fatal on envoy-rust and a load error upstream.

- [ ] **Step 3: Write `expectations.yaml` — the four cells**

```yaml
# 112.2 cells 1-4. One listener, one server list (["h2", "http/1.1"]), four
# independent TLS handshakes that vary only the CLIENT's offer. Each probe
# asserts what the completed handshake negotiated, on BOTH proxies; both
# satisfying the rule IS the cross-proxy equivalence claim (E3).
driver:
  kind: tls_tcp_probe_list
  probes:
    # Cell 1 — full intersection; the server's first choice wins.
    - sni: a.example.com
      expected_cn: a.example.com
      client_alpn: ["h2", "http/1.1"]
      expected_alpn: { kind: selected, protocol: h2 }
    # Cell 2 — the client offers only the server's SECOND choice.
    - sni: a.example.com
      expected_cn: a.example.com
      client_alpn: ["http/1.1"]
      expected_alpn: { kind: selected, protocol: http/1.1 }
    # Cell 3 — NO intersection. Upstream Envoy completes the handshake with
    # nothing selected and sends no `no_application_protocol` alert; this is
    # the cell that fails if 112.1's D6' accept path is wrong.
    - sni: a.example.com
      expected_cn: a.example.com
      client_alpn: ["h3"]
      expected_alpn: { kind: none_selected }
    # Cell 4 — the client sends no ALPN extension at all.
    - sni: a.example.com
      expected_cn: a.example.com
      expected_alpn: { kind: none_selected }
equivalence:
  response_body:
    kind: byte_exact
```

⚠ **`protocol: http/1.1` is correct unquoted** — YAML reads it as the string
`http/1.1` (`/` is not special in a plain scalar). Verified by parsing the real
file, not by reading the spec.

⚠ **The `equivalence:` block is inert for this driver** —
`run_tls_tcp_probe_list_arm` never calls `assert_equivalence`; byte-equality is
enforced by `read_exact` inside `drive_tls_probes`. It is included because
`0006-tls-sni` includes it, and consistency across the TLS fixtures is worth more
than removing a no-op.

- [ ] **Step 4: Write `README.md`**

```markdown
# Fixture 0091-tls-alpn

Cells 1–4 of the phase-112 ALPN cell table. A `tcp_proxy` listener terminates
downstream TLS with a leaf whose SAN is `a.example.com` (rcgen-generated at
fixture-run time per ADR-0018, signed by the harness CA), and its
`common_tls_context` advertises `alpn_protocols: ["h2", "http/1.1"]`. Both
proxies dial the same plaintext echo backend.

`Driver::TlsTcpProbeList` drives four independent TLS handshakes against the
one listener, varying only the **client's** offer — which is why one fixture
covers four cells and why `TlsTcpProbeList` rather than `TlsTcp` is the right
driver. Each probe asserts the negotiated protocol on BOTH proxies through
`expected_alpn`; both sides satisfying the rule is the cross-proxy equivalence
claim (112.2 SPEC §3 E3), so this driver needs no final `assert_equivalence`.

| probe | client offers | expected |
|---|---|---|
| 1 | `h2`, `http/1.1` | `h2` — the server's first choice |
| 2 | `http/1.1` | `http/1.1` |
| 3 | `h3` | nothing negotiated, handshake SUCCEEDS |
| 4 | *(no ALPN extension)* | nothing negotiated |

Every row was MEASURED on upstream Envoy v1.33.0 at the phase-112 and 112.1
PLAN-write sessions, 45/45 runs each, against the `ENVOY_TARGET.md` digest
verified on the running container.

**Probe 3 is the load-bearing one.** `rustls` by default sends a fatal
`no_application_protocol` alert when a client's non-empty offer misses a
non-empty server list, and upstream Envoy does not. Sub-phase 112.1's D6′
`LazyConfigAcceptor` accept path exists to remove that divergence; probe 3 is
its cross-proxy witness, and it goes RED if that path regresses.

What is *out* of this fixture:

- Server-preference ordering — fixture `0092-tls-alpn-server-preference`.
- The no-ALPN control (cell 6) — it rides on `0004-tls-downstream`, because a
  second listener is illegal (`ConfigError::TooManyListeners`) and per-chain
  ALPN is inexpressible (`DownstreamTls::from_listener` builds one
  `rustls::ServerConfig` per listener; CF-112-4).
- The UPSTREAM ALPN offer — no driver can report what a backend negotiated
  (CF-112-2).
- ALPN × SNI (CF-112-3), and `Http2OverTlsNotSupported` (CF-112-1): `h2` is
  advertised here but never spoken; the filter chain is `tcp_proxy`.
```

- [ ] **Step 5: Write the runner**

`tests/differential/tests/tls_alpn.rs`:

```rust
//! Phase 112.2 differential acceptance tests: TLS ALPN negotiation across a
//! `tcp_proxy` listener that terminates downstream TLS, against upstream
//! Envoy v1.33.0 and envoy-rust. Docker-gated.
//!
//! `0091-tls-alpn` carries cells 1-4 of the phase-112 cell table as four
//! probes against one server list `["h2", "http/1.1"]`; `0092` carries cell 5,
//! the server-preference witness, with the list reversed. Cell 6 (the
//! no-ALPN control) rides on the pre-existing `0004-tls-downstream`.

use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures")
        .join(name)
}

#[tokio::test]
async fn tls_alpn_fixture() {
    differential::run_fixture(&fixture("0091-tls-alpn"))
        .await
        .expect("fixture passes");
}
```

(Task 5 appends the second test; the `fixture` helper is introduced here so
Task 5 adds only the test body.)

- [ ] **Step 6: Prove the fixture PARSES before spending a Docker run on it**

```bash
cargo test -p differential --lib 2>&1 | grep 'test result'
```

then, as a throwaway (delete it before committing):

```rust
// tests/differential/tests/zz_throwaway_parse.rs
use std::path::PathBuf;
#[test]
fn throwaway_fixture_parses() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/fixtures/0091-tls-alpn/expectations.yaml");
    let e = differential::load_expectations(&p).unwrap_or_else(|e| panic!("{e:#}"));
    println!("{:?}", e.driver);
}
```

```bash
cargo test -p differential --test zz_throwaway_parse -- --nocapture
rm tests/differential/tests/zz_throwaway_parse.rs
```

Expected print: `TlsTcpProbeList { probes: [ … client_alpn: ["h2", "http/1.1"],
expected_alpn: Some(Selected { protocol: "h2" }) … ] }` with **four** probes, the
fourth carrying `client_alpn: []`. **This was run in the prototype and produced
exactly that.** `envoy-bin` has NO `--mode validate`, so this throwaway test is
the way to prove an in-tree YAML parses.

- [ ] **Step 7: Run the fixture**

```bash
cargo test -p differential --test tls_alpn tls_alpn_fixture -- --nocapture > /tmp/alpn91.log 2>&1
grep -E 'test result|panicked|Error' /tmp/alpn91.log
```

Expected: `test result: ok. 1 passed`. ⚠ **Gate on the `test result` line's
existence, not on the exit code** — a test-function name fed to `--test` exits
101 having run nothing, which reads exactly like a RED. ⚠ **Redirect to a file,
never pipe through `tail`** — it truncates the `failures:` block. ⚠ If it is
RED, classify by **ISOLATION only**, never by the failure text, and leave a
30-second settle gap between Docker-spawning runs.

⚠ **If probe 3 is the failing probe, do not weaken it.** The probe-list driver
aborts at the first failing probe, so one red run names ONE probe. Probe 3 is the
D6′ witness; a RED there means `112.1`'s accept path regressed or was never
correct, which is a §5 non-goal-1 escalation ("raise it, do not absorb it"), not
a fixture to relax.

- [ ] **Step 8: MUTATION-PROVE the fixture (SPEC §9(a))**

Use a **scratch worktree** (mutation checks collide with parallel subagents and
with the foreign workstream). Before each `sed`, **assert the target occurs
EXACTLY ONCE** — a mutation that hits both the implementation and the test fakes
a GREEN and reads as "vacuous tests". Force a rebuild and confirm
`Compiling differential`; a stale binary is a FALSE PASS. Run an **unmutated
control from the same tree** each time.

```bash
WT=$(mktemp -d)/mut && git worktree add --detach "$WT" main && cd "$WT"
git cherry-pick --no-commit <this phase's task commits>   # or copy the tree

# M1 — delete the server list from envoy-rust.yaml only.
grep -c 'alpn_protocols' tests/fixtures/0091-tls-alpn/envoy-rust.yaml   # MUST print 1
sed -i '/alpn_protocols/d' tests/fixtures/0091-tls-alpn/envoy-rust.yaml
cargo test -p differential --test tls_alpn tls_alpn_fixture > /tmp/m1.log 2>&1
grep -E 'Compiling differential|test result' /tmp/m1.log
```

**Predicted RED set, stated in advance so a GREEN is diagnosable:** `tls_alpn_fixture`
FAILS at **probe 1**, with `expected_alpn match against <subject addr> for probe
sni="a.example.com" client_alpn=["h2", "http/1.1"]` and
`expected ALPN "h2" to be negotiated, but the handshake completed with no
protocol selected`. Only that one test reddens; `tls_downstream_fixture` and
`tls_sni_fixture` stay green. **A GREEN here usually means the mutation is
MISAIMED, not that the fixture is vacuous** — check the `grep -c` really printed
1 and that the rebuild happened.

```bash
# M2 — delete the assertion from the driver.
grep -c 'if let Some(rule) = &probe.expected_alpn {' tests/differential/src/lib.rs  # MUST print 1
# delete that block, then:
cargo test -p differential --test tls_alpn tls_alpn_fixture > /tmp/m2.log 2>&1
```

Predicted: `tls_alpn_fixture` **passes** (the assertion is gone) — which is the
point: it proves the assertion, not the config, is what the test rests on. The
meaningful RED for M2 is the **unit** test `check_alpn_adjudicates_all_four_outcomes`
plus a compile error on the now-unused `check_alpn`. Record both.

```bash
# M3 — the control, from the same tree, unmutated.
git checkout -- . && cargo test -p differential --test tls_alpn > /tmp/ctl.log 2>&1
grep 'test result' /tmp/ctl.log     # MUST be green
cd - && git worktree remove --force "$WT"
```

⚠ **`git checkout --` is a NO-OP on an untracked fixture.** `0091` is untracked
inside a fresh worktree unless the task commits are present — adjudicate the
restore by `md5sum`, not by assuming the checkout worked.

- [ ] **Step 9: Gate and commit**

```bash
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git add tests/fixtures/0091-tls-alpn tests/differential/tests/tls_alpn.rs
git commit -m "phase 112.2 task 4: differential fixture 0091-tls-alpn (cells 1-4) + runner"
```

⚠ **`git add` the directory, then verify `payload.bin` is tracked** —
`git ls-files tests/fixtures/0091-tls-alpn/inputs/` must print it. A
`.gitignore` rule that swallows a binary input would make the fixture pass
locally and fail in CI with a missing-file error.

---

## Task 5: Fixture `0092-tls-alpn-server-preference` (cell 5)

**Files:**
- Create: `tests/fixtures/0092-tls-alpn-server-preference/{envoy.yaml (49),
  envoy-rust.yaml (42), expectations.yaml (14), README.md (23),
  inputs/payload.bin (1)}`
- Modify: `tests/differential/tests/tls_alpn.rs` — append the second test

**Interfaces:**
- Consumes: everything Task 4 consumes, plus Task 4's `fixture()` helper.
- Produces: `tls_alpn_server_preference_fixture`.

**Why a second fixture rather than a fifth probe in `0091`:** ALPN is a
`rustls::ServerConfig` property and a fixture may carry exactly **one** listener —
`validate()` returns `ConfigError::TooManyListeners` above one, on the MERGED
static+dynamic list (`crates/envoy-config/src/bootstrap.rs:3669-3672` at
`2fcc651`; the SPEC's `:3663-3666` is stale). A second filter chain does not help
either: `DownstreamTls::from_listener` builds one `ServerConfig` for the whole
listener and warns when chains disagree. **One server ALPN list per fixture is
forced.**

- [ ] **Step 1: Create the fixture, reusing Task 4's mechanics with the list REVERSED**

```bash
mkdir -p tests/fixtures/0092-tls-alpn-server-preference/inputs
printf 'hello, envoy-rust\n' > tests/fixtures/0092-tls-alpn-server-preference/inputs/payload.bin
for side in envoy.yaml envoy-rust.yaml; do
  sed -e 's/envoy-rust-phase-03-1-subject/envoy-rust-phase-112-2-subject/' \
      -e 's/envoy-rust-phase-03-1$/envoy-rust-phase-112-2/' \
      -e 's/^\( *\)common_tls_context:$/\1common_tls_context:\n\1  alpn_protocols: ["http\/1.1", "h2"]/' \
      tests/fixtures/0004-tls-downstream/$side > tests/fixtures/0092-tls-alpn-server-preference/$side
done
diff tests/fixtures/0091-tls-alpn/envoy.yaml tests/fixtures/0092-tls-alpn-server-preference/envoy.yaml
```

Expected diff: **exactly one line**, the `alpn_protocols` list order. That
one-line difference IS the experiment — assert it rather than assuming it.

- [ ] **Step 2: Write `expectations.yaml`**

```yaml
# 112.2 cell 5 — the D5 selection-order witness. The server list is REVERSED
# relative to 0091 and the client's offer is unchanged, so the selected
# protocol discriminates server preference from client preference: it is
# `http/1.1` iff selection follows the SERVER's order.
driver:
  kind: tls_tcp_probe_list
  probes:
    - sni: a.example.com
      expected_cn: a.example.com
      client_alpn: ["h2", "http/1.1"]
      expected_alpn: { kind: selected, protocol: http/1.1 }
equivalence:
  response_body:
    kind: byte_exact
```

- [ ] **Step 3: Write `README.md`**

```markdown
# Fixture 0092-tls-alpn-server-preference

Cell 5 of the phase-112 ALPN cell table: the selection-order witness.

Identical to `0091-tls-alpn` except that the listener's list is **reversed** to
`alpn_protocols: ["http/1.1", "h2"]` while the client still offers
`h2, http/1.1`. Because the two lists now disagree on order, the selected
protocol discriminates server preference from client preference — it is
`http/1.1` iff selection follows the **server's** order.

MEASURED `http/1.1`, 5/5 runs, on upstream Envoy v1.33.0 at the phase-112
PLAN-write session. `rustls` agrees by construction: its selection loop
(`rustls-0.23.39/src/server/hs.rs`) iterates the server's list in the outer
position and only scans the client's with `.any()`, and
`ServerConfig::alpn_protocols` is documented "most preferred first".

**This fixture needs its own directory rather than a fifth probe in `0091`**
because ALPN is a `rustls::ServerConfig` property and a fixture may carry
exactly one listener (`ConfigError::TooManyListeners`; the merged static +
dynamic cap is one). One server ALPN list per fixture is therefore forced.

The cell is expected GREEN on both proxies; its value is that it would catch a
silent inversion of the preference rule on either side.
```

- [ ] **Step 4: Append the second test to the runner**

```rust
#[tokio::test]
async fn tls_alpn_server_preference_fixture() {
    differential::run_fixture(&fixture("0092-tls-alpn-server-preference"))
        .await
        .expect("fixture passes");
}
```

- [ ] **Step 5: Run it**

```bash
cargo test -p differential --test tls_alpn -- --nocapture > /tmp/alpn92.log 2>&1
grep -E 'test result|panicked' /tmp/alpn92.log
```

Expected: `test result: ok. 2 passed` — both fixtures. Same isolation and
settle-gap discipline as Task 4 Step 7.

- [ ] **Step 6: MUTATION-PROVE it — invert the list, do not delete it**

The informative mutation here is not deletion but **inversion**, because
inversion is precisely the silent failure this fixture exists to catch:

```bash
grep -c 'alpn_protocols' tests/fixtures/0092-tls-alpn-server-preference/envoy-rust.yaml  # MUST print 1
sed -i 's/\["http\/1.1", "h2"\]/["h2", "http\/1.1"]/' \
  tests/fixtures/0092-tls-alpn-server-preference/envoy-rust.yaml
```

Predicted RED: `tls_alpn_server_preference_fixture` fails with
`expected ALPN "http/1.1", got "h2"` on the SUBJECT side only —
`tls_alpn_fixture` stays green. Restore, run the unmutated control, and adjudicate
the restore by `md5sum` (see Task 4 Step 8).

- [ ] **Step 7: Gate and commit**

```bash
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git add tests/fixtures/0092-tls-alpn-server-preference tests/differential/tests/tls_alpn.rs
git commit -m "phase 112.2 task 5: differential fixture 0092-tls-alpn-server-preference (cell 5)"
```

---

## Task 6: Cell 6 — the no-ALPN control on the EXISTING `0004-tls-downstream`

**Files:**
- Modify: `tests/fixtures/0004-tls-downstream/expectations.yaml` (+5)

**Interfaces:**
- Consumes: `Driver::TlsTcp.{client_alpn, expected_alpn}` (Task 1) and the
  `drive_tls` path (Task 2). This is the ONLY task that exercises Task 2's code
  through a fixture.

`0004` today carries **zero** occurrences of `alpn_protocols` in any of its
files — it is already exactly the cell-6 shape (server list ABSENT). Cell 6 is
therefore "make the client offer ALPN and assert nothing is negotiated", which is
a change to `expectations.yaml` alone. **Do not add `alpn_protocols` to either of
`0004`'s config files** — the absent server list IS the cell.

⚠ **This makes `0004` a CHANGED fixture under §7.5(a), not a pre-existing one
under (b).** Gate (b) therefore covers **89**, not 90.

⚠ **This changes the client's behaviour on a long-green fixture** — it starts
offering ALPN where it previously offered none. That is expected inert:
`112.1`'s D6′.1 keeps a no-ALPN listener on the unchanged `TlsAcceptor` path, and
upstream Envoy's cell-6 answer is measured. Expected inert is not the same as
verified inert, which is what Step 3 is for.

- [ ] **Step 1: Add the three keys**

`tests/fixtures/0004-tls-downstream/expectations.yaml` becomes:

```yaml
driver:
  kind: tls_tcp
  sni: a.example.com
  # 112.2 cell 6 — the no-ALPN control. This listener configures no
  # `alpn_protocols`, so a client that DOES offer ALPN must still complete the
  # handshake with nothing negotiated (parent SPEC §1.1 F4, MEASURED).
  client_alpn: ["h2", "http/1.1"]
  expected_alpn: { kind: none_selected }
equivalence:
  response_body:
    kind: byte_exact
```

- [ ] **Step 2: Confirm no config file gained a server list**

```bash
grep -rc alpn_protocols tests/fixtures/0004-tls-downstream/ | grep -v ':0' || echo "NONE — correct"
```

Expected: `NONE — correct`. Any hit means the server list was added by mistake
and cell 6 has become a duplicate of cell 1.

- [ ] **Step 3: Run `0004` and the two sibling TLS fixtures**

```bash
cargo test -p differential --test tls_downstream --test tls_sni --test tls_upstream \
  -- --nocapture > /tmp/tls_siblings.log 2>&1
grep -E 'test result' /tmp/tls_siblings.log
```

Expected: all three green. `tls_sni` and `tls_upstream` are the control — they
share the driver code Task 3 changed and must be unaffected.

- [ ] **Step 4: MUTATION-PROVE cell 6**

```bash
grep -c 'none_selected' tests/fixtures/0004-tls-downstream/expectations.yaml  # MUST print 1
sed -i 's/{ kind: none_selected }/{ kind: selected, protocol: h2 }/' \
  tests/fixtures/0004-tls-downstream/expectations.yaml
cargo test -p differential --test tls_downstream > /tmp/m6.log 2>&1
grep -E 'Compiling differential|test result' /tmp/m6.log
```

Predicted RED: `tls_downstream_fixture` fails on the UPSTREAM side first, with
`expected_alpn match against <upstream addr>` and
`expected ALPN "h2" to be negotiated, but the handshake completed with no
protocol selected`. Restore and re-run the control.

- [ ] **Step 5: Gate and commit**

```bash
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git add tests/fixtures/0004-tls-downstream/expectations.yaml
git commit -m "phase 112.2 task 6: cell 6 (no-ALPN control) on the existing 0004-tls-downstream"
```

---

## Task 7: The `BEHAVIOR_CONTRACT.md` ALPN section

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (+116)

**Interfaces:**
- Consumes: the six cells now witnessed by Tasks 4–6.
- Produces: the `## ALPN` section — the canonical statement of the rule, per
  invariant 4.1.5.

**Placement:** insert as a new `## ALPN` section immediately BEFORE
`## Header allow-list`, i.e. after `## Response trailers`. That is where phase
111 put its own new section, and the file is not alphabetical. Locate by text
(`grep -n '^## Header allow-list'`), never by line number.

⚠ **This section states one cell as INFERRED and several as UNMEASURED. Do not
"tidy" those hedges away** — CF-112-8 Consequence 2 is explicitly not measured
(see "Carry-forward decisions" above), and recording it as established would be
the exact failure `ADR-0188` fired over.

- [ ] **Step 1: Insert the section**

```markdown
## ALPN

**Phase 112** (`ADR-0183` scope, `ADR-0184` split), landed as sub-phases `112.1`
(the config surface and the `rustls` wiring) and `112.2` (this witness). Before
phase 112 envoy-rust rejected `common_tls_context.alpn_protocols` at
config-parse time, so every cell below was unreachable — the divergence was
boot-level, not a value divergence. Witnesses: fixtures
`tests/fixtures/0091-tls-alpn/` (cells 1–4),
`tests/fixtures/0092-tls-alpn-server-preference/` (cell 5) and
`tests/fixtures/0004-tls-downstream/` (cell 6).

Every upstream-Envoy value below was MEASURED against the pinned
`envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df3…`, `ENVOY_TARGET.md`,
verified on the running container), on loopback-mapped ports asserted free
before each run, with a REACHABLE backend — see "Determinism" below. Nothing
here is projected.

### The negotiation rule

`common_tls_context.alpn_protocols` is a list of ALPN protocol identifiers,
**most preferred first**, honored on both sides of the connection: on a
`DownstreamTlsContext` it is the server's list (`rustls::ServerConfig`), and on
an `UpstreamTlsContext` it is the list envoy-rust OFFERS to a backend
(`rustls::ClientConfig`). An absent or empty list means "do not advertise ALPN
at all".

**Selection follows the SERVER's order**, not the client's. Measured on
upstream Envoy 5/5 with the lists deliberately disagreeing (fixture `0092`), and
true of `rustls` by construction — its selection loop iterates the server's list
in the outer position and only scans the client's with `.any()`.

### The six cells

Server list `["h2", "http/1.1"]` unless stated otherwise.

| # | client offers | server lists | negotiated | witness |
|---|---|---|---|---|
| 1 | `h2`, `http/1.1` | `h2`, `http/1.1` | `h2` | `0091` probe 1 |
| 2 | `http/1.1` | `h2`, `http/1.1` | `http/1.1` | `0091` probe 2 |
| 3 | `h3` (no intersection) | `h2`, `http/1.1` | **nothing**, handshake SUCCEEDS | `0091` probe 3 |
| 4 | *(no ALPN extension)* | `h2`, `http/1.1` | nothing | `0091` probe 4 |
| 5 | `h2`, `http/1.1` | **`http/1.1`, `h2`** | `http/1.1` — the SERVER's first choice | `0092` |
| 6 | `h2`, `http/1.1` | *(field absent)* | nothing | `0004` |

### The mismatch disposition — cell 3, the one piece of real engineering

RFC 7301 §3.2 PERMITS a fatal `no_application_protocol` alert when nothing the
client offered is acceptable. **Upstream Envoy declines to send one:** the
handshake completes with no protocol selected. `rustls` by default does the
opposite — it sends the fatal alert and the handshake FAILS.

envoy-rust matches Envoy, and does so through a deliberate accept path rather
than by luck: when — and only when — a non-empty `alpn_protocols` is configured,
`DownstreamTls::accept` drives a `tokio_rustls::LazyConfigAcceptor`, reads the
ClientHello's offered list, and hands `into_stream` an ALPN-free twin of the
`ServerConfig` if nothing intersects. `rustls` then takes its
`our_protocols.is_empty()` branch: no alert, nothing selected, handshake
completes. When no ALPN is configured the pre-112 `TlsAcceptor` path is taken
unchanged, which is why every pre-112 fixture is unaffected.

### Determinism — a probe on this surface needs a reachable backend

With `tcp_proxy` pointed at an unreachable upstream, upstream Envoy's ALPN cells
are **non-deterministic**: Envoy tears the connection down when the upstream
connect fails and the teardown races the handshake, returning
`No ALPN negotiated` for a cell that should have negotiated on roughly 1 in 4
sequences, landing on a different cell each time. With a reachable backend, 40
consecutive handshakes were deterministic and `listener.<addr>.ssl.handshake`
equalled `downstream_cx_total` exactly. Fixtures on this surface must therefore
supply a real backend; the differential harness always does.

### Element validation

Upstream Envoy v1.33.0 ACCEPTS an empty list, an empty element (`""`) and a
duplicate element, and REJECTS an element that is too long with
`Invalid ALPN protocol string`. envoy-rust rejects the over-long element at
config-load with `ConfigError::InvalidAlpnProtocol { side, index, len }`. The
error TEXT differs and is not compared; only the accept/reject direction is.

⚠ **The reject sets do NOT fully coincide, and this is a live divergence**
(`CF-112-8`, MEASURED). Upstream applies the 255-byte bound **per
comma-separated segment** of an element; envoy-rust applies it to the whole
element. So `alpn_protocols: ["<255 a's>,<255 b's>"]` — a 511-byte element whose
segments are each 255 — BOOTS upstream and is REJECTED by envoy-rust, while a
259-byte element containing one 256-byte segment is rejected by both. Total
element length is irrelevant upstream; the segment is the unit.

### Cells that are NOT matched, NOT compared, or NOT measured

- **The UPSTREAM offer is not differentially witnessed** (`CF-112-2`). It is
  honored and unit-tested — Envoy was measured to offer the configured list
  verbatim and in order — but no harness driver can report what a BACKEND
  negotiated, so the cross-proxy witness covers the downstream direction only.
- **Whether a comma inside an element yields TWO offered protocols on the wire
  is INFERRED, not measured** (`CF-112-8` Consequence 2). If the segment is the
  unit of the length check it is probably the unit of the wire encoding too, in
  which case `["h2,http/1.1"]` — accepted by both — offers two protocols
  upstream and one 11-byte protocol here.
- **An empty element's runtime behaviour diverges** (`CF-112-6`, `CF-112-9`).
  Upstream Envoy with a server list of `["", "h2"]` negotiates NOTHING AT ALL,
  not even `h2`: one empty element poisons the whole list. envoy-rust does not
  reproduce that quirk, and an empty element in a CLUSTER's list additionally
  trips a `debug_assert!` inside `rustls`' `PayloadU8<NonEmpty>` on the upstream
  connect — a panic in debug and test builds. No fixture configures one.
- **Per-filter-chain ALPN is inexpressible** (`CF-112-4`). ALPN is a
  `rustls::ServerConfig` property and `DownstreamTls::from_listener` builds one
  config per listener, so when several chains disagree the FIRST chain's
  non-empty list wins for the whole listener and a warning is logged. Upstream
  Envoy's own per-chain-vs-per-listener semantics are UNMEASURED, so this is an
  unadjudicated gap rather than a known divergence.
- **ALPN × SNI-selected filter chains is UNMEASURED** (`CF-112-3`).
- **No ALPN-related stat is asserted.** No `ssl.*` counter is compared on this
  surface; the negotiated protocol is read from the completed handshake only.
- **Advertising `h2` does not mean serving it.** `ConfigError::Http2OverTlsNotSupported`
  is NOT lifted (`CF-112-1`): every ALPN fixture is `tcp_proxy`, and a listener
  that negotiates `h2` over TLS and then speaks HTTP/2 is a later phase.
```

- [ ] **Step 2: Verify placement and that nothing else moved**

```bash
grep -n '^## ' docs/envoy-rust/BEHAVIOR_CONTRACT.md | sed -n '/ALPN/,+2p'
git diff --numstat docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

Expected: `## ALPN` appears between `## Response trailers` and
`## Header allow-list`, and the numstat is **`117 0`** (116 section lines plus one
blank separator) — an insertion with **zero deletions**. Any deletion means an
existing section was damaged.

- [ ] **Step 3: Check the section's own citations**

```bash
sed -n '/^## ALPN/,/^## Header allow-list/p' docs/envoy-rust/BEHAVIOR_CONTRACT.md \
  | grep -oE '[a-z_/.-]+\.rs:[0-9]+'
```

Expected: **no output.** This section deliberately cites no `file:line` — it
names symbols and files only. That is a direct response to **CF-112-12**: a
contract section is read for years, and a line-numbered citation in it is a
future stale pointer.

- [ ] **Step 4: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 112.2 task 7: BEHAVIOR_CONTRACT.md gains the ALPN section"
```

---

## What state 4 will have to verify (not this plan's work — written so it is not rediscovered)

`SPEC.md` §9 instantiates the §7.5 gate. Two figures in it need care:

- **(b) covers 89, not 90.** 90 fixture directories exist today (re-measured at
  `2fcc651`: `ls -d tests/fixtures/*/ | wc -l` = 90, `git ls-files` agrees, and
  `ls tests/differential/tests/*.rs | wc -l` = 90). `0004` becomes a CHANGED
  fixture under (a). After this phase there are **92** fixtures and **91**
  runners — the counts stop matching, which is expected: `tls_alpn.rs` drives two
  fixtures. **Census differential work by RUNNER FILE NAME**, and remember the CI
  log carries the crate-relative `Running tests/<n>.rs`.
- **The CI identity is predicted to move `passed` by +9 and `binaries` by +1.**
  The +9 is **7** new unit tests in `lib.rs` (Task 1 five, Task 2 one, Task 3
  one) plus **2** integration tests (`tls_alpn_fixture`,
  `tls_alpn_server_preference_fixture`). The +1 binary is `tls_alpn.rs`, a new
  test target. Baseline at `2fcc651` is `binaries=167 passed=2265 failed=0`, so
  the prediction is **`binaries=168 passed=2274 failed=0`**. **Nothing is
  renamed and nothing is deleted here**, which is what made `ADR-0187`'s
  prediction wrong (a renamed test was double-counted, and a rename adds zero).
  Predict, then reconcile; if the number differs, suspect the prediction before
  suspecting the run.
- **(d) is vacuous by construction** — no new fuzz target, no `ci.yml` edit.
  `112.1` shipped the corpus seed.
- **(c)** h2spec at `PASS_RATE_GATE = 0.95`, `known-failures.txt` **untrimmed**
  (21 lines, md5 `19cd44d86a8b15d825f76c6e7b265e65`). Locally the runner
  self-skips silently and still reports `ok`; CI is authoritative (`ADR-0163`).

---

## Self-Review

**1. Spec coverage.** Every `SPEC.md` §1 deliverable maps to a task:

| §1 deliverable | task |
|---|---|
| 1 — client offer + `expected_alpn` assertion on the existing TLS drivers | 1, 2, 3 |
| 2 — NEW `tests/fixtures/0091-tls-alpn/` | 4 |
| 3 — NEW `tests/fixtures/0092-tls-alpn-server-preference/` | 5 |
| 4 — cell 6 on the EXISTING `0004-tls-downstream` | 6 |
| 5 — `tests/differential/tests/tls_alpn.rs` | 4 (created), 5 (second test) |
| 6 — `BEHAVIOR_CONTRACT.md` ALPN section | 7 |
| 7 — the parent-112 close-out | **NOT this plan** — §5 state 6, a separate session |

Design decisions E1–E7 are all discharged: E1 (per-probe offer/expectation)
Tasks 1+3; E2 (`AlpnRule`'s explicit negative arm) Task 1; E3 (per-side
assertion, equivalence as the conjunction — no `assert_equivalence` change)
Tasks 2+3; E4 (`tcp_proxy`, zero HCM) Global Constraints + Tasks 4/5; E5 (reuse
the `rcgen` PKI unchanged) Tasks 4/5 Step 2, by copying `0004`; E6 (reachable
backend) Global Constraints + Tasks 4/5 Step 2; E7 (assert no `ssl.*` stat) —
no task asserts one, and the contract section says so.

**2. Placeholder scan.** No `TBD`, no "implement later", no "similar to Task N",
no "add appropriate error handling". Every code step carries the literal code,
and every fixture file's full content appears in the plan. The one place a step
says "or copy the tree" (Task 4 Step 8's worktree seeding) is a genuine choice
between two equivalent mechanics, both spelled out.

**3. Type consistency.** Checked across tasks: `AlpnRule::Selected { protocol:
String }` and `AlpnRule::NoneSelected` are spelled identically in Tasks 1, 2, 3,
4, 5 and 6; `client_alpn` is `Vec<String>` on both carriers and `&[String]` in
`drive_tls`; `expected_alpn` is `Option<AlpnRule>` on both carriers,
`Option<&AlpnRule>` as a parameter, and reaches `check_alpn` as `&AlpnRule`;
`check_alpn(negotiated: Option<&[u8]>, rule: &AlpnRule) -> Result<()>` matches
`rustls`' `alpn_protocol() -> Option<&[u8]>` exactly. The YAML tag is
`kind: selected` / `kind: none_selected` everywhere.

**4. The plan's own code was RUN, not written.** Every Rust and YAML block above
was taken from a scratch-worktree tree that compiled, passed
`clippy --all-targets --all-features -- -D warnings`, passed
`cargo fmt --all -- --check`, ran 176 `differential` lib tests green, and parsed
all three real fixture files through `load_expectations` into the exact expected
shapes. The three task boundaries were gated separately, and the tempting bad cut
was built and shown to fail. **The one claim this plan made without measuring —
that `#[allow(clippy::too_many_arguments)]` was required — was then measured and
is FALSE; the plan and the LoC row were corrected rather than left standing.**

**5. What this plan does NOT do**, restated so the executor does not drift:
no crate change; no new dependency; no `ROADMAP.md` edit (the row flips at state
6); no carry-forward fix; no `stop` file; no edit to any landed artifact
including `SPEC.md`; and no parent-112 close.

---

## Next state

**§5 state 3 — the implementation — is a SEPARATE session** (§5.1; `ADR-0127`:
the context that writes an artifact must not grade it). It runs
`superpowers:executing-plans` or `superpowers:subagent-driven-development`, TDD
per task, appending to `PROGRESS.md` on each task completion.

This session wrote `PLAN.md`, `ADR-0189` and the `STATE.md` advance, and
**nothing else**. It landed no code, created no fixture, ran no §7.5 gate,
touched no `ROADMAP.md` row, and **fixed nothing** (§6.3; `ADR-0165`).
