# Phase 00 Progress

## Task 1 — ADR-0002 (2026-04-23)
- Commit: 3fd0a97
- Change: appended ADR-0002 (GitHub Actions as CI provider) to DECISIONS.md
- Verification: `grep -q '^## ADR-0002' DECISIONS.md` → exit 0

## Task 2 — ADR-0003 (2026-04-23)
- Commit: 95839ba
- Change: appended ADR-0003 (Rust edition 2024) to DECISIONS.md
- Verification: `grep -q '^## ADR-0003' DECISIONS.md` → exit 0

## Task 3 — ADR-0004 + ENVOY_TARGET.md (2026-04-23)
- Commit: 9f5d1d2
- Change: ENVOY_TARGET.md populated with v1.33.0 pin (multi-arch index digest sha256:56da5a…70c2, proto tree commit b0f43d6); ADR-0004 appended
- Verification: grep checks for ADR-0004, sha256:, Proto tree commit: all exit 0; no `TBD` in either file
- Deviation: local Docker daemon has an IPv6 routing bug; digest resolved via Docker Hub public API (https://hub.docker.com/v2/repositories/envoyproxy/envoy/tags/v1.33.0) instead of `docker inspect`. Value is the canonical multi-arch manifest-index digest — equivalent to what `docker inspect` would report against a freshly-pulled manifest.
