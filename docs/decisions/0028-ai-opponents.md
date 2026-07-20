# ADR 0028: AI opponents as server-driven seat occupants

- Status: accepted
- Date: 2026-07-20
- Issue: #415

## Context

A lobby seat could only be filled by a human WebSocket connection. Solo play, filling out
a free-for-all table, and offline-style practice all require an opponent that is not a
person. `docs/brief.md` names "AI opponents working" as a development milestone, and the
terminal client (`rune-cli`) already proves a non-interactive agent can play a game over
the protocol — but only as an *external* process driving its own socket, not as a seat a
host can add from inside a lobby.

Two hard rules constrain where AI logic may live: the engine performs **no I/O and holds
its only randomness in an injected seed** (ADR 0002, ADR 0014), and the **client computes
no game logic** — interactivity comes solely from server-advertised `valid_commands` /
`valid_actions` (ADR 0012). An AI opponent must not violate either: it cannot be a rules
authority in the engine, and it cannot be a client-side bot.

We also want the first cut to be simple (essentially random legal play) without painting
the architecture into a corner — a stronger AI should drop in later without reworking the
lobby, protocol, room, or client.

## Decision

An AI opponent is a **new kind of seat occupant, driven entirely server-side**, layered
over the existing room/protocol machinery rather than threaded through the engine.

1. **Seat model.** A room seat is human, AI, or empty. An AI seat carries no session; the
   lobby tracks it alongside the human-occupant vector and stores its host-chosen deck in
   the same per-seat gate a human uses, so an AI seat is *decked + ready by construction*
   and satisfies the unchanged ready gate. AI seats count as filled for occupancy and are
   never offered to a human joiner.

2. **Host control over the wire.** Two host-only lobby commands, `add_ai { seat, kind,
   cards, commander? }` and `remove_ai { seat }`, fill and clear AI seats pre-game. "Host"
   is the seat 0 occupant, decided by the **server**, which advertises the commands in that
   connection's `valid_commands` only when legal. `SeatView.ai` reports the seat's AI kind,
   and `CatalogView.ai_opponents` advertises the seatable kinds. The client renders the
   affordance purely from `valid_commands` and the catalog — it never infers host-ness or
   hardcodes the kind set. These are contract changes made in the Rust types, the
   TypeScript mirror, and `docs/protocol.md` together.

3. **View-level policy seam.** An `AiPolicy` maps a `GameView` to a `ChooseAction` — the
   *same* protocol path a human takes. A server-side driver task, `serve_ai_seat`, is the
   in-process sibling of `serve_connection`: it joins the room, reacts to the seat's pushed
   `GameView`s, and sends the policy's chosen action back as an ordinary `RoomInput`. The
   room re-validates every answer through its existing `resolve_action`, so the AI can only
   ever take an action the engine already offered. AI decision-making lives in the server
   (like the existing auto-pass and decision-timeout defaults), never in the engine or the
   client.

4. **Simple first policy, deterministic.** The one shipped kind, `random`, plays a
   uniformly random *legal* action each decision, filling slots from the server's own
   candidate sets (a random subset of attackers, a random legal target, no blocks, keep at
   the mulligan) so it never stalls a room. It draws from a seeded PRNG, preserving the
   engine's seed-only determinism (ADR 0014) — a pinned game seed replays the AI exactly —
   with no new dependency. A stronger policy is a new `AiPolicy` implementation behind a new
   `AiKind`; nothing else changes.

## Consequences

- **Any seat count.** A room may mix human and AI seats — one human versus three AI in a
  free-for-all, or a two-player game against one AI — because each AI seat is an
  independent driver. No special two-player path.
- **Reuses tested machinery.** The AI plays through the room's existing view projection,
  action resolution, and validation, so it inherits redaction, legality, and combat/target
  handling for free, and cannot desync the game.
- **Engine and client stay pure.** No game logic entered the engine or the client; the
  additive protocol fields are default-elided, so older clients and non-AI rooms are
  byte-for-byte unaffected.
- **Groundwork, not a finished AI.** The `random` policy is intentionally weak — a sparring
  partner. The value delivered now is the seam (`AiPolicy` + `AiKind` + the wire), so
  investing in a smarter opponent later is a localized change.
- **Deferred.** AI difficulty settings, per-AI names/personalities, and hot-swapping a
  disconnected human for an AI mid-game are out of scope; an AI seat lives for the game.
