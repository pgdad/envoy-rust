# Sub-phase 75.2 — `HeaderMatcher` absence semantics: the ACCESS-LOG-path differential witnesses + the contract bank — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Witness the already-landed sub-phase-75.1 `HeaderMatcher` absence fix CROSS-PROXY on the access-log path — the second consumer of the shared matching engine, reached through the ADR-0150 `HeaderMatch` trait seam — with two new backend-free byte-exact differential fixtures (`0084`, `0085`), and bank the measured `present_match`-polarity rule plus the reject-direction carry-forwards into `BEHAVIOR_CONTRACT.md`.

**Architecture:** This sub-phase changes **NO** Rust behavior code. It adds two fixture directories (four YAML/Markdown files each), two ~45-line `#[tokio::test]` entrypoints, one new `### Phase 75` block in `BEHAVIOR_CONTRACT.md`, two contract-record amendments, and eight small live-document corrections carried over from the sub-phase-75.1 code review. The differential driver (`Driver::Http1AccessLogByteExact`) is reused with **ZERO** change.

**Tech Stack:** Rust 2024 (pinned by `rust-toolchain.toml`), `tokio` test harness, the in-tree `differential` test crate, `testcontainers` + Docker against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, the `ENVOY_TARGET.md` pin), YAML fixtures.

---

## Global Constraints

Every task's requirements implicitly include this section.

- **The 75.1 engine is LANDED and is the PREMISE of this sub-phase. Do NOT re-derive, revert, "simplify" or re-verify it.** `HeaderMatcher::matches` (`crates/envoy-config/src/matcher.rs:39-70`) is an exhaustive tuple `match (&self.mode, value)` whose `(_, None) => return false` arm sits **AFTER** the `PresentMatch` arm and **BEFORE** every value arm. Hoisting `(_, None)` to the top is the naive uniform absent-DROP and was MEASURED to turn FOUR tests RED.
- **This sub-phase touches NO `crates/` behavior code.** The only permitted `crates/` edits are the two doc-comment corrections named in Task 8 (`bootstrap.rs`, `matcher.rs` — both comment-only, zero executable lines).
- **`cargo build -p envoy-bin` before ANY local differential run.** The harness runs `target/debug/envoy-bin`, not release; a stale binary mis-reports every fixture.
- **Never weaken a fixture. Never trim `tests/conformance/h2spec/known-failures.txt`** (21 lines; this host scores h2spec 3.5/2 as PASS, so trimming on local evidence would break CI).
- **Append-only files — never edit a landed entry:** `docs/envoy-rust/DECISIONS.md`, `docs/envoy-rust/ROADMAP.md`, `docs/envoy-rust/STATE_HISTORY.md`, and every artifact under `docs/envoy-rust/phases/` for phases `00`–`75.1` (including the FROZEN parent `75-headermatcher-absence-parity/SPEC.md` and every `75.1-headermatcher-absence-engine-route/` file). D-3.5.
- **`%REQ(NAME)%` is ALLOW-LIST gated in envoy-rust.** `REQ_ALLOW_LIST` (`crates/envoy-accesslog/src/command_operator.rs:89-100`) is exactly `:method`, `:authority`, `:path`, `x-envoy-original-path`, `x-forwarded-for`, `user-agent`, `x-request-id`. A `%REQ(X-A)%` operator is **BOOT-FATAL** (`ConfigError::InvalidAccessLogFormat`). **Neither new fixture may echo the gating `x-a` header into its log line.** Both use `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n`, exactly as `0078`/`0079`/`0080` do.
- **`codec_type: HTTP1` goes on BOTH sides.** envoy-rust has no serde default for it under `deny_unknown_fields`; upstream would default to `AUTO`. It is NOT a per-side divergence (ADR-0158 C3).
- **The ADR-0150 seam must keep holding.** `envoy-accesslog` has ZERO workspace deps and MUST NOT depend on `envoy-config`. Matchers cross as trait objects (`Arc<dyn HeaderMatch>`, `Arc<dyn MetadataMatch>`). `LogFilter` has NO `Eq`/`PartialEq` — do not add either.
- **Do NOT add `on_header_missing` to fixtures `0081` or `0082`** while editing them for Task 7. envoy-rust requires a `value` on that block, which would make the key RESOLVE and silently vacate the witness while the fixture stayed GREEN (ADR-0155 PV-6). Their `on_header_missing` occurrences are `#` COMMENTS documenting the deliberate omission — do not "clean them up".
- **NO new fuzz target, NO new corpus seed, NO `ci.yml` edit.** RE-CONFIRMED at this PLAN-write (see §"§7.4 fuzz disposition" below), not inherited.
- **`.claude/worktrees/agent-*` belongs to a parallel workstream.** LEAVE IT ALONE and EXCLUDE it from every repo-wide `find`/`grep`/census — it duplicates the whole tree and inflates counts.
- **Docker bind mounts are STALE-CACHED on this host.** For any hand-rolled probe, use a FRESH FILENAME per config revision; never edit in place and re-run. `--volumes-from` cannot reach a stopped container's `/tmp` — use `docker cp`.
- **Commit after every task.** Commit message prefix: `phase 75.2: <what>`.

---

## §6.2 Empirical Reconciliation — RE-MEASURED at this PLAN-write

`BOOTSTRAP_PROMPT.md` §6.2 requires a state-2 PLAN-write to RE-CONFIRM its `SPEC.md`'s citations against the LIVE tree rather than inherit them. All measurements below were taken on `HEAD == 3f0ec8905fe25d7a0dcd05bc0b027c208d82928a`, working tree clean, `origin/main` at the same SHA. **This fired six corrections.** They are recorded in **ADR-0161** and are already applied throughout this plan.

### Corrections to `SPEC.md`

| # | `SPEC.md` claim | LIVE truth | Verdict |
|---|---|---|---|
| **C1** | §4.1 item 4: insert the new `### Phase 75` block at **~line 2632**, after the phase-74 block ending at `:2631` and before `## xDS wire state machine` at `:2633` | The phase-74 block opens at **2493**, its body ends at **2673**, its closing `---` is at **2675**, and `## xDS wire state machine` is at **2677**. Insertion point is **immediately before line 2677**. File total is **3363** lines. | **DRIFTED +44** — sub-phase 75.1 landed ~44 lines into this file. |
| **C2** | §4.1 item 5: the CF-72-2 record `**§D Name-only + treat_missing_header_as_empty (PV-5 …)**` is at `:2379-2383` | It is at **`:2423-2427`**, and its heading reads `**§D Name-only + treat_missing_header_as_empty (PV-5, MEASURED — inherited boundary).**` | **DRIFTED +44**; heading text also differs from the SPEC's paraphrase. |
| **C3** | §4.1 item 7: the M74-31 `BEHAVIOR_CONTRACT.md` site is at `:2612-2614` | It is at **`:2657`** (`"is placed SECOND, not last, so kept-LAST (ADR-0147) holds"`). | **DRIFTED +45.** |
| **C4** | §4.1 item 7: M74-31 is a **five-site** non-sequitur, then enumerates **four** paths | A repo-wide sweep for the CAUSAL `"placed SECOND … **so** …"` claim (excluding `.claude/worktrees/` and all append-only `phases/`/`DECISIONS.md`/`STATE*` history) finds **exactly FOUR** live sites — the same four the SPEC enumerates. The "FIVE" figure originates at `docs/envoy-rust/phases/74-accesslog-metadata-filter/REVIEW.md:1269`, which asserts "now at FIVE sites" and then lists four. `0081/expectations.yaml:20` (*"Probe 2 — KEPT, and placed SECOND (phase 74 §5.2 state-3 re-entry, `REVIEW.md` I-3)"*) is DESCRIPTIVE, not causal, and is **not** a site. | **REFUTED — it is a FOUR-site problem.** Task 7 fixes four sites and says four. The landed "FIVE" figure in `74/REVIEW.md` is an append-only historical artifact and must **NOT** be edited (D-3.5). |
| **C5** | §9: ~725 net LoC / ~6-8 tasks | Re-derived on fresh measured comparables: **~760 net LoC / 8 tasks** (table below). | **CONFIRMED (no split).** |
| **C6** | §5.2: consider a THIRD fixture for the P1 guard; §11: consider folding M71-6 (the H2 access-log-filter differential) | Both DECLINED — see "Judgement calls" below. | **RESOLVED.** |

### Citations RE-CONFIRMED as still correct (no drift)

- **Driver constraint (the reason this sub-phase has TWO fixtures, ADR-0158) — HOLDS.** `AccessLogPaths` (`tests/differential/src/lib.rs:1088-1093`) is exactly `{ envoy: String, envoy_rust: String }` under `#[serde(deny_unknown_fields)]`. Only the ENVOY-side parent dir is bind-mounted — `vec![(envoy_parent_s.clone(), envoy_parent_s)]` at **`lib.rs:4019`**, verbatim as cited. Corpus census RE-RUN: the MAXIMUM number of `name: envoy.access_loggers.file` sinks in any single fixture YAML is **1**, across all 83 fixtures. **One sink per fixture, hence two fixtures.**
- **`AccessLogByteExactProbe` (`lib.rs:1102-1128`)** — field list confirmed EXACTLY as the SPEC states: `method` (`Http1Method`, required), `path` (`String`, required), `host` (`String`, required), `extra_headers` (`Vec<(String,String)>`, `#[serde(default)]`), `body` (`Option<String>`, `#[serde(default)]`), `expected_status` (`u16`, `default = "default_byte_exact_status"` → **200**), `expect_logged` (`bool`, `default = "default_expect_logged"` → **true**). `#[serde(deny_unknown_fields)]`. **There is NO `name` field** — failures identify probes by 0-based index.
- **`Driver::Http1AccessLogByteExact`** declared at `lib.rs:159-165`, selected by `kind: http1_access_log_byte_exact`.
- **`suppression_settle` (`lib.rs:1694-1699`)** inspects ONLY `probes.last()`; `CF71_1_SETTLE` = **12 s** (`lib.rs:1689`), `CF70_3_SETTLE` = **2 s** (`lib.rs:1683`). `has_suppression` = `expected_lines < probes.len()` (`lib.rs:6296`) — the settle is paid only when at least one probe is suppressed. **Both new fixtures are kept-LAST, so both pay 2 s.**
- **The assertion is pure cross-proxy equality.** Line-count asserts at `lib.rs:6414-6430`; `assert_access_log_lines_byte_identical(&envoy_lines, &envoy_rust_lines)` at `lib.rs:6432`. **There is NO expected-log-line-text field in the driver schema** — the expected line is stated in YAML `#` comments and the README, never as a field.
- **The harness creates and `chmod 0o777`s both parent dirs and deletes leftover log files itself** (`lib.rs:3986-4014`); they need not pre-exist. `ACCESS_LOG_FLUSH_WAIT` = 15 s (`lib.rs:1675`), with `wait_file_lines` polled on each file BEFORE teardown.
- **Registration cost is ONE file each.** `tests/differential/Cargo.toml` has **no `[[test]]` stanza** (cargo autodiscovers `tests/*.rs`); the workspace root already lists `tests/differential`; `.github/workflows/ci.yml` runs `cargo test --workspace`; there is no fixture registry — `run_fixture(&dir)` takes the directory path.
- **Next free fixture ids are `0084` and `0085`.** The corpus holds exactly **83** numbered directories, highest `0083`; neither `tests/fixtures/0084*` nor `tests/fixtures/0085*` exists.
- **`invert_match` appears in exactly ONE fixture's YAML corpus-wide** (`0083`, three files). `present_match` appears in two (`0044` — a `ValueMatcher` on RBAC metadata, a DIFFERENT message — and `0083`).
- **All EIGHT of the sub-phase-75.1 review's citations HOLD on the live tree.** `matcher.rs:52` at `BEHAVIOR_CONTRACT.md:2408` (M-1); `DIFFER when it is ABSENT` at `BEHAVIOR_CONTRACT.md:1884` mirrored at `crates/envoy-config/src/bootstrap.rs:1707` (M-2); `whose divergence is mode-scoped` at `BEHAVIOR_CONTRACT.md:2545` and `tests/fixtures/0081-accesslog-metadata-filter/README.md:100` (M-3); `flipped the two \`false ×\` expectations` at `crates/envoy-config/src/matcher.rs:349` (N-1); `See §C for the \`HeaderMatcher\` rule in full` at `BEHAVIOR_CONTRACT.md:1887` with exactly **EIGHT** `**§C ` headings in the file and the intended phase-72 §C at `:2364` (N-2). **The XOR is confirmed at `crates/envoy-config/src/matcher.rs:69`** (`mode_result ^ self.invert_match`).
- **The per-side divergence recipe is CONFIRMED 4/4** for the `0078`–`0082` access-log lineage that `0084`/`0085` follow: drop `admin:`, rewrite the listener bind `0.0.0.0` → `127.0.0.1`, drop `generate_request_id: false`, repoint the access-log `path:`. (`0083` diverges from this recipe — it ADDS a `node:` block to the envoy-rust side and puts `admin:` at EOF — but `0083` is a `http1_probe_list` fixture, not an access-log one. **Follow `0078`, not `0083`.**)

### Judgement calls made at this PLAN-write

- **The optional THIRD fixture for the P1 guard (`present_match: true` + `invert_match`, SPEC §5.2) — DECLINED.** The SPEC itself calls it "optional polish, not a §6.3 requirement". P1 is already pinned in-process by `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream` and `invert_match_inverts_present_match_result`, and CROSS-PROXY on the route path by fixture `0083`. A third container-gated fixture would buy redundant coverage at the cost of real CI wall-clock and a third startup-race flake surface. **The P1 guard is instead documented in the new `### Phase 75` contract block (Task 5 §D) and cited from both new READMEs.**
- **M71-6 (a standalone H2 access-log-filter differential) — DECLINED, carried forward.** The SPEC's own weighing says it "lights up no NEW rule" because H2 delegates route resolution to `envoy_http1::hcm::resolve_route` and the access-log seam is shared. Folding it would widen a correctness sub-phase into H2 driver coverage and add ~300 LoC for zero new semantics. **M71-6 stays on the carry-forward list.**
- **M-5 and N-3 (from the 75.1 review) need NO fix** — `PROGRESS.md` and commit messages are landed historical artifacts; editing them retroactively would be worse than the imprecision (D-3.5). **N-4** is a coverage note, record only. None of the three appears as a task.
- **M-1's fix is made LINE-NUMBER-FREE**, per the review's own suggestion. That citation class has gone stale three times (`:51` → `:52` → `:69`), twice inside the very phase chartered to fix it. Task 8 replaces the line number with a prose anchor rather than re-pointing it at `:69`.

### §6.1 size gate — RE-DERIVED on FRESH numbers

Measured comparables on disk at this PLAN-write (`wc -l`):

| Fixture (one file sink, backend-free, byte-exact) | 4 files | entrypoint | total |
|---|---|---|---|
| `0078-accesslog-header-filter` (2 probes, `header_filter`) | 211 | 39 | 250 |
| `0082-accesslog-metadata-filter-key-not-found` (2 probes) | 264 | 42 | 306 |
| `0081-accesslog-metadata-filter` (3 probes) | 352 | 50 | 402 |

Projection for this sub-phase:

| Task | Area | Net LoC |
|---|---|---|
| 1 | Fixture `0084`: `envoy.yaml` (~45) + `envoy-rust.yaml` (~43) + `expectations.yaml` (~52, 3 probes) + `README.md` (~120) | ~260 |
| 2 | `0084` test entrypoint incl. the house `//!` header | ~48 |
| 3 | Fixture `0085`: `envoy.yaml` (~43) + `envoy-rust.yaml` (~41) + `expectations.yaml` (~42, 2 probes) + `README.md` (~110) | ~236 |
| 4 | `0085` test entrypoint | ~45 |
| 5 | `BEHAVIOR_CONTRACT.md`: the new `### Phase 75` `present_match`-polarity block with both MEASURED matrices | ~95 |
| 6 | `BEHAVIOR_CONTRACT.md`: CF-72-2 §D extension + the new CF-75-1 row | ~45 |
| 7 | M74-31 — the FOUR-site causal correction | ~14 |
| 8 | Review findings M-1 / M-2 (×2 sites) / M-3 (×2 sites) / N-1 / N-2 | ~18 |
| | **Total** | **~760 net LoC / 8 tasks** |

**VERDICT: NO SPLIT.** ~760 net LoC against the ~1500 gate (49% margin) and 8 tasks against the ~25 gate. This is a far more comfortable margin than sub-phase 75.1's, which landed at **+1553 / −96 = 1457 net** and cleared the gate by only ~43 lines. The difference is structural, not optimistic: 75.1 was an engine change with a 7-mode × 3-presence × 2-polarity in-process matrix and five consumer-propagation test sites; 75.2 adds **zero** Rust behavior code and its two fixtures are near-clones of an existing 211-line comparable.

### §7.4 fuzz disposition — RE-CONFIRMED, not inherited

**No new fuzz target, no new corpus seed, no `ci.yml` step.** This sub-phase adds no parser, codec, filter or config surface — only fixtures and documentation. The existing `parse_bootstrap` target already covers the unchanged `HeaderMatcher` deserializer, and it is **parse-only**: it never calls `HeaderMatcher::matches`, so no seed can encode runtime semantics. Census re-run at this PLAN-write: **63** tracked `parse_bootstrap` corpus seeds, **5** fuzz targets — both unchanged and both to stay unchanged.

---

## File Structure

**Created (10 files):**

| Path | Responsibility |
|---|---|
| `tests/fixtures/0084-headermatcher-absence-accesslog/envoy.yaml` | Upstream-Envoy side of the D1 witness |
| `tests/fixtures/0084-headermatcher-absence-accesslog/envoy-rust.yaml` | envoy-rust side, differing only by the four recipe deltas |
| `tests/fixtures/0084-headermatcher-absence-accesslog/expectations.yaml` | Driver selection + the 3 probes |
| `tests/fixtures/0084-headermatcher-absence-accesslog/README.md` | What it proves, the keep/drop table, per-side divergences, cross-refs |
| `tests/differential/tests/headermatcher_absence_accesslog.rs` | The `#[tokio::test]` entrypoint for `0084` |
| `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/envoy.yaml` | Upstream side of the D2 witness |
| `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/envoy-rust.yaml` | envoy-rust side |
| `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/expectations.yaml` | Driver selection + the 2 probes |
| `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/README.md` | Ditto for D2 |
| `tests/differential/tests/headermatcher_absence_accesslog_present_polarity.rs` | The `#[tokio::test]` entrypoint for `0085` |

**Modified (6 files):**

| Path | Change |
|---|---|
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | New `### Phase 75` block (Task 5); §D CF-72-2 extension + CF-75-1 row (Task 6); M74-31 site (Task 7); M-1/M-2/M-3/N-2 sites (Task 8) |
| `tests/differential/tests/access_log_metadata_filter.rs` | M74-31 site (Task 7) — comment only |
| `tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml` | M74-31 site (Task 7) — comment only |
| `tests/fixtures/0081-accesslog-metadata-filter/README.md` | M74-31 site (Task 7) + M-3 site (Task 8) |
| `crates/envoy-config/src/bootstrap.rs` | M-2 mirror site (Task 8) — **doc comment only, zero executable lines** |
| `crates/envoy-config/src/matcher.rs` | N-1 site (Task 8) — **test-module comment only, zero executable lines** |

**Deliberately NOT modified:** any `crates/` executable code; `tests/differential/src/lib.rs`; `tests/differential/Cargo.toml`; the workspace root `Cargo.toml`; `.github/workflows/ci.yml`; `tests/conformance/h2spec/known-failures.txt`; any fuzz target or corpus; fixture `0082`'s files; `crates/envoy-config/src/matcher.rs:471` (a PAST-TENSE historical `matcher.rs:52` citation the 75.1 review explicitly adjudicated as **correct** and **not** a finding).

---

## The rule this sub-phase witnesses (for a reader with zero context)

Sub-phase 75.1 landed this MEASURED rule into `crates/envoy-config/src/matcher.rs`:

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

Two divergences it closed, both of which this sub-phase now witnesses cross-proxy on the ACCESS-LOG path:

- **D1** (= the former carry-forward CF-72-1, now CLOSED): a VALUE matcher (`exact_match` / `prefix_match` / `suffix_match` / `safe_regex_match` / `range_match` / `string_match`) + `invert_match: true` + an ABSENT header — upstream DROPS, the pre-75.1 in-tree engine KEPT. **Fixture `0084` probe 1.**
- **D2**: upstream `present_match: false` means **"the header must be ABSENT"**; the pre-75.1 in-tree engine treated it as unconditionally true. Fires on a plain, NON-inverted, single-line matcher. **Fixture `0085` probe 1.**
- **P1 — the guard that must survive**: `present_match: true` + `invert_match` + absent is FULL PARITY (both KEEP). Not fixtured here (see "Judgement calls"), but documented in Task 5 §D.

The access-log path reaches the same engine through the ADR-0150 trait seam: `crates/envoy-accesslog/src/filter.rs` (`LogFilter::Header { matcher } => matcher.matches(headers)`) → `impl envoy_accesslog::HeaderMatch for HeaderMatcher` at `crates/envoy-config/src/matcher.rs:82`, whose trait object is injected in `compile_access_log_filter` (`crates/envoy-http1/src/hcm.rs`).

---

### Task 1: Fixture `0084-headermatcher-absence-accesslog` — the D1 witness

**Files:**
- Create: `tests/fixtures/0084-headermatcher-absence-accesslog/envoy.yaml`
- Create: `tests/fixtures/0084-headermatcher-absence-accesslog/envoy-rust.yaml`
- Create: `tests/fixtures/0084-headermatcher-absence-accesslog/expectations.yaml`
- Create: `tests/fixtures/0084-headermatcher-absence-accesslog/README.md`

**Interfaces:**
- Consumes: nothing from earlier tasks. Consumes the EXISTING driver `Driver::Http1AccessLogByteExact` (`tests/differential/src/lib.rs:159-165`, selected by `kind: http1_access_log_byte_exact`) with ZERO change.
- Produces: the fixture directory path `tests/fixtures/0084-headermatcher-absence-accesslog`, which **Task 2** passes to `differential::run_fixture(&dir)`. The mount dirs `/tmp/0084-envoy-mount/` and `/tmp/0084-envoy-rust-mount/` are created by the harness itself.

**Context.** This is a near-clone of `tests/fixtures/0078-accesslog-header-filter/` (an H1 HCM listener, one `FileAccessLog` sink gated by a `header_filter`, one `direct_response` route, `clusters: []`, no backend). The only substantive change is the matcher body and a third probe. `0078` is 211 lines across four files and is the exact structural stencil.

**The matcher and why each probe lands where it does.** The filter is `exact_match: "v"` + `invert_match: true` on header `x-a`:

| # | request | engine evaluation | verdict | `expect_logged` |
|---|---|---|---|---|
| 1 | `GET /x`, **no** `x-a` | `(ExactMatch, None)` hits the `(_, None) => return false` arm → `false`, `invert_match` NOT applied | **DROPPED** — the D1 cell | `false` |
| 2 | `GET /x`, `x-a: v` | `"v" == "v"` → `true`; `true ^ true` → `false` | DROPPED | `false` |
| 3 | `GET /x`, `x-a: zzz` | `"zzz" == "v"` → `false`; `false ^ true` → `true` | **KEPT** (LAST) | `true` |

`expected_logged_count` = **1**. Probe 1 is the load-bearing one: a pre-75.1 tree logs it too, giving TWO lines on the envoy-rust side against upstream's one, and the fixture goes RED. The LAST probe is KEPT, so `suppression_settle` charges the cheap 2 s `CF70_3_SETTLE`.

- [ ] **Step 1: Create `tests/fixtures/0084-headermatcher-absence-accesslog/envoy.yaml`**

```yaml
node: { id: envoy-rust-phase-75-fixture-0084, cluster: envoy-rust-phase-75 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                generate_request_id: false
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0084-envoy-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n"
                    # Sub-phase 75.2 (ADR-0156/0157/0158/0161) — the D1 witness on
                    # the ACCESS-LOG path, through the ADR-0150 `HeaderMatch` trait
                    # seam. A VALUE matcher (`exact_match`) plus `invert_match` with
                    # the header ABSENT must DROP: upstream treats a missing header
                    # as an unconditional value no-match that inversion does NOT
                    # resurrect. Sub-phase 75.1 made the in-tree engine agree by
                    # short-circuiting every value mode to `false` on an absent
                    # header BEFORE the XOR (`crates/envoy-config/src/matcher.rs`,
                    # the `(_, None)` arm). Before 75.1 envoy-rust KEPT it — that was
                    # carry-forward CF-72-1, now CLOSED.
                    #
                    # The log line deliberately does NOT echo `x-a`: envoy-rust's
                    # `%REQ(NAME)%` operator is ALLOW-LIST gated (`REQ_ALLOW_LIST`,
                    # `crates/envoy-accesslog/src/command_operator.rs`) and a
                    # `%REQ(X-A)%` would be BOOT-FATAL. The witness is the keep/drop
                    # LINE COUNT plus whole-line cross-proxy equality, exactly as in
                    # fixture 0078.
                    filter:
                      header_filter:
                        header:
                          name: x-a
                          exact_match: "v"
                          invert_match: true
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { path: "/x" }
                          direct_response:
                            status: 200
                            body: { inline_string: "hi\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 2: Create `tests/fixtures/0084-headermatcher-absence-accesslog/envoy-rust.yaml`**

Byte-identical to Step 1 **except the four recipe deltas** (MEASURED across the `0078`–`0082` lineage at this PLAN-write): (a) the `admin:` line is DELETED, (b) the listener bind is `127.0.0.1` not `0.0.0.0`, (c) `generate_request_id: false` is DELETED, (d) the access-log `path:` points at the `-envoy-rust-mount` dir. `node:`, `codec_type: HTTP1`, the filter body, the log format, the route table and the comments stay identical.

```yaml
node: { id: envoy-rust-phase-75-fixture-0084, cluster: envoy-rust-phase-75 }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0084-envoy-rust-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n"
                    # Sub-phase 75.2 (ADR-0156/0157/0158/0161) — the D1 witness on
                    # the ACCESS-LOG path, through the ADR-0150 `HeaderMatch` trait
                    # seam. A VALUE matcher (`exact_match`) plus `invert_match` with
                    # the header ABSENT must DROP: upstream treats a missing header
                    # as an unconditional value no-match that inversion does NOT
                    # resurrect. Sub-phase 75.1 made the in-tree engine agree by
                    # short-circuiting every value mode to `false` on an absent
                    # header BEFORE the XOR (`crates/envoy-config/src/matcher.rs`,
                    # the `(_, None)` arm). Before 75.1 envoy-rust KEPT it — that was
                    # carry-forward CF-72-1, now CLOSED.
                    #
                    # The log line deliberately does NOT echo `x-a`: envoy-rust's
                    # `%REQ(NAME)%` operator is ALLOW-LIST gated (`REQ_ALLOW_LIST`,
                    # `crates/envoy-accesslog/src/command_operator.rs`) and a
                    # `%REQ(X-A)%` would be BOOT-FATAL. The witness is the keep/drop
                    # LINE COUNT plus whole-line cross-proxy equality, exactly as in
                    # fixture 0078.
                    filter:
                      header_filter:
                        header:
                          name: x-a
                          exact_match: "v"
                          invert_match: true
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { path: "/x" }
                          direct_response:
                            status: 200
                            body: { inline_string: "hi\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 3: Verify the two configs differ ONLY by the four recipe deltas**

Run:
```bash
diff -u tests/fixtures/0084-headermatcher-absence-accesslog/envoy.yaml \
        tests/fixtures/0084-headermatcher-absence-accesslog/envoy-rust.yaml
```
Expected: exactly four changes — one deleted `admin:` line, one `0.0.0.0` → `127.0.0.1` on the listener bind, one deleted `generate_request_id: false`, one `path:` rewrite. **Nothing else.** If the diff shows a fifth hunk, the two files have drifted and the fixture would be testing two different configs.

- [ ] **Step 4: Create `tests/fixtures/0084-headermatcher-absence-accesslog/expectations.yaml`**

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0084-envoy-mount/access.log
    envoy_rust: /tmp/0084-envoy-rust-mount/access.log
  probes:
    # Probe 1 — DROPPED, and FIRST. THE LOAD-BEARING PROBE (divergence D1, the
    # former carry-forward CF-72-1). NO `x-a` header at all. The matcher is a
    # VALUE matcher (`exact_match: "v"`) with `invert_match: true`, and a missing
    # header is an unconditional value no-match that inversion does NOT
    # resurrect — so BOTH proxies emit NOTHING.
    #
    # This is the cell sub-phase 75.1 changed. On a PRE-75.1 tree the in-tree
    # engine computed `mode_result(false) ^ invert_match(true)` = KEEP, so
    # envoy-rust would write TWO lines here against upstream's ONE and this
    # fixture would fail its line-count assertion. That is why 75.2 depends on
    # 75.1 and could not have landed first.
    - method: get
      path: /x
      host: envoy-rust.test
      expected_status: 200
      expect_logged: false
    # Probe 2 — DROPPED, and SECOND. `x-a: v` MATCHES `exact_match: "v"`, and
    # `invert_match` flips the match to a drop: `true ^ true` = false. This is the
    # ordinary inverted-match cell and is PARITY on both proxies before and after
    # 75.1 — it is the control that proves the matcher is wired up at all, so
    # probe 1's absence is attributable to the ABSENCE rule rather than to a
    # dead filter.
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-a", "v"]
      expected_status: 200
      expect_logged: false
    # Probe 3 — KEPT, and LAST. `x-a: zzz` does NOT match `exact_match: "v"`, and
    # `invert_match` flips the no-match to a keep: `false ^ true` = true. Expected
    # line (byte-identical on both sides):
    #   STATUS=200 PATH=/x
    #
    # The LAST probe is KEPT, therefore the driver's ordering-aware
    # `suppression_settle` (tests/differential/src/lib.rs) charges the cheap 2 s
    # CF70_3_SETTLE rather than the 12 s CF71_1_SETTLE. `suppression_settle`
    # inspects ONLY `probes.last()`, so it is the identity of the LAST probe that
    # decides the settle — not the position of any other probe.
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-a", "zzz"]
      expected_status: 200
      expect_logged: true
  # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). There is NO expected
  # log-line field on this driver: it asserts (a) each side's file holds exactly
  # `expected_logged_count(probes)` lines — here ONE — and (b) the lines are
  # byte-identical between upstream Envoy v1.33.0 and envoy-rust. Both proxies
  # must agree on ALL THREE halves of the filter decision: the kept `x-a: zzz`
  # line AND the absence of any line for the absent-header probe AND for the
  # value-matching probe. A one-sided keep fails the line-count assertion before
  # the byte compare is reached. The measured line is:
  #   STATUS=200 PATH=/x
  # The only route is a direct_response → `clusters: []`, no backend spawns.
```

- [ ] **Step 5: Create `tests/fixtures/0084-headermatcher-absence-accesslog/README.md`**

Model it on `tests/fixtures/0078-accesslog-header-filter/README.md` (84 lines) and `tests/fixtures/0082-accesslog-metadata-filter-key-not-found/README.md` (103 lines). It MUST contain, in this order:

1. **Title + one-paragraph summary** — this is the D1 witness for the sub-phase-75.1 `HeaderMatcher` absence rule, on the ACCESS-LOG path, through the ADR-0150 `HeaderMatch` trait seam; the sibling `0085` witnesses D2; the ROUTE-path witness is `0083`.
2. **"What this proves"** — the three-row keep/drop table from the task Context above, with the columns `# | request | matcher verdict | emitted?`, plus one sentence naming probe 1 as the D1 cell and stating that a pre-75.1 tree would emit two lines here.
3. **"The rule"** — the four-line pseudocode block from the "The rule this sub-phase witnesses" section of this plan, verbatim, plus the sentence: *"`present_match(want)` is the ONLY mode evaluated with the header ABSENT; every value mode short-circuits to `false` and `invert_match` is NOT applied. An EMPTY header VALUE counts as PRESENT."*
4. **"Why the log line does not echo `x-a`"** — `%REQ(NAME)%` is allow-list gated (`REQ_ALLOW_LIST` in `crates/envoy-accesslog/src/command_operator.rs`, seven entries: `:method`, `:authority`, `:path`, `x-envoy-original-path`, `x-forwarded-for`, `user-agent`, `x-request-id`); `%REQ(X-A)%` is BOOT-FATAL in envoy-rust. The witness is the line COUNT plus whole-line equality, as in `0078`.
5. **"Probes / driver"** — `kind: http1_access_log_byte_exact`; `expected_logged_count` = 1; probe ordering follows the kept-LAST convention (ADR-0147), and **because the LAST probe is KEPT the driver's ordering-aware `suppression_settle` charges the cheap 2 s `CF70_3_SETTLE` rather than the 12 s `CF71_1_SETTLE`.** **Do NOT write "probe N is placed FIRST/SECOND *so* the last probe is kept"** — `suppression_settle` inspects only `probes.last()`, so that causal claim is a non-sequitur (this is exactly the M74-31 defect Task 7 removes from four other files; do not mint a fifth).
6. **"Per-side divergences"** — a four-row table: `admin:` block dropped on the envoy-rust side; listener bind `0.0.0.0` → `127.0.0.1`; `generate_request_id: false` dropped; access-log `path:` repointed at the `-envoy-rust-mount` dir. Add the note that `codec_type: HTTP1` is written on **BOTH** sides and is NOT a divergence (envoy-rust has no serde default for it; upstream would default to `AUTO` — ADR-0158 C3).
7. **"Two conflation traps"** — (**Trap A**) `HeaderMatcher.present_match` (this fixture's family) is a DIFFERENT message from `ValueMatcher.present_match` (RBAC / access-log metadata, e.g. fixture `0044`), where `present_match: false` NEVER matches. The two rules AGREE in three of four cells and differ in exactly one (ABSENT × `want = false`). Do NOT unify them. (**Trap B**) `HeaderMatcher.invert_match` is unrelated to `MetadataMatcher.invert` (CF-74-1), which is MEASURED accepted-but-INERT upstream and stays boot-fatal here — "implementing" it would CREATE a divergence.
8. **"Cross-references"** — ADR-0156 (the phase-75 pick), ADR-0157 (the §6.1 split), ADR-0158 (the parent's §6.2 reconciliation, incl. the single-log-file driver constraint that forced two fixtures), ADR-0159 (75.1's §6.2 reconciliation), ADR-0161 (this sub-phase's §6.2 reconciliation); the sibling fixture `0085`; the route-path witness `0083`; the shape stencil `0078`.
9. **"Deferred / out of scope"** — CF-72-2 (name-only `{ name }`, `treat_missing_header_as_empty`, the top-level `contains_match` arm — all REJECT-direction load-parity gaps that cannot appear in a fixture until implemented, because the config would not boot on the subject side); CF-75-1 (`exact_match: ""` degenerates to a PRESENCE match upstream); CF-75-2 (upstream comma-joins duplicate header values before value matching; envoy-rust matches only the first occurrence). All three are BANKED in `BEHAVIOR_CONTRACT.md`, not fixed here.

- [ ] **Step 6: Build the debug binary the harness actually runs**

Run: `cargo build -p envoy-bin`
Expected: `Finished` with no errors. **This is not optional.** The differential harness executes `target/debug/envoy-bin`, not release; a stale binary reds the fixture on old code rather than on its subject.

- [ ] **Step 7: Run the fixture and confirm it PASSES**

> This task's deliverable is a characterization pin on ALREADY-CORRECT code (sub-phase 75.1 landed the fix), so it passes immediately. TDD's RED is honored by the mutation check in Step 8, not by a failing first run. This is the standing house pattern for §5.2-style pins.

Run (after Task 2 lands the entrypoint — if executing tasks strictly in order, defer Steps 7–8 until Task 2 Step 2 and run them together):
```bash
cargo test -p differential --test headermatcher_absence_accesslog -- --nocapture
```
Expected: `test result: ok. 1 passed; 0 failed`.

**Do not accept a green from the exit code alone** — assert on the `N passed` count. `cargo test -p <pkg> --test <name>` can exit 0 with `0 passed; N filtered out` (a false green) and can exit 101 on `error: no test target named …` (a false RED). Read the output text.

A ~1–3 s green on a backend-free fixture with a warm Envoy image is NORMAL, not a silent skip. If you want to prove it really ran, poll `docker ps` in a second shell during the run.

If the run REDs with `client error (Connect)` on this and many other fixtures at once, the Docker daemon is down, not the fixture: `sudo setfacl -m u:esa:rw /dev/kvm && systemctl --user restart docker-desktop`, then re-run.

- [ ] **Step 8: Mutation check — prove the fixture is load-bearing (this is the RED)**

Do this in a **scratch `git worktree`**, never in the main tree — a parallel agent's `git checkout` can silently revert an in-place mutation and hand you a false green.

```bash
git worktree add /tmp/claude-1000/mut-0084 HEAD
cd /tmp/claude-1000/mut-0084
git reset --hard main   # ensure the worktree is on current main, not the session's start commit
```

In `/tmp/claude-1000/mut-0084/crates/envoy-config/src/matcher.rs`, revert the 75.1 engine to the pre-fix uniform XOR by MOVING the `(_, None) => return false,` arm from its position (after the `PresentMatch` arm) to the TOP of the `match (&self.mode, value)` — i.e. reinstate the naive uniform absent-DROP. Then copy this task's fixture and Task 2's entrypoint into the worktree and run:

```bash
cd /tmp/claude-1000/mut-0084
grep -n 'None) => return false' crates/envoy-config/src/matcher.rs   # confirm the mutation is PRESENT
cargo build -p envoy-bin 2>&1 | grep -c 'Compiling envoy-config'      # must be >= 1 — a forced rebuild
cargo test -p differential --test headermatcher_absence_accesslog -- --nocapture
```

Expected: **RED**, with failure text naming the line-count assertion, e.g.
`envoy-rust emitted 2 access-log lines but 1 were expected to be logged`.

**Read the failure TEXT.** A RED that says `admin ready: ConnectionRefused` or `client error (Connect)` is a container/startup failure that never reached an assertion — that is NOT evidence. Re-run the UNMUTATED fixture from the same worktree as a control; it must be GREEN. Only "unmutated GREEN + mutated RED with a line-count message" is a valid mutation result.

Then clean up **only your own** worktree:
```bash
cd /home/esa/git/envoy-rust
git worktree remove --force /tmp/claude-1000/mut-0084
```
Do **not** touch anything under `.claude/worktrees/` — that belongs to a parallel workstream.

- [ ] **Step 9: Commit**

```bash
git add tests/fixtures/0084-headermatcher-absence-accesslog/
git commit -m "phase 75.2: fixture 0084 — the D1 access-log witness (value matcher + invert + absent DROPS)"
```

---

### Task 2: The `0084` test entrypoint

**Files:**
- Create: `tests/differential/tests/headermatcher_absence_accesslog.rs`

**Interfaces:**
- Consumes: the fixture directory created in **Task 1**; the existing `differential::run_fixture(&std::path::Path) -> impl Future<Output = anyhow::Result<()>>`.
- Produces: the cargo test target name `headermatcher_absence_accesslog` and the test function `headermatcher_absence_accesslog`, referenced by Task 1 Steps 7–8 and by the §7.5 gate in Task 9.

**Context.** `tests/differential/Cargo.toml` has **no `[[test]]` stanza** — cargo autodiscovers `tests/*.rs`, so this one file is the entire registration cost. No manifest edit, no registry entry, no `ci.yml` change. The naming convention follows `tests/differential/tests/headermatcher_absence_parity.rs` (fixture `0083`). The house `//!` header is long by design: it is the only place a reader lands when a CI failure names this test.

- [ ] **Step 1: Create the file**

```rust
//! Docker-gated differential test for fixture
//! 0084-headermatcher-absence-accesslog.
//!
//! Sub-phase 75.2 (ADR-0156 / ADR-0157 / ADR-0158 / ADR-0161) — the **D1**
//! cross-proxy witness for the `HeaderMatcher` ABSENCE rule on the ACCESS-LOG
//! path, i.e. the SECOND consumer of the shared matching engine, reached through
//! the ADR-0150 `HeaderMatch` trait seam (`LogFilter::Header { matcher }` in
//! `crates/envoy-accesslog/src/filter.rs` dispatches to
//! `impl envoy_accesslog::HeaderMatch for HeaderMatcher` in
//! `crates/envoy-config/src/matcher.rs`, whose trait object is injected by
//! `compile_access_log_filter` in `crates/envoy-http1/src/hcm.rs`). The ROUTE-path
//! witness of the same rule is fixture 0083; the D2 sibling is fixture 0085.
//!
//! Shape: one H1 HCM listener; ONE `FileAccessLog` sink with
//! `text_format_source` `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%`, gated by
//! `header_filter { header: { name: x-a, exact_match: "v", invert_match: true } }`;
//! ONE `direct_response` route `/x` → 200 `hi`; `clusters: []`, no backend spawns.
//!
//! THE MEASURED RULE (landed by sub-phase 75.1): `present_match(want)` is the ONLY
//! mode evaluated with the header ABSENT — `(present == want) ^ invert_match`.
//! EVERY value mode short-circuits to `false` when the header is absent, and
//! `invert_match` is NOT applied: upstream treats a missing header as an
//! unconditional value no-match that inversion does not resurrect. An EMPTY header
//! VALUE counts as PRESENT.
//!
//! Three probes, ordered so the LAST is KEPT (ADR-0147):
//! (1) `GET /x` with NO `x-a` → **DROPPED — the D1 cell.** Before 75.1 the in-tree
//!     engine computed `false ^ true` = KEEP, so envoy-rust wrote TWO lines against
//!     upstream's ONE and this fixture would be RED. That is why 75.2 was gated
//!     behind 75.1.
//! (2) `GET /x` with `x-a: v` → DROPPED (value matches, `invert_match` flips it to
//!     a drop). The control that proves the filter is live, so probe 1's silence is
//!     attributable to the ABSENCE rule and not to a dead matcher.
//! (3) `GET /x` with `x-a: zzz` → KEPT (value does not match, `invert_match` flips
//!     it to a keep).
//!
//! Each side's file holds EXACTLY ONE line, byte-identical ACROSS THE TWO PROXIES:
//! `STATUS=200 PATH=/x`. Because the LAST probe is KEPT, the driver's
//! ordering-aware `suppression_settle` charges the cheap 2 s `CF70_3_SETTLE`
//! instead of the 12 s `CF71_1_SETTLE` (it inspects only `probes.last()`).
//!
//! The line deliberately does NOT echo `x-a`: envoy-rust's `%REQ(NAME)%` operator
//! is ALLOW-LIST gated (`REQ_ALLOW_LIST`,
//! `crates/envoy-accesslog/src/command_operator.rs`), so `%REQ(X-A)%` would be
//! BOOT-FATAL. The witness is the keep/drop LINE COUNT plus whole-line
//! cross-proxy equality — the same design fixture 0078 uses.
//!
//! PURE cross-proxy equality: there is no static expected-line field on this
//! driver. Both proxies must agree on the kept line AND on the ABSENCE of a line
//! for each dropped probe; a one-sided keep fails the line-count assertion before
//! the byte compare is reached.

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

- [ ] **Step 2: Run the fixture end to end**

Run:
```bash
cargo build -p envoy-bin
cargo test -p differential --test headermatcher_absence_accesslog -- --nocapture
```
Expected: `test result: ok. 1 passed; 0 failed; 0 ignored`.

Assert on the `1 passed` figure, not the exit code. If you deferred Task 1 Steps 7–8, run them now.

- [ ] **Step 3: Confirm formatting and lint are clean for the new file**

Run:
```bash
cargo fmt --all -- --check
cargo clippy -p differential --all-targets --all-features -- -D warnings
```
Expected: both silent / `Finished`. A clippy run that completes in ~1 s off a handful of `Checking` lines is PARTIALLY CACHED — `touch tests/differential/tests/headermatcher_absence_accesslog.rs` and re-run before believing it. (`cargo clippy` prints `Checking`, not `Compiling`.)

- [ ] **Step 4: Commit**

```bash
git add tests/differential/tests/headermatcher_absence_accesslog.rs
git commit -m "phase 75.2: test entrypoint for fixture 0084"
```

---

### Task 3: Fixture `0085-headermatcher-absence-accesslog-present-polarity` — the D2 witness

**Files:**
- Create: `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/envoy.yaml`
- Create: `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/envoy-rust.yaml`
- Create: `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/expectations.yaml`
- Create: `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/README.md`

**Interfaces:**
- Consumes: nothing from Tasks 1–2 (the two fixtures are independent). Consumes the same unchanged `Driver::Http1AccessLogByteExact`.
- Produces: the fixture directory path `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity`, passed to `differential::run_fixture(&dir)` by **Task 4**.

**Context — why this is a SEPARATE fixture and not a second sink in `0084`.** MEASURED at the parent phase's state-2 and RE-CONFIRMED at this PLAN-write: `AccessLogPaths` (`tests/differential/src/lib.rs:1088-1093`) is `{ envoy: String, envoy_rust: String }` under `#[serde(deny_unknown_fields)]` — exactly ONE log file per side — and only the envoy-side parent directory is bind-mounted into the container (`lib.rs:4019`), so a second sink writing elsewhere would not even be visible to the host. Corpus census: the maximum number of `name: envoy.access_loggers.file` sinks in ANY fixture config is **1**. This REFUTES the parent SPEC's multi-sink design (ADR-0158) and is why one rule needs a sibling PAIR — exactly as `0081`/`0082` split the two polarities of the `metadata_filter` rule.

**The matcher and the probes.** The filter is a plain, NON-inverted `present_match: false` on header `x-a` — the simplest possible spelling, which is what makes D2 worse than D1:

| # | request | engine evaluation | verdict | `expect_logged` |
|---|---|---|---|---|
| 1 | `GET /x`, `x-a: v` | `PresentMatch(false)`: `v.is_some()(true) == want(false)` → `false`; `false ^ false` → `false` | **DROPPED** — the D2 cell | `false` |
| 2 | `GET /x`, **no** `x-a` | `v.is_some()(false) == want(false)` → `true`; `true ^ false` → `true` | **KEPT** (LAST) | `true` |

`expected_logged_count` = **1**. Before 75.1 the in-tree engine returned `true` UNCONDITIONALLY for `PresentMatch(false)`, so probe 1 was KEPT — two lines against upstream's one, and RED.

- [ ] **Step 1: Create `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/envoy.yaml`**

```yaml
node: { id: envoy-rust-phase-75-fixture-0085, cluster: envoy-rust-phase-75 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                generate_request_id: false
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0085-envoy-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n"
                    # Sub-phase 75.2 (ADR-0156/0157/0158/0161) — the D2 witness on
                    # the ACCESS-LOG path, the sibling of fixture 0084.
                    #
                    # Upstream `present_match: false` means "the header must be
                    # ABSENT": the rule is `(present == want)`. Before sub-phase
                    # 75.1 the in-tree engine modelled this arm as UNCONDITIONALLY
                    # TRUE, so a plain, NON-inverted, single-line matcher silently
                    # matched EVERY request here and only header-absent requests
                    # upstream. That made D2 strictly worse than D1: it needs no
                    # `invert_match` to fire.
                    #
                    # NOTE — `HeaderMatcher.present_match` is a DIFFERENT field on a
                    # DIFFERENT message from `ValueMatcher.present_match` (RBAC and
                    # access-log METADATA, e.g. fixture 0044), where
                    # `present_match: false` NEVER matches. That rule is CORRECT and
                    # must NOT be "fixed" to match this one. See BEHAVIOR_CONTRACT.md,
                    # the Phase 75 block, Trap A.
                    filter:
                      header_filter:
                        header:
                          name: x-a
                          present_match: false
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { path: "/x" }
                          direct_response:
                            status: 200
                            body: { inline_string: "hi\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 2: Create `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/envoy-rust.yaml`**

Byte-identical to Step 1 except the same four recipe deltas: delete the `admin:` line, bind `127.0.0.1`, delete `generate_request_id: false`, and repoint `path:` to `/tmp/0085-envoy-rust-mount/access.log`.

```yaml
node: { id: envoy-rust-phase-75-fixture-0085, cluster: envoy-rust-phase-75 }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0085-envoy-rust-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n"
                    # Sub-phase 75.2 (ADR-0156/0157/0158/0161) — the D2 witness on
                    # the ACCESS-LOG path, the sibling of fixture 0084.
                    #
                    # Upstream `present_match: false` means "the header must be
                    # ABSENT": the rule is `(present == want)`. Before sub-phase
                    # 75.1 the in-tree engine modelled this arm as UNCONDITIONALLY
                    # TRUE, so a plain, NON-inverted, single-line matcher silently
                    # matched EVERY request here and only header-absent requests
                    # upstream. That made D2 strictly worse than D1: it needs no
                    # `invert_match` to fire.
                    #
                    # NOTE — `HeaderMatcher.present_match` is a DIFFERENT field on a
                    # DIFFERENT message from `ValueMatcher.present_match` (RBAC and
                    # access-log METADATA, e.g. fixture 0044), where
                    # `present_match: false` NEVER matches. That rule is CORRECT and
                    # must NOT be "fixed" to match this one. See BEHAVIOR_CONTRACT.md,
                    # the Phase 75 block, Trap A.
                    filter:
                      header_filter:
                        header:
                          name: x-a
                          present_match: false
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { path: "/x" }
                          direct_response:
                            status: 200
                            body: { inline_string: "hi\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 3: Verify the two configs differ ONLY by the four recipe deltas**

Run:
```bash
diff -u tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/envoy.yaml \
        tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/envoy-rust.yaml
```
Expected: exactly four changes, as in Task 1 Step 3. Nothing else.

- [ ] **Step 4: Create `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/expectations.yaml`**

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0085-envoy-mount/access.log
    envoy_rust: /tmp/0085-envoy-rust-mount/access.log
  probes:
    # Probe 1 — DROPPED, and FIRST. THE LOAD-BEARING PROBE (divergence D2).
    # `x-a: v` is PRESENT, and `present_match: false` requires the header to be
    # ABSENT: `(present == want)` = `(true == false)` = false. BOTH proxies emit
    # NOTHING.
    #
    # This is the cell sub-phase 75.1 changed, and it is the WORSE of the two
    # divergences because it fires on a plain, NON-inverted, single-line matcher.
    # On a PRE-75.1 tree the in-tree engine returned `true` UNCONDITIONALLY for
    # `PresentMatch(false)`, so envoy-rust would write TWO lines here against
    # upstream's ONE and this fixture would fail its line-count assertion.
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-a", "v"]
      expected_status: 200
      expect_logged: false
    # Probe 2 — KEPT, and LAST. NO `x-a` header at all:
    # `(present == want)` = `(false == false)` = true → the record IS emitted.
    # Expected line (byte-identical on both sides):
    #   STATUS=200 PATH=/x
    #
    # The LAST probe is KEPT, therefore the driver's ordering-aware
    # `suppression_settle` charges the cheap 2 s CF70_3_SETTLE rather than the
    # 12 s CF71_1_SETTLE. It inspects ONLY `probes.last()`, so it is the identity
    # of the LAST probe that decides the settle.
    - method: get
      path: /x
      host: envoy-rust.test
      expected_status: 200
      expect_logged: true
  # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). There is NO expected
  # log-line field on this driver: it asserts (a) each side's file holds exactly
  # `expected_logged_count(probes)` lines — here ONE — and (b) those lines are
  # byte-identical between upstream Envoy v1.33.0 and envoy-rust. Both proxies
  # must agree on the kept header-absent line AND on the ABSENCE of any line for
  # the header-present probe. The measured line is:
  #   STATUS=200 PATH=/x
  # The only route is a direct_response → `clusters: []`, no backend spawns.
```

- [ ] **Step 5: Create `tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/README.md`**

Same nine-section structure as Task 1 Step 5, with these substitutions:

- Section 1: this is the **D2** witness; the sibling `0084` witnesses D1; the ROUTE-path witness is `0083`.
- Section 2: the TWO-row keep/drop table from this task's Context, with the emphasis that **D2 fires on a plain, NON-inverted, single-line matcher** — no `invert_match` is needed — and that a pre-75.1 tree logs both probes.
- Section 5: `expected_logged_count` = 1; kept-LAST; the cheap 2 s `CF70_3_SETTLE` **because the LAST probe is KEPT**. Again: do NOT write a causal "placed FIRST/SECOND *so* the last probe is kept" claim.
- Section 7, **Trap A, is load-bearing here and must be prominent**: this fixture's `present_match` is `HeaderMatcher.present_match`, whose rule is `(present == want)`. `ValueMatcher.present_match` (RBAC / access-log metadata, fixture `0044`) is a DIFFERENT field on a DIFFERENT message where **`present_match: false` NEVER matches** — a different and CORRECT rule. After 75.1 the two AGREE in three of four cells and differ in exactly ONE (ABSENT × `want = false`, where `ValueMatcher` → `false` and `HeaderMatcher` → `true`). **Do NOT unify them and do NOT "fix" the `ValueMatcher` rule to match.**
- Section 8: same ADR list, plus a pointer to the new `### Phase 75` block in `BEHAVIOR_CONTRACT.md` (landed by Task 5) as the canonical statement of the polarity rule.

- [ ] **Step 6: Run the fixture and confirm it PASSES**

(As with Task 1, this needs Task 4's entrypoint; if executing strictly in order, defer Steps 6–7 until Task 4 Step 2.)

```bash
cargo build -p envoy-bin
cargo test -p differential --test headermatcher_absence_accesslog_present_polarity -- --nocapture
```
Expected: `test result: ok. 1 passed; 0 failed`. Assert on `1 passed`, not the exit code.

- [ ] **Step 7: Mutation check — prove the fixture is load-bearing (this is the RED)**

Same scratch-worktree procedure as Task 1 Step 8, but with a mutation targeted at **D2** specifically. In `/tmp/claude-1000/mut-0085/crates/envoy-config/src/matcher.rs`, revert the `PresentMatch` arm to its pre-75.1 body:

```rust
// MUTATION (pre-75.1 behavior): present_match(false) was unconditionally true.
(HeaderMatcherMode::PresentMatch(want_present), v) => {
    if *want_present { v.is_some() } else { true }
}
```

Then:
```bash
cd /tmp/claude-1000/mut-0085
grep -n 'if \*want_present' crates/envoy-config/src/matcher.rs   # confirm the mutation is PRESENT
cargo build -p envoy-bin 2>&1 | grep -c 'Compiling envoy-config'  # must be >= 1
cargo test -p differential --test headermatcher_absence_accesslog_present_polarity -- --nocapture
```

Expected: **RED** with `envoy-rust emitted 2 access-log lines but 1 were expected to be logged`.

Read the failure TEXT — a container/startup failure is not evidence. Run the UNMUTATED fixture from the same worktree as the control; it must be GREEN. Then `git worktree remove --force /tmp/claude-1000/mut-0085`.

- [ ] **Step 8: Commit**

```bash
git add tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/
git commit -m "phase 75.2: fixture 0085 — the D2 access-log witness (present_match: false means header-must-be-ABSENT)"
```

---

### Task 4: The `0085` test entrypoint

**Files:**
- Create: `tests/differential/tests/headermatcher_absence_accesslog_present_polarity.rs`

**Interfaces:**
- Consumes: the fixture directory created in **Task 3**; `differential::run_fixture`.
- Produces: the cargo test target and function `headermatcher_absence_accesslog_present_polarity`, referenced by Task 3 Steps 6–7 and by the §7.5 gate in Task 9.

- [ ] **Step 1: Create the file**

```rust
//! Docker-gated differential test for fixture
//! 0085-headermatcher-absence-accesslog-present-polarity.
//!
//! Sub-phase 75.2 (ADR-0156 / ADR-0157 / ADR-0158 / ADR-0161) — the **D2**
//! cross-proxy witness for the `HeaderMatcher` ABSENCE rule on the ACCESS-LOG
//! path, and the sibling of fixture 0084 (which witnesses D1). Two fixtures
//! rather than one is a MEASURED constraint, not a preference: the byte-exact
//! access-log driver takes exactly ONE log file per side (`AccessLogPaths` in
//! `tests/differential/src/lib.rs` is two `String` fields under
//! `deny_unknown_fields`, and only the envoy-side parent dir is bind-mounted), so
//! one sink per fixture is the only shape available — ADR-0158. This mirrors the
//! existing sibling pair 0081 / 0082.
//!
//! Shape: one H1 HCM listener; ONE `FileAccessLog` sink with
//! `text_format_source` `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%`, gated by
//! `header_filter { header: { name: x-a, present_match: false } }` — a plain,
//! NON-inverted, single-line matcher; ONE `direct_response` route `/x` → 200
//! `hi`; `clusters: []`, no backend spawns.
//!
//! THE MEASURED RULE (landed by sub-phase 75.1): upstream `present_match: false`
//! means **the header must be ABSENT** — `(present == want) ^ invert_match`.
//! Before 75.1 the in-tree engine modelled this arm as UNCONDITIONALLY TRUE, so
//! the matcher silently matched every request here and only header-absent
//! requests upstream. **D2 is strictly worse than D1** because it needs no
//! `invert_match` to fire, and before phase 75 it had NO behavioral test anywhere
//! in the tree.
//!
//! Two probes, ordered so the LAST is KEPT (ADR-0147):
//! (1) `GET /x` with `x-a: v` → **DROPPED — the D2 cell.** `(true == false)` is
//!     false. A pre-75.1 tree KEPT it, writing TWO lines against upstream's ONE.
//! (2) `GET /x` with NO `x-a` → KEPT. `(false == false)` is true.
//!
//! Each side's file holds EXACTLY ONE line, byte-identical ACROSS THE TWO
//! PROXIES: `STATUS=200 PATH=/x`. Because the LAST probe is KEPT, the driver's
//! ordering-aware `suppression_settle` charges the cheap 2 s `CF70_3_SETTLE`
//! instead of the 12 s `CF71_1_SETTLE` (it inspects only `probes.last()`).
//!
//! CONFLATION TRAP — `HeaderMatcher.present_match` (this fixture) and
//! `ValueMatcher.present_match` (RBAC / access-log METADATA, fixture 0044) are
//! DIFFERENT fields on DIFFERENT messages with DIFFERENT measured rules. For the
//! `ValueMatcher` one, `present_match: false` NEVER matches — that rule is
//! CORRECT and must NOT be "fixed" to match this one. After 75.1 the two agree in
//! three of four cells and differ in exactly one: ABSENT with `want = false`,
//! where `ValueMatcher` yields `false` and `HeaderMatcher` yields `true`.
//!
//! The line deliberately does NOT echo `x-a`: `%REQ(NAME)%` is ALLOW-LIST gated in
//! envoy-rust, so `%REQ(X-A)%` would be BOOT-FATAL. PURE cross-proxy equality:
//! both proxies must agree on the kept line AND on the ABSENCE of a line for the
//! dropped probe.

use std::path::PathBuf;

#[tokio::test]
async fn headermatcher_absence_accesslog_present_polarity() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Run the fixture end to end**

```bash
cargo build -p envoy-bin
cargo test -p differential --test headermatcher_absence_accesslog_present_polarity -- --nocapture
```
Expected: `test result: ok. 1 passed; 0 failed; 0 ignored`. If you deferred Task 3 Steps 6–7, run them now.

- [ ] **Step 3: Confirm formatting and lint**

```bash
cargo fmt --all -- --check
cargo clippy -p differential --all-targets --all-features -- -D warnings
```
Expected: both clean. `touch` the new file and re-run if clippy finishes suspiciously fast off a handful of `Checking` lines.

- [ ] **Step 4: Commit**

```bash
git add tests/differential/tests/headermatcher_absence_accesslog_present_polarity.rs
git commit -m "phase 75.2: test entrypoint for fixture 0085"
```

---

### Task 5: `BEHAVIOR_CONTRACT.md` — the new `### Phase 75` `present_match`-polarity block

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — INSERT a new block immediately before the `## xDS wire state machine` heading.

**Interfaces:**
- Consumes: the fixtures created in Tasks 1 and 3 (the block's §E names them as the authoritative fixtures).
- Produces: a `### Phase 75 (…)` heading and a `**§D` sub-heading inside it, cited by Task 3 Step 5 (the `0085` README) and by both fixtures' YAML comments. **Task 6 appends to a DIFFERENT part of the file** (the phase-72 `**§D` record at `:2423`) — do not confuse the two.

**Context and the insertion anchor.** `BEHAVIOR_CONTRACT.md` is 3363 lines and exceeds a single Read — chunk it with `offset`/`limit` or `grep -n`. Its convention is one `### Phase NN (ADR-…): <title>` block per phase in ASCENDING order, each closed by a `---` line. As of `HEAD == 3f0ec89`:

- the phase-74 block opens at **`:2493`**, its body ends at **`:2673`**, its closing `---` is at **`:2675`**;
- **`## xDS wire state machine` is at `:2677`** — the new block goes immediately before it.

**These line numbers WILL drift. Anchor on TEXT, not on numbers.** Re-derive before editing:

```bash
grep -n '^## xDS wire state machine' docs/envoy-rust/BEHAVIOR_CONTRACT.md
grep -n '^### Phase 7[0-9]' docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

> The `SPEC.md` for this sub-phase cites this insertion point as `~2632` / `:2631` / `:2633`. **Those numbers are STALE by +44** — sub-phase 75.1 landed ~44 lines into this file after the SPEC was authored. This is §6.2 correction **C1**; use the anchors above.

- [ ] **Step 1: Re-derive the insertion anchor on the live file**

Run the two `grep -n` commands above and record the current line number of `## xDS wire state machine`. Confirm the immediately preceding non-blank line is `---` and the block above it opens with `### Phase 74 (ADR-0154/0155)`.

- [ ] **Step 2: Insert the new block immediately before `## xDS wire state machine`**

Insert this text, followed by a blank line, `---`, and a blank line, so the file's block convention is preserved:

````markdown
### Phase 75 (ADR-0156/0157/0158/0159/0161): `HeaderMatcher` ABSENCE semantics — the `present_match` POLARITY rule and its two-consumer witnesses

> Fixtures `0083-headermatcher-absence-parity` (ROUTE path, sub-phase 75.1) +
> `0084-headermatcher-absence-accesslog` and
> `0085-headermatcher-absence-accesslog-present-polarity` (ACCESS-LOG path,
> sub-phase 75.2). MEASURED cross-proxy against `envoyproxy/envoy:v1.33.0` on
> BOTH proxies — a 13-probe backend-free ROUTE matrix (7 matcher modes ×
> invert polarity × {absent, matching value, non-matching value, numeric value,
> empty value}) and a nine-sink ACCESS-LOG probe read back after a graceful
> `docker stop -t 15` flush.

**§A The rule.** One engine, `HeaderMatcher::matches` in
`crates/envoy-config/src/matcher.rs` — an exhaustive tuple `match` over
`(&self.mode, value)` whose absent-header arm sits AFTER the `present_match` arm
and BEFORE every value arm, closed by an XOR with `invert_match`:

```
present := the named header is present (name matched case-insensitively;
           an EMPTY VALUE still counts as PRESENT)

if mode is present_match(want):   result = (present == want) XOR invert_match
else if not present:              result = false      # invert_match NOT applied
else:                             result = mode_matches(value) XOR invert_match
```

`present_match(want)` is the **ONLY** mode evaluated with the header ABSENT, and
therefore the only one that carries an absent header into `invert_match`. In
particular **`present_match: false` means "the header must be ABSENT"** — it is
NOT "no presence requirement".

**§B The four-cell polarity matrix** (MEASURED both proxies; `invert_match`
absent). This is the surface sub-phase 75.1 corrected and sub-phase 75.2
witnesses cross-proxy on the access-log path:

| | header PRESENT | header ABSENT |
|---|---|---|
| `present_match: true` | MATCH | no match |
| `present_match: false` | **no match** | **MATCH** |

Before 75.1 the in-tree engine returned `true` for BOTH cells of the
`present_match: false` row (divergence **D2**). The `present_match: true` row was
already correct. Applying `invert_match` XORs each cell.

**§C The MEASURED access-log matrix** (nine `FileAccessLog` sinks, one per
`header_filter` under test, each with a distinct `text_format_source`; four
requests — `/absent` with no `x-a`, `/valmatch` with `x-a: v`, `/valmiss` with
`x-a: zzz`, `/empty` with an EMPTY-VALUE `x-a`). The "pre-fix" column is the
pre-75.1 in-tree behavior; every cell now matches the upstream column.

| sink | `header_filter.header` | upstream logged | envoy-rust PRE-75.1 | verdict |
|---|---|---|---|---|
| s1 | `exact_match: v` + invert | `/valmiss`, `/empty` | `/absent`, `/valmiss`, `/empty` | **D1** — rust logged an extra `/absent`; **CLOSED**, witnessed by `0084` |
| s2 | `present_match: false` | `/absent` | all four | **D2** — rust logged 3 extra; **CLOSED**, witnessed by `0085` |
| s3 | `present_match: false` + invert | `/valmatch`, `/valmiss`, `/empty` | *(nothing)* | **D2** — rust logged 3 too few; **CLOSED** |
| s4 | `present_match: true` + invert | `/absent` | `/absent` | **P1 — PARITY, the guard** |
| s5 | name-only `{ name: x-a }` | `/valmatch`, `/valmiss`, `/empty` | *(boot-fatal)* | CF-72-2, reject-direction — see §D of the Phase 72 block |
| s6 | `string_match {exact: v}` + `treat_missing_header_as_empty` | `/valmatch` | *(boot-fatal)* | CF-72-2, reject-direction |
| s7 | `exact_match: v` | `/valmatch` | `/valmatch` | PARITY control |
| s8 | `string_match {exact: v}` + invert | `/valmiss`, `/empty` | `/absent`, `/valmiss`, `/empty` | **D1** — **CLOSED** |
| s9 | `present_match: true` | `/valmatch`, `/valmiss`, `/empty` | same | PARITY control — an EMPTY value counts as PRESENT on BOTH proxies |

**The access-log table matches the ROUTE-path matrix CELL FOR CELL.** The rule is
UNIFORM across the five subsystems that share the engine (H1 and H2 route
matching, HTTP RBAC, the fault-filter header gate, JWT-authn rule matching, and
the access-log `header_filter`), which is why the fix is one expression and why
the second witness is about the SEAM rather than about a different rule. The
access-log path reaches the engine through the ADR-0150 `Arc<dyn HeaderMatch>`
trait object, injected by `compile_access_log_filter`.

**§D The guard — the rule is MODE-SCOPED and must stay that way.** A naive
uniform "absent ⇒ DROP" simplification closes the value-matcher case (D1) while
BREAKING the `present_match: true` + invert + absent PARITY cell (s4 / P1),
minting a NEW divergence in its place. This is not hypothetical: the exact
mutation was applied in a scratch worktree at the 75.1 PLAN-write and again at
its implementation, and turns three in-process guards RED while leaving every
value-mode assertion green. **Any future refactor of the arm ORDER must preserve
it.**

**§E TRAP A — two DIFFERENT `present_match` fields; do NOT unify them.**
`HeaderMatcher.present_match` (this block) and `ValueMatcher.present_match` (the
RBAC and access-log-METADATA matcher, recorded in the Phase 35/36 material above
and witnessed by fixture `0044`) are different fields on different messages with
different MEASURED rules. For the `ValueMatcher` one, `present_match: false`
**NEVER matches**, even when the key is present — a DIFFERENT and CORRECT rule
that must NOT be "fixed" to match this one. Since 75.1 the two agree in THREE of
four cells and differ in exactly ONE:

| | `want = true` | `want = false` |
|---|---|---|
| PRESENT | `true` / `true` — agree | `false` / `false` — agree |
| ABSENT | `false` / `false` — agree | **`false` / `true` — DIFFER** |

(`ValueMatcher` verdict first, `HeaderMatcher` second.)

**§F TRAP B — two DIFFERENT `invert` fields.** `HeaderMatcher.invert_match` (this
block) and `MetadataMatcher.invert` (Phase 74, carry-forward CF-74-1) are
unrelated fields on unrelated messages. The latter is MEASURED accepted-but-INERT
upstream on the access-log path and stays BOOT-FATAL here; "implementing" it
would CREATE a divergence.

**§G Authoritative fixtures.** `0083` (ROUTE path, `kind: http1_probe_list`,
8 matchers / ~24 probes) is the FIRST differential witness of `invert_match` OR of
`HeaderMatcher.present_match` in the corpus. `0084` (ACCESS-LOG path,
`kind: http1_access_log_byte_exact`) witnesses **D1**: `exact_match: "v"` +
`invert_match: true` on `x-a`, three probes — absent → DROPPED (the D1 cell),
`x-a: v` → DROPPED, `x-a: zzz` → KEPT — one byte-identical line
`STATUS=200 PATH=/x` per side. `0085` witnesses **D2** on a plain, NON-inverted
`present_match: false`, two probes — `x-a: v` → DROPPED (the D2 cell), no `x-a` →
KEPT — again one byte-identical line per side. Both are backend-free
(`direct_response` 200, `clusters: []`) and both order the KEPT probe LAST, so the
driver's ordering-aware `suppression_settle` charges the cheap 2 s
`CF70_3_SETTLE`. Neither log line echoes `x-a`: envoy-rust's `%REQ(NAME)%`
operator is allow-list gated and `%REQ(X-A)%` is boot-fatal, so the witness is the
keep/drop LINE COUNT plus whole-line cross-proxy equality.

**§H TWO fixtures, not one — a driver constraint, not a preference.** The
byte-exact access-log driver takes exactly ONE log file per side: `AccessLogPaths`
(`tests/differential/src/lib.rs`) is `{ envoy: String, envoy_rust: String }` under
`deny_unknown_fields`, and only the envoy-side parent directory is bind-mounted
into the container, so a second sink writing elsewhere would be invisible to the
host. Corpus-wide, the maximum number of `envoy.access_loggers.file` sinks in any
single fixture config is **1**. One sink per fixture is therefore the only
available shape (ADR-0158), mirroring the existing sibling pair `0081`/`0082`.
````

- [ ] **Step 3: Verify the insertion is well-formed**

Run:
```bash
grep -n '^### Phase 7[0-9]' docs/envoy-rust/BEHAVIOR_CONTRACT.md
awk '/^### Phase 75 /{f=1} f&&/^## xDS wire state machine/{exit} f' \
  docs/envoy-rust/BEHAVIOR_CONTRACT.md | grep -c '^---$'
awk '/^### Phase 75 /{f=1} f' docs/envoy-rust/BEHAVIOR_CONTRACT.md | head -1
```
Expected: the phase headings now read `70, 71, 72, 73, 74, 75` in ascending order; the middle command prints exactly **`1`** (one `---` separator closes the new block before the `## xDS` heading); and the last prints the new `### Phase 75 …` heading, confirming it exists and is unique.

- [ ] **Step 4: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 75.2: BEHAVIOR_CONTRACT — the Phase 75 present_match-polarity block with both measured matrices"
```

---

### Task 6: `BEHAVIOR_CONTRACT.md` — extend the CF-72-2 record and add the CF-75-1 row

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — the phase-72 block's `**§D Name-only + treat_missing_header_as_empty …**` record.

**Interfaces:**
- Consumes: nothing. Independent of Tasks 1–5 except that Task 5's insertion shifts line numbers BELOW the phase-72 block only if run first — the §D record sits ABOVE the insertion point, so **its line number is unaffected by Task 5**. Re-derive by text anchor regardless.
- Produces: the extended CF-72-2 record and a new `**§E CF-75-1 …**` record, referenced by both new fixtures' READMEs (§9, "Deferred / out of scope").

**Context.** The existing record is five lines, currently at `:2423-2427` (the SPEC cites `:2379-2383`, **stale by +44** — §6.2 correction **C2**). Anchor on text:

```bash
grep -n '§D Name-only + treat_missing_header_as_empty' docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

Its current verbatim content is:

```markdown
**§D Name-only + treat_missing_header_as_empty (PV-5, MEASURED — inherited
boundary).** Upstream accepts `header: { name }` (presence match) and
`treat_missing_header_as_empty: true`; the in-tree `HeaderMatcher` deserializer
REJECTS both (name-only → "missing mode key"; `treat_missing_header_as_empty` →
unknown field). Kept fail-loud per ADR-0049; carry-forward **CF-72-2**.
```

Note the sub-heading letters in the phase-72 block are already in use through `**§F`; the new CF-75-1 record therefore becomes `**§G`. **Re-derive the next free letter before writing** rather than assuming:

```bash
awk '/^### Phase 72 /{f=1} f&&/^### Phase 73 /{exit} f' \
  docs/envoy-rust/BEHAVIOR_CONTRACT.md | grep -o '^\*\*§[A-Z]' | sort
```
(As of `HEAD == 3f0ec89` this prints `§A §B §C §D §E §F` — so the next free letter is `§G`. VERIFY rather than assume; sub-phase 75.1 rewrote the §C of this very block.)

- [ ] **Step 1: Replace the §D record with the extended version**

Replace the five lines above with:

```markdown
**§D Name-only, treat_missing_header_as_empty, and the top-level contains_match
(PV-5, MEASURED — inherited boundary; EXTENDED at phase 75).** Three members,
all REJECT-direction load-parity gaps: upstream accepts a spelling that
envoy-rust boot-fatals on, so a config carrying one never runs here and the
divergence cannot be witnessed differentially until it is implemented.
Carry-forward **CF-72-2**; owner is a future `HeaderMatcher` wire-shape-parity
phase, which should decide all three together rather than piecemeal.

1. **Name-only `header: { name: x-a }`** — upstream treats it as a PRESENCE
   match; the in-tree `HeaderMatcher` deserializer rejects it with "missing mode
   key". MEASURED at phase 72 and re-measured at the phase-75 pick (access-log
   sink s5: upstream logged `/valmatch`, `/valmiss` and `/empty` — i.e. every
   request whose `x-a` was present, empty value included).
2. **`treat_missing_header_as_empty: true`** — envoy-rust rejects it as an
   unknown field. Upstream does not merely ACCEPT it, it **HONORS** it: MEASURED
   at the phase-75 pick (access-log sink s6, `string_match {exact: v}` +
   `treat_missing_header_as_empty`), an absent header is treated as `""`, which
   fails `exact: v`, so only `/valmatch` was logged. Combined with
   `invert_match` it therefore FLIPS the D1 absent cell from DROP back to KEEP —
   which is why implementing it interacts with the Phase 75 §A rule and must not
   be done casually.
3. **The top-level `contains_match` arm** — a THIRD member, NEW at phase 75.
   Upstream accepts it (with a deprecation warning); envoy-rust rejects it as an
   unknown field. It is reachable in-tree only as `string_match: { contains: … }`,
   BY DESIGN — see the `HeaderMatcher` deserializer in
   `crates/envoy-config/src/bootstrap.rs`, which documents the v1.33.0 rationale
   for admitting `contains` only through `StringMatcher`.

Kept fail-loud per the ADR-0049 posture: envoy-rust is deliberately STRICTER at
config load rather than silently different at runtime.
```

- [ ] **Step 2: Add the new CF-75-1 record immediately after it**

Using the next free `**§<letter>` in the phase-72 block as re-derived above (expected `**§G`, but VERIFY):

```markdown
**§G Empty-string `exact_match` degenerates to a PRESENCE match upstream
(MEASURED at the phase-75 pick; carry-forward CF-75-1).** `header: { name: x-a,
exact_match: "" }` does NOT mean "the value must be the empty string" upstream —
it degenerates to a PRESENCE match:

| request | upstream | envoy-rust |
|---|---|---|
| no `x-a` | no match | no match |
| `x-a: v` | **MATCH** | **no match** |
| `x-a:` (empty value) | MATCH | MATCH |

envoy-rust performs a literal empty-value exact comparison, so it diverges in the
middle row. The degeneracy is specific to the DEPRECATED top-level scalar arm:
`string_match: { exact: "" }` does **NOT** degenerate (it is a genuine
empty-string comparison on both proxies), and PGV separately REJECTS
`string_match: { prefix: "" }` with *"value length must be at least 1
characters"*.

**Scope note (re-measured at the sub-phase-75.1 code review).** The
`exact_match: ""` + `invert_match` + ABSENT cell was DIVERGENT before 75.1 and is
PARITY after — the mode-scoped absence fix closed it as a side effect, because an
absent header now short-circuits before the comparison ever runs. CF-75-1's
remaining divergence is therefore confined to the **PRESENT-value cells, both
polarities** (the middle row above). BANKED, not fixed: the fix encodes a
surprising proto3 degeneracy and should be decided alongside §D by the same future
wire-shape-parity phase.
```

- [ ] **Step 3: Verify no sub-heading letter collides and the phase-72 block is still well-formed**

Run:
```bash
grep -n '^\*\*§[A-Z] ' docs/envoy-rust/BEHAVIOR_CONTRACT.md | sed -n '/24[0-9][0-9]/,/25[0-9][0-9]/p'
grep -c '^### Phase 7[0-9]' docs/envoy-rust/BEHAVIOR_CONTRACT.md
```
Expected: within the phase-72 block the letters run `§A … §G` with **no duplicate letter**; the phase-heading count is unchanged from Task 5 (this task adds no `### Phase` heading).

- [ ] **Step 4: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 75.2: BEHAVIOR_CONTRACT — extend CF-72-2 with contains_match + the HONORED finding; bank CF-75-1"
```

---

### Task 7: M74-31 — correct the causal non-sequitur at all FOUR live sites

**Files:**
- Modify: `tests/differential/tests/access_log_metadata_filter.rs` (~`:30-31`)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (~`:2657`, shifted by Tasks 5/6 — re-derive)
- Modify: `tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml` (~`:36-38`)
- Modify: `tests/fixtures/0081-accesslog-metadata-filter/README.md` (~`:106-108`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: nothing consumed by later tasks. Independent; may be executed in any order relative to Tasks 1–6.

**Context — what the defect actually is.** Four live documents claim that fixture `0081`'s probe 2 is *placed SECOND **so that** the last probe is KEPT and the cheap settle applies*. That "so" is a non-sequitur. `suppression_settle` (`tests/differential/src/lib.rs:1694-1699`) is:

```rust
fn suppression_settle(probes: &[AccessLogByteExactProbe]) -> std::time::Duration {
    match probes.last() {
        Some(p) if !p.expect_logged => CF71_1_SETTLE,
        _ => CF70_3_SETTLE,
    }
}
```

It inspects **only `probes.last()`**. `0081`'s probe order is `[drop, keep, keep]`, so the last probe is KEPT *whichever kept probe sits last* — appending probe 2 instead of inserting it would yield the same cheap settle. **Every OUTCOME asserted at those four sites is TRUE** (kept-LAST does hold; the fixture does pay 2 s); only the causal "so" is wrong. What placement SECOND actually buys is the pinned line ORDER — `M=-` before `M=1`, two byte-DISTINCT lines — which the same documents already state correctly and separately.

**§6.2 correction C4 — this is a FOUR-site problem, not five.** The originating record (`docs/envoy-rust/phases/74-accesslog-metadata-filter/REVIEW.md:1269`) says "now at FIVE sites" and then enumerates four; a repo-wide sweep at this PLAN-write finds exactly four live causal sites. `tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml:20` (*"Probe 2 — KEPT, and placed SECOND (phase 74 §5.2 state-3 re-entry, `REVIEW.md` I-3)"*) is DESCRIPTIVE, not causal, and is **NOT** a site — leave it alone. **Do NOT "fix" the FIVE figure in `74/REVIEW.md`**: landed phase artifacts are append-only (D-3.5).

**Do NOT weaken `0081` while editing it.** It has THREE probes and expects TWO logged lines, byte-distinct so ORDER is pinned. Do not change any probe, any `expect_logged` value, or the probe ORDER — this task edits COMMENTS and PROSE only. And do NOT add an `on_header_missing` block to `0081` or `0082` (ADR-0155 PV-6): their `on_header_missing` mentions are `#` comments documenting a deliberate omission.

- [ ] **Step 1: Re-derive all four sites on the live tree**

Run:
```bash
grep -rn 'placed SECOND' --include=*.rs --include=*.md --include=*.yaml \
  tests/ docs/envoy-rust/BEHAVIOR_CONTRACT.md
```
Expected: four causal hits (in `access_log_metadata_filter.rs`, `BEHAVIOR_CONTRACT.md`, `0081/expectations.yaml`, `0081/README.md`) plus the one descriptive hit at `0081/expectations.yaml:20`. Record the current line numbers.

- [ ] **Step 2: Fix site 1 — `tests/differential/tests/access_log_metadata_filter.rs`**

Current text (a `//!` doc comment):
```rust
//! matches) → KEPT. Probe 2 is placed SECOND rather than appended, so the LAST
//! probe is still KEPT and the driver pays the cheap 2 s `CF70_3_SETTLE`.
```

Replace with:
```rust
//! matches) → KEPT. The LAST probe is KEPT, so the driver's ordering-aware
//! `suppression_settle` — which inspects only `probes.last()` — pays the cheap
//! 2 s `CF70_3_SETTLE` rather than the 12 s `CF71_1_SETTLE`. What placing probe 2
//! SECOND buys is separate: it pins the LINE ORDER (`M=-` before `M=1`).
```

- [ ] **Step 3: Fix site 2 — `docs/envoy-rust/BEHAVIOR_CONTRACT.md`**

Current text (inside the phase-74 block's `**§H Authoritative fixtures.**`):
```markdown
is placed SECOND, not last, so kept-LAST (ADR-0147) holds; the two kept lines are
byte-DISTINCT, so the fixture pins line ORDER as well as count.
```

Replace with:
```markdown
is placed SECOND, not last; kept-LAST (ADR-0147) holds because the LAST probe is
KEPT, which is all `suppression_settle` inspects. What placing it SECOND buys is
that the two kept lines are byte-DISTINCT in a pinned ORDER, so the fixture pins
line ORDER as well as count.
```

- [ ] **Step 4: Fix site 3 — `tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml`**

Current text (a `#` comment block):
```yaml
    # It must stay SECOND, not last: kept-LAST (ADR-0147) is what lets the
    # driver's ordering-aware `suppression_settle` pay the cheap 2 s
    # CF70_3_SETTLE instead of the 12 s CF71_1_SETTLE.
```

Replace with:
```yaml
    # It must stay SECOND, not last — but NOT for settle reasons: the driver's
    # `suppression_settle` inspects only `probes.last()`, and with the order
    # [drop, keep, keep] the last probe is KEPT whichever kept probe sits last,
    # so the cheap 2 s CF70_3_SETTLE applies either way. What placement SECOND
    # buys is the pinned LINE ORDER — `M=-` before `M=1`, byte-DISTINCT — which
    # is what makes this fixture assert order and not merely count.
```

- [ ] **Step 5: Fix site 4 — `tests/fixtures/0081-accesslog-metadata-filter/README.md`**

Current text (in the "Probes / driver" section):
```markdown
convention (ADR-0147): the single DROPPED probe comes first and both KEPT probes
follow, so the driver's ordering-aware `suppression_settle` pays the cheap 2 s
`CF70_3_SETTLE` rather than the 12 s `CF71_1_SETTLE`. That is why probe 2 is
placed SECOND rather than appended last. `expected_logged_count` is therefore
**2**.
```

Replace with:
```markdown
convention (ADR-0147): the single DROPPED probe comes first and both KEPT probes
follow, so the LAST probe is KEPT and the driver's ordering-aware
`suppression_settle` pays the cheap 2 s `CF70_3_SETTLE` rather than the 12 s
`CF71_1_SETTLE`. `suppression_settle` inspects only `probes.last()`, so the cheap
settle would hold with the two kept probes in either order; probe 2 is placed
SECOND for a DIFFERENT reason — it pins the LINE ORDER (`M=-` before `M=1`).
`expected_logged_count` is therefore **2**.
```

- [ ] **Step 6: Verify no causal site survives, and that `0081` is behaviorally untouched**

Run:
```bash
grep -rn 'placed SECOND' --include=*.rs --include=*.md --include=*.yaml \
  tests/ docs/envoy-rust/BEHAVIOR_CONTRACT.md
git diff --stat
git diff tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml | grep -E '^[+-]' | grep -v '^[+-][[:space:]]*#' | grep -v '^[+-][+-]'
```
Expected: the remaining `placed SECOND` hits carry no causal "so"; `git diff --stat` shows exactly the four files this task edits; and the third command prints **NOTHING** — proving every changed line in `0081/expectations.yaml` is a comment, so no probe, no `expect_logged` value and no ordering changed.

**Adjudicate by LINE and by FILE, never by COUNT.** A grep here can legitimately return >0 because a record QUOTES the defect it fixes.

- [ ] **Step 7: Re-run the two fixtures this touches, to prove the edits were comment-only**

```bash
cargo build -p envoy-bin
cargo test -p differential --test access_log_metadata_filter -- --nocapture
```
Expected: `1 passed`. (Only `0081`'s entrypoint is affected; `0082` was not edited.)

- [ ] **Step 8: Commit**

```bash
git add tests/differential/tests/access_log_metadata_filter.rs \
        docs/envoy-rust/BEHAVIOR_CONTRACT.md \
        tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml \
        tests/fixtures/0081-accesslog-metadata-filter/README.md
git commit -m "phase 75.2: CONSUME M74-31 — drop the kept-LAST causal non-sequitur at all four live sites"
```

---

### Task 8: The sub-phase-75.1 review's open findings — M-1, M-2, M-3, N-1, N-2

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (M-1 at ~`:2408`; M-2 at ~`:1884`; M-3 at ~`:2545`; N-2 at ~`:1887`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (~`:1706-1708`) — **doc comment only**
- Modify: `crates/envoy-config/src/matcher.rs` (~`:348-350`) — **test-module comment only**
- Modify: `tests/fixtures/0081-accesslog-metadata-filter/README.md` (~`:100`)

**Interfaces:**
- Consumes: nothing from earlier tasks. Independent.
- Produces: nothing consumed by later tasks.

**Context.** These are five cheap, co-located live-document accuracy fixes carried forward from `docs/envoy-rust/phases/75.1-headermatcher-absence-engine-route/REVIEW.md` (verdict APPROVED — 0 Critical, 0 Important). **All five citations were RE-VERIFIED on the live tree at this PLAN-write and every one HOLDS**, but line numbers shift as blocks are inserted above them (that is precisely how M-1 went stale inside the phase chartered to fix it) — and Tasks 5/6 insert ~140 lines into `BEHAVIOR_CONTRACT.md` ABOVE the `:2408` and `:2545` sites. **Re-derive every site by text anchor before editing.**

Two findings from the same review need NO fix and appear nowhere in this plan: **M-5** (three `PROGRESS.md` blocks presented as verbatim that were transcribed) and **N-3** (commit-message imprecision) — both are landed historical artifacts whose retroactive editing would be worse than the imprecision (D-3.5). **N-4** is a coverage note, record only.

- [ ] **Step 1: Re-derive all six sites by text anchor**

```bash
grep -rn 'matcher\.rs:52' docs/envoy-rust/BEHAVIOR_CONTRACT.md
grep -rn 'DIFFER when it is ABSENT' docs/envoy-rust/BEHAVIOR_CONTRACT.md crates/envoy-config/src/bootstrap.rs
grep -rn 'See §C for the' docs/envoy-rust/BEHAVIOR_CONTRACT.md
grep -rn 'divergence is mode-scoped' docs/envoy-rust/BEHAVIOR_CONTRACT.md tests/fixtures/0081-accesslog-metadata-filter/README.md
grep -rn 'flipped the two' crates/envoy-config/src/matcher.rs
```
Expected: exactly one hit each for M-1, N-2 and N-1; two hits each for M-2 and M-3. Record the current line numbers.

- [ ] **Step 2: M-1 — make the stale `matcher.rs:52` citation LINE-NUMBER-FREE**

In `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, current text:
```markdown
The engine is `HeaderMatcher::matches` (the XOR is at `matcher.rs:52`), shared
```
Replace with:
```markdown
The engine is `HeaderMatcher::matches` (the XOR that closes the function), shared
```

**Why line-number-free rather than re-pointed at `:69`:** this citation class has gone stale three times (`:51` → `:52` → `:69`), twice inside the phase chartered to fix it, because any doc block inserted above `pub fn matches` moves it. A prose anchor cannot drift.

**Do NOT touch `crates/envoy-config/src/matcher.rs:471`**, which also says `matcher.rs:52`. That one is PAST TENSE (*"Until phase 75.1 the shared engine (matcher.rs:52) applied…"*) and is therefore a correct historical reference; the 75.1 review adjudicated it explicitly as **not** a finding.

- [ ] **Step 3: M-2 — qualify the over-broad restatement, at BOTH sites**

The same sentence appears in `docs/envoy-rust/BEHAVIOR_CONTRACT.md` and, mirrored, as a doc comment in `crates/envoy-config/src/bootstrap.rs`.

`BEHAVIOR_CONTRACT.md`, current:
```markdown
rule is **`(present == want)`** since phase 75.1 (ADR-0159; the parenthetical
`want ? present : true` recorded here before 75.1 described the pre-75.1 in-tree
behavior, which was divergence D2 and has been fixed). The two now **AGREE when
the key/header is PRESENT** and still **DIFFER when it is ABSENT** —
`ValueMatcher` → `false`, `HeaderMatcher` → `true`. They remain different fields
on different messages: do NOT unify them, and do not "fix" the `ValueMatcher`
rule to match. See §C for the `HeaderMatcher` rule in full.
```

Replace with (this ALSO lands N-2's disambiguation in the final sentence):
```markdown
rule is **`(present == want)`** since phase 75.1 (ADR-0159; the parenthetical
`want ? present : true` recorded here before 75.1 described the pre-75.1 in-tree
behavior, which was divergence D2 and has been fixed). The two now differ in
**exactly ONE of four cells** — ABSENT × `present_match: false`, where
`ValueMatcher` → `false` and `HeaderMatcher` → `true`. They AGREE in the other
three, including ABSENT × `present_match: true` (both → `false`). They remain
different fields on different messages: do NOT unify them, and do not "fix" the
`ValueMatcher` rule to match. See the **Phase 75** block for the `HeaderMatcher`
rule in full, and its §E for this four-cell table.
```

`crates/envoy-config/src/bootstrap.rs`, current doc comment:
```rust
    /// PRESENT and still DIFFER when it is ABSENT (`ValueMatcher` → false,
    /// `HeaderMatcher` → true). Do not unify them.
```

Replace with (adjust the leading prose line above it if the sentence starts on the preceding line — read the surrounding 8 lines first):
```rust
    /// PRESENT. They differ in exactly ONE of four cells — ABSENT with
    /// `present_match: false`, where `ValueMatcher` → false and `HeaderMatcher`
    /// → true; ABSENT with `present_match: true` AGREES (both → false). Do not
    /// unify them.
```

**The load-bearing "do NOT unify them" instruction is CORRECT and must be KEPT** — the review's severity finding was that a reader is led to the right *action* by imprecise *reasoning*. Fix the reasoning; keep the action.

- [ ] **Step 4: M-3 — re-tense the stale divergence claim, at BOTH sites**

The identical sentence appears in `docs/envoy-rust/BEHAVIOR_CONTRACT.md` and `tests/fixtures/0081-accesslog-metadata-filter/README.md`. Current:
```
Note this is a DIFFERENT field on a DIFFERENT message from
`HeaderMatcher.invert_match` (CF-72-1), whose divergence is mode-scoped.
```

Replace at BOTH sites with:
```
Note this is a DIFFERENT field on a DIFFERENT message from
`HeaderMatcher.invert_match` (CF-72-1), whose divergence *was* mode-scoped and is
CLOSED by sub-phase 75.1.
```

(The `0081/README.md` copy is inside a `>` blockquote — preserve the `> ` prefix on each line.)

**The surrounding CF-74-1 conflation warning is the POINT of the sentence and must be KEPT.** Only the trailing clause is re-tensed.

- [ ] **Step 5: N-1 — fix the "two" over-count**

In `crates/envoy-config/src/matcher.rs` (a comment inside the `#[cfg(test)]` module), current:
```rust
    // PresentMatch: 4 cells (true × present, true × absent, false × present,
    // false × absent). Phase 75.1 flipped the two `false ×` expectations: the
    // measured rule is `(present == want)`, not "false ⇒ always true".
```

Replace with:
```rust
    // PresentMatch: 4 cells (true × present, true × absent, false × present,
    // false × absent). Phase 75.1 flipped exactly ONE expectation —
    // `false × present`, which went true → false. `false × absent` keeps its
    // verdict (true), now for the right reason: the measured rule is
    // `(present == want)`, not "false ⇒ always true".
```

Verification of the arithmetic, from the landed engine: pre-75.1 `PresentMatch(want)` was `if want { v.is_some() } else { true }`; post-75.1 it is `v.is_some() == want`. Cell by cell — `true × present`: `true` → `true` (unchanged); `true × absent`: `false` → `false` (unchanged); `false × present`: `true` → **`false`** (FLIPPED); `false × absent`: `true` → `true` (unchanged). **One flip, not two.** The test's own body comment 20 lines below already says so ("Right answer, and after phase 75.1 for the right reason"), which is why the block comment contradicted the test it introduces.

- [ ] **Step 6: N-2 — confirm the "See §C" disambiguation landed**

N-2's fix was folded into Step 3's replacement text (*"See the **Phase 75** block … and its §E"*). Confirm no bare ambiguous reference remains:
```bash
grep -n 'See §C' docs/envoy-rust/BEHAVIOR_CONTRACT.md
grep -c '^\*\*§C ' docs/envoy-rust/BEHAVIOR_CONTRACT.md
```
Expected: the first command prints NOTHING. The second prints **8** — unchanged; this task removes an ambiguous *reference*, not any `§C` heading.

- [ ] **Step 7: Verify the two `crates/` edits are comment-only**

```bash
git diff crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/matcher.rs \
  | grep -E '^[+-]' | grep -v '^[+-][+-]' | grep -vE '^[+-][[:space:]]*(///|//)'
```
Expected: **NOTHING**. If this prints any line, an executable statement was changed — revert it. This sub-phase must not alter `crates/` behavior.

Then:
```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test -p envoy-config --lib
```
Expected: build and clippy clean; fmt silent; `envoy-config` lib tests all pass with the same count as before the edit. Note `cargo fmt` does **NOT** reflow doc comments, so a too-long comment line will not be auto-fixed — keep lines within the surrounding file's width.

- [ ] **Step 8: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md \
        crates/envoy-config/src/bootstrap.rs \
        crates/envoy-config/src/matcher.rs \
        tests/fixtures/0081-accesslog-metadata-filter/README.md
git commit -m "phase 75.2: close 75.1 review findings M-1, M-2, M-3, N-1, N-2 (live-document accuracy; comment-only in crates/)"
```

---

### Task 9: `PROGRESS.md` — the running implementation log

**Files:**
- Create: `docs/envoy-rust/phases/75.2-headermatcher-absence-accesslog/PROGRESS.md`

**Interfaces:**
- Consumes: the outcome of every preceding task.
- Produces: the artifact the §5 state-4 verification gate and the state-5 code review both read. Its presence alongside `PLAN.md` is also part of the state-3 → state-4 detection.

**Context.** `PROGRESS.md` is appended to **as each task completes**, not written at the end — the point is a contemporaneous log. Model it on `docs/envoy-rust/phases/75.1-headermatcher-absence-engine-route/PROGRESS.md`.

**Quote command output VERBATIM, freshly captured.** The 75.1 review's finding M-5 was that three blocks presented as verbatim had been transcribed from an earlier tree rather than re-run — a uniform 19-line offset gave it away. Re-run and paste; do not retype from memory or reuse an older capture.

- [ ] **Step 1: After each task's commit, append a section**

Each section records: the task number and title; the files created/modified with `wc -l`; the exact commands run and their VERBATIM output (including the `N passed` figures); for Tasks 1 and 3, the mutation-check result with BOTH the mutated RED failure text AND the unmutated GREEN control from the same worktree, plus the `Compiling envoy-config` evidence of a forced rebuild; the commit SHA; and any deviation from this plan with its reason.

- [ ] **Step 2: Record the census figures at completion**

```bash
ls -d tests/fixtures/[0-9]* | wc -l                    # expect 85
ls tests/differential/tests/*.rs | wc -l               # expect 85
wc -l tests/conformance/h2spec/known-failures.txt      # expect 21 — NEVER trimmed
git ls-files | grep 'parse_bootstrap' | grep -c corpus # expect 63 — unchanged
grep -rc '#!\[forbid(unsafe_code)\]' $(git ls-files 'crates/*/src/lib.rs' 'crates/*/src/main.rs') | grep -c ':1'   # expect 17
git diff --stat main...HEAD | tail -1                  # the net LoC, for the §6.1 retrospective
```

Record the final net-LoC figure against this plan's **~760** projection. Sub-phase 75.1 projected ~1210 and landed 1457 (+20%); if 75.2 overshoots by a similar margin it lands near ~910, still far under the ~1500 gate.

- [ ] **Step 3: Commit `PROGRESS.md` alongside the final task**

```bash
git add docs/envoy-rust/phases/75.2-headermatcher-absence-accesslog/PROGRESS.md
git commit -m "phase 75.2: PROGRESS.md — implementation log"
```

---

## The §7.5 phase-done gate (run at state-4, NOT during implementation)

Recorded here so the state-4 session knows exactly what to run. **State-4 is SOLO-SERIAL** — the cargo lock makes concurrency pointless and the adjudication is one indivisible judgment.

| gate | command / criterion |
|---|---|
| (a) new fixtures green | `cargo test -p differential --test headermatcher_absence_accesslog` and `--test headermatcher_absence_accesslog_present_polarity` — assert `1 passed` each |
| (b) pre-existing fixtures still green | the full `cargo test --workspace` sweep below; watch `0078`/`0079`/`0080`/`0081`/`0082` (the access-log filter family) and `0083` in particular |
| (c) conformance | unchanged; h2spec stays at its declared threshold and `known-failures.txt` stays **21** lines |
| (d) fuzz | **no new target** — nothing new to run (re-confirmed, see §7.4 above) |
| (e) build / clippy / fmt / test / deny | `cargo build --workspace --all-targets`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo fmt --all -- --check`; `cargo test --workspace`; `cargo deny check` |
| (f) review | `REVIEW.md` approved (state 5) |

**Adjudication discipline for gate (b):**

- Run `cargo test --workspace --no-fail-fast` and **redirect the FULL output to a file — never pipe through `tail`**, which truncates the `failures:` block and destroys the names the gate must adjudicate. A bare `cargo test --workspace` aborts at the first failing BINARY.
- Run it **2–3 times and diff the failing SET**. The startup-race flake family's membership changes run to run.
- Re-run each failing member **in ISOLATION, naming its target binary**. `0 passed; N filtered out` is **NOT** a pass, and `error: no test target named …` exits 101 exactly like a real RED. **Read the failure TEXT.**
- Cross-check `local passed + failed == CI passed`.

**The documented host-flake set is CI-AUTHORITATIVE and is NOT a regression:** `eds_cluster_with_neither_is_fatal`, `no_rds_is_inert`, `happy_reload_flips_endpoint_and_ticks_counters`, `happy_path_dynamic_cluster_serves_and_reports`, `plaintext_rbac_before_tcp_proxy_delivers_banner_to_a_byteless_client`, `wait_accept_ready_times_out_for_closed_socket`, `access_log_rf_retry_exhausted`, `upstream_h2_connection_pooling`, `network_filter_direct_response_fixture`, `network_filter_rbac_allow_fixture`, `upstream_active_health_check_fixture`, `upstream_circuit_breaker_budgets_fixture`, `send_request_maps_h2_handshake_failure_to_typed_error`, the `TcpCloseBackend` IPv6-unreachable set (fixtures `0061`/`0062`/`0069` plus the four `access_log_*_upstream_reset` binaries), and `admin_config_dump_server_info` (the `192.168.65.2` bridge-IP family).

The IPv6 and bridge-IP families fail **DETERMINISTICALLY in isolation — that determinism IS the environmental signature, not a regression.** The startup-race family passes in isolation and its membership varies, so diff the failing SET across runs rather than trusting one run's set.

`cargo deny check` can red on a freshly-published RustSec advisory against an EXISTING dependency; patch-bump it (`cargo update -p <crate> --precise <version>`) rather than treating it as a phase regression.

---

## Carry-forward disposition after this sub-phase

**CONSUMED:** **M74-31** — corrected in place at all four live sites (Task 7), rather than propagated into two more kept-LAST fixtures. The sub-phase-75.1 review findings **M-1, M-2, M-3, N-1, N-2** are CLOSED (Task 8).

**BANKED in `BEHAVIOR_CONTRACT.md`, not fixed:** **CF-72-2** (extended to three members — name-only `{ name }`; `treat_missing_header_as_empty` accepted AND HONORED upstream; the top-level `contains_match` arm) and **CF-75-1** (`exact_match: ""` degenerates to a PRESENCE match upstream; remaining divergence confined to the PRESENT-value cells). Owner for both: a future `HeaderMatcher` wire-shape-parity phase, which should decide them together.

**No fix, by design:** **M-5** and **N-3** (landed historical artifacts, D-3.5); **N-4** (coverage note, record only).

**Untouched and carried forward:** **CF-75-2** (upstream comma-joins duplicate header values before value matching; envoy-rust matches only the first occurrence — MEASURED, PRE-EXISTING, affects all six value modes across all five consumers, needs its own phase), **M71-6** (the H2 access-log-filter differential — DECLINED at this PLAN-write, see "Judgement calls"), CF-74-1/2/3/4/6, CF-73-1, N73-R2, M73-R1/M73-R2, M71-3, M71-7/8, M70-R4/R9, M69-A..I, CF-69-1/2/3/5, M68-1, M-1, CF-67-3/5/6/7, M74-3..M74-14, M74-16, M74-17/18/20/21/22/26/27/28/29, M74-30 and M74-32..M74-39, the older Minors in `67.3/SPEC.md` §10, and the HTTP-filters-family (1)–(4) in `STATE_HISTORY.md`.

**Already closed before this sub-phase:** CF-72-1 (consumed at the 75.1 close-out), CF-74-5.

---

## What happens after this plan

Per `BOOTSTRAP_PROMPT.md` §5.1, **one state per session; do not chain**. The remaining path for sub-phase 75.2:

- **state-3** — implementation of this plan (`superpowers:executing-plans` or `superpowers:subagent-driven-development`), appending to `PROGRESS.md` per task.
- **state-4** — the §7.5 verification gate above (`superpowers:verification-before-completion`), SOLO-SERIAL.
- **state-5** — code review (`superpowers:requesting-code-review`) → `REVIEW.md`.
- **state-6** — close-out: ROADMAP row `75.2` → `done`, **and parent row `75` → `done` as well**, because 75.2 is the LAST sub-phase. That parent flip belongs to 75.2's close-out and to no earlier session.

After parent `75` flips, **103 of 104** ROADMAP rows are `done`. The mission is still NOT complete — the `ROADMAP.md` `## Feature Families` (`ROADMAP.md:58`) remain largely unbuilt (network-filter payload codecs, `sni_cluster`, non-deterministic LB, HTTP/3 + QUIC, gRPC bridge/transcoding, observability SINKS, runtime/RTDS, hot-restart, WASM host), and the carry-forward set above is live.
