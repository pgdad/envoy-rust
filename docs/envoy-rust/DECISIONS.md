# envoy-rust Architecture Decision Records

> Append-only log. Landed ADRs are never edited; they are superseded by a new
> ADR that explicitly names the superseded ADR number. Format per D-3.5 of
> `MISSION.md`.

Each ADR uses the structure:

```
## ADR-NNNN: <title>

- Date: YYYY-MM-DD
- Status: proposed | accepted | superseded-by ADR-MMMM
- Context: <why the decision was needed>
- Options considered: <list, briefly>
- Decision: <the choice>
- Rationale: <why this choice over the others>
- Consequences: <what this implies, including follow-up ADRs if any>
```

---

## ADR-0001: Bootstrap prompt version pin

- Date: 2026-04-23
- Status: accepted
- Context: The project is driven end-to-end by `BOOTSTRAP_PROMPT.md`. Every later ADR and every phase artifact implicitly assumes a specific version of that prompt (wording of doctrine rules, numbering of sections, contents of the seeded roadmap). A future edit to the prompt must therefore be a deliberate event, not a silent drift.
- Options considered:
  - Pin to the SHA of the commit that introduced `BOOTSTRAP_PROMPT.md` as this ADR does.
  - Do not pin; rely on the current working-tree contents at bootstrap time.
  - Embed the prompt's contents inline in `MISSION.md`.
- Decision: Pin the prompt to the git SHA of the commit that last modified `BOOTSTRAP_PROMPT.md` at bootstrap time: `3ee76395238123f4b9214c8998907ee6c830d3e2`. `MISSION.md` also contains a verbatim copy of §§2 and 3 of the prompt at that SHA, so the mission and doctrine survive in-tree without rereading the prompt.
- Rationale: An ADR is the project's permanent, append-only record; referencing a SHA lets every subsequent artifact be traced to an exact prompt version. A later prompt edit becomes ADR-MMMM ("supersede the prompt pin to SHA X") and the change log is unambiguous.
- Consequences:
  - All subsequent phases assume the prompt at this SHA. Any behavior described by the prompt that this ADR log or a later ADR does not supersede is in force.
  - If `BOOTSTRAP_PROMPT.md` is edited, a new ADR must be landed referencing the new SHA and any doctrine deltas, and `MISSION.md` must be updated in the same commit.

---

## ADR-0002: GitHub Actions as the CI provider

- Date: 2026-04-23
- Status: accepted
- Context: Phase 00 needs a CI pipeline that runs the full green-build gate (`cargo build`, `cargo clippy`, `cargo fmt --check`, `cargo test --workspace`, `cargo deny check`) on every push and PR. Doctrine D-3.6 makes the green-build gate non-negotiable, so the CI choice is a foundation for every later phase.
- Options considered:
  - **GitHub Actions** — free for public OSS repos, first-class Rust toolchain action (`dtolnay/rust-toolchain`), mature cargo cache action (`Swatinem/rust-cache`), and docker-in-docker on `ubuntu-latest` runners (required for `testcontainers` → upstream Envoy).
  - **Buildkite** — requires self-hosted runners; upfront infra cost that does not pay back for a single-maintainer OSS project.
  - **Drone** — requires a hosted controller; comparable feature set to GH Actions without the ecosystem.
  - **GitLab CI** — would require mirroring the repo to GitLab; divergent source-of-truth risk.
- Decision: GitHub Actions.
- Rationale: zero infra cost, docker-in-docker on `ubuntu-latest` is exactly what the differential harness needs, and the ecosystem actions (`dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `taiki-e/install-action@cargo-deny`) cover every step of the phase-done gate without bespoke scripting.
- Consequences:
  - The single CI job lives at `.github/workflows/ci.yml`, running on `ubuntu-latest`. macOS and Windows runners are explicitly out of scope (Envoy's production posture is Linux; see SPEC §4).
  - A future "CI scale-out" phase (adding matrix runs, release workflows, etc.) lands as its own phase with its own ADR. The only artifact this ADR commits to is a single linting/build/test job.

---

## ADR-0003: Rust edition 2024 for all workspace crates

- Date: 2026-04-23
- Status: accepted
- Context: Every crate introduced starting in phase 00 declares an `edition = "…"` in its `Cargo.toml`. We must commit to a single edition at bootstrap to avoid a mass edition-bump phase later, which would touch every crate root's macro behavior, Pin/disjoint-borrow rules, and `expect`/`assume` forward-compatibility semantics.
- Options considered:
  - **Edition 2024** — stabilized in rustc 1.85. Our toolchain pin is 1.95.0 (D-3.9), well past the stabilization point. Tightens closure capture rules, `async fn` in traits, Pin semantics.
  - **Edition 2021** — the still-most-popular edition; safe and boring.
  - **Edition 2018** — legacy; picks up none of the last six years of ergonomics work.
- Decision: Edition 2024 for every workspace crate.
- Rationale: since the toolchain pin (1.95.0) strictly dominates the 2024 stabilization threshold (1.85), there is no compatibility cost. Future edition bumps become the next-edition problem, not the "we-already-skipped-three-editions" problem.
- Consequences:
  - Every new `Cargo.toml` this phase and later specifies `edition = "2024"`. The phase-00 workspace includes this in both `crates/envoy-bin/Cargo.toml` and `tests/differential/Cargo.toml`.
  - A future toolchain-bump phase (per D-3.9) must verify edition-2024 behavior on the new toolchain before landing.
