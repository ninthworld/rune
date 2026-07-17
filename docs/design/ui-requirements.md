# Web client UI requirements

The RUNE web client must present any supported game state and collect choices without
implementing game rules. This document describes target capabilities, not a claim that each
one is implemented. Current delivery status belongs in [`../roadmap.md`](../roadmap.md).

## Architectural requirements

- The server is authoritative. The client renders `GameView` and sends an issued action id
  with server-enumerated choices.
- `valid_actions` is the only source of actionable elements. The client does not infer
  legality, costs, targets, combat outcomes, or terminal state.
- A fresh `GameView` and its prompt data reconstruct the in-game UI. Local state may retain
  only ephemeral presentation choices such as selection or an open inspector.
- Hidden information is absent unless the server explicitly reveals it.
- Every pointer interaction has keyboard and touch equivalents; no action depends only on
  drag or hover.
- Current characteristics, counters, damage, statuses, and results are displayed exactly as
  the server supplies them.
- Because `valid_actions` enumerates every legal interaction, only currently offered
  interactions receive controls and visual emphasis. The UI never renders speculative
  controls for actions the server has not issued.

## Layout and devices

No screen size, aspect ratio, or orientation is guaranteed. Layout derives from measured
viewport geometry and detected input capabilities (pointer precision, hover, keyboard,
gamepad), not from an enumerated device or breakpoint list. The same abstract interaction
verbs — focus or select, confirm, inspect, back or cancel, pass — must be reachable by
pointer, touch, and keyboard today, and by controller focus without redesign.

At every supported geometry the battlefield and the receiver's hand are the primary
surfaces and claim most of the viewport; persistent chrome docks around the board and may
condense or collapse, but never displaces the board into a scrolled document flow.
Information regions keep stable positions across states so play does not require visually
re-locating controls; density and emphasis, not region order, are what vary.

Adaptation must run in both directions. On large viewports the layout spends the
available space — card sizes and region breathing room scale up and the table stays
centered — rather than anchoring content to one corner and leaving the remainder empty.

## Table and zones

The table must represent:

- each player’s battlefield, with controller determining placement and owner displayed when
  different;
- the receiver’s hand and opponent hand counts;
- ordered graveyard and exile piles with browsable public contents;
- library counts and any server-revealed cards;
- the stack in resolution order;
- attachments, counters, marked damage, tapped state, attacking and blocking relationships;
- face-down, transformed, copied, phased-out, or merged objects when the engine supports
  those states; and
- command, sideboard, and other format zones when their formats are implemented.

Large zones need scrolling or virtualization. Large battlefields may collapse only objects
with identical complete state; a prompt must still allow selection of each physical object.
Tokens leaving the battlefield must not appear as cards in another zone.

Tapped state must render without colliding with neighboring objects: layout reserves
whatever footprint the tapped treatment occupies. Zone piles must be locatable at a
glance as spatial objects in the owner's board area, not summary text in chrome, and a
pile must be able to display a server-revealed card (for example a revealed library top
card) in place.

## Cards and inspection

At battlefield scale, a card must remain identifiable by name, frame color or monogram,
mana cost when relevant, computed power/toughness, and essential state badges — including
keyword indicators and the presence of an activated ability. A player must not need
per-card inspection to learn that a creature flies or that a permanent can be activated.
Full inspection must expose all server-provided characteristics, rules text, keywords,
counters, attachments, and linked objects.

Inspection affordances must not add permanently visible chrome to every card. Inspect is
reached through selection, hover dwell, or long-press — each input class has a path, and
none requires a dedicated always-on control per object.

The client uses no official images or frames. Color is never the only carrier of identity or
selection state. Printed and current characteristics must be clearly distinguished if both
are ever supplied.

Any visible card in a public or player-visible zone must be inspectable by pointer, keyboard,
and touch. Hover may provide a transient preview but cannot be the only path.

## Players and global state

Player surfaces must display, when supplied:

- life, hand size, library size, graveyard size, and exile size;
- active player, priority holder, turn number, and current phase or step;
- mana pool and player counters or statuses;
- eliminated or disconnected state; and
- turn order, teams, commander damage, or format designations when supported.

Layouts must handle large numeric values and 2–8 player tiles without moving the receiver
from the bottom interaction area. Multiplayer support must not assume every attacker targets
the same defending player.

## Stack, priority, and timers

The stack must show objects in resolution order, including synthetic entries for activated
and triggered abilities. A stack entry must be able to show its controller, source, targets,
modes, and values when the protocol supplies them.

Priority, active player, phase, and turn must remain visible. On-demand expansions of the
phase display (the full step sequence) must render entirely within the viewport at every
supported geometry, never clipped by an edge. A decision deadline is a server-owned value:
the client displays a live countdown and warning but never enforces or restarts it.

Priority automation, stops, auto-yield, and hold-priority controls require a server contract
before client implementation. The client cannot decide that a player has no meaningful
response.

## Prompt system

All decisions use a consistent prompt surface with:

- the source and human-readable question;
- progress and selection count;
- server-supplied legal candidates or options;
- inspect access for relevant cards;
- confirm and, only when allowed, back or cancel; and
- an atomic submission containing every required slot.

The prompt system must be extensible to targets, multi-target selection, option lists, zone
selection, ordering, modes, numeric values, divided quantities, searches, pile splits,
alternative costs, mana choices, and multiplayer attack targets. Adding a prompt kind is a
protocol change and must not embed its legality in the client.

Rejected input produces non-blaming feedback and a state refresh. Resolution failures and
fizzles should be explained by server-supplied log data.

## Combat

The UI must support server-issued attacker and blocker selections, multiple blockers,
attacks against different legal defenders, damage-order or division prompts, and the distinct
first-strike and normal damage steps.

Combat relationships must remain readable on crowded boards. Focus may isolate one object’s
links instead of drawing every line at once. State changes during combat are rendered from the
latest view, not replayed as client-owned combat state.

Any combat forecast must be supplied by the server. The client must not calculate predicted
damage, legality, or deaths.

## Session and game lifecycle

The client must support connection, room creation or joining, deck submission, readiness,
mulligans, game play, game over, concession, disconnect, and per-tab reconnect. Future
spectators receive a redacted view and must not rely on owning a hand.

Between-game sideboarding, matches, draw offers, takebacks, and simultaneous multiplayer
decisions require explicit server and protocol support before UI work begins.

## Comprehension

The UI should make a game understandable without external narration:

- a structured, redacted game log with inspectable entity references;
- clear explanations for rejected actions and fizzled effects;
- visible recent changes and unread activity after returning to the tab;
- zone counts and ownership at a glance; and
- inspect access from logs, prompts, zones, the stack, and the battlefield.

Log content and hidden-information history must come from the server. The client cannot
reconstruct events by comparing private assumptions across views.

## Accessibility and input

- Interactive targets are at least 44 CSS pixels where practical.
- Keyboard navigation reaches every action, prompt, browser, and inspector with visible focus.
- Prompt and log text remains in the DOM for screen readers.
- Selection and targeting differ by shape or weight as well as color.
- Text scaling preserves usable targets and avoids clipping critical values.
- Reduced-motion mode can skip every animation without changing state.
- Audio or system notifications are optional attention aids and must be independently muted.

Controller support uses the same select, confirm, inspect, back, and cancel actions as other
inputs. It is a future capability, not a reason to make current controls drag-dependent.

## Performance and determinism

The target envelope includes 100 or more permanents on one battlefield, up to eight player
areas, public-zone browsers with dozens of cards, and logs with thousands of entries.

Layout is deterministic for the same normalized `GameView`, viewport, and mode. Rendering may
animate the difference between views, but the latest view is already authoritative and input
on a live prompt cannot wait for an animation queue.

## Long-term formats and deck construction

The client will eventually include deck construction and saved deck lists. Card search,
filtering, quantities, and format feedback are presentation tools; the server remains the
authority on card availability and deck legality.

Team formats such as Two-Headed Giant require team-aware seating, shared resources, team turn
ownership, and layouts that can place more than one player in the receiver’s side of the
table. Those requirements follow the free-for-all multiplayer foundation rather than being
designed out of the current layout.

Archenemy, Planechase, ante, subgames, banding, and Un-set mechanics require separate engine
and protocol decisions before UI requirements are expanded around them.
