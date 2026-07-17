# ADR 0007: Hybrid declarative card-effect IR

- Status: accepted; identity and catalog layout amended by ADR 0018
- Date: 2026-07-10

## Context

Card behavior must be executable, serializable as embedded data, compatible with immutable
`GameState`, and expressive enough for unusual cards. A listener-based class per card would
conflict with the engine’s pull-based state model, while a universal data language would grow
awkward for exceptional behavior.

## Decision

Common card behavior uses a closed declarative IR in
`crates/rune-engine/src/ability.rs`: abilities, costs, trigger conditions, targets, effects,
keywords, and Aura grants. Pure engine functions interpret those values.

Behavior the IR cannot express may use the pure code escape hatch in `src/scripted.rs`. The
escape hatch is keyed by stable `FunctionalId`, stores no closures or function pointers in game
state, and supplies explanatory rules text beside scripted behavior. Catalog validation requires
the data declaration and code registration to agree.

Mana abilities are identified from their structured costs and effects and resolve without the
stack. Other spells and abilities use the normal stack and priority rules. Trigger conditions
are evaluated from explicit state transitions rather than registered listeners.

Add ordinary fixed behavior to the data IR. Add a reusable IR variant when multiple cards need
a new primitive. Use scripted behavior only for genuinely exceptional semantics.

## Consequences

Most cards remain inspectable, validated data, and new mechanics extend one interpreter rather
than many card classes. The closed enums require deliberate additions and exhaustive handling;
the scripted seam adds maintenance and text obligations but preserves engine purity.
