//! CR 613 **layer 6**, ability-adding *and* ability-removing: the keywords, combat
//! restrictions, and abilities a permanent currently has, printed ones folded together
//! with every grant and every removal in force, in timestamp order.

use super::*;

/// The permanent's *current* keyword set at CR 613 **layer 6** (CR 613.1f):
/// `printed` folded through every layer-6 modification in force, **in timestamp
/// order** — a grant adds, a removal subtracts, and a loses-all clears.
///
/// Duplicates are collapsed so a redundant grant is idempotent (CR 702, "having a
/// keyword ability twice is the same as having it once"), and a removal takes the
/// keyword out however it got there: by the time this layer applies, a granted keyword
/// is indistinguishable from a printed one.
///
/// **Order is the whole of CR 613.1f.** Grants alone commute, but the layer subtracts
/// now, so `loses defender` followed by `gains defender` leaves it present and the
/// reverse leaves it absent. That is why this folds [`ordered_layer_six_effects`]
/// rather than an unordered bag.
///
/// `is_creature` gates the anthem-style "creatures you control" selector.
pub(super) fn current_keywords(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    printed: Vec<Keyword>,
    db: &CardDatabase,
) -> Vec<Keyword> {
    let mut keywords = printed;
    for effect in ordered_layer_six_effects(state, perm, is_creature, db) {
        match effect.modification {
            Modification::GrantKeyword(keyword) => {
                if !keywords.contains(&keyword) {
                    keywords.push(keyword);
                }
            }
            Modification::LoseKeyword(keyword) => keywords.retain(|held| *held != keyword),
            // A keyword ability is an ability (CR 702.1), so losing all of them loses
            // these too — and only the ones that applied *before* this timestamp.
            Modification::LoseAllAbilities => keywords.clear(),
            Modification::GrantRestriction(_)
            | Modification::GrantAbility(_)
            | Modification::PowerToughness { .. }
            // Layers 4 and 7b change what a permanent is and how big it is; neither
            // adds or removes an ability.
            | Modification::AddTypes { .. }
            | Modification::SetBasePowerToughness { .. }
            // A rule modification adds and removes no keyword — that is the whole of
            // what distinguishes an as-though permission from
            // [`Modification::LoseKeyword`] above it.
            | Modification::ModifyRule(_)
            | Modification::GainControl(_) => {}
        }
    }
    keywords
}

/// The permanent's *current* combat restrictions at CR 613 **layer 6** (CR 613.1f):
/// `printed` folded through the same [`ordered_layer_six_effects`] list
/// [`current_keywords`] folds — one traversal of the battlefield answers both halves of
/// the layer, so a source that grants a keyword *and* imposes a restriction (an Aura may
/// do both) can never be honoured for one and missed for the other.
///
/// Duplicates are collapsed so a redundant imposition is idempotent. A printed
/// restriction is a printed ability, so a loses-all clears these as well; there is no
/// per-restriction removal, because no card prints one.
pub(super) fn current_restrictions(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    printed: Vec<CombatRestriction>,
    db: &CardDatabase,
) -> Vec<CombatRestriction> {
    let mut restrictions = printed;
    for effect in ordered_layer_six_effects(state, perm, is_creature, db) {
        match effect.modification {
            Modification::GrantRestriction(restriction) => {
                if !restrictions.contains(&restriction) {
                    restrictions.push(restriction);
                }
            }
            Modification::LoseAllAbilities => restrictions.clear(),
            Modification::GrantKeyword(_)
            | Modification::LoseKeyword(_)
            | Modification::GrantAbility(_)
            | Modification::PowerToughness { .. }
            | Modification::AddTypes { .. }
            | Modification::SetBasePowerToughness { .. }
            | Modification::ModifyRule(_)
            | Modification::GainControl(_) => {}
        }
    }
    restrictions
}

/// The permanent's *current* ability set at CR 613 **layer 6** (CR 613.1f): `printed`
/// folded through every ability-adding and ability-removing effect in force, in
/// timestamp order — a grant appends, and a loses-all clears everything granted or
/// printed before it.
///
/// The non-keyword half of [`current_keywords`], and the answer
/// [`abilities_of_permanent`](crate::abilities_of_permanent) returns — which is what
/// makes a granted activation offered by [`valid_actions`](crate::valid_actions), a
/// granted mana ability still one (CR 605.1a), and a granted trigger collected, each by
/// the code a printed ability already goes through.
///
/// **It folds rather than answering a boolean, and it has to.** While nothing in the IR
/// could grant a written-out ability, "has this lost everything?" *was* the ordered
/// answer; now an Aura hung on a silenced permanent afterwards gives it an ability, and
/// only the timestamps say so.
///
/// Unlike the keyword fold this one does not collapse duplicates: two Auras each saying
/// `{T}: Add {G}` are two activations, and a granted copy of an ability the host prints
/// is a second copy (CR 613.1f adds, it does not merge).
///
/// It folds all **three** sources, printed static abilities included, because a printed
/// static ability can now take abilities away and hand others out — that is the whole of
/// Alpine Moon. The recursion that would imply is cut in exactly one place:
/// [`static_ability_effects`] gates each *source* with [`stored_abilities`] rather than
/// with this function, so the walk goes one level deep and stops.
pub(crate) fn current_abilities(
    state: &GameState,
    perm: &Permanent,
    printed: Vec<Ability>,
    db: &CardDatabase,
) -> Vec<Ability> {
    fold_abilities(printed, ordered_ability_effects(state, perm, db))
}

/// [`current_abilities`] over the **stored** sources only — until-end-of-turn effects and
/// the attachments on `perm` — with no printed static ability consulted.
///
/// The non-recursive answer, and the one the static-ability collector uses to ask whether
/// a source still has the ability it is about to contribute. It is a strictly smaller
/// answer than [`current_abilities`], deliberately: see
/// [`stored_abilities_of_permanent`](crate::stored_abilities_of_permanent) for what that
/// costs and why the cost is the right one.
pub(crate) fn stored_abilities(
    state: &GameState,
    perm: &Permanent,
    printed: Vec<Ability>,
    db: &CardDatabase,
) -> Vec<Ability> {
    let is_creature = printed_is_creature(perm, db);
    fold_abilities(
        printed,
        ordered_layer_six_sources(state, perm, is_creature, db),
    )
}

/// Fold `effects` — already in ascending timestamp order — over `printed`, keeping only
/// the two modifications that are about written-out abilities. Shared by the two answers
/// above so they can never fold the same list differently.
fn fold_abilities(printed: Vec<Ability>, effects: Vec<StaticEffect>) -> Vec<Ability> {
    let mut abilities = printed;
    for effect in effects {
        match effect.modification {
            Modification::GrantAbility(granted) => abilities.push(*granted),
            // CR 613.1f: everything that applied *before* this timestamp goes, printed
            // and granted alike; a grant after it still grants.
            Modification::LoseAllAbilities => abilities.clear(),
            Modification::GrantKeyword(_)
            | Modification::LoseKeyword(_)
            | Modification::GrantRestriction(_)
            // A rule modification is in no layer and is not an ability, so it
            // contributes nothing to the set.
            | Modification::ModifyRule(_)
            | Modification::PowerToughness { .. }
            | Modification::AddTypes { .. }
            | Modification::SetBasePowerToughness { .. }
            | Modification::GainControl(_) => {}
        }
    }
    abilities
}

/// Whether `perm` is currently a creature, read from its **printed** face.
///
/// The reading every selector in this module makes — the type-changing layers are not
/// implemented, so printed types are current types — and the only non-recursive one
/// available to a caller that is itself computing `perm`'s characteristics.
fn printed_is_creature(perm: &Permanent, db: &CardDatabase) -> bool {
    perm.printed
        .face(db)
        .is_some_and(|face| face.has_type(CardType::Creature))
}

/// The CR 613 layer-6 effects on `perm` that come from the two **stored** sources —
/// [`GameState::static_effects`] and the attachments on `perm` — sorted by ascending
/// timestamp (CR 613.7).
///
/// The subset [`stored_abilities`] folds, and the reason there is a subset at all: the
/// third source, a printed static ability ([`static_ability_effects`]), is collected by
/// reading each source permanent's *abilities*, so a walk that asked for those in full
/// would not terminate. This list is where that walk bottoms out.
///
/// `is_creature` gates the anthem-style selector on a stored effect; ability-shaped
/// modifications are always keyed to one permanent, so it only ever matters to the
/// caller that also wants keywords.
fn ordered_layer_six_sources(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    db: &CardDatabase,
) -> Vec<StaticEffect> {
    // Stored continuous effects (until-end-of-turn grants and removals) that apply here.
    // Cloned rather than copied: a restriction naming a subtype owns a `String`, and a
    // granted ability owns its whole tree.
    let mut effects: Vec<StaticEffect> = state
        .static_effects
        .iter()
        .filter(|effect| affects(state, effect, perm, is_creature))
        .cloned()
        .collect();
    // CR 303.4 / 301.5 / 613.1f: each attachment on `perm` — an Aura or an Equipment —
    // grants its listed keywords, abilities, and restrictions while attached. One walk
    // for all three: a keyword an Equipment grants is a keyword, an ability an Aura
    // grants is an ability, and nothing that reads this list has any business knowing
    // where a member came from.
    for attachment in &state.battlefield {
        if attachment.attached_to != Some(perm.id) {
            continue;
        }
        let Some(grant) = attachment
            .printed
            .face(db)
            .and_then(|face| face.attachment())
        else {
            continue;
        };
        let mut push = |modification| {
            effects.push(StaticEffect {
                source: attachment.id.0,
                affects: EffectAffects::SpecificPermanent(perm.id),
                modification,
                duration: Duration::WhileOnBattlefield,
            });
        };
        for keyword in &grant.keywords {
            push(Modification::GrantKeyword(*keyword));
        }
        for restriction in &grant.restrictions {
            push(Modification::GrantRestriction(restriction.clone()));
        }
        for ability in &grant.abilities {
            push(Modification::GrantAbility(Box::new(ability.clone())));
        }
    }
    effects.sort_by_key(StaticEffect::timestamp);
    effects
}

/// [`ordered_layer_six_effects`] for the ability question, with `is_creature` answered
/// from the **printed** face.
///
/// The one caller is [`current_abilities`], reached from
/// [`abilities_of_permanent`](crate::abilities_of_permanent), which has no computed
/// characteristics to consult and must not ask for any. Reading the printed type is what
/// [`characteristics`](crate::characteristics::characteristics) does too — the
/// type-changing layers are not implemented — and it is what gates the one selector on
/// this path that cares, `creatures you control`.
fn ordered_ability_effects(
    state: &GameState,
    perm: &Permanent,
    db: &CardDatabase,
) -> Vec<StaticEffect> {
    ordered_layer_six_effects(state, perm, printed_is_creature(perm, db), db)
}

/// Every CR 613 **layer 6** effect currently applying to `perm` — the ability-adding
/// and ability-removing layer (CR 613.1f) — sorted by ascending timestamp
/// ([`StaticEffect::timestamp`], i.e. the source object's id, CR 613.7).
///
/// Three sources feed it, mirroring [`ordered_pt_modifiers`] (ADR 0005 §4): the two
/// [`ordered_layer_six_sources`] collects — the stored [`GameState::static_effects`] and
/// the **attachments** on `perm`, whose [`Attachment`](crate::Attachment) grants are read
/// off the attachment (CR 303.4 / 301.5 / 613.1f) and so vanish the instant it leaves or
/// is moved to another host (ADR 0005) — plus each printed static ability in force
/// ([`static_ability_effects`], the anthem and lord shape), which only this caller may
/// ask for.
///
/// The sort is stable, so the several modifications one effect contributes (a
/// `loses defender and gains flying` shares one minted timestamp, being one continuous
/// effect) stay in the order they were pushed: subtraction first, then addition, which
/// is the order the card prints them in. Layer-7c P/T modifications that happen to reach
/// the same permanent are returned too and ignored by both folds above.
pub(super) fn ordered_layer_six_effects(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    db: &CardDatabase,
) -> Vec<StaticEffect> {
    let mut effects = ordered_layer_six_sources(state, perm, is_creature, db);
    // CR 604.3 / 613.1f: a printed static ability in force ("Creatures you control have
    // vigilance"), derived from its source's battlefield presence alone.
    effects.extend(static_ability_effects(state, perm, is_creature, db));
    effects.sort_by_key(StaticEffect::timestamp);
    effects
}
