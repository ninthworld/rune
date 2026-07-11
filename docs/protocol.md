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
  "subject": ["perm_xyz"] }
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
- Multi-step actions (targets, modes, X) will extend this with a `requirements`
  list consumed as a client-side prompt queue; answers are submitted atomically.
  Spec to be finalized alongside the first targeted spell (see backlog). The
  effect IR that backs these actions is decided in ADR 0007.

## Client → server: ChooseAction

```json
{ "type": "choose_action", "action_id": "a2" }
```

That is the entire message. The server validates against the actions it issued;
anything else is rejected and the current GameView is re-sent.

## Invariants

- The client is stateless with respect to rules: a fresh GameView must fully
  reconstruct the UI (reconnect, spectate, resync all depend on this).
- Displayed values (P/T, counters, costs) are server-computed; clients never derive.
- Unknown fields must be ignored by clients (forward compatibility).
