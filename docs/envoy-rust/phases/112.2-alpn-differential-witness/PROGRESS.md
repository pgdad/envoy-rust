# Phase 112.2 — the ALPN differential witness: PROGRESS

§5 state 3 (the implementation), executed in one session against
`PLAN.md`'s 7 TDD tasks, in order, with a per-task
`clippy --all-targets --all-features -- -D warnings` + `fmt --all -- --check`
gate. `superpowers:executing-plans`; TDD per task (D-3.1).

Baseline recorded before Task 1, so every delta below is asserted rather than
eyeballed: `cargo test -p differential --lib` = **171 passed; 0 failed; 2
ignored**. Upstream image digest verified on this host before the first Docker
run: `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`,
matching `ENVOY_TARGET.md` exactly.

---

## Task 1 — the fixture grammar (`a0b2908`, `174 1`)

`AlpnRule` (internally tagged, `deny_unknown_fields`) plus `client_alpn:
Vec<String>` and `expected_alpn: Option<AlpnRule>` on BOTH `TlsTcpProbe` and
`Driver::TlsTcp`, every new field `#[serde(default)]`.

- **RED, measured:** `E0433 cannot find type AlpnRule in this scope`,
  `E0609 no field client_alpn on type TlsTcpProbe`, `E0026 variant
  Driver::TlsTcp does not have fields named client_alpn, expected_alpn` —
  12 errors, exit 101.
- **GREEN:** `test result: ok. 4 passed` on the `alpn` filter, `1 passed` on
  `expectations_parse_pre_112`. Both counts asserted NON-ZERO (`0 passed; N
  filtered out` is a false green).
- **Boundary trap 1, handled as planned:** the dispatch site was left matching
  on `..`. The plan's two predicted `-D warnings` errors are real and both
  were avoided rather than encountered.
- Gate: clippy exit 0 with **1** `Checking` line (not a cached no-op), fmt
  exit 0.

## Task 2 — `check_alpn` + the `drive_tls` path (`3c28950`, `103 6`)

- **RED:** `E0425 cannot find function check_alpn in this scope`, 5 errors,
  exit 101.
- **GREEN:** `1 passed` on the filter; full lib suite **177 passed; 0 failed;
  2 ignored** = 171 + 5 + 1.
- **Boundary trap 2, honoured:** `check_alpn` landed together with its first
  production caller. (M2′ below independently re-measures why.)
- **The plan's own corrected claim, re-measured here:** `run_tls_tcp_arm` now
  takes seven arguments and clippy exits **0** with **zero**
  `too_many_arguments` diagnostics. No `#[allow]` was added. This
  independently reproduces the correction `ADR-0189` recorded against the
  plan's first draft.
- Gate: clippy exit 0, fmt exit 0.

## Task 3 — the per-probe path (`dabc2db`, `55 5`)

`ClientConfig` moved inside the probe loop with `root_store.clone()` per
iteration; `drive_tls_probes`' signature unchanged.

- The new test is a **characterization pin, not a RED** — it goes green on
  Task 1's grammar alone, exactly as the plan states openly. The genuine RED
  for this plumbing is fixture `0091` probe 1 (Task 4 M1, below).
- Full lib suite **178 passed; 0 failed; 2 ignored** = 171 + 7 (5+1+1).
- `run_tls_tcp_probe_list_arm` confirmed untouched: `git diff HEAD --
  tests/differential/src/lib.rs | grep -c run_tls_tcp_probe_list_arm` = **0**.
- Gate: clippy exit 0, fmt exit 0.

## Task 4 — fixture `0091-tls-alpn` + the runner (`13d705f`)

`envoy.yaml` 49, `envoy-rust.yaml` 42, `expectations.yaml` 31, `README.md`
43, `payload.bin` 1, `tls_alpn.rs` 25 — **every one matching the plan's
per-file row exactly.**

- Both config files produced mechanically from `0004`'s. Diff against `0004`:
  exactly the two `node:` lines plus one added `alpn_protocols` line, per
  side. The added key's indentation was **asserted** equal to
  `tls_certificates`' (16 on both sides), not assumed. `payload.bin` md5
  `8a1f7b23acbf1406b09b2f5b2ffc286f`, byte-identical to `0004`'s.
- **Parse proved before spending a Docker run** (throwaway test; `envoy-bin`
  has no `--mode validate`): four probes, probe 2 carrying `protocol:
  "http/1.1"` as a STRING — the unquoted `/` claim measured rather than read
  off the spec — and probe 4 carrying `client_alpn: []`. Throwaway deleted.
- **GREEN:** `test result: ok. 1 passed` in 3.79s.
- **The fast green was AUDITED, not trusted.** testcontainers auto-removes on
  drop, so `docker ps -a` afterwards shows nothing and reads like "no
  container ever ran". Polling `docker ps` DURING the run caught
  `envoyproxy/envoy:v1.33.0 0.0.0.0:55223->10000/tcp` across **11** ticks.

### Mutation checks (scratch worktree at `dabc2db`, 30s settle gap, control first AND last)

| # | mutation | predicted | measured |
|---|---|---|---|
| M1 | delete the server list from `envoy-rust.yaml` only | RED at probe 1, subject side | **RED, verbatim** |
| — | scope control | `tls_downstream`/`tls_sni` unaffected | **both green** |
| M2 | delete the probe-list assertion | fixture PASSES | **passes** |
| M2 | *its stated "meaningful RED"* | unit test reddens + `check_alpn` unused | **BOTH FALSE — see below** |
| M2′ | delete BOTH assertion sites | (not in the plan) | `error: function check_alpn is never used`, exit 101 |
| M3 | unmutated control, same tree | green | **green**, with `Compiling differential` |

M1's exact text: `expected_alpn match against 127.0.0.1:38793 for probe
sni="a.example.com" client_alpn=["h2", "http/1.1"]` → `expected ALPN "h2" to
be negotiated, but the handshake completed with no protocol selected`.

M1 produced **no** `Compiling differential` line, and that is correct rather
than a stale-binary false pass: the mutation is YAML read at runtime. The
forced-rebuild check applies to the source mutations, where it did fire.

## Task 5 — fixture `0092-tls-alpn-server-preference` (`b8498d9`)

49 / 42 / 14 / 23 / 1 and the runner at 32 — again matching the plan exactly.

- The one-line difference IS the experiment, so it was asserted: `diff`
  against `0091` reports **exactly one** changed line on each config file.
- **GREEN:** `test result: ok. 2 passed` in 4.05s.
- **Mutation — inversion, not deletion**, because a silent inversion of the
  preference rule is what this fixture exists to catch. Predicted RED
  reproduced exactly: `expected ALPN "http/1.1", got "h2"` on the SUBJECT side
  only, with `tls_alpn_fixture` staying green (1 passed, 1 failed). Restore
  adjudicated by md5 (`7a543c6b56a693df34daea96879dcb2d`, matched); control
  afterwards 2 passed.

## Task 6 — cell 6 on the existing `0004-tls-downstream` (`55eb5ae`, `5 0`)

- Neither config file gained a server list — the ABSENT list IS the cell.
- All three TLS siblings green: `tls_downstream` (which now carries cell 6),
  plus `tls_sni` and `tls_upstream` as the controls for Task 3's driver
  change.
- **Mutation:** cell 6 flipped to `{ kind: selected, protocol: h2 }`.
  Predicted RED reproduced exactly **and on the predicted side** — it fails on
  the UPSTREAM arm first: `upstream envoy tls drive` → `expected_alpn match
  against 127.0.0.1:55239` → `expected ALPN "h2" to be negotiated, but the
  handshake completed with no protocol selected`. That is also the positive
  evidence that upstream Envoy completes a handshake with nothing negotiated
  when it advertises no list (parent SPEC §1.1 F4). Restore adjudicated by
  md5 (`d69432cb7629ab9bd932a5c92442abb1`); control green.

## Task 7 — the `BEHAVIOR_CONTRACT.md` ALPN section (`4d1a80d`, `119 0`)

- Placed between `## Response trailers` and `## Header allow-list`, located by
  text. **Zero deletions** — the assertion that actually matters.
- **Transcription verified, not trusted:** the 116-line section was extracted
  programmatically from `PLAN.md`'s markdown fence and `diff`ed against the
  inserted text — IDENTICAL.
- Citation check passes: the section carries **no** `file:line` citation, a
  deliberate CF-112-12 response. The zero is non-vacuous — the same grep finds
  **19** such citations elsewhere in the same file.
- The INFERRED and UNMEASURED hedges were preserved verbatim.

---

## Findings

### F1 — the PLAN's MEASURED `lib.rs` row is 57 lines under, and the root cause is fully reconciled

`lib.rs` landed at **320 net (+331 −11)** against the plan's MEASURED **263
(+274 −11)**. The deletions match EXACTLY (11 = 11), which localises the whole
divergence in the additions and rules out any structural difference in the
edits. The implementation is byte-faithful to the plan: 13 of its 14 Tasks-1-3
`rust` fences occur verbatim in the tree, and the 14th is Task 1 Step 5's
intermediate `..` dispatch, correctly superseded by Task 2 Step 5.

The cause is a single fact, and two independent numbers confirm it:

| observation | plan | tree | difference |
|---|---|---|---|
| lib tests after the slice | 176 | **178** | 2 tests |
| added lines in `lib.rs` | 274 | **331** | 57 lines |

176 = 171 baseline + **5**; 178 = 171 + **7**. The prototype was measured with
five of the seven tests present. Task 2's test block (23 lines + separator) and
Task 3's (32 + separator) are 57 lines — the exact shortfall. Both tests were
drafted into `PLAN.md` after the prototype was measured, and neither the LoC
row nor the test count was re-measured afterwards.

**Consequence for the §6.1 gate: none.** The code subtotal moves 595 → **652**
and the total 711 → **771**; 652 is still far below the ~1500 ceiling, and even
at the worst PROJECTED factor in the ledger (1.66×) it reaches 1082. The
verdict does not change; only the figure does. `ADR-0190` records it.

### F2 — the PLAN's M2 predicted RED is wrong, and it is the ADR-0186 defect class again

`PLAN.md` Task 4 Step 8 predicts that deleting the probe-list assertion makes
the unit test redden and `check_alpn` become unused. **Measured: neither
happens.** The unit test passes (it calls `check_alpn` directly, and
`check_alpn` is unchanged) and clippy `-D warnings` exits 0, because
`drive_tls` still calls it. Deleting BOTH call sites (M2′) is what produces
`error: function check_alpn is never used`, exit 101.

The plan reasoned about an intermediate state it never built — precisely the
class `ADR-0186` named. The state-2 session applied that correction to the
task BOUNDARIES and got all three right; it did not apply it to the MUTATION
PREDICTIONS, which are intermediate states too.

### F3 — Task 6 Step 2's check self-triggers on Step 1's own comment

`grep -rc alpn_protocols tests/fixtures/0004-tls-downstream/ | grep -v ':0'` is
specified to print `NONE — correct`, but Step 1's added comment contains the
word `alpn_protocols`, so the check reports a hit and reads as "the server list
was added by mistake". Anchored at column 0 on the two CONFIG files it returns
0 and 0, with a positive control on `0091` returning 1 and 1.

### F4 — a self-inflicted hazard: sharing `CARGO_TARGET_DIR` with a scratch worktree

Task 4's mutation worktree was run with `CARGO_TARGET_DIR` pointed at the main
tree's `target/` to avoid a full rebuild. That baked the worktree's
`CARGO_MANIFEST_DIR` into the reused test binary, so the next main-tree run
died in **0.00s** with `tcp-echo-server not found at
/tmp/mut112_2/target/debug/tcp-echo-server`. It reads exactly like a fixture
RED and is not one. Recovered by removing the worktree and forcing a rebuild.
Task 5's and Task 6's mutations were therefore run in the main tree with
md5-adjudicated restores — equivalent for a one-line YAML mutation read at
runtime by an unchanged binary, and strictly lower risk. Deviation recorded in
both commit messages.

### F5 — `BEHAVIOR_CONTRACT.md` is `119 0`, not the predicted `117 0`

The extra two lines are a `---` separator and its blank. The plan's 117 would
have left `## Header allow-list` without the separator that 16 of the file's
17 headings carry. With it, 17 of 18 follow the convention — the same ratio as
before. Zero deletions either way.

### F6 — every `SPEC.md` §2.4 citation into `lib.rs` is now stale except one

Task 1 inserted above almost all of them, exactly as the plan warned. Re-derived
by TEXT at `4d1a80d` (`lib.rs` 11229 → **11549** lines):

| SPEC §2.4 | now | anchor |
|---|---|---|
| `:38` | **`:38`** (unmoved) | `#[serde(tag = "kind", …)]` on `Driver` |
| `:84` | **`:91`** | `TlsTcp {` |
| `:98` | **`:109`** | `TlsTcpProbeList {` |
| `:732` | **`:748`** | `#[serde(deny_unknown_fields)]` on `TlsTcpProbe` |
| `:733` | **`:749`** | `pub struct TlsTcpProbe {` |
| `:1910` | **`:1954`** | `pub async fn drive_tls(` |
| `:1985` | **`:2043`** | `pub async fn drive_tls_probes(` |
| `:4945` | **`:5071`** | `let upstream_out = drive_tls(` |
| `:4954` | **`:5082`** | `let subject_out = drive_tls(` |

New symbols: `pub enum AlpnRule` `:774`, `fn check_alpn` `:2139`,
`async fn run_tls_tcp_arm` `:5034`, `async fn run_tls_tcp_probe_list_arm`
`:5101`.

⚠ **Locating `:732` by its attribute text alone lands on `:30`** — that bare
attribute occurs **16** times in the file, and `:30` is a plausible-looking
wrong answer. Anchor it on `pub struct TlsTcpProbe {` and read the line above.

---

## Census and predictions handed to state 4

- **92** fixture directories (was 90) and **91** runner files (was 90);
  `git ls-files` agrees on 92. The counts stop matching by design —
  `tls_alpn.rs` drives two fixtures. Census differential work by RUNNER FILE
  NAME.
- `0004` is a CHANGED fixture under §7.5(a), so gate **(b) covers 89**, not 90.
- **Predicted CI identity: `binaries=168 passed=2274 failed=0`** (baseline
  `binaries=167 passed=2265 failed=0`; +7 unit tests, +2 integration tests, +1
  test binary). Nothing renamed, nothing deleted.
- **(d) is vacuous** — no new fuzz target, no `ci.yml` edit.
- No `crates/` change, no `Cargo.toml` change, no new dependency — the Global
  Constraints held throughout, and no task ever appeared to need a crate change.
- **No carry-forward was fixed** (§6.3; `ADR-0165`). CF-112-8 Consequence 2
  stays banked as structurally unwitnessable; CF-112-9 stays banked and
  actively avoided (no fixture configures an empty element on either side);
  CF-112-6 is recorded in the contract section as a known unmatched cell.
- `ROADMAP.md` was NOT touched — rows `112.2` and parent `112` flip at state 6.
