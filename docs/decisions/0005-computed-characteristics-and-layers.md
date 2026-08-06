# ADR 0005: Computed characteristics and the CR 613 layer system

- Status: accepted
- Date: 2026-07-30

## Context

A permanent's *current* characteristics — power and toughness, types, colors, abilities — are
not what is printed on its card. Counters, anthems, pump spells, and type-changing effects
alter them continuously (CR 613, the layer system).

The engine's rules force one answer to how that is represented. `GameState` is a `Clone`/`Eq`
value type with no cached derivations (ADR 0001); there are no listeners, so continuous
effects can never be pushed onto stored characteristics; and there is no wall-clock or ambient
state, so any ordering signal must derive from data already in `GameState`.

The alternative — storing current power/toughness on the permanent and updating it whenever
something changes — is the push-based model the engine rejects everywhere else. It also fails
on its own terms: with two effects whose order matters, a stored value has no way to
recompute itself when the earlier one goes away.

## Decision

### 1. Store versus compute

`Permanent` stores only **raw, non-derivable** state: identity, card, base controller,
tapped status, counters, and the per-permanent raw inputs a continuous effect needs. A
permanent's **current characteristics are never stored** — power/toughness, current types
and subtypes, colors, and the current ability set are all computed.

The stored controller is the *base* controller — the seat the permanent arrived under —
and not the answer to "who controls this?" A control-changing continuous effect is layer 2
(see §3) and is never written onto the permanent. That falls out of the same reasoning as
everything else here, and it pays for itself twice: control reverts when the effect ends
with nothing to put back, and the stored field goes on standing in for the permanent's
**owner**, which is what CR 400.7 reads when the card leaves the battlefield.

Counters are stored because they are raw input, not derivation: nothing else in state
determines how many `+1/+1` counters a permanent has. The no-cached-derivations rule forbids
storing *computed* characteristics; it does not forbid storing counters.

### 2. One pure read path

All current-characteristics reads go through a single pure function:

```
characteristics(&GameState, PermanentId, &CardDatabase) -> Characteristics
```

`Characteristics` is a value type holding what rules code needs: types, supertypes, subtypes,
mana cost, power, toughness, abilities. The function runs the layer system fresh on every call
and caches nothing. It takes `&CardDatabase` for the same reason `apply_action` does
(ADR 0003): printed seed values live in the database, which stays out of `GameState` to
preserve the value type's `Eq` and purity.

### 3. Layer ordering and timestamps derive from state

Within a layer, effects apply in timestamp order (CR 613.7). Timestamps derive from data
already in `GameState` — no wall-clock, no ambient counter. The monotonic
`GameState::next_object_id` minted by `mint_id` is the source of strictly increasing,
replayable ordering: a continuous effect's timestamp is the object id assigned when it was
created. Counters within a permanent are order-independent (they sum) and need no timestamp;
order-sensitive modifiers carry one on the stored effect input.

The full CR 613 order is copy → control → text → type → color → ability-adding →
power/toughness. The power/toughness end (layer 7c) and keyword granting (layer 6) are
implemented, because that is what counters, anthems, pumps, and keyword grants need.
**Control (layer 2) is implemented too**, and is the exception to the paragraph above:
control is not a characteristic (CR 109.3), so it has no place in the `Characteristics`
value and is answered by its own function instead —

```
controller_of(&GameState, &Permanent) -> PlayerId
```

That separation is load-bearing, not cosmetic. Layer 2 is applied before every other
layer, so the layer-6 and layer-7c selectors have to ask it "does this permanent's
controller match?" from *inside* the computation they are part of; a function that reads
only stored effects and the base controller can answer that without recursing, and one
routed through `characteristics()` could not. Reviewers should treat a direct read of
`Permanent::controller` the same way §4 treats a direct read of printed `CardData`: it is
legitimate only when the question is about **ownership**, which today means the four
battlefield-departure seams. Layers 1 and 3–5 sit behind the existing signature, so filling
them in changes no call site.

**Layer 6 subtracts as well as adds**, which is what made it an *ordered* layer. A grant
after a removal grants and a removal after a grant removes (CR 613.1f), so the layer folds
its effects in ascending timestamp order exactly as layer 7c does; among grants alone the
order is still immaterial, which is why it did not have to be until now.

Losing **all** abilities is the one modification that is not about a single named thing,
and it is answered by a second non-recursive predicate beside `controller_of` —

```
loses_all_abilities(&GameState, &Permanent) -> bool
```

— read from inside `abilities_of_permanent`, which every path that walks a battlefield
object's abilities goes through and which therefore takes `&GameState`. That is the whole
of the enforcement: there is one accessor, it answers the layer-6 question, and no
printed-abilities reader is left to reach for by mistake. The predicate reads stored
effects only, so calling it from inside the characteristics computation cannot recurse.
A boolean is exact rather than convenient here: a later grant would put an ability back,
and the only grants the IR can express are keyword grants, which the ordered layer-6 fold
already settles.

### 4. The invariant

**Rules code never reads printed `CardData` for a permanent's current characteristics —
always through `characteristics()`.** Printed data is the *seed* the layer system reads, not
the answer to "what is this permanent's power right now." Reading `CardData` directly stays
legitimate for a card **outside** the battlefield — in hand, on the stack, in a graveyard —
where no permanent and no continuous effect exist.

## Consequences

- **Easier.** Every characteristic-changing mechanic slots into one pure function with a fixed
  signature instead of touching every call site. The pull-based, no-listener discipline
  extends cleanly: characteristics are a query, never a stored reaction. Undo, replay, and
  resync stay free because nothing derived is persisted.
- **Harder / given up.** `characteristics()` recomputes on every query, so hot paths —
  legal-action generation, state-based actions — pay repeated layer-system cost instead of
  reading a cached field. That is an accepted trade for purity and correctness, and it matches
  the engine's recompute-everything stance everywhere else. Reviewers must guard against new
  direct printed-value reads for battlefield permanents.
- **Not covered.** Layers 1 and 3–5; a control change lasting longer than a turn, or
  exchanging two permanents' controllers; an ability loss aimed at a target or a class
  rather than at the effect's own source, or one outliving the turn; a rule applying *as
  though* a permanent lacked a keyword it still has, which is not a layer at all; counter
  kinds beyond `+1/+1` and `-1/-1`; characteristic-defining abilities. Each is an addition
  behind the existing signature.
