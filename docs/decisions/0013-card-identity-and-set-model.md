# ADR 0013: Card identity, printings, and format policy

- Status: accepted in part; catalog identity and layout superseded by ADR 0018
- Date: 2026-07-11
- Issue: #106

## Context

A card’s rules identity is independent of a particular set printing. Duplicating behavior for
each printing would make reprints drift. Game setup parameters also need separation from
server-owned deck and format policy.

## Decision

### Functional card and printing

One printing-independent card record owns all characteristics and behavior. A printing contains
only set code, collector number, rarity, and a reference to that card. Adding a reprint changes
no rules logic.

ADR 0018 replaces this ADR’s original integer-authored identity and monolithic file layout with
stable `FunctionalId` values, per-card catalog files, and build-interned `CardId` handles. The
card/printing separation remains in force.

Decklists reference functional card identity, not a printing or build-local engine handle.
Printing preference is presentation metadata and does not enter `GameState`.

### Game setup and format

`rune-engine::GameSetup` contains only rules-affecting construction data: player decklists,
starting life, starting hand size, and the deterministic random seed.

The server maps a named `game_setup` id to those engine parameters plus deck-policy rules such
as size, copy limits, and basic-land exemptions. The server validates deck legality before game
construction; the engine remains independent of named formats and lobby policy.

## Consequences

Rules exist once per functional card, decklists survive reprints and catalog rebuilding, and
server formats can evolve without putting policy in the engine. Presentation-specific printings
would require separate client or deck metadata because the running game intentionally stores
only functional identity.
