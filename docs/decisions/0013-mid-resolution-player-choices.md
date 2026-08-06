# ADR 0013: How the engine poses a mid-resolution player choice

- Status: accepted
- Date: 2026-07-31

## Context

Until now every effect in the card IR resolved without asking anyone anything. The only
mid-resolution decision the engine modelled was choosing a **target**, and targets are
not really mid-resolution at all: they are chosen when a spell or ability is announced
(CR 601.2c) and merely re-checked when it resolves (CR 608.2b). A triggered ability was
the one exception, and issue #602 handled it by making "this trigger is owed targets" a
*derived* fact about a stack object, handing priority to the trigger's controller, and
offering that seat one action and nothing else until it answered.

A discard, a scry, a look at the top N, and a library search are a different shape. The
object is genuinely part-way through resolving: Tormenting Voice must discard, *then*
draw two, *then* reach its graveyard, and the discard is a question its controller has to
answer in the middle. Four exclusion entries named this same gap from four angles, and
between them they blocked a couple of dozen otherwise-writable cards.

Three forces shaped the design:

- **A stall is worse than a wrong effect.** A question nobody is asked to answer hangs
  the game for every seat. Discarding from an empty hand, searching an empty library, and
  looking at four cards that contain nothing takeable are all real situations.
- **Hidden information becomes load-bearing for the first time.** A library search shows
  one player cards that no other seat may ever see. The engine holds every zone in the
  clear, so nothing structural prevented a projection bug from leaking a deck.
- **The engine may not grow a second control flow.** `apply_action` is one pure
  transition over an immutable state (ADR 0001). Suspending mid-resolution must not
  become a coroutine, a continuation closure, or a mutable "we are inside an effect" flag.

## Decision

A mid-resolution choice is **queued state plus a derived question**, and everything else
follows from that.

1. **The queue is state; being owed a choice is derived.** `GameState::pending_choices`
   holds `PendingChoice` values in the order they were posed, exactly as `stack` holds
   objects. `pending_player_choice` is the head of that queue and nothing else. There is
   no "waiting" flag that could disagree with the queue, mirroring how
   `pending_trigger_target_choice` reads the stack rather than a bit.

2. **The candidates are derived, never snapshotted.** A `ChoiceRequest` names a zone, a
   class of cards, and bounds; `choice_candidates` and `choice_bounds` evaluate that
   against current state on every call — the same relationship a `TargetSpec` has to the
   legal target set. An answer is therefore validated against the set that exists *now*.
   That is affordable precisely because the game is frozen: while a choice is owed
   nothing else is legal, so the zone cannot move underneath it.

3. **The remainder of the resolution rides on the choice.** A `PendingChoice` carries a
   `Resume`: the effects the object had not reached, the targets they still owe, and —
   for a spell — the card that must still reach its final zone (CR 608.3). Resolution
   suspends by returning `true` and resumes by replaying the same function over the
   remainder, so there is one code path, not two, and no continuation closure anywhere
   near `GameState`'s value semantics.

4. **Priority goes to the chooser, and one slot remembers whose it was.** The chooser is
   the seat the *effect* names, which is frequently neither the priority holder nor the
   resolving object's controller. `interrupted_priority` is the single saved slot shared
   with the trigger-aiming hand-off, because both can be owed at the same moment and only
   one seat can hold priority. Choices are checked first: an object part-way through
   resolving outranks a trigger waiting to be aimed.

5. **A choice with no legal answer is never posed.** `pose_choices` clamps the bounds to
   what the zone actually holds; when the maximum comes out zero it applies the outcome
   immediately with an empty selection and resolution carries on. This is one branch in
   one place rather than a per-effect judgement, and it is why the *aftermath* belongs to
   the request rather than to the answer: a search that finds nothing still shuffles
   (CR 701.19c), and a look that takes nothing still bottoms what it looked at.

6. **Hidden information is scoped to the chooser, and to one wire field.**
   `GameView::revealed` carries the choice's candidate cards, and the projection emits it
   only when `chooser == viewer`. `SpectatorView` has no such field at all, so a
   spectator is structurally incapable of receiving one. The log records
   `cards_discarded` as a count and `library_searched` as a bare player — never a card
   identity.

7. **The wire reuses `select_from_zone` rather than adding a shape.** The prompt gains an
   optional `min`, present only when a player may legally under-fill the slot (scry any
   number, take up to one, fail to find). An exact prompt omits it and serializes exactly
   as it did before.

8. **A question that is not mid-resolution still belongs in this queue** (issue #738). The
   colour a permanent names *as it enters* (CR 614.12) is not part of a resolution at all —
   it is part of an arrival — and it is queued here anyway, because everything around the
   asking is identical: one queue, one chooser, one action, the same priority hand-off, the
   same freeze on every other seat.

   What differs is only what the answer *does*, so `ColorRequest` carries a `ColorOutcome`
   the way `ChoiceRequest` carries a `ChoiceOutcome`: add a point of mana, or complete a
   battlefield entry. The entry itself — the card, its controller, whether an effect said
   "tapped", an Aura's chosen host — rides on the question as a `PendingEntry`, the direct
   analogue of the `SuspendedSpell` a `Resume` carries.

   That placement is what makes "the permanent is never briefly on the battlefield without
   its colour" true **by construction rather than by ordering**: while the question is owed
   the card is in no zone, so there is no permanent for a state-based action, a trigger
   diff, or a projection to catch mid-decision. It also needs no `Resume` — the entry is the
   last step of a resolution rather than one of its effects — which is why the resume slot
   is simply `None` there rather than special-cased.

9. **An entry question is an unfilled slot on the event, not a branch in the seam** (issue
   #738, second half). Naming a *card* is the second question a permanent can be asked as it
   enters, and adding it made the shape of §8 clearer than one question could: the answers
   live on the `PendingEntry` itself, `begin_battlefield_entry` refuses to finish while a
   card that asks has an empty slot, and answering writes the slot and re-enters that same
   function. It is the loop the CR 616.1 ordering answer already used, generalised — so a
   card asking two questions needs no code deciding which comes first, and it terminates for
   the reason CR 614.5 makes the replacement loop terminate: a filled slot is never emptied.

   **A named card is a `FunctionalId`, and that is a legal rule rather than an
   implementation choice.** The project ships no card name it has not itself defined, and a
   free-text answer would be the one way a game in progress could come to hold one. So the
   answer set is derived from the **catalog** — `named_card_candidates`, filtered by the
   class the card declared — the action carries a `CardId` handle, the legality gate re-checks
   it against that freshly derived list, and the wire offers the cards' authored identities
   with the catalog's own names as labels. The client composes nothing and sends no string;
   the projection resolves the recorded identity back to a name for display and nothing else.
   This is the same regenerate-and-check discipline every other answer follows, doing double
   duty as the enforcement point for a posture the repository otherwise only states.

## Consequences

Four exclusions collapse to one mechanism, and the next choice-shaped effect — a modal
spell, naming a card, an optional sacrifice — has a queue, a routing rule, a
no-legal-answer rule, and a hidden-information channel already built. Adding one is a
`ChoiceOutcome` variant and a `choices_for_effect` arm.

The queue turned out to be reusable beyond resolution (§8), which is the strongest evidence
the shape was right — but it also means "a choice is owed" no longer implies "something is
mid-resolution". A `PendingChoice` with no `Resume` is now an ordinary state of affairs, and
code that reads one must not assume the other.

The cost is a second kind of thing that can be "owed" alongside a trigger's targets, and
a `GameState` that can now be *mid-resolution* rather than only between actions. Any
future code that reasons about "the stack is empty, so nothing is happening" has to
consult `pending_choices` too. The shared `interrupted_priority` slot is the concentrated
form of that risk: if a third interrupting choice ever arrives, it must join the same
check rather than add a second saved slot.

We gave up two orderings, both listed in the exclusion report rather than quietly
approximated: the cards a scry keeps on top stay in their printed order, and the cards a
look-at-the-top bottoms go there at random rather than in an order the player picks. The
random one is right for the cards that say "in a random order" and conservative for the
ones that say "in any order" — it tells the player strictly less than the real card does,
which is the safe direction to be wrong in.

**The second of those was taken back** (issue #746), and the shape held: a look that says
"in any order" now asks its controller for a *permutation* of the remainder, as a fifth
`ChoiceQuestion` variant plus its own `Action`, on the same queue with the same routing.
Three things it needed that were already here — the never-stall rule (a remainder of one
card is not a decision), the derive-don't-snapshot rule (the remainder stays on top of the
library, so the question is a window onto it), and the hidden-information channel — and
one thing it did not: **an answer must not consume randomness**. A player-chosen bottoming
that drew from the seeded stream would fork every later shuffle on replay, so the two
orders take different roads through the same function, and the ordered one leaves
`rng_seed` exactly where it found it. That is a new obligation on every future answer that
replaces something the game used to roll for.

It also produced the first choice that poses **another choice as its outcome**: "the rest"
is not knowable until the taking is answered, so the second question is queued when the
first is applied and the suspended `Resume` moves across to it. A `PendingChoice`'s resume
therefore travels, and code that assumes the question a resume was attached to is the
question it will be answered on is wrong.
