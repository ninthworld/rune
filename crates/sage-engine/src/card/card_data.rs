//! Functional definition of a card.

use serde::Deserialize;

use super::attachment::{Attachment, AttachmentKind};
use super::face::{BackFace, Face};
use super::keyword::Keyword;
use super::restriction::CombatRestriction;
use crate::ability::{Ability, Effect, TargetSpec};
use crate::card_type::{CardType, Supertype};
use crate::id::FunctionalId;
use crate::mana::Color;
use crate::token::PrintedFace;

/// One functional definition: the static, printing-independent rules object for a
/// card (ADR 0008 §2).
///
/// This is the immutable data the engine reasons about. It holds no zone, no
/// battlefield identity, and no per-game state — those live on
/// [`crate::GameState`]. Current characteristics (after continuous effects) are
/// computed by the layer system, never stored here.
///
/// **It is also the card's front face** (CR 712.2). A card has an ordered list of faces
/// — [`Self::faces`] — and for almost every card that list has one entry, which is this
/// object. A transforming double-faced card authors its second face under
/// [`Self::back_face`], and the identity above stays the *card's*: one
/// [`FunctionalId`], one printing, one row in the compatibility report, exactly as a
/// real set prints one card.
///
/// `deny_unknown_fields` is what keeps the schema *functional*: an upstream
/// presentation asset — `flavor_text`, `image_uris`, `artist`, a frame or watermark
/// — is a parse error rather than a silently ignored field, so no such data can
/// enter the catalog by accident (ADR 0008 §2, `docs/brief.md` Legal
/// Considerations). It is also why this type, not a wrapper, is the direct
/// deserialization target: serde does not enforce `deny_unknown_fields` through a
/// `flatten`ed field.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardData {
    /// The schema version this definition is authored against; must be
    /// [`SCHEMA_VERSION`](super::super::SCHEMA_VERSION) (ADR 0008 §2).
    pub schema_version: u32,
    /// This definition's authored, stable identity (ADR 0008 §3) — what printings
    /// and decklists reference, and what survives a rebuild, unlike the [`CardId`](crate::id::CardId)
    /// it is interned to.
    pub functional_id: FunctionalId,
    /// The card's name (e.g. `"Thornback Boar"`).
    pub name: String,
    /// Printed supertypes (e.g. `Basic`, `Legendary`); empty for most cards. Part
    /// of the structured type line the engine reasons about — the display string
    /// is rendered by [`CardData::type_line`], never parsed back.
    #[serde(default)]
    pub supertypes: Vec<Supertype>,
    /// Printed card types (e.g. `Creature`, `Land`). Every card has at least one.
    pub types: Vec<CardType>,
    /// Printed subtypes (e.g. `"Elf"`, `"Scout"`, `"Forest"`); empty for many
    /// cards. Open-ended, so kept as strings rather than an enum.
    #[serde(default)]
    pub subtypes: Vec<String>,
    /// The mana cost in curly-brace notation (e.g. `"{2}{G}"`); empty for cards
    /// with no mana cost, such as basic lands.
    pub mana_cost: String,
    /// The card's colors (CR 105.2); empty for a colorless card.
    ///
    /// Authored explicitly rather than re-derived by parsing [`Self::mana_cost`]'s
    /// pips (ADR 0008 §2) — the same "structured, never parsed back" discipline
    /// [`CardData::type_line`] uses. A colorless-cost-but-colored card is therefore
    /// representable without the cost string having to imply it.
    #[serde(default)]
    pub colors: Vec<Color>,
    /// Printed power, for creatures; `None` for non-creatures.
    #[serde(default)]
    pub power: Option<i32>,
    /// Printed toughness, for creatures; `None` for non-creatures.
    #[serde(default)]
    pub toughness: Option<i32>,
    /// Printed **starting loyalty**, for planeswalkers; `None` for every other card
    /// (CR 306.5b).
    ///
    /// The planeswalker counterpart of [`Self::power`]/[`Self::toughness`], and
    /// validated the same way: a planeswalker must carry one and nothing else may
    /// ([`Violation::LoyaltyMismatch`](crate::Violation)). It is the number of loyalty
    /// counters the permanent *enters with* — applied at the battlefield-entry seam as
    /// a self-replacement (CR 614.1c, applied by the replacement layer as it enters), so
    /// a planeswalker is never on the battlefield at zero loyalty and immediately dead
    /// to CR 704.5i. Current loyalty is the counter count, never this field.
    #[serde(default)]
    pub loyalty: Option<u32>,
    /// An **additional cost** to cast this card (CR 601.2b) — `As an additional cost
    /// to cast this spell, discard a card.` `None` for the overwhelming majority of
    /// cards, which cost only mana.
    ///
    /// Kept apart from [`Self::spell_effects`] on purpose: a cost is paid *while the
    /// spell is cast*, so it gates whether the card may be cast at all
    /// ([`GameState::additional_cost_is_payable`](crate::GameState)), while an effect
    /// happens on resolution and cannot gate anything. Authoring one as the other is
    /// the difference between a spell you cannot cast with an empty hand and a spell
    /// you can.
    #[serde(default)]
    pub additional_cost: Option<super::AdditionalCost>,
    /// The card's abilities as declarative data. Empty for vanilla cards. Cards
    /// whose behavior the data IR cannot express instead register abilities in
    /// [`crate::scripted`]; use [`crate::card::abilities_of`] to read both sources together.
    #[serde(default)]
    pub abilities: Vec<Ability>,
    /// The effects this card's **spell ability** produces on resolution — the
    /// instant/sorcery analogue of an ability's effects (CR 608.2c). Empty for a
    /// vanilla card and for a permanent spell whose only "effect" is entering the
    /// battlefield. A targeting spell effect declares its [`TargetSpec`]
    /// here exactly as an ability's effect does, so a spell chooses targets as it
    /// is cast (CR 601.2c); read them with [`crate::card::spell_effects_of`].
    #[serde(default)]
    pub spell_effects: Vec<Effect>,
    /// The **modes** of a modal spell (CR 700.2) — the bulleted list under `Choose
    /// one —`. Empty for every card that is not modal, which is nearly all of them.
    ///
    /// A modal card carries its effects here instead of in [`Self::spell_effects`], and
    /// the catalog validator enforces that either-or in both directions: a card with
    /// modes and loose spell effects would be a spell that does something the player
    /// never chose. It also holds the count between two and
    /// [`MAX_MODES`](crate::MAX_MODES), because a mode is a numbered control in a band
    /// of fixed height and a fifth one is a card the catalog refuses rather than a
    /// layout to degrade at render time (`docs/client-design.md` §6.7).
    ///
    /// **Which mode was chosen decides which target slots exist**, so the choice is made
    /// first, at announcement (CR 601.2b), and rides on the action and then on the stack
    /// object. Read one mode's effects with [`Self::spell_effects_for_mode`].
    #[serde(default)]
    pub modes: Vec<super::SpellMode>,
    /// What is true of this card **as a spell on the stack** that no effect of its own
    /// produces — `this spell can't be countered`, `the damage can't be prevented`
    /// (CR 701.5a, CR 615.1). Empty for every other card.
    ///
    /// Each entry may name the announced X it needs (CR 601.2b), which is what lets one
    /// card be an ordinary burn spell for a small X and an uncounterable one for a
    /// large one. Read through [`SpellTrait::applies`](super::SpellTrait::applies)
    /// against the value the stack object recorded, never against a value re-derived
    /// from the cost.
    #[serde(default)]
    pub spell_traits: Vec<super::SpellTrait>,
    /// The attachment ability of an **Aura** (CR 303.4) or an **Equipment**
    /// (CR 301.5): what it may be attached to, and what it grants its host while it is.
    /// `None` for every card that attaches to nothing, which is nearly all of them.
    ///
    /// One block for both kinds because the grant is one thing — see [`Attachment`].
    /// The kind decides how it *arrives*: an Aura is castable only with a legal enchant
    /// target (CR 303.4c/601.2c) and enters attached to it (CR 303.4d), while an
    /// Equipment is cast like any other artifact and moved onto a creature by the equip
    /// ability derived from this block ([`equip_ability`](super::equip_ability),
    /// CR 702.6b).
    #[serde(default)]
    pub attachment: Option<Attachment>,
    /// The card's printed keyword abilities (CR 702), e.g. flying or haste. Empty
    /// for a card with none. Read with [`CardData::has_keyword`] for the *printed*
    /// set; a permanent's *current* keywords fold these together with any granted by
    /// continuous effects (CR 613.1f, layer 6) via
    /// [`characteristics`](crate::characteristics::characteristics), which the combat
    /// and summoning-sickness code consults.
    #[serde(default)]
    pub keywords: Vec<Keyword>,
    /// The card's printed **combat restrictions** (CR 506.3, CR 509.1b) — "can't be
    /// blocked by black creatures", "can't be blocked by more than one creature". Empty
    /// for a card with none, which is nearly all of them.
    ///
    /// Kept apart from [`Self::keywords`] because these are not keyword abilities: no
    /// card prints them as a keyword and some carry a parameter. Like keywords they are
    /// only the *printed* seed; a permanent's current restrictions fold these together
    /// with any granted continuously (CR 613.1f, layer 6) via
    /// [`characteristics`](crate::characteristics::characteristics), which is what the
    /// combat-declaration gates read.
    #[serde(default)]
    pub restrictions: Vec<CombatRestriction>,
    /// The card's **back face**, for a transforming double-faced card (CR 712.2);
    /// `None` for every single-faced card, which is nearly all of them.
    ///
    /// Its presence is the whole of what makes a card two-faced: [`Self::faces`] lists
    /// two positions instead of one, a permanent of this card may be turned over
    /// ([`Effect::TransformSelf`](crate::Effect)), and the projection carries the other
    /// side so a client can show it. Absent, nothing anywhere behaves differently — a
    /// single-faced card is not a special case of a two-faced one, it is the same code
    /// with [`Face::Front`] the only position there is.
    ///
    /// Boxed because it is `None` on 99% of the catalog and [`CardData`] is held by
    /// value in the interned card list; a rare face should not widen every card.
    #[serde(default)]
    pub back_face: Option<Box<BackFace>>,
    /// Whether this card's behavior is (also) defined in code rather than data
    /// (ADR 0008 §2; the escape hatch of ADR 0003).
    ///
    /// `true` means [`crate::scripted`] carries an arm for this definition's
    /// interned [`CardId`](crate::id::CardId). Authored explicitly so the two tiers are declared, not
    /// inferred: today the flag is `false` on every bundled card, and no card's
    /// behavior lives in code.
    #[serde(default)]
    pub scripted: bool,
}

impl CardData {
    /// Render the printed type line for display, e.g. `"Basic Land — Forest"` or
    /// `"Creature — Elf Scout"`. Supertypes and types are joined with spaces;
    /// subtypes, if any, follow an em dash. This is the single source for the
    /// display string — it is never parsed back into types.
    #[must_use]
    pub fn type_line(&self) -> String {
        crate::card_type::render_type_line(&self.supertypes, &self.types, &self.subtypes)
    }

    /// This card's faces in printed order (CR 712.2): `[Front]` for a single-faced
    /// card, `[Front, Back]` for a transforming double-faced one.
    ///
    /// The ordered list the rest of the engine indexes into. It is derived from
    /// [`Self::back_face`] rather than stored, so "how many faces has this card" and
    /// "is there a back face to turn over" can never disagree.
    #[must_use]
    pub fn faces(&self) -> Vec<Face> {
        match self.back_face {
            Some(_) => vec![Face::Front, Face::Back],
            None => vec![Face::Front],
        }
    }

    /// Whether this card has a **back face** (CR 712.2) — whether it can be
    /// transformed at all.
    #[must_use]
    pub fn has_back_face(&self) -> bool {
        self.back_face.is_some()
    }

    /// The characteristics of one of this card's faces, or `None` when the card has no
    /// such face — which is [`Face::Back`] on every single-faced card.
    ///
    /// The single reader: a permanent's face
    /// ([`Printed::face`](crate::Printed::face)), the projection of a card in a zone,
    /// and the rules-text formatter all go through here, so no path can read the front
    /// face of a permanent that has turned over.
    #[must_use]
    pub fn face(&self, face: Face) -> Option<PrintedFace<'_>> {
        match face {
            Face::Front => Some(PrintedFace::Card(self)),
            Face::Back => self
                .back_face
                .as_deref()
                .map(|face| PrintedFace::CardBack { card: self, face }),
        }
    }

    /// The abilities printed on one of this card's faces (CR 712.4b) — the data tier
    /// only.
    ///
    /// The face-aware half of [`abilities_of`](super::abilities_of), which unions this
    /// with the derived equip ability and the code tier. Empty for a face the card has
    /// not got.
    #[must_use]
    pub fn face_abilities(&self, face: Face) -> &[Ability] {
        match face {
            Face::Front => &self.abilities,
            Face::Back => self.back_face.as_deref().map_or(&[][..], |b| &b.abilities),
        }
    }

    /// Whether the card has printed card type `card_type`.
    #[must_use]
    pub fn has_type(&self, card_type: CardType) -> bool {
        self.types.contains(&card_type)
    }

    /// Whether this is a permanent card — one that enters the battlefield when
    /// it resolves. True when any printed [`CardType`] is a permanent type
    /// (land, creature, artifact, enchantment, planeswalker, or battle); false
    /// for an instant/sorcery-only card, which resolves to a graveyard instead
    /// (CR 608.3). Keyed off the structured types, never a parsed string, and
    /// matched exhaustively so a new [`CardType`] must be classified here.
    #[must_use]
    pub fn is_permanent(&self) -> bool {
        self.types.iter().any(|t| match t {
            CardType::Land
            | CardType::Creature
            | CardType::Artifact
            | CardType::Enchantment
            | CardType::Planeswalker
            | CardType::Battle => true,
            CardType::Instant | CardType::Sorcery => false,
        })
    }

    /// Whether the card has printed subtype `subtype` (case-sensitive, as printed).
    #[must_use]
    pub fn has_subtype(&self, subtype: &str) -> bool {
        self.subtypes.iter().any(|s| s == subtype)
    }

    /// The ordered [`TargetSpec`]s a player chooses a target for when **casting**
    /// this card as a spell (CR 601.2c), in slot order.
    ///
    /// An Aura contributes its enchant restriction (CR 303.4a) as the first slot —
    /// the object it will be attached to — and every card contributes the target
    /// specs of its spell-ability effects ([`Self::spell_effects`]). The two are
    /// disjoint in practice (an Aura has no spell effects), but both are honored so
    /// a single accessor drives casting legality ([`crate::valid_actions`]), the
    /// per-slot candidate enumeration, and the on-resolution fizzle re-check
    /// (CR 608.2b). Empty for a spell that chooses no targets.
    #[must_use]
    pub fn cast_target_specs(&self, mode: Option<u8>) -> Vec<TargetSpec> {
        self.cast_target_groups(mode)
            .into_iter()
            .map(|group| group.spec)
            .collect()
    }

    /// Whether this card chooses a **mode** as it is announced (CR 700.2).
    #[must_use]
    pub fn is_modal(&self) -> bool {
        !self.modes.is_empty()
    }

    /// How many `{X}` symbols this card's mana cost carries (CR 107.3) — zero for every
    /// fixed cost, and the number an announced value is multiplied by for the rest.
    #[must_use]
    pub fn x_pips(&self) -> u8 {
        crate::mana::x_pip_count(&self.mana_cost)
    }

    /// Whether casting this card **announces a value for X** (CR 601.2b) — its printed
    /// cost contains at least one `{X}`.
    #[must_use]
    pub fn announces_x(&self) -> bool {
        self.x_pips() > 0
    }

    /// The effects this card's spell ability produces for the chosen `mode`.
    ///
    /// A non-modal card ignores the argument and answers [`Self::spell_effects`]; a
    /// modal one answers the named mode's effects, and answers **nothing** for a mode
    /// that was not chosen or does not exist. That last part is the ordering rule made
    /// structural rather than remembered: with no mode there are no effects, so there
    /// are no target slots either, and an announcement that skipped the choice cannot
    /// accidentally look like a complete one — [`crate::apply_action`] refuses it
    /// outright.
    #[must_use]
    pub fn spell_effects_for_mode(&self, mode: Option<u8>) -> Vec<Effect> {
        if !self.is_modal() {
            return self.spell_effects.clone();
        }
        mode.and_then(|index| self.modes.get(usize::from(index)))
            .map(|chosen| chosen.effects.clone())
            .unwrap_or_default()
    }

    /// The ordered [`TargetGroup`]s a player chooses targets for when **casting** this
    /// card as a spell (CR 601.2c), in slot order — the arity-aware form of
    /// [`Self::cast_target_specs`], and the one every gate reads.
    ///
    /// An Aura's enchant restriction is a single required target (CR 303.4a), so it
    /// contributes a one-target group. An **Equipment** contributes none: it is cast like
    /// any other artifact and enters attached to nothing (CR 301.5c), choosing its host
    /// later, on its equip activation.
    ///
    /// `mode` is the mode announced for a **modal** card (CR 700.2) and is what makes
    /// this answerable at all for one: the slots are the chosen mode's, so with no mode
    /// there are none. Ignored by every non-modal card.
    #[must_use]
    pub fn cast_target_groups(&self, mode: Option<u8>) -> Vec<crate::ability::TargetGroup> {
        let mut groups: Vec<crate::ability::TargetGroup> = self
            .attachment
            .as_ref()
            .filter(|a| a.kind == AttachmentKind::Aura)
            .map(|a| crate::ability::TargetGroup {
                spec: a.attach_to,
                min: 1,
                max: 1,
            })
            .into_iter()
            .collect();
        groups.extend(
            self.spell_effects_for_mode(mode)
                .iter()
                .flat_map(Effect::target_groups),
        );
        groups
    }

    /// The card's **mana value** (CR 202.3): the total amount of mana in its cost,
    /// counting each generic point and each colored or colorless pip as one.
    ///
    /// Derived from [`Self::mana_cost`] on demand through the one parser every payment
    /// uses, so "mana value 2 or less" and "can you pay {1}{R}" can never disagree about
    /// what a cost string means. A card with no mana cost — a land, a token's face — has
    /// mana value 0, which is what CR 202.3a says.
    #[must_use]
    pub fn mana_value(&self) -> u32 {
        let cost = crate::mana::parse_mana_cost(&self.mana_cost);
        u32::from(cost.generic) + u32::from(cost.colored_total())
    }

    /// Whether the card has printed keyword ability `keyword` (CR 702). Reads only
    /// the printed [`CardData::keywords`]. A permanent's *current* keywords also
    /// include those granted by continuous effects at CR 613 layer 6; read those
    /// through [`characteristics`](crate::characteristics::characteristics).
    #[must_use]
    pub fn has_keyword(&self, keyword: Keyword) -> bool {
        self.keywords.contains(&keyword)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::card_type::Supertype;

    #[test]
    fn known_id_resolves_to_expected_characteristics() {
        let db = crate::card::CardDatabase::bundled().unwrap();
        let ogre = crate::card::tests::card_named(&db, "onakke_ogre");
        assert_eq!(ogre.name, "Onakke Ogre");
        assert_eq!(ogre.types, vec![CardType::Creature]);
        assert_eq!(
            ogre.subtypes,
            vec!["Ogre".to_string(), "Warrior".to_string()]
        );
        assert_eq!(ogre.type_line(), "Creature — Ogre Warrior");
        assert_eq!(ogre.mana_cost, "{2}{R}");
        assert_eq!(ogre.power, Some(4));
        assert_eq!(ogre.toughness, Some(2));
    }

    #[test]
    fn basic_land_has_no_power_or_toughness() {
        let db = crate::card::CardDatabase::bundled().unwrap();
        let forest = crate::card::tests::card_named(&db, "forest");
        assert_eq!(forest.name, "Forest");
        assert_eq!(forest.supertypes, vec![Supertype::Basic]);
        assert_eq!(forest.types, vec![CardType::Land]);
        assert_eq!(forest.type_line(), "Basic Land — Forest");
        assert_eq!(forest.mana_cost, "");
        assert_eq!(forest.power, None);
        assert_eq!(forest.toughness, None);
    }

    #[test]
    fn type_line_renders_supertypes_types_and_subtypes() {
        let db = crate::card::CardDatabase::bundled().unwrap();
        // Multiple subtypes are space-joined after the em dash.
        assert_eq!(
            crate::card::tests::card_named(&db, "tolarian_scholar").type_line(),
            "Creature — Human Wizard"
        );
        // A supertype precedes the card type; the land subtype follows the dash.
        assert_eq!(
            crate::card::tests::card_named(&db, "forest").type_line(),
            "Basic Land — Forest"
        );
    }

    #[test]
    fn has_type_and_has_subtype_query_structured_types() {
        let db = crate::card::CardDatabase::bundled().unwrap();
        let elves = crate::card::tests::card_named(&db, "llanowar_elves");
        assert!(elves.has_type(CardType::Creature));
        assert!(!elves.has_type(CardType::Land));
        assert!(elves.has_subtype("Elf"));
        assert!(!elves.has_subtype("Goblin"));
    }

    #[test]
    fn is_permanent_splits_permanent_types_from_instants_and_sorceries() {
        let db = crate::card::CardDatabase::bundled().unwrap();
        // Creature and land are permanent cards.
        assert!(crate::card::tests::card_named(&db, "onakke_ogre").is_permanent());
        assert!(crate::card::tests::card_named(&db, "forest").is_permanent());
        // An instant-only card is not.
        let json = r#"[{"schema_version":1,"functional_id":"test_bolt","name":"Test Bolt","types":["instant"],"mana_cost":"{R}"}]"#;
        let bolt = crate::card::CardDatabase::from_json(json).unwrap();
        assert!(!crate::card::tests::card_named(&bolt, "test_bolt").is_permanent());
    }

    #[test]
    fn vanilla_cards_deserialize_with_no_abilities() {
        let db = crate::card::CardDatabase::bundled().unwrap();
        assert!(crate::card::tests::card_named(&db, "onakke_ogre")
            .abilities
            .is_empty());
    }

    #[test]
    fn colors_are_authored_not_derived_from_the_cost() {
        // ADR 0008 §2: colors are an explicit field. For the current fixtures they
        // agree with the pips of their cost (this test is that authoring check), but
        // nothing derives them at runtime — so a card whose colors do not follow from
        // its cost is representable.
        let db = crate::card::CardDatabase::bundled().unwrap();

        for id in crate::card::tests::every_id() {
            let card = db.card(id).unwrap();
            let cost = crate::mana::parse_mana_cost(&card.mana_cost);
            let from_pips: Vec<Color> = [
                (cost.white, Color::White),
                (cost.blue, Color::Blue),
                (cost.black, Color::Black),
                (cost.red, Color::Red),
                (cost.green, Color::Green),
            ]
            .into_iter()
            .filter(|(pips, _)| *pips > 0)
            .map(|(_, color)| color)
            .collect();
            assert_eq!(
                card.colors, from_pips,
                "{}'s authored colors disagree with its cost",
                card.name
            );
        }

        // A colorless cost with an authored color — the case pip-parsing could not
        // express — round-trips.
        let json = r#"[{"schema_version":1,"functional_id":"void_thing","name":"Void Thing",
                        "types":["creature"],"mana_cost":"{2}","colors":["black"],
                        "power":2,"toughness":2}]"#;
        let db = crate::card::CardDatabase::from_json(json).unwrap();
        assert_eq!(
            crate::card::tests::card_named(&db, "void_thing").colors,
            vec![Color::Black]
        );
    }
}
