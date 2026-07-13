# Phase 68 — `68-upstream-tcp-health-check` — REVIEW

**Status: APPROVED** — 0 Critical / 0 Important / 2 Minor. The phase does **NOT**
re-enter §5 state 3. The next session is the §5 **state-6 close-out**.

> Produced by the §5 **state-5 code-review** session
> (`superpowers:requesting-code-review`), 2026-07-13. Pick + scope locked by
> **ADR-0136**; the §6.2 empirical reconciliation (PV-1..PV-6) by **ADR-0137**;
> the §6.1 split did NOT fire (**ADR-0138** reserved-unfired, single-phase).
>
> **STEP 0 (disk is authoritative):** `git status --porcelain` clean; branch
> `main`; `HEAD` = `origin/main` =
> `4331fecf3d03f0853739096a27d43286cda4fdf2` (the phase-68 state-4 verification
> docs commit). `git fetch origin --prune` → `0 0` ahead/behind; no sibling
> autonomous-loop session had written `REVIEW.md`, and ROADMAP row `68` is still
> `in-progress`.
>
> **STEP 0.5 (CI, FULL 40-char SHA):** the HEAD docs commit's CI run
> `29230120434` is `completed`/`success`; the code was already proven at the
> state-3 green commit `9ac38d8` (CI run `29216603720` = `success`).
>
> Per `BOOTSTRAP_PROMPT.md` §5.1 (one state per session) the state-6 close-out
> was deliberately **NOT** run here; no `stop` file was created (the mission is
> far from complete).
>
> **The §7.5 gate was NOT re-run.** It ran at state-4 and it PASSES; its evidence
> is quoted verbatim in `PROGRESS.md` §"§5 state-4 verification". This session
> changed **no code** — the review is read-only over the tree (re-confirmed after
> the adversarial subagent returned: `git status --porcelain` empty, `HEAD`
> unmoved). All live-probe artifacts were written to the session scratchpad, never
> the repo.

---

## Verdict

**0 Critical / 0 Important / 2 Minor.** The phase-68 diff (commits
`66f011c`..`701049e` + the state-advance `f167f23` + the fmt/clippy follow-ups
`df863e6`/`9ac38d8`, base `733d156`) implements every SPEC §2.1 in-scope item
(1–8) and every one of `PLAN.md`'s 7 tasks, and — verified by **live probes
against a running `target/debug/envoy-bin` and reference `envoyproxy/envoy:v1.33.0`**
— is behaviorally equivalent to Envoy on every composition it claims. Two Minor
findings (below) are carry-forwards; neither has a differential observable on any
config a user actually writes, and neither is a regression to landed 12/67 work.

Per `superpowers:requesting-code-review` ("Fix Critical immediately / Fix
Important before proceeding / **Note Minor for later**") and the standing
precedent (phase-64 APPROVED 0/0/2, phase-65 0/0/1, phase-66 0/0/5 — all closed
at state-6 without re-entry), **zero Critical + zero Important ⇒ the phase does
not re-enter state 3.** §5's "if issues → back to step 3" has never been read as
"zero Minors."

---

## Grading — SPEC §2.1 in-scope (1–8) × `PLAN.md` tasks

Every item was read in the landed source (not merely trusted from `PROGRESS.md`).

| SPEC §2.1 | Landed | Where | Verdict |
|---|---|---|---|
| 1. Config schema (`TcpHealthCheck`, `HealthCheckPayload` hex/base64 decode) | ✓ | `bootstrap.rs` `TcpHealthCheck`/`HealthCheckPayload`/`decode()`/`decode_hex()`; `+base64 = "0.22"` | pass |
| 2. Validation (both-checkers oneof; neither→Unsupported; shared timing/thresholds; message + pinning-test update) | ✓ | `validate_health_checks` (`bootstrap.rs:4766`), `lib.rs` `BothHttpAndTcpHealthCheck` + reworded `UnsupportedHealthCheckType`; pinning test repointed `tcp_health_check`→`grpc_health_check` | pass |
| 3. TCP probe (connect / optional send / receive-scan / connection-only; ONE `timeout`) | ✓ | `probe.rs` `tcp_probe_once`/`tcp_probe_loop`/`receive_matches`/`find_subslice`; `+net`/`io-util` tokio | pass |
| 4. Dispatch wiring (checker-type selection) | ✓ | `scheduler.rs` `Scheduler::spawn` `(http_cfg, tcp_cfg)` match; ejection/`pick()`/counters untouched | pass |
| 5. Differential fixture `0074` | ✓ | `tests/fixtures/0074-*` + `DEAD_BACKEND_PORT` marker (`tests/differential/src/lib.rs`) + runner `upstream_tcp_health_check.rs` | pass |
| 6. In-process coverage (decode + rejections + conn-only + receive-match + mismatch) | ✓ | 6 decode + 3 parse + 5 validator tests (`bootstrap.rs`); 3 matcher + 5 probe-integration tests (`probe.rs`); 1 scheduler test | pass |
| 7. `BEHAVIOR_CONTRACT.md` subsection | ✓ | `## Active TCP health check` + "68 entries" stat-map note | pass |
| 8. `known-failures.txt`/conformance unchanged | ✓ | not touched (correct) | pass |

**Commit granularity (recorded, accepted):** Tasks 2+3 landed in ONE green commit
(`a454ab0`) because the Task-2 parse tests route through the validator, which
cannot accept a TCP-only checker until the Task-3 restructure — green-commit
discipline D-3.6. Documented in `PROGRESS.md` §"Deviation". The PLAN also split
the field-add and validator into two commit steps; the merge is faithful to the
intent (all Task-2 and Task-3 tests present and green). Accepted.

**Additional file beyond PLAN's File Structure:** the fixture runner
`tests/differential/tests/upstream_tcp_health_check.rs` (mirrors the 0019
runner). PLAN Task 6 named only the fixture dir + harness marker; the runner is
required for the fixture to execute (memory `new-fuzz-target-needs-a-ci-yml-step`
analogue for differential runners). Correct and necessary; noted, not a defect.

---

## Live-probe evidence (the heart of the review)

Per memory `state5-must-probe-untested-compositions`, a green §7.5 gate proves
the code does what its tests ask, not that the tests ask the right question. The
differential fixture `0074` exercises ONLY the connection-only refused path; the
`send`/`receive` scan is in-process-only (ADR-0137 PV-3). So I booted a live
`target/debug/envoy-bin` (rebuilt first — memory
`differential-harness-uses-debug-envoy-bin`) with six TCP-HC clusters against
local banner/echo backends and read the admin `/stats`, comparing to the
recon-measured Envoy table (SPEC §0 R-0.5):

| Composition | envoy-rust `/stats` | Measured Envoy (R-0.5) | Match |
|---|---|---|---|
| `receive:[PING]`, backend sends `PING` | success↑, `membership_healthy:1` | healthy | ✓ |
| `receive:[PONG]`, backend sends `PING` | failure↑, `membership_healthy:0` | `active_hc_timeout` unhealthy | ✓ |
| connection-only `{}`, live backend | success↑, `membership_healthy:1` | healthy | ✓ |
| connection-only `{}` → refused port | failure↑, `membership_healthy:0` | `/failed_active_hc` unhealthy | ✓ |
| `send:hi` + `receive:OKOK`, echo backend | success↑, `membership_healthy:1` | healthy | ✓ |
| `send:hi`, empty `receive` (send-only) | success↑, `membership_healthy:1` | healthy (send-then-close) | ✓ |

All six match. The `.attempt`/`.success`/`.failure` counters and
`membership_healthy`/`membership_total` gauges are the phase-12 names — no new
stat names, as claimed.

**Fail-loud config paths** (envoy-bin writes `ConfigError` to STDOUT), all
producing the correct typed variant:

| Config | envoy-rust | Envoy v1.33.0 |
|---|---|---|
| `http_health_check` + `tcp_health_check` both | `BothHttpAndTcpHealthCheck` | reject (oneof already set) — re-confirmed via `--mode validate` |
| `send:{text:"zzzz"}` (non-hex) | `InvalidHealthCheckPayloadHex … 'zzzz'` | `invalid hex string` |
| `send:{text:"0"}` (odd) | `InvalidHealthCheckPayloadHex … '0'` | `invalid hex string` |
| `receive:[{}]` (neither field) | `EmptyHealthCheckPayload` | `payload is required` |
| `send:{text:"00",binary:"AA=="}` (both fields) | `EmptyHealthCheckPayload` | reject (oneof already set) |
| `send:{binary:"!!!!"}` (bad b64) | `InvalidHealthCheckPayloadBase64` | reject |
| neither http nor tcp | `UnsupportedHealthCheckType` (reworded) | n/a |
| unknown `grpc_health_check` key | serde `deny_unknown_fields` reject | reject |

**Fixture `0074`** ran GREEN on isolated re-run (the H1 listener served synth-503
after the sole endpoint was ejected — envoy-rust logged
`no healthy endpoint for cluster … tcp_hc_backend`). A first run false-RED with
`127.0.0.1:55000 not accept-ready within 10s: Connection refused` — the documented
proxy-startup race (`eds-fatal-startup-test-port-reuse-flake` /
`differential-fixtures-flake-under-parallel-load`), not a health-check-logic
failure; isolated rerun → `ok`.

**Adversarial refutation** (independent `general-purpose` subagent, tasked to
REFUTE the parity claim against real Envoy): confirmed parity on both-fields-set,
http+tcp, empty-`receive`-element, uppercase hex, space/`0x`-prefixed hex, and
reasoned the runtime send-only / EOF-vs-timeout / split-receive / multi-block
cases to outcome-parity. It surfaced ONE real divergence → **M68-1** below (which
I then reproduced and confirmed against real Envoy myself — I did not take the
finding on trust).

---

## Findings

### M68-1 (Minor) — empty-hex `text: ""` is accepted by envoy-rust, load-fatal in Envoy

**Measured, both sides:**

- Envoy `envoyproxy/envoy:v1.33.0 --mode validate` REJECTS
  `tcp_health_check: { send: { text: "" } }` and `{ receive: [ { text: "" } ] }`
  with `PayloadValidationError.Text: value length must be at least 1 characters`
  (`HealthCheck.Payload.text` carries PGV `min_bytes: 1` on the raw hex string).
- envoy-rust ACCEPTS both and boots: `decode_hex("")` (`bootstrap.rs:2531`)
  returns `Some(vec![])` because `0 % 2 == 0` and the loop body never runs, so
  `HealthCheckPayload::decode()` returns `Ok(vec![])` and `validate_payload`
  passes. Reproduced directly (real Envoy rejected; envoy-bin ran with no error).

**Why Minor, not Important** (stated, not silently softened — this sits on the
Minor/Important boundary):

- **No differential observable on any config a user writes.** It is a
  config-load accept-vs-reject on a *degenerate/malformed* input (`text: ""`);
  every valid config both proxies accept behaves identically (the six live probes
  above). By the phase-66 precedent (a no-differential-observable finding
  downgraded to Minor and APPROVED), this is Minor.
- **Consistent with envoy-rust's selective-PGV posture.** envoy-rust models the
  fields it consumes and validates what it measured (D-3.3); it is not a full PGV
  rule engine, and the state-0 recon measured only odd-length / non-hex `text`
  (R-0.3), never the empty string. Message byte-parity is already waived
  (ADR-0049 / PV-1); this is one unenforced `min_bytes` rule among many
  project-wide.
- **Benign runtime.** For the `receive:[{text:""}]` form, `tcp_probe_once` keys
  its connection-only early-return on `receive.is_empty()` (the raw `Vec`, here
  length-1), so it enters the read loop and waits for ≥1 byte; `receive_matches`
  then *skips* the empty payload (`if payload.is_empty() { continue }`) → healthy
  on the first inbound byte, or timeout→unhealthy if none. Odd, but on a config no
  one writes and which Envoy refuses outright.

**But it is a genuine fail-loud gap** against the phase's own ADR-0049 invariant
("every invalid `tcp_health_check` config surfaces a typed `ConfigError` … never
a silent default"), and there is a **direct sibling precedent in the same
validator** — `EmptyHealthCheckPath` rejects an empty `http_health_check.path`.
So the correct-and-consistent behavior is to reject empty `text` too.
**Recommended fix** (trivial, ~2 lines + a test): in `HealthCheckPayload::decode`
treat a `Some(s)` `text` arm with `s.is_empty()` as `Err(PayloadDecodeError::InvalidHex(s))`
(mirroring `min_bytes: 1`), and extend the BEHAVIOR_CONTRACT "odd-length /
non-hex" clause to name the empty string. Owner = the next session (a cheap
§5.2-style follow-up) or the next phase touching the TCP-HC payload validator.
This does **not** block approval.

### M68-2 (Minor) — a read error is mislabeled `TcpProbeError::Send` in the debug diagnostic

In `tcp_probe_once` (`probe.rs:209`) the `stream.read(&mut chunk)` error arm maps
to `TcpProbeError::Send(e.to_string())` — a *read* failure surfaced under the
*Send* label. Purely cosmetic: `TcpProbeError` is `#[allow(dead_code)]` and is
consumed only by `tracing::debug!(… error=?e …)`; the health OUTCOME
(`record_failure` → unhealthy) is identical whichever label is used, and the
meaningful terminal cases (`Eof`, `Timeout`) are already distinct. A mid-read
IO error (e.g. connection reset during the scan) would log as "Send". Fix if the
probe.rs error surface is ever revisited; no behavioral impact. Carry-forward.

---

## Traps honored (confirmed against the diff)

- **No revert of landed 12/67 work.** The `envoy-health` `Scheduler` /
  `EndpointHealth` / ejection / `pick()` / stat tree and the `0019`
  `http1_after_settle` harness are reused unchanged; the removed
  `.expect("validator-guaranteed http_health_check present")` is replaced by the
  `(http_cfg, tcp_cfg)` dispatch — the only remaining `.expect("http checker
  present")` is inside a test where HTTP is present (grep-confirmed; no production
  consumer assumes http-always-present).
- **CidrRange / M-1 untouched** (no `crates/envoy-filter` or CidrRange change in
  the diff). No `rbac` added to `is_terminal_network_filter`; `filters: []` not
  rejected; ADR-0016/0124/0131 and the fail-loud RBAC/TLS divergences untouched.
- **No fixture weakened; `known-failures.txt` untouched; no ROADMAP
  malformed-row "fixes".** Fuzz seed `cluster_tcp_health_check.yaml` is `!`-un-
  ignored and `git ls-files`-tracked; NO new fuzz TARGET → no `ci.yml` change
  (ADR-0137 §7.4, correct).
- `#![forbid(unsafe_code)]` holds at every crate root (no diff removes it).

---

## Carry-forwards after this review

- **M68-1** — reject empty-hex `text: ""` (fail-loud gap vs Envoy `min_bytes: 1`;
  sibling `EmptyHealthCheckPath` precedent). First to close.
- **M68-2** — read-error mislabeled `TcpProbeError::Send` (cosmetic diagnostic).
- Inherited, unconsumed by this phase (each owned by the next phase touching its
  surface): **M-1** (CidrRange `prefix_match` guard band), **CF-67-3** (payload
  `on_data` iteration), **CF-67-5** (empty `filters: []`), **CF-67-6** (bound
  `close_with_drain` drain), **CF-67-7** (TLS `[rbac, tcp_proxy]` establishment
  ordering), and the older `67.3/SPEC.md §10` Minors + the HTTP-filters-family
  (1)–(4) carry-forwards in `STATE.md` `## Notes`.

**Next session: the §5 state-6 close-out** (a state-management session, NOT a
`superpowers:*` skill; SEPARATE session per ADR-0127 + memory
`closeout-and-pick-are-separate-sessions`). Flip ROADMAP row `68` → `done`,
relocate this phase's Notes/top-section narratives per ADR-0035, set STATE →
awaiting the next planning pick.
