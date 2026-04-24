# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `02`
**slug:** `02-tcp-proxy`
**directory:** `docs/envoy-rust/phases/02-tcp-proxy/` (does not yet exist — created by the next session per `SKILL_ROUTING.md` state 1).
**status:** phase 02 lifecycle **state 1 (pending brainstorm)** — row exists in `ROADMAP.md` with `status: planned`; no SPEC / PLAN / PROGRESS yet.

Phase 01 (`01-static-bootstrap-config`) is **done** as of the phase-done
commit whose subject matches SPEC §8 and includes
`[ADR-0008, ADR-0009, ADR-0010, ADR-0011, ADR-0012]`. `ROADMAP.md` row 01
is now `done`. Phase-01 `REVIEW.md` verdict is **Approved** (state 5
complete; I1 closed in-phase; I3/I4/M1 tracked forward).

Phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

**`superpowers:brainstorming`** — the next session brainstorms phase 02
scoped to the static TCP proxy filter surface: listener + TCP proxy
network filter + static cluster + round-robin load balancer (plaintext).
Per `ROADMAP.md` row 02 summary, the acceptance signal is "TCP proxy
fixture green" against upstream Envoy v1.33.0. The brainstorm output is
`docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` per `SKILL_ROUTING.md`
state 1.

Inputs the brainstorm should read:

- `BOOTSTRAP_PROMPT.md` §4 (phase naming / slug convention) and §8 row 02.
- `docs/envoy-rust/MISSION.md`, `docs/envoy-rust/BEHAVIOR_CONTRACT.md`,
  `docs/envoy-rust/DECISIONS.md` (ADR-0001..0012).
- `docs/envoy-rust/phases/00-bootstrap/SPEC.md` and `REVIEW.md` (for
  deferrals carried forward: `Minor` M1, M2, M4–M8; `Important` I3
  SIGKILL→SIGTERM switch — still blocked by the `nix` crate unless
  phase 02 takes it under an ADR).
- `docs/envoy-rust/phases/01-static-bootstrap-config/SPEC.md`,
  `PROGRESS.md`, and `REVIEW.md` (for the starter-items rollover
  enumerated in the Notes section below).

## Last commit

Phase-01 phase-done final commit:
`phase 01: Static bootstrap config loader + admin /ready [ADR-0008, ADR-0009, ADR-0010, ADR-0011, ADR-0012]`.
Flips `ROADMAP.md` row 01 → `done` and advances this file to phase 02.

## Last updated

2026-04-24 (phase 01 complete; STATE advanced to phase 02 at lifecycle
state 1).

## Notes

### Phase-02 starter items (carry forward from phase-01 REVIEW.md §9)

These three items are tracked forward from phase-01's state-5 REVIEW as
explicit starter work for phase 02. The phase-02 brainstorm should decide
whether to fold each into the phase-02 SPEC or leave them for an opportunistic
cleanup commit alongside phase-02 work.

- **I3** — add four unit tests for `decode_chunked` in
  `tests/differential/src/lib.rs` (covering empty chunk, chunk-size
  extension, truncated body, trailer headers). Helper landed in commit
  `5b852ce` during phase-01 state-4 CI-fix work with only transitive
  exercise via the Docker-gated `admin_ready_fixture` test.
- **I4** — tighten the admin 8 KiB header cap in
  `crates/envoy-bin/src/admin.rs:156–170` from "effectively ~9 KiB"
  (the `buf.len() >= MAX_REQUEST_HEAD` check fires *before* each
  1024-byte read) to an exact boundary. Not a correctness bug; ~2-line
  fix (clamp the read slice to `MAX_REQUEST_HEAD - buf.len()`).
- **M1** — retarget the stale `TODO(phase-01)` comment in
  `tests/differential/src/subject.rs:25–32`. Phase 01 did not pick up
  the phase-00 I3 SIGKILL→SIGTERM switch (the `nix` crate is still not
  on the D-3.2 permitted-foundations list). The TODO target moves to
  phase 04 or later — whichever phase genuinely takes the `nix` dep
  under a new ADR.

### Phase-00 deferrals still open

- Minors M1, M2, M4, M5, M6, M7, M8 (see
  `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`). None blocked phase 01.
- Important I3 (SIGKILL → SIGTERM graceful termination of the subject
  subprocess): still deferred. The `nix` crate remains the stated
  blocker (not on the D-3.2 permitted-foundations list). Phase-01
  SPEC explicitly did not pick it up. A future phase that genuinely
  needs `nix` adds it under a new ADR and closes this item.
- N2 (phase-00 deferred Minor — `deny_unknown_fields` regression-test
  gap on deeper struct levels): **closed** by phase-01 Task 4 Step 4
  via five new regression tests.

### Phase-01 ADR ledger

ADR-0008 (envoy-config extraction), ADR-0009 (cargo-fuzz + libfuzzer-sys
as fuzz-only dev deps), ADR-0010 (nightly toolchain, explicit `+nightly`
CI invocation; workspace-root pin stays stable), ADR-0011 (phase-01
defers response-header equivalence to phase 04; `server: envoy-rust`
tolerated until then), ADR-0012 (nested nightly pin in fuzz subcrate;
narrowly supersedes ADR-0010 on that single sub-point while preserving
its main decision). ADR-0010 is unedited per D-3.5 append-only doctrine;
ADR-0012 is the retroactive ADR that legitimated the nested
`crates/envoy-config/fuzz/rust-toolchain.toml` landed during the state-4
CI-fix run.

### Doctrine reminders

- Any deviation from the state machine requires
  `superpowers:systematic-debugging` before proceeding — see §1 Step E
  of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase
  lifecycle state machine.
