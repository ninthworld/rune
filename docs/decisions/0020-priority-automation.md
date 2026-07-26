# ADR 0020: Basic priority automation — engine hint, server policy, per-phase stops

- Status: accepted
- Date: 2026-07-17
- Issue: #264

## Context

Every priority pass is manual today. The engine offers `pass_priority` as an
ordinary [`Action`](../../crates/rune-engine/src/actions.rs) and nothing anywhere
auto-passes, holds, or stops — a spell-less turn still costs a seat a click at
every step it is handed priority. `docs/design/ui-requirements.md` ("Stack,
priority, and timers") calls priority automation the single biggest lever on game
pace and fixes two hard constraints on where it may live:

- **The client cannot decide "no meaningful response."** Deciding that a lone
  `pass_priority` is safe *is* a rules judgment — it depends on the seat's
  `valid_actions`, the stack, and timing. The client renders `valid_actions` and
  computes no legality (AGENTS.md), so it cannot even auto-fire a solitary pass.
  Automation is engine/server side, behind a server contract.
- **The engine does no I/O and holds no policy.** ADR 0002 keeps
  `rune-engine` a pure function over immutable `GameState` — no loops driven by
  wall-clock, no rooms, no per-connection configuration. A *loop* that keeps
  auto-passing, and the *preferences* that gate it, are room-layer concerns, the
  same way the decision timer (issue #263, [`TimerPolicy`](../../crates/rune-server/src/room.rs))
  and player display names (issue #294) live in the room, not the engine.

Per-player **stop preferences** are input like any other: they reach the server
over the protocol and must survive reconnect, so they cannot live only in client
memory. The room already holds exactly this shape of non-engine, per-seat,
reconnect-surviving state (`player_names`), and reconnect re-sends full state, so
a preference stored on the room is durable by construction.

This is the *basic* tier (M4). The M6 automation suite — auto-yield, hold
priority, "full control" — and any timer interaction beyond not conflicting are
out of scope.

## Decision

Automation is split across the two layers along their existing seam: the **engine**
answers the one rules question ("does this seat have a meaningful action?"), and the
**server** owns the policy loop, the per-seat stop preferences, and the wire.

### Engine — the "no meaningful action" hint

- **A single pure predicate,
  [`priority_has_no_meaningful_action`](../../crates/rune-engine/src/automation.rs).**
  It returns `true` exactly when the current priority holder's *entire*
  `valid_actions` set is drawn from {`PassPriority`, `Concede`, a mana ability}.
  Casting a spell, activating a non-mana ability, and every forced turn-based
  choice (a combat declaration, the cleanup discard, a mulligan decision) make it
  `false`. It is `false` when no one holds priority or the game is over.
- **Mana abilities do not count as meaningful.** Activating a mana ability with
  nothing to spend the mana on floats mana that empties again — it accomplishes
  nothing on its own — so a seat holding only lands is idle. This is what lets the
  common case (a seat with untapped lands and no play) auto-pass at all.
- **A window with no pass on offer is never idle.** Forced choices are advertised
  *without* `PassPriority` (combat declarations, cleanup discard, the mulligan
  decision), so the predicate short-circuits to `false` there. This is the
  structural guarantee behind the safety property below: the predicate can only be
  `true` when passing is a legal move the seat is already entitled to take.
- The predicate reads nothing but `state` and the card database, so it is
  deterministic and replay-stable — automation changes *who clicks pass*, never
  *what state a pass produces* (a pass is still `apply_action(PassPriority)`).

### Server — the auto-pass loop and stop preferences

- **A room policy, [`AutoPassPolicy`](../../crates/rune-server/src/room.rs), off by
  default.** Mirroring `TimerPolicy`, `Off` reproduces exactly the pre-automation
  behavior (every existing flow and test is unchanged); the server binary enables
  `On` for real games. Determinism holds with it on or off.
- **Per-seat stop preferences live on the room**, a set of `Phase`s at which that
  seat wants priority *even when idle*, defaulting to **empty** (stop nowhere).
  They are set over the protocol (`set_stops`) and held on the room exactly as
  `player_names` are, so they survive a disconnect/reconnect with no extra
  machinery. The default is empty because the safety property already guarantees a
  seat is only ever auto-passed when passing is its sole meaningful move — so an
  empty default never skips a real decision; a stop is the *opt-in* escape hatch.
- **The settle loop.** After every applied action (and at room start, on a
  timeout's default action, and after a stops change), the room repeatedly applies
  `PassPriority` on the current holder's behalf while, for that seat,
  `priority_has_no_meaningful_action` holds **and** the seat's stops do not name the
  current step. The loop halts the moment a seat has a meaningful action, owes a
  forced choice, or has opted to stop — and, as a defence against a future
  non-terminating configuration, after a fixed cap (logged), never hanging the room
  task. It terminates naturally every turn regardless: the active player's
  declare-attackers step is a forced choice (no pass on offer), so the loop always
  stops there at the latest.
- **The escape hatch from an auto-pass chain.** The classic trap — configuring
  auto-pass and then wanting to act at end of turn — is resolved by adding the
  target step (e.g. `end`) to the seat's stops: the loop then hands that seat
  priority at that step even while idle. A seat with an instant-speed play or a
  relevant activated ability never needs this — it is non-idle and stops
  automatically.

### Protocol — preferences up, an indicator down

- **`set_stops` (client → server):** a new in-game
  [`ClientMessage`](../../crates/rune-protocol/src/lib.rs) variant carrying the
  seat's stop `Phase`s. Server-authoritative and reconnect-durable (stored on the
  room); an unparseable message is ignored and the current view re-sent, the
  non-fatal pattern.
- **`GameView.stops` (server → client):** the receiver's own current stop phases,
  so the stops UI is reconstructable from one view (reconnect-safe). Omitted when
  empty, defaulted to empty by older clients — the optional-field convention.
- **`GameView.auto_passed` (server → client):** a display-only flag set on the
  broadcast that follows a settle which auto-passed *this* seat, so the client can
  show a transient "passed for you" indicator. Advisory, not load-bearing: the UI
  reconstructs fully without it.

### Client — display only

- Per-step stop toggles on the phase indicator's expanded step list (which already
  reserves a stable per-step handle), reading `view.stops` and answering with
  `set_stops`. A visible indicator when `view.auto_passed`. The client computes no
  legality and decides no "no meaningful response" — it renders the server's stops
  and echoes toggles back.

## Consequences

**Easier:** a spell-less turn collapses from a click per step to none; the reduction
is asserted by a server test that counts pass prompts. The engine gains one small,
pure, well-tested predicate that later automation tiers (auto-yield, hold priority)
can build on. Stops ride the same room-state/reconnect machinery as names and
timers, so nothing new is needed to make them durable.

**Harder / given up:** the room now advances state between client messages (the
settle loop), so a view can reflect several steps of progress at once; the
`auto_passed` flag exists to make that legible. The auto-pass predicate is
deliberately conservative (empty stack of meaningful actions only) — it does not
try to auto-pass a seat that *could* respond but obviously would not, which stays a
later, richer-policy concern.

**Deferred:** auto-yield, hold-priority, and "full control" mode (M6); any coupling
with the decision timer beyond the two not conflicting; a smarter idle predicate
that reasons about whether a *possible* response is *worth* making.

## Follow-up

- **2026-07-26 — automation also resolves *choiceless* forced declarations (#453).**
  The Decision above rests on "a window with no pass on offer is never idle", and
  that was load-bearing in both directions: it kept a seat from being auto-passed
  out of a choice it owed, but it also stopped the settle dead on a
  declare-attackers step for a player who controls no legal attacker at all — an
  empty prompt with exactly one possible answer. The split stands; the engine gains
  a second pure predicate beside the first,
  [`forced_declaration_without_choice`](../../crates/rune-engine/src/automation.rs),
  which returns the empty `DeclareAttackers`/`DeclareBlockers` **only** when it can
  prove no non-empty declaration is legal (CR 508.1a / 509.1a, evasion included).
  The room's settle loop applies it as the ordinary engine action it is, behind the
  same `AutoPassPolicy` and the same per-seat stops, so `Off` still reproduces
  pre-automation behavior bit for bit and determinism is untouched. One consequence
  is worth recording: the loop's natural per-turn terminator ("the declare-attackers
  step is a forced choice") no longer holds on a board where neither seat can ever
  act, so `MAX_AUTO_PASSES` — previously a pure bug-detector — is now also the
  ordinary stopping point for a game with genuinely nothing left to do.
- **2026-07-26 — human seats stop at their own main phases by default, and a settle
  says where it acted (#455).** The Decision above chose an **empty** default stop
  set, reasoning that automation only ever passes a seat whose sole meaningful move
  is a pass, so an empty default can never skip a real decision. That argument is
  correct about *decisions* and false about *comprehension*, which is what the #455
  playtest records: a human whose own turn holds nothing castable watches the settle
  run both main phases and the whole turn between two broadcasts, and reports
  believing they are still on the previous turn. Nothing was decided for them and
  they still lost the turn. Two changes follow, neither of which touches the engine.

  **A default, not a rule.** The room gains a third policy beside `TimerPolicy` and
  `AutoPassPolicy` — [`StopPolicy`](../../crates/rune-server/src/room/policy.rs),
  **off by default**, turned on by the lobby for real games — which *seeds* a seat
  that has never sent `set_stops`. A human seat is seeded with its own main phases;
  an AI seat (the room's existing `ai_seats` knowledge) is seeded with nothing, so
  AI-only, mixed, and headless games keep exactly the throughput they had and
  `AutoPassPolicy::Off` still reproduces pre-automation behavior bit for bit. The
  seed is retired for good by the first `set_stops` a seat sends, which is why the
  room now distinguishes "never asked" from "asked for nothing": without that
  distinction a player could not clear a default — they would send the empty set and
  be handed the seed straight back.

  **Stops gained a scope.** "Your own main phase" is not expressible in the original
  per-step set, and seeding the unscoped one would hand a player priority in every
  *opponent* main phase too — reintroducing the per-step click this ADR removed, for
  a window where they have nothing to do they could not already do at instant speed.
  So the preference is two lists: `stops` (any turn, unchanged) and `own_turn_stops`
  (only while the seat is the active player), with the wider claim winning where a
  step is on both. Both ride `set_stops` and both are reflected in the view as the
  *effective* sets, seeds included, so the stops UI still rebuilds from one message.

  **The settle now reports its path.** `auto_passed` was one boolean for a whole
  settle and never named the steps it covered — enough to say "you were skipped",
  never enough to say what you missed, which is the second half of what #455
  reports. `GameView.auto_passed_steps` carries the steps the room acted at *for that
  receiver*, in order, consecutive duplicates collapsed; `auto_passed` is now exactly
  that list being non-empty. It is a path, not a set: a settle crossing a turn
  boundary may name a step twice. It stays advisory and display-only — the
  authoritative record of what happened inside a settle is the ADR 0021 log window,
  which the view already carries, so a spell that resolved and a creature that died
  while the receiver held no priority are recoverable from the same single message.

- **client polish:** richer auto-pass affordances (a log entry, a per-step "you
  were skipped here" marker) beyond the basic indicator — the per-step marker landed
  with #455 above, reading `auto_passed_steps`; the log entry folds into the game-log
  work (#260).
- **M6 — expanded automation:** auto-yield and hold-priority, which generalize the
  stop set into a full per-step yield/stop/act matrix; this ADR's engine predicate
  and room loop are the substrate.
