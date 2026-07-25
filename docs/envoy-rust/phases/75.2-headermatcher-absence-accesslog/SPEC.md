# Phase 75.2 — `HeaderMatcher` absence semantics: the ACCESS-LOG-path differential witnesses + the contract bank

> **What this document is.** The `SPEC.md` for sub-phase **75.2**, created by the
> §6.1 SPLIT of phase 75 at that phase's §5 state-2 PLAN-write (ADR-0157). It
> redistributes the parent `SPEC.md`
> (`docs/envoy-rust/phases/75-headermatcher-absence-parity/SPEC.md`, which stays
> on disk as the parent record and is NOT edited) per `BOOTSTRAP_PROMPT.md` §6.2
> step 3.
>
> **Written for a stranger with zero prior context (D-3.4).** Every behavioral
> claim below was MEASURED against `envoyproxy/envoy:v1.33.0` (the
> `ENVOY_TARGET.md` pin) at the phase-75 state-2 PLAN-write on
> `HEAD == 5d78df443461d002db5ce9cc9d6b238fe1de6b66`, or is cited to a file:line
> verified on disk in that same session. No Envoy C++ source was read (D-3.3).
>
> **DEPENDS ON 75.1, which must be `done` first.** 75.1 lands the engine fix
> itself. **This sub-phase changes NO engine code** — it adds the second
> consumer's cross-proxy witness and banks the contract rows. If 75.1 has not
> landed, the fixtures here would encode the OLD (wrong) behavior and go red the
> moment 75.1 lands.

---

## §1. Goal

Witness the 75.1 engine fix cross-proxy on the **access-log** path — the second
consumer of the shared `HeaderMatcher` engine, reached through the ADR-0150
`HeaderMatch` trait seam — with TWO new backend-free byte-exact differential
fixtures, and bank the measured `present_match`-polarity rule plus the
reject-direction carry-forwards into `BEHAVIOR_CONTRACT.md`.

---

## §2. Context: the rule 75.1 lands, and why the access-log path needs its own witness

### 2.1 The MEASURED rule (landed by 75.1)

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

- **D1** (= carry-forward **CF-72-1**): a VALUE matcher (`exact_match` /
  `prefix_match` / `suffix_match` / `safe_regex_match` / `range_match` /
  `string_match`) + `invert_match: true` + ABSENT header — upstream DROPS, the
  pre-fix in-tree engine KEEPS.
- **D2**: upstream `present_match: false` means **"the header must be ABSENT"**;
  the pre-fix in-tree engine treats it as unconditionally true. Fires on a plain,
  NON-inverted matcher.
- **P1 — the guard**: `present_match: true` + `invert_match` is FULL PARITY and
  must stay so.

### 2.2 The access-log path shares the engine

The access-log `header_filter` arm evaluates the SAME engine through the
ADR-0150 trait seam: `crates/envoy-accesslog/src/filter.rs:139`
(`LogFilter::Header { matcher } => matcher.matches(headers)`) dispatches to
`crates/envoy-config/src/matcher.rs:69`
(`impl envoy_accesslog::HeaderMatch for HeaderMatcher`), whose trait object is
injected at `crates/envoy-http1/src/hcm.rs:1784-1786` inside
`compile_access_log_filter`. `envoy-accesslog` must not depend on `envoy-config`
(the reverse edge exists → cycle), which is why the matcher crosses as
`Arc<dyn HeaderMatch>`.

### 2.3 MEASURED cross-proxy on the access-log path (state-2 PLAN-write)

Method: one boot per proxy; **nine** `FileAccessLog` sinks, one per
`header_filter` under test, each writing a distinct file with a distinct
`text_format_source` (`sN PATH=%REQ(:PATH)%`); four requests — `/absent` with no
`x-a`, `/valmatch` with `x-a: v`, `/valmiss` with `x-a: zzz`, `/empty` with an
EMPTY-VALUE `x-a;`; then a graceful `docker stop -t 15` (SIGTERM) so Envoy's
FileAccessLog buffer flushes, and the files retrieved with `docker cp` (**not**
`--volumes-from`, which cannot reach a stopped container's `/tmp`).

The parent SPEC measured this table on the UPSTREAM side only. **It is now
measured on BOTH sides.** Sinks s5/s6 are boot-fatal in envoy-rust (they are the
CF-72-2 reject-direction gaps) and were therefore omitted from the envoy-rust
config; every other sink ran on both.

| sink | `header_filter.header` | upstream logged | envoy-rust logged (pre-fix) | verdict |
|---|---|---|---|---|
| s1 | `exact_match: v` + invert | `/valmiss`, `/empty` | `/absent`, `/valmiss`, `/empty` | **DIVERGE (D1)** — rust logs an extra `/absent` |
| s2 | `present_match: false` | `/absent` | `/absent`, `/valmatch`, `/valmiss`, `/empty` | **DIVERGE (D2)** — rust logs 3 extra |
| s3 | `present_match: false` + invert | `/valmatch`, `/valmiss`, `/empty` | *(nothing)* | **DIVERGE (D2)** — rust logs 3 too few |
| s4 | `present_match: true` + invert | `/absent` | `/absent` | **PARITY (P1)** — the guard |
| s5 | name-only `{ name: x-a }` | `/valmatch`, `/valmiss`, `/empty` | *(boot-fatal)* | CF-72-2, reject-direction |
| s6 | `string_match {exact: v}` + `treat_missing_header_as_empty` | `/valmatch` | *(boot-fatal)* | CF-72-2, reject-direction |
| s7 | `exact_match: v` | `/valmatch` | `/valmatch` | PARITY control |
| s8 | `string_match {exact: v}` + invert | `/valmiss`, `/empty` | `/absent`, `/valmiss`, `/empty` | **DIVERGE (D1)** |
| s9 | `present_match: true` | `/valmatch`, `/valmiss`, `/empty` | `/valmatch`, `/valmiss`, `/empty` | PARITY control — and an EMPTY value counts as PRESENT on both |

**Every cell is exactly what §2.1 predicts, and matches the route-path matrix
CELL-FOR-CELL** (parent SPEC R-0.2 / 75.1 SPEC §2.3). The rule is UNIFORM across
subsystems — which is why the fix is one expression, and why this second witness
is about the SEAM rather than about a different rule.

s9 and the `/empty` column are new at state-2 (the parent SPEC had neither).

---

## §3. Why this needs TWO fixtures, not one — the driver constraint

This is the finding that forced the §6.1 split, and it REFUTES the parent SPEC's
§5 design for its access-log fixture. **MEASURED on disk at state-2:**

- `AccessLogPaths` (`tests/differential/src/lib.rs:1088-1093`) is
  `{ envoy: String, envoy_rust: String }` under `deny_unknown_fields` — **exactly
  ONE log file per side.**
- `run_http1_access_log_byte_exact_arm` reads exactly those two paths
  (`lib.rs:6344`, `:6365`, `:6403-6412`); there is no per-sink dimension in the
  arm at all.
- Only the **envoy-side parent directory** of that one path is bind-mounted into
  the container (`lib.rs:4019`, a single-element
  `vec![(envoy_parent_s.clone(), envoy_parent_s)]`), so a second sink writing
  elsewhere would not even be visible to the host.
- Census: across all 82 fixtures the maximum number of
  `- name: envoy.access_loggers.file` sinks in ANY config is **1**.

The parent SPEC §5 specified ONE fixture `0084` with "multiple `FileAccessLog`
sinks … each with a distinct `text_format_source`". **That is infeasible under the
same SPEC's own "both drivers reused with ZERO change" constraint (its R-0.7).**

**Resolution: one sink per fixture, hence two fixtures** — `0084` witnesses D1
through the seam, `0085` witnesses D2. This mirrors the existing house precedent
of a sibling PAIR splitting the two polarities of one rule
(`0081-accesslog-metadata-filter` / `0082-accesslog-metadata-filter-key-not-found`).

Changing the driver to support N sinks was considered and rejected: it would widen
a correctness phase into harness work, and the sibling-pair pattern already
exists and costs one extra ~12-line entrypoint.

---

## §4. Scope

### 4.1 In scope

1. **NEW differential fixture `0084`** — the D1 witness through the seam. See §5.
2. **NEW differential fixture `0085`** — the D2 witness through the seam. See §5.
3. **Two ~12-line test entrypoints** under `tests/differential/tests/`, per the
   §5.4 stencil.
4. **`BEHAVIOR_CONTRACT.md`: a new `present_match`-polarity subsection.**
   `present_match: X` matches iff `(header present) == X`, then `^ invert_match`.
   Carries the §2.3 s2/s3/s4/s9 rows and the route-path p07/p08/p11/p12 rows as
   the measurement, and explicitly cross-references Trap A (§7) so no future
   reader conflates it with `ValueMatcher.present_match`. Placement: a new
   `### Phase 75 …` block at **~line 2632**, immediately after the phase-74 block
   (which ends at `:2631`) and before `## xDS wire state machine` (`:2633`) —
   the file's convention is one `### Phase NN (ADR-…): <title>` per phase in
   ascending order, each closed by a `---`.
5. **`BEHAVIOR_CONTRACT.md`: the CF-72-2 row updates.** The existing record is
   `**§D Name-only + treat_missing_header_as_empty (PV-5 …)**` at
   `BEHAVIOR_CONTRACT.md:2379-2383`. Extend it with the two facts measured at the
   phase-75 pick and re-confirmed at state-2:
   - the **top-level `contains_match` arm** is a THIRD member — upstream accepts
     it (with a deprecation warning), envoy-rust rejects it as an unknown field
     (it is reachable in-tree only as `string_match: { contains: … }`, by design,
     `bootstrap.rs:2976-2979`);
   - `treat_missing_header_as_empty: true` is not merely ACCEPTED upstream but
     **HONORED** — §2.3 sink s6 proves it (absent header → treated as `""` →
     no match against `exact: v` → only `/valmatch` logged), and with
     `invert_match` it flips D1's absent cell to KEEP.
6. **`BEHAVIOR_CONTRACT.md`: a new row for CF-75-1.** `exact_match: ""`
   degenerates to a PRESENCE match upstream (MEASURED: absent → no match,
   `x-a: v` → **match**, empty value → match) while envoy-rust performs a literal
   empty-value exact match (MEASURED: absent → no, `x-a: v` → **no**, empty
   value → match). Note `string_match: { exact: "" }` does **NOT** degenerate,
   and PGV separately rejects `string_match: { prefix: "" }` (`value length must
   be at least 1 characters`) — the degeneracy is specific to the deprecated
   top-level scalar arm.
7. **M74-31 — fix the five-site non-sequitur rather than mint a sixth and
   seventh.** The parent SPEC §10 asked state-2 to weigh this, and the answer is
   YES because this sub-phase writes two more kept-LAST access-log fixtures and
   would otherwise propagate the same wrong causal claim. The claim "placed
   SECOND **so** the last probe is KEPT" is a non-sequitur: `suppression_settle`
   inspects ONLY `probes.last()`, so with `[drop, keep, keep]` the last probe is
   kept whichever kept probe sits last. Every OUTCOME asserted at those sites is
   true; only the causal "so" is wrong. Sites:
   `tests/differential/tests/access_log_metadata_filter.rs:30-31`,
   `BEHAVIOR_CONTRACT.md:2612-2614`,
   `tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml:36-38`, and
   that fixture's `README.md:104-108`.

### 4.2 Out of scope

- **Any engine change.** 75.1 owns `crates/envoy-config/src/matcher.rs`. This
  sub-phase touches no `crates/` behavior. (Doc-comment corrections in `crates/`
  are 75.1's too.)
- **The §C rewrite** (`BEHAVIOR_CONTRACT.md:2357-2377`) and the
  `BEHAVIOR_CONTRACT.md:1878-1880` correction — both 75.1's.
- **Implementing CF-72-2's three members.** They need NEW config surface (a new
  `HeaderMatcher` field and a name-only default mode) and — decisively — cannot
  appear in a differential fixture until implemented, because the fixture would
  not boot on the subject side. This sub-phase only BANKS the measurement.
- **Implementing CF-75-1.** Banked as a contract row; the fix encodes a
  surprising proto3 degeneracy and should be decided alongside CF-72-2 by a
  future `HeaderMatcher` wire-shape-parity phase.
- **`MetadataMatcher.invert` (CF-74-1).** MEASURED accepted-but-INERT upstream on
  the access-log path; it stays boot-fatal here and must NOT be "implemented" —
  doing so would CREATE a divergence. See Trap B (§7).
- **A new fuzz target, corpus seed, or `ci.yml` step.** See §8.
- **Editing any landed ADR** (append-only, D-3.5) or the parent
  `75-headermatcher-absence-parity/SPEC.md` (a frozen artifact).

---

## §5. The two fixtures

### 5.1 `0084-headermatcher-absence-accesslog` — the D1 witness

`tests/fixtures/0084-headermatcher-absence-accesslog/`. One H1 HCM listener,
`clusters: []`, ONE `direct_response` route (`/x` → 200), ONE `FileAccessLog`
sink whose filter is:

```yaml
                    filter:
                      header_filter:
                        header:
                          name: x-a
                          exact_match: "v"
                          invert_match: true
```

Three probes, derived from §2.3 sink s1 and ordered **kept-LAST**:

| # | request | post-fix verdict | `expect_logged` |
|---|---|---|---|
| 1 | `GET /x`, no `x-a` | **DROPPED** — the D1 cell. Pre-fix envoy-rust KEPT it; upstream always dropped it | `false` |
| 2 | `GET /x`, `x-a: v` | DROPPED — value matches, invert flips to drop | `false` |
| 3 | `GET /x`, `x-a: zzz` | **KEPT** — value does not match, invert flips to keep | `true` (LAST) |

`expected_logged_count` = **1**. Each side's file holds exactly ONE byte-identical
line. The LAST probe is KEPT, so `suppression_settle` charges the cheap 2 s
`CF70_3_SETTLE` (see §5.3).

> Probe 1 is the load-bearing one: it is the cell the 75.1 fix changes. A
> pre-75.1 tree would log TWO lines here and this fixture would be RED — which is
> exactly why 75.2 depends on 75.1.

### 5.2 `0085-headermatcher-absence-accesslog-present-polarity` — the D2 witness

Same shape, with the sink filter:

```yaml
                    filter:
                      header_filter:
                        header:
                          name: x-a
                          present_match: false
```

Two probes, derived from §2.3 sink s2 and ordered kept-LAST:

| # | request | post-fix verdict | `expect_logged` |
|---|---|---|---|
| 1 | `GET /x`, `x-a: v` | **DROPPED** — the D2 cell: header PRESENT, `(present == want) = (true == false) = false`. Pre-fix envoy-rust KEPT it | `false` |
| 2 | `GET /x`, no `x-a` | **KEPT** — `(false == false) = true` | `true` (LAST) |

`expected_logged_count` = **1**. Note this fixture uses a **plain, NON-inverted,
single-line** matcher — the simplest possible spelling — which is what makes D2
worse than D1.

Consider also folding in the P1 guard (`present_match: true` + `invert_match`,
§2.3 sink s4) as a third fixture or as the PLAN's judgement call; the guard is
already pinned in-process by 75.1 and on the route path by `0083`, so a third
access-log fixture is optional polish, not a §6.3 requirement. Decide it at this
sub-phase's state-2 PLAN-write against the then-re-derived size.

### 5.3 `expectations.yaml` schema (re-verified at state-2; `deny_unknown_fields` throughout)

`Driver::Http1AccessLogByteExact` is declared at
`tests/differential/src/lib.rs:159-165` and selected by
`kind: http1_access_log_byte_exact`. Its probe type
(`AccessLogByteExactProbe`, `lib.rs:1102-1128`):

| YAML key | type | required | default |
|---|---|---|---|
| `method` | `get` \| `options` \| `post` | **yes** | — |
| `path` | `String` | **yes** | — |
| `host` | `String` | **yes** | — |
| `extra_headers` | list of `[name, value]` pairs | no | `[]` |
| `body` | `String` | no | `null` |
| `expected_status` | `u16` | no | **`200`** |
| `expect_logged` | `bool` | no | **`true`** |

**There is NO per-probe `name` field on this driver** (unlike the probe-list
driver) — failures identify probes by 0-based index. **And there is NO expected
log-line field at all.** The assertion is (a) each side's file has exactly
`expected_logged_count(probes)` lines (`lib.rs:6415-6430`) and (b)
`assert_access_log_lines_byte_identical(&envoy_lines, &envoy_rust_lines)`
(`lib.rs:6432`) — PURE cross-proxy whole-line equality. House style states the
expected line in YAML **comments** and in the README, not as a field.

Paths block (`AccessLogPaths`, `lib.rs:1088-1093`) — exactly:

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0084-envoy-mount/access.log
    envoy_rust: /tmp/0084-envoy-rust-mount/access.log
  probes: [...]
```

The harness creates and `chmod 0o777`s both parent dirs and deletes leftover log
files itself (`lib.rs:3991-4014`), so they need not pre-exist.

**Sending vs omitting `x-a`.** `drive_http1` (`lib.rs:2182-2211`) emits
`extra_headers` VERBATIM and in order after `Host:`, injecting only `Host`, an
optional `Content-Length`, and `Connection: close`. So `extra_headers: [["x-a",
"v"]]` sends it and **omitting the key entirely** makes it genuinely absent on the
wire.

**The ordering cost.** `suppression_settle` (`lib.rs:1691-1699`) inspects ONLY
`probes.last()` and charges `CF71_1_SETTLE` = **12 s** (`:1689`) when the last
probe is DROPPED, versus `CF70_3_SETTLE` = **2 s** (`:1683`) otherwise. It is
paid at most once per fixture and only when at least one probe has
`expect_logged: false` (`has_suppression`, `:6296`). Both fixtures here are
kept-LAST, so both pay 2 s. **Do not restate the M74-31 non-sequitur** while
documenting this (§4.1 item 7): the correct claim is "the last probe is KEPT,
therefore the cheap settle applies", not "probe X is placed SECOND *so* the last
probe is kept".

Also automatic: a per-probe status assert against `expected_status` on both sides
(`:6325-6338`), and `wait_file_lines(..., ACCESS_LOG_FLUSH_WAIT = 15 s, :1675)`
on each file BEFORE teardown — Envoy's FileAccessLog flushes on a ~10 s timer
rather than per record, so a post-stop-only read would see only the first line.

### 5.4 Registration cost — ONE file each (PV-7, re-confirmed)

`tests/differential/Cargo.toml` has **no `[[test]]` stanza** (cargo autodiscovers
`tests/*.rs`); the workspace root `Cargo.toml:19` already lists
`tests/differential`; `.github/workflows/ci.yml:67` is `cargo test --workspace`;
there is no fixture registry — `run_fixture(&dir)` takes the directory path. So
each fixture costs one entrypoint:

```rust
use std::path::PathBuf;

#[tokio::test]
async fn headermatcher_absence_accesslog() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0084-headermatcher-absence-accesslog");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

House style prefixes this with a long `//!` header: opening line ("Docker-gated
differential test for fixture NNNN-name."), phase + ADR refs, the config shape,
a probe-by-probe enumeration with kept/dropped and the settle rationale, the exact
expected log line, and the `clusters: []` / no-backend note.

### 5.5 Per-side config divergences (the house recipe, re-verified)

Write one config, then on the `envoy-rust.yaml` copy: (a) DROP the `admin:` block,
(b) change the listener bind `0.0.0.0` → `127.0.0.1`, (c) drop
`generate_request_id: false` if present, (d) point the access-log `path:` at the
`-envoy-rust-mount` dir. Keep `node:`, `codec_type: HTTP1`, the filters, the log
format and the route table byte-identical.

**Format operators.** `%REQ(:PATH)%` and `%RESPONSE_CODE%` are safe. Note
`%REQ(NAME)%` is ALLOW-LIST gated — the list is `:method`, `:authority`, `:path`,
`x-envoy-original-path`, `x-forwarded-for`, `user-agent`, `x-request-id`, so a
`%REQ(X-A)%` is **BOOT-FATAL** in envoy-rust (ADR-0153 PV-6). These fixtures must
NOT try to echo the gating `x-a` header into the log line. (`%DYNAMIC_METADATA(ns:key)%`
is a separate operator and is NOT allow-list gated, but is not needed here.)

---

## §6. Differential surface at sub-phase end

- **NEW fixture `0084-headermatcher-absence-accesslog`** — green cross-proxy;
  the D1 witness through the ADR-0150 `HeaderMatch` seam.
- **NEW fixture `0085-headermatcher-absence-accesslog-present-polarity`** — green
  cross-proxy; the D2 witness, on a plain non-inverted matcher.
- **All pre-existing fixtures stay green** — 83 by the time this sub-phase runs
  (82 plus 75.1's `0083`). Watch in particular `0078`/`0079`/`0080`/`0081`/`0082`
  (the access-log filter family) and `0083`.
- **Conformance:** unchanged. h2spec stays at its declared threshold;
  `known-failures.txt` stays **21** lines and is NEVER trimmed (this host scores
  h2spec 3.5/2 as PASS, so trimming on local evidence would break CI).

---

## §7. Risks and traps

- **This sub-phase is RED before 75.1 lands, by design.** Both fixtures assert
  the POST-fix behavior. Confirm 75.1 is `done` before starting.
- **A stale `target/debug/envoy-bin` mis-reports the differential.** The harness
  runs the DEBUG binary; run `cargo build -p envoy-bin` before ANY local
  differential, or a fixture reds on stale code rather than on its subject.
- **Access-log flush is timer-driven.** Do not read a log file after a hard stop
  and conclude the lines are missing; the harness's pre-teardown
  `wait_file_lines` exists for this. In hand-rolled probes use a graceful
  `docker stop -t 15` and `docker cp` (NOT `--volumes-from`, which cannot reach a
  stopped container's `/tmp`).
- **Upstream Envoy will not create a log directory** — a `path:` under a
  nonexistent dir is boot-fatal (`unable to open file … No such file or
  directory`). The harness creates both parent dirs itself; this bites only
  hand-rolled probes.
- **Docker bind mounts are STALE-CACHED on this host.** After editing a config in
  a bind-mounted directory the container keeps reading the PREVIOUS contents. Use
  a FRESH FILENAME for every config revision.
- **YAML 1.1 booleans.** An unquoted `cluster: y` in `node:` parses as boolean
  `true`; upstream's JSON-proto path then rejects the bootstrap with
  `@ node.cluster: string, … unexpected character 't'`. Quote scalar node fields
  in hand-rolled probe configs.
- **The ADR-0150 seam must keep holding.** `envoy-accesslog` has ZERO workspace
  deps (`tokio`, `bytes`, `tracing`, `thiserror`) and MUST NOT depend on
  `envoy-config`; matchers cross as trait objects (`Arc<dyn HeaderMatch>`,
  `Arc<dyn MetadataMatch>`); `LogFilter` has NO `Eq`/`PartialEq` — do not add
  either.
- **Do NOT add `on_header_missing` to fixtures `0081` or `0082`** while touching
  them for M74-31. envoy-rust requires a `value` on that block, which would make
  the key RESOLVE and silently vacate the witness while the fixture stayed GREEN
  (ADR-0155 PV-6). The `on_header_missing` occurrences in those YAMLs are `#`
  COMMENTS documenting the deliberate omission — do not "clean them up".
  `0081` has THREE probes / TWO expected lines, byte-distinct so ORDER is pinned.
- **Never weaken a fixture; never trim `known-failures.txt`.**

**TWO CONFLATION TRAPS — do NOT unify them.**

- **Trap A — two different `present_match` fields.**
  `HeaderMatcher.present_match` (this sub-phase's subject) and
  `ValueMatcher.present_match` (RBAC / access-log **metadata**) are different
  messages with different MEASURED rules. `crates/envoy-config/src/bootstrap.rs:1704`
  and `BEHAVIOR_CONTRACT.md:1863-1885` record that for the `ValueMatcher` one
  **`present_match: false` NEVER matches** — a DIFFERENT and CORRECT rule. After
  the 75.1 fix the two rules AGREE for the present case and still DIFFER for the
  absent case (`ValueMatcher` → `false`; `HeaderMatcher` → `true`). The new
  polarity subsection (§4.1 item 4) must cross-reference this explicitly.
- **Trap B — two different `invert` fields.** `HeaderMatcher.invert_match` and
  `MetadataMatcher.invert` (CF-74-1) are unrelated fields on unrelated messages.
  The latter is MEASURED accepted-but-INERT upstream on the access-log path and
  stays boot-fatal here.

---

## §8. §7.4 fuzz disposition — CONFIRMED, not inherited

**No new fuzz target, no new corpus seed, no `ci.yml` step.** This sub-phase adds
no parser, codec, filter or config surface — it adds fixtures and documentation.
The existing `parse_bootstrap` target already covers the unchanged `HeaderMatcher`
deserializer, and it is **parse-only**: it never calls `HeaderMatcher::matches`,
so no seed can encode runtime semantics. Re-confirm rather than inherit at this
sub-phase's state-2.

(Both omissions are otherwise easy to miss: a new target is not auto-discovered
and needs a hand-written `ci.yml` step, and a new seed needs an explicit
`!`-un-ignore line in the fuzz `.gitignore` or it is silently untracked and
invisible to CI — verify with `git ls-files`. There are **63** tracked
`parse_bootstrap` seeds.)

---

## §9. Estimated size (§6.1 gate for THIS sub-phase)

| Area | Net LoC |
|---|---|
| Fixture `0084`: 2 configs (~50 each) + `expectations.yaml` (~50) + README (~110) | ~260 |
| Fixture `0085`: same shape | ~260 |
| 2 test entrypoints incl. the house `//!` headers | ~70 |
| `BEHAVIOR_CONTRACT.md`: the `present_match`-polarity subsection with both measured tables | ~70 |
| `BEHAVIOR_CONTRACT.md`: CF-72-2 row updates + the CF-75-1 row | ~50 |
| M74-31 five-site correction | ~15 |
| **Total** | **~725 net LoC / ~6-8 tasks** |

Comfortably under the ~1500 LoC / ~25 task gate. Basis: MEASURED comparables on
disk — `0082-accesslog-metadata-filter-key-not-found` is 264 lines across its four
files and is the closest analogue (one sink, two probes, one kept line).

---

## §10. ADR pointers

- **ADR-0156** — the phase-75 pick (state-0/1): the measured basis (D1 + D2 + P1)
  and the scope line.
- **ADR-0157** — the §6.1 SPLIT of phase 75 into 75.1 + 75.2.
- **ADR-0158** — the §6.2 empirical reconciliation, including the single-log-file
  driver constraint of §3 (which is why this sub-phase has two fixtures) and the
  both-sides / `/empty` / s9 extension of the §2.3 table.

---

## §11. Carry-forwards

**CONSUMED by this sub-phase (if it lands as scoped):** **M74-31** — the five-site
kept-LAST non-sequitur, corrected in place rather than propagated.

**BANKED (recorded in `BEHAVIOR_CONTRACT.md`, not fixed):** **CF-75-1**
(`exact_match: ""` presence degeneracy) and the extended **CF-72-2** (name-only
`{ name }`, `treat_missing_header_as_empty` accepted AND honored, and the
top-level `contains_match` arm). Owner for both = a future `HeaderMatcher`
wire-shape-parity phase, which should decide them together rather than
piecemeal.

**Closed earlier in the parent phase:** **CF-72-1**, by 75.1's engine fix.

**Untouched, carried forward:** CF-74-1/2/3/4/6, CF-73-1, N73-R2, M73-R1/M73-R2,
M71-3, M71-6/7/8, M70-R4/R9, M69-A..I, CF-69-1/2/3/5, M68-1, M-1, CF-67-3/5/6/7,
M74-3..M74-14, M74-16, M74-17/18/20/21/22/26/27/28/29, M74-30..M74-39 (minus
M74-31), the older Minors in `67.3/SPEC.md` §10, and the HTTP-filters-family
(1)-(4) in `STATE_HISTORY.md`.

**Worth weighing at this sub-phase's state-2:** **M71-6** — the standalone H2
access-log-filter differential. It would reuse `Driver::Http2AccessLogByteExact`
unchanged, and this sub-phase is already building access-log fixtures, so an H2
sibling is the cheapest it will ever be. It lights up no NEW rule (H2 delegates
route resolution to `envoy_http1::hcm::resolve_route`, and the access-log seam is
shared), so it is polish — fold it only if the re-derived size leaves room.
