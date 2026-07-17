# ADR 0007: Card effects as a hybrid declarative IR with a code escape hatch

- Status: accepted
- Date: 2026-07-10
- Issue: #N

## Context
The engine could resolve a `CardId` to vanilla characteristics (name, type line,
mana cost, oracle text, power/toughness) but had no way to represent a card's
*behavior* — abilities, effects, triggers. That is the next foundation: every
downstream feature (the stack, mana, combat, the layer system) needs a way for a
specific card to say "when this enters, draw a card" or "{T}: add {G}".

The hard architectural rules constrain the solution (`docs/brief.md`,
`crates/rune-engine/AGENTS.md`):

- The engine is pure and immutable; `GameState` is a `Clone`/`Eq` value type.
- **No listeners or observers.** Triggered abilities are found by a pure diff of
  the states before and after an action; continuous effects are recomputed on
  demand. Nothing reacts to events; everything is pulled from current state.
- No new runtime dependencies. serde is permitted *only* for deserializing the
  bundled, compile-time-embedded card snapshot (ADR 0006).

xMage's model — one imperative class per card that registers event listeners — is
push-based and therefore incompatible with these rules. What transfers is its
*data/code split*: metadata lives in a flat file, behavior lives elsewhere.

The realistic options for the behavior side:

1. **Pure data-driven IR/DSL.** All behavior is data; never any per-card code.
   Maximizes card-as-data but forces the IR to encode every MTG mechanic, and
   complex one-offs become awkward encodings.
2. **Code-per-card.** Each card is a Rust module of pure functions. Most flexible
   for oddballs, but thousands of files, more boilerplate, and behavior stops
   being uniformly data-inspectable.
3. **Hybrid.** A declarative IR for the common vocabulary, plus a code escape
   hatch for the cards the IR can't express.

## Decision
Card behavior is a **closed, declarative IR carried on `CardData` as data**, with
a **`CardId`-keyed pure code escape hatch** for the rest.

- The IR lives in `crates/rune-engine/src/ability.rs`: `Ability`
  (`Activated`/`Triggered`), `Cost`, `Effect`, `TriggerCondition`. All are
  `Deserialize` data enums, so ordinary cards are authored as JSON in
  `data/cards.json` (the direct analog of xMage's `.txt`) via the serde path
  already sanctioned by ADR 0006 — **no new dependency**.
- Abilities are interpreted by **pure functions** over `GameState`. A triggered
  ability's condition is a data value evaluated by a `match` against a
  before/after diff (`condition_met`) — never a stored closure or listener.
- Whether an ability is a **mana ability is derived, not stored** (`is_mana_ability`):
  a mana ability resolves immediately and does not use the stack or change
  priority (CR 605.3); all other abilities and spells go on `GameState::stack`
  and resolve when players pass priority in succession.
- Fresh object identity comes from a **monotonic `GameState::next_object_id`**
  counter (`mint_id`), preserving "a fresh `PermanentId` on every battlefield
  entry" without reuse.
- The **escape hatch** is `crates/rune-engine/src/scripted.rs`:
  `scripted_abilities(CardId) -> Vec<Ability>`, unioned with the data-driven
  abilities by `card::abilities_of`. It stores nothing in state — behavior is
  re-derived from the `CardId` on demand, the same discipline as the layer
  system — so `GameState` keeps its `Clone`/`Eq` value semantics. A future card
  whose *resolution* the fixed `Effect` set can't express gets an `Effect`
  variant dispatched by a pure resolver keyed by an opaque tag; function pointers
  may be looked up but are never stored in state.

The IR-vs-hatch boundary: a fixed sequence of existing `Effect` primitives with no
player choices → JSON IR; a new primitive or branching resolution → a new `Effect`
variant (promote into the IR) or the escape hatch.

## Consequences
- **Easier:** most cards are pure data — inspectable, serde-validated, no code.
  Adding an ordinary card is a JSON entry; adding a new mechanic is one `Effect`
  (or `Cost`/`TriggerCondition`) variant plus its interpreter. The no-listener
  rule is upheld: triggers are a diff, abilities are data.
- **Harder / given up:** the closed `Effect`/`Cost` enums bound expressiveness by
  construction — genuinely novel cards need an enum addition or the escape hatch,
  not arbitrary inline logic. `apply_action` and `valid_actions` now take a
  `&CardDatabase` parameter, because trigger collection and cost checks read
  oracle data. Storing the database inside `GameState` was rejected: it would
  compromise the value type's `Eq`/purity.
- **Deferred:** targeting and the `requirements` prompt queue (the slice's cards
  need no targets), multi-trigger APNAP ordering, and the full layer system.
