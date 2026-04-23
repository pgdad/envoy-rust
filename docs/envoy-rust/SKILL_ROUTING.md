# envoy-rust Skill Routing

> Verbatim copy of the phase lifecycle state machine from §5 of
> `BOOTSTRAP_PROMPT.md`. This file exists so an executing session does not need
> to re-parse the whole prompt to route its next action. If §5 and this file
> ever diverge, §5 wins and this file must be corrected via an ADR.

```
0. Phase not yet in ROADMAP.md
   → superpowers:brainstorming (adds/refines row in ROADMAP)

1. Phase in ROADMAP, directory does not exist
   → create docs/envoy-rust/phases/NN-slug/
   → superpowers:brainstorming (scoped to THIS phase)
   → output: SPEC.md

2. SPEC.md exists, PLAN.md does not
   → superpowers:writing-plans
   → output: PLAN.md
   → GATE: if PLAN.md > ~25 tasks OR > ~1500 LoC estimated
           → split into NN.1, NN.2, …; update ROADMAP + STATE; stop

3. PLAN.md exists, implementation incomplete
   → superpowers:executing-plans (or subagent-driven-development for independent tasks)
   → TDD per superpowers:test-driven-development on every task
   → append to PROGRESS.md on each task completion

4. Implementation complete, not verified
   → superpowers:verification-before-completion
   → run: cargo build --workspace --all-targets,
          cargo clippy --workspace --all-targets --all-features -- -D warnings,
          cargo fmt --all -- --check,
          cargo test --workspace,
          cargo deny check,
          cargo fuzz run <target> [for each new fuzz target, short-budget CI run],
          differential suite for phase's feature surface, conformance suites
   → quote all command outputs into PROGRESS.md

5. Verified, not reviewed
   → superpowers:requesting-code-review
   → output: REVIEW.md
   → if issues → back to step 3 (NOT 4) until REVIEW.md approved

6. Reviewed and approved
   → commit (message format: "phase NN: <title> [ADR-xxxx,...]")
   → ROADMAP.md status → done
   → STATE.md advanced to next phase or "awaiting next planning"
   → phase ends; session may exit

Deviations:
  * Ambiguity           → ADR + proceed
  * Blocked by upstream → ROADMAP status=blocked, STATE note, exit clean
  * Unexpected state    → superpowers:systematic-debugging FIRST
```
