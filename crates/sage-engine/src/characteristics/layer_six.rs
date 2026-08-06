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
            | Modification::PowerToughness { .. }
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
            | Modification::PowerToughness { .. }
            | Modification::GainControl(_) => {}
        }
    }
    restrictions
}

/// Whether `perm` currently **loses all abilities** at CR 613 layer 6 — the one
/// question every collector that walks a permanent's abilities asks before it walks
/// them ([`abilities_of_permanent`](crate::abilities_of_permanent), which is the only
/// caller that matters because it is the only accessor).
///
/// Reads [`GameState::static_effects`] and nothing else, which is what makes it safe to
/// call from *inside* the computation of a permanent's characteristics — the same
/// property that lets [`controller_of`] answer layer 2 for the layer-6 and layer-7c
/// selectors. A static ability cannot say "loses all abilities" today (no
/// [`StaticModification`](crate::ability::StaticModification) produces one), so there is
/// no derived source to miss and no way for this to recurse.
///
/// **A boolean, not an ordered fold, and that is exact rather than convenient.** A
/// later grant would put an ability back, and the only grants the IR can express are
/// keyword grants — which [`current_keywords`] folds in order. Nothing grants an
/// activated, triggered, or static ability, so there is nothing for an order to decide
/// here.
#[must_use]
pub fn loses_all_abilities(state: &GameState, perm: &Permanent) -> bool {
    state.static_effects.iter().any(|effect| {
        effect.modification == Modification::LoseAllAbilities
            && effect.affects == EffectAffects::SpecificPermanent(perm.id)
    })
}

/// Every CR 613 **layer 6** effect currently applying to `perm` — the ability-adding
/// and ability-removing layer (CR 613.1f) — sorted by ascending timestamp
/// ([`StaticEffect::timestamp`], i.e. the source object's id, CR 613.7).
///
/// Three sources feed it, mirroring [`ordered_pt_modifiers`] (ADR 0005 §4): the stored
/// [`GameState::static_effects`] (until-end-of-turn grants, removals, and impositions),
/// each printed static ability in force ([`static_ability_effects`], the anthem and lord
/// shape), and — synthesized fresh — each **attachment** on `perm`, whose
/// [`Attachment`](crate::Attachment) keywords and restrictions are read off the
/// attachment (CR 303.4 / 301.5 / 613.1f) and so vanish the instant it leaves or is moved
/// to another host (ADR 0005). An attachment's contributions are timestamped by the
/// attachment's own object id, so an Aura hung on a creature *after* it was silenced
/// still grants what it grants.
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
    // Stored continuous effects (until-end-of-turn grants and removals) that apply here.
    // Cloned rather than copied: a restriction naming a subtype owns a `String`.
    let mut effects: Vec<StaticEffect> = state
        .static_effects
        .iter()
        .filter(|effect| affects(state, effect, perm, is_creature))
        .cloned()
        .collect();
    // CR 604.3 / 613.1f: a printed static ability in force ("Creatures you control have
    // vigilance"), derived from its source's battlefield presence alone.
    effects.extend(static_ability_effects(state, perm, is_creature, db));
    // CR 303.4 / 301.5 / 613.1f: each attachment on `perm` — an Aura or an Equipment —
    // grants its listed keywords and imposes its listed restrictions while attached. One
    // walk for both kinds: a keyword an Equipment grants is a keyword, and nothing that
    // reads this list has any business knowing where it came from.
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
    }
    effects.sort_by_key(StaticEffect::timestamp);
    effects
}
