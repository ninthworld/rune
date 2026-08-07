//! Continuous effects that modify a **rule** rather than a characteristic — the ones CR
//! 613 does not order, because there is no characteristic for a layer to be about.
//!
//! Deliberately **not** routed through [`characteristics`](super::characteristics), and
//! for a sharper reason than [`controller_of`](super::controller_of) is not: control is
//! at least a fact about the permanent that every later layer reads. A rule modification
//! is a fact about *one rule*. Folding it into [`Characteristics`](super::Characteristics)
//! would mean every reader of a permanent could see it, and the whole content of
//! [`RuleModification`] is that they cannot — a creature assigning combat damage by its
//! toughness has the power it always had, and a creature attacking as though it had no
//! defender has defender.
//!
//! So each rule asks its own question here, at the one place the rule is read. The two
//! questions share [`rule_modifications`], which gathers from the same two sources every
//! layer does: the stored [`GameState::static_effects`] and the printed static abilities
//! in force ([`static_ability_effects`]). Attachments are not a third source, because
//! [`Attachment`](crate::Attachment) has no field that could carry one; when a card prints
//! `equipped creature assigns combat damage by its toughness`, that is where it goes.
//!
//! Nothing here can recurse. `static_ability_effects` reads printed faces, stored
//! effects, and [`controller_of`](super::controller_of) — never a computed
//! characteristic — which is what makes it safe for the layer folds to call, and safe
//! here for the same reason.

use super::*;
use crate::card::{DamageCharacteristic, RuleModification};

/// Which characteristic the permanent identified by `permanent` assigns combat damage by
/// (CR 510.1a, as modified by [`RuleModification::AssignsCombatDamageBy`]).
///
/// [`DamageCharacteristic::Power`] unless a continuous effect says otherwise, which is
/// the unmodified rule rather than a fallback. When more than one effect applies the
/// **latest timestamp** wins (CR 613.7, the ordering every other continuous effect
/// follows); today no card can produce a second one, so the sort exists to make that
/// answer determinate rather than incidental.
///
/// A permanent not on the battlefield assigns by power, which is what the combat-damage
/// step's `0` for a missing permanent already means.
#[must_use]
pub fn assigns_combat_damage_by(
    state: &GameState,
    permanent: PermanentId,
    db: &CardDatabase,
) -> DamageCharacteristic {
    rule_modifications(state, permanent, db)
        .into_iter()
        .filter_map(|modification| match modification {
            RuleModification::AssignsCombatDamageBy { characteristic } => Some(characteristic),
            RuleModification::AttacksAsThoughNoDefender
            | RuleModification::DoesNotUntap
            | RuleModification::CannotHaveCountersPut
            | RuleModification::ExiledInsteadOfLeavingBattlefield => None,
        })
        .next_back()
        .unwrap_or_default()
}

/// Whether the permanent identified by `permanent` may be declared as an attacker as
/// though it did not have defender (CR 609.4 over CR 702.3b).
///
/// **An as-though permission, not a keyword removal**, and every consequence of that is
/// in what this function does *not* do: it does not touch
/// [`Characteristics::keywords`](super::Characteristics::keywords), so the creature still
/// has defender for the selector that granted the permission, for a card that counts
/// creatures with defender, and for the keyword line a client prints. The single reader
/// is [`defender_stops_attacking`](crate::combat::defender_stops_attacking).
///
/// It also permits exactly one thing. A creature that is tapped, summoning sick, or under
/// a [`CombatRestriction::CantAttack`](crate::CombatRestriction::CantAttack) still cannot
/// attack: those are other rules, and an as-though clause modifies the rule it names.
#[must_use]
pub fn attacks_as_though_no_defender(
    state: &GameState,
    permanent: PermanentId,
    db: &CardDatabase,
) -> bool {
    rule_modifications(state, permanent, db).contains(&RuleModification::AttacksAsThoughNoDefender)
}

/// Whether the permanent identified by `permanent` **does not untap** during its
/// controller's untap step (CR 502.4, as modified by [`RuleModification::DoesNotUntap`]).
///
/// The single reader is the untap step's turn-based action. Like the two questions above
/// it, this changes no characteristic: a permanent under it stays exactly as tapped as it
/// was, and every selector that asks about tapped-ness reads that unchanged answer.
///
/// It is asked of the permanent rather than of the effect that grants it, so an Aura that
/// leaves takes the restriction with it on the very next read — nothing has to be cleared.
#[must_use]
pub fn does_not_untap(state: &GameState, permanent: PermanentId, db: &CardDatabase) -> bool {
    rule_modifications(state, permanent, db).contains(&RuleModification::DoesNotUntap)
}

/// Whether counters **can't be put on** the permanent identified by `permanent`
/// (CR 614.1b, as stated by [`RuleModification::CannotHaveCountersPut`]).
///
/// The single reader is the counter seam
/// ([`GameState::put_counters_on_permanent`](crate::GameState)), which is what makes the
/// prohibition one fact rather than one per road a counter could arrive by. Asked of the
/// permanent rather than of the effect granting it, so a source that leaves the
/// battlefield stops forbidding anything on the very next read.
///
/// It says nothing about the counters already there, and nothing about removing them: the
/// rule it modifies is the one that would have put a counter on.
#[must_use]
pub fn cannot_have_counters_put_on(
    state: &GameState,
    permanent: PermanentId,
    db: &CardDatabase,
) -> bool {
    rule_modifications(state, permanent, db).contains(&RuleModification::CannotHaveCountersPut)
}

/// Whether the permanent identified by `permanent` is **exiled instead of being put
/// anywhere else** when it would leave the battlefield (CR 614.1a, as stated by
/// [`RuleModification::ExiledInsteadOfLeavingBattlefield`]).
///
/// Read by every zone seam that takes a permanent off the battlefield, which is what makes
/// "would leave the battlefield" one fact rather than one per road out. The exile seam
/// does not ask: a permanent already headed for exile is going where this would send it.
///
/// It redirects the destination and nothing else. The permanent still *leaves*, so a dies
/// trigger, an Aura falling off, and every other watcher of a departure see what they
/// always saw.
///
/// Read straight off [`GameState::static_effects`](crate::GameState) rather than through
/// [`rule_modifications`], for [`player_cannot_get_counters`]'s reason turned the other
/// way round: this is asked from inside the zone seams, three of which take no card
/// database at all, and it never needs one. The effect is only ever *created* by the
/// resolution that reanimated the permanent, keyed to it by id — no printed static ability
/// grants it, and none could, because it is about one specific object that did not exist
/// when any card was written.
#[must_use]
pub fn exiled_instead_of_leaving(state: &GameState, permanent: PermanentId) -> bool {
    state.static_effects.iter().any(|effect| {
        effect.affects == crate::state::EffectAffects::SpecificPermanent(permanent)
            && effect.modification
                == crate::state::Modification::ModifyRule(
                    RuleModification::ExiledInsteadOfLeavingBattlefield,
                )
    })
}

/// The player-side twin: whether the player in `seat` **can't get counters**
/// (CR 614.1b).
///
/// Read straight off [`GameState::static_effects`](crate::GameState) rather than through
/// [`rule_modifications`], because that function is about a permanent from its first line
/// — it looks one up on the battlefield and reads its printed face. A player has neither.
/// What the two share is the part that matters: an effect keyed to a source with
/// [`Duration::WhileOnBattlefield`](crate::Duration) stops applying the moment its source
/// is gone, and the state-based-action loop prunes it with nothing here to clear.
///
/// Printed static abilities are not a second source here, and could not be: a printed
/// ability applies to a class of objects, and no class of objects contains a player.
#[must_use]
pub fn player_cannot_get_counters(state: &GameState, seat: crate::id::PlayerId) -> bool {
    state.static_effects.iter().any(|effect| {
        effect.affects == crate::state::EffectAffects::SpecificPlayer(seat)
            && effect.modification
                == crate::state::Modification::ModifyRule(RuleModification::CannotHaveCountersPut)
    })
}

/// Every [`RuleModification`] currently applying to the permanent identified by
/// `permanent`, in ascending timestamp order (CR 613.7 — the source object's id), so a
/// reader that has to pick one picks the last.
///
/// Empty for a permanent that is not on the battlefield, or whose card is absent from
/// `db` — the same unknown-id fallback [`characteristics`](super::characteristics) gives,
/// and the same reason: there is no answer to compute and the engine forbids panicking.
fn rule_modifications(
    state: &GameState,
    permanent: PermanentId,
    db: &CardDatabase,
) -> Vec<RuleModification> {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return Vec::new();
    };
    let Some(face) = perm.printed.face(db) else {
        return Vec::new();
    };
    // The same gate the layer folds pass to the selector: an "each creature you control"
    // scope is about creatures, and current type is printed type until layers 1–5 land.
    let is_creature = face.has_type(CardType::Creature);
    let mut effects: Vec<StaticEffect> = state
        .static_effects
        .iter()
        .filter(|effect| affects(state, effect, perm, is_creature))
        .cloned()
        .collect();
    effects.extend(static_ability_effects(state, perm, is_creature, db));
    effects.sort_by_key(StaticEffect::timestamp);
    effects
        .into_iter()
        .filter_map(|effect| match effect.modification {
            Modification::ModifyRule(modification) => Some(modification),
            // Every other modification is in a layer, and is read from the computed
            // characteristics rather than from here.
            Modification::PowerToughness { .. }
            | Modification::AddTypes { .. }
            | Modification::SetBasePowerToughness { .. }
            | Modification::GrantKeyword(_)
            | Modification::LoseKeyword(_)
            | Modification::GrantAbility(_)
            | Modification::LoseAllAbilities
            | Modification::GrantRestriction(_)
            | Modification::GainControl(_) => None,
        })
        .collect()
}
