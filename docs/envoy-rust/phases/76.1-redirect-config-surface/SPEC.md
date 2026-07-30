# Sub-phase 76.1 — SPEC

**Title:** `Route.redirect` — the CONFIG SURFACE slice: the `RedirectAction` schema, the
`RedirectResponseCode` enum, the THIRD `RouteAction` variant, the widened three-way
`Route`-action cardinality check, the two intra-`RedirectAction` oneof validators, and
`Serialize` support — landing REJECT-DIRECTION parity with `envoyproxy/envoy:v1.33.0` on
seven measured rejections, with NO runtime behaviour and NO new fixture.

**ROADMAP row:** `76.1` (status `planned` at this writing).
**Parent phase:** `76` (`docs/envoy-rust/phases/76-route-redirect-action/`, status `in-progress`).
**Split ADR:** `ADR-0169` (the §6.1 split that created this sub-phase).
**Parent pick ADR:** `ADR-0168`.
**Depends on:** `04` (HCM + route match + `direct_response`), `05` (HTTP/2), `32`
(access-log command operators), `42` (`%RESPONSE_CODE_DETAILS%`) — i.e. the parent's
dependency set, unchanged.
**Sibling:** `76.2` (the runtime + fixture `0086` + the contract bank) depends on this
sub-phase and must not start before it is `done`.

---

## 0. How to read this document

This SPEC is written for a session with **zero prior context** (doctrine D-3.4). It is
self-contained: you do not need to read the parent `76/SPEC.md` to execute this sub-phase,
though the parent holds the full runtime measurement table that `76.2` consumes.

Every behavioural claim about upstream Envoy below was **MEASURED** against the pinned
reference image `envoyproxy/envoy:v1.33.0` (`docs/envoy-rust/ENVOY_TARGET.md`) — most of
them at the parent phase's §5 state-0/1 recon, and the ones marked **[76.1-NEW]** at the
§5 state-2 session that fired the split. Nothing here is read from documentation or
inherited from a previous phase. Where a claim was *not* measured, it says so explicitly.

Terminology:

- **upstream** / **Envoy** — the reference proxy, the pinned Docker image.
- **envoy-rust** — the Rust implementation in this repository, the subject under test.
- **boot-fatal** — the config is rejected at load and the process exits non-zero.
- **reject-direction parity** — envoy-rust rejects exactly the configs upstream rejects.
  Error *text* is explicitly NOT compared; only the accept/reject verdict is.

**Every `file:line` citation below was re-verified on disk at commit `f438cb9`** by the
state-2 session, by anchoring on TEXT rather than trusting a number. Line numbers in
`crates/envoy-config/src/bootstrap.rs` (~14 400 lines) drift; re-anchor again before
transcribing.

---

## 1. The gap, in one paragraph

Upstream Envoy's `Route` message carries a three-way `oneof action`: `route:` (proxy to a
cluster), `direct_response:` (synthesize a reply), and **`redirect:`** (synthesize a 3xx
reply carrying a `location:` header). envoy-rust implements the first two and **wholly
lacks the third**. `crates/envoy-config/src/bootstrap.rs:2178` declares a two-variant enum:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum RouteAction {
    /// Direct-response action — write a static body downstream. Phase 04.1 carryover.
    DirectResponse(DirectResponse),

    /// Route-to-cluster action — proxy through to the named cluster. Phase 04.3 NEW.
    Route(RouteAction_Route),
}
```

and the hand-written `Route` deserializer (`impl<'de> serde::Deserialize<'de> for Route`,
`bootstrap.rs:2416-2527`) accepts exactly five keys, rejecting anything else in its
`other =>` arm at `bootstrap.rs:2483-2494`. So a `redirect:` route that upstream Envoy
loads and serves is **boot-fatal in envoy-rust today**.

MEASURED (state-2 session, `grep -rni` over `crates/`, excluding `.claude/worktrees/`):
`RedirectAction`, `host_redirect`, `path_redirect`, `port_redirect`, `https_redirect`,
`scheme_redirect`, `prefix_rewrite` return **ZERO hits anywhere in `crates/`**. The
feature is entirely greenfield — no partial implementation, no dead scaffolding.

Two near-miss name collisions, recorded so they are not mistaken for prior art:

- `crates/envoy-filter/src/rbac.rs:73` defines a private `fn strip_query(path: &str) -> &str`
  used by an RBAC URL-path matcher (`rbac.rs:65`). Different crate, different meaning; a
  `strip_query` **bool field** on `RedirectAction` does not conflict with it.
- A case-insensitive grep for `response_code` in `bootstrap.rs` returns 11 hits, **all**
  the access-log command-operator literal `%RESPONSE_CODE%` inside test YAML strings. A
  case-**sensitive** grep returns **zero** — there is no lowercase `response_code`
  identifier or YAML key in the file today.

---

## 2. Why this is a coherent standalone slice

This sub-phase lands a **config surface with no runtime behaviour**. That is the
established "foundation-slice" pattern already used by sub-phases `05.1`, `07.1`, `12.1`
and `14.1`, and it is coherent for three concrete reasons:

1. **It has its own differential surface: the REJECT direction.** Seven measured upstream
   rejections (§3) become seven envoy-rust rejections. Load-parity is a real, testable
   equivalence axis that needs no fixture and no running proxy — `envoy-bin` exits
   non-zero and writes the `ConfigError` to **stdout** (it takes only `-c <path>`).
2. **Its regression obligation is provable without a new fixture.** All **85** existing
   fixtures must stay green. Adding a variant to `RouteAction` cannot change behaviour for
   any config that does not use `redirect:`, and no existing fixture does — MEASURED: a
   `grep -rn "redirect" tests/fixtures/ --include=*.yaml` returns 7 hits, **all** of them
   Prometheus metric NAMES in an allow-list inside
   `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml` (lines 127, 150-154,
   157). **Zero** fixture configures a `redirect:` route action.
3. **The compiler enforces the seam.** `RouteAction` is matched non-exhaustively (no `_`
   arm) at both its config-validator site and its runtime dispatch site, so adding a third
   variant **fails to compile** until every site is handled. That is a forcing function,
   not a hazard — but it means this sub-phase MUST add a compiling (if inert) arm at the
   runtime dispatch site. See §4.4.

---

## 3. MEASURED upstream load behaviour — the REJECT direction

All rows measured via `docker run envoyproxy/envoy:v1.33.0 --mode validate -c <cfg>`,
which binds no sockets. Each config declared `clusters: []`, one HCM listener, one
virtual host, and a single route at `match: { prefix: "/t" }` carrying the cell under test.

### 3.1 Rejections — envoy-rust must reject all seven

| # | config under `redirect:` (or on the `Route`) | upstream verdict | upstream error class |
|---|---|---|---|
| J1 | `path_redirect: "/p"` **+** `prefix_rewrite: "/q"` | **REJECT** | `Unable to parse JSON as proto (INVALID_ARGUMENT: invalid JSON …)` — `oneof path_rewrite_specifier` conflict |
| J2 | `path_redirect: "/p"` **+** `regex_rewrite: {…}` | **REJECT** | same oneof |
| J3 | `redirect:` **+** `route:` on one `Route` | **REJECT** | `oneof action` conflict |
| J4 | `redirect:` **+** `direct_response:` on one `Route` | **REJECT** | same oneof |
| J5 | `scheme_redirect: "https"` **+** `https_redirect: true` | **REJECT** | `oneof scheme_rewrite_specifier` conflict |
| J6 | `response_code: BOGUS` | **REJECT** | `Protobuf message … reason INVALID_ARGUMENT: unknown enum value: 'BOGUS'` |
| J7 | `response_code: 302` (numeric literal) | **REJECT** | `Proto constraint validation failed (… RouteConfiguration …)` — PGV `defined_only` |

**Error TEXT is not part of the equivalence contract** — envoy-rust must match the
accept/reject VERDICT only. Its own messages follow house `ConfigError` style (§4.3).

### 3.2 Acceptances — envoy-rust must NOT reject these

| # | config | upstream verdict | why it is recorded |
|---|---|---|---|
| A1 | `port_redirect: 0` | **ACCEPT** | surprising; no PGV lower bound |
| A2 | `port_redirect: 70000` | **ACCEPT** | **there is NO PGV upper bound.** At runtime it renders verbatim (parent SPEC measured `location: http://e.com:70000/…`). envoy-rust must **NOT** add a `1..=65535` bound or it manufactures a reject-direction divergence. |
| A3 | `host_redirect: ""` | **ACCEPT** | empty string is a legal value |
| A4 | `scheme_redirect: ""` | **ACCEPT** | empty string is a legal value |

### 3.3 **[76.1-NEW]** The oneofs are exclusive on FIELD PRESENCE, not on value

This was **not** in the parent SPEC and is the single most likely thing a from-scratch
implementation gets wrong. Measured at the state-2 session:

| # | config | upstream verdict |
|---|---|---|
| A5 | `https_redirect: false` **+** `scheme_redirect: "ftp"` | **REJECT** |
| A6 | `https_redirect: false` **alone** | **ACCEPT** |
| A7 | `path_redirect: ""` **+** `prefix_rewrite: "/q"` | **REJECT** |

Read the rule off A5/A6/A7: these are **protobuf `oneof` members**, so *writing the key at
all* sets the oneof — regardless of whether the value is `false` or `""`. A5 rejects even
though `https_redirect` is `false`; A7 rejects even though `path_redirect` is empty.

**The direct consequence for the Rust model:** `https_redirect` MUST be modelled as
`Option<bool>`, and `path_redirect` / `prefix_rewrite` / `scheme_redirect` as
`Option<String>`. A `#[serde(default)] pub https_redirect: bool` **loses presence** and
would wrongly ACCEPT A5, minting a brand-new reject-direction divergence. This must be
pinned by its own test (§6, T-R5/T-R7).

### 3.4 NOT MEASURED — do not treat as settled

1. Whether `regex_rewrite` alone (without `path_redirect`) is accepted — it is (the parent
   recon measured it working at runtime, row R12), but it is a **NON-GOAL** here (§5).
2. Any interaction of `redirect:` with `typed_per_filter_config` on the same `Route`.
3. Whether upstream imposes any length/charset bound on `host_redirect` or
   `scheme_redirect` beyond accepting the empty string.
4. `port_redirect` boundary behaviour above 65535 beyond the single `70000` probe.

---

## 4. Scope — what this sub-phase builds

All work is in `crates/envoy-config` except §4.4, which is a single inert arm required to
keep the workspace compiling.

### 4.1 `RedirectResponseCode` — a five-value enum

Upstream's `RedirectAction.RedirectResponseCode` has exactly five values. Model it as a
plain derived enum so an unknown name (J6) and a numeric literal (J7) both fail
deserialization:

- `MOVED_PERMANENTLY` → 301 (**the default**)
- `FOUND` → 302
- `SEE_OTHER` → 303
- `TEMPORARY_REDIRECT` → 307
- `PERMANENT_REDIRECT` → 308

The numeric mapping is measured (see `76.2/SPEC.md` §2 for the wire status lines) but is
**consumed by `76.2`, not by this sub-phase** — here the enum only needs to round-trip and
to default correctly. Provide the `-> u16` mapping now (it is 6 lines and is what makes
the enum meaningful), and let `76.2` wire it to the response.

### 4.2 `RedirectAction` — the struct

Eight fields. Model with `#[serde(deny_unknown_fields)]`, which additionally gives the
NON-GOAL `regex_rewrite` a boot-fatal unknown-field rejection for free (that is how J2
rejects here, by a different mechanism than upstream's oneof error — same verdict, which
is all the contract requires):

| field | type | serde | why |
|---|---|---|---|
| `host_redirect` | `Option<String>` | `default`, `skip_serializing_if = "Option::is_none"` | absence is meaningful (authority preserved) |
| `port_redirect` | `Option<u32>` | same | **no range bound** (A1/A2) |
| `path_redirect` | `Option<String>` | same | oneof member — presence-tracked (A7) |
| `prefix_rewrite` | `Option<String>` | same | oneof member — presence-tracked |
| `https_redirect` | `Option<bool>` | same | oneof member — **presence-tracked (A5/A6); NOT a bare `bool`** |
| `scheme_redirect` | `Option<String>` | same | oneof member — presence-tracked |
| `strip_query` | `bool` | `default` | not a oneof member; plain proto3 scalar |
| `response_code` | `RedirectResponseCode` | `default` | not a oneof member; defaults to `MOVED_PERMANENTLY` |

The house already uses exactly this `Option` + `skip_serializing_if` idiom — see
`RouteAction_Route` at `bootstrap.rs:2192-2199`:

```rust
pub struct RouteAction_Route {
    pub cluster: String,
    /// 16.1 D1: optional per-route retry policy. Absent → no retries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
```

and the simpler sibling `DirectResponse` at `bootstrap.rs:2583-2588` (fully derived
de/serialize, `deny_unknown_fields`, no helper fns) is the minimal template:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DirectResponse {
    pub status: u16,
    pub body: DataSource,
}
```

### 4.3 The two intra-`RedirectAction` oneof validators

`path_rewrite_specifier` (`path_redirect` | `prefix_rewrite`) and
`scheme_rewrite_specifier` (`https_redirect` | `scheme_redirect`) are exclusive on
**presence** (§3.3). Because `RedirectAction` derives `Deserialize`, the exclusivity check
belongs in the bootstrap validator, which is the house's boot-fatal mechanism and returns
`crate::ConfigError`.

**Two new `ConfigError` variants** — one per oneof. `ConfigError` is declared at
`crates/envoy-config/src/lib.rs:74` (the `#[derive(Debug, thiserror::Error)]` is at `:73`),
closes at `:991`, and has **exactly 123 variants** today (MEASURED two independent ways:
counting `#[error(` attributes at 4-space indent, and counting variant identifiers — both
give 123). New variants append at the end, following the landed house style, which pairs a
`/// Phase NN (§ref): …` doc comment with a wrapped `#[error(…)]`:

```rust
    /// Phase 34 (§A5-LOCKED): a `header_to_metadata` rule is malformed (empty header, no action,
    /// empty key, or an on_header_missing with no value). Envoy rejects these boot-fatally; envoy-rust
    /// matches (ADR-0049). `listener` names the offending HCM; `detail` the specific violation.
    #[error("header_to_metadata filter on listener `{listener}` has an invalid rule: {detail}")]
    HeaderToMetadataInvalidRule { listener: String, detail: String },
```

The validator arm attaches at the existing per-route loop, `bootstrap.rs:3975-3996`, whose
`match &r.action` at `:3981` is non-exhaustive and **will fail to compile** when the third
variant lands — the forcing function of §2.3.

### 4.4 The `RouteAction::Redirect` variant and the three-way cardinality check

Five edit sites, all in `bootstrap.rs`, plus one inert arm outside the crate:

1. **The enum** (`:2178`) gains `Redirect(RedirectAction)`.
2. **The visitor's accumulator block** (`:2436-2444`) gains a
   `let mut redirect: Option<RedirectAction> = None;`.
3. **The visitor's key match** (`:2447-2482`) gains a `"redirect" => { … }` arm in the
   same duplicate-checking shape as its four peers, and the unknown-field name list at
   `:2486-2492` gains `"redirect"` (it currently reads exactly
   `&["name", "match", "direct_response", "route", "typed_per_filter_config"]`).
4. **The cardinality check** (`:2499-2514`) widens from a 2-tuple match to a three-way
   exactly-one check. **CORRECTION to the parent SPEC §4.1 item 3/5:** this site does
   **NOT** use `ConfigError`. It uses `serde::de::Error::custom` (`M::Error::custom`), a
   deserializer error. The two current messages are, verbatim:
   - `"Route must carry exactly one of \`direct_response\` or \`route\`; both are present"`
   - `"Route must carry exactly one of \`direct_response\` or \`route\`; neither is present"`

   Widening these strings to the three-way form is what makes **J3 and J4** reject. The
   `expecting` string at `:2428-2432` (`"a Route map with \`match\` and exactly one of
   \`direct_response\` or \`route\`"`) must be widened in lockstep or it becomes a lie.
5. **Two `Serialize` impls**, not one — the parent SPEC cited one arm from each and it
   reads as though they were the same impl. They are separate:
   - `impl serde::Serialize for Route` (`:2529-2552`), action arms at `:2544`/`:2545`.
     Its `len` computation at `:2535` is a fixed base of `2` plus two optional adders; a
     third action variant still emits exactly one action key, so **the count is unchanged**.
   - `impl serde::Serialize for RouteAction` (`:2554-2570`), arms at `:2565`/`:2566`.

   Both gain a `RouteAction::Redirect(rd) => map.serialize_entry("redirect", rd)?` arm.
6. **One inert runtime arm.** The runtime dispatch `match &route.action` at
   `crates/envoy-http1/src/hcm.rs:2110` is also non-exhaustive and will fail to compile.
   This sub-phase adds a **minimal placeholder arm** so the workspace builds; `76.2`
   replaces it with the real redirect. See §5 item 1 for exactly what the placeholder must
   and must not be.

### 4.5 Fuzz corpus seed

One new `parse_bootstrap` corpus seed exercising a `redirect:` route.

**NO new fuzz target** — so §7.5 gate (d) is satisfied by the existing `parse_bootstrap`
short-budget CI run and **no `.github/workflows/ci.yml` edit is needed**.

The corpus directory is `*`-ignored, so the seed needs an **explicit `!`-un-ignore line**
or it is silently untracked and invisible to CI. MEASURED: `crates/envoy-config/fuzz/.gitignore`
is **66 lines** — `corpus/parse_bootstrap/*` at line 1, **63** `!`-un-ignore lines at
lines 2-64, then `artifacts/` (65) and `target/` (66). The convention is
`!corpus/parse_bootstrap/<snake_case_name>.yaml`, appended at the END (line 64 is the
newest). **Confirm the seed is tracked with `git ls-files` before claiming gate (d).**

---

## 5. Non-goals (explicit — do NOT widen into these)

1. **ALL runtime behaviour.** No `location` header, no status code, no response synthesis,
   no `prefix_rewrite` `:path` mutation. That is `76.2`.
   **The placeholder arm added at §4.4 item 6 must be honest, not a silent stub.** §6.3 of
   `BOOTSTRAP_PROMPT.md` forbids incomplete stubs that tests cannot exercise. The
   placeholder therefore returns the **existing** `synth_501` "not implemented" outcome
   (`BuildOutcome::Synth(synth_501(close), None)`) rather than a fabricated 3xx — a
   configured-but-unserved redirect is loudly wrong, and `76.2` replaces it. Pin this with
   a test (§6, T-C9) so the placeholder is exercised and its replacement in `76.2` is a
   visible, deliberate change rather than a silent one.
2. **`regex_rewrite` inside `redirect`.** Measured working upstream; excluded to hold the
   LoC gate. It is boot-fatal here via `deny_unknown_fields`, which is the intended
   posture and is what makes J2 reject.
3. **`RouteAction.prefix_rewrite` / `regex_rewrite` / `host_rewrite_*` on the `route:`
   (proxying) arm.** A separate, larger surface.
4. **`internal_redirect_policy`, `non_forwarding_action`, `weighted_clusters`,
   `cluster_header`, route-level `timeout`, route/vhost/route-config-level header
   mutation.** All measured ABSENT from envoy-rust; all out of scope.
5. **`CF-76-1`** — upstream strips the query before route path matching while envoy-rust
   matches the raw target byte-for-byte. Opened by the parent recon, OUT OF SCOPE, owner is
   its own phase. Do NOT fix it opportunistically (§6.3).
6. **`CF-75-2`, `CF-75-3`, `CF-75-4`, `CF-75-5`, `CF-75-6`** and every other open
   carry-forward. This sub-phase consumes none of them.

---

## 6. The test obligations (what `PLAN.md` must order TDD-first)

Every item is an in-process unit test in `crates/envoy-config`. None needs Docker, a
socket, or a fixture. Named here so the PLAN can order them tests-first (D-3.1).

**Reject-direction (the differential surface of this sub-phase):**

- **T-R1** J1 — `path_redirect` + `prefix_rewrite` → error.
- **T-R2** J2 — `path_redirect` + `regex_rewrite` → error (unknown field).
- **T-R3** J3 — `redirect` + `route` → error, three-way cardinality message.
- **T-R4** J4 — `redirect` + `direct_response` → error, same.
- **T-R5** J5 — `scheme_redirect` + `https_redirect: true` → error.
- **T-R6** J6 — `response_code: BOGUS` → error.
- **T-R7** J7 — `response_code: 302` (numeric) → error.
- **T-R8** **[76.1-NEW]** A5 — `https_redirect: false` + `scheme_redirect: "ftp"` → error.
  **This is the presence-not-truthiness pin.** It is the test that fails if
  `https_redirect` is modelled as a bare `bool`.
- **T-R9** **[76.1-NEW]** A7 — `path_redirect: ""` + `prefix_rewrite: "/q"` → error.
- **T-R10** neither-action `Route` (no `redirect`, no `route`, no `direct_response`) →
  error, three-way "neither is present" message.

**Accept-direction:**

- **T-A1** A1 `port_redirect: 0` parses.
- **T-A2** A2 `port_redirect: 70000` parses **and round-trips as `70000`** — the anti-bound pin.
- **T-A3** A3 `host_redirect: ""` parses.
- **T-A4** A4 `scheme_redirect: ""` parses.
- **T-A5** **[76.1-NEW]** A6 `https_redirect: false` **alone** parses (the other half of
  the presence pin — proves the model rejects on presence, not on truthiness).
- **T-A6** a bare `redirect: {}` parses, with `strip_query == false` and
  `response_code == MOVED_PERMANENTLY`.
- **T-A7** each of the five `response_code` names parses to its variant.

**Config-surface mechanics:**

- **T-C1..C5** each of the five accepted `Route` keys still parses (regression on the
  widened visitor).
- **T-C6** an unknown `Route` key still rejects, and the error names all **six** accepted
  keys.
- **T-C7** `Serialize` round-trip through `impl Serialize for Route` emits the `redirect`
  key and re-parses equal.
- **T-C8** `Serialize` round-trip through `impl Serialize for RouteAction`.
- **T-C9** the §5-item-1 placeholder arm: a `redirect:` route builds the 501
  not-implemented outcome (the honest-placeholder pin; `76.2` deliberately flips it).

**Regression:** all **85** existing differential fixtures stay green, unchanged.

---

## 7. Definition of done (the §7.5 gate, instantiated)

- **(a)** No new/changed differential fixtures — this sub-phase adds none. Vacuously met,
  and stated explicitly so a reviewer does not read the absence as an oversight.
- **(b)** All **85** pre-existing fixtures still green. This is the sub-phase's
  regression-equivalence proof, and it is the reason the slice is coherent (§2.2).
- **(c)** Conformance: `h2spec` unchanged at its existing threshold. **Do not trim
  `known-failures.txt`** (21 lines) — this development host scores h2spec 3.5/2 as PASS
  where CI does not, so trimming on local evidence breaks CI.
- **(d)** No new fuzz target, so the existing `parse_bootstrap` short-budget CI run
  satisfies the gate. The new corpus seed must be confirmed **tracked** via `git ls-files`.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`,
  `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

---

## 8. Size estimate

Re-derived by component at the state-2 split session, calibrated against the measured net
`crates/` + `tests/` LoC of recent comparable phases (68→950, 69→1540, 70→1372, 73→873,
74→1981, 75.1→1413, 75.2→897):

| component | est. net LoC |
|---|---|
| `RedirectResponseCode` enum + `-> u16` mapping | 28 |
| `RedirectAction` struct + serde attrs + docs | 50 |
| `RouteAction::Redirect` variant, visitor accumulator + arm + key list | 16 |
| three-way cardinality rewrite + `expecting` widening | 30 |
| two `Serialize` arms | 4 |
| two `ConfigError` variants | 18 |
| validator arm (both oneof checks) | 30 |
| inert `synth_501` placeholder arm at the H1 dispatch | 12 |
| in-process tests T-R1..T-R10, T-A1..T-A7, T-C1..T-C9 (26 tests) | 360 |
| fuzz seed YAML + `!`-un-ignore line | 31 |
| **total** | **≈ 579** |

Comfortably under both §6.1 thresholds (~25 tasks / ~1500 LoC). Projected **≈ 10-12
numbered tasks**. **No further split is projected.**

---

## 9. Next state

The next session is this sub-phase's **§5 state-2 PLAN-write** (`superpowers:writing-plans`),
a **SEPARATE session** (§5.1 permits exactly one state per session). It must:

1. **Re-verify every `file:line` citation above on disk before transcribing it.** They were
   re-anchored at commit `f438cb9`; `bootstrap.rs` is ~14 400 lines and numbers drift.
   Anchor on TEXT, never on a number.
2. Write `PLAN.md` as TDD-ordered numbered tasks (D-3.1: tests first, no exceptions).
3. **Pre-flight the PLAN's own literal Rust** against `cargo fmt --all -- --check` and
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`. A recurring
   failure mode in this project is a PLAN whose example code trips the plan's OWN gate, and
   which cites helper functions that do not exist.
4. Re-derive the §8 estimate and re-own the §6.1 split decision.
