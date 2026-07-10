# RUNE protocol

The entire client/server contract is two message types over WebSocket (or an
in-process call for WASM/FFI deployments). Any client — web UI, CLI, LLM agent —
speaks exactly this. **Changing any shape here requires updating `rune-protocol`
and this document in the same PR.**

## Server → client: GameView

Personalized per player (hidden information is redacted server-side; the client
never receives what its player may not know).

| Field | Notes |
|---|---|
| `my_hand` | Full card objects for the receiving player only |
| `opponents[]` | `player_id`, `hand_size`, `life`, zone counts, statuses |
| `battlefield[]` | Permanents with controller, owner, computed characteristics |
| `stack[]` | Spells and abilities; ability entries carry source + trigger text |
| `graveyards`, `exile` | Public ordered lists per player |
| `phase`, `priority_player` | Drives overview/focus mode client-side |
| `valid_actions[]` | See below — the only source of interactivity |
| `action_deadline` | Seconds remaining for the pending decision |

### valid_actions[]

```json
{ "id": "a2", "type": "activate_ability", "label": "Tap for mana",
  "subject": ["perm_xyz"] }
```

- `subject` lists the entity ids this action belongs to. Clients render
  entity-subject actions on the entity; subject-less actions (pass, end turn)
  go in the action bar (ADR 0004).
- Multi-step actions (targets, modes, X) will extend this with a `requirements`
  list consumed as a client-side prompt queue; answers are submitted atomically.
  Spec to be finalized alongside the first targeted spell (see backlog).

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
