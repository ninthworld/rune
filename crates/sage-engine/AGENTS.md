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
  Shared validators live in `src/catalog/` so build-time and load-time checks agree.
- **Automation policy belongs to the server, not here.** The engine may expose pure rules
  *predicates* (`automation.rs`); the loop, the per-seat preferences, and the pacing
  decisions live in the room layer (ADR 0010). This seam is the reason the engine stays
  sustainable — do not move policy across it.

## The IR is the constraint

Catalog coverage is limited by what the ability IR can *express*, not by authoring
throughput. Today `Cost` says tapping, mana, and loyalty, so an activation cost paid by
sacrificing or discarding is unwritable. `TriggerCondition` observes zone changes (its own
source's and, through `ObservedPermanent`, another permanent's), attack declaration, life
gain, casting, and step boundaries — but not an activation, and its selectors filter by
subtype, controller, and token-ness only. A condition *is* attachable now, as
`Effect::Conditional`, and `Condition` names four questions: a permanent count, a mill by
this resolution, a discard by this resolution, and life gained this turn. It is judged as
the effect is reached (CR 608.2), which is an if-clause on an effect rather than the
CR 603.4 trigger check. Every question but the count reads recorded events over a window —
the resolution, or the turn — because none of them can be answered from a snapshot.
An effect's amount may scale with a count of permanents (`count_of`); cards in a zone, life
totals, and mana values feed nothing.

**`data/exclusions.json` is the maintained list, and it is the one that has to stay
right.** Every exclusion names a single blocker; `make compat` regenerates
`docs/generated/compatibility.md` from it, and `cargo test` fails if the committed report
has drifted. Prose in an `AGENTS.md` or in the brief drifts silently — this file does not,
so when a paragraph here and an entry there disagree, believe the entry and fix the
paragraph.

Combat restrictions are a second layer-6 vocabulary beside `Keyword` (`CombatRestriction`):
they are not keyword abilities, some carry a parameter, and each is enforced in exactly one
place — the attacker candidate set, the blocker candidate set, the pairwise block check, or
the whole-selection block check. A restriction that can only be judged over the assembled
declaration must also be stated in the blocker slot's prompt, or it reaches the player as a
submit that silently does nothing. Attack and block *requirements* ("attacks each combat if
able") are still unmodeled, and a blocker still blocks exactly one attacker.

**Not every permanent is a card** (ADR 0015). `Permanent.printed` is a `Printed` — a
catalog `CardId`, or the `TokenData` an effect gave a token (CR 111) — and every read of a
permanent's printed face goes through `Printed::face(db)`, which answers both. The one
accessor that crosses back to card identity, `Printed::card()`, returns `None` for a token,
and that `None` is where CR 111.7 lives: a token leaving the battlefield has no
`CardInstance` to put in the destination zone, so it is put nowhere and ceases to exist.
A token's *death* is therefore observed from the recorded `PermanentDied` event rather than
from a graveyard it never reaches. `TokenData` has no `functional_id` field at all, which
is why a token cannot reach the compatibility report.

**A planeswalker's loyalty is counters, and an attack names a target** (ADR 0016).
`CounterKind::Loyalty` is what a planeswalker enters with (CR 306.5b, applied at the
battlefield-entry seam), what `Cost::Loyalty` spends, what damage removes
(CR 120.3c — `deal_damage_to_permanent` is the one seam that decides marking versus
loyalty), and what CR 704.5i reads at zero. `is_loyalty_ability` carries the two CR 606.3
timing rules, gated in the offer *and* re-derived in `apply_action`. `Attack.defender` and
`Permanent.attacking` are an `AttackTarget` — a player or a planeswalker — so
"what is attacked" (`attack_target_of`) and "who declares blockers" (`attacking_defender_of`,
which resolves a planeswalker's controller) are separate questions.

**An emblem is in no zone and is never removed** (ADR 0017). `GameState::emblems` is a
*second source list* both ability paths walk — `characteristics::static_ability_effects` and
`triggers::collect_triggers` — and nothing else in the engine reads it. Neither list's
position decides anything: every contribution is timestamped by its source's object id and
the caller sorts by that. `AbilitySource` says what an ability on the stack came from, and its
`permanent()` answering `None` is what makes an emblem need no special case in a
self-referential effect. Do not put an emblem on the battlefield: every state-based action,
every target spec, and every combat gate would then need a clause saying why it does not
apply, and *saying nothing* is the correct answer.

**One effect may declare more than one target** (ADR 0017). `Effect::target_group` returns
`{spec, min, max}`; `min == 0` is the "up to N" shape, and a group with `min == 0` is never a
reason to withhold an offer. At most **one** variable-arity group per ability or spell — the
stored target list is flat, and the validator enforces the limit so the pairing back onto
effects is exact rather than a guess.

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
