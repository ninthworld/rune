# ADR 0005: License selection

- Status: accepted
- Date: 2026-07-10

## Context

RUNE needs a standard open-source license for code contributions and reuse. The project’s
separate fan-content policy forbids monetization, official assets, and branding in RUNE’s own
distribution; a source-code license does not enforce that policy.

## Decision

RUNE source code is licensed under the MIT License. Contributions are accepted under the same
license. Third-party code must be license-compatible and pass `cargo-deny` policy.

The copyright holder is “the RUNE authors” so contributions accrue to the project.

## Consequences

MIT keeps reuse and future client embedding simple, but it permits commercial and closed-source
reuse. The legal constraints in the project brief continue to govern this project’s content
and distribution independently of the code license.
