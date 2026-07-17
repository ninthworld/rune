# ADR 0004: Actions carry subjects; entities own their actions

- Status: accepted
- Date: 2026-07-10

## Context

Rendering every valid action as a bar button does not scale (100 playable cards
= 100 buttons) and duplicates the cards themselves, which are already visible
and tappable.

## Decision

An entity-owned entry in `valid_actions[]` carries `subject: [entity_id]`. Clients render it
as interactivity on that entity; full opacity and a visible focus response communicate that
it is actionable. The action bar shows only subject-less actions such as pass, end turn, and
confirm, plus a contextual echo of the selected entity's actions. Interaction is select then
confirm on every input method.

## Consequences

The bar is O(1) in board size. Multi-action entities disambiguate through the
contextual echo, which doubles as the accessibility and controller surface.
The protocol and server must maintain stable subject ids for every action within each game
view.
