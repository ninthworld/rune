# RUNE protocol

RUNE uses JSON over one WebSocket connection. Before a game starts, the connection
exchanges complete lobby views and lobby commands. Once the room constructs a game, the
same connection exchanges personalized game views and chosen actions.

The Rust types in `crates/rune-protocol/src/lib.rs` are the wire authority. The TypeScript
mirror in `clients/web/src/protocol.ts` and this document must change with them.

## Message lifecycle

| Phase | Server to client | Client to server |
| --- | --- | --- |
| Lobby | `LobbyView` | tagged `LobbyCommand` |
| Game | `GameView` | `{"type":"choose_action", ...}` or `{"type":"set_stops", ...}` |

The server sends a complete personalized view after every accepted state change and after
rejected or stale input. There is no patch or event-stream protocol. The client reconstructs
its current UI from the latest view.

Empty collections and optional values are generally omitted. Clients must normalize missing
fields to the defaults defined by the protocol types and tolerate unknown fields.

## Game phase

### `GameView`

`GameView` contains only information the receiving player may know. Hidden information is
redacted before serialization.

| Field | Type | Meaning |
| --- | --- | --- |
| `you` | `PlayerId` | Receiver’s opaque player id |
| `my_hand` | `CardView[]` | Receiver’s visible hand |
| `me` | `SelfView` | Receiver’s `life` and `library_size` |
| `opponents` | `OpponentView[]` | Public opponent state and hidden-zone counts |
| `battlefield` | `Permanent[]` | Public permanents and computed state |
| `stack` | `StackItem[]` | Stack objects, bottom first |
| `graveyards` | `ZonePile[]` | Public ordered graveyards |
| `exile` | `ZonePile[]` | Public ordered exile zones |
| `phase` | `Phase` | Current turn step |
| `turn` | `number` | One-based turn number; `0` only for an empty state |
| `active_player` | `PlayerId` | Player whose turn it is |
| `mana_pool` | `string[]` | Receiver’s unspent mana as pip strings |
| `priority_player` | `PlayerId?` | Player currently holding priority |
| `valid_actions` | `ValidAction[]` | Only actions available to the receiver |
| `action_deadline` | `number?` | Seconds remaining for the receiver’s current decision |
| `result` | `GameResult?` | Terminal result; absent during a live game |
| `log` | `GameLogEntry[]` | Bounded, sequence-numbered recent public game history |
| `stops` | `Phase[]` | Receiver’s own priority-stop preferences; omitted when empty |
| `auto_passed` | `boolean` | Whether reaching this state auto-passed the receiver; omitted when `false` |
| `player_names` | `{ [PlayerId]: string }` | Public display names by player id; omitted when empty |

`player_names` maps a `PlayerId` to that player’s chosen display name (issue #294), so
any in-game surface — the turn indicator, player tiles, zone-browser titles, the
game-over verdict — can label `you`, an opponent, the active/priority player, or a winner
without a lobby round-trip. Names are public (no redaction beyond the validation applied
when they are set) and never replace the `p{N}` id an action echoes back. A player with no
name has no entry; the field is omitted from the wire when empty, and a client treats a
missing key as “unnamed”, falling back to a seat-derived label — so an older server that
never sends names keeps working.

### Game log

`log` is a bounded window of `GameLogEntry` values. Every entry has a monotonically
increasing `sequence` and a tagged `event`; a window can start after sequence one, so
clients render the carried entries and do not invent missing history. It is included in
each complete `GameView`, which means reconnecting clients never need an accumulated
local log. Event names are `spell_cast`, `spell_resolved`, `spell_countered`,
`spell_fizzled`, `attackers_declared`, `blockers_declared`, `mulligan`, `hand_kept`,
`life_changed`, `damage_dealt`, `cards_drawn`, `permanent_died`, `step_changed`, and
`game_over`. Named `LogEntity` references have an opaque `id` and server-supplied
`name`; the id may be used for presentational highlighting only. The `name` on every
reference is fixed at the moment the event was recorded, so an entry naming a permanent
stays stable after that permanent leaves play (dies, is bounced) — the server does not
re-resolve names against the current board.

A `cards_drawn` event contains only player and count, never a hidden card identity.
`damage_dealt` reports both lethal and nonlethal damage; its `target` is tagged by
`kind` — `player` (with a `player` id) or `permanent` (with a `LogEntity`). Damage to a
player is a `damage_dealt` event, not a `life_changed` one; `life_changed` carries only
non-damage life movement (life gain, life paid or lost), so the two never double-report
a hit. Events are ordered so a step change precedes the consequences of entering that
step (a `step_changed: draw` precedes its `cards_drawn`; entering combat damage precedes
the `damage_dealt` and `permanent_died` it causes), and `game_over` closes the sequence
after every fact that produced it. Only creatures produce `permanent_died`; an Aura or
other permanent moving to a graveyard is a zone change, not a death.

`Phase` is a snake-case enum:

```text
untap, upkeep, draw, precombat_main, begin_combat, declare_attackers,
declare_blockers, combat_damage, end_combat, postcombat_main, end, cleanup
```

When a room uses a decision clock, `action_deadline` appears only in the deciding
player’s view. It is calculated from an absolute server deadline, so reconnecting does not
restart the clock. The client displays the countdown but does not enforce it. On expiry the
server may pass priority or submit an empty combat declaration; it does not concede for the
player.

`stops` and `auto_passed` carry basic priority automation (issue #264, ADR 0020). `stops`
is the receiver’s own set of steps at which they want to receive priority even when the
engine reports they have no meaningful action — the per-phase opt-in that keeps automation
from skipping past a step they care about. It is set with the `set_stops` message (below),
stored server-side, and reflected here so the stops UI is reconstructable from a single
view and survives reconnect; it is omitted when empty (“stop nowhere”, the default), and a
client treats a missing field as an empty set. `auto_passed` is a display-only flag set on
the broadcast that follows a settle in which the server passed priority on this receiver’s
behalf, so a client can show a transient “passed for you” indicator; it is advisory (the UI
reconstructs without it) and omitted when `false`. The decision of whether a player has “no
meaningful action” is the server’s alone — the client never computes it and never
auto-passes on its own.

### Card and zone views

`CardView` contains server-computed display data:

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | `EntityId` | Per-game card-instance id |
| `functional_id` | `string?` | Stable catalog identity across games and builds |
| `name` | `string` | Display name |
| `type_line` | `string` | Generated type line |
| `mana_cost` | `string?` | Pip notation such as `"{1}{G}"` |
| `rules_text` | `string?` | Server-generated rules text, never stored Oracle text |
| `power`, `toughness` | `string?` | Computed creature values |
| `keywords` | `string[]?` | Lowercase keyword names |

`id` identifies one physical game object and is used by actions. `functional_id` identifies
the underlying card definition and is not a legal-action handle. Clients treat both as
opaque strings.

`OpponentView` contains `player_id`, `hand_size`, `life`, `library_size`,
`graveyard_size`, and optional display-only `statuses`. `ZonePile` contains a `player_id`
and ordered `cards`; the top of the zone is last.

### Permanents and stack objects

A `Permanent` contains:

- `id`, `controller`, `owner`, and a computed `card`;
- optional `tapped` and `attacking` booleans;
- optional `blocking`, naming the attacker’s entity id;
- optional marked `damage`; and
- optional `counters`, each `{ "kind": string, "count": number }`.

These fields describe server-computed state. They do not authorize interaction.

A `StackItem` contains `id`, `controller`, a display `description`, and optional `source`
for an ability originating from a permanent.

### Valid actions

```json
{
  "id": "a2",
  "type": "activate_ability",
  "label": "Tap for mana",
  "subject": ["perm_17"],
  "token": "t00000000deadbeef"
}
```

- `id` is the opaque handle returned in `ChooseAction`.
- `type` is a free-form category used for presentation and input routing.
- `label` is server-supplied display text.
- `subject` names the entities that own the action. An empty subject identifies a global
  action such as passing priority.
- `token` binds the answer to the action’s exact current content. The client echoes it
  verbatim and never derives or parses it.

Current action categories include `pass_priority`, `play_land`, `cast_spell`,
`activate_ability`, `mulligan_decision`, `discard`, `declare_attackers`,
`declare_blockers`, and `concede`. Clients must tolerate unknown categories.

Entity ids are opaque and identify physical game instances. Clients must not parse naming
patterns such as `card_`, `perm_`, or `p`.

### Targets and prompts

A `ValidAction` can request additional choices without adding extra network round trips.
The client collects every answer and submits them atomically with the action.

Target choices use `requirements`:

```json
{
  "id": "a3",
  "type": "cast_spell",
  "label": "Cast Quickfire Bolt",
  "subject": ["card_3"],
  "token": "t00000000cafebabe",
  "requirements": [
    {
      "slot": "t0",
      "prompt": "Target creature or player",
      "candidates": ["perm_9", "p1"]
    }
  ]
}
```

Each requirement contains an opaque `slot`, display `prompt`, and the complete set of legal
candidate entity ids. The server enumerates candidates per slot rather than enumerating the
cartesian product of possible answers.

Non-target choices use tagged `prompts`:

| `kind` | Fields | Answer |
| --- | --- | --- |
| `option` | `slot`, `prompt`, `options[{id,label}]` | One option id |
| `select_from_zone` | `slot`, `prompt`, `zone`, `owner`, `count`, `candidates` | Exactly `count` candidate ids |
| `order` | `slot`, `prompt`, `items` | A permutation of all item ids |

`option` is used for choices such as keep or mulligan. `select_from_zone` supports choices
such as discarding or bottoming cards. `order` is part of the contract for ordering effects,
although current engine gameplay does not emit it.

Combat declarations also use requirements. The attackers slot lists creatures eligible to
attack; blocker slots list eligible blockers for each attacker. Empty selections are legal
for these optional declarations. The server validates cardinality and action-specific rules.

### `ChooseAction`

A plain action returns its id and token:

```json
{
  "type": "choose_action",
  "action_id": "a2",
  "token": "t00000000deadbeef"
}
```

An action with choices includes one `targets` entry for each answered requirement or prompt
slot:

```json
{
  "type": "choose_action",
  "action_id": "a3",
  "token": "t00000000cafebabe",
  "targets": [{ "slot": "t0", "chosen": ["perm_9"] }]
}
```

The shared `targets` name is historical; it carries answers for target requirements and all
prompt kinds. The server regenerates the action, checks the content token, and validates each
choice against the fresh legal set. Invalid input is a no-op followed by the current
`GameView`.

### `SetStops`

The second in-game client message sets the receiver’s priority-stop preferences (issue #264,
ADR 0020): the steps at which they want priority even when they have no meaningful action, so
basic auto-pass does not skip them there.

```json
{ "type": "set_stops", "stops": ["upkeep", "end"] }
```

`stops` is a list of `Phase` values and replaces the seat’s current set wholesale; an empty
set clears all stops and is omitted from the wire (`{"type":"set_stops"}`). The server is
authoritative: it stores the set per seat — so it survives reconnect — and reflects the
accepted set back in `GameView.stops`, which is the sole source of the client’s toggle state
(nothing is stored client-side). An unparseable message is ignored and the current `GameView`
re-sent, the same non-fatal pattern the lobby uses. Automation itself (whether an idle seat’s
priority is auto-passed) is a server decision; the client only configures where to stop and
renders the `auto_passed` indicator.

### Game result

When the game ends, `result` is present and `valid_actions` is empty:

```json
{
  "winner": "p0",
  "losers": ["p1"],
  "reason": "decked"
}
```

`winner` is absent for a draw. `reason` is one of `life_zero`, `decked`, or `concede`.
Further submitted actions are rejected and the final view is re-sent.

## Lobby phase

### `LobbyView`

`LobbyView` is the complete pre-game state for one connection:

| Field | Type | Meaning |
| --- | --- | --- |
| `session` | `SessionToken` | Private reconnect token |
| `you` | `PlayerId` | Public player identity |
| `name` | `string?` | The connection’s own display name, if set; omitted when unset |
| `room` | `RoomView?` | Current room, if joined |
| `directory` | `RoomSummary[]` | Public rooms available to browse |
| `valid_commands` | `string[]` | Only commands currently available |

The client stores `session` per browser tab and echoes it on a later `hello`. It is an
identity/reconnect handle, not a user account or human authentication credential.

`RoomView` contains an opaque `room_id`, a `config`, and the ordered seat roster. The room
config contains `seats` and an opaque `game_setup` id. The lobby validates a 2–8 seat range
and requires the setup id to exist in the server format registry. Supported gameplay remains
two-player even though the lobby shape is wider.

Each seat contains:

- zero-based `seat` index;
- optional public `occupied_by` player id;
- optional public `name`, the occupant’s chosen display name (issue #294), omitted for
  an empty or unnamed seat;
- `decked`, indicating a validated deck was submitted; and
- `ready`.

Deck contents are private and never appear in another connection’s view. A seat’s `name`
is public and un-redacted; when it is absent a client falls back to a seat-derived label
(e.g. `"Player 2"`, using the real `seat` index — never by parsing the opaque id).

### `RoomSummary`

Each `directory` entry exposes only the information needed to browse rooms:

| Field | Type | Meaning |
| --- | --- | --- |
| `room_id` | `RoomId` | Opaque id accepted by `join_room` |
| `config` | `RoomConfig` | Seat count and game setup |
| `filled` | `number` | Occupied seat count |
| `state` | `RoomState` | `gathering` or `in_progress` |

The directory never exposes rosters, deck lists, or game state. A `gathering` room is joinable
while it has an open seat; an `in_progress` room is visible but not joinable. Empty and
finished rooms leave the directory. The server re-sends affected lobby views whenever the
directory changes. A missing `directory` field is treated as an empty list.

### `LobbyCommand`

Lobby commands are tagged by `type`:

| `type` | Fields | Purpose |
| --- | --- | --- |
| `hello` | optional `token` | Start a session or reclaim one |
| `create_room` | `config` | Create and occupy a room |
| `join_room` | `room_id` | Join a listed room or a room identified out of band |
| `submit_deck` | `cards` | Submit functional card identities |
| `ready` | `ready` | Set or clear readiness |
| `set_name` | `name` | Set or change this connection’s public display name |
| `leave` | none | Vacate the current room |

```json
{ "type": "hello", "token": "s:ab12" }
{ "type": "create_room", "config": { "seats": 2, "game_setup": "standard_2p" } }
{ "type": "join_room", "room_id": "r:7f3" }
{ "type": "submit_deck", "cards": ["forest", "verdant_scout"] }
{ "type": "ready", "ready": true }
{ "type": "set_name", "name": "Alice" }
{ "type": "leave" }
```

`set_name` sets the connection’s public display name (issue #294). The server validates it
authoritatively — it trims surrounding whitespace and rejects a name that is empty, longer
than 32 characters, or holds a control (non-printable) character; an invalid name is
rejected with the lobby’s non-fatal error pattern (the current `LobbyView` is re-sent
unchanged), exactly like an illegal deck. Names need not be unique — the seat’s `PlayerId`
remains the identity, so a collision is allowed rather than rejected. The name is bound to
the session, so it survives a per-tab reconnect, and it is projected into the lobby roster
(`SeatView.name`) and, once a game starts, into every `GameView.player_names`. `set_name`
is available throughout the pre-game phase (before joining a room and while seated, up to
game start).

Deck entries are stable `functional_id` strings, repeated once per physical card. The server
resolves every identity and applies the selected format’s deck policy. A player may ready only
after submitting a valid deck. The game begins when every required seat is occupied, decked,
and ready.

The directory provides room discovery, not matchmaking; the server never pairs players
automatically.

## Invariants

- The server is authoritative for rules, legality, redaction, timers, and results.
- A fresh `LobbyView` or `GameView` is sufficient to rebuild the corresponding UI.
- `valid_commands` and `valid_actions` are the only sources of interactivity.
- Clients display server-computed characteristics and never infer legal choices.
- Unknown fields are ignored, and omitted optional fields receive documented defaults.
