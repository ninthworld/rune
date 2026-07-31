# SAGE protocol

SAGE uses JSON over one WebSocket connection. Before a game starts, the connection
exchanges complete lobby views and lobby commands. Once the room constructs a game, the
same connection exchanges personalized game views and chosen actions.

The Rust types in `crates/sage-protocol/src/lib.rs` are the wire authority; this document and
the TypeScript mirror in `clients/web/src/protocol.ts` must change with them in the same PR.

Drift is caught rather than trusted: the fixtures under `crates/sage-protocol/fixtures/` are
pinned by tests on both sides. Rust deserializes and round-trips them, and the client asserts a
parsed fixture is identical to the fixture — so a field the server sends and the mirror omits
fails the client suite instead of being silently dropped.

## Message lifecycle

| Phase | Server to client | Client to server |
| --- | --- | --- |
| Lobby | `LobbyView` (and, on request, one `CatalogView`) | tagged `LobbyCommand` |
| Game | `GameView` | `{"type":"choose_action", ...}` or `{"type":"set_stops", ...}` |

The server sends a complete personalized view after every accepted state change and after
rejected or stale input. There is no patch or event-stream protocol. The client reconstructs
its current UI from the latest view.

Empty collections and optional values are generally omitted. Clients must normalize missing
fields to the defaults defined by the protocol types and tolerate unknown fields. A field
that classifies something (a `kind`, a reason, a phase) has no safe default: when it is
absent, or carries a value this client does not know, the thing is **unclassified** and is
rendered generically — a client never substitutes a guess.

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
| `stops` | `Phase[]` | Receiver’s own priority-stop preferences, applying on **any** turn; omitted when empty |
| `own_turn_stops` | `Phase[]` | The same preference for steps that stop **only while the receiver is the active player** (issue #455); omitted when empty |
| `auto_passed` | `boolean` | Whether reaching this state auto-passed the receiver; omitted when `false` |
| `auto_passed_steps` | `AutoPassedStep[]` | The ordered path of turn-and-step positions the settle acted at on the receiver’s behalf (issue #455); omitted when empty |
| `action_rejected` | `boolean` | Whether this view answers a rejected in-game action by the receiver; omitted when `false` |
| `action_ack` | `ActionAck?` | Acknowledgement of the receiver's last correlated submission (issue #554); rides that receiver's views from the one answering it until its next submission supersedes it, omitted for every other seat and by an older server |
| `player_names` | `{ [PlayerId]: string }` | Public display names by player id; omitted when empty |
| `commander_damage` | `CommanderDamage[]` | Public per-commander combat-damage tally (CR 903.10a, issue #371); omitted when empty |
| `commander_tax` | `CommanderTax[]` | Public per-commander tax owed on the next cast from the command zone (CR 903.8, issue #372); omitted when empty |
| `format` | `MatchFormat?` | The format this match is played under (issue #553); omitted by an older server, which a client MUST read as "unknown format, not Commander" |
| `commander_identity` | `CommanderIdentity[]` | Public per-seat commander name and colour identity (CR 903.3/903.4, issue #553); omitted when empty, and by an older server |

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

`format` is the match’s format signal (issue #553): `{ id, commander }`, where `id` is the
room’s free-form `game_setup` identifier (`"standard"`, `"commander"`, …) and `commander`
is the typed flag a client keys Commander-specific presentation off. It exists because
**no client can infer the format from zone contents**: a Commander game whose commanders
are all on the battlefield has an empty `command`, an elided all-zero `commander_tax`, and
an empty `commander_damage`, which is indistinguishable from a non-Commander game. `id` is
omitted when empty and `commander` when `false`; the whole object is omitted by a server
that has no registered format for the room, and by an older server — in every one of those
cases the documented default is **not a Commander game**, exactly the pre-#553 reading. It
is public information (a room’s format is advertised in the lobby), so spectators receive
the identical value.

`commander_identity` carries each seat’s commander name and colour identity (CR 903.3 /
903.4, issue #553): one `{ commander, name, color_identity }` entry per player that
designated a commander, keyed — like `commander_damage` and `commander_tax` — by the owning
player’s `PlayerId`. It is **stable for the whole game**: the entry does not change when
the commander is cast, dies, is exiled, or returns to the command zone, which is the point,
since the `command` pile (the only previous source of a commander’s name and colours)
disappears the instant the commander leaves it. `color_identity` is an array of the closed
colour letters `"W"`, `"U"`, `"B"`, `"R"`, `"G"` in that order; it is **omitted for a
colourless commander**, which is a real, empty identity rather than a missing value, and
`name` is omitted only for a card the server cannot resolve. Public information, computed
by the same server-side CR 903.4 routine that validated the deck, so what a client renders
can never disagree with what the format enforced. The list is omitted (defaults to `[]`) in
a non-commander game and by an older server.

Per-seat presentation state (issue #553) rides the seat records themselves rather than a
parallel list. `OpponentView` gains `connected` and `ai`; `SelfView` gains `eliminated`,
`connected`, and `ai`, so the **receiver’s own** elimination — losing while two or more
players remain, CR 800.4a — has an authoritative source while the game continues (`result`
arrives only at game over, and the bounded `log` window is not reconstructable, so neither
may stand in for it). `connected` is the **one flag on the wire whose omitted value is
`true`**: the server holds a disconnected seat open, so the flag rides the wire only as
`false` and a client must test `=== false` rather than falsiness — an older server that
never sends it means every seat is connected. `ai` carries the lobby’s `SeatView.ai` into
the match so the marker is not lost at the hand-off; it is public presentation information
and exposes nothing about the AI’s decisions or policy. Both default to
connected/human when omitted. `Permanent` likewise gains `is_commander`, the server-computed
marker that this object *is* somebody’s commander; it is omitted (defaults to `false`) for
every other permanent, and a client MUST NOT infer it — a commander on the battlefield is an
ordinary permanent, and “legendary creature” is neither necessary (a commander may be a
planeswalker) nor sufficient (most legends are not commanders). `result` remains the sole
authority for the game’s outcome; none of these fields lets a client conclude a loss.

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
`life_changed`, `damage_dealt`, `cards_drawn`, `cards_milled`, `permanent_died`, `step_changed`,
`player_eliminated`, `commander_returned_to_command_zone`, and `game_over`. Named
`LogEntity` references have an opaque `id`
and server-supplied
`name`; the id may be used for presentational highlighting only. The `name` on every
reference is fixed at the moment the event was recorded, so an entry naming a permanent
stays stable after that permanent leaves play (dies, is bounced) — the server does not
re-resolve names against the current board.

A `cards_drawn` event contains only player and count, never a hidden card identity. A
`cards_milled` event carries the same two fields for cards put from the top of a library
into its owner's graveyard (CR 701.13). It is deliberately *not* a `cards_drawn`: milling
never causes the empty-library loss, and its `count` is what actually moved, so a player
asked to mill past an empty library logs the smaller number.
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

`commander_returned_to_command_zone` (with the owning `player` id and the commander
`card` as a `LogEntity`) marks a commander its owner chose to move from a graveyard or
exile back to the command zone (CR 903.9a). The commander is designated openly and moves
between public zones, so the card is named like any other zone-movement event; declining
the return moves nothing and records no event.

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

`stops`, `own_turn_stops`, `auto_passed`, and `auto_passed_steps` carry basic priority
automation and its pacing contract (issues #264 and #455, ADR 0010). `stops` is the
receiver’s own set of steps at which they want to receive priority even when the engine
reports they have no meaningful action — the per-phase opt-in that keeps automation from
skipping past a step they care about. It is set with the `set_stops` message (below),
stored server-side, and reflected here so the stops UI is reconstructable from a single
view and survives reconnect; it is omitted when empty, and a client treats a missing field
as an empty set. `auto_passed` is a display-only flag set on the broadcast that follows a
settle in which the server acted on this receiver’s behalf, so a client can show a
transient “passed for you” indicator; it is advisory (the UI reconstructs without it) and
omitted when `false`. The decision of whether a player has “no meaningful action” is the
server’s alone — the client never computes it and never auto-passes on its own.

`own_turn_stops` is the **narrower half** of the same preference: those steps stop only
while the receiver is the active player. A step listed in `stops` stops on every turn and
wins outright, so a step never appears in both lists on the wire. Two lists rather than one
because a stop answers two different questions — “hand me priority here whoever’s turn it
is” (the escape hatch for acting at an opponent’s end step) and “hand me priority here
while the turn is mine”, which is what a main-phase stop has to mean. Stopping a player in
every opponent main phase would reintroduce the per-step click automation exists to remove,
for a window in which they have nothing to do they could not already do at instant speed.

**Human seats have default stops.** A seat the server considers human, in a room whose
default-stop policy is on (real games; off in headless and unit-test rooms), starts with
`own_turn_stops` seeded to its own precombat and postcombat main phases, so a turn never
fast-forwards past the point where its owner would act. AI seats are seeded with nothing,
so AI-only and mixed games keep their throughput. The seed is a *starting value*, not a
rule: the first `set_stops` a seat sends replaces the whole preference and the server never
re-seeds it, so a bare `{"type":"set_stops"}` means “stop nowhere” and the defaults stay
cleared across reconnects. Because the reflected lists are the **effective** ones, a client
renders exactly what the server honours, defaults included, from a single view.

`auto_passed_steps` says *where* a settle acted for this receiver, where `auto_passed` says
only *that* it did. A settle can advance a dozen steps between two broadcasts, so a client
that knows only the boolean cannot tell a player what they did not get to see. Each entry
is an `AutoPassedStep`:

| Field | Type | Notes |
| --- | --- | --- |
| `phase` | `Phase` | The step the server acted at |
| `turn` | `number` | The turn that step belonged to; present on every entry |

Entries are in the order the server acted, with consecutive entries for the same position
collapsed (several priority windows inside one step are one entry). It is a **path, not a
set**: a position genuinely reached twice appears twice, and a consumer must not
de-duplicate it — every occurrence is part of how far the game moved unasked.

**Each entry carries its own turn, and a client must not infer one.** The tempting reading
of a repeated phase — “the settle crossed into a new turn” — is wrong in two ordinary
cases: an extra combat phase (CR 506.1) revisits the combat steps inside one turn, and an
extra cleanup step (CR 514.3a) revisits cleanup. Only the server knows which happened, so
the server says. With `turn` present a presentation can group the path into per-turn runs,
keep every occurrence in order, and report a boundary where one actually fell. Note the
consequence for any per-step UI keyed to the *current* turn (the client's phase plaque, for
one): only entries whose `turn` matches `GameView.turn` may mark a step there, since a
cross-turn path also carries the previous turn's positions.

The **active player** is deliberately not carried: this field refines an indicator, it is
not a second game log. Whose turn it was lives in the `step_changed` entries of `log`.

The list names only positions where *this receiver* was acted for; a step where another
seat was passed is that seat's entry. `auto_passed` is exactly `auto_passed_steps` being
non-empty. Both are advisory, transient, and display-only, and both are omitted at their
empty defaults — the authoritative record of what happened during a settle remains `log`
(ADR 0007), which carries the events themselves so a resolved spell, a death, or a turn
change is recoverable even when the receiver never held priority over it.

A settle also resolves a forced combat declaration that has **no legal non-empty answer**
(issue #453): a seat with no eligible attacker is never handed a `declare_attackers` prompt
it could only answer with an empty selection, and likewise for `declare_blockers`. This is
the same server-side judgment, made by the same rules authority — the empty declaration is
submitted as an ordinary action, so nothing new appears on the wire and a client sees only
that the step passed. A declaration the seat *could* answer non-emptily is always prompted;
automation never resolves a real choice. A seat that has listed the step in its `stops`
receives the prompt regardless, as it does for an idle pass.

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
cache (ADR 0012) — a pure presentation enrichment; the wire contract is unchanged and a
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
  (e.g. an Aura, CR 303.4) is attached to another;
- optional `is_commander` (default `false`, issue #553), the server-computed marker that
  this object **is** somebody’s commander (CR 903.3) — matched on the card instance, so it
  survives every zone change and recast; a client must never infer it from a name, a zone,
  or a type line; and
- optional `counters`, each `{ "kind": string, "count": number }`.

These fields describe server-computed state. They do not authorize interaction.

A `StackItem` describes one object on the stack:

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | `EntityId` | Per-game id of this stack object |
| `controller` | `PlayerId` | Player who controls it (chooses targets and resolution) |
| `description` | `string` | Display text: a spell’s name, or an ability’s composed sentence |
| `source` | `EntityId?` | Source permanent for an ability; **omitted for a spell** |
| `kind` | `"spell" \| "ability" \| "activated" \| "triggered"?` | What this object is (issues #550, #579); **omitted by an older server**, and then unclassified — never guessed |
| `targets` | `StackTarget[]?` | Targets chosen for it, in the order its effects consume them; **omitted when empty** (and by an older server), meaning no targets |
| `card` | `CardView?` | The face to render: a spell’s card, or an ability’s source permanent; **omitted when there is no face** (and by an older server) |

A `StackTarget` is an internally tagged object — the target’s kind is **stated by the
server**, so a client never classifies a target by testing which collection its id appears
in (that classification is rules interpretation, ADR 0001):

| `kind` | Payload | Names |
| --- | --- | --- |
| `player` | `player: PlayerId` | A player — the same seat key `controller`, `seat_order`, and `player_names` use |
| `permanent` | `id: EntityId` | A permanent, as it appears in `battlefield[].id` |
| `card` | `id: EntityId` | A card in a public pile, as it appears in a `ZonePile` |
| `stack` | `id: EntityId` | Another object on the stack (what a counterspell names, CR 701.5) |

Three rules govern these fields:

- **`description` stays authoritative for text.** `targets` is additive structure for
  presentation geometry (which entry points at what), never a replacement a client
  reassembles prose from — and a client must never parse `description` to recover targets.
- **The list is as it currently stands.** Targets are locked in on announcement
  (CR 601.2c) and a target that has since become illegal stays named until the object
  resolves or fizzles (CR 608.2b), so a client reconnecting mid-resolution rebuilds
  exactly the relationships the game holds. What to draw for an endpoint that is no
  longer in the view is a rendering decision, not a protocol one.
- **`kind` is only as fine-grained as the server can prove, and the union widens
  additively.** Issue #579 landed the first widening: the engine now records how an
  ability got onto the stack, so a server states `activated` (CR 602.2 — a player chose
  it and paid its costs) or `triggered` (CR 603.3 — the game put it there) where it
  previously could only say `ability`. Two compatibility rules follow, and both are
  load-bearing:
  - **`ability` stays valid.** It is the coarse value — what a server predating #579
    sends, and what any server sends for an ability whose provenance it cannot prove. A
    client keeps accepting it and renders such an entry generically; it never means
    “neither activated nor triggered”.
  - **An unrecognized value leaves the entry unclassified**, never coerced into a known
    one — render it from `description`. A `copy` value arrives with a copy mechanic
    (gap G3). “Unclassified” here is *stronger* than an omitted `kind`: a client that
    reads a missing `kind` as “ability when `source` is present” — the only inference an
    older-server payload permits — must not apply that reading to a kind it merely
    failed to recognize. The server stated one; overruling it with a guess is the same
    rules interpretation the field exists to prevent.

  A client must never reconstruct activated-vs-triggered from `description` prose or from
  when the entry appeared: that is rules interpretation, which ADR 0001 puts on the
  server. There is deliberately **no** mode/X/additional-cost summary and **no zone
  target kind**: the engine has no modal spells, no `X` costs, and no zone targets, so
  carrying either would be a field no projection could ever fill.

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
  offer a lighter gesture — one-click tap-for-mana — for exactly these actions without ever
  classifying abilities itself. Omitted when `false`.
- `destinations` (optional, issue #554) lists the server-authoritative surfaces this
  action may be taken *to*, each `{ type, id, owner?, label? }` where `type` is
  `"zone"`, `"entity"`, or `"player"` (free form — clients ignore kinds they do not
  recognize) and `id` is a zone name, an entity id, or a player id. `owner` names whose
  copy of a per-player zone this is and is omitted for a shared zone. **A client derives
  its drop regions from this list alone and fails closed**: an action with no
  `destinations` has no drop target at all. Drag remains optional input — every action
  is also reachable by click, keyboard, and touch — so a client that ignores this field
  loses nothing.
- `token` binds the answer to the action’s exact current content. The client echoes it
  verbatim and never derives or parses it. `submission` (below) is deliberately *not*
  part of it: the token binds the action, the submission identifies the message.

`label` is **contextual**, and choosing it is the server’s job (issue #554). A
`pass_priority` is labelled `"Resolve"` when passing would resolve the top of the stack
and `"Pass"` otherwise — the same action either way, with the same id and the same
token, so only the presentation differs. The distinction is a rules judgment (CR 117.4
plus CR 800.4a: an eliminated seat neither receives nor passes priority, so a round of
passes is not a seat count), which is why the client renders the string verbatim rather
than deciding the word itself.

Current action categories include `pass_priority`, `play_land`, `cast_spell`,
`activate_ability`, `choose_targets`, `mulligan_decision`, `discard`, `declare_attackers`,
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
| `option` | `slot`, `prompt`, `options[{id,label,requires}]` | One option id |
| `select_from_zone` | `slot`, `prompt`, `zone`, `owner`, `count`, `candidates` | Exactly `count` candidate ids |
| `order` | `slot`, `prompt`, `items` | A permutation of all item ids |
| `number` | `slot`, `prompt`, `min`, `max` | The chosen number as a decimal string |

`option` is used for choices such as keep or mulligan. An option's `requires` (issue #451)
lists the action's other slots **that choice** owes an answer to, and is omitted when it owes
none: the `mulligan_decision` action carries the `decision` option slot plus, once the seat
has mulliganed, a `select_from_zone` `bottom` slot over its hand, and only the *keep* choice
requires `bottom` — taking another hand bottoms nothing. A client enables a choice once every
slot it requires holds exactly the advertised number of ids; the server enforces the same
coupling on resolution, so `requires` changes no legality, it only keeps a client from
offering an answer that must be rejected. `select_from_zone` supports choices such as
discarding or bottoming cards. `order` requests a permutation of its `items`; the
`order_combat_damage` action emits one `order` prompt per attacker blocked by two or more
creatures, so its controller chooses the combat-damage assignment order (CR 510.1, issue
#346) — lethal damage is then assigned to the blockers along the chosen order. An attacker
with 0–1 blockers produces no ordering prompt.

`number` (issue #554) requests a value in the inclusive range `min`..`max` — the value
of X, how many counters to remove, one share of a divided effect. It is answered with
the chosen number rendered as a **decimal string** in the slot’s `chosen` array (e.g.
`["3"]`), sharing `TargetChoice` with every other slot kind so one atomic
`choose_action` still answers a whole action and the content `token` still binds every
slot (the bounds are folded into it, so an answer bound to a range the server no longer
offers is rejected like any other stale binding). **The bounds are the server’s**,
computed from available mana, the source’s text, and the game state; the client offers a
control over exactly that range and computes no affordability of its own. Both `min` and
`max` are always present — a zero `min` is not elided — so the range reads completely
rather than by inference. A *divided* value is posed as one `number` slot per recipient,
each with its own bounds, and the server validates the total on resolution; the client
never enforces a sum.

`choose_targets` aims a **triggered ability already on the stack** (CR 603.3d). A trigger
is put there by the game rather than by a player, so it arrives unaimed and its controller
is asked to fill one target slot per targeting effect — the same per-slot `requirements`
a cast or an activation carries, bound by the same token. While one is owed the server
offers that seat nothing else (and no other seat anything at all): the ability goes on the
stack before any player receives priority (CR 603.3b), so play does not continue around it.
The seat asked is the trigger's *controller*, which is frequently not whoever last acted —
a creature killed by an opponent's removal spell gives its own controller the choice.
A trigger with no legal choice for a slot never reaches the stack at all (CR 603.3c), so a
`choose_targets` is always answerable.

Combat declarations also use requirements. The `attackers` slot lists creatures eligible to
attack; blocker slots list eligible blockers for each attacker. In a game with more than one
opponent (issue #345), `declare_attackers` additionally offers one **defender slot per
attacker candidate** — a slot whose candidates are the defending players that attacker may be
declared to attack (CR 508.1a); the client answers a defender for each attacker it declares,
and the slot is correlated to its attacker the same way blocker slots are. A two-player game
offers no defender slots (the sole opponent is the only defender), so the wire and the client
flow are unchanged. `declare_blockers` requirements are scoped to the player who currently
owes the declaration (issue #344): with attacks split across defenders, each attacked player
sees only the attackers attacking them.

A blocker slot carries its attacker's restrictions in two different places, according to what
kind of restriction it is (issue #606). A **pairwise** one — flying, "can't be blocked",
"can't be blocked by black creatures" — is a fact about one attacker/blocker pair, so it is
projected as the slot's `candidates`: only creatures that may legally block *that* attacker
are listed, and an attacker nothing may block gets no slot at all. A **whole-selection** one
constrains *how many* blockers may be assigned — menace's two-or-more (CR 702.110b) and the
"no more than one" ceiling (CR 509.1b) — and the engine can only reject it once the
declaration is assembled, so the slot's `prompt` states it in words rather than letting a
submit silently do nothing. Either way the server asks the engine and the client still
computes no legality: it renders the candidates and the prompt it was given. Empty selections are legal for these optional
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

A message may also carry an opaque, client-generated `submission` correlation id (issue
#554):

```json
{
  "type": "choose_action",
  "action_id": "a2",
  "token": "t00000000deadbeef",
  "submission": "s:17"
}
```

The server echoes it verbatim in `action_ack`, starting with the view that answers this
message — `{ "submission": "s:17", "accepted": true }` when the action was applied,
`accepted: false` when it was rejected (the same event `action_rejected` flags, now tied
to a specific submission). `accepted` is always present, so a client never reads an
absence as a verdict; the *ack itself* is the optional part, and its presence is the
signal that “this view answers my click”. Only that receiver's views ever carry it —
every other seat's, and every spectator's, carry none.

**The ack is matched, not counted.** It rides that receiver's views until its next
submission supersedes it (a submission with no id supersedes it too, to `null`), and it
is dropped when the seat reconnects. That is deliberate: a seat's view channel is
latest-value, so a view pushed while an earlier one is still in flight replaces it, and
an ack that answered exactly one view would be lost whenever an unrelated broadcast
overtook it. A client therefore compares `submission` against the id it is still waiting
on and ignores anything else — a repeat of an ack it has already consumed names nothing
and does nothing.

Correspondingly, **a view carrying no ack says nothing about a submission in flight.**
An ordinary broadcast — another seat acting — is ack-less, so a client must not read one
as an answer to its own click; that is the race the correlation exists to remove. The id
identifies the **message**, not the action: it is never part of the content `token`, and
resubmitting the same action with a new id is a new submission. It is optional — a client
that omits it sends exactly the message it always sent and receives no ack — and, like
`action_rejected`, purely advisory: the UI reconstructs fully without it, so a client
releases a pending indicator on a transport discontinuity rather than waiting forever for
an ack an older server will never send.

### `SetStops`

The second in-game client message sets the receiver’s priority-stop preferences (issue #264,
ADR 0010): the steps at which they want priority even when they have no meaningful action, so
basic auto-pass does not skip them there.

```json
{ "type": "set_stops", "stops": ["end"], "own_turn": ["precombat_main", "postcombat_main"] }
```

Both fields are lists of `Phase` values: `stops` for steps that stop on any turn, `own_turn`
for steps that stop only while the sender is the active player (issue #455). Each is omitted
from the wire when empty, so the minimal message is `{"type":"set_stops"}`.

The message replaces the seat’s **whole** preference — both lists at once, never a delta —
which is what lets a player clear the human default stops the server seeds: two empty lists
mean “stop nowhere”, not “leave my defaults alone”, and the server never re-seeds a seat
that has sent one. A step named on both lists keeps only the wider `stops` claim, and the
echo reflects that, so a client drawing one control per step is never told a step is two
things at once.

The server is authoritative: it stores the preference per seat — so it survives reconnect —
and reflects the accepted, *effective* lists back in `GameView.stops` and
`GameView.own_turn_stops`, which together are the sole source of the client’s toggle state
(nothing is stored client-side). An unparseable message is ignored and the current
`GameView` re-sent, the same non-fatal pattern the lobby uses. Automation itself (whether an
idle seat’s priority is auto-passed) is a server decision; the client only configures where
to stop and renders the `auto_passed`/`auto_passed_steps` indicators.

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

A connection that joined with `spectate_room` (issue #351) receives a
`SpectatorView` instead of a `GameView` on every change — a **non-seated observer** watching
the game live with all hidden information redacted. Redaction is **structural**: the type
simply has no receiver or decision fields, so a projection cannot leak a hand, a library’s
contents, a mana pool, or a `valid_actions` list to a spectator. It reuses `GameView`’s public
component types verbatim (`OpponentView`, `Permanent`, `StackItem`, `ZonePile`, `GameLogEntry`,
`Phase`, `PlayerId`, `GameResult`, `CommanderDamage`, `MatchFormat`, `CommanderIdentity`).

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
| `format` | `MatchFormat?` | The match format signal (issue #553); omitted by an older server (read as "not Commander") |
| `commander_identity` | `CommanderIdentity[]` | Public per-seat commander name and colour identity (issue #553); omitted when empty |

A `SpectatorView` carries **no** `you`, `me`, `my_hand`, `mana_pool`, `valid_actions`,
`action_deadline`, `stops`, `own_turn_stops`, `auto_passed`, `auto_passed_steps`, or
`action_rejected` — those fields do not exist on
the type. The issue #553 presentation metadata it does carry is public by construction: the
format is advertised in the lobby, a commander is announced before the game, and a seat’s
connection/AI state is what every seated player already sees, so a spectator’s `players[]`
entries carry the same `connected`/`ai` flags a seated `OpponentView` does. A spectator reconstructs the whole public board from a single `SpectatorView` (the
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
config contains `seats`, an opaque `game_setup` id, an optional table `name`, and a
`visibility` (issue #546). The lobby validates a 2–8 seat range,
requires the setup id to exist in the server format registry, and rejects a seat count
outside the chosen format's own range (issue #349). Two-player formats and 3–4 seat
free-for-all formats both start real games.

| `RoomConfig` field | Type | Meaning |
| --- | --- | --- |
| `seats` | `number` | Seat count, validated into `2..=8` and against the format's own range |
| `game_setup` | `GameSetupId` | Opaque id naming the format the room builds its game from |
| `name` | `string?` | The host's chosen table name (issue #546); omitted when unnamed |
| `visibility` | `RoomVisibility?` | `public` (default, omitted) or `private` (issue #546) |

`name` is public, display-only text validated exactly like a `set_name` display name —
trimmed, non-empty, at most 32 characters, printable — and a blank name normalizes to
*absent* rather than being stored. The server never invents one: when `name` is omitted a
client labels the table by its `game_setup`, which is what every client did before the
field existed. `visibility` is `public` or `private`; both `name` and `visibility` are
omitted from the wire at their defaults, so a client that sends neither creates exactly
the room the pre-#546 shape created, and an older client ignores both.

A `private` room is **omitted from the public directory for every connection** — that is
the whole of what the field does, and it is a server behaviour rather than a label. It
stays reachable by the `room_id` its host shares out of band, so `join_room` works on it
exactly as before; its own occupants still see it in their `RoomView`.

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
| `config` | `RoomConfig` | The room's whole config — seat count, game setup, and (issue #546) table name |
| `filled` | `number` | Occupied seat count |
| `spectators` | `number` | How many observers are watching (issue #351); omitted when `0` |
| `state` | `RoomState` | `gathering` or `in_progress` |

A directory entry carries the room's whole `RoomConfig`, so a table's `name` reaches the
browser through the field it already had rather than through a second, divergent copy; a
listed room is public by definition, so its `visibility` is always the elided default.
The directory never exposes rosters, deck lists, or game state. A `gathering` room is joinable
while it has an open seat. An `in_progress` room is not seat-joinable, but it **can be
spectated** (`spectate_room`, issue #351): observers do not consume seats, so
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
| `update_room` | `config` | Host-only: change the room's whole configuration (issue #546) |
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
{ "type": "update_room", "config": { "seats": 4, "game_setup": "commander", "name": "Casual Commander", "visibility": "private" } }
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

`update_room` lets the room **host** change its table's configuration after the room
exists (issue #546). It carries a **whole** `RoomConfig`, not a patch of changed fields —
the same full-state discipline `LobbyView` follows, and the reason one client surface can
serve both creating and editing a table. The server:

- accepts it only from the seat 0 occupant (`add_ai`'s host rule), and advertises
  `update_room` in that connection's `valid_commands` while the room is pre-game — the
  client renders Edit Table from that advertisement, never from its own idea of who the
  host is;
- rejects it once the game has started;
- validates the new config with exactly the rules a `create_room` gets (seat range,
  known `game_setup`, the format's own seat range, table-name bounds) — a table you could
  not have created is a table you cannot edit into;
- **rejects, never clamps, a seat count that would remove an occupied seat.** Growing a
  table appends empty, joinable seats; shrinking is allowed only onto seats that hold
  neither a player nor an AI, at any index. Nobody is evicted by a configuration change.

Readiness follows the change. Changing the **seat count** or the **format** clears every
seat's `ready` flag, because nobody stays ready to a table they did not agree to; changing
the **format** additionally clears every submitted deck (and empties any AI seat), because
each deck was validated against a format that no longer applies and must be resubmitted.
A name- or visibility-only edit disturbs nothing. An accepted update can therefore only
ever *clear* gate state, so it never completes the ready gate and never starts a game.

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

`spectate_room` joins a room as a **spectator** (issue #351): a non-seated observer
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

### `LobbyErrorFrame` (deck-rejection reason)

When a `submit_deck` (or a host’s `add_ai`) deck is rejected, the server sends the **rejecting
connection only** a structured, human-readable reason (issue #395) in addition to re-sending its
unchanged `LobbyView` (the non-fatal pattern above). Other seats and spectators receive nothing
about the rejected deck — the reason rides the sender’s own socket, and any named card is always
one of the sender’s own submitted cards, never another seat’s hidden deck.

The frame is a single object under a `lobby_error` key — the on-wire discriminator, carried by no
other frame (`LobbyView`, `GameView`, `SpectatorView`, `CatalogView`):

```json
{ "lobby_error": { "code": "copy_limit", "reason": "Onakke Ogre appears 5 times, above the 4-copy limit", "card": "onakke_ogre" } }
```

`LobbyRejection` fields:

| Field | Type | Meaning |
| --- | --- | --- |
| `code` | `string` | Stable `snake_case` rejection class (see below); free-form so a newer class never breaks an older client |
| `reason` | `string` | Human-readable explanation, safe to display verbatim; the server derives it from structured deck-legality data and composes no other prose |
| `card` | `CardIdentity?` | The offending card’s `functional_id`, present only when one specific card is at fault; omitted otherwise |

`code` is one of `below_minimum`, `above_maximum`, `copy_limit`, `missing_commander`,
`commander_not_in_deck`, `commander_not_legendary_creature`, `out_of_identity` (the deck-legality
classes), or `unknown_card` (a decklist identity that does not resolve). `card` is present for
`copy_limit`, `out_of_identity`, `commander_not_in_deck`, `commander_not_legendary_creature`, and
`unknown_card`; the size and missing-commander classes name no card.

The same frame also carries the **table-configuration** rejection classes (issue #546) —
`invalid_seat_count`, `seat_count_for_format`, `unknown_format`, `seats_below_occupancy`,
`invalid_room_name`, and `not_host` — so a refused `create_room`/`update_room` explains
itself instead of leaving a host watching a control do nothing. These name no card, and
each reports only what the sender itself sent (a seat count, a format id, its own table
name, its own room's occupancy), so nothing leaks. The client shows `reason`
and keeps its builder state so the list can be corrected and resubmitted in the same room session;
an older client that does not recognize the frame simply ignores it and keeps its `LobbyView`, so
the feedback is additive. The client computes no legality of its own — this reason is the server’s
authoritative explanation, not a client-side pre-validation.

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
- Drag and drop is optional input. Drop regions come only from `destinations`, and an
  action naming none has no drop target; every action stays reachable by click,
  keyboard, and touch.
- A default is what the protocol documents, not what the type’s zero value happens to be:
  an omitted `connected` means **connected**, an omitted `format` means **not Commander**,
  an omitted `commander_identity`/`is_commander` means no commander presentation at all.
- Relationships between objects are **stated by the server, typed at the source**, and
  never reconstructed by a client — from prose, from id membership in a collection, or
  from anything else. `Permanent.blocking`/`attacking_player`/`attached_to` and
  `StackItem.targets` are the whole set; each names its subject explicitly, and an
  omitted one means "no such relationship", never "work it out".
