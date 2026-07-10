# ADR 0005: License selection

- Status: proposed (decision needed before first public release)
- Date: 2026-07-10

## Context
The brief requires the project remain free and non-monetized (WotC fan content
policy). Prior art: Forge is GPL; XMage is MIT. Copyleft (GPL/AGPL) structurally
prevents closed monetized forks; permissive (MIT/Apache-2.0) maximizes adoption
including in the planned Tauri/mobile embeddings.

## Decision
Pending maintainer choice. Until decided: LICENSE contains a placeholder, crate
manifests say `license = "TBD"`, and no external code may be vendored in.

## Consequences
The repository must not accept third-party code contributions of significant
size until this is resolved, to avoid relicensing friction.
