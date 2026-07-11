# ADR 0010: Computed characteristics and the continuous-effects (layers) seam

- Status: accepted
- Date: 2026-07-11
- Issue: #52

## Context
A permanent's *current* characteristics — its power/toughness, types, colors,
abilities — are not what is printed on the card. Counters, anthems, pump spells,
and type-changing effects all alter them continuously (CR 613, the layer
system). The engine has always promised to compute these on demand rather than
store them: `card_type.rs:6-7` says "type-changing continuous effects (the layer
system, later) derive a permanent's current types from these [printed types],"
`docs/brief.md` ("Continuous effects / Layer system") specifies a pull-based
`characteristic(state, permanent_id)` that "runs the layer system fresh on every
call," and `state.rs:36-40` records the invariant that nothing derivable is
stored on `GameState`.

That function does not exist yet, and today's state cannot express any of its
inputs:

- `Permanent` stores only `id / card / controller / tapped` (`state.rs:24-34`).
  It has no power/toughness and no counters.
- The protocol has already settled the *wire* shape: `PermanentView.counters` is
  a `Vec<Counter>` (`{ kind, count }`) in `rune-protocol`, and the server
  serializes it as always-empty (`view.rs:275`, "counters: Vec::new()"). The
  engine representation that would fill it is undecided.
- Rules code reads printed `CardData` directly for type and P/T questions —
  `is_land`/`is_creature` call `db.card(..).has_type(..)` (`lib.rs:366-375`).
  That is correct only while no effect can change a permanent's characteristics.
  The first `+1/+1` counter, anthem, or pump spell makes it wrong.

This is the #2 structural gap flagged by the 2026-07-11 sustainability review
(after targeting, ADR 0009). Deciding the computed-characteristics discipline
*before* rules code proliferates direct printed-value reads is far cheaper than
retrofitting it afterward. This ADR fixes the seam; it ships no code (issue #52
is docs-only). Implementation is filed as the follow-ups listed below.

The hard rules constrain the design (`AGENTS.md`, `crates/rune-engine/AGENTS.md`,
ADR 0002, ADR 0007):

- The engine is pure; `GameState` is a `Clone`/`Eq` value type with **no cached
  derivations** (`state.rs:36-40`).
- **No listeners or observers.** Continuous effects are recomputed from current
  state on every query, never pushed onto stored characteristics.
- No wall-clock time and no ambient state; any ordering signal (timestamps) must
  derive from data already in `GameState`.

## Decision

### 1. Store vs. compute
`Permanent` stores only **raw, non-derivable** state: its identity, its card, its
controller, its tapped status, and — added by the follow-ups — its **counters**
and any per-permanent raw inputs a continuous effect needs (e.g. a stored
timestamp; see §4). A permanent's **current characteristics are never stored.**
Power/toughness, current types/subtypes, colors, and the current ability set are
all *computed*.

Counters are stored because they are raw input, not a derivation: nothing else in
state determines how many `+1/+1` counters a permanent has. The "no cached
derivations" invariant forbids storing *computed* characteristics; it does not
forbid storing counters.

### 2. One pure read path
All current-characteristics reads go through a single pure function:

```
characteristics(&GameState, PermanentId, &CardDatabase) -> Characteristics
```

`Characteristics` is a value type holding the fields rules code needs (types,
supertypes, subtypes, mana cost, power, toughness, abilities). The function runs
the layer system fresh on every call and caches nothing (consistent with
`state.rs:36-40`). It takes `&CardDatabase` for the same reason `apply_action`
does (ADR 0007): the printed seed values live in the database, which is kept out
of `GameState` to preserve the value type's `Eq`/purity.

### 3. Layer subset shipped first
The full CR 613 order is copy → control → text → type → color → ability-adding →
power/toughness (`docs/brief.md`). We ship the **power/toughness end first**,
because that is what the earliest cards (counters, anthems, pumps) need, in three
slices:

1. **Printed values** — `characteristics()` returns the permanent's printed
   `CardData` as its current characteristics. Establishes the read path and the
   §5 invariant with no behavior change. (#66)
2. **Counters** — `+1/+1` / `-1/-1` counters fold into power/toughness at CR 613
   **layer 7c**. (#67, wire projection #68)
3. **Simple static P/T modifications** — anthem-style continuous "+X/+Y"
   effects, also layer 7c, applied after counters in timestamp order. (#69)

Layers 1–6 (copy, control, text, type, color, ability-adding) are deliberately
out of this first subset; each is added later behind the same function signature,
so callers never change as layers are filled in.

### 4. Layer ordering and timestamps in state
Within a layer, effects apply in **timestamp order** (CR 613.7). Timestamps must
be derived from data already in `GameState` — no wall-clock, no ambient counter
outside the state. The monotonic `GameState::next_object_id` (`state.rs:66-69`,
minted by `mint_id`) is the project's existing source of strictly increasing,
replayable ordering; a continuous effect's timestamp is the object id assigned
when it was created (the id of the permanent or stack object that produced it).
Counters within a permanent are order-independent (they sum), so slice 2 needs no
timestamp; slice 3 introduces the timestamp field on the stored effect input when
the first genuinely order-sensitive modifier lands. The exact representation of a
static effect (an entry keyed by source id, its layer, and its timestamp) is
settled in the slice-3 follow-up (#69) under this ADR.

### 5. The invariant
**Rules code never reads printed `CardData` for a permanent's current
characteristics — always through `characteristics()`.** Printed `CardData` is the
*seed* the layer system reads; it is not the answer to "what is this permanent's
power right now." The direct printed-value reads that exist today for battlefield
permanents (`is_land`/`is_creature`, `lib.rs:366-375`) are migrated to the
computed path as the layers that could change those characteristics land. Reading
`CardData` directly stays legitimate for a card **outside** the battlefield (in
hand, on the stack, in a graveyard), where no permanent and no continuous effect
exist yet.

## Consequences
- **Easier:** the first counter/anthem/pump becomes implementable, and every
  future characteristic-changing mechanic slots into one pure function with a
  fixed signature instead of touching every call site. The pull-based, no-listener
  discipline (ADR 0002, ADR 0007) extends cleanly: characteristics are a query,
  never a stored reaction. Undo/replay/resync stay free because nothing derived is
  persisted. The already-settled `PermanentView.counters` wire shape gets a real
  source with no protocol change.
- **Harder / given up:** `characteristics()` recomputes on every query, so hot
  paths (e.g. `valid_actions`, state-based actions) pay repeated layer-system cost
  rather than reading a cached field — an accepted trade for purity and
  correctness, matching the engine's existing recompute-everything stance. Call
  sites that read printed types/P/T for permanents must be migrated to the
  computed path as the relevant layers land, and reviewers must guard against new
  direct printed-value reads for battlefield permanents (§5).
- **Deferred:** layers 1–6 (copy, control, text, type, color, ability-adding);
  loyalty and other counter kinds beyond `+1/+1` / `-1/-1` P/T effects;
  characteristic-defining abilities; and the effects that *create* counters and
  static modifiers (authored as cards/abilities later). Targeting is a separate
  decision (ADR 0009); replacement effects remain the existing no-op pipeline slot
  (`apply_replacements`, `lib.rs:377-379`) and get their own future ADR.

## Follow-up issues
- #66 — engine: computed characteristics read path (printed-values layer).
- #67 — engine: store counters on `Permanent` and fold them into computed P/T.
- #68 — server: project engine counters into `PermanentView.counters`.
- #69 — engine: simple static P/T modifications with timestamp ordering (layer 7c).
