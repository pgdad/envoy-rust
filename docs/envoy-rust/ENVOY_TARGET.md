# Upstream Envoy Target Pin

> Pinned during phase 00. Upgrading this pin is its own phase per doctrine rule
> D-3.7 and supersedes ADR-0004 with a new ADR.

## Pin

- **Image:** `envoyproxy/envoy:v1.33.0`
- **Digest:** `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`
- **Upstream release notes:** https://github.com/envoyproxy/envoy/releases/tag/v1.33.0
- **Proto tree commit:** `b0f43d67aa25c1b03c97186a200cc187f4c22db3`
- **xDS transport version:** v3

## How to refresh the pin

Upgrading the pin is its own phase per doctrine rule D-3.7. The refresh phase must:

1. Open a new phase in `ROADMAP.md` titled "Refresh upstream Envoy pin to <new-tag>", depending on the most recent trunk/feature phase.
2. Add an ADR that supersedes `ADR-0004`, naming the old digest, new digest, new tag, and any doctrine-surface changes in the release notes.
3. Re-run every existing differential fixture against the new image. Any red fixture is either a product fix (update envoy-rust) or a contract fix (update `BEHAVIOR_CONTRACT.md`, documented in the same or a follow-up ADR), never both silently — per doctrine rule D-3.3.
4. Update this file with the new fields and commit.

This file is otherwise not edited outside a refresh phase.
