# Sub-phase 76.2 — REVIEW (§5 state 5, the code review)

> **What this file is.** The §5 **state-5** code review of sub-phase
> `76.2-redirect-runtime-fixture`. Its inputs are `SPEC.md` (556 lines), `PLAN.md` (2371) and
> `PROGRESS.md` (1789, carrying both the state-3 implementation log and the state-4 §7.5 gate
> adjudication). Its only output is this file.
>
> Written for a reader with **zero prior context** (doctrine D-3.4).
>
> **This session did NOT re-run the §7.5 gate** — it was run and adjudicated at state 4 and its
> real output is quoted in `PROGRESS.md` §S4.0-§S4.11. Re-deriving it is not the review's job;
> grading the CODE is. **This session fixed nothing it graded** (ADR-0127; ADR-0165 — a reviewer
> must not fix what it grades). No ROADMAP status cell was flipped; no `SPEC.md`, `PLAN.md`,
> `PROGRESS.md` or `76.1` artifact was edited.

---

## 1. VERDICT

**CHANGES-REQUESTED.**

**0 Critical · 1 Issue · 9 Minor · 9 Nit.**

The runtime slice itself is correct and unusually well tested. All 22 measured `location` cells are
faithfully transcribed and pinned; the header set, the `content-type` omission, the reason-phrase
gap and both directions of the `:path`-mutation asymmetry are all genuinely witnessed; fixture
`0086` verifies clean against every mechanical constraint. **Nothing on the request/response wire
path is wrong.**

The single Issue is not on that path. Task 8 closed carry-forward **CF-76-2** by teaching the RDS
warm-reload path to reject a mutually-exclusive `redirect:` oneof — but it widened the set of
errors `reparse_and_select_route_config` can return **without extending the consumer that its own
in-code contract comment demands be extended**. The result is that an RDS hot reload carrying
exactly the config CF-76-2 was filed about now reaches an `unreachable!()` and **panics**; release
builds are `panic = "abort"`. CF-76-2's failure mode was converted from *"installs a bad config"*
into *"aborts the proxy"*, which is worse than the gap it closed, and no test covers it.

Per §5.2 this is a re-entry at **state 3**, not a re-verification at state 4.

**Everything else is banked, not required.** The nine Minors and nine Nits are recorded for the
§5.2 re-entry's `PLAN.md` to schedule or explicitly defer (§6.3). See §6 for the recommended split.

---

## 2. How this review was conducted

Six **read-only** review dimensions were fanned out in parallel (the state-5 parallelism rule:
subagents return CITATIONS, the main session makes every decisive measurement itself):
the pure `location`-builder; the response builder + dispatch seam + `&mut Request` widening;
fixture `0086`; the CF-76-2 / M-1 / M-2 config-crate work; the `BEHAVIOR_CONTRACT.md` Phase 76
bank; and test quality / mutation resistance.

**Every finding below was RE-VERIFIED ON DISK by the main session before being written down.** A
subagent finding is a claim, not a result. Two consequences of that discipline are worth recording:

- One subagent's line numbers for `crates/envoy-http1/src/hcm.rs` were consistently **~12 lines
  high** (e.g. `:2331` for `matched_len`, which is at `:2319`). Every citation in this file was
  re-read directly. **Do not inherit a line number from a review either.**
- One subagent's headline finding — that upstream strips a *default* port (`:80`/`:443`) from the
  request authority when the redirect changes the scheme — was explicitly labelled by that agent
  as *"a hypothesis from upstream implementation shape, not from anything on disk."* It is
  therefore **NOT graded as a finding**. It is recorded in §5 as an unverified hypothesis with the
  probe that would settle it. Banking an unmeasured upstream claim as a defect is exactly the
  error this project's doctrine forbids.

Scope reviewed: `git diff 0ea2de1..5782a06` — 12 non-`docs/` files, +1334/−69, plus
`BEHAVIOR_CONTRACT.md` +169.

---

## 3. ISSUE (must fix — this is the §5.2 re-entry's charter)

### I-1 — the CF-76-2 fix makes an RDS hot reload **panic** on exactly the config it was written to reject

**`crates/envoy-http1/src/rds_watcher.rs:216-222`**, consequence of
**`crates/envoy-config/src/rds.rs:159`**.

**The chain, every link measured on disk:**

1. `validate_redirect_oneofs` (`crates/envoy-config/src/bootstrap.rs:2674`) returns
   `ConfigError::RedirectPathRewriteConflict` (`:2680`) or `RedirectSchemeRewriteConflict`
   (`:2686`).
2. Task 8 added a call to it inside `reparse_and_select_route_config`
   (`crates/envoy-config/src/rds.rs:159`). So that function can now return **two variants it could
   not return before**.
3. Its **sole production caller** is the reload pipeline at
   `crates/envoy-http1/src/rds_watcher.rs:184` (verified by workspace-wide grep — every other hit
   is a doc comment or an in-crate test).
4. That caller classifies the error with a `match` over exactly **four** variants —
   `RdsFileError`, `RdsParseError` (`rds_watcher.rs:202-206`), `RdsRouteConfigNotFound`,
   `UnknownCluster` (`:208-210`) — and ends:

   ```rust
   other => {
       unreachable!(
           "reparse_and_select_route_config returned an unexpected \
            ConfigError variant not handled by the reload classifier: \
            {other:?}"
       )
   }
   ```

5. `grep -c 'RedirectPathRewriteConflict\|RedirectSchemeRewriteConflict' crates/envoy-http1/src/rds_watcher.rs`
   returns **0**. The classifier was not extended.
6. The comment immediately above that arm (`rds_watcher.rs:211-215`) states the invariant the
   phase broke, verbatim: *"`reparse_and_select_route_config` can return ONLY the four variants
   matched above… **If a new variant is added, this match must be extended explicitly.**"*
7. `Cargo.toml:42` sets `panic = "abort"` under `[profile.release]`.

**Concrete failure scenario.** A deployment uses file-based RDS. An operator edits the RDS file to
add a route with `redirect: { path_redirect: "/p", prefix_rewrite: "/q" }` (or
`https_redirect` + `scheme_redirect`). The file watcher fires the reload; `reparse_and_select_route_config`
correctly returns `RedirectPathRewriteConflict`; the classifier falls to `other =>` and panics. In
a release binary `panic = "abort"` kills **the whole proxy process** on a routine config edit. In
debug/test builds the panic unwinds inside the spawned watch task, so RDS hot reload dies silently
for that target with no counter ticked.

**Why it matters more than the gap it closed.** CF-76-2 was adjudicated MINOR in the `76.1` review
on the measured grounds that the blast radius was *nil* — the runtime arm was an inert 501 either
way. This phase makes the arm live, which is the correct trigger for closing it; but the close
stopped at the crate boundary. The intended outcome was a clean **warm reject** (last-good table
retained, `update_rejected` ticked). The actual outcome is an abort.

**Why nothing caught it.** `unreachable!()` compiles cleanly, so gate (e) cannot see it. The two
new tests (`crates/envoy-config/src/rds.rs:413`, `:438`) call
`reparse_and_select_route_config` directly and never reach `reload()`. The four existing
`reload()` tests (`crates/envoy-http1/src/rds_watcher.rs:360`, `:392`, `:421`, `:456`) cover
happy / malformed / name-not-found / unknown-cluster only.

**Fix shape.** Add both variants to the `update_rejected` arm at `rds_watcher.rs:208-210` — they
are validation rejections, the same class as `UnknownCluster` — and add a `reload()`-level test
asserting the counter ticks and the last-good table survives, rather than a panic. The fix is
small; the missing test is the part that matters.

---

## 4. MINOR (real, recorded, not required for the re-entry)

### M-1 — `BEHAVIOR_CONTRACT.md` states that §F items 7 and 8 are "pinned by in-process tests". **Neither is pinned by any test.**

**`docs/envoy-rust/BEHAVIOR_CONTRACT.md:3122-3124`**; branches at
**`crates/envoy-http1/src/hcm.rs:2319`** and **`:2323-2326`**.

§F items 7 and 8 are this phase's two *invented* cells — behaviours the implementation chose rather
than measured. §F labels them honestly (`**[introduced by the 76.2 implementation, not by the
recon]**`, "a *choice*, not a measurement") and correctly says they are unwitnessed by the
differential fixture. Then its closing note adds:

> "they are envoy-rust's current choice, **pinned by in-process tests**, and never compared
> against upstream."

**MEASURED FALSE for both.**

- **Item 7** (`matched_prefix == None` ⇒ the whole path is the matched span). All eight
  `plan_redirect` call sites in tests pass `Some(...)`
  (`hcm.rs:10722, 10738, 10747, 10753, 10783, 10789`). The 22-row table *cannot* express `None`:
  `Cell.prefix` is typed `Option<&'static str>` (`:10461`) but the only constructor hard-wires
  `prefix: Some(prefix)` (`:10484`), and all 22 rows go through it (0 direct `Cell { … }`
  literals). Every redirect route builder in the tree is `prefix:`-matched
  (`hcm.rs:9856-9857`, `:9938-9939`, `crates/envoy-http2/src/hcm.rs:6878-6879`).
  Mutating `map_or(path.len(), str::len)` → `map_or(0, str::len)` at `hcm.rs:2319` survives the
  entire suite.
- **Item 8** (the query rides along on the rewritten `:path`). No test combines `prefix_rewrite`
  with a query-bearing target: every one uses `/e-pfx/sub`, `/ab` or `/\u{e9}`. Deleting
  `Some(q) => format!("{rewritten}?{q}")` at `hcm.rs:2324` survives the entire suite.

**Why it matters.** This is the precise inversion of what §F exists for. §F correctly warns a
future session that these are choices — then tells that session a safety net exists which does
not. A refactor can silently flip either cell with a fully green gate.

**Fix.** Either delete the four words "pinned by in-process tests", or make them true: one `Cell`
with `prefix: None`, and one `rewritten_path` assertion on a query-bearing target. The second is
~4 lines and is strictly better, because `plan_redirect` is pure and total so the characterization
is free.

*Independently reached by the main session and by the contract-audit dimension.*

### M-2 — fixture `0086` has **no** differential witness for the `host_redirect` port-DROP, and two shipped docs claim it does

**`tests/differential/tests/route_redirect_action.rs:9-10`** and
**`tests/fixtures/0086-route-redirect-action/README.md:16`**.

The entrypoint's module doc says: *"the AUTHORITY ASYMMETRY — `host_redirect` DROPS the request's
original port (probe `q01` vs `r01`)"*. That pair cannot show it. Measured by parsing the route
table and the probe list and running first-match-wins selection:

- **9** routes set `host_redirect` (`/a-host`, `/b-query`, `/g-c307`, `/h-strip`, `/i-port`,
  `/l-both`, `/m-see`, `/o-found`, `/p-perm`).
- **All nine** are probed with the *unported* `host: "envoy-rust.test"`.
- The only two ported probes are `q01` and `q03` (`host: "envoy-rust.test:1234"`), whose routes are
  `redirect: { https_redirect: true }` and `redirect: {}` (`envoy.yaml:58-61`) — **neither sets
  `host_redirect`**.
- **Probes combining `host_redirect` with a ported `Host:` — 0.**

So a mutation making the `host_redirect`-set branch *append* the request's port instead of dropping
it leaves all 18 probes GREEN, on the rule SPEC §2.4(b) itself calls *"the one rule a from-scratch
implementation is most likely to get wrong."*

**Not unpinned, only uncompared.** The cell *is* pinned in-process — `hcm.rs:10671-10679`, cell
`"Q2 host_redirect SET DROPS the request's port — the asymmetry"`. The defect is the two doc
claims, plus the fact that `BEHAVIOR_CONTRACT.md` §F records two *other* unwitnessed cells
explicitly and omits this one, which is the same class and concerns the headline rule.

Root cause is a reasoning step in `SPEC.md:384-386`: *"Q2 duplicates R1's `location` … so they add
config lines without adding a distinguishable cell."* True of the **output**, false of the
**cell** — Q2's *input* `Host` differs, and that is exactly what makes it the only differential
discriminator.

**Fix.** Either add a 19th probe (a new `/q2-hostport` route with
`redirect: { host_redirect: "example.com" }`, probed with `host: "envoy-rust.test:1234"`; no
shadowing risk, no existing prefix is a prefix of it), or correct the two doc claims and add the
gap to §F alongside items 7 and 8.

### M-3 — the three new reason phrases are pinned only at the lookup table, never end-to-end

**`crates/envoy-http1/src/hcm.rs:2359`** (`reason: None`);
test **`crates/envoy-http1/src/response.rs:521`**.

SPEC §2.1 correctly identifies the reason phrase as *"a silent-wrong-answer hazard the differential
fixture CANNOT catch"* — the harness parses the status **code** only. The in-process pin added
(`canonical_reason_covers_the_three_redirect_codes`) asserts the exact strings, which is right, but
it tests the **pure lookup function**. Nothing asserts that a redirect `Response` actually reaches
it. `canonical_reason` is consulted only at `crates/envoy-http1/src/response.rs:99`
(`resp.reason.unwrap_or_else(…)`), so setting `reason: Some("OK")` in `synth_redirect` would
restore `HTTP/1.1 303 OK` on the wire and survive the whole workspace. Measured: the strings
`See Other` / `Temporary Redirect` / `Permanent Redirect` appear in **no** file other than
`crates/envoy-http1/src/response.rs`.

**Fix.** One line — `assert_eq!(resp.reason, None)` in the `synth_redirect` test at
`hcm.rs:10800`. Cheap, and it closes the only hazard the phase itself flagged as invisible to the
fixture.

### M-4 — the `RouteAction::Redirect` variant doc is now stale and says the runtime is still the placeholder

**`crates/envoy-config/src/bootstrap.rs:2260-2263`**:

> `/// 76.1 NEW: the config surface only. The runtime dispatch arm is an honest`
> `/// `synth_501` not-implemented placeholder until 76.2 lands the real`
> `/// behaviour (ADR-0169 DECISION 4).`

76.2 landed it. `bootstrap.rs` **was** edited by this phase (`validate_redirect_oneofs` was lifted
into it at `:2674`), so the doc was in scope and was missed. A future reader greps `synth_501`,
lands here, and concludes redirect is still inert. Note this is the same *class* as `76.1`'s
M-1/M-2 — which this phase's Task 9 correctly fixed — reappearing 8 lines below the fix.

### M-5 — the two new `ADR-0028` citations are misattributions

**`crates/envoy-config/src/rds.rs:94-95`** and **`:165-166`**, both new in this phase, say
`direct_response` re-validation *"stays deferred under the OPEN ADR-0028 deferral."*

**ADR-0028** (`docs/envoy-rust/DECISIONS.md:513-533`) is *"Resolution of the `envoy-http1` ↔
`envoy-http2` cycle introduced by SPEC §3 D4 router dispatch."* Its Decision is option (B), defer
the H1-listener-side dispatch. Measured: the ADR body contains **zero** occurrences of
`direct_response`. The pre-existing citation at `rds.rs:135`/`:140` uses ADR-0028 **correctly**
(for the `Http2ClusterFromHttp1Listener` H1×H2 gate), as does every other reference in the tree.

**Mitigating.** The mislabel is **inherited, not minted** — the same association appears in the
landed `76.1/REVIEW.md` narrative and in `STATE.md`. This phase transcribed it into code comments,
which is where it will now be greppable and durable.

**Why it matters.** In this project an ADR citation is the audit trail a later session greps to
decide whether a `=> {}` arm is a deliberate deferral or an oversight. Chasing ADR-0028 lands on a
dependency-cycle ADR. **Do not edit ADR-0028** (append-only, D-3.5): either cite the real
justification (a `direct_response` route names no cluster and cannot panic the request path) or
open a new ADR — **ADR-0171 is next free**.

### M-6 — `strip_port` is applied to the *configured* `host_redirect`, not only to the request authority

**`crates/envoy-http1/src/hcm.rs:2302`**: `format!("{}:{}", strip_port(host_part), port)`, where
`host_part` is `rd.host_redirect.as_deref().unwrap_or(authority)` (`:2300`). `strip_port`
(`hcm.rs:2165-2170`) is `rfind(':')`-based.

Two wrong outputs, both reachable from an accepted config (`validate_redirect_oneofs` checks only
the two oneof pairs; nothing constrains the shape of `host_redirect`):

- `host_redirect: "[::1]"` + `port_redirect: 443` → `strip_port` finds the `:` at byte 2 and
  returns `"[:"`, so `location: http://[::443/…` — an unparseable URI. (Verified by executing the
  same algorithm on the four relevant inputs.)
- `host_redirect: "example.com:9000"` + `port_redirect: 443` → `example.com:443`, silently
  *rewriting* the configured port rather than being an unmeasured concatenation.

`BEHAVIOR_CONTRACT.md` §F item 6 flags `strip_port`'s IPv6 handling as unprobed, but scopes it to
*"a redirect echoing the authority"* — it does not cover `strip_port` being applied to
config-supplied text. Note the *non*-`port_redirect` path is fine: the authority is echoed
verbatim and never touches `strip_port`.

### M-7 — the H2 shared-seam test does not exercise the H2 request path

**`crates/envoy-http2/src/hcm.rs:6910`** (`h2_shared_seam_serves_the_redirect_arm`), call at
**`:6929`**.

The test hand-builds an `envoy_http1::Request` and calls `build_response(&h1cfg, &mut req, false)`
directly. It never drives an H2 stream, so `handle_one_stream` and the actual seam call at
`crates/envoy-http2/src/hcm.rs:518` are never executed — the test would still pass if `:518` were
deleted or diverted.

To be fair to the implementation: the test's **own doc block is honest** ("pins that the seam is
reachable from the H2 crate"), it follows the established sibling precedent
(`h2_resolve_route_reachable_and_returns_cors_route`), it genuinely pins that
`Http1HCMConfig::from_config` with `codec_type: HTTP2` preserves a `RouteAction::Redirect` into the
route table, and the state-3 session's cross-crate mutation (mutating `plan_redirect` in
`envoy-http1` turned this `envoy-http2` test RED) really does prove H2's answer is *computed by*
H1's code. What it does **not** prove is that H2's dispatch *reaches* it. SPEC §6 promised "an
HTTP/2 in-process redirect test proving the shared seam really does serve H2"; this is a weaker
article. A real end-to-end harness exists in the same file (`spawn_h2_hcm`, used by ~30 tests),
which would additionally have exercised the H2 forbidden-header strip that removes
`synth_redirect`'s `connection` header on the H2 path — currently the untested half of the H2
redirect wire shape.

### M-8 — only one of the two redirect oneofs is tested on the RDS warm path

**`crates/envoy-config/src/rds.rs:413`** covers `path_redirect` + `prefix_rewrite` →
`RedirectPathRewriteConflict`. Measured: `RedirectSchemeRewriteConflict` appears in
`bootstrap.rs` (definition + boot-path tests) and in the shared function only — **never** in an
RDS-path test. Both rules funnel through the same function so the risk is low, but the
newly-closed CF-76-2 hole is pinned for one oneof, not both.

**Worth recording as a strength alongside it:** `rds.rs:438`
(`rds_reload_accepts_a_valid_redirect_route`) is a deliberate **accept-direction control**, written
so the reject test cannot pass by rejecting everything. That discipline is uncommon and correct.

### M-9 — the `connection` header VALUE has no witness in either direction

**`crates/envoy-http1/src/hcm.rs:2363-2366`**.

`synth_redirect` derives `connection` from `connection_value(close)`, correctly. But the unit test
calls `synth_redirect(301, …, true)` (`hcm.rs:10801`), and the differential driver
unconditionally appends `Connection: close` to every probe (`tests/differential/src/lib.rs:2069`),
so all 18 fixture probes are also `close=true`. Replacing `connection_value(close)` with the
literal `"close"` passes the unit test **and** the whole fixture green. One `close=false`
assertion closes it.

---

## 5. An unverified hypothesis — recorded, NOT graded, and NOT a finding

A review dimension proposed that upstream Envoy additionally **strips a default port** from the
request authority when the redirect changes the scheme (`:80` dropped when the request was `http`,
`:443` when `https`), which envoy-rust would not do. The agent explicitly labelled this *"a
hypothesis from upstream implementation shape, not from anything on disk."*

**It is therefore not graded.** No measurement supports it, and this project does not bank an
unmeasured upstream claim as a defect (D-3.3: the contract is the contract; you do not read
upstream source to decide what equivalence means).

It is recorded here only because it is **cheap to settle and none of the 22 measured cells can
discriminate it** — every ported probe used `:1234`, which is neither default port, and every
unported probe had no port to strip. The two probes that would settle it, for a future phase that
chooses to measure:

| `Host:` sent | route config | discriminates |
|---|---|---|
| `envoy-rust.test:80` | `redirect: { https_redirect: true }` | scheme changed — does `:80` survive? |
| `envoy-rust.test:80` | `redirect: {}` | scheme unchanged — control; `:80` should survive |

If measured and confirmed, SPEC §2.4(b)'s bullet *"a scheme-only change does not normalise a
now-redundant port"* would need narrowing, since it was read off Q1 (`:1234`) — a cell where the
hypothesised upstream rule is also a no-op.

---

## 6. NITS

- **N-1** `crates/envoy-http1/src/hcm.rs:2285-2286` — the doc says the scheme falls back to *"the
  scheme the request arrived on"*, but `plan_redirect` takes no scheme/TLS input and the arm at
  `:2290` can only ever return the constant `"http"`. Consistent with §F item 1 (TLS not measured)
  and currently unreachable-wrong, but a future phase generalising to TLS will read the comment as
  an implemented rule.
- **N-2** `crates/envoy-http1/src/hcm.rs:9834-9836` — `redirect_route_config`'s doc says the route
  is *"`prefix`- or `path`-matched as the caller chooses"*; the body hardcodes
  `prefix: Some(prefix), path: None` (`:9856-9857`) and has no such parameter. This is the helper
  that *would* have made M-1 item 7 testable.
- **N-3** `tests/fixtures/0086-route-redirect-action/README.md:19` and
  `tests/differential/tests/route_redirect_action.rs:14` — "all five `response_code` values" is
  four. `envoy.yaml` spells `TEMPORARY_REDIRECT`, `SEE_OTHER`, `FOUND`, `PERMANENT_REDIRECT`;
  `MOVED_PERMANENTLY` never appears — the 301 rows take it from the default. Five *status codes*
  reach the wire; four *enum values* are exercised.
- **N-4** `crates/envoy-http1/src/hcm.rs:10808-10811` — `assert!(!names.contains(&"content-type"))`
  is strictly implied by the exact `Vec` equality three lines above and can never fail
  independently. Harmless documentation.
- **N-5** `docs/envoy-rust/BEHAVIOR_CONTRACT.md:3118-3120` — §F item 8's stated reason for being
  unwitnessed ("`0086`'s `r05` probe is deliberately query-free") names the wrong mechanism. The
  rewritten `:path` is an **access-log** observable and `0086` uses `driver: { kind:
  http1_probe_list }`, which compares responses only — so `0086` could not witness item 8 **even
  with a query on `r05`**. The contract states the correct reason itself 34 lines earlier at
  `:3084`. A future session reading item 8 literally would add `?k=v` to `r05` and believe the cell
  became witnessed.
- **N-6** `docs/envoy-rust/phases/76.2-redirect-runtime-fixture/SPEC.md:394-396` and commit
  `b9afd81`'s message — "exactly three hunks" is **three logical edits** but **two** `diff -u`
  hunks at default `-U3` (the `node:` prepend and the `0.0.0.0`→`127.0.0.1` change coalesce).
  Re-verified: the three logical edits are all present and the route table is byte-identical. A
  session re-checking the constraint with `grep -c '^@@'` will read 2 and suspect drift. Phrase it
  as "three logical edits" in future SPECs.
- **N-7** `docs/envoy-rust/BEHAVIOR_CONTRACT.md:2995`/`:2997` — rows Q1/Q3 silently re-anchor the
  recon's targets (`/f-https/x` → `/q1-hostport/x`, `/j-bare/d` → `/q3-hostport/d`) onto the
  fixture's own dedicated routes. The re-anchoring is **correct and arguably better** (those are
  the shipped probes, and a green `0086` really is an upstream measurement of those strings), but
  nothing says so, and anyone diffing the contract against SPEC §2.3 sees two rows that don't
  reconcile.
- **N-8** `crates/envoy-http1/src/hcm.rs:2340` — a `path_redirect` / `prefix_rewrite` with **no
  leading `/`** is unvalidated and concatenated straight onto the authority:
  `path_redirect: "newpath"` yields `http://envoy-rust.testnewpath`. Every measured row uses a
  leading slash. Unmeasured and not listed in §F.
- **N-9** `crates/envoy-http1/src/hcm.rs:2310-2340` — a `path_redirect` that itself carries a query
  produces a double-`?` location: `path_redirect: "/new?a=1"` against `/d-pathq/x?k=v` yields
  `http://h/new?a=1?k=v`, because `new_path` and `query_suffix` are computed independently and
  concatenated. Unmeasured and not listed in §F.

---

## 7. STRENGTHS — what this phase did well

These are not courtesies; each is a measured property that a re-entry must not regress.

- **The 22-cell table is the real thing.** `crates/envoy-http1/src/hcm.rs:10457-10726` asserts
  **both** `location` and `status` per row (`:10723-10724`), carries a per-row `label` so a failure
  names the exact cell, and length-guards itself (`assert_eq!(cells.len(), 22)`, `:10720`). Every
  row's expected value was cross-checked against SPEC §2.3 — **zero transcription errors**,
  including the four traps (R14/Q4 keep the redundant `:443`; Q2 drops `:1234`; Q1/Q3 keep it; E2's
  empty `path_redirect` leaves the path intact). No dead columns: every `Cell` field is consumed.
- **The authority asymmetry is correctly encoded** (`hcm.rs:2300-2304`) — the rule the SPEC named
  as the most likely to be got wrong.
- **`plan_redirect` is genuinely pure and total.** No `unwrap`/`expect`/`panic!`, no arithmetic, no
  indexing. The one byte-index slice (`hcm.rs:2167`) takes its index from `rfind(':')` — always a
  char boundary. `path.get(matched_len..).unwrap_or("")` (`:2320`) is range- and boundary-safe by
  construction, and the totality test (`:10776-10792`) genuinely reaches the `str::get → None`
  path.
- **`synth_redirect`'s test uses exact `Vec` equality on header names in wire order**
  (`hcm.rs:10803-10807`), not a `contains`. That catches an added, reordered *or* missing header —
  strictly stronger than the `synth_overflow` precedent it was modelled on, and it kills the
  `synth_with` regression at the root.
- **Both directions of the `:path` asymmetry are pinned, at two levels** — pure
  (`hcm.rs:10732-10763`) and through `build_response` (`:9874`, `:9893`) — and the non-mutation
  test *also* asserts the location changed, so it cannot pass vacuously. Pinning the negative
  direction is the part most phases skip.
- **The deliberate T-C9 flip is strictly stronger.** The old
  `build_response_redirect_is_not_implemented_placeholder` (at `0ea2de1`, `hcm.rs:9731`) asserted
  two things (`status == 501`, `detail == None`). The replacement
  `build_response_redirect_emits_301_and_location` (`hcm.rs:9970`) asserts four: the 301, the exact
  `location`, the **absence** of `content-type`, and `detail == Some("direct_response")`. Nothing
  the old test asserted is now unasserted, the rename makes the change visible rather than silent,
  and the old doc block was correctly moved onto the new test rather than left glued to the helper.
- **Fixture `0086` verifies clean against every mechanical constraint**, re-computed by this
  review rather than taken on trust: 18 probes / 18 distinct paths / 18 routes / 18 distinct
  prefixes / **0 shadowing pairs** / 18 distinct routes selected / no unprobed route / no unmatched
  probe; assertion density **18/18/18** (`expected_status`, `expected_headers`, `expected_body`);
  zero `path:` matchers (keeping it clean of the open CF-76-1); zero `{{ADMIN_PORT}}`; and
  `envoy-rust.yaml` differing from `envoy.yaml` by exactly the three intended logical edits with
  the **route table byte-identical**.
- **The two authority-port probes landed intact** (`q01`, `q03`, both `host:
  "envoy-rust.test:1234"`). Without them `location` would not be byte-comparable at all between two
  proxies on different ports — this is the design insight that makes the fixture possible.
- **`%RESPONSE_CODE_DETAILS%` reuses the existing `"direct_response"` literal** (`hcm.rs:2138`)
  rather than inventing one — zero new `Op` or `AccessLogRecord` surface, exactly as measured.
- **`synth_501` was correctly NOT deleted** — defined `hcm.rs:2501`, still consumed by the chunked
  `Transfer-Encoding` path at `hcm.rs:915` and `crates/envoy-http1/src/uring.rs:285`.
- **CF-76-2's config-crate half is cleanly closed.** `rds.rs` has no live `if let` over
  `RouteAction` (the single grep hit, `rds.rs:408`, is a test doc comment quoting the *superseded*
  code — adjudicated by line, not by count); the dispatch is a genuine exhaustive `match` with
  **no** `_ =>` catch-all (`rds.rs:151-171`), restoring the compile-time forcing function; the
  shared `validate_redirect_oneofs` is called by both paths (`bootstrap.rs:4111`, `rds.rs:159`)
  with zero duplicated logic left behind; and it is correctly **presence**-based
  (`.is_some() && .is_some()`, `bootstrap.rs:2679`/`:2685`), not truthiness-based, which is what
  the measured rule requires.
- **M-1 and M-2 from the `76.1` review are genuinely fixed, together.** `pub enum RouteAction`
  (`bootstrap.rs:2253`) carries its own doc block again, and it is **corrected** — it describes the
  three-way oneof, not the stale two-way text — while `RedirectResponseCode` retains its own.
  Nothing orphaned, nothing duplicated. Fixing them together was the right call; a verbatim restore
  alone would have re-attached stale text.
- **`BEHAVIOR_CONTRACT.md` §A-§E is substantively accurate.** Every factual claim was cross-checked
  against SPEC §2 and against the code: no false measured claim was found, and — the specific thing
  this review hunted — **no §A-§E section banks item 7 or item 8 as a measured rule**. §A(c) is
  correctly scoped to "the span matched by the route's `prefix:` matcher" and §D's example is
  query-free, so the body and the §F disclaimer do not contradict each other. (The defect is
  confined to the closing note — M-1.)
- **§E is the strongest paragraph in the bank** (`BEHAVIOR_CONTRACT.md:3088-3098`) — the standing
  prohibition on allow-listing `location`, stated in the contract **and** independently in the
  fixture README and the entrypoint, so the fact survives losing any one document. Verified:
  `HEADER_ALLOW_LIST` is still exactly three entries and `location` appears nowhere in
  `tests/differential/src/lib.rs`.
- **The deviations table D-1..D-6 is exemplary.** Three of the six are defects in `PLAN.md`'s own
  *pre-flighted* literal Rust (a `str` slice that panics mid-codepoint, an `assert_eq!` message
  whose bare `{}` is a format placeholder, a `HttpVersion::Http2` that does not exist). Each was
  recorded with its measurement rather than papered over, and `PLAN.md` was correctly left
  unedited (D-3.5). A reviewer diffing plan text against landed code finds every difference
  explained.

---

## 8. What this review did NOT do

- **Did not re-run the §7.5 gate.** It was adjudicated at state 4 and its real output is in
  `PROGRESS.md` §S4.0-§S4.11 — five of six green, gate (f) open by construction because it *is*
  this review. This session ran no `cargo` command.
- **Did not fix anything it graded** (ADR-0127; ADR-0165). I-1 in particular is left open for the
  §5.2 re-entry; the reviewer identifying a defect does not make it the reviewer's to repair.
- **Did not flip any ROADMAP status cell.** Re-measured at this commit: row `76.2` `planned`, `76`
  `in-progress`, `76.1` `done` — unchanged.
- **Did not edit** `76.2/SPEC.md`, `76.2/PLAN.md`, `PROGRESS.md`, `76/SPEC.md`, or any of `76.1`'s
  four artifacts (D-3.5). Several findings above name defects *in* those files (M-2's root cause in
  `SPEC.md:384-386`, N-6's "three hunks"); they are recorded, not repaired.
- **Did not add an ADR.** Head **ADR-0170**, next free **ADR-0171**, re-derived on disk. Nothing
  here settles a new decision — the severity vocabulary is the project's own, the §5.2 re-entry
  rule is `BOOTSTRAP_PROMPT.md` §5.2, and the no-fix-what-you-grade rule is ADR-0127/ADR-0165.
  M-5's fix, if a later session takes it, **will** need ADR-0171.
- **Did not touch any banked carry-forward** (§6.3). CF-76-1, CF-75-2..6 and the `76.1` review's
  remaining Minors/Nits are untouched and still open.
- **Did not add `location` to `HEADER_ALLOW_LIST`, trim `known-failures.txt`, weaken a fixture, or
  create a `stop` file.**

---

## 9. Disposition — what the §5.2 re-entry owns

**Required (the Issue):**

- **I-1** — extend the `rds_watcher.rs` reload classifier and add the `reload()`-level test.

**Recommended in the same re-entry** (each is a few lines, each is about *this phase's own*
artifacts, and each is a claim-vs-reality mismatch rather than a feature):

- **M-1** — make "pinned by in-process tests" true, or delete the claim.
- **M-2** — add the 19th probe, or correct the two doc claims and bank the gap in §F.
- **M-3** — `assert_eq!(resp.reason, None)`.
- **M-4** — the stale `RouteAction::Redirect` doc.

**Banked, not scheduled** (§6.3 — do not fix opportunistically): M-5 through M-9 and N-1 through
N-9. M-5 needs ADR-0171 if taken. M-6 and N-8/N-9 belong with the §F expansion of unmeasured
`plan_redirect` edges and would be better served by one measured probe session than by guessing.

**A re-entry must not:** weaken any of the 22 table cells, collapse the table's per-row `label` or
its `cells.len() == 22` guard, reduce `synth_redirect`'s exact-`Vec` header assertion to a
`contains`, delete `synth_501`, re-introduce an `if let` over `RouteAction` in `rds.rs`, add
`location` to `HEADER_ALLOW_LIST`, or edit `SPEC.md` / `PLAN.md` / this file's inputs.
