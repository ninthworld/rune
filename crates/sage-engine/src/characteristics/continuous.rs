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
            // CR 613 layer 6 gates the *source*: a lord that has lost all its abilities
            // has no static ability to contribute, so it stops pumping. The gate reads
            // stored effects and attachments only — never another permanent's printed
            // static ability — which is why asking it from inside this computation
            // cannot recurse, and is the one place the layer-6 walk is cut short. It is
            // the same property that lets `controller_of` be read here.
            stored_abilities_of_permanent(state, db, source),
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
/// The **keyword** test reads the printed face for the second of those two reasons and
/// not the first: layer 6 *is* implemented, so a granted keyword is a real keyword — but
/// it is granted by the very fold this selector is called from
/// ([`current_keywords`]), and asking for the answer while computing it would not
/// terminate. So "each creature you control with defender" finds every creature that
/// prints defender and no creature that was handed one. The observer's counterpart
/// ([`crate::ObservedPermanent`]) runs outside the layer system and does read the
/// computed set.
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
            keyword,
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
            let Some(face) = perm.printed.face(db) else {
                return false;
            };
            if let Some(wanted) = subtype {
                if !face.has_subtype(wanted) {
                    return false;
                }
            }
            if let Some(wanted) = keyword {
                if !face.keywords().contains(wanted) {
                    return false;
                }
            }
            true
        }
        // A class of one. An emblem has no source permanent, so a `source` static on one
        // applies to nothing — which is what `None == Some(perm.id)` says.
        StaticAffects::Source => source.permanent == Some(perm.id),
        // The other class of one: whatever the source is attached to right now. Read off
        // the source's own [`Permanent::attached_to`], so an Aura that has moved — or one
        // whose host has left — affects whatever it is on now, which is nothing.
        StaticAffects::AttachedTo => source
            .permanent
            .and_then(|id| state.battlefield.iter().find(|p| p.id == id))
            .and_then(|attachment| attachment.attached_to)
            .is_some_and(|host| host == perm.id),
        // The first class a static ability names that reaches past its own controller.
        // Everything about it is re-asked here, on this read: who controls `perm` right
        // now (layer 2, applied before this one), what it is, and what the source named
        // as it entered. Nothing was decided when the source arrived, which is the whole
        // difference between this and a `pump_all`.
        StaticAffects::PermanentsYourOpponentsControl {
            card_type,
            with_the_named_card,
        } => {
            if controller_of(state, perm) == source.controller {
                return false;
            }
            if let Some(wanted) = card_type {
                let matches = perm
                    .printed
                    .face(db)
                    .is_some_and(|face| face.has_type(*wanted));
                if !matches {
                    return false;
                }
            }
            if !*with_the_named_card {
                return true;
            }
            // "With the chosen name" compares **card identity**, not a string: two
            // printings of one functional card share a `CardId` and nothing else does,
            // and a token has no card at all (CR 111), so it can never bear a name
            // anyone named. A source that named nothing matches nothing.
            let named = source
                .permanent
                .and_then(|id| state.battlefield.iter().find(|p| p.id == id))
                .and_then(|source| source.named_card);
            named.is_some() && named == perm.printed.card()
        }
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
        // CR 303.4 / CR 301.5: "enchanted or equipped" is one question about the host —
        // is anything attached to it — and the attachment's own kind is the only thing
        // that would tell the two words apart. Read off the battlefield on this call, so
        // an Aura resolving onto it turns the ability on and moving the Equipment away
        // turns it off, with nothing to prune.
        Some(StaticCondition::SourceIsEnchantedOrEquipped) => source
            .permanent
            .is_some_and(|id| state.battlefield.iter().any(|p| p.attached_to == Some(id))),
        // Re-asked here on every read, like every other clause: the flag is written at
        // the damage seams and this is the only thing that reads it, so hexproof is gone
        // in the same batch the damage was dealt in rather than at some later resolution.
        // A source that is not a permanent (an emblem) has nothing to have dealt damage
        // and nothing to modify either, so `None` answers no exactly as it does above.
        Some(StaticCondition::SourceHasNotDealtDamage) => source.permanent.is_some_and(|id| {
            state
                .battlefield
                .iter()
                .any(|p| p.id == id && !p.dealt_damage)
        }),
    }
}

/// The layer-7c power/toughness [`StaticEffect`] a single attached permanent
/// contributes to its host (CR 303.4 / 301.5 / 613.7c), or `None` if `attachment` is not
/// attached to anything or carries no [`Attachment`](crate::Attachment) block.
///
/// **One function for both kinds**, which is the whole point of one attachment block: at
/// layer 7c an Equipment's `+2/+1` and an Aura's `+2/+2` are the same kind of thing, so a
/// creature holding a sword and one under an Aura are indistinguishable to every reader
/// of a permanent's power.
///
/// Synthesized on demand rather than stored (ADR 0005): its `source` is the attachment's
/// own object id — a strictly increasing, replayable timestamp (CR 613.7) — and it
/// is keyed to the specific host permanent, so it folds in exactly like a pump
/// keyed to that permanent, and disappears when the attachment leaves *or moves*.
///
/// A grant with an [`Attachment::count_of`](crate::Attachment::count_of) multiplies by
/// that count **here, on every read** rather than at the moment the Aura resolved. This is
/// a static ability (CR 604.3), not a one-shot effect, so CR 608.2 does not apply to it:
/// `+1/+1 for each Forest you control` grows when a Forest arrives and shrinks when one
/// leaves, which is what separates it from the fixed modifier
/// [`Effect::PumpByCount`](crate::Effect::PumpByCount) leaves behind. Counting is safe
/// from inside the layer system for the same two reasons a static ability's condition is:
/// [`count_permanents`](crate::condition::count_permanents) reads printed characteristics,
/// and the one field that would read a computed power is refused at build time
/// ([`Violation::PowerInAttachmentCount`](crate::Violation::PowerInAttachmentCount)).
///
/// The count is relative to the **attachment's** controller, which is who "you control"
/// means on the card that printed the grant — not the host's controller, who may be an
/// opponent that stole the creature.
pub(super) fn attachment_pt_effect(
    state: &GameState,
    attachment: &Permanent,
    db: &CardDatabase,
) -> Option<StaticEffect> {
    let host = attachment.attached_to?;
    let grant = attachment.printed.face(db)?.attachment()?;
    let scale = match &grant.count_of {
        None => 1,
        Some(wanted) => i32::try_from(crate::condition::count_permanents(
            state,
            wanted,
            controller_of(state, attachment),
            db,
        ))
        .unwrap_or(i32::MAX),
    };
    Some(StaticEffect {
        source: attachment.id.0,
        affects: EffectAffects::SpecificPermanent(host),
        modification: Modification::PowerToughness {
            power: grant.power.saturating_mul(scale),
            toughness: grant.toughness.saturating_mul(scale),
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
