# Phase 71 — Observability family: the access-log FILTER subsystem — arm #2, `response_flag_filter`

> **Status:** `in-progress` (§5 state-1 brainstorm output). This SPEC is the
> brainstorming deliverable for a stranger with zero prior context (D-3.4).
> Every load-bearing wire/behavior claim in §0 was MEASURED against the pinned
> reference `envoyproxy/envoy:v1.33.0` (D-3.3 / D-3.7) during the state-0 recon
> of this session (two read-only recon fan-outs — one in-tree code survey, one
> live-Envoy `--mode validate` + port-mapped runtime probe); nothing here is
> asserted from memory or upstream source.
>
> **Pick + scope recorded in ADR-0144** (ledger head was ADR-0143; ADR-0144 was
> UNRESERVED — claimed by this pick). The next session is the §5 state-2
> PLAN-write (`superpowers:writing-plans`) — do NOT implement from this SPEC.

---

## §0. State-0 recon — evidence (MEASURED this session against `envoyproxy/envoy:v1.33.0`)

Phase 70 OPENED the access-log **FILTER** subsystem
(`envoy.config.accesslog.v3.AccessLogFilter`) — the per-`AccessLog`-entry
predicate deciding **whether a log record is emitted at all** — with its single
canonical arm `status_code_filter`. This phase lands the **SECOND oneof arm**,
`response_flag_filter` (`envoy.config.accesslog.v3.ResponseFlagFilter`): a sink
emits a record ONLY IF the request's response flags include one of the
configured flags. It is the textbook cheapest-strong-differential arm-#2 leaf:
it reuses the ENTIRE phase-70 `filter` seam (the `AccessLog.filter` oneof, the
`should_log` gate in BOTH HCM emit loops, the byte-exact `Http1AccessLogByteExact`
differential driver, the `expect_logged` probe field) and the mature
`%RESPONSE_FLAGS%` derivation (phases 48–65) — envoy-rust ALREADY renders `NR`
byte-exact — and yields a fully deterministic, **backend-free** byte-exact
single-line observable. Landing arm #2 also discharges the two arm-#2
obligations phase 70 deferred (CF-70-1 + M70-R1) and is the designated owner to
close CF-70-3.

### R-0.1 — the in-tree arm-#2 seam (MEASURED in-tree; `response_flag_filter` is greenfield)

`grep -rni "response_flag_filter\|ResponseFlagFilter" --include=*.rs` returns
ZERO hits — arm #2 is greenfield. The exact reuse points phase 70 left:

- **Config oneof (`crates/envoy-config/src/bootstrap.rs`).** `AccessLog.filter`
  is `Option<AccessLogFilter>` (`bootstrap.rs:701`-`711`, serde `default`).
  `AccessLogFilter` is an **Option-per-variant struct** (NOT an internally-tagged
  enum), cardinality enforced by the validator, not serde
  (`bootstrap.rs:719`-`723`):
  ```rust
  #[serde(default, deny_unknown_fields)]
  pub struct AccessLogFilter {
      pub status_code_filter: Option<StatusCodeFilter>,
  }
  ```
  Its doc comment states the arm-#2 contract literally: *"future filter-family
  phases add further `Option` arms here rather than reshaping the type."* Arm #2
  adds `pub response_flag_filter: Option<ResponseFlagFilter>` here. Today the
  `response_flag_filter` key is an unknown field on this
  `deny_unknown_fields` struct, so a `response_flag_filter` config is currently
  **boot-fatal** (`ConfigError::Yaml`), not silently ignored.
- **The zero-arm `expect()` — CF-70-1 (`crates/envoy-http1/src/hcm.rs:1741`-`1757`).**
  `compile_access_log_filter` does
  `f.status_code_filter.as_ref().expect("validated: exactly one filter arm is set")`
  (`hcm.rs:1742`-`1745`) — the doc comment admits *"the `expect` below is
  unreachable in practice; this phase ships the single `status_code_filter`
  arm."* Arm #2 MUST convert this into a full `match` over both arms
  (**CF-70-1**).
- **The runtime predicate + `should_log` seam (`crates/envoy-accesslog/src/filter.rs`).**
  `LogFilter` is a single-variant enum (`filter.rs:23`-`26`):
  `enum LogFilter { StatusCode(StatusCodeComparison) }`. The evaluate seam is
  **`should_log(&self, status: u16) -> bool`** (`filter.rs:32`-`43`) — it takes
  **only the status code**; it has NO access to the response flags. Arm #2 adds a
  `LogFilter::ResponseFlag { .. }` variant AND **widens `should_log`'s
  signature** to carry the record's response-flag token — the one genuinely-new
  plumbing this phase introduces (the call site is in the HCM emit loop, PV-3).
- **The `set_arms` cardinality array — M70-R1 (`crates/envoy-config/src/bootstrap.rs:5116`-`5130`,
  inside `validate_access_logs`).** A hand-maintained one-element array
  `let set_arms = [filter.status_code_filter.is_some()].iter().filter(...).count();`
  with an OVER-claiming `"more than one filter variant is set"` branch that can
  never fire while the array is length 1. Arm #2 converts it to a
  compiler-forcing destructuring (so a new arm can't be added without updating
  the count) and the `> 1` branch becomes reachable (**M70-R1**). The
  `runtime_key`-non-empty check immediately below (`bootstrap.rs:5131`-`5135`) is
  `status_code_filter`-specific; the new arm gets its own sibling validation.
  The `ConfigError` variants live in `crates/envoy-config/src/lib.rs:457`
  (`AmbiguousAccessLogFilter { detail }`, REUSED for the 2-arm cardinality) and
  `:463` (`EmptyStatusCodeFilterRuntimeKey`).
- **The differential driver — reusable UNCHANGED.** `AccessLogByteExactProbe`
  (`tests/differential/src/lib.rs:1102`-`1120`) already carries
  `expect_logged: bool` (default `true`); `expected_logged_count`
  (`lib.rs:1134`-`1136`) counts the `expect_logged==true` subset;
  `run_http1_access_log_byte_exact_arm` (`lib.rs:6243`) drives all probes, waits
  for exactly `expected_logged_count` lines on both files, then asserts
  byte-identical. This harness is response-flag-AGNOSTIC — a `response_flag_filter`
  fixture sets `expect_logged` per probe based on the flag emitted, **no harness
  code change required** for the positive witness (contrast phase 70, which had
  to ADD the field). The CF-70-3 hardening (R-0.5) is the only optional driver
  touch.
- **The `%RESPONSE_FLAGS%` vocabulary envoy-rust emits today (EXHAUSTIVE).**
  There is NO `ResponseFlag` enum; the flag is a single field
  `AccessLogRecord.response_flags: String` (`crates/envoy-accesslog/src/record.rs:51`),
  set by the HCM's `build_access_log_record` derive as a hardcoded string
  literal, and rendered verbatim by `Op::ResponseFlags`
  (`command_operator.rs:527`, `push_str(&record.response_flags)`). The derive
  emits exactly **6 tokens** (identical vocabulary H1 `hcm.rs:1649`-`1661` and H2
  `hcm.rs:1085`-`1101`), plus the no-flag sentinel `-`:

  | Token | Meaning | Trigger (H1 site) |
  |---|---|---|
  | `NR` | NoRoute | rcd `Some("route_not_found")` (`hcm.rs:1655`) |
  | `UH` | NoHealthyUpstream | rcd `Some("no_healthy_upstream")` (`hcm.rs:1656`) |
  | `UO` | UpstreamOverflow | rcd `…{overflow}` (`hcm.rs:1657`) |
  | `UC` | UpstreamConnectionTermination | rcd `…{connection_termination}` (`hcm.rs:1658`) |
  | `UF` | UpstreamConnectionFailure | `connect_failure` boolean (`hcm.rs:1652`) |
  | `URX` | UpstreamRetryLimitExceeded | `retry_limit_exceeded` boolean (`hcm.rs:1650`) |
  | `-` | (no flag) | `_` catch-all (`hcm.rs:1659`) |

  The value is a SINGLE token — never a multi-flag concatenation (no combination,
  brace-free — the whole `%RESPONSE_FLAGS%` doctrine, `BEHAVIOR_CONTRACT.md:1444`).
  So a `response_flag_filter` match reduces to a token-membership test:
  `configured_flags.contains(record.response_flags)`.
- **The no-route 404 (NR) is deterministic + backend-free.** Both H1 synth-404
  arms (host-miss `hcm.rs:2032`, route-miss `hcm.rs:2051`) tag rcd
  `"route_not_found"`, from which `NR` is derived at the record-build site; H2
  reuses `envoy_http1::build_response` byte-identically (no independent 404 path).
  A no-route request is a fully deterministic synth response with `%RESPONSE_FLAGS% =
  NR`, needing no cluster/backend — the ideal fixture target.

### R-0.2 — LIVE-ENVOY (`--mode validate`, networking-free): the `response_flag_filter` wire shape + flag vocabulary

Measured with `docker run … --mode validate -c cfg.yaml` (memory
`mode-validate-probes-wire-shape-networking-free`):

| `filter:` value | Result |
|---|---|
| `response_flag_filter: { flags: ["NR"] }` | **OK** |
| `response_flag_filter: { flags: [] }` (empty list) | **OK** — `flags` NOT PGV-required (no `min_items`) |
| `response_flag_filter: {}` (`flags` key absent) | **OK** — `flags` optional |
| `response_flag_filter: { flags: ["NR","UH"] }` (multiple) | **OK** |
| `response_flag_filter: { flags: ["NR","NR"] }` (duplicate) | **OK** — no uniqueness constraint |
| `response_flag_filter: { flags: ["BOGUS"] }` | **REJECTED** (PGV `in`-list) |
| `response_flag_filter: { flags: ["nr"] }` (lowercase) | **REJECTED** (case-sensitive) |

The rejection message enumerates the **complete accepted set (30 tokens)**:
`ResponseFlagFilterValidationError.Flags[0]: value must be in list
[LH UH UT LR UR UF UC UO NR DI FI RL UAEX RLSE DC URX SI IH DPE UMSDR RFCF NFCF DT UPE NC OM DF DO DR]`.

**MEASURED schema:** `ResponseFlagFilter = { flags: repeated string }`, each
element PGV-constrained to that 30-token `in` list. Upstream accepts all 30; of
those, envoy-rust can PRODUCE only the 6 in R-0.1 — the other 24 are
**parsed-but-inert** (a `flags: ["DI"]` config parses and boots but never
matches, because envoy-rust never emits `DI`; inert-correct, the same posture as
phase 70's RTDS-inert `runtime_key`). **Load-parity (§D of the phase-70 contract)
requires envoy-rust to ACCEPT all 30 tokens** — rejecting a valid-but-unproduced
token would reject a config upstream accepts. `BOGUS`/lowercase MUST be
rejected fail-loud (parity with the PGV `in` list).

### R-0.3 — LIVE-ENVOY: `response_flag_filter` and `status_code_filter` are mutually-exclusive oneof arms

A `filter:` carrying BOTH `status_code_filter` and `response_flag_filter` →
**REJECTED** at JSON→proto parse: *"`status_code_filter` … has already been set
(either directly or as part of a oneof)."* So the two are sibling arms of the
`AccessLogFilter` oneof — exactly one permitted. This is the cardinality the
`set_arms` destructuring (M70-R1) now enforces across TWO arms (`> 1` becomes
reachable → `ConfigError::AmbiguousAccessLogFilter`).

### R-0.4 — LIVE-ENVOY (runtime, port-mapped, no backend): `response_flag_filter { flags: ["NR"] }` suppression is deterministic + byte-exact

Booted live `envoyproxy/envoy:v1.33.0` (`docker -p`, memory
`state0-recon-docker-needs-port-mapping`): ONE HCM listener, a file access log
(`text_format_source` = `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%
FLAGS=%RESPONSE_FLAGS%\n`) + `response_flag_filter: { flags: ["NR"] }`, and ONE
`direct_response` route (`/direct` → 503). No cluster / no backend. Drove two
requests:

| Request | HTTP status | `%RESPONSE_FLAGS%` | logged? |
|---|---|---|---|
| `GET /nowhere` | 404 | `NR` (no route matched) | **YES** |
| `GET /direct` | 503 | `-` (clean `direct_response`) | **NO** (no NR flag) |

The access-log FILE (captured after a full graceful drain, R-0.5) contained
EXACTLY ONE line: `STATUS=404 PATH=/nowhere FLAGS=NR` (34 bytes).

**Controls:** `flags: ["UH"]` (a flag neither request produces) → the file is
EMPTY (both suppressed). NO `filter` → BOTH lines present
(`…FLAGS=NR` and `…FLAGS=-`). So the filter gates on the record's response
flags: it KEEPS a record whose flag ∈ `flags`, DROPS one whose flag ∉ `flags`
(including the `-` no-flag sentinel). This inverts phase 70's fixture (there the
kept record carried `-`; here the kept record carries a NON-trivial flag `NR` —
a strictly stronger observable).

### R-0.5 — LIVE-ENVOY: the FileAccessLog async-flush confound (empirically reproduces CF-70-3)

Measured this session: `FileAccessLog` does NOT flush every record immediately.
The FIRST record after boot appears near-instantly, but subsequent records batch
and only reach the file after a **~10-second** flush interval (a control with two
kept records showed the 2nd line only at ~t+12s, not t+1s). A byte-exact harness
that reads too soon sees a **false "suppressed"** for a record that is merely
un-flushed. Sound ways to force a complete flush observed here: wait past the
~10s interval, or a graceful `docker stop` (SIGTERM) drain (the R-0.4 witness
used graceful stop, proving the `/direct` absence is genuine suppression, not a
pending buffer). This is EXACTLY the mechanism behind **CF-70-3** (the phase-70
`wait_file_lines` false-pass-only window). Phase 71, as "the next access-log-filter
phase," is the designated owner to close it (§2.1 item 6 / PV-7).

### R-0.6 — numbering

Next ROADMAP id **71** (highest defined is `70`; `59`/`60`/`62` are intentional
gaps). Next fixture id **0077** (`0076` is the last). Next ADR **ADR-0144**
(ledger head `ADR-0143`; UNRESERVED — claimed by this pick).

---

## §1. Goal

Land the second `AccessLogFilter` oneof arm, `response_flag_filter`
(`envoy.config.accesslog.v3.AccessLogFilter.response_flag_filter`), behaviorally
equivalent to `envoyproxy/envoy:v1.33.0` under the differential contract (§7):

- An `AccessLog` entry MAY carry `filter: { response_flag_filter: { flags: [...] } }`;
  when present, a log record is emitted to that sink ONLY IF the record's
  response flag is one of `flags`. A record whose flag ∉ `flags` (including the
  `-` no-flag sentinel) produces NO line for that sink (R-0.4). The match is a
  token-membership test over the single `response_flags` string envoy-rust
  renders (R-0.1).
- `flags` accepts the full measured 30-token vocabulary for LOAD PARITY (R-0.2);
  the 24 tokens envoy-rust cannot produce are parsed-but-inert. An unknown token
  (`BOGUS`, lowercase) is fail-loud (`ConfigError`, R-0.2).
- `response_flag_filter` and `status_code_filter` are mutually-exclusive oneof
  arms — exactly one permitted (R-0.3), enforced by the `set_arms` destructuring
  (M70-R1); the zero-arm `expect()` becomes a full 2-arm match (CF-70-1).
- Reuse the entire phase-70 `filter` seam + the `Http1AccessLogByteExact`
  differential driver; the ONLY new runtime plumbing is widening `should_log` to
  see the response-flag token (R-0.1 / PV-3). Close CF-70-3 (R-0.5 / PV-7).

**Differential surface at phase end:** a new fixture `0077` witnessing
`response_flag_filter { flags: ["NR"] }` byte-exact — an H1 HCM with a filtered
file access log + one `direct_response` route + a no-route probe (404 NR **kept**,
503 `-` **suppressed**), asserting the log file across both proxies is the SAME
single byte-identical `…FLAGS=NR` line — plus in-process coverage of the
membership semantics (kept/dropped per flag, the `-` sentinel never matches, the
inert 24-token acceptance), the oneof cardinality (now 2 arms) + unknown-token
rejections, and the CF-70-3 suppression-robustness hardening.

---

## §2. Scope

### 2.1 In scope

1. **Config schema (`crates/envoy-config`).** Add
   `AccessLogFilter.response_flag_filter: Option<ResponseFlagFilter>` (the
   Option-arm precedent, `bootstrap.rs:722`). New type
   `ResponseFlagFilter { flags: Vec<...> }` under
   `#[serde(deny_unknown_fields)]`. PV-1 decides the flag representation: a
   validated `Vec<String>` (store the raw token, membership-match by string —
   lighter, matches the "parsed-but-inert" posture) vs. a 30-variant
   `ResponseFlag` enum (the `ComparisonOp` type-safe precedent, but verbose).
   `flags` is optional/defaultable (R-0.2); empty-`flags` semantics per PV-6.
2. **Validation (`crates/envoy-config`).** (a) Each `flags` token ∈ the 30-token
   set (R-0.2) → else a fail-loud `ConfigError` (a new
   `UnknownResponseFlag { token }`, mirroring `EmptyStatusCodeFilterRuntimeKey`;
   native message OK per ADR-0049). (b) The `AccessLogFilter` oneof cardinality
   now spans 2 arms: convert the `set_arms` array to a compiler-forcing
   destructuring (M70-R1), making the `> 1` → `AmbiguousAccessLogFilter` branch
   reachable and the `== 0` branch unchanged (PV-2).
3. **Compile + runtime predicate (`crates/envoy-accesslog` + HCM).** (a) Convert
   `compile_access_log_filter`'s zero-arm `expect()` (`hcm.rs:1742`) into a full
   `match` over `{status_code_filter, response_flag_filter}` (CF-70-1). (b) Add a
   `LogFilter::ResponseFlag { flags: <set> }` variant (`filter.rs:24`). (c)
   **Widen `should_log`** (`filter.rs:32`) so it receives the record's
   response-flag token in addition to the status (PV-3 fixes the exact signature:
   pass the `&AccessLogRecord`, or `(status, response_flags: &str)`), and
   implement the `ResponseFlag` arm as `flags.contains(response_flags)` with the
   empty-`flags` semantics from PV-6. `status_code_filter` behavior is
   byte-unchanged. (d) Thread the widened call at the per-sink emit gate in BOTH
   HCM emit loops (H1 `hcm.rs`, H2 `crates/envoy-http2/src/hcm.rs`, PV-3) —
   the record already carries `response_flags`, so no new derivation.
4. **Differential fixture `0077-accesslog-response-flag-filter`.** An H1 HCM
   listener with a file access log (deterministic `text_format_source`, the
   `0040`+/`0076` discipline) + `response_flag_filter: { flags: ["NR"] }` + one
   `direct_response` route (`/direct` → 503) + a no-route probe (`/nowhere` →
   404 NR). Two probes: `/nowhere` (`expect_logged: true`), `/direct`
   (`expect_logged: false`). Assert the log file across both proxies is the SAME
   single byte-identical `STATUS=404 PATH=/nowhere FLAGS=NR` line, via the
   EXISTING `Http1AccessLogByteExact` driver (no harness change for the positive
   witness).
5. **In-process coverage.** The `should_log` `ResponseFlag` membership across the
   6 producible tokens (a record with `NR` matches `flags:["NR"]` and
   `flags:["UH","NR"]`, misses `flags:["UH"]`; the `-` sentinel matches nothing
   non-empty); the inert 24-token acceptance (a `flags:["DI"]` config boots and
   never matches); the empty-`flags` semantics (PV-6); the oneof cardinality
   (zero-arm → `AmbiguousAccessLogFilter`, two-arm → `AmbiguousAccessLogFilter`)
   + unknown-token (`BOGUS`, lowercase) rejection; a no-`filter` sink still
   logging every record (regression); `status_code_filter` unchanged (regression).
6. **CF-70-3 closure (differential driver hardening).** Harden the suppression
   assertion so a filter-DROPPED record cannot false-pass as suppressed when it
   is merely un-flushed (R-0.5): after confirming `expected_logged_count` lines,
   ALSO assert the file does not reach `expected+1` under a bounded settle — the
   SOUND mechanism given the ~10s batch (PV-7: a short-budget `!wait_file_lines`
   can itself false-pass; the robust design either drives an ordering witness
   [a KEPT probe after the dropped one, whose flush proves the dropped one would
   have flushed] or sets a deterministic flush trigger). Keep the change surgical
   — do NOT disturb the 30 existing access-log fixtures or fixture `0076`.
7. **`BEHAVIOR_CONTRACT.md`** — a `response_flag_filter` subsection under the
   access-log filter section (§6), sibling to the phase-70 `status_code_filter`
   subsection.
8. **`known-failures.txt` / conformance** — unchanged (no protocol-conformance
   surface; never trimmed, memory `h2spec-3-5-2-preface-host-sensitive`).

### 2.2 Out of scope (deliberate, with rationale)

- **The remaining `AccessLogFilter` variants** — `duration_filter`,
  `header_filter`, `not_health_check_filter`, `and_filter`, `or_filter`,
  `grpc_status_filter`, `runtime_filter`, `metadata_filter`, `traceable_filter`,
  `log_type_filter`. Each is a future cheapest-strong leaf reusing this phase's
  now-2-arm oneof + widened `should_log` seam (§10 notes the natural next picks).
- **Producing the 24 currently-unproduced response flags** (`DI`, `FI`, `RL`,
  `LH`, `SI`, `DPE`, …). Those are wire-accepted-but-inert (R-0.2); making
  envoy-rust EMIT them is separate feature work in the paths that would set them,
  wholly outside the access-log-filter subsystem.
- **The RTDS `runtime_filter` variant / any runtime override** — envoy-rust has
  no runtime subsystem (the same boundary phase 70's `runtime_key` documented).
- **H2 `response_flag_filter` differential fixture** — the widened `should_log`
  gate is codec-agnostic (it reads `record.response_flags`, derived identically
  on H1/H2, R-0.1), so it is inert-correct on H2 and the H2 emit-loop wiring IS
  done (PV-3) so H2 does not regress; a dedicated H2 filtered fixture is deferred
  (the opener is H1, the simplest driver) — the same disposition phase 70 took.

### 2.3 §7.4 fuzz disposition

The `filter` surface reuses the `parse_bootstrap` parser (a new sub-message
field + a token-validation branch). **Default projection:** extend/replace the
phase-70 `parse_bootstrap` corpus seed to carry an
`access_log[].filter.response_flag_filter.flags` (exercising the new
token-validation path) — **no new fuzz target** (the phase-68/69/70 precedent,
ADR-0137: a config-only sub-message rides the existing `parse_bootstrap`
target). There is NO new byte-parser this phase, so a dedicated target is NOT
expected; **confirm at the state-2 PLAN-write** (PV-5). A new/edited seed must be
`!`-un-ignored (memory `fuzz-corpus-seed-gitignored-by-default`); a new target
(not expected) would need a hand-wired `ci.yml` step (memory
`new-fuzz-target-needs-a-ci-yml-step`).

---

## §3. PLAN-VERIFY items (re-confirm against the live tree at the state-2 PLAN-write)

- **PV-1 — the `ResponseFlagFilter` serde model + token validation.** Decide a
  validated `Vec<String>` (membership-match by string, lighter) vs. a 30-variant
  `ResponseFlag` enum (the `ComparisonOp` precedent). Author the token-membership
  validator against the 30-token set (R-0.2) → the new
  `ConfigError::UnknownResponseFlag`. Confirm where in `validate_access_logs`
  (`bootstrap.rs:5109`+) the per-arm validation attaches (sibling to the
  `runtime_key` check at `bootstrap.rs:5131`).
- **PV-2 — the `set_arms` destructuring (M70-R1).** Convert
  `bootstrap.rs:5116`-`5130`'s one-element array to a compiler-forcing
  destructuring over `{status_code_filter, response_flag_filter}` so the count is
  compiler-checked; the `== 0` and `> 1` branches both map to
  `AmbiguousAccessLogFilter { detail }` (R-0.3 proves the `> 1` case is a real
  upstream rejection). Fix the over-claiming doc comment.
- **PV-3 — widen `should_log` + thread both HCM emit loops (CF-70-1).** Fix the
  exact widened signature (pass `&AccessLogRecord` vs `(status, response_flags:
  &str)`; the record carries `response_flags: String`, `record.rs:51`). Convert
  the `compile_access_log_filter` `expect()` (`hcm.rs:1742`) to a full 2-arm
  match. Locate BOTH emit-loop call sites (H1 `hcm.rs` per-sink gate; H2
  `crates/envoy-http2/src/hcm.rs` sibling) and thread the widened call. Confirm
  the record already carries the final response-flag token at the gate on both
  paths (R-0.1 says yes).
- **PV-4 — the membership + `-` sentinel semantics.** Confirm
  `flags.contains(record.response_flags)` is the correct match, that the `-`
  no-flag sentinel matches nothing (a non-empty `flags` never contains `-`,
  because `-` ∉ the 30-token set), and that the 6 producible tokens each
  match/miss as measured (R-0.4). Re-confirm envoy-rust renders a SINGLE token
  (never a combination) so containment is exact.
- **PV-5 — §6.1 size re-derivation + §7.4.** Re-estimate net LoC / task count
  against the live tree (§8). A SMALL leaf (~450-650 LoC) — a split is very
  unlikely; PV-5 re-derives (ADR-0146 held in reserve as a formality). Confirm
  the §7.4 disposition (corpus seed edit, no new target).
- **PV-6 — empty-`flags` semantics (MEASURE at state-2).** R-0.2 proved
  `flags: []` and absent-`flags` both VALIDATE, but the state-0 recon did NOT
  measure their RUNTIME behavior. Upstream's `ResponseFlagFilter` documents
  "if empty, matches any response flag set." **PLAN-write MUST live-measure**
  (port-mapped, drive an NR 404 and a clean `-` 503 through an empty-`flags`
  filter) whether empty-`flags` matches ANY-flag (keep the 404, drop the `-`) or
  matches NONE, and model it correctly (load-parity forbids rejecting the
  upstream-accepted empty list). The opener fixture uses a NON-empty
  `flags:["NR"]`; the empty-`flags` behavior is pinned in-process once measured.
- **PV-7 — the CF-70-3 sound closure mechanism.** R-0.5 measured the ~10s
  FileAccessLog batch, so a naive short-budget `!wait_file_lines(expected+1)`
  can itself false-pass (the dropped record simply hasn't flushed). Decide the
  SOUND design: (a) an ordering witness — drive a KEPT probe AFTER the dropped
  one so the kept record's flush proves the dropped one would have flushed by
  then; or (b) a deterministic flush trigger; or (c) a graceful-drain-then-read.
  Keep it surgical and not disturbing the 30 existing access-log fixtures.

---

## §4. Rejected / deferred alternatives (what this pick was chosen over)

- **`header_filter`** (the other strong access-log-filter arm). Reuses the
  phase-04.2 header matcher and is deterministic, BUT gates on REQUEST headers
  and needs the full `HeaderMatcher` config surface (name + `string_match`
  {exact/prefix/safe_regex/present}) modeled and validated — a materially larger
  schema than `ResponseFlagFilter { flags: [...] }`, and it does NOT discharge
  the arm-#2 obligations any more cheaply. A strong NEXT leaf, not the cheapest.
- **`duration_filter`** — timing-based (`op(request_duration_ms, threshold)`);
  the differential needs latency bounds and is flaky; deferred to a
  timing-tolerant phase (the same rationale that deferred retry `per_try_timeout`
  / fault `fixed_delay` at the phase-70 pick).
- **`and_filter` / `or_filter`** — recursive `AccessLogFilter` composition;
  heavier (a nested oneof), a natural phase once ≥2 leaf arms exist (this phase
  makes it 2).
- **The standalone H2 `status_code_filter`/`response_flag_filter` differential**
  (the ADR-0140 §2.2 deferral) — the composition is already measured
  byte-identical and the gate is codec-agnostic, so as a standalone phase it is
  weak; better folded into a future H2 access-log-filter fixture set.
- **Re-weighed and still rejected from the phase-70 pick** (re-verified against
  the live tree): outlier `consecutive_local_origin_failure` (needs the unmodeled
  local-origin signal path), CB `max_pending_requests>0` (real pending queue
  unbuilt; timing-flaky), the `upstream_rq_overflow` stat rename (too thin,
  reopens phase-17), retry `retriable_headers` (needs a controllable retriable
  backend), and each §9 family opener (network-filter payload codecs,
  `sni_cluster`, non-deterministic LB, HTTP/3+QUIC, gRPC bridge/transcoding,
  observability SINKS [gRPC ALS, OTLP], runtime/RTDS, hot-restart, WASM host) —
  each a large new subsystem far above the cheapest-strong-differential bar.

**`response_flag_filter` wins:** it is the cheapest arm-#2 leaf — it reuses the
ENTIRE phase-70 `filter` seam + the `%RESPONSE_FLAGS%` derivation envoy-rust
ALREADY renders byte-exact (`NR`), needs the `expect_logged` driver UNCHANGED for
its positive witness, and yields a fully deterministic BACKEND-FREE byte-exact
single-line observable whose KEPT record carries a NON-trivial flag (`NR`) — a
strictly stronger witness than phase 70's `-`-flagged kept record. It discharges
the two deferred arm-#2 obligations (CF-70-1 + M70-R1) and is the designated
owner to close CF-70-3, introducing exactly one new runtime seam (the widened
`should_log`).

---

## §5. Differential surface at phase end

- **NEW fixture `0077-accesslog-response-flag-filter`** — green cross-proxy: an
  H1 HCM listener with a file access log (deterministic `text_format_source`
  `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% FLAGS=%RESPONSE_FLAGS%\n`) +
  `response_flag_filter { flags: ["NR"] }` + one `direct_response` route
  (`/direct` → 503). Two probes: `GET /nowhere` (404 NR, `expect_logged: true`)
  and `GET /direct` (503 `-`, `expect_logged: false`). The access-log file across
  BOTH proxies is asserted the SAME single **byte-identical** line
  `STATUS=404 PATH=/nowhere FLAGS=NR` (the `/direct` 503 SUPPRESSED), via the
  EXISTING `Http1AccessLogByteExact` driver + the CF-70-3 suppression hardening.
- **All pre-existing fixtures `0001`–`0076` stay green** — a sink with no
  `filter` behaves exactly as today; `status_code_filter` (fixture `0076`) is
  byte-unchanged; no existing fixture sets a `response_flag_filter` (§7.5 (b)).
- **In-process:** the `should_log` `ResponseFlag` membership over the 6 producible
  tokens + the `-` sentinel + the inert 24-token acceptance + the empty-`flags`
  semantics (PV-6); the oneof cardinality (zero-arm + two-arm) + unknown-token
  fail-loud rejections; the no-`filter`-still-logs regression; the
  `status_code_filter`-unchanged regression.

**Why the differential needs no backend:** the strong deterministic byte-exact
observable comes from a `direct_response` 503 (dropped, flag `-`) and a no-route
404 (kept, flag `NR`) — no cluster, no upstream (R-0.4), mirroring fixture
`0076`.

---

## §6. `BEHAVIOR_CONTRACT.md` additions

A `response_flag_filter` subsection under the access-log filter section (sibling
to the phase-70 `status_code_filter` subsection), recording the MEASURED facts
(R-0.2–R-0.5): `filter: { response_flag_filter: { flags: [<30-token set>] } }`
gates emission per sink; a record is KEPT iff its single `%RESPONSE_FLAGS%` token
∈ `flags` (the `-` no-flag sentinel matches nothing non-empty); `flags` accepts
the full 30-token vocabulary for load-parity, of which envoy-rust produces only
`{NR, UH, UO, UC, UF, URX}` (the other 24 parsed-but-inert); `BOGUS`/lowercase
tokens are fail-loud; `response_flag_filter` and `status_code_filter` are
mutually-exclusive oneof arms; the empty-`flags` semantics (per PV-6 measurement);
a no-route 404 renders `NR` (kept by `flags:["NR"]`), a clean `direct_response`
503 renders `-` (dropped); a sink with no `filter` logs every record (unchanged).

---

## §7. ADR reservations

- **ADR-0144 (FIRED this session):** the phase-71 pick + scope + rejected
  alternatives (this SPEC's decisions).
- **ADR-0145 (reserved):** the §6.2 empirical-verification reconciliation at the
  state-2 PLAN-write (PV-1..PV-7 resolutions — the `ResponseFlagFilter` serde
  model + token validator, the `set_arms` destructuring, the widened `should_log`
  + the 2-arm compile match + both HCM emit loops, the membership/`-` semantics,
  the empty-`flags` measurement, the CF-70-3 sound closure).
- **ADR-0146 (reserved):** the §6.1 split, if PV-5 fires it (very unlikely — a
  small single-phase arm-#2 leaf).

---

## §8. Estimated size (for the §6.1 split gate at state-2)

| Area | Net LoC (rough) |
|---|---|
| `envoy-config`: `ResponseFlagFilter` schema + the `response_flag_filter` oneof arm | ~70 |
| `envoy-config`: token validator + `set_arms` destructuring (M70-R1) + `ConfigError::UnknownResponseFlag` | ~70 |
| `envoy-accesslog` + HCM: `LogFilter::ResponseFlag` + the widened `should_log` (CF-70-1 match) + both emit-loop call-site threads | ~90 |
| fixture `0077` (2 YAMLs + expectations + README) — reuses the driver | ~120 |
| differential driver CF-70-3 suppression hardening (PV-7) | ~40 |
| in-process tests (membership + `-` sentinel + inert tokens + empty-flags + cardinality + unknown-token + regressions) | ~180 |
| `BEHAVIOR_CONTRACT.md` + ROADMAP/docs | ~60 |
| **Total** | **~630 net LoC / ~10–13 tasks** |

Well UNDER the ~1500 LoC / ~25 task gate — a **single phase**, no split projected
(lighter than phase 70 ~670; the driver `expect_logged` field already exists, and
there is no new codec/primitive — only a config sub-message, a predicate variant,
a signature widening, and one bounded driver hardening). PV-5 re-derives at the
state-2 PLAN-write; ADR-0146 is held in reserve as a formality only.

---

## §10. Carry-forwards

**CONSUMED by this pick** (arm-#2 obligations phase 70 attached to WHICHEVER phase
lands the 2nd `AccessLogFilter` oneof arm): **CF-70-1** (the zero-arm `expect()` →
full match), **M70-R1** (the `set_arms` destructuring + over-claiming doc
comment). **CLOSED by this pick:** **CF-70-3** (the `wait_file_lines`
false-pass-only window — §2.1 item 6 / PV-7; this IS "the next access-log-filter
phase").

**NOT consumed** (owner = whatever future phase touches their surface; this phase
touches the access-log config + emitter + the differential driver):

- **M70-R2** (`expected_logged_count` has no in-process witness), **M70-R4**
  (`"filter": null` serialization), **M70-R9** (the phase-38/32 provenance note)
  — access-log-adjacent; weigh at state-2 whether any is cheaply folded in, else
  carry forward.
- **M69-A..I** — gRPC-HC doc/coverage polish. **Not touched here.**
- **CF-69-1/2/3/5** — the phase-69 documented boundaries / correct divergences.
  **Not touched here.**
- **M68-1** — empty-hex `text:""` TCP-HC validator gap. **Not touched here.**
- **M-1** — the `CidrRange` `prefix_match` guard band. **Not touched here.**
- **CF-67-3/5/6/7** (incl. the TLS `[rbac, tcp_proxy]` establishment ordering) —
  the older Minors in `67.3/SPEC.md` §10 + the HTTP-filters-family (1)–(4) in
  `STATE.md` `## Notes`. **Not touched here.**

**Natural next cheapest-strong leaves this phase UNLOCKS** (each reuses the
now-2-arm `filter` oneof + the widened `should_log` seam, none an obligation):
`header_filter` (reuses the phase-04.2 header matcher), `duration_filter`
(timing), `and_filter`/`or_filter` (recursive composition — now that 2 leaf arms
exist), and the H2 access-log-filter differential.
