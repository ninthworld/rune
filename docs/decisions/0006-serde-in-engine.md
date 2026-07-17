# ADR 0006: Serde in the engine for embedded card data

- Status: accepted; catalog layout amended by ADR 0018
- Date: 2026-07-10
- Issue: #25

## Context

The engine needs typed card data but may not perform runtime I/O. Maintaining a custom JSON
parser would add code without improving the purity boundary.

## Decision

`rune-engine` may depend on `serde` and `serde_json` to parse card data embedded at compile
time. Runtime filesystem and network access remain forbidden.

ADR 0018 later replaced the original monolithic snapshot with a build-generated manifest of
per-card files. That build step may read the catalog while compiling; the running engine still
receives only embedded strings and parses them in memory.

Any additional runtime dependency requires its own architectural justification and must pass
the repository license and supply-chain policy.

## Consequences

Card data receives derived deserialization and closed-schema validation without weakening the
runtime I/O boundary. The engine no longer has an empty dependency table, so purity is enforced
by the nature and use of dependencies rather than their count.
