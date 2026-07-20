# RUNE protocol

RUNE uses JSON over one WebSocket connection. Before a game starts, the connection
exchanges complete lobby views and lobby commands. Once the room constructs a game, the
same connection exchanges personalized game views and chosen actions.

The Rust types in `crates/rune-protocol/src/lib.rs` are the wire authority. The TypeScript
mirror in `clients/web/src/protocol.ts` and this document must change with them.

## Message lifecycle

| Phase | Server to client | Client to server |
| --- | --- | --- |
| Lobby | `LobbyView` (and, on request, one `CatalogView`) | tagged `LobbyCommand` |
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
| `command` | `ZonePile[]` | Public ordered command zones (CR 903.6, issue #372); omitted when empty |
| `phase` | `Phase` | Current turn step |
| `turn` | `number` | One-based turn number; `0` only for an empty state |
| `active_player` | `PlayerId` | Player whose turn it is |
| `seat_order` | `PlayerId[]` | Every seat's id in seat order, including the receiver and any eliminated players (issue #345). The explicit ordering a multiplayer client uses to arrange opponents; omitted (defaults to `[]`) by an older server |
| `mana_pool` | `string[]` | Receiver’s unspent mana as pip strings |
| `priority_player` | `PlayerId?` | Player currently holding priority |
| `valid_actions` | `ValidAction[]` | Only actions available to the receiver |
| `action_deadline` | `number?` | Seconds remaining for the receiver’s current decision |
| `result` | `GameResult?` | Terminal result; absent during a live game |
| `log` | `GameLogEntry[]` | Bounded, sequence-numbered recent public game history |
| `stops` | `Phase[]` | Receiver’s own priority-stop preferences; omitted when empty |
| `auto_passed` | `boolean` | Whether reaching this state auto-passed the receiver; omitted when `false` |
| `action_rejected` | `boolean` | Whether this view answers a rejected in-game action by the receiver; omitted when `false` |
| `player_names` | `{ [PlayerId]: string }` | Public display names by player id; omitted when empty |
| `commander_damage` | `CommanderDamage[]` | Public per-commander combat-damage tally (CR 903.10a, issue #371); omitted when empty |
| `commander_tax` | `CommanderTax[]` | Public per-commander tax owed on the next cast from the command zone (CR 903.8, issue #372); omitted when empty |

`command` is each player's command zone (CR 903.6), carried in the same public `ZonePile`
shape as `graveyards`/`exile` (`{ player_id, cards }`), one entry per player with a card
there. Public information; omitted (defaults to `[]`) in a non-commander game.

`commander_damage` is the cumulative **combat** damage each commander has dealt each
player this game (CR 903.10a). Each entry is `{ commander, damaged, amount }`, where
`commander` and `damaged` are `PlayerId`s — a commander is named by its owning player’s
id, since one player designates at most one commander today, and that key is stable
across the commander’s zone changes. Public information (identical for every receiver
and for spectators); a player who has taken 21 or more from a single commander has lost,
which surfaces in `result.reason` as `commander_damage`. The list is omitted (defaults to
`[]`) in a non-commander game, so an older client is unaffected.

`commander_tax` is the commander tax each designation owes (CR 903.8): each entry is
`{ commander, casts, tax }`, where `commander` is the owning player's `PlayerId`, `casts`
is how many times that commander has been cast from the command zone this game, and `tax`
is the generic mana the tax adds to the next such cast (`2 * casts`). `casts` and `tax`
are omitted when zero. Public information; the list is omitted (defaults to `[]`) in a
non-commander game.

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
`life_changed`, `damage_dealt`, `cards_drawn`, `permanent_died`, `step_changed`,
`player_eliminated`, and `game_over`. Named `LogEntity` references have an opaque `id`
and server-supplied
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

`player_eliminated` (with a `player` id and a `reason`, the same `GameOverReason` enum
`game_over` carries) marks a player *leaving the game* mid-game under CR 800.4a — they
lost while two or more players remained, so play continues without them and their
objects are removed. It is distinct from `game_over`, which fires only once one player
is left: a two-player loss produces `game_over` alone, never `player_eliminated`.

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

`action_rejected` is the in-game counterpart of the lobby’s non-fatal error pattern (issue
#265). A rejected `choose_action` is answered by re-sending the receiver’s current, unchanged
`GameView` (below); that one re-send carries `action_rejected: true` so the client can show a
brief, non-blaming “the game moved on” notice. Because a `valid_actions`-driven client only
ever offers actions the server issued, a rejection means a stale-view race (the offered
action was superseded before it arrived), not a user error — the tone is informational, not
blaming. Like `auto_passed`, it is advisory and transient: `valid_actions` already reflects
the true current legal set, the UI reconstructs fully without it, and it is omitted when
`false` (so every normal broadcast and every resync clears it). A client renders it as
ephemeral presentation only (an auto-dismissing toast) — never load-bearing state.

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
opaque strings. The web client uses `functional_id` as the key of its client-local card-art
cache (ADR 0024) — a pure presentation enrichment; the wire contract is unchanged and a
client that ignores the field renders completely without it.

`OpponentView` contains `player_id`, `hand_size`, `life`, `library_size`,
`graveyard_size`, optional display-only `statuses`, and an optional `eliminated` boolean —
`true` when the opponent has left the game (CR 800.4a, issue #342/#345), omitted (and
defaulting to `false`) in a two-player game. `ZonePile` contains a `player_id` and ordered
`cards`; the top of the zone is last.

### Permanents and stack objects

A `Permanent` contains:

- `id`, `controller`, `owner`, and a computed `card`;
- optional `tapped` and `attacking` booleans;
- optional `attacking_player`, naming the defending player's entity id this attacker
  attacks (CR 508.1a, issue #341/#345) — the multiplayer generalization of `attacking`,
  omitted when not attacking; a two-player client may ignore it (the sole opponent is the
  only defender);
- optional `blocking`, naming the attacker’s entity id;
- optional marked `damage`;
- optional `attached_to`, naming the host permanent’s entity id when this permanent
  (e.g. an Aura, CR 303.4) is attached to another; and
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
  "mana_ability": true,
  "token": "t00000000deadbeef"
}
```

- `id` is the opaque handle returned in `ChooseAction`.
- `type` is a free-form category used for presentation and input routing.
- `label` is server-supplied display text.
- `subject` names the entities that own the action. An empty subject identifies a global
  action such as passing priority.
- `mana_ability` (optional, default `false`) marks the activation of a mana ability
  (CR 605): no targets, no stack, only mana production. Server-computed so a client may
  offer a lighter gesture — one-click tap-for-mana (ADR 0025) — for exactly these actions
  without ever classifying abilities itself. Omitted when `false`.
- `token` binds the answer to the action’s exact current content. The client echoes it
  verbatim and never derives or parses it.

Current action categories include `pass_priority`, `play_land`, `cast_spell`,
`activate_ability`, `mulligan_decision`, `discard`, `declare_attackers`,
`declare_blockers`, `order_combat_damage`, and `concede`. Clients must tolerate unknown
categories.

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
such as discarding or bottoming cards. `order` requests a permutation of its `items`; the
`order_combat_damage` action emits one `order` prompt per attacker blocked by two or more
creatures, so its controller chooses the combat-damage assignment order (CR 510.1, issue
#346) — lethal damage is then assigned to the blockers along the chosen order. An attacker
with 0–1 blockers produces no ordering prompt.

Combat declarations also use requirements. The `attackers` slot lists creatures eligible to
attack; blocker slots list eligible blockers for each attacker. In a game with more than one
opponent (issue #345), `declare_attackers` additionally offers one **defender slot per
attacker candidate** — a slot whose candidates are the defending players that attacker may be
declared to attack (CR 508.1a); the client answers a defender for each attacker it declares,
and the slot is correlated to its attacker the same way blocker slots are. A two-player game
offers no defender slots (the sole opponent is the only defender), so the wire and the client
flow are unchanged. `declare_blockers` requirements are scoped to the player who currently
owes the declaration (issue #344): with attacks split across defenders, each attacked player
sees only the attackers attacking them. Empty selections are legal for these optional
declarations. The server validates cardinality and action-specific rules.

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
`GameView`, and that re-send sets `action_rejected: true` (above) so the receiver gets a
brief, non-blaming notice rather than a silently unchanged screen.

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

`winner` is absent for a draw. `reason` is one of `life_zero`, `decked`, `concede`, or
`commander_damage` (a player took 21+ combat damage from a single commander, CR 903.10a).
Further submitted actions are rejected and the final view is re-sent.

### `SpectatorView`

A connection that joined with `spectate_room` (ADR 0022, issue #351) receives a
`SpectatorView` instead of a `GameView` on every change — a **non-seated observer** watching
the game live with all hidden information redacted. Redaction is **structural**: the type
simply has no receiver or decision fields, so a projection cannot leak a hand, a library’s
contents, a mana pool, or a `valid_actions` list to a spectator. It reuses `GameView`’s public
component types verbatim (`OpponentView`, `Permanent`, `StackItem`, `ZonePile`, `GameLogEntry`,
`Phase`, `PlayerId`, `GameResult`, `CommanderDamage`).

| Field | Type | Meaning |
| --- | --- | --- |
| `players` | `OpponentView[]` | **Every** seat as public state and hidden-zone counts — no privileged “self” |
| `battlefield` | `Permanent[]` | Public permanents and computed state |
| `stack` | `StackItem[]` | Stack objects, bottom first |
| `graveyards` | `ZonePile[]` | Public ordered graveyards |
| `exile` | `ZonePile[]` | Public ordered exile zones |
| `command` | `ZonePile[]` | Public ordered command zones (CR 903.6, issue #372); omitted when empty |
| `phase` | `Phase` | Current turn step |
| `turn` | `number` | One-based turn number |
| `active_player` | `PlayerId` | Player whose turn it is |
| `seat_order` | `PlayerId[]` | Every seat’s id in seat order, including eliminated players |
| `priority_player` | `PlayerId?` | Player currently holding priority (whose turn it is to act — never the actions themselves) |
| `result` | `GameResult?` | Terminal result; absent during a live game |
| `log` | `GameLogEntry[]` | Bounded, sequence-numbered recent **public** game history |
| `player_names` | `{ [PlayerId]: string }` | Public display names by player id; omitted when empty |
| `commander_damage` | `CommanderDamage[]` | Public per-commander combat-damage tally (CR 903.10a, issue #371); omitted when empty |
| `commander_tax` | `CommanderTax[]` | Public per-commander tax owed (CR 903.8, issue #372); omitted when empty |

A `SpectatorView` carries **no** `you`, `me`, `my_hand`, `mana_pool`, `valid_actions`,
`action_deadline`, `stops`, `auto_passed`, or `action_rejected` — those fields do not exist on
the type. A spectator reconstructs the whole public board from a single `SpectatorView` (the
complete-view principle), so it may join mid-game and resume after a reconnect with no history.
The client distinguishes a `SpectatorView` from a seated `GameView` structurally: a
`SpectatorView` has no `you` field, whereas a `GameView` always serializes one.

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
config contains `seats` and an opaque `game_setup` id. The lobby validates a 2–8 seat range,
requires the setup id to exist in the server format registry, and rejects a seat count
outside the chosen format's own range (issue #349). Two-player formats and 3–4 seat
free-for-all formats both start real games.

Each seat contains:

- zero-based `seat` index;
- optional public `occupied_by` player id;
- optional public `name`, the occupant’s chosen display name (issue #294), omitted for
  an empty or unnamed seat;
- `decked`, indicating a validated deck was submitted;
- `ready`; and
- optional `ai`, the id of the **AI opponent** kind filling the seat (issue #415), omitted
  for an empty or human seat.

Deck contents are private and never appear in another connection’s view. A seat’s `name`
is public and un-redacted; when it is absent a client falls back to a seat-derived label
(e.g. `"Player 2"`, using the real `seat` index — never by parsing the opaque id).

A seat filled by an AI opponent (issue #415) carries `ai` set to the AI kind’s id (e.g.
`"random"`), no `occupied_by` (it is not a session), and `decked`/`ready` both `true` — its
deck was chosen by the host when it was seated and it is ready by construction. `ai` is a
free-form string like the other lobby id fields, so a newer AI kind never breaks an older
client; the kind’s display label comes from the `CatalogView`’s `ai_opponents` list.

### `RoomSummary`

Each `directory` entry exposes only the information needed to browse rooms:

| Field | Type | Meaning |
| --- | --- | --- |
| `room_id` | `RoomId` | Opaque id accepted by `join_room` |
| `config` | `RoomConfig` | Seat count and game setup |
| `filled` | `number` | Occupied seat count |
| `spectators` | `number` | How many observers are watching (issue #351); omitted when `0` |
| `state` | `RoomState` | `gathering` or `in_progress` |

The directory never exposes rosters, deck lists, or game state. A `gathering` room is joinable
while it has an open seat. An `in_progress` room is not seat-joinable, but it **can be
spectated** (`spectate_room`, ADR 0022 / issue #351): observers do not consume seats, so
`spectators` is independent of `filled`, and only a count is advertised — never a spectator’s
identity. Empty and finished rooms leave the directory. The server re-sends affected lobby
views whenever the directory changes (including a spectator count change). A missing
`directory` field is treated as an empty list; a missing `spectators` field as `0`.

### `LobbyCommand`

Lobby commands are tagged by `type`:

| `type` | Fields | Purpose |
| --- | --- | --- |
| `hello` | optional `token` | Start a session or reclaim one |
| `create_room` | `config` | Create and occupy a room |
| `join_room` | `room_id` | Join a listed room or a room identified out of band |
| `spectate_room` | `room_id` | Watch an in-progress room as an observer (issue #351) |
| `submit_deck` | `cards`, optional `commander` | Submit functional card identities, and (commander format) the designated commander |
| `add_ai` | `seat`, `kind`, `cards`, optional `commander` | Host-only: fill an empty seat with an AI opponent (issue #415) |
| `remove_ai` | `seat` | Host-only: empty an AI seat again (issue #415) |
| `ready` | `ready` | Set or clear readiness |
| `set_name` | `name` | Set or change this connection’s public display name |
| `request_catalog` | none | Request the public card catalog and format deck rules (issue #367) |
| `leave` | none | Vacate the current room, or stop spectating |

```json
{ "type": "hello", "token": "s:ab12" }
{ "type": "create_room", "config": { "seats": 2, "game_setup": "standard_2p" } }
{ "type": "join_room", "room_id": "r:7f3" }
{ "type": "spectate_room", "room_id": "r:7f3" }
{ "type": "submit_deck", "cards": ["forest", "verdant_scout"] }
{ "type": "submit_deck", "cards": ["jedit_ojanen", "forest"], "commander": "jedit_ojanen" }
{ "type": "add_ai", "seat": 1, "kind": "random", "cards": ["forest", "verdant_scout"] }
{ "type": "remove_ai", "seat": 1 }
{ "type": "ready", "ready": true }
{ "type": "set_name", "name": "Alice" }
{ "type": "request_catalog" }
{ "type": "leave" }
```

`submit_deck`’s optional `commander` names the card the seat designates as its commander
(CR 903.3, issue #372), by the same `CardIdentity` (`functional_id`) its decklist uses. It is
present only for a commander-format deck and omitted otherwise, so the frame stays byte-for-byte
the pre-commander shape for every other format. The server validates the designation
authoritatively against the room’s format — it must be one of the deck’s cards and, for the
commander format, a **legendary creature** whose color identity (and every deck card’s) fits the
rules (see `CatalogFormat` and the deck-legality notes below); an illegal deck or designation is
rejected with the lobby’s non-fatal error and the seat keeps whatever deck it had. Deck legality
is server policy — the client never computes it.

`add_ai` and `remove_ai` let the room **host** seat and clear **AI opponents** (issue #415, ADR
0028). They are host-only: the server accepts them only from the seat 0 occupant, and advertises
them in that connection’s `valid_commands` only when they are legal (`add_ai` while a seat is open,
`remove_ai` while an AI seat exists) — the client renders the affordance from `valid_commands`, never
from a client-side notion of “host”. `add_ai` names the target `seat`, the AI `kind` (one of the
`CatalogView.ai_opponents` ids), and the deck the AI plays — the same flat `cards` list (and optional
`commander`) a `submit_deck` carries, validated authoritatively against the room’s format. On success
the seat shows as AI-occupied (`SeatView.ai`) and already decked + ready, and counts as filled for
the ready gate; the AI plays its own seat once the game starts. `remove_ai` empties an AI seat again.
Both are pre-game only and rejected once the game has started. This works for any seat count — a room
may mix human and AI seats, e.g. one human against three AI in a free-for-all.

`spectate_room` joins a room as a **spectator** (ADR 0022, issue #351): a non-seated observer
that watches the game live with all hidden information redacted. Unlike `join_room` it does not
consume a seat, so it succeeds on a room whose seats are full — but the room’s game must already
be running (spectating a `gathering` room is rejected with the lobby’s non-fatal error, since
there is no board to watch yet). On success the connection stops receiving `LobbyView`s and
begins receiving `SpectatorView`s (below); it sends nothing back. `leave` ends the spectator
session. Spectators are advertised to the directory as `RoomSummary.spectators` (a count only).

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

### `CatalogView`

`request_catalog` asks for the public card catalog and per-format deck rules (issue #367), so a
connection can browse the supported card pool and format rules before joining or starting a
game. The server answers with **one** `CatalogView` frame and changes no lobby state; a
`request_catalog` never affects a room, seat, or deck. The catalog is reference data, not
per-connection state, so it is **not** carried on the pushed `LobbyView` — a client requests it
when it needs it (e.g. to build a deck) and re-requests it after a reconnect if wanted.

`CatalogView` is a versioned single-frame projection. It is distinguished from a `LobbyView` by
its `catalog_version` field (a `LobbyView` carries none) and from a `GameView`/`SpectatorView`
by carrying no `phase`.

| Field | Type | Meaning |
| --- | --- | --- |
| `catalog_version` | `number` | Projection schema version (currently `1`); also the wire discriminator |
| `cards` | `CatalogCard[]` | Every supported card, in a stable order |
| `formats` | `CatalogFormat[]` | Every advertised format’s deck rules and seat range |
| `ai_opponents` | `AiOption[]` | Every AI opponent kind a host may seat (issue #415); omitted/empty when none |

Each `AiOption` describes a seatable **AI opponent** kind (issue #415) — the `kind` an `add_ai`
carries and a `SeatView.ai` reports — so a client learns the available kinds from server metadata
rather than hardcoding them:

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | `string` | Stable kind id (e.g. `"random"`) — the value `add_ai.kind` / `SeatView.ai` use |
| `name` | `string` | Short human-readable name (e.g. `"Random"`) |
| `description` | `string?` | One-line description of how the kind plays; omitted when empty |

Each `CatalogCard` carries a card’s public characteristics — the browse-time counterpart of the
in-game `CardView`, named by identity rather than a per-game entity id:

| Field | Type | Meaning |
| --- | --- | --- |
| `functional_id` | `CardIdentity` | Stable identity — the same handle a `submit_deck` decklist uses |
| `name` | `string` | Display name |
| `type_line` | `string` | Full type line, including any basic supertype (e.g. `"Basic Land — Forest"`) |
| `mana_cost` | `string?` | Mana cost string; omitted for a card without one |
| `rules_text` | `string?` | Server-generated rules text, identical to the in-game `CardView`; omitted when empty |
| `power` | `string?` | Power (creatures only) |
| `toughness` | `string?` | Toughness (creatures only) |
| `keywords` | `string[]?` | Keyword abilities as lowercase wire names; omitted when empty |

Each `CatalogFormat` exposes exactly the server-side deck-legality policy a `submit_deck` is
validated against, so a client can build a legal deck ahead of time:

| Field | Type | Meaning |
| --- | --- | --- |
| `game_setup` | `GameSetupId` | The id naming this format — the same id a `RoomConfig` carries |
| `min_deck_size` | `number` | Fewest cards a legal deck may hold; `0` for no minimum |
| `max_deck_size` | `number?` | Most cards a legal deck may hold; omitted for no upper bound |
| `max_copies` | `number?` | Most copies of any single non-exempt card; **omitted for no copy limit** |
| `basic_land_exempt` | `boolean` | Whether basic lands are exempt from `max_copies` (CR 100.2a) |
| `requires_commander` | `boolean` | Whether a legal deck must designate a commander (CR 903.3); **omitted, default `false`** (issue #394) |
| `enforce_color_identity` | `boolean` | Whether every card’s color identity must fit the commander’s (CR 903.4); **omitted, default `false`** (issue #394) |
| `min_seats` | `number` | Fewest seats a room using this format may have |
| `max_seats` | `number` | Most seats a room using this format may have |

The projection is derived server-side from the one embedded card database and the format
registry — there is no bundled catalog copy — and each card’s `rules_text` is generated by the
same generator an in-game `CardView` uses, so the two can never disagree. A **permissive**
format advertises its permissiveness honestly: an unbounded deck size or copy limit is an
**omitted** field, never a sentinel number. `requires_commander` and `enforce_color_identity` are
projected from the server’s `DeckRules` (the single source of truth) so a client learns a
format’s commander requirement from advertised metadata instead of hardcoding the format name
(issue #394); both are additive and default-elided, so an existing frame stays valid. The catalog
is public data only — it never carries a deck, a roster, or any game state.

## Invariants

- The server is authoritative for rules, legality, redaction, timers, and results.
- A fresh `LobbyView` or `GameView` is sufficient to rebuild the corresponding UI.
- `valid_commands` and `valid_actions` are the only sources of interactivity.
- Clients display server-computed characteristics and never infer legal choices.
- Unknown fields are ignored, and omitted optional fields receive documented defaults.
