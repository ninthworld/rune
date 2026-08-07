//! Public helper functions for card data.

use super::attachment::AttachmentKind;
use super::database::CardDatabase;
use super::face::Face;
use crate::ability::{Ability, Cost, Effect};
use crate::id::CardId;
use crate::scripted::scripted_abilities;
use crate::state::Permanent;

/// All abilities of a card: its data-driven [`super::CardData::abilities`], the **equip
/// ability** an Equipment's attachment block implies ([`equip_ability`]), plus any
/// code-defined ones from [`crate::scripted`].
///
/// Returns an empty list if the id is unknown and has no scripted abilities. This
/// is the single accessor the pipeline uses so all three authoring tiers are always
/// considered together — which is what makes the equip ability an ordinary activation
/// everywhere downstream: it is offered, targeted, paid for, put on the stack, resolved,
/// and labelled by exactly the code an authored `{2}: …` goes through.
#[must_use]
pub fn abilities_of(db: &CardDatabase, card: CardId) -> Vec<crate::ability::Ability> {
    abilities_of_face(db, card, Face::Front)
}

/// All abilities of **one face** of a card (CR 712.4b) — the face-aware form of
/// [`abilities_of`], and the one a battlefield object reads.
///
/// A permanent has only the abilities printed on the face that is up: a transforming
/// creature that has turned over loses its front face's activation and gains its back
/// face's loyalty abilities, because those are two different faces' printed text and
/// nothing about the object carries the other one.
///
/// The two tiers that are *not* face-scoped stay unioned in either case, and both are
/// keyed to the card rather than to a face: the equip ability derived from an
/// `attachment` block (which only a front face can author) and the code tier
/// [`crate::scripted`] keys on the card's authored identity.
#[must_use]
pub fn abilities_of_face(
    db: &CardDatabase,
    card: CardId,
    face: Face,
) -> Vec<crate::ability::Ability> {
    let Some(data) = db.card(card) else {
        // An unknown handle has no data tier, and the code tier is keyed on the authored
        // identity this handle would have resolved to — so there is nothing to union.
        return Vec::new();
    };
    let mut abilities = data.face_abilities(face).to_vec();
    abilities.extend(equip_ability(data));
    abilities.extend(scripted_abilities(&data.functional_id));
    abilities
}

/// The **equip ability** of an Equipment (CR 702.6a): `{cost}: Attach this to target
/// creature you control`, derived from the card's attachment block. `None` for every card
/// that is not an Equipment.
///
/// Derived rather than authored, for the reason an Aura's cast target slot is derived: an
/// authored ability could name a cost the card does not print, or attach something other
/// than itself, and neither is a thing a printed Equipment can say. What it *may* say —
/// the cost and the class of legal host — is the two fields the block already carries.
///
/// The one shape the whole feature hangs off, so it is public: the rules-text formatter
/// composes the printed equip line from the same value the engine activates, and the two
/// therefore cannot describe different costs (ADR 0008 §7).
#[must_use]
pub fn equip_ability(data: &super::CardData) -> Option<Ability> {
    let attachment = data.attachment.as_ref()?;
    if attachment.kind != AttachmentKind::Equipment {
        return None;
    }
    // The catalog validator guarantees an Equipment authors an equip cost, so this is
    // belt-and-braces: an Equipment with none would advertise a free equip, and no
    // ability at all is the safer of the two wrong answers.
    let mana = attachment.equip.clone()?;
    Some(Ability::Activated {
        cost: vec![Cost::Mana { mana }],
        effects: vec![Effect::Attach {
            target: attachment.attach_to,
        }],
        // Equip has no per-turn allowance (CR 702.6b), so the printed-text field stays
        // at its default here, exactly as the timing note below says of its own.
        once_each_turn: false,
        // CR 702.6b's sorcery timing is derived from the ability *being* an equip
        // ability, not authored — so the printed-text field stays at its default here.
        timing: crate::ability::ActivationTiming::AnyTime,
    })
}

/// All abilities of a **permanent**, whether it is a card or a token — after CR 613
/// **layer 6**.
///
/// The permanent-side counterpart of [`abilities_of`], and the accessor every path
/// that reads a battlefield object's abilities goes through. A card permanent defers
/// to [`abilities_of`], so its data tier and its code tier are still unioned; a token
/// has only what the effect that created it wrote down, because the code tier is keyed
/// on an authored `functional_id` and a token has none (CR 111).
///
/// **It takes the state because layer 6 both adds and subtracts.** A permanent under a
/// loses-all-abilities effect has none, and one carrying an Aura that hands it an ability
/// has that too — both settled in timestamp order by
/// [`current_abilities`](crate::characteristics::current_abilities). *Every* collector
/// has to agree about the answer or a removed trigger still fires, a silenced permanent
/// still offers its activation, or a granted mana ability is never offered at all. Making
/// the one accessor answer it is what makes those impossible to get wrong individually —
/// there is no printed-abilities reader left to reach for by mistake.
///
/// The fold reads no computed characteristic of `perm`, so this is safe to call from
/// inside the characteristics computation itself — but it *does* read the printed static
/// abilities of every other permanent, which is why the one thing it may not be used for
/// is deciding whether one of those sources still has its abilities. That question has
/// its own accessor, [`stored_abilities_of_permanent`].
#[must_use]
pub fn abilities_of_permanent(
    state: &crate::GameState,
    db: &CardDatabase,
    perm: &Permanent,
) -> Vec<crate::ability::Ability> {
    crate::characteristics::current_abilities(
        state,
        perm,
        printed_abilities_of(state, db, perm),
        db,
    )
}

/// The abilities `perm` has after folding in only what is **stored** — until-end-of-turn
/// effects and the attachments on it — and *not* what a printed static ability elsewhere
/// grants or takes away.
///
/// A deliberately smaller answer than [`abilities_of_permanent`], and it exists for
/// exactly one caller: the gate that asks whether a *source* still has the static ability
/// it is about to contribute (CR 613 layer 6 applies to a lord before the lord pumps
/// anything). Asking the full accessor there would be asking a permanent's abilities from
/// inside the computation of a permanent's abilities, which does not terminate.
///
/// What that costs is nameable and small: a permanent silenced by an Aura or by a spell
/// stops contributing its static ability, and one silenced by *another printed static
/// ability* does not. The engine models CR 613.7 timestamps but not the CR 613.8
/// dependency rules, and this is where that shows.
#[must_use]
pub(crate) fn stored_abilities_of_permanent(
    state: &crate::GameState,
    db: &CardDatabase,
    perm: &Permanent,
) -> Vec<crate::ability::Ability> {
    crate::characteristics::stored_abilities(state, perm, printed_abilities_of(state, db, perm), db)
}

/// The abilities printed on `perm`'s face, before any layer **below 1** applies: a card's
/// two authoring tiers unioned by [`abilities_of`], or the list the effect that created a
/// token wrote down (ADR 0015).
///
/// CR 613 layer 1 comes first: a permanent's rules text is a copiable value (CR 707.2), so
/// a copy has the abilities it copied and none of its own — Mirror Image copying a
/// Skyscanner really does draw a card as it enters. The seed is the layer-1 answer rather
/// than the stored face, and it is a read of stored fields only, so it cannot recurse into
/// the computation this feeds.
fn printed_abilities_of(
    state: &crate::GameState,
    db: &CardDatabase,
    perm: &Permanent,
) -> Vec<crate::ability::Ability> {
    match crate::copy::copiable_printed(state, perm) {
        // CR 712.4b: only the face that is up is read, so a permanent that has
        // transformed offers exactly its back face's abilities and none of its front's.
        crate::token::Printed::Card { card, face } => abilities_of_face(db, *card, *face),
        crate::token::Printed::Token(token) => token.abilities.clone(),
    }
}

/// What a card declares about **copying something as it enters** (CR 707.5 / CR 614.12) —
/// its [`Ability::EntersAsCopy`], or `None` for every card that copies nothing.
///
/// The copy counterpart of [`chooses_color_on_entry`], and read at the same seam for the
/// same reason: at the moment the question is asked there is no permanent to read the
/// ability off, because the whole point is that none exists until it is answered. It is
/// face-aware — a back face may print its own — and honours both authoring tiers through
/// [`abilities_of_face`].
///
/// A **token** is never asked: a token's abilities are whatever the creating effect wrote
/// down (ADR 0015), and nothing in the effect IR creates a token as a copy.
#[must_use]
pub(crate) fn copies_on_entry(
    db: &CardDatabase,
    card: CardId,
    face: Face,
) -> Option<EntryCopyDeclaration> {
    abilities_of_face(db, card, face)
        .into_iter()
        .find_map(|ability| match ability {
            Ability::EntersAsCopy {
                of,
                subject,
                optional,
            } => Some(EntryCopyDeclaration {
                of,
                subject,
                optional,
            }),
            _ => None,
        })
}

/// The three facts [`copies_on_entry`] answers with — the printed declaration, unpacked
/// so the entry seam does not have to match an [`Ability`] it has no other business with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EntryCopyDeclaration {
    /// Which permanents may be named.
    pub of: crate::copy::CopyClass,
    /// What becomes the copy.
    pub subject: crate::copy::CopySubject,
    /// Whether naming nothing is a legal answer.
    pub optional: bool,
}

/// The effects a spell of printed card `card` produces on resolution
/// ([`super::CardData::spell_effects`]), or an empty list for an unknown id or a card
/// with no spell ability.
///
/// The spell-side counterpart of [`abilities_of`]: the resolve path reads these
/// to apply a spell's effects (pairing targeting effects with the targets chosen
/// at cast), and [`crate::valid_actions`] reads them to enumerate a targeted
/// cast's requirement slots — the same effect IR, whether it rides an ability or
/// a spell.
///
/// `mode` is the mode chosen at announcement for a **modal** card (CR 700.2); it names
/// which of the printed bullets these effects are. A modal card given no mode has no
/// effects at all, which is what makes an unchosen mode unresolvable rather than
/// silently all of them.
#[must_use]
pub(crate) fn spell_effects_of(db: &CardDatabase, card: CardId, mode: Option<u8>) -> Vec<Effect> {
    db.card(card)
        .map(|c| c.spell_effects_for_mode(mode))
        .unwrap_or_default()
}

/// Whether the card `card` names a **colour as it enters** the battlefield
/// (CR 614.12) — whether it declares [`Ability::EntersChoosingColor`].
///
/// Read at the battlefield-entry seam ([`crate::GameState::put_card_onto_battlefield`])
/// to decide whether the entry can happen now or has to wait for an answer, and read
/// there off the *card* rather than off a built [`Permanent`] because at that moment
/// there is no permanent: the whole point is that none exists until the question is
/// answered.
///
/// Both authoring tiers are honoured via [`abilities_of`], and a **token** is never
/// asked: `create_token` does not consult this, because a token's abilities are whatever
/// the creating effect wrote down (ADR 0015) and no effect can write down a question that
/// a resolving effect has no way to stop and ask.
#[must_use]
pub(crate) fn chooses_color_on_entry(db: &CardDatabase, card: CardId) -> bool {
    abilities_of(db, card)
        .iter()
        .any(|ability| matches!(ability, Ability::EntersChoosingColor))
}

/// Whether the spell `card` belongs to the class `observes`, for an ability whose source
/// named `chosen` as it entered (CR 614.12).
///
/// **One answer, in one place, because two abilities ask it.** A cast trigger asks it of
/// a spell that has just gone on the stack ([`crate::collect_triggers`]) and a cost
/// modifier asks it of a card about to be cast ([`crate::total_cast_cost`]) — and both
/// are asking about the *card*, not about a stack object, which is why one predicate can
/// serve both. That equality matters: a card whose class the offer and the trigger
/// disagreed about would be reduced and then not noticed, or noticed and then charged
/// full price.
///
/// Every characteristic here is read off the printed face, which is the only face a card
/// in a hand, a graveyard, or on the stack has in this engine — CR 613's layers are
/// applied to permanents, and no effect in the catalog changes a spell's type, colour, or
/// power on the stack.
///
/// `chosen` is threaded in rather than looked up because only one class reads it, and a
/// source that named no colour — an emblem, a token, a card that never declared the
/// choice — passes `None` and matches nothing of that class. That is the correct answer
/// rather than a defensive one: "a spell of the chosen color" is unsatisfiable until a
/// colour has been chosen.
#[must_use]
pub(crate) fn spell_matches_class(
    db: &CardDatabase,
    card: CardId,
    observes: crate::ability::ObservedSpell,
    chosen: Option<crate::mana::Color>,
) -> bool {
    use crate::ability::ObservedSpell;
    use crate::card_type::CardType;

    let Some(data) = db.card(card) else {
        return false;
    };
    match observes {
        ObservedSpell::Enchantment => data.has_type(CardType::Enchantment),
        // CR 205.2b: an artifact creature is both, so it satisfies this class and the
        // creature one. Nothing here excludes a card for the other types it also has.
        ObservedSpell::Artifact => data.has_type(CardType::Artifact),
        ObservedSpell::InstantOrSorcery => {
            data.has_type(CardType::Instant) || data.has_type(CardType::Sorcery)
        }
        // A power bound that no printed power can satisfy excludes the card rather than
        // defaulting it to zero: "a creature spell with power 4 or greater" is a question
        // about a number the card has, and a card with none is not an answer.
        ObservedSpell::Creature {
            min_power,
            max_power,
        } => {
            data.has_type(CardType::Creature)
                && min_power.is_none_or(|min| data.power.is_some_and(|power| power >= min))
                && max_power.is_none_or(|max| data.power.is_some_and(|power| power <= max))
        }
        // CR 303.4: the attachment block is what makes a card an Aura here, so it is what
        // the class asks about.
        ObservedSpell::Aura => data
            .attachment
            .as_ref()
            .is_some_and(|attachment| attachment.kind == crate::card::AttachmentKind::Aura),
        // CR 105.2: a spell *is* each of its colours, so a gold spell satisfies a
        // watcher of any one of them and a colourless spell satisfies none.
        ObservedSpell::ChosenColor => chosen.is_some_and(|color| data.colors.contains(&color)),
    }
}

/// The class of card `card` names **as it enters** the battlefield (CR 614.12), or `None`
/// for a card that names none — whether it declares [`Ability::EntersNamingCard`], and
/// what it may name.
///
/// [`chooses_color_on_entry`]'s sibling, read at the same seam for the same reason, and
/// returning the class rather than a bare `bool` because the question the seam has to
/// pose needs it: the answer set is derived from the class
/// ([`named_card_candidates`](crate::named_card_candidates)), so "does it ask?" and "what
/// may it name?" are one lookup rather than two that could disagree.
#[must_use]
pub(crate) fn names_a_card_on_entry(
    db: &CardDatabase,
    card: CardId,
) -> Option<crate::choice::NamedCardClass> {
    abilities_of(db, card)
        .iter()
        .find_map(|ability| match ability {
            Ability::EntersNamingCard { class } => Some(*class),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::ability::{Ability, Effect, TriggerCondition};

    #[test]
    fn forest_has_one_activated_mana_ability() {
        let db = CardDatabase::bundled().unwrap();
        let forest = crate::card::tests::card_named(&db, "forest");
        assert_eq!(forest.abilities.len(), 1);
        assert!(crate::ability::is_mana_ability(&forest.abilities[0]));
    }

    #[test]
    fn skyscanner_has_an_etb_draw_trigger() {
        let db = CardDatabase::bundled().unwrap();
        let skyscanner = crate::card::tests::card_named(&db, "skyscanner");
        assert_eq!(
            skyscanner.abilities,
            vec![Ability::Triggered {
                event: TriggerCondition::SelfEntersBattlefield,
                effects: vec![Effect::DrawCard { count: 1 }],
            }]
        );
    }

    #[test]
    fn issue_155_etb_replacement_fixtures_carry_their_self_replacements() {
        // The bundled tapland authors an `enters_tapped` self-replacement (CR 614.1c)
        // alongside its two mana abilities. The `enters_with_counters` self-replacement
        // (CR 614.12) has no clean representative in the real M19 catalog, so it is
        // exercised through an inline definition — the sanctioned pattern for an IR
        // shape the shipped set does not use (ADR 0009).
        use crate::card_type::CardType;
        use crate::state::CounterKind;
        let db = CardDatabase::bundled().unwrap();

        let land = crate::card::tests::card_named(&db, "tranquil_expanse");
        assert_eq!(land.name, "Tranquil Expanse");
        assert_eq!(land.types, vec![CardType::Land]);
        assert_eq!(
            land.abilities
                .iter()
                .filter(|a| matches!(a, Ability::EntersTapped))
                .count(),
            1,
            "the tapland enters tapped (CR 614.1c)"
        );
        // Its two tap-for-mana abilities are still present and activatable.
        assert_eq!(
            land.abilities
                .iter()
                .filter(|a| crate::ability::is_mana_ability(a))
                .count(),
            2
        );

        let json = r#"[{"schema_version":1,"functional_id":"test_broodling","name":"Test Broodling",
            "types":["creature"],"subtypes":["Insect"],"mana_cost":"{1}{G}","colors":["green"],
            "power":0,"toughness":0,
            "abilities":[{"type":"enters_with_counters","counter":"plus_one_plus_one","count":2}]}]"#;
        let inline = CardDatabase::from_json(json).unwrap();
        let broodling = crate::card::tests::card_named(&inline, "test_broodling");
        assert_eq!(broodling.power, Some(0));
        assert_eq!(broodling.toughness, Some(0));
        assert_eq!(
            broodling.abilities,
            vec![Ability::EntersWithCounters {
                counter: CounterKind::PlusOnePlusOne,
                count: 2,
            }]
        );
    }

    #[test]
    fn abilities_of_unions_data_and_scripted_sources() {
        let db = CardDatabase::bundled().unwrap();
        // Forest's ability comes from data; no scripted card is registered, so
        // the accessor returns exactly the data-driven ability.
        let forest = crate::card::tests::id_of(&db, "forest");
        assert_eq!(
            abilities_of(&db, forest),
            db.card(forest).unwrap().abilities
        );
        // An unknown id with no scripted abilities yields nothing.
        assert!(abilities_of(&db, CardId(9999)).is_empty());
    }

    #[test]
    fn issue_149_effect_ir_wave_fixtures_carry_their_verbs() {
        use crate::ability::{DamageSubject, PlayerRef, TargetSpec};
        use crate::state::CounterKind;
        let db = CardDatabase::bundled().unwrap();

        // A burn instant: deal 2 to any target.
        let shock = crate::card::tests::card_named(&db, "shock");
        assert_eq!(shock.name, "Shock");
        assert_eq!(
            shock.spell_effects,
            vec![Effect::DealDamage {
                subject: DamageSubject::Target(TargetSpec::AnyTarget),
                amount: 2
            }]
        );
        // A burn instant restricted to a creature: deal 4 to target creature.
        assert_eq!(
            crate::card::tests::card_named(&db, "electrify").spell_effects,
            vec![Effect::DealDamage {
                subject: DamageSubject::Target(TargetSpec::AnyCreature),
                amount: 4
            }]
        );
        // A destroy instant.
        let murder = crate::card::tests::card_named(&db, "murder");
        assert_eq!(
            murder.spell_effects,
            vec![Effect::Destroy {
                target: TargetSpec::AnyCreature
            }]
        );
        // A two-effect spell: gain life, then draw.
        assert_eq!(
            crate::card::tests::card_named(&db, "revitalize").spell_effects,
            vec![
                Effect::GainLife {
                    player_ref: PlayerRef::Controller,
                    amount: 3
                },
                Effect::DrawCard { count: 1 },
            ]
        );

        // Effects the real M19 catalog does not use — a +1/+1 ETB counter, life loss,
        // and a -1/-1 counter — are exercised inline (ADR 0009).
        let json = r#"[
            {"schema_version":1,"functional_id":"test_sprite","name":"Test Sprite",
             "types":["creature"],"subtypes":["Faerie"],"mana_cost":"{1}{G}","colors":["green"],
             "power":1,"toughness":1,
             "abilities":[{"type":"triggered","event":"self_enters_battlefield",
               "effects":[{"kind":"put_counters","target":"any_creature","counter":"plus_one_plus_one","count":1}]}]},
            {"schema_version":1,"functional_id":"test_drain","name":"Test Drain",
             "types":["instant"],"mana_cost":"{B}","colors":["black"],
             "spell_effects":[{"kind":"lose_life","player_ref":"controller","amount":2}]},
            {"schema_version":1,"functional_id":"test_wither","name":"Test Wither",
             "types":["sorcery"],"mana_cost":"{B}","colors":["black"],
             "spell_effects":[{"kind":"put_counters","target":"any_creature","counter":"minus_one_minus_one","count":1}]}
        ]"#;
        let inline = CardDatabase::from_json(json).unwrap();
        assert_eq!(
            crate::card::tests::card_named(&inline, "test_sprite").abilities,
            vec![Ability::Triggered {
                event: TriggerCondition::SelfEntersBattlefield,
                effects: vec![Effect::PutCounters {
                    targets: crate::ability::TargetCount::Exactly(1),
                    target: TargetSpec::AnyCreature,
                    counter: CounterKind::PlusOnePlusOne,
                    count: 1,
                }],
            }]
        );
        assert_eq!(
            crate::card::tests::card_named(&inline, "test_drain").spell_effects,
            vec![Effect::LoseLife {
                player_ref: PlayerRef::Controller,
                amount: 2
            }]
        );
        assert_eq!(
            crate::card::tests::card_named(&inline, "test_wither").spell_effects,
            vec![Effect::PutCounters {
                targets: crate::ability::TargetCount::Exactly(1),
                target: TargetSpec::AnyCreature,
                counter: CounterKind::MinusOneMinusOne,
                count: 1,
            }]
        );
    }

    #[test]
    fn bundled_spells_carry_their_functions() {
        use crate::ability::{Cost, DamageSubject};
        use crate::card_type::CardType;
        use crate::mana::Color;

        let db = CardDatabase::bundled().unwrap();

        // Lightning Strike: a {1}{R} bolt dealing 3 to any target — distinct from
        // Shock's 2, so it is its own definition rather than a reprint of one identity.
        let strike = crate::card::tests::card_named(&db, "lightning_strike");
        assert_eq!(strike.types, vec![CardType::Instant]);
        assert_eq!(
            strike.spell_effects,
            vec![Effect::DealDamage {
                subject: DamageSubject::Target(crate::ability::TargetSpec::AnyTarget),
                amount: 3,
            }]
        );
        assert_ne!(
            strike.spell_effects,
            crate::card::tests::card_named(&db, "shock").spell_effects,
            "a byte-identical twin should be a reprint, not a second definition"
        );

        // Divination: a {2}{U} sorcery drawing two.
        let divination = crate::card::tests::card_named(&db, "divination");
        assert_eq!(divination.types, vec![CardType::Sorcery]);
        assert_eq!(
            divination.spell_effects,
            vec![Effect::DrawCard { count: 2 }]
        );

        // Viashino Pyromancer: a creature whose ETB trigger deals 2 to a target
        // player or planeswalker.
        let pyromancer = crate::card::tests::card_named(&db, "viashino_pyromancer");
        assert_eq!(
            pyromancer.abilities,
            vec![Ability::Triggered {
                event: TriggerCondition::SelfEntersBattlefield,
                effects: vec![Effect::DealDamage {
                    subject: DamageSubject::Target(
                        crate::ability::TargetSpec::AnyPlayerOrPlaneswalker
                    ),
                    amount: 2,
                }],
            }]
        );

        // A mana dork: {T}: Add {G} is a mana ability (CR 605.1a).
        let elves = crate::card::tests::card_named(&db, "llanowar_elves");
        assert_eq!(
            elves.abilities,
            vec![Ability::Activated {
                once_each_turn: false,
                cost: vec![Cost::Tap],
                effects: vec![Effect::AddMana {
                    color: Color::Green,
                    amount: 1,
                }],
                timing: crate::ability::ActivationTiming::AnyTime,
            }]
        );
        assert!(crate::ability::is_mana_ability(&elves.abilities[0]));

        // The colorless-mana verb ({T}: Add {C}) has no clean M19 representative,
        // so a mana rock is exercised inline (ADR 0009).
        let json = r#"[{"schema_version":1,"functional_id":"test_lodestone","name":"Test Lodestone",
            "types":["artifact"],"mana_cost":"{1}","colors":[],
            "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
              "effects":[{"kind":"add_colorless_mana","amount":1}]}]}]"#;
        let inline = CardDatabase::from_json(json).unwrap();
        let lodestone = crate::card::tests::card_named(&inline, "test_lodestone");
        assert_eq!(lodestone.types, vec![CardType::Artifact]);
        assert_eq!(
            lodestone.abilities,
            vec![Ability::Activated {
                once_each_turn: false,
                cost: vec![Cost::Tap],
                effects: vec![Effect::AddColorlessMana { amount: 1 }],
                timing: crate::ability::ActivationTiming::AnyTime,
            }]
        );
        assert!(crate::ability::is_mana_ability(&lodestone.abilities[0]));
    }
}
