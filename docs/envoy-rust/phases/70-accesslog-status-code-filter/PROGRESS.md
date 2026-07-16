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
- **CF-70-2 — latent `expected_lines == 0` in the differential arms.** If a future fixture
  suppressed **every** probe, `wait_file_lines(path, 0)` returns instantly and
  `read_to_string` would error on a never-created file, yielding a misleading I/O failure
  rather than a clean pass. Unreachable from `0076`. Owner: the next filter fixture.
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
