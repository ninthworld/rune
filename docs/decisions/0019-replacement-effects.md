# ADR 0019: An event as a value, and the layer that rewrites it

- Status: accepted
- Date: 2026-08-05

## Context

Everything the engine did before this was a reaction. A triggered ability is collected by
diffing the state a change produced; a state-based action tidies a board that has already
moved; a continuous effect (ADR 0005) recomputes a characteristic of an object that
already exists. All three answer the question *what is true now*.

A replacement effect asks a different question (CR 614.1): given an event that **would**
happen, what happens instead? The original event never occurs, so nothing downstream may
observe it — no trigger, no state-based action, no projection. That cannot be expressed as
a reaction to the event, because there is no event to react to.

The engine already had one place shaped like this. `enters_tapped` and
`enters_with_counters` were applied to a freshly built `Permanent` before it was pushed
onto the battlefield, so a 0/0 arriving with two `+1/+1` counters was a 2/2 and survived
CR 704.5f. That was a correct outcome reached by a mechanism that could only ever produce
that outcome: it read the *permanent*, so nothing outside the permanent could participate,
and it applied whatever it found in whatever order it found it, so CR 616.1 — the affected
player chooses which of several applicable replacements applies first — had nowhere to
live. And there were three other entry sites (a land played, a token created, a card put
there by an effect) that each built a permanent their own way.

M19's Mistcaller is what made the shape matter: `The next time a nontoken creature would
enter the battlefield this turn without being cast, exile it instead.` The effect belongs
to no entering object, applies to an entry an opponent controls, and cancels the event
rather than modifying it.

## Decision

**A replaceable event is a value the engine passes around before it happens.**
`PendingEntry` describes a permanent's arrival in full — what is entering (a card or a
token), under whom, tapped or not, with which counters, attached to what, attacking what,
and whether it got there *by being cast* — and every road onto the battlefield builds one
and hands it to a single function. Nothing constructs a `Permanent` and pushes it; the
entry seam is the only place a permanent is born.

Four rules follow, and they are the layer:

1. **Applicability is derived, never stored.** `applicable_to_entry` recomputes, from the
   state and the event, which replacements apply right now: the entering object's own
   self-replacements (CR 614.1c) and the one-shot replacements an ability created for the
   turn (`GameState::replacements`). Two source lists walked into one ordered set — the
   shape ADR 0017 established for emblems.
2. **The affected player orders them (CR 616.1).** With more than one applicable, the
   *affected object's controller* — routinely not the effects' controller — picks which
   applies first. That is a player decision in the middle of something, so it rides the
   queue every other one rides (ADR 0013): a fourth `ChoiceQuestion`, one `Action`, and
   the entry waits **in no zone at all** while the question is owed. The permanent is
   therefore never briefly on the battlefield mid-decision — not by careful ordering, but
   because it is not there.
3. **Nothing applies twice to one event (CR 614.5).** The event records what has already
   been applied to it, and the collector skips those. This is what terminates the loop: a
   modification that leaves the event still matching the effect that made it — "enters
   tapped" is exactly that — would otherwise be applied forever.
4. **Applying is a loop, and answering re-enters it.** Apply one, ask again. Either the
   event is modified and the layer looks for what else applies, or it is replaced outright
   and nothing enters. Answering an ordering question applies the named replacement and
   hands the event back to the same function, so a second question, a lone survivor, and a
   completed entry are one code path rather than three.

A created replacement is **one-shot and one-turn**: applying it removes it, and the turn
boundary clears the list. Those are the two halves of the only duration a card prints for
this shape (`the next time … this turn`), so neither is a duration vocabulary — they are
one fact each, recorded where every other per-turn permission already is.

`enters_choosing_color` is deliberately **not** collected as a replacement, though
CR 614.12 calls it one. It is a question rather than a modification, there is nothing to
order it against, and the entry is already deferred on it (ADR 0013 §8). Folding it in
would mean posing a choice about a choice.

## Consequences

The four battlefield-entry roads became one, and gained a rules layer on the way. A token
now runs through the same seam a card does, which is what makes the `nontoken` wording of a
printed replacement a *filter* rather than an omission — the layer is asked about tokens
and answers no. A land played goes through it too, so a tapland's `enters tapped` is the
same mechanism as everything else rather than an inline write at the play-land site.

The cost is that `create_token` and the entry seam now return `Option<PermanentId>`: an
entry can be replaced, and then there is no permanent. That is honest and every caller
already ignored the id.

The bar for the next event is low and the bar for the next *modification* is lower. Damage
prevention (CR 615) is a second event value plus its seam; entering as a copy is a third
`ReplacementEffect` variant. Neither needs a second collector, a second ordering
mechanism, or a second never-twice rule.

What we did not build, and what the compatibility report's exclusion says plainly: only
the entry event is replaceable. A permanent *leaving* the battlefield, damage, a draw, and
life gained are all events printed cards replace, and none of them routes through this
layer. Adding one is not free — the leave seams run inside the state-based-action loop,
where there is nothing to suspend a CR 616.1 question onto, and that is the real work
behind the next event rather than the collector.

The replacement is also **silent in the public log**. `sage_protocol::GameLogEvent` has no
entry for a replaced event, and a fact recorded in the engine that the projection quietly
drops is worse than one that is not recorded at all; the exile zone is public, so the card
is visible where it landed. A wire variant is the fix, and it belongs to a change that is
allowed to touch the protocol.
