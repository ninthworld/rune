# ADR 0004: Actions carry subjects; entities own their actions

- Status: accepted
- Date: 2026-07-10

## Context
Rendering every valid action as a bar button does not scale (100 playable cards
= 100 buttons) and duplicates the cards themselves, which are already visible
and tappable.

## Decision
Every entry in valid_actions[] carries `subject: [entity_id]`. Clients render
entity-subject actions as interactivity on the entity (full opacity + focus
response = actionable); the action bar shows only subject-less actions (pass,
end turn, confirm) plus a contextual echo of the currently selected entity's
actions. Interaction is select-then-confirm on every input method.

## Consequences
The bar is O(1) in board size. Multi-action entities disambiguate through the
contextual echo, which doubles as the accessibility and controller surface.
The protocol must maintain subject ids for every action — a server obligation.
