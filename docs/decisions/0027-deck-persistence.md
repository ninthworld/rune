# ADR 0027: Where saved decks live — device-local persistence with portable export

- Status: accepted
- Date: 2026-07-20
- Issue: #366

## Context

RUNE has no durable player identity. A session token identifies one seat in one
room for reconnect (`docs/protocol.md`, ADR 0012); it is not an account and does
not outlive the room. The only decks a player can use today are the two starter
lists bundled with the client (`clients/web/src/starter-decks.json`, generated
from the engine catalog). M6 promises deck construction *and* saved deck lists
(`docs/roadmap.md`). The deck builder (#368) lets a player assemble a legal list
within a session; #369 must let that list survive to the next session. Before
#369 can be built, one thing must be decided: **where a saved deck list is
stored, what identity anchors it, and what durability and privacy expectations
follow** — a deck list is the first player-authored data the project would keep.

Forces:

- **No accounts, by posture.** The server is deliberately account-free and
  credential-free (ADR 0012). Introducing named accounts (Option 2) brings
  authentication, credential handling, and operational burden far beyond the
  current server's scope and contradicts that posture.
- **Server statelessness.** The server holds no per-player state across rooms.
  Server-side storage keyed by a durable anonymous token (Option 1) is the
  smallest server surface that still lets decks follow a token between browsers,
  but it is still the project's first persistent player-data store — it adds a
  storage backend, a data-retention question, and a privacy surface (player-
  authored data the server now holds) that nothing else in the project needs
  yet.
- **A deck list is pre-game data, not game state.** The web client guide forbids
  `localStorage` *of game state* because the server is the source of truth for a
  view (`clients/web/AGENTS.md`). A saved deck list is neither game state nor
  load-bearing for any view: it is an input a player chooses *before* the
  `submit_deck` gate, exactly analogous to the device-local, player-selected art
  cache already accepted in ADR 0024. Persisting it on the device therefore
  needs an explicit carve-out from the `localStorage` rule, but breaks none of
  its intent.
- **Legality is unchanged.** Whatever stores a deck, saving never implies format
  legality. A saved list is validated only at submission time, by the room
  format, through the unchanged `submit_deck` gate (ADR 0013 §4). Storage and
  legality are independent concerns.
- **The directory promise.** The player directory already promises never to
  expose deck lists to other players before game start; any persistence choice
  must keep that true. Device-local storage keeps it trivially true — a saved
  deck never leaves the device until it is submitted into a room.

Options 1–4 from the issue: (1) server-side, durable anonymous token; (2)
server-side, named accounts; (3) client-local only; (4) hybrid — client-local by
default with a portable export format now, deferring any server persistence
until real demand.

## Decision

**Saved decks are a device-local, player-owned concern with a portable export
format. The server stays stateless; no durable player identity is introduced.**
This is Option 4 (the hybrid), starting at its client-local half.

Concretely, the rule the codebase follows:

1. **Storage is device-local.** A saved deck list persists in the player's
   browser (IndexedDB, alongside the ADR 0024 art cache), keyed by a
   player-chosen name. There is no server-side deck store and no account. Losing
   the device's storage loses the decks — the same durability contract as every
   other device preference.
2. **Portability is a first-class export/import format.** A saved deck can be
   exported to, and imported from, a small human-readable JSON document
   (schema-versioned, a list of `functional_id` + count plus a name). This is
   the *only* mechanism for moving a deck between devices or sharing a list by
   hand; the project ships no sync. The format is stable and documented so it
   survives client changes.
3. **This is a carve-out from the `localStorage`-of-game-state rule, written
   into `clients/web/AGENTS.md`.** Saved decks are pre-game, player-authored
   input, never load-bearing for a rendered view; the builder and every view
   must still reconstruct fully from server data with the deck store empty.
4. **Saving never implies legality.** A saved deck is validated only when
   submitted, by the room format, through the unchanged `submit_deck` gate. A
   deck saved under one format may be rejected by another without corrupting the
   saved copy.
5. **Server persistence is deferred, not foreclosed.** If real demand for
   cross-device durability appears, a future ADR may add server-side storage —
   Option 1's durable anonymous token is the most likely shape. The export
   format defined here is the migration path and keeps that door open without
   committing to it now.

## Consequences

**Easier.** #369 becomes a client-only change: a deck store in the browser plus
save/load/list/delete/export/import in the builder surface, with no protocol
change, no server storage, no new wire shapes, and no round-trip contract to
maintain. It ships behind the same `submit_deck` gate that already exists. The
directory promise stays trivially intact — a saved deck never reaches another
player before submission. It extends the device-local precedent ADR 0024 already
set, so there is one coherent story for "player data that lives on the device."

**Harder / given up.** Decks do not follow a player across devices or browsers
except by explicit export/import — there is no automatic sync and no recovery if
device storage is cleared. Players on shared or ephemeral browsers keep only the
bundled starters. Storage failures (private-mode, disabled storage, quota) must
degrade to the current bundled-starters experience rather than a broken screen —
#369 owns that fallback. We accept these limits deliberately: they are the price
of keeping the server stateless and account-free, and the export format is the
escape hatch for anyone who needs portability today.

**Scope this does not touch.** No accounts, authentication, or cross-device sync
(Options 2 and the durable-token half of Options 1/4 are explicitly deferred).
No deck sharing between players inside the app. No format-tagged folders or
metadata beyond a name. Those remain future decisions.
