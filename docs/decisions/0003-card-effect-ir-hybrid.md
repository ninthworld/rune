# ADR 0003: Card behavior as a declarative IR with a code escape hatch

- Status: accepted
- Date: 2026-07-30

## Context

Resolving a card to its printed characteristics is not enough: the engine needs a way for a
specific card to say "when this enters, draw a card" or "{T}: add {G}". Every downstream
system — the stack, mana, combat, the layer system — depends on that representation.

The architecture constrains it hard:

- The engine is pure and immutable; `GameState` is a `Clone`/`Eq` value type (ADR 0001).
- **No listeners or observers.** Triggered abilities are found by a pure diff of the states
  before and after an action; continuous effects are recomputed on demand. Nothing reacts to
  events; everything is pulled from current state.
- No new runtime dependencies; serde is permitted only for the compile-time-embedded card
  snapshot (ADR 0002).

XMage's model — one imperative class per card registering event listeners — is push-based and
incompatible with these rules. What transfers is its *data/code split*: metadata in a flat
file, behavior elsewhere.

Three options for the behavior side:

1. **Pure data-driven IR.** All behavior is data, never per-card code. Maximizes
   card-as-data, but forces the IR to encode every mechanic in Magic, and complex one-offs
   become tortured encodings.
2. **Code per card.** Each card is a module of pure functions. Most flexible for oddities,
   but thousands of files, heavy boilerplate, and behavior stops being uniformly inspectable.
3. **Hybrid.** A declarative IR for the common vocabulary, plus a code escape hatch for what
   the IR cannot express.

## Decision

Card behavior is a **closed, declarative IR carried on `CardData` as data**, with a
**`CardId`-keyed pure code escape hatch** for the rest.

- The IR lives in `crates/sage-engine/src/ability.rs`: `Ability` (`Activated`/`Triggered`
  plus the self-replacement variants), `Cost`, `Effect`, `TriggerCondition`, `TargetSpec`.
  All are `Deserialize` data enums, so ordinary cards are authored as JSON through the serde
  path of ADR 0002 — no new dependency.
- Abilities are interpreted by **pure functions** over `GameState`. A triggered ability's
  condition is a data value matched against a before/after diff (`condition_met`) — never a
  stored closure or listener.
- Whether an ability is a **mana ability is derived, not stored** (`is_mana_ability`): a mana
  ability resolves immediately without using the stack or changing priority (CR 605.3), while
  everything else goes on `GameState::stack` and resolves when players pass priority in
  succession.
- Fresh object identity comes from the monotonic `GameState::next_object_id` counter
  (`mint_id`), so every battlefield entry gets a fresh `PermanentId` with no reuse.
- The **escape hatch** is `crates/sage-engine/src/scripted.rs`:
  `scripted_abilities(CardId) -> Vec<Ability>`, unioned with the data-driven abilities by
  `card::abilities_of`. It stores nothing in state — behavior is re-derived from the `CardId`
  on demand, the same discipline as the layer system — so `GameState` keeps its `Clone`/`Eq`
  semantics. Function pointers may be looked up, never stored in state.

The boundary: a fixed sequence of existing `Effect` primitives with no player choices is JSON
IR; a new primitive or a branching resolution is either a new `Effect` variant (promote it
into the IR) or the escape hatch.

## Consequences

- **Easier.** Most cards are pure data — inspectable, serde-validated, no code. Adding an
  ordinary card is a JSON file; adding a mechanic is one enum variant plus its interpreter.
  The no-listener rule holds: triggers are a diff, abilities are data.
- **Harder / given up.** The closed enums bound expressiveness by construction. This is the
  binding constraint on catalog growth, and it is deliberate: coverage is limited by what the
  vocabulary can *say*, not by authoring throughput, so growing the vocabulary is the primary
  engine workstream. Each new primitive is one variant plus every exhaustive match that
  consumes it, across the resolver and the server's rules-text formatter.
- `apply_action` and `valid_actions` take a `&CardDatabase` parameter, because trigger
  collection and cost checks read card data. Storing the database inside `GameState` was
  rejected: it would compromise the value type's `Eq` and purity.
