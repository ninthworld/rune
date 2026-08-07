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
| Game | `GameView` | `{"type":"choose_action", ...}`, `{"type":"set_stops", ...}`, or `{"type":"undo"}` |

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
| `revealed` | `CardView[]` | Cards from a **hidden zone this receiver alone is currently being shown** (issue #604) — the candidates of a mid-resolution choice they are answering. Omitted when empty, which is every view outside that window; never present on another seat’s view or on a `SpectatorView` |
| `me` | `SelfView` | Receiver’s `life`, `library_size`, and `maximum_hand_size` |
| `opponents` | `OpponentView[]` | Public opponent state and hidden-zone counts |
| `battlefield` | `Permanent[]` | Public permanents and computed state |
| `emblems` | `Emblem[]` | The emblems in the game (CR 114, issue #620); omitted when empty |
| `stack` | `StackItem[]` | Stack objects, bottom first |
| `graveyards` | `ZonePile[]` | Public ordered graveyards |
| `exile` | `ZonePile[]` | Public ordered exile zones |
| `command` | `ZonePile[]` | Public ordered command zones (CR 903.6, issue #372); omitted when empty |
| `phase` | `Phase` | Current turn step |
| `turn` | `number` | One-based turn number; `0` only for an empty state |
| `active_player` | `PlayerId` | Player whose turn it is |
| `seat_order` | `PlayerId[]` | Every seat's id in seat order, including the receiver and any eliminated players (issue #345). The explicit ordering a multiplayer client uses to arrange opponents; omitted (defaults to `[]`) by an older server |
| `mana_pool` | `string[]` | Receiver’s unspent mana as pip strings; a pip suffixed `*` is **restricted** mana (CR 106.6, issue #620) that may be spent only on what made it |
| `priority_player` | `PlayerId?` | Player currently holding priority |
| `valid_actions` | `ValidAction[]` | Only actions available to the receiver |
| `action_deadline` | `number?` | Seconds remaining for the receiver’s current decision |
| `result` | `GameResult?` | Terminal result; absent during a live game |
| `log` | `GameLogEntry[]` | Bounded, sequence-numbered recent public game history |
| `stops` | `Phase[]` | Receiver’s own priority-stop preferences, applying on **any** turn; omitted when empty |
| `own_turn_stops` | `Phase[]` | The same preference for steps that stop **only while the receiver is the active player** (issue #455); omitted when empty |
| `auto_passed` | `boolean` | Whether reaching this state auto-passed the receiver; omitted when `false` |
| `auto_passed_steps` | `AutoPassedStep[]` | The ordered path of turn-and-step positions the settle acted at on the receiver’s behalf (issue #455); omitted when empty |
| `auto_passed_from` | `number?` | The `log` sequence the receiver’s unattended stretch began at (issue #644); present exactly when `auto_passed_steps` is non-empty |
| `action_rejected` | `boolean` | Whether this view answers a rejected in-game action by the receiver; omitted when `false` |
| `action_ack` | `ActionAck?` | Acknowledgement of the receiver's last correlated submission (issue #554); rides that receiver's views from the one answering it until its next submission supersedes it, omitted for every other seat and by an older server |
| `player_names` | `{ [PlayerId]: string }` | Public display names by player id; omitted when empty |
| `commander_damage` | `CommanderDamage[]` | Public per-commander combat-damage tally (CR 903.10a, issue #371); omitted when empty |
| `commander_tax` | `CommanderTax[]` | Public per-commander tax owed on the next cast from the command zone (CR 903.8, issue #372); omitted when empty |
| `format` | `MatchFormat?` | The format this match is played under (issue #553); omitted by an older server, which a client MUST read as "unknown format, not Commander" |
| `commander_identity` | `CommanderIdentity[]` | Public per-seat commander name and colour identity (CR 903.3/903.4, issue #553); omitted when empty, and by an older server |
| `undo` | `UndoView?` | What **undo** can do at this table right now (issue #648): `{ available, limit }`. Present exactly when the room enabled `undo_enabled`; omitted at every other table, and by an older server |

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

`undo` is what the room’s **undo** rule can do right now (issue #648) — `{ available,
limit }`, both counts of *checkpoints*, where one checkpoint is one server-accepted
transition. Its **presence** answers “does this table allow taking an action back”, so it is
absent entirely at a table whose `RoomConfig.undo_enabled` is off, and `available` answers
“is there anything left to take back”, so `0` draws the control unavailable rather than
removing it. `limit` is how many checkpoints the room retains at most; it is on the wire
because a bound a client cannot see is a bound it would misreport, promising a rollback the
server never kept the state for. Public and identical for every seat — undo is a table rule,
not a personal preference — and both counts are the server’s: a client that counted
transitions itself would be holding load-bearing history across messages.

Per-seat presentation state (issue #553) rides the seat records themselves rather than a
parallel list. `OpponentView` gains `connected` and `ai`; `SelfView` gains `eliminated`,
`connected`, and `ai`, so the **receiver’s own** elimination — losing while two or more
players remain, CR 800.4a — has an authoritative source while the game continues (`result`
arrives only at game over, and the bounded `log` window is not reconstructable, so neither
may stand in for it). `connected` is the **one flag on the wire whose omitted value is
`true`**: the server holds a disconnected seat open, so the flag rides the wire only as
`false` and a client must test `=== false` rather than falsiness — an older server that
never sends it means every seat is connected.

`SelfView.maximum_hand_size` (issue #745) is how many cards the receiver may still be
holding when the cleanup step ends (CR 402.2). It is either `{"cards": n}` or the string
`"unlimited"`, and never a number standing in for "no maximum": a sentinel would be a
value nobody printed that every reader would have to recognise. It is stated because the
cleanup discard is the one turn-based action a player performs on their own hand, and a
client that assumed the default seven would tell a player holding nine cards they are
about to discard two when a permanent on the battlefield says otherwise. Omitted, it means
`{"cards": 7}` — not a guess, but exactly what every game an older server could run
actually used. `ai` carries the lobby’s `SeatView.ai` into
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
`life_changed`, `damage_dealt`, `cards_drawn`, `cards_milled`, `cards_exiled`, `cards_discarded`,
`library_searched`, `optional_applied`, `optional_declined`, `permanent_died`,
`step_changed`,
`player_eliminated`, `commander_returned_to_command_zone`, `game_over`, and `undone`. Named
`LogEntity` references have an opaque `id`
and server-supplied
`name`; the id may be used for presentational highlighting only. The `name` on every
reference is fixed at the moment the event was recorded, so an entry naming a permanent
stays stable after that permanent leaves play (dies, is bounced) — the server does not
re-resolve names against the current board.

A `cards_drawn` event contains only player and count, never a hidden card identity. A
`cards_milled` event carries the same two fields for cards put from the top of a library
into its owner's graveyard (CR 701.13). A `cards_exiled` event carries those same two fields
for cards exiled **from a library** face up (CR 701.16a) — the digging of a card that looks
through a library and exiles what it passes. Its identities are absent for a mill's reason
rather than a draw's: the cards are visible in the exile pile on their own, so the count is
what the log adds. It is deliberately *not* a `cards_drawn`: milling
never causes the empty-library loss, and its `count` is what actually moved, so a player
asked to mill past an empty library logs the smaller number. `cards_discarded` (CR 701.8)
carries the same player-and-count pair, and for the same reason: a hand is hidden, and the
cards become visible on their own once they are in the public graveyard.
`library_searched` (CR 701.19) carries only the player who searched — neither what they
looked at nor what they found, since a library is hidden from every other seat and naming
the found card would leak it to all of them before it arrives anywhere public. That a
search *happened* is public: everyone at the table sees the deck picked up.
`optional_applied` and `optional_declined` (issue #610) each carry only the player who
answered an optional effect's yes-or-no. What was offered is not repeated — the offering
ability's text is already public — and the two events do not distinguish a declined offer
from one that was never posed because its cost was unpayable, since telling those apart
would report on a mana pool the rest of the table cannot see. They are recorded at all
because the alternative is silence: an optional effect that happens reads exactly like a
mandatory one, and an optional effect that does not reads exactly like a bug.
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

`undone` (with the `player` who asked) marks a **rollback** at a table that allows undo
(issue #648) — the one event that reports something a player did to the game rather than
something the game did. It is also the only record of it: the entries the undone transition
wrote went back with the state that held them, so the window a client renders after a
rollback is the window as it stood at the restored checkpoint plus this entry. It names no
"what was taken back", because the state that would describe it no longer exists.

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

`auto_passed_from` is the other half of the same signal, and it is the half a player
actually reads. `auto_passed_steps` says *where* the room acted and never *what happened
there* — a spell that was cast, resolved, and killed a creature inside one settle is three
log events and zero steps anybody would recognise, so a client that shows only the path
leaves its player to work out a dead creature after the fact. The events are already in
`log`; what the log cannot say on its own is which of them this receiver missed, since that
depends on when they were last sent anything, and only the room knows that.

So the room states it: every `log` entry whose `sequence` is **at or after**
`auto_passed_from` happened while this receiver was not being asked. The mark begins at the
**action that triggered the settle**, not at the settle's first pass — an opponent's spell
is logged by their own action, and a report starting after it would omit the very event
being explained. Present exactly when `auto_passed_steps` is non-empty, and advisory and
display-only like the rest of the group: a client whose log window no longer reaches the
sequence simply shows the path, as before.

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
| `loyalty` | `string?` | Printed **starting** loyalty (planeswalkers only, CR 306.5b) |
| `keywords` | `string[]?` | Lowercase keyword names |
| `card_types` | `CardType[]?` | The card's types (CR 300), as the structured set `type_line` is rendered from; omitted when the server states none |
| `color_identity` | `Color[]?` | The card's **colour identity** (CR 903.4), in WUBRG order; omitted when empty |
| `token` | `boolean?` | The object is a **token** (CR 111) rather than a card; omitted (and `false`) for every card |
| `other_face` | `CardFace?` | The card's **other face**, for a card that has two (CR 712); omitted for every single-faced card |

`id` identifies one physical game object and is used by actions. `functional_id` identifies
the underlying card definition and is not a legal-action handle. Clients treat both as
opaque strings. The web client uses `functional_id` as the key of its client-local card-art
cache (ADR 0012) — a pure presentation enrichment; the wire contract is unchanged and a
client that ignores the field renders completely without it.

For a **battlefield permanent** every one of these fields is the permanent's *current*
answer, not its card's printed one. That includes a **copy** (CR 707, CR 613 layer 1): a
permanent that is a copy of something else projects the copied name, mana cost, type line,
card types, rules text, and power/toughness, and there is no copy badge or second identity
on the wire — the client is told what the permanent is and draws it. `id` is still the
permanent's own; only the characteristics come from elsewhere.

`color_identity` (issue #700) is what a card *belongs to*: its colours, the colours of the
mana symbols in its cost, and the colours of the mana symbols in its rules text. It is stated
for the same reason `card_types` is — the alternative is a client deriving it — and it is
stated on the card rather than only on a commander because it is the answer to the question a
board is scanned with. A Forest has no cost and prints no coloured pip, so a client reading
the cost alone can only call it colourless, and a mana base drawn in five shades of grey is
unreadable. It is the **same computation** the deck-legality gate and a seat's commander gems
use, so what a card is drawn as and what it is legal under can never disagree. It is **not**
the card's colour (CR 105) and must not be rendered as one.

`card_types` is the **set** behind `type_line`'s sentence, and both are projected from one
source so they can never disagree about the same card. It exists because the questions a
presentation actually asks — group the battlefield, put the lands in a row, arrange combat —
need to know that a permanent is a creature, and the only other way to find out is to parse
`"Artifact Creature — Thopter"`. That parse is exactly what a client must not do: it
re-implements a grammar in every consumer and it is wrong on the cards where the answer
matters, an animated land or a permanent whose types an effect changed. Values are lowercase
(`"land"`, `"creature"`, `"artifact"`, `"enchantment"`, `"instant"`, `"sorcery"`,
`"planeswalker"`, `"battle"`); a client that meets one it does not know renders the card
anyway, because the type line still says what it is.

Subtypes are deliberately absent. They are an open set of thousands, they belong to the
printed sentence, and no presentation keys off them. An **empty or omitted** list means the
server stated no types — a defensive placeholder for an object it could not resolve — and
never "this card has no types"; a client renders such an object normally rather than
concluding anything from the absence. `CatalogCard` carries the same field, so a card being
browsed and the same card in a hand present identically.

A card with **two faces** (CR 712) states the face that is **up** in every field above,
and the other one in `other_face`:

| Field | Type | Meaning |
| --- | --- | --- |
| `name` | `string` | The face's display name — different from the up face's on every printed two-faced card |
| `type_line` | `string` | The face's type line |
| `mana_cost` | `string?` | Absent for a back face (CR 712.4a); present when this is the front face of a permanent that has transformed |
| `rules_text` | `string?` | The face's generated rules text; omitted when empty |
| `power`, `toughness` | `string?` | Printed values, when that face is a creature |
| `loyalty` | `string?` | Printed starting loyalty, when that face is a planeswalker |
| `keywords` | `string[]?` | The face's printed keywords |
| `card_types` | `CardType[]?` | The face's types, as the set behind its `type_line` |

`other_face` carries two facts in one field, and both are things a client cannot work out.
Its **presence** is the statement that there is another side — that is what the board's
state mark is drawn from — and its **contents** are what the pinned preview turns over to
show (`docs/client-design.md` §6.7). A client cannot tell a transforming card from an
ordinary one, and it certainly cannot reconstruct a face nobody sent it.

It is **not a second object**. There is one physical card and one `id`; the two faces are
two sets of characteristics of it, which is why `CardFace` restates none of the fields that
belong to the *card* rather than to a face — no `id`, no `functional_id` (identity names
the card, ADR 0008 §3), no `token`, and no `color_identity` (CR 903.4 is computed across
both faces at once, so the value on the `CardView` is already right for either). A card in
a hand carries its back face here; a permanent that has transformed carries its **front**
face here, and a client draws whichever it is told is up without ever knowing which is
which. A **token** never has one: it has exactly one face, the effect that created it.

Which face is up is decided entirely by the server. A card outside the battlefield always
projects its front face (CR 712.4a), and a permanent projects the face it is currently
showing — transforming does not change the object, so the permanent's `id`, counters,
damage, and combat state are unchanged across it (CR 712.a). Additive: omitted by a server
predating the field, and a client that ignores it renders exactly as it did.

`loyalty` is what a planeswalker card *enters the battlefield with* — the number printed
in its corner — and never changes. It is **not** how much loyalty a planeswalker on the
battlefield has: that is its `loyalty` entry in `Permanent.counters`, which its abilities
spend and damage removes. Render this one on a card in hand, on the stack, or in a
graveyard; render the counter on the battlefield. Showing this one on a battlefield
planeswalker would report `4` for a planeswalker already down to `1`.

A **token** is a permanent the game created, with no card behind it (issue #605). It
projects as an ordinary `CardView` — name, type line, computed power/toughness, keywords,
and server-generated rules text all present — with two differences: `functional_id` is
**empty**, because there is no card definition to name, and `token` is `true`. The flag is
what a client should branch on: an empty `functional_id` alone is indistinguishable from a
card the server could not resolve, and the two want opposite treatment — a token is a real
object to render normally that simply has no identity to cache or look art up by, while an
unresolvable card is a fault. A token also has no `mana_cost` (CR 111.3). Tokens appear
only on the battlefield; a client will never see one in a hand, a graveyard, or exile,
because a token that would leave the battlefield ceases to exist (CR 111.7).

`OpponentView` contains `player_id`, `hand_size`, `life`, `library_size`,
`graveyard_size`, optional display-only `statuses`, and an optional `eliminated` boolean —
`true` when the opponent has left the game (CR 800.4a, issue #342/#345), omitted (and
defaulting to `false`) in a two-player game. `ZonePile` contains a `player_id` and ordered
`cards`; the top of the zone is last.

Both `OpponentView` and `SelfView` also carry an optional `counters` — counters on the
**player** (CR 122.1a), in the same `{kind, count}` shape a permanent's ride in, and public
information exactly as a life total is. It is omitted when empty, which is every seat in
every game the bundled catalog can currently produce: `poison` is the only kind defined
(CR 704.5d — ten of them and that player loses, surfacing as `result.reason` of `poison`),
and no card in the catalog gives one out. A client renders the list it is sent and derives
nothing: which kinds exist, and what any of them mean, is the server's to say.

### Permanents and stack objects

A `Permanent` contains:

- `id`, `controller`, `owner`, and a computed `card`. `id` names **this permanent** and lives in
  its own space, so it never collides with a card's — which says the two ids are different, and
  deliberately says nothing about whether the objects can be followed from one to the other. That
  question is `physical_card`'s;
- `controller` is the seat that controls the permanent **right now**, after CR 613 layer 2, and
  is the row a client draws it in. `owner` is the seat the card goes home to (CR 400.7). The two
  differ exactly while a control-changing effect is in force — a permanent someone has gained
  control of appears in the thief's row with the victim still named as its owner — and a client
  infers neither: both are stated;
- optional `physical_card` (issue #650), naming the **physical card** (CR 108.1) this permanent
  is a projection of. Omitted for a token, and by an older server — see
  [Following a card between zones](#following-a-card-between-zones);
- optional `tapped` and `attacking` booleans;
- optional `attacking_player`, naming the **defending player** for this attack
  (CR 508.1a, issue #341/#345) — the seat that answers for it, which when a planeswalker
  is being attacked is that planeswalker's *controller*. Omitted when not attacking;
- optional `attacking_planeswalker`, naming the **planeswalker** this attacker is
  attacking (CR 508.1a, issue #608), when it is attacking one rather than a player.
  Omitted otherwise. The pair is deliberate: one names what is attacked, the other names
  who answers for it, and a client draws its arrow at whichever it wants without deriving
  the relationship. A two-player client with no planeswalkers on the board may ignore
  both;
- optional `blocking`, the attackers this permanent is blocking as a list of entity ids
  (a blocker blocks one attacker unless an effect lets it block additional creatures,
  CR 509.1a). Omitted when it is not blocking, and **ordered**: the order is the blocker’s
  combat-damage assignment order (CR 509.3), which the declaration itself named and which
  a client renders rather than derives;
- optional marked `damage`;
- optional `attached_to`, naming the host permanent’s entity id when this permanent
  (e.g. an Aura, CR 303.4) is attached to another;
- optional `is_commander` (default `false`, issue #553), the server-computed marker that
  this object **is** somebody’s commander (CR 903.3) — matched on the card instance, so it
  survives every zone change and recast; a client must never infer it from a name, a zone,
  or a type line; and
- optional `counters`, each `{ "kind": string, "count": number }` — the kinds today are
  `"+1/+1"`, `"-1/-1"`, and `"loyalty"`. A planeswalker's `loyalty` counter is its
  **current** loyalty, the number every rule reads: its abilities spend it, damage removes
  it (CR 120.3c), and it is put into its owner's graveyard at zero (CR 704.5i); and
- optional `summoning_sick` (default `false`, issue #700), whether the summoning-sickness
  restriction of CR 302.6 **currently applies**: the permanent is a creature, its controller
  has not controlled it continuously since their most recent turn began, and it does not have
  haste (CR 702.10b). It is a *restriction*, not a property — a sick creature with haste
  reports `false` — and it is stated because no client can derive it: continuous control is
  stored engine state, haste may be granted by an Aura or a pump, and the absence of an attack
  action means nothing outside the declare-attackers step. It comes from the same engine
  predicate that gates attacking and `{T}` costs, so the board and the action list agree; and
- optional `skips_next_untap` (default `false`, issue #730), whether this permanent will **not**
  untap in its controller's next untap step (CR 502.4). Stated for the reason `summoning_sick`
  is: the spell that imposed it is in a graveyard and the permanent's own printed text says
  nothing about it, so without the field a tapped creature that stays tapped through an untap
  step is a rule the board applied and never explained. Like summoning sickness it reports the
  *restriction* a player is looking at rather than a mechanism — a card names one untap step and
  the engine holds one flag, so there is no count to expose; and
- optional `granted_keywords`, the keywords this permanent has that its **printed card does
  not** (CR 613 layer 6, CR 613.1f) — the trample an until-end-of-turn pump gave it, the
  flying an Aura grants, the vigilance an anthem hands a whole team. `card.keywords` already
  carries the *current* set and `card.rules_text` is the *printed* card's text, so between
  them a client cannot say which words are new; subtracting one from the other means matching
  generated prose against keyword names, which is a client reading rules text to learn a rules
  fact. Carried as the **words a card prints them with** — `"Trample"`, `"First strike"` —
  because they are drawn as text beside text. Omitted when empty, which is every permanent
  whose abilities are all printed; and
- optional `chosen_color` (issue #738), the colour this permanent's controller **named as it
  entered** the battlefield (CR 614.12) — the "chosen color" its own rules text refers to.
  One of the wire's colour letters, omitted for every permanent that named none. Stated
  because there is nothing to infer it from: it is a decision a player made, recorded on this
  one object. It is *not* the permanent's colour — a colourless artifact may have named red —
  it is nowhere in `card.color_identity`, and it does not follow from the printed cost, since
  two copies of one card side by side may have chosen differently. Public: it was announced as
  the permanent entered, so every seat and every spectator receives it; and
- optional `named_card` (issue #738), the card this permanent's controller **named as it
  entered** the battlefield (CR 614.12) — the "chosen name" its own rules text refers to.
  `chosen_color`'s sibling in every respect, public the same way, and omitted for every
  permanent that named none. It is **the catalog's own name for that card**: the engine
  records a functional identity and the server resolves it here, so a client never handles a
  card handle and the only names that can appear are names SAGE has itself defined. Render
  it; never read it as the name of *this* permanent.

These fields describe server-computed state. They do not authorize interaction.

### Emblems

An `Emblem` (CR 114, issue #620) is a marker one player has, whose only characteristics are
its abilities:

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | `EntityId` | Per-game id, stable for the rest of the game; never collides with a permanent's or a card's |
| `controller` | `PlayerId` | The player who has it. Control never changes |
| `abilities` | `string[]` | Its abilities as server-composed rules sentences, in order; omitted when empty |

It rides **beside** `battlefield` rather than inside it because it is not a permanent: it
cannot be tapped, attacked, blocked, damaged, destroyed, or targeted, and no other field of a
`Permanent` would mean anything on one. It is in no zone, and nothing in the game removes it —
so a client that has rendered an emblem never has to un-render it for any reason but the
server saying the list is shorter.

**Public information**: every seat and every spectator receives the identical list, and there
is nothing about an emblem to redact. The list is omitted (read as empty) in the
overwhelming majority of games, where no ultimate has resolved.

A `StackItem` describes one object on the stack:

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | `EntityId` | Per-game id of this stack object |
| `controller` | `PlayerId` | Player who controls it (chooses targets and resolution) |
| `description` | `string` | Display text: a spell’s name, or an ability’s composed sentence |
| `source` | `EntityId?` | Source permanent for an ability; **omitted for a spell** |
| `physical_card` | `EntityId?` | The **physical card** being cast (CR 108.1, issue #650); **omitted for an ability**, which has no card, and by an older server |
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

### Following a card between zones

`Permanent.physical_card` and `StackItem.physical_card` (issue #650) each name the **physical
card** (CR 108.1) that projection is of, as an `EntityId` — the same id that card carries as its
`CardView.id` wherever a view shows it in a zone: in `my_hand`, in a `ZonePile`, in `revealed`.
No new id space is introduced; the values join to the rest of the view by construction.

**It is not object identity.** CR 400.7: *"An object that moves from one zone to another becomes
a new object with no memory of, or relation to, its previous existence."* The permanent that died
and the card now in the graveyard are two different objects, and their differing ids are the rule
rather than an oversight. This field states only that both are projections of one physical card —
the thing a player's eye follows across the table, which is a strictly weaker claim than identity.

A client may therefore use it to **follow a card**, and may never conclude that counters, damage,
marked state, attachments, control, targeting, or anything else came across, because CR 400.7 says
none of it did. The exceptions (CR 400.7a–400.7m) are the server's to apply; where one is in force
the server states the resulting state on the new object directly, and nothing about it becomes a
client's business.

Four rules complete the shape:

- **It addresses nothing.** `valid_actions[].subject`, `StackTarget`, `attached_to`, `blocking`,
  `attacking_planeswalker`, and the relationship join all address objects by their per-zone entity
  ids, unchanged. This field answers one question and is never a second handle for an object. A
  client that sends it back where an `id` belongs is sending an id the server does not recognize.
- **A token names none.** A token (CR 111) is not a card, so there is no physical card for it to
  be a projection of, and the field is omitted. `card.token: true` and an absent `physical_card`
  say the same thing from both ends. (The server does hold a per-object handle for a token, but
  CR 111.7 — a token that leaves the battlefield ceases to exist — means it could never appear in
  a hand, a graveyard, or exile, so stating it would offer a join with no possible second end.)
- **An ability names none.** An ability on the stack (CR 113.3) has no card behind it, activated
  or triggered alike. Its `source` names the permanent it came from, which is a different
  question — and note that a `StackItem.card` for an ability is that *permanent's* face, keyed by
  a `perm_` id, so joining on `card.id` would silently mix id spaces on exactly the entries where
  the answer is "there is no card". Use this field.
- **Nothing hidden becomes linkable.** The field rides only on a battlefield permanent and on a
  spell on the stack, and both are public objects whose whole face the same view already carries —
  so every seat and every spectator is told the identical thing, and there is no receiver-specific
  withholding because there is no receiver for whom either object is hidden. The other half is
  what is *not* projected: a card in a hand is in no view but its owner's, so no id for it reaches
  anybody else and there is nothing for them to join to later.

**Purely a function of the current state.** The server reads the card instance the engine already
stores on the permanent and on a spell's stack object; there is no diff against a previous view,
no history, and no server-side memory. That is deliberate — "what was this a moment ago" is
history the engine drops on purpose (CR 400.7), and reconstructing it would mean either the engine
remembering across a zone change or the server diffing two states, both of which ADR 0005 keeps
out to leave the engine undo/replay/resync free. Correspondingly a client must hold no
"what this used to be" map across messages: after an undo, such a map would describe a future that
no longer happens. Every view states where every card it can see is, on its own.

A client that does not know the field renders exactly as it did.

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
  action such as passing priority. More than one id is not a list of separate actions: it is
  the same action reachable from each of them, which is how a trigger waiting to be aimed is
  offered both where it sits on the stack and from the permanent whose ability it is.
- `mana_ability` (optional, default `false`) marks the activation of a mana ability
  (CR 605): no targets, no stack, only mana production. Server-computed so a client may
  offer a lighter gesture — one-click tap-for-mana — for exactly these actions without ever
  classifying abilities itself. Omitted when `false`.
- `cost` (optional, issue #735) states what a **cast** costs in mana, as
  `{ printed?, modified }` — both in `{...}` notation. `printed` is the cost on the card
  and `modified` is what the game will charge: the printed cost plus the commander tax
  where one applies (CR 903.8), after every cost-modification effect in force
  (CR 601.2f). The two are equal for nearly every cast; they differ when a permanent on
  the battlefield makes a class of spells cheaper or dearer, and then the difference is
  the point — the card keeps its printed cost and the surface a player acts on carries
  the modified one, marked against the printed one beside it. `modified` is `"{0}"` for a
  cost reduced to nothing, which is a real cost and not an absent one. Present on a cast
  and omitted for every other action, none of which has a mana cost to state. **Display
  text**: a client draws the symbols and parses neither value — the arithmetic behind
  `modified` is the server's, and a client that reproduced it would be computing cost.
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

Each requirement contains an opaque `slot`, display `prompt`, an optional `optional` flag, an
optional `subject`, an optional `taps` list, and the complete set of legal candidate entity
ids. The server enumerates
candidates per slot rather than enumerating the cartesian product of possible answers.

`subject` (issue #700) names the entity a slot is **about**, when it is about one. A combat
declaration is several slots that all list the same candidates and differ only in whose choice
they are — one per attacker for what that attacker attacks, one per attacker for what blocks
it — and the correlation was previously readable only by parsing the slot id, which the slot
id's own contract forbids. With it stated, a client can ask the choices one subject at a time,
draw the arrow from the card the choice belongs to, and show a per-attacker slot only once
that attacker is in the declaration. It is absent for every slot that is about the action as a
whole: an ordinary spell's target slot, the `attackers` multi-select.

`taps` names the `candidates` that answering this slot with them would **tap**: the attackers
in a declaration that are not vigilant (CR 508.1f, CR 702.20b). A declaration and a payment are
both assembled a choice at a time and send nothing until they are confirmed, so the board a
player is looking at while they choose is one the server has not been told about yet — and what
the choice *does to the card* is a rules question. Stating it per candidate lets a client turn
each card as it goes into the slot and turn it back as it comes out, without judging a keyword.
It is a subset of `candidates`, in the same order, and is omitted for every slot whose answer
taps nothing: an ordinary spell's target slot, a blocker assignment (blocking does not tap,
CR 509.1), and the per-attacker defender slots.

`optional` (issue #620) says the slot **may be left unanswered** — the "up to" of *put a +1/+1
counter on each of up to two target creatures*. It is absent (read as `false`) for every slot
of an ordinary targeted spell or ability, which must be filled or the submission is rejected.
An effect that may name fewer targets than it allows is advertised as its maximum number of
slots, of which the ones past its minimum carry the flag; the client omits those from its
answer, or sends them empty, and the server accepts either. A client MUST NOT infer the bound
from anything else: an unflagged slot is required and a flagged one is not, and that is the
whole rule.

The **combat declaration** multi-selects carry it too: declaring no attackers and blocking with
nothing are both legal declarations (CR 508.1a, CR 509.1a), which is exactly "this slot may be
left unanswered", and the resolve path has always bound an empty declaration directly. Saying so
is what lets a client tell a slot it may skip from one whose emptiness makes the submission
rejectable, since on the wire the two are otherwise the same shape.

Non-target choices use tagged `prompts`:

| `kind` | Fields | Answer |
| --- | --- | --- |
| `option` | `slot`, `prompt`, `options[{id,label,requires}]` | One option id |
| `select_from_zone` | `slot`, `prompt`, `zone`, `owner`, `count`, `min?`, `candidates` | Between `min` and `count` candidate ids, in the chosen order |
| `order` | `slot`, `prompt`, `items` | A permutation of all item ids |
| `number` | `slot`, `prompt`, `min`, `max`, `values?[{value,cost?}]` | The chosen number as a decimal string |
| `pay_mana` | `slot`, `prompt`, `pip`, `candidates[{id,source,label?,taps?}]` | One candidate `id` |

`option` is used for choices such as keep or mulligan. An option's `requires` (issue #451)
lists the action's other slots **that choice** owes an answer to, and is omitted when it owes
none: the `mulligan_decision` action carries the `decision` option slot plus, once the seat
has mulliganed, a `select_from_zone` `bottom` slot over its hand, and only the *keep* choice
requires `bottom` — taking another hand bottoms nothing. A client enables a choice once every
slot it requires holds exactly the advertised number of ids; the server enforces the same
coupling on resolution, so `requires` changes no legality, it only keeps a client from
offering an answer that must be rejected.

`select_from_zone` supports choices such as discarding, bottoming, scrying, or searching.
Its `zone` is a **free-form string** rather than an enum precisely so a new zone needs no
new prompt kind: a cast's additional cost poses `"hand"` for `As an additional cost,
discard a card` and `"battlefield"` for `sacrifice a creature` (CR 601.2b / 701.17), on
the `cost_discard` and `cost_sacrifice` slots, and both are answered with entity ids from
the server-enumerated `candidates` like any other selection. A client that renders "pick
from this list" already renders both; one that ignores the slots leaves the cost unpaid,
and the server pays it (ADR 0010).

**An `activate_ability` carries the same slots** when its cost asks the player to pick
what pays it — `{B}, Sacrifice another creature:` poses `cost_sacrifice` over the
battlefield, `{T}, Discard a card:` poses `cost_discard` over the hand, and
`{2}{B}, Exile a creature card from your graveyard:` poses `cost_exile` over `"graveyard"`
(CR 701.19) — with the same slot
ids, the same server fallback, and no new prompt kind for the third zone, which is the
whole point of `zone` being free-form. The slot's
`prompt` is the cost as the card writes it, so a player is asked the question printed on the
permanent. Nothing about any of them is action-kind-specific on the wire, and a client that
already answers them on a cast answers them here without learning anything new.

**Every cost slot is an exact selection.** `Sacrifice two artifacts` is `cost_sacrifice`
with `count: 2` and no `min`, which is the shape every cost slot has: a cost is paid for
what it asks and not for less. A sacrifice whose *size* the player picks — `Sacrifice any
number of lands` — is not a cost at all but a resolution's question, so it reaches the
client as a `player_choice` prompt over the battlefield rather than as a slot on the cast.

An activation poses **no `pay_mana` slots**: an activated ability's mana comes from its
controller's pool (CR 602.2b), floated by activating mana abilities as actions in their own
right, which is why such an ability is only offered once that mana is available. Pips are a
cast's shape and stay one.
Its `count` is the **maximum** number of ids a legal answer may name; `min` is the
minimum, and is **omitted when it equals `count`** — which is every exact choice, and the
only shape this prompt had before issue #604. It is present exactly when a player may
legally under-fill the slot: scrying *any number* of the cards looked at, taking *up to*
one of them, or failing to find on a search (CR 701.19c). A client that ignores `min`
therefore behaves as it always did on exact prompts, and only over-constrains the new
ones. The **order** of the returned ids is significant and is preserved by the server: it
is the order a scry puts its cards on the bottom in. `order` requests a permutation of its `items`. Two actions emit one: the
`order_combat_damage` action emits one `order` prompt per attacker blocked by two or more
creatures, so its controller chooses the combat-damage assignment order (CR 510.1, issue
#346) — lethal damage is then assigned to the blockers along the chosen order. An attacker
with 0–1 blockers produces no ordering prompt. And `player_choice` emits one for the *in any
order* of a look putting cards back on a library (issue #746, below), over two items or
more. Both follow the same rule: fewer than two items is not a decision, and no prompt is
emitted for one.

`number` (issue #554) requests a value in the inclusive range `min`..`max` — the value
of X, how many counters to remove, one share of a divided effect. It is answered with
the chosen number rendered as a **decimal string** in the slot’s `chosen` array (e.g.
`["3"]`), sharing `TargetChoice` with every other slot kind so one atomic
`choose_action` still answers a whole action and the content `token` still binds every
slot (the bounds *and the enumerated values* are folded into it, so an answer bound to a
range or a price the server no longer offers is rejected like any other stale binding).
**The bounds are the server’s**, computed from available mana, the source’s text, and the
game state; the client offers a control over exactly that range and computes no
affordability of its own. Both `min` and `max` are always present — a zero `min` is not
elided — so the range reads completely rather than by inference. A *divided* value is
posed as one `number` slot per recipient, each with its own bounds, and the server
validates the total on resolution; the client never enforces a sum.

Since issue #706 a `number` slot may also carry a **mid-resolution** X — the `you may pay
{X}` of a triggered ability, which has no announcement to ride on. It rides the same
`player_choice` action every other mid-resolution answer does, on the same `choice` slot,
so a client that can answer a yes-or-no and announce an X can answer this with no new
shape to learn. Its bounds are recomputed on every projection, because a player owed the
question may still activate mana abilities before answering (CR 605.3a) — the range grows
as they tap.

`values` (issue #733) is present exactly when the number is **the X of a mana cost**, and
it lists every legal value together with what announcing it costs:

```json
{
  "kind": "number",
  "slot": "x",
  "prompt": "Choose a value for X",
  "min": 0,
  "max": 3,
  "values": [
    { "value": 0, "cost": "{R}" },
    { "value": 1, "cost": "{1}{R}" },
    { "value": 2, "cost": "{2}{R}" },
    { "value": 3, "cost": "{3}{R}" }
  ]
}
```

A range alone is enough for a number that costs nothing. It is not enough for X, because
**choosing X changes what the spell costs**, and a client that worked the new cost out
would be deciding what a spell costs — the one thing it must never do. So the server
never sends `{X}{R}` and leaves a multiplication to whoever draws the bar; it sends the
values and their prices, and the stepper walks exactly that list (`client-design.md`
§6.7). Each `cost` is the **whole** cost at that value, in printed `{...}` notation, with
no `X` left in it and never a delta. Where the list is present it and the range agree;
`values` is omitted entirely for every other `number` slot, which therefore serializes
exactly as it did before this field existed.

`pay_mana` pays **one pip** of a cost by tapping something (CR 601.2f–g). A cast poses one
of these slots per unit of its cost — `{1}{W}` is two of them — and a cast covered by mana
already floating (CR 605.3) poses none at all.

One slot per pip is what lets a client show a running cost without doing arithmetic: **the
still-to-pay line is the unfilled slots**, drawn from their `pip` symbols. Filling a slot
removes a pip; taking it back out puts it back. Nothing subtracts a cost from anything —
which is deliberate, because cost arithmetic is exactly what a client must not do. It also
makes "may this be cast yet" the slot-counting test every other multi-slot action already
uses: every mandatory slot filled means the cost is covered.

Each candidate names a permanent to click (`source`) and the activation to send back
(`id`), and those are **different fields on purpose**. A permanent that could pay the pip
more than one way — a dual land is `{T}: Add {W}` *and* `{T}: Add {U}` — appears once per
way, with the same `source`, a different `id`, and a `label` naming what it produces. So a
client asks "which one did you mean?" exactly when the slot it is filling lists the clicked
`source` more than once, and offers the labels as the answers. It needs to know nothing
about mana to get this right. Where the choice cannot matter the server does not offer it:
a generic pip is paid equally well by either half of a dual land, so it lists that permanent
once and the player is never asked a question with one meaningful answer.

A permanent can be tapped once, so `source`s are **not** shared across the slots of one
action: a client must not offer a source already spent on another slot, and a submission
naming one twice is rejected. A slot with no candidates cannot be filled — a client offers
no way to fill it rather than guessing.

A candidate's `taps` says whether sending it **taps its `source`** — the `{T}` in `{T}: Add
{G}` (CR 602.2a). It is the payment's half of the same statement `taps` makes on a target
requirement, and for the same reason: the sources a player picks are not spent until the cast
is confirmed, so a client drawing them as tapped is drawing a board the server has not been
told about yet. It must be told which ones turn, because a mana ability that sacrifices its
source or pays life taps nothing and no client can tell those apart without reading the cost.
Omitted when `false`.

### Announcing a spell: the mode and X

A cast makes up to two choices **before** it chooses targets and before it pays
(CR 601.2b, issue #733), and both ride the prompt kinds above rather than a new shape:

| Choice | Slot | Prompt kind |
| --- | --- | --- |
| The mode of a modal spell (CR 700.2) | `mode` | `option`, one option per mode |
| The value of X | `x` | `number`, carrying `values` |

The order is not decoration. **A mode decides which target slots the spell has**, so the
mode is asked first and the targets cannot be asked at all until it is answered; X is
asked next because it decides what the spell costs. A spell with neither — every other
card in the catalog — is unchanged and simply starts at its targets.

A mode option's `label` is the mode's own generated sentence, so a player picks between
the words the card prints, and its `requires` names the target slots **that mode** owes.
That is the existing `option` coupling doing exactly what it was built for: a modal cast
advertises every mode's slots side by side, named `m<mode>t<index>` rather than
`t<index>`, all marked `optional` because at most one mode's are ever filled, and
`requires` is how a client tells which belong to which. The server binds the answer
against the chosen mode's requirements and the engine re-derives the whole thing again;
`requires` changes no legality.

Both choices are **re-validated independently at apply**. A mode index the card does not
print is rejected, an announcement that skipped the mode question is rejected, a value of
X the offer did not enumerate is rejected, and an X the payment cannot cover is rejected —
each of them at the engine's own gate, not merely left off the offer. The offer is
computed before the player has chosen anything; what they chose is a separate question.

The stack entry for an announced spell states what was chosen: its `description` carries
the mode's sentence and the value of X (`Banefire (X=5)`), because two casts of one card
at different values are two different things to everyone deciding whether to respond.

`choose_targets` aims a **triggered ability already on the stack** (CR 603.3d). A trigger
is put there by the game rather than by a player, so it arrives unaimed and its controller
is asked to fill one target slot per targeting effect — the same per-slot `requirements`
a cast or an activation carries, bound by the same token. While one is owed the server
offers that seat nothing else (and no other seat anything at all): the ability goes on the
stack before any player receives priority (CR 603.3b), so play does not continue around it.
The seat asked is the trigger's *controller*, which is frequently not whoever last acted —
a creature killed by an opponent's removal spell gives its own controller the choice.
A trigger with no legal choice for a slot never reaches the stack at all (CR 603.3c), so a
`choose_targets` is always answerable. Its `subject` names two entities — the trigger's own
stack object and the permanent whose ability it is — because both are places a player looks
for it; either id reaches the same action.

`player_choice` answers the **mid-resolution player choice** an effect has posed (issue
#604): a discard, a scry, a look at the top N, or a library search. Unlike every prompt
above it, this one interrupts an object that is *part-way through resolving* — the game
does not proceed until it is answered, so while one is owed the server offers that seat
this action and a concede, and every other seat nothing at all. The seat asked is the one
the **effect names**, which is frequently neither the priority holder nor the resolving
object's controller: "target player discards two cards" asks the targeted seat, while a
coercive hand attack asks the *caster* to choose from the opponent's hand. Priority
returns to whoever it was taken from once the choice is answered.

The action carries one `select_from_zone` slot (`choice`) whose candidates and bounds are
the engine's, already clamped to what the zone actually holds — so "discard two cards"
against a one-card hand advertises `count: 1`, and a choice with no legal answer at all is
never posed (the effect applies with an empty selection instead, and the game moves on).
The cards the slot names are carried on the same view's `revealed` array, and on no other
seat's, which is how a searching player sees their library without the table seeing it.

The same `player_choice` action also carries the **yes-or-no of an optional effect**
(issue #610) — "you may draw a card", "you may pay `{1}`. If you do, draw a card". That
question adds no wire shape: it rides the `option` prompt on the same `choice` slot, with
an `accept` and a `decline` choice, and it is answered the same atomic way. Three things
distinguish it from a card selection:

- The seat asked is the offering ability's **controller**, not a seat the effect names.
- `accept` is listed **only while the server would accept it** — an optional cost the
  chooser cannot currently pay leaves `decline` as the only option. Declining is always
  offered, so an unpayable cost can never stall the game, and a cost no amount of tapping
  could pay is never posed at all.
- While such a question is owed, the chooser is additionally offered their **mana
  abilities** (CR 605.3a: a player asked to pay during resolution may make mana), marked
  as usual with `mana_ability`. Activating one answers nothing — the question stays owed —
  but `accept` appears once the pool can pay. No other action, and no other seat, becomes
  legal. A cost paid by **sacrificing or discarding** (issue #744) does not widen that:
  its `accept` is labelled with the payment as the card writes it ("Sacrifice another
  creature"), and accepting owes the payment as the *next* `player_choice` — the ordinary
  `select_from_zone` over the battlefield or the hand — which is answered before the
  effects the payment bought happen. Nothing else is legal while that one is owed.
- Nothing is revealed: a yes-or-no is about an effect, not a zone, so `revealed` stays
  empty.

The same `player_choice` action carries a third question: **which colour?** It adds no wire
shape either — it is the `option` prompt on the same `choice` slot, listing the five colours,
and every one of them is always a legal answer, so unlike the yes-or-no it never withholds an
option. Nothing is revealed.

Two things ask it, and the prompt's own sentence is what tells them apart:

- **Which colour of mana**, for an effect that adds mana in any combination of colours. Such
  an effect poses **one question per point**, so the client answers this action once per mana
  and may name a different colour each time.
- **Which colour a permanent enters with** (CR 614.12, issue #738) — the choice a card makes
  as it arrives, which the prompt names the card in: *"Choose a color as Diamond Mare enters
  the battlefield"*. While it is owed the permanent is **not yet on the battlefield**: the
  spell has left the stack and its card is in no zone, exactly as a spell's card is while a
  mid-resolution choice suspends it, so a client that renders the board it is sent is never
  showing a permanent whose colour has not been named. Answering makes it appear, with
  `chosen_color` already set on it.

The same `option` prompt carries **which card a permanent names as it enters** (CR 614.12,
issue #738) — *"Choose a card name as Alpine Moon enters the battlefield"* — under the same
freeze and with the permanent likewise not yet on the battlefield. Its options are the cards
the **server** says may be named: each option's `label` is the card's name and its `id` is
that card's authored `functional_id`, the same stable identity `card.functional_id` carries
everywhere else. The client picks one and echoes the id; it composes no list, sends no typed
name, and an id the offer did not list is refused rather than guessed at.

The same `player_choice` action carries another question: **in which order?** (issue #746)
— the *put the rest on the bottom of your library in any order* of a look. It adds no wire
shape either: it is the `order` prompt the `order_combat_damage` action already rides on,
on the same `choice` slot, and it is answered the same atomic way — with a **permutation of
every one of its `items`**, no more and no fewer. Four things distinguish it from the three
questions above:

- It is the **second** question one effect asks. A look that says "in any order" poses its
  card selection first (*which one do you keep?*) and this one only once that is answered,
  because until then nobody knows what "the rest" is. The action id and the slot are the
  same both times; the prompt `kind` is what changed.
- The answer's **order is the whole answer**, and the prompt says which end is which:
  *"Choose the order these go on the bottom of your library, deepest first"*. The first id
  sent ends up deepest, which is the same convention a `select_from_zone` bottoming uses.
- A permutation of one item, or of none, is not a decision, so it is **never posed** — the
  cards are bottomed and the resolution carries on. A client will not see this prompt with
  fewer than two items.
- The items are cards from the top of a library, so they ride the same `revealed` array a
  search does, on the chooser's view and no other seat's.

The counterpart of *in any order* is *in a random order*, which is not a question at all:
the server bottoms those cards itself and no prompt is emitted. Which of the two a card
uses is the card's own text and never a client decision.

And one more: **which permanents to sacrifice** (CR 701.17), for an effect that makes a
player sacrifice a number of their own permanents mid-resolution. It adds no wire shape
either — it is the same `select_from_zone` prompt on the same `choice` slot, with a `zone`
of `"battlefield"` and permanent entity ids for candidates, which is exactly what that
field being free-form is for. The seat asked is always the sacrificing player (CR 701.17b
lets nobody sacrifice what they do not control), the count is the engine's already-clamped
bound — a player told to sacrifice two who controls one is offered `count: 1` — and a
player who controls nothing of the named class is never asked. Nothing is revealed: the
battlefield is public.

Combat declarations also use requirements. The `attackers` slot lists creatures eligible to
attack; blocker slots list eligible blockers for each attacker. When there is more than one
thing to attack (issue #345, widened by #608), `declare_attackers` additionally offers one
**defender slot per attacker candidate** — a slot whose candidates are everything that
attacker may be declared to attack (CR 508.1a); the client answers one for each attacker it
declares, and the slot names its attacker in `subject`, exactly as a blocker slot names the
attacker it assigns blockers to.

Those candidates are **player ids and permanent ids in one list**: an attack may name an
opponent or a **planeswalker** they control. The two are told apart by which collection the
id appears in — a `p…` seat id in `seat_order`, a `perm_…` id on the battlefield — and a
client need not classify them at all to answer, since it echoes back an id the server
offered. A game in which there is only one thing to attack — two players, no planeswalker on
the far side — offers **no** defender slots at all, so the wire and the client flow are
exactly as before. The gate is the number of *targets*, not the number of opponents: a
two-player game becomes a real choice the moment an opponent resolves a planeswalker.

`declare_blockers` requirements are scoped to the player who currently owes the declaration
(issue #344): with attacks split across defenders, each attacked player sees only the
attackers attacking them — including attackers aimed at a planeswalker they control, since
they are the defending player for those too.

A blocker slot carries its attacker's restrictions in two different places, according to what
kind of restriction it is (issue #606). A **pairwise** one — flying, "can't be blocked",
"can't be blocked by black creatures" — is a fact about one attacker/blocker pair, so it is
projected as the slot's `candidates`: only creatures that may legally block *that* attacker
are listed, and an attacker nothing may block gets no slot at all. A **whole-selection** one
constrains *how many* blockers may be assigned — menace's two-or-more (CR 702.110b) and the
"no more than one" ceiling (CR 509.1b) — and the engine can only reject it once the
declaration is assembled, so the slot's `prompt` states it in words rather than letting a
submit silently do nothing. Either way the server asks the engine and the client still
computes no legality: it renders the candidates and the prompt it was given.

The same creature may legitimately appear in **more than one** blocker slot's answer, and
be sent in both: a blocker blocks one attacker unless an effect lets it block additional
creatures (CR 509.1a, issue #739). Which creatures those are is a fact about the blocker
rather than about any slot, and it is printed on the card — its `rules_text` says so — so
no slot advertises it and a client must not try to work out how many assignments a
creature may take. It sends the declaration the player assembled; the engine judges it, and
rejects the whole declaration if a creature was assigned to more attackers than it may
block. The order the assignments are sent in is the order the blocker will assign its
combat damage (CR 509.3), and it is what comes back as that permanent’s `blocking` list.
Empty selections are legal for these optional
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

### `Undo`

The third in-game client message asks the room to restore the state before its last accepted
transition (issue #648). It is a bare tag:

```json
{ "type": "undo" }
```

It carries nothing, and that is the shape of the feature: *which* state to restore is the
server’s alone — a message that named one would be a client asserting a game state — and the
sender is the connection’s own seat. It is a separate message rather than a `ValidAction`
because an undo is not a play: the rules never offer it, it takes no priority, and it is legal
for a seat that is not being asked anything. Availability still rides the view (`GameView.undo`),
so the client renders the control from what the server stated and computes no legality, exactly
as it does for `set_stops`.

The server is authoritative for everything about it:

- **Who may.** Any seat in the room, at any time during the game, with no vote and no
  approval from the others. A table that did not want that did not enable the rule.
- **What a checkpoint is.** One server-accepted transition — the state as it stood when the
  room last put a question to the table, captured immediately before the action that left it.
  So one undo takes back one action **and** whatever the settle (ADR 0010) did after it; the
  pair is what a player experienced as a single move.
- **What is restored.** The whole authoritative state: hidden zones and library order, the
  stack and pending choices, priority and pass state, turn and step, mana pools and payments,
  counters and attachments, continuous-effect inputs, the deterministic RNG position, and the
  game-over verdict. A rollback restores a whole state, never an enumerated subset of it.
- **What happens to newer history.** It is discarded. Restoring a checkpoint pops it, so play
  after a rollback builds a new branch; there is no redo.
- **How deep.** Bounded (`GameView.undo.limit`); the oldest checkpoint is dropped past it.
- **When it is refused.** The table does not allow undo, or no earlier checkpoint survives.
  A refusal changes nothing and re-sends the sender’s current `GameView` with
  `action_rejected` set — the same non-fatal answer a stale `choose_action` gets.

On success every connected seat and every spectator is pushed the restored state as an
ordinary full view, and the rollback is recorded in the log as an `undone` event naming the
player who asked. Undoing the transition that *ended* a game returns the table to live play.

A rollback can return information to a hidden zone that players have already seen — a drawn
card, a revealed choice. That is inherent, it is why the rule is opt-in per room, and the
lobby says as much where the table is made: undo is for casual play, testing, and fixing a
misclick, not for competitive integrity.

### Game result

When the game ends, `result` is present and `valid_actions` is empty:

```json
{
  "winner": "p0",
  "losers": ["p1"],
  "reason": "decked"
}
```

`winner` is absent for a draw. `reason` is one of `life_zero`, `decked`, `concede`,
`commander_damage` (a player took 21+ combat damage from a single commander, CR 903.10a),
`poison` (a player had ten or more poison counters, CR 704.5d), or `opponent_won` (an
effect stated that a player *wins* the game, CR 104.2b, so everyone else lost it — the one
reason that describes what a card did rather than what happened to the loser).
Further submitted actions are rejected and the final view is re-sent.

### `SpectatorView`

A connection that joined with `spectate_room` (issue #351) receives a
`SpectatorView` instead of a `GameView` on every change — a **non-seated observer** watching
the game live with all hidden information redacted. Redaction is **structural**: the type
simply has no receiver or decision fields, so a projection cannot leak a hand, a library’s
contents, a mana pool, or a `valid_actions` list to a spectator. It reuses `GameView`’s public
component types verbatim (`OpponentView`, `Permanent`, `Emblem`, `StackItem`, `ZonePile`,
`GameLogEntry`, `Phase`, `PlayerId`, `GameResult`, `CommanderDamage`, `MatchFormat`,
`CommanderIdentity`).

| Field | Type | Meaning |
| --- | --- | --- |
| `players` | `OpponentView[]` | **Every** seat as public state and hidden-zone counts — no privileged “self” |
| `battlefield` | `Permanent[]` | Public permanents and computed state |
| `emblems` | `Emblem[]` | The emblems in the game (CR 114, issue #620); omitted when empty |
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

**A `hello` that reclaims a seat whose game is still running is answered with the game, not
with a `LobbyView`** (issue #628). The reconnecting connection is put back on the in-game
contract exactly as the ready gate puts it there — it joins the room and is brought current
with one complete `GameView` — because a held-open seat belongs to a match in progress and a
lobby view would say nothing about it. A reclaimed session that is *not* seated in a live game
(pre-game, or a game whose room has finished) is answered with its lobby view as before.

Two consequences for a client. First, a reconnect may deliver **no** `LobbyView` at all, so a
client must not wait for one before considering itself resumed. Second, a fresh connection is
issued its own session and its own `LobbyView` before its `hello` is read, and that frame
carries a *different* `session` — a client that overwrites its stored token with every
`LobbyView` it sees will discard the token that owns its seat. The token to keep is the one
that reached the game.

`RoomView` contains an opaque `room_id`, a `config`, and the ordered seat roster. The room
config contains `seats`, an opaque `game_setup` id, an optional table `name`, and a
`visibility` (issue #546). The lobby validates a 2–8 seat range,
requires the setup id to exist in the server format registry, and rejects a seat count
outside the chosen format's own range (issue #349). Two-player formats and 3–4 seat
free-for-all formats both start real games.

**A format's name and its advertised seat range describe the same game** (issue #707). A
`game_setup` naming a duel — `starter-1v1`, `standard_2p`, `1v1` — advertises `2..=2` and
rejects a third seat; `standard_ffa` and `ffa-4` advertise `3..=4`; `commander` advertises
`2..=4`; and the permissive catch-all that spans the lobby's whole `2..=8` plumbing is
`standard_multiplayer`, named for what it seats. **Migration:** before #707 `standard_2p` and
`1v1` both resolved to that permissive format and would open a room seating up to eight. A
client that asks for more than two seats on either id is now rejected with
`SeatCountForFormat` rather than silently opening a table its name misdescribes; the room it
wanted is created by naming `standard_multiplayer`. A client reads every range off
`CatalogView` and needs no change to keep working, because the ranges were always the
catalog's to state.

| `RoomConfig` field | Type | Meaning |
| --- | --- | --- |
| `seats` | `number` | Seat count, validated into `2..=8` and against the format's own range |
| `game_setup` | `GameSetupId` | Opaque id naming the format the room builds its game from |
| `name` | `string?` | The host's chosen table name (issue #546); omitted when unnamed |
| `visibility` | `RoomVisibility?` | `public` (default, omitted) or `private` (issue #546) |
| `undo_enabled` | `boolean?` | Whether any player at this table may take the last action back (issue #648); `false` by default and omitted at that default |

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

`undo_enabled` is a **table rule** (issue #648): chosen when the room is made, editable by
its host with `update_room` while the room is still gathering, and carried in every
`RoomView` and `RoomSummary` — so a player reads whether this table takes moves back before
they sit down. It defaults to `false` and elides at that default, so a client that never
learned the field creates exactly the table it always created. Changing it clears every
seat's readiness, exactly as changing the seat count does: nobody stays ready to a table
whose rules moved under them. It says only what the table *allows*; whether a rollback is
available at any moment is `GameView.undo`, and the `undo` message is how one is asked for.

Each seat contains:

- zero-based `seat` index;
- optional public `occupied_by` player id;
- optional public `name`, the occupant’s chosen display name (issue #294), omitted for
  an empty or unnamed seat;
- `decked`, indicating a validated deck was submitted;
- optional `colors`, the colour identity of that deck (CR 903.4) in WUBRG order, omitted
  for a seat that has submitted none;
- optional `commander`, the `CardIdentity` the seat designated (CR 903.3), omitted for a
  seat that designated none;
- `ready`; and
- optional `ai`, the id of the **AI opponent** kind filling the seat (issue #415), omitted
  for an empty or human seat.

Deck contents are private and never appear in another connection’s view. A seat’s `name`
is public and un-redacted; when it is absent a client falls back to a seat-derived label
(e.g. `"Player 2"`, using the real `seat` index — never by parsing the opaque id).

`colors` and `commander` are what a player **shows** the table, and both are server-derived
when a deck is accepted rather than sent by the client: `colors` is the union of the deck’s
cards’ colour identities, and `commander` is the identity the `submit_deck` designated. They
summarise a decklist without disclosing it — a seat is red and green, and which cards make it
so stays private. A commander is public for the same reason the physical card is: it begins
the game face up in the command zone (`Permanent.is_commander` marks the same card in play).
Both elide from the wire at their empty values, so an older client that never reads them sees
exactly what it saw before, and a client MUST NOT treat their absence as “no deck” — `decked`
remains the only statement about that.

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
{ "type": "submit_deck", "cards": ["lathliss_dragon_queen", "mountain"], "commander": "lathliss_dragon_queen" }
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

Readiness follows the change. Changing the **seat count**, the **undo rule** (issue #648),
or the **format** clears every
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
begins receiving `SpectatorView`s (below). Spectators are advertised to the directory as
`RoomSummary.spectators` (a count only).

**A spectator connection is one-way.** After the hand-off the server answers pings and notices a
close, and every text frame the client writes is ignored rather than decoded — there is no command
a spectator can send, `leave` included. Closing the socket is what ends the session.

**A spectator is not held open across a disconnect**, because it owns no seat to hold: the server
drops it from the room's roster the moment its socket goes. Reconnecting is therefore an ordinary
`hello` — which lands the connection back in the lobby — followed by a fresh `spectate_room` for
the same room, and the `SpectatorView` that answers it is a whole public game, so resuming and
joining mid-game are the same thing. A client that wants to resume must remember which room it was
watching; the server, by design, does not remember for it.

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
| `loyalty` | `string?` | Printed starting loyalty (planeswalkers only) |
| `keywords` | `string[]?` | Keyword abilities as lowercase wire names; omitted when empty |
| `card_types` | `CardType[]?` | The card's types, exactly as an in-game `CardView` states them |

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
- **Object ids name objects; `physical_card` names a card.** Every entity id is a handle on
  one object in one zone, and two of them are never the same object (CR 400.7).
  `physical_card` is the one field that crosses that boundary, and it crosses it only as far
  as CR 108.1 allows: which physical card two projections are of, and nothing about what
  either object carries. A client follows a card with it, addresses nothing with it, and
  joins by `name` or `functional_id` never.
