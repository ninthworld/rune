# ADR 0010: Computed characteristics and continuous-effect layers

- Status: accepted
- Date: 2026-07-11
- Issue: #52

## Context

A permanent’s current characteristics can differ from its printed card because of counters,
continuous effects, copies, and type or ability changes. Storing derived values would allow
them to drift from their inputs and would complicate replay and state equality.

## Decision

`Permanent` stores only non-derivable facts such as identity, card handle, controller, tapped
state, counters, attachments, marked damage, and timestamps required by rules ordering.

Rules code reads a battlefield permanent’s current values through the pure function:

```rust
characteristics(&GameState, PermanentId, &CardDatabase) -> Characteristics
```

The function starts from the functional definition and applies supported continuous effects
in rules order. It caches nothing. Reading `CardData` directly is valid for a card outside the
battlefield or as the printed seed, not as the answer to a permanent’s current characteristics.

The first implemented layer subset covers printed values, counters, and simple timestamped
power/toughness modifications. Additional copy, control, text, type, color, and ability layers
extend the same read path as their mechanics are implemented. Ordering data comes from
deterministic game-state identity, never wall-clock time.

## Consequences

Every rules consumer and server projection receives consistent computed values, and replay or
simulation stores no derived cache. Recalculation costs more than field access, and new layer
support must be applied centrally and tested against its Comprehensive Rules ordering.
