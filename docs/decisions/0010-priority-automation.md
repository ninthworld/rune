# ADR 0010: Priority automation — engine predicate, server policy

- Status: accepted
- Date: 2026-07-30

## Context

Priority automation is the single biggest lever on game pace. Without it, a spell-less turn
costs a seat a click at every step it is handed priority, and a game of Magic becomes a game
of pressing pass.

Two constraints fix where the automation may live:

- **The client cannot decide "no meaningful response."** Judging that a lone `pass_priority`
  is safe *is* a rules judgment: it depends on the seat's legal actions, the stack, and
  timing. The client renders `valid_actions` and computes no legality, so it cannot even
  auto-fire a solitary pass. Automation is server-side, behind a contract the engine answers.
- **The engine does no I/O and holds no policy** (ADR 0001). A *loop* that keeps auto-passing,
  and the *preferences* that gate it, are room-layer concerns — the same way decision timers
  and display names live in the room, not the engine.

Per-player stop preferences are input like any other: they reach the server over the protocol
and must survive reconnect, so they cannot live only in client memory.

## Decision

Automation splits across the two layers along their existing seam. The **engine** answers pure
rules questions; the **server** owns the loop, the preferences, and the pacing.

### Engine — two pure predicates

- **`priority_has_no_meaningful_action`** returns `true` exactly when the current priority
  holder's *entire* legal-action set is drawn from {`PassPriority`, `Concede`, a mana
  ability}. Casting a spell, activating a non-mana ability, and every forced turn-based choice
  make it `false`, as does a state where nobody holds priority or the game is over.

  **Mana abilities do not count as meaningful.** Floating mana with nothing to spend it on
  accomplishes nothing, so a seat holding only lands is idle. This is what lets the common
  case auto-pass at all.

  **A window with no pass on offer is never idle.** Forced choices — combat declarations, the
  cleanup discard, the mulligan decision — are advertised *without* `PassPriority`, so the
  predicate short-circuits to `false`. That is the structural guarantee behind the safety
  property: the predicate can only be `true` when passing is a legal move the seat is already
  entitled to make.

- **`forced_declaration_without_choice`** returns the empty `DeclareAttackers` or
  `DeclareBlockers` **only** when it can prove no non-empty declaration is legal (CR 508.1a /
  509.1a, evasion included). Without it the settle stops dead on a declare-attackers step for
  a player controlling no legal attacker — an empty prompt with exactly one possible answer.

Both read nothing but state and the card database, so they are deterministic and
replay-stable. Automation changes *who clicks pass*, never *what state a pass produces*.

### Server — the settle loop and stop preferences

- **Room policies, off by default.** `AutoPassPolicy` and `StopPolicy` both default to `Off`,
  reproducing un-automated behavior bit for bit; the server binary turns them on for real
  games. Determinism holds either way.
- **The settle loop.** After every applied action — and at room start, after a timeout's
  default action, and after a stops change — the room repeatedly acts on the current holder's
  behalf while that seat is idle by the predicates above and its stops do not name the current
  step. The loop halts the moment a seat has a meaningful action, owes a forced choice, or has
  opted to stop, and, as a defence against a non-terminating configuration, after a fixed
  logged cap. That cap is not only a bug detector: on a board where neither seat can ever act,
  it is the ordinary stopping point for a game with nothing left to do.
- **Per-seat stop preferences live on the room**, set over the protocol and held exactly as
  display names are, so they survive disconnect and reconnect with no extra machinery.

**Human seats stop at their own main phases by default.** An empty default is safe for
*decisions* — the predicates guarantee a seat is only auto-passed when passing is its sole
meaningful move — and wrong for *comprehension*. A human whose turn holds nothing castable
otherwise watches the settle run both main phases and the whole turn between two broadcasts,
and cannot tell whose turn it now is. Nothing was decided for them, and they still lost the
turn.

So `StopPolicy` *seeds* a seat that has never sent preferences: a human seat with its own main
phases, an AI seat with nothing, so AI-only and headless games keep their throughput. The seed
is retired permanently by the first preference a seat sends, which is why the room
distinguishes "never asked" from "asked for nothing" — without that distinction a player could
not clear a default, since sending the empty set would hand the seed straight back.

**Stops carry a scope.** "Your own main phase" is not expressible in a flat per-step set, and
seeding an unscoped one would hand a player priority in every *opponent* main phase too,
reintroducing the very click this removes. The preference is therefore two lists: `stops`
(any turn) and `own_turn_stops` (only while the seat is the active player), with the wider
claim winning where a step appears in both.

### Protocol — preferences up, a legible settle down

- **`set_stops` (client → server)** carries the seat's stop phases. Server-authoritative and
  reconnect-durable; an unparseable message is ignored and the current view re-sent.
- **`GameView.stops` / `own_turn_stops`** carry the receiver's *effective* sets, seeds
  included, so the stops UI rebuilds from one message.
- **`GameView.auto_passed_steps`** carries the turn-and-step positions the room acted at for
  that receiver, in order, with consecutive duplicates collapsed; `auto_passed` is exactly that
  list being non-empty. A boolean alone can say "you were skipped" but never *what you
  missed*, which is the half that matters.

  It is a **path, not a set**: a genuinely revisited position appears twice, and consumers must
  not de-duplicate. Each entry carries **its own turn** rather than leaving a client to read a
  repeated phase as a turn boundary — an inference that looks safe and is not, since an extra
  combat phase (CR 506.1) revisits the combat steps within one turn and an extra cleanup
  (CR 514.3a) revisits cleanup. "Same step twice" and "new turn" are independent facts, and
  deriving one from the other would be the client asserting game structure the server never
  stated.

  It stays advisory and display-only. The authoritative record of what happened inside a
  settle is the ADR 0007 log window, which the same view already carries.

### Client — display only

Per-step stop toggles reading the view's stop sets and answering with `set_stops`, and a
visible indicator of where a settle acted. The client computes no legality and decides no
"no meaningful response" — it renders the server's stops and echoes toggles back.

## Consequences

- **Easier.** A spell-less turn collapses from a click per step to none. Stops ride the same
  room-state and reconnect machinery as names and timers, so nothing new is needed to make
  them durable.
- **Harder / given up.** The room now advances state between client messages, so one view can
  reflect several steps of progress at once; `auto_passed_steps` exists to make that legible,
  and every client must handle a view that jumped. The predicates are deliberately
  conservative — they do not try to auto-pass a seat that *could* respond but obviously would
  not.
- **This is the basic tier.** Auto-yield, hold-priority, and full-control mode generalize the
  stop set into a per-step yield/stop/act matrix; the engine predicates and the room loop are
  the substrate they would build on.
