# Fixture 0083 — `HeaderMatcher` absence semantics: the ROUTE-path parity witness

Phase **75.1** (sub-phase of parent 75, created by the §6.1 split — ADR-0157).
Driver `http1_probe_list`. Docker-gated. **Backend-free.**

This is the **FIRST differential witness of `invert_match` AND of
`HeaderMatcher.present_match` in the entire fixture corpus.** Before this
fixture, `invert_match` appeared in ZERO `.yaml` files anywhere in the repo, and
the only `present_match:` in fixture YAML was `0044`'s — a `ValueMatcher` on RBAC
**metadata**, a different message with a different (and correct) rule.

---

## 1. What this fixture witnesses

The MEASURED absence rule of the shared `HeaderMatcher` engine
(`crates/envoy-config/src/matcher.rs`), quoted in full:

```
present := the named header is present in the request
           (name matched case-insensitively;
            an EMPTY VALUE still counts as PRESENT)

if mode is present_match(want):
        result = (present == want) XOR invert_match
else if not present:
        result = false                    # <-- invert_match is NOT applied
else:
        result = mode_matches(value) XOR invert_match
```

Every expectation in `expectations.yaml` is the measured verdict of upstream
`envoyproxy/envoy:v1.33.0` (the `docs/envoy-rust/ENVOY_TARGET.md` pin) — which,
since phase 75.1, is also envoy-rust's.

Before phase 75.1 the engine applied `mode_result ^ invert_match` **uniformly**,
which produced two silent runtime divergences in a matcher shared by five
subsystems (route matching on H1+H2, HTTP RBAC, the fault header gate, JWT-authn
rule matching, and the access-log `header_filter`):

- **D1** (= carry-forward **CF-72-1**, **CLOSED** by this phase) — a VALUE
  matcher (`exact_match` / `prefix_match` / `suffix_match` / `safe_regex_match` /
  `range_match` / `string_match`) + `invert_match: true` + an **ABSENT** header:
  upstream **DROPS** (a missing header is an unconditional value no-match that
  inversion does not resurrect); envoy-rust computed `false ^ true` and **KEPT**.
- **D2** — upstream `present_match: false` means **"the header must be
  ABSENT"**. envoy-rust modelled it as *unconditionally true*. This fires on a
  **plain, NON-inverted, single-line** matcher, which is why it is worse than D1.

---

## 2. Config shape

One HTTP/1.1 HCM listener; `clusters: []`; `direct_response` routes only — so **no
backend container spawns** (backend-free-ness is decided by a text scan for the
`{{BACKEND_PORT}}` template marker, which this fixture does not carry).

**EIGHT matchers over SIXTEEN routes, as ordered PAIRS.** For probe id `pNN`
there are two routes on prefix `/pNN`: the first carries the `HeaderMatcher`
under test and answers `pNN=MATCH`; the second is an unguarded catch-all
answering `pNN=NOMATCH`. **The response body IS the matcher's verdict**, compared
byte-exact. No stats, no access log, no timing.

The probe ids are **non-contiguous on purpose** — they are the ids of the
`docs/envoy-rust/phases/75.1-headermatcher-absence-engine-route/SPEC.md` §2.3
measured matrix, so every expectation can be read straight off that table's
UPSTREAM column without re-deriving anything.

---

## 3. The eight matchers

| id | matcher | witnesses |
|---|---|---|
| p01 | `exact_match: "v"` + `invert_match: true` | **D1** — the plain value-matcher case |
| p06 | `range_match: {start: 1, end: 10}` + `invert_match: true` | **D1** on the numeric parse path |
| p07 | `present_match: true` + `invert_match: true` | **P1 — THE GUARD.** Must stay MATCH-on-absent |
| p08 | `present_match: false` + `invert_match: true` | **D2**, inverted |
| p09 | `string_match: {exact: "v"}` + `invert_match: true` | **D1** through the `StringMatcher` delegation |
| p10 | `exact_match: "v"` | parity control |
| p11 | `present_match: true` | parity control + the empty-value presence cell |
| p12 | `present_match: false` | **D2**, NON-inverted — the worst cell |

The three D1 probes are deliberately spread across three DIFFERENT value paths —
literal compare (p01), numeric parse (p06) and the `StringMatcher` delegation
(p09) — so the fix is witnessed as mode-general rather than special-cased.

---

## 4. The 22 probes

`x-a` column: **omitted** = the `extra_headers` key is absent entirely, so the
header is genuinely absent on the wire. **`(empty)`** = sent as `["x-a", ""]`.

| probe | path | `x-a` sent | expected body |
|---|---|---|---|
| `p01-absent-drops` | `/p01` | omitted | `p01=NOMATCH` |
| `p01-value-matches-so-invert-drops` | `/p01` | `v` | `p01=NOMATCH` |
| `p01-value-differs-so-invert-keeps` | `/p01` | `zzz` | `p01=MATCH` |
| `p06-absent-drops` | `/p06` | omitted | `p06=NOMATCH` |
| `p06-non-numeric-so-invert-keeps` | `/p06` | `v` | `p06=MATCH` |
| `p06-in-range-so-invert-drops` | `/p06` | `5` | `p06=NOMATCH` |
| `p07-absent-keeps-GUARD` | `/p07` | omitted | `p07=MATCH` |
| `p07-present-drops` | `/p07` | `v` | `p07=NOMATCH` |
| `p08-absent-drops` | `/p08` | omitted | `p08=NOMATCH` |
| `p08-present-keeps` | `/p08` | `v` | `p08=MATCH` |
| `p09-absent-drops` | `/p09` | omitted | `p09=NOMATCH` |
| `p09-value-matches-so-invert-drops` | `/p09` | `v` | `p09=NOMATCH` |
| `p09-value-differs-so-invert-keeps` | `/p09` | `zzz` | `p09=MATCH` |
| `p10-absent-drops` | `/p10` | omitted | `p10=NOMATCH` |
| `p10-value-matches` | `/p10` | `v` | `p10=MATCH` |
| `p10-value-differs` | `/p10` | `zzz` | `p10=NOMATCH` |
| `p11-absent-drops` | `/p11` | omitted | `p11=NOMATCH` |
| `p11-present-keeps` | `/p11` | `v` | `p11=MATCH` |
| `p11-empty-value-counts-as-present` | `/p11` | `(empty)` | `p11=MATCH` |
| `p12-absent-keeps` | `/p12` | omitted | `p12=MATCH` |
| `p12-present-drops` | `/p12` | `v` | `p12=NOMATCH` |
| `p12-empty-value-counts-as-present` | `/p12` | `(empty)` | `p12=NOMATCH` |

**Equivalence:** `response_status: exact`, `response_body: byte_exact`,
`expected_headers: set_equal_modulo_allow_list` on every probe.

### The empty-value probes and the one wire-shape caveat

`SPEC.md` §2.3 measured the empty-value column with `curl -H "x-a;"`, which puts
`x-a:` on the wire; the harness's `drive_http1` instead emits `x-a: ` (a SPACE
before CRLF). Both are an empty value, and **both byte shapes were driven at both
proxies at the state-3 implementation with all four cells agreeing** — so the two
empty-value probes are kept at full strength. They pin **presence, not
emptiness**: an empty value counts as PRESENT, so `present_match: true` MATCHes it
(p11) and `present_match: false` does NOT (p12).

---

## 5. Why `p07` is load-bearing

`p07-absent-keeps-GUARD` is the single most important probe in this fixture.

`present_match: true` + `invert_match: true` + an ABSENT header is **MEASURED
PARITY** — both proxies KEEP. Here the XOR *is* upstream's behavior, because
`present_match` is the ONLY mode that evaluates at all with the header absent,
and so the only one that carries an absent header into the inversion.

**A naive uniform "absent ⇒ DROP" fix of the shared engine passes every other
probe in this fixture and fails only `p07-absent-keeps-GUARD`.** Such a fix would
close D1 and D2 while minting a NEW divergence in their place. That is not
hypothetical: at the state-3 implementation the exact mutation (hoisting the
absent short-circuit above the `present_match` arm) was applied in a scratch
worktree and turned three in-process guards RED while leaving every value-mode
assertion green.

**Any future refactor of `HeaderMatcher::matches` must keep this probe green.**
The in-process companions are `invert_match_inverts_present_match_result` and
`pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream` in
`crates/envoy-config/src/matcher.rs`.

---

## 6. Per-side config divergences

| delta | `envoy.yaml` (upstream) | `envoy-rust.yaml` (subject) | why |
|---|---|---|---|
| `node:` block | absent | `id: x`, `cluster: y` | envoy-rust requires it; the house form used by every fixture |
| listener bind | `0.0.0.0` | `127.0.0.1` | the subject runs as a host subprocess, not in a container |
| `admin:` block | present (`port_value: 0`) | absent | the subject exposes no admin listener in this fixture |

**Everything else — the whole route table, `codec_type`, the filter chain — is
byte-identical between the two files.** Both were generated from one shared body,
so this is true by construction.

> **`codec_type: HTTP1` is written on BOTH sides and is NOT a per-side
> divergence** (ADR-0158 correction C3). The underlying mechanic is real —
> envoy-rust's `codec_type` has no serde default under `deny_unknown_fields`, so
> a missing key is a hard parse error there while upstream defaults to `AUTO` —
> but every fixture in the corpus writes it explicitly on both sides.
>
> **The unquoted `cluster: y` YAML-1.1 boolean trap does not apply here.** An
> unquoted `y` can parse as boolean `true`, and upstream's JSON-proto path would
> then reject the whole bootstrap. That trap bites hand-rolled probe configs that
> send a `node:` block to UPSTREAM. Here the `node:` block exists ONLY on the
> envoy-rust side (exactly as in `0007`, where this form is proven green), and
> envoy-rust's `serde_yaml` reads bare `y` as the string `"y"`. Do not "fix" it.

---

## 7. Cross-references

- **ADR-0156** — the phase-75 pick, including the measured basis (D1 + D2 + P1).
- **ADR-0157** — the §6.1 SPLIT of phase 75 into 75.1 + 75.2.
- **ADR-0158** — the parent's §6.2 empirical reconciliation (corrections C1/C2/C3
  and the single-log-file driver constraint).
- **ADR-0159** — sub-phase 75.1's own reconciliation: the engine restructure
  SHAPE and its two rejected alternatives, the pre-validated mutation, and the
  §7.4 disposition.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` **§C** — the contract statement of this
  rule (rewritten by this phase from an accepted-divergence record into the
  parity rule).
- `docs/envoy-rust/phases/75.1-headermatcher-absence-engine-route/` — `SPEC.md`
  (the measured matrix at §2.3), `PLAN.md`, `PROGRESS.md`.
- **Sub-phase 75.2** — the ACCESS-LOG-path witness of the same rule (fixtures
  `0084` + `0085`). Two fixtures rather than one because the byte-exact
  access-log driver takes exactly ONE log file per side, so one fixture can
  witness one matcher.

---

## 8. Deferred — NOT in this differential

| item | why not here |
|---|---|
| The **access-log path** witness | sub-phase **75.2** (`0084` + `0085`). The engine fix changes access-log behavior too, and that IS covered in-process here through the ADR-0150 `Arc<dyn HeaderMatch>` trait object; 75.2 adds the cross-proxy witness |
| **CF-72-2**: name-only `{ name }` | REJECT-direction load-parity gap — envoy-rust boot-fatals on it, so a config that would behave differently never runs on the subject side and cannot be differentially witnessed until implemented |
| **CF-72-2**: `treat_missing_header_as_empty` | same — and note upstream ACCEPTS **and HONORS** this field, so the gap is real, not cosmetic |
| **CF-72-2**: the top-level `contains_match` arm | same REJECT-direction reason |
| **CF-75-1**: `exact_match: ""` | MEASURED to degenerate to a PRESENCE match upstream; a distinct finding carried forward, not part of the absence rule |
| `MetadataMatcher.invert` (**CF-74-1**) | a DIFFERENT field on a DIFFERENT message — measured accepted-but-INERT upstream, so it stays boot-fatal here; "implementing" it would CREATE a divergence |

All four CF-72-2 / CF-75-1 items need NEW config surface. This sub-phase adds
**no config surface at all** — the entire change is behavioral.
