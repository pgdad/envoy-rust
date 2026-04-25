# Phase 02.2 Progress

## Task 1 — ADRs 0015 + 0016 (2026-04-25)

- Commit: <SHA>
- Change: appended ADR-0015 (cross-container host reachability via host.docker.internal + host-gateway) and ADR-0016 (phase 02 TCP proxy runs with Envoy's default enable_half_close: false) to DECISIONS.md.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 16 (ADR-0001 through ADR-0016).
