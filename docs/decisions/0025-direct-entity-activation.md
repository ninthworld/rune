# ADR 0025: Direct entity activation — one gesture vocabulary for common actions

- Status: accepted
- Date: 2026-07-18

## Context

The shipped interaction model routes every entity action through select-then-dock
(ADR 0004/0023): click the card, travel to the action dock, click the labeled
button. That is the right home for *disambiguation* — a permanent with several
abilities must present labeled choices — but it taxes the two highest-frequency
flows in every game with the same three-step dance:

- **Paying mana.** Tapping N lands for an expensive spell costs 2N clicks plus 2N
  pointer travels between board and dock.
- **Combat declarations.** Declaring attackers/blockers first requires finding
  the subject-less "Declare attackers" dock button before any creature can be
  toggled — the creatures themselves are not even interactive until then.

Any fix must hold the architecture's hard rules: `valid_actions[]` drives all
interactivity, the client computes no legality, and every interaction must work
with pointer, touch, and keyboard alike (no hover- or timing-dependent gestures;
44px targets).

## Decision

One **direct-activation vocabulary**, layered on the universal single gesture
(click = tap = keyboard activate) with no double-click timing windows:

1. **Combat-declaration entry on the creature.** A permanent that has no
   subject-actions but is a candidate of exactly one offered subject-less
   `declare_attackers`/`declare_blockers` action is itself interactive (and wears
   the playable affordance). Activating it enters that declaration with the
   creature already toggled; further candidates toggle as before and the
   declaration still submits atomically via Confirm. Safe on first gesture
   because a declaration is reversible until confirmed. Only the two combat
   declarations participate; other multi-select flows (mulligan bottoming, zone
   selections) keep their explicit entry.
2. **One-gesture mana.** The server marks the activation of a **mana ability**
   (CR 605.1a: no targets, no stack, only mana production) with a new optional
   `ValidAction.mana_ability` flag, computed by the engine's existing classifier.
   When an entity's sole offered action carries the flag, the first activation
   fires it — tap the land, get the mana. The client keys off the flag alone and
   never classifies abilities.
3. **Second activation fires the sole action.** Otherwise the first activation
   selects (inspect + dock exactly as before); activating the already-selected
   entity again fires its single offered action — entering targeting mode if it
   carries requirement slots. An entity with several actions keeps the dock as
   the disambiguator (the repeated activation is a no-op). This replaces
   click-again-to-deselect; deselection remains on Escape, the dock's clear
   affordance, and selecting elsewhere.

The gesture only changes how a server-offered action is *reached*, never what is
legal. During targeting/multi-select flows the vocabulary is suspended — the only
interaction remains picking candidates. All three rules apply identically to
pointer, touch, and keyboard, because they attach to the entity's one activation
event.

## Consequences

- Paying {5}{G}{G} drops from 14 pointer actions to 7 clicks on the lands
  themselves; declaring attackers starts on the attackers. The dock remains the
  authoritative, labeled home for every action and the only home for ambiguous
  ones.
- The protocol gains one optional, backward-compatible field (`mana_ability`,
  omitted when false). Older clients ignore it; older servers simply never set
  it and the client falls back to select-then-act.
- Combat candidates now render as actionable before the declaration is entered —
  a presentation change that doubles as discoverability (the gold bar says
  "click me").
- Clicking a selected entity no longer deselects it. The risk of an accidental
  second-click firing is bounded: the first click already surfaced the selection
  ring and the dock's labeled action, and every destructive-ish flow (targeting,
  declarations) opens a further reversible step rather than committing.
- The engine-side future for cost payment (server-computed auto-tap payment
  plans) remains open on the roadmap; this ADR removes most of its urgency
  without preempting it.
