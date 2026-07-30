# Architectural decisions

These ADRs are **live and binding**. Superseded ones live in
[`../archive/decisions/`](../archive/decisions/) — history, not guidance.

> **Naming:** the project was renamed RUNE → SAGE on 2026-07-30. ADRs dated before then say
> "RUNE" in their prose. They are dated records of what was decided, and are left as written;
> crate paths inside them were updated to `sage-*` so links resolve. The current name and scope
> live in [`../brief.md`](../brief.md).

| ADR | Decision |
| --- | --- |
| [0002](0002-server-authoritative-immutable-engine.md) | Server-authoritative, immutable engine |
| [0006](0006-serde-in-engine.md) | serde permitted in the engine for embedded data only |
| [0007](0007-card-effect-ir-hybrid.md) | Card effects as a data IR plus a code escape hatch |
| [0009](0009-targeting-model.md) | Targeting model |
| [0010](0010-computed-characteristics-and-layers.md) | Computed characteristics and the CR 613 layer system |
| [0011](0011-e2e-browser-test-strategy.md) | Browser e2e strategy — **reinstated as a required gate** |
| [0014](0014-deterministic-seeded-shuffle.md) | Deterministic seeded shuffle |
| [0018](0018-scalable-functional-card-definitions.md) | Functional card definitions and stable `FunctionalId` |
| [0020](0020-priority-automation.md) | Priority automation — engine predicate, server policy |
| [0021](0021-game-log-history.md) | Structured game log in `GameState` |
| [0024](0024-user-side-card-art.md) | Player-side, opt-in, device-local card art |
| [0026](0026-real-functional-card-data.md) | Real functional card data (no Oracle text or art) |

New ADRs copy [`0000-template.md`](0000-template.md).

**Write an ADR after a decision survives contact with working code, not before.** Design
documents written ahead of implementation are what produced the archive.
