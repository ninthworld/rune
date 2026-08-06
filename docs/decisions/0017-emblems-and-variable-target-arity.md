# ADR 0017: An object in no zone, and an effect that may name fewer targets than it allows

- Status: accepted
- Date: 2026-07-31

## Context

Every object model the engine had grown up to this point shared two assumptions so quietly
that neither was ever written down.

**One: an ability's source is a permanent.** ADR 0005 made a permanent's characteristics a
pure function of the battlefield, and `characteristics::static_ability_effects` iterates
`state.battlefield` reading `Ability::Static` off each permanent it finds. ADR 0007's
diff-based trigger collector does the same, and its watching conditions add a precondition
on top: *the source must still be on the battlefield after the transition*, because an
ability that has left is not watching the board it is no longer on.

**Two: everything eventually leaves.** A permanent has three leave-the-battlefield seams, a
card is in some zone, a stack object resolves or is countered, and the state-based-action
loop owns the removal path for each. ADR 0015 leaned on this directly: CR 111.7 works
because a token never reaches a destination zone.

An **emblem** (CR 114) satisfies neither. It is not a permanent, has no `PermanentId`, is in
no zone, and nothing in the game removes it — a planeswalker's ultimate can resolve on turn
six and the emblem is still there on turn sixty, with its source long dead. `exclusions.json`
carried "Emblems — no zoneless, controller-scoped continuous object", and three of the five
M19 planeswalkers were unauthorable behind it.

Separately, Ajani's `+1` — *put a +1/+1 counter on each of **up to two** target creatures* —
broke a third unwritten assumption: **one effect fills exactly one target slot**. That is what
`Effect::target_spec() -> Option<TargetSpec>` says, and the whole targeting pipeline was built
on it: announcement (CR 601.2c), the per-slot candidate enumeration of ADR 0004, the legality
gate, and the CR 608.2b resolution re-check all counted slots and effects one for one.

## Decision

### 1. An emblem is its own list on `GameState`, not a permanent with exceptions

`GameState::emblems: Vec<Emblem>`, where an `Emblem` is an object id, a controller, and its
abilities — and nothing else, because CR 114.1 says an emblem has no other characteristics.

The alternative was to put it on the battlefield with flags saying it is not really there.
That was rejected because of what it would cost everywhere else: every state-based action,
every target spec, every combat gate, and every view projection would have had to grow a
clause explaining why it does not apply. A separate list means each of them says *nothing*,
which is the correct answer, and means an emblem cannot be collected by a loop that walks the
battlefield because it is not on the battlefield.

Its `id` comes from the same monotonic `next_object_id` every other object's does. That is
deliberate and load-bearing twice: it is a unique, replay-stable handle, **and** it is the
CR 613.7 timestamp, which is what lets an emblem's anthem fold into the layer system beside a
permanent's without the ordering code learning that emblems exist.

### 2. Both ability paths grow a second source list, not a new selector

`characteristics::static_ability_effects` walks the emblems after the battlefield, and
`triggers::collect_triggers` does the same. Nothing else in the engine reads either list.

The risk flagged before the work was ordering: `characteristics` runs on every rules read and
every view projection, and adding a second collection to its hottest loop could plausibly have
introduced a CR 613.7 timestamp bug invisible in a two-object test. It did not, and the reason
is structural rather than lucky — **neither list's position decides anything**. Every
contribution is timestamped by its own source's object id and the caller sorts by that, so an
emblem created before an anthem entered applies before it, exactly as two anthems do.

The selector needed no new variant. `StaticAffects::CreaturesYouControl` already means
"creatures the source's controller controls", and an emblem *is* controlled; what had to
change was the *source* the selector is evaluated against. Two small types carry that:
`StaticSource` for the characteristics path and `Watcher` for the trigger path, each reducing
a source to what its consumers actually read — a controller, a timestamp, and an
`Option<&Permanent>`. Everything that had been a special case falls out of that `Option` being
`None`: an emblem never enters, never dies, never attacks, and can never be the "this" an
`except_this` excludes. Each of those is one `is_some_and` rather than an arm apiece.

The "source still on the battlefield" precondition the watching conditions carry becomes
`Watcher::still_present`, which answers `true` for an emblem — nothing removes one (CR 114.5),
so the question has one answer.

### 3. An emblem's trigger is read from `before`, not `after`

The two battlefield passes read permanents from whichever snapshot still has them. The emblem
pass reads `before.emblems`, which is the whole of "an ability triggers only for events after
its source exists" (CR 603.6): an emblem created by the very transition that crossed an end
step must not fire on that end step.

### 4. `AbilitySource` states what an ability came from

`StackObjectKind::Ability::source` was a `PermanentId`; it is now
`AbilitySource::Permanent(..) | Emblem(..)`. The sentinel alternative — a reserved id meaning
"no permanent" — was rejected: it compiles everywhere and is silently wrong in the places that
matter, which is the same trap ADR 0015 avoided by retyping `Permanent.card` rather than
adding a field beside it.

`AbilitySource::permanent() -> Option<PermanentId>` is what every existing caller wants, and
it answers `None` for an emblem — the same answer a permanent that has left the battlefield
effectively gave, so self-referential effects needed no new case at all.

### 5. A target *group*, not a target slot

`Effect::target_group() -> Option<TargetGroup>` replaces the slot count of one with
`{spec, min, max}`. Every existing effect is `{min: 1, max: 1}` and behaves exactly as it did.

*Since issue #737 the accessor is `Effect::target_groups() -> Vec<TargetGroup>`* — an ordered
list, because an effect's slots need not share a spec (a fight names "target creature you
control" and "target creature you don't control"). Nothing below changes: an effect that
declares one group is the overwhelming majority and behaves as this section describes, and
`target_group()` remains as the narrowing for the paths that have already established they
are looking at exactly one.

Three consequences, each enforced rather than assumed:

- **Offering.** A group with `min == 0` is never a reason to withhold an ability: choosing
  nothing is a legal announcement (CR 601.2c). The same change closed a real pre-existing gap
  in the other direction — an activated ability whose *mandatory* slot had no candidate used
  to be offered, activated, charged (including a planeswalker's loyalty and its one activation
  for the turn), and then fizzle. It is now withheld.
- **The wire.** A group becomes `max` requirement slots, of which the ones past `min` carry
  `optional: true`. One slot carrying a count was the alternative; it was rejected because it
  makes every existing client and every existing binding path learn about counts to express
  something one card needs, where a flag makes the new case additive and the old case
  byte-identical.
- **Pairing.** Targets are still one flat list per stack object. Fixed groups take their size
  and the slack goes to the one variable group — and *one* is a rule the catalog validator
  enforces (`Violation::TwoVariableTargetGroups`), because with two the split back onto effects
  would be genuinely ambiguous and no announcement could disambiguate it.

Individual re-checking on resolution (CR 608.2c) matters more here than it ever did: for an
"up to two" effect it is the difference between one dead target wasting the whole ability and
it doing half its work.

### 6. A conditional splices, it does not branch

`Effect::Conditional` is applied by rewriting the *remaining effect list* rather than by
recursing into a second application path. The resolve loop became a work queue, and a taken
branch is pushed onto its front — so a branch travels through the same targeting,
choice-posing, and suspension machinery every other effect does. A second path would have had
to reimplement all three.

Two of its three conditions ask what *this resolution* has already done, which no snapshot can
answer: a Zombie already in a graveyard is indistinguishable from one milled a moment ago.
They read the events recorded since the resolution began — the same discipline ADR 0007's
life-gain and cast conditions follow, over a narrower window. The window is a log sequence
carried on `Resume`, so a discard that stops to ask a question still answers
`discarded_this_way` correctly when it wakes up.

## Consequences

- An emblem is the first object with **no removal path**, and code that assumed "everything
  eventually leaves" would be wrong quietly. Two things stop that: the emblem is not on any
  list such code walks, and `tests/emblems.rs` wipes the battlefield and walks forty turns to
  prove nothing collects it.
- `characteristics` walks one more collection on every call. It is empty in every game where
  no ultimate has resolved, so the common case pays a null check.
- `Emblem` is public information with no redaction, so `GameView` and `SpectatorView` carry the
  identical list. It rides beside `battlefield` rather than inside it for the same reason it
  does in the engine.
- The legend rule still keeps the newest copy rather than asking (`exclusions.json`), and an
  emblem still cannot carry an activated ability. Both are named exclusions rather than
  silences.
- Five M19 planeswalkers became authorable, and with them: indestructible, restricted mana,
  graveyard casting, a graveyard→battlefield return, count-derived amounts, and the
  intervening-if. Each is narrow and each is exercised by the card that needed it.
