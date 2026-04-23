# Upstream Envoy Target Pin

> To be filled during phase 00. Must pin an upstream Envoy Docker image by tag
> and SHA256.

## Required fields (all TBD until phase 00)

- **Image:** `envoyproxy/envoy:<tag>` — TBD
- **Digest:** `sha256:<hex>` — TBD
- **Upstream release notes:** link to the Envoy release announcement for the pinned tag — TBD
- **Proto tree commit:** the matching `envoyproxy/envoy` git SHA whose `api/` tree corresponds to this image — TBD
- **xDS transport version:** v3 (fixed; v2 is retired upstream — confirm during phase 00)

## How to refresh the pin

Upgrading the pin is its own phase per doctrine rule D-3.7. The refresh phase must:

1. Open a new phase in `ROADMAP.md` titled "Refresh upstream Envoy pin to <new-tag>", depending on the most recent trunk/feature phase.
2. Add an ADR that supersedes the previous pin ADR, naming the old SHA256, new SHA256, new tag, and any doctrine-surface changes in the release notes.
3. Re-run every existing differential fixture against the new image. Any red fixture is either a product fix (update envoy-rust) or a contract fix (update `BEHAVIOR_CONTRACT.md`, documented in the same or a follow-up ADR), never both silently — per doctrine rule D-3.3.
4. Update this file with the new fields and commit.

This file is otherwise not edited outside a refresh phase.
