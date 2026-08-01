# ADR 0016: Loyalty as counters, and what an attack names

- Status: accepted
- Date: 2026-07-31

## Context

`CardType::Planeswalker` existed and `CardData::is_permanent` classified it, and that was
the whole of it. There was no loyalty counter, no loyalty ability, no way to attack one,
and `TargetSpec::AnyTarget` documented its own gap in prose — "planeswalkers and battles
are not modeled, so the legal set is exactly creatures plus players". It was the oldest
entry in `data/exclusions.json` and the last structural hole in the permanent model.

Two things made it worth building for its own sake rather than for the cards it unblocks.
The first is that the hole was load-bearing on other work: `any target` is the burn
spell's target line, and a spec that quietly means something narrower than the rule it
names is the kind of thing that stays wrong. The second is that the fix is not additive
— `Attack.defender` was a `PlayerId`, and CR 508.1a says an attack names a player *or a
planeswalker*, so the widening ripples through the declaration, its legality check, damage
assignment, the multi-defender flow of #344, and the wire. That single type change was
most of the risk, and it is why this ADR exists.

Not for the cards, though. All five M19 planeswalkers need an **emblem** — a zoneless,
controller-scoped continuous object the engine had nothing like — and four of them need a
second unmodeled subsystem besides. None of them was authorable from this change, and none
was authored. The acceptance evidence is a set of inline `test_*` definitions, exactly as
ADR 0009 prescribes for a shape the shipped set does not represent. (ADR 0017 built the
emblem object and the rest, and authored all five.)

## Decision

**1. Loyalty is a counter kind, not a field.** `CounterKind::Loyalty` joins the two
power/toughness kinds in the same `BTreeMap` on `Permanent`. A dedicated `loyalty: u32`
field would have been a second place to keep a number, and every rule that touches
loyalty — the ability cost, damage, the state-based action — is already a rule about
counters. It folds into no characteristic, which is why the layer-7c delta reads only the
two P/T kinds and ignores it without a special case.

Printed loyalty is a *characteristic* and stays one: `CardData.loyalty` and
`Characteristics.loyalty` are the number in the corner, the thing a card in hand shows.
The permanent gets that many counters **at the battlefield-entry seam** (CR 306.5b),
beside `enters_tapped` and `enters_with_counters` — not as an authored ability, since
every planeswalker does it from the printed number alone, and not afterwards, because a
planeswalker that arrived at zero would be collected by CR 704.5i before anyone could act.

**2. A loyalty ability is an ordinary activated ability with a new cost.**
`Cost::Loyalty { amount: i32 }` is the first cost in the IR that can be *positive*: `+1`
adds a counter. What makes an ability a loyalty ability is having one — `is_loyalty_ability`
is derived from the cost, never a flag on the card, so an ability cannot claim the
timing without paying like one.

The two CR 606.3 timing rules (sorcery speed, once per turn per permanent) are enforced
where every other activation gate is: in the offer, and again independently in
`apply_action`. The second gate is not redundant. It is the same hardening shape
`activation_clears_summoning_sickness` uses, and it is what stops a forged or stale action
id from spending loyalty a planeswalker has not got, taking a second activation in one
turn, or slipping one in at instant speed.

Sorcery speed here is measured from the **controller**, not from the priority holder. The
two agree in the ordinary case and come apart exactly where it matters: an opponent
holding priority during your main phase must not be able to activate their own
planeswalker through a window that is not theirs.

`GameState.loyalty_activations` is the one new piece of stored state, keyed by
`PermanentId` and cleared by `begin_next_turn`. It is raw history for the same reason
`Permanent::damage` is: a planeswalker that spent its activation leaves exactly the board
one that did not would leave, so a snapshot cannot recover it. Keying it to the permanent
rather than to the card is what makes a planeswalker that leaves and returns get a fresh
allowance, which is what CR 606.3 says.

**3. `Attack.defender` and `Permanent.attacking` became `AttackTarget`.** This is the
contract change. `AttackTarget` is `Player(PlayerId)` or `Planeswalker(PermanentId)`, and
retyping both fields — rather than adding an optional planeswalker beside the existing
seat — is deliberate for ADR 0015's reason: every read became a compile error, so the
compiler named the sites instead of a reviewer's memory.

What the retype exposed is that the old field was answering **two questions at once**.
Downstream code read `attacking` as both "what is being attacked" and "who declares
blockers against this". Those come apart the instant a planeswalker can be attacked: the
thing attacked is the planeswalker, the player who blocks for it is its controller. So
they are two functions now — `attack_target_of` for the thing, `attacking_defender_of` for
the player — and the multi-defender flow of #344, the blocker scoping, and the APNAP
declaration order all read the second and needed no other change.

`AttackTarget::defending_player` returning `Option` is where a second rule lives for free:
a planeswalker that has left the battlefield has no controller to answer for it, so an
attacker aimed at one that died mid-combat has no defending player, blocks against nothing,
and deals its damage nowhere. **The redirection rule is gone from current rules** — damage
aimed at a player is never moved to a planeswalker — so "the attack simply misses" is the
whole behavior, and it falls out rather than being written.

**4. One damage-to-a-permanent seam decides what damage does.**
`GameState::deal_damage_to_permanent` replaced the bare `mark_damage_on_permanent` at every
call site: it removes loyalty from a planeswalker (CR 120.3c) and marks damage on
everything else (CR 120.3d), recording the same `DamageDealt` event either way. Combat, a
targeted burn spell, and a class-wide sweeper all route through it, so "damage to a
planeswalker takes loyalty" is one fact rather than three that could be implemented
two-thirds of the way — and, in particular, a planeswalker's damage is never *marked*,
which matters because marked damage clears at cleanup and a planeswalker whose damage were
merely marked would heal every turn.

The seam's existence also forced an honest guard elsewhere: CR 704.5h destroys a *creature*
dealt deathtouch damage, and the state-based action had been relying on the other two
death checks to imply creature-ness. A planeswalker can now be dealt combat damage by a
deathtouch source, so the creature test is stated.

**5. Every target spec states its position on planeswalkers.** `AnyTarget` regains its
planeswalker arm — CR 115.4 *means* creature, player, or planeswalker — and every other
spec now says in its doc comment whether it includes one and why. This is the part a
compiler cannot ask for: a spec that names a type excludes planeswalkers by saying
nothing, which is indistinguishable from having forgotten them. Three groups, all written
down: included by construction (`any_permanent`, `any_nonland_permanent`), included by
rule (`any_target`), and excluded because of what they name (everything else). No new
`any_planeswalker` variant, because no card needs one; the vocabulary grows when a card
does.

**6. The legend rule is written for legendary permanents, with one substitution.**
CR 704.5j is the rule the planeswalker uniqueness rule became in 2017, so it is
implemented once for legendary permanents generally rather than specially for
planeswalkers — a rule stated once cannot be right for one type and absent for another.

The rules let the controller *choose* which copy survives. The engine keeps the one that
entered most recently and bins the rest. That is a deterministic policy standing in for a
player choice, it is a real simplification, and it is recorded in `data/exclusions.json`
rather than left implicit. A choice belongs in the `PendingChoice` queue of ADR 0013, and
putting it there is its own change; keeping the newest is the reading that makes the common
case — casting a second copy to reset it — behave as a player expects.

**7. The wire names both halves of an attack.** `Permanent.attacking_player` keeps its
meaning as *the defending player* — now the controller when a planeswalker is attacked —
and `attacking_planeswalker` names the planeswalker itself. They ride together rather than
one replacing the other, so a client draws its arrow at whichever it wants and derives no
relationship between them. `CardView.loyalty` is printed starting loyalty and is
deliberately not the same channel as the `loyalty` counter in `Permanent.counters`: one is
what the card enters with, the other is what it has now, and a client that showed the first
on the battlefield would report `4` for a planeswalker already down to `1`.

The `defend_<id>` requirement slot's candidates are now player ids **and** permanent ids in
one list, and its gate changed from "more than one opponent" to "more than one thing to
attack". A two-player game with no planeswalker on the far side still offers no slot at
all, so the common wire is untouched; a two-player game becomes a genuine choice the moment
an opponent resolves a planeswalker, which is correct and was not expressible before.

## Consequences

The oldest exclusion is gone, replaced by three narrower and more honest ones: emblems,
planeswalker-specific static abilities, and the legend-rule choice. **No M19 planeswalker
became authorable**, and the exclusion list says why — that was true before this change and
is unaffected by it. What became possible is the subsystem: a planeswalker can be cast,
accumulate and spend loyalty, be targeted by `any target`, be attacked, be blocked for, and
die, all through the ordinary pipeline.

Emblems were the next thing, and they were not a variation on anything here: an emblem is a
zoneless object with a controller-scoped continuous effect, while `Ability::Static` is derived
from a *battlefield* source's presence. ADR 0017 moved that assumption, and the first two of
those three exclusions are gone with it — the legend-rule choice remains.

The cost is that combat no longer answers "which seat is this attacker attacking?" without
a lookup — `attacking_defender_of` walks the battlefield to find a planeswalker's
controller. That is the honest shape: the defending player of an attack on a planeswalker
is not stored anywhere, it is a fact about who controls that object right now, and control
can change. Code that read the seat straight off the attacker was assuming an answer that
was only ever true because planeswalkers did not exist.
