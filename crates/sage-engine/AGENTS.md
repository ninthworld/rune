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
throughput. Today `Cost` says tapping, mana, loyalty, spending the source itself, removing
counters from it, and the two whose payment the **player picks** — sacrificing one permanent
they control (filtered by card type, by subtype, and optionally excluding the source) and
discarding cards. A picked payment rides on the action, in `Action::ActivateAbility`'s
`payment` list, exactly as a cast's additional cost rides on `Action::CastSpell`'s; mana
never does, because an activation pays it from the pool. Exiling a card as a cost,
sacrificing two permanents, and *optional* non-mana costs are all still unwritable.
`TriggerCondition` observes zone changes and attack
declarations (its own source's and, through `ObservedPermanent`, another permanent's), a
draw by its controller, an activation that uses the stack — never a mana ability, which
uses none — life gain, casting, and step boundaries; its observed-permanent selectors filter
by subtype, controller, token-ness, power, and keyword. A condition *is* attachable now, as
`Effect::Conditional`, and `Condition` names four questions: a permanent count, a mill by
this resolution, a discard by this resolution, and life gained this turn. It is judged as
the effect is reached (CR 608.2), which is an if-clause on an effect rather than the
CR 603.4 trigger check. Every question but the count reads recorded events over a window —
the resolution, or the turn — because none of them can be answered from a snapshot.
A count of permanents (`count_of`) may feed an effect's amount, the number of tokens it
creates, and an attachment's static grant — the last recalculated on every read, because a
static ability is not a resolution. Every *other* X is a `DerivedAmount`, a closed set of
four phrases with no arithmetic over them — the life gained this turn, a count of what
this resolution milled, the greatest mana value among a class, and the **X its controller
announced** — read once where the effect applies and feeding three verbs, a pump, a draw,
and a damage. The count keeps its own spelling because it is the one source a static grant
may also name; nothing windowed over events could stand there. Cards in a zone, a life
total, one named object's mana value, another object's power, and half of anything still
feed nothing.

**A choice made at announcement rides the action, and the mode is made first** (CR
601.2b). `Action::CastSpell` carries a `mode` and an `x`, both cleared to build the
requirement form. The ordering is structural rather than remembered: `target_requirements`
reads the mode off the action, and a modal cast that has not chosen one declares **no slots
at all** — which is why `announcement_is_legal` refuses that announcement explicitly
instead of letting it pass as a spell that happened to target nothing. A modal card's
effects live in `CardData::modes`, held between two and `MAX_MODES` (four, because a mode is
a numbered dock row and a fifth would have to be truncated). **X is announced, then
locked**: `cast_cost` is the one place it becomes generic mana, `StackObjectKind::Spell::x`
is the only X anything reads afterwards, and `x_options` enumerates the legal values *with
what each one costs* — the multiplication is the engine's. Both are re-derived
independently in `apply_action`, so a forged mode and an unpayable X are refused rather
than merely unoffered. A `SpellTrait` is what is true of a spell *on the stack* rather than
what it does, which is why it is not an `Effect`: both members are read by somebody else's
resolution — a counterspell (CR 701.5a) and the damage seam (CR 615.1,
`PendingDamage::unpreventable`). What a resolution knows about itself now travels as one
`Resolution` value: its log window, its announced X, and that declaration.

**`data/exclusions.json` is the maintained list, and it is the one that has to stay
right.** Every exclusion names a single blocker; `make compat` regenerates
`docs/generated/compatibility.md` from it, and `cargo test` fails if the committed report
has drifted. Prose in an `AGENTS.md` or in the brief drifts silently — this file does not,
so when a paragraph here and an entry there disagree, believe the entry and fix the
paragraph.

**Layer 6 subtracts as well as adds**, and is therefore ordered by timestamp (CR 613.1f):
a grant after a removal grants, a removal after a grant removes. `alter_abilities_self` is
the one verb that subtracts — it names its own source, loses named keywords or *all*
abilities until end of turn, and reaches no target and no class. Losing all abilities is
answered by `characteristics::loses_all_abilities`, a stored-effects-only predicate read
from inside `abilities_of_permanent`; that accessor takes `&GameState` for exactly this
reason and is the only path any collector uses, so there is no printed-abilities reader to
pick by mistake.

**A rule modification is in no layer, and that is the point** (`Modification::ModifyRule`,
`RuleModification`). CR 613 orders effects that change *characteristics*; these change
none, so nothing folds them in and each is read where its rule is asked —
`assigns_combat_damage_by` at the one place the combat-damage step computes an amount (so
trample's excess and the marked damage CR 704.5g reads follow it for free),
`attacks_as_though_no_defender` at the attacker declaration. Assigning by toughness is
**not** a P/T change and attacking as though it had no defender is **not** `LoseKeyword`
(CR 609.4): the power and the keyword are both untouched, which is what keeps the creature
inside the `keyword`-filtered class that granted it either one. That filter reads the
**printed** face inside the layer system — asking the layer-6 fold for the set it is
producing would not terminate — while the trigger selector's runs outside it and reads the
computed set.

Combat restrictions are a second layer-6 vocabulary beside `Keyword` (`CombatRestriction`):
they are not keyword abilities, some carry a parameter, and each is enforced in exactly one
place — the attacker candidate set, the blocker candidate set, the pairwise block check, or
the whole-selection block check. A restriction that can only be judged over the assembled
declaration must also be stated in the blocker slot's prompt, or it reaches the player as a
submit that silently does nothing. One member of the vocabulary is a *permission* rather
than a restriction — `CanBlockAdditional`, which lifts the CR 509.1a default that a blocker
blocks one attacker — so `Permanent.blocking` is an ordered list, and its order is the
blocker's CR 509.3 damage assignment order, carried by the declaration that named them.
One member is a **requirement** rather than either — `MustBeBlockedByAllAble` — and it is
the only rule in the engine that refuses a declaration for what it *omits*: CR 509.1c asks
for the maximum number of requirements obeyable without violating a restriction, which is a
fact about the declarations that were not submitted. That is why it is a search
(`combat::requirements`) run last, over declarations every other gate has already called
legal — so a restriction beats a requirement, and a requirement no legal declaration can
meet is simply not met, with no clause anywhere saying so. **Attack** requirements
("attacks each combat if able", CR 508.1d) remain unmodeled.

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

**One effect may declare more than one target, and its slots need not share a spec**
(ADR 0017 §5, ADR 0004). `Effect::target_groups` returns an ordered `Vec<{spec, min, max}>`;
`min == 0` is the "up to N" shape, and a group with `min == 0` is never a reason to withhold
an offer. At most **one** variable-arity group per ability or spell — the stored target list
is flat, and the validator enforces the limit so the pairing back onto effects is exact
rather than a guess. `Effect::Fight` is the one effect with *two* groups, one spec each, and
an effect with more than one group acts on all of its slots or on none (CR 701.12c) rather
than doing as much as it can. It is also the one damage whose **source is a permanent**, so
it is the one place outside combat where deathtouch and lifelink apply.

`Ability::Static` exists and covers anthems and lords ("creatures you control", optionally
filtered to a printed subtype or a printed keyword, optionally excluding the source), and
its `as long as …` asks one of three questions — a permanent count, whether the source is
attacking, and whether anything is attached to it. It is **derived, never stored**:
`characteristics` reads it off the battlefield on every call, so the effect begins and ends
with its source's presence and nothing enters `GameState::static_effects`. Extend
`StaticAffects` when a card needs a scope it cannot name.

**A continuous ability is one of three kinds, and its subject decides which**: a `Static`
modifies permanents at a CR 613 layer, a `PlayerStatic` states something about a person, and
a `CostModifier` changes what a class of **spell** costs its own controller to cast
(CR 601.2f) — not a layer, since it applies before the spell's object exists. Its arithmetic
is the rule's own (plus every increase, minus every reduction, one floor at `{0}`) and only
the generic component moves. It is the one continuous ability with several readers, so it has
exactly one seam: `total_cast_cost` is what the offer, the pips, the payment search, the
legality gate, the charge, and the view all ask, and the idle predicate joins them by asking
`valid_actions` rather than a cost. Wiring it into fewer than all of those advertises casts a
seat cannot take, or auto-passes a seat that has a play.

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

**A choice a permanent makes as it enters is the same queue, and the permanent waits off the
battlefield for it** (ADR 0013 §8). `Ability::EntersChoosingColor` is a card's declaration
that its controller names a colour as it arrives (CR 614.12); `put_card_onto_battlefield`
reads it, and when it is there the entry is *deferred* — the card is put on the choice queue
as a `ColorOutcome::RecordOnEntry(PendingEntry)` and returns no `PermanentId`, so nothing is
on the battlefield to be caught mid-decision. Answering completes the entry through
`complete_battlefield_entry`, the one function both roads take, and the answer lives on
`Permanent::chosen_color` — stored, written once, and read back by
`ObservedSpell::ChosenColor`. Naming a **type** or a **card** is still unwritable, and nothing
records a choice on a spell.

**A replaceable event is a value, and there is exactly one road onto the battlefield**
(ADR 0019). `PendingEntry` describes an arrival *before it happens* — the object, its
controller, the tapped state and counters it would carry, and whether it got there by being
cast — and `begin_battlefield_entry` is the only place a `Permanent` is born: a land played,
a token created, a permanent spell resolving, and a card an effect put there all build one.
Applicable replacements are **derived** from two source lists (the entering object's own
`EntersTapped`/`EntersWithCounters`, and the one-shot `GameState::replacements` an ability
created for the turn), the affected object's **controller** orders them when more than one
applies (CR 616.1, through the same choice queue), and `PendingEntry::applied` is what stops
any of them applying twice (CR 614.5) — which is also what makes the loop terminate.
Applying either modifies the event or replaces it outright, and answering an ordering
question re-enters the same function. `EntersChoosingColor` is not collected here — it is a
question, not a modification to order.

**Damage is the second replaceable event** (CR 615). `PendingDamage` is the value — the
recipient, the amount, whether it is *combat* damage — and `GameState::deal_damage` is the
one seam anything deals damage through, so a prevention shield covers combat, a burn spell,
and a fight in one place; prevented damage is never marked, never lethal, never life loss,
and gains a lifelink source nothing. A shield is **not** one-shot: it lives on
`GameState::prevention` and ends in the cleanup step beside the pumps (CR 514.2), and none
is ordered against another because every one modeled prevents all of it. A permanent
leaving the battlefield, a draw, and life gained route nowhere near this, and the leave
seams run inside the SBA loop where there is nothing to suspend a question onto.

A choice asks one shape of **question** (`ChoiceQuestion`, ADR 0014): pick cards, answer a
`you may` yes-or-no, name a colour, or order applicable replacements by position in a
derived list. Everything around them is single — one queue, one chooser, one
`Resume` — and only the answer branches, so a new question shape is a variant plus its own
`Action`, never a second queue. An accepted optional effect is *spliced onto the front of
the remainder*, not applied on the spot; declining is the same path with nothing spliced,
which is why "a decline leaves the game as if the effect were absent" needs no proof. An
optional **cost** is mana, charged from the chooser's pool, and is the one place mana
moves outside the cast path: while such a question is owed its chooser may activate mana
abilities (CR 605.3a) and nothing else. Whether it is *posed* is judged against the mana
the board could still make (`potential_mana_pool`, shared with the idle-seat predicate);
whether accepting is *legal* is judged against the pool as it stands.

**Control is CR 613 layer 2, and it is computed** (ADR 0005 §1/§3). `Permanent::controller`
is the *base* controller, not the answer: every rule that asks who controls a permanent —
attacking, activating, `creatures you control`, combat damage, the untap step — goes through
`characteristics::controller_of`, which folds `Modification::GainControl` over the stored
field. Reading the field directly is right **only** for a question about *ownership*, which
today means the four battlefield-departure seams: because nothing overwrites it, a creature
that dies while stolen goes to its own graveyard (CR 400.7) with no ownership model needed.
A control change restamps `entered_turn` (CR 302.6), which is why a card that steals a
creature to attack with also grants it haste.

The catalog was selected as cards this vocabulary can say, so the empty `scripted.rs` table
is not evidence of expressiveness. Growing the vocabulary is the primary engine workstream;
each new primitive is one enum variant plus every exhaustive match that consumes it, across
the resolver and the server's rules-text formatter — both are wildcard-free, so the compiler
names every site.

## Commands

- `cargo test -p sage-engine`
- `cargo clippy -p sage-engine --all-targets -- -D warnings`
