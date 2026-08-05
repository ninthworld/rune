//! CR 613 **layer 6**, ability-adding: the keywords and combat restrictions a permanent
//! currently has, printed ones folded together with every grant in force.

use super::*;

/// The permanent's *current* keyword set at CR 613 **layer 6** (CR 613.1f):
/// `printed` plus every keyword granted to `perm` by a continuous effect, with
/// duplicates collapsed so a redundant grant is idempotent (CR 702, "having a
/// keyword ability twice is the same as having it once").
///
/// Two sources feed the grants, mirroring [`ordered_pt_modifiers`] (ADR 0005 §4):
/// the stored [`GameState::static_effects`] carrying [`Modification::GrantKeyword`]
/// (anthems and until-end-of-turn pumps) and, synthesized fresh, each Aura attached
/// to `perm` whose [`AuraGrant`](crate::AuraGrant) lists keywords (CR 303.4 /
/// 613.1f). Layer 6 grants are timestamp-independent for a pure keyword grant, so —
/// unlike the layer-7c P/T folds — no ordering is imposed. `is_creature` gates the
/// anthem-style "creatures you control" selector.
pub(super) fn current_keywords(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    printed: Vec<Keyword>,
    db: &CardDatabase,
) -> Vec<Keyword> {
    let mut keywords = printed;
    for modification in layer_six_modifications(state, perm, is_creature, db) {
        if let Modification::GrantKeyword(keyword) = modification {
            if !keywords.contains(&keyword) {
                keywords.push(keyword);
            }
        }
    }
    keywords
}

/// The permanent's *current* combat restrictions at CR 613 **layer 6** (CR 613.1f):
/// `printed` plus every restriction imposed on `perm` by a continuous effect, with
/// duplicates collapsed so a redundant imposition is idempotent.
///
/// The exact counterpart of [`current_keywords`], reading the same
/// [`layer_six_modifications`] list — one traversal of the battlefield answers both
/// halves of the layer, so a source that grants a keyword *and* imposes a restriction
/// (an Aura may do both) can never be honoured for one and missed for the other.
pub(super) fn current_restrictions(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    printed: Vec<CombatRestriction>,
    db: &CardDatabase,
) -> Vec<CombatRestriction> {
    let mut restrictions = printed;
    for modification in layer_six_modifications(state, perm, is_creature, db) {
        if let Modification::GrantRestriction(restriction) = modification {
            if !restrictions.contains(&restriction) {
                restrictions.push(restriction);
            }
        }
    }
    restrictions
}

/// Every CR 613 **layer 6** [`Modification`] a continuous effect currently applies to
/// `perm` — the ability-adding layer (CR 613.1f), in no particular order.
///
/// Three sources feed it, mirroring [`ordered_pt_modifiers`] (ADR 0005 §4): the stored
/// [`GameState::static_effects`] (until-end-of-turn grants and impositions), each
/// printed static ability in force ([`static_ability_effects`], the anthem and lord
/// shape), and — synthesized fresh — each Aura attached to `perm`, whose
/// [`AuraGrant`](crate::AuraGrant) keywords and restrictions are read off the
/// attachment (CR 303.4 / 613.1f) and so vanish the instant the Aura leaves (ADR 0005).
///
/// Layer 6 is timestamp-independent for a pure grant, so no ordering is imposed:
/// a grant either adds something or finds it already there. Layer-7c P/T modifications
/// that happen to reach the same permanent are not returned.
pub(super) fn layer_six_modifications(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    db: &CardDatabase,
) -> Vec<Modification> {
    let mut modifications = Vec::new();
    // Stored continuous effects (until-end-of-turn grants) that apply to this permanent.
    modifications.extend(
        state
            .static_effects
            .iter()
            .filter(|effect| affects(state, effect, perm, is_creature))
            .map(|effect| effect.modification),
    );
    // CR 604.3 / 613.1f: a printed static ability in force ("Creatures you control have
    // vigilance"), derived from its source's battlefield presence alone.
    modifications.extend(
        static_ability_effects(state, perm, is_creature, db)
            .into_iter()
            .map(|effect| effect.modification),
    );
    // CR 303.4 / 613.1f: each Aura attached to `perm` grants its listed keywords and
    // imposes its listed restrictions while attached.
    for aura in &state.battlefield {
        if aura.attached_to != Some(perm.id) {
            continue;
        }
        if let Some(grant) = aura.printed.face(db).and_then(|face| face.aura()) {
            modifications.extend(
                grant
                    .keywords
                    .iter()
                    .copied()
                    .map(Modification::GrantKeyword),
            );
            modifications.extend(
                grant
                    .restrictions
                    .iter()
                    .copied()
                    .map(Modification::GrantRestriction),
            );
        }
    }
    modifications
}
