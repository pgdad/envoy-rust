# Phase 75 — `HeaderMatcher` ABSENCE-SEMANTICS parity: the mode-scoped `invert_match` short-circuit (CF-72-1) + the `present_match: false` polarity (NEW)

> **What this document is.** The `superpowers:brainstorming` output for phase 75
> (§5 state-0/1 of `BOOTSTRAP_PROMPT.md`). It records the state-0 recon evidence
> MEASURED against `envoyproxy/envoy:v1.33.0` (D-3.3 — no Envoy C++ source was
> read), the pick, the scope, the PLAN-VERIFY items the state-2 PLAN-write must
> re-confirm, the rejected alternatives, and the carry-forward ledger.
>
> **Written for a stranger with zero prior context (D-3.4).** Every claim below
> is either MEASURED this session (and labelled with how) or cited to a file:line
> on disk. Nothing is "as discussed earlier".
>
> **This phase fixes SILENT runtime divergences in a SHARED matching engine read
> by five subsystems.** Read §0 R-0.3 (the mode-scoping guard) before touching
> `crates/envoy-config/src/matcher.rs` — the naive uniform fix BREAKS a measured
> parity case, and phase 72 already proved that by mutation check.

---

## §0. State-0 recon — evidence (MEASURED this session against `envoyproxy/envoy:v1.33.0`)

**Method.** All behavioral claims below were produced by booting BOTH proxies on
equivalent configs and driving the same request matrix at each:

- **Upstream Envoy:** `envoyproxy/envoy:v1.33.0` (the `ENVOY_TARGET.md` pin), in
  Docker with `-p` PORT-MAPPING (**not** `--network host` — the host-net
  namespace is not shared on this host and the admin port would be unreachable
  though the container reported `Up`).
- **envoy-rust:** the DEBUG `target/debug/envoy-bin` built from the phase-74
  close-out tree (`HEAD == c32d5e8d766e6afc2da8df99ec283e80aac68dd4`), run as a
  host subprocess.
- Backend-free throughout: every route is a `direct_response`, so no upstream
  container is involved and nothing depends on this host's Docker bridge IP.
- Route-path probes discriminate by `direct_response` **body**
  (`<probe>=MATCH` / `<probe>=NOMATCH`), with an ordered fallback route per
  probe id. Access-log probes discriminate by which sink emitted a line, read
  back after a graceful `docker stop -t 15` (SIGTERM) so Envoy's FileAccessLog
  buffer is flushed.
- Wire-shape / PGV questions were answered networking-free with
  `--mode validate`.

### R-0.1 — the headline: THREE distinct absence-semantics facts, and the in-tree engine gets TWO of them wrong

The whole subject of this phase is one expression,
`crates/envoy-config/src/matcher.rs:52`:

```rust
mode_result ^ self.invert_match
```

That XOR is applied **uniformly across all seven modes**. Upstream does not.
The MEASURED upstream rule (see R-0.2 for the raw matrix) is:

```
present := the named header is present in the request
          (matched case-insensitively; an empty VALUE still counts as present)

if mode is present_match(want):
        result = (present == want) XOR invert_match
else if not present:
        result = false                    # <-- invert_match is NOT applied
else:
        result = mode_matches(value) XOR invert_match
```

Two consequences, both of which the in-tree engine gets WRONG:

- **D1 (= the recorded carry-forward CF-72-1).** For a VALUE matcher
  (`exact_match` / `prefix_match` / `suffix_match` / `safe_regex_match` /
  `range_match` / `string_match`) with `invert_match: true` and the header
  ABSENT, upstream returns `false` (**DROP**) — the missing header
  short-circuits *before* the inversion. The in-tree engine computes
  `false ^ true` = **KEEP**.
- **D2 (NEW — not previously recorded anywhere in this repo).** Upstream
  `present_match: false` means **"the header must be ABSENT"**. The in-tree
  engine models it as *unconditionally true*
  (`matcher.rs:46`: `if *want_present { value.is_some() } else { true }`), and
  the doc comment immediately above it states that wrong rule verbatim
  (`matcher.rs:44`: `present_match: false → no presence requirement (always
  true)`).

And one case the in-tree engine gets RIGHT, which the fix **must not break**:

- **P1 (parity — the guard).** `present_match: true` + `invert_match: true` +
  ABSENT header is **KEEP on both proxies**, and `present_match: true` +
  invert + PRESENT is **DROP on both**. Here the XOR *is* upstream's behavior.
  This is why the fix must be MODE-SCOPED. Phase 72 recorded a mutation check
  proving a naive uniform "absent ⇒ DROP" fix breaks this case
  (`docs/envoy-rust/phases/72-accesslog-header-filter/PROGRESS.md:355-360`), and
  `BEHAVIOR_CONTRACT.md:2357-2377` (§C) already states the mode-dependence.

### R-0.2 — LIVE-ENVOY + LIVE-ENVOY-RUST (runtime, port-mapped, backend-free): the ROUTE-path matrix

One HCM listener; per probe id `pNN` an ordered pair of routes on prefix
`/pNN` — the first carrying the `HeaderMatcher` under test and answering
`pNN=MATCH`, the second a catch-all answering `pNN=NOMATCH`. Driven four ways:
no `x-a` header; `x-a: v`; `x-a: zzz`; `x-a: 5`.

`U` = `envoyproxy/envoy:v1.33.0`, `R` = envoy-rust DEBUG. **✗ marks a
DIVERGENCE.**

| probe | matcher | absent (U/R) | `x-a: v` (U/R) | `x-a: zzz` (U/R) | `x-a: 5` (U/R) |
|---|---|---|---|---|---|
| p01 | `exact_match: v` + invert | **NO / MATCH ✗** | NO / NO | MATCH / MATCH | MATCH / MATCH |
| p02 | `prefix_match: v` + invert | **NO / MATCH ✗** | NO / NO | MATCH / MATCH | MATCH / MATCH |
| p03 | `suffix_match: v` + invert | **NO / MATCH ✗** | NO / NO | MATCH / MATCH | MATCH / MATCH |
| p05 | `safe_regex_match: v` + invert | **NO / MATCH ✗** | NO / NO | MATCH / MATCH | MATCH / MATCH |
| p06 | `range_match: [1,10)` + invert | **NO / MATCH ✗** | MATCH / MATCH | MATCH / MATCH | NO / NO |
| p09 | `string_match: {exact: v}` + invert | **NO / MATCH ✗** | NO / NO | MATCH / MATCH | MATCH / MATCH |
| p07 | `present_match: true` + invert | MATCH / MATCH | NO / NO | NO / NO | NO / NO |
| p08 | `present_match: false` + invert | NO / NO | **MATCH / NO ✗** | **MATCH / NO ✗** | **MATCH / NO ✗** |
| p10 | `exact_match: v` | NO / NO | MATCH / MATCH | NO / NO | NO / NO |
| p11 | `present_match: true` | NO / NO | MATCH / MATCH | MATCH / MATCH | MATCH / MATCH |
| p12 | `present_match: false` | MATCH / MATCH | **NO / MATCH ✗** | **NO / MATCH ✗** | **NO / MATCH ✗** |
| p13 | `string_match: {exact: v}` | NO / NO | MATCH / MATCH | NO / NO | NO / NO |
| p14 | `range_match: [1,10)` | NO / NO | NO / NO | NO / NO | MATCH / MATCH |

Read off:

- **D1** is rows p01/p02/p03/p05/p06/p09 — **six** value-matcher modes, all
  diverging in exactly the absent cell, all in the same direction.
- **D2** is rows p08 and p12 — and note p12 is **NOT inverted**. A plain
  `present_match: false` matcher, the simplest possible spelling, silently
  matches every request in envoy-rust and only header-absent requests upstream.
- **P1** is row p07 (and p11, the non-inverted presence control): fully at
  parity, both cells, both proxies.
- An additional upstream probe (`p04`, `contains_match: v` + invert) behaved
  identically to p01 upstream; it is absent from the envoy-rust column because
  envoy-rust rejects the top-level `contains_match` arm at load (R-0.5).

Empty-VALUE control (`curl -H "x-a;"`, which sends `x-a:` with an empty value):
upstream scored `present_match: true` as MATCH, confirming that an empty value
still counts as **present** — the `present`/`absent` axis is presence, not
emptiness. envoy-rust agrees (`value.is_some()`).

### R-0.3 — LIVE-ENVOY (runtime, graceful-stop flush): the ACCESS-LOG path agrees EXACTLY

The same engine is read by the access-log `header_filter` arm through the
ADR-0150 trait seam. A single upstream Envoy boot with **eight** `FileAccessLog`
sinks — one per `header_filter` under test, each writing a distinct file with a
distinct `text_format_source` — driven with three requests (`/absent` with no
`x-a`; `/valmatch` with `x-a: v`; `/valmiss` with `x-a: zzz`), then
`docker stop -t 15` and the eight files read back:

| sink | `header_filter.header` | upstream logged |
|---|---|---|
| s1 | `exact_match: v` + invert | `/valmiss` only — **absent DROPPED (D1)** |
| s2 | `present_match: false` | `/absent` only — **(D2)** |
| s3 | `present_match: false` + invert | `/valmatch` + `/valmiss` — **(D2)** |
| s4 | `present_match: true` + invert | `/absent` only — **parity (P1)** |
| s5 | name-only `{ name: x-a }` | `/valmatch` + `/valmiss` (presence match) |
| s6 | `string_match {exact: v}` + `treat_missing_header_as_empty` | `/valmatch` only |
| s7 | `exact_match: v` | `/valmatch` only — parity control |
| s8 | `string_match {exact: v}` + invert | `/valmiss` only — **absent DROPPED (D1)** |

Every cell is exactly what the R-0.1 rule predicts, and exactly what the
route-path matrix produced for the same matcher. **The rule is uniform across
subsystems** — which is the whole reason the fix is a single expression and the
whole reason its blast radius is five subsystems.

### R-0.4 — the blast radius, MEASURED in-tree (not assumed)

`HeaderMatcher::matches` (`crates/envoy-config/src/matcher.rs:22`, XOR at `:52`)
is evaluated at **exactly five call sites**, spanning **five subsystems** in
**three crates**:

| # | call site | subsystem |
|---|---|---|
| 1 | `crates/envoy-http1/src/hcm.rs:2165` (`route_matches`) | Route header matching — serves **both H1 and H2** (H2 has no independent walker; `crates/envoy-http2/src/hcm.rs:475` calls `envoy_http1::hcm::resolve_route`) |
| 2 | `crates/envoy-filter/src/rbac.rs:60` (`eval`) | HTTP RBAC filter (permissions **and** principals) |
| 3 | `crates/envoy-filter/src/fault.rs:76` (`header_gate_matches`) | HTTP fault filter header gate |
| 4 | `crates/envoy-filter/src/jwt_authn.rs:185` (`route_match_matches`) | JWT authn requirement-rule matching |
| 5 | `crates/envoy-accesslog/src/filter.rs:139` (`LogFilter::should_log`) | Access-log `header_filter` |

Plus one pure delegation seam that is NOT a subsystem:
`crates/envoy-config/src/matcher.rs:69`, the
`impl envoy_accesslog::HeaderMatch for HeaderMatcher` required by ADR-0150
(`envoy-accesslog` must not depend on `envoy-config` — cycle), injected at
`crates/envoy-http1/src/hcm.rs:1784-1786` inside `compile_access_log_filter`.

**Correction to a landed claim, recorded here rather than edited into place
(ADRs are append-only, D-3.5):** `docs/envoy-rust/DECISIONS.md:2448` says "FIVE
call sites across FOUR subsystems", but its own parenthetical enumerates five
("H1+H2 route matching, RBAC, fault, JWT authn, access-log filtering"). The
count of **five call sites** is correct; **five** is the right subsystem count.
Do NOT edit ADR-0149/ADR-0151 — this SPEC and the phase's ADR carry the
correction forward.

**Explicitly NOT in the blast radius: network RBAC.**
`crates/envoy-bin/src/network_rbac.rs` is an independent L4 evaluator whose
`Permission::Header` / `Principal::Header` arms return `false` behind a
`debug_assert!` (`network_rbac.rs:123-129`, `:151-157`) because `envoy-config`
rejects those arms at load. It never calls `HeaderMatcher::matches`. Also not in
radius: cors and csrf (they call `StringMatcher::matches`, a different engine),
local_rate_limit, header_to_metadata, cdn_loop, buffer.

### R-0.5 — the REJECT-direction load-parity gaps (MEASURED both ways; deliberately OUT of scope)

Upstream ACCEPTS all four of the following; envoy-rust boot-fatals on the first
three and silently differs on the fourth. Measured by feeding each to
`target/debug/envoy-bin` and reading the `ConfigError` it writes to STDOUT:

| spelling | upstream | envoy-rust |
|---|---|---|
| name-only `{ name: x-a }` | accepted; behaves as `present_match: true` (R-0.3 s5) | **REJECT** — `HeaderMatcher: missing mode key (expected one of [...])` |
| `treat_missing_header_as_empty: true` | accepted **and honored** (R-0.3 s6; and it flips D1's absent cell to KEEP because the absent header becomes a present `""`) | **REJECT** — ``unknown field `treat_missing_header_as_empty` `` |
| top-level `contains_match: v` | accepted (with a deprecation warning) | **REJECT** — ``unknown field `contains_match` `` (only reachable in-tree as `string_match: { contains: ... }`, by design — `bootstrap.rs:2976-2979`) |
| `exact_match: ""` | **degenerates to a PRESENCE match** — MEASURED: absent → NO, `x-a: v` → **MATCH**, empty value → MATCH (identical to name-only) | **ACCEPTS and boots**, then does a literal empty-value exact match — MEASURED: absent → NO, `x-a: v` → **NO**, empty value → MATCH |

The first three are **fail-loud** (a REJECT-direction gap, the ADR-0049
posture): a config that would behave differently never boots, so there is no
silent runtime difference. They are the existing carry-forward **CF-72-2** and
stay out of scope (§2.2).

The fourth, `exact_match: ""`, IS a silent divergence — but only on a
*deprecated* oneof arm with an *empty* literal, and "fixing" it means encoding a
genuinely surprising proto3-degeneracy rule into the config model. It is opened
as the NEW carry-forward **CF-75-1** (§10) with the measurement banked, so a
future phase can decide it on evidence rather than rediscover it. Note
`string_match: { exact: "" }` does **not** degenerate (MEASURED: it is a real
empty-exact match, and PGV separately rejects `string_match: { prefix: "" }`
with `value length must be at least 1 characters`) — the degeneracy is specific
to the deprecated top-level scalar arm.

### R-0.6 — existing coverage: ZERO differential fixtures, five in-process tests, three of which encode the bug

- **No differential fixture anywhere in `tests/fixtures/` sets `invert_match`** —
  verified: `grep -rl invert_match --include=*.yaml --include=*.yml
  tests/fixtures` returns **0 files**. The four `invert_match` hits under
  `tests/` are prose in READMEs. **The divergence is entirely un-witnessed
  differentially**, which is exactly why this phase must ship new fixtures.
- **No fixture exercises `present_match` on a `HeaderMatcher` at all.** The only
  `present_match:` in fixture YAML is
  `tests/fixtures/0044-http-rbac-matcher-value-enrichment/envoy-rust.yaml:77`,
  which is a `ValueMatcher` on RBAC **metadata** — a DIFFERENT message (see the
  trap in §2.3).
- Route header matching has exactly **one** differential witness in the whole
  corpus: `tests/fixtures/0007-http1-direct-response` (non-inverted
  `exact_match`).
- The entire behavioral coverage of the engine is **five in-process tests, all
  in `crates/envoy-config/src/matcher.rs`**:

| path:line | test | disposition under this phase |
|---|---|---|
| `matcher.rs:419` | `invert_match_inverts_exact_match_result` | unaffected (header PRESENT only) |
| `matcher.rs:425` | `invert_match_inverts_present_match_result` | **must stay GREEN** (pins P1) |
| `matcher.rs:432` | `pv4_value_matcher_absent_plus_invert_kept_diverges_from_upstream` | **must be AMENDED** — it asserts the bug twice (inherent engine `:449`, trait object `:457`) |
| `matcher.rs:463` | `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream` | **must stay GREEN — this is the mode-scoping guard.** Its own doc comment says "A future CF-72-1 fixer MUST PRESERVE this KEEP" |
| `matcher.rs:489` | `header_match_trait_delegates_to_inherent_engine` | **must be AMENDED** — carries a third copy of the divergent assertion at `:503` |

  No test in `envoy-bin`, `envoy-http1`, `envoy-http2`, `envoy-filter` or
  `envoy-accesslog` asserts on invert behavior at all. **`present_match: false`
  has no behavioral test anywhere** — which is how D2 survived unnoticed.

### R-0.7 — cost of the two new fixtures (MEASURED against the harness on disk)

Adding a differential fixture costs **one new `tests/differential/tests/*.rs`
file and nothing else** — no registry, no list, no `[[test]]` stanza (the
`tests/differential/Cargo.toml` has none; cargo autodiscovers `tests/*.rs`), no
workspace edit (`Cargo.toml:19` already lists `tests/differential`), no CI edit
(`.github/workflows/ci.yml:67` is `cargo test --workspace`). The entrypoint is a
12-line stencil in which only the test fn name and the fixture directory change.

Both drivers this phase needs already exist and need **no change**:

- `Driver::Http1ProbeList` / `kind: http1_probe_list`
  (`tests/differential/src/lib.rs:119`) — N independent H1 request/response
  cycles with a per-probe equivalence cascade. This is the driver
  `0007-http1-direct-response` uses, and it is backend-free with
  `direct_response` routes. **Right shape for the route-path fixture.**
  (`Driver::Http1RouteSelect`, `lib.rs:448`, is the *wrong* one — it
  discriminates on a `backend: <marker>` body from a subset-LB cluster and
  therefore needs a backend.)
- `Driver::Http1AccessLogByteExact` / `kind: http1_access_log_byte_exact`
  (`tests/differential/src/lib.rs:159`) — byte-exact whole-line access-log
  comparison over a probe sequence, with `expect_logged: false` marking a
  suppressed probe. This is the `0081`/`0082` driver. **Right shape for the
  access-log fixture.**

Backend-free-ness is decided by a text scan for the `BACKEND_PORT` template
marker (`lib.rs:3322-3330`); with `clusters: []` and `direct_response` routes,
no backend container spawns.

**Ordering cost to budget for:** `suppression_settle`
(`tests/differential/src/lib.rs:1694-1699`) inspects `probes.last()` and charges
`CF71_1_SETTLE` = **12 s** when the last probe is DROPPED, versus
`CF70_3_SETTLE` = **2 s** otherwise. Order the access-log fixture's probes so the
last one is KEPT.

### R-0.8 — the config model needs NO new field

Unlike phase 74 (which added a message) this phase adds **no config surface**.
`HeaderMatcher` already carries everything the fix needs:
`name: String`, `mode: HeaderMatcherMode`, `invert_match: bool`
(`crates/envoy-config/src/bootstrap.rs:3104-3123`, hand-rolled `Deserialize` at
`:3150-3271`, hand-rolled `Serialize` at `:3273-3296`). `PresentMatch(bool)` is
already a variant (`bootstrap.rs:3144`) and `present_match: false` already
deserializes and validates (`validate_header_matcher`, `bootstrap.rs:5555-5586`,
which does not inspect `invert_match` at all). **The entire fix is behavioral.**

### R-0.9 — recon traps hit this session (bank these; they cost real time)

- **This host's Docker bind mounts are STALE-CACHED.** After editing a config
  file in a bind-mounted directory, the container kept reading the *previous*
  contents — verified directly by `docker run --entrypoint sed` printing the old
  line while the host file showed the new one. `--mode validate` therefore
  reported a PGV error for a constraint the on-disk file no longer violated.
  **Use a FRESH FILENAME for every config revision**; do not edit in place and
  re-run. (Same family as memory `host-docker-desktop-virtiofs-no-inotify`.)
- **`--volumes-from` does not retrieve a stopped container's `/tmp`** (no
  declared volume). Use `docker cp <container>:/path ./local` after the graceful
  stop.
- **envoy-rust requires `codec_type`** on the HCM (`missing field codec_type`),
  where upstream defaults it. Every envoy-rust probe config needs it.

### R-0.10 — numbering (measured on disk this session)

- Next ROADMAP id **75** (id-column parse: 101 rows, max `74`, missing integer
  ids exactly `{59, 60, 62}` — the documented intentional gaps).
- **82** fixture directories exist → next fixture ids **0083** and **0084**.
- `DECISIONS.md` ledger head **ADR-0155**; **0** occurrences of `ADR-0156` →
  next ADR **ADR-0156** (its §6.1-split reservation lapsed unused when phase 74
  closed without a split, exactly as ADR-0154's did at the phase-73 close).
- `tests/conformance/h2spec/known-failures.txt` is **21** lines; **63** tracked
  `parse_bootstrap` corpus seeds; five workspace fuzz targets in a bijection
  with `.github/workflows/ci.yml` lines 107/113/120/127/134.

---

## §1. Goal

Make `HeaderMatcher` **absence semantics** behaviorally equivalent to
`envoyproxy/envoy:v1.33.0` by replacing the uniform
`mode_result ^ invert_match` at `crates/envoy-config/src/matcher.rs:52` with the
MEASURED mode-scoped rule of R-0.1, and witness the change with two new
backend-free differential fixtures — one on the **route** path and one on the
**access-log** path — so that both consumers of the shared engine are pinned
cross-proxy for the first time.

Two silent divergences close: **D1** (the recorded CF-72-1, six value-matcher
modes) and **D2** (`present_match: false`, NEW this session, and diverging even
without `invert_match`). One measured parity case (**P1**,
`present_match: true` + invert) must be preserved, and the phase must prove it
was preserved rather than assert it.

---

## §2. Scope

### 2.1 In scope

1. **The engine fix** — `HeaderMatcher::matches`
   (`crates/envoy-config/src/matcher.rs:22-54`) restructured to the R-0.1 rule:
   `present_match` evaluates `(present == want) ^ invert_match`; every other
   mode short-circuits to `false` when the header is absent and applies
   `^ invert_match` only when it is present.
2. **The wrong doc comment** at `matcher.rs:43-45` (`present_match: false → no
   presence requirement (always true)`) corrected to the measured rule, with the
   measurement cited.
3. **The two divergence-encoding tests AMENDED** —
   `pv4_value_matcher_absent_plus_invert_kept_diverges_from_upstream`
   (`matcher.rs:432`, two assertions) and
   `header_match_trait_delegates_to_inherent_engine` (`matcher.rs:489`, one
   assertion at `:503`) — flipped to the corrected expectation and renamed to
   describe parity rather than divergence.
4. **The two guard tests kept GREEN and strengthened** —
   `invert_match_inverts_present_match_result` (`matcher.rs:425`) and
   `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream`
   (`matcher.rs:463`). The latter is the mode-scoping guard; its doc comment
   already instructs the fixer to preserve it.
5. **A full in-process engine matrix** covering every cell of R-0.2: seven modes
   × {absent, present-matching, present-non-matching} × {invert, no-invert},
   plus the empty-header-VALUE control that pins "present" as presence rather
   than non-emptiness. This is the coverage whose absence let D2 survive.
6. **Consumer-level in-process tests proving the fix propagates** through each
   of the five call sites of R-0.4 — at minimum the route walker (H1 and H2, the
   latter via `resolve_route`), the HTTP RBAC filter, the fault filter's header
   gate, the JWT-authn rule matcher, and the access-log `header_filter` seam
   (via `Arc<dyn HeaderMatch>`, so the ADR-0150 trait object is exercised, not
   just the inherent method).
7. **NEW differential fixture `0083`** — route path, `kind: http1_probe_list`,
   backend-free (`clusters: []`, `direct_response` routes only). Witnesses D1
   (a value matcher + invert + absent), D2 (`present_match: false`, both invert
   polarities, header present AND absent) and P1 (`present_match: true` +
   invert) cross-proxy for the first time.
8. **NEW differential fixture `0084`** — access-log path,
   `kind: http1_access_log_byte_exact`, backend-free. The same axes through the
   `header_filter` seam, asserted as byte-exact whole-line equality. Probes
   ordered so the LAST one is KEPT (R-0.7: 2 s settle, not 12 s).
9. **Two 12-line test entrypoints** under `tests/differential/tests/`, per the
   R-0.7 stencil.
10. **`BEHAVIOR_CONTRACT.md` updates** — rewrite §C (`:2357-2377`), which
    currently records D1 as an accepted divergence, into the MEASURED parity
    rule; add a new subsection stating the `present_match` polarity rule
    (including that `present_match: false` means header-absent) with the R-0.2 /
    R-0.3 matrices; and correct the two stale `matcher.rs:51` line citations to
    `matcher.rs:52`.
11. **`ROADMAP.md`** row `75` flipped `in-progress` → `done` at the state-6
    close-out.

### 2.2 Out of scope (deliberate, with rationale)

- **CF-72-2 — name-only `{ name }`, `treat_missing_header_as_empty`, and the
  top-level `contains_match` arm.** All three are REJECT-direction load-parity
  gaps (R-0.5): envoy-rust boot-fatals, so a config that would behave
  differently never runs. They are fail-loud by the ADR-0049 posture, they need
  NEW config surface (a new `HeaderMatcher` field and a name-only default mode),
  and — decisively — **they cannot appear in a differential fixture until they
  are implemented**, because the fixture would not boot on the subject side.
  Bundling them would roughly double the phase and mix a silent-correctness fix
  with a config-surface widening. They stay CF-72-2.
- **`exact_match: ""` degenerating to a presence match** (R-0.5, row 4). A
  genuine silent divergence, but confined to a deprecated arm with an empty
  literal, and the "fix" encodes a surprising rule. Opened as **CF-75-1** with
  the measurement banked (§10).
- **`ValueMatcher.present_match` (the RBAC/metadata message).** A DIFFERENT
  message that this phase must not touch — see the trap in §2.3.
- **`ValueMatcher.invert` (CF-74-1).** Also a different field on a different
  message, MEASURED accepted-but-INERT upstream on the access-log path; it stays
  boot-fatal here and must NOT be "implemented" (doing so would CREATE a
  divergence).
- **Any change to the five call sites themselves.** The fix is inside the shared
  engine; the call sites are unchanged. Their behavior changes, which is why §2.1
  item 6 tests them, but no call-site code is edited.
- **A new fuzz target.** See §2.3.

### 2.3 §7.4 fuzz disposition, and the two conflation traps

**Fuzz.** This phase introduces no parser, codec or filter, and adds no config
surface (R-0.8) — the existing `parse_bootstrap` target already covers the
`HeaderMatcher` deserializer unchanged. **No new fuzz target, therefore no new
`ci.yml` step and no new corpus seed.** (Both are otherwise easy to miss:
a new target is not auto-discovered and needs a hand-written `ci.yml` step, and
a new seed needs an explicit `!`-un-ignore line or it is silently untracked.)
The state-2 PLAN-write should confirm this disposition rather than inherit it.

**Trap A — two different `present_match` fields.** `HeaderMatcher.present_match`
(this phase) and `ValueMatcher.present_match` (RBAC / access-log metadata) are
different messages with **different measured rules**. `BEHAVIOR_CONTRACT.md`
lines 1863-1885 and `crates/envoy-config/src/bootstrap.rs:1704` state for the
`ValueMatcher` one: "`present_match: false` NEVER matches — NOT the
HeaderMatcher `present_match` precedent". That note is correct and must not be
"unified" with this phase's finding. Confusingly, after this fix the two rules
*look* similar for the absent case and still differ for the present case — do
not collapse them.

**Trap B — two different `invert` fields.** `HeaderMatcher.invert_match` (this
phase) and `MetadataMatcher.invert` (CF-74-1) are unrelated. The latter is
accepted-but-INERT upstream and stays boot-fatal here.

---

## §3. PLAN-VERIFY items (re-confirm against the live tree at the state-2 PLAN-write)

The state-2 session must MEASURE or re-read each of these before writing
`PLAN.md`, and record the result. Do not inherit them from this document.

- **PV-1.** Re-confirm the R-0.2 route matrix cross-proxy on the then-current
  tree, including the `present_match: false` rows (p08/p12). A stale
  `target/debug/envoy-bin` mis-reports differentials — run
  `cargo build -p envoy-bin` first. Use a fresh config FILENAME per revision
  (R-0.9).
- **PV-2.** Re-confirm the R-0.3 access-log sink table, with the graceful
  `docker stop -t 15` flush and `docker cp` retrieval.
- **PV-3.** Re-derive the five call sites of R-0.4 by grep on the then-current
  tree (they may have moved). Confirm network RBAC is still out of radius.
- **PV-4.** Re-read the five in-process tests of R-0.6 at their then-current
  lines and confirm which two must be amended and which two are the guards.
- **PV-5.** Confirm `grep -rl invert_match --include=*.yaml tests/fixtures` is
  still **0 files** — i.e. no sibling phase added inverted-matcher coverage.
- **PV-6.** Confirm `Driver::Http1ProbeList` and `Driver::Http1AccessLogByteExact`
  are unchanged and still backend-free under `clusters: []` + `direct_response`,
  and re-read `suppression_settle` to confirm the kept-LAST ordering still buys
  the 2 s settle.
- **PV-7.** Confirm the fixture-registration cost of R-0.7 (one `.rs` file, no
  list, no `[[test]]`, no `ci.yml` edit) still holds.
- **PV-8.** Re-derive the §8 size estimate against the live tree and adjudicate
  the §6.1 split gate **on the re-derived number, not on §8's**. §8 is close
  enough to the threshold that this is a real decision, not a formality — see
  the named split line in §8.
- **PV-9.** Confirm that no OTHER in-tree code depends on the current (wrong)
  `present_match: false` semantics — grep `PresentMatch(false)` and
  `present_match: false` across `crates/` and `tests/`. A consumer relying on
  "always true" would turn this fix into a regression.
- **PV-10.** Confirm the two stale `matcher.rs:51` citations (the XOR is at
  `:52`) in `BEHAVIOR_CONTRACT.md` and in the `matcher.rs` doc comments, and
  scope their correction. Do NOT edit the landed ADRs that carry the same stale
  citation (append-only, D-3.5).

---

## §4. Rejected / deferred alternatives (what this pick was chosen over)

- **The remaining `AccessLogFilter` oneof arms** — `duration_filter`,
  `grpc_status_filter`, `runtime_filter`. Each was costed at the phase-74 pick
  and each has a MEASURED reason it is not cheap, unchanged this session:
  `duration_filter`'s predicate is request DURATION, so the differential would
  assert a latency comparison — exactly what `BEHAVIOR_CONTRACT.md` excludes by
  default ("Timing: not compared by default"); it needs a timing-tolerant phase.
  `grpc_status_filter` reads the gRPC response TRAILER and envoy-rust has no
  gRPC data plane, so the differential would be **vacuous**; it needs the gRPC
  family to open first. `runtime_filter` needs RTDS, and no runtime subsystem
  exists. Not re-measured this session — re-measure only if picking one.
- **The fixture-only pins M73-R2 / M71-3 / M71-6** (a CI pin for the
  already-measured mixed-leaf / depth-3 compositions; a dedicated all-drop
  `expected_logged_count == 0` fixture; the standalone H2 access-log-filter
  differential). Each lights up **no new observable** — they pin parity already
  measured. Below the bar as phases. M71-6 in particular is worth weighing at
  state-2 as a cheap FOLD, since this phase already builds an access-log
  fixture (§10).
- **Bundling CF-72-2 into this phase.** Rejected on the R-0.5 reasoning in §2.2:
  it is fail-loud, it needs new config surface, and it cannot be
  differentially witnessed until implemented.
- **Fixing D1 alone (the recorded CF-72-1) and leaving D2.** Rejected: both live
  in the same expression, and D2 is the *worse* bug — it fires on a plain,
  non-inverted, single-line matcher. A D1-only fix would have to re-touch the
  same three lines a phase later, and the state-5 reviewer would rightly ask why
  a measured silent divergence sitting one line away was left in.
- **Fixing D2 alone.** Rejected symmetrically: CF-72-1 is the project's own
  recorded "strongest next candidate", the two share the mode-scoping analysis,
  and the same two fixtures witness both at near-zero marginal cost.
- **The ROADMAP `## Feature Families` openers** (`ROADMAP.md:58`) — network-filter
  payload codecs, `sni_cluster`, non-deterministic LB, HTTP/3 + QUIC, gRPC
  bridge/transcoding, observability SINKS (gRPC ALS, OTLP), runtime/RTDS,
  hot-restart, WASM host. Each is a LARGE new subsystem, re-weighed and again
  far above the cheapest-strong-differential bar.

**Why this pick wins.** It is the project's own recorded strongest next
candidate; the recon turned up a *second*, worse, previously-unrecorded silent
divergence sitting in the same expression; the fix is one small mode-scoped
restructure with **no new config surface** (R-0.8); both differential drivers
already exist and need no change (R-0.7); the fixtures are backend-free and
byte-exact, so nothing depends on this host's Docker bridge; and it converts the
single least-covered high-fan-out engine in the tree — five subsystems, zero
differential coverage, five in-process tests, three of which encode the bug —
into a measured, pinned surface.

---

## §5. Differential surface at phase end

- **NEW fixture `0083-headermatcher-absence-parity`** — green cross-proxy. One
  H1 HCM listener, `clusters: []`, routes are `direct_response` only. Ordered
  route pairs discriminate by body, one pair per matcher under test:
  a value matcher + `invert_match` (D1), `present_match: false` (D2),
  `present_match: false` + `invert_match` (D2), `present_match: true` +
  `invert_match` (P1, the guard), and non-inverted controls. Driven by
  `kind: http1_probe_list` with per-probe `expected_status` +
  `expected_body: { kind: byte_exact }`, both with and without the `x-a` header.
  **This is the first differential witness of `invert_match` and of
  `HeaderMatcher.present_match` in the entire fixture corpus.**
- **NEW fixture `0084-headermatcher-absence-parity-accesslog`** — green
  cross-proxy. One H1 HCM listener, `clusters: []`, a single `direct_response`
  route, and multiple `FileAccessLog` sinks whose `header_filter` carries the
  same matchers, each with a distinct `text_format_source` so the emitted lines
  are byte-DISTINCT and line ORDER is pinned as well as count. Driven by
  `kind: http1_access_log_byte_exact` with `expect_logged` marking each
  suppressed probe, probes ordered so the LAST is KEPT.
- **All 82 pre-existing fixtures stay green**, in particular
  `0007-http1-direct-response` (the only other route-header-matching witness),
  `0017-http-filter-rbac`, `0018-http-filter-fault`, and
  `0078`/`0079`/`0080`/`0081`/`0082` (the access-log filter family). This is
  §7.5 gate (b) and it is the real risk surface of a shared-engine change.
- **Conformance:** unchanged. h2spec stays at its declared threshold;
  `known-failures.txt` stays **21** lines and is NEVER trimmed (this host scores
  h2spec 3.5/2 as PASS, so trimming on local evidence would break CI).

---

## §6. `BEHAVIOR_CONTRACT.md` additions

1. **Rewrite §C** (`BEHAVIOR_CONTRACT.md:2357-2377`). It currently records D1 as
   an accepted, carried divergence ("envoy-rust KEEPS, upstream DROPS … the
   shared-engine fix is carry-forward CF-72-1"). After this phase it states the
   MEASURED parity rule of R-0.1 in full, names the fixtures that pin it, and
   records that CF-72-1 is CLOSED. The existing mode-dependence warning and the
   "a fixer MUST preserve the `present_match` KEEP" instruction are **kept** —
   they remain true and remain the guard.
2. **New subsection: the `present_match` polarity rule.** `present_match: X`
   matches iff `(header present) == X`, then `^ invert_match`. Carries the
   R-0.2 p07/p08/p11/p12 rows and the R-0.3 s2/s3/s4 rows as the measurement.
   Explicitly cross-references Trap A (§2.3) so no future reader conflates it
   with `ValueMatcher.present_match`.
3. **New subsection under §D or adjacent: the REJECT-direction gaps**, updated
   with the two facts R-0.5 added to the existing CF-72-2 record — that the
   top-level `contains_match` arm is also rejected in-tree, and that
   `treat_missing_header_as_empty: true` is not merely accepted upstream but
   **honored**, and specifically flips D1's absent cell to KEEP.
4. **New row for CF-75-1**: `exact_match: ""` degenerates to a presence match
   upstream and does a literal empty-value exact match in-tree, with the
   measured three-cell result on each side, and the note that
   `string_match: { exact: "" }` does NOT degenerate.
5. **Citation correction**: the XOR expression is at `matcher.rs:52`, not
   `matcher.rs:51`. Correct it in `BEHAVIOR_CONTRACT.md` only. Landed ADRs
   carrying the same stale citation are append-only and must NOT be edited.

---

## §7. ADR reservations

- **ADR-0156** — *fired by this state-0/1 pick session*: records the pick, the
  measured basis (D1 + D2 + P1), the scope decision (silent runtime divergences
  IN, reject-direction load-parity gaps OUT), and the subsystem-count correction
  to ADR-0149/ADR-0151's "FIVE call sites across FOUR subsystems" phrasing.
- **ADR-0157** — RESERVED for a §6.1 split at the state-2 PLAN-write. §8 puts
  this phase close enough to the gate that the reservation is a real option, not
  a formality. If no split occurs, the number lapses unused and the next phase's
  pick may claim it (the ADR-0154 / ADR-0156 precedent).
- A further ADR may be needed at state-2 if the §6.2 wire-shape reconciliation
  turns up anything the state-0 recon did not measure. That is the established
  cadence (ADR-0155 at phase 74, ADR-0061 at phase 24).

---

## §8. Estimated size (for the §6.1 split gate at state-2)

| Area | Net LoC (rough) |
|---|---|
| `crates/envoy-config/src/matcher.rs`: the mode-scoped engine restructure + the corrected doc comments | ~45 |
| Amend the 2 divergence-encoding tests + strengthen the 2 guard tests (R-0.6) | ~60 |
| In-process engine matrix: 7 modes × 3 presence states × 2 invert polarities + the empty-value control (§2.1 item 5) | ~200 |
| Consumer-level propagation tests across the 5 call sites (§2.1 item 6) | ~180 |
| Fixture `0083` (route path): 2× config + `expectations.yaml` + README | ~255 |
| Fixture `0084` (access-log path): 2× config + `expectations.yaml` + README | ~300 |
| 2 differential test entrypoints (12 LoC each + the doc header the house style requires) | ~60 |
| `BEHAVIOR_CONTRACT.md` §6 items 1-5 | ~95 |
| `ROADMAP.md` row + ADR-0156 + docs | ~35 |
| **Total** | **~1230 net LoC / ~15–18 tasks** |

Under the ~1500 LoC / ~25 task gate — **a single phase is projected** — but by
the narrowest margin of any phase since 67, and the LoC estimate is dominated by
two fixtures whose real size is only knowable once written. **The §6.1 valve is
genuinely live at state-2.** If PV-8 re-derives above the gate, split on this
line, which keeps each half independently green and differentially witnessed:

- **75.1** — the engine fix + the corrected doc comments + all in-process tests
  (engine matrix + the five consumer propagation tests) + fixture `0083` (route
  path) + the §C rewrite. Ships the correctness fix with a differential witness.
- **75.2** — fixture `0084` (access-log path) + the `present_match` polarity
  subsection + the CF-75-1 and CF-72-2 contract rows. Ships the second consumer's
  witness and the contract bank.

Do **not** split engine-fix-from-tests or fix-from-fixture: a half that changes
behavior without a differential witness violates §6.3 (no "defer by cramming").

---

## §9. Risks

- **Highest risk is gate (b), not gate (a).** This is a shared-engine behavior
  change under five subsystems with almost no existing behavioral coverage
  (R-0.6). The new fixtures will pass; the danger is a pre-existing fixture
  (`0007`, `0017`, `0018`, `0078`-`0082`) or an in-process test that silently
  depended on the old semantics. PV-9 exists to find that before implementation.
- **The mode-scoping guard.** A naive uniform "absent ⇒ DROP" fix breaks P1 and
  introduces a NEW divergence. Phase 72 proved this by mutation check. The state-3
  session should honor TDD's RED with a mutation check of its own — break the
  mode-scoping, watch
  `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream` go RED,
  revert — and record that as the RED evidence. Run any such mutation in a
  scratch `git worktree`, never in the main tree with parallel subagents active.
- **The two traps of §2.3** (`ValueMatcher.present_match`,
  `MetadataMatcher.invert`) are the most likely way a reviewer or a later fixer
  breaks something adjacent.

---

## §10. Carry-forwards

**CONSUMED by this phase (if it lands as scoped):**

- **CF-72-1** — the shared-engine value-matcher `absent + invert` divergence.
  Closed by the D1 half of the fix.

**OPENED by this pick:**

- **CF-75-1** — `exact_match: ""` degenerates to a PRESENCE match upstream
  (MEASURED R-0.5: absent → no match, `x-a: v` → **match**, empty value →
  match) while envoy-rust performs a literal empty-value exact match (MEASURED:
  absent → no, `x-a: v` → **no**, empty value → match). A silent divergence, but
  confined to a DEPRECATED oneof arm with an empty literal, and the fix encodes
  a surprising proto3 degeneracy. Note `string_match: { exact: "" }` does NOT
  degenerate, and PGV separately rejects `string_match: { prefix: "" }`
  (`value length must be at least 1 characters`). Owner = a future
  `HeaderMatcher` wire-shape-parity phase, which should decide it alongside
  CF-72-2 rather than alone.

**NOT consumed — carried forward (owner = whatever future phase touches their
surface):**

- **CF-72-2** — name-only `{ name }` (upstream: presence match) and
  `treat_missing_header_as_empty` (upstream: accepted AND honored; it flips D1's
  absent cell to KEEP). **Extended this session** with a third member: the
  top-level `contains_match` arm, which upstream accepts (deprecated) and
  envoy-rust rejects as an unknown field. All three are REJECT-direction
  load-parity gaps, fail-loud per ADR-0049. Out of scope per §2.2.
- **CF-74-1** — `MetadataMatcher.invert` accepted-but-INERT upstream on the
  access-log path, boot-fatal in-tree. Must NOT be "implemented" (that would
  CREATE a divergence). A future fixer must first measure whether `invert` is
  honored on the **RBAC** path before touching the shared type.
- **CF-74-2 / CF-74-3** — multi-segment metadata `path`, and the unmodelled
  `ValueMatcher` arms (`bool_match`, `double_match`, `list_match`, `null_match`,
  `or_match`). Both blocked on the FLAT string-only metadata store.
- **CF-74-4** — the RBAC-scoped `validate_metadata_matcher` does not enforce a
  non-empty path-segment `key` though upstream PGV does. Owner = the next
  RBAC-matcher phase.
- **CF-74-6** — the wrapped `match_if_key_not_found: { value: <bool> }` spelling
  is accepted and honored upstream, boot-fatal here. Do **not** "fix" it by
  making `Option<bool>` wrapper-accepting — that would lose the
  absent-vs-explicit-`false` distinction `unwrap_or(true)` depends on.
- **M74-31** — the five-site "placed SECOND **so** the last probe is KEPT"
  NON-SEQUITUR. `suppression_settle`
  (`tests/differential/src/lib.rs:1694-1699`) inspects only `probes.last()`, so
  with `[drop, keep, keep]` the last probe is kept whichever kept probe sits
  last; every OUTCOME asserted is true, only the causal "so" is wrong. Sites:
  `tests/differential/tests/access_log_metadata_filter.rs:30-31`,
  `BEHAVIOR_CONTRACT.md:2612-2614`,
  `tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml:36-38`, and
  that fixture's `README.md:104-108`. **This phase writes a new access-log
  fixture with the same kept-LAST ordering — weigh at state-2 whether to fix all
  five sites together and avoid minting a sixth.**
- **M73-R2 / M71-3 / M71-6** — the fixture-only pins (mixed-leaf / depth-3
  compositions; a dedicated all-drop `expected_logged_count == 0` fixture; the
  standalone H2 access-log-filter differential). **M71-6 is worth weighing at
  state-2 as a cheap fold**, since this phase already builds an access-log
  fixture and an H2 sibling would reuse `Driver::Http2AccessLogByteExact`
  unchanged.
- **M74-33 + M74-26** — measured-but-unrecorded cross-proxy facts to bank into
  `BEHAVIOR_CONTRACT.md` §G. Documentation-only; fold into whatever phase next
  touches §G.
- **CF-73-1 / N73-R2** (unbounded `and_filter`/`or_filter` nesting depth, parity
  with upstream), **M73-R1**, **M74-3..M74-14**, **M74-16**,
  **M74-17/18/20/21/22/26/27/28/29**, **M74-30..M74-39**, **M71-7/M71-8**,
  **M70-R4/M70-R9**, **M69-A..I**, **CF-69-1/2/3/5**, **M68-1**, **M-1**,
  **CF-67-3/5/6/7**, the older Minors in `67.3/SPEC.md` §10, and the
  HTTP-filters-family (1)-(4) in `STATE_HISTORY.md` — all untouched by this
  phase; carry forward.

**Also recorded forward (documentary, from R-0.4 / R-0.9):**

- The "FIVE call sites across FOUR subsystems" phrasing at
  `DECISIONS.md:2448` under-counts the subsystems by one (five call sites, five
  subsystems, three crates). ADR-0156 carries the correction; the landed ADR is
  append-only and must NOT be edited.
- This host's Docker bind mounts are stale-cached: an edited config file is not
  re-read by a new container. Use a fresh filename per config revision.
