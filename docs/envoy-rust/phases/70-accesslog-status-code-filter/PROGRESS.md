# Phase 70 — `status_code_filter` — §5 state-3 implementation PROGRESS

> **Status:** §5 state-3 implementation **COMPLETE**. All 14 `PLAN.md` TDD tasks landed
> (16 commits: 14 task commits + 2 review-fix commits), each RED→GREEN→commit per doctrine
> D-3.1. Every per-task review is clean and the final whole-branch review returned
> **READY TO MERGE (0 Critical / 0 Important / 3 Minor)**.
>
> This file is written for a stranger with zero prior context (D-3.4). The next session is
> the **SEPARATE §5 state-4 verification** (`superpowers:verification-before-completion`) —
> the §7.5 gate re-run below was a PLAN Task-14 *dry-run* and is explicitly **NOT** the
> state-4 verdict.

---

## §1. What this phase built (one paragraph for a stranger)

Until phase 70, an envoy-rust access-log sink logged **every** request record. Upstream Envoy
lets an `AccessLog` entry carry a **filter** — a predicate deciding *whether a record is
emitted at all*. This phase OPENS that subsystem with its single canonical variant,
`status_code_filter`: a comparison of the **final response code** against a threshold using
`op ∈ {EQ, GE, LE}`. Concretely, `GE 500` keeps a 503 and **drops** a 200. The filter is a
config field (`AccessLog.filter`) compiled at HCM config-load time into a runtime predicate
carried on each sink, then consulted in both the HTTP/1.1 and HTTP/2 emit loops. A sink with
**no** filter behaves exactly as before — that regression parity is load-bearing, because 29
pre-existing access-log fixtures depend on it.

**Architecture as landed** (config → runtime, mirroring the existing `compiled_log_format`
posture so the emitter crate never depends on the config crate):

| Layer | File | What |
|---|---|---|
| Config schema | `crates/envoy-config/src/bootstrap.rs` | `AccessLog.filter: Option<AccessLogFilter>`; `AccessLogFilter{status_code_filter}`; `StatusCodeFilter{comparison}`; `ComparisonFilter{op,value}`; `ComparisonOp{Eq,Ge,Le}`; `RuntimeUInt32{default_value,runtime_key}` |
| Validation | `crates/envoy-config/src/bootstrap.rs` (`validate_access_logs`) + `src/lib.rs` | `ConfigError::AmbiguousAccessLogFilter`, `ConfigError::EmptyStatusCodeFilterRuntimeKey` |
| Runtime predicate | `crates/envoy-accesslog/src/filter.rs` (NEW) | `LogFilter::should_log(status: u16) -> bool` |
| Sink | `crates/envoy-accesslog/src/file_sink.rs` | `FileSink` carries `Option<LogFilter>`; `should_log` returns `true` when `None` |
| Compile + H1 gate | `crates/envoy-http1/src/hcm.rs` | `compile_access_log_filter` at `HCMConfig::from_config`; gated emit loop |
| H2 gate | `crates/envoy-http2/src/hcm.rs` | the mirror gate (inert today — no H2 fixture sets a filter) |
| Differential | `tests/differential/src/lib.rs` | `expect_logged: bool` (serde default `true`) + `expected_logged_count` |
| Differential proof | `tests/fixtures/0076-accesslog-status-code-filter/` | GREEN |

**Net change:** 16 files, +1462 / −88 (vs. the SPEC §8 / ADR-0141 estimate of ~670 net LoC —
the overage is test/doc mass, not scope; see §5).

---

## §2. The measured facts this rests on

All measured against the pinned reference `envoyproxy/envoy:v1.33.0` (D-3.7) — nothing here
is asserted from memory or from reading Envoy source (D-3.3).

- `ComparisonFilter.op` is **exactly** `{EQ, GE, LE}`; `NE` and bogus tokens are REJECTED.
- `runtime_key` is PGV-mandatory (`min_len 1`; upstream REJECTS empty) but **RTDS-inert**
  here: envoy-rust has no runtime subsystem, so the comparison **always** uses
  `default_value`. The validator still requires a non-empty key, for load-time parity.
- `direct_response` responses **ARE** access-logged, so the differential needs **no backend
  and no cluster**. A `direct_response` 503 carries `%RESPONSE_FLAGS% = -`.
- The state-0 live recon of exactly the fixture-0076 config produced a file containing
  **exactly one line**: `STATUS=503 PATH=/log FLAGS=-`.

---

## §3. Task-by-task record (RED → GREEN → commit)

Execution used `superpowers:subagent-driven-development`: a fresh implementer subagent per
task-front in an isolated git worktree, each doing full TDD; the main session integrated
(cherry-pick), ran every workspace-global step itself, and is the sole writer of this file
and the ledger. Independent fronts ran in parallel (T1–T3 `envoy-config` ‖ T4–T5
`envoy-accesslog`; then T6–T7 `envoy-http1` ‖ T8 `envoy-http2`).

### Task 1 — config schema (`8ce57be`)
- **RED:** `cargo test -p envoy-config parses_status_code_filter_ge_500` → unknown field
  `filter` under `deny_unknown_fields` / `ComparisonOp` absent.
- **GREEN:** both `parses_status_code_filter_ge_500` and
  `rejects_status_code_filter_unknown_op` pass. `NE` is rejected as `ConfigError::Yaml`
  (serde has no catch-all → parity with upstream's unknown-enum rejection).
- **Deviation from PLAN (deliberate):** PLAN mandated a hand-written `impl Default for
  AccessLogFilter`; that is precisely what `clippy::derivable_impls` flags, and the §7.5 gate
  runs `clippy … -D warnings` — so the PLAN's own code would have failed the PLAN's own
  Global Constraints. Resolved to `#[derive(Default)]` (behaviorally identical) per D-3.5
  (resolve ambiguity and proceed; no human mid-phase).
- **Review:** spec ✅. `validate_access_logs` independently confirmed **live** on the real
  `parse_bootstrap` path (`bootstrap.rs:3795`), not a dead function. Both rejection tests
  confirmed mutation-sensitive.

### Task 2 — oneof cardinality validator (`5dd0cdb`)
- **RED:** `filter: {}` parsed successfully (all arms `Option`, default `None`) and was not
  rejected; `AmbiguousAccessLogFilter` did not exist.
- **GREEN:** `rejects_access_log_filter_with_no_variant` passes.
- Models the in-tree `SubstitutionFormatString` / `AmbiguousLogFormat` precedent: `Option`
  arms + an **external** cardinality validator, **not** an `@type`-tagged enum (ADR-0141 PV-1).

### Task 3 — empty `runtime_key` validator (`b54cdcc`)
- **RED:** empty `runtime_key` parsed and was not rejected; the variant did not exist.
- **GREEN:** `rejects_status_code_filter_empty_runtime_key` passes.
- **Deviation from PLAN (deliberate):** PLAN's nested `if let` + `if` trips
  `clippy::collapsible_if` (same failure class as T1). Collapsed to a let-chain —
  behaviorally identical.

### Task 4 — `LogFilter` runtime predicate (`906578b`)
- **RED:** `cargo test -p envoy-accesslog filter::` → module `filter` does not exist.
- **GREEN:** all three boundary tests pass (GE 500 at 499/500/503; EQ 404 at 403/404/405;
  LE 200 at 200/201/100).
- **Review:** every single-operator mutation is caught by the tests (Ge↔Le, Eq↔Ge, Eq↔Le,
  `>=`→`>`, `<=`→`<` each break a specific assertion). Widening confirmed `status as u32`
  (lossless), never a narrowing cast of `default_value`. `envoy-accesslog/Cargo.toml`
  confirmed to carry **no** `envoy-config` dependency.

### Task 5 — `FileSink` carries `Option<LogFilter>` (`3f3730b`)
- **RED:** `FileSink::new` took 2 args; `should_log` did not exist.
- **GREEN:** `cargo test -p envoy-accesslog` → **102 passed / 0 failed**.
- **Deviation from PLAN (deliberate):** PLAN told this task to fix *every* caller including
  out-of-crate ones. That would have violated the crate-boundary rule, so the implementer was
  scoped to its own crate and instead **enumerated** the out-of-crate call sites for the
  integrating tasks. The workspace was therefore knowingly red between T5 and T8 — see §4.
- **Review:** Approved (0C/0I). Call-site enumeration independently re-grepped and confirmed
  complete.

### Task 6 — compile config→runtime at `HCMConfig::from_config` (`def5ccb`)
- **RED:** `FileSink::new` still received `None`, so `should_log(200)` was `true`.
- **GREEN:** `from_config_compiles_status_code_filter_into_sink` passes, routed through the
  **production** `from_config` (not a hand-built literal).
- **Review:** the `ComparisonOp`→`FilterOp` translation is **exhaustive and not swapped**
  (Eq→Eq, Ge→Ge, Le→Le) and carries **no `_` wildcard** — so a future oneof arm is a compile
  error rather than a silent fallthrough. `threshold` confirmed taken from `default_value`
  (the RTDS-inert requirement).

### Task 7 — H1 emit gate + counter (`867c45b`, fix `8fdae31`)
- **RED:** the sink logged the 200 (no gate), so the file had 1 line, not 0.
- **GREEN:** `cargo test -p envoy-http1` → **171 passed / 0 failed**.
- The `access_logs_total` counter moved from a pre-loop `add(config.access_log.len())` to a
  per-emit `inc()` **inside** the gated branch, so suppressed sinks do not over-count.
- **Review finding (Important) — FIXED.** The reviewer proved the counter half was pinned by
  **no test**: reverting `inc()` to the pre-loop `add(len)` while keeping the gate left all
  170 tests green (the line-count test pins the gate, not the counter; the existing counter
  test used a single unfiltered sink where `add(1) ≡ 1×inc()`). The H2 sibling already pinned
  exactly this. Fix `8fdae31` added the H1 analogue
  `h1_filtered_sink_suppresses_access_logs_total`, with the RED **proven** by reverting the
  counter: `assertion left == right failed: a suppressed sink must not tick
  access_logs_total — left: 1 / right: 0`, and the run grepped for `Compiling envoy-http1` to
  rule out a stale-binary false pass.
- Two doc defects also fixed in `8fdae31`: `compiled_log_format`'s rustdoc had been
  inadvertently stolen by the newly-inserted function, and the `pub` field docs still
  described the removed bulk-`add` behavior as current.
- **Counter parity verified:** `inc()` still precedes the `emit` await, so an emission failure
  cannot deflate the total (`access_logs_failed` is separate); for unfiltered sinks
  `N×inc()` reaches an identical value to `add(N)` (both `fetch_add`/`Relaxed`). All 58
  access-log fixture files are **single-sink**, so `N > 1` never arises in practice.

### Task 8 — H2 emit gate (`4e88912`)
- **RED:** the H2 loop emitted unconditionally.
- **GREEN:** `cargo test -p envoy-http2` → **102 passed / 0 failed / 1 ignored** (the
  `0064`–`0070` in-process access-log tests did not regress).
- **Review:** Approved (0C/0I). A faithful mirror of the H1 gate (only the
  `config.inner.access_log` access path differs). Its test pins **both** halves (line count
  **and** `stats.value()` 0→1). Statically confirmed that **no fixture anywhere** sets
  `status_code_filter`, so the H2 gate is **provably inert** for `0064`–`0070`.

### Task 9 — differential `expect_logged` extension (`9a8ecdb`, fix `567a7fc`)
- **RED:** `expect_logged` field and `expected_logged_count` did not exist (E0560/E0425).
- **GREEN:** `expected_logged_count_excludes_suppressed` passes; `--tests` builds clean; all
  **75** fixture `expectations.yaml` still deserialize (empirical sweep, not inspection).
- The `expected_lines` binding change is **mandatory, not cosmetic**: with a stale
  `probes.len()`, `wait_file_lines` would never reach its target and would burn the full 15s
  `ACCESS_LOG_FLUSH_WAIT` on every suppressed-probe run. The review **confirmed** (rather
  than assumed) that the single binding feeds all four downstream sites per arm — H1
  `6258`→`6311`/`6328`/`6355`/`6363`, H2 `6401`→`6441`/`6451`/`6476`/`6484` — with no stale
  `probes.len()` surviving in either arm, and that
  `assert_access_log_lines_byte_identical` is genuinely probe-agnostic (`&[String]` vs
  `&[String]`, zero probe knowledge) and so correctly needed no change.
- **Review finding (Important) — FIXED.** `expect_logged`'s serde default `true` is the sole
  reason the 28 pre-existing byte-exact fixtures still deserialize — yet it was guarded by
  **no test** (the passing test built its probes with a struct literal, bypassing serde
  entirely). Fix `567a7fc` added `byte_exact_probe_expect_logged_defaults_true`, which
  deserializes real YAML, with the RED **proven both ways**: forcing
  `default_expect_logged()` → `false` FAILED, and removing the `#[serde(default = …)]`
  attribute FAILED with `missing field \`expect_logged\``; each run grepped for `Compiling`.

### Task 10 — differential fixture `0076` (`4c6e4bb`) — **GREEN**
- An H1 HCM, a filtered file access log (`GE 500`), two `direct_response` routes
  (`/log`→503, `/nolog`→200), **no backend / no cluster**.
- **GREEN:** `access_log_status_code_filter` passes; both proxies' log files are
  **md5-identical**, containing exactly one line `STATUS=503 PATH=/log FLAGS=-` — matching
  the state-0 recon **byte-for-byte**. It also passed inside the full workspace run (§4).
- **Non-vacuity PROVEN by mutation:** the fixture passed on first run (the implementation
  pre-landed), so a green run alone proved nothing. Mutating `should_log` to always-true made
  the fixture **RED**, with envoy-rust emitting the extra `STATUS=200 PATH=/nolog` line;
  forced-rebuild verified; mutation reverted.
- **Config-divergence adjudicated:** `envoy.yaml` / `envoy-rust.yaml` are not byte-identical,
  but the 4-hunk delta (admin block, bind address, `generate_request_id`, per-proxy log path)
  is **hunk-for-hunk identical to the established `0040` precedent**, and the `filter` block
  itself is byte-identical across both proxies. This is in-tree convention, not an invented
  divergence — **no ADR is owed**.
- Auto-registered (`tests/differential/Cargo.toml` has no `[[test]]`/`autotests` keys, so it
  mirrors its 28 neighbors).

### Task 11 — RTDS-inert + no-filter regression coverage (`a8e25d2`)
- Both tests pin already-correct behavior, so a green run proves nothing by itself — **both
  were mutation-killed and restored**, with `Compiling` confirmed to rule out a stale-binary
  false pass.
- **Deviation from PLAN (deliberate, reviewed):** PLAN placed these in `envoy-accesslog` /
  `envoy-config`. **Neither crate can host them** — `envoy-accesslog` has no `envoy-config`
  dependency (by design), and `envoy-config` cannot dev-depend on `envoy-http1` (where
  `compile_access_log_filter` lives) without a cycle. Writing them in either would have
  pinned a *duplicated copy* of the compile step rather than the production path. They live
  in `crates/envoy-http1/src/hcm.rs` and drive the real
  `parse_bootstrap` → `compile_access_log_filter` path.

### Task 12 — `parse_bootstrap` fuzz corpus seed (`af9a13c`)
- §7.4 disposition (ADR-0141 PV-5): a corpus **seed** only — **no new fuzz target**, and
  therefore **no `ci.yml` step is owed** (a new target would need hand-wiring; verified
  `.github/` diff is empty).
- **The gitignore trap was cleared:** the corpus dir is `*`-ignored, so a seed is silently
  untracked and invisible to CI without an explicit `!`-un-ignore line. Verified the only way
  that actually proves it — `git ls-files` **prints** the seed path, and `git check-ignore`
  exits 1.
- The seed was additionally confirmed to be a **genuinely valid** bootstrap
  (`SEED OK: op=Ge default_value=500`); an unparseable seed would be near-useless.

### Task 13 — `BEHAVIOR_CONTRACT.md` (`cdb1d8c`, fixes `567a7fc` + `9537a0d`)
- A `status_code_filter` subsection (§A–§G) under the access-log section, recording the §2
  measured facts.
- **Review finding (Minor) — FIXED.** The "27 pre-existing fixtures" figure was **wrong** and
  had landed in the file doctrine treats as canonical. Independently re-verified **by driver
  `kind`** across every `expectations.yaml`: **29** pre-phase-70 access-log-driver fixtures
  (`0012`, `0040`, `0041`, `0042`, `0046`–`0070`), of which **28** are byte-exact (`0012` is
  the lone `http1_with_access_log`).
- **Final-review findings (Minor ×2) — FIXED in `9537a0d`** (see §5).

### Task 14 — §7.5 gate dry-run
Run by the main session; results in §4. **This is a dry-run, not the state-4 verdict.**

---

## §4. §7.5 gate dry-run (PLAN Task 14) — main-session run

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **CLEAN** (exit 0) |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **CLEAN** (exit 0, zero warnings) |
| `cargo build --workspace --all-targets` | **CLEAN** (exit 0) |
| `cargo deny check` | **CLEAN** (exit 0) |
| `cargo test --workspace --no-fail-fast` | **2016 passed / 5 failed / 9 ignored** |

**The new fixture `0076` PASSED inside the full workspace run** (`test
access_log_status_code_filter ... ok`).

### All 5 failures adjudicated ENVIRONMENTAL — none in the phase-70 surface, none sets a filter

Adjudicated with `--no-fail-fast` and a full-output redirect (never piped through `tail`,
which would truncate the `failures:` block and destroy the failing test names).

1. **`access_log_rcd_upstream_reset`, `access_log_rf_upstream_reset`,
   `access_log_h2_rcd_upstream_reset`, `access_log_h2_uc_upstream_reset`** — the documented
   IPv6-unreachable host flake. **Evidence (self-evidently environmental):** upstream Envoy
   logs `rcd="upstream_reset_before_response_started{remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:34523}" rf="UF"`
   where envoy-rust logs `{connection_termination}` / `"UC"` — i.e. real Envoy cannot reach
   the host-spawned close backend over IPv6 and reports a connect failure instead of a reset.
2. **`admin_config_dump_server_info`** — the documented Docker bridge-IP flake. **Evidence:**
   envoy-only stats `backend::192.168.65.2:40835::*` (this host routes the backend via
   `192.168.65.2`, not the allow-listed address).

Both families are pre-existing, documented, and **CI-authoritative** — never to be treated as
regressions, and never to be "fixed" by weakening a fixture. This set is a strict **subset**
of the phase-69 state-3 sweep's documented flakes (which also hit `lb_ring_hash_fixture` and
`upstream_connection_pooling`), consistent with their known non-determinism.

---

## §5. Reviews

Every task front received an independent task review (spec compliance + code quality) from a
fresh subagent with no implementation context (ADR-0127: the context that wrote an artifact
must not grade it). All spec verdicts: ✅.

**Final whole-branch review (`b362bae..567a7fc`, 15 commits): READY TO MERGE — 0 Critical /
0 Important / 3 Minor.** The reviewer went beyond the diff and ran fresh networking-free
`--mode validate` probes against the pinned Envoy to test acceptance-class claims the state-0
recon had not covered, and independently traced the **LDS path** — a cross-cutting exposure no
task-scoped review could see (see CF-70-1 below).

Findings and disposition:

- **Minor 1 — acceptance-class boundary (documented, `9537a0d`).** MEASURED: upstream
  **accepts** `op` omitted (proto3 implicit default → `EQ`), `default_value` omitted (→ `0`),
  and a numeric enum token (`op: 1` → `GE`); envoy-rust **rejects** all three, because it
  models these proto3 scalars as serde-**required** and no in-tree enum accepts numeric
  tokens. The divergence is **fail-loud, never silent** (envoy-rust refuses to boot; runtime
  behavior never differs) and is **consistent with the tree-wide posture**
  (`FractionalPercent.numerator`, `TokenBucket.max_tokens` are likewise serde-required).
  Recorded as a boundary note in `BEHAVIOR_CONTRACT.md` §E.1 — **no code change**, since
  altering the config surface is out of scope for this phase.
- **Minor 3 — contract inaccuracy (FIXED, `9537a0d`).** §A claimed `access_logs_total`
  "counts EMITTED records only". The code `.inc()`s **before** the emit await and
  deliberately does not deflate on `Err`, i.e. it counts **intent-to-emit**. §A was the only
  wrong site (the rustdoc and the H2 comment were already correct); corrected.
- **Minor 2 — carried forward as CF-70-2** (see below).

---

## §6. Carry-forwards opened by this phase

- **CF-70-1 — `compile_access_log_filter`'s `expect()` on a zero-arm filter.** Unreachable
  today: the full guard chain was traced (`validate_access_logs` ← `validate_hcm` ←
  `bootstrap::validate` ← **both** production entry points), and the final review additionally
  confirmed the **dynamic LDS path** is also guarded (`parse_lds_file` itself does no
  validation, but `load_dynamic_resources` re-runs `bootstrap::validate` over the chained
  static+dynamic listeners) — so it is not a live panic. **It becomes a live footgun the
  moment a second `AccessLogFilter` arm lands:** the phase adding arm #2 **must** convert the
  `expect()` to a full match.
- **CF-70-2 — ~~latent `expected_lines == 0` in the differential arms~~ — CLOSED at the §5.2
  state-3 re-entry (its premise was MEASURED FALSE; see `REVIEW.md` §4.2 / M70-R5).** As
  originally written this warned: if a future fixture suppressed **every** probe,
  `wait_file_lines(path, 0)` returns instantly and `read_to_string` "would error on a
  never-created file, yielding a misleading I/O failure rather than a clean pass."
  ~~"Unreachable from `0076`. Owner: the next filter fixture."~~ *(the original entry's tail
  sentences, struck at the second §5.2 re-entry per M70-R8 — the first closure ELIDED them
  instead of striking them; the "Owner:" clause is superseded by "No owner, no action" below,
  D-3.5.)* **That failure mode cannot occur.** The state-5 review measured both proxies opening the
  sink file **eagerly at config-load, before any request**: envoy-rust booted with the
  filtered config and no request driven leaves `access.log` present at size 0, and upstream
  `envoyproxy/envoy:v1.33.0` likewise creates it at boot (still size 0 after a single
  suppressed 200). So an all-suppressed fixture reads `""` → 0 lines → compares 0 against 0
  → **a correct clean pass**, which is the desired behavior. The file is never
  "never-created", so there is no misleading I/O failure to guard against. **No owner, no
  action: this is CLOSED, not carried.** It is recorded here rather than deleted (D-3.5)
  precisely so a future filter phase does not re-open it and chase a phantom.
- **CF-70-3 — `wait_file_lines(have >= want)` false-GREEN window on the DROPPED half.** Now
  that `want = expected_logged_count(probes)` (1 for `0076`) rather than `probes.len()` (2),
  a proxy that *wrongly* logged the suppressed probe could satisfy the poll with the kept line
  alone and be read before the extra line lands → the suppression regression would be missed.
  It can only produce a **false pass, never a false fail**, and the window is narrow (both
  probes complete before the poll starts and both records flush from the same buffer). Closing
  it (a settle-sleep before the read, or asserting `!wait_file_lines(path, expected+1,
  short_budget)`) is owned by the next access-log-filter phase.
- **Noted, no action:** T7 reverses the phase-06.1 REVIEW §7 R-8 directive (bulk `add(N)` was
  once chosen over N×`inc()`); the move is mandated by ADR-0141 and R-8's rationale is now
  dead.

**Carry-forwards NOT consumed by this phase** (unchanged, each owned by whatever future phase
touches its surface): M69-A..I, CF-69-1/2/3/5, M68-1, M-1, CF-67-3/5/6/7, the older Minors,
and the HTTP-filters-family (1)–(4).

---

## §7. Scope discipline

Only `status_code_filter` shipped. `response_flag_filter` / `header_filter` /
`duration_filter` / `not_health_check_filter` / `and_filter` / `or_filter` /
`grpc_status_filter` / `runtime_filter` / `metadata_filter` / `traceable_filter` /
`log_type_filter` are **DEFERRED by ADR-0140** and the final review confirmed none appears
anywhere in the tree. No RTDS override (ADR-0140 §2.2). No H2 filtered *fixture* (deferred —
the H2 *gate* is wired so H2 cannot regress). No new crate, no new external dependency, no new
fuzz target. The Envoy pin is untouched (D-3.7); `known-failures.txt` is untouched;
`#![forbid(unsafe_code)]` holds at every crate root (D-3.8).

The §6.1 split gate does **not** fire and **ADR-0142 stays UNFIRED**: 14 tasks, well under the
~25-task / ~1500-LoC threshold. The +1462/−88 diff exceeds ADR-0141's ~670 net-LoC estimate,
but the overage is **test and documentation mass** (mutation-proven coverage, the fixture, and
the contract subsection), not scope growth — the implementation surface is exactly the
config sub-message + predicate + per-sink gate + one bounded harness extension that ADR-0141
scoped.

---

## §8. Next session

**§5 state-4 verification** (`superpowers:verification-before-completion`) — a SEPARATE
session per §5.1 (one state per session; the context that wrote an artifact must not grade
it — ADR-0127). The §4 dry-run above is **not** the state-4 verdict: state-4 re-runs the full
§7.5 gate (a)–(f) itself and records its own evidence.

---
---

# §5 STATE-4 VERIFICATION — independent gate re-run (SEPARATE session)

> **Written by the §5 state-4 verification session** (`superpowers:verification-before-completion`),
> appended to — never rewriting — the state-3 record above (§1–§8 are the state-3 session's
> artifact). Per **ADR-0127** the context that wrote an artifact must not grade it: the §4
> dry-run above carried **no authority** here, and **every gate below was re-run from scratch
> by this session**. Written for a stranger with zero prior context (D-3.4).
>
> **VERDICT: the §7.5 gate PASSES on every sub-gate this state owns — (a), (b), (c), (d), (e).**
> Sub-gate **(f) `REVIEW.md` is NOT this session's job** — it is the §5 state-5 code-review's
> deliverable and remains legitimately unmet. Phase 70 therefore advances to state-5, NOT to
> state-6.

## §V1. Preconditions confirmed (disk + CI are the authority, not the handoff)

| Check | Command | Result |
|---|---|---|
| Tree clean | `git status --porcelain` | empty |
| Branch | `git rev-parse --abbrev-ref HEAD` | `main` |
| HEAD | `git rev-parse HEAD` | `2d272aaf88268b55266e996ef2c6f9234079fb8e` (the state-3 commit) |
| No sibling ahead | `git fetch origin --prune` + `git log --oneline -1 origin/main` | `2d272aa` — `HEAD` == `origin/main` |
| State-3 CI GREEN | `gh run list --commit 2d272aaf88268b55266e996ef2c6f9234079fb8e` | `{"conclusion":"success","databaseId":29488932188,"status":"completed","workflowName":"ci"}` |

The state-3 CI run was re-confirmed with the **FULL 40-char SHA** (a short SHA silently returns
`[]` and would look like "CI never ran"). Both jobs are `success` with `steps=15` / `steps=13` —
**not** the runner-starvation signature (`cancelled` + `runner_name:""` + `steps:0`), so the
commit genuinely executed.

## §V2. Gate (e) — build / lint / format / deny — ALL CLEAN

Run serially (cargo's file lock makes concurrent invocations contend), full output redirected
to files — **never piped through `tail`**, which truncates the `failures:` block.

| Gate | Command | Exit | Output |
|---|---|---|---|
| fmt | `cargo fmt --all -- --check` | **0** | **zero bytes** (a 0-line file — no diff) |
| build | `cargo build --workspace --all-targets` | **0** | `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 9.17s` |
| clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **0** | `Finished \`dev\` profile … in 1.66s`; `grep -cE "^(warning\|error)"` → **0** |
| deny | `cargo deny check` | **0** | `advisories ok, bans ok, licenses ok, sources ok` |

`cargo deny check` emitted five `warning[license-not-encountered]` lines (`0BSD`,
`BSD-2-Clause`, `MPL-2.0`, `Unicode-DFS-2016`, `Zlib` — allow-list entries no dependency
actually uses). These are **unmatched-allowance notices about `deny.toml`, not findings against
any dependency**; the summary line is `advisories ok, bans ok, licenses ok, sources ok` and the
exit code is 0. **No freshly-published RustSec advisory fired this session** — no `cargo update
-p X --precise` was needed.

**`cargo build -p envoy-bin` was run BEFORE any differential** (exit 0) — the harness executes
`target/debug/envoy-bin`, and a stale debug binary would RED with `unknown field: filter` on
this phase's new config key, which mimics a real failure but is not one.

## §V3. Gates (a)+(b) — `cargo test --workspace` — 2016 passed / 6 failed / 9 ignored

Run as `cargo test --workspace --no-fail-fast`. **`--no-fail-fast` is mandatory for
adjudication**: the bare `cargo test --workspace` aborts at the first failing *binary* and never
exercises the rest of the gate.

```
TEST_RUN1_EXIT=101
passed=2016 failed=6 ignored=9
```

### Gate (a) — the new fixture is GREEN

```
     Running tests/access_log_status_code_filter.rs (target/debug/deps/access_log_status_code_filter-1d5200f2899a0d82)
test access_log_status_code_filter ... ok
```

`0076` passed **inside the full workspace run** (i.e. under full parallel load) **and** again in
isolation:

```
$ cargo test -p differential --test access_log_status_code_filter
test access_log_status_code_filter ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.13s
```

### Gate (b) — all 6 failures adjudicated ENVIRONMENTAL; none in the phase-70 surface

**Blast-radius check first (the load-bearing one).** The only fixture anywhere in the tree that
configures a `status_code_filter` is the new fixture itself:

```
$ grep -rlE "status_code_filter" tests/fixtures/*/expectations.yaml tests/fixtures/*/*.yaml | sed 's|/[^/]*$||' | sort -u
tests/fixtures/0076-accesslog-status-code-filter
```

**None of the 6 failures touches a filter**, and `0076` — the one that does — passes. A sink
with no filter takes the `should_log → true` path, which is the pre-phase-70 behavior.

Each failure was re-run **in isolation** to separate deterministic-environmental from
parallel-load flake (the discriminator: environmental fails alone, load-flake passes alone):

| # | Test | Isolated | Class | MEASURED evidence |
|---|---|---|---|---|
| 1–4 | `access_log_rcd_upstream_reset`, `access_log_rf_upstream_reset`, `access_log_h2_rcd_upstream_reset`, `access_log_h2_uc_upstream_reset` | **FAILS alone** (deterministic) | environmental — IPv6-unreachable close backend | `immediate_connect_error:_Network_is_unreachable` + `remote_address:[fdc4:f303:9324::254]:40435`; and the mirror assert `envoy="{\"rc\":503,\"rf\":\"UF\"}"` vs `envoy-rust="{\"rc\":503,\"rf\":\"UC\"}"` |
| 5 | `admin_config_dump_server_info` | **FAILS alone** (deterministic) | environmental — Docker bridge IP | envoy-only stats `backend::192.168.65.2:38459::hostname::host.docker.internal` (this host routes the backend via `192.168.65.2`, not the allow-listed address) |
| 6 | `client::tests::send_request_maps_h2_handshake_failure_to_typed_error` | **PASSES alone** (`1 passed; 0 failed`) | parallel-load / host flake | `expected H2ClientHandshake, got Ok(ClientStream { host: "test.example", .. })` — the handshake *unexpectedly succeeds* on this host's networking |

Failures 1–4 are one root cause seen from two sides: real Envoy **cannot reach** the
host-spawned close backend over IPv6, so it logs a **connect failure (`UF`)** where envoy-rust
logs a genuine **reset (`UC`)**. That is a property of this host's networking, not of the code.

**Difference from the state-3 dry-run (5 failures) is fully accounted for:** this run added
exactly one — #6, the `envoy-http2` handshake test, which is itself a documented host-flake and
**passes deterministically in isolation**. The set is non-identical run-to-run by design; the
membership is a subset of the documented flake families, with no new member.

### The decisive cross-check — local `passed + failed` == CI `passed`

CI ran this **exact tree** (SHA `2d272aaf…`) and was GREEN, so every one of these 6 tests passes
on CI:

```
$ gh run view 29488932188 --log | grep -oE "test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed" | awk '{p+=$1; f+=$3} END {print "CI passed="p" CI failed="f}'
CI passed=2022 CI failed=0
```

**2016 local passed + 6 local failed = 2022 == 2022 CI passed.** The identity holds exactly.
This proves two things at once: the local RED set is **entirely environmental** (CI runs the same
code green), and **no test silently disappeared** from the local run. **CI is authoritative for
this documented flake set — never a regression, and never to be "fixed" by weakening a fixture.**

## §V4. Gate (c) — conformance — unchanged, nothing owed

No protocol-conformance surface this phase.

```
$ git diff --stat b362bae..HEAD -- tests/conformance/ .github/
(empty)
```

`known-failures.txt` is **untouched** and **must not be trimmed** — local h2spec scores
invalid-preface 3.5/2 as PASS while CI still fails it, so trimming from local evidence would
break CI.

## §V5. Gate (d) — fuzz — no new target; the corpus seed is genuinely tracked

The §7.4 disposition (ADR-0141 PV-5) is a `parse_bootstrap` **corpus seed** only, riding the
EXISTING target — so **no new fuzz target and no `ci.yml` step is owed** (a new target is not
auto-discovered and would need hand-wiring; the empty `.github/` diff in §V4 confirms none was
added). The seed is **verified tracked** the only way that actually proves it — `git status` is
not sufficient, because the corpus dir is `*`-ignored and a seed is silently untracked and
invisible to CI without an explicit `!`-un-ignore line:

```
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml
crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml     # PRINTS → tracked
$ git check-ignore crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml
CHECK_IGNORE_EXIT=1                                                          # exit 1 → NOT ignored
```

Short-budget run executed from the **crate dir** (`cd crates/envoy-config` first — it errors from
the repo root with `could not read .../fuzz/Cargo.toml`):

```
$ cargo +nightly fuzz run parse_bootstrap -- -max_total_time=60
seed corpus: files: 6882
#6883     INITED cov: 15401 ft: 30817 corp: 2587/1693Kb exec/s: 6883 rss: 336Mb
#797741   DONE   cov: 16262 ft: 33691 corp: 3154/2200Kb lim: 3055 exec/s: 13077 rss: 354Mb
Done 797741 runs in 61 second(s)
FUZZ_EXIT=0
```

**797,741 runs, zero crashes / panics / leaks, exit 0.** CI's fuzz job independently ran the
`parse_bootstrap` target green on this SHA (run `29488932188`, job `fuzz (parse_bootstrap + …)`
→ `success`).

## §V6. Gate (f) — `REVIEW.md` — NOT this state's job

`REVIEW.md` does not exist and **is not owed by state-4**. It is the §5 state-5 code-review's
deliverable (`superpowers:requesting-code-review`). Per §5.1 this session advances exactly ONE
state and does **not** chain into the review.

## §V7. State-4 verdict

**PASS on (a), (b), (c), (d), (e). (f) deferred to state-5 by design.** No REAL regression was
found, so there is **no §5.2 re-entry to state-3**: no code was changed by this session, and no
fixture was weakened. The 3 Minors carried from the state-3 review (**CF-70-1/2/3**) are
**not** gate failures — CF-70-1 is unreachable today, CF-70-2 is latent, CF-70-3 is
false-pass-only — and remain live carry-forwards for the state-5 reviewer to weigh.

**This session changed no code.** Its only artifact is this §V section plus the ledger
(`STATE.md`) and the commit.

### For the state-5 code-review session (NOT state-4's job — recorded here so it is not lost)

- **The untested composition:** phase 70 added a per-sink gate over **two** consumers (H1 + H2),
  but the differential covers **H1 only**. The H2 filtered path is covered **in-process only**,
  and — as the blast-radius grep in §V3 re-confirmed this session — **no fixture anywhere sets a
  filter on H2**. A green gate proves the code does what its tests ask, not that the tests ask
  the right question; weigh whether that composition warrants a live probe against both proxies.
- **A judgment call to weigh:** the state-3 session recorded a MEASURED **stricter-than-upstream**
  acceptance boundary in `BEHAVIOR_CONTRACT.md` §E.1 rather than firing a new ADR (upstream
  ACCEPTS `op` omitted → proto3 default `EQ`, `default_value` omitted → `0`, and numeric enum
  tokens `op: 1`; envoy-rust REJECTS all three, because these proto3 scalars are modeled
  serde-required). It is fail-loud (never a silent runtime divergence) and consistent with the
  tree-wide posture (`FractionalPercent.numerator`, `TokenBucket.max_tokens`). ADR-0049 already
  governs config-validity as all-fatal with native messages. Weigh whether this narrows
  ADR-0049's "same class of configs is rejected/accepted" claim enough to deserve its own ADR —
  **ADR-0142 is available** (the §6.1 split it was reserved for is confirmed NOT to fire).
- T7 reverses the phase-06.1 REVIEW §7 R-8 directive (bulk `add(N)` over N×`inc()`); the move is
  ADR-0141-mandated and R-8's rationale is now dead.

## §V8. Next session

**§5 state-5 code-review** (`superpowers:requesting-code-review`) — a SEPARATE session per §5.1.
Its deliverable is `REVIEW.md` (gate (f)). If it finds issues, the re-entry point is **state-3,
not state-4** (§5.2).

---

# §5.2 RE-ENTRY — state-3 implementation (the `REVIEW.md` I-1 fix)

> **Written by the §5.2 state-3 RE-ENTRY session** (`superpowers:executing-plans` +
> `superpowers:test-driven-development`). Per `BOOTSTRAP_PROMPT.md` §5.2 a `REVIEW.md`
> carrying issues re-enters at **state 3, NOT state 4** — this session resumed
> IMPLEMENTATION under TDD; it did **not** re-run the §7.5 gate (state-4 owns that; its
> evidence is §V1–§V8 above) and did **not** re-do the review (state-5's artifact is
> `REVIEW.md`). Written for a stranger with zero prior context (D-3.4).
>
> **Sections §1–§8 (state-3) and §V1–§V8 (state-4) above are the historical record and were
> NOT rewritten.** The sole exception is §6's **CF-70-2** entry, which M70-R5 explicitly
> directs be corrected or closed; its original wording is preserved inline, struck through,
> alongside the measurement that falsifies it (D-3.5 — nothing was deleted).
>
> **Cold-start:** `git status --porcelain` clean, branch `main`, `HEAD` = `origin/main` =
> `b860e4e6` (the phase-70 §5 state-5 code-review commit); `git fetch origin --prune` showed
> no sibling ahead. **STEP 0.5:** that commit's CI run `29516429241` re-confirmed
> `completed`/`success` on the FULL 40-char SHA.

## §R1. The blocking finding and what was actually wrong

`REVIEW.md` §3.2 **I-1 (Important)** — the config→runtime `ComparisonOp` → `FilterOp` mapping
in `compile_access_log_filter` (`crates/envoy-http1/src/hcm.rs:1746-1750`) was **unpinned for
`Eq` and `Le`**. This is a **test-coverage gap, NOT a behavioral bug**: the mapping is correct
as written; nothing held it correct.

**Independently re-confirmed this session before touching anything** (not taken on trust from
`REVIEW.md`):

```
$ grep -rn "op: EQ\|op: LE" crates/ tests/
(zero hits)
$ grep -rn "ComparisonOp::Eq\|ComparisonOp::Le" crates/ tests/
crates/envoy-http1/src/hcm.rs:1747:  ComparisonOp::Eq => FilterOp::Eq,
crates/envoy-http1/src/hcm.rs:1749:  ComparisonOp::Le => FilterOp::Le,
```

Both tokens appeared **nowhere in the tree except the two match arms that define them**. The
`envoy-accesslog` `filter.rs` tests prove each `FilterOp` **evaluates** correctly ~~and the
`envoy-config` tests prove `op: EQ` **parses** correctly~~ *(struck at the second §5.2 re-entry
per `REVIEW.md` §8.3 I-2, D-3.5 — this claim was FALSE and self-contradicted by the grep quoted
six lines above: `op: EQ` had **zero hits**, so no test anywhere parsed it; the YAML-token →
`ComparisonOp` serde mapping is pinned since the second re-entry by
`yaml_op_token_compiles_to_matching_filter_op`)* — **nothing connected the two**, so a
user config saying `op: EQ` could compile to an `Le` predicate (logging every record at-or-below
404 instead of exactly 404) with the entire suite still green.

## §R2. The fix (one file, one test — the seam already existed)

`hcm_config_with_filtered_access_log` (`hcm.rs:4487`) **already took the operator as a
parameter** (`filter: Option<(envoy_config::ComparisonOp, u32)>`), so no production code and no
new helper was needed. `from_config_compiles_status_code_filter_into_sink` (`hcm.rs:4562`) —
previously a single `(Ge, 500)` leg — is now **table-driven across all three shipped operators**,
routed through the production `from_config` constructor so the config→runtime compilation is the
thing under test:

| leg | threshold | probed statuses (`must_log`) |
|---|---|---|
| `Ge` | 500 | 499 → false, 500 → **true**, 503 → **true** |
| `Eq` | 404 | 403 → false, 404 → **true**, 405 → false |
| `Le` | 200 | 100 → **true**, 200 → **true**, 201 → false |

**Each leg probes both sides of the boundary deliberately, so no other operator satisfies the
same row.** The `Le` leg's `(100, true)` probe is load-bearing and is the reason the table is
shaped this way: a naive `Le 200` table of only `(200, true), (201, false)` is **also satisfied
by `Eq 200`** and would have stayed GREEN under the very mutation this fix exists to catch.
`(100, true)` is what separates them.

**No production code changed.** The mapping at `hcm.rs:1746-1750` is byte-for-byte as it landed.

## §R3. RED→GREEN evidence (D-3.1 — the RED was PROVEN, three times, arm by arm)

The production code was already correct, so the RED could not come from absence of the feature —
it comes from **mutating the mapping** and proving the new assertions catch it. Per memory
`mutation-checks-collide-with-parallel-subagents`, **every mutation ran in an isolated
`git worktree --detach` at `b860e4e6`, never in-place** (during the state-5 review a concurrent
subagent's `git checkout --` silently clobbered an in-place mutation and nearly produced a false
conclusion). Per memory `mutation-check-needs-forced-rebuild`, **every run was grepped for
`Compiling envoy-http1`** (a stale binary yields a FALSE PASS), and the mutation's **presence was
re-grepped AFTER each run**, not just before.

| # | Mutation (`Ge` untouched unless noted) | `Compiling envoy-http1` | Result |
|---|---|---|---|
| 1 | `Eq => FilterOp::Le` **and** `Le => FilterOp::Eq` (the review's swap) | 1 hit | **RED** — `Eq 404 filter on status 403: expected should_log=false` |
| 2 | `Le => FilterOp::Eq` **only** (`Eq` correct) | 1 hit | **RED** — `Le 200 filter on status 100: expected should_log=true` |
| 3 | `Ge => FilterOp::Le` **only** | 1 hit | **RED** — `Ge 500 filter on status 499: expected should_log=false` |

Mutations 2 and 3 exist because `assert_eq!` **bails at the first failing assertion**: run 1
alone only proves the `Eq` leg fires, and would leave "does the `Le` leg actually bite?"
unproven — precisely the "a test that could not fail" trap this phase keeps hitting. Each arm is
therefore pinned **independently**, each failing for its own correct and distinct reason.

**GREEN** — the mapping restored (unmutated: `Eq => Eq`, `Ge => Ge`, `Le => Le`), in the main
tree, `Compiling envoy-http1` confirmed:

```
test hcm::tests::from_config_compiles_status_code_filter_into_sink ... ok
test result: ok. 1 passed; 0 failed
```

**A fourth mutation re-proved the RED against the FINAL landed test code**, after the
`clippy::type_complexity` refactor in §R4 changed the test (a mutation proof against
pre-refactor code does not cover post-refactor code): the `Eq`⇄`Le` swap → **RED**,
`Eq 404 filter on status 403`, `Compiling envoy-http1` = 1 hit, mutation confirmed present after
the run.

## §R4. `clippy::type_complexity` — caught locally, not left for CI

The table's first form tripped the phase's own gate-(e) lint (memory `plan-md-example-code-trips-clippy`):

```
error: very complex type used. Consider factoring parts into `type` definitions
    --> crates/envoy-http1/src/hcm.rs:4574:20
     |     let cases: &[(envoy_config::ComparisonOp, u32, &[(u16, bool)])] = &[
     = note: `-D clippy::type-complexity` implied by `-D warnings`
```

Resolved per D-3.5 to the clippy-clean equivalent — a documented local `type OpCase<'a>` alias —
rather than an `#[allow]`.

## §R5. The Minors folded (M70-R3, M70-R5) — and the three deliberately NOT folded

**M70-R3 — FOLDED** (`crates/envoy-config/src/bootstrap.rs`, `rejects_status_code_filter_unknown_op`).
The test asserted only `matches!(err, ConfigError::Yaml(_))`, which **any** YAML-level error
satisfies — an unrelated typo in its own fixture would have kept it green for the wrong reason.
It now also asserts the rejection **names the offending token**. **The tightening was proven to
bite** (isolated worktree, `Compiling envoy-config` = 1 hit): mutating the fixture so `op` is
valid (`GE`) but an unrelated field is typo'd (`path` → `pathx`) still yields a
`ConfigError::Yaml(_)` — the OLD assertion stays green — while the NEW assertion goes **RED**:

```
rejection must name the offending op token, got "parsing bootstrap YAML: static_resources
.listeners[0].filter_chains[0].filters[0]: unknown field `pathx`, expected `path` or
`log_format` at line 9 column 15"
```

**M70-R5 — FOLDED: CF-70-2 is CLOSED** (§6 above). Its premise is MEASURED FALSE (`REVIEW.md`
§4.2): both proxies create the sink file **eagerly at config-load** (size 0 before any request,
verified on envoy-rust AND on real `envoyproxy/envoy:v1.33.0`), so an all-suppressed fixture
`read_to_string`s `""` → 0 lines → 0 == 0 → **a correct clean pass**, never the "misleading I/O
failure on a never-created file" it warned of. Closed rather than propagated so a future filter
phase does not chase a phantom. **This session relied on the state-5 measurement and did not
re-derive it.**

**NOT folded, and why** (all three remain live carry-forwards):
- **M70-R1** (the hand-maintained one-element `set_arms` array + its overclaiming doc comment,
  `bootstrap.rs:5117-5130` / `5097-5098`) — `REVIEW.md` §3.3 is explicit that this is best
  discharged **by the phase that lands oneof arm #2, alongside CF-70-1**, which is the same
  surface. Folding it here would touch a surface this re-entry has no other reason to open.
- **M70-R2** (`expected_logged_count`'s wiring into the two byte-exact arms has no in-process
  witness) and **M70-R4** (`AccessLog.filter` serializes as `"filter": null` — no
  `skip_serializing_if`; it EXTENDS the existing `FileAccessLog.log_format` pattern rather than
  introducing one) — both explicitly "reasonable carry-forwards" per `REVIEW.md` §7.

## §R6. Verification run this session (NOT the §7.5 gate — that is state-4's job)

Scoped to the three crates this re-entry touches. **The full §7.5 gate was deliberately NOT
re-run** (§5.1 — that is the next session's state-4 re-verification).

```
$ cargo test -p envoy-http1 -p envoy-config -p envoy-accesslog --no-fail-fast
test result: ok. 102 passed; 0 failed    (envoy-accesslog)
test result: ok. 611 passed; 0 failed    (envoy-config)
test result: ok. 173 passed; 0 failed    (envoy-http1)
→ 886 passed / 0 failed

$ cargo clippy -p envoy-http1 -p envoy-config -p envoy-accesslog --all-targets --all-features -- -D warnings
(exit 0, clean)

$ cargo fmt --all -- --check
(exit 0, clean)
```

**886 / 0 is the same total the state-5 review measured GREEN *under the mutation*** (102 + 611 +
173) — the count is unchanged because this fix adds assertions to two EXISTING tests rather than
new test functions. That is the point: the suite that was green under a silently-inverted mapping
is now green with the mapping actually pinned, and goes RED the moment it is inverted. `cargo fmt`
was run and checked here rather than deferred, since CI has no `paths-ignore` and a fmt miss reds
the next push (memory `envoy-rust-state4-ci-first-execution`).

## §R7. Scope discipline

- **No production code changed** — the diff is two test bodies (`envoy-http1`, `envoy-config`)
  plus this record and the CF-70-2 correction. The `ComparisonOp`→`FilterOp` mapping is exactly
  as it landed at state-3.
- **No ADR fired.** The I-1 fix adds test coverage; it changes no decision. Next-available ADR
  remains **ADR-0143** (unreserved). **ADR-0142's settlement of the `BEHAVIOR_CONTRACT.md` §E.1
  stricter-than-upstream boundary was NOT re-litigated** — the phase-70 config surface stays
  closed and no code is owed by it.
- **No fixture weakened; `known-failures.txt` untouched** (memory `h2spec-3-5-2-preface-host-sensitive`).
- `#![forbid(unsafe_code)]` holds; no new dependency (D-3.2/D-3.8).

## §R8. Next session

**§5 state-4 RE-VERIFICATION** (`superpowers:verification-before-completion`) — a SEPARATE
session per §5.1; this session did NOT chain into it. A fresh context re-runs the **full §7.5
gate** (a)–(e) over the re-entry's head commit and appends its evidence. Then a **state-5
re-review** (gate (f) — the current `REVIEW.md` verdict is NOT approved and I-1 is now fixed),
then the **state-6 close-out**.

---
---

# §5 STATE-4 RE-VERIFICATION — the full §7.5 gate re-run over the §5.2 re-entry head (SEPARATE session)

> **Written by the §5 state-4 RE-VERIFICATION session** (`superpowers:verification-before-completion`),
> **appended to — never rewriting — §1–§8 (state-3), §V1–§V8 (the state-4 verification of the
> PRE-fix commit `899ca5c`), and §R1–§R8 (the §5.2 re-entry).** Written for a stranger with zero
> prior context (D-3.4).
>
> **Why this re-run exists.** The §V1–§V8 evidence was measured against the **PRE-fix** commit
> `899ca5c` and does **NOT** carry over to the re-entry's head. Per **ADR-0127** the re-entry
> session's own scoped run (§R6: `886 passed / 0 failed`) carries **ZERO authority** here — that is
> the implementing context grading itself, and it never ran the workspace-global gate, the
> differential, `cargo deny`, or the fuzzer. **Every gate below was re-measured from scratch by this
> session.**
>
> **VERDICT: the §7.5 gate PASSES on every sub-gate this state owns — (a), (b), (c), (d), (e).**
> Sub-gate **(f)** is **NOT met and is NOT this session's job**: `REVIEW.md` exists but its verdict
> is **NOT approved**. Discharging it is the §5 **state-5 RE-review**. Phase 70 therefore advances
> to the state-5 re-review, **NOT** to state-6.

## §V(2)1. Preconditions confirmed (disk + CI are the authority, not the handoff)

| Check | Command | Result |
|---|---|---|
| Tree clean | `git status --porcelain` | empty |
| Branch | `git rev-parse --abbrev-ref HEAD` | `main` |
| HEAD | `git rev-parse HEAD` | `2763c73525821a42012ee354fcb2b0c34ed449a4` (the §5.2 re-entry commit) |
| No sibling ahead | `git fetch origin --prune` + `git log --oneline -1 origin/main` | `2763c73` — `HEAD` == `origin/main` |
| Re-entry CI GREEN | `gh run list --commit 2763c73525821a42012ee354fcb2b0c34ed449a4` | `{"conclusion":"success","databaseId":29520553072,"status":"completed","workflowName":"ci"}` |

Confirmed with the **FULL 40-char SHA** (a short SHA silently returns `[]` and would look like "CI
never ran"). Both jobs are `success` with `steps=15` (`build + test + lint`) and `steps=13` (`fuzz`)
— **not** the runner-starvation signature (`cancelled` + `steps:0`), so the commit genuinely executed.

## §V(2)2. Gate (e) — build / lint / format / deny — ALL CLEAN

Run **serially** (cargo's file lock makes concurrent invocations contend), full output redirected to
files — **never piped through `tail`**, which truncates the `failures:` block.

| Gate | Command | Exit | Output |
|---|---|---|---|
| fmt | `cargo fmt --all -- --check` | **0** | **zero bytes** (a 0-line file — no diff) |
| build | `cargo build --workspace --all-targets` | **0** | `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 10.79s` |
| clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **0** | `grep -cE "^(warning\|error)"` → **0** |
| deny | `cargo deny check` | **0** | `advisories ok, bans ok, licenses ok, sources ok` |

`cargo deny check` emitted five `warning[license-not-encountered]` lines — **unmatched-allowance
notices about `deny.toml`'s allow-list, not findings against any dependency**; the summary is
`advisories ok, bans ok, licenses ok, sources ok`, exit 0. **No freshly-published RustSec advisory
fired this session** — no `cargo update -p X --precise` was needed.

**`cargo build -p envoy-bin` was run BEFORE any differential** (exit 0) — the harness executes
`target/debug/envoy-bin` (debug, NOT release), and a stale debug binary REDs with
`unknown field: filter` on this phase's config key, which mimics a real failure but is not one.

## §V(2)3. Gates (a)+(b) — `cargo test --workspace --no-fail-fast` — 2015 passed / 7 failed / 9 ignored

`--no-fail-fast` is **mandatory** for adjudication: a bare `cargo test --workspace` aborts at the
first failing *binary* and never exercises the rest of the gate.

```
TEST_RUN1_EXIT=101
passed=2015 failed=7 ignored=9
```

### Gate (a) — the new fixture is GREEN

`0076` passed **inside the full workspace run** (i.e. under full parallel load) **and** again in
isolation:

```
     Running tests/access_log_status_code_filter.rs
test access_log_status_code_filter ... ok

$ cargo test -p differential --test access_log_status_code_filter
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.14s
```

### Gate (b) — all 7 failures adjudicated NOT-A-REGRESSION; none in the phase-70 surface

**Blast-radius check first (the load-bearing one).** The only fixture anywhere in the tree that
configures a `status_code_filter` is the new fixture itself:

```
$ grep -rlE "status_code_filter" tests/fixtures/ | sed 's|/[^/]*$||' | sort -u
tests/fixtures/0076-accesslog-status-code-filter
```

**None of the 7 failures touches a filter**, and `0076` — the one that does — passes. A sink with no
filter takes the `should_log → true` path, which is exactly the pre-phase-70 behavior.

Each failure was re-run **in isolation** to separate deterministic-environmental from parallel-load
flake (**the discriminator: environmental fails alone; load-flake passes alone**):

| # | Test | Isolated | Class | MEASURED evidence |
|---|---|---|---|---|
| 1–4 | `access_log_rcd_upstream_reset`, `access_log_rf_upstream_reset`, `access_log_h2_rcd_upstream_reset`, `access_log_h2_uc_upstream_reset` | **FAILS alone** (deterministic) | environmental — IPv6-unreachable close backend | `immediate_connect_error:_Network_is_unreachable` + `remote_address:[fdc4:f303:9324::254]:33837` |
| 5 | `admin_config_dump_server_info` | **FAILS alone** (deterministic) | environmental — Docker bridge IP | envoy-only stats `backend::192.168.65.2:37497::{canary,cx_active,cx_connect_fail}` (this host routes the backend via `192.168.65.2`, not the allow-listed address) |
| 6 | `admin_ready_returns_200_post_migration` (`crates/envoy-bin/tests/admin_ready.rs:47`) | **PASSES alone** (`1 passed; 0 failed`) | parallel-load startup race | `drive /ready: Os { code: 11, kind: WouldBlock, message: "Resource temporarily unavailable" }` |
| 7 | `dataless_fin_ticks_allowed_for_tcp_proxy_but_not_echo` (`crates/envoy-bin/tests/network_filter_rbac.rs:782`) | **PASSES alone** (`1 passed; 0 failed`) | parallel-load startup race | `data listener never came up within 10s: Connection refused (os error 111)` |

Failures 1–4 are **one root cause seen from two sides**: real Envoy cannot reach the host-spawned
close backend over IPv6, so it logs a **connect failure (`UF`)** where envoy-rust logs a genuine
**reset (`UC`)**. That is a property of this host's networking, not of the code.

**#6 and #7 are NEW members relative to the §V3 sweep — and were investigated, not waved through.**
Neither is in the phase-70 surface (neither configures an access log at all; the blast-radius grep
above is decisive), and **both pass deterministically in isolation**, which is the signature of the
documented **port-reuse / startup-race** family (memories `eds-fatal-startup-test-port-reuse-flake`,
`rds-no-rds-is-inert-startup-flake`, `xds-eds-hot-reload-admin-ready-startup-flake`,
`xds-file-cds-happy-path-admin-ready-startup-flake`, `differential-fixtures-flake-under-parallel-load`)
— a listener/admin endpoint not yet accepting when the probe fires (`Connection refused` /
`WouldBlock`). Conversely `client::tests::send_request_maps_h2_handshake_failure_to_typed_error`,
which the §V3 sweep saw RED, **passed this run**. **The RED set legitimately varies run-to-run**;
the membership here is entirely within the documented flake families, with no new *family*.

### The decisive cross-check — local `passed + failed` == CI `passed`

CI ran this **exact tree** (SHA `2763c73525…`) and was GREEN, so every one of these 7 tests passes on CI:

```
$ gh run view 29520553072 --log | grep -oE "test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed" \
    | awk '{p+=$4; f+=$6} END {print "CI passed="p" CI failed="f}'
CI passed=2022 CI failed=0
```

**2015 local passed + 7 local failed = 2022 == 2022 CI passed.** The identity holds **exactly**.
This proves two things at once: the local RED set is **entirely environmental/flake** (CI runs the
same code green), and **no test silently disappeared** from the local run. **CI is authoritative for
this documented flake set — never a regression, and never to be "fixed" by weakening a fixture.**

**The total is UNCHANGED from the §V3 sweep (2022 == 2022)** — the predicted result, and it was
checked rather than assumed: the §5.2 re-entry adds assertions to two EXISTING tests rather than new
test functions, so the count must not move. A changed total would have been a signal worth chasing.

### A methodological trap this session hit (recorded so the next session does not repeat it)

The first isolation attempt used `cargo test -p envoy-bin <test_name> -- --exact` and reported
`test result: ok. 0 passed; 0 failed; 5 filtered out` — **exit 0 with the test never having run.**
That is a **FALSE GREEN**: `-p envoy-bin` selects *a* test binary, and the name lived in a different
one, so the filter matched nothing and cargo still exited 0. **`0 passed` is not a pass.** The
re-run named the target explicitly (`--test admin_ready` / `--test network_filter_rbac`) and only
then produced the real `1 passed` verdicts quoted above. Always assert on the `N passed` count, never
on the exit code alone.

## §V(2)4. Gate (c) — conformance — unchanged, nothing owed

No protocol-conformance surface this phase.

```
$ git diff --stat b362bae..HEAD -- tests/conformance/ .github/
(empty)
```

`tests/conformance/h2spec/known-failures.txt` is **untouched** (21 lines) and **must not be trimmed**
— local h2spec scores invalid-preface 3.5/2 as PASS while CI still fails it, so trimming from local
evidence would break CI (memory `h2spec-3-5-2-preface-host-sensitive`).

## §V(2)5. Gate (d) — fuzz — no new target; the corpus seed is genuinely tracked

The §7.4 disposition (ADR-0141 PV-5) is a `parse_bootstrap` **corpus seed** only, riding the EXISTING
target — so **no new fuzz target and no `ci.yml` step is owed** (a new target is not auto-discovered
and would need hand-wiring; the empty `.github/` diff in §V(2)4 confirms none was added). The seed is
verified tracked the only way that actually proves it — the corpus dir is `*`-ignored, so a seed is
silently untracked and invisible to CI without an explicit `!`-un-ignore line:

```
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml
crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml     # PRINTS → tracked
$ git check-ignore crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml
CHECK_IGNORE_EXIT=1                                                          # exit 1 → NOT ignored
```

Short-budget run executed from the **crate dir** (`cd crates/envoy-config` first — it errors from the
repo root with `could not read .../fuzz/Cargo.toml`, memory `cargo-fuzz-runs-from-crate-dir-not-repo-root`):

```
$ cargo +nightly fuzz run parse_bootstrap -- -max_total_time=60
INFO: seed corpus: files: 9448 min: 1b max: 2922b total: 5285897b
#9449     INITED cov: 16260 ft: 33689 corp: 3071/2070Kb
#106327   DONE   cov: 16316 ft: 33878 corp: 3114/2091Kb lim: 2966 exec/s: 1084
Done 106327 runs in 98 second(s)
FUZZ_EXIT=0
```

**106,327 runs, zero crashes / panics / leaks, exit 0.** CI's fuzz job independently ran the
`parse_bootstrap` target green on this SHA (run `29520553072`, `steps=13` → `success`).

## §V(2)6. Gate (f) — `REVIEW.md` — NOT this state's job, and NOT met

`REVIEW.md` **exists** but its verdict is **NOT approved** (0 Critical / 1 Important / 5 Minor). Its
one blocking finding (§3.2 **I-1**) is what the §5.2 re-entry fixed. Confirming that discharge and
re-issuing the verdict is the §5 **state-5 RE-review**'s deliverable
(`superpowers:requesting-code-review`). Per §5.1 this session advances exactly ONE state and does
**not** chain into it.

## §V(2)7. State-4 re-verification verdict

**PASS on (a), (b), (c), (d), (e). (f) is unmet by design — state-5's job.** No REAL regression was
found, so there is **no §5.2 re-entry to state-3**: this session changed **no code** and weakened
**no fixture**. Its only artifacts are this §V(2) section, the ledger (`STATE.md` /
`STATE_HISTORY.md`), and the commit.

**The re-entry's "no production code changed" claim was independently VERIFIED, not accepted on
trust** (it is the premise that makes the unchanged 2022 total meaningful):

```
$ diff <(git show 899ca5c:crates/envoy-http1/src/hcm.rs | sed -n '/fn compile_access_log_filter/,/^}/p') \
       <(sed -n '/fn compile_access_log_filter/,/^}/p' crates/envoy-http1/src/hcm.rs)
(no output — exit 0)
```

`compile_access_log_filter` is **byte-identical** to the pre-review landing `899ca5c`, and the
mapping reads `Eq => FilterOp::Eq`, `Ge => FilterOp::Ge`, `Le => FilterOp::Le` with **no `_`
wildcard**. The re-entry diff (`b860e4e..2763c73`) touches 5 files: two test bodies
(`crates/envoy-config/src/bootstrap.rs`, `crates/envoy-http1/src/hcm.rs`) plus docs
(`PROGRESS.md`, `STATE.md`, `STATE_HISTORY.md`).

The **29 pre-phase-70 access-log-driver fixtures (28 byte-exact)** figure was **independently
recounted** this session by driver `kind`: 22 `http1_access_log_byte_exact` + 7
`http2_access_log_byte_exact` + 1 `http1_with_access_log` = **30 including `0076` → 29 pre-phase-70**.
Confirmed correct.

Live carry-forwards are **not** gate failures and remain for the state-5 reviewer to weigh:
**CF-70-1** (unreachable today), **CF-70-3** (false-pass-only), **M70-R1/M70-R2/M70-R4**
(**M70-R3 + M70-R5 CONSUMED** by the re-entry; **CF-70-2 CLOSED**).

## §V(2)8. Next session

**§5 state-5 RE-review** (`superpowers:requesting-code-review`) — a SEPARATE session per §5.1. It is
a **RE-review**: the current `REVIEW.md` verdict is NOT approved, and its I-1 is now fixed. Its job
is to confirm I-1 is genuinely discharged (**the fix is a test change — verify it BITES**, e.g. by
re-running the `Eq`⇄`Le` mutation in an ISOLATED worktree, rather than reading the diff and
agreeing), confirm M70-R3 + M70-R5 are genuinely consumed, and re-issue the verdict. If it approves,
the phase advances to the **state-6 close-out**. If it finds issues, the re-entry point is
**state-3, not state-4** (§5.2).

---
---

# §5.2 RE-ENTRY (2nd) — state-3 implementation (the `REVIEW.md` §8.3 I-2 fix)

> **Written by the SECOND §5.2 state-3 RE-ENTRY session** (`superpowers:executing-plans` +
> `superpowers:test-driven-development`). Per `BOOTSTRAP_PROMPT.md` §5.2 a `REVIEW.md` carrying
> an Important re-enters at **state 3, NOT state 4** — this session resumed IMPLEMENTATION under
> TDD; it did **not** re-run the §7.5 gate (a state-4 RE-verification over this head is the NEXT
> session) and did **not** re-do the review (state-5's artifact is `REVIEW.md`, whose current
> verdict is §8). Written for a stranger with zero prior context (D-3.4).
>
> **Sections §1–§8, §V1–§V8, §R1–§R8, and §V(2)1–§V(2)8 above are the historical record and were
> NOT rewritten.** The sole in-place edits are the two strikethrough corrections the re-review
> explicitly directed (D-3.5 — original wording preserved, struck, never deleted): the §R1 false
> claim ("the `envoy-config` tests prove `op: EQ` parses" — I-2) and the §6 CF-70-2 closure's
> elided tail sentences (M70-R8).
>
> **Cold-start:** `git status --porcelain` clean, branch `main`, `HEAD` = `origin/main` =
> `1c6a5c27352440ece41198bf7f1198788707e7bb` (the phase-70 §5 state-5 re-review commit); two
> `git fetch origin --prune` attempts timed out (GitHub transiently unreachable from this host,
> as at the state-5 session's close) but the local `origin/main` ref equals `HEAD` — no sibling
> ahead. **STEP 0.5:** `gh run list --commit <full SHA>` was also blocked by the same outage
> (`dial tcp … i/o timeout`); the state-5 session had already confirmed CI run `29526749591`
> `completed`/`success` on the FULL 40-char SHA, and this session re-confirmed CI at push time
> (see §R(2)7).

## §R(2)1. The blocking finding and what was actually wrong

`REVIEW.md` §8.3 **I-2 (Important)** — the **YAML-token → `ComparisonOp`** serde mapping (the
`#[serde(rename)]` attributes on `envoy_config::ComparisonOp`, `bootstrap.rs:747-754`) was
**unpinned for `EQ` and `LE`**: `GE` was the only token any test parsed. The I-1 fix pinned the
LOWER half of the config→runtime seam (`ComparisonOp` → `FilterOp`) but drives its table with
**Rust struct literals** that never cross the serde boundary — and its own doc comment
(`hcm.rs:4566-4568`) FALSELY asserted the upper half was covered ("the envoy-config tests pin
that `op: EQ` parses"), a claim §R1 itself contradicted (its quoted grep → zero hits). This is a
**test-coverage gap, NOT a behavioral bug**: the renames are correct as written; nothing held
them correct. The fourth instance of this phase's "a test that could not fail" defect class.

**Relied on the re-review's measurement rather than re-deriving it:** the swap mutation's
whole-suite 886/0 GREEN and the end-to-end probe RED are quoted in `REVIEW.md` §8.3; this
session reproduced the RED through the landed test (below) rather than re-measuring the
whole-suite blindness.

## §R(2)2. The fix (test + comments only — NO production change)

The seam already existed: `compiled_filter_from_bootstrap_yaml` (`hcm.rs`, Task-11) drives
production YAML through the real `envoy_config::parse_bootstrap` → validators →
`compile_access_log_filter` path. Its YAML builder `bootstrap_yaml_with_runtime_key` hard-coded
`op: GE`; it is now a thin wrapper over a new
`bootstrap_yaml_with_filter(op_token, default_value, runtime_key)` (byte-identical output for
`("GE", 500, …)`, so its two existing callers — including `no_filter_logs_every_record`'s
byte-exact `.replace()` of the filter block — are untouched and unweakened).

The NEW test `yaml_op_token_compiles_to_matching_filter_op` (`crates/envoy-http1/src/hcm.rs`)
table-drives all three tokens through that seam:

| leg | YAML token | threshold | probed statuses (`must_log`) |
|---|---|---|---|
| 1 | `EQ` | 404 | 403 → false, 404 → **true**, 405 → false |
| 2 | `GE` | 500 | 499 → false, 500 → **true**, 503 → **true** |
| 3 | `LE` | 200 | 100 → **true**, 200 → **true**, 201 → false |

Each row is uniquely satisfied by its own operator; the `LE` leg's `(100, true)` probe is the
load-bearing one (a naive `(200,true),(201,false)` table is also satisfied by `Eq 200` —
measured at the re-review, `REVIEW.md` §8.1).

**No production code changed.** The renames at `bootstrap.rs:747-754` and the mapping at
`hcm.rs:1746-1750` are byte-for-byte as they landed. ADR-0142's §E.1 boundary is untouched —
the phase-70 config surface stays CLOSED.

## §R(2)3. RED→GREEN evidence (D-3.1 — the RED was PROVEN, token by token)

Per memory `mutation-checks-collide-with-parallel-subagents`, **every mutation ran in an
isolated `git worktree --detach` at `1c6a5c2`, never in-place**. Per memory
`mutation-check-needs-forced-rebuild`, **every run was grepped for `Compiling envoy-config`**
(or, where only the test file changed, the corresponding `Compiling envoy-http1`) and the
mutation's **presence was re-grepped AFTER each run**. Per memory
`cargo-test-p-name-false-green-filtered-out`, the target was **named** (`--lib`) and every
verdict taken from the **`N passed`/`N failed` counts, never the exit code**.

| # | Run (worktree carries the NEW test) | `Compiling` | Result |
|---|---|---|---|
| — | *baseline: renames UNMUTATED* | envoy-config: 1 hit | **1 passed / 0 failed** (proves the test genuinely RUNS) |
| A | `EQ`⇄`LE` renames swapped (`#[serde(rename="LE")] Eq` / `"EQ"` Le; variant names untouched) | envoy-config: 1 hit | **RED** — `op: EQ 404 on status 403: expected should_log=false` (the re-review's proven RED, reproduced through the landed test) |
| B | same swap, table reordered in the scratch copy so the `LE` row runs FIRST | envoy-http1: 1 hit (envoy-config unchanged since A; swap re-grepped present) | **RED** — `op: LE 200 on status 100: expected should_log=true` (the `(100,true)` probe bites independently — `assert_eq!` bails at the first failing row, so run A alone leaves this leg's bite unproven) |
| C | renames restored, then `GE`⇄`LE` renames swapped | envoy-config: 1 hit | **RED** — `op: GE 500 on status 499: expected should_log=false` (the `GE` token pinned for its own reason) |
| — | *all mutations reverted (worktree diffed byte-identical to the main tree)* | envoy-config: 1 hit | **GREEN** — `1 passed / 0 failed` |

Each of the three tokens is pinned **independently**, each RED for its own distinct reason —
the same arm-by-arm discipline the re-review demanded of the I-1 fix (`REVIEW.md` §8.1). The
worktree was removed after the GREEN.

## §R(2)4. The comment corrections (D-3.5 strikethrough, never silent rewrite)

- **`hcm.rs` Task-6 doc comment (the I-2 false claim + M70-R7, same block):** both original
  claims are struck inline with the correction alongside — ~~"Each leg probes … statuses on
  BOTH sides that it must DROP"~~ (true only for `Eq`; a `Ge`/`Le` predicate cannot drop on
  both sides — wrong rationale, RIGHT table, the uniqueness conclusion holds) and
  ~~"the envoy-config tests pin that `op: EQ` parses; this is what connects the two"~~ (FALSE —
  the connection is `yaml_op_token_compiles_to_matching_filter_op`, which now exists).
- **`PROGRESS.md` §R1:** the same false claim struck in place with the correction inline (the
  self-contradiction with its own quoted zero-hit grep is named).
- **`PROGRESS.md` §6 CF-70-2 (M70-R8):** the closure's elided tail — ~~"Unreachable from
  `0076`. Owner: the next filter fixture."~~ — restored struck-through instead of silently
  absent.

## §R(2)5. The Minors folded (M70-R6/R7/R8) — and the four deliberately NOT folded

**FOLDED (all three were `REVIEW.md` §8.5's "cheap same-file folds"):**
- **M70-R6** — `rejects_status_code_filter_unknown_op` (`bootstrap.rs`) now anchors
  `msg.contains("unknown variant \`NE\`")` instead of the 2-char `contains("NE")` (which a
  future `NONE`-like token would silently satisfy). The surrounding comment records why.
- **M70-R7** — the "BOTH sides" doc-comment sentence struck + corrected (§R(2)4); same comment
  block as I-2's correction.
- **M70-R8** — the CF-70-2 elided tail struck in place (§R(2)4).

**NOT folded, and why:** **M70-R1** (the hand-maintained one-element `set_arms` array + its
overclaiming doc comment) belongs to the phase landing oneof **arm #2**, alongside **CF-70-1**
— the same surface (`REVIEW.md` §8.5 is explicit). **M70-R2** (`expected_logged_count` wiring
has no in-process witness), **M70-R4** (`AccessLog.filter` serializes as `"filter": null`), and
**M70-R9** (the first review's phase-38/phase-32 provenance error — recorded, not edited, per
D-3.5) remain reasonable carry-forwards per `REVIEW.md` §8.5/§8.7.

## §R(2)6. Verification run this session (NOT the §7.5 gate — that is the next session's job)

Scoped to the three crates this re-entry touches; **the full §7.5 gate was deliberately NOT
re-run** (§5.1 — the state-4 RE-verification is the next session, over this head).

```
$ cargo test -p envoy-http1 -p envoy-config -p envoy-accesslog --no-fail-fast
test result: ok. 102 passed; 0 failed    (envoy-accesslog)
test result: ok. 611 passed; 0 failed    (envoy-config)
test result: ok. 174 passed; 0 failed    (envoy-http1)
→ 887 passed / 0 failed

$ cargo fmt --all -- --check              (exit 0, zero bytes)
$ cargo clippy -p envoy-http1 -p envoy-config -p envoy-accesslog --all-targets --all-features -- -D warnings
(exit 0, zero warnings)
```

**887 = 886 + exactly 1** — the single new test function (`envoy-http1` 173 → 174; the other
two crates unchanged). The prior re-entry's 886 total was the number the swap mutation could
not move (`REVIEW.md` §8.3); the suite can now tell the trees apart, and goes RED the moment
either rename half is inverted (§R(2)3).

## §R(2)7. Scope discipline

- **No production code changed** — the diff is two test-module bodies
  (`crates/envoy-http1/src/hcm.rs`: the new test + the parameterized YAML builder + the doc
  comment correction; `crates/envoy-config/src/bootstrap.rs`: the M70-R6 anchor) plus docs
  (`PROGRESS.md`, `STATE.md`, `STATE_HISTORY.md`).
- **No ADR fired.** I-2 is a coverage gap, not an ambiguity; next-available remains
  **ADR-0143** (unreserved). **ADR-0142 NOT re-litigated** — the phase-70 config surface stays
  CLOSED; the fix needed no production change.
- **No fixture weakened; `known-failures.txt` untouched.**
- `#![forbid(unsafe_code)]` holds; no new dependency (D-3.2/D-3.8).

## §R(2)8. Next session

**§5 state-4 RE-VERIFICATION (2nd)** (`superpowers:verification-before-completion`) — a
SEPARATE session per §5.1; this session did NOT chain into it. A fresh context re-runs the
**full §7.5 gate** (a)–(e) over THIS re-entry's head commit and appends its evidence (the
§V(2) evidence was measured over the PRE-I-2-fix head and does not carry over). Then a
**state-5 re-review** (gate (f) — confirm I-2 is genuinely discharged BY MEASUREMENT: re-run
the `EQ`⇄`LE` rename swap in an isolated worktree and take the verdict from the `N failed`
count), then the **state-6 close-out**.

---
---

# §5 STATE-4 RE-VERIFICATION (2nd) — the full §7.5 gate re-run over the second re-entry head (SEPARATE session)

> **Written by the §5 state-4 RE-VERIFICATION (2nd) session** (`superpowers:verification-before-completion`),
> **appended to — never rewriting — §1–§8, §V1–§V8, §R1–§R8, §V(2)1–§V(2)8, and §R(2)1–§R(2)8.**
> Written for a stranger with zero prior context (D-3.4).
>
> **Why this re-run exists.** The §V(2) evidence was measured over the **PRE-I-2-fix** head
> (`2763c73`) and does NOT carry over to the second re-entry's head. Per **ADR-0127** the
> re-entry session's own scoped run (§R(2)6: `887/0`) carries **ZERO authority** here — the
> implementing context grading itself, and it never ran the workspace-global gate, the
> differential, `cargo deny`, or the fuzzer. **Every gate below was re-measured from scratch by
> this session over `60a5272e0bb55ee06fa39e35e6069d8d3e234dfe`.**
>
> **VERDICT: the §7.5 gate PASSES on every sub-gate this state owns — (a), (b), (c), (d), (e)** —
> with gate (b)'s decisive numeric CI-log cross-check discharged by the equivalent-strength
> substitute recorded in **ADR-0143** (the host's GitHub credential is invalid and GitHub serves
> Actions logs only to authenticated users; see §V(3)3). Sub-gate **(f)** is **NOT met and NOT
> this session's job**: `REVIEW.md` §8's verdict stands NOT approved until the §5 **state-5
> RE-review** (the next session) confirms I-2 is discharged by measurement.

## §V(3)1. Preconditions confirmed (disk + CI are the authority, not the handoff)

| Check | Command | Result |
|---|---|---|
| Tree clean | `git status --porcelain` | empty |
| Branch | `git branch --show-current` | `main` |
| HEAD | `git log --format=%H -1` | `60a5272e0bb55ee06fa39e35e6069d8d3e234dfe` (the second §5.2 re-entry commit) |
| Fetch | `git fetch origin --prune` | exit 0 — **the GitHub outage the re-entry session hit is OVER** |
| Unpushed commit | `git log origin/main..HEAD` | `60a5272` was STILL UNPUSHED (the re-entry's 42+ push retries all failed during the outage) |
| Push FIRST (per STEP 0) | `git push origin main` | `1c6a5c2..60a5272 main -> main`, exit 0 |
| CI on the head SHA | `gh run list --commit 60a5272e0bb55ee06fa39e35e6069d8d3e234dfe` | run `29596323921` → **`completed` / `success`** |

CI confirmed on the **FULL 40-char SHA**. Both jobs `success` with healthy step counts —
`build + test + lint` steps=15, `fuzz` steps=13 — **not** the runner-starvation signature
(`cancelled` + `runner_name:""` + `steps:0`), so the commit genuinely executed.

**Production-path identity re-verified (the premise that makes the totals meaningful):**
`compile_access_log_filter` and the `ComparisonOp` enum are **byte-identical** to the state-5
head `1c6a5c2` (diff-empty on both extracts); the whole `crates/` diff is 2 files whose hunks
all sit at `hcm.rs:4558+` / `bootstrap.rs:12962+` — inside the `#[cfg(test)]` modules. **NO
production change**, as the re-entry recorded.

## §V(3)2. Gate (e) — build / lint / format / deny — ALL CLEAN

Run **serially** (cargo's file lock), full output redirected to files — never piped through
`tail`.

| Gate | Command | Exit | Output |
|---|---|---|---|
| fmt | `cargo fmt --all -- --check` | **0** | **zero bytes** |
| build | `cargo build --workspace --all-targets` | **0** | `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 4.97s` |
| clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **0** | `grep -cE "^(warning\|error)"` → **0** |
| deny | `cargo deny check` | **0** | `advisories ok, bans ok, licenses ok, sources ok` |

No freshly-published RustSec advisory fired — no patch-bump needed.

**`cargo build -p envoy-bin` was run BEFORE any differential** (exit 0) — the harness executes
`target/debug/envoy-bin`; a stale debug binary REDs with `unknown field: filter`.

## §V(3)3. Gates (a)+(b) — `cargo test --workspace --no-fail-fast` — 2017 passed / 6 failed / 9 ignored

### An environmental incident first — the FIRST sweep attempt was VOID (Docker daemon down)

The first `cargo test --workspace --no-fail-fast` returned **1946 passed / 77 failed** — every
Docker-based differential fixture RED at once, all with the same
`failed to create a container: Error in the hyper legacy client: client error (Connect)`.
Root-caused before any adjudication (`superpowers:systematic-debugging`): the **Docker Desktop
daemon was down** — the host had rebooted, this headless session holds no logind seat, so
`/dev/kvm` carried no uaccess ACL for `esa` (only `user:gdm:rw-`), and Docker Desktop's backend
exits at its `UserCanAccessDevKVM` check. Fixed environmentally
(`sudo setfacl -m u:esa:rw /dev/kvm && systemctl --user restart docker-desktop`; daemon up,
server 28.1.1) and the ENTIRE sweep re-run from scratch — the 77-RED run carries **no
adjudication value** and none of its REDs was treated as a signal. (Recorded as memory
`docker-desktop-down-after-reboot-kvm-acl`; the ACL does not survive a reboot.)

### The adjudicated sweep

```
TEST_EXIT=101
passed=2017 failed=6 ignored=9
```

### Gate (a) — the new fixture is GREEN

`0076` passed **inside the full workspace run** (under full parallel load) **and** in isolation:

```
     Running tests/access_log_status_code_filter.rs
test access_log_status_code_filter ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.11s

$ cargo test -p differential --test access_log_status_code_filter
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.13s
```

### Gate (b) — all 6 failures adjudicated NOT-A-REGRESSION; none in the phase-70 surface

**Blast-radius check first:** the only fixture anywhere in the tree configuring a
`status_code_filter` is the new fixture itself
(`grep -rlE "status_code_filter" tests/fixtures/` → `tests/fixtures/0076-accesslog-status-code-filter`
only). None of the 6 failures touches a filter, and `0076` — the one that does — passes.

Each failure re-run **in isolation naming the target binary** (verdicts from the `N passed`/
`N failed` counts, never the exit code; the discriminator: environmental fails alone,
load-flake passes alone):

| # | Test | Isolated | Class | MEASURED evidence |
|---|---|---|---|---|
| 1–4 | `access_log_rcd_upstream_reset`, `access_log_rf_upstream_reset`, `access_log_h2_rcd_upstream_reset`, `access_log_h2_uc_upstream_reset` | **FAILS alone** (`0 passed; 1 failed`, deterministic) | environmental — IPv6-unreachable close backend (memory `tcpclosebackend-ipv6-unreachable-host-flake`) | `immediate_connect_error:_Network_is_unreachable` + `remote_address:[fdc4:f303:9324::254]:39369` — real Envoy logs a connect failure (`UF`) where envoy-rust logs a genuine reset (`UC`) |
| 5 | `admin_config_dump_server_info` | **FAILS alone** (`0 passed; 1 failed`, deterministic) | environmental — Docker bridge IP (memory `differential-host-bridge-ip-192-168-65-2`) | envoy-only stats `backend::192.168.65.2:41947::{canary,cx_active,cx_connect_fail,…}` |
| 6 | `client::tests::send_request_maps_h2_handshake_failure_to_typed_error` (`envoy-http2 --lib`) | **PASSES alone** (`1 passed; 0 failed`) | documented host flake (memory `envoyrust-h2-handshake-test-host-flake`) | `expected H2ClientHandshake, got Ok(ClientStream { host: "test.example", .. })` — the handshake unexpectedly succeeds on this host |

The membership is entirely within the documented flake families (a strict subset of the §V(2)3
set — the two §V(2)3 port-reuse members passed this run; **the RED set legitimately varies
run-to-run**). No new family.

### The decisive cross-check — discharged by the ADR-0143 substitute (GitHub log access is credential-blocked)

The prescribed numeric form — grep the CI run log for `test result:` lines and assert
`local passed+failed == CI passed == 2023` — was **attempted and is unobtainable this session**:
`gh auth status` reports **"The token in default is invalid"** (the env `GITHUB_TOKEN` is
empty), and GitHub serves Actions LOG content only to authenticated users — `gh run view
--log`, the run-level API, and the job-level API all return HTTP 403; the web job page's
per-step `data-log-url` endpoints return a login shell to an anonymous session; the run has 0
artifacts and null check-run `output`. **Only the human can restore this (`gh auth login`).**

Per **ADR-0143** the identity's substance is established by measurement through a substitute
chain of equivalent strength:

1. **The local RED set is environmental:** CI ran this EXACT tree (`60a5272…`) to
   `success` on both jobs (steps 15/13) — the workflow fails on any test failure, so every test
   CI executed passed.
2. **No test silently disappeared locally:** local enumerated `2017 + 6 = 2023` — EXACTLY the
   predicted total, where `2023 = 2022 + 1`: the parent tree's identity was measured
   numerically TWICE at 2022 (§V3, §V(2)3 — both `CI passed=2022`), and this head's whole diff
   vs that parent adds **exactly one `#[test]` function** (`+1 −0` measured on the diff:
   `yaml_op_token_compiles_to_matching_filter_op`; the added/renamed YAML-builder fns are
   non-test helpers) while `git diff 1c6a5c2..HEAD -- tests/` is **EMPTY** (the harness/fixture
   set is untouched).

**The state-5 re-review SHOULD re-run the numeric identity over this SAME SHA if the credential
is restored** (expected `CI passed=2023`) — a cheap corroborating backstop (ADR-0143).

## §V(3)4. Gate (c) — conformance — unchanged, nothing owed

```
$ git diff --stat b362bae..HEAD -- tests/conformance/ .github/
(empty)
```

`tests/conformance/h2spec/known-failures.txt` is **untouched** (21 lines) and **must not be
trimmed** (memory `h2spec-3-5-2-preface-host-sensitive`).

## §V(3)5. Gate (d) — fuzz — no new target; the corpus seed is genuinely tracked

No new fuzz target this phase (ADR-0141 PV-5) → no `ci.yml` step owed (the empty `.github/`
diff above confirms none was added). The seed verified tracked the only way that proves it:

```
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml
crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml     # PRINTS → tracked
$ git check-ignore …/status_code_filter.yaml ; echo $?
1                                                                            # NOT ignored
```

Short-budget run from the **crate dir** (`cd crates/envoy-config`):

```
$ cargo +nightly fuzz run parse_bootstrap -- -max_total_time=60
#9798   INITED cov: 16316 ft: 33878 corp: 3111/2091Kb exec/s: 4899 rss: 364Mb
#27918  DONE   cov: 16326 ft: 33909 corp: 3122/2097Kb lim: 2915 exec/s: 300 rss: 380Mb
FUZZ_EXIT=0
```

**27,918 runs, zero crashes / panics / leaks, exit 0.** CI's fuzz job independently ran
`parse_bootstrap` green on this SHA (run `29596323921`, `fuzz` job `success`, steps=13).

## §V(3)6. Gate (f) — `REVIEW.md` — NOT this state's job, and NOT met

`REVIEW.md` exists; its CURRENT verdict (§8) is **NOT approved** (0C / 1 Important I-2 / 7
Minor). The I-2 fix is what the second re-entry landed. Confirming that discharge **by
measurement** (the `EQ`⇄`LE` rename swap at `bootstrap.rs:747-754` in an ISOLATED worktree —
the landed test must go RED `op: EQ 404 on status 403`; verdict from the `N failed` count) and
re-issuing the verdict is the §5 **state-5 RE-review**'s deliverable. Per §5.1 this session did
NOT chain into it.

## §V(3)7. State-4 re-verification (2nd) verdict

**PASS on (a), (b), (c), (d), (e)** — gate (b)'s numeric CI-log cross-check discharged by the
ADR-0143 substitute (recorded, measured, scoped). **(f) unmet by design — state-5's job.** No
REAL regression: both incidents this session hit were environmental and were root-caused before
any adjudication (the Docker-Desktop/KVM daemon outage — fixed, sweep re-run from scratch; the
invalid GitHub credential — substitute evidence per ADR-0143, human action owed:
**`gh auth login`**). This session changed **no code** (its artifacts are this §V(3) section,
ADR-0143, the ledger, and the commit); **no fixture weakened; `known-failures.txt` untouched**.

Live carry-forwards are NOT gate failures and remain for the state-5 re-reviewer:
**CF-70-1**, **CF-70-3**, **M70-R1/M70-R2/M70-R4/M70-R9** (I-1 + M70-R3/R5/R6/R7/R8 CONSUMED;
CF-70-2 CLOSED).

## §V(3)8. Next session

**§5 state-5 RE-review** (`superpowers:requesting-code-review`) — a SEPARATE session per §5.1.
Its job: confirm **I-2** is genuinely discharged **BY MEASUREMENT** (never by reading the
diff), confirm M70-R6/R7/R8 are genuinely consumed, weigh the carry-forwards, and re-issue the
`REVIEW.md` verdict (a §9 or an appended section — never rewriting §1–§8). If the GitHub
credential is restored, ALSO re-run the numeric identity over `60a5272…` (expected
`CI passed=2023`, ADR-0143's corroborating backstop). If it approves → the **state-6
close-out** (its own session). If it finds an Important → a third §5.2 state-3 re-entry.
