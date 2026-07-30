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

`Permanent` stores only **raw, non-derivable** state: identity, card, controller, tapped
status, counters, and the per-permanent raw inputs a continuous effect needs. A permanent's
**current characteristics are never stored** — power/toughness, current types and subtypes,
colors, and the current ability set are all computed.

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
implemented, because that is what counters, anthems, pumps, and keyword grants need. Layers
1–5 sit behind the same function signature, so filling them in changes no call site.

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
- **Not covered.** Layers 1–5; counter kinds beyond `+1/+1` and `-1/-1`; characteristic-defining
  abilities. Each is an addition behind the existing signature.
