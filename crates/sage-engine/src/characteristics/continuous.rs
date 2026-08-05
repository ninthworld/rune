//! Where a continuous effect comes from, for either layer: a printed static ability on a
//! permanent or an emblem, an Aura's contribution derived on read, and the check that one
//! applies to the permanent being computed.

use super::*;

/// Every continuous effect a **printed static ability** (CR 604.3) currently
/// contributes to `perm` — the anthem and lord shape.
///
/// Synthesized on every call, never stored (ADR 0005 §1). A static ability functions
/// exactly while its source is in force, and that is a fact about the current state, so
/// pushing an entry into [`GameState::static_effects`] on entry and pruning it on
/// departure would be bookkeeping that can desync. Deriving it cannot.
///
/// **Two source lists, not one.** The battlefield is where a printed static ability
/// almost always lives, but an [`Emblem`] (CR 114) is a source of continuous effects
/// that is on no battlefield and never will be — so this walks both, in that order.
/// Ordering between the two lists does not decide anything on its own: every
/// contribution is timestamped by its **source's** object id (CR 613.7) and the caller
/// sorts by that, so an emblem created before an anthem entered applies before it,
/// exactly as two anthems do. Each contribution is keyed to `perm` by
/// [`EffectAffects::SpecificPermanent`], so both fold through the existing ordering path
/// unchanged.
pub(super) fn static_ability_effects(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    db: &CardDatabase,
) -> Vec<StaticEffect> {
    let mut effects = Vec::new();
    let mut collect = |source: StaticSource, abilities: Vec<Ability>| {
        for ability in abilities {
            let Ability::Static {
                affects,
                modification,
                condition,
            } = ability
            else {
                continue;
            };
            if !static_affects_match(state, &affects, source, perm, is_creature, db) {
                continue;
            }
            // The `as long as …` clause, re-asked on this read rather than remembered
            // from a previous one — which is the whole reason a conditional continuous
            // ability needs no pruning.
            if !static_condition_holds(state, condition.as_ref(), source, db) {
                continue;
            }
            effects.push(StaticEffect {
                source: source.timestamp,
                affects: EffectAffects::SpecificPermanent(perm.id),
                modification: modification.to_modification(),
                duration: Duration::WhileOnBattlefield,
            });
        }
    };
    for source in &state.battlefield {
        collect(
            StaticSource {
                timestamp: source.id.0,
                // CR 613 layer 2 applies before this one, so the "you" of a lord that
                // has changed hands is its *new* controller.
                controller: controller_of(state, source),
                permanent: Some(source.id),
            },
            abilities_of_permanent(db, source),
        );
    }
    // CR 114.1: an emblem's abilities function from nowhere — it is in no zone, so the
    // battlefield walk above can never see it. This is the second list, and the whole of
    // what makes an emblem's static ability real.
    for emblem in &state.emblems {
        collect(
            StaticSource {
                timestamp: emblem.id,
                controller: emblem.controller,
                permanent: None,
            },
            emblem.abilities.clone(),
        );
    }
    effects
}

/// The object a printed [`Ability::Static`] is on, reduced to the three facts the
/// selector needs: its CR 613.7 timestamp, whose "you" it speaks in, and — only if it
/// is a permanent — which one, for the "other" of a lord.
///
/// A plain struct rather than an enum over permanent/emblem because the difference is
/// exactly one `Option`: an emblem is not a permanent, so it can never be the "this" an
/// `except_this` excludes, and answering `None` says so without a match arm.
#[derive(Clone, Copy)]
struct StaticSource {
    /// The source's object id, which is its CR 613.7 timestamp.
    timestamp: u64,
    /// The source's controller — the "you" of "creatures you control".
    controller: crate::id::PlayerId,
    /// The source permanent, or `None` for an emblem.
    permanent: Option<PermanentId>,
}

/// Whether a printed static ability on `source` applies to `perm`.
///
/// The subtype test reads `perm`'s **printed** subtypes. That is correct today and
/// also the only non-recursive option: the type-changing layers (1–5) are not
/// implemented, so printed subtypes are current subtypes, and asking for `perm`'s
/// computed subtypes from inside the computation of `perm`'s characteristics would
/// not terminate. When those layers land, this is the call site that must start
/// reading a computed value — through a seam that cannot recurse.
///
/// Control is already read that way: [`controller_of`] is layer 2, applied before this
/// layer, and it *is* a seam that cannot recurse (it reads stored effects only). So an
/// anthem stops pumping a creature the moment someone else gains control of it, and
/// starts pumping one it just stole.
fn static_affects_match(
    state: &GameState,
    affects: &StaticAffects,
    source: StaticSource,
    perm: &Permanent,
    is_creature: bool,
    db: &CardDatabase,
) -> bool {
    match affects {
        StaticAffects::CreaturesYouControl {
            subtype,
            except_this,
        } => {
            if !is_creature || controller_of(state, perm) != source.controller {
                return false;
            }
            // "Other …" excludes the source itself. `PermanentId` is minted fresh on
            // every battlefield entry, so this compares the specific object, not the
            // card — two copies of one lord do pump each other. An emblem is not a
            // permanent and so is never the excluded "this".
            if *except_this && source.permanent == Some(perm.id) {
                return false;
            }
            match subtype {
                None => true,
                Some(wanted) => perm
                    .printed
                    .face(db)
                    .is_some_and(|face| face.has_subtype(wanted)),
            }
        }
        // A class of one. An emblem has no source permanent, so a `source` static on one
        // applies to nothing — which is what `None == Some(perm.id)` says.
        StaticAffects::Source => source.permanent == Some(perm.id),
    }
}

/// Whether a printed static ability's `as long as …` clause currently holds for a
/// source on `source`. `None` — an unconditional ability — always holds.
///
/// The board question delegates to the one permanent counter every other selector uses
/// ([`crate::condition::count_permanents`]), so "you control an artifact" means the same
/// thing here as it does in an intervening if. It reads **printed** types, which is both
/// correct today and the only non-recursive option: asking for computed characteristics
/// from inside the computation of a permanent's characteristics would not terminate.
fn static_condition_holds(
    state: &GameState,
    condition: Option<&StaticCondition>,
    source: StaticSource,
    db: &CardDatabase,
) -> bool {
    match condition {
        None => true,
        Some(StaticCondition::ControlsAtLeast { permanents, count }) => {
            crate::condition::count_permanents(state, permanents, source.controller, db) >= *count
        }
        Some(StaticCondition::SourceIsAttacking) => source.permanent.is_some_and(|id| {
            state
                .battlefield
                .iter()
                .any(|p| p.id == id && p.attacking.is_some())
        }),
    }
}

/// The layer-7c power/toughness [`StaticEffect`] a single attached Aura `aura`
/// contributes to its host (CR 303.4 / 613.7c), or `None` if `aura` is not an
/// attached Aura (no host, or its card carries no [`AuraGrant`](crate::AuraGrant)).
///
/// Synthesized on demand rather than stored (ADR 0005): its `source` is the Aura's
/// own object id — a strictly increasing, replayable timestamp (CR 613.7) — and it
/// is keyed to the specific host permanent, so it folds in exactly like a pump
/// keyed to that permanent, and disappears when the Aura leaves.
pub(super) fn aura_pt_effect(aura: &Permanent, db: &CardDatabase) -> Option<StaticEffect> {
    let host = aura.attached_to?;
    let grant = aura.printed.face(db)?.aura()?;
    Some(StaticEffect {
        source: aura.id.0,
        affects: EffectAffects::SpecificPermanent(host),
        modification: Modification::PowerToughness {
            power: grant.power,
            toughness: grant.toughness,
        },
        duration: Duration::WhileOnBattlefield,
    })
}

/// Whether `effect` applies to `perm`, given whether `perm` is currently a
/// creature. Encodes the [`EffectAffects`] selector semantics in one place.
///
/// The controller test is the **layer-2** answer ([`controller_of`]), which is applied
/// before every layer this gate feeds — so a class-scoped modifier follows a permanent
/// that changes hands rather than staying with the seat it was created against.
pub(super) fn affects(
    state: &GameState,
    effect: &StaticEffect,
    perm: &Permanent,
    is_creature: bool,
) -> bool {
    match effect.affects {
        EffectAffects::CreaturesControlledBy(player) => {
            is_creature && controller_of(state, perm) == player
        }
        // A pump targets one specific permanent by its battlefield identity
        // (CR 601.2c). Layer 7c only adjusts an existing power/toughness, so a
        // pump landed on a non-creature (which has none) is folded into `None`
        // and has no visible effect — no `is_creature` gate is needed here.
        EffectAffects::SpecificPermanent(id) => perm.id == id,
    }
}
