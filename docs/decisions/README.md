# Architectural decisions

Every ADR in this directory is **live and binding**. Nothing here is superseded, and there is
no archive: a decision that stops being true is rewritten or deleted, never left standing as
history for someone to mistake for guidance.

They are numbered in dependency order — foundations first, then the rules mechanics built on
them, then card data, the server, and the client. An ADR cites only ADRs before it, so reading
them in order never requires a forward reference.

| ADR | Decision |
| --- | --- |
| [0001](0001-server-authoritative-immutable-engine.md) | Server-authoritative rules; immutable engine state |
| [0002](0002-serde-in-engine.md) | serde in the engine, for compile-time-embedded data only |
| [0003](0003-card-effect-ir-hybrid.md) | Card behavior as a declarative IR with a code escape hatch |
| [0004](0004-targeting-model.md) | End-to-end targeting model |
| [0005](0005-computed-characteristics-and-layers.md) | Computed characteristics and the CR 613 layer system |
| [0006](0006-deterministic-seeded-shuffle.md) | Deterministic seeded shuffle |
| [0007](0007-game-log-history.md) | Structured game-log history in `GameState` |
| [0008](0008-functional-card-definitions.md) | Functional card definitions and stable `FunctionalId` |
| [0009](0009-real-functional-card-data.md) | Real functional card data from a single set |
| [0010](0010-priority-automation.md) | Priority automation — engine predicate, server policy |
| [0011](0011-e2e-browser-test-strategy.md) | Browser end-to-end test strategy |
| [0012](0012-user-side-card-art.md) | Player-side, opt-in, device-local card art |
| [0013](0013-mid-resolution-player-choices.md) | How the engine poses a mid-resolution player choice |
| [0014](0014-optional-effects.md) | Optional effects, and paying for one mid-resolution |
| [0015](0015-tokens.md) | What a permanent is, when it is not a card |

New ADRs copy [`0000-template.md`](0000-template.md) and take the next free number.

**Write an ADR after a decision survives contact with working code, not before.** A design
document written ahead of the code it describes is speculation with a version number.
