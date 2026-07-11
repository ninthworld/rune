# RUNE protocol

The entire client/server contract is two message types over WebSocket (or an
in-process call for WASM/FFI deployments). Any client — web UI, CLI, LLM agent —
speaks exactly this. **Changing any shape here requires updating `rune-protocol`
and this document in the same PR.**

## Server → client: GameView

Personalized per player (hidden information is redacted server-side; the client
never receives what its player may not know). The concrete types live in the
`rune-protocol` crate; the wire format is their serde JSON.

| Field | Type | Notes |
|---|---|---|
| `you` | `PlayerId` | The receiver's own seat entity id (same `p{N}` form used for players). Lets a client identify itself directly. A client that receives a payload without it (older server) treats it as `""`/unknown |
| `my_hand` | `CardView[]` | Full card objects for the receiving player only |
| `opponents` | `OpponentView[]` | `player_id`, `hand_size`, `life`, `library_size`, `graveyard_size`, `statuses` |
| `battlefield` | `Permanent[]` | Permanents with `controller`, `owner`, computed `card`, `tapped`, `counters` (a `Counter[]`, see below) |
| `stack` | `StackItem[]` | Spells and abilities; ability entries carry `source` + display text |
| `graveyards`, `exile` | `ZonePile[]` | Public ordered lists per player |
| `phase` | `Phase` | Current turn step (snake_case enum); drives overview/focus mode |
| `mana_pool` | `string[]` | The receiving player's unspent mana as pip strings (e.g. `["{G}"]`); server-computed, display-only. Omitted when empty |
| `priority_player` | `PlayerId?` | Who holds priority now, if anyone |
| `valid_actions` | `ValidAction[]` | See below — the only source of interactivity |
| `action_deadline` | `number?` | Seconds remaining for the pending decision |

Empty collections and absent optionals are omitted from the JSON; clients must
treat a missing field as its empty/`null` default.

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
  id); `activate_ability` (subject = the source permanent's entity id). Clients
  key off `type`/`subject`/`label` and tolerate unknown kinds.
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

## Invariants

- The client is stateless with respect to rules: a fresh GameView must fully
  reconstruct the UI (reconnect, spectate, resync all depend on this).
- Displayed values (P/T, counters, costs) are server-computed; clients never derive.
- Unknown fields must be ignored by clients (forward compatibility).
