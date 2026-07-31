# Engine agent guide

`sage-engine` is the pure, deterministic rules state machine. Read the engine section of
[`docs/brief.md`](../../docs/brief.md) before changing it.

## Hard rules

- Pure functions over immutable `GameState`. `apply_action` clones; it never mutates.
- **No dependencies on tokio, networking, timers, threads, wall-clock time, or randomness
  without an injected seed.** Runtime dependencies are limited to serde for embedded card
  data; adding another dependency requires an ADR.
- No listeners/observers. Triggers are collected by diffing before/after states.
  Characteristics are computed fresh by pure functions (layer system), never cached.
- Every permanent gets a fresh `PermanentId` on battlefield entry — zone-change identity
  is the mechanism, do not add zone-change counters.
- Every rules behavior change ships with tests in the same PR. Rules bugs get a
  regression test named after the issue (`issue_123_...`).
- Cards are **data**, authored against [`docs/card-schema.md`](../../docs/card-schema.md):
  a functional definition per card under a stable `functional_id`, no presentation
  assets (the schema rejects them structurally), and code-defined behavior only via the
  declared `scripted` escape hatch.
- **Never write a `CardId` down.** `build.rs` assembles `data/catalog/` + `data/sets/`
  at compile time and interns `CardId(0..n)` from the sorted `FunctionalId`s (ADR 0008
  §3), so authoring one card renumbers its neighbours. Name a card by its
  `functional_id` and resolve the handle (`CardDatabase::card_id`) — in decklists, in
  `scripted.rs`, and in tests (`crate::fixtures::fixture`).
- `build.rs` may read catalog files at compile time. The running engine performs no I/O.
  Shared validators live in `src/catalog.rs` so build-time and load-time checks agree.
- **Automation policy belongs to the server, not here.** The engine may expose pure rules
  *predicates* (`automation.rs`); the loop, the per-seat preferences, and the pacing
  decisions live in the room layer (ADR 0010). This seam is the reason the engine stays
  sustainable — do not move policy across it.

## The IR is the constraint

Catalog coverage is limited by what the ability IR can *express*, not by authoring
throughput. Today `Cost` says tapping and mana and nothing else, so an activation cost
paid by sacrificing or discarding is unwritable. `TriggerCondition` observes zone changes,
attack declaration, life gain, casting, and step boundaries — but nothing may be *attached*
to a trigger or an effect, so an intervening-if clause ("at the beginning of your upkeep,
**if** you control a creature with power 4 or greater") is unwritable, and that one gap
blocks more real cards than any other. Effect amounts are fixed numbers, never counted
from the board. Read `data/exclusions.json` for the maintained list.

Combat restrictions are a second layer-6 vocabulary beside `Keyword` (`CombatRestriction`):
they are not keyword abilities, some carry a parameter, and each is enforced in exactly one
place — the attacker candidate set, the blocker candidate set, the pairwise block check, or
the whole-selection block check. A restriction that can only be judged over the assembled
declaration must also be stated in the blocker slot's prompt, or it reaches the player as a
submit that silently does nothing. Attack and block *requirements* ("attacks each combat if
able") are still unmodeled, and a blocker still blocks exactly one attacker.

`Ability::Static` exists and covers anthems and lords ("creatures you control", optionally
filtered to a subtype, optionally excluding the source). It is **derived, never stored**:
`characteristics` reads it off the battlefield on every call, so the effect begins and ends
with its source's presence and nothing enters `GameState::static_effects`. Extend
`StaticAffects` when a card needs a scope it cannot name; a cost reducer still needs a
`Modification` variant that no layer implements yet.

**A mid-resolution player choice is queued state, never a flag** (ADR 0013). An effect that
asks a player to choose cards (discard, scry, look at the top N, search) pushes a
`PendingChoice` onto `GameState::pending_choices` and *suspends* the resolution, carrying the
rest of it — remaining effects, remaining targets, and the spell's final zone — in the
choice's `Resume`. Whether a choice is owed is derived (`pending_player_choice`), and so are
the cards it offers (`choice_candidates`); nothing snapshots a candidate list. A choice whose
clamped maximum is zero is applied outright instead of posed, which is the whole of the
never-stall guarantee. Priority goes to the chooser and returns via the one
`interrupted_priority` slot shared with trigger aiming — a third interrupting choice must
join that check rather than add a second slot.

A choice asks one of two **questions** (`ChoiceQuestion`, ADR 0014): pick cards, or answer
a `you may` yes-or-no. Everything around them is single — one queue, one chooser, one
`Resume` — and only the answer branches, so a new question shape is a variant plus its own
`Action`, never a second queue. An accepted optional effect is *spliced onto the front of
the remainder*, not applied on the spot; declining is the same path with nothing spliced,
which is why "a decline leaves the game as if the effect were absent" needs no proof. An
optional **cost** is mana, charged from the chooser's pool, and is the one place mana
moves outside the cast path: while such a question is owed its chooser may activate mana
abilities (CR 605.3a) and nothing else. Whether it is *posed* is judged against the mana
the board could still make (`potential_mana_pool`, shared with the idle-seat predicate);
whether accepting is *legal* is judged against the pool as it stands.

The catalog was selected as cards this vocabulary can say, so the empty `scripted.rs` table
is not evidence of expressiveness. Growing the vocabulary is the primary engine workstream;
each new primitive is one enum variant plus every exhaustive match that consumes it, across
the resolver and the server's rules-text formatter — both are wildcard-free, so the compiler
names every site.

## Commands

- `cargo test -p sage-engine`
- `cargo clippy -p sage-engine --all-targets -- -D warnings`
