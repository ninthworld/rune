# Card schema

SAGE cards are versioned functional definitions: structured, printing-independent data the
engine executes. The model is defined by [ADR 0008](decisions/0008-functional-card-definitions.md),
including the split between a functional definition and the printings that reference it.

The Rust authorities are `CardData` and `Printing` in
`crates/sage-engine/src/card.rs`; validators live in `src/catalog/`.

## File layout

- `crates/sage-engine/data/catalog/<functional_id>.json` contains one functional
  definition. The file stem must equal its `functional_id`.
- `crates/sage-engine/data/sets/<SET>.json` contains that set’s printing records. A
  printing refers to a functional definition and carries no rules.

`build.rs` discovers, validates, sorts, and embeds both directories at compile time. The
running engine performs no filesystem I/O.

## Functional definition

The bundled catalog's functional definitions are sourced from a real set (Core Set 2019);
see [ADR 0009](decisions/0009-real-functional-card-data.md). Only functional data is stored —
no Oracle text, flavor, art, or branding.

```json
{
  "schema_version": 1,
  "functional_id": "skyscanner",
  "name": "Skyscanner",
  "types": ["artifact", "creature"],
  "subtypes": ["Thopter"],
  "mana_cost": "{3}",
  "colors": [],
  "power": 1,
  "toughness": 1,
  "keywords": ["flying"],
  "abilities": [
    {
      "type": "triggered",
      "event": "self_enters_battlefield",
      "effects": [{ "kind": "draw_card", "count": 1 }]
    }
  ]
}
```

### Fields

| Field | Required | Meaning |
| --- | --- | --- |
| `schema_version` | yes | Must equal `sage_engine::SCHEMA_VERSION`, currently `1` |
| `functional_id` | yes | Stable lowercase `snake_case` identity; must match the file name |
| `name` | yes | Display name |
| `types` | yes | One or more card types such as `creature`, `land`, or `instant` |
| `supertypes` | no | Values such as `basic` or `legendary` |
| `subtypes` | no | Printed subtype names such as `Elf` or `Aura` |
| `mana_cost` | yes | Curly-brace notation; empty when the card has no mana cost |
| `colors` | no | Explicit card colors; empty means colorless |
| `power`, `toughness` | conditional | Both required for creatures and forbidden for non-creatures |
| `loyalty` | conditional | Required for planeswalkers and forbidden for everything else |
| `keywords` | no | Supported keyword abilities |
| `restrictions` | no | Printed combat restrictions; creatures only |
| `abilities` | no | Activated, triggered, or replacement-style ability IR |
| `spell_effects` | no | Resolution effects for instants and sorceries; empty on a modal card |
| `modes` | no | The bullets of a modal spell (CR 700.2); two to four, each `{ "effects": [...] }` |
| `spell_traits` | no | What is true of the card **as a spell on the stack** — can't be countered, damage can't be prevented |
| `additional_cost` | no | An additional cost to **cast** the card (CR 601.2b); never on a land |
| `attachment` | no | Aura or Equipment: what it may be attached to, its equip cost, and its static power/toughness, keyword, ability, and/or combat-restriction grant |
| `scripted` | no | Declares behavior implemented in `src/scripted.rs`; defaults to `false` |

Current keyword values are `flying`, `reach`, `vigilance`, `haste`, `defender`, `menace`,
`first_strike`, `trample`, `deathtouch`, `lifelink`, `double_strike`, `hexproof`,
`indestructible`, and `flash`.

`flash` (CR 702.8) is the one keyword that is not about a permanent at all — it is a static
ability of the **card**, and it stops mattering the instant that card becomes a permanent. It
lifts the sorcery-speed gate and nothing else, in the single timing predicate every casting
road asks (from hand, from a graveyard under a one-turn permission, from the command zone),
which both the offer and the apply-time re-check run.

`indestructible` (CR 702.12) is not a combat rule either, and not a targeting one: it is an
exception to **destruction**, enforced at the two places destruction happens — the CR
704.5g/704.5h state-based actions and the `destroy` effect. It is deliberately not an
exception to anything else: a creature at 0 or less toughness still goes to the graveyard
(CR 704.5f is not destruction), a planeswalker at zero loyalty still leaves (CR 704.5i), and
sacrifice, exile, and bounce are untouched.

`defender` (CR 702.3b) removes a creature from the attacker candidate set; `menace`
(CR 702.110b) is checked over the whole declare-blockers selection, since a lone blocker
is illegal precisely because it is alone. Both are read through the computed keywords, so
a granted one restricts exactly as a printed one does.

`hexproof` (CR 702.11b) is not a combat rule at all: it is enforced in `target_is_legal`,
the one predicate both the announcement gate and the CR 608.2b resolution re-check run.
It is controller-relative — an opponent may not target the creature, and its own
controller may.

### Granting keywords (continuous, CR 613.1f)

Effects can grant a keyword ability, applied at the layer system's ability-adding
layer 6 so a granted keyword is indistinguishable from a printed one everywhere
keywords are read (combat legality, evasion, damage, view projection, generated text):

- An **attachment** — an Aura or an Equipment — grants for as long as it is attached:
  list the keywords under `attachment.keywords`, e.g. an Aura granting flying is
  `"attachment": {"kind": "aura", "attach_to": "any_creature", "keywords": ["flying"]}`.
  The `power`/`toughness` and `keywords` grants are independent; either or both may be
  present.
- A **spell or ability** grants **until end of turn** with the `grant_keyword` effect,
  e.g. `{"kind": "grant_keyword", "target": "any_creature", "keyword": "trample"}`. The
  grant expires in the cleanup step (CR 514.2). Duplicate grants are redundant, not
  additive. Conditional grants are out of scope.
- A card that pumps **and** grants in one breath — `Target creature gets +2/+2 and gains
  flying until end of turn` — is **one** `pump` effect carrying `keywords`, never a
  `pump` beside a `grant_keyword`:

  ```json
  {"kind": "pump", "target": "any_creature", "power": 2, "toughness": 2,
   "keywords": ["flying"]}
  ```

  One effect declares one target group, so two effects would advertise two independent
  slots and let a player pump one creature while a different one gained flying. Author
  the two-effect form only when the card really names two targets.

  A `restrictions` list rides beside `keywords` on the same effect, for the same reason
  and with the same until-end-of-turn duration — including the one *requirement* in that
  vocabulary:

  ```json
  {"kind": "pump", "target": "any_creature", "power": 3, "toughness": 3,
   "restrictions": ["must_be_blocked_by_all_able"]}
  ```

### Granting a whole ability (continuous, CR 613.1f)

A keyword ability *is* an ability (CR 702.1), so granting one written out in full is the
same layer and the same mechanism — the ability is folded into the host's set by
`abilities_of_permanent`, the one accessor every collector reads, and is therefore offered
by `valid_actions`, paid for, put on the stack, and fired by exactly the code a printed
ability goes through. A granted **mana** ability is still a mana ability (CR 605.1a) and
still uses no stack, because that is derived from what the ability says rather than from
who granted it.

- An **attachment** grants for as long as it is attached, through `attachment.abilities`:

  ```json
  "attachment": {"kind": "aura", "attach_to": "any_land", "abilities": [
    {"type": "activated", "cost": [{"kind": "tap"}],
     "effects": [{"kind": "add_mana_any_color", "amount": 2, "same_color": true}]}]}
  ```

  One block for both kinds, so an Equipment grants an ability exactly as an Aura does.
- A **spell or ability** grants **until end of turn** through the `abilities` list on a
  `pump`, beside its `keywords` and `restrictions` and for the same reason — one effect
  declares one target group, and a card that says *gets +2/+0 **and** gains "…"* names one
  creature:

  ```json
  {"kind": "pump", "target": "any_creature", "power": 2, "toughness": 0, "abilities": [
    {"type": "triggered", "event": "self_dies",
     "effects": [{"kind": "return_self_from_graveyard",
                  "destination": "battlefield_tapped"}]}]}
  ```

Unlike a keyword grant this one is **not** idempotent: two grants of the same ability are
two abilities, because two Auras each saying `{T}: Add {G}` really are two activations.

A granted **dies** trigger is the one grant that outlives the grant. It fires on the way
out (CR 603.6c), read from the snapshot the permanent still existed in — and still carried
the grant in — so `return_self_from_graveyard` on such a trigger reaches the card the
permanent became. That is why a dies trigger's source records both halves of what the
object now is (CR 603.10a), and why a `self_dies` trigger is never treated as an ability
that *functions from* a graveyard: it functioned from the battlefield.

### Losing keywords and losing all abilities (continuous, CR 613.1f)

Layer 6 **subtracts** as well as adds. `alter_abilities_self` is the one verb that says
so, and its subject is the ability's own source — the way `pump_self` and `restrict_self`
name theirs:

```json
{ "kind": "alter_abilities_self", "lose": ["defender"], "gain": ["flying"] }
```

That is the whole of `{3}: Until end of turn, this creature loses defender and gains
flying` — **one** effect, not a removal beside a grant. Both halves are one printed
sentence about one permanent, so they share one CR 613.7 timestamp, and within the clause
the losses are applied before the gains.

- `lose` names keywords to remove. It removes a keyword however the permanent got it:
  by the time layer 6 applies, a granted keyword is indistinguishable from a printed one.
  Removing one it does not have does nothing.
- `gain` names keywords to add, exactly as `grant_keyword` does.
- `lose_all` (default `false`) is `loses all abilities`: every keyword, every combat
  restriction, and every printed static, triggered, and activated ability. A permanent
  under it offers no activation, fires no trigger, and contributes no continuous effect
  to anything — every collector that walks a permanent's abilities reads the same
  answer, so there is no path that sees a silenced ability.
- All three fields default, and a clause that says nothing is rejected by the catalog
  validator rather than minting a timestamp for an effect that changes nothing.

**Order is what decides a disagreement** (CR 613.1f). Between two clauses the later
timestamp speaks last: a grant after a removal leaves the keyword present, and a removal
after a grant leaves it absent. An Aura hung on a permanent *after* it lost all abilities
still grants what it grants, because the attachment's grant is timestamped by the
attachment. The whole effect is until-end-of-turn and the cleanup step removes it, at
which point the printed abilities are simply there again — nothing was ever taken off the
card.

What is **not** here: removal aimed at a target or a class (the verb names its own
source), removal that outlives the turn, and "attacks as though it didn't have defender",
which is not removal at all — the keyword stays and one rule stops applying to it.

### Additional costs (CR 601.2b)

A cost the card's own text adds to casting it, beyond its mana cost:

```json
"additional_cost": { "kind": "discard", "count": 1 }
```

A cost is **not** an effect, and authoring one as the other changes what the card does.
A cost gates the cast: a spell whose additional cost cannot be paid is never offered,
and it is paid as the spell goes on the stack (CR 601.2h) rather than on resolution — so
it cannot be countered away, cannot be responded to, and cannot be skipped by a player
with an empty hand. What pays it is chosen from a hand, or a battlefield, that no longer
offers the spell itself.

The kinds today are `discard` and `sacrifice`:

```json
"additional_cost": { "kind": "sacrifice", "card_type": "creature" }
```

A sacrifice takes permanents of the named type. **Whose permanent stays a rule
rather than a field** — CR 701.17b lets a player sacrifice only what they control, so
there is no scope to author and none to get wrong. How *many* is a field, because a printed
card varies it in both directions:

```json
"additional_cost": { "kind": "sacrifice", "card_type": "land", "count": "any" }
```

`count` is `{"exactly": n}` or `"any"`, and defaults to exactly one. **`"any"` is the one
cost whose size is a decision**: a payment of none is legal, so such a cost never withholds
the offer and never makes a cast unplayable — and the number the player settles on is
recorded as the cost is paid, for an amount that reads it back (see *Amounts derived from
something else*). A fixed count is exact in both directions: two artifacts is paid by two
and refused by one, because over-paying a cost is not something a player may choose to do.

Both kinds carry their choice on the **action**, in its `payment` list, beside the mana
sources. A cost paid at announcement has no resolution to ask during, and once the spell
is on the stack there is nothing left to take back — so the choice arrives with the cast
or the cast does not happen. The server poses each as a slot over a server-enumerated
candidate list (`docs/protocol.md`), and anything a client leaves unanswered the server
pays for it.

A sacrifice is a **real death**: it goes down the one leaves-battlefield seam, so a dies
trigger — including the sacrificed permanent's own — sees it, exactly as `sacrifice_this`
already does for an activation cost.

A `count` of zero (bare, or `{"exactly": 0}`), or an `additional_cost` on a land (which is
played, not cast — CR 116.2a), fails the catalog validator. Exiling as a *cast* cost is not
modeled; that shape exists only on an activation (`exile_from_graveyard`, below).

### Modal spells (CR 700.2)

A modal card puts its effects in `modes` instead of in `spell_effects`:

```json
"modes": [
  { "effects": [ { "kind": "destroy_all", "affects": { "scope": "each_creature" } } ] },
  { "effects": [ { "kind": "destroy_all",
                   "affects": { "scope": "each_artifact_or_enchantment" } } ] }
]
```

The choice is made **as the spell is announced** (CR 601.2b) and it is made *first*,
before targets — because the chosen mode is what decides how many target slots the spell
has and what each may aim at. Nothing downstream ever sees the modes that were not chosen:
one mode's effects are what resolves, and the resolution path can reach no other.

The validator enforces four things, and the last one is not a rules rule:

- a modal card carries **no** `spell_effects` of its own — loose effects beside modes
  would resolve whichever mode was chosen;
- every mode does something;
- there are at least **two** modes, since one is a question with a single answer;
- there are at most **four** (`sage_engine::MAX_MODES`). A mode is a numbered row in a
  dock band of fixed height (`client-design.md` §6.7), and the alternative to refusing a
  fifth is truncating a sentence a player has to read *before* choosing it. So the limit
  lands on whoever authors the card rather than on whoever plays it.

Choosing more than one mode, repeating one, a mode with a cost of its own, and a modal
*ability* are all still unwritable.

### X in a mana cost (CR 107.3, CR 601.2b)

A card whose `mana_cost` contains `{X}` announces a value for X as it is cast. There is no
field for it: the symbol in the cost *is* the declaration.

```json
"mana_cost": "{X}{R}",
"spell_effects": [
  { "kind": "deal_damage_by_amount", "target": "any_target",
    "amount": { "source": "announced_x" } }
]
```

The announced value is folded into the cost as generic mana — one lot per `{X}` — by the
single function every road asks what a cast costs, so the offer, the payment, and the
charge cannot price the same spell differently. It is then **locked**: recorded on the
stack object and read from there by the resolving effect and by anything measuring a
threshold against it. Nothing re-derives it from the cost, because by then the cost has
been paid and is gone.

`{X}` in an **activation** cost is rejected by the validator. An activation pays out of a
pool and has no announcement step to fix a value in, so the symbol would simply be ignored
and the ability activated for nothing.

### What is true of a spell on the stack

`spell_traits` says what is true of the card **as a spell**, which no effect could say
because an effect happens when its own object resolves and both of these are read by
somebody else's:

```json
"spell_traits": [
  { "kind": "cant_be_countered", "if_x_at_least": 5 },
  { "kind": "damage_cant_be_prevented", "if_x_at_least": 5 }
]
```

`cant_be_countered` (CR 701.5a) does **not** touch targeting: a counterspell may still
choose the spell, resolve, and simply fail to remove it. `damage_cant_be_prevented`
(CR 615.1) defeats every prevention shield at once, however many are in force.

`if_x_at_least` measures the trait against the value this cast **announced**, so one card
is an ordinary spell for a small X and an uncounterable one for a large one. It is omitted
for a trait that always applies, and the validator rejects it on a card whose cost prints
no `{X}` — a clause about a value the card never asks for could never be true.

### Combat restrictions (CR 506.3, CR 509.1b)

Restrictions on attacking and blocking that are **not** keyword abilities live in
`restrictions` rather than `keywords`: no card prints them as a keyword, and some carry a
parameter. A unit restriction is its bare name and a parameterized one wraps its payload:

```json
"restrictions": ["cant_be_blocked_by_more_than_one", {"cant_be_blocked_by": "black"}]
```

| Restriction | Meaning | Enforced in |
| --- | --- | --- |
| `cant_attack` | can't be declared as an attacker | the attacker candidate set |
| `cant_block` | can't be declared as a blocker | the blocker candidate set |
| `cant_be_blocked` | no creature may block it | the pairwise block check |
| `cant_be_blocked_by` | no creature of the named colour may block it | the pairwise block check |
| `cant_be_blocked_by_more_than_one` | at most one blocker may be assigned to it | the whole-selection block check |
| `cant_be_blocked_by_power_or_less` | no creature of at most the named power may block it | the pairwise block check |
| `cant_be_blocked_except_by` | **only** a creature of the named subtype may block it | the pairwise block check |
| `can_block_additional` | it may block that many creatures beyond the first | the whole-selection block check |
| `must_be_blocked_by_all_able` | every creature able to block it does so | the whole-selection block check |

One of them is a *permission* rather than a restriction. `can_block_additional` names a
count — `{"can_block_additional": 1}` is "can block an additional creature each
combat" — and lifts the CR 509.1a default that a blocker blocks one attacker. It is
printed, granted, and read exactly as the restrictions beside it, and permissions of
different sizes sum. Two of *the same* size collapse, because the layer-6 fold
deduplicates every restriction alike; no catalog card can reach that today. It grants no
requirement to use the extra assignment, and blocking nothing stays legal.

Printed restrictions belong only on creatures; the loader rejects them elsewhere. They are
read through the computed characteristics at CR 613 layer 6, exactly as keywords are, so a
restriction an Aura or a spell imposes binds identically to a printed one and ends with the
effect that imposed it.

Four of these are facts about the **whole** declaration rather than about one
attacker/blocker pair — `cant_be_blocked_by_more_than_one`, menace, `can_block_additional`,
and `must_be_blocked_by_all_able` — so the engine can only judge them once the declaration
is assembled. The first two bound how many creatures may block one attacker and are stated
in that attacker's blocker slot `prompt` (`docs/protocol.md`) rather than left to a submit
that silently does nothing. The third is the same question from the blocker's side, and it
needs no prompt of its own: it is printed on the creature that carries it, where its
generated rules text says so, and unlike menace it depends on nothing else in the
declaration. The fourth is a *requirement* rather than a restriction, and is its own
section below.

The colour test reads the blocker's **printed** colours: CR 613 layer 5 (colour-changing
effects) is not implemented, so printed colour is current colour, the same way printed
types stand in for current types elsewhere in the engine.

The power test reads the blocker's **computed** power, and the difference from the colour
test is deliberate rather than an inconsistency: the layers that change power *are*
implemented, so a blocker pumped past the bound has really escaped it and one shrunk into
it has really fallen in. Printed colour stands in for current colour only because nothing
in the engine can change a colour yet.

`cant_be_blocked_except_by` names a **subtype** and is the one restriction stated as a
permission — everything the subtype does not name is forbidden, which is the exact
inverse of what `cant_be_blocked_by` forbids, and why it is its own restriction rather
than a negated colour form:

```json
"restrictions": [{"cant_be_blocked_except_by": "Spirit"}]
```

Its test reads the blocker's **computed** subtypes, following the power test rather than
the colour one. CR 613 layer 4 (type-changing effects) is not implemented, so today those
are the printed subtypes — but the read path is already the one that becomes correct on
its own the day that layer lands, with no call site left to remember.

### The one requirement (CR 509.1c)

`must_be_blocked_by_all_able` is the only member of this vocabulary that *requires* part
of a declaration rather than permitting or forbidding one, and it is a different kind of
rule rather than a restriction turned around. Everything above rejects a declaration
because of something it **contains**; CR 509.1c rejects one because of what it **omits** —
the declaration chosen must obey the maximum possible number of requirements without
violating any restriction. "The maximum possible" is a fact about the declarations that
were *not* submitted, so validating one is a **search** rather than a per-pair or
per-count check, and that search is the engine's own (`max_block_requirements_met`).

Two consequences follow from the rule itself, and both are why an approximation would be
wrong rather than merely imprecise:

- **A restriction always wins.** A requirement is met only by a declaration that is legal
  to begin with, so a creature that may not legally block the attacker — it lacks flying
  or reach, it can't block, a menace floor puts the whole block out of reach — is not
  required to. Nothing here makes an illegal declaration legal.
- **A requirement that cannot be met is not met.** Two attackers that each demand the
  defender's only creature demand it once between them: the maximum is one, and either
  answer is legal.

"Able" is judged per candidate blocker and per pair, exactly as a blocker slot's candidate
list is, so a tapped creature is never required to block. The requirement is a
whole-declaration fact, so the blocker slot's `prompt` states it (`docs/protocol.md`)
rather than leaving a short declaration to be silently refused on submit.

**Attack** requirements ("attacks each combat if able", CR 508.1d) are still not modeled:
nothing can force a creature into the attacker declaration.

### Imposing restrictions (continuous, CR 613.1f)

The three restriction verbs mirror the keyword-granting ones exactly, and all impose
**until end of turn**:

- `{"kind": "restrict", "target": "any_creature", "restriction": "cant_block"}` — one
  chosen target, or **up to N** of them with the same `targets` count `put_counters`
  takes: `{"kind": "restrict", "target": "any_creature", "targets": {"up_to": 2},
  "restriction": "cant_be_blocked"}` is "up to two target creatures can't be blocked this
  turn", imposed once per target still legal on resolution (CR 608.2c);
- `{"kind": "restrict_self", "restriction": "cant_be_blocked"}` — the ability's own
  source, which is not a target and never fizzles;
- `{"kind": "restrict_all", "affects": {"scope": "creatures_without_flying"}, "restriction": "cant_block"}`
  — a class, whose members are locked in on resolution (CR 611.2c).

An **attachment** imposes for as long as it is attached, via `attachment.restrictions`,
e.g. `"attachment": {"kind": "aura", "attach_to": "any_creature", "restrictions":
["cant_attack", "cant_block"]}`. The `power`/`toughness`, `keywords`, and `restrictions`
grants are independent; any combination may be present.

### Attachments: Auras and Equipment (CR 303.4, CR 301.5, CR 702.6)

**One block, with a `kind`.** An Aura and an Equipment share a single `attachment` field
rather than owning a field each, and that is a decision worth stating because it is the
one the rest of the vocabulary is built on. The *grant* is one thing: what an attached
permanent does to its host is read at CR 613 layer 6 (keywords, abilities, combat
restrictions) and layer 7c (power/toughness), and a creature carrying a sword is
indistinguishable at both
layers from one under an Aura. Two fields would mean every reader of a permanent's
characteristics asked two questions where the rules ask one — and could answer them
differently. Widening what may be attached to (a player, a land) therefore widens both
kinds at once, in one place.

```json
"attachment": {"kind": "aura", "attach_to": "any_creature", "power": 2, "toughness": 2}
"attachment": {"kind": "equipment", "attach_to": "any_creature_you_control",
               "equip": "{2}", "power": 2, "toughness": 1}
```

**`attach_to` is an ordinary target class**, so an Aura's host need not be a creature: an
enchant-land Aura writes `"attach_to": "any_land"`, and the cast slot, the CR 704.5m
state-based action, and the generated text all read that one field. A grant that has
nothing to say about the host — a `power`/`toughness` on a land — simply applies to a
characteristic that is not there. Enchanting a **player** is not expressible: no target
class an attachment may name is a player.

The `kind` decides exactly two things, and nothing else in the schema branches on it:

- **How it arrives.** An Aura is cast *at* an object: `attach_to` is a required target
  slot on the cast (CR 601.2c), the card is uncastable with no legal object (CR 303.4c),
  and it enters already attached (CR 303.4d). An Equipment is cast like any other
  artifact — no target, attached to nothing — and `attach_to` is instead the target slot
  of its **equip ability**.
- **What happens when the host leaves.** An Aura attached to an illegal object, or to
  nothing, is put into its owner's graveyard (CR 704.5m). An Equipment becomes
  **unattached and stays on the battlefield** (CR 704.5n), ready to be equipped again.

The equip ability is **derived, never authored**: `equip` is the cost, `attach_to` is the
class of legal host, and `sage_engine::equip_ability` composes
`{cost}: attach this to target creature you control` from the two. There is no way to
write an equip ability that charges a cost the card does not print or attaches something
other than itself. It is an ordinary activated ability everywhere downstream — offered,
targeted, paid for, put on the stack, resolved, and labelled by the same code an authored
`{2}: …` goes through — with one extra rule: it is **sorcery-speed** (CR 702.6b), gated
both where the action is offered and again, independently, where it is applied.

`attach_to` is a restriction on the *equip target*, not on where the Equipment may stay:
CR 301.5c says only that an Equipment is attached to a creature, so a card that equips
"target creature you control" does not fall off a creature an opponent gains control of.
An Aura's `attach_to`, by contrast, is exactly what CR 704.5m re-checks.

Moving an Equipment onto a second creature is one write: attaching an already-attached
permanent unattaches it first (CR 701.3c), and because the grant is derived from the
attachment on every read, the old host loses it and the new host gains it with nothing to
migrate. Aura *movement* remains out of scope, and is listed in the exclusions.

The grant may also **scale with a count of permanents** (`+1/+1 for each Forest you
control`) through a `count_of` — see *Amounts derived from a count*, which is also where
the one way it differs from every other counted amount is stated: it is recalculated on
every read rather than fixed once.

The validator rejects an `attachment` whose `kind` names a subtype the card does not bear
(`Aura`, `Equipment`), an Equipment with no `equip` cost, an `equip` cost on an Aura, and a
`count_of` that counts by `min_power`.

### Targets (CR 115.1)

A targeting effect names a **class** with `target`, not an object; the player chooses one
member as the spell or ability is announced (CR 601.2c) and the choice is re-checked on
resolution (CR 608.2b). The classes are `any_player`, `any_player_or_planeswalker`,
`any_opponent`, `any_permanent`, `any_nonland_permanent`,
`any_nonland_permanent_an_opponent_controls`, `any_artifact_creature_you_control`,
`any_creature`,
`any_creature_you_control`, `any_creature_an_opponent_controls`,
`any_creature_with_flying`, `any_tapped_creature`, `any_artifact`, `any_enchantment`,
`any_artifact_or_enchantment`, `any_artifact_enchantment_or_creature_with_flying`,
`any_land`, `spell_on_stack`, `creature_spell_on_stack`, `any_target`,
`any_permanent_with_mana_value`, and `card_in_graveyard`.

`any_player_or_planeswalker` is the burn class that names both halves and no creature —
`Lava Axe deals 5 damage to target player or planeswalker`. It is neither `any_player`
(which would drop the planeswalker half) nor `any_target` (which would add creatures the
card cannot hit).

`any_permanent_with_mana_value` narrows the permanent universe by the number printed on the
face (CR 202.3) — `Exile target permanent with mana value 1`. It is written in the enum's
tagged form because it carries a field:

```json
{ "kind": "exile", "target": { "any_permanent_with_mana_value": { "mana_value": 1 } } }
```

The comparison is an **equality**, not the cap `card_in_graveyard` takes: a card that names
one value means that value, and a cap would silently admit everything cheaper. The value is
read through the same printed face a token answers with, so a token is a candidate at mana
value 0 (CR 111.4 — no mana cost) rather than being absent from the universe, and so is a
land.

`card_in_graveyard` is the one class that names a **card in a zone** rather than an object
on the battlefield or the stack, so it is the only one a chosen card target satisfies. It is
written in the enum's tagged form rather than as a bare string, and carries the three
independent things a printed card says about such a target:

```json
{ "kind": "return_card_to_battlefield",
  "target": { "card_in_graveyard": { "class": "creature", "max_mana_value": 2 } } }
{ "kind": "return_card_to_hand",
  "target": { "card_in_graveyard": { "class": "instant_or_sorcery" } } }
{ "kind": "return_card_to_battlefield", "tapped": true,
  "target": { "card_in_graveyard": { "scope": "any", "class": "creature" } } }
```

- `scope` is `yours` (the default — "from **your** graveyard") or `any` ("from **a**
  graveyard"). The difference is most of what such a card costs to print, so it is a field
  rather than an assumption.
- `class` is `any` (the default), `creature`, `instant_or_sorcery`, `artifact`, or `land`,
  read off the card's **printed** types — a card in a graveyard is not on the battlefield,
  so it has no computed characteristics to read instead. It is a small enum of its own
  rather than the `filter` the mid-resolution choices take, because a target spec is
  threaded by value through every targeting path and a filter carrying a subtype string
  would cost all of them that for no card in this set.
- `max_mana_value` compares against the card's mana value (CR 202.3), derived from its cost
  through the same parser every payment uses; absent means any.

A graveyard is public, so its candidates are enumerable exactly as a battlefield's are.
`return_card_to_hand` sends its target to that card's **owner's** hand (CR 400.7), and is
one of the three effects that may name more than one target — "return up to two target
creature cards from your graveyard to your hand" is one effect with a two-slot group.

Every class is evaluated **relative to the choosing object's controller**, which is what
lets one authored card mean "you" from either seat. Classes read through the computed
characteristics where they can, so `any_creature_with_flying` accepts a creature that was
*granted* flying and stops accepting it when the grant ends (CR 613.1f).

An effect fills exactly one target slot by default, and a card's slots are consumed in the
order its effects are written.

**Two differently-specified slots.** An effect's slots are an ordered list, so they need
not all name the same class. `fight` is the effect that uses it: its two slots are written
as two fields, and each carries its own spec (see [Fighting](#fighting-cr-70112)). Two
effects would not do — they would be aimed independently, and the point of such a card is
that the two creatures it names are related to each other. Every rule stated above in terms
of *groups* counts both: an announcement fills both slots or is illegal, a `may` may not
wrap such an effect (one wrapper cannot forward two slots), and a conditional branch may
not contain one.

**Up to N targets.** Three effects — `put_counters`, `return_card_to_hand`, and
`restrict` — may name more than one, with a `targets` count:

```json
{ "kind": "put_counters", "target": "any_creature", "targets": { "up_to": 2 },
  "counter": "plus_one_plus_one", "count": 1 }
{ "kind": "restrict", "target": "any_creature", "targets": { "up_to": 2 },
  "restriction": "cant_be_blocked" }
```

The arity is one field read in one place, so a fourth effect that needs it inherits the
whole pipeline — the slots, the offer, the pairing, the per-target re-check — by declaring
the field rather than by joining anything.

`{"exactly": n}` demands exactly `n` distinct targets; `{"up_to": n}` lets the player choose
between none and `n`. Omitting the field is `{"exactly": 1}`, which is what every other
authoring means. Three rules follow, and all three are enforced rather than assumed:

- an ability whose every target group has a **minimum of zero** is offered even with nothing
  to aim at, because choosing nothing is a legal announcement (CR 601.2c) — while one with a
  mandatory slot and no candidate is withheld;
- the targets within one group must be **distinct objects**;
- **at most one** variable-arity group may appear in a single ability or spell. Targets are
  stored as one flat list per stack object, and with two variable groups the split back onto
  effects would be ambiguous. The catalog validator rejects it
  (`Violation::TwoVariableTargetGroups`).

### Player references

An effect whose subject is a player carries a `player_ref` rather than a `target`. The
reference itself decides whether a target is chosen:

| `player_ref` | Meaning | Chooses a target? |
| --- | --- | --- |
| `controller` | "you" | no |
| `each_opponent` | every opponent still in the game | no |
| `target_player` | one chosen player | yes |
| `target_opponent` | one chosen opponent | yes |

A non-targeting reference can never fizzle and, in a game of three or more, really does
name every opponent. `gain_life`, `lose_life`, and `mill` all take a reference, so both
shapes exist for each without any of them restating the fizzle rule.

### Damage (CR 120.3)

`deal_damage` names **who or what takes the damage** with exactly one of three keys, and
that key decides whether a target is chosen:

```json
{ "kind": "deal_damage", "target": "any_target",      "amount": 2 }
{ "kind": "deal_damage", "player_ref": "each_opponent", "amount": 2 }
{ "kind": "deal_damage", "affects": {"scope": "each_creature"}, "amount": 2 }
```

- `target` is a target spec: one slot, chosen on announcement, re-checked on resolution,
  fizzling if the choice is gone.
- `player_ref` is the same reference `gain_life` and `mill` take, with the same rule —
  `each_opponent` chooses nothing and hits every opponent, `target_player` fills a slot.
- `affects` is the same class `pump_all` takes, and never targets.

Both class forms enumerate their subjects **on resolution** (CR 611.2c), so a creature
that arrived after the spell was cast is included and one that has died is not.

Damage is not life loss. Damage to a creature is *marked* on it and drives the
lethal-damage state-based action (CR 704.5g); damage to a player is life loss
(CR 120.3a). A card that says "loses life" is authored with `lose_life`, and one that
says "deals damage" cannot be approximated by it. Damage prevention is out of scope.
Deathtouch and lifelink apply to damage whose **source is a permanent** — combat damage,
and `fight` below; a spell's damage has no creature behind it that could carry either.

### Fighting (CR 701.12)

`fight` is the one effect whose target slots do not share a spec. It names two creatures in
two fields, in the order the printed sentence names them:

```json
{ "kind": "fight", "dealer": "any_creature_you_control",
  "dealt_to": "any_creature_an_opponent_controls" }
{ "kind": "fight", "dealer": "any_creature_you_control",
  "dealt_to": "any_creature_an_opponent_controls", "mutual": true }
```

- `dealer` is the first slot: the creature that deals damage equal to its power.
- `dealt_to` is the second slot: the creature that damage is dealt to.
- `mutual` (default `false`) makes the second creature deal damage equal to *its* power
  back — the printed word **fights** (CR 701.12a). Left off, it is the one-sided form a
  card prints as "deals damage equal to its power to".

There is no amount field, and there is no amount vocabulary behind it: CR 701.12a *defines*
the verb as each creature dealing damage equal to its power, so the power read is part of
the verb. Both powers are read before either damage is dealt, so the damage is simultaneous
and a creature that dies to it still dealt its own power.

If **either** creature is an illegal target as the effect is reached, neither deals nor is
dealt damage (CR 701.12c) — stricter than the do-as-much-as-possible default, and the rule
for every effect whose slots do not share a spec: half of such an effect is not a smaller
version of it. If *every* target is illegal the spell never resolves at all (CR 608.2b).

### Activation costs

`cost` entries are `{"kind":"tap"}` (the `{T}` symbol),
`{"kind":"mana","mana":"{1}{R}"}` — written in the same curly-brace notation a card's
`mana_cost` uses — `{"kind":"loyalty","amount":-2}` (below), `{"kind":"sacrifice_this"}`,
`{"kind":"remove_counters","counter":"charge","count":1}`, and the three the player picks
the payment for: `{"kind":"sacrifice","card_type":"creature","another":true}`,
`{"kind":"discard","count":1}`, and
`{"kind":"exile_from_graveyard","class":"creature"}`. Mana is paid from the
activating player's pool through the same seam a cast uses, and the whole cost is paid all
or nothing — a failed mana payment never leaves the source tapped. CR 302.6 still forbids
a summoning-sick creature paying `{T}`, including for a mana ability.

The two costs that spend the **source** rather than a resource are deliberately about the
source and nothing else: neither requires the player to pick anything, so neither needs a
choice to ride on the action.

- `sacrifice_this` (CR 701.17) is always payable — an ability is only offered from a
  permanent on the battlefield, and one on the battlefield can always be sacrificed. It is
  applied **last**, whatever order the costs are written in, so a `{T}` beside it taps a
  permanent that is still there. Sacrificing is a real death: the permanent goes to its
  owner's graveyard through the one leaves-battlefield seam, so a dies trigger — including
  the source's own — sees it. The ability itself is unaffected and still resolves
  (CR 113.7a), which is the whole point of a card that spends itself for an effect.
- `remove_counters` is payable only while the source holds that many, which is the entire
  content of a charge-counter card: the ability is offered three times and then stops. It
  is not a loyalty cost: that one is signed, may *add*, and carries CR 606.3's two
  timing rules, and collapsing the two would make a charge counter a loyalty ability.

The other three are the ones the **player picks the payment for**, and everything about them
follows from that. The choice arrives on the action, in the same `payment` list a cast
carries (`docs/protocol.md`), because a cost is paid as the ability is activated (CR
602.2b): there is no resolution to ask during, and once the ability is on the stack there is
nothing left to take back. None is offered without enough to pay it, so an ability
with nothing to feed it is simply not activatable rather than activatable and then free.

- `sacrifice` takes permanents the activator controls (CR 701.17b — whose permanent
  it is stays a rule rather than a field). `card_type` and `subtype` narrow what qualifies
  and both default to any, so `{"kind":"sacrifice","subtype":"Goblin"}` is exactly
  `Sacrifice a Goblin`: a Goblin is a Goblin whatever else it is, and a Goblin token counts
  because the subtype is read off the printed face. `another` excludes the source — the
  *another* of `Sacrifice another creature` — and without it an ability may eat its own
  source, which is legal and still resolves (CR 113.7a). `count` is the same
  `{"exactly": n}` / `"any"` field a cast's additional cost takes and defaults to exactly
  one, so `{"kind":"sacrifice","card_type":"artifact","count":{"exactly":2}}` is
  `Sacrifice two artifacts` — one cost taking a pair, refused by one, rather than two costs
  a player could half-pay. Paying it is a real death down the
  same leaves-battlefield seam `sacrifice_this` uses, so a dies trigger sees it.
- `discard` takes `count` cards from the activator's hand (CR 701.8). Unlike the cast-side
  additional cost there is no card to exclude: the source is a permanent, not a card in the
  hand paying for itself.
- `exile_from_graveyard` takes `count` cards out of the activator's **own** graveyard
  (CR 701.19), narrowed by `class` — the same `any` / `creature` / `instant_or_sorcery` /
  `artifact` / `land` set a graveyard target spec uses, read off the printed face because a
  card in a graveyard has no computed characteristics. Whose graveyard stays a rule rather
  than a field: every printed cost of this shape says *your graveyard*. It is **not** a
  sacrifice with a different destination — nothing leaves the battlefield, so nothing dies
  and no dies trigger fires — and the ability stops being offered when the pile runs out,
  which is the whole of what such a card does.

Mana is **not** named on an activation's payment. It is paid from the pool, floated by
activating mana abilities as actions in their own right, exactly as it always was.

The counter kinds are `plus_one_plus_one`, `minus_one_minus_one`, `loyalty`, and the four
that fold into no characteristic and no state-based action — `charge`, `gold`, `wish`, and
`corpse`. Those four are a name and a count that the printing card's own abilities read;
they are kept distinct from one another rather than aliased, because two cards on one
battlefield may name different counters and one card's ability must not spend the other's.

### Planeswalkers and loyalty (CR 306, CR 606)

A planeswalker authors `loyalty` — its printed starting loyalty, the number in its corner:

```json
{
  "schema_version": 1,
  "functional_id": "test_warden",
  "name": "Test Warden",
  "supertypes": ["legendary"],
  "types": ["planeswalker"],
  "subtypes": ["Warden"],
  "mana_cost": "{2}{W}{W}",
  "colors": ["white"],
  "loyalty": 4,
  "abilities": [
    { "type": "activated", "cost": [{ "kind": "loyalty", "amount": 1 }],
      "effects": [{ "kind": "gain_life", "player_ref": "controller", "amount": 2 }] },
    { "type": "activated", "cost": [{ "kind": "loyalty", "amount": -2 }],
      "effects": [{ "kind": "deal_damage", "target": "any_target", "amount": 2 }] }
  ]
}
```

`loyalty` is a *characteristic*, not a running total. The permanent enters the
battlefield with that many **loyalty counters** (CR 306.5b) — applied at the same
battlefield-entry seam `enters_tapped` and `enters_with_counters` use, so a planeswalker
is never briefly on the battlefield at zero — and everything afterwards reads the
counters: the ability cost spends them, damage removes them (CR 120.3c, rather than being
marked), and a planeswalker with none is put into its owner's graveyard (CR 704.5i).

An ability whose cost includes `{"kind":"loyalty"}` is a **loyalty ability** and carries
two timing rules no other activated ability has (CR 606.3): it may be activated only at
sorcery speed on its controller's own turn, and only once per turn per permanent. A
negative amount is payable only out of loyalty the permanent actually has, so a `-7` is
simply not offered at 4. All three restrictions are enforced twice — once when the action
is offered and again, independently, when it is applied — so a forged action cannot slip
past them.

A planeswalker is **not a creature**: it has no power or toughness, it cannot attack or
block, and the toughness-based state-based actions never touch it. It *can* be attacked
(CR 508.1a): an attack names a player or a planeswalker, the planeswalker's controller
declares blockers for attackers attacking it, and combat damage that gets through removes
loyalty.

### Emblems (CR 114)

`create_emblem` gives a player an **emblem**: a marker whose only characteristics are its
abilities, in no zone, which nothing in the game ever removes.

```json
{ "kind": "create_emblem",
  "abilities": [
    { "type": "triggered",
      "event": { "beginning_of_step": { "step": "end_step", "whose_turn": "yours" } },
      "effects": [{ "kind": "create_token", "count": 3, "token": { "…": "…" } }] }
  ] }
```

Its abilities are authored inline, exactly as a token's characteristics are and for the same
reason: an emblem is not a card, so there is no catalog entry to point at. An optional
`player_ref` names who gets it, defaulting to `controller` — *you get an emblem*.

Only **static** and **triggered** abilities may appear. An activated ability would have no way
to be activated and an enters-the-battlefield replacement would have no entry to replace, so
either is an emblem with a dead ability; the catalog validator rejects it
(`Violation::EmblemAbilityIsNotStaticOrTriggered`).

An emblem's abilities reach the game through a **second source list** in both ability paths:
the computed-characteristics loop walks the emblems alongside the battlefield, and so does the
diff-based trigger collector. Everything else follows: `creatures_you_control` means the
emblem's controller's creatures, a step trigger is scoped by `whose_turn` exactly as a
permanent's is, and the emblem's own object id is its CR 613.7 timestamp, so an older emblem's
anthem applies before a newer permanent's.

### Conditional effects (CR 608.2, the intervening-if)

`conditional` applies one branch or the other, judged **as the effect is reached** — so it
sees everything the effects before it did:

```json
{ "kind": "conditional",
  "condition": { "kind": "controls_at_least",
                 "permanents": { "card_type": "artifact" }, "count": 3 },
  "then":      [{ "kind": "draw_card", "count": 2 }],
  "otherwise": [{ "kind": "draw_card", "count": 1 }] }
```

The conditions are:

| `kind` | Asks |
| --- | --- |
| `controls_at_least` | Whether the controller controls at least `count` permanents matching `permanents` |
| `milled_this_way` | Whether a card matching `filter` was milled **by this resolution** |
| `discarded_this_way` | Whether the controller discarded a card during this resolution |
| `gained_life_this_turn` | Whether the controller gained at least `amount` life **this turn** |
| `attacked_or_blocked_this_turn` | Whether the effect's own **source** was declared as an attacker or a blocker **this turn** |

Every one but the first reads the recorded events rather than the zones or the totals, and
that is the point: a Zombie already in the graveyard was not milled this way, and a graveyard
scan could never tell the two apart. The window survives a suspension, so a discard that stops
to ask a question still answers `discarded_this_way` correctly when it resumes.

```json
{ "kind": "conditional",
  "condition": { "kind": "gained_life_this_turn", "amount": 5 },
  "then":      [{ "kind": "create_token", "token": { "name": "Angel", "types": ["creature"] } }] }
```

`gained_life_this_turn` is the one whose window is the **turn** rather than the resolution,
and it reads the turn's life-gain *events* for the same reason the `you_gain_life` trigger
does: gaining three life and losing it again leaves every total where it started and is still
three life gained, so no reading of a life total — against the turn's opening total or any
other — could answer it. `amount` is an inclusive lower bound on the turn's gains **in
total**, so two gains of three satisfy a threshold of five; `1` is the plain "if you gained
life this turn" and is written that way in the generated rules text.

```json
{ "kind": "conditional",
  "condition": { "kind": "attacked_or_blocked_this_turn" },
  "then":      [{ "kind": "shuffle_self_into_library" }] }
```

`attacked_or_blocked_this_turn` is the one condition about the **source** rather than about
its controller — "you" is not the subject of that sentence, one particular permanent is — so
it is false for any object that is not an ability of a permanent (a spell, an emblem's
ability). Its window is the turn, and it reads the turn's recorded attacker and blocker
declarations rather than the board, because the board has forgotten: the end-of-combat
turn-based action clears `attacking` and `blocking` (CR 511.3), so by the end step where a
card asks this, the declaration is the only witness left. Both halves are one question
because a printed card asks them as one, and two combats in a turn both count.

A `permanents` selector is a small product — `scope` (`you_control`, `opponents_control`,
`any`; default `you_control`), optional `card_type`, optional `subtype`, optional `color`,
and optional `min_power` — read against printed types, like every other selector in the
engine.

`min_power` is the one exception to that, and the exception is the point: it is read
through the **computed** characteristics, because power is what the implemented layers
actually change. "If you control a creature with power 4 or greater" is satisfied by a 3/3
under an anthem and stops being satisfied when the anthem leaves; a printed reading would
get both wrong. It is therefore **rejected inside a static ability's `condition`**
(`Violation::PowerInStaticCondition`): that condition is evaluated from inside the
computation of a permanent's characteristics, and asking there would ask again, forever.
Only a lower bound exists, because only a lower bound is printed on a card the catalog
defines.

A branch **may not choose a target** (`Violation::TargetInsideConditional`), and this is where
a conditional differs from a `may`: an optional effect forwards the group of the one effect it
wraps, but a conditional has two branches sharing one flat target list, so a group named in
either could not be paired back onto the branch that was actually taken.

### Amounts derived from a count

`pump_by_count` shrinks or pumps a target by a per-permanent amount, with X taken **once, on
resolution** (CR 608.2):

```json
{ "kind": "pump_by_count", "target": "any_creature",
  "power_per": -1, "toughness_per": -1,
  "count_of": { "scope": "you_control", "subtype": "Zombie" } }
```

The resulting fixed modifier is what the layer system folds in, so a Zombie that dies later in
the turn does not give the shrunk creature its toughness back — which is what the printed card
means and what a re-evaluated selector would get wrong.

`gain_life_by_count` and `deal_damage_by_count` are the same idea for the other two amounts,
and take the same `count_of` selector:

```json
{ "kind": "gain_life_by_count", "player_ref": "controller", "amount_per": 1,
  "count_of": { "card_type": "creature" } }
{ "kind": "deal_damage_by_count", "target": "any_creature_an_opponent_controls",
  "amount_per": 1, "count_of": { "subtype": "Goblin" } }
```

Each takes X **once, on resolution**, from the board as it stands then, and each keeps the
subject vocabulary its fixed sibling has — a `player_ref` for life, and the same
target/`player_ref`/`affects` choice for damage. The count is relative to the effect's
*controller* even when the life or the damage goes elsewhere, because "each creature you
control" says "you" and the subject clause does not change who that is.

Two other numbers take the same `count_of`, and neither gets a `_by_count` verb of its own
— they are a field on the thing that already carries the number:

```json
{ "kind": "create_token", "count_of": { "card_type": "creature" },
  "token": { "name": "Soldier", "types": ["creature"], "subtypes": ["Soldier"],
             "colors": ["white"], "power": 1, "toughness": 1 } }
"attachment": { "kind": "aura", "attach_to": "any_creature", "power": 1, "toughness": 1,
                "count_of": { "scope": "you_control", "subtype": "Forest" } }
```

- On `create_token`, `count_of` makes `count` (default `1`) the number created **per
  counted permanent**. A second variant would have duplicated the token's whole face, its
  creator, `tapped`, and `attacking`, and the count is the same number the effect already
  had; the field says where that number comes from. X is taken once, on resolution, before
  the first token arrives, so a token never counts towards its own number.
- On an `attachment`, `count_of` makes `power`/`toughness` the grant **per counted
  permanent** — `+1/+1 for each Forest you control`.

**The attachment one is the exception to "once, on resolution", and deliberately.** It is a
static ability (CR 604.3), not a one-shot effect, so CR 608.2 never applies to it: the
grant exists only while the attachment is attached, and its value is recalculated on every
read of the host's characteristics. A Forest played after the Aura resolved makes the Aura
bigger, and one that leaves makes it smaller — which is what the printed card means, and
the opposite of what `pump_by_count`'s frozen modifier does.

Because it is evaluated from *inside* the computation of a permanent's characteristics, an
attachment's `count_of` may not carry `min_power` (`Violation::PowerInAttachmentCount`) —
the same recursion, and the same refusal, as `min_power` inside a static ability's
`condition`. The count is relative to the **attachment's** controller, which is who "you
control" means on the card that printed the grant, not the host's controller.

### Amounts derived from something else (`where X is …`)

Not every X is a count of permanents. The other sources are a closed set — a
`DerivedAmount`, authored as an `amount` block with a `source` tag — and there is no
arithmetic over them: no halving, no adding two together, and no way to compose one out of
another. A card that needs a new phrase adds a source.

| `source` | Reads | Written on a card as |
| --- | --- | --- |
| `announced_x` | the value of **X its controller announced** as the spell was cast (CR 601.2b) | `deals X damage to any target` |
| `life_gained_this_turn` | how much life **you** have gained this turn (CR 118.3) | `where X is the amount of life you gained this turn` |
| `milled_this_way` | how many cards **this resolution** milled matching `filter` | `for each land card put into their graveyard this way` |
| `greatest_mana_value` | the greatest mana value among the permanents `among` names (CR 202.3) | `equal to the greatest mana value among artifacts you control` |
| `sacrificed_to_cost` | how many permanents **this object's own cost** sacrificed | `Sacrifice any number of lands. Search your library for up to that many land cards` |
| `sacrificed_creature_power` | the power the creature that cost sacrificed **had** (CR 608.2h) | `deals damage equal to the sacrificed creature's power` |

`announced_x` is the odd one out and worth stating plainly: it reads neither the board
nor the event log, because there is nothing to read. X was **chosen**, at announcement,
before targets and before payment, and it was locked the moment it was named — it rides on
the stack object from there, so the mana that was charged, the effect that resolves, and
the text the stack entry shows are all the same number by construction. It is zero for an
object that announced none.

Five effects read one:

```json
{ "kind": "pump_by_amount", "target": "any_creature",
  "power_per": -1, "toughness_per": -1,
  "amount": { "source": "life_gained_this_turn" } }
{ "kind": "draw_cards_by_amount",
  "amount": { "source": "greatest_mana_value",
              "among": { "scope": "you_control", "card_type": "artifact" } } }
{ "kind": "draw_cards_by_amount",
  "amount": { "source": "milled_this_way", "filter": { "kind": "land" } } }
{ "kind": "deal_damage_by_amount", "target": "any_target",
  "amount": { "source": "announced_x" } }
{ "kind": "deal_damage_by_amount", "target": "any_target",
  "amount": { "source": "sacrificed_creature_power" } }
{ "kind": "search_library", "take": 0, "filter": { "kind": "land" },
  "destination": "battlefield_tapped",
  "take_amount": { "source": "sacrificed_to_cost" } }
```

**The last two read the payment, not the game, and that is the whole reason they are
stored.** A cost is paid as the object goes on the stack (CR 601.2h), so by the time it
resolves the permanents it ate are in a graveyard with no identity of their own — or, for a
token, nowhere at all. Both numbers are therefore captured *as the cost is paid* and carried
on the stack object beside its targets; reading them at resolution reads what was written
down, which is exactly CR 608.2h's last-known information. A card that names one but whose
own cost sacrifices nothing fails the catalog validator
(`Violation::PaymentAmountIsNeverPaid`), because the honest answer would be a silent zero.

`deal_damage_by_amount` is `deal_damage`'s and `deal_damage_by_count`'s sibling in the same
way, and its subject decides whether a target is chosen exactly as every other damage verb's
does. `search_library`'s `take_amount` is a **field** rather than a twin verb for the reason
`create_token`'s `count_of` is one: a second variant would duplicate the filter and the
destination, and the number is the same number the effect already carries. When
`take_amount` is present `take` is ignored. An amount of zero is a search that shuffles and
finds nothing (CR 701.19c), not a stall.

`pump_by_amount` is `pump_by_count`'s sibling for every X that is not a count, and freezes
X into a fixed modifier in exactly the same way: life gained later in the turn does not
shrink the creature any further. `draw_cards_by_amount` is `draw_card`'s, with the number
taken off the game instead of off the card; each draw goes through the same seam, so
emptying a library still flags the decking loss (CR 704.5c).

They are separate verbs rather than an extra field on their fixed siblings because exactly
one source is ever present: a card says `for each Zombie you control` **or** `where X is
the amount of life you gained this turn`, never both, and two optional fields would make
"neither" and "both" authorable shapes that mean nothing.

**A count of permanents is deliberately not one of these sources**, and keeps the
`count_of` spelling of the previous section. It is the one X a *static* ability may also
read — an Aura's `+1/+1 for each Forest you control` is recalculated on every read of its
host — and nothing here could stand in that position: `life_gained_this_turn` and
`milled_this_way` read recorded **events** over a window (the turn, and this resolution),
and a window has no meaning outside a resolution.

`milled_this_way` names no player, on purpose. A resolution's mills belong to the
resolution, and the card reading the number is the card that just did the milling — "their
graveyard" is the player the effect above it already named, so a scope here would be a
second way to say the same thing and a second way to get it wrong.

### Emptying a graveyard, and the top of a library

`exile_graveyard` moves **every** card of the named player's graveyard to exile at once. Its
subject is the same `player_ref` a mill takes, with the same rule — `target_player` fills a
slot and `each_opponent` does not — and an already-empty graveyard is a legal subject and a
resolution that does nothing.

`put_on_top_of_library` is the third destination a permanent can be pushed to, beside
`return_to_hand`'s hand and `exile`'s exile. A **token** put anywhere but the battlefield
ceases to exist (CR 111.7), so a bounced token never arrives in the library either.

`shuffle_self_into_library` is the fourth, and the only one that is not a place: the card
goes into its owner's library and the library is then randomized (CR 701.19), so a shuffled
card is somewhere in the deck and nowhere in particular. It takes no `target` — the subject
is the ability's **own source**, like `pump_self` and `restrict_self` — so it fills no slot
and can never fizzle, and a source that has already left the battlefield is simply not there
to move. The shuffle draws from the game's seeded stream (ADR 0006), so the same seed replays
the same deck order; it happens whether or not a card arrived, which is why a token shuffled
into a library ceases to exist on the way and the library is still shuffled.

```json
{ "kind": "shuffle_self_into_library" }
```

### Mana whose colours the player chooses

`add_mana_any_color` produces mana whose **colours the player chooses**:

```json
{ "kind": "add_mana_any_color", "amount": 2,
  "restriction": { "kind": "spells_with_subtype", "subtype": "Dragon" } }
{ "kind": "add_mana_any_color", "amount": 2, "same_color": true }
```

The amount is authored; the colours are not authored at all. On resolution the controller is
asked through the same mid-resolution choice queue a discard or a scry uses, answered with
`answer_color`. How many times they are asked is the whole of the difference between the two
printed phrasings: *add two mana in any combination of colours* asks once per point, so two
mana may be one each of two, while `"same_color": true` — *add two mana of any one colour* —
asks once and pays out the whole amount in the answer. `same_color` defaults to `false`. The
optional `restriction` rides on every point produced, exactly as `add_restricted_mana`'s does.

### A colour named as a permanent enters (CR 614.12)

`enters_choosing_color` is a card's declaration that its controller names one of the five
colours **as it enters the battlefield**, and that the answer stays on the permanent:

```json
{ "abilities": [
    { "type": "enters_choosing_color" },
    { "type": "triggered",
      "event": { "you_cast_spell": "chosen_color" },
      "effects": [{ "kind": "gain_life", "player_ref": "controller", "amount": 1 }] } ] }
```

It carries no answer, the way `enters_tapped` carries no tapped state: the answer belongs to
the permanent, so two copies of one card side by side may have chosen differently, and a card
that leaves and comes back is a new object that chooses again (CR 400.7).

**It is not an enters-the-battlefield trigger**, and the difference is the point. A trigger
goes on the stack after the permanent has arrived, leaving a window in which it sat there with
no colour recorded and any player could respond. This is part of arriving: the card waits on
the choice queue in no zone at all — where a spell's card already waits while a mid-resolution
choice is owed — and the permanent that then enters already carries its colour. Every read of
the board, including the state-based-action loop and the trigger diff, therefore sees one
complete arrival. The controller is asked, always, and all five colours are always legal, so
the question can never stall (ADR 0013).

The only thing that reads the answer back today is `{"you_cast_spell": "chosen_color"}` — "a
spell of the chosen color", satisfied by a spell whose printed colours *include* it (CR 105.2),
so a gold spell is of each of its colours and a colourless spell is of none. Watching it on a
card that never names a colour is a validation error (`ChosenColorIsNeverNamed`), because the
engine's honest answer to a permanent with no colour recorded is to notice nothing, and a card
that silently does nothing is the hardest kind of wrong to spot.

Naming a **type** is not authorable, and nothing on a spell records a choice at all.

### A card named as a permanent enters (CR 614.12)

`enters_naming_card` is the same seam for a *card name*. It declares that the permanent's
controller names a card as it enters, and that the answer stays on the permanent:

```json
{ "abilities": [
    { "type": "enters_naming_card", "class": "nonbasic_land" },
    { "type": "static",
      "affects": { "scope": "permanents_your_opponents_control", "card_type": "land",
                   "with_the_named_card": true },
      "modification": { "kind": "lose_all_abilities" } } ] }
```

`class` is what a card puts between "name a" and "card"; the only value is `nonbasic_land`,
and it grows by adding one when a card needs it. The choice happens exactly where the colour
choice does — the card waits off the battlefield until it is answered — and everything said
about that above applies here unchanged.

**What is recorded is a functional identity, never prose, and this is a legal rule rather
than a style one.** The answer set is derived from the **catalog** on every read, so a player
names one of the cards SAGE has itself defined: the choice offers those cards, the answer
carries the card's authored identity, and the engine refuses anything outside the freshly
derived list. There is no path by which a name the repository does not already contain can
reach a game state, and a client composes no list and sends no string.

A static ability reads the answer back with `{"with_the_named_card": true}` on a
`permanents_your_opponents_control` selector — "with the chosen name". Writing that on a card
that declares no `enters_naming_card` is a validation error (`ChosenNameIsNeverNamed`), for
the reason watching the chosen colour without choosing one is: the phrase would have no
referent and the class could never contain a permanent. Naming a card and never reading it
back is fine.

### Restricted mana (CR 106.6)

`add_restricted_mana` adds mana that may be spent only on what its restriction names:

```json
{ "kind": "add_restricted_mana", "color": "red", "amount": 2,
  "restriction": { "kind": "spells_with_subtype", "subtype": "Dragon" } }
```

It is a mana verb like `add_mana`, so an ability whose every effect is one of the four is a
mana ability and never uses the stack — unless it is a **loyalty** ability, which CR 605.1a
excludes however it is written. The restriction rides on the mana rather
than on the pool, so restricted and ordinary mana of the same colour coexist and both empty at
the end of the step (CR 500.4). A payment is told what it is *for*: casting a spell whose
printed subtypes match may spend it, and nothing else can. Restricted mana is spent first,
which is optimal rather than merely convenient — mana that can pay for nothing else can never
be saved for anything else.

### Casting from a graveyard

`allow_casting_from_graveyard` grants a player permission to cast cards matching `filter` from
their graveyard **for the rest of the turn**:

```json
{ "kind": "allow_casting_from_graveyard", "player_ref": "controller",
  "filter": { "kind": "creature", "subtype": "Zombie" } }
```

The cards do not move: they stay in the graveyard, are offered by `valid_actions` beside the
hand, and are cast through the same action, the same stack object, and the same timing gates a
hand cast uses. The permission is recorded with the turn it was granted on and dropped at the
turn boundary, so "this turn" is a comparison of turn numbers rather than a countdown that
could drift.

### Ignoring hexproof

`ignore_hexproof` grants a player permission to aim spells and abilities **as though hexproof
were not there**, for the rest of the turn:

```json
{ "kind": "ignore_hexproof", "player_ref": "controller" }
```

It is the same permission shape as `allow_casting_from_graveyard` — per player, recorded with
the turn it was granted on, dropped at the same turn boundary — applied at the targeting gate
instead of the casting one.

It names **no permanent and no class of permanent**, and that is the whole of why one field is
enough. Hexproof is already relative to who is aiming (CR 702.11b): a permanent's own
controller is never stopped by it, so the only player a permission can change anything for is
its holder, and the only permanents it can change anything about are their opponents' hexproof
ones. "Creatures your opponents control with hexproof can be targeted by spells and abilities
you control as though they didn't have hexproof" and "this player is not stopped by hexproof"
describe the same set of legal aims.

Hexproof is enforced in exactly one predicate, and that predicate is what both the announcement
gate and the CR 608.2b resolution re-check run. So the permission is consulted in one place and
honoured in both by construction: a creature targeted while the permission is in force stays a
legal target when the spell resolves, and a spell aimed on an earlier turn does not become legal
because a permission was granted on a later one.

### Creating a replacement effect

`create_replacement` creates a **one-shot replacement effect** (CR 614.1b) that lasts for the
rest of the turn and is spent by the first event it applies to — the `The next time a … would
… this turn, … instead` a card prints. Mistcaller:

```json
{ "kind": "create_replacement",
  "replacement": {
    "kind": "exile_entering",
    "entering": { "card_type": "creature", "nontoken": true, "not_cast": true } } }
```

It is the third per-turn thing an ability can put into the state, recorded exactly like
`allow_casting_from_graveyard` and `ignore_hexproof` — on a list carrying the turn it was
created on, dropped at the same turn boundary. It differs in one way, and that is the `next
time`: **applying it removes it**, so it can never do its job twice.

It names no target and no player. A replacement watches an *event*, and which events it
watches is the replacement's own filter — a class of thing that might happen, chosen when the
card was written rather than aimed when the ability was activated.

`exile_entering` is today's only replacement: a permanent that would enter the battlefield is
**exiled instead**. The event is cancelled rather than modified, so nothing enters, no
enters-the-battlefield trigger is collected, and every other replacement that was applicable to
that entry stops applying — there is no longer an entry to modify. A card goes to its owner's
exile; a token simply ceases to exist (CR 111.7).

Its `entering` filter is a small product of independent restrictions, every one of which
defaults to "no restriction":

| Field | Meaning |
| --- | --- |
| `card_type` | Only an entering permanent with this printed card type |
| `nontoken` | Only a permanent that is not a token (CR 111) |
| `not_cast` | Only a permanent that got there **without being cast** |

`not_cast` is the one fact about an entry that cannot be read off the object: the same creature
card reanimated and cast produces the same permanent, and it is recorded at the single seam
where a resolving permanent spell becomes a permanent.

**Self-replacements are the same layer.** `enters_tapped` and `enters_with_counters` are
collected alongside whatever an ability created, so when more than one applies to the same
entry the affected permanent's **controller** — not the effects' controller — chooses which
applies first (CR 616.1), through the mid-resolution choice queue every other player decision
rides. Each applies at most once to one event (CR 614.5), which is what makes the loop
terminate. `enters_choosing_color` and `enters_naming_card` are deliberately not among them:
they are questions, not modifications anyone could order them against. Each is instead an
**answer slot on the entry event** the seam refuses to finish while it is empty, so an answer
fills its slot and hands the same event straight back — which is what lets a card ask more
than one without any code saying which comes first, and what makes the whole loop terminate
for the same reason CR 614.5 does.

### Preventing damage

`prevent_damage` raises a **damage-prevention shield** for the rest of the turn (CR 615.1).
Root Snare:

```json
{ "kind": "prevent_damage", "damage": { "combat_only": true } }
```

Prevention is a replacement effect, and it is consulted at the single seam damage is dealt —
so combat damage, a burn spell, a sweeper, and a fight are covered by one shield and by one
piece of code. Damage that is prevented is **never dealt**: it is not marked on a permanent
(CR 120.3d), so it never feeds the lethal-damage state-based action (CR 704.5g); it is not
life loss (CR 120.3a); it gains a lifelink source nothing (CR 702.15e); it removes no loyalty
(CR 120.3c); and it is reported nowhere, because there is no damage event to report.

Its `damage` filter is the same shape `exile_entering`'s `entering` is — independent
restrictions, each defaulting to "no restriction", so an omitted filter prevents every point
of damage anyone would deal:

| Field | Meaning |
| --- | --- |
| `combat_only` | Only **combat** damage (CR 510.1), including a trampler's excess |

It differs from `create_replacement` in exactly one way, and it is the duration. `The next
time …` is spent by applying it; `this turn` is not, and covers every damage event until the
turn ends — so a shield is recorded with a duration rather than on the one-shot list, and it
ends in the **cleanup step** alongside the pumps and the marked damage (CR 514.2) rather than
at the turn boundary.

Like a replacement it names no target and no player: it watches an *event*, so it prevents
damage **anyone** would deal, which is what `all combat damage` says. Nothing yet prevents a
fixed amount, names a recipient or a source, or makes damage unpreventable.

### Abilities that function from a graveyard

An **activated or triggered** ability that returns **its own card** from a graveyard
functions there rather than on the battlefield (CR 113.6). Reassembling Skeleton:

```json
{ "type": "activated",
  "cost": [{ "kind": "mana", "mana": "{1}{B}" }],
  "effects": [{ "kind": "return_self_from_graveyard", "destination": "battlefield_tapped" }] }
```

`destination` is the `hand` / `battlefield` / `battlefield_tapped` set a `search_library`
takes. There is no field saying *where* the ability works: it is derived from the effect,
because an ability that moves its own card out of a graveyard could function nowhere else —
on the battlefield its source is a permanent, and there is no card in a graveyard for it to
move. The derivation reads the whole effect **tree**, so the return may sit inside a `may`
and still say where its ability works. Spit Flame:

```json
{ "type": "triggered",
  "event": { "permanent_enters": { "scope": "creatures_you_control", "subtype": "Dragon" } },
  "effects": [{ "kind": "may", "cost": "{R}",
                "effects": [{ "kind": "return_self_from_graveyard", "destination": "hand" }] }] }
```

Three consequences follow, and each is enforced:

- **The offer follows the card.** While the card is in its controller's graveyard an
  activated ability is offered beside their hand and battlefield activations, bound by the
  same priority and timing rules a hand cast is, and re-checked at apply. While the card is
  anywhere else — a hand, the battlefield, exile — it is not offered at all.
- **So does the trigger.** A graveyard is a third source list the trigger collector walks,
  beside the battlefield and the emblems. Which list reads a given ability is decided by
  the ability: one that returns its own card from a graveyard fires only from a graveyard,
  and every other ability fires only from the others, so nothing can fire twice. The "you"
  of such a trigger is the seat whose graveyard the card is in.
- **An activated ability's cost is mana and nothing else.** A card in a zone is not a
  permanent: it cannot be tapped, sacrificed, or have counters removed. A definition that
  authors this effect outside an activated or triggered ability — on a spell's own
  effects, or on an ability handed to an emblem — or beside an activation cost of any
  other kind, fails the build (`GraveyardAbilityCannotFunction`).

The card does not move when the ability is activated — only when it resolves — so removing
it in response leaves an ability that resolves and does nothing.

### Effects on the ability's own source

`pump_self`, `put_counters_on_self`, and `alter_abilities_self` act on the permanent
whose ability is resolving.
The source is not a *target* (CR 115.1), so these choose nothing, fill no slot, and never
fizzle; a source that has left the battlefield is simply not there to modify. They are
meaningless on a spell, which has no source permanent.
`return_self_from_graveyard` above is the same shape over a source that is a *card in a
zone* rather than a permanent.

### Mass, non-targeting modifications

`pump_all` and `grant_keyword_all` modify a **class** until end of turn rather than a
target, so they choose nothing and never fizzle:

```json
{ "kind": "pump_all", "affects": { "scope": "creatures_you_control" }, "power": 2, "toughness": 1 }
```

The `scope` values are `creatures_you_control`, `each_creature`,
`creatures_your_opponents_control`, `creatures_without_flying`, and
`attacking_creatures`. The first three are read relative to the effect's controller so one
authored card means "you" from either seat; `creatures_without_flying` reads flying through
the computed keywords, so a *granted* flying excludes a creature exactly as a printed one
does; and `attacking_creatures` is read off the declaration the combat step produced, so it
is empty outside combat. `deal_damage` takes the same set.

`creatures_you_control` additionally takes a `subtype`, which narrows the class to a tribe
and replaces the noun in the generated text — `{"scope": "creatures_you_control", "subtype":
"Dragon"}` reads as "Dragons you control" — and a `min_power`, the "each creature you control
**with power 4 or greater**" of an attack trigger:

```json
{ "kind": "grant_keyword_all",
  "affects": { "scope": "creatures_you_control", "min_power": 4 },
  "keyword": "trample" }
```

`min_power` is read through the **computed** characteristics (CR 613.1f), so a creature pumped
to 4 is in the class and one shrunk below it is out. Asking for a computed power is safe here
and refused inside a static ability's condition, and the difference is *when* the question is
asked: a mass effect enumerates its class during a resolution, from outside the layer system,
where there is no computation to recurse into.

The affected set is locked in on resolution (CR 611.2c) — a creature that arrives later in
the turn is untouched. That is the whole difference between one of these and an
`Ability::Static` anthem, which is re-derived on every read.

`destroy_all` is the mass counterpart of `destroy` (CR 701.7), and it takes its **own**
scope vocabulary rather than the one above:

```json
{ "kind": "destroy_all", "affects": { "scope": "each_artifact_or_enchantment" } }
```

Its scopes are `each_creature` and `each_artifact_or_enchantment`. Separate because every
member of the set above is a class of *creatures* feeding a pump or a keyword grant, and a
non-creature scope there would make "artifacts you control get +1/+1" an authorable
sentence that means nothing. Like every mass effect the set is enumerated on resolution,
and each member leaves through the same destruction seam a single `destroy` uses — so a
token ceases to exist (CR 111.7) and a death trigger sees every one of them.

### Tapping a whole seat's creatures, and skipping an untap step

`tap_all` taps every creature a **named player** controls, and may flag those same
creatures to sit out that player's next untap step (CR 502.4):

```json
{ "kind": "tap_all", "player_ref": "target_player", "skip_next_untap": true }
```

Its subject is the `player_ref` a `mill` takes, with the same rule — `target_player` fills
a slot and can fizzle, `each_opponent` fills none and cannot. It is deliberately **not** a
`pump_all`-style `affects` class: every one of those is read relative to the effect's
controller and none of them targets, so "creatures *that player* controls" is unsayable
there and sayable here without inventing anything.

The skip rides on this effect rather than beside it as a second one, for the reason a
`pump` carries its `keywords`: one effect declares one target group, so two effects would
advertise two slots and let a player tap one seat's creatures while stopping another
seat's untapping.

The skip is a **flag, not a countdown**. A card names one untap step, so the flag is spent
at the first untap step its controller reaches — whether or not the permanent is still
tapped by then, since a flag left set would go on skipping every untap step for the rest of
the game. It is stored on the permanent beside its damage, not computed: nothing about it
is a continuous effect, it does not end at cleanup, and no layer applies to it. It rides the
wire as `skips_next_untap` (`docs/protocol.md`), because the spell that imposed it is gone
and no client could work it out.

Untapping a permanent on its own is a separate verb the IR does not have, and stays in the
exclusions; the one untap that exists rides on `gain_control` below.

### Gaining control of a permanent (CR 613 layer 2)

`gain_control` takes the creature it targets until end of turn, and may untap it and grant
it keywords in the same breath:

```json
{ "kind": "gain_control", "target": "any_creature", "untap": true, "keywords": ["haste"] }
```

The control change is applied at **CR 613 layer 2**, the earliest layer the engine models
and the one the most rules read: who may attack with the permanent, who may activate its
abilities, whose `creatures you control` counts it, and who its combat damage comes from
all read the same computed answer. Layer 2 is applied *before* layers 6 and 7c, so an
anthem lets go of a creature that has been taken and picks up one that has been given.

It is **computed, never written onto the permanent** (ADR 0005). Two consequences follow,
and both are the reason it is done this way:

- the effect's `until end of turn` duration is ended by the cleanup step exactly as a
  pump's is, and control simply reverts — nothing is put back, and two changes in force
  resolve to the later timestamp with the earlier one still underneath;
- the permanent's *stored* controller goes on standing in for its **owner**, so a creature
  that dies while stolen goes to its owner's graveyard (CR 400.7), and the same is true of
  a bounce, an exile, or a trip to the top of a library.

A control change **re-triggers summoning sickness** (CR 302.6): the creature has not been
under its new controller's control since their turn began. That is why the printed cards
that do this grant haste, and why `untap` and `keywords` ride on this effect rather than
beside it as two more — one effect declares one target group, so three effects would
advertise three slots and let a player steal one creature, untap a second, and haste a
third.

Only the until-end-of-turn duration is expressible, and nothing exchanges control of two
permanents; both stay in the exclusions.

### Static abilities (continuous, CR 604.3)

A printed static ability applies continuously for as long as its source is on the
battlefield, with nothing ever put on the stack — an anthem or a lord:

```json
{
  "type": "static",
  "affects": { "scope": "creatures_you_control", "subtype": "Elf", "except_this": true },
  "modification": { "kind": "power_toughness", "power": 1, "toughness": 1 }
}
```

- `affects` names the class. `creatures_you_control` takes `subtype` (which restricts it
  to a lord's tribe — "other **Elves** you control"), `keyword` (the "with **defender**"
  of "each creature you control with defender"), and `except_this` (the "other" in a
  lord's wording, comparing the *permanent* rather than the card, so two copies of one
  lord do pump each other). `{"scope": "source"}` is the class of one — the "this
  creature" of a card that modifies itself, which flows through the same selector, the
  same timestamp, and the same layer as an anthem rather than needing a path of its own.

  Both `subtype` and `keyword` are matched against the **printed** face. That is forced
  rather than chosen: this selector is evaluated from *inside* the computation of the
  affected permanent's characteristics, and asking for a computed keyword there is asking
  the layer-6 fold for the answer it is in the middle of producing. So a creature that was
  *granted* defender is outside "each creature you control with defender". The trigger
  selector's `keyword` (see [Trigger conditions](#trigger-conditions)) runs outside the
  layer system and does read the computed set; the asymmetry is that recursion and nothing
  else.

  `permanents_your_opponents_control` is the one class that reaches past the source's own
  controller: it takes a `card_type` ("**lands** your opponents control") and
  `with_the_named_card`, which narrows it to permanents whose card is the one the source
  named as it entered. Every part of it is re-asked on each read, which is the whole
  difference between it and a `pump_all` — a land that arrives afterwards is in the class
  the moment it arrives, and one that changes hands leaves it at CR 613 layer 2.
- `modification` is one of six, and two of them are not in a layer at all:
  - `power_toughness` — layer 7c, folded after counters in timestamp order.
  - `grant_keyword` — layer 6, idempotent.
  - `assigns_combat_damage_by` — **no layer**. The affected creatures assign combat damage
    equal to the named `characteristic` (`power` or `toughness`) rather than to their
    power (CR 510.1a, modified), read at the one place the combat-damage step asks how
    much a creature assigns. Everything downstream follows for free: trample's excess
    (CR 702.19e) is what is left of that amount after each blocker's lethal, and the
    marked damage the lethal-damage state-based action reads (CR 704.5g) is what of it was
    dealt.

    **This is not a power-setting effect**, and the distinction is the card rather than a
    nicety. The creature's power is untouched — every evasion rule, every selector with a
    power bound, and the power a client is shown all keep reading it — because CR 613
    orders effects that change *characteristics* and this changes none. A layer-7b
    `set power to toughness` would be visible to all of them and would be a different
    card.
  - `attacks_as_though_no_defender` — **no layer**. The affected creatures may be declared
    as attackers even though they have defender (CR 702.3b applied as though absent,
    CR 609.4), read at the attacker declaration and nowhere else.

    **This is not keyword removal.** The creature still has defender for every other
    purpose: a card that counts creatures with defender still counts it, the keyword line
    still prints it, and — the case that matters — the `keyword` filter of the very
    selector that granted the permission still matches it. A permission built out of
    `loses_keyword` would take the creature out of the class that granted it. It also
    permits exactly one thing: a tapped creature, a summoning-sick one, and one under a
    `cant_attack` restriction all still cannot attack.
  - `lose_all_abilities` and `grant_ability` — layer 6, the same addition and subtraction an
    attachment makes, reached from a printed static ability instead:

    ```json
    {"kind": "grant_ability",
     "ability": {"type": "activated", "cost": [{"kind": "tap"}],
                 "effects": [{"kind": "add_mana_any_color", "amount": 1}]}}
    ```

    A granted ability is folded into the affected permanent's set by the one accessor every
    collector reads, so it is offered, paid for, and fired by the code a printed ability goes
    through. Two static abilities on one card share their source's timestamp and apply in the
    order the card lists them, which is how "lose all abilities **and** have …" is written as
    two entries and still means one thing.
- `condition` is the optional `as long as …` clause. Absent is unconditional, which is
  what every anthem and lord says:

  ```json
  {"type": "static", "affects": {"scope": "source"},
   "modification": {"kind": "power_toughness", "power": 1, "toughness": 0},
   "condition": {"kind": "controls_at_least",
                 "permanents": {"card_type": "artifact"}}}
  ```

  The conditions are `controls_at_least` — the same `permanents` selector an
  intervening-if counts, with `count` defaulting to the "an" of "as long as you control
  **an** artifact" — `source_is_attacking`, `source_is_enchanted_or_equipped` (the
  source has something attached to it, CR 303.4 / CR 301.5 — one condition because a card
  prints one, and only the attachment's own kind tells the two words apart), and
  `source_has_not_dealt_damage`. It is a separate vocabulary from the `Condition` an
  `Effect::Conditional` takes, because most of that one's variants ask what a *resolution*
  or a *turn* has already done and a continuous ability is neither: it has no window of its
  own to read and no start to measure from.

  ```json
  {"type": "static", "affects": {"scope": "source"},
   "modification": {"kind": "grant_keyword", "keyword": "hexproof"},
   "condition": {"kind": "source_has_not_dealt_damage"}}
  ```

  `source_has_not_dealt_damage` is the "…**yet**" of a card that is protected until it
  strikes, and its window is the permanent's whole life on the battlefield — not a turn and
  not a resolution, which is exactly why it belongs here rather than in the intervening-if
  vocabulary. It is the one condition answered from a fact stored on the permanent rather
  than from the event log: the log is a bounded ring and records what damage was dealt *to*,
  never by what, so a long enough game would forget the very hit the condition exists to
  notice. Stored is not latched — the clause is re-asked on every read, so the keyword is
  gone in the same batch the damage lands in — and a permanent that leaves and returns is a
  new object that has dealt nothing (CR 400.7). The fact is written at the three seams a
  permanent is the *source* of damage: combat damage, a `fight`, and the damage verb of an
  ability whose source is a permanent (CR 609.7).

  A `permanents` selector gains a `color` alongside its `card_type` and `subtype`, which
  is what lets a card ask for "a **blue** creature" or "an **Ajani** planeswalker".

The effect is **derived from the battlefield on every read, never stored**: it starts and
stops with its source, so a static ability cannot outlive the permanent that printed it —
and a conditional one starts and stops with its condition, for the same reason and with
nothing to prune.
Rules text is composed from the same selector the engine applies, so the sentence and the
scope cannot disagree.

### Continuous abilities about a player (CR 402.2)

A static ability whose subject is a **player** rather than a permanent is its own ability
kind, not a widening of `static`:

```json
{ "type": "player_static", "modification": { "kind": "no_maximum_hand_size" } }
```

The two share nothing but the word "continuous". A `static`'s `affects` names a class of
permanents and its `modification` names a CR 613 layer; neither has anything to say about
a player, and one variant carrying both vocabularies could express `{"affects": "source",
"modification": "no_maximum_hand_size"}` — nonsense the loader would then have to reject
at runtime instead of the type rejecting it outright.

The subject is always the source's **controller**: every printed ability of this shape
says "you", so there is no selector to author and none to get wrong.

There are two modifications. Each is read where its question is asked, never applied
anywhere, so it takes effect the instant its source is on the battlefield and stops the
instant it leaves — the same derived-on-every-read rule a `static` follows, with nothing
stored and nothing to prune. Emblems are walked alongside the battlefield, exactly as the
characteristics loop walks them.

- `no_maximum_hand_size` (CR 402.2), read by `sage_engine::maximum_hand_size`.
- `play_lands_from_graveyard` (CR 305.9 — Crucible of Worlds), read by
  `sage_engine::plays_lands_from_graveyard` where the land play is offered. A land is
  **played**, never cast (CR 116.2a), so this is not the permission
  `allow_casting_from_graveyard` grants and could not be: that one is granted for a turn
  by a resolved effect and reaches spells. Everything else about the play is unchanged —
  one land per turn, the active player's, at sorcery speed — because those gates are asked
  of the play rather than of the zone it came from, so a land played out of a graveyard
  spends the turn's land drop like any other.

**No maximum is a distinct state, not a large number.** The predicate answers
`Option<usize>` and the view carries `{"cards": n}` or `"unlimited"`; a sentinel would be
a number nobody printed that every reader would have to recognise.

### Cost modification (CR 601.2f)

A continuous ability whose subject is a **spell** is its own ability kind for the reason
`player_static` is: a `static`'s `affects` names a class of permanents and its `modification`
names a CR 613 layer, and a cost modification is neither — it applies while a spell is being
cast, before the object it produces exists.

```json
{ "type": "cost_modifier",
  "spells": { "creature": { "min_power": 4 } },
  "modification": { "kind": "reduce", "generic": 2 } }
```

- `spells` names the class, in the same vocabulary a
  [cast trigger](#trigger-conditions) watches.
- `modification` is `reduce` or `increase`, each carrying a `generic` amount.

**Only the generic component moves**, in both directions. A coloured or `{C}` requirement is
untouched, which is what every printed reducer says: `{2}` off a `{4}{G}` leaves `{2}{G}`, and
a seat with no green source still cannot cast it. Two variants rather than one signed number
because CR 601.2f applies them at different moments — the total is the printed cost *plus*
every additional cost and cost increase, *minus* every cost reduction, and only the result is
held at `{0}`. A `{1}` spell under a `{2}` tax and a `{2}` reduction therefore costs `{1}`,
not `{2}`.

The caster is always the source's **controller** — the "you cast" every printed ability of
this shape says — so there is no scope to author and none to get wrong.

Like the other two continuous kinds it is **derived on every read, never stored**: the
discount begins the instant its source is on the battlefield and ends the instant it leaves,
with nothing to prune. It is read by `sage_engine::total_cast_cost`, which is the single
answer every road that touches a cast's price goes through — the offer (`valid_actions`), the
pip enumeration, the payment search, the legality gate, the charge, and the view. The idle
predicate joins them by construction, since it asks `valid_actions` of a board with its mana
floated rather than reading a cost of its own. That single answer is the whole point: a
modification applied at only one of those sites would advertise casts a seat cannot take,
auto-pass a seat that has a play, or offer a discount the charge then refuses.

### Effects that ask a player to choose cards

Four effects stop mid-resolution and hand one named player a decision (issue #604). The
game does not proceed until it is answered, and the answer is validated against the zone
as it is *at that moment* — see `docs/decisions/0013-mid-resolution-player-choices.md`.

| `kind` | Asks | Aftermath |
| --- | --- | --- |
| `discard` | `count` cards of `player_ref`'s hand | they go to that player's graveyard |
| `scry` | any number of the top `count` cards | the chosen go to the bottom, in the chosen order |
| `look_at_top` | up to `take` of the top `count` | the chosen go to `destination`, the rest to the bottom in a random order |
| `search_library` | up to `take` cards of the library | the chosen go to `destination`, then the library is shuffled |

```json
{ "kind": "discard", "player_ref": "target_opponent", "count": 1,
  "chosen_by": "controller", "filter": { "kind": "noncreature_nonland" } }
```

- `player_ref` decides whether a discard **targets**, exactly as it does for `mill` — so
  both "target player discards two cards" and "each opponent discards a card" are
  writable, and neither restates the fizzle rule. `scry`, `look_at_top`, and
  `search_library` act on the controller's own library and never target.
- `chosen_by` is `owner` (the default — the discarding player picks) or `controller` (the
  spell's controller picks, the hand-attack shape). The chooser is also the only seat the
  cards are shown to.
- `filter` narrows which cards may be picked: `any` (the default), `land`,
  `creature` with an optional `max_power` and an optional `subtype`, `noncreature_nonland`,
  `creature_or_land` (one class as a card writes it, not two), `permanent` (CR 110.1 — what a
  search that puts its find straight onto the battlefield names), `subtype` (a card with that
  printed subtype whatever its card type — "a **Zombie** card" is not "a Zombie **creature**
  card"), `same_name_as_source` ("a card named *this card*", matched on printed identity so
  two copies of one printing find each other), `color` (a card of a printed colour —
  `{"kind": "color", "color": "white"}` — read off the colour indicator rather than the mana
  cost, so a colourless artifact matches none and a gold card matches each of its own),
  `instant_or_sorcery` (one class as a card writes it), or `artifact`.

  The same filter vocabulary is what `milled_this_way` and `allow_casting_from_graveyard` read,
  so a Zombie is a Zombie in all three.
- `destination` is `hand` (the default), `battlefield`, or `battlefield_tapped`. A card
  entering the battlefield this way goes through the same seam a resolving permanent
  spell uses, so its "enters tapped"/"enters with counters" replacements and its ETB
  triggers all fire.

A question with **no legal answer is never asked**: an empty hand, an empty library, or a
look that turns up nothing matching applies the effect with an empty selection and
resolves — including the aftermath, so a look that whiffs still bottoms what it looked at
and a search that finds nothing still shuffles (CR 701.19c).

Two orderings are deliberately **not** modeled and are listed in the exclusions: the
cards a scry keeps on top stay in their printed order, and the cards a `look_at_top`
bottoms go there at random rather than in an order the player picks.

### Creating tokens (CR 111)

`create_token` puts a permanent onto the battlefield that is **not a card**. The token's
whole printed face is authored inline, because the effect that creates it *is* its face
(CR 111.3):

```json
{ "kind": "create_token",
  "token": { "name": "Thopter", "types": ["artifact", "creature"],
             "subtypes": ["Thopter"], "colors": [], "power": 1, "toughness": 1,
             "keywords": ["flying"] } }
{ "kind": "create_token", "count": 2, "tapped": true, "attacking": true,
  "token": { "name": "Cat", "types": ["creature"], "subtypes": ["Cat"],
             "colors": ["white"], "power": 1, "toughness": 1,
             "keywords": ["lifelink"] } }
```

- `count` is how many are created (default `1`), or how many **per counted permanent**
  when `count_of` is present (see *Amounts derived from a count*). Each is a **separate
  object** with its own battlefield identity, so an "enters the battlefield" watcher sees
  two entries for two tokens.
- `tapped` (default `false`) is the entry state the creating effect dictates.
- `attacking` (default `false`) puts each token into the combat already in progress
  (CR 506.3c), and is a **sibling** of `tapped` rather than a mode of it — Leonin
  Warleader says both.
- `player_ref` names **who creates them**, and therefore who controls them, exactly as it
  names whose library a `mill` empties: `controller` (the default), `each_opponent`, or a
  targeting `target_player` / `target_opponent`.

**An attacking token is never told what to attack**, because no card says: it attacks the
same player or planeswalker the effect's own source is attacking. Every way that can fail
to name an attack is the same answer — the effect resolving outside combat, its source
removed from combat before it resolved, a source that is a spell rather than a permanent,
or a token created under some other seat's control — and in each the tokens are created
and simply are not attacking. The effect never invents a defender.

It was never **declared** as an attacker (CR 506.3c), which is the rest of the rule:
attacking does not tap it (only `tapped` does), summoning sickness does not restrict it,
and no "whenever this creature attacks" ability triggers for it. Everything else about it
is an ordinary attacker — a defender may block it, and it deals combat damage in that
combat.

A `token` block takes `name`, `types`, and optionally `subtypes`, `colors`, `power`,
`toughness`, `keywords`, `restrictions`, and `abilities` — the same vocabulary a card
uses for each. What it **cannot** take is as deliberate: no `functional_id` (a token is
not a card, is not decklist-legal, and never appears in the compatibility report), no
`mana_cost`, no `spell_effects`, no `attachment`, and no `scripted`. Those fields do not exist
on the type, so writing one is a parse error rather than a rule to remember. The
validator additionally rejects a token that is not a permanent (it could exist in no
zone) and one whose power/toughness disagrees with being a creature.

A token is an ordinary permanent while it is on the battlefield — it attacks, blocks, is
targeted, takes damage, bears counters, and dies. It differs in one place: **the instant
it would leave the battlefield it ceases to exist** (CR 111.7). A token that dies reaches
no graveyard (though the death is real and a dies trigger still sees it, CR 603.6c), a
bounced token never arrives in a hand, and an exiled token is not in exile. See
`docs/decisions/0015-tokens.md`.

Creating a token as a *copy* of another permanent is out of scope and listed in the
exclusions: copiable values are decided at CR 613 layer 1, ahead of every layer the
engine applies.

### Effects a player may decline

`may` wraps other effects in a yes-or-no its controller answers mid-resolution (issue
#610). It rides the same queue, routing, and never-stall rules as the card choices above
— see `docs/decisions/0014-optional-effects.md`.

```json
{ "kind": "may", "effects": [{ "kind": "draw_card", "count": 1 }] }
{ "kind": "may", "cost": "{1}",
  "effects": [{ "kind": "draw_card", "count": 1 }] }
```

- The first reads "you may draw a card"; the second, "you may pay {1}. If you do, draw a
  card". Rules text is composed from the wrapped effects, so the printed sentence and the
  question the player is asked are the same words.
- **The controller answers**, whoever else the surrounding ability names and whoever holds
  priority. A trigger that goes on the stack during an opponent's turn still asks its own
  controller.
- `cost` is a mana cost in the same `{...}` notation an activation cost uses, paid from
  the controller's pool. While the question is owed they may activate **mana abilities**
  and nothing else (CR 605.3a), so a cost is payable if the board could still make the
  mana. A cost no amount of tapping could pay is never asked at all — it is declined, and
  recorded as declined.
- **Declining is not a fizzle.** The wrapped effects are skipped; every other effect of
  the same ability, and the spell's own trip to its final zone (CR 608.3), happen exactly
  as if the `may` were not there.
- **The target is chosen up front.** A `may` over a single targeting effect declares that
  effect's target group as its own, so the slot is filled at announcement (CR 601.2c) and
  only the yes-or-no waits for resolution:

  ```json
  { "kind": "may", "effects": [{ "kind": "destroy", "target": "any_artifact_or_enchantment" }] }
  ```

  Accepting hands the chosen target back to the wrapped effect, which re-checks it
  (CR 608.2c); declining drops the target with the rest of the offer, so the effect after
  it still takes the target *it* was aimed at. An object whose every target has become
  illegal never resolves (CR 608.2b), and the question is not asked at all.
- **One group, still.** A `may` over **two** targeting effects is rejected
  (`Violation::TwoTargetsInsideOptional`): one forwarding cannot advertise two slots, and
  the flat stored target list would have no way to pair them back. The "at most one
  variable-arity group" rule looks through the wrapper too.
- Choosing an optional cost at *announcement* (kicker) is a different mechanism and is
  still excluded, and so is a **reflexive** trigger (CR 603.11) — a `when you do` aimed
  *after* a cost is paid chooses its target at a moment no announcement has reached.

### Trigger conditions

Conditions about the ability's **own source**, and about its controller, are authored as
bare strings: `self_enters_battlefield`, `self_dies`, `self_attacks`, `you_gain_life`,
`you_draw_card`. A condition that carries a selector wraps it:

```json
{ "type": "triggered",
  "event": { "permanent_dies": { "scope": "any_creature", "except_this": true } },
  "effects": [{ "kind": "lose_life", "player_ref": "each_opponent", "amount": 1 }] }
```

`permanent_enters`, `permanent_dies`, and `permanent_attacks` take an observed-permanent
selector — `scope` is
`creatures_you_control` or `any_creature`, with an optional `subtype`, an `except_this` that
means "another", and a `nontoken` that excludes tokens (CR 111 — a token is not a card),
which is what keeps a card that makes tokens off a loop with its own trigger. An optional
`max_power` narrows it to creatures of at most that power — "whenever another creature you
control **with power 2 or less** enters" — and an optional `keyword` to creatures that have
it, "whenever a creature **with flying** attacks". Both are read through the computed
characteristics of the state the event happened in, so a creature that entered pumped is
judged by what it was then, one that died shrunk by what it was as it died, and one that was
*granted* flying is a flier for exactly as long as the grant lasts.
`you_cast_spell` takes a **class of spell**: the bare strings `enchantment`, `artifact`,
`instant_or_sorcery`, and `chosen_color`, or the wrapped `{"creature": {"min_power": 4}}`.
The classes are read off the printed types and do not exclude each other — an artifact
creature spell is an `artifact` spell *and* a `creature` spell (CR 205.2b).
`chosen_color` is the one whose meaning comes from elsewhere on the same card, and is
described under [a colour named as a permanent enters](#a-colour-named-as-a-permanent-enters-cr-61412);
`creature` takes an optional `min_power`, and `{"creature": {}}` is every creature spell.
The same vocabulary names the spells a [cost modifier](#cost-modification-cr-6012f) applies
to, because both abilities ask one question of one card. Its `min_power` is read off the
**printed** power — unlike the mass-effect and permanent-count bounds above — and that is
the only reading available rather than a simplification: the class is asked about a card in
a hand, a graveyard, or on the stack, which has no permanent and no computed
characteristics of any kind.

`ability_activated` watches a player activating an ability (CR 602.2), with two optional
filters: `activator` is `any` (the default) or `opponents`, and `source_types` names the
permanent types whose abilities count, satisfied by any one of them.

```json
{ "type": "triggered",
  "event": { "ability_activated": { "activator": "opponents",
                                    "source_types": ["creature", "land"] } },
  "effects": [{ "kind": "may", "effects": [{ "kind": "draw_card", "count": 1 }] }] }
```

**A mana ability never fires it** (CR 605.3a), and no card has to say so: the condition
looks at the objects a transition put on the stack, and a mana ability resolves without
ever using one.

`beginning_of_step` is about the turn rather than about an object:

```json
{ "type": "triggered",
  "event": { "beginning_of_step": { "step": "upkeep", "whose_turn": "yours" } },
  "effects": [{ "kind": "gain_life", "player_ref": "controller", "amount": 1 }] }
```

`step` is `upkeep`, `draw`, `begin_combat`, or `end_step` — the four steps printed cards
trigger at, and deliberately not every step of the turn: all four grant priority, so a
trigger owed at one is answered in the step it belongs to. `whose_turn` is `yours` (only
the controller's own turn) or `each` (every turn), and that choice is most of what such an
ability means: "each upkeep" fires twice as often as "your upkeep" and is otherwise the
same card.

Every condition is observed by diffing the state before and after an action, never by a
listener. A condition about an **event** rather than a board position (life gain, casting,
a draw, a step beginning) is read from the events that transition recorded, because gaining
and losing the same life leaves every total unchanged and still triggered — and because one
pass of priority can walk through several steps at once, so comparing the step before with
the step after would miss every crossing but the last. A draw is the same shape: a card
drawn and then discarded leaves the hand the size it was, and each card drawn is its own
event, so a two-card draw fires a draw-watcher twice.

A watching condition reports **how many times** it was met, not whether: two creatures
dying at once trigger a death-watcher twice. A watching ability must still be on the
battlefield afterwards, except a death-watcher, which observes a creature that died
alongside it.

A triggered ability reaches the stack **unaimed** and its controller is then asked to
choose its targets (CR 603.3d), so a trigger whose effect targets works exactly as a
spell's does — including the resolution-time re-check that fizzles it if its target has
gone (CR 608.2b). A trigger with no legal choice for one of its slots is never put on the
stack at all (CR 603.3c), so authoring one costs nothing when the board cannot answer it.

The full `abilities`, `spell_effects`, target, cost, and attachment shapes are the enums in
`crates/sage-engine/src/ability.rs`. Those Rust types are authoritative; do not reproduce
the IR in a second documentation schema that can drift.

## Closed schema and generated text

`CardData` uses `deny_unknown_fields`. A definition cannot contain exact Oracle text,
flavor text, image paths or URLs, official symbols or frames, artist credit, watermarks,
or arbitrary presentation fields. Unknown fields fail parsing.

Definitions contain no rules-prose field. `crates/sage-server/src/rules_text.rs` generates
`CardView.rules_text` from the same structured behavior the engine executes. Formatter
matches are exhaustive, so adding an IR variant without display support fails compilation.

A scripted card is the exception because its Rust behavior cannot be inspected by the
formatter. It must provide its own non-Oracle explanatory text beside the code in
`src/scripted.rs`. Loader validation requires the catalog’s `scripted` flag and the code
registration to agree in both directions.

## Printing record

```json
{
  "functional_id": "skyscanner",
  "collector_number": "19",
  "rarity": "common"
}
```

All three fields are required. `rarity` is one of `common`, `uncommon`, `rare`, or
`mythic`. The set code comes from the file name. A printing must resolve to an existing
functional definition, and collector numbers must be unique within a set.

## Identity model

| Layer | Type | Assigned by | Lifetime |
| --- | --- | --- | --- |
| Functional card | `FunctionalId` | Card author | Stable across builds |
| Engine handle | `CardId` (`OracleId` alias) | Build script | One catalog build |
| Printing | set code + collector number | Set file | Stable bibliography |
| Game object | `CardInstanceId`, `PermanentId` | Engine | One game or battlefield stay |

Never persist or hand-author a `CardId`. Adding a definition can change sorted interning and
renumber handles. Printings, decklists, tests, and scripted code use `FunctionalId` and
resolve it through `CardDatabase::card_id` when a handle is needed.

## Adding a card

1. Add `data/catalog/<functional_id>.json` with schema version `1` and a matching id.
2. Add or update a set file if the card needs a printing record.
3. Add behavior tests using the card’s `functional_id`.
4. Run `make check`.

Adding a functional definition creates one catalog file; adding a printing may also edit its
set file.

## Validation

The build and loader reject:

- unknown fields or malformed JSON;
- unsupported schema versions;
- malformed, duplicate, or file-mismatched functional ids;
- missing types or invalid creature power/toughness;
- a planeswalker with no `loyalty`, or a `loyalty` on anything else;
- an `attachment` whose `kind` names a subtype the card does not have;
- an Equipment with no `equip` cost, or an `equip` cost on an Aura;
- printed `restrictions` on a card that is not a creature;
- an optional effect's contents, or a conditional's branches, choosing a target;
- a `create_emblem` handing out anything but a static or triggered ability (CR 114.1);
- two variable-arity (`up_to`) target groups in one ability or spell;
- a `create_token` describing an object that could not be a permanent, or a creature token
  with no power/toughness;
- unresolved printing references or duplicate collector numbers; and
- disagreement between a scripted definition and `src/scripted.rs`.

Breaking schema changes increment `SCHEMA_VERSION` and migrate the entire catalog in the
same change. Unsupported versions fail rather than being skipped.
