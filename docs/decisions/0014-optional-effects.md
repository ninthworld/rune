# ADR 0014: Optional effects, and paying for one mid-resolution

- Status: accepted
- Date: 2026-07-31

## Context

ADR 0013 gave the engine one way to stop mid-resolution and ask a player something:
*choose N cards from this zone*. It closed with the observation that the next
choice-shaped effect — "a modal spell, naming a card, an optional sacrifice" — would find
the queue, the routing rule, the no-legal-answer rule, and the hidden-information channel
already built, and would cost "a `ChoiceOutcome` variant and a `choices_for_effect` arm".

The optional effect (`you may draw a card`, `you may pay {1}. If you do, draw a card`) is
the next one, and that estimate turned out to be half right. The queue, the routing, and
the never-stall rule all carried over unchanged. The *answer* did not: a yes-or-no is not
a selection of zero or more cards, and pretending it was — an empty pick means no, a
nonempty pick means yes — would have made the wire ambiguous and the legality check
meaningless.

The second half is genuinely new. `you may pay {1}` charges mana **while an object is
resolving**, and every payment the engine had until now happened on the cast or activation
path, where the player holds priority and pays as part of taking an action. Nothing had
ever spent mana in the middle of somebody else's resolution.

## Decision

**1. A pending choice carries a *question*, and the question has two shapes.**
`PendingChoice::question` is a `ChoiceQuestion` — `Cards(ChoiceRequest)` or
`Confirm(ConfirmRequest)`. Everything around it stays single: one queue, one chooser
field, one `Resume`, one saved `interrupted_priority` slot, one "a question with no legal
answer is never posed" rule. Only the answer differs, so only the answer's shape branches.

**2. The answer is its own action.** `Action::AnswerConfirm { accept }` sits beside
`Action::AnswerChoice { chosen }` rather than overloading it. Each is legal only against
its own shape of question, so a card selection aimed at a yes-or-no is not a wrong answer —
it is an answer to a question nobody asked, and it is rejected as such.

**3. Accepting splices, it does not apply.** The accepted effects are put on the *front*
of the suspended remainder and resumed through the ordinary effect walk. Three things fall
out of that instead of being arranged: the card happens in printed order; an accepted
effect that poses a further choice suspends exactly as any other effect does; and
declining is the same code path with nothing spliced, which is what makes "a decline
leaves the game as if the effect were absent" true by construction rather than by two
branches maintained in parallel.

**4. The controller answers.** Not the priority holder, and not a player the surrounding
ability names. An optional effect is an offer to the object's controller (CR 608.2), which
for a trigger on an opponent's turn is a seat that is not acting at all.

**5. Paying mid-resolution is a mana payment and nothing more, and mana abilities stay
legal while it is owed.** CR 605.3a lets a player activate mana abilities whenever they
are asked to pay a cost, so while a costed question is owed its chooser is offered the
answer, their mana abilities, and a concede — nothing else, and no other seat anything at
all. A mana ability uses no stack and hands nobody priority, which is the only reason it
can be let through a freeze. This is also the whole of the payment vocabulary: an optional
cost is mana, and paying by sacrificing or discarding is listed in the exclusions rather
than half-modeled.

**5a. Revised (issue #744): the cost is the activation vocabulary, and a payment the
player *picks* is a second question.** The last sentence of decision 5 was a scoping call,
not a finding, and Brawl-Bash Ogre is the card that closes it. `Effect::May::cost` is an
`OptionalCost` — mana, a sacrifice, or a discard — which is `Cost` minus every component
whose payment is the source itself (tap it, move its loyalty, sacrifice it, take counters
off it). Those four are *unwritable* rather than rejected: they fail to parse, in
`build.rs` and in the loader, because the question is answered from a queue that carries no
source and a spell never had one. That is the exclusion entry, narrowed to what is actually
still missing.

The mana form is unchanged. The other two cannot be charged the way mana is, because
*which* permanent and *which* cards is a decision — so accepting poses that decision as an
ordinary `ChoiceQuestion` (the same `Permanents` shape a mandatory sacrifice uses, the same
`Cards` shape a discard uses) and hangs the whole remainder, spliced effects and all,
behind it. Three properties fall out of that placement rather than being arranged:

- **The cost is paid before what it bought.** The wrapped effects are already on the front
  of the remainder when the payment question is posed, so they resume after it is answered.
- **The sacrifice is a real death.** It runs through the one leaves-battlefield seam, so a
  dies trigger sees a creature eaten by an optional cost exactly as it sees one destroyed.
- **The freeze does not widen.** While the payment itself is owed, nothing at all is legal
  but answering it — not even a mana ability, which a sacrifice has no use for.

**6. Two payability questions, deliberately different.** Whether the question is *posed*
is judged against the mana the seat could still make if it tapped out; whether an
acceptance is *legal* is judged against the pool as it stands. The first uses the
over-estimate the idle-seat predicate already made (now one shared function, so the engine
has a single answer to "what could this board pay for"). Both err the same way — toward
offering a decision that turns out unaffordable, never toward silently taking one away —
and the gap between them is exactly the window in which a player taps lands.

**6a. A picked payment has no gap, and one function answers both questions for it**
(issue #744). Tapping a land cannot conjure a creature to sacrifice or a card to discard,
so "could they pay" and "can they pay" are the same question there, and both are answered
by building the payment's own request and asking whether its clamped maximum still covers
what the cost demands — the engine's single signal for "there is nothing here", reused. The
offer, the acceptance gate, and the question the acceptance then poses are read off that
one construction, so the class the player is offered can never differ from the class the
engine checked before offering them the choice.

**7. The wire adds no shape.** The yes-or-no rides the `option` prompt the mulligan
decision already uses, on the same `choice` slot the card selection uses, under the same
`player_choice` action kind. The accepting option is listed only while the engine would
accept it, so a client renders what it was given and computes no affordability.

**8. An answered offer is logged, both ways.** `optional_applied` and `optional_declined`
carry a seat and nothing else. Recording them is not bookkeeping: an optional effect that
happens is indistinguishable from a mandatory one, and an optional effect that does not is
indistinguishable from a bug. The two events do not separate "declined" from "never posed
because the cost was unpayable" — that distinction is about a hidden pool, and the rest of
the table is not entitled to it.

**9. The wrapped effects may not target.** One `Effect` declares at most one target slot,
so a wrapper cannot honestly speak for what it wraps. Rather than let such a card resolve
into nothing, the catalog validator rejects it — in `build.rs` and in the loader, from the
one shared file — and "optional effects that choose a target" is an exclusion entry.

**9a. Revised (issue #725): a `may` *forwards* the target group of the one effect it
wraps.** The reasoning above had the right premise and drew the wrong line. "One effect
declares at most one group" does not forbid a wrapper from declaring a group — it forbids
it from declaring *two*, and a `may` over a single targeting effect has exactly one to
pass along. So `Effect::target_group` answers for a `may` by asking what it wraps, the
slot is filled at announcement (CR 601.2c) like any other, and the yes-or-no still waits
for resolution. Three consequences, and the third is the reason the target rides the
`ConfirmRequest` rather than the `Resume`:

- **Accepting splices the target with the effects.** Decision 3 already put the accepted
  effects on the front of the remainder; their targets go on the front of the remaining
  targets in the same move, so the wrapped effect consumes exactly what the announcement
  chose for it.
- **Declining drops both together.** A target left sitting in the remainder would be
  inherited by whatever effect came next — a decline that silently re-aimed the rest of
  the card. Carrying it on the offer makes "a decline leaves the game as if the effect
  were absent" stay true of targets as well as of effects.
- **A target that has gone still fizzles.** CR 608.2b is unchanged and applies before the
  question is asked at all: an object whose every target is illegal does not resolve, so
  there is nothing to accept. A target that goes illegal with the offer already posed is
  skipped by the resumed walk (CR 608.2c), which is the wrapped effect's own re-check
  rather than a second rule.

A **conditional** keeps the original rejection, and the difference is precisely why the
rule needed drawing again: its two branches share one flat target list, so a group named
in either could not be paired back onto the branch that was actually taken. A `may` over
two targeting effects is rejected for the same reason
(`Violation::TwoTargetsInsideOptional`). The exclusion narrows to those, plus the
reflexive trigger (CR 603.11), which is a *third* moment — after a cost is paid,
mid-resolution — that no announcement has reached and that deserves its own mechanism.

## Consequences

The three cards issue #610 was written for (Windreader Sphinx, Mentor of the Meek, Runic
Armasaur) are each one primitive away rather than two, and the primitives they still want
are selector work, not control-flow work.

Gravedigger and Reclamation Sage — the two cards decision 9a was drawn for — are authored
against the vocabulary as it stands, with no new effect variant between them.

Brawl-Bash Ogre, the card decision 5a was drawn for, needed no new effect variant either:
a cost field widened, and the question its acceptance poses was already built for
Fraying Omnipotence. Ajani's Last Stand — the other card the exclusion blocked — is still
out, and its optional cost is the smaller of the two reasons: it sacrifices *the source*,
which 5a leaves unwritable, and its second ability triggers on the card being discarded
from a hand, which nothing in the engine observes at all.

The cost is that `GameState` now has a state in which a player may act *and* an object is
part-way through resolving *and* what they may do is neither answering nor passing but
tapping a land. That is three facts at once where there used to be one, and it lives in a
single `if` in the action generator. A third interrupting choice must join that same
branch, exactly as ADR 0013 said of the shared priority slot.

We also accepted an asymmetry: the offer is posed on a generous estimate of the mana a
board could make, so a player with two lands that each tap for a different colour can be
asked a question they cannot actually pay for. They decline, which is always legal and
always free. Erring the other way would auto-decline a player out of a real decision, and
that is a rules error rather than an annoyance.
