# Sub-phase 109.2 — PROGRESS (§5 state-3 implementation)

> Running log, appended per PLAN task (BOOTSTRAP_PROMPT.md §5 state 3). The
> execution authority is `PLAN.md` (5 tasks); the design authority `SPEC.md`.
> Session started at `e458765` (clean tree, level with `origin/main`; detection
> rule re-verified on disk: `109.2/` held `SPEC.md` + `PLAN.md` only and NO
> `PROGRESS.md`; ROADMAP **113** rows / **111** `done` / **1** `in-progress`
> (parent `109`) / **1** `planned` (`109.2`); ADR head **ADR-0176**, next free
> **ADR-0177** UNRESERVED; `STATE.md` **199** lines with the Standing-traps line
> MEASURED **125473** characters; `STATE_HISTORY.md` **15843** lines;
> `runtime.rs` **888** lines; `BEHAVIOR_CONTRACT.md` **3927** lines).
>
> **This plan is UNEDITABLE once execution started (D-3.5). Every deviation is
> RECORDED here instead**, and the ledger's completeness is itself reviewed at
> state 5 — an unrecorded deviation is a finding.

## Task 1 — the three banked cascade-guard witness rows in `runtime.rs` ✅

- **Site located by TEXT, not by the inherited number.** The PLAN cites the
  anchor row `"edge: empty runtime_key is not consulted -> default 0 -> Never"`
  at `:752-756`; MEASURED on disk at `:751-756` (label line `:752`) — the
  anchor text resolved to exactly one site, so the drift is cosmetic. The
  guards the three rows pin were re-read on disk and match the PLAN's mutation
  table exactly: `route_fraction_gate` at `:163-211`, the empty-key filter at
  `:167`, `&& v.is_finite()` at `:181`, `p.numerator == p.denominator.value()`
  at `:203`.
- **Edit:** three tuples appended INSIDE the existing `ok_cells` `vec!` in the
  existing test fn `route_fraction_gate_pins_every_measured_cell`, immediately
  after the anchor row and before the closing `];`. **No new test function and
  no new public item** — the workspace test COUNT does not move (measured
  below).
- **`cargo fmt --all` then `--check`:** exit 0, silent.
  `git diff --numstat crates/envoy-config/src/runtime.rs` → **`22 0`**, exactly
  the PLAN's MEASURED figure (and 3.7× the 109.1 REVIEW §8 estimate of "~6
  lines" — the fourth confirmation of the LoC-calibration memory, and the first
  on a test-DATA patch). File **888 → 910** lines.
- **GREEN (characterization pins pass on arrival):**
  `cargo test -p envoy-config --lib route_fraction_gate_pins_every_measured_cell`
  → `test runtime::tests::route_fraction_gate_pins_every_measured_cell ... ok`,
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 708 filtered out`.
  **`1 passed` asserted literally, never the exit code.** The filter was used
  WITHOUT `-- --exact` per the PLAN's Global Constraint — the plan-write
  pre-flight measured `--exact` on the short name returning the false green
  `ok. 0 passed; 709 filtered out`, exit 0. The `708 filtered out + 1 passed =
  709` identity is unmoved from the plan-write's 709, which is the mechanical
  confirmation that this task added no test function.

### TDD RED — the three mutation checks (scratch worktree, unmutated control first)

| # | Mutation | Guard removed | Result | `Compiling envoy-config` |
|---|---|---|---|---|
| — | *(unmutated control)* | — | `ok. 1 passed; 0 failed` | 1 |
| M1 | `rf.runtime_key.as_deref().filter(\|k\| !k.is_empty())` → `rf.runtime_key.as_deref()` | the empty-key filter (`:167`) | `FAILED. 0 passed; 1 failed`, panic naming **`M-1: empty runtime_key is NOT consulted — a `.`-prefixed snapshot entry discriminates …`** | 1 |
| M2 | delete the line `                && v.is_finite()` (`:181`) | the non-finite guard | `FAILED. 0 passed; 1 failed`, panic naming **`M-2: `inf` paired with default 0 — the non-masking direction of the is_finite guard`** | 1 |
| M3 | `p.numerator == p.denominator.value()` → `p.numerator == 100` (`:203`) | the denominator consultation | `FAILED. 0 passed; 1 failed`, panic naming **`M-3: default 1_000_000/MILLION with no key pins the denominator.value() consultation`** | 1 |

Every run printed a real `test result:` line (an exit code with no such line is
a compile error and proves nothing), and every run showed a **non-zero
`Compiling envoy-config` count** (a stale test binary is a FALSE PASS). Each
mutation was applied ONE AT A TIME by a script that asserts the target string
occurs **exactly once** before replacing it, and the file was restored from a
pre-mutation backup (`/tmp/rt.bak`) between mutations. Each panic named its OWN
row — the rows are discriminating, not merely present.

**Post-run:** `git worktree remove --force` succeeded; `git worktree list` shows
the scratch tree gone (the four `.claude/worktrees/agent-*` siblings are a
concurrent workstream's and were LEFT ALONE); `git status --porcelain` in the
main tree shows exactly `M crates/envoy-config/src/runtime.rs`; and the main
tree's `runtime.rs` md5 (`61b18068d2af02171a12a3a35a028313`) is byte-identical
to the pre-mutation backup — the main tree was never mutated.

### DEVIATION 1 (RECORDED) — the scratch worktree was SEEDED, not created at bare `HEAD`

PLAN Task 1 Step 4 prescribes `git worktree add --detach /tmp/109_2-wt HEAD`
and then running the control and the three mutations there. Taken literally at
that point in the task order, **the worktree would predate this task's own
rows**: Step 4 runs BEFORE Step 5's commit, so `HEAD` is still `e458765` and
the worktree's `ok_cells` table is the pre-109.2 one. The M1 mutation against
that table is precisely the case the 109.1 REVIEW M-1 finding says survives
undetected — it would have returned a GREEN and been read as "the mutation is
misaimed" when in fact the rows under test were absent.

**What was done instead:** the worktree was created `--detach` at `HEAD` as
written, and then this task's `crates/envoy-config/src/runtime.rs` was COPIED
into it (`git diff --numstat` inside the worktree confirmed `22 0`, and md5
equality with the main tree's file was asserted). The main tree was still never
mutated — the standing constraint the step exists to enforce — and the control
+ three mutations ran against exactly the bytes this task lands. The
alternative (commit first, then create the worktree at the new commit, per
memory `mutation-checks-collide-with-parallel-subagents`) would have been
equally sound but splits Task 1 across two commits.

### Task 1 gate

- `cargo build --workspace --all-targets` → exit 0, **13** `Compiling` lines
  (non-zero: not a cached no-op).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → exit
  0, **13** `Checking` lines (non-zero: not a cached no-op — the build cache and
  the clippy cache are independent and were measured BOTH cold-no-op at once at
  109.1 state-4, so both counts are gated separately).
- `cargo fmt --all -- --check` → exit 0, silent.
