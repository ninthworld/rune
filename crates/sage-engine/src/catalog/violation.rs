//! Every way a catalog definition can be malformed, and how each one reads.
//!
//! Split out of `catalog.rs` for size (issue #711). Like its parent this names no
//! `crate::` path: `build.rs` compiles it before the engine exists.

use super::*;

/// A catalog file that does not satisfy the authored schema (ADR 0008 §5).
///
/// Returned by [`validate_definition`] and [`check_printings`]. `build.rs` turns one
/// of these into a build failure; the loader turns it into a
/// [`CatalogError`](crate::CatalogError). Each variant names the offending card so the
/// message points at the file to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Violation {
    /// A definition file holds something other than a single JSON object — most
    /// likely the old monolithic array, which ADR 0008 §4 replaced with one file per
    /// card.
    NotAnObject,
    /// A required field is missing, or holds the wrong JSON type.
    MalformedField {
        /// The card the field belongs to, or the file stem if identity itself is the
        /// problem.
        functional_id: String,
        /// The field at fault.
        field: &'static str,
    },
    /// A definition declares a `schema_version` this engine does not understand.
    UnsupportedSchemaVersion {
        /// The definition that declared it.
        functional_id: String,
        /// The version it declared.
        found: u64,
    },
    /// A `functional_id` is not a well-formed slug (see [`is_well_formed_slug`]).
    MalformedFunctionalId {
        /// The ill-formed slug.
        slug: String,
    },
    /// A definition's `functional_id` does not match the file it is stored in. The
    /// file name *is* the identity (ADR 0008 §4), so the two may not disagree.
    FileNameMismatch {
        /// The identity the file declares.
        functional_id: String,
        /// The file it was found in, without its `.json` extension.
        file_stem: String,
    },
    /// A `Creature` carries no printed power/toughness, or a non-creature carries
    /// them (ADR 0008 §5).
    PowerToughnessMismatch {
        /// The definition at fault.
        functional_id: String,
        /// Whether the card is a creature — which is to say, which way it is wrong.
        creature: bool,
    },
    /// A `Planeswalker` carries no printed `loyalty`, or a non-planeswalker carries
    /// one (CR 306.5b) — the planeswalker counterpart of
    /// [`Self::PowerToughnessMismatch`].
    ///
    /// Wrong in both directions for the same reason that one is: a planeswalker with no
    /// starting loyalty would enter the battlefield with no loyalty counters and be put
    /// straight into its owner's graveyard by CR 704.5i, and a loyalty on anything else
    /// is a number nothing would ever read.
    LoyaltyMismatch {
        /// The definition at fault.
        functional_id: String,
        /// Whether the card is a planeswalker — which is to say, which way it is wrong.
        planeswalker: bool,
    },
    /// An `attachment` block names a `kind` the card's `subtypes` do not bear — an Aura
    /// grant on something that is not an Aura (CR 303.4), or an Equipment grant on
    /// something that is not an Equipment (CR 301.5).
    ///
    /// The subtype is what makes a card one of these things; the block only says what it
    /// does while attached. A card carrying one without the other would be granting from
    /// a type line that never claimed it could.
    AttachmentSubtypeMismatch {
        /// The definition at fault.
        functional_id: String,
        /// The subtype the authored `kind` requires and the card does not have.
        subtype: &'static str,
    },
    /// An `attachment` block's `equip` cost disagrees with its `kind`: an Equipment with
    /// no equip cost, or an Aura with one (CR 702.6a).
    ///
    /// Wrong in both directions, like [`Self::PowerToughnessMismatch`]. An Equipment with
    /// no equip cost could never be attached to anything and would sit on the battlefield
    /// doing nothing for ever; an Aura with one would advertise an activated ability the
    /// rules do not give it.
    EquipCostMismatch {
        /// The definition at fault.
        functional_id: String,
        /// Whether the card is an Equipment — which is to say, which way it is wrong.
        equipment: bool,
    },
    /// An ability watches "a spell of the **chosen color**" on a card that never names
    /// one — it declares no `enters_choosing_color` (CR 614.12).
    ///
    /// The phrase has no referent, so the trigger could not fire once in the whole game.
    /// It is caught here rather than left to the engine because the engine's honest
    /// answer — a permanent with no recorded colour notices nothing — is silence, and a
    /// card that silently does nothing is the hardest kind of wrong to notice.
    ChosenColorIsNeverNamed {
        /// The definition at fault.
        functional_id: String,
    },
    /// A static ability selects permanents "with the **chosen name**" on a card that
    /// never names one — it declares no `enters_naming_card` (CR 614.12).
    ///
    /// [`Self::ChosenColorIsNeverNamed`]'s counterpart, caught for the same reason: the
    /// phrase has no referent, so the selector could not match one permanent in the whole
    /// game, and the engine's honest answer — a source with no recorded name affects
    /// nothing — is silence.
    ChosenNameIsNeverNamed {
        /// The definition at fault.
        functional_id: String,
    },
    /// An `additional_cost` appears on a card that cannot be cast, or names a cost of
    /// nothing. A land is *played*, not cast (CR 116.2a), so a cast cost on one could
    /// never be paid or checked; a cost of zero cards is not a cost, and authoring one
    /// is a way of writing a card that reads as costing something and does not.
    AdditionalCostIsUnpayable {
        /// The definition at fault.
        functional_id: String,
    },
    /// A `modes` list that is not a choice a player could be asked to make (CR 700.2):
    /// fewer than two modes, more than [`MAX_MODES`], a mode that does nothing, or a
    /// modal card that also carries loose `spell_effects`.
    ///
    /// The upper bound is the one rule here that is not a rules rule. A mode is a
    /// numbered row in a dock band of fixed height (`docs/client-design.md` §6.7), and
    /// the alternative to refusing a fourth is truncating a sentence the player has to
    /// read *before* choosing it — so the limit belongs in the schema, where it fails
    /// the person authoring the card, rather than in a renderer, where it fails the
    /// person playing it. Three is where every *choose one* card in the game already
    /// sits; four is the Commands, which choose **two** and are a different question.
    MalformedModes {
        /// The definition at fault.
        functional_id: String,
        /// How many modes it declared.
        modes: usize,
    },
    /// An `{X}` appears in an **activation** cost (CR 107.3). X is announced as part of
    /// casting a spell (CR 601.2b); an activation pays out of a pool and has no
    /// announcement step to fix a value in, so the symbol would simply be ignored and
    /// the ability activated for nothing.
    XOutsideAManaCost {
        /// The definition at fault.
        functional_id: String,
        /// The cost string that contains it.
        cost: String,
    },
    /// A `spell_traits` entry names an `if_x_at_least` threshold on a card whose mana
    /// cost prints no `{X}` — a sentence about a value the card never asks for, and
    /// therefore a clause that could never be true.
    SpellTraitNeedsX {
        /// The definition at fault.
        functional_id: String,
    },
    /// An effect reads a **sacrifice** back that nothing on the card ever performs:
    /// `sacrificed_creature_power` with no cost that sacrifices, or `sacrificed_this_way`
    /// with no sacrifice effect.
    ///
    /// Caught here for the reason [`Self::ChosenColorIsNeverNamed`] is: the engine's
    /// honest answer to "how many were sacrificed" when none were is zero, and a card that
    /// reads as throwing a creature and always deals zero damage is the hardest kind of
    /// wrong to notice.
    ///
    /// The two sources are checked against **different** producers, because they read
    /// different moments — a cost paid at announcement (CR 601.2h) and a sacrifice this
    /// resolution performed (CR 701.17).
    AmountIsNeverSacrificed {
        /// The definition at fault.
        functional_id: String,
    },
    /// A **back face** carries a mana cost (CR 712.4a).
    ///
    /// The back face of a transforming double-faced card has no mana cost and can never
    /// be cast: it is only ever reached by turning a permanent over, and nothing in the
    /// game offers it as a spell. A cost written there would be a number nobody could
    /// pay and no gate would read — so it is refused at authoring time, naming the card,
    /// rather than sitting in the catalog looking castable.
    ///
    /// It is a validation rule rather than an absent field because the rule is worth
    /// *stating*: the field exists on [`BackFace`](crate::BackFace) and is always empty,
    /// which is what makes "a transformed permanent has mana value 0" a fact the read
    /// path can simply read.
    BackFaceHasManaCost {
        /// The definition at fault.
        functional_id: String,
    },
    /// A definition turns itself over — `transform_self` or
    /// `exile_self_and_return_transformed` — without having a second face to turn to
    /// (CR 701.28d).
    ///
    /// Caught here for [`Self::ChosenColorIsNeverNamed`]'s reason: the engine's honest
    /// answer is to leave the permanent exactly as it was, which is silence, and a card
    /// that silently does nothing is the hardest kind of wrong to notice.
    TransformWithoutABackFace {
        /// The definition at fault.
        functional_id: String,
    },
    /// A printed `restrictions` list appears on a card that is not a creature. Every
    /// combat restriction is about attacking or blocking (CR 506.3, CR 509.1b), so on
    /// a non-creature it could only ever be inert — which makes it an authoring
    /// mistake worth failing on rather than a harmless field.
    RestrictionsOnNonCreature {
        /// The definition at fault.
        functional_id: String,
    },
    /// An `alter_abilities_self` effect neither loses anything nor gains anything —
    /// `lose_all` is absent or false and both keyword lists are empty.
    ///
    /// A CR 613 layer-6 effect that changes no ability is not a small effect, it is
    /// no effect: it would mint a timestamp, sit in the stored effects until cleanup,
    /// and be describable only as "unchanged until end of turn". Every field defaults,
    /// which is exactly why authoring all of them away has to be caught here.
    AbilityChangeIsEmpty {
        /// The definition at fault.
        functional_id: String,
    },
    /// A `create_token` effect describes a token that could not be a permanent: it
    /// names no types at all, or names one no permanent has (an instant or a sorcery).
    ///
    /// The battlefield is the only zone a token may be in (CR 111.7), so a token that
    /// is not a permanent could never exist — the card would author an object with
    /// nowhere to go, which is worth failing the build over rather than creating.
    TokenIsNotAPermanent {
        /// The definition at fault.
        functional_id: String,
    },
    /// A `create_token` effect describes a creature token with no power/toughness, or
    /// a noncreature token that carries them — the token counterpart of
    /// [`Self::PowerToughnessMismatch`], and wrong for the same reason.
    TokenPowerToughnessMismatch {
        /// The definition at fault.
        functional_id: String,
        /// Whether the token is a creature — which is to say, which way it is wrong.
        creature: bool,
    },
    /// An optional effect (`{"kind":"may"}`) wraps **two** effects that choose a
    /// target.
    ///
    /// A `may` declares the target group of the one effect it wraps, so the slot is
    /// named at announcement (CR 601.2c) and the yes-or-no comes on resolution. One
    /// effect still declares at most one group, so a wrapper over two of them would
    /// have to advertise two slots from one forwarding and could never pair the flat
    /// stored target list back onto them. That is a card that looks authored and is
    /// not, which is worth failing the build over.
    TwoTargetsInsideOptional {
        /// The definition at fault.
        functional_id: String,
    },
    /// A conditional effect (`{"kind":"conditional"}`) has a branch that chooses a
    /// target.
    ///
    /// The neighbouring rule to [`Self::TwoTargetsInsideOptional`], and the reason a
    /// conditional forwards nothing where an optional effect forwards one group: its
    /// two branches share one flat target list, so a group named in either could not be
    /// paired back onto the branch that was actually taken. A branch that targeted
    /// would have its slot filled by nobody at announcement (CR 601.2c) and silently do
    /// nothing.
    TargetInsideConditional {
        /// The definition at fault.
        functional_id: String,
    },
    /// A **static** ability's `as long as …` condition counts permanents by
    /// `min_power`.
    ///
    /// A power bound is the one selector field read through the computed
    /// characteristics rather than the printed face, because power is what the
    /// implemented layers actually change. That reading is correct everywhere a
    /// condition is evaluated during a resolution, and non-terminating in the one place
    /// it is evaluated *inside* the layer system: computing a permanent's
    /// characteristics would ask for another permanent's characteristics, which would
    /// ask again. Rejected at build time rather than left as a stack overflow nobody
    /// can read.
    PowerInStaticCondition {
        /// The definition at fault.
        functional_id: String,
    },
    /// An `attachment` block's counted grant counts permanents by `min_power`.
    ///
    /// The second site with [`Self::PowerInStaticCondition`]'s problem and the same
    /// answer. An attachment's grant is a static ability (CR 604.3) evaluated from inside
    /// the computation of its host's characteristics, so a count that read a *computed*
    /// power there would ask the layer system for the answer it is in the middle of
    /// producing — and two mutually enchanted creatures would ask each other forever.
    /// Refused at build time rather than left as a stack overflow.
    ///
    /// A separate variant rather than a shared one because a violation names one place: a
    /// card author who wrote `min_power` on an Aura is told about the Aura, not about a
    /// static ability the card has not got.
    PowerInAttachmentCount {
        /// The definition at fault.
        functional_id: String,
    },
    /// A `create_emblem` effect gives the emblem an ability an emblem cannot have
    /// (CR 114.1–114.4) — anything but a static or triggered ability.
    ///
    /// An emblem is in no zone and is never an object a player acts on, so an activated
    /// ability would have no way to be activated and an enters-the-battlefield
    /// self-replacement would have no entry to replace. Either is an emblem with a dead
    /// ability, which is worth failing the build over.
    EmblemAbilityIsNotStaticOrTriggered {
        /// The definition at fault.
        functional_id: String,
    },
    /// One ability or spell declares **two** variable-arity target groups (two effects
    /// each taking "up to N" targets).
    ///
    /// Targets are stored as one flat list per stack object, and the pairing back onto
    /// effects works by giving every fixed group its size and the slack to the one
    /// variable group ([`target_counts`](crate::target_counts)). With two variable
    /// groups the split is ambiguous — six targets could be four and two or two and four
    /// — and no announcement could say which. One card wants this shape and none wants
    /// two, so the ambiguity is rejected rather than guessed at.
    TwoVariableTargetGroups {
        /// The definition at fault.
        functional_id: String,
    },
    /// A `return_self_from_graveyard` effect is authored where the ability carrying it
    /// could never function (CR 113.6): anywhere but an activated or triggered ability,
    /// or on an activated one whose cost a card in a graveyard could not pay.
    ///
    /// The effect is what *makes* an ability function from a graveyard
    /// ([`is_graveyard_ability`](crate::is_graveyard_ability)), so the derivation is only
    /// honest where the ability can actually be reached from there. On a spell's own
    /// effects or on an ability handed to an emblem there is no card in a graveyard at
    /// all; beside a `{T}` there is no permanent to tap. Either way the card reads as
    /// recursive and never is, which is worth failing the build over rather than
    /// shipping.
    GraveyardAbilityCannotFunction {
        /// The definition at fault.
        functional_id: String,
    },
    /// A **granted** ability is one of the kinds only an *entering* object's own printed
    /// abilities are ever read for (CR 614.12 / CR 616.1), so the grant is silently dead.
    ///
    /// `enters_tapped`, `enters_with_counters`, `enters_choosing_color`,
    /// `enters_naming_card`, and `enters_as_copy` are all consulted at one seam — the
    /// moment a permanent arrives — off the *card*'s abilities
    /// ([`abilities_of`](crate::abilities_of)), before any permanent exists to have been
    /// granted anything. The CR 613 layer-6 fold will happily carry one: `fold_abilities`
    /// boxes whatever it is handed and pushes it, and every later read simply never asks.
    ///
    /// Caught here for the reason [`Self::ChosenColorIsNeverNamed`] is: the engine's
    /// honest answer is silence, and an authored ability that does nothing at all is the
    /// failure mode no test catches — nobody writes a test for a card they believe works
    /// (issue #776).
    GrantedAbilityIsNeverRead {
        /// The definition at fault.
        functional_id: String,
        /// The ability `type` it granted.
        ability: String,
    },
    /// A `discard` cost names **zero** cards — an activation cost component, or the cost
    /// of accepting a `you may …` (CR 601.2b / CR 701.8).
    ///
    /// The `AdditionalCost::Discard` half of this rule has been checked since it was
    /// written ([`Self::AdditionalCostIsUnpayable`]); these two carried the same claim in
    /// a doc comment — *at least one on every printed card* — with nothing enforcing it.
    /// A cost of zero cards is not a cost: the ability reads as charging a card and is
    /// free, which is a rules difference rather than a cosmetic one (issue #776).
    DiscardCostIsFree {
        /// The definition at fault.
        functional_id: String,
    },
    /// Two printings in one set claim the same collector number, so one would shadow
    /// the other.
    DuplicatePrinting {
        /// The set the collision is in.
        set_code: String,
        /// The collector number claimed twice.
        collector_number: String,
    },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAnObject => write!(
                f,
                "a functional definition must be a single JSON object, one card per file"
            ),
            Self::MalformedField {
                functional_id,
                field,
            } => write!(f, "{functional_id}: `{field}` is missing or the wrong type"),
            Self::UnsupportedSchemaVersion {
                functional_id,
                found,
            } => write!(
                f,
                "{functional_id} declares schema_version {found}; \
                 this engine understands {SCHEMA_VERSION}"
            ),
            Self::MalformedFunctionalId { slug } => write!(
                f,
                "`{slug}` is not a well-formed functional id: expected a lowercase \
                 snake_case slug (e.g. `onakke_ogre`)"
            ),
            Self::FileNameMismatch {
                functional_id,
                file_stem,
            } => write!(
                f,
                "{file_stem}.json declares functional_id `{functional_id}`; \
                 a definition's file name must match its identity"
            ),
            Self::TransformWithoutABackFace { functional_id } => write!(
                f,
                "{functional_id} transforms itself but has no `back_face` to turn to \
                 (CR 701.28d)"
            ),
            Self::BackFaceHasManaCost { functional_id } => write!(
                f,
                "{functional_id}: a back face has no mana cost and can never be cast \
                 (CR 712.4a)"
            ),
            Self::PowerToughnessMismatch {
                functional_id,
                creature: true,
            } => write!(f, "{functional_id} is a Creature with no power/toughness"),
            Self::PowerToughnessMismatch {
                functional_id,
                creature: false,
            } => write!(
                f,
                "{functional_id} is not a Creature but carries power/toughness"
            ),
            Self::LoyaltyMismatch {
                functional_id,
                planeswalker: true,
            } => write!(f, "{functional_id} is a Planeswalker with no loyalty"),
            Self::LoyaltyMismatch {
                functional_id,
                planeswalker: false,
            } => write!(
                f,
                "{functional_id} is not a Planeswalker but carries loyalty"
            ),
            Self::AttachmentSubtypeMismatch {
                functional_id,
                subtype,
            } => write!(
                f,
                "{functional_id} carries an `attachment` grant of that kind but its \
                 subtypes do not include `{subtype}`"
            ),
            Self::EquipCostMismatch {
                functional_id,
                equipment: true,
            } => write!(f, "{functional_id} is an Equipment with no `equip` cost"),
            Self::ChosenColorIsNeverNamed { functional_id } => write!(
                f,
                "{functional_id} watches a spell of the chosen color but never chooses \
                 one: it declares no `enters_choosing_color` ability"
            ),
            Self::ChosenNameIsNeverNamed { functional_id } => write!(
                f,
                "{functional_id} selects permanents with the chosen name but never names \
                 a card: it declares no `enters_naming_card` ability"
            ),
            Self::EquipCostMismatch {
                functional_id,
                equipment: false,
            } => write!(
                f,
                "{functional_id} is not an Equipment but carries an `equip` cost"
            ),
            Self::AdditionalCostIsUnpayable { functional_id } => write!(
                f,
                "{functional_id} carries an `additional_cost` that could never be paid: \
                 a land is played rather than cast, and a cost of no cards is no cost"
            ),
            Self::MalformedModes {
                functional_id,
                modes,
            } => write!(
                f,
                "{functional_id} declares {modes} mode(s): a modal card prints between \
                 {MIN_MODES} and {MAX_MODES} of them, each doing something, and carries \
                 no `spell_effects` of its own"
            ),
            Self::XOutsideAManaCost {
                functional_id,
                cost,
            } => write!(
                f,
                "{functional_id} writes `{{X}}` in the activation cost `{cost}`; X is \
                 announced only as a spell is cast, so nothing would ever charge for it"
            ),
            Self::SpellTraitNeedsX { functional_id } => write!(
                f,
                "{functional_id} declares a spell trait conditional on X, but its mana \
                 cost prints no `{{X}}` to announce"
            ),
            Self::AmountIsNeverSacrificed { functional_id } => write!(
                f,
                "{functional_id} reads a sacrifice back but nothing on it sacrifices \
                 anything; the amount could only ever be zero"
            ),
            Self::RestrictionsOnNonCreature { functional_id } => write!(
                f,
                "{functional_id} carries printed `restrictions` but is not a creature; \
                 a combat restriction can only restrict attacking or blocking"
            ),
            Self::AbilityChangeIsEmpty { functional_id } => write!(
                f,
                "{functional_id} authors an `alter_abilities_self` that loses nothing \
                 and gains nothing; a layer-6 effect that changes no ability is no effect"
            ),
            Self::TokenIsNotAPermanent { functional_id } => write!(
                f,
                "{functional_id} creates a token that is not a permanent; \
                 a token exists only on the battlefield, so its `types` must be \
                 permanent types"
            ),
            Self::TokenPowerToughnessMismatch {
                functional_id,
                creature: true,
            } => write!(
                f,
                "{functional_id} creates a Creature token with no power/toughness"
            ),
            Self::TokenPowerToughnessMismatch {
                functional_id,
                creature: false,
            } => write!(
                f,
                "{functional_id} creates a token that is not a Creature \
                 but carries power/toughness"
            ),
            Self::TwoTargetsInsideOptional { functional_id } => write!(
                f,
                "{functional_id} has a `may` effect wrapping two effects that target; \
                 an optional effect declares the target group of one wrapped effect"
            ),
            Self::TargetInsideConditional { functional_id } => write!(
                f,
                "{functional_id}: a conditional effect's branch may not choose a target"
            ),
            Self::PowerInStaticCondition { functional_id } => write!(
                f,
                "{functional_id}: a static ability's condition may not count by \
                 `min_power`; a computed power read from inside the layer system \
                 would not terminate"
            ),
            Self::PowerInAttachmentCount { functional_id } => write!(
                f,
                "{functional_id}: an attachment's counted grant may not count by \
                 `min_power`; a computed power read from inside the layer system \
                 would not terminate"
            ),
            Self::EmblemAbilityIsNotStaticOrTriggered { functional_id } => write!(
                f,
                "{functional_id}: an emblem may carry only static and triggered \
                 abilities (CR 114.1)"
            ),
            Self::TwoVariableTargetGroups { functional_id } => write!(
                f,
                "{functional_id}: one ability or spell may declare at most one \
                 variable-arity (`up_to`) target group"
            ),
            Self::GraveyardAbilityCannotFunction { functional_id } => write!(
                f,
                "{functional_id}: `return_self_from_graveyard` must sit inside an \
                 activated or triggered ability, and an activated one may charge only \
                 mana; a card in a graveyard is not a permanent and has nothing else to \
                 pay with (CR 113.6)"
            ),
            Self::DuplicatePrinting {
                set_code,
                collector_number,
            } => write!(
                f,
                "two printings in {set_code} claim collector number {collector_number}"
            ),
            Self::GrantedAbilityIsNeverRead {
                functional_id,
                ability,
            } => write!(
                f,
                "{functional_id} grants `{ability}`, which is read only off an entering \
                 object's own printed abilities (CR 614.12) — a granted one is never \
                 asked for and does nothing"
            ),
            Self::DiscardCostIsFree { functional_id } => write!(
                f,
                "{functional_id} names a discard cost of zero cards: a cost of nothing is \
                 not a cost (CR 601.2b)"
            ),
        }
    }
}
