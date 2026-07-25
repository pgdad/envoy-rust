# Phase 75.1 — `HeaderMatcher` absence semantics: the MODE-SCOPED engine fix + the ROUTE-path differential witness

> **What this document is.** The `SPEC.md` for sub-phase **75.1**, created by the
> §6.1 SPLIT of phase 75 at that phase's §5 state-2 PLAN-write (ADR-0157). It
> redistributes the parent `SPEC.md`
> (`docs/envoy-rust/phases/75-headermatcher-absence-parity/SPEC.md`, which stays
> on disk as the parent record and is NOT edited) per `BOOTSTRAP_PROMPT.md` §6.2
> step 3, and it folds in the RE-MEASURED evidence and the three corrections that
> the state-2 session produced (ADR-0158).
>
> **Written for a stranger with zero prior context (D-3.4).** Every behavioral
> claim below was MEASURED against `envoyproxy/envoy:v1.33.0` (the
> `ENVOY_TARGET.md` pin) at the phase-75 state-2 PLAN-write on
> `HEAD == 5d78df443461d002db5ce9cc9d6b238fe1de6b66`, or is cited to a file:line
> verified on disk in that same session. No Envoy C++ source was read (D-3.3).
>
> **The next session is this sub-phase's §5 state-2 PLAN-write** (`SPEC.md`
> exists, `PLAN.md` does not). It writes `PLAN.md` for 75.1 ONLY.
>
> **READ §2 BEFORE TOUCHING `crates/envoy-config/src/matcher.rs`.** The naive
> uniform "absent ⇒ DROP" fix BREAKS a MEASURED PARITY case and mints a NEW
> divergence. Phase 72 already proved that by mutation check.

---

## §1. Goal

Replace the UNIFORM `mode_result ^ self.invert_match` at
`crates/envoy-config/src/matcher.rs:52` with the MEASURED **mode-scoped** rule,
closing TWO silent runtime divergences in the single `HeaderMatcher` engine that
five subsystems share, and witness the change cross-proxy with ONE new
backend-free differential fixture on the **route** path (`0083`).

The **access-log** path's differential witness is sub-phase **75.2** — see §7 for
why the two paths cannot share one fixture, and §8 for why that is nonetheless a
complete, §6.3-compliant slice.

---

## §2. The MEASURED rule, and the guard the fix must not break

### 2.1 The rule

```
present := the named header is present in the request
           (name matched case-insensitively; an EMPTY VALUE still counts as PRESENT)

if mode is present_match(want):
        result = (present == want) XOR invert_match
else if not present:
        result = false                    # <-- invert_match is NOT applied
else:
        result = mode_matches(value) XOR invert_match
```

### 2.2 What the in-tree engine gets wrong — and right

The whole subject of this sub-phase is one expression. `matcher.rs:22-53` today:

```rust
        let mode_result = match &self.mode {
            /* ... six value modes ... */
            HeaderMatcherMode::PresentMatch(want_present) => {
                // present_match: true  → header must be present
                // present_match: false → no presence requirement (always true)
                // SPEC §6 signpost 7.
                if *want_present { value.is_some() } else { true }
            }
            /* ... */
        };

        mode_result ^ self.invert_match          // <-- matcher.rs:52
```

- **D1** (= the recorded carry-forward **CF-72-1**). A VALUE matcher
  (`exact_match` / `prefix_match` / `suffix_match` / `safe_regex_match` /
  `range_match` / `string_match`) + `invert_match: true` + ABSENT header:
  upstream returns `false` (**DROP** — the missing header short-circuits BEFORE
  the inversion); the in-tree engine computes `false ^ true` = **KEEP**.
  Confirmed on ALL SIX value modes.
- **D2** (recorded first at the phase-75 state-0/1 pick; strictly WORSE than D1).
  Upstream `present_match: false` means **"the header must be ABSENT"**. The
  in-tree engine models it as *unconditionally true* (`matcher.rs:47`), and the
  doc comment at `matcher.rs:44-45` states that wrong rule verbatim. It fires on
  a **plain, NON-inverted, single-line** matcher.
- **P1 — THE GUARD.** `present_match: true` + `invert_match` is **FULL PARITY**
  in both cells (absent → KEEP on both; present → DROP on both). Here the XOR
  *is* upstream's behavior. **A naive uniform "absent ⇒ DROP" fix breaks this and
  introduces a NEW divergence.** Phase 72 proved it by mutation check
  (`docs/envoy-rust/phases/72-accesslog-header-filter/PROGRESS.md:355-360`). The
  guard tests are `matcher.rs:425` and `matcher.rs:463`; the latter's own doc
  comment instructs the fixer to preserve it.

### 2.3 The evidence — RE-MEASURED at the state-2 PLAN-write (both proxies, live)

Method: one HCM listener, `clusters: []`, `direct_response` routes only; per
probe id `pNN` an ordered route PAIR on prefix `/pNN` — the first carrying the
`HeaderMatcher` under test and answering `pNN=MATCH`, the second a catch-all
answering `pNN=NOMATCH`. Upstream `envoyproxy/envoy:v1.33.0` in Docker with `-p`
PORT-MAPPING (**not** `--network host` — the host-net namespace is not shared on
this host); envoy-rust as the DEBUG `target/debug/envoy-bin` host subprocess,
rebuilt first (a stale binary mis-reports). Backend-free throughout.

`U` = upstream, `R` = envoy-rust. **✗ = DIVERGENCE.** The `x-a:(empty)` column
is `curl -H "x-a;"`, which sends `x-a:` with an EMPTY VALUE.

| probe | matcher | absent (U/R) | `x-a: v` (U/R) | `x-a: zzz` (U/R) | `x-a: 5` (U/R) | `x-a:(empty)` (U/R) |
|---|---|---|---|---|---|---|
| p01 | `exact_match: v` + invert | **NOMATCH / MATCH ✗** | NO / NO | MATCH / MATCH | MATCH / MATCH | MATCH / MATCH |
| p02 | `prefix_match: v` + invert | **NOMATCH / MATCH ✗** | NO / NO | MATCH / MATCH | MATCH / MATCH | MATCH / MATCH |
| p03 | `suffix_match: v` + invert | **NOMATCH / MATCH ✗** | NO / NO | MATCH / MATCH | MATCH / MATCH | MATCH / MATCH |
| p05 | `safe_regex_match: v` + invert | **NOMATCH / MATCH ✗** | NO / NO | MATCH / MATCH | MATCH / MATCH | MATCH / MATCH |
| p06 | `range_match: [1,10)` + invert | **NOMATCH / MATCH ✗** | MATCH / MATCH | MATCH / MATCH | NO / NO | MATCH / MATCH |
| p09 | `string_match: {exact: v}` + invert | **NOMATCH / MATCH ✗** | NO / NO | MATCH / MATCH | MATCH / MATCH | MATCH / MATCH |
| p07 | `present_match: true` + invert | MATCH / MATCH | NO / NO | NO / NO | NO / NO | NO / NO |
| p08 | `present_match: false` + invert | NO / NO | **MATCH / NOMATCH ✗** | **MATCH / NOMATCH ✗** | **MATCH / NOMATCH ✗** | **MATCH / NOMATCH ✗** |
| p10 | `exact_match: v` | NO / NO | MATCH / MATCH | NO / NO | NO / NO | NO / NO |
| p11 | `present_match: true` | NO / NO | MATCH / MATCH | MATCH / MATCH | MATCH / MATCH | MATCH / MATCH |
| p12 | `present_match: false` | MATCH / MATCH | **NOMATCH / MATCH ✗** | **NOMATCH / MATCH ✗** | **NOMATCH / MATCH ✗** | **NOMATCH / MATCH ✗** |
| p13 | `string_match: {exact: v}` | NO / NO | MATCH / MATCH | NO / NO | NO / NO | NO / NO |
| p14 | `range_match: [1,10)` | NO / NO | NO / NO | NO / NO | MATCH / MATCH | NO / NO |

**65 cells; 14 diverge** — 6 (D1, the absent cell of every value mode) + 4 (p08,
every PRESENT cell) + 4 (p12, every PRESENT cell). Every other cell is parity.

Read off, beyond what the parent SPEC recorded:

- The parent SPEC measured the empty-value control for `present_match: true`
  ONLY. **It is now measured for all 13 probes and is FULL PARITY except p12**
  (which is D2). p11/p07 confirm an empty value counts as PRESENT on BOTH
  proxies; p01/p06 confirm a present-but-non-matching empty value takes the
  value-matcher path on both. The `present`/`absent` axis is **presence, not
  emptiness** — the fix must not conflate them.
- p12 is **NOT inverted**. This is the whole reason D2 is worse than D1.

---

## §3. Blast radius — RE-DERIVED on the live tree (PV-3, zero line drift)

`HeaderMatcher::matches` (`crates/envoy-config/src/matcher.rs:22`, XOR at `:52`)
is evaluated at **exactly five production call sites**, spanning **five
subsystems** in **three crates**. Every line below was re-verified at state-2:

| # | call site | subsystem |
|---|---|---|
| 1 | `crates/envoy-http1/src/hcm.rs:2165` (`route_matches`) | Route header matching — serves **both H1 and H2** (H2 has no independent walker: `crates/envoy-http2/src/hcm.rs:475` calls `envoy_http1::hcm::resolve_route`) |
| 2 | `crates/envoy-filter/src/rbac.rs:60` (`eval`) | HTTP RBAC filter (permissions **and** principals) |
| 3 | `crates/envoy-filter/src/fault.rs:76` (`header_gate_matches`) | HTTP fault filter header gate |
| 4 | `crates/envoy-filter/src/jwt_authn.rs:185` (`route_match_matches`) | JWT authn requirement-rule matching |
| 5 | `crates/envoy-accesslog/src/filter.rs:139` (`LogFilter::should_log`) | Access-log `header_filter` |

Plus ONE pure delegation seam that is not a subsystem:
`crates/envoy-config/src/matcher.rs:69`, the
`impl envoy_accesslog::HeaderMatch for HeaderMatcher` required by ADR-0150
(`envoy-accesslog` must not depend on `envoy-config` — cycle), whose trait object
is injected at `crates/envoy-http1/src/hcm.rs:1784-1786` inside
`compile_access_log_filter`.

**NOT in the blast radius.** Network RBAC: `crates/envoy-bin/src/network_rbac.rs`
is an independent L4 evaluator whose `Permission::Header` / `Principal::Header`
arms bind `(_)` and return `false` behind a `debug_assert!`
(`network_rbac.rs:125-131` and `:151-157` — note the Permission arm is at
`:125-131`, NOT the `:123-129` the parent SPEC cited). It never calls
`HeaderMatcher::matches`. Also out: `cors` and `csrf` (they call
`StringMatcher::matches` — `cors.rs:132`, `csrf.rs:151` — a DIFFERENT engine),
`local_rate_limit`, `header_to_metadata`, `cdn_loop`, `buffer` (no
`HeaderMatcher` at all).

---

## §4. Scope

### 4.1 In scope

1. **The engine fix.** `HeaderMatcher::matches`
   (`crates/envoy-config/src/matcher.rs:22-53`) restructured to the §2.1 rule:
   `present_match` evaluates `(present == want) ^ invert_match`; every other mode
   short-circuits to `false` when the header is absent and applies
   `^ invert_match` only when it is present.
2. **Four WRONG or now-stale doc comments corrected**, each citing the
   measurement:
   - `matcher.rs:44-45` — states `present_match: false → no presence requirement
     (always true)`. **This is D2's rule, verbatim.**
   - `matcher.rs:61-63` — the ADR-0150 seam doc asserts `mode_result ^
     invert_match, incl. absent+invert = keep` as a design GUARANTEE of the seam.
   - `crates/envoy-config/src/bootstrap.rs:3142-3143` — the
     `HeaderMatcherMode::PresentMatch` variant doc repeats the wrong rule
     (`"no presence requirement" (false; SPEC §6 signpost 7 for the subtle false
     semantics)`).
   - `crates/envoy-config/src/bootstrap.rs:3119-3121` — the `invert_match` field
     doc says the result is inverted "(XOR after the mode match runs)", which is
     no longer unconditional.
   - Plus `crates/envoy-accesslog/src/filter.rs:135-138`, whose comment states
     "PV-4's `mode_result ^ invert_match` is preserved because the injected impl
     calls `HeaderMatcher::matches` verbatim". The delegation stays true; the
     asserted semantics do not.
   > **Doc-comment hazard (memory `mechanical-fanout-scripts-corrupt-doc-comments`).**
   > `cargo fmt` does NOT reflow `///` / `//!` / `//` lines, so nothing catches a
   > mis-wrapped or semantically-backwards comment. Wrap-check every touched
   > comment BY HAND and grep the commit's `+` lines for `///`.
3. **THREE divergence-encoding tests AMENDED** (the parent SPEC said two — see
   §6, correction C1):
   - `pv4_value_matcher_absent_plus_invert_kept_diverges_from_upstream`
     (`matcher.rs:432`) — asserts D1 TWICE, at `:449` (inherent engine) and
     `:457` (trait object). Its 12-line comment (`:433-445`) records the
     divergence as accepted and carries a stale `matcher.rs:51` citation.
   - `header_match_trait_delegates_to_inherent_engine` (`matcher.rs:489`) — a
     THIRD copy of the divergent assertion at `:503`.
   - **`present_match_false_returns_true_when_present` (`matcher.rs:342`)** —
     found at state-2. Constructs `PresentMatch(false)`, asserts `matches(&[h(
     "authorization", "Bearer x")])` is **true**, and its comment at `:343`
     states the wrong rule (`// Subtle: present_match: false is "no presence
     requirement", always true.`). Under the §2.1 rule this becomes `(present ==
     want) = (true == false) = false`. **This is the test that pins D2, and the
     parent SPEC's R-0.6 claim that "`present_match: false` has no behavioral
     test anywhere" is FALSE.**
   All three are renamed to describe PARITY rather than divergence.
4. **The guard tests kept GREEN and strengthened** —
   `invert_match_inverts_present_match_result` (`matcher.rs:425`) and
   `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream`
   (`matcher.rs:463`). Also `present_match_false_returns_true_when_absent`
   (`matcher.rs:348`), which yields the RIGHT answer for the WRONG stated reason:
   `(false == false) = true` still KEEPs, but its rationale ("no presence
   requirement") must be restated. And `matcher.rs:330`'s
   `// PresentMatch: 4 cells` comment, still 4 cells but with two expectations
   flipped.
5. **A full in-process engine matrix** covering every cell of §2.3: seven modes ×
   {absent, present-matching, present-non-matching} × {invert, no-invert}, plus
   the **empty-header-VALUE control** pinning presence-not-emptiness. This is the
   coverage whose absence let D2 survive.
6. **Consumer-level in-process tests proving the fix propagates** through all
   five call sites of §3 — the route walker (H1 **and** H2, the latter via
   `resolve_route`), HTTP RBAC, the fault header gate, the JWT-authn rule
   matcher, and the access-log `header_filter` seam **via
   `Arc<dyn HeaderMatch>`** so the ADR-0150 trait object is exercised and not
   only the inherent method. Existing analogues to follow:
   `crates/envoy-http1/src/hcm.rs:5004`
   (`header_filter_membership_across_modes_and_absent_drop`),
   `crates/envoy-http2/src/hcm.rs:3573`, `crates/envoy-filter/src/fault.rs:139`.
7. **NEW differential fixture `0083`** — route path, `kind: http1_probe_list`,
   backend-free (`clusters: []`, `direct_response` only). See §5.
8. **ONE ~19-line test entrypoint** under `tests/differential/tests/`, per the
   §5.3 stencil.
9. **`BEHAVIOR_CONTRACT.md` — the §C rewrite and two corrections.** See §6.

### 4.2 Out of scope

- **The ACCESS-LOG path's differential fixture** — sub-phase **75.2**. The engine
  fix in this sub-phase changes access-log behavior too (call site 5), and that
  change IS covered here in-process through the trait object (§4.1 item 6); what
  75.2 adds is the cross-proxy witness. See §7-§8.
- **The `present_match`-polarity `BEHAVIOR_CONTRACT.md` subsection, the CF-75-1
  row, and the CF-72-2 row updates** — 75.2 (they travel with the second
  witness).
- **CF-72-2's three members** — name-only `{ name }`,
  `treat_missing_header_as_empty`, the top-level `contains_match` arm. All are
  REJECT-direction load-parity gaps (envoy-rust boot-fatals, so a config that
  would behave differently never runs), all need NEW config surface, and
  decisively **none can appear in a differential fixture until implemented**
  because the fixture would not boot on the subject side.
- **`exact_match: ""`** degenerating to a presence match upstream — carry-forward
  **CF-75-1**.
- **Any change to the five call sites themselves.** The fix is inside the shared
  engine. Their behavior changes, which is why §4.1 item 6 tests them, but no
  call-site code is edited.
- **A new fuzz target, corpus seed, or `ci.yml` step.** See §9.
- **Editing any landed ADR** (append-only, D-3.5) or the parent
  `75-headermatcher-absence-parity/SPEC.md` (a frozen artifact).

---

## §5. Fixture `0083` — the ROUTE-path witness

### 5.1 Shape

`tests/fixtures/0083-headermatcher-absence-parity/` with the four house files
(`envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`).

One H1 HCM listener, `clusters: []`, `direct_response` routes only — so no
backend container spawns. Backend-free-ness is decided by a text scan for the
`{{BACKEND_PORT}}` template marker (`tests/differential/src/lib.rs:3322-3330`);
this fixture carries none.

Per matcher under test, an ordered route PAIR on prefix `/pNN`: the first carries
the `HeaderMatcher` and answers `pNN=MATCH`, the second is a catch-all answering
`pNN=NOMATCH`. Discrimination is by `direct_response` **body**, byte-exact.

**Eight matchers**, chosen so every distinct code path in §2.1 is witnessed and
the guard is pinned:

| id | matcher | witnesses |
|---|---|---|
| p01 | `exact_match: "v"` + `invert_match: true` | **D1** — the plain value-matcher case |
| p06 | `range_match: {start: 1, end: 10}` + `invert_match: true` | **D1** on the numeric parse path |
| p09 | `string_match: {exact: "v"}` + `invert_match: true` | **D1** through the `StringMatcher` delegation |
| p08 | `present_match: false` + `invert_match: true` | **D2**, inverted |
| p12 | `present_match: false` | **D2**, NON-inverted — the worst cell |
| p07 | `present_match: true` + `invert_match: true` | **P1 — THE GUARD.** Must stay MATCH-on-absent |
| p10 | `exact_match: "v"` | parity control |
| p11 | `present_match: true` | parity control + the empty-value presence cell |

Driven with, per matcher, the request variants that discriminate: no `x-a`;
`x-a: v`; `x-a: zzz`; plus `x-a: 5` for p06 and an EMPTY-VALUE `x-a;` probe on
p11/p12 (the presence-not-emptiness pin). Expected values are read directly off
the §2.3 table's **upstream** column — which, after the fix, is also
envoy-rust's.

**This is the FIRST differential witness of `invert_match` and of
`HeaderMatcher.present_match` in the entire 82-fixture corpus** (PV-5, re-run at
state-2: `grep -rl invert_match --include=*.yaml --include=*.yml tests/` → **0
files**; the only `present_match:` in fixture YAML is
`tests/fixtures/0044-http-rbac-matcher-value-enrichment/envoy-rust.yaml:77` and
`envoy.yaml:94`, both a `ValueMatcher` on RBAC **metadata** — a DIFFERENT
message, see §10 Trap A).

### 5.2 `expectations.yaml` schema (re-verified at state-2; `deny_unknown_fields` throughout)

`Driver::Http1ProbeList` is declared at `tests/differential/src/lib.rs:119` and
selected by `kind: http1_probe_list`. Its probe type
(`Http1Probe`, `lib.rs:1142-1165`):

| YAML key | type | required | default |
|---|---|---|---|
| `name` | `String` | **yes** | — |
| `method` | `get` \| `options` \| `post` | **yes** | — |
| `path` | `String` | **yes** | — |
| `host` | `String` | **yes** | — |
| `extra_headers` | list of `[name, value]` pairs | no | `[]` |
| `body` | `String` | no | `null` (adds `Content-Length` automatically) |
| `expected_status` | `u16` | no | `null` (no assert) |
| `expected_body` | `{ kind: byte_exact, body: "<str>" }` | no | `null` — **`body:` is MANDATORY inside the rule** |
| `expected_headers` | scalar `set_equal_modulo_allow_list` | no | `null` |

**Sending vs omitting `x-a`.** `drive_http1` (`lib.rs:2182-2211`) emits
`extra_headers` VERBATIM and in order after `Host:`, and injects only `Host`, an
optional `Content-Length`, and `Connection: close`. So `extra_headers: [["x-a",
"v"]]` sends it and **omitting the key entirely** makes it genuinely absent on
the wire. An empty value is `["x-a", ""]`.

The working stencil (from `tests/fixtures/0007-http1-direct-response/expectations.yaml`):

```yaml
driver:
  kind: http1_probe_list
  probes:
    - name: p01-absent
      method: get
      path: "/p01"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body:
        kind: byte_exact
        body: "p01=NOMATCH"
      expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body:
    kind: byte_exact
```

### 5.3 Registration cost — ONE file (PV-7, re-confirmed)

- `tests/differential/Cargo.toml` has **no `[[test]]` stanza** — cargo
  autodiscovers `tests/*.rs`.
- The workspace root `Cargo.toml:19` already lists `tests/differential`.
- `.github/workflows/ci.yml:67` is `cargo test --workspace`.
- There is no fixture registry: `run_fixture(&dir)` takes the directory path.

So the only new Rust is one entrypoint, of which only the fn name and the
directory change:

```rust
use std::path::PathBuf;

#[tokio::test]
async fn headermatcher_absence_parity() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0083-headermatcher-absence-parity");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

House style prefixes this with a long `//!` header: opening line, phase + ADR
refs, the config shape, a probe-by-probe enumeration, and the `clusters: []` /
no-backend note.

### 5.4 Per-side config divergences (the house recipe, re-verified)

Write one config, then on the `envoy-rust.yaml` copy: (a) DROP the `admin:`
block, (b) change the listener bind `0.0.0.0` → `127.0.0.1`, (c) drop
`generate_request_id: false` if present. Keep `node:`, `codec_type: HTTP1`, the
filters and the whole route table byte-identical.

> **`codec_type` correction (C3, §6).** The parent SPEC's R-0.9 said "envoy-rust
> requires an explicit `codec_type` where upstream defaults it. Every envoy-rust
> probe config needs it." The *mechanic* is real —
> `crates/envoy-config/src/bootstrap.rs:1103` declares
> `pub codec_type: CodecType` with **no** `#[serde(default)]` under
> `deny_unknown_fields`, so a missing key is a hard parse error here while
> upstream defaults to `AUTO`. But it is **NOT a per-side divergence**: every one
> of the 82 fixtures writes `codec_type: HTTP1` on BOTH sides. Do the same.

---

## §6. `BEHAVIOR_CONTRACT.md` changes in this sub-phase

1. **Rewrite §C** (`BEHAVIOR_CONTRACT.md:2357-2377`, verified exact at state-2;
   the `### Phase 72 …` heading is at `:2334`, and the boundary a rewrite must
   NOT cross is `**§D Name-only + treat_missing_header_as_empty …**` at `:2379`).
   §C currently records D1 as an accepted, carried divergence ("Phase 72 reuses
   the engine verbatim … and does NOT fix it; the shared-engine fix is
   carry-forward **CF-72-1**"). After this sub-phase it states the §2.1 parity
   rule in full, names fixture `0083` as the pin, and records **CF-72-1 CLOSED**.
   The existing mode-dependence warning and the "a fixer MUST preserve the
   `present_match` KEEP" instruction are **KEPT** — they remain true and remain
   the guard.
   > §C also **omits D2 entirely** — there is no mention of the non-inverted
   > `present_match: false` divergence anywhere in the contract today. The
   > dedicated polarity subsection is 75.2's; §C's rewrite here must at minimum
   > stop asserting the old uniform-XOR rule.
2. **Correct `BEHAVIOR_CONTRACT.md:1878-1880`** (correction **C2**, new at
   state-2, NOT in the parent SPEC's §6). Inside the phase-36 `ValueMatcher`
   block, the contract says the RBAC rule is "a MATERIAL DIVERGENCE from the
   existing `HeaderMatcherMode::PresentMatch` (`want ? present : true`)". After
   this fix the parenthetical formula is **wrong** (`present == want`), and the
   two rules now AGREE for the present case and still DIFFER for the absent case
   (`ValueMatcher` → `false`; `HeaderMatcher` → `true`). The `ValueMatcher` rule
   itself — "**`present_match: false` NEVER matches**" — is CORRECT and must not
   be touched. Restate the comparison; do not delete it. The same stale
   parenthetical is mirrored in source at
   `crates/envoy-config/src/bootstrap.rs:1704`.
3. **Citation correction (PV-10).** The XOR is at `matcher.rs:52`, not `:51`.
   There are **26** `matcher.rs:51` citations in the repo. Correct exactly TWO of
   them: `BEHAVIOR_CONTRACT.md:2369` (inside the §C rewrite) and the in-SOURCE
   one at `crates/envoy-config/src/matcher.rs:439` (inside a test whose
   assertions are being amended anyway — the parent SPEC scoped the correction to
   "the CONTRACT only" and missed this). **Do NOT touch** the four in
   `docs/envoy-rust/DECISIONS.md` (`:2479`, `:2546`, `:2555`, `:2624`, plus
   `:2631`) — append-only, D-3.5 — nor the historical phase docs or
   `STATE_HISTORY.md`.
4. **`tests/fixtures/0078-accesslog-header-filter/README.md:69-73`** documents
   the invert+absent divergence as deferred and live. It must be updated to say
   it is CLOSED, with a pointer to `0083`.

---

## §7. Why the route and access-log paths cannot share one fixture

This is the finding that forced the split, and it REFUTES the parent SPEC's §5
design for its access-log fixture. **MEASURED on disk at state-2:**

- `AccessLogPaths` (`tests/differential/src/lib.rs:1088-1093`) is
  `{ envoy: String, envoy_rust: String }` under `deny_unknown_fields` — **exactly
  ONE log file per side.**
- `run_http1_access_log_byte_exact_arm` reads exactly those two paths
  (`lib.rs:6344`, `:6365`, `:6403-6412`) and there is no per-sink dimension
  anywhere in the arm.
- Only the **envoy-side parent directory** of that one path is bind-mounted into
  the container (`lib.rs:4019`, a single-element `vec![(envoy_parent_s.clone(),
  envoy_parent_s)]`), so a second sink writing elsewhere would not even be
  visible to the host.
- Corroborating census: across all 82 fixtures the maximum number of
  `- name: envoy.access_loggers.file` sinks in ANY config is **1**.

The parent SPEC §5 specified fixture `0084` as "multiple `FileAccessLog` sinks
whose `header_filter` carries the same matchers, each with a distinct
`text_format_source`". **That is infeasible with the "both drivers reused with
ZERO change" constraint the same SPEC asserts (its R-0.7).** One sink can carry
one `header_filter`, so one access-log fixture can witness one matcher — which is
why 75.2 needs TWO fixtures (`0084` for D1, `0085` for D2) rather than one, and
why the access-log witness is a coherent separate slice rather than a rider on
this one.

---

## §8. Why 75.1 is a complete slice (§6.3 compliance)

`BOOTSTRAP_PROMPT.md` §6.3 forbids a half that changes behavior without a
differential witness. 75.1 does not do that:

- It ships the engine fix **with** a cross-proxy differential witness on the
  route path (`0083`) — the highest-fan-out consumer, serving both H1 and H2.
- It ships in-process propagation tests across **all five** call sites including
  the access-log trait object, so no consumer's behavior change is unverified.
- The access-log path's *cross-proxy* witness lands in 75.2. That is an
  ADDITIONAL witness of an already-tested change, not deferred work — it is the
  second consumer of a rule the first fixture already pins.

75.1 is independently green, independently reviewable, and leaves no stub.

---

## §9. §7.4 fuzz disposition — CONFIRMED, not inherited

**No new fuzz target, no new corpus seed, no `ci.yml` step.** Re-derived at
state-2 rather than inherited:

- This sub-phase introduces no parser, codec or filter, and adds **no config
  surface**: `HeaderMatcher` already carries `name` + `mode` + `invert_match`
  (`bootstrap.rs:3104-3123`), `PresentMatch(bool)` is already a variant
  (`:3144`), `present_match: false` already deserializes (`:3236-3239`) and
  already validates (`validate_header_matcher`, `:5555-5586`, whose
  `PresentMatch` arm at `:5567` is a no-op and which never inspects
  `invert_match`). **The entire fix is behavioral.**
- The existing `parse_bootstrap` target already covers the unchanged
  deserializer — and it is **parse-only**: it never calls
  `HeaderMatcher::matches`, so no corpus seed can encode runtime semantics at
  all. Of 57 corpus files containing `present_match`, exactly **2 are tracked**
  (`route_with_header_matchers.yaml:33`, a HeaderMatcher `present_match: true`;
  `rbac_present_match.yaml:45`, a ValueMatcher one) and **`present_match: false`
  appears in ZERO of them**.
- **No corpus seed and no fixture YAML can break**: `invert_match` appears in
  **zero** `.yaml`/`.yml` files anywhere in the repo, including the fuzz corpus.

(Both omissions are otherwise easy to miss: a new target is not auto-discovered
and needs a hand-written `ci.yml` step, and a new seed needs an explicit
`!`-un-ignore line or it is silently untracked.)

---

## §10. Risks and traps

**The dominant §7.5 risk is gate (b), not gate (a).** This is a shared-engine
behavior change under five subsystems. The new fixture will pass; the danger is a
PRE-EXISTING fixture or in-process test that silently depended on the old
semantics. **PV-9, run exhaustively at state-2, found the complete break set —
and it is entirely in-process tests, no fixtures:**

| site | verdict |
|---|---|
| `matcher.rs:43-48` | the engine arm — the fix itself |
| `matcher.rs:342-346` | **WOULD BREAK** — `present_match_false_returns_true_when_present` |
| `matcher.rs:448-451`, `:456-459` | **WOULD BREAK** — the two D1 assertions |
| `matcher.rs:503` | **WOULD BREAK** — the third D1 assertion |
| `matcher.rs:348-351` | right answer, wrong stated reason |
| every fixture YAML | **ZERO RISK** — no HeaderMatcher `present_match`, no `invert_match` anywhere |
| every fuzz corpus seed | **ZERO RISK** — parse-only target |
| all `Default` paths | **ZERO RISK** — neither `HeaderMatcher` nor `HeaderMatcherMode` derives `Default`, and the deserializer requires exactly one mode key, so nothing can silently produce `PresentMatch(false)` |
| the whole `ValueMatcher` / RBAC / metadata-filter surface | **ZERO RISK** — different type, different (correct) rule |

Watch nonetheless at gate (b): `0007-http1-direct-response` (the only other
route-header-matching witness, non-inverted `exact_match`), `0017-http-filter-rbac`,
`0018-http-filter-fault`, and `0078`-`0082` (the access-log filter family).

**Honor TDD's RED with a MUTATION CHECK.** Break the mode-scoping — preferring
the exact mistake §2.2 warns about, a uniform absent-DROP — and watch
`pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream`
(`matcher.rs:463`) go RED, then revert. Record it as the RED evidence. Run it in
a **scratch `git worktree`**, never in the main tree with parallel subagents
active (a parallel reviewer's `git checkout --` silently reverts an in-place
mutation mid-run → FALSE GREEN). Grep the run for `Compiling` before believing a
green; note `cargo clippy` prints `Checking`, not `Compiling`. And a mutation RED
is not automatically a SEMANTIC red — read the failure TEXT and run the
UNMUTATED control from the same tree first.

**TWO CONFLATION TRAPS — do NOT unify them.**

- **Trap A — two different `present_match` fields.**
  `HeaderMatcher.present_match` (this sub-phase) and `ValueMatcher.present_match`
  (RBAC / access-log metadata) are different messages with different MEASURED
  rules. `crates/envoy-config/src/bootstrap.rs:1704` and
  `BEHAVIOR_CONTRACT.md:1863-1885` record that for the `ValueMatcher` one
  **`present_match: false` NEVER matches** — a DIFFERENT and CORRECT rule.
  Confusingly, after this fix the two rules AGREE for the present case and still
  DIFFER for the absent case. Do not collapse them; do restate the stale
  comparison per §6 item 2.
- **Trap B — two different `invert` fields.** `HeaderMatcher.invert_match` (this
  sub-phase) and `MetadataMatcher.invert` (CF-74-1) are unrelated. The latter is
  MEASURED accepted-but-INERT upstream and stays boot-fatal here; "implementing"
  it would CREATE a divergence.

**Recon traps that cost real time on this host.**

- **Docker bind mounts are STALE-CACHED.** After editing a config in a
  bind-mounted directory the container keeps reading the PREVIOUS contents. **Use
  a FRESH FILENAME for every config revision**; never edit in place and re-run.
- **`--volumes-from` does not retrieve a stopped container's `/tmp`** — use
  `docker cp <container>:/path ./local`.
- **Upstream Envoy will not create a log directory** — a `path:` under a
  nonexistent dir is a boot-fatal `unable to open file … No such file or
  directory`. (The differential harness creates and `chmod 0o777`s both parent
  dirs itself, so this bites only hand-rolled probes.)
- **YAML 1.1 booleans.** An unquoted `cluster: y` in `node:` parses as boolean
  `true`; upstream's JSON-proto path then rejects the bootstrap with
  `@ node.cluster: string, … unexpected character 't'`. Quote scalar node
  fields in hand-rolled probe configs.
- Port-map upstream with `-p`, never `--network host` (the host-net namespace is
  not shared on this host). Rebuild `cargo build -p envoy-bin` before ANY local
  differential — the harness runs `target/debug/envoy-bin`.

---

## §11. Differential surface at sub-phase end

- **NEW fixture `0083-headermatcher-absence-parity`** — green cross-proxy. First
  differential witness of `invert_match` and of `HeaderMatcher.present_match` in
  the corpus.
- **All 82 pre-existing fixtures stay green** (§7.5 gate (b) — the real risk
  surface).
- **Conformance:** unchanged. h2spec stays at its declared threshold;
  `known-failures.txt` stays **21** lines and is NEVER trimmed (this host scores
  h2spec 3.5/2 as PASS, so trimming on local evidence would break CI).

---

## §12. Estimated size (§6.1 gate for THIS sub-phase)

| Area | Net LoC |
|---|---|
| Engine restructure + the five corrected doc comments (§4.1 items 1-2) | ~55 |
| Amend 3 divergence-encoding tests + strengthen 3 guards (§4.1 items 3-4) | ~95 |
| In-process engine matrix, 7×3×2 + empty-value control (§4.1 item 5) | ~180 |
| Consumer propagation tests across the 5 call sites (§4.1 item 6) | ~200 |
| Fixture `0083`: 2 configs (~118 each) + `expectations.yaml` (~200) + README (~120) | ~560 |
| Test entrypoint incl. the house `//!` header | ~30 |
| `BEHAVIOR_CONTRACT.md` §C rewrite + C2 correction + citations + the 0078 README | ~90 |
| **Total** | **~1210 net LoC / ~12-14 tasks** |

Under the ~1500 LoC / ~25 task gate. Basis: the fixture line is MEASURED against
comparables on disk — `0007` is 183 lines total for 1 matcher / 2 probes, `0081`
is 352 and `0082` 264; `0083` carries 8 matchers and ~24 probes.

---

## §13. ADR pointers

- **ADR-0156** — the phase-75 pick (state-0/1), including the measured basis
  (D1 + D2 + P1) and the scope line (silent runtime divergences IN,
  reject-direction load-parity gaps OUT).
- **ADR-0157** — the §6.1 SPLIT of phase 75 into 75.1 + 75.2, with the
  re-derived size number.
- **ADR-0158** — the §6.2 empirical reconciliation: the three corrections to the
  parent SPEC (C1 the third divergence-encoding test, C2 the
  `BEHAVIOR_CONTRACT.md:1878-1880` staleness, C3 the `codec_type` non-divergence)
  plus the single-log-file driver constraint of §7 and the extended empty-value
  measurement of §2.3.

---

## §14. Carry-forwards

**CONSUMED by this sub-phase (if it lands as scoped):** **CF-72-1** — the
shared-engine value-matcher `absent + invert` divergence, closed by the D1 half
of the fix. D2 closes with it; D2 had no carry-forward id (it was unrecorded
before the phase-75 pick).

**Carried to 75.2:** the `present_match`-polarity contract subsection,
**CF-75-1**, the **CF-72-2** contract-row updates, and **M74-31** (the five-site
"placed SECOND **so** the last probe is KEPT" non-sequitur, which travels with
the access-log fixtures).

**Untouched, carried forward:** CF-72-2, CF-74-1/2/3/4/6, CF-73-1, N73-R2,
M73-R1/M73-R2, M71-3, M71-6/7/8, M70-R4/R9, M69-A..I, CF-69-1/2/3/5, M68-1, M-1,
CF-67-3/5/6/7, M74-3..M74-14, M74-16, M74-17/18/20/21/22/26/27/28/29,
M74-30..M74-39, the older Minors in `67.3/SPEC.md` §10, and the
HTTP-filters-family (1)-(4) in `STATE_HISTORY.md`.

**Documentary, recorded forward:** the parent SPEC's R-0.4 correction to
`DECISIONS.md:2448`'s "FIVE call sites across FOUR subsystems" phrasing (five
call sites, five subsystems, three crates) stands, and the landed ADR stays
un-edited. Additionally the `network_rbac.rs` Permission-arm citation is
`:125-131`, not the `:123-129` the parent SPEC recorded.
