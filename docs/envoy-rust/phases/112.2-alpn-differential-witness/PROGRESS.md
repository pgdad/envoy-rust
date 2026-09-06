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

---

# §5 state-4 — THE §7.5 VERIFICATION GATE

> **A separate session from the implementation**, per §5.1 and `ADR-0127`: the
> context that wrote an artifact must not grade it. This section records what
> the gate actually printed, and re-verifies on disk every one of the six
> findings state 3 banked rather than rediscovering them.
>
> **Session start:** `git status --porcelain` empty; branch `main`; HEAD
> `31169c2e3c45e698511cea71a4e586beb039ed5b` — the state-3 CI RECORD commit,
> whose parent `5e51add1acc7bfd5a6789b3e245204455eb2ae8c` is the state-3
> implementation. The record was already landed (numstat `2 0`), so no
> outstanding CI line was owed. `ls stop` → `No such file or directory`. The
> four `.claude/worktrees/agent-*` worktrees belong to a PARALLEL WORKSTREAM
> and were left untouched throughout; `git fetch origin --prune` exited 0 at
> start and again before the commit.
>
> The handoff matched disk this time: `STATE.md` `## Next expected skill`
> named the state-4 gate, `112.2/` held `SPEC.md` + `PLAN.md` + `PROGRESS.md`
> and no `REVIEW.md`, which IS the state-4 detection rule.

---

## Gate (e) — the five workspace commands, run locally from a clean tree

```
$ cargo build --workspace --all-targets
   Compiling differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.94s
BUILD_EXIT=0
```
`Compiling` lines = **1** (the one crate this phase touched), `^warning` = **0**,
`^error` = **0**. Non-zero, so not a cached no-op.

```
$ touch tests/differential/src/lib.rs    # mtime-only, forces a dirty set
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Checking differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.30s
CLIPPY_EXIT=0
```
**Zero findings over 1 `Checking` line** — clippy prints `Checking`, not
`Compiling`; the dirty set was forced with an mtime-only `touch` of the one
file this phase changed, and `git status --porcelain` stayed empty.

```
$ cargo fmt --all -- --check
FMT_EXIT=0
```
The whole log is **11 bytes** — the exit marker and nothing else, a zero diff.

```
$ cargo deny check
advisories ok, bans ok, licenses ok, sources ok
DENY_EXIT=0
```
Gated on the ANSI-stripped four-ok line. The 5 `unmatched license allowance`
notes are the normal warnings on a green run.

```
$ cargo test --workspace --no-fail-fast          # sweep 1
binaries=168 passed=2269 failed=5   identity(passed+failed)=2274
$ cargo test --workspace --no-fail-fast          # sweep 2
binaries=168 passed=2268 failed=6   identity(passed+failed)=2274
```

**The identity is 2274 in both sweeps and equals CI's `passed` exactly** —
and `binaries` is **168**, the predicted +1 for `tls_alpn.rs`. Counts come
from matching `test result: (ok|FAILED)` — never bare `ok` — with the awk
fields derived from a real matched line (`$4` passed / `$6` failed) and
asserted non-zero. `Running ` lines: **152**, one more than the 151 of the
last close-out, which is the new test binary. The `differential` lib row reads
**`178 passed; 0 failed; 2 ignored`**, the same figure state 3 measured and CI
printed. `docker ps` was polled every 5 s during sweep 1: **37** ticks showed
`envoyproxy/envoy:v1.33.0` across **30** distinct host ports, so the
differential fixtures genuinely spawned the upstream container.

---

## Gate (e), continued — the local RED set, classified by ISOLATION ONLY

Failing names were extracted from the `---- <name> stdout ----` markers, never
by indentation, from a log redirected to a file (never `tail`).

| sweep | RED set |
|---|---|
| 1 (5) | `access_log_h2_rcd_upstream_reset`, `access_log_h2_uc_upstream_reset`, `access_log_rcd_upstream_reset`, `access_log_rf_upstream_reset`, `admin_config_dump_server_info` |
| 2 (6) | the five core + `access_log_metadata_filter` |

Every member was re-run **ALONE with a 30-second settle gap before it**:

```
access_log_h2_rcd_upstream_reset    FAILED. 0 passed; 1 failed   (2.70s)   -> CORE
access_log_h2_uc_upstream_reset     FAILED. 0 passed; 1 failed   (2.75s)   -> CORE
access_log_rcd_upstream_reset       FAILED. 0 passed; 1 failed   (2.88s)   -> CORE
access_log_rf_upstream_reset        FAILED. 0 passed; 1 failed   (2.74s)   -> CORE
admin_config_dump_server_info       FAILED. 0 passed; 1 failed   (2.73s)   -> CORE
access_log_metadata_filter          ok. 1 passed; 0 failed      (12.67s)   -> parallel-load flake
```

**The stable core is FIVE — exactly the recorded host signature, and exactly
sweep 1's RED set.** Its determinism in isolation IS this host's signature,
not a regression; all five are green in CI (whole-log `test result: FAILED` =
**0** on the state-3 push). **No test was weakened and nothing was fixed.**

---

## Gate (a) — new/changed differential fixtures: `0091`, `0092` AND `0004`

Three fixtures, not two: `0004-tls-downstream` is a **CHANGED** fixture (its
`expectations.yaml` gained cell 6) and belongs here, not under (b).

Under the full-parallel sweep 1, all four TLS runners were green:

```
Running tests/tls_alpn.rs        test result: ok. 2 passed; 0 failed   (3.88s)
Running tests/tls_downstream.rs  test result: ok. 1 passed; 0 failed   (3.02s)
Running tests/tls_sni.rs         test result: ok. 1 passed; 0 failed   (3.23s)
Running tests/tls_upstream.rs    test result: ok. 1 passed; 0 failed   (2.96s)
```

Run ALONE, after a settle gap, with `docker ps` polled every second:

```
$ cargo test -p differential --test tls_alpn -- --nocapture
     Running tests/tls_alpn.rs (target/debug/deps/tls_alpn-39f76a0eabb6a587)
test tls_alpn_server_preference_fixture ... ok
test tls_alpn_fixture ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.07s
TLS_ALPN_EXIT=0
docker ps ticks during the run: 7, on TWO distinct upstream containers
  (envoyproxy/envoy:v1.33.0 0.0.0.0:55465->10000/tcp and 0.0.0.0:55466->10000/tcp)
```

```
$ cargo test -p differential --test tls_downstream --test tls_sni --test tls_upstream
     Running tests/tls_downstream.rs   test result: ok. 1 passed; 0 failed   (3.03s)
     Running tests/tls_sni.rs          test result: ok. 1 passed; 0 failed   (3.31s)
     Running tests/tls_upstream.rs     test result: ok. 1 passed; 0 failed   (3.02s)
TLS_SIBLINGS_EXIT=0
```

**Mutation re-check — M1 re-run by THIS session, independently of state 3.**
State 3 ran M1, the `0092` inversion and the cell-6 flip and recorded their
RED sets; `ADR-0190` records that an in-tree run with an md5-adjudicated
restore is equivalent to a scratch worktree for a one-line YAML mutation read
at runtime by an unchanged binary, and strictly lower risk (its F4). This
session re-ran M1 that way — delete the server list from `0091`'s
`envoy-rust.yaml` ONLY — with the target asserted to occur exactly once:

```
$ grep -c alpn_protocols tests/fixtures/0091-tls-alpn/envoy-rust.yaml
1                                            # target occurs EXACTLY ONCE
pre md5     a4a7b8f996149baf7c41f1358a959873
$ sed -i '/alpn_protocols/d' tests/fixtures/0091-tls-alpn/envoy-rust.yaml
mutated md5 585a187f300703f9c3931dd961a53d1d   occurrences now: 0
$ cargo test -p differential --test tls_alpn tls_alpn_fixture
Compiling differential lines: 0              # correct: a YAML mutation read at runtime, no rebuild expected
fixture passes: envoy-rust tls probes
    0: expected_alpn match against 127.0.0.1:41887 for probe sni="a.example.com" client_alpn=["h2", "http/1.1"]
    1: expected ALPN "h2" to be negotiated, but the handshake completed with no protocol selected
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out; finished in 3.34s
$ git checkout -- tests/fixtures/0091-tls-alpn/envoy-rust.yaml
restored md5 a4a7b8f996149baf7c41f1358a959873 == pre md5 ; git status --porcelain EMPTY
$ cargo test -p differential --test tls_alpn                 # unmutated control, same tree, 30s later
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.89s
```

**M1 reproduces state 3's predicted RED SET exactly**: RED at probe 1, on the
SUBJECT side (`envoy-rust tls probes`), with the verbatim text, and the
`test result` line present (a compile error is not a mutation RED). The fixture
therefore rests on the config it claims to witness, and the assertion is not
vacuous. State 3's other two mutations (the `0092` inversion and the cell-6
flip) were not re-run; their RED sets are recorded in the state-3 section
above and `ADR-0190`.

**On-disk shape checks, all asserted rather than read off the plan:**
`0092`'s two config files differ from `0091`'s in **exactly one line each**
(line 18 of `envoy-rust.yaml`, line 24 of `envoy.yaml` — the list order);
`0091`'s `envoy-rust.yaml` differs from `0004`'s in exactly the two `node:`
lines plus one added `alpn_protocols` line at `tls_certificates`' indentation;
`0004`'s two config files carry **zero** column-0-anchored `alpn_protocols`
keys (positive control: `0091`'s carry 1 and 1); both `payload.bin` inputs are
tracked (`git ls-files`); the runner is 32 lines and the two fixtures 166 and
129 lines, matching the plan's per-file rows.

## Gate (b) — the 89 other pre-existing differential fixtures still green

```
fixture dirs on disk              : 92   (git ls-files agrees: 92)
differential runner files on disk : 91
runners MISSING from the CI log   : 0    (state-3 push, run 34050709816)
wrong-prefix control              : 0    (`Running tests/differential/tests/`)
CI whole-log `test result: FAILED`: 0
```

**91 of 91 runner files executed in CI, zero missing**, censused by RUNNER
FILE NAME against the crate-relative `Running tests/<n>.rs` prefix — the
counts stop matching fixture directories by design, because `tls_alpn.rs`
drives two. Locally, sweep 1's only reds are the five-member core above, so
the 89 pre-existing fixtures minus that core were green here too, and the
core is green in CI.

## Gate (c) — conformance suites

```
CI (run 34050709816):
       Running tests/h2spec_runner.rs
       test h2spec_pass_rate_gate ... ok
CI control: `h2spec not found` = 0  -> the suite GENUINELY RAN (ADR-0163)
```
`known-failures.txt` **untrimmed**: `tests/conformance/h2spec/known-failures.txt`
is **21** lines, md5 `19cd44d86a8b15d825f76c6e7b265e65`, re-measured here
(`tests/h2spec/` does not exist).

Local run, with `--nocapture` so the skip is visible:

```
$ cargo test -p h2spec-conformance --test h2spec_runner -- --nocapture
h2spec_runner: h2spec not found — skipping locally
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
H2SPEC_LOCAL_EXIT=0
```
**Vacuous locally by construction; CI is authoritative.** Not re-raised.

## Gate (d) — fuzzing: VACUOUS BY CONSTRUCTION

`git diff --stat 00060ad 5e51add -- fuzz '*/fuzz/*' .github` is **EMPTY**: no
new fuzz target, no `ci.yml` edit. The tree holds the same **5** fuzz targets
(`accesslog_format_parse`, `cdn_loop_parse`, `grpc_health_decode`,
`jwt_parse`, `parse_bootstrap`), and the CI fuzz job on the state-3 push ran
all five on a real runner (`GitHub Actions 1000005759`, 13 steps, `success`):

```
Done   170189 runs in 31 second
Done  3881442 runs in 31 second
Done  3201234 runs in 31 second
Done  2502233 runs in 31 second
Done 16403837 runs in 31 second
ERROR: libFuzzer      = 0
Test unit written to  = 0      (no crash artifacts)
```

⚠ **A new `gh` trap, recorded because it produced a believable zero twice:**
`gh run view --job <fuzz-job-id> --log` returned **exit 0 and a 0-byte file**
for this job (both attempts), which reads as "nothing ran". The REST endpoint
`gh api repos/<owner>/<repo>/actions/jobs/<id>/logs` returned **3,642,971
bytes** for the same job. The `build + test + lint` job's log came through
`gh run view --job --log` normally (696,514 bytes). Assert the byte count
before counting anything in a job log.

## Gate (f) — REVIEW.md

**Not this session's.** Gate (f) is closed by the §5 state-5 code review.

---

## CI on the state-3 implementation — CONFIRMED, and re-derived independently

`STATE.md` already carries the record line (`31169c2`, numstat `2 0`). This
session re-derived it from the job log rather than trusting the record:

```
run 34050709816   conclusion success   attempt 1   the ONLY run on 5e51add1acc7bfd5a6789b3e245204455eb2ae8c
  build + test + lint : 15 steps, success
  fuzz                : 13 steps, success
identity: binaries=168 passed=2274 failed=0     (ANSI-stripped; fields $4/$6 derived from a matched line)
`test result: FAILED` = 0      `Running ` raw = 0 / stripped = 152      `h2spec not found` = 0
```

**The identity moved exactly as `PLAN.md` and `PROGRESS.md` predicted**
against the `00060ad` baseline `binaries=167 passed=2265 failed=0`: +1 binary
(`tls_alpn.rs`), +9 passed (+7 lib tests, 171 → 178; +2 integration tests).
Nothing was renamed and nothing deleted, so unlike `ADR-0187`'s case there
was no double count to correct. **This session's own commit is docs-only, so
the identity must NOT move from `binaries=168 passed=2274 failed=0`.**

---

## The six banked findings — re-verified on disk, not rediscovered

| # | claim | re-measured here |
|---|---|---|
| F1 | 652 net code lines, `lib.rs` `+331 −11` | `git diff --numstat 00060ad 5e51add -- . ':(exclude)docs/**'` → added 663, deleted 11, **net 652**; `lib.rs` `331 11` ✓ |
| F2 | M2 prediction false in both legs | not re-run (a source mutation needing a rebuild); `check_alpn` has its two production call sites at `lib.rs:2139`'s callers in `drive_tls` and `drive_tls_probes`, so the ADR-0190 reasoning holds on the tree |
| F3 | Task 6 Step 2 self-triggers | `grep -rc alpn_protocols tests/fixtures/0004-tls-downstream/` → `expectations.yaml:1`, every other file 0; column-0-anchored on the two config files → **0 and 0**, control on `0091` → **1 and 1** ✓ |
| F4 | shared `CARGO_TARGET_DIR` poisons the binary | not reproduced; this session used no worktree and no `/tmp/` path appears in any red |
| F5 | `BEHAVIOR_CONTRACT.md` `119 0` | `git show 4d1a80d --numstat` → `119 0` ✓ |
| F6 | citations re-anchored | every anchor asserted to occur **exactly once** by `grep -cF`: `pub struct TlsTcpProbe {` `:749`, `pub enum AlpnRule` `:774`, `pub async fn drive_tls(` `:1954`, `pub async fn drive_tls_probes(` `:2043`, `async fn run_tls_tcp_arm` `:5034`, `async fn run_tls_tcp_probe_list_arm` `:5101`, `TlsTcp {` `:91`, `TlsTcpProbeList {` `:109`, the two `drive_tls(` call sites `:5071`/`:5082`; `lib.rs` **11549** lines ✓. `fn check_alpn` occurs **twice** (`:2139` the fn, `:9104` its test) — anchor on `fn check_alpn(` if a unique hit is needed. The bare `#[serde(deny_unknown_fields)]` substring occurs **16** times unanchored and **14** anchored at column 0; both are method-true, and neither is a unique anchor. |

`bootstrap.rs` (untouched by this phase): `Http2OverTlsNotSupported` at
**`:4275`** with `UnsupportedCodecType` at `:4267`; `TooManyListeners` at
**`:3671`**. CF-112-1's definition: `grep -n 'OPENS CF-112-1'` →
`DECISIONS.md:2665`.

---

## Stop condition — re-derived from disk this session. ALL THREE LEGS FALSE

No `stop` file was created; `ls stop` → `No such file or directory`.

- **Leg (i) FALSE** — **120** rows / **118** `done` / **1** `in-progress`
  (`112`) / **1** `planned` (`112.2`); buckets sum to the row count. Status is
  field **4** on a `' | '` split driven from the `^\| [0-9]` prefix. Control:
  the forbidden `NF == 6` form reads **118**, coinciding with the `done` count
  by accident — it drops rows 38 and 39, which were NOT "fixed".
- **Leg (ii) FALSE** — **14** crates, none of `envoy-http3`/`envoy-grpc`/
  `envoy-wasm`/`envoy-protos`/`envoy-runtime` (`test -d`); `quinn`/`wasmtime`/
  `tonic`/`opentelemetry`/`prost` = **0** across all **28** manifests from
  `git ls-files '*Cargo.toml'`, against `tokio` = **19** of 28 with the
  identical invocation. `tests/conformance/` holds only `h2spec/`.
- **Leg (iii) FALSE** — **11** `### ` family headings driven from a single
  `/^### /` rule, reading 10/5/3/14/3/4/6/29/6/**0**/13 with **27** rows before
  the first heading, summing to 120; `### WASM host family` carries zero rows.

---

## Carry-forwards — banked, not consumed (§6.3; `ADR-0165`)

**Nothing was fixed, no fixture was edited, no landed artifact was edited**
(`112.2/SPEC.md` and `112.2/PLAN.md` included), and `ROADMAP.md` was
deliberately untouched — rows `112.2` and parent `112` flip at state 6. The
whole banked set carries forward INTACT: CF-112-8 Consequence 2 (structurally
unwitnessable by this harness), CF-112-9 (actively avoided — no fixture
configures an empty element on either side, re-checked by grep), CF-112-6,
CF-112-1/2/3/4/7, CF-112-10/11/12, the `112.1` REVIEW's M-1…M-5 and
N-1…N-12, phase 111's M-1…M-15 / N-1…N-13 and CF-111-1…CF-111-9, the
`110.2`/`110.1`/`109.2`/`109.1`/`108.2` REVIEW sets, CF-110-1…9, CF-109-1/2/3,
CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6,
CF-74-1/2/3/4/6, CF-73-1 and the HTTP-filters-family (1)-(4). **CF-112-5 stays
CLOSED.** No new carry-forward is opened.

---

## Verdict

**§7.5 gate: (a) PASS on all three fixtures (`0091`, `0092`, and the CHANGED
`0004`), (b) PASS on the other 89, (c) PASS in CI, (d) vacuous by construction,
(e) PASS locally and in CI. (f) is state 5's.** `112.2` is code-complete and
verified. No ADR fired: nothing in the record was corrected and no decision
was made that the record does not already carry.

## Next state

**§5 state 5 — the code review — is a SEPARATE session** (§5.1; `ADR-0127`:
the context that graded an artifact must not review it). It runs
`superpowers:requesting-code-review` and outputs `REVIEW.md`, closing gate (f).
A state-5 review writes no code, so **the CI identity must NOT move from
`binaries=168 passed=2274 failed=0`.**
