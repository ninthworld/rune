# Architectural decisions

Every ADR in this directory is **live and binding**. Nothing here is superseded, and there is
no archive: a decision that stops being true is rewritten or deleted, never left standing as
history for someone to mistake for guidance.

Numbers are allocation order and are not contiguous. A gap means a decision was retired; it
carries no other meaning, and nothing points back at one.

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

**Write an ADR after a decision survives contact with working code, not before.** A design
document written ahead of the code it describes is speculation with a version number.
