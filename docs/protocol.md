# RUNE protocol

A RUNE connection speaks two phases over one WebSocket (or an in-process call for
WASM/FFI deployments). Before a game exists it speaks a **small lobby message
set** (`LobbyView` out, `LobbyCommand` in — full lobby state every message, same
philosophy as `GameView`); the instant a game is constructed it speaks **exactly
the two in-game message types** (`GameView` out, `ChooseAction` in) for the life
of that game. Any client — web UI, CLI, LLM agent — speaks exactly this.
**Changing any shape here requires updating `rune-protocol` and this document in
the same PR.**

> **On "the entire API is two messages."** The two *in-game* messages
> (`GameView` / `ChooseAction`) are still the whole contract for a game already in
> progress. They are now **flanked** by the lobby pair (`LobbyView` /
> `LobbyCommand`) that governs the pre-game phase and hands off to the in-game
> contract at game construction (docs/decisions/0012-lobby-protocol.md). Both
> pairs obey the same discipline: full personalized state pushed every message,
> the client reconstructs its whole UI from one message, and it computes no
> legality of its own.

## Server → client: GameView

Personalized per player (hidden information is redacted server-side; the client
never receives what its player may not know). The concrete types live in the
`rune-protocol` crate; the wire format is their serde JSON.

| Field | Type | Notes |
|---|---|---|
| `you` | `PlayerId` | The receiver's own seat entity id (same `p{N}` form used for players). Lets a client identify itself directly. A client that receives a payload without it (older server) treats it as `""`/unknown |
| `my_hand` | `CardView[]` | Full card objects for the receiving player only |
| `opponents` | `OpponentView[]` | `player_id`, `hand_size`, `life`, `library_size`, `graveyard_size`, `statuses` |
| `battlefield` | `Permanent[]` | Permanents with `controller`, `owner`, computed `card`, `tapped`, `attacking`, `blocking`, `damage`, `counters` (a `Counter[]`, see below) |
| `stack` | `StackItem[]` | Spells and abilities; ability entries carry `source` + display text |
| `graveyards`, `exile` | `ZonePile[]` | Public ordered lists per player |
| `phase` | `Phase` | Current turn step (snake_case enum); drives overview/focus mode |
| `mana_pool` | `string[]` | The receiving player's unspent mana as pip strings (e.g. `["{G}"]`); server-computed, display-only. Omitted when empty |
| `priority_player` | `PlayerId?` | Who holds priority now, if anyone |
| `valid_actions` | `ValidAction[]` | See below — the only source of interactivity |
| `action_deadline` | `number?` | Seconds remaining for the pending decision |
| `result` | `GameResult?` | The terminal outcome once the game is over (CR 104.2a). Omitted while the game is live; when present, `valid_actions` is empty. See [Game over](#game-over-result) |

Empty collections and absent optionals are omitted from the JSON; clients must
treat a missing field as its empty/`null` default.

### Game over: result

When the game ends (CR 104.2a), the view carries a `result` object and
`valid_actions` becomes empty; further actions the client submits are rejected and
the same final view is re-sent. While the game is live `result` is **omitted
entirely** (the empty-optional convention), so its mere presence signals game over.

```json
{ "winner": "p0", "losers": ["p1"], "reason": "decked" }
```

- `winner` — the surviving player's id (CR 104.2a). Omitted for a draw, where every
  remaining player lost at once (CR 104.4a).
- `losers` — the players who lost, in seat order.
- `reason` — why the game ended, a snake_case enum: `life_zero` (a player at 0 or
  less life, CR 704.5a), `decked` (a player attempted to draw from an empty library,
  CR 704.5c), or `concede` (a player conceded, CR 104.3a).

The losing conditions are unified server-side; a player may always **concede** — a
`concede` action is offered in `valid_actions` in every phase (CR 104.3a). The
client renders the result; it never decides a winner or terminality itself.

### Combat state on a Permanent

A permanent carries its combat declaration state (CR 508/509), server-computed
and display-only like every other characteristic:

- `attacking` — `true` while the permanent is a declared attacker this combat.
  Omitted (treated as `false`) when it is not attacking.
- `blocking` — the entity id of the attacker this permanent is blocking, when it
  is a declared blocker. Omitted (`null`) when it is not blocking. Several
  blockers may name the same attacker; a blocker names exactly one attacker.
- `damage` — combat damage marked on this permanent this turn (CR 120.3), an
  unsigned integer. This is the value the engine's lethal-damage state-based
  action compares against toughness (CR 704.5g); it accumulates during the
  combat-damage step (CR 510) and is cleared at cleanup (CR 514.2). Omitted
  (treated as `0`) when no damage is marked.

All three are omitted from the JSON in the common not-in-combat, undamaged case,
so a permanent outside combat keeps its terse wire shape. The client renders
these; it never derives combat legality or lethality (which creatures may attack
or block, and which die, is decided by the engine and surfaced through the view
and `valid_actions`).

### Counter

A permanent's `counters` is an array of `Counter` objects, each a named counter
and its quantity. Both fields are required:

```json
{ "kind": "+1/+1", "count": 2 }
```

- `kind` — the counter name as displayed, e.g. `"+1/+1"` or `"loyalty"`.
- `count` — how many of that counter are present (an unsigned integer).

The server computes these; clients display them verbatim and never derive them.
The whole array is omitted when a permanent has no counters.

### valid_actions[]

```json
{ "id": "a2", "type": "activate_ability", "label": "Tap for mana",
  "subject": ["perm_xyz"], "token": "h:00ab" }
```

- `subject` lists the entity ids this action belongs to. Clients render
  entity-subject actions on the entity; subject-less actions (pass, end turn)
  go in the action bar (ADR 0004).
- Entity ids are opaque strings and are **per physical instance**: two copies of
  the same printed card in a zone carry different card entity ids, so an action
  subject names exactly one copy. Clients treat these ids as opaque handles and
  never parse them (the `card_N`/`perm_N` forms are server-internal).
- `type` is a free-form string. Kinds emitted today: `pass_priority`
  (subject-less); `play_land` and `cast_spell` (subject = the hand card's entity
  id); `activate_ability` (subject = the source permanent's entity id);
  `declare_attackers` and `declare_blockers` (subject-less combat declarations,
  CR 508/509 — their multi-select candidate `requirements` are a follow-up, as the
  targeting `requirements` still are). Clients key off `type`/`subject`/`label`
  and tolerate unknown kinds.
- `token` is a **content-binding token** (ADR 0009): a server-issued value bound
  to this action's exact content (kind + subject + requirements). The client
  echoes it back verbatim in `ChooseAction`; the server recomputes it from the
  freshly regenerated action and rejects any answer whose token does not match.
  This stops a stale positional `id` (e.g. `a2`) from silently rebinding to a
  *different* action once decisions stop being strictly sequential. Specified as
  a hash/echo of the action content so the server stays stateless (it remembers
  no per-id secret; it recomputes). Opaque — clients never parse or derive it.
  Omitted when unbound (older server); an omitted/`""` token matches no real
  action and is safely rejected.

#### Multi-step actions: `requirements`

A targeted spell/ability (and later modes and X) carries an ordered
`requirements` list — one entry per choice slot the player must fill before the
action can be taken:

```json
{ "id": "a3", "type": "cast_spell", "label": "Cast Lightning Bolt",
  "subject": ["c3"], "token": "h:9f2c",
  "requirements": [
    { "slot": "t0", "prompt": "target creature or player",
      "candidates": ["perm_bear", "p1", "p2"] }
  ] }
```

- Each requirement is one target slot: `slot` (opaque id the answer keys back
  to), `prompt` (human-readable spec label to display), and `candidates` (the
  legal entity ids the server enumerated — the **only** choices the client may
  offer). The server computes legality; the client highlights exactly these
  candidates and derives nothing (ADR 0009 §Client).
- `candidates` is enumerated O(N) per slot, never the cartesian product of
  combinations across slots (ADR 0009 §Enumeration).
- The client walks `requirements` as a prompt queue and submits **all** answers
  in a single `ChooseAction` (see below) — never a stateful multi-message
  handshake. The effect IR that backs these actions is decided in ADR 0007.
- Absent/empty for a plain action that needs no sub-choice.

## Client → server: ChooseAction

For a plain, no-choice action the message is just the id (and, once servers
issue them, the action's `token`):

```json
{ "type": "choose_action", "action_id": "a2", "token": "h:00ab" }
```

For a multi-step action the client submits its whole selection atomically —
`token` plus one `targets` entry per `requirements` slot:

```json
{ "type": "choose_action", "action_id": "a3", "token": "h:9f2c",
  "targets": [ { "slot": "t0", "chosen": ["perm_bear"] } ] }
```

- `token` echoes the chosen action's `token` verbatim (content binding above).
- `targets[]` answers the action's requirement slots: `slot` matches a
  `requirements[].slot`, and `chosen` lists the selected entity ids for it (one
  for a single-target slot; the list generalizes to multi-select choices the
  model defers for now). Every id in `chosen` must be one of that slot's
  advertised `candidates`, or the server treats the action as a no-op.
- `token` and `targets` are omitted when empty, so the minimal message above
  stays valid. The server validates the id, verifies the token against the
  action it currently offers, and re-checks each chosen target against that
  slot's freshly computed legal set; anything else is rejected and the current
  GameView is re-sent.

## Lobby phase

Before a game exists the connection speaks the lobby pair. It is the pre-game
analogue of the in-game contract: the server pushes a full `LobbyView` on every
change and the client rebuilds its entire pre-game UI (identity, room, seat
roster, who is decked/ready) from that one message. `valid_commands` is the only
source of interactivity, exactly as `valid_actions` is in `GameView`; the client
computes no legality. Hidden information stays redacted server-side — a
`LobbyView` never leaks another seat's decklist, only that the seat is decked.
When every seat is simultaneously filled, decked, and ready the server constructs
the game and the connection begins receiving `GameView`s; there is no auto-start
and no game with empty decks. All lobby types live in `rune-protocol`; the wire
format is their serde JSON, and unknown fields are ignored (forward compat).

### Server → client: LobbyView

| Field | Type | Notes |
|---|---|---|
| `session` | `SessionToken` (string) | The connection's opaque session/reconnect token. The client stores it and echoes it on a later `hello`. Always present on the wire; treated as `""` if a payload omits it. **Private** — it is the client's own handle, distinct from the public `you` shown to other seats |
| `you` | `PlayerId` (string) | The connection's public player identity, used to match itself against a seat's `occupied_by`. `""` if absent |
| `room` | `RoomView?` | The room the connection is in, if any (see below). Omitted when not in a room |
| `valid_commands` | `string[]` | The lobby command kinds currently legal for this connection (e.g. `"create_room"`, `"join_room"`, `"submit_deck"`, `"ready"`, `"unready"`, `"leave"`). Free-form strings; clients render exactly these and tolerate unknown kinds. Omitted when empty |

#### RoomView

| Field | Type | Notes |
|---|---|---|
| `room_id` | `RoomId` (string) | Opaque room id, shared out-of-band to invite a second player |
| `config` | `RoomConfig` | The room's configuration (see below) |
| `seats` | `SeatView[]` | The seat roster, in seat order. Omitted when empty |

#### RoomConfig

| Field | Type | Notes |
|---|---|---|
| `seats` | `number` (u8) | Number of seats, validated server-side into the inclusive range `2..=8`. The lobby supports 2–8 seats even while the engine remains two-player |
| `game_setup` | `GameSetupId` (string) | Opaque game-setup id naming which setup (players, starting life, hand size, …) the room builds its game from. The catalogue is owned by ADR 0013; the server validates the id |

#### SeatView

| Field | Type | Notes |
|---|---|---|
| `seat` | `number` (u8) | Zero-based seat index within the room |
| `occupied_by` | `PlayerId?` | The player occupying this seat; omitted when the seat is empty |
| `decked` | `bool` | Whether this seat has submitted a server-validated deck. Contents are never exposed to other seats — only this flag. Omitted (false) when not decked |
| `ready` | `bool` | Whether this seat has declared itself ready. Omitted (false) when not ready |

```json
{ "session": "s:ab12", "you": "p1",
  "room": { "room_id": "r:7f3",
    "config": { "seats": 2, "game_setup": "standard_2p" },
    "seats": [
      { "seat": 0, "occupied_by": "p1", "decked": true, "ready": true },
      { "seat": 1, "occupied_by": "p2", "decked": true } ] },
  "valid_commands": ["submit_deck", "unready", "leave"] }
```

Empty collections and absent optionals are omitted from the JSON; clients treat a
missing field as its empty/`null` default.

### Client → server: LobbyCommand

A single tagged message the client sends to act, structurally parallel to
`choose_action`. The server validates every command against authoritative state
and answers with a fresh `LobbyView`; an invalid command is rejected and the
current `LobbyView` re-sent. The `type` discriminator selects the command:

| `type` | Fields | Notes |
|---|---|---|
| `hello` | `token?` (`SessionToken`) | First contact or reconnect. Carries a previously issued session token to reclaim a held-open seat, echoed verbatim; omitted on a fresh connection (server then issues a new identity) |
| `create_room` | `config` (`RoomConfig`) | Create a new room; the reply's `RoomView` carries the freshly issued `room_id` |
| `join_room` | `room_id` (`RoomId`) | Join an existing room by id. No matchmaking or discovery — the id must have been shared out-of-band |
| `submit_deck` | `cards` (`CardIdentity[]`) | Submit a decklist as a flat list of opaque card-identity handles (a card appearing multiple times is repeated). The server validates it authoritatively against the card database; `cards` is omitted when empty |
| `ready` | `ready` (`bool`) | Declare (`true`) or retract (`false`) readiness. A seat may ready only once it is occupied and decked |
| `leave` | — | Leave the current room, vacating the seat |

```json
{ "type": "hello", "token": "s:ab12" }
{ "type": "create_room", "config": { "seats": 2, "game_setup": "standard_2p" } }
{ "type": "join_room", "room_id": "r:7f3" }
{ "type": "submit_deck", "cards": ["ci_bear", "ci_bear", "ci_forest"] }
{ "type": "ready", "ready": true }
{ "type": "leave" }
```

Card identities and the `game_setup` catalogue are opaque here; their
identity-vs-printing model and values are owned by ADR 0013. Per the project's
legal rules these are card *identities*, never printings, images, or WotC
branding.

## Invariants

- The client is stateless with respect to rules: a fresh `GameView` (in game) or
  `LobbyView` (pre-game) must fully reconstruct the UI — reconnect, spectate, and
  resync all depend on this.
- `valid_actions` (in game) and `valid_commands` (lobby) are the only sources of
  interactivity; the client renders exactly what it is given and computes no
  legality of its own.
- Displayed values (P/T, counters, costs) are server-computed; clients never derive.
- Unknown fields must be ignored by clients (forward compatibility).
