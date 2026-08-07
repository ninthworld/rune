# Scenarios: playing an exact position

A **scenario** is a small file describing one game position. `sage-scenario` reads it, builds a
real `GameState`, serves it on a loopback socket, and points the built web client at it:

```bash
cargo run -p sage-scenario -- scenarios/murder-the-dreadmaw.toml
# Ready: http://127.0.0.1:4173/?server=ws://127.0.0.1:9010
```

Open the URL, play the position through the shipping UI, and press Ctrl-C when you are done. The
game is gone; nothing persists.

`make scenario SCENARIO=path/to.toml` does the same thing and builds the client first.

## What is real

Everything except how the first state was reached.

- The **engine** generates the legal actions and applies them. A click that is not a legal play is
  not offered, and would be rejected if it were sent.
- The **server** projects each seat's view, redacts hidden zones, binds actions, paces the game,
  and drives the AI seat — through `Room` and `serve_ai_seat`, the same code a lobby game uses.
- The **client** is the built bundle, unmodified. It renders a `GameView` and returns an
  advertised `action_id`, exactly as it does anywhere else.

This is the difference between a scenario and the fixture tier under `clients/web/e2e`: a fixture
replays a `GameView` over an intercepted socket, so a click there proves rendering and nothing
about the rules. Here, a click is a rules event.

## What is not

- **No deck, no format, no legality.** A scenario is a position; a library is whatever the file
  says, in the order it says, unshuffled. Deck rules exist to judge decklists for games that start
  at turn one.
- **No mulligan and no opening hand.** The game is already in progress.
- **No production surface.** This adds no protocol command; nothing on the wire can inject
  authoritative state. The runner refuses to bind an address that is not loopback, and no shipped
  crate depends on `sage-scenario`.

## Writing one

A scenario is TOML. Every key is optional except `players`, and an unrecognised key is an error
naming the key rather than a setting that silently did nothing.

```toml
name = "Murder the Dreadmaw"          # a label, shown when the runner starts
note = "Seat 0 holds Murder…"         # a line about what the position is for

seed = 777                            # every draw in the run comes from this (default 0)
turn = 6                              # 1-based (default 3)
step = "precombat_main"               # default
active_player = 0                     # seat index (default 0)
priority = 0                          # seat index (default: the active player)

[[players]]                           # seat 0 — the seat the browser is handed
name = "You"
life = 14                             # default 20
hand = ["murder", "swamp"]
library = ["swamp", "forest"]         # TOP CARD FIRST — the next card drawn is the first entry
graveyard = ["shock"]
exile = []
command = []
mana = { black = 1 }                  # white / blue / black / red / green / colorless

[[players.battlefield]]
card = "swamp"

[[players]]                           # seat 1 — everything but seat 0 wants an AI
name = "Sparring partner"
ai = "random"

[[players.battlefield]]
card = "colossal_dreadmaw"
damage = 2
```

**Cards are named by `functional_id`** — the file's basename under
`crates/sage-engine/data/catalog/`, and the same identity a decklist and the wire use. A `CardId`
is interned from the catalog's sort order and would silently come to mean a different card the
moment one was authored ahead of it, so a file can never name one.

`scenarios/murder-the-dreadmaw.toml` is the worked example, and is the file the browser and Rust
tests both run.

### Seats

Seat `n` is `PlayerId(n)` in seating order. **Seat 0 is the one the browser connects to**; every
other seat should name an `ai`, or nobody will ever act for it. The kinds are the server's own
(`random` today); an unknown one is refused with the list of kinds that would have worked.

### The turn

`turn`, `step`, `active_player`, and `priority` place the position in the turn structure. Each
seat's *most recently begun turn* is derived from `active_player` and `turn` by walking backwards
through the seating order — the active seat's is the current turn, the seat before it began the
turn before, and so on. That is what summoning sickness is measured against (CR 302.6), so a
position wanting established permanents wants a `turn` past the first: at turn 1, seat 1 has never
begun a turn, and a permanent it controls cannot have lost summoning sickness. The runner says so
rather than quietly handing back a sick creature.

### Zones

`hand`, `library`, `graveyard`, `exile`, and `command` are lists of card names. **The library is
listed top card first**, because that is how a person describes one; everything else is in the
order it sits in.

### Permanents

Each `[[players.battlefield]]` entry is one permanent, **owned** by the seat it is listed under —
which is where the card goes when it leaves the battlefield (CR 400.7).

| Key | Meaning |
| --- | --- |
| `card` | the `functional_id` (required) |
| `label` | a name *this file* uses to point at this permanent, for `attached_to` |
| `controller` | the seat that controls it, when that is not the seat that owns it |
| `tapped` | default `false` |
| `summoning_sick` | it came under its controller's control this turn (default `false`) |
| `damage` | damage marked on it this turn (CR 120.3) |
| `counters` | a table of counter kind → count |
| `attached_to` | the `label` of the permanent it is attached to (an Aura's host, an Equipment's creature) |
| `face` | `"front"` (default) or `"back"`, for a permanent that has transformed |

Counter kinds are the engine's own names: `plus_one_plus_one`, `minus_one_minus_one`, `loyalty`,
`charge`, `gold`, `wish`, `corpse`, `phylactery`.

**`controller` is not a written-down field.** A permanent controlled by someone other than its
owner gets a CR 613 layer 2 control-changing effect, which is how the engine models `Act of
Treason` — so every rule that asks who controls it reads the same computed answer, and a stolen
creature that dies still goes home.

**Labels are not ids.** Object and physical-card identities are minted by the builder from the
engine's own monotonic counter; a label is a name for the file's own use and never crosses the
wire. A file with no attachments needs no labels.

## What a scenario cannot express yet

Deliberately, and to be extended only when a real mechanic needs it:

- stack objects, and anything mid-resolution (a pending choice, a suspended spell);
- combat declarations — attackers, blockers, damage-assignment orders;
- continuous effects other than the control change `controller` produces, and delayed or
  reflexive triggers;
- a designated commander, commander tax, or commander damage;
- tokens and emblems;
- restricted mana (CR 106.6), and per-turn allowances (`land_played`, loyalty activations).

A position needing one of these is the reason to add it, and the schema is where it goes.

## What is validated, and when

Everything, before a socket is bound. The runner reads the file, builds the position in memory,
and refuses the whole thing rather than opening a browser onto a game that cannot move. In order:

1. the file parses, and every key is one the schema has;
2. the table has at least two seats, `turn` is not zero, and every seat index a field names exists;
3. every AI kind resolves;
4. every card name resolves, reported with the seat and the zone it was written in;
5. labels are unique, every `attached_to` resolves, and nothing is attached to itself;
6. no permanent is established under a seat that has not begun a turn;
7. nobody starts at zero life (CR 704.5a — that game ends on the first check, before anyone acts);
8. the engine offers the seat holding priority at least one legal action.

Step 8 is the one the others cannot make: it asks the engine, through the same `valid_actions` the
room will call, whether this is a position anyone can play from.

## Determinism

`seed` is the only source of randomness in the run: the engine's own stream (ADR 0006) and each AI
seat's policy, which is seeded per seat from it exactly as a lobby game's is. The same file with
the same seed replays identically — the same board, the same ids, the same AI decisions.

## Options

| Flag | Default | |
| --- | --- | --- |
| `--addr <host:port>` | `127.0.0.1:9010` | the loopback address the game socket binds |
| `--client-addr <host:port>` | `127.0.0.1:4173` | where the built client is served |
| `--client-dir <path>` | alongside the crate | where `clients/web` is |
| `--no-client` | off | serve the game only, and print the URL for a client you are running |

The game socket must be loopback. The runner serves an authoritative position built from a local
file and refuses to be reachable from anywhere else.

## Testing

- `cargo test -p sage-scenario` — the schema, the builder's diagnostics, and `tests/live.rs`,
  which drives the runner over a real socket and checks the *authoritative* position moved. Part
  of `make check`, so a broken runner is caught without a browser.
- `make e2e-scenario` — the same path through the shipping client
  (`clients/web/e2e/scenario.spec.ts`). Its own non-blocking tier: a contributor tool breaking
  should be noticed, not block a merge (ADR 0011).
