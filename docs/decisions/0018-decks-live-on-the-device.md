# ADR 0018: Decks live on the device, and travel as files

- Status: accepted
- Date: 2026-08-04

## Context

A player needs somewhere to keep a deck between sessions. The protocol has never had one:
`submit_deck` carries a flat list of card identities plus an optional commander, the server
validates it against the room's format, and nothing about that deck outlives the room. Before this
work the client matched that shape exactly — the only decks that existed were the bundled starters
(`data/starter-decks.json`), and anything a player built by hand was gone on reload.

Closing that gap has an obvious shape and a tempting one. The obvious shape is a **deck store on
the server**: accounts, saved decks, decks synced across devices. The tempting part is that it
looks like one small command; the part that is not small is everything it drags in — identity,
ownership, storage, migration, and a second thing the server is authoritative about. SAGE has no
accounts and deliberately no monetisation path (`docs/brief.md`, Legal constraints); a session is
an opaque token that survives a reconnect and nothing more (ADR 0012's lobby protocol). Adding
per-player durable state would be the first thing in the project to require knowing *who* a player
is across sessions.

There is also an existing answer in the world. Every desktop Magic client reads and writes plain
decklist files, and players already keep decks that way, in text they can read, diff, and paste.

The deck editor (#705, `docs/client-design.md` §9.7) forced the question: a screen for building
decks is not worth much if what it builds cannot be kept.

## Decision

**A deck is device-local state and a file format, never server state.**

1. **Kept on the device.** Saved decks live in this browser's storage under one key, in the manner
   of ADR 0012's art preference and `connect.ts`'s remembered server: written by this device,
   never sent anywhere, and **absent or unreadable is always a working answer**. A player with
   storage switched off builds decks and loses them on reload; the screen still opens.

2. **Interchanged as `.dck` files.** Load and save go through a plain-text decklist — `[Main]`,
   `[Sideboard]`, `[Commander]` sections of `4 Lightning Bolt|LEA` lines. The reader is tolerant:
   unknown section headers and unreadable lines are skipped, `//` and `#` are comments, and a bare
   list with no header is the main deck. That is what makes a deck portable between devices without
   the server learning what a deck is.

3. **Names join to identities in the client, and misses are reported.** A file names cards; this
   client addresses `functional_id`s. The join is by name, case-insensitively, against the catalog
   the server sent. **A name the catalog does not hold is reported to the player by name**, never
   dropped — a deck that came back four cards short without saying so is the worse failure. The set
   code after the bar is read and kept out of the matching, because `CatalogCard` carries no
   printings; nothing is written back into that position that the server did not state.

4. **A sideboard is a device-local note.** The wire has no sideboard and this ADR does not add one.
   The builder holds it, storage keeps it, files carry it, and `submit_deck` leaves it out — and the
   surface says so where a player would otherwise find out by its absence.

5. **What a seat shows the table is the server's, not the device's.** The one deck-derived thing
   other players see — a seat's colours and its commander (`docs/protocol.md`, `SeatView`) — is
   computed by the server when it accepts a deck, from the resolved list. A client never publishes
   a claim about its own deck, and never reads another seat's from anything but the view.

Rules the codebase follows:

- Deck storage is injectable (`savedDecks(storage)`), so every test runs against a fake and none
  touches a real browser store. Failure modes — storage absent, key holding junk, quota exceeded —
  return "no decks kept" rather than throwing.
- The file layer (`dck.ts`) is pure: parse, resolve against a catalog, format. It decides no
  legality, and the round trip is a test.
- The client still computes no legality anywhere in this path. A deck loaded from a file is an
  *input* to `submit_deck`, and the verdict stays the server's `LobbyRejection`.

## Consequences

- A player can build a deck, keep it, and bring it back — on that device, in that browser. Moving
  to another device means exporting a file, which is the same thing every desktop client asks.
- Clearing browser storage loses saved decks. That is stated where it matters rather than
  engineered around; the file export is the durable copy.
- The server gains nothing to store, migrate, or authorise, and the project stays account-free.
- The `.dck` reader is deliberately permissive, so a file exported from another tool usually loads
  with a list of the cards SAGE does not have. That list is the honest output of a catalog with 134
  cards in it, and it will shrink as the catalog grows rather than needing a format change.
- Two things are now possible that were not: a deck can name cards this build has never heard of
  (they are reported), and a deck can carry a sideboard the server will never see (it is stated).
  Both are visible to the player at the moment they matter.
